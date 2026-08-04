use super::*;

/// Result type returned by OCB safe wrappers.
pub type OcbResult<T> = std::result::Result<T, OcbError>;

/// OCB validation depth used while opening a selected snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenValidation {
    /// Validate metadata graph and chunk headers; payload CRCs are checked when selected chunks are read.
    MetadataGraph,
    /// Validate every referenced payload before returning from open.
    FullPayload,
}

impl OpenValidation {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbOpenValidation {
        match self {
            Self::MetadataGraph => sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH,
            Self::FullPayload => sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_FULL_PAYLOAD,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbOpenValidation) -> OcbResult<Self> {
        match raw {
            sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH => Ok(Self::MetadataGraph),
            sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_FULL_PAYLOAD => Ok(Self::FullPayload),
            other => Err(OcbError::invalid_input(format!(
                "unknown OCB open validation value {other}"
            ))),
        }
    }
}

/// OCB open options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenOptions {
    /// Validation depth applied before open returns.
    pub validation: OpenValidation,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            validation: OpenValidation::MetadataGraph,
        }
    }
}

/// Finite byte limits applied to OCB reads through one opened handle.
///
/// The default is Policy A. Larger finite values should be selected only
/// for reviewed workloads; streaming/fill APIs remain preferable when an
/// owned result would exceed the aggregate limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResourceLimits {
    /// Maximum encoded size of one OCB body object.
    pub max_encoded_object_bytes: u64,
    /// Maximum encoded size of one compressed column-chunk payload.
    pub max_compressed_chunk_bytes: u64,
    /// Maximum decoded size of one column-chunk payload.
    pub max_decompressed_chunk_bytes: u64,
    /// Maximum decoded/materialized size of one projected row group.
    pub max_projected_row_group_bytes: u64,
    /// Maximum selected compressed bytes retained by one owned read request.
    ///
    /// Independently caps unique dictionary/key-tuple auxiliary-object
    /// bytes validated while opening a handle. Full-payload chunks are not
    /// charged to that open-time auxiliary budget.
    pub max_owned_selected_compressed_bytes: u64,
    /// Maximum decoded/materialized bytes retained by one owned read request.
    ///
    /// Independently caps logical metadata allocations while validating
    /// one root candidate. V2 candidates are attempted sequentially.
    pub max_owned_decoded_materialized_bytes: u64,
}

impl ResourceLimits {
    /// Return the approved finite default policy used by legacy opens.
    pub const fn policy_a() -> Self {
        Self {
            max_encoded_object_bytes: 1_073_741_824,
            max_compressed_chunk_bytes: 536_870_912,
            max_decompressed_chunk_bytes: 536_870_912,
            max_projected_row_group_bytes: 1_073_741_824,
            max_owned_selected_compressed_bytes: 8_589_934_592,
            max_owned_decoded_materialized_bytes: 17_179_869_184,
        }
    }
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self::policy_a()
    }
}

/// OCB selected-snapshot export-copy options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SnapshotExportOptions {
    /// Validation depth applied to the source snapshot and staged copy.
    pub validation: OpenValidation,
}

impl Default for SnapshotExportOptions {
    fn default() -> Self {
        Self {
            validation: OpenValidation::MetadataGraph,
        }
    }
}

/// OCB manifest build options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestBuildOptions {
    /// Validation depth applied to each input file while building the manifest.
    pub validation: OpenValidation,
    /// Whether to compute and include full-file SHA-256 digests.
    pub compute_file_digest: bool,
    /// Optional tool name override recorded in `generated_by`.
    pub generated_by_name: Option<String>,
    /// Optional tool version override recorded in `generated_by`.
    pub generated_by_version: Option<String>,
    /// Optional generated-at timestamp override.
    pub generated_at_unix_seconds: Option<u64>,
}

impl Default for ManifestBuildOptions {
    fn default() -> Self {
        Self {
            validation: OpenValidation::MetadataGraph,
            compute_file_digest: false,
            generated_by_name: None,
            generated_by_version: None,
            generated_at_unix_seconds: None,
        }
    }
}

/// File-set compatibility status returned by OCB manifest validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompatibilityStatus {
    /// Manifest entries match the current selected local snapshots.
    Compatible,
    /// One or more manifest entries do not match current local snapshots.
    Incompatible,
    /// Compatibility could not be fully determined.
    Unknown,
    /// Unknown forward-compatible raw value.
    Other(i32),
}

impl CompatibilityStatus {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbCompatibilityStatus) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_COMPATIBILITY_STATUS_COMPATIBLE => Self::Compatible,
            sys::ARCADIA_TIO_OCB_COMPATIBILITY_STATUS_INCOMPATIBLE => Self::Incompatible,
            sys::ARCADIA_TIO_OCB_COMPATIBILITY_STATUS_UNKNOWN => Self::Unknown,
            other => Self::Other(other),
        }
    }
}

/// Generic health status returned by OCB maintenance analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    /// The selected snapshot is valid.
    Valid,
    /// The file is invalid or no selected snapshot could be bound.
    Invalid,
    /// Health could not be fully determined.
    Unknown,
    /// Unknown forward-compatible raw value.
    Other(i32),
}

impl HealthStatus {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbHealthStatus) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_HEALTH_STATUS_VALID => Self::Valid,
            sys::ARCADIA_TIO_OCB_HEALTH_STATUS_INVALID => Self::Invalid,
            sys::ARCADIA_TIO_OCB_HEALTH_STATUS_UNKNOWN => Self::Unknown,
            other => Self::Other(other),
        }
    }
}

/// Structured OCB error with the ordinary C ABI code plus OCB-specific metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcbError {
    pub(super) code: ErrorCode,
    pub(super) kind: ErrorKind,
    pub(super) cause: Option<FailureCause>,
    pub(super) message: String,
}

impl OcbError {
    pub(super) fn last(fallback: &str) -> Self {
        let raw_code = unsafe { sys::arcadia_tio_last_error_code() };
        let raw_kind = unsafe { sys::arcadia_tio_ocb_last_error_kind() };
        let raw_cause = unsafe { sys::arcadia_tio_ocb_last_error_cause() };
        let raw_message = unsafe { sys::arcadia_tio_last_error_message() };
        let message = if raw_message.is_null() {
            fallback.to_string()
        } else {
            unsafe { CStr::from_ptr(raw_message) }
                .to_string_lossy()
                .into_owned()
        };
        Self {
            code: ErrorCode::from_raw(raw_code),
            kind: ErrorKind::from_raw(raw_kind),
            cause: FailureCause::from_raw(raw_cause),
            message,
        }
    }

