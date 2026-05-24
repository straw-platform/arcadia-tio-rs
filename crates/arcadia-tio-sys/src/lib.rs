#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

use core::ffi::{c_char, c_double, c_float, c_int, c_void};

/// Current C ABI version expected by this sys crate.
pub const ARCADIA_TIO_ABI_VERSION: u32 = 1;

/// V4 precise reason-code taxonomy string exposed by the C ABI.
pub const ARCADIA_TIO_V4_PRECISE_REASON_CODE_TAXONOMY: &str = "v4.precise.v1";
/// Query parallel reason-code taxonomy string exposed by the C ABI.
pub const ARCADIA_TIO_QUERY_PARALLEL_REASON_CODE_TAXONOMY: &str = "v4.query_parallel.v1";

/// Opaque TensorFile handle owned by the native library.
#[repr(C)]
pub struct ArcadiaTioHandle {
    _private: [u8; 0],
}

/// Thread-local C ABI error code value.
pub type ArcadiaTioErrorCode = c_int;
/// Native payload dtype value.
pub type ArcadiaTioDType = c_int;
/// Compression mode value.
pub type ArcadiaTioCompressionMode = c_int;
/// Compression codec value.
pub type ArcadiaTioCompressionCodec = c_int;
/// Coordinate payload dtype value.
pub type ArcadiaTioCoordinateDType = c_int;
/// Axis coordinate semantic kind value.
pub type ArcadiaTioCoordinateKind = c_int;
/// Axis coordinate integer encoding value.
pub type ArcadiaTioCoordinateEncoding = c_int;
/// Declared sortedness value for coordinate values.
pub type ArcadiaTioCoordinateSortedness = c_int;
/// Declared monotonicity value for coordinate values.
pub type ArcadiaTioCoordinateMonotonicity = c_int;
/// Declared uniqueness value for coordinate values.
pub type ArcadiaTioCoordinateUniqueness = c_int;
/// Coordinate storage location kind value.
pub type ArcadiaTioCoordinateStorageKind = c_int;
/// External coordinate source kind value.
pub type ArcadiaTioCoordinateSourceKind = c_int;
/// Coordinate validation status value.
pub type ArcadiaTioCoordinateValidationStatus = c_int;
/// Tensor axis kind value.
pub type ArcadiaTioAxisKind = c_int;
/// Storage profile selector value used by policy create helpers.
pub type ArcadiaTioStorageProfile = c_int;
/// Storage access kind value used by inferred create helpers.
pub type ArcadiaTioStorageAccessKind = c_int;
/// Expected open/query pattern value used by inferred create helpers.
pub type ArcadiaTioOpenPattern = c_int;
/// File population kind value used by inferred create helpers.
pub type ArcadiaTioFilePopulation = c_int;
/// Metadata stability hint value used by inferred create helpers.
pub type ArcadiaTioMetadataStability = c_int;
/// Header profile value used in loaded metadata.
pub type ArcadiaTioHeaderProfile = c_int;
/// Entry-selector tag value for historical/current selector reads.
pub type ArcadiaTioEntrySelectorTag = c_int;
/// Read execution mode value for option-bearing read APIs.
pub type ArcadiaTioReadExecutionMode = c_int;
/// Read shape policy tag value for current and historical reads.
pub type ArcadiaTioReadShapePolicyTag = c_int;
/// Axis identity mode used by universe-aware create APIs.
pub type ArcadiaTioAxisIdentityMode = c_int;
/// Historical query source kind reported by historical read APIs.
pub type ArcadiaTioHistoricalQuerySourceKind = c_int;
/// Compaction mode selector value.
pub type ArcadiaTioCompactionModeTag = c_int;
/// Reform target layout selector value.
pub type ArcadiaTioReformTargetLayout = c_int;
/// Status value for non-precise V4 report APIs.
pub type ArcadiaTioV4ReportStatus = c_int;
/// Ordinary V4 compaction-analysis policy value.
pub type ArcadiaTioV4CompactionAnalysisPolicy = c_int;
/// Precise-accounting field selector value.
pub type ArcadiaTioV4PreciseAccountingField = c_int;
/// Retained-history compaction policy value.
pub type ArcadiaTioV4RetainedHistoryPolicy = c_int;
/// Sparse-intent detector selector value.
pub type ArcadiaTioSparseDetectorKind = c_int;
/// Sparse-intent value predicate selector value.
pub type ArcadiaTioSparseValuePredicateKind = c_int;
/// Sparse-intent fallback policy selector value.
pub type ArcadiaTioSparseFallbackPolicy = c_int;
/// Sparse-append analysis outcome value.
pub type ArcadiaTioSparseAppendOutcome = c_int;
/// Sparse-append analysis reason-code value.
pub type ArcadiaTioSparseAppendReason = c_int;
/// Read-index item tag value.
pub type ArcadiaTioReadIndexItemTag = c_int;
/// Read-index lowering-kind report value.
pub type ArcadiaTioReadIndexLoweringKind = c_int;

macro_rules! raw_constant {
    ($name:ident: $ty:ty = $value:expr) => {
        #[doc = concat!("Raw C ABI constant `", stringify!($name), "`.")]
        pub const $name: $ty = $value;
    };
}

raw_constant!(ARCADIA_TIO_ERROR_OK: ArcadiaTioErrorCode = 0);
raw_constant!(ARCADIA_TIO_ERROR_INVALID_ARGUMENT: ArcadiaTioErrorCode = 1);
raw_constant!(ARCADIA_TIO_ERROR_UNIMPLEMENTED: ArcadiaTioErrorCode = 2);
raw_constant!(ARCADIA_TIO_ERROR_IO: ArcadiaTioErrorCode = 3);
raw_constant!(ARCADIA_TIO_ERROR_FLATBUFFERS: ArcadiaTioErrorCode = 4);

raw_constant!(ARCADIA_TIO_DTYPE_F32: ArcadiaTioDType = 0);
raw_constant!(ARCADIA_TIO_DTYPE_F64: ArcadiaTioDType = 1);
raw_constant!(ARCADIA_TIO_DTYPE_I32: ArcadiaTioDType = 2);
raw_constant!(ARCADIA_TIO_DTYPE_I64: ArcadiaTioDType = 3);
raw_constant!(ARCADIA_TIO_COMPRESSION_FORCE_OFF: ArcadiaTioCompressionMode = 0);
raw_constant!(ARCADIA_TIO_COMPRESSION_AUTO: ArcadiaTioCompressionMode = 1);
raw_constant!(ARCADIA_TIO_COMPRESSION_FORCE_ON: ArcadiaTioCompressionMode = 2);
raw_constant!(ARCADIA_TIO_COMPRESSION_CODEC_ZSTD: ArcadiaTioCompressionCodec = 0);
raw_constant!(ARCADIA_TIO_COMPRESSION_CODEC_LZ4: ArcadiaTioCompressionCodec = 1);

raw_constant!(ARCADIA_TIO_COORDINATE_DTYPE_I32: ArcadiaTioCoordinateDType = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_DTYPE_I64: ArcadiaTioCoordinateDType = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_KIND_POSITION: ArcadiaTioCoordinateKind = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_KIND_LABEL_ID: ArcadiaTioCoordinateKind = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_KIND_DATE: ArcadiaTioCoordinateKind = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP: ArcadiaTioCoordinateKind = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE: ArcadiaTioCoordinateKind = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_PLAIN: ArcadiaTioCoordinateEncoding = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_DAYS: ArcadiaTioCoordinateEncoding = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD: ArcadiaTioCoordinateEncoding = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_SECONDS: ArcadiaTioCoordinateEncoding = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MILLISECONDS: ArcadiaTioCoordinateEncoding = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MICROSECONDS: ArcadiaTioCoordinateEncoding = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_NANOSECONDS: ArcadiaTioCoordinateEncoding = 6);
raw_constant!(ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN: ArcadiaTioCoordinateSortedness = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_SORTED_ASCENDING: ArcadiaTioCoordinateSortedness = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_SORTED_DESCENDING: ArcadiaTioCoordinateSortedness = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_SORTED_UNSORTED: ArcadiaTioCoordinateSortedness = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_UNKNOWN: ArcadiaTioCoordinateMonotonicity = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_DECREASING: ArcadiaTioCoordinateMonotonicity = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_INCREASING: ArcadiaTioCoordinateMonotonicity = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_INCREASING: ArcadiaTioCoordinateMonotonicity = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_DECREASING: ArcadiaTioCoordinateMonotonicity = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_MONOTONICITY_NOT_MONOTONIC: ArcadiaTioCoordinateMonotonicity = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN: ArcadiaTioCoordinateUniqueness = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE: ArcadiaTioCoordinateUniqueness = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES: ArcadiaTioCoordinateUniqueness = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_STORAGE_INLINE: ArcadiaTioCoordinateStorageKind = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL: ArcadiaTioCoordinateStorageKind = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT: ArcadiaTioCoordinateSourceKind = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH: ArcadiaTioCoordinateSourceKind = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH: ArcadiaTioCoordinateSourceKind = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_URI: ArcadiaTioCoordinateSourceKind = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_VALIDATED: ArcadiaTioCoordinateValidationStatus = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_UNVALIDATED: ArcadiaTioCoordinateValidationStatus = 1);

