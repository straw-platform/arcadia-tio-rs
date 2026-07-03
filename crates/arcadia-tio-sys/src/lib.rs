#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

use core::ffi::{c_char, c_double, c_float, c_int, c_void};

/// Current C ABI version expected by this sys crate.
pub const ARCADIA_TIO_ABI_VERSION: u32 = 1;
/// Current OCB C ABI version expected by this sys crate.
#[cfg(feature = "format-ocb")]
pub const ARCADIA_TIO_OCB_ABI_VERSION: u32 = 1;

/// V4 precise reason-code taxonomy string exposed by the C ABI.
pub const ARCADIA_TIO_V4_PRECISE_REASON_CODE_TAXONOMY: &str = "v4.precise.v1";
/// Query parallel reason-code taxonomy string exposed by the C ABI.
pub const ARCADIA_TIO_QUERY_PARALLEL_REASON_CODE_TAXONOMY: &str = "v4.query_parallel.v1";

/// Opaque TensorFile handle owned by the native library.
#[repr(C)]
pub struct ArcadiaTioHandle {
    _private: [u8; 0],
}

/// Opaque OCB file handle owned by the native library.
#[cfg(feature = "format-ocb")]
#[repr(C)]
pub struct ArcadiaTioOcbFile {
    _private: [u8; 0],
}

/// Opaque OCB read plan owned by the native library.
#[cfg(feature = "format-ocb")]
#[repr(C)]
pub struct ArcadiaTioOcbReadPlan {
    _private: [u8; 0],
}

/// Thread-local C ABI error code value.
pub type ArcadiaTioErrorCode = c_int;
/// OCB structured error-kind value.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbErrorKind = c_int;
/// OCB structured failure-cause value.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbFailureCause = c_int;
/// OCB open validation selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbOpenValidation = c_int;
/// OCB column physical type value.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbPhysicalType = c_int;
/// OCB column logical-kind value.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbLogicalKind = c_int;
/// OCB dictionary value-kind selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbDictionaryValueKind = c_int;
/// OCB ordering direction selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbOrderingDirection = c_int;
/// OCB null-order selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbNullOrder = c_int;
/// OCB projection-kind selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbProjectionKind = c_int;
/// OCB body kind selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbBodyKind = c_int;
/// OCB checksum kind selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbChecksumKind = c_int;
/// OCB column-chunk summary codec selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbColumnChunkSummaryCodec = c_int;
/// OCB write chunk codec selector.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbWriteChunkCodec = c_int;
/// OCB batch visitor callback.
#[cfg(feature = "format-ocb")]
pub type ArcadiaTioOcbBatchVisitor = Option<
    unsafe extern "C" fn(
        user: *mut c_void,
        batch: *const ArcadiaTioOcbColumnBatch,
        out_continue: *mut u8,
    ) -> ArcadiaTioErrorCode,
>;
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
/// Coordinate v2 value-domain selector.
pub type ArcadiaTioCoordinateValueDomainV2 = c_int;
/// Coordinate v2 lookup-key domain selector.
pub type ArcadiaTioCoordinateKeyDomainV2 = c_int;
/// Coordinate v2 dictionary-code integer dtype selector.
pub type ArcadiaTioCoordinateCodeDTypeV2 = c_int;
/// Coordinate v2 fixed-text encoding selector.
pub type ArcadiaTioCoordinateFixedTextEncodingV2 = c_int;
/// Coordinate v2 fixed-text padding selector.
pub type ArcadiaTioCoordinateFixedTextPaddingV2 = c_int;
/// Coordinate v2 external-source kind selector.
pub type ArcadiaTioCoordinateSourceKindV2 = c_int;
/// Coordinate v2 availability status selector.
pub type ArcadiaTioCoordinateAvailabilityV2 = c_int;
/// Coordinate v2 status-category selector.
pub type ArcadiaTioCoordinateStatusCategoryV2 = c_int;
/// Coordinate v2 optional-index kind selector.
pub type ArcadiaTioCoordinateIndexKindV2 = c_int;
/// Coordinate v2 optional-index validation status selector.
pub type ArcadiaTioCoordinateIndexValidationStatusV2 = c_int;
/// Coordinate v2 optional-index fallback selector.
pub type ArcadiaTioCoordinateIndexFallbackV2 = c_int;
/// Coordinate v2 optional-index selected-use selector.
pub type ArcadiaTioCoordinateIndexUseV2 = c_int;
/// Coordinate v2 lookup-result status selector.
pub type ArcadiaTioCoordinateLookupResultStatusV2 = c_int;
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
/// Sparse-intent V2 value predicate selector value.
pub type ArcadiaTioSparseValuePredicateKindV2 = c_int;
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

#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_NONE: ArcadiaTioOcbErrorKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_INVALID_INPUT: ArcadiaTioOcbErrorKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_UNSUPPORTED_FORMAT: ArcadiaTioOcbErrorKind = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_CORRUPT_FILE: ArcadiaTioOcbErrorKind = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_LOCK_UNAVAILABLE: ArcadiaTioOcbErrorKind = 4);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ERROR_KIND_IO: ArcadiaTioOcbErrorKind = 5);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_FAILURE_CAUSE_NONE: ArcadiaTioOcbFailureCause = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_FAILURE_CAUSE_INVALID_INPUT: ArcadiaTioOcbFailureCause = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_FAILURE_CAUSE_UNSUPPORTED_FORMAT: ArcadiaTioOcbFailureCause = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_FAILURE_CAUSE_CORRUPT_FILE: ArcadiaTioOcbFailureCause = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_FAILURE_CAUSE_LOCK_UNAVAILABLE: ArcadiaTioOcbFailureCause = 4);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH: ArcadiaTioOcbOpenValidation = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_OPEN_VALIDATION_FULL_PAYLOAD: ArcadiaTioOcbOpenValidation = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32: ArcadiaTioOcbPhysicalType = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64: ArcadiaTioOcbPhysicalType = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32: ArcadiaTioOcbPhysicalType = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64: ArcadiaTioOcbPhysicalType = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY: ArcadiaTioOcbPhysicalType = 4);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN: ArcadiaTioOcbLogicalKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_TIMESTAMP_NANOS_LIKE: ArcadiaTioOcbLogicalKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_SCALED_INTEGER: ArcadiaTioOcbLogicalKind = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_DICTIONARY_CODE: ArcadiaTioOcbLogicalKind = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_ENUM_CODE: ArcadiaTioOcbLogicalKind = 4);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_LOGICAL_KIND_OPAQUE_KEY: ArcadiaTioOcbLogicalKind = 5);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8: ArcadiaTioOcbDictionaryValueKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_BYTES: ArcadiaTioOcbDictionaryValueKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_FIXED_BYTES: ArcadiaTioOcbDictionaryValueKind = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_ENUM_LABELS: ArcadiaTioOcbDictionaryValueKind = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ORDERING_DIRECTION_ASCENDING: ArcadiaTioOcbOrderingDirection = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_ORDERING_DIRECTION_DESCENDING: ArcadiaTioOcbOrderingDirection = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_NULL_ORDER_NULLS_FIRST: ArcadiaTioOcbNullOrder = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_NULL_ORDER_NULLS_LAST: ArcadiaTioOcbNullOrder = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_NULL_ORDER_NO_NULLS: ArcadiaTioOcbNullOrder = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PROJECTION_ALL: ArcadiaTioOcbProjectionKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_PROJECTION_NAMES: ArcadiaTioOcbProjectionKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_UNKNOWN: ArcadiaTioOcbBodyKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_ROOT: ArcadiaTioOcbBodyKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_SCHEMA: ArcadiaTioOcbBodyKind = 2);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_INDEX: ArcadiaTioOcbBodyKind = 3);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_VALUES: ArcadiaTioOcbBodyKind = 4);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX: ArcadiaTioOcbBodyKind = 5);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_ORDERING_PROOF: ArcadiaTioOcbBodyKind = 6);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_COLUMN_CHUNK: ArcadiaTioOcbBodyKind = 7);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_STRING_TABLE: ArcadiaTioOcbBodyKind = 8);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_DEBUG_JSON_METADATA: ArcadiaTioOcbBodyKind = 9);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_VALIDITY_BITMAP: ArcadiaTioOcbBodyKind = 10);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_KEY_TUPLE: ArcadiaTioOcbBodyKind = 11);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX_DELTA: ArcadiaTioOcbBodyKind = 12);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_CHECKSUM_KIND_NONE: ArcadiaTioOcbChecksumKind = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_CHECKSUM_KIND_CRC32C: ArcadiaTioOcbChecksumKind = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_NONE: ArcadiaTioOcbColumnChunkSummaryCodec = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_ZSTD: ArcadiaTioOcbColumnChunkSummaryCodec = 1);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_NONE: ArcadiaTioOcbWriteChunkCodec = 0);
#[cfg(feature = "format-ocb")]
raw_constant!(ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_ZSTD: ArcadiaTioOcbWriteChunkCodec = 1);

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

