//! Certification helpers for channel-sharded compact-L2 OCB artifacts.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::compact_l2::{
    COMPACT_L2_BIZ_INDEX_COLUMN_NAME, COMPACT_L2_CHANNEL_ID_COLUMN_NAME,
    COMPACT_L2_DAY_KEY_COLUMN_NAME, COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
    COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1, COMPACT_L2_PAYLOAD_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT, COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS, COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME, COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
    COMPACT_L2_RECORD_KIND_COLUMN_NAME, COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE,
    COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME, CompactL2PhysicalV2BatchView, CompactL2RecordKind,
    decode_compact_l2_fixed_binary_header_v1,
};
use crate::manifest::{
    ChannelArtifactEntryV1, ChannelShardedManifestV1, resolve_manifest_relative_artifact_path,
    validate_hex_hash,
};
use crate::{
    ArcadiaTioError, ColumnBundleFile, ColumnBundleReadCursorOptions, ColumnBundleReadOptions,
    ColumnBundleReadRequest, ColumnBundleRowGroupSummary, ColumnBundleVisitControl,
    ColumnPhysicalType, ColumnProjection, OcbErrorKind, PrimitiveColumnValuesRef, Result,
};

/// Options for channel-sharded compact-L2 artifact certification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationOptions {
    /// Expected artifact format/layout label.
    pub artifact_format: String,
    /// Expected fixed-binary record width.
    pub expected_record_width: u32,
    /// Fixed-binary payload column name.
    pub payload_column_name: String,
    /// Validate compact-L2 payload headers and channel/BizIndex continuity.
    pub verify_payload_header: bool,
    /// Request CRC/checksum validation. Current OCB reads remain fail-closed and
    /// validate checksums even when this is false.
    pub verify_crc32c: bool,
    /// Optional aggregate row cap.
    pub max_rows: Option<u64>,
    /// Requested OCB read threads.
    pub read_threads: usize,
    /// Maximum reusable row groups in flight while certifying one artifact.
    pub max_in_flight_row_groups: usize,
    /// Validate optional manifest SHA-256/FNV fingerprints when present.
    pub verify_hashes: bool,
}

impl Default for CertificationOptions {
    fn default() -> Self {
        Self {
            artifact_format: COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1.to_owned(),
            expected_record_width: COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            payload_column_name: COMPACT_L2_PAYLOAD_COLUMN_NAME.to_owned(),
            verify_payload_header: true,
            verify_crc32c: true,
            max_rows: None,
            read_threads: 1,
            max_in_flight_row_groups: 1,
            verify_hashes: true,
        }
    }
}

/// Path-free certification report for a channel-sharded compact-L2 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationReport {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Number of channels certified.
    pub channel_count: usize,
    /// Aggregate certified row count.
    pub row_count: u64,
    /// Aggregate certified row-group count.
    pub row_group_count: u64,
    /// Number of failed channels. This is zero for `Ok` reports; failures return
    /// an error and stop before exposing raw paths.
    pub failed_channel_count: usize,
    /// Per-channel path-free reports in manifest order.
    pub channel_reports: Vec<ChannelCertificationReport>,
    /// Safe aggregate summary with no absolute paths.
    pub safe_summary: SafeCertificationSummary,
}

/// Path-free per-channel certification report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelCertificationReport {
    /// ChannelID certified for this artifact.
    pub channel_id: u32,
    /// Observed row count.
    pub row_count: u64,
    /// Observed OCB row-group count.
    pub row_group_count: u32,
    /// First decoded BizIndex.
    pub first_biz_index: u64,
    /// Last decoded BizIndex.
    pub last_biz_index: u64,
    /// Minimum observed receive nano, when payload headers were verified.
    pub min_receive_nano: Option<i64>,
    /// Maximum observed receive nano, when payload headers were verified.
    pub max_receive_nano: Option<i64>,
    /// Observed order record count, when payload headers were verified.
    pub order_record_count: Option<u64>,
    /// Observed trade record count, when payload headers were verified.
    pub trade_record_count: Option<u64>,
    /// Whether at least one optional manifest hash/fingerprint check was present
    /// and passed.
    pub checksum_verified: bool,
}

/// Path-free aggregate certification summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeCertificationSummary {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Artifact format/layout label.
    pub artifact_format: String,
    /// Number of certified channels.
    pub channel_count: usize,
    /// Aggregate row count.
    pub row_count: u64,
    /// Aggregate OCB row-group count.
    pub row_group_count: u64,
    /// Whether certification completed.
    pub certified: bool,
    /// Always true: raw/absolute paths are excluded.
    pub path_redacted: bool,
}

/// Options for certifying one compact-L2 physical-v2 OCB artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2CertificationOptions {
    /// Optional expected total row count.
    pub expected_row_count: Option<u64>,
    /// Optional expected trading day encoded in `day_key`.
    pub expected_trading_day: Option<u32>,
    /// Optional expected channel id encoded in `channel_id`.
    pub expected_channel_id: Option<u32>,
    /// Optional expected first BizIndex.
    pub expected_first_biz_index: Option<u64>,
    /// Optional expected last BizIndex.
    pub expected_last_biz_index: Option<u64>,
    /// Validate row-level scalar values and gap-free BizIndex continuity.
    pub verify_scalar_continuity: bool,
    /// Reconstruct legacy fixed-binary v1 payloads while scanning rows.
    pub verify_legacy_reconstruction: bool,
    /// Optional expected FNV-1a64 hash over reconstructed legacy payload bytes.
    pub expected_legacy_payload_hash_fnv1a64: Option<String>,
    /// Optional aggregate row cap.
    pub max_rows: Option<u64>,
    /// Requested OCB read threads.
    pub read_threads: usize,
    /// Maximum row groups read at once.
    pub max_in_flight_row_groups: usize,
}

impl Default for CompactL2PhysicalV2CertificationOptions {
    fn default() -> Self {
        Self {
            expected_row_count: None,
            expected_trading_day: None,
            expected_channel_id: None,
            expected_first_biz_index: None,
            expected_last_biz_index: None,
            verify_scalar_continuity: true,
            verify_legacy_reconstruction: false,
            expected_legacy_payload_hash_fnv1a64: None,
            max_rows: None,
            read_threads: 1,
            max_in_flight_row_groups: 1,
        }
    }
}

/// Path-free report for one compact-L2 physical-v2 artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2CertificationReport {
    /// Observed row count.
    pub row_count: u64,
    /// Observed OCB row-group count.
    pub row_group_count: u32,
    /// Number of required physical-v2 columns certified.
    pub required_column_count: usize,
    /// Number of projected column chunks certified.
    pub selected_column_chunk_count: u64,
    /// Sum of projected compressed chunk bytes.
    pub selected_compressed_bytes: u64,
    /// Sum of projected uncompressed chunk bytes.
    pub selected_uncompressed_bytes: u64,
    /// First decoded BizIndex when scalar continuity was verified.
    pub first_biz_index: Option<u64>,
    /// Last decoded BizIndex when scalar continuity was verified.
    pub last_biz_index: Option<u64>,
    /// Minimum decoded receive nano when scalar continuity was verified.
    pub min_receive_nano: Option<i64>,
    /// Maximum decoded receive nano when scalar continuity was verified.
    pub max_receive_nano: Option<i64>,
    /// Observed order row count when scalar continuity was verified.
    pub order_record_count: Option<u64>,
    /// Observed trade row count when scalar continuity was verified.
    pub trade_record_count: Option<u64>,
    /// Reconstructed legacy payload FNV-1a64 hash, when requested.
    pub legacy_payload_hash_fnv1a64: Option<String>,
    /// Whether an expected legacy payload hash was supplied and matched.
    pub legacy_payload_hash_verified: bool,
    /// Always true: raw/absolute paths are excluded.
    pub path_redacted: bool,
    /// Always false: certification is read-only.
    pub writes_transformed_artifacts: bool,
}

/// Options for certifying a channel-sharded compact-L2 physical-v2 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ManifestCertificationOptions {
    /// Expected artifact format/layout label.
    pub artifact_format: String,
    /// Validate row-level scalar values and gap-free BizIndex continuity.
    pub verify_scalar_continuity: bool,
    /// Reconstruct legacy fixed-binary v1 payloads while scanning rows.
    pub verify_legacy_reconstruction: bool,
    /// Validate optional manifest SHA-256/FNV file fingerprints when present.
    pub verify_hashes: bool,
    /// Optional aggregate row cap.
    pub max_rows: Option<u64>,
    /// Requested OCB read threads.
    pub read_threads: usize,
    /// Maximum row groups read at once.
    pub max_in_flight_row_groups: usize,
}

impl Default for CompactL2PhysicalV2ManifestCertificationOptions {
    fn default() -> Self {
        Self {
            artifact_format: COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT.to_owned(),
            verify_scalar_continuity: true,
            verify_legacy_reconstruction: false,
            verify_hashes: true,
            max_rows: None,
            read_threads: 1,
            max_in_flight_row_groups: 1,
        }
    }
}

