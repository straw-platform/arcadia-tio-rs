//! Public Rust Coordinate v2 first-surface tutorial.
//!
//! This example demonstrates the bounded public Rust Coordinate v2 surface:
//! inline numeric values, fixed-width ASCII text, dictionary codes, unavailable
//! external-reference summaries, visible lookup status records, append-axis
//! coordinates, and no-partial-publication failures. It does not dereference
//! external references, infer variable-length string semantics, extend
//! dictionaries during append, or treat optional indexes as coordinate truth.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AppendCoordinateBatchV2, AppendCoordinateEntryV2, AxisCoordinateInputV2, AxisKind,
    CoordinateAvailabilityV2, CoordinateCodeDTypeV2, CoordinateDType, CoordinateDictionaryEntryV2,
    CoordinateDictionarySummaryV2, CoordinateEncoding, CoordinateExternalBindingV2,
    CoordinateFixedTextLayoutV2, CoordinateKind, CoordinateLookupKeyV2,
    CoordinateLookupResultStatusV2, CoordinateMonotonicity, CoordinateOrdering,
    CoordinateSourceKindV2, CoordinateStatusCategoryV2, CoordinateUniqueness, CoordinateV2Options,
    CoordinateValueDomainV2, CreateOptions, DType, DimSpec, ErrorCode, TensorData, TensorFile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("coordinates_v2_full_surface")?;

    demo_numeric_and_fixed_text(temp.path())?;
    demo_dictionary_codes(temp.path())?;
    demo_external_unavailable(temp.path())?;
    demo_append_coordinates_and_atomic_failures(temp.path())?;

    println!(
        "coordinate v2 ok: metadata, lookups, external status, append coordinates, and atomic failures passed in {}",
        temp.path().display()
    );
    Ok(())
}

fn demo_numeric_and_fixed_text(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("coordinate_v2_numeric_fixed_text.tio");
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
        AxisCoordinateInputV2::inline_i32(1, vec![10, 20])
            .with_descriptor_id("symbol-id-v2")
            .with_name("symbol_id")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true)
            .with_ordering(CoordinateOrdering {
                sorted: arcadia_tio_rs::CoordinateSortedness::Ascending,
                monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
                uniqueness: CoordinateUniqueness::Unique,
            }),
        AxisCoordinateInputV2::fixed_text_ascii(2, 4, ["BID", "ASK"])?
            .with_descriptor_id("channel-code-v2")
            .with_name("channel_code")
            .with_kind(CoordinateKind::LabelId)
            .with_required(true),
    ];

    let mut file = TensorFile::create_with_coordinates_v2(
        &path,
        options,
        &coordinates,
        CoordinateV2Options::default(),
    )?;
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[1, 2, 2])?;

    let meta = file.coordinate_meta_v2()?;
    assert_eq!(meta.len(), 2);
    assert_eq!(meta[0].descriptor_id.as_deref(), Some("symbol-id-v2"));
    assert_eq!(meta[0].value_domain, CoordinateValueDomainV2::InlineNumeric);
    assert_eq!(meta[0].availability, CoordinateAvailabilityV2::Available);
    assert_eq!(meta[0].status_category, CoordinateStatusCategoryV2::Ok);
    assert_eq!(meta[1].descriptor_id.as_deref(), Some("channel-code-v2"));
    assert_eq!(meta[1].value_domain, CoordinateValueDomainV2::FixedText);
    assert_eq!(meta[1].fixed_text.width, 4);

    let numeric_values = file.read_axis_coordinates_v2(1, CoordinateV2Options::default())?;
    assert_eq!(
        numeric_values.value_domain,
        CoordinateValueDomainV2::InlineNumeric
    );
    assert_eq!(numeric_values.numeric_dtype, CoordinateDType::I32);
    assert_eq!(numeric_values.element_size, std::mem::size_of::<i32>());
    assert_eq!(numeric_values.len, 2);
    assert_eq!(numeric_values.data, i32_bytes(&[10, 20]));

    let fixed_values = file.read_axis_coordinates_v2(2, CoordinateV2Options::default())?;
    assert_eq!(
        fixed_values.value_domain,
        CoordinateValueDomainV2::FixedText
    );
    assert_eq!(fixed_values.fixed_text_width, 4);
    assert_eq!(fixed_values.len, 2);
    assert_eq!(fixed_values.data, b"BID ASK ".to_vec());

    let lookup_options = CoordinateV2Options::authoritative_scan();
    let numeric_exact =
        file.coordinate_lookup_v2(1, &CoordinateLookupKeyV2::i32(20), lookup_options)?;
    assert_eq!(numeric_exact.status, CoordinateLookupResultStatusV2::Unique);
    assert_eq!(numeric_exact.unique_position(), Some(1));

    let numeric_range = file.coordinate_lookup_range_v2(
        1,
        &CoordinateLookupKeyV2::i32(10),
        &CoordinateLookupKeyV2::i32(21),
        lookup_options,
    )?;
    assert_eq!(numeric_range.status, CoordinateLookupResultStatusV2::Range);
    assert_eq!(numeric_range.range(), Some(0..2));

    let fixed_exact = file.coordinate_lookup_v2(
        2,
        &CoordinateLookupKeyV2::fixed_text_ascii("ASK", 4)?,
        lookup_options,
    )?;
    assert_eq!(fixed_exact.status, CoordinateLookupResultStatusV2::Unique);
    assert_eq!(fixed_exact.unique_position(), Some(1));

    let fixed_range = file.coordinate_lookup_range_v2(
        2,
        &CoordinateLookupKeyV2::fixed_text_ascii("BID", 4)?,
        &CoordinateLookupKeyV2::fixed_text_ascii("BIE", 4)?,
        lookup_options,
    )?;
    assert_eq!(fixed_range.status, CoordinateLookupResultStatusV2::Range);
    assert_eq!(fixed_range.range(), Some(0..1));

    let missing = file.coordinate_lookup_v2(1, &CoordinateLookupKeyV2::i32(99), lookup_options)?;
    assert_eq!(missing.status, CoordinateLookupResultStatusV2::Missing);
    assert_eq!(missing.status_category, CoordinateStatusCategoryV2::Ok);

    let domain_mismatch =
        file.coordinate_lookup_v2(2, &CoordinateLookupKeyV2::i64(10), lookup_options)?;
    assert_eq!(
        domain_mismatch.status_category,
        CoordinateStatusCategoryV2::LookupDomainMismatch
    );
    assert!(domain_mismatch.is_error() || domain_mismatch.is_unsupported());

    Ok(())
}

