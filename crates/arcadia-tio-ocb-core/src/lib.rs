//! Source-visible Rust-core Ordered Column Bundle (OCB) reader APIs.
//!
//! This crate exposes the generic OCB selected-snapshot reader, read planner,
//! projected/predicate batch reads, explicit row-group visitors, reusable-buffer
//! lower-copy visitors, generic fixed-binary record field projection helpers,
//! and diagnostic attribution without depending on the native C ABI wrapper path. It does not
//! expose OCB writer APIs, C/Python bindings, `TensorFile`, market-data/L2
//! semantics, native compact-L2 decode, or release/performance claims.

mod column_bundle;
mod error;
mod format;
mod read;

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
pub use crate::error::{ArcadiaTioError, ArcadiaTioErrorCode, OcbFailureCause, Result};
