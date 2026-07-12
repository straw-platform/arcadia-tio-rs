#![allow(dead_code)]

//! One-file ordered column bundle facade used by the public `format-ocb` API.
//!
//! This module is deliberately generic. It exposes columnar batches and logical
//! annotations, but it does not attach market-data, order, trade, or replay
//! semantics to columns.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::format::{
    OCB_COLUMN_CHUNK_V1_HEADER_LEN, OCB_NULL_U32, OcbBodyKindV1, OcbBodyRefV2, OcbChecksumKindV1,
    OcbChunkCodecV1, OcbColumnChunkDescV1, OcbColumnChunkObjectV1, OcbColumnStatsV1,
    OcbDictionaryValueKindV1, OcbDictionaryValuesV1, OcbLogicalKindV1, OcbNullOrderV1,
    OcbNullabilityV1, OcbOrderingDirectionV1, OcbPhysicalTypeV1, OcbRowGroupDescV1,
    OcbStatScalarV1, crc32c,
};
use crate::parallel_prepare::{
    ColumnBundleParallelPrepareContext, ColumnBundleParallelPrepareOptions,
    ColumnBundleParallelPrepareReport, ParallelPrepareTaskSpec, execute_parallel_prepare,
};
use crate::read::{
    OcbMetadataV1, OcbOpenValidationMode, OcbReadObjectAttribution, read_column_chunk,
    read_metadata, read_metadata_with_validation, read_object_bytes,
    read_object_bytes_with_attribution,
};
use crate::{ArcadiaTioError, OcbFailureCause, Result};

pub const OCB_FALLBACK_THREAD_CAP_ONE: &str = "thread_cap_one";
pub const OCB_FALLBACK_TOO_FEW_ROW_GROUPS: &str = "too_few_row_groups";

/// Stable fail-closed error message for explicit read-plan subset ids that are not in the plan.
pub const OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR: &str =
    "OCB read plan subset contains a row group id not present in the plan";
/// Stable fail-closed error message for duplicate explicit read-plan subset ids.
pub const OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR: &str =
    "OCB read plan subset contains duplicate row group ids";

/// OCB error taxonomy for public Rust and mapped external surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OcbErrorKind {
    /// Caller input or operation preconditions are invalid.
    InvalidInput,
    /// The file is not a supported OCB file or uses an unsupported OCB revision.
    UnsupportedFormat,
    /// The file appears to be corrupt, torn, truncated, or internally inconsistent.
    CorruptFile,
    /// Manifest or certification schema version is unsupported.
    UnsupportedSchemaVersion,
    /// Manifest JSON or required manifest fields are invalid.
    InvalidManifest,
    /// Manifest-relative artifact path is absolute, empty, traversing, or escapes the root.
    UnsafeManifestPath,
    /// A manifest-listed artifact is missing.
    MissingArtifact,
    /// A fixed-binary payload width differs from the expected compact-L2 width.
    FixedBinaryWidthMismatch,
    /// A fixed-binary payload header failed fail-closed validation.
    PayloadHeaderMismatch,
    /// Payload CRC/checksum validation failed.
    PayloadCrcMismatch,
    /// A channel artifact contains a ChannelID other than the manifest channel.
    ChannelIdMismatch,
    /// A per-channel BizIndex value is duplicated.
    BizIndexDuplicate,
    /// A per-channel BizIndex value has a gap relative to the expected sequence.
    BizIndexGap,
    /// A per-channel BizIndex value regressed below the expected sequence.
    BizIndexRegression,
    /// Observed rows do not match manifest, metadata, or row-group counts.
    RowCountMismatch,
    /// Manifest hash/fingerprint metadata does not match the artifact.
    ChecksumMismatch,
    /// A cooperating OCB mutation lock is already held or unavailable.
    LockUnavailable,
    /// Low-level I/O failure not otherwise classified.
    Io,
}

impl OcbErrorKind {
    pub fn from_error(error: &ArcadiaTioError) -> Option<Self> {
        if let ArcadiaTioError::OcbDiagnostic { kind, .. } = error {
            return Some(*kind);
        }
        if let Some(cause) = error.ocb_failure_cause() {
            return Some(Self::from_cause(cause));
        }
        match error {
            ArcadiaTioError::Io(io) => {
                let message = io.to_string();
                if message.contains("OCB mutation lock") {
                    Some(Self::LockUnavailable)
                } else {
                    Some(Self::Io)
                }
            }
            ArcadiaTioError::InvalidArgument(message) => classify_ocb_invalid_argument(message),
            ArcadiaTioError::Unimplemented(message) => {
                message.contains("OCB").then_some(Self::UnsupportedFormat)
            }
            ArcadiaTioError::Ocb { .. } => None,
            ArcadiaTioError::OcbDiagnostic { .. } => None,
        }
    }

    pub const fn from_cause(cause: OcbFailureCause) -> Self {
        match cause {
            OcbFailureCause::InvalidInput => Self::InvalidInput,
            OcbFailureCause::UnsupportedFormat => Self::UnsupportedFormat,
            OcbFailureCause::CorruptFile => Self::CorruptFile,
            OcbFailureCause::LockUnavailable => Self::LockUnavailable,
            OcbFailureCause::Io => Self::Io,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::UnsupportedFormat => "unsupported_format",
            Self::CorruptFile => "corrupt_file",
            Self::UnsupportedSchemaVersion => "unsupported_schema_version",
            Self::InvalidManifest => "invalid_manifest",
            Self::UnsafeManifestPath => "unsafe_manifest_path",
            Self::MissingArtifact => "missing_artifact",
            Self::FixedBinaryWidthMismatch => "fixed_binary_width_mismatch",
            Self::PayloadHeaderMismatch => "payload_header_mismatch",
            Self::PayloadCrcMismatch => "payload_crc_mismatch",
            Self::ChannelIdMismatch => "channel_id_mismatch",
            Self::BizIndexDuplicate => "biz_index_duplicate",
            Self::BizIndexGap => "biz_index_gap",
            Self::BizIndexRegression => "biz_index_regression",
            Self::RowCountMismatch => "row_count_mismatch",
            Self::ChecksumMismatch => "checksum_mismatch",
            Self::LockUnavailable => "lock_unavailable",
            Self::Io => "io",
        }
    }
}

fn classify_ocb_invalid_argument(message: &str) -> Option<OcbErrorKind> {
    if !message.contains("OCB") {
        return None;
    }
    if message.contains("unsupported")
        || message.contains("invalid OCB bootstrap magic")
        || message.contains("not a TensorFile")
    {
        return Some(OcbErrorKind::UnsupportedFormat);
    }
    if message.contains("crc")
        || message.contains("checksum")
        || message.contains("root selection")
        || message.contains("truncated")
        || message.contains("shorter than bootstrap")
        || message.contains("out of bounds")
        || message.contains("is inconsistent")
    {
        return Some(OcbErrorKind::CorruptFile);
    }
    Some(OcbErrorKind::InvalidInput)
}

/// Physical type for one OCB column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnPhysicalType {
    /// Signed 32-bit integer values.
    I32,
    /// Signed 64-bit integer values.
    I64,
    /// 32-bit floating-point values.
    F32,
    /// 64-bit floating-point values.
    F64,
    /// Fixed-width opaque byte values.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
    },
}

impl ColumnPhysicalType {
    pub(crate) fn ocb_physical_type(self) -> OcbPhysicalTypeV1 {
        match self {
            Self::I32 => OcbPhysicalTypeV1::I32,
            Self::I64 => OcbPhysicalTypeV1::I64,
            Self::F32 => OcbPhysicalTypeV1::F32,
            Self::F64 => OcbPhysicalTypeV1::F64,
            Self::FixedBinary { .. } => OcbPhysicalTypeV1::FixedBinary,
        }
    }

    pub fn fixed_binary_width(self) -> u32 {
        match self {
            Self::FixedBinary { width } => width,
            _ => 0,
        }
    }
}

fn column_physical_type_from_desc(
    column: &crate::format::OcbColumnDescV1,
) -> Result<ColumnPhysicalType> {
    Ok(match column.physical_type {
        OcbPhysicalTypeV1::I32 => {
            if column.fixed_binary_width != 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB primitive column has unexpected fixed-binary width",
                ));
            }
            ColumnPhysicalType::I32
        }
        OcbPhysicalTypeV1::I64 => {
            if column.fixed_binary_width != 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB primitive column has unexpected fixed-binary width",
                ));
            }
            ColumnPhysicalType::I64
        }
        OcbPhysicalTypeV1::F32 => {
            if column.fixed_binary_width != 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB primitive column has unexpected fixed-binary width",
                ));
            }
            ColumnPhysicalType::F32
        }
        OcbPhysicalTypeV1::F64 => {
            if column.fixed_binary_width != 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB primitive column has unexpected fixed-binary width",
                ));
            }
            ColumnPhysicalType::F64
        }
        OcbPhysicalTypeV1::FixedBinary => {
            if column.fixed_binary_width == 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary column requires fixed width",
                ));
            }
            ColumnPhysicalType::FixedBinary {
                width: column.fixed_binary_width,
            }
        }
    })
}

fn scalar_column_physical_type(value: OcbPhysicalTypeV1) -> Result<ColumnPhysicalType> {
    Ok(match value {
        OcbPhysicalTypeV1::I32 => ColumnPhysicalType::I32,
        OcbPhysicalTypeV1::I64 => ColumnPhysicalType::I64,
        OcbPhysicalTypeV1::F32 => ColumnPhysicalType::F32,
        OcbPhysicalTypeV1::F64 => ColumnPhysicalType::F64,
        OcbPhysicalTypeV1::FixedBinary => {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB fixed-binary physical type requires schema width",
            ));
        }
    })
}

/// Generic logical annotation for a physical OCB column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnLogicalKind {
    /// No additional logical annotation beyond the physical type.
    Plain,
    /// Integer values that should be interpreted as timestamp-like nanoseconds.
    TimestampNanosLike,
    /// Integer values scaled by the column's `scale` metadata.
    ScaledInteger,
    /// Integer values are codes into a file-local dictionary.
    DictionaryCode,
    /// Integer values are enum-like codes.
    EnumCode,
    /// Values are opaque stable keys with application-defined meaning.
    OpaqueKey,
}

impl From<OcbLogicalKindV1> for ColumnLogicalKind {
    fn from(value: OcbLogicalKindV1) -> Self {
        match value {
            OcbLogicalKindV1::Plain => Self::Plain,
            OcbLogicalKindV1::TimestampNanosLike => Self::TimestampNanosLike,
            OcbLogicalKindV1::ScaledInteger => Self::ScaledInteger,
            OcbLogicalKindV1::DictionaryCode => Self::DictionaryCode,
            OcbLogicalKindV1::EnumCode => Self::EnumCode,
            OcbLogicalKindV1::OpaqueKey => Self::OpaqueKey,
        }
    }
}

/// One column in an opened bundle schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleColumn {
    /// Stable file-local column id.
    pub id: u32,
    /// UTF-8 column name.
    pub name: String,
    /// Physical primitive representation used on disk and in decoded batches.
    pub physical_type: ColumnPhysicalType,
    /// Logical annotation for consumers that need semantic hints.
    pub logical_kind: ColumnLogicalKind,
    /// File-local dictionary id for dictionary-coded columns.
    pub dictionary_id: Option<u32>,
    /// Scale metadata for scaled-integer logical columns.
    pub scale: i32,
    /// Whether decoded batches may carry a validity bitmap for this column.
    pub nullable: bool,
}

/// File-local dictionary value kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DictionaryValueKind {
    /// Dictionary values are UTF-8 strings.
    Utf8,
    /// Dictionary values are variable-width byte strings.
    Bytes,
    /// Dictionary values are fixed-width byte strings.
    FixedBytes,
    /// Dictionary values are UTF-8 enum labels.
    EnumLabels,
}

impl From<OcbDictionaryValueKindV1> for DictionaryValueKind {
    fn from(value: OcbDictionaryValueKindV1) -> Self {
        match value {
            OcbDictionaryValueKindV1::Utf8 => Self::Utf8,
            OcbDictionaryValueKindV1::Bytes => Self::Bytes,
            OcbDictionaryValueKindV1::FixedBytes => Self::FixedBytes,
            OcbDictionaryValueKindV1::EnumLabels => Self::EnumLabels,
        }
    }
}

/// Decoded cold-path dictionary values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DictionaryValues {
    /// UTF-8 dictionary entries.
    Utf8(Vec<String>),
    /// Variable-width byte dictionary entries.
    Bytes(Vec<Vec<u8>>),
    /// Fixed-width byte dictionary entries plus their declared byte width.
    FixedBytes {
        /// Number of bytes in each entry.
        fixed_width: u32,
        /// Dictionary entry bytes; each value should have `fixed_width` bytes.
        values: Vec<Vec<u8>>,
    },
    /// UTF-8 labels for enum-like code columns.
    EnumLabels(Vec<String>),
}

/// One decoded file-local dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleDictionaryValues {
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// UTF-8 dictionary name.
    pub name: String,
    /// Value representation stored by this dictionary.
    pub value_kind: DictionaryValueKind,
    /// Decoded dictionary values.
    pub values: DictionaryValues,
}

/// File-local dictionary descriptor without decoding dictionary values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleDictionaryDescriptor {
    /// File-local dictionary id.
    pub dictionary_id: u32,
    /// UTF-8 dictionary name.
    pub name: String,
    /// Physical type used by columns that store codes into this dictionary.
    pub code_physical_type: ColumnPhysicalType,
    /// Value representation stored by this dictionary.
    pub value_kind: DictionaryValueKind,
    /// Number of entries in the frozen dictionary.
    pub entry_count: u32,
}

/// Ordering direction for one OCB ordering key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleOrderingDirection {
    /// Ordering key values increase within the append domain.
    Ascending,
    /// Ordering key values decrease within the append domain.
    Descending,
}

impl From<OcbOrderingDirectionV1> for BundleOrderingDirection {
    fn from(value: OcbOrderingDirectionV1) -> Self {
        match value {
            OcbOrderingDirectionV1::Ascending => Self::Ascending,
            OcbOrderingDirectionV1::Descending => Self::Descending,
        }
    }
}

/// Null ordering policy for one OCB ordering key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleNullOrder {
    /// Null values sort before non-null values for this key.
    NullsFirst,
    /// Null values sort after non-null values for this key.
    NullsLast,
    /// This ordering key is declared non-null.
    NoNulls,
}

impl From<OcbNullOrderV1> for BundleNullOrder {
    fn from(value: OcbNullOrderV1) -> Self {
        match value {
            OcbNullOrderV1::NullsFirst => Self::NullsFirst,
            OcbNullOrderV1::NullsLast => Self::NullsLast,
            OcbNullOrderV1::NoNulls => Self::NoNulls,
        }
    }
}

/// One ordering key in the committed OCB ordering declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleOrderingKey {
    /// File-local id of the ordered column.
    pub column_id: u32,
    /// UTF-8 name of the ordered column.
    pub column_name: String,
    /// Sort direction for this key.
    pub direction: BundleOrderingDirection,
    /// Null ordering policy for this key.
    pub null_order: BundleNullOrder,
}

/// Generic OCB body-object kind recorded by a body reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnBundleBodyKind {
    /// Null or unknown body reference kind.
    Unknown,
    /// Root object.
    Root,
    /// Schema object.
    Schema,
    /// Dictionary index object.
    DictionaryIndex,
    /// Dictionary values object.
    DictionaryValues,
    /// Row-group index object.
    RowGroupIndex,
    /// Ordering proof object.
    OrderingProof,
    /// Column chunk object.
    ColumnChunk,
    /// String table object.
    StringTable,
    /// Diagnostic JSON metadata object.
    DebugJsonMetadata,
    /// Validity bitmap object.
    ValidityBitmap,
    /// Ordering key tuple object.
    KeyTuple,
    /// Row-group index delta object.
    RowGroupIndexDelta,
}

impl From<OcbBodyKindV1> for ColumnBundleBodyKind {
    fn from(value: OcbBodyKindV1) -> Self {
        match value {
            OcbBodyKindV1::Unknown => Self::Unknown,
            OcbBodyKindV1::Root => Self::Root,
            OcbBodyKindV1::Schema => Self::Schema,
            OcbBodyKindV1::DictionaryIndex => Self::DictionaryIndex,
            OcbBodyKindV1::DictionaryValues => Self::DictionaryValues,
            OcbBodyKindV1::RowGroupIndex => Self::RowGroupIndex,
            OcbBodyKindV1::OrderingProof => Self::OrderingProof,
            OcbBodyKindV1::ColumnChunk => Self::ColumnChunk,
            OcbBodyKindV1::StringTable => Self::StringTable,
            OcbBodyKindV1::DebugJsonMetadata => Self::DebugJsonMetadata,
            OcbBodyKindV1::ValidityBitmap => Self::ValidityBitmap,
            OcbBodyKindV1::KeyTuple => Self::KeyTuple,
            OcbBodyKindV1::RowGroupIndexDelta => Self::RowGroupIndexDelta,
        }
    }
}

/// Generic checksum kind recorded by an OCB body reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnBundleChecksumKind {
    /// No checksum is recorded.
    None,
    /// CRC32C checksum.
    Crc32c,
}

impl From<OcbChecksumKindV1> for ColumnBundleChecksumKind {
    fn from(value: OcbChecksumKindV1) -> Self {
        match value {
            OcbChecksumKindV1::None => Self::None,
            OcbChecksumKindV1::Crc32c => Self::Crc32c,
        }
    }
}

/// Generic codec recorded by an OCB column chunk descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnBundleColumnChunkSummaryCodec {
    /// Chunk payload is stored uncompressed.
    None,
    /// Chunk payload is stored with zstd compression.
    Zstd,
}

impl From<OcbChunkCodecV1> for ColumnBundleColumnChunkSummaryCodec {
    fn from(value: OcbChunkCodecV1) -> Self {
        match value {
            OcbChunkCodecV1::None => Self::None,
            OcbChunkCodecV1::Zstd => Self::Zstd,
        }
    }
}

/// Public, read-only summary of an OCB body reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleBodyRefSummary {
    /// File offset of the referenced object body.
    pub offset: u64,
    /// Total byte length of the referenced object body.
    pub length: u64,
    /// Generic kind tag recorded by the reference.
    pub kind: ColumnBundleBodyKind,
    /// Generic body-reference flags.
    pub flags: u16,
    /// Checksum algorithm recorded by the reference.
    pub checksum_kind: ColumnBundleChecksumKind,
    /// Checksum value recorded by the reference.
    pub checksum: u32,
}

/// Read-only summary of one projected OCB column chunk descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleColumnChunkSummary {
    /// File-local row-group id owning this chunk.
    pub row_group_id: u32,
    /// File-local column id for this chunk.
    pub column_id: u32,
    /// UTF-8 column name for this chunk.
    pub column_name: String,
    /// Physical primitive/fixed-binary representation for this chunk.
    pub physical_type: ColumnPhysicalType,
    /// Generic logical annotation for this chunk's column.
    pub logical_kind: ColumnLogicalKind,
    /// Opaque fixed-binary width when this is a fixed-binary column.
    pub fixed_binary_width: Option<u32>,
    /// Compression codec recorded by the chunk descriptor.
    pub codec: ColumnBundleColumnChunkSummaryCodec,
    /// Logical row count recorded by the chunk descriptor.
    pub row_count: u64,
    /// Compressed payload byte count derived from the column-chunk object length.
    pub compressed_bytes: u64,
    /// Uncompressed value byte count recorded by the chunk descriptor.
    pub uncompressed_bytes: u64,
    /// Value object reference and checksum metadata.
    pub value_ref: ColumnBundleBodyRefSummary,
    /// Optional validity-bitmap object reference and checksum metadata.
    pub validity_ref: Option<ColumnBundleBodyRefSummary>,
}

/// Read-only scalar statistics summary for one row-group column.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleColumnStatsSummary {
    /// File-local row-group id owning these stats.
    pub row_group_id: u32,
    /// File-local column id for these stats.
    pub column_id: u32,
    /// UTF-8 column name for these stats.
    pub column_name: String,
    /// Physical scalar representation for min/max.
    pub physical_type: ColumnPhysicalType,
    /// Number of null values recorded for this row-group column.
    pub null_count: u32,
    /// Inclusive scalar minimum recorded in row-group metadata.
    pub min: ColumnPredicateValue,
    /// Inclusive scalar maximum recorded in row-group metadata.
    pub max: ColumnPredicateValue,
}

/// Generic read-only summary for one file-local OCB row group.
///
/// This is row-group/chunk metadata only. It is not a row-level filtering or
/// replay certificate API; callers that need domain semantics must map and
/// validate them outside TIO.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleRowGroupSummary {
    /// File-local row-group id.
    pub row_group_id: u32,
    /// Logical starting row for this row group in the selected snapshot.
    pub base_row: u64,
    /// Logical row count for this row group.
    pub row_count: u64,
    /// Optional first ordering-key tuple object reference metadata.
    pub first_key_tuple_ref: Option<ColumnBundleBodyRefSummary>,
    /// Optional last ordering-key tuple object reference metadata.
    pub last_key_tuple_ref: Option<ColumnBundleBodyRefSummary>,
    /// Column chunks included in this summary; plan summaries include only the
    /// plan projection, while whole-file summaries include every chunk.
    pub chunks: Vec<ColumnBundleColumnChunkSummary>,
    /// Scalar min/max stats recorded for this row group.
    pub stats: Vec<ColumnBundleColumnStatsSummary>,
}

/// Stable metadata summary for an opened OCB snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleMetadata {
    /// Public format name; currently always `"OCB"`.
    pub format_name: &'static str,
    /// Whether the selected snapshot belongs to the appendable OCB format.
    pub appendable: bool,
    /// Generation number of the selected committed root.
    pub root_generation: u64,
    /// Previous committed root generation, if this snapshot is not the first.
    pub previous_root_generation: Option<u64>,
    /// Total logical rows visible in this selected snapshot.
    pub row_count: u64,
    /// Number of row groups visible in this selected snapshot.
    pub row_group_count: u32,
    /// Number of column chunks referenced by visible row groups.
    pub column_chunk_count: u32,
    /// Frozen schema columns in file-local order.
    pub columns: Vec<BundleColumn>,
    /// Frozen file-local dictionary descriptors; values decode on request.
    pub dictionaries: Vec<BundleDictionaryDescriptor>,
    /// Frozen ordering declaration used to validate append suffixes.
    pub ordering_keys: Vec<BundleOrderingKey>,
}

/// Deterministic generic fingerprint algorithm for OCB certification summaries.
pub const OCB_CERTIFICATION_FINGERPRINT_ALGORITHM: &str = "ocb.generic.crc32c.v1";

/// Deterministic generic fingerprint over selected-snapshot OCB declarations.
///
/// This is a compatibility/certification aid, not a cryptographic file digest.
/// It normalizes public metadata and row-group/chunk descriptor summaries into
/// CRC32C hex strings without reading column payload bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleSnapshotFingerprint {
    /// Fingerprint algorithm label.
    pub algorithm: &'static str,
    /// Fingerprint over schema column declarations.
    pub schema: String,
    /// Fingerprint over dictionary declarations.
    pub dictionaries: String,
    /// Fingerprint over ordering declarations.
    pub ordering: String,
    /// Fingerprint over row-group/chunk/stat descriptor metadata.
    pub row_groups: String,
    /// Combined fingerprint over all components above.
    pub combined: String,
}

/// Generic certification metadata for one read plan.
///
/// The summary is read-only metadata for fail-closed downstream gates. It does
/// not certify application semantics, row-level filtering, payload equivalence,
/// or production/default runtime readiness by itself.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReadPlanCertification {
    /// Selected-snapshot declaration fingerprint.
    pub snapshot_fingerprint: ColumnBundleSnapshotFingerprint,
    /// OCB file length observed for the selected snapshot.
    pub file_len: u64,
    /// Selected root generation.
    pub root_generation: u64,
    /// Previous root generation, if any.
    pub previous_root_generation: Option<u64>,
    /// Total selected-snapshot row count.
    pub row_count: u64,
    /// Total selected-snapshot row-group count.
    pub row_group_count: u32,
    /// Plan report for the certified selected row groups/projection.
    pub report: ColumnBundleReadReport,
    /// Plan-order row-group summaries restricted to the plan projection.
    pub row_groups: Vec<ColumnBundleRowGroupSummary>,
    /// Sum of selected projected chunk compressed payload bytes.
    pub selected_compressed_bytes: u64,
    /// Sum of selected projected chunk uncompressed payload bytes.
    pub selected_uncompressed_bytes: u64,
    /// Fingerprint over selected projected chunk body refs and checksums.
    pub selected_chunk_fingerprint: String,
}

/// Values for one decoded uncompressed column chunk.
#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveColumnValues {
    /// Signed 32-bit integer column values.
    I32(Vec<i32>),
    /// Signed 64-bit integer column values.
    I64(Vec<i64>),
    /// 32-bit floating-point column values.
    F32(Vec<f32>),
    /// 64-bit floating-point column values.
    F64(Vec<f64>),
    /// Fixed-width opaque byte values stored contiguously row-major.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
        /// Contiguous row-major bytes. Length must equal row_count * width.
        bytes: Vec<u8>,
    },
}

/// Borrowed view over primitive values in caller-owned reusable buffers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrimitiveColumnValuesRef<'a> {
    /// Signed 32-bit integer column values.
    I32(&'a [i32]),
    /// Signed 64-bit integer column values.
    I64(&'a [i64]),
    /// 32-bit floating-point column values.
    F32(&'a [f32]),
    /// 64-bit floating-point column values.
    F64(&'a [f64]),
    /// Fixed-width opaque byte values stored contiguously row-major.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
        /// Contiguous row-major bytes. Length equals row_count * width.
        bytes: &'a [u8],
    },
}

impl<'a> PrimitiveColumnValuesRef<'a> {
    /// Physical type represented by this borrowed value view.
    pub fn physical_type(&self) -> ColumnPhysicalType {
        match self {
            Self::I32(_) => ColumnPhysicalType::I32,
            Self::I64(_) => ColumnPhysicalType::I64,
            Self::F32(_) => ColumnPhysicalType::F32,
            Self::F64(_) => ColumnPhysicalType::F64,
            Self::FixedBinary { width, .. } => ColumnPhysicalType::FixedBinary { width: *width },
        }
    }

    /// Number of logical row values in this view.
    pub fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::FixedBinary { width: 0, .. } => 0,
            Self::FixedBinary { width, bytes } => bytes.len() / *width as usize,
        }
    }

    /// Whether this view contains no row values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Interpret this borrowed value view as fixed-width binary records.
    pub fn fixed_binary_records(self) -> Result<FixedBinaryRecordView<'a>> {
        match self {
            Self::FixedBinary { width, bytes } => FixedBinaryRecordView::new(width, bytes),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary record projection requires a fixed-binary column",
            )),
        }
    }
}

/// Little-endian primitive field type for generic fixed-binary record projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FixedBinaryFieldType {
    /// Unsigned 8-bit integer field.
    U8,
    /// Signed 8-bit integer field.
    I8,
    /// Unsigned 16-bit little-endian integer field.
    U16Le,
    /// Signed 16-bit little-endian integer field.
    I16Le,
    /// Unsigned 32-bit little-endian integer field.
    U32Le,
    /// Signed 32-bit little-endian integer field.
    I32Le,
    /// Unsigned 64-bit little-endian integer field.
    U64Le,
    /// Signed 64-bit little-endian integer field.
    I64Le,
}

impl FixedBinaryFieldType {
    /// Width of this field in bytes.
    pub const fn byte_width(self) -> usize {
        match self {
            Self::U8 | Self::I8 => 1,
            Self::U16Le | Self::I16Le => 2,
            Self::U32Le | Self::I32Le => 4,
            Self::U64Le | Self::I64Le => 8,
        }
    }
}