fn demo_dictionary_codes(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("coordinate_v2_dictionary.tio");
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let dictionary_summary = CoordinateDictionarySummaryV2::new(CoordinateCodeDTypeV2::U16)
        .with_dictionary_id("symbol-dict-v2")
        .with_revision(7)
        .with_content_id("symbol-dict-content-v2");
    let dictionary_entries = vec![
        CoordinateDictionaryEntryV2::new(1, Some("AAPL".to_string()), Some("AAPL".to_string())),
        CoordinateDictionaryEntryV2::new(2, Some("MSFT".to_string()), Some("MSFT".to_string())),
    ];
    let coordinates = vec![
        AxisCoordinateInputV2::dictionary_codes_u16(
            1,
            vec![1, 2],
            CoordinateFixedTextLayoutV2::ascii_right_space_padded(4)?,
            dictionary_summary,
            dictionary_entries,
        )?
        .with_descriptor_id("symbol-dictionary-code-v2")
        .with_name("symbol_code")
        .with_required(true),
    ];

    let mut file = TensorFile::create_with_coordinates_v2(
        &path,
        options,
        &coordinates,
        CoordinateV2Options::default(),
    )?;
    file.append_f64(&[1.5, 2.5], &[1, 2])?;

    let meta = file.coordinate_meta_v2()?;
    assert_eq!(meta.len(), 1);
    assert_eq!(
        meta[0].value_domain,
        CoordinateValueDomainV2::DictionaryCode
    );
    assert_eq!(
        meta[0].dictionary.dictionary_id.as_deref(),
        Some("symbol-dict-v2")
    );
    assert_eq!(meta[0].dictionary.revision, 7);
    assert_eq!(meta[0].dictionary.entry_count, 2);

    let code_values = file.read_axis_coordinates_v2(1, CoordinateV2Options::default())?;
    assert_eq!(
        code_values.value_domain,
        CoordinateValueDomainV2::DictionaryCode
    );
    assert_eq!(code_values.code_dtype, CoordinateCodeDTypeV2::U16);
    assert_eq!(code_values.data, u16_bytes(&[1, 2]));

    let dictionary = file.coordinate_dictionary_v2(
        1,
        CoordinateV2Options {
            include_dictionary_entries: true,
            ..CoordinateV2Options::default()
        },
    )?;
    assert_eq!(dictionary.status_category, CoordinateStatusCategoryV2::Ok);
    assert_eq!(dictionary.entries.len(), 2);
    assert_eq!(dictionary.entries[0].stable_id.as_deref(), Some("AAPL"));
    assert_eq!(dictionary.entries[1].display_label.as_deref(), Some("MSFT"));

    let lookup_options = CoordinateV2Options::authoritative_scan();
    let code_lookup = file.coordinate_lookup_v2(
        1,
        &CoordinateLookupKeyV2::dictionary_code(2),
        lookup_options,
    )?;
    assert_eq!(code_lookup.status, CoordinateLookupResultStatusV2::Unique);
    assert_eq!(code_lookup.unique_position(), Some(1));

    let stable_lookup =
        file.coordinate_lookup_v2(1, &CoordinateLookupKeyV2::stable_id("AAPL"), lookup_options)?;
    assert_eq!(stable_lookup.status, CoordinateLookupResultStatusV2::Unique);
    assert_eq!(stable_lookup.unique_position(), Some(0));

    let label_lookup = file.coordinate_lookup_v2(
        1,
        &CoordinateLookupKeyV2::display_label("MSFT"),
        lookup_options,
    )?;
    assert_eq!(label_lookup.status, CoordinateLookupResultStatusV2::Unique);
    assert_eq!(label_lookup.unique_position(), Some(1));

    Ok(())
}

