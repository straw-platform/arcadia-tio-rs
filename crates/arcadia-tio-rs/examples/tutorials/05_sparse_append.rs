//! Public Rust sparse-intent append tutorial.
//!
//! Sparse analysis reports the current native lowering decision for a proposed
//! append. It is a diagnostic/outcome API, not a storage-efficiency,
//! compression-ratio, capacity, layout-superiority, or performance claim.
//! Callers should keep explicit fallback/error handling around sparse-intent
//! rules that a given file/layout/dtype cannot lower sparsely.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AxisKind, CreateOptions, DType, DimSpec, ErrorCode, SparseAppendOutcome, SparseRule,
    SparseValuePredicate, TensorData, TensorFile,
};

const SHAPE: &[u64] = &[1, 4];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("sparse_append")?;

    demo_f32_zero(temp.path())?;
    demo_f64_zero(temp.path())?;
    demo_i32_zero_and_exact(temp.path())?;
    demo_i64_zero_and_exact(temp.path())?;
    demo_null_rule_boundary(temp.path())?;

    println!(
        "sparse append ok: f32/f64 zero plus i32/i64 zero/null/exact diagnostics passed in {}",
        temp.path().display()
    );
    Ok(())
}

fn create_sparse_file(path: &Path, dtype: DType) -> Result<TensorFile, Box<dyn std::error::Error>> {
    let options = CreateOptions::random_access(
        dtype,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Symbol, 4).with_name("symbol"),
        ],
        0,
    );
    Ok(TensorFile::create(path, options)?)
}

fn demo_f32_zero(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = create_sparse_file(&root.join("f32_zero.tio"), DType::F32)?;
    let values = [11.0_f32, 0.0, 13.0, 0.0];
    let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
    let analysis = file.analyze_sparse_append_f32(&values, SHAPE, &rule)?;
    assert_eq!(analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(analysis.absent_subtensor_count, 2);
    assert_eq!(analysis.total_subtensor_count, 4);

    let range = file.append_sparse_f32_with_range(&values, SHAPE, &rule)?;
    assert_eq!((range.start, range.end), (0, 1));
    let dense = file.read_all_dense(-1.0)?;
    assert_eq!(dense.tensor.dtype, DType::F32);
    assert_eq!(dense.tensor.shape, vec![1, 4]);
    assert_eq!(
        dense.tensor.data,
        TensorData::F32(vec![11.0, -1.0, 13.0, -1.0])
    );
    assert_eq!(dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));
    Ok(())
}

fn demo_f64_zero(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut file = create_sparse_file(&root.join("f64_zero.tio"), DType::F64)?;
    let values = [101.0_f64, 0.0, 103.0, 0.0];
    let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
    let analysis = file.analyze_sparse_append_f64(&values, SHAPE, &rule)?;
    assert_eq!(analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(analysis.absent_subtensor_count, 2);
    assert_eq!(analysis.total_subtensor_count, 4);

    let range = file.append_sparse_f64_with_range(&values, SHAPE, &rule)?;
    assert_eq!((range.start, range.end), (0, 1));
    let dense = file.read_all_dense(-1.0)?;
    assert_eq!(dense.tensor.dtype, DType::F64);
    assert_eq!(dense.tensor.shape, vec![1, 4]);
    assert_eq!(
        dense.tensor.data,
        TensorData::F64(vec![101.0, -1.0, 103.0, -1.0])
    );
    assert_eq!(dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));
    Ok(())
}