raw_constant!(ARCADIA_TIO_AXIS_TIME: ArcadiaTioAxisKind = 0);
raw_constant!(ARCADIA_TIO_AXIS_SYMBOL: ArcadiaTioAxisKind = 1);
raw_constant!(ARCADIA_TIO_AXIS_CHANNEL: ArcadiaTioAxisKind = 2);
raw_constant!(ARCADIA_TIO_AXIS_OTHER: ArcadiaTioAxisKind = 3);
raw_constant!(ARCADIA_TIO_STORAGE_BALANCED: ArcadiaTioStorageProfile = 0);
raw_constant!(ARCADIA_TIO_STORAGE_NVME: ArcadiaTioStorageProfile = 1);
raw_constant!(ARCADIA_TIO_STORAGE_HDD: ArcadiaTioStorageProfile = 2);
raw_constant!(ARCADIA_TIO_STORAGE_ACCESS_SEEKABLE_MOUNTED: ArcadiaTioStorageAccessKind = 0);
raw_constant!(ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ: ArcadiaTioStorageAccessKind = 1);
raw_constant!(ARCADIA_TIO_STORAGE_ACCESS_FORWARD_ONLY: ArcadiaTioStorageAccessKind = 2);
raw_constant!(ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT: ArcadiaTioOpenPattern = 0);
raw_constant!(ARCADIA_TIO_OPEN_PATTERN_DATA_HOT: ArcadiaTioOpenPattern = 1);
raw_constant!(ARCADIA_TIO_OPEN_PATTERN_MIXED: ArcadiaTioOpenPattern = 2);
raw_constant!(ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED: ArcadiaTioFilePopulation = 0);
raw_constant!(ARCADIA_TIO_FILE_POPULATION_MANY_SHARDS: ArcadiaTioFilePopulation = 1);
raw_constant!(ARCADIA_TIO_METADATA_STABILITY_STABLE: ArcadiaTioMetadataStability = 0);
raw_constant!(ARCADIA_TIO_METADATA_STABILITY_GROWING: ArcadiaTioMetadataStability = 1);
raw_constant!(ARCADIA_TIO_HEADER_PROFILE_STREAMING: ArcadiaTioHeaderProfile = 0);
raw_constant!(ARCADIA_TIO_HEADER_PROFILE_RANDOM_ACCESS: ArcadiaTioHeaderProfile = 1);
raw_constant!(ARCADIA_TIO_ENTRY_SELECTOR_ALL: ArcadiaTioEntrySelectorTag = 0);
raw_constant!(ARCADIA_TIO_ENTRY_SELECTOR_RANGE: ArcadiaTioEntrySelectorTag = 1);
raw_constant!(ARCADIA_TIO_ENTRY_SELECTOR_TAKE: ArcadiaTioEntrySelectorTag = 2);
raw_constant!(ARCADIA_TIO_READ_EXECUTION_SERIAL: ArcadiaTioReadExecutionMode = 0);
raw_constant!(ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS: ArcadiaTioReadExecutionMode = 1);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE: ArcadiaTioReadShapePolicyTag = 0);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_CURRENT_HEAD: ArcadiaTioReadShapePolicyTag = 1);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_UNION: ArcadiaTioReadShapePolicyTag = 2);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_INTERSECTION: ArcadiaTioReadShapePolicyTag = 3);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_INITIAL_REGISTERED: ArcadiaTioReadShapePolicyTag = 4);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_EXTENTS: ArcadiaTioReadShapePolicyTag = 5);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE: ArcadiaTioReadShapePolicyTag = 6);
raw_constant!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS: ArcadiaTioReadShapePolicyTag = 7);
raw_constant!(ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY: ArcadiaTioAxisIdentityMode = 0);
raw_constant!(ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE: ArcadiaTioAxisIdentityMode = 1);
raw_constant!(ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT: ArcadiaTioHistoricalQuerySourceKind = 0);
raw_constant!(ARCADIA_TIO_COMPACTION_COPY_LIVE: ArcadiaTioCompactionModeTag = 0);
raw_constant!(ARCADIA_TIO_COMPACTION_REBLOCK: ArcadiaTioCompactionModeTag = 1);
raw_constant!(ARCADIA_TIO_REFORM_TARGET_PRESERVE_FAMILY: ArcadiaTioReformTargetLayout = 0);
raw_constant!(ARCADIA_TIO_REFORM_TARGET_WHOLE_APPEND_UNIT: ArcadiaTioReformTargetLayout = 1);
raw_constant!(ARCADIA_TIO_REFORM_TARGET_REGULAR_CHUNKED: ArcadiaTioReformTargetLayout = 2);
raw_constant!(ARCADIA_TIO_V4_REPORT_COMPLETE: ArcadiaTioV4ReportStatus = 0);
raw_constant!(ARCADIA_TIO_V4_REPORT_UNSUPPORTED: ArcadiaTioV4ReportStatus = 1);
raw_constant!(ARCADIA_TIO_V4_REPORT_UNKNOWN: ArcadiaTioV4ReportStatus = 2);
raw_constant!(ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE: ArcadiaTioV4CompactionAnalysisPolicy = 0);
raw_constant!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES: ArcadiaTioV4PreciseAccountingField = 0);
raw_constant!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES: ArcadiaTioV4PreciseAccountingField = 1);
raw_constant!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES: ArcadiaTioV4PreciseAccountingField = 2);
raw_constant!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES: ArcadiaTioV4PreciseAccountingField = 3);
raw_constant!(ARCADIA_TIO_V4_RETAINED_HISTORY_RETAIN_LAST: ArcadiaTioV4RetainedHistoryPolicy = 0);

