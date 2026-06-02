//! Public Rust layouts/create and read/history walkthrough.
//!
//! This first half demonstrates the safe wrapper create entry points for tiny
//! streaming, random-access, bounded policy, and inferred-layout files. The
//! read/history half below covers selectors, read options, shape policies,
//! dense fills/masks, and retained historical reads.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AxisKind, CreateInferredOptions, CreateOptions, CreatePolicyOptions, DType, DimSpec,
    EntrySelector, HistoricalReadWithOptions, HistoricalReadWithShapePolicyOptions,
    ReadShapePolicy, ReadWithOptions, ReadWithShapePolicyOptions, StorageAccessKind, TensorData,
    TensorFile,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use one isolated directory and create dedicated files for each layout/read
    // path to keep assertions independent and easy to reason about.
    let temp = TutorialTempDir::new("layouts_reads_history")?;

    create_streaming(&temp.path().join("streaming.tio"))?;
    create_random_access(&temp.path().join("random_access.tio"))?;
    create_with_bounded_policy(&temp.path().join("policy_regular_chunked.tio"))?;
    create_inferred(&temp.path().join("inferred.tio"))?;
    read_selectors_options_shape_and_history(&temp.path().join("reads_history.tio"))?;

    println!(
        "layout/create and read/history tutorial completed in {}",
        temp.path().display()
    );
    Ok(())
}

fn create_streaming(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Streaming layout: fixed schema and append-only progression.
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let mut file = TensorFile::create(path, options)?;
    let appended = file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
    assert_eq!((appended.start, appended.end), (0, 2));
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0])
    );
    Ok(())
}

fn create_random_access(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Random-access layout: supports non-monotonic read patterns and random
    // slice revisits while preserving same rank/shape contract.
    let options = CreateOptions::random_access(
        DType::I32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let mut file = TensorFile::create(path, options)?;
    file.append_i32(&[10, 20, 30, 40], &[2, 2])?;
    assert_eq!(file.read_all()?.data, TensorData::I32(vec![10, 20, 30, 40]));
    Ok(())
}

fn create_with_bounded_policy(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Bounded policy: fixed axes are chunked with explicit arity to keep page
    // geometry predictable for deterministic tests.
    let mut options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    options.symbols = vec!["AAPL".to_string(), "MSFT".to_string()];
    options.channels = vec!["open".to_string(), "close".to_string()];

    // Bounded RegularChunked policy: only fixed axes are chunked, and each
    // typical query extent is tiny and deterministic.
    let policy = CreatePolicyOptions::new(vec![1, 2], vec![0, 2, 2]);
    let mut file = TensorFile::create_with_policy(path, options, policy)?;
    file.append_f32(&[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0], &[2, 2, 2])?;
    assert_eq!(file.dim_lens()?, vec![2, 2, 2]);
    Ok(())
}

fn create_inferred(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Inferred layout lets storage hinting and access behavior be selected from
    // runtime policy while still asserting the logical tensor shape.
    let options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 2).with_name("symbol"),
        ],
        0,
    );
    let mut hints = CreateInferredOptions::new();
    hints.storage_access = StorageAccessKind::RemoteRangeRead;

    let mut file = TensorFile::create_inferred(path, options, hints)?;
    file.append_f64(&[9.0, 10.0], &[1, 2])?;
    assert_eq!(file.read_all()?.data, TensorData::F64(vec![9.0, 10.0]));
    Ok(())
}

fn read_selectors_options_shape_and_history(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Exercise selector-based reads and all three history/shape-policy families.
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let mut file = TensorFile::create(path, options)?;
    file.append_f32(&[1.0, 2.0, 3.0, 4.0], &[2, 2])?;
    // Capture sequential commit IDs and then create divergence via append.
    let first_commit = file.head_commit()?.commit_seq;
    file.append_f32(&[5.0, 6.0], &[1, 2])?;
    let second_commit = file.head_commit()?.commit_seq;
    assert!(second_commit > first_commit);

    // Selector read with explicit parallelism demonstrates request-scoped options.
    let selected = file.read_with_options(
        &[
            EntrySelector::Range { start: 1, end: 3 },
            EntrySelector::All,
        ],
        ReadWithOptions::parallel_threads(2),
    )?;
    assert_eq!(selected.value.shape, vec![2, 2]);
    assert_eq!(
        selected.value.data,
        TensorData::F32(vec![3.0, 4.0, 5.0, 6.0])
    );
    assert_eq!(selected.execution.query_max_threads, 2);

    // Dense shape-policy read exercises dense fill behavior for an explicit shape.
    let explicit = file.read_with_shape_policy_dense(
        &[],
        ReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitExtents(vec![3])),
        -1.0,
    )?;
    assert_eq!(explicit.value.tensor.shape, vec![3, 3]);
    assert_eq!(
        explicit.value.tensor.data,
        TensorData::F32(vec![1.0, 2.0, -1.0, 3.0, 4.0, -1.0, 5.0, 6.0, -1.0])
    );
    if let Some(mask) = explicit.value.mask.as_deref() {
        assert_eq!(mask, &[1, 1, 0, 1, 1, 0, 1, 1, 0]);
    }

    // Verify historic visibility while still asserting ordering and retention.
    let history = file.list_commits(None)?;
    assert!(
        history
            .iter()
            .any(|commit| commit.commit_seq == first_commit)
    );
    let historical =
        file.read_at_commit_with_options(first_commit, &[], HistoricalReadWithOptions::serial())?;
    assert_eq!(historical.value.shape, vec![2, 2]);
    assert_eq!(
        historical.value.data,
        TensorData::F32(vec![1.0, 2.0, 3.0, 4.0])
    );
    assert_eq!(historical.execution.query_commit_seq, first_commit);

    let historical_dense = file.read_at_commit_with_shape_policy_dense(
        first_commit,
        &[],
        HistoricalReadWithShapePolicyOptions::serial(ReadShapePolicy::ExplicitExtents(vec![3])),
        -1.0,
    )?;
    assert_eq!(historical_dense.value.tensor.shape, vec![2, 3]);
    assert_eq!(
        historical_dense.value.tensor.data,
        TensorData::F32(vec![1.0, 2.0, -1.0, 3.0, 4.0, -1.0])
    );
    if let Some(mask) = historical_dense.value.mask.as_deref() {
        assert_eq!(mask, &[1, 1, 0, 1, 1, 0]);
    }

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
