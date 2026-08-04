use super::*;

/// Dense read result with an optional validity mask copied into Rust memory.
#[derive(Debug, Clone, PartialEq)]
pub struct DenseTensor {
    /// Dense tensor payload.
    pub tensor: Tensor,
    /// Optional validity mask where `1` means valid and `0` means filled/null.
    pub mask: Option<Vec<u8>>,
}

/// Append entry range assigned by the native append call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendRange {
    /// First appended entry id.
    pub start: u32,
    /// One-past-last appended entry id.
    pub end: u32,
}

/// Sparse-intent detector used to classify logically absent subtensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseDetector {
    /// Treat a subtensor as absent when the native nullable representation marks it null.
    NullSubtensor,
    /// Treat a subtensor as absent when every value matches the supplied predicate.
    PredicateSubtensor,
}

impl SparseDetector {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioSparseDetectorKind {
        match self {
            Self::NullSubtensor => sys::ARCADIA_TIO_SPARSE_DETECTOR_NULL_SUBTENSOR,
            Self::PredicateSubtensor => sys::ARCADIA_TIO_SPARSE_DETECTOR_PREDICATE_SUBTENSOR,
        }
    }
}

/// Value predicate for sparse-intent absence detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SparseValuePredicate {
    /// Match IEEE NaN values.
    Nan,
    /// Match zero values.
    Zero,
    /// Match an exact `f32` value.
    EqualF32(f32),
    /// Match an exact `f64` value.
    EqualF64(f64),
    /// Match an exact `i32` value.
    EqualI32(i32),
    /// Match an exact `i64` value.
    EqualI64(i64),
}

impl SparseValuePredicate {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioSparseValuePredicate {
        let (kind, value) = match self {
            Self::Nan => (sys::ARCADIA_TIO_SPARSE_PREDICATE_NAN, 0.0),
            Self::Zero => (sys::ARCADIA_TIO_SPARSE_PREDICATE_ZERO, 0.0),
            Self::EqualF32(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F32, value as f64),
            Self::EqualF64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F64, value),
            Self::EqualI32(_) | Self::EqualI64(_) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_ZERO, 0.0),
        };
        sys::ArcadiaTioSparseValuePredicate { kind, value }
    }

    pub(crate) fn to_raw_v2(self) -> sys::ArcadiaTioSparseValuePredicateV2 {
        let (kind, float_value, integer_value) = match self {
            Self::Nan => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_NAN, 0.0, 0),
            Self::Zero => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_ZERO, 0.0, 0),
            Self::EqualF32(value) => (
                sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F32,
                value as f64,
                0,
            ),
            Self::EqualF64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F64, value, 0),
            Self::EqualI32(value) => (
                sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I32,
                0.0,
                value as i64,
            ),
            Self::EqualI64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I64, 0.0, value),
        };
        sys::ArcadiaTioSparseValuePredicateV2 {
            kind,
            float_value,
            integer_value,
        }
    }
}

/// Fallback policy when native sparse lowering is not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseFallbackPolicy {
    /// Preserve exact values by appending densely when sparse lowering cannot be used.
    Dense,
}

impl SparseFallbackPolicy {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioSparseFallbackPolicy {
        match self {
            Self::Dense => sys::ARCADIA_TIO_SPARSE_FALLBACK_DENSE,
        }
    }
}

/// Safe sparse-intent rule used by f32/f64/i32/i64 sparse analysis and append helpers.
///
/// Integer payloads support [`SparseRule::null_subtensor`], [`SparseValuePredicate::Zero`],
/// and exact [`SparseValuePredicate::EqualI32`] / [`SparseValuePredicate::EqualI64`] predicates.
///
/// A rule owns the sparse-axis list and threshold settings. The wrapper validates the owned
/// axes against the open file before calling the C ABI so borrowed raw pointers only live for a
/// single FFI call. Sparse-intent diagnostics describe the current native lowering decision; they
/// are not storage-efficiency, compression-ratio, layout-superiority, or capacity claims.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseRule {
    pub(crate) sparse_axes: Vec<usize>,
    pub(crate) detector: SparseDetector,
    pub(crate) predicate: SparseValuePredicate,
    pub(crate) min_absent_fraction: f64,
    pub(crate) min_absent_subtensors: u64,
    pub(crate) fallback: SparseFallbackPolicy,
}

impl SparseRule {
    /// Creates a null-subtensor sparse rule for the provided non-append sparse axes.
    pub fn null_subtensor(sparse_axes: Vec<usize>) -> Self {
        Self {
            sparse_axes,
            detector: SparseDetector::NullSubtensor,
            predicate: SparseValuePredicate::Nan,
            min_absent_fraction: 0.0,
            min_absent_subtensors: 1,
            fallback: SparseFallbackPolicy::Dense,
        }
    }

    /// Creates a predicate-subtensor sparse rule for the provided non-append sparse axes.
    pub fn predicate_subtensor(sparse_axes: Vec<usize>, predicate: SparseValuePredicate) -> Self {
        Self {
            sparse_axes,
            detector: SparseDetector::PredicateSubtensor,
            predicate,
            min_absent_fraction: 0.0,
            min_absent_subtensors: 1,
            fallback: SparseFallbackPolicy::Dense,
        }
    }

    /// Returns the configured sparse axes.
    pub fn sparse_axes(&self) -> &[usize] {
        &self.sparse_axes
    }

    /// Returns the configured absence detector.
    pub fn detector(&self) -> SparseDetector {
        self.detector
    }

    /// Returns the configured predicate. It is ignored for null-subtensor rules.
    pub fn predicate(&self) -> SparseValuePredicate {
        self.predicate
    }

    /// Returns the minimum absent fraction threshold.
    pub fn min_absent_fraction(&self) -> f64 {
        self.min_absent_fraction
    }

    /// Returns the minimum absent subtensor-count threshold.
    pub fn min_absent_subtensors(&self) -> u64 {
        self.min_absent_subtensors
    }

    /// Returns the configured fallback policy.
    pub fn fallback(&self) -> SparseFallbackPolicy {
        self.fallback
    }

    /// Sets the minimum absent fraction required before sparse lowering is considered.
    pub fn with_min_absent_fraction(mut self, min_absent_fraction: f64) -> Self {
        self.min_absent_fraction = min_absent_fraction;
        self
    }

    /// Sets the minimum absent subtensor count required before sparse lowering is considered.
    pub fn with_min_absent_subtensors(mut self, min_absent_subtensors: u64) -> Self {
        self.min_absent_subtensors = min_absent_subtensors;
        self
    }

    /// Sets the fallback policy used when sparse lowering is not selected.
    pub fn with_fallback(mut self, fallback: SparseFallbackPolicy) -> Self {
        self.fallback = fallback;
        self
    }