raw_constant!(ARCADIA_TIO_SPARSE_DETECTOR_NULL_SUBTENSOR: ArcadiaTioSparseDetectorKind = 0);
raw_constant!(ARCADIA_TIO_SPARSE_DETECTOR_PREDICATE_SUBTENSOR: ArcadiaTioSparseDetectorKind = 1);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_NAN: ArcadiaTioSparseValuePredicateKind = 0);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_ZERO: ArcadiaTioSparseValuePredicateKind = 1);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F32: ArcadiaTioSparseValuePredicateKind = 2);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F64: ArcadiaTioSparseValuePredicateKind = 3);
raw_constant!(ARCADIA_TIO_SPARSE_FALLBACK_DENSE: ArcadiaTioSparseFallbackPolicy = 0);
raw_constant!(ARCADIA_TIO_SPARSE_APPEND_SPARSE_REGULAR_CHUNKED: ArcadiaTioSparseAppendOutcome = 0);
raw_constant!(ARCADIA_TIO_SPARSE_APPEND_DENSE_FALLBACK: ArcadiaTioSparseAppendOutcome = 1);
raw_constant!(ARCADIA_TIO_SPARSE_APPEND_REJECT: ArcadiaTioSparseAppendOutcome = 2);
raw_constant!(ARCADIA_TIO_SPARSE_APPEND_SPARSE_CHUNK_TREE: ArcadiaTioSparseAppendOutcome = 3);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_NO_ABSENT_SUBTENSORS_DETECTED: ArcadiaTioSparseAppendReason = 0);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_NOT_BE_EMPTY: ArcadiaTioSparseAppendReason = 1);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_BE_UNIQUE: ArcadiaTioSparseAppendReason = 2);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_OUT_OF_BOUNDS: ArcadiaTioSparseAppendReason = 3);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_EXCLUDE_APPEND_AXIS: ArcadiaTioSparseAppendReason = 4);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_APPEND_AXIS_MUST_BE_ZERO_FOR_CURRENT_ROOT_APPEND: ArcadiaTioSparseAppendReason = 5);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_PREDICATE_DTYPE_MISMATCH: ArcadiaTioSparseAppendReason = 6);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_DENSE_FALLBACK_PRESERVES_EXACT_VALUES: ArcadiaTioSparseAppendReason = 7);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_SPARSE_LOWERING_BELOW_THRESHOLD: ArcadiaTioSparseAppendReason = 8);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_WHOLE_APPEND_UNIT_HAS_NO_SPARSE_PRODUCER_PATH: ArcadiaTioSparseAppendReason = 9);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_BLOCK_SHAPE_UNPUBLISHED: ArcadiaTioSparseAppendReason = 10);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_STABLE_NON_APPEND_EXTENTS: ArcadiaTioSparseAppendReason = 11);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_DENSE_PUBLISHED_LANE_SET: ArcadiaTioSparseAppendReason = 12);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_SPARSE_LOWERING_REQUIRES_STABLE_PUBLISHED_LANE_SET: ArcadiaTioSparseAppendReason = 13);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_TENSOR_CONTAINS_NULLS_THAT_DENSE_FALLBACK_CANNOT_PRESERVE: ArcadiaTioSparseAppendReason = 14);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_LOGICAL_ABSENCE_DOES_NOT_COMPILE_TO_CURRENT_SPARSE_MODEL: ArcadiaTioSparseAppendReason = 15);
raw_constant!(ARCADIA_TIO_SPARSE_REASON_CURRENT_SPARSE_LOWERING_NOT_YET_IMPLEMENTED_FOR_DETECTOR: ArcadiaTioSparseAppendReason = 16);

raw_constant!(ARCADIA_TIO_READ_INDEX_ALL: ArcadiaTioReadIndexItemTag = 0);
raw_constant!(ARCADIA_TIO_READ_INDEX_SLICE: ArcadiaTioReadIndexItemTag = 1);
raw_constant!(ARCADIA_TIO_READ_INDEX_INDEX: ArcadiaTioReadIndexItemTag = 2);
raw_constant!(ARCADIA_TIO_READ_INDEX_NEW_AXIS: ArcadiaTioReadIndexItemTag = 3);
raw_constant!(ARCADIA_TIO_READ_INDEX_ELLIPSIS: ArcadiaTioReadIndexItemTag = 4);
raw_constant!(ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN: ArcadiaTioReadIndexLoweringKind = 0);
raw_constant!(ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ: ArcadiaTioReadIndexLoweringKind = 1);
raw_constant!(ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ_WITH_SHAPE_POSTPROCESS: ArcadiaTioReadIndexLoweringKind = 2);

/// Write-time compression config passed by pointer.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCompressionConfig {
    /// Struct version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Compression mode.
    pub mode: ArcadiaTioCompressionMode,
    /// Compression codec.
    pub codec: ArcadiaTioCompressionCodec,
    /// Auto-mode minimum raw payload bytes.
    pub min_payload_bytes: u32,
    /// Zstd level.
    pub zstd_level: i32,
}

/// Owned raw tensor returned by read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioTensor {
    /// Native-owned data pointer; free with [`arcadia_tio_tensor_free`].
    pub data: *mut u8,
    /// Data length in bytes.
    pub len_bytes: usize,
    /// Rank of the tensor shape.
    pub rank: usize,
    /// Native-owned shape pointer; free with [`arcadia_tio_tensor_free`].
    pub shape: *mut u64,
    /// Payload dtype.
    pub dtype: ArcadiaTioDType,
}

impl Default for ArcadiaTioTensor {
    fn default() -> Self {
        Self {
            data: core::ptr::null_mut(),
            len_bytes: 0,
            rank: 0,
            shape: core::ptr::null_mut(),
            dtype: ARCADIA_TIO_DTYPE_F32,
        }
    }
}

/// Owned dense validity mask returned by dense read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioMask {
    /// Native-owned byte mask pointer; free with [`arcadia_tio_mask_free`].
    pub data: *mut u8,
    /// Number of mask elements.
    pub len: usize,
}

/// Arrow C Data Interface array carrier returned by Arrow read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArrowArray {
    /// Logical length.
    pub length: i64,
    /// Null count, or -1 if unknown.
    pub null_count: i64,
    /// Logical offset.
    pub offset: i64,
    /// Number of buffers.
    pub n_buffers: i64,
    /// Number of child arrays.
    pub n_children: i64,
    /// Pointer to buffer pointers.
    pub buffers: *mut *const c_void,
    /// Pointer to child array pointers.
    pub children: *mut *mut ArrowArray,
    /// Optional dictionary array.
    pub dictionary: *mut ArrowArray,
    /// Release callback; caller must invoke it when done if non-null.
    pub release: Option<unsafe extern "C" fn(*mut ArrowArray)>,
    /// Private native data owned by the release callback.
    pub private_data: *mut c_void,
}

/// Arrow C Data Interface schema carrier returned by Arrow read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArrowSchema {
    /// Format string.
    pub format: *const c_char,
    /// Optional field name.
    pub name: *const c_char,
    /// Optional metadata string.
    pub metadata: *const c_char,
    /// Arrow schema flags.
    pub flags: i64,
    /// Number of child schemas.
    pub n_children: i64,
    /// Pointer to child schema pointers.
    pub children: *mut *mut ArrowSchema,
    /// Optional dictionary schema.
    pub dictionary: *mut ArrowSchema,
    /// Release callback; caller must invoke it when done if non-null.
    pub release: Option<unsafe extern "C" fn(*mut ArrowSchema)>,
    /// Private native data owned by the release callback.
    pub private_data: *mut c_void,
}

/// Compaction behavior selector passed to compaction APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCompactionMode {
    /// Compaction mode tag.
    pub kind: ArcadiaTioCompactionModeTag,
    /// Entry block size used for reblocking modes.
    pub reblock_entry_block_size: u32,
}

/// Shallow compatibility compaction statistics.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCompactionStats {
    /// Bytes considered live by the native implementation.
    pub live_bytes: u64,
    /// Bytes considered dead by the native implementation.
    pub dead_bytes: u64,
    /// Dead-byte ratio reported by the native implementation.
    pub dead_ratio: c_double,
    /// Number of commits represented by the file.
    pub commit_count: u32,
}

/// Reform destination layout options.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReformOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Target layout family.
    pub target_layout: ArcadiaTioReformTargetLayout,
    /// Borrowed RegularChunked block shape.
    pub regular_chunked_block_shape: *const u32,
    /// Number of block-shape entries.
    pub regular_chunked_block_shape_len: usize,
}

/// Native-owned reform diagnostic report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReformReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Native-owned stable reason code string.
    pub reason_code: *mut c_char,
    /// Native-owned reason-code taxonomy string.
    pub reason_code_taxonomy: *mut c_char,
    /// Native-owned human-readable reason string.
    pub reason: *mut c_char,
}

/// Precise-accounting option flags for report-producing APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4PreciseAccountingOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Zero requests every precise field relevant to the report family.
    pub requested_fields_mask: u32,
    /// Nonzero includes human-readable omitted-field reason strings.
    pub include_omitted_field_reasons: u8,
}

/// Precise-accounting field intentionally omitted by a report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4OmittedPreciseAccountingField {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Omitted precise-accounting field id.
    pub field: ArcadiaTioV4PreciseAccountingField,
    /// Native-owned omission reason string.
    pub reason: *mut c_char,
}

