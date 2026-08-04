use super::{
    DType, Result, Scalar, Tensor, TensorData, TioError, shape_element_len, validate_tensor_parts,
};

/// Validate and clone an already-owned row-major tensor.
pub fn to_contiguous(tensor: &Tensor) -> Result<Tensor> {
    tensor.validate()?;
    Ok(tensor.clone())
}

/// Reshape a tensor in row-major order, preserving dtype and payload order.
pub fn reshape(tensor: &Tensor, shape: Vec<u64>) -> Result<Tensor> {
    tensor.validate()?;
    validate_shape_rank(&shape)?;
    let expected = shape_element_len(&shape)?;
    if expected != tensor.data.len() {
        return Err(TioError::invalid_argument(format!(
            "reshape element count {expected} does not match tensor element count {}",
            tensor.data.len()
        )));
    }
    tensor_from_data(tensor.dtype, shape, tensor.data.clone())
}

/// Flatten a tensor to a one-dimensional owned tensor.
pub fn flatten(tensor: &Tensor) -> Result<Tensor> {
    tensor.validate()?;
    reshape(tensor, vec![usize_to_u64(tensor.data.len())?])
}

/// Owned alias for [`flatten`].
pub fn ravel_view(tensor: &Tensor) -> Result<Tensor> {
    flatten(tensor)
}

/// Insert a length-1 axis at `axis`.
pub fn expand_dims(tensor: &Tensor, axis: isize) -> Result<Tensor> {
    tensor.validate()?;
    let mut shape = tensor.shape.clone();
    let axis = normalize_insert_axis(axis, shape.len())?;
    shape.insert(axis, 1);
    tensor_from_data(tensor.dtype, shape, tensor.data.clone())
}

/// Remove all length-1 axes.
pub fn squeeze(tensor: &Tensor) -> Result<Tensor> {
    tensor.validate()?;
    let shape: Vec<u64> = tensor
        .shape
        .iter()
        .copied()
        .filter(|&dim| dim != 1)
        .collect();
    if shape.is_empty() {
        return Err(TioError::invalid_argument(
            "squeeze would produce a rank-0 tensor",
        ));
    }
    tensor_from_data(tensor.dtype, shape, tensor.data.clone())
}

/// Remove a length-1 axis.
pub fn squeeze_axis(tensor: &Tensor, axis: isize) -> Result<Tensor> {
    tensor.validate()?;
    let axis = normalize_axis(axis, tensor.shape.len())?;
    if tensor.shape[axis] != 1 {
        return Err(TioError::invalid_argument(
            "squeeze axis must have length 1",
        ));
    }
    let mut shape = tensor.shape.clone();
    shape.remove(axis);
    if shape.is_empty() {
        return Err(TioError::invalid_argument(
            "squeeze would produce a rank-0 tensor",
        ));
    }
    tensor_from_data(tensor.dtype, shape, tensor.data.clone())
}

