//! Channel-parallel compact-L2 physical-v2 reader helpers.
//!
//! These helpers keep the OCB file reader generic but provide a small
//! compact-L2 scheduling layer for channel-sharded physical-v2 artifacts.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::Instant;

use crate::column_bundle::{
    ColumnBatch, ColumnBundleFile, ColumnBundleReadAttribution, ColumnBundleReadCursorOptions,
    ColumnBundleReadOptions, ColumnBundleReadRequest, ColumnBundleVisitControl, ColumnProjection,
};
use crate::compact_l2::{
    COMPACT_L2_BIZ_INDEX_COLUMN_NAME, COMPACT_L2_CHANNEL_ID_COLUMN_NAME,
    COMPACT_L2_DAY_KEY_COLUMN_NAME, COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
    COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME, COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS,
    COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME, COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
    COMPACT_L2_RECORD_KIND_COLUMN_NAME, COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME,
    CompactL2PhysicalV2BatchView,
};
use crate::manifest::{
    ChannelShardedManifestV1, ManifestArtifactResolver, resolve_manifest_relative_artifact_path,
};
use crate::parallel_prepare::{
    ColumnBundleParallelPrepareContext, ColumnBundleParallelPrepareOptions,
    ColumnBundleParallelPrepareReport,
};
use crate::read::OcbReadSource;
use crate::{ArcadiaTioError, OcbErrorKind, OcbResourceLimits, Result};

/// Default channel worker count for compact-L2 physical-v2 reads.
pub const COMPACT_L2_PHYSICAL_V2_DEFAULT_CHANNEL_WORKERS: usize = 8;

/// One compact-L2 physical-v2 channel artifact selected for reading.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ChannelReadInput {
    /// Source ChannelID.
    pub channel_id: u32,
    /// Path to the channel-local OCB artifact.
    pub path: PathBuf,
    /// Optional explicit file-local row-group ids. `None` selects all planned
    /// row groups.
    pub row_group_ids: Option<Vec<u32>>,
    /// Optional expected row count for fail-closed full-channel reads.
    pub expected_rows: Option<u64>,
}

impl CompactL2PhysicalV2ChannelReadInput {
    /// Create a full-channel read input with no expected row-count check.
    pub fn new(channel_id: u32, path: impl Into<PathBuf>) -> Self {
        Self {
            channel_id,
            path: path.into(),
            row_group_ids: None,
            expected_rows: None,
        }
    }

    /// Set an optional expected row count.
    pub fn with_expected_rows(mut self, expected_rows: u64) -> Self {
        self.expected_rows = Some(expected_rows);
        self
    }

    /// Set explicit file-local row group ids.
    pub fn with_row_group_ids(mut self, row_group_ids: Vec<u32>) -> Self {
        self.row_group_ids = Some(row_group_ids);
        self
    }
}

/// One manifest-derived compact-L2 channel whose artifact identity is already
/// bound to a checked file handle.
///
/// Instances are created by
/// [`compact_l2_physical_v2_bound_inputs_from_manifest`]. The diagnostic path
/// is intentionally private and is never used for later I/O.
#[derive(Debug, Clone)]
pub struct CompactL2PhysicalV2ManifestReadInput {
    channel_id: u32,
    source: Arc<OcbReadSource>,
    row_group_ids: Option<Vec<u32>>,
    expected_rows: Option<u64>,
}

impl CompactL2PhysicalV2ManifestReadInput {
    /// Return the source ChannelID declared by the manifest.
    pub const fn channel_id(&self) -> u32 {
        self.channel_id
    }

    /// Return the expected full-channel row count declared by the manifest.
    pub const fn expected_rows(&self) -> Option<u64> {
        self.expected_rows
    }

    /// Return explicit file-local row-group ids, if selected.
    pub fn row_group_ids(&self) -> Option<&[u32]> {
        self.row_group_ids.as_deref()
    }

    /// Select explicit file-local row-group ids while retaining the same bound
    /// artifact identity.
    pub fn with_row_group_ids(mut self, row_group_ids: Vec<u32>) -> Self {
        self.row_group_ids = Some(row_group_ids);
        self
    }
}

/// Read options for channel-parallel compact-L2 physical-v2 scans.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ParallelReadOptions {
    /// Number of channel files to read concurrently.
    pub channel_workers: usize,
    /// OCB row-group worker cap used inside each channel file.
    pub per_file_read_threads: usize,
    /// Maximum row groups in flight inside one channel file.
    pub max_in_flight_row_groups: usize,
    /// Whether chunk checksums are validated during reads.
    pub validate_checksums: bool,
}

impl Default for CompactL2PhysicalV2ParallelReadOptions {
    fn default() -> Self {
        Self {
            channel_workers: COMPACT_L2_PHYSICAL_V2_DEFAULT_CHANNEL_WORKERS,
            per_file_read_threads: 1,
            max_in_flight_row_groups: 1,
            validate_checksums: true,
        }
    }
}

impl CompactL2PhysicalV2ParallelReadOptions {
    /// Conservative low-contention read options from the private full-day
    /// parallelism experiment.
    pub fn channel_parallel_default() -> Self {
        Self::default()
    }

    /// High-CPU variant for hosts where nested per-file row-group parallelism is
    /// acceptable.
    pub fn high_cpu(channel_workers: usize) -> Self {
        Self {
            channel_workers,
            per_file_read_threads: 2,
            max_in_flight_row_groups: 2,
            validate_checksums: true,
        }
    }

    fn validate(self) -> Result<()> {
        if self.channel_workers == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 channel_workers must be greater than zero",
            ));
        }
        if self.per_file_read_threads == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 per_file_read_threads must be greater than zero",
            ));
        }
        if self.max_in_flight_row_groups == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 max_in_flight_row_groups must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Owned compact-L2 physical-v2 batch yielded to a read visitor.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactL2PhysicalV2ReadBatch {
    /// Source ChannelID.
    pub channel_id: u32,
    /// Decoded OCB row-group batch. The batch has already been validated as the
    /// required physical-v2 projection.
    pub batch: ColumnBatch,
}

/// Per-channel read report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ChannelReadReport {
    /// Source ChannelID.
    pub channel_id: u32,
    /// Number of row-group batches yielded to the visitor.
    pub batches_yielded: u64,
    /// Number of rows yielded to the visitor.
    pub rows_yielded: u64,
    /// Maximum in-flight row groups observed inside this channel file.
    pub max_in_flight_row_groups_observed: usize,
    /// Whether the read stopped before all planned batches were yielded.
    pub cancelled: bool,
    /// Channel-local OCB attribution.
    pub attribution: ColumnBundleReadAttribution,
}

/// Aggregate channel-parallel read report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ParallelReadReport {
    /// Requested channel workers from the options.
    pub requested_channel_workers: usize,
    /// Effective worker count used for this input set.
    pub effective_channel_workers: usize,
    /// Per-file OCB row-group thread cap.
    pub per_file_read_threads: usize,
    /// Per-file row-group in-flight cap.
    pub max_in_flight_row_groups: usize,
    /// Number of channel inputs.
    pub channel_count: usize,
    /// Number of row-group batches yielded to the visitor.
    pub batches_yielded: u64,
    /// Number of rows yielded to the visitor.
    pub rows_yielded: u64,
    /// Whether any channel read stopped early.
    pub cancelled: bool,
    /// Aggregate OCB attribution summed over channel reports.
    pub attribution: ColumnBundleReadAttribution,
    /// Per-channel reports sorted by ChannelID.
    pub channel_reports: Vec<CompactL2PhysicalV2ChannelReadReport>,
}

/// Options for one-channel compact-L2 physical-v2 parallel preparation.
///
/// This is a single fixed row-group worker budget. It is not layered under the
/// channel-parallel reader and therefore cannot create a nested worker pool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ParallelPrepareOptions {
    /// Fixed row-group worker budget.
    pub workers: usize,
    /// Global cap across queued, active, completed, and pending row groups.
    pub max_in_flight_row_groups: usize,
    /// Whether chunk checksums are validated during reads.
    pub validate_checksums: bool,
}