fn demo_external_unavailable(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("coordinate_v2_external_unavailable.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let external_binding = CoordinateExternalBindingV2::metadata_only(
        CoordinateSourceKindV2::SameFileObject,
        Some("tutorial_external_symbol_source".to_string()),
        Some("symbol coordinate object".to_string()),
        CoordinateValueDomainV2::FixedText,
        2,
    );
    let coordinates = vec![
        AxisCoordinateInputV2::external_reference_fixed_text(
            1,
            external_binding,
            CoordinateFixedTextLayoutV2::ascii_right_space_padded(4)?,
        )?
        .with_descriptor_id("external-symbol-v2")
        .with_name("external_symbol"),
    ];

    let mut file = TensorFile::create_with_coordinates_v2(
        &path,
        options,
        &coordinates,
        CoordinateV2Options::default(),
    )?;
    file.append_f32(&[3.0, 4.0], &[1, 2])?;

    let meta = file.coordinate_meta_v2()?;
    assert_eq!(meta.len(), 1);
    assert_eq!(
        meta[0].value_domain,
        CoordinateValueDomainV2::ExternalReference
    );
    assert_eq!(meta[0].availability, CoordinateAvailabilityV2::Unavailable);
    assert_eq!(meta[0].status_category, CoordinateStatusCategoryV2::Ok);
    assert_eq!(
        meta[0].external_binding.logical_id.as_deref(),
        Some("tutorial_external_symbol_source")
    );

    let values = file.read_axis_coordinates_v2(1, CoordinateV2Options::default())?;
    assert_ne!(values.availability, CoordinateAvailabilityV2::Available);
    assert_eq!(values.status_category, CoordinateStatusCategoryV2::Ok);
    assert!(values.data.is_empty());

    let unavailable_lookup = file.coordinate_lookup_v2(
        1,
        &CoordinateLookupKeyV2::fixed_text_ascii("AAPL", 4)?,
        CoordinateV2Options::default(),
    )?;
    assert_eq!(
        unavailable_lookup.status,
        CoordinateLookupResultStatusV2::Unavailable
    );
    assert_eq!(
        unavailable_lookup.availability,
        CoordinateAvailabilityV2::Unavailable
    );

    Ok(())
}

fn demo_append_coordinates_and_atomic_failures(
    root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root.join("coordinate_v2_append.tio");
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let coordinates = vec![
        AxisCoordinateInputV2::append_numeric_i32(0)
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

    let mut file = TensorFile::create_with_coordinates_v2(
        &path,
        options,
        &coordinates,
        CoordinateV2Options::default(),
    )?;
    let good_batch = AppendCoordinateBatchV2::new(vec![
        AppendCoordinateEntryV2::i32(0, vec![20260531, 20260601])
            .with_descriptor_id("append-day-v2")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    let range = file.append_f32_with_coordinates_v2(&[1.0, 2.0, 3.0, 4.0], &[2, 2], &good_batch)?;
    assert_eq!((range.start, range.end), (0, 2));
    assert_eq!(file.dim_lens()?, vec![2, 2]);
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0])
    );

    let before_dims = file.dim_lens()?;
    let before_payload = file.read_all()?;
    let before_meta = file.coordinate_meta_v2()?;
    let before_values = file.read_axis_coordinates_v2(0, CoordinateV2Options::default())?;

    let missing = file
        .append_f32_with_coordinates_v2(
            &[5.0, 6.0, 7.0, 8.0],
            &[2, 2],
            &AppendCoordinateBatchV2::empty(),
        )
        .expect_err("missing required append coordinate should fail");
    assert_eq!(missing.code(), ErrorCode::InvalidArgument);

    let wrong_count_batch = AppendCoordinateBatchV2::new(vec![
        AppendCoordinateEntryV2::i32(0, vec![20260602])
            .with_descriptor_id("append-day-v2")
            .with_numeric_encoding(CoordinateEncoding::DateYyyymmdd),
    ]);
    let wrong_count = file
        .append_f32_with_coordinates_v2(&[5.0, 6.0, 7.0, 8.0], &[2, 2], &wrong_count_batch)
        .expect_err("wrong-count append coordinate should fail");
    assert_eq!(wrong_count.code(), ErrorCode::InvalidArgument);

    assert_eq!(file.dim_lens()?, before_dims);
    assert_eq!(file.read_all()?, before_payload);
    assert_eq!(file.coordinate_meta_v2()?, before_meta);
    assert_eq!(
        file.read_axis_coordinates_v2(0, CoordinateV2Options::default())?,
        before_values
    );

    Ok(())
}

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

struct TutorialTempDir {
    path: PathBuf,
}

impl TutorialTempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "arcadia_tio_rust_tutorial_{label}_{}_{}",
            process::id(),
            nanos
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TutorialTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
