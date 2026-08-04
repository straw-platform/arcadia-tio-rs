use super::{Result, Tensor, TensorElement, TensorI64, TypedTensor, ops};

fn typed_from_result<T: TensorElement>(result: Result<Tensor>) -> Result<TypedTensor<T>> {
    TypedTensor::try_from_tensor(result?)
}

fn typed_vec_from_result<T: TensorElement>(
    result: Result<Vec<Tensor>>,
) -> Result<Vec<TypedTensor<T>>> {
    let tensors = result?;
    let mut out = Vec::with_capacity(tensors.len());
    for tensor in tensors {
        out.push(TypedTensor::try_from_tensor(tensor)?);
    }
    Ok(out)
}

fn tensor_refs<'a, T: TensorElement>(tensors: &'a [&'a TypedTensor<T>]) -> Vec<&'a Tensor> {
    let mut refs = Vec::with_capacity(tensors.len());
    for tensor in tensors {
        refs.push(tensor.as_tensor());
    }
    refs
}

/// Validate and clone an already-owned row-major typed tensor.
pub fn to_contiguous<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::to_contiguous(tensor.as_tensor()))
}

/// Reshape a typed tensor in row-major order.
pub fn reshape<T: TensorElement>(
    tensor: &TypedTensor<T>,
    shape: Vec<u64>,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::reshape(tensor.as_tensor(), shape))
}

/// Flatten a typed tensor to one dimension.
pub fn flatten<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::flatten(tensor.as_tensor()))
}

/// Owned alias for [`flatten`].
pub fn ravel_view<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::ravel_view(tensor.as_tensor()))
}

/// Insert a length-1 axis.
pub fn expand_dims<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::expand_dims(tensor.as_tensor(), axis))
}

/// Remove all length-1 axes.
pub fn squeeze<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::squeeze(tensor.as_tensor()))
}

/// Remove one length-1 axis.
pub fn squeeze_axis<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::squeeze_axis(tensor.as_tensor(), axis))
}

/// Permute axes and materialize an owned typed tensor.
pub fn permute_axes<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: &[isize],
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::permute_axes(tensor.as_tensor(), axes))
}

/// Owned alias for [`permute_axes`].
pub fn permute_axes_view<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: &[isize],
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::permute_axes_view(tensor.as_tensor(), axes))
}

/// Swap two axes and materialize an owned typed tensor.
pub fn swap_axes<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis_a: isize,
    axis_b: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::swap_axes(tensor.as_tensor(), axis_a, axis_b))
}

/// Owned alias for [`swap_axes`].
pub fn swap_axes_view<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis_a: isize,
    axis_b: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::swap_axes_view(tensor.as_tensor(), axis_a, axis_b))
}

/// Transpose a rank-2 typed tensor.
pub fn transpose<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::transpose(tensor.as_tensor()))
}

/// Owned alias for [`transpose`].
pub fn transpose_view<T: TensorElement>(tensor: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::transpose_view(tensor.as_tensor()))
}

/// Move one axis to a new position.
pub fn move_axis<T: TensorElement>(
    tensor: &TypedTensor<T>,
    source: isize,
    destination: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::move_axis(tensor.as_tensor(), source, destination))
}

/// Owned alias for [`move_axis`].
pub fn move_axis_view<T: TensorElement>(
    tensor: &TypedTensor<T>,
    source: isize,
    destination: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::move_axis_view(tensor.as_tensor(), source, destination))
}

/// Broadcast to a target shape and materialize an owned typed tensor.
pub fn broadcast_to<T: TensorElement>(
    tensor: &TypedTensor<T>,
    shape: Vec<u64>,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::broadcast_to(tensor.as_tensor(), shape))
}

/// Slice one axis by a half-open range.
pub fn slice_axis<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    start: usize,
    end: usize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::slice_axis(tensor.as_tensor(), axis, start, end))
}

/// Slice one axis by a stepped range.
pub fn slice_axis_step<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    start: isize,
    end: isize,
    step: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::slice_axis_step(
        tensor.as_tensor(),
        axis,
        start,
        end,
        step,
    ))
}

/// Take explicit indices along one axis.
pub fn take_axis<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    indices: &[usize],
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::take_axis(tensor.as_tensor(), axis, indices))
}

/// Select one index along an axis while retaining a length-1 axis.
pub fn index_axis<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    index: usize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::index_axis(tensor.as_tensor(), axis, index))
}

/// Concatenate typed tensors along one existing axis.
pub fn concat<T: TensorElement>(
    tensors: &[&TypedTensor<T>],
    axis: isize,
) -> Result<TypedTensor<T>> {
    let refs = tensor_refs(tensors);
    typed_from_result(ops::concat(&refs, axis))
}

/// Stack typed tensors along a new axis.
pub fn stack<T: TensorElement>(tensors: &[&TypedTensor<T>], axis: isize) -> Result<TypedTensor<T>> {
    let refs = tensor_refs(tensors);
    typed_from_result(ops::stack(&refs, axis))
}