impl Default for CompactL2PhysicalV2ParallelPrepareOptions {
    fn default() -> Self {
        Self {
            workers: COMPACT_L2_PHYSICAL_V2_DEFAULT_CHANNEL_WORKERS,
            max_in_flight_row_groups: COMPACT_L2_PHYSICAL_V2_DEFAULT_CHANNEL_WORKERS,
            validate_checksums: true,
        }
    }
}

impl CompactL2PhysicalV2ParallelPrepareOptions {
    fn validate(self) -> Result<()> {
        if self.workers == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 parallel-prepare workers must be greater than zero",
            ));
        }
        if self.max_in_flight_row_groups == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 parallel-prepare max_in_flight_row_groups must be greater than zero",
            ));
        }
        Ok(())
    }
}

/// Compact-L2 context supplied to worker preparation and ordered commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ParallelPrepareContext {
    /// Source ChannelID for this one-channel invocation.
    pub channel_id: u32,
    /// Generic deterministic row-group context, including stable worker id.
    pub row_group: ColumnBundleParallelPrepareContext,
}

/// Report for one-channel compact-L2 physical-v2 parallel preparation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ParallelPrepareReport {
    /// Source ChannelID.
    pub channel_id: u32,
    /// Generic bounded preparation and ordered-commit report.
    pub parallel_prepare: ColumnBundleParallelPrepareReport,
}

/// Build channel read inputs from a physical-v2 channel-sharded manifest.
pub fn compact_l2_physical_v2_inputs_from_manifest(
    manifest_path: impl AsRef<Path>,
    manifest: &ChannelShardedManifestV1,
) -> Result<Vec<CompactL2PhysicalV2ChannelReadInput>> {
    if manifest.artifact_format != COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::InvalidManifest,
            "compact-L2 physical-v2 reader requires a physical-v2 manifest",
        ));
    }
    manifest
        .channels()
        .iter()
        .map(|channel| {
            Ok(CompactL2PhysicalV2ChannelReadInput {
                channel_id: channel.channel_id,
                path: resolve_manifest_relative_artifact_path(
                    manifest_path.as_ref(),
                    &channel.relative_path,
                )?,
                row_group_ids: None,
                expected_rows: Some(channel.row_count),
            })
        })
        .collect()
}

/// Bind physical-v2 channel artifacts from a channel-sharded manifest to
/// checked file handles.
///
/// The returned inputs retain each accepted file identity. Later path changes
/// cannot redirect bound prepare or read operations. Use this helper instead of
/// [`compact_l2_physical_v2_inputs_from_manifest`] when the manifest is a trust
/// boundary; the older helper remains path-based for 0.3.x source compatibility.
pub fn compact_l2_physical_v2_bound_inputs_from_manifest(
    manifest_path: impl AsRef<Path>,
    manifest: &ChannelShardedManifestV1,
) -> Result<Vec<CompactL2PhysicalV2ManifestReadInput>> {
    if manifest.artifact_format != COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::InvalidManifest,
            "compact-L2 physical-v2 reader requires a physical-v2 manifest",
        ));
    }
    manifest.validate()?;
    let resolver = ManifestArtifactResolver::new(manifest_path.as_ref())?;
    let mut inputs = Vec::new();
    inputs
        .try_reserve_exact(manifest.channels().len())
        .map_err(|_| parallel_read_allocation_error())?;
    for channel in manifest.channels() {
        inputs.push(CompactL2PhysicalV2ManifestReadInput {
            channel_id: channel.channel_id,
            source: Arc::new(resolver.open_artifact(&channel.relative_path)?),
            row_group_ids: None,
            expected_rows: Some(channel.row_count),
        });
    }
    Ok(inputs)
}

/// Read and prepare one compact-L2 physical-v2 channel on a fixed worker set,
/// then release owned results in selected-plan row-group order.
///
/// Physical-v2 column shape and the constant input ChannelID are validated on
/// each worker before caller preparation. The borrowed batch view cannot leave
/// the worker callback; caller preparation must return owned `Send + 'static`
/// output. Whole-channel ownership remains a downstream responsibility. A
/// fail-closed consumer must stage ordered callback output locally and publish
/// replay-visible state only after a terminally completed `Ok` report; it must
/// discard the stage on failure or `Stop` because callback side effects cannot
/// be rolled back by TIO. This compatibility entry point uses
/// [`OcbResourceLimits::policy_a`].
pub fn parallel_prepare_compact_l2_physical_v2_channel<T, Prepare, Commit>(
    input: CompactL2PhysicalV2ChannelReadInput,
    options: CompactL2PhysicalV2ParallelPrepareOptions,
    prepare: Prepare,
    ordered_commit: Commit,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport>
where
    T: Send + 'static,
    Prepare: for<'a> Fn(
            CompactL2PhysicalV2ParallelPrepareContext,
            CompactL2PhysicalV2BatchView<'a>,
        ) -> Result<T>
        + Sync,
    Commit: FnMut(CompactL2PhysicalV2ParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    parallel_prepare_compact_l2_physical_v2_channel_with_resource_limits(
        input,
        options,
        OcbResourceLimits::policy_a(),
        prepare,
        ordered_commit,
    )
}

/// Read and prepare one compact-L2 physical-v2 channel using an explicit OCB
/// resource policy, then release owned results in selected-plan row-group
/// order.
///
/// This has the same sequencing and fail-closed publication contract as
/// [`parallel_prepare_compact_l2_physical_v2_channel`]. The supplied limits are
/// applied while opening metadata and throughout every planned row-group read.
pub fn parallel_prepare_compact_l2_physical_v2_channel_with_resource_limits<T, Prepare, Commit>(
    input: CompactL2PhysicalV2ChannelReadInput,
    options: CompactL2PhysicalV2ParallelPrepareOptions,
    resource_limits: OcbResourceLimits,
    prepare: Prepare,
    ordered_commit: Commit,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport>
where
    T: Send + 'static,
    Prepare: for<'a> Fn(
            CompactL2PhysicalV2ParallelPrepareContext,
            CompactL2PhysicalV2BatchView<'a>,
        ) -> Result<T>
        + Sync,
    Commit: FnMut(CompactL2PhysicalV2ParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    parallel_prepare_compact_l2_physical_v2_input_with_resource_limits(
        input,
        options,
        resource_limits,
        prepare,
        ordered_commit,
    )
}

/// Prepare one handle-bound manifest channel with Policy A resource limits.
pub fn parallel_prepare_compact_l2_physical_v2_bound_channel<T, Prepare, Commit>(
    input: CompactL2PhysicalV2ManifestReadInput,
    options: CompactL2PhysicalV2ParallelPrepareOptions,
    prepare: Prepare,
    ordered_commit: Commit,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport>
where
    T: Send + 'static,
    Prepare: for<'a> Fn(
            CompactL2PhysicalV2ParallelPrepareContext,
            CompactL2PhysicalV2BatchView<'a>,
        ) -> Result<T>
        + Sync,
    Commit: FnMut(CompactL2PhysicalV2ParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    parallel_prepare_compact_l2_physical_v2_bound_channel_with_resource_limits(
        input,
        options,
        OcbResourceLimits::policy_a(),
        prepare,
        ordered_commit,
    )
}

/// Prepare one handle-bound manifest channel with explicit finite resource
/// limits while retaining its checked artifact identity.
pub fn parallel_prepare_compact_l2_physical_v2_bound_channel_with_resource_limits<
    T,
    Prepare,
    Commit,
>(
    input: CompactL2PhysicalV2ManifestReadInput,
    options: CompactL2PhysicalV2ParallelPrepareOptions,
    resource_limits: OcbResourceLimits,
    prepare: Prepare,
    ordered_commit: Commit,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport>
where
    T: Send + 'static,
    Prepare: for<'a> Fn(
            CompactL2PhysicalV2ParallelPrepareContext,
            CompactL2PhysicalV2BatchView<'a>,
        ) -> Result<T>
        + Sync,
    Commit: FnMut(CompactL2PhysicalV2ParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    parallel_prepare_compact_l2_physical_v2_input_with_resource_limits(
        input,
        options,
        resource_limits,
        prepare,
        ordered_commit,
    )
}

