use super::*;

unsafe extern "C" {
    /// Reads the full tensor into a native-owned raw tensor.
    pub fn arcadia_tio_read_all(
        handle: *mut ArcadiaTioHandle,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads the full tensor values as native-owned Arrow C Data array/schema carriers.
    pub fn arcadia_tio_read_values_arrow(
        handle: *mut ArcadiaTioHandle,
        out_array: *mut ArrowArray,
        out_schema: *mut ArrowSchema,
    ) -> c_int;
    /// Reads the full tensor into a dense tensor and optional native-owned mask.
    pub fn arcadia_tio_read_all_dense(
        handle: *mut ArcadiaTioHandle,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
    ) -> c_int;
    /// Frees native-owned tensor buffers.
    pub fn arcadia_tio_tensor_free(tensor: *mut ArcadiaTioTensor);
    /// Frees native-owned mask buffers.
    pub fn arcadia_tio_mask_free(mask: *mut ArcadiaTioMask);
    /// Materializes a copy-only contiguous tensor.
    pub fn arcadia_tio_tensor_to_contiguous(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reshapes a tensor in row-major order.
    pub fn arcadia_tio_tensor_reshape(
        input: *const ArcadiaTioTensor,
        shape: *const u64,
        rank: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Flattens a tensor to shape `[numel]`.
    pub fn arcadia_tio_tensor_flatten(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Inserts a length-1 axis.
    pub fn arcadia_tio_tensor_expand_dims(
        input: *const ArcadiaTioTensor,
        axis: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Removes all length-1 axes.
    pub fn arcadia_tio_tensor_squeeze(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Removes one length-1 axis.
    pub fn arcadia_tio_tensor_squeeze_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Permutes axes and materializes row-major output.
    pub fn arcadia_tio_tensor_permute_axes(
        input: *const ArcadiaTioTensor,
        axes: *const i64,
        axes_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reverses axis order and materializes row-major output.
    pub fn arcadia_tio_tensor_transpose(
        input: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Slices one axis using `[start, end)`.
    pub fn arcadia_tio_tensor_slice_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        start: u64,
        end: u64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Slices one axis with a non-zero step.
    pub fn arcadia_tio_tensor_slice_axis_step(
        input: *const ArcadiaTioTensor,
        axis: i64,
        start: i64,
        end: i64,
        step: i64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Takes explicit indices on one axis.
    pub fn arcadia_tio_tensor_take_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        indices: *const u64,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Selects one index on an axis while preserving rank with axis length 1.
    pub fn arcadia_tio_tensor_index_axis(
        input: *const ArcadiaTioTensor,
        axis: i64,
        index: u64,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Adds two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_add(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Subtracts two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_sub(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Multiplies two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_mul(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Divides two floating-point tensors with exact dtype matching and broadcasting.
    pub fn arcadia_tio_tensor_div(
        lhs: *const ArcadiaTioTensor,
        rhs: *const ArcadiaTioTensor,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Adds a floating-point scalar to a tensor.
    pub fn arcadia_tio_tensor_add_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Subtracts a floating-point scalar from a tensor.
    pub fn arcadia_tio_tensor_sub_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Multiplies a tensor by a floating-point scalar.
    pub fn arcadia_tio_tensor_mul_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Divides a tensor by a floating-point scalar.
    pub fn arcadia_tio_tensor_div_scalar(
        input: *const ArcadiaTioTensor,
        rhs: c_double,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
}