    pub(super) fn invalid_input(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidArgument,
            kind: ErrorKind::InvalidInput,
            cause: Some(FailureCause::InvalidInput),
            message: message.into(),
        }
    }

    pub(super) fn from_tio_error(err: TioError) -> Self {
        Self {
            code: err.code(),
            kind: ErrorKind::InvalidInput,
            cause: Some(FailureCause::InvalidInput),
            message: err.message().to_string(),
        }
    }

    /// Ordinary C ABI error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// OCB-specific error kind.
    pub fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// OCB-specific failure cause when present.
    pub fn cause(&self) -> Option<FailureCause> {
        self.cause
    }

    /// Human diagnostic message.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for OcbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for OcbError {}

/// OCB structured error kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    /// No OCB error.
    None,
    /// Invalid caller input.
    InvalidInput,
    /// Unsupported format or option.
    UnsupportedFormat,
    /// Corrupt file contents.
    CorruptFile,
    /// Lock was unavailable.
    LockUnavailable,
    /// Low-level I/O failure.
    Io,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl ErrorKind {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbErrorKind) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_ERROR_KIND_NONE => Self::None,
            sys::ARCADIA_TIO_OCB_ERROR_KIND_INVALID_INPUT => Self::InvalidInput,
            sys::ARCADIA_TIO_OCB_ERROR_KIND_UNSUPPORTED_FORMAT => Self::UnsupportedFormat,
            sys::ARCADIA_TIO_OCB_ERROR_KIND_CORRUPT_FILE => Self::CorruptFile,
            sys::ARCADIA_TIO_OCB_ERROR_KIND_LOCK_UNAVAILABLE => Self::LockUnavailable,
            sys::ARCADIA_TIO_OCB_ERROR_KIND_IO => Self::Io,
            other => Self::Unknown(other),
        }
    }
}

/// OCB structured failure cause.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCause {
    /// Invalid caller input.
    InvalidInput,
    /// Unsupported OCB format.
    UnsupportedFormat,
    /// Corrupt file contents.
    CorruptFile,
    /// Lock was unavailable.
    LockUnavailable,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl FailureCause {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbFailureCause) -> Option<Self> {
        match raw {
            sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_NONE => None,
            sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_INVALID_INPUT => Some(Self::InvalidInput),
            sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_UNSUPPORTED_FORMAT => Some(Self::UnsupportedFormat),
            sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_CORRUPT_FILE => Some(Self::CorruptFile),
            sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_LOCK_UNAVAILABLE => Some(Self::LockUnavailable),
            other => Some(Self::Unknown(other)),
        }
    }
}

/// Generic OCB body-object kind recorded by a summary body reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    Unknown,
    Root,
    Schema,
    DictionaryIndex,
    DictionaryValues,
    RowGroupIndex,
    OrderingProof,
    ColumnChunk,
    StringTable,
    DebugJsonMetadata,
    ValidityBitmap,
    KeyTuple,
    RowGroupIndexDelta,
    Other(i32),
}

impl BodyKind {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbBodyKind) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_BODY_KIND_UNKNOWN => Self::Unknown,
            sys::ARCADIA_TIO_OCB_BODY_KIND_ROOT => Self::Root,
            sys::ARCADIA_TIO_OCB_BODY_KIND_SCHEMA => Self::Schema,
            sys::ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_INDEX => Self::DictionaryIndex,
            sys::ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_VALUES => Self::DictionaryValues,
            sys::ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX => Self::RowGroupIndex,
            sys::ARCADIA_TIO_OCB_BODY_KIND_ORDERING_PROOF => Self::OrderingProof,
            sys::ARCADIA_TIO_OCB_BODY_KIND_COLUMN_CHUNK => Self::ColumnChunk,
            sys::ARCADIA_TIO_OCB_BODY_KIND_STRING_TABLE => Self::StringTable,
            sys::ARCADIA_TIO_OCB_BODY_KIND_DEBUG_JSON_METADATA => Self::DebugJsonMetadata,
            sys::ARCADIA_TIO_OCB_BODY_KIND_VALIDITY_BITMAP => Self::ValidityBitmap,
            sys::ARCADIA_TIO_OCB_BODY_KIND_KEY_TUPLE => Self::KeyTuple,
            sys::ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX_DELTA => Self::RowGroupIndexDelta,
            other => Self::Other(other),
        }
    }
}

/// Generic OCB checksum kind recorded by a body reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChecksumKind {
    None,
    Crc32c,
    Other(i32),
}

impl ChecksumKind {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbChecksumKind) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_CHECKSUM_KIND_NONE => Self::None,
            sys::ARCADIA_TIO_OCB_CHECKSUM_KIND_CRC32C => Self::Crc32c,
            other => Self::Other(other),
        }
    }
}

/// Generic column-chunk summary codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnChunkSummaryCodec {
    None,
    Zstd,
    Other(i32),
}

impl ColumnChunkSummaryCodec {
    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbColumnChunkSummaryCodec) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_NONE => Self::None,
            sys::ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_ZSTD => Self::Zstd,
            other => Self::Other(other),
        }
    }
}

/// OCB writer chunk codec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteChunkCodec {
    None,
    Zstd { level: i32 },
}

impl WriteChunkCodec {
    pub(super) fn to_raw(self) -> (sys::ArcadiaTioOcbWriteChunkCodec, i32) {
        match self {
            Self::None => (sys::ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_NONE, 3),
            Self::Zstd { level } => (sys::ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_ZSTD, level),
        }
    }
}

/// OCB write options for create/append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOptions {
    pub write_threads: usize,
    pub chunk_codec: WriteChunkCodec,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            write_threads: 1,
            chunk_codec: WriteChunkCodec::None,
        }
    }
}

impl WriteOptions {
    pub fn zstd(level: i32) -> Self {
        Self {
            write_threads: 1,
            chunk_codec: WriteChunkCodec::Zstd { level },
        }
    }

    pub fn with_write_threads(mut self, write_threads: usize) -> Self {
        self.write_threads = write_threads;
        self
    }

    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbWriteOptions {
        let (chunk_codec, zstd_level) = self.chunk_codec.to_raw();
        sys::ArcadiaTioOcbWriteOptions {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteOptions>(),
            write_threads: self.write_threads,
            chunk_codec,
            zstd_level,
            reserved: [0; 4],
        }
    }
}