raw_constant!(ARCADIA_TIO_COORDINATE_V2_ABI_VERSION: u32 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_INLINE_NUMERIC: ArcadiaTioCoordinateValueDomainV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_FIXED_TEXT: ArcadiaTioCoordinateValueDomainV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_DICTIONARY_CODE: ArcadiaTioCoordinateValueDomainV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_APPEND_SEQUENCE: ArcadiaTioCoordinateValueDomainV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_EXTERNAL_REFERENCE: ArcadiaTioCoordinateValueDomainV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I32: ArcadiaTioCoordinateKeyDomainV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I64: ArcadiaTioCoordinateKeyDomainV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_FIXED_TEXT: ArcadiaTioCoordinateKeyDomainV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DICTIONARY_CODE: ArcadiaTioCoordinateKeyDomainV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_STABLE_ID: ArcadiaTioCoordinateKeyDomainV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DISPLAY_LABEL: ArcadiaTioCoordinateKeyDomainV2 = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_ALIAS: ArcadiaTioCoordinateKeyDomainV2 = 6);
raw_constant!(ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_RAW_TIME: ArcadiaTioCoordinateKeyDomainV2 = 7);
raw_constant!(ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U8: ArcadiaTioCoordinateCodeDTypeV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U16: ArcadiaTioCoordinateCodeDTypeV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U32: ArcadiaTioCoordinateCodeDTypeV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U64: ArcadiaTioCoordinateCodeDTypeV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_FIXED_TEXT_ENCODING_V2_ASCII: ArcadiaTioCoordinateFixedTextEncodingV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_FIXED_TEXT_PADDING_V2_RIGHT_SPACE: ArcadiaTioCoordinateFixedTextPaddingV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_V2_SAME_FILE_OBJECT: ArcadiaTioCoordinateSourceKindV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_V2_RELATIVE_PATH: ArcadiaTioCoordinateSourceKindV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_V2_ABSOLUTE_PATH: ArcadiaTioCoordinateSourceKindV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_V2_URI: ArcadiaTioCoordinateSourceKindV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_SOURCE_V2_APPLICATION_REGISTRY: ArcadiaTioCoordinateSourceKindV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_AVAILABLE: ArcadiaTioCoordinateAvailabilityV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_ABSENT: ArcadiaTioCoordinateAvailabilityV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNKNOWN: ArcadiaTioCoordinateAvailabilityV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_INVALID: ArcadiaTioCoordinateAvailabilityV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNAVAILABLE: ArcadiaTioCoordinateAvailabilityV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNSUPPORTED: ArcadiaTioCoordinateAvailabilityV2 = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_OK: ArcadiaTioCoordinateStatusCategoryV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_ARGUMENT: ArcadiaTioCoordinateStatusCategoryV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_DOMAIN: ArcadiaTioCoordinateStatusCategoryV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_UNKNOWN_REQUIRED_VERSION: ArcadiaTioCoordinateStatusCategoryV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_REQUIRED_UNAVAILABLE: ArcadiaTioCoordinateStatusCategoryV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_EXTERNAL_BINDING: ArcadiaTioCoordinateStatusCategoryV2 = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_DUPLICATE_UNIQUE_LOOKUP: ArcadiaTioCoordinateStatusCategoryV2 = 6);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_LOOKUP_DOMAIN_MISMATCH: ArcadiaTioCoordinateStatusCategoryV2 = 7);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_INDEX: ArcadiaTioCoordinateStatusCategoryV2 = 8);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_INDEX: ArcadiaTioCoordinateStatusCategoryV2 = 9);
raw_constant!(ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_INDEX: ArcadiaTioCoordinateStatusCategoryV2 = 10);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_EXACT: ArcadiaTioCoordinateIndexKindV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_RANGE: ArcadiaTioCoordinateIndexKindV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_DICTIONARY_KEY: ArcadiaTioCoordinateIndexKindV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_VALIDATED: ArcadiaTioCoordinateIndexValidationStatusV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_MISSING: ArcadiaTioCoordinateIndexValidationStatusV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_STALE: ArcadiaTioCoordinateIndexValidationStatusV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_INVALID: ArcadiaTioCoordinateIndexValidationStatusV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_UNSUPPORTED: ArcadiaTioCoordinateIndexValidationStatusV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_AUTHORITATIVE_SCAN: ArcadiaTioCoordinateIndexFallbackV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_REBUILD: ArcadiaTioCoordinateIndexFallbackV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_REJECT_INDEX_DEPENDENT_OPERATION: ArcadiaTioCoordinateIndexFallbackV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_USE_V2_USE_INDEX: ArcadiaTioCoordinateIndexUseV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_USE_V2_AUTHORITATIVE_SCAN: ArcadiaTioCoordinateIndexUseV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_USE_V2_REBUILD: ArcadiaTioCoordinateIndexUseV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_INDEX_USE_V2_UNAVAILABLE: ArcadiaTioCoordinateIndexUseV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNIQUE: ArcadiaTioCoordinateLookupResultStatusV2 = 0);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_RANGE: ArcadiaTioCoordinateLookupResultStatusV2 = 1);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_MANY: ArcadiaTioCoordinateLookupResultStatusV2 = 2);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_MISSING: ArcadiaTioCoordinateLookupResultStatusV2 = 3);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNAVAILABLE: ArcadiaTioCoordinateLookupResultStatusV2 = 4);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_DUPLICATE: ArcadiaTioCoordinateLookupResultStatusV2 = 5);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNSUPPORTED: ArcadiaTioCoordinateLookupResultStatusV2 = 6);
raw_constant!(ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_ERROR: ArcadiaTioCoordinateLookupResultStatusV2 = 7);

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
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_NAN: ArcadiaTioSparseValuePredicateKindV2 = 0);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_ZERO: ArcadiaTioSparseValuePredicateKindV2 = 1);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F32: ArcadiaTioSparseValuePredicateKindV2 = 2);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F64: ArcadiaTioSparseValuePredicateKindV2 = 3);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I32: ArcadiaTioSparseValuePredicateKindV2 = 4);
raw_constant!(ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I64: ArcadiaTioSparseValuePredicateKindV2 = 5);
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

/// OCB column descriptor returned in metadata.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbColumnDescriptor {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local column id.
    pub id: u32,
    /// Native-owned UTF-8 column name.
    pub name: *mut c_char,
    /// Physical primitive type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// Logical column kind.
    pub logical_kind: ArcadiaTioOcbLogicalKind,
    /// Nonzero when dictionary_id is meaningful.
    pub has_dictionary_id: u8,
    /// Dictionary id for dictionary-coded columns.
    pub dictionary_id: u32,
    /// Decimal scale for scaled-integer logical columns.
    pub scale: i32,
    /// Nonzero when values may be null.
    pub nullable: u8,
    /// Reserved words. For fixed-binary columns, reserved[0] carries the byte
    /// width; prefer [`arcadia_tio_ocb_column_descriptor_fixed_binary_width`].
    pub reserved: [u64; 3],
}