/// Caller-owned output buffers for fixed-binary record field projection.
#[derive(Debug)]
pub enum FixedBinaryFieldValuesMut<'a> {
    /// Unsigned 8-bit integer output values.
    U8(&'a mut [u8]),
    /// Signed 8-bit integer output values.
    I8(&'a mut [i8]),
    /// Unsigned 16-bit integer output values.
    U16(&'a mut [u16]),
    /// Signed 16-bit integer output values.
    I16(&'a mut [i16]),
    /// Unsigned 32-bit integer output values.
    U32(&'a mut [u32]),
    /// Signed 32-bit integer output values.
    I32(&'a mut [i32]),
    /// Unsigned 64-bit integer output values.
    U64(&'a mut [u64]),
    /// Signed 64-bit integer output values.
    I64(&'a mut [i64]),
}

impl FixedBinaryFieldValuesMut<'_> {
    /// Field type represented by this output buffer.
    pub fn field_type(&self) -> FixedBinaryFieldType {
        match self {
            Self::U8(_) => FixedBinaryFieldType::U8,
            Self::I8(_) => FixedBinaryFieldType::I8,
            Self::U16(_) => FixedBinaryFieldType::U16Le,
            Self::I16(_) => FixedBinaryFieldType::I16Le,
            Self::U32(_) => FixedBinaryFieldType::U32Le,
            Self::I32(_) => FixedBinaryFieldType::I32Le,
            Self::U64(_) => FixedBinaryFieldType::U64Le,
            Self::I64(_) => FixedBinaryFieldType::I64Le,
        }
    }

    /// Number of output values available in this buffer.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::I8(values) => values.len(),
            Self::U16(values) => values.len(),
            Self::I16(values) => values.len(),
            Self::U32(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::U64(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    /// Whether this output buffer has no values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// One fixed-binary field projection into caller-owned output storage.
#[derive(Debug)]
pub struct FixedBinaryFieldProjectionMut<'a> {
    /// Byte offset of the field inside each fixed-width record.
    pub offset: u32,
    /// Caller-owned output storage. The active prefix after projection is the
    /// projected record count returned by [`FixedBinaryRecordView::project_fields`].
    pub values: FixedBinaryFieldValuesMut<'a>,
}

/// Diagnostic report for generic fixed-binary field projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedBinaryProjectionReport {
    /// Number of records projected.
    pub rows_projected: usize,
    /// Number of field projections completed.
    pub fields_projected: usize,
    /// Wall-clock nanoseconds spent in the projection helper.
    pub projection_wall_ns: u64,
}

/// Caller-described fixed-binary field for reusable projection visitors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedBinaryProjectedField {
    /// Optional caller-owned field label for diagnostics and downstream mapping.
    pub name: Option<String>,
    /// Byte offset of the field inside each fixed-width record.
    pub offset: u32,
    /// Little-endian primitive field type to decode.
    pub field_type: FixedBinaryFieldType,
}

impl FixedBinaryProjectedField {
    /// Create an unnamed projected field at a byte offset.
    pub fn new(offset: u32, field_type: FixedBinaryFieldType) -> Self {
        Self {
            name: None,
            offset,
            field_type,
        }
    }

    /// Attach a caller-owned diagnostic/output name to this field.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Generic fixed-binary record projection description.
///
/// The projection names exactly one fixed-binary source column and decodes
/// caller-described little-endian fields into caller-owned reusable buffers. It
/// is intentionally generic: no channel, BizIndex, fixed-ingress, replay,
/// order-book, or market-data semantics are attached to these APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixedBinaryRecordProjection {
    /// Optional file-local source column id.
    pub column_id: Option<u32>,
    /// Optional UTF-8 source column name.
    pub column_name: Option<String>,
    /// Expected fixed byte width for each source record.
    pub expected_width: u32,
    /// Whether a nullable source column/chunk is allowed. Required compact
    /// payload paths should keep this `false` to fail closed before callbacks.
    pub allow_nulls: bool,
    /// Fields to project from every record.
    pub fields: Vec<FixedBinaryProjectedField>,
}

impl FixedBinaryRecordProjection {
    /// Build a projection by source column name.
    pub fn by_column_name(name: impl Into<String>, expected_width: u32) -> Self {
        Self {
            column_id: None,
            column_name: Some(name.into()),
            expected_width,
            allow_nulls: false,
            fields: Vec::new(),
        }
    }

    /// Build a projection by file-local source column id.
    pub fn by_column_id(column_id: u32, expected_width: u32) -> Self {
        Self {
            column_id: Some(column_id),
            column_name: None,
            expected_width,
            allow_nulls: false,
            fields: Vec::new(),
        }
    }

    /// Set whether nullable source chunks are allowed for this projection.
    pub fn allow_nulls(mut self, allow_nulls: bool) -> Self {
        self.allow_nulls = allow_nulls;
        self
    }

    /// Append one projected field.
    pub fn field(mut self, field: FixedBinaryProjectedField) -> Self {
        self.fields.push(field);
        self
    }

    /// Replace the projected field list.
    pub fn fields(mut self, fields: impl Into<Vec<FixedBinaryProjectedField>>) -> Self {
        self.fields = fields.into();
        self
    }
}

/// Borrowed field values from a reusable fixed-binary projection buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FixedBinaryFieldValuesRef<'a> {
    /// Unsigned 8-bit integer values.
    U8(&'a [u8]),
    /// Signed 8-bit integer values.
    I8(&'a [i8]),
    /// Unsigned 16-bit integer values.
    U16(&'a [u16]),
    /// Signed 16-bit integer values.
    I16(&'a [i16]),
    /// Unsigned 32-bit integer values.
    U32(&'a [u32]),
    /// Signed 32-bit integer values.
    I32(&'a [i32]),
    /// Unsigned 64-bit integer values.
    U64(&'a [u64]),
    /// Signed 64-bit integer values.
    I64(&'a [i64]),
}

impl<'a> FixedBinaryFieldValuesRef<'a> {
    /// Little-endian primitive field type represented by this borrowed slice.
    pub fn field_type(&self) -> FixedBinaryFieldType {
        match self {
            Self::U8(_) => FixedBinaryFieldType::U8,
            Self::I8(_) => FixedBinaryFieldType::I8,
            Self::U16(_) => FixedBinaryFieldType::U16Le,
            Self::I16(_) => FixedBinaryFieldType::I16Le,
            Self::U32(_) => FixedBinaryFieldType::U32Le,
            Self::I32(_) => FixedBinaryFieldType::I32Le,
            Self::U64(_) => FixedBinaryFieldType::U64Le,
            Self::I64(_) => FixedBinaryFieldType::I64Le,
        }
    }

    /// Number of decoded values in this field view.
    pub fn len(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::I8(values) => values.len(),
            Self::U16(values) => values.len(),
            Self::I16(values) => values.len(),
            Self::U32(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::U64(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    /// Whether this field view has no decoded values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow this field as `u8` values, or fail closed on a type mismatch.
    pub fn as_u8(&self) -> Result<&'a [u8]> {
        match self {
            Self::U8(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not u8",
            )),
        }
    }

    /// Borrow this field as `i8` values, or fail closed on a type mismatch.
    pub fn as_i8(&self) -> Result<&'a [i8]> {
        match self {
            Self::I8(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not i8",
            )),
        }
    }

    /// Borrow this field as `u16` values, or fail closed on a type mismatch.
    pub fn as_u16(&self) -> Result<&'a [u16]> {
        match self {
            Self::U16(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not u16",
            )),
        }
    }

    /// Borrow this field as `i16` values, or fail closed on a type mismatch.
    pub fn as_i16(&self) -> Result<&'a [i16]> {
        match self {
            Self::I16(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not i16",
            )),
        }
    }

    /// Borrow this field as `u32` values, or fail closed on a type mismatch.
    pub fn as_u32(&self) -> Result<&'a [u32]> {
        match self {
            Self::U32(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not u32",
            )),
        }
    }

    /// Borrow this field as `i32` values, or fail closed on a type mismatch.
    pub fn as_i32(&self) -> Result<&'a [i32]> {
        match self {
            Self::I32(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not i32",
            )),
        }
    }

    /// Borrow this field as `u64` values, or fail closed on a type mismatch.
    pub fn as_u64(&self) -> Result<&'a [u64]> {
        match self {
            Self::U64(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not u64",
            )),
        }
    }

    /// Borrow this field as `i64` values, or fail closed on a type mismatch.
    pub fn as_i64(&self) -> Result<&'a [i64]> {
        match self {
            Self::I64(values) => Ok(values),
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field is not i64",
            )),
        }
    }
}

/// Owned reusable storage for one projected fixed-binary field.
#[derive(Debug, Clone, PartialEq)]
pub enum ReusableFixedBinaryFieldValues {
    /// Unsigned 8-bit integer output values.
    U8(Vec<u8>),
    /// Signed 8-bit integer output values.
    I8(Vec<i8>),
    /// Unsigned 16-bit integer output values.
    U16(Vec<u16>),
    /// Signed 16-bit integer output values.
    I16(Vec<i16>),
    /// Unsigned 32-bit integer output values.
    U32(Vec<u32>),
    /// Signed 32-bit integer output values.
    I32(Vec<i32>),
    /// Unsigned 64-bit integer output values.
    U64(Vec<u64>),
    /// Signed 64-bit integer output values.
    I64(Vec<i64>),
}

impl ReusableFixedBinaryFieldValues {
    fn new(field_type: FixedBinaryFieldType, capacity: usize) -> Self {
        match field_type {
            FixedBinaryFieldType::U8 => Self::U8(vec![0; capacity]),
            FixedBinaryFieldType::I8 => Self::I8(vec![0; capacity]),
            FixedBinaryFieldType::U16Le => Self::U16(vec![0; capacity]),
            FixedBinaryFieldType::I16Le => Self::I16(vec![0; capacity]),
            FixedBinaryFieldType::U32Le => Self::U32(vec![0; capacity]),
            FixedBinaryFieldType::I32Le => Self::I32(vec![0; capacity]),
            FixedBinaryFieldType::U64Le => Self::U64(vec![0; capacity]),
            FixedBinaryFieldType::I64Le => Self::I64(vec![0; capacity]),
        }
    }

    fn field_type(&self) -> FixedBinaryFieldType {
        match self {
            Self::U8(_) => FixedBinaryFieldType::U8,
            Self::I8(_) => FixedBinaryFieldType::I8,
            Self::U16(_) => FixedBinaryFieldType::U16Le,
            Self::I16(_) => FixedBinaryFieldType::I16Le,
            Self::U32(_) => FixedBinaryFieldType::U32Le,
            Self::I32(_) => FixedBinaryFieldType::I32Le,
            Self::U64(_) => FixedBinaryFieldType::U64Le,
            Self::I64(_) => FixedBinaryFieldType::I64Le,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::I8(values) => values.len(),
            Self::U16(values) => values.len(),
            Self::I16(values) => values.len(),
            Self::U32(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::U64(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    fn values_mut(&mut self) -> FixedBinaryFieldValuesMut<'_> {
        match self {
            Self::U8(values) => FixedBinaryFieldValuesMut::U8(values.as_mut_slice()),
            Self::I8(values) => FixedBinaryFieldValuesMut::I8(values.as_mut_slice()),
            Self::U16(values) => FixedBinaryFieldValuesMut::U16(values.as_mut_slice()),
            Self::I16(values) => FixedBinaryFieldValuesMut::I16(values.as_mut_slice()),
            Self::U32(values) => FixedBinaryFieldValuesMut::U32(values.as_mut_slice()),
            Self::I32(values) => FixedBinaryFieldValuesMut::I32(values.as_mut_slice()),
            Self::U64(values) => FixedBinaryFieldValuesMut::U64(values.as_mut_slice()),
            Self::I64(values) => FixedBinaryFieldValuesMut::I64(values.as_mut_slice()),
        }
    }

    fn values_ref(&self, row_count: usize) -> Result<FixedBinaryFieldValuesRef<'_>> {
        if self.len() < row_count {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection buffer is too small for row group",
            ));
        }
        Ok(match self {
            Self::U8(values) => FixedBinaryFieldValuesRef::U8(&values[..row_count]),
            Self::I8(values) => FixedBinaryFieldValuesRef::I8(&values[..row_count]),
            Self::U16(values) => FixedBinaryFieldValuesRef::U16(&values[..row_count]),
            Self::I16(values) => FixedBinaryFieldValuesRef::I16(&values[..row_count]),
            Self::U32(values) => FixedBinaryFieldValuesRef::U32(&values[..row_count]),
            Self::I32(values) => FixedBinaryFieldValuesRef::I32(&values[..row_count]),
            Self::U64(values) => FixedBinaryFieldValuesRef::U64(&values[..row_count]),
            Self::I64(values) => FixedBinaryFieldValuesRef::I64(&values[..row_count]),
        })
    }
}

/// One reusable decoded field in a fixed-binary projection buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleFixedBinaryProjectedFieldBuffer {
    /// Optional caller-owned field label.
    pub name: Option<String>,
    /// Byte offset of the field inside each fixed-width record.
    pub offset: u32,
    /// Decoded reusable values.
    pub values: ReusableFixedBinaryFieldValues,
}

/// Reusable caller-owned fixed-binary projection buffer.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleFixedBinaryProjectionBuffer {
    /// Source column id validated against the read plan.
    pub source_column_id: u32,
    /// Source column name validated against the read plan.
    pub source_column_name: String,
    /// Expected source record width in bytes.
    pub source_width: u32,
    /// Decoded field buffers reused for each callback.
    pub fields: Vec<ColumnBundleFixedBinaryProjectedFieldBuffer>,
    row_count: usize,
}

impl ColumnBundleFixedBinaryProjectionBuffer {
    fn for_projection(
        source_column: &BundleColumn,
        projection: &FixedBinaryRecordProjection,
        capacity: usize,
    ) -> Result<Self> {
        let fields = projection
            .fields
            .iter()
            .map(|field| ColumnBundleFixedBinaryProjectedFieldBuffer {
                name: field.name.clone(),
                offset: field.offset,
                values: ReusableFixedBinaryFieldValues::new(field.field_type, capacity),
            })
            .collect();
        Ok(Self {
            source_column_id: source_column.id,
            source_column_name: source_column.name.clone(),
            source_width: projection.expected_width,
            fields,
            row_count: 0,
        })
    }

    fn project_records(
        &mut self,
        records: FixedBinaryRecordView<'_>,
        projection: &FixedBinaryRecordProjection,
    ) -> Result<FixedBinaryProjectionReport> {
        let started = Instant::now();
        let rows = records.len();
        if self.source_width != projection.expected_width
            || records.width != projection.expected_width
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection source width does not match projection",
            ));
        }
        if self.fields.len() != projection.fields.len() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection buffer field count does not match projection",
            ));
        }
        for (spec, field) in projection.fields.iter().zip(self.fields.iter_mut()) {
            if field.offset != spec.offset || field.values.field_type() != spec.field_type {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary projection buffer field does not match projection",
                ));
            }
            let values = field.values.values_mut();
            let mut field_projection = FixedBinaryFieldProjectionMut {
                offset: spec.offset,
                values,
            };
            records.project_fields_inner(std::slice::from_mut(&mut field_projection))?;
        }
        self.row_count = rows;
        Ok(FixedBinaryProjectionReport {
            rows_projected: rows,
            fields_projected: projection.fields.len(),
            projection_wall_ns: duration_to_ns(started.elapsed()),
        })
    }

    fn view(&self, row_group_id: u32, base_row: u64) -> FixedBinaryProjectedBatchView<'_> {
        FixedBinaryProjectedBatchView {
            row_group_id,
            base_row,
            row_count: self.row_count as u64,
            source_column_id: self.source_column_id,
            source_column_name: &self.source_column_name,
            source_width: self.source_width,
            fields: &self.fields,
        }
    }
}

/// Borrowed view of one projected fixed-binary field.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedBinaryProjectedFieldView<'a> {
    /// Optional caller-owned field label.
    pub name: Option<&'a str>,
    /// Byte offset of the field inside each fixed-width record.
    pub offset: u32,
    /// Little-endian primitive field type.
    pub field_type: FixedBinaryFieldType,
    /// Borrowed decoded field values valid only for the callback duration.
    pub values: FixedBinaryFieldValuesRef<'a>,
}

/// Borrowed projected fixed-binary batch view.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FixedBinaryProjectedBatchView<'a> {
    /// File-local row-group id.
    pub row_group_id: u32,
    /// Logical starting row for this row group in the selected snapshot.
    pub base_row: u64,
    /// Number of rows projected.
    pub row_count: u64,
    /// File-local source column id.
    pub source_column_id: u32,
    /// Source column name.
    pub source_column_name: &'a str,
    /// Fixed byte width of each source record.
    pub source_width: u32,
    fields: &'a [ColumnBundleFixedBinaryProjectedFieldBuffer],
}

impl FixedBinaryProjectedBatchView<'_> {
    /// Number of projected fields.
    pub fn field_count(&self) -> usize {
        self.fields.len()
    }

    /// Borrow one projected field by its caller-supplied name.
    pub fn field_by_name(&self, name: &str) -> Result<FixedBinaryProjectedFieldView<'_>> {
        let mut matched = None;
        for (index, field) in self.fields.iter().enumerate() {
            if field.name.as_deref() == Some(name) {
                if matched.is_some() {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB fixed-binary projected field name is ambiguous",
                    ));
                }
                matched = Some(index);
            }
        }
        let index = matched.ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projected field name is unknown",
        ))?;
        self.field(index)
    }

    /// Borrow one projected field by projection index.
    pub fn field(&self, index: usize) -> Result<FixedBinaryProjectedFieldView<'_>> {
        let field = self
            .fields
            .get(index)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected field index is out of bounds",
            ))?;
        let row_count = usize::try_from(self.row_count).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projected row count does not fit usize",
            )
        })?;
        let values = field.values.values_ref(row_count)?;
        Ok(FixedBinaryProjectedFieldView {
            name: field.name.as_deref(),
            offset: field.offset,
            field_type: field.values.field_type(),
            values,
        })
    }
}

/// Borrowed fixed-width record view over one fixed-binary column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedBinaryRecordView<'a> {
    /// Fixed byte width of each logical record.
    pub width: u32,
    /// Contiguous row-major record bytes. Length is `len() * width`.
    pub bytes: &'a [u8],
}

impl<'a> FixedBinaryRecordView<'a> {
    /// Create a fixed-width record view, failing closed on zero width or
    /// unaligned byte length.
    pub fn new(width: u32, bytes: &'a [u8]) -> Result<Self> {
        let width_usize = usize::try_from(width).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB fixed-binary record width does not fit usize")
        })?;
        if width_usize == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary record width must be greater than zero",
            ));
        }
        if !bytes.len().is_multiple_of(width_usize) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary record bytes are not aligned to record width",
            ));
        }
        Ok(Self { width, bytes })
    }

    /// Number of fixed-width records in this view.
    pub fn len(&self) -> usize {
        self.bytes.len() / self.width as usize
    }

    /// Whether this view has no records.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Borrow one record by row index.
    pub fn row(&self, row: usize) -> Result<&'a [u8]> {
        let width = self.width as usize;
        let start = row
            .checked_mul(width)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary row offset overflows",
            ))?;
        let end = start
            .checked_add(width)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary row end offset overflows",
            ))?;
        self.bytes
            .get(start..end)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary row index is out of bounds",
            ))
    }

    /// Project little-endian fields from every record into caller-owned typed
    /// output buffers.
    ///
    /// This is a generic fixed-width binary helper: it knows only byte offsets
    /// and primitive little-endian field widths. It does not add market-data,
    /// channel, BizIndex, replay, or fixed-ingress semantics to OCB.
    pub fn project_fields(
        &self,
        fields: &mut [FixedBinaryFieldProjectionMut<'_>],
    ) -> Result<usize> {
        self.project_fields_inner(fields)
    }

    /// Project little-endian fields and return a small projection attribution report.
    pub fn project_fields_with_report(
        &self,
        fields: &mut [FixedBinaryFieldProjectionMut<'_>],
    ) -> Result<FixedBinaryProjectionReport> {
        let started = Instant::now();
        let fields_projected = fields.len();
        let rows_projected = self.project_fields_inner(fields)?;
        Ok(FixedBinaryProjectionReport {
            rows_projected,
            fields_projected,
            projection_wall_ns: duration_to_ns(started.elapsed()),
        })
    }

    fn project_fields_inner(
        &self,
        fields: &mut [FixedBinaryFieldProjectionMut<'_>],
    ) -> Result<usize> {
        let rows = self.len();
        let width = self.width as usize;
        for field in fields {
            let field_type = field.values.field_type();
            let offset = usize::try_from(field.offset).map_err(|_| {
                ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary field offset does not fit usize",
                )
            })?;
            let end = offset.checked_add(field_type.byte_width()).ok_or(
                ArcadiaTioError::ocb_invalid_input("OCB fixed-binary field end offset overflows"),
            )?;
            if end > width {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary field extends past record width",
                ));
            }
            if field.values.len() < rows {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary field output buffer is too small",
                ));
            }
            project_fixed_binary_field(self.bytes, width, offset, rows, &mut field.values)?;
        }
        Ok(rows)
    }
}

fn project_fixed_binary_field(
    bytes: &[u8],
    width: usize,
    offset: usize,
    rows: usize,
    values: &mut FixedBinaryFieldValuesMut<'_>,
) -> Result<()> {
    match values {
        FixedBinaryFieldValuesMut::U8(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = row[offset];
            }
        }
        FixedBinaryFieldValuesMut::I8(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = i8::from_le_bytes([row[offset]]);
            }
        }
        FixedBinaryFieldValuesMut::U16(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = u16::from_le_bytes([row[offset], row[offset + 1]]);
            }
        }
        FixedBinaryFieldValuesMut::I16(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = i16::from_le_bytes([row[offset], row[offset + 1]]);
            }
        }
        FixedBinaryFieldValuesMut::U32(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = u32::from_le_bytes([
                    row[offset],
                    row[offset + 1],
                    row[offset + 2],
                    row[offset + 3],
                ]);
            }
        }
        FixedBinaryFieldValuesMut::I32(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = i32::from_le_bytes([
                    row[offset],
                    row[offset + 1],
                    row[offset + 2],
                    row[offset + 3],
                ]);
            }
        }
        FixedBinaryFieldValuesMut::U64(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = u64::from_le_bytes([
                    row[offset],
                    row[offset + 1],
                    row[offset + 2],
                    row[offset + 3],
                    row[offset + 4],
                    row[offset + 5],
                    row[offset + 6],
                    row[offset + 7],
                ]);
            }
        }
        FixedBinaryFieldValuesMut::I64(out) => {
            for (dst, row) in out[..rows].iter_mut().zip(bytes.chunks_exact(width)) {
                *dst = i64::from_le_bytes([
                    row[offset],
                    row[offset + 1],
                    row[offset + 2],
                    row[offset + 3],
                    row[offset + 4],
                    row[offset + 5],
                    row[offset + 6],
                    row[offset + 7],
                ]);
            }
        }
    }
    Ok(())
}

/// Owned reusable primitive storage for lower-copy visitor reads.
#[derive(Debug, Clone, PartialEq)]
pub enum ReusablePrimitiveColumnValues {
    /// Signed 32-bit integer storage.
    I32(Vec<i32>),
    /// Signed 64-bit integer storage.
    I64(Vec<i64>),
    /// 32-bit floating-point storage.
    F32(Vec<f32>),
    /// 64-bit floating-point storage.
    F64(Vec<f64>),
    /// Fixed-width opaque byte storage, contiguous row-major.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
        /// Reused byte storage. The active prefix is row_count * width.
        bytes: Vec<u8>,
    },
}

impl ReusablePrimitiveColumnValues {
    /// Create empty reusable storage for one physical type.
    pub fn new(physical_type: ColumnPhysicalType) -> Self {
        match physical_type {
            ColumnPhysicalType::I32 => Self::I32(Vec::new()),
            ColumnPhysicalType::I64 => Self::I64(Vec::new()),
            ColumnPhysicalType::F32 => Self::F32(Vec::new()),
            ColumnPhysicalType::F64 => Self::F64(Vec::new()),
            ColumnPhysicalType::FixedBinary { width } => Self::FixedBinary {
                width,
                bytes: Vec::new(),
            },
        }
    }

    /// Physical type represented by this reusable storage.
    pub fn physical_type(&self) -> ColumnPhysicalType {
        match self {
            Self::I32(_) => ColumnPhysicalType::I32,
            Self::I64(_) => ColumnPhysicalType::I64,
            Self::F32(_) => ColumnPhysicalType::F32,
            Self::F64(_) => ColumnPhysicalType::F64,
            Self::FixedBinary { width, .. } => ColumnPhysicalType::FixedBinary { width: *width },
        }
    }

    fn resize_for_rows(&mut self, row_count: usize) -> Result<()> {
        match self {
            Self::I32(values) => values.resize(row_count, 0),
            Self::I64(values) => values.resize(row_count, 0),
            Self::F32(values) => values.resize(row_count, 0.0),
            Self::F64(values) => values.resize(row_count, 0.0),
            Self::FixedBinary { width: 0, .. } => {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable fixed-binary buffer requires width > 0",
                ));
            }
            Self::FixedBinary { width, bytes } => {
                let byte_count = row_count.checked_mul(*width as usize).ok_or(
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB reusable fixed-binary byte count overflows",
                    ),
                )?;
                bytes.resize(byte_count, 0);
            }
        }
        Ok(())
    }

    fn as_mut_values(&mut self) -> PrimitiveColumnValuesMut<'_> {
        match self {
            Self::I32(values) => PrimitiveColumnValuesMut::I32(values.as_mut_slice()),
            Self::I64(values) => PrimitiveColumnValuesMut::I64(values.as_mut_slice()),
            Self::F32(values) => PrimitiveColumnValuesMut::F32(values.as_mut_slice()),
            Self::F64(values) => PrimitiveColumnValuesMut::F64(values.as_mut_slice()),
            Self::FixedBinary { width, bytes } => PrimitiveColumnValuesMut::FixedBinary {
                width: *width,
                bytes: bytes.as_mut_slice(),
            },
        }
    }

    fn as_ref_values(&self, rows: usize) -> Result<PrimitiveColumnValuesRef<'_>> {
        match self {
            Self::I32(values) => values.get(..rows).map(PrimitiveColumnValuesRef::I32).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable i32 buffer is too small for filled rows",
                ),
            ),
            Self::I64(values) => values.get(..rows).map(PrimitiveColumnValuesRef::I64).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable i64 buffer is too small for filled rows",
                ),
            ),
            Self::F32(values) => values.get(..rows).map(PrimitiveColumnValuesRef::F32).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable f32 buffer is too small for filled rows",
                ),
            ),
            Self::F64(values) => values.get(..rows).map(PrimitiveColumnValuesRef::F64).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable f64 buffer is too small for filled rows",
                ),
            ),
            Self::FixedBinary { width: 0, .. } => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable fixed-binary buffer requires width > 0",
            )),
            Self::FixedBinary { width, bytes } => {
                let byte_count =
                    rows.checked_mul(*width as usize)
                        .ok_or(ArcadiaTioError::ocb_invalid_input(
                            "OCB reusable fixed-binary byte count overflows",
                        ))?;
                let bytes = bytes
                    .get(..byte_count)
                    .ok_or(ArcadiaTioError::ocb_invalid_input(
                        "OCB reusable fixed-binary buffer is too small for filled rows",
                    ))?;
                Ok(PrimitiveColumnValuesRef::FixedBinary {
                    width: *width,
                    bytes,
                })
            }
        }
    }
}

/// Borrowed validity bitmap view into caller-owned reusable buffers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidityBitmapRef<'a> {
    /// Number of meaningful row-validity bits.
    pub row_count: u64,
    /// LSB-first validity bytes; bit value `1` means valid and `0` means null.
    pub bytes: &'a [u8],
}

impl ValidityBitmapRef<'_> {
    pub fn is_valid(&self, row: u64) -> Result<bool> {
        if row >= self.row_count {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB validity bitmap row index is out of bounds",
            ));
        }
        let byte = self
            .bytes
            .get((row / 8) as usize)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB validity bitmap storage is too small for row index",
            ))?;
        Ok((byte & (1 << (row % 8))) != 0)
    }
}

/// One reusable caller-owned column buffer used by lower-copy visitor reads.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReusableColumnBuffer {
    /// Resolved file-local column id.
    pub column_id: u32,
    /// UTF-8 column name.
    pub name: String,
    /// Physical primitive type of `values`.
    pub physical_type: ColumnPhysicalType,
    /// Logical annotation for the decoded values.
    pub logical_kind: ColumnLogicalKind,
    /// File-local dictionary id for dictionary-coded columns.
    pub dictionary_id: Option<u32>,
    /// Whether the schema allows this column to carry validity bitmaps.
    pub nullable: bool,
    /// Reused caller-owned value storage.
    pub values: ReusablePrimitiveColumnValues,
    /// Reused caller-owned validity storage. The active prefix is row_count.div_ceil(8).
    pub validity_bytes: Vec<u8>,
    /// Whether nullable chunks are accepted for this buffer.
    pub allow_nulls: bool,
}

impl ColumnBundleReusableColumnBuffer {
    fn for_column(column: &BundleColumn, row_capacity: usize, allow_nulls: bool) -> Result<Self> {
        let mut values = ReusablePrimitiveColumnValues::new(column.physical_type);
        values.resize_for_rows(row_capacity)?;
        let validity_capacity = if allow_nulls {
            row_capacity.div_ceil(8)
        } else {
            0
        };
        Ok(Self {
            column_id: column.id,
            name: column.name.clone(),
            physical_type: column.physical_type,
            logical_kind: column.logical_kind,
            dictionary_id: column.dictionary_id,
            nullable: column.nullable,
            values,
            validity_bytes: vec![0; validity_capacity],
            allow_nulls,
        })
    }

    fn prepare_for_rows(&mut self, row_count: usize) -> Result<()> {
        self.values.resize_for_rows(row_count)?;
        if self.allow_nulls {
            self.validity_bytes.resize(row_count.div_ceil(8), 0);
        } else {
            self.validity_bytes.clear();
        }
        Ok(())
    }

    fn as_fill_buffer(&mut self) -> ColumnBundleColumnFillBuffer<'_> {
        ColumnBundleColumnFillBuffer {
            column_name: Some(self.name.as_str()),
            column_id: Some(self.column_id),
            values: self.values.as_mut_values(),
            validity_bytes: self
                .allow_nulls
                .then_some(self.validity_bytes.as_mut_slice()),
            allow_nulls: self.allow_nulls,
        }
    }

    fn view(
        &self,
        report: &ColumnBundleColumnFillReport,
    ) -> Result<ColumnBundleReusableColumnView<'_>> {
        if report.column_id != self.column_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB reusable column report does not match buffer column",
            ));
        }
        let rows = report.rows_filled;
        let values = self.values.as_ref_values(rows)?;
        let validity = if report.validity_filled {
            let validity_bytes = rows.div_ceil(8);
            let bytes = self.validity_bytes.get(..validity_bytes).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable validity buffer is too small for filled rows",
                ),
            )?;
            Some(ValidityBitmapRef {
                row_count: rows as u64,
                bytes,
            })
        } else {
            None
        };
        Ok(ColumnBundleReusableColumnView {
            column_id: self.column_id,
            name: self.name.as_str(),
            physical_type: self.physical_type,
            logical_kind: self.logical_kind,
            dictionary_id: self.dictionary_id,
            values,
            validity,
        })
    }
}

/// Reusable caller-owned buffers for one in-flight row-group batch.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReusableBuffers {
    /// Projected column buffers in plan projection order.
    pub columns: Vec<ColumnBundleReusableColumnBuffer>,
}

impl ColumnBundleReusableBuffers {
    /// Number of reusable column buffers.
    pub fn len(&self) -> usize {
        self.columns.len()
    }

    /// Whether this reusable batch buffer has no columns.
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty()
    }

    fn for_columns(
        columns: &[BundleColumn],
        row_capacity: usize,
        allow_nulls: bool,
    ) -> Result<Self> {
        let columns = columns
            .iter()
            .map(|column| {
                ColumnBundleReusableColumnBuffer::for_column(column, row_capacity, allow_nulls)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { columns })
    }

    fn prepare_for_rows(&mut self, row_count: usize) -> Result<()> {
        for column in &mut self.columns {
            column.prepare_for_rows(row_count)?;
        }
        Ok(())
    }

    fn fill_buffers(&mut self) -> Vec<ColumnBundleColumnFillBuffer<'_>> {
        self.columns
            .iter_mut()
            .map(ColumnBundleReusableColumnBuffer::as_fill_buffer)
            .collect()
    }
}

/// Caller-owned reusable buffer pool for bounded lower-copy visitor reads.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReusableBufferPool {
    /// One reusable batch buffer per possible in-flight row group.
    pub buffers: Vec<ColumnBundleReusableBuffers>,
}

impl ColumnBundleReusableBufferPool {
    /// Number of reusable in-flight batch buffers in this pool.
    pub fn len(&self) -> usize {
        self.buffers.len()
    }

    /// Whether this pool has no reusable batch buffers.
    pub fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

/// Borrowed view of one reusable column buffer after a fill read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ColumnBundleReusableColumnView<'a> {
    /// File-local column id.
    pub column_id: u32,
    /// UTF-8 column name.
    pub name: &'a str,
    /// Physical primitive type of `values`.
    pub physical_type: ColumnPhysicalType,
    /// Logical annotation for the decoded values.
    pub logical_kind: ColumnLogicalKind,
    /// File-local dictionary id for dictionary-coded columns.
    pub dictionary_id: Option<u32>,
    /// Borrowed primitive values valid only for the visitor callback.
    pub values: PrimitiveColumnValuesRef<'a>,
    /// Optional LSB-first validity bitmap; `None` means all rows are valid.
    pub validity: Option<ValidityBitmapRef<'a>>,
}

/// Borrowed view of one row-group batch in reusable caller-owned buffers.
pub struct ColumnBundleReusableBatchView<'a> {
    report: &'a ColumnBundleReadFillReport,
    buffers: &'a ColumnBundleReusableBuffers,
}

impl ColumnBundleReusableBatchView<'_> {
    /// File-local row-group id.
    pub fn row_group_id(&self) -> u32 {
        self.report.row_group_id
    }

    /// Logical starting row for this row group in the selected snapshot.
    pub fn base_row(&self) -> u64 {
        self.report.base_row
    }

    /// Number of rows in this row-group batch.
    pub fn row_count(&self) -> u64 {
        self.report.row_count
    }

    /// Number of projected columns in this view.
    pub fn column_count(&self) -> usize {
        self.report.columns.len()
    }

    /// Borrow one projected column view by projection index.
    pub fn column(&self, index: usize) -> Result<ColumnBundleReusableColumnView<'_>> {
        let buffer = self
            .buffers
            .columns
            .get(index)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable view column index is out of bounds",
            ))?;
        let report = self
            .report
            .columns
            .get(index)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable view column report is missing",
            ))?;
        buffer.view(report)
    }
}

/// Optional validity bitmap for nullable column chunks.
///
/// `None` on [`ColumnArray::validity`] means every value in the chunk is valid.
/// When present, bit `i` is set if row `i` is valid. Bits are stored
/// least-significant-bit first within each byte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidityBitmap {
    /// Number of meaningful row-validity bits.
    pub row_count: u64,
    /// LSB-first validity bytes; bit value `1` means valid and `0` means null.
    pub bytes: Vec<u8>,
}

impl ValidityBitmap {
    pub fn is_valid(&self, row: u64) -> Result<bool> {
        if row >= self.row_count {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB validity bitmap row index is out of bounds",
            ));
        }
        let byte = self.bytes[(row / 8) as usize];
        Ok((byte & (1 << (row % 8))) != 0)
    }
}

