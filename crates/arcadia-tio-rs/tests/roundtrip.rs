use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::{
    AppendWithUniverseOptions, AutoCompactionConfig, AxisIdentityInput, AxisKind, CompactionMode,
    CompactionOptions, CompressionConfig, CoordinateDType, CoordinateEncoding, CoordinateKind,
    CoordinateMonotonicity, CoordinateOrdering, CoordinateSpec, CoordinateStorage,
    CoordinateStorageKind, CoordinateUniqueness, CoordinateValidationStatus, CoordinateValues,
    CreateInferredOptions, CreateOptions, CreatePolicyOptions, CreateUniverseOptions, DType,
    DimSpec, EntrySelector, ErrorCode, ExplicitExtentAxisTarget, ExplicitUniverseAxisTarget,
    HistoricalQuerySourceKind, HistoricalReadWithOptions, HistoricalReadWithShapePolicyOptions,
    QueryTraceContext, ReadIndexItem, ReadIndexLoweringKind, ReadShapePolicy, ReadWithOptions,
    ReadWithShapePolicyOptions, ReformOptions, SlotUniverseBindings, SparseAppendOutcome,
    SparseAppendReason, SparseRule, SparseValuePredicate, StorageAccessKind, TensorData,
    TensorFile, UniverseBinding, V4CompactionAnalysisPolicy, V4PreciseAccountingField,
    V4PreciseAccountingOptions, V4ReportStatus, V4RetainedHistoryCompactionOptions,
};

#[test]
fn safe_wrapper_roundtrips_f64_with_metadata_and_coordinates() {
    let path = unique_path("safe-wrapper-f64.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F64, dims, 0);
    options.channels = vec!["bid".to_string(), "ask".to_string()];
    options.user_kv = vec![("source".to_string(), "safe-wrapper-test".to_string())];
    options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("channel_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![10, 20])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });

    {
        let mut file = TensorFile::create(&path, options).expect("create through safe wrapper");
        let range = file
            .append_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
            .expect("append through safe wrapper");
        assert_eq!((range.start, range.end), (0, 2));
        assert_eq!(file.dtype().expect("dtype"), DType::F64);
        assert_eq!(file.dim_lens().expect("dim lens"), vec![2, 2]);
    }

    let file = TensorFile::open(&path).expect("reopen through safe wrapper");
    let tensor = file.read_all().expect("read through safe wrapper");
    assert_eq!(tensor.dtype, DType::F64);
    assert_eq!(tensor.shape, vec![2, 2]);
    assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0, 4.0]));

    let meta = TensorFile::load_meta(&path).expect("load metadata");
    assert_eq!(meta.dtype, DType::F64);
    assert_eq!(meta.dims.len(), 2);
    assert_eq!(meta.dims[0].name.as_deref(), Some("time"));
    assert_eq!(meta.channels.len(), 2);
    assert_eq!(meta.user_kv[0].key, "source");

    let coordinates = file.coordinate_meta().expect("coordinate metadata");
    assert_eq!(coordinates.len(), 1);
    assert_eq!(coordinates[0].axis, 1);
    assert_eq!(coordinates[0].name.as_deref(), Some("channel_id"));
    assert_eq!(coordinates[0].dtype, CoordinateDType::I32);
    assert_eq!(coordinates[0].storage_kind, CoordinateStorageKind::Inline);
    assert_eq!(
        coordinates[0].validation_status,
        CoordinateValidationStatus::Validated
    );

    let coordinate_values = file
        .read_axis_coordinates(1)
        .expect("inline coordinate values");
    assert_eq!(coordinate_values.dtype, DType::I32);
    assert_eq!(coordinate_values.shape, vec![2]);
    assert_eq!(coordinate_values.data, TensorData::I32(vec![10, 20]));
    assert_eq!(file.coordinate_index_i32(1, 10).expect("exact i32 10"), 0);
    assert_eq!(file.coordinate_index_i32(1, 20).expect("exact i32 20"), 1);
    assert_eq!(
        file.coordinate_range_i32(1, 10, 20)
            .expect("inclusive i32 range"),
        0..2
    );
    assert_eq!(
        file.coordinate_range_i32(1, 11, 20)
            .expect("partial i32 range"),
        1..2
    );
    let exact_read = file
        .read_at_coordinate_i32(1, 20)
        .expect("read exact i32 coordinate");
    assert_eq!(exact_read.dtype, DType::F64);
    assert_eq!(exact_read.shape, vec![2, 1]);
    assert_eq!(exact_read.data, TensorData::F64(vec![2.0, 4.0]));
    let range_read = file
        .read_coordinate_range_i32(1, 10, 20)
        .expect("read i32 coordinate range");
    assert_eq!(range_read.dtype, DType::F64);
    assert_eq!(range_read.shape, vec![2, 2]);
    assert_eq!(range_read.data, TensorData::F64(vec![1.0, 2.0, 3.0, 4.0]));
    let missing_read = file
        .read_at_coordinate_i32(1, 30)
        .expect_err("missing i32 coordinate read should fail");
    assert_eq!(missing_read.code(), ErrorCode::InvalidArgument);
    assert!(
        missing_read
            .message()
            .contains("coordinate value not found")
    );
    let no_overlap_read = file
        .read_coordinate_range_i32(1, 30, 40)
        .expect_err("non-overlapping i32 coordinate read should fail");
    assert_eq!(no_overlap_read.code(), ErrorCode::InvalidArgument);
    assert!(
        no_overlap_read
            .message()
            .contains("coordinate range does not overlap coordinate values")
    );
    let missing = file
        .coordinate_index_i32(1, 30)
        .expect_err("missing i32 coordinate should fail");
    assert_eq!(missing.code(), ErrorCode::InvalidArgument);
    assert!(missing.message().contains("coordinate value not found"));
    let dtype_mismatch = file
        .coordinate_index_i64(1, 10)
        .expect_err("i64 lookup on i32 coordinates should fail");
    assert_eq!(dtype_mismatch.code(), ErrorCode::InvalidArgument);
    assert!(
        dtype_mismatch
            .message()
            .contains("coordinate dtype is i32; expected i64 lookup value")
    );

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_reads_empty_inline_coordinate_values() {
    let path = unique_path("safe-wrapper-empty-coordinate.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 2),
    ];
    let mut options = CreateOptions::random_access(DType::F32, dims, 0);
    options.coordinates.push(CoordinateSpec {
        axis: 0,
        name: Some("empty_time_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(Vec::new())),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Unknown,
            monotonicity: CoordinateMonotonicity::Unknown,
            uniqueness: CoordinateUniqueness::Unknown,
        },
        required: false,
    });

    let file = TensorFile::create(&path, options).expect("create empty coordinate file");
    let coordinates = file.coordinate_meta().expect("coordinate metadata");
    assert_eq!(coordinates.len(), 1);
    assert_eq!(coordinates[0].axis, 0);
    assert_eq!(coordinates[0].length, 0);

    let coordinate_values = file
        .read_axis_coordinates(0)
        .expect("empty inline coordinate values");
    assert_eq!(coordinate_values.dtype, DType::I32);
    assert_eq!(coordinate_values.shape, vec![0]);
    assert_eq!(coordinate_values.data, TensorData::I32(Vec::new()));

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_looks_up_i64_coordinate_ranges() {
    let path = unique_path("safe-wrapper-i64-coordinate-lookup.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 3).with_name("symbol"),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("symbol_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I64(vec![1000, 2000, 3000])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });

    let mut file = TensorFile::create(&path, options).expect("create i64 coordinate file");
    file.append_f32(&[10.0, 20.0, 30.0, 40.0, 50.0, 60.0], &[2, 3])
        .expect("append i64 coordinate payload");
    assert_eq!(file.coordinate_index_i64(1, 1000).expect("exact i64"), 0);
    assert_eq!(file.coordinate_index_i64(1, 3000).expect("exact i64"), 2);
    assert_eq!(
        file.coordinate_range_i64(1, 1500, 3000)
            .expect("overlapping i64 range"),
        1..3
    );
    let exact_read = file
        .read_at_coordinate_i64(1, 3000)
        .expect("read exact i64 coordinate");
    assert_eq!(exact_read.dtype, DType::F32);
    assert_eq!(exact_read.shape, vec![2, 1]);
    assert_eq!(exact_read.data, TensorData::F32(vec![30.0, 60.0]));
    let range_read = file
        .read_coordinate_range_i64(1, 1500, 3000)
        .expect("read i64 coordinate range");
    assert_eq!(range_read.dtype, DType::F32);
    assert_eq!(range_read.shape, vec![2, 2]);
    assert_eq!(
        range_read.data,
        TensorData::F32(vec![20.0, 30.0, 50.0, 60.0])
    );
    let missing_read = file
        .read_at_coordinate_i64(1, 4000)
        .expect_err("missing i64 coordinate read should fail");
    assert_eq!(missing_read.code(), ErrorCode::InvalidArgument);
    assert!(
        missing_read
            .message()
            .contains("coordinate value not found")
    );
    let no_overlap_read = file
        .read_coordinate_range_i64(1, 4000, 5000)
        .expect_err("non-overlapping i64 coordinate read should fail");
    assert_eq!(no_overlap_read.code(), ErrorCode::InvalidArgument);
    assert!(
        no_overlap_read
            .message()
            .contains("coordinate range does not overlap coordinate values")
    );

    let no_overlap = file
        .coordinate_range_i64(1, 4000, 5000)
        .expect_err("non-overlapping i64 range should fail");
    assert_eq!(no_overlap.code(), ErrorCode::InvalidArgument);
    assert!(
        no_overlap
            .message()
            .contains("coordinate range does not overlap coordinate values")
    );
    let invalid_interval = file
        .coordinate_range_i64(1, 3000, 1000)
        .expect_err("start>end i64 range should fail");
    assert_eq!(invalid_interval.code(), ErrorCode::InvalidArgument);
    assert!(
        invalid_interval
            .message()
            .contains("coordinate range start must be <= end")
    );
    let dtype_mismatch = file
        .coordinate_index_i32(1, 1000)
        .expect_err("i32 lookup on i64 coordinates should fail");
    assert_eq!(dtype_mismatch.code(), ErrorCode::InvalidArgument);
    assert!(
        dtype_mismatch
            .message()
            .contains("coordinate dtype is i64; expected i32 lookup value")
    );

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_analyzes_sparse_append_diagnostics_for_f32_and_f64() {
    let f32_path = unique_path("safe-wrapper-sparse-analyze-f32.tio");
    let f32_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let file = TensorFile::create(&f32_path, f32_options).expect("create f32 analyzer file");
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
        let analysis = file
            .analyze_sparse_append_f32(&[0.0, 0.0, 1.0, 2.0], &[2, 2], &rule)
            .expect("analyze f32 sparse append");
        assert_eq!(analysis.outcome, SparseAppendOutcome::DenseFallback);
        assert!(analysis.absent_subtensor_count > 0);
        assert!(analysis.absent_subtensor_count <= analysis.total_subtensor_count);
        assert!(
            analysis
                .reasons
                .contains(&SparseAppendReason::WholeAppendUnitHasNoSparseProducerPath)
        );
        assert!(
            analysis
                .reasons
                .contains(&SparseAppendReason::DenseFallbackPreservesExactValues)
        );
    }
    let _ = fs::remove_file(f32_path);

    let f64_path = unique_path("safe-wrapper-sparse-analyze-f64.tio");
    let f64_options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let file = TensorFile::create(&f64_path, f64_options).expect("create f64 analyzer file");
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualF64(-1.0));
        let analysis = file
            .analyze_sparse_append_f64(&[-1.0, -1.0, 1.0, 2.0], &[2, 2], &rule)
            .expect("analyze f64 sparse append");
        assert_eq!(analysis.outcome, SparseAppendOutcome::DenseFallback);
        assert!(analysis.absent_subtensor_count > 0);
        assert!(analysis.absent_subtensor_count <= analysis.total_subtensor_count);
        assert!(
            analysis
                .reasons
                .contains(&SparseAppendReason::WholeAppendUnitHasNoSparseProducerPath)
        );
    }
    let _ = fs::remove_file(f64_path);
}