/// Path-free certification report for a compact-L2 physical-v2 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ManifestCertificationReport {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Number of channels certified.
    pub channel_count: usize,
    /// Aggregate certified row count.
    pub row_count: u64,
    /// Aggregate certified row-group count.
    pub row_group_count: u64,
    /// Number of failed channels. This is zero for `Ok` reports; failures return
    /// an error and stop before exposing raw paths.
    pub failed_channel_count: usize,
    /// Per-channel path-free reports in manifest order.
    pub channel_reports: Vec<CompactL2PhysicalV2ChannelCertificationReport>,
    /// Safe aggregate summary with no absolute paths.
    pub safe_summary: SafeCertificationSummary,
}

/// Path-free per-channel compact-L2 physical-v2 certification report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ChannelCertificationReport {
    /// ChannelID certified for this artifact.
    pub channel_id: u32,
    /// Observed row count.
    pub row_count: u64,
    /// Observed OCB row-group count.
    pub row_group_count: u32,
    /// First decoded BizIndex.
    pub first_biz_index: Option<u64>,
    /// Last decoded BizIndex.
    pub last_biz_index: Option<u64>,
    /// Minimum observed receive nano, when scalar continuity was verified.
    pub min_receive_nano: Option<i64>,
    /// Maximum observed receive nano, when scalar continuity was verified.
    pub max_receive_nano: Option<i64>,
    /// Observed order record count, when scalar continuity was verified.
    pub order_record_count: Option<u64>,
    /// Observed trade record count, when scalar continuity was verified.
    pub trade_record_count: Option<u64>,
    /// Number of required physical-v2 columns certified.
    pub required_column_count: usize,
    /// Number of projected column chunks certified.
    pub selected_column_chunk_count: u64,
    /// Sum of projected compressed chunk bytes.
    pub selected_compressed_bytes: u64,
    /// Sum of projected uncompressed chunk bytes.
    pub selected_uncompressed_bytes: u64,
    /// Reconstructed legacy payload FNV-1a64 hash, when requested.
    pub legacy_payload_hash_fnv1a64: Option<String>,
    /// Whether at least one optional manifest hash/fingerprint check was present
    /// and passed.
    pub checksum_verified: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelPayloadScanState {
    rows: u64,
    order_rows: u64,
    trade_rows: u64,
    first_biz_index: Option<u64>,
    last_biz_index: Option<u64>,
    min_receive_nano: Option<i64>,
    max_receive_nano: Option<i64>,
}

#[derive(Debug, Clone, Copy, Default)]
struct PhysicalV2ScanState {
    rows: u64,
    order_rows: u64,
    trade_rows: u64,
    first_biz_index: Option<u64>,
    last_biz_index: Option<u64>,
    next_biz_index: Option<u64>,
    min_receive_nano: Option<i64>,
    max_receive_nano: Option<i64>,
}

/// Certify a channel-sharded compact-L2 artifact manifest.
///
/// The helper validates manifest schema/count/path invariants, confines
/// manifest-relative artifact paths, opens every listed OCB artifact through the
/// pure-Rust core reader, checks row/row-group/fixed-binary width metadata, and
/// by default validates compact-L2 payload headers for trading-day, ChannelID,
/// strict gap-free BizIndex continuity, record kind, receive-nano edges, and
/// optional manifest hashes/fingerprints.
pub fn certify_channel_sharded_artifact_v1(
    manifest_path: impl AsRef<Path>,
    options: &CertificationOptions,
) -> Result<CertificationReport> {
    let manifest_path = manifest_path.as_ref();
    let manifest = ChannelShardedManifestV1::from_path(manifest_path)?;
    if manifest.artifact_format != options.artifact_format {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsupportedFormat,
            "channel-sharded OCB artifact format does not match certification options",
        ));
    }
    if let Some(width) = manifest.payload_width_bytes {
        if width != options.expected_record_width {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "channel-sharded OCB manifest payload width mismatch: expected={} observed={width}",
                    options.expected_record_width
                ),
            ));
        }
    }
    let manifest_rows = manifest.channels.iter().try_fold(0u64, |acc, channel| {
        acc.checked_add(channel.row_count).ok_or_else(|| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                "channel-sharded OCB manifest row total overflows",
            )
        })
    })?;
    if let Some(max_rows) = options.max_rows {
        if manifest_rows > max_rows {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB manifest rows exceed max_rows: rows={manifest_rows} max_rows={max_rows}"
                ),
            ));
        }
    }

    let mut reports = Vec::with_capacity(manifest.channels.len());
    for channel in &manifest.channels {
        reports.push(certify_channel_artifact(
            manifest_path,
            &manifest,
            channel,
            options,
        )?);
    }

    let row_count = reports.iter().map(|report| report.row_count).sum();
    let row_group_count = reports
        .iter()
        .map(|report| u64::from(report.row_group_count))
        .sum();
    let safe_summary = SafeCertificationSummary {
        schema_version: manifest.schema_version,
        trading_day: manifest.trading_day,
        artifact_format: manifest.artifact_format.clone(),
        channel_count: reports.len(),
        row_count,
        row_group_count,
        certified: true,
        path_redacted: true,
    };
    Ok(CertificationReport {
        schema_version: manifest.schema_version,
        channel_count: reports.len(),
        row_count,
        row_group_count,
        failed_channel_count: 0,
        channel_reports: reports,
        safe_summary,
    })
}

/// Certify one compact-L2 physical-v2 OCB artifact.
///
/// This helper validates the promoted physical-v2 column set on an existing OCB
/// file and, when requested, reconstructs legacy 168-byte compact-L2 payloads
/// only in memory to verify compatibility hashes. It deliberately does not
/// certify a v2 manifest contract or downstream runtime readiness.
pub fn certify_compact_l2_physical_v2_artifact(
    artifact_path: impl AsRef<Path>,
    options: &CompactL2PhysicalV2CertificationOptions,
) -> Result<CompactL2PhysicalV2CertificationReport> {
    if let Some(expected) = &options.expected_legacy_payload_hash_fnv1a64 {
        validate_hex_hash(expected, 16, OcbErrorKind::ChecksumMismatch)?;
    }
    let file = ColumnBundleFile::open(artifact_path.as_ref())?;
    let metadata = file.metadata()?;
    if let Some(expected) = options.expected_row_count {
        if metadata.row_count != expected {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "compact-L2 physical-v2 row count mismatch: expected={} observed={}",
                    expected, metadata.row_count
                ),
            ));
        }
    }
    if let Some(max_rows) = options.max_rows {
        if metadata.row_count > max_rows {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "compact-L2 physical-v2 rows exceed max_rows: rows={} max_rows={}",
                    metadata.row_count, max_rows
                ),
            ));
        }
    }

    let required_columns = compact_l2_physical_v2_required_columns();
    let request = ColumnBundleReadRequest {
        projection: ColumnProjection::names(required_columns.iter().copied()),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: options.read_threads.max(1),
            validate_checksums: true,
            decode_dictionaries: false,
        },
    };
    let plan = file.plan_read(&request)?;
    let plan_certification = file.read_plan_certification(&plan)?;
    certify_physical_v2_row_group_summaries(
        &plan_certification.row_groups,
        required_columns.len(),
    )?;

    let mut scan_state = PhysicalV2ScanState::default();
    let mut legacy_hash = options
        .verify_legacy_reconstruction
        .then(CompactL2Fnv1a64::new);
    let row_group_ids = plan.row_group_ids.clone();
    for wave in row_group_ids.chunks(options.max_in_flight_row_groups.max(1)) {
        let outcome = file.read_plan_row_groups(&plan, wave)?;
        for batch in &outcome.batches {
            let view = CompactL2PhysicalV2BatchView::from_column_batch(batch)?;
            if options.verify_scalar_continuity {
                scan_compact_l2_physical_v2_batch(&view, options, &mut scan_state)?;
            } else {
                scan_state.rows =
                    scan_state
                        .rows
                        .checked_add(batch.row_count)
                        .ok_or_else(|| {
                            ArcadiaTioError::ocb_diagnostic(
                                OcbErrorKind::RowCountMismatch,
                                "compact-L2 physical-v2 row count overflows",
                            )
                        })?;
            }
            if let Some(hash) = &mut legacy_hash {
                let mut payload = Vec::with_capacity(
                    view.row_count * COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize,
                );
                view.append_fixed_binary_v1_payloads(&mut payload)?;
                hash.update(&payload);
            }
        }
    }

    if scan_state.rows != metadata.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "compact-L2 physical-v2 decoded row count mismatch: expected={} observed={}",
                metadata.row_count, scan_state.rows
            ),
        ));
    }
    if let Some(expected) = options.expected_first_biz_index {
        if scan_state.first_biz_index != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::BizIndexGap,
                format!(
                    "compact-L2 physical-v2 first BizIndex mismatch: expected={} observed={:?}",
                    expected, scan_state.first_biz_index
                ),
            ));
        }
    }
    if let Some(expected) = options.expected_last_biz_index {
        if scan_state.last_biz_index != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::BizIndexGap,
                format!(
                    "compact-L2 physical-v2 last BizIndex mismatch: expected={} observed={:?}",
                    expected, scan_state.last_biz_index
                ),
            ));
        }
    }

    let legacy_payload_hash_fnv1a64 = legacy_hash.map(|hash| hash.finish_hex());
    let legacy_payload_hash_verified = match (
        &legacy_payload_hash_fnv1a64,
        &options.expected_legacy_payload_hash_fnv1a64,
    ) {
        (Some(actual), Some(expected)) => {
            if !actual.eq_ignore_ascii_case(expected) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChecksumMismatch,
                    "compact-L2 physical-v2 reconstructed legacy payload hash mismatch",
                ));
            }
            true
        }
        _ => false,
    };

    Ok(CompactL2PhysicalV2CertificationReport {
        row_count: metadata.row_count,
        row_group_count: metadata.row_group_count,
        required_column_count: required_columns.len(),
        selected_column_chunk_count: plan_certification
            .row_groups
            .iter()
            .map(|row_group| row_group.chunks.len() as u64)
            .sum(),
        selected_compressed_bytes: plan_certification.selected_compressed_bytes,
        selected_uncompressed_bytes: plan_certification.selected_uncompressed_bytes,
        first_biz_index: options
            .verify_scalar_continuity
            .then_some(scan_state.first_biz_index)
            .flatten(),
        last_biz_index: options
            .verify_scalar_continuity
            .then_some(scan_state.last_biz_index)
            .flatten(),
        min_receive_nano: options
            .verify_scalar_continuity
            .then_some(scan_state.min_receive_nano)
            .flatten(),
        max_receive_nano: options
            .verify_scalar_continuity
            .then_some(scan_state.max_receive_nano)
            .flatten(),
        order_record_count: options
            .verify_scalar_continuity
            .then_some(scan_state.order_rows),
        trade_record_count: options
            .verify_scalar_continuity
            .then_some(scan_state.trade_rows),
        legacy_payload_hash_fnv1a64,
        legacy_payload_hash_verified,
        path_redacted: true,
        writes_transformed_artifacts: false,
    })
}

