#![allow(dead_code)]

//! One-file ordered column bundle facade used by the public `format-ocb` API.
//!
//! This module is deliberately generic. It exposes columnar batches and logical
//! annotations, but it does not attach market-data, order, trade, or replay
//! semantics to columns.
//!
//! Public model types and the stable reader facade remain here. Private planning,
//! execution, materialization, and attribution details live in focused child
//! modules; binary format, object I/O, and parallel scheduling retain their
//! existing crate-level owners.

mod attribution;
mod execution;
mod materialize;
mod planning;

#[cfg(test)]
mod tests;

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
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
    ColumnBundleParallelPrepareReport, OrderedCommitMode, OrderedCommitPanicMode,
    ParallelPrepareTaskSpec, execute_bounded_ordered, execute_parallel_prepare,
};
use crate::read::{
    OcbMetadataV1, OcbOpenValidationMode, OcbReadObjectAttribution, OcbReadSource,
    read_column_chunk_from_source_with_attribution_and_resource_limits,
    read_column_chunk_from_source_with_resource_limits,
    read_metadata_from_source_with_validation_and_resource_limits,
    read_object_bytes_with_attribution_and_resource_limits, read_object_bytes_with_resource_limits,
    read_uncompressed_fixed_binary_chunk_from_source_into_with_attribution_and_resource_limits,
    read_uncompressed_fixed_binary_chunk_from_source_into_with_resource_limits,
};
use crate::resource_limits::{MetadataMaterializationBudget, OcbResourceLimits};
use crate::{ArcadiaTioError, OcbFailureCause, Result};

