#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::mem::{self, MaybeUninit};
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::slice;

use arcadia_tio_sys as sys;

/// Result type returned by the safe wrapper.
pub type Result<T> = std::result::Result<T, TioError>;

/// Error code surfaced by the C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// No error.
    Ok,
    /// Invalid argument.
    InvalidArgument,
    /// Operation is not implemented by the native library.
    Unimplemented,
    /// I/O failure.
    Io,
    /// FlatBuffers serialization/deserialization failure.
    Flatbuffers,
    /// Unknown native status code.
    Unknown(i32),
}

impl ErrorCode {
    fn from_raw(value: i32) -> Self {
        match value {
            sys::ARCADIA_TIO_ERROR_OK => Self::Ok,
            sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT => Self::InvalidArgument,
            sys::ARCADIA_TIO_ERROR_UNIMPLEMENTED => Self::Unimplemented,
            sys::ARCADIA_TIO_ERROR_IO => Self::Io,
            sys::ARCADIA_TIO_ERROR_FLATBUFFERS => Self::Flatbuffers,
            other => Self::Unknown(other),
        }
    }

    fn as_raw(self) -> i32 {
        match self {
            Self::Ok => sys::ARCADIA_TIO_ERROR_OK,
            Self::InvalidArgument => sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT,
            Self::Unimplemented => sys::ARCADIA_TIO_ERROR_UNIMPLEMENTED,
            Self::Io => sys::ARCADIA_TIO_ERROR_IO,
            Self::Flatbuffers => sys::ARCADIA_TIO_ERROR_FLATBUFFERS,
            Self::Unknown(value) => value,
        }
    }
}

/// Owned safe wrapper error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TioError {
    code: ErrorCode,
    message: String,
}

impl TioError {
    /// Returns the native/status error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the owned error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidArgument,
            message: message.into(),
        }
    }

    fn unimplemented(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Unimplemented,
            message: message.into(),
        }
    }

    fn from_last_error(fallback: &str) -> Self {
        // SAFETY: The C ABI exposes thread-local borrowed error storage. The wrapper copies the
        // string immediately into owned Rust memory before returning.
        let raw_code = unsafe { sys::arcadia_tio_last_error_code() };
        // SAFETY: The returned pointer is borrowed and may be null. It is only read for this call.
        let raw_message = unsafe { sys::arcadia_tio_last_error_message() };
        let message = if raw_message.is_null() {
            fallback.to_string()
        } else {
            // SAFETY: C ABI documents this as a NUL-terminated thread-local string.
            let copied = unsafe { CStr::from_ptr(raw_message) }
                .to_string_lossy()
                .into_owned();
            if copied.is_empty() {
                fallback.to_string()
            } else {
                copied
            }
        };
        Self {
            code: ErrorCode::from_raw(raw_code),
            message,
        }
    }

    fn conversion(message: impl Into<String>) -> Self {
        Self::invalid_argument(message)
    }

    fn with_reform_report(mut self, report: &ReformReport) -> Self {
        let mut details = Vec::new();
        if let Some(reason_code) = &report.reason_code {
            details.push(format!("reason_code={reason_code}"));
        }
        if let Some(taxonomy) = &report.reason_code_taxonomy {
            details.push(format!("reason_code_taxonomy={taxonomy}"));
        }
        if let Some(reason) = &report.reason {
            details.push(format!("reason={reason}"));
        }
        if !details.is_empty() {
            self.message = format!("{}; reform report: {}", self.message, details.join(", "));
        }
        self
    }
}

impl fmt::Display for TioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Arcadia TIO error {:?} ({}): {}",
            self.code,
            self.code.as_raw(),
            self.message
        )
    }
}

impl std::error::Error for TioError {}

fn status_result(status: i32, context: &str) -> Result<()> {
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(())
    } else {
        Err(TioError::from_last_error(context))
    }
}

fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    let path = path.as_ref();
    let text = path
        .to_str()
        .ok_or_else(|| TioError::invalid_argument("path must be valid UTF-8 for the C ABI"))?;
    CString::new(text).map_err(|_| TioError::invalid_argument("path contains an interior NUL byte"))
}

fn string_to_cstring(value: &str, label: &str) -> Result<CString> {
    CString::new(value)
        .map_err(|_| TioError::invalid_argument(format!("{label} contains an interior NUL byte")))
}

fn optional_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        // SAFETY: Native metadata strings are documented as NUL-terminated C strings owned by the
        // metadata object while it is alive. The wrapper copies them immediately.
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn required_c_string(ptr: *const c_char) -> String {
    optional_c_string(ptr).unwrap_or_default()
}

/// Payload dtype supported by the first safe wrapper slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    /// 32-bit floating point.
    F32,
    /// 64-bit floating point.
    F64,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
}

impl DType {
    fn to_raw(self) -> sys::ArcadiaTioDType {
        match self {
            Self::F32 => sys::ARCADIA_TIO_DTYPE_F32,
            Self::F64 => sys::ARCADIA_TIO_DTYPE_F64,
            Self::I32 => sys::ARCADIA_TIO_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_DTYPE_I64,
        }
    }

    fn from_raw(value: sys::ArcadiaTioDType) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_DTYPE_F32 => Ok(Self::F32),
            sys::ARCADIA_TIO_DTYPE_F64 => Ok(Self::F64),
            sys::ARCADIA_TIO_DTYPE_I32 => Ok(Self::I32),
            sys::ARCADIA_TIO_DTYPE_I64 => Ok(Self::I64),
            other => Err(TioError::conversion(format!("unknown dtype value {other}"))),
        }
    }

    /// Returns the number of bytes per scalar value for this dtype.
    pub fn size_bytes(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F64 | Self::I64 => 8,
        }
    }
}

/// Semantic axis kind used in create metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    /// Time axis.
    Time,
    /// Symbol axis.
    Symbol,
    /// Channel axis.
    Channel,
    /// Other axis.
    Other,
}

impl AxisKind {
    fn to_raw(self) -> sys::ArcadiaTioAxisKind {
        match self {
            Self::Time => sys::ARCADIA_TIO_AXIS_TIME,
            Self::Symbol => sys::ARCADIA_TIO_AXIS_SYMBOL,
            Self::Channel => sys::ARCADIA_TIO_AXIS_CHANNEL,
            Self::Other => sys::ARCADIA_TIO_AXIS_OTHER,
        }
    }

    fn from_raw(value: sys::ArcadiaTioAxisKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_AXIS_TIME => Ok(Self::Time),
            sys::ARCADIA_TIO_AXIS_SYMBOL => Ok(Self::Symbol),
            sys::ARCADIA_TIO_AXIS_CHANNEL => Ok(Self::Channel),
            sys::ARCADIA_TIO_AXIS_OTHER => Ok(Self::Other),
            other => Err(TioError::conversion(format!(
                "unknown axis kind value {other}"
            ))),
        }
    }
}

/// Effective header/profile reported by metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderProfile {
    /// Streaming profile.
    Streaming,
    /// Random-access profile.
    RandomAccess,
}

impl HeaderProfile {
    fn from_raw(value: sys::ArcadiaTioHeaderProfile) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_HEADER_PROFILE_STREAMING => Ok(Self::Streaming),
            sys::ARCADIA_TIO_HEADER_PROFILE_RANDOM_ACCESS => Ok(Self::RandomAccess),
            other => Err(TioError::conversion(format!(
                "unknown header profile value {other}"
            ))),
        }
    }
}

/// Shape/dimension metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimSpec {
    /// Semantic axis kind.
    pub kind: AxisKind,
    /// Current axis length.
    pub len: u32,
    /// Optional axis name.
    pub name: Option<String>,
}

impl DimSpec {
    /// Creates a dimension descriptor without a name.
    pub fn new(kind: AxisKind, len: u32) -> Self {
        Self {
            kind,
            len,
            name: None,
        }
    }

    /// Sets an axis name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Axis label metadata item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisLabel {
    /// Numeric label id assigned by the native metadata model.
    pub id: u32,
    /// Label name.
    pub name: String,
}

/// User metadata key/value item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserKv {
    /// Metadata key.
    pub key: String,
    /// Metadata value.
    pub value: String,
}

/// File metadata snapshot copied into Rust-owned values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// Payload dtype.
    pub dtype: DType,
    /// Dimension descriptors.
    pub dims: Vec<DimSpec>,
    /// Append dimension index.
    pub append_dim: usize,
    /// Symbol labels.
    pub symbols: Vec<AxisLabel>,
    /// Channel labels.
    pub channels: Vec<AxisLabel>,
    /// User metadata.
    pub user_kv: Vec<UserKv>,
    /// Effective header profile.
    pub effective_profile: HeaderProfile,
    /// Current head commit sequence.
    pub commit_seq: u64,
}

/// Owned tensor payload copied out of native C ABI buffers.
#[derive(Debug, Clone, PartialEq)]
pub enum TensorData {
    /// f32 payload data.
    F32(Vec<f32>),
    /// f64 payload data.
    F64(Vec<f64>),
    /// i32 payload data.
    I32(Vec<i32>),
    /// i64 payload data.
    I64(Vec<i64>),
}

impl TensorData {
    /// Returns the payload dtype.
    pub fn dtype(&self) -> DType {
        match self {
            Self::F32(_) => DType::F32,
            Self::F64(_) => DType::F64,
            Self::I32(_) => DType::I32,
            Self::I64(_) => DType::I64,
        }
    }

    /// Returns the number of scalar values.
    pub fn len(&self) -> usize {
        match self {
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    /// Returns true when there are no scalar values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Owned tensor copied into Rust memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    /// Payload dtype.
    pub dtype: DType,
    /// Tensor shape.
    pub shape: Vec<u64>,
    /// Owned tensor payload.
    pub data: TensorData,
}

impl Tensor {
    /// Returns the number of scalar values implied by the shape.
    pub fn element_len(&self) -> Result<usize> {
        shape_element_len(&self.shape)
    }
}

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
    fn to_raw(self) -> sys::ArcadiaTioSparseDetectorKind {
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
}

impl SparseValuePredicate {
    fn to_raw(self) -> sys::ArcadiaTioSparseValuePredicate {
        let (kind, value) = match self {
            Self::Nan => (sys::ARCADIA_TIO_SPARSE_PREDICATE_NAN, 0.0),
            Self::Zero => (sys::ARCADIA_TIO_SPARSE_PREDICATE_ZERO, 0.0),
            Self::EqualF32(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F32, value as f64),
            Self::EqualF64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F64, value),
        };
        sys::ArcadiaTioSparseValuePredicate { kind, value }
    }
}

/// Fallback policy when native sparse lowering is not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseFallbackPolicy {
    /// Preserve exact values by appending densely when sparse lowering cannot be used.
    Dense,
}

impl SparseFallbackPolicy {
    fn to_raw(self) -> sys::ArcadiaTioSparseFallbackPolicy {
        match self {
            Self::Dense => sys::ARCADIA_TIO_SPARSE_FALLBACK_DENSE,
        }
    }
}