fn parallel_prepare_compact_l2_physical_v2_input_with_resource_limits<I, T, Prepare, Commit>(
    input: I,
    options: CompactL2PhysicalV2ParallelPrepareOptions,
    resource_limits: OcbResourceLimits,
    prepare: Prepare,
    mut ordered_commit: Commit,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport>
where
    I: Into<NormalizedCompactL2PhysicalV2ChannelReadInput>,
    T: Send + 'static,
    Prepare: for<'a> Fn(
            CompactL2PhysicalV2ParallelPrepareContext,
            CompactL2PhysicalV2BatchView<'a>,
        ) -> Result<T>
        + Sync,
    Commit: FnMut(CompactL2PhysicalV2ParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    options.validate()?;
    let mut inputs = normalize_inputs([input])?;
    let input = inputs.pop().expect("one normalized channel input");
    let expected_channel_id = i32::try_from(input.channel_id).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input(
            "compact-L2 physical-v2 parallel-prepare ChannelID does not fit i32",
        )
    })?;
    let file = input.open_file(resource_limits)?;
    let full_channel_selection = input.row_group_ids.is_none();
    if let Some(expected_rows) = input
        .expected_rows
        .filter(|expected_rows| full_channel_selection && file.row_count() != *expected_rows)
    {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "compact-L2 physical-v2 parallel-prepare metadata row count mismatch: expected={expected_rows} observed={}",
                file.row_count()
            ),
        ));
    }
    let request = ColumnBundleReadRequest {
        projection: compact_l2_physical_v2_projection(),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: options.workers,
            validate_checksums: options.validate_checksums,
            decode_dictionaries: false,
        },
    };
    let plan_started = Instant::now();
    let plan = file.plan_read(&request)?;
    let plan_ns = duration_to_ns(plan_started.elapsed());
    let row_group_ids = input
        .row_group_ids
        .clone()
        .unwrap_or_else(|| plan.row_group_ids.clone());
    let channel_id = input.channel_id;
    let expected_channel_id = i64::from(expected_channel_id);
    let mut report = file.parallel_prepare_plan_row_groups(
        &plan,
        &row_group_ids,
        ColumnBundleParallelPrepareOptions {
            max_in_flight_row_groups: options.max_in_flight_row_groups,
        },
        |context, batch| {
            let view = CompactL2PhysicalV2BatchView::from_column_batch(batch)?;
            view.validate()?;
            if view
                .channel_id
                .iter()
                .any(|observed| *observed != expected_channel_id)
            {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChannelIdMismatch,
                    format!(
                        "compact-L2 physical-v2 worker ChannelID mismatch: expected={channel_id}"
                    ),
                ));
            }
            prepare(
                CompactL2PhysicalV2ParallelPrepareContext {
                    channel_id,
                    row_group: context,
                },
                view,
            )
        },
        |context, prepared| {
            ordered_commit(
                CompactL2PhysicalV2ParallelPrepareContext {
                    channel_id,
                    row_group: context,
                },
                prepared,
            )
        },
    )?;
    report.attribution.plan_ns = plan_ns;
    Ok(CompactL2PhysicalV2ParallelPrepareReport {
        channel_id,
        parallel_prepare: report,
    })
}

/// Read compact-L2 physical-v2 channel artifacts with bounded channel-level
/// parallelism.
///
/// The visitor may be invoked concurrently from multiple channel worker
/// threads. Reports are deterministic and sorted by ChannelID, but visitor
/// callback order is intentionally not a stable API contract. Return
/// [`ColumnBundleVisitControl::Stop`] to request best-effort early stop; workers
/// already reading a row-group wave may still yield in-flight batches before the
/// function returns. This compatibility entry point uses
/// [`OcbResourceLimits::policy_a`].
pub fn read_compact_l2_physical_v2_channels<F>(
    inputs: impl IntoIterator<Item = CompactL2PhysicalV2ChannelReadInput>,
    options: CompactL2PhysicalV2ParallelReadOptions,
    visitor: F,
) -> Result<CompactL2PhysicalV2ParallelReadReport>
where
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    read_compact_l2_physical_v2_channels_with_resource_limits(
        inputs,
        options,
        OcbResourceLimits::policy_a(),
        visitor,
    )
}

/// Read compact-L2 physical-v2 channel artifacts with bounded channel-level
/// parallelism and an explicit operation-wide OCB resource policy.
///
/// The supplied per-object, per-chunk, and projected-row-group limits apply to
/// every opened channel file. The two aggregate selected-compressed and
/// decoded-materialized budgets apply across simultaneously active channel
/// workers: each fixed worker slot receives the same conservative floor share,
/// and the per-file row-group preflight rejects a wave that exceeds that share
/// before payload reads begin. Any division remainder remains unassigned so
/// acceptance does not depend on which worker dequeues a channel. A visitor
/// that retains or copies callback-owned data is responsible for accounting for
/// that additional caller-owned lifetime.
pub fn read_compact_l2_physical_v2_channels_with_resource_limits<F>(
    inputs: impl IntoIterator<Item = CompactL2PhysicalV2ChannelReadInput>,
    options: CompactL2PhysicalV2ParallelReadOptions,
    resource_limits: OcbResourceLimits,
    visitor: F,
) -> Result<CompactL2PhysicalV2ParallelReadReport>
where
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    read_compact_l2_physical_v2_inputs_with_resource_limits(
        inputs,
        options,
        resource_limits,
        visitor,
    )
}

/// Read handle-bound physical-v2 manifest channels with Policy A resource
/// limits and bounded channel-level parallelism.
pub fn read_compact_l2_physical_v2_bound_channels<F>(
    inputs: impl IntoIterator<Item = CompactL2PhysicalV2ManifestReadInput>,
    options: CompactL2PhysicalV2ParallelReadOptions,
    visitor: F,
) -> Result<CompactL2PhysicalV2ParallelReadReport>
where
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    read_compact_l2_physical_v2_bound_channels_with_resource_limits(
        inputs,
        options,
        OcbResourceLimits::policy_a(),
        visitor,
    )
}

/// Read handle-bound physical-v2 manifest channels with an explicit
/// operation-wide OCB resource policy.
pub fn read_compact_l2_physical_v2_bound_channels_with_resource_limits<F>(
    inputs: impl IntoIterator<Item = CompactL2PhysicalV2ManifestReadInput>,
    options: CompactL2PhysicalV2ParallelReadOptions,
    resource_limits: OcbResourceLimits,
    visitor: F,
) -> Result<CompactL2PhysicalV2ParallelReadReport>
where
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    read_compact_l2_physical_v2_inputs_with_resource_limits(
        inputs,
        options,
        resource_limits,
        visitor,
    )
}