/// Certify a channel-sharded compact-L2 physical-v2 artifact manifest.
///
/// This helper is the manifest-level readiness gate for the promoted
/// physical-v2 column contract. It keeps the v1 fixed-binary certifier
/// unchanged, confines manifest-relative artifact paths, opens each v2 OCB
/// artifact through the core reader, validates the required physical-v2 columns,
/// and optionally reconstructs legacy 168-byte payloads only in memory.
pub fn certify_compact_l2_physical_v2_manifest(
    manifest_path: impl AsRef<Path>,
    options: &CompactL2PhysicalV2ManifestCertificationOptions,
) -> Result<CompactL2PhysicalV2ManifestCertificationReport> {
    let manifest_path = manifest_path.as_ref();
    let manifest = ChannelShardedManifestV1::from_path(manifest_path)?;
    if manifest.artifact_format != options.artifact_format {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsupportedFormat,
            "compact-L2 physical-v2 manifest artifact format does not match certification options",
        ));
    }
    if let Some(width) = manifest.payload_width_bytes {
        if width != COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "compact-L2 physical-v2 manifest legacy payload width mismatch: expected={} observed={width}",
                    COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1
                ),
            ));
        }
    }
    let manifest_rows = manifest.channels.iter().try_fold(0u64, |acc, channel| {
        acc.checked_add(channel.row_count).ok_or_else(|| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                "compact-L2 physical-v2 manifest row total overflows",
            )
        })
    })?;
    if let Some(max_rows) = options.max_rows {
        if manifest_rows > max_rows {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "compact-L2 physical-v2 manifest rows exceed max_rows: rows={manifest_rows} max_rows={max_rows}"
                ),
            ));
        }
    }

    let mut reports = Vec::with_capacity(manifest.channels.len());
    for channel in &manifest.channels {
        reports.push(certify_physical_v2_channel_artifact(
            manifest_path,
            &manifest,
            channel,
            options,
        )?);
    }

    let row_count = reports.iter().map(|report| report.row_count).sum();
    let row_group_count = reports
        .iter()
        .map(|report| u64::from(report.row_group_count))
        .sum();
    let safe_summary = SafeCertificationSummary {
        schema_version: manifest.schema_version,
        trading_day: manifest.trading_day,
        artifact_format: manifest.artifact_format.clone(),
        channel_count: reports.len(),
        row_count,
        row_group_count,
        certified: true,
        path_redacted: true,
    };
    Ok(CompactL2PhysicalV2ManifestCertificationReport {
        schema_version: manifest.schema_version,
        channel_count: reports.len(),
        row_count,
        row_group_count,
        failed_channel_count: 0,
        channel_reports: reports,
        safe_summary,
    })
}

fn certify_channel_artifact(
    manifest_path: &Path,
    manifest: &ChannelShardedManifestV1,
    channel: &ChannelArtifactEntryV1,
    options: &CertificationOptions,
) -> Result<ChannelCertificationReport> {
    let artifact_canonical = canonicalize_manifest_artifact_path(manifest_path, channel)?;

    let checksum_verified = if options.verify_hashes {
        verify_optional_hashes(&artifact_canonical, channel)?
    } else {
        false
    };

    let file = ColumnBundleFile::open(&artifact_canonical)?;
    let metadata = file.metadata()?;
    if metadata.row_count != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB metadata row count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_count, metadata.row_count
            ),
        ));
    }
    if channel.row_group_count != 0 && metadata.row_group_count != channel.row_group_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB row-group count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_group_count, metadata.row_group_count
            ),
        ));
    }

    let request = ColumnBundleReadRequest {
        projection: ColumnProjection::names([options.payload_column_name.as_str()]),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: options.read_threads.max(1),
            validate_checksums: true,
            decode_dictionaries: false,
        },
    };
    let plan = file.plan_read(&request)?;
    let row_group_summaries = file.read_plan_row_group_summaries(&plan)?;
    let mut summary_rows = 0u64;
    for row_group in &row_group_summaries {
        summary_rows = summary_rows
            .checked_add(row_group.row_count)
            .ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row-group row total overflows",
                )
            })?;
        if row_group.chunks.len() != 1 {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::InvalidManifest,
                format!(
                    "channel-sharded OCB row group projected unexpected chunk count: channel={} row_group={} chunks={}",
                    channel.channel_id,
                    row_group.row_group_id,
                    row_group.chunks.len()
                ),
            ));
        }
        let chunk = &row_group.chunks[0];
        if chunk.physical_type
            != (ColumnPhysicalType::FixedBinary {
                width: options.expected_record_width,
            })
            || chunk.fixed_binary_width != Some(options.expected_record_width)
        {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "channel-sharded OCB fixed-binary width mismatch: channel={} row_group={} expected={} observed={}",
                    channel.channel_id,
                    row_group.row_group_id,
                    options.expected_record_width,
                    chunk.fixed_binary_width.unwrap_or(0)
                ),
            ));
        }
        if chunk.validity_ref.is_some() {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB payload chunk must be non-nullable: channel={} row_group={}",
                    channel.channel_id, row_group.row_group_id
                ),
            ));
        }
    }
    if summary_rows != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB row-group rows mismatch: channel={} expected={} observed={summary_rows}",
                channel.channel_id, channel.row_count
            ),
        ));
    }

    let scan_state = if options.verify_payload_header {
        scan_payload_headers(&file, &plan, channel, manifest.trading_day, options)?
    } else {
        ChannelPayloadScanState {
            rows: channel.row_count,
            first_biz_index: Some(channel.first_biz_index),
            last_biz_index: Some(channel.last_biz_index),
            ..ChannelPayloadScanState::default()
        }
    };

    if scan_state.rows != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB decoded row count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_count, scan_state.rows
            ),
        ));
    }
    if scan_state.first_biz_index != Some(channel.first_biz_index)
        || scan_state.last_biz_index != Some(channel.last_biz_index)
    {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::BizIndexGap,
            format!(
                "channel-sharded OCB decoded BizIndex range mismatch: channel={} expected={}..={} observed={:?}..{:?}",
                channel.channel_id,
                channel.first_biz_index,
                channel.last_biz_index,
                scan_state.first_biz_index,
                scan_state.last_biz_index
            ),
        ));
    }
    if let Some(expected) = channel.min_receive_nano {
        if scan_state.min_receive_nano != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB first receive_nano mismatch: channel={} expected={} observed={:?}",
                    channel.channel_id, expected, scan_state.min_receive_nano
                ),
            ));
        }
    }
    if let Some(expected) = channel.max_receive_nano {
        if scan_state.max_receive_nano != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB last receive_nano mismatch: channel={} expected={} observed={:?}",
                    channel.channel_id, expected, scan_state.max_receive_nano
                ),
            ));
        }
    }
    if let Some(expected) = channel.order_record_count {
        if scan_state.order_rows != expected {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB order count mismatch: channel={} expected={} observed={}",
                    channel.channel_id, expected, scan_state.order_rows
                ),
            ));
        }
    }
    if let Some(expected) = channel.trade_record_count {
        if scan_state.trade_rows != expected {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB trade count mismatch: channel={} expected={} observed={}",
                    channel.channel_id, expected, scan_state.trade_rows
                ),
            ));
        }
    }

    Ok(ChannelCertificationReport {
        channel_id: channel.channel_id,
        row_count: channel.row_count,
        row_group_count: metadata.row_group_count,
        first_biz_index: channel.first_biz_index,
        last_biz_index: channel.last_biz_index,
        min_receive_nano: scan_state.min_receive_nano.or(channel.min_receive_nano),
        max_receive_nano: scan_state.max_receive_nano.or(channel.max_receive_nano),
        order_record_count: options
            .verify_payload_header
            .then_some(scan_state.order_rows),
        trade_record_count: options
            .verify_payload_header
            .then_some(scan_state.trade_rows),
        checksum_verified,
    })
}