/// Precise-accounting byte values plus per-field validity metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4PreciseAccountingBytes {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Nonzero when unreachable_bytes is valid.
    pub has_unreachable_bytes: u8,
    /// Precise unreachable bytes.
    pub unreachable_bytes: u64,
    /// Nonzero when retained_history_required_bytes is valid.
    pub has_retained_history_required_bytes: u8,
    /// Precise bytes required by retained history.
    pub retained_history_required_bytes: u64,
    /// Nonzero when popped_skipped_bytes is valid.
    pub has_popped_skipped_bytes: u8,
    /// Precise popped/skipped bytes.
    pub popped_skipped_bytes: u64,
    /// Nonzero when reclaimable_bytes is valid.
    pub has_reclaimable_bytes: u8,
    /// Precise reclaimable bytes.
    pub reclaimable_bytes: u64,
    /// Native-owned omitted-field array.
    pub omitted_fields: *mut ArcadiaTioV4OmittedPreciseAccountingField,
    /// Number of omitted-field entries.
    pub omitted_fields_len: usize,
    /// Native-owned omitted-field reason-code array aligned with omitted_fields.
    pub omitted_field_reason_codes: *mut *mut c_char,
    /// Number of omitted-field reason-code entries.
    pub omitted_field_reason_codes_len: usize,
}

/// Bytes currently required by the visible head.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4CurrentHeadBytes {
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

/// Audit bytes for the visible commit chain.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4AuditBytes {
    /// Commit bytes.
    pub commit_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// Epoch bytes.
    pub epoch_bytes: u64,
    /// Auxiliary bytes.
    pub aux_bytes: u64,
}

/// Payload reuse byte breakdown.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4PayloadReuseBytes {
    /// Payload bytes resurrected from previous commits.
    pub resurrected_payload_bytes: u64,
    /// Payload bytes shared with other visible data.
    pub shared_payload_bytes: u64,
}

/// Superseded byte breakdown.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4SupersededBytes {
    /// Superseded payload bytes.
    pub payload_bytes: u64,
    /// Superseded index bytes.
    pub index_bytes: u64,
    /// Superseded epoch bytes.
    pub epoch_bytes: u64,
    /// Superseded auxiliary bytes.
    pub aux_bytes: u64,
}

/// Non-precise V4 source-file diagnostics report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4DiagnosticsReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Current-head bytes.
    pub current_head: ArcadiaTioV4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: ArcadiaTioV4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: ArcadiaTioV4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: ArcadiaTioV4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Nonzero when precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: u8,
    /// Native-owned omission reason string.
    pub omitted_unreachable_bytes_reason: *mut c_char,
}

/// Precise V4 source-file diagnostics report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4DiagnosticsPreciseReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Current-head bytes.
    pub current_head: ArcadiaTioV4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: ArcadiaTioV4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: ArcadiaTioV4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: ArcadiaTioV4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting values and validity flags.
    pub precise_accounting: ArcadiaTioV4PreciseAccountingBytes,
    /// Native-owned stable reason code string.
    pub reason_code: *mut c_char,
}

/// Non-precise V4 ordinary compaction analysis report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4CompactionAnalysisReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Compaction policy analyzed.
    pub policy: ArcadiaTioV4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Nonzero when precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: u8,
    /// Native-owned omission reason string.
    pub omitted_unreachable_bytes_reason: *mut c_char,
}

/// Precise V4 ordinary compaction analysis report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4CompactionAnalysisPreciseReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Compaction policy analyzed.
    pub policy: ArcadiaTioV4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting values and validity flags.
    pub precise_accounting: ArcadiaTioV4PreciseAccountingBytes,
    /// Native-owned stable reason code string.
    pub reason_code: *mut c_char,
}

/// Retained-history compaction options.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4RetainedHistoryCompactionOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Retained-history policy.
    pub policy: ArcadiaTioV4RetainedHistoryPolicy,
    /// Number of latest commits to retain for retain-last policy.
    pub retain_last_n: u32,
}

/// Non-precise V4 retained-history compaction report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4RetainedHistoryCompactionReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Count of retained commits.
    pub retained_commit_count: u32,
    /// Native-owned retained commit sequence array.
    pub retained_commit_seqs: *mut u64,
    /// Number of retained commit sequence entries.
    pub retained_commit_seqs_len: usize,
    /// Nonzero when unretained older commit count is present.
    pub has_unretained_older_commit_count: u8,
    /// Number of older commits not retained.
    pub unretained_older_commit_count: u64,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Nonzero when precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: u8,
    /// Native-owned omission reason string.
    pub omitted_unreachable_bytes_reason: *mut c_char,
}

/// Precise V4 retained-history compaction report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioV4RetainedHistoryCompactionPreciseReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Report status.
    pub status: ArcadiaTioV4ReportStatus,
    /// Native-owned status reason string.
    pub reason: *mut c_char,
    /// Count of retained commits.
    pub retained_commit_count: u32,
    /// Native-owned retained commit sequence array.
    pub retained_commit_seqs: *mut u64,
    /// Number of retained commit sequence entries.
    pub retained_commit_seqs_len: usize,
    /// Nonzero when unretained older commit count is present.
    pub has_unretained_older_commit_count: u8,
    /// Number of older commits not retained.
    pub unretained_older_commit_count: u64,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Source-file precise accounting at retained-history compaction time.
    pub precise_source_accounting: ArcadiaTioV4PreciseAccountingBytes,
    /// Native-owned stable reason code string.
    pub reason_code: *mut c_char,
}

/// Sparse-intent value predicate passed inside a sparse rule.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSparseValuePredicate {
    /// Predicate kind.
    pub kind: ArcadiaTioSparseValuePredicateKind,
    /// Comparison value for equal predicates; ignored for other predicate kinds.
    pub value: c_double,
}

/// Sparse-intent lowering rule borrowed by sparse append APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSparseRule {
    /// Detector kind.
    pub detector_kind: ArcadiaTioSparseDetectorKind,
    /// Borrowed sparse-axis indices.
    pub sparse_axes: *const usize,
    /// Number of sparse-axis indices.
    pub sparse_axes_len: usize,
    /// Predicate used by predicate detectors.
    pub predicate: ArcadiaTioSparseValuePredicate,
    /// Minimum absent fraction required for sparse lowering.
    pub min_absent_fraction: c_double,
    /// Minimum absent subtensor count required for sparse lowering.
    pub min_absent_subtensors: u64,
    /// Dense fallback policy.
    pub fallback: ArcadiaTioSparseFallbackPolicy,
}

/// Sparse-append analysis report returned by sparse analysis APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSparseAppendAnalysis {
    /// Selected append outcome.
    pub outcome: ArcadiaTioSparseAppendOutcome,
    /// Fraction of absent subtensors detected.
    pub absent_fraction: c_double,
    /// Count of absent subtensors.
    pub absent_subtensor_count: u64,
    /// Count of total subtensors considered.
    pub total_subtensor_count: u64,
    /// Native-owned reason-code array; free with [`arcadia_tio_sparse_append_analysis_free`].
    pub reasons: *mut ArcadiaTioSparseAppendReason,
    /// Number of reason codes.
    pub reasons_len: usize,
}

/// Auto-compaction configuration stored in file metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAutoCompactionConfig {
    /// Nonzero when auto-compaction is enabled.
    pub enabled: u8,
    /// Commit retention count for compaction.
    pub retain_commits: u32,
    /// Dead-byte ratio threshold.
    pub dead_ratio_threshold: c_double,
    /// Minimum dead bytes before compaction can trigger.
    pub min_dead_bytes: u64,
    /// Compaction mode.
    pub mode: ArcadiaTioCompactionMode,
    /// Commit interval for auto-compaction checks.
    pub check_every_commits: u32,
    /// Commit cooldown after compaction.
    pub cooldown_commits: u32,
}

/// Auto-compaction state stored in file metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCompactionState {
    /// Last compacted commit sequence.
    pub last_compacted_commit_seq: u64,
    /// Last compaction timestamp in Unix milliseconds.
    pub last_compacted_at_unix_ms: u64,
}

/// Scalar return value for scalar reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioScalar {
    /// Scalar dtype.
    pub dtype: ArcadiaTioDType,
    /// Scalar value represented as a C double by the current C ABI.
    pub value: c_double,
}