/// One selected column returned in a row-group batch.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnArray {
    /// File-local column id.
    pub column_id: u32,
    /// UTF-8 column name.
    pub name: String,
    /// Physical primitive type of `values`.
    pub physical_type: ColumnPhysicalType,
    /// Logical annotation for the decoded values.
    pub logical_kind: ColumnLogicalKind,
    /// File-local dictionary id for dictionary-coded columns.
    pub dictionary_id: Option<u32>,
    /// Decoded primitive values for this column chunk.
    pub values: PrimitiveColumnValues,
    /// Optional LSB-first validity bitmap; `None` means all rows are valid.
    pub validity: Option<ValidityBitmap>,
}

/// One row-group batch returned by the bundle reader.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBatch {
    /// File-local row-group id.
    pub row_group_id: u32,
    /// Logical starting row for this row group in the selected snapshot.
    pub base_row: u64,
    /// Number of rows in this row-group batch.
    pub row_count: u64,
    /// Projected columns decoded for this row group.
    pub columns: Vec<ColumnArray>,
}

/// Projection for bundle reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ColumnProjection {
    /// Read every column in schema order.
    All,
    /// Read only the named columns, preserving request order after validation.
    Names(Vec<String>),
}

impl ColumnProjection {
    pub fn names<I, S>(names: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self::Names(names.into_iter().map(Into::into).collect())
    }
}

/// Predicate/stat scalar for row-group pruning.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColumnPredicateValue {
    /// Signed 32-bit integer predicate bound.
    I32(i32),
    /// Signed 64-bit integer predicate bound.
    I64(i64),
    /// 32-bit floating-point predicate bound; NaN is rejected.
    F32(f32),
    /// 64-bit floating-point predicate bound; NaN is rejected.
    F64(f64),
}

impl ColumnPredicateValue {
    fn physical_type(self) -> ColumnPhysicalType {
        match self {
            Self::I32(_) => ColumnPhysicalType::I32,
            Self::I64(_) => ColumnPhysicalType::I64,
            Self::F32(_) => ColumnPhysicalType::F32,
            Self::F64(_) => ColumnPhysicalType::F64,
        }
    }

    fn from_stat(value: OcbStatScalarV1) -> Self {
        match value {
            OcbStatScalarV1::I32(value) => Self::I32(value),
            OcbStatScalarV1::I64(value) => Self::I64(value),
            OcbStatScalarV1::F32(value) => Self::F32(value),
            OcbStatScalarV1::F64(value) => Self::F64(value),
        }
    }

    fn cmp_same_type(self, other: Self) -> Result<Ordering> {
        match (self, other) {
            (Self::I32(left), Self::I32(right)) => Ok(left.cmp(&right)),
            (Self::I64(left), Self::I64(right)) => Ok(left.cmp(&right)),
            (Self::F32(left), Self::F32(right)) => {
                left.partial_cmp(&right)
                    .ok_or(ArcadiaTioError::ocb_invalid_input(
                        "OCB f32 predicate/stat value cannot be NaN",
                    ))
            }
            (Self::F64(left), Self::F64(right)) => {
                left.partial_cmp(&right)
                    .ok_or(ArcadiaTioError::ocb_invalid_input(
                        "OCB f64 predicate/stat value cannot be NaN",
                    ))
            }
            _ => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB predicate/stat type mismatch",
            )),
        }
    }
}

/// Inclusive row-group predicate over one named column.
#[derive(Debug, Clone, PartialEq)]
pub struct RowGroupPredicate {
    /// Name of the column whose row-group statistics are tested.
    pub column: String,
    /// Inclusive lower bound; `None` leaves the lower side open.
    pub lower: Option<ColumnPredicateValue>,
    /// Inclusive upper bound; `None` leaves the upper side open.
    pub upper: Option<ColumnPredicateValue>,
}

impl RowGroupPredicate {
    pub fn new(
        column: impl Into<String>,
        lower: Option<ColumnPredicateValue>,
        upper: Option<ColumnPredicateValue>,
    ) -> Self {
        Self {
            column: column.into(),
            lower,
            upper,
        }
    }

    pub fn between(
        column: impl Into<String>,
        lower: ColumnPredicateValue,
        upper: ColumnPredicateValue,
    ) -> Self {
        Self::new(column, Some(lower), Some(upper))
    }

    pub fn equal(column: impl Into<String>, value: ColumnPredicateValue) -> Self {
        Self::new(column, Some(value), Some(value))
    }
}

/// Scalar bounds for one declared OCB ordering-key column.
///
/// This is a row-group pruning helper, not a row-level or lexicographic query
/// engine. Bounds are inclusive scalar value bounds over the selected ordering
/// column. For composite ordering declarations, multiple key ranges are combined
/// as ordinary conjunctive row-group predicates and may include extra rows.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleOrderingKeyRange {
    /// Zero-based index into [`ColumnBundleMetadata::ordering_keys`].
    pub key_index: usize,
    /// Inclusive scalar lower bound for this ordering-key column.
    pub lower: Option<ColumnPredicateValue>,
    /// Inclusive scalar upper bound for this ordering-key column.
    pub upper: Option<ColumnPredicateValue>,
}

impl ColumnBundleOrderingKeyRange {
    pub fn new(
        key_index: usize,
        lower: Option<ColumnPredicateValue>,
        upper: Option<ColumnPredicateValue>,
    ) -> Self {
        Self {
            key_index,
            lower,
            upper,
        }
    }

    pub fn between(
        key_index: usize,
        lower: ColumnPredicateValue,
        upper: ColumnPredicateValue,
    ) -> Self {
        Self::new(key_index, Some(lower), Some(upper))
    }

    pub fn equal(key_index: usize, value: ColumnPredicateValue) -> Self {
        Self::new(key_index, Some(value), Some(value))
    }
}

#[derive(Debug, Clone)]
struct ResolvedRowGroupPredicate {
    column_id: u32,
    physical_type: ColumnPhysicalType,
    lower: Option<ColumnPredicateValue>,
    upper: Option<ColumnPredicateValue>,
}

/// Read planning/reporting metadata for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleReadReport {
    /// Thread count requested by the read options.
    pub requested_threads: usize,
    /// Thread count actually used after bounded fallback decisions.
    pub effective_threads: usize,
    /// Number of row groups selected by projection/predicate planning.
    pub selected_row_groups: usize,
    /// Number of row groups pruned by predicates.
    pub pruned_row_groups: usize,
    /// Number of selected column chunks that may be decoded.
    pub selected_column_chunks: usize,
    /// Stable snake_case fallback reason, when the planner reduced execution.
    pub fallback_reason: Option<&'static str>,
}

/// Opt-in diagnostic timing and byte counters for one OCB read.
///
/// These fields are cumulative diagnostics, not benchmark claims. Timings use a
/// monotonic clock and are expressed in nanoseconds. A zero value means either
/// the bucket did not apply to this read or the measured duration rounded down;
/// callers should use these fields for attribution experiments rather than API
/// correctness.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ColumnBundleReadAttribution {
    /// Time spent planning projection and row-group predicates.
    pub plan_ns: u64,
    /// Wall time spent executing selected row-group reads and visitor callbacks after planning.
    pub execute_wall_ns: u64,
    /// Cumulative wall time spent inside visitor callbacks for visitor-style reads.
    ///
    /// Ordinary owned read APIs report zero. For visitor APIs this measures only
    /// caller callback execution/handoff time after a decoded `ColumnBatch` has
    /// been produced; any surplus `execute_wall_ns` beyond TIO worker buckets and
    /// this field is scheduler/join/wave orchestration overhead.
    pub callback_wall_ns: u64,
    /// Cumulative worker time spent reading selected row groups.
    pub row_group_read_ns: u64,
    /// Cumulative time spent seeking/reading selected OCB objects from the file.
    pub read_io_ns: u64,
    /// Cumulative time spent validating OCB object checksums.
    pub checksum_ns: u64,
    /// Cumulative time spent decompressing selected column chunks.
    pub decompression_ns: u64,
    /// Cumulative time spent decoding primitive byte payloads into typed vectors.
    pub primitive_decode_ns: u64,
    /// Cumulative time spent projecting fixed-binary payload fields into caller buffers.
    pub fixed_payload_decode_ns: u64,
    /// Cumulative time spent copying/materializing values when separately measured.
    pub copy_materialization_ns: u64,
    /// Native C ABI conversion/allocation/copy time when measured by that layer.
    pub native_to_c_copy_ns: Option<u64>,
    /// Public wrapper copy time when measured by that layer.
    pub wrapper_copy_ns: Option<u64>,
    /// Selected object bytes physically read, including chunk/object headers.
    pub bytes_read: u64,
    /// Selected compressed column-value payload bytes from chunk descriptors.
    pub compressed_bytes: u64,
    /// Selected uncompressed column-value payload bytes from chunk descriptors.
    pub uncompressed_bytes: u64,
    /// Thread count requested by read options.
    pub requested_threads: usize,
    /// Thread count actually used after bounded fallback decisions.
    pub effective_threads: usize,
    /// Number of row groups materialized for this read execution.
    ///
    /// For complete reads this equals the selected plan row groups. For visitor
    /// reads stopped early, this includes row groups already materialized in the
    /// current bounded wave, which may exceed `cursor_report.batches_yielded`.
    pub selected_row_groups: usize,
    /// Number of row groups pruned during planning.
    pub pruned_row_groups: usize,
    /// Number of column chunks materialized for this read execution.
    pub selected_column_chunks: usize,
    /// Stable snake_case fallback reason, when execution was reduced.
    pub fallback_reason: Option<&'static str>,
}

/// Planned row groups and columns for one request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleReadPlan {
    /// File-local column ids selected by the projection.
    pub projected_column_ids: Vec<u32>,
    /// File-local row-group ids selected by predicates.
    pub row_group_ids: Vec<u32>,
    /// Planning report for the request.
    pub report: ColumnBundleReadReport,
}

/// Read result plus execution report.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReadOutcome {
    /// Deterministic row-group-ordered batches returned by the read.
    pub batches: Vec<ColumnBatch>,
    /// Execution report for the request.
    pub report: ColumnBundleReadReport,
}

/// Read result plus opt-in diagnostic attribution.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReadAttributedOutcome {
    /// Deterministic row-group-ordered batches and ordinary read report.
    pub outcome: ColumnBundleReadOutcome,
    /// Diagnostic timing/byte counters collected during the read.
    pub attribution: ColumnBundleReadAttribution,
}

/// Visitor cursor report plus opt-in diagnostic attribution.
///
/// `attribution.execute_wall_ns` spans selected row-group reads, wave joins, and
/// visitor callbacks. The cumulative `row_group_read_ns`/I/O/decode counters
/// only cover TIO row-group materialization work; `callback_wall_ns` isolates
/// caller callback/handoff time measured by the visitor loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleReadAttributedCursorReport {
    /// Bounded visitor progress report.
    pub cursor_report: ColumnBundleReadCursorReport,
    /// Diagnostic timing/byte counters collected during the visitor read.
    pub attribution: ColumnBundleReadAttribution,
}

/// Visitor return control for bounded OCB reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnBundleVisitControl {
    /// Continue visiting batches.
    Continue,
    /// Stop after the current batch without treating the read as a failure.
    Stop,
}

/// Options for bounded visitor-style OCB reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleReadCursorOptions {
    /// Maximum decoded row-group batches resident before visitor callbacks drain them.
    pub max_in_flight_row_groups: usize,
    /// Preserve deterministic plan row-group order while yielding batches.
    pub ordered: bool,
}

impl Default for ColumnBundleReadCursorOptions {
    fn default() -> Self {
        Self {
            max_in_flight_row_groups: 1,
            ordered: true,
        }
    }
}

/// Report returned by visitor-style OCB reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleReadCursorReport {
    /// Ordinary planning/execution report for the read.
    pub base_report: ColumnBundleReadReport,
    /// Number of row-group batches yielded to the visitor.
    pub batches_yielded: usize,
    /// Number of logical rows yielded to the visitor.
    pub rows_yielded: u64,
    /// Largest number of decoded row groups materialized before callbacks drained a wave.
    ///
    /// This is bounded by `max_in_flight_row_groups` and the effective read
    /// thread count. It remains zero when no row groups were selected.
    pub max_in_flight_row_groups_observed: usize,
    /// Whether the visitor requested an early stop.
    pub cancelled: bool,
}

/// Mutable caller-owned storage for a fill read.
pub enum PrimitiveColumnValuesMut<'a> {
    /// Signed 32-bit integer output storage.
    I32(&'a mut [i32]),
    /// Signed 64-bit integer output storage.
    I64(&'a mut [i64]),
    /// 32-bit floating-point output storage.
    F32(&'a mut [f32]),
    /// 64-bit floating-point output storage.
    F64(&'a mut [f64]),
    /// Fixed-width opaque byte output storage, filled contiguously row-major.
    FixedBinary {
        /// Number of bytes in each row value.
        width: u32,
        /// Caller-owned byte storage. The first row_count * width bytes are filled.
        bytes: &'a mut [u8],
    },
}

impl PrimitiveColumnValuesMut<'_> {
    /// Physical type represented by this mutable output slice.
    pub fn physical_type(&self) -> ColumnPhysicalType {
        match self {
            Self::I32(_) => ColumnPhysicalType::I32,
            Self::I64(_) => ColumnPhysicalType::I64,
            Self::F32(_) => ColumnPhysicalType::F32,
            Self::F64(_) => ColumnPhysicalType::F64,
            Self::FixedBinary { width, .. } => ColumnPhysicalType::FixedBinary { width: *width },
        }
    }

    /// Element capacity in the caller-owned output slice.
    pub fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::FixedBinary { width: 0, .. } => 0,
            Self::FixedBinary { width, bytes } => bytes.len() / *width as usize,
        }
    }

    /// Whether the caller-owned output slice is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn validate_capacity(&self, row_count: usize) -> Result<()> {
        match self {
            Self::FixedBinary { width: 0, .. } => Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary fill buffer requires width > 0",
            )),
            Self::FixedBinary { width, bytes } => {
                let expected_bytes = row_count.checked_mul(*width as usize).ok_or(
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB fixed-binary fill byte count overflows",
                    ),
                )?;
                if bytes.len() < expected_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB fill buffer value capacity is too small for row group",
                    ));
                }
                Ok(())
            }
            _ => {
                if self.len() < row_count {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB fill buffer value capacity is too small for row group",
                    ));
                }
                Ok(())
            }
        }
    }
}

/// One caller-owned column output buffer for a single-row-group fill read.
pub struct ColumnBundleColumnFillBuffer<'a> {
    /// Optional column name selector. At least one of name or id must be set.
    pub column_name: Option<&'a str>,
    /// Optional file-local column-id selector. If both name and id are set they must match.
    pub column_id: Option<u32>,
    /// Caller-owned primitive output storage.
    pub values: PrimitiveColumnValuesMut<'a>,
    /// Optional caller-owned LSB-first validity bitmap output storage.
    pub validity_bytes: Option<&'a mut [u8]>,
    /// Whether the caller accepts nullable chunks. A validity buffer is still
    /// required when the selected chunk has a validity bitmap.
    pub allow_nulls: bool,
}

/// Options for caller-owned fill reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleReadFillOptions {
    /// Whether checksum validation should run. Current OCB reads remain fail-closed.
    pub validate_checksums: bool,
}

impl Default for ColumnBundleReadFillOptions {
    fn default() -> Self {
        Self {
            validate_checksums: true,
        }
    }
}

/// Per-column result from a caller-owned fill read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleColumnFillReport {
    /// Resolved file-local column id.
    pub column_id: u32,
    /// Number of rows copied into the caller-owned value buffer.
    pub rows_filled: usize,
    /// Whether caller-owned validity bytes were filled.
    pub validity_filled: bool,
}

/// Report from a caller-owned single-row-group fill read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleReadFillReport {
    /// File-local row-group id.
    pub row_group_id: u32,
    /// Logical starting row for this row group in the selected snapshot.
    pub base_row: u64,
    /// Number of rows in this row group.
    pub row_count: u64,
    /// Per-requested-column fill reports in caller buffer order.
    pub columns: Vec<ColumnBundleColumnFillReport>,
}

/// Read options for the OCB bundle reader.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleReadOptions {
    /// Maximum worker threads requested by the caller.
    pub max_threads: usize,
    /// Whether chunk checksum validation should run while decoding.
    pub validate_checksums: bool,
    /// Reserved decoded-dictionary hint; batch reads currently return dictionary codes.
    pub decode_dictionaries: bool,
}

impl Default for ColumnBundleReadOptions {
    fn default() -> Self {
        Self {
            max_threads: 1,
            validate_checksums: true,
            decode_dictionaries: false,
        }
    }
}

impl ColumnBundleReadOptions {
    pub fn serial() -> Self {
        Self::default()
    }

    pub fn parallel(max_threads: usize) -> Self {
        Self {
            max_threads,
            ..Self::default()
        }
    }
}

/// Read request for the OCB bundle reader.
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnBundleReadRequest {
    /// Column projection for this read.
    pub projection: ColumnProjection,
    /// Inclusive row-group pruning predicates.
    pub predicates: Vec<RowGroupPredicate>,
    /// Execution and validation options.
    pub options: ColumnBundleReadOptions,
}

impl Default for ColumnBundleReadRequest {
    fn default() -> Self {
        Self {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::default(),
        }
    }
}

/// Fail-closed guard options for strict OCB read planning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleStrictReadPlanningOptions {
    /// Maximum number of row groups the resulting plan may select.
    pub max_selected_row_groups: usize,
}

impl ColumnBundleStrictReadPlanningOptions {
    pub const fn new(max_selected_row_groups: usize) -> Self {
        Self {
            max_selected_row_groups,
        }
    }
}

impl ColumnBundleReadRequest {
    /// Build a normal read request from inclusive scalar bounds over declared
    /// ordering-key columns.
    ///
    /// This helper only constructs row-group pruning predicates. It does not
    /// add row-level filtering, and for composite ordering declarations the
    /// generated predicates are conjunctive scalar bounds over the requested
    /// ordering-key columns rather than a lexicographic cursor predicate. Reads
    /// may therefore include extra rows and callers should still apply any
    /// required row-level filtering outside OCB.
    pub fn from_ordering_key_ranges(
        metadata: &ColumnBundleMetadata,
        projection: ColumnProjection,
        ranges: Vec<ColumnBundleOrderingKeyRange>,
        options: ColumnBundleReadOptions,
    ) -> Result<Self> {
        if ranges.is_empty() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB ordering range request requires at least one bound",
            ));
        }
        let mut ranges = ranges;
        ranges.sort_by_key(|range| range.key_index);
        let mut seen = BTreeSet::new();
        let mut predicates = Vec::with_capacity(ranges.len());
        for range in ranges {
            if !seen.insert(range.key_index) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB ordering range request contains duplicate key indexes",
                ));
            }
            if range.lower.is_none() && range.upper.is_none() {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB ordering range bound must include at least one side",
                ));
            }
            let key = metadata.ordering_keys.get(range.key_index).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB ordering range references an unknown ordering key",
                ),
            )?;
            let column = metadata
                .columns
                .iter()
                .find(|column| column.id == key.column_id)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB ordering key column is missing from metadata",
                ))?;
            if matches!(column.physical_type, ColumnPhysicalType::FixedBinary { .. }) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB ordering range over fixed-binary columns is not supported",
                ));
            }
            for bound in [range.lower, range.upper].into_iter().flatten() {
                if bound.physical_type() != column.physical_type {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB ordering range bound dtype does not match ordering column dtype",
                    ));
                }
            }
            if let (Some(lower), Some(upper)) = (range.lower, range.upper) {
                if lower.cmp_same_type(upper)? == Ordering::Greater {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB ordering range lower bound is greater than upper bound",
                    ));
                }
            }
            predicates.push(RowGroupPredicate::new(
                key.column_name.clone(),
                range.lower,
                range.upper,
            ));
        }
        Ok(Self {
            projection,
            predicates,
            options,
        })
    }
}

/// Validation depth used while opening an OCB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnBundleOpenValidation {
    /// Validate the root, schema, dictionaries, row-group index, ordering graph,
    /// chunk descriptors, object bounds, and chunk headers. Column payload CRCs
    /// are validated when selected chunks are read.
    MetadataGraph,
    /// Validate every referenced column payload and validity bitmap during open.
    FullPayload,
}

impl Default for ColumnBundleOpenValidation {
    fn default() -> Self {
        Self::MetadataGraph
    }
}

impl From<ColumnBundleOpenValidation> for OcbOpenValidationMode {
    fn from(value: ColumnBundleOpenValidation) -> Self {
        match value {
            ColumnBundleOpenValidation::MetadataGraph => OcbOpenValidationMode::MetadataGraph,
            ColumnBundleOpenValidation::FullPayload => OcbOpenValidationMode::FullPayload,
        }
    }
}

/// Options for opening an OCB file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColumnBundleOpenOptions {
    /// Validation depth to apply before returning an opened snapshot.
    pub validation: ColumnBundleOpenValidation,
}

/// One-file ordered column bundle reader.
#[derive(Debug, Clone)]
pub struct ColumnBundleFile {
    path: PathBuf,
    metadata: Arc<OcbMetadataV1>,
    columns: Arc<Vec<BundleColumn>>,
}

