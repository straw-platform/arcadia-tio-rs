use std::ffi::CString;
use std::slice;

use arcadia_tio_sys::*;

#[cfg(feature = "format-ocb")]
#[test]
fn ocb_resource_limits_initializer_returns_policy_a_and_zero_reserved_fields() {
    let mut limits = std::mem::MaybeUninit::<ArcadiaTioOcbResourceLimits>::uninit();
    unsafe { arcadia_tio_ocb_resource_limits_init(limits.as_mut_ptr()) };
    let limits = unsafe { limits.assume_init() };

    assert_eq!(limits.version, ARCADIA_TIO_OCB_ABI_VERSION);
    assert_eq!(
        limits.struct_size,
        std::mem::size_of::<ArcadiaTioOcbResourceLimits>()
    );
    assert_eq!(limits.max_encoded_object_bytes, 1_073_741_824);
    assert_eq!(limits.max_compressed_chunk_bytes, 536_870_912);
    assert_eq!(limits.max_decompressed_chunk_bytes, 536_870_912);
    assert_eq!(limits.max_projected_row_group_bytes, 1_073_741_824);
    assert_eq!(limits.max_owned_selected_compressed_bytes, 8_589_934_592);
    assert_eq!(limits.max_owned_decoded_materialized_bytes, 17_179_869_184);
    assert_eq!(limits.reserved, [0; 4]);
}

#[test]
fn linked_native_library_can_roundtrip_tiny_f64_tensor() {
    let path = unique_path("arcadia-tio-sys-smoke-f64.tio");
    let c_path = CString::new(path.to_string_lossy().as_bytes()).expect("test path has no NUL");
    let kinds = [ARCADIA_TIO_AXIS_TIME];
    let dim_lens = [0_u32];

    unsafe {
        assert_eq!(arcadia_tio_abi_version(), ARCADIA_TIO_ABI_VERSION);
        assert_eq!(
            arcadia_tio_compaction_abi_version(),
            ARCADIA_TIO_COMPACTION_ABI_VERSION
        );

        let handle = arcadia_tio_create_streaming(
            c_path.as_ptr(),
            ARCADIA_TIO_DTYPE_F64,
            kinds.as_ptr(),
            dim_lens.as_ptr(),
            1,
            0,
        );
        assert!(!handle.is_null(), "create failed: {}", last_error());

        let values = [1.25_f64, -2.5, 3.75];
        let shape = [values.len() as u64];
        let mut start = u32::MAX;
        let mut end = u32::MAX;
        let append_status = arcadia_tio_append_f64_with_range(
            handle,
            values.as_ptr(),
            shape.as_ptr(),
            shape.len(),
            &mut start,
            &mut end,
        );
        assert_eq!(
            append_status,
            ARCADIA_TIO_ERROR_OK,
            "append failed: {}",
            last_error()
        );
        assert_eq!((start, end), (0, 3));

        let mut out = ArcadiaTioTensor::default();
        let read_status = arcadia_tio_read_all(handle, &mut out);
        assert_eq!(
            read_status,
            ARCADIA_TIO_ERROR_OK,
            "read failed: {}",
            last_error()
        );
        assert_eq!(out.dtype, ARCADIA_TIO_DTYPE_F64);
        assert_eq!(out.rank, 1);
        assert!(!out.shape.is_null());
        assert_eq!(*out.shape, values.len() as u64);
        assert_eq!(out.len_bytes, values.len() * std::mem::size_of::<f64>());
        assert!(!out.data.is_null());

        let got = slice::from_raw_parts(out.data.cast::<f64>(), values.len());
        assert_eq!(got, values);

        let reshape_shape = [1_u64, 3];
        let mut reshaped = ArcadiaTioTensor::default();
        let reshape_status = arcadia_tio_tensor_reshape(
            &out,
            reshape_shape.as_ptr(),
            reshape_shape.len(),
            &mut reshaped,
        );
        assert_eq!(
            reshape_status,
            ARCADIA_TIO_ERROR_OK,
            "reshape failed: {}",
            last_error()
        );
        assert_eq!(reshaped.dtype, ARCADIA_TIO_DTYPE_F64);
        assert_eq!(reshaped.rank, 2);
        assert_eq!(
            slice::from_raw_parts(reshaped.shape, reshaped.rank),
            reshape_shape
        );
        assert_eq!(
            slice::from_raw_parts(reshaped.data.cast::<f64>(), values.len()),
            values
        );
        arcadia_tio_tensor_free(&mut reshaped);

        let mut indexed = ArcadiaTioTensor::default();
        let index_status = arcadia_tio_tensor_index_axis(&out, 0, 1, &mut indexed);
        assert_eq!(
            index_status,
            ARCADIA_TIO_ERROR_OK,
            "index_axis failed: {}",
            last_error()
        );
        assert_eq!(indexed.dtype, ARCADIA_TIO_DTYPE_F64);
        assert_eq!(indexed.rank, 1);
        assert_eq!(slice::from_raw_parts(indexed.shape, indexed.rank), [1_u64]);
        assert_eq!(
            slice::from_raw_parts(indexed.data.cast::<f64>(), 1),
            [values[1]]
        );
        arcadia_tio_tensor_free(&mut indexed);

        let mut scaled = ArcadiaTioTensor::default();
        let scale_status = arcadia_tio_tensor_mul_scalar(&out, 2.0, &mut scaled);
        assert_eq!(
            scale_status,
            ARCADIA_TIO_ERROR_OK,
            "mul_scalar failed: {}",
            last_error()
        );
        assert_eq!(scaled.dtype, ARCADIA_TIO_DTYPE_F64);
        assert_eq!(scaled.rank, 1);
        assert_eq!(slice::from_raw_parts(scaled.shape, scaled.rank), [3_u64]);
        assert_eq!(
            slice::from_raw_parts(scaled.data.cast::<f64>(), values.len()),
            [2.5_f64, -5.0, 7.5]
        );
        arcadia_tio_tensor_free(&mut scaled);

        arcadia_tio_tensor_free(&mut out);
        arcadia_tio_close(handle);
    }

    let _ = std::fs::remove_file(path);
}

fn unique_path(name: &str) -> std::path::PathBuf {
    let nonce = format!("{}-{}", std::process::id(), unique_counter());
    let dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"))
        .join("sys-tests");
    std::fs::create_dir_all(&dir).expect("create project-local sys test directory");
    dir.join(format!("{nonce}-{name}"))
}

fn unique_counter() -> usize {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

unsafe fn last_error() -> String {
    let msg = unsafe { arcadia_tio_last_error_message() };
    if msg.is_null() {
        format!("code={}", unsafe { arcadia_tio_last_error_code() })
    } else {
        unsafe { std::ffi::CStr::from_ptr(msg) }
            .to_string_lossy()
            .into_owned()
    }
}