/// Entry selector borrowed by selector read and mutation APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioEntrySelector {
    /// Selector tag.
    pub kind: ArcadiaTioEntrySelectorTag,
    /// Range start.
    pub start: u32,
    /// Range end.
    pub end: u32,
    /// Borrowed index pointer for take selectors.
    pub indices: *const u32,
    /// Number of indices.
    pub indices_len: usize,
}

/// Chunk key borrowed by clear-block mutation APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioChunkKey {
    /// Borrowed chunk coordinate pointer.
    pub coords: *const u32,
    /// Number of chunk coordinates.
    pub len: usize,
}

/// Commit metadata returned by history APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCommitInfo {
    /// Commit sequence number.
    pub commit_seq: u64,
    /// Footer offset for this commit.
    pub footer_offset: u64,
    /// Previous footer offset.
    pub prev_footer_offset: u64,
}

/// Native-owned commit list returned by history APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCommitList {
    /// Native-owned commit array; free with [`arcadia_tio_commit_list_free`].
    pub items: *mut ArcadiaTioCommitInfo,
    /// Number of commits.
    pub len: usize,
}

/// Explicit universe target for shape-policy reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioExplicitUniverseAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Universe family UUID bytes.
    pub family_uuid: [u8; 16],
    /// Universe version UUID bytes.
    pub version_uuid: [u8; 16],
    /// Target universe length.
    pub length: u64,
}

/// Explicit extent target for split-domain shape-policy reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioExplicitExtentAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Target axis length.
    pub length: u64,
}

/// Axis identity descriptor for universe-aware create APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAxisIdentityInput {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: u32,
    /// Axis identity mode.
    pub mode: ArcadiaTioAxisIdentityMode,
}

/// Universe binding for one axis in one appended slot.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioUniverseBindingInput {
    /// Axis index.
    pub axis: u32,
    /// Universe family UUID bytes.
    pub family_uuid: [u8; 16],
    /// Universe version UUID bytes.
    pub version_uuid: [u8; 16],
    /// Source universe length.
    pub length: u64,
}

/// Borrowed universe bindings for one appended slot.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSlotUniverseBindingInput {
    /// Borrowed universe binding array.
    pub axes: *const ArcadiaTioUniverseBindingInput,
    /// Number of axis bindings.
    pub axes_len: usize,
}

/// Optional universe remap for one axis in one appended slot.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioUniverseRemapInput {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: u32,
    /// Target universe family UUID bytes.
    pub target_family_uuid: [u8; 16],
    /// Target universe version UUID bytes.
    pub target_version_uuid: [u8; 16],
    /// Target universe length.
    pub target_length: u64,
    /// Borrowed source-to-target index mapping.
    pub source_to_target: *const u64,
    /// Number of mapping entries.
    pub source_to_target_len: usize,
}

/// Borrowed universe remaps for one appended slot.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSlotUniverseRemapInput {
    /// Borrowed universe remap array.
    pub axes: *const ArcadiaTioUniverseRemapInput,
    /// Number of axis remaps.
    pub axes_len: usize,
}

/// Universe-aware create options.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioCreateWithUniverseOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed axis identity array.
    pub axis_identities: *const ArcadiaTioAxisIdentityInput,
    /// Number of axis identity descriptors.
    pub axis_identities_len: usize,
}

/// Universe-aware append options.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAppendWithUniverseOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed per-slot universe binding array.
    pub slots: *const ArcadiaTioSlotUniverseBindingInput,
    /// Number of appended slots.
    pub slots_len: usize,
    /// Borrowed per-slot universe remap array.
    pub remap_slots: *const ArcadiaTioSlotUniverseRemapInput,
    /// Number of remap slots.
    pub remap_slots_len: usize,
}

/// Read shape policy options for current and historical reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadShapePolicyOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Shape policy tag.
    pub policy: ArcadiaTioReadShapePolicyTag,
    /// Borrowed explicit extents for all axes.
    pub explicit_extents: *const u64,
    /// Number of explicit extents.
    pub explicit_extents_len: usize,
    /// Borrowed explicit universe axis targets.
    pub explicit_universe_axes: *const ArcadiaTioExplicitUniverseAxisTarget,
    /// Number of explicit universe axis targets.
    pub explicit_universe_axes_len: usize,
    /// Borrowed explicit extent axis targets for split-domain policies.
    pub explicit_extent_axes: *const ArcadiaTioExplicitExtentAxisTarget,
    /// Number of explicit extent axis targets.
    pub explicit_extent_axes_len: usize,
}

/// Current read options with execution mode only.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadWithOptionsOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub mode: ArcadiaTioReadExecutionMode,
    /// Maximum thread count for parallel execution.
    pub max_threads: usize,
}

/// Current read options with execution mode and shape policy.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadWithShapePolicyOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub mode: ArcadiaTioReadExecutionMode,
    /// Maximum thread count for parallel execution.
    pub max_threads: usize,
    /// Shape policy options.
    pub shape_policy: ArcadiaTioReadShapePolicyOptions,
}

/// Historical read options with execution mode only.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioHistoricalReadWithOptionsOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub mode: ArcadiaTioReadExecutionMode,
    /// Maximum thread count for parallel execution.
    pub max_threads: usize,
}

/// Historical read options with execution mode and shape policy.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioHistoricalReadWithShapePolicyOptions {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub mode: ArcadiaTioReadExecutionMode,
    /// Maximum thread count for parallel execution.
    pub max_threads: usize,
    /// Shape policy options.
    pub shape_policy: ArcadiaTioReadShapePolicyOptions,
}

/// Current read execution report returned by option-bearing current reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadExecutionReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub requested_mode: ArcadiaTioReadExecutionMode,
    /// Requested maximum query threads.
    pub query_max_threads: usize,
    /// Effective execution mode used by the query.
    pub query_effective_mode: ArcadiaTioReadExecutionMode,
    /// Effective thread count used by the query.
    pub query_effective_threads: usize,
    /// Native-owned query parallel runtime string.
    pub query_parallel_runtime: *mut c_char,
    /// Native-owned query parallel fallback reason string.
    pub query_parallel_fallback_reason: *mut c_char,
    /// Native-owned query parallel reason code string.
    pub query_parallel_reason_code: *mut c_char,
    /// Native-owned query parallel reason-code taxonomy string.
    pub query_parallel_reason_code_taxonomy: *mut c_char,
}

/// Query trace context borrowed by attributed read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioQueryTraceContext {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed run identifier string.
    pub run_id: *const c_char,
    /// Borrowed row identifier string.
    pub row_id: *const c_char,
    /// Repeat index for benchmark-style callers.
    pub repeat_index: u32,
    /// Borrowed phase name string.
    pub phase: *const c_char,
    /// Borrowed language name string.
    pub language: *const c_char,
    /// Borrowed API surface name string.
    pub api_surface: *const c_char,
    /// Borrowed operation name string.
    pub operation: *const c_char,
    /// Borrowed trace-clock label string.
    pub trace_clock: *const c_char,
}

/// Native-owned JSON trace returned by attributed read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioQueryTraceJson {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Native-owned JSON string; free with [`arcadia_tio_query_trace_json_free`].
    pub json: *mut c_char,
}

/// Read-index item borrowed by low-level index read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadIndexItem {
    /// Item tag.
    pub kind: ArcadiaTioReadIndexItemTag,
    /// Nonzero when `start` is present.
    pub has_start: u8,
    /// Slice start value.
    pub start: i64,
    /// Nonzero when `end` is present.
    pub has_end: u8,
    /// Slice end value.
    pub end: i64,
    /// Slice step value.
    pub step: i64,
    /// Scalar index value.
    pub index: i64,
}

/// Read-index lowering report returned by low-level index read APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioReadIndexReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Lowering strategy selected by native code.
    pub lowering_kind: ArcadiaTioReadIndexLoweringKind,
    /// Nonzero when native code used a full-tensor fallback.
    pub used_full_tensor_fallback: u8,
    /// Reserved padding bytes.
    pub reserved0: [u8; 7],
}