fn certify_physical_v2_channel_artifact(
    manifest_path: &Path,
    manifest: &ChannelShardedManifestV1,
    channel: &ChannelArtifactEntryV1,
    options: &CompactL2PhysicalV2ManifestCertificationOptions,
) -> Result<CompactL2PhysicalV2ChannelCertificationReport> {
    let artifact_canonical = canonicalize_manifest_artifact_path(manifest_path, channel)?;

    let checksum_verified = if options.verify_hashes {
        verify_optional_hashes(&artifact_canonical, channel)?
    } else {
        false
    };

    let artifact_report = certify_compact_l2_physical_v2_artifact(
        &artifact_canonical,
        &CompactL2PhysicalV2CertificationOptions {
            expected_row_count: Some(channel.row_count),
            expected_trading_day: options
                .verify_scalar_continuity
                .then_some(manifest.trading_day),
            expected_channel_id: options
                .verify_scalar_continuity
                .then_some(channel.channel_id),
            expected_first_biz_index: options
                .verify_scalar_continuity
                .then_some(channel.first_biz_index),
            expected_last_biz_index: options
                .verify_scalar_continuity
                .then_some(channel.last_biz_index),
            verify_scalar_continuity: options.verify_scalar_continuity,
            verify_legacy_reconstruction: options.verify_legacy_reconstruction,
            expected_legacy_payload_hash_fnv1a64: None,
            max_rows: Some(channel.row_count),
            read_threads: options.read_threads,
            max_in_flight_row_groups: options.max_in_flight_row_groups,
        },
    )?;

    if channel.row_group_count != 0 && artifact_report.row_group_count != channel.row_group_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "compact-L2 physical-v2 row-group count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_group_count, artifact_report.row_group_count
            ),
        ));
    }
    if options.verify_scalar_continuity {
        if let Some(expected) = channel.min_receive_nano {
            if artifact_report.min_receive_nano != Some(expected) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::PayloadHeaderMismatch,
                    format!(
                        "compact-L2 physical-v2 first receive_nano mismatch: channel={} expected={} observed={:?}",
                        channel.channel_id, expected, artifact_report.min_receive_nano
                    ),
                ));
            }
        }
        if let Some(expected) = channel.max_receive_nano {
            if artifact_report.max_receive_nano != Some(expected) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::PayloadHeaderMismatch,
                    format!(
                        "compact-L2 physical-v2 last receive_nano mismatch: channel={} expected={} observed={:?}",
                        channel.channel_id, expected, artifact_report.max_receive_nano
                    ),
                ));
            }
        }
        if let Some(expected) = channel.order_record_count {
            if artifact_report.order_record_count != Some(expected) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    format!(
                        "compact-L2 physical-v2 order count mismatch: channel={} expected={} observed={:?}",
                        channel.channel_id, expected, artifact_report.order_record_count
                    ),
                ));
            }
        }
        if let Some(expected) = channel.trade_record_count {
            if artifact_report.trade_record_count != Some(expected) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    format!(
                        "compact-L2 physical-v2 trade count mismatch: channel={} expected={} observed={:?}",
                        channel.channel_id, expected, artifact_report.trade_record_count
                    ),
                ));
            }
        }
    }

    Ok(CompactL2PhysicalV2ChannelCertificationReport {
        channel_id: channel.channel_id,
        row_count: artifact_report.row_count,
        row_group_count: artifact_report.row_group_count,
        first_biz_index: artifact_report.first_biz_index,
        last_biz_index: artifact_report.last_biz_index,
        min_receive_nano: artifact_report
            .min_receive_nano
            .or(channel.min_receive_nano),
        max_receive_nano: artifact_report
            .max_receive_nano
            .or(channel.max_receive_nano),
        order_record_count: artifact_report.order_record_count,
        trade_record_count: artifact_report.trade_record_count,
        required_column_count: artifact_report.required_column_count,
        selected_column_chunk_count: artifact_report.selected_column_chunk_count,
        selected_compressed_bytes: artifact_report.selected_compressed_bytes,
        selected_uncompressed_bytes: artifact_report.selected_uncompressed_bytes,
        legacy_payload_hash_fnv1a64: artifact_report.legacy_payload_hash_fnv1a64,
        checksum_verified,
    })
}

fn canonicalize_manifest_artifact_path(
    manifest_path: &Path,
    channel: &ChannelArtifactEntryV1,
) -> Result<std::path::PathBuf> {
    let artifact_path =
        resolve_manifest_relative_artifact_path(manifest_path, &channel.relative_path)?;
    if !artifact_path.exists() {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::MissingArtifact,
            format!(
                "channel-sharded OCB artifact is missing: channel={}",
                channel.channel_id
            ),
        ));
    }
    let root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root_canonical = fs::canonicalize(root).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB manifest root could not be canonicalized",
        )
    })?;
    let artifact_canonical = fs::canonicalize(&artifact_path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::MissingArtifact,
            format!(
                "channel-sharded OCB artifact could not be canonicalized: channel={}",
                channel.channel_id
            ),
        )
    })?;
    if !artifact_canonical.starts_with(&root_canonical) {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB artifact path escapes manifest root",
        ));
    }
    Ok(artifact_canonical)
}

fn scan_payload_headers(
    file: &ColumnBundleFile,
    plan: &crate::ColumnBundleReadPlan,
    channel: &ChannelArtifactEntryV1,
    trading_day: u32,
    options: &CertificationOptions,
) -> Result<ChannelPayloadScanState> {
    let row_group_ids = plan.row_group_ids.clone();
    let mut buffers =
        file.reusable_buffer_pool_for_plan(plan, options.max_in_flight_row_groups.max(1), false)?;
    let mut state = ChannelPayloadScanState::default();
    let mut expected_biz_index = channel.first_biz_index;
    file.visit_plan_row_groups_into_with_attribution(
        plan,
        &row_group_ids,
        ColumnBundleReadCursorOptions {
            max_in_flight_row_groups: options.max_in_flight_row_groups.max(1),
            ordered: true,
        },
        &mut buffers,
        |batch| {
            let column = batch.column(0)?;
            let (width, bytes) = match column.values {
                PrimitiveColumnValuesRef::FixedBinary { width, bytes } => (width, bytes),
                _ => {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::FixedBinaryWidthMismatch,
                        "channel-sharded OCB payload column is not fixed-binary",
                    ));
                }
            };
            if width != options.expected_record_width {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::FixedBinaryWidthMismatch,
                    format!(
                        "channel-sharded OCB payload width mismatch: channel={} row_group={} expected={} observed={width}",
                        channel.channel_id,
                        batch.row_group_id(),
                        options.expected_record_width
                    ),
                ));
            }
            let row_count = usize::try_from(batch.row_count()).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row group row count exceeds usize",
                )
            })?;
            let expected_bytes = row_count.checked_mul(width as usize).ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row group payload byte count overflows",
                )
            })?;
            if bytes.len() < expected_bytes {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    format!(
                        "channel-sharded OCB row group payload byte count mismatch: channel={} row_group={}",
                        channel.channel_id,
                        batch.row_group_id()
                    ),
                ));
            }
            for record in bytes[..expected_bytes].chunks_exact(width as usize) {
                let header = decode_compact_l2_fixed_binary_header_v1(record)?;
                if header.trading_day != trading_day {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::PayloadHeaderMismatch,
                        format!(
                            "channel-sharded OCB trading day mismatch: channel={} expected={} observed={}",
                            channel.channel_id, trading_day, header.trading_day
                        ),
                    ));
                }
                if u32::try_from(header.channel_id).ok() != Some(channel.channel_id) {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::ChannelIdMismatch,
                        format!(
                            "channel-sharded OCB ChannelID mismatch: expected={} observed={}",
                            channel.channel_id, header.channel_id
                        ),
                    ));
                }
                let biz_index = u64::try_from(header.biz_index).map_err(|_| {
                    ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::PayloadHeaderMismatch,
                        "channel-sharded OCB BizIndex is negative",
                    )
                })?;
                if biz_index < expected_biz_index {
                    let kind = if expected_biz_index.saturating_sub(biz_index) == 1 {
                        OcbErrorKind::BizIndexDuplicate
                    } else {
                        OcbErrorKind::BizIndexRegression
                    };
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        kind,
                        format!(
                            "channel-sharded OCB BizIndex regressed: channel={} expected={} observed={biz_index}",
                            channel.channel_id, expected_biz_index
                        ),
                    ));
                }
                if biz_index > expected_biz_index {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        format!(
                            "channel-sharded OCB BizIndex gap: channel={} expected={} observed={biz_index}",
                            channel.channel_id, expected_biz_index
                        ),
                    ));
                }
                if biz_index > channel.last_biz_index {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        format!(
                            "channel-sharded OCB BizIndex exceeds manifest range: channel={} last={} observed={biz_index}",
                            channel.channel_id, channel.last_biz_index
                        ),
                    ));
                }
                state.first_biz_index.get_or_insert(biz_index);
                state.last_biz_index = Some(biz_index);
                state.min_receive_nano = Some(match state.min_receive_nano {
                    Some(current) => current.min(header.receive_nano),
                    None => header.receive_nano,
                });
                state.max_receive_nano = Some(match state.max_receive_nano {
                    Some(current) => current.max(header.receive_nano),
                    None => header.receive_nano,
                });
                match header.record_kind {
                    CompactL2RecordKind::Order => state.order_rows = state.order_rows.saturating_add(1),
                    CompactL2RecordKind::Trade => state.trade_rows = state.trade_rows.saturating_add(1),
                }
                state.rows = state.rows.saturating_add(1);
                expected_biz_index = expected_biz_index.checked_add(1).ok_or_else(|| {
                    ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        "channel-sharded OCB BizIndex sequence overflows",
                    )
                })?;
            }
            Ok(ColumnBundleVisitControl::Continue)
        },
    )?;
    if expected_biz_index != channel.last_biz_index.saturating_add(1) {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::BizIndexGap,
            format!(
                "channel-sharded OCB final BizIndex mismatch: channel={} expected_next={} manifest_last={}",
                channel.channel_id, expected_biz_index, channel.last_biz_index
            ),
        ));
    }
    Ok(state)
}

