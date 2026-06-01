//! Public Rust quickstart: create a tiny tensor file, append values, reopen,
//! read values, and inspect metadata.
//!
//! This example uses the safe `arcadia-tio-rs` wrapper. It still links to the
//! native `arcadia_tio_capi` library at build/run time; point Cargo and the
//! platform loader at a locally built library (for example, `target/release`)
//! instead of copying native artifacts into this tutorial tree.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{AxisKind, CreateOptions, DType, DimSpec, TensorData, TensorFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("quickstart_create_append_read")?;
    let path = temp.path().join("quickstart.tio");

    let dims = vec![
        DimSpec::new(AxisKind::Time, 0).with_name("time"),
        DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
    ];
    let mut options = CreateOptions::streaming(DType::F32, dims, 0);
    options.channels = vec!["bid".to_string(), "ask".to_string()];
    options.user_kv = vec![("tutorial".to_string(), "rust quickstart".to_string())];

    {
        let mut file = TensorFile::create(&path, options)?;
        let appended = file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
        assert_eq!((appended.start, appended.end), (0, 2));
        assert_eq!(file.dim_lens()?, vec![2, 2]);
    }

    let file = TensorFile::open(&path)?;
    let values = file.read_all()?;
    assert_eq!(values.dtype, DType::F32);
    assert_eq!(values.shape, vec![2, 2]);
    assert_eq!(values.data, TensorData::F32(vec![1.0, 2.0, 3.0, 4.0]));

    let meta = TensorFile::load_meta(&path)?;
    assert_eq!(meta.dtype, DType::F32);
    assert_eq!(meta.dims[0].name.as_deref(), Some("time"));
    assert_eq!(meta.dims[1].name.as_deref(), Some("channel"));
    assert_eq!(meta.channels[0].name, "bid");
    assert_eq!(meta.channels[1].name, "ask");
    assert!(
        meta.user_kv
            .iter()
            .any(|item| { item.key == "tutorial" && item.value == "rust quickstart" })
    );

    println!(
        "created {}, read shape {:?}, commit {}",
        path.display(),
        values.shape,
        meta.commit_seq
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