/// Diagnostic phase timings for one OCB create/append operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WritePhaseTimings {
    pub to_internal_ns: u64,
    pub validate_spec_ns: u64,
    pub validate_dictionary_codes_ns: u64,
    pub validate_ordering_ns: u64,
    pub append_base_read_ns: u64,
    pub append_base_validate_ns: u64,
    pub row_group_encode_ns: u64,
    pub row_group_merge_ns: u64,
    pub metadata_encode_ns: u64,
    pub file_write_ns: u64,
    pub sync_data_ns: u64,
    pub commit_validate_ns: u64,
    pub slot_publish_ns: u64,
    pub sync_all_ns: u64,
    pub rename_ns: u64,
    pub parent_sync_ns: u64,
}

/// Diagnostic counters for one OCB create/append operation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WriteReport {
    pub requested_write_threads: usize,
    pub effective_write_threads: usize,
    pub row_count: u64,
    pub row_group_count: u32,
    pub column_count: u32,
    pub dictionary_count: u32,
    pub dictionary_coded_column_count: u32,
    pub column_chunk_count: u32,
    pub stat_count: u32,
    pub payload_bytes: u64,
    pub validity_bytes: u64,
    pub row_group_object_bytes: u64,
    pub file_bytes: u64,
    pub tail_bytes: u64,
    pub root_generation: u64,
    pub previous_root_generation: u64,
    pub parallel_batches: usize,
    pub worker_count: usize,
    pub timings: WritePhaseTimings,
}

/// Options for certifying one local compact-L2 physical-v2 OCB artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ArtifactCertificationOptions {
    /// Optional expected total row count.
    pub expected_row_count: Option<u64>,
    /// Optional expected trading day encoded in the artifact rows.
    pub expected_trading_day: Option<u32>,
    /// Optional expected channel id encoded in the artifact rows.
    pub expected_channel_id: Option<u32>,
    /// Optional expected first BizIndex.
    pub expected_first_biz_index: Option<u64>,
    /// Optional expected last BizIndex.
    pub expected_last_biz_index: Option<u64>,
    /// Validate row-level scalar values and gap-free BizIndex continuity.
    pub verify_scalar_continuity: bool,
    /// Reconstruct legacy 168-byte payloads while scanning rows.
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

impl CompactL2PhysicalV2ArtifactCertificationOptions {
    /// Defaults matching the native physical-v2 artifact certifier.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for CompactL2PhysicalV2ArtifactCertificationOptions {
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

/// Path-redacted report for one compact-L2 physical-v2 artifact certification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2ArtifactCertificationReport {
    pub row_count: u64,
    pub row_group_count: u32,
    pub required_column_count: usize,
    pub selected_column_chunk_count: u64,
    pub selected_compressed_bytes: u64,
    pub selected_uncompressed_bytes: u64,
    pub first_biz_index: Option<u64>,
    pub last_biz_index: Option<u64>,
    pub min_receive_nano: Option<i64>,
    pub max_receive_nano: Option<i64>,
    pub order_record_count: Option<u64>,
    pub trade_record_count: Option<u64>,
    pub legacy_payload_hash_fnv1a64: Option<String>,
    pub legacy_payload_hash_verified: bool,
    pub certified: bool,
    pub path_redacted: bool,
    pub writes_transformed_artifacts: bool,
}

/// Primitive physical type supported by OCB columns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicalType {
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
    /// 32-bit float.
    F32,
    /// 64-bit float.
    F64,
    /// Fixed-width opaque byte values.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
    },
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl PhysicalType {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbPhysicalType {
        match self {
            Self::I32 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64,
            Self::F32 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32,
            Self::F64 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64,
            Self::FixedBinary { .. } => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY,
            Self::Unknown(raw) => raw,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbPhysicalType) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32 => Self::I32,
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64 => Self::I64,
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32 => Self::F32,
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64 => Self::F64,
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY => Self::FixedBinary { width: 0 },
            other => Self::Unknown(other),
        }
    }

    pub(super) fn from_raw_with_width(raw: sys::ArcadiaTioOcbPhysicalType, width: u32) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY => Self::FixedBinary { width },
            _ => Self::from_raw(raw),
        }
    }

    pub(super) fn fixed_binary_width(self) -> u32 {
        match self {
            Self::FixedBinary { width } => width,
            _ => 0,
        }
    }
}

/// Logical kind attached to an OCB column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogicalKind {
    /// Plain primitive values.
    Plain,
    /// Timestamp-nanos-like integer values.
    TimestampNanosLike,
    /// Scaled integer values.
    ScaledInteger,
    /// Dictionary-code values.
    DictionaryCode,
    /// Enum-code values.
    EnumCode,
    /// Opaque ordering key values.
    OpaqueKey,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl LogicalKind {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbLogicalKind {
        match self {
            Self::Plain => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN,
            Self::TimestampNanosLike => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_TIMESTAMP_NANOS_LIKE,
            Self::ScaledInteger => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_SCALED_INTEGER,
            Self::DictionaryCode => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_DICTIONARY_CODE,
            Self::EnumCode => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_ENUM_CODE,
            Self::OpaqueKey => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_OPAQUE_KEY,
            Self::Unknown(raw) => raw,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbLogicalKind) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN => Self::Plain,
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_TIMESTAMP_NANOS_LIKE => Self::TimestampNanosLike,
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_SCALED_INTEGER => Self::ScaledInteger,
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_DICTIONARY_CODE => Self::DictionaryCode,
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_ENUM_CODE => Self::EnumCode,
            sys::ARCADIA_TIO_OCB_LOGICAL_KIND_OPAQUE_KEY => Self::OpaqueKey,
            other => Self::Unknown(other),
        }
    }
}

/// OCB dictionary decoded value kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryValueKind {
    /// UTF-8 string values.
    Utf8,
    /// Variable-length byte values.
    Bytes,
    /// Fixed-width byte values.
    FixedBytes,
    /// Enum-label string values.
    EnumLabels,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl DictionaryValueKind {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbDictionaryValueKind {
        match self {
            Self::Utf8 => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8,
            Self::Bytes => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_BYTES,
            Self::FixedBytes => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_FIXED_BYTES,
            Self::EnumLabels => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_ENUM_LABELS,
            Self::Unknown(raw) => raw,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbDictionaryValueKind) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8 => Self::Utf8,
            sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_BYTES => Self::Bytes,
            sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_FIXED_BYTES => Self::FixedBytes,
            sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_ENUM_LABELS => Self::EnumLabels,
            other => Self::Unknown(other),
        }
    }
}