    pub(crate) fn validate_for_append(
        &self,
        dtype: DType,
        rank: usize,
        append_axis: usize,
    ) -> Result<()> {
        if rank == 0 {
            return Err(TioError::invalid_argument(
                "sparse append shape rank must be non-zero",
            ));
        }
        if append_axis >= rank {
            return Err(TioError::invalid_argument(format!(
                "append axis {append_axis} out of range for rank {rank}"
            )));
        }
        if append_axis != 0 {
            return Err(TioError::invalid_argument(
                "sparse append currently supports append axis 0 only",
            ));
        }
        if self.sparse_axes.is_empty() {
            return Err(TioError::invalid_argument(
                "sparse rule sparse_axes must not be empty",
            ));
        }
        for (index, &axis) in self.sparse_axes.iter().enumerate() {
            if axis >= rank {
                return Err(TioError::invalid_argument(format!(
                    "sparse axis {axis} out of range for rank {rank}"
                )));
            }
            if axis == append_axis {
                return Err(TioError::invalid_argument(
                    "sparse axes must exclude the append axis",
                ));
            }
            if self.sparse_axes[..index].contains(&axis) {
                return Err(TioError::invalid_argument("sparse axes must be unique"));
            }
        }
        if !self.min_absent_fraction.is_finite() || !(0.0..=1.0).contains(&self.min_absent_fraction)
        {
            return Err(TioError::invalid_argument(
                "sparse rule min_absent_fraction must be finite and between 0.0 and 1.0",
            ));
        }
        if self.detector == SparseDetector::PredicateSubtensor {
            match (dtype, self.predicate) {
                (
                    DType::F32,
                    SparseValuePredicate::EqualF64(_)
                    | SparseValuePredicate::EqualI32(_)
                    | SparseValuePredicate::EqualI64(_),
                ) => {
                    return Err(TioError::invalid_argument(
                        "f32 sparse append cannot use this predicate dtype",
                    ));
                }
                (
                    DType::F64,
                    SparseValuePredicate::EqualF32(_)
                    | SparseValuePredicate::EqualI32(_)
                    | SparseValuePredicate::EqualI64(_),
                ) => {
                    return Err(TioError::invalid_argument(
                        "f64 sparse append cannot use this predicate dtype",
                    ));
                }
                (DType::I32, SparseValuePredicate::Zero | SparseValuePredicate::EqualI32(_)) => {}
                (DType::I64, SparseValuePredicate::Zero | SparseValuePredicate::EqualI64(_)) => {}
                (DType::I32 | DType::I64, _) => {
                    return Err(TioError::invalid_argument(
                        "integer sparse append predicate does not match tensor dtype",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Native sparse-intent analysis outcome copied into Rust-owned values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseAppendOutcome {
    /// Native analysis selected the RegularChunked sparse producer path.
    SparseRegularChunked,
    /// Native analysis selected dense append fallback.
    DenseFallback,
    /// Native analysis rejected the sparse-intent request.
    Reject,
    /// Native analysis selected the SparseChunkTree sparse producer path.
    SparseChunkTree,
}

impl SparseAppendOutcome {
    pub(crate) fn from_raw(value: sys::ArcadiaTioSparseAppendOutcome) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_SPARSE_APPEND_SPARSE_REGULAR_CHUNKED => Ok(Self::SparseRegularChunked),
            sys::ARCADIA_TIO_SPARSE_APPEND_DENSE_FALLBACK => Ok(Self::DenseFallback),
            sys::ARCADIA_TIO_SPARSE_APPEND_REJECT => Ok(Self::Reject),
            sys::ARCADIA_TIO_SPARSE_APPEND_SPARSE_CHUNK_TREE => Ok(Self::SparseChunkTree),
            other => Err(TioError::conversion(format!(
                "unknown sparse append outcome value {other}"
            ))),
        }
    }
}

/// Structured reason code explaining a sparse-intent analysis decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseAppendReason {
    /// No absent subtensors were detected in the append payload.
    NoAbsentSubtensorsDetected,
    /// Sparse axes must not be empty.
    SparseAxesMustNotBeEmpty,
    /// Sparse axes must be unique.
    SparseAxesMustBeUnique,
    /// Sparse axes must be within the file rank.
    SparseAxesOutOfBounds,
    /// Sparse axes must not include the append axis.
    SparseAxesMustExcludeAppendAxis,
    /// Current root sparse append supports append axis zero only.
    AppendAxisMustBeZeroForCurrentRootAppend,
    /// The predicate is not compatible with the payload dtype.
    PredicateDTypeMismatch,
    /// Dense fallback preserves exact values.
    DenseFallbackPreservesExactValues,
    /// Sparse lowering was below the configured threshold.
    SparseLoweringBelowThreshold,
    /// WholeAppendUnit layout has no current sparse producer path.
    WholeAppendUnitHasNoSparseProducerPath,
    /// RegularChunked block shape was not published for sparse lowering.
    RegularChunkedBlockShapeUnpublished,
    /// RegularChunked dense fallback requires stable non-append extents.
    RegularChunkedDenseFallbackRequiresStableNonAppendExtents,
    /// RegularChunked dense fallback requires a dense published lane set.
    RegularChunkedDenseFallbackRequiresDensePublishedLaneSet,
    /// RegularChunked sparse lowering requires a stable published lane set.
    RegularChunkedSparseLoweringRequiresStablePublishedLaneSet,
    /// The tensor contains nulls that dense fallback cannot preserve.
    TensorContainsNullsThatDenseFallbackCannotPreserve,
    /// Logical absence does not compile to the current sparse model.
    LogicalAbsenceDoesNotCompileToCurrentSparseModel,
    /// The current native sparse lowering is not implemented for this detector.
    CurrentSparseLoweringNotYetImplementedForDetector,
}

impl SparseAppendReason {
    pub(crate) fn from_raw(value: sys::ArcadiaTioSparseAppendReason) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_SPARSE_REASON_NO_ABSENT_SUBTENSORS_DETECTED => {
                Ok(Self::NoAbsentSubtensorsDetected)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_NOT_BE_EMPTY => {
                Ok(Self::SparseAxesMustNotBeEmpty)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_BE_UNIQUE => {
                Ok(Self::SparseAxesMustBeUnique)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_OUT_OF_BOUNDS => {
                Ok(Self::SparseAxesOutOfBounds)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_EXCLUDE_APPEND_AXIS => {
                Ok(Self::SparseAxesMustExcludeAppendAxis)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_APPEND_AXIS_MUST_BE_ZERO_FOR_CURRENT_ROOT_APPEND => {
                Ok(Self::AppendAxisMustBeZeroForCurrentRootAppend)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_PREDICATE_DTYPE_MISMATCH => {
                Ok(Self::PredicateDTypeMismatch)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_DENSE_FALLBACK_PRESERVES_EXACT_VALUES => {
                Ok(Self::DenseFallbackPreservesExactValues)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_LOWERING_BELOW_THRESHOLD => {
                Ok(Self::SparseLoweringBelowThreshold)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_WHOLE_APPEND_UNIT_HAS_NO_SPARSE_PRODUCER_PATH => {
                Ok(Self::WholeAppendUnitHasNoSparseProducerPath)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_BLOCK_SHAPE_UNPUBLISHED => {
                Ok(Self::RegularChunkedBlockShapeUnpublished)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_STABLE_NON_APPEND_EXTENTS => {
                Ok(Self::RegularChunkedDenseFallbackRequiresStableNonAppendExtents)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_DENSE_PUBLISHED_LANE_SET => {
                Ok(Self::RegularChunkedDenseFallbackRequiresDensePublishedLaneSet)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_SPARSE_LOWERING_REQUIRES_STABLE_PUBLISHED_LANE_SET => {
                Ok(Self::RegularChunkedSparseLoweringRequiresStablePublishedLaneSet)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_TENSOR_CONTAINS_NULLS_THAT_DENSE_FALLBACK_CANNOT_PRESERVE => {
                Ok(Self::TensorContainsNullsThatDenseFallbackCannotPreserve)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_LOGICAL_ABSENCE_DOES_NOT_COMPILE_TO_CURRENT_SPARSE_MODEL => {
                Ok(Self::LogicalAbsenceDoesNotCompileToCurrentSparseModel)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_CURRENT_SPARSE_LOWERING_NOT_YET_IMPLEMENTED_FOR_DETECTOR => {
                Ok(Self::CurrentSparseLoweringNotYetImplementedForDetector)
            }
            other => Err(TioError::conversion(format!(
                "unknown sparse append reason value {other}"
            ))),
        }
    }
}

/// Rust-owned sparse-intent analysis report copied from native output.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseAppendAnalysis {
    /// Selected native append outcome.
    pub outcome: SparseAppendOutcome,
    /// Fraction of detected absent subtensors considered by native analysis.
    pub absent_fraction: f64,
    /// Count of absent subtensors detected by native analysis.
    pub absent_subtensor_count: u64,
    /// Count of total subtensors considered by native analysis.
    pub total_subtensor_count: u64,
    /// Structured native reason codes copied into Rust memory.
    pub reasons: Vec<SparseAppendReason>,
}

pub(crate) fn empty_sparse_append_analysis() -> sys::ArcadiaTioSparseAppendAnalysis {
    sys::ArcadiaTioSparseAppendAnalysis {
        outcome: sys::ARCADIA_TIO_SPARSE_APPEND_REJECT,
        absent_fraction: 0.0,
        absent_subtensor_count: 0,
        total_subtensor_count: 0,
        reasons: ptr::null_mut(),
        reasons_len: 0,
    }
}

pub(crate) struct SparseAppendAnalysisGuard<'a> {
    pub(crate) raw: &'a mut sys::ArcadiaTioSparseAppendAnalysis,
}

impl Drop for SparseAppendAnalysisGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: The guard is created only for raw analysis values initialized by this wrapper or
        // native sparse analysis. Native free tolerates empty/null reason buffers and nulls the raw
        // output after releasing any owned reasons, preventing accidental double-free by callers.
        unsafe { sys::arcadia_tio_sparse_append_analysis_free(self.raw) };
    }
}

pub(crate) fn take_sparse_append_analysis(
    raw: &mut sys::ArcadiaTioSparseAppendAnalysis,
) -> Result<SparseAppendAnalysis> {
    let guard = SparseAppendAnalysisGuard { raw };
    // SAFETY: Successful native analysis returns `reasons` pointing to `reasons_len` values. The
    // guard frees the native analysis exactly once on success and every conversion error.
    let raw_reasons = unsafe {
        checked_slice(
            guard.raw.reasons.cast_const(),
            guard.raw.reasons_len,
            "sparse append reasons",
        )
    }?;
    let reasons = raw_reasons
        .iter()
        .copied()
        .map(SparseAppendReason::from_raw)
        .collect::<Result<Vec<_>>>()?;
    Ok(SparseAppendAnalysis {
        outcome: SparseAppendOutcome::from_raw(guard.raw.outcome)?,
        absent_fraction: guard.raw.absent_fraction,
        absent_subtensor_count: guard.raw.absent_subtensor_count,
        total_subtensor_count: guard.raw.total_subtensor_count,
        reasons,
    })
}

/// Commit metadata returned by retained-history listing APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitInfo {
    /// Native commit sequence number.
    pub commit_seq: u64,
    /// Footer offset for this commit in the native file.
    pub footer_offset: u64,
    /// Previous visible footer offset recorded for this commit.
    pub prev_footer_offset: u64,
}

impl From<sys::ArcadiaTioCommitInfo> for CommitInfo {
    fn from(raw: sys::ArcadiaTioCommitInfo) -> Self {
        Self {
            commit_seq: raw.commit_seq,
            footer_offset: raw.footer_offset,
            prev_footer_offset: raw.prev_footer_offset,
        }
    }
}

/// Native chunking plan copied into Rust-owned memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPlan {
    /// Block size per axis in native rank order.
    pub block_sizes: Vec<u32>,
}

/// 16-byte universe family/version identifier used by the C ABI.
pub type UniverseUuid = [u8; 16];

/// Axis identity mode used when creating universe-aware files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisIdentityMode {
    /// Axis identity is ordinary extent-only shape identity.
    ExtentOnly,
    /// Axis identity is universe-aware and can be targeted by explicit universe reads.
    UniverseAware,
}

impl AxisIdentityMode {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioAxisIdentityMode {
        match self {
            Self::ExtentOnly => sys::ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY,
            Self::UniverseAware => sys::ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE,
        }
    }
}

/// Create-time axis identity descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisIdentityInput {
    /// Axis index.
    pub axis: u32,
    /// Axis identity mode.
    pub mode: AxisIdentityMode,
}

impl AxisIdentityInput {
    /// Creates an extent-only axis identity descriptor.
    pub fn extent_only(axis: u32) -> Self {
        Self {
            axis,
            mode: AxisIdentityMode::ExtentOnly,
        }
    }

    /// Creates a universe-aware axis identity descriptor.
    pub fn universe_aware(axis: u32) -> Self {
        Self {
            axis,
            mode: AxisIdentityMode::UniverseAware,
        }
    }
}

/// Universe-aware create options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateUniverseOptions {
    /// Axis identity descriptors.
    pub axis_identities: Vec<AxisIdentityInput>,
}

impl CreateUniverseOptions {
    /// Creates universe options from axis identity descriptors.
    pub fn new(axis_identities: Vec<AxisIdentityInput>) -> Self {
        Self { axis_identities }
    }
}

/// Per-axis universe binding for one appended slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniverseBinding {
    /// Axis index.
    pub axis: u32,
    /// Universe family UUID.
    pub family_uuid: UniverseUuid,
    /// Universe version UUID.
    pub version_uuid: UniverseUuid,
    /// Source universe length.
    pub length: u64,
}

impl UniverseBinding {
    /// Creates a per-axis universe binding.
    pub fn new(
        axis: u32,
        family_uuid: UniverseUuid,
        version_uuid: UniverseUuid,
        length: u64,
    ) -> Self {
        Self {
            axis,
            family_uuid,
            version_uuid,
            length,
        }
    }
}

/// Universe bindings for one appended slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotUniverseBindings {
    /// Axis bindings for this appended slot.
    pub axes: Vec<UniverseBinding>,
}

impl SlotUniverseBindings {
    /// Creates slot bindings from per-axis universe bindings.
    pub fn new(axes: Vec<UniverseBinding>) -> Self {
        Self { axes }
    }
}

/// Payload-driven universe remap for one axis in one appended slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniverseRemap {
    /// Axis index.
    pub axis: u32,
    /// Target universe family UUID.
    pub target_family_uuid: UniverseUuid,
    /// Target universe version UUID.
    pub target_version_uuid: UniverseUuid,
    /// Target universe length.
    pub target_length: u64,
    /// Source index to target index mapping.
    pub source_to_target: Vec<u64>,
}

impl UniverseRemap {
    /// Creates a payload-driven universe remap.
    pub fn new(
        axis: u32,
        target_family_uuid: UniverseUuid,
        target_version_uuid: UniverseUuid,
        target_length: u64,
        source_to_target: Vec<u64>,
    ) -> Self {
        Self {
            axis,
            target_family_uuid,
            target_version_uuid,
            target_length,
            source_to_target,
        }
    }
}

/// Universe remaps for one appended slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotUniverseRemaps {
    /// Axis remaps for this appended slot.
    pub axes: Vec<UniverseRemap>,
}

impl SlotUniverseRemaps {
    /// Creates slot remaps from per-axis universe remaps.
    pub fn new(axes: Vec<UniverseRemap>) -> Self {
        Self { axes }
    }
}

/// Universe-aware append options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppendWithUniverseOptions {
    /// Per-appended-slot universe bindings.
    pub slots: Vec<SlotUniverseBindings>,
    /// Optional per-appended-slot universe remaps.
    pub remap_slots: Vec<SlotUniverseRemaps>,
}

impl AppendWithUniverseOptions {
    /// Creates append options from per-slot universe bindings.
    pub fn new(slots: Vec<SlotUniverseBindings>) -> Self {
        Self {
            slots,
            remap_slots: Vec::new(),
        }
    }
}

/// Explicit universe target for shape-policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitUniverseAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Target universe family UUID.
    pub family_uuid: UniverseUuid,
    /// Target universe version UUID.
    pub version_uuid: UniverseUuid,
    /// Target universe length.
    pub length: u64,
}

impl ExplicitUniverseAxisTarget {
    /// Creates an explicit universe axis target.
    pub fn new(
        axis: u32,
        family_uuid: UniverseUuid,
        version_uuid: UniverseUuid,
        length: u64,
    ) -> Self {
        Self {
            axis,
            family_uuid,
            version_uuid,
            length,
        }
    }
}

/// Explicit extent target for split-domain shape-policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitExtentAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Target axis length.
    pub length: u64,
}

impl ExplicitExtentAxisTarget {
    /// Creates an explicit extent axis target.
    pub fn new(axis: u32, length: u64) -> Self {
        Self { axis, length }
    }
}

/// Shape policy for current and historical reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadShapePolicy {
    /// Use the file envelope shape. This matches bare/current `read_all` defaults.
    FileEnvelope,
    /// Use the current head shape.
    CurrentHead,
    /// Use the union of selected entry shapes.
    Union,
    /// Use the intersection of selected entry shapes.
    Intersection,
    /// Use the initially registered extents.
    InitialRegistered,
    /// Use explicit extents for all axes.
    ExplicitExtents(Vec<u64>),
    /// Use explicit universe targets for universe-aware axes.
    ExplicitUniverse(Vec<ExplicitUniverseAxisTarget>),
    /// Use explicit universe targets for universe-aware axes and explicit extents for extent-only axes.
    ExplicitUniverseAndExtents {
        /// Universe-aware axis targets.
        universe_axes: Vec<ExplicitUniverseAxisTarget>,
        /// Extent-only axis targets.
        extent_axes: Vec<ExplicitExtentAxisTarget>,
    },
}

impl ReadShapePolicy {
    pub(crate) fn to_raw_tag(&self) -> sys::ArcadiaTioReadShapePolicyTag {
        match self {
            Self::FileEnvelope => sys::ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE,
            Self::CurrentHead => sys::ARCADIA_TIO_READ_SHAPE_POLICY_CURRENT_HEAD,
            Self::Union => sys::ARCADIA_TIO_READ_SHAPE_POLICY_UNION,
            Self::Intersection => sys::ARCADIA_TIO_READ_SHAPE_POLICY_INTERSECTION,
            Self::InitialRegistered => sys::ARCADIA_TIO_READ_SHAPE_POLICY_INITIAL_REGISTERED,
            Self::ExplicitExtents(_) => sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_EXTENTS,
            Self::ExplicitUniverse(_) => sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE,
            Self::ExplicitUniverseAndExtents { .. } => {
                sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS
            }
        }
    }
}

/// Read execution mode for option-bearing reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadExecutionMode {
    /// Serial execution.
    Serial,
    /// Native parallel thread execution with a maximum thread count.
    ParallelThreads { max_threads: usize },
}