fn read_compact_l2_physical_v2_inputs_with_resource_limits<I, F>(
    inputs: impl IntoIterator<Item = I>,
    options: CompactL2PhysicalV2ParallelReadOptions,
    resource_limits: OcbResourceLimits,
    visitor: F,
) -> Result<CompactL2PhysicalV2ParallelReadReport>
where
    I: Into<NormalizedCompactL2PhysicalV2ChannelReadInput>,
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    options.validate()?;
    let inputs = normalize_inputs(inputs)?;
    let channel_count = inputs.len();
    let effective_channel_workers = options.channel_workers.min(channel_count).max(1);
    let queue = Arc::new(Mutex::new(VecDeque::from(inputs)));
    let visitor = Arc::new(visitor);
    let stop = Arc::new(AtomicBool::new(false));
    let (sender, receiver) = mpsc::channel::<Result<CompactL2PhysicalV2ChannelReadReport>>();
    let mut handles = Vec::new();
    handles
        .try_reserve_exact(effective_channel_workers)
        .map_err(|_| parallel_read_allocation_error())?;

    for worker_index in 0..effective_channel_workers {
        let worker_resource_limits = channel_worker_resource_limits(
            resource_limits,
            worker_index,
            effective_channel_workers,
        )?;
        let queue = Arc::clone(&queue);
        let visitor = Arc::clone(&visitor);
        let stop = Arc::clone(&stop);
        let sender = sender.clone();
        handles.push(thread::spawn(move || {
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }
                let input = {
                    let mut queue = queue.lock().expect("compact-L2 read queue poisoned");
                    queue.pop_front()
                };
                let Some(input) = input else {
                    break;
                };
                let result = read_physical_v2_channel(
                    input,
                    options,
                    worker_resource_limits,
                    Arc::clone(&visitor),
                    Arc::clone(&stop),
                );
                if result.is_err() {
                    stop.store(true, Ordering::Relaxed);
                }
                if sender.send(result).is_err() {
                    break;
                }
            }
        }));
    }
    drop(sender);

    let mut reports = Vec::new();
    reports
        .try_reserve_exact(channel_count)
        .map_err(|_| parallel_read_allocation_error())?;
    let mut first_error = None;
    for result in receiver {
        match result {
            Ok(report) => reports.push(report),
            Err(err) => {
                if first_error.is_none() {
                    first_error = Some(err);
                }
            }
        }
    }
    for handle in handles {
        if handle.join().is_err() && first_error.is_none() {
            first_error = Some(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 reader worker panicked",
            ));
        }
    }
    if let Some(err) = first_error {
        return Err(err);
    }

    reports.sort_by_key(|report| report.channel_id);
    let mut attribution = ColumnBundleReadAttribution::default();
    let mut batches_yielded = 0u64;
    let mut rows_yielded = 0u64;
    let mut cancelled = false;
    for report in &reports {
        batches_yielded = batches_yielded.saturating_add(report.batches_yielded);
        rows_yielded = rows_yielded.saturating_add(report.rows_yielded);
        cancelled |= report.cancelled;
        add_attribution(&mut attribution, &report.attribution);
    }

    Ok(CompactL2PhysicalV2ParallelReadReport {
        requested_channel_workers: options.channel_workers,
        effective_channel_workers,
        per_file_read_threads: options.per_file_read_threads,
        max_in_flight_row_groups: options.max_in_flight_row_groups,
        channel_count,
        batches_yielded,
        rows_yielded,
        cancelled,
        attribution,
        channel_reports: reports,
    })
}

fn channel_worker_resource_limits(
    resource_limits: OcbResourceLimits,
    worker_index: usize,
    worker_count: usize,
) -> Result<OcbResourceLimits> {
    let compressed_share = channel_worker_resource_share(
        resource_limits.max_owned_selected_compressed_bytes(),
        worker_index,
        worker_count,
    )?;
    let decoded_share = channel_worker_resource_share(
        resource_limits.max_owned_decoded_materialized_bytes(),
        worker_index,
        worker_count,
    )?;
    resource_limits
        .with_max_owned_selected_compressed_bytes(compressed_share)?
        .with_max_owned_decoded_materialized_bytes(decoded_share)
}

fn channel_worker_resource_share(
    total: u64,
    worker_index: usize,
    worker_count: usize,
) -> Result<u64> {
    if worker_count == 0 || worker_index >= worker_count {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "compact-L2 physical-v2 resource accounting has an invalid worker slot",
        ));
    }
    let worker_count = u64::try_from(worker_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input(
            "compact-L2 physical-v2 worker count does not fit resource accounting",
        )
    })?;
    Ok(total / worker_count)
}

#[derive(Debug, Clone)]
enum CompactL2PhysicalV2FileAuthority {
    Path(PathBuf),
    Bound(Arc<OcbReadSource>),
}

#[derive(Debug, Clone)]
struct NormalizedCompactL2PhysicalV2ChannelReadInput {
    channel_id: u32,
    authority: CompactL2PhysicalV2FileAuthority,
    row_group_ids: Option<Vec<u32>>,
    expected_rows: Option<u64>,
}

impl NormalizedCompactL2PhysicalV2ChannelReadInput {
    fn open_file(&self, resource_limits: OcbResourceLimits) -> Result<ColumnBundleFile> {
        match &self.authority {
            CompactL2PhysicalV2FileAuthority::Path(path) => {
                ColumnBundleFile::open_with_resource_limits(path, resource_limits)
            }
            CompactL2PhysicalV2FileAuthority::Bound(source) => {
                ColumnBundleFile::open_from_source_with_resource_limits(
                    Arc::clone(source),
                    resource_limits,
                )
            }
        }
    }
}

impl From<CompactL2PhysicalV2ChannelReadInput> for NormalizedCompactL2PhysicalV2ChannelReadInput {
    fn from(input: CompactL2PhysicalV2ChannelReadInput) -> Self {
        Self {
            channel_id: input.channel_id,
            authority: CompactL2PhysicalV2FileAuthority::Path(input.path),
            row_group_ids: input.row_group_ids,
            expected_rows: input.expected_rows,
        }
    }
}

impl From<CompactL2PhysicalV2ManifestReadInput> for NormalizedCompactL2PhysicalV2ChannelReadInput {
    fn from(input: CompactL2PhysicalV2ManifestReadInput) -> Self {
        Self {
            channel_id: input.channel_id,
            authority: CompactL2PhysicalV2FileAuthority::Bound(input.source),
            row_group_ids: input.row_group_ids,
            expected_rows: input.expected_rows,
        }
    }
}

fn normalize_inputs<I>(
    inputs: impl IntoIterator<Item = I>,
) -> Result<Vec<NormalizedCompactL2PhysicalV2ChannelReadInput>>
where
    I: Into<NormalizedCompactL2PhysicalV2ChannelReadInput>,
{
    let mut normalized = Vec::new();
    for input in inputs {
        let input = input.into();
        normalized
            .try_reserve(1)
            .map_err(|_| parallel_read_allocation_error())?;
        if input.channel_id == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 channel input has invalid ChannelID",
            ));
        }
        if let Some(row_group_ids) = &input.row_group_ids {
            if row_group_ids.is_empty() {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "compact-L2 physical-v2 channel row_group_ids must be non-empty",
                ));
            }
        }
        normalized.push(input);
    }
    if normalized.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "compact-L2 physical-v2 reader requires at least one channel input",
        ));
    }
    normalized.sort_by_key(|input| input.channel_id);
    if normalized
        .windows(2)
        .any(|inputs| inputs[0].channel_id == inputs[1].channel_id)
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "compact-L2 physical-v2 channel inputs contain duplicate ChannelID",
        ));
    }
    Ok(normalized)
}

fn parallel_read_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        std::io::ErrorKind::OutOfMemory,
        "compact-L2 physical-v2 reader allocation failed within resource limit",
    ))
}

fn read_physical_v2_channel<F>(
    input: NormalizedCompactL2PhysicalV2ChannelReadInput,
    options: CompactL2PhysicalV2ParallelReadOptions,
    resource_limits: OcbResourceLimits,
    visitor: Arc<F>,
    stop: Arc<AtomicBool>,
) -> Result<CompactL2PhysicalV2ChannelReadReport>
where
    F: Fn(CompactL2PhysicalV2ReadBatch) -> Result<ColumnBundleVisitControl> + Send + Sync + 'static,
{
    let file = input.open_file(resource_limits)?;
    let request = ColumnBundleReadRequest {
        projection: compact_l2_physical_v2_projection(),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: options.per_file_read_threads,
            validate_checksums: options.validate_checksums,
            decode_dictionaries: false,
        },
    };
    let plan_started = Instant::now();
    let plan = file.plan_read(&request)?;
    let plan_ns = duration_to_ns(plan_started.elapsed());
    let row_group_ids = input
        .row_group_ids
        .clone()
        .unwrap_or_else(|| plan.row_group_ids.clone());
    let cursor_options = ColumnBundleReadCursorOptions {
        max_in_flight_row_groups: options.max_in_flight_row_groups,
        ordered: true,
    };
    let channel_id = input.channel_id;
    let outcome = file.visit_plan_row_groups_with_attribution(
        &plan,
        &row_group_ids,
        cursor_options,
        move |batch| {
            if stop.load(Ordering::Relaxed) {
                return Ok(ColumnBundleVisitControl::Stop);
            }
            let view = CompactL2PhysicalV2BatchView::from_column_batch(&batch)?;
            view.validate()?;
            let control = visitor(CompactL2PhysicalV2ReadBatch { channel_id, batch })?;
            if matches!(control, ColumnBundleVisitControl::Stop) {
                stop.store(true, Ordering::Relaxed);
            }
            Ok(control)
        },
    )?;
    let mut attribution = outcome.attribution;
    attribution.plan_ns = plan_ns;
    if let Some(expected_rows) = input.expected_rows {
        if !outcome.cursor_report.cancelled && outcome.cursor_report.rows_yielded != expected_rows {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "compact-L2 physical-v2 channel row count does not match expected_rows",
            ));
        }
    }
    Ok(CompactL2PhysicalV2ChannelReadReport {
        channel_id: input.channel_id,
        batches_yielded: outcome.cursor_report.batches_yielded as u64,
        rows_yielded: outcome.cursor_report.rows_yielded,
        max_in_flight_row_groups_observed: outcome.cursor_report.max_in_flight_row_groups_observed,
        cancelled: outcome.cursor_report.cancelled,
        attribution,
    })
}