fn compact_l2_physical_v2_required_columns() -> Vec<&'static str> {
    let mut columns = vec![
        COMPACT_L2_DAY_KEY_COLUMN_NAME,
        COMPACT_L2_CHANNEL_ID_COLUMN_NAME,
        COMPACT_L2_BIZ_INDEX_COLUMN_NAME,
        COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
        COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME,
        COMPACT_L2_RECORD_KIND_COLUMN_NAME,
        COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
        COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
        COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME,
        COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
    ];
    columns.extend(
        COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS
            .iter()
            .map(|column| column.column_name),
    );
    columns
}

fn certify_physical_v2_row_group_summaries(
    row_groups: &[ColumnBundleRowGroupSummary],
    expected_column_count: usize,
) -> Result<()> {
    for row_group in row_groups {
        if row_group.chunks.len() != expected_column_count {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::InvalidManifest,
                format!(
                    "compact-L2 physical-v2 row group projected unexpected chunk count: row_group={} expected={} observed={}",
                    row_group.row_group_id,
                    expected_column_count,
                    row_group.chunks.len()
                ),
            ));
        }
        for chunk in &row_group.chunks {
            if chunk.row_count != row_group.row_count {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    format!(
                        "compact-L2 physical-v2 chunk row count mismatch: row_group={} column={}",
                        row_group.row_group_id, chunk.column_name
                    ),
                ));
            }
            if chunk.validity_ref.is_some() {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::PayloadHeaderMismatch,
                    format!(
                        "compact-L2 physical-v2 column must be non-nullable: row_group={} column={}",
                        row_group.row_group_id, chunk.column_name
                    ),
                ));
            }
            certify_physical_v2_chunk_type(
                &chunk.column_name,
                chunk.physical_type,
                chunk.fixed_binary_width,
            )?;
        }
    }
    Ok(())
}

fn certify_physical_v2_chunk_type(
    column_name: &str,
    physical_type: ColumnPhysicalType,
    fixed_binary_width: Option<u32>,
) -> Result<()> {
    let expected = compact_l2_physical_v2_expected_column_type(column_name).ok_or_else(|| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::InvalidManifest,
            format!("compact-L2 physical-v2 unexpected column {column_name:?}"),
        )
    })?;
    if physical_type != expected {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::FixedBinaryWidthMismatch,
            format!("compact-L2 physical-v2 column type mismatch: column={column_name:?}"),
        ));
    }
    match expected {
        ColumnPhysicalType::FixedBinary { width } if fixed_binary_width != Some(width) => {
            Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "compact-L2 physical-v2 fixed-binary width mismatch: column={column_name:?} expected={width} observed={}",
                    fixed_binary_width.unwrap_or(0)
                ),
            ))
        }
        _ => Ok(()),
    }
}

fn compact_l2_physical_v2_expected_column_type(column_name: &str) -> Option<ColumnPhysicalType> {
    match column_name {
        COMPACT_L2_DAY_KEY_COLUMN_NAME | COMPACT_L2_RECORD_KIND_COLUMN_NAME => {
            Some(ColumnPhysicalType::I32)
        }
        COMPACT_L2_CHANNEL_ID_COLUMN_NAME
        | COMPACT_L2_BIZ_INDEX_COLUMN_NAME
        | COMPACT_L2_RECEIVE_NANO_COLUMN_NAME
        | COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME
        | COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME => Some(ColumnPhysicalType::I64),
        COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME => {
            Some(ColumnPhysicalType::FixedBinary { width: 2 })
        }
        COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME => {
            Some(ColumnPhysicalType::FixedBinary { width: 9 })
        }
        COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME => {
            Some(ColumnPhysicalType::FixedBinary { width: 7 })
        }
        other => COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS
            .iter()
            .any(|column| column.column_name == other)
            .then_some(ColumnPhysicalType::I64),
    }
}

fn scan_compact_l2_physical_v2_batch(
    view: &CompactL2PhysicalV2BatchView<'_>,
    options: &CompactL2PhysicalV2CertificationOptions,
    state: &mut PhysicalV2ScanState,
) -> Result<()> {
    view.validate()?;
    for row in 0..view.row_count {
        let day_key = u32::try_from(view.day_key[row]).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                "compact-L2 physical-v2 day_key is negative",
            )
        })?;
        if let Some(expected) = options.expected_trading_day {
            if day_key != expected {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::PayloadHeaderMismatch,
                    format!(
                        "compact-L2 physical-v2 trading day mismatch: expected={} observed={}",
                        expected, day_key
                    ),
                ));
            }
        }
        let channel_id = u32::try_from(view.channel_id[row]).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::ChannelIdMismatch,
                "compact-L2 physical-v2 channel_id is invalid",
            )
        })?;
        if let Some(expected) = options.expected_channel_id {
            if channel_id != expected {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChannelIdMismatch,
                    format!(
                        "compact-L2 physical-v2 ChannelID mismatch: expected={} observed={}",
                        expected, channel_id
                    ),
                ));
            }
        }
        let biz_index = u64::try_from(view.biz_index[row]).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                "compact-L2 physical-v2 BizIndex is negative",
            )
        })?;
        let expected_biz_index = match state.next_biz_index {
            Some(value) => value,
            None => options.expected_first_biz_index.unwrap_or(biz_index),
        };
        if biz_index < expected_biz_index {
            let kind = if expected_biz_index.saturating_sub(biz_index) == 1 {
                OcbErrorKind::BizIndexDuplicate
            } else {
                OcbErrorKind::BizIndexRegression
            };
            return Err(ArcadiaTioError::ocb_diagnostic(
                kind,
                format!(
                    "compact-L2 physical-v2 BizIndex regressed: expected={} observed={}",
                    expected_biz_index, biz_index
                ),
            ));
        }
        if biz_index > expected_biz_index {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::BizIndexGap,
                format!(
                    "compact-L2 physical-v2 BizIndex gap: expected={} observed={}",
                    expected_biz_index, biz_index
                ),
            ));
        }
        state.first_biz_index.get_or_insert(biz_index);
        state.last_biz_index = Some(biz_index);
        state.next_biz_index = Some(biz_index.checked_add(1).ok_or_else(|| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::BizIndexGap,
                "compact-L2 physical-v2 BizIndex sequence overflows",
            )
        })?);
        state.min_receive_nano = Some(match state.min_receive_nano {
            Some(current) => current.min(view.receive_nano[row]),
            None => view.receive_nano[row],
        });
        state.max_receive_nano = Some(match state.max_receive_nano {
            Some(current) => current.max(view.receive_nano[row]),
            None => view.receive_nano[row],
        });
        match u8::try_from(view.record_kind[row]).ok() {
            Some(COMPACT_L2_RECORD_KIND_ORDER) => {
                state.order_rows = state.order_rows.saturating_add(1)
            }
            Some(COMPACT_L2_RECORD_KIND_TRADE) => {
                state.trade_rows = state.trade_rows.saturating_add(1)
            }
            _ => {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::PayloadHeaderMismatch,
                    "compact-L2 physical-v2 record_kind is invalid",
                ));
            }
        }
        state.rows = state.rows.checked_add(1).ok_or_else(|| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                "compact-L2 physical-v2 row count overflows",
            )
        })?;
    }
    Ok(())
}