impl ReadExecutionMode {
    /// Serial execution.
    pub fn serial() -> Self {
        Self::Serial
    }

    /// Native parallel thread execution with a maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self::ParallelThreads { max_threads }
    }

    pub(crate) fn to_raw(self) -> Result<(sys::ArcadiaTioReadExecutionMode, usize)> {
        match self {
            Self::Serial => Ok((sys::ARCADIA_TIO_READ_EXECUTION_SERIAL, 1)),
            Self::ParallelThreads { max_threads } if max_threads > 0 => Ok((
                sys::ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS,
                max_threads,
            )),
            Self::ParallelThreads { .. } => Err(TioError::invalid_argument(
                "parallel read max_threads must be > 0",
            )),
        }
    }

    pub(crate) fn from_raw(
        value: sys::ArcadiaTioReadExecutionMode,
        threads: usize,
    ) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_READ_EXECUTION_SERIAL => Ok(Self::Serial),
            sys::ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS => Ok(Self::ParallelThreads {
                max_threads: threads,
            }),
            other => Err(TioError::conversion(format!(
                "unknown read execution mode value {other}"
            ))),
        }
    }
}

/// Current read options with execution mode only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadWithOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
}

impl ReadWithOptions {
    /// Serial read execution.
    pub fn serial() -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
        }
    }

    /// Parallel read execution with the provided maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
        }
    }
}