/// Historical read execution report returned by option-bearing historical reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioHistoricalReadExecutionReport {
    /// Structure version; set to 1.
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested execution mode.
    pub requested_mode: ArcadiaTioReadExecutionMode,
    /// Requested maximum query threads.
    pub query_max_threads: usize,
    /// Effective execution mode used by the query.
    pub query_effective_mode: ArcadiaTioReadExecutionMode,
    /// Effective thread count used by the query.
    pub query_effective_threads: usize,
    /// Native-owned query parallel runtime string.
    pub query_parallel_runtime: *mut c_char,
    /// Native-owned query parallel fallback reason string.
    pub query_parallel_fallback_reason: *mut c_char,
    /// Native-owned query parallel reason code string.
    pub query_parallel_reason_code: *mut c_char,
    /// Native-owned query parallel reason-code taxonomy string.
    pub query_parallel_reason_code_taxonomy: *mut c_char,
    /// Historical query source kind.
    pub query_source_kind: ArcadiaTioHistoricalQuerySourceKind,
    /// Commit sequence used for the historical query.
    pub query_commit_seq: u64,
}

/// Chunk plan returned by metadata APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioChunkPlan {
    /// Native-owned block-size array; free with [`arcadia_tio_chunk_plan_free`].
    pub block_sizes: *mut u32,
    /// Number of block sizes.
    pub len: usize,
}

/// Axis label item in file metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAxisLabel {
    /// Numeric label id.
    pub id: u32,
    /// Native-owned label name pointer.
    pub name: *mut c_char,
}

/// User metadata key/value item in file metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioUserKv {
    /// Native-owned key pointer.
    pub key: *mut c_char,
    /// Native-owned value pointer.
    pub value: *mut c_char,
}

/// Dimension metadata item in file metadata.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioDimSpec {
    /// Axis kind.
    pub kind: ArcadiaTioAxisKind,
    /// Current axis length.
    pub len: u32,
    /// Native-owned optional axis name pointer.
    pub name: *mut c_char,
}

/// Owned file metadata returned by load-meta APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioFileMeta {
    /// Payload dtype.
    pub dtype: ArcadiaTioDType,
    /// Native-owned dimension array.
    pub dims: *mut ArcadiaTioDimSpec,
    /// Number of dimensions.
    pub rank: usize,
    /// Append dimension index.
    pub append_dim: usize,
    /// Native-owned symbol labels.
    pub symbols: *mut ArcadiaTioAxisLabel,
    /// Number of symbol labels.
    pub symbols_len: usize,
    /// Native-owned channel labels.
    pub channels: *mut ArcadiaTioAxisLabel,
    /// Number of channel labels.
    pub channels_len: usize,
    /// Native-owned user key/value metadata.
    pub user_kv: *mut ArcadiaTioUserKv,
    /// Number of user key/value items.
    pub user_kv_len: usize,
    /// Effective header profile.
    pub effective_profile: ArcadiaTioHeaderProfile,
    /// Current head commit sequence.
    pub commit_seq: u64,
}

/// Borrowed coordinate input descriptor for create APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAxisCoordinateInput {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: usize,
    /// Borrowed coordinate name.
    pub name: *const c_char,
    /// Coordinate kind.
    pub kind: ArcadiaTioCoordinateKind,
    /// Coordinate dtype.
    pub dtype: ArcadiaTioCoordinateDType,
    /// Coordinate encoding.
    pub encoding: ArcadiaTioCoordinateEncoding,
    /// Borrowed dense no-null values pointer for inline coordinates.
    pub values: *const c_void,
    /// Number of coordinate values.
    pub values_len: usize,
    /// Sortedness declaration.
    pub sorted: ArcadiaTioCoordinateSortedness,
    /// Monotonicity declaration.
    pub monotonicity: ArcadiaTioCoordinateMonotonicity,
    /// Uniqueness declaration.
    pub uniqueness: ArcadiaTioCoordinateUniqueness,
    /// Storage kind.
    pub storage_kind: ArcadiaTioCoordinateStorageKind,
    /// External source kind.
    pub external_source_kind: ArcadiaTioCoordinateSourceKind,
    /// Borrowed external URI pointer.
    pub external_uri: *const c_char,
    /// External coordinate dtype.
    pub external_dtype: ArcadiaTioCoordinateDType,
    /// External coordinate length.
    pub external_length: u64,
    /// Nonzero when coordinate is required.
    pub required: u8,
}

/// Owned coordinate metadata returned by metadata APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioAxisCoordinateMeta {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: usize,
    /// Native-owned axis name snapshot pointer.
    pub axis_name_snapshot: *mut c_char,
    /// Native-owned coordinate name pointer.
    pub name: *mut c_char,
    /// Coordinate kind.
    pub kind: ArcadiaTioCoordinateKind,
    /// Coordinate dtype.
    pub dtype: ArcadiaTioCoordinateDType,
    /// Coordinate encoding.
    pub encoding: ArcadiaTioCoordinateEncoding,
    /// Coordinate length.
    pub length: u64,
    /// Sortedness declaration.
    pub sorted: ArcadiaTioCoordinateSortedness,
    /// Monotonicity declaration.
    pub monotonicity: ArcadiaTioCoordinateMonotonicity,
    /// Uniqueness declaration.
    pub uniqueness: ArcadiaTioCoordinateUniqueness,
    /// Storage kind.
    pub storage_kind: ArcadiaTioCoordinateStorageKind,
    /// External source kind.
    pub external_source_kind: ArcadiaTioCoordinateSourceKind,
    /// Native-owned external URI pointer.
    pub external_uri: *mut c_char,
    /// Nonzero when coordinate is required.
    pub required: u8,
    /// Coordinate validation status.
    pub validation_status: ArcadiaTioCoordinateValidationStatus,
}

