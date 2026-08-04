#![doc = include_str!("../README.md")]
#![deny(dead_code)]
#![forbid(unsafe_op_in_unsafe_fn)]

use std::ffi::{CStr, CString};
use std::fmt;
use std::marker::PhantomData;
use std::mem::{self, MaybeUninit};
use std::ops::Range;
use std::os::raw::{c_char, c_void};
use std::path::Path;
use std::ptr::{self, NonNull};
use std::rc::Rc;
use std::slice;
use std::sync::OnceLock;

use arcadia_tio_sys as sys;

mod error;
pub use error::*;

mod ownership;
pub(crate) use ownership::*;

mod tensor;
pub use tensor::*;

/// Owned in-memory tensor operations over [`Tensor`] values.
/// The public wrapper's tensor-operation surface is intentionally source-visible and owned-copy:
/// helpers accept borrowed [`Tensor`] values, validate dtype/shape/payload consistency, and return
/// new owned [`Tensor`] values. The first-pass surface is the bounded dense-payload subset from
/// TP-430 Slice B:
///
/// - row-major shape helpers such as reshape, flatten/ravel aliases, expand/squeeze, axis
///   permutation, transpose, move-axis, and broadcast materialization;
/// - indexing and assembly helpers for half-open slices, stepped slices, explicit takes,
///   concat/stack/split/unstack, repeat/tile, flip, and roll;
/// - scalar and binary elementwise arithmetic with exact dtype matching and binary broadcasting;
/// - reductions for sum/mean/min/max over selected axes where the owned dense dtype can represent
///   the result.
///
/// Shape functions materialize output rather than promising zero-copy views; `to_contiguous` is a
/// validation-plus-clone boundary for this already-owned row-major tensor model. `*_view` aliases keep
/// parity with private/C++ naming while preserving the same owned-copy behavior. The supported
/// payload dtypes are the public dense [`TensorData`] variants (`f32`, `f64`, `i32`, and `i64`).
/// Dense read masks remain on [`DenseTensor`]; these helpers operate on the owned payload only and
/// do not propagate or inspect validity masks, null bitmaps, Arrow arrays, or borrowed native views.
pub mod ops;

/// Typed owned tensor operations over [`TypedTensor`] values.
///
/// This module forwards to the public untyped [`ops`] implementation and then validates that each
/// typed result has the expected dtype. The first slice intentionally covers dtype-preserving
/// operations plus `argmin`/`argmax` returning [`TensorI64`]; dtype-promoting `mean`, `var`, and
/// `std` remain available through [`ops`] on the untyped [`Tensor`] returned by
/// [`TypedTensor::as_tensor`].
pub mod typed_ops;

mod file_types;
pub use file_types::*;

mod coordinates;
pub use coordinates::*;

mod tensor_file;
pub use tensor_file::*;

mod conversion;
pub(crate) use conversion::*;

/// Safe Rust wrappers for the appendable OCB (Ordered Column Bundle) C ABI.
#[cfg(feature = "format-ocb")]
pub mod ocb;

#[cfg(test)]
mod tests;
