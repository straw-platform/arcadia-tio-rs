//! Public Rust owned tensor operations and optional conversion tutorial.
//!
//! This example uses the safe `arcadia-tio-rs` wrapper's Rust-owned `Tensor`
//! and `TypedTensor<T>` values. The tensor-operation helpers and optional
//! Arrow/ndarray/CSV/Parquet conversions copy data into new owned values; this
//! is API shape and interoperability coverage only, not performance, native
//! storage-format, or zero-copy evidence.
//!
//! Run with the non-default conversion features enabled, for example:
//!
//! ```sh
//! cargo run --features arrow,ndarray,csv,parquet --example tutorial_09_tensor_ops_conversions
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{DType, Tensor, TensorData, TensorF64, TensorI32, ops, typed_ops};
use ndarray::IxDyn;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Step 1: create an isolated temporary workspace for generated companion payloads.
    let temp = TutorialTempDir::new("tensor_ops_conversions")?;
    let ipc_path = temp.path().join("owned_tensor.arrow");
    let parquet_path = temp.path().join("owned_tensor.parquet");

    // Step 2: start from a tiny deterministic Rust-owned dense tensor.
    let tensor = Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])?;

    demonstrate_tensor_ops(&tensor)?;
    demonstrate_typed_wrappers(&tensor)?;
    demonstrate_arrow_and_ndarray_conversions(&tensor, &ipc_path)?;
    demonstrate_csv_and_parquet_conversions(&tensor, &parquet_path)?;

    println!(
        "tensor ops/conversions ok: temporary companion payloads written under {}",
        temp.path().display()
    );
    Ok(())
}

fn demonstrate_tensor_ops(tensor: &Tensor) -> arcadia_tio_rs::Result<()> {
    // Shape helpers materialize new owned tensors rather than borrowed native views.
    let reshaped = ops::reshape(tensor, vec![3, 2])?;
    assert_eq!(reshaped.shape, vec![3, 2]);
    assert_eq!(
        reshaped.data,
        TensorData::F64(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    );

    let transposed = ops::transpose(tensor)?;
    assert_eq!(transposed.shape, vec![3, 2]);
    assert_eq!(
        transposed.data,
        TensorData::F64(vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0])
    );

    // Index helpers use explicit axes and return owned row-major outputs.
    let first_and_last_columns = ops::take_axis(tensor, 1, &[0, 2])?;
    assert_eq!(first_and_last_columns.shape, vec![2, 2]);
    assert_eq!(
        first_and_last_columns.data,
        TensorData::F64(vec![1.0, 3.0, 4.0, 6.0])
    );

    // Assembly and reordering helpers validate shape/dtype compatibility before copying.
    let split_rows = ops::split(tensor, 0, &[1, 1])?;
    assert_eq!(split_rows.len(), 2);
    let rejoined = ops::concat(&[&split_rows[0], &split_rows[1]], 0)?;
    assert_eq!(rejoined, *tensor);

    let rolled_columns = ops::roll(tensor, 1, 1)?;
    assert_eq!(rolled_columns.shape, vec![2, 3]);
    assert_eq!(
        rolled_columns.data,
        TensorData::F64(vec![3.0, 1.0, 2.0, 6.0, 4.0, 5.0])
    );

    // Math helpers validate dtype/shape consistency and materialize new tensors.
    let shifted = ops::add_scalar(tensor, 10.0_f64)?;
    assert_eq!(shifted.values_f64()?, &[11.0, 12.0, 13.0, 14.0, 15.0, 16.0]);

    let column_offsets = Tensor::from_dense_f64(vec![1, 3], vec![100.0, 200.0, 300.0])?;
    let broadcast_sum = ops::add(tensor, &column_offsets)?;
    assert_eq!(broadcast_sum.shape, vec![2, 3]);
    assert_eq!(
        broadcast_sum.data,
        TensorData::F64(vec![101.0, 202.0, 303.0, 104.0, 205.0, 306.0])
    );

    // Reductions return owned tensors with explicit dtype behavior.
    let row_sums = ops::sum(tensor, Some(&[1]), false)?;
    assert_eq!(row_sums.shape, vec![2]);
    assert_eq!(row_sums.data, TensorData::F64(vec![6.0, 15.0]));

    let row_argmax = ops::argmax(tensor, Some(&[1]), false)?;
    assert_eq!(row_argmax.data, TensorData::I64(vec![2, 2]));

    let cumulative = ops::cumsum(tensor, Some(-1))?;
    assert_eq!(
        cumulative.data,
        TensorData::F64(vec![1.0, 3.0, 6.0, 4.0, 9.0, 15.0])
    );

    let row_var = ops::var(tensor, Some(&[1]), false)?;
    assert_eq!(row_var.data, TensorData::F64(vec![2.0 / 3.0, 2.0 / 3.0]));

    Ok(())
}