/// OCB dictionary descriptor returned in metadata.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbDictionaryDescriptor {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Native-owned UTF-8 dictionary name.
    pub name: *mut c_char,
    /// Physical code type used by dictionary-coded columns.
    pub code_physical_type: ArcadiaTioOcbPhysicalType,
    /// Decoded value kind.
    pub value_kind: ArcadiaTioOcbDictionaryValueKind,
    /// Number of entries in the dictionary.
    pub entry_count: u32,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB open options.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbOpenOptions {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Open validation depth.
    pub validation: ArcadiaTioOcbOpenValidation,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB ordering-key descriptor returned in metadata.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbOrderingKey {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local column id.
    pub column_id: u32,
    /// Native-owned UTF-8 column name snapshot.
    pub column_name: *mut c_char,
    /// Sort direction.
    pub direction: ArcadiaTioOcbOrderingDirection,
    /// Null-order declaration.
    pub null_order: ArcadiaTioOcbNullOrder,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// Owned OCB metadata result; free with [`arcadia_tio_ocb_metadata_free`].
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbMetadata {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Native-owned format name, currently `OCB`.
    pub format_name: *mut c_char,
    /// Nonzero when the selected file is appendable.
    pub appendable: u8,
    /// Selected root generation.
    pub root_generation: u64,
    /// Nonzero when previous_root_generation is meaningful.
    pub has_previous_root_generation: u8,
    /// Previous root generation when available.
    pub previous_root_generation: u64,
    /// Rows visible in the selected snapshot.
    pub row_count: u64,
    /// Row groups visible in the selected snapshot.
    pub row_group_count: u32,
    /// Column chunks visible in the selected snapshot.
    pub column_chunk_count: u32,
    /// Native-owned column descriptor array.
    pub columns: *mut ArcadiaTioOcbColumnDescriptor,
    /// Number of column descriptors.
    pub columns_len: usize,
    /// Native-owned dictionary descriptor array.
    pub dictionaries: *mut ArcadiaTioOcbDictionaryDescriptor,
    /// Number of dictionary descriptors.
    pub dictionaries_len: usize,
    /// Native-owned ordering-key descriptor array.
    pub ordering_keys: *mut ArcadiaTioOcbOrderingKey,
    /// Number of ordering keys.
    pub ordering_keys_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB byte slice in owned dictionary/read results.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbByteSlice {
    /// Borrowed byte pointer tied to the owning result object.
    pub data: *const u8,
    /// Number of bytes.
    pub len: usize,
}

/// Owned OCB dictionary values result; free with [`arcadia_tio_ocb_dictionary_values_free`].
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbDictionaryValues {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Native-owned UTF-8 dictionary name.
    pub name: *mut c_char,
    /// Decoded value kind.
    pub value_kind: ArcadiaTioOcbDictionaryValueKind,
    /// Fixed byte width when value_kind is fixed bytes.
    pub fixed_width: u32,
    /// Native-owned UTF-8 string array for string-like dictionaries.
    pub string_values: *mut *mut c_char,
    /// Number of string values.
    pub string_values_len: usize,
    /// Native-owned byte-slice array for bytes-like dictionaries.
    pub byte_values: *mut ArcadiaTioOcbByteSlice,
    /// Number of byte values.
    pub byte_values_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Borrowed OCB primitive values input or owned primitive values output.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbPrimitiveValues {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Physical primitive type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// Primitive buffer pointer.
    pub data: *const c_void,
    /// Number of primitive values. For fixed-binary values this is row count,
    /// while `data` points to `len * fixed_binary_width` bytes.
    pub len: usize,
    /// Reserved words. For fixed-binary values, reserved[0] carries the byte
    /// width.
    pub reserved: [u64; 3],
}

/// OCB validity bitmap; least-significant-bit first, where 1 means valid.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbValidityBitmap {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Bitmap byte pointer.
    pub data: *const u8,
    /// Number of bitmap bytes.
    pub len: usize,
    /// Number of meaningful bits/rows.
    pub row_count: u64,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write-column schema input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteColumn {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed UTF-8 column name.
    pub name: *const c_char,
    /// Physical primitive type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// Logical column kind.
    pub logical_kind: ArcadiaTioOcbLogicalKind,
    /// Nonzero when dictionary_id is meaningful.
    pub has_dictionary_id: u8,
    /// Dictionary id for dictionary-coded columns.
    pub dictionary_id: u32,
    /// Decimal scale for scaled-integer logical columns.
    pub scale: i32,
    /// Nonzero when values may be null.
    pub nullable: u8,
    /// Reserved words. For fixed-binary columns, set reserved[0] to the byte
    /// width, preferably via
    /// [`arcadia_tio_ocb_write_column_set_fixed_binary_width`].
    pub reserved: [u64; 3],
}

/// OCB write-dictionary entry input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbDictionaryEntry {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed bytes for one decoded dictionary value.
    pub data: *const u8,
    /// Number of bytes.
    pub len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write-dictionary input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteDictionary {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Borrowed UTF-8 dictionary name.
    pub name: *const c_char,
    /// Physical code type.
    pub code_physical_type: ArcadiaTioOcbPhysicalType,
    /// Decoded value kind.
    pub value_kind: ArcadiaTioOcbDictionaryValueKind,
    /// Fixed byte width when value_kind is fixed bytes.
    pub fixed_width: u32,
    /// Borrowed dictionary-entry array.
    pub entries: *const ArcadiaTioOcbDictionaryEntry,
    /// Number of entries.
    pub entries_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write row-group column chunk input.
#[cfg(feature = "format-ocb")]
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteColumnChunk {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local column id.
    pub column_id: u32,
    /// Borrowed primitive values.
    pub values: ArcadiaTioOcbPrimitiveValues,
    /// Optional borrowed validity bitmap; NULL means all rows valid.
    pub validity: *const ArcadiaTioOcbValidityBitmap,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write row-group input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteRowGroup {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed column chunk array.
    pub columns: *const ArcadiaTioOcbWriteColumnChunk,
    /// Number of column chunks.
    pub columns_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write ordering-key input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteOrderingKey {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local column id.
    pub column_id: u32,
    /// Sort direction.
    pub direction: ArcadiaTioOcbOrderingDirection,
    /// Null-order declaration.
    pub null_order: ArcadiaTioOcbNullOrder,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB write spec input for create/append.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbWriteSpec {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed column schema array.
    pub columns: *const ArcadiaTioOcbWriteColumn,
    /// Number of columns.
    pub columns_len: usize,
    /// Borrowed dictionary declarations.
    pub dictionaries: *const ArcadiaTioOcbWriteDictionary,
    /// Number of dictionaries.
    pub dictionaries_len: usize,
    /// Borrowed row-group array.
    pub row_groups: *const ArcadiaTioOcbWriteRowGroup,
    /// Number of row groups.
    pub row_groups_len: usize,
    /// Borrowed ordering-key array.
    pub ordering_keys: *const ArcadiaTioOcbWriteOrderingKey,
    /// Number of ordering keys.
    pub ordering_keys_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB write options.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbWriteOptions {
    pub version: u32,
    pub struct_size: usize,
    pub write_threads: usize,
    pub chunk_codec: ArcadiaTioOcbWriteChunkCodec,
    pub zstd_level: i32,
    pub reserved: [u64; 4],
}

/// OCB cleanup result.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbCleanupResult {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Nonzero when orphan tail bytes were truncated.
    pub truncated: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// Compact-L2 certification options for channel-sharded OCB artifacts.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbCompactL2CertificationOptions {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Expected source record width in bytes.
    pub expected_record_width: u32,
    /// Nonzero to verify payload headers.
    pub verify_payload_header: u8,
    /// Nonzero to verify CRC32C checksums.
    pub verify_crc32c: u8,
    /// Nonzero to verify content hashes.
    pub verify_hashes: u8,
    /// Nonzero when max_rows is meaningful.
    pub has_max_rows: u8,
    /// Maximum rows accepted during certification.
    pub max_rows: u64,
    /// Requested worker thread count.
    pub read_threads: usize,
    /// Maximum row groups allowed in flight.
    pub max_in_flight_row_groups: usize,
    /// Borrowed expected artifact format string or NULL.
    pub artifact_format: *const c_char,
    /// Borrowed payload column name string or NULL.
    pub payload_column_name: *const c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Per-channel compact-L2 certification report.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbCompactL2ChannelCertificationReport {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Channel identifier.
    pub channel_id: u32,
    /// Certified row count.
    pub row_count: u64,
    /// Certified row-group count.
    pub row_group_count: u32,
    /// First business index in the channel.
    pub first_biz_index: u64,
    /// Last business index in the channel.
    pub last_biz_index: u64,
    /// Nonzero when min_receive_nano is meaningful.
    pub has_min_receive_nano: u8,
    /// Minimum receive timestamp in nanoseconds.
    pub min_receive_nano: i64,
    /// Nonzero when max_receive_nano is meaningful.
    pub has_max_receive_nano: u8,
    /// Maximum receive timestamp in nanoseconds.
    pub max_receive_nano: i64,
    /// Nonzero when order_record_count is meaningful.
    pub has_order_record_count: u8,
    /// Count of order records.
    pub order_record_count: u64,
    /// Nonzero when trade_record_count is meaningful.
    pub has_trade_record_count: u8,
    /// Count of trade records.
    pub trade_record_count: u64,
    /// Nonzero when checksums were verified.
    pub checksum_verified: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Compact-L2 certification report.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbCompactL2CertificationReport {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Certified schema version.
    pub schema_version: u32,
    /// Certified trading day.
    pub trading_day: u32,
    /// Native-owned artifact format string or NULL.
    pub artifact_format: *mut c_char,
    /// Number of certified channels.
    pub channel_count: usize,
    /// Total certified row count.
    pub row_count: u64,
    /// Total certified row-group count.
    pub row_group_count: u64,
    /// Number of channels that failed certification.
    pub failed_channel_count: usize,
    /// Nonzero when the artifact is certified.
    pub certified: u8,
    /// Nonzero when paths were redacted.
    pub path_redacted: u8,
    /// Native-owned channel reports.
    pub channels: *mut ArcadiaTioOcbCompactL2ChannelCertificationReport,
    /// Number of channel reports.
    pub channels_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB predicate bound value.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbPredicateValue {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Physical primitive type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// i32 predicate value.
    pub i32_value: i32,
    /// i64 predicate value.
    pub i64_value: i64,
    /// f32 predicate value.
    pub f32_value: c_float,
    /// f64 predicate value.
    pub f64_value: c_double,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB row-group predicate input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbRowGroupPredicate {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Borrowed UTF-8 column name.
    pub column: *const c_char,
    /// Nonzero when lower is meaningful.
    pub has_lower: u8,
    /// Inclusive lower bound.
    pub lower: ArcadiaTioOcbPredicateValue,
    /// Nonzero when upper is meaningful.
    pub has_upper: u8,
    /// Inclusive upper bound.
    pub upper: ArcadiaTioOcbPredicateValue,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 3],
}

/// OCB read request input.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadRequest {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Projection kind.
    pub projection_kind: ArcadiaTioOcbProjectionKind,
    /// Borrowed UTF-8 column name array for name projections.
    pub column_names: *const *const c_char,
    /// Number of projected column names.
    pub column_names_len: usize,
    /// Borrowed predicate array.
    pub predicates: *const ArcadiaTioOcbRowGroupPredicate,
    /// Number of predicates.
    pub predicates_len: usize,
    /// Requested worker thread count.
    pub max_threads: usize,
    /// Nonzero to validate checksums.
    pub validate_checksums: u8,
    /// Reserved flag for dictionary decode behavior.
    pub decode_dictionaries: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB read execution report.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadReport {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Requested worker thread count.
    pub requested_threads: usize,
    /// Effective worker thread count.
    pub effective_threads: usize,
    /// Selected row groups.
    pub selected_row_groups: usize,
    /// Pruned row groups.
    pub pruned_row_groups: usize,
    /// Selected column chunks.
    pub selected_column_chunks: usize,
    /// Native-owned fallback reason string or NULL.
    pub fallback_reason: *mut c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB read attribution diagnostics.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadAttribution {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Planning duration in nanoseconds.
    pub plan_ns: u64,
    /// Read execution wall duration in nanoseconds.
    pub execute_wall_ns: u64,
    /// Cumulative row-group read duration in nanoseconds.
    pub row_group_read_ns: u64,
    /// Cumulative file read duration in nanoseconds.
    pub read_io_ns: u64,
    /// Cumulative checksum duration in nanoseconds.
    pub checksum_ns: u64,
    /// Cumulative decompression duration in nanoseconds.
    pub decompression_ns: u64,
    /// Cumulative primitive decode duration in nanoseconds.
    pub primitive_decode_ns: u64,
    /// Nonzero when native_to_c_copy_ns is meaningful.
    pub has_native_to_c_copy_ns: u8,
    /// Native-to-C outcome conversion duration in nanoseconds.
    pub native_to_c_copy_ns: u64,
    /// Nonzero when wrapper_copy_ns is meaningful.
    pub has_wrapper_copy_ns: u8,
    /// Safe-wrapper copy duration in nanoseconds.
    pub wrapper_copy_ns: u64,
    /// Selected object bytes read.
    pub bytes_read: u64,
    /// Selected compressed column payload bytes.
    pub compressed_bytes: u64,
    /// Selected uncompressed column payload bytes.
    pub uncompressed_bytes: u64,
    /// Requested worker thread count.
    pub requested_threads: usize,
    /// Effective worker thread count.
    pub effective_threads: usize,
    /// Selected row groups.
    pub selected_row_groups: usize,
    /// Pruned row groups.
    pub pruned_row_groups: usize,
    /// Selected column chunks.
    pub selected_column_chunks: usize,
    /// Native-owned fallback reason string or NULL.
    pub fallback_reason: *mut c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB read cursor/visitor options.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbBodyRefSummary {
    pub version: u32,
    pub struct_size: usize,
    pub offset: u64,
    pub length: u64,
    pub kind: ArcadiaTioOcbBodyKind,
    pub flags: u16,
    pub checksum_kind: ArcadiaTioOcbChecksumKind,
    pub checksum: u32,
    pub reserved: [u64; 4],
}

#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbColumnChunkSummary {
    pub version: u32,
    pub struct_size: usize,
    pub row_group_id: u32,
    pub column_id: u32,
    pub column_name: *mut c_char,
    pub physical_type: ArcadiaTioOcbPhysicalType,
    pub logical_kind: ArcadiaTioOcbLogicalKind,
    pub fixed_binary_width: u32,
    pub codec: ArcadiaTioOcbColumnChunkSummaryCodec,
    pub row_count: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub value_ref: ArcadiaTioOcbBodyRefSummary,
    pub has_validity_ref: u8,
    pub validity_ref: ArcadiaTioOcbBodyRefSummary,
    pub reserved: [u64; 4],
}

#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbColumnStatsSummary {
    pub version: u32,
    pub struct_size: usize,
    pub row_group_id: u32,
    pub column_id: u32,
    pub column_name: *mut c_char,
    pub physical_type: ArcadiaTioOcbPhysicalType,
    pub null_count: u32,
    pub min: ArcadiaTioOcbPredicateValue,
    pub max: ArcadiaTioOcbPredicateValue,
    pub reserved: [u64; 4],
}

#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbRowGroupSummary {
    pub version: u32,
    pub struct_size: usize,
    pub row_group_id: u32,
    pub base_row: u64,
    pub row_count: u64,
    pub has_first_key_tuple_ref: u8,
    pub first_key_tuple_ref: ArcadiaTioOcbBodyRefSummary,
    pub has_last_key_tuple_ref: u8,
    pub last_key_tuple_ref: ArcadiaTioOcbBodyRefSummary,
    pub chunks: *mut ArcadiaTioOcbColumnChunkSummary,
    pub chunks_len: usize,
    pub stats: *mut ArcadiaTioOcbColumnStatsSummary,
    pub stats_len: usize,
    pub reserved: [u64; 4],
}

#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbRowGroupSummaries {
    pub version: u32,
    pub struct_size: usize,
    pub row_groups: *mut ArcadiaTioOcbRowGroupSummary,
    pub row_groups_len: usize,
    pub reserved: [u64; 4],
}

#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
#[allow(missing_docs)]
pub struct ArcadiaTioOcbReadCursorOptions {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Maximum decoded row-group batches in flight.
    pub max_in_flight_row_groups: usize,
    /// Nonzero to preserve deterministic row-group order.
    pub ordered: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 8],
}

/// OCB read cursor/visitor report.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadCursorReport {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Base read planning/execution report.
    pub base_report: ArcadiaTioOcbReadReport,
    /// Batches yielded to the visitor.
    pub batches_yielded: usize,
    /// Rows yielded to the visitor.
    pub rows_yielded: u64,
    /// Nonzero when visitor stopped early.
    pub cancelled: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// OCB caller-owned column fill buffer.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbColumnFillBuffer {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Optional borrowed UTF-8 column name selector.
    pub column_name: *const c_char,
    /// File-local column id selector or success output.
    pub column_id: u32,
    /// Nonzero when column_id is an input selector; set on success output.
    pub has_column_id: u8,
    /// Caller-owned value physical type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// Caller-owned typed value storage.
    pub values: *mut c_void,
    /// Value element capacity. For fixed-binary fill buffers this is byte
    /// capacity (`rows * fixed_binary_width`), not row count.
    pub values_len: usize,
    /// Optional caller-owned validity bitmap storage.
    pub validity_bytes: *mut u8,
    /// Validity byte capacity.
    pub validity_bytes_len: usize,
    /// Nonzero if nullable chunks are accepted.
    pub allow_nulls: u8,
    /// Rows filled on success.
    pub rows_filled: usize,
    /// Nonzero if validity bytes were filled on success.
    pub validity_filled: u8,
    /// Reserved words. For fixed-binary fill buffers, reserved[0] carries the
    /// byte width, preferably via
    /// [`arcadia_tio_ocb_column_fill_buffer_set_fixed_binary_width`].
    pub reserved: [u64; 8],
}

/// OCB single-row-group caller-owned fill request.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbRowGroupFillRequest {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local row group id.
    pub row_group_id: u32,
    /// Caller-owned column buffers.
    pub columns: *mut ArcadiaTioOcbColumnFillBuffer,
    /// Number of column buffers.
    pub columns_len: usize,
    /// Nonzero to validate checksums.
    pub validate_checksums: u8,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 8],
}

/// OCB caller-owned fill report.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadFillReport {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local row group id.
    pub row_group_id: u32,
    /// Base row offset.
    pub base_row: u64,
    /// Rows in the row group.
    pub row_count: u64,
    /// Number of column buffers filled.
    pub columns_filled: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 8],
}

/// OCB read-result column array.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbColumnArray {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local column id.
    pub column_id: u32,
    /// Native-owned UTF-8 column name.
    pub name: *mut c_char,
    /// Physical primitive type.
    pub physical_type: ArcadiaTioOcbPhysicalType,
    /// Logical column kind.
    pub logical_kind: ArcadiaTioOcbLogicalKind,
    /// Nonzero when dictionary_id is meaningful.
    pub has_dictionary_id: u8,
    /// Dictionary id for dictionary-coded columns.
    pub dictionary_id: u32,
    /// Owned primitive values tied to the read outcome.
    pub values: ArcadiaTioOcbPrimitiveValues,
    /// Nonzero when validity is meaningful.
    pub has_validity: u8,
    /// Owned validity bitmap tied to the read outcome.
    pub validity: ArcadiaTioOcbValidityBitmap,
    /// Reserved words. For fixed-binary columns, reserved[0] carries the byte
    /// width; prefer [`arcadia_tio_ocb_column_array_fixed_binary_width`].
    pub reserved: [u64; 4],
}

/// OCB read-result row group batch.
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbColumnBatch {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// File-local row group id.
    pub row_group_id: u32,
    /// Base row offset.
    pub base_row: u64,
    /// Number of rows.
    pub row_count: u64,
    /// Native-owned column array.
    pub columns: *mut ArcadiaTioOcbColumnArray,
    /// Number of columns.
    pub columns_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Owned OCB read outcome; free with [`arcadia_tio_ocb_read_outcome_free`].
#[cfg(feature = "format-ocb")]
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioOcbReadOutcome {
    /// Struct version; set to [`ARCADIA_TIO_OCB_ABI_VERSION`].
    pub version: u32,
    /// Size of this struct in bytes.
    pub struct_size: usize,
    /// Native-owned batch array.
    pub batches: *mut ArcadiaTioOcbColumnBatch,
    /// Number of batches.
    pub batches_len: usize,
    /// Read execution report.
    pub report: ArcadiaTioOcbReadReport,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Write-time compression configuration.
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

/// V2 sparse-intent value predicate passed inside a sparse rule.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSparseValuePredicateV2 {
    /// Predicate kind.
    pub kind: ArcadiaTioSparseValuePredicateKindV2,
    /// Comparison value for floating equal predicates; ignored otherwise.
    pub float_value: c_double,
    /// Comparison value for integer equal predicates; ignored otherwise.
    pub integer_value: i64,
}

/// V2 sparse-intent lowering rule borrowed by sparse append APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioSparseRuleV2 {
    /// Size of this struct in bytes.
    pub struct_size: u32,
    /// Detector kind.
    pub detector_kind: ArcadiaTioSparseDetectorKind,
    /// Borrowed sparse-axis indices.
    pub sparse_axes: *const usize,
    /// Number of sparse-axis indices.
    pub sparse_axes_len: usize,
    /// Predicate used by predicate detectors.
    pub predicate: ArcadiaTioSparseValuePredicateV2,
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

/// Historical read-index execution and lowering report.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ArcadiaTioHistoricalReadIndexReport {
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
    /// Lowering strategy selected by native code.
    pub lowering_kind: ArcadiaTioReadIndexLoweringKind,
    /// Nonzero when native code used a full-tensor fallback.
    pub used_full_tensor_fallback: u8,
    /// Reserved padding bytes.
    pub reserved0: [u8; 7],
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

/// Fixed-width text layout for Coordinate v2 descriptors and carriers.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateFixedTextLayoutV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Fixed text width in bytes.
    pub width: usize,
    /// Fixed text byte encoding.
    pub encoding: ArcadiaTioCoordinateFixedTextEncodingV2,
    /// Fixed text padding policy.
    pub padding: ArcadiaTioCoordinateFixedTextPaddingV2,
    /// Nonzero rejects values wider than `width`.
    pub reject_over_width: u8,
    /// Nonzero rejects non-ASCII bytes.
    pub reject_non_ascii: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 6],
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 dictionary identity and cardinality summary.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateDictionarySummaryV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Borrowed or library-owned dictionary identifier.
    pub dictionary_id: *const c_char,
    /// Dictionary revision bound to the selected root.
    pub revision: u64,
    /// Dictionary code integer dtype.
    pub code_dtype: ArcadiaTioCoordinateCodeDTypeV2,
    /// Number of dictionary entries.
    pub entry_count: u64,
    /// Nonzero when stable IDs are unique.
    pub stable_ids_unique: u8,
    /// Nonzero when display labels are unique.
    pub display_labels_unique: u8,
    /// Nonzero when aliases are unique.
    pub aliases_unique: u8,
    /// Nonzero when codes remain stable across revisions.
    pub codes_stable_across_revisions: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 4],
    /// Borrowed or library-owned content identifier.
    pub content_id: *const c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 external binding summary without arbitrary dereference semantics.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateExternalBindingV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// External source kind.
    pub source_kind: ArcadiaTioCoordinateSourceKindV2,
    /// Borrowed or library-owned logical identifier.
    pub logical_id: *const c_char,
    /// Borrowed or library-owned privacy-safe display text.
    pub privacy_safe_display: *const c_char,
    /// Borrowed or library-owned content identifier.
    pub content_id: *const c_char,
    /// External value domain.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Declared external coordinate length.
    pub length: u64,
    /// External binding availability.
    pub availability: ArcadiaTioCoordinateAvailabilityV2,
    /// External binding status category.
    pub status_category: ArcadiaTioCoordinateStatusCategoryV2,
    /// Nonzero when the external coordinate is required.
    pub required: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 7],
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 source binding recorded for optional index summaries.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateIndexSourceBindingV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Library-owned descriptor identifier.
    pub descriptor_id: *const c_char,
    /// Descriptor revision bound to the selected root.
    pub descriptor_revision: u64,
    /// Value domain indexed by the optional index.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Library-owned value-object identifier.
    pub value_object_id: *const c_char,
    /// Library-owned dictionary identifier.
    pub dictionary_id: *const c_char,
    /// Dictionary revision used by the index.
    pub dictionary_revision: u64,
    /// Library-owned dictionary content identifier.
    pub dictionary_content_id: *const c_char,
    /// External source kind used by the index, if any.
    pub external_source_kind: ArcadiaTioCoordinateSourceKindV2,
    /// Library-owned external logical identifier.
    pub external_logical_id: *const c_char,
    /// Library-owned external content identifier.
    pub external_content_id: *const c_char,
    /// Library-owned selected-root identifier.
    pub root_id: *const c_char,
    /// Axis index covered by the index.
    pub axis: usize,
    /// Root extent covered by the index.
    pub root_extent: u64,
    /// First append entry covered by the index.
    pub append_start: u64,
    /// Number of append entries covered by the index.
    pub append_count: u64,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 optional index summary.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateIndexSummaryV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Library-owned index identifier.
    pub index_id: *const c_char,
    /// Optional index kind.
    pub index_kind: ArcadiaTioCoordinateIndexKindV2,
    /// Lookup key domain covered by the index.
    pub key_domain: ArcadiaTioCoordinateKeyDomainV2,
    /// Selected-root source binding for the index.
    pub source_binding: ArcadiaTioCoordinateIndexSourceBindingV2,
    /// Sortedness declaration.
    pub sorted: ArcadiaTioCoordinateSortedness,
    /// Monotonicity declaration.
    pub monotonicity: ArcadiaTioCoordinateMonotonicity,
    /// Uniqueness declaration.
    pub uniqueness: ArcadiaTioCoordinateUniqueness,
    /// Index format version.
    pub format_version: u32,
    /// Index build version.
    pub build_version: u32,
    /// Validation status for the index.
    pub validation_status: ArcadiaTioCoordinateIndexValidationStatusV2,
    /// Fallback policy when the index is not usable.
    pub fallback: ArcadiaTioCoordinateIndexFallbackV2,
    /// Selected use for the current operation.
    pub selected_use: ArcadiaTioCoordinateIndexUseV2,
    /// Nonzero when the index is required.
    pub required: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 7],
    /// Library-owned status reason.
    pub reason: *const c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 dictionary entry with owned strings in returned dictionaries.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateDictionaryEntryV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Dictionary code value.
    pub code: u64,
    /// Native-owned or borrowed stable identifier.
    pub stable_id: *mut c_char,
    /// Native-owned or borrowed display label.
    pub display_label: *mut c_char,
    /// Native-owned or borrowed alias string array.
    pub aliases: *mut *mut c_char,
    /// Number of alias strings.
    pub aliases_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 dictionary result carrier.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateDictionaryV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Dictionary summary.
    pub summary: ArcadiaTioCoordinateDictionarySummaryV2,
    /// Native-owned dictionary entry array.
    pub entries: *mut ArcadiaTioCoordinateDictionaryEntryV2,
    /// Number of dictionary entries.
    pub entries_len: usize,
    /// Dictionary read status category.
    pub status_category: ArcadiaTioCoordinateStatusCategoryV2,
    /// Native-owned status reason.
    pub reason: *mut c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 owned value-slice result carrier.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateValueSliceV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Value domain for the returned carrier.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Numeric dtype for inline numeric values.
    pub numeric_dtype: ArcadiaTioCoordinateDType,
    /// Numeric encoding for inline numeric values.
    pub numeric_encoding: ArcadiaTioCoordinateEncoding,
    /// Dictionary code dtype for dictionary-coded values.
    pub code_dtype: ArcadiaTioCoordinateCodeDTypeV2,
    /// Native-owned value buffer.
    pub data: *mut c_void,
    /// Number of logical values.
    pub len: usize,
    /// Element size in bytes.
    pub element_size: usize,
    /// Fixed text width in bytes.
    pub fixed_text_width: usize,
    /// Availability for the returned values.
    pub availability: ArcadiaTioCoordinateAvailabilityV2,
    /// Status category for the read.
    pub status_category: ArcadiaTioCoordinateStatusCategoryV2,
    /// Native-owned status reason.
    pub reason: *mut c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 typed lookup key.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateLookupKeyV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Lookup key domain.
    pub key_domain: ArcadiaTioCoordinateKeyDomainV2,
    /// Signed 32-bit key value.
    pub i32_value: i32,
    /// Signed 64-bit key value.
    pub i64_value: i64,
    /// Dictionary code key value.
    pub code_value: u64,
    /// Borrowed byte key pointer.
    pub bytes: *const u8,
    /// Number of key bytes.
    pub bytes_len: usize,
    /// Fixed text width in bytes.
    pub fixed_text_width: usize,
    /// Borrowed text key pointer.
    pub text: *const c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 lookup result carrier.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateLookupResultV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Lookup result status.
    pub status: ArcadiaTioCoordinateLookupResultStatusV2,
    /// Lookup status category.
    pub status_category: ArcadiaTioCoordinateStatusCategoryV2,
    /// Unique result position when status is unique.
    pub unique_position: u32,
    /// Half-open result range start when status is range.
    pub range_start: u32,
    /// Half-open result range end when status is range.
    pub range_end: u32,
    /// Native-owned positions array for many-result lookups.
    pub positions: *mut u32,
    /// Number of positions.
    pub positions_len: usize,
    /// Availability for the lookup result.
    pub availability: ArcadiaTioCoordinateAvailabilityV2,
    /// Native-owned status reason.
    pub reason: *mut c_char,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 append-axis coordinate entry.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioAppendCoordinateEntryV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: usize,
    /// Borrowed descriptor identifier.
    pub descriptor_id: *const c_char,
    /// Borrowed coordinate name.
    pub name: *const c_char,
    /// Coordinate value domain.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Numeric dtype for inline numeric values.
    pub numeric_dtype: ArcadiaTioCoordinateDType,
    /// Numeric encoding for inline numeric values.
    pub numeric_encoding: ArcadiaTioCoordinateEncoding,
    /// Dictionary code dtype for dictionary-coded values.
    pub code_dtype: ArcadiaTioCoordinateCodeDTypeV2,
    /// Borrowed append-coordinate value buffer.
    pub values: *const c_void,
    /// Number of coordinate values.
    pub count: usize,
    /// Element size in bytes.
    pub element_size: usize,
    /// Fixed text width in bytes.
    pub fixed_text_width: usize,
    /// Borrowed append-time dictionary-extension entries for dictionary-code append entries.
    pub dictionary_entries: *const ArcadiaTioCoordinateDictionaryEntryV2,
    /// Number of append-time dictionary-extension entries.
    pub dictionary_entries_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 2],
}

