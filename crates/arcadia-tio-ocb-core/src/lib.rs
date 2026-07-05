//! Source-visible Rust-core Ordered Column Bundle (OCB) reader APIs.
//!
//! This crate exposes the generic OCB selected-snapshot reader, read planner,
//! projected/predicate batch reads, explicit row-group visitors, reusable-buffer
//! lower-copy visitors, generic fixed-binary record field projection helpers,
//! channel-sharded compact-L2 manifest/certification helpers, and diagnostic
//! attribution without depending on the native C ABI wrapper path. It does not
//! expose OCB writer APIs, C/Python bindings, `TensorFile`, order-book replay,
//! owner assignment, factor/KOB logic, shm-ring transport, production LIVE
//! orchestration, or release/performance claims.

mod certification;
mod column_bundle;
mod compact_l2;
mod error;
mod format;
mod manifest;
mod parallel_read;
mod read;

pub use crate::certification::{
    CertificationOptions, CertificationReport, ChannelCertificationReport,
    CompactL2PhysicalV2CertificationOptions, CompactL2PhysicalV2CertificationReport,
    CompactL2PhysicalV2ChannelCertificationReport, CompactL2PhysicalV2ManifestCertificationOptions,
    CompactL2PhysicalV2ManifestCertificationReport, SafeCertificationSummary,
    certify_channel_sharded_artifact_v1, certify_compact_l2_physical_v2_artifact,
    certify_compact_l2_physical_v2_manifest,
};
pub use crate::column_bundle::{
    BundleColumn, BundleDictionaryDescriptor, BundleDictionaryValues, BundleNullOrder,
    BundleOrderingDirection, BundleOrderingKey, ColumnArray, ColumnBatch, ColumnBundleBodyKind,
    ColumnBundleBodyRefSummary, ColumnBundleChecksumKind, ColumnBundleColumnChunkSummary,
    ColumnBundleColumnChunkSummaryCodec, ColumnBundleColumnFillBuffer,
    ColumnBundleColumnFillReport, ColumnBundleColumnStatsSummary, ColumnBundleFile,
    ColumnBundleFixedBinaryProjectedFieldBuffer, ColumnBundleFixedBinaryProjectionBuffer,
    ColumnBundleMetadata, ColumnBundleOpenOptions, ColumnBundleOpenValidation,
    ColumnBundleOrderingKeyRange, ColumnBundleReadAttributedCursorReport,
    ColumnBundleReadAttributedOutcome, ColumnBundleReadAttribution, ColumnBundleReadCursorOptions,
    ColumnBundleReadCursorReport, ColumnBundleReadFillOptions, ColumnBundleReadFillReport,
    ColumnBundleReadOptions, ColumnBundleReadOutcome, ColumnBundleReadPlan,
    ColumnBundleReadPlanCertification, ColumnBundleReadReport, ColumnBundleReadRequest,
    ColumnBundleReusableBatchView, ColumnBundleReusableBufferPool, ColumnBundleReusableBuffers,
    ColumnBundleReusableColumnBuffer, ColumnBundleReusableColumnView, ColumnBundleRowGroupSummary,
    ColumnBundleSnapshotFingerprint, ColumnBundleStrictReadPlanningOptions,
    ColumnBundleVisitControl, ColumnLogicalKind, ColumnPhysicalType, ColumnPredicateValue,
    ColumnProjection, DictionaryValueKind, DictionaryValues, FixedBinaryFieldProjectionMut,
    FixedBinaryFieldType, FixedBinaryFieldValuesMut, FixedBinaryFieldValuesRef,
    FixedBinaryProjectedBatchView, FixedBinaryProjectedField, FixedBinaryProjectedFieldView,
    FixedBinaryProjectionReport, FixedBinaryRecordProjection, FixedBinaryRecordView,
    OCB_CERTIFICATION_FINGERPRINT_ALGORITHM, OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR,
    OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR, OcbErrorKind, PrimitiveColumnValues,
    PrimitiveColumnValuesMut, PrimitiveColumnValuesRef, ReusableFixedBinaryFieldValues,
    ReusablePrimitiveColumnValues, RowGroupPredicate, ValidityBitmap, ValidityBitmapRef,
};
pub use crate::compact_l2::{
    CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1, COMPACT_L2_BIZ_INDEX_COLUMN_NAME,
    COMPACT_L2_CHANNEL_ID_COLUMN_NAME, COMPACT_L2_DAY_KEY_COLUMN_NAME,
    COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1, COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
    COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1, COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1,
    COMPACT_L2_FIXED_INGRESS_MAGIC, COMPACT_L2_PAYLOAD_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT, COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_88_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_96_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_104_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_112_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_120_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_128_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_136_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_144_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_152_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_160_COLUMN_NAME, COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS,
    COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS, COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
    COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME, COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
    COMPACT_L2_RECORD_KIND_COLUMN_NAME, COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE,
    COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME, CompactL2FixedBinaryHeaderV1,
    CompactL2PhysicalV2BatchView, CompactL2PhysicalV2BodyWordColumn, CompactL2PhysicalV2Record,
    CompactL2RecordKind, OCB_CORE_READER_API_VERSION, compact_l2_physical_v2_body_word_column_name,
    decode_compact_l2_fixed_binary_header_v1,
};
pub use crate::error::{ArcadiaTioError, ArcadiaTioErrorCode, OcbFailureCause, Result};
pub use crate::manifest::{
    ChannelArtifactEntryV1, ChannelArtifactFingerprintV1, ChannelShardedManifestClaimsV1,
    ChannelShardedManifestCountsV1, ChannelShardedManifestV1, SafeManifestSummary,
    resolve_manifest_relative_artifact_path, validate_manifest_relative_path,
};
pub use crate::parallel_read::{
    COMPACT_L2_PHYSICAL_V2_DEFAULT_CHANNEL_WORKERS, CompactL2PhysicalV2ChannelReadInput,
    CompactL2PhysicalV2ChannelReadReport, CompactL2PhysicalV2ParallelReadOptions,
    CompactL2PhysicalV2ParallelReadReport, CompactL2PhysicalV2ReadBatch,
    compact_l2_physical_v2_inputs_from_manifest, read_compact_l2_physical_v2_channels,
};