#[test]
fn safe_wrapper_sparse_append_fallback_and_reject_behaviors() {
    let fallback_path = unique_path("safe-wrapper-sparse-append-fallback.tio");
    let fallback_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file =
            TensorFile::create(&fallback_path, fallback_options).expect("create fallback file");
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
        let range = file
            .append_sparse_f32_returning_range(&[0.0, 0.0, 1.0, 2.0], &[2, 2], &rule)
            .expect("append f32 sparse-intent fallback via readability alias");
        assert_eq!((range.start, range.end), (0, 2));
        let tensor = file.read_all().expect("read sparse fallback values");
        assert_eq!(tensor.shape, vec![2, 2]);
        assert_eq!(tensor.data, TensorData::F32(vec![0.0, 0.0, 1.0, 2.0]));
    }
    let _ = fs::remove_file(fallback_path);

    let no_range_path = unique_path("safe-wrapper-sparse-append-f64-no-range.tio");
    let no_range_options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file =
            TensorFile::create(&no_range_path, no_range_options).expect("create f64 no-range file");
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualF64(-1.0));
        file.append_sparse_f64(&[-1.0, -1.0, 5.0, 6.0], &[2, 2], &rule)
            .expect("append f64 sparse-intent without range");
        let tensor = file.read_all().expect("read sparse no-range values");
        assert_eq!(tensor.shape, vec![2, 2]);
        assert_eq!(tensor.data, TensorData::F64(vec![-1.0, -1.0, 5.0, 6.0]));
        let range = file
            .append_sparse_f64_returning_range(&[-1.0, -1.0, 7.0, 8.0], &[2, 2], &rule)
            .expect("append f64 sparse-intent with readability alias");
        assert_eq!((range.start, range.end), (2, 4));
    }
    let _ = fs::remove_file(no_range_path);

    let reject_path = unique_path("safe-wrapper-sparse-append-reject.tio");
    let reject_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Symbol, 4),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    let policy = CreatePolicyOptions::new(vec![1, 2], vec![0, 2, 2]);
    {
        let mut file = TensorFile::create_with_policy(&reject_path, reject_options, policy)
            .expect("create RegularChunked reject file");
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
        let values = [21.0, 22.0, 23.0, 24.0, 0.0, 0.0, 0.0, 0.0];
        let analysis = file
            .analyze_sparse_append_f32(&values, &[1, 4, 2], &rule)
            .expect("analyze sparse reject path");
        assert_eq!(analysis.outcome, SparseAppendOutcome::Reject);
        assert!(!analysis.reasons.is_empty());
        let err = file
            .append_sparse_f32_with_range(&values, &[1, 4, 2], &rule)
            .expect_err("native sparse reject should surface as wrapper error");
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }
    let _ = fs::remove_file(reject_path);
}