impl ColumnBundleFile {
    /// Open one OCB file using metadata-graph validation.
    ///
    /// Column payload CRCs are still checked when selected chunks are read. Use
    /// [`Self::open_with_options`] with [`ColumnBundleOpenValidation::FullPayload`]
    /// when whole-file payload integrity must be verified before reads.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let metadata = Arc::new(read_metadata(&path)?);
        Self::from_metadata(path, metadata)
    }

    /// Open one OCB file with explicit validation options.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: ColumnBundleOpenOptions,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let metadata = Arc::new(read_metadata_with_validation(
            &path,
            OcbOpenValidationMode::from(options.validation),
        )?);
        Self::from_metadata(path, metadata)
    }

    fn from_metadata(path: PathBuf, metadata: Arc<OcbMetadataV1>) -> Result<Self> {
        validate_metadata(&metadata)?;
        let columns = Arc::new(resolve_columns(&metadata)?);
        Ok(Self {
            path,
            metadata,
            columns,
        })
    }

    /// Return the resolved generic schema columns.
    pub fn columns(&self) -> &[BundleColumn] {
        &self.columns
    }

    /// Return total logical row count recorded by the root object.
    pub fn row_count(&self) -> u64 {
        self.metadata.root.row_count
    }

    /// Return number of internal row groups recorded by the root object.
    pub fn row_group_count(&self) -> u32 {
        self.metadata.root.row_group_count
    }

    /// Return a stable metadata summary for the opened snapshot.
    pub fn metadata(&self) -> Result<ColumnBundleMetadata> {
        let dictionaries = self
            .metadata
            .dictionary_index
            .as_ref()
            .map(|index| {
                index
                    .dictionaries
                    .iter()
                    .map(|dictionary| {
                        let name = self
                            .metadata
                            .string_table
                            .strings
                            .get(dictionary.name_string_id as usize)
                            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                                "OCB dictionary name string id is out of range",
                            ))?
                            .clone();
                        Ok(BundleDictionaryDescriptor {
                            dictionary_id: dictionary.dictionary_id,
                            name,
                            code_physical_type: scalar_column_physical_type(
                                dictionary.code_physical_type,
                            )?,
                            value_kind: dictionary.value_kind.into(),
                            entry_count: dictionary.entry_count,
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let ordering_keys = self
            .metadata
            .ordering_proof
            .as_ref()
            .map(|proof| {
                proof
                    .keys
                    .iter()
                    .map(|key| {
                        let column = self
                            .columns
                            .iter()
                            .find(|column| column.id == key.column_id)
                            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                                "OCB ordering key column id is out of range",
                            ))?;
                        Ok(BundleOrderingKey {
                            column_id: key.column_id,
                            column_name: column.name.clone(),
                            direction: key.direction.into(),
                            null_order: key.null_order.into(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()
            })
            .transpose()?
            .unwrap_or_default();

        let column_chunk_count = u32::try_from(self.metadata.row_group_index.column_chunks.len())
            .map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file("OCB column chunk count exceeds u32")
        })?;

        Ok(ColumnBundleMetadata {
            format_name: "OCB",
            appendable: self.metadata.appendable,
            root_generation: self.metadata.root_generation,
            previous_root_generation: self.metadata.previous_root_generation,
            row_count: self.metadata.root.row_count,
            row_group_count: self.metadata.root.row_group_count,
            column_chunk_count,
            columns: self.columns.as_ref().clone(),
            dictionaries,
            ordering_keys,
        })
    }

    /// Return generic read-only summaries for every visible row group.
    ///
    /// This inspects only committed snapshot metadata and does not read,
    /// repair, clean up, or decode column payloads.
    pub fn row_group_summaries(&self) -> Result<Vec<ColumnBundleRowGroupSummary>> {
        self.build_row_group_summaries(
            self.metadata
                .row_group_index
                .row_groups
                .iter()
                .map(|row_group| row_group.row_group_id),
            None,
        )
    }

    /// Return generic read-only summaries for row groups selected by a plan.
    ///
    /// The returned chunk summaries are restricted to the plan projection, while
    /// scalar stats remain the row-group metadata recorded in the file. Forged
    /// or stale plans fail closed through the same validation used for reads.
    pub fn read_plan_row_group_summaries(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<Vec<ColumnBundleRowGroupSummary>> {
        self.validate_read_plan(plan)?;
        let projected_column_ids = plan
            .projected_column_ids
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        self.build_row_group_summaries(
            plan.row_group_ids.iter().copied(),
            Some(&projected_column_ids),
        )
    }

    /// Compute deterministic generic selected-snapshot fingerprints.
    ///
    /// These fingerprints are intended for compatibility/certification gates.
    /// They do not read payload bytes and are not cryptographic file digests.
    pub fn snapshot_fingerprint(&self) -> Result<ColumnBundleSnapshotFingerprint> {
        let metadata = self.metadata()?;
        let row_groups = self.row_group_summaries()?;
        Ok(snapshot_fingerprint_for_summaries(&metadata, &row_groups))
    }

    /// Return generic certification metadata for a previously validated read plan.
    ///
    /// This captures the selected snapshot fingerprint, plan report, plan-order
    /// row-group summaries, selected chunk byte totals, and a selected-chunk
    /// descriptor/checksum fingerprint. It is a fail-closed metadata substrate;
    /// downstream remains responsible for application-specific payload
    /// equivalence, channel/index continuity, and runtime readiness gates.
    pub fn read_plan_certification(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<ColumnBundleReadPlanCertification> {
        self.validate_read_plan(plan)?;
        let metadata = self.metadata()?;
        let snapshot_fingerprint = self.snapshot_fingerprint()?;
        let row_groups = self.read_plan_row_group_summaries(plan)?;
        let mut selected_compressed_bytes = 0u64;
        let mut selected_uncompressed_bytes = 0u64;
        for row_group in &row_groups {
            for chunk in &row_group.chunks {
                selected_compressed_bytes = selected_compressed_bytes
                    .checked_add(chunk.compressed_bytes)
                    .ok_or(ArcadiaTioError::ocb_corrupt_file(
                        "OCB read plan certification compressed byte total overflows",
                    ))?;
                selected_uncompressed_bytes = selected_uncompressed_bytes
                    .checked_add(chunk.uncompressed_bytes)
                    .ok_or(ArcadiaTioError::ocb_corrupt_file(
                        "OCB read plan certification uncompressed byte total overflows",
                    ))?;
            }
        }
        let selected_chunk_fingerprint = fingerprint_selected_chunks(&row_groups);
        Ok(ColumnBundleReadPlanCertification {
            snapshot_fingerprint,
            file_len: self.metadata.file_len,
            root_generation: metadata.root_generation,
            previous_root_generation: metadata.previous_root_generation,
            row_count: metadata.row_count,
            row_group_count: metadata.row_group_count,
            report: plan.report.clone(),
            row_groups,
            selected_compressed_bytes,
            selected_uncompressed_bytes,
            selected_chunk_fingerprint,
        })
    }

    /// Decode one file-local dictionary on the explicit cold path.
    pub fn dictionary_values(&self, dictionary_id: u32) -> Result<BundleDictionaryValues> {
        let dictionary_index =
            self.metadata
                .dictionary_index
                .as_ref()
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB file does not contain dictionaries",
                ))?;
        let dictionary = dictionary_index
            .dictionaries
            .iter()
            .find(|dictionary| dictionary.dictionary_id == dictionary_id)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB dictionary id not found",
            ))?;
        let mut file = std::fs::File::open(&self.path)?;
        let bytes = read_object_bytes(
            &mut file,
            self.metadata.file_len,
            dictionary.values_ref,
            OcbBodyKindV1::DictionaryValues,
        )?;
        let raw_values = OcbDictionaryValuesV1::read_from(std::io::Cursor::new(bytes))?;
        if raw_values.value_kind != dictionary.value_kind {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values kind does not match dictionary descriptor",
            ));
        }
        if raw_values.values.len() != dictionary.entry_count as usize {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values entry count does not match descriptor",
            ));
        }
        let name = self
            .metadata
            .string_table
            .strings
            .get(dictionary.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary name string id is out of range",
            ))?
            .clone();
        Ok(BundleDictionaryValues {
            dictionary_id,
            name,
            value_kind: dictionary.value_kind.into(),
            values: decode_dictionary_values(raw_values)?,
        })
    }

    /// Strictly plan projected/pruned row-group reads without reading data chunks.
    ///
    /// Unlike [`Self::plan_read`], this helper fails closed when any requested
    /// predicate column lacks scalar row-group stats for any visible row group,
    /// and when the selected row groups exceed the caller-provided cap. It does
    /// not add row-level filtering and does not change ordinary read behavior.
    pub fn plan_read_strict(
        &self,
        request: &ColumnBundleReadRequest,
        options: ColumnBundleStrictReadPlanningOptions,
    ) -> Result<ColumnBundleReadPlan> {
        validate_read_options(&request.options)?;
        let selected = self.resolve_projection(&request.projection)?;
        let predicates = self.resolve_predicates(&request.predicates)?;
        self.require_predicate_stats_available(&predicates)?;

        let mut row_group_ids = Vec::new();
        let mut pruned_row_groups = 0usize;
        for row_group in &self.metadata.row_group_index.row_groups {
            if row_group_matches_predicates(&self.metadata, row_group, &predicates)? {
                row_group_ids.push(row_group.row_group_id);
            } else {
                pruned_row_groups += 1;
            }
        }
        if row_group_ids.len() > options.max_selected_row_groups {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB strict read plan selected more row groups than the caller cap",
            ));
        }
        let report = build_read_report(
            request.options.max_threads,
            row_group_ids.len(),
            pruned_row_groups,
            selected.len(),
        );
        Ok(ColumnBundleReadPlan {
            projected_column_ids: selected,
            row_group_ids,
            report,
        })
    }

    /// Plan projected/pruned row-group reads without reading data chunks.
    pub fn plan_read(&self, request: &ColumnBundleReadRequest) -> Result<ColumnBundleReadPlan> {
        validate_read_options(&request.options)?;
        let selected = self.resolve_projection(&request.projection)?;
        let predicates = self.resolve_predicates(&request.predicates)?;
        let mut row_group_ids = Vec::new();
        let mut pruned_row_groups = 0usize;
        for row_group in &self.metadata.row_group_index.row_groups {
            if row_group_matches_predicates(&self.metadata, row_group, &predicates)? {
                row_group_ids.push(row_group.row_group_id);
            } else {
                pruned_row_groups += 1;
            }
        }
        let report = build_read_report(
            request.options.max_threads,
            row_group_ids.len(),
            pruned_row_groups,
            selected.len(),
        );
        Ok(ColumnBundleReadPlan {
            projected_column_ids: selected,
            row_group_ids,
            report,
        })
    }

    /// Read every column for one file-local row group id.
    #[doc(hidden)]
    pub fn read_row_group_by_id(&self, row_group_id: u32) -> Result<ColumnBatch> {
        self.validate_read_plan(&ColumnBundleReadPlan {
            projected_column_ids: self.columns.iter().map(|column| column.id).collect(),
            row_group_ids: vec![row_group_id],
            report: build_read_report(1, 1, 0, self.columns.len()),
        })?;
        read_row_group(
            &self.path,
            &self.metadata,
            &self.columns,
            row_group_id,
            &self
                .columns
                .iter()
                .map(|column| column.id)
                .collect::<Vec<_>>(),
        )
    }

    /// Read projected columns as deterministic row-group ordered batches.
    pub fn read_batches(&self, request: ColumnBundleReadRequest) -> Result<Vec<ColumnBatch>> {
        Ok(self.read_batches_with_report(request)?.batches)
    }

    /// Read projected columns and return execution report metadata.
    pub fn read_batches_with_report(
        &self,
        request: ColumnBundleReadRequest,
    ) -> Result<ColumnBundleReadOutcome> {
        let plan = self.plan_read(&request)?;
        self.read_plan_batches(&plan)
    }

    /// Read projected columns and collect opt-in diagnostic attribution.
    pub fn read_batches_with_attribution(
        &self,
        request: ColumnBundleReadRequest,
    ) -> Result<ColumnBundleReadAttributedOutcome> {
        let plan_started = Instant::now();
        let plan = self.plan_read(&request)?;
        let plan_ns = duration_to_ns(plan_started.elapsed());
        self.read_plan_batches_with_attribution_and_plan_ns(&plan, plan_ns)
    }

    /// Read one row group directly into caller-owned typed column buffers.
    ///
    /// This lower-copy API validates all requested buffers (column identity,
    /// dtype, capacity, duplicates, and nullable-validity requirements) before
    /// reading selected payload chunks. If a later I/O, checksum, corruption, or
    /// decode error occurs, output buffers are unspecified/partial and callers
    /// must discard them.
    pub fn read_row_group_into(
        &self,
        row_group_id: u32,
        buffers: &mut [ColumnBundleColumnFillBuffer<'_>],
        options: ColumnBundleReadFillOptions,
    ) -> Result<ColumnBundleReadFillReport> {
        validate_read_fill_options(options)?;
        read_row_group_into(
            &self.path,
            &self.metadata,
            &self.columns,
            row_group_id,
            buffers,
        )
    }

    /// Allocate a reusable caller-owned buffer pool for a planned projection.
    ///
    /// The pool contains one slot per requested in-flight row group. Each slot is
    /// initialized for the plan projection and can be reused by
    /// [`Self::visit_plan_row_groups_into`] or
    /// [`Self::visit_plan_row_groups_into_with_attribution`].
    pub fn reusable_buffer_pool_for_plan(
        &self,
        plan: &ColumnBundleReadPlan,
        max_in_flight_row_groups: usize,
        allow_nulls: bool,
    ) -> Result<ColumnBundleReusableBufferPool> {
        self.validate_read_plan(plan)?;
        if max_in_flight_row_groups == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable buffer pool requires at least one in-flight row group",
            ));
        }
        let projected_columns = projected_columns_for_plan(&self.columns, plan)?;
        let row_capacity = max_row_count_for_plan(&self.metadata, plan)?;
        let mut buffers = Vec::with_capacity(max_in_flight_row_groups);
        for _ in 0..max_in_flight_row_groups {
            buffers.push(ColumnBundleReusableBuffers::for_columns(
                &projected_columns,
                row_capacity,
                allow_nulls,
            )?);
        }
        Ok(ColumnBundleReusableBufferPool { buffers })
    }

    /// Allocate a reusable buffer for generic fixed-binary record projection.
    ///
    /// The source column must be part of `plan.projected_column_ids`, must have
    /// the expected fixed-binary width, and is rejected before payload reads when
    /// `projection.allow_nulls` is false and the schema marks it nullable.
    pub fn fixed_binary_projection_buffer_for_plan(
        &self,
        plan: &ColumnBundleReadPlan,
        projection: &FixedBinaryRecordProjection,
    ) -> Result<ColumnBundleFixedBinaryProjectionBuffer> {
        self.validate_read_plan(plan)?;
        let source_column =
            validate_fixed_binary_record_projection(&self.columns, plan, projection)?;
        let row_capacity = max_row_count_for_plan(&self.metadata, plan)?;
        ColumnBundleFixedBinaryProjectionBuffer::for_projection(
            source_column,
            projection,
            row_capacity,
        )
    }

    /// Visit projected columns as bounded deterministic row-group batches.
    ///
    /// This API yields owned `ColumnBatch` values one at a time to the visitor
    /// while bounding internal in-flight materialization to
    /// `max_in_flight_row_groups`. It preserves the same projection, predicate,
    /// checksum, and dictionary-code behavior as `read_batches`.
    pub fn visit_batches<F>(
        &self,
        request: ColumnBundleReadRequest,
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        let plan = self.plan_read(&request)?;
        self.visit_plan_batches(&plan, cursor_options, visitor)
    }

    /// Visit projected columns and collect opt-in diagnostic attribution.
    pub fn visit_batches_with_attribution<F>(
        &self,
        request: ColumnBundleReadRequest,
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        let plan_started = Instant::now();
        let plan = self.plan_read(&request)?;
        let plan_ns = duration_to_ns(plan_started.elapsed());
        self.visit_plan_batches_with_attribution_and_plan_ns(
            &plan,
            cursor_options,
            plan_ns,
            visitor,
        )
    }

    /// Visit a previously planned read as bounded deterministic row-group batches.
    pub fn visit_plan_batches<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let report = execution_report_for_plan(plan, plan.row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: plan.row_group_ids.clone(),
            report,
        };
        self.visit_execution_plan(&execution_plan, cursor_options, visitor)
    }

    /// Visit a previously planned read and collect opt-in diagnostic attribution.
    pub fn visit_plan_batches_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        self.visit_plan_batches_with_attribution_and_plan_ns(plan, cursor_options, 0, visitor)
    }

    fn visit_plan_batches_with_attribution_and_plan_ns<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        plan_ns: u64,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let report = execution_report_for_plan(plan, plan.row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: plan.row_group_ids.clone(),
            report,
        };
        self.visit_execution_plan_with_attribution(
            &execution_plan,
            cursor_options,
            plan_ns,
            visitor,
        )
    }

    /// Visit an explicit subset of a previously planned read.
    ///
    /// `row_group_ids` are file-local ids that must already be present in
    /// `plan`. The subset is validated before payload reads begin: unknown ids
    /// and duplicate ids fail closed, and there is no fallback predicate scan.
    /// Batches are visited in the deterministic row-group order from the
    /// original plan, not caller-supplied subset order. Internal decoded
    /// materialization is bounded by `max_in_flight_row_groups` and the
    /// effective thread count. `ColumnBundleVisitControl::Stop` stops launching
    /// later waves and drops any already-materialized current-wave batches before
    /// returning.
    pub fn visit_plan_row_groups<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report,
        };
        self.visit_execution_plan(&execution_plan, cursor_options, visitor)
    }

    /// Visit an explicit row-group subset and collect opt-in diagnostic attribution.
    ///
    /// This has the same validation, ordering, and boundedness contract as
    /// [`Self::visit_plan_row_groups`]. Attribution row-group/chunk/byte counters
    /// describe row groups materialized by TIO before completion or `Stop`;
    /// `cursor_report` describes batches actually yielded to the visitor.
    pub fn visit_plan_row_groups_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        cursor_options: ColumnBundleReadCursorOptions,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report,
        };
        self.visit_execution_plan_with_attribution(&execution_plan, cursor_options, 0, visitor)
    }

    /// Read and prepare selected row groups on a fixed worker budget, then
    /// release caller-owned results through a deterministic ordered boundary.
    ///
    /// The sole worker budget is the plan's original
    /// [`ColumnBundleReadOptions::max_threads`] request. The additional
    /// `max_in_flight_row_groups` cap bounds queued tasks, active decoded
    /// batches, the result queue, and out-of-order pending results together.
    /// The worker `prepare` callback receives a borrowed batch that is valid
    /// only for that invocation and must return an owned `Send + 'static`
    /// result. `ordered_commit` runs only on the calling thread and always in
    /// the selected plan's row-group order.
    ///
    /// `row_group_ids` are validated and restored to original plan order before
    /// any payload read. If multiple workers fail, the error for the earliest
    /// selected ordinal is returned only after every earlier ordinal resolves.
    /// [`ColumnBundleVisitControl::Stop`] commits the current result, prevents
    /// ordered terminal completion, and returns a coherent partial report.
    ///
    /// The callback name describes its ordered sequencing boundary; the call is
    /// not transactional and cannot roll back callback side effects. A consumer
    /// that requires fail-closed publication must use invocation-local staging,
    /// publish it only after `Ok(report)` with
    /// `report.ordered_terminal_completed == true`, and discard it after `Err`
    /// or non-terminal `Ok`. In particular, replay-visible state must not be
    /// published directly from this callback.
    ///
    /// A preparation result cannot borrow the callback-lifetime batch:
    ///
    /// ```compile_fail
    /// use arcadia_tio_ocb_core::{
    ///     ColumnBundleFile, ColumnBundleParallelPrepareOptions,
    ///     ColumnBundleReadPlan, ColumnBundleVisitControl,
    /// };
    ///
    /// fn borrowed_result_cannot_escape(
    ///     file: &ColumnBundleFile,
    ///     plan: &ColumnBundleReadPlan,
    /// ) {
    ///     let _ = file.parallel_prepare_plan_row_groups(
    ///         plan,
    ///         &plan.row_group_ids,
    ///         ColumnBundleParallelPrepareOptions::default(),
    ///         |_, batch| Ok(&batch.columns),
    ///         |_, _| Ok(ColumnBundleVisitControl::Continue),
    ///     );
    /// }
    /// ```
    pub fn parallel_prepare_plan_row_groups<T, Prepare, Commit>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        options: ColumnBundleParallelPrepareOptions,
        prepare: Prepare,
        ordered_commit: Commit,
    ) -> Result<ColumnBundleParallelPrepareReport>
    where
        T: Send + 'static,
        Prepare: Fn(ColumnBundleParallelPrepareContext, &ColumnBatch) -> Result<T> + Sync,
        Commit: FnMut(ColumnBundleParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let mut tasks = Vec::with_capacity(selected_row_group_ids.len());
        for (selected_row_group_ordinal, row_group_id) in
            selected_row_group_ids.iter().copied().enumerate()
        {
            let row_group = self
                .metadata
                .row_group_index
                .row_groups
                .iter()
                .find(|row_group| row_group.row_group_id == row_group_id)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB parallel prepare row group not found",
                ))?;
            let row_end = row_group.base_row.checked_add(row_group.row_count).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB parallel prepare row range overflows"),
            )?;
            tasks.push(ParallelPrepareTaskSpec {
                selected_row_group_ordinal,
                row_group_id,
                base_row: row_group.base_row,
                row_end,
                row_count: row_group.row_count,
            });
        }
        execute_parallel_prepare(
            tasks,
            report,
            0,
            options,
            |row_group_id| {
                read_row_group_with_attribution(
                    &self.path,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )
            },
            prepare,
            ordered_commit,
        )
    }

    /// Visit an explicit row-group subset into caller-owned reusable buffers.
    ///
    /// This lower-copy visitor keeps decoded values in `buffers` and gives the
    /// callback a borrowed view valid only for the callback duration. The pool
    /// size, `max_in_flight_row_groups`, and effective thread count bound the
    /// number of simultaneously materialized row groups.
    pub fn visit_plan_row_groups_into<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBundleReusableBatchView<'_>) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        validate_reusable_buffer_pool(buffers, plan, &self.columns)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report,
        };
        self.visit_execution_plan_into(&execution_plan, cursor_options, buffers, visitor)
    }

    /// Visit an explicit row-group subset into reusable buffers with attribution.
    ///
    /// Attribution row-group/chunk/byte counters describe row groups
    /// materialized by TIO into the reusable pool before completion or `Stop`;
    /// `cursor_report` describes batches actually yielded to the visitor.
    pub fn visit_plan_row_groups_into_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBundleReusableBatchView<'_>) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        validate_reusable_buffer_pool(buffers, plan, &self.columns)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report,
        };
        self.visit_execution_plan_into_with_attribution(
            &execution_plan,
            cursor_options,
            buffers,
            0,
            visitor,
        )
    }

    /// Visit an explicit row-group subset, project a fixed-binary source column,
    /// and collect attribution.
    ///
    /// This is the native generic compact-payload path: TIO reads the planned
    /// row groups into reusable column buffers, validates the fixed-binary source
    /// projection, decodes caller-described little-endian fields into a reusable
    /// projection buffer, then invokes the coarse row-group callback with both
    /// scalar/reusable column views and projected payload-field views. It keeps
    /// the OCB API generic and does not encode downstream domain semantics.
    pub fn visit_plan_row_groups_project_fixed_binary_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        projection: &FixedBinaryRecordProjection,
        projection_buffer: &mut ColumnBundleFixedBinaryProjectionBuffer,
        visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(
            ColumnBundleReusableBatchView<'_>,
            FixedBinaryProjectedBatchView<'_>,
        ) -> Result<ColumnBundleVisitControl>,
    {
        self.validate_read_plan(plan)?;
        validate_read_cursor_options(cursor_options)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        validate_reusable_buffer_pool(buffers, plan, &self.columns)?;
        validate_fixed_binary_record_projection(&self.columns, plan, projection)?;
        validate_fixed_binary_projection_buffer(projection_buffer, projection)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report,
        };
        self.visit_execution_plan_project_fixed_binary_with_attribution(
            &execution_plan,
            cursor_options,
            buffers,
            projection,
            projection_buffer,
            visitor,
        )
    }

    /// Execute a previously planned read against this selected snapshot.
    ///
    /// The plan is snapshot-local and file-local. Forged or stale plans that
    /// reference unknown/duplicate row-group ids or column ids are rejected
    /// before any payload chunks are read.
    pub fn read_plan_batches(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<ColumnBundleReadOutcome> {
        self.validate_read_plan(plan)?;
        let report = execution_report_for_plan(plan, plan.row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: plan.row_group_ids.clone(),
            report: report.clone(),
        };
        let batches = self.execute_plan(&execution_plan)?;
        Ok(ColumnBundleReadOutcome { batches, report })
    }

    /// Execute a previously planned read and collect diagnostic attribution.
    pub fn read_plan_batches_with_attribution(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<ColumnBundleReadAttributedOutcome> {
        self.read_plan_batches_with_attribution_and_plan_ns(plan, 0)
    }

    fn read_plan_batches_with_attribution_and_plan_ns(
        &self,
        plan: &ColumnBundleReadPlan,
        plan_ns: u64,
    ) -> Result<ColumnBundleReadAttributedOutcome> {
        self.validate_read_plan(plan)?;
        let report = execution_report_for_plan(plan, plan.row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: plan.row_group_ids.clone(),
            report: report.clone(),
        };
        let execute_started = Instant::now();
        let (batches, accumulator) = self.execute_plan_with_attribution(&execution_plan)?;
        let attribution = attribution_from_accumulator(
            accumulator,
            &execution_plan.report,
            plan_ns,
            duration_to_ns(execute_started.elapsed()),
        );
        Ok(ColumnBundleReadAttributedOutcome {
            outcome: ColumnBundleReadOutcome { batches, report },
            attribution,
        })
    }

    /// Execute an explicit subset of a previously planned read.
    ///
    /// `row_group_ids` are file-local ids selected from `plan`. Unknown ids and
    /// duplicate ids fail closed. Output batches are returned in the deterministic
    /// order from the original plan, not caller-supplied subset order.
    pub fn read_plan_row_groups(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
    ) -> Result<ColumnBundleReadOutcome> {
        self.validate_read_plan(plan)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report: report.clone(),
        };
        let batches = self.execute_plan(&execution_plan)?;
        Ok(ColumnBundleReadOutcome { batches, report })
    }

    /// Execute an explicit row-group subset and collect diagnostic attribution.
    pub fn read_plan_row_groups_with_attribution(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
    ) -> Result<ColumnBundleReadAttributedOutcome> {
        self.validate_read_plan(plan)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        let execution_plan = ColumnBundleReadPlan {
            projected_column_ids: plan.projected_column_ids.clone(),
            row_group_ids: selected_row_group_ids,
            report: report.clone(),
        };
        let execute_started = Instant::now();
        let (batches, accumulator) = self.execute_plan_with_attribution(&execution_plan)?;
        let attribution = attribution_from_accumulator(
            accumulator,
            &execution_plan.report,
            0,
            duration_to_ns(execute_started.elapsed()),
        );
        Ok(ColumnBundleReadAttributedOutcome {
            outcome: ColumnBundleReadOutcome { batches, report },
            attribution,
        })
    }

    fn visit_execution_plan<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        mut visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        let mut report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            return Ok(report);
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1));
        for wave in plan.row_group_ids.chunks(wave_size) {
            let wave_batches = if wave_size <= 1 {
                let row_group_id = wave[0];
                vec![read_row_group(
                    &self.path,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )?]
            } else {
                let mut handles = Vec::with_capacity(wave.len());
                for row_group_id in wave.iter().copied() {
                    let path = self.path.clone();
                    let metadata = Arc::clone(&self.metadata);
                    let columns = Arc::clone(&self.columns);
                    let selected = plan.projected_column_ids.clone();
                    handles.push(thread::spawn(move || {
                        read_row_group(&path, &metadata, &columns, row_group_id, &selected)
                    }));
                }
                let mut wave_batches = Vec::with_capacity(handles.len());
                let mut first_error = None;
                for handle in handles {
                    match handle.join() {
                        Ok(Ok(batch)) => wave_batches.push(batch),
                        Ok(Err(err)) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                        Err(_) => {
                            if first_error.is_none() {
                                first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                    "OCB read worker panicked",
                                )));
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    return Err(err);
                }
                wave_batches
            };
            report.max_in_flight_row_groups_observed = report
                .max_in_flight_row_groups_observed
                .max(wave_batches.len());
            for batch in wave_batches {
                let row_count = batch.row_count;
                match visitor(batch)? {
                    ColumnBundleVisitControl::Continue => {
                        report.batches_yielded = report.batches_yielded.saturating_add(1);
                        report.rows_yielded = report.rows_yielded.saturating_add(row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        report.batches_yielded = report.batches_yielded.saturating_add(1);
                        report.rows_yielded = report.rows_yielded.saturating_add(row_count);
                        report.cancelled = true;
                        return Ok(report);
                    }
                }
            }
        }
        Ok(report)
    }

    fn visit_execution_plan_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        plan_ns: u64,
        mut visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBatch) -> Result<ColumnBundleVisitControl>,
    {
        let execute_started = Instant::now();
        let mut accumulator = ReadAttributionAccumulator::default();
        let mut cursor_report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            let attribution = attribution_from_accumulator(
                accumulator,
                &plan.report,
                plan_ns,
                duration_to_ns(execute_started.elapsed()),
            );
            return Ok(ColumnBundleReadAttributedCursorReport {
                cursor_report,
                attribution,
            });
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1));
        for wave in plan.row_group_ids.chunks(wave_size) {
            let wave_batches = if wave_size <= 1 {
                let row_group_id = wave[0];
                let (batch, row_attr) = read_row_group_with_attribution(
                    &self.path,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )?;
                accumulator.add(row_attr);
                vec![batch]
            } else {
                let mut handles = Vec::with_capacity(wave.len());
                for row_group_id in wave.iter().copied() {
                    let path = self.path.clone();
                    let metadata = Arc::clone(&self.metadata);
                    let columns = Arc::clone(&self.columns);
                    let selected = plan.projected_column_ids.clone();
                    handles.push(thread::spawn(move || {
                        read_row_group_with_attribution(
                            &path,
                            &metadata,
                            &columns,
                            row_group_id,
                            &selected,
                        )
                    }));
                }
                let mut wave_batches = Vec::with_capacity(handles.len());
                let mut first_error = None;
                for handle in handles {
                    match handle.join() {
                        Ok(Ok((batch, row_attr))) => {
                            accumulator.add(row_attr);
                            wave_batches.push(batch);
                        }
                        Ok(Err(err)) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                        Err(_) => {
                            if first_error.is_none() {
                                first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                    "OCB read worker panicked",
                                )));
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    return Err(err);
                }
                wave_batches
            };
            cursor_report.max_in_flight_row_groups_observed = cursor_report
                .max_in_flight_row_groups_observed
                .max(wave_batches.len());
            for batch in wave_batches {
                let row_count = batch.row_count;
                let callback_started = Instant::now();
                let control = visitor(batch);
                accumulator.callback += callback_started.elapsed();
                match control? {
                    ColumnBundleVisitControl::Continue => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(row_count);
                        cursor_report.cancelled = true;
                        let attribution = attribution_from_accumulator(
                            accumulator,
                            &plan.report,
                            plan_ns,
                            duration_to_ns(execute_started.elapsed()),
                        );
                        return Ok(ColumnBundleReadAttributedCursorReport {
                            cursor_report,
                            attribution,
                        });
                    }
                }
            }
        }
        let attribution = attribution_from_accumulator(
            accumulator,
            &plan.report,
            plan_ns,
            duration_to_ns(execute_started.elapsed()),
        );
        Ok(ColumnBundleReadAttributedCursorReport {
            cursor_report,
            attribution,
        })
    }

    fn visit_execution_plan_into<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        mut visitor: F,
    ) -> Result<ColumnBundleReadCursorReport>
    where
        F: FnMut(ColumnBundleReusableBatchView<'_>) -> Result<ColumnBundleVisitControl>,
    {
        let mut cursor_report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            return Ok(cursor_report);
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1))
            .min(buffers.len());
        for wave in plan.row_group_ids.chunks(wave_size) {
            let mut reports = Vec::with_capacity(wave.len());
            thread::scope(|scope| {
                let mut handles = Vec::with_capacity(wave.len());
                for (slot, row_group_id) in buffers.buffers[..wave.len()]
                    .iter_mut()
                    .zip(wave.iter().copied())
                {
                    let path = self.path.as_path();
                    let metadata = &self.metadata;
                    let columns = &self.columns;
                    handles.push(scope.spawn(move || {
                        read_row_group_into_reusable(path, metadata, columns, row_group_id, slot)
                    }));
                }
                let mut first_error = None;
                for handle in handles {
                    match handle.join() {
                        Ok(Ok(report)) => reports.push(report),
                        Ok(Err(err)) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                        Err(_) => {
                            if first_error.is_none() {
                                first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                    "OCB reusable read worker panicked",
                                )));
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    return Err(err);
                }
                Ok(())
            })?;
            cursor_report.max_in_flight_row_groups_observed = cursor_report
                .max_in_flight_row_groups_observed
                .max(reports.len());
            for (slot, report) in buffers.buffers.iter().zip(reports.iter()) {
                let view = ColumnBundleReusableBatchView {
                    report,
                    buffers: slot,
                };
                match visitor(view)? {
                    ColumnBundleVisitControl::Continue => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                        cursor_report.cancelled = true;
                        return Ok(cursor_report);
                    }
                }
            }
        }
        Ok(cursor_report)
    }

    fn visit_execution_plan_into_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        plan_ns: u64,
        mut visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(ColumnBundleReusableBatchView<'_>) -> Result<ColumnBundleVisitControl>,
    {
        let execute_started = Instant::now();
        let mut accumulator = ReadAttributionAccumulator::default();
        let mut cursor_report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            let attribution = attribution_from_accumulator(
                accumulator,
                &plan.report,
                plan_ns,
                duration_to_ns(execute_started.elapsed()),
            );
            return Ok(ColumnBundleReadAttributedCursorReport {
                cursor_report,
                attribution,
            });
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1))
            .min(buffers.len());
        for wave in plan.row_group_ids.chunks(wave_size) {
            let mut reports = Vec::with_capacity(wave.len());
            thread::scope(|scope| {
                let mut handles = Vec::with_capacity(wave.len());
                for (slot, row_group_id) in buffers.buffers[..wave.len()]
                    .iter_mut()
                    .zip(wave.iter().copied())
                {
                    let path = self.path.as_path();
                    let metadata = &self.metadata;
                    let columns = &self.columns;
                    handles.push(scope.spawn(move || {
                        read_row_group_into_reusable_with_attribution(
                            path,
                            metadata,
                            columns,
                            row_group_id,
                            slot,
                        )
                    }));
                }
                let mut first_error = None;
                for handle in handles {
                    match handle.join() {
                        Ok(Ok((report, row_attr))) => {
                            accumulator.add(row_attr);
                            reports.push(report);
                        }
                        Ok(Err(err)) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                        Err(_) => {
                            if first_error.is_none() {
                                first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                    "OCB reusable read worker panicked",
                                )));
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    return Err(err);
                }
                Ok(())
            })?;
            cursor_report.max_in_flight_row_groups_observed = cursor_report
                .max_in_flight_row_groups_observed
                .max(reports.len());
            for (slot, report) in buffers.buffers.iter().zip(reports.iter()) {
                let view = ColumnBundleReusableBatchView {
                    report,
                    buffers: slot,
                };
                let callback_started = Instant::now();
                let control = visitor(view);
                accumulator.callback += callback_started.elapsed();
                match control? {
                    ColumnBundleVisitControl::Continue => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                        cursor_report.cancelled = true;
                        let attribution = attribution_from_accumulator(
                            accumulator,
                            &plan.report,
                            plan_ns,
                            duration_to_ns(execute_started.elapsed()),
                        );
                        return Ok(ColumnBundleReadAttributedCursorReport {
                            cursor_report,
                            attribution,
                        });
                    }
                }
            }
        }
        let attribution = attribution_from_accumulator(
            accumulator,
            &plan.report,
            plan_ns,
            duration_to_ns(execute_started.elapsed()),
        );
        Ok(ColumnBundleReadAttributedCursorReport {
            cursor_report,
            attribution,
        })
    }

    fn visit_execution_plan_project_fixed_binary_with_attribution<F>(
        &self,
        plan: &ColumnBundleReadPlan,
        cursor_options: ColumnBundleReadCursorOptions,
        buffers: &mut ColumnBundleReusableBufferPool,
        projection: &FixedBinaryRecordProjection,
        projection_buffer: &mut ColumnBundleFixedBinaryProjectionBuffer,
        mut visitor: F,
    ) -> Result<ColumnBundleReadAttributedCursorReport>
    where
        F: FnMut(
            ColumnBundleReusableBatchView<'_>,
            FixedBinaryProjectedBatchView<'_>,
        ) -> Result<ColumnBundleVisitControl>,
    {
        let execute_started = Instant::now();
        let mut accumulator = ReadAttributionAccumulator::default();
        let mut cursor_report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            let attribution = attribution_from_accumulator(
                accumulator,
                &plan.report,
                0,
                duration_to_ns(execute_started.elapsed()),
            );
            return Ok(ColumnBundleReadAttributedCursorReport {
                cursor_report,
                attribution,
            });
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1))
            .min(buffers.len());
        for wave in plan.row_group_ids.chunks(wave_size) {
            let mut reports = Vec::with_capacity(wave.len());
            thread::scope(|scope| {
                let mut handles = Vec::with_capacity(wave.len());
                for (slot, row_group_id) in buffers.buffers[..wave.len()]
                    .iter_mut()
                    .zip(wave.iter().copied())
                {
                    let path = self.path.as_path();
                    let metadata = &self.metadata;
                    let columns = &self.columns;
                    handles.push(scope.spawn(move || {
                        read_row_group_into_reusable_with_attribution(
                            path,
                            metadata,
                            columns,
                            row_group_id,
                            slot,
                        )
                    }));
                }
                let mut first_error = None;
                for handle in handles {
                    match handle.join() {
                        Ok(Ok((report, row_attr))) => {
                            accumulator.add(row_attr);
                            reports.push(report);
                        }
                        Ok(Err(err)) => {
                            if first_error.is_none() {
                                first_error = Some(err);
                            }
                        }
                        Err(_) => {
                            if first_error.is_none() {
                                first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                    "OCB fixed-binary projection read worker panicked",
                                )));
                            }
                        }
                    }
                }
                if let Some(err) = first_error {
                    return Err(err);
                }
                Ok(())
            })?;
            cursor_report.max_in_flight_row_groups_observed = cursor_report
                .max_in_flight_row_groups_observed
                .max(reports.len());
            for (slot, report) in buffers.buffers.iter().zip(reports.iter()) {
                let view = ColumnBundleReusableBatchView {
                    report,
                    buffers: slot,
                };
                let projection_report =
                    project_fixed_binary_reusable_batch(&view, projection, projection_buffer)?;
                accumulator.fixed_payload_decode +=
                    Duration::from_nanos(projection_report.projection_wall_ns);
                let projected_view = projection_buffer.view(report.row_group_id, report.base_row);
                let callback_started = Instant::now();
                let control = visitor(view, projected_view);
                accumulator.callback += callback_started.elapsed();
                match control? {
                    ColumnBundleVisitControl::Continue => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        cursor_report.batches_yielded =
                            cursor_report.batches_yielded.saturating_add(1);
                        cursor_report.rows_yielded =
                            cursor_report.rows_yielded.saturating_add(report.row_count);
                        cursor_report.cancelled = true;
                        let attribution = attribution_from_accumulator(
                            accumulator,
                            &plan.report,
                            0,
                            duration_to_ns(execute_started.elapsed()),
                        );
                        return Ok(ColumnBundleReadAttributedCursorReport {
                            cursor_report,
                            attribution,
                        });
                    }
                }
            }
        }
        let attribution = attribution_from_accumulator(
            accumulator,
            &plan.report,
            0,
            duration_to_ns(execute_started.elapsed()),
        );
        Ok(ColumnBundleReadAttributedCursorReport {
            cursor_report,
            attribution,
        })
    }

    fn execute_plan(&self, plan: &ColumnBundleReadPlan) -> Result<Vec<ColumnBatch>> {
        if plan.row_group_ids.is_empty() {
            return Ok(Vec::new());
        }
        if plan.report.effective_threads <= 1 {
            return plan
                .row_group_ids
                .iter()
                .map(|row_group_id| {
                    read_row_group(
                        &self.path,
                        &self.metadata,
                        &self.columns,
                        *row_group_id,
                        &plan.projected_column_ids,
                    )
                })
                .collect();
        }

        let mut batches = Vec::with_capacity(plan.row_group_ids.len());
        for wave in plan.row_group_ids.chunks(plan.report.effective_threads) {
            let mut handles = Vec::with_capacity(wave.len());
            for row_group_id in wave.iter().copied() {
                let path = self.path.clone();
                let metadata = Arc::clone(&self.metadata);
                let columns = Arc::clone(&self.columns);
                let selected = plan.projected_column_ids.clone();
                handles.push(thread::spawn(move || {
                    read_row_group(&path, &metadata, &columns, row_group_id, &selected)
                }));
            }

            let mut wave_batches = Vec::with_capacity(handles.len());
            let mut first_error = None;
            for handle in handles {
                match handle.join() {
                    Ok(Ok(batch)) => wave_batches.push(batch),
                    Ok(Err(err)) => {
                        if first_error.is_none() {
                            first_error = Some(err);
                        }
                    }
                    Err(_) => {
                        if first_error.is_none() {
                            first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                "OCB read worker panicked",
                            )));
                        }
                    }
                }
            }
            if let Some(err) = first_error {
                return Err(err);
            }
            batches.extend(wave_batches);
        }
        Ok(batches)
    }

    fn execute_plan_with_attribution(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<(Vec<ColumnBatch>, ReadAttributionAccumulator)> {
        if plan.row_group_ids.is_empty() {
            return Ok((Vec::new(), ReadAttributionAccumulator::default()));
        }
        if plan.report.effective_threads <= 1 {
            let mut batches = Vec::with_capacity(plan.row_group_ids.len());
            let mut attribution = ReadAttributionAccumulator::default();
            for row_group_id in &plan.row_group_ids {
                let (batch, row_attr) = read_row_group_with_attribution(
                    &self.path,
                    &self.metadata,
                    &self.columns,
                    *row_group_id,
                    &plan.projected_column_ids,
                )?;
                attribution.add(row_attr);
                batches.push(batch);
            }
            return Ok((batches, attribution));
        }

        let mut batches = Vec::with_capacity(plan.row_group_ids.len());
        let mut attribution = ReadAttributionAccumulator::default();
        for wave in plan.row_group_ids.chunks(plan.report.effective_threads) {
            let mut handles = Vec::with_capacity(wave.len());
            for row_group_id in wave.iter().copied() {
                let path = self.path.clone();
                let metadata = Arc::clone(&self.metadata);
                let columns = Arc::clone(&self.columns);
                let selected = plan.projected_column_ids.clone();
                handles.push(thread::spawn(move || {
                    read_row_group_with_attribution(
                        &path,
                        &metadata,
                        &columns,
                        row_group_id,
                        &selected,
                    )
                }));
            }

            let mut wave_batches = Vec::with_capacity(handles.len());
            let mut first_error = None;
            for handle in handles {
                match handle.join() {
                    Ok(Ok((batch, row_attr))) => {
                        attribution.add(row_attr);
                        wave_batches.push(batch);
                    }
                    Ok(Err(err)) => {
                        if first_error.is_none() {
                            first_error = Some(err);
                        }
                    }
                    Err(_) => {
                        if first_error.is_none() {
                            first_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                                "OCB read worker panicked",
                            )));
                        }
                    }
                }
            }
            if let Some(err) = first_error {
                return Err(err);
            }
            batches.extend(wave_batches);
        }
        Ok((batches, attribution))
    }

    fn validate_read_plan(&self, plan: &ColumnBundleReadPlan) -> Result<()> {
        let available_columns = self
            .columns
            .iter()
            .map(|column| column.id)
            .collect::<BTreeSet<_>>();
        let mut seen_columns = BTreeSet::new();
        for column_id in &plan.projected_column_ids {
            if !available_columns.contains(column_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB read plan references an unknown projected column id",
                ));
            }
            if !seen_columns.insert(*column_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB read plan contains duplicate projected column ids",
                ));
            }
        }

        let available_row_groups = self
            .metadata
            .row_group_index
            .row_groups
            .iter()
            .map(|row_group| row_group.row_group_id)
            .collect::<BTreeSet<_>>();
        let mut seen_row_groups = BTreeSet::new();
        for row_group_id in &plan.row_group_ids {
            if !available_row_groups.contains(row_group_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB read plan references an unknown row group id",
                ));
            }
            if !seen_row_groups.insert(*row_group_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB read plan contains duplicate row group ids",
                ));
            }
        }
        Ok(())
    }

    fn build_row_group_summaries<I>(
        &self,
        row_group_ids: I,
        projected_column_ids: Option<&BTreeSet<u32>>,
    ) -> Result<Vec<ColumnBundleRowGroupSummary>>
    where
        I: IntoIterator<Item = u32>,
    {
        let by_id = self
            .metadata
            .row_group_index
            .row_groups
            .iter()
            .map(|row_group| (row_group.row_group_id, row_group))
            .collect::<BTreeMap<_, _>>();
        let mut summaries = Vec::new();
        let mut seen = BTreeSet::new();
        for row_group_id in row_group_ids {
            if !seen.insert(row_group_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB row-group summary request contains duplicate row group ids",
                ));
            }
            let row_group =
                by_id
                    .get(&row_group_id)
                    .copied()
                    .ok_or(ArcadiaTioError::ocb_invalid_input(
                        "OCB row-group summary request references an unknown row group id",
                    ))?;
            summaries.push(self.build_row_group_summary(row_group, projected_column_ids)?);
        }
        Ok(summaries)
    }

    fn build_row_group_summary(
        &self,
        row_group: &OcbRowGroupDescV1,
        projected_column_ids: Option<&BTreeSet<u32>>,
    ) -> Result<ColumnBundleRowGroupSummary> {
        let chunks = column_chunks_for_row_group(
            &self.metadata,
            row_group.chunk_desc_begin,
            row_group.chunk_desc_count,
        )?;
        let mut chunk_summaries = Vec::new();
        for chunk in chunks {
            if chunk.row_group_id != row_group.row_group_id {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB column chunk references a different row group",
                ));
            }
            if projected_column_ids.is_some_and(|selected| !selected.contains(&chunk.column_id)) {
                continue;
            }
            let column = self.column_by_id(chunk.column_id)?;
            let physical_type = public_physical_type_from_chunk(chunk.physical_type, column)?;
            let fixed_binary_width = match physical_type {
                ColumnPhysicalType::FixedBinary { width } => Some(width),
                _ => None,
            };
            let value_ref = checked_body_ref_summary(
                chunk.value_ref,
                OcbBodyKindV1::ColumnChunk,
                self.metadata.file_len,
            )?;
            let validity_ref = checked_optional_body_ref_summary(
                chunk.validity_ref,
                OcbBodyKindV1::ValidityBitmap,
                self.metadata.file_len,
            )?;
            chunk_summaries.push(ColumnBundleColumnChunkSummary {
                row_group_id: chunk.row_group_id,
                column_id: chunk.column_id,
                column_name: column.name.clone(),
                physical_type,
                logical_kind: column.logical_kind,
                fixed_binary_width,
                codec: chunk.codec.into(),
                row_count: chunk.row_count,
                compressed_bytes: column_chunk_compressed_payload_bytes(chunk.value_ref)?,
                uncompressed_bytes: chunk.uncompressed_bytes,
                value_ref,
                validity_ref,
            });
        }

        let stats =
            stats_for_row_group(&self.metadata, row_group.stat_begin, row_group.stat_count)?;
        let mut stat_summaries = Vec::with_capacity(stats.len());
        for stat in stats {
            if stat.row_group_id != row_group.row_group_id {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row-group stat references a different row group",
                ));
            }
            let column = self.column_by_id(stat.column_id)?;
            let physical_type = scalar_column_physical_type(stat.physical_type)?;
            if physical_type != column.physical_type {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB row-group stat dtype does not match column dtype",
                ));
            }
            stat_summaries.push(ColumnBundleColumnStatsSummary {
                row_group_id: stat.row_group_id,
                column_id: stat.column_id,
                column_name: column.name.clone(),
                physical_type,
                null_count: stat.null_count,
                min: ColumnPredicateValue::from_stat(stat.min_value),
                max: ColumnPredicateValue::from_stat(stat.max_value),
            });
        }

        Ok(ColumnBundleRowGroupSummary {
            row_group_id: row_group.row_group_id,
            base_row: row_group.base_row,
            row_count: row_group.row_count,
            first_key_tuple_ref: checked_optional_body_ref_summary(
                row_group.first_key_tuple_ref,
                OcbBodyKindV1::KeyTuple,
                self.metadata.file_len,
            )?,
            last_key_tuple_ref: checked_optional_body_ref_summary(
                row_group.last_key_tuple_ref,
                OcbBodyKindV1::KeyTuple,
                self.metadata.file_len,
            )?,
            chunks: chunk_summaries,
            stats: stat_summaries,
        })
    }

    fn column_by_id(&self, column_id: u32) -> Result<&BundleColumn> {
        self.columns
            .iter()
            .find(|column| column.id == column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB metadata references an unknown column id",
            ))
    }

    fn require_predicate_stats_available(
        &self,
        predicates: &[ResolvedRowGroupPredicate],
    ) -> Result<()> {
        if predicates.is_empty() {
            return Ok(());
        }
        for row_group in &self.metadata.row_group_index.row_groups {
            let stats_by_column = stats_by_column_for_row_group(&self.metadata, row_group)?;
            for predicate in predicates {
                let Some(stat) = stats_by_column.get(&predicate.column_id) else {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB strict read planning requires row-group stats for every predicate column",
                    ));
                };
                if scalar_column_physical_type(stat.physical_type)? != predicate.physical_type {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB stat dtype does not match predicate column dtype",
                    ));
                }
            }
        }
        Ok(())
    }

    fn resolve_projection(&self, projection: &ColumnProjection) -> Result<Vec<u32>> {
        match projection {
            ColumnProjection::All => Ok(self.columns.iter().map(|column| column.id).collect()),
            ColumnProjection::Names(names) => {
                let by_name = self
                    .columns
                    .iter()
                    .map(|column| (column.name.as_str(), column.id))
                    .collect::<BTreeMap<_, _>>();
                let mut selected = Vec::with_capacity(names.len());
                let mut seen = BTreeSet::new();
                for name in names {
                    let Some(column_id) = by_name.get(name.as_str()) else {
                        return Err(ArcadiaTioError::ocb_invalid_input(
                            "OCB projection references an unknown column",
                        ));
                    };
                    if !seen.insert(*column_id) {
                        return Err(ArcadiaTioError::ocb_invalid_input(
                            "OCB projection contains duplicate columns",
                        ));
                    }
                    selected.push(*column_id);
                }
                Ok(selected)
            }
        }
    }

    fn resolve_predicates(
        &self,
        predicates: &[RowGroupPredicate],
    ) -> Result<Vec<ResolvedRowGroupPredicate>> {
        let by_name = self
            .columns
            .iter()
            .map(|column| (column.name.as_str(), column))
            .collect::<BTreeMap<_, _>>();
        let mut resolved = Vec::with_capacity(predicates.len());
        for predicate in predicates {
            let Some(column) = by_name.get(predicate.column.as_str()) else {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB predicate references an unknown column",
                ));
            };
            if predicate.lower.is_none() && predicate.upper.is_none() {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB predicate must include at least one bound",
                ));
            }
            if matches!(column.physical_type, ColumnPhysicalType::FixedBinary { .. }) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB predicates over fixed-binary columns are not supported",
                ));
            }
            for bound in [predicate.lower, predicate.upper].into_iter().flatten() {
                if bound.physical_type() != column.physical_type {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB predicate bound dtype does not match column dtype",
                    ));
                }
            }
            if let (Some(lower), Some(upper)) = (predicate.lower, predicate.upper) {
                if lower.cmp_same_type(upper)? == Ordering::Greater {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB predicate lower bound is greater than upper bound",
                    ));
                }
            }
            resolved.push(ResolvedRowGroupPredicate {
                column_id: column.id,
                physical_type: column.physical_type,
                lower: predicate.lower,
                upper: predicate.upper,
            });
        }
        Ok(resolved)
    }
}