/// Current read options with execution mode and shape policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadWithShapePolicyOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
    /// Shape policy.
    pub shape_policy: ReadShapePolicy,
}

impl ReadWithShapePolicyOptions {
    /// Serial read with the provided shape policy.
    pub fn serial(shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
            shape_policy,
        }
    }

    /// Parallel read with the provided maximum thread count and shape policy.
    pub fn parallel_threads(max_threads: usize, shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
            shape_policy,
        }
    }
}

/// Query-attribution context for opt-in diagnostic current reads.
///
/// All string fields are owned Rust strings. The safe wrapper converts them to temporary C strings
/// for the attributed read call and never exposes the borrowed native pointers in public Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTraceContext {
    /// Run identifier copied into native trace metadata.
    pub run_id: String,
    /// Result-row identifier copied into native trace metadata.
    pub row_id: String,
    /// Repeat index copied into native trace metadata.
    pub repeat_index: u32,
    /// Phase label copied into native trace metadata.
    pub phase: String,
    /// Language label copied into native trace metadata.
    pub language: String,
    /// Public API surface label copied into native trace metadata.
    pub api_surface: String,
    /// Operation label copied into native trace metadata.
    pub operation: String,
    /// Trace-clock label copied into native trace metadata.
    pub trace_clock: String,
}

impl QueryTraceContext {
    /// Creates a query-attribution context for a single diagnostic read.
    pub fn new(
        run_id: impl Into<String>,
        row_id: impl Into<String>,
        phase: impl Into<String>,
        language: impl Into<String>,
        api_surface: impl Into<String>,
        operation: impl Into<String>,
        trace_clock: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            row_id: row_id.into(),
            repeat_index: 0,
            phase: phase.into(),
            language: language.into(),
            api_surface: api_surface.into(),
            operation: operation.into(),
            trace_clock: trace_clock.into(),
        }
    }

    /// Sets the repeat index included in native trace metadata.
    pub fn with_repeat_index(mut self, repeat_index: u32) -> Self {
        self.repeat_index = repeat_index;
        self
    }
}

/// Native query-attribution trace JSON copied into Rust memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTraceJson {
    /// JSON text following the native `tio_query_attribution_trace.v1` schema.
    pub json: String,
}

impl QueryTraceJson {
    /// Returns the owned trace JSON text as `str`.
    pub fn as_str(&self) -> &str {
        &self.json
    }

    /// Consumes the trace wrapper and returns the owned JSON text.
    pub fn into_string(self) -> String {
        self.json
    }
}

/// Current attributed read value with execution metadata and diagnostic trace JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributedReadResult<T> {
    /// Read value.
    pub value: T,
    /// Execution metadata.
    pub execution: ReadExecutionReport,
    /// Query-attribution trace JSON copied from native-owned output.
    pub trace: QueryTraceJson,
}

/// Historical read options with execution mode only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadWithOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
}

impl HistoricalReadWithOptions {
    /// Serial historical read execution.
    pub fn serial() -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
        }
    }

    /// Parallel historical read execution with the provided maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
        }
    }
}

/// Historical read options with execution mode and shape policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadWithShapePolicyOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
    /// Shape policy evaluated against the selected historical snapshot.
    pub shape_policy: ReadShapePolicy,
}

impl HistoricalReadWithShapePolicyOptions {
    /// Serial historical read with the provided shape policy.
    pub fn serial(shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
            shape_policy,
        }
    }

    /// Parallel historical read with the provided maximum thread count and shape policy.
    pub fn parallel_threads(max_threads: usize, shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
            shape_policy,
        }
    }
}

/// Safe selector for current and historical read APIs and scoped mutation APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySelector {
    /// Select all indices along this axis.
    All,
    /// Select a half-open range along this axis.
    Range { start: u32, end: u32 },
    /// Select explicit indices along this axis.
    Take(Vec<u32>),
}

/// Basic read-index item for the native `read_index` lowering path.
///
/// This intentionally exposes the bounded C ABI first slice: `all`, `slice`, scalar `index`,
/// `new_axis`, and `ellipsis`. Advanced array/mask indexing is not part of this API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadIndexItem {
    /// Select all values along one input axis.
    All,
    /// Select a Python-style half-open slice with optional start/end and a non-zero step.
    Slice {
        /// Optional inclusive start bound.
        start: Option<i64>,
        /// Optional exclusive end bound.
        end: Option<i64>,
        /// Slice step; must be non-zero.
        step: i64,
    },
    /// Select a single scalar index along one input axis.
    Index(i64),
    /// Insert a length-one output axis.
    NewAxis,
    /// Expand to the remaining input axes during native normalization.
    Ellipsis,
}

impl ReadIndexItem {
    /// Selects all values along one input axis.
    pub fn all() -> Self {
        Self::All
    }

    /// Creates a bounded or open-ended slice with a non-zero step.
    pub fn slice(start: Option<i64>, end: Option<i64>, step: i64) -> Result<Self> {
        if step == 0 {
            return Err(TioError::invalid_argument(
                "read_index slice step must not be zero",
            ));
        }
        Ok(Self::Slice { start, end, step })
    }

    /// Selects a single scalar index along one input axis.
    pub fn index(index: i64) -> Self {
        Self::Index(index)
    }

    /// Inserts a length-one output axis.
    pub fn new_axis() -> Self {
        Self::NewAxis
    }

    /// Expands to the remaining input axes during native normalization.
    pub fn ellipsis() -> Self {
        Self::Ellipsis
    }

    pub(crate) fn to_raw(&self) -> Result<sys::ArcadiaTioReadIndexItem> {
        match self {
            Self::All => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_ALL)),
            Self::Slice { start, end, step } => {
                if *step == 0 {
                    return Err(TioError::invalid_argument(
                        "read_index slice step must not be zero",
                    ));
                }
                Ok(sys::ArcadiaTioReadIndexItem {
                    kind: sys::ARCADIA_TIO_READ_INDEX_SLICE,
                    has_start: u8::from(start.is_some()),
                    start: start.unwrap_or_default(),
                    has_end: u8::from(end.is_some()),
                    end: end.unwrap_or_default(),
                    step: *step,
                    index: 0,
                })
            }
            Self::Index(index) => {
                let mut raw = raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_INDEX);
                raw.index = *index;
                Ok(raw)
            }
            Self::NewAxis => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_NEW_AXIS)),
            Self::Ellipsis => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_ELLIPSIS)),
        }
    }
}

pub(crate) fn raw_read_index_item(
    kind: sys::ArcadiaTioReadIndexItemTag,
) -> sys::ArcadiaTioReadIndexItem {
    sys::ArcadiaTioReadIndexItem {
        kind,
        has_start: 0,
        start: 0,
        has_end: 0,
        end: 0,
        step: 1,
        index: 0,
    }
}

/// Chunk key used by clear-block mutation APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkKey {
    pub(crate) coords: Vec<u32>,
}

impl ChunkKey {
    /// Creates a chunk key from chunk coordinates.
    pub fn new(coords: Vec<u32>) -> Self {
        Self { coords }
    }

    /// Returns the chunk coordinates.
    pub fn coords(&self) -> &[u32] {
        &self.coords
    }
}

impl From<Vec<u32>> for ChunkKey {
    fn from(coords: Vec<u32>) -> Self {
        Self::new(coords)
    }
}

/// Current read execution metadata copied from the native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadExecutionReport {
    /// Requested execution mode.
    pub requested_mode: ReadExecutionMode,
    /// Requested maximum query threads.
    pub query_max_threads: usize,
    /// Effective execution mode.
    pub query_effective_mode: ReadExecutionMode,
    /// Effective query threads.
    pub query_effective_threads: usize,
    /// Query parallel runtime if reported.
    pub query_parallel_runtime: Option<String>,
    /// Query parallel fallback reason if reported.
    pub query_parallel_fallback_reason: Option<String>,
    /// Query parallel reason code if reported.
    pub query_parallel_reason_code: Option<String>,
    /// Query parallel reason-code taxonomy if reported.
    pub query_parallel_reason_code_taxonomy: Option<String>,
}