#[test]
fn safe_wrapper_sparse_append_i32_i64_roundtrip_and_rejects() {
    for (path, dtype) in [
        (unique_path("safe-wrapper-sparse-i32.tio"), DType::I32),
        (unique_path("safe-wrapper-sparse-i64.tio"), DType::I64),
    ] {
        let options = CreateOptions::random_access(
            dtype,
            vec![
                DimSpec::new(AxisKind::Time, 0),
                DimSpec::new(AxisKind::Symbol, 4),
            ],
            0,
        );
        let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
        match dtype {
            DType::I32 => {
                let values = [11, 0, 13, 0];
                let mut file = TensorFile::create(&path, options).expect("create i32 sparse file");
                let analysis = file
                    .analyze_sparse_append_i32(&values, &[1, 4], &rule)
                    .expect("analyze i32 sparse append");
                assert_eq!(analysis.outcome, SparseAppendOutcome::SparseChunkTree);
                assert_eq!(analysis.absent_subtensor_count, 2);
                let range = file
                    .append_sparse_i32(&values, &[1, 4], &rule)
                    .expect("append i32 sparse values");
                assert_eq!((range.start, range.end), (0, 1));
                let dense = file.read_all_dense(0.0).expect("read i32 sparse values");
                assert_eq!(dense.tensor.dtype, DType::I32);
                assert_eq!(dense.tensor.shape, vec![1, 4]);
                assert_eq!(dense.tensor.data, TensorData::I32(vec![11, 0, 13, 0]));
                assert_eq!(dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

                let exact_values = [21, -7, 23, -7];
                let exact_rule =
                    SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI32(-7));
                let exact_analysis = file
                    .analyze_sparse_append_i32(&exact_values, &[1, 4], &exact_rule)
                    .expect("analyze i32 exact sparse append");
                assert_eq!(exact_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
                assert_eq!(exact_analysis.absent_subtensor_count, 2);
                let exact_range = file
                    .append_sparse_i32(&exact_values, &[1, 4], &exact_rule)
                    .expect("append i32 exact sparse values");
                assert_eq!((exact_range.start, exact_range.end), (1, 2));

                let mismatch_rule =
                    SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI64(-7));
                let err = file
                    .analyze_sparse_append_i32(&exact_values, &[1, 4], &mismatch_rule)
                    .expect_err("i32/equal_i64 predicate should reject");
                assert_eq!(err.code(), ErrorCode::InvalidArgument);
                assert!(
                    err.message()
                        .contains("predicate does not match tensor dtype")
                );

                let null_rule = SparseRule::null_subtensor(vec![1]);
                let null_analysis = file
                    .analyze_sparse_append_i32(&[21, 22, 23, 24], &[1, 4], &null_rule)
                    .expect("analyze dense i32 null-subtensor rule");
                assert_eq!(null_analysis.absent_subtensor_count, 0);

                let unsupported =
                    SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Nan);
                let err = file
                    .analyze_sparse_append_i32(&values, &[1, 4], &unsupported)
                    .expect_err("integer NaN sparse predicate should reject");
                assert_eq!(err.code(), ErrorCode::InvalidArgument);
                assert!(
                    err.message()
                        .contains("predicate does not match tensor dtype")
                );
            }
            DType::I64 => {
                let values = [101_i64, 0, 103, 0];
                let mut file = TensorFile::create(&path, options).expect("create i64 sparse file");
                let analysis = file
                    .analyze_sparse_append_i64(&values, &[1, 4], &rule)
                    .expect("analyze i64 sparse append");
                assert_eq!(analysis.outcome, SparseAppendOutcome::SparseChunkTree);
                assert_eq!(analysis.absent_subtensor_count, 2);
                let range = file
                    .append_sparse_i64(&values, &[1, 4], &rule)
                    .expect("append i64 sparse values");
                assert_eq!((range.start, range.end), (0, 1));
                let dense = file.read_all_dense(0.0).expect("read i64 sparse values");
                assert_eq!(dense.tensor.dtype, DType::I64);
                assert_eq!(dense.tensor.shape, vec![1, 4]);
                assert_eq!(dense.tensor.data, TensorData::I64(vec![101, 0, 103, 0]));
                assert_eq!(dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

                let exact_absent = 9_007_199_254_740_993_i64;
                let exact_values = [201_i64, exact_absent, 203, exact_absent];
                let exact_rule = SparseRule::predicate_subtensor(
                    vec![1],
                    SparseValuePredicate::EqualI64(exact_absent),
                );
                let exact_analysis = file
                    .analyze_sparse_append_i64(&exact_values, &[1, 4], &exact_rule)
                    .expect("analyze i64 exact sparse append");
                assert_eq!(exact_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
                assert_eq!(exact_analysis.absent_subtensor_count, 2);
                let exact_range = file
                    .append_sparse_i64(&exact_values, &[1, 4], &exact_rule)
                    .expect("append i64 exact sparse values");
                assert_eq!((exact_range.start, exact_range.end), (1, 2));

                let mismatch_rule =
                    SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI32(0));
                let err = file
                    .analyze_sparse_append_i64(&exact_values, &[1, 4], &mismatch_rule)
                    .expect_err("i64/equal_i32 predicate should reject");
                assert_eq!(err.code(), ErrorCode::InvalidArgument);
                assert!(
                    err.message()
                        .contains("predicate does not match tensor dtype")
                );

                let unsupported =
                    SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualF64(0.0));
                let err = file
                    .append_sparse_i64(&values, &[1, 4], &unsupported)
                    .expect_err("integer exact-float sparse predicate should reject");
                assert_eq!(err.code(), ErrorCode::InvalidArgument);
                assert!(
                    err.message()
                        .contains("predicate does not match tensor dtype")
                );
            }
            _ => unreachable!("test matrix only includes integer dtypes"),
        }
        let _ = fs::remove_file(path);
    }
}