fn demo_i32_zero_and_exact(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut zero_file = create_sparse_file(&root.join("i32_zero.tio"), DType::I32)?;
    let zero_values = [21_i32, 0, 23, 0];
    let zero_rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
    let zero_analysis = zero_file.analyze_sparse_append_i32(&zero_values, SHAPE, &zero_rule)?;
    assert_eq!(zero_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(zero_analysis.absent_subtensor_count, 2);
    let zero_range = zero_file.append_sparse_i32(&zero_values, SHAPE, &zero_rule)?;
    assert_eq!((zero_range.start, zero_range.end), (0, 1));
    let zero_dense = zero_file.read_all_dense(-1.0)?;
    assert_eq!(zero_dense.tensor.dtype, DType::I32);
    assert_eq!(
        zero_dense.tensor.data,
        TensorData::I32(vec![21, -1, 23, -1])
    );
    assert_eq!(zero_dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

    let mut exact_file = create_sparse_file(&root.join("i32_exact.tio"), DType::I32)?;
    let exact_values = [41_i32, -7, 43, -7];
    let exact_rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI32(-7));
    let exact_analysis = exact_file.analyze_sparse_append_i32(&exact_values, SHAPE, &exact_rule)?;
    assert_eq!(exact_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(exact_analysis.absent_subtensor_count, 2);
    let exact_range = exact_file.append_sparse_i32(&exact_values, SHAPE, &exact_rule)?;
    assert_eq!((exact_range.start, exact_range.end), (0, 1));
    let exact_dense = exact_file.read_all_dense(-1.0)?;
    assert_eq!(
        exact_dense.tensor.data,
        TensorData::I32(vec![41, -1, 43, -1])
    );
    assert_eq!(exact_dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

    let mismatch = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI64(-7));
    let err = exact_file
        .analyze_sparse_append_i32(&exact_values, SHAPE, &mismatch)
        .expect_err("i32/equal_i64 predicate should fail");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(
        err.message()
            .contains("predicate does not match tensor dtype")
    );
    Ok(())
}

fn demo_i64_zero_and_exact(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut zero_file = create_sparse_file(&root.join("i64_zero.tio"), DType::I64)?;
    let zero_values = [201_i64, 0, 203, 0];
    let zero_rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
    let zero_analysis = zero_file.analyze_sparse_append_i64(&zero_values, SHAPE, &zero_rule)?;
    assert_eq!(zero_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(zero_analysis.absent_subtensor_count, 2);
    let zero_range = zero_file.append_sparse_i64(&zero_values, SHAPE, &zero_rule)?;
    assert_eq!((zero_range.start, zero_range.end), (0, 1));
    let zero_dense = zero_file.read_all_dense(-1.0)?;
    assert_eq!(zero_dense.tensor.dtype, DType::I64);
    assert_eq!(
        zero_dense.tensor.data,
        TensorData::I64(vec![201, -1, 203, -1])
    );
    assert_eq!(zero_dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

    let mut exact_file = create_sparse_file(&root.join("i64_exact.tio"), DType::I64)?;
    let absent = 9_007_199_254_740_993_i64;
    let exact_values = [401_i64, absent, 403, absent];
    let exact_rule =
        SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI64(absent));
    let exact_analysis = exact_file.analyze_sparse_append_i64(&exact_values, SHAPE, &exact_rule)?;
    assert_eq!(exact_analysis.outcome, SparseAppendOutcome::SparseChunkTree);
    assert_eq!(exact_analysis.absent_subtensor_count, 2);
    let exact_range = exact_file.append_sparse_i64(&exact_values, SHAPE, &exact_rule)?;
    assert_eq!((exact_range.start, exact_range.end), (0, 1));
    let exact_dense = exact_file.read_all_dense(-1.0)?;
    assert_eq!(
        exact_dense.tensor.data,
        TensorData::I64(vec![401, -1, 403, -1])
    );
    assert_eq!(exact_dense.mask.as_deref(), Some(&[1, 0, 1, 0][..]));

    let mismatch = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::EqualI32(0));
    let err = exact_file
        .append_sparse_i64(&exact_values, SHAPE, &mismatch)
        .expect_err("i64/equal_i32 predicate should fail");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(
        err.message()
            .contains("predicate does not match tensor dtype")
    );
    Ok(())
}

fn demo_null_rule_boundary(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Plain typed Rust slices have no nullable bitmap. A NullSubtensor rule is
    // still explicit API input, but for these dense numeric slices it detects no
    // absent subtensors and appends densely to preserve exact values.
    let null_rule = SparseRule::null_subtensor(vec![1]);

    let mut i32_file = create_sparse_file(&root.join("i32_null_rule_dense_view.tio"), DType::I32)?;
    let i32_values = [31_i32, 32, 33, 34];
    let i32_analysis = i32_file.analyze_sparse_append_i32(&i32_values, SHAPE, &null_rule)?;
    assert_eq!(i32_analysis.outcome, SparseAppendOutcome::DenseFallback);
    assert_eq!(i32_analysis.absent_subtensor_count, 0);
    let i32_range = i32_file.append_sparse_i32(&i32_values, SHAPE, &null_rule)?;
    assert_eq!((i32_range.start, i32_range.end), (0, 1));
    assert_eq!(
        i32_file.read_all()?.data,
        TensorData::I32(i32_values.to_vec())
    );

    let mut i64_file = create_sparse_file(&root.join("i64_null_rule_dense_view.tio"), DType::I64)?;
    let i64_values = [301_i64, 302, 303, 304];
    let i64_analysis = i64_file.analyze_sparse_append_i64(&i64_values, SHAPE, &null_rule)?;
    assert_eq!(i64_analysis.outcome, SparseAppendOutcome::DenseFallback);
    assert_eq!(i64_analysis.absent_subtensor_count, 0);
    let i64_range = i64_file.append_sparse_i64(&i64_values, SHAPE, &null_rule)?;
    assert_eq!((i64_range.start, i64_range.end), (0, 1));
    assert_eq!(
        i64_file.read_all()?.data,
        TensorData::I64(i64_values.to_vec())
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