fn decode_dictionary_values(raw: OcbDictionaryValuesV1) -> Result<DictionaryValues> {
    match raw.value_kind {
        OcbDictionaryValueKindV1::Utf8 => raw
            .values
            .into_iter()
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file("OCB UTF-8 dictionary value is invalid")
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(DictionaryValues::Utf8),
        OcbDictionaryValueKindV1::Bytes => Ok(DictionaryValues::Bytes(raw.values)),
        OcbDictionaryValueKindV1::FixedBytes => Ok(DictionaryValues::FixedBytes {
            fixed_width: raw.fixed_width,
            values: raw.values,
        }),
        OcbDictionaryValueKindV1::EnumLabels => raw
            .values
            .into_iter()
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file("OCB enum-label dictionary value is invalid")
                })
            })
            .collect::<Result<Vec<_>>>()
            .map(DictionaryValues::EnumLabels),
    }
}

fn validate_read_cursor_options(options: ColumnBundleReadCursorOptions) -> Result<()> {
    if options.max_in_flight_row_groups == 0 {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB read cursor max_in_flight_row_groups must be greater than zero",
        ));
    }
    if !options.ordered {
        return Err(ArcadiaTioError::Unimplemented(
            "OCB read cursor unordered mode is not implemented yet",
        ));
    }
    Ok(())
}

fn projected_columns_for_plan(
    columns: &[BundleColumn],
    plan: &ColumnBundleReadPlan,
) -> Result<Vec<BundleColumn>> {
    plan.projected_column_ids
        .iter()
        .map(|column_id| {
            columns
                .iter()
                .find(|column| column.id == *column_id)
                .cloned()
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB selected column not found",
                ))
        })
        .collect()
}

fn validate_reusable_buffer_pool(
    pool: &ColumnBundleReusableBufferPool,
    plan: &ColumnBundleReadPlan,
    columns: &[BundleColumn],
) -> Result<()> {
    if pool.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB reusable visitor requires at least one buffer slot",
        ));
    }
    let projected_columns = projected_columns_for_plan(columns, plan)?;
    if projected_columns.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB reusable visitor requires at least one projected column",
        ));
    }
    for slot in &pool.buffers {
        if slot.columns.len() != projected_columns.len() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable visitor buffer slot does not match plan projection",
            ));
        }
        for (buffer, column) in slot.columns.iter().zip(projected_columns.iter()) {
            if buffer.column_id != column.id
                || buffer.name != column.name
                || buffer.physical_type != column.physical_type
                || buffer.logical_kind != column.logical_kind
                || buffer.dictionary_id != column.dictionary_id
                || buffer.nullable != column.nullable
                || buffer.values.physical_type() != column.physical_type
            {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable visitor buffer column does not match plan projection",
                ));
            }
        }
    }
    Ok(())
}

fn validate_fixed_binary_record_projection<'a>(
    columns: &'a [BundleColumn],
    plan: &ColumnBundleReadPlan,
    projection: &FixedBinaryRecordProjection,
) -> Result<&'a BundleColumn> {
    if projection.expected_width == 0 {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection expected_width must be greater than zero",
        ));
    }
    if projection.fields.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection requires at least one field",
        ));
    }
    if projection.column_id.is_none() && projection.column_name.is_none() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection must identify a source column",
        ));
    }
    let by_id_column = match projection.column_id {
        Some(column_id) => Some(columns.iter().find(|column| column.id == column_id).ok_or(
            ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection references an unknown column id",
            ),
        )?),
        None => None,
    };
    let by_name_column = match projection.column_name.as_deref() {
        Some(column_name) => Some(
            columns
                .iter()
                .find(|column| column.name == column_name)
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary projection references an unknown column name",
                ))?,
        ),
        None => None,
    };
    let source_column = match (by_id_column, by_name_column) {
        (Some(left), Some(right)) if left.id != right.id => {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection column name and id do not match",
            ));
        }
        (Some(column), _) | (_, Some(column)) => column,
        (None, None) => unreachable!("source identity checked above"),
    };
    if !plan.projected_column_ids.contains(&source_column.id) {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection source column is not in the read plan projection",
        ));
    }
    match source_column.physical_type {
        ColumnPhysicalType::FixedBinary { width } if width == projection.expected_width => {}
        ColumnPhysicalType::FixedBinary { .. } => {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection expected_width does not match source column",
            ));
        }
        _ => {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection source column must be fixed-binary",
            ));
        }
    }
    if source_column.nullable && !projection.allow_nulls {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection source column is nullable",
        ));
    }
    let width = usize::try_from(projection.expected_width).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection expected_width does not fit usize",
        )
    })?;
    for field in &projection.fields {
        let offset = usize::try_from(field.offset).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection field offset does not fit usize",
            )
        })?;
        let end = offset.checked_add(field.field_type.byte_width()).ok_or(
            ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection field end offset overflows",
            ),
        )?;
        if end > width {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection field extends past record width",
            ));
        }
    }
    Ok(source_column)
}

fn validate_fixed_binary_projection_buffer(
    buffer: &ColumnBundleFixedBinaryProjectionBuffer,
    projection: &FixedBinaryRecordProjection,
) -> Result<()> {
    if projection
        .column_id
        .map(|column_id| buffer.source_column_id != column_id)
        .unwrap_or(false)
        || projection
            .column_name
            .as_deref()
            .map(|column_name| buffer.source_column_name != column_name)
            .unwrap_or(false)
        || buffer.source_width != projection.expected_width
        || buffer.fields.len() != projection.fields.len()
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection buffer does not match projection",
        ));
    }
    for (spec, field) in projection.fields.iter().zip(buffer.fields.iter()) {
        if spec.offset != field.offset || spec.field_type != field.values.field_type() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection buffer field does not match projection",
            ));
        }
    }
    Ok(())
}

fn project_fixed_binary_reusable_batch(
    view: &ColumnBundleReusableBatchView<'_>,
    projection: &FixedBinaryRecordProjection,
    projection_buffer: &mut ColumnBundleFixedBinaryProjectionBuffer,
) -> Result<FixedBinaryProjectionReport> {
    let mut source_column = None;
    for index in 0..view.column_count() {
        let column = view.column(index)?;
        let id_matches = projection
            .column_id
            .map(|column_id| column_id == column.column_id)
            .unwrap_or(false);
        let name_matches = projection
            .column_name
            .as_deref()
            .map(|column_name| column_name == column.name)
            .unwrap_or(false);
        if id_matches || name_matches {
            source_column = Some(column);
            break;
        }
    }
    let source_column = source_column.ok_or(ArcadiaTioError::ocb_invalid_input(
        "OCB fixed-binary projection source column is not in the batch",
    ))?;
    if !projection.allow_nulls && source_column.validity.is_some() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection source column contains nulls",
        ));
    }
    if projection_buffer.source_column_id != source_column.column_id
        || projection_buffer.source_column_name != source_column.name
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection buffer source column does not match batch",
        ));
    }
    let records = source_column.values.fixed_binary_records()?;
    if records.width != projection.expected_width {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection source width does not match projection",
        ));
    }
    projection_buffer.project_records(records, projection)
}

fn max_row_count_for_plan(metadata: &OcbMetadataV1, plan: &ColumnBundleReadPlan) -> Result<usize> {
    let mut max_rows = 0usize;
    for row_group_id in &plan.row_group_ids {
        let row_group = metadata
            .row_group_index
            .row_groups
            .iter()
            .find(|row_group| row_group.row_group_id == *row_group_id)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable buffer plan references an unknown row group",
            ))?;
        let row_count = usize::try_from(row_group.row_count).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB reusable buffer row count does not fit usize")
        })?;
        max_rows = max_rows.max(row_count);
    }
    Ok(max_rows)
}

fn validate_read_fill_options(options: ColumnBundleReadFillOptions) -> Result<()> {
    if !options.validate_checksums {
        return Err(ArcadiaTioError::Unimplemented(
            "OCB fill reads currently always validate checksums",
        ));
    }
    Ok(())
}

fn validate_read_options(options: &ColumnBundleReadOptions) -> Result<()> {
    if !options.validate_checksums {
        return Err(ArcadiaTioError::Unimplemented(
            "OCB reads currently always validate checksums",
        ));
    }
    if options.decode_dictionaries {
        return Err(ArcadiaTioError::Unimplemented(
            "OCB dictionary value decoding is not implemented yet",
        ));
    }
    Ok(())
}

fn build_read_report(
    requested_threads: usize,
    selected_row_groups: usize,
    pruned_row_groups: usize,
    selected_columns: usize,
) -> ColumnBundleReadReport {
    let requested_cap = requested_threads.max(1);
    let (effective_threads, fallback_reason) = if requested_cap <= 1 {
        (1, Some(OCB_FALLBACK_THREAD_CAP_ONE))
    } else if selected_row_groups <= 1 {
        (1, Some(OCB_FALLBACK_TOO_FEW_ROW_GROUPS))
    } else {
        (requested_cap.min(selected_row_groups), None)
    };
    ColumnBundleReadReport {
        requested_threads,
        effective_threads,
        selected_row_groups,
        pruned_row_groups,
        selected_column_chunks: selected_row_groups.saturating_mul(selected_columns),
        fallback_reason,
    }
}

fn execution_report_for_plan(
    plan: &ColumnBundleReadPlan,
    selected_row_groups: usize,
) -> ColumnBundleReadReport {
    build_read_report(
        plan.report.requested_threads,
        selected_row_groups,
        plan.report.pruned_row_groups,
        plan.projected_column_ids.len(),
    )
}

fn planned_row_group_subset(
    plan: &ColumnBundleReadPlan,
    row_group_ids: &[u32],
) -> Result<Vec<u32>> {
    let planned = plan.row_group_ids.iter().copied().collect::<BTreeSet<_>>();
    let mut requested = BTreeSet::new();
    for row_group_id in row_group_ids {
        if !planned.contains(row_group_id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR,
            ));
        }
        if !requested.insert(*row_group_id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR,
            ));
        }
    }
    Ok(plan
        .row_group_ids
        .iter()
        .copied()
        .filter(|row_group_id| requested.contains(row_group_id))
        .collect())
}

fn public_physical_type_from_chunk(
    physical_type: OcbPhysicalTypeV1,
    column: &BundleColumn,
) -> Result<ColumnPhysicalType> {
    match (physical_type, column.physical_type) {
        (OcbPhysicalTypeV1::I32, ColumnPhysicalType::I32) => Ok(ColumnPhysicalType::I32),
        (OcbPhysicalTypeV1::I64, ColumnPhysicalType::I64) => Ok(ColumnPhysicalType::I64),
        (OcbPhysicalTypeV1::F32, ColumnPhysicalType::F32) => Ok(ColumnPhysicalType::F32),
        (OcbPhysicalTypeV1::F64, ColumnPhysicalType::F64) => Ok(ColumnPhysicalType::F64),
        (OcbPhysicalTypeV1::FixedBinary, ColumnPhysicalType::FixedBinary { width }) => {
            Ok(ColumnPhysicalType::FixedBinary { width })
        }
        _ => Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk dtype does not match schema column dtype",
        )),
    }
}

fn body_ref_summary(body_ref: OcbBodyRefV2) -> ColumnBundleBodyRefSummary {
    ColumnBundleBodyRefSummary {
        offset: body_ref.offset,
        length: body_ref.length,
        kind: body_ref.kind.into(),
        flags: body_ref.flags,
        checksum_kind: body_ref.checksum_kind.into(),
        checksum: body_ref.checksum,
    }
}

fn checked_body_ref_summary(
    body_ref: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    file_len: u64,
) -> Result<ColumnBundleBodyRefSummary> {
    body_ref.validate(expected_kind, file_len)?;
    Ok(body_ref_summary(body_ref))
}

fn checked_optional_body_ref_summary(
    body_ref: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    file_len: u64,
) -> Result<Option<ColumnBundleBodyRefSummary>> {
    if body_ref.is_null() {
        return Ok(None);
    }
    checked_body_ref_summary(body_ref, expected_kind, file_len).map(Some)
}

fn snapshot_fingerprint_for_summaries(
    metadata: &ColumnBundleMetadata,
    row_groups: &[ColumnBundleRowGroupSummary],
) -> ColumnBundleSnapshotFingerprint {
    let schema = fingerprint_schema(metadata);
    let dictionaries = fingerprint_dictionaries(metadata);
    let ordering = fingerprint_ordering(metadata);
    let row_groups_fp = fingerprint_row_group_summaries(row_groups);
    let mut combined = Vec::new();
    fp_str(&mut combined, OCB_CERTIFICATION_FINGERPRINT_ALGORITHM);
    fp_str(&mut combined, &schema);
    fp_str(&mut combined, &dictionaries);
    fp_str(&mut combined, &ordering);
    fp_str(&mut combined, &row_groups_fp);
    ColumnBundleSnapshotFingerprint {
        algorithm: OCB_CERTIFICATION_FINGERPRINT_ALGORITHM,
        schema,
        dictionaries,
        ordering,
        row_groups: row_groups_fp,
        combined: fp_hex(&combined),
    }
}

fn fingerprint_schema(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "schema");
    fp_str(&mut bytes, metadata.format_name);
    fp_bool(&mut bytes, metadata.appendable);
    fp_u64(&mut bytes, metadata.row_count);
    fp_u32(&mut bytes, metadata.row_group_count);
    fp_u32(&mut bytes, metadata.column_chunk_count);
    fp_u32(&mut bytes, metadata.columns.len() as u32);
    for column in &metadata.columns {
        fp_u32(&mut bytes, column.id);
        fp_str(&mut bytes, &column.name);
        fp_column_physical_type(&mut bytes, column.physical_type);
        fp_column_logical_kind(&mut bytes, column.logical_kind);
        fp_option_u32(&mut bytes, column.dictionary_id);
        fp_i32(&mut bytes, column.scale);
        fp_bool(&mut bytes, column.nullable);
    }
    fp_hex(&bytes)
}

fn fingerprint_dictionaries(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "dictionaries");
    fp_u32(&mut bytes, metadata.dictionaries.len() as u32);
    for dictionary in &metadata.dictionaries {
        fp_u32(&mut bytes, dictionary.dictionary_id);
        fp_str(&mut bytes, &dictionary.name);
        fp_column_physical_type(&mut bytes, dictionary.code_physical_type);
        fp_dictionary_value_kind(&mut bytes, dictionary.value_kind);
        fp_u32(&mut bytes, dictionary.entry_count);
    }
    fp_hex(&bytes)
}

fn fingerprint_ordering(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "ordering");
    fp_u32(&mut bytes, metadata.ordering_keys.len() as u32);
    for key in &metadata.ordering_keys {
        fp_u32(&mut bytes, key.column_id);
        fp_str(&mut bytes, &key.column_name);
        fp_ordering_direction(&mut bytes, key.direction);
        fp_null_order(&mut bytes, key.null_order);
    }
    fp_hex(&bytes)
}

fn fingerprint_row_group_summaries(row_groups: &[ColumnBundleRowGroupSummary]) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "row_groups");
    fp_u32(&mut bytes, row_groups.len() as u32);
    for row_group in row_groups {
        fp_row_group_summary(&mut bytes, row_group);
    }
    fp_hex(&bytes)
}

fn fingerprint_selected_chunks(row_groups: &[ColumnBundleRowGroupSummary]) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "selected_chunks");
    fp_u32(&mut bytes, row_groups.len() as u32);
    for row_group in row_groups {
        fp_u32(&mut bytes, row_group.row_group_id);
        fp_u64(&mut bytes, row_group.base_row);
        fp_u64(&mut bytes, row_group.row_count);
        fp_u32(&mut bytes, row_group.chunks.len() as u32);
        for chunk in &row_group.chunks {
            fp_column_chunk_summary(&mut bytes, chunk);
        }
    }
    fp_hex(&bytes)
}

fn fp_row_group_summary(bytes: &mut Vec<u8>, row_group: &ColumnBundleRowGroupSummary) {
    fp_u32(bytes, row_group.row_group_id);
    fp_u64(bytes, row_group.base_row);
    fp_u64(bytes, row_group.row_count);
    fp_option_body_ref_summary(bytes, row_group.first_key_tuple_ref);
    fp_option_body_ref_summary(bytes, row_group.last_key_tuple_ref);
    fp_u32(bytes, row_group.chunks.len() as u32);
    for chunk in &row_group.chunks {
        fp_column_chunk_summary(bytes, chunk);
    }
    fp_u32(bytes, row_group.stats.len() as u32);
    for stat in &row_group.stats {
        fp_column_stats_summary(bytes, stat);
    }
}

fn fp_column_chunk_summary(bytes: &mut Vec<u8>, chunk: &ColumnBundleColumnChunkSummary) {
    fp_u32(bytes, chunk.row_group_id);
    fp_u32(bytes, chunk.column_id);
    fp_str(bytes, &chunk.column_name);
    fp_column_physical_type(bytes, chunk.physical_type);
    fp_column_logical_kind(bytes, chunk.logical_kind);
    fp_option_u32(bytes, chunk.fixed_binary_width);
    fp_chunk_codec(bytes, chunk.codec);
    fp_u64(bytes, chunk.row_count);
    fp_u64(bytes, chunk.compressed_bytes);
    fp_u64(bytes, chunk.uncompressed_bytes);
    fp_body_ref_summary(bytes, chunk.value_ref);
    fp_option_body_ref_summary(bytes, chunk.validity_ref);
}

fn fp_column_stats_summary(bytes: &mut Vec<u8>, stat: &ColumnBundleColumnStatsSummary) {
    fp_u32(bytes, stat.row_group_id);
    fp_u32(bytes, stat.column_id);
    fp_str(bytes, &stat.column_name);
    fp_column_physical_type(bytes, stat.physical_type);
    fp_u32(bytes, stat.null_count);
    fp_predicate_value(bytes, stat.min);
    fp_predicate_value(bytes, stat.max);
}

fn fp_body_ref_summary(bytes: &mut Vec<u8>, body_ref: ColumnBundleBodyRefSummary) {
    fp_u64(bytes, body_ref.offset);
    fp_u64(bytes, body_ref.length);
    fp_body_kind(bytes, body_ref.kind);
    fp_u16(bytes, body_ref.flags);
    fp_checksum_kind(bytes, body_ref.checksum_kind);
    fp_u32(bytes, body_ref.checksum);
}

fn fp_option_body_ref_summary(bytes: &mut Vec<u8>, value: Option<ColumnBundleBodyRefSummary>) {
    match value {
        Some(value) => {
            fp_u8(bytes, 1);
            fp_body_ref_summary(bytes, value);
        }
        None => fp_u8(bytes, 0),
    }
}

fn fp_column_physical_type(bytes: &mut Vec<u8>, value: ColumnPhysicalType) {
    match value {
        ColumnPhysicalType::I32 => fp_u8(bytes, 1),
        ColumnPhysicalType::I64 => fp_u8(bytes, 2),
        ColumnPhysicalType::F32 => fp_u8(bytes, 3),
        ColumnPhysicalType::F64 => fp_u8(bytes, 4),
        ColumnPhysicalType::FixedBinary { width } => {
            fp_u8(bytes, 5);
            fp_u32(bytes, width);
        }
    }
}

fn fp_column_logical_kind(bytes: &mut Vec<u8>, value: ColumnLogicalKind) {
    fp_u8(
        bytes,
        match value {
            ColumnLogicalKind::Plain => 0,
            ColumnLogicalKind::TimestampNanosLike => 1,
            ColumnLogicalKind::ScaledInteger => 2,
            ColumnLogicalKind::DictionaryCode => 3,
            ColumnLogicalKind::EnumCode => 4,
            ColumnLogicalKind::OpaqueKey => 5,
        },
    );
}

fn fp_dictionary_value_kind(bytes: &mut Vec<u8>, value: DictionaryValueKind) {
    fp_u8(
        bytes,
        match value {
            DictionaryValueKind::Utf8 => 1,
            DictionaryValueKind::Bytes => 2,
            DictionaryValueKind::FixedBytes => 3,
            DictionaryValueKind::EnumLabels => 4,
        },
    );
}

fn fp_ordering_direction(bytes: &mut Vec<u8>, value: BundleOrderingDirection) {
    fp_u8(
        bytes,
        match value {
            BundleOrderingDirection::Ascending => 1,
            BundleOrderingDirection::Descending => 2,
        },
    );
}

fn fp_null_order(bytes: &mut Vec<u8>, value: BundleNullOrder) {
    fp_u8(
        bytes,
        match value {
            BundleNullOrder::NullsFirst => 1,
            BundleNullOrder::NullsLast => 2,
            BundleNullOrder::NoNulls => 3,
        },
    );
}

fn fp_chunk_codec(bytes: &mut Vec<u8>, value: ColumnBundleColumnChunkSummaryCodec) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleColumnChunkSummaryCodec::None => 0,
            ColumnBundleColumnChunkSummaryCodec::Zstd => 1,
        },
    );
}

fn fp_body_kind(bytes: &mut Vec<u8>, value: ColumnBundleBodyKind) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleBodyKind::Unknown => 0,
            ColumnBundleBodyKind::Root => 1,
            ColumnBundleBodyKind::Schema => 2,
            ColumnBundleBodyKind::DictionaryIndex => 3,
            ColumnBundleBodyKind::DictionaryValues => 4,
            ColumnBundleBodyKind::RowGroupIndex => 5,
            ColumnBundleBodyKind::OrderingProof => 6,
            ColumnBundleBodyKind::ColumnChunk => 7,
            ColumnBundleBodyKind::StringTable => 8,
            ColumnBundleBodyKind::DebugJsonMetadata => 9,
            ColumnBundleBodyKind::ValidityBitmap => 10,
            ColumnBundleBodyKind::KeyTuple => 11,
            ColumnBundleBodyKind::RowGroupIndexDelta => 12,
        },
    );
}

fn fp_checksum_kind(bytes: &mut Vec<u8>, value: ColumnBundleChecksumKind) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleChecksumKind::None => 0,
            ColumnBundleChecksumKind::Crc32c => 1,
        },
    );
}