use attribution::record_value_materialization_time;
pub(crate) use attribution::{attribution_from_accumulator, duration_to_ns};
use execution::*;
use materialize::*;
use planning::*;

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
    fn new(field_type: FixedBinaryFieldType, capacity: usize) -> Result<Self> {
        Ok(match field_type {
            FixedBinaryFieldType::U8 => Self::U8(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::I8 => Self::I8(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::U16Le => Self::U16(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::I16Le => Self::I16(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::U32Le => Self::U32(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::I32Le => Self::I32(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::U64Le => Self::U64(zeroed_vec_fallibly(capacity)?),
            FixedBinaryFieldType::I64Le => Self::I64(zeroed_vec_fallibly(capacity)?),
        })
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
        let mut fields = try_column_bundle_vec_with_capacity(projection.fields.len())?;
        for field in &projection.fields {
            fields.push(ColumnBundleFixedBinaryProjectedFieldBuffer {
                name: field
                    .name
                    .as_deref()
                    .map(clone_column_bundle_string_fallibly)
                    .transpose()?,
                offset: field.offset,
                values: ReusableFixedBinaryFieldValues::new(field.field_type, capacity)?,
            });
        }
        Ok(Self {
            source_column_id: source_column.id,
            source_column_name: clone_column_bundle_string_fallibly(&source_column.name)?,
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
            Self::I32(values) => resize_vec_fallibly(values, row_count, 0)?,
            Self::I64(values) => resize_vec_fallibly(values, row_count, 0)?,
            Self::F32(values) => resize_vec_fallibly(values, row_count, 0.0)?,
            Self::F64(values) => resize_vec_fallibly(values, row_count, 0.0)?,
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
                resize_vec_fallibly(bytes, byte_count, 0)?;
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
            name: clone_column_bundle_string_fallibly(&column.name)?,
            physical_type: column.physical_type,
            logical_kind: column.logical_kind,
            dictionary_id: column.dictionary_id,
            nullable: column.nullable,
            values,
            validity_bytes: zeroed_vec_fallibly(validity_capacity)?,
            allow_nulls,
        })
    }

    fn prepare_for_rows(&mut self, row_count: usize) -> Result<()> {
        self.values.resize_for_rows(row_count)?;
        if self.allow_nulls {
            resize_vec_fallibly(&mut self.validity_bytes, row_count.div_ceil(8), 0)?;
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
        columns: &[&BundleColumn],
        row_capacity: usize,
        allow_nulls: bool,
    ) -> Result<Self> {
        let mut buffers = try_column_bundle_vec_with_capacity(columns.len())?;
        for column in columns {
            buffers.push(ColumnBundleReusableColumnBuffer::for_column(
                column,
                row_capacity,
                allow_nulls,
            )?);
        }
        Ok(Self { columns: buffers })
    }

    fn prepare_for_rows(&mut self, row_count: usize) -> Result<()> {
        for column in &mut self.columns {
            column.prepare_for_rows(row_count)?;
        }
        Ok(())
    }

    fn fill_buffers(&mut self) -> Result<Vec<ColumnBundleColumnFillBuffer<'_>>> {
        let mut buffers = try_column_bundle_vec_with_capacity(self.columns.len())?;
        buffers.extend(
            self.columns
                .iter_mut()
                .map(ColumnBundleReusableColumnBuffer::as_fill_buffer),
        );
        Ok(buffers)
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
        let mut seen = try_column_bundle_hash_set_with_capacity(ranges.len())?;
        let mut predicates = try_column_bundle_vec_with_capacity(ranges.len())?;
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
                clone_column_bundle_string_fallibly(&key.column_name)?,
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
    source: Arc<OcbReadSource>,
    metadata: Arc<OcbMetadataV1>,
    columns: Arc<Vec<BundleColumn>>,
    resource_limits: OcbResourceLimits,
    open_metadata_materialized_bytes: u64,
}

impl ColumnBundleFile {
    /// Open one OCB file using metadata-graph validation.
    ///
    /// Column payload CRCs are still checked when selected chunks are read. Use
    /// [`Self::open_with_options`] with [`ColumnBundleOpenValidation::FullPayload`]
    /// when whole-file payload integrity must be verified before reads.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::open_with_options_and_resource_limits(
            path,
            ColumnBundleOpenOptions::default(),
            OcbResourceLimits::policy_a(),
        )
    }

    /// Open one OCB file using explicit finite resource limits.
    pub fn open_with_resource_limits(
        path: impl AsRef<Path>,
        resource_limits: OcbResourceLimits,
    ) -> Result<Self> {
        Self::open_with_options_and_resource_limits(
            path,
            ColumnBundleOpenOptions::default(),
            resource_limits,
        )
    }

    /// Open one OCB file with explicit validation options.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: ColumnBundleOpenOptions,
    ) -> Result<Self> {
        Self::open_with_options_and_resource_limits(path, options, OcbResourceLimits::policy_a())
    }

    /// Open one OCB file with explicit validation and finite resource limits.
    pub fn open_with_options_and_resource_limits(
        path: impl AsRef<Path>,
        options: ColumnBundleOpenOptions,
        resource_limits: OcbResourceLimits,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let source = Arc::new(OcbReadSource::open(&path)?);
        Self::open_from_source_with_options_and_resource_limits(source, options, resource_limits)
    }

    pub(crate) fn open_from_source_with_resource_limits(
        source: Arc<OcbReadSource>,
        resource_limits: OcbResourceLimits,
    ) -> Result<Self> {
        Self::open_from_source_with_options_and_resource_limits(
            source,
            ColumnBundleOpenOptions::default(),
            resource_limits,
        )
    }

    fn open_from_source_with_options_and_resource_limits(
        source: Arc<OcbReadSource>,
        options: ColumnBundleOpenOptions,
        resource_limits: OcbResourceLimits,
    ) -> Result<Self> {
        let metadata = Arc::new(
            read_metadata_from_source_with_validation_and_resource_limits(
                &source,
                OcbOpenValidationMode::from(options.validation),
                resource_limits,
            )?,
        );
        Self::from_metadata(source, metadata, resource_limits)
    }

    fn from_metadata(
        source: Arc<OcbReadSource>,
        metadata: Arc<OcbMetadataV1>,
        resource_limits: OcbResourceLimits,
    ) -> Result<Self> {
        validate_metadata(&metadata)?;
        let mut metadata_budget = MetadataMaterializationBudget::from_limits(resource_limits);
        metadata_budget.charge(metadata.open_metadata_materialized_bytes)?;
        let resolved_columns_materialized_bytes =
            preflight_resolved_columns_materialized_bytes(&metadata)?;
        metadata_budget.charge(resolved_columns_materialized_bytes)?;
        let open_metadata_materialized_bytes = metadata_budget.charged_bytes();
        let columns = Arc::new(resolve_columns(&metadata)?);
        Ok(Self {
            source,
            metadata,
            columns,
            resource_limits,
            open_metadata_materialized_bytes,
        })
    }

    /// Return the finite resource policy selected when this handle was opened.
    pub const fn resource_limits(&self) -> OcbResourceLimits {
        self.resource_limits
    }

    pub(crate) fn read_source(&self) -> &OcbReadSource {
        &self.source
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
        let dictionary_count = self
            .metadata
            .dictionary_index
            .as_ref()
            .map_or(0, |index| index.dictionaries.len());
        let mut dictionaries = try_column_bundle_vec_with_capacity(dictionary_count)?;
        if let Some(index) = &self.metadata.dictionary_index {
            for dictionary in &index.dictionaries {
                let name = self
                    .metadata
                    .string_table
                    .strings
                    .get(dictionary.name_string_id as usize)
                    .ok_or(ArcadiaTioError::ocb_corrupt_file(
                        "OCB dictionary name string id is out of range",
                    ))?;
                dictionaries.push(BundleDictionaryDescriptor {
                    dictionary_id: dictionary.dictionary_id,
                    name: clone_column_bundle_string_fallibly(name)?,
                    code_physical_type: scalar_column_physical_type(dictionary.code_physical_type)?,
                    value_kind: dictionary.value_kind.into(),
                    entry_count: dictionary.entry_count,
                });
            }
        }

        let ordering_key_count = self
            .metadata
            .ordering_proof
            .as_ref()
            .map_or(0, |proof| proof.keys.len());
        let mut ordering_keys = try_column_bundle_vec_with_capacity(ordering_key_count)?;
        if let Some(proof) = &self.metadata.ordering_proof {
            for key in &proof.keys {
                let column = self
                    .columns
                    .iter()
                    .find(|column| column.id == key.column_id)
                    .ok_or(ArcadiaTioError::ocb_corrupt_file(
                        "OCB ordering key column id is out of range",
                    ))?;
                ordering_keys.push(BundleOrderingKey {
                    column_id: key.column_id,
                    column_name: clone_column_bundle_string_fallibly(&column.name)?,
                    direction: key.direction.into(),
                    null_order: key.null_order.into(),
                });
            }
        }

        let mut columns = try_column_bundle_vec_with_capacity(self.columns.len())?;
        for column in self.columns.iter() {
            columns.push(BundleColumn {
                id: column.id,
                name: clone_column_bundle_string_fallibly(&column.name)?,
                physical_type: column.physical_type,
                logical_kind: column.logical_kind,
                dictionary_id: column.dictionary_id,
                scale: column.scale,
                nullable: column.nullable,
            });
        }

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
            columns,
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
        let mut projected_column_ids =
            try_column_bundle_hash_set_with_capacity(plan.projected_column_ids.len())?;
        projected_column_ids.extend(plan.projected_column_ids.iter().copied());
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
        let mut metadata_budget = MetadataMaterializationBudget::from_limits(self.resource_limits);
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
        let mut file = self.source.cursor();
        let bytes = read_object_bytes_with_resource_limits(
            &mut file,
            self.metadata.file_len,
            dictionary.values_ref,
            OcbBodyKindV1::DictionaryValues,
            self.resource_limits,
        )?;
        let raw_values =
            OcbDictionaryValuesV1::read_from_bytes_with_budget(bytes, &mut metadata_budget)?;
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
            ))?;
        let conversion_materialized_bytes =
            preflight_dictionary_conversion_materialized_bytes(&raw_values)?;
        let name_bytes = u64::try_from(name.len()).map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file("OCB dictionary name length exceeds u64")
        })?;
        let returned_materialized_bytes = conversion_materialized_bytes
            .checked_add(name_bytes)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary result materialization size overflows",
            ))?;
        metadata_budget.charge(returned_materialized_bytes)?;
        let name = clone_column_bundle_string_fallibly(name)?;
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

        let mut row_group_ids =
            try_column_bundle_vec_with_capacity(self.metadata.row_group_index.row_groups.len())?;
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
        let mut row_group_ids =
            try_column_bundle_vec_with_capacity(self.metadata.row_group_index.row_groups.len())?;
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
        let mut projected_column_ids = try_column_bundle_vec_with_capacity(self.columns.len())?;
        projected_column_ids.extend(self.columns.iter().map(|column| column.id));
        let mut row_group_ids = try_column_bundle_vec_with_capacity(1)?;
        row_group_ids.push(row_group_id);
        let plan = ColumnBundleReadPlan {
            projected_column_ids,
            row_group_ids,
            report: build_read_report(1, 1, 0, self.columns.len()),
        };
        self.validate_read_plan(&plan)?;
        validate_owned_plan_resource_limits(&self.metadata, &plan)?;
        read_row_group(
            &self.source,
            &self.metadata,
            &self.columns,
            row_group_id,
            &plan.projected_column_ids,
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
            &self.source,
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
        validate_sliding_in_flight_plan_resource_limits(
            &self.metadata,
            plan,
            max_in_flight_row_groups,
        )?;
        validate_reusable_pool_allocation_resource_limits(
            &self.metadata,
            &projected_columns,
            row_capacity,
            max_in_flight_row_groups,
            allow_nulls,
        )?;
        let mut buffers = try_column_bundle_vec_with_capacity(max_in_flight_row_groups)?;
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
        validate_in_flight_plan_resource_limits(&self.metadata, plan, 1)?;
        let row_capacity = max_row_count_for_plan(&self.metadata, plan)?;
        validate_fixed_binary_projection_resource_limits(
            &self.metadata,
            plan,
            projection,
            row_capacity,
        )?;
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
            row_group_ids: clone_column_bundle_u32s_fallibly(&plan.row_group_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
            row_group_ids: clone_column_bundle_u32s_fallibly(&plan.row_group_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
        let (tasks, report) =
            self.parallel_prepare_tasks_and_report(plan, row_group_ids, options)?;
        execute_parallel_prepare(
            tasks,
            report,
            0,
            options,
            |row_group_id| {
                read_row_group_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )
            },
            move |context, batch| prepare(context, &batch),
            ordered_commit,
        )
    }

    /// Unsupported owned-batch adapter for the private pull-session façade.
    #[doc(hidden)]
    pub fn __parallel_prepare_plan_row_groups_owned_for_private_adapter<T, Prepare, Commit>(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        options: ColumnBundleParallelPrepareOptions,
        prepare: Prepare,
        ordered_commit: Commit,
    ) -> Result<ColumnBundleParallelPrepareReport>
    where
        T: Send + 'static,
        Prepare: Fn(ColumnBundleParallelPrepareContext, ColumnBatch) -> Result<T> + Sync,
        Commit: FnMut(ColumnBundleParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
    {
        let (tasks, report) =
            self.parallel_prepare_tasks_and_report(plan, row_group_ids, options)?;
        execute_parallel_prepare(
            tasks,
            report,
            0,
            options,
            |row_group_id| {
                read_row_group_with_attribution(
                    &self.source,
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

    fn parallel_prepare_tasks_and_report(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
        options: ColumnBundleParallelPrepareOptions,
    ) -> Result<(Vec<ParallelPrepareTaskSpec>, ColumnBundleReadReport)> {
        self.validate_read_plan(plan)?;
        let selected_row_group_ids = planned_row_group_subset(plan, row_group_ids)?;
        let report = execution_report_for_plan(plan, selected_row_group_ids.len());
        validate_sliding_in_flight_plan_resource_limits(
            &self.metadata,
            &ColumnBundleReadPlan {
                projected_column_ids: clone_column_bundle_u32s_fallibly(
                    &plan.projected_column_ids,
                )?,
                row_group_ids: clone_column_bundle_u32s_fallibly(&selected_row_group_ids)?,
                report: report.clone(),
            },
            options.max_in_flight_row_groups,
        )?;
        let mut tasks = try_column_bundle_vec_with_capacity(selected_row_group_ids.len())?;
        for (selected_row_group_ordinal, row_group_id) in
            selected_row_group_ids.iter().copied().enumerate()
        {
            let row_group = self.metadata.row_group_by_id(row_group_id).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB parallel prepare row group not found"),
            )?;
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
        Ok((tasks, report))
    }

    /// Unsupported subset validator for the private pull-session façade.
    #[doc(hidden)]
    pub fn __validate_plan_row_group_subset_for_private_adapter(
        &self,
        plan: &ColumnBundleReadPlan,
        row_group_ids: &[u32],
    ) -> Result<Vec<u32>> {
        self.validate_read_plan(plan)?;
        planned_row_group_subset(plan, row_group_ids)
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
            row_group_ids: clone_column_bundle_u32s_fallibly(&plan.row_group_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
            row_group_ids: clone_column_bundle_u32s_fallibly(&plan.row_group_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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
            projected_column_ids: clone_column_bundle_u32s_fallibly(&plan.projected_column_ids)?,
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

    fn bounded_ordered_tasks_for_plan(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<Vec<ParallelPrepareTaskSpec>> {
        let mut tasks = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
        for (selected_row_group_ordinal, row_group_id) in
            plan.row_group_ids.iter().copied().enumerate()
        {
            let row_group = self.metadata.row_group_by_id(row_group_id).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB bounded scheduler row group not found"),
            )?;
            let row_end = row_group.base_row.checked_add(row_group.row_count).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB bounded scheduler row range overflows"),
            )?;
            tasks.push(ParallelPrepareTaskSpec {
                selected_row_group_ordinal,
                row_group_id,
                base_row: row_group.base_row,
                row_end,
                row_count: row_group.row_count,
            });
        }
        Ok(tasks)
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
        let mut serial_report = ColumnBundleReadCursorReport {
            base_report: plan.report.clone(),
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        };
        if plan.row_group_ids.is_empty() {
            return Ok(serial_report);
        }
        let wave_size = plan
            .report
            .effective_threads
            .max(1)
            .min(cursor_options.max_in_flight_row_groups.max(1));
        validate_contiguous_in_flight_plan_resource_limits(&self.metadata, plan, wave_size)?;
        if wave_size <= 1 {
            serial_report.max_in_flight_row_groups_observed = 1;
            for row_group_id in plan.row_group_ids.iter().copied() {
                let batch = read_row_group(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )?;
                let row_count = batch.row_count;
                match visitor(batch)? {
                    ColumnBundleVisitControl::Continue => {
                        serial_report.batches_yielded =
                            serial_report.batches_yielded.saturating_add(1);
                        serial_report.rows_yielded =
                            serial_report.rows_yielded.saturating_add(row_count);
                    }
                    ColumnBundleVisitControl::Stop => {
                        serial_report.batches_yielded =
                            serial_report.batches_yielded.saturating_add(1);
                        serial_report.rows_yielded =
                            serial_report.rows_yielded.saturating_add(row_count);
                        serial_report.cancelled = true;
                        return Ok(serial_report);
                    }
                }
            }
            return Ok(serial_report);
        }

        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: wave_size,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            false,
            |context| {
                let batch = read_row_group(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &plan.projected_column_ids,
                )?;
                Ok((batch, ReadAttributionAccumulator::default()))
            },
            |_, batch| Ok(batch),
            |_, batch| visitor(batch),
        )?;
        let mut cursor_report = outcome.report.cursor_report;
        cursor_report.base_report = plan.report.clone();
        Ok(cursor_report)
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
        validate_contiguous_in_flight_plan_resource_limits(&self.metadata, plan, wave_size)?;
        if wave_size <= 1 {
            cursor_report.max_in_flight_row_groups_observed = 1;
            for row_group_id in plan.row_group_ids.iter().copied() {
                let (batch, row_attr) = read_row_group_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    row_group_id,
                    &plan.projected_column_ids,
                )?;
                accumulator.add(row_attr);
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

        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            plan_ns,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: wave_size,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            true,
            |context| {
                read_row_group_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &plan.projected_column_ids,
                )
            },
            |_, batch| Ok(batch),
            |_, batch| visitor(batch),
        )?;
        let mut cursor_report = outcome.report.cursor_report;
        cursor_report.base_report = plan.report.clone();
        let attribution = attribution_from_accumulator(
            outcome.attribution_accumulator,
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
        let cursor_report = ColumnBundleReadCursorReport {
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
        validate_contiguous_in_flight_plan_resource_limits(&self.metadata, plan, wave_size)?;
        let mut slots = try_column_bundle_vec_with_capacity(wave_size)?;
        slots.extend(buffers.buffers[..wave_size].iter_mut().map(Mutex::new));
        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: wave_size,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            false,
            |context| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let mut slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let report = read_row_group_into_reusable(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &mut slot,
                )?;
                Ok((report, ReadAttributionAccumulator::default()))
            },
            |_, report| Ok(report),
            |context, report| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let view = ColumnBundleReusableBatchView {
                    report: &report,
                    buffers: &slot,
                };
                visitor(view)
            },
        )?;
        let mut cursor_report = outcome.report.cursor_report;
        cursor_report.base_report = plan.report.clone();
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
        let accumulator = ReadAttributionAccumulator::default();
        let cursor_report = ColumnBundleReadCursorReport {
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
        validate_contiguous_in_flight_plan_resource_limits(&self.metadata, plan, wave_size)?;
        let mut slots = try_column_bundle_vec_with_capacity(wave_size)?;
        slots.extend(buffers.buffers[..wave_size].iter_mut().map(Mutex::new));
        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            plan_ns,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: wave_size,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            true,
            |context| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let mut slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                read_row_group_into_reusable_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &mut slot,
                )
            },
            |_, report| Ok(report),
            |context, report| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let view = ColumnBundleReusableBatchView {
                    report: &report,
                    buffers: &slot,
                };
                visitor(view)
            },
        )?;
        let mut cursor_report = outcome.report.cursor_report;
        cursor_report.base_report = plan.report.clone();
        let attribution = attribution_from_accumulator(
            outcome.attribution_accumulator,
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
        let cursor_report = ColumnBundleReadCursorReport {
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
        validate_contiguous_in_flight_plan_resource_limits(&self.metadata, plan, wave_size)?;
        let mut slots = try_column_bundle_vec_with_capacity(wave_size)?;
        slots.extend(buffers.buffers[..wave_size].iter_mut().map(Mutex::new));
        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: wave_size,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            false,
            |context| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let mut slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                read_row_group_into_reusable_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &mut slot,
                )
            },
            |_, report| Ok(report),
            |context, report| {
                let slot_index = context.selected_row_group_ordinal % wave_size;
                let slot = slots[slot_index]
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let view = ColumnBundleReusableBatchView {
                    report: &report,
                    buffers: &slot,
                };
                let projection_report =
                    project_fixed_binary_reusable_batch(&view, projection, projection_buffer)?;
                accumulator.fixed_payload_decode +=
                    Duration::from_nanos(projection_report.projection_wall_ns);
                let projected_view = projection_buffer.view(report.row_group_id, report.base_row);
                let callback_started = Instant::now();
                let control = visitor(view, projected_view);
                accumulator.callback += callback_started.elapsed();
                control
            },
        )?;
        accumulator.add(outcome.attribution_accumulator);
        let mut cursor_report = outcome.report.cursor_report;
        cursor_report.base_report = plan.report.clone();
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
        validate_owned_plan_resource_limits(&self.metadata, plan)?;
        if plan.row_group_ids.is_empty() {
            return Ok(Vec::new());
        }
        if plan.report.effective_threads <= 1 {
            let mut batches = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
            for row_group_id in &plan.row_group_ids {
                batches.push(read_row_group(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    *row_group_id,
                    &plan.projected_column_ids,
                )?);
            }
            return Ok(batches);
        }

        let mut batches = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
        execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: plan.report.effective_threads,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            false,
            |context| {
                let batch = read_row_group(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &plan.projected_column_ids,
                )?;
                Ok((batch, ReadAttributionAccumulator::default()))
            },
            |_, batch| Ok(batch),
            |_, batch| {
                batches.push(batch);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )?;
        Ok(batches)
    }

    fn execute_plan_with_attribution(
        &self,
        plan: &ColumnBundleReadPlan,
    ) -> Result<(Vec<ColumnBatch>, ReadAttributionAccumulator)> {
        validate_owned_plan_resource_limits(&self.metadata, plan)?;
        if plan.row_group_ids.is_empty() {
            return Ok((Vec::new(), ReadAttributionAccumulator::default()));
        }
        if plan.report.effective_threads <= 1 {
            let mut batches = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
            let mut attribution = ReadAttributionAccumulator::default();
            for row_group_id in &plan.row_group_ids {
                let (batch, row_attr) = read_row_group_with_attribution(
                    &self.source,
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

        let mut batches = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
        let outcome = execute_bounded_ordered(
            self.bounded_ordered_tasks_for_plan(plan)?,
            plan.report.clone(),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: plan.report.effective_threads,
            },
            OrderedCommitMode::Windowed,
            OrderedCommitPanicMode::Propagate,
            false,
            |context| {
                read_row_group_with_attribution(
                    &self.source,
                    &self.metadata,
                    &self.columns,
                    context.row_group_id,
                    &plan.projected_column_ids,
                )
            },
            |_, batch| Ok(batch),
            |_, batch| {
                batches.push(batch);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )?;
        Ok((batches, outcome.attribution_accumulator))
    }

    fn validate_read_plan(&self, plan: &ColumnBundleReadPlan) -> Result<()> {
        let mut available_columns = try_column_bundle_hash_set_with_capacity(self.columns.len())?;
        available_columns.extend(self.columns.iter().map(|column| column.id));
        let mut seen_columns =
            try_column_bundle_hash_set_with_capacity(plan.projected_column_ids.len())?;
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

        let mut seen_row_groups =
            try_column_bundle_hash_set_with_capacity(plan.row_group_ids.len())?;
        for row_group_id in &plan.row_group_ids {
            if self.metadata.row_group_by_id(*row_group_id).is_none() {
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
        projected_column_ids: Option<&HashSet<u32>>,
    ) -> Result<Vec<ColumnBundleRowGroupSummary>>
    where
        I: IntoIterator<Item = u32>,
    {
        let row_group_count = self.metadata.row_group_index.row_groups.len();
        let mut summaries = try_column_bundle_vec_with_capacity(row_group_count)?;
        let mut seen = try_column_bundle_hash_set_with_capacity(row_group_count)?;
        for row_group_id in row_group_ids {
            if !seen.insert(row_group_id) {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB row-group summary request contains duplicate row group ids",
                ));
            }
            let row_group = self.metadata.row_group_by_id(row_group_id).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB row-group summary request references an unknown row group id",
                ),
            )?;
            summaries.push(self.build_row_group_summary(row_group, projected_column_ids)?);
        }
        Ok(summaries)
    }

    fn build_row_group_summary(
        &self,
        row_group: &OcbRowGroupDescV1,
        projected_column_ids: Option<&HashSet<u32>>,
    ) -> Result<ColumnBundleRowGroupSummary> {
        let chunks = column_chunks_for_row_group(
            &self.metadata,
            row_group.chunk_desc_begin,
            row_group.chunk_desc_count,
        )?;
        let mut chunk_summaries = try_column_bundle_vec_with_capacity(chunks.len())?;
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
                column_name: clone_column_bundle_string_fallibly(&column.name)?,
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
        let mut stat_summaries = try_column_bundle_vec_with_capacity(stats.len())?;
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
                column_name: clone_column_bundle_string_fallibly(&column.name)?,
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
            ColumnProjection::All => {
                let mut selected = try_column_bundle_vec_with_capacity(self.columns.len())?;
                selected.extend(self.columns.iter().map(|column| column.id));
                Ok(selected)
            }
            ColumnProjection::Names(names) => {
                let mut by_name = try_column_bundle_hash_map_with_capacity(self.columns.len())?;
                by_name.extend(
                    self.columns
                        .iter()
                        .map(|column| (column.name.as_str(), column.id)),
                );
                let mut selected = try_column_bundle_vec_with_capacity(names.len())?;
                let mut seen = try_column_bundle_hash_set_with_capacity(names.len())?;
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
        let mut by_name = try_column_bundle_hash_map_with_capacity(self.columns.len())?;
        by_name.extend(
            self.columns
                .iter()
                .map(|column| (column.name.as_str(), column)),
        );
        let mut resolved = try_column_bundle_vec_with_capacity(predicates.len())?;
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
        OcbDictionaryValueKindV1::Utf8 => {
            decode_utf8_dictionary_values(raw.values, "OCB UTF-8 dictionary value is invalid")
                .map(DictionaryValues::Utf8)
        }
        OcbDictionaryValueKindV1::Bytes => Ok(DictionaryValues::Bytes(raw.values)),
        OcbDictionaryValueKindV1::FixedBytes => Ok(DictionaryValues::FixedBytes {
            fixed_width: raw.fixed_width,
            values: raw.values,
        }),
        OcbDictionaryValueKindV1::EnumLabels => {
            decode_utf8_dictionary_values(raw.values, "OCB enum-label dictionary value is invalid")
                .map(DictionaryValues::EnumLabels)
        }
    }
}

fn preflight_dictionary_conversion_materialized_bytes(raw: &OcbDictionaryValuesV1) -> Result<u64> {
    let invalid_message = match raw.value_kind {
        OcbDictionaryValueKindV1::Utf8 => Some("OCB UTF-8 dictionary value is invalid"),
        OcbDictionaryValueKindV1::EnumLabels => Some("OCB enum-label dictionary value is invalid"),
        OcbDictionaryValueKindV1::Bytes | OcbDictionaryValueKindV1::FixedBytes => None,
    };
    let Some(invalid_message) = invalid_message else {
        return Ok(0);
    };
    for value in &raw.values {
        std::str::from_utf8(value)
            .map_err(|_| ArcadiaTioError::ocb_corrupt_file(invalid_message))?;
    }
    let value_count = u64::try_from(raw.values.len())
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB dictionary value count exceeds u64"))?;
    value_count
        .checked_mul(std::mem::size_of::<String>() as u64)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB dictionary string descriptor materialization size overflows",
        ))
}