#[test]
fn safe_wrapper_read_index_matches_basic_native_semantics() {
    let path = unique_path("safe-wrapper-read-index.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create read_index file");
        file.append_f64(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2])
            .expect("append read_index values");
    }

    let file = TensorFile::open(&path).expect("open read_index file");
    let sliced = file
        .read_index(&[
            ReadIndexItem::slice(Some(0), Some(3), 2).expect("valid slice"),
            ReadIndexItem::all(),
        ])
        .expect("slice read_index");
    assert_eq!(
        sliced.report.lowering_kind,
        ReadIndexLoweringKind::SelectorRead
    );
    assert!(!sliced.report.used_full_tensor_fallback);
    assert_eq!(sliced.value.shape, vec![2, 2]);
    assert_eq!(sliced.value.data, TensorData::F64(vec![1.0, 2.0, 5.0, 6.0]));

    let postprocessed = file
        .read_index(&[
            ReadIndexItem::index(1),
            ReadIndexItem::new_axis(),
            ReadIndexItem::ellipsis(),
        ])
        .expect("index/newaxis read_index");
    assert_eq!(
        postprocessed.report.lowering_kind,
        ReadIndexLoweringKind::SelectorReadWithShapePostprocess
    );
    assert_eq!(postprocessed.value.shape, vec![1, 2]);
    assert_eq!(postprocessed.value.data, TensorData::F64(vec![3.0, 4.0]));

    let err = ReadIndexItem::slice(None, None, 0).expect_err("zero step rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    let err = file
        .read_index(&[ReadIndexItem::ellipsis(), ReadIndexItem::ellipsis()])
        .expect_err("duplicate ellipsis rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    let err = file
        .read_index(&[ReadIndexItem::index(0), ReadIndexItem::index(1)])
        .expect_err("scalar output rejects before FFI");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_exports_arrow_c_data_and_allows_later_reads() {
    let path = unique_path("safe-wrapper-arrow.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create Arrow export file");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2])
            .expect("append Arrow export values");
    }

    let file = TensorFile::open(&path).expect("open Arrow export file");
    {
        let arrow = file.read_values_arrow().expect("export Arrow C Data");
        assert_eq!(arrow.array().length, 3);
        assert_eq!(arrow.array().n_children, 1);
        assert!(arrow.array().release.is_some());
        assert!(arrow.schema().release.is_some());
        assert!(!arrow.schema().format.is_null());
        assert!(!arrow.array_ptr().is_null());
        assert!(!arrow.schema_ptr().is_null());
    }

    let tensor = file
        .read_all()
        .expect("ordinary read after Arrow export drop");
    assert_eq!(tensor.shape, vec![3, 2]);
    assert_eq!(
        tensor.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    );

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_compression_option_roundtrips_f32() {
    let path = unique_path("safe-wrapper-compressed-f32.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 32),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.compression = Some(CompressionConfig::zstd_level(3));
    let values = vec![0.0f32; 4 * 32];
    {
        let mut file = TensorFile::create(&path, options).expect("create compressed wrapper file");
        let range = file
            .append_f32(&values, &[4, 32])
            .expect("append compressed wrapper values");
        assert_eq!((range.start, range.end), (0, 4));
    }
    let file = TensorFile::open(&path).expect("open compressed wrapper file");
    let tensor = file.read_all().expect("read compressed wrapper values");
    assert_eq!(tensor.dtype, DType::F32);
    assert_eq!(tensor.shape, vec![4, 32]);
    assert_eq!(tensor.data, TensorData::F32(values));
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_rewrites_f32_and_f64_data() {
    let path = unique_path("safe-wrapper-rewrite-f32.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create f32 rewrite file");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0], &[3, 2])
            .expect("append f32 rewrite base");
        file.rewrite_f32(EntrySelector::Take(vec![1]), &[30.0, 31.0], &[1, 2])
            .expect("rewrite one f32 append entry");
        file.rewrite_slice_f32(
            &[EntrySelector::Take(vec![0, 2]), EntrySelector::All],
            &[10.0, 11.0, 50.0, 51.0],
            &[2, 2],
        )
        .expect("rewrite f32 selector slice");
    }
    let file = TensorFile::open(&path).expect("reopen f32 rewrite file");
    let tensor = file.read_all().expect("read f32 rewritten tensor");
    assert_eq!(tensor.dtype, DType::F32);
    assert_eq!(tensor.shape, vec![3, 2]);
    assert_eq!(
        tensor.data,
        TensorData::F32(vec![10.0, 11.0, 30.0, 31.0, 50.0, 51.0])
    );
    drop(file);
    let _ = fs::remove_file(path);

    let path = unique_path("safe-wrapper-rewrite-f64.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create f64 rewrite file");
        file.append_f64(&[1.5, 2.5, 3.5, 4.5, 5.5, 6.5], &[3, 2])
            .expect("append f64 rewrite base");
        file.rewrite_f64(EntrySelector::Take(vec![0]), &[7.5, 8.5], &[1, 2])
            .expect("rewrite one f64 append entry");
        file.rewrite_slice_f64(
            &[
                EntrySelector::Range { start: 1, end: 3 },
                EntrySelector::All,
            ],
            &[30.5, 31.5, 60.5, 61.5],
            &[2, 2],
        )
        .expect("rewrite f64 selector slice");
    }
    let file = TensorFile::open(&path).expect("reopen f64 rewrite file");
    let tensor = file.read_all().expect("read f64 rewritten tensor");
    assert_eq!(tensor.dtype, DType::F64);
    assert_eq!(tensor.shape, vec![3, 2]);
    assert_eq!(
        tensor.data,
        TensorData::F64(vec![7.5, 8.5, 30.5, 31.5, 60.5, 61.5])
    );
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_reform_workflows_roundtrip_and_report_errors() {
    let source_path = unique_path("safe-wrapper-reform-source.tio");
    let regular_path = unique_path("safe-wrapper-reform-regular.tio");
    let wau_path = unique_path("safe-wrapper-reform-wau.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&source_path, options).expect("create reform source");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
            .expect("append reform source");
        file.reform_to(&regular_path, ReformOptions::regular_chunked(vec![1, 2]))
            .expect("reform to RegularChunked");
    }

    {
        let mut regular = TensorFile::open(&regular_path).expect("open regular reform output");
        regular
            .reform_to(&wau_path, ReformOptions::whole_append_unit())
            .expect("reform to WholeAppendUnit with empty block shape");
        let err = regular
            .reform_to_ex(&regular_path, ReformOptions::regular_chunked(vec![0, 2]))
            .expect_err("invalid reform report should be surfaced");
        assert!(err.message().contains("v4.reform."));
        assert!(err.message().contains("v4.reform.v1"));
    }

    for path in [&regular_path, &wau_path] {
        let file = TensorFile::open(path).expect("open reform output");
        let tensor = file.read_all().expect("read reform output");
        assert_eq!(tensor.dtype, DType::F32);
        assert_eq!(tensor.shape, vec![2, 2]);
        assert_eq!(tensor.data, TensorData::F32(vec![1.0, 2.0, 3.0, 4.0]));
    }

    let _ = fs::remove_file(source_path);
    let _ = fs::remove_file(regular_path);
    let _ = fs::remove_file(wau_path);
}

#[test]
fn safe_wrapper_compaction_and_retained_history_workflows() {
    let path = unique_path("safe-wrapper-compaction-source.tio");
    let compact_path = unique_path("safe-wrapper-compact-dst.tio");
    let maybe_path = unique_path("safe-wrapper-maybe-compact-dst.tio");
    let retained_path = unique_path("safe-wrapper-retained-compact-dst.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create compaction source");
        file.append_f64(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
            .expect("append compaction source");
        let stats = file.analyze_compaction().expect("analyze compaction");
        assert_eq!(stats.dead_bytes, 0);
        assert_eq!(stats.commit_count, 1);
        let analysis = file.analyze_v4_compaction().expect("analyze V4 compaction");
        assert_eq!(analysis.status, V4ReportStatus::Complete);
        assert!(analysis.source_file_bytes > 0);
        file.compact_to(&compact_path, CompactionOptions::default())
            .expect("compact current state");
        let compacted = file
            .maybe_compact(
                &maybe_path,
                CompactionOptions {
                    dead_ratio_threshold: 2.0,
                    ..CompactionOptions::default()
                },
            )
            .expect("maybe compact no-op");
        assert!(!compacted);
        let retained = file
            .compact_v4_retained_history_to(
                &retained_path,
                V4RetainedHistoryCompactionOptions::retain_last(1),
            )
            .expect("retained-history compaction");
        assert_eq!(retained.status, V4ReportStatus::Complete);
        assert!(retained.destination_file_bytes > 0);
    }

    for path in [&compact_path, &retained_path] {
        let file = TensorFile::open(path).expect("open compacted output");
        let tensor = file.read_all().expect("read compacted output");
        assert_eq!(tensor.dtype, DType::F64);
        assert_eq!(tensor.shape, vec![2, 2]);
        assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0, 4.0]));
    }

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(compact_path);
    let _ = fs::remove_file(maybe_path);
    let _ = fs::remove_file(retained_path);
}