/// Ordering direction for OCB ordering keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderingDirection {
    /// Ascending order.
    Ascending,
    /// Descending order.
    Descending,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl OrderingDirection {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbOrderingDirection {
        match self {
            Self::Ascending => sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_ASCENDING,
            Self::Descending => sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_DESCENDING,
            Self::Unknown(raw) => raw,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbOrderingDirection) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_ASCENDING => Self::Ascending,
            sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_DESCENDING => Self::Descending,
            other => Self::Unknown(other),
        }
    }
}

/// Null ordering for OCB ordering keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NullOrder {
    /// Nulls compare first.
    NullsFirst,
    /// Nulls compare last.
    NullsLast,
    /// Column has no nulls for this ordering key.
    NoNulls,
    /// Unknown forward-compatible raw value.
    Unknown(i32),
}

impl NullOrder {
    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbNullOrder {
        match self {
            Self::NullsFirst => sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_FIRST,
            Self::NullsLast => sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_LAST,
            Self::NoNulls => sys::ARCADIA_TIO_OCB_NULL_ORDER_NO_NULLS,
            Self::Unknown(raw) => raw,
        }
    }

    pub(super) fn from_raw(raw: sys::ArcadiaTioOcbNullOrder) -> Self {
        match raw {
            sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_FIRST => Self::NullsFirst,
            sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_LAST => Self::NullsLast,
            sys::ARCADIA_TIO_OCB_NULL_ORDER_NO_NULLS => Self::NoNulls,
            other => Self::Unknown(other),
        }
    }
}

/// OCB primitive payload values.
#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveValues {
    /// i32 values.
    I32(Vec<i32>),
    /// i64 values.
    I64(Vec<i64>),
    /// f32 values.
    F32(Vec<f32>),
    /// f64 values.
    F64(Vec<f64>),
    /// Fixed-width opaque bytes stored contiguously row-major.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
        /// Contiguous row-major bytes.
        bytes: Vec<u8>,
    },
}

impl PrimitiveValues {
    pub(super) fn physical_type(&self) -> PhysicalType {
        match self {
            Self::I32(_) => PhysicalType::I32,
            Self::I64(_) => PhysicalType::I64,
            Self::F32(_) => PhysicalType::F32,
            Self::F64(_) => PhysicalType::F64,
            Self::FixedBinary { width, .. } => PhysicalType::FixedBinary { width: *width },
        }
    }

    pub(super) fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::FixedBinary { width: 0, .. } => 0,
            Self::FixedBinary { width, bytes } => bytes.len() / *width as usize,
        }
    }

    pub(super) fn data_ptr(&self) -> *const c_void {
        match self {
            Self::I32(values) => values.as_ptr().cast(),
            Self::I64(values) => values.as_ptr().cast(),
            Self::F32(values) => values.as_ptr().cast(),
            Self::F64(values) => values.as_ptr().cast(),
            Self::FixedBinary { bytes, .. } => bytes.as_ptr().cast(),
        }
    }

    pub(super) fn to_raw(&self) -> sys::ArcadiaTioOcbPrimitiveValues {
        sys::ArcadiaTioOcbPrimitiveValues {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbPrimitiveValues>(),
            physical_type: self.physical_type().to_raw(),
            data: self.data_ptr(),
            len: self.len(),
            reserved: [u64::from(self.physical_type().fixed_binary_width()), 0, 0],
        }
    }
}

/// Validity bitmap with least-significant-bit-first bits; bit 1 means valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityBitmap {
    /// Bitmap bytes.
    pub bytes: Vec<u8>,
    /// Meaningful row count.
    pub row_count: u64,
}

/// Column declaration for OCB create/append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteColumn {
    /// Column name.
    pub name: String,
    /// Physical primitive type.
    pub physical_type: PhysicalType,
    /// Logical column kind.
    pub logical_kind: LogicalKind,
    /// Optional dictionary id for dictionary-coded columns.
    pub dictionary_id: Option<u32>,
    /// Decimal scale for scaled-integer logical columns.
    pub scale: i32,
    /// Whether values may be null.
    pub nullable: bool,
}

/// Dictionary declaration for OCB create/append.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriteDictionary {
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Dictionary name.
    pub name: String,
    /// Physical code type used by dictionary-coded columns.
    pub code_physical_type: PhysicalType,
    /// Decoded value kind.
    pub value_kind: DictionaryValueKind,
    /// Fixed byte width when `value_kind` is fixed bytes.
    pub fixed_width: u32,
    /// Decoded dictionary entries as bytes.
    pub entries: Vec<Vec<u8>>,
}

/// Column chunk for one OCB write row group.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteColumnChunk {
    /// File-local column id.
    pub column_id: u32,
    /// Primitive values.
    pub values: PrimitiveValues,
    /// Optional validity bitmap.
    pub validity: Option<ValidityBitmap>,
}

/// Row group for OCB create/append.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteRowGroup {
    /// Column chunks in this row group.
    pub columns: Vec<WriteColumnChunk>,
}

/// Ordering key declaration for OCB create/append.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WriteOrderingKey {
    /// File-local column id.
    pub column_id: u32,
    /// Sort direction.
    pub direction: OrderingDirection,
    /// Null ordering.
    pub null_order: NullOrder,
}

/// Complete OCB write specification for create/append.
#[derive(Debug, Clone, PartialEq)]
pub struct WriteSpec {
    /// Frozen column schema.
    pub columns: Vec<WriteColumn>,
    /// Frozen dictionary declarations.
    pub dictionaries: Vec<WriteDictionary>,
    /// Row groups to publish in this commit.
    pub row_groups: Vec<WriteRowGroup>,
    /// Ordering keys.
    pub ordering_keys: Vec<WriteOrderingKey>,
}

/// Metadata for an opened OCB snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Metadata {
    /// Format name, currently `OCB`.
    pub format_name: String,
    /// Whether the format is appendable.
    pub appendable: bool,
    /// Selected root generation.
    pub root_generation: u64,
    /// Previous root generation when present.
    pub previous_root_generation: Option<u64>,
    /// Visible row count.
    pub row_count: u64,
    /// Visible row group count.
    pub row_group_count: u32,
    /// Visible column chunk count.
    pub column_chunk_count: u32,
    /// Column descriptors.
    pub columns: Vec<ColumnDescriptor>,
    /// Dictionary descriptors.
    pub dictionaries: Vec<DictionaryDescriptor>,
    /// Ordering keys.
    pub ordering_keys: Vec<OrderingKey>,
}