// Safety: these declarations are raw FFI bindings to `arcadia_tio_capi`. Callers must
// uphold the pointer, ownership, lifetime, shape, dtype, and thread-local-error contracts
// documented by the C headers. Functions returning owned buffers require the matching
// `arcadia_tio_*_free` function exactly once; borrowed input pointers must remain valid
// for the duration of the call.
unsafe extern "C" {
    /// Returns a borrowed pointer to the last error message for the current thread.
    pub fn arcadia_tio_last_error_message() -> *const c_char;
    /// Returns the last error code for the current thread.
    pub fn arcadia_tio_last_error_code() -> ArcadiaTioErrorCode;
    /// Returns the native library ABI version.
    pub fn arcadia_tio_abi_version() -> u32;

    /// Sets write-time compression for future appends.
    pub fn arcadia_tio_set_compression_config(
        handle: *mut ArcadiaTioHandle,
        config: *const ArcadiaTioCompressionConfig,
    ) -> c_int;
    /// Gets write-time compression for future appends.
    pub fn arcadia_tio_get_compression_config(
        handle: *const ArcadiaTioHandle,
        out_config: *mut ArcadiaTioCompressionConfig,
    ) -> c_int;

    /// Creates a random-access V4 TensorFile.
    pub fn arcadia_tio_create_random_access(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with metadata overrides.
    pub fn arcadia_tio_create_random_access_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with universe-aware axis identity options.
    pub fn arcadia_tio_create_random_access_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile.
    pub fn arcadia_tio_create_streaming(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with metadata overrides.
    pub fn arcadia_tio_create_streaming_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with universe-aware axis identity options.
    pub fn arcadia_tio_create_streaming_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile using inferred layout-family selection.
    pub fn arcadia_tio_create_inferred(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile using inferred layout-family selection and metadata overrides.
    pub fn arcadia_tio_create_inferred_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking.
    pub fn arcadia_tio_create_with_policy(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking and metadata overrides.
    pub fn arcadia_tio_create_with_policy_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking and universe options.
    pub fn arcadia_tio_create_with_policy_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with coordinate descriptors.
    pub fn arcadia_tio_create_random_access_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with coordinate descriptors.
    pub fn arcadia_tio_create_streaming_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Opens an existing TensorFile.
    pub fn arcadia_tio_open(path: *const c_char) -> *mut ArcadiaTioHandle;
    /// Closes a handle returned by create/open functions.
    pub fn arcadia_tio_close(handle: *mut ArcadiaTioHandle);

    /// Loads file metadata without opening a handle.
    pub fn arcadia_tio_load_meta(path: *const c_char, out_meta: *mut ArcadiaTioFileMeta) -> c_int;
    /// Reads coordinate descriptors from an open handle.
    pub fn arcadia_tio_coordinate_meta(
        handle: *mut ArcadiaTioHandle,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMeta,
        out_len: *mut usize,
    ) -> c_int;
    /// Loads coordinate descriptors without opening a handle.
    pub fn arcadia_tio_load_coordinate_meta(
        path: *const c_char,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMeta,
        out_len: *mut usize,
    ) -> c_int;
    /// Frees coordinate metadata arrays returned by metadata APIs.
    pub fn arcadia_tio_axis_coordinate_meta_free(
        meta: *mut ArcadiaTioAxisCoordinateMeta,
        len: usize,
    );
    /// Reads inline axis coordinate values into an owned tensor.
    pub fn arcadia_tio_read_axis_coordinates(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        out_values: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Looks up the unique axis index for an inline validated i32 coordinate value.
    pub fn arcadia_tio_coordinate_index_i32(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        value: i32,
        out_index: *mut u32,
    ) -> c_int;
    /// Looks up the unique axis index for an inline validated i64 coordinate value.
    pub fn arcadia_tio_coordinate_index_i64(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        value: i64,
        out_index: *mut u32,
    ) -> c_int;
    /// Looks up the half-open axis-index range for an inclusive i32 coordinate interval.
    pub fn arcadia_tio_coordinate_range_i32(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: i32,
        end: i32,
        out_start: *mut u32,
        out_end: *mut u32,
    ) -> c_int;
    /// Looks up the half-open axis-index range for an inclusive i64 coordinate interval.
    pub fn arcadia_tio_coordinate_range_i64(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: i64,
        end: i64,
        out_start: *mut u32,
        out_end: *mut u32,
    ) -> c_int;

    /// Reads the full tensor into a native-owned raw tensor.
    pub fn arcadia_tio_read_all(
        handle: *mut ArcadiaTioHandle,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads the full tensor values as native-owned Arrow C Data array/schema carriers.
    pub fn arcadia_tio_read_values_arrow(
        handle: *mut ArcadiaTioHandle,
        out_array: *mut ArrowArray,
        out_schema: *mut ArrowSchema,
    ) -> c_int;
    /// Reads the full tensor into a dense tensor and optional native-owned mask.
    pub fn arcadia_tio_read_all_dense(
        handle: *mut ArcadiaTioHandle,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
    ) -> c_int;
    /// Frees native-owned tensor buffers.
    pub fn arcadia_tio_tensor_free(tensor: *mut ArcadiaTioTensor);
    /// Frees native-owned mask buffers.
    pub fn arcadia_tio_mask_free(mask: *mut ArcadiaTioMask);

    /// Appends f32 payload data.
    pub fn arcadia_tio_append_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends f32 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_f32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f32 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_f32_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data.
    pub fn arcadia_tio_append_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends f64 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_f64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_f64_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data.
    pub fn arcadia_tio_append_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends i32 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_i32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_i32_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data.
    pub fn arcadia_tio_append_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends i64 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_i64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_i64_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;

    /// Analyzes how sparse-intent f32 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent f64 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i32 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i64 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Appends f32 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends f32 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_f32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends f64 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_f64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends i32 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_i32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends i64 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_i64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Frees native-owned reason arrays in a sparse append analysis.
    pub fn arcadia_tio_sparse_append_analysis_free(analysis: *mut ArcadiaTioSparseAppendAnalysis);

    /// Rewrites one selected entry with f32 payload data.
    pub fn arcadia_tio_rewrite_f32(
        handle: *mut ArcadiaTioHandle,
        selector: *const ArcadiaTioEntrySelector,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites one selected entry with f64 payload data.
    pub fn arcadia_tio_rewrite_f64(
        handle: *mut ArcadiaTioHandle,
        selector: *const ArcadiaTioEntrySelector,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites a selector slice with f32 payload data.
    pub fn arcadia_tio_rewrite_slice_f32(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites a selector slice with f64 payload data.
    pub fn arcadia_tio_rewrite_slice_f64(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Clears storage blocks for borrowed chunk keys.
    pub fn arcadia_tio_clear_blocks(
        handle: *mut ArcadiaTioHandle,
        keys: *const ArcadiaTioChunkKey,
        keys_len: usize,
    ) -> c_int;

    /// Returns shallow compatibility compaction stats for an open handle.
    pub fn arcadia_tio_analyze_compaction(
        handle: *mut ArcadiaTioHandle,
        out_stats: *mut ArcadiaTioCompactionStats,
    ) -> c_int;
    /// Returns non-precise V4 source-file diagnostics.
    pub fn arcadia_tio_v4_diagnostics(
        handle: *mut ArcadiaTioHandle,
        out_report: *mut ArcadiaTioV4DiagnosticsReport,
    ) -> c_int;
    /// Frees native-owned strings in a V4 diagnostics report.
    pub fn arcadia_tio_v4_diagnostics_report_free(report: *mut ArcadiaTioV4DiagnosticsReport);
    /// Returns precise V4 source-file diagnostics.
    pub fn arcadia_tio_v4_diagnostics_precise(
        handle: *mut ArcadiaTioHandle,
        options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4DiagnosticsPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise V4 diagnostics report.
    pub fn arcadia_tio_v4_diagnostics_precise_report_free(
        report: *mut ArcadiaTioV4DiagnosticsPreciseReport,
    );
    /// Returns non-precise V4 current-state compaction analysis.
    pub fn arcadia_tio_analyze_v4_compaction(
        handle: *mut ArcadiaTioHandle,
        out_report: *mut ArcadiaTioV4CompactionAnalysisReport,
    ) -> c_int;
    /// Frees native-owned strings in a V4 compaction analysis report.
    pub fn arcadia_tio_v4_compaction_analysis_report_free(
        report: *mut ArcadiaTioV4CompactionAnalysisReport,
    );
    /// Returns precise V4 current-state compaction analysis.
    pub fn arcadia_tio_analyze_v4_compaction_precise(
        handle: *mut ArcadiaTioHandle,
        options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4CompactionAnalysisPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise V4 compaction analysis report.
    pub fn arcadia_tio_v4_compaction_analysis_precise_report_free(
        report: *mut ArcadiaTioV4CompactionAnalysisPreciseReport,
    );
    /// Compacts live chunks into a destination file.
    pub fn arcadia_tio_compact_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        retain_commits: u32,
        mode: ArcadiaTioCompactionMode,
    ) -> c_int;
    /// Conditionally compacts live chunks into a destination file.
    pub fn arcadia_tio_maybe_compact(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        dead_ratio_threshold: c_double,
        min_dead_bytes: u64,
        retain_commits: u32,
        mode: ArcadiaTioCompactionMode,
        out_compacted: *mut u8,
    ) -> c_int;
    /// Reads auto-compaction metadata configuration.
    pub fn arcadia_tio_get_auto_compaction_config(
        handle: *mut ArcadiaTioHandle,
        out_config: *mut ArcadiaTioAutoCompactionConfig,
        out_has_config: *mut u8,
    ) -> c_int;
    /// Updates auto-compaction metadata configuration.
    pub fn arcadia_tio_set_auto_compaction_config(
        handle: *mut ArcadiaTioHandle,
        config: *const ArcadiaTioAutoCompactionConfig,
        has_config: u8,
    ) -> c_int;
    /// Reads auto-compaction state metadata.
    pub fn arcadia_tio_compaction_state(
        handle: *mut ArcadiaTioHandle,
        out_state: *mut ArcadiaTioCompactionState,
        out_has_state: *mut u8,
    ) -> c_int;
    /// Runs metadata-configured auto-compaction if thresholds trigger.
    pub fn arcadia_tio_maybe_compact_auto(
        handle: *mut ArcadiaTioHandle,
        out_compacted: *mut u8,
    ) -> c_int;
    /// Reforms visible data into a destination file.
    pub fn arcadia_tio_reform_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioReformOptions,
    ) -> c_int;
    /// Reforms visible data into a destination file with diagnostic report output.
    pub fn arcadia_tio_reform_to_ex(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioReformOptions,
        out_report: *mut ArcadiaTioReformReport,
    ) -> c_int;
    /// Frees native-owned strings in a reform report.
    pub fn arcadia_tio_reform_report_free(report: *mut ArcadiaTioReformReport);
    /// Compacts a V4 file while retaining bounded visible commit history.
    pub fn arcadia_tio_compact_v4_retained_history_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioV4RetainedHistoryCompactionOptions,
        out_report: *mut ArcadiaTioV4RetainedHistoryCompactionReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a retained-history compaction report.
    pub fn arcadia_tio_v4_retained_history_compaction_report_free(
        report: *mut ArcadiaTioV4RetainedHistoryCompactionReport,
    );
    /// Compacts a V4 file while retaining bounded history and precise source accounting.
    pub fn arcadia_tio_compact_v4_retained_history_to_precise(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        retention_options: *const ArcadiaTioV4RetainedHistoryCompactionOptions,
        precise_options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise retained-history compaction report.
    pub fn arcadia_tio_v4_retained_history_compaction_precise_report_free(
        report: *mut ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
    );

    /// Reads rank for an open handle.
    pub fn arcadia_tio_rank(handle: *mut ArcadiaTioHandle, out_rank: *mut usize) -> c_int;
    /// Reads dtype for an open handle.
    pub fn arcadia_tio_dtype(
        handle: *mut ArcadiaTioHandle,
        out_dtype: *mut ArcadiaTioDType,
    ) -> c_int;
    /// Reads append-axis index for an open handle.
    pub fn arcadia_tio_append_axis(
        handle: *mut ArcadiaTioHandle,
        out_append_axis: *mut usize,
    ) -> c_int;
    /// Reads index-checkpoint interval metadata.
    pub fn arcadia_tio_get_index_checkpoint_every_commits(
        handle: *mut ArcadiaTioHandle,
        out_every_commits: *mut u32,
    ) -> c_int;
    /// Updates index-checkpoint interval metadata.
    pub fn arcadia_tio_set_index_checkpoint_every_commits(
        handle: *mut ArcadiaTioHandle,
        every_commits: u32,
    ) -> c_int;
    /// Updates or clears one dimension name.
    pub fn arcadia_tio_set_dim_name(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        name: *const c_char,
        has_name: u8,
    ) -> c_int;
    /// Replaces Symbol-axis labels from borrowed strings.
    pub fn arcadia_tio_set_symbols(
        handle: *mut ArcadiaTioHandle,
        symbols: *const *const c_char,
        symbols_len: usize,
    ) -> c_int;
    /// Replaces Channel-axis labels from borrowed strings.
    pub fn arcadia_tio_set_channels(
        handle: *mut ArcadiaTioHandle,
        channels: *const *const c_char,
        channels_len: usize,
    ) -> c_int;
    /// Replaces user metadata key/value pairs from borrowed strings.
    pub fn arcadia_tio_set_user_kv(
        handle: *mut ArcadiaTioHandle,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> c_int;
    /// Reads current dimension lengths.
    pub fn arcadia_tio_dim_lens(
        handle: *mut ArcadiaTioHandle,
        out_dim_lens: *mut u32,
        out_dim_lens_len: usize,
    ) -> c_int;
    /// Reads the native chunk plan into a native-owned plan carrier.
    pub fn arcadia_tio_chunk_plan(
        handle: *mut ArcadiaTioHandle,
        out_plan: *mut ArcadiaTioChunkPlan,
    ) -> c_int;
    /// Reads current file path into a native-owned string.
    pub fn arcadia_tio_path(handle: *mut ArcadiaTioHandle, out_path: *mut *mut c_char) -> c_int;
    /// Frees native-owned strings returned by string APIs.
    pub fn arcadia_tio_string_free(value: *mut c_char);
    /// Frees native-owned chunk plan arrays.
    pub fn arcadia_tio_chunk_plan_free(plan: *mut ArcadiaTioChunkPlan);
    /// Frees native-owned file metadata.
    pub fn arcadia_tio_file_meta_free(meta: *mut ArcadiaTioFileMeta);

    /// Reads an axis range into an owned tensor.
    pub fn arcadia_tio_read_axis_range(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: u32,
        end: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads an axis take selection into an owned tensor.
    pub fn arcadia_tio_read_axis_take(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        indices: *const u32,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads one axis index into an owned tensor.
    pub fn arcadia_tio_read_axis_one(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        index: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads an append-entry range into an owned tensor.
    pub fn arcadia_tio_read_entry_range(
        handle: *mut ArcadiaTioHandle,
        start: u32,
        end: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Takes append entries into an owned tensor.
    pub fn arcadia_tio_take_entries(
        handle: *mut ArcadiaTioHandle,
        indices: *const u32,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads one scalar value.
    pub fn arcadia_tio_read_scalar(
        handle: *mut ArcadiaTioHandle,
        indices: *const u32,
        indices_len: usize,
        out_value: *mut ArcadiaTioScalar,
    ) -> c_int;
    /// Reads selector data at a commit into an owned tensor.
    pub fn arcadia_tio_read_at_commit(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads selector data at a commit into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
    ) -> c_int;
    /// Frees native-owned strings in a current read execution report.
    pub fn arcadia_tio_read_execution_report_free(report: *mut ArcadiaTioReadExecutionReport);
    /// Frees native-owned JSON strings in an attributed query trace.
    pub fn arcadia_tio_query_trace_json_free(trace_json: *mut ArcadiaTioQueryTraceJson);
    /// Frees native-owned strings in a historical read execution report.
    pub fn arcadia_tio_historical_read_execution_report_free(
        report: *mut ArcadiaTioHistoricalReadExecutionReport,
    );
    /// Frees native-owned strings in a read-index report.
    pub fn arcadia_tio_read_index_report_free(report: *mut ArcadiaTioReadIndexReport);
    /// Reads data through low-level read-index items into an owned tensor.
    pub fn arcadia_tio_read_index(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads current selector data with execution options into an owned tensor.
    pub fn arcadia_tio_read_with_options(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with execution options into a dense tensor and optional mask.
    pub fn arcadia_tio_read_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with execution options and query attribution.
    pub fn arcadia_tio_read_with_options_attributed(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        trace_context: *const ArcadiaTioQueryTraceContext,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
        out_trace_json: *mut ArcadiaTioQueryTraceJson,
    ) -> c_int;
    /// Reads current dense selector data with execution options and query attribution.
    pub fn arcadia_tio_read_with_options_dense_attributed(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        trace_context: *const ArcadiaTioQueryTraceContext,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
        out_trace_json: *mut ArcadiaTioQueryTraceJson,
    ) -> c_int;
    /// Reads historical selector data with execution options into an owned tensor.
    pub fn arcadia_tio_read_at_commit_with_options(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with execution options into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with a shape policy into an owned tensor.
    pub fn arcadia_tio_read_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with a shape policy into a dense tensor and optional mask.
    pub fn arcadia_tio_read_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with a shape policy into an owned tensor.
    pub fn arcadia_tio_read_at_commit_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with a shape policy into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;

    /// Pops the current head commit.
    pub fn arcadia_tio_pop(handle: *mut ArcadiaTioHandle) -> c_int;
    /// Pops up to `n` current head commits.
    pub fn arcadia_tio_pop_batched(handle: *mut ArcadiaTioHandle, n: u32) -> c_int;
    /// Reverts the file to a target visible commit.
    pub fn arcadia_tio_revert_commit(
        handle: *mut ArcadiaTioHandle,
        target_commit_seq: u64,
    ) -> c_int;
    /// Reads current head commit metadata.
    pub fn arcadia_tio_head_commit(
        handle: *mut ArcadiaTioHandle,
        out_commit: *mut ArcadiaTioCommitInfo,
    ) -> c_int;
    /// Lists visible commit metadata into a native-owned commit list.
    pub fn arcadia_tio_list_commits(
        handle: *mut ArcadiaTioHandle,
        limit: u32,
        out_commits: *mut ArcadiaTioCommitList,
    ) -> c_int;
    /// Frees native-owned commit-list arrays.
    pub fn arcadia_tio_commit_list_free(commits: *mut ArcadiaTioCommitList);
}