#[test]
fn safe_wrapper_diagnostics_reports_small_file() {
    let path = unique_path("safe-wrapper-diagnostics.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    let mut file = TensorFile::create(&path, options).expect("create diagnostics source");
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
        .expect("append diagnostics source");

    let diagnostics = file.v4_diagnostics().expect("V4 diagnostics report");
    assert_eq!(diagnostics.status, V4ReportStatus::Complete);
    assert!(diagnostics.current_head.payload_bytes > 0);
    assert!(diagnostics.omitted_unreachable_bytes);
    assert!(diagnostics.omitted_unreachable_bytes_reason.is_some());

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_precise_accounting_reports_and_omissions() {
    let path = unique_path("safe-wrapper-precise-accounting.tio");
    let retained_path = unique_path("safe-wrapper-precise-retained.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    let mut file = TensorFile::create(&path, options).expect("create precise source");
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])
        .expect("append precise source");

    let diagnostics = file
        .v4_diagnostics_precise(V4PreciseAccountingOptions::default())
        .expect("precise diagnostics report");
    assert_eq!(diagnostics.status, V4ReportStatus::Complete);
    assert_eq!(
        diagnostics.reason_code.as_deref(),
        Some("v4.precise.complete")
    );
    assert!(diagnostics.precise_accounting.unreachable_bytes.is_some());
    assert!(
        diagnostics
            .precise_accounting
            .retained_history_required_bytes
            .is_some()
    );
    assert_eq!(diagnostics.precise_accounting.popped_skipped_bytes, Some(0));
    assert!(diagnostics.precise_accounting.reclaimable_bytes.is_some());
    assert!(diagnostics.precise_accounting.omitted_fields.is_empty());

    let analysis = file
        .analyze_v4_compaction_precise(V4PreciseAccountingOptions::fields([
            V4PreciseAccountingField::UnreachableBytes,
            V4PreciseAccountingField::ReclaimableBytes,
        ]))
        .expect("precise compaction analysis report");
    assert_eq!(analysis.status, V4ReportStatus::Complete);
    assert_eq!(
        analysis.policy,
        V4CompactionAnalysisPolicy::CompactToCurrentState
    );
    assert_eq!(analysis.reason_code.as_deref(), Some("v4.precise.complete"));
    assert!(analysis.precise_accounting.unreachable_bytes.is_some());
    assert!(analysis.precise_accounting.reclaimable_bytes.is_some());

    let retained = file
        .compact_v4_retained_history_to_precise(
            &retained_path,
            V4RetainedHistoryCompactionOptions::retain_last(1),
            V4PreciseAccountingOptions::default(),
        )
        .expect("precise retained-history report");
    assert_eq!(retained.status, V4ReportStatus::Complete);
    assert_eq!(retained.reason_code.as_deref(), Some("v4.precise.complete"));
    assert!(retained.source_file_bytes > 0);
    assert!(retained.destination_file_bytes > 0);
    assert!(
        retained
            .precise_source_accounting
            .retained_history_required_bytes
            .is_some()
    );

    drop(file);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open precise source for unknown tail")
        .write_all(&[0xde, 0xad, 0xbe, 0xef, 0, 1, 2, 3])
        .expect("append unknown tail");
    let file = TensorFile::open(&path).expect("reopen source with unknown tail");
    let unknown = file
        .v4_diagnostics_precise(V4PreciseAccountingOptions::default())
        .expect("unknown precise diagnostics report");
    assert_eq!(unknown.status, V4ReportStatus::Unknown);
    assert_eq!(
        unknown.reason_code.as_deref(),
        Some("v4.precise.unknown.directory.unclassified_ranges")
    );
    assert_eq!(unknown.precise_accounting.unreachable_bytes, None);
    assert_eq!(unknown.precise_accounting.omitted_fields.len(), 4);
    assert_eq!(
        unknown.precise_accounting.omitted_fields[0].field,
        V4PreciseAccountingField::UnreachableBytes
    );
    assert!(
        unknown.precise_accounting.omitted_fields[0]
            .reason
            .is_some()
    );
    assert_eq!(
        unknown.precise_accounting.omitted_fields[0]
            .reason_code
            .as_deref(),
        Some("v4.precise.omitted.unreachable_bytes")
    );

    drop(file);
    let _ = fs::remove_file(path);
    let _ = fs::remove_file(retained_path);
}

#[test]
fn safe_wrapper_auto_compaction_helpers_surface_native_state() {
    let path = unique_path("safe-wrapper-auto-compaction.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    let mut file = TensorFile::create(&path, options).expect("create auto compaction source");
    file.append_f32(&[1.0, 2.0], &[1, 2])
        .expect("append auto compaction source");
    assert!(
        file.get_auto_compaction_config()
            .expect("read auto config")
            .is_none()
    );
    assert!(
        file.compaction_state()
            .expect("read compaction state")
            .is_none()
    );
    let err = file
        .set_auto_compaction_config(Some(AutoCompactionConfig {
            mode: CompactionMode::CopyLive,
            ..AutoCompactionConfig::default()
        }))
        .expect_err("V4 auto-compaction config set is unsupported");
    assert_eq!(err.code(), ErrorCode::Unimplemented);
    let err = file
        .maybe_compact_auto()
        .expect_err("V4 maybe_compact_auto is unsupported");
    assert_eq!(err.code(), ErrorCode::Unimplemented);
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_mutation_validation_and_clear_blocks_errors() {
    let path = unique_path("safe-wrapper-mutation-negative.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Channel, 2),
        ],
        0,
    );
    let mut file = TensorFile::create(&path, options).expect("create negative mutation file");
    file.append_f32(&[1.0, 2.0], &[1, 2])
        .expect("append negative mutation base");

    let err = file
        .rewrite_f32(EntrySelector::Take(vec![0]), &[1.0], &[1, 2])
        .expect_err("rewrite length mismatch should be rejected before FFI");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(err.message().contains("rewrite data length"));

    let err = file
        .rewrite_slice_f32(&[EntrySelector::All], &[1.0, 2.0], &[1, 2])
        .expect_err("rewrite selector/rank mismatch should be rejected before FFI");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(err.message().contains("selector count"));

    let err = file
        .rewrite_f64(EntrySelector::Take(vec![0]), &[1.0, 2.0], &[1, 2])
        .expect_err("rewrite dtype mismatch should be rejected before FFI");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(err.message().contains("rewrite dtype"));

    let err = file
        .clear_blocks(&[])
        .expect_err("clear_blocks unsupported native path should surface");
    assert_eq!(err.code(), ErrorCode::Unimplemented);

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_roundtrips_all_first_slice_numeric_dtypes() {
    roundtrip_dtype(
        "f32",
        DType::F32,
        |file| file.append_f32(&[1.5, 2.5, 3.5], &[3]),
        TensorData::F32(vec![1.5, 2.5, 3.5]),
    );
    roundtrip_dtype(
        "f64",
        DType::F64,
        |file| file.append_f64(&[1.25, 2.25, 3.25], &[3]),
        TensorData::F64(vec![1.25, 2.25, 3.25]),
    );
    roundtrip_dtype(
        "i32",
        DType::I32,
        |file| file.append_i32(&[1, 2, 3], &[3]),
        TensorData::I32(vec![1, 2, 3]),
    );
    roundtrip_dtype(
        "i64",
        DType::I64,
        |file| file.append_i64(&[10, 20, 30], &[3]),
        TensorData::I64(vec![10, 20, 30]),
    );
}

#[test]
fn safe_wrapper_read_options_policy_and_inferred_create_roundtrip() {
    let path = unique_path("safe-wrapper-policy-create.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.symbols = vec!["AAPL".to_string(), "MSFT".to_string()];
    options.channels = vec!["open".to_string(), "close".to_string()];
    options.coordinates.push(CoordinateSpec {
        axis: 2,
        name: Some("channel_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![10, 20])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });
    let policy = CreatePolicyOptions::new(vec![1, 2], vec![0, 2, 2]);
    {
        let mut file = TensorFile::create_with_policy(&path, options, policy)
            .expect("create RegularChunked policy wrapper file");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 2, 2])
            .expect("append policy-created values");
    }

    let file = TensorFile::open(&path).expect("open policy-created wrapper file");
    let full = file
        .read_with_options(&[], ReadWithOptions::parallel_threads(2))
        .expect("read with execution options");
    assert_eq!(full.value.shape, vec![2, 2, 2]);
    assert_eq!(
        full.value.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0])
    );
    assert_eq!(full.execution.query_max_threads, 2);
    let policy_coordinates = file.coordinate_meta().expect("policy coordinate metadata");
    assert_eq!(policy_coordinates.len(), 1);
    assert_eq!(policy_coordinates[0].axis, 2);
    assert_eq!(policy_coordinates[0].name.as_deref(), Some("channel_id"));
    assert_eq!(policy_coordinates[0].dtype, CoordinateDType::I32);
    assert_eq!(
        policy_coordinates[0].storage_kind,
        CoordinateStorageKind::Inline
    );
    let policy_coordinate_values = file
        .read_axis_coordinates(2)
        .expect("policy coordinate values");
    assert_eq!(policy_coordinate_values.shape, vec![2]);
    assert_eq!(policy_coordinate_values.data, TensorData::I32(vec![10, 20]));

    let dense = file
        .read_with_options_dense(
            &[
                EntrySelector::Range { start: 1, end: 2 },
                EntrySelector::All,
                EntrySelector::All,
            ],
            ReadWithOptions::serial(),
            -1.0,
        )
        .expect("dense read with execution options");
    assert_eq!(dense.value.tensor.shape, vec![1, 2, 2]);
    assert_eq!(
        dense.value.tensor.data,
        TensorData::F32(vec![5.0, 6.0, 7.0, 8.0])
    );

    let historical = file
        .read_at_commit_with_options(1, &[], HistoricalReadWithOptions::serial())
        .expect("historical read with execution options");
    assert_eq!(historical.value.shape, vec![2, 2, 2]);
    assert_eq!(historical.execution.query_commit_seq, 1);
    drop(file);
    let _ = fs::remove_file(path);

    let inferred_path = unique_path("safe-wrapper-inferred-create.tio");
    let mut inferred_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Symbol, 2),
        ],
        0,
    );
    inferred_options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("symbol_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I64(vec![1000, 2000])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });
    let mut hints = CreateInferredOptions::new();
    hints.storage_access = StorageAccessKind::RemoteRangeRead;
    {
        let mut file = TensorFile::create_inferred(&inferred_path, inferred_options, hints)
            .expect("create inferred wrapper file");
        file.append_f32(&[9.0, 10.0], &[1, 2])
            .expect("append inferred values");
    }
    let file = TensorFile::open(&inferred_path).expect("open inferred wrapper file");
    let tensor = file.read_all().expect("read inferred wrapper file");
    assert_eq!(tensor.shape, vec![1, 2]);
    assert_eq!(tensor.data, TensorData::F32(vec![9.0, 10.0]));
    let inferred_coordinates = file
        .coordinate_meta()
        .expect("inferred coordinate metadata");
    assert_eq!(inferred_coordinates.len(), 1);
    assert_eq!(inferred_coordinates[0].axis, 1);
    assert_eq!(inferred_coordinates[0].name.as_deref(), Some("symbol_id"));
    assert_eq!(inferred_coordinates[0].dtype, CoordinateDType::I64);
    assert_eq!(
        inferred_coordinates[0].storage_kind,
        CoordinateStorageKind::Inline
    );
    let inferred_coordinate_values = file
        .read_axis_coordinates(1)
        .expect("inferred coordinate values");
    assert_eq!(inferred_coordinate_values.shape, vec![2]);
    assert_eq!(
        inferred_coordinate_values.data,
        TensorData::I64(vec![1000, 2000])
    );
    drop(file);
    let _ = fs::remove_file(inferred_path);
}

#[test]
fn safe_wrapper_attributed_read_options_return_trace_json() {
    let path = unique_path("safe-wrapper-attributed-read-options.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.symbols = vec!["AAPL".to_string(), "MSFT".to_string()];
    options.channels = vec!["open".to_string(), "close".to_string()];
    {
        let mut file = TensorFile::create(&path, options).expect("create attributed source file");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 2, 2])
            .expect("append attributed source values");
    }

    let file = TensorFile::open(&path).expect("open attributed source file");
    let selectors = [
        EntrySelector::All,
        EntrySelector::Range { start: 0, end: 1 },
        EntrySelector::All,
    ];
    let ordinary = file
        .read_with_options(&selectors, ReadWithOptions::serial())
        .expect("ordinary read with options remains available");
    let context = QueryTraceContext::new(
        "tp353-run",
        "tp353-row",
        "safe-wrapper-test",
        "rust",
        "arcadia-tio-rs",
        "read_with_options_attributed",
        "monotonic",
    )
    .with_repeat_index(7);
    let attributed = file
        .read_with_options_attributed(&selectors, ReadWithOptions::serial(), &context)
        .expect("attributed read with options");
    assert_eq!(attributed.value, ordinary.value);
    assert_eq!(attributed.execution, ordinary.execution);
    assert_query_trace_json(attributed.trace.as_str(), "read_with_options_attributed");
    assert!(
        attributed
            .trace
            .as_str()
            .contains("\"run_id\":\"tp353-run\"")
    );
    assert!(
        attributed
            .trace
            .as_str()
            .contains("\"row_id\":\"tp353-row\"")
    );
    assert!(attributed.trace.as_str().contains("\"repeat_index\":7"));
    assert!(attributed.trace.as_str().contains("\"language\":\"rust\""));
    assert!(
        attributed
            .trace
            .as_str()
            .contains("\"api_surface\":\"arcadia-tio-rs\"")
    );

    let dense_context = QueryTraceContext::new(
        "tp353-run",
        "tp353-dense-row",
        "safe-wrapper-test",
        "rust",
        "arcadia-tio-rs",
        "read_with_options_dense_attributed",
        "monotonic",
    )
    .with_repeat_index(8);
    let dense = file
        .read_with_options_dense_attributed(
            &selectors,
            ReadWithOptions::parallel_threads(2),
            &dense_context,
            -1.0,
        )
        .expect("dense attributed read with options");
    assert_eq!(dense.value.tensor.shape, vec![1, 1, 2]);
    assert_eq!(dense.value.tensor.data, TensorData::F32(vec![1.0, 2.0]));
    assert_query_trace_json(dense.trace.as_str(), "read_with_options_dense_attributed");
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_attributed_read_rejects_invalid_context_inputs() {
    let path = unique_path("safe-wrapper-attributed-invalid-context.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![DimSpec::new(AxisKind::Time, 0).with_name("time")],
        0,
    );
    {
        let mut file = TensorFile::create(&path, options).expect("create invalid-context file");
        file.append_f32(&[1.0], &[1])
            .expect("append invalid-context value");
    }

    let file = TensorFile::open(&path).expect("open invalid-context file");
    let empty_run = QueryTraceContext::new(
        "",
        "row",
        "phase",
        "rust",
        "arcadia-tio-rs",
        "read_with_options_attributed",
        "monotonic",
    );
    let err = file
        .read_with_options_attributed(&[], ReadWithOptions::serial(), &empty_run)
        .expect_err("empty trace run id is rejected before native call");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(err.message().contains("run_id must not be empty"));

    let nul_phase = QueryTraceContext::new(
        "run",
        "row",
        "bad\0phase",
        "rust",
        "arcadia-tio-rs",
        "read_with_options_attributed",
        "monotonic",
    );
    let err = file
        .read_with_options_dense_attributed(&[], ReadWithOptions::serial(), &nul_phase, -1.0)
        .expect_err("interior-NUL trace phase is rejected before native call");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(
        err.message()
            .contains("phase contains an interior NUL byte")
    );
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_policy_universe_create_roundtrip() {
    let path = unique_path("safe-wrapper-policy-universe-create.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let options = CreateOptions::streaming(DType::F32, dims, 0);
    let policy = CreatePolicyOptions::new(vec![1, 2], vec![0, 2, 2]);
    let universe_options = CreateUniverseOptions::new(vec![AxisIdentityInput::universe_aware(1)]);
    let family = [24_u8; 16];
    {
        let mut file =
            TensorFile::create_with_policy_and_universe(&path, options, policy, universe_options)
                .expect("create policy universe wrapper file");
        let append_options = AppendWithUniverseOptions::new(vec![SlotUniverseBindings::new(vec![
            UniverseBinding::new(1, family, [3_u8; 16], 2),
        ])]);
        file.append_f32_with_universe(&[3.0, 3.0, 4.0, 4.0], &[1, 2, 2], &append_options)
            .expect("append policy universe values");
    }
    let file = TensorFile::open(&path).expect("open policy universe wrapper file");
    let target = ExplicitUniverseAxisTarget::new(1, family, [3_u8; 16], 2);
    let read = file
        .read_with_shape_policy_dense(
            &[],
            ReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitUniverse(vec![target])),
            -1.0,
        )
        .expect("read policy universe with explicit universe target");
    assert_eq!(read.value.tensor.shape, vec![1, 2, 2]);
    assert_eq!(
        read.value.tensor.data,
        TensorData::F32(vec![3.0, 3.0, 4.0, 4.0])
    );
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_admin_metadata_helpers_validate_and_return_native_state() {
    let path = unique_path("safe-wrapper-admin-metadata.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.symbols = vec!["AAPL".to_string(), "MSFT".to_string()];
    options.channels = vec!["open".to_string(), "close".to_string()];
    options.user_kv = vec![("source".to_string(), "initial".to_string())];
    let policy = CreatePolicyOptions::new(vec![1, 2], vec![0, 2, 2]);

    let mut file = TensorFile::create_with_policy(&path, options, policy)
        .expect("create admin metadata wrapper file");
    assert_eq!(
        file.chunk_plan().expect("read chunk plan").block_sizes,
        vec![1, 2, 2]
    );
    assert_eq!(
        file.index_checkpoint_every_commits()
            .expect("read index checkpoint interval"),
        1
    );
    assert_eq!(
        file.set_index_checkpoint_every_commits(0)
            .expect_err("zero checkpoint interval rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        file.set_dim_name(0, Some(""))
            .expect_err("empty dimension name rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        file.set_dim_name(0, Some("bad\0name"))
            .expect_err("interior NUL dimension name rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        file.set_symbols(&["bad\0symbol"])
            .expect_err("interior NUL symbol rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        file.set_channels(&["bad\0channel"])
            .expect_err("interior NUL channel rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        file.set_user_kv(&[("bad\0key", "value")])
            .expect_err("interior NUL user metadata key rejected")
            .code(),
        ErrorCode::InvalidArgument
    );

    match file.set_index_checkpoint_every_commits(2) {
        Ok(()) => assert_eq!(
            file.index_checkpoint_every_commits()
                .expect("read updated checkpoint interval"),
            2
        ),
        Err(err) => assert_eq!(err.code(), ErrorCode::Unimplemented),
    }
    match file.set_dim_name(0, Some("timestamp")) {
        Ok(()) => assert_eq!(
            TensorFile::load_meta(&path)
                .expect("metadata after dimension rename")
                .dims[0]
                .name
                .as_deref(),
            Some("timestamp")
        ),
        Err(err) => assert_eq!(err.code(), ErrorCode::Unimplemented),
    }
    match file.set_symbols(&["GOOG", "TSLA"]) {
        Ok(()) => assert_eq!(
            TensorFile::load_meta(&path)
                .expect("metadata after symbol update")
                .symbols
                .into_iter()
                .map(|label| label.name)
                .collect::<Vec<_>>(),
            vec!["GOOG".to_string(), "TSLA".to_string()]
        ),
        Err(err) => assert_eq!(err.code(), ErrorCode::Unimplemented),
    }
    match file.set_channels(&["bid", "ask"]) {
        Ok(()) => assert_eq!(
            TensorFile::load_meta(&path)
                .expect("metadata after channel update")
                .channels
                .into_iter()
                .map(|label| label.name)
                .collect::<Vec<_>>(),
            vec!["bid".to_string(), "ask".to_string()]
        ),
        Err(err) => assert_eq!(err.code(), ErrorCode::Unimplemented),
    }
    match file.set_user_kv(&[("source", "updated"), ("owner", "rust-wrapper")]) {
        Ok(()) => assert_eq!(
            TensorFile::load_meta(&path)
                .expect("metadata after user metadata update")
                .user_kv
                .into_iter()
                .map(|item| (item.key, item.value))
                .collect::<Vec<_>>(),
            vec![
                ("source".to_string(), "updated".to_string()),
                ("owner".to_string(), "rust-wrapper".to_string())
            ]
        ),
        Err(err) => assert_eq!(err.code(), ErrorCode::Unimplemented),
    }

    let dim_clear_supported = match file.set_dim_name(0, None) {
        Ok(()) => {
            assert_eq!(
                TensorFile::load_meta(&path)
                    .expect("metadata after dimension name clear")
                    .dims[0]
                    .name
                    .as_deref(),
                None
            );
            true
        }
        Err(err) => {
            assert_eq!(err.code(), ErrorCode::Unimplemented);
            false
        }
    };
    let empty_user_kv: [(&str, &str); 0] = [];
    let user_kv_clear_supported = match file.set_user_kv(&empty_user_kv) {
        Ok(()) => {
            assert!(
                TensorFile::load_meta(&path)
                    .expect("metadata after user metadata clear")
                    .user_kv
                    .is_empty()
            );
            true
        }
        Err(err) => {
            assert_eq!(err.code(), ErrorCode::Unimplemented);
            false
        }
    };
    drop(file);

    let reopened = TensorFile::open(&path).expect("reopen admin metadata wrapper file");
    let reopened_meta = TensorFile::load_meta(&path).expect("metadata after admin reopen");
    if dim_clear_supported {
        assert_eq!(reopened_meta.dims[0].name.as_deref(), None);
    }
    if user_kv_clear_supported {
        assert!(reopened_meta.user_kv.is_empty());
    }
    drop(reopened);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_history_listing_and_mutation_methods() {
    let path = unique_path("safe-wrapper-history-mutation.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Channel, 1).with_name("channel"),
    ];
    let options = CreateOptions::streaming(DType::F32, dims, 0);

    let mut file = TensorFile::create(&path, options).expect("create history wrapper file");
    file.append_f32(&[1.0], &[1, 1]).expect("append first row");
    let commit_one = file
        .head_commit()
        .expect("head after first append")
        .commit_seq;
    file.append_f32(&[2.0], &[1, 1]).expect("append second row");
    let commit_two = file
        .head_commit()
        .expect("head after second append")
        .commit_seq;
    file.append_f32(&[3.0], &[1, 1]).expect("append third row");
    let commit_three = file
        .head_commit()
        .expect("head after third append")
        .commit_seq;
    assert_eq!(commit_two, commit_one + 1);
    assert_eq!(commit_three, commit_two + 1);

    let commits = file.list_commits(None).expect("list full commit history");
    assert_eq!(commits.len(), 3);
    assert_eq!(commits[0].commit_seq, commit_three);
    assert_eq!(commits[1].commit_seq, commit_two);
    assert_eq!(commits[2].commit_seq, commit_one);
    assert_eq!(file.head_commit().expect("head commit"), commits[0]);
    assert_eq!(
        file.list_commits(Some(2))
            .expect("list limited commit history")
            .iter()
            .map(|commit| commit.commit_seq)
            .collect::<Vec<_>>(),
        vec![commit_three, commit_two]
    );
    assert_eq!(
        file.list_commits(Some(0))
            .expect_err("zero limit rejected")
            .code(),
        ErrorCode::InvalidArgument
    );

    let historical = file
        .read_at_commit(commit_two, &[])
        .expect("read at retained second commit");
    assert_eq!(historical.shape, vec![2, 1]);
    assert_eq!(historical.data, TensorData::F32(vec![1.0, 2.0]));

    file.pop().expect("pop current head");
    let after_pop = file.read_all().expect("latest after pop");
    assert_eq!(after_pop.shape, vec![2, 1]);
    assert_eq!(after_pop.data, TensorData::F32(vec![1.0, 2.0]));
    let pop_head = file.head_commit().expect("head after pop").commit_seq;
    assert!(pop_head > commit_three);
    assert_eq!(
        file.list_commits(Some(2))
            .expect("history after pop")
            .iter()
            .map(|commit| commit.commit_seq)
            .collect::<Vec<_>>(),
        vec![pop_head, commit_two]
    );

    file.append_f32(&[4.0], &[1, 1]).expect("append fourth row");
    file.append_f32(&[5.0], &[1, 1]).expect("append fifth row");
    assert_eq!(
        file.pop_batched(0)
            .expect_err("zero batched pop rejected")
            .code(),
        ErrorCode::InvalidArgument
    );
    file.pop_batched(2).expect("batched pop latest two rows");
    let after_batched_pop = file.read_all().expect("latest after batched pop");
    assert_eq!(after_batched_pop.shape, vec![2, 1]);
    assert_eq!(after_batched_pop.data, TensorData::F32(vec![1.0, 2.0]));

    file.revert_commit(commit_one)
        .expect("revert to first retained commit");
    let after_revert = file.read_all().expect("latest after revert");
    assert_eq!(after_revert.shape, vec![1, 1]);
    assert_eq!(after_revert.data, TensorData::F32(vec![1.0]));
    let current_head = file.head_commit().expect("head after revert").commit_seq;
    assert!(current_head > pop_head);
    assert_eq!(
        file.read_at_commit(commit_two, &[])
            .expect("historical read survives mutations")
            .data,
        TensorData::F32(vec![1.0, 2.0])
    );
    let visible_after_revert = file
        .list_commits(Some(2))
        .expect("visible history after revert")
        .iter()
        .map(|commit| commit.commit_seq)
        .collect::<Vec<_>>();
    assert_eq!(visible_after_revert.len(), 2);
    assert_eq!(visible_after_revert[0], current_head);
    assert!(visible_after_revert[1] < current_head);
    drop(file);

    let reopened = TensorFile::open(&path).expect("reopen mutated history wrapper file");
    assert_eq!(
        reopened
            .head_commit()
            .expect("head after reopening mutated history")
            .commit_seq,
        current_head
    );
    assert_eq!(
        reopened
            .list_commits(Some(2))
            .expect("visible history after reopening")
            .iter()
            .map(|commit| commit.commit_seq)
            .collect::<Vec<_>>(),
        visible_after_revert
    );
    assert_eq!(
        reopened
            .read_at_commit(commit_two, &[])
            .expect("historical read survives reopening after mutations")
            .data,
        TensorData::F32(vec![1.0, 2.0])
    );
    drop(reopened);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_universe_shape_policy_and_historical_reads() {
    let path = unique_path("safe-wrapper-universe-shape-policy.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let options = CreateOptions::streaming(DType::F32, dims, 0);
    let universe_options = CreateUniverseOptions::new(vec![AxisIdentityInput::universe_aware(1)]);
    let family = [42_u8; 16];

    {
        let mut file = TensorFile::create_with_universe(&path, options, universe_options)
            .expect("create universe-aware wrapper file");
        let first = AppendWithUniverseOptions::new(vec![SlotUniverseBindings::new(vec![
            UniverseBinding::new(1, family, [1_u8; 16], 2),
        ])]);
        let first_range = file
            .append_f32_with_universe(&[1.0, 1.0, 1.0, 1.0], &[1, 2, 2], &first)
            .expect("append first universe row");
        assert_eq!((first_range.start, first_range.end), (0, 1));

        let second = AppendWithUniverseOptions::new(vec![SlotUniverseBindings::new(vec![
            UniverseBinding::new(1, family, [2_u8; 16], 2),
        ])]);
        let second_range = file
            .append_f32_with_universe(&[2.0, 2.0, 2.0, 2.0], &[1, 2, 2], &second)
            .expect("append second universe row");
        assert_eq!((second_range.start, second_range.end), (1, 2));
    }

    let file = TensorFile::open(&path).expect("reopen universe-aware wrapper file");
    let current_selectors = vec![
        EntrySelector::Range { start: 1, end: 2 },
        EntrySelector::All,
        EntrySelector::All,
    ];
    let current_policy = ReadShapePolicy::ExplicitUniverse(vec![ExplicitUniverseAxisTarget::new(
        1, family, [2_u8; 16], 2,
    )]);
    let current = file
        .read_with_shape_policy_dense(
            &current_selectors,
            ReadWithShapePolicyOptions::serial(current_policy),
            -1.0,
        )
        .expect("current explicit-universe dense read");
    assert_eq!(current.value.tensor.dtype, DType::F32);
    assert_eq!(current.value.tensor.shape, vec![1, 2, 2]);
    assert_eq!(current.value.tensor.data, TensorData::F32(vec![2.0; 4]));

    let historical_policy =
        ReadShapePolicy::ExplicitUniverse(vec![ExplicitUniverseAxisTarget::new(
            1, family, [1_u8; 16], 2,
        )]);
    let historical = file
        .read_at_commit_with_shape_policy_dense(
            1,
            &[],
            HistoricalReadWithShapePolicyOptions::serial(historical_policy),
            -1.0,
        )
        .expect("historical explicit-universe dense read");
    assert_eq!(historical.value.tensor.shape, vec![1, 2, 2]);
    assert_eq!(historical.value.tensor.data, TensorData::F32(vec![1.0; 4]));
    assert_eq!(historical.execution.query_commit_seq, 1);
    assert_eq!(
        historical.execution.query_source_kind,
        HistoricalQuerySourceKind::RetainedVisibleCommit
    );

    let combined_policy = ReadShapePolicy::ExplicitUniverseAndExtents {
        universe_axes: vec![ExplicitUniverseAxisTarget::new(1, family, [2_u8; 16], 2)],
        extent_axes: vec![ExplicitExtentAxisTarget::new(2, 3)],
    };
    let combined = file
        .read_with_shape_policy_dense(
            &current_selectors,
            ReadWithShapePolicyOptions::serial(combined_policy),
            -1.0,
        )
        .expect("combined explicit-universe/extents dense read");
    assert_eq!(combined.value.tensor.shape, vec![1, 2, 3]);
    assert_eq!(combined.value.mask.as_ref().map(Vec::len), Some(6));
    assert_eq!(combined.value.mask.as_ref().expect("mask")[2], 0);

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_rejects_universe_create_with_coordinates() {
    let path = unique_path("safe-wrapper-universe-coordinate-reject.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Channel, 1),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("channel_id".to_string()),
        kind: CoordinateKind::LabelId,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![7])),
        ordering: CoordinateOrdering {
            sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });
    let err = match TensorFile::create_with_universe(
        &path,
        options,
        CreateUniverseOptions::new(vec![AxisIdentityInput::universe_aware(1)]),
    ) {
        Ok(_) => panic!("coordinates plus universe create unexpectedly succeeded"),
        Err(err) => err,
    };
    assert_eq!(err.code(), arcadia_tio_rs::ErrorCode::InvalidArgument);
    assert!(!path.exists());
}

fn assert_query_trace_json(json: &str, operation: &str) {
    assert_json_object_text(json);
    assert!(json.contains("\"schema_version\":\"tio_query_attribution_trace.v1\""));
    assert!(json.contains("\"public_api_call_seconds\":null"));
    assert!(json.contains("\"name\":\"selector_normalize_and_validate\""));
    assert!(json.contains("\"category\":\"binding\""));
    assert!(json.contains(&format!("\"operation\":\"{operation}\"")));
}

fn assert_json_object_text(json: &str) {
    let bytes = json.as_bytes();
    assert!(
        bytes.first() == Some(&b'{'),
        "JSON should start with object: {json}"
    );
    assert!(
        bytes.last() == Some(&b'}'),
        "JSON should end with object: {json}"
    );
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in bytes {
        if in_string {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'\"' {
                in_string = false;
            }
            continue;
        }
        match byte {
            b'\"' => in_string = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => depth -= 1,
            _ => {}
        }
        assert!(depth >= 0, "JSON delimiters closed too early: {json}");
    }
    assert!(!in_string, "JSON string was unterminated: {json}");
    assert_eq!(depth, 0, "JSON delimiters were unbalanced: {json}");
}

fn roundtrip_dtype(
    label: &str,
    dtype: DType,
    append: impl FnOnce(&mut TensorFile) -> arcadia_tio_rs::Result<arcadia_tio_rs::AppendRange>,
    expected: TensorData,
) {
    let path = unique_path(&format!("safe-wrapper-{label}.tio"));
    let options = CreateOptions::streaming(dtype, vec![DimSpec::new(AxisKind::Time, 0)], 0);
    {
        let mut file = TensorFile::create(&path, options).expect("create through safe wrapper");
        let range = append(&mut file).expect("append through safe wrapper");
        assert_eq!((range.start, range.end), (0, 3));
    }
    let file = TensorFile::open(&path).expect("open through safe wrapper");
    let tensor = file.read_all().expect("read through safe wrapper");
    assert_eq!(tensor.dtype, dtype);
    assert_eq!(tensor.shape, vec![3]);
    assert_eq!(tensor.data, expected);
    drop(file);
    let _ = fs::remove_file(path);
}

fn unique_path(name: &str) -> PathBuf {
    let nonce = format!("{}-{}", std::process::id(), unique_counter());
    std::env::temp_dir().join(format!("{nonce}-{name}"))
}

fn unique_counter() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
