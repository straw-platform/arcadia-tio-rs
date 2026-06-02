use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::{
    AppendCoordinateBatch, AppendCoordinateEntry, AppendWithUniverseOptions, AutoCompactionConfig,
    AxisCoordinateInput, AxisIdentityInput, AxisKind, CompactionMode, CompactionOptions,
    CompressionCodec, CompressionConfig, CompressionMode, CoordinateAvailability,
    CoordinateCodeDType, CoordinateDType, CoordinateDictionaryEntry, CoordinateDictionarySummary,
    CoordinateEncoding, CoordinateExternalBindingV2, CoordinateFixedTextLayout, CoordinateKind,
    CoordinateLookupKey, CoordinateLookupResultStatus, CoordinateMonotonicity, CoordinateOptions,
    CoordinateOrdering, CoordinateSourceKind, CoordinateSpec, CoordinateStatusCategory,
    CoordinateStorage, CoordinateStorageKind, CoordinateUniqueness, CoordinateValidationStatus,
    CoordinateValueDomain, CoordinateValues, CreateInferredOptions, CreateOptions,
    CreatePolicyOptions, CreateUniverseOptions, DType, DimSpec, EntrySelector, ErrorCode,
    ExplicitExtentAxisTarget, ExplicitUniverseAxisTarget, HistoricalQuerySourceKind,
    HistoricalReadWithOptions, HistoricalReadWithShapePolicyOptions, QueryTraceContext,
    ReadIndexItem, ReadIndexLoweringKind, ReadShapePolicy, ReadWithOptions,
    ReadWithShapePolicyOptions, ReformOptions, SlotUniverseBindings, SparseAppendOutcome,
    SparseAppendReason, SparseRule, SparseValuePredicate, StorageAccessKind, Tensor, TensorData,
    TensorF32, TensorF64, TensorFile, TensorI32, TensorI64, UniverseBinding,
    V4CompactionAnalysisPolicy, V4PreciseAccountingField, V4PreciseAccountingOptions,
    V4ReportStatus, V4RetainedHistoryCompactionOptions, typed_ops,
};