/// OCB column metadata descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnDescriptor {
    /// File-local column id.
    pub id: u32,
    /// Column name.
    pub name: String,
    /// Physical type.
    pub physical_type: PhysicalType,
    /// Logical kind.
    pub logical_kind: LogicalKind,
    /// Dictionary id when present.
    pub dictionary_id: Option<u32>,
    /// Decimal scale.
    pub scale: i32,
    /// Whether values may be null.
    pub nullable: bool,
}

/// OCB dictionary metadata descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryDescriptor {
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Dictionary name.
    pub name: String,
    /// Code physical type.
    pub code_physical_type: PhysicalType,
    /// Decoded value kind.
    pub value_kind: DictionaryValueKind,
    /// Number of entries.
    pub entry_count: u32,
}

/// OCB ordering-key metadata descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderingKey {
    /// File-local column id.
    pub column_id: u32,
    /// Column name.
    pub column_name: String,
    /// Sort direction.
    pub direction: OrderingDirection,
    /// Null ordering.
    pub null_order: NullOrder,
}

/// Decoded dictionary values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryValues {
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// Dictionary name.
    pub name: String,
    /// Decoded values.
    pub values: DecodedDictionaryValues,
}

/// Decoded dictionary payload variants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodedDictionaryValues {
    /// UTF-8 string values.
    Utf8(Vec<String>),
    /// Variable-length byte values.
    Bytes(Vec<Vec<u8>>),
    /// Fixed-width byte values.
    FixedBytes {
        fixed_width: u32,
        values: Vec<Vec<u8>>,
    },
    /// Enum-label string values.
    EnumLabels(Vec<String>),
    /// Unknown value kind with copied raw strings/bytes when possible.
    Unknown {
        raw_kind: i32,
        strings: Vec<String>,
        bytes: Vec<Vec<u8>>,
    },
}

/// Column projection for OCB reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Projection {
    /// Select all columns.
    All,
    /// Select columns by name.
    Names(Vec<String>),
}

/// Predicate primitive value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PredicateValue {
    /// i32 value.
    I32(i32),
    /// i64 value.
    I64(i64),
    /// f32 value.
    F32(f32),
    /// f64 value.
    F64(f64),
}

impl PredicateValue {
    pub(super) fn physical_type(self) -> PhysicalType {
        match self {
            Self::I32(_) => PhysicalType::I32,
            Self::I64(_) => PhysicalType::I64,
            Self::F32(_) => PhysicalType::F32,
            Self::F64(_) => PhysicalType::F64,
        }
    }

    pub(super) fn cmp_same_type(self, other: Self) -> OcbResult<CmpOrdering> {
        match (self, other) {
            (Self::I32(left), Self::I32(right)) => Ok(left.cmp(&right)),
            (Self::I64(left), Self::I64(right)) => Ok(left.cmp(&right)),
            (Self::F32(left), Self::F32(right)) => left.partial_cmp(&right).ok_or_else(|| {
                OcbError::invalid_input("OCB f32 predicate/stat value cannot be NaN")
            }),
            (Self::F64(left), Self::F64(right)) => left.partial_cmp(&right).ok_or_else(|| {
                OcbError::invalid_input("OCB f64 predicate/stat value cannot be NaN")
            }),
            _ => Err(OcbError::invalid_input("OCB predicate/stat type mismatch")),
        }
    }

    pub(super) fn to_raw(self) -> sys::ArcadiaTioOcbPredicateValue {
        let mut raw = sys::ArcadiaTioOcbPredicateValue {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbPredicateValue>(),
            physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
            i32_value: 0,
            i64_value: 0,
            f32_value: 0.0,
            f64_value: 0.0,
            reserved: [0; 3],
        };
        match self {
            Self::I32(value) => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32;
                raw.i32_value = value;
            }
            Self::I64(value) => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64;
                raw.i64_value = value;
            }
            Self::F32(value) => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32;
                raw.f32_value = value;
            }
            Self::F64(value) => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64;
                raw.f64_value = value;
            }
        }
        raw
    }
}

/// Inclusive row-group predicate over one column.
#[derive(Debug, Clone, PartialEq)]
pub struct RowGroupPredicate {
    /// Column name.
    pub column: String,
    /// Inclusive lower bound when present.
    pub lower: Option<PredicateValue>,
    /// Inclusive upper bound when present.
    pub upper: Option<PredicateValue>,
}

/// Scalar bounds for one declared OCB ordering-key column.
///
/// This is a row-group pruning helper, not a row-level filter or
/// lexicographic cursor engine. For composite ordering declarations,
/// multiple ranges become ordinary conjunctive predicates and may include
/// extra rows that callers should filter outside OCB if exact row-level
/// semantics are required.
#[derive(Debug, Clone, PartialEq)]
pub struct OrderingKeyRange {
    /// Zero-based index into [`Metadata::ordering_keys`].
    pub key_index: usize,
    /// Inclusive scalar lower bound.
    pub lower: Option<PredicateValue>,
    /// Inclusive scalar upper bound.
    pub upper: Option<PredicateValue>,
}

impl OrderingKeyRange {
    /// Create a range with optional inclusive bounds.
    pub fn new(
        key_index: usize,
        lower: Option<PredicateValue>,
        upper: Option<PredicateValue>,
    ) -> Self {
        Self {
            key_index,
            lower,
            upper,
        }
    }

    /// Create a closed inclusive range.
    pub fn between(key_index: usize, lower: PredicateValue, upper: PredicateValue) -> Self {
        Self::new(key_index, Some(lower), Some(upper))
    }

    /// Create an equality range.
    pub fn equal(key_index: usize, value: PredicateValue) -> Self {
        Self::new(key_index, Some(value), Some(value))
    }
}

/// OCB read request.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadRequest {
    /// Column projection.
    pub projection: Projection,
    /// Row-group pruning predicates.
    pub predicates: Vec<RowGroupPredicate>,
    /// Requested worker threads.
    pub max_threads: usize,
    /// Validate checksums while reading.
    pub validate_checksums: bool,
    /// Reserved dictionary decode flag; current reads still return codes.
    pub decode_dictionaries: bool,
}

impl Default for ReadRequest {
    fn default() -> Self {
        Self {
            projection: Projection::All,
            predicates: Vec::new(),
            max_threads: 1,
            validate_checksums: true,
            decode_dictionaries: false,
        }
    }
}