fn demonstrate_typed_wrappers(tensor: &Tensor) -> arcadia_tio_rs::Result<()> {
    // Typed wrappers enforce the expected dtype over the same owned public Tensor model.
    let typed = TensorF64::try_from(tensor.clone())?;
    assert_eq!(typed.dtype(), DType::F64);
    assert_eq!(typed.shape(), &[2, 3]);
    assert_eq!(typed.values()?, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    let typed_shifted = typed_ops::add_scalar(&typed, 0.5)?;
    assert_eq!(typed_shifted.values()?, &[1.5, 2.5, 3.5, 4.5, 5.5, 6.5]);

    let typed_row_sums = typed_ops::sum(&typed, Some(&[1]), false)?;
    assert_eq!(typed_row_sums.values()?, &[6.0, 15.0]);

    let typed_argmax = typed_ops::argmax(&typed, Some(&[1]), false)?;
    assert_eq!(typed_argmax.values()?, &[2, 2]);

    let pieces = typed_ops::split(&typed, 0, &[1, 1])?;
    let stacked = typed_ops::stack(&[&pieces[0], &pieces[1]], 0)?;
    assert_eq!(stacked.shape(), &[2, 1, 3]);

    let raw: Tensor = typed_row_sums.clone().into();
    let rebuilt = TensorF64::try_from(raw)?;
    assert_eq!(rebuilt, typed_row_sums);

    let ints = TensorI32::from_dense(vec![2, 2], vec![1, 2, 3, 4])?;
    let cumulative = typed_ops::cumsum(&ints, Some(1))?;
    assert_eq!(cumulative.values()?, &[1, 3, 3, 7]);

    Ok(())
}

fn demonstrate_arrow_and_ndarray_conversions(
    tensor: &Tensor,
    ipc_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // The `arrow` feature converts owned tensors to a companion RecordBatch layout.
    let batch = tensor.to_arrow_record_batch()?;
    assert_eq!(batch.num_rows(), 2);
    assert_eq!(batch.num_columns(), 2);
    assert_eq!(
        batch
            .schema()
            .metadata()
            .get("arcadia_tio_dim_lens")
            .map(String::as_str),
        Some("2,3")
    );
    assert_eq!(Tensor::from_arrow_record_batch(&batch)?, *tensor);

    // Arrow IPC bytes can be persisted by the caller; this tutorial writes a tiny
    // temp artifact and immediately decodes it back to an owned Tensor.
    fs::write(ipc_path, tensor.to_arrow_ipc()?)?;
    let ipc_bytes = fs::read(ipc_path)?;
    assert_eq!(Tensor::from_arrow_ipc(&ipc_bytes)?, *tensor);

    // The `ndarray` feature copies to and from Rust ndarray ArrayD values.
    let array = tensor.to_ndarray_f64()?;
    assert_eq!(array.shape(), &[2_usize, 3]);
    assert_eq!(array.get(IxDyn(&[1, 2])).copied(), Some(6.0));
    assert_eq!(Tensor::from_ndarray_f64(array)?, *tensor);

    let from_array = Tensor::from_ndarray_f64(ndarray::ArrayD::from_shape_vec(
        IxDyn(&[2_usize, 2]),
        vec![10.0, 20.0, 30.0, 40.0],
    )?)?;
    assert_eq!(from_array.shape, vec![2, 2]);
    assert_eq!(
        from_array.data,
        TensorData::F64(vec![10.0, 20.0, 30.0, 40.0])
    );

    Ok(())
}

fn demonstrate_csv_and_parquet_conversions(
    tensor: &Tensor,
    parquet_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    // CSV is a UTF-8 companion layout with dtype, shape, row-major order, and flat indices.
    let csv_text = tensor.to_csv_string()?;
    assert!(
        csv_text
            .starts_with("record,dtype,shape,order,flat_index,value\nmetadata,f64,2x3,row-major,,")
    );
    assert_eq!(Tensor::from_csv_str(&csv_text)?, *tensor);
    assert_eq!(Tensor::from_csv_bytes(&tensor.to_csv_bytes()?)?, *tensor);

    // Parquet uses a companion two-column flat-index/value schema plus tensor metadata.
    let parquet_bytes = tensor.to_parquet_bytes()?;
    assert!(!parquet_bytes.is_empty());
    assert_eq!(Tensor::from_parquet_bytes(&parquet_bytes)?, *tensor);

    tensor.to_parquet_file(parquet_path)?;
    assert_eq!(Tensor::from_parquet_file(parquet_path)?, *tensor);

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
