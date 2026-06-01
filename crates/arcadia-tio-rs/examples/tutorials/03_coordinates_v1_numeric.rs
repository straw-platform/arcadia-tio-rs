//! Public Rust numeric Coordinate v1 tutorial.
//!
//! Coordinate v1 metadata is axis metadata. This example uses tiny integer date
//! encodings, reads descriptor/value metadata, performs exact/range lookup, and
//! composes lookup helpers with ordinary reads. The integer values are not a
//! timezone, calendar, or session semantics claim.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AxisKind, CoordinateDType, CoordinateEncoding, CoordinateKind, CoordinateMonotonicity,
    CoordinateOrdering, CoordinateSortedness, CoordinateSpec, CoordinateStorage,
    CoordinateStorageKind, CoordinateUniqueness, CoordinateValidationStatus, CoordinateValues,
    CreateOptions, DType, DimSpec, ErrorCode, TensorData, TensorFile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("coordinates_v1_numeric")?;
    let path = temp.path().join("coordinates_v1_numeric.tio");

    let mut options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 3).with_name("symbol"),
        ],
        0,
    );
    options.coordinates.push(CoordinateSpec {
        axis: 1,
        name: Some("calendar_date".to_string()),
        kind: CoordinateKind::Date,
        encoding: CoordinateEncoding::DateYyyymmdd,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![
            20260514, 20260515, 20260516,
        ])),
        ordering: CoordinateOrdering {
            sorted: CoordinateSortedness::Ascending,
            monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
            uniqueness: CoordinateUniqueness::Unique,
        },
        required: true,
    });

    let mut file = TensorFile::create(&path, options)?;
    let appended = file.append_f64(&[101.0, 102.0, 103.0, 201.0, 202.0, 203.0], &[2, 3])?;
    assert_eq!((appended.start, appended.end), (0, 2));

    let descriptors = file.coordinate_meta()?;
    assert_eq!(descriptors.len(), 1);
    assert_eq!(descriptors[0].axis, 1);
    assert_eq!(descriptors[0].name.as_deref(), Some("calendar_date"));
    assert_eq!(descriptors[0].kind, CoordinateKind::Date);
    assert_eq!(descriptors[0].dtype, CoordinateDType::I32);
    assert_eq!(descriptors[0].encoding, CoordinateEncoding::DateYyyymmdd);
    assert_eq!(descriptors[0].storage_kind, CoordinateStorageKind::Inline);
    assert_eq!(
        descriptors[0].validation_status,
        CoordinateValidationStatus::Validated
    );

    let coordinate_values = file.read_axis_coordinates(1)?;
    assert_eq!(coordinate_values.dtype, DType::I32);
    assert_eq!(coordinate_values.shape, vec![3]);
    assert_eq!(
        coordinate_values.data,
        TensorData::I32(vec![20260514, 20260515, 20260516])
    );

    assert_eq!(file.coordinate_index_i32(1, 20260515)?, 1);
    assert_eq!(file.coordinate_range_i32(1, 20260514, 20260516)?, 0..3);
    assert_eq!(file.coordinate_range_i32(1, 20260515, 20260516)?, 1..3);

    let exact_read = file.read_at_coordinate_i32(1, 20260516)?;
    assert_eq!(exact_read.shape, vec![2, 1]);
    assert_eq!(exact_read.data, TensorData::F64(vec![103.0, 203.0]));

    let range_read = file.read_coordinate_range_i32(1, 20260514, 20260515)?;
    assert_eq!(range_read.shape, vec![2, 2]);
    assert_eq!(
        range_read.data,
        TensorData::F64(vec![101.0, 102.0, 201.0, 202.0])
    );

    let missing = file
        .coordinate_index_i32(1, 20260520)
        .expect_err("missing date should fail");
    assert_eq!(missing.code(), ErrorCode::InvalidArgument);
    assert!(missing.message().contains("coordinate value not found"));

    println!(
        "coordinate v1 ok: descriptor/value reads and lookup-composed reads passed at {}",
        path.display()
    );
    Ok(())
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