impl ReadRequest {
    /// Build a read request from inclusive scalar bounds over declared
    /// ordering-key columns.
    ///
    /// The returned request contains ordinary row-group pruning predicates
    /// and default read options. It does not add row-level filtering or
    /// lexicographic cursor semantics.
    pub fn from_ordering_key_ranges(
        metadata: &Metadata,
        projection: Projection,
        ranges: Vec<OrderingKeyRange>,
    ) -> OcbResult<Self> {
        Self {
            projection,
            ..Self::default()
        }
        .with_ordering_key_ranges(metadata, ranges)
    }

    /// Replace this request's predicates with predicates derived from
    /// ordering-key ranges while preserving projection and read options.
    pub fn with_ordering_key_ranges(
        mut self,
        metadata: &Metadata,
        ranges: Vec<OrderingKeyRange>,
    ) -> OcbResult<Self> {
        self.predicates = ordering_key_range_predicates(metadata, ranges)?;
        Ok(self)
    }
}

pub(super) fn ordering_key_range_predicates(
    metadata: &Metadata,
    ranges: Vec<OrderingKeyRange>,
) -> OcbResult<Vec<RowGroupPredicate>> {
    if ranges.is_empty() {
        return Err(OcbError::invalid_input(
            "OCB ordering range request requires at least one bound",
        ));
    }
    let mut ranges = ranges;
    ranges.sort_by_key(|range| range.key_index);
    let mut seen = BTreeSet::new();
    let mut predicates = Vec::with_capacity(ranges.len());
    for range in ranges {
        if !seen.insert(range.key_index) {
            return Err(OcbError::invalid_input(
                "OCB ordering range request contains duplicate key indexes",
            ));
        }
        if range.lower.is_none() && range.upper.is_none() {
            return Err(OcbError::invalid_input(
                "OCB ordering range bound must include at least one side",
            ));
        }
        let key = metadata.ordering_keys.get(range.key_index).ok_or_else(|| {
            OcbError::invalid_input("OCB ordering range references an unknown ordering key")
        })?;
        let column = metadata
            .columns
            .iter()
            .find(|column| column.id == key.column_id)
            .ok_or_else(|| {
                OcbError::invalid_input("OCB ordering key column is missing from metadata")
            })?;
        if matches!(column.physical_type, PhysicalType::FixedBinary { .. }) {
            return Err(OcbError::invalid_input(
                "OCB ordering range over fixed-binary columns is not supported",
            ));
        }
        for bound in [range.lower, range.upper].into_iter().flatten() {
            if bound.physical_type() != column.physical_type {
                return Err(OcbError::invalid_input(
                    "OCB ordering range bound dtype does not match ordering column dtype",
                ));
            }
        }
        if let (Some(lower), Some(upper)) = (range.lower, range.upper) {
            if lower.cmp_same_type(upper)? == CmpOrdering::Greater {
                return Err(OcbError::invalid_input(
                    "OCB ordering range lower bound is greater than upper bound",
                ));
            }
        }
        predicates.push(RowGroupPredicate {
            column: key.column_name.clone(),
            lower: range.lower,
            upper: range.upper,
        });
    }
    Ok(predicates)
}

/// Snapshot-local OCB read plan.
///
/// A plan contains generic projected column ids and selected row-group ids.
/// Row-group ids are file-local/snapshot-local, not stable business ids.
#[derive(Debug)]
pub struct ReadPlan<'a> {
    pub(super) raw: NonNull<sys::ArcadiaTioOcbReadPlan>,
    pub(super) file_raw: NonNull<sys::ArcadiaTioOcbFile>,
    pub(super) _abi: AbiCompatible,
    /// File-local column ids selected by the projection.
    pub projected_column_ids: Vec<u32>,
    /// File-local row-group ids selected by predicates.
    pub row_group_ids: Vec<u32>,
    /// Planning report for the request.
    pub report: ReadReport,
    pub(super) _file: PhantomData<&'a ColumnBundleFile>,
}

/// OCB read outcome with owned batches and report.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadOutcome {
    /// Returned column batches.
    pub batches: Vec<ColumnBatch>,
    /// Read execution report.
    pub report: ReadReport,
}

/// OCB read outcome with opt-in diagnostics.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributedReadOutcome {
    /// Returned column batches and ordinary read report.
    pub outcome: ReadOutcome,
    /// Diagnostic attribution counters. These are not benchmark claims.
    pub attribution: ReadAttribution,
}

/// Generic OCB body reference summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BodyRefSummary {
    pub offset: u64,
    pub length: u64,
    pub kind: BodyKind,
    pub flags: u16,
    pub checksum_kind: ChecksumKind,
    pub checksum: u32,
}

/// Generic OCB column chunk summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnChunkSummary {
    pub row_group_id: u32,
    pub column_id: u32,
    pub column_name: String,
    pub physical_type: PhysicalType,
    pub logical_kind: LogicalKind,
    pub fixed_binary_width: Option<u32>,
    pub codec: ColumnChunkSummaryCodec,
    pub row_count: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub value_ref: BodyRefSummary,
    pub validity_ref: Option<BodyRefSummary>,
}

/// Generic OCB scalar stats summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnStatsSummary {
    pub row_group_id: u32,
    pub column_id: u32,
    pub column_name: String,
    pub physical_type: PhysicalType,
    pub null_count: u32,
    pub min: PredicateValue,
    pub max: PredicateValue,
}

/// Generic OCB row-group summary.
#[derive(Debug, Clone, PartialEq)]
pub struct RowGroupSummary {
    pub row_group_id: u32,
    pub base_row: u64,
    pub row_count: u64,
    pub first_key_tuple_ref: Option<BodyRefSummary>,
    pub last_key_tuple_ref: Option<BodyRefSummary>,
    pub chunks: Vec<ColumnChunkSummary>,
    pub stats: Vec<ColumnStatsSummary>,
}

/// Visitor return control for bounded OCB reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitControl {
    /// Continue visiting batches.
    Continue,
    /// Stop after the current batch.
    Stop,
}

/// Options for bounded visitor-style OCB reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadCursorOptions {
    /// Maximum decoded row-group batches in flight.
    pub max_in_flight_row_groups: usize,
    /// Preserve deterministic row-group order. Unordered mode is reserved.
    pub ordered: bool,
}

impl Default for ReadCursorOptions {
    fn default() -> Self {
        Self {
            max_in_flight_row_groups: 1,
            ordered: true,
        }
    }
}

/// Report for visitor-style OCB reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadCursorReport {
    pub base_report: ReadReport,
    pub batches_yielded: usize,
    pub rows_yielded: u64,
    pub cancelled: bool,
}