fn decode_utf8_dictionary_values(
    values: Vec<Vec<u8>>,
    invalid_message: &'static str,
) -> Result<Vec<String>> {
    let mut decoded = try_column_bundle_vec_with_capacity(values.len())?;
    for bytes in values {
        decoded.push(
            String::from_utf8(bytes)
                .map_err(|_| ArcadiaTioError::ocb_corrupt_file(invalid_message))?,
        );
    }
    Ok(decoded)
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

fn projected_columns_for_plan<'a>(
    columns: &'a [BundleColumn],
    plan: &ColumnBundleReadPlan,
) -> Result<Vec<&'a BundleColumn>> {
    let mut projected = try_column_bundle_vec_with_capacity(plan.projected_column_ids.len())?;
    for column_id in &plan.projected_column_ids {
        projected.push(
            columns
                .iter()
                .find(|column| column.id == *column_id)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB selected column not found",
                ))?,
        );
    }
    Ok(projected)
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
    for row_group in selected_row_groups_for_plan(metadata, plan)? {
        let row_count = usize::try_from(row_group.row_count).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB reusable buffer row count does not fit usize")
        })?;
        max_rows = max_rows.max(row_count);
    }
    Ok(max_rows)
}

fn validate_reusable_pool_allocation_resource_limits(
    metadata: &OcbMetadataV1,
    columns: &[&BundleColumn],
    row_capacity: usize,
    slot_count: usize,
    allow_nulls: bool,
) -> Result<()> {
    let row_capacity = u64::try_from(row_capacity).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input(
            "OCB reusable buffer row capacity does not fit resource accounting",
        )
    })?;
    let mut slot_bytes = 0u64;
    for column in columns {
        let value_width = match column.physical_type {
            ColumnPhysicalType::I32 | ColumnPhysicalType::F32 => 4u64,
            ColumnPhysicalType::I64 | ColumnPhysicalType::F64 => 8u64,
            ColumnPhysicalType::FixedBinary { width: 0 } => {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB reusable fixed-binary column has zero width",
                ));
            }
            ColumnPhysicalType::FixedBinary { width } => u64::from(width),
        };
        slot_bytes = slot_bytes
            .checked_add(row_capacity.checked_mul(value_width).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable buffer value allocation accounting overflows",
                ),
            )?)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable buffer slot accounting overflows",
            ))?;
        if allow_nulls {
            slot_bytes = slot_bytes.checked_add(row_capacity.div_ceil(8)).ok_or(
                ArcadiaTioError::ocb_invalid_input(
                    "OCB reusable validity allocation accounting overflows",
                ),
            )?;
        }
    }
    if slot_bytes > metadata.resource_limits.max_projected_row_group_bytes() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB reusable buffer slot exceeds projected row-group resource limit",
        ));
    }
    let slot_count = u64::try_from(slot_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input(
            "OCB reusable buffer slot count does not fit resource accounting",
        )
    })?;
    let total_bytes =
        slot_bytes
            .checked_mul(slot_count)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable buffer pool accounting overflows",
            ))?;
    if total_bytes
        > metadata
            .resource_limits
            .max_owned_decoded_materialized_bytes()
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB reusable buffer pool exceeds decoded materialization resource limit",
        ));
    }
    Ok(())
}