fn compact_l2_physical_v2_projection() -> ColumnProjection {
    let mut names = Vec::with_capacity(20);
    names.push(COMPACT_L2_DAY_KEY_COLUMN_NAME);
    names.push(COMPACT_L2_CHANNEL_ID_COLUMN_NAME);
    names.push(COMPACT_L2_BIZ_INDEX_COLUMN_NAME);
    names.push(COMPACT_L2_RECEIVE_NANO_COLUMN_NAME);
    names.push(COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME);
    names.push(COMPACT_L2_RECORD_KIND_COLUMN_NAME);
    names.push(COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME);
    names.push(COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME);
    names.push(COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME);
    names.push(COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME);
    for column in COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS {
        names.push(column.column_name);
    }
    ColumnProjection::names(names)
}

fn add_attribution(target: &mut ColumnBundleReadAttribution, value: &ColumnBundleReadAttribution) {
    target.plan_ns = target.plan_ns.saturating_add(value.plan_ns);
    target.execute_wall_ns = target.execute_wall_ns.saturating_add(value.execute_wall_ns);
    target.callback_wall_ns = target
        .callback_wall_ns
        .saturating_add(value.callback_wall_ns);
    target.row_group_read_ns = target
        .row_group_read_ns
        .saturating_add(value.row_group_read_ns);
    target.read_io_ns = target.read_io_ns.saturating_add(value.read_io_ns);
    target.checksum_ns = target.checksum_ns.saturating_add(value.checksum_ns);
    target.decompression_ns = target
        .decompression_ns
        .saturating_add(value.decompression_ns);
    target.primitive_decode_ns = target
        .primitive_decode_ns
        .saturating_add(value.primitive_decode_ns);
    target.fixed_payload_decode_ns = target
        .fixed_payload_decode_ns
        .saturating_add(value.fixed_payload_decode_ns);
    target.copy_materialization_ns = target
        .copy_materialization_ns
        .saturating_add(value.copy_materialization_ns);
    target.native_to_c_copy_ns =
        sum_optional(target.native_to_c_copy_ns, value.native_to_c_copy_ns);
    target.wrapper_copy_ns = sum_optional(target.wrapper_copy_ns, value.wrapper_copy_ns);
    target.bytes_read = target.bytes_read.saturating_add(value.bytes_read);
    target.compressed_bytes = target
        .compressed_bytes
        .saturating_add(value.compressed_bytes);
    target.uncompressed_bytes = target
        .uncompressed_bytes
        .saturating_add(value.uncompressed_bytes);
    target.requested_threads = target.requested_threads.max(value.requested_threads);
    target.effective_threads = target.effective_threads.max(value.effective_threads);
    target.selected_row_groups = target
        .selected_row_groups
        .saturating_add(value.selected_row_groups);
    target.pruned_row_groups = target
        .pruned_row_groups
        .saturating_add(value.pruned_row_groups);
    target.selected_column_chunks = target
        .selected_column_chunks
        .saturating_add(value.selected_column_chunks);
    if target.fallback_reason.is_none() {
        target.fallback_reason = value.fallback_reason;
    }
}