/// Permute axes and materialize row-major output.
pub fn permute_axes(tensor: &Tensor, axes: &[isize]) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    if axes.len() != shape.len() {
        return Err(TioError::invalid_argument(
            "permute axes length must equal tensor rank",
        ));
    }
    let normalized = normalize_axes(axes.iter().copied(), shape.len())?;
    let out_shape_usize: Vec<usize> = normalized.iter().map(|&axis| shape[axis]).collect();
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(permute_values(
            values,
            &shape,
            &normalized,
            &out_shape_usize,
        )?),
        TensorData::F64(values) => TensorData::F64(permute_values(
            values,
            &shape,
            &normalized,
            &out_shape_usize,
        )?),
        TensorData::I32(values) => TensorData::I32(permute_values(
            values,
            &shape,
            &normalized,
            &out_shape_usize,
        )?),
        TensorData::I64(values) => TensorData::I64(permute_values(
            values,
            &shape,
            &normalized,
            &out_shape_usize,
        )?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Owned alias for [`permute_axes`].
pub fn permute_axes_view(tensor: &Tensor, axes: &[isize]) -> Result<Tensor> {
    permute_axes(tensor, axes)
}

/// Swap two axes and materialize row-major output.
pub fn swap_axes(tensor: &Tensor, axis_a: isize, axis_b: isize) -> Result<Tensor> {
    tensor.validate()?;
    let rank = tensor.shape.len();
    let axis_a = normalize_axis(axis_a, rank)?;
    let axis_b = normalize_axis(axis_b, rank)?;
    let mut axes: Vec<isize> = (0..rank)
        .map(|axis| isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow")))
        .collect::<Result<Vec<_>>>()?;
    axes.swap(axis_a, axis_b);
    permute_axes(tensor, &axes)
}

/// Owned alias for [`swap_axes`].
pub fn swap_axes_view(tensor: &Tensor, axis_a: isize, axis_b: isize) -> Result<Tensor> {
    swap_axes(tensor, axis_a, axis_b)
}

/// Reverse axis order and materialize row-major output.
pub fn transpose(tensor: &Tensor) -> Result<Tensor> {
    tensor.validate()?;
    let rank = tensor.shape.len();
    let axes: Vec<isize> = (0..rank)
        .rev()
        .map(|axis| isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow")))
        .collect::<Result<Vec<_>>>()?;
    permute_axes(tensor, &axes)
}

/// Owned alias for [`transpose`].
pub fn transpose_view(tensor: &Tensor) -> Result<Tensor> {
    transpose(tensor)
}

/// Move one axis to a new position and materialize row-major output.
pub fn move_axis(tensor: &Tensor, source: isize, destination: isize) -> Result<Tensor> {
    tensor.validate()?;
    let rank = tensor.shape.len();
    let source = normalize_axis(source, rank)?;
    let destination = normalize_axis(destination, rank)?;
    let mut axes: Vec<usize> = (0..rank).collect();
    let moved = axes.remove(source);
    axes.insert(destination, moved);
    let axes: Vec<isize> = axes
        .into_iter()
        .map(|axis| isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow")))
        .collect::<Result<Vec<_>>>()?;
    permute_axes(tensor, &axes)
}

/// Owned alias for [`move_axis`].
pub fn move_axis_view(tensor: &Tensor, source: isize, destination: isize) -> Result<Tensor> {
    move_axis(tensor, source, destination)
}

/// Broadcast a tensor to `shape` and materialize the result.
pub fn broadcast_to(tensor: &Tensor, shape: Vec<u64>) -> Result<Tensor> {
    let input_shape = validated_shape(tensor)?;
    validate_shape_rank(&shape)?;
    let target_shape = shape_u64_to_usize(&shape)?;
    if broadcast_shape(&input_shape, &target_shape)? != target_shape {
        return Err(TioError::invalid_argument(
            "target shape is not broadcast-compatible",
        ));
    }
    let data = match &tensor.data {
        TensorData::F32(values) => {
            TensorData::F32(broadcast_values(values, &input_shape, &target_shape)?)
        }
        TensorData::F64(values) => {
            TensorData::F64(broadcast_values(values, &input_shape, &target_shape)?)
        }
        TensorData::I32(values) => {
            TensorData::I32(broadcast_values(values, &input_shape, &target_shape)?)
        }
        TensorData::I64(values) => {
            TensorData::I64(broadcast_values(values, &input_shape, &target_shape)?)
        }
    };
    tensor_from_data(tensor.dtype, shape, data)
}

/// Select a half-open range `[start, end)` along one axis.
pub fn slice_axis(tensor: &Tensor, axis: isize, start: usize, end: usize) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    if start > end || end > shape[axis] {
        return Err(TioError::invalid_argument("slice out of bounds"));
    }
    let indices: Vec<usize> = (start..end).collect();
    take_axis_normalized(tensor, &shape, axis, &indices)
}

/// Select a stepped slice along one axis. Negative starts/ends follow Python-style bounds.
pub fn slice_axis_step(
    tensor: &Tensor,
    axis: isize,
    start: isize,
    end: isize,
    step: isize,
) -> Result<Tensor> {
    if step == 0 {
        return Err(TioError::invalid_argument("slice step cannot be zero"));
    }
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    let indices = strided_indices(shape[axis], start, end, step)?;
    take_axis_normalized(tensor, &shape, axis, &indices)
}

/// Take explicit indices along one axis.
pub fn take_axis(tensor: &Tensor, axis: isize, indices: &[usize]) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    take_axis_normalized(tensor, &shape, axis, indices)
}

/// Take one index along an axis, preserving rank with axis length 1.
pub fn index_axis(tensor: &Tensor, axis: isize, index: usize) -> Result<Tensor> {
    take_axis(tensor, axis, &[index])
}

macro_rules! concat_variant_data {
    ($tensors:expr, $shapes:expr, $axis:expr, $out_shape:expr, $variant:ident, $ty:ty) => {{
        let mut inputs: Vec<DenseInput<'_, $ty>> =
            fallible_vec_with_capacity($tensors.len(), "tensor concat metadata")?;
        for (&tensor, shape) in $tensors.iter().zip($shapes.iter()) {
            match &tensor.data {
                TensorData::$variant(values) => inputs.push(DenseInput { shape, values }),
                _ => return Err(TioError::invalid_argument("tensor payload dtype mismatch")),
            }
        }
        Ok(TensorData::$variant(concat_values(
            &inputs,
            $axis,
            &$out_shape,
        )?))
    }};
}

macro_rules! stack_variant_data {
    ($tensors:expr, $shapes:expr, $axis:expr, $out_shape:expr, $variant:ident, $ty:ty) => {{
        let mut inputs: Vec<DenseInput<'_, $ty>> =
            fallible_vec_with_capacity($tensors.len(), "tensor stack metadata")?;
        for (&tensor, shape) in $tensors.iter().zip($shapes.iter()) {
            match &tensor.data {
                TensorData::$variant(values) => inputs.push(DenseInput { shape, values }),
                _ => return Err(TioError::invalid_argument("tensor payload dtype mismatch")),
            }
        }
        Ok(TensorData::$variant(stack_values(
            &inputs,
            $axis,
            &$out_shape,
        )?))
    }};
}

/// Concatenate tensors along one existing axis and materialize an owned row-major output.
pub fn concat(tensors: &[&Tensor], axis: isize) -> Result<Tensor> {
    let first = tensors
        .first()
        .copied()
        .ok_or_else(|| TioError::invalid_argument("concat requires at least one tensor"))?;
    let first_shape = validated_shape(first)?;
    let rank = first_shape.len();
    let axis = normalize_axis(axis, rank)?;
    let dtype = first.dtype;

    let mut shapes: Vec<Vec<usize>> =
        fallible_vec_with_capacity(tensors.len(), "tensor concat metadata")?;
    let mut out_shape_usize = first_shape.clone();
    out_shape_usize[axis] = 0;

    for &tensor in tensors {
        let shape = validated_shape(tensor)?;
        if tensor.dtype != dtype {
            return Err(TioError::invalid_argument("tensor dtype mismatch"));
        }
        if shape.len() != rank {
            return Err(TioError::invalid_argument("concat rank mismatch"));
        }
        for dim_axis in 0..rank {
            if dim_axis != axis && shape[dim_axis] != first_shape[dim_axis] {
                return Err(TioError::invalid_argument("concat shapes mismatch"));
            }
        }
        out_shape_usize[axis] = out_shape_usize[axis]
            .checked_add(shape[axis])
            .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
        shapes.push(shape);
    }

    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match dtype {
        DType::F32 => concat_variant_data!(tensors, shapes, axis, out_shape_usize, F32, f32),
        DType::F64 => concat_variant_data!(tensors, shapes, axis, out_shape_usize, F64, f64),
        DType::I32 => concat_variant_data!(tensors, shapes, axis, out_shape_usize, I32, i32),
        DType::I64 => concat_variant_data!(tensors, shapes, axis, out_shape_usize, I64, i64),
    }?;
    tensor_from_data(dtype, out_shape, data)
}

/// Stack tensors along a new axis and materialize an owned row-major output.
pub fn stack(tensors: &[&Tensor], axis: isize) -> Result<Tensor> {
    let first = tensors
        .first()
        .copied()
        .ok_or_else(|| TioError::invalid_argument("stack requires at least one tensor"))?;
    let first_shape = validated_shape(first)?;
    let rank = first_shape.len();
    let insert_axis = normalize_insert_axis(axis, rank)?;
    let dtype = first.dtype;

    let mut shapes: Vec<Vec<usize>> =
        fallible_vec_with_capacity(tensors.len(), "tensor stack metadata")?;
    for &tensor in tensors {
        let shape = validated_shape(tensor)?;
        if tensor.dtype != dtype {
            return Err(TioError::invalid_argument("tensor dtype mismatch"));
        }
        if shape != first_shape {
            return Err(TioError::invalid_argument("stack shapes mismatch"));
        }
        shapes.push(shape);
    }

    let mut out_shape_usize = first_shape.clone();
    out_shape_usize.insert(insert_axis, tensors.len());
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match dtype {
        DType::F32 => {
            stack_variant_data!(tensors, shapes, insert_axis, out_shape_usize, F32, f32)
        }
        DType::F64 => {
            stack_variant_data!(tensors, shapes, insert_axis, out_shape_usize, F64, f64)
        }
        DType::I32 => {
            stack_variant_data!(tensors, shapes, insert_axis, out_shape_usize, I32, i32)
        }
        DType::I64 => {
            stack_variant_data!(tensors, shapes, insert_axis, out_shape_usize, I64, i64)
        }
    }?;
    tensor_from_data(dtype, out_shape, data)
}

/// Split one axis into explicit section lengths.
pub fn split(tensor: &Tensor, axis: isize, sections: &[usize]) -> Result<Vec<Tensor>> {
    if sections.is_empty() {
        return Err(TioError::invalid_argument("split sections cannot be empty"));
    }
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    let total = sections.iter().try_fold(0usize, |acc, &value| {
        acc.checked_add(value)
            .ok_or_else(|| TioError::invalid_argument("shape product overflow"))
    })?;
    if total != shape[axis] {
        return Err(TioError::invalid_argument(
            "split sections must sum to axis length",
        ));
    }

    let mut out: Vec<Tensor> = fallible_vec_with_capacity(sections.len(), "tensor split outputs")?;
    let mut start = 0usize;
    for &len in sections {
        let end = start
            .checked_add(len)
            .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
        out.push(slice_axis_range_normalized(
            tensor, &shape, axis, start, end,
        )?);
        start = end;
    }
    Ok(out)
}

/// Split a tensor into one tensor per index along an axis.
pub fn unstack(tensor: &Tensor, axis: isize) -> Result<Vec<Tensor>> {
    let shape = validated_shape(tensor)?;
    if shape.len() == 1 {
        return Err(TioError::invalid_argument(
            "unstack rank-1 tensor would produce rank-0 outputs",
        ));
    }
    let axis = normalize_axis(axis, shape.len())?;
    let mut out: Vec<Tensor> = fallible_vec_with_capacity(shape[axis], "tensor unstack outputs")?;
    for index in 0..shape[axis] {
        let indexed = take_axis_normalized(tensor, &shape, axis, &[index])?;
        out.push(squeeze_axis(&indexed, axis as isize)?);
    }
    Ok(out)
}

/// Repeat each element along one axis.
pub fn repeat(tensor: &Tensor, axis: isize, repeats: usize) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    let mut out_shape_usize = shape.clone();
    out_shape_usize[axis] = out_shape_usize[axis]
        .checked_mul(repeats)
        .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(repeat_values(
            values,
            &shape,
            &out_shape_usize,
            axis,
            repeats,
        )?),
        TensorData::F64(values) => TensorData::F64(repeat_values(
            values,
            &shape,
            &out_shape_usize,
            axis,
            repeats,
        )?),
        TensorData::I32(values) => TensorData::I32(repeat_values(
            values,
            &shape,
            &out_shape_usize,
            axis,
            repeats,
        )?),
        TensorData::I64(values) => TensorData::I64(repeat_values(
            values,
            &shape,
            &out_shape_usize,
            axis,
            repeats,
        )?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Tile a tensor by repeat factors on each axis.
pub fn tile(tensor: &Tensor, reps: &[usize]) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    if reps.len() != shape.len() {
        return Err(TioError::invalid_argument(
            "tile reps length must equal tensor rank",
        ));
    }
    let mut out_shape_usize: Vec<usize> =
        fallible_vec_with_capacity(reps.len(), "tensor tile shape")?;
    for (&dim, &rep) in shape.iter().zip(reps) {
        out_shape_usize.push(
            dim.checked_mul(rep)
                .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?,
        );
    }
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(tile_values(values, &shape, &out_shape_usize)?),
        TensorData::F64(values) => TensorData::F64(tile_values(values, &shape, &out_shape_usize)?),
        TensorData::I32(values) => TensorData::I32(tile_values(values, &shape, &out_shape_usize)?),
        TensorData::I64(values) => TensorData::I64(tile_values(values, &shape, &out_shape_usize)?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Reverse one axis and materialize an owned row-major output.
pub fn flip(tensor: &Tensor, axis: isize) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    let out_shape = tensor.shape.clone();
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(flip_values(values, &shape, axis)?),
        TensorData::F64(values) => TensorData::F64(flip_values(values, &shape, axis)?),
        TensorData::I32(values) => TensorData::I32(flip_values(values, &shape, axis)?),
        TensorData::I64(values) => TensorData::I64(flip_values(values, &shape, axis)?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Circularly shift indices along one axis and materialize an owned row-major output.
pub fn roll(tensor: &Tensor, axis: isize, shift: isize) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let axis = normalize_axis(axis, shape.len())?;
    let out_shape = tensor.shape.clone();
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(roll_values(values, &shape, axis, shift)?),
        TensorData::F64(values) => TensorData::F64(roll_values(values, &shape, axis, shift)?),
        TensorData::I32(values) => TensorData::I32(roll_values(values, &shape, axis, shift)?),
        TensorData::I64(values) => TensorData::I64(roll_values(values, &shape, axis, shift)?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Add a scalar to every tensor element. The scalar dtype must match the tensor dtype.
pub fn add_scalar(tensor: &Tensor, rhs: impl Into<Scalar>) -> Result<Tensor> {
    scalar_op(tensor, rhs.into(), ScalarOp::Add)
}

/// Subtract a scalar from every tensor element. The scalar dtype must match the tensor dtype.
pub fn sub_scalar(tensor: &Tensor, rhs: impl Into<Scalar>) -> Result<Tensor> {
    scalar_op(tensor, rhs.into(), ScalarOp::Sub)
}

/// Multiply every tensor element by a scalar. The scalar dtype must match the tensor dtype.
pub fn mul_scalar(tensor: &Tensor, rhs: impl Into<Scalar>) -> Result<Tensor> {
    scalar_op(tensor, rhs.into(), ScalarOp::Mul)
}

/// Divide every tensor element by a scalar. Integer division is checked and rejects zero.
pub fn div_scalar(tensor: &Tensor, rhs: impl Into<Scalar>) -> Result<Tensor> {
    scalar_op(tensor, rhs.into(), ScalarOp::Div)
}

/// Add tensors with exact dtype matching and NumPy-style broadcasting.
pub fn add(lhs: &Tensor, rhs: &Tensor) -> Result<Tensor> {
    binary_op(lhs, rhs, BinaryOp::Add)
}

/// Subtract tensors with exact dtype matching and NumPy-style broadcasting.
pub fn sub(lhs: &Tensor, rhs: &Tensor) -> Result<Tensor> {
    binary_op(lhs, rhs, BinaryOp::Sub)
}

/// Multiply tensors with exact dtype matching and NumPy-style broadcasting.
pub fn mul(lhs: &Tensor, rhs: &Tensor) -> Result<Tensor> {
    binary_op(lhs, rhs, BinaryOp::Mul)
}

/// Divide tensors with exact dtype matching and NumPy-style broadcasting.
pub fn div(lhs: &Tensor, rhs: &Tensor) -> Result<Tensor> {
    binary_op(lhs, rhs, BinaryOp::Div)
}

/// Sum values across selected axes.
///
/// `axes = None` selects all axes. Because public [`Tensor`] values always have rank >= 1,
/// all-axis reductions must use `keepdims = true`; otherwise the operation would produce an
/// unsupported rank-0 scalar and returns an error.
pub fn sum(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(reduce_sum_values(
            values,
            &shape,
            &plan,
            0.0_f32,
            |a, b| Ok(a + b),
        )?),
        TensorData::F64(values) => TensorData::F64(reduce_sum_values(
            values,
            &shape,
            &plan,
            0.0_f64,
            |a, b| Ok(a + b),
        )?),
        TensorData::I32(values) => {
            TensorData::I32(reduce_sum_values(values, &shape, &plan, 0_i32, |a, b| {
                checked_i32(a.checked_add(b), "integer reduction overflow")
            })?)
        }
        TensorData::I64(values) => {
            TensorData::I64(reduce_sum_values(values, &shape, &plan, 0_i64, |a, b| {
                checked_i64(a.checked_add(b), "integer reduction overflow")
            })?)
        }
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Mean values across selected axes. Integer means return an `f64` tensor.
///
/// `axes = None` selects all axes. Because public [`Tensor`] values always have rank >= 1,
/// all-axis reductions must use `keepdims = true`.
pub fn mean(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    if plan.reduced_elems == 0 {
        return Err(TioError::invalid_argument("mean of an empty reduction"));
    }
    let divisor = plan.reduced_elems as f64;
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    match &tensor.data {
        TensorData::F32(values) => {
            let mut out = reduce_sum_values(values, &shape, &plan, 0.0_f32, |a, b| Ok(a + b))?;
            let divisor = plan.reduced_elems as f32;
            for value in &mut out {
                *value /= divisor;
            }
            Tensor::from_dense_f32(out_shape, out)
        }
        TensorData::F64(values) => {
            let mut out = reduce_sum_values(values, &shape, &plan, 0.0_f64, |a, b| Ok(a + b))?;
            for value in &mut out {
                *value /= divisor;
            }
            Tensor::from_dense_f64(out_shape, out)
        }
        TensorData::I32(values) => {
            let mut out = reduce_sum_mapped_values(values, &shape, &plan, 0.0_f64, |a, b| {
                Ok(a + f64::from(b))
            })?;
            for value in &mut out {
                *value /= divisor;
            }
            Tensor::from_dense_f64(out_shape, out)
        }
        TensorData::I64(values) => {
            let mut out =
                reduce_sum_mapped_values(values, &shape, &plan, 0.0_f64, |a, b| Ok(a + b as f64))?;
            for value in &mut out {
                *value /= divisor;
            }
            Tensor::from_dense_f64(out_shape, out)
        }
    }
}

/// Minimum values across selected axes.
///
/// `axes = None` selects all axes. Because public [`Tensor`] values always have rank >= 1,
/// all-axis reductions must use `keepdims = true`.
pub fn min(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    let data = match &tensor.data {
        TensorData::F32(values) => {
            TensorData::F32(reduce_extreme_values(values, &shape, &plan, false)?)
        }
        TensorData::F64(values) => {
            TensorData::F64(reduce_extreme_values(values, &shape, &plan, false)?)
        }
        TensorData::I32(values) => {
            TensorData::I32(reduce_extreme_values(values, &shape, &plan, false)?)
        }
        TensorData::I64(values) => {
            TensorData::I64(reduce_extreme_values(values, &shape, &plan, false)?)
        }
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Maximum values across selected axes.
///
/// `axes = None` selects all axes. Because public [`Tensor`] values always have rank >= 1,
/// all-axis reductions must use `keepdims = true`.
pub fn max(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    let data = match &tensor.data {
        TensorData::F32(values) => {
            TensorData::F32(reduce_extreme_values(values, &shape, &plan, true)?)
        }
        TensorData::F64(values) => {
            TensorData::F64(reduce_extreme_values(values, &shape, &plan, true)?)
        }
        TensorData::I32(values) => {
            TensorData::I32(reduce_extreme_values(values, &shape, &plan, true)?)
        }
        TensorData::I64(values) => {
            TensorData::I64(reduce_extreme_values(values, &shape, &plan, true)?)
        }
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

/// Zero-based argmin indices across selected axes.
///
/// Output values are `i64` row-major offsets within the reduced subspace. `axes = None`
/// selects all axes. Because public [`Tensor`] values always have rank >= 1, all-axis
/// reductions must use `keepdims = true`.
pub fn argmin(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    arg_reduce(tensor, axes, keepdims, false)
}

/// Zero-based argmax indices across selected axes.
///
/// Output values are `i64` row-major offsets within the reduced subspace. `axes = None`
/// selects all axes. Because public [`Tensor`] values always have rank >= 1, all-axis
/// reductions must use `keepdims = true`.
pub fn argmax(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    arg_reduce(tensor, axes, keepdims, true)
}

/// Cumulative sum along one axis, or over the flattened tensor when `axis = None`.
pub fn cumsum(tensor: &Tensor, axis: Option<isize>) -> Result<Tensor> {
    cumulative_op(tensor, axis, CumulativeOp::Sum)
}

/// Cumulative product along one axis, or over the flattened tensor when `axis = None`.
pub fn cumprod(tensor: &Tensor, axis: Option<isize>) -> Result<Tensor> {
    cumulative_op(tensor, axis, CumulativeOp::Product)
}

/// Population variance (`ddof = 0`) across selected axes.
///
/// Integer inputs promote to `f64`. `axes = None` selects all axes. Because public [`Tensor`]
/// values always have rank >= 1, all-axis reductions must use `keepdims = true`.
pub fn var(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    if plan.reduced_elems == 0 && plan.out_elems > 0 {
        return Err(TioError::invalid_argument("variance of an empty reduction"));
    }
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    match &tensor.data {
        TensorData::F32(values) => {
            Tensor::from_dense_f32(out_shape, reduce_variance_f32(values, &shape, &plan)?)
        }
        TensorData::F64(values) => {
            Tensor::from_dense_f64(out_shape, reduce_variance_f64(values, &shape, &plan)?)
        }
        TensorData::I32(values) => {
            Tensor::from_dense_f64(out_shape, reduce_variance_i32(values, &shape, &plan)?)
        }
        TensorData::I64(values) => {
            Tensor::from_dense_f64(out_shape, reduce_variance_i64(values, &shape, &plan)?)
        }
    }
}

/// Population standard deviation (`ddof = 0`) across selected axes.
///
/// Integer inputs promote to `f64`. `axes = None` selects all axes. Because public [`Tensor`]
/// values always have rank >= 1, all-axis reductions must use `keepdims = true`.
pub fn std(tensor: &Tensor, axes: Option<&[isize]>, keepdims: bool) -> Result<Tensor> {
    let variance = var(tensor, axes, keepdims)?;
    match variance.data {
        TensorData::F32(values) => Tensor::from_dense_f32(variance.shape, sqrt_f32_values(values)?),
        TensorData::F64(values) => Tensor::from_dense_f64(variance.shape, sqrt_f64_values(values)?),
        TensorData::I32(_) | TensorData::I64(_) => Err(TioError::invalid_argument(
            "variance output payload dtype mismatch",
        )),
    }
}

#[derive(Clone, Copy)]
enum ScalarOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy)]
enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
}

#[derive(Clone, Copy)]
enum CumulativeOp {
    Sum,
    Product,
}

struct ReductionPlan {
    reduce_mask: Vec<bool>,
    keepdims: bool,
    out_shape: Vec<usize>,
    out_strides: Vec<usize>,
    reduced_strides: Vec<usize>,
    out_elems: usize,
    reduced_elems: usize,
}

impl ReductionPlan {
    fn new(shape: &[usize], axes: Option<&[isize]>, keepdims: bool) -> Result<Self> {
        let reduced_axes = match axes {
            Some(values) => normalize_axes(values.iter().copied(), shape.len())?,
            None => (0..shape.len()).collect(),
        };
        let mut reduce_mask = vec![false; shape.len()];
        for axis in reduced_axes {
            reduce_mask[axis] = true;
        }
        if !keepdims && reduce_mask.iter().all(|&reduced| reduced) {
            return Err(TioError::invalid_argument(
                "reduction would produce a rank-0 tensor; set keepdims=true",
            ));
        }
        let mut reduced_strides = vec![0usize; shape.len()];
        let mut reduced_elems = 1usize;
        for axis in (0..shape.len()).rev() {
            if reduce_mask[axis] {
                reduced_strides[axis] = reduced_elems;
                reduced_elems = reduced_elems
                    .checked_mul(shape[axis])
                    .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
            }
        }
        let mut out_shape = Vec::new();
        for (axis, &dim) in shape.iter().enumerate() {
            if reduce_mask[axis] {
                if keepdims {
                    out_shape.push(1);
                }
            } else {
                out_shape.push(dim);
            }
        }
        if out_shape.is_empty() {
            return Err(TioError::invalid_argument(
                "reduction would produce a rank-0 tensor",
            ));
        }
        let out_strides = row_major_strides(&out_shape)?;
        let out_elems = shape_product_usize(&out_shape)?;
        Ok(Self {
            reduce_mask,
            keepdims,
            out_shape,
            out_strides,
            reduced_strides,
            out_elems,
            reduced_elems,
        })
    }

    fn out_index(&self, in_indices: &[usize]) -> Result<usize> {
        let mut out_linear = 0usize;
        if self.keepdims {
            for (axis, &in_index) in in_indices.iter().enumerate() {
                if self.reduce_mask[axis] {
                    continue;
                }
                let term = in_index
                    .checked_mul(self.out_strides[axis])
                    .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
                out_linear = out_linear
                    .checked_add(term)
                    .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
            }
            return Ok(out_linear);
        }
        let mut out_axis = 0usize;
        for (axis, &in_index) in in_indices.iter().enumerate() {
            if self.reduce_mask[axis] {
                continue;
            }
            let term = in_index
                .checked_mul(self.out_strides[out_axis])
                .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
            out_linear = out_linear
                .checked_add(term)
                .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
            out_axis += 1;
        }
        Ok(out_linear)
    }

    fn reduced_index(&self, in_indices: &[usize]) -> Result<usize> {
        let mut reduced_linear = 0usize;
        for (axis, &in_index) in in_indices.iter().enumerate() {
            if !self.reduce_mask[axis] {
                continue;
            }
            let term = in_index
                .checked_mul(self.reduced_strides[axis])
                .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
            reduced_linear = reduced_linear
                .checked_add(term)
                .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
        }
        Ok(reduced_linear)
    }
}

fn validate_shape_rank(shape: &[u64]) -> Result<()> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    Ok(())
}

fn validated_shape(tensor: &Tensor) -> Result<Vec<usize>> {
    tensor.validate()?;
    shape_u64_to_usize(&tensor.shape)
}

fn tensor_from_data(dtype: DType, shape: Vec<u64>, data: TensorData) -> Result<Tensor> {
    validate_tensor_parts(dtype, &shape, &data)?;
    Ok(Tensor { dtype, shape, data })
}

fn shape_u64_to_usize(shape: &[u64]) -> Result<Vec<usize>> {
    shape.iter().copied().map(dim_to_usize).collect()
}

fn shape_usize_to_u64(shape: &[usize]) -> Result<Vec<u64>> {
    shape.iter().copied().map(usize_to_u64).collect()
}

fn dim_to_usize(dim: u64) -> Result<usize> {
    usize::try_from(dim)
        .map_err(|_| TioError::invalid_argument("shape dimension does not fit usize"))
}

fn usize_to_u64(value: usize) -> Result<u64> {
    u64::try_from(value).map_err(|_| TioError::invalid_argument("value does not fit u64"))
}

fn shape_product_usize(shape: &[usize]) -> Result<usize> {
    shape.iter().try_fold(1usize, |product, &dim| {
        product
            .checked_mul(dim)
            .ok_or_else(|| TioError::invalid_argument("shape product overflow"))
    })
}

fn row_major_strides(shape: &[usize]) -> Result<Vec<usize>> {
    let mut strides = vec![1usize; shape.len()];
    for axis in (0..shape.len().saturating_sub(1)).rev() {
        strides[axis] = shape[axis + 1]
            .checked_mul(strides[axis + 1])
            .ok_or_else(|| TioError::invalid_argument("stride overflow"))?;
    }
    Ok(strides)
}

fn normalize_axis(axis: isize, rank: usize) -> Result<usize> {
    let rank = isize::try_from(rank).map_err(|_| TioError::invalid_argument("rank overflow"))?;
    let normalized = if axis < 0 {
        rank.checked_add(axis)
            .ok_or_else(|| TioError::invalid_argument("axis overflow"))?
    } else {
        axis
    };
    if normalized < 0 || normalized >= rank {
        return Err(TioError::invalid_argument("axis out of bounds"));
    }
    usize::try_from(normalized).map_err(|_| TioError::invalid_argument("axis overflow"))
}

fn normalize_insert_axis(axis: isize, rank: usize) -> Result<usize> {
    let rank = isize::try_from(rank).map_err(|_| TioError::invalid_argument("rank overflow"))?;
    let normalized = if axis < 0 {
        rank.checked_add(axis)
            .and_then(|value| value.checked_add(1))
            .ok_or_else(|| TioError::invalid_argument("axis overflow"))?
    } else {
        axis
    };
    if normalized < 0 || normalized > rank {
        return Err(TioError::invalid_argument("axis out of bounds"));
    }
    usize::try_from(normalized).map_err(|_| TioError::invalid_argument("axis overflow"))
}

fn normalize_axes<I>(axes: I, rank: usize) -> Result<Vec<usize>>
where
    I: IntoIterator<Item = isize>,
{
    let mut out = Vec::new();
    for axis in axes {
        let normalized = normalize_axis(axis, rank)?;
        if out.contains(&normalized) {
            return Err(TioError::invalid_argument("duplicate axis"));
        }
        out.push(normalized);
    }
    Ok(out)
}

fn broadcast_shape(lhs: &[usize], rhs: &[usize]) -> Result<Vec<usize>> {
    let rank = lhs.len().max(rhs.len());
    let mut out = Vec::with_capacity(rank);
    for offset in 0..rank {
        let lhs_dim = lhs
            .len()
            .checked_sub(offset + 1)
            .map(|index| lhs[index])
            .unwrap_or(1);
        let rhs_dim = rhs
            .len()
            .checked_sub(offset + 1)
            .map(|index| rhs[index])
            .unwrap_or(1);
        if lhs_dim == rhs_dim || lhs_dim == 1 {
            out.push(rhs_dim);
        } else if rhs_dim == 1 {
            out.push(lhs_dim);
        } else {
            return Err(TioError::invalid_argument(
                "shapes are not broadcast-compatible",
            ));
        }
    }
    out.reverse();
    Ok(out)
}

fn fallible_vec_with_capacity<T>(len: usize, context: &'static str) -> Result<Vec<T>> {
    let mut out = Vec::new();
    out.try_reserve(len)
        .map_err(|err| TioError::invalid_argument(format!("{context} allocation failed: {err}")))?;
    Ok(out)
}

fn fallible_filled_vec<T: Clone>(len: usize, value: T, context: &'static str) -> Result<Vec<T>> {
    let mut out = fallible_vec_with_capacity(len, context)?;
    out.resize(len, value);
    Ok(out)
}

fn linear_from_indices(indices: &[usize], strides: &[usize], shape: &[usize]) -> Result<usize> {
    if indices.len() != strides.len() || indices.len() != shape.len() {
        return Err(TioError::invalid_argument("indices rank mismatch"));
    }
    let mut linear = 0usize;
    for ((&index, &stride), &dim) in indices.iter().zip(strides).zip(shape) {
        if index >= dim {
            return Err(TioError::invalid_argument("index out of bounds"));
        }
        let term = index
            .checked_mul(stride)
            .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
        linear = linear
            .checked_add(term)
            .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
    }
    Ok(linear)
}

fn increment_indices(indices: &mut [usize], shape: &[usize]) {
    for axis in (0..indices.len()).rev() {
        indices[axis] += 1;
        if indices[axis] < shape[axis] {
            return;
        }
        indices[axis] = 0;
    }
}

fn permute_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    axes: &[usize],
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor permutation")?;
    let mut out_indices = vec![0usize; out_shape.len()];
    let mut in_indices = vec![0usize; input_shape.len()];
    for _ in 0..out_elems {
        for (out_axis, &in_axis) in axes.iter().enumerate() {
            in_indices[in_axis] = out_indices[out_axis];
        }
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn broadcast_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let offset = out_shape
        .len()
        .checked_sub(input_shape.len())
        .ok_or_else(|| TioError::invalid_argument("broadcast rank mismatch"))?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor broadcast")?;
    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = vec![0usize; input_shape.len()];
        for axis in 0..input_shape.len() {
            let out_index = out_indices[offset + axis];
            in_indices[axis] = if input_shape[axis] == 1 { 0 } else { out_index };
        }
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

struct DenseInput<'a, T> {
    shape: &'a [usize],
    values: &'a [T],
}

fn take_axis_normalized(
    tensor: &Tensor,
    shape: &[usize],
    axis: usize,
    indices: &[usize],
) -> Result<Tensor> {
    for &index in indices {
        if index >= shape[axis] {
            return Err(TioError::invalid_argument("index out of bounds"));
        }
    }
    let mut out_shape_usize = shape.to_vec();
    out_shape_usize[axis] = indices.len();
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(take_axis_values(
            values,
            shape,
            axis,
            indices,
            &out_shape_usize,
        )?),
        TensorData::F64(values) => TensorData::F64(take_axis_values(
            values,
            shape,
            axis,
            indices,
            &out_shape_usize,
        )?),
        TensorData::I32(values) => TensorData::I32(take_axis_values(
            values,
            shape,
            axis,
            indices,
            &out_shape_usize,
        )?),
        TensorData::I64(values) => TensorData::I64(take_axis_values(
            values,
            shape,
            axis,
            indices,
            &out_shape_usize,
        )?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

fn take_axis_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    axis: usize,
    indices: &[usize],
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor take")?;
    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = out_indices.clone();
        let take_pos = out_indices[axis];
        in_indices[axis] = indices[take_pos];
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn slice_axis_range_normalized(
    tensor: &Tensor,
    shape: &[usize],
    axis: usize,
    start: usize,
    end: usize,
) -> Result<Tensor> {
    if start > end || end > shape[axis] {
        return Err(TioError::invalid_argument("slice out of bounds"));
    }
    let mut out_shape_usize = shape.to_vec();
    out_shape_usize[axis] = end - start;
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    let data = match &tensor.data {
        TensorData::F32(values) => TensorData::F32(slice_axis_range_values(
            values,
            shape,
            axis,
            start,
            &out_shape_usize,
        )?),
        TensorData::F64(values) => TensorData::F64(slice_axis_range_values(
            values,
            shape,
            axis,
            start,
            &out_shape_usize,
        )?),
        TensorData::I32(values) => TensorData::I32(slice_axis_range_values(
            values,
            shape,
            axis,
            start,
            &out_shape_usize,
        )?),
        TensorData::I64(values) => TensorData::I64(slice_axis_range_values(
            values,
            shape,
            axis,
            start,
            &out_shape_usize,
        )?),
    };
    tensor_from_data(tensor.dtype, out_shape, data)
}

fn slice_axis_range_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    axis: usize,
    start: usize,
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor slice")?;
    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = out_indices.clone();
        in_indices[axis] = start
            .checked_add(out_indices[axis])
            .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn concat_values<T: Copy>(
    inputs: &[DenseInput<'_, T>],
    axis: usize,
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let mut prepared: Vec<(&[usize], Vec<usize>, &[T])> =
        fallible_vec_with_capacity(inputs.len(), "tensor concat metadata")?;
    let mut axis_prefix = fallible_vec_with_capacity(inputs.len() + 1, "tensor concat metadata")?;
    axis_prefix.push(0usize);

    for input in inputs {
        let strides = row_major_strides(input.shape)?;
        let next = axis_prefix
            .last()
            .copied()
            .unwrap_or(0)
            .checked_add(input.shape[axis])
            .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
        axis_prefix.push(next);
        prepared.push((input.shape, strides, input.values));
    }

    let out_elems = shape_product_usize(out_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor concat")?;
    if out_elems == 0 {
        return Ok(out);
    }

    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let axis_index = out_indices[axis];
        let input_idx = axis_prefix
            .windows(2)
            .position(|window| axis_index >= window[0] && axis_index < window[1])
            .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
        let (shape, strides, values) = &prepared[input_idx];
        let local_axis = axis_index - axis_prefix[input_idx];
        let mut in_indices = out_indices.clone();
        in_indices[axis] = local_axis;
        let in_linear = linear_from_indices(&in_indices, strides, shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn stack_values<T: Copy>(
    inputs: &[DenseInput<'_, T>],
    axis: usize,
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let input_shape = inputs
        .first()
        .map(|input| input.shape)
        .ok_or_else(|| TioError::invalid_argument("stack requires at least one tensor"))?;
    let in_strides = row_major_strides(input_shape)?;
    let out_elems = shape_product_usize(out_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor stack")?;
    if out_elems == 0 {
        return Ok(out);
    }

    let mut out_indices = vec![0usize; out_shape.len()];
    let mut in_indices = vec![0usize; input_shape.len()];
    for _ in 0..out_elems {
        let input_index = out_indices[axis];
        let input = inputs
            .get(input_index)
            .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
        let mut in_axis = 0usize;
        for (out_axis, &out_index) in out_indices.iter().enumerate() {
            if out_axis == axis {
                continue;
            }
            in_indices[in_axis] = out_index;
            in_axis += 1;
        }
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *input
                .values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn repeat_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    out_shape: &[usize],
    axis: usize,
    repeats: usize,
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor repeat")?;
    if out_elems == 0 {
        return Ok(out);
    }

    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = out_indices.clone();
        in_indices[axis] = out_indices[axis] / repeats;
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn tile_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    out_shape: &[usize],
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(out_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor tile")?;
    if out_elems == 0 {
        return Ok(out);
    }

    let mut out_indices = vec![0usize; out_shape.len()];
    let mut in_indices = vec![0usize; input_shape.len()];
    for _ in 0..out_elems {
        for axis in 0..input_shape.len() {
            in_indices[axis] = out_indices[axis] % input_shape[axis];
        }
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn flip_values<T: Copy>(values: &[T], input_shape: &[usize], axis: usize) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(input_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor flip")?;
    if out_elems == 0 {
        return Ok(out);
    }

    let mut out_indices = vec![0usize; input_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = out_indices.clone();
        in_indices[axis] = input_shape[axis]
            .checked_sub(1)
            .and_then(|value| value.checked_sub(out_indices[axis]))
            .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, input_shape);
    }
    Ok(out)
}

fn roll_values<T: Copy>(
    values: &[T],
    input_shape: &[usize],
    axis: usize,
    shift: isize,
) -> Result<Vec<T>> {
    let out_elems = shape_product_usize(input_shape)?;
    let in_strides = row_major_strides(input_shape)?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor roll")?;
    if out_elems == 0 || input_shape[axis] == 0 {
        return Ok(out);
    }
    let axis_len = isize::try_from(input_shape[axis])
        .map_err(|_| TioError::invalid_argument("axis length overflow"))?;
    let shift_norm = usize::try_from(shift.rem_euclid(axis_len))
        .map_err(|_| TioError::invalid_argument("shift overflow"))?;

    let mut out_indices = vec![0usize; input_shape.len()];
    for _ in 0..out_elems {
        let mut in_indices = out_indices.clone();
        in_indices[axis] = (out_indices[axis] + input_shape[axis] - shift_norm) % input_shape[axis];
        let in_linear = linear_from_indices(&in_indices, &in_strides, input_shape)?;
        out.push(
            *values
                .get(in_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?,
        );
        increment_indices(&mut out_indices, input_shape);
    }
    Ok(out)
}

fn strided_indices(len: usize, start: isize, end: isize, step: isize) -> Result<Vec<usize>> {
    if len == 0 {
        return Ok(Vec::new());
    }
    let len =
        isize::try_from(len).map_err(|_| TioError::invalid_argument("axis length overflow"))?;
    let mut out = Vec::new();
    if step > 0 {
        let mut current = if start < 0 {
            start
                .checked_add(len)
                .ok_or_else(|| TioError::invalid_argument("slice start overflow"))?
        } else {
            start
        };
        let end = if end < 0 {
            end.checked_add(len)
                .ok_or_else(|| TioError::invalid_argument("slice end overflow"))?
        } else {
            end
        };
        current = current.clamp(0, len);
        let end = end.clamp(0, len);
        while current < end {
            out.push(
                usize::try_from(current)
                    .map_err(|_| TioError::invalid_argument("slice index overflow"))?,
            );
            current = current
                .checked_add(step)
                .ok_or_else(|| TioError::invalid_argument("slice index overflow"))?;
        }
    } else {
        let mut current = if start < 0 {
            start
                .checked_add(len)
                .ok_or_else(|| TioError::invalid_argument("slice start overflow"))?
        } else {
            start
        };
        let end = if end < 0 {
            end.checked_add(len)
                .ok_or_else(|| TioError::invalid_argument("slice end overflow"))?
        } else {
            end
        };
        current = current.clamp(-1, len.saturating_sub(1));
        let end = end.clamp(-1, len.saturating_sub(1));
        while current > end {
            if current >= 0 {
                out.push(
                    usize::try_from(current)
                        .map_err(|_| TioError::invalid_argument("slice index overflow"))?,
                );
            }
            current = current
                .checked_add(step)
                .ok_or_else(|| TioError::invalid_argument("slice index overflow"))?;
        }
    }
    Ok(out)
}

fn scalar_op(tensor: &Tensor, rhs: Scalar, op: ScalarOp) -> Result<Tensor> {
    tensor.validate()?;
    match (&tensor.data, rhs) {
        (TensorData::F32(values), Scalar::F32(rhs)) => Tensor::from_dense_f32(
            tensor.shape.clone(),
            values
                .iter()
                .copied()
                .map(|value| scalar_f32(value, rhs, op))
                .collect(),
        ),
        (TensorData::F64(values), Scalar::F64(rhs)) => Tensor::from_dense_f64(
            tensor.shape.clone(),
            values
                .iter()
                .copied()
                .map(|value| scalar_f64(value, rhs, op))
                .collect(),
        ),
        (TensorData::I32(values), Scalar::I32(rhs)) => Tensor::from_dense_i32(
            tensor.shape.clone(),
            values
                .iter()
                .copied()
                .map(|value| scalar_i32(value, rhs, op))
                .collect::<Result<Vec<_>>>()?,
        ),
        (TensorData::I64(values), Scalar::I64(rhs)) => Tensor::from_dense_i64(
            tensor.shape.clone(),
            values
                .iter()
                .copied()
                .map(|value| scalar_i64(value, rhs, op))
                .collect::<Result<Vec<_>>>()?,
        ),
        _ => Err(TioError::invalid_argument("scalar dtype mismatch")),
    }
}

fn scalar_f32(lhs: f32, rhs: f32, op: ScalarOp) -> f32 {
    match op {
        ScalarOp::Add => lhs + rhs,
        ScalarOp::Sub => lhs - rhs,
        ScalarOp::Mul => lhs * rhs,
        ScalarOp::Div => lhs / rhs,
    }
}

fn scalar_f64(lhs: f64, rhs: f64, op: ScalarOp) -> f64 {
    match op {
        ScalarOp::Add => lhs + rhs,
        ScalarOp::Sub => lhs - rhs,
        ScalarOp::Mul => lhs * rhs,
        ScalarOp::Div => lhs / rhs,
    }
}

fn scalar_i32(lhs: i32, rhs: i32, op: ScalarOp) -> Result<i32> {
    match op {
        ScalarOp::Add => checked_i32(lhs.checked_add(rhs), "integer addition overflow"),
        ScalarOp::Sub => checked_i32(lhs.checked_sub(rhs), "integer subtraction overflow"),
        ScalarOp::Mul => checked_i32(lhs.checked_mul(rhs), "integer multiplication overflow"),
        ScalarOp::Div => checked_i32(lhs.checked_div(rhs), "integer division failed"),
    }
}

fn scalar_i64(lhs: i64, rhs: i64, op: ScalarOp) -> Result<i64> {
    match op {
        ScalarOp::Add => checked_i64(lhs.checked_add(rhs), "integer addition overflow"),
        ScalarOp::Sub => checked_i64(lhs.checked_sub(rhs), "integer subtraction overflow"),
        ScalarOp::Mul => checked_i64(lhs.checked_mul(rhs), "integer multiplication overflow"),
        ScalarOp::Div => checked_i64(lhs.checked_div(rhs), "integer division failed"),
    }
}

fn binary_op(lhs: &Tensor, rhs: &Tensor, op: BinaryOp) -> Result<Tensor> {
    let lhs_shape = validated_shape(lhs)?;
    let rhs_shape = validated_shape(rhs)?;
    if lhs.dtype != rhs.dtype {
        return Err(TioError::invalid_argument("tensor dtype mismatch"));
    }
    let out_shape_usize = broadcast_shape(&lhs_shape, &rhs_shape)?;
    let out_shape = shape_usize_to_u64(&out_shape_usize)?;
    match (&lhs.data, &rhs.data) {
        (TensorData::F32(lhs_values), TensorData::F32(rhs_values)) => Tensor::from_dense_f32(
            out_shape,
            binary_broadcast_values(
                lhs_values,
                &lhs_shape,
                rhs_values,
                &rhs_shape,
                &out_shape_usize,
                |a, b| Ok(binary_f32(a, b, op)),
            )?,
        ),
        (TensorData::F64(lhs_values), TensorData::F64(rhs_values)) => Tensor::from_dense_f64(
            out_shape,
            binary_broadcast_values(
                lhs_values,
                &lhs_shape,
                rhs_values,
                &rhs_shape,
                &out_shape_usize,
                |a, b| Ok(binary_f64(a, b, op)),
            )?,
        ),
        (TensorData::I32(lhs_values), TensorData::I32(rhs_values)) => Tensor::from_dense_i32(
            out_shape,
            binary_broadcast_values(
                lhs_values,
                &lhs_shape,
                rhs_values,
                &rhs_shape,
                &out_shape_usize,
                |a, b| binary_i32(a, b, op),
            )?,
        ),
        (TensorData::I64(lhs_values), TensorData::I64(rhs_values)) => Tensor::from_dense_i64(
            out_shape,
            binary_broadcast_values(
                lhs_values,
                &lhs_shape,
                rhs_values,
                &rhs_shape,
                &out_shape_usize,
                |a, b| binary_i64(a, b, op),
            )?,
        ),
        _ => Err(TioError::invalid_argument("tensor payload dtype mismatch")),
    }
}

fn binary_f32(lhs: f32, rhs: f32, op: BinaryOp) -> f32 {
    match op {
        BinaryOp::Add => lhs + rhs,
        BinaryOp::Sub => lhs - rhs,
        BinaryOp::Mul => lhs * rhs,
        BinaryOp::Div => lhs / rhs,
    }
}

fn binary_f64(lhs: f64, rhs: f64, op: BinaryOp) -> f64 {
    match op {
        BinaryOp::Add => lhs + rhs,
        BinaryOp::Sub => lhs - rhs,
        BinaryOp::Mul => lhs * rhs,
        BinaryOp::Div => lhs / rhs,
    }
}

fn binary_i32(lhs: i32, rhs: i32, op: BinaryOp) -> Result<i32> {
    match op {
        BinaryOp::Add => checked_i32(lhs.checked_add(rhs), "integer addition overflow"),
        BinaryOp::Sub => checked_i32(lhs.checked_sub(rhs), "integer subtraction overflow"),
        BinaryOp::Mul => checked_i32(lhs.checked_mul(rhs), "integer multiplication overflow"),
        BinaryOp::Div => checked_i32(lhs.checked_div(rhs), "integer division failed"),
    }
}

fn binary_i64(lhs: i64, rhs: i64, op: BinaryOp) -> Result<i64> {
    match op {
        BinaryOp::Add => checked_i64(lhs.checked_add(rhs), "integer addition overflow"),
        BinaryOp::Sub => checked_i64(lhs.checked_sub(rhs), "integer subtraction overflow"),
        BinaryOp::Mul => checked_i64(lhs.checked_mul(rhs), "integer multiplication overflow"),
        BinaryOp::Div => checked_i64(lhs.checked_div(rhs), "integer division failed"),
    }
}

fn binary_broadcast_values<T: Copy, F>(
    lhs: &[T],
    lhs_shape: &[usize],
    rhs: &[T],
    rhs_shape: &[usize],
    out_shape: &[usize],
    mut op: F,
) -> Result<Vec<T>>
where
    F: FnMut(T, T) -> Result<T>,
{
    let out_elems = shape_product_usize(out_shape)?;
    let lhs_strides = row_major_strides(lhs_shape)?;
    let rhs_strides = row_major_strides(rhs_shape)?;
    let lhs_offset = out_shape
        .len()
        .checked_sub(lhs_shape.len())
        .ok_or_else(|| TioError::invalid_argument("broadcast rank mismatch"))?;
    let rhs_offset = out_shape
        .len()
        .checked_sub(rhs_shape.len())
        .ok_or_else(|| TioError::invalid_argument("broadcast rank mismatch"))?;
    let mut out = fallible_vec_with_capacity(out_elems, "tensor binary operation")?;
    let mut out_indices = vec![0usize; out_shape.len()];
    for _ in 0..out_elems {
        let lhs_linear = broadcast_linear_index(&out_indices, lhs_shape, &lhs_strides, lhs_offset)?;
        let rhs_linear = broadcast_linear_index(&out_indices, rhs_shape, &rhs_strides, rhs_offset)?;
        let lhs_value = *lhs
            .get(lhs_linear)
            .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
        let rhs_value = *rhs
            .get(rhs_linear)
            .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
        out.push(op(lhs_value, rhs_value)?);
        increment_indices(&mut out_indices, out_shape);
    }
    Ok(out)
}

fn broadcast_linear_index(
    out_indices: &[usize],
    in_shape: &[usize],
    in_strides: &[usize],
    offset: usize,
) -> Result<usize> {
    let mut in_linear = 0usize;
    for axis in 0..in_shape.len() {
        let out_index = out_indices[offset + axis];
        let index = if in_shape[axis] == 1 { 0 } else { out_index };
        if index >= in_shape[axis] {
            return Err(TioError::invalid_argument("broadcast index out of bounds"));
        }
        let term = index
            .checked_mul(in_strides[axis])
            .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
        in_linear = in_linear
            .checked_add(term)
            .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
    }
    Ok(in_linear)
}

fn reduce_sum_values<T: Copy, F>(
    values: &[T],
    shape: &[usize],
    plan: &ReductionPlan,
    zero: T,
    mut add: F,
) -> Result<Vec<T>>
where
    F: FnMut(T, T) -> Result<T>,
{
    let mut out = fallible_filled_vec(plan.out_elems, zero, "tensor reduction")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        out[out_index] = add(out[out_index], value)?;
        increment_indices(&mut in_indices, shape);
    }
    Ok(out)
}

fn reduce_sum_mapped_values<I: Copy, O: Copy, F>(
    values: &[I],
    shape: &[usize],
    plan: &ReductionPlan,
    zero: O,
    mut add: F,
) -> Result<Vec<O>>
where
    F: FnMut(O, I) -> Result<O>,
{
    let mut out = fallible_filled_vec(plan.out_elems, zero, "tensor reduction")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        out[out_index] = add(out[out_index], value)?;
        increment_indices(&mut in_indices, shape);
    }
    Ok(out)
}

fn reduce_extreme_values<T: Copy + PartialOrd>(
    values: &[T],
    shape: &[usize],
    plan: &ReductionPlan,
    take_max: bool,
) -> Result<Vec<T>> {
    if plan.reduced_elems == 0 && plan.out_elems > 0 {
        return Err(TioError::invalid_argument("cannot reduce an empty axis"));
    }
    let mut out = fallible_filled_vec(plan.out_elems, None, "tensor reduction")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        match &mut out[out_index] {
            Some(current) => {
                if (take_max && value > *current) || (!take_max && value < *current) {
                    *current = value;
                }
            }
            slot @ None => *slot = Some(value),
        }
        increment_indices(&mut in_indices, shape);
    }
    out.into_iter()
        .map(|value| value.ok_or_else(|| TioError::invalid_argument("cannot reduce an empty axis")))
        .collect()
}

fn arg_reduce(
    tensor: &Tensor,
    axes: Option<&[isize]>,
    keepdims: bool,
    take_max: bool,
) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let plan = ReductionPlan::new(&shape, axes, keepdims)?;
    let out_shape = shape_usize_to_u64(&plan.out_shape)?;
    let out = match &tensor.data {
        TensorData::F32(values) => arg_reduce_values(values, &shape, &plan, take_max)?,
        TensorData::F64(values) => arg_reduce_values(values, &shape, &plan, take_max)?,
        TensorData::I32(values) => arg_reduce_values(values, &shape, &plan, take_max)?,
        TensorData::I64(values) => arg_reduce_values(values, &shape, &plan, take_max)?,
    };
    Tensor::from_dense_i64(out_shape, out)
}

fn arg_reduce_values<T: Copy + PartialOrd>(
    values: &[T],
    shape: &[usize],
    plan: &ReductionPlan,
    take_max: bool,
) -> Result<Vec<i64>> {
    if plan.reduced_elems == 0 && plan.out_elems > 0 {
        return Err(TioError::invalid_argument("cannot reduce an empty axis"));
    }
    let mut out = fallible_filled_vec(plan.out_elems, None, "tensor arg reduction")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        let reduced_index = i64::try_from(plan.reduced_index(&in_indices)?)
            .map_err(|_| TioError::invalid_argument("arg reduction index exceeds i64"))?;
        match &mut out[out_index] {
            Some((current, current_index)) => {
                if (take_max && value > *current) || (!take_max && value < *current) {
                    *current = value;
                    *current_index = reduced_index;
                }
            }
            slot @ None => *slot = Some((value, reduced_index)),
        }
        increment_indices(&mut in_indices, shape);
    }
    let mut indices = fallible_vec_with_capacity(out.len(), "tensor arg reduction")?;
    for value in out {
        indices.push(
            value
                .map(|(_, index)| index)
                .ok_or_else(|| TioError::invalid_argument("cannot reduce an empty axis"))?,
        );
    }
    Ok(indices)
}

fn cumulative_op(tensor: &Tensor, axis: Option<isize>, op: CumulativeOp) -> Result<Tensor> {
    let shape = validated_shape(tensor)?;
    let out_shape = match axis {
        Some(_) => tensor.shape.clone(),
        None => vec![usize_to_u64(shape_product_usize(&shape)?)?],
    };
    match &tensor.data {
        TensorData::F32(values) => Tensor::from_dense_f32(
            out_shape,
            cumulative_values(values, &shape, axis, |a, b| match op {
                CumulativeOp::Sum => Ok(a + b),
                CumulativeOp::Product => Ok(a * b),
            })?,
        ),
        TensorData::F64(values) => Tensor::from_dense_f64(
            out_shape,
            cumulative_values(values, &shape, axis, |a, b| match op {
                CumulativeOp::Sum => Ok(a + b),
                CumulativeOp::Product => Ok(a * b),
            })?,
        ),
        TensorData::I32(values) => Tensor::from_dense_i32(
            out_shape,
            cumulative_values(values, &shape, axis, |a, b| match op {
                CumulativeOp::Sum => {
                    checked_i32(a.checked_add(b), "integer cumulative sum overflow")
                }
                CumulativeOp::Product => {
                    checked_i32(a.checked_mul(b), "integer cumulative product overflow")
                }
            })?,
        ),
        TensorData::I64(values) => Tensor::from_dense_i64(
            out_shape,
            cumulative_values(values, &shape, axis, |a, b| match op {
                CumulativeOp::Sum => {
                    checked_i64(a.checked_add(b), "integer cumulative sum overflow")
                }
                CumulativeOp::Product => {
                    checked_i64(a.checked_mul(b), "integer cumulative product overflow")
                }
            })?,
        ),
    }
}

fn cumulative_values<T: Copy, F>(
    values: &[T],
    shape: &[usize],
    axis: Option<isize>,
    combine: F,
) -> Result<Vec<T>>
where
    F: FnMut(T, T) -> Result<T>,
{
    match axis {
        Some(axis) => {
            cumulative_axis_values(values, shape, normalize_axis(axis, shape.len())?, combine)
        }
        None => cumulative_flat_values(values, combine),
    }
}

fn cumulative_flat_values<T: Copy, F>(values: &[T], mut combine: F) -> Result<Vec<T>>
where
    F: FnMut(T, T) -> Result<T>,
{
    let mut out = fallible_vec_with_capacity(values.len(), "tensor cumulative reduction")?;
    let mut accumulator = None;
    for &value in values {
        let next = match accumulator {
            Some(current) => combine(current, value)?,
            None => value,
        };
        out.push(next);
        accumulator = Some(next);
    }
    Ok(out)
}

fn cumulative_axis_values<T: Copy, F>(
    values: &[T],
    shape: &[usize],
    axis: usize,
    mut combine: F,
) -> Result<Vec<T>>
where
    F: FnMut(T, T) -> Result<T>,
{
    let strides = row_major_strides(shape)?;
    let mut out = fallible_vec_with_capacity(values.len(), "tensor cumulative reduction")?;
    let mut in_indices = vec![0usize; shape.len()];
    for (linear, &value) in values.iter().enumerate() {
        let next = if in_indices[axis] == 0 {
            value
        } else {
            let previous_linear = linear
                .checked_sub(strides[axis])
                .ok_or_else(|| TioError::invalid_argument("index overflow"))?;
            let previous = *out
                .get(previous_linear)
                .ok_or_else(|| TioError::invalid_argument("index out of bounds"))?;
            combine(previous, value)?
        };
        out.push(next);
        increment_indices(&mut in_indices, shape);
    }
    Ok(out)
}

fn sqrt_f32_values(values: Vec<f32>) -> Result<Vec<f32>> {
    let mut out = fallible_vec_with_capacity(values.len(), "tensor standard deviation")?;
    for value in values {
        out.push(value.sqrt());
    }
    Ok(out)
}

fn sqrt_f64_values(values: Vec<f64>) -> Result<Vec<f64>> {
    let mut out = fallible_vec_with_capacity(values.len(), "tensor standard deviation")?;
    for value in values {
        out.push(value.sqrt());
    }
    Ok(out)
}

fn reduce_variance_f32(values: &[f32], shape: &[usize], plan: &ReductionPlan) -> Result<Vec<f32>> {
    let mut means = reduce_sum_values(values, shape, plan, 0.0_f32, |a, b| Ok(a + b))?;
    let divisor = plan.reduced_elems as f32;
    for mean in &mut means {
        *mean /= divisor;
    }
    let mut out = fallible_filled_vec(plan.out_elems, 0.0_f32, "tensor variance")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        let delta = value - means[out_index];
        out[out_index] += delta * delta;
        increment_indices(&mut in_indices, shape);
    }
    for value in &mut out {
        *value /= divisor;
    }
    Ok(out)
}

fn reduce_variance_f64(values: &[f64], shape: &[usize], plan: &ReductionPlan) -> Result<Vec<f64>> {
    let mut means = reduce_sum_values(values, shape, plan, 0.0_f64, |a, b| Ok(a + b))?;
    let divisor = plan.reduced_elems as f64;
    for mean in &mut means {
        *mean /= divisor;
    }
    let mut out = fallible_filled_vec(plan.out_elems, 0.0_f64, "tensor variance")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        let delta = value - means[out_index];
        out[out_index] += delta * delta;
        increment_indices(&mut in_indices, shape);
    }
    for value in &mut out {
        *value /= divisor;
    }
    Ok(out)
}

fn reduce_variance_i32(values: &[i32], shape: &[usize], plan: &ReductionPlan) -> Result<Vec<f64>> {
    reduce_variance_mapped(values, shape, plan, |value| f64::from(value))
}

fn reduce_variance_i64(values: &[i64], shape: &[usize], plan: &ReductionPlan) -> Result<Vec<f64>> {
    reduce_variance_mapped(values, shape, plan, |value| value as f64)
}

fn reduce_variance_mapped<T: Copy, F>(
    values: &[T],
    shape: &[usize],
    plan: &ReductionPlan,
    mut as_f64: F,
) -> Result<Vec<f64>>
where
    F: FnMut(T) -> f64,
{
    let mut means =
        reduce_sum_mapped_values(values, shape, plan, 0.0_f64, |a, b| Ok(a + as_f64(b)))?;
    let divisor = plan.reduced_elems as f64;
    for mean in &mut means {
        *mean /= divisor;
    }
    let mut out = fallible_filled_vec(plan.out_elems, 0.0_f64, "tensor variance")?;
    let mut in_indices = vec![0usize; shape.len()];
    for &value in values {
        let out_index = plan.out_index(&in_indices)?;
        let delta = as_f64(value) - means[out_index];
        out[out_index] += delta * delta;
        increment_indices(&mut in_indices, shape);
    }
    for value in &mut out {
        *value /= divisor;
    }
    Ok(out)
}

fn checked_i32(value: Option<i32>, message: &'static str) -> Result<i32> {
    value.ok_or_else(|| TioError::invalid_argument(message))
}

fn checked_i64(value: Option<i64>, message: &'static str) -> Result<i64> {
    value.ok_or_else(|| TioError::invalid_argument(message))
}