/// Options for a pull-driven bounded parallel OCB read session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelReadOptions {
    /// Maximum launched row groups not yet retired through ordered delivery.
    pub max_in_flight_row_groups: usize,
}

impl Default for ParallelReadOptions {
    fn default() -> Self {
        Self {
            max_in_flight_row_groups: 1,
        }
    }
}

/// Deterministic context attached to one owned parallel OCB batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParallelReadContext {
    pub selected_row_group_ordinal: usize,
    pub row_group_id: u32,
    pub base_row: u64,
    pub row_end: u64,
    pub row_count: u64,
    pub worker_id: usize,
}

/// One owned batch released in plan order by a parallel read session.
#[derive(Debug, Clone, PartialEq)]
pub struct ParallelReadBatch {
    pub context: ParallelReadContext,
    pub batch: ColumnBatch,
}

/// Outcome of one blocking caller-thread `ParallelReadSession::next` call.
#[derive(Debug, Clone, PartialEq)]
pub enum ParallelReadNext {
    Batch(ParallelReadBatch),
    End,
    Cancelled,
}

/// Per-worker diagnostics for a bounded parallel OCB read session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelReadWorkerReport {
    pub worker_id: usize,
    pub row_groups_completed: usize,
    pub rows_completed: u64,
    pub row_group_read_ns: u64,
    pub caller_prepare_ns: u64,
}

/// Terminal diagnostics for a bounded parallel OCB read session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParallelReadReport {
    pub cursor_report: ReadCursorReport,
    pub attribution: ReadAttribution,
    pub requested_workers: usize,
    pub started_workers: usize,
    pub max_active_workers_observed: usize,
    pub row_groups_queued: usize,
    pub row_groups_completed: usize,
    pub row_groups_ordered_committed: usize,
    pub rows_completed: u64,
    pub rows_ordered_committed: u64,
    pub max_in_flight_row_groups_observed: usize,
    pub max_pending_results_observed: usize,
    pub max_pending_rows_observed: u64,
    pub capacity_wait_count: usize,
    pub capacity_wait_ns: u64,
    pub task_queue_full_wait_count: usize,
    pub task_queue_full_wait_ns: u64,
    pub result_queue_full_wait_count: usize,
    pub result_queue_full_wait_ns: u64,
    pub ordered_frontier_wait_count: usize,
    pub ordered_frontier_wait_ns: u64,
    pub caller_prepare_ns: u64,
    pub ordered_commit_ns: u64,
    pub ordered_terminal_completed: bool,
    pub worker_reports: Vec<ParallelReadWorkerReport>,
}

/// Options for caller-owned single-row-group fill reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadFillOptions {
    /// Validate checksums while reading. Current native OCB reads remain fail-closed.
    pub validate_checksums: bool,
}

impl Default for ReadFillOptions {
    fn default() -> Self {
        Self {
            validate_checksums: true,
        }
    }
}

/// Caller-owned typed storage for one OCB column fill.
#[derive(Debug)]
pub enum ColumnFillBufferMut<'a> {
    I32 {
        name: &'a str,
        values: &'a mut [i32],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    I32ById {
        column_id: u32,
        values: &'a mut [i32],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    I64 {
        name: &'a str,
        values: &'a mut [i64],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    I64ById {
        column_id: u32,
        values: &'a mut [i64],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    F32 {
        name: &'a str,
        values: &'a mut [f32],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    F32ById {
        column_id: u32,
        values: &'a mut [f32],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    F64 {
        name: &'a str,
        values: &'a mut [f64],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    F64ById {
        column_id: u32,
        values: &'a mut [f64],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    FixedBinary {
        name: &'a str,
        width: u32,
        bytes: &'a mut [u8],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
    FixedBinaryById {
        column_id: u32,
        width: u32,
        bytes: &'a mut [u8],
        validity: Option<&'a mut [u8]>,
        allow_nulls: bool,
    },
}

/// Per-column caller-owned fill result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnFillReport {
    pub column_id: u32,
    pub rows_filled: usize,
    pub validity_filled: bool,
}

/// Caller-owned single-row-group fill report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadFillReport {
    pub row_group_id: u32,
    pub base_row: u64,
    pub row_count: u64,
    pub columns: Vec<ColumnFillReport>,
}

/// OCB read attribution diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadAttribution {
    pub plan_ns: u64,
    pub execute_wall_ns: u64,
    pub row_group_read_ns: u64,
    pub read_io_ns: u64,
    pub checksum_ns: u64,
    pub decompression_ns: u64,
    pub primitive_decode_ns: u64,
    pub native_to_c_copy_ns: Option<u64>,
    pub wrapper_copy_ns: Option<u64>,
    pub bytes_read: u64,
    pub compressed_bytes: u64,
    pub uncompressed_bytes: u64,
    pub requested_threads: usize,
    pub effective_threads: usize,
    pub selected_row_groups: usize,
    pub pruned_row_groups: usize,
    pub selected_column_chunks: usize,
    pub fallback_reason: Option<String>,
}

/// OCB read execution report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadReport {
    /// Requested thread count.
    pub requested_threads: usize,
    /// Effective thread count.
    pub effective_threads: usize,
    /// Selected row groups.
    pub selected_row_groups: usize,
    /// Pruned row groups.
    pub pruned_row_groups: usize,
    /// Selected column chunks.
    pub selected_column_chunks: usize,
    /// Stable fallback reason string when present.
    pub fallback_reason: Option<String>,
}

/// One returned OCB row-group batch.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBatch {
    /// File-local row group id.
    pub row_group_id: u32,
    /// Base row offset.
    pub base_row: u64,
    /// Number of rows.
    pub row_count: u64,
    /// Returned columns.
    pub columns: Vec<ColumnArray>,
}

/// One returned OCB column array.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnArray {
    /// File-local column id.
    pub column_id: u32,
    /// Column name.
    pub name: String,
    /// Physical type.
    pub physical_type: PhysicalType,
    /// Logical kind.
    pub logical_kind: LogicalKind,
    /// Dictionary id when present.
    pub dictionary_id: Option<u32>,
    /// Primitive values.
    pub values: PrimitiveValues,
    /// Optional validity bitmap.
    pub validity: Option<ValidityBitmap>,
}

/// Result from orphan-tail cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CleanupResult {
    /// Whether orphan tail bytes were truncated.
    pub truncated: bool,
}

/// Generic diagnostic issue in an OCB report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    /// Stable reason code string.
    pub code: String,
    /// Optional field path for the diagnostic.
    pub field_path: Option<String>,
    /// Human-readable diagnostic message.
    pub message: String,
}

