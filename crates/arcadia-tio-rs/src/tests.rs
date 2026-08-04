use super::*;

fn assert_f32_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (*actual - *expected).abs() <= 1.0e-6,
            "expected {expected}, got {actual}"
        );
    }
}

fn assert_f64_close(actual: &[f64], expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (*actual - *expected).abs() <= 1.0e-12,
            "expected {expected}, got {actual}"
        );
    }
}

fn empty_file_meta() -> sys::ArcadiaTioFileMeta {
    empty_file_meta_output()
}

#[test]
fn native_abi_range_accepts_only_authoritative_version_three() {
    let below = validate_native_abi_version(2).expect_err("ABI below minimum must reject");
    assert_eq!(below.code(), ErrorCode::Unimplemented);
    assert!(validate_native_abi_version(3).is_ok());
    let above = validate_native_abi_version(4).expect_err("ABI above maximum must reject");
    assert_eq!(above.code(), ErrorCode::Unimplemented);
    assert_eq!(MIN_SUPPORTED_NATIVE_ABI_VERSION, 3);
    assert_eq!(MAX_SUPPORTED_NATIVE_ABI_VERSION, 3);
}

#[test]
fn native_abi_gate_caches_success_and_incompatibility() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    let success_queries = AtomicUsize::new(0);
    let success_gate = AbiGate::new();
    for _ in 0..2 {
        assert!(
            success_gate
                .get_or_query(|| {
                    success_queries.fetch_add(1, Ordering::SeqCst);
                    3
                })
                .is_ok()
        );
    }
    assert_eq!(success_queries.load(Ordering::SeqCst), 1);

    let rejected_queries = AtomicUsize::new(0);
    let rejected_gate = AbiGate::new();
    for _ in 0..2 {
        let error = rejected_gate
            .get_or_query(|| {
                rejected_queries.fetch_add(1, Ordering::SeqCst);
                4
            })
            .expect_err("unsupported ABI must stay rejected");
        assert_eq!(error.code(), ErrorCode::Unimplemented);
    }
    assert_eq!(rejected_queries.load(Ordering::SeqCst), 1);
}

