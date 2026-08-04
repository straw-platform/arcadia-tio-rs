//! Compile-only coverage for raw crate-root paths retained across ABI-family splits.

use arcadia_tio_sys::{
    ArcadiaTioDType, ArcadiaTioHandle, arcadia_tio_abi_version, arcadia_tio_dtype,
};

#[allow(dead_code)]
fn root_paths_remain_available() {
    let _abi: unsafe extern "C" fn() -> u32 = arcadia_tio_abi_version;
    let _dtype: unsafe extern "C" fn(*mut ArcadiaTioHandle, *mut ArcadiaTioDType) -> i32 =
        arcadia_tio_dtype;
}

#[cfg(feature = "format-ocb")]
#[allow(dead_code)]
fn ocb_root_paths_remain_available() {
    let _open = arcadia_tio_sys::arcadia_tio_ocb_open;
    let _limits = core::mem::size_of::<arcadia_tio_sys::ArcadiaTioOcbResourceLimits>();
    let _ = (_open, _limits);
}