fn i32_bytes(values: &[i32]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

fn u16_bytes(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

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
fn safe_wrapper_coordinate_v2_numeric_and_fixed_text_roundtrip() {
    let path = unique_path("safe-wrapper-coordinate-v2-numeric-fixed.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let coordinates = vec![
        AxisCoordinateInput::inline_i32(1, vec![10, 20])
            .with_descriptor_id("symbol-id-v2")
            .with_name("symbol_id")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true)
            .with_ordering(CoordinateOrdering {
                sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
                monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
                uniqueness: CoordinateUniqueness::Unique,
            }),
        AxisCoordinateInput::fixed_text_ascii(2, 4, ["BID", "ASK"])
            .expect("fixed-text descriptor")
            .with_descriptor_id("channel-code-v2")
            .with_name("channel_code")
            .with_required(true),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &path,
            options,
            &coordinates,
            CoordinateOptions::default(),
        )
        .expect("create Coordinate v2 numeric/fixed file");
        file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 2, 2])
            .expect("append Coordinate v2 payload");
    }

    let file = TensorFile::open(&path).expect("open Coordinate v2 numeric/fixed file");
    let meta = file.coordinate_metadata().expect("Coordinate v2 metadata");
    assert_eq!(meta.len(), 2);
    assert_eq!(meta[0].descriptor_id.as_deref(), Some("symbol-id-v2"));
    assert_eq!(meta[0].value_domain, CoordinateValueDomain::InlineNumeric);
    assert_eq!(meta[0].availability, CoordinateAvailability::Available);
    assert_eq!(meta[0].status_category, CoordinateStatusCategory::Ok);
    assert_eq!(meta[1].descriptor_id.as_deref(), Some("channel-code-v2"));
    assert_eq!(meta[1].value_domain, CoordinateValueDomain::FixedText);
    assert_eq!(meta[1].fixed_text.width, 4);

    let numeric_values = file
        .read_coordinate_axis(1, CoordinateOptions::default())
        .expect("Coordinate v2 numeric values");
    assert_eq!(
        numeric_values.value_domain,
        CoordinateValueDomain::InlineNumeric
    );
    assert_eq!(numeric_values.numeric_dtype, CoordinateDType::I32);
    assert_eq!(numeric_values.len, 2);
    assert_eq!(numeric_values.element_size, std::mem::size_of::<i32>());
    assert_eq!(
        numeric_values.availability,
        CoordinateAvailability::Available
    );
    assert_eq!(numeric_values.status_category, CoordinateStatusCategory::Ok);
    assert_eq!(numeric_values.data, i32_bytes(&[10, 20]));

    let text_values = file
        .read_coordinate_axis(2, CoordinateOptions::default())
        .expect("Coordinate v2 fixed-text values");
    assert_eq!(text_values.value_domain, CoordinateValueDomain::FixedText);
    assert_eq!(text_values.fixed_text_width, 4);
    assert_eq!(text_values.len, 2);
    assert_eq!(text_values.data, b"BID ASK ".to_vec());

    let lookup_options = CoordinateOptions::authoritative_scan();
    let numeric_exact = file
        .coordinate_lookup(1, &CoordinateLookupKey::i32(20), lookup_options)
        .expect("Coordinate v2 numeric exact lookup");
    assert_eq!(numeric_exact.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(numeric_exact.unique_position(), Some(1));
    assert_eq!(numeric_exact.status_category, CoordinateStatusCategory::Ok);
    let numeric_range = file
        .coordinate_lookup_range(
            1,
            &CoordinateLookupKey::i32(10),
            &CoordinateLookupKey::i32(21),
            lookup_options,
        )
        .expect("Coordinate v2 numeric range lookup");
    assert_eq!(numeric_range.status, CoordinateLookupResultStatus::Range);
    assert_eq!(numeric_range.range(), Some(0..2));

    let fixed_exact = file
        .coordinate_lookup(
            2,
            &CoordinateLookupKey::fixed_text_ascii("ASK", 4).expect("fixed-text key"),
            lookup_options,
        )
        .expect("Coordinate v2 fixed-text exact lookup");
    assert_eq!(fixed_exact.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(fixed_exact.unique_position(), Some(1));
    let fixed_range = file
        .coordinate_lookup_range(
            2,
            &CoordinateLookupKey::fixed_text_ascii("BID", 4).expect("fixed-text lower"),
            &CoordinateLookupKey::fixed_text_ascii("BIE", 4).expect("fixed-text upper"),
            lookup_options,
        )
        .expect("Coordinate v2 fixed-text range lookup");
    assert_eq!(fixed_range.status, CoordinateLookupResultStatus::Range);
    assert_eq!(fixed_range.range(), Some(0..1));

    let missing = file
        .coordinate_lookup(1, &CoordinateLookupKey::i32(99), lookup_options)
        .expect("Coordinate v2 missing is a visible result");
    assert_eq!(missing.status, CoordinateLookupResultStatus::Missing);
    assert_eq!(missing.status_category, CoordinateStatusCategory::Ok);
    let domain_mismatch = file
        .coordinate_lookup(2, &CoordinateLookupKey::i64(10), lookup_options)
        .expect("Coordinate v2 domain mismatch is a visible result");
    assert_eq!(
        domain_mismatch.status_category,
        CoordinateStatusCategory::LookupDomainMismatch
    );
    assert!(domain_mismatch.is_error() || domain_mismatch.is_unsupported());

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_coordinate_v2_dictionary_roundtrip() {
    let path = unique_path("safe-wrapper-coordinate-v2-dictionary.tio");
    let options = CreateOptions::random_access(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let dictionary_summary = CoordinateDictionarySummary::new(CoordinateCodeDType::U16)
        .with_dictionary_id("symbol-dict-v2")
        .with_revision(7)
        .with_content_id("symbol-dict-content-v2");
    let dictionary_entries = vec![
        CoordinateDictionaryEntry::new(1, Some("AAPL".to_string()), Some("AAPL".to_string())),
        CoordinateDictionaryEntry::new(2, Some("MSFT".to_string()), Some("MSFT".to_string())),
    ];
    let coordinates = vec![
        AxisCoordinateInput::dictionary_codes_u16(
            1,
            vec![1, 2],
            CoordinateFixedTextLayout::ascii_right_space_padded(4).expect("dictionary labels"),
            dictionary_summary,
            dictionary_entries,
        )
        .expect("dictionary-code descriptor")
        .with_descriptor_id("symbol-dictionary-code-v2")
        .with_name("symbol_code")
        .with_required(true),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &path,
            options,
            &coordinates,
            CoordinateOptions::default(),
        )
        .expect("create Coordinate v2 dictionary file");
        file.append_f64(&[1.5, 2.5], &[1, 2])
            .expect("append dictionary payload");
    }

    let file = TensorFile::open(&path).expect("open Coordinate v2 dictionary file");
    let meta = file
        .coordinate_metadata()
        .expect("Coordinate v2 dictionary metadata");
    assert_eq!(meta.len(), 1);
    assert_eq!(meta[0].value_domain, CoordinateValueDomain::DictionaryCode);
    assert_eq!(
        meta[0].dictionary.dictionary_id.as_deref(),
        Some("symbol-dict-v2")
    );
    assert_eq!(meta[0].dictionary.revision, 7);
    assert_eq!(meta[0].dictionary.entry_count, 2);

    let code_values = file
        .read_coordinate_axis(1, CoordinateOptions::default())
        .expect("Coordinate v2 dictionary code values");
    assert_eq!(
        code_values.value_domain,
        CoordinateValueDomain::DictionaryCode
    );
    assert_eq!(code_values.code_dtype, CoordinateCodeDType::U16);
    assert_eq!(code_values.data, u16_bytes(&[1, 2]));

    let dictionary = file
        .coordinate_dictionary(
            1,
            CoordinateOptions {
                include_dictionary_entries: true,
                ..CoordinateOptions::default()
            },
        )
        .expect("Coordinate v2 dictionary read");
    assert_eq!(dictionary.status_category, CoordinateStatusCategory::Ok);
    assert_eq!(
        dictionary.summary.dictionary_id.as_deref(),
        Some("symbol-dict-v2")
    );
    assert_eq!(dictionary.entries.len(), 2);
    assert_eq!(dictionary.entries[0].stable_id.as_deref(), Some("AAPL"));
    assert_eq!(dictionary.entries[1].display_label.as_deref(), Some("MSFT"));

    let lookup_options = CoordinateOptions::authoritative_scan();
    let code_lookup = file
        .coordinate_lookup(1, &CoordinateLookupKey::dictionary_code(2), lookup_options)
        .expect("Coordinate v2 dictionary-code lookup");
    assert_eq!(code_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(code_lookup.unique_position(), Some(1));
    let stable_lookup = file
        .coordinate_lookup(1, &CoordinateLookupKey::stable_id("AAPL"), lookup_options)
        .expect("Coordinate v2 stable-id lookup");
    assert_eq!(stable_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(stable_lookup.unique_position(), Some(0));
    let label_lookup = file
        .coordinate_lookup(
            1,
            &CoordinateLookupKey::display_label("MSFT"),
            lookup_options,
        )
        .expect("Coordinate v2 display-label lookup");
    assert_eq!(label_lookup.status, CoordinateLookupResultStatus::Unique);
    assert_eq!(label_lookup.unique_position(), Some(1));
    let alias_lookup = file
        .coordinate_lookup(1, &CoordinateLookupKey::alias("A.N"), lookup_options)
        .expect("Coordinate v2 alias unsupported is a visible result");
    assert_eq!(
        alias_lookup.status,
        CoordinateLookupResultStatus::Unsupported
    );
    assert_eq!(
        alias_lookup.status_category,
        CoordinateStatusCategory::UnsupportedDomain
    );

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_coordinate_v2_append_with_coordinates_success() {
    let numeric_path = unique_path("safe-wrapper-coordinate-v2-append-numeric.tio");
    let fixed_path = unique_path("safe-wrapper-coordinate-v2-append-fixed.tio");
    let dict_path = unique_path("safe-wrapper-coordinate-v2-append-dictionary.tio");

    let numeric_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let numeric_coordinates = vec![
        AxisCoordinateInput::append_numeric_i32(0)
            .with_descriptor_id("append-day-v2")
            .with_name("append_day")
            .with_kind(CoordinateKind::Date)
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd)
            .with_required(true)
            .with_ordering(CoordinateOrdering {
                sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
                monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
                uniqueness: CoordinateUniqueness::Unique,
            }),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &numeric_path,
            numeric_options,
            &numeric_coordinates,
            CoordinateOptions::default(),
        )
        .expect("create append numeric Coordinate v2 file");
        let batch = AppendCoordinateBatch::new(vec![
            AppendCoordinateEntry::i32(0, vec![20260531, 20260601])
                .with_descriptor_id("append-day-v2")
                .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
        ]);
        let range = file
            .append_f32_with_coordinates(&[1.0, 2.0, 3.0, 4.0], &[2, 2], &batch)
            .expect("append f32 payload with numeric append coordinates");
        assert_eq!((range.start, range.end), (0, 2));
        assert_eq!(file.dim_lens().expect("numeric append dims"), vec![2, 2]);
        assert_eq!(
            file.read_all().expect("numeric append payload").data,
            TensorData::F32(vec![1.0, 2.0, 3.0, 4.0])
        );
        let meta = file
            .coordinate_metadata()
            .expect("numeric append-coordinate metadata");
        assert_eq!(meta[0].value_domain, CoordinateValueDomain::AppendSequence);
        assert_eq!(meta[0].numeric_dtype, CoordinateDType::I32);
        assert_eq!(meta[0].length, 2);
    }

    let fixed_options = CreateOptions::streaming(
        DType::I32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 1).with_name("channel"),
        ],
        0,
    );
    let fixed_layout =
        CoordinateFixedTextLayout::ascii_right_space_padded(4).expect("fixed append layout");
    let fixed_coordinates = vec![
        AxisCoordinateInput::append_fixed_text(0, fixed_layout)
            .expect("append fixed-text descriptor")
            .with_descriptor_id("append-session-v2")
            .with_name("append_session")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &fixed_path,
            fixed_options,
            &fixed_coordinates,
            CoordinateOptions::default(),
        )
        .expect("create append fixed-text Coordinate v2 file");
        let batch = AppendCoordinateBatch::new(vec![
            AppendCoordinateEntry::fixed_text_ascii(0, 4, ["AM", "PM"])
                .expect("fixed append values")
                .with_descriptor_id("append-session-v2"),
        ]);
        let range = file
            .append_i32_with_coordinates(&[5, 6], &[2, 1], &batch)
            .expect("append i32 payload with fixed-text append coordinates");
        assert_eq!((range.start, range.end), (0, 2));
        assert_eq!(file.dim_lens().expect("fixed append dims"), vec![2, 1]);
        let fixed_lookup = file
            .coordinate_lookup(
                0,
                &CoordinateLookupKey::fixed_text_ascii("PM", 4).expect("fixed lookup key"),
                CoordinateOptions::authoritative_scan(),
            )
            .expect("fixed append coordinate lookup");
        assert_eq!(fixed_lookup.status, CoordinateLookupResultStatus::Unique);
        assert_eq!(fixed_lookup.unique_position(), Some(1));
    }

    let dict_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 1).with_name("channel"),
        ],
        0,
    );
    let mut dictionary_summary = CoordinateDictionarySummary::new(CoordinateCodeDType::U16)
        .with_dictionary_id("instrument-dictionary")
        .with_revision(1);
    dictionary_summary.entry_count = 2;
    let dictionary_entries = vec![
        CoordinateDictionaryEntry::new(
            1,
            Some("instrument-a".to_string()),
            Some("AAA".to_string()),
        ),
        CoordinateDictionaryEntry::new(
            2,
            Some("instrument-b".to_string()),
            Some("BBB".to_string()),
        ),
    ];
    let dict_coordinates = vec![
        AxisCoordinateInput::append_dictionary_codes(
            0,
            CoordinateCodeDType::U16,
            fixed_layout,
            dictionary_summary,
            dictionary_entries,
        )
        .expect("append dictionary descriptor")
        .with_descriptor_id("append-instrument-v2")
        .with_name("append_instrument")
        .with_kind(CoordinateKind::LabelId)
        .with_required(true),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &dict_path,
            dict_options,
            &dict_coordinates,
            CoordinateOptions::default(),
        )
        .expect("create append dictionary Coordinate v2 file");
        let batch = AppendCoordinateBatch::new(vec![
            AppendCoordinateEntry::dictionary_codes_u16(0, vec![1, 2])
                .expect("dictionary append codes")
                .with_descriptor_id("append-instrument-v2"),
        ]);
        assert_eq!(
            file.append_f32_with_coordinates(&[10.0, 20.0], &[2, 1], &batch)
                .expect("append dictionary codes")
                .end,
            2
        );
        let extension_batch = AppendCoordinateBatch::new(vec![
            AppendCoordinateEntry::dictionary_codes_u16_with_entries(
                0,
                vec![3],
                vec![CoordinateDictionaryEntry::new(
                    3,
                    Some("instrument-c".to_string()),
                    Some("CCC".to_string()),
                )],
            )
            .expect("dictionary extension append codes")
            .with_descriptor_id("append-instrument-v2"),
        ]);
        assert_eq!(
            file.append_f32_with_coordinates(&[30.0], &[1, 1], &extension_batch)
                .expect("append dictionary extension")
                .start,
            2
        );
        let lookup = file
            .coordinate_lookup(
                0,
                &CoordinateLookupKey::stable_id("instrument-c"),
                CoordinateOptions::authoritative_scan(),
            )
            .expect("lookup appended dictionary entry");
        assert_eq!(lookup.status, CoordinateLookupResultStatus::Unique);
        assert_eq!(lookup.unique_position(), Some(2));
        let invalid_extension = AppendCoordinateBatch::new(vec![
            AppendCoordinateEntry::dictionary_codes_u16_with_entries(
                0,
                vec![3],
                vec![CoordinateDictionaryEntry::new(
                    3,
                    Some("instrument-c-again".to_string()),
                    Some("CC2".to_string()),
                )],
            )
            .expect("duplicate dictionary extension append codes")
            .with_descriptor_id("append-instrument-v2"),
        ]);
        assert!(
            file.append_f32_with_coordinates(&[40.0], &[1, 1], &invalid_extension)
                .is_err()
        );
        assert_eq!(
            file.dim_lens().expect("dict dims after failed append"),
            vec![3, 1]
        );
    }

    let _ = fs::remove_file(numeric_path);
    let _ = fs::remove_file(fixed_path);
    let _ = fs::remove_file(dict_path);
}