fn sum_optional(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left.saturating_add(right)),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn duration_to_ns(duration: std::time::Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    use crate::column_bundle::PrimitiveColumnValues;
    use crate::compact_l2::{
        COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1, COMPACT_L2_RECORD_KIND_ORDER,
        COMPACT_L2_RECORD_KIND_TRADE, CompactL2PhysicalV2Record,
    };
    use crate::format::{
        OCB_BOOTSTRAP_PAGE_V1_LEN, OCB_NULL_U32, OCB_ROOT_V1_LEN, OcbBodyKindV1, OcbBodyRefV2,
        OcbBootstrapPageV1, OcbChunkCodecV1, OcbColumnChunkDescV1, OcbColumnChunkObjectV1,
        OcbColumnDescV1, OcbLogicalKindV1, OcbNullabilityV1, OcbPhysicalTypeV1, OcbRootV1,
        OcbRowGroupDescV1, OcbRowGroupIndexV1, OcbSchemaV1, OcbStringTableV1, crc32c,
    };

    #[test]
    fn channel_parallel_reader_matches_serial_fingerprint() {
        let root = fixture_root("channel_parallel_reader_matches_serial_fingerprint");
        let inputs = write_channel_fixtures(&root, &[3, 1, 2], 4);
        let serial = read_fingerprint(&inputs, 1, 1, 1);
        let parallel = read_fingerprint(&inputs, 4, 1, 1);
        assert_eq!(parallel.0, serial.0);
        assert_eq!(parallel.1.rows_yielded, 12);
        assert_eq!(parallel.1.channel_reports.len(), 3);
        assert_eq!(parallel.1.effective_channel_workers, 3);
        assert_eq!(
            parallel
                .1
                .channel_reports
                .iter()
                .map(|report| report.channel_id)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );
    }

    #[cfg(unix)]
    #[test]
    fn bound_manifest_prepare_and_read_ignore_later_path_replacement() {
        let root = fixture_root("bound_manifest_path_replacement");
        let artifact = root.join("channels/2011/l2_mutations.physical-v2.ocb");
        let replacement = root.join("channels/2011/replacement.physical-v2.ocb");
        let original_records = (0..4)
            .map(|offset| {
                compact_l2_record(20260705, 2011, offset + 1, COMPACT_L2_RECORD_KIND_ORDER)
            })
            .collect::<Vec<_>>();
        let replacement_records = (0..4)
            .map(|offset| {
                compact_l2_record(20260705, 2999, offset + 1, COMPACT_L2_RECORD_KIND_ORDER)
            })
            .collect::<Vec<_>>();
        write_compact_l2_physical_v2_fixture(&artifact, &original_records);
        write_compact_l2_physical_v2_fixture(&replacement, &replacement_records);
        let manifest_path = root.join("manifest.json");
        write_physical_v2_manifest(&manifest_path, 2011, original_records.len() as u64);
        let manifest =
            ChannelShardedManifestV1::from_path(&manifest_path).expect("parse physical manifest");
        let inputs = compact_l2_physical_v2_bound_inputs_from_manifest(&manifest_path, &manifest)
            .expect("bind manifest inputs");
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].channel_id(), 2011);

        fs::rename(&replacement, &artifact).expect("atomically replace manifest artifact");

        let mut prepared_channel_ids = Vec::new();
        parallel_prepare_compact_l2_physical_v2_bound_channel(
            inputs[0].clone(),
            CompactL2PhysicalV2ParallelPrepareOptions::default(),
            |_, view| Ok(view.channel_id.to_vec()),
            |_, channel_ids| {
                prepared_channel_ids.extend(channel_ids);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("bound parallel prepare stays on original artifact");
        assert!(
            prepared_channel_ids
                .iter()
                .all(|channel_id| *channel_id == 2011)
        );

        let observed_channel_ids = Arc::new(Mutex::new(Vec::new()));
        let observed_for_visitor = Arc::clone(&observed_channel_ids);
        let report = read_compact_l2_physical_v2_bound_channels(
            inputs,
            CompactL2PhysicalV2ParallelReadOptions::default(),
            move |batch| {
                let view = CompactL2PhysicalV2BatchView::from_column_batch(&batch.batch)?;
                observed_for_visitor
                    .lock()
                    .expect("observed channel lock")
                    .extend_from_slice(view.channel_id);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("bound channel read stays on original artifact");
        assert_eq!(report.rows_yielded, 4);
        assert!(
            observed_channel_ids
                .lock()
                .expect("observed channel lock")
                .iter()
                .all(|channel_id| *channel_id == 2011)
        );

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn channel_parallel_reader_propagates_worker_error() {
        let root = fixture_root("channel_parallel_reader_propagates_worker_error");
        let missing = root.join("missing.ocb");
        let err = read_compact_l2_physical_v2_channels(
            [CompactL2PhysicalV2ChannelReadInput::new(1, missing)],
            CompactL2PhysicalV2ParallelReadOptions::default(),
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("missing file must fail");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::Io);
    }

    #[test]
    fn channel_parallel_reader_rejects_invalid_options_and_inputs() {
        let options = CompactL2PhysicalV2ParallelReadOptions {
            channel_workers: 0,
            ..CompactL2PhysicalV2ParallelReadOptions::default()
        };
        let err = read_compact_l2_physical_v2_channels(
            [CompactL2PhysicalV2ChannelReadInput::new(1, "unused.ocb")],
            options,
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("zero channel workers must fail");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);

        let err = read_compact_l2_physical_v2_channels(
            [
                CompactL2PhysicalV2ChannelReadInput::new(7, "a.ocb"),
                CompactL2PhysicalV2ChannelReadInput::new(7, "b.ocb"),
            ],
            CompactL2PhysicalV2ParallelReadOptions::default(),
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("duplicate channel id must fail");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
    }

    #[test]
    fn channel_worker_limits_partition_operation_wide_aggregate_budgets() {
        let limits = OcbResourceLimits::new(100, 90, 80, 70, 11, 17).expect("resource limits");
        let worker_limits = (0..3)
            .map(|worker_index| channel_worker_resource_limits(limits, worker_index, 3).unwrap())
            .collect::<Vec<_>>();

        assert_eq!(
            worker_limits
                .iter()
                .map(|limits| limits.max_owned_selected_compressed_bytes())
                .collect::<Vec<_>>(),
            vec![3, 3, 3]
        );
        assert_eq!(
            worker_limits
                .iter()
                .map(|limits| limits.max_owned_decoded_materialized_bytes())
                .collect::<Vec<_>>(),
            vec![5, 5, 5]
        );
        assert!(worker_limits.iter().all(|worker_limits| {
            worker_limits.max_encoded_object_bytes() == limits.max_encoded_object_bytes()
                && worker_limits.max_compressed_chunk_bytes() == limits.max_compressed_chunk_bytes()
                && worker_limits.max_decompressed_chunk_bytes()
                    == limits.max_decompressed_chunk_bytes()
                && worker_limits.max_projected_row_group_bytes()
                    == limits.max_projected_row_group_bytes()
        }));

        let err = channel_worker_resource_limits(limits, 3, 3)
            .expect_err("out-of-range worker slot must fail");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
    }

    #[test]
    fn explicit_resource_limits_reach_prepare_and_channel_read_paths() {
        let root = fixture_root("parallel_read_explicit_resource_limits");
        let inputs = write_channel_fixtures(&root, &[1], 2);
        let limits = OcbResourceLimits::policy_a()
            .with_max_projected_row_group_bytes(0)
            .unwrap();

        let err = read_compact_l2_physical_v2_channels_with_resource_limits(
            inputs.clone(),
            CompactL2PhysicalV2ParallelReadOptions::default(),
            limits,
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("channel read must enforce the explicit projected row-group limit");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);

        let err = parallel_prepare_compact_l2_physical_v2_channel_with_resource_limits(
            inputs.into_iter().next().unwrap(),
            CompactL2PhysicalV2ParallelPrepareOptions::default(),
            limits,
            |_, _| Ok(()),
            |_, _| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("parallel prepare must enforce the explicit projected row-group limit");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
    }

    #[test]
    fn compact_parallel_prepare_matches_worker_counts_and_carries_context() {
        let root = fixture_root("compact_parallel_prepare_context");
        let channel_id = 2011;
        let records = (0..10)
            .map(|offset| {
                compact_l2_record(
                    20260705,
                    channel_id,
                    offset + 1,
                    if offset % 2 == 0 {
                        COMPACT_L2_RECORD_KIND_ORDER
                    } else {
                        COMPACT_L2_RECORD_KIND_TRADE
                    },
                )
            })
            .collect::<Vec<_>>();
        let path = root.join("channel_2011.ocb");
        write_compact_l2_physical_v2_fixture(&path, &records);
        let input = CompactL2PhysicalV2ChannelReadInput::new(channel_id, path)
            .with_expected_rows(records.len() as u64);
        let mut expected = None;

        for workers in [1, 2, 4, 8] {
            let mut committed = Vec::new();
            let report = parallel_prepare_compact_l2_physical_v2_channel(
                input.clone(),
                CompactL2PhysicalV2ParallelPrepareOptions {
                    workers,
                    max_in_flight_row_groups: 5,
                    validate_checksums: true,
                },
                |context, view| {
                    assert_eq!(context.channel_id, channel_id);
                    assert!(context.row_group.worker_id < workers.min(5));
                    assert_eq!(view.row_count as u64, context.row_group.row_count);
                    assert!(
                        view.channel_id
                            .iter()
                            .all(|observed| *observed == i64::from(channel_id))
                    );
                    Ok((
                        context.row_group.selected_row_group_ordinal,
                        context.row_group.row_group_id,
                        context.row_group.base_row,
                        context.row_group.row_end,
                        context.row_group.row_count,
                        view.biz_index.to_vec(),
                    ))
                },
                |context, prepared| {
                    assert_eq!(context.channel_id, channel_id);
                    assert_eq!(context.row_group.selected_row_group_ordinal, prepared.0);
                    committed.push(prepared);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("compact parallel prepare");
            assert_eq!(report.channel_id, channel_id);
            assert_eq!(report.parallel_prepare.requested_workers, workers);
            assert_eq!(report.parallel_prepare.started_workers, workers.min(5));
            assert_eq!(report.parallel_prepare.row_groups_queued, 5);
            assert_eq!(report.parallel_prepare.row_groups_completed, 5);
            assert_eq!(report.parallel_prepare.row_groups_ordered_committed, 5);
            assert_eq!(report.parallel_prepare.rows_ordered_committed, 10);
            assert!(report.parallel_prepare.ordered_terminal_completed);
            assert_eq!(
                committed
                    .iter()
                    .map(|prepared| prepared.1)
                    .collect::<Vec<_>>(),
                vec![0, 1, 2, 3, 4]
            );
            assert_eq!(
                committed
                    .iter()
                    .map(|prepared| prepared.2)
                    .collect::<Vec<_>>(),
                vec![0, 2, 4, 6, 8]
            );
            if let Some(expected) = &expected {
                assert_eq!(&committed, expected);
            } else {
                expected = Some(committed);
            }
        }
    }

    #[test]
    fn compact_parallel_prepare_fails_closed_for_channel_and_view_mismatch() {
        let root = fixture_root("compact_parallel_prepare_malformed");
        let records = (0..4)
            .map(|offset| {
                compact_l2_record(20260705, 2011, offset + 1, COMPACT_L2_RECORD_KIND_ORDER)
            })
            .collect::<Vec<_>>();
        let path = root.join("channel_2011.ocb");
        write_compact_l2_physical_v2_fixture(&path, &records);
        let mut committed = 0usize;
        let error = parallel_prepare_compact_l2_physical_v2_channel(
            CompactL2PhysicalV2ChannelReadInput::new(2011, &path).with_expected_rows(999),
            CompactL2PhysicalV2ParallelPrepareOptions::default(),
            |_, _| Ok(()),
            |_, _| {
                committed += 1;
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect_err("metadata row mismatch must fail before commit");
        assert_eq!(
            OcbErrorKind::from_error(&error),
            Some(OcbErrorKind::RowCountMismatch)
        );
        assert_eq!(committed, 0);

        let error = parallel_prepare_compact_l2_physical_v2_channel(
            CompactL2PhysicalV2ChannelReadInput::new(2012, &path),
            CompactL2PhysicalV2ParallelPrepareOptions::default(),
            |_, _| Ok(()),
            |_, _| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("mismatched channel must fail closed");
        assert_eq!(error.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
        assert_eq!(
            OcbErrorKind::from_error(&error),
            Some(OcbErrorKind::ChannelIdMismatch)
        );

        let malformed_path = root.join("channel_2011_bad_symbol.ocb");
        write_compact_l2_physical_v2_fixture_with_symbol_width(&malformed_path, &records, 8);
        let mut committed = 0usize;
        let error = parallel_prepare_compact_l2_physical_v2_channel(
            CompactL2PhysicalV2ChannelReadInput::new(2011, malformed_path),
            CompactL2PhysicalV2ParallelPrepareOptions::default(),
            |_, _| Ok(()),
            |_, _| {
                committed += 1;
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect_err("malformed physical-v2 view must fail closed");
        assert_eq!(error.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
        assert_eq!(committed, 0);
    }

    fn read_fingerprint(
        inputs: &[CompactL2PhysicalV2ChannelReadInput],
        channel_workers: usize,
        per_file_read_threads: usize,
        max_in_flight_row_groups: usize,
    ) -> (
        Vec<(u32, u32, u64, i64)>,
        CompactL2PhysicalV2ParallelReadReport,
    ) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let seen_for_visitor = Arc::clone(&seen);
        let report = read_compact_l2_physical_v2_channels(
            inputs.to_vec(),
            CompactL2PhysicalV2ParallelReadOptions {
                channel_workers,
                per_file_read_threads,
                max_in_flight_row_groups,
                validate_checksums: true,
            },
            move |event| {
                let biz_index = column_i64(&event.batch, COMPACT_L2_BIZ_INDEX_COLUMN_NAME);
                let first = biz_index.first().copied().unwrap_or_default();
                seen_for_visitor.lock().unwrap().push((
                    event.channel_id,
                    event.batch.row_group_id,
                    event.batch.row_count,
                    first,
                ));
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("read compact-L2 physical-v2 channels");
        let mut seen = Arc::try_unwrap(seen).unwrap().into_inner().unwrap();
        seen.sort();
        (seen, report)
    }

    fn column_i64<'a>(batch: &'a ColumnBatch, name: &str) -> &'a [i64] {
        let column = batch
            .columns
            .iter()
            .find(|column| column.name == name)
            .expect("column exists");
        match &column.values {
            PrimitiveColumnValues::I64(values) => values,
            _ => panic!("column is not i64"),
        }
    }

    fn fixture_root(name: &str) -> PathBuf {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tmp")
            .join(name);
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn write_physical_v2_manifest(path: &Path, channel_id: u32, rows: u64) {
        let manifest = serde_json::json!({
            "schema_version": 1,
            "trading_day": 20260705,
            "artifact_format": COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
            "selection_scope": "contiguous-prefix",
            "channel_indivisible": true,
            "counts": {"channels": 1, "row_count": rows},
            "channels": [{
                "channel_id": channel_id,
                "relative_path": format!("channels/{channel_id}/l2_mutations.physical-v2.ocb"),
                "row_count": rows,
                "row_group_count": rows.div_ceil(2),
                "first_biz_index": 1,
                "last_biz_index": rows
            }]
        });
        fs::write(
            path,
            serde_json::to_vec_pretty(&manifest).expect("encode manifest"),
        )
        .expect("write physical-v2 manifest");
    }

    fn write_channel_fixtures(
        root: &Path,
        channel_ids: &[u32],
        rows_per_channel: usize,
    ) -> Vec<CompactL2PhysicalV2ChannelReadInput> {
        channel_ids
            .iter()
            .copied()
            .map(|channel_id| {
                let records = (0..rows_per_channel)
                    .map(|offset| {
                        compact_l2_record(
                            20260705,
                            channel_id,
                            offset as u64 + 1,
                            if offset % 2 == 0 {
                                COMPACT_L2_RECORD_KIND_ORDER
                            } else {
                                COMPACT_L2_RECORD_KIND_TRADE
                            },
                        )
                    })
                    .collect::<Vec<_>>();
                let path = root.join(format!("channel_{channel_id:03}.ocb"));
                write_compact_l2_physical_v2_fixture(&path, &records);
                CompactL2PhysicalV2ChannelReadInput::new(channel_id, path)
                    .with_expected_rows(rows_per_channel as u64)
            })
            .collect()
    }

    fn write_compact_l2_physical_v2_fixture(path: &Path, records: &[[u8; 168]]) {
        write_compact_l2_physical_v2_fixture_with_symbol_width(path, records, 9);
    }

    fn write_compact_l2_physical_v2_fixture_with_symbol_width(
        path: &Path,
        records: &[[u8; 168]],
        symbol_width: u32,
    ) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create physical-v2 parent");
        }
        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
        let mut row_groups = Vec::new();
        let mut chunk_descs = Vec::new();
        let row_group_size = 2usize;
        let column_names = physical_v2_column_names();

        for (row_group_id, chunk_records) in records.chunks(row_group_size).enumerate() {
            let row_group_id = row_group_id as u32;
            let chunk_desc_begin = chunk_descs.len() as u64;
            let decoded = chunk_records
                .iter()
                .map(|record| CompactL2PhysicalV2Record::from_fixed_binary_v1(record).unwrap())
                .collect::<Vec<_>>();
            let row_count = decoded.len() as u64;

            push_i32_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                0,
                &decoded
                    .iter()
                    .map(|record| i32::try_from(record.trading_day).unwrap())
                    .collect::<Vec<_>>(),
            );
            push_i64_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                1,
                &decoded
                    .iter()
                    .map(|record| i64::from(record.channel_id))
                    .collect::<Vec<_>>(),
            );
            push_i64_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                2,
                &decoded
                    .iter()
                    .map(|record| record.biz_index)
                    .collect::<Vec<_>>(),
            );
            push_i64_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                3,
                &decoded
                    .iter()
                    .map(|record| record.receive_nano)
                    .collect::<Vec<_>>(),
            );
            push_i64_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                4,
                &decoded
                    .iter()
                    .map(|record| i64::try_from(record.source_ordinal).unwrap())
                    .collect::<Vec<_>>(),
            );
            push_i32_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                5,
                &decoded
                    .iter()
                    .map(|record| i32::from(record.record_kind.code()))
                    .collect::<Vec<_>>(),
            );
            let header_bytes = decoded
                .iter()
                .map(|record| [record.source_family, record.exchange_id])
                .collect::<Vec<_>>();
            push_fixed_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                6,
                2,
                &header_bytes,
            );
            push_i64_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                7,
                &decoded
                    .iter()
                    .map(|record| record.exchange_time)
                    .collect::<Vec<_>>(),
            );
            let symbols = decoded
                .iter()
                .map(|record| record.symbol)
                .collect::<Vec<_>>();
            push_fixed_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                8,
                9,
                &symbols,
            );
            let body_bytes = decoded
                .iter()
                .map(|record| record.body_bytes_80_86)
                .collect::<Vec<_>>();
            push_fixed_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                9,
                7,
                &body_bytes,
            );
            for slot in 0..10 {
                push_i64_chunk(
                    &mut file_bytes,
                    &mut chunk_descs,
                    row_group_id,
                    10 + slot as u32,
                    &decoded
                        .iter()
                        .map(|record| record.body_words_88_160[slot])
                        .collect::<Vec<_>>(),
                );
            }

            row_groups.push(OcbRowGroupDescV1 {
                row_group_id,
                flags: 0,
                base_row: (row_group_id as u64) * row_group_size as u64,
                row_count,
                chunk_desc_begin,
                chunk_desc_count: column_names.len() as u32,
                stat_begin: 0,
                stat_count: 0,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            });
        }

        let string_table = OcbStringTableV1 {
            version: 1,
            strings: column_names.iter().map(|name| (*name).to_owned()).collect(),
            crc32c: 0,
        };
        let string_table_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
                string_table.write_to(buf)
            });
        let mut column_descs = physical_v2_column_descs();
        column_descs[8].fixed_binary_width = symbol_width;
        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns: column_descs,
            crc32c: 0,
        };
        let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
            schema.write_to(buf)
        });
        let row_group_index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups,
            column_chunks: chunk_descs,
            stats: Vec::new(),
            crc32c: 0,
        };
        let row_group_index_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
                row_group_index.write_to(buf)
            });
        let root = OcbRootV1 {
            version: 1,
            flags: 0,
            row_count: records.len() as u64,
            column_count: column_names.len() as u32,
            row_group_count: records.len().div_ceil(row_group_size) as u32,
            dictionary_count: 0,
            schema_ref,
            dictionary_index_ref: OcbBodyRefV2::NULL,
            row_group_index_ref,
            ordering_proof_ref: OcbBodyRefV2::NULL,
            debug_json_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 0,
            content_flags: 0,
            crc32c: 0,
        };
        let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            root.write_to(buf)
        });
        assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);
        let bootstrap = OcbBootstrapPageV1::new([91u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
        let mut file = fs::File::create(path).expect("create physical-v2 fixture");
        file.write_all(&file_bytes)
            .expect("write physical-v2 fixture");
    }

    fn physical_v2_column_names() -> Vec<&'static str> {
        let mut names = Vec::new();
        names.push(COMPACT_L2_DAY_KEY_COLUMN_NAME);
        names.push(COMPACT_L2_CHANNEL_ID_COLUMN_NAME);
        names.push(COMPACT_L2_BIZ_INDEX_COLUMN_NAME);
        names.push(COMPACT_L2_RECEIVE_NANO_COLUMN_NAME);
        names.push(COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME);
        names.push(COMPACT_L2_RECORD_KIND_COLUMN_NAME);
        names.push(COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME);
        names.push(COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME);
        names.push(COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME);
        names.push(COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME);
        for column in COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS {
            names.push(column.column_name);
        }
        names
    }

    fn push_i32_chunk(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        values: &[i32],
    ) {
        let mut payload = Vec::with_capacity(values.len() * 4);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let value_ref = append_primitive_chunk(
            file_bytes,
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I32,
            values.len() as u64,
            payload,
        );
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I32,
            value_ref,
            values.len() as u64,
            (values.len() * 4) as u64,
        ));
    }

    fn push_i64_chunk(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        values: &[i64],
    ) {
        let mut payload = Vec::with_capacity(values.len() * 8);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let value_ref = append_primitive_chunk(
            file_bytes,
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I64,
            values.len() as u64,
            payload,
        );
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I64,
            value_ref,
            values.len() as u64,
            (values.len() * 8) as u64,
        ));
    }

    fn push_fixed_chunk<const N: usize>(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        width: u32,
        values: &[[u8; N]],
    ) {
        let rows = values
            .iter()
            .map(|value| value.as_slice())
            .collect::<Vec<_>>();
        let value_ref =
            append_fixed_binary_chunk(file_bytes, row_group_id, column_id, width, &rows);
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::FixedBinary,
            value_ref,
            values.len() as u64,
            values.len() as u64 * u64::from(width),
        ));
    }

    fn append_primitive_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        row_count: u64,
        payload: Vec<u8>,
    ) -> OcbBodyRefV2 {
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id,
            column_id,
            row_count,
            uncompressed_bytes: payload.len() as u64,
            payload,
            crc32c: 0,
        };
        append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
            chunk.write_to(buf)
        })
    }

    fn append_fixed_binary_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        width: u32,
        values: &[&[u8]],
    ) -> OcbBodyRefV2 {
        let mut payload = Vec::with_capacity(values.len() * width as usize);
        for value in values {
            assert_eq!(value.len(), width as usize);
            payload.extend_from_slice(value);
        }
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::FixedBinary,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id,
            column_id,
            row_count: values.len() as u64,
            uncompressed_bytes: payload.len() as u64,
            payload,
            crc32c: 0,
        };
        append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
            chunk.write_to(buf)
        })
    }

    fn chunk_desc(
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        value_ref: OcbBodyRefV2,
        row_count: u64,
        uncompressed_bytes: u64,
    ) -> OcbColumnChunkDescV1 {
        OcbColumnChunkDescV1 {
            row_group_id,
            column_id,
            physical_type,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            value_ref,
            validity_ref: OcbBodyRefV2::NULL,
            row_count,
            uncompressed_bytes,
        }
    }

    fn physical_v2_column_descs() -> Vec<OcbColumnDescV1> {
        let mut columns = Vec::new();
        columns.push(primitive_column_desc(0, 0, OcbPhysicalTypeV1::I32));
        columns.push(primitive_column_desc(1, 1, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(2, 2, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(3, 3, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(4, 4, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(5, 5, OcbPhysicalTypeV1::I32));
        columns.push(fixed_binary_column_desc(6, 6, 2));
        columns.push(primitive_column_desc(7, 7, OcbPhysicalTypeV1::I64));
        columns.push(fixed_binary_column_desc(8, 8, 9));
        columns.push(fixed_binary_column_desc(9, 9, 7));
        for column_id in 10..20 {
            columns.push(primitive_column_desc(
                column_id,
                column_id,
                OcbPhysicalTypeV1::I64,
            ));
        }
        columns
    }

    fn primitive_column_desc(
        column_id: u32,
        name_string_id: u32,
        physical_type: OcbPhysicalTypeV1,
    ) -> OcbColumnDescV1 {
        OcbColumnDescV1 {
            column_id,
            name_string_id,
            physical_type,
            logical_kind: OcbLogicalKindV1::Plain,
            flags: 0,
            dictionary_id: OCB_NULL_U32,
            scale: 0,
            nullability: OcbNullabilityV1::NonNull,
            reserved0: 0,
            fixed_binary_width: 0,
        }
    }

    fn fixed_binary_column_desc(
        column_id: u32,
        name_string_id: u32,
        width: u32,
    ) -> OcbColumnDescV1 {
        let mut desc =
            primitive_column_desc(column_id, name_string_id, OcbPhysicalTypeV1::FixedBinary);
        desc.fixed_binary_width = width;
        desc
    }

    fn compact_l2_record(
        trading_day: u32,
        channel_id: u32,
        biz_index: u64,
        record_kind: u8,
    ) -> [u8; COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize] {
        let mut bytes = [0u8; COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize];
        bytes[0..4].copy_from_slice(b"ALIR");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&80u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&168u16.to_le_bytes());
        bytes[10] = record_kind;
        bytes[11] = 1;
        bytes[12] = 1;
        bytes[16..20].copy_from_slice(&trading_day.to_le_bytes());
        bytes[20..24].copy_from_slice(&(channel_id as i32).to_le_bytes());
        bytes[24..32].copy_from_slice(&(biz_index as i64).to_le_bytes());
        bytes[32..40].copy_from_slice(&biz_index.to_le_bytes());
        bytes[40..48].copy_from_slice(&(1_000_000_i64 + biz_index as i64).to_le_bytes());
        bytes[48..56].copy_from_slice(&(2_000_000_i64 + biz_index as i64).to_le_bytes());
        bytes[56..65].copy_from_slice(b"600000.SH");
        bytes
    }

    fn append_encoded_object(
        file_bytes: &mut Vec<u8>,
        kind: OcbBodyKindV1,
        write: impl FnOnce(&mut Vec<u8>) -> Result<()>,
    ) -> OcbBodyRefV2 {
        let mut object = Vec::new();
        write(&mut object).expect("encode object");
        align_file(file_bytes, 8);
        let offset = file_bytes.len() as u64;
        let length = object.len() as u64;
        let checksum = crc32c(&object);
        file_bytes.extend_from_slice(&object);
        OcbBodyRefV2::new(offset, length, kind, checksum)
    }

    fn align_file(file_bytes: &mut Vec<u8>, alignment: usize) {
        let rem = file_bytes.len() % alignment;
        if rem != 0 {
            file_bytes.resize(file_bytes.len() + (alignment - rem), 0);
        }
    }
}