#[test]
fn native_output_guards_free_exactly_once_on_success_and_conversion_error() {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static VALUE_FREES: AtomicUsize = AtomicUsize::new(0);
    static ARRAY_FREES: AtomicUsize = AtomicUsize::new(0);
    static POINTER_FREES: AtomicUsize = AtomicUsize::new(0);

    unsafe extern "C" fn free_value(_: *mut u32) {
        VALUE_FREES.fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "C" fn free_array(_: *mut u8, _: usize) {
        ARRAY_FREES.fetch_add(1, Ordering::SeqCst);
    }

    unsafe extern "C" fn free_pointer(_: *mut u8) {
        POINTER_FREES.fetch_add(1, Ordering::SeqCst);
    }

    VALUE_FREES.store(0, Ordering::SeqCst);
    let result: Result<()> = {
        let _guard = NativeOutput::new(7u32, free_value);
        Err(TioError::conversion("synthetic conversion error"))
    };
    assert!(result.is_err());
    assert_eq!(VALUE_FREES.load(Ordering::SeqCst), 1);
    {
        let _guard = NativeOutput::new(8u32, free_value);
    }
    assert_eq!(VALUE_FREES.load(Ordering::SeqCst), 2);

    ARRAY_FREES.store(0, Ordering::SeqCst);
    let result: Result<()> = {
        let mut guard = NativeArrayOutput::<u8>::new(free_array);
        unsafe {
            *guard.ptr_out() = NonNull::<u8>::dangling().as_ptr();
            *guard.len_out() = 1;
        }
        Err(TioError::conversion("synthetic array conversion error"))
    };
    assert!(result.is_err());
    assert_eq!(ARRAY_FREES.load(Ordering::SeqCst), 1);

    POINTER_FREES.store(0, Ordering::SeqCst);
    let result: Result<()> = {
        let mut guard = NativePointerOutput::<u8>::new(free_pointer);
        unsafe {
            *guard.out() = NonNull::<u8>::dangling().as_ptr();
        }
        Err(TioError::conversion("synthetic pointer conversion error"))
    };
    assert!(result.is_err());
    assert_eq!(POINTER_FREES.load(Ordering::SeqCst), 1);

    let transferred = {
        let mut guard = NativePointerOutput::<u8>::new(free_pointer);
        unsafe {
            *guard.out() = NonNull::<u8>::dangling().as_ptr();
        }
        guard.take()
    };
    assert_eq!(POINTER_FREES.load(Ordering::SeqCst), 1);
    unsafe { free_pointer(transferred) };
    assert_eq!(POINTER_FREES.load(Ordering::SeqCst), 2);

    {
        let _guard = NativePointerOutput::<u8>::new(free_pointer);
    }
    assert_eq!(POINTER_FREES.load(Ordering::SeqCst), 2);
}

#[test]
fn checked_native_slice_rejects_misalignment_and_unrepresentable_lengths() {
    let misaligned = ptr::without_provenance::<u32>(1);
    let error = unsafe { checked_slice(misaligned, 1, "synthetic u32 output") }
        .expect_err("misaligned pointer must reject before dereference");
    assert_eq!(error.code(), ErrorCode::InvalidArgument);

    let aligned = NonNull::<u64>::dangling().as_ptr().cast_const();
    let error = unsafe { checked_slice(aligned, usize::MAX, "synthetic huge output") }
        .expect_err("unrepresentable byte length must reject before dereference");
    assert_eq!(error.code(), ErrorCode::InvalidArgument);
}

#[test]
fn file_meta_rejects_null_dimensions_with_nonzero_rank() {
    let mut raw = empty_file_meta();
    raw.rank = 1;
    let result = copy_file_meta(&raw);
    assert!(result.is_err(), "null/nonzero dimensions must fail closed");
}

#[test]
fn file_meta_rejects_null_required_axis_label_name() {
    let mut dim = sys::ArcadiaTioDimSpec {
        kind: sys::ARCADIA_TIO_AXIS_TIME,
        len: 0,
        name: ptr::null_mut(),
    };
    let mut label = sys::ArcadiaTioAxisLabel {
        id: 7,
        name: ptr::null_mut(),
    };
    let mut raw = empty_file_meta();
    raw.dims = &mut dim;
    raw.rank = 1;
    raw.symbols = &mut label;
    raw.symbols_len = 1;
    let result = copy_file_meta(&raw);
    assert!(
        result.is_err(),
        "null required axis-label name must fail closed"
    );
}

#[test]
fn file_meta_rejects_invalid_utf8_optional_dimension_name() {
    let mut invalid_utf8 = [0xff_u8, 0];
    let mut dim = sys::ArcadiaTioDimSpec {
        kind: sys::ARCADIA_TIO_AXIS_TIME,
        len: 0,
        name: invalid_utf8.as_mut_ptr().cast(),
    };
    let mut raw = empty_file_meta();
    raw.dims = &mut dim;
    raw.rank = 1;
    let result = copy_file_meta(&raw);
    assert!(
        result.is_err(),
        "invalid UTF-8 optional dimension name must fail closed"
    );
}

#[test]
fn coordinate_dictionary_rejects_null_entries_with_nonzero_length() {
    let raw = sys::ArcadiaTioCoordinateDictionaryV2 {
        entries: ptr::null_mut(),
        entries_len: 1,
        ..sys::ArcadiaTioCoordinateDictionaryV2::default()
    };
    let result = unsafe { CoordinateDictionaryV2::from_raw_borrowed(&raw) };
    assert!(
        result.is_err(),
        "null/nonzero dictionary entries must fail closed"
    );
}

#[test]
fn coordinate_value_slice_rejects_null_data_with_nonzero_length() {
    let raw = sys::ArcadiaTioCoordinateValueSliceV2 {
        data: ptr::null_mut(),
        len: 1,
        element_size: mem::size_of::<i32>(),
        ..sys::ArcadiaTioCoordinateValueSliceV2::default()
    };
    let result = unsafe { CoordinateValueSliceV2::from_raw_borrowed(&raw) };
    assert!(
        result.is_err(),
        "null/nonzero coordinate values must fail closed"
    );
}

#[test]
fn coordinate_lookup_rejects_null_positions_with_nonzero_length() {
    let raw = sys::ArcadiaTioCoordinateLookupResultV2 {
        positions: ptr::null_mut(),
        positions_len: 1,
        ..sys::ArcadiaTioCoordinateLookupResultV2::default()
    };
    let result = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
    assert!(
        result.is_err(),
        "null/nonzero lookup positions must fail closed"
    );
}

#[test]
fn tensor_constructors_validate_shape_and_accessors() {
    let tensor =
        Tensor::from_dense_i32(vec![2, 2], vec![1, 2, 3, 4]).expect("valid dense i32 tensor");
    assert_eq!(tensor.dtype, DType::I32);
    assert_eq!(tensor.element_len().expect("element len"), 4);
    assert_eq!(tensor.values_i32().expect("i32 values"), &[1, 2, 3, 4]);
    assert_eq!(tensor.data.dtype(), DType::I32);

    let err = Tensor::from_dense_i32(vec![3], vec![1, 2]).expect_err("shape mismatch rejects");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let mismatched = Tensor {
        dtype: DType::F32,
        shape: vec![1],
        data: TensorData::I32(vec![1]),
    };
    assert_eq!(
        mismatched
            .validate()
            .expect_err("dtype mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn tensor_ops_shape_index_and_broadcast_success() {
    let tensor = Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .expect("input tensor");

    let contiguous = ops::to_contiguous(&tensor).expect("to contiguous");
    assert_eq!(contiguous, tensor);

    let reshaped = ops::reshape(&tensor, vec![3, 2]).expect("reshape");
    assert_eq!(reshaped.shape, vec![3, 2]);
    assert_eq!(reshaped.data, tensor.data);

    let transposed = ops::transpose(&tensor).expect("transpose");
    assert_eq!(transposed.shape, vec![3, 2]);
    assert_eq!(
        transposed.data,
        TensorData::F32(vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0])
    );

    let sliced = ops::slice_axis(&tensor, 1, 1, 3).expect("slice axis");
    assert_eq!(sliced.shape, vec![2, 2]);
    assert_eq!(sliced.data, TensorData::F32(vec![2.0, 3.0, 5.0, 6.0]));

    let taken = ops::take_axis(&tensor, 0, &[1, 0]).expect("take axis");
    assert_eq!(taken.shape, vec![2, 3]);
    assert_eq!(
        taken.data,
        TensorData::F32(vec![4.0, 5.0, 6.0, 1.0, 2.0, 3.0])
    );

    let indexed = ops::index_axis(&tensor, -1, 0).expect("index axis");
    assert_eq!(indexed.shape, vec![2, 1]);
    assert_eq!(indexed.data, TensorData::F32(vec![1.0, 4.0]));

    let broadcasted = ops::broadcast_to(
        &Tensor::from_dense_i32(vec![2, 1], vec![10, 20]).expect("broadcast input"),
        vec![2, 3],
    )
    .expect("broadcast");
    assert_eq!(broadcasted.shape, vec![2, 3]);
    assert_eq!(
        broadcasted.data,
        TensorData::I32(vec![10, 10, 10, 20, 20, 20])
    );
}

#[test]
fn tensor_ops_assembly_helpers_success() {
    let rows_a = Tensor::from_dense_f64(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0]).expect("rows a");
    let rows_b = Tensor::from_dense_f64(vec![1, 2], vec![5.0, 6.0]).expect("rows b");
    let concatenated_rows = ops::concat(&[&rows_a, &rows_b], 0).expect("concat rows");
    assert_eq!(concatenated_rows.shape, vec![3, 2]);
    assert_eq!(
        concatenated_rows.data,
        TensorData::F64(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
    );

    let cols_b = Tensor::from_dense_f64(vec![2, 1], vec![7.0, 8.0]).expect("cols b");
    let concatenated_cols = ops::concat(&[&rows_a, &cols_b], -1).expect("concat cols");
    assert_eq!(concatenated_cols.shape, vec![2, 3]);
    assert_eq!(
        concatenated_cols.data,
        TensorData::F64(vec![1.0, 2.0, 7.0, 3.0, 4.0, 8.0])
    );

    let left = Tensor::from_dense_i32(vec![2], vec![1, 2]).expect("stack left");
    let right = Tensor::from_dense_i32(vec![2], vec![3, 4]).expect("stack right");
    let stacked_axis0 = ops::stack(&[&left, &right], 0).expect("stack axis 0");
    assert_eq!(stacked_axis0.shape, vec![2, 2]);
    assert_eq!(stacked_axis0.data, TensorData::I32(vec![1, 2, 3, 4]));
    let stacked_last = ops::stack(&[&left, &right], -1).expect("stack last axis");
    assert_eq!(stacked_last.shape, vec![2, 2]);
    assert_eq!(stacked_last.data, TensorData::I32(vec![1, 3, 2, 4]));

    let split_input = Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
        .expect("split input");
    let split = ops::split(&split_input, -1, &[1, 2]).expect("split last axis");
    assert_eq!(split.len(), 2);
    assert_eq!(split[0].shape, vec![2, 1]);
    assert_eq!(split[0].data, TensorData::F32(vec![1.0, 4.0]));
    assert_eq!(split[1].shape, vec![2, 2]);
    assert_eq!(split[1].data, TensorData::F32(vec![2.0, 3.0, 5.0, 6.0]));

    let unstacked = ops::unstack(&rows_a, -1).expect("unstack last axis");
    assert_eq!(unstacked.len(), 2);
    assert_eq!(unstacked[0].shape, vec![2]);
    assert_eq!(unstacked[0].data, TensorData::F64(vec![1.0, 3.0]));
    assert_eq!(unstacked[1].shape, vec![2]);
    assert_eq!(unstacked[1].data, TensorData::F64(vec![2.0, 4.0]));

    let repeated = ops::repeat(&split_input, -1, 2).expect("repeat last axis");
    assert_eq!(repeated.shape, vec![2, 6]);
    assert_eq!(
        repeated.data,
        TensorData::F32(vec![
            1.0, 1.0, 2.0, 2.0, 3.0, 3.0, 4.0, 4.0, 5.0, 5.0, 6.0, 6.0
        ])
    );

    let tiled = ops::tile(
        &Tensor::from_dense_i64(vec![2, 1], vec![5, 6]).expect("tile input"),
        &[1, 3],
    )
    .expect("tile cols");
    assert_eq!(tiled.shape, vec![2, 3]);
    assert_eq!(tiled.data, TensorData::I64(vec![5, 5, 5, 6, 6, 6]));

    let reorder =
        Tensor::from_dense_i32(vec![2, 3], vec![1, 2, 3, 4, 5, 6]).expect("reorder input");
    let flipped = ops::flip(&reorder, -1).expect("flip last axis");
    assert_eq!(flipped.shape, vec![2, 3]);
    assert_eq!(flipped.data, TensorData::I32(vec![3, 2, 1, 6, 5, 4]));
    let rolled = ops::roll(&reorder, 1, 1).expect("roll axis 1");
    assert_eq!(rolled.shape, vec![2, 3]);
    assert_eq!(rolled.data, TensorData::I32(vec![3, 1, 2, 6, 4, 5]));
}

#[test]
fn tensor_ops_assembly_validation_failures_are_reported() {
    let f32_tensor = Tensor::from_dense_f32(vec![2], vec![1.0, 2.0]).expect("f32 tensor");
    let f64_tensor = Tensor::from_dense_f64(vec![2], vec![1.0, 2.0]).expect("f64 tensor");
    let rank2 = Tensor::from_dense_f32(vec![1, 2], vec![1.0, 2.0]).expect("rank-2 tensor");
    let mismatched_cols =
        Tensor::from_dense_f32(vec![1, 3], vec![1.0, 2.0, 3.0]).expect("cols mismatch");

    assert_eq!(
        ops::concat(&[], 0)
            .expect_err("empty concat rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::stack(&[], 0).expect_err("empty stack rejects").code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::concat(&[&f32_tensor, &f64_tensor], 0)
            .expect_err("concat dtype mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::concat(&[&f32_tensor, &rank2], 0)
            .expect_err("concat rank mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::concat(&[&rank2, &mismatched_cols], 0)
            .expect_err("concat shape mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::stack(&[&f32_tensor, &f64_tensor], 0)
            .expect_err("stack dtype mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::stack(&[&rank2, &mismatched_cols], 0)
            .expect_err("stack shape mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::split(&rank2, 1, &[])
            .expect_err("empty split sections reject")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::split(&rank2, -1, &[1])
            .expect_err("split section sum mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::unstack(&f32_tensor, 0)
            .expect_err("rank-1 unstack rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::repeat(&f32_tensor, 1, 2)
            .expect_err("repeat axis out of bounds rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::tile(&rank2, &[2])
            .expect_err("tile reps rank mismatch rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::flip(&rank2, -3)
            .expect_err("flip axis out of bounds rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::roll(&rank2, 2, 1)
            .expect_err("roll axis out of bounds rejects")
            .code(),
        ErrorCode::InvalidArgument
    );
}

#[test]
fn tensor_ops_assembly_empty_and_huge_outputs_return_errors() {
    let tensor = Tensor::from_dense_i32(vec![2, 2], vec![1, 2, 3, 4]).expect("tensor");
    let repeated_zero = ops::repeat(&tensor, 0, 0).expect("zero repeat");
    assert_eq!(repeated_zero.shape, vec![0, 2]);
    assert_eq!(repeated_zero.data, TensorData::I32(Vec::new()));

    let tiled_zero = ops::tile(&tensor, &[2, 0]).expect("zero tile");
    assert_eq!(tiled_zero.shape, vec![4, 0]);
    assert_eq!(tiled_zero.data, TensorData::I32(Vec::new()));

    let empty = Tensor::from_dense_i64(vec![0, 2], Vec::new()).expect("empty tensor");
    let flipped_empty = ops::flip(&empty, 0).expect("flip empty axis");
    assert_eq!(flipped_empty.shape, vec![0, 2]);
    assert_eq!(flipped_empty.data, TensorData::I64(Vec::new()));
    let rolled_empty = ops::roll(&empty, -1, -3).expect("roll empty tensor");
    assert_eq!(rolled_empty.shape, vec![0, 2]);
    assert_eq!(rolled_empty.data, TensorData::I64(Vec::new()));

    let split_empty = ops::split(&empty, 0, &[0]).expect("split empty axis");
    assert_eq!(split_empty.len(), 1);
    assert_eq!(split_empty[0].shape, vec![0, 2]);
    assert_eq!(split_empty[0].data, TensorData::I64(Vec::new()));
    let unstack_empty = ops::unstack(&empty, 0).expect("unstack empty axis");
    assert!(unstack_empty.is_empty());

    let scalar_like = Tensor::from_dense_i32(vec![1], vec![7]).expect("scalar-like tensor");
    let err = ops::repeat(&scalar_like, 0, usize::MAX)
        .expect_err("huge repeat should not allocate or panic");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    let err =
        ops::tile(&scalar_like, &[usize::MAX]).expect_err("huge tile should not allocate or panic");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[test]
fn tensor_ops_math_and_reductions_cover_public_dtypes() {
    let lhs = Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).expect("lhs");
    let rhs = Tensor::from_dense_f64(vec![3], vec![10.0, 20.0, 30.0]).expect("rhs");
    let added = ops::add(&lhs, &rhs).expect("broadcast add");
    assert_eq!(added.shape, vec![2, 3]);
    assert_eq!(
        added.data,
        TensorData::F64(vec![11.0, 22.0, 33.0, 14.0, 25.0, 36.0])
    );

    let scaled = ops::mul_scalar(&lhs, 2.0_f64).expect("scalar multiply");
    assert_eq!(
        scaled.data,
        TensorData::F64(vec![2.0, 4.0, 6.0, 8.0, 10.0, 12.0])
    );

    let ints = Tensor::from_dense_i64(vec![2, 3], vec![1, 2, 3, 4, 5, 6]).expect("i64");
    let int_sums = ops::sum(&ints, Some(&[1]), false).expect("i64 sum");
    assert_eq!(int_sums.shape, vec![2]);
    assert_eq!(int_sums.data, TensorData::I64(vec![6, 15]));

    let int_mean = ops::mean(
        &Tensor::from_dense_i32(vec![2, 2], vec![1, 2, 3, 4]).expect("i32 mean input"),
        Some(&[0]),
        true,
    )
    .expect("i32 mean promotes to f64");
    assert_eq!(int_mean.dtype, DType::F64);
    assert_eq!(int_mean.shape, vec![1, 2]);
    assert_eq!(int_mean.data, TensorData::F64(vec![2.0, 3.0]));

    let floats = Tensor::from_dense_f32(vec![2, 2], vec![3.0, 1.0, 4.0, 2.0]).expect("f32");
    assert_eq!(
        ops::min(&floats, Some(&[1]), false).expect("f32 min").data,
        TensorData::F32(vec![1.0, 2.0])
    );
    assert_eq!(
        ops::max(&floats, Some(&[0]), false).expect("f32 max").data,
        TensorData::F32(vec![4.0, 2.0])
    );

    let all_axis_sum = ops::sum(&ints, None, true).expect("all-axis keepdims sum");
    assert_eq!(all_axis_sum.shape, vec![1, 1]);
    assert_eq!(all_axis_sum.data, TensorData::I64(vec![21]));

    let all_axis_min = ops::min(&floats, None, true).expect("all-axis keepdims min");
    assert_eq!(all_axis_min.shape, vec![1, 1]);
    assert_eq!(all_axis_min.data, TensorData::F32(vec![1.0]));
}

#[test]
fn tensor_ops_arg_and_cumulative_reductions_cover_public_dtypes() {
    let floats =
        Tensor::from_dense_f32(vec![2, 3], vec![3.0, 1.0, 2.0, 6.0, 5.0, 4.0]).expect("f32 tensor");
    let argmin_rows = ops::argmin(&floats, Some(&[1]), false).expect("argmin rows");
    assert_eq!(argmin_rows.dtype, DType::I64);
    assert_eq!(argmin_rows.shape, vec![2]);
    assert_eq!(argmin_rows.data, TensorData::I64(vec![1, 2]));
    let argmax_rows_keep = ops::argmax(&floats, Some(&[-1]), true).expect("argmax rows");
    assert_eq!(argmax_rows_keep.shape, vec![2, 1]);
    assert_eq!(argmax_rows_keep.data, TensorData::I64(vec![0, 0]));
    let argmin_all = ops::argmin(&floats, None, true).expect("argmin all axes");
    assert_eq!(argmin_all.shape, vec![1, 1]);
    assert_eq!(argmin_all.data, TensorData::I64(vec![1]));
    let argmax_empty_axes = ops::argmax(&floats, Some(&[]), false).expect("argmax no axes");
    assert_eq!(argmax_empty_axes.shape, vec![2, 3]);
    assert_eq!(argmax_empty_axes.data, TensorData::I64(vec![0; 6]));

    let f64_values =
        Tensor::from_dense_f64(vec![2, 2], vec![1.0, 4.0, 3.0, 2.0]).expect("f64 tensor");
    assert_eq!(
        ops::argmax(&f64_values, Some(&[0]), false)
            .expect("f64 argmax")
            .data,
        TensorData::I64(vec![1, 0])
    );
    let i32_values =
        Tensor::from_dense_i32(vec![2, 3], vec![3, 1, 2, 6, 5, 4]).expect("i32 tensor");
    assert_eq!(
        ops::argmin(&i32_values, Some(&[1]), false)
            .expect("i32 argmin")
            .data,
        TensorData::I64(vec![1, 2])
    );
    let i64_values = Tensor::from_dense_i64(vec![2, 2], vec![9, 7, 8, 6]).expect("i64 tensor");
    assert_eq!(
        ops::argmax(&i64_values, Some(&[0]), true)
            .expect("i64 argmax")
            .data,
        TensorData::I64(vec![0, 0])
    );

    let cumsum_f32 = ops::cumsum(&floats, Some(-1)).expect("f32 cumsum");
    assert_eq!(cumsum_f32.shape, vec![2, 3]);
    assert_eq!(
        cumsum_f32.data,
        TensorData::F32(vec![3.0, 4.0, 6.0, 6.0, 11.0, 15.0])
    );
    let cumsum_f64 = ops::cumsum(&f64_values, None).expect("f64 flat cumsum");
    assert_eq!(cumsum_f64.shape, vec![4]);
    assert_eq!(cumsum_f64.data, TensorData::F64(vec![1.0, 5.0, 8.0, 10.0]));
    let cumprod_i32 = ops::cumprod(&i32_values, Some(1)).expect("i32 cumprod");
    assert_eq!(cumprod_i32.data, TensorData::I32(vec![3, 3, 6, 6, 30, 120]));
    let cumsum_i64 = ops::cumsum(&i64_values, None).expect("i64 flat cumsum");
    assert_eq!(cumsum_i64.shape, vec![4]);
    assert_eq!(cumsum_i64.data, TensorData::I64(vec![9, 16, 24, 30]));
}

#[test]
fn tensor_ops_var_std_cover_dtype_promotion_and_keepdims() {
    let f32_tensor =
        Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).expect("f32 tensor");
    let f32_var = ops::var(&f32_tensor, Some(&[1]), false).expect("f32 var");
    assert_eq!(f32_var.dtype, DType::F32);
    assert_eq!(f32_var.shape, vec![2]);
    match f32_var.data {
        TensorData::F32(values) => assert_f32_close(&values, &[2.0 / 3.0, 2.0 / 3.0]),
        other => panic!("unexpected payload {other:?}"),
    }
    let f32_std = ops::std(&f32_tensor, Some(&[1]), true).expect("f32 std keepdims");
    assert_eq!(f32_std.shape, vec![2, 1]);
    match f32_std.data {
        TensorData::F32(values) => {
            let expected = (2.0_f32 / 3.0).sqrt();
            assert_f32_close(&values, &[expected, expected]);
        }
        other => panic!("unexpected payload {other:?}"),
    }

    let f64_var = ops::var(
        &Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .expect("f64 tensor"),
        Some(&[0]),
        true,
    )
    .expect("f64 var keepdims");
    assert_eq!(f64_var.dtype, DType::F64);
    assert_eq!(f64_var.shape, vec![1, 3]);
    match f64_var.data {
        TensorData::F64(values) => assert_f64_close(&values, &[2.25, 2.25, 2.25]),
        other => panic!("unexpected payload {other:?}"),
    }

    let i32_var = ops::var(
        &Tensor::from_dense_i32(vec![2, 2], vec![1, 2, 3, 4]).expect("i32 tensor"),
        Some(&[0]),
        false,
    )
    .expect("i32 var promotes");
    assert_eq!(i32_var.dtype, DType::F64);
    assert_eq!(i32_var.shape, vec![2]);
    assert_eq!(i32_var.data, TensorData::F64(vec![1.0, 1.0]));

    let i64_std_empty_axes = ops::std(
        &Tensor::from_dense_i64(vec![2, 2], vec![1, 2, 3, 4]).expect("i64 tensor"),
        Some(&[]),
        false,
    )
    .expect("i64 std over no axes");
    assert_eq!(i64_std_empty_axes.dtype, DType::F64);
    assert_eq!(i64_std_empty_axes.shape, vec![2, 2]);
    assert_eq!(i64_std_empty_axes.data, TensorData::F64(vec![0.0; 4]));
}

#[test]
fn tensor_ops_reduction_edge_cases_and_nan_policy_are_explicit() {
    let nan_tensor =
        Tensor::from_dense_f32(vec![2, 2], vec![f32::NAN, 1.0, 2.0, f32::NAN]).expect("nan tensor");
    match ops::min(&nan_tensor, Some(&[1]), false)
        .expect("nan min")
        .data
    {
        TensorData::F32(values) => {
            assert!(values[0].is_nan());
            assert_eq!(values[1], 2.0);
        }
        other => panic!("unexpected payload {other:?}"),
    }
    assert_eq!(
        ops::argmin(&nan_tensor, Some(&[1]), false)
            .expect("nan argmin")
            .data,
        TensorData::I64(vec![0, 0])
    );
    assert_eq!(
        ops::argmax(&nan_tensor, Some(&[1]), false)
            .expect("nan argmax")
            .data,
        TensorData::I64(vec![0, 0])
    );

    let empty = Tensor::from_dense_i32(vec![0, 3], Vec::new()).expect("empty tensor");
    assert!(ops::argmin(&empty, Some(&[0]), false).is_err());
    assert!(ops::var(&empty, Some(&[0]), false).is_err());
    let empty_cumsum = ops::cumsum(&empty, Some(1)).expect("empty cumsum");
    assert_eq!(empty_cumsum.shape, vec![0, 3]);
    assert_eq!(empty_cumsum.data, TensorData::I32(Vec::new()));
    let zero_output_empty =
        Tensor::from_dense_i32(vec![0, 0], Vec::new()).expect("zero-output empty tensor");
    let empty_var = ops::var(&zero_output_empty, Some(&[0]), false)
        .expect("zero-output variance should not require a value");
    assert_eq!(empty_var.shape, vec![0]);
    assert_eq!(empty_var.data, TensorData::F64(Vec::new()));
    let empty_std = ops::std(&zero_output_empty, Some(&[0]), false)
        .expect("zero-output std should not require a value");
    assert_eq!(empty_std.shape, vec![0]);
    assert_eq!(empty_std.data, TensorData::F64(Vec::new()));

    let f32_tensor = Tensor::from_dense_f32(vec![2], vec![1.0, 2.0]).expect("f32 tensor");
    assert!(ops::argmax(&f32_tensor, Some(&[0, 0]), true).is_err());
    assert!(ops::argmin(&f32_tensor, None, false).is_err());
    assert!(ops::std(&f32_tensor, None, false).is_err());
    assert!(ops::cumsum(&f32_tensor, Some(1)).is_err());
    assert_eq!(
        ops::cumsum(
            &Tensor::from_dense_i32(vec![2], vec![i32::MAX, 1]).expect("i32 overflow"),
            None,
        )
        .expect_err("cumsum overflow rejects")
        .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::cumprod(
            &Tensor::from_dense_i64(vec![2], vec![i64::MAX, 2]).expect("i64 overflow"),
            None,
        )
        .expect_err("cumprod overflow rejects")
        .code(),
        ErrorCode::InvalidArgument
    );

    #[cfg(target_pointer_width = "64")]
    {
        let empty_wide = Tensor::from_dense_i32(vec![0, u64::MAX], Vec::new())
            .expect("zero-element huge-shape tensor");
        let err = ops::argmax(&empty_wide, Some(&[0]), false)
            .expect_err("huge arg output should not allocate or panic");
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }
}

#[test]
fn tensor_ops_validation_failures_are_reported() {
    let f32_tensor = Tensor::from_dense_f32(vec![2], vec![1.0, 2.0]).expect("f32 tensor");
    let f64_tensor = Tensor::from_dense_f64(vec![2], vec![1.0, 2.0]).expect("f64 tensor");

    assert_eq!(
        ops::add(&f32_tensor, &f64_tensor)
            .expect_err("dtype mismatch")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert_eq!(
        ops::add_scalar(&f32_tensor, Scalar::F64(1.0))
            .expect_err("scalar mismatch")
            .code(),
        ErrorCode::InvalidArgument
    );
    assert!(ops::broadcast_to(&f32_tensor, vec![3]).is_err());
    assert!(ops::reshape(&f32_tensor, vec![3]).is_err());
    assert!(ops::permute_axes(&f32_tensor, &[0, 0]).is_err());
    assert!(
        ops::permute_axes(
            &Tensor::from_dense_f32(vec![1, 2], vec![1.0, 2.0]).expect("rank-2 tensor"),
            &[0],
        )
        .is_err()
    );
    assert!(ops::sum(&f32_tensor, None, false).is_err());
    assert!(
        ops::div_scalar(
            &Tensor::from_dense_i32(vec![1], vec![1]).expect("i32 tensor"),
            0_i32,
        )
        .is_err()
    );
}

#[test]
fn tensor_ops_huge_materializations_return_errors() {
    let scalar = Tensor::from_dense_i32(vec![1], vec![7]).expect("scalar-like tensor");
    let err = ops::broadcast_to(&scalar, vec![u64::MAX])
        .expect_err("huge broadcast should not allocate or panic");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    #[cfg(target_pointer_width = "64")]
    {
        let empty_wide = Tensor::from_dense_i32(vec![0, u64::MAX], Vec::new())
            .expect("zero-element huge-shape tensor");
        let err = ops::sum(&empty_wide, Some(&[0]), false)
            .expect_err("huge reduction output should not allocate or panic");
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }
}

#[test]
fn tensor_ops_empty_axis_stepped_slice_is_empty() {
    let empty = Tensor::from_dense_i32(vec![0], Vec::new()).expect("empty tensor");
    let sliced = ops::slice_axis_step(&empty, 0, 10, -10, -1)
        .expect("empty stepped slice should stay empty");
    assert_eq!(sliced.shape, vec![0]);
    assert_eq!(sliced.data, TensorData::I32(Vec::new()));
}

#[test]
fn create_options_validation_rejects_empty_rank() {
    let result = TensorFile::create(
        "unused.tio",
        CreateOptions::streaming(DType::F64, Vec::new(), 0),
    );
    let err = match result {
        Ok(_) => panic!("empty-rank create unexpectedly succeeded"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
}

#[test]
fn invalid_compression_mode_rejects_before_native_create() {
    let mut options =
        CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0);
    options.compression = Some(CompressionConfig {
        mode: 99,
        codec: sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
        min_payload_bytes: 0,
        zstd_level: 3,
    });
    let path = std::env::temp_dir().join("arcadia_tio_wrapper_invalid_compression_mode.tio");
    let _ = std::fs::remove_file(&path);
    let err = match TensorFile::create(&path, options) {
        Ok(_) => panic!("invalid mode unexpectedly succeeded"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);
    assert!(!path.exists());
}

#[test]
fn unsupported_raw_compression_codec_rejects_before_native_create() {
    let mut options =
        CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0);
    options.compression = Some(CompressionConfig {
        mode: sys::ARCADIA_TIO_COMPRESSION_FORCE_ON,
        codec: sys::ARCADIA_TIO_COMPRESSION_CODEC_LZ4,
        min_payload_bytes: 0,
        zstd_level: 3,
    });
    let path = std::env::temp_dir().join("arcadia_tio_wrapper_unsupported_compression_codec.tio");
    let _ = std::fs::remove_file(&path);
    let err = match TensorFile::create(&path, options) {
        Ok(_) => panic!("unsupported codec unexpectedly succeeded"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::Unimplemented);
    assert!(!path.exists());
}

#[test]
fn dtype_sizes_match_first_slice() {
    assert_eq!(DType::F32.size_bytes(), 4);
    assert_eq!(DType::F64.size_bytes(), 8);
    assert_eq!(DType::I32.size_bytes(), 4);
    assert_eq!(DType::I64.size_bytes(), 8);
}

#[test]
fn coordinate_v2_options_and_layout_set_raw_contract_fields() {
    let options = CoordinateV2Options {
        allow_authoritative_scan: true,
        include_dictionary_entries: true,
        include_index_summaries: true,
        allow_external_resolution: false,
    };
    let raw_options = options.to_raw();
    assert_eq!(
        raw_options.version,
        sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION
    );
    assert_eq!(
        raw_options.struct_size,
        mem::size_of::<sys::ArcadiaTioCoordinateV2Options>()
    );
    assert_eq!(raw_options.allow_authoritative_scan, 1);
    assert_eq!(raw_options.include_dictionary_entries, 1);
    assert_eq!(raw_options.include_index_summaries, 1);
    assert_eq!(raw_options.allow_external_resolution, 0);
    assert_eq!(raw_options.reserved_u8, [0; 4]);
    assert_eq!(raw_options.reserved, [0; 4]);

    let layout = CoordinateFixedTextLayoutV2 {
        width: 4,
        ..CoordinateFixedTextLayoutV2::default()
    };
    let raw_layout = layout.to_raw();
    assert_eq!(
        raw_layout.version,
        sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION
    );
    assert_eq!(
        raw_layout.struct_size,
        mem::size_of::<sys::ArcadiaTioCoordinateFixedTextLayoutV2>()
    );
    assert_eq!(raw_layout.width, 4);
    assert_eq!(raw_layout.reserved_u8, [0; 6]);
    assert_eq!(raw_layout.reserved, [0; 2]);
}

#[test]
fn coordinate_v2_input_prepare_sets_pointer_and_reserved_fields() {
    let mut input = AxisCoordinateInputV2::inline_i32(1, vec![10, 20]);
    input.descriptor_id = Some("trade-date".to_string());
    input.name = Some("trade_date".to_string());
    input.kind = CoordinateKind::Date;
    input.numeric_encoding = CoordinateEncoding::DateYyyymmdd;
    input.required = true;
    let prepared = input.prepare().expect("Coordinate v2 input prepares");
    let raw = prepared.raw();
    assert_eq!(raw.version, sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION);
    assert_eq!(
        raw.struct_size,
        mem::size_of::<sys::ArcadiaTioAxisCoordinateInputV2>()
    );
    assert_eq!(raw.axis, 1);
    assert!(!raw.descriptor_id.is_null());
    assert!(!raw.name.is_null());
    assert!(!raw.values.is_null());
    assert_eq!(raw.values_len, 2);
    assert_eq!(raw.required, 1);
    assert_eq!(raw.reserved_u8, [0; 7]);
    assert_eq!(raw.reserved, [0; 4]);
}

#[test]
fn coordinate_v2_lookup_and_append_prepare_raw_contract_fields() {
    let key = CoordinateLookupKeyV2::fixed_text_ascii("B", 4)
        .expect("fixed-text ASCII lookup key builds");
    let prepared_key = key.prepare().expect("lookup key prepares");
    let raw_key = prepared_key.raw();
    assert_eq!(raw_key.version, sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION);
    assert_eq!(
        raw_key.struct_size,
        mem::size_of::<sys::ArcadiaTioCoordinateLookupKeyV2>()
    );
    assert_eq!(
        raw_key.key_domain,
        sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_FIXED_TEXT
    );
    assert!(!raw_key.bytes.is_null());
    assert_eq!(raw_key.bytes_len, 1);
    assert_eq!(raw_key.fixed_text_width, 4);
    assert_eq!(raw_key.reserved, [0; 4]);

    let stable_key = CoordinateLookupKeyV2::stable_id("instrument-a");
    let prepared_stable = stable_key.prepare().expect("stable-id key prepares");
    let raw_stable = prepared_stable.raw();
    assert_eq!(
        raw_stable.version,
        sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION
    );
    assert_eq!(
        raw_stable.struct_size,
        mem::size_of::<sys::ArcadiaTioCoordinateLookupKeyV2>()
    );
    assert_eq!(
        raw_stable.key_domain,
        sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_STABLE_ID
    );
    assert!(raw_stable.bytes.is_null());
    assert!(!raw_stable.text.is_null());
    assert_eq!(raw_stable.reserved, [0; 4]);

    let raw_time = CoordinateLookupKeyV2::raw_time_i64(1778918400000000000);
    let prepared_raw_time = raw_time.prepare().expect("raw-time key prepares");
    let raw_raw_time = prepared_raw_time.raw();
    assert_eq!(
        raw_raw_time.key_domain,
        sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_RAW_TIME
    );
    assert_eq!(raw_raw_time.i64_value, 1778918400000000000);
    assert_eq!(raw_raw_time.reserved, [0; 4]);

    let batch = AppendCoordinateBatchV2 {
        entries: vec![AppendCoordinateEntryV2::i64(0, vec![100, 101])],
    };
    let prepared_batch = batch.prepare().expect("append batch prepares");
    let raw_batch = prepared_batch.raw();
    assert_eq!(
        raw_batch.version,
        sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION
    );
    assert_eq!(
        raw_batch.struct_size,
        mem::size_of::<sys::ArcadiaTioAppendCoordinateBatchV2>()
    );
    assert!(!raw_batch.entries.is_null());
    assert_eq!(raw_batch.entries_len, 1);
    assert_eq!(raw_batch.reserved, [0; 4]);
}

#[test]
fn coordinate_v2_lookup_result_mapping_preserves_status_fields() {
    let positions = [2u32, 4u32, 6u32];
    let reason = CString::new("duplicate display labels").expect("cstring");
    let raw = sys::ArcadiaTioCoordinateLookupResultV2 {
        version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioCoordinateLookupResultV2>(),
        status: sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_MANY,
        status_category: sys::ARCADIA_TIO_COORDINATE_STATUS_V2_DUPLICATE_UNIQUE_LOOKUP,
        unique_position: 0,
        range_start: 1,
        range_end: 7,
        positions: positions.as_ptr() as *mut u32,
        positions_len: positions.len(),
        availability: sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_AVAILABLE,
        reason: reason.as_ptr() as *mut c_char,
        reserved: [0; 4],
    };
    let mapped =
        unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) }.expect("lookup result maps");
    assert!(mapped.is_many());
    assert_eq!(mapped.status, CoordinateLookupResultStatusV2::Many);
    assert_eq!(
        mapped.status_category,
        CoordinateStatusCategoryV2::DuplicateUniqueLookup
    );
    assert_eq!(mapped.range_start, 1);
    assert_eq!(mapped.range_end, 7);
    assert_eq!(mapped.many_positions(), Some([2u32, 4u32, 6u32].as_slice()));
    assert_eq!(mapped.reason.as_deref(), Some("duplicate display labels"));
    assert_eq!(mapped.availability, CoordinateAvailabilityV2::Available);
}

#[test]
fn coordinate_v2_lookup_builders_reject_deferred_semantics() {
    let non_ascii = CoordinateLookupKeyV2::fixed_text_ascii("å", 4)
        .expect_err("non-ASCII fixed text is deferred");
    assert_eq!(non_ascii.code(), ErrorCode::InvalidArgument);
    let over_width = CoordinateLookupKeyV2::fixed_text_ascii("ABCDE", 4)
        .expect_err("over-width fixed text is rejected");
    assert_eq!(over_width.code(), ErrorCode::InvalidArgument);
    let variable =
        CoordinateLookupKeyV2::variable_string("abc").expect_err("variable strings are deferred");
    assert_eq!(variable.code(), ErrorCode::Unimplemented);
    let calendar = CoordinateLookupKeyV2::calendar_time("2026-06-01T00:00:00Z")
        .expect_err("calendar semantics are deferred");
    assert_eq!(calendar.code(), ErrorCode::Unimplemented);
    let resolver = CoordinateLookupKeyV2::external_resolver("symbol://ABC")
        .expect_err("external resolver semantics are deferred");
    assert_eq!(resolver.code(), ErrorCode::Unimplemented);
}

#[test]
fn coordinate_v2_numeric_append_sequence_zeroes_fixed_text_layout() {
    let mut input = AxisCoordinateInputV2::inline_i32(0, Vec::new());
    input.value_domain = CoordinateValueDomainV2::AppendSequence;
    input.values = CoordinateInputValuesV2::None;
    let prepared = input.prepare().expect("append-sequence input prepares");
    let raw = prepared.raw();
    assert_eq!(
        raw.value_domain,
        sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_APPEND_SEQUENCE
    );
    assert_eq!(raw.fixed_text.struct_size, 0);
    assert_eq!(raw.fixed_text.width, 0);
}

#[test]
fn coordinate_v2_fixed_text_append_uses_byte_element_size() {
    let entry = AppendCoordinateEntryV2 {
        axis: 0,
        descriptor_id: Some("venue".to_string()),
        name: Some("venue".to_string()),
        value_domain: CoordinateValueDomainV2::FixedText,
        numeric_dtype: CoordinateDType::I32,
        numeric_encoding: CoordinateEncoding::Plain,
        code_dtype: CoordinateCodeDTypeV2::U32,
        values: CoordinateInputValuesV2::FixedText(b"ABCDWXYZ".to_vec()),
        fixed_text_width: 4,
        dictionary_entries: Vec::new(),
    };
    let batch = AppendCoordinateBatchV2 {
        entries: vec![entry],
    };
    let prepared = batch.prepare().expect("fixed-text append prepares");
    let raw = prepared.raw();
    assert_eq!(raw.entries_len, 1);
    let raw_entry = unsafe { &*raw.entries };
    assert_eq!(raw_entry.count, 2);
    assert_eq!(raw_entry.fixed_text_width, 4);
    assert_eq!(raw_entry.element_size, mem::size_of::<u8>());
    assert!(!raw_entry.values.is_null());
}

#[test]
fn coordinate_v2_empty_buffers_use_null_raw_pointers() {
    let input = AxisCoordinateInputV2::inline_i32(0, Vec::new());
    let prepared_input = input.prepare().expect("empty inline input prepares");
    let raw_input = prepared_input.raw();
    assert_eq!(raw_input.values_len, 0);
    assert!(raw_input.values.is_null());

    let mut fixed_input = AxisCoordinateInputV2::inline_i32(1, Vec::new());
    fixed_input.value_domain = CoordinateValueDomainV2::FixedText;
    fixed_input.fixed_text.width = 4;
    fixed_input.values = CoordinateInputValuesV2::FixedText(Vec::new());
    let prepared_fixed = fixed_input.prepare().expect("empty fixed input prepares");
    let raw_fixed = prepared_fixed.raw();
    assert_eq!(raw_fixed.values_len, 0);
    assert!(raw_fixed.values.is_null());

    let dictionary_input = AxisCoordinateInputV2::dictionary_codes_u16(
        2,
        Vec::new(),
        CoordinateFixedTextLayoutV2::ascii_right_space_padded(4).expect("layout"),
        CoordinateDictionarySummaryV2::new(CoordinateCodeDTypeV2::U16)
            .with_dictionary_id("empty-codes"),
        vec![CoordinateDictionaryEntryV2::new(
            0,
            Some("ZERO".to_string()),
            Some("Zero".to_string()),
        )],
    )
    .expect("dictionary input builds");
    let prepared_dictionary = dictionary_input
        .prepare()
        .expect("empty dictionary input prepares");
    let raw_dictionary = prepared_dictionary.raw();
    assert_eq!(raw_dictionary.values_len, 0);
    assert!(raw_dictionary.values.is_null());

    let empty_append = AppendCoordinateBatchV2 {
        entries: vec![AppendCoordinateEntryV2::i32(0, Vec::new())],
    };
    let prepared_append = empty_append.prepare().expect("empty append prepares");
    let raw_append = prepared_append.raw();
    let raw_entry = unsafe { &*raw_append.entries };
    assert_eq!(raw_entry.count, 0);
    assert!(raw_entry.values.is_null());
}

#[test]
fn coordinate_v2_descriptor_builders_prepare_implemented_domains() {
    let fixed = AxisCoordinateInputV2::fixed_text_ascii(0, 4, ["AB", "XYZ"])
        .expect("fixed-text builder pads ASCII values");
    let prepared_fixed = fixed.prepare().expect("fixed-text descriptor prepares");
    assert_eq!(prepared_fixed.raw().fixed_text.width, 4);
    assert_eq!(prepared_fixed.raw().values_len, 2);

    let dictionary = AxisCoordinateInputV2::dictionary_codes_u32(
        1,
        vec![0, 1],
        CoordinateFixedTextLayoutV2::ascii_right_space_padded(3).expect("label layout"),
        CoordinateDictionarySummaryV2::new(CoordinateCodeDTypeV2::U32)
            .with_dictionary_id("symbols"),
        vec![CoordinateDictionaryEntryV2::new(
            0,
            Some("A".to_string()),
            Some("AAA".to_string()),
        )],
    )
    .expect("dictionary builder succeeds");
    let prepared_dictionary = dictionary
        .prepare()
        .expect("dictionary descriptor prepares");
    assert_eq!(
        prepared_dictionary.raw().code_dtype,
        CoordinateCodeDTypeV2::U32.to_raw()
    );
    assert!(!prepared_dictionary.raw().dictionary.is_null());

    let append = AxisCoordinateInputV2::append_fixed_text(
        0,
        CoordinateFixedTextLayoutV2::ascii_right_space_padded(6).expect("append layout"),
    )
    .expect("append fixed-text declaration builds");
    let prepared_append = append.prepare().expect("append descriptor prepares");
    assert_eq!(prepared_append.raw().values_len, 0);
    assert!(prepared_append.raw().values.is_null());

    let external = AxisCoordinateInputV2::external_reference_fixed_text(
        1,
        CoordinateExternalBindingV2::metadata_only(
            CoordinateSourceKindV2::SameFileObject,
            Some("coords-symbol".to_string()),
            Some("symbol coordinates".to_string()),
            CoordinateValueDomainV2::FixedText,
            2,
        ),
        CoordinateFixedTextLayoutV2::ascii_right_space_padded(6).expect("external layout"),
    )
    .expect("fixed-text external summary builds");
    let prepared_external = external.prepare().expect("external summary prepares");
    assert_eq!(
        prepared_external.raw().value_domain,
        sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_EXTERNAL_REFERENCE
    );
    assert!(!prepared_external.raw().external_binding.is_null());

    let external_dictionary = AxisCoordinateInputV2::external_reference_dictionary_codes(
        1,
        CoordinateExternalBindingV2::metadata_only(
            CoordinateSourceKindV2::SameFileObject,
            Some("coords-symbol-code".to_string()),
            Some("symbol code coordinates".to_string()),
            CoordinateValueDomainV2::DictionaryCode,
            2,
        ),
        CoordinateCodeDTypeV2::U16,
    )
    .expect("external dictionary-code summary builds");
    let prepared_external_dictionary = external_dictionary
        .prepare()
        .expect("external dictionary-code descriptor prepares");
    assert!(prepared_external_dictionary.raw().dictionary.is_null());
    assert_eq!(
        prepared_external_dictionary.raw().code_dtype,
        CoordinateCodeDTypeV2::U16.to_raw()
    );

    let create_inputs = [fixed, dictionary, append, external, external_dictionary];
    let create_prepared = PreparedAxisCoordinateInputsV2::new(&create_inputs, 2)
        .expect("create helper preparation keeps builder descriptors FFI-ready");
    assert_eq!(create_prepared.len(), 5);
    let raw = unsafe { slice::from_raw_parts(create_prepared.ptr(), create_prepared.len()) };
    assert!(raw.iter().all(|item| !item.descriptor_id.is_null()));
}

#[test]
fn coordinate_v2_create_validation_rejects_unsupported_semantics() {
    let mut missing_id = AxisCoordinateInputV2::inline_i32(0, vec![1]);
    missing_id.descriptor_id = None;
    let err = match missing_id.prepare() {
        Ok(_) => panic!("missing descriptor id unexpectedly prepared"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = AxisCoordinateInputV2::fixed_text_ascii(0, 2, ["ABC"])
        .expect_err("over-width fixed text rejects before native create");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let bad_dictionary = AxisCoordinateInputV2::dictionary_codes_u32(
        0,
        vec![0],
        CoordinateFixedTextLayoutV2::ascii_right_space_padded(2).expect("layout"),
        CoordinateDictionarySummaryV2::new(CoordinateCodeDTypeV2::U32),
        Vec::new(),
    )
    .expect("builder permits validation to report required dictionary fields");
    let err = match bad_dictionary.prepare() {
        Ok(_) => panic!("dictionary descriptor without id/entries unexpectedly prepared"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let bad_external = AxisCoordinateInputV2::external_reference(
        0,
        CoordinateExternalBindingV2::metadata_only(
            CoordinateSourceKindV2::SameFileObject,
            Option::<String>::None,
            Some("missing logical id".to_string()),
            CoordinateValueDomainV2::InlineNumeric,
            1,
        ),
    );
    let err = match bad_external.prepare() {
        Ok(_) => panic!("external descriptor without logical_id unexpectedly prepared"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let mut ignored_external_dictionary =
        AxisCoordinateInputV2::external_reference_dictionary_codes(
            0,
            CoordinateExternalBindingV2::metadata_only(
                CoordinateSourceKindV2::SameFileObject,
                Some("coords-codes".to_string()),
                Some("codes".to_string()),
                CoordinateValueDomainV2::DictionaryCode,
                1,
            ),
            CoordinateCodeDTypeV2::U32,
        )
        .expect("external dictionary-code builder succeeds");
    ignored_external_dictionary.dictionary = Some(
        CoordinateDictionarySummaryV2::new(CoordinateCodeDTypeV2::U32)
            .with_dictionary_id("ignored"),
    );
    ignored_external_dictionary.dictionary_entries = vec![CoordinateDictionaryEntryV2::new(
        0,
        Some("ZERO".to_string()),
        Some("Zero".to_string()),
    )];
    let err = match ignored_external_dictionary.prepare() {
        Ok(_) => panic!("ignored external dictionary metadata unexpectedly prepared"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let external = AxisCoordinateInputV2::external_reference(
        0,
        CoordinateExternalBindingV2::metadata_only(
            CoordinateSourceKindV2::ApplicationRegistry,
            Some("resolver-key".to_string()),
            Some("resolver".to_string()),
            CoordinateValueDomainV2::InlineNumeric,
            1,
        ),
    );
    let err = match external.prepare() {
        Ok(_) => panic!("application-registry resolver semantics unexpectedly prepared"),
        Err(err) => err,
    };
    assert_eq!(err.code(), ErrorCode::Unimplemented);

    let mut create_options =
        CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0);
    create_options.coordinates.push(CoordinateSpec {
        axis: 0,
        name: None,
        kind: CoordinateKind::DomainValue,
        encoding: CoordinateEncoding::Plain,
        storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![1])),
        ordering: CoordinateOrdering::default(),
        required: false,
    });
    let err = validate_create_with_coordinates_v2_options(
        &create_options,
        CoordinateV2Options::default(),
    )
    .expect_err("v1/v2 coordinate descriptors cannot mix");
    assert_eq!(err.code(), ErrorCode::InvalidArgument);

    let err = validate_create_with_coordinates_v2_options(
        &CreateOptions::streaming(DType::F64, vec![DimSpec::new(AxisKind::Time, 0)], 0),
        CoordinateV2Options {
            allow_external_resolution: true,
            ..CoordinateV2Options::default()
        },
    )
    .expect_err("external resolution rejects");
    assert_eq!(err.code(), ErrorCode::Unimplemented);
}