/// Native lowering path reported by `read_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadIndexLoweringKind {
    /// Native code did not report a recognized lowering path.
    Unknown,
    /// Lowered directly to selector reads.
    SelectorRead,
    /// Lowered to selector reads plus shape post-processing for scalar/new-axis items.
    SelectorReadWithShapePostprocess,
}

impl ReadIndexLoweringKind {
    pub(crate) fn from_raw(value: sys::ArcadiaTioReadIndexLoweringKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ => Ok(Self::SelectorRead),
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ_WITH_SHAPE_POSTPROCESS => {
                Ok(Self::SelectorReadWithShapePostprocess)
            }
            other => Err(TioError::conversion(format!(
                "unknown read_index lowering kind value {other}"
            ))),
        }
    }
}

/// Rust-owned `read_index` lowering report copied from native output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadIndexReport {
    /// Lowering strategy selected by native code.
    pub lowering_kind: ReadIndexLoweringKind,
    /// Whether native code used a full-tensor fallback.
    pub used_full_tensor_fallback: bool,
}

/// Current read-index value with lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadIndexResult {
    /// Read value.
    pub value: Tensor,
    /// Lowering metadata.
    pub report: ReadIndexReport,
}

/// Current dense read-index value with validity mask and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadIndexDenseResult {
    /// Dense read value and optional validity mask.
    pub value: DenseTensor,
    /// Lowering metadata.
    pub report: ReadIndexReport,
}

/// Historical read-index execution and lowering metadata copied from native output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadIndexReport {
    /// Historical execution metadata.
    pub execution: HistoricalReadExecutionReport,
    /// Read-index lowering metadata.
    pub read_index: ReadIndexReport,
}

/// Historical read-index value with execution and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadIndexResult {
    /// Read value.
    pub value: Tensor,
    /// Historical execution and lowering metadata.
    pub report: HistoricalReadIndexReport,
}

/// Historical dense read-index value with validity mask plus execution and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadIndexDenseResult {
    /// Dense read value and optional validity mask.
    pub value: DenseTensor,
    /// Historical execution and lowering metadata.
    pub report: HistoricalReadIndexReport,
}

/// RAII owner for an Arrow C Data array/schema pair returned by native full-value export.
///
/// The pointers exposed by [`ArrowCData::array`] and [`ArrowCData::schema`] are borrowed and remain
/// valid only while this value is alive. Dropping this value invokes non-null Arrow `release`
/// callbacks exactly once. This is a bounded interop surface; it is not a generic zero-copy or
/// performance guarantee.
pub struct ArrowCData {
    pub(crate) array: sys::ArrowArray,
    pub(crate) schema: sys::ArrowSchema,
    pub(crate) _not_send_or_sync: PhantomData<Rc<()>>,
}

impl ArrowCData {
    /// Returns the borrowed Arrow C Data array carrier.
    pub fn array(&self) -> &sys::ArrowArray {
        &self.array
    }

    /// Returns the borrowed Arrow C Data schema carrier.
    pub fn schema(&self) -> &sys::ArrowSchema {
        &self.schema
    }

    /// Returns a raw borrowed pointer to the Arrow C Data array carrier.
    pub fn array_ptr(&self) -> *const sys::ArrowArray {
        &self.array
    }

    /// Returns a raw borrowed pointer to the Arrow C Data schema carrier.
    pub fn schema_ptr(&self) -> *const sys::ArrowSchema {
        &self.schema
    }
}

impl Drop for ArrowCData {
    fn drop(&mut self) {
        // SAFETY: The native Arrow C Data contract transfers ownership of any non-null release
        // callbacks to the caller. This RAII owner invokes each callback at most once on drop.
        unsafe {
            release_arrow_array(&mut self.array);
            release_arrow_schema(&mut self.schema);
        }
    }
}

pub(crate) unsafe fn release_arrow_array(array: *mut sys::ArrowArray) {
    // SAFETY: Caller guarantees `array` is a writable ArrowArray slot. A non-null release callback
    // means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*array).release } {
        unsafe { release(array) };
    }
}

pub(crate) unsafe fn release_arrow_schema(schema: *mut sys::ArrowSchema) {
    // SAFETY: Caller guarantees `schema` is a writable ArrowSchema slot. A non-null release
    // callback means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*schema).release } {
        unsafe { release(schema) };
    }
}

impl fmt::Debug for ArrowCData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = f.debug_struct("ArrowCData");
        debug
            .field("array_length", &self.array.length)
            .field("array_n_buffers", &self.array.n_buffers)
            .field("array_n_children", &self.array.n_children);
        match optional_c_string(self.schema.format) {
            Ok(schema_format) => {
                debug.field("schema_format", &schema_format);
            }
            Err(error) => {
                debug.field("schema_format_error", &error);
            }
        }
        debug.finish_non_exhaustive()
    }
}

/// Historical query source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoricalQuerySourceKind {
    /// Query used a retained visible commit snapshot.
    RetainedVisibleCommit,
}

impl HistoricalQuerySourceKind {
    pub(crate) fn from_raw(value: sys::ArcadiaTioHistoricalQuerySourceKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT => {
                Ok(Self::RetainedVisibleCommit)
            }
            other => Err(TioError::conversion(format!(
                "unknown historical query source kind value {other}"
            ))),
        }
    }
}

/// Historical read execution metadata copied from the native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadExecutionReport {
    /// Current-read execution fields.
    pub execution: ReadExecutionReport,
    /// Historical query source kind.
    pub query_source_kind: HistoricalQuerySourceKind,
    /// Commit sequence used for the historical query.
    pub query_commit_seq: u64,
}

/// Current read value with execution metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult<T> {
    /// Read value.
    pub value: T,
    /// Execution metadata.
    pub execution: ReadExecutionReport,
}

/// Current Coordinate v2 lookup result plus an optional payload read.
#[derive(Debug, Clone, PartialEq)]
pub struct CoordinateReadResult<T> {
    /// Status-rich Coordinate v2 lookup result.
    pub lookup: CoordinateLookupResultV2,
    /// Payload read when the lookup result is readable for this helper.
    pub read: Option<ReadResult<T>>,
}

/// Historical read value with execution metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadResult<T> {
    /// Read value.
    pub value: T,
    /// Historical execution metadata.
    pub execution: HistoricalReadExecutionReport,
}

/// Historical Coordinate v2 lookup result plus an optional payload read.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalCoordinateReadResult<T> {
    /// Status-rich Coordinate v2 lookup result.
    pub lookup: CoordinateLookupResultV2,
    /// Payload read when the lookup result is readable for this helper.
    pub read: Option<HistoricalReadResult<T>>,
}

/// Compaction mode used by compaction workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionMode {
    /// Copy live data without reblocking.
    #[default]
    CopyLive,
    /// Reblock live data with the requested entry block size.
    Reblock { entry_block_size: u32 },
}

impl CompactionMode {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCompactionMode {
        match self {
            Self::CopyLive => sys::ArcadiaTioCompactionMode {
                kind: sys::ARCADIA_TIO_COMPACTION_COPY_LIVE,
                reblock_entry_block_size: 0,
            },
            Self::Reblock { entry_block_size } => sys::ArcadiaTioCompactionMode {
                kind: sys::ARCADIA_TIO_COMPACTION_REBLOCK,
                reblock_entry_block_size: entry_block_size,
            },
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCompactionMode) -> Result<Self> {
        match value.kind {
            sys::ARCADIA_TIO_COMPACTION_COPY_LIVE => Ok(Self::CopyLive),
            sys::ARCADIA_TIO_COMPACTION_REBLOCK => Ok(Self::Reblock {
                entry_block_size: value.reblock_entry_block_size,
            }),
            other => Err(TioError::conversion(format!(
                "unknown compaction mode value {other}"
            ))),
        }
    }
}

/// Shallow compatibility compaction stats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionStats {
    /// Native-reported live bytes.
    pub live_bytes: u64,
    /// Native-reported dead bytes.
    pub dead_bytes: u64,
    /// Native-reported dead-byte ratio.
    pub dead_ratio: f64,
    /// Number of commits represented by the file.
    pub commit_count: u32,
}

/// Status returned by status-aware V4 report APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4ReportStatus {
    /// Report completed.
    Complete,
    /// Report family is unsupported for this file/operation.
    Unsupported,
    /// Report outcome is unknown.
    Unknown,
    /// A future native status value preserved in-band.
    Other(i32),
}

impl V4ReportStatus {
    pub(crate) fn from_raw(value: sys::ArcadiaTioV4ReportStatus) -> Self {
        match value {
            sys::ARCADIA_TIO_V4_REPORT_COMPLETE => Self::Complete,
            sys::ARCADIA_TIO_V4_REPORT_UNSUPPORTED => Self::Unsupported,
            sys::ARCADIA_TIO_V4_REPORT_UNKNOWN => Self::Unknown,
            other => Self::Other(other),
        }
    }
}