fn fp_predicate_value(bytes: &mut Vec<u8>, value: ColumnPredicateValue) {
    match value {
        ColumnPredicateValue::I32(value) => {
            fp_u8(bytes, 1);
            fp_i32(bytes, value);
        }
        ColumnPredicateValue::I64(value) => {
            fp_u8(bytes, 2);
            fp_i64(bytes, value);
        }
        ColumnPredicateValue::F32(value) => {
            fp_u8(bytes, 3);
            fp_u32(bytes, value.to_bits());
        }
        ColumnPredicateValue::F64(value) => {
            fp_u8(bytes, 4);
            fp_u64(bytes, value.to_bits());
        }
    }
}

fn fp_option_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            fp_u8(bytes, 1);
            fp_u32(bytes, value);
        }
        None => fp_u8(bytes, 0),
    }
}

fn fp_str(bytes: &mut Vec<u8>, value: &str) {
    fp_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn fp_bool(bytes: &mut Vec<u8>, value: bool) {
    fp_u8(bytes, u8::from(value));
}

fn fp_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn fp_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn fp_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn fp_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn fp_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn fp_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn fp_hex(bytes: &[u8]) -> String {
    format!("{:08x}", crc32c(bytes))
}

fn column_chunk_compressed_payload_bytes(value_ref: OcbBodyRefV2) -> Result<u64> {
    let object_overhead = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    value_ref
        .length
        .checked_sub(object_overhead)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk body reference is too short",
        ))
}

fn row_group_matches_predicates(
    metadata: &OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
    predicates: &[ResolvedRowGroupPredicate],
) -> Result<bool> {
    if predicates.is_empty() {
        return Ok(true);
    }
    let stats_by_column = stats_by_column_for_row_group(metadata, row_group)?;

    for predicate in predicates {
        let Some(stat) = stats_by_column.get(&predicate.column_id) else {
            // Missing stats are conservative: keep the row group.
            continue;
        };
        if scalar_column_physical_type(stat.physical_type)? != predicate.physical_type {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB stat dtype does not match predicate column dtype",
            ));
        }
        let min = ColumnPredicateValue::from_stat(stat.min_value);
        let max = ColumnPredicateValue::from_stat(stat.max_value);
        if let Some(lower) = predicate.lower {
            if max.cmp_same_type(lower)? == Ordering::Less {
                return Ok(false);
            }
        }
        if let Some(upper) = predicate.upper {
            if min.cmp_same_type(upper)? == Ordering::Greater {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

fn column_chunks_for_row_group(
    metadata: &OcbMetadataV1,
    begin: u64,
    count: u32,
) -> Result<&[OcbColumnChunkDescV1]> {
    let begin = usize::try_from(begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group chunk descriptor begin is too large")
    })?;
    let count = count as usize;
    let end = begin
        .checked_add(count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .column_chunks
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range is out of bounds",
        ))
}

fn stats_by_column_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<BTreeMap<u32, &'a OcbColumnStatsV1>> {
    let stats = stats_for_row_group(metadata, row_group.stat_begin, row_group.stat_count)?;
    let mut stats_by_column = BTreeMap::new();
    for stat in stats {
        if stat.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group stat references a different row group",
            ));
        }
        if stats_by_column.insert(stat.column_id, stat).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate stats for a column",
            ));
        }
    }
    Ok(stats_by_column)
}

fn stats_for_row_group(
    metadata: &OcbMetadataV1,
    begin: u64,
    count: u32,
) -> Result<&[OcbColumnStatsV1]> {
    let begin = usize::try_from(begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor begin is too large")
    })?;
    let count = count as usize;
    let end = begin
        .checked_add(count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .stats
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range is out of bounds",
        ))
}

fn validate_metadata(metadata: &OcbMetadataV1) -> Result<()> {
    if metadata.root.column_count as usize != metadata.schema.columns.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_count does not match schema",
        ));
    }
    if metadata.root.row_group_count as usize != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_group_count does not match row-group index",
        ));
    }
    if metadata.root.dictionary_count > 0 && metadata.dictionary_index.is_none() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root dictionary_count requires dictionary index",
        ));
    }
    if let Some(dictionary_index) = &metadata.dictionary_index {
        if metadata.root.dictionary_count as usize != dictionary_index.dictionaries.len() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root dictionary_count does not match dictionary index",
            ));
        }
    }
    if let Some(ordering_proof) = &metadata.ordering_proof {
        if ordering_proof.row_group_proofs.len() != metadata.row_group_index.row_groups.len() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof row-group count does not match row-group index",
            ));
        }
    }
    let total_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row_count overflows"))?;
    if total_rows != metadata.root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_count does not match row groups",
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ReadAttributionAccumulator {
    row_group_read: Duration,
    read_io: Duration,
    checksum: Duration,
    decompression: Duration,
    primitive_decode: Duration,
    fixed_payload_decode: Duration,
    copy_materialization: Duration,
    callback: Duration,
    row_groups_materialized: usize,
    column_chunks_materialized: usize,
    bytes_read: u64,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
}

impl ReadAttributionAccumulator {
    pub(crate) fn add(&mut self, other: Self) {
        self.row_group_read += other.row_group_read;
        self.read_io += other.read_io;
        self.checksum += other.checksum;
        self.decompression += other.decompression;
        self.primitive_decode += other.primitive_decode;
        self.fixed_payload_decode += other.fixed_payload_decode;
        self.copy_materialization += other.copy_materialization;
        self.callback += other.callback;
        self.row_groups_materialized = self
            .row_groups_materialized
            .saturating_add(other.row_groups_materialized);
        self.column_chunks_materialized = self
            .column_chunks_materialized
            .saturating_add(other.column_chunks_materialized);
        self.bytes_read = self.bytes_read.saturating_add(other.bytes_read);
        self.compressed_bytes = self.compressed_bytes.saturating_add(other.compressed_bytes);
        self.uncompressed_bytes = self
            .uncompressed_bytes
            .saturating_add(other.uncompressed_bytes);
    }

    fn add_object(&mut self, object: OcbReadObjectAttribution) {
        self.read_io += object.read_io;
        self.checksum += object.checksum;
        self.bytes_read = self.bytes_read.saturating_add(object.bytes_read);
    }

    pub(crate) fn add_callback(&mut self, duration: Duration) {
        self.callback += duration;
    }
}

pub(crate) fn duration_to_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

fn record_value_materialization_time(
    attribution: &mut ReadAttributionAccumulator,
    physical_type: ColumnPhysicalType,
    elapsed: Duration,
) {
    match physical_type {
        ColumnPhysicalType::FixedBinary { .. } => attribution.copy_materialization += elapsed,
        _ => attribution.primitive_decode += elapsed,
    }
}

pub(crate) fn attribution_from_accumulator(
    accumulator: ReadAttributionAccumulator,
    report: &ColumnBundleReadReport,
    plan_ns: u64,
    execute_wall_ns: u64,
) -> ColumnBundleReadAttribution {
    ColumnBundleReadAttribution {
        plan_ns,
        execute_wall_ns,
        callback_wall_ns: duration_to_ns(accumulator.callback),
        row_group_read_ns: duration_to_ns(accumulator.row_group_read),
        read_io_ns: duration_to_ns(accumulator.read_io),
        checksum_ns: duration_to_ns(accumulator.checksum),
        decompression_ns: duration_to_ns(accumulator.decompression),
        primitive_decode_ns: duration_to_ns(accumulator.primitive_decode),
        fixed_payload_decode_ns: duration_to_ns(accumulator.fixed_payload_decode),
        copy_materialization_ns: duration_to_ns(accumulator.copy_materialization),
        native_to_c_copy_ns: None,
        wrapper_copy_ns: None,
        bytes_read: accumulator.bytes_read,
        compressed_bytes: accumulator.compressed_bytes,
        uncompressed_bytes: accumulator.uncompressed_bytes,
        requested_threads: report.requested_threads,
        effective_threads: report.effective_threads,
        selected_row_groups: accumulator.row_groups_materialized,
        pruned_row_groups: report.pruned_row_groups,
        selected_column_chunks: accumulator.column_chunks_materialized,
        fallback_reason: report.fallback_reason,
    }
}

fn resolve_columns(metadata: &OcbMetadataV1) -> Result<Vec<BundleColumn>> {
    let mut columns = Vec::with_capacity(metadata.schema.columns.len());
    let mut seen_ids = BTreeSet::new();
    let mut seen_names = BTreeSet::new();
    for column in &metadata.schema.columns {
        if !seen_ids.insert(column.column_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(column.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column name string id is out of range",
            ))?
            .clone();
        if !seen_names.insert(name.clone()) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column names",
            ));
        }
        columns.push(BundleColumn {
            id: column.column_id,
            name,
            physical_type: column_physical_type_from_desc(column)?,
            logical_kind: column.logical_kind.into(),
            dictionary_id: if column.dictionary_id == OCB_NULL_U32 {
                None
            } else {
                Some(column.dictionary_id)
            },
            scale: column.scale,
            nullable: column.nullability == OcbNullabilityV1::Nullable,
        });
    }
    columns.sort_by_key(|column| column.id);
    Ok(columns)
}

#[derive(Debug, Clone)]
struct ResolvedColumnFill {
    buffer_index: usize,
    column: BundleColumn,
    chunk: OcbColumnChunkDescV1,
}

fn read_row_group_into(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    buffers: &mut [ColumnBundleColumnFillBuffer<'_>],
) -> Result<ColumnBundleReadFillReport> {
    if buffers.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read requires at least one column buffer",
        ));
    }
    let row_group = *metadata
        .row_group_index
        .row_groups
        .iter()
        .find(|row_group| row_group.row_group_id == row_group_id)
        .ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read references an unknown row group",
        ))?;
    let row_count = usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB fill read row count does not fit usize")
    })?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = BTreeMap::new();
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, *chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let by_id = columns
        .iter()
        .map(|column| (column.id, column))
        .collect::<BTreeMap<_, _>>();
    let by_name = columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let mut seen_columns = BTreeSet::new();
    let mut resolved = Vec::with_capacity(buffers.len());
    for (buffer_index, buffer) in buffers.iter().enumerate() {
        let Some(column) = resolve_fill_column(buffer, &by_id, &by_name)? else {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer must identify a column by name or id",
            ));
        };
        if !seen_columns.insert(column.id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill request contains duplicate column buffers",
            ));
        }
        if buffer.values.physical_type() != column.physical_type {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer dtype does not match column dtype",
            ));
        }
        buffer.values.validate_capacity(row_count)?;
        let chunk = *chunk_by_column
            .get(&column.id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        if chunk.physical_type != column.physical_type.ocb_physical_type() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk physical type does not match schema",
            ));
        }
        if chunk.row_count != row_group.row_count {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk row_count does not match row group",
            ));
        }
        if !column.nullable && !chunk.validity_ref.is_null() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        if !chunk.validity_ref.is_null() {
            if !buffer.allow_nulls {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer rejected nullable column chunk",
                ));
            }
            let Some(validity_bytes) = buffer.validity_bytes.as_deref() else {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage for nullable column chunk",
                ));
            };
            let expected_validity_bytes =
                usize::try_from(chunk.row_count.div_ceil(8)).map_err(|_| {
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB fill validity byte count does not fit usize",
                    )
                })?;
            if validity_bytes.len() < expected_validity_bytes {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer validity capacity is too small for row group",
                ));
            }
        }
        resolved.push(ResolvedColumnFill {
            buffer_index,
            column: column.clone(),
            chunk,
        });
    }

    let mut reports = Vec::with_capacity(resolved.len());
    for target in resolved {
        let report = fill_column_buffer(
            path,
            metadata,
            row_count,
            &target,
            &mut buffers[target.buffer_index],
        )?;
        reports.push(report);
    }
    Ok(ColumnBundleReadFillReport {
        row_group_id: row_group.row_group_id,
        base_row: row_group.base_row,
        row_count: row_group.row_count,
        columns: reports,
    })
}

fn read_row_group_into_reusable(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    reusable: &mut ColumnBundleReusableBuffers,
) -> Result<ColumnBundleReadFillReport> {
    let row_count = row_count_for_row_group(metadata, row_group_id)?;
    reusable.prepare_for_rows(row_count)?;
    let mut fill_buffers = reusable.fill_buffers();
    read_row_group_into(path, metadata, columns, row_group_id, &mut fill_buffers)
}

fn read_row_group_into_reusable_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    reusable: &mut ColumnBundleReusableBuffers,
) -> Result<(ColumnBundleReadFillReport, ReadAttributionAccumulator)> {
    let row_count = row_count_for_row_group(metadata, row_group_id)?;
    reusable.prepare_for_rows(row_count)?;
    let mut fill_buffers = reusable.fill_buffers();
    read_row_group_into_with_attribution(path, metadata, columns, row_group_id, &mut fill_buffers)
}

fn row_count_for_row_group(metadata: &OcbMetadataV1, row_group_id: u32) -> Result<usize> {
    let row_group = metadata
        .row_group_index
        .row_groups
        .iter()
        .find(|row_group| row_group.row_group_id == row_group_id)
        .ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB reusable read references an unknown row group",
        ))?;
    usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB reusable read row count does not fit usize")
    })
}

fn read_row_group_into_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    buffers: &mut [ColumnBundleColumnFillBuffer<'_>],
) -> Result<(ColumnBundleReadFillReport, ReadAttributionAccumulator)> {
    let row_group_started = Instant::now();
    if buffers.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read requires at least one column buffer",
        ));
    }
    let row_group = *metadata
        .row_group_index
        .row_groups
        .iter()
        .find(|row_group| row_group.row_group_id == row_group_id)
        .ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read references an unknown row group",
        ))?;
    let row_count = usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB fill read row count does not fit usize")
    })?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = BTreeMap::new();
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, *chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let by_id = columns
        .iter()
        .map(|column| (column.id, column))
        .collect::<BTreeMap<_, _>>();
    let by_name = columns
        .iter()
        .map(|column| (column.name.as_str(), column))
        .collect::<BTreeMap<_, _>>();
    let mut seen_columns = BTreeSet::new();
    let mut resolved = Vec::with_capacity(buffers.len());
    for (buffer_index, buffer) in buffers.iter().enumerate() {
        let Some(column) = resolve_fill_column(buffer, &by_id, &by_name)? else {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer must identify a column by name or id",
            ));
        };
        if !seen_columns.insert(column.id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill request contains duplicate column buffers",
            ));
        }
        if buffer.values.physical_type() != column.physical_type {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer dtype does not match column dtype",
            ));
        }
        buffer.values.validate_capacity(row_count)?;
        let chunk = *chunk_by_column
            .get(&column.id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        if chunk.physical_type != column.physical_type.ocb_physical_type() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk physical type does not match schema",
            ));
        }
        if chunk.row_count != row_group.row_count {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk row_count does not match row group",
            ));
        }
        if !column.nullable && !chunk.validity_ref.is_null() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        if !chunk.validity_ref.is_null() {
            if !buffer.allow_nulls {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer rejected nullable column chunk",
                ));
            }
            let Some(validity_bytes) = buffer.validity_bytes.as_deref() else {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage for nullable column chunk",
                ));
            };
            let expected_validity_bytes =
                usize::try_from(chunk.row_count.div_ceil(8)).map_err(|_| {
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB fill validity byte count does not fit usize",
                    )
                })?;
            if validity_bytes.len() < expected_validity_bytes {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer validity capacity is too small for row group",
                ));
            }
        }
        resolved.push(ResolvedColumnFill {
            buffer_index,
            column: column.clone(),
            chunk,
        });
    }

    let mut reports = Vec::with_capacity(resolved.len());
    let mut attribution = ReadAttributionAccumulator::default();
    for target in resolved {
        let report = fill_column_buffer_with_attribution(
            path,
            metadata,
            row_count,
            &target,
            &mut buffers[target.buffer_index],
            &mut attribution,
        )?;
        reports.push(report);
    }
    attribution.row_group_read += row_group_started.elapsed();
    attribution.row_groups_materialized = 1;
    attribution.column_chunks_materialized = reports.len();
    Ok((
        ColumnBundleReadFillReport {
            row_group_id: row_group.row_group_id,
            base_row: row_group.base_row,
            row_count: row_group.row_count,
            columns: reports,
        },
        attribution,
    ))
}

fn resolve_fill_column<'a>(
    buffer: &ColumnBundleColumnFillBuffer<'_>,
    by_id: &BTreeMap<u32, &'a BundleColumn>,
    by_name: &BTreeMap<&str, &'a BundleColumn>,
) -> Result<Option<&'a BundleColumn>> {
    let by_name_column = match buffer.column_name {
        Some(name) => Some(*by_name.get(name).ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fill buffer references an unknown column name",
        ))?),
        None => None,
    };
    let by_id_column = match buffer.column_id {
        Some(column_id) => Some(*by_id.get(&column_id).ok_or(
            ArcadiaTioError::ocb_invalid_input("OCB fill buffer references an unknown column id"),
        )?),
        None => None,
    };
    match (by_name_column, by_id_column) {
        (Some(left), Some(right)) if left.id != right.id => Err(
            ArcadiaTioError::ocb_invalid_input("OCB fill buffer column name and id do not match"),
        ),
        (Some(column), _) | (_, Some(column)) => Ok(Some(column)),
        (None, None) => Ok(None),
    }
}

fn fill_column_buffer(
    path: &Path,
    metadata: &OcbMetadataV1,
    row_count: usize,
    target: &ResolvedColumnFill,
    buffer: &mut ColumnBundleColumnFillBuffer<'_>,
) -> Result<ColumnBundleColumnFillReport> {
    let object = read_column_chunk(path, metadata.file_len, target.chunk.value_ref)?;
    let payload = validate_and_decode_chunk_object(&object, &target.column, &target.chunk)?;
    fill_primitive_values(&payload, row_count, &mut buffer.values)?;
    let mut validity_filled = false;
    if !target.chunk.validity_ref.is_null() {
        let validity = read_validity_bitmap(path, metadata, &target.chunk)?.ok_or(
            ArcadiaTioError::ocb_corrupt_file("OCB validity bitmap is missing after validation"),
        )?;
        let validity_bytes =
            buffer
                .validity_bytes
                .as_deref_mut()
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage",
                ))?;
        validity_bytes[..validity.bytes.len()].copy_from_slice(&validity.bytes);
        validity_filled = true;
    }
    Ok(ColumnBundleColumnFillReport {
        column_id: target.column.id,
        rows_filled: row_count,
        validity_filled,
    })
}

fn fill_column_buffer_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    row_count: usize,
    target: &ResolvedColumnFill,
    buffer: &mut ColumnBundleColumnFillBuffer<'_>,
    attribution: &mut ReadAttributionAccumulator,
) -> Result<ColumnBundleColumnFillReport> {
    let object = read_column_chunk_attributed(path, metadata.file_len, &target.chunk, attribution)?;
    let (payload, decompression) =
        validate_and_decode_chunk_object_attributed(&object, &target.column, &target.chunk)?;
    attribution.decompression += decompression;
    let decode_started = Instant::now();
    fill_primitive_values(&payload, row_count, &mut buffer.values)?;
    record_value_materialization_time(
        attribution,
        target.column.physical_type,
        decode_started.elapsed(),
    );
    let mut validity_filled = false;
    if !target.chunk.validity_ref.is_null() {
        let validity =
            read_validity_bitmap_with_attribution(path, metadata, &target.chunk, attribution)?
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB validity bitmap is missing after validation",
                ))?;
        let validity_bytes =
            buffer
                .validity_bytes
                .as_deref_mut()
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage",
                ))?;
        validity_bytes[..validity.bytes.len()].copy_from_slice(&validity.bytes);
        validity_filled = true;
    }
    Ok(ColumnBundleColumnFillReport {
        column_id: target.column.id,
        rows_filled: row_count,
        validity_filled,
    })
}

fn fill_primitive_values(
    payload: &[u8],
    row_count: usize,
    values: &mut PrimitiveColumnValuesMut<'_>,
) -> Result<()> {
    match values {
        PrimitiveColumnValuesMut::I32(out) => fill_i32_values(payload, row_count, out),
        PrimitiveColumnValuesMut::I64(out) => fill_i64_values(payload, row_count, out),
        PrimitiveColumnValuesMut::F32(out) => fill_f32_values(payload, row_count, out),
        PrimitiveColumnValuesMut::F64(out) => fill_f64_values(payload, row_count, out),
        PrimitiveColumnValuesMut::FixedBinary { width, bytes } => {
            fill_fixed_binary_values(payload, row_count, *width, bytes)
        }
    }
}

fn fill_fixed_binary_values(
    payload: &[u8],
    row_count: usize,
    width: u32,
    out: &mut [u8],
) -> Result<()> {
    if width == 0 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary column requires fixed width",
        ));
    }
    let expected_bytes =
        row_count
            .checked_mul(width as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB fixed-binary payload byte length overflows",
            ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary payload length does not match row count",
        ));
    }
    if out.len() < expected_bytes {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill buffer value capacity is too small for row group",
        ));
    }
    out[..expected_bytes].copy_from_slice(payload);
    Ok(())
}

fn fill_i32_values(payload: &[u8], row_count: usize, out: &mut [i32]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB i32 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB i32 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(4)) {
        *dst = i32::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

fn fill_i64_values(payload: &[u8], row_count: usize, out: &mut [i64]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(8)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB i64 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB i64 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(8)) {
        *dst = i64::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

fn fill_f32_values(payload: &[u8], row_count: usize, out: &mut [f32]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB f32 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB f32 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(4)) {
        *dst = f32::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

fn fill_f64_values(payload: &[u8], row_count: usize, out: &mut [f64]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(8)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB f64 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB f64 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(8)) {
        *dst = f64::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

fn read_row_group(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    selected_column_ids: &[u32],
) -> Result<ColumnBatch> {
    let row_group = metadata
        .row_group_index
        .row_groups
        .iter()
        .find(|row_group| row_group.row_group_id == row_group_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row group not found"))?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = BTreeMap::new();
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut arrays = Vec::with_capacity(selected_column_ids.len());
    for column_id in selected_column_ids {
        let column = columns
            .iter()
            .find(|column| column.id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected column not found",
            ))?;
        let chunk = chunk_by_column
            .get(column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        arrays.push(read_column_array(
            path,
            metadata,
            column,
            chunk,
            row_group.row_count,
        )?);
    }

    Ok(ColumnBatch {
        row_group_id: row_group.row_group_id,
        base_row: row_group.base_row,
        row_count: row_group.row_count,
        columns: arrays,
    })
}

fn read_row_group_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    selected_column_ids: &[u32],
) -> Result<(ColumnBatch, ReadAttributionAccumulator)> {
    let row_group_started = Instant::now();
    let row_group = metadata
        .row_group_index
        .row_groups
        .iter()
        .find(|row_group| row_group.row_group_id == row_group_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row group not found"))?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = BTreeMap::new();
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut arrays = Vec::with_capacity(selected_column_ids.len());
    let mut attribution = ReadAttributionAccumulator::default();
    for column_id in selected_column_ids {
        let column = columns
            .iter()
            .find(|column| column.id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected column not found",
            ))?;
        let chunk = chunk_by_column
            .get(column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        let (array, column_attr) =
            read_column_array_with_attribution(path, metadata, column, chunk, row_group.row_count)?;
        attribution.add(column_attr);
        arrays.push(array);
    }

    attribution.row_group_read += row_group_started.elapsed();
    attribution.row_groups_materialized = 1;
    attribution.column_chunks_materialized = arrays.len();
    Ok((
        ColumnBatch {
            row_group_id: row_group.row_group_id,
            base_row: row_group.base_row,
            row_count: row_group.row_count,
            columns: arrays,
        },
        attribution,
    ))
}

fn chunks_for_row_group(
    metadata: &OcbMetadataV1,
    begin: u64,
    count: u32,
) -> Result<&[OcbColumnChunkDescV1]> {
    let begin = usize::try_from(begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group chunk descriptor begin is too large")
    })?;
    let count = count as usize;
    let end = begin
        .checked_add(count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .column_chunks
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range is out of bounds",
        ))
}

fn read_column_array(
    path: &Path,
    metadata: &OcbMetadataV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    expected_rows: u64,
) -> Result<ColumnArray> {
    if !column.nullable && !chunk.validity_ref.is_null() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB non-null column chunk cannot have a validity bitmap",
        ));
    }
    if chunk.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk physical type does not match schema",
        ));
    }
    if chunk.row_count != expected_rows {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }
    let object = read_column_chunk(path, metadata.file_len, chunk.value_ref)?;
    let payload = validate_and_decode_chunk_object(&object, column, chunk)?;
    let validity = read_validity_bitmap(path, metadata, chunk)?;
    Ok(ColumnArray {
        column_id: column.id,
        name: column.name.clone(),
        physical_type: column.physical_type,
        logical_kind: column.logical_kind,
        dictionary_id: column.dictionary_id,
        values: decode_primitive_values(column.physical_type, &payload)?,
        validity,
    })
}

fn read_column_array_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    expected_rows: u64,
) -> Result<(ColumnArray, ReadAttributionAccumulator)> {
    if !column.nullable && !chunk.validity_ref.is_null() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB non-null column chunk cannot have a validity bitmap",
        ));
    }
    if chunk.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk physical type does not match schema",
        ));
    }
    if chunk.row_count != expected_rows {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }

    let mut attribution = ReadAttributionAccumulator::default();
    let object = read_column_chunk_attributed(path, metadata.file_len, chunk, &mut attribution)?;
    let (payload, decompression) =
        validate_and_decode_chunk_object_attributed(&object, column, chunk)?;
    attribution.decompression += decompression;
    let validity = read_validity_bitmap_with_attribution(path, metadata, chunk, &mut attribution)?;
    let decode_started = Instant::now();
    let values = decode_primitive_values(column.physical_type, &payload)?;
    record_value_materialization_time(
        &mut attribution,
        column.physical_type,
        decode_started.elapsed(),
    );
    Ok((
        ColumnArray {
            column_id: column.id,
            name: column.name.clone(),
            physical_type: column.physical_type,
            logical_kind: column.logical_kind,
            dictionary_id: column.dictionary_id,
            values,
            validity,
        },
        attribution,
    ))
}

fn read_column_chunk_attributed(
    path: &Path,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    attribution: &mut ReadAttributionAccumulator,
) -> Result<OcbColumnChunkObjectV1> {
    let mut file = std::fs::File::open(path)?;
    let mut object_attr = OcbReadObjectAttribution::default();
    let bytes = read_object_bytes_with_attribution(
        &mut file,
        file_len,
        chunk.value_ref,
        OcbBodyKindV1::ColumnChunk,
        &mut object_attr,
    )?;
    attribution.add_object(object_attr);
    let object = OcbColumnChunkObjectV1::read_from(std::io::Cursor::new(bytes))?;
    attribution.compressed_bytes = attribution
        .compressed_bytes
        .saturating_add(object.payload.len() as u64);
    attribution.uncompressed_bytes = attribution
        .uncompressed_bytes
        .saturating_add(chunk.uncompressed_bytes);
    Ok(object)
}

fn read_validity_bitmap(
    path: &Path,
    metadata: &OcbMetadataV1,
    chunk: &OcbColumnChunkDescV1,
) -> Result<Option<ValidityBitmap>> {
    if chunk.validity_ref.is_null() {
        return Ok(None);
    }
    let expected_bytes = chunk.row_count.div_ceil(8);
    let mut file = std::fs::File::open(path)?;
    let bytes = read_object_bytes(
        &mut file,
        metadata.file_len,
        chunk.validity_ref,
        OcbBodyKindV1::ValidityBitmap,
    )?;
    if bytes.len() as u64 != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB validity bitmap length does not match row count",
        ));
    }
    Ok(Some(ValidityBitmap {
        row_count: chunk.row_count,
        bytes,
    }))
}

fn read_validity_bitmap_with_attribution(
    path: &Path,
    metadata: &OcbMetadataV1,
    chunk: &OcbColumnChunkDescV1,
    attribution: &mut ReadAttributionAccumulator,
) -> Result<Option<ValidityBitmap>> {
    if chunk.validity_ref.is_null() {
        return Ok(None);
    }
    let expected_bytes = chunk.row_count.div_ceil(8);
    let mut file = std::fs::File::open(path)?;
    let mut object_attr = OcbReadObjectAttribution::default();
    let bytes = read_object_bytes_with_attribution(
        &mut file,
        metadata.file_len,
        chunk.validity_ref,
        OcbBodyKindV1::ValidityBitmap,
        &mut object_attr,
    )?;
    attribution.add_object(object_attr);
    if bytes.len() as u64 != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB validity bitmap length does not match row count",
        ));
    }
    Ok(Some(ValidityBitmap {
        row_count: chunk.row_count,
        bytes,
    }))
}

fn validate_and_decode_chunk_object(
    object: &OcbColumnChunkObjectV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
) -> Result<Vec<u8>> {
    if object.row_group_id != chunk.row_group_id || object.column_id != chunk.column_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object identity does not match descriptor",
        ));
    }
    if object.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object physical type does not match schema",
        ));
    }
    if object.codec != chunk.codec {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object codec does not match descriptor",
        ));
    }
    if object.row_count != chunk.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object row_count does not match descriptor",
        ));
    }
    let payload = object.decode_payload()?;
    if payload.len() as u64 != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    Ok(payload)
}

fn validate_and_decode_chunk_object_attributed(
    object: &OcbColumnChunkObjectV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
) -> Result<(Vec<u8>, Duration)> {
    if object.row_group_id != chunk.row_group_id || object.column_id != chunk.column_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object identity does not match descriptor",
        ));
    }
    if object.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object physical type does not match schema",
        ));
    }
    if object.codec != chunk.codec {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object codec does not match descriptor",
        ));
    }
    if object.row_count != chunk.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object row_count does not match descriptor",
        ));
    }
    let decode_started = Instant::now();
    let payload = object.decode_payload()?;
    let decompression = if object.codec == OcbChunkCodecV1::Zstd {
        decode_started.elapsed()
    } else {
        Duration::ZERO
    };
    if payload.len() as u64 != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    Ok((payload, decompression))
}