fn verify_optional_hashes(path: &Path, channel: &ChannelArtifactEntryV1) -> Result<bool> {
    let mut verified = false;
    if let Some(expected) = &channel.payload_sha256 {
        validate_hex_hash(expected, 64, OcbErrorKind::ChecksumMismatch)?;
        let actual = sha256_file_hex(path)?;
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::ChecksumMismatch,
                format!(
                    "channel-sharded OCB SHA-256 mismatch: channel={}",
                    channel.channel_id
                ),
            ));
        }
        verified = true;
    }
    if let Some(fingerprint) = &channel.fingerprint {
        if let Some(expected_bytes) = fingerprint.file_bytes {
            let actual_bytes = fs::metadata(path)
                .map_err(|_| {
                    ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "stat artifact failed")
                })?
                .len();
            if actual_bytes != expected_bytes {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChecksumMismatch,
                    format!(
                        "channel-sharded OCB fingerprint file size mismatch: channel={}",
                        channel.channel_id
                    ),
                ));
            }
            verified = true;
        }
        if let Some(expected) = &fingerprint.content_hash_fnv1a64 {
            validate_hex_hash(expected, 16, OcbErrorKind::ChecksumMismatch)?;
            let actual = fnv1a64_file_hex(path)?;
            if !expected.eq_ignore_ascii_case(&actual) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChecksumMismatch,
                    format!(
                        "channel-sharded OCB FNV-1a64 mismatch: channel={}",
                        channel.channel_id
                    ),
                ));
            }
            verified = true;
        }
    }
    Ok(verified)
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "open artifact for sha256 failed")
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "read artifact for sha256 failed")
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_bytes(&hasher.finalize()))
}

fn fnv1a64_file_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "open artifact for fnv failed")
    })?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "read artifact for fnv failed")
        })?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

#[derive(Debug, Clone, Copy)]
struct CompactL2Fnv1a64 {
    value: u64,
}

impl CompactL2Fnv1a64 {
    fn new() -> Self {
        Self {
            value: 0xcbf2_9ce4_8422_2325_u64,
        }
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.value ^= u64::from(*byte);
            self.value = self.value.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }

    fn finish_hex(self) -> String {
        format!("{:016x}", self.value)
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::*;
    use crate::compact_l2::{
        COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1, COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
        COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT, COMPACT_L2_RECORD_KIND_ORDER,
        COMPACT_L2_RECORD_KIND_TRADE, CompactL2PhysicalV2Record,
    };
    use crate::format::{
        OCB_BOOTSTRAP_PAGE_V1_LEN, OCB_NULL_U32, OCB_ROOT_V1_LEN, OcbBodyKindV1, OcbBodyRefV2,
        OcbBootstrapPageV1, OcbChunkCodecV1, OcbColumnChunkDescV1, OcbColumnChunkObjectV1,
        OcbColumnDescV1, OcbLogicalKindV1, OcbNullabilityV1, OcbPhysicalTypeV1, OcbRootV1,
        OcbRowGroupDescV1, OcbRowGroupIndexV1, OcbSchemaV1, OcbStringTableV1, crc32c,
    };

    #[test]
    fn valid_channel_sharded_fixture_certifies_path_free_summary() {
        let root = fixture_root("valid_channel_sharded_fixture_certifies");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 2, 3],
            &[
                COMPACT_L2_RECORD_KIND_ORDER,
                COMPACT_L2_RECORD_KIND_TRADE,
                COMPACT_L2_RECORD_KIND_ORDER,
            ],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 3, 2, 1, 2, 1);

