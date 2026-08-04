//! Compile-only coverage for crate-root paths retained across source-module splits.

use arcadia_tio_rs::{
    AxisCoordinateInputV2, CoordinateValueDomainV2, DType, DenseTensor, ErrorCode, Result, Tensor,
    TensorFile, check_native_abi_compatibility,
};

#[allow(dead_code)]
fn root_paths_remain_available(
    file: &TensorFile,
    tensor: Tensor,
    coordinate: AxisCoordinateInputV2,
) -> Result<(DType, ErrorCode, DenseTensor, CoordinateValueDomainV2)> {
    let _abi_check: fn() -> Result<u32> = check_native_abi_compatibility;
    let _ = (file, tensor, coordinate);
    unreachable!()
}

#[cfg(feature = "format-ocb")]
#[allow(dead_code)]
fn ocb_paths_remain_available(
    file: &arcadia_tio_rs::ocb::ColumnBundleFile,
    plan: &arcadia_tio_rs::ocb::ReadPlan<'_>,
) -> arcadia_tio_rs::ocb::OcbResult<()> {
    let _open = |path: &std::path::Path| arcadia_tio_rs::ocb::open(path);
    let _ = (file, plan, _open);
    Ok(())
}

#[allow(dead_code)]
fn module_paths_remain_available(tensor: &Tensor) -> Result<Tensor> {
    let tensor = arcadia_tio_rs::ops::to_contiguous(tensor)?;
    arcadia_tio_rs::typed_ops::reshape(
        &arcadia_tio_rs::TypedTensor::<f32>::try_from_tensor(tensor)?,
        vec![1],
    )
    .map(Into::into)
}