/// Precise-accounting field ids that can be requested or omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4PreciseAccountingField {
    /// Source-file bytes unreachable from the selected report view.
    UnreachableBytes,
    /// Bytes required to retain requested history.
    RetainedHistoryRequiredBytes,
    /// Bytes skipped due to pop/revert semantics.
    PoppedSkippedBytes,
    /// Bytes reclaimable by the selected workflow.
    ReclaimableBytes,
    /// A future native precise-accounting field id preserved in-band.
    Other(i32),
}

impl V4PreciseAccountingField {
    /// Returns this field's single-bit request mask.
    pub fn mask(self) -> u32 {
        match self.to_raw() {
            raw if raw >= 0 && raw < u32::BITS as i32 => 1u32 << raw,
            _ => 0,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioV4PreciseAccountingField) -> Self {
        match value {
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES => Self::UnreachableBytes,
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES => {
                Self::RetainedHistoryRequiredBytes
            }
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES => Self::PoppedSkippedBytes,
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES => Self::ReclaimableBytes,
            other => Self::Other(other),
        }
    }

    pub(crate) fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingField {
        match self {
            Self::UnreachableBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES,
            Self::RetainedHistoryRequiredBytes => {
                sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES
            }
            Self::PoppedSkippedBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES,
            Self::ReclaimableBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES,
            Self::Other(value) => value,
        }
    }
}

/// Options for precise-accounting report APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4PreciseAccountingOptions {
    /// Zero requests every precise field relevant to the report family.
    pub requested_fields_mask: u32,
    /// Whether native should include human-readable omitted-field reason strings.
    pub include_omitted_field_reasons: bool,
}

impl V4PreciseAccountingOptions {
    /// Requests every precise-accounting field relevant to the report family.
    pub fn all() -> Self {
        Self {
            requested_fields_mask: 0,
            include_omitted_field_reasons: true,
        }
    }

    /// Requests only the provided precise-accounting fields.
    pub fn fields(fields: impl IntoIterator<Item = V4PreciseAccountingField>) -> Self {
        Self {
            requested_fields_mask: fields
                .into_iter()
                .fold(0u32, |mask, field| mask | field.mask()),
            include_omitted_field_reasons: true,
        }
    }

    pub(crate) fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingOptions {
        sys::ArcadiaTioV4PreciseAccountingOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioV4PreciseAccountingOptions>(),
            requested_fields_mask: self.requested_fields_mask,
            include_omitted_field_reasons: u8::from(self.include_omitted_field_reasons),
        }
    }
}

impl Default for V4PreciseAccountingOptions {
    fn default() -> Self {
        Self::all()
    }
}

/// Omitted precise-accounting field metadata copied from a native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4OmittedPreciseAccountingField {
    /// Omitted field id.
    pub field: V4PreciseAccountingField,
    /// Optional human-readable reason.
    pub reason: Option<String>,
    /// Optional stable reason code aligned with this omitted field.
    pub reason_code: Option<String>,
}

/// Precise-accounting bytes with native validity flags preserved as `Option` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4PreciseAccountingBytes {
    /// Precise unreachable bytes when available.
    pub unreachable_bytes: Option<u64>,
    /// Precise retained-history-required bytes when available.
    pub retained_history_required_bytes: Option<u64>,
    /// Precise popped/skipped bytes when available.
    pub popped_skipped_bytes: Option<u64>,
    /// Precise reclaimable bytes when available.
    pub reclaimable_bytes: Option<u64>,
    /// Fields intentionally omitted by native accounting.
    pub omitted_fields: Vec<V4OmittedPreciseAccountingField>,
}

/// V4 current-head byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4CurrentHeadBytes {
    /// Payload bytes.
    pub payload_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// Epoch bytes.
    pub epoch_bytes: u64,
    /// Auxiliary bytes.
    pub aux_bytes: u64,
    /// Commit bytes.
    pub commit_bytes: u64,
}

/// V4 visible-chain audit byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4AuditBytes {
    /// Commit bytes.
    pub commit_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// Epoch bytes.
    pub epoch_bytes: u64,
    /// Auxiliary bytes.
    pub aux_bytes: u64,
}

/// V4 payload-reuse byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4PayloadReuseBytes {
    /// Payload bytes resurrected from previous commits.
    pub resurrected_payload_bytes: u64,
    /// Payload bytes shared with other visible data.
    pub shared_payload_bytes: u64,
}

/// V4 superseded byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4SupersededBytes {
    /// Superseded payload bytes.
    pub payload_bytes: u64,
    /// Superseded index bytes.
    pub index_bytes: u64,
    /// Superseded epoch bytes.
    pub epoch_bytes: u64,
    /// Superseded auxiliary bytes.
    pub aux_bytes: u64,
}

/// V4 compaction analysis policy reported by the native API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4CompactionAnalysisPolicy {
    /// Analyze compaction to the current visible state.
    CompactToCurrentState,
}

impl V4CompactionAnalysisPolicy {
    pub(crate) fn from_raw(value: sys::ArcadiaTioV4CompactionAnalysisPolicy) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE => {
                Ok(Self::CompactToCurrentState)
            }
            other => Err(TioError::conversion(format!(
                "unknown V4 compaction analysis policy value {other}"
            ))),
        }
    }
}

/// Non-precise V4 source-file diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4DiagnosticsReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Current-head byte breakdown.
    pub current_head: V4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: V4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: V4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: V4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise V4 source-file diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4DiagnosticsPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Current-head byte breakdown.
    pub current_head: V4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: V4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: V4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: V4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting bytes and omitted-field metadata.
    pub precise_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Non-precise V4 ordinary compaction analysis report.
#[derive(Debug, Clone, PartialEq)]
pub struct V4CompactionAnalysisReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Native compaction policy analyzed.
    pub policy: V4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise V4 ordinary compaction analysis report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4CompactionAnalysisPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Native compaction policy analyzed.
    pub policy: V4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting bytes and omitted-field metadata.
    pub precise_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Options for ordinary compaction helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionOptions {
    /// Number of commits to retain.
    pub retain_commits: u32,
    /// Compaction mode.
    pub mode: CompactionMode,
    /// Dead-byte ratio threshold for conditional compaction.
    pub dead_ratio_threshold: f64,
    /// Minimum dead bytes for conditional compaction.
    pub min_dead_bytes: u64,
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            retain_commits: 1,
            mode: CompactionMode::CopyLive,
            dead_ratio_threshold: 0.3,
            min_dead_bytes: 0,
        }
    }
}

/// Auto-compaction metadata configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoCompactionConfig {
    /// Whether auto-compaction is enabled.
    pub enabled: bool,
    /// Number of commits to retain.
    pub retain_commits: u32,
    /// Dead-byte ratio threshold.
    pub dead_ratio_threshold: f64,
    /// Minimum dead bytes before auto-compaction can trigger.
    pub min_dead_bytes: u64,
    /// Compaction mode.
    pub mode: CompactionMode,
    /// Commit interval for auto-compaction checks.
    pub check_every_commits: u32,
    /// Commit cooldown after compaction.
    pub cooldown_commits: u32,
}

impl Default for AutoCompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retain_commits: 1,
            dead_ratio_threshold: 0.3,
            min_dead_bytes: 0,
            mode: CompactionMode::CopyLive,
            check_every_commits: 1,
            cooldown_commits: 0,
        }
    }
}

impl AutoCompactionConfig {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioAutoCompactionConfig {
        sys::ArcadiaTioAutoCompactionConfig {
            enabled: u8::from(self.enabled),
            retain_commits: self.retain_commits,
            dead_ratio_threshold: self.dead_ratio_threshold,
            min_dead_bytes: self.min_dead_bytes,
            mode: self.mode.to_raw(),
            check_every_commits: self.check_every_commits,
            cooldown_commits: self.cooldown_commits,
        }
    }
}

/// Auto-compaction state metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionState {
    /// Last compacted commit sequence.
    pub last_compacted_commit_seq: u64,
    /// Last compaction timestamp in Unix milliseconds.
    pub last_compacted_at_unix_ms: u64,
}

/// Reform target layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReformTargetLayout {
    /// Preserve the source layout family.
    PreserveFamily,
    /// Reform to WholeAppendUnit.
    WholeAppendUnit,
    /// Reform to RegularChunked.
    RegularChunked,
}

impl ReformTargetLayout {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioReformTargetLayout {
        match self {
            Self::PreserveFamily => sys::ARCADIA_TIO_REFORM_TARGET_PRESERVE_FAMILY,
            Self::WholeAppendUnit => sys::ARCADIA_TIO_REFORM_TARGET_WHOLE_APPEND_UNIT,
            Self::RegularChunked => sys::ARCADIA_TIO_REFORM_TARGET_REGULAR_CHUNKED,
        }
    }
}