/// Split a typed tensor into typed tensors along one axis.
pub fn split<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    sections: &[usize],
) -> Result<Vec<TypedTensor<T>>> {
    typed_vec_from_result(ops::split(tensor.as_tensor(), axis, sections))
}

/// Unstack a typed tensor along one axis.
pub fn unstack<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
) -> Result<Vec<TypedTensor<T>>> {
    typed_vec_from_result(ops::unstack(tensor.as_tensor(), axis))
}

/// Repeat each element along one axis.
pub fn repeat<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    repeats: usize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::repeat(tensor.as_tensor(), axis, repeats))
}

/// Tile a typed tensor by per-axis repeat counts.
pub fn tile<T: TensorElement>(tensor: &TypedTensor<T>, reps: &[usize]) -> Result<TypedTensor<T>> {
    typed_from_result(ops::tile(tensor.as_tensor(), reps))
}

/// Reverse one axis.
pub fn flip<T: TensorElement>(tensor: &TypedTensor<T>, axis: isize) -> Result<TypedTensor<T>> {
    typed_from_result(ops::flip(tensor.as_tensor(), axis))
}

/// Roll one axis by a signed shift.
pub fn roll<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: isize,
    shift: isize,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::roll(tensor.as_tensor(), axis, shift))
}

/// Add a scalar of the same dtype to every element.
pub fn add_scalar<T: TensorElement>(tensor: &TypedTensor<T>, rhs: T) -> Result<TypedTensor<T>> {
    typed_from_result(ops::add_scalar(tensor.as_tensor(), rhs.into_scalar()))
}

/// Subtract a scalar of the same dtype from every element.
pub fn sub_scalar<T: TensorElement>(tensor: &TypedTensor<T>, rhs: T) -> Result<TypedTensor<T>> {
    typed_from_result(ops::sub_scalar(tensor.as_tensor(), rhs.into_scalar()))
}

/// Multiply every element by a scalar of the same dtype.
pub fn mul_scalar<T: TensorElement>(tensor: &TypedTensor<T>, rhs: T) -> Result<TypedTensor<T>> {
    typed_from_result(ops::mul_scalar(tensor.as_tensor(), rhs.into_scalar()))
}

/// Divide every element by a scalar of the same dtype.
pub fn div_scalar<T: TensorElement>(tensor: &TypedTensor<T>, rhs: T) -> Result<TypedTensor<T>> {
    typed_from_result(ops::div_scalar(tensor.as_tensor(), rhs.into_scalar()))
}

/// Add typed tensors with exact dtype matching and broadcasting.
pub fn add<T: TensorElement>(lhs: &TypedTensor<T>, rhs: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::add(lhs.as_tensor(), rhs.as_tensor()))
}

/// Subtract typed tensors with exact dtype matching and broadcasting.
pub fn sub<T: TensorElement>(lhs: &TypedTensor<T>, rhs: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::sub(lhs.as_tensor(), rhs.as_tensor()))
}

/// Multiply typed tensors with exact dtype matching and broadcasting.
pub fn mul<T: TensorElement>(lhs: &TypedTensor<T>, rhs: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::mul(lhs.as_tensor(), rhs.as_tensor()))
}

/// Divide typed tensors with exact dtype matching and broadcasting.
pub fn div<T: TensorElement>(lhs: &TypedTensor<T>, rhs: &TypedTensor<T>) -> Result<TypedTensor<T>> {
    typed_from_result(ops::div(lhs.as_tensor(), rhs.as_tensor()))
}

/// Sum values across selected axes while preserving dtype.
pub fn sum<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: Option<&[isize]>,
    keepdims: bool,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::sum(tensor.as_tensor(), axes, keepdims))
}

/// Minimum values across selected axes while preserving dtype.
pub fn min<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: Option<&[isize]>,
    keepdims: bool,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::min(tensor.as_tensor(), axes, keepdims))
}

/// Maximum values across selected axes while preserving dtype.
pub fn max<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: Option<&[isize]>,
    keepdims: bool,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::max(tensor.as_tensor(), axes, keepdims))
}

/// Zero-based argmin offsets across selected axes.
pub fn argmin<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: Option<&[isize]>,
    keepdims: bool,
) -> Result<TensorI64> {
    typed_from_result(ops::argmin(tensor.as_tensor(), axes, keepdims))
}

/// Zero-based argmax offsets across selected axes.
pub fn argmax<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axes: Option<&[isize]>,
    keepdims: bool,
) -> Result<TensorI64> {
    typed_from_result(ops::argmax(tensor.as_tensor(), axes, keepdims))
}

/// Cumulative sum along one axis, or over the flattened tensor when `axis = None`.
pub fn cumsum<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: Option<isize>,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::cumsum(tensor.as_tensor(), axis))
}

/// Cumulative product along one axis, or over the flattened tensor when `axis = None`.
pub fn cumprod<T: TensorElement>(
    tensor: &TypedTensor<T>,
    axis: Option<isize>,
) -> Result<TypedTensor<T>> {
    typed_from_result(ops::cumprod(tensor.as_tensor(), axis))
}