/// Safe sparse-intent rule used by f32/f64/i32/i64 sparse analysis and append helpers.
///
/// Integer payloads currently support only [`SparseRule::null_subtensor`] and
/// [`SparseValuePredicate::Zero`]; exact integer predicates remain deferred.
///
/// A rule owns the sparse-axis list and threshold settings. The wrapper validates the owned
/// axes against the open file before calling the C ABI so borrowed raw pointers only live for a
/// single FFI call. Sparse-intent diagnostics describe the current native lowering decision; they
/// are not storage-efficiency, compression-ratio, layout-superiority, or capacity claims.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseRule {
    sparse_axes: Vec<usize>,
    detector: SparseDetector,
    predicate: SparseValuePredicate,
    min_absent_fraction: f64,
    min_absent_subtensors: u64,
    fallback: SparseFallbackPolicy,
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

    fn validate_for_append(&self, dtype: DType, rank: usize, append_axis: usize) -> Result<()> {
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
                (DType::F32, SparseValuePredicate::EqualF64(_)) => {
                    return Err(TioError::invalid_argument(
                        "f32 sparse append cannot use an EqualF64 predicate",
                    ));
                }
                (DType::F64, SparseValuePredicate::EqualF32(_)) => {
                    return Err(TioError::invalid_argument(
                        "f64 sparse append cannot use an EqualF32 predicate",
                    ));
                }
                (DType::I32 | DType::I64, SparseValuePredicate::Zero) => {}
                (DType::I32 | DType::I64, _) => {
                    return Err(TioError::invalid_argument(
                        "integer sparse append supports only NullSubtensor or Zero absence detection",
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
    fn from_raw(value: sys::ArcadiaTioSparseAppendOutcome) -> Result<Self> {
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
    fn from_raw(value: sys::ArcadiaTioSparseAppendReason) -> Result<Self> {
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

fn empty_sparse_append_analysis() -> sys::ArcadiaTioSparseAppendAnalysis {
    sys::ArcadiaTioSparseAppendAnalysis {
        outcome: sys::ARCADIA_TIO_SPARSE_APPEND_REJECT,
        absent_fraction: 0.0,
        absent_subtensor_count: 0,
        total_subtensor_count: 0,
        reasons: ptr::null_mut(),
        reasons_len: 0,
    }
}

struct SparseAppendAnalysisGuard<'a> {
    raw: &'a mut sys::ArcadiaTioSparseAppendAnalysis,
}

impl Drop for SparseAppendAnalysisGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: The guard is created only for raw analysis values initialized by this wrapper or
        // native sparse analysis. Native free tolerates empty/null reason buffers and nulls the raw
        // output after releasing any owned reasons, preventing accidental double-free by callers.
        unsafe { sys::arcadia_tio_sparse_append_analysis_free(self.raw) };
    }
}

fn take_sparse_append_analysis(
    raw: &mut sys::ArcadiaTioSparseAppendAnalysis,
) -> Result<SparseAppendAnalysis> {
    let guard = SparseAppendAnalysisGuard { raw };
    if guard.raw.reasons.is_null() && guard.raw.reasons_len != 0 {
        return Err(TioError::conversion(
            "native sparse append analysis returned null reasons with non-zero length",
        ));
    }
    let raw_reasons = if guard.raw.reasons_len == 0 {
        &[][..]
    } else {
        // SAFETY: Successful native analysis returns `reasons` pointing to `reasons_len` values.
        // The guard frees the native analysis exactly once after this function copies the values;
        // it also runs on conversion errors caused by unknown outcome or reason codes.
        unsafe { slice::from_raw_parts(guard.raw.reasons.cast_const(), guard.raw.reasons_len) }
    };
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
    fn to_raw(self) -> sys::ArcadiaTioAxisIdentityMode {
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
    fn to_raw_tag(&self) -> sys::ArcadiaTioReadShapePolicyTag {
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

    fn to_raw(self) -> Result<(sys::ArcadiaTioReadExecutionMode, usize)> {
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

    fn from_raw(value: sys::ArcadiaTioReadExecutionMode, threads: usize) -> Result<Self> {
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

    fn to_raw(&self) -> Result<sys::ArcadiaTioReadIndexItem> {
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

fn raw_read_index_item(kind: sys::ArcadiaTioReadIndexItemTag) -> sys::ArcadiaTioReadIndexItem {
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
    coords: Vec<u32>,
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
    fn from_raw(value: sys::ArcadiaTioReadIndexLoweringKind) -> Result<Self> {
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

/// RAII owner for an Arrow C Data array/schema pair returned by native full-value export.
///
/// The pointers exposed by [`ArrowCData::array`] and [`ArrowCData::schema`] are borrowed and remain
/// valid only while this value is alive. Dropping this value invokes non-null Arrow `release`
/// callbacks exactly once. This is a bounded interop surface; it is not a generic zero-copy or
/// performance guarantee.
pub struct ArrowCData {
    array: sys::ArrowArray,
    schema: sys::ArrowSchema,
    _not_send_or_sync: PhantomData<Rc<()>>,
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

unsafe fn release_arrow_array(array: *mut sys::ArrowArray) {
    // SAFETY: Caller guarantees `array` is a writable ArrowArray slot. A non-null release callback
    // means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*array).release } {
        unsafe { release(array) };
    }
}

unsafe fn release_arrow_schema(schema: *mut sys::ArrowSchema) {
    // SAFETY: Caller guarantees `schema` is a writable ArrowSchema slot. A non-null release
    // callback means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*schema).release } {
        unsafe { release(schema) };
    }
}

impl fmt::Debug for ArrowCData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrowCData")
            .field("array_length", &self.array.length)
            .field("array_n_buffers", &self.array.n_buffers)
            .field("array_n_children", &self.array.n_children)
            .field("schema_format", &optional_c_string(self.schema.format))
            .finish_non_exhaustive()
    }
}

/// Historical query source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoricalQuerySourceKind {
    /// Query used a retained visible commit snapshot.
    RetainedVisibleCommit,
}

impl HistoricalQuerySourceKind {
    fn from_raw(value: sys::ArcadiaTioHistoricalQuerySourceKind) -> Result<Self> {
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

/// Historical read value with execution metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadResult<T> {
    /// Read value.
    pub value: T,
    /// Historical execution metadata.
    pub execution: HistoricalReadExecutionReport,
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
    fn to_raw(self) -> sys::ArcadiaTioCompactionMode {
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

    fn from_raw(value: sys::ArcadiaTioCompactionMode) -> Result<Self> {
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
    fn from_raw(value: sys::ArcadiaTioV4ReportStatus) -> Self {
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

    fn from_raw(value: sys::ArcadiaTioV4PreciseAccountingField) -> Self {
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

    fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingField {
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

    fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingOptions {
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
    fn from_raw(value: sys::ArcadiaTioV4CompactionAnalysisPolicy) -> Result<Self> {
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
    fn to_raw(self) -> sys::ArcadiaTioAutoCompactionConfig {
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
    fn to_raw(self) -> sys::ArcadiaTioReformTargetLayout {
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

    fn to_raw(&self) -> sys::ArcadiaTioReformOptions {
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
    fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryPolicy {
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

    fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryCompactionOptions {
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
    fn to_raw(self) -> sys::ArcadiaTioStorageProfile {
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
    fn to_raw(self) -> sys::ArcadiaTioStorageAccessKind {
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
    fn to_raw(self) -> sys::ArcadiaTioOpenPattern {
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
    fn to_raw(self) -> sys::ArcadiaTioFilePopulation {
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
    fn to_raw(self) -> sys::ArcadiaTioMetadataStability {
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

/// Write-time compression policy for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionConfig {
    /// Native compression mode.
    pub mode: sys::ArcadiaTioCompressionMode,
    /// Native compression codec.
    pub codec: sys::ArcadiaTioCompressionCodec,
    /// Auto-mode minimum raw payload bytes.
    pub min_payload_bytes: u32,
    /// Zstd level.
    pub zstd_level: i32,
}

impl CompressionConfig {
    /// Explicit uncompressed writes.
    pub fn uncompressed() -> Self {
        Self {
            mode: sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF,
            codec: sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
            min_payload_bytes: 0,
            zstd_level: 3,
        }
    }

    /// Explicit zstd writes at the requested level.
    pub fn zstd_level(level: i32) -> Self {
        Self {
            mode: sys::ARCADIA_TIO_COMPRESSION_FORCE_ON,
            codec: sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
            min_payload_bytes: 0,
            zstd_level: level,
        }
    }

    fn validate(self) -> Result<Self> {
        if !matches!(
            self.mode,
            sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF
                | sys::ARCADIA_TIO_COMPRESSION_AUTO
                | sys::ARCADIA_TIO_COMPRESSION_FORCE_ON
        ) {
            return Err(TioError::invalid_argument("unknown compression mode"));
        }
        if self.codec != sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD {
            return Err(TioError::unimplemented(
                "LZ4 V4 payload compression is not supported yet",
            ));
        }
        if !(-7..=22).contains(&self.zstd_level) {
            return Err(TioError::invalid_argument(
                "zstd_level must be within [-7, 22]",
            ));
        }
        Ok(self)
    }

    fn to_raw(self) -> sys::ArcadiaTioCompressionConfig {
        sys::ArcadiaTioCompressionConfig {
            version: 1,
            struct_size: std::mem::size_of::<sys::ArcadiaTioCompressionConfig>(),
            mode: self.mode,
            codec: self.codec,
            min_payload_bytes: self.min_payload_bytes,
            zstd_level: self.zstd_level,
        }
    }
}

/// Coordinate dtype supported by native coordinate metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateDType {
    /// 32-bit signed integer coordinates.
    I32,
    /// 64-bit signed integer coordinates.
    I64,
}

impl CoordinateDType {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateDType {
        match self {
            Self::I32 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I64,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateDType) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_DTYPE_I32 => Ok(Self::I32),
            sys::ARCADIA_TIO_COORDINATE_DTYPE_I64 => Ok(Self::I64),
            other => Err(TioError::conversion(format!(
                "unknown coordinate dtype value {other}"
            ))),
        }
    }
}

/// Coordinate semantic kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateKind {
    /// Ordinal/position coordinate.
    Position,
    /// Numeric label id coordinate.
    LabelId,
    /// Date coordinate.
    Date,
    /// Timestamp coordinate.
    Timestamp,
    /// Domain-specific numeric value.
    DomainValue,
}

impl CoordinateKind {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateKind {
        match self {
            Self::Position => sys::ARCADIA_TIO_COORDINATE_KIND_POSITION,
            Self::LabelId => sys::ARCADIA_TIO_COORDINATE_KIND_LABEL_ID,
            Self::Date => sys::ARCADIA_TIO_COORDINATE_KIND_DATE,
            Self::Timestamp => sys::ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP,
            Self::DomainValue => sys::ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_KIND_POSITION => Ok(Self::Position),
            sys::ARCADIA_TIO_COORDINATE_KIND_LABEL_ID => Ok(Self::LabelId),
            sys::ARCADIA_TIO_COORDINATE_KIND_DATE => Ok(Self::Date),
            sys::ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP => Ok(Self::Timestamp),
            sys::ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE => Ok(Self::DomainValue),
            other => Err(TioError::conversion(format!(
                "unknown coordinate kind value {other}"
            ))),
        }
    }
}

/// Integer coordinate encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateEncoding {
    /// Plain integer coordinate values.
    Plain,
    /// Days since an agreed epoch.
    DateDays,
    /// YYYYMMDD encoded date integer.
    DateYyyymmdd,
    /// Unix epoch seconds.
    EpochSeconds,
    /// Unix epoch milliseconds.
    EpochMilliseconds,
    /// Unix epoch microseconds.
    EpochMicroseconds,
    /// Unix epoch nanoseconds.
    EpochNanoseconds,
}

impl CoordinateEncoding {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateEncoding {
        match self {
            Self::Plain => sys::ARCADIA_TIO_COORDINATE_ENCODING_PLAIN,
            Self::DateDays => sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_DAYS,
            Self::DateYyyymmdd => sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD,
            Self::EpochSeconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_SECONDS,
            Self::EpochMilliseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MILLISECONDS,
            Self::EpochMicroseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MICROSECONDS,
            Self::EpochNanoseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_NANOSECONDS,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateEncoding) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_ENCODING_PLAIN => Ok(Self::Plain),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_DAYS => Ok(Self::DateDays),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD => Ok(Self::DateYyyymmdd),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_SECONDS => Ok(Self::EpochSeconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MILLISECONDS => Ok(Self::EpochMilliseconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MICROSECONDS => Ok(Self::EpochMicroseconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_NANOSECONDS => Ok(Self::EpochNanoseconds),
            other => Err(TioError::conversion(format!(
                "unknown coordinate encoding value {other}"
            ))),
        }
    }
}

/// Coordinate sortedness hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSortedness {
    /// Sortedness not declared.
    Unknown,
    /// Values are ascending.
    Ascending,
    /// Values are descending.
    Descending,
    /// Values are unsorted.
    Unsorted,
}

impl CoordinateSortedness {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateSortedness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN,
            Self::Ascending => sys::ARCADIA_TIO_COORDINATE_SORTED_ASCENDING,
            Self::Descending => sys::ARCADIA_TIO_COORDINATE_SORTED_DESCENDING,
            Self::Unsorted => sys::ARCADIA_TIO_COORDINATE_SORTED_UNSORTED,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateSortedness) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_SORTED_ASCENDING => Ok(Self::Ascending),
            sys::ARCADIA_TIO_COORDINATE_SORTED_DESCENDING => Ok(Self::Descending),
            sys::ARCADIA_TIO_COORDINATE_SORTED_UNSORTED => Ok(Self::Unsorted),
            other => Err(TioError::conversion(format!(
                "unknown coordinate sortedness value {other}"
            ))),
        }
    }
}

/// Coordinate monotonicity hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateMonotonicity {
    /// Monotonicity not declared.
    Unknown,
    /// Values are non-decreasing.
    NonDecreasing,
    /// Values are strictly increasing.
    StrictlyIncreasing,
    /// Values are non-increasing.
    NonIncreasing,
    /// Values are strictly decreasing.
    StrictlyDecreasing,
    /// Values are not monotonic.
    NotMonotonic,
}

impl CoordinateMonotonicity {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateMonotonicity {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_UNKNOWN,
            Self::NonDecreasing => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_DECREASING,
            Self::StrictlyIncreasing => {
                sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_INCREASING
            }
            Self::NonIncreasing => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_INCREASING,
            Self::StrictlyDecreasing => {
                sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_DECREASING
            }
            Self::NotMonotonic => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NOT_MONOTONIC,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateMonotonicity) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_DECREASING => Ok(Self::NonDecreasing),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_INCREASING => {
                Ok(Self::StrictlyIncreasing)
            }
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_INCREASING => Ok(Self::NonIncreasing),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_DECREASING => {
                Ok(Self::StrictlyDecreasing)
            }
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NOT_MONOTONIC => Ok(Self::NotMonotonic),
            other => Err(TioError::conversion(format!(
                "unknown coordinate monotonicity value {other}"
            ))),
        }
    }
}

/// Coordinate uniqueness hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateUniqueness {
    /// Uniqueness not declared.
    Unknown,
    /// Values are unique.
    Unique,
    /// Values have duplicates.
    HasDuplicates,
}

impl CoordinateUniqueness {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateUniqueness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN,
            Self::Unique => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE,
            Self::HasDuplicates => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateUniqueness) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE => Ok(Self::Unique),
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES => Ok(Self::HasDuplicates),
            other => Err(TioError::conversion(format!(
                "unknown coordinate uniqueness value {other}"
            ))),
        }
    }
}

/// Coordinate storage location kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateStorageKind {
    /// Inline coordinate values stored in the TIO file.
    Inline,
    /// External coordinates referenced by descriptor metadata only.
    External,
}

impl CoordinateStorageKind {
    fn from_raw(value: sys::ArcadiaTioCoordinateStorageKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_STORAGE_INLINE => Ok(Self::Inline),
            sys::ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL => Ok(Self::External),
            other => Err(TioError::conversion(format!(
                "unknown coordinate storage kind value {other}"
            ))),
        }
    }
}

/// External coordinate source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCoordinateSourceKind {
    /// Same-file object reference.
    SameFileObject,
    /// Relative path reference.
    RelativePath,
    /// Absolute path reference.
    AbsolutePath,
    /// URI reference.
    Uri,
}

impl ExternalCoordinateSourceKind {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateSourceKind {
        match self {
            Self::SameFileObject => sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            Self::RelativePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH,
            Self::AbsolutePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH,
            Self::Uri => sys::ARCADIA_TIO_COORDINATE_SOURCE_URI,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateSourceKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT => Ok(Self::SameFileObject),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH => Ok(Self::RelativePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH => Ok(Self::AbsolutePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_URI => Ok(Self::Uri),
            other => Err(TioError::conversion(format!(
                "unknown coordinate source kind value {other}"
            ))),
        }
    }
}

/// Coordinate validation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateValidationStatus {
    /// Coordinate values are validated.
    Validated,
    /// Coordinate values are not validated or externally referenced.
    Unvalidated,
}

impl CoordinateValidationStatus {
    fn from_raw(value: sys::ArcadiaTioCoordinateValidationStatus) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_VALIDATED => Ok(Self::Validated),
            sys::ARCADIA_TIO_COORDINATE_UNVALIDATED => Ok(Self::Unvalidated),
            other => Err(TioError::conversion(format!(
                "unknown coordinate validation status value {other}"
            ))),
        }
    }
}

/// Coordinate ordering hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinateOrdering {
    /// Sortedness hint.
    pub sorted: CoordinateSortedness,
    /// Monotonicity hint.
    pub monotonicity: CoordinateMonotonicity,
    /// Uniqueness hint.
    pub uniqueness: CoordinateUniqueness,
}

impl Default for CoordinateOrdering {
    fn default() -> Self {
        Self {
            sorted: CoordinateSortedness::Unknown,
            monotonicity: CoordinateMonotonicity::Unknown,
            uniqueness: CoordinateUniqueness::Unknown,
        }
    }
}

/// Owned inline coordinate values accepted by create metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateValues {
    /// i32 coordinate values.
    I32(Vec<i32>),
    /// i64 coordinate values.
    I64(Vec<i64>),
}

impl CoordinateValues {
    fn dtype(&self) -> CoordinateDType {
        match self {
            Self::I32(_) => CoordinateDType::I32,
            Self::I64(_) => CoordinateDType::I64,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    fn as_ptr(&self) -> *const c_void {
        match self {
            Self::I32(values) => values.as_ptr().cast(),
            Self::I64(values) => values.as_ptr().cast(),
        }
    }
}

/// Coordinate storage descriptor accepted at create time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateStorage {
    /// Inline coordinate values. The values are borrowed only for the create call.
    Inline(CoordinateValues),
    /// External coordinate descriptor. External values are not resolved by this wrapper slice.
    External {
        /// External source kind.
        source_kind: ExternalCoordinateSourceKind,
        /// External URI/path.
        uri: String,
        /// External coordinate dtype.
        dtype: CoordinateDType,
        /// External coordinate length.
        length: u64,
    },
}

/// Coordinate descriptor accepted by create APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateSpec {
    /// Axis index.
    pub axis: usize,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate kind.
    pub kind: CoordinateKind,
    /// Coordinate encoding.
    pub encoding: CoordinateEncoding,
    /// Coordinate storage descriptor.
    pub storage: CoordinateStorage,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Whether the coordinate is required by consumers.
    pub required: bool,
}

/// Coordinate metadata snapshot copied from native-owned descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateMeta {
    /// Axis index.
    pub axis: usize,
    /// Optional axis name snapshot.
    pub axis_name_snapshot: Option<String>,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate kind.
    pub kind: CoordinateKind,
    /// Coordinate dtype.
    pub dtype: CoordinateDType,
    /// Coordinate encoding.
    pub encoding: CoordinateEncoding,
    /// Coordinate length.
    pub length: u64,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Storage kind.
    pub storage_kind: CoordinateStorageKind,
    /// External source kind.
    pub external_source_kind: ExternalCoordinateSourceKind,
    /// External URI when storage is external.
    pub external_uri: Option<String>,
    /// Whether this coordinate is required.
    pub required: bool,
    /// Validation status.
    pub validation_status: CoordinateValidationStatus,
}

/// RAII TensorFile handle over the native C ABI.
///
/// The wrapper closes the native handle exactly once in `Drop`. It deliberately does not
/// implement `Send` or `Sync` in this first slice because the C ABI handle thread-safety contract
/// is not documented for concurrent mutation.
pub struct TensorFile {
    raw: NonNull<sys::ArcadiaTioHandle>,
    _not_send_or_sync: PhantomData<Rc<()>>,
}

impl TensorFile {
    /// Creates a TensorFile from safe create options.
    pub fn create(path: impl AsRef<Path>, options: CreateOptions) -> Result<Self> {
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_coordinates(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    prepared.coordinate_ptr(),
                    prepared.coordinate_len(),
                ),
                CreateLayout::RandomAccess => {
                    sys::arcadia_tio_create_random_access_with_coordinates(
                        prepared.path.as_ptr(),
                        options.dtype.to_raw(),
                        prepared.dim_kinds.as_ptr(),
                        prepared.dim_lens.as_ptr(),
                        prepared.dim_lens.len(),
                        options.append_dim,
                        prepared.dim_name_ptr(),
                        prepared.dim_name_len(),
                        prepared.symbol_ptr(),
                        prepared.symbol_len(),
                        prepared.channel_ptr(),
                        prepared.channel_len(),
                        prepared.user_key_ptr(),
                        prepared.user_value_ptr(),
                        prepared.user_kv_len(),
                        prepared.coordinate_ptr(),
                        prepared.coordinate_len(),
                    )
                }
            }
        };
        let file = Self::from_raw_handle(raw, "failed to create TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile with universe-aware axis identity options.
    ///
    /// Coordinate descriptors cannot be combined with universe create options in this wrapper slice
    /// because the current C ABI exposes separate coordinate and universe create families.
    pub fn create_with_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with universe create options yet",
            ));
        }
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
                CreateLayout::RandomAccess => sys::arcadia_tio_create_random_access_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
            }
        };
        let file = Self::from_raw_handle(raw, "failed to create universe-aware TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile using native inferred layout-family selection.
    ///
    /// Coordinate descriptors cannot be combined with inferred create in this wrapper slice
    /// because the current C ABI exposes no inferred+coordinate create family.
    pub fn create_inferred(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with inferred create options yet",
            ));
        }
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        let raw = unsafe {
            sys::arcadia_tio_create_inferred_ex(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                inferred_options.storage_access.to_raw(),
                inferred_options.open_pattern.to_raw(),
                inferred_options.file_population.to_raw(),
                inferred_options.metadata_stability.to_raw(),
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create inferred TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile using native policy-based chunking.
    ///
    /// Coordinate descriptors cannot be combined with policy create in this wrapper slice
    /// because the current C ABI exposes no policy+coordinate create family.
    pub fn create_with_policy(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with policy create options yet",
            ));
        }
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_ex(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create policy TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile with native policy chunking and universe-aware axes.
    ///
    /// Coordinate descriptors cannot be combined with policy+universe create in this wrapper slice
    /// because the current C ABI exposes no policy+universe+coordinate create family.
    pub fn create_with_policy_and_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with policy universe create options yet",
            ));
        }
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_universe_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_universe(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                &raw_universe_options,
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create policy universe TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Set write-time compression for future appends on this handle.
    pub fn set_compression(&self, compression: CompressionConfig) -> Result<()> {
        let raw = compression.validate()?.to_raw();
        let status = unsafe { sys::arcadia_tio_set_compression_config(self.raw.as_ptr(), &raw) };
        status_result(status, "failed to set compression config")
    }

    /// Opens an existing TensorFile.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path_to_cstring(path)?;
        // SAFETY: The C string is valid for the duration of this call.
        let raw = unsafe { sys::arcadia_tio_open(path.as_ptr()) };
        Self::from_raw_handle(raw, "failed to open TensorFile")
    }

    /// Loads metadata without keeping a TensorFile handle open.
    pub fn load_meta(path: impl AsRef<Path>) -> Result<FileMeta> {
        let path = path_to_cstring(path)?;
        let mut raw = MaybeUninit::<sys::ArcadiaTioFileMeta>::uninit();
        // SAFETY: `raw` points to valid uninitialized storage for the C ABI to fill.
        let status = unsafe { sys::arcadia_tio_load_meta(path.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to load TensorFile metadata")?;
        // SAFETY: Successful status initializes `raw`.
        let mut raw = unsafe { raw.assume_init() };
        let meta = copy_file_meta(&raw);
        // SAFETY: `raw` contains native-owned buffers returned by load_meta and is freed exactly once.
        unsafe { sys::arcadia_tio_file_meta_free(&mut raw) };
        meta
    }

    /// Loads coordinate metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_meta(path: impl AsRef<Path>) -> Result<Vec<CoordinateMeta>> {
        let path = path_to_cstring(path)?;
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMeta = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: The path C string and out pointers are valid for the duration of this call.
        let status = unsafe {
            sys::arcadia_tio_load_coordinate_meta(path.as_ptr(), &mut raw_meta, &mut len)
        };
        status_result(status, "failed to load coordinate metadata")?;
        let out = copy_coordinate_meta(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output from load_coordinate_meta and freed once.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_free(raw_meta, len) };
        out
    }

    /// Returns the native C ABI version reported by the linked library.
    pub fn native_abi_version() -> u32 {
        // SAFETY: Version query has no preconditions.
        unsafe { sys::arcadia_tio_abi_version() }
    }

    /// Returns the tensor rank.
    pub fn rank(&self) -> Result<usize> {
        let mut rank = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_rank(self.raw.as_ptr(), &mut rank) };
        status_result(status, "failed to read TensorFile rank")?;
        Ok(rank)
    }

    /// Returns the payload dtype.
    pub fn dtype(&self) -> Result<DType> {
        let mut dtype = sys::ARCADIA_TIO_DTYPE_F32;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_dtype(self.raw.as_ptr(), &mut dtype) };
        status_result(status, "failed to read TensorFile dtype")?;
        DType::from_raw(dtype)
    }

    /// Returns the append-axis index.
    pub fn append_axis(&self) -> Result<usize> {
        let mut axis = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_append_axis(self.raw.as_ptr(), &mut axis) };
        status_result(status, "failed to read TensorFile append axis")?;
        Ok(axis)
    }

    /// Returns the current dimension lengths.
    pub fn dim_lens(&self) -> Result<Vec<u32>> {
        let rank = self.rank()?;
        let mut dims = vec![0u32; rank];
        // SAFETY: `dims` has exactly `rank` writable elements and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_dim_lens(self.raw.as_ptr(), dims.as_mut_ptr(), dims.len()) };
        status_result(status, "failed to read TensorFile dimension lengths")?;
        Ok(dims)
    }

    /// Returns the native index-checkpoint interval in commits.
    pub fn index_checkpoint_every_commits(&self) -> Result<u32> {
        let mut every_commits = 0u32;
        // SAFETY: `every_commits` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_index_checkpoint_every_commits(
                self.raw.as_ptr(),
                &mut every_commits,
            )
        };
        status_result(status, "failed to read index checkpoint interval")?;
        Ok(every_commits)
    }

    /// Updates the native index-checkpoint interval in commits.
    ///
    /// The interval must be at least one. Native implementations that do not support this
    /// metadata update return an ordinary wrapper error without changing the file.
    pub fn set_index_checkpoint_every_commits(&mut self, every_commits: u32) -> Result<()> {
        if every_commits == 0 {
            return Err(TioError::invalid_argument(
                "index checkpoint interval must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe {
            sys::arcadia_tio_set_index_checkpoint_every_commits(self.raw.as_ptr(), every_commits)
        };
        status_result(status, "failed to set index checkpoint interval")
    }

    /// Returns the native chunking plan copied into Rust-owned memory.
    pub fn chunk_plan(&self) -> Result<ChunkPlan> {
        let mut raw_plan = NativeChunkPlan::new();
        // SAFETY: `raw_plan` is a valid output pointer and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_chunk_plan(self.raw.as_ptr(), raw_plan.as_mut_ptr()) };
        status_result(status, "failed to read chunk plan")?;
        copy_chunk_plan(raw_plan.as_ref())
    }

    /// Updates or clears one dimension name through the native metadata administration API.
    ///
    /// Passing `None` clears the name. Native implementations that do not support metadata-only
    /// updates return an ordinary wrapper error without changing the file.
    pub fn set_dim_name(&mut self, axis: usize, name: Option<&str>) -> Result<()> {
        if matches!(name, Some("")) {
            return Err(TioError::invalid_argument("dimension name cannot be empty"));
        }
        let name = name
            .map(|value| string_to_cstring(value, "dimension name"))
            .transpose()?;
        let (ptr, has_name) = match name.as_ref() {
            Some(value) => (value.as_ptr(), 1),
            None => (ptr::null(), 0),
        };
        // SAFETY: Optional name CString, when present, outlives the call and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_set_dim_name(self.raw.as_ptr(), axis, ptr, has_name) };
        status_result(status, "failed to set dimension name")
    }

    /// Replaces Symbol-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_symbols<S: AsRef<str>>(&mut self, symbols: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(symbols, "symbol label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_symbols(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set symbol labels")
    }

    /// Replaces Channel-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_channels<S: AsRef<str>>(&mut self, channels: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(channels, "channel label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_channels(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set channel labels")
    }

    /// Replaces user key/value metadata through the native metadata administration API.
    ///
    /// Passing an empty slice requests clearing all user metadata. Native implementations that do
    /// not support metadata-only updates return an ordinary wrapper error without changing the file.
    pub fn set_user_kv<K, V>(&mut self, user_kv: &[(K, V)]) -> Result<()>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let prepared = PreparedUserKvList::new(user_kv)?;
        // SAFETY: Prepared key/value C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_user_kv(
                self.raw.as_ptr(),
                prepared.key_ptr(),
                prepared.value_ptr(),
                prepared.len(),
            )
        };
        status_result(status, "failed to set user metadata")
    }

    /// Returns the native path snapshot for this handle.
    pub fn path(&self) -> Result<String> {
        let mut raw_path: *mut c_char = ptr::null_mut();
        // SAFETY: `raw_path` is a valid out pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_path(self.raw.as_ptr(), &mut raw_path) };
        status_result(status, "failed to read TensorFile path")?;
        let value = required_c_string(raw_path.cast_const());
        // SAFETY: `raw_path` is native-owned output from arcadia_tio_path.
        unsafe { sys::arcadia_tio_string_free(raw_path) };
        Ok(value)
    }

    /// Reads coordinate metadata from the open handle.
    pub fn coordinate_meta(&self) -> Result<Vec<CoordinateMeta>> {
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMeta = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: Out pointers are valid and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_coordinate_meta(self.raw.as_ptr(), &mut raw_meta, &mut len) };
        status_result(status, "failed to read coordinate metadata")?;
        let out = copy_coordinate_meta(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output from coordinate_meta and freed once.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_free(raw_meta, len) };
        out
    }

    /// Analyzes how a sparse-intent f32 append would be handled by the native writer.
    pub fn analyze_sparse_append_f32(
        &self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f32(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent f64 append would be handled by the native writer.
    pub fn analyze_sparse_append_f64(
        &self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f64(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i32 append would be handled by the native writer.
    ///
    /// Integer sparse append currently supports null-subtensor rules and zero predicates only;
    /// exact integer predicates remain deferred.
    pub fn analyze_sparse_append_i32(
        &self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i32(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i64 append would be handled by the native writer.
    ///
    /// Integer sparse append currently supports null-subtensor rules and zero predicates only;
    /// exact integer predicates remain deferred.
    pub fn analyze_sparse_append_i64(
        &self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i64(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f32(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F32, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f32(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f32_with_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f32_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f32_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f32`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f32_returning_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f32_with_range(data, shape, rule)
    }

    /// Appends f64 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f64(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F64, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f64(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f64_with_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f64_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f64_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f64`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f64_returning_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f64_with_range(data, shape, rule)
    }

    /// Appends i32 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// Integer sparse append currently supports null-subtensor rules and zero predicates only;
    /// exact integer predicates remain deferred.
    pub fn append_sparse_i32(
        &mut self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i32_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends i64 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// Integer sparse append currently supports null-subtensor rules and zero predicates only;
    /// exact integer predicates remain deferred.
    pub fn append_sparse_i64(
        &mut self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i64_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends a bulk f32 slice and returns the assigned append-entry range.
    pub fn append_f32(&mut self, data: &[f32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice and returns the assigned append-entry range.
    pub fn append_f64(&mut self, data: &[f64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice and returns the assigned append-entry range.
    pub fn append_i32(&mut self, data: &[i32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice and returns the assigned append-entry range.
    pub fn append_i64(&mut self, data: &[i64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f32 slice with universe bindings and returns the assigned entry range.
    pub fn append_f32_with_universe(
        &mut self,
        data: &[f32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice with universe bindings and returns the assigned entry range.
    pub fn append_f64_with_universe(
        &mut self,
        data: &[f64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice with universe bindings and returns the assigned entry range.
    pub fn append_i32_with_universe(
        &mut self,
        data: &[i32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice with universe bindings and returns the assigned entry range.
    pub fn append_i64_with_universe(
        &mut self,
        data: &[i64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Rewrites a single native entry selector with f32 payload data.
    pub fn rewrite_f32(
        &mut self,
        selector: EntrySelector,
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f32(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 data")
    }

    /// Rewrites a single native entry selector with f64 payload data.
    pub fn rewrite_f64(
        &mut self,
        selector: EntrySelector,
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f64(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 data")
    }

    /// Rewrites a selector slice with f32 payload data.
    pub fn rewrite_slice_f32(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f32(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 selector slice")
    }

    /// Rewrites a selector slice with f64 payload data.
    pub fn rewrite_slice_f64(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f64(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 selector slice")
    }

    /// Clears storage blocks identified by chunk keys.
    pub fn clear_blocks(&mut self, keys: &[ChunkKey]) -> Result<()> {
        let prepared_keys = PreparedChunkKeys::new(keys);
        // SAFETY: Prepared chunk-key buffers and their borrowed coordinate slices outlive the call.
        let status = unsafe {
            sys::arcadia_tio_clear_blocks(
                self.raw.as_ptr(),
                prepared_keys.ptr(),
                prepared_keys.len(),
            )
        };
        status_result(status, "failed to clear blocks")
    }

    /// Returns metadata for the current visible head commit.
    pub fn head_commit(&self) -> Result<CommitInfo> {
        let mut raw = MaybeUninit::<sys::ArcadiaTioCommitInfo>::uninit();
        // SAFETY: `raw` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_head_commit(self.raw.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to read head commit")?;
        // SAFETY: Successful native call initialized the output commit.
        Ok(unsafe { raw.assume_init() }.into())
    }

    /// Lists visible commits in native order.
    ///
    /// A `limit` of `None` requests the native full visible list; `Some(0)` is rejected because the
    /// underlying C ABI uses zero as the unbounded sentinel.
    pub fn list_commits(&self, limit: Option<u32>) -> Result<Vec<CommitInfo>> {
        let raw_limit = match limit {
            Some(0) => {
                return Err(TioError::invalid_argument(
                    "commit list limit must be non-zero; use None for the full list",
                ));
            }
            Some(value) => value,
            None => 0,
        };
        let mut raw_list = NativeCommitList::new();
        // SAFETY: `raw_list` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_list_commits(self.raw.as_ptr(), raw_limit, raw_list.as_mut_ptr())
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(TioError::from_last_error("failed to list commits"));
        }
        copy_commit_list(raw_list.as_ref())
    }

    /// Removes the current visible head commit.
    ///
    /// This mutates the open file in place and delegates all retention/underflow validation to the
    /// native history implementation.
    pub fn pop(&mut self) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop(self.raw.as_ptr()) };
        status_result(status, "failed to pop head commit")
    }

    /// Removes up to `n` visible head commits.
    ///
    /// This mutates the open file in place. Passing `0` is rejected by the safe wrapper because it
    /// cannot change history and is usually a caller bug.
    pub fn pop_batched(&mut self, n: u32) -> Result<()> {
        if n == 0 {
            return Err(TioError::invalid_argument(
                "pop_batched count must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop_batched(self.raw.as_ptr(), n) };
        status_result(status, "failed to pop batched commits")
    }

    /// Reverts the file to a visible target commit sequence.
    ///
    /// This mutates the open file in place and preserves native semantics for invalid or retained
    /// history targets.
    pub fn revert_commit(&mut self, target_commit_seq: u64) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status =
            unsafe { sys::arcadia_tio_revert_commit(self.raw.as_ptr(), target_commit_seq) };
        status_result(status, "failed to revert commit")
    }

    /// Returns shallow compatibility compaction statistics.
    pub fn analyze_compaction(&self) -> Result<CompactionStats> {
        let mut stats = sys::ArcadiaTioCompactionStats {
            live_bytes: 0,
            dead_bytes: 0,
            dead_ratio: 0.0,
            commit_count: 0,
        };
        // SAFETY: `stats` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_analyze_compaction(self.raw.as_ptr(), &mut stats) };
        status_result(status, "failed to analyze compaction")?;
        Ok(CompactionStats {
            live_bytes: stats.live_bytes,
            dead_bytes: stats.dead_bytes,
            dead_ratio: stats.dead_ratio,
            commit_count: stats.commit_count,
        })
    }

    /// Returns non-precise V4 source-file diagnostics.
    pub fn v4_diagnostics(&self) -> Result<V4DiagnosticsReport> {
        let mut report = new_v4_diagnostics_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status = unsafe { sys::arcadia_tio_v4_diagnostics(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to get V4 diagnostics"));
        }
        let copied = copy_v4_diagnostics_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
        Ok(copied)
    }

    /// Returns precise V4 source-file diagnostics with validity metadata.
    pub fn v4_diagnostics_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4DiagnosticsPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_diagnostics_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_v4_diagnostics_precise(self.raw.as_ptr(), &raw_options, &mut report)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to get precise V4 diagnostics",
            ));
        }
        let copied = copy_v4_diagnostics_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
        Ok(copied)
    }

    /// Returns non-precise V4 current-state compaction analysis.
    pub fn analyze_v4_compaction(&self) -> Result<V4CompactionAnalysisReport> {
        let mut report = new_v4_compaction_analysis_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_analyze_v4_compaction(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to analyze V4 compaction"));
        }
        let copied = copy_v4_compaction_analysis_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
        copied
    }

    /// Returns precise V4 current-state compaction analysis with validity metadata.
    pub fn analyze_v4_compaction_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4CompactionAnalysisPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_compaction_analysis_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_analyze_v4_compaction_precise(
                self.raw.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to analyze precise V4 compaction",
            ));
        }
        let copied = copy_v4_compaction_analysis_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
        copied
    }

    /// Compacts live chunks into a destination file.
    pub fn compact_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        // SAFETY: Destination path C string and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.retain_commits,
                options.mode.to_raw(),
            )
        };
        status_result(status, "failed to compact TensorFile")
    }

    /// Conditionally compacts live chunks into a destination file.
    pub fn maybe_compact(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<bool> {
        let dst_path = path_to_cstring(dst_path)?;
        let mut compacted = 0u8;
        // SAFETY: Destination path C string, output flag, and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_maybe_compact(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.dead_ratio_threshold,
                options.min_dead_bytes,
                options.retain_commits,
                options.mode.to_raw(),
                &mut compacted,
            )
        };
        status_result(status, "failed to maybe compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        self.get_auto_compaction_config()
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn get_auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        let mut config = new_auto_compaction_config();
        let mut has_config = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_auto_compaction_config(
                self.raw.as_ptr(),
                &mut config,
                &mut has_config,
            )
        };
        status_result(status, "failed to get auto-compaction config")?;
        if has_config == 0 {
            Ok(None)
        } else {
            copy_auto_compaction_config(config).map(Some)
        }
    }

    /// Updates or clears auto-compaction metadata configuration.
    pub fn set_auto_compaction_config(
        &mut self,
        config: Option<AutoCompactionConfig>,
    ) -> Result<()> {
        let raw = config.map(|cfg| cfg.to_raw());
        let (ptr, has_config) = match raw.as_ref() {
            Some(cfg) => (cfg as *const sys::ArcadiaTioAutoCompactionConfig, 1u8),
            None => (ptr::null(), 0u8),
        };
        // SAFETY: Optional config pointer is either null or points to a local value valid for this call.
        let status = unsafe {
            sys::arcadia_tio_set_auto_compaction_config(self.raw.as_ptr(), ptr, has_config)
        };
        status_result(status, "failed to set auto-compaction config")
    }

    /// Clears auto-compaction metadata configuration.
    pub fn clear_auto_compaction(&mut self) -> Result<()> {
        self.set_auto_compaction_config(None)
    }

    /// Reads auto-compaction state metadata, if present.
    pub fn compaction_state(&self) -> Result<Option<CompactionState>> {
        let mut state = sys::ArcadiaTioCompactionState {
            last_compacted_commit_seq: 0,
            last_compacted_at_unix_ms: 0,
        };
        let mut has_state = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_compaction_state(self.raw.as_ptr(), &mut state, &mut has_state)
        };
        status_result(status, "failed to read compaction state")?;
        if has_state == 0 {
            Ok(None)
        } else {
            Ok(Some(CompactionState {
                last_compacted_commit_seq: state.last_compacted_commit_seq,
                last_compacted_at_unix_ms: state.last_compacted_at_unix_ms,
            }))
        }
    }

    /// Runs metadata-configured auto-compaction if native thresholds trigger.
    pub fn maybe_compact_auto(&mut self) -> Result<bool> {
        let mut compacted = 0u8;
        // SAFETY: Output flag is valid and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_maybe_compact_auto(self.raw.as_ptr(), &mut compacted) };
        status_result(status, "failed to maybe auto-compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Compacts a V4 file into a retained-history destination.
    pub fn compact_v4_retained_history_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: V4RetainedHistoryCompactionOptions,
    ) -> Result<V4RetainedHistoryCompactionReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_v4_retained_history_compaction_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history",
            ));
        }
        let copied = copy_v4_retained_history_compaction_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
        copied
    }

    /// Compacts a V4 file into a retained-history destination with precise source accounting.
    pub fn compact_v4_retained_history_to_precise(
        &mut self,
        dst_path: impl AsRef<Path>,
        retention_options: V4RetainedHistoryCompactionOptions,
        precise_options: V4PreciseAccountingOptions,
    ) -> Result<V4RetainedHistoryCompactionPreciseReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_retention_options = retention_options.to_raw();
        let raw_precise_options = precise_options.to_raw();
        let mut report = new_v4_retained_history_compaction_precise_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to_precise(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_retention_options,
                &raw_precise_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report)
            };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history with precise accounting",
            ));
        }
        let copied = copy_v4_retained_history_compaction_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report) };
        copied
    }

    /// Reforms visible data into a destination file with an explicit target layout.
    pub fn reform_to(&mut self, dst_path: impl AsRef<Path>, options: ReformOptions) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        // SAFETY: Inputs are valid for the duration of the FFI call.
        let status = unsafe {
            sys::arcadia_tio_reform_to(self.raw.as_ptr(), dst_path.as_ptr(), &raw_options)
        };
        status_result(status, "failed to reform TensorFile")
    }

    /// Reforms visible data into a destination file and returns native diagnostic metadata.
    pub fn reform_to_ex(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: ReformOptions,
    ) -> Result<ReformReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_reform_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_reform_to_ex(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            let copied = copy_reform_report(&report);
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
            return Err(
                TioError::from_last_error("failed to reform TensorFile with report")
                    .with_reform_report(&copied),
            );
        }
        let copied = copy_reform_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
        Ok(copied)
    }

    /// Reads the full tensor into Rust-owned buffers.
    pub fn read_all(&self) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe { sys::arcadia_tio_read_all(handle, out) })
    }

    /// Exports full tensor values through the Arrow C Data Interface.
    ///
    /// The returned [`ArrowCData`] owns the Arrow `release` callbacks and invokes them on drop.
    /// Borrowed C Data pointers are valid only while the returned value is alive.
    pub fn read_values_arrow(&self) -> Result<ArrowCData> {
        // SAFETY: All-zero Arrow C Data carriers represent empty caller-owned output slots with
        // null release callbacks before the native function writes initialized values.
        let mut raw_array: sys::ArrowArray = unsafe { mem::zeroed() };
        // SAFETY: See `raw_array` initialization above.
        let mut raw_schema: sys::ArrowSchema = unsafe { mem::zeroed() };
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_values_arrow(self.raw.as_ptr(), &mut raw_array, &mut raw_schema)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Defensive cleanup for any partially initialized Arrow carriers.
            unsafe {
                release_arrow_array(&mut raw_array);
                release_arrow_schema(&mut raw_schema);
            }
            return Err(TioError::from_last_error(
                "failed to export tensor values as Arrow C Data",
            ));
        }
        Ok(ArrowCData {
            array: raw_array,
            schema: raw_schema,
            _not_send_or_sync: PhantomData,
        })
    }

    /// Reads the full tensor densely with a fill value and optional validity mask.
    pub fn read_all_dense(&self, fill_value: f64) -> Result<DenseTensor> {
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_all_dense(
                self.raw.as_ptr(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
            )
        };
        status_result(status, "failed to read dense tensor")?;
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        // SAFETY: Native-owned buffers are returned by read_all_dense and freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
        }
        Ok(DenseTensor {
            tensor: tensor?,
            mask,
        })
    }

    /// Reads current data through the native basic read-index lowering API.
    pub fn read_index(&self, items: &[ReadIndexItem]) -> Result<ReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_report = new_read_index_report();
        // SAFETY: Prepared read-index items outlive the call; outputs are initialized and valid.
        let status = unsafe {
            sys::arcadia_tio_read_index(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                &mut raw_tensor,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error("failed to read with read_index"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexResult {
            value: tensor?,
            report: report?,
        })
    }

    /// Reads an axis range into Rust-owned buffers.
    pub fn read_axis_range(&self, axis: usize, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "axis range start must be <= end",
            ));
        }
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_range(handle, axis, start, end, out)
        })
    }

    /// Reads an axis take selection into Rust-owned buffers.
    pub fn read_axis_take(&self, axis: usize, indices: &[u32]) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_take(handle, axis, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads an append-entry range into Rust-owned buffers.
    pub fn read_entry_range(&self, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "entry range start must be <= end",
            ));
        }
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_entry_range(handle, start, end, out)
        })
    }

    /// Reads selected append entries into Rust-owned buffers.
    pub fn take_entries(&self, indices: &[u32]) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_take_entries(handle, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads inline coordinate values for an axis into Rust-owned buffers.
    ///
    /// This is metadata-scope coordinate value access, not native exact/range coordinate lookup.
    pub fn read_axis_coordinates(&self, axis: usize) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_coordinates(handle, axis, out)
        })
    }

    /// Looks up the unique axis index for an inline validated i32 coordinate value.
    pub fn coordinate_index_i32(&self, axis: usize, value: i32) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i32(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i32 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the unique axis index for an inline validated i64 coordinate value.
    pub fn coordinate_index_i64(&self, axis: usize, value: i64) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i64(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i64 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i32 coordinate interval.
    pub fn coordinate_range_i32(
        &self,
        axis: usize,
        start: i32,
        end: i32,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i32(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i32 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i64 coordinate interval.
    pub fn coordinate_range_i64(
        &self,
        axis: usize,
        start: i64,
        end: i64,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i64(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i64 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Reads current selector data with execution options and metadata.
    pub fn read_with_options(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error("failed to read with options"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with execution options and metadata.
    pub fn read_with_options_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads current selector data with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by
    /// itself.
    pub fn read_with_options_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
    ) -> Result<AttributedReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                &mut raw_tensor,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: tensor?,
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data densely with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options_dense` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by itself.
    pub fn read_with_options_dense_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
        fill_value: f64,
    ) -> Result<AttributedReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data with a shape policy and execution metadata.
    pub fn read_with_shape_policy(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with a shape policy and execution metadata.
    pub fn read_with_shape_policy_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit into Rust-owned buffers.
    pub fn read_at_commit(&self, commit_seq: u64, selectors: &[EntrySelector]) -> Result<Tensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_at_commit(
                handle,
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                out,
            )
        })
    }

    /// Reads selector data at a retained commit densely with a fill value.
    pub fn read_at_commit_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        fill_value: f64,
    ) -> Result<DenseTensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        // SAFETY: Prepared selector buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
            )
        };
        status_result(status, "failed to read dense tensor at commit")?;
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
        }
        Ok(DenseTensor {
            tensor: tensor?,
            mask,
        })
    }

    /// Reads selector data at a retained commit with execution options and metadata.
    pub fn read_at_commit_with_options(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with execution options and metadata.
    pub fn read_at_commit_with_options_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    fn append_with_range(
        &mut self,
        shape: &[u64],
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut u32, *mut u32) -> i32,
    ) -> Result<AppendRange> {
        let mut start = 0u32;
        let mut end = 0u32;
        let status = call(self.raw.as_ptr(), &mut start, &mut end);
        status_result(status, "failed to append tensor data")?;
        let _ = shape;
        Ok(AppendRange { start, end })
    }

    fn analyze_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut sys::ArcadiaTioSparseAppendAnalysis,
        ) -> i32,
    ) -> Result<SparseAppendAnalysis> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let mut raw_analysis = empty_sparse_append_analysis();
        let status = call(self.raw.as_ptr(), &raw_rule, &mut raw_analysis);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: `raw_analysis` was initialized to an empty native-compatible value before the
            // call. If native populated reasons before returning an error, this releases them once.
            unsafe { sys::arcadia_tio_sparse_append_analysis_free(&mut raw_analysis) };
            return Err(TioError::from_last_error("failed to analyze sparse append"));
        }
        take_sparse_append_analysis(&mut raw_analysis)
    }

    fn append_sparse(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *const sys::ArcadiaTioSparseRule) -> i32,
    ) -> Result<()> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let status = call(self.raw.as_ptr(), &raw_rule);
        status_result(status, "failed to append sparse tensor data")
    }

    fn append_sparse_with_range(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut u32,
            *mut u32,
        ) -> i32,
    ) -> Result<AppendRange> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        self.append_with_range(shape, |handle, start, end| {
            call(handle, &raw_rule, start, end)
        })
    }

    fn prepare_selectors(&self, selectors: &[EntrySelector]) -> Result<PreparedSelectors> {
        PreparedSelectors::new(selectors, self.rank()?)
    }

    fn read_tensor(
        &self,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut sys::ArcadiaTioTensor) -> i32,
    ) -> Result<Tensor> {
        let mut raw = sys::ArcadiaTioTensor::default();
        let status = call(self.raw.as_ptr(), &mut raw);
        status_result(status, "failed to read tensor")?;
        let tensor = copy_tensor(&raw);
        // SAFETY: `raw` is native-owned output from a tensor read call and freed exactly once.
        unsafe { sys::arcadia_tio_tensor_free(&mut raw) };
        tensor
    }

    fn validate_axis(&self, axis: usize) -> Result<()> {
        let rank = self.rank()?;
        if axis >= rank {
            Err(TioError::invalid_argument(format!(
                "axis {axis} out of range for rank {rank}"
            )))
        } else {
            Ok(())
        }
    }

    fn validate_append(&self, dtype: DType, data_len: usize, shape: &[u64]) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "append")
    }

    fn validate_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "sparse append")?;
        rule.validate_for_append(dtype, shape.len(), self.append_axis()?)
    }

    fn validate_mutation_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, operation)
    }

    fn validate_typed_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        let actual_dtype = self.dtype()?;
        if actual_dtype != dtype {
            return Err(TioError::invalid_argument(format!(
                "{operation} dtype {dtype:?} does not match file dtype {actual_dtype:?}"
            )));
        }
        let rank = self.rank()?;
        if shape.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "{operation} shape rank {} does not match file rank {rank}",
                shape.len()
            )));
        }
        let expected_len = shape_element_len(shape)?;
        if expected_len != data_len {
            return Err(TioError::invalid_argument(format!(
                "{operation} data length {data_len} does not match shape element count {expected_len}"
            )));
        }
        Ok(())
    }

    fn from_raw_handle(raw: *mut sys::ArcadiaTioHandle, context: &str) -> Result<Self> {
        let raw = NonNull::new(raw).ok_or_else(|| TioError::from_last_error(context))?;
        Ok(Self {
            raw,
            _not_send_or_sync: PhantomData,
        })
    }

    #[allow(dead_code)]
    fn raw_handle(&self) -> *mut sys::ArcadiaTioHandle {
        self.raw.as_ptr()
    }
}

impl Drop for TensorFile {
    fn drop(&mut self) {
        // SAFETY: `TensorFile` owns this non-null handle and Drop runs at most once.
        unsafe { sys::arcadia_tio_close(self.raw.as_ptr()) };
    }
}

fn shape_element_len(shape: &[u64]) -> Result<usize> {
    let mut product = 1usize;
    for &dim in shape {
        let dim = usize::try_from(dim)
            .map_err(|_| TioError::invalid_argument("shape dimension does not fit usize"))?;
        product = product
            .checked_mul(dim)
            .ok_or_else(|| TioError::invalid_argument("shape element count overflows usize"))?;
    }
    Ok(product)
}

fn validate_create_policy(options: &CreateOptions, policy: &CreatePolicyOptions) -> Result<()> {
    let rank = options.dims.len();
    if options.append_dim >= rank {
        return Err(TioError::invalid_argument("append_dim out of range"));
    }
    if policy.chunk_axes.is_empty() {
        return Err(TioError::invalid_argument(
            "policy create requires at least one chunk axis",
        ));
    }
    if policy.typical_query_sizes.len() != rank {
        return Err(TioError::invalid_argument(format!(
            "typical_query_sizes length {} does not match rank {rank}",
            policy.typical_query_sizes.len()
        )));
    }
    if options.append_dim != 0 {
        return Err(TioError::invalid_argument(
            "RegularChunked policy create currently requires append_dim == 0",
        ));
    }
    if policy.storage_profile != StorageProfile::Balanced {
        return Err(TioError::invalid_argument(
            "RegularChunked policy create currently supports only balanced storage_profile",
        ));
    }
    if !matches!(policy.typical_query_sizes[options.append_dim], 0 | 1) {
        return Err(TioError::invalid_argument(
            "append-axis typical_query_size must be 0 or 1",
        ));
    }
    let mut seen = Vec::with_capacity(policy.chunk_axes.len());
    for &axis in &policy.chunk_axes {
        if axis >= rank {
            return Err(TioError::invalid_argument(format!(
                "chunk axis {axis} out of range for rank {rank}"
            )));
        }
        if axis == options.append_dim {
            return Err(TioError::invalid_argument(
                "chunk axes must exclude the append axis",
            ));
        }
        if seen.contains(&axis) {
            return Err(TioError::invalid_argument(
                "chunk axes must be unique for policy create",
            ));
        }
        if policy.typical_query_sizes[axis] == 0 {
            return Err(TioError::invalid_argument(
                "chunk-axis typical_query_size must be > 0",
            ));
        }
        seen.push(axis);
    }
    for axis in 0..rank {
        if axis != options.append_dim && !seen.contains(&axis) {
            return Err(TioError::invalid_argument(
                "chunk_axes must include every non-append axis for policy create",
            ));
        }
    }
    Ok(())
}

fn copy_shape(raw: &sys::ArcadiaTioTensor) -> Result<Vec<u64>> {
    if raw.rank == 0 {
        return Ok(Vec::new());
    }
    if raw.shape.is_null() {
        return Err(TioError::conversion("native tensor shape pointer is null"));
    }
    // SAFETY: Native tensor shape pointer is valid for `rank` while the tensor output is alive.
    Ok(unsafe { slice::from_raw_parts(raw.shape, raw.rank) }.to_vec())
}

fn copy_tensor(raw: &sys::ArcadiaTioTensor) -> Result<Tensor> {
    let dtype = DType::from_raw(raw.dtype)?;
    let shape = copy_shape(raw)?;
    let element_count = shape_element_len(&shape)?;
    let expected_bytes = element_count
        .checked_mul(dtype.size_bytes())
        .ok_or_else(|| TioError::conversion("native tensor byte length overflows usize"))?;
    if raw.len_bytes != expected_bytes {
        return Err(TioError::conversion(format!(
            "native tensor byte length {} does not match shape/dtype byte length {expected_bytes}",
            raw.len_bytes
        )));
    }
    if raw.len_bytes > 0 && raw.data.is_null() {
        return Err(TioError::conversion("native tensor data pointer is null"));
    }
    let data = match dtype {
        DType::F32 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<f32>(), element_count) };
            TensorData::F32(values.to_vec())
        }
        DType::F64 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<f64>(), element_count) };
            TensorData::F64(values.to_vec())
        }
        DType::I32 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<i32>(), element_count) };
            TensorData::I32(values.to_vec())
        }
        DType::I64 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<i64>(), element_count) };
            TensorData::I64(values.to_vec())
        }
    };
    Ok(Tensor { dtype, shape, data })
}

fn copy_mask(raw: &sys::ArcadiaTioMask) -> Option<Vec<u8>> {
    if raw.len == 0 || raw.data.is_null() {
        return None;
    }
    // SAFETY: The C ABI returns a native-owned mask with `len` bytes while the mask output is alive.
    Some(unsafe { slice::from_raw_parts(raw.data, raw.len) }.to_vec())
}

struct NativeCommitList {
    raw: sys::ArcadiaTioCommitList,
}

impl NativeCommitList {
    fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioCommitList {
                items: ptr::null_mut(),
                len: 0,
            },
        }
    }

    fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioCommitList {
        &mut self.raw
    }

    fn as_ref(&self) -> &sys::ArcadiaTioCommitList {
        &self.raw
    }
}

impl Drop for NativeCommitList {
    fn drop(&mut self) {
        // SAFETY: `raw` is either empty or a native-owned commit-list output. The guard owns it and
        // drops exactly once on all success/error/copy-conversion paths.
        unsafe { sys::arcadia_tio_commit_list_free(&mut self.raw) };
    }
}

struct NativeChunkPlan {
    raw: sys::ArcadiaTioChunkPlan,
}

impl NativeChunkPlan {
    fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioChunkPlan {
                block_sizes: ptr::null_mut(),
                len: 0,
            },
        }
    }

    fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioChunkPlan {
        &mut self.raw
    }

    fn as_ref(&self) -> &sys::ArcadiaTioChunkPlan {
        &self.raw
    }
}

impl Drop for NativeChunkPlan {
    fn drop(&mut self) {
        // SAFETY: `raw` is either empty or a native-owned chunk-plan output. The guard owns it and
        // drops exactly once on all success/error/copy-conversion paths.
        unsafe { sys::arcadia_tio_chunk_plan_free(&mut self.raw) };
    }
}

fn copy_commit_list(raw: &sys::ArcadiaTioCommitList) -> Result<Vec<CommitInfo>> {
    if raw.len == 0 {
        return Ok(Vec::new());
    }
    if raw.items.is_null() {
        return Err(TioError::conversion("native commit list pointer is null"));
    }
    // SAFETY: The C ABI returns `len` commit records owned by the commit-list output while alive.
    Ok(unsafe { slice::from_raw_parts(raw.items, raw.len) }
        .iter()
        .copied()
        .map(CommitInfo::from)
        .collect())
}

fn copy_chunk_plan(raw: &sys::ArcadiaTioChunkPlan) -> Result<ChunkPlan> {
    if raw.len == 0 {
        return Ok(ChunkPlan {
            block_sizes: Vec::new(),
        });
    }
    if raw.block_sizes.is_null() {
        return Err(TioError::conversion(
            "native chunk plan block-size pointer is null",
        ));
    }
    // SAFETY: The C ABI returns `len` block-size entries owned by the chunk-plan output while alive.
    Ok(ChunkPlan {
        block_sizes: unsafe { slice::from_raw_parts(raw.block_sizes, raw.len) }.to_vec(),
    })
}

fn new_query_trace_json() -> sys::ArcadiaTioQueryTraceJson {
    sys::ArcadiaTioQueryTraceJson {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceJson>(),
        json: ptr::null_mut(),
    }
}

fn copy_query_trace_json(raw: &sys::ArcadiaTioQueryTraceJson) -> Result<QueryTraceJson> {
    if raw.json.is_null() {
        return Err(TioError::conversion(
            "native query trace JSON pointer is null",
        ));
    }
    // SAFETY: The C ABI returns a native-owned NUL-terminated JSON string while the output is alive.
    let json = unsafe { CStr::from_ptr(raw.json.cast_const()) }
        .to_string_lossy()
        .into_owned();
    Ok(QueryTraceJson { json })
}

fn new_read_execution_report() -> sys::ArcadiaTioReadExecutionReport {
    sys::ArcadiaTioReadExecutionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReadExecutionReport>(),
        requested_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_max_threads: 0,
        query_effective_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_effective_threads: 0,
        query_parallel_runtime: ptr::null_mut(),
        query_parallel_fallback_reason: ptr::null_mut(),
        query_parallel_reason_code: ptr::null_mut(),
        query_parallel_reason_code_taxonomy: ptr::null_mut(),
    }
}

fn new_read_index_report() -> sys::ArcadiaTioReadIndexReport {
    sys::ArcadiaTioReadIndexReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReadIndexReport>(),
        lowering_kind: sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN,
        used_full_tensor_fallback: 0,
        reserved0: [0; 7],
    }
}

fn new_historical_read_execution_report() -> sys::ArcadiaTioHistoricalReadExecutionReport {
    sys::ArcadiaTioHistoricalReadExecutionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadExecutionReport>(),
        requested_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_max_threads: 0,
        query_effective_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_effective_threads: 0,
        query_parallel_runtime: ptr::null_mut(),
        query_parallel_fallback_reason: ptr::null_mut(),
        query_parallel_reason_code: ptr::null_mut(),
        query_parallel_reason_code_taxonomy: ptr::null_mut(),
        query_source_kind: sys::ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT,
        query_commit_seq: 0,
    }
}

fn copy_read_execution_report(
    raw: &sys::ArcadiaTioReadExecutionReport,
) -> Result<ReadExecutionReport> {
    Ok(ReadExecutionReport {
        requested_mode: ReadExecutionMode::from_raw(raw.requested_mode, raw.query_max_threads)?,
        query_max_threads: raw.query_max_threads,
        query_effective_mode: ReadExecutionMode::from_raw(
            raw.query_effective_mode,
            raw.query_effective_threads,
        )?,
        query_effective_threads: raw.query_effective_threads,
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const()),
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        ),
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const()),
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        ),
    })
}

fn copy_read_index_report(raw: &sys::ArcadiaTioReadIndexReport) -> Result<ReadIndexReport> {
    Ok(ReadIndexReport {
        lowering_kind: ReadIndexLoweringKind::from_raw(raw.lowering_kind)?,
        used_full_tensor_fallback: raw.used_full_tensor_fallback != 0,
    })
}

fn copy_historical_read_execution_report(
    raw: &sys::ArcadiaTioHistoricalReadExecutionReport,
) -> Result<HistoricalReadExecutionReport> {
    let execution = ReadExecutionReport {
        requested_mode: ReadExecutionMode::from_raw(raw.requested_mode, raw.query_max_threads)?,
        query_max_threads: raw.query_max_threads,
        query_effective_mode: ReadExecutionMode::from_raw(
            raw.query_effective_mode,
            raw.query_effective_threads,
        )?,
        query_effective_threads: raw.query_effective_threads,
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const()),
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        ),
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const()),
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        ),
    };
    Ok(HistoricalReadExecutionReport {
        execution,
        query_source_kind: HistoricalQuerySourceKind::from_raw(raw.query_source_kind)?,
        query_commit_seq: raw.query_commit_seq,
    })
}

fn new_v4_precise_accounting_bytes() -> sys::ArcadiaTioV4PreciseAccountingBytes {
    sys::ArcadiaTioV4PreciseAccountingBytes {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4PreciseAccountingBytes>(),
        has_unreachable_bytes: 0,
        unreachable_bytes: 0,
        has_retained_history_required_bytes: 0,
        retained_history_required_bytes: 0,
        has_popped_skipped_bytes: 0,
        popped_skipped_bytes: 0,
        has_reclaimable_bytes: 0,
        reclaimable_bytes: 0,
        omitted_fields: ptr::null_mut(),
        omitted_fields_len: 0,
        omitted_field_reason_codes: ptr::null_mut(),
        omitted_field_reason_codes_len: 0,
    }
}

fn copy_v4_precise_accounting_bytes(
    raw: &sys::ArcadiaTioV4PreciseAccountingBytes,
) -> V4PreciseAccountingBytes {
    let omitted_fields = if raw.omitted_fields.is_null() || raw.omitted_fields_len == 0 {
        Vec::new()
    } else {
        // SAFETY: Native report owns `omitted_fields_len` entries until the parent report is freed.
        let fields = unsafe { slice::from_raw_parts(raw.omitted_fields, raw.omitted_fields_len) };
        let reason_codes = if raw.omitted_field_reason_codes.is_null()
            || raw.omitted_field_reason_codes_len == 0
        {
            &[][..]
        } else {
            // SAFETY: Native report owns `omitted_field_reason_codes_len` entries until parent free.
            unsafe {
                slice::from_raw_parts(
                    raw.omitted_field_reason_codes,
                    raw.omitted_field_reason_codes_len,
                )
            }
        };
        fields
            .iter()
            .enumerate()
            .map(|(index, field)| V4OmittedPreciseAccountingField {
                field: V4PreciseAccountingField::from_raw(field.field),
                reason: optional_c_string(field.reason.cast_const()),
                reason_code: reason_codes
                    .get(index)
                    .and_then(|ptr| optional_c_string((*ptr).cast_const())),
            })
            .collect()
    };
    V4PreciseAccountingBytes {
        unreachable_bytes: (raw.has_unreachable_bytes != 0).then_some(raw.unreachable_bytes),
        retained_history_required_bytes: (raw.has_retained_history_required_bytes != 0)
            .then_some(raw.retained_history_required_bytes),
        popped_skipped_bytes: (raw.has_popped_skipped_bytes != 0)
            .then_some(raw.popped_skipped_bytes),
        reclaimable_bytes: (raw.has_reclaimable_bytes != 0).then_some(raw.reclaimable_bytes),
        omitted_fields,
    }
}

fn copy_v4_current_head_bytes(raw: sys::ArcadiaTioV4CurrentHeadBytes) -> V4CurrentHeadBytes {
    V4CurrentHeadBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
        commit_bytes: raw.commit_bytes,
    }
}

fn copy_v4_audit_bytes(raw: sys::ArcadiaTioV4AuditBytes) -> V4AuditBytes {
    V4AuditBytes {
        commit_bytes: raw.commit_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

fn copy_v4_payload_reuse_bytes(raw: sys::ArcadiaTioV4PayloadReuseBytes) -> V4PayloadReuseBytes {
    V4PayloadReuseBytes {
        resurrected_payload_bytes: raw.resurrected_payload_bytes,
        shared_payload_bytes: raw.shared_payload_bytes,
    }
}

fn copy_v4_superseded_bytes(raw: sys::ArcadiaTioV4SupersededBytes) -> V4SupersededBytes {
    V4SupersededBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

fn new_v4_diagnostics_report() -> sys::ArcadiaTioV4DiagnosticsReport {
    sys::ArcadiaTioV4DiagnosticsReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4DiagnosticsReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        current_head: sys::ArcadiaTioV4CurrentHeadBytes {
            payload_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
            commit_bytes: 0,
        },
        visible_chain_audit: sys::ArcadiaTioV4AuditBytes {
            commit_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
        },
        payload_reuse: sys::ArcadiaTioV4PayloadReuseBytes {
            resurrected_payload_bytes: 0,
            shared_payload_bytes: 0,
        },
        superseded: sys::ArcadiaTioV4SupersededBytes {
            payload_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
        },
        unknown_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_v4_diagnostics_report(raw: &sys::ArcadiaTioV4DiagnosticsReport) -> V4DiagnosticsReport {
    V4DiagnosticsReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    }
}

fn new_v4_diagnostics_precise_report() -> sys::ArcadiaTioV4DiagnosticsPreciseReport {
    sys::ArcadiaTioV4DiagnosticsPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4DiagnosticsPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        current_head: new_v4_diagnostics_report().current_head,
        visible_chain_audit: new_v4_diagnostics_report().visible_chain_audit,
        payload_reuse: new_v4_diagnostics_report().payload_reuse,
        superseded: new_v4_diagnostics_report().superseded,
        unknown_bytes: 0,
        precise_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_diagnostics_precise_report(
    raw: &sys::ArcadiaTioV4DiagnosticsPreciseReport,
) -> V4DiagnosticsPreciseReport {
    V4DiagnosticsPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    }
}

fn new_v4_compaction_analysis_report() -> sys::ArcadiaTioV4CompactionAnalysisReport {
    sys::ArcadiaTioV4CompactionAnalysisReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4CompactionAnalysisReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        policy: sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE,
        source_file_bytes: 0,
        current_state_required_bytes: 0,
        ordinary_reclaimable_bytes: 0,
        unknown_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_v4_compaction_analysis_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisReport,
) -> Result<V4CompactionAnalysisReport> {
    Ok(V4CompactionAnalysisReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    })
}

fn new_v4_compaction_analysis_precise_report() -> sys::ArcadiaTioV4CompactionAnalysisPreciseReport {
    sys::ArcadiaTioV4CompactionAnalysisPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4CompactionAnalysisPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        policy: sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE,
        source_file_bytes: 0,
        current_state_required_bytes: 0,
        ordinary_reclaimable_bytes: 0,
        unknown_bytes: 0,
        precise_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_compaction_analysis_precise_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisPreciseReport,
) -> Result<V4CompactionAnalysisPreciseReport> {
    Ok(V4CompactionAnalysisPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    })
}

fn new_v4_retained_history_compaction_report() -> sys::ArcadiaTioV4RetainedHistoryCompactionReport {
    sys::ArcadiaTioV4RetainedHistoryCompactionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        retained_commit_count: 0,
        retained_commit_seqs: ptr::null_mut(),
        retained_commit_seqs_len: 0,
        has_unretained_older_commit_count: 0,
        unretained_older_commit_count: 0,
        source_file_bytes: 0,
        destination_file_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_retained_commit_seqs(ptr: *mut u64, len: usize) -> Vec<u64> {
    if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: Native report owns `len` entries until the parent report is freed.
        unsafe { slice::from_raw_parts(ptr, len) }.to_vec()
    }
}

fn copy_v4_retained_history_compaction_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionReport,
) -> Result<V4RetainedHistoryCompactionReport> {
    Ok(V4RetainedHistoryCompactionReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        ),
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    })
}

fn new_v4_retained_history_compaction_precise_report()
-> sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport {
    sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        retained_commit_count: 0,
        retained_commit_seqs: ptr::null_mut(),
        retained_commit_seqs_len: 0,
        has_unretained_older_commit_count: 0,
        unretained_older_commit_count: 0,
        source_file_bytes: 0,
        destination_file_bytes: 0,
        precise_source_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_retained_history_compaction_precise_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
) -> Result<V4RetainedHistoryCompactionPreciseReport> {
    Ok(V4RetainedHistoryCompactionPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        ),
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        precise_source_accounting: copy_v4_precise_accounting_bytes(&raw.precise_source_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    })
}

fn new_reform_report() -> sys::ArcadiaTioReformReport {
    sys::ArcadiaTioReformReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReformReport>(),
        reason_code: ptr::null_mut(),
        reason_code_taxonomy: ptr::null_mut(),
        reason: ptr::null_mut(),
    }
}

fn copy_reform_report(raw: &sys::ArcadiaTioReformReport) -> ReformReport {
    ReformReport {
        reason_code: optional_c_string(raw.reason_code.cast_const()),
        reason_code_taxonomy: optional_c_string(raw.reason_code_taxonomy.cast_const()),
        reason: optional_c_string(raw.reason.cast_const()),
    }
}

fn new_auto_compaction_config() -> sys::ArcadiaTioAutoCompactionConfig {
    AutoCompactionConfig::default().to_raw()
}

fn copy_auto_compaction_config(
    raw: sys::ArcadiaTioAutoCompactionConfig,
) -> Result<AutoCompactionConfig> {
    Ok(AutoCompactionConfig {
        enabled: raw.enabled != 0,
        retain_commits: raw.retain_commits,
        dead_ratio_threshold: raw.dead_ratio_threshold,
        min_dead_bytes: raw.min_dead_bytes,
        mode: CompactionMode::from_raw(raw.mode)?,
        check_every_commits: raw.check_every_commits,
        cooldown_commits: raw.cooldown_commits,
    })
}

fn copy_axis_labels(ptr: *mut sys::ArcadiaTioAxisLabel, len: usize) -> Vec<AxisLabel> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| AxisLabel {
            id: item.id,
            name: required_c_string(item.name.cast_const()),
        })
        .collect()
}

fn copy_user_kv(ptr: *mut sys::ArcadiaTioUserKv, len: usize) -> Vec<UserKv> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| UserKv {
            key: required_c_string(item.key.cast_const()),
            value: required_c_string(item.value.cast_const()),
        })
        .collect()
}

fn copy_file_meta(raw: &sys::ArcadiaTioFileMeta) -> Result<FileMeta> {
    let dims = if raw.dims.is_null() || raw.rank == 0 {
        Vec::new()
    } else {
        // SAFETY: Metadata dimension array is valid for `rank` while the native metadata object is alive.
        unsafe { slice::from_raw_parts(raw.dims, raw.rank) }
            .iter()
            .map(|dim| {
                Ok(DimSpec {
                    kind: AxisKind::from_raw(dim.kind)?,
                    len: dim.len,
                    name: optional_c_string(dim.name.cast_const()),
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    Ok(FileMeta {
        dtype: DType::from_raw(raw.dtype)?,
        dims,
        append_dim: raw.append_dim,
        symbols: copy_axis_labels(raw.symbols, raw.symbols_len),
        channels: copy_axis_labels(raw.channels, raw.channels_len),
        user_kv: copy_user_kv(raw.user_kv, raw.user_kv_len),
        effective_profile: HeaderProfile::from_raw(raw.effective_profile)?,
        commit_seq: raw.commit_seq,
    })
}

fn copy_coordinate_meta(
    ptr: *mut sys::ArcadiaTioAxisCoordinateMeta,
    len: usize,
) -> Result<Vec<CoordinateMeta>> {
    if ptr.is_null() || len == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: Coordinate metadata array is valid for `len` until freed by the caller.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| {
            Ok(CoordinateMeta {
                axis: item.axis,
                axis_name_snapshot: optional_c_string(item.axis_name_snapshot.cast_const()),
                name: optional_c_string(item.name.cast_const()),
                kind: CoordinateKind::from_raw(item.kind)?,
                dtype: CoordinateDType::from_raw(item.dtype)?,
                encoding: CoordinateEncoding::from_raw(item.encoding)?,
                length: item.length,
                ordering: CoordinateOrdering {
                    sorted: CoordinateSortedness::from_raw(item.sorted)?,
                    monotonicity: CoordinateMonotonicity::from_raw(item.monotonicity)?,
                    uniqueness: CoordinateUniqueness::from_raw(item.uniqueness)?,
                },
                storage_kind: CoordinateStorageKind::from_raw(item.storage_kind)?,
                external_source_kind: ExternalCoordinateSourceKind::from_raw(
                    item.external_source_kind,
                )?,
                external_uri: optional_c_string(item.external_uri.cast_const()),
                required: item.required != 0,
                validation_status: CoordinateValidationStatus::from_raw(item.validation_status)?,
            })
        })
        .collect()
}

struct PreparedStringList {
    _strings: Vec<CString>,
    ptrs: Vec<*const c_char>,
}

impl PreparedStringList {
    fn new<S: AsRef<str>>(values: &[S], label: &str) -> Result<Self> {
        let strings = values
            .iter()
            .map(|value| string_to_cstring(value.as_ref(), label))
            .collect::<Result<Vec<_>>>()?;
        let ptrs = strings.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _strings: strings,
            ptrs,
        })
    }

    fn ptr(&self) -> *const *const c_char {
        if self.ptrs.is_empty() {
            ptr::null()
        } else {
            self.ptrs.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.ptrs.len()
    }
}

struct PreparedUserKvList {
    _keys: Vec<CString>,
    _values: Vec<CString>,
    key_ptrs: Vec<*const c_char>,
    value_ptrs: Vec<*const c_char>,
}

impl PreparedUserKvList {
    fn new<K, V>(values: &[(K, V)]) -> Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let keys = values
            .iter()
            .map(|(key, _)| string_to_cstring(key.as_ref(), "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = values
            .iter()
            .map(|(_, value)| string_to_cstring(value.as_ref(), "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let key_ptrs = keys.iter().map(|value| value.as_ptr()).collect();
        let value_ptrs = user_values.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _keys: keys,
            _values: user_values,
            key_ptrs,
            value_ptrs,
        })
    }

    fn key_ptr(&self) -> *const *const c_char {
        if self.key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.key_ptrs.as_ptr()
        }
    }

    fn value_ptr(&self) -> *const *const c_char {
        if self.value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.value_ptrs.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.key_ptrs.len()
    }
}

#[allow(dead_code)]
struct PreparedCreate<'a> {
    path: CString,
    dim_kinds: Vec<sys::ArcadiaTioAxisKind>,
    dim_lens: Vec<u32>,
    dim_name_strings: Vec<CString>,
    dim_name_ptrs: Vec<*const c_char>,
    symbols: Vec<CString>,
    symbol_ptrs: Vec<*const c_char>,
    channels: Vec<CString>,
    channel_ptrs: Vec<*const c_char>,
    user_keys: Vec<CString>,
    user_values: Vec<CString>,
    user_key_ptrs: Vec<*const c_char>,
    user_value_ptrs: Vec<*const c_char>,
    coordinate_names: Vec<Option<CString>>,
    coordinate_external_uris: Vec<Option<CString>>,
    coordinate_inputs: Vec<sys::ArcadiaTioAxisCoordinateInput>,
    _coordinate_values: PhantomData<&'a [CoordinateSpec]>,
}

impl<'a> PreparedCreate<'a> {
    fn new(path: impl AsRef<Path>, options: &'a CreateOptions) -> Result<Self> {
        if options.dims.is_empty() {
            return Err(TioError::invalid_argument("rank must be > 0"));
        }
        if options.append_dim >= options.dims.len() {
            return Err(TioError::invalid_argument("append_dim out of range"));
        }
        if options.dims.len() > usize::MAX / 2 {
            return Err(TioError::invalid_argument("rank is too large"));
        }
        for (idx, dim) in options.dims.iter().enumerate() {
            if matches!(dim.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "dimension {idx} name cannot be empty"
                )));
            }
        }

        let path = path_to_cstring(path)?;
        let dim_kinds = options
            .dims
            .iter()
            .map(|dim| dim.kind.to_raw())
            .collect::<Vec<_>>();
        let dim_lens = options.dims.iter().map(|dim| dim.len).collect::<Vec<_>>();

        let dim_name_strings = options
            .dims
            .iter()
            .filter_map(|dim| dim.name.as_ref())
            .map(|name| string_to_cstring(name, "dimension name"))
            .collect::<Result<Vec<_>>>()?;
        let mut dim_name_iter = dim_name_strings.iter();
        let dim_name_ptrs = options
            .dims
            .iter()
            .map(|dim| {
                if dim.name.is_some() {
                    dim_name_iter.next().expect("name count matches").as_ptr()
                } else {
                    ptr::null()
                }
            })
            .collect::<Vec<_>>();

        let symbols = options
            .symbols
            .iter()
            .map(|value| string_to_cstring(value, "symbol label"))
            .collect::<Result<Vec<_>>>()?;
        let symbol_ptrs = symbols
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let channels = options
            .channels
            .iter()
            .map(|value| string_to_cstring(value, "channel label"))
            .collect::<Result<Vec<_>>>()?;
        let channel_ptrs = channels
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_keys = options
            .user_kv
            .iter()
            .map(|(key, _)| string_to_cstring(key, "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = options
            .user_kv
            .iter()
            .map(|(_, value)| string_to_cstring(value, "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let user_key_ptrs = user_keys
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_value_ptrs = user_values
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();

        for (idx, coord) in options.coordinates.iter().enumerate() {
            if coord.axis >= options.dims.len() {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} axis out of range"
                )));
            }
            if matches!(coord.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} name cannot be empty"
                )));
            }
        }
        let coordinate_names = options
            .coordinates
            .iter()
            .map(|coord| {
                coord
                    .name
                    .as_deref()
                    .map(|name| string_to_cstring(name, "coordinate name"))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_external_uris = options
            .coordinates
            .iter()
            .map(|coord| match &coord.storage {
                CoordinateStorage::Inline(_) => Ok(None),
                CoordinateStorage::External { uri, .. } => {
                    string_to_cstring(uri, "external coordinate URI").map(Some)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_inputs = options
            .coordinates
            .iter()
            .enumerate()
            .map(|(idx, coord)| {
                coordinate_input(
                    coord,
                    coordinate_names[idx].as_ref(),
                    coordinate_external_uris[idx].as_ref(),
                )
            })
            .collect::<Vec<_>>();

        Ok(Self {
            path,
            dim_kinds,
            dim_lens,
            dim_name_strings,
            dim_name_ptrs,
            symbols,
            symbol_ptrs,
            channels,
            channel_ptrs,
            user_keys,
            user_values,
            user_key_ptrs,
            user_value_ptrs,
            coordinate_names,
            coordinate_external_uris,
            coordinate_inputs,
            _coordinate_values: PhantomData,
        })
    }

    fn dim_name_ptr(&self) -> *const *const c_char {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            ptr::null()
        } else {
            self.dim_name_ptrs.as_ptr()
        }
    }

    fn dim_name_len(&self) -> usize {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            0
        } else {
            self.dim_name_ptrs.len()
        }
    }

    fn symbol_ptr(&self) -> *const *const c_char {
        if self.symbol_ptrs.is_empty() {
            ptr::null()
        } else {
            self.symbol_ptrs.as_ptr()
        }
    }

    fn symbol_len(&self) -> usize {
        self.symbol_ptrs.len()
    }

    fn channel_ptr(&self) -> *const *const c_char {
        if self.channel_ptrs.is_empty() {
            ptr::null()
        } else {
            self.channel_ptrs.as_ptr()
        }
    }

    fn channel_len(&self) -> usize {
        self.channel_ptrs.len()
    }

    fn user_key_ptr(&self) -> *const *const c_char {
        if self.user_key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_key_ptrs.as_ptr()
        }
    }

    fn user_value_ptr(&self) -> *const *const c_char {
        if self.user_value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_value_ptrs.as_ptr()
        }
    }

    fn user_kv_len(&self) -> usize {
        self.user_key_ptrs.len()
    }

    fn coordinate_ptr(&self) -> *const sys::ArcadiaTioAxisCoordinateInput {
        if self.coordinate_inputs.is_empty() {
            ptr::null()
        } else {
            self.coordinate_inputs.as_ptr()
        }
    }

    fn coordinate_len(&self) -> usize {
        self.coordinate_inputs.len()
    }
}

struct PreparedCreateUniverseOptions {
    axis_identities: Vec<sys::ArcadiaTioAxisIdentityInput>,
}

impl PreparedCreateUniverseOptions {
    fn new(options: &CreateUniverseOptions) -> Self {
        let axis_identities = options
            .axis_identities
            .iter()
            .map(|identity| sys::ArcadiaTioAxisIdentityInput {
                version: 1,
                struct_size: mem::size_of::<sys::ArcadiaTioAxisIdentityInput>(),
                axis: identity.axis,
                mode: identity.mode.to_raw(),
            })
            .collect();
        Self { axis_identities }
    }

    fn raw_options(&self) -> sys::ArcadiaTioCreateWithUniverseOptions {
        sys::ArcadiaTioCreateWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioCreateWithUniverseOptions>(),
            axis_identities: if self.axis_identities.is_empty() {
                ptr::null()
            } else {
                self.axis_identities.as_ptr()
            },
            axis_identities_len: self.axis_identities.len(),
        }
    }
}

struct PreparedAppendUniverseOptions<'a> {
    slot_axes: Vec<Vec<sys::ArcadiaTioUniverseBindingInput>>,
    slots: Vec<sys::ArcadiaTioSlotUniverseBindingInput>,
    remap_axes: Vec<Vec<sys::ArcadiaTioUniverseRemapInput>>,
    remap_slots: Vec<sys::ArcadiaTioSlotUniverseRemapInput>,
    _borrowed: PhantomData<&'a AppendWithUniverseOptions>,
}

impl<'a> PreparedAppendUniverseOptions<'a> {
    fn new(options: &'a AppendWithUniverseOptions) -> Self {
        let slot_axes = options
            .slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseBindingInput {
                        axis: axis.axis,
                        family_uuid: axis.family_uuid,
                        version_uuid: axis.version_uuid,
                        length: axis.length,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let slots = slot_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseBindingInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        let remap_axes = options
            .remap_slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseRemapInput {
                        version: 1,
                        struct_size: mem::size_of::<sys::ArcadiaTioUniverseRemapInput>(),
                        axis: axis.axis,
                        target_family_uuid: axis.target_family_uuid,
                        target_version_uuid: axis.target_version_uuid,
                        target_length: axis.target_length,
                        source_to_target: if axis.source_to_target.is_empty() {
                            ptr::null()
                        } else {
                            axis.source_to_target.as_ptr()
                        },
                        source_to_target_len: axis.source_to_target.len(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let remap_slots = remap_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseRemapInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        Self {
            slot_axes,
            slots,
            remap_axes,
            remap_slots,
            _borrowed: PhantomData,
        }
    }

    fn raw_options(&self) -> sys::ArcadiaTioAppendWithUniverseOptions {
        let _ = (&self.slot_axes, &self.remap_axes);
        sys::ArcadiaTioAppendWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioAppendWithUniverseOptions>(),
            slots: if self.slots.is_empty() {
                ptr::null()
            } else {
                self.slots.as_ptr()
            },
            slots_len: self.slots.len(),
            remap_slots: if self.remap_slots.is_empty() {
                ptr::null()
            } else {
                self.remap_slots.as_ptr()
            },
            remap_slots_len: self.remap_slots.len(),
        }
    }
}

struct PreparedSparseRule {
    sparse_axes: Vec<usize>,
    detector_kind: sys::ArcadiaTioSparseDetectorKind,
    predicate: sys::ArcadiaTioSparseValuePredicate,
    min_absent_fraction: f64,
    min_absent_subtensors: u64,
    fallback: sys::ArcadiaTioSparseFallbackPolicy,
}

impl PreparedSparseRule {
    fn new(rule: &SparseRule) -> Self {
        Self {
            sparse_axes: rule.sparse_axes.clone(),
            detector_kind: rule.detector.to_raw(),
            predicate: rule.predicate.to_raw(),
            min_absent_fraction: rule.min_absent_fraction,
            min_absent_subtensors: rule.min_absent_subtensors,
            fallback: rule.fallback.to_raw(),
        }
    }

    fn raw(&self) -> sys::ArcadiaTioSparseRule {
        sys::ArcadiaTioSparseRule {
            detector_kind: self.detector_kind,
            sparse_axes: if self.sparse_axes.is_empty() {
                ptr::null()
            } else {
                self.sparse_axes.as_ptr()
            },
            sparse_axes_len: self.sparse_axes.len(),
            predicate: self.predicate,
            min_absent_fraction: self.min_absent_fraction,
            min_absent_subtensors: self.min_absent_subtensors,
            fallback: self.fallback,
        }
    }
}

struct PreparedSingleSelector {
    take_indices: Option<Vec<u32>>,
    selector: sys::ArcadiaTioEntrySelector,
}

impl PreparedSingleSelector {
    fn new(selector: &EntrySelector) -> Result<Self> {
        let (take_indices, selector) = match selector {
            EntrySelector::All => (
                None,
                sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
            ),
            EntrySelector::Range { start, end } => {
                if start > end {
                    return Err(TioError::invalid_argument(
                        "selector range start must be <= end",
                    ));
                }
                (
                    None,
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    },
                )
            }
            EntrySelector::Take(indices) => {
                let values = indices.clone();
                let selector = sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                    start: 0,
                    end: 0,
                    indices: if values.is_empty() {
                        ptr::null()
                    } else {
                        values.as_ptr()
                    },
                    indices_len: values.len(),
                };
                (Some(values), selector)
            }
        };
        Ok(Self {
            take_indices,
            selector,
        })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        &self.selector
    }
}

struct PreparedChunkKeys<'a> {
    keys: &'a [ChunkKey],
    raw: Vec<sys::ArcadiaTioChunkKey>,
}

impl<'a> PreparedChunkKeys<'a> {
    fn new(keys: &'a [ChunkKey]) -> Self {
        let raw = keys
            .iter()
            .map(|key| sys::ArcadiaTioChunkKey {
                coords: if key.coords.is_empty() {
                    ptr::null()
                } else {
                    key.coords.as_ptr()
                },
                len: key.coords.len(),
            })
            .collect();
        Self { keys, raw }
    }

    fn ptr(&self) -> *const sys::ArcadiaTioChunkKey {
        let _ = &self.keys;
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.raw.len()
    }
}

struct PreparedReadIndexItems {
    items: Vec<sys::ArcadiaTioReadIndexItem>,
}

impl PreparedReadIndexItems {
    fn new(items: &[ReadIndexItem], rank: usize) -> Result<Self> {
        let mut ellipsis_count = 0usize;
        let mut consuming = 0usize;
        let mut output_rank_without_ellipsis_fill = 0usize;
        for item in items {
            match item {
                ReadIndexItem::All | ReadIndexItem::Slice { .. } => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Index(_) => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::NewAxis => {
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Ellipsis => {
                    ellipsis_count += 1;
                    if ellipsis_count > 1 {
                        return Err(TioError::invalid_argument(
                            "read_index supports at most one ellipsis",
                        ));
                    }
                }
            }
        }
        if consuming > rank {
            return Err(TioError::invalid_argument(
                "read_index has too many axis-consuming items for file rank",
            ));
        }
        let ellipsis_or_padding_fill = rank - consuming;
        let output_rank = output_rank_without_ellipsis_fill
            .checked_add(ellipsis_or_padding_fill)
            .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
        if output_rank == 0 {
            return Err(TioError::invalid_argument(
                "read_index scalar output is unsupported by the C ABI first slice",
            ));
        }
        let items = items
            .iter()
            .map(ReadIndexItem::to_raw)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { items })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioReadIndexItem {
        if self.items.is_empty() {
            ptr::null()
        } else {
            self.items.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

struct PreparedSelectors {
    take_indices: Vec<Vec<u32>>,
    selectors: Vec<sys::ArcadiaTioEntrySelector>,
}

impl PreparedSelectors {
    fn new(selectors: &[EntrySelector], rank: usize) -> Result<Self> {
        if selectors.is_empty() {
            return Ok(Self {
                take_indices: Vec::new(),
                selectors: Vec::new(),
            });
        }
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let take_indices = selectors
            .iter()
            .filter_map(|selector| match selector {
                EntrySelector::Take(indices) => Some(indices.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut next_take = 0usize;
        let mut raw = Vec::with_capacity(selectors.len());
        for selector in selectors {
            let item = match selector {
                EntrySelector::All => sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
                EntrySelector::Range { start, end } => {
                    if start > end {
                        return Err(TioError::invalid_argument(
                            "selector range start must be <= end",
                        ));
                    }
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    }
                }
                EntrySelector::Take(_) => {
                    let values = &take_indices[next_take];
                    next_take += 1;
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                        start: 0,
                        end: 0,
                        indices: if values.is_empty() {
                            ptr::null()
                        } else {
                            values.as_ptr()
                        },
                        indices_len: values.len(),
                    }
                }
            };
            raw.push(item);
        }
        Ok(Self {
            take_indices,
            selectors: raw,
        })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        if self.selectors.is_empty() {
            ptr::null()
        } else {
            self.selectors.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.selectors.len()
    }
}

struct PreparedQueryTraceContext {
    run_id: CString,
    row_id: CString,
    phase: CString,
    language: CString,
    api_surface: CString,
    operation: CString,
    trace_clock: CString,
    repeat_index: u32,
}

impl PreparedQueryTraceContext {
    fn new(context: &QueryTraceContext) -> Result<Self> {
        Ok(Self {
            run_id: non_empty_cstring(&context.run_id, "query trace run_id")?,
            row_id: non_empty_cstring(&context.row_id, "query trace row_id")?,
            phase: non_empty_cstring(&context.phase, "query trace phase")?,
            language: non_empty_cstring(&context.language, "query trace language")?,
            api_surface: non_empty_cstring(&context.api_surface, "query trace api_surface")?,
            operation: non_empty_cstring(&context.operation, "query trace operation")?,
            trace_clock: non_empty_cstring(&context.trace_clock, "query trace trace_clock")?,
            repeat_index: context.repeat_index,
        })
    }

    fn raw_context(&self) -> sys::ArcadiaTioQueryTraceContext {
        sys::ArcadiaTioQueryTraceContext {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceContext>(),
            run_id: self.run_id.as_ptr(),
            row_id: self.row_id.as_ptr(),
            repeat_index: self.repeat_index,
            phase: self.phase.as_ptr(),
            language: self.language.as_ptr(),
            api_surface: self.api_surface.as_ptr(),
            operation: self.operation.as_ptr(),
            trace_clock: self.trace_clock.as_ptr(),
        }
    }
}

fn non_empty_cstring(value: &str, label: &str) -> Result<CString> {
    if value.is_empty() {
        return Err(TioError::invalid_argument(format!(
            "{label} must not be empty"
        )));
    }
    string_to_cstring(value, label)
}

struct PreparedReadWithOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
}

impl PreparedReadWithOptions {
    fn new(options: &ReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadWithOptionsOptions {
        sys::ArcadiaTioReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

struct PreparedHistoricalReadWithOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
}

impl PreparedHistoricalReadWithOptions {
    fn new(options: &HistoricalReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithOptionsOptions {
        sys::ArcadiaTioHistoricalReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

struct PreparedReadShapePolicy {
    explicit_extents: Vec<u64>,
    explicit_universe_axes: Vec<sys::ArcadiaTioExplicitUniverseAxisTarget>,
    explicit_extent_axes: Vec<sys::ArcadiaTioExplicitExtentAxisTarget>,
    policy: sys::ArcadiaTioReadShapePolicyTag,
}

impl PreparedReadShapePolicy {
    fn new(policy: &ReadShapePolicy) -> Self {
        let explicit_extents = match policy {
            ReadShapePolicy::ExplicitExtents(extents) => extents.clone(),
            _ => Vec::new(),
        };
        let explicit_universe_axes = match policy {
            ReadShapePolicy::ExplicitUniverse(axes) => axes.iter().map(raw_universe_axis).collect(),
            ReadShapePolicy::ExplicitUniverseAndExtents { universe_axes, .. } => {
                universe_axes.iter().map(raw_universe_axis).collect()
            }
            _ => Vec::new(),
        };
        let explicit_extent_axes = match policy {
            ReadShapePolicy::ExplicitUniverseAndExtents { extent_axes, .. } => extent_axes
                .iter()
                .map(|axis| sys::ArcadiaTioExplicitExtentAxisTarget {
                    axis: axis.axis,
                    length: axis.length,
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            explicit_extents,
            explicit_universe_axes,
            explicit_extent_axes,
            policy: policy.to_raw_tag(),
        }
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadShapePolicyOptions {
        sys::ArcadiaTioReadShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadShapePolicyOptions>(),
            policy: self.policy,
            explicit_extents: if self.explicit_extents.is_empty() {
                ptr::null()
            } else {
                self.explicit_extents.as_ptr()
            },
            explicit_extents_len: self.explicit_extents.len(),
            explicit_universe_axes: if self.explicit_universe_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_universe_axes.as_ptr()
            },
            explicit_universe_axes_len: self.explicit_universe_axes.len(),
            explicit_extent_axes: if self.explicit_extent_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_extent_axes.as_ptr()
            },
            explicit_extent_axes_len: self.explicit_extent_axes.len(),
        }
    }
}

fn raw_universe_axis(
    axis: &ExplicitUniverseAxisTarget,
) -> sys::ArcadiaTioExplicitUniverseAxisTarget {
    sys::ArcadiaTioExplicitUniverseAxisTarget {
        axis: axis.axis,
        family_uuid: axis.family_uuid,
        version_uuid: axis.version_uuid,
        length: axis.length,
    }
}

struct PreparedReadWithShapePolicyOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
    shape_policy: PreparedReadShapePolicy,
}

impl PreparedReadWithShapePolicyOptions {
    fn new(options: &ReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadWithShapePolicyOptions {
        sys::ArcadiaTioReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

struct PreparedHistoricalReadWithShapePolicyOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
    shape_policy: PreparedReadShapePolicy,
}

impl PreparedHistoricalReadWithShapePolicyOptions {
    fn new(options: &HistoricalReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
        sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

fn coordinate_input(
    coord: &CoordinateSpec,
    name: Option<&CString>,
    external_uri: Option<&CString>,
) -> sys::ArcadiaTioAxisCoordinateInput {
    let (
        storage_kind,
        external_source_kind,
        external_uri_ptr,
        external_dtype,
        external_length,
        values_ptr,
        values_len,
        dtype,
    ) = match &coord.storage {
        CoordinateStorage::Inline(values) => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_INLINE,
            sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            ptr::null(),
            values.dtype().to_raw(),
            0,
            values.as_ptr(),
            values.len(),
            values.dtype(),
        ),
        CoordinateStorage::External {
            source_kind,
            uri: _,
            dtype,
            length,
        } => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL,
            source_kind.to_raw(),
            external_uri.map_or(ptr::null(), |value| value.as_ptr()),
            dtype.to_raw(),
            *length,
            ptr::null(),
            0,
            *dtype,
        ),
    };
    sys::ArcadiaTioAxisCoordinateInput {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioAxisCoordinateInput>(),
        axis: coord.axis,
        name: name.map_or(ptr::null(), |value| value.as_ptr()),
        kind: coord.kind.to_raw(),
        dtype: dtype.to_raw(),
        encoding: coord.encoding.to_raw(),
        values: values_ptr,
        values_len,
        sorted: coord.ordering.sorted.to_raw(),
        monotonicity: coord.ordering.monotonicity.to_raw(),
        uniqueness: coord.ordering.uniqueness.to_raw(),
        storage_kind,
        external_source_kind,
        external_uri: external_uri_ptr,
        external_dtype,
        external_length,
        required: u8::from(coord.required),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_options_validation_rejects_empty_rank() {
        let result = TensorFile::create(
            "unused.tio",
            CreateOptions::streaming(DType::F64, Vec::new(), 0),
        );
        let err = match result {
            Ok(_) => panic!("empty-rank create unexpectedly succeeded"),
            Err(err) => err,
        };
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }

    #[test]
    fn invalid_compression_mode_rejects_before_native_create() {
        let mut options =
            CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0);
        options.compression = Some(CompressionConfig {
            mode: 99,
            codec: sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
            min_payload_bytes: 0,
            zstd_level: 3,
        });
        let path = std::env::temp_dir().join("arcadia_tio_wrapper_invalid_compression_mode.tio");
        let _ = std::fs::remove_file(&path);
        let err = match TensorFile::create(&path, options) {
            Ok(_) => panic!("invalid mode unexpectedly succeeded"),
            Err(err) => err,
        };
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
        assert!(!path.exists());
    }

    #[test]
    fn dtype_sizes_match_first_slice() {
        assert_eq!(DType::F32.size_bytes(), 4);
        assert_eq!(DType::F64.size_bytes(), 8);
        assert_eq!(DType::I32.size_bytes(), 4);
        assert_eq!(DType::I64.size_bytes(), 8);
    }
}