/// Safe reform policy/options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformOptions {
    /// Target layout family.
    pub target_layout: ReformTargetLayout,
    /// RegularChunked block shape used when target_layout is RegularChunked.
    pub regular_chunked_block_shape: Vec<u32>,
}

impl ReformOptions {
    /// Builds options that preserve the source layout family.
    pub fn preserve_family() -> Self {
        Self {
            target_layout: ReformTargetLayout::PreserveFamily,
            regular_chunked_block_shape: Vec::new(),
        }
    }

    /// Builds options targeting WholeAppendUnit.
    pub fn whole_append_unit() -> Self {
        Self {
            target_layout: ReformTargetLayout::WholeAppendUnit,
            regular_chunked_block_shape: Vec::new(),
        }
    }

    /// Builds options targeting RegularChunked with a native block shape.
    pub fn regular_chunked(block_shape: Vec<u32>) -> Self {
        Self {
            target_layout: ReformTargetLayout::RegularChunked,
            regular_chunked_block_shape: block_shape,
        }
    }

    pub(crate) fn to_raw(&self) -> sys::ArcadiaTioReformOptions {
        let block_shape_ptr = if self.regular_chunked_block_shape.is_empty() {
            ptr::null()
        } else {
            self.regular_chunked_block_shape.as_ptr()
        };
        sys::ArcadiaTioReformOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReformOptions>(),
            target_layout: self.target_layout.to_raw(),
            regular_chunked_block_shape: block_shape_ptr,
            regular_chunked_block_shape_len: self.regular_chunked_block_shape.len(),
        }
    }
}

/// Native reform diagnostic report copied into owned Rust strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformReport {
    /// Stable reason code if reported.
    pub reason_code: Option<String>,
    /// Reason-code taxonomy if reported.
    pub reason_code_taxonomy: Option<String>,
    /// Human-readable reason if reported.
    pub reason: Option<String>,
}

/// Retained-history compaction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4RetainedHistoryPolicy {
    /// Retain the last N visible commits.
    RetainLast,
}

impl V4RetainedHistoryPolicy {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryPolicy {
        match self {
            Self::RetainLast => sys::ARCADIA_TIO_V4_RETAINED_HISTORY_RETAIN_LAST,
        }
    }
}

/// Retained-history compaction options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionOptions {
    /// Retained-history policy.
    pub policy: V4RetainedHistoryPolicy,
    /// Number of latest commits to retain for retain-last.
    pub retain_last_n: u32,
}

impl V4RetainedHistoryCompactionOptions {
    /// Builds retain-last retained-history compaction options.
    pub fn retain_last(retain_last_n: u32) -> Self {
        Self {
            policy: V4RetainedHistoryPolicy::RetainLast,
            retain_last_n,
        }
    }

    pub(crate) fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryCompactionOptions {
        sys::ArcadiaTioV4RetainedHistoryCompactionOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionOptions>(),
            policy: self.policy.to_raw(),
            retain_last_n: self.retain_last_n,
        }
    }
}

impl Default for V4RetainedHistoryCompactionOptions {
    fn default() -> Self {
        Self::retain_last(1)
    }
}

/// Non-precise retained-history compaction report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Number of retained commits.
    pub retained_commit_count: u32,
    /// Retained commit sequence numbers.
    pub retained_commit_seqs: Vec<u64>,
    /// Optional count of older commits not retained.
    pub unretained_older_commit_count: Option<u64>,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise retained-history compaction report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Number of retained commits.
    pub retained_commit_count: u32,
    /// Retained commit sequence numbers.
    pub retained_commit_seqs: Vec<u64>,
    /// Optional count of older commits not retained.
    pub unretained_older_commit_count: Option<u64>,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Source-file precise accounting at retained-history compaction time.
    pub precise_source_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Create-time storage/layout profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateLayout {
    /// Streaming V4 create path.
    Streaming,
    /// Random-access V4 create path.
    RandomAccess,
}

/// Storage profile selector for RegularChunked policy create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProfile {
    /// Balanced default profile.
    Balanced,
    /// NVMe-oriented profile.
    Nvme,
    /// HDD-oriented profile.
    Hdd,
}

impl StorageProfile {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioStorageProfile {
        match self {
            Self::Balanced => sys::ARCADIA_TIO_STORAGE_BALANCED,
            Self::Nvme => sys::ARCADIA_TIO_STORAGE_NVME,
            Self::Hdd => sys::ARCADIA_TIO_STORAGE_HDD,
        }
    }
}

/// Storage access hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageAccessKind {
    /// Seekable mounted storage.
    SeekableMounted,
    /// Remote storage with range-read capability.
    RemoteRangeRead,
    /// Forward-only storage.
    ForwardOnly,
}

impl StorageAccessKind {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioStorageAccessKind {
        match self {
            Self::SeekableMounted => sys::ARCADIA_TIO_STORAGE_ACCESS_SEEKABLE_MOUNTED,
            Self::RemoteRangeRead => sys::ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ,
            Self::ForwardOnly => sys::ARCADIA_TIO_STORAGE_ACCESS_FORWARD_ONLY,
        }
    }
}

/// Expected open/query pattern hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPattern {
    /// Metadata-hot open pattern.
    MetadataHot,
    /// Data-hot open pattern.
    DataHot,
    /// Mixed metadata/data open pattern.
    Mixed,
}

impl OpenPattern {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioOpenPattern {
        match self {
            Self::MetadataHot => sys::ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT,
            Self::DataHot => sys::ARCADIA_TIO_OPEN_PATTERN_DATA_HOT,
            Self::Mixed => sys::ARCADIA_TIO_OPEN_PATTERN_MIXED,
        }
    }
}

/// File population hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePopulation {
    /// Few long-lived files.
    FewLongLived,
    /// Many shard files.
    ManyShards,
}

impl FilePopulation {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioFilePopulation {
        match self {
            Self::FewLongLived => sys::ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED,
            Self::ManyShards => sys::ARCADIA_TIO_FILE_POPULATION_MANY_SHARDS,
        }
    }
}

/// Metadata stability hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataStability {
    /// Metadata is expected to remain stable.
    Stable,
    /// Metadata is expected to grow.
    Growing,
}

impl MetadataStability {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioMetadataStability {
        match self {
            Self::Stable => sys::ARCADIA_TIO_METADATA_STABILITY_STABLE,
            Self::Growing => sys::ARCADIA_TIO_METADATA_STABILITY_GROWING,
        }
    }
}

/// Policy options for RegularChunked create helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePolicyOptions {
    /// Non-append axes that should be chunked.
    pub chunk_axes: Vec<usize>,
    /// Storage profile used by the native policy planner.
    pub storage_profile: StorageProfile,
    /// Typical query sizes, one per rank axis. Use 0 for unspecified axes.
    pub typical_query_sizes: Vec<u32>,
}

impl CreatePolicyOptions {
    /// Creates RegularChunked policy options with a balanced storage profile.
    pub fn new(chunk_axes: Vec<usize>, typical_query_sizes: Vec<u32>) -> Self {
        Self {
            chunk_axes,
            storage_profile: StorageProfile::Balanced,
            typical_query_sizes,
        }
    }
}

/// Inferred layout-family create hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateInferredOptions {
    /// Storage access kind.
    pub storage_access: StorageAccessKind,
    /// Expected open pattern.
    pub open_pattern: OpenPattern,
    /// File population hint.
    pub file_population: FilePopulation,
    /// Metadata stability hint.
    pub metadata_stability: MetadataStability,
}

impl CreateInferredOptions {
    /// Conservative default inferred-create hints.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for CreateInferredOptions {
    fn default() -> Self {
        Self {
            storage_access: StorageAccessKind::SeekableMounted,
            open_pattern: OpenPattern::MetadataHot,
            file_population: FilePopulation::FewLongLived,
            metadata_stability: MetadataStability::Stable,
        }
    }
}

/// Owned create options for the first wrapper slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOptions {
    /// Payload dtype.
    pub dtype: DType,
    /// Dimension descriptors.
    pub dims: Vec<DimSpec>,
    /// Append dimension index.
    pub append_dim: usize,
    /// Create layout/profile.
    pub layout: CreateLayout,
    /// Symbol labels.
    pub symbols: Vec<String>,
    /// Channel labels.
    pub channels: Vec<String>,
    /// User metadata key/value pairs.
    pub user_kv: Vec<(String, String)>,
    /// Optional coordinate descriptors.
    pub coordinates: Vec<CoordinateSpec>,
    /// Optional write-time compression policy override for future appends.
    ///
    /// `None` leaves the native persisted default in place (currently Auto/Zstd).
    /// Use `Some(CompressionConfig::uncompressed())` or
    /// `Some(CompressionConfig::zstd_level(...))` only when the caller needs an
    /// explicit override.
    pub compression: Option<CompressionConfig>,
}

