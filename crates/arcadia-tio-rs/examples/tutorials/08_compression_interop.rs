//! Public Rust compression controls and interop tutorial.
//!
//! Compression settings here demonstrate wrapper API controls only. The Arrow C
//! Data and read-index examples are bounded interoperability surfaces; they are
//! not performance, storage-ratio, source-secrecy, or generic zero-copy claims.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AxisKind, CompressionConfig, CreateOptions, DType, DimSpec, ReadIndexItem,
    ReadIndexLoweringKind, TensorData, TensorFile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("compression_interop")?;

    let default_path = temp.path().join("default_then_zstd.tio");
    let uncompressed_path = temp.path().join("explicit_uncompressed.tio");

    demonstrate_compression_controls(&default_path, &uncompressed_path)?;
    demonstrate_interop_surfaces(&default_path)?;

    println!(
        "compression/interop ok: tiny files written under {}",
        temp.path().display()
    );
    Ok(())
}

fn demonstrate_compression_controls(
    default_path: &Path,
    uncompressed_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut default_options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    // Leaving `compression` as None asks native to use its persisted default
    // policy for future writes. The example validates readability only.
    default_options.compression = None;
    let mut file = TensorFile::create(default_path, default_options)?;
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;

    // The safe wrapper can also set a write-forward override for later appends.
    file.set_compression(CompressionConfig::zstd_level(3))?;
    file.append_f32(&[5.0, 6.0], &[1, 2])?;
    assert_eq!(file.read_all()?.shape, vec![3, 2]);
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    );

    let mut explicit_options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    explicit_options.compression = Some(CompressionConfig::uncompressed());
    let mut uncompressed = TensorFile::create(uncompressed_path, explicit_options)?;
    uncompressed.append_f64(&[10.0, 11.0, 12.0, 13.0], &[2, 2])?;
    assert_eq!(
        uncompressed.read_all()?.data,
        TensorData::F64(vec![10.0, 11.0, 12.0, 13.0])
    );

    Ok(())
}

fn demonstrate_interop_surfaces(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let file = TensorFile::open(path)?;

    let indexed = file.read_index(&[
        ReadIndexItem::slice(Some(0), Some(3), 2)?,
        ReadIndexItem::all(),
    ])?;
    assert_eq!(
        indexed.report.lowering_kind,
        ReadIndexLoweringKind::SelectorRead
    );
    assert!(!indexed.report.used_full_tensor_fallback);
    assert_eq!(indexed.value.shape, vec![2, 2]);
    assert_eq!(
        indexed.value.data,
        TensorData::F32(vec![1.0, 2.0, 5.0, 6.0])
    );

    {
        let arrow = file.read_values_arrow()?;
        assert_eq!(arrow.array().length, 3);
        assert_eq!(arrow.array().n_children, 1);
        assert!(arrow.array().release.is_some());
        assert!(arrow.schema().release.is_some());
        assert!(!arrow.array_ptr().is_null());
        assert!(!arrow.schema_ptr().is_null());
    }

    // Dropping the RAII Arrow owner releases native callbacks. Ordinary reads
    // remain available afterward.
    assert_eq!(file.read_all()?.shape, vec![3, 2]);

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
