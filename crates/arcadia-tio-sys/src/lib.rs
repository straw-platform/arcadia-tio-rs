#![doc = include_str!("../README.md")]
#![deny(dead_code)]
#![forbid(unsafe_op_in_unsafe_fn)]
#![deny(missing_docs)]

use core::ffi::{c_char, c_double, c_float, c_int, c_void};

mod types;
pub use types::*;
mod common;
pub use common::*;
#[cfg(feature = "format-ocb")]
mod ocb;
#[cfg(feature = "format-ocb")]
pub use ocb::*;
mod lifecycle_coordinates;
pub use lifecycle_coordinates::*;

mod tensor;
pub use tensor::*;

mod mutation;
pub use mutation::*;

mod maintenance;
pub use maintenance::*;

mod metadata;
pub use metadata::*;

mod read;
pub use read::*;

mod history;
pub use history::*;