#[test]
fn safe_wrapper_coordinate_v2_append_rejects_missing_required_and_wrong_count() {
    let path = unique_path("safe-wrapper-coordinate-v2-append-failures.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let coordinates = vec![
        AxisCoordinateInput::append_numeric_i32(0)
            .with_descriptor_id("required-append-day-v2")
            .with_name("required_append_day")
            .with_kind(CoordinateKind::Date)
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd)
            .with_required(true),
    ];

    let mut file = TensorFile::create_with_coordinates(
        &path,
        options,
        &coordinates,
        CoordinateOptions::default(),
    )
    .expect("create required append-coordinate file");
    let good_batch = AppendCoordinateBatch::new(vec![
        AppendCoordinateEntry::i32(0, vec![20260531, 20260601])
            .with_descriptor_id("required-append-day-v2")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    file.append_f64_with_coordinates(&[1.0, 2.0, 3.0, 4.0], &[2, 2], &good_batch)
        .expect("seed append-coordinate state");
    let before_dims = file.dim_lens().expect("dims before failed append");
    let before_payload = file.read_all().expect("payload before failed append");
    let before_meta = file
        .coordinate_metadata()
        .expect("metadata before failed append");
    let before_values = file
        .read_coordinate_axis(0, CoordinateOptions::default())
        .expect("coordinates before failed append");

    let missing = file
        .append_f64_with_coordinates(
            &[5.0, 6.0, 7.0, 8.0],
            &[2, 2],
            &AppendCoordinateBatch::empty(),
        )
        .expect_err("missing required append coordinates should fail");
    assert_eq!(missing.code(), ErrorCode::InvalidArgument);
    assert!(
        missing.message().contains("missing required")
            || missing.message().contains("required append coordinate"),
        "unexpected missing-required error: {}",
        missing.message()
    );

    let wrong_count_batch = AppendCoordinateBatch::new(vec![
        AppendCoordinateEntry::i32(0, vec![20260602])
            .with_descriptor_id("required-append-day-v2")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    let wrong_count = file
        .append_f64_with_coordinates(&[5.0, 6.0, 7.0, 8.0], &[2, 2], &wrong_count_batch)
        .expect_err("wrong-count append coordinates should fail");
    assert_eq!(wrong_count.code(), ErrorCode::InvalidArgument);
    assert!(
        wrong_count.message().contains("count") || wrong_count.message().contains("extent"),
        "unexpected wrong-count error: {}",
        wrong_count.message()
    );

    assert_eq!(
        file.dim_lens().expect("dims after failed append"),
        before_dims
    );
    assert_eq!(
        file.read_all().expect("payload after failed append"),
        before_payload
    );
    assert_eq!(
        file.coordinate_metadata()
            .expect("metadata after failed append"),
        before_meta
    );
    assert_eq!(
        file.read_coordinate_axis(0, CoordinateOptions::default())
            .expect("coordinates after failed append"),
        before_values
    );

    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_coordinate_v2_external_unavailable_status_without_dereference() {
    let path = unique_path("safe-wrapper-coordinate-v2-external.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let external_binding = CoordinateExternalBindingV2::metadata_only(
        CoordinateSourceKind::SameFileObject,
        Some("coords-symbols".to_string()),
        Some("symbol coordinate object".to_string()),
        CoordinateValueDomain::FixedText,
        2,
    );
    let coordinates = vec![
        AxisCoordinateInput::external_reference_fixed_text(
            1,
            external_binding,
            CoordinateFixedTextLayout::ascii_right_space_padded(4).expect("external layout"),
        )
        .expect("external fixed-text descriptor")
        .with_descriptor_id("symbol-external-v2")
        .with_name("symbol_external"),
    ];
    {
        let mut file = TensorFile::create_with_coordinates(
            &path,
            options,
            &coordinates,
            CoordinateOptions::default(),
        )
        .expect("create Coordinate v2 external file");
        file.append_f32(&[3.0, 4.0], &[1, 2])
            .expect("append external-coordinate payload");
    }

    let file = TensorFile::open(&path).expect("open Coordinate v2 external file");
    let meta = file
        .coordinate_metadata()
        .expect("Coordinate v2 external metadata");
    assert_eq!(meta.len(), 1);
    assert_eq!(
        meta[0].value_domain,
        CoordinateValueDomain::ExternalReference
    );
    assert_eq!(meta[0].availability, CoordinateAvailability::Unavailable);
    assert_eq!(meta[0].status_category, CoordinateStatusCategory::Ok);
    assert!(!meta[0].required);
    assert_eq!(
        meta[0].external_binding.logical_id.as_deref(),
        Some("coords-symbols")
    );
    assert_eq!(
        meta[0].external_binding.availability,
        CoordinateAvailability::Unavailable
    );

    let values = file
        .read_coordinate_axis(1, CoordinateOptions::default())
        .expect("Coordinate v2 external values return status carrier");
    assert_ne!(values.availability, CoordinateAvailability::Available);
    assert_eq!(values.status_category, CoordinateStatusCategory::Ok);
    assert!(
        values
            .reason
            .as_deref()
            .map_or(true, |reason| !reason.is_empty())
    );
    assert!(values.data.is_empty());

    let unavailable_lookup = file
        .coordinate_lookup(
            1,
            &CoordinateLookupKey::fixed_text_ascii("AAPL", 4).expect("external fixed key"),
            CoordinateOptions::default(),
        )
        .expect("Coordinate v2 external unavailable lookup is visible");
    assert_eq!(
        unavailable_lookup.status,
        CoordinateLookupResultStatus::Unavailable
    );
    assert_eq!(
        unavailable_lookup.availability,
        CoordinateAvailability::Unavailable
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
fn typed_tensor_constructors_accessors_and_roundtrip_supported_dtypes() {
    let f32_tensor = TensorF32::from_dense(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0])
        .expect("valid typed f32 tensor");
    assert_eq!(f32_tensor.dtype(), DType::F32);
    assert_eq!(f32_tensor.shape(), &[2, 2]);
    assert_eq!(f32_tensor.element_len().expect("element count"), 4);
    assert_eq!(
        f32_tensor.values().expect("typed f32 values"),
        &[1.0, 2.0, 3.0, 4.0]
    );

    let raw: Tensor = f32_tensor.clone().into();
    assert_eq!(raw.data, TensorData::F32(vec![1.0, 2.0, 3.0, 4.0]));
    let rebuilt = TensorF32::try_from(raw).expect("typed f32 roundtrip from Tensor");
    assert_eq!(rebuilt, f32_tensor);

    let f64_tensor =
        TensorF64::from_dense(vec![1, 2], vec![1.5, 2.5]).expect("valid typed f64 tensor");
    assert_eq!(f64_tensor.values().expect("typed f64 values"), &[1.5, 2.5]);

    let i32_tensor =
        TensorI32::from_dense(vec![3], vec![10, 20, 30]).expect("valid typed i32 tensor");
    assert_eq!(
        i32_tensor.values().expect("typed i32 values"),
        &[10, 20, 30]
    );

    let i64_tensor =
        TensorI64::from_dense(vec![2], vec![100, 200]).expect("valid typed i64 tensor");
    assert_eq!(
        i64_tensor.into_tensor().data,
        TensorData::I64(vec![100, 200])
    );
}

#[test]
fn typed_tensor_rejects_shape_and_dtype_mismatches() {
    let err =
        TensorF32::from_dense(vec![2, 2], vec![1.0, 2.0]).expect_err("shape mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let raw_f64 = Tensor::from_dense_f64(vec![2], vec![1.0, 2.0]).expect("valid f64 tensor");
    let err = TensorF32::try_from(raw_f64).expect_err("dtype mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let inconsistent = Tensor {
        dtype: DType::F32,
        shape: vec![1],
        data: TensorData::F64(vec![1.0]),
    };
    let err = TensorF32::try_from_tensor(inconsistent).expect_err("payload dtype mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[test]
fn typed_ops_forward_selected_operations_and_validate_outputs() {
    let tensor = TensorF64::from_dense(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .expect("valid typed f64 tensor");

    let transposed = typed_ops::transpose(&tensor).expect("typed transpose");
    assert_eq!(transposed.shape(), &[3, 2]);
    assert_eq!(
        transposed.values().expect("transposed values"),
        &[1.0, 4.0, 2.0, 5.0, 3.0, 6.0]
    );

    let shifted = typed_ops::add_scalar(&tensor, 10.0).expect("typed add scalar");
    assert_eq!(shifted.values().expect("shifted values")[0], 11.0);

    let row_sums = typed_ops::sum(&tensor, Some(&[1]), false).expect("typed row sums");
    assert_eq!(row_sums.shape(), &[2]);
    assert_eq!(row_sums.values().expect("row sum values"), &[6.0, 15.0]);

    let row_argmax = typed_ops::argmax(&tensor, Some(&[1]), false).expect("typed argmax");
    assert_eq!(row_argmax.dtype(), DType::I64);
    assert_eq!(row_argmax.values().expect("argmax values"), &[2, 2]);

    let rows = typed_ops::split(&tensor, 0, &[1, 1]).expect("typed split");
    let rejoined = typed_ops::concat(&[&rows[0], &rows[1]], 0).expect("typed concat");
    assert_eq!(rejoined.into_tensor(), tensor.clone().into_tensor());

    let ints = TensorI32::from_dense(vec![2, 2], vec![1, 2, 3, 4]).expect("valid typed i32 tensor");
    let doubled = typed_ops::mul_scalar(&ints, 2).expect("typed integer scalar multiply");
    assert_eq!(
        doubled.values().expect("doubled integer values"),
        &[2, 4, 6, 8]
    );
    let cumulative = typed_ops::cumsum(&ints, Some(1)).expect("typed integer cumsum");
    assert_eq!(
        cumulative.values().expect("integer cumsum values"),
        &[1, 3, 3, 7]
    );
}

#[test]
fn typed_tensor_addition_preserves_untyped_tensor_api_compatibility() {
    let tensor = Tensor::from_dense_i32(vec![2], vec![10, 20]).expect("valid untyped i32 tensor");
    assert_eq!(tensor.values_i32().expect("untyped values"), &[10, 20]);

    let shifted = arcadia_tio_rs::ops::add_scalar(&tensor, 5_i32).expect("untyped add scalar");
    assert_eq!(shifted.data, TensorData::I32(vec![15, 25]));

    let typed = TensorI32::try_from(tensor.clone()).expect("typed view of existing Tensor");
    assert_eq!(typed.into_tensor(), tensor);
}

#[cfg(feature = "arrow")]
#[test]
fn tensor_arrow_record_batch_and_ipc_roundtrip_owned_values() {
    let tensor = Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .expect("valid f32 tensor");

    let batch = tensor
        .to_arrow_record_batch()
        .expect("convert tensor to Arrow record batch");
    assert_eq!(batch.num_rows(), 2);
    assert_eq!(
        batch
            .schema()
            .metadata()
            .get("arcadia_tio_dim_lens")
            .map(String::as_str),
        Some("2,3")
    );
    assert_eq!(
        batch
            .schema()
            .metadata()
            .get("arcadia_tio_order")
            .map(String::as_str),
        Some("row-major")
    );
    assert_eq!(
        Tensor::from_arrow_record_batch(&batch).expect("decode Arrow record batch"),
        tensor
    );

    let ipc = tensor.to_arrow_ipc().expect("encode Arrow IPC");
    assert!(!ipc.is_empty());
    assert_eq!(
        Tensor::from_arrow_ipc(&ipc).expect("decode Arrow IPC"),
        tensor
    );

    let int_tensor = Tensor::from_dense_i64(vec![3], vec![10, 20, 30]).expect("valid i64 tensor");
    assert_eq!(
        Tensor::from_arrow_ipc(&int_tensor.to_arrow_ipc().expect("encode i64 IPC"))
            .expect("decode i64 IPC"),
        int_tensor
    );
}

#[cfg(feature = "arrow")]
#[test]
fn tensor_arrow_record_batch_rejects_shape_metadata_mismatch() {
    use std::collections::HashMap;
    use std::sync::Arc;

    use arrow_array::{
        Array as _, ArrayRef, FixedSizeListArray, Float32Array, RecordBatch, UInt32Array,
    };
    use arrow_schema::{DataType, Field, Schema};

    let time_ids = Arc::new(UInt32Array::from_iter_values([0_u32])) as ArrayRef;
    let values = Arc::new(Float32Array::from(vec![1.0_f32, 2.0])) as ArrayRef;
    let value_field = Arc::new(Field::new("item", DataType::Float32, false));
    let list_array = FixedSizeListArray::try_new(value_field, 2, values, None)
        .expect("valid fixed-size list array");

    let mut metadata = HashMap::new();
    metadata.insert("arcadia_tio_dim_lens".to_string(), "1,3".to_string());
    metadata.insert("arcadia_tio_order".to_string(), "row-major".to_string());
    let schema = Schema::new_with_metadata(
        vec![
            Field::new("time_id", DataType::UInt32, false),
            Field::new("values", list_array.data_type().clone(), false),
        ],
        metadata,
    );
    let batch = RecordBatch::try_new(
        Arc::new(schema),
        vec![time_ids, Arc::new(list_array) as ArrayRef],
    )
    .expect("valid record batch with intentionally mismatched metadata");

    let err = Tensor::from_arrow_record_batch(&batch).expect_err("metadata mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[cfg(feature = "arrow")]
#[test]
fn tensor_arrow_record_batch_rejects_zero_width_inner_shape() {
    let tensor = Tensor::from_dense_f32(vec![2, 0], Vec::new()).expect("zero-width tensor");
    let err = tensor
        .to_arrow_record_batch()
        .expect_err("Arrow FixedSizeList companion layout requires positive row width");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[cfg(feature = "csv")]
#[test]
fn tensor_csv_roundtrips_supported_dtypes_and_metadata() {
    let f32_tensor =
        Tensor::from_dense_f32(vec![2, 2], vec![1.0, 2.5, 3.25, 4.75]).expect("valid f32 tensor");
    let csv = f32_tensor.to_csv_string().expect("f32 tensor to CSV");
    assert!(csv.starts_with("record,dtype,shape,order,flat_index,value\n"));
    assert!(csv.contains("metadata,f32,2x2,row-major,,\n"));
    assert!(csv.contains("value,,,,0,1"));
    assert_eq!(
        Tensor::from_csv_str(&csv).expect("decode f32 CSV string"),
        f32_tensor
    );
    assert_eq!(
        Tensor::from_csv_bytes(&f32_tensor.to_csv_bytes().expect("f32 CSV bytes"))
            .expect("decode f32 CSV bytes"),
        f32_tensor
    );

    let f64_tensor =
        Tensor::from_dense_f64(vec![1, 2, 2], vec![1.5, 2.5, 3.5, 4.5]).expect("valid f64 tensor");
    assert_eq!(
        Tensor::from_csv_str(&f64_tensor.to_csv_string().expect("f64 CSV"))
            .expect("decode f64 CSV"),
        f64_tensor
    );

    let i32_tensor =
        Tensor::from_dense_i32(vec![4], vec![10, 20, 30, 40]).expect("valid i32 tensor");
    assert_eq!(
        Tensor::from_csv_bytes(&i32_tensor.to_csv_bytes().expect("i32 CSV bytes"))
            .expect("decode i32 CSV"),
        i32_tensor
    );

    let i64_tensor =
        Tensor::from_dense_i64(vec![2, 2], vec![100, 200, 300, 400]).expect("valid i64 tensor");
    assert_eq!(
        Tensor::from_csv_str(&i64_tensor.to_csv_string().expect("i64 CSV"))
            .expect("decode i64 CSV"),
        i64_tensor
    );

    let empty_inner =
        Tensor::from_dense_i64(vec![2, 0], Vec::new()).expect("zero-element shape remains valid");
    let empty_csv = empty_inner.to_csv_string().expect("empty tensor CSV");
    assert!(empty_csv.contains("metadata,i64,2x0,row-major,,\n"));
    assert_eq!(
        Tensor::from_csv_str(&empty_csv).expect("decode empty tensor CSV"),
        empty_inner
    );
}

#[cfg(feature = "csv")]
#[test]
fn tensor_csv_rejects_malformed_input() {
    let err = Tensor::from_csv_str("record,dtype,shape,order,flat_index,value\n")
        .expect_err("missing metadata rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,u32,2,row-major,,\n",
    )
    .expect_err("invalid dtype rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,f32,2x,row-major,,\n",
    )
    .expect_err("invalid shape rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,i32,2,column-major,,\n",
    )
    .expect_err("invalid order rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,i32,2,row-major,,\nvalue,,,,1,10\n",
    )
    .expect_err("out-of-order index rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,i32,2,row-major,,\nvalue,,,,0,10\n",
    )
    .expect_err("value count mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = Tensor::from_csv_str(
        "record,dtype,shape,order,flat_index,value\nmetadata,i32,1,row-major,,\nvalue,,,,0,not_an_i32\n",
    )
    .expect_err("scalar parse error rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[cfg(feature = "parquet")]
#[test]
fn tensor_parquet_roundtrips_supported_dtypes_metadata_and_files() {
    let f32_tensor =
        Tensor::from_dense_f32(vec![2, 2], vec![1.0, 2.5, 3.25, 4.75]).expect("valid f32 tensor");
    let f32_bytes = f32_tensor
        .to_parquet_bytes()
        .expect("f32 tensor to Parquet");
    assert!(f32_bytes.starts_with(b"PAR1"));
    assert!(f32_bytes.ends_with(b"PAR1"));
    assert_eq!(
        Tensor::from_parquet_bytes(&f32_bytes).expect("decode f32 Parquet bytes"),
        f32_tensor
    );

    let f64_tensor =
        Tensor::from_dense_f64(vec![1, 2, 2], vec![1.5, 2.5, 3.5, 4.5]).expect("valid f64 tensor");
    assert_eq!(
        Tensor::from_parquet_bytes(&f64_tensor.to_parquet_bytes().expect("f64 Parquet"))
            .expect("decode f64 Parquet"),
        f64_tensor
    );

    let i32_tensor =
        Tensor::from_dense_i32(vec![4], vec![10, 20, 30, 40]).expect("valid i32 tensor");
    assert_eq!(
        Tensor::from_parquet_bytes(&i32_tensor.to_parquet_bytes().expect("i32 Parquet"))
            .expect("decode i32 Parquet"),
        i32_tensor
    );

    let i64_tensor =
        Tensor::from_dense_i64(vec![2, 2], vec![100, 200, 300, 400]).expect("valid i64 tensor");
    let path = parquet_local_unique_path("tensor-parquet-roundtrip.parquet");
    i64_tensor
        .to_parquet_file(&path)
        .expect("write i64 Parquet file");
    assert_eq!(
        Tensor::from_parquet_file(&path).expect("decode i64 Parquet file"),
        i64_tensor
    );
    let _ = fs::remove_file(path);

    let empty_inner =
        Tensor::from_dense_i64(vec![2, 0], Vec::new()).expect("zero-element shape remains valid");
    assert_eq!(
        Tensor::from_parquet_bytes(
            &empty_inner
                .to_parquet_bytes()
                .expect("empty tensor Parquet bytes")
        )
        .expect("decode empty tensor Parquet"),
        empty_inner
    );
}

#[cfg(feature = "parquet")]
#[test]
fn tensor_parquet_rejects_malformed_and_unsupported_input() {
    let err = Tensor::from_parquet_bytes(b"not a parquet file")
        .expect_err("malformed Parquet bytes reject");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let missing_metadata = parquet_missing_metadata_bytes();
    let err = Tensor::from_parquet_bytes(&missing_metadata).expect_err("missing metadata rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let out_of_order = parquet_i32_bytes_with_indices(&[1, 0], &[10, 20]);
    let err = Tensor::from_parquet_bytes(&out_of_order).expect_err("out-of-order indices reject");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let schema_mismatch = parquet_i64_value_bytes_with_i32_metadata();
    let err = Tensor::from_parquet_bytes(&schema_mismatch).expect_err("schema mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[cfg(feature = "parquet")]
fn parquet_local_unique_path(name: &str) -> PathBuf {
    let dir = PathBuf::from(".tmp").join("arcadia-tio-rs-tests");
    fs::create_dir_all(&dir).expect("create project-local test temp directory");
    dir.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        unique_counter()
    ))
}

#[cfg(feature = "parquet")]
fn parquet_missing_metadata_bytes() -> Vec<u8> {
    let schema = std::sync::Arc::new(
        parquet::schema::parser::parse_message_type(
            "message arcadia_tio_tensor { REQUIRED INT64 flat_index; REQUIRED INT32 value; }",
        )
        .expect("test Parquet schema"),
    );
    let mut out = Vec::new();
    {
        let writer =
            parquet::file::writer::SerializedFileWriter::new(&mut out, schema, Default::default())
                .expect("test Parquet writer");
        writer.close().expect("close missing-metadata Parquet");
    }
    out
}

#[cfg(feature = "parquet")]
fn parquet_i32_bytes_with_indices(indices: &[i64], values: &[i32]) -> Vec<u8> {
    assert_eq!(indices.len(), values.len());
    let schema = std::sync::Arc::new(
        parquet::schema::parser::parse_message_type(
            "message arcadia_tio_tensor { REQUIRED INT64 flat_index; REQUIRED INT32 value; }",
        )
        .expect("test Parquet schema"),
    );
    let props = std::sync::Arc::new(
        parquet::file::properties::WriterProperties::builder()
            .set_key_value_metadata(Some(parquet_test_metadata(
                "i32",
                &values.len().to_string(),
            )))
            .build(),
    );
    let mut out = Vec::new();
    {
        let mut writer = parquet::file::writer::SerializedFileWriter::new(&mut out, schema, props)
            .expect("test Parquet writer");
        let mut row_group = writer.next_row_group().expect("test row group");
        let mut flat_column = row_group
            .next_column()
            .expect("flat column result")
            .expect("flat column");
        assert_eq!(
            flat_column
                .typed::<parquet::data_type::Int64Type>()
                .write_batch(indices, None, None)
                .expect("write flat indices"),
            indices.len()
        );
        flat_column.close().expect("close flat column");
        let mut value_column = row_group
            .next_column()
            .expect("value column result")
            .expect("value column");
        assert_eq!(
            value_column
                .typed::<parquet::data_type::Int32Type>()
                .write_batch(values, None, None)
                .expect("write values"),
            values.len()
        );
        value_column.close().expect("close value column");
        row_group.close().expect("close row group");
        writer.close().expect("close indexed Parquet");
    }
    out
}

#[cfg(feature = "parquet")]
fn parquet_i64_value_bytes_with_i32_metadata() -> Vec<u8> {
    let schema = std::sync::Arc::new(
        parquet::schema::parser::parse_message_type(
            "message arcadia_tio_tensor { REQUIRED INT64 flat_index; REQUIRED INT64 value; }",
        )
        .expect("test Parquet schema"),
    );
    let props = std::sync::Arc::new(
        parquet::file::properties::WriterProperties::builder()
            .set_key_value_metadata(Some(parquet_test_metadata("i32", "1")))
            .build(),
    );
    let mut out = Vec::new();
    {
        let mut writer = parquet::file::writer::SerializedFileWriter::new(&mut out, schema, props)
            .expect("test Parquet writer");
        let mut row_group = writer.next_row_group().expect("test row group");
        let mut flat_column = row_group
            .next_column()
            .expect("flat column result")
            .expect("flat column");
        assert_eq!(
            flat_column
                .typed::<parquet::data_type::Int64Type>()
                .write_batch(&[0_i64], None, None)
                .expect("write flat index"),
            1
        );
        flat_column.close().expect("close flat column");
        let mut value_column = row_group
            .next_column()
            .expect("value column result")
            .expect("value column");
        assert_eq!(
            value_column
                .typed::<parquet::data_type::Int64Type>()
                .write_batch(&[10_i64], None, None)
                .expect("write i64 value"),
            1
        );
        value_column.close().expect("close value column");
        row_group.close().expect("close row group");
        writer.close().expect("close schema-mismatch Parquet");
    }
    out
}

#[cfg(feature = "parquet")]
fn parquet_test_metadata(dtype: &str, shape: &str) -> Vec<parquet::file::metadata::KeyValue> {
    vec![
        parquet::file::metadata::KeyValue::new(
            "arcadia_tio_format".to_string(),
            "arcadia_tio_tensor_parquet_v1".to_string(),
        ),
        parquet::file::metadata::KeyValue::new("arcadia_tio_dtype".to_string(), dtype.to_string()),
        parquet::file::metadata::KeyValue::new("arcadia_tio_shape".to_string(), shape.to_string()),
        parquet::file::metadata::KeyValue::new(
            "arcadia_tio_order".to_string(),
            "row-major".to_string(),
        ),
    ]
}

#[cfg(feature = "ndarray")]
#[test]
fn tensor_ndarray_roundtrips_supported_dtypes() {
    let f32_tensor = Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .expect("valid f32 tensor");
    let f32_array = f32_tensor.to_ndarray_f32().expect("f32 tensor to ndarray");
    assert_eq!(f32_array.shape(), &[2_usize, 3]);
    assert_eq!(
        f32_array.iter().copied().collect::<Vec<_>>(),
        vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]
    );
    assert_eq!(
        Tensor::from_ndarray_f32(f32_array).expect("f32 ndarray to tensor"),
        f32_tensor
    );

    let f64_array =
        ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&[1_usize, 2, 2]), vec![1.5, 2.5, 3.5, 4.5])
            .expect("valid f64 ndarray");
    let f64_tensor = Tensor::from_ndarray_f64(f64_array).expect("f64 ndarray to tensor");
    assert_eq!(f64_tensor.shape, vec![1, 2, 2]);
    assert_eq!(f64_tensor.data, TensorData::F64(vec![1.5, 2.5, 3.5, 4.5]));
    assert_eq!(
        f64_tensor
            .to_ndarray_f64()
            .expect("f64 tensor to ndarray")
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        vec![1.5, 2.5, 3.5, 4.5]
    );

    let i32_tensor =
        Tensor::from_dense_i32(vec![4], vec![10, 20, 30, 40]).expect("valid i32 tensor");
    assert_eq!(
        Tensor::from_ndarray_i32(i32_tensor.to_ndarray_i32().expect("i32 tensor to ndarray"))
            .expect("i32 ndarray to tensor"),
        i32_tensor
    );

    let i64_tensor =
        Tensor::from_dense_i64(vec![2, 2], vec![100, 200, 300, 400]).expect("valid i64 tensor");
    assert_eq!(
        Tensor::from_ndarray_i64(i64_tensor.to_ndarray_i64().expect("i64 tensor to ndarray"))
            .expect("i64 ndarray to tensor"),
        i64_tensor
    );
}

#[cfg(feature = "ndarray")]
#[test]
fn tensor_ndarray_rejects_dtype_shape_and_rank_mismatches() {
    let dtype_mismatch = Tensor {
        dtype: DType::F64,
        shape: vec![2],
        data: TensorData::F32(vec![1.0, 2.0]),
    };
    let err = dtype_mismatch
        .to_ndarray_f32()
        .expect_err("dtype mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let shape_mismatch = Tensor {
        dtype: DType::F32,
        shape: vec![2, 2],
        data: TensorData::F32(vec![1.0, 2.0]),
    };
    let err = shape_mismatch
        .to_ndarray_f32()
        .expect_err("shape mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let scalar = ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&[]), vec![1.0_f32])
        .expect("valid ndarray scalar");
    let err = Tensor::from_ndarray_f32(scalar).expect_err("rank-0 ndarray rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[test]
fn safe_wrapper_compression_option_roundtrips_f32() {
    let path = unique_path("safe-wrapper-compressed-f32.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 32),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.compression = Some(CompressionConfig::try_zstd_level(5).expect("valid zstd level"));
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
fn safe_wrapper_uncompressed_compression_option_roundtrips_f32() {
    let path = unique_path("safe-wrapper-uncompressed-f32.tio");
    let dims = vec![
        DimSpec::new(AxisKind::Time, 0),
        DimSpec::new(AxisKind::Symbol, 4),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.compression = Some(CompressionConfig::uncompressed());
    let values = vec![1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    {
        let mut file =
            TensorFile::create(&path, options).expect("create uncompressed wrapper file");
        let range = file
            .append_f32(&values, &[2, 4])
            .expect("append uncompressed wrapper values");
        assert_eq!((range.start, range.end), (0, 2));
    }
    let file = TensorFile::open(&path).expect("open uncompressed wrapper file");
    let tensor = file.read_all().expect("read uncompressed wrapper values");
    assert_eq!(tensor.dtype, DType::F32);
    assert_eq!(tensor.shape, vec![2, 4]);
    assert_eq!(tensor.data, TensorData::F32(values));
    drop(file);
    let _ = fs::remove_file(path);
}

#[test]
fn safe_wrapper_compression_public_builders_and_raw_compatibility() {
    let default_auto = CompressionConfig::auto_zstd();
    assert_eq!(
        default_auto.mode().expect("default auto mode"),
        CompressionMode::Auto
    );
    assert_eq!(
        default_auto.codec().expect("default auto codec"),
        CompressionCodec::Zstd
    );
    assert_eq!(
        default_auto.min_payload_bytes,
        CompressionConfig::DEFAULT_MIN_PAYLOAD_BYTES
    );
    assert_eq!(
        default_auto.zstd_level,
        CompressionConfig::DEFAULT_ZSTD_LEVEL
    );

    let config = CompressionConfig::auto_zstd_min_payload(1024)
        .with_codec(CompressionCodec::Zstd)
        .try_with_zstd_level(5)
        .expect("valid Auto/Zstd config");
    assert_eq!(config.mode().expect("safe mode"), CompressionMode::Auto);
    assert_eq!(config.codec().expect("safe codec"), CompressionCodec::Zstd);
    assert_eq!(config.min_payload_bytes, 1024);
    assert_eq!(config.zstd_level, 5);

    let raw = config.try_to_raw().expect("validated raw config");
    assert_eq!(raw.mode, CompressionMode::Auto.to_raw());
    assert_eq!(raw.codec, CompressionCodec::Zstd.to_raw());
    assert_eq!(
        CompressionConfig::from_raw(raw).expect("raw roundtrip"),
        config
    );

    let raw_field_config = CompressionConfig {
        mode: CompressionMode::ForceOn.to_raw(),
        codec: CompressionCodec::Zstd.to_raw(),
        min_payload_bytes: 0,
        zstd_level: 4,
    };
    assert_eq!(
        raw_field_config
            .validate()
            .expect("raw fields remain compatible")
            .mode()
            .expect("safe mode"),
        CompressionMode::ForceOn
    );

    assert_eq!(
        CompressionConfig::try_zstd_level(CompressionConfig::ZSTD_MAX_LEVEL + 1)
            .expect_err("invalid zstd level rejects early")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        CompressionConfig {
            mode: 99,
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes: 0,
            zstd_level: CompressionConfig::DEFAULT_ZSTD_LEVEL,
        }
        .validate()
        .expect_err("invalid raw mode rejects")
        .code(),
        ErrorCode::InvalidArgument
    );
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