        let report =
            certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
                .expect("valid fixture certifies");
        assert_eq!(report.channel_count, 1);
        assert_eq!(report.row_count, 3);
        assert_eq!(report.row_group_count, 2);
        assert_eq!(report.safe_summary.trading_day, 20260702);
        assert!(report.safe_summary.path_redacted);
        assert!(!format!("{:?}", report.safe_summary).contains(&root.display().to_string()));
        cleanup_root(&root);
    }

    #[test]
    fn physical_v2_artifact_certifies_schema_continuity_and_legacy_hash() {
        let root = fixture_root("physical_v2_artifact_certifies");
        let artifact = root.join("channel_2011.typed-split-v2.ocb");
        let records = vec![
            compact_l2_record(20260702, 2011, 1, COMPACT_L2_RECORD_KIND_ORDER),
            compact_l2_record(20260702, 2011, 2, COMPACT_L2_RECORD_KIND_TRADE),
            compact_l2_record(20260702, 2011, 3, COMPACT_L2_RECORD_KIND_ORDER),
        ];
        write_compact_l2_physical_v2_fixture(&artifact, &records, None);
        let mut expected_hash = CompactL2Fnv1a64::new();
        for record in &records {
            expected_hash.update(record);
        }
        let expected_hash = expected_hash.finish_hex();

        let report = certify_compact_l2_physical_v2_artifact(
            &artifact,
            &CompactL2PhysicalV2CertificationOptions {
                expected_row_count: Some(3),
                expected_trading_day: Some(20260702),
                expected_channel_id: Some(2011),
                expected_first_biz_index: Some(1),
                expected_last_biz_index: Some(3),
                verify_legacy_reconstruction: true,
                expected_legacy_payload_hash_fnv1a64: Some(expected_hash.clone()),
                ..CompactL2PhysicalV2CertificationOptions::default()
            },
        )
        .expect("physical-v2 fixture certifies");

        assert_eq!(report.row_count, 3);
        assert_eq!(report.row_group_count, 2);
        assert_eq!(report.required_column_count, 20);
        assert_eq!(report.selected_column_chunk_count, 40);
        assert_eq!(report.first_biz_index, Some(1));
        assert_eq!(report.last_biz_index, Some(3));
        assert_eq!(report.order_record_count, Some(2));
        assert_eq!(report.trade_record_count, Some(1));
        assert_eq!(report.legacy_payload_hash_fnv1a64, Some(expected_hash));
        assert!(report.legacy_payload_hash_verified);
        assert!(report.path_redacted);
        assert!(!report.writes_transformed_artifacts);
        cleanup_root(&root);
    }

    #[test]
    fn physical_v2_artifact_rejects_wrong_lane_width() {
        let root = fixture_root("physical_v2_artifact_rejects_wrong_lane_width");
        let artifact = root.join("channel_2011.typed-split-v2.ocb");
        let records = vec![compact_l2_record(
            20260702,
            2011,
            1,
            COMPACT_L2_RECORD_KIND_TRADE,
        )];
        write_compact_l2_physical_v2_fixture(&artifact, &records, Some(8));

        let err = certify_compact_l2_physical_v2_artifact(
            &artifact,
            &CompactL2PhysicalV2CertificationOptions::default(),
        )
        .expect_err("wrong symbol width should fail");
        assert_eq!(err.code(), crate::ArcadiaTioErrorCode::InvalidArgument);
        cleanup_root(&root);
    }

    #[test]
    fn physical_v2_cross_row_group_biz_index_failures_fail_closed() {
        for (case, biz_indexes, expected_kind) in [
            ("gap", vec![1, 2, 4], OcbErrorKind::BizIndexGap),
            ("duplicate", vec![1, 2, 2], OcbErrorKind::BizIndexDuplicate),
        ] {
            let root = fixture_root(&format!("physical_v2_cross_row_group_{case}"));
            let artifact = root.join("channel_2011.typed-split-v2.ocb");
            let records = biz_indexes
                .into_iter()
                .enumerate()
                .map(|(ordinal, biz_index)| {
                    compact_l2_record(
                        20260702,
                        2011,
                        biz_index,
                        if ordinal % 2 == 0 {
                            COMPACT_L2_RECORD_KIND_ORDER
                        } else {
                            COMPACT_L2_RECORD_KIND_TRADE
                        },
                    )
                })
                .collect::<Vec<_>>();
            // The fixture writer uses two rows per row group, so the invalid
            // third row exercises continuity across the row-group boundary.
            write_compact_l2_physical_v2_fixture(&artifact, &records, None);

            let error = certify_compact_l2_physical_v2_artifact(
                &artifact,
                &CompactL2PhysicalV2CertificationOptions::default(),
            )
            .expect_err("cross-row-group BizIndex failure must fail closed");
            assert_eq!(OcbErrorKind::from_error(&error), Some(expected_kind));
            cleanup_root(&root);
        }
    }

    #[test]
    fn physical_v2_manifest_certifies_channel_artifact_set() {
        let root = fixture_root("physical_v2_manifest_certifies");
        let artifact = root.join("channels/2011/l2_mutations.physical-v2.ocb");
        let records = vec![
            compact_l2_record(20260702, 2011, 1, COMPACT_L2_RECORD_KIND_ORDER),
            compact_l2_record(20260702, 2011, 2, COMPACT_L2_RECORD_KIND_TRADE),
            compact_l2_record(20260702, 2011, 3, COMPACT_L2_RECORD_KIND_ORDER),
        ];
        write_compact_l2_physical_v2_fixture(&artifact, &records, None);
        let manifest = root.join("manifest.json");
        write_physical_v2_manifest(&manifest, 20260702, 2011, 3, 2, 1, 2, 1);

        let manifest_model =
            ChannelShardedManifestV1::from_path(&manifest).expect("v2 manifest parses");
        assert_eq!(
            manifest_model.artifact_format,
            COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT
        );

        let report = certify_compact_l2_physical_v2_manifest(
            &manifest,
            &CompactL2PhysicalV2ManifestCertificationOptions {
                verify_legacy_reconstruction: true,
                ..CompactL2PhysicalV2ManifestCertificationOptions::default()
            },
        )
        .expect("physical-v2 manifest certifies");

        assert_eq!(report.channel_count, 1);
        assert_eq!(report.row_count, 3);
        assert_eq!(report.row_group_count, 2);
        assert_eq!(
            report.safe_summary.artifact_format,
            COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT
        );
        assert!(report.safe_summary.path_redacted);
        assert!(!format!("{:?}", report.safe_summary).contains(&root.display().to_string()));
        let channel = &report.channel_reports[0];
        assert_eq!(channel.channel_id, 2011);
        assert_eq!(channel.first_biz_index, Some(1));
        assert_eq!(channel.last_biz_index, Some(3));
        assert_eq!(channel.order_record_count, Some(2));
        assert_eq!(channel.trade_record_count, Some(1));
        assert_eq!(channel.required_column_count, 20);
        assert_eq!(channel.selected_column_chunk_count, 40);
        assert!(channel.legacy_payload_hash_fnv1a64.is_some());
        cleanup_root(&root);
    }

    #[test]
    fn fixed_binary_v1_certifier_rejects_physical_v2_manifest() {
        let root = fixture_root("fixed_binary_v1_certifier_rejects_physical_v2_manifest");
        let artifact = root.join("channels/2011/l2_mutations.physical-v2.ocb");
        let records = vec![compact_l2_record(
            20260702,
            2011,
            1,
            COMPACT_L2_RECORD_KIND_ORDER,
        )];
        write_compact_l2_physical_v2_fixture(&artifact, &records, None);
        let manifest = root.join("manifest.json");
        write_physical_v2_manifest(&manifest, 20260702, 2011, 1, 1, 1, 1, 0);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("v1 certifier must not accept physical-v2 manifests");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::UnsupportedFormat)
        );
        cleanup_root(&root);
    }

    #[test]
    fn biz_index_gap_fixture_fails_with_expected_kind() {
        let root = fixture_root("biz_index_gap_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 3],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("gap fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::BizIndexGap)
        );
        cleanup_root(&root);
    }

    #[test]
    fn biz_index_duplicate_fixture_fails_with_expected_kind() {
        let root = fixture_root("biz_index_duplicate_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 1],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("duplicate fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::BizIndexDuplicate)
        );
        cleanup_root(&root);
    }

    #[test]
    fn channel_id_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("channel_id_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 2],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            Some(2012),
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("channel mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::ChannelIdMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn record_width_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("record_width_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1],
            &[COMPACT_L2_RECORD_KIND_ORDER],
            16,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 1, 1, 1, 1, 0);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("width mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::FixedBinaryWidthMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn checksum_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("checksum_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1],
            &[COMPACT_L2_RECORD_KIND_ORDER],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest_with_payload_sha256(
            &manifest,
            20260702,
            2011,
            1,
            1,
            1,
            1,
            0,
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        );

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("checksum mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::ChecksumMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn unsafe_manifest_relative_path_fails_with_expected_kind() {
        let root = fixture_root("unsafe_manifest_path_fixture");
        let manifest = root.join("manifest.json");
        let manifest_json = json!({
            "schema_version": 1,
            "trading_day": 20260702,
            "artifact_format": COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
            "payload_width_bytes": COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            "channel_indivisible": true,
            "channels": [{
                "channel_id": 2011,
                "relative_path": "../escape.ocb",
                "row_count": 1,
                "row_group_count": 1,
                "first_biz_index": 1,
                "last_biz_index": 1
            }]
        });
        fs::write(
            &manifest,
            serde_json::to_vec_pretty(&manifest_json).unwrap(),
        )
        .unwrap();
        let err = ChannelShardedManifestV1::from_path(&manifest).expect_err("unsafe path rejected");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::UnsafeManifestPath)
        );
        cleanup_root(&root);
    }

    fn fixture_root(name: &str) -> PathBuf {
        let root =
            PathBuf::from(".tmp/ocb-cert-tests").join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn cleanup_root(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    fn write_manifest(
        manifest: &Path,
        trading_day: u32,
        channel_id: u32,
        rows: u64,
        row_groups: u32,
        first_biz: u64,
        orders: u64,
        trades: u64,
    ) {
        let parent = manifest.parent().expect("manifest parent");
        fs::create_dir_all(parent).expect("create manifest parent");
        write_manifest_with_payload_sha256(
            manifest,
            trading_day,
            channel_id,
            rows,
            row_groups,
            first_biz,
            orders,
            trades,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_manifest_with_payload_sha256(
        manifest: &Path,
        trading_day: u32,
        channel_id: u32,
        rows: u64,
        row_groups: u32,
        first_biz: u64,
        orders: u64,
        trades: u64,
        payload_sha256: Option<&str>,
    ) {
        let parent = manifest.parent().expect("manifest parent");
        fs::create_dir_all(parent).expect("create manifest parent");
        let last_biz = first_biz + rows - 1;
        let mut channel_entry = json!({
            "channel_id": channel_id,
            "relative_path": format!("channels/{channel_id}/l2_mutations.ocb"),
            "row_count": rows,
            "row_group_count": row_groups,
            "first_biz_index": first_biz,
            "last_biz_index": last_biz,
            "min_receive_nano": 1_000_000_i64 + first_biz as i64,
            "max_receive_nano": 1_000_000_i64 + last_biz as i64,
            "order_record_count": orders,
            "trade_record_count": trades
        });
        if let Some(payload_sha256) = payload_sha256 {
            channel_entry["payload_sha256"] = json!(payload_sha256);
        }
        let manifest_json = json!({
            "schema_version": 1,
            "trading_day": trading_day,
            "artifact_format": COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
            "payload_width_bytes": COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            "selection_scope": "contiguous-prefix",
            "channel_indivisible": true,
            "counts": {"channels": 1, "row_count": rows, "order_records": orders, "trade_records": trades},
            "channels": [channel_entry],
            "claims": {"default_readiness": false, "runtime_readiness": false, "performance_dominance": false}
        });
        fs::write(manifest, serde_json::to_vec_pretty(&manifest_json).unwrap())
            .expect("write manifest");
    }

    #[allow(clippy::too_many_arguments)]
    fn write_physical_v2_manifest(
        manifest: &Path,
        trading_day: u32,
        channel_id: u32,
        rows: u64,
        row_groups: u32,
        first_biz: u64,
        orders: u64,
        trades: u64,
    ) {
        let parent = manifest.parent().expect("manifest parent");
        fs::create_dir_all(parent).expect("create manifest parent");
        let last_biz = first_biz + rows - 1;
        let manifest_json = json!({
            "schema_version": 1,
            "trading_day": trading_day,
            "artifact_format": COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
            "payload_width_bytes": COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            "selection_scope": "contiguous-prefix",
            "channel_indivisible": true,
            "counts": {"channels": 1, "row_count": rows, "order_records": orders, "trade_records": trades},
            "channels": [{
                "channel_id": channel_id,
                "relative_path": format!("channels/{channel_id}/l2_mutations.physical-v2.ocb"),
                "row_count": rows,
                "row_group_count": row_groups,
                "first_biz_index": first_biz,
                "last_biz_index": last_biz,
                "min_receive_nano": 1_000_000_i64 + first_biz as i64,
                "max_receive_nano": 1_000_000_i64 + last_biz as i64,
                "order_record_count": orders,
                "trade_record_count": trades
            }],
            "claims": {"default_readiness": false, "runtime_readiness": false, "performance_dominance": false}
        });
        fs::write(manifest, serde_json::to_vec_pretty(&manifest_json).unwrap())
            .expect("write physical-v2 manifest");
    }

    fn write_compact_l2_fixture(
        path: &Path,
        trading_day: u32,
        channel_id: u32,
        biz_indexes: &[u64],
        record_kinds: &[u8],
        width: u32,
        override_channel_id: Option<u32>,
    ) {
        assert_eq!(biz_indexes.len(), record_kinds.len());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create artifact parent");
        }
        let mut records = Vec::new();
        for (biz_index, record_kind) in biz_indexes
            .iter()
            .copied()
            .zip(record_kinds.iter().copied())
        {
            let actual_channel = override_channel_id.unwrap_or(channel_id);
            let full = compact_l2_record(trading_day, actual_channel, biz_index, record_kind);
            records.push(full[..width as usize].to_vec());
        }

        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
        let mut row_groups = Vec::new();
        let mut chunk_descs = Vec::new();
        let row_group_size = 2usize;
        for (row_group_id, chunk_records) in records.chunks(row_group_size).enumerate() {
            let row_group_id = row_group_id as u32;
            let slices = chunk_records.iter().map(Vec::as_slice).collect::<Vec<_>>();
            let value_ref = append_fixed_binary_chunk(
                &mut file_bytes,
                row_group_id,
                0,
                width,
                &slices,
                OcbChunkCodecV1::None,
            );
            let row_count = chunk_records.len() as u64;
            row_groups.push(OcbRowGroupDescV1 {
                row_group_id,
                flags: 0,
                base_row: (row_group_id as u64) * row_group_size as u64,
                row_count,
                chunk_desc_begin: u64::from(row_group_id),
                chunk_desc_count: 1,
                stat_begin: 0,
                stat_count: 0,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            });
            chunk_descs.push(OcbColumnChunkDescV1 {
                row_group_id,
                column_id: 0,
                physical_type: OcbPhysicalTypeV1::FixedBinary,
                codec: OcbChunkCodecV1::None,
                flags: 0,
                value_ref,
                validity_ref: OcbBodyRefV2::NULL,
                row_count,
                uncompressed_bytes: row_count * u64::from(width),
            });
        }

        let string_table = OcbStringTableV1 {
            version: 1,
            strings: vec!["payload".into()],
            crc32c: 0,
        };
        let string_table_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
                string_table.write_to(buf)
            });
        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns: vec![fixed_binary_column_desc(0, 0, width)],
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
            column_count: 1,
            row_group_count: ((records.len() + row_group_size - 1) / row_group_size) as u32,
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
        let bootstrap = OcbBootstrapPageV1::new([88u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
        let mut file = fs::File::create(path).expect("create compact L2 fixture");
        file.write_all(&file_bytes)
            .expect("write compact L2 fixture");
    }

    fn write_compact_l2_physical_v2_fixture(
        path: &Path,
        records: &[[u8; 168]],
        symbol_width_override: Option<u32>,
    ) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create physical-v2 parent");
        }
        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
        let mut row_groups = Vec::new();
        let mut chunk_descs = Vec::new();
        let row_group_size = 2usize;
        let column_names = compact_l2_physical_v2_required_columns();

        for (row_group_id, chunk_records) in records.chunks(row_group_size).enumerate() {
            let row_group_id = row_group_id as u32;
            let chunk_desc_begin = chunk_descs.len() as u64;
            let decoded = chunk_records
                .iter()
                .map(|record| CompactL2PhysicalV2Record::from_fixed_binary_v1(record).unwrap())
                .collect::<Vec<_>>();
            let row_count = decoded.len() as u64;

            push_i32_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                0,
                &decoded
                    .iter()
                    .map(|record| i32::try_from(record.trading_day).unwrap())
                    .collect::<Vec<_>>(),
            );
            push_i64_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                1,
                &decoded
                    .iter()
                    .map(|record| i64::from(record.channel_id))
                    .collect::<Vec<_>>(),
            );
            push_i64_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                2,
                &decoded
                    .iter()
                    .map(|record| record.biz_index)
                    .collect::<Vec<_>>(),
            );
            push_i64_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                3,
                &decoded
                    .iter()
                    .map(|record| record.receive_nano)
                    .collect::<Vec<_>>(),
            );
            push_i64_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                4,
                &decoded
                    .iter()
                    .map(|record| i64::try_from(record.source_ordinal).unwrap())
                    .collect::<Vec<_>>(),
            );
            push_i32_physical_v2_chunk(
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
            push_fixed_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                6,
                2,
                &header_bytes,
            );
            push_i64_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                7,
                &decoded
                    .iter()
                    .map(|record| record.exchange_time)
                    .collect::<Vec<_>>(),
            );
            let symbol_width = symbol_width_override.unwrap_or(9);
            let symbols = decoded
                .iter()
                .map(|record| record.symbol[..symbol_width as usize].to_vec())
                .collect::<Vec<_>>();
            push_fixed_bytes_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                8,
                symbol_width,
                &symbols,
            );
            let body_bytes = decoded
                .iter()
                .map(|record| record.body_bytes_80_86)
                .collect::<Vec<_>>();
            push_fixed_physical_v2_chunk(
                &mut file_bytes,
                &mut chunk_descs,
                row_group_id,
                9,
                7,
                &body_bytes,
            );
            for slot in 0..10 {
                push_i64_physical_v2_chunk(
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
        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns: physical_v2_column_descs(symbol_width_override),
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
        let bootstrap = OcbBootstrapPageV1::new([77u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
        let mut file = fs::File::create(path).expect("create physical-v2 fixture");
        file.write_all(&file_bytes)
            .expect("write physical-v2 fixture");
    }

    fn push_i32_physical_v2_chunk(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        values: &[i32],
    ) {
        let value_ref = append_i32_chunk(file_bytes, row_group_id, column_id, values);
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I32,
            value_ref,
            values.len() as u64,
            (values.len() * 4) as u64,
        ));
    }

    fn push_i64_physical_v2_chunk(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        values: &[i64],
    ) {
        let value_ref = append_i64_chunk(file_bytes, row_group_id, column_id, values);
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I64,
            value_ref,
            values.len() as u64,
            (values.len() * 8) as u64,
        ));
    }

    fn push_fixed_physical_v2_chunk<const N: usize>(
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
        let value_ref = append_fixed_binary_chunk(
            file_bytes,
            row_group_id,
            column_id,
            width,
            &rows,
            OcbChunkCodecV1::None,
        );
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::FixedBinary,
            value_ref,
            values.len() as u64,
            values.len() as u64 * u64::from(width),
        ));
    }

    fn push_fixed_bytes_physical_v2_chunk(
        file_bytes: &mut Vec<u8>,
        chunk_descs: &mut Vec<OcbColumnChunkDescV1>,
        row_group_id: u32,
        column_id: u32,
        width: u32,
        values: &[Vec<u8>],
    ) {
        let rows = values.iter().map(Vec::as_slice).collect::<Vec<_>>();
        let value_ref = append_fixed_binary_chunk(
            file_bytes,
            row_group_id,
            column_id,
            width,
            &rows,
            OcbChunkCodecV1::None,
        );
        chunk_descs.push(chunk_desc(
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::FixedBinary,
            value_ref,
            values.len() as u64,
            values.len() as u64 * u64::from(width),
        ));
    }

    fn append_i32_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        values: &[i32],
    ) -> OcbBodyRefV2 {
        let mut payload = Vec::with_capacity(values.len() * 4);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        append_primitive_chunk(
            file_bytes,
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I32,
            values.len() as u64,
            payload,
        )
    }

    fn append_i64_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        values: &[i64],
    ) -> OcbBodyRefV2 {
        let mut payload = Vec::with_capacity(values.len() * 8);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        append_primitive_chunk(
            file_bytes,
            row_group_id,
            column_id,
            OcbPhysicalTypeV1::I64,
            values.len() as u64,
            payload,
        )
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

    fn physical_v2_column_descs(symbol_width_override: Option<u32>) -> Vec<OcbColumnDescV1> {
        let mut columns = Vec::new();
        columns.push(primitive_column_desc(0, 0, OcbPhysicalTypeV1::I32));
        columns.push(primitive_column_desc(1, 1, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(2, 2, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(3, 3, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(4, 4, OcbPhysicalTypeV1::I64));
        columns.push(primitive_column_desc(5, 5, OcbPhysicalTypeV1::I32));
        columns.push(fixed_binary_column_desc(6, 6, 2));
        columns.push(primitive_column_desc(7, 7, OcbPhysicalTypeV1::I64));
        columns.push(fixed_binary_column_desc(
            8,
            8,
            symbol_width_override.unwrap_or(9),
        ));
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

    fn compact_l2_record(
        trading_day: u32,
        channel_id: u32,
        biz_index: u64,
        record_kind: u8,
    ) -> [u8; 168] {
        let mut bytes = [0u8; 168];
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

    fn append_fixed_binary_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        width: u32,
        values: &[&[u8]],
        codec: OcbChunkCodecV1,
    ) -> OcbBodyRefV2 {
        let mut raw_payload = Vec::with_capacity(values.len() * width as usize);
        for value in values {
            assert_eq!(value.len(), width as usize);
            raw_payload.extend_from_slice(value);
        }
        let payload = match codec {
            OcbChunkCodecV1::None => raw_payload.clone(),
            OcbChunkCodecV1::Zstd => {
                zstd::stream::encode_all(std::io::Cursor::new(raw_payload.as_slice()), 1)
                    .expect("compress fixed-binary chunk")
            }
        };
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::FixedBinary,
            codec,
            flags: 0,
            row_group_id,
            column_id,
            row_count: values.len() as u64,
            uncompressed_bytes: raw_payload.len() as u64,
            payload,
            crc32c: 0,
        };
        append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
            chunk.write_to(buf)
        })
    }

    fn append_encoded_object(
        file_bytes: &mut Vec<u8>,
        kind: OcbBodyKindV1,
        write: impl FnOnce(&mut Vec<u8>) -> Result<()>,
    ) -> OcbBodyRefV2 {
        let mut object = Vec::new();
        write(&mut object).expect("encode object");
        append_raw_object(file_bytes, kind, object)
    }

    fn append_raw_object(
        file_bytes: &mut Vec<u8>,
        kind: OcbBodyKindV1,
        object: Vec<u8>,
    ) -> OcbBodyRefV2 {
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

    fn fixed_binary_column_desc(
        column_id: u32,
        name_string_id: u32,
        width: u32,
    ) -> OcbColumnDescV1 {
        OcbColumnDescV1 {
            column_id,
            name_string_id,
            physical_type: OcbPhysicalTypeV1::FixedBinary,
            logical_kind: OcbLogicalKindV1::OpaqueKey,
            flags: 0,
            dictionary_id: OCB_NULL_U32,
            scale: 0,
            nullability: OcbNullabilityV1::NonNull,
            reserved0: 0,
            fixed_binary_width: width,
        }
    }
}