impl CreateOptions {
    /// Builds streaming create options.
    pub fn streaming(dtype: DType, dims: Vec<DimSpec>, append_dim: usize) -> Self {
        Self {
            dtype,
            dims,
            append_dim,
            layout: CreateLayout::Streaming,
            symbols: Vec::new(),
            channels: Vec::new(),
            user_kv: Vec::new(),
            coordinates: Vec::new(),
            compression: None,
        }
    }

    /// Builds random-access create options.
    pub fn random_access(dtype: DType, dims: Vec<DimSpec>, append_dim: usize) -> Self {
        Self {
            layout: CreateLayout::RandomAccess,
            ..Self::streaming(dtype, dims, append_dim)
        }
    }
}

/// Write-time compression mode for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMode {
    /// Force uncompressed writes.
    ForceOff,
    /// Let the native writer choose according to the configured codec and threshold.
    Auto,
    /// Force the configured codec for future writes.
    ForceOn,
}

impl CompressionMode {
    /// Converts this safe mode to the raw C ABI mode value.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionMode {
        match self {
            Self::ForceOff => sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF,
            Self::Auto => sys::ARCADIA_TIO_COMPRESSION_AUTO,
            Self::ForceOn => sys::ARCADIA_TIO_COMPRESSION_FORCE_ON,
        }
    }

    /// Converts a raw C ABI mode value into a safe mode.
    pub fn from_raw(value: sys::ArcadiaTioCompressionMode) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF => Ok(Self::ForceOff),
            sys::ARCADIA_TIO_COMPRESSION_AUTO => Ok(Self::Auto),
            sys::ARCADIA_TIO_COMPRESSION_FORCE_ON => Ok(Self::ForceOn),
            other => Err(TioError::invalid_argument(format!(
                "unknown compression mode {other}"
            ))),
        }
    }
}

/// Write-time compression codec for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionCodec {
    /// Zstandard payload compression.
    Zstd,
}

impl CompressionCodec {
    /// Converts this safe codec to the raw C ABI codec value.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionCodec {
        match self {
            Self::Zstd => sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
        }
    }

    /// Converts a raw C ABI codec value into a safe codec.
    pub fn from_raw(value: sys::ArcadiaTioCompressionCodec) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD => Ok(Self::Zstd),
            sys::ARCADIA_TIO_COMPRESSION_CODEC_LZ4 => Err(TioError::unimplemented(
                "LZ4 V4 payload compression is not supported yet",
            )),
            other => Err(TioError::invalid_argument(format!(
                "unknown compression codec {other}"
            ))),
        }
    }
}

/// Write-time compression policy for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionConfig {
    /// Native compression mode.
    ///
    /// Prefer [`CompressionMode`] builders/accessors for ordinary code. This raw
    /// field remains public as a low-level compatibility escape hatch.
    pub mode: sys::ArcadiaTioCompressionMode,
    /// Native compression codec.
    ///
    /// Prefer [`CompressionCodec`] builders/accessors for ordinary code. This
    /// raw field remains public as a low-level compatibility escape hatch.
    pub codec: sys::ArcadiaTioCompressionCodec,
    /// Auto-mode minimum raw payload bytes.
    pub min_payload_bytes: u32,
    /// Zstd level.
    pub zstd_level: i32,
}

impl CompressionConfig {
    /// Minimum accepted zstd level.
    pub const ZSTD_MIN_LEVEL: i32 = -7;
    /// Maximum accepted zstd level.
    pub const ZSTD_MAX_LEVEL: i32 = 22;
    /// Default zstd level used by wrapper constructors.
    pub const DEFAULT_ZSTD_LEVEL: i32 = 3;
    /// Native default Auto/Zstd minimum raw payload threshold in bytes.
    pub const DEFAULT_MIN_PAYLOAD_BYTES: u32 = 256;

    /// Explicit uncompressed writes.
    pub fn uncompressed() -> Self {
        Self {
            mode: CompressionMode::ForceOff.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes: 0,
            zstd_level: Self::DEFAULT_ZSTD_LEVEL,
        }
    }

    /// Native Auto/Zstd writes with the native/default threshold.
    pub fn auto_zstd() -> Self {
        Self::auto_zstd_min_payload(Self::DEFAULT_MIN_PAYLOAD_BYTES)
    }

    /// Native Auto/Zstd writes with an explicit minimum raw payload threshold.
    pub fn auto_zstd_min_payload(min_payload_bytes: u32) -> Self {
        Self {
            mode: CompressionMode::Auto.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes,
            zstd_level: Self::DEFAULT_ZSTD_LEVEL,
        }
    }

    /// Explicit zstd writes at the requested level.
    ///
    /// This constructor preserves the historical source-compatible behavior of
    /// returning a config directly; call [`Self::try_zstd_level`] when the level
    /// should be checked before the config reaches a file operation.
    pub fn zstd_level(level: i32) -> Self {
        Self {
            mode: CompressionMode::ForceOn.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes: 0,
            zstd_level: level,
        }
    }

    /// Explicit zstd writes with early level validation.
    pub fn try_zstd_level(level: i32) -> Result<Self> {
        Self::zstd_level(level).validate()
    }

    /// Returns this config with a safe compression mode.
    pub fn with_mode(mut self, mode: CompressionMode) -> Self {
        self.mode = mode.to_raw();
        self
    }

    /// Returns this config with a safe compression codec.
    pub fn with_codec(mut self, codec: CompressionCodec) -> Self {
        self.codec = codec.to_raw();
        self
    }

    /// Returns this config with an Auto-mode payload threshold.
    pub fn with_min_payload_bytes(mut self, min_payload_bytes: u32) -> Self {
        self.min_payload_bytes = min_payload_bytes;
        self
    }

    /// Returns this config with a zstd level without changing historical late-validation behavior.
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = level;
        self
    }

    /// Returns this config with a zstd level, validating the resulting policy immediately.
    pub fn try_with_zstd_level(self, level: i32) -> Result<Self> {
        self.with_zstd_level(level).validate()
    }

    /// Returns the safe compression mode represented by the raw field.
    pub fn mode(&self) -> Result<CompressionMode> {
        CompressionMode::from_raw(self.mode)
    }

    /// Returns the safe compression codec represented by the raw field.
    pub fn codec(&self) -> Result<CompressionCodec> {
        CompressionCodec::from_raw(self.codec)
    }

    /// Validates raw compatibility fields before a native call.
    pub fn validate(self) -> Result<Self> {
        CompressionMode::from_raw(self.mode)?;
        CompressionCodec::from_raw(self.codec)?;
        if !(Self::ZSTD_MIN_LEVEL..=Self::ZSTD_MAX_LEVEL).contains(&self.zstd_level) {
            return Err(TioError::invalid_argument(format!(
                "zstd_level must be within [{}, {}]",
                Self::ZSTD_MIN_LEVEL,
                Self::ZSTD_MAX_LEVEL
            )));
        }
        Ok(self)
    }

    /// Converts this policy to the raw C ABI config without validating raw compatibility fields.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionConfig {
        sys::ArcadiaTioCompressionConfig {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioCompressionConfig>(),
            mode: self.mode,
            codec: self.codec,
            min_payload_bytes: self.min_payload_bytes,
            zstd_level: self.zstd_level,
        }
    }

    /// Validates and converts this policy to the raw C ABI config.
    pub fn try_to_raw(self) -> Result<sys::ArcadiaTioCompressionConfig> {
        Ok(self.validate()?.to_raw())
    }

    /// Converts a raw C ABI compression config into a validated wrapper config.
    pub fn from_raw(raw: sys::ArcadiaTioCompressionConfig) -> Result<Self> {
        if raw.version != 1 {
            return Err(TioError::invalid_argument(format!(
                "unsupported compression config version {}",
                raw.version
            )));
        }
        let expected_size = mem::size_of::<sys::ArcadiaTioCompressionConfig>();
        if raw.struct_size != expected_size {
            return Err(TioError::invalid_argument(format!(
                "compression config struct_size must be {expected_size}"
            )));
        }
        Self {
            mode: raw.mode,
            codec: raw.codec,
            min_payload_bytes: raw.min_payload_bytes,
            zstd_level: raw.zstd_level,
        }
        .validate()
    }
}

impl From<CompressionMode> for sys::ArcadiaTioCompressionMode {
    fn from(value: CompressionMode) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionMode> for CompressionMode {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionMode) -> Result<Self> {
        Self::from_raw(value)
    }
}

impl From<CompressionCodec> for sys::ArcadiaTioCompressionCodec {
    fn from(value: CompressionCodec) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionCodec> for CompressionCodec {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionCodec) -> Result<Self> {
        Self::from_raw(value)
    }
}

impl From<CompressionConfig> for sys::ArcadiaTioCompressionConfig {
    fn from(value: CompressionConfig) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionConfig> for CompressionConfig {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionConfig) -> Result<Self> {
        Self::from_raw(value)
    }
}
