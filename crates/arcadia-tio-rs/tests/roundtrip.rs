use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::{
    AppendWithUniverseOptions, AxisIdentityInput, AxisKind, CompressionConfig, CoordinateDType,
    CoordinateEncoding, CoordinateKind, CoordinateMonotonicity, CoordinateOrdering, CoordinateSpec,
    CoordinateStorage, CoordinateStorageKind, CoordinateUniqueness, CoordinateValidationStatus,
    CoordinateValues, CreateInferredOptions, CreateOptions, CreatePolicyOptions,
    CreateUniverseOptions, DType, DimSpec, EntrySelector, ExplicitExtentAxisTarget,
    ExplicitUniverseAxisTarget, HistoricalQuerySourceKind, HistoricalReadWithOptions,
    HistoricalReadWithShapePolicyOptions, ReadShapePolicy, ReadWithOptions,
    ReadWithShapePolicyOptions, SlotUniverseBindings, StorageAccessKind, TensorData, TensorFile,
    UniverseBinding,
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
    let inferred_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0),
            DimSpec::new(AxisKind::Symbol, 2),
        ],
        0,
    );
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
    drop(file);
    let _ = fs::remove_file(inferred_path);
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