fn validate_fixed_binary_projection_resource_limits(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
    projection: &FixedBinaryRecordProjection,
    row_capacity: usize,
) -> Result<()> {
    let field_widths = projection.fields.iter().try_fold(0u64, |total, field| {
        total.checked_add(field.field_type.byte_width() as u64)
    });
    let field_widths = field_widths.ok_or(ArcadiaTioError::ocb_invalid_input(
        "OCB fixed-binary projection field-width accounting overflows",
    ))?;
    let buffer_bytes = u64::try_from(row_capacity)
        .ok()
        .and_then(|rows| rows.checked_mul(field_widths))
        .ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection buffer accounting overflows",
        ))?;
    if buffer_bytes
        > metadata
            .resource_limits
            .max_owned_decoded_materialized_bytes()
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fixed-binary projection buffer exceeds decoded materialization resource limit",
        ));
    }
    for footprint in selected_resource_footprints_for_plan(metadata, plan)? {
        let projected_bytes = footprint
            .row_count
            .checked_mul(field_widths)
            .and_then(|bytes| bytes.checked_add(footprint.decoded_materialized_bytes))
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection row-group accounting overflows",
            ))?;
        if projected_bytes > metadata.resource_limits.max_projected_row_group_bytes() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary projection exceeds projected row-group resource limit",
            ));
        }
    }
    Ok(())
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
    let mut planned = try_column_bundle_hash_set_with_capacity(plan.row_group_ids.len())?;
    planned.extend(plan.row_group_ids.iter().copied());
    let mut requested = try_column_bundle_hash_set_with_capacity(row_group_ids.len())?;
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
    let mut selected = try_column_bundle_vec_with_capacity(plan.row_group_ids.len())?;
    selected.extend(
        plan.row_group_ids
            .iter()
            .copied()
            .filter(|row_group_id| requested.contains(row_group_id)),
    );
    Ok(selected)
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
