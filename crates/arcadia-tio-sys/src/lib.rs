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

/// Scalar return value for scalar reads.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioScalar {
    /// Scalar dtype.
    pub dtype: ArcadiaTioDType,
    /// Scalar value represented as a C double by the current C ABI.
    pub value: c_double,
}

/// Entry selector borrowed by selector read APIs.
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

    /// Reads the full tensor into a native-owned raw tensor.
    pub fn arcadia_tio_read_all(
        handle: *mut ArcadiaTioHandle,
        out_tensor: *mut ArcadiaTioTensor,
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
    /// Reads current dimension lengths.
    pub fn arcadia_tio_dim_lens(
        handle: *mut ArcadiaTioHandle,
        out_dim_lens: *mut u32,
        out_dim_lens_len: usize,
    ) -> c_int;
    /// Reads current file path into a native-owned string.
    pub fn arcadia_tio_path(handle: *mut ArcadiaTioHandle, out_path: *mut *mut c_char) -> c_int;
    /// Frees native-owned strings returned by string APIs.
    pub fn arcadia_tio_string_free(value: *mut c_char);
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
    /// Frees native-owned strings in a historical read execution report.
    pub fn arcadia_tio_historical_read_execution_report_free(
        report: *mut ArcadiaTioHistoricalReadExecutionReport,
    );
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
}