/// Coordinate v2 append-axis coordinate batch.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioAppendCoordinateBatchV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Borrowed append-coordinate entries.
    pub entries: *const ArcadiaTioAppendCoordinateEntryV2,
    /// Number of append-coordinate entries.
    pub entries_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Coordinate v2 operation options.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioCoordinateV2Options {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Nonzero allows authoritative scans when indexes are absent or unusable.
    pub allow_authoritative_scan: u8,
    /// Nonzero includes dictionary entries in dictionary reads.
    pub include_dictionary_entries: u8,
    /// Nonzero includes optional index summaries in metadata reads.
    pub include_index_summaries: u8,
    /// Nonzero allows external resolution where supported.
    pub allow_external_resolution: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 4],
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Borrowed Coordinate v2 input descriptor for create APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioAxisCoordinateInputV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: usize,
    /// Borrowed descriptor identifier.
    pub descriptor_id: *const c_char,
    /// Borrowed coordinate name.
    pub name: *const c_char,
    /// Coordinate semantic kind.
    pub kind: ArcadiaTioCoordinateKind,
    /// Coordinate value domain.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Numeric dtype for inline numeric values.
    pub numeric_dtype: ArcadiaTioCoordinateDType,
    /// Numeric encoding for inline numeric values.
    pub numeric_encoding: ArcadiaTioCoordinateEncoding,
    /// Fixed-text layout for fixed-text domains.
    pub fixed_text: ArcadiaTioCoordinateFixedTextLayoutV2,
    /// Dictionary code dtype.
    pub code_dtype: ArcadiaTioCoordinateCodeDTypeV2,
    /// Borrowed value buffer.
    pub values: *const c_void,
    /// Number of values.
    pub values_len: usize,
    /// Borrowed dictionary summary.
    pub dictionary: *const ArcadiaTioCoordinateDictionarySummaryV2,
    /// Borrowed dictionary entries.
    pub dictionary_entries: *const ArcadiaTioCoordinateDictionaryEntryV2,
    /// Number of dictionary entries.
    pub dictionary_entries_len: usize,
    /// Borrowed external binding summary.
    pub external_binding: *const ArcadiaTioCoordinateExternalBindingV2,
    /// Sortedness declaration.
    pub sorted: ArcadiaTioCoordinateSortedness,
    /// Monotonicity declaration.
    pub monotonicity: ArcadiaTioCoordinateMonotonicity,
    /// Uniqueness declaration.
    pub uniqueness: ArcadiaTioCoordinateUniqueness,
    /// Nonzero when coordinate is required.
    pub required: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 7],
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
}