fn decode_primitive_values(
    physical_type: ColumnPhysicalType,
    payload: &[u8],
) -> Result<PrimitiveColumnValues> {
    match physical_type {
        ColumnPhysicalType::I32 => {
            if !payload.len().is_multiple_of(4) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB i32 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::I32(
                payload
                    .chunks_exact(4)
                    .map(|chunk| i32::from_le_bytes(chunk.try_into().expect("chunk length")))
                    .collect(),
            ))
        }
        ColumnPhysicalType::I64 => {
            if !payload.len().is_multiple_of(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB i64 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::I64(
                payload
                    .chunks_exact(8)
                    .map(|chunk| i64::from_le_bytes(chunk.try_into().expect("chunk length")))
                    .collect(),
            ))
        }
        ColumnPhysicalType::F32 => {
            if !payload.len().is_multiple_of(4) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB f32 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::F32(
                payload
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("chunk length")))
                    .collect(),
            ))
        }
        ColumnPhysicalType::F64 => {
            if !payload.len().is_multiple_of(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB f64 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::F64(
                payload
                    .chunks_exact(8)
                    .map(|chunk| f64::from_le_bytes(chunk.try_into().expect("chunk length")))
                    .collect(),
            ))
        }
        ColumnPhysicalType::FixedBinary { width } => {
            if width == 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary column requires fixed width",
                ));
            }
            if !payload.len().is_multiple_of(width as usize) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::FixedBinary {
                width,
                bytes: payload.to_vec(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{Cursor, Write};

    use super::*;
    use crate::format::{
        OCB_BOOTSTRAP_PAGE_V1_LEN, OCB_FORMAT_MAJOR_V2, OCB_NULL_U32, OCB_ROOT_V1_LEN,
        OCB_ROOT_V2_LEN, OcbBodyKindV1, OcbBodyRefV2, OcbBootstrapPageV1, OcbBootstrapPageV2,
        OcbChunkCodecV1, OcbColumnChunkDescV1, OcbColumnChunkObjectV1, OcbColumnDescV1,
        OcbColumnStatsV1, OcbDictionaryDescV1, OcbDictionaryIndexV1, OcbDictionaryValueKindV1,
        OcbDictionaryValuesV1, OcbNullOrderV1, OcbOrderingDirectionV1, OcbOrderingKeyV1,
        OcbOrderingProofV1, OcbPhysicalTypeV1, OcbRootSlotV2, OcbRootV1, OcbRootV2,
        OcbRowGroupDescV1, OcbRowGroupIndexV1, OcbRowGroupOrderingProofV1, OcbSchemaV1,
        OcbStatScalarV1, OcbStringTableV1, crc32c,
    };

    #[test]
    fn column_bundle_opens_one_file_and_parallel_reads_projected_batches() {
        let path = fixture_path("column_bundle_parallel_read");
        write_fixture(&path);

        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        assert_eq!(bundle.row_count(), 6);
        assert_eq!(bundle.row_group_count(), 2);
        assert_eq!(bundle.columns()[2].name, "category_code");
        assert_eq!(
            bundle.columns()[2].logical_kind,
            ColumnLogicalKind::DictionaryCode
        );

        let batches = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names([
                    "partition_key",
                    "order_key",
                    "category_code",
                ]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("read batches");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].row_group_id, 0);
        assert_eq!(batches[1].row_group_id, 1);
        assert_eq!(batches[0].base_row, 0);
        assert_eq!(batches[1].base_row, 3);
        assert_eq!(
            batches[0].columns[0].values,
            PrimitiveColumnValues::I32(vec![10, 10, 10])
        );
        assert_eq!(
            batches[0].columns[1].values,
            PrimitiveColumnValues::I64(vec![100, 101, 102])
        );
        assert_eq!(
            batches[1].columns[2].values,
            PrimitiveColumnValues::I32(vec![1, 2, 2])
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_opens_v2_latest_root_slot() {
        let path = fixture_path("column_bundle_v2_latest_root");
        write_fixture(&path);
        rewrite_fixture_as_v2_with_dual_roots(&path);

        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        assert_eq!(bundle.row_count(), 6);
        assert_eq!(bundle.row_group_count(), 2);
        assert_eq!(bundle.columns()[2].name, "category_code");

        let batches = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: vec![RowGroupPredicate::equal(
                    "partition_key",
                    ColumnPredicateValue::I32(11),
                )],
                options: ColumnBundleReadOptions::serial(),
            })
            .expect("read latest v2 root");
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].row_group_id, 1);
        assert_eq!(
            batches[0].columns[1].values,
            PrimitiveColumnValues::I64(vec![200, 201, 202])
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_decodes_dictionary_values_on_cold_path() {
        let path = fixture_path("column_bundle_dictionary_values");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let dictionary = bundle.dictionary_values(0).expect("decode dictionary");
        assert_eq!(dictionary.dictionary_id, 0);
        assert_eq!(dictionary.name, "category_dictionary");
        assert_eq!(dictionary.value_kind, DictionaryValueKind::Utf8);
        assert_eq!(
            dictionary.values,
            DictionaryValues::Utf8(vec!["alpha".into(), "beta".into()])
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_prunes_row_groups_with_i32_stats_and_reports_plan() {
        let path = fixture_path("column_bundle_prune_i32");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let outcome = bundle
            .read_batches_with_report(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: vec![RowGroupPredicate::equal(
                    "partition_key",
                    ColumnPredicateValue::I32(11),
                )],
                options: ColumnBundleReadOptions::parallel(4),
            })
            .expect("read pruned batches");

        assert_eq!(outcome.batches.len(), 1);
        assert_eq!(outcome.batches[0].row_group_id, 1);
        assert_eq!(
            outcome.batches[0].columns[0].values,
            PrimitiveColumnValues::I32(vec![11, 11, 11])
        );
        assert_eq!(outcome.report.requested_threads, 4);
        assert_eq!(outcome.report.effective_threads, 1);
        assert_eq!(outcome.report.selected_row_groups, 1);
        assert_eq!(outcome.report.pruned_row_groups, 1);
        assert_eq!(outcome.report.selected_column_chunks, 2);
        assert_eq!(
            outcome.report.fallback_reason,
            Some(OCB_FALLBACK_TOO_FEW_ROW_GROUPS)
        );
        cleanup(&path);
    }

    #[test]
    fn column_bundle_prunes_row_groups_with_i64_range() {
        let path = fixture_path("column_bundle_prune_i64");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let outcome = bundle
            .read_batches_with_report(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["order_key"]),
                predicates: vec![RowGroupPredicate::between(
                    "order_key",
                    ColumnPredicateValue::I64(101),
                    ColumnPredicateValue::I64(150),
                )],
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("read pruned batches");

        assert_eq!(outcome.batches.len(), 1);
        assert_eq!(outcome.batches[0].row_group_id, 0);
        assert_eq!(outcome.report.pruned_row_groups, 1);
        cleanup(&path);
    }

    #[test]
    fn column_bundle_row_group_summaries_include_projected_chunks_stats_and_fixed_binary_bytes() {
        let path = fixture_path("column_bundle_row_group_summaries");
        write_summary_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB summary fixture");

        let summaries = bundle.row_group_summaries().expect("summarize row groups");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].row_group_id, 0);
        assert_eq!(summaries[0].base_row, 0);
        assert_eq!(summaries[0].row_count, 3);
        assert_eq!(summaries[1].row_group_id, 1);
        assert_eq!(summaries[1].base_row, 3);
        assert_eq!(summaries[0].chunks.len(), 2);
        assert_eq!(summaries[0].stats.len(), 1);
        assert_eq!(summaries[0].stats[0].column_name, "partition_key");
        assert_eq!(summaries[0].stats[0].min, ColumnPredicateValue::I32(10));
        assert_eq!(summaries[0].stats[0].max, ColumnPredicateValue::I32(10));

        let payload = summaries[0]
            .chunks
            .iter()
            .find(|chunk| chunk.column_name == "payload")
            .expect("payload chunk summary");
        assert_eq!(
            payload.physical_type,
            ColumnPhysicalType::FixedBinary { width: 2 }
        );
        assert_eq!(payload.fixed_binary_width, Some(2));
        assert_eq!(payload.codec, ColumnBundleColumnChunkSummaryCodec::Zstd);
        assert_eq!(payload.row_count, 3);
        assert_eq!(payload.uncompressed_bytes, 6);
        assert!(payload.compressed_bytes > 0);
        assert_eq!(payload.value_ref.kind, ColumnBundleBodyKind::ColumnChunk);
        assert_eq!(
            payload.value_ref.checksum_kind,
            ColumnBundleChecksumKind::Crc32c
        );
        assert!(payload.validity_ref.is_none());

        let plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["payload"]),
                predicates: vec![RowGroupPredicate::equal(
                    "partition_key",
                    ColumnPredicateValue::I32(11),
                )],
                options: ColumnBundleReadOptions::serial(),
            })
            .expect("plan payload read");
        let plan_summaries = bundle
            .read_plan_row_group_summaries(&plan)
            .expect("summarize plan row groups");
        assert_eq!(plan_summaries.len(), 1);
        assert_eq!(plan_summaries[0].row_group_id, 1);
        assert_eq!(plan_summaries[0].chunks.len(), 1);
        assert_eq!(plan_summaries[0].chunks[0].column_name, "payload");
        assert_eq!(plan_summaries[0].stats[0].column_name, "partition_key");

        let fingerprint = bundle.snapshot_fingerprint().expect("snapshot fingerprint");
        assert_eq!(
            fingerprint.algorithm,
            OCB_CERTIFICATION_FINGERPRINT_ALGORITHM
        );
        assert_eq!(fingerprint.schema.len(), 8);
        assert_eq!(fingerprint.row_groups.len(), 8);
        let reopened_fingerprint = ColumnBundleFile::open(&path)
            .expect("reopen summary fixture")
            .snapshot_fingerprint()
            .expect("reopened snapshot fingerprint");
        assert_eq!(fingerprint, reopened_fingerprint);

        let certification = bundle
            .read_plan_certification(&plan)
            .expect("plan certification");
        assert_eq!(certification.snapshot_fingerprint, fingerprint);
        assert_eq!(certification.root_generation, 0);
        assert_eq!(certification.row_count, 6);
        assert_eq!(certification.row_group_count, 2);
        assert_eq!(certification.report.selected_row_groups, 1);
        assert_eq!(certification.row_groups.len(), 1);
        assert_eq!(certification.row_groups[0].row_group_id, 1);
        assert_eq!(certification.selected_uncompressed_bytes, 6);
        assert!(certification.selected_compressed_bytes > 0);
        assert_eq!(certification.selected_chunk_fingerprint.len(), 8);
        let reopened_certification = ColumnBundleFile::open(&path)
            .expect("reopen certification fixture")
            .read_plan_certification(&plan)
            .expect("reopened plan certification");
        assert_eq!(reopened_certification, certification);

        let full_plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "payload"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("plan fixed-binary projection read");
        let projection = FixedBinaryRecordProjection::by_column_name("payload", 2)
            .field(FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8).with_name("first"))
            .field(FixedBinaryProjectedField::new(1, FixedBinaryFieldType::U8).with_name("second"));
        let mut reusable = bundle
            .reusable_buffer_pool_for_plan(&full_plan, 2, false)
            .expect("reusable pool");
        let mut projection_buffer = bundle
            .fixed_binary_projection_buffer_for_plan(&full_plan, &projection)
            .expect("fixed-binary projection buffer");
        let mut visited = Vec::new();
        let projected = bundle
            .visit_plan_row_groups_project_fixed_binary_with_attribution(
                &full_plan,
                &[1, 0],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 2,
                    ordered: true,
                },
                &mut reusable,
                &projection,
                &mut projection_buffer,
                |batch, projected| {
                    assert_eq!(batch.row_count(), projected.row_count);
                    let first = projected.field_by_name("first")?;
                    let second = projected.field(1)?;
                    assert_eq!(first.values.field_type(), FixedBinaryFieldType::U8);
                    visited.push((
                        projected.row_group_id,
                        first.values.as_u8()?.to_vec(),
                        second.values.as_u8()?.to_vec(),
                    ));
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit fixed-binary projected row groups");
        assert_eq!(visited.len(), 2);
        assert_eq!(visited[0], (0, b"abc".to_vec(), b"abc".to_vec()));
        assert_eq!(visited[1], (1, b"def".to_vec(), b"def".to_vec()));
        assert_eq!(projected.cursor_report.batches_yielded, 2);
        assert_eq!(projected.cursor_report.rows_yielded, 6);
        assert_eq!(projected.cursor_report.max_in_flight_row_groups_observed, 2);
        assert_eq!(projected.attribution.selected_row_groups, 2);
        let _fixed_payload_decode_ns = projected.attribution.fixed_payload_decode_ns;
        let _copy_materialization_ns = projected.attribution.copy_materialization_ns;

        let missing_source_plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect("plan without fixed-binary source");
        let missing_source_err = bundle
            .fixed_binary_projection_buffer_for_plan(&missing_source_plan, &projection)
            .expect_err("fixed-binary source must be part of the plan projection");
        assert!(
            missing_source_err
                .to_string()
                .contains("not in the read plan")
        );

        let wrong_width = FixedBinaryRecordProjection::by_column_name("payload", 3)
            .field(FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8));
        let wrong_width_err = bundle
            .fixed_binary_projection_buffer_for_plan(&full_plan, &wrong_width)
            .expect_err("wrong fixed-binary width rejected");
        assert!(wrong_width_err.to_string().contains("expected_width"));

        let field_overrun = FixedBinaryRecordProjection::by_column_name("payload", 2).field(
            FixedBinaryProjectedField::new(1, FixedBinaryFieldType::U16Le),
        );
        let field_overrun_err = bundle
            .fixed_binary_projection_buffer_for_plan(&full_plan, &field_overrun)
            .expect_err("field overrun rejected before payload reads");
        assert!(
            field_overrun_err
                .to_string()
                .contains("extends past record width")
        );

        let mut bad_projection_buffer = projection_buffer.clone();
        bad_projection_buffer.fields[0].offset = 99;
        let mut bad_reusable = bundle
            .reusable_buffer_pool_for_plan(&full_plan, 1, false)
            .expect("bad reusable pool");
        let mut callbacks = 0usize;
        let bad_buffer_err = bundle
            .visit_plan_row_groups_project_fixed_binary_with_attribution(
                &full_plan,
                &[0],
                ColumnBundleReadCursorOptions::default(),
                &mut bad_reusable,
                &projection,
                &mut bad_projection_buffer,
                |_, _| {
                    callbacks = callbacks.saturating_add(1);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect_err("mismatched projected-field buffer rejected before callbacks");
        assert_eq!(callbacks, 0);
        assert!(
            bad_buffer_err
                .to_string()
                .contains("buffer field does not match")
        );

        cleanup(&path);
    }

    #[test]
    fn fixed_binary_record_projection_decodes_little_endian_fields() {
        let width = 24usize;
        let mut bytes = vec![0u8; width * 2];
        for (row, (kind, side, key, sequence, ordinal)) in [
            (7u8, -3i8, 1234i32, 10_000_000_001i64, 42u64),
            (9u8, 4i8, -5678i32, -10_000_000_002i64, 43u64),
        ]
        .into_iter()
        .enumerate()
        {
            let base = row * width;
            bytes[base] = kind;
            bytes[base + 1] = side as u8;
            bytes[base + 4..base + 8].copy_from_slice(&key.to_le_bytes());
            bytes[base + 8..base + 16].copy_from_slice(&sequence.to_le_bytes());
            bytes[base + 16..base + 24].copy_from_slice(&ordinal.to_le_bytes());
        }

        let primitive = PrimitiveColumnValuesRef::FixedBinary {
            width: width as u32,
            bytes: bytes.as_slice(),
        };
        let records = primitive
            .fixed_binary_records()
            .expect("fixed-binary record view");
        assert_eq!(records.len(), 2);
        assert_eq!(records.row(1).expect("second row")[0], 9);

        let mut kinds = [0u8; 2];
        let mut sides = [0i8; 2];
        let mut keys = [0i32; 2];
        let mut sequences = [0i64; 2];
        let mut ordinals = [0u64; 2];
        let projected = records
            .project_fields(&mut [
                FixedBinaryFieldProjectionMut {
                    offset: 0,
                    values: FixedBinaryFieldValuesMut::U8(&mut kinds),
                },
                FixedBinaryFieldProjectionMut {
                    offset: 1,
                    values: FixedBinaryFieldValuesMut::I8(&mut sides),
                },
                FixedBinaryFieldProjectionMut {
                    offset: 4,
                    values: FixedBinaryFieldValuesMut::I32(&mut keys),
                },
                FixedBinaryFieldProjectionMut {
                    offset: 8,
                    values: FixedBinaryFieldValuesMut::I64(&mut sequences),
                },
                FixedBinaryFieldProjectionMut {
                    offset: 16,
                    values: FixedBinaryFieldValuesMut::U64(&mut ordinals),
                },
            ])
            .expect("project fixed-binary fields");
        assert_eq!(projected, 2);
        let mut reported_keys = [0i32; 2];
        let projection_report = records
            .project_fields_with_report(&mut [FixedBinaryFieldProjectionMut {
                offset: 4,
                values: FixedBinaryFieldValuesMut::I32(&mut reported_keys),
            }])
            .expect("project fixed-binary fields with report");
        assert_eq!(projection_report.rows_projected, 2);
        assert_eq!(projection_report.fields_projected, 1);
        assert_eq!(reported_keys, [1234, -5678]);
        let _projection_wall_ns = projection_report.projection_wall_ns;
        assert_eq!(kinds, [7, 9]);
        assert_eq!(sides, [-3, 4]);
        assert_eq!(keys, [1234, -5678]);
        assert_eq!(sequences, [10_000_000_001, -10_000_000_002]);
        assert_eq!(ordinals, [42, 43]);

        let err = FixedBinaryRecordView::new(3, &[1, 2, 3, 4])
            .expect_err("unaligned record bytes rejected");
        assert!(err.to_string().contains("not aligned"));

        let mut too_small = [0i32; 1];
        let err = records
            .project_fields(&mut [FixedBinaryFieldProjectionMut {
                offset: 4,
                values: FixedBinaryFieldValuesMut::I32(&mut too_small),
            }])
            .expect_err("small output rejected");
        assert!(err.to_string().contains("output buffer is too small"));

        let mut out = [0i64; 2];
        let err = records
            .project_fields(&mut [FixedBinaryFieldProjectionMut {
                offset: 20,
                values: FixedBinaryFieldValuesMut::I64(&mut out),
            }])
            .expect_err("field overrun rejected");
        assert!(err.to_string().contains("extends past record width"));
    }

    #[test]
    fn column_bundle_strict_planning_fails_closed_and_validates_summary_plan_ids() {
        let path = fixture_path("column_bundle_strict_planning");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let strict_plan = bundle
            .plan_read_strict(
                &ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key"]),
                    predicates: vec![RowGroupPredicate::equal(
                        "partition_key",
                        ColumnPredicateValue::I32(11),
                    )],
                    options: ColumnBundleReadOptions::parallel(4),
                },
                ColumnBundleStrictReadPlanningOptions::new(1),
            )
            .expect("strict plan with complete stats");
        assert_eq!(strict_plan.row_group_ids, vec![1]);
        assert_eq!(strict_plan.report.selected_row_groups, 1);
        assert_eq!(strict_plan.report.pruned_row_groups, 1);

        let broad_scan = bundle
            .plan_read_strict(
                &ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key"]),
                    predicates: Vec::new(),
                    options: ColumnBundleReadOptions::serial(),
                },
                ColumnBundleStrictReadPlanningOptions::new(1),
            )
            .expect_err("strict plan rejects broad scan over cap");
        assert_eq!(
            broad_scan.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(broad_scan.to_string().contains("caller cap"));

        let missing_stats = bundle
            .plan_read_strict(
                &ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["category_code"]),
                    predicates: vec![RowGroupPredicate::equal(
                        "category_code",
                        ColumnPredicateValue::I32(2),
                    )],
                    options: ColumnBundleReadOptions::serial(),
                },
                ColumnBundleStrictReadPlanningOptions::new(2),
            )
            .expect_err("strict plan rejects unavailable predicate stats");
        assert_eq!(
            missing_stats.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(missing_stats.to_string().contains("stats"));

        let mut duplicate_plan = strict_plan.clone();
        duplicate_plan.row_group_ids = vec![1, 1];
        let duplicate = bundle
            .read_plan_row_group_summaries(&duplicate_plan)
            .expect_err("duplicate summary plan row group ids reject");
        assert_eq!(
            duplicate.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(duplicate.to_string().contains("duplicate"));

        let mut unknown_plan = strict_plan.clone();
        unknown_plan.row_group_ids = vec![99];
        let unknown = bundle
            .read_plan_row_group_summaries(&unknown_plan)
            .expect_err("unknown summary plan row group ids reject");
        assert_eq!(
            unknown.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(unknown.to_string().contains("unknown row group"));

        cleanup(&path);

        let path = fixture_path("column_bundle_strict_fixed_binary_predicate");
        write_summary_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB summary fixture");
        let fixed_binary_predicate = bundle
            .plan_read_strict(
                &ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["payload"]),
                    predicates: vec![RowGroupPredicate::equal(
                        "payload",
                        ColumnPredicateValue::I32(1),
                    )],
                    options: ColumnBundleReadOptions::serial(),
                },
                ColumnBundleStrictReadPlanningOptions::new(1),
            )
            .expect_err("strict plan rejects unsupported fixed-binary predicate metadata");
        assert_eq!(
            fixed_binary_predicate.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(fixed_binary_predicate.to_string().contains("fixed-binary"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_default_reads_remain_conservative_without_strict_planning() {
        let path = fixture_path("column_bundle_default_read_after_strict");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let ordinary = bundle
            .read_batches_with_report(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["category_code"]),
                predicates: vec![RowGroupPredicate::equal(
                    "category_code",
                    ColumnPredicateValue::I32(2),
                )],
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("ordinary reads still keep row groups with missing stats");
        assert_eq!(ordinary.batches.len(), 2);
        assert_eq!(ordinary.report.selected_row_groups, 2);
        assert_eq!(ordinary.report.pruned_row_groups, 0);
        assert_eq!(ordinary.report.effective_threads, 2);
        assert_eq!(
            ordinary.batches[0].columns[0].values,
            PrimitiveColumnValues::I32(vec![1, 1, 2])
        );
        assert_eq!(
            ordinary.batches[1].columns[0].values,
            PrimitiveColumnValues::I32(vec![1, 2, 2])
        );

        let strict = bundle
            .plan_read_strict(
                &ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["category_code"]),
                    predicates: vec![RowGroupPredicate::equal(
                        "category_code",
                        ColumnPredicateValue::I32(2),
                    )],
                    options: ColumnBundleReadOptions::parallel(2),
                },
                ColumnBundleStrictReadPlanningOptions::new(2),
            )
            .expect_err("strict helper fails where ordinary read remains conservative");
        assert_eq!(
            strict.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_executes_read_plan_and_row_group_subsets() {
        let path = fixture_path("column_bundle_read_plan_subset");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(4),
            })
            .expect("plan read");
        assert_eq!(plan.projected_column_ids, vec![0, 1]);
        assert_eq!(plan.row_group_ids, vec![0, 1]);
        assert_eq!(plan.report.selected_row_groups, 2);
        assert_eq!(plan.report.effective_threads, 2);

        let all = bundle.read_plan_batches(&plan).expect("execute plan");
        assert_eq!(all.batches.len(), 2);
        assert_eq!(all.batches[0].row_group_id, 0);
        assert_eq!(all.batches[1].row_group_id, 1);
        assert_eq!(all.report.selected_row_groups, 2);

        let subset = bundle
            .read_plan_row_groups(&plan, &[1, 0])
            .expect("execute subset in plan order");
        assert_eq!(subset.batches.len(), 2);
        assert_eq!(subset.batches[0].row_group_id, 0);
        assert_eq!(subset.batches[1].row_group_id, 1);

        let subset = bundle
            .read_plan_row_groups(&plan, &[1])
            .expect("execute one row group subset");
        assert_eq!(subset.batches.len(), 1);
        assert_eq!(subset.batches[0].row_group_id, 1);
        assert_eq!(subset.report.selected_row_groups, 1);
        assert_eq!(subset.report.effective_threads, 1);
        assert_eq!(subset.report.selected_column_chunks, 2);

        let mut visited = Vec::new();
        let visit_report = bundle
            .visit_plan_row_groups(
                &plan,
                &[1, 0],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 1,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit subset in plan order");
        assert_eq!(visited, vec![0, 1]);
        assert_eq!(visit_report.batches_yielded, 2);
        assert_eq!(visit_report.rows_yielded, 6);
        assert_eq!(visit_report.max_in_flight_row_groups_observed, 1);
        assert!(!visit_report.cancelled);
        assert_eq!(visit_report.base_report.selected_row_groups, 2);
        assert_eq!(visit_report.base_report.selected_column_chunks, 4);

        let mut visited = Vec::new();
        let visit_report = bundle
            .visit_plan_row_groups(
                &plan,
                &[1, 0],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 2,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    Ok(ColumnBundleVisitControl::Stop)
                },
            )
            .expect("cancel subset visitor");
        assert_eq!(visited, vec![0]);
        assert_eq!(visit_report.batches_yielded, 1);
        assert_eq!(visit_report.rows_yielded, 3);
        assert_eq!(visit_report.max_in_flight_row_groups_observed, 2);
        assert!(visit_report.cancelled);

        let mut visited = Vec::new();
        let attributed_visit = bundle
            .visit_plan_row_groups_with_attribution(
                &plan,
                &[1],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 1,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    thread::sleep(Duration::from_millis(1));
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit subset with attribution");
        assert_eq!(visited, vec![1]);
        assert_eq!(attributed_visit.cursor_report.batches_yielded, 1);
        assert_eq!(attributed_visit.cursor_report.rows_yielded, 3);
        assert_eq!(
            attributed_visit
                .cursor_report
                .max_in_flight_row_groups_observed,
            1
        );
        assert!(!attributed_visit.cursor_report.cancelled);
        assert_eq!(attributed_visit.attribution.selected_row_groups, 1);
        assert_eq!(attributed_visit.attribution.selected_column_chunks, 2);
        assert_eq!(attributed_visit.attribution.effective_threads, 1);
        assert!(attributed_visit.attribution.execute_wall_ns > 0);
        assert!(attributed_visit.attribution.callback_wall_ns > 0);
        assert!(attributed_visit.attribution.row_group_read_ns > 0);
        assert!(attributed_visit.attribution.read_io_ns > 0);
        assert!(attributed_visit.attribution.bytes_read > 0);
        assert!(attributed_visit.attribution.compressed_bytes > 0);
        assert!(attributed_visit.attribution.uncompressed_bytes > 0);
        assert_eq!(attributed_visit.attribution.native_to_c_copy_ns, None);
        assert_eq!(attributed_visit.attribution.wrapper_copy_ns, None);

        let mut pool = bundle
            .reusable_buffer_pool_for_plan(&plan, 2, false)
            .expect("allocate reusable buffers");
        let mut reusable_visited = Vec::new();
        let reusable_report = bundle
            .visit_plan_row_groups_into(
                &plan,
                &[1, 0],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 2,
                    ordered: true,
                },
                &mut pool,
                |view| {
                    reusable_visited.push(view.row_group_id());
                    assert_eq!(view.column_count(), 2);
                    let partition = view.column(0)?;
                    assert_eq!(partition.name, "partition_key");
                    match partition.values {
                        PrimitiveColumnValuesRef::I32(values) if view.row_group_id() == 0 => {
                            assert_eq!(values, &[10, 10, 10]);
                        }
                        PrimitiveColumnValuesRef::I32(values) if view.row_group_id() == 1 => {
                            assert_eq!(values, &[11, 11, 11]);
                        }
                        _ => panic!("unexpected reusable partition values"),
                    }
                    assert!(partition.validity.is_none());
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit subset into reusable buffers");
        assert_eq!(reusable_visited, vec![0, 1]);
        assert_eq!(reusable_report.batches_yielded, 2);
        assert_eq!(reusable_report.rows_yielded, 6);
        assert_eq!(reusable_report.max_in_flight_row_groups_observed, 2);
        assert!(!reusable_report.cancelled);

        let mut mismatched_pool = pool.clone();
        mismatched_pool.buffers[0].columns[0].column_id = 999;
        let mismatch_err = bundle
            .visit_plan_row_groups_into(
                &plan,
                &[0],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 1,
                    ordered: true,
                },
                &mut mismatched_pool,
                |_| Ok(ColumnBundleVisitControl::Continue),
            )
            .expect_err("mismatched reusable buffers are rejected");
        assert!(
            mismatch_err
                .to_string()
                .contains("buffer column does not match plan projection")
        );

        let mut pool = bundle
            .reusable_buffer_pool_for_plan(&plan, 1, false)
            .expect("allocate attributed reusable buffers");
        let reusable_attributed = bundle
            .visit_plan_row_groups_into_with_attribution(
                &plan,
                &[1],
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 1,
                    ordered: true,
                },
                &mut pool,
                |view| {
                    assert_eq!(view.row_group_id(), 1);
                    thread::sleep(Duration::from_millis(1));
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit subset into reusable buffers with attribution");
        assert_eq!(reusable_attributed.cursor_report.batches_yielded, 1);
        assert_eq!(reusable_attributed.cursor_report.rows_yielded, 3);
        assert_eq!(
            reusable_attributed
                .cursor_report
                .max_in_flight_row_groups_observed,
            1
        );
        assert_eq!(reusable_attributed.attribution.selected_row_groups, 1);
        assert_eq!(reusable_attributed.attribution.selected_column_chunks, 2);
        assert!(reusable_attributed.attribution.callback_wall_ns > 0);
        assert!(reusable_attributed.attribution.row_group_read_ns > 0);
        assert!(reusable_attributed.attribution.read_io_ns > 0);

        let duplicate = bundle
            .read_plan_row_groups(&plan, &[1, 1])
            .expect_err("duplicate subset ids reject");
        assert_eq!(
            duplicate.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(
            duplicate
                .to_string()
                .contains(OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR)
        );

        let unknown = bundle
            .read_plan_row_groups(&plan, &[99])
            .expect_err("unknown subset ids reject");
        assert_eq!(
            unknown.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(
            unknown
                .to_string()
                .contains(OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR)
        );

        let duplicate = bundle
            .visit_plan_row_groups(
                &plan,
                &[1, 1],
                ColumnBundleReadCursorOptions::default(),
                |_| Ok(ColumnBundleVisitControl::Continue),
            )
            .expect_err("duplicate visitor subset ids reject");
        assert_eq!(
            duplicate.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(
            duplicate
                .to_string()
                .contains(OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR)
        );

        let unknown = bundle
            .visit_plan_row_groups(
                &plan,
                &[99],
                ColumnBundleReadCursorOptions::default(),
                |_| Ok(ColumnBundleVisitControl::Continue),
            )
            .expect_err("unknown visitor subset ids reject");
        assert_eq!(
            unknown.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(
            unknown
                .to_string()
                .contains(OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR)
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_parallel_prepare_matches_worker_counts_and_plan_order() {
        let path = fixture_path("column_bundle_parallel_prepare");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let mut expected = None;

        for workers in [1, 2, 4, 8] {
            let plan = bundle
                .plan_read(&ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key", "order_key"]),
                    predicates: Vec::new(),
                    options: ColumnBundleReadOptions::parallel(workers),
                })
                .expect("plan parallel preparation");
            let mut committed = Vec::new();
            let report = bundle
                .parallel_prepare_plan_row_groups(
                    &plan,
                    &[1, 0],
                    ColumnBundleParallelPrepareOptions {
                        max_in_flight_row_groups: 2,
                    },
                    |context, batch| {
                        assert_eq!(context.row_group_id, batch.row_group_id);
                        assert_eq!(context.base_row, batch.base_row);
                        assert_eq!(context.row_count, batch.row_count);
                        assert_eq!(context.row_end, batch.base_row + batch.row_count);
                        let order_keys = match &batch.columns[1].values {
                            PrimitiveColumnValues::I64(values) => values.clone(),
                            _ => panic!("order key must be i64"),
                        };
                        Ok((
                            context.selected_row_group_ordinal,
                            context.row_group_id,
                            context.base_row,
                            context.row_end,
                            context.row_count,
                            order_keys,
                        ))
                    },
                    |context, prepared| {
                        assert_eq!(context.selected_row_group_ordinal, prepared.0);
                        committed.push(prepared);
                        Ok(ColumnBundleVisitControl::Continue)
                    },
                )
                .expect("parallel prepare row groups");
            assert_eq!(
                committed
                    .iter()
                    .map(|prepared| prepared.1)
                    .collect::<Vec<_>>(),
                vec![0, 1]
            );
            if let Some(expected) = &expected {
                assert_eq!(&committed, expected);
            } else {
                expected = Some(committed);
            }
            assert_eq!(report.requested_workers, workers);
            assert_eq!(report.started_workers, workers.min(2));
            assert_eq!(report.row_groups_queued, 2);
            assert_eq!(report.row_groups_completed, 2);
            assert_eq!(report.row_groups_ordered_committed, 2);
            assert_eq!(report.rows_ordered_committed, 6);
            assert!(report.ordered_terminal_completed);
            assert!(!report.cursor_report.cancelled);
            assert_eq!(report.attribution.selected_row_groups, 2);
            assert_eq!(report.attribution.selected_column_chunks, 4);
            assert!(report.attribution.read_io_ns > 0);
            assert!(report.attribution.primitive_decode_ns > 0);
            assert_eq!(
                report.caller_prepare_ns,
                report
                    .worker_reports
                    .iter()
                    .map(|worker| worker.caller_prepare_ns)
                    .sum::<u64>()
            );
        }

        let plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(4),
            })
            .expect("plan stopped preparation");
        let stopped = bundle
            .parallel_prepare_plan_row_groups(
                &plan,
                &plan.row_group_ids,
                ColumnBundleParallelPrepareOptions {
                    max_in_flight_row_groups: 2,
                },
                |context, _| Ok(context.row_group_id),
                |context, row_group_id| {
                    assert_eq!(context.row_group_id, row_group_id);
                    Ok(ColumnBundleVisitControl::Stop)
                },
            )
            .expect("stop ordered commit");
        assert!(stopped.cursor_report.cancelled);
        assert_eq!(stopped.row_groups_ordered_committed, 1);
        assert!(!stopped.ordered_terminal_completed);
        assert!(stopped.row_groups_ordered_committed <= stopped.row_groups_completed);
        assert!(stopped.row_groups_completed <= stopped.row_groups_queued);

        cleanup(&path);
    }

    #[test]
    fn column_bundle_read_attribution_reports_diagnostic_counters() {
        let path = fixture_path("column_bundle_read_attribution");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let attributed = bundle
            .read_batches_with_attribution(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("attributed read");

        assert_eq!(attributed.outcome.batches.len(), 2);
        assert_eq!(attributed.attribution.selected_row_groups, 2);
        assert_eq!(attributed.attribution.selected_column_chunks, 4);
        assert_eq!(attributed.attribution.requested_threads, 2);
        assert_eq!(attributed.attribution.effective_threads, 2);
        assert!(attributed.attribution.execute_wall_ns > 0);
        assert_eq!(attributed.attribution.callback_wall_ns, 0);
        assert!(attributed.attribution.row_group_read_ns > 0);
        assert!(attributed.attribution.read_io_ns > 0);
        assert!(attributed.attribution.bytes_read > 0);
        assert!(attributed.attribution.compressed_bytes > 0);
        assert!(attributed.attribution.uncompressed_bytes > 0);
        assert_eq!(attributed.attribution.native_to_c_copy_ns, None);
        assert_eq!(attributed.attribution.wrapper_copy_ns, None);

        let mut visited = Vec::new();
        let attributed_cursor = bundle
            .visit_batches_with_attribution(
                ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key", "order_key"]),
                    predicates: Vec::new(),
                    options: ColumnBundleReadOptions::parallel(2),
                },
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 2,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("attributed visitor read");
        assert_eq!(visited, vec![0, 1]);
        assert_eq!(attributed_cursor.cursor_report.batches_yielded, 2);
        assert_eq!(attributed_cursor.cursor_report.rows_yielded, 6);
        assert_eq!(
            attributed_cursor
                .cursor_report
                .max_in_flight_row_groups_observed,
            2
        );
        assert_eq!(attributed_cursor.attribution.selected_row_groups, 2);
        assert_eq!(attributed_cursor.attribution.selected_column_chunks, 4);
        assert_eq!(attributed_cursor.attribution.requested_threads, 2);
        assert_eq!(attributed_cursor.attribution.effective_threads, 2);
        assert!(attributed_cursor.attribution.execute_wall_ns > 0);
        assert!(attributed_cursor.attribution.row_group_read_ns > 0);
        assert!(attributed_cursor.attribution.read_io_ns > 0);
        assert!(attributed_cursor.attribution.bytes_read > 0);

        cleanup(&path);
    }

    #[test]
    fn column_bundle_visit_batches_bounds_and_can_cancel() {
        let path = fixture_path("column_bundle_visit_batches");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let mut visited = Vec::new();
        let report = bundle
            .visit_batches(
                ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key"]),
                    predicates: Vec::new(),
                    options: ColumnBundleReadOptions::parallel(4),
                },
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 1,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("visit all batches");
        assert_eq!(visited, vec![0, 1]);
        assert_eq!(report.batches_yielded, 2);
        assert_eq!(report.rows_yielded, 6);
        assert_eq!(report.max_in_flight_row_groups_observed, 1);
        assert!(!report.cancelled);
        assert_eq!(report.base_report.selected_row_groups, 2);

        let mut visited = Vec::new();
        let report = bundle
            .visit_batches(
                ColumnBundleReadRequest {
                    projection: ColumnProjection::names(["partition_key"]),
                    predicates: Vec::new(),
                    options: ColumnBundleReadOptions::parallel(4),
                },
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 2,
                    ordered: true,
                },
                |batch| {
                    visited.push(batch.row_group_id);
                    Ok(ColumnBundleVisitControl::Stop)
                },
            )
            .expect("cancel visit");
        assert_eq!(visited, vec![0]);
        assert_eq!(report.batches_yielded, 1);
        assert_eq!(report.rows_yielded, 3);
        assert_eq!(report.max_in_flight_row_groups_observed, 2);
        assert!(report.cancelled);

        let err = bundle
            .visit_batches(
                ColumnBundleReadRequest::default(),
                ColumnBundleReadCursorOptions {
                    max_in_flight_row_groups: 0,
                    ordered: true,
                },
                |_| Ok(ColumnBundleVisitControl::Continue),
            )
            .expect_err("zero in-flight rejects");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );

        cleanup(&path);
    }

    #[test]
    fn column_bundle_read_row_group_into_fills_caller_buffers() {
        let path = fixture_path("column_bundle_read_row_group_into");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let mut partition = [0i32; 3];
        let mut order = [0i64; 3];
        let report = bundle
            .read_row_group_into(
                1,
                &mut [
                    ColumnBundleColumnFillBuffer {
                        column_name: Some("partition_key"),
                        column_id: None,
                        values: PrimitiveColumnValuesMut::I32(&mut partition),
                        validity_bytes: None,
                        allow_nulls: false,
                    },
                    ColumnBundleColumnFillBuffer {
                        column_name: Some("order_key"),
                        column_id: Some(1),
                        values: PrimitiveColumnValuesMut::I64(&mut order),
                        validity_bytes: None,
                        allow_nulls: false,
                    },
                ],
                ColumnBundleReadFillOptions::default(),
            )
            .expect("fill row group");

        assert_eq!(partition, [11, 11, 11]);
        assert_eq!(order, [200, 201, 202]);
        assert_eq!(report.row_group_id, 1);
        assert_eq!(report.base_row, 3);
        assert_eq!(report.row_count, 3);
        assert_eq!(report.columns.len(), 2);
        assert_eq!(report.columns[0].column_id, 0);
        assert_eq!(report.columns[0].rows_filled, 3);
        assert!(!report.columns[0].validity_filled);

        cleanup(&path);
    }

    #[test]
    fn column_bundle_read_row_group_into_rejects_bad_buffers() {
        let path = fixture_path("column_bundle_read_row_group_into_bad_buffers");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let mut short = [0i32; 2];
        let err = bundle
            .read_row_group_into(
                0,
                &mut [ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I32(&mut short),
                    validity_bytes: None,
                    allow_nulls: false,
                }],
                ColumnBundleReadFillOptions::default(),
            )
            .expect_err("short buffer rejects");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("capacity"));

        let mut wrong_dtype = [0i64; 3];
        let err = bundle
            .read_row_group_into(
                0,
                &mut [ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I64(&mut wrong_dtype),
                    validity_bytes: None,
                    allow_nulls: false,
                }],
                ColumnBundleReadFillOptions::default(),
            )
            .expect_err("wrong dtype rejects");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("dtype"));

        let mut first = [0i32; 3];
        let mut second = [0i32; 3];
        let err = bundle
            .read_row_group_into(
                0,
                &mut [
                    ColumnBundleColumnFillBuffer {
                        column_name: Some("partition_key"),
                        column_id: None,
                        values: PrimitiveColumnValuesMut::I32(&mut first),
                        validity_bytes: None,
                        allow_nulls: false,
                    },
                    ColumnBundleColumnFillBuffer {
                        column_name: None,
                        column_id: Some(0),
                        values: PrimitiveColumnValuesMut::I32(&mut second),
                        validity_bytes: None,
                        allow_nulls: false,
                    },
                ],
                ColumnBundleReadFillOptions::default(),
            )
            .expect_err("duplicate buffer rejects");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("duplicate"));

        cleanup(&path);
    }

    #[test]
    fn column_bundle_read_row_group_into_fills_validity_bitmap() {
        let path = fixture_path("column_bundle_read_row_group_into_validity");
        write_fixture_with_options(
            &path,
            FixtureOptions {
                nullable_column_id: Some(0),
                validity_ref_column_id: Some(0),
                ..FixtureOptions::default()
            },
        );
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let mut values = [0i32; 3];
        let mut validity = [0u8; 1];
        let report = bundle
            .read_row_group_into(
                0,
                &mut [ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I32(&mut values),
                    validity_bytes: Some(&mut validity),
                    allow_nulls: true,
                }],
                ColumnBundleReadFillOptions::default(),
            )
            .expect("fill nullable row group");
        assert_eq!(values, [10, 10, 10]);
        assert_eq!(validity, [0b0000_0101]);
        assert!(report.columns[0].validity_filled);

        let mut values_without_validity = [0i32; 3];
        let err = bundle
            .read_row_group_into(
                0,
                &mut [ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I32(&mut values_without_validity),
                    validity_bytes: None,
                    allow_nulls: true,
                }],
                ColumnBundleReadFillOptions::default(),
            )
            .expect_err("missing validity rejects");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("validity"));

        cleanup(&path);
    }

    #[test]
    fn column_bundle_missing_stats_keep_row_groups_conservatively() {
        let path = fixture_path("column_bundle_missing_stats_keep");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let outcome = bundle
            .read_batches_with_report(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["category_code"]),
                predicates: vec![RowGroupPredicate::equal(
                    "category_code",
                    ColumnPredicateValue::I32(2),
                )],
                options: ColumnBundleReadOptions::parallel(2),
            })
            .expect("read conservatively kept batches");

        assert_eq!(outcome.batches.len(), 2);
        assert_eq!(outcome.report.selected_row_groups, 2);
        assert_eq!(outcome.report.pruned_row_groups, 0);
        assert_eq!(outcome.report.effective_threads, 2);
        assert_eq!(outcome.report.fallback_reason, None);
        cleanup(&path);
    }

    #[test]
    fn column_bundle_rejects_predicate_dtype_mismatch() {
        let path = fixture_path("column_bundle_predicate_dtype_mismatch");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: vec![RowGroupPredicate::equal(
                    "partition_key",
                    ColumnPredicateValue::I64(11),
                )],
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("dtype mismatch");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("dtype"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_rejects_invalid_predicate_requests_as_input() {
        let path = fixture_path("column_bundle_invalid_predicate_requests");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: vec![RowGroupPredicate::equal(
                    "missing",
                    ColumnPredicateValue::I32(11),
                )],
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("unknown predicate column");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("unknown column"));

        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: vec![RowGroupPredicate::new("partition_key", None, None)],
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("empty predicate");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("at least one bound"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_rejects_unknown_projection() {
        let path = fixture_path("column_bundle_unknown_projection");
        write_fixture(&path);
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["missing"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("unknown projection");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("unknown column"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_reads_nullable_column_without_bitmap_as_all_valid() {
        let path = fixture_path("column_bundle_nullable_all_valid");
        write_fixture_with_options(
            &path,
            FixtureOptions {
                nullable_column_id: Some(0),
                ..FixtureOptions::default()
            },
        );
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let batches = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect("read nullable all-valid column");
        assert!(batches[0].columns[0].validity.is_none());
        cleanup(&path);
    }

    #[test]
    fn column_bundle_reads_nullable_validity_bitmap() {
        let path = fixture_path("column_bundle_validity_bitmap");
        write_fixture_with_options(
            &path,
            FixtureOptions {
                nullable_column_id: Some(0),
                validity_ref_column_id: Some(0),
                ..FixtureOptions::default()
            },
        );
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let batches = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect("read validity bitmap");
        let validity = batches[0].columns[0]
            .validity
            .as_ref()
            .expect("validity bitmap present");
        assert_eq!(validity.row_count, 3);
        assert_eq!(validity.bytes, vec![0b0000_0101]);
        assert!(validity.is_valid(0).expect("row 0 validity"));
        assert!(!validity.is_valid(1).expect("row 1 validity"));
        assert!(validity.is_valid(2).expect("row 2 validity"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_rejects_non_null_validity_refs() {
        let path = fixture_path("column_bundle_validity_ref_rejected");
        write_fixture_with_options(
            &path,
            FixtureOptions {
                validity_ref_column_id: Some(0),
                ..FixtureOptions::default()
            },
        );
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("non-null validity rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert!(err.to_string().contains("non-null column"));
        cleanup(&path);
    }

    #[test]
    fn column_bundle_rejects_row_group_pointing_at_other_group_chunks() {
        let path = fixture_path("column_bundle_wrong_row_group_chunks");
        write_fixture_with_options(
            &path,
            FixtureOptions {
                row_group0_chunk_desc_begin: Some(3),
                ..FixtureOptions::default()
            },
        );
        let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
        let err = bundle
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            })
            .expect_err("wrong row group chunks");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert!(err.to_string().contains("different row group"));
        cleanup(&path);
    }

    fn fixture_path(name: &str) -> PathBuf {
        let root = PathBuf::from(".tmp/ocb-tests");
        fs::create_dir_all(&root).expect("create test tmp dir");
        root.join(format!("{name}-{}.tio", std::process::id()))
    }

    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
    }

    #[derive(Debug, Clone, Copy, Default)]
    struct FixtureOptions {
        nullable_column_id: Option<u32>,
        validity_ref_column_id: Option<u32>,
        row_group0_chunk_desc_begin: Option<u64>,
    }

    fn write_fixture(path: &Path) {
        write_fixture_with_options(path, FixtureOptions::default());
    }

    fn rewrite_fixture_as_v2_with_dual_roots(path: &Path) {
        let mut file_bytes = fs::read(path).expect("read v1 fixture bytes");
        let bootstrap = OcbBootstrapPageV1::read_from(Cursor::new(file_bytes.as_slice()))
            .expect("read v1 bootstrap");
        let root_start = bootstrap.root_ref.offset as usize;
        let root_end = root_start + bootstrap.root_ref.length as usize;
        let base_root = OcbRootV1::read_from(Cursor::new(&file_bytes[root_start..root_end]))
            .expect("read v1 root");

        let mut stale_root = root_v2_from_v1(&base_root, 1, 0, OcbBodyRefV2::NULL);
        stale_root.row_count = 999;
        let stale_root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            stale_root.write_to(buf)
        });
        assert_eq!(stale_root_ref.length, OCB_ROOT_V2_LEN as u64);

        let latest_root = root_v2_from_v1(&base_root, 2, 1, stale_root_ref);
        let latest_root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            latest_root.write_to(buf)
        });
        assert_eq!(latest_root_ref.length, OCB_ROOT_V2_LEN as u64);

        let bootstrap = OcbBootstrapPageV2::new(
            [77u8; 16],
            [
                OcbRootSlotV2::new(
                    0,
                    1,
                    stale_root_ref,
                    0,
                    OcbBodyRefV2::NULL,
                    OcbBodyRefV2::NULL,
                ),
                OcbRootSlotV2::new(1, 2, latest_root_ref, 1, stale_root_ref, OcbBodyRefV2::NULL),
            ],
        )
        .expect("build v2 bootstrap");
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write v2 bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
        fs::write(path, file_bytes).expect("write v2 fixture bytes");
    }

    fn root_v2_from_v1(
        root: &OcbRootV1,
        generation: u64,
        previous_generation: u64,
        previous_root_ref: OcbBodyRefV2,
    ) -> OcbRootV2 {
        OcbRootV2 {
            version: OCB_FORMAT_MAJOR_V2,
            flags: root.flags,
            generation,
            previous_generation,
            previous_root_ref,
            append_base_row: 0,
            append_row_count: root.row_count,
            append_base_row_group: 0,
            append_row_group_count: root.row_group_count,
            row_count: root.row_count,
            column_count: root.column_count,
            row_group_count: root.row_group_count,
            dictionary_count: root.dictionary_count,
            column_chunk_count: root.column_count * root.row_group_count,
            schema_ref: root.schema_ref,
            dictionary_index_ref: root.dictionary_index_ref,
            row_group_index_ref: root.row_group_index_ref,
            ordering_proof_ref: root.ordering_proof_ref,
            debug_json_ref: root.debug_json_ref,
            first_key_tuple_ref: OcbBodyRefV2::NULL,
            last_key_tuple_ref: OcbBodyRefV2::NULL,
            append_first_key_tuple_ref: OcbBodyRefV2::NULL,
            append_last_key_tuple_ref: OcbBodyRefV2::NULL,
            commit_diagnostics_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: root.created_unix_nanos,
            content_flags: root.content_flags,
            crc32c: 0,
        }
    }

    fn write_summary_fixture(path: &Path) {
        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];

        let rg0_partition =
            append_chunk(&mut file_bytes, 0, 0, OcbPhysicalTypeV1::I32, &[10, 10, 10]);
        let rg0_payload = append_fixed_binary_chunk(
            &mut file_bytes,
            0,
            1,
            2,
            &[b"aa".as_slice(), b"bb".as_slice(), b"cc".as_slice()],
            OcbChunkCodecV1::Zstd,
        );
        let rg1_partition =
            append_chunk(&mut file_bytes, 1, 0, OcbPhysicalTypeV1::I32, &[11, 11, 11]);
        let rg1_payload = append_fixed_binary_chunk(
            &mut file_bytes,
            1,
            1,
            2,
            &[b"dd".as_slice(), b"ee".as_slice(), b"ff".as_slice()],
            OcbChunkCodecV1::Zstd,
        );

        let string_table = OcbStringTableV1 {
            version: 1,
            strings: vec!["partition_key".into(), "payload".into()],
            crc32c: 0,
        };
        let string_table_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
                string_table.write_to(buf)
            });

        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns: vec![
                column_desc(
                    0,
                    0,
                    OcbPhysicalTypeV1::I32,
                    OcbLogicalKindV1::OpaqueKey,
                    OCB_NULL_U32,
                ),
                fixed_binary_column_desc(1, 1, 2),
            ],
            crc32c: 0,
        };
        let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
            schema.write_to(buf)
        });

        let row_group_index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups: vec![
                OcbRowGroupDescV1 {
                    row_group_id: 0,
                    flags: 0,
                    base_row: 0,
                    row_count: 3,
                    chunk_desc_begin: 0,
                    chunk_desc_count: 2,
                    stat_begin: 0,
                    stat_count: 1,
                    first_key_tuple_ref: OcbBodyRefV2::NULL,
                    last_key_tuple_ref: OcbBodyRefV2::NULL,
                },
                OcbRowGroupDescV1 {
                    row_group_id: 1,
                    flags: 0,
                    base_row: 3,
                    row_count: 3,
                    chunk_desc_begin: 2,
                    chunk_desc_count: 2,
                    stat_begin: 1,
                    stat_count: 1,
                    first_key_tuple_ref: OcbBodyRefV2::NULL,
                    last_key_tuple_ref: OcbBodyRefV2::NULL,
                },
            ],
            column_chunks: vec![
                chunk_desc(0, 0, OcbPhysicalTypeV1::I32, rg0_partition, 3),
                chunk_desc_with_codec_bytes(
                    0,
                    1,
                    OcbPhysicalTypeV1::FixedBinary,
                    OcbChunkCodecV1::Zstd,
                    rg0_payload,
                    3,
                    6,
                ),
                chunk_desc(1, 0, OcbPhysicalTypeV1::I32, rg1_partition, 3),
                chunk_desc_with_codec_bytes(
                    1,
                    1,
                    OcbPhysicalTypeV1::FixedBinary,
                    OcbChunkCodecV1::Zstd,
                    rg1_payload,
                    3,
                    6,
                ),
            ],
            stats: vec![stats_i32(0, 0, 10, 10), stats_i32(1, 0, 11, 11)],
            crc32c: 0,
        };
        let row_group_index_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
                row_group_index.write_to(buf)
            });

        let ordering = OcbOrderingProofV1 {
            version: 1,
            flags: 0b1,
            keys: vec![ordering_key(0)],
            row_group_proofs: vec![
                OcbRowGroupOrderingProofV1 {
                    row_group_id: 0,
                    flags: 1,
                    first_tuple_ref: OcbBodyRefV2::NULL,
                    last_tuple_ref: OcbBodyRefV2::NULL,
                },
                OcbRowGroupOrderingProofV1 {
                    row_group_id: 1,
                    flags: 1,
                    first_tuple_ref: OcbBodyRefV2::NULL,
                    last_tuple_ref: OcbBodyRefV2::NULL,
                },
            ],
            crc32c: 0,
        };
        let ordering_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::OrderingProof, |buf| {
                ordering.write_to(buf)
            });

        let root = OcbRootV1 {
            version: 1,
            flags: 0,
            row_count: 6,
            column_count: 2,
            row_group_count: 2,
            dictionary_count: 0,
            schema_ref,
            dictionary_index_ref: OcbBodyRefV2::NULL,
            row_group_index_ref,
            ordering_proof_ref: ordering_ref,
            debug_json_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 0,
            content_flags: 0,
            crc32c: 0,
        };
        let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            root.write_to(buf)
        });
        assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);

        let bootstrap = OcbBootstrapPageV1::new([43u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);

        let mut file = fs::File::create(path).expect("create summary fixture");
        file.write_all(&file_bytes).expect("write summary fixture");
    }

    fn write_fixture_with_options(path: &Path, options: FixtureOptions) {
        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];

        let rg0_partition =
            append_chunk(&mut file_bytes, 0, 0, OcbPhysicalTypeV1::I32, &[10, 10, 10]);
        let rg0_order = append_chunk_i64(&mut file_bytes, 0, 1, &[100, 101, 102]);
        let rg0_category = append_chunk(&mut file_bytes, 0, 2, OcbPhysicalTypeV1::I32, &[1, 1, 2]);
        let rg1_partition =
            append_chunk(&mut file_bytes, 1, 0, OcbPhysicalTypeV1::I32, &[11, 11, 11]);
        let rg1_order = append_chunk_i64(&mut file_bytes, 1, 1, &[200, 201, 202]);
        let rg1_category = append_chunk(&mut file_bytes, 1, 2, OcbPhysicalTypeV1::I32, &[1, 2, 2]);

        let dictionary_values = OcbDictionaryValuesV1 {
            version: 1,
            value_kind: OcbDictionaryValueKindV1::Utf8,
            fixed_width: 0,
            values: vec![b"alpha".to_vec(), b"beta".to_vec()],
            crc32c: 0,
        };
        let dictionary_values_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::DictionaryValues, |buf| {
                dictionary_values.write_to(buf)
            });

        let string_table = OcbStringTableV1 {
            version: 1,
            strings: vec![
                "partition_key".into(),
                "order_key".into(),
                "category_code".into(),
                "category_dictionary".into(),
            ],
            crc32c: 0,
        };
        let string_table_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
                string_table.write_to(buf)
            });

        let mut columns = vec![
            column_desc(
                0,
                0,
                OcbPhysicalTypeV1::I32,
                OcbLogicalKindV1::OpaqueKey,
                OCB_NULL_U32,
            ),
            column_desc(
                1,
                1,
                OcbPhysicalTypeV1::I64,
                OcbLogicalKindV1::OpaqueKey,
                OCB_NULL_U32,
            ),
            column_desc(
                2,
                2,
                OcbPhysicalTypeV1::I32,
                OcbLogicalKindV1::DictionaryCode,
                0,
            ),
        ];
        if let Some(nullable_column_id) = options.nullable_column_id {
            columns
                .iter_mut()
                .find(|column| column.column_id == nullable_column_id)
                .expect("fixture nullable column exists")
                .nullability = OcbNullabilityV1::Nullable;
        }
        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns,
            crc32c: 0,
        };
        let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
            schema.write_to(buf)
        });

        let dictionary_index = OcbDictionaryIndexV1 {
            version: 1,
            dictionaries: vec![OcbDictionaryDescV1 {
                dictionary_id: 0,
                name_string_id: 3,
                code_physical_type: OcbPhysicalTypeV1::I32,
                value_kind: OcbDictionaryValueKindV1::Utf8,
                flags: 0,
                values_ref: dictionary_values_ref,
                entry_count: 2,
                reserved0: 0,
            }],
            crc32c: 0,
        };
        let dictionary_index_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::DictionaryIndex, |buf| {
                dictionary_index.write_to(buf)
            });

        let forced_validity_ref = if options.validity_ref_column_id.is_some() {
            append_raw_object(
                &mut file_bytes,
                OcbBodyKindV1::ValidityBitmap,
                vec![0b0000_0101],
            )
        } else {
            OcbBodyRefV2::NULL
        };
        let validity_ref_for = |row_group_id: u32, column_id: u32| {
            if row_group_id == 0 && options.validity_ref_column_id == Some(column_id) {
                forced_validity_ref
            } else {
                OcbBodyRefV2::NULL
            }
        };

        let row_group_index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups: vec![
                OcbRowGroupDescV1 {
                    row_group_id: 0,
                    flags: 0,
                    base_row: 0,
                    row_count: 3,
                    chunk_desc_begin: options.row_group0_chunk_desc_begin.unwrap_or(0),
                    chunk_desc_count: 3,
                    stat_begin: 0,
                    stat_count: 2,
                    first_key_tuple_ref: OcbBodyRefV2::NULL,
                    last_key_tuple_ref: OcbBodyRefV2::NULL,
                },
                OcbRowGroupDescV1 {
                    row_group_id: 1,
                    flags: 0,
                    base_row: 3,
                    row_count: 3,
                    chunk_desc_begin: 3,
                    chunk_desc_count: 3,
                    stat_begin: 2,
                    stat_count: 2,
                    first_key_tuple_ref: OcbBodyRefV2::NULL,
                    last_key_tuple_ref: OcbBodyRefV2::NULL,
                },
            ],
            column_chunks: vec![
                chunk_desc_with_validity(
                    0,
                    0,
                    OcbPhysicalTypeV1::I32,
                    rg0_partition,
                    validity_ref_for(0, 0),
                    3,
                ),
                chunk_desc_with_validity(
                    0,
                    1,
                    OcbPhysicalTypeV1::I64,
                    rg0_order,
                    validity_ref_for(0, 1),
                    3,
                ),
                chunk_desc_with_validity(
                    0,
                    2,
                    OcbPhysicalTypeV1::I32,
                    rg0_category,
                    validity_ref_for(0, 2),
                    3,
                ),
                chunk_desc(1, 0, OcbPhysicalTypeV1::I32, rg1_partition, 3),
                chunk_desc(1, 1, OcbPhysicalTypeV1::I64, rg1_order, 3),
                chunk_desc(1, 2, OcbPhysicalTypeV1::I32, rg1_category, 3),
            ],
            stats: vec![
                stats_i32(0, 0, 10, 10),
                stats_i64(0, 1, 100, 102),
                stats_i32(1, 0, 11, 11),
                stats_i64(1, 1, 200, 202),
            ],
            crc32c: 0,
        };
        let row_group_index_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
                row_group_index.write_to(buf)
            });

        let ordering = OcbOrderingProofV1 {
            version: 1,
            flags: 0b11,
            keys: vec![ordering_key(0), ordering_key(1)],
            row_group_proofs: vec![
                OcbRowGroupOrderingProofV1 {
                    row_group_id: 0,
                    flags: 1,
                    first_tuple_ref: OcbBodyRefV2::NULL,
                    last_tuple_ref: OcbBodyRefV2::NULL,
                },
                OcbRowGroupOrderingProofV1 {
                    row_group_id: 1,
                    flags: 1,
                    first_tuple_ref: OcbBodyRefV2::NULL,
                    last_tuple_ref: OcbBodyRefV2::NULL,
                },
            ],
            crc32c: 0,
        };
        let ordering_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::OrderingProof, |buf| {
                ordering.write_to(buf)
            });

        let root = OcbRootV1 {
            version: 1,
            flags: 0,
            row_count: 6,
            column_count: 3,
            row_group_count: 2,
            dictionary_count: 1,
            schema_ref,
            dictionary_index_ref,
            row_group_index_ref,
            ordering_proof_ref: ordering_ref,
            debug_json_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 0,
            content_flags: 0,
            crc32c: 0,
        };
        let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            root.write_to(buf)
        });
        assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);

        let bootstrap = OcbBootstrapPageV1::new([42u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);

        let mut file = fs::File::create(path).expect("create fixture");
        file.write_all(&file_bytes).expect("write fixture");
    }

    fn append_chunk(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        values: &[i32],
    ) -> OcbBodyRefV2 {
        let mut payload = Vec::with_capacity(values.len() * 4);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id,
            column_id,
            row_count: values.len() as u64,
            uncompressed_bytes: (values.len() * 4) as u64,
            payload,
            crc32c: 0,
        };
        append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
            chunk.write_to(buf)
        })
    }

    fn append_chunk_i64(
        file_bytes: &mut Vec<u8>,
        row_group_id: u32,
        column_id: u32,
        values: &[i64],
    ) -> OcbBodyRefV2 {
        let mut payload = Vec::with_capacity(values.len() * 8);
        for value in values {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::I64,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id,
            column_id,
            row_count: values.len() as u64,
            uncompressed_bytes: (values.len() * 8) as u64,
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
                zstd::stream::encode_all(Cursor::new(raw_payload.as_slice()), 1)
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

    fn column_desc(
        column_id: u32,
        name_string_id: u32,
        physical_type: OcbPhysicalTypeV1,
        logical_kind: OcbLogicalKindV1,
        dictionary_id: u32,
    ) -> OcbColumnDescV1 {
        OcbColumnDescV1 {
            column_id,
            name_string_id,
            physical_type,
            logical_kind,
            flags: 0,
            dictionary_id,
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

    fn chunk_desc(
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        value_ref: OcbBodyRefV2,
        row_count: u64,
    ) -> OcbColumnChunkDescV1 {
        chunk_desc_with_validity(
            row_group_id,
            column_id,
            physical_type,
            value_ref,
            OcbBodyRefV2::NULL,
            row_count,
        )
    }

    fn chunk_desc_with_codec_bytes(
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        codec: OcbChunkCodecV1,
        value_ref: OcbBodyRefV2,
        row_count: u64,
        uncompressed_bytes: u64,
    ) -> OcbColumnChunkDescV1 {
        OcbColumnChunkDescV1 {
            row_group_id,
            column_id,
            physical_type,
            codec,
            flags: 0,
            value_ref,
            validity_ref: OcbBodyRefV2::NULL,
            row_count,
            uncompressed_bytes,
        }
    }

    fn chunk_desc_with_validity(
        row_group_id: u32,
        column_id: u32,
        physical_type: OcbPhysicalTypeV1,
        value_ref: OcbBodyRefV2,
        validity_ref: OcbBodyRefV2,
        row_count: u64,
    ) -> OcbColumnChunkDescV1 {
        OcbColumnChunkDescV1 {
            row_group_id,
            column_id,
            physical_type,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            value_ref,
            validity_ref,
            row_count,
            uncompressed_bytes: row_count
                * physical_type
                    .primitive_byte_width()
                    .expect("fixture uses primitive physical types") as u64,
        }
    }

    fn stats_i32(row_group_id: u32, column_id: u32, min: i32, max: i32) -> OcbColumnStatsV1 {
        OcbColumnStatsV1 {
            row_group_id,
            column_id,
            physical_type: OcbPhysicalTypeV1::I32,
            flags: 0,
            null_count: 0,
            min_value: OcbStatScalarV1::I32(min),
            max_value: OcbStatScalarV1::I32(max),
        }
    }

    fn stats_i64(row_group_id: u32, column_id: u32, min: i64, max: i64) -> OcbColumnStatsV1 {
        OcbColumnStatsV1 {
            row_group_id,
            column_id,
            physical_type: OcbPhysicalTypeV1::I64,
            flags: 0,
            null_count: 0,
            min_value: OcbStatScalarV1::I64(min),
            max_value: OcbStatScalarV1::I64(max),
        }
    }

    fn ordering_key(column_id: u32) -> OcbOrderingKeyV1 {
        OcbOrderingKeyV1 {
            column_id,
            direction: OcbOrderingDirectionV1::Ascending,
            null_order: OcbNullOrderV1::NoNulls,
            reserved0: 0,
        }
    }
}