/// Rejected appendable-root candidate observed during maintenance analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootCandidateDiagnostic {
    /// Candidate root slot id when available.
    pub slot_id: Option<u16>,
    /// Candidate root generation when available.
    pub generation: Option<u64>,
    /// Rejection diagnostic.
    pub issue: Issue,
}

/// Read-only OCB maintenance report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MaintenanceReport {
    /// Path copied from the native report.
    pub path: PathBuf,
    /// Selected-snapshot health status.
    pub status: HealthStatus,
    /// Observed file length when available.
    pub file_bytes: Option<u64>,
    /// Selected root generation when available.
    pub selected_root_generation: Option<u64>,
    /// Previous selected root generation when available.
    pub previous_root_generation: Option<u64>,
    /// Selected root slot id when available.
    pub selected_slot_id: Option<u16>,
    /// End offset of the selected root object when available.
    pub selected_root_end_offset: Option<u64>,
    /// End offset of the selected snapshot when available.
    pub selected_snapshot_end_offset: Option<u64>,
    /// Unreachable trailing bytes after the selected snapshot when available.
    pub orphan_tail_bytes: Option<u64>,
    /// Whether explicit orphan-tail cleanup is recommended.
    pub cleanup_recommended: bool,
    /// Whether rejected root candidates were observed.
    pub root_candidate_rejection_observed: bool,
    /// Number of rejected root candidates observed.
    pub rejected_root_candidate_count: usize,
    /// Rejected root-candidate diagnostics.
    pub rejected_root_candidates: Vec<RootCandidateDiagnostic>,
    /// Top-level report issues.
    pub issues: Vec<Issue>,
}

/// Report from explicit orphan-tail cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReport {
    /// Path copied from the native report.
    pub path: PathBuf,
    /// File length before cleanup.
    pub before_file_bytes: u64,
    /// File length after cleanup.
    pub after_file_bytes: u64,
    /// Selected root generation.
    pub selected_root_generation: u64,
    /// Previous selected root generation when available.
    pub previous_root_generation: Option<u64>,
    /// Selected root slot id.
    pub selected_slot_id: u16,
    /// End offset of the selected root object.
    pub selected_root_end_offset: u64,
    /// End offset of the selected snapshot.
    pub selected_snapshot_end_offset: u64,
    /// Orphan-tail bytes before cleanup.
    pub orphan_tail_bytes_before: u64,
    /// Orphan-tail bytes after cleanup.
    pub orphan_tail_bytes_after: u64,
    /// Bytes removed by cleanup.
    pub bytes_removed: u64,
    /// Whether cleanup shortened the file.
    pub truncated: bool,
    /// Top-level report issues.
    pub issues: Vec<Issue>,
}

/// Deterministic OCB declaration fingerprints returned by export-copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotFingerprints {
    /// Fingerprint algorithm label.
    pub algorithm: String,
    /// Schema fingerprint.
    pub schema: String,
    /// Dictionary declarations fingerprint.
    pub dictionaries: String,
    /// Ordering declarations fingerprint.
    pub ordering: String,
    /// Combined declaration fingerprint.
    pub combined: String,
}

/// Provenance report from selected-snapshot export-copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotExportReport {
    /// Source path copied from the native report.
    pub source_path: PathBuf,
    /// Destination path copied from the native report.
    pub destination_path: PathBuf,
    /// Validation depth applied during export.
    pub validation: OpenValidation,
    /// Source file length observed before export.
    pub source_file_bytes: u64,
    /// Destination file length after export.
    pub destination_file_bytes: u64,
    /// Number of selected-snapshot bytes copied.
    pub bytes_copied: u64,
    /// Orphan tail bytes excluded from the destination.
    pub orphan_tail_bytes_excluded: u64,
    /// Selected root generation.
    pub root_generation: u64,
    /// Previous selected root generation when present.
    pub previous_root_generation: Option<u64>,
    /// Selected snapshot row count.
    pub row_count: u64,
    /// Selected snapshot row-group count.
    pub row_group_count: u32,
    /// Declaration fingerprints for the selected snapshot.
    pub fingerprints: SnapshotFingerprints,
}

/// Manifest tool identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestTool {
    pub name: String,
    pub version: String,
    pub generated_at_unix_seconds: u64,
}

/// Optional manifest file digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestDigest {
    pub algorithm: String,
    pub digest: String,
}

/// Deterministic OCB declaration fingerprints recorded in a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestFingerprints {
    pub algorithm: String,
    pub schema: String,
    pub dictionaries: String,
    pub ordering: String,
    pub combined: String,
}

/// Manifest validation issue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestIssue {
    pub code: String,
    pub field_path: Option<String>,
    pub message: String,
}

/// Per-entry validation summary captured in a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntryValidation {
    pub mode: String,
    pub status: String,
    pub issues: Vec<ManifestIssue>,
}

/// One selected-snapshot file entry in an OCB manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    pub path: String,
    pub uri: Option<String>,
    pub file_bytes: Option<u64>,
    pub digest: Option<ManifestDigest>,
    pub root_generation: u64,
    pub row_count: u64,
    pub row_group_count: u32,
    pub fingerprints: ManifestFingerprints,
    pub validation: ManifestEntryValidation,
}

/// Selected-snapshot OCB manifest with owned Rust values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub schema: String,
    pub generated_by: ManifestTool,
    pub entries: Vec<ManifestEntry>,
}

/// Report from validating a manifest against current selected local snapshots.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestValidationReport {
    pub status: CompatibilityStatus,
    pub validation: OpenValidation,
    pub entries_checked: usize,
    pub issues: Vec<ManifestIssue>,
}

/// OCB file handle bound to one selected committed snapshot.
#[derive(Debug)]
pub struct ColumnBundleFile {
    pub(super) raw: NonNull<sys::ArcadiaTioOcbFile>,
    pub(super) _abi: AbiCompatible,
}

/// Pull-driven owner of Rust-backed bounded OCB read workers.
///
/// `next` calls are serialized internally. `cancel` is idempotent and may
/// run concurrently with a blocked `next`. Successful completion may win
/// that race, so inspect the terminal status and matching report. Dropping
/// an active session cancels, drains, joins, and frees all native state.
#[derive(Debug)]
pub struct ParallelReadSession {
    pub(super) raw: NonNull<sys::ArcadiaTioOcbParallelReadSession>,
    pub(super) _abi: AbiCompatible,
}