/// Owned Coordinate v2 metadata returned by metadata APIs.
#[repr(C)]
#[derive(Debug, Copy, Clone, Default)]
pub struct ArcadiaTioAxisCoordinateMetaV2 {
    /// Structure version.
    pub version: u32,
    /// Structure size in bytes.
    pub struct_size: usize,
    /// Axis index.
    pub axis: usize,
    /// Native-owned axis name snapshot pointer.
    pub axis_name_snapshot: *mut c_char,
    /// Native-owned descriptor identifier.
    pub descriptor_id: *mut c_char,
    /// Descriptor revision bound to the selected root.
    pub descriptor_revision: u64,
    /// Native-owned coordinate name pointer.
    pub name: *mut c_char,
    /// Coordinate semantic kind.
    pub kind: ArcadiaTioCoordinateKind,
    /// Coordinate value domain.
    pub value_domain: ArcadiaTioCoordinateValueDomainV2,
    /// Numeric dtype for inline numeric values.
    pub numeric_dtype: ArcadiaTioCoordinateDType,
    /// Numeric encoding for inline numeric values.
    pub numeric_encoding: ArcadiaTioCoordinateEncoding,
    /// Fixed-text layout for fixed-text domains.
    pub fixed_text: ArcadiaTioCoordinateFixedTextLayoutV2,
    /// Dictionary code dtype.
    pub code_dtype: ArcadiaTioCoordinateCodeDTypeV2,
    /// Coordinate length.
    pub length: u64,
    /// Sortedness declaration.
    pub sorted: ArcadiaTioCoordinateSortedness,
    /// Monotonicity declaration.
    pub monotonicity: ArcadiaTioCoordinateMonotonicity,
    /// Uniqueness declaration.
    pub uniqueness: ArcadiaTioCoordinateUniqueness,
    /// Nonzero when coordinate is required.
    pub required: u8,
    /// Reserved bytes; callers set to zero.
    pub reserved_u8: [u8; 7],
    /// Coordinate availability.
    pub availability: ArcadiaTioCoordinateAvailabilityV2,
    /// Coordinate status category.
    pub status_category: ArcadiaTioCoordinateStatusCategoryV2,
    /// Native-owned status reason.
    pub reason: *mut c_char,
    /// Dictionary summary.
    pub dictionary: ArcadiaTioCoordinateDictionarySummaryV2,
    /// External binding summary.
    pub external_binding: ArcadiaTioCoordinateExternalBindingV2,
    /// Native-owned optional index summaries.
    pub index_summaries: *mut ArcadiaTioCoordinateIndexSummaryV2,
    /// Number of optional index summaries.
    pub index_summaries_len: usize,
    /// Reserved words; callers set to zero.
    pub reserved: [u64; 4],
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

