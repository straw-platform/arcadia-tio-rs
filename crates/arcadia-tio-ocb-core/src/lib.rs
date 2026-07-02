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
mod read;

pub use crate::certification::{
    CertificationOptions, CertificationReport, ChannelCertificationReport,
    SafeCertificationSummary, certify_channel_sharded_artifact_v1,
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
    CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1, COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
    COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1, COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1,
    COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1, COMPACT_L2_FIXED_INGRESS_MAGIC,
    COMPACT_L2_PAYLOAD_COLUMN_NAME, COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE,
    CompactL2FixedBinaryHeaderV1, CompactL2RecordKind, OCB_CORE_READER_API_VERSION,
    decode_compact_l2_fixed_binary_header_v1,
};
pub use crate::error::{ArcadiaTioError, ArcadiaTioErrorCode, OcbFailureCause, Result};
pub use crate::manifest::{
    ChannelArtifactEntryV1, ChannelArtifactFingerprintV1, ChannelShardedManifestClaimsV1,
    ChannelShardedManifestCountsV1, ChannelShardedManifestV1, SafeManifestSummary,
    resolve_manifest_relative_artifact_path, validate_manifest_relative_path,
};