    /// Returns machine-readable OCB error kind for the current thread.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_last_error_kind() -> ArcadiaTioOcbErrorKind;
    /// Returns machine-readable OCB failure cause for the current thread.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_last_error_cause() -> ArcadiaTioOcbFailureCause;
    /// Opens an appendable OCB file and binds the handle to the selected committed snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open(path: *const c_char) -> *mut ArcadiaTioOcbFile;
    /// Opens an appendable OCB file with explicit validation options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open_with_options(
        path: *const c_char,
        options: *const ArcadiaTioOcbOpenOptions,
    ) -> *mut ArcadiaTioOcbFile;
    /// Clones an immutable selected-snapshot OCB reader handle.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_reader_clone(
        file: *mut ArcadiaTioOcbFile,
        out_reader: *mut *mut ArcadiaTioOcbFile,
    ) -> ArcadiaTioErrorCode;
    /// Closes an OCB handle.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_close(file: *mut ArcadiaTioOcbFile);
    /// Reads selected-snapshot OCB metadata.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_metadata(
        file: *mut ArcadiaTioOcbFile,
        out_metadata: *mut ArcadiaTioOcbMetadata,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB metadata result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_metadata_free(metadata: *mut ArcadiaTioOcbMetadata);
    /// Decodes one OCB dictionary on the explicit cold path.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_values(
        file: *mut ArcadiaTioOcbFile,
        dictionary_id: u32,
        out_values: *mut ArcadiaTioOcbDictionaryValues,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB dictionary-values result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_values_free(values: *mut ArcadiaTioOcbDictionaryValues);
    /// Reads projected/pruned OCB batches from the selected snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
    ) -> ArcadiaTioErrorCode;
    /// Reads projected/pruned OCB batches and attribution diagnostics.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches_with_attribution(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
        out_attribution: *mut ArcadiaTioOcbReadAttribution,
    ) -> ArcadiaTioErrorCode;
    /// Visits projected/pruned OCB batches incrementally.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_visit_batches(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        options: *const ArcadiaTioOcbReadCursorOptions,
        visitor: ArcadiaTioOcbBatchVisitor,
        user: *mut c_void,
        out_report: *mut ArcadiaTioOcbReadCursorReport,
    ) -> ArcadiaTioErrorCode;
    /// Reads one row group into caller-owned buffers.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_row_group_into(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbRowGroupFillRequest,
        out_report: *mut ArcadiaTioOcbReadFillReport,
    ) -> ArcadiaTioErrorCode;
    /// Plans an OCB read without reading payload chunks.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_plan_read(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_plan: *mut *mut ArcadiaTioOcbReadPlan,
    ) -> ArcadiaTioErrorCode;
    /// Copies a read-plan report into caller-provided output.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_report(
        plan: *const ArcadiaTioOcbReadPlan,
        out_report: *mut ArcadiaTioOcbReadReport,
    ) -> ArcadiaTioErrorCode;
    /// Copies projected column ids from a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_projected_column_ids(
        plan: *const ArcadiaTioOcbReadPlan,
        out_ids: *mut u32,
        out_ids_len: usize,
        out_required_len: *mut usize,
    ) -> ArcadiaTioErrorCode;
    /// Copies row-group ids from a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_row_group_ids(
        plan: *const ArcadiaTioOcbReadPlan,
        out_ids: *mut u32,
        out_ids_len: usize,
        out_required_len: *mut usize,
    ) -> ArcadiaTioErrorCode;
    /// Returns owned row-group summaries for a selected OCB snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries(
        file: *mut ArcadiaTioOcbFile,
        out_summaries: *mut ArcadiaTioOcbRowGroupSummaries,
    ) -> ArcadiaTioErrorCode;
    /// Returns owned row-group summaries for a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_row_group_summaries(
        file: *mut ArcadiaTioOcbFile,
        plan: *const ArcadiaTioOcbReadPlan,
        out_summaries: *mut ArcadiaTioOcbRowGroupSummaries,
    ) -> ArcadiaTioErrorCode;
    /// Reads OCB batches from an existing read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches_from_plan(
        file: *mut ArcadiaTioOcbFile,
        plan: *const ArcadiaTioOcbReadPlan,
        row_group_ids: *const u32,
        row_group_ids_len: usize,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB read report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_report_free(report: *mut ArcadiaTioOcbReadReport);
    /// Frees owned fields inside an OCB read attribution result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_attribution_free(attribution: *mut ArcadiaTioOcbReadAttribution);
    /// Frees owned fields inside an OCB read cursor report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_report_free(report: *mut ArcadiaTioOcbReadCursorReport);
    /// Frees an opaque OCB read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_free(plan: *mut ArcadiaTioOcbReadPlan);
    /// Frees owned fields inside an OCB read outcome.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_outcome_free(outcome: *mut ArcadiaTioOcbReadOutcome);
    /// Initializes OCB primitive values.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_primitive_values_init(values: *mut ArcadiaTioOcbPrimitiveValues);
    /// Initializes an OCB validity bitmap.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_validity_bitmap_init(bitmap: *mut ArcadiaTioOcbValidityBitmap);
    /// Initializes an OCB predicate value.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_predicate_value_init(value: *mut ArcadiaTioOcbPredicateValue);
    /// Initializes an OCB row-group predicate.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_predicate_init(predicate: *mut ArcadiaTioOcbRowGroupPredicate);
    /// Initializes an OCB read request.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_request_init(request: *mut ArcadiaTioOcbReadRequest);
    /// Initializes an OCB read report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_report_init(report: *mut ArcadiaTioOcbReadReport);
    /// Initializes an OCB read attribution result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_attribution_init(attribution: *mut ArcadiaTioOcbReadAttribution);
    /// Initializes OCB read cursor options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_options_init(options: *mut ArcadiaTioOcbReadCursorOptions);
    /// Initializes an OCB read cursor report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_report_init(report: *mut ArcadiaTioOcbReadCursorReport);
    /// Initializes an OCB column fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_init(buffer: *mut ArcadiaTioOcbColumnFillBuffer);
    /// Initializes an OCB row-group fill request.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_fill_request_init(
        request: *mut ArcadiaTioOcbRowGroupFillRequest,
    );
    /// Initializes an OCB read fill report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_fill_report_init(report: *mut ArcadiaTioOcbReadFillReport);
    /// Initializes an OCB read outcome.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_outcome_init(outcome: *mut ArcadiaTioOcbReadOutcome);
    /// Initializes OCB open options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open_options_init(options: *mut ArcadiaTioOcbOpenOptions);
    /// Initializes an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_init(column: *mut ArcadiaTioOcbWriteColumn);
    /// Initializes an OCB dictionary entry.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_entry_init(entry: *mut ArcadiaTioOcbDictionaryEntry);
    /// Initializes an OCB write dictionary.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_dictionary_init(dictionary: *mut ArcadiaTioOcbWriteDictionary);
    /// Initializes an OCB write column chunk.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_chunk_init(chunk: *mut ArcadiaTioOcbWriteColumnChunk);
    /// Initializes an OCB write row group.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_row_group_init(row_group: *mut ArcadiaTioOcbWriteRowGroup);
    /// Initializes an OCB write ordering key.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_ordering_key_init(key: *mut ArcadiaTioOcbWriteOrderingKey);
    /// Initializes an OCB write spec.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_spec_init(spec: *mut ArcadiaTioOcbWriteSpec);
    /// Initializes OCB write options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_options_init(options: *mut ArcadiaTioOcbWriteOptions);
    /// Initializes an OCB row-group summary output container.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries_init(summaries: *mut ArcadiaTioOcbRowGroupSummaries);
    /// Frees an OCB row-group summary output container.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries_free(summaries: *mut ArcadiaTioOcbRowGroupSummaries);
    /// Initializes an OCB cleanup result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_result_init(result: *mut ArcadiaTioOcbCleanupResult);
    /// Initializes compact-L2 certification options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_options_init(
        options: *mut ArcadiaTioOcbCompactL2CertificationOptions,
    );
    /// Initializes a compact-L2 certification report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_report_init(
        report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    );
    /// Sets fixed-binary width metadata on an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_set_fixed_binary_width(
        column: *mut ArcadiaTioOcbWriteColumn,
        width: u32,
    );
    /// Reads fixed-binary width metadata from an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_fixed_binary_width(
        column: *const ArcadiaTioOcbWriteColumn,
    ) -> u32;
    /// Sets fixed-binary width metadata on an OCB fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_set_fixed_binary_width(
        buffer: *mut ArcadiaTioOcbColumnFillBuffer,
        width: u32,
    );
    /// Reads fixed-binary width metadata from an OCB fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_fixed_binary_width(
        buffer: *const ArcadiaTioOcbColumnFillBuffer,
    ) -> u32;
    /// Reads fixed-binary width metadata from an OCB column descriptor.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_descriptor_fixed_binary_width(
        column: *const ArcadiaTioOcbColumnDescriptor,
    ) -> u32;
    /// Reads fixed-binary width metadata from an OCB read column array.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_array_fixed_binary_width(
        column: *const ArcadiaTioOcbColumnArray,
    ) -> u32;
    /// Creates an appendable OCB file.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_create(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
    ) -> ArcadiaTioErrorCode;
    /// Creates an appendable OCB file with explicit writer options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_create_with_options(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
    ) -> ArcadiaTioErrorCode;
    /// Appends one sorted suffix commit to an existing appendable OCB file.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_append(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
    ) -> ArcadiaTioErrorCode;
    /// Appends to an OCB file with explicit writer options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_append_with_options(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
    ) -> ArcadiaTioErrorCode;
    /// Truncates orphan tail bytes after the latest valid OCB root.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_orphan_tail(
        path: *const c_char,
        out_result: *mut ArcadiaTioOcbCleanupResult,
    ) -> ArcadiaTioErrorCode;
    /// Certifies a channel-sharded compact-L2 manifest.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_certify_compact_l2_manifest(
        manifest_path: *const c_char,
        options: *const ArcadiaTioOcbCompactL2CertificationOptions,
        out_report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned compact-L2 certification report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_report_free(
        report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    );

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
    /// Creates a V4 TensorFile using inferred layout-family selection, metadata overrides, and coordinate descriptors.
    pub fn arcadia_tio_create_inferred_with_coordinates(
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
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
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
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking, metadata overrides, and coordinate descriptors.
    pub fn arcadia_tio_create_with_policy_with_coordinates(
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
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
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
    /// Creates a RegularChunked V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_with_policy_with_coordinates_v2(
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
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile with inferred layout selection and Coordinate v2 descriptors.
    pub fn arcadia_tio_create_inferred_with_coordinates_v2(
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
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_random_access_with_coordinates_v2(
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
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_streaming_with_coordinates_v2(
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
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
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

    /// Reads Coordinate v2 descriptors from an open handle.
    pub fn arcadia_tio_coordinate_meta_v2(
        handle: *mut ArcadiaTioHandle,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMetaV2,
        out_len: *mut usize,
    ) -> c_int;
    /// Loads Coordinate v2 descriptors without opening a handle.
    pub fn arcadia_tio_load_coordinate_meta_v2(
        path: *const c_char,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMetaV2,
        out_len: *mut usize,
    ) -> c_int;
    /// Frees Coordinate v2 metadata arrays returned by metadata APIs.
    pub fn arcadia_tio_axis_coordinate_meta_v2_free(
        meta: *mut ArcadiaTioAxisCoordinateMetaV2,
        len: usize,
    );
    /// Reads Coordinate v2 values for one axis into an owned value carrier.
    pub fn arcadia_tio_read_axis_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        options: *const ArcadiaTioCoordinateV2Options,
        out_values: *mut ArcadiaTioCoordinateValueSliceV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 value slice.
    pub fn arcadia_tio_coordinate_value_slice_v2_free(
        values: *mut ArcadiaTioCoordinateValueSliceV2,
    );
    /// Reads Coordinate v2 dictionary metadata and entries.
    pub fn arcadia_tio_coordinate_dictionary_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        options: *const ArcadiaTioCoordinateV2Options,
        out_dictionary: *mut ArcadiaTioCoordinateDictionaryV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 dictionary result.
    pub fn arcadia_tio_coordinate_dictionary_v2_free(
        dictionary: *mut ArcadiaTioCoordinateDictionaryV2,
    );
    /// Performs an exact Coordinate v2 lookup.
    pub fn arcadia_tio_coordinate_lookup_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Performs a half-open range Coordinate v2 lookup.
    pub fn arcadia_tio_coordinate_lookup_range_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 lookup result.
    pub fn arcadia_tio_coordinate_lookup_result_v2_free(
        result: *mut ArcadiaTioCoordinateLookupResultV2,
    );

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
    /// Materializes a copy-only contiguous tensor.
    pub fn arcadia_tio_tensor_to_contiguous(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reshapes a tensor in row-major order.
    pub fn arcadia_tio_tensor_reshape(
        input: *const ArcadiaTioTensor,
        shape: *const u64,
        rank: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Flattens a tensor to shape `[numel]`.
    pub fn arcadia_tio_tensor_flatten(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Inserts a length-1 axis.
    pub fn arcadia_tio_tensor_expand_dims(
        input: *const ArcadiaTioTensor,
        axis: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Removes all length-1 axes.
    pub fn arcadia_tio_tensor_squeeze(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Removes one length-1 axis.
    pub fn arcadia_tio_tensor_squeeze_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Permutes axes and materializes row-major output.
    pub fn arcadia_tio_tensor_permute_axes(
        input: *const ArcadiaTioTensor,
        axes: *const i64,
        axes_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reverses axis order and materializes row-major output.
    pub fn arcadia_tio_tensor_transpose(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Slices one axis using `[start, end)`.
    pub fn arcadia_tio_tensor_slice_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        start: u64,
        end: u64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Slices one axis with a non-zero step.
    pub fn arcadia_tio_tensor_slice_axis_step(
        input: *const ArcadiaTioTensor,
        axis: i64,
        start: i64,
        end: i64,
        step: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Takes explicit indices on one axis.
    pub fn arcadia_tio_tensor_take_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        indices: *const u64,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Selects one index on an axis while preserving rank with axis length 1.
    pub fn arcadia_tio_tensor_index_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        index: u64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Adds two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_add(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Subtracts two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_sub(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Multiplies two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_mul(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Divides two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_div(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Adds a floating-point scalar to a tensor.
    pub fn arcadia_tio_tensor_add_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Subtracts a floating-point scalar from a tensor.
    pub fn arcadia_tio_tensor_sub_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Multiplies a tensor by a floating-point scalar.
    pub fn arcadia_tio_tensor_mul_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Divides a tensor by a floating-point scalar.
    pub fn arcadia_tio_tensor_div_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;

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
    /// Appends f32 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_f32_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_f64_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_i32_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_i64_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
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
    /// Analyzes how sparse-intent f32 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_f32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent f64 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_f64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i32 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_i32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i64 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_i64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Appends f32 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_f32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends f32 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_f32_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_f64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends f64 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_f64_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_i32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends i32 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_i32_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_i64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends i64 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_i64_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
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
    /// Frees native-owned strings in a historical read-index report.
    pub fn arcadia_tio_historical_read_index_report_free(
        report: *mut ArcadiaTioHistoricalReadIndexReport,
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
    /// Reads data through low-level read-index items into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_dense(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items with execution options.
    pub fn arcadia_tio_read_index_at_commit_with_options(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_at_commit_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
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
