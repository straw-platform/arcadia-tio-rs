#![doc = include_str!("../README.md")]
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

use arcadia_tio_sys as sys;

/// Result type returned by the safe wrapper.
pub type Result<T> = std::result::Result<T, TioError>;

/// Error code surfaced by the C ABI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// No error.
    Ok,
    /// Invalid argument.
    InvalidArgument,
    /// Operation is not implemented by the native library.
    Unimplemented,
    /// I/O failure.
    Io,
    /// FlatBuffers serialization/deserialization failure.
    Flatbuffers,
    /// Unknown native status code.
    Unknown(i32),
}

impl ErrorCode {
    fn from_raw(value: i32) -> Self {
        match value {
            sys::ARCADIA_TIO_ERROR_OK => Self::Ok,
            sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT => Self::InvalidArgument,
            sys::ARCADIA_TIO_ERROR_UNIMPLEMENTED => Self::Unimplemented,
            sys::ARCADIA_TIO_ERROR_IO => Self::Io,
            sys::ARCADIA_TIO_ERROR_FLATBUFFERS => Self::Flatbuffers,
            other => Self::Unknown(other),
        }
    }

    fn as_raw(self) -> i32 {
        match self {
            Self::Ok => sys::ARCADIA_TIO_ERROR_OK,
            Self::InvalidArgument => sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT,
            Self::Unimplemented => sys::ARCADIA_TIO_ERROR_UNIMPLEMENTED,
            Self::Io => sys::ARCADIA_TIO_ERROR_IO,
            Self::Flatbuffers => sys::ARCADIA_TIO_ERROR_FLATBUFFERS,
            Self::Unknown(value) => value,
        }
    }
}

/// Owned safe wrapper error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TioError {
    code: ErrorCode,
    message: String,
}

impl TioError {
    /// Returns the native/status error code.
    pub fn code(&self) -> ErrorCode {
        self.code
    }

    /// Returns the owned error message.
    pub fn message(&self) -> &str {
        &self.message
    }

    fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidArgument,
            message: message.into(),
        }
    }

    fn unimplemented(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Unimplemented,
            message: message.into(),
        }
    }

    fn from_last_error(fallback: &str) -> Self {
        // SAFETY: The C ABI exposes thread-local borrowed error storage. The wrapper copies the
        // string immediately into owned Rust memory before returning.
        let raw_code = unsafe { sys::arcadia_tio_last_error_code() };
        // SAFETY: The returned pointer is borrowed and may be null. It is only read for this call.
        let raw_message = unsafe { sys::arcadia_tio_last_error_message() };
        let message = if raw_message.is_null() {
            fallback.to_string()
        } else {
            // SAFETY: C ABI documents this as a NUL-terminated thread-local string.
            let copied = unsafe { CStr::from_ptr(raw_message) }
                .to_string_lossy()
                .into_owned();
            if copied.is_empty() {
                fallback.to_string()
            } else {
                copied
            }
        };
        Self {
            code: ErrorCode::from_raw(raw_code),
            message,
        }
    }

    fn conversion(message: impl Into<String>) -> Self {
        Self::invalid_argument(message)
    }

    fn with_reform_report(mut self, report: &ReformReport) -> Self {
        let mut details = Vec::new();
        if let Some(reason_code) = &report.reason_code {
            details.push(format!("reason_code={reason_code}"));
        }
        if let Some(taxonomy) = &report.reason_code_taxonomy {
            details.push(format!("reason_code_taxonomy={taxonomy}"));
        }
        if let Some(reason) = &report.reason {
            details.push(format!("reason={reason}"));
        }
        if !details.is_empty() {
            self.message = format!("{}; reform report: {}", self.message, details.join(", "));
        }
        self
    }
}

impl fmt::Display for TioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Arcadia TIO error {:?} ({}): {}",
            self.code,
            self.code.as_raw(),
            self.message
        )
    }
}

impl std::error::Error for TioError {}

fn status_result(status: i32, context: &str) -> Result<()> {
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(())
    } else {
        Err(TioError::from_last_error(context))
    }
}

fn path_to_cstring(path: impl AsRef<Path>) -> Result<CString> {
    let path = path.as_ref();
    let text = path
        .to_str()
        .ok_or_else(|| TioError::invalid_argument("path must be valid UTF-8 for the C ABI"))?;
    CString::new(text).map_err(|_| TioError::invalid_argument("path contains an interior NUL byte"))
}

fn string_to_cstring(value: &str, label: &str) -> Result<CString> {
    CString::new(value)
        .map_err(|_| TioError::invalid_argument(format!("{label} contains an interior NUL byte")))
}

fn optional_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        // SAFETY: Native metadata strings are documented as NUL-terminated C strings owned by the
        // metadata object while it is alive. The wrapper copies them immediately.
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

fn required_c_string(ptr: *const c_char) -> String {
    optional_c_string(ptr).unwrap_or_default()
}

/// Payload dtype supported by the first safe wrapper slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DType {
    /// 32-bit floating point.
    F32,
    /// 64-bit floating point.
    F64,
    /// 32-bit signed integer.
    I32,
    /// 64-bit signed integer.
    I64,
}

impl DType {
    fn to_raw(self) -> sys::ArcadiaTioDType {
        match self {
            Self::F32 => sys::ARCADIA_TIO_DTYPE_F32,
            Self::F64 => sys::ARCADIA_TIO_DTYPE_F64,
            Self::I32 => sys::ARCADIA_TIO_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_DTYPE_I64,
        }
    }

    fn from_raw(value: sys::ArcadiaTioDType) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_DTYPE_F32 => Ok(Self::F32),
            sys::ARCADIA_TIO_DTYPE_F64 => Ok(Self::F64),
            sys::ARCADIA_TIO_DTYPE_I32 => Ok(Self::I32),
            sys::ARCADIA_TIO_DTYPE_I64 => Ok(Self::I64),
            other => Err(TioError::conversion(format!("unknown dtype value {other}"))),
        }
    }

    /// Returns the number of bytes per scalar value for this dtype.
    pub fn size_bytes(self) -> usize {
        match self {
            Self::F32 | Self::I32 => 4,
            Self::F64 | Self::I64 => 8,
        }
    }
}

/// Semantic axis kind used in create metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    /// Time axis.
    Time,
    /// Symbol axis.
    Symbol,
    /// Channel axis.
    Channel,
    /// Other axis.
    Other,
}

impl AxisKind {
    fn to_raw(self) -> sys::ArcadiaTioAxisKind {
        match self {
            Self::Time => sys::ARCADIA_TIO_AXIS_TIME,
            Self::Symbol => sys::ARCADIA_TIO_AXIS_SYMBOL,
            Self::Channel => sys::ARCADIA_TIO_AXIS_CHANNEL,
            Self::Other => sys::ARCADIA_TIO_AXIS_OTHER,
        }
    }

    fn from_raw(value: sys::ArcadiaTioAxisKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_AXIS_TIME => Ok(Self::Time),
            sys::ARCADIA_TIO_AXIS_SYMBOL => Ok(Self::Symbol),
            sys::ARCADIA_TIO_AXIS_CHANNEL => Ok(Self::Channel),
            sys::ARCADIA_TIO_AXIS_OTHER => Ok(Self::Other),
            other => Err(TioError::conversion(format!(
                "unknown axis kind value {other}"
            ))),
        }
    }
}

/// Effective header/profile reported by metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderProfile {
    /// Streaming profile.
    Streaming,
    /// Random-access profile.
    RandomAccess,
}

impl HeaderProfile {
    fn from_raw(value: sys::ArcadiaTioHeaderProfile) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_HEADER_PROFILE_STREAMING => Ok(Self::Streaming),
            sys::ARCADIA_TIO_HEADER_PROFILE_RANDOM_ACCESS => Ok(Self::RandomAccess),
            other => Err(TioError::conversion(format!(
                "unknown header profile value {other}"
            ))),
        }
    }
}

/// Shape/dimension metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimSpec {
    /// Semantic axis kind.
    pub kind: AxisKind,
    /// Current axis length.
    pub len: u32,
    /// Optional axis name.
    pub name: Option<String>,
}

impl DimSpec {
    /// Creates a dimension descriptor without a name.
    pub fn new(kind: AxisKind, len: u32) -> Self {
        Self {
            kind,
            len,
            name: None,
        }
    }

    /// Sets an axis name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

/// Axis label metadata item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisLabel {
    /// Numeric label id assigned by the native metadata model.
    pub id: u32,
    /// Label name.
    pub name: String,
}

/// User metadata key/value item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserKv {
    /// Metadata key.
    pub key: String,
    /// Metadata value.
    pub value: String,
}

/// File metadata snapshot copied into Rust-owned values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// Payload dtype.
    pub dtype: DType,
    /// Dimension descriptors.
    pub dims: Vec<DimSpec>,
    /// Append dimension index.
    pub append_dim: usize,
    /// Symbol labels.
    pub symbols: Vec<AxisLabel>,
    /// Channel labels.
    pub channels: Vec<AxisLabel>,
    /// User metadata.
    pub user_kv: Vec<UserKv>,
    /// Effective header profile.
    pub effective_profile: HeaderProfile,
    /// Current head commit sequence.
    pub commit_seq: u64,
}

/// Owned tensor payload copied out of native C ABI buffers.
#[derive(Debug, Clone, PartialEq)]
pub enum TensorData {
    /// f32 payload data.
    F32(Vec<f32>),
    /// f64 payload data.
    F64(Vec<f64>),
    /// i32 payload data.
    I32(Vec<i32>),
    /// i64 payload data.
    I64(Vec<i64>),
}

impl TensorData {
    /// Returns the payload dtype.
    pub fn dtype(&self) -> DType {
        match self {
            Self::F32(_) => DType::F32,
            Self::F64(_) => DType::F64,
            Self::I32(_) => DType::I32,
            Self::I64(_) => DType::I64,
        }
    }

    /// Returns the number of scalar values.
    pub fn len(&self) -> usize {
        match self {
            Self::F32(values) => values.len(),
            Self::F64(values) => values.len(),
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    /// Returns true when there are no scalar values.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrow the payload as `f32` values.
    pub fn as_f32(&self) -> Result<&[f32]> {
        match self {
            Self::F32(values) => Ok(values),
            _ => Err(TioError::invalid_argument("tensor data is not f32")),
        }
    }

    /// Borrow the payload as `f64` values.
    pub fn as_f64(&self) -> Result<&[f64]> {
        match self {
            Self::F64(values) => Ok(values),
            _ => Err(TioError::invalid_argument("tensor data is not f64")),
        }
    }

    /// Borrow the payload as `i32` values.
    pub fn as_i32(&self) -> Result<&[i32]> {
        match self {
            Self::I32(values) => Ok(values),
            _ => Err(TioError::invalid_argument("tensor data is not i32")),
        }
    }

    /// Borrow the payload as `i64` values.
    pub fn as_i64(&self) -> Result<&[i64]> {
        match self {
            Self::I64(values) => Ok(values),
            _ => Err(TioError::invalid_argument("tensor data is not i64")),
        }
    }
}

/// Scalar value used by public in-memory tensor arithmetic helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Scalar {
    /// f32 scalar value.
    F32(f32),
    /// f64 scalar value.
    F64(f64),
    /// i32 scalar value.
    I32(i32),
    /// i64 scalar value.
    I64(i64),
}

impl Scalar {
    /// Returns the scalar dtype.
    pub fn dtype(&self) -> DType {
        match self {
            Self::F32(_) => DType::F32,
            Self::F64(_) => DType::F64,
            Self::I32(_) => DType::I32,
            Self::I64(_) => DType::I64,
        }
    }
}

impl From<f32> for Scalar {
    fn from(value: f32) -> Self {
        Self::F32(value)
    }
}

impl From<f64> for Scalar {
    fn from(value: f64) -> Self {
        Self::F64(value)
    }
}

impl From<i32> for Scalar {
    fn from(value: i32) -> Self {
        Self::I32(value)
    }
}

impl From<i64> for Scalar {
    fn from(value: i64) -> Self {
        Self::I64(value)
    }
}

/// Owned tensor copied into Rust memory.
#[derive(Debug, Clone, PartialEq)]
pub struct Tensor {
    /// Payload dtype.
    pub dtype: DType,
    /// Tensor shape.
    pub shape: Vec<u64>,
    /// Owned tensor payload.
    pub data: TensorData,
}

impl Tensor {
    /// Builds a dense f32 tensor and validates that `shape` matches `values.len()`.
    pub fn from_dense_f32(shape: Vec<u64>, values: Vec<f32>) -> Result<Self> {
        Self::from_dense_data(DType::F32, shape, TensorData::F32(values))
    }

    /// Builds a dense f64 tensor and validates that `shape` matches `values.len()`.
    pub fn from_dense_f64(shape: Vec<u64>, values: Vec<f64>) -> Result<Self> {
        Self::from_dense_data(DType::F64, shape, TensorData::F64(values))
    }

    /// Builds a dense i32 tensor and validates that `shape` matches `values.len()`.
    pub fn from_dense_i32(shape: Vec<u64>, values: Vec<i32>) -> Result<Self> {
        Self::from_dense_data(DType::I32, shape, TensorData::I32(values))
    }

    /// Builds a dense i64 tensor and validates that `shape` matches `values.len()`.
    pub fn from_dense_i64(shape: Vec<u64>, values: Vec<i64>) -> Result<Self> {
        Self::from_dense_data(DType::I64, shape, TensorData::I64(values))
    }

    /// Returns the number of scalar values implied by the shape.
    pub fn element_len(&self) -> Result<usize> {
        shape_element_len(&self.shape)
    }

    /// Validates that dtype, shape, and owned payload length agree.
    pub fn validate(&self) -> Result<()> {
        validate_tensor_parts(self.dtype, &self.shape, &self.data)
    }

    /// Borrow tensor values as `f32`, validating the tensor dtype and payload kind.
    pub fn values_f32(&self) -> Result<&[f32]> {
        self.validate_dtype(DType::F32)?;
        self.data.as_f32()
    }

    /// Borrow tensor values as `f64`, validating the tensor dtype and payload kind.
    pub fn values_f64(&self) -> Result<&[f64]> {
        self.validate_dtype(DType::F64)?;
        self.data.as_f64()
    }

    /// Borrow tensor values as `i32`, validating the tensor dtype and payload kind.
    pub fn values_i32(&self) -> Result<&[i32]> {
        self.validate_dtype(DType::I32)?;
        self.data.as_i32()
    }

    /// Borrow tensor values as `i64`, validating the tensor dtype and payload kind.
    pub fn values_i64(&self) -> Result<&[i64]> {
        self.validate_dtype(DType::I64)?;
        self.data.as_i64()
    }

    /// Convert this owned tensor into an Arrow [`RecordBatch`](arrow_array::RecordBatch).
    ///
    /// This opt-in `arrow` feature API is separate from [`TensorFile::read_values_arrow`]: it
    /// copies public [`TensorData`] values into Arrow crate-owned arrays instead of exposing native
    /// Arrow C Data release callbacks. The conversion preserves row-major shape metadata and is
    /// designed for dense f32/f64/i32/i64 payloads.
    #[cfg(feature = "arrow")]
    pub fn to_arrow_record_batch(&self) -> Result<arrow_array::RecordBatch> {
        tensor_to_arrow_record_batch(self)
    }

    /// Build an owned tensor from an Arrow [`RecordBatch`](arrow_array::RecordBatch).
    ///
    /// The accepted record-batch layout is the companion to [`Tensor::to_arrow_record_batch`]: a
    /// `time_id` column plus a dense `values` column with Arcadia TIO shape metadata.
    #[cfg(feature = "arrow")]
    pub fn from_arrow_record_batch(batch: &arrow_array::RecordBatch) -> Result<Self> {
        tensor_from_arrow_record_batch(batch)
    }

    /// Serialize this owned tensor to an Arrow IPC file payload using the `arrow` feature.
    #[cfg(feature = "arrow")]
    pub fn to_arrow_ipc(&self) -> Result<Vec<u8>> {
        let batch = self.to_arrow_record_batch()?;
        let mut out = Vec::new();
        {
            let mut writer =
                arrow_ipc::writer::FileWriter::try_new(&mut out, batch.schema().as_ref())
                    .map_err(arrow_err)?;
            writer.write(&batch).map_err(arrow_err)?;
            writer.finish().map_err(arrow_err)?;
        }
        Ok(out)
    }

    /// Decode an owned tensor from an Arrow IPC file payload using the `arrow` feature.
    #[cfg(feature = "arrow")]
    pub fn from_arrow_ipc(bytes: &[u8]) -> Result<Self> {
        let cursor = std::io::Cursor::new(bytes);
        let mut reader = arrow_ipc::reader::FileReaderBuilder::new()
            .build(cursor)
            .map_err(arrow_err)?;
        let mut batches = Vec::new();
        for batch in reader.by_ref() {
            batches.push(batch.map_err(arrow_err)?);
        }
        if batches.is_empty() {
            return Err(TioError::invalid_argument("no record batches found"));
        }
        if batches.len() > 1 {
            return Err(TioError::invalid_argument("expected a single record batch"));
        }
        Self::from_arrow_record_batch(&batches.remove(0))
    }

    /// Serialize this owned tensor to the companion CSV text format using the `csv` feature.
    ///
    /// The CSV payload is an owned-copy in-memory tensor interchange format with explicit dtype,
    /// shape, row-major order, and flat-index metadata. It is not a native `.tio` storage format or
    /// file-to-file conversion shortcut.
    #[cfg(feature = "csv")]
    pub fn to_csv_string(&self) -> Result<String> {
        tensor_to_csv_string(self)
    }

    /// Serialize this owned tensor to UTF-8 CSV bytes using the `csv` feature.
    #[cfg(feature = "csv")]
    pub fn to_csv_bytes(&self) -> Result<Vec<u8>> {
        Ok(self.to_csv_string()?.into_bytes())
    }

    /// Decode an owned tensor from the companion CSV text format using the `csv` feature.
    #[cfg(feature = "csv")]
    pub fn from_csv_str(text: &str) -> Result<Self> {
        tensor_from_csv_bytes(text.as_bytes())
    }

    /// Decode an owned tensor from UTF-8 CSV bytes using the `csv` feature.
    #[cfg(feature = "csv")]
    pub fn from_csv_bytes(bytes: &[u8]) -> Result<Self> {
        tensor_from_csv_bytes(bytes)
    }

    /// Serialize this owned tensor to companion Parquet bytes using the `parquet` feature.
    ///
    /// The Parquet payload is an owned-copy in-memory tensor interchange format with explicit
    /// Arcadia TIO key/value metadata for dtype, shape, and row-major order. It is not a native
    /// `.tio` storage format or file-to-file conversion shortcut.
    #[cfg(feature = "parquet")]
    pub fn to_parquet_bytes(&self) -> Result<Vec<u8>> {
        tensor_to_parquet_bytes(self)
    }

    /// Write this owned tensor to a companion Parquet file using the `parquet` feature.
    ///
    /// This writes the same owned-copy companion format as [`Tensor::to_parquet_bytes`]; it does
    /// not convert a native `.tio` file path or expose native storage internals.
    #[cfg(feature = "parquet")]
    pub fn to_parquet_file(&self, path: impl AsRef<Path>) -> Result<()> {
        std::fs::write(path, self.to_parquet_bytes()?).map_err(parquet_io_err)
    }

    /// Decode an owned tensor from companion Parquet bytes using the `parquet` feature.
    #[cfg(feature = "parquet")]
    pub fn from_parquet_bytes(bytes: &[u8]) -> Result<Self> {
        tensor_from_parquet_bytes(bytes)
    }

    /// Decode an owned tensor from a companion Parquet file using the `parquet` feature.
    #[cfg(feature = "parquet")]
    pub fn from_parquet_file(path: impl AsRef<Path>) -> Result<Self> {
        let bytes = std::fs::read(path).map_err(parquet_io_err)?;
        Self::from_parquet_bytes(&bytes)
    }

    /// Convert this owned tensor into an owned row-major [`ndarray::ArrayD<f32>`].
    ///
    /// This opt-in `ndarray` feature API validates that the tensor dtype is [`DType::F32`], that
    /// dtype/shape/payload metadata agree, and that every tensor dimension fits `usize` before it
    /// copies values into an ndarray-owned dynamic-dimensional array.
    #[cfg(feature = "ndarray")]
    pub fn to_ndarray_f32(&self) -> Result<ndarray::ArrayD<f32>> {
        self.validate()?;
        self.validate_dtype(DType::F32)?;
        tensor_to_ndarray(&self.shape, self.data.as_f32()?)
    }

    /// Convert this owned tensor into an owned row-major [`ndarray::ArrayD<f64>`].
    ///
    /// This opt-in `ndarray` feature API validates that the tensor dtype is [`DType::F64`], that
    /// dtype/shape/payload metadata agree, and that every tensor dimension fits `usize` before it
    /// copies values into an ndarray-owned dynamic-dimensional array.
    #[cfg(feature = "ndarray")]
    pub fn to_ndarray_f64(&self) -> Result<ndarray::ArrayD<f64>> {
        self.validate()?;
        self.validate_dtype(DType::F64)?;
        tensor_to_ndarray(&self.shape, self.data.as_f64()?)
    }

    /// Convert this owned tensor into an owned row-major [`ndarray::ArrayD<i32>`].
    ///
    /// This opt-in `ndarray` feature API validates that the tensor dtype is [`DType::I32`], that
    /// dtype/shape/payload metadata agree, and that every tensor dimension fits `usize` before it
    /// copies values into an ndarray-owned dynamic-dimensional array.
    #[cfg(feature = "ndarray")]
    pub fn to_ndarray_i32(&self) -> Result<ndarray::ArrayD<i32>> {
        self.validate()?;
        self.validate_dtype(DType::I32)?;
        tensor_to_ndarray(&self.shape, self.data.as_i32()?)
    }

    /// Convert this owned tensor into an owned row-major [`ndarray::ArrayD<i64>`].
    ///
    /// This opt-in `ndarray` feature API validates that the tensor dtype is [`DType::I64`], that
    /// dtype/shape/payload metadata agree, and that every tensor dimension fits `usize` before it
    /// copies values into an ndarray-owned dynamic-dimensional array.
    #[cfg(feature = "ndarray")]
    pub fn to_ndarray_i64(&self) -> Result<ndarray::ArrayD<i64>> {
        self.validate()?;
        self.validate_dtype(DType::I64)?;
        tensor_to_ndarray(&self.shape, self.data.as_i64()?)
    }

    /// Build an owned f32 tensor from an owned row-major [`ndarray::ArrayD<f32>`].
    ///
    /// The conversion records the ndarray shape as public tensor dimensions, rejects dimensions that
    /// do not fit `u64`, and validates that the resulting [`TensorData::F32`] length matches the
    /// shape product. Python NumPy integration remains outside this Rust feature boundary.
    #[cfg(feature = "ndarray")]
    pub fn from_ndarray_f32(array: ndarray::ArrayD<f32>) -> Result<Self> {
        let shape = ndarray_shape_to_tensor_shape(array.shape())?;
        Tensor::from_dense_f32(shape, array.iter().copied().collect())
    }

    /// Build an owned f64 tensor from an owned row-major [`ndarray::ArrayD<f64>`].
    ///
    /// The conversion records the ndarray shape as public tensor dimensions, rejects dimensions that
    /// do not fit `u64`, and validates that the resulting [`TensorData::F64`] length matches the
    /// shape product. Python NumPy integration remains outside this Rust feature boundary.
    #[cfg(feature = "ndarray")]
    pub fn from_ndarray_f64(array: ndarray::ArrayD<f64>) -> Result<Self> {
        let shape = ndarray_shape_to_tensor_shape(array.shape())?;
        Tensor::from_dense_f64(shape, array.iter().copied().collect())
    }

    /// Build an owned i32 tensor from an owned row-major [`ndarray::ArrayD<i32>`].
    ///
    /// The conversion records the ndarray shape as public tensor dimensions, rejects dimensions that
    /// do not fit `u64`, and validates that the resulting [`TensorData::I32`] length matches the
    /// shape product. Python NumPy integration remains outside this Rust feature boundary.
    #[cfg(feature = "ndarray")]
    pub fn from_ndarray_i32(array: ndarray::ArrayD<i32>) -> Result<Self> {
        let shape = ndarray_shape_to_tensor_shape(array.shape())?;
        Tensor::from_dense_i32(shape, array.iter().copied().collect())
    }

    /// Build an owned i64 tensor from an owned row-major [`ndarray::ArrayD<i64>`].
    ///
    /// The conversion records the ndarray shape as public tensor dimensions, rejects dimensions that
    /// do not fit `u64`, and validates that the resulting [`TensorData::I64`] length matches the
    /// shape product. Python NumPy integration remains outside this Rust feature boundary.
    #[cfg(feature = "ndarray")]
    pub fn from_ndarray_i64(array: ndarray::ArrayD<i64>) -> Result<Self> {
        let shape = ndarray_shape_to_tensor_shape(array.shape())?;
        Tensor::from_dense_i64(shape, array.iter().copied().collect())
    }

    fn from_dense_data(dtype: DType, shape: Vec<u64>, data: TensorData) -> Result<Self> {
        validate_tensor_parts(dtype, &shape, &data)?;
        Ok(Self { dtype, shape, data })
    }

    fn validate_dtype(&self, expected: DType) -> Result<()> {
        if self.dtype != expected {
            return Err(TioError::invalid_argument(format!(
                "tensor dtype {:?} does not match expected {:?}",
                self.dtype, expected
            )));
        }
        Ok(())
    }
}

/// Scalar element types supported by public owned typed tensor wrappers.
pub trait TensorElement: Copy + 'static {
    /// Tensor dtype associated with this Rust scalar type.
    const DTYPE: DType;

    /// Convert this Rust scalar into the public scalar enum used by [`ops`].
    fn into_scalar(self) -> Scalar;

    /// Build an untyped public tensor from dense row-major values of this scalar type.
    fn tensor_from_dense(shape: Vec<u64>, values: Vec<Self>) -> Result<Tensor>;

    /// Borrow a public tensor payload as this scalar type.
    fn values(data: &TensorData) -> Result<&[Self]>;
}

macro_rules! impl_tensor_element {
    ($ty:ty, $dtype:ident, $scalar:ident, $from_dense:ident, $values:ident) => {
        impl TensorElement for $ty {
            const DTYPE: DType = DType::$dtype;

            fn into_scalar(self) -> Scalar {
                Scalar::$scalar(self)
            }

            fn tensor_from_dense(shape: Vec<u64>, values: Vec<Self>) -> Result<Tensor> {
                Tensor::$from_dense(shape, values)
            }

            fn values(data: &TensorData) -> Result<&[Self]> {
                data.$values()
            }
        }
    };
}

impl_tensor_element!(f32, F32, F32, from_dense_f32, as_f32);
impl_tensor_element!(f64, F64, F64, from_dense_f64, as_f64);
impl_tensor_element!(i32, I32, I32, from_dense_i32, as_i32);
impl_tensor_element!(i64, I64, I64, from_dense_i64, as_i64);

/// Owned dtype-specific wrapper around the public [`Tensor`] model.
///
/// `TypedTensor<T>` keeps the same owned, row-major, dense-payload contract as [`Tensor`] while
/// validating that the wrapped dtype matches the Rust scalar type `T`. It is a convenience wrapper
/// for public Rust callers and does not borrow native buffers or depend on the private core crate's
/// typed tensor implementation.
#[derive(Debug, Clone, PartialEq)]
pub struct TypedTensor<T: TensorElement> {
    inner: Tensor,
    _marker: std::marker::PhantomData<T>,
}

impl<T: TensorElement> TypedTensor<T> {
    /// Build a typed tensor from dense row-major values, validating shape and dtype.
    pub fn from_dense(shape: Vec<u64>, values: Vec<T>) -> Result<Self> {
        Self::try_from_tensor(T::tensor_from_dense(shape, values)?)
    }

    /// Wrap an existing public tensor after validating its dtype, shape, and payload.
    pub fn try_from_tensor(inner: Tensor) -> Result<Self> {
        inner.validate()?;
        if inner.dtype != T::DTYPE {
            return Err(TioError::invalid_argument(format!(
                "tensor dtype {:?} does not match typed wrapper dtype {:?}",
                inner.dtype,
                T::DTYPE
            )));
        }
        T::values(&inner.data)?;
        Ok(Self {
            inner,
            _marker: std::marker::PhantomData,
        })
    }

    /// Return the dtype enforced by this typed wrapper.
    pub fn dtype(&self) -> DType {
        T::DTYPE
    }

    /// Borrow the tensor shape.
    pub fn shape(&self) -> &[u64] {
        &self.inner.shape
    }

    /// Borrow the typed dense values.
    pub fn values(&self) -> Result<&[T]> {
        self.inner.validate()?;
        T::values(&self.inner.data)
    }

    /// Return the element count implied by the tensor shape.
    pub fn element_len(&self) -> Result<usize> {
        self.inner.element_len()
    }

    /// Validate that dtype, shape, and payload length agree.
    pub fn validate(&self) -> Result<()> {
        self.inner.validate()
    }

    /// Borrow the untyped public tensor for APIs that still operate on [`Tensor`].
    pub fn as_tensor(&self) -> &Tensor {
        &self.inner
    }

    /// Borrow the untyped public tensor.
    pub fn inner(&self) -> &Tensor {
        self.as_tensor()
    }

    /// Consume the wrapper and return the underlying public tensor.
    pub fn into_tensor(self) -> Tensor {
        self.inner
    }
}

impl<T: TensorElement> TryFrom<Tensor> for TypedTensor<T> {
    type Error = TioError;

    fn try_from(value: Tensor) -> Result<Self> {
        Self::try_from_tensor(value)
    }
}

impl<T: TensorElement> From<TypedTensor<T>> for Tensor {
    fn from(value: TypedTensor<T>) -> Self {
        value.into_tensor()
    }
}

impl<T: TensorElement> AsRef<Tensor> for TypedTensor<T> {
    fn as_ref(&self) -> &Tensor {
        self.as_tensor()
    }
}

impl<T: TensorElement> std::borrow::Borrow<Tensor> for TypedTensor<T> {
    fn borrow(&self) -> &Tensor {
        self.as_tensor()
    }
}

/// Owned f32 tensor wrapper.
pub type TensorF32 = TypedTensor<f32>;
/// Owned f64 tensor wrapper.
pub type TensorF64 = TypedTensor<f64>;
/// Owned i32 tensor wrapper.
pub type TensorI32 = TypedTensor<i32>;
/// Owned i64 tensor wrapper.
pub type TensorI64 = TypedTensor<i64>;

#[cfg(feature = "arrow")]
const ARROW_META_DIM_LENS: &str = "arcadia_tio_dim_lens";
#[cfg(feature = "arrow")]
const ARROW_META_ORDER: &str = "arcadia_tio_order";

#[cfg(feature = "arrow")]
fn tensor_to_arrow_record_batch(tensor: &Tensor) -> Result<arrow_array::RecordBatch> {
    use std::collections::HashMap;
    use std::sync::Arc;

    use arrow_array::{
        Array as _, ArrayRef, FixedSizeListArray, Float32Array, Float64Array, Int32Array,
        Int64Array, UInt32Array,
    };
    use arrow_schema::{DataType, Field, Schema};

    tensor.validate()?;
    let entry_count = arrow_u64_to_usize(tensor.shape[0], "entry length")?;
    let row_width = arrow_row_width_for_shape(&tensor.shape)?;
    if row_width == 0 {
        return Err(TioError::invalid_argument(
            "tensor has zero-sized inner dimensions",
        ));
    }
    let expected_len = entry_count
        .checked_mul(row_width)
        .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
    if expected_len != tensor.data.len() {
        return Err(TioError::invalid_argument(
            "values length does not match shape",
        ));
    }
    let entry_count_u32 = u32::try_from(entry_count)
        .map_err(|_| TioError::invalid_argument("entry length exceeds u32"))?;
    let row_width_i32 = i32::try_from(row_width)
        .map_err(|_| TioError::invalid_argument("entry width exceeds i32"))?;

    let time_ids = UInt32Array::from_iter_values(0..entry_count_u32);
    let time_field = Field::new("time_id", DataType::UInt32, false);

    let values_array: ArrayRef = match &tensor.data {
        TensorData::F32(values) => Arc::new(Float32Array::from(values.clone())) as ArrayRef,
        TensorData::F64(values) => Arc::new(Float64Array::from(values.clone())) as ArrayRef,
        TensorData::I32(values) => Arc::new(Int32Array::from(values.clone())) as ArrayRef,
        TensorData::I64(values) => Arc::new(Int64Array::from(values.clone())) as ArrayRef,
    };
    let value_field = Arc::new(Field::new("item", values_array.data_type().clone(), false));
    let list_array = FixedSizeListArray::try_new(value_field, row_width_i32, values_array, None)
        .map_err(arrow_err)?;
    let values_field = Field::new("values", list_array.data_type().clone(), false);

    let mut metadata = HashMap::new();
    metadata.insert(
        ARROW_META_DIM_LENS.to_string(),
        arrow_encode_dim_lens(&tensor.shape)?,
    );
    metadata.insert(ARROW_META_ORDER.to_string(), "row-major".to_string());

    let schema = Schema::new_with_metadata(vec![time_field, values_field], metadata);
    arrow_array::RecordBatch::try_new(
        Arc::new(schema),
        vec![
            Arc::new(time_ids) as ArrayRef,
            Arc::new(list_array) as ArrayRef,
        ],
    )
    .map_err(arrow_err)
}

#[cfg(feature = "arrow")]
fn tensor_from_arrow_record_batch(batch: &arrow_array::RecordBatch) -> Result<Tensor> {
    use arrow_array::{
        Array as _, FixedSizeListArray, Float32Array, Float64Array, Int32Array, Int64Array,
        UInt32Array,
    };
    use arrow_schema::DataType;

    let schema = batch.schema();
    let metadata = schema.metadata();
    if let Some(order) = metadata.get(ARROW_META_ORDER) {
        if order != "row-major" {
            return Err(TioError::invalid_argument(
                "arcadia_tio_order metadata must be row-major",
            ));
        }
    }

    let time_idx = schema.index_of("time_id").map_err(arrow_err)?;
    let values_idx = schema.index_of("values").map_err(arrow_err)?;
    let time_array = batch.column(time_idx);
    let values_array = batch.column(values_idx);

    let time_array = time_array
        .as_any()
        .downcast_ref::<UInt32Array>()
        .ok_or_else(|| TioError::invalid_argument("time_id must be UInt32"))?;
    let list_array = values_array
        .as_any()
        .downcast_ref::<FixedSizeListArray>()
        .ok_or_else(|| TioError::invalid_argument("values must be FixedSizeList"))?;

    if time_array.null_count() != 0 {
        return Err(TioError::invalid_argument("time_id contains nulls"));
    }
    if list_array.null_count() != 0 {
        return Err(TioError::invalid_argument("values contains null lists"));
    }

    let entry_count = list_array.len();
    if time_array.len() != entry_count {
        return Err(TioError::invalid_argument(
            "time_id length does not match values length",
        ));
    }
    if entry_count > u32::MAX as usize {
        return Err(TioError::invalid_argument("entry length exceeds u32"));
    }
    for row in 0..entry_count {
        if time_array.value(row) != row as u32 {
            return Err(TioError::invalid_argument(
                "time_id values must be exactly 0..entry_count-1 in row order",
            ));
        }
    }

    let list_size = usize::try_from(list_array.value_length())
        .map_err(|_| TioError::invalid_argument("values FixedSizeList width is negative"))?;
    if list_size == 0 {
        return Err(TioError::invalid_argument(
            "values FixedSizeList width must be positive",
        ));
    }
    let shape = match metadata.get(ARROW_META_DIM_LENS) {
        Some(value) => arrow_parse_dim_lens(value, entry_count, list_size)?,
        None => arrow_infer_shape(entry_count, list_size)?,
    };

    let expected_len = entry_count
        .checked_mul(list_size)
        .ok_or_else(|| TioError::invalid_argument("shape product overflow"))?;
    let values = list_array.values();
    if values.len() != expected_len {
        return Err(TioError::invalid_argument(
            "values length does not match shape",
        ));
    }
    if values.null_count() != 0 {
        return Err(TioError::invalid_argument("values contains null scalars"));
    }

    match values.data_type() {
        DataType::Float32 => {
            let values = values
                .as_any()
                .downcast_ref::<Float32Array>()
                .ok_or_else(|| TioError::invalid_argument("values must be Float32"))?;
            Tensor::from_dense_f32(
                shape,
                (0..expected_len).map(|idx| values.value(idx)).collect(),
            )
        }
        DataType::Float64 => {
            let values = values
                .as_any()
                .downcast_ref::<Float64Array>()
                .ok_or_else(|| TioError::invalid_argument("values must be Float64"))?;
            Tensor::from_dense_f64(
                shape,
                (0..expected_len).map(|idx| values.value(idx)).collect(),
            )
        }
        DataType::Int32 => {
            let values = values
                .as_any()
                .downcast_ref::<Int32Array>()
                .ok_or_else(|| TioError::invalid_argument("values must be Int32"))?;
            Tensor::from_dense_i32(
                shape,
                (0..expected_len).map(|idx| values.value(idx)).collect(),
            )
        }
        DataType::Int64 => {
            let values = values
                .as_any()
                .downcast_ref::<Int64Array>()
                .ok_or_else(|| TioError::invalid_argument("values must be Int64"))?;
            Tensor::from_dense_i64(
                shape,
                (0..expected_len).map(|idx| values.value(idx)).collect(),
            )
        }
        other => Err(TioError::invalid_argument(format!(
            "unsupported Arrow values dtype {other:?}"
        ))),
    }
}

#[cfg(feature = "arrow")]
fn arrow_row_width_for_shape(shape: &[u64]) -> Result<usize> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    shape_element_len(&shape[1..])
}

#[cfg(feature = "arrow")]
fn arrow_encode_dim_lens(shape: &[u64]) -> Result<String> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    Ok(shape
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(","))
}

#[cfg(feature = "arrow")]
fn arrow_parse_dim_lens(value: &str, entry_count: usize, list_size: usize) -> Result<Vec<u64>> {
    let mut dims = Vec::new();
    for part in value.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(TioError::invalid_argument("invalid dim lens metadata"));
        }
        dims.push(
            part.parse::<u64>()
                .map_err(|_| TioError::invalid_argument("invalid dim lens metadata"))?,
        );
    }
    if dims.is_empty() {
        return Err(TioError::invalid_argument("dim lens metadata is empty"));
    }
    let entry_count_u64 = u64::try_from(entry_count)
        .map_err(|_| TioError::invalid_argument("entry length exceeds u64"))?;
    if dims[0] != entry_count_u64 {
        return Err(TioError::invalid_argument(
            "entry length does not match batch entry count",
        ));
    }
    let expected = shape_element_len(&dims[1..])?;
    if expected != list_size {
        return Err(TioError::invalid_argument(
            "list size does not match dim lens metadata",
        ));
    }
    Ok(dims)
}

#[cfg(feature = "arrow")]
fn arrow_infer_shape(entry_count: usize, list_size: usize) -> Result<Vec<u64>> {
    let entry_count = u64::try_from(entry_count)
        .map_err(|_| TioError::invalid_argument("entry length exceeds u64"))?;
    let list_size = u64::try_from(list_size)
        .map_err(|_| TioError::invalid_argument("entry width exceeds u64"))?;
    if list_size <= 1 {
        Ok(vec![entry_count])
    } else {
        Ok(vec![entry_count, list_size])
    }
}

#[cfg(feature = "arrow")]
fn arrow_u64_to_usize(value: u64, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| TioError::invalid_argument(format!("{label} exceeds usize")))
}

#[cfg(feature = "arrow")]
fn arrow_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError {
        code: ErrorCode::Io,
        message: err.to_string(),
    }
}

#[cfg(feature = "csv")]
const CSV_HEADER: [&str; 6] = ["record", "dtype", "shape", "order", "flat_index", "value"];
#[cfg(feature = "csv")]
const CSV_RECORD_METADATA: &str = "metadata";
#[cfg(feature = "csv")]
const CSV_RECORD_VALUE: &str = "value";
#[cfg(any(feature = "csv", feature = "parquet"))]
const TENSOR_ORDER_ROW_MAJOR: &str = "row-major";

#[cfg(feature = "csv")]
fn tensor_to_csv_string(tensor: &Tensor) -> Result<String> {
    tensor.validate()?;
    let mut writer = csv::WriterBuilder::new()
        .has_headers(false)
        .from_writer(Vec::new());
    writer.write_record(CSV_HEADER).map_err(csv_err)?;
    let dtype = tensor_dtype_name(tensor.dtype);
    let shape = tensor_shape_string(&tensor.shape)?;
    writer
        .write_record([
            CSV_RECORD_METADATA,
            dtype,
            shape.as_str(),
            TENSOR_ORDER_ROW_MAJOR,
            "",
            "",
        ])
        .map_err(csv_err)?;
    match &tensor.data {
        TensorData::F32(values) => csv_write_value_rows(&mut writer, values)?,
        TensorData::F64(values) => csv_write_value_rows(&mut writer, values)?,
        TensorData::I32(values) => csv_write_value_rows(&mut writer, values)?,
        TensorData::I64(values) => csv_write_value_rows(&mut writer, values)?,
    }
    writer.flush().map_err(csv_err)?;
    let bytes = writer
        .into_inner()
        .map_err(|err| csv_err(err.into_error()))?;
    String::from_utf8(bytes).map_err(csv_err)
}

#[cfg(feature = "csv")]
fn csv_write_value_rows<T: ToString>(
    writer: &mut csv::Writer<Vec<u8>>,
    values: &[T],
) -> Result<()> {
    for (flat_index, value) in values.iter().enumerate() {
        writer
            .write_record([
                String::from(CSV_RECORD_VALUE),
                String::new(),
                String::new(),
                String::new(),
                flat_index.to_string(),
                value.to_string(),
            ])
            .map_err(csv_err)?;
    }
    Ok(())
}

#[cfg(feature = "csv")]
fn tensor_from_csv_bytes(bytes: &[u8]) -> Result<Tensor> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .from_reader(bytes);
    let headers = reader.headers().map_err(csv_err)?.clone();
    if headers.len() != CSV_HEADER.len()
        || headers
            .iter()
            .zip(CSV_HEADER.iter())
            .any(|(actual, expected)| actual != *expected)
    {
        return Err(TioError::invalid_argument(
            "invalid Arcadia TIO tensor CSV header",
        ));
    }

    let mut records = reader.records();
    let metadata = records
        .next()
        .ok_or_else(|| TioError::invalid_argument("missing Arcadia TIO tensor CSV metadata"))?
        .map_err(csv_err)?;
    csv_expect_record_len(&metadata)?;
    if csv_field(&metadata, 0)? != CSV_RECORD_METADATA {
        return Err(TioError::invalid_argument(
            "first Arcadia TIO tensor CSV record must be metadata",
        ));
    }
    let dtype = tensor_dtype_from_name(csv_field(&metadata, 1)?)?;
    let shape = tensor_shape_from_string(csv_field(&metadata, 2)?)?;
    if csv_field(&metadata, 3)? != TENSOR_ORDER_ROW_MAJOR {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor CSV order must be row-major",
        ));
    }
    if !csv_field(&metadata, 4)?.is_empty() || !csv_field(&metadata, 5)?.is_empty() {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor CSV metadata row must not contain value fields",
        ));
    }

    let expected_len = shape_element_len(&shape)?;
    let mut f32_values = Vec::new();
    let mut f64_values = Vec::new();
    let mut i32_values = Vec::new();
    let mut i64_values = Vec::new();
    let mut count = 0usize;
    for record in records {
        let record = record.map_err(csv_err)?;
        csv_expect_record_len(&record)?;
        if csv_field(&record, 0)? != CSV_RECORD_VALUE {
            return Err(TioError::invalid_argument(
                "Arcadia TIO tensor CSV data records must be value records",
            ));
        }
        if !csv_field(&record, 1)?.is_empty()
            || !csv_field(&record, 2)?.is_empty()
            || !csv_field(&record, 3)?.is_empty()
        {
            return Err(TioError::invalid_argument(
                "Arcadia TIO tensor CSV value rows must leave metadata fields empty",
            ));
        }
        let flat_index = csv_field(&record, 4)?
            .parse::<usize>()
            .map_err(|_| TioError::invalid_argument("invalid Arcadia TIO tensor CSV flat index"))?;
        if flat_index != count {
            return Err(TioError::invalid_argument(
                "Arcadia TIO tensor CSV flat indices must be contiguous and ordered",
            ));
        }
        let value = csv_field(&record, 5)?;
        match dtype {
            DType::F32 => f32_values.push(csv_parse_scalar::<f32>(value)?),
            DType::F64 => f64_values.push(csv_parse_scalar::<f64>(value)?),
            DType::I32 => i32_values.push(csv_parse_scalar::<i32>(value)?),
            DType::I64 => i64_values.push(csv_parse_scalar::<i64>(value)?),
        }
        count = count
            .checked_add(1)
            .ok_or_else(|| TioError::invalid_argument("CSV value count overflow"))?;
    }
    if count != expected_len {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor CSV value count does not match shape",
        ));
    }

    match dtype {
        DType::F32 => Tensor::from_dense_f32(shape, f32_values),
        DType::F64 => Tensor::from_dense_f64(shape, f64_values),
        DType::I32 => Tensor::from_dense_i32(shape, i32_values),
        DType::I64 => Tensor::from_dense_i64(shape, i64_values),
    }
}

#[cfg(feature = "csv")]
fn csv_expect_record_len(record: &csv::StringRecord) -> Result<()> {
    if record.len() != CSV_HEADER.len() {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor CSV records must have six fields",
        ));
    }
    Ok(())
}

#[cfg(feature = "csv")]
fn csv_field(record: &csv::StringRecord, index: usize) -> Result<&str> {
    record
        .get(index)
        .ok_or_else(|| TioError::invalid_argument("missing Arcadia TIO tensor CSV field"))
}

#[cfg(feature = "csv")]
fn csv_parse_scalar<T>(value: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse::<T>()
        .map_err(|_| TioError::invalid_argument("invalid Arcadia TIO tensor CSV scalar value"))
}

#[cfg(any(feature = "csv", feature = "parquet"))]
fn tensor_dtype_name(dtype: DType) -> &'static str {
    match dtype {
        DType::F32 => "f32",
        DType::F64 => "f64",
        DType::I32 => "i32",
        DType::I64 => "i64",
    }
}

#[cfg(any(feature = "csv", feature = "parquet"))]
fn tensor_dtype_from_name(value: &str) -> Result<DType> {
    match value {
        "f32" => Ok(DType::F32),
        "f64" => Ok(DType::F64),
        "i32" => Ok(DType::I32),
        "i64" => Ok(DType::I64),
        _ => Err(TioError::invalid_argument(
            "unsupported Arcadia TIO tensor dtype",
        )),
    }
}

#[cfg(any(feature = "csv", feature = "parquet"))]
fn tensor_shape_string(shape: &[u64]) -> Result<String> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    Ok(shape
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join("x"))
}

#[cfg(any(feature = "csv", feature = "parquet"))]
fn tensor_shape_from_string(value: &str) -> Result<Vec<u64>> {
    if value.is_empty() {
        return Err(TioError::invalid_argument("tensor shape metadata is empty"));
    }
    value
        .split('x')
        .map(|part| {
            if part.is_empty() {
                return Err(TioError::invalid_argument("invalid tensor shape metadata"));
            }
            part.parse::<u64>()
                .map_err(|_| TioError::invalid_argument("invalid tensor shape metadata"))
        })
        .collect()
}

#[cfg(feature = "csv")]
fn csv_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(err.to_string())
}

#[cfg(feature = "parquet")]
const PARQUET_TENSOR_FORMAT_KEY: &str = "arcadia_tio_format";
#[cfg(feature = "parquet")]
const PARQUET_TENSOR_FORMAT_VALUE: &str = "arcadia_tio_tensor_parquet_v1";
#[cfg(feature = "parquet")]
const PARQUET_TENSOR_DTYPE_KEY: &str = "arcadia_tio_dtype";
#[cfg(feature = "parquet")]
const PARQUET_TENSOR_SHAPE_KEY: &str = "arcadia_tio_shape";
#[cfg(feature = "parquet")]
const PARQUET_TENSOR_ORDER_KEY: &str = "arcadia_tio_order";
#[cfg(feature = "parquet")]
const PARQUET_COLUMN_FLAT_INDEX: &str = "flat_index";
#[cfg(feature = "parquet")]
const PARQUET_COLUMN_VALUE: &str = "value";

#[cfg(feature = "parquet")]
fn tensor_to_parquet_bytes(tensor: &Tensor) -> Result<Vec<u8>> {
    use parquet::file::writer::SerializedFileWriter;

    tensor.validate()?;
    let value_len = tensor.element_len()?;
    let flat_indices = parquet_flat_indices(value_len)?;
    let schema = parquet_tensor_schema(tensor.dtype)?;
    let properties = parquet_writer_properties(tensor)?;
    let mut out = Vec::new();
    {
        let mut writer =
            SerializedFileWriter::new(&mut out, schema, properties).map_err(parquet_err)?;
        let mut row_group = writer.next_row_group().map_err(parquet_err)?;
        let flat_column = row_group
            .next_column()
            .map_err(parquet_err)?
            .ok_or_else(|| {
                TioError::invalid_argument("missing Parquet flat_index column writer")
            })?;
        parquet_write_i64_column(flat_column, &flat_indices)?;
        let value_column = row_group
            .next_column()
            .map_err(parquet_err)?
            .ok_or_else(|| TioError::invalid_argument("missing Parquet value column writer"))?;
        match &tensor.data {
            TensorData::F32(values) => parquet_write_f32_column(value_column, values)?,
            TensorData::F64(values) => parquet_write_f64_column(value_column, values)?,
            TensorData::I32(values) => parquet_write_i32_column(value_column, values)?,
            TensorData::I64(values) => parquet_write_i64_column(value_column, values)?,
        }
        if row_group.next_column().map_err(parquet_err)?.is_some() {
            return Err(TioError::invalid_argument(
                "unexpected extra Parquet tensor column writer",
            ));
        }
        row_group.close().map_err(parquet_err)?;
        writer.close().map_err(parquet_err)?;
    }
    Ok(out)
}

#[cfg(feature = "parquet")]
fn tensor_from_parquet_bytes(input: &[u8]) -> Result<Tensor> {
    use parquet::file::reader::{FileReader, SerializedFileReader};

    let reader =
        SerializedFileReader::new(bytes::Bytes::copy_from_slice(input)).map_err(parquet_err)?;
    let file_metadata = reader.metadata().file_metadata();
    let (dtype, shape) = parquet_tensor_metadata(file_metadata)?;
    parquet_validate_schema(file_metadata.schema_descr(), dtype)?;
    let expected_len = shape_element_len(&shape)?;
    let num_rows = parquet_i64_to_usize(
        file_metadata.num_rows(),
        "Arcadia TIO tensor Parquet file row count",
    )?;
    if num_rows != expected_len {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet row count does not match shape",
        ));
    }

    match dtype {
        DType::F32 => Tensor::from_dense_f32(
            shape,
            parquet_read_tensor_column_values(&reader, expected_len, |row_group, row_count| {
                parquet_read_f32_column(row_group, 1, row_count)
            })?,
        ),
        DType::F64 => Tensor::from_dense_f64(
            shape,
            parquet_read_tensor_column_values(&reader, expected_len, |row_group, row_count| {
                parquet_read_f64_column(row_group, 1, row_count)
            })?,
        ),
        DType::I32 => Tensor::from_dense_i32(
            shape,
            parquet_read_tensor_column_values(&reader, expected_len, |row_group, row_count| {
                parquet_read_i32_column(row_group, 1, row_count)
            })?,
        ),
        DType::I64 => Tensor::from_dense_i64(
            shape,
            parquet_read_tensor_column_values(&reader, expected_len, |row_group, row_count| {
                parquet_read_i64_column(row_group, 1, row_count)
            })?,
        ),
    }
}

#[cfg(feature = "parquet")]
fn parquet_tensor_schema(dtype: DType) -> Result<std::sync::Arc<parquet::schema::types::Type>> {
    let value_type = parquet_value_type_name(dtype);
    let message_type = format!(
        "message arcadia_tio_tensor {{ REQUIRED INT64 {PARQUET_COLUMN_FLAT_INDEX}; REQUIRED {value_type} {PARQUET_COLUMN_VALUE}; }}"
    );
    parquet::schema::parser::parse_message_type(&message_type)
        .map(std::sync::Arc::new)
        .map_err(parquet_err)
}

#[cfg(feature = "parquet")]
fn parquet_writer_properties(
    tensor: &Tensor,
) -> Result<std::sync::Arc<parquet::file::properties::WriterProperties>> {
    let dtype = tensor_dtype_name(tensor.dtype);
    let shape = tensor_shape_string(&tensor.shape)?;
    let metadata = vec![
        parquet::file::metadata::KeyValue::new(
            PARQUET_TENSOR_FORMAT_KEY.to_string(),
            PARQUET_TENSOR_FORMAT_VALUE.to_string(),
        ),
        parquet::file::metadata::KeyValue::new(
            PARQUET_TENSOR_DTYPE_KEY.to_string(),
            dtype.to_string(),
        ),
        parquet::file::metadata::KeyValue::new(PARQUET_TENSOR_SHAPE_KEY.to_string(), shape),
        parquet::file::metadata::KeyValue::new(
            PARQUET_TENSOR_ORDER_KEY.to_string(),
            TENSOR_ORDER_ROW_MAJOR.to_string(),
        ),
    ];
    Ok(std::sync::Arc::new(
        parquet::file::properties::WriterProperties::builder()
            .set_key_value_metadata(Some(metadata))
            .build(),
    ))
}

#[cfg(feature = "parquet")]
fn parquet_value_type_name(dtype: DType) -> &'static str {
    match dtype {
        DType::F32 => "FLOAT",
        DType::F64 => "DOUBLE",
        DType::I32 => "INT32",
        DType::I64 => "INT64",
    }
}

#[cfg(feature = "parquet")]
fn parquet_physical_type(dtype: DType) -> parquet::basic::Type {
    match dtype {
        DType::F32 => parquet::basic::Type::FLOAT,
        DType::F64 => parquet::basic::Type::DOUBLE,
        DType::I32 => parquet::basic::Type::INT32,
        DType::I64 => parquet::basic::Type::INT64,
    }
}

#[cfg(feature = "parquet")]
fn parquet_flat_indices(len: usize) -> Result<Vec<i64>> {
    (0..len)
        .map(|idx| {
            i64::try_from(idx).map_err(|_| {
                TioError::invalid_argument("Arcadia TIO tensor Parquet flat index exceeds i64")
            })
        })
        .collect()
}

#[cfg(feature = "parquet")]
fn parquet_write_f32_column(
    mut column: parquet::file::writer::SerializedColumnWriter<'_>,
    values: &[f32],
) -> Result<()> {
    let written = column
        .typed::<parquet::data_type::FloatType>()
        .write_batch(values, None, None)
        .map_err(parquet_err)?;
    parquet_validate_written_column(PARQUET_COLUMN_VALUE, values.len(), written)?;
    column.close().map_err(parquet_err)
}

#[cfg(feature = "parquet")]
fn parquet_write_f64_column(
    mut column: parquet::file::writer::SerializedColumnWriter<'_>,
    values: &[f64],
) -> Result<()> {
    let written = column
        .typed::<parquet::data_type::DoubleType>()
        .write_batch(values, None, None)
        .map_err(parquet_err)?;
    parquet_validate_written_column(PARQUET_COLUMN_VALUE, values.len(), written)?;
    column.close().map_err(parquet_err)
}

#[cfg(feature = "parquet")]
fn parquet_write_i32_column(
    mut column: parquet::file::writer::SerializedColumnWriter<'_>,
    values: &[i32],
) -> Result<()> {
    let written = column
        .typed::<parquet::data_type::Int32Type>()
        .write_batch(values, None, None)
        .map_err(parquet_err)?;
    parquet_validate_written_column(PARQUET_COLUMN_VALUE, values.len(), written)?;
    column.close().map_err(parquet_err)
}

#[cfg(feature = "parquet")]
fn parquet_write_i64_column(
    mut column: parquet::file::writer::SerializedColumnWriter<'_>,
    values: &[i64],
) -> Result<()> {
    let written = column
        .typed::<parquet::data_type::Int64Type>()
        .write_batch(values, None, None)
        .map_err(parquet_err)?;
    parquet_validate_written_column(PARQUET_COLUMN_VALUE, values.len(), written)?;
    column.close().map_err(parquet_err)
}

#[cfg(feature = "parquet")]
fn parquet_validate_written_column(column: &str, expected: usize, written: usize) -> Result<()> {
    if written != expected {
        return Err(TioError::invalid_argument(format!(
            "Arcadia TIO tensor Parquet column {column} wrote {written} values, expected {expected}"
        )));
    }
    Ok(())
}

#[cfg(feature = "parquet")]
fn parquet_tensor_metadata(
    metadata: &parquet::file::metadata::FileMetaData,
) -> Result<(DType, Vec<u64>)> {
    if parquet_metadata_value(metadata, PARQUET_TENSOR_FORMAT_KEY)? != PARQUET_TENSOR_FORMAT_VALUE {
        return Err(TioError::invalid_argument(
            "unsupported Arcadia TIO tensor Parquet format marker",
        ));
    }
    let dtype =
        tensor_dtype_from_name(parquet_metadata_value(metadata, PARQUET_TENSOR_DTYPE_KEY)?)?;
    let shape =
        tensor_shape_from_string(parquet_metadata_value(metadata, PARQUET_TENSOR_SHAPE_KEY)?)?;
    if parquet_metadata_value(metadata, PARQUET_TENSOR_ORDER_KEY)? != TENSOR_ORDER_ROW_MAJOR {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet order must be row-major",
        ));
    }
    Ok((dtype, shape))
}

#[cfg(feature = "parquet")]
fn parquet_metadata_value<'a>(
    metadata: &'a parquet::file::metadata::FileMetaData,
    key: &str,
) -> Result<&'a str> {
    let values = metadata
        .key_value_metadata()
        .ok_or_else(|| TioError::invalid_argument("missing Arcadia TIO tensor Parquet metadata"))?;
    let entry = values
        .iter()
        .find(|entry| entry.key == key)
        .ok_or_else(|| {
            TioError::invalid_argument(format!(
                "missing Arcadia TIO tensor Parquet metadata key {key}"
            ))
        })?;
    entry.value.as_deref().ok_or_else(|| {
        TioError::invalid_argument(format!(
            "missing Arcadia TIO tensor Parquet metadata value {key}"
        ))
    })
}

#[cfg(feature = "parquet")]
fn parquet_validate_schema(
    schema: &parquet::schema::types::SchemaDescriptor,
    dtype: DType,
) -> Result<()> {
    if schema.num_columns() != 2 {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet schema must have two columns",
        ));
    }
    let flat_index = schema.column(0);
    if flat_index.name() != PARQUET_COLUMN_FLAT_INDEX
        || flat_index.physical_type() != parquet::basic::Type::INT64
        || flat_index.max_def_level() != 0
        || flat_index.max_rep_level() != 0
    {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet flat_index column must be required INT64",
        ));
    }
    let value = schema.column(1);
    if value.name() != PARQUET_COLUMN_VALUE
        || value.physical_type() != parquet_physical_type(dtype)
        || value.max_def_level() != 0
        || value.max_rep_level() != 0
    {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet value column type does not match metadata dtype",
        ));
    }
    Ok(())
}

#[cfg(feature = "parquet")]
fn parquet_read_tensor_column_values<R, T, F>(
    reader: &parquet::file::reader::SerializedFileReader<R>,
    expected_len: usize,
    mut read_column: F,
) -> Result<Vec<T>>
where
    R: 'static + parquet::file::reader::ChunkReader,
    F: FnMut(&dyn parquet::file::reader::RowGroupReader, usize) -> Result<Vec<T>>,
{
    use parquet::file::reader::FileReader;

    let mut expected_flat_index = 0usize;
    let mut values = Vec::with_capacity(expected_len);
    for row_group_index in 0..reader.num_row_groups() {
        let row_group = reader.get_row_group(row_group_index).map_err(parquet_err)?;
        let row_count = parquet_i64_to_usize(
            row_group.metadata().num_rows(),
            "Arcadia TIO tensor Parquet row group row count",
        )?;
        let flat_indices = parquet_read_i64_column(row_group.as_ref(), 0, row_count)?;
        parquet_validate_flat_indices(&flat_indices, &mut expected_flat_index)?;
        values.extend(read_column(row_group.as_ref(), row_count)?);
    }
    if expected_flat_index != expected_len || values.len() != expected_len {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet value count does not match shape",
        ));
    }
    Ok(values)
}

#[cfg(feature = "parquet")]
fn parquet_read_f32_column(
    row_group: &dyn parquet::file::reader::RowGroupReader,
    column_index: usize,
    expected_len: usize,
) -> Result<Vec<f32>> {
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    match row_group
        .get_column_reader(column_index)
        .map_err(parquet_err)?
    {
        parquet::column::reader::ColumnReader::FloatColumnReader(mut reader) => {
            let mut values = Vec::with_capacity(expected_len);
            let (records_read, values_read, levels_read) = reader
                .read_records(expected_len, None, None, &mut values)
                .map_err(parquet_err)?;
            parquet_validate_read_column(
                PARQUET_COLUMN_VALUE,
                expected_len,
                records_read,
                values_read,
                levels_read,
                values.len(),
            )?;
            Ok(values)
        }
        _ => Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet column has unexpected physical type",
        )),
    }
}

#[cfg(feature = "parquet")]
fn parquet_read_f64_column(
    row_group: &dyn parquet::file::reader::RowGroupReader,
    column_index: usize,
    expected_len: usize,
) -> Result<Vec<f64>> {
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    match row_group
        .get_column_reader(column_index)
        .map_err(parquet_err)?
    {
        parquet::column::reader::ColumnReader::DoubleColumnReader(mut reader) => {
            let mut values = Vec::with_capacity(expected_len);
            let (records_read, values_read, levels_read) = reader
                .read_records(expected_len, None, None, &mut values)
                .map_err(parquet_err)?;
            parquet_validate_read_column(
                PARQUET_COLUMN_VALUE,
                expected_len,
                records_read,
                values_read,
                levels_read,
                values.len(),
            )?;
            Ok(values)
        }
        _ => Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet column has unexpected physical type",
        )),
    }
}

#[cfg(feature = "parquet")]
fn parquet_read_i32_column(
    row_group: &dyn parquet::file::reader::RowGroupReader,
    column_index: usize,
    expected_len: usize,
) -> Result<Vec<i32>> {
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    match row_group
        .get_column_reader(column_index)
        .map_err(parquet_err)?
    {
        parquet::column::reader::ColumnReader::Int32ColumnReader(mut reader) => {
            let mut values = Vec::with_capacity(expected_len);
            let (records_read, values_read, levels_read) = reader
                .read_records(expected_len, None, None, &mut values)
                .map_err(parquet_err)?;
            parquet_validate_read_column(
                PARQUET_COLUMN_VALUE,
                expected_len,
                records_read,
                values_read,
                levels_read,
                values.len(),
            )?;
            Ok(values)
        }
        _ => Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet column has unexpected physical type",
        )),
    }
}

#[cfg(feature = "parquet")]
fn parquet_read_i64_column(
    row_group: &dyn parquet::file::reader::RowGroupReader,
    column_index: usize,
    expected_len: usize,
) -> Result<Vec<i64>> {
    if expected_len == 0 {
        return Ok(Vec::new());
    }
    match row_group
        .get_column_reader(column_index)
        .map_err(parquet_err)?
    {
        parquet::column::reader::ColumnReader::Int64ColumnReader(mut reader) => {
            let mut values = Vec::with_capacity(expected_len);
            let (records_read, values_read, levels_read) = reader
                .read_records(expected_len, None, None, &mut values)
                .map_err(parquet_err)?;
            parquet_validate_read_column(
                PARQUET_COLUMN_FLAT_INDEX,
                expected_len,
                records_read,
                values_read,
                levels_read,
                values.len(),
            )?;
            Ok(values)
        }
        _ => Err(TioError::invalid_argument(
            "Arcadia TIO tensor Parquet column has unexpected physical type",
        )),
    }
}

#[cfg(feature = "parquet")]
fn parquet_validate_read_column(
    column: &str,
    expected: usize,
    records_read: usize,
    values_read: usize,
    levels_read: usize,
    actual_len: usize,
) -> Result<()> {
    if records_read != expected
        || values_read != expected
        || levels_read != expected
        || actual_len != expected
    {
        return Err(TioError::invalid_argument(format!(
            "Arcadia TIO tensor Parquet column {column} read {actual_len} values, expected {expected}"
        )));
    }
    Ok(())
}

#[cfg(feature = "parquet")]
fn parquet_validate_flat_indices(indices: &[i64], expected_flat_index: &mut usize) -> Result<()> {
    for &flat_index in indices {
        let flat_index = usize::try_from(flat_index).map_err(|_| {
            TioError::invalid_argument(
                "Arcadia TIO tensor Parquet flat index is negative or too large",
            )
        })?;
        if flat_index != *expected_flat_index {
            return Err(TioError::invalid_argument(
                "Arcadia TIO tensor Parquet flat indices must be contiguous and ordered",
            ));
        }
        *expected_flat_index = expected_flat_index.checked_add(1).ok_or_else(|| {
            TioError::invalid_argument("Arcadia TIO tensor Parquet flat index count overflow")
        })?;
    }
    Ok(())
}

#[cfg(feature = "parquet")]
fn parquet_i64_to_usize(value: i64, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| TioError::invalid_argument(format!("{label} exceeds usize")))
}

#[cfg(feature = "parquet")]
fn parquet_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(format!("Arcadia TIO tensor Parquet error: {err}"))
}

#[cfg(feature = "parquet")]
fn parquet_io_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError {
        code: ErrorCode::Io,
        message: format!("Arcadia TIO tensor Parquet I/O error: {err}"),
    }
}

#[cfg(feature = "ndarray")]
fn tensor_to_ndarray<T: Clone>(shape: &[u64], values: &[T]) -> Result<ndarray::ArrayD<T>> {
    let shape = ndarray_shape_to_usize(shape)?;
    ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&shape), values.to_vec()).map_err(ndarray_err)
}

#[cfg(feature = "ndarray")]
fn ndarray_shape_to_usize(shape: &[u64]) -> Result<Vec<usize>> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    shape
        .iter()
        .map(|&dim| {
            usize::try_from(dim)
                .map_err(|_| TioError::invalid_argument("shape dimension does not fit usize"))
        })
        .collect()
}

#[cfg(feature = "ndarray")]
fn ndarray_shape_to_tensor_shape(shape: &[usize]) -> Result<Vec<u64>> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    shape
        .iter()
        .map(|&dim| {
            u64::try_from(dim)
                .map_err(|_| TioError::invalid_argument("shape dimension does not fit u64"))
        })
        .collect()
}

#[cfg(feature = "ndarray")]
fn ndarray_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(err.to_string())
}

/// Owned in-memory tensor operations over [`Tensor`] values.
///
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
pub mod ops {
    use super::{
        DType, Result, Scalar, Tensor, TensorData, TioError, shape_element_len,
        validate_tensor_parts,
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
            .map(|axis| {
                isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow"))
            })
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
            .map(|axis| {
                isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow"))
            })
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
            .map(|axis| {
                isize::try_from(axis).map_err(|_| TioError::invalid_argument("rank overflow"))
            })
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

        let mut out: Vec<Tensor> =
            fallible_vec_with_capacity(sections.len(), "tensor split outputs")?;
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
        let mut out: Vec<Tensor> =
            fallible_vec_with_capacity(shape[axis], "tensor unstack outputs")?;
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
            TensorData::F32(values) => {
                TensorData::F32(tile_values(values, &shape, &out_shape_usize)?)
            }
            TensorData::F64(values) => {
                TensorData::F64(tile_values(values, &shape, &out_shape_usize)?)
            }
            TensorData::I32(values) => {
                TensorData::I32(tile_values(values, &shape, &out_shape_usize)?)
            }
            TensorData::I64(values) => {
                TensorData::I64(tile_values(values, &shape, &out_shape_usize)?)
            }
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
                let mut out = reduce_sum_mapped_values(values, &shape, &plan, 0.0_f64, |a, b| {
                    Ok(a + b as f64)
                })?;
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
            TensorData::F32(values) => {
                Tensor::from_dense_f32(variance.shape, sqrt_f32_values(values)?)
            }
            TensorData::F64(values) => {
                Tensor::from_dense_f64(variance.shape, sqrt_f64_values(values)?)
            }
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
        let rank =
            isize::try_from(rank).map_err(|_| TioError::invalid_argument("rank overflow"))?;
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
        let rank =
            isize::try_from(rank).map_err(|_| TioError::invalid_argument("rank overflow"))?;
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
        out.try_reserve(len).map_err(|err| {
            TioError::invalid_argument(format!("{context} allocation failed: {err}"))
        })?;
        Ok(out)
    }

    fn fallible_filled_vec<T: Clone>(
        len: usize,
        value: T,
        context: &'static str,
    ) -> Result<Vec<T>> {
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
        let mut axis_prefix =
            fallible_vec_with_capacity(inputs.len() + 1, "tensor concat metadata")?;
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
            in_indices[axis] =
                (out_indices[axis] + input_shape[axis] - shift_norm) % input_shape[axis];
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
            let lhs_linear =
                broadcast_linear_index(&out_indices, lhs_shape, &lhs_strides, lhs_offset)?;
            let rhs_linear =
                broadcast_linear_index(&out_indices, rhs_shape, &rhs_strides, rhs_offset)?;
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
            .map(|value| {
                value.ok_or_else(|| TioError::invalid_argument("cannot reduce an empty axis"))
            })
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

    fn reduce_variance_f32(
        values: &[f32],
        shape: &[usize],
        plan: &ReductionPlan,
    ) -> Result<Vec<f32>> {
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

    fn reduce_variance_f64(
        values: &[f64],
        shape: &[usize],
        plan: &ReductionPlan,
    ) -> Result<Vec<f64>> {
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

    fn reduce_variance_i32(
        values: &[i32],
        shape: &[usize],
        plan: &ReductionPlan,
    ) -> Result<Vec<f64>> {
        reduce_variance_mapped(values, shape, plan, |value| f64::from(value))
    }

    fn reduce_variance_i64(
        values: &[i64],
        shape: &[usize],
        plan: &ReductionPlan,
    ) -> Result<Vec<f64>> {
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
}

/// Typed owned tensor operations over [`TypedTensor`] values.
///
/// This module forwards to the public untyped [`ops`] implementation and then validates that each
/// typed result has the expected dtype. The first slice intentionally covers dtype-preserving
/// operations plus `argmin`/`argmax` returning [`TensorI64`]; dtype-promoting `mean`, `var`, and
/// `std` remain available through [`ops`] on the untyped [`Tensor`] returned by
/// [`TypedTensor::as_tensor`].
pub mod typed_ops {
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
    pub fn stack<T: TensorElement>(
        tensors: &[&TypedTensor<T>],
        axis: isize,
    ) -> Result<TypedTensor<T>> {
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
    pub fn tile<T: TensorElement>(
        tensor: &TypedTensor<T>,
        reps: &[usize],
    ) -> Result<TypedTensor<T>> {
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
    pub fn add<T: TensorElement>(
        lhs: &TypedTensor<T>,
        rhs: &TypedTensor<T>,
    ) -> Result<TypedTensor<T>> {
        typed_from_result(ops::add(lhs.as_tensor(), rhs.as_tensor()))
    }

    /// Subtract typed tensors with exact dtype matching and broadcasting.
    pub fn sub<T: TensorElement>(
        lhs: &TypedTensor<T>,
        rhs: &TypedTensor<T>,
    ) -> Result<TypedTensor<T>> {
        typed_from_result(ops::sub(lhs.as_tensor(), rhs.as_tensor()))
    }

    /// Multiply typed tensors with exact dtype matching and broadcasting.
    pub fn mul<T: TensorElement>(
        lhs: &TypedTensor<T>,
        rhs: &TypedTensor<T>,
    ) -> Result<TypedTensor<T>> {
        typed_from_result(ops::mul(lhs.as_tensor(), rhs.as_tensor()))
    }

    /// Divide typed tensors with exact dtype matching and broadcasting.
    pub fn div<T: TensorElement>(
        lhs: &TypedTensor<T>,
        rhs: &TypedTensor<T>,
    ) -> Result<TypedTensor<T>> {
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
}

/// Dense read result with an optional validity mask copied into Rust memory.
#[derive(Debug, Clone, PartialEq)]
pub struct DenseTensor {
    /// Dense tensor payload.
    pub tensor: Tensor,
    /// Optional validity mask where `1` means valid and `0` means filled/null.
    pub mask: Option<Vec<u8>>,
}

/// Append entry range assigned by the native append call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppendRange {
    /// First appended entry id.
    pub start: u32,
    /// One-past-last appended entry id.
    pub end: u32,
}

/// Sparse-intent detector used to classify logically absent subtensors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseDetector {
    /// Treat a subtensor as absent when the native nullable representation marks it null.
    NullSubtensor,
    /// Treat a subtensor as absent when every value matches the supplied predicate.
    PredicateSubtensor,
}

impl SparseDetector {
    fn to_raw(self) -> sys::ArcadiaTioSparseDetectorKind {
        match self {
            Self::NullSubtensor => sys::ARCADIA_TIO_SPARSE_DETECTOR_NULL_SUBTENSOR,
            Self::PredicateSubtensor => sys::ARCADIA_TIO_SPARSE_DETECTOR_PREDICATE_SUBTENSOR,
        }
    }
}

/// Value predicate for sparse-intent absence detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SparseValuePredicate {
    /// Match IEEE NaN values.
    Nan,
    /// Match zero values.
    Zero,
    /// Match an exact `f32` value.
    EqualF32(f32),
    /// Match an exact `f64` value.
    EqualF64(f64),
    /// Match an exact `i32` value.
    EqualI32(i32),
    /// Match an exact `i64` value.
    EqualI64(i64),
}

impl SparseValuePredicate {
    fn to_raw(self) -> sys::ArcadiaTioSparseValuePredicate {
        let (kind, value) = match self {
            Self::Nan => (sys::ARCADIA_TIO_SPARSE_PREDICATE_NAN, 0.0),
            Self::Zero => (sys::ARCADIA_TIO_SPARSE_PREDICATE_ZERO, 0.0),
            Self::EqualF32(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F32, value as f64),
            Self::EqualF64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F64, value),
            Self::EqualI32(_) | Self::EqualI64(_) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_ZERO, 0.0),
        };
        sys::ArcadiaTioSparseValuePredicate { kind, value }
    }

    fn to_raw_v2(self) -> sys::ArcadiaTioSparseValuePredicateV2 {
        let (kind, float_value, integer_value) = match self {
            Self::Nan => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_NAN, 0.0, 0),
            Self::Zero => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_ZERO, 0.0, 0),
            Self::EqualF32(value) => (
                sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F32,
                value as f64,
                0,
            ),
            Self::EqualF64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_F64, value, 0),
            Self::EqualI32(value) => (
                sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I32,
                0.0,
                value as i64,
            ),
            Self::EqualI64(value) => (sys::ARCADIA_TIO_SPARSE_PREDICATE_V2_EQUAL_I64, 0.0, value),
        };
        sys::ArcadiaTioSparseValuePredicateV2 {
            kind,
            float_value,
            integer_value,
        }
    }
}

/// Fallback policy when native sparse lowering is not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseFallbackPolicy {
    /// Preserve exact values by appending densely when sparse lowering cannot be used.
    Dense,
}

impl SparseFallbackPolicy {
    fn to_raw(self) -> sys::ArcadiaTioSparseFallbackPolicy {
        match self {
            Self::Dense => sys::ARCADIA_TIO_SPARSE_FALLBACK_DENSE,
        }
    }
}

/// Safe sparse-intent rule used by f32/f64/i32/i64 sparse analysis and append helpers.
///
/// Integer payloads support [`SparseRule::null_subtensor`], [`SparseValuePredicate::Zero`],
/// and exact [`SparseValuePredicate::EqualI32`] / [`SparseValuePredicate::EqualI64`] predicates.
///
/// A rule owns the sparse-axis list and threshold settings. The wrapper validates the owned
/// axes against the open file before calling the C ABI so borrowed raw pointers only live for a
/// single FFI call. Sparse-intent diagnostics describe the current native lowering decision; they
/// are not storage-efficiency, compression-ratio, layout-superiority, or capacity claims.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseRule {
    sparse_axes: Vec<usize>,
    detector: SparseDetector,
    predicate: SparseValuePredicate,
    min_absent_fraction: f64,
    min_absent_subtensors: u64,
    fallback: SparseFallbackPolicy,
}

impl SparseRule {
    /// Creates a null-subtensor sparse rule for the provided non-append sparse axes.
    pub fn null_subtensor(sparse_axes: Vec<usize>) -> Self {
        Self {
            sparse_axes,
            detector: SparseDetector::NullSubtensor,
            predicate: SparseValuePredicate::Nan,
            min_absent_fraction: 0.0,
            min_absent_subtensors: 1,
            fallback: SparseFallbackPolicy::Dense,
        }
    }

    /// Creates a predicate-subtensor sparse rule for the provided non-append sparse axes.
    pub fn predicate_subtensor(sparse_axes: Vec<usize>, predicate: SparseValuePredicate) -> Self {
        Self {
            sparse_axes,
            detector: SparseDetector::PredicateSubtensor,
            predicate,
            min_absent_fraction: 0.0,
            min_absent_subtensors: 1,
            fallback: SparseFallbackPolicy::Dense,
        }
    }

    /// Returns the configured sparse axes.
    pub fn sparse_axes(&self) -> &[usize] {
        &self.sparse_axes
    }

    /// Returns the configured absence detector.
    pub fn detector(&self) -> SparseDetector {
        self.detector
    }

    /// Returns the configured predicate. It is ignored for null-subtensor rules.
    pub fn predicate(&self) -> SparseValuePredicate {
        self.predicate
    }

    /// Returns the minimum absent fraction threshold.
    pub fn min_absent_fraction(&self) -> f64 {
        self.min_absent_fraction
    }

    /// Returns the minimum absent subtensor-count threshold.
    pub fn min_absent_subtensors(&self) -> u64 {
        self.min_absent_subtensors
    }

    /// Returns the configured fallback policy.
    pub fn fallback(&self) -> SparseFallbackPolicy {
        self.fallback
    }

    /// Sets the minimum absent fraction required before sparse lowering is considered.
    pub fn with_min_absent_fraction(mut self, min_absent_fraction: f64) -> Self {
        self.min_absent_fraction = min_absent_fraction;
        self
    }

    /// Sets the minimum absent subtensor count required before sparse lowering is considered.
    pub fn with_min_absent_subtensors(mut self, min_absent_subtensors: u64) -> Self {
        self.min_absent_subtensors = min_absent_subtensors;
        self
    }

    /// Sets the fallback policy used when sparse lowering is not selected.
    pub fn with_fallback(mut self, fallback: SparseFallbackPolicy) -> Self {
        self.fallback = fallback;
        self
    }

    fn validate_for_append(&self, dtype: DType, rank: usize, append_axis: usize) -> Result<()> {
        if rank == 0 {
            return Err(TioError::invalid_argument(
                "sparse append shape rank must be non-zero",
            ));
        }
        if append_axis >= rank {
            return Err(TioError::invalid_argument(format!(
                "append axis {append_axis} out of range for rank {rank}"
            )));
        }
        if append_axis != 0 {
            return Err(TioError::invalid_argument(
                "sparse append currently supports append axis 0 only",
            ));
        }
        if self.sparse_axes.is_empty() {
            return Err(TioError::invalid_argument(
                "sparse rule sparse_axes must not be empty",
            ));
        }
        for (index, &axis) in self.sparse_axes.iter().enumerate() {
            if axis >= rank {
                return Err(TioError::invalid_argument(format!(
                    "sparse axis {axis} out of range for rank {rank}"
                )));
            }
            if axis == append_axis {
                return Err(TioError::invalid_argument(
                    "sparse axes must exclude the append axis",
                ));
            }
            if self.sparse_axes[..index].contains(&axis) {
                return Err(TioError::invalid_argument("sparse axes must be unique"));
            }
        }
        if !self.min_absent_fraction.is_finite() || !(0.0..=1.0).contains(&self.min_absent_fraction)
        {
            return Err(TioError::invalid_argument(
                "sparse rule min_absent_fraction must be finite and between 0.0 and 1.0",
            ));
        }
        if self.detector == SparseDetector::PredicateSubtensor {
            match (dtype, self.predicate) {
                (
                    DType::F32,
                    SparseValuePredicate::EqualF64(_)
                    | SparseValuePredicate::EqualI32(_)
                    | SparseValuePredicate::EqualI64(_),
                ) => {
                    return Err(TioError::invalid_argument(
                        "f32 sparse append cannot use this predicate dtype",
                    ));
                }
                (
                    DType::F64,
                    SparseValuePredicate::EqualF32(_)
                    | SparseValuePredicate::EqualI32(_)
                    | SparseValuePredicate::EqualI64(_),
                ) => {
                    return Err(TioError::invalid_argument(
                        "f64 sparse append cannot use this predicate dtype",
                    ));
                }
                (DType::I32, SparseValuePredicate::Zero | SparseValuePredicate::EqualI32(_)) => {}
                (DType::I64, SparseValuePredicate::Zero | SparseValuePredicate::EqualI64(_)) => {}
                (DType::I32 | DType::I64, _) => {
                    return Err(TioError::invalid_argument(
                        "integer sparse append predicate does not match tensor dtype",
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

/// Native sparse-intent analysis outcome copied into Rust-owned values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseAppendOutcome {
    /// Native analysis selected the RegularChunked sparse producer path.
    SparseRegularChunked,
    /// Native analysis selected dense append fallback.
    DenseFallback,
    /// Native analysis rejected the sparse-intent request.
    Reject,
    /// Native analysis selected the SparseChunkTree sparse producer path.
    SparseChunkTree,
}

impl SparseAppendOutcome {
    fn from_raw(value: sys::ArcadiaTioSparseAppendOutcome) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_SPARSE_APPEND_SPARSE_REGULAR_CHUNKED => Ok(Self::SparseRegularChunked),
            sys::ARCADIA_TIO_SPARSE_APPEND_DENSE_FALLBACK => Ok(Self::DenseFallback),
            sys::ARCADIA_TIO_SPARSE_APPEND_REJECT => Ok(Self::Reject),
            sys::ARCADIA_TIO_SPARSE_APPEND_SPARSE_CHUNK_TREE => Ok(Self::SparseChunkTree),
            other => Err(TioError::conversion(format!(
                "unknown sparse append outcome value {other}"
            ))),
        }
    }
}

/// Structured reason code explaining a sparse-intent analysis decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseAppendReason {
    /// No absent subtensors were detected in the append payload.
    NoAbsentSubtensorsDetected,
    /// Sparse axes must not be empty.
    SparseAxesMustNotBeEmpty,
    /// Sparse axes must be unique.
    SparseAxesMustBeUnique,
    /// Sparse axes must be within the file rank.
    SparseAxesOutOfBounds,
    /// Sparse axes must not include the append axis.
    SparseAxesMustExcludeAppendAxis,
    /// Current root sparse append supports append axis zero only.
    AppendAxisMustBeZeroForCurrentRootAppend,
    /// The predicate is not compatible with the payload dtype.
    PredicateDTypeMismatch,
    /// Dense fallback preserves exact values.
    DenseFallbackPreservesExactValues,
    /// Sparse lowering was below the configured threshold.
    SparseLoweringBelowThreshold,
    /// WholeAppendUnit layout has no current sparse producer path.
    WholeAppendUnitHasNoSparseProducerPath,
    /// RegularChunked block shape was not published for sparse lowering.
    RegularChunkedBlockShapeUnpublished,
    /// RegularChunked dense fallback requires stable non-append extents.
    RegularChunkedDenseFallbackRequiresStableNonAppendExtents,
    /// RegularChunked dense fallback requires a dense published lane set.
    RegularChunkedDenseFallbackRequiresDensePublishedLaneSet,
    /// RegularChunked sparse lowering requires a stable published lane set.
    RegularChunkedSparseLoweringRequiresStablePublishedLaneSet,
    /// The tensor contains nulls that dense fallback cannot preserve.
    TensorContainsNullsThatDenseFallbackCannotPreserve,
    /// Logical absence does not compile to the current sparse model.
    LogicalAbsenceDoesNotCompileToCurrentSparseModel,
    /// The current native sparse lowering is not implemented for this detector.
    CurrentSparseLoweringNotYetImplementedForDetector,
}

impl SparseAppendReason {
    fn from_raw(value: sys::ArcadiaTioSparseAppendReason) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_SPARSE_REASON_NO_ABSENT_SUBTENSORS_DETECTED => {
                Ok(Self::NoAbsentSubtensorsDetected)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_NOT_BE_EMPTY => {
                Ok(Self::SparseAxesMustNotBeEmpty)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_BE_UNIQUE => {
                Ok(Self::SparseAxesMustBeUnique)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_OUT_OF_BOUNDS => {
                Ok(Self::SparseAxesOutOfBounds)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_AXES_MUST_EXCLUDE_APPEND_AXIS => {
                Ok(Self::SparseAxesMustExcludeAppendAxis)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_APPEND_AXIS_MUST_BE_ZERO_FOR_CURRENT_ROOT_APPEND => {
                Ok(Self::AppendAxisMustBeZeroForCurrentRootAppend)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_PREDICATE_DTYPE_MISMATCH => {
                Ok(Self::PredicateDTypeMismatch)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_DENSE_FALLBACK_PRESERVES_EXACT_VALUES => {
                Ok(Self::DenseFallbackPreservesExactValues)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_SPARSE_LOWERING_BELOW_THRESHOLD => {
                Ok(Self::SparseLoweringBelowThreshold)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_WHOLE_APPEND_UNIT_HAS_NO_SPARSE_PRODUCER_PATH => {
                Ok(Self::WholeAppendUnitHasNoSparseProducerPath)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_BLOCK_SHAPE_UNPUBLISHED => {
                Ok(Self::RegularChunkedBlockShapeUnpublished)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_STABLE_NON_APPEND_EXTENTS => {
                Ok(Self::RegularChunkedDenseFallbackRequiresStableNonAppendExtents)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_DENSE_FALLBACK_REQUIRES_DENSE_PUBLISHED_LANE_SET => {
                Ok(Self::RegularChunkedDenseFallbackRequiresDensePublishedLaneSet)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_REGULAR_CHUNKED_SPARSE_LOWERING_REQUIRES_STABLE_PUBLISHED_LANE_SET => {
                Ok(Self::RegularChunkedSparseLoweringRequiresStablePublishedLaneSet)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_TENSOR_CONTAINS_NULLS_THAT_DENSE_FALLBACK_CANNOT_PRESERVE => {
                Ok(Self::TensorContainsNullsThatDenseFallbackCannotPreserve)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_LOGICAL_ABSENCE_DOES_NOT_COMPILE_TO_CURRENT_SPARSE_MODEL => {
                Ok(Self::LogicalAbsenceDoesNotCompileToCurrentSparseModel)
            }
            sys::ARCADIA_TIO_SPARSE_REASON_CURRENT_SPARSE_LOWERING_NOT_YET_IMPLEMENTED_FOR_DETECTOR => {
                Ok(Self::CurrentSparseLoweringNotYetImplementedForDetector)
            }
            other => Err(TioError::conversion(format!(
                "unknown sparse append reason value {other}"
            ))),
        }
    }
}

/// Rust-owned sparse-intent analysis report copied from native output.
#[derive(Debug, Clone, PartialEq)]
pub struct SparseAppendAnalysis {
    /// Selected native append outcome.
    pub outcome: SparseAppendOutcome,
    /// Fraction of detected absent subtensors considered by native analysis.
    pub absent_fraction: f64,
    /// Count of absent subtensors detected by native analysis.
    pub absent_subtensor_count: u64,
    /// Count of total subtensors considered by native analysis.
    pub total_subtensor_count: u64,
    /// Structured native reason codes copied into Rust memory.
    pub reasons: Vec<SparseAppendReason>,
}

fn empty_sparse_append_analysis() -> sys::ArcadiaTioSparseAppendAnalysis {
    sys::ArcadiaTioSparseAppendAnalysis {
        outcome: sys::ARCADIA_TIO_SPARSE_APPEND_REJECT,
        absent_fraction: 0.0,
        absent_subtensor_count: 0,
        total_subtensor_count: 0,
        reasons: ptr::null_mut(),
        reasons_len: 0,
    }
}

struct SparseAppendAnalysisGuard<'a> {
    raw: &'a mut sys::ArcadiaTioSparseAppendAnalysis,
}

impl Drop for SparseAppendAnalysisGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: The guard is created only for raw analysis values initialized by this wrapper or
        // native sparse analysis. Native free tolerates empty/null reason buffers and nulls the raw
        // output after releasing any owned reasons, preventing accidental double-free by callers.
        unsafe { sys::arcadia_tio_sparse_append_analysis_free(self.raw) };
    }
}

fn take_sparse_append_analysis(
    raw: &mut sys::ArcadiaTioSparseAppendAnalysis,
) -> Result<SparseAppendAnalysis> {
    let guard = SparseAppendAnalysisGuard { raw };
    if guard.raw.reasons.is_null() && guard.raw.reasons_len != 0 {
        return Err(TioError::conversion(
            "native sparse append analysis returned null reasons with non-zero length",
        ));
    }
    let raw_reasons = if guard.raw.reasons_len == 0 {
        &[][..]
    } else {
        // SAFETY: Successful native analysis returns `reasons` pointing to `reasons_len` values.
        // The guard frees the native analysis exactly once after this function copies the values;
        // it also runs on conversion errors caused by unknown outcome or reason codes.
        unsafe { slice::from_raw_parts(guard.raw.reasons.cast_const(), guard.raw.reasons_len) }
    };
    let reasons = raw_reasons
        .iter()
        .copied()
        .map(SparseAppendReason::from_raw)
        .collect::<Result<Vec<_>>>()?;
    Ok(SparseAppendAnalysis {
        outcome: SparseAppendOutcome::from_raw(guard.raw.outcome)?,
        absent_fraction: guard.raw.absent_fraction,
        absent_subtensor_count: guard.raw.absent_subtensor_count,
        total_subtensor_count: guard.raw.total_subtensor_count,
        reasons,
    })
}

/// Commit metadata returned by retained-history listing APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommitInfo {
    /// Native commit sequence number.
    pub commit_seq: u64,
    /// Footer offset for this commit in the native file.
    pub footer_offset: u64,
    /// Previous visible footer offset recorded for this commit.
    pub prev_footer_offset: u64,
}

impl From<sys::ArcadiaTioCommitInfo> for CommitInfo {
    fn from(raw: sys::ArcadiaTioCommitInfo) -> Self {
        Self {
            commit_seq: raw.commit_seq,
            footer_offset: raw.footer_offset,
            prev_footer_offset: raw.prev_footer_offset,
        }
    }
}

/// Native chunking plan copied into Rust-owned memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPlan {
    /// Block size per axis in native rank order.
    pub block_sizes: Vec<u32>,
}

/// 16-byte universe family/version identifier used by the C ABI.
pub type UniverseUuid = [u8; 16];

/// Axis identity mode used when creating universe-aware files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisIdentityMode {
    /// Axis identity is ordinary extent-only shape identity.
    ExtentOnly,
    /// Axis identity is universe-aware and can be targeted by explicit universe reads.
    UniverseAware,
}

impl AxisIdentityMode {
    fn to_raw(self) -> sys::ArcadiaTioAxisIdentityMode {
        match self {
            Self::ExtentOnly => sys::ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY,
            Self::UniverseAware => sys::ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE,
        }
    }
}

/// Create-time axis identity descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisIdentityInput {
    /// Axis index.
    pub axis: u32,
    /// Axis identity mode.
    pub mode: AxisIdentityMode,
}

impl AxisIdentityInput {
    /// Creates an extent-only axis identity descriptor.
    pub fn extent_only(axis: u32) -> Self {
        Self {
            axis,
            mode: AxisIdentityMode::ExtentOnly,
        }
    }

    /// Creates a universe-aware axis identity descriptor.
    pub fn universe_aware(axis: u32) -> Self {
        Self {
            axis,
            mode: AxisIdentityMode::UniverseAware,
        }
    }
}

/// Universe-aware create options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CreateUniverseOptions {
    /// Axis identity descriptors.
    pub axis_identities: Vec<AxisIdentityInput>,
}

impl CreateUniverseOptions {
    /// Creates universe options from axis identity descriptors.
    pub fn new(axis_identities: Vec<AxisIdentityInput>) -> Self {
        Self { axis_identities }
    }
}

/// Per-axis universe binding for one appended slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniverseBinding {
    /// Axis index.
    pub axis: u32,
    /// Universe family UUID.
    pub family_uuid: UniverseUuid,
    /// Universe version UUID.
    pub version_uuid: UniverseUuid,
    /// Source universe length.
    pub length: u64,
}

impl UniverseBinding {
    /// Creates a per-axis universe binding.
    pub fn new(
        axis: u32,
        family_uuid: UniverseUuid,
        version_uuid: UniverseUuid,
        length: u64,
    ) -> Self {
        Self {
            axis,
            family_uuid,
            version_uuid,
            length,
        }
    }
}

/// Universe bindings for one appended slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotUniverseBindings {
    /// Axis bindings for this appended slot.
    pub axes: Vec<UniverseBinding>,
}

impl SlotUniverseBindings {
    /// Creates slot bindings from per-axis universe bindings.
    pub fn new(axes: Vec<UniverseBinding>) -> Self {
        Self { axes }
    }
}

/// Payload-driven universe remap for one axis in one appended slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UniverseRemap {
    /// Axis index.
    pub axis: u32,
    /// Target universe family UUID.
    pub target_family_uuid: UniverseUuid,
    /// Target universe version UUID.
    pub target_version_uuid: UniverseUuid,
    /// Target universe length.
    pub target_length: u64,
    /// Source index to target index mapping.
    pub source_to_target: Vec<u64>,
}

impl UniverseRemap {
    /// Creates a payload-driven universe remap.
    pub fn new(
        axis: u32,
        target_family_uuid: UniverseUuid,
        target_version_uuid: UniverseUuid,
        target_length: u64,
        source_to_target: Vec<u64>,
    ) -> Self {
        Self {
            axis,
            target_family_uuid,
            target_version_uuid,
            target_length,
            source_to_target,
        }
    }
}

/// Universe remaps for one appended slot.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotUniverseRemaps {
    /// Axis remaps for this appended slot.
    pub axes: Vec<UniverseRemap>,
}

impl SlotUniverseRemaps {
    /// Creates slot remaps from per-axis universe remaps.
    pub fn new(axes: Vec<UniverseRemap>) -> Self {
        Self { axes }
    }
}

/// Universe-aware append options.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppendWithUniverseOptions {
    /// Per-appended-slot universe bindings.
    pub slots: Vec<SlotUniverseBindings>,
    /// Optional per-appended-slot universe remaps.
    pub remap_slots: Vec<SlotUniverseRemaps>,
}

impl AppendWithUniverseOptions {
    /// Creates append options from per-slot universe bindings.
    pub fn new(slots: Vec<SlotUniverseBindings>) -> Self {
        Self {
            slots,
            remap_slots: Vec::new(),
        }
    }
}

/// Explicit universe target for shape-policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitUniverseAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Target universe family UUID.
    pub family_uuid: UniverseUuid,
    /// Target universe version UUID.
    pub version_uuid: UniverseUuid,
    /// Target universe length.
    pub length: u64,
}

impl ExplicitUniverseAxisTarget {
    /// Creates an explicit universe axis target.
    pub fn new(
        axis: u32,
        family_uuid: UniverseUuid,
        version_uuid: UniverseUuid,
        length: u64,
    ) -> Self {
        Self {
            axis,
            family_uuid,
            version_uuid,
            length,
        }
    }
}

/// Explicit extent target for split-domain shape-policy reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExplicitExtentAxisTarget {
    /// Axis index.
    pub axis: u32,
    /// Target axis length.
    pub length: u64,
}

impl ExplicitExtentAxisTarget {
    /// Creates an explicit extent axis target.
    pub fn new(axis: u32, length: u64) -> Self {
        Self { axis, length }
    }
}

/// Shape policy for current and historical reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadShapePolicy {
    /// Use the file envelope shape. This matches bare/current `read_all` defaults.
    FileEnvelope,
    /// Use the current head shape.
    CurrentHead,
    /// Use the union of selected entry shapes.
    Union,
    /// Use the intersection of selected entry shapes.
    Intersection,
    /// Use the initially registered extents.
    InitialRegistered,
    /// Use explicit extents for all axes.
    ExplicitExtents(Vec<u64>),
    /// Use explicit universe targets for universe-aware axes.
    ExplicitUniverse(Vec<ExplicitUniverseAxisTarget>),
    /// Use explicit universe targets for universe-aware axes and explicit extents for extent-only axes.
    ExplicitUniverseAndExtents {
        /// Universe-aware axis targets.
        universe_axes: Vec<ExplicitUniverseAxisTarget>,
        /// Extent-only axis targets.
        extent_axes: Vec<ExplicitExtentAxisTarget>,
    },
}

impl ReadShapePolicy {
    fn to_raw_tag(&self) -> sys::ArcadiaTioReadShapePolicyTag {
        match self {
            Self::FileEnvelope => sys::ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE,
            Self::CurrentHead => sys::ARCADIA_TIO_READ_SHAPE_POLICY_CURRENT_HEAD,
            Self::Union => sys::ARCADIA_TIO_READ_SHAPE_POLICY_UNION,
            Self::Intersection => sys::ARCADIA_TIO_READ_SHAPE_POLICY_INTERSECTION,
            Self::InitialRegistered => sys::ARCADIA_TIO_READ_SHAPE_POLICY_INITIAL_REGISTERED,
            Self::ExplicitExtents(_) => sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_EXTENTS,
            Self::ExplicitUniverse(_) => sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE,
            Self::ExplicitUniverseAndExtents { .. } => {
                sys::ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS
            }
        }
    }
}

/// Read execution mode for option-bearing reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadExecutionMode {
    /// Serial execution.
    Serial,
    /// Native parallel thread execution with a maximum thread count.
    ParallelThreads { max_threads: usize },
}

impl ReadExecutionMode {
    /// Serial execution.
    pub fn serial() -> Self {
        Self::Serial
    }

    /// Native parallel thread execution with a maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self::ParallelThreads { max_threads }
    }

    fn to_raw(self) -> Result<(sys::ArcadiaTioReadExecutionMode, usize)> {
        match self {
            Self::Serial => Ok((sys::ARCADIA_TIO_READ_EXECUTION_SERIAL, 1)),
            Self::ParallelThreads { max_threads } if max_threads > 0 => Ok((
                sys::ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS,
                max_threads,
            )),
            Self::ParallelThreads { .. } => Err(TioError::invalid_argument(
                "parallel read max_threads must be > 0",
            )),
        }
    }

    fn from_raw(value: sys::ArcadiaTioReadExecutionMode, threads: usize) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_READ_EXECUTION_SERIAL => Ok(Self::Serial),
            sys::ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS => Ok(Self::ParallelThreads {
                max_threads: threads,
            }),
            other => Err(TioError::conversion(format!(
                "unknown read execution mode value {other}"
            ))),
        }
    }
}

/// Current read options with execution mode only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadWithOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
}

impl ReadWithOptions {
    /// Serial read execution.
    pub fn serial() -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
        }
    }

    /// Parallel read execution with the provided maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
        }
    }
}

/// Current read options with execution mode and shape policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadWithShapePolicyOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
    /// Shape policy.
    pub shape_policy: ReadShapePolicy,
}

impl ReadWithShapePolicyOptions {
    /// Serial read with the provided shape policy.
    pub fn serial(shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
            shape_policy,
        }
    }

    /// Parallel read with the provided maximum thread count and shape policy.
    pub fn parallel_threads(max_threads: usize, shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
            shape_policy,
        }
    }
}

/// Query-attribution context for opt-in diagnostic current reads.
///
/// All string fields are owned Rust strings. The safe wrapper converts them to temporary C strings
/// for the attributed read call and never exposes the borrowed native pointers in public Rust.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTraceContext {
    /// Run identifier copied into native trace metadata.
    pub run_id: String,
    /// Result-row identifier copied into native trace metadata.
    pub row_id: String,
    /// Repeat index copied into native trace metadata.
    pub repeat_index: u32,
    /// Phase label copied into native trace metadata.
    pub phase: String,
    /// Language label copied into native trace metadata.
    pub language: String,
    /// Public API surface label copied into native trace metadata.
    pub api_surface: String,
    /// Operation label copied into native trace metadata.
    pub operation: String,
    /// Trace-clock label copied into native trace metadata.
    pub trace_clock: String,
}

impl QueryTraceContext {
    /// Creates a query-attribution context for a single diagnostic read.
    pub fn new(
        run_id: impl Into<String>,
        row_id: impl Into<String>,
        phase: impl Into<String>,
        language: impl Into<String>,
        api_surface: impl Into<String>,
        operation: impl Into<String>,
        trace_clock: impl Into<String>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            row_id: row_id.into(),
            repeat_index: 0,
            phase: phase.into(),
            language: language.into(),
            api_surface: api_surface.into(),
            operation: operation.into(),
            trace_clock: trace_clock.into(),
        }
    }

    /// Sets the repeat index included in native trace metadata.
    pub fn with_repeat_index(mut self, repeat_index: u32) -> Self {
        self.repeat_index = repeat_index;
        self
    }
}

/// Native query-attribution trace JSON copied into Rust memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryTraceJson {
    /// JSON text following the native `tio_query_attribution_trace.v1` schema.
    pub json: String,
}

impl QueryTraceJson {
    /// Returns the owned trace JSON text as `str`.
    pub fn as_str(&self) -> &str {
        &self.json
    }

    /// Consumes the trace wrapper and returns the owned JSON text.
    pub fn into_string(self) -> String {
        self.json
    }
}

/// Current attributed read value with execution metadata and diagnostic trace JSON.
#[derive(Debug, Clone, PartialEq)]
pub struct AttributedReadResult<T> {
    /// Read value.
    pub value: T,
    /// Execution metadata.
    pub execution: ReadExecutionReport,
    /// Query-attribution trace JSON copied from native-owned output.
    pub trace: QueryTraceJson,
}

/// Historical read options with execution mode only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadWithOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
}

impl HistoricalReadWithOptions {
    /// Serial historical read execution.
    pub fn serial() -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
        }
    }

    /// Parallel historical read execution with the provided maximum thread count.
    pub fn parallel_threads(max_threads: usize) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
        }
    }
}

/// Historical read options with execution mode and shape policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadWithShapePolicyOptions {
    /// Requested execution mode.
    pub mode: ReadExecutionMode,
    /// Shape policy evaluated against the selected historical snapshot.
    pub shape_policy: ReadShapePolicy,
}

impl HistoricalReadWithShapePolicyOptions {
    /// Serial historical read with the provided shape policy.
    pub fn serial(shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::Serial,
            shape_policy,
        }
    }

    /// Parallel historical read with the provided maximum thread count and shape policy.
    pub fn parallel_threads(max_threads: usize, shape_policy: ReadShapePolicy) -> Self {
        Self {
            mode: ReadExecutionMode::ParallelThreads { max_threads },
            shape_policy,
        }
    }
}

/// Safe selector for current and historical read APIs and scoped mutation APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntrySelector {
    /// Select all indices along this axis.
    All,
    /// Select a half-open range along this axis.
    Range { start: u32, end: u32 },
    /// Select explicit indices along this axis.
    Take(Vec<u32>),
}

/// Basic read-index item for the native `read_index` lowering path.
///
/// This intentionally exposes the bounded C ABI first slice: `all`, `slice`, scalar `index`,
/// `new_axis`, and `ellipsis`. Advanced array/mask indexing is not part of this API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadIndexItem {
    /// Select all values along one input axis.
    All,
    /// Select a Python-style half-open slice with optional start/end and a non-zero step.
    Slice {
        /// Optional inclusive start bound.
        start: Option<i64>,
        /// Optional exclusive end bound.
        end: Option<i64>,
        /// Slice step; must be non-zero.
        step: i64,
    },
    /// Select a single scalar index along one input axis.
    Index(i64),
    /// Insert a length-one output axis.
    NewAxis,
    /// Expand to the remaining input axes during native normalization.
    Ellipsis,
}

impl ReadIndexItem {
    /// Selects all values along one input axis.
    pub fn all() -> Self {
        Self::All
    }

    /// Creates a bounded or open-ended slice with a non-zero step.
    pub fn slice(start: Option<i64>, end: Option<i64>, step: i64) -> Result<Self> {
        if step == 0 {
            return Err(TioError::invalid_argument(
                "read_index slice step must not be zero",
            ));
        }
        Ok(Self::Slice { start, end, step })
    }

    /// Selects a single scalar index along one input axis.
    pub fn index(index: i64) -> Self {
        Self::Index(index)
    }

    /// Inserts a length-one output axis.
    pub fn new_axis() -> Self {
        Self::NewAxis
    }

    /// Expands to the remaining input axes during native normalization.
    pub fn ellipsis() -> Self {
        Self::Ellipsis
    }

    fn to_raw(&self) -> Result<sys::ArcadiaTioReadIndexItem> {
        match self {
            Self::All => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_ALL)),
            Self::Slice { start, end, step } => {
                if *step == 0 {
                    return Err(TioError::invalid_argument(
                        "read_index slice step must not be zero",
                    ));
                }
                Ok(sys::ArcadiaTioReadIndexItem {
                    kind: sys::ARCADIA_TIO_READ_INDEX_SLICE,
                    has_start: u8::from(start.is_some()),
                    start: start.unwrap_or_default(),
                    has_end: u8::from(end.is_some()),
                    end: end.unwrap_or_default(),
                    step: *step,
                    index: 0,
                })
            }
            Self::Index(index) => {
                let mut raw = raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_INDEX);
                raw.index = *index;
                Ok(raw)
            }
            Self::NewAxis => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_NEW_AXIS)),
            Self::Ellipsis => Ok(raw_read_index_item(sys::ARCADIA_TIO_READ_INDEX_ELLIPSIS)),
        }
    }
}

fn raw_read_index_item(kind: sys::ArcadiaTioReadIndexItemTag) -> sys::ArcadiaTioReadIndexItem {
    sys::ArcadiaTioReadIndexItem {
        kind,
        has_start: 0,
        start: 0,
        has_end: 0,
        end: 0,
        step: 1,
        index: 0,
    }
}

/// Chunk key used by clear-block mutation APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkKey {
    coords: Vec<u32>,
}

impl ChunkKey {
    /// Creates a chunk key from chunk coordinates.
    pub fn new(coords: Vec<u32>) -> Self {
        Self { coords }
    }

    /// Returns the chunk coordinates.
    pub fn coords(&self) -> &[u32] {
        &self.coords
    }
}

impl From<Vec<u32>> for ChunkKey {
    fn from(coords: Vec<u32>) -> Self {
        Self::new(coords)
    }
}

/// Current read execution metadata copied from the native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadExecutionReport {
    /// Requested execution mode.
    pub requested_mode: ReadExecutionMode,
    /// Requested maximum query threads.
    pub query_max_threads: usize,
    /// Effective execution mode.
    pub query_effective_mode: ReadExecutionMode,
    /// Effective query threads.
    pub query_effective_threads: usize,
    /// Query parallel runtime if reported.
    pub query_parallel_runtime: Option<String>,
    /// Query parallel fallback reason if reported.
    pub query_parallel_fallback_reason: Option<String>,
    /// Query parallel reason code if reported.
    pub query_parallel_reason_code: Option<String>,
    /// Query parallel reason-code taxonomy if reported.
    pub query_parallel_reason_code_taxonomy: Option<String>,
}

/// Native lowering path reported by `read_index`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadIndexLoweringKind {
    /// Native code did not report a recognized lowering path.
    Unknown,
    /// Lowered directly to selector reads.
    SelectorRead,
    /// Lowered to selector reads plus shape post-processing for scalar/new-axis items.
    SelectorReadWithShapePostprocess,
}

impl ReadIndexLoweringKind {
    fn from_raw(value: sys::ArcadiaTioReadIndexLoweringKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ => Ok(Self::SelectorRead),
            sys::ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ_WITH_SHAPE_POSTPROCESS => {
                Ok(Self::SelectorReadWithShapePostprocess)
            }
            other => Err(TioError::conversion(format!(
                "unknown read_index lowering kind value {other}"
            ))),
        }
    }
}

/// Rust-owned `read_index` lowering report copied from native output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadIndexReport {
    /// Lowering strategy selected by native code.
    pub lowering_kind: ReadIndexLoweringKind,
    /// Whether native code used a full-tensor fallback.
    pub used_full_tensor_fallback: bool,
}

/// Current read-index value with lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadIndexResult {
    /// Read value.
    pub value: Tensor,
    /// Lowering metadata.
    pub report: ReadIndexReport,
}

/// Current dense read-index value with validity mask and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadIndexDenseResult {
    /// Dense read value and optional validity mask.
    pub value: DenseTensor,
    /// Lowering metadata.
    pub report: ReadIndexReport,
}

/// Historical read-index execution and lowering metadata copied from native output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadIndexReport {
    /// Historical execution metadata.
    pub execution: HistoricalReadExecutionReport,
    /// Read-index lowering metadata.
    pub read_index: ReadIndexReport,
}

/// Historical read-index value with execution and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadIndexResult {
    /// Read value.
    pub value: Tensor,
    /// Historical execution and lowering metadata.
    pub report: HistoricalReadIndexReport,
}

/// Historical dense read-index value with validity mask plus execution and lowering metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadIndexDenseResult {
    /// Dense read value and optional validity mask.
    pub value: DenseTensor,
    /// Historical execution and lowering metadata.
    pub report: HistoricalReadIndexReport,
}

/// RAII owner for an Arrow C Data array/schema pair returned by native full-value export.
///
/// The pointers exposed by [`ArrowCData::array`] and [`ArrowCData::schema`] are borrowed and remain
/// valid only while this value is alive. Dropping this value invokes non-null Arrow `release`
/// callbacks exactly once. This is a bounded interop surface; it is not a generic zero-copy or
/// performance guarantee.
pub struct ArrowCData {
    array: sys::ArrowArray,
    schema: sys::ArrowSchema,
    _not_send_or_sync: PhantomData<Rc<()>>,
}

impl ArrowCData {
    /// Returns the borrowed Arrow C Data array carrier.
    pub fn array(&self) -> &sys::ArrowArray {
        &self.array
    }

    /// Returns the borrowed Arrow C Data schema carrier.
    pub fn schema(&self) -> &sys::ArrowSchema {
        &self.schema
    }

    /// Returns a raw borrowed pointer to the Arrow C Data array carrier.
    pub fn array_ptr(&self) -> *const sys::ArrowArray {
        &self.array
    }

    /// Returns a raw borrowed pointer to the Arrow C Data schema carrier.
    pub fn schema_ptr(&self) -> *const sys::ArrowSchema {
        &self.schema
    }
}

impl Drop for ArrowCData {
    fn drop(&mut self) {
        // SAFETY: The native Arrow C Data contract transfers ownership of any non-null release
        // callbacks to the caller. This RAII owner invokes each callback at most once on drop.
        unsafe {
            release_arrow_array(&mut self.array);
            release_arrow_schema(&mut self.schema);
        }
    }
}

unsafe fn release_arrow_array(array: *mut sys::ArrowArray) {
    // SAFETY: Caller guarantees `array` is a writable ArrowArray slot. A non-null release callback
    // means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*array).release } {
        unsafe { release(array) };
    }
}

unsafe fn release_arrow_schema(schema: *mut sys::ArrowSchema) {
    // SAFETY: Caller guarantees `schema` is a writable ArrowSchema slot. A non-null release
    // callback means the slot owns Arrow C Data resources that must be released by the caller.
    if let Some(release) = unsafe { (*schema).release } {
        unsafe { release(schema) };
    }
}

impl fmt::Debug for ArrowCData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ArrowCData")
            .field("array_length", &self.array.length)
            .field("array_n_buffers", &self.array.n_buffers)
            .field("array_n_children", &self.array.n_children)
            .field("schema_format", &optional_c_string(self.schema.format))
            .finish_non_exhaustive()
    }
}

/// Historical query source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HistoricalQuerySourceKind {
    /// Query used a retained visible commit snapshot.
    RetainedVisibleCommit,
}

impl HistoricalQuerySourceKind {
    fn from_raw(value: sys::ArcadiaTioHistoricalQuerySourceKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT => {
                Ok(Self::RetainedVisibleCommit)
            }
            other => Err(TioError::conversion(format!(
                "unknown historical query source kind value {other}"
            ))),
        }
    }
}

/// Historical read execution metadata copied from the native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoricalReadExecutionReport {
    /// Current-read execution fields.
    pub execution: ReadExecutionReport,
    /// Historical query source kind.
    pub query_source_kind: HistoricalQuerySourceKind,
    /// Commit sequence used for the historical query.
    pub query_commit_seq: u64,
}

/// Current read value with execution metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadResult<T> {
    /// Read value.
    pub value: T,
    /// Execution metadata.
    pub execution: ReadExecutionReport,
}

/// Historical read value with execution metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct HistoricalReadResult<T> {
    /// Read value.
    pub value: T,
    /// Historical execution metadata.
    pub execution: HistoricalReadExecutionReport,
}

/// Compaction mode used by compaction workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompactionMode {
    /// Copy live data without reblocking.
    #[default]
    CopyLive,
    /// Reblock live data with the requested entry block size.
    Reblock { entry_block_size: u32 },
}

impl CompactionMode {
    fn to_raw(self) -> sys::ArcadiaTioCompactionMode {
        match self {
            Self::CopyLive => sys::ArcadiaTioCompactionMode {
                kind: sys::ARCADIA_TIO_COMPACTION_COPY_LIVE,
                reblock_entry_block_size: 0,
            },
            Self::Reblock { entry_block_size } => sys::ArcadiaTioCompactionMode {
                kind: sys::ARCADIA_TIO_COMPACTION_REBLOCK,
                reblock_entry_block_size: entry_block_size,
            },
        }
    }

    fn from_raw(value: sys::ArcadiaTioCompactionMode) -> Result<Self> {
        match value.kind {
            sys::ARCADIA_TIO_COMPACTION_COPY_LIVE => Ok(Self::CopyLive),
            sys::ARCADIA_TIO_COMPACTION_REBLOCK => Ok(Self::Reblock {
                entry_block_size: value.reblock_entry_block_size,
            }),
            other => Err(TioError::conversion(format!(
                "unknown compaction mode value {other}"
            ))),
        }
    }
}

/// Shallow compatibility compaction stats.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionStats {
    /// Native-reported live bytes.
    pub live_bytes: u64,
    /// Native-reported dead bytes.
    pub dead_bytes: u64,
    /// Native-reported dead-byte ratio.
    pub dead_ratio: f64,
    /// Number of commits represented by the file.
    pub commit_count: u32,
}

/// Status returned by status-aware V4 report APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4ReportStatus {
    /// Report completed.
    Complete,
    /// Report family is unsupported for this file/operation.
    Unsupported,
    /// Report outcome is unknown.
    Unknown,
    /// A future native status value preserved in-band.
    Other(i32),
}

impl V4ReportStatus {
    fn from_raw(value: sys::ArcadiaTioV4ReportStatus) -> Self {
        match value {
            sys::ARCADIA_TIO_V4_REPORT_COMPLETE => Self::Complete,
            sys::ARCADIA_TIO_V4_REPORT_UNSUPPORTED => Self::Unsupported,
            sys::ARCADIA_TIO_V4_REPORT_UNKNOWN => Self::Unknown,
            other => Self::Other(other),
        }
    }
}

/// Precise-accounting field ids that can be requested or omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4PreciseAccountingField {
    /// Source-file bytes unreachable from the selected report view.
    UnreachableBytes,
    /// Bytes required to retain requested history.
    RetainedHistoryRequiredBytes,
    /// Bytes skipped due to pop/revert semantics.
    PoppedSkippedBytes,
    /// Bytes reclaimable by the selected workflow.
    ReclaimableBytes,
    /// A future native precise-accounting field id preserved in-band.
    Other(i32),
}

impl V4PreciseAccountingField {
    /// Returns this field's single-bit request mask.
    pub fn mask(self) -> u32 {
        match self.to_raw() {
            raw if raw >= 0 && raw < u32::BITS as i32 => 1u32 << raw,
            _ => 0,
        }
    }

    fn from_raw(value: sys::ArcadiaTioV4PreciseAccountingField) -> Self {
        match value {
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES => Self::UnreachableBytes,
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES => {
                Self::RetainedHistoryRequiredBytes
            }
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES => Self::PoppedSkippedBytes,
            sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES => Self::ReclaimableBytes,
            other => Self::Other(other),
        }
    }

    fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingField {
        match self {
            Self::UnreachableBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES,
            Self::RetainedHistoryRequiredBytes => {
                sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES
            }
            Self::PoppedSkippedBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES,
            Self::ReclaimableBytes => sys::ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES,
            Self::Other(value) => value,
        }
    }
}

/// Options for precise-accounting report APIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4PreciseAccountingOptions {
    /// Zero requests every precise field relevant to the report family.
    pub requested_fields_mask: u32,
    /// Whether native should include human-readable omitted-field reason strings.
    pub include_omitted_field_reasons: bool,
}

impl V4PreciseAccountingOptions {
    /// Requests every precise-accounting field relevant to the report family.
    pub fn all() -> Self {
        Self {
            requested_fields_mask: 0,
            include_omitted_field_reasons: true,
        }
    }

    /// Requests only the provided precise-accounting fields.
    pub fn fields(fields: impl IntoIterator<Item = V4PreciseAccountingField>) -> Self {
        Self {
            requested_fields_mask: fields
                .into_iter()
                .fold(0u32, |mask, field| mask | field.mask()),
            include_omitted_field_reasons: true,
        }
    }

    fn to_raw(self) -> sys::ArcadiaTioV4PreciseAccountingOptions {
        sys::ArcadiaTioV4PreciseAccountingOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioV4PreciseAccountingOptions>(),
            requested_fields_mask: self.requested_fields_mask,
            include_omitted_field_reasons: u8::from(self.include_omitted_field_reasons),
        }
    }
}

impl Default for V4PreciseAccountingOptions {
    fn default() -> Self {
        Self::all()
    }
}

/// Omitted precise-accounting field metadata copied from a native report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4OmittedPreciseAccountingField {
    /// Omitted field id.
    pub field: V4PreciseAccountingField,
    /// Optional human-readable reason.
    pub reason: Option<String>,
    /// Optional stable reason code aligned with this omitted field.
    pub reason_code: Option<String>,
}

/// Precise-accounting bytes with native validity flags preserved as `Option` values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4PreciseAccountingBytes {
    /// Precise unreachable bytes when available.
    pub unreachable_bytes: Option<u64>,
    /// Precise retained-history-required bytes when available.
    pub retained_history_required_bytes: Option<u64>,
    /// Precise popped/skipped bytes when available.
    pub popped_skipped_bytes: Option<u64>,
    /// Precise reclaimable bytes when available.
    pub reclaimable_bytes: Option<u64>,
    /// Fields intentionally omitted by native accounting.
    pub omitted_fields: Vec<V4OmittedPreciseAccountingField>,
}

/// V4 current-head byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4CurrentHeadBytes {
    /// Payload bytes.
    pub payload_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// Epoch bytes.
    pub epoch_bytes: u64,
    /// Auxiliary bytes.
    pub aux_bytes: u64,
    /// Commit bytes.
    pub commit_bytes: u64,
}

/// V4 visible-chain audit byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4AuditBytes {
    /// Commit bytes.
    pub commit_bytes: u64,
    /// Index bytes.
    pub index_bytes: u64,
    /// Epoch bytes.
    pub epoch_bytes: u64,
    /// Auxiliary bytes.
    pub aux_bytes: u64,
}

/// V4 payload-reuse byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4PayloadReuseBytes {
    /// Payload bytes resurrected from previous commits.
    pub resurrected_payload_bytes: u64,
    /// Payload bytes shared with other visible data.
    pub shared_payload_bytes: u64,
}

/// V4 superseded byte breakdown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4SupersededBytes {
    /// Superseded payload bytes.
    pub payload_bytes: u64,
    /// Superseded index bytes.
    pub index_bytes: u64,
    /// Superseded epoch bytes.
    pub epoch_bytes: u64,
    /// Superseded auxiliary bytes.
    pub aux_bytes: u64,
}

/// V4 compaction analysis policy reported by the native API.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4CompactionAnalysisPolicy {
    /// Analyze compaction to the current visible state.
    CompactToCurrentState,
}

impl V4CompactionAnalysisPolicy {
    fn from_raw(value: sys::ArcadiaTioV4CompactionAnalysisPolicy) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE => {
                Ok(Self::CompactToCurrentState)
            }
            other => Err(TioError::conversion(format!(
                "unknown V4 compaction analysis policy value {other}"
            ))),
        }
    }
}

/// Non-precise V4 source-file diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4DiagnosticsReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Current-head byte breakdown.
    pub current_head: V4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: V4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: V4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: V4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise V4 source-file diagnostics report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4DiagnosticsPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Current-head byte breakdown.
    pub current_head: V4CurrentHeadBytes,
    /// Visible-chain audit bytes.
    pub visible_chain_audit: V4AuditBytes,
    /// Payload reuse bytes.
    pub payload_reuse: V4PayloadReuseBytes,
    /// Superseded bytes.
    pub superseded: V4SupersededBytes,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting bytes and omitted-field metadata.
    pub precise_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Non-precise V4 ordinary compaction analysis report.
#[derive(Debug, Clone, PartialEq)]
pub struct V4CompactionAnalysisReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Native compaction policy analyzed.
    pub policy: V4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise V4 ordinary compaction analysis report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4CompactionAnalysisPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Native compaction policy analyzed.
    pub policy: V4CompactionAnalysisPolicy,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Bytes required for current-state compaction.
    pub current_state_required_bytes: u64,
    /// Ordinary reclaimable bytes.
    pub ordinary_reclaimable_bytes: u64,
    /// Bytes the report cannot classify.
    pub unknown_bytes: u64,
    /// Precise-accounting bytes and omitted-field metadata.
    pub precise_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Options for ordinary compaction helpers.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CompactionOptions {
    /// Number of commits to retain.
    pub retain_commits: u32,
    /// Compaction mode.
    pub mode: CompactionMode,
    /// Dead-byte ratio threshold for conditional compaction.
    pub dead_ratio_threshold: f64,
    /// Minimum dead bytes for conditional compaction.
    pub min_dead_bytes: u64,
}

impl Default for CompactionOptions {
    fn default() -> Self {
        Self {
            retain_commits: 1,
            mode: CompactionMode::CopyLive,
            dead_ratio_threshold: 0.3,
            min_dead_bytes: 0,
        }
    }
}

/// Auto-compaction metadata configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AutoCompactionConfig {
    /// Whether auto-compaction is enabled.
    pub enabled: bool,
    /// Number of commits to retain.
    pub retain_commits: u32,
    /// Dead-byte ratio threshold.
    pub dead_ratio_threshold: f64,
    /// Minimum dead bytes before auto-compaction can trigger.
    pub min_dead_bytes: u64,
    /// Compaction mode.
    pub mode: CompactionMode,
    /// Commit interval for auto-compaction checks.
    pub check_every_commits: u32,
    /// Commit cooldown after compaction.
    pub cooldown_commits: u32,
}

impl Default for AutoCompactionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            retain_commits: 1,
            dead_ratio_threshold: 0.3,
            min_dead_bytes: 0,
            mode: CompactionMode::CopyLive,
            check_every_commits: 1,
            cooldown_commits: 0,
        }
    }
}

impl AutoCompactionConfig {
    fn to_raw(self) -> sys::ArcadiaTioAutoCompactionConfig {
        sys::ArcadiaTioAutoCompactionConfig {
            enabled: u8::from(self.enabled),
            retain_commits: self.retain_commits,
            dead_ratio_threshold: self.dead_ratio_threshold,
            min_dead_bytes: self.min_dead_bytes,
            mode: self.mode.to_raw(),
            check_every_commits: self.check_every_commits,
            cooldown_commits: self.cooldown_commits,
        }
    }
}

/// Auto-compaction state metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionState {
    /// Last compacted commit sequence.
    pub last_compacted_commit_seq: u64,
    /// Last compaction timestamp in Unix milliseconds.
    pub last_compacted_at_unix_ms: u64,
}

/// Reform target layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReformTargetLayout {
    /// Preserve the source layout family.
    PreserveFamily,
    /// Reform to WholeAppendUnit.
    WholeAppendUnit,
    /// Reform to RegularChunked.
    RegularChunked,
}

impl ReformTargetLayout {
    fn to_raw(self) -> sys::ArcadiaTioReformTargetLayout {
        match self {
            Self::PreserveFamily => sys::ARCADIA_TIO_REFORM_TARGET_PRESERVE_FAMILY,
            Self::WholeAppendUnit => sys::ARCADIA_TIO_REFORM_TARGET_WHOLE_APPEND_UNIT,
            Self::RegularChunked => sys::ARCADIA_TIO_REFORM_TARGET_REGULAR_CHUNKED,
        }
    }
}

/// Safe reform policy/options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformOptions {
    /// Target layout family.
    pub target_layout: ReformTargetLayout,
    /// RegularChunked block shape used when target_layout is RegularChunked.
    pub regular_chunked_block_shape: Vec<u32>,
}

impl ReformOptions {
    /// Builds options that preserve the source layout family.
    pub fn preserve_family() -> Self {
        Self {
            target_layout: ReformTargetLayout::PreserveFamily,
            regular_chunked_block_shape: Vec::new(),
        }
    }

    /// Builds options targeting WholeAppendUnit.
    pub fn whole_append_unit() -> Self {
        Self {
            target_layout: ReformTargetLayout::WholeAppendUnit,
            regular_chunked_block_shape: Vec::new(),
        }
    }

    /// Builds options targeting RegularChunked with a native block shape.
    pub fn regular_chunked(block_shape: Vec<u32>) -> Self {
        Self {
            target_layout: ReformTargetLayout::RegularChunked,
            regular_chunked_block_shape: block_shape,
        }
    }

    fn to_raw(&self) -> sys::ArcadiaTioReformOptions {
        let block_shape_ptr = if self.regular_chunked_block_shape.is_empty() {
            ptr::null()
        } else {
            self.regular_chunked_block_shape.as_ptr()
        };
        sys::ArcadiaTioReformOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReformOptions>(),
            target_layout: self.target_layout.to_raw(),
            regular_chunked_block_shape: block_shape_ptr,
            regular_chunked_block_shape_len: self.regular_chunked_block_shape.len(),
        }
    }
}

/// Native reform diagnostic report copied into owned Rust strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReformReport {
    /// Stable reason code if reported.
    pub reason_code: Option<String>,
    /// Reason-code taxonomy if reported.
    pub reason_code_taxonomy: Option<String>,
    /// Human-readable reason if reported.
    pub reason: Option<String>,
}

/// Retained-history compaction policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V4RetainedHistoryPolicy {
    /// Retain the last N visible commits.
    RetainLast,
}

impl V4RetainedHistoryPolicy {
    fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryPolicy {
        match self {
            Self::RetainLast => sys::ARCADIA_TIO_V4_RETAINED_HISTORY_RETAIN_LAST,
        }
    }
}

/// Retained-history compaction options.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionOptions {
    /// Retained-history policy.
    pub policy: V4RetainedHistoryPolicy,
    /// Number of latest commits to retain for retain-last.
    pub retain_last_n: u32,
}

impl V4RetainedHistoryCompactionOptions {
    /// Builds retain-last retained-history compaction options.
    pub fn retain_last(retain_last_n: u32) -> Self {
        Self {
            policy: V4RetainedHistoryPolicy::RetainLast,
            retain_last_n,
        }
    }

    fn to_raw(self) -> sys::ArcadiaTioV4RetainedHistoryCompactionOptions {
        sys::ArcadiaTioV4RetainedHistoryCompactionOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionOptions>(),
            policy: self.policy.to_raw(),
            retain_last_n: self.retain_last_n,
        }
    }
}

impl Default for V4RetainedHistoryCompactionOptions {
    fn default() -> Self {
        Self::retain_last(1)
    }
}

/// Non-precise retained-history compaction report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Number of retained commits.
    pub retained_commit_count: u32,
    /// Retained commit sequence numbers.
    pub retained_commit_seqs: Vec<u64>,
    /// Optional count of older commits not retained.
    pub unretained_older_commit_count: Option<u64>,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Whether precise unreachable-byte details were intentionally omitted.
    pub omitted_unreachable_bytes: bool,
    /// Optional omission reason.
    pub omitted_unreachable_bytes_reason: Option<String>,
}

/// Precise retained-history compaction report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct V4RetainedHistoryCompactionPreciseReport {
    /// Report status.
    pub status: V4ReportStatus,
    /// Optional native status reason.
    pub reason: Option<String>,
    /// Number of retained commits.
    pub retained_commit_count: u32,
    /// Retained commit sequence numbers.
    pub retained_commit_seqs: Vec<u64>,
    /// Optional count of older commits not retained.
    pub unretained_older_commit_count: Option<u64>,
    /// Source file size in bytes.
    pub source_file_bytes: u64,
    /// Destination file size in bytes.
    pub destination_file_bytes: u64,
    /// Source-file precise accounting at retained-history compaction time.
    pub precise_source_accounting: V4PreciseAccountingBytes,
    /// Optional stable status/reason code.
    pub reason_code: Option<String>,
}

/// Create-time storage/layout profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateLayout {
    /// Streaming V4 create path.
    Streaming,
    /// Random-access V4 create path.
    RandomAccess,
}

/// Storage profile selector for RegularChunked policy create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageProfile {
    /// Balanced default profile.
    Balanced,
    /// NVMe-oriented profile.
    Nvme,
    /// HDD-oriented profile.
    Hdd,
}

impl StorageProfile {
    fn to_raw(self) -> sys::ArcadiaTioStorageProfile {
        match self {
            Self::Balanced => sys::ARCADIA_TIO_STORAGE_BALANCED,
            Self::Nvme => sys::ARCADIA_TIO_STORAGE_NVME,
            Self::Hdd => sys::ARCADIA_TIO_STORAGE_HDD,
        }
    }
}

/// Storage access hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageAccessKind {
    /// Seekable mounted storage.
    SeekableMounted,
    /// Remote storage with range-read capability.
    RemoteRangeRead,
    /// Forward-only storage.
    ForwardOnly,
}

impl StorageAccessKind {
    fn to_raw(self) -> sys::ArcadiaTioStorageAccessKind {
        match self {
            Self::SeekableMounted => sys::ARCADIA_TIO_STORAGE_ACCESS_SEEKABLE_MOUNTED,
            Self::RemoteRangeRead => sys::ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ,
            Self::ForwardOnly => sys::ARCADIA_TIO_STORAGE_ACCESS_FORWARD_ONLY,
        }
    }
}

/// Expected open/query pattern hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPattern {
    /// Metadata-hot open pattern.
    MetadataHot,
    /// Data-hot open pattern.
    DataHot,
    /// Mixed metadata/data open pattern.
    Mixed,
}

impl OpenPattern {
    fn to_raw(self) -> sys::ArcadiaTioOpenPattern {
        match self {
            Self::MetadataHot => sys::ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT,
            Self::DataHot => sys::ARCADIA_TIO_OPEN_PATTERN_DATA_HOT,
            Self::Mixed => sys::ARCADIA_TIO_OPEN_PATTERN_MIXED,
        }
    }
}

/// File population hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilePopulation {
    /// Few long-lived files.
    FewLongLived,
    /// Many shard files.
    ManyShards,
}

impl FilePopulation {
    fn to_raw(self) -> sys::ArcadiaTioFilePopulation {
        match self {
            Self::FewLongLived => sys::ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED,
            Self::ManyShards => sys::ARCADIA_TIO_FILE_POPULATION_MANY_SHARDS,
        }
    }
}

/// Metadata stability hint for inferred create helpers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataStability {
    /// Metadata is expected to remain stable.
    Stable,
    /// Metadata is expected to grow.
    Growing,
}

impl MetadataStability {
    fn to_raw(self) -> sys::ArcadiaTioMetadataStability {
        match self {
            Self::Stable => sys::ARCADIA_TIO_METADATA_STABILITY_STABLE,
            Self::Growing => sys::ARCADIA_TIO_METADATA_STABILITY_GROWING,
        }
    }
}

/// Policy options for RegularChunked create helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePolicyOptions {
    /// Non-append axes that should be chunked.
    pub chunk_axes: Vec<usize>,
    /// Storage profile used by the native policy planner.
    pub storage_profile: StorageProfile,
    /// Typical query sizes, one per rank axis. Use 0 for unspecified axes.
    pub typical_query_sizes: Vec<u32>,
}

impl CreatePolicyOptions {
    /// Creates RegularChunked policy options with a balanced storage profile.
    pub fn new(chunk_axes: Vec<usize>, typical_query_sizes: Vec<u32>) -> Self {
        Self {
            chunk_axes,
            storage_profile: StorageProfile::Balanced,
            typical_query_sizes,
        }
    }
}

/// Inferred layout-family create hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateInferredOptions {
    /// Storage access kind.
    pub storage_access: StorageAccessKind,
    /// Expected open pattern.
    pub open_pattern: OpenPattern,
    /// File population hint.
    pub file_population: FilePopulation,
    /// Metadata stability hint.
    pub metadata_stability: MetadataStability,
}

impl CreateInferredOptions {
    /// Conservative default inferred-create hints.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for CreateInferredOptions {
    fn default() -> Self {
        Self {
            storage_access: StorageAccessKind::SeekableMounted,
            open_pattern: OpenPattern::MetadataHot,
            file_population: FilePopulation::FewLongLived,
            metadata_stability: MetadataStability::Stable,
        }
    }
}

/// Owned create options for the first wrapper slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateOptions {
    /// Payload dtype.
    pub dtype: DType,
    /// Dimension descriptors.
    pub dims: Vec<DimSpec>,
    /// Append dimension index.
    pub append_dim: usize,
    /// Create layout/profile.
    pub layout: CreateLayout,
    /// Symbol labels.
    pub symbols: Vec<String>,
    /// Channel labels.
    pub channels: Vec<String>,
    /// User metadata key/value pairs.
    pub user_kv: Vec<(String, String)>,
    /// Optional coordinate descriptors.
    pub coordinates: Vec<CoordinateSpec>,
    /// Optional write-time compression policy override for future appends.
    ///
    /// `None` leaves the native persisted default in place (currently Auto/Zstd).
    /// Use `Some(CompressionConfig::uncompressed())` or
    /// `Some(CompressionConfig::zstd_level(...))` only when the caller needs an
    /// explicit override.
    pub compression: Option<CompressionConfig>,
}

impl CreateOptions {
    /// Builds streaming create options.
    pub fn streaming(dtype: DType, dims: Vec<DimSpec>, append_dim: usize) -> Self {
        Self {
            dtype,
            dims,
            append_dim,
            layout: CreateLayout::Streaming,
            symbols: Vec::new(),
            channels: Vec::new(),
            user_kv: Vec::new(),
            coordinates: Vec::new(),
            compression: None,
        }
    }

    /// Builds random-access create options.
    pub fn random_access(dtype: DType, dims: Vec<DimSpec>, append_dim: usize) -> Self {
        Self {
            layout: CreateLayout::RandomAccess,
            ..Self::streaming(dtype, dims, append_dim)
        }
    }
}

/// Write-time compression mode for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionMode {
    /// Force uncompressed writes.
    ForceOff,
    /// Let the native writer choose according to the configured codec and threshold.
    Auto,
    /// Force the configured codec for future writes.
    ForceOn,
}

impl CompressionMode {
    /// Converts this safe mode to the raw C ABI mode value.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionMode {
        match self {
            Self::ForceOff => sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF,
            Self::Auto => sys::ARCADIA_TIO_COMPRESSION_AUTO,
            Self::ForceOn => sys::ARCADIA_TIO_COMPRESSION_FORCE_ON,
        }
    }

    /// Converts a raw C ABI mode value into a safe mode.
    pub fn from_raw(value: sys::ArcadiaTioCompressionMode) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COMPRESSION_FORCE_OFF => Ok(Self::ForceOff),
            sys::ARCADIA_TIO_COMPRESSION_AUTO => Ok(Self::Auto),
            sys::ARCADIA_TIO_COMPRESSION_FORCE_ON => Ok(Self::ForceOn),
            other => Err(TioError::invalid_argument(format!(
                "unknown compression mode {other}"
            ))),
        }
    }
}

/// Write-time compression codec for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionCodec {
    /// Zstandard payload compression.
    Zstd,
}

impl CompressionCodec {
    /// Converts this safe codec to the raw C ABI codec value.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionCodec {
        match self {
            Self::Zstd => sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD,
        }
    }

    /// Converts a raw C ABI codec value into a safe codec.
    pub fn from_raw(value: sys::ArcadiaTioCompressionCodec) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COMPRESSION_CODEC_ZSTD => Ok(Self::Zstd),
            sys::ARCADIA_TIO_COMPRESSION_CODEC_LZ4 => Err(TioError::unimplemented(
                "LZ4 V4 payload compression is not supported yet",
            )),
            other => Err(TioError::invalid_argument(format!(
                "unknown compression codec {other}"
            ))),
        }
    }
}

/// Write-time compression policy for future appends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompressionConfig {
    /// Native compression mode.
    ///
    /// Prefer [`CompressionMode`] builders/accessors for ordinary code. This raw
    /// field remains public as a low-level compatibility escape hatch.
    pub mode: sys::ArcadiaTioCompressionMode,
    /// Native compression codec.
    ///
    /// Prefer [`CompressionCodec`] builders/accessors for ordinary code. This
    /// raw field remains public as a low-level compatibility escape hatch.
    pub codec: sys::ArcadiaTioCompressionCodec,
    /// Auto-mode minimum raw payload bytes.
    pub min_payload_bytes: u32,
    /// Zstd level.
    pub zstd_level: i32,
}

impl CompressionConfig {
    /// Minimum accepted zstd level.
    pub const ZSTD_MIN_LEVEL: i32 = -7;
    /// Maximum accepted zstd level.
    pub const ZSTD_MAX_LEVEL: i32 = 22;
    /// Default zstd level used by wrapper constructors.
    pub const DEFAULT_ZSTD_LEVEL: i32 = 3;
    /// Native default Auto/Zstd minimum raw payload threshold in bytes.
    pub const DEFAULT_MIN_PAYLOAD_BYTES: u32 = 256;

    /// Explicit uncompressed writes.
    pub fn uncompressed() -> Self {
        Self {
            mode: CompressionMode::ForceOff.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes: 0,
            zstd_level: Self::DEFAULT_ZSTD_LEVEL,
        }
    }

    /// Native Auto/Zstd writes with the native/default threshold.
    pub fn auto_zstd() -> Self {
        Self::auto_zstd_min_payload(Self::DEFAULT_MIN_PAYLOAD_BYTES)
    }

    /// Native Auto/Zstd writes with an explicit minimum raw payload threshold.
    pub fn auto_zstd_min_payload(min_payload_bytes: u32) -> Self {
        Self {
            mode: CompressionMode::Auto.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes,
            zstd_level: Self::DEFAULT_ZSTD_LEVEL,
        }
    }

    /// Explicit zstd writes at the requested level.
    ///
    /// This constructor preserves the historical source-compatible behavior of
    /// returning a config directly; call [`Self::try_zstd_level`] when the level
    /// should be checked before the config reaches a file operation.
    pub fn zstd_level(level: i32) -> Self {
        Self {
            mode: CompressionMode::ForceOn.to_raw(),
            codec: CompressionCodec::Zstd.to_raw(),
            min_payload_bytes: 0,
            zstd_level: level,
        }
    }

    /// Explicit zstd writes with early level validation.
    pub fn try_zstd_level(level: i32) -> Result<Self> {
        Self::zstd_level(level).validate()
    }

    /// Returns this config with a safe compression mode.
    pub fn with_mode(mut self, mode: CompressionMode) -> Self {
        self.mode = mode.to_raw();
        self
    }

    /// Returns this config with a safe compression codec.
    pub fn with_codec(mut self, codec: CompressionCodec) -> Self {
        self.codec = codec.to_raw();
        self
    }

    /// Returns this config with an Auto-mode payload threshold.
    pub fn with_min_payload_bytes(mut self, min_payload_bytes: u32) -> Self {
        self.min_payload_bytes = min_payload_bytes;
        self
    }

    /// Returns this config with a zstd level without changing historical late-validation behavior.
    pub fn with_zstd_level(mut self, level: i32) -> Self {
        self.zstd_level = level;
        self
    }

    /// Returns this config with a zstd level, validating the resulting policy immediately.
    pub fn try_with_zstd_level(self, level: i32) -> Result<Self> {
        self.with_zstd_level(level).validate()
    }

    /// Returns the safe compression mode represented by the raw field.
    pub fn mode(&self) -> Result<CompressionMode> {
        CompressionMode::from_raw(self.mode)
    }

    /// Returns the safe compression codec represented by the raw field.
    pub fn codec(&self) -> Result<CompressionCodec> {
        CompressionCodec::from_raw(self.codec)
    }

    /// Validates raw compatibility fields before a native call.
    pub fn validate(self) -> Result<Self> {
        CompressionMode::from_raw(self.mode)?;
        CompressionCodec::from_raw(self.codec)?;
        if !(Self::ZSTD_MIN_LEVEL..=Self::ZSTD_MAX_LEVEL).contains(&self.zstd_level) {
            return Err(TioError::invalid_argument(format!(
                "zstd_level must be within [{}, {}]",
                Self::ZSTD_MIN_LEVEL,
                Self::ZSTD_MAX_LEVEL
            )));
        }
        Ok(self)
    }

    /// Converts this policy to the raw C ABI config without validating raw compatibility fields.
    pub fn to_raw(self) -> sys::ArcadiaTioCompressionConfig {
        sys::ArcadiaTioCompressionConfig {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioCompressionConfig>(),
            mode: self.mode,
            codec: self.codec,
            min_payload_bytes: self.min_payload_bytes,
            zstd_level: self.zstd_level,
        }
    }

    /// Validates and converts this policy to the raw C ABI config.
    pub fn try_to_raw(self) -> Result<sys::ArcadiaTioCompressionConfig> {
        Ok(self.validate()?.to_raw())
    }

    /// Converts a raw C ABI compression config into a validated wrapper config.
    pub fn from_raw(raw: sys::ArcadiaTioCompressionConfig) -> Result<Self> {
        if raw.version != 1 {
            return Err(TioError::invalid_argument(format!(
                "unsupported compression config version {}",
                raw.version
            )));
        }
        let expected_size = mem::size_of::<sys::ArcadiaTioCompressionConfig>();
        if raw.struct_size != expected_size {
            return Err(TioError::invalid_argument(format!(
                "compression config struct_size must be {expected_size}"
            )));
        }
        Self {
            mode: raw.mode,
            codec: raw.codec,
            min_payload_bytes: raw.min_payload_bytes,
            zstd_level: raw.zstd_level,
        }
        .validate()
    }
}

impl From<CompressionMode> for sys::ArcadiaTioCompressionMode {
    fn from(value: CompressionMode) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionMode> for CompressionMode {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionMode) -> Result<Self> {
        Self::from_raw(value)
    }
}

impl From<CompressionCodec> for sys::ArcadiaTioCompressionCodec {
    fn from(value: CompressionCodec) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionCodec> for CompressionCodec {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionCodec) -> Result<Self> {
        Self::from_raw(value)
    }
}

impl From<CompressionConfig> for sys::ArcadiaTioCompressionConfig {
    fn from(value: CompressionConfig) -> Self {
        value.to_raw()
    }
}

impl TryFrom<sys::ArcadiaTioCompressionConfig> for CompressionConfig {
    type Error = TioError;

    fn try_from(value: sys::ArcadiaTioCompressionConfig) -> Result<Self> {
        Self::from_raw(value)
    }
}

/// Coordinate dtype supported by native coordinate metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateDType {
    /// 32-bit signed integer coordinates.
    I32,
    /// 64-bit signed integer coordinates.
    I64,
}

impl CoordinateDType {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateDType {
        match self {
            Self::I32 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I64,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateDType) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_DTYPE_I32 => Ok(Self::I32),
            sys::ARCADIA_TIO_COORDINATE_DTYPE_I64 => Ok(Self::I64),
            other => Err(TioError::conversion(format!(
                "unknown coordinate dtype value {other}"
            ))),
        }
    }
}

/// Coordinate semantic kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateKind {
    /// Ordinal/position coordinate.
    Position,
    /// Numeric label id coordinate.
    LabelId,
    /// Date coordinate.
    Date,
    /// Timestamp coordinate.
    Timestamp,
    /// Domain-specific numeric value.
    DomainValue,
}

impl CoordinateKind {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateKind {
        match self {
            Self::Position => sys::ARCADIA_TIO_COORDINATE_KIND_POSITION,
            Self::LabelId => sys::ARCADIA_TIO_COORDINATE_KIND_LABEL_ID,
            Self::Date => sys::ARCADIA_TIO_COORDINATE_KIND_DATE,
            Self::Timestamp => sys::ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP,
            Self::DomainValue => sys::ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_KIND_POSITION => Ok(Self::Position),
            sys::ARCADIA_TIO_COORDINATE_KIND_LABEL_ID => Ok(Self::LabelId),
            sys::ARCADIA_TIO_COORDINATE_KIND_DATE => Ok(Self::Date),
            sys::ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP => Ok(Self::Timestamp),
            sys::ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE => Ok(Self::DomainValue),
            other => Err(TioError::conversion(format!(
                "unknown coordinate kind value {other}"
            ))),
        }
    }
}

/// Integer coordinate encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateEncoding {
    /// Plain integer coordinate values.
    Plain,
    /// Days since an agreed epoch.
    DateDays,
    /// YYYYMMDD encoded date integer.
    DateYyyymmdd,
    /// Unix epoch seconds.
    EpochSeconds,
    /// Unix epoch milliseconds.
    EpochMilliseconds,
    /// Unix epoch microseconds.
    EpochMicroseconds,
    /// Unix epoch nanoseconds.
    EpochNanoseconds,
}

impl CoordinateEncoding {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateEncoding {
        match self {
            Self::Plain => sys::ARCADIA_TIO_COORDINATE_ENCODING_PLAIN,
            Self::DateDays => sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_DAYS,
            Self::DateYyyymmdd => sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD,
            Self::EpochSeconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_SECONDS,
            Self::EpochMilliseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MILLISECONDS,
            Self::EpochMicroseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MICROSECONDS,
            Self::EpochNanoseconds => sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_NANOSECONDS,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateEncoding) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_ENCODING_PLAIN => Ok(Self::Plain),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_DAYS => Ok(Self::DateDays),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD => Ok(Self::DateYyyymmdd),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_SECONDS => Ok(Self::EpochSeconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MILLISECONDS => Ok(Self::EpochMilliseconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_MICROSECONDS => Ok(Self::EpochMicroseconds),
            sys::ARCADIA_TIO_COORDINATE_ENCODING_EPOCH_NANOSECONDS => Ok(Self::EpochNanoseconds),
            other => Err(TioError::conversion(format!(
                "unknown coordinate encoding value {other}"
            ))),
        }
    }
}

/// Coordinate sortedness hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSortedness {
    /// Sortedness not declared.
    Unknown,
    /// Values are ascending.
    Ascending,
    /// Values are descending.
    Descending,
    /// Values are unsorted.
    Unsorted,
}

impl CoordinateSortedness {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateSortedness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN,
            Self::Ascending => sys::ARCADIA_TIO_COORDINATE_SORTED_ASCENDING,
            Self::Descending => sys::ARCADIA_TIO_COORDINATE_SORTED_DESCENDING,
            Self::Unsorted => sys::ARCADIA_TIO_COORDINATE_SORTED_UNSORTED,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateSortedness) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_SORTED_ASCENDING => Ok(Self::Ascending),
            sys::ARCADIA_TIO_COORDINATE_SORTED_DESCENDING => Ok(Self::Descending),
            sys::ARCADIA_TIO_COORDINATE_SORTED_UNSORTED => Ok(Self::Unsorted),
            other => Err(TioError::conversion(format!(
                "unknown coordinate sortedness value {other}"
            ))),
        }
    }
}

/// Coordinate monotonicity hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateMonotonicity {
    /// Monotonicity not declared.
    Unknown,
    /// Values are non-decreasing.
    NonDecreasing,
    /// Values are strictly increasing.
    StrictlyIncreasing,
    /// Values are non-increasing.
    NonIncreasing,
    /// Values are strictly decreasing.
    StrictlyDecreasing,
    /// Values are not monotonic.
    NotMonotonic,
}

impl CoordinateMonotonicity {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateMonotonicity {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_UNKNOWN,
            Self::NonDecreasing => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_DECREASING,
            Self::StrictlyIncreasing => {
                sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_INCREASING
            }
            Self::NonIncreasing => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_INCREASING,
            Self::StrictlyDecreasing => {
                sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_DECREASING
            }
            Self::NotMonotonic => sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NOT_MONOTONIC,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateMonotonicity) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_DECREASING => Ok(Self::NonDecreasing),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_INCREASING => {
                Ok(Self::StrictlyIncreasing)
            }
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NON_INCREASING => Ok(Self::NonIncreasing),
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_STRICTLY_DECREASING => {
                Ok(Self::StrictlyDecreasing)
            }
            sys::ARCADIA_TIO_COORDINATE_MONOTONICITY_NOT_MONOTONIC => Ok(Self::NotMonotonic),
            other => Err(TioError::conversion(format!(
                "unknown coordinate monotonicity value {other}"
            ))),
        }
    }
}

/// Coordinate uniqueness hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateUniqueness {
    /// Uniqueness not declared.
    Unknown,
    /// Values are unique.
    Unique,
    /// Values have duplicates.
    HasDuplicates,
}

impl CoordinateUniqueness {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateUniqueness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN,
            Self::Unique => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE,
            Self::HasDuplicates => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateUniqueness) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE => Ok(Self::Unique),
            sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES => Ok(Self::HasDuplicates),
            other => Err(TioError::conversion(format!(
                "unknown coordinate uniqueness value {other}"
            ))),
        }
    }
}

/// Coordinate storage location kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateStorageKind {
    /// Inline coordinate values stored in the TIO file.
    Inline,
    /// External coordinates referenced by descriptor metadata only.
    External,
}

impl CoordinateStorageKind {
    fn from_raw(value: sys::ArcadiaTioCoordinateStorageKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_STORAGE_INLINE => Ok(Self::Inline),
            sys::ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL => Ok(Self::External),
            other => Err(TioError::conversion(format!(
                "unknown coordinate storage kind value {other}"
            ))),
        }
    }
}

/// External coordinate source kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalCoordinateSourceKind {
    /// Same-file object reference.
    SameFileObject,
    /// Relative path reference.
    RelativePath,
    /// Absolute path reference.
    AbsolutePath,
    /// URI reference.
    Uri,
}

impl ExternalCoordinateSourceKind {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateSourceKind {
        match self {
            Self::SameFileObject => sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            Self::RelativePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH,
            Self::AbsolutePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH,
            Self::Uri => sys::ARCADIA_TIO_COORDINATE_SOURCE_URI,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateSourceKind) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT => Ok(Self::SameFileObject),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH => Ok(Self::RelativePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH => Ok(Self::AbsolutePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_URI => Ok(Self::Uri),
            other => Err(TioError::conversion(format!(
                "unknown coordinate source kind value {other}"
            ))),
        }
    }
}

/// Coordinate validation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateValidationStatus {
    /// Coordinate values are validated.
    Validated,
    /// Coordinate values are not validated or externally referenced.
    Unvalidated,
}

impl CoordinateValidationStatus {
    fn from_raw(value: sys::ArcadiaTioCoordinateValidationStatus) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_VALIDATED => Ok(Self::Validated),
            sys::ARCADIA_TIO_COORDINATE_UNVALIDATED => Ok(Self::Unvalidated),
            other => Err(TioError::conversion(format!(
                "unknown coordinate validation status value {other}"
            ))),
        }
    }
}

/// Coordinate ordering hints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinateOrdering {
    /// Sortedness hint.
    pub sorted: CoordinateSortedness,
    /// Monotonicity hint.
    pub monotonicity: CoordinateMonotonicity,
    /// Uniqueness hint.
    pub uniqueness: CoordinateUniqueness,
}

impl Default for CoordinateOrdering {
    fn default() -> Self {
        Self {
            sorted: CoordinateSortedness::Unknown,
            monotonicity: CoordinateMonotonicity::Unknown,
            uniqueness: CoordinateUniqueness::Unknown,
        }
    }
}

/// Owned inline coordinate values accepted by create metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateValues {
    /// i32 coordinate values.
    I32(Vec<i32>),
    /// i64 coordinate values.
    I64(Vec<i64>),
}

impl CoordinateValues {
    fn dtype(&self) -> CoordinateDType {
        match self {
            Self::I32(_) => CoordinateDType::I32,
            Self::I64(_) => CoordinateDType::I64,
        }
    }

    fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    fn as_ptr(&self) -> *const c_void {
        match self {
            Self::I32(values) => values.as_ptr().cast(),
            Self::I64(values) => values.as_ptr().cast(),
        }
    }
}

/// Coordinate storage descriptor accepted at create time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateStorage {
    /// Inline coordinate values. The values are borrowed only for the create call.
    Inline(CoordinateValues),
    /// External coordinate descriptor. External values are not resolved by this wrapper slice.
    External {
        /// External source kind.
        source_kind: ExternalCoordinateSourceKind,
        /// External URI/path.
        uri: String,
        /// External coordinate dtype.
        dtype: CoordinateDType,
        /// External coordinate length.
        length: u64,
    },
}

/// Coordinate descriptor accepted by create APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateSpec {
    /// Axis index.
    pub axis: usize,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate kind.
    pub kind: CoordinateKind,
    /// Coordinate encoding.
    pub encoding: CoordinateEncoding,
    /// Coordinate storage descriptor.
    pub storage: CoordinateStorage,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Whether the coordinate is required by consumers.
    pub required: bool,
}

/// Coordinate metadata snapshot copied from native-owned descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateMeta {
    /// Axis index.
    pub axis: usize,
    /// Optional axis name snapshot.
    pub axis_name_snapshot: Option<String>,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate kind.
    pub kind: CoordinateKind,
    /// Coordinate dtype.
    pub dtype: CoordinateDType,
    /// Coordinate encoding.
    pub encoding: CoordinateEncoding,
    /// Coordinate length.
    pub length: u64,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Storage kind.
    pub storage_kind: CoordinateStorageKind,
    /// External source kind.
    pub external_source_kind: ExternalCoordinateSourceKind,
    /// External URI when storage is external.
    pub external_uri: Option<String>,
    /// Whether this coordinate is required.
    pub required: bool,
    /// Validation status.
    pub validation_status: CoordinateValidationStatus,
}

/// Coordinate v2 value-domain selector for the public Rust source-only contract.
///
/// This first public Rust slice mirrors the raw C ABI domains that already exist in
/// `arcadia-tio-capi`. It does not add variable-length strings, locale/collation,
/// calendar interpretation, arbitrary external dereference, or authoritative index semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateValueDomainV2 {
    /// Inline numeric i32/i64 coordinate values.
    InlineNumeric,
    /// Fixed-width byte/text coordinate values.
    FixedText,
    /// Dictionary-code coordinate values bound to a dictionary revision.
    DictionaryCode,
    /// Append-axis sequence whose values arrive with payload appends.
    AppendSequence,
    /// External reference metadata only; this wrapper does not dereference it.
    ExternalReference,
}

impl CoordinateValueDomainV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateValueDomainV2 {
        match self {
            Self::InlineNumeric => sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_INLINE_NUMERIC,
            Self::FixedText => sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_FIXED_TEXT,
            Self::DictionaryCode => sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_DICTIONARY_CODE,
            Self::AppendSequence => sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_APPEND_SEQUENCE,
            Self::ExternalReference => {
                sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_EXTERNAL_REFERENCE
            }
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateValueDomainV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_INLINE_NUMERIC => Ok(Self::InlineNumeric),
            sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_FIXED_TEXT => Ok(Self::FixedText),
            sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_DICTIONARY_CODE => Ok(Self::DictionaryCode),
            sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_APPEND_SEQUENCE => Ok(Self::AppendSequence),
            sys::ARCADIA_TIO_COORDINATE_VALUE_DOMAIN_V2_EXTERNAL_REFERENCE => {
                Ok(Self::ExternalReference)
            }
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 value-domain value {other}"
            ))),
        }
    }
}

/// Coordinate v2 lookup-key domain selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateKeyDomainV2 {
    /// Signed 32-bit integer key.
    I32,
    /// Signed 64-bit integer key.
    I64,
    /// Fixed-width byte/text key.
    FixedText,
    /// Dictionary code key.
    DictionaryCode,
    /// Dictionary stable-id key.
    StableId,
    /// Dictionary display-label key.
    DisplayLabel,
    /// Dictionary alias key.
    Alias,
    /// Raw integer time key; broad calendar interpretation is deferred.
    RawTime,
}

impl CoordinateKeyDomainV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateKeyDomainV2 {
        match self {
            Self::I32 => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I32,
            Self::I64 => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I64,
            Self::FixedText => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_FIXED_TEXT,
            Self::DictionaryCode => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DICTIONARY_CODE,
            Self::StableId => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_STABLE_ID,
            Self::DisplayLabel => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DISPLAY_LABEL,
            Self::Alias => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_ALIAS,
            Self::RawTime => sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_RAW_TIME,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateKeyDomainV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I32 => Ok(Self::I32),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I64 => Ok(Self::I64),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_FIXED_TEXT => Ok(Self::FixedText),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DICTIONARY_CODE => Ok(Self::DictionaryCode),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_STABLE_ID => Ok(Self::StableId),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_DISPLAY_LABEL => Ok(Self::DisplayLabel),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_ALIAS => Ok(Self::Alias),
            sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_RAW_TIME => Ok(Self::RawTime),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 key-domain value {other}"
            ))),
        }
    }
}

/// Coordinate v2 dictionary-code integer dtype.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateCodeDTypeV2 {
    /// Unsigned 8-bit dictionary code.
    U8,
    /// Unsigned 16-bit dictionary code.
    U16,
    /// Unsigned 32-bit dictionary code.
    U32,
    /// Unsigned 64-bit dictionary code.
    U64,
}

impl CoordinateCodeDTypeV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateCodeDTypeV2 {
        match self {
            Self::U8 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U8,
            Self::U16 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U16,
            Self::U32 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U32,
            Self::U64 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U64,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateCodeDTypeV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U8 => Ok(Self::U8),
            sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U16 => Ok(Self::U16),
            sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U32 => Ok(Self::U32),
            sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U64 => Ok(Self::U64),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 code-dtype value {other}"
            ))),
        }
    }
}

/// Coordinate v2 fixed-text byte encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFixedTextEncodingV2 {
    /// ASCII bytes only.
    Ascii,
}

impl CoordinateFixedTextEncodingV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateFixedTextEncodingV2 {
        match self {
            Self::Ascii => sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_ENCODING_V2_ASCII,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateFixedTextEncodingV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_ENCODING_V2_ASCII => Ok(Self::Ascii),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 fixed-text encoding value {other}"
            ))),
        }
    }
}

/// Coordinate v2 fixed-text padding policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateFixedTextPaddingV2 {
    /// Right-pad with spaces.
    RightSpace,
}

impl CoordinateFixedTextPaddingV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateFixedTextPaddingV2 {
        match self {
            Self::RightSpace => sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_PADDING_V2_RIGHT_SPACE,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateFixedTextPaddingV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_PADDING_V2_RIGHT_SPACE => Ok(Self::RightSpace),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 fixed-text padding value {other}"
            ))),
        }
    }
}

/// Coordinate v2 external source kind.
///
/// These values are metadata only in this public Rust foundation. The wrapper does not resolve,
/// dereference, fetch, or authorize arbitrary paths, URIs, or application registries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateSourceKindV2 {
    /// Same-file object reference.
    SameFileObject,
    /// Relative path reference metadata.
    RelativePath,
    /// Absolute path reference metadata.
    AbsolutePath,
    /// URI reference metadata.
    Uri,
    /// Application-registry reference metadata.
    ApplicationRegistry,
}

impl CoordinateSourceKindV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateSourceKindV2 {
        match self {
            Self::SameFileObject => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_SAME_FILE_OBJECT,
            Self::RelativePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_RELATIVE_PATH,
            Self::AbsolutePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_ABSOLUTE_PATH,
            Self::Uri => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_URI,
            Self::ApplicationRegistry => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_APPLICATION_REGISTRY,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateSourceKindV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_SAME_FILE_OBJECT => Ok(Self::SameFileObject),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_RELATIVE_PATH => Ok(Self::RelativePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_ABSOLUTE_PATH => Ok(Self::AbsolutePath),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_URI => Ok(Self::Uri),
            sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_APPLICATION_REGISTRY => {
                Ok(Self::ApplicationRegistry)
            }
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 source-kind value {other}"
            ))),
        }
    }
}

/// Coordinate v2 availability status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateAvailabilityV2 {
    /// Coordinate values are available.
    Available,
    /// Coordinate is absent.
    Absent,
    /// Coordinate availability is unknown.
    Unknown,
    /// Coordinate binding is invalid.
    Invalid,
    /// Coordinate is unavailable.
    Unavailable,
    /// Coordinate domain or operation is unsupported.
    Unsupported,
}

impl CoordinateAvailabilityV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateAvailabilityV2 {
        match self {
            Self::Available => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_AVAILABLE,
            Self::Absent => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_ABSENT,
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNKNOWN,
            Self::Invalid => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_INVALID,
            Self::Unavailable => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNAVAILABLE,
            Self::Unsupported => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNSUPPORTED,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateAvailabilityV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_AVAILABLE => Ok(Self::Available),
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_ABSENT => Ok(Self::Absent),
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNKNOWN => Ok(Self::Unknown),
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_INVALID => Ok(Self::Invalid),
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNAVAILABLE => Ok(Self::Unavailable),
            sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNSUPPORTED => Ok(Self::Unsupported),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 availability value {other}"
            ))),
        }
    }
}

/// Coordinate v2 status category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateStatusCategoryV2 {
    /// Operation succeeded.
    Ok,
    /// Invalid argument.
    InvalidArgument,
    /// Unsupported coordinate domain.
    UnsupportedDomain,
    /// Unknown required version.
    UnknownRequiredVersion,
    /// Required coordinate is unavailable.
    RequiredUnavailable,
    /// External binding is stale.
    StaleExternalBinding,
    /// Lookup requested uniqueness but found duplicates.
    DuplicateUniqueLookup,
    /// Lookup key domain does not match coordinate domain.
    LookupDomainMismatch,
    /// Optional index is invalid.
    InvalidIndex,
    /// Optional index is stale.
    StaleIndex,
    /// Optional index kind is unsupported.
    UnsupportedIndex,
}

impl CoordinateStatusCategoryV2 {
    fn to_raw(self) -> sys::ArcadiaTioCoordinateStatusCategoryV2 {
        match self {
            Self::Ok => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_OK,
            Self::InvalidArgument => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_ARGUMENT,
            Self::UnsupportedDomain => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_DOMAIN,
            Self::UnknownRequiredVersion => {
                sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNKNOWN_REQUIRED_VERSION
            }
            Self::RequiredUnavailable => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_REQUIRED_UNAVAILABLE,
            Self::StaleExternalBinding => {
                sys::ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_EXTERNAL_BINDING
            }
            Self::DuplicateUniqueLookup => {
                sys::ARCADIA_TIO_COORDINATE_STATUS_V2_DUPLICATE_UNIQUE_LOOKUP
            }
            Self::LookupDomainMismatch => {
                sys::ARCADIA_TIO_COORDINATE_STATUS_V2_LOOKUP_DOMAIN_MISMATCH
            }
            Self::InvalidIndex => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_INDEX,
            Self::StaleIndex => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_INDEX,
            Self::UnsupportedIndex => sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_INDEX,
        }
    }

    fn from_raw(value: sys::ArcadiaTioCoordinateStatusCategoryV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_OK => Ok(Self::Ok),
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_ARGUMENT => Ok(Self::InvalidArgument),
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_DOMAIN => Ok(Self::UnsupportedDomain),
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNKNOWN_REQUIRED_VERSION => {
                Ok(Self::UnknownRequiredVersion)
            }
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_REQUIRED_UNAVAILABLE => {
                Ok(Self::RequiredUnavailable)
            }
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_EXTERNAL_BINDING => {
                Ok(Self::StaleExternalBinding)
            }
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_DUPLICATE_UNIQUE_LOOKUP => {
                Ok(Self::DuplicateUniqueLookup)
            }
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_LOOKUP_DOMAIN_MISMATCH => {
                Ok(Self::LookupDomainMismatch)
            }
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_INVALID_INDEX => Ok(Self::InvalidIndex),
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_STALE_INDEX => Ok(Self::StaleIndex),
            sys::ARCADIA_TIO_COORDINATE_STATUS_V2_UNSUPPORTED_INDEX => Ok(Self::UnsupportedIndex),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 status-category value {other}"
            ))),
        }
    }
}

/// Coordinate v2 optional index kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateIndexKindV2 {
    /// Exact lookup index.
    Exact,
    /// Range lookup index.
    Range,
    /// Dictionary-key lookup index.
    DictionaryKey,
}

impl CoordinateIndexKindV2 {
    fn from_raw(value: sys::ArcadiaTioCoordinateIndexKindV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_EXACT => Ok(Self::Exact),
            sys::ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_RANGE => Ok(Self::Range),
            sys::ARCADIA_TIO_COORDINATE_INDEX_KIND_V2_DICTIONARY_KEY => Ok(Self::DictionaryKey),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 index-kind value {other}"
            ))),
        }
    }
}

/// Coordinate v2 optional-index validation status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateIndexValidationStatusV2 {
    /// Index is validated for the selected root.
    Validated,
    /// Index is missing.
    Missing,
    /// Index is stale.
    Stale,
    /// Index is invalid.
    Invalid,
    /// Index is unsupported.
    Unsupported,
}

impl CoordinateIndexValidationStatusV2 {
    fn from_raw(value: sys::ArcadiaTioCoordinateIndexValidationStatusV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_VALIDATED => Ok(Self::Validated),
            sys::ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_MISSING => Ok(Self::Missing),
            sys::ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_STALE => Ok(Self::Stale),
            sys::ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_INVALID => Ok(Self::Invalid),
            sys::ARCADIA_TIO_COORDINATE_INDEX_STATUS_V2_UNSUPPORTED => Ok(Self::Unsupported),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 index-validation value {other}"
            ))),
        }
    }
}

/// Coordinate v2 optional-index fallback policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateIndexFallbackV2 {
    /// Fall back to authoritative coordinate scan.
    AuthoritativeScan,
    /// Rebuild the optional index.
    Rebuild,
    /// Reject operations that depend on an index.
    RejectIndexDependentOperation,
}

impl CoordinateIndexFallbackV2 {
    fn from_raw(value: sys::ArcadiaTioCoordinateIndexFallbackV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_AUTHORITATIVE_SCAN => {
                Ok(Self::AuthoritativeScan)
            }
            sys::ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_REBUILD => Ok(Self::Rebuild),
            sys::ARCADIA_TIO_COORDINATE_INDEX_FALLBACK_V2_REJECT_INDEX_DEPENDENT_OPERATION => {
                Ok(Self::RejectIndexDependentOperation)
            }
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 index-fallback value {other}"
            ))),
        }
    }
}

/// Coordinate v2 optional-index selected use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateIndexUseV2 {
    /// Use optional index.
    UseIndex,
    /// Authoritative coordinate scan is selected.
    AuthoritativeScan,
    /// Rebuild is selected.
    Rebuild,
    /// Index is unavailable.
    Unavailable,
}

impl CoordinateIndexUseV2 {
    fn from_raw(value: sys::ArcadiaTioCoordinateIndexUseV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_INDEX_USE_V2_USE_INDEX => Ok(Self::UseIndex),
            sys::ARCADIA_TIO_COORDINATE_INDEX_USE_V2_AUTHORITATIVE_SCAN => {
                Ok(Self::AuthoritativeScan)
            }
            sys::ARCADIA_TIO_COORDINATE_INDEX_USE_V2_REBUILD => Ok(Self::Rebuild),
            sys::ARCADIA_TIO_COORDINATE_INDEX_USE_V2_UNAVAILABLE => Ok(Self::Unavailable),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 index-use value {other}"
            ))),
        }
    }
}

/// Coordinate v2 lookup-result status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateLookupResultStatusV2 {
    /// Unique position result.
    Unique,
    /// Half-open range result.
    Range,
    /// Many positions result.
    Many,
    /// Missing result.
    Missing,
    /// Coordinate is unavailable.
    Unavailable,
    /// Duplicate result for a unique lookup.
    Duplicate,
    /// Lookup is unsupported.
    Unsupported,
    /// Lookup failed.
    Error,
}

impl CoordinateLookupResultStatusV2 {
    fn from_raw(value: sys::ArcadiaTioCoordinateLookupResultStatusV2) -> Result<Self> {
        match value {
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNIQUE => Ok(Self::Unique),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_RANGE => Ok(Self::Range),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_MANY => Ok(Self::Many),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_MISSING => Ok(Self::Missing),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNAVAILABLE => Ok(Self::Unavailable),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_DUPLICATE => Ok(Self::Duplicate),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_UNSUPPORTED => Ok(Self::Unsupported),
            sys::ARCADIA_TIO_COORDINATE_LOOKUP_RESULT_V2_ERROR => Ok(Self::Error),
            other => Err(TioError::conversion(format!(
                "unknown Coordinate v2 lookup-result status value {other}"
            ))),
        }
    }
}

/// Coordinate v2 fixed-text layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoordinateFixedTextLayoutV2 {
    /// Fixed text width in bytes.
    pub width: usize,
    /// Fixed text byte encoding.
    pub encoding: CoordinateFixedTextEncodingV2,
    /// Fixed text padding policy.
    pub padding: CoordinateFixedTextPaddingV2,
    /// Reject values wider than `width`.
    pub reject_over_width: bool,
    /// Reject non-ASCII bytes.
    pub reject_non_ascii: bool,
}

impl Default for CoordinateFixedTextLayoutV2 {
    fn default() -> Self {
        Self {
            width: 0,
            encoding: CoordinateFixedTextEncodingV2::Ascii,
            padding: CoordinateFixedTextPaddingV2::RightSpace,
            reject_over_width: true,
            reject_non_ascii: true,
        }
    }
}

impl CoordinateFixedTextLayoutV2 {
    /// Builds the implemented fixed-width ASCII/right-space-padded layout.
    pub fn ascii_right_space_padded(width: usize) -> Result<Self> {
        if width == 0 {
            return Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text width must be > 0",
            ));
        }
        Ok(Self {
            width,
            encoding: CoordinateFixedTextEncodingV2::Ascii,
            padding: CoordinateFixedTextPaddingV2::RightSpace,
            reject_over_width: true,
            reject_non_ascii: true,
        })
    }

    /// Converts this safe layout to a raw C ABI layout with version, size, and reserved fields set.
    pub fn to_raw(self) -> sys::ArcadiaTioCoordinateFixedTextLayoutV2 {
        sys::ArcadiaTioCoordinateFixedTextLayoutV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioCoordinateFixedTextLayoutV2>(),
            width: self.width,
            encoding: self.encoding.to_raw(),
            padding: self.padding.to_raw(),
            reject_over_width: u8::from(self.reject_over_width),
            reject_non_ascii: u8::from(self.reject_non_ascii),
            reserved_u8: [0; 6],
            reserved: [0; 2],
        }
    }

    fn from_raw(raw: sys::ArcadiaTioCoordinateFixedTextLayoutV2) -> Result<Self> {
        Ok(Self {
            width: raw.width,
            encoding: CoordinateFixedTextEncodingV2::from_raw(raw.encoding)?,
            padding: CoordinateFixedTextPaddingV2::from_raw(raw.padding)?,
            reject_over_width: raw.reject_over_width != 0,
            reject_non_ascii: raw.reject_non_ascii != 0,
        })
    }
}

/// Coordinate v2 dictionary summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateDictionarySummaryV2 {
    /// Dictionary identifier.
    pub dictionary_id: Option<String>,
    /// Dictionary revision bound to the selected root.
    pub revision: u64,
    /// Dictionary code dtype.
    pub code_dtype: CoordinateCodeDTypeV2,
    /// Number of dictionary entries.
    pub entry_count: u64,
    /// Whether stable IDs are unique.
    pub stable_ids_unique: bool,
    /// Whether display labels are unique.
    pub display_labels_unique: bool,
    /// Whether aliases are unique.
    pub aliases_unique: bool,
    /// Whether codes remain stable across revisions.
    pub codes_stable_across_revisions: bool,
    /// Content identifier for the dictionary revision.
    pub content_id: Option<String>,
}

impl CoordinateDictionarySummaryV2 {
    /// Builds a dictionary summary for create-time Coordinate v2 descriptors.
    pub fn new(code_dtype: CoordinateCodeDTypeV2) -> Self {
        Self {
            dictionary_id: None,
            revision: 0,
            code_dtype,
            entry_count: 0,
            stable_ids_unique: true,
            display_labels_unique: true,
            aliases_unique: true,
            codes_stable_across_revisions: true,
            content_id: None,
        }
    }

    /// Sets the optional dictionary identifier.
    pub fn with_dictionary_id(mut self, dictionary_id: impl Into<String>) -> Self {
        self.dictionary_id = Some(dictionary_id.into());
        self
    }

    /// Sets the selected-root dictionary revision.
    pub fn with_revision(mut self, revision: u64) -> Self {
        self.revision = revision;
        self
    }

    /// Sets the optional dictionary content identifier.
    pub fn with_content_id(mut self, content_id: impl Into<String>) -> Self {
        self.content_id = Some(content_id.into());
        self
    }

    fn from_raw(raw: &sys::ArcadiaTioCoordinateDictionarySummaryV2) -> Result<Self> {
        Ok(Self {
            dictionary_id: optional_c_string(raw.dictionary_id),
            revision: raw.revision,
            code_dtype: CoordinateCodeDTypeV2::from_raw(raw.code_dtype)?,
            entry_count: raw.entry_count,
            stable_ids_unique: raw.stable_ids_unique != 0,
            display_labels_unique: raw.display_labels_unique != 0,
            aliases_unique: raw.aliases_unique != 0,
            codes_stable_across_revisions: raw.codes_stable_across_revisions != 0,
            content_id: optional_c_string(raw.content_id),
        })
    }

    fn prepare(&self) -> Result<PreparedCoordinateDictionarySummaryV2> {
        PreparedCoordinateDictionarySummaryV2::new(self)
    }
}

/// Coordinate v2 dictionary entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateDictionaryEntryV2 {
    /// Dictionary code value.
    pub code: u64,
    /// Stable identifier.
    pub stable_id: Option<String>,
    /// Display label.
    pub display_label: Option<String>,
    /// Alias labels.
    pub aliases: Vec<String>,
}

impl CoordinateDictionaryEntryV2 {
    /// Builds a dictionary entry with optional stable identifier and display label.
    pub fn new(
        code: u64,
        stable_id: impl Into<Option<String>>,
        display_label: impl Into<Option<String>>,
    ) -> Self {
        Self {
            code,
            stable_id: stable_id.into(),
            display_label: display_label.into(),
            aliases: Vec::new(),
        }
    }

    /// Sets alias labels for this dictionary entry.
    pub fn with_aliases<I, S>(mut self, aliases: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.aliases = aliases.into_iter().map(Into::into).collect();
        self
    }

    fn from_raw(raw: &sys::ArcadiaTioCoordinateDictionaryEntryV2) -> Self {
        let aliases = if raw.aliases.is_null() || raw.aliases_len == 0 {
            Vec::new()
        } else {
            // SAFETY: Native dictionary entry aliases are valid for `aliases_len` until the parent is freed.
            unsafe { slice::from_raw_parts(raw.aliases.cast_const(), raw.aliases_len) }
                .iter()
                .filter_map(|alias| optional_c_string((*alias).cast_const()))
                .collect()
        };
        Self {
            code: raw.code,
            stable_id: optional_c_string(raw.stable_id.cast_const()),
            display_label: optional_c_string(raw.display_label.cast_const()),
            aliases,
        }
    }
}

/// Coordinate v2 dictionary result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateDictionaryV2 {
    /// Dictionary summary.
    pub summary: CoordinateDictionarySummaryV2,
    /// Dictionary entries.
    pub entries: Vec<CoordinateDictionaryEntryV2>,
    /// Status category.
    pub status_category: CoordinateStatusCategoryV2,
    /// Status reason.
    pub reason: Option<String>,
}

impl CoordinateDictionaryV2 {
    /// Copies a raw dictionary result into safe Rust values.
    ///
    /// # Safety
    ///
    /// `raw.entries` and nested string pointers must be valid according to the C ABI until the
    /// caller releases the parent raw dictionary with the matching free function.
    pub unsafe fn from_raw_borrowed(raw: &sys::ArcadiaTioCoordinateDictionaryV2) -> Result<Self> {
        let entries = if raw.entries.is_null() || raw.entries_len == 0 {
            Vec::new()
        } else {
            // SAFETY: Caller guarantees the native entry array is valid for `entries_len`.
            unsafe { slice::from_raw_parts(raw.entries, raw.entries_len) }
                .iter()
                .map(CoordinateDictionaryEntryV2::from_raw)
                .collect()
        };
        Ok(Self {
            summary: CoordinateDictionarySummaryV2::from_raw(&raw.summary)?,
            entries,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            reason: optional_c_string(raw.reason.cast_const()),
        })
    }
}

/// Coordinate v2 external binding metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateExternalBindingV2 {
    /// External source kind.
    pub source_kind: CoordinateSourceKindV2,
    /// Logical identifier.
    pub logical_id: Option<String>,
    /// Privacy-safe display string.
    pub privacy_safe_display: Option<String>,
    /// Content identifier.
    pub content_id: Option<String>,
    /// Value domain carried externally.
    pub value_domain: CoordinateValueDomainV2,
    /// Declared coordinate length.
    pub length: u64,
    /// Availability status.
    pub availability: CoordinateAvailabilityV2,
    /// Status category.
    pub status_category: CoordinateStatusCategoryV2,
    /// Whether this binding is required.
    pub required: bool,
}

impl CoordinateExternalBindingV2 {
    /// Builds a descriptor-only external-reference summary. The wrapper does not dereference it.
    pub fn metadata_only(
        source_kind: CoordinateSourceKindV2,
        logical_id: impl Into<Option<String>>,
        privacy_safe_display: impl Into<Option<String>>,
        value_domain: CoordinateValueDomainV2,
        length: u64,
    ) -> Self {
        Self {
            source_kind,
            logical_id: logical_id.into(),
            privacy_safe_display: privacy_safe_display.into(),
            content_id: None,
            value_domain,
            length,
            availability: CoordinateAvailabilityV2::Unavailable,
            status_category: CoordinateStatusCategoryV2::Ok,
            required: false,
        }
    }

    /// Sets the optional external content identifier.
    pub fn with_content_id(mut self, content_id: impl Into<String>) -> Self {
        self.content_id = Some(content_id.into());
        self
    }

    /// Sets availability and status category for a descriptor-only external summary.
    pub fn with_status(
        mut self,
        availability: CoordinateAvailabilityV2,
        status_category: CoordinateStatusCategoryV2,
    ) -> Self {
        self.availability = availability;
        self.status_category = status_category;
        self
    }

    /// Marks the external binding required or optional.
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        self
    }

    fn from_raw(raw: &sys::ArcadiaTioCoordinateExternalBindingV2) -> Result<Self> {
        Ok(Self {
            source_kind: CoordinateSourceKindV2::from_raw(raw.source_kind)?,
            logical_id: optional_c_string(raw.logical_id),
            privacy_safe_display: optional_c_string(raw.privacy_safe_display),
            content_id: optional_c_string(raw.content_id),
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            length: raw.length,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            required: raw.required != 0,
        })
    }

    fn prepare(&self) -> Result<PreparedCoordinateExternalBindingV2> {
        PreparedCoordinateExternalBindingV2::new(self)
    }
}

/// Coordinate v2 selected-root source binding for optional index summaries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateIndexSourceBindingV2 {
    /// Descriptor identifier.
    pub descriptor_id: Option<String>,
    /// Descriptor revision.
    pub descriptor_revision: u64,
    /// Value domain.
    pub value_domain: CoordinateValueDomainV2,
    /// Value-object identifier.
    pub value_object_id: Option<String>,
    /// Dictionary identifier.
    pub dictionary_id: Option<String>,
    /// Dictionary revision.
    pub dictionary_revision: u64,
    /// Dictionary content identifier.
    pub dictionary_content_id: Option<String>,
    /// External source kind.
    pub external_source_kind: CoordinateSourceKindV2,
    /// External logical identifier.
    pub external_logical_id: Option<String>,
    /// External content identifier.
    pub external_content_id: Option<String>,
    /// Selected-root identifier.
    pub root_id: Option<String>,
    /// Axis index.
    pub axis: usize,
    /// Root extent.
    pub root_extent: u64,
    /// Append start.
    pub append_start: u64,
    /// Append count.
    pub append_count: u64,
}

impl CoordinateIndexSourceBindingV2 {
    fn from_raw(raw: &sys::ArcadiaTioCoordinateIndexSourceBindingV2) -> Result<Self> {
        Ok(Self {
            descriptor_id: optional_c_string(raw.descriptor_id),
            descriptor_revision: raw.descriptor_revision,
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            value_object_id: optional_c_string(raw.value_object_id),
            dictionary_id: optional_c_string(raw.dictionary_id),
            dictionary_revision: raw.dictionary_revision,
            dictionary_content_id: optional_c_string(raw.dictionary_content_id),
            external_source_kind: CoordinateSourceKindV2::from_raw(raw.external_source_kind)?,
            external_logical_id: optional_c_string(raw.external_logical_id),
            external_content_id: optional_c_string(raw.external_content_id),
            root_id: optional_c_string(raw.root_id),
            axis: raw.axis,
            root_extent: raw.root_extent,
            append_start: raw.append_start,
            append_count: raw.append_count,
        })
    }
}

/// Coordinate v2 optional index summary.
///
/// Optional indexes are descriptive acceleration metadata only. Public Rust v2 contract types keep
/// authoritative coordinate values/dictionaries/external bindings selected-root-bound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateIndexSummaryV2 {
    /// Index identifier.
    pub index_id: Option<String>,
    /// Index kind.
    pub index_kind: CoordinateIndexKindV2,
    /// Key domain covered by the index.
    pub key_domain: CoordinateKeyDomainV2,
    /// Source binding.
    pub source_binding: CoordinateIndexSourceBindingV2,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Index format version.
    pub format_version: u32,
    /// Index build version.
    pub build_version: u32,
    /// Validation status.
    pub validation_status: CoordinateIndexValidationStatusV2,
    /// Fallback policy.
    pub fallback: CoordinateIndexFallbackV2,
    /// Selected use.
    pub selected_use: CoordinateIndexUseV2,
    /// Whether the index is required.
    pub required: bool,
    /// Status reason.
    pub reason: Option<String>,
}

impl CoordinateIndexSummaryV2 {
    fn from_raw(raw: &sys::ArcadiaTioCoordinateIndexSummaryV2) -> Result<Self> {
        Ok(Self {
            index_id: optional_c_string(raw.index_id),
            index_kind: CoordinateIndexKindV2::from_raw(raw.index_kind)?,
            key_domain: CoordinateKeyDomainV2::from_raw(raw.key_domain)?,
            source_binding: CoordinateIndexSourceBindingV2::from_raw(&raw.source_binding)?,
            ordering: CoordinateOrdering {
                sorted: CoordinateSortedness::from_raw(raw.sorted)?,
                monotonicity: CoordinateMonotonicity::from_raw(raw.monotonicity)?,
                uniqueness: CoordinateUniqueness::from_raw(raw.uniqueness)?,
            },
            format_version: raw.format_version,
            build_version: raw.build_version,
            validation_status: CoordinateIndexValidationStatusV2::from_raw(raw.validation_status)?,
            fallback: CoordinateIndexFallbackV2::from_raw(raw.fallback)?,
            selected_use: CoordinateIndexUseV2::from_raw(raw.selected_use)?,
            required: raw.required != 0,
            reason: optional_c_string(raw.reason),
        })
    }
}

/// Coordinate v2 operation options.
///
/// Optional indexes are never coordinate truth. These options only choose whether lookup calls may
/// fall back to selected-root authoritative values/dictionaries when optional indexes are absent,
/// invalid, stale, or unsupported.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CoordinateV2Options {
    /// Allow selected-root authoritative scans when optional indexes are absent or unusable.
    pub allow_authoritative_scan: bool,
    /// Include dictionary entries in dictionary reads.
    pub include_dictionary_entries: bool,
    /// Include optional index summaries in metadata reads.
    pub include_index_summaries: bool,
    /// Allow external resolution where a future implementation explicitly supports it.
    pub allow_external_resolution: bool,
}

impl CoordinateV2Options {
    /// Returns options that allow explicit authoritative coordinate scans.
    pub fn authoritative_scan() -> Self {
        Self {
            allow_authoritative_scan: true,
            ..Self::default()
        }
    }

    /// Converts this safe option set to raw C ABI options with reserved fields zeroed.
    pub fn to_raw(self) -> sys::ArcadiaTioCoordinateV2Options {
        sys::ArcadiaTioCoordinateV2Options {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioCoordinateV2Options>(),
            allow_authoritative_scan: u8::from(self.allow_authoritative_scan),
            include_dictionary_entries: u8::from(self.include_dictionary_entries),
            include_index_summaries: u8::from(self.include_index_summaries),
            allow_external_resolution: u8::from(self.allow_external_resolution),
            reserved_u8: [0; 4],
            reserved: [0; 4],
        }
    }
}

/// Coordinate v2 owned/buffered input values for descriptor and append conversions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateInputValuesV2 {
    /// No immediate value buffer; used for append-sequence declarations and external references.
    None,
    /// Inline i32 numeric values.
    I32(Vec<i32>),
    /// Inline i64 numeric values.
    I64(Vec<i64>),
    /// Fixed-width text bytes, stored as `len * fixed_text.width` contiguous bytes.
    FixedText(Vec<u8>),
    /// Unsigned 8-bit dictionary codes.
    CodesU8(Vec<u8>),
    /// Unsigned 16-bit dictionary codes.
    CodesU16(Vec<u16>),
    /// Unsigned 32-bit dictionary codes.
    CodesU32(Vec<u32>),
    /// Unsigned 64-bit dictionary codes.
    CodesU64(Vec<u64>),
}

impl Default for CoordinateInputValuesV2 {
    fn default() -> Self {
        Self::None
    }
}

impl CoordinateInputValuesV2 {
    fn pointer_len_for_axis(&self, fixed_text_width: usize) -> Result<(*const c_void, usize)> {
        match self {
            Self::None => Ok((ptr::null(), 0)),
            Self::I32(values) => Ok(buffer_ptr_len(values)),
            Self::I64(values) => Ok(buffer_ptr_len(values)),
            Self::FixedText(bytes) => {
                let len = fixed_text_value_count(bytes.len(), fixed_text_width)?;
                Ok((buffer_ptr_for_count(bytes, len), len))
            }
            Self::CodesU8(values) => Ok(buffer_ptr_len(values)),
            Self::CodesU16(values) => Ok(buffer_ptr_len(values)),
            Self::CodesU32(values) => Ok(buffer_ptr_len(values)),
            Self::CodesU64(values) => Ok(buffer_ptr_len(values)),
        }
    }

    fn pointer_count_element_size(
        &self,
        fixed_text_width: usize,
    ) -> Result<(*const c_void, usize, usize)> {
        match self {
            Self::None => Ok((ptr::null(), 0, 0)),
            Self::I32(values) => Ok(buffer_ptr_count_element_size(values)),
            Self::I64(values) => Ok(buffer_ptr_count_element_size(values)),
            Self::FixedText(bytes) => {
                let count = fixed_text_value_count(bytes.len(), fixed_text_width)?;
                Ok((
                    buffer_ptr_for_count(bytes, count),
                    count,
                    mem::size_of::<u8>(),
                ))
            }
            Self::CodesU8(values) => Ok(buffer_ptr_count_element_size(values)),
            Self::CodesU16(values) => Ok(buffer_ptr_count_element_size(values)),
            Self::CodesU32(values) => Ok(buffer_ptr_count_element_size(values)),
            Self::CodesU64(values) => Ok(buffer_ptr_count_element_size(values)),
        }
    }
}

fn buffer_ptr_len<T>(values: &[T]) -> (*const c_void, usize) {
    (buffer_ptr_for_count(values, values.len()), values.len())
}

fn buffer_ptr_count_element_size<T>(values: &[T]) -> (*const c_void, usize, usize) {
    (
        buffer_ptr_for_count(values, values.len()),
        values.len(),
        mem::size_of::<T>(),
    )
}

fn buffer_ptr_for_count<T>(values: &[T], count: usize) -> *const c_void {
    if count == 0 {
        ptr::null()
    } else {
        values.as_ptr().cast()
    }
}

fn validate_fixed_text_lookup_key(bytes_len: usize, width: usize) -> Result<()> {
    if width == 0 {
        return Err(TioError::invalid_argument(
            "fixed-text Coordinate v2 lookup width must be > 0",
        ));
    }
    if bytes_len > width {
        return Err(TioError::invalid_argument(
            "fixed-text Coordinate v2 lookup key must be no wider than width",
        ));
    }
    Ok(())
}

fn fixed_text_value_count(bytes_len: usize, width: usize) -> Result<usize> {
    if width == 0 {
        return Err(TioError::invalid_argument(
            "fixed-text Coordinate v2 width must be > 0 when values are present",
        ));
    }
    if bytes_len % width != 0 {
        return Err(TioError::invalid_argument(
            "fixed-text Coordinate v2 values length must be a multiple of width",
        ));
    }
    Ok(bytes_len / width)
}

/// Coordinate v2 input descriptor for future create APIs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisCoordinateInputV2 {
    /// Axis index.
    pub axis: usize,
    /// Optional descriptor identifier.
    pub descriptor_id: Option<String>,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate semantic kind.
    pub kind: CoordinateKind,
    /// Coordinate value domain.
    pub value_domain: CoordinateValueDomainV2,
    /// Numeric dtype for inline numeric values.
    pub numeric_dtype: CoordinateDType,
    /// Numeric encoding for inline numeric values.
    pub numeric_encoding: CoordinateEncoding,
    /// Fixed-text layout for fixed-text domains.
    pub fixed_text: CoordinateFixedTextLayoutV2,
    /// Dictionary code dtype.
    pub code_dtype: CoordinateCodeDTypeV2,
    /// Immediate create-time values, if this is a fixed-axis value domain.
    pub values: CoordinateInputValuesV2,
    /// Dictionary summary for dictionary-code domains.
    pub dictionary: Option<CoordinateDictionarySummaryV2>,
    /// Dictionary entries for dictionary-code domains.
    pub dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    /// External binding for external-reference domains.
    pub external_binding: Option<CoordinateExternalBindingV2>,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Whether this coordinate is required.
    pub required: bool,
}

impl AxisCoordinateInputV2 {
    /// Creates an inline i32 Coordinate v2 descriptor.
    pub fn inline_i32(axis: usize, values: Vec<i32>) -> Self {
        Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "inline-i32")),
            name: None,
            kind: CoordinateKind::DomainValue,
            value_domain: CoordinateValueDomainV2::InlineNumeric,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: CoordinateFixedTextLayoutV2::default(),
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::I32(values),
            dictionary: None,
            dictionary_entries: Vec::new(),
            external_binding: None,
            ordering: CoordinateOrdering::default(),
            required: false,
        }
    }

    /// Creates an inline i64 Coordinate v2 descriptor.
    pub fn inline_i64(axis: usize, values: Vec<i64>) -> Self {
        Self {
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "inline-i64")),
            numeric_dtype: CoordinateDType::I64,
            values: CoordinateInputValuesV2::I64(values),
            ..Self::inline_i32(axis, Vec::new())
        }
    }

    /// Creates an inline fixed-text descriptor from already padded fixed-width bytes.
    pub fn fixed_text_bytes(
        axis: usize,
        layout: CoordinateFixedTextLayoutV2,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        validate_fixed_text_layout_v2(layout)?;
        validate_fixed_text_bytes_v2(&bytes, layout)?;
        Ok(Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "fixed-text")),
            name: None,
            kind: CoordinateKind::DomainValue,
            value_domain: CoordinateValueDomainV2::FixedText,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: layout,
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::FixedText(bytes),
            dictionary: None,
            dictionary_entries: Vec::new(),
            external_binding: None,
            ordering: CoordinateOrdering::default(),
            required: false,
        })
    }

    /// Creates an inline fixed-width ASCII descriptor, right-padding each value with spaces.
    pub fn fixed_text_ascii<I, S>(axis: usize, width: usize, values: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let layout = CoordinateFixedTextLayoutV2::ascii_right_space_padded(width)?;
        let bytes = encode_fixed_text_ascii_values(width, values)?;
        Self::fixed_text_bytes(axis, layout, bytes)
    }

    /// Creates a dictionary-code descriptor from owned code values and dictionary metadata.
    pub fn dictionary_codes(
        axis: usize,
        code_dtype: CoordinateCodeDTypeV2,
        values: CoordinateInputValuesV2,
        label_layout: CoordinateFixedTextLayoutV2,
        mut dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        validate_dictionary_values_v2(&values, code_dtype)?;
        validate_fixed_text_layout_v2(label_layout)?;
        if dictionary.code_dtype != code_dtype {
            return Err(TioError::invalid_argument(
                "Coordinate v2 dictionary summary code_dtype must match descriptor code_dtype",
            ));
        }
        if dictionary.entry_count == 0 && !dictionary_entries.is_empty() {
            dictionary.entry_count = dictionary_entries.len() as u64;
        }
        Ok(Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "dictionary-code")),
            name: None,
            kind: CoordinateKind::LabelId,
            value_domain: CoordinateValueDomainV2::DictionaryCode,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: label_layout,
            code_dtype,
            values,
            dictionary: Some(dictionary),
            dictionary_entries,
            external_binding: None,
            ordering: CoordinateOrdering::default(),
            required: false,
        })
    }

    /// Creates a dictionary-code descriptor with `u8` code values.
    pub fn dictionary_codes_u8(
        axis: usize,
        values: Vec<u8>,
        label_layout: CoordinateFixedTextLayoutV2,
        dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U8,
            CoordinateInputValuesV2::CodesU8(values),
            label_layout,
            dictionary,
            dictionary_entries,
        )
    }

    /// Creates a dictionary-code descriptor with `u16` code values.
    pub fn dictionary_codes_u16(
        axis: usize,
        values: Vec<u16>,
        label_layout: CoordinateFixedTextLayoutV2,
        dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U16,
            CoordinateInputValuesV2::CodesU16(values),
            label_layout,
            dictionary,
            dictionary_entries,
        )
    }

    /// Creates a dictionary-code descriptor with `u32` code values.
    pub fn dictionary_codes_u32(
        axis: usize,
        values: Vec<u32>,
        label_layout: CoordinateFixedTextLayoutV2,
        dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U32,
            CoordinateInputValuesV2::CodesU32(values),
            label_layout,
            dictionary,
            dictionary_entries,
        )
    }

    /// Creates a dictionary-code descriptor with `u64` code values.
    pub fn dictionary_codes_u64(
        axis: usize,
        values: Vec<u64>,
        label_layout: CoordinateFixedTextLayoutV2,
        dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U64,
            CoordinateInputValuesV2::CodesU64(values),
            label_layout,
            dictionary,
            dictionary_entries,
        )
    }

    /// Creates an append-axis numeric i32 declaration; append values arrive with payload appends.
    pub fn append_numeric_i32(axis: usize) -> Self {
        Self {
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "append-i32")),
            value_domain: CoordinateValueDomainV2::AppendSequence,
            values: CoordinateInputValuesV2::None,
            ..Self::inline_i32(axis, Vec::new())
        }
    }

    /// Creates an append-axis numeric i64 declaration; append values arrive with payload appends.
    pub fn append_numeric_i64(axis: usize) -> Self {
        Self {
            descriptor_id: Some(default_coordinate_v2_descriptor_id(axis, "append-i64")),
            numeric_dtype: CoordinateDType::I64,
            value_domain: CoordinateValueDomainV2::AppendSequence,
            values: CoordinateInputValuesV2::None,
            ..Self::inline_i32(axis, Vec::new())
        }
    }

    /// Creates an append-axis fixed-text declaration; append values arrive with payload appends.
    pub fn append_fixed_text(axis: usize, layout: CoordinateFixedTextLayoutV2) -> Result<Self> {
        validate_fixed_text_layout_v2(layout)?;
        Ok(Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(
                axis,
                "append-fixed-text",
            )),
            name: None,
            kind: CoordinateKind::DomainValue,
            value_domain: CoordinateValueDomainV2::AppendSequence,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: layout,
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::None,
            dictionary: None,
            dictionary_entries: Vec::new(),
            external_binding: None,
            ordering: CoordinateOrdering::default(),
            required: false,
        })
    }

    /// Creates an append-axis dictionary-code declaration; append codes arrive with payload appends.
    pub fn append_dictionary_codes(
        axis: usize,
        code_dtype: CoordinateCodeDTypeV2,
        label_layout: CoordinateFixedTextLayoutV2,
        mut dictionary: CoordinateDictionarySummaryV2,
        dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        validate_fixed_text_layout_v2(label_layout)?;
        if dictionary.code_dtype != code_dtype {
            return Err(TioError::invalid_argument(
                "Coordinate v2 append dictionary summary code_dtype must match descriptor code_dtype",
            ));
        }
        if dictionary.entry_count == 0 && !dictionary_entries.is_empty() {
            dictionary.entry_count = dictionary_entries.len() as u64;
        }
        Ok(Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(
                axis,
                "append-dictionary-code",
            )),
            name: None,
            kind: CoordinateKind::LabelId,
            value_domain: CoordinateValueDomainV2::AppendSequence,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: label_layout,
            code_dtype,
            values: CoordinateInputValuesV2::None,
            dictionary: Some(dictionary),
            dictionary_entries,
            external_binding: None,
            ordering: CoordinateOrdering::default(),
            required: false,
        })
    }

    /// Creates a numeric external-reference descriptor summary. The public Rust wrapper never dereferences it.
    pub fn external_reference(axis: usize, external_binding: CoordinateExternalBindingV2) -> Self {
        Self {
            axis,
            descriptor_id: Some(default_coordinate_v2_descriptor_id(
                axis,
                "external-reference",
            )),
            name: None,
            kind: CoordinateKind::DomainValue,
            value_domain: CoordinateValueDomainV2::ExternalReference,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            fixed_text: CoordinateFixedTextLayoutV2::default(),
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::None,
            dictionary: None,
            dictionary_entries: Vec::new(),
            required: external_binding.required,
            external_binding: Some(external_binding),
            ordering: CoordinateOrdering::default(),
        }
    }

    /// Creates a numeric external-reference descriptor with explicit numeric metadata.
    pub fn external_reference_numeric(
        axis: usize,
        external_binding: CoordinateExternalBindingV2,
        numeric_dtype: CoordinateDType,
        numeric_encoding: CoordinateEncoding,
    ) -> Result<Self> {
        if external_binding.value_domain != CoordinateValueDomainV2::InlineNumeric {
            return Err(TioError::invalid_argument(
                "Coordinate v2 numeric external references require InlineNumeric binding metadata",
            ));
        }
        let mut input = Self::external_reference(axis, external_binding);
        input.numeric_dtype = numeric_dtype;
        input.numeric_encoding = numeric_encoding;
        Ok(input)
    }

    /// Creates a fixed-text external-reference descriptor with explicit fixed-text metadata.
    pub fn external_reference_fixed_text(
        axis: usize,
        external_binding: CoordinateExternalBindingV2,
        layout: CoordinateFixedTextLayoutV2,
    ) -> Result<Self> {
        if external_binding.value_domain != CoordinateValueDomainV2::FixedText {
            return Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text external references require FixedText binding metadata",
            ));
        }
        validate_fixed_text_layout_v2(layout)?;
        let mut input = Self::external_reference(axis, external_binding);
        input.fixed_text = layout;
        Ok(input)
    }

    /// Creates a dictionary-code external-reference descriptor with persisted code-dtype metadata only.
    ///
    /// The current C ABI create path ignores dictionary summaries on external references, so this
    /// helper deliberately accepts only the code dtype that native create persists.
    pub fn external_reference_dictionary_codes(
        axis: usize,
        external_binding: CoordinateExternalBindingV2,
        code_dtype: CoordinateCodeDTypeV2,
    ) -> Result<Self> {
        if external_binding.value_domain != CoordinateValueDomainV2::DictionaryCode {
            return Err(TioError::invalid_argument(
                "Coordinate v2 dictionary external references require DictionaryCode binding metadata",
            ));
        }
        let mut input = Self::external_reference(axis, external_binding);
        input.kind = CoordinateKind::LabelId;
        input.code_dtype = code_dtype;
        Ok(input)
    }

    /// Sets the optional descriptor identifier and returns the modified descriptor.
    pub fn with_descriptor_id(mut self, descriptor_id: impl Into<String>) -> Self {
        self.descriptor_id = Some(descriptor_id.into());
        self
    }

    /// Sets the optional coordinate name and returns the modified descriptor.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the coordinate semantic kind and returns the modified descriptor.
    pub fn with_kind(mut self, kind: CoordinateKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets numeric encoding metadata and returns the modified descriptor.
    pub fn with_numeric_encoding(mut self, encoding: CoordinateEncoding) -> Self {
        self.numeric_encoding = encoding;
        self
    }

    /// Sets ordering hints and returns the modified descriptor.
    pub fn with_ordering(mut self, ordering: CoordinateOrdering) -> Self {
        self.ordering = ordering;
        self
    }

    /// Marks the coordinate required or optional and returns the modified descriptor.
    pub fn with_required(mut self, required: bool) -> Self {
        self.required = required;
        if let Some(binding) = &mut self.external_binding {
            binding.required = required;
        }
        self
    }

    /// Prepares a raw C ABI Coordinate v2 input descriptor with borrowed pointers.
    pub fn prepare(&self) -> Result<PreparedAxisCoordinateInputV2<'_>> {
        PreparedAxisCoordinateInputV2::new(self)
    }

    fn raw_fixed_text_layout(&self) -> sys::ArcadiaTioCoordinateFixedTextLayoutV2 {
        if self.value_domain == CoordinateValueDomainV2::FixedText || self.fixed_text.width > 0 {
            self.fixed_text.to_raw()
        } else {
            sys::ArcadiaTioCoordinateFixedTextLayoutV2::default()
        }
    }
}

/// Prepared Coordinate v2 input descriptor whose raw pointers borrow from owned Rust storage.
pub struct PreparedAxisCoordinateInputV2<'a> {
    // Keep CString/nested preparation storage alive for raw C ABI pointers in `raw`.
    _descriptor_id: Option<CString>,
    _name: Option<CString>,
    _dictionary: Option<PreparedCoordinateDictionarySummaryV2>,
    _dictionary_entries: PreparedCoordinateDictionaryEntriesV2,
    _external_binding: Option<PreparedCoordinateExternalBindingV2>,
    raw: sys::ArcadiaTioAxisCoordinateInputV2,
    _values: PhantomData<&'a AxisCoordinateInputV2>,
}

impl<'a> PreparedAxisCoordinateInputV2<'a> {
    fn new(input: &'a AxisCoordinateInputV2) -> Result<Self> {
        validate_coordinate_input_v2(input)?;
        let descriptor_id =
            optional_owned_cstring(&input.descriptor_id, "Coordinate v2 descriptor_id")?;
        let name = optional_owned_cstring(&input.name, "Coordinate v2 name")?;
        let dictionary = input
            .dictionary
            .as_ref()
            .map(CoordinateDictionarySummaryV2::prepare)
            .transpose()?;
        let dictionary_entries =
            PreparedCoordinateDictionaryEntriesV2::new(&input.dictionary_entries)?;
        let external_binding = input
            .external_binding
            .as_ref()
            .map(CoordinateExternalBindingV2::prepare)
            .transpose()?;
        let (values, values_len) = input.values.pointer_len_for_axis(input.fixed_text.width)?;
        let raw = sys::ArcadiaTioAxisCoordinateInputV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioAxisCoordinateInputV2>(),
            axis: input.axis,
            descriptor_id: opt_cstring_ptr(&descriptor_id),
            name: opt_cstring_ptr(&name),
            kind: input.kind.to_raw(),
            value_domain: input.value_domain.to_raw(),
            numeric_dtype: input.numeric_dtype.to_raw(),
            numeric_encoding: input.numeric_encoding.to_raw(),
            fixed_text: input.raw_fixed_text_layout(),
            code_dtype: input.code_dtype.to_raw(),
            values,
            values_len,
            dictionary: dictionary
                .as_ref()
                .map_or(ptr::null(), |value| value.raw_ptr()),
            dictionary_entries: dictionary_entries.ptr(),
            dictionary_entries_len: dictionary_entries.len(),
            external_binding: external_binding
                .as_ref()
                .map_or(ptr::null(), |value| value.raw_ptr()),
            sorted: input.ordering.sorted.to_raw(),
            monotonicity: input.ordering.monotonicity.to_raw(),
            uniqueness: input.ordering.uniqueness.to_raw(),
            required: u8::from(input.required),
            reserved_u8: [0; 7],
            reserved: [0; 4],
        };
        Ok(Self {
            _descriptor_id: descriptor_id,
            _name: name,
            _dictionary: dictionary,
            _dictionary_entries: dictionary_entries,
            _external_binding: external_binding,
            raw,
            _values: PhantomData,
        })
    }

    /// Returns the raw C ABI input descriptor. Pointers remain valid while `self` is alive.
    pub fn raw(&self) -> &sys::ArcadiaTioAxisCoordinateInputV2 {
        &self.raw
    }
}

struct PreparedAxisCoordinateInputsV2<'a> {
    prepared: Vec<PreparedAxisCoordinateInputV2<'a>>,
    raw: Vec<sys::ArcadiaTioAxisCoordinateInputV2>,
}

impl<'a> PreparedAxisCoordinateInputsV2<'a> {
    fn new(inputs: &'a [AxisCoordinateInputV2], rank: usize) -> Result<Self> {
        for (idx, input) in inputs.iter().enumerate() {
            if input.axis >= rank {
                return Err(TioError::invalid_argument(format!(
                    "Coordinate v2 descriptor {idx} axis out of range"
                )));
            }
        }
        let prepared = inputs
            .iter()
            .map(PreparedAxisCoordinateInputV2::new)
            .collect::<Result<Vec<_>>>()?;
        let raw = prepared.iter().map(|item| *item.raw()).collect::<Vec<_>>();
        Ok(Self { prepared, raw })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioAxisCoordinateInputV2 {
        let _keep_alive = &self.prepared;
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.raw.len()
    }
}

/// Coordinate v2 metadata snapshot copied from native-owned descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AxisCoordinateMetaV2 {
    /// Axis index.
    pub axis: usize,
    /// Optional axis name snapshot.
    pub axis_name_snapshot: Option<String>,
    /// Descriptor identifier.
    pub descriptor_id: Option<String>,
    /// Descriptor revision.
    pub descriptor_revision: u64,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Coordinate semantic kind.
    pub kind: CoordinateKind,
    /// Coordinate value domain.
    pub value_domain: CoordinateValueDomainV2,
    /// Numeric dtype.
    pub numeric_dtype: CoordinateDType,
    /// Numeric encoding.
    pub numeric_encoding: CoordinateEncoding,
    /// Fixed-text layout.
    pub fixed_text: CoordinateFixedTextLayoutV2,
    /// Dictionary code dtype.
    pub code_dtype: CoordinateCodeDTypeV2,
    /// Coordinate length.
    pub length: u64,
    /// Ordering hints.
    pub ordering: CoordinateOrdering,
    /// Whether the coordinate is required.
    pub required: bool,
    /// Availability status.
    pub availability: CoordinateAvailabilityV2,
    /// Status category.
    pub status_category: CoordinateStatusCategoryV2,
    /// Status reason.
    pub reason: Option<String>,
    /// Dictionary summary.
    pub dictionary: CoordinateDictionarySummaryV2,
    /// External binding summary.
    pub external_binding: CoordinateExternalBindingV2,
    /// Optional index summaries.
    pub index_summaries: Vec<CoordinateIndexSummaryV2>,
}

impl AxisCoordinateMetaV2 {
    fn from_raw(raw: &sys::ArcadiaTioAxisCoordinateMetaV2) -> Result<Self> {
        Ok(Self {
            axis: raw.axis,
            axis_name_snapshot: optional_c_string(raw.axis_name_snapshot.cast_const()),
            descriptor_id: optional_c_string(raw.descriptor_id.cast_const()),
            descriptor_revision: raw.descriptor_revision,
            name: optional_c_string(raw.name.cast_const()),
            kind: CoordinateKind::from_raw(raw.kind)?,
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            numeric_dtype: CoordinateDType::from_raw(raw.numeric_dtype)?,
            numeric_encoding: CoordinateEncoding::from_raw(raw.numeric_encoding)?,
            fixed_text: CoordinateFixedTextLayoutV2::from_raw(raw.fixed_text)?,
            code_dtype: CoordinateCodeDTypeV2::from_raw(raw.code_dtype)?,
            length: raw.length,
            ordering: CoordinateOrdering {
                sorted: CoordinateSortedness::from_raw(raw.sorted)?,
                monotonicity: CoordinateMonotonicity::from_raw(raw.monotonicity)?,
                uniqueness: CoordinateUniqueness::from_raw(raw.uniqueness)?,
            },
            required: raw.required != 0,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            reason: optional_c_string(raw.reason.cast_const()),
            dictionary: CoordinateDictionarySummaryV2::from_raw(&raw.dictionary)?,
            external_binding: CoordinateExternalBindingV2::from_raw(&raw.external_binding)?,
            index_summaries: copy_coordinate_index_summaries_v2(
                raw.index_summaries,
                raw.index_summaries_len,
            )?,
        })
    }
}

/// Coordinate v2 value-slice result copied into Rust-owned bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateValueSliceV2 {
    /// Value domain.
    pub value_domain: CoordinateValueDomainV2,
    /// Numeric dtype.
    pub numeric_dtype: CoordinateDType,
    /// Numeric encoding.
    pub numeric_encoding: CoordinateEncoding,
    /// Dictionary code dtype.
    pub code_dtype: CoordinateCodeDTypeV2,
    /// Rust-owned raw value bytes.
    pub data: Vec<u8>,
    /// Number of logical values.
    pub len: usize,
    /// Element size in bytes.
    pub element_size: usize,
    /// Fixed-text width.
    pub fixed_text_width: usize,
    /// Availability.
    pub availability: CoordinateAvailabilityV2,
    /// Status category.
    pub status_category: CoordinateStatusCategoryV2,
    /// Status reason.
    pub reason: Option<String>,
}

impl CoordinateValueSliceV2 {
    /// Copies a raw value-slice carrier into safe Rust-owned bytes.
    ///
    /// # Safety
    ///
    /// `raw.data` must be valid for `raw.len * raw.element_size` bytes when non-null according to
    /// the C ABI, and the caller must later release the raw carrier with the matching free function.
    pub unsafe fn from_raw_borrowed(raw: &sys::ArcadiaTioCoordinateValueSliceV2) -> Result<Self> {
        let byte_len = raw.len.checked_mul(raw.element_size).ok_or_else(|| {
            TioError::conversion("Coordinate v2 value slice byte length overflow")
        })?;
        let data = if raw.data.is_null() || byte_len == 0 {
            Vec::new()
        } else {
            // SAFETY: Caller guarantees the C ABI value buffer is valid for `byte_len` bytes.
            unsafe { slice::from_raw_parts(raw.data.cast::<u8>(), byte_len) }.to_vec()
        };
        Ok(Self {
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            numeric_dtype: CoordinateDType::from_raw(raw.numeric_dtype)?,
            numeric_encoding: CoordinateEncoding::from_raw(raw.numeric_encoding)?,
            code_dtype: CoordinateCodeDTypeV2::from_raw(raw.code_dtype)?,
            data,
            len: raw.len,
            element_size: raw.element_size,
            fixed_text_width: raw.fixed_text_width,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            reason: optional_c_string(raw.reason.cast_const()),
        })
    }
}

/// Coordinate v2 typed lookup key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoordinateLookupKeyV2 {
    /// Signed 32-bit integer key.
    I32(i32),
    /// Signed 64-bit integer key.
    I64(i64),
    /// Fixed-width byte key.
    FixedText { bytes: Vec<u8>, width: usize },
    /// Dictionary code key.
    DictionaryCode(u64),
    /// Dictionary stable-id key.
    StableId(String),
    /// Dictionary display-label key.
    DisplayLabel(String),
    /// Dictionary alias key.
    Alias(String),
    /// Raw integer time key.
    RawTime(i64),
}

impl CoordinateLookupKeyV2 {
    /// Builds a signed 32-bit integer lookup key.
    pub fn i32(value: i32) -> Self {
        Self::I32(value)
    }

    /// Builds a signed 64-bit integer lookup key.
    pub fn i64(value: i64) -> Self {
        Self::I64(value)
    }

    /// Builds a fixed-width ASCII byte lookup key with an explicit descriptor width.
    ///
    /// The bytes are logical fixed-text bytes; the native Coordinate v2 lookup normalizes them
    /// against the selected descriptor width and right-space padding. Variable-length string,
    /// collation, and case-folding semantics are intentionally not inferred here.
    pub fn fixed_text_bytes(bytes: impl Into<Vec<u8>>, width: usize) -> Result<Self> {
        let bytes = bytes.into();
        validate_fixed_text_lookup_key(bytes.len(), width)?;
        if !bytes.is_ascii() {
            return Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text lookup keys must be ASCII bytes",
            ));
        }
        Ok(Self::FixedText { bytes, width })
    }

    /// Builds a fixed-width ASCII text lookup key with an explicit descriptor width.
    ///
    /// This accepts only raw ASCII logical text. Variable-length strings, Unicode
    /// normalization, locale/collation, and case folding remain deferred.
    pub fn fixed_text_ascii(value: impl AsRef<str>, width: usize) -> Result<Self> {
        Self::fixed_text_bytes(value.as_ref().as_bytes().to_vec(), width)
    }

    /// Builds a dictionary-code lookup key.
    pub fn dictionary_code(code: u64) -> Self {
        Self::DictionaryCode(code)
    }

    /// Builds a dictionary stable-id lookup key.
    pub fn stable_id(value: impl Into<String>) -> Self {
        Self::StableId(value.into())
    }

    /// Builds a dictionary display-label lookup key.
    pub fn display_label(value: impl Into<String>) -> Self {
        Self::DisplayLabel(value.into())
    }

    /// Builds a dictionary alias lookup key.
    ///
    /// Alias lookup is represented because the raw C ABI has a stable key domain for it; current
    /// native implementations may return an ordinary unsupported lookup result for descriptors that
    /// do not support alias lookup.
    pub fn alias(value: impl Into<String>) -> Self {
        Self::Alias(value.into())
    }

    /// Builds a raw encoded time lookup key.
    ///
    /// The value is passed as an integer key only. Calendar/session/timezone/leap-second
    /// interpretation is deliberately not implemented by the public Rust wrapper.
    pub fn raw_time_i64(raw_encoded_value: i64) -> Self {
        Self::RawTime(raw_encoded_value)
    }

    /// Rejects unsupported variable-string lookup semantics explicitly.
    pub fn variable_string(_value: impl AsRef<str>) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 variable-length string lookup semantics are not supported by the public Rust wrapper",
        ))
    }

    /// Rejects unsupported calendar-aware lookup semantics explicitly.
    pub fn calendar_time(_value: impl AsRef<str>) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 calendar-aware lookup semantics are not supported; use raw_time_i64 for raw encoded values",
        ))
    }

    /// Rejects unsupported external resolver lookup semantics explicitly.
    pub fn external_resolver(_value: impl AsRef<str>) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 external resolver lookup semantics are not supported by the public Rust wrapper",
        ))
    }

    /// Prepares a raw lookup key with pointer fields borrowing from this prepared object.
    pub fn prepare(&self) -> Result<PreparedCoordinateLookupKeyV2<'_>> {
        PreparedCoordinateLookupKeyV2::new(self)
    }
}

/// Prepared Coordinate v2 lookup key.
pub struct PreparedCoordinateLookupKeyV2<'a> {
    // Keep optional lookup text alive for raw C ABI pointers in `raw`.
    _text: Option<CString>,
    raw: sys::ArcadiaTioCoordinateLookupKeyV2,
    _bytes: PhantomData<&'a CoordinateLookupKeyV2>,
}

impl<'a> PreparedCoordinateLookupKeyV2<'a> {
    fn new(key: &'a CoordinateLookupKeyV2) -> Result<Self> {
        let mut raw = sys::ArcadiaTioCoordinateLookupKeyV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioCoordinateLookupKeyV2>(),
            key_domain: sys::ARCADIA_TIO_COORDINATE_KEY_DOMAIN_V2_I32,
            i32_value: 0,
            i64_value: 0,
            code_value: 0,
            bytes: ptr::null(),
            bytes_len: 0,
            fixed_text_width: 0,
            text: ptr::null(),
            reserved: [0; 4],
        };
        let text = match key {
            CoordinateLookupKeyV2::I32(value) => {
                raw.key_domain = CoordinateKeyDomainV2::I32.to_raw();
                raw.i32_value = *value;
                None
            }
            CoordinateLookupKeyV2::I64(value) => {
                raw.key_domain = CoordinateKeyDomainV2::I64.to_raw();
                raw.i64_value = *value;
                None
            }
            CoordinateLookupKeyV2::FixedText { bytes, width } => {
                validate_fixed_text_lookup_key(bytes.len(), *width)?;
                raw.key_domain = CoordinateKeyDomainV2::FixedText.to_raw();
                raw.bytes = buffer_ptr_for_count(bytes, bytes.len()).cast::<u8>();
                raw.bytes_len = bytes.len();
                raw.fixed_text_width = *width;
                None
            }
            CoordinateLookupKeyV2::DictionaryCode(value) => {
                raw.key_domain = CoordinateKeyDomainV2::DictionaryCode.to_raw();
                raw.code_value = *value;
                None
            }
            CoordinateLookupKeyV2::StableId(value) => {
                let cstr = string_to_cstring(value, "Coordinate v2 stable-id lookup key")?;
                raw.key_domain = CoordinateKeyDomainV2::StableId.to_raw();
                raw.text = cstr.as_ptr();
                Some(cstr)
            }
            CoordinateLookupKeyV2::DisplayLabel(value) => {
                let cstr = string_to_cstring(value, "Coordinate v2 display-label lookup key")?;
                raw.key_domain = CoordinateKeyDomainV2::DisplayLabel.to_raw();
                raw.text = cstr.as_ptr();
                Some(cstr)
            }
            CoordinateLookupKeyV2::Alias(value) => {
                let cstr = string_to_cstring(value, "Coordinate v2 alias lookup key")?;
                raw.key_domain = CoordinateKeyDomainV2::Alias.to_raw();
                raw.text = cstr.as_ptr();
                Some(cstr)
            }
            CoordinateLookupKeyV2::RawTime(value) => {
                raw.key_domain = CoordinateKeyDomainV2::RawTime.to_raw();
                raw.i64_value = *value;
                None
            }
        };
        Ok(Self {
            _text: text,
            raw,
            _bytes: PhantomData,
        })
    }

    /// Returns the raw C ABI lookup key. Pointers remain valid while `self` is alive.
    pub fn raw(&self) -> &sys::ArcadiaTioCoordinateLookupKeyV2 {
        &self.raw
    }
}

/// Coordinate v2 lookup result copied into Rust-owned vectors/strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoordinateLookupResultV2 {
    /// Lookup result status.
    pub status: CoordinateLookupResultStatusV2,
    /// Status category.
    pub status_category: CoordinateStatusCategoryV2,
    /// Unique position.
    pub unique_position: u32,
    /// Half-open range start.
    pub range_start: u32,
    /// Half-open range end.
    pub range_end: u32,
    /// Many-result positions.
    pub positions: Vec<u32>,
    /// Availability.
    pub availability: CoordinateAvailabilityV2,
    /// Status reason.
    pub reason: Option<String>,
}

impl CoordinateLookupResultV2 {
    /// Returns true when this result carries one unique position.
    pub fn is_unique(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Unique
    }

    /// Returns true when this result carries a half-open range.
    pub fn is_range(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Range
    }

    /// Returns true when this result carries many positions.
    pub fn is_many(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Many
    }

    /// Returns true when the key is missing.
    pub fn is_missing(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Missing
    }

    /// Returns true when coordinate data is unavailable for the selected root.
    pub fn is_unavailable(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Unavailable
    }

    /// Returns true when a unique lookup found duplicates.
    pub fn is_duplicate(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Duplicate
    }

    /// Returns true when the lookup domain/operation is unsupported.
    pub fn is_unsupported(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Unsupported
    }

    /// Returns true when the raw lookup reports an ordinary error-status result.
    pub fn is_error(&self) -> bool {
        self.status == CoordinateLookupResultStatusV2::Error
    }

    /// Returns the unique position when this result is unique.
    pub fn unique_position(&self) -> Option<u32> {
        self.is_unique().then_some(self.unique_position)
    }

    /// Returns the half-open range when this result is a range result.
    pub fn range(&self) -> Option<Range<u32>> {
        self.is_range().then_some(self.range_start..self.range_end)
    }

    /// Returns many-result positions when this result carries many positions.
    pub fn many_positions(&self) -> Option<&[u32]> {
        self.is_many().then_some(self.positions.as_slice())
    }

    /// Copies a raw lookup result into safe Rust-owned values.
    ///
    /// # Safety
    ///
    /// `raw.positions` must be valid for `raw.positions_len` entries when non-null according to
    /// the C ABI, and the caller must later release the raw carrier with the matching free function.
    pub unsafe fn from_raw_borrowed(raw: &sys::ArcadiaTioCoordinateLookupResultV2) -> Result<Self> {
        let positions = if raw.positions.is_null() || raw.positions_len == 0 {
            Vec::new()
        } else {
            // SAFETY: Caller guarantees the C ABI positions buffer is valid for `positions_len`.
            unsafe { slice::from_raw_parts(raw.positions, raw.positions_len) }.to_vec()
        };
        Ok(Self {
            status: CoordinateLookupResultStatusV2::from_raw(raw.status)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            unique_position: raw.unique_position,
            range_start: raw.range_start,
            range_end: raw.range_end,
            positions,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            reason: optional_c_string(raw.reason.cast_const()),
        })
    }
}

/// Coordinate v2 append coordinate entry.
///
/// Safe builders own the coordinate buffers. During `prepare`, raw C ABI pointers borrow from
/// these Rust-owned buffers and from prepared descriptor/name strings; those borrowed pointers are
/// valid only while the returned `PreparedAppendCoordinateBatchV2` and this source batch remain
/// alive. Append-with-coordinate methods prepare a batch and call the C ABI synchronously without
/// storing the borrowed pointers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppendCoordinateEntryV2 {
    /// Axis index.
    pub axis: usize,
    /// Optional descriptor identifier.
    pub descriptor_id: Option<String>,
    /// Optional coordinate name.
    pub name: Option<String>,
    /// Value domain.
    pub value_domain: CoordinateValueDomainV2,
    /// Numeric dtype.
    pub numeric_dtype: CoordinateDType,
    /// Numeric encoding.
    pub numeric_encoding: CoordinateEncoding,
    /// Dictionary code dtype.
    pub code_dtype: CoordinateCodeDTypeV2,
    /// Append values.
    pub values: CoordinateInputValuesV2,
    /// Fixed-text width for fixed-text append values.
    pub fixed_text_width: usize,
    /// Append-time dictionary-extension entries for dictionary-code append values.
    pub dictionary_entries: Vec<CoordinateDictionaryEntryV2>,
}

impl AppendCoordinateEntryV2 {
    /// Creates an i32 append-coordinate entry from Rust-owned coordinate values.
    pub fn i32(axis: usize, values: Vec<i32>) -> Self {
        Self {
            axis,
            descriptor_id: None,
            name: None,
            value_domain: CoordinateValueDomainV2::InlineNumeric,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::I32(values),
            fixed_text_width: 0,
            dictionary_entries: Vec::new(),
        }
    }

    /// Creates an i64 append-coordinate entry from Rust-owned coordinate values.
    pub fn i64(axis: usize, values: Vec<i64>) -> Self {
        Self {
            numeric_dtype: CoordinateDType::I64,
            values: CoordinateInputValuesV2::I64(values),
            ..Self::i32(axis, Vec::new())
        }
    }

    /// Creates a fixed-width ASCII/right-space-padded append-coordinate entry from raw bytes.
    ///
    /// `bytes` must contain exactly `count * layout.width` bytes. NUL termination, variable-length
    /// strings, Unicode normalization, locale/collation, and case folding are not inferred.
    pub fn fixed_text_bytes(
        axis: usize,
        layout: CoordinateFixedTextLayoutV2,
        bytes: Vec<u8>,
    ) -> Result<Self> {
        validate_fixed_text_bytes_v2(&bytes, layout)?;
        Ok(Self {
            axis,
            descriptor_id: None,
            name: None,
            value_domain: CoordinateValueDomainV2::FixedText,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            code_dtype: CoordinateCodeDTypeV2::U32,
            values: CoordinateInputValuesV2::FixedText(bytes),
            fixed_text_width: layout.width,
            dictionary_entries: Vec::new(),
        })
    }

    /// Creates a fixed-width ASCII append-coordinate entry, right-padding each logical value.
    pub fn fixed_text_ascii<I, S>(axis: usize, width: usize, values: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let layout = CoordinateFixedTextLayoutV2::ascii_right_space_padded(width)?;
        let bytes = encode_fixed_text_ascii_values(width, values)?;
        Self::fixed_text_bytes(axis, layout, bytes)
    }

    /// Creates a dictionary-code append-coordinate entry from Rust-owned code values.
    ///
    /// The codes must refer to entries in the descriptor-bound dictionary revision unless matching
    /// append-time dictionary-extension entries are attached with `with_dictionary_entries`.
    pub fn dictionary_codes(
        axis: usize,
        code_dtype: CoordinateCodeDTypeV2,
        values: CoordinateInputValuesV2,
    ) -> Result<Self> {
        validate_dictionary_values_v2(&values, code_dtype)?;
        Ok(Self {
            axis,
            descriptor_id: None,
            name: None,
            value_domain: CoordinateValueDomainV2::DictionaryCode,
            numeric_dtype: CoordinateDType::I32,
            numeric_encoding: CoordinateEncoding::Plain,
            code_dtype,
            values,
            fixed_text_width: 0,
            dictionary_entries: Vec::new(),
        })
    }

    /// Creates a dictionary-code append-coordinate entry with `u8` codes.
    pub fn dictionary_codes_u8(axis: usize, values: Vec<u8>) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U8,
            CoordinateInputValuesV2::CodesU8(values),
        )
    }

    /// Creates a dictionary-code append-coordinate entry with `u16` codes.
    pub fn dictionary_codes_u16(axis: usize, values: Vec<u16>) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U16,
            CoordinateInputValuesV2::CodesU16(values),
        )
    }

    /// Creates a dictionary-code append-coordinate entry with `u32` codes.
    pub fn dictionary_codes_u32(axis: usize, values: Vec<u32>) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U32,
            CoordinateInputValuesV2::CodesU32(values),
        )
    }

    /// Creates a dictionary-code append-coordinate entry with `u64` codes.
    pub fn dictionary_codes_u64(axis: usize, values: Vec<u64>) -> Result<Self> {
        Self::dictionary_codes(
            axis,
            CoordinateCodeDTypeV2::U64,
            CoordinateInputValuesV2::CodesU64(values),
        )
    }

    /// Sets the descriptor identifier used to match a specific append-axis descriptor.
    pub fn with_descriptor_id(mut self, descriptor_id: impl Into<String>) -> Self {
        self.descriptor_id = Some(descriptor_id.into());
        self
    }

    /// Sets the coordinate name used to match a specific append-axis descriptor.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets numeric encoding metadata for numeric append entries.
    pub fn with_numeric_encoding(mut self, encoding: CoordinateEncoding) -> Self {
        self.numeric_encoding = encoding;
        self
    }

    /// Rejects unsupported variable-length string append-coordinate semantics explicitly.
    pub fn variable_string<I, S>(_axis: usize, _values: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Err(TioError::unimplemented(
            "Coordinate v2 variable-length string append semantics are not supported by the public Rust wrapper; use fixed_text_ascii/fixed_text_bytes for fixed-width ASCII values",
        ))
    }

    /// Rejects unsupported external append-value resolution explicitly.
    pub fn external_reference(_axis: usize) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 append-time external coordinate values are not supported by the public Rust wrapper",
        ))
    }

    /// Attaches append-time dictionary-extension entries to a dictionary-code append entry.
    pub fn with_dictionary_entries(mut self, entries: Vec<CoordinateDictionaryEntryV2>) -> Self {
        self.dictionary_entries = entries;
        self
    }

    /// Appends one append-time dictionary-extension entry to a dictionary-code append entry.
    pub fn with_dictionary_entry(mut self, entry: CoordinateDictionaryEntryV2) -> Self {
        self.dictionary_entries.push(entry);
        self
    }

    /// Creates a dictionary-code append-coordinate entry with `u8` codes and extension entries.
    pub fn dictionary_codes_u8_with_entries(
        axis: usize,
        values: Vec<u8>,
        entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Ok(Self::dictionary_codes_u8(axis, values)?.with_dictionary_entries(entries))
    }

    /// Creates a dictionary-code append-coordinate entry with `u16` codes and extension entries.
    pub fn dictionary_codes_u16_with_entries(
        axis: usize,
        values: Vec<u16>,
        entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Ok(Self::dictionary_codes_u16(axis, values)?.with_dictionary_entries(entries))
    }

    /// Creates a dictionary-code append-coordinate entry with `u32` codes and extension entries.
    pub fn dictionary_codes_u32_with_entries(
        axis: usize,
        values: Vec<u32>,
        entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Ok(Self::dictionary_codes_u32(axis, values)?.with_dictionary_entries(entries))
    }

    /// Creates a dictionary-code append-coordinate entry with `u64` codes and extension entries.
    pub fn dictionary_codes_u64_with_entries(
        axis: usize,
        values: Vec<u64>,
        entries: Vec<CoordinateDictionaryEntryV2>,
    ) -> Result<Self> {
        Ok(Self::dictionary_codes_u64(axis, values)?.with_dictionary_entries(entries))
    }

    /// Rejects standalone append-time dictionary extension without accompanying codes explicitly.
    pub fn dictionary_extension(_axis: usize) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 append-time dictionary extension entries must be attached to a dictionary-code append entry with with_dictionary_entries",
        ))
    }

    /// Rejects treating optional indexes as authoritative append-coordinate truth explicitly.
    pub fn index_authority(_axis: usize) -> Result<Self> {
        Err(TioError::unimplemented(
            "Coordinate v2 optional indexes are not authoritative append-coordinate values in the public Rust wrapper",
        ))
    }
}

/// Coordinate v2 append coordinate batch.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AppendCoordinateBatchV2 {
    /// Append-coordinate entries.
    pub entries: Vec<AppendCoordinateEntryV2>,
}

impl AppendCoordinateBatchV2 {
    /// Creates an append-coordinate batch from Rust-owned entries.
    pub fn new(entries: Vec<AppendCoordinateEntryV2>) -> Self {
        Self { entries }
    }

    /// Creates an empty append-coordinate batch.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Appends one coordinate entry to this batch.
    pub fn push(&mut self, entry: AppendCoordinateEntryV2) {
        self.entries.push(entry);
    }

    /// Returns the append-coordinate entries.
    pub fn entries(&self) -> &[AppendCoordinateEntryV2] {
        &self.entries
    }

    /// Returns the number of append-coordinate entries in the batch.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true when the batch carries no append-coordinate entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Prepares a raw append batch with borrowed pointers valid while the prepared object lives.
    ///
    /// The raw batch borrows coordinate value buffers from `self.entries` and must be consumed by a
    /// single synchronous C ABI append call before either `self` or the prepared batch is dropped.
    pub fn prepare(&self) -> Result<PreparedAppendCoordinateBatchV2<'_>> {
        PreparedAppendCoordinateBatchV2::new(self)
    }
}

/// Prepared Coordinate v2 append-coordinate batch.
///
/// This object owns the raw entry array and prepared descriptor/name C strings while borrowing the
/// coordinate value buffers from the source `AppendCoordinateBatchV2`. The raw pointers returned by
/// `raw()` must not outlive this prepared object or the source batch.
pub struct PreparedAppendCoordinateBatchV2<'a> {
    // Keep per-entry preparations and the raw entry array alive for C ABI pointers in `raw`.
    _entries: Vec<PreparedAppendCoordinateEntryV2<'a>>,
    _raw_entries: Vec<sys::ArcadiaTioAppendCoordinateEntryV2>,
    raw: sys::ArcadiaTioAppendCoordinateBatchV2,
    _batch: PhantomData<&'a AppendCoordinateBatchV2>,
}

impl<'a> PreparedAppendCoordinateBatchV2<'a> {
    fn new(batch: &'a AppendCoordinateBatchV2) -> Result<Self> {
        let entries = batch
            .entries
            .iter()
            .map(PreparedAppendCoordinateEntryV2::new)
            .collect::<Result<Vec<_>>>()?;
        let raw_entries = entries.iter().map(|entry| entry.raw).collect::<Vec<_>>();
        let raw = sys::ArcadiaTioAppendCoordinateBatchV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioAppendCoordinateBatchV2>(),
            entries: if raw_entries.is_empty() {
                ptr::null()
            } else {
                raw_entries.as_ptr()
            },
            entries_len: raw_entries.len(),
            reserved: [0; 4],
        };
        Ok(Self {
            _entries: entries,
            _raw_entries: raw_entries,
            raw,
            _batch: PhantomData,
        })
    }

    /// Returns the raw C ABI append batch. Pointers remain valid while `self` is alive.
    pub fn raw(&self) -> &sys::ArcadiaTioAppendCoordinateBatchV2 {
        &self.raw
    }
}

/// Canonical alias for current coordinate options.
pub type CoordinateOptions = CoordinateV2Options;
/// Canonical alias for current coordinate value-domain metadata.
pub type CoordinateValueDomain = CoordinateValueDomainV2;
/// Canonical alias for current coordinate lookup key domains.
pub type CoordinateKeyDomain = CoordinateKeyDomainV2;
/// Canonical alias for current dictionary code dtypes.
pub type CoordinateCodeDType = CoordinateCodeDTypeV2;
/// Canonical alias for current fixed-text encoding metadata.
pub type CoordinateFixedTextEncoding = CoordinateFixedTextEncodingV2;
/// Canonical alias for current fixed-text padding metadata.
pub type CoordinateFixedTextPadding = CoordinateFixedTextPaddingV2;
/// Canonical alias for current external coordinate source kinds.
pub type CoordinateSourceKind = CoordinateSourceKindV2;
/// Canonical alias for current coordinate availability metadata.
pub type CoordinateAvailability = CoordinateAvailabilityV2;
/// Canonical alias for current coordinate status categories.
pub type CoordinateStatusCategory = CoordinateStatusCategoryV2;
/// Canonical alias for current coordinate index kinds.
pub type CoordinateIndexKind = CoordinateIndexKindV2;
/// Canonical alias for current coordinate index validation status.
pub type CoordinateIndexValidationStatus = CoordinateIndexValidationStatusV2;
/// Canonical alias for current coordinate index fallback metadata.
pub type CoordinateIndexFallback = CoordinateIndexFallbackV2;
/// Canonical alias for current coordinate index use metadata.
pub type CoordinateIndexUse = CoordinateIndexUseV2;
/// Canonical alias for current coordinate lookup result status.
pub type CoordinateLookupResultStatus = CoordinateLookupResultStatusV2;
/// Canonical alias for current fixed-text coordinate layout.
pub type CoordinateFixedTextLayout = CoordinateFixedTextLayoutV2;
/// Canonical alias for current dictionary summary metadata.
pub type CoordinateDictionarySummary = CoordinateDictionarySummaryV2;
/// Canonical alias for current dictionary entries.
pub type CoordinateDictionaryEntry = CoordinateDictionaryEntryV2;
/// Canonical alias for current external coordinate bindings.
pub type CoordinateExternalBinding = CoordinateExternalBindingV2;
/// Canonical alias for current index source bindings.
pub type CoordinateIndexSourceBinding = CoordinateIndexSourceBindingV2;
/// Canonical alias for current coordinate index summaries.
pub type CoordinateIndexSummary = CoordinateIndexSummaryV2;
/// Canonical alias for current coordinate input values.
pub type CoordinateInputValues = CoordinateInputValuesV2;
/// Canonical alias for current axis coordinate descriptors.
pub type AxisCoordinateInput = AxisCoordinateInputV2;
/// Canonical alias for current axis coordinate metadata.
pub type AxisCoordinateMeta = AxisCoordinateMetaV2;
/// More explicit canonical alias for current axis coordinate metadata.
pub type AxisCoordinateMetadata = AxisCoordinateMetaV2;
/// Canonical alias for current coordinate lookup keys.
pub type CoordinateLookupKey = CoordinateLookupKeyV2;
/// Canonical alias for current coordinate value slices.
pub type CoordinateValueSlice = CoordinateValueSliceV2;
/// Canonical alias for current dictionary outputs.
pub type CoordinateDictionary = CoordinateDictionaryV2;
/// Canonical alias for current coordinate lookup results.
pub type CoordinateLookupResult = CoordinateLookupResultV2;
/// Canonical alias for current append-coordinate entries.
pub type AppendCoordinateEntry = AppendCoordinateEntryV2;
/// Canonical alias for current append-coordinate batches.
pub type AppendCoordinateBatch = AppendCoordinateBatchV2;
/// Canonical alias for prepared append-coordinate batches.
pub type PreparedAppendCoordinateBatch<'a> = PreparedAppendCoordinateBatchV2<'a>;

struct PreparedAppendCoordinateEntryV2<'a> {
    // Keep descriptor/name C strings and dictionary-entry strings alive for raw C ABI pointers in `raw`.
    _descriptor_id: Option<CString>,
    _name: Option<CString>,
    _dictionary_entries: PreparedCoordinateDictionaryEntriesV2,
    raw: sys::ArcadiaTioAppendCoordinateEntryV2,
    _entry: PhantomData<&'a AppendCoordinateEntryV2>,
}

impl<'a> PreparedAppendCoordinateEntryV2<'a> {
    fn new(entry: &'a AppendCoordinateEntryV2) -> Result<Self> {
        validate_append_entry_v2(entry)?;
        let descriptor_id =
            optional_owned_cstring(&entry.descriptor_id, "Coordinate v2 append descriptor_id")?;
        let name = optional_owned_cstring(&entry.name, "Coordinate v2 append name")?;
        let (values, count, element_size) = entry
            .values
            .pointer_count_element_size(entry.fixed_text_width)?;
        let values = if count == 0 { ptr::null() } else { values };
        let dictionary_entries =
            PreparedCoordinateDictionaryEntriesV2::new(&entry.dictionary_entries)?;
        Ok(Self {
            raw: sys::ArcadiaTioAppendCoordinateEntryV2 {
                version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioAppendCoordinateEntryV2>(),
                axis: entry.axis,
                descriptor_id: opt_cstring_ptr(&descriptor_id),
                name: opt_cstring_ptr(&name),
                value_domain: entry.value_domain.to_raw(),
                numeric_dtype: entry.numeric_dtype.to_raw(),
                numeric_encoding: entry.numeric_encoding.to_raw(),
                code_dtype: entry.code_dtype.to_raw(),
                values,
                count,
                element_size,
                fixed_text_width: entry.fixed_text_width,
                dictionary_entries: dictionary_entries.ptr(),
                dictionary_entries_len: dictionary_entries.len(),
                reserved: [0; 2],
            },
            _descriptor_id: descriptor_id,
            _name: name,
            _dictionary_entries: dictionary_entries,
            _entry: PhantomData,
        })
    }
}

struct PreparedCoordinateDictionarySummaryV2 {
    // Keep dictionary summary C strings alive for raw C ABI pointers in `raw`.
    _dictionary_id: Option<CString>,
    _content_id: Option<CString>,
    raw: Box<sys::ArcadiaTioCoordinateDictionarySummaryV2>,
}

impl PreparedCoordinateDictionarySummaryV2 {
    fn new(summary: &CoordinateDictionarySummaryV2) -> Result<Self> {
        let dictionary_id =
            optional_owned_cstring(&summary.dictionary_id, "Coordinate v2 dictionary_id")?;
        let content_id =
            optional_owned_cstring(&summary.content_id, "Coordinate v2 dictionary content_id")?;
        let raw = Box::new(sys::ArcadiaTioCoordinateDictionarySummaryV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioCoordinateDictionarySummaryV2>(),
            dictionary_id: opt_cstring_ptr(&dictionary_id),
            revision: summary.revision,
            code_dtype: summary.code_dtype.to_raw(),
            entry_count: summary.entry_count,
            stable_ids_unique: u8::from(summary.stable_ids_unique),
            display_labels_unique: u8::from(summary.display_labels_unique),
            aliases_unique: u8::from(summary.aliases_unique),
            codes_stable_across_revisions: u8::from(summary.codes_stable_across_revisions),
            reserved_u8: [0; 4],
            content_id: opt_cstring_ptr(&content_id),
            reserved: [0; 2],
        });
        Ok(Self {
            _dictionary_id: dictionary_id,
            _content_id: content_id,
            raw,
        })
    }

    fn raw_ptr(&self) -> *const sys::ArcadiaTioCoordinateDictionarySummaryV2 {
        self.raw.as_ref()
    }
}

struct PreparedCoordinateExternalBindingV2 {
    // Keep external-binding C strings alive for raw C ABI pointers in `raw`.
    _logical_id: Option<CString>,
    _privacy_safe_display: Option<CString>,
    _content_id: Option<CString>,
    raw: Box<sys::ArcadiaTioCoordinateExternalBindingV2>,
}

impl PreparedCoordinateExternalBindingV2 {
    fn new(binding: &CoordinateExternalBindingV2) -> Result<Self> {
        let logical_id =
            optional_owned_cstring(&binding.logical_id, "Coordinate v2 external logical_id")?;
        let privacy_safe_display = optional_owned_cstring(
            &binding.privacy_safe_display,
            "Coordinate v2 external privacy_safe_display",
        )?;
        let content_id =
            optional_owned_cstring(&binding.content_id, "Coordinate v2 external content_id")?;
        let raw = Box::new(sys::ArcadiaTioCoordinateExternalBindingV2 {
            version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioCoordinateExternalBindingV2>(),
            source_kind: binding.source_kind.to_raw(),
            logical_id: opt_cstring_ptr(&logical_id),
            privacy_safe_display: opt_cstring_ptr(&privacy_safe_display),
            content_id: opt_cstring_ptr(&content_id),
            value_domain: binding.value_domain.to_raw(),
            length: binding.length,
            availability: binding.availability.to_raw(),
            status_category: binding.status_category.to_raw(),
            required: u8::from(binding.required),
            reserved_u8: [0; 7],
            reserved: [0; 2],
        });
        Ok(Self {
            _logical_id: logical_id,
            _privacy_safe_display: privacy_safe_display,
            _content_id: content_id,
            raw,
        })
    }

    fn raw_ptr(&self) -> *const sys::ArcadiaTioCoordinateExternalBindingV2 {
        self.raw.as_ref()
    }
}

#[derive(Default)]
struct PreparedCoordinateDictionaryEntriesV2 {
    // Keep dictionary entry strings and alias pointer arrays alive for raw C ABI pointers in `raw`.
    _stable_ids: Vec<Option<CString>>,
    _display_labels: Vec<Option<CString>>,
    _aliases: Vec<Vec<CString>>,
    _alias_ptrs: Vec<Vec<*mut c_char>>,
    raw: Vec<sys::ArcadiaTioCoordinateDictionaryEntryV2>,
}

impl PreparedCoordinateDictionaryEntriesV2 {
    fn new(entries: &[CoordinateDictionaryEntryV2]) -> Result<Self> {
        let stable_ids = entries
            .iter()
            .map(|entry| {
                optional_owned_cstring(&entry.stable_id, "Coordinate v2 dictionary stable_id")
            })
            .collect::<Result<Vec<_>>>()?;
        let display_labels = entries
            .iter()
            .map(|entry| {
                optional_owned_cstring(
                    &entry.display_label,
                    "Coordinate v2 dictionary display_label",
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let aliases = entries
            .iter()
            .map(|entry| {
                entry
                    .aliases
                    .iter()
                    .map(|alias| string_to_cstring(alias, "Coordinate v2 dictionary alias"))
                    .collect::<Result<Vec<_>>>()
            })
            .collect::<Result<Vec<_>>>()?;
        let mut alias_ptrs = aliases
            .iter()
            .map(|items| {
                items
                    .iter()
                    .map(|item| item.as_ptr() as *mut c_char)
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let raw = entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| sys::ArcadiaTioCoordinateDictionaryEntryV2 {
                version: sys::ARCADIA_TIO_COORDINATE_V2_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioCoordinateDictionaryEntryV2>(),
                code: entry.code,
                stable_id: opt_cstring_mut_ptr(&stable_ids[idx]),
                display_label: opt_cstring_mut_ptr(&display_labels[idx]),
                aliases: if alias_ptrs[idx].is_empty() {
                    ptr::null_mut()
                } else {
                    alias_ptrs[idx].as_mut_ptr()
                },
                aliases_len: alias_ptrs[idx].len(),
                reserved: [0; 2],
            })
            .collect::<Vec<_>>();
        Ok(Self {
            _stable_ids: stable_ids,
            _display_labels: display_labels,
            _aliases: aliases,
            _alias_ptrs: alias_ptrs,
            raw,
        })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioCoordinateDictionaryEntryV2 {
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.raw.len()
    }
}

fn default_coordinate_v2_descriptor_id(axis: usize, suffix: &str) -> String {
    format!("axis{axis}-{suffix}")
}

fn encode_fixed_text_ascii_values<I, S>(width: usize, values: I) -> Result<Vec<u8>>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let layout = CoordinateFixedTextLayoutV2::ascii_right_space_padded(width)?;
    let mut encoded = Vec::new();
    for value in values {
        let bytes = value.as_ref().as_bytes();
        if bytes.len() > width {
            return Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text value exceeds declared width",
            ));
        }
        if layout.reject_non_ascii && !bytes.is_ascii() {
            return Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text values must be ASCII",
            ));
        }
        encoded.extend_from_slice(bytes);
        encoded.extend(std::iter::repeat_n(b' ', width - bytes.len()));
    }
    Ok(encoded)
}

fn validate_fixed_text_layout_v2(layout: CoordinateFixedTextLayoutV2) -> Result<()> {
    if layout.width == 0 {
        return Err(TioError::invalid_argument(
            "Coordinate v2 fixed-text width must be > 0",
        ));
    }
    if layout.encoding != CoordinateFixedTextEncodingV2::Ascii {
        return Err(TioError::invalid_argument(
            "Coordinate v2 public Rust wrappers currently support only ASCII fixed text",
        ));
    }
    if layout.padding != CoordinateFixedTextPaddingV2::RightSpace {
        return Err(TioError::invalid_argument(
            "Coordinate v2 public Rust wrappers currently support only right-space padding",
        ));
    }
    Ok(())
}

fn validate_fixed_text_bytes_v2(bytes: &[u8], layout: CoordinateFixedTextLayoutV2) -> Result<()> {
    validate_fixed_text_layout_v2(layout)?;
    fixed_text_value_count(bytes.len(), layout.width)?;
    if layout.reject_non_ascii && !bytes.is_ascii() {
        return Err(TioError::invalid_argument(
            "Coordinate v2 fixed-text values must be ASCII",
        ));
    }
    Ok(())
}

fn validate_dictionary_values_v2(
    values: &CoordinateInputValuesV2,
    code_dtype: CoordinateCodeDTypeV2,
) -> Result<()> {
    match (values, code_dtype) {
        (CoordinateInputValuesV2::CodesU8(_), CoordinateCodeDTypeV2::U8)
        | (CoordinateInputValuesV2::CodesU16(_), CoordinateCodeDTypeV2::U16)
        | (CoordinateInputValuesV2::CodesU32(_), CoordinateCodeDTypeV2::U32)
        | (CoordinateInputValuesV2::CodesU64(_), CoordinateCodeDTypeV2::U64) => Ok(()),
        _ => Err(TioError::invalid_argument(
            "Coordinate v2 dictionary-code values must match code_dtype",
        )),
    }
}

fn validate_dictionary_descriptor_v2(input: &AxisCoordinateInputV2) -> Result<()> {
    let Some(dictionary) = &input.dictionary else {
        return Err(TioError::invalid_argument(
            "Coordinate v2 dictionary-code descriptors require dictionary metadata",
        ));
    };
    if dictionary
        .dictionary_id
        .as_deref()
        .is_none_or(|value| value.is_empty())
    {
        return Err(TioError::invalid_argument(
            "Coordinate v2 dictionary-code descriptors require a non-empty dictionary_id",
        ));
    }
    if input.dictionary_entries.is_empty() {
        return Err(TioError::invalid_argument(
            "Coordinate v2 dictionary-code descriptors require at least one dictionary entry",
        ));
    }
    if dictionary.entry_count != input.dictionary_entries.len() as u64 {
        return Err(TioError::invalid_argument(
            "Coordinate v2 dictionary entry_count must match dictionary_entries length",
        ));
    }
    for (idx, entry) in input.dictionary_entries.iter().enumerate() {
        validate_dictionary_entry_v2(entry, idx)?;
    }
    if dictionary.code_dtype != input.code_dtype {
        return Err(TioError::invalid_argument(
            "Coordinate v2 dictionary summary code_dtype must match descriptor code_dtype",
        ));
    }
    validate_fixed_text_layout_v2(input.fixed_text)?;
    Ok(())
}

fn validate_dictionary_entry_v2(entry: &CoordinateDictionaryEntryV2, idx: usize) -> Result<()> {
    if entry
        .stable_id
        .as_deref()
        .is_none_or(|value| value.is_empty())
    {
        return Err(TioError::invalid_argument(format!(
            "Coordinate v2 dictionary entry {idx} requires a non-empty stable_id"
        )));
    }
    if entry
        .display_label
        .as_deref()
        .is_none_or(|value| value.is_empty())
    {
        return Err(TioError::invalid_argument(format!(
            "Coordinate v2 dictionary entry {idx} requires a non-empty display_label"
        )));
    }
    if entry.aliases.iter().any(|alias| alias.is_empty()) {
        return Err(TioError::invalid_argument(format!(
            "Coordinate v2 dictionary entry {idx} aliases cannot be empty"
        )));
    }
    Ok(())
}

fn validate_external_binding_v2(binding: &CoordinateExternalBindingV2) -> Result<()> {
    let Some(logical_id) = binding.logical_id.as_deref() else {
        return Err(TioError::invalid_argument(
            "Coordinate v2 external-reference descriptors require a non-empty logical_id",
        ));
    };
    if logical_id.is_empty() {
        return Err(TioError::invalid_argument(
            "Coordinate v2 external-reference descriptors require a non-empty logical_id",
        ));
    }
    if binding.source_kind == CoordinateSourceKindV2::SameFileObject
        && (logical_id.contains('/') || logical_id.contains('\\'))
    {
        return Err(TioError::invalid_argument(
            "Coordinate v2 same-file external logical_id must be an object id, not a path",
        ));
    }
    if binding.source_kind == CoordinateSourceKindV2::ApplicationRegistry {
        return Err(TioError::unimplemented(
            "Coordinate v2 application-registry external resolution is not supported by the public Rust wrapper",
        ));
    }
    Ok(())
}

fn validate_coordinate_input_v2(input: &AxisCoordinateInputV2) -> Result<()> {
    if input
        .descriptor_id
        .as_deref()
        .is_none_or(|value| value.is_empty())
    {
        return Err(TioError::invalid_argument(
            "Coordinate v2 descriptor_id is required and cannot be empty",
        ));
    }
    if matches!(input.name.as_deref(), Some("")) {
        return Err(TioError::invalid_argument(
            "Coordinate v2 name cannot be empty",
        ));
    }
    match input.value_domain {
        CoordinateValueDomainV2::InlineNumeric => match (&input.values, input.numeric_dtype) {
            (CoordinateInputValuesV2::I32(_), CoordinateDType::I32)
            | (CoordinateInputValuesV2::I64(_), CoordinateDType::I64)
                if input.fixed_text.width == 0 && input.dictionary.is_none() =>
            {
                Ok(())
            }
            _ => Err(TioError::invalid_argument(
                "Coordinate v2 inline numeric values must match numeric_dtype and must not carry fixed-text/dictionary metadata",
            )),
        },
        CoordinateValueDomainV2::FixedText => match &input.values {
            CoordinateInputValuesV2::FixedText(bytes) => {
                validate_fixed_text_bytes_v2(bytes, input.fixed_text)?;
                if input.dictionary.is_some() {
                    return Err(TioError::invalid_argument(
                        "Coordinate v2 fixed-text descriptors must not carry dictionary metadata",
                    ));
                }
                Ok(())
            }
            _ => Err(TioError::invalid_argument(
                "Coordinate v2 fixed-text descriptors require fixed-text values",
            )),
        },
        CoordinateValueDomainV2::DictionaryCode => {
            validate_dictionary_values_v2(&input.values, input.code_dtype)?;
            validate_dictionary_descriptor_v2(input)
        }
        CoordinateValueDomainV2::AppendSequence => {
            if !matches!(input.values, CoordinateInputValuesV2::None) {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append-sequence descriptors must not carry create-time values",
                ));
            }
            if input.external_binding.is_some() {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append-sequence descriptors must not carry external bindings",
                ));
            }
            if input.fixed_text.width != 0 {
                validate_fixed_text_layout_v2(input.fixed_text)?;
            }
            if input.dictionary.is_some() {
                validate_dictionary_descriptor_v2(input)?;
            }
            Ok(())
        }
        CoordinateValueDomainV2::ExternalReference => {
            let Some(binding) = &input.external_binding else {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 external-reference descriptors require an external binding",
                ));
            };
            if !matches!(input.values, CoordinateInputValuesV2::None) {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 external-reference descriptors must not carry a value buffer",
                ));
            }
            validate_external_binding_v2(binding)?;
            if input.required != binding.required {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 external descriptor required flag must match external binding required flag",
                ));
            }
            match binding.value_domain {
                CoordinateValueDomainV2::InlineNumeric => {
                    if input.fixed_text.width != 0 || input.dictionary.is_some() {
                        return Err(TioError::invalid_argument(
                            "Coordinate v2 numeric external references must not carry fixed-text/dictionary metadata",
                        ));
                    }
                    Ok(())
                }
                CoordinateValueDomainV2::FixedText => {
                    validate_fixed_text_layout_v2(input.fixed_text)
                }
                CoordinateValueDomainV2::DictionaryCode => {
                    if input.fixed_text.width != 0
                        || input.dictionary.is_some()
                        || !input.dictionary_entries.is_empty()
                    {
                        return Err(TioError::invalid_argument(
                            "Coordinate v2 external dictionary-code references persist only code_dtype metadata; dictionary summaries/entries are not accepted",
                        ));
                    }
                    Ok(())
                }
                CoordinateValueDomainV2::AppendSequence => Err(TioError::invalid_argument(
                    "Coordinate v2 append-sequence external references are not supported by the public Rust wrapper",
                )),
                CoordinateValueDomainV2::ExternalReference => Err(TioError::invalid_argument(
                    "Coordinate v2 nested external-reference metadata is not supported",
                )),
            }
        }
    }
}

fn validate_append_entry_v2(entry: &AppendCoordinateEntryV2) -> Result<()> {
    if matches!(entry.descriptor_id.as_deref(), Some("")) {
        return Err(TioError::invalid_argument(
            "Coordinate v2 append descriptor_id cannot be empty",
        ));
    }
    if matches!(entry.name.as_deref(), Some("")) {
        return Err(TioError::invalid_argument(
            "Coordinate v2 append name cannot be empty",
        ));
    }
    match entry.value_domain {
        CoordinateValueDomainV2::InlineNumeric => {
            if !entry.dictionary_entries.is_empty() {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append dictionary-extension entries are only valid for dictionary-code entries",
                ));
            }
            if entry.fixed_text_width != 0 {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append numeric entries must not carry fixed-text width",
                ));
            }
            match (&entry.values, entry.numeric_dtype) {
                (CoordinateInputValuesV2::I32(_), CoordinateDType::I32)
                | (CoordinateInputValuesV2::I64(_), CoordinateDType::I64) => Ok(()),
                _ => Err(TioError::invalid_argument(
                    "Coordinate v2 append numeric values must match numeric_dtype",
                )),
            }
        }
        CoordinateValueDomainV2::FixedText => {
            if !entry.dictionary_entries.is_empty() {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append dictionary-extension entries are only valid for dictionary-code entries",
                ));
            }
            match &entry.values {
                CoordinateInputValuesV2::FixedText(bytes) => {
                    let layout = CoordinateFixedTextLayoutV2::ascii_right_space_padded(
                        entry.fixed_text_width,
                    )?;
                    validate_fixed_text_bytes_v2(bytes, layout)
                }
                _ => Err(TioError::invalid_argument(
                    "Coordinate v2 append fixed-text values are required for fixed-text entries",
                )),
            }
        }
        CoordinateValueDomainV2::DictionaryCode => {
            if entry.fixed_text_width != 0 {
                return Err(TioError::invalid_argument(
                    "Coordinate v2 append dictionary-code entries must not carry fixed-text width",
                ));
            }
            validate_dictionary_values_v2(&entry.values, entry.code_dtype)?;
            for (idx, dictionary_entry) in entry.dictionary_entries.iter().enumerate() {
                validate_dictionary_entry_v2(dictionary_entry, idx)?;
            }
            Ok(())
        }
        CoordinateValueDomainV2::AppendSequence | CoordinateValueDomainV2::ExternalReference => {
            Err(TioError::invalid_argument(
                "Coordinate v2 append entries only carry implemented numeric, fixed-text, or dictionary-code values",
            ))
        }
    }
}

fn copy_coordinate_index_summaries_v2(
    ptr: *mut sys::ArcadiaTioCoordinateIndexSummaryV2,
    len: usize,
) -> Result<Vec<CoordinateIndexSummaryV2>> {
    if ptr.is_null() || len == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: Coordinate v2 index summary array is valid for `len` until the parent metadata is freed.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(CoordinateIndexSummaryV2::from_raw)
        .collect()
}

fn optional_owned_cstring(value: &Option<String>, label: &str) -> Result<Option<CString>> {
    value
        .as_deref()
        .map(|item| string_to_cstring(item, label))
        .transpose()
}

fn opt_cstring_ptr(value: &Option<CString>) -> *const c_char {
    value.as_ref().map_or(ptr::null(), |item| item.as_ptr())
}

fn opt_cstring_mut_ptr(value: &Option<CString>) -> *mut c_char {
    value
        .as_ref()
        .map_or(ptr::null_mut(), |item| item.as_ptr() as *mut c_char)
}

/// RAII TensorFile handle over the native C ABI.
///
/// The wrapper closes the native handle exactly once in `Drop`. It deliberately does not
/// implement `Send` or `Sync` in this first slice because the C ABI handle thread-safety contract
/// is not documented for concurrent mutation.
pub struct TensorFile {
    raw: NonNull<sys::ArcadiaTioHandle>,
    _not_send_or_sync: PhantomData<Rc<()>>,
}

impl TensorFile {
    /// Creates a TensorFile from safe create options.
    pub fn create(path: impl AsRef<Path>, options: CreateOptions) -> Result<Self> {
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_coordinates(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    prepared.coordinate_ptr(),
                    prepared.coordinate_len(),
                ),
                CreateLayout::RandomAccess => {
                    sys::arcadia_tio_create_random_access_with_coordinates(
                        prepared.path.as_ptr(),
                        options.dtype.to_raw(),
                        prepared.dim_kinds.as_ptr(),
                        prepared.dim_lens.as_ptr(),
                        prepared.dim_lens.len(),
                        options.append_dim,
                        prepared.dim_name_ptr(),
                        prepared.dim_name_len(),
                        prepared.symbol_ptr(),
                        prepared.symbol_len(),
                        prepared.channel_ptr(),
                        prepared.channel_len(),
                        prepared.user_key_ptr(),
                        prepared.user_value_ptr(),
                        prepared.user_kv_len(),
                        prepared.coordinate_ptr(),
                        prepared.coordinate_len(),
                    )
                }
            }
        };
        let file = Self::from_raw_handle(raw, "failed to create TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_with_coordinates_v2(path, options, coordinates, coordinate_options)
    }

    /// Creates a TensorFile from Coordinate v2 descriptors while leaving v1 `CoordinateSpec` helpers unchanged.
    pub fn create_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns common create strings/vectors and PreparedAxisCoordinateInputsV2
        // owns Coordinate v2 C strings/dictionary/external helper storage for the duration of this call.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_coordinates_v2(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    prepared_coordinates.ptr(),
                    prepared_coordinates.len(),
                    &raw_coordinate_options,
                ),
                CreateLayout::RandomAccess => {
                    sys::arcadia_tio_create_random_access_with_coordinates_v2(
                        prepared.path.as_ptr(),
                        options.dtype.to_raw(),
                        prepared.dim_kinds.as_ptr(),
                        prepared.dim_lens.as_ptr(),
                        prepared.dim_lens.len(),
                        options.append_dim,
                        prepared.dim_name_ptr(),
                        prepared.dim_name_len(),
                        prepared.symbol_ptr(),
                        prepared.symbol_len(),
                        prepared.channel_ptr(),
                        prepared.channel_len(),
                        prepared.user_key_ptr(),
                        prepared.user_value_ptr(),
                        prepared.user_kv_len(),
                        prepared_coordinates.ptr(),
                        prepared_coordinates.len(),
                        &raw_coordinate_options,
                    )
                }
            }
        };
        let file = Self::from_raw_handle(raw, "failed to create Coordinate v2 TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile with universe-aware axis identity options.
    ///
    /// Coordinate descriptors cannot be combined with universe create options in this wrapper slice
    /// because the current C ABI exposes separate coordinate and universe create families.
    pub fn create_with_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with universe create options yet",
            ));
        }
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
                CreateLayout::RandomAccess => sys::arcadia_tio_create_random_access_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
            }
        };
        let file = Self::from_raw_handle(raw, "failed to create universe-aware TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile using native inferred layout-family selection.
    ///
    /// Inline coordinate descriptors are accepted for fixed non-append axes; external
    /// coordinate storage and append-axis coordinate growth are rejected by the native API.
    pub fn create_inferred(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
    ) -> Result<Self> {
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            sys::arcadia_tio_create_inferred_with_coordinates(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                inferred_options.storage_access.to_raw(),
                inferred_options.open_pattern.to_raw(),
                inferred_options.file_population.to_raw(),
                inferred_options.metadata_stability.to_raw(),
                prepared.coordinate_ptr(),
                prepared.coordinate_len(),
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create inferred TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates an inferred-layout TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_inferred_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_inferred_with_coordinates_v2(
            path,
            options,
            inferred_options,
            coordinates,
            coordinate_options,
        )
    }

    /// Creates an inferred-layout TensorFile from Coordinate v2 descriptors.
    pub fn create_inferred_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate and PreparedAxisCoordinateInputsV2 keep all borrowed raw pointers
        // valid until the C ABI create call returns.
        let raw = unsafe {
            sys::arcadia_tio_create_inferred_with_coordinates_v2(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                inferred_options.storage_access.to_raw(),
                inferred_options.open_pattern.to_raw(),
                inferred_options.file_population.to_raw(),
                inferred_options.metadata_stability.to_raw(),
                prepared_coordinates.ptr(),
                prepared_coordinates.len(),
                &raw_coordinate_options,
            )
        };
        let file =
            Self::from_raw_handle(raw, "failed to create inferred Coordinate v2 TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile using native policy-based chunking.
    ///
    /// Inline coordinate descriptors are accepted for fixed non-append axes; external
    /// coordinate storage and append-axis coordinate growth are rejected by the native API.
    pub fn create_with_policy(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
    ) -> Result<Self> {
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_coordinates(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                prepared.coordinate_ptr(),
                prepared.coordinate_len(),
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create policy TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_with_policy_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_with_policy_with_coordinates_v2(
            path,
            options,
            policy_options,
            coordinates,
            coordinate_options,
        )
    }

    /// Creates a RegularChunked TensorFile from Coordinate v2 descriptors.
    pub fn create_with_policy_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_policy(&options, &policy_options)?;
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate and PreparedAxisCoordinateInputsV2 keep all borrowed raw pointers
        // valid until the C ABI create call returns.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_coordinates_v2(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                prepared_coordinates.ptr(),
                prepared_coordinates.len(),
                &raw_coordinate_options,
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create policy Coordinate v2 TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile with native policy chunking and universe-aware axes.
    ///
    /// Coordinate descriptors cannot be combined with policy+universe create in this wrapper slice
    /// because the current C ABI exposes no policy+universe+coordinate create family.
    pub fn create_with_policy_and_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with policy universe create options yet",
            ));
        }
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_universe_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_universe(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                &raw_universe_options,
            )
        };
        let file = Self::from_raw_handle(raw, "failed to create policy universe TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Set write-time compression for future appends on this handle.
    pub fn set_compression(&self, compression: CompressionConfig) -> Result<()> {
        let raw = compression.validate()?.to_raw();
        let status = unsafe { sys::arcadia_tio_set_compression_config(self.raw.as_ptr(), &raw) };
        status_result(status, "failed to set compression config")
    }

    /// Opens an existing TensorFile.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path_to_cstring(path)?;
        // SAFETY: The C string is valid for the duration of this call.
        let raw = unsafe { sys::arcadia_tio_open(path.as_ptr()) };
        Self::from_raw_handle(raw, "failed to open TensorFile")
    }

    /// Loads metadata without keeping a TensorFile handle open.
    pub fn load_meta(path: impl AsRef<Path>) -> Result<FileMeta> {
        let path = path_to_cstring(path)?;
        let mut raw = MaybeUninit::<sys::ArcadiaTioFileMeta>::uninit();
        // SAFETY: `raw` points to valid uninitialized storage for the C ABI to fill.
        let status = unsafe { sys::arcadia_tio_load_meta(path.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to load TensorFile metadata")?;
        // SAFETY: Successful status initializes `raw`.
        let mut raw = unsafe { raw.assume_init() };
        let meta = copy_file_meta(&raw);
        // SAFETY: `raw` contains native-owned buffers returned by load_meta and is freed exactly once.
        unsafe { sys::arcadia_tio_file_meta_free(&mut raw) };
        meta
    }

    /// Loads coordinate metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_meta(path: impl AsRef<Path>) -> Result<Vec<CoordinateMeta>> {
        let path = path_to_cstring(path)?;
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMeta = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: The path C string and out pointers are valid for the duration of this call.
        let status = unsafe {
            sys::arcadia_tio_load_coordinate_meta(path.as_ptr(), &mut raw_meta, &mut len)
        };
        status_result(status, "failed to load coordinate metadata")?;
        let out = copy_coordinate_meta(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output from load_coordinate_meta and freed once.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_free(raw_meta, len) };
        out
    }

    /// Loads current coordinate metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_metadata(path: impl AsRef<Path>) -> Result<Vec<AxisCoordinateMeta>> {
        Self::load_coordinate_meta_v2(path)
    }

    /// Loads Coordinate v2 metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_meta_v2(path: impl AsRef<Path>) -> Result<Vec<AxisCoordinateMetaV2>> {
        let path = path_to_cstring(path)?;
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMetaV2 = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: The path C string and out pointers are valid for the duration of this call.
        let status = unsafe {
            sys::arcadia_tio_load_coordinate_meta_v2(path.as_ptr(), &mut raw_meta, &mut len)
        };
        status_result(status, "failed to load Coordinate v2 metadata")?;
        let out = copy_coordinate_meta_v2(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output and are freed exactly once after copying.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_v2_free(raw_meta, len) };
        out
    }

    /// Returns the native C ABI version reported by the linked library.
    pub fn native_abi_version() -> u32 {
        // SAFETY: Version query has no preconditions.
        unsafe { sys::arcadia_tio_abi_version() }
    }

    /// Returns the tensor rank.
    pub fn rank(&self) -> Result<usize> {
        let mut rank = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_rank(self.raw.as_ptr(), &mut rank) };
        status_result(status, "failed to read TensorFile rank")?;
        Ok(rank)
    }

    /// Returns the payload dtype.
    pub fn dtype(&self) -> Result<DType> {
        let mut dtype = sys::ARCADIA_TIO_DTYPE_F32;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_dtype(self.raw.as_ptr(), &mut dtype) };
        status_result(status, "failed to read TensorFile dtype")?;
        DType::from_raw(dtype)
    }

    /// Returns the append-axis index.
    pub fn append_axis(&self) -> Result<usize> {
        let mut axis = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_append_axis(self.raw.as_ptr(), &mut axis) };
        status_result(status, "failed to read TensorFile append axis")?;
        Ok(axis)
    }

    /// Returns the current dimension lengths.
    pub fn dim_lens(&self) -> Result<Vec<u32>> {
        let rank = self.rank()?;
        let mut dims = vec![0u32; rank];
        // SAFETY: `dims` has exactly `rank` writable elements and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_dim_lens(self.raw.as_ptr(), dims.as_mut_ptr(), dims.len()) };
        status_result(status, "failed to read TensorFile dimension lengths")?;
        Ok(dims)
    }

    /// Returns the native index-checkpoint interval in commits.
    pub fn index_checkpoint_every_commits(&self) -> Result<u32> {
        let mut every_commits = 0u32;
        // SAFETY: `every_commits` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_index_checkpoint_every_commits(
                self.raw.as_ptr(),
                &mut every_commits,
            )
        };
        status_result(status, "failed to read index checkpoint interval")?;
        Ok(every_commits)
    }

    /// Updates the native index-checkpoint interval in commits.
    ///
    /// The interval must be at least one. Native implementations that do not support this
    /// metadata update return an ordinary wrapper error without changing the file.
    pub fn set_index_checkpoint_every_commits(&mut self, every_commits: u32) -> Result<()> {
        if every_commits == 0 {
            return Err(TioError::invalid_argument(
                "index checkpoint interval must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe {
            sys::arcadia_tio_set_index_checkpoint_every_commits(self.raw.as_ptr(), every_commits)
        };
        status_result(status, "failed to set index checkpoint interval")
    }

    /// Returns the native chunking plan copied into Rust-owned memory.
    pub fn chunk_plan(&self) -> Result<ChunkPlan> {
        let mut raw_plan = NativeChunkPlan::new();
        // SAFETY: `raw_plan` is a valid output pointer and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_chunk_plan(self.raw.as_ptr(), raw_plan.as_mut_ptr()) };
        status_result(status, "failed to read chunk plan")?;
        copy_chunk_plan(raw_plan.as_ref())
    }

    /// Updates or clears one dimension name through the native metadata administration API.
    ///
    /// Passing `None` clears the name. Native implementations that do not support metadata-only
    /// updates return an ordinary wrapper error without changing the file.
    pub fn set_dim_name(&mut self, axis: usize, name: Option<&str>) -> Result<()> {
        if matches!(name, Some("")) {
            return Err(TioError::invalid_argument("dimension name cannot be empty"));
        }
        let name = name
            .map(|value| string_to_cstring(value, "dimension name"))
            .transpose()?;
        let (ptr, has_name) = match name.as_ref() {
            Some(value) => (value.as_ptr(), 1),
            None => (ptr::null(), 0),
        };
        // SAFETY: Optional name CString, when present, outlives the call and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_set_dim_name(self.raw.as_ptr(), axis, ptr, has_name) };
        status_result(status, "failed to set dimension name")
    }

    /// Replaces Symbol-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_symbols<S: AsRef<str>>(&mut self, symbols: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(symbols, "symbol label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_symbols(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set symbol labels")
    }

    /// Replaces Channel-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_channels<S: AsRef<str>>(&mut self, channels: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(channels, "channel label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_channels(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set channel labels")
    }

    /// Replaces user key/value metadata through the native metadata administration API.
    ///
    /// Passing an empty slice requests clearing all user metadata. Native implementations that do
    /// not support metadata-only updates return an ordinary wrapper error without changing the file.
    pub fn set_user_kv<K, V>(&mut self, user_kv: &[(K, V)]) -> Result<()>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let prepared = PreparedUserKvList::new(user_kv)?;
        // SAFETY: Prepared key/value C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_user_kv(
                self.raw.as_ptr(),
                prepared.key_ptr(),
                prepared.value_ptr(),
                prepared.len(),
            )
        };
        status_result(status, "failed to set user metadata")
    }

    /// Returns the native path snapshot for this handle.
    pub fn path(&self) -> Result<String> {
        let mut raw_path: *mut c_char = ptr::null_mut();
        // SAFETY: `raw_path` is a valid out pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_path(self.raw.as_ptr(), &mut raw_path) };
        status_result(status, "failed to read TensorFile path")?;
        let value = required_c_string(raw_path.cast_const());
        // SAFETY: `raw_path` is native-owned output from arcadia_tio_path.
        unsafe { sys::arcadia_tio_string_free(raw_path) };
        Ok(value)
    }

    /// Reads coordinate metadata from the open handle.
    pub fn coordinate_meta(&self) -> Result<Vec<CoordinateMeta>> {
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMeta = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: Out pointers are valid and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_coordinate_meta(self.raw.as_ptr(), &mut raw_meta, &mut len) };
        status_result(status, "failed to read coordinate metadata")?;
        let out = copy_coordinate_meta(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output from coordinate_meta and freed once.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_free(raw_meta, len) };
        out
    }

    /// Reads current coordinate metadata from the open handle.
    pub fn coordinate_metadata(&self) -> Result<Vec<AxisCoordinateMeta>> {
        self.coordinate_meta_v2()
    }

    /// Reads Coordinate v2 metadata from the open handle.
    pub fn coordinate_meta_v2(&self) -> Result<Vec<AxisCoordinateMetaV2>> {
        let mut raw_meta: *mut sys::ArcadiaTioAxisCoordinateMetaV2 = ptr::null_mut();
        let mut len = 0usize;
        // SAFETY: Out pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_coordinate_meta_v2(self.raw.as_ptr(), &mut raw_meta, &mut len)
        };
        status_result(status, "failed to read Coordinate v2 metadata")?;
        let out = copy_coordinate_meta_v2(raw_meta, len);
        // SAFETY: `raw_meta`/`len` are native-owned output and are freed exactly once after copying.
        unsafe { sys::arcadia_tio_axis_coordinate_meta_v2_free(raw_meta, len) };
        out
    }

    /// Analyzes how a sparse-intent f32 append would be handled by the native writer.
    pub fn analyze_sparse_append_f32(
        &self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f32(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent f64 append would be handled by the native writer.
    pub fn analyze_sparse_append_f64(
        &self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f64(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i32 append would be handled by the native writer.
    pub fn analyze_sparse_append_i32(
        &self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append_v2(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i32_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i64 append would be handled by the native writer.
    pub fn analyze_sparse_append_i64(
        &self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append_v2(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i64_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f32(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F32, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f32(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f32_with_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f32_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f32_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f32`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f32_returning_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f32_with_range(data, shape, rule)
    }

    /// Appends f64 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f64(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F64, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f64(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f64_with_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f64_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f64_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f64`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f64_returning_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f64_with_range(data, shape, rule)
    }

    /// Appends i32 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_i32(
        &mut self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range_v2(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i32_with_range_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends i64 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_i64(
        &mut self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range_v2(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i64_with_range_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends a bulk f32 slice and returns the assigned append-entry range.
    pub fn append_f32(&mut self, data: &[f32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice and returns the assigned append-entry range.
    pub fn append_f64(&mut self, data: &[f64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice and returns the assigned append-entry range.
    pub fn append_i32(&mut self, data: &[i32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice and returns the assigned append-entry range.
    pub fn append_i64(&mut self, data: &[i64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f32 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_f32_with_coordinates(
        &mut self,
        data: &[f32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_f32_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk f32 slice with Coordinate v2 append-axis values and returns the assigned range.
    ///
    /// Coordinate semantic validation (missing required values, wrong counts, descriptor/domain
    /// mismatches, dictionary/fixed-text conflicts, and publication conflicts) is delegated to the
    /// raw Coordinate v2 append call so native last-error details are preserved. The wrapper prepares
    /// borrowed coordinate buffers only for this synchronous call and never falls back to a payload-only
    /// append, preserving raw no-partial-publication semantics on failure.
    pub fn append_f32_with_coordinates_v2(
        &mut self,
        data: &[f32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_f64_with_coordinates(
        &mut self,
        data: &[f64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_f64_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk f64 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_f64_with_coordinates_v2(
        &mut self,
        data: &[f64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_i32_with_coordinates(
        &mut self,
        data: &[i32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_i32_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk i32 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_i32_with_coordinates_v2(
        &mut self,
        data: &[i32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_i64_with_coordinates(
        &mut self,
        data: &[i64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_i64_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk i64 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_i64_with_coordinates_v2(
        &mut self,
        data: &[i64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f32 slice with universe bindings and returns the assigned entry range.
    pub fn append_f32_with_universe(
        &mut self,
        data: &[f32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice with universe bindings and returns the assigned entry range.
    pub fn append_f64_with_universe(
        &mut self,
        data: &[f64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice with universe bindings and returns the assigned entry range.
    pub fn append_i32_with_universe(
        &mut self,
        data: &[i32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice with universe bindings and returns the assigned entry range.
    pub fn append_i64_with_universe(
        &mut self,
        data: &[i64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Rewrites a single native entry selector with f32 payload data.
    pub fn rewrite_f32(
        &mut self,
        selector: EntrySelector,
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f32(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 data")
    }

    /// Rewrites a single native entry selector with f64 payload data.
    pub fn rewrite_f64(
        &mut self,
        selector: EntrySelector,
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f64(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 data")
    }

    /// Rewrites a selector slice with f32 payload data.
    pub fn rewrite_slice_f32(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f32(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 selector slice")
    }

    /// Rewrites a selector slice with f64 payload data.
    pub fn rewrite_slice_f64(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f64(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 selector slice")
    }

    /// Clears storage blocks identified by chunk keys.
    pub fn clear_blocks(&mut self, keys: &[ChunkKey]) -> Result<()> {
        let prepared_keys = PreparedChunkKeys::new(keys);
        // SAFETY: Prepared chunk-key buffers and their borrowed coordinate slices outlive the call.
        let status = unsafe {
            sys::arcadia_tio_clear_blocks(
                self.raw.as_ptr(),
                prepared_keys.ptr(),
                prepared_keys.len(),
            )
        };
        status_result(status, "failed to clear blocks")
    }

    /// Returns metadata for the current visible head commit.
    pub fn head_commit(&self) -> Result<CommitInfo> {
        let mut raw = MaybeUninit::<sys::ArcadiaTioCommitInfo>::uninit();
        // SAFETY: `raw` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_head_commit(self.raw.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to read head commit")?;
        // SAFETY: Successful native call initialized the output commit.
        Ok(unsafe { raw.assume_init() }.into())
    }

    /// Lists visible commits in native order.
    ///
    /// A `limit` of `None` requests the native full visible list; `Some(0)` is rejected because the
    /// underlying C ABI uses zero as the unbounded sentinel.
    pub fn list_commits(&self, limit: Option<u32>) -> Result<Vec<CommitInfo>> {
        let raw_limit = match limit {
            Some(0) => {
                return Err(TioError::invalid_argument(
                    "commit list limit must be non-zero; use None for the full list",
                ));
            }
            Some(value) => value,
            None => 0,
        };
        let mut raw_list = NativeCommitList::new();
        // SAFETY: `raw_list` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_list_commits(self.raw.as_ptr(), raw_limit, raw_list.as_mut_ptr())
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(TioError::from_last_error("failed to list commits"));
        }
        copy_commit_list(raw_list.as_ref())
    }

    /// Removes the current visible head commit.
    ///
    /// This mutates the open file in place and delegates all retention/underflow validation to the
    /// native history implementation.
    pub fn pop(&mut self) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop(self.raw.as_ptr()) };
        status_result(status, "failed to pop head commit")
    }

    /// Removes up to `n` visible head commits.
    ///
    /// This mutates the open file in place. Passing `0` is rejected by the safe wrapper because it
    /// cannot change history and is usually a caller bug.
    pub fn pop_batched(&mut self, n: u32) -> Result<()> {
        if n == 0 {
            return Err(TioError::invalid_argument(
                "pop_batched count must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop_batched(self.raw.as_ptr(), n) };
        status_result(status, "failed to pop batched commits")
    }

    /// Reverts the file to a visible target commit sequence.
    ///
    /// This mutates the open file in place and preserves native semantics for invalid or retained
    /// history targets.
    pub fn revert_commit(&mut self, target_commit_seq: u64) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status =
            unsafe { sys::arcadia_tio_revert_commit(self.raw.as_ptr(), target_commit_seq) };
        status_result(status, "failed to revert commit")
    }

    /// Returns shallow compatibility compaction statistics.
    pub fn analyze_compaction(&self) -> Result<CompactionStats> {
        let mut stats = sys::ArcadiaTioCompactionStats {
            live_bytes: 0,
            dead_bytes: 0,
            dead_ratio: 0.0,
            commit_count: 0,
        };
        // SAFETY: `stats` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_analyze_compaction(self.raw.as_ptr(), &mut stats) };
        status_result(status, "failed to analyze compaction")?;
        Ok(CompactionStats {
            live_bytes: stats.live_bytes,
            dead_bytes: stats.dead_bytes,
            dead_ratio: stats.dead_ratio,
            commit_count: stats.commit_count,
        })
    }

    /// Returns non-precise V4 source-file diagnostics.
    pub fn v4_diagnostics(&self) -> Result<V4DiagnosticsReport> {
        let mut report = new_v4_diagnostics_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status = unsafe { sys::arcadia_tio_v4_diagnostics(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to get V4 diagnostics"));
        }
        let copied = copy_v4_diagnostics_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
        Ok(copied)
    }

    /// Returns precise V4 source-file diagnostics with validity metadata.
    pub fn v4_diagnostics_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4DiagnosticsPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_diagnostics_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_v4_diagnostics_precise(self.raw.as_ptr(), &raw_options, &mut report)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to get precise V4 diagnostics",
            ));
        }
        let copied = copy_v4_diagnostics_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
        Ok(copied)
    }

    /// Returns non-precise V4 current-state compaction analysis.
    pub fn analyze_v4_compaction(&self) -> Result<V4CompactionAnalysisReport> {
        let mut report = new_v4_compaction_analysis_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_analyze_v4_compaction(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to analyze V4 compaction"));
        }
        let copied = copy_v4_compaction_analysis_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
        copied
    }

    /// Returns precise V4 current-state compaction analysis with validity metadata.
    pub fn analyze_v4_compaction_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4CompactionAnalysisPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_compaction_analysis_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_analyze_v4_compaction_precise(
                self.raw.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to analyze precise V4 compaction",
            ));
        }
        let copied = copy_v4_compaction_analysis_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
        copied
    }

    /// Compacts live chunks into a destination file.
    pub fn compact_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        // SAFETY: Destination path C string and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.retain_commits,
                options.mode.to_raw(),
            )
        };
        status_result(status, "failed to compact TensorFile")
    }

    /// Conditionally compacts live chunks into a destination file.
    pub fn maybe_compact(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<bool> {
        let dst_path = path_to_cstring(dst_path)?;
        let mut compacted = 0u8;
        // SAFETY: Destination path C string, output flag, and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_maybe_compact(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.dead_ratio_threshold,
                options.min_dead_bytes,
                options.retain_commits,
                options.mode.to_raw(),
                &mut compacted,
            )
        };
        status_result(status, "failed to maybe compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        self.get_auto_compaction_config()
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn get_auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        let mut config = new_auto_compaction_config();
        let mut has_config = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_auto_compaction_config(
                self.raw.as_ptr(),
                &mut config,
                &mut has_config,
            )
        };
        status_result(status, "failed to get auto-compaction config")?;
        if has_config == 0 {
            Ok(None)
        } else {
            copy_auto_compaction_config(config).map(Some)
        }
    }

    /// Updates or clears auto-compaction metadata configuration.
    pub fn set_auto_compaction_config(
        &mut self,
        config: Option<AutoCompactionConfig>,
    ) -> Result<()> {
        let raw = config.map(|cfg| cfg.to_raw());
        let (ptr, has_config) = match raw.as_ref() {
            Some(cfg) => (cfg as *const sys::ArcadiaTioAutoCompactionConfig, 1u8),
            None => (ptr::null(), 0u8),
        };
        // SAFETY: Optional config pointer is either null or points to a local value valid for this call.
        let status = unsafe {
            sys::arcadia_tio_set_auto_compaction_config(self.raw.as_ptr(), ptr, has_config)
        };
        status_result(status, "failed to set auto-compaction config")
    }

    /// Clears auto-compaction metadata configuration.
    pub fn clear_auto_compaction(&mut self) -> Result<()> {
        self.set_auto_compaction_config(None)
    }

    /// Reads auto-compaction state metadata, if present.
    pub fn compaction_state(&self) -> Result<Option<CompactionState>> {
        let mut state = sys::ArcadiaTioCompactionState {
            last_compacted_commit_seq: 0,
            last_compacted_at_unix_ms: 0,
        };
        let mut has_state = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_compaction_state(self.raw.as_ptr(), &mut state, &mut has_state)
        };
        status_result(status, "failed to read compaction state")?;
        if has_state == 0 {
            Ok(None)
        } else {
            Ok(Some(CompactionState {
                last_compacted_commit_seq: state.last_compacted_commit_seq,
                last_compacted_at_unix_ms: state.last_compacted_at_unix_ms,
            }))
        }
    }

    /// Runs metadata-configured auto-compaction if native thresholds trigger.
    pub fn maybe_compact_auto(&mut self) -> Result<bool> {
        let mut compacted = 0u8;
        // SAFETY: Output flag is valid and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_maybe_compact_auto(self.raw.as_ptr(), &mut compacted) };
        status_result(status, "failed to maybe auto-compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Compacts a V4 file into a retained-history destination.
    pub fn compact_v4_retained_history_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: V4RetainedHistoryCompactionOptions,
    ) -> Result<V4RetainedHistoryCompactionReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_v4_retained_history_compaction_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history",
            ));
        }
        let copied = copy_v4_retained_history_compaction_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
        copied
    }

    /// Compacts a V4 file into a retained-history destination with precise source accounting.
    pub fn compact_v4_retained_history_to_precise(
        &mut self,
        dst_path: impl AsRef<Path>,
        retention_options: V4RetainedHistoryCompactionOptions,
        precise_options: V4PreciseAccountingOptions,
    ) -> Result<V4RetainedHistoryCompactionPreciseReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_retention_options = retention_options.to_raw();
        let raw_precise_options = precise_options.to_raw();
        let mut report = new_v4_retained_history_compaction_precise_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to_precise(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_retention_options,
                &raw_precise_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report)
            };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history with precise accounting",
            ));
        }
        let copied = copy_v4_retained_history_compaction_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report) };
        copied
    }

    /// Reforms visible data into a destination file with an explicit target layout.
    pub fn reform_to(&mut self, dst_path: impl AsRef<Path>, options: ReformOptions) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        // SAFETY: Inputs are valid for the duration of the FFI call.
        let status = unsafe {
            sys::arcadia_tio_reform_to(self.raw.as_ptr(), dst_path.as_ptr(), &raw_options)
        };
        status_result(status, "failed to reform TensorFile")
    }

    /// Reforms visible data into a destination file and returns native diagnostic metadata.
    pub fn reform_to_ex(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: ReformOptions,
    ) -> Result<ReformReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_reform_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_reform_to_ex(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            let copied = copy_reform_report(&report);
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
            return Err(
                TioError::from_last_error("failed to reform TensorFile with report")
                    .with_reform_report(&copied),
            );
        }
        let copied = copy_reform_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
        Ok(copied)
    }

    /// Reads the full tensor into Rust-owned buffers.
    pub fn read_all(&self) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe { sys::arcadia_tio_read_all(handle, out) })
    }

    /// Exports full tensor values through the Arrow C Data Interface.
    ///
    /// The returned [`ArrowCData`] owns the Arrow `release` callbacks and invokes them on drop.
    /// Borrowed C Data pointers are valid only while the returned value is alive.
    pub fn read_values_arrow(&self) -> Result<ArrowCData> {
        // SAFETY: All-zero Arrow C Data carriers represent empty caller-owned output slots with
        // null release callbacks before the native function writes initialized values.
        let mut raw_array: sys::ArrowArray = unsafe { mem::zeroed() };
        // SAFETY: See `raw_array` initialization above.
        let mut raw_schema: sys::ArrowSchema = unsafe { mem::zeroed() };
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_values_arrow(self.raw.as_ptr(), &mut raw_array, &mut raw_schema)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Defensive cleanup for any partially initialized Arrow carriers.
            unsafe {
                release_arrow_array(&mut raw_array);
                release_arrow_schema(&mut raw_schema);
            }
            return Err(TioError::from_last_error(
                "failed to export tensor values as Arrow C Data",
            ));
        }
        Ok(ArrowCData {
            array: raw_array,
            schema: raw_schema,
            _not_send_or_sync: PhantomData,
        })
    }

    /// Reads the full tensor densely with a fill value and optional validity mask.
    pub fn read_all_dense(&self, fill_value: f64) -> Result<DenseTensor> {
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_all_dense(
                self.raw.as_ptr(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
            )
        };
        status_result(status, "failed to read dense tensor")?;
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        // SAFETY: Native-owned buffers are returned by read_all_dense and freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
        }
        Ok(DenseTensor {
            tensor: tensor?,
            mask,
        })
    }

    /// Reads current data through the native basic read-index lowering API.
    pub fn read_index(&self, items: &[ReadIndexItem]) -> Result<ReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_report = new_read_index_report();
        // SAFETY: Prepared read-index items outlive the call; outputs are initialized and valid.
        let status = unsafe {
            sys::arcadia_tio_read_index(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                &mut raw_tensor,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error("failed to read with read_index"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexResult {
            value: tensor?,
            report: report?,
        })
    }

    /// Reads current data through the native basic read-index API with dense fill materialization.
    pub fn read_index_dense(
        &self,
        items: &[ReadIndexItem],
        fill_value: f64,
    ) -> Result<ReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut raw_report = new_read_index_report();
        // SAFETY: Prepared read-index items outlive the call; outputs are initialized and valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_dense(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            report: report?,
        })
    }

    /// Reads an axis range into Rust-owned buffers.
    pub fn read_axis_range(&self, axis: usize, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "axis range start must be <= end",
            ));
        }
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_range(handle, axis, start, end, out)
        })
    }

    /// Reads an axis take selection into Rust-owned buffers.
    pub fn read_axis_take(&self, axis: usize, indices: &[u32]) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_take(handle, axis, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads an append-entry range into Rust-owned buffers.
    pub fn read_entry_range(&self, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "entry range start must be <= end",
            ));
        }
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_entry_range(handle, start, end, out)
        })
    }

    /// Reads selected append entries into Rust-owned buffers.
    pub fn take_entries(&self, indices: &[u32]) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_take_entries(handle, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads inline coordinate values for an axis into Rust-owned buffers.
    ///
    /// This is metadata-scope coordinate value access, not native exact/range coordinate lookup.
    pub fn read_axis_coordinates(&self, axis: usize) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_coordinates(handle, axis, out)
        })
    }

    /// Reads current coordinate axis values into Rust-owned bytes while preserving status fields.
    pub fn read_coordinate_axis(
        &self,
        axis: usize,
        options: CoordinateOptions,
    ) -> Result<CoordinateValueSlice> {
        self.read_axis_coordinates_v2(axis, options)
    }

    /// Reads Coordinate v2 axis values into Rust-owned bytes while preserving status fields.
    pub fn read_axis_coordinates_v2(
        &self,
        axis: usize,
        options: CoordinateV2Options,
    ) -> Result<CoordinateValueSliceV2> {
        self.validate_axis(axis)?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateValueSliceV2::default();
        // SAFETY: `self.raw` is live, `raw_options` and `raw` are valid for the duration of the call.
        let status = unsafe {
            sys::arcadia_tio_read_axis_coordinates_v2(
                self.raw.as_ptr(),
                axis,
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to read Coordinate v2 axis values") {
            // SAFETY: The raw value carrier is either empty/default or native-owned partial output;
            // the paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_value_slice_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies data before free.
        let out = unsafe { CoordinateValueSliceV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_value_slice_v2_free(&mut raw) };
        out
    }

    /// Reads current coordinate dictionary metadata/entries into Rust-owned values.
    pub fn coordinate_dictionary(
        &self,
        axis: usize,
        options: CoordinateOptions,
    ) -> Result<CoordinateDictionary> {
        self.coordinate_dictionary_v2(axis, options)
    }

    /// Reads Coordinate v2 dictionary metadata/entries into Rust-owned values.
    pub fn coordinate_dictionary_v2(
        &self,
        axis: usize,
        options: CoordinateV2Options,
    ) -> Result<CoordinateDictionaryV2> {
        self.validate_axis(axis)?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateDictionaryV2::default();
        // SAFETY: `self.raw` is live, `raw_options` and `raw` are valid for the duration of the call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_dictionary_v2(
                self.raw.as_ptr(),
                axis,
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to read Coordinate v2 dictionary") {
            // SAFETY: The raw dictionary is either empty/default or native-owned partial output;
            // the paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_dictionary_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies data before free.
        let out = unsafe { CoordinateDictionaryV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_dictionary_v2_free(&mut raw) };
        out
    }

    /// Performs an exact coordinate lookup using a typed key.
    pub fn coordinate_lookup(
        &self,
        axis: usize,
        key: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_v2(axis, key, options)
    }

    /// Performs an exact Coordinate v2 lookup using a typed key.
    ///
    /// Transport/API misuse is returned as `Err(TioError)`. Ordinary Coordinate v2 outcomes such
    /// as missing, unavailable, duplicate, unsupported, invalid/stale index, or domain mismatch are
    /// preserved in the returned [`CoordinateLookupResultV2`]. Optional indexes are acceleration
    /// metadata only; pass [`CoordinateV2Options::authoritative_scan`] when callers explicitly allow
    /// fallback to authoritative selected-root coordinate values.
    pub fn coordinate_lookup_v2(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_key = key.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. `prepared_key`, `raw_options`, and `raw` remain valid for
        // the duration of the call and the raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_v2(
                self.raw.as_ptr(),
                axis,
                prepared_key.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to perform Coordinate v2 exact lookup") {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs an exact coordinate lookup against a retained historical commit.
    pub fn coordinate_lookup_at_commit(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, options)
    }

    /// Performs an exact Coordinate v2 lookup against a retained historical commit.
    ///
    /// The lookup binds Coordinate v2 values, dictionaries, and append-coordinate chunks to the
    /// target commit before evaluating the key. Result status semantics match
    /// [`Self::coordinate_lookup_v2`].
    pub fn coordinate_lookup_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_key = key.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. `prepared_key`, `raw_options`, and `raw` remain valid for
        // the duration of the call and the raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_at_commit_v2(
                self.raw.as_ptr(),
                commit_seq,
                axis,
                prepared_key.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(
            status,
            "failed to perform historical Coordinate v2 exact lookup",
        ) {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs a half-open coordinate range lookup using typed lower/upper keys.
    pub fn coordinate_lookup_range(
        &self,
        axis: usize,
        lower: &CoordinateLookupKey,
        upper: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_range_v2(axis, lower, upper, options)
    }

    /// Performs a half-open Coordinate v2 range lookup using typed lower/upper keys.
    ///
    /// Status-rich raw lookup outcomes are returned as [`CoordinateLookupResultV2`] instead of
    /// being collapsed into opaque errors. Optional indexes remain non-authoritative; callers must
    /// opt into authoritative scans through [`CoordinateV2Options`].
    pub fn coordinate_lookup_range_v2(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_lower = lower.prepare()?;
        let prepared_upper = upper.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. Prepared keys/options/output outlive the FFI call and the
        // raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_range_v2(
                self.raw.as_ptr(),
                axis,
                prepared_lower.raw(),
                prepared_upper.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to perform Coordinate v2 range lookup") {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs a half-open coordinate range lookup against a retained historical commit.
    pub fn coordinate_lookup_range_at_commit(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKey,
        upper: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_range_at_commit_v2(commit_seq, axis, lower, upper, options)
    }

    /// Performs a half-open Coordinate v2 range lookup against a retained historical commit.
    ///
    /// The target commit is bound before Coordinate v2 lower-inclusive, upper-exclusive range
    /// semantics are evaluated. Result status semantics match
    /// [`Self::coordinate_lookup_range_v2`].
    pub fn coordinate_lookup_range_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_lower = lower.prepare()?;
        let prepared_upper = upper.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. Prepared keys/options/output outlive the FFI call and the
        // raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_range_at_commit_v2(
                self.raw.as_ptr(),
                commit_seq,
                axis,
                prepared_lower.raw(),
                prepared_upper.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(
            status,
            "failed to perform historical Coordinate v2 range lookup",
        ) {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Looks up the unique axis index for an inline validated i32 coordinate value.
    pub fn coordinate_index_i32(&self, axis: usize, value: i32) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i32(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i32 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the unique axis index for an inline validated i64 coordinate value.
    pub fn coordinate_index_i64(&self, axis: usize, value: i64) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i64(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i64 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i32 coordinate interval.
    pub fn coordinate_range_i32(
        &self,
        axis: usize,
        start: i32,
        end: i32,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i32(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i32 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i64 coordinate interval.
    pub fn coordinate_range_i64(
        &self,
        axis: usize,
        start: i64,
        end: i64,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i64(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i64 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Reads the one-position axis slice for an inline validated i32 coordinate value.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_index_i32`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_at_coordinate_i32(&self, axis: usize, value: i32) -> Result<Tensor> {
        let index = self.coordinate_index_i32(axis, value)?;
        let end = index.checked_add(1).ok_or_else(|| {
            TioError::invalid_argument("coordinate index cannot be converted to a one-item range")
        })?;
        self.read_axis_range(axis, index, end)
    }

    /// Reads the one-position axis slice for an inline validated i64 coordinate value.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_index_i64`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_at_coordinate_i64(&self, axis: usize, value: i64) -> Result<Tensor> {
        let index = self.coordinate_index_i64(axis, value)?;
        let end = index.checked_add(1).ok_or_else(|| {
            TioError::invalid_argument("coordinate index cannot be converted to a one-item range")
        })?;
        self.read_axis_range(axis, index, end)
    }

    /// Reads the axis slice overlapping an inclusive i32 coordinate interval.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_range_i32`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_coordinate_range_i32(&self, axis: usize, start: i32, end: i32) -> Result<Tensor> {
        let range = self.coordinate_range_i32(axis, start, end)?;
        self.read_axis_range(axis, range.start, range.end)
    }

    /// Reads the axis slice overlapping an inclusive i64 coordinate interval.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_range_i64`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_coordinate_range_i64(&self, axis: usize, start: i64, end: i64) -> Result<Tensor> {
        let range = self.coordinate_range_i64(axis, start, end)?;
        self.read_axis_range(axis, range.start, range.end)
    }

    /// Reads current selector data with execution options and metadata.
    pub fn read_with_options(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error("failed to read with options"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with execution options and metadata.
    pub fn read_with_options_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads current selector data with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by
    /// itself.
    pub fn read_with_options_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
    ) -> Result<AttributedReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                &mut raw_tensor,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: tensor?,
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data densely with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options_dense` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by itself.
    pub fn read_with_options_dense_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
        fill_value: f64,
    ) -> Result<AttributedReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data with a shape policy and execution metadata.
    pub fn read_with_shape_policy(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with a shape policy and execution metadata.
    pub fn read_with_shape_policy_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit into Rust-owned buffers.
    pub fn read_at_commit(&self, commit_seq: u64, selectors: &[EntrySelector]) -> Result<Tensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_at_commit(
                handle,
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                out,
            )
        })
    }

    /// Reads selector data at a retained commit densely with a fill value.
    pub fn read_at_commit_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        fill_value: f64,
    ) -> Result<DenseTensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        // SAFETY: Prepared selector buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
            )
        };
        status_result(status, "failed to read dense tensor at commit")?;
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
        }
        Ok(DenseTensor {
            tensor: tensor?,
            mask,
        })
    }

    /// Reads selector data at a retained commit with execution options and metadata.
    pub fn read_at_commit_with_options(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with execution options and metadata.
    pub fn read_at_commit_with_options_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    /// Reads retained historical data through the native basic read-index lowering API.
    pub fn read_index_at_commit_with_options(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithOptions,
    ) -> Result<HistoricalReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_options(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexResult {
            value: tensor?,
            report: copied_report?,
        })
    }

    /// Reads retained historical data through the native basic read-index API with dense fill materialization.
    pub fn read_index_at_commit_with_options_dense(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_options_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            report: copied_report?,
        })
    }

    /// Reads selector data at a retained commit with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask,
            },
            execution: execution?,
        })
    }

    fn append_with_range(
        &mut self,
        shape: &[u64],
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut u32, *mut u32) -> i32,
    ) -> Result<AppendRange> {
        let mut start = 0u32;
        let mut end = 0u32;
        let status = call(self.raw.as_ptr(), &mut start, &mut end);
        status_result(status, "failed to append tensor data")?;
        let _ = shape;
        Ok(AppendRange { start, end })
    }

    fn analyze_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut sys::ArcadiaTioSparseAppendAnalysis,
        ) -> i32,
    ) -> Result<SparseAppendAnalysis> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let mut raw_analysis = empty_sparse_append_analysis();
        let status = call(self.raw.as_ptr(), &raw_rule, &mut raw_analysis);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: `raw_analysis` was initialized to an empty native-compatible value before the
            // call. If native populated reasons before returning an error, this releases them once.
            unsafe { sys::arcadia_tio_sparse_append_analysis_free(&mut raw_analysis) };
            return Err(TioError::from_last_error("failed to analyze sparse append"));
        }
        take_sparse_append_analysis(&mut raw_analysis)
    }

    fn append_sparse(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *const sys::ArcadiaTioSparseRule) -> i32,
    ) -> Result<()> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let status = call(self.raw.as_ptr(), &raw_rule);
        status_result(status, "failed to append sparse tensor data")
    }

    fn append_sparse_with_range(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut u32,
            *mut u32,
        ) -> i32,
    ) -> Result<AppendRange> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        self.append_with_range(shape, |handle, start, end| {
            call(handle, &raw_rule, start, end)
        })
    }

    fn analyze_sparse_append_v2(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRuleV2,
            *mut sys::ArcadiaTioSparseAppendAnalysis,
        ) -> i32,
    ) -> Result<SparseAppendAnalysis> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw_v2();
        let mut raw_analysis = empty_sparse_append_analysis();
        let status = call(self.raw.as_ptr(), &raw_rule, &mut raw_analysis);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: `raw_analysis` was initialized to an empty native-compatible value before the
            // call. If native populated reasons before returning an error, this releases them once.
            unsafe { sys::arcadia_tio_sparse_append_analysis_free(&mut raw_analysis) };
            return Err(TioError::from_last_error("failed to analyze sparse append"));
        }
        take_sparse_append_analysis(&mut raw_analysis)
    }

    fn append_sparse_with_range_v2(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRuleV2,
            *mut u32,
            *mut u32,
        ) -> i32,
    ) -> Result<AppendRange> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw_v2();
        self.append_with_range(shape, |handle, start, end| {
            call(handle, &raw_rule, start, end)
        })
    }

    fn prepare_selectors(&self, selectors: &[EntrySelector]) -> Result<PreparedSelectors> {
        PreparedSelectors::new(selectors, self.rank()?)
    }

    fn read_tensor(
        &self,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut sys::ArcadiaTioTensor) -> i32,
    ) -> Result<Tensor> {
        let mut raw = sys::ArcadiaTioTensor::default();
        let status = call(self.raw.as_ptr(), &mut raw);
        status_result(status, "failed to read tensor")?;
        let tensor = copy_tensor(&raw);
        // SAFETY: `raw` is native-owned output from a tensor read call and freed exactly once.
        unsafe { sys::arcadia_tio_tensor_free(&mut raw) };
        tensor
    }

    fn validate_axis(&self, axis: usize) -> Result<()> {
        let rank = self.rank()?;
        if axis >= rank {
            Err(TioError::invalid_argument(format!(
                "axis {axis} out of range for rank {rank}"
            )))
        } else {
            Ok(())
        }
    }

    fn validate_append(&self, dtype: DType, data_len: usize, shape: &[u64]) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "append")
    }

    fn validate_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "sparse append")?;
        rule.validate_for_append(dtype, shape.len(), self.append_axis()?)
    }

    fn validate_mutation_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, operation)
    }

    fn validate_typed_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        let actual_dtype = self.dtype()?;
        if actual_dtype != dtype {
            return Err(TioError::invalid_argument(format!(
                "{operation} dtype {dtype:?} does not match file dtype {actual_dtype:?}"
            )));
        }
        let rank = self.rank()?;
        if shape.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "{operation} shape rank {} does not match file rank {rank}",
                shape.len()
            )));
        }
        let expected_len = shape_element_len(shape)?;
        if expected_len != data_len {
            return Err(TioError::invalid_argument(format!(
                "{operation} data length {data_len} does not match shape element count {expected_len}"
            )));
        }
        Ok(())
    }

    fn from_raw_handle(raw: *mut sys::ArcadiaTioHandle, context: &str) -> Result<Self> {
        let raw = NonNull::new(raw).ok_or_else(|| TioError::from_last_error(context))?;
        Ok(Self {
            raw,
            _not_send_or_sync: PhantomData,
        })
    }

    #[allow(dead_code)]
    fn raw_handle(&self) -> *mut sys::ArcadiaTioHandle {
        self.raw.as_ptr()
    }
}

impl Drop for TensorFile {
    fn drop(&mut self) {
        // SAFETY: `TensorFile` owns this non-null handle and Drop runs at most once.
        unsafe { sys::arcadia_tio_close(self.raw.as_ptr()) };
    }
}

fn shape_element_len(shape: &[u64]) -> Result<usize> {
    let mut product = 1usize;
    for &dim in shape {
        let dim = usize::try_from(dim)
            .map_err(|_| TioError::invalid_argument("shape dimension does not fit usize"))?;
        product = product
            .checked_mul(dim)
            .ok_or_else(|| TioError::invalid_argument("shape element count overflows usize"))?;
    }
    Ok(product)
}

fn validate_tensor_parts(dtype: DType, shape: &[u64], data: &TensorData) -> Result<()> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    let data_dtype = data.dtype();
    if data_dtype != dtype {
        return Err(TioError::invalid_argument(format!(
            "tensor dtype {:?} does not match payload dtype {:?}",
            dtype, data_dtype
        )));
    }
    let expected = shape_element_len(shape)?;
    let actual = data.len();
    if expected != actual {
        return Err(TioError::invalid_argument(format!(
            "tensor data length {actual} does not match shape element count {expected}"
        )));
    }
    Ok(())
}

fn validate_create_with_coordinates_v2_options(
    options: &CreateOptions,
    coordinate_options: CoordinateV2Options,
) -> Result<()> {
    if !options.coordinates.is_empty() {
        return Err(TioError::invalid_argument(
            "Coordinate v2 create helpers cannot be combined with v1 CoordinateSpec descriptors",
        ));
    }
    if coordinate_options.allow_external_resolution {
        return Err(TioError::unimplemented(
            "Coordinate v2 public Rust create helpers do not resolve external references",
        ));
    }
    Ok(())
}

fn validate_create_policy(options: &CreateOptions, policy: &CreatePolicyOptions) -> Result<()> {
    let rank = options.dims.len();
    if options.append_dim >= rank {
        return Err(TioError::invalid_argument("append_dim out of range"));
    }
    if policy.chunk_axes.is_empty() {
        return Err(TioError::invalid_argument(
            "policy create requires at least one chunk axis",
        ));
    }
    if policy.typical_query_sizes.len() != rank {
        return Err(TioError::invalid_argument(format!(
            "typical_query_sizes length {} does not match rank {rank}",
            policy.typical_query_sizes.len()
        )));
    }
    if options.append_dim != 0 {
        return Err(TioError::invalid_argument(
            "RegularChunked policy create currently requires append_dim == 0",
        ));
    }
    if policy.storage_profile != StorageProfile::Balanced {
        return Err(TioError::invalid_argument(
            "RegularChunked policy create currently supports only balanced storage_profile",
        ));
    }
    if !matches!(policy.typical_query_sizes[options.append_dim], 0 | 1) {
        return Err(TioError::invalid_argument(
            "append-axis typical_query_size must be 0 or 1",
        ));
    }
    let mut seen = Vec::with_capacity(policy.chunk_axes.len());
    for &axis in &policy.chunk_axes {
        if axis >= rank {
            return Err(TioError::invalid_argument(format!(
                "chunk axis {axis} out of range for rank {rank}"
            )));
        }
        if axis == options.append_dim {
            return Err(TioError::invalid_argument(
                "chunk axes must exclude the append axis",
            ));
        }
        if seen.contains(&axis) {
            return Err(TioError::invalid_argument(
                "chunk axes must be unique for policy create",
            ));
        }
        if policy.typical_query_sizes[axis] == 0 {
            return Err(TioError::invalid_argument(
                "chunk-axis typical_query_size must be > 0",
            ));
        }
        seen.push(axis);
    }
    for axis in 0..rank {
        if axis != options.append_dim && !seen.contains(&axis) {
            return Err(TioError::invalid_argument(
                "chunk_axes must include every non-append axis for policy create",
            ));
        }
    }
    Ok(())
}

fn copy_shape(raw: &sys::ArcadiaTioTensor) -> Result<Vec<u64>> {
    if raw.rank == 0 {
        return Ok(Vec::new());
    }
    if raw.shape.is_null() {
        return Err(TioError::conversion("native tensor shape pointer is null"));
    }
    // SAFETY: Native tensor shape pointer is valid for `rank` while the tensor output is alive.
    Ok(unsafe { slice::from_raw_parts(raw.shape, raw.rank) }.to_vec())
}

fn copy_tensor(raw: &sys::ArcadiaTioTensor) -> Result<Tensor> {
    let dtype = DType::from_raw(raw.dtype)?;
    let shape = copy_shape(raw)?;
    let element_count = shape_element_len(&shape)?;
    let expected_bytes = element_count
        .checked_mul(dtype.size_bytes())
        .ok_or_else(|| TioError::conversion("native tensor byte length overflows usize"))?;
    if raw.len_bytes != expected_bytes {
        return Err(TioError::conversion(format!(
            "native tensor byte length {} does not match shape/dtype byte length {expected_bytes}",
            raw.len_bytes
        )));
    }
    if element_count == 0 {
        let data = match dtype {
            DType::F32 => TensorData::F32(Vec::new()),
            DType::F64 => TensorData::F64(Vec::new()),
            DType::I32 => TensorData::I32(Vec::new()),
            DType::I64 => TensorData::I64(Vec::new()),
        };
        return Ok(Tensor { dtype, shape, data });
    }
    if raw.data.is_null() {
        return Err(TioError::conversion("native tensor data pointer is null"));
    }
    let data = match dtype {
        DType::F32 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<f32>(), element_count) };
            TensorData::F32(values.to_vec())
        }
        DType::F64 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<f64>(), element_count) };
            TensorData::F64(values.to_vec())
        }
        DType::I32 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<i32>(), element_count) };
            TensorData::I32(values.to_vec())
        }
        DType::I64 => {
            // SAFETY: The C ABI guarantees alignment and byte length for the tensor dtype.
            let values = unsafe { slice::from_raw_parts(raw.data.cast::<i64>(), element_count) };
            TensorData::I64(values.to_vec())
        }
    };
    Ok(Tensor { dtype, shape, data })
}

fn copy_mask(raw: &sys::ArcadiaTioMask) -> Option<Vec<u8>> {
    if raw.len == 0 || raw.data.is_null() {
        return None;
    }
    // SAFETY: The C ABI returns a native-owned mask with `len` bytes while the mask output is alive.
    Some(unsafe { slice::from_raw_parts(raw.data, raw.len) }.to_vec())
}

struct NativeCommitList {
    raw: sys::ArcadiaTioCommitList,
}

impl NativeCommitList {
    fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioCommitList {
                items: ptr::null_mut(),
                len: 0,
            },
        }
    }

    fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioCommitList {
        &mut self.raw
    }

    fn as_ref(&self) -> &sys::ArcadiaTioCommitList {
        &self.raw
    }
}

impl Drop for NativeCommitList {
    fn drop(&mut self) {
        // SAFETY: `raw` is either empty or a native-owned commit-list output. The guard owns it and
        // drops exactly once on all success/error/copy-conversion paths.
        unsafe { sys::arcadia_tio_commit_list_free(&mut self.raw) };
    }
}

struct NativeChunkPlan {
    raw: sys::ArcadiaTioChunkPlan,
}

impl NativeChunkPlan {
    fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioChunkPlan {
                block_sizes: ptr::null_mut(),
                len: 0,
            },
        }
    }

    fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioChunkPlan {
        &mut self.raw
    }

    fn as_ref(&self) -> &sys::ArcadiaTioChunkPlan {
        &self.raw
    }
}

impl Drop for NativeChunkPlan {
    fn drop(&mut self) {
        // SAFETY: `raw` is either empty or a native-owned chunk-plan output. The guard owns it and
        // drops exactly once on all success/error/copy-conversion paths.
        unsafe { sys::arcadia_tio_chunk_plan_free(&mut self.raw) };
    }
}

fn copy_commit_list(raw: &sys::ArcadiaTioCommitList) -> Result<Vec<CommitInfo>> {
    if raw.len == 0 {
        return Ok(Vec::new());
    }
    if raw.items.is_null() {
        return Err(TioError::conversion("native commit list pointer is null"));
    }
    // SAFETY: The C ABI returns `len` commit records owned by the commit-list output while alive.
    Ok(unsafe { slice::from_raw_parts(raw.items, raw.len) }
        .iter()
        .copied()
        .map(CommitInfo::from)
        .collect())
}

fn copy_chunk_plan(raw: &sys::ArcadiaTioChunkPlan) -> Result<ChunkPlan> {
    if raw.len == 0 {
        return Ok(ChunkPlan {
            block_sizes: Vec::new(),
        });
    }
    if raw.block_sizes.is_null() {
        return Err(TioError::conversion(
            "native chunk plan block-size pointer is null",
        ));
    }
    // SAFETY: The C ABI returns `len` block-size entries owned by the chunk-plan output while alive.
    Ok(ChunkPlan {
        block_sizes: unsafe { slice::from_raw_parts(raw.block_sizes, raw.len) }.to_vec(),
    })
}

fn new_query_trace_json() -> sys::ArcadiaTioQueryTraceJson {
    sys::ArcadiaTioQueryTraceJson {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceJson>(),
        json: ptr::null_mut(),
    }
}

fn copy_query_trace_json(raw: &sys::ArcadiaTioQueryTraceJson) -> Result<QueryTraceJson> {
    if raw.json.is_null() {
        return Err(TioError::conversion(
            "native query trace JSON pointer is null",
        ));
    }
    // SAFETY: The C ABI returns a native-owned NUL-terminated JSON string while the output is alive.
    let json = unsafe { CStr::from_ptr(raw.json.cast_const()) }
        .to_string_lossy()
        .into_owned();
    Ok(QueryTraceJson { json })
}

fn new_read_execution_report() -> sys::ArcadiaTioReadExecutionReport {
    sys::ArcadiaTioReadExecutionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReadExecutionReport>(),
        requested_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_max_threads: 0,
        query_effective_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_effective_threads: 0,
        query_parallel_runtime: ptr::null_mut(),
        query_parallel_fallback_reason: ptr::null_mut(),
        query_parallel_reason_code: ptr::null_mut(),
        query_parallel_reason_code_taxonomy: ptr::null_mut(),
    }
}

fn new_read_index_report() -> sys::ArcadiaTioReadIndexReport {
    sys::ArcadiaTioReadIndexReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReadIndexReport>(),
        lowering_kind: sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN,
        used_full_tensor_fallback: 0,
        reserved0: [0; 7],
    }
}

fn new_historical_read_execution_report() -> sys::ArcadiaTioHistoricalReadExecutionReport {
    sys::ArcadiaTioHistoricalReadExecutionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadExecutionReport>(),
        requested_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_max_threads: 0,
        query_effective_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_effective_threads: 0,
        query_parallel_runtime: ptr::null_mut(),
        query_parallel_fallback_reason: ptr::null_mut(),
        query_parallel_reason_code: ptr::null_mut(),
        query_parallel_reason_code_taxonomy: ptr::null_mut(),
        query_source_kind: sys::ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT,
        query_commit_seq: 0,
    }
}

fn new_historical_read_index_report() -> sys::ArcadiaTioHistoricalReadIndexReport {
    sys::ArcadiaTioHistoricalReadIndexReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadIndexReport>(),
        requested_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_max_threads: 0,
        query_effective_mode: sys::ARCADIA_TIO_READ_EXECUTION_SERIAL,
        query_effective_threads: 0,
        query_parallel_runtime: ptr::null_mut(),
        query_parallel_fallback_reason: ptr::null_mut(),
        query_parallel_reason_code: ptr::null_mut(),
        query_parallel_reason_code_taxonomy: ptr::null_mut(),
        query_source_kind: sys::ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT,
        query_commit_seq: 0,
        lowering_kind: sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN,
        used_full_tensor_fallback: 0,
        reserved0: [0; 7],
    }
}

fn copy_read_execution_report(
    raw: &sys::ArcadiaTioReadExecutionReport,
) -> Result<ReadExecutionReport> {
    Ok(ReadExecutionReport {
        requested_mode: ReadExecutionMode::from_raw(raw.requested_mode, raw.query_max_threads)?,
        query_max_threads: raw.query_max_threads,
        query_effective_mode: ReadExecutionMode::from_raw(
            raw.query_effective_mode,
            raw.query_effective_threads,
        )?,
        query_effective_threads: raw.query_effective_threads,
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const()),
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        ),
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const()),
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        ),
    })
}

fn copy_read_index_report(raw: &sys::ArcadiaTioReadIndexReport) -> Result<ReadIndexReport> {
    Ok(ReadIndexReport {
        lowering_kind: ReadIndexLoweringKind::from_raw(raw.lowering_kind)?,
        used_full_tensor_fallback: raw.used_full_tensor_fallback != 0,
    })
}

fn copy_historical_read_execution_report(
    raw: &sys::ArcadiaTioHistoricalReadExecutionReport,
) -> Result<HistoricalReadExecutionReport> {
    let execution = ReadExecutionReport {
        requested_mode: ReadExecutionMode::from_raw(raw.requested_mode, raw.query_max_threads)?,
        query_max_threads: raw.query_max_threads,
        query_effective_mode: ReadExecutionMode::from_raw(
            raw.query_effective_mode,
            raw.query_effective_threads,
        )?,
        query_effective_threads: raw.query_effective_threads,
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const()),
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        ),
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const()),
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        ),
    };
    Ok(HistoricalReadExecutionReport {
        execution,
        query_source_kind: HistoricalQuerySourceKind::from_raw(raw.query_source_kind)?,
        query_commit_seq: raw.query_commit_seq,
    })
}

fn copy_historical_read_index_report(
    raw: &sys::ArcadiaTioHistoricalReadIndexReport,
) -> Result<HistoricalReadIndexReport> {
    let execution = ReadExecutionReport {
        requested_mode: ReadExecutionMode::from_raw(raw.requested_mode, raw.query_max_threads)?,
        query_max_threads: raw.query_max_threads,
        query_effective_mode: ReadExecutionMode::from_raw(
            raw.query_effective_mode,
            raw.query_effective_threads,
        )?,
        query_effective_threads: raw.query_effective_threads,
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const()),
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        ),
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const()),
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        ),
    };
    Ok(HistoricalReadIndexReport {
        execution: HistoricalReadExecutionReport {
            execution,
            query_source_kind: HistoricalQuerySourceKind::from_raw(raw.query_source_kind)?,
            query_commit_seq: raw.query_commit_seq,
        },
        read_index: ReadIndexReport {
            lowering_kind: ReadIndexLoweringKind::from_raw(raw.lowering_kind)?,
            used_full_tensor_fallback: raw.used_full_tensor_fallback != 0,
        },
    })
}

fn new_v4_precise_accounting_bytes() -> sys::ArcadiaTioV4PreciseAccountingBytes {
    sys::ArcadiaTioV4PreciseAccountingBytes {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4PreciseAccountingBytes>(),
        has_unreachable_bytes: 0,
        unreachable_bytes: 0,
        has_retained_history_required_bytes: 0,
        retained_history_required_bytes: 0,
        has_popped_skipped_bytes: 0,
        popped_skipped_bytes: 0,
        has_reclaimable_bytes: 0,
        reclaimable_bytes: 0,
        omitted_fields: ptr::null_mut(),
        omitted_fields_len: 0,
        omitted_field_reason_codes: ptr::null_mut(),
        omitted_field_reason_codes_len: 0,
    }
}

fn copy_v4_precise_accounting_bytes(
    raw: &sys::ArcadiaTioV4PreciseAccountingBytes,
) -> V4PreciseAccountingBytes {
    let omitted_fields = if raw.omitted_fields.is_null() || raw.omitted_fields_len == 0 {
        Vec::new()
    } else {
        // SAFETY: Native report owns `omitted_fields_len` entries until the parent report is freed.
        let fields = unsafe { slice::from_raw_parts(raw.omitted_fields, raw.omitted_fields_len) };
        let reason_codes = if raw.omitted_field_reason_codes.is_null()
            || raw.omitted_field_reason_codes_len == 0
        {
            &[][..]
        } else {
            // SAFETY: Native report owns `omitted_field_reason_codes_len` entries until parent free.
            unsafe {
                slice::from_raw_parts(
                    raw.omitted_field_reason_codes,
                    raw.omitted_field_reason_codes_len,
                )
            }
        };
        fields
            .iter()
            .enumerate()
            .map(|(index, field)| V4OmittedPreciseAccountingField {
                field: V4PreciseAccountingField::from_raw(field.field),
                reason: optional_c_string(field.reason.cast_const()),
                reason_code: reason_codes
                    .get(index)
                    .and_then(|ptr| optional_c_string((*ptr).cast_const())),
            })
            .collect()
    };
    V4PreciseAccountingBytes {
        unreachable_bytes: (raw.has_unreachable_bytes != 0).then_some(raw.unreachable_bytes),
        retained_history_required_bytes: (raw.has_retained_history_required_bytes != 0)
            .then_some(raw.retained_history_required_bytes),
        popped_skipped_bytes: (raw.has_popped_skipped_bytes != 0)
            .then_some(raw.popped_skipped_bytes),
        reclaimable_bytes: (raw.has_reclaimable_bytes != 0).then_some(raw.reclaimable_bytes),
        omitted_fields,
    }
}

fn copy_v4_current_head_bytes(raw: sys::ArcadiaTioV4CurrentHeadBytes) -> V4CurrentHeadBytes {
    V4CurrentHeadBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
        commit_bytes: raw.commit_bytes,
    }
}

fn copy_v4_audit_bytes(raw: sys::ArcadiaTioV4AuditBytes) -> V4AuditBytes {
    V4AuditBytes {
        commit_bytes: raw.commit_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

fn copy_v4_payload_reuse_bytes(raw: sys::ArcadiaTioV4PayloadReuseBytes) -> V4PayloadReuseBytes {
    V4PayloadReuseBytes {
        resurrected_payload_bytes: raw.resurrected_payload_bytes,
        shared_payload_bytes: raw.shared_payload_bytes,
    }
}

fn copy_v4_superseded_bytes(raw: sys::ArcadiaTioV4SupersededBytes) -> V4SupersededBytes {
    V4SupersededBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

fn new_v4_diagnostics_report() -> sys::ArcadiaTioV4DiagnosticsReport {
    sys::ArcadiaTioV4DiagnosticsReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4DiagnosticsReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        current_head: sys::ArcadiaTioV4CurrentHeadBytes {
            payload_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
            commit_bytes: 0,
        },
        visible_chain_audit: sys::ArcadiaTioV4AuditBytes {
            commit_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
        },
        payload_reuse: sys::ArcadiaTioV4PayloadReuseBytes {
            resurrected_payload_bytes: 0,
            shared_payload_bytes: 0,
        },
        superseded: sys::ArcadiaTioV4SupersededBytes {
            payload_bytes: 0,
            index_bytes: 0,
            epoch_bytes: 0,
            aux_bytes: 0,
        },
        unknown_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_v4_diagnostics_report(raw: &sys::ArcadiaTioV4DiagnosticsReport) -> V4DiagnosticsReport {
    V4DiagnosticsReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    }
}

fn new_v4_diagnostics_precise_report() -> sys::ArcadiaTioV4DiagnosticsPreciseReport {
    sys::ArcadiaTioV4DiagnosticsPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4DiagnosticsPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        current_head: new_v4_diagnostics_report().current_head,
        visible_chain_audit: new_v4_diagnostics_report().visible_chain_audit,
        payload_reuse: new_v4_diagnostics_report().payload_reuse,
        superseded: new_v4_diagnostics_report().superseded,
        unknown_bytes: 0,
        precise_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_diagnostics_precise_report(
    raw: &sys::ArcadiaTioV4DiagnosticsPreciseReport,
) -> V4DiagnosticsPreciseReport {
    V4DiagnosticsPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    }
}

fn new_v4_compaction_analysis_report() -> sys::ArcadiaTioV4CompactionAnalysisReport {
    sys::ArcadiaTioV4CompactionAnalysisReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4CompactionAnalysisReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        policy: sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE,
        source_file_bytes: 0,
        current_state_required_bytes: 0,
        ordinary_reclaimable_bytes: 0,
        unknown_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_v4_compaction_analysis_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisReport,
) -> Result<V4CompactionAnalysisReport> {
    Ok(V4CompactionAnalysisReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    })
}

fn new_v4_compaction_analysis_precise_report() -> sys::ArcadiaTioV4CompactionAnalysisPreciseReport {
    sys::ArcadiaTioV4CompactionAnalysisPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4CompactionAnalysisPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        policy: sys::ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE,
        source_file_bytes: 0,
        current_state_required_bytes: 0,
        ordinary_reclaimable_bytes: 0,
        unknown_bytes: 0,
        precise_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_compaction_analysis_precise_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisPreciseReport,
) -> Result<V4CompactionAnalysisPreciseReport> {
    Ok(V4CompactionAnalysisPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    })
}

fn new_v4_retained_history_compaction_report() -> sys::ArcadiaTioV4RetainedHistoryCompactionReport {
    sys::ArcadiaTioV4RetainedHistoryCompactionReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        retained_commit_count: 0,
        retained_commit_seqs: ptr::null_mut(),
        retained_commit_seqs_len: 0,
        has_unretained_older_commit_count: 0,
        unretained_older_commit_count: 0,
        source_file_bytes: 0,
        destination_file_bytes: 0,
        omitted_unreachable_bytes: 0,
        omitted_unreachable_bytes_reason: ptr::null_mut(),
    }
}

fn copy_retained_commit_seqs(ptr: *mut u64, len: usize) -> Vec<u64> {
    if ptr.is_null() || len == 0 {
        Vec::new()
    } else {
        // SAFETY: Native report owns `len` entries until the parent report is freed.
        unsafe { slice::from_raw_parts(ptr, len) }.to_vec()
    }
}

fn copy_v4_retained_history_compaction_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionReport,
) -> Result<V4RetainedHistoryCompactionReport> {
    Ok(V4RetainedHistoryCompactionReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        ),
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        ),
    })
}

fn new_v4_retained_history_compaction_precise_report()
-> sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport {
    sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport>(),
        status: sys::ARCADIA_TIO_V4_REPORT_UNKNOWN,
        reason: ptr::null_mut(),
        retained_commit_count: 0,
        retained_commit_seqs: ptr::null_mut(),
        retained_commit_seqs_len: 0,
        has_unretained_older_commit_count: 0,
        unretained_older_commit_count: 0,
        source_file_bytes: 0,
        destination_file_bytes: 0,
        precise_source_accounting: new_v4_precise_accounting_bytes(),
        reason_code: ptr::null_mut(),
    }
}

fn copy_v4_retained_history_compaction_precise_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
) -> Result<V4RetainedHistoryCompactionPreciseReport> {
    Ok(V4RetainedHistoryCompactionPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const()),
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        ),
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        precise_source_accounting: copy_v4_precise_accounting_bytes(&raw.precise_source_accounting),
        reason_code: optional_c_string(raw.reason_code.cast_const()),
    })
}

fn new_reform_report() -> sys::ArcadiaTioReformReport {
    sys::ArcadiaTioReformReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReformReport>(),
        reason_code: ptr::null_mut(),
        reason_code_taxonomy: ptr::null_mut(),
        reason: ptr::null_mut(),
    }
}

fn copy_reform_report(raw: &sys::ArcadiaTioReformReport) -> ReformReport {
    ReformReport {
        reason_code: optional_c_string(raw.reason_code.cast_const()),
        reason_code_taxonomy: optional_c_string(raw.reason_code_taxonomy.cast_const()),
        reason: optional_c_string(raw.reason.cast_const()),
    }
}

fn new_auto_compaction_config() -> sys::ArcadiaTioAutoCompactionConfig {
    AutoCompactionConfig::default().to_raw()
}

fn copy_auto_compaction_config(
    raw: sys::ArcadiaTioAutoCompactionConfig,
) -> Result<AutoCompactionConfig> {
    Ok(AutoCompactionConfig {
        enabled: raw.enabled != 0,
        retain_commits: raw.retain_commits,
        dead_ratio_threshold: raw.dead_ratio_threshold,
        min_dead_bytes: raw.min_dead_bytes,
        mode: CompactionMode::from_raw(raw.mode)?,
        check_every_commits: raw.check_every_commits,
        cooldown_commits: raw.cooldown_commits,
    })
}

fn copy_axis_labels(ptr: *mut sys::ArcadiaTioAxisLabel, len: usize) -> Vec<AxisLabel> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| AxisLabel {
            id: item.id,
            name: required_c_string(item.name.cast_const()),
        })
        .collect()
}

fn copy_user_kv(ptr: *mut sys::ArcadiaTioUserKv, len: usize) -> Vec<UserKv> {
    if ptr.is_null() || len == 0 {
        return Vec::new();
    }
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| UserKv {
            key: required_c_string(item.key.cast_const()),
            value: required_c_string(item.value.cast_const()),
        })
        .collect()
}

fn copy_file_meta(raw: &sys::ArcadiaTioFileMeta) -> Result<FileMeta> {
    let dims = if raw.dims.is_null() || raw.rank == 0 {
        Vec::new()
    } else {
        // SAFETY: Metadata dimension array is valid for `rank` while the native metadata object is alive.
        unsafe { slice::from_raw_parts(raw.dims, raw.rank) }
            .iter()
            .map(|dim| {
                Ok(DimSpec {
                    kind: AxisKind::from_raw(dim.kind)?,
                    len: dim.len,
                    name: optional_c_string(dim.name.cast_const()),
                })
            })
            .collect::<Result<Vec<_>>>()?
    };
    Ok(FileMeta {
        dtype: DType::from_raw(raw.dtype)?,
        dims,
        append_dim: raw.append_dim,
        symbols: copy_axis_labels(raw.symbols, raw.symbols_len),
        channels: copy_axis_labels(raw.channels, raw.channels_len),
        user_kv: copy_user_kv(raw.user_kv, raw.user_kv_len),
        effective_profile: HeaderProfile::from_raw(raw.effective_profile)?,
        commit_seq: raw.commit_seq,
    })
}

fn copy_coordinate_meta(
    ptr: *mut sys::ArcadiaTioAxisCoordinateMeta,
    len: usize,
) -> Result<Vec<CoordinateMeta>> {
    if ptr.is_null() || len == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: Coordinate metadata array is valid for `len` until freed by the caller.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(|item| {
            Ok(CoordinateMeta {
                axis: item.axis,
                axis_name_snapshot: optional_c_string(item.axis_name_snapshot.cast_const()),
                name: optional_c_string(item.name.cast_const()),
                kind: CoordinateKind::from_raw(item.kind)?,
                dtype: CoordinateDType::from_raw(item.dtype)?,
                encoding: CoordinateEncoding::from_raw(item.encoding)?,
                length: item.length,
                ordering: CoordinateOrdering {
                    sorted: CoordinateSortedness::from_raw(item.sorted)?,
                    monotonicity: CoordinateMonotonicity::from_raw(item.monotonicity)?,
                    uniqueness: CoordinateUniqueness::from_raw(item.uniqueness)?,
                },
                storage_kind: CoordinateStorageKind::from_raw(item.storage_kind)?,
                external_source_kind: ExternalCoordinateSourceKind::from_raw(
                    item.external_source_kind,
                )?,
                external_uri: optional_c_string(item.external_uri.cast_const()),
                required: item.required != 0,
                validation_status: CoordinateValidationStatus::from_raw(item.validation_status)?,
            })
        })
        .collect()
}

fn copy_coordinate_meta_v2(
    ptr: *mut sys::ArcadiaTioAxisCoordinateMetaV2,
    len: usize,
) -> Result<Vec<AxisCoordinateMetaV2>> {
    if ptr.is_null() || len == 0 {
        return Ok(Vec::new());
    }
    // SAFETY: Coordinate v2 metadata array is valid for `len` until freed by the caller.
    unsafe { slice::from_raw_parts(ptr, len) }
        .iter()
        .map(AxisCoordinateMetaV2::from_raw)
        .collect()
}

struct PreparedStringList {
    _strings: Vec<CString>,
    ptrs: Vec<*const c_char>,
}

impl PreparedStringList {
    fn new<S: AsRef<str>>(values: &[S], label: &str) -> Result<Self> {
        let strings = values
            .iter()
            .map(|value| string_to_cstring(value.as_ref(), label))
            .collect::<Result<Vec<_>>>()?;
        let ptrs = strings.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _strings: strings,
            ptrs,
        })
    }

    fn ptr(&self) -> *const *const c_char {
        if self.ptrs.is_empty() {
            ptr::null()
        } else {
            self.ptrs.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.ptrs.len()
    }
}

struct PreparedUserKvList {
    _keys: Vec<CString>,
    _values: Vec<CString>,
    key_ptrs: Vec<*const c_char>,
    value_ptrs: Vec<*const c_char>,
}

impl PreparedUserKvList {
    fn new<K, V>(values: &[(K, V)]) -> Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let keys = values
            .iter()
            .map(|(key, _)| string_to_cstring(key.as_ref(), "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = values
            .iter()
            .map(|(_, value)| string_to_cstring(value.as_ref(), "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let key_ptrs = keys.iter().map(|value| value.as_ptr()).collect();
        let value_ptrs = user_values.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _keys: keys,
            _values: user_values,
            key_ptrs,
            value_ptrs,
        })
    }

    fn key_ptr(&self) -> *const *const c_char {
        if self.key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.key_ptrs.as_ptr()
        }
    }

    fn value_ptr(&self) -> *const *const c_char {
        if self.value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.value_ptrs.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.key_ptrs.len()
    }
}

#[allow(dead_code)]
struct PreparedCreate<'a> {
    path: CString,
    dim_kinds: Vec<sys::ArcadiaTioAxisKind>,
    dim_lens: Vec<u32>,
    dim_name_strings: Vec<CString>,
    dim_name_ptrs: Vec<*const c_char>,
    symbols: Vec<CString>,
    symbol_ptrs: Vec<*const c_char>,
    channels: Vec<CString>,
    channel_ptrs: Vec<*const c_char>,
    user_keys: Vec<CString>,
    user_values: Vec<CString>,
    user_key_ptrs: Vec<*const c_char>,
    user_value_ptrs: Vec<*const c_char>,
    coordinate_names: Vec<Option<CString>>,
    coordinate_external_uris: Vec<Option<CString>>,
    coordinate_inputs: Vec<sys::ArcadiaTioAxisCoordinateInput>,
    _coordinate_values: PhantomData<&'a [CoordinateSpec]>,
}

impl<'a> PreparedCreate<'a> {
    fn new(path: impl AsRef<Path>, options: &'a CreateOptions) -> Result<Self> {
        if options.dims.is_empty() {
            return Err(TioError::invalid_argument("rank must be > 0"));
        }
        if options.append_dim >= options.dims.len() {
            return Err(TioError::invalid_argument("append_dim out of range"));
        }
        if options.dims.len() > usize::MAX / 2 {
            return Err(TioError::invalid_argument("rank is too large"));
        }
        for (idx, dim) in options.dims.iter().enumerate() {
            if matches!(dim.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "dimension {idx} name cannot be empty"
                )));
            }
        }

        let path = path_to_cstring(path)?;
        let dim_kinds = options
            .dims
            .iter()
            .map(|dim| dim.kind.to_raw())
            .collect::<Vec<_>>();
        let dim_lens = options.dims.iter().map(|dim| dim.len).collect::<Vec<_>>();

        let dim_name_strings = options
            .dims
            .iter()
            .filter_map(|dim| dim.name.as_ref())
            .map(|name| string_to_cstring(name, "dimension name"))
            .collect::<Result<Vec<_>>>()?;
        let mut dim_name_iter = dim_name_strings.iter();
        let dim_name_ptrs = options
            .dims
            .iter()
            .map(|dim| {
                if dim.name.is_some() {
                    dim_name_iter.next().expect("name count matches").as_ptr()
                } else {
                    ptr::null()
                }
            })
            .collect::<Vec<_>>();

        let symbols = options
            .symbols
            .iter()
            .map(|value| string_to_cstring(value, "symbol label"))
            .collect::<Result<Vec<_>>>()?;
        let symbol_ptrs = symbols
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let channels = options
            .channels
            .iter()
            .map(|value| string_to_cstring(value, "channel label"))
            .collect::<Result<Vec<_>>>()?;
        let channel_ptrs = channels
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_keys = options
            .user_kv
            .iter()
            .map(|(key, _)| string_to_cstring(key, "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = options
            .user_kv
            .iter()
            .map(|(_, value)| string_to_cstring(value, "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let user_key_ptrs = user_keys
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_value_ptrs = user_values
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();

        for (idx, coord) in options.coordinates.iter().enumerate() {
            if coord.axis >= options.dims.len() {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} axis out of range"
                )));
            }
            if matches!(coord.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} name cannot be empty"
                )));
            }
        }
        let coordinate_names = options
            .coordinates
            .iter()
            .map(|coord| {
                coord
                    .name
                    .as_deref()
                    .map(|name| string_to_cstring(name, "coordinate name"))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_external_uris = options
            .coordinates
            .iter()
            .map(|coord| match &coord.storage {
                CoordinateStorage::Inline(_) => Ok(None),
                CoordinateStorage::External { uri, .. } => {
                    string_to_cstring(uri, "external coordinate URI").map(Some)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_inputs = options
            .coordinates
            .iter()
            .enumerate()
            .map(|(idx, coord)| {
                coordinate_input(
                    coord,
                    coordinate_names[idx].as_ref(),
                    coordinate_external_uris[idx].as_ref(),
                )
            })
            .collect::<Vec<_>>();

        Ok(Self {
            path,
            dim_kinds,
            dim_lens,
            dim_name_strings,
            dim_name_ptrs,
            symbols,
            symbol_ptrs,
            channels,
            channel_ptrs,
            user_keys,
            user_values,
            user_key_ptrs,
            user_value_ptrs,
            coordinate_names,
            coordinate_external_uris,
            coordinate_inputs,
            _coordinate_values: PhantomData,
        })
    }

    fn dim_name_ptr(&self) -> *const *const c_char {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            ptr::null()
        } else {
            self.dim_name_ptrs.as_ptr()
        }
    }

    fn dim_name_len(&self) -> usize {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            0
        } else {
            self.dim_name_ptrs.len()
        }
    }

    fn symbol_ptr(&self) -> *const *const c_char {
        if self.symbol_ptrs.is_empty() {
            ptr::null()
        } else {
            self.symbol_ptrs.as_ptr()
        }
    }

    fn symbol_len(&self) -> usize {
        self.symbol_ptrs.len()
    }

    fn channel_ptr(&self) -> *const *const c_char {
        if self.channel_ptrs.is_empty() {
            ptr::null()
        } else {
            self.channel_ptrs.as_ptr()
        }
    }

    fn channel_len(&self) -> usize {
        self.channel_ptrs.len()
    }

    fn user_key_ptr(&self) -> *const *const c_char {
        if self.user_key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_key_ptrs.as_ptr()
        }
    }

    fn user_value_ptr(&self) -> *const *const c_char {
        if self.user_value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_value_ptrs.as_ptr()
        }
    }

    fn user_kv_len(&self) -> usize {
        self.user_key_ptrs.len()
    }

    fn coordinate_ptr(&self) -> *const sys::ArcadiaTioAxisCoordinateInput {
        if self.coordinate_inputs.is_empty() {
            ptr::null()
        } else {
            self.coordinate_inputs.as_ptr()
        }
    }

    fn coordinate_len(&self) -> usize {
        self.coordinate_inputs.len()
    }
}

struct PreparedCreateUniverseOptions {
    axis_identities: Vec<sys::ArcadiaTioAxisIdentityInput>,
}

impl PreparedCreateUniverseOptions {
    fn new(options: &CreateUniverseOptions) -> Self {
        let axis_identities = options
            .axis_identities
            .iter()
            .map(|identity| sys::ArcadiaTioAxisIdentityInput {
                version: 1,
                struct_size: mem::size_of::<sys::ArcadiaTioAxisIdentityInput>(),
                axis: identity.axis,
                mode: identity.mode.to_raw(),
            })
            .collect();
        Self { axis_identities }
    }

    fn raw_options(&self) -> sys::ArcadiaTioCreateWithUniverseOptions {
        sys::ArcadiaTioCreateWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioCreateWithUniverseOptions>(),
            axis_identities: if self.axis_identities.is_empty() {
                ptr::null()
            } else {
                self.axis_identities.as_ptr()
            },
            axis_identities_len: self.axis_identities.len(),
        }
    }
}

struct PreparedAppendUniverseOptions<'a> {
    slot_axes: Vec<Vec<sys::ArcadiaTioUniverseBindingInput>>,
    slots: Vec<sys::ArcadiaTioSlotUniverseBindingInput>,
    remap_axes: Vec<Vec<sys::ArcadiaTioUniverseRemapInput>>,
    remap_slots: Vec<sys::ArcadiaTioSlotUniverseRemapInput>,
    _borrowed: PhantomData<&'a AppendWithUniverseOptions>,
}

impl<'a> PreparedAppendUniverseOptions<'a> {
    fn new(options: &'a AppendWithUniverseOptions) -> Self {
        let slot_axes = options
            .slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseBindingInput {
                        axis: axis.axis,
                        family_uuid: axis.family_uuid,
                        version_uuid: axis.version_uuid,
                        length: axis.length,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let slots = slot_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseBindingInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        let remap_axes = options
            .remap_slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseRemapInput {
                        version: 1,
                        struct_size: mem::size_of::<sys::ArcadiaTioUniverseRemapInput>(),
                        axis: axis.axis,
                        target_family_uuid: axis.target_family_uuid,
                        target_version_uuid: axis.target_version_uuid,
                        target_length: axis.target_length,
                        source_to_target: if axis.source_to_target.is_empty() {
                            ptr::null()
                        } else {
                            axis.source_to_target.as_ptr()
                        },
                        source_to_target_len: axis.source_to_target.len(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let remap_slots = remap_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseRemapInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        Self {
            slot_axes,
            slots,
            remap_axes,
            remap_slots,
            _borrowed: PhantomData,
        }
    }

    fn raw_options(&self) -> sys::ArcadiaTioAppendWithUniverseOptions {
        let _ = (&self.slot_axes, &self.remap_axes);
        sys::ArcadiaTioAppendWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioAppendWithUniverseOptions>(),
            slots: if self.slots.is_empty() {
                ptr::null()
            } else {
                self.slots.as_ptr()
            },
            slots_len: self.slots.len(),
            remap_slots: if self.remap_slots.is_empty() {
                ptr::null()
            } else {
                self.remap_slots.as_ptr()
            },
            remap_slots_len: self.remap_slots.len(),
        }
    }
}

struct PreparedSparseRule {
    sparse_axes: Vec<usize>,
    detector_kind: sys::ArcadiaTioSparseDetectorKind,
    predicate: sys::ArcadiaTioSparseValuePredicate,
    predicate_v2: sys::ArcadiaTioSparseValuePredicateV2,
    min_absent_fraction: f64,
    min_absent_subtensors: u64,
    fallback: sys::ArcadiaTioSparseFallbackPolicy,
}

impl PreparedSparseRule {
    fn new(rule: &SparseRule) -> Self {
        Self {
            sparse_axes: rule.sparse_axes.clone(),
            detector_kind: rule.detector.to_raw(),
            predicate: rule.predicate.to_raw(),
            predicate_v2: rule.predicate.to_raw_v2(),
            min_absent_fraction: rule.min_absent_fraction,
            min_absent_subtensors: rule.min_absent_subtensors,
            fallback: rule.fallback.to_raw(),
        }
    }

    fn raw(&self) -> sys::ArcadiaTioSparseRule {
        sys::ArcadiaTioSparseRule {
            detector_kind: self.detector_kind,
            sparse_axes: if self.sparse_axes.is_empty() {
                ptr::null()
            } else {
                self.sparse_axes.as_ptr()
            },
            sparse_axes_len: self.sparse_axes.len(),
            predicate: self.predicate,
            min_absent_fraction: self.min_absent_fraction,
            min_absent_subtensors: self.min_absent_subtensors,
            fallback: self.fallback,
        }
    }

    fn raw_v2(&self) -> sys::ArcadiaTioSparseRuleV2 {
        sys::ArcadiaTioSparseRuleV2 {
            struct_size: mem::size_of::<sys::ArcadiaTioSparseRuleV2>() as u32,
            detector_kind: self.detector_kind,
            sparse_axes: if self.sparse_axes.is_empty() {
                ptr::null()
            } else {
                self.sparse_axes.as_ptr()
            },
            sparse_axes_len: self.sparse_axes.len(),
            predicate: self.predicate_v2,
            min_absent_fraction: self.min_absent_fraction,
            min_absent_subtensors: self.min_absent_subtensors,
            fallback: self.fallback,
        }
    }
}

struct PreparedSingleSelector {
    take_indices: Option<Vec<u32>>,
    selector: sys::ArcadiaTioEntrySelector,
}

impl PreparedSingleSelector {
    fn new(selector: &EntrySelector) -> Result<Self> {
        let (take_indices, selector) = match selector {
            EntrySelector::All => (
                None,
                sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
            ),
            EntrySelector::Range { start, end } => {
                if start > end {
                    return Err(TioError::invalid_argument(
                        "selector range start must be <= end",
                    ));
                }
                (
                    None,
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    },
                )
            }
            EntrySelector::Take(indices) => {
                let values = indices.clone();
                let selector = sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                    start: 0,
                    end: 0,
                    indices: if values.is_empty() {
                        ptr::null()
                    } else {
                        values.as_ptr()
                    },
                    indices_len: values.len(),
                };
                (Some(values), selector)
            }
        };
        Ok(Self {
            take_indices,
            selector,
        })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        &self.selector
    }
}

struct PreparedChunkKeys<'a> {
    keys: &'a [ChunkKey],
    raw: Vec<sys::ArcadiaTioChunkKey>,
}

impl<'a> PreparedChunkKeys<'a> {
    fn new(keys: &'a [ChunkKey]) -> Self {
        let raw = keys
            .iter()
            .map(|key| sys::ArcadiaTioChunkKey {
                coords: if key.coords.is_empty() {
                    ptr::null()
                } else {
                    key.coords.as_ptr()
                },
                len: key.coords.len(),
            })
            .collect();
        Self { keys, raw }
    }

    fn ptr(&self) -> *const sys::ArcadiaTioChunkKey {
        let _ = &self.keys;
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.raw.len()
    }
}

struct PreparedReadIndexItems {
    items: Vec<sys::ArcadiaTioReadIndexItem>,
}

impl PreparedReadIndexItems {
    fn new(items: &[ReadIndexItem], rank: usize) -> Result<Self> {
        let mut ellipsis_count = 0usize;
        let mut consuming = 0usize;
        let mut output_rank_without_ellipsis_fill = 0usize;
        for item in items {
            match item {
                ReadIndexItem::All | ReadIndexItem::Slice { .. } => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Index(_) => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::NewAxis => {
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Ellipsis => {
                    ellipsis_count += 1;
                    if ellipsis_count > 1 {
                        return Err(TioError::invalid_argument(
                            "read_index supports at most one ellipsis",
                        ));
                    }
                }
            }
        }
        if consuming > rank {
            return Err(TioError::invalid_argument(
                "read_index has too many axis-consuming items for file rank",
            ));
        }
        let ellipsis_or_padding_fill = rank - consuming;
        let output_rank = output_rank_without_ellipsis_fill
            .checked_add(ellipsis_or_padding_fill)
            .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
        if output_rank == 0 {
            return Err(TioError::invalid_argument(
                "read_index scalar output is unsupported by the C ABI first slice",
            ));
        }
        let items = items
            .iter()
            .map(ReadIndexItem::to_raw)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { items })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioReadIndexItem {
        if self.items.is_empty() {
            ptr::null()
        } else {
            self.items.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.items.len()
    }
}

struct PreparedSelectors {
    take_indices: Vec<Vec<u32>>,
    selectors: Vec<sys::ArcadiaTioEntrySelector>,
}

impl PreparedSelectors {
    fn new(selectors: &[EntrySelector], rank: usize) -> Result<Self> {
        if selectors.is_empty() {
            return Ok(Self {
                take_indices: Vec::new(),
                selectors: Vec::new(),
            });
        }
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let take_indices = selectors
            .iter()
            .filter_map(|selector| match selector {
                EntrySelector::Take(indices) => Some(indices.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut next_take = 0usize;
        let mut raw = Vec::with_capacity(selectors.len());
        for selector in selectors {
            let item = match selector {
                EntrySelector::All => sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
                EntrySelector::Range { start, end } => {
                    if start > end {
                        return Err(TioError::invalid_argument(
                            "selector range start must be <= end",
                        ));
                    }
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    }
                }
                EntrySelector::Take(_) => {
                    let values = &take_indices[next_take];
                    next_take += 1;
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                        start: 0,
                        end: 0,
                        indices: if values.is_empty() {
                            ptr::null()
                        } else {
                            values.as_ptr()
                        },
                        indices_len: values.len(),
                    }
                }
            };
            raw.push(item);
        }
        Ok(Self {
            take_indices,
            selectors: raw,
        })
    }

    fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        if self.selectors.is_empty() {
            ptr::null()
        } else {
            self.selectors.as_ptr()
        }
    }

    fn len(&self) -> usize {
        self.selectors.len()
    }
}

struct PreparedQueryTraceContext {
    run_id: CString,
    row_id: CString,
    phase: CString,
    language: CString,
    api_surface: CString,
    operation: CString,
    trace_clock: CString,
    repeat_index: u32,
}

impl PreparedQueryTraceContext {
    fn new(context: &QueryTraceContext) -> Result<Self> {
        Ok(Self {
            run_id: non_empty_cstring(&context.run_id, "query trace run_id")?,
            row_id: non_empty_cstring(&context.row_id, "query trace row_id")?,
            phase: non_empty_cstring(&context.phase, "query trace phase")?,
            language: non_empty_cstring(&context.language, "query trace language")?,
            api_surface: non_empty_cstring(&context.api_surface, "query trace api_surface")?,
            operation: non_empty_cstring(&context.operation, "query trace operation")?,
            trace_clock: non_empty_cstring(&context.trace_clock, "query trace trace_clock")?,
            repeat_index: context.repeat_index,
        })
    }

    fn raw_context(&self) -> sys::ArcadiaTioQueryTraceContext {
        sys::ArcadiaTioQueryTraceContext {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceContext>(),
            run_id: self.run_id.as_ptr(),
            row_id: self.row_id.as_ptr(),
            repeat_index: self.repeat_index,
            phase: self.phase.as_ptr(),
            language: self.language.as_ptr(),
            api_surface: self.api_surface.as_ptr(),
            operation: self.operation.as_ptr(),
            trace_clock: self.trace_clock.as_ptr(),
        }
    }
}

fn non_empty_cstring(value: &str, label: &str) -> Result<CString> {
    if value.is_empty() {
        return Err(TioError::invalid_argument(format!(
            "{label} must not be empty"
        )));
    }
    string_to_cstring(value, label)
}

struct PreparedReadWithOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
}

impl PreparedReadWithOptions {
    fn new(options: &ReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadWithOptionsOptions {
        sys::ArcadiaTioReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

struct PreparedHistoricalReadWithOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
}

impl PreparedHistoricalReadWithOptions {
    fn new(options: &HistoricalReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithOptionsOptions {
        sys::ArcadiaTioHistoricalReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

struct PreparedReadShapePolicy {
    explicit_extents: Vec<u64>,
    explicit_universe_axes: Vec<sys::ArcadiaTioExplicitUniverseAxisTarget>,
    explicit_extent_axes: Vec<sys::ArcadiaTioExplicitExtentAxisTarget>,
    policy: sys::ArcadiaTioReadShapePolicyTag,
}

impl PreparedReadShapePolicy {
    fn new(policy: &ReadShapePolicy) -> Self {
        let explicit_extents = match policy {
            ReadShapePolicy::ExplicitExtents(extents) => extents.clone(),
            _ => Vec::new(),
        };
        let explicit_universe_axes = match policy {
            ReadShapePolicy::ExplicitUniverse(axes) => axes.iter().map(raw_universe_axis).collect(),
            ReadShapePolicy::ExplicitUniverseAndExtents { universe_axes, .. } => {
                universe_axes.iter().map(raw_universe_axis).collect()
            }
            _ => Vec::new(),
        };
        let explicit_extent_axes = match policy {
            ReadShapePolicy::ExplicitUniverseAndExtents { extent_axes, .. } => extent_axes
                .iter()
                .map(|axis| sys::ArcadiaTioExplicitExtentAxisTarget {
                    axis: axis.axis,
                    length: axis.length,
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            explicit_extents,
            explicit_universe_axes,
            explicit_extent_axes,
            policy: policy.to_raw_tag(),
        }
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadShapePolicyOptions {
        sys::ArcadiaTioReadShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadShapePolicyOptions>(),
            policy: self.policy,
            explicit_extents: if self.explicit_extents.is_empty() {
                ptr::null()
            } else {
                self.explicit_extents.as_ptr()
            },
            explicit_extents_len: self.explicit_extents.len(),
            explicit_universe_axes: if self.explicit_universe_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_universe_axes.as_ptr()
            },
            explicit_universe_axes_len: self.explicit_universe_axes.len(),
            explicit_extent_axes: if self.explicit_extent_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_extent_axes.as_ptr()
            },
            explicit_extent_axes_len: self.explicit_extent_axes.len(),
        }
    }
}

fn raw_universe_axis(
    axis: &ExplicitUniverseAxisTarget,
) -> sys::ArcadiaTioExplicitUniverseAxisTarget {
    sys::ArcadiaTioExplicitUniverseAxisTarget {
        axis: axis.axis,
        family_uuid: axis.family_uuid,
        version_uuid: axis.version_uuid,
        length: axis.length,
    }
}

struct PreparedReadWithShapePolicyOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
    shape_policy: PreparedReadShapePolicy,
}

impl PreparedReadWithShapePolicyOptions {
    fn new(options: &ReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    fn raw_options(&self) -> sys::ArcadiaTioReadWithShapePolicyOptions {
        sys::ArcadiaTioReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

struct PreparedHistoricalReadWithShapePolicyOptions {
    mode: sys::ArcadiaTioReadExecutionMode,
    max_threads: usize,
    shape_policy: PreparedReadShapePolicy,
}

impl PreparedHistoricalReadWithShapePolicyOptions {
    fn new(options: &HistoricalReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
        sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

fn coordinate_input(
    coord: &CoordinateSpec,
    name: Option<&CString>,
    external_uri: Option<&CString>,
) -> sys::ArcadiaTioAxisCoordinateInput {
    let (
        storage_kind,
        external_source_kind,
        external_uri_ptr,
        external_dtype,
        external_length,
        values_ptr,
        values_len,
        dtype,
    ) = match &coord.storage {
        CoordinateStorage::Inline(values) => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_INLINE,
            sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            ptr::null(),
            values.dtype().to_raw(),
            0,
            values.as_ptr(),
            values.len(),
            values.dtype(),
        ),
        CoordinateStorage::External {
            source_kind,
            uri: _,
            dtype,
            length,
        } => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL,
            source_kind.to_raw(),
            external_uri.map_or(ptr::null(), |value| value.as_ptr()),
            dtype.to_raw(),
            *length,
            ptr::null(),
            0,
            *dtype,
        ),
    };
    sys::ArcadiaTioAxisCoordinateInput {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioAxisCoordinateInput>(),
        axis: coord.axis,
        name: name.map_or(ptr::null(), |value| value.as_ptr()),
        kind: coord.kind.to_raw(),
        dtype: dtype.to_raw(),
        encoding: coord.encoding.to_raw(),
        values: values_ptr,
        values_len,
        sorted: coord.ordering.sorted.to_raw(),
        monotonicity: coord.ordering.monotonicity.to_raw(),
        uniqueness: coord.ordering.uniqueness.to_raw(),
        storage_kind,
        external_source_kind,
        external_uri: external_uri_ptr,
        external_dtype,
        external_length,
        required: u8::from(coord.required),
    }
}

/// Safe Rust wrappers for the appendable OCB (Ordered Column Bundle) C ABI.
#[cfg(feature = "format-ocb")]
pub mod ocb {
    use super::{ErrorCode, TioError, path_to_cstring};
    use arcadia_tio_sys as sys;
    use std::cmp::Ordering as CmpOrdering;
    use std::collections::BTreeSet;
    use std::ffi::{CStr, CString};
    use std::fmt;
    use std::marker::PhantomData;
    use std::mem;
    use std::os::raw::{c_char, c_void};
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::path::Path;
    use std::ptr::{self, NonNull};
    use std::slice;
    use std::time::Instant;

    /// Result type returned by OCB safe wrappers.
    pub type OcbResult<T> = std::result::Result<T, OcbError>;

    /// OCB validation depth used while opening a selected snapshot.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OpenValidation {
        /// Validate metadata graph and chunk headers; payload CRCs are checked when selected chunks are read.
        MetadataGraph,
        /// Validate every referenced payload before returning from open.
        FullPayload,
    }

    impl OpenValidation {
        fn to_raw(self) -> sys::ArcadiaTioOcbOpenValidation {
            match self {
                Self::MetadataGraph => sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH,
                Self::FullPayload => sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_FULL_PAYLOAD,
            }
        }
    }

    /// OCB open options.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct OpenOptions {
        /// Validation depth applied before open returns.
        pub validation: OpenValidation,
    }

    impl Default for OpenOptions {
        fn default() -> Self {
            Self {
                validation: OpenValidation::MetadataGraph,
            }
        }
    }

    /// Structured OCB error with the ordinary C ABI code plus OCB-specific metadata.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OcbError {
        code: ErrorCode,
        kind: ErrorKind,
        cause: Option<FailureCause>,
        message: String,
    }

    impl OcbError {
        fn last(fallback: &str) -> Self {
            let raw_code = unsafe { sys::arcadia_tio_last_error_code() };
            let raw_kind = unsafe { sys::arcadia_tio_ocb_last_error_kind() };
            let raw_cause = unsafe { sys::arcadia_tio_ocb_last_error_cause() };
            let raw_message = unsafe { sys::arcadia_tio_last_error_message() };
            let message = if raw_message.is_null() {
                fallback.to_string()
            } else {
                unsafe { CStr::from_ptr(raw_message) }
                    .to_string_lossy()
                    .into_owned()
            };
            Self {
                code: ErrorCode::from_raw(raw_code),
                kind: ErrorKind::from_raw(raw_kind),
                cause: FailureCause::from_raw(raw_cause),
                message,
            }
        }

        fn invalid_input(message: impl Into<String>) -> Self {
            Self {
                code: ErrorCode::InvalidArgument,
                kind: ErrorKind::InvalidInput,
                cause: Some(FailureCause::InvalidInput),
                message: message.into(),
            }
        }

        fn from_tio_error(err: TioError) -> Self {
            Self {
                code: err.code(),
                kind: ErrorKind::InvalidInput,
                cause: Some(FailureCause::InvalidInput),
                message: err.message().to_string(),
            }
        }

        /// Ordinary C ABI error code.
        pub fn code(&self) -> ErrorCode {
            self.code
        }

        /// OCB-specific error kind.
        pub fn kind(&self) -> ErrorKind {
            self.kind
        }

        /// OCB-specific failure cause when present.
        pub fn cause(&self) -> Option<FailureCause> {
            self.cause
        }

        /// Human diagnostic message.
        pub fn message(&self) -> &str {
            &self.message
        }
    }

    impl fmt::Display for OcbError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.message)
        }
    }

    impl std::error::Error for OcbError {}

    /// OCB structured error kind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ErrorKind {
        /// No OCB error.
        None,
        /// Invalid caller input.
        InvalidInput,
        /// Unsupported format or option.
        UnsupportedFormat,
        /// Corrupt file contents.
        CorruptFile,
        /// Lock was unavailable.
        LockUnavailable,
        /// Low-level I/O failure.
        Io,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl ErrorKind {
        fn from_raw(raw: sys::ArcadiaTioOcbErrorKind) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_ERROR_KIND_NONE => Self::None,
                sys::ARCADIA_TIO_OCB_ERROR_KIND_INVALID_INPUT => Self::InvalidInput,
                sys::ARCADIA_TIO_OCB_ERROR_KIND_UNSUPPORTED_FORMAT => Self::UnsupportedFormat,
                sys::ARCADIA_TIO_OCB_ERROR_KIND_CORRUPT_FILE => Self::CorruptFile,
                sys::ARCADIA_TIO_OCB_ERROR_KIND_LOCK_UNAVAILABLE => Self::LockUnavailable,
                sys::ARCADIA_TIO_OCB_ERROR_KIND_IO => Self::Io,
                other => Self::Unknown(other),
            }
        }
    }

    /// OCB structured failure cause.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FailureCause {
        /// Invalid caller input.
        InvalidInput,
        /// Unsupported OCB format.
        UnsupportedFormat,
        /// Corrupt file contents.
        CorruptFile,
        /// Lock was unavailable.
        LockUnavailable,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl FailureCause {
        fn from_raw(raw: sys::ArcadiaTioOcbFailureCause) -> Option<Self> {
            match raw {
                sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_NONE => None,
                sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_INVALID_INPUT => Some(Self::InvalidInput),
                sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_UNSUPPORTED_FORMAT => {
                    Some(Self::UnsupportedFormat)
                }
                sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_CORRUPT_FILE => Some(Self::CorruptFile),
                sys::ARCADIA_TIO_OCB_FAILURE_CAUSE_LOCK_UNAVAILABLE => Some(Self::LockUnavailable),
                other => Some(Self::Unknown(other)),
            }
        }
    }

    /// Generic OCB body-object kind recorded by a summary body reference.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum BodyKind {
        Unknown,
        Root,
        Schema,
        DictionaryIndex,
        DictionaryValues,
        RowGroupIndex,
        OrderingProof,
        ColumnChunk,
        StringTable,
        DebugJsonMetadata,
        ValidityBitmap,
        KeyTuple,
        RowGroupIndexDelta,
        Other(i32),
    }

    impl BodyKind {
        fn from_raw(raw: sys::ArcadiaTioOcbBodyKind) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_BODY_KIND_UNKNOWN => Self::Unknown,
                sys::ARCADIA_TIO_OCB_BODY_KIND_ROOT => Self::Root,
                sys::ARCADIA_TIO_OCB_BODY_KIND_SCHEMA => Self::Schema,
                sys::ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_INDEX => Self::DictionaryIndex,
                sys::ARCADIA_TIO_OCB_BODY_KIND_DICTIONARY_VALUES => Self::DictionaryValues,
                sys::ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX => Self::RowGroupIndex,
                sys::ARCADIA_TIO_OCB_BODY_KIND_ORDERING_PROOF => Self::OrderingProof,
                sys::ARCADIA_TIO_OCB_BODY_KIND_COLUMN_CHUNK => Self::ColumnChunk,
                sys::ARCADIA_TIO_OCB_BODY_KIND_STRING_TABLE => Self::StringTable,
                sys::ARCADIA_TIO_OCB_BODY_KIND_DEBUG_JSON_METADATA => Self::DebugJsonMetadata,
                sys::ARCADIA_TIO_OCB_BODY_KIND_VALIDITY_BITMAP => Self::ValidityBitmap,
                sys::ARCADIA_TIO_OCB_BODY_KIND_KEY_TUPLE => Self::KeyTuple,
                sys::ARCADIA_TIO_OCB_BODY_KIND_ROW_GROUP_INDEX_DELTA => Self::RowGroupIndexDelta,
                other => Self::Other(other),
            }
        }
    }

    /// Generic OCB checksum kind recorded by a body reference.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ChecksumKind {
        None,
        Crc32c,
        Other(i32),
    }

    impl ChecksumKind {
        fn from_raw(raw: sys::ArcadiaTioOcbChecksumKind) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_CHECKSUM_KIND_NONE => Self::None,
                sys::ARCADIA_TIO_OCB_CHECKSUM_KIND_CRC32C => Self::Crc32c,
                other => Self::Other(other),
            }
        }
    }

    /// Generic column-chunk summary codec.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum ColumnChunkSummaryCodec {
        None,
        Zstd,
        Other(i32),
    }

    impl ColumnChunkSummaryCodec {
        fn from_raw(raw: sys::ArcadiaTioOcbColumnChunkSummaryCodec) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_NONE => Self::None,
                sys::ARCADIA_TIO_OCB_COLUMN_CHUNK_SUMMARY_CODEC_ZSTD => Self::Zstd,
                other => Self::Other(other),
            }
        }
    }

    /// OCB writer chunk codec.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum WriteChunkCodec {
        None,
        Zstd { level: i32 },
    }

    impl WriteChunkCodec {
        fn to_raw(self) -> (sys::ArcadiaTioOcbWriteChunkCodec, i32) {
            match self {
                Self::None => (sys::ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_NONE, 3),
                Self::Zstd { level } => (sys::ARCADIA_TIO_OCB_WRITE_CHUNK_CODEC_ZSTD, level),
            }
        }
    }

    /// OCB write options for create/append.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WriteOptions {
        pub write_threads: usize,
        pub chunk_codec: WriteChunkCodec,
    }

    impl Default for WriteOptions {
        fn default() -> Self {
            Self {
                write_threads: 1,
                chunk_codec: WriteChunkCodec::None,
            }
        }
    }

    impl WriteOptions {
        pub fn zstd(level: i32) -> Self {
            Self {
                write_threads: 1,
                chunk_codec: WriteChunkCodec::Zstd { level },
            }
        }

        pub fn with_write_threads(mut self, write_threads: usize) -> Self {
            self.write_threads = write_threads;
            self
        }

        fn to_raw(self) -> sys::ArcadiaTioOcbWriteOptions {
            let (chunk_codec, zstd_level) = self.chunk_codec.to_raw();
            sys::ArcadiaTioOcbWriteOptions {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteOptions>(),
                write_threads: self.write_threads,
                chunk_codec,
                zstd_level,
                reserved: [0; 4],
            }
        }
    }

    /// Primitive physical type supported by OCB columns.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PhysicalType {
        /// 32-bit signed integer.
        I32,
        /// 64-bit signed integer.
        I64,
        /// 32-bit float.
        F32,
        /// 64-bit float.
        F64,
        /// Fixed-width opaque byte values.
        FixedBinary {
            /// Number of bytes in each row value.
            width: u32,
        },
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl PhysicalType {
        fn to_raw(self) -> sys::ArcadiaTioOcbPhysicalType {
            match self {
                Self::I32 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
                Self::I64 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64,
                Self::F32 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32,
                Self::F64 => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64,
                Self::FixedBinary { .. } => sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY,
                Self::Unknown(raw) => raw,
            }
        }

        fn from_raw(raw: sys::ArcadiaTioOcbPhysicalType) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32 => Self::I32,
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64 => Self::I64,
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32 => Self::F32,
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64 => Self::F64,
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY => Self::FixedBinary { width: 0 },
                other => Self::Unknown(other),
            }
        }

        fn from_raw_with_width(raw: sys::ArcadiaTioOcbPhysicalType, width: u32) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY => Self::FixedBinary { width },
                _ => Self::from_raw(raw),
            }
        }

        fn fixed_binary_width(self) -> u32 {
            match self {
                Self::FixedBinary { width } => width,
                _ => 0,
            }
        }
    }

    /// Logical kind attached to an OCB column.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum LogicalKind {
        /// Plain primitive values.
        Plain,
        /// Timestamp-nanos-like integer values.
        TimestampNanosLike,
        /// Scaled integer values.
        ScaledInteger,
        /// Dictionary-code values.
        DictionaryCode,
        /// Enum-code values.
        EnumCode,
        /// Opaque ordering key values.
        OpaqueKey,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl LogicalKind {
        fn to_raw(self) -> sys::ArcadiaTioOcbLogicalKind {
            match self {
                Self::Plain => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN,
                Self::TimestampNanosLike => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_TIMESTAMP_NANOS_LIKE,
                Self::ScaledInteger => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_SCALED_INTEGER,
                Self::DictionaryCode => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_DICTIONARY_CODE,
                Self::EnumCode => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_ENUM_CODE,
                Self::OpaqueKey => sys::ARCADIA_TIO_OCB_LOGICAL_KIND_OPAQUE_KEY,
                Self::Unknown(raw) => raw,
            }
        }

        fn from_raw(raw: sys::ArcadiaTioOcbLogicalKind) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN => Self::Plain,
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_TIMESTAMP_NANOS_LIKE => Self::TimestampNanosLike,
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_SCALED_INTEGER => Self::ScaledInteger,
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_DICTIONARY_CODE => Self::DictionaryCode,
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_ENUM_CODE => Self::EnumCode,
                sys::ARCADIA_TIO_OCB_LOGICAL_KIND_OPAQUE_KEY => Self::OpaqueKey,
                other => Self::Unknown(other),
            }
        }
    }

    /// OCB dictionary decoded value kind.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DictionaryValueKind {
        /// UTF-8 string values.
        Utf8,
        /// Variable-length byte values.
        Bytes,
        /// Fixed-width byte values.
        FixedBytes,
        /// Enum-label string values.
        EnumLabels,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl DictionaryValueKind {
        fn to_raw(self) -> sys::ArcadiaTioOcbDictionaryValueKind {
            match self {
                Self::Utf8 => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8,
                Self::Bytes => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_BYTES,
                Self::FixedBytes => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_FIXED_BYTES,
                Self::EnumLabels => sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_ENUM_LABELS,
                Self::Unknown(raw) => raw,
            }
        }

        fn from_raw(raw: sys::ArcadiaTioOcbDictionaryValueKind) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8 => Self::Utf8,
                sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_BYTES => Self::Bytes,
                sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_FIXED_BYTES => Self::FixedBytes,
                sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_ENUM_LABELS => Self::EnumLabels,
                other => Self::Unknown(other),
            }
        }
    }

    /// Ordering direction for OCB ordering keys.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum OrderingDirection {
        /// Ascending order.
        Ascending,
        /// Descending order.
        Descending,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl OrderingDirection {
        fn to_raw(self) -> sys::ArcadiaTioOcbOrderingDirection {
            match self {
                Self::Ascending => sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_ASCENDING,
                Self::Descending => sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_DESCENDING,
                Self::Unknown(raw) => raw,
            }
        }

        fn from_raw(raw: sys::ArcadiaTioOcbOrderingDirection) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_ASCENDING => Self::Ascending,
                sys::ARCADIA_TIO_OCB_ORDERING_DIRECTION_DESCENDING => Self::Descending,
                other => Self::Unknown(other),
            }
        }
    }

    /// Null ordering for OCB ordering keys.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum NullOrder {
        /// Nulls compare first.
        NullsFirst,
        /// Nulls compare last.
        NullsLast,
        /// Column has no nulls for this ordering key.
        NoNulls,
        /// Unknown forward-compatible raw value.
        Unknown(i32),
    }

    impl NullOrder {
        fn to_raw(self) -> sys::ArcadiaTioOcbNullOrder {
            match self {
                Self::NullsFirst => sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_FIRST,
                Self::NullsLast => sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_LAST,
                Self::NoNulls => sys::ARCADIA_TIO_OCB_NULL_ORDER_NO_NULLS,
                Self::Unknown(raw) => raw,
            }
        }

        fn from_raw(raw: sys::ArcadiaTioOcbNullOrder) -> Self {
            match raw {
                sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_FIRST => Self::NullsFirst,
                sys::ARCADIA_TIO_OCB_NULL_ORDER_NULLS_LAST => Self::NullsLast,
                sys::ARCADIA_TIO_OCB_NULL_ORDER_NO_NULLS => Self::NoNulls,
                other => Self::Unknown(other),
            }
        }
    }

    /// OCB primitive payload values.
    #[derive(Debug, Clone, PartialEq)]
    pub enum PrimitiveValues {
        /// i32 values.
        I32(Vec<i32>),
        /// i64 values.
        I64(Vec<i64>),
        /// f32 values.
        F32(Vec<f32>),
        /// f64 values.
        F64(Vec<f64>),
        /// Fixed-width opaque bytes stored contiguously row-major.
        FixedBinary {
            /// Number of bytes in each row value.
            width: u32,
            /// Contiguous row-major bytes.
            bytes: Vec<u8>,
        },
    }

    impl PrimitiveValues {
        fn physical_type(&self) -> PhysicalType {
            match self {
                Self::I32(_) => PhysicalType::I32,
                Self::I64(_) => PhysicalType::I64,
                Self::F32(_) => PhysicalType::F32,
                Self::F64(_) => PhysicalType::F64,
                Self::FixedBinary { width, .. } => PhysicalType::FixedBinary { width: *width },
            }
        }

        fn len(&self) -> usize {
            match self {
                Self::I32(values) => values.len(),
                Self::I64(values) => values.len(),
                Self::F32(values) => values.len(),
                Self::F64(values) => values.len(),
                Self::FixedBinary { width: 0, .. } => 0,
                Self::FixedBinary { width, bytes } => bytes.len() / *width as usize,
            }
        }

        fn data_ptr(&self) -> *const c_void {
            match self {
                Self::I32(values) => values.as_ptr().cast(),
                Self::I64(values) => values.as_ptr().cast(),
                Self::F32(values) => values.as_ptr().cast(),
                Self::F64(values) => values.as_ptr().cast(),
                Self::FixedBinary { bytes, .. } => bytes.as_ptr().cast(),
            }
        }

        fn to_raw(&self) -> sys::ArcadiaTioOcbPrimitiveValues {
            sys::ArcadiaTioOcbPrimitiveValues {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbPrimitiveValues>(),
                physical_type: self.physical_type().to_raw(),
                data: self.data_ptr(),
                len: self.len(),
                reserved: [u64::from(self.physical_type().fixed_binary_width()), 0, 0],
            }
        }
    }

    /// Validity bitmap with least-significant-bit-first bits; bit 1 means valid.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ValidityBitmap {
        /// Bitmap bytes.
        pub bytes: Vec<u8>,
        /// Meaningful row count.
        pub row_count: u64,
    }

    /// Column declaration for OCB create/append.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WriteColumn {
        /// Column name.
        pub name: String,
        /// Physical primitive type.
        pub physical_type: PhysicalType,
        /// Logical column kind.
        pub logical_kind: LogicalKind,
        /// Optional dictionary id for dictionary-coded columns.
        pub dictionary_id: Option<u32>,
        /// Decimal scale for scaled-integer logical columns.
        pub scale: i32,
        /// Whether values may be null.
        pub nullable: bool,
    }

    /// Dictionary declaration for OCB create/append.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct WriteDictionary {
        /// File-local dictionary id.
        pub dictionary_id: u32,
        /// Dictionary name.
        pub name: String,
        /// Physical code type used by dictionary-coded columns.
        pub code_physical_type: PhysicalType,
        /// Decoded value kind.
        pub value_kind: DictionaryValueKind,
        /// Fixed byte width when `value_kind` is fixed bytes.
        pub fixed_width: u32,
        /// Decoded dictionary entries as bytes.
        pub entries: Vec<Vec<u8>>,
    }

    /// Column chunk for one OCB write row group.
    #[derive(Debug, Clone, PartialEq)]
    pub struct WriteColumnChunk {
        /// File-local column id.
        pub column_id: u32,
        /// Primitive values.
        pub values: PrimitiveValues,
        /// Optional validity bitmap.
        pub validity: Option<ValidityBitmap>,
    }

    /// Row group for OCB create/append.
    #[derive(Debug, Clone, PartialEq)]
    pub struct WriteRowGroup {
        /// Column chunks in this row group.
        pub columns: Vec<WriteColumnChunk>,
    }

    /// Ordering key declaration for OCB create/append.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct WriteOrderingKey {
        /// File-local column id.
        pub column_id: u32,
        /// Sort direction.
        pub direction: OrderingDirection,
        /// Null ordering.
        pub null_order: NullOrder,
    }

    /// Complete OCB write specification for create/append.
    #[derive(Debug, Clone, PartialEq)]
    pub struct WriteSpec {
        /// Frozen column schema.
        pub columns: Vec<WriteColumn>,
        /// Frozen dictionary declarations.
        pub dictionaries: Vec<WriteDictionary>,
        /// Row groups to publish in this commit.
        pub row_groups: Vec<WriteRowGroup>,
        /// Ordering keys.
        pub ordering_keys: Vec<WriteOrderingKey>,
    }

    /// Metadata for an opened OCB snapshot.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Metadata {
        /// Format name, currently `OCB`.
        pub format_name: String,
        /// Whether the format is appendable.
        pub appendable: bool,
        /// Selected root generation.
        pub root_generation: u64,
        /// Previous root generation when present.
        pub previous_root_generation: Option<u64>,
        /// Visible row count.
        pub row_count: u64,
        /// Visible row group count.
        pub row_group_count: u32,
        /// Visible column chunk count.
        pub column_chunk_count: u32,
        /// Column descriptors.
        pub columns: Vec<ColumnDescriptor>,
        /// Dictionary descriptors.
        pub dictionaries: Vec<DictionaryDescriptor>,
        /// Ordering keys.
        pub ordering_keys: Vec<OrderingKey>,
    }

    /// OCB column metadata descriptor.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ColumnDescriptor {
        /// File-local column id.
        pub id: u32,
        /// Column name.
        pub name: String,
        /// Physical type.
        pub physical_type: PhysicalType,
        /// Logical kind.
        pub logical_kind: LogicalKind,
        /// Dictionary id when present.
        pub dictionary_id: Option<u32>,
        /// Decimal scale.
        pub scale: i32,
        /// Whether values may be null.
        pub nullable: bool,
    }

    /// OCB dictionary metadata descriptor.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DictionaryDescriptor {
        /// File-local dictionary id.
        pub dictionary_id: u32,
        /// Dictionary name.
        pub name: String,
        /// Code physical type.
        pub code_physical_type: PhysicalType,
        /// Decoded value kind.
        pub value_kind: DictionaryValueKind,
        /// Number of entries.
        pub entry_count: u32,
    }

    /// OCB ordering-key metadata descriptor.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct OrderingKey {
        /// File-local column id.
        pub column_id: u32,
        /// Column name.
        pub column_name: String,
        /// Sort direction.
        pub direction: OrderingDirection,
        /// Null ordering.
        pub null_order: NullOrder,
    }

    /// Decoded dictionary values.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct DictionaryValues {
        /// File-local dictionary id.
        pub dictionary_id: u32,
        /// Dictionary name.
        pub name: String,
        /// Decoded values.
        pub values: DecodedDictionaryValues,
    }

    /// Decoded dictionary payload variants.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum DecodedDictionaryValues {
        /// UTF-8 string values.
        Utf8(Vec<String>),
        /// Variable-length byte values.
        Bytes(Vec<Vec<u8>>),
        /// Fixed-width byte values.
        FixedBytes {
            fixed_width: u32,
            values: Vec<Vec<u8>>,
        },
        /// Enum-label string values.
        EnumLabels(Vec<String>),
        /// Unknown value kind with copied raw strings/bytes when possible.
        Unknown {
            raw_kind: i32,
            strings: Vec<String>,
            bytes: Vec<Vec<u8>>,
        },
    }

    /// Column projection for OCB reads.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Projection {
        /// Select all columns.
        All,
        /// Select columns by name.
        Names(Vec<String>),
    }

    /// Predicate primitive value.
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub enum PredicateValue {
        /// i32 value.
        I32(i32),
        /// i64 value.
        I64(i64),
        /// f32 value.
        F32(f32),
        /// f64 value.
        F64(f64),
    }

    impl PredicateValue {
        fn physical_type(self) -> PhysicalType {
            match self {
                Self::I32(_) => PhysicalType::I32,
                Self::I64(_) => PhysicalType::I64,
                Self::F32(_) => PhysicalType::F32,
                Self::F64(_) => PhysicalType::F64,
            }
        }

        fn cmp_same_type(self, other: Self) -> OcbResult<CmpOrdering> {
            match (self, other) {
                (Self::I32(left), Self::I32(right)) => Ok(left.cmp(&right)),
                (Self::I64(left), Self::I64(right)) => Ok(left.cmp(&right)),
                (Self::F32(left), Self::F32(right)) => left.partial_cmp(&right).ok_or_else(|| {
                    OcbError::invalid_input("OCB f32 predicate/stat value cannot be NaN")
                }),
                (Self::F64(left), Self::F64(right)) => left.partial_cmp(&right).ok_or_else(|| {
                    OcbError::invalid_input("OCB f64 predicate/stat value cannot be NaN")
                }),
                _ => Err(OcbError::invalid_input("OCB predicate/stat type mismatch")),
            }
        }

        fn to_raw(self) -> sys::ArcadiaTioOcbPredicateValue {
            let mut raw = sys::ArcadiaTioOcbPredicateValue {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbPredicateValue>(),
                physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
                i32_value: 0,
                i64_value: 0,
                f32_value: 0.0,
                f64_value: 0.0,
                reserved: [0; 3],
            };
            match self {
                Self::I32(value) => {
                    raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32;
                    raw.i32_value = value;
                }
                Self::I64(value) => {
                    raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64;
                    raw.i64_value = value;
                }
                Self::F32(value) => {
                    raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32;
                    raw.f32_value = value;
                }
                Self::F64(value) => {
                    raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64;
                    raw.f64_value = value;
                }
            }
            raw
        }
    }

    /// Inclusive row-group predicate over one column.
    #[derive(Debug, Clone, PartialEq)]
    pub struct RowGroupPredicate {
        /// Column name.
        pub column: String,
        /// Inclusive lower bound when present.
        pub lower: Option<PredicateValue>,
        /// Inclusive upper bound when present.
        pub upper: Option<PredicateValue>,
    }

    /// Scalar bounds for one declared OCB ordering-key column.
    ///
    /// This is a row-group pruning helper, not a row-level filter or
    /// lexicographic cursor engine. For composite ordering declarations,
    /// multiple ranges become ordinary conjunctive predicates and may include
    /// extra rows that callers should filter outside OCB if exact row-level
    /// semantics are required.
    #[derive(Debug, Clone, PartialEq)]
    pub struct OrderingKeyRange {
        /// Zero-based index into [`Metadata::ordering_keys`].
        pub key_index: usize,
        /// Inclusive scalar lower bound.
        pub lower: Option<PredicateValue>,
        /// Inclusive scalar upper bound.
        pub upper: Option<PredicateValue>,
    }

    impl OrderingKeyRange {
        /// Create a range with optional inclusive bounds.
        pub fn new(
            key_index: usize,
            lower: Option<PredicateValue>,
            upper: Option<PredicateValue>,
        ) -> Self {
            Self {
                key_index,
                lower,
                upper,
            }
        }

        /// Create a closed inclusive range.
        pub fn between(key_index: usize, lower: PredicateValue, upper: PredicateValue) -> Self {
            Self::new(key_index, Some(lower), Some(upper))
        }

        /// Create an equality range.
        pub fn equal(key_index: usize, value: PredicateValue) -> Self {
            Self::new(key_index, Some(value), Some(value))
        }
    }

    /// OCB read request.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ReadRequest {
        /// Column projection.
        pub projection: Projection,
        /// Row-group pruning predicates.
        pub predicates: Vec<RowGroupPredicate>,
        /// Requested worker threads.
        pub max_threads: usize,
        /// Validate checksums while reading.
        pub validate_checksums: bool,
        /// Reserved dictionary decode flag; current reads still return codes.
        pub decode_dictionaries: bool,
    }

    impl Default for ReadRequest {
        fn default() -> Self {
            Self {
                projection: Projection::All,
                predicates: Vec::new(),
                max_threads: 1,
                validate_checksums: true,
                decode_dictionaries: false,
            }
        }
    }

    impl ReadRequest {
        /// Build a read request from inclusive scalar bounds over declared
        /// ordering-key columns.
        ///
        /// The returned request contains ordinary row-group pruning predicates
        /// and default read options. It does not add row-level filtering or
        /// lexicographic cursor semantics.
        pub fn from_ordering_key_ranges(
            metadata: &Metadata,
            projection: Projection,
            ranges: Vec<OrderingKeyRange>,
        ) -> OcbResult<Self> {
            Self {
                projection,
                ..Self::default()
            }
            .with_ordering_key_ranges(metadata, ranges)
        }

        /// Replace this request's predicates with predicates derived from
        /// ordering-key ranges while preserving projection and read options.
        pub fn with_ordering_key_ranges(
            mut self,
            metadata: &Metadata,
            ranges: Vec<OrderingKeyRange>,
        ) -> OcbResult<Self> {
            self.predicates = ordering_key_range_predicates(metadata, ranges)?;
            Ok(self)
        }
    }

    fn ordering_key_range_predicates(
        metadata: &Metadata,
        ranges: Vec<OrderingKeyRange>,
    ) -> OcbResult<Vec<RowGroupPredicate>> {
        if ranges.is_empty() {
            return Err(OcbError::invalid_input(
                "OCB ordering range request requires at least one bound",
            ));
        }
        let mut ranges = ranges;
        ranges.sort_by_key(|range| range.key_index);
        let mut seen = BTreeSet::new();
        let mut predicates = Vec::with_capacity(ranges.len());
        for range in ranges {
            if !seen.insert(range.key_index) {
                return Err(OcbError::invalid_input(
                    "OCB ordering range request contains duplicate key indexes",
                ));
            }
            if range.lower.is_none() && range.upper.is_none() {
                return Err(OcbError::invalid_input(
                    "OCB ordering range bound must include at least one side",
                ));
            }
            let key = metadata.ordering_keys.get(range.key_index).ok_or_else(|| {
                OcbError::invalid_input("OCB ordering range references an unknown ordering key")
            })?;
            let column = metadata
                .columns
                .iter()
                .find(|column| column.id == key.column_id)
                .ok_or_else(|| {
                    OcbError::invalid_input("OCB ordering key column is missing from metadata")
                })?;
            if matches!(column.physical_type, PhysicalType::FixedBinary { .. }) {
                return Err(OcbError::invalid_input(
                    "OCB ordering range over fixed-binary columns is not supported",
                ));
            }
            for bound in [range.lower, range.upper].into_iter().flatten() {
                if bound.physical_type() != column.physical_type {
                    return Err(OcbError::invalid_input(
                        "OCB ordering range bound dtype does not match ordering column dtype",
                    ));
                }
            }
            if let (Some(lower), Some(upper)) = (range.lower, range.upper) {
                if lower.cmp_same_type(upper)? == CmpOrdering::Greater {
                    return Err(OcbError::invalid_input(
                        "OCB ordering range lower bound is greater than upper bound",
                    ));
                }
            }
            predicates.push(RowGroupPredicate {
                column: key.column_name.clone(),
                lower: range.lower,
                upper: range.upper,
            });
        }
        Ok(predicates)
    }

    /// Snapshot-local OCB read plan.
    ///
    /// A plan contains generic projected column ids and selected row-group ids.
    /// Row-group ids are file-local/snapshot-local, not stable business ids.
    #[derive(Debug)]
    pub struct ReadPlan<'a> {
        raw: NonNull<sys::ArcadiaTioOcbReadPlan>,
        file_raw: NonNull<sys::ArcadiaTioOcbFile>,
        /// File-local column ids selected by the projection.
        pub projected_column_ids: Vec<u32>,
        /// File-local row-group ids selected by predicates.
        pub row_group_ids: Vec<u32>,
        /// Planning report for the request.
        pub report: ReadReport,
        _file: PhantomData<&'a ColumnBundleFile>,
    }

    /// OCB read outcome with owned batches and report.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ReadOutcome {
        /// Returned column batches.
        pub batches: Vec<ColumnBatch>,
        /// Read execution report.
        pub report: ReadReport,
    }

    /// OCB read outcome with opt-in diagnostics.
    #[derive(Debug, Clone, PartialEq)]
    pub struct AttributedReadOutcome {
        /// Returned column batches and ordinary read report.
        pub outcome: ReadOutcome,
        /// Diagnostic attribution counters. These are not benchmark claims.
        pub attribution: ReadAttribution,
    }

    /// Generic OCB body reference summary.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct BodyRefSummary {
        pub offset: u64,
        pub length: u64,
        pub kind: BodyKind,
        pub flags: u16,
        pub checksum_kind: ChecksumKind,
        pub checksum: u32,
    }

    /// Generic OCB column chunk summary.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ColumnChunkSummary {
        pub row_group_id: u32,
        pub column_id: u32,
        pub column_name: String,
        pub physical_type: PhysicalType,
        pub logical_kind: LogicalKind,
        pub fixed_binary_width: Option<u32>,
        pub codec: ColumnChunkSummaryCodec,
        pub row_count: u64,
        pub compressed_bytes: u64,
        pub uncompressed_bytes: u64,
        pub value_ref: BodyRefSummary,
        pub validity_ref: Option<BodyRefSummary>,
    }

    /// Generic OCB scalar stats summary.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ColumnStatsSummary {
        pub row_group_id: u32,
        pub column_id: u32,
        pub column_name: String,
        pub physical_type: PhysicalType,
        pub null_count: u32,
        pub min: PredicateValue,
        pub max: PredicateValue,
    }

    /// Generic OCB row-group summary.
    #[derive(Debug, Clone, PartialEq)]
    pub struct RowGroupSummary {
        pub row_group_id: u32,
        pub base_row: u64,
        pub row_count: u64,
        pub first_key_tuple_ref: Option<BodyRefSummary>,
        pub last_key_tuple_ref: Option<BodyRefSummary>,
        pub chunks: Vec<ColumnChunkSummary>,
        pub stats: Vec<ColumnStatsSummary>,
    }

    /// Visitor return control for bounded OCB reads.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum VisitControl {
        /// Continue visiting batches.
        Continue,
        /// Stop after the current batch.
        Stop,
    }

    /// Options for bounded visitor-style OCB reads.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ReadCursorOptions {
        /// Maximum decoded row-group batches in flight.
        pub max_in_flight_row_groups: usize,
        /// Preserve deterministic row-group order. Unordered mode is reserved.
        pub ordered: bool,
    }

    impl Default for ReadCursorOptions {
        fn default() -> Self {
            Self {
                max_in_flight_row_groups: 1,
                ordered: true,
            }
        }
    }

    /// Report for visitor-style OCB reads.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ReadCursorReport {
        pub base_report: ReadReport,
        pub batches_yielded: usize,
        pub rows_yielded: u64,
        pub cancelled: bool,
    }

    /// Options for caller-owned single-row-group fill reads.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct ReadFillOptions {
        /// Validate checksums while reading. Current native OCB reads remain fail-closed.
        pub validate_checksums: bool,
    }

    impl Default for ReadFillOptions {
        fn default() -> Self {
            Self {
                validate_checksums: true,
            }
        }
    }

    /// Caller-owned typed storage for one OCB column fill.
    #[derive(Debug)]
    pub enum ColumnFillBufferMut<'a> {
        I32 {
            name: &'a str,
            values: &'a mut [i32],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        I32ById {
            column_id: u32,
            values: &'a mut [i32],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        I64 {
            name: &'a str,
            values: &'a mut [i64],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        I64ById {
            column_id: u32,
            values: &'a mut [i64],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        F32 {
            name: &'a str,
            values: &'a mut [f32],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        F32ById {
            column_id: u32,
            values: &'a mut [f32],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        F64 {
            name: &'a str,
            values: &'a mut [f64],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        F64ById {
            column_id: u32,
            values: &'a mut [f64],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        FixedBinary {
            name: &'a str,
            width: u32,
            bytes: &'a mut [u8],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
        FixedBinaryById {
            column_id: u32,
            width: u32,
            bytes: &'a mut [u8],
            validity: Option<&'a mut [u8]>,
            allow_nulls: bool,
        },
    }

    /// Per-column caller-owned fill result.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ColumnFillReport {
        pub column_id: u32,
        pub rows_filled: usize,
        pub validity_filled: bool,
    }

    /// Caller-owned single-row-group fill report.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ReadFillReport {
        pub row_group_id: u32,
        pub base_row: u64,
        pub row_count: u64,
        pub columns: Vec<ColumnFillReport>,
    }

    /// OCB read attribution diagnostics.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ReadAttribution {
        pub plan_ns: u64,
        pub execute_wall_ns: u64,
        pub row_group_read_ns: u64,
        pub read_io_ns: u64,
        pub checksum_ns: u64,
        pub decompression_ns: u64,
        pub primitive_decode_ns: u64,
        pub native_to_c_copy_ns: Option<u64>,
        pub wrapper_copy_ns: Option<u64>,
        pub bytes_read: u64,
        pub compressed_bytes: u64,
        pub uncompressed_bytes: u64,
        pub requested_threads: usize,
        pub effective_threads: usize,
        pub selected_row_groups: usize,
        pub pruned_row_groups: usize,
        pub selected_column_chunks: usize,
        pub fallback_reason: Option<String>,
    }

    /// OCB read execution report.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct ReadReport {
        /// Requested thread count.
        pub requested_threads: usize,
        /// Effective thread count.
        pub effective_threads: usize,
        /// Selected row groups.
        pub selected_row_groups: usize,
        /// Pruned row groups.
        pub pruned_row_groups: usize,
        /// Selected column chunks.
        pub selected_column_chunks: usize,
        /// Stable fallback reason string when present.
        pub fallback_reason: Option<String>,
    }

    /// One returned OCB row-group batch.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ColumnBatch {
        /// File-local row group id.
        pub row_group_id: u32,
        /// Base row offset.
        pub base_row: u64,
        /// Number of rows.
        pub row_count: u64,
        /// Returned columns.
        pub columns: Vec<ColumnArray>,
    }

    /// One returned OCB column array.
    #[derive(Debug, Clone, PartialEq)]
    pub struct ColumnArray {
        /// File-local column id.
        pub column_id: u32,
        /// Column name.
        pub name: String,
        /// Physical type.
        pub physical_type: PhysicalType,
        /// Logical kind.
        pub logical_kind: LogicalKind,
        /// Dictionary id when present.
        pub dictionary_id: Option<u32>,
        /// Primitive values.
        pub values: PrimitiveValues,
        /// Optional validity bitmap.
        pub validity: Option<ValidityBitmap>,
    }

    /// Result from orphan-tail cleanup.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct CleanupResult {
        /// Whether orphan tail bytes were truncated.
        pub truncated: bool,
    }

    /// OCB file handle bound to one selected committed snapshot.
    #[derive(Debug)]
    pub struct ColumnBundleFile {
        raw: NonNull<sys::ArcadiaTioOcbFile>,
    }

    impl ColumnBundleFile {
        /// Open an OCB file and bind this handle to the selected committed snapshot.
        pub fn open(path: impl AsRef<Path>) -> OcbResult<Self> {
            open(path)
        }

        /// Open an OCB file with explicit validation options.
        pub fn open_with_options(path: impl AsRef<Path>, options: OpenOptions) -> OcbResult<Self> {
            open_with_options(path, options)
        }

        /// Clone this selected-snapshot reader handle.
        ///
        /// The clone observes the same immutable committed OCB snapshot as this
        /// handle. Reopen the file path to observe later appends.
        pub fn clone_reader(&self) -> OcbResult<Self> {
            let mut raw_reader = ptr::null_mut();
            let status =
                unsafe { sys::arcadia_tio_ocb_reader_clone(self.raw.as_ptr(), &mut raw_reader) };
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB reader_clone failed"));
            }
            NonNull::new(raw_reader)
                .map(|raw| ColumnBundleFile { raw })
                .ok_or_else(|| OcbError::last("OCB reader_clone returned null reader"))
        }

        /// Read metadata for the selected snapshot.
        pub fn metadata(&self) -> OcbResult<Metadata> {
            let mut raw = empty_metadata();
            let status = unsafe { sys::arcadia_tio_ocb_metadata(self.raw.as_ptr(), &mut raw) };
            let guard = MetadataGuard(raw);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB metadata failed"));
            }
            unsafe { metadata_from_raw(&guard.0) }
        }

        /// Decode one dictionary on the explicit cold path.
        pub fn dictionary_values(&self, dictionary_id: u32) -> OcbResult<DictionaryValues> {
            let mut raw = empty_dictionary_values();
            let status = unsafe {
                sys::arcadia_tio_ocb_dictionary_values(self.raw.as_ptr(), dictionary_id, &mut raw)
            };
            let guard = DictionaryValuesGuard(raw);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB dictionary_values failed"));
            }
            unsafe { dictionary_values_from_raw(&guard.0) }
        }

        /// Read projected/pruned column batches from the selected snapshot.
        pub fn read_batches(&self, request: &ReadRequest) -> OcbResult<ReadOutcome> {
            let raw_request = RawReadRequest::new(request)?;
            let mut raw_outcome = empty_read_outcome();
            let status = unsafe {
                sys::arcadia_tio_ocb_read_batches(
                    self.raw.as_ptr(),
                    &raw_request.raw,
                    &mut raw_outcome,
                )
            };
            let guard = ReadOutcomeGuard(raw_outcome);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB read_batches failed"));
            }
            unsafe { read_outcome_from_raw(&guard.0) }
        }

        /// Read projected/pruned column batches and collect diagnostic attribution.
        pub fn read_batches_with_attribution(
            &self,
            request: &ReadRequest,
        ) -> OcbResult<AttributedReadOutcome> {
            let raw_request = RawReadRequest::new(request)?;
            let mut raw_outcome = empty_read_outcome();
            let mut raw_attribution = empty_read_attribution();
            let status = unsafe {
                sys::arcadia_tio_ocb_read_batches_with_attribution(
                    self.raw.as_ptr(),
                    &raw_request.raw,
                    &mut raw_outcome,
                    &mut raw_attribution,
                )
            };
            let outcome_guard = ReadOutcomeGuard(raw_outcome);
            let attribution_guard = ReadAttributionGuard(raw_attribution);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB read_batches_with_attribution failed"));
            }
            let wrapper_started = Instant::now();
            let outcome = unsafe { read_outcome_from_raw(&outcome_guard.0) }?;
            let mut attribution = read_attribution_from_raw(&attribution_guard.0);
            attribution.wrapper_copy_ns = Some(duration_to_ns(wrapper_started.elapsed()));
            Ok(AttributedReadOutcome {
                outcome,
                attribution,
            })
        }

        /// Return generic metadata summaries for every visible row group.
        pub fn row_group_summaries(&self) -> OcbResult<Vec<RowGroupSummary>> {
            let mut raw = empty_row_group_summaries();
            let status =
                unsafe { sys::arcadia_tio_ocb_row_group_summaries(self.raw.as_ptr(), &mut raw) };
            let guard = RowGroupSummariesGuard(raw);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB row_group_summaries failed"));
            }
            unsafe { row_group_summaries_from_raw(&guard.0) }
        }

        /// Return generic summaries for row groups selected by a read plan.
        pub fn read_plan_row_group_summaries(
            &self,
            plan: &ReadPlan<'_>,
        ) -> OcbResult<Vec<RowGroupSummary>> {
            let mut raw = empty_row_group_summaries();
            let status = unsafe {
                sys::arcadia_tio_ocb_read_plan_row_group_summaries(
                    self.raw.as_ptr(),
                    plan.raw.as_ptr(),
                    &mut raw,
                )
            };
            let guard = RowGroupSummariesGuard(raw);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB read_plan_row_group_summaries failed"));
            }
            unsafe { row_group_summaries_from_raw(&guard.0) }
        }

        /// Visit projected/pruned row-group batches incrementally.
        ///
        /// Each callback receives an owned Rust `ColumnBatch` copied from the
        /// native callback view. Internal native materialization is bounded by
        /// `options.max_in_flight_row_groups`.
        pub fn visit_batches<F>(
            &self,
            request: &ReadRequest,
            options: ReadCursorOptions,
            mut visitor: F,
        ) -> OcbResult<ReadCursorReport>
        where
            F: FnMut(ColumnBatch) -> OcbResult<VisitControl>,
        {
            let raw_request = RawReadRequest::new(request)?;
            let raw_options = raw_read_cursor_options(options);
            let mut raw_report = empty_read_cursor_report();
            let mut callback = VisitCallback {
                visitor: &mut visitor,
            };
            let status = unsafe {
                sys::arcadia_tio_ocb_visit_batches(
                    self.raw.as_ptr(),
                    &raw_request.raw,
                    &raw_options,
                    Some(visit_trampoline::<F>),
                    (&mut callback as *mut VisitCallback<'_, F>).cast(),
                    &mut raw_report,
                )
            };
            let report_guard = ReadCursorReportGuard(raw_report);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB visit_batches failed"));
            }
            Ok(read_cursor_report_from_raw(&report_guard.0))
        }

        /// Read one row group directly into caller-owned typed column buffers.
        ///
        /// This avoids constructing an owned `ReadOutcome`. On error, caller
        /// buffers may be partially written and should be discarded.
        pub fn read_row_group_into(
            &self,
            row_group_id: u32,
            buffers: &mut [ColumnFillBufferMut<'_>],
            options: ReadFillOptions,
        ) -> OcbResult<ReadFillReport> {
            let raw = RawFillRequest::new(row_group_id, buffers, options)?;
            let mut raw_report = empty_read_fill_report();
            let status = unsafe {
                sys::arcadia_tio_ocb_read_row_group_into(
                    self.raw.as_ptr(),
                    &raw.raw,
                    &mut raw_report,
                )
            };
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB read_row_group_into failed"));
            }
            Ok(read_fill_report_from_raw(&raw_report, &raw.raw_columns))
        }

        /// Plan a projected/pruned read without reading column payloads.
        pub fn plan_read<'a>(&'a self, request: &ReadRequest) -> OcbResult<ReadPlan<'a>> {
            let raw_request = RawReadRequest::new(request)?;
            let mut raw_plan = ptr::null_mut();
            let status = unsafe {
                sys::arcadia_tio_ocb_plan_read(self.raw.as_ptr(), &raw_request.raw, &mut raw_plan)
            };
            let raw = NonNull::new(raw_plan);
            if status != sys::ARCADIA_TIO_ERROR_OK {
                return Err(OcbError::last("OCB plan_read failed"));
            }
            let raw = raw.ok_or_else(|| OcbError::last("OCB plan_read returned null plan"))?;
            let report = match read_plan_report(raw) {
                Ok(report) => report,
                Err(err) => {
                    unsafe { sys::arcadia_tio_ocb_read_plan_free(raw.as_ptr()) };
                    return Err(err);
                }
            };
            let projected_column_ids = match read_plan_projected_column_ids(raw) {
                Ok(ids) => ids,
                Err(err) => {
                    unsafe { sys::arcadia_tio_ocb_read_plan_free(raw.as_ptr()) };
                    return Err(err);
                }
            };
            let row_group_ids = match read_plan_row_group_ids(raw) {
                Ok(ids) => ids,
                Err(err) => {
                    unsafe { sys::arcadia_tio_ocb_read_plan_free(raw.as_ptr()) };
                    return Err(err);
                }
            };
            Ok(ReadPlan {
                raw,
                file_raw: self.raw,
                projected_column_ids,
                row_group_ids,
                report,
                _file: PhantomData,
            })
        }

        /// Execute all row groups selected by a read plan.
        pub fn read_plan_batches(&self, plan: &ReadPlan<'_>) -> OcbResult<ReadOutcome> {
            ensure_plan_belongs_to_file(self.raw, plan)?;
            read_batches_from_plan(self.raw, plan.raw, None)
        }

        /// Execute an explicit row-group subset selected by a read plan.
        ///
        /// Unknown or duplicate row-group ids fail closed. Returned batches use
        /// deterministic plan order rather than caller-supplied subset order.
        pub fn read_plan_row_groups(
            &self,
            plan: &ReadPlan<'_>,
            row_group_ids: &[u32],
        ) -> OcbResult<ReadOutcome> {
            ensure_plan_belongs_to_file(self.raw, plan)?;
            read_batches_from_plan(self.raw, plan.raw, Some(row_group_ids))
        }
    }

    impl Drop for ColumnBundleFile {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_close(self.raw.as_ptr()) };
        }
    }

    // SAFETY: Native OCB handles are immutable selected-snapshot readers. Read
    // calls do not mutate shared handle state and open independent file objects
    // for payload I/O. Safe Rust ownership prevents dropping a handle while it is
    // borrowed by another read call.
    unsafe impl Send for ColumnBundleFile {}
    // SAFETY: See the Send impl above; concurrent read-only calls on a selected
    // snapshot handle are supported by the C ABI contract.
    unsafe impl Sync for ColumnBundleFile {}

    impl Drop for ReadPlan<'_> {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_read_plan_free(self.raw.as_ptr()) };
        }
    }

    /// Open an OCB file and bind the returned handle to the selected committed snapshot.
    pub fn open(path: impl AsRef<Path>) -> OcbResult<ColumnBundleFile> {
        open_with_options(path, OpenOptions::default())
    }

    /// Open an OCB file with explicit validation options.
    pub fn open_with_options(
        path: impl AsRef<Path>,
        options: OpenOptions,
    ) -> OcbResult<ColumnBundleFile> {
        let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
        let mut raw_options = sys::ArcadiaTioOcbOpenOptions {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbOpenOptions>(),
            validation: options.validation.to_raw(),
            reserved: [0; 4],
        };
        unsafe { sys::arcadia_tio_ocb_open_options_init(&mut raw_options) };
        raw_options.validation = options.validation.to_raw();
        let raw = unsafe { sys::arcadia_tio_ocb_open_with_options(path.as_ptr(), &raw_options) };
        NonNull::new(raw)
            .map(|raw| ColumnBundleFile { raw })
            .ok_or_else(|| OcbError::last("OCB open failed"))
    }

    /// Create an appendable OCB file and publish its first committed root.
    pub fn create(path: impl AsRef<Path>, spec: &WriteSpec) -> OcbResult<()> {
        write_path(path, spec, true, None)
    }

    /// Create an appendable OCB file with explicit writer options.
    pub fn create_with_options(
        path: impl AsRef<Path>,
        spec: &WriteSpec,
        options: WriteOptions,
    ) -> OcbResult<()> {
        write_path(path, spec, true, Some(options))
    }

    /// Append one sorted suffix commit to an existing appendable OCB file.
    pub fn append(path: impl AsRef<Path>, spec: &WriteSpec) -> OcbResult<()> {
        write_path(path, spec, false, None)
    }

    /// Append one sorted suffix commit with explicit writer options.
    pub fn append_with_options(
        path: impl AsRef<Path>,
        spec: &WriteSpec,
        options: WriteOptions,
    ) -> OcbResult<()> {
        write_path(path, spec, false, Some(options))
    }

    /// Truncate orphan tail bytes after the latest valid appendable OCB root.
    pub fn cleanup_orphan_tail(path: impl AsRef<Path>) -> OcbResult<CleanupResult> {
        let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
        let mut raw = sys::ArcadiaTioOcbCleanupResult {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbCleanupResult>(),
            truncated: 0,
            reserved: [0; 3],
        };
        let status = unsafe { sys::arcadia_tio_ocb_cleanup_orphan_tail(path.as_ptr(), &mut raw) };
        if status == sys::ARCADIA_TIO_ERROR_OK {
            Ok(CleanupResult {
                truncated: raw.truncated != 0,
            })
        } else {
            Err(OcbError::last("OCB cleanup_orphan_tail failed"))
        }
    }

    fn write_path(
        path: impl AsRef<Path>,
        spec: &WriteSpec,
        create_file: bool,
        options: Option<WriteOptions>,
    ) -> OcbResult<()> {
        let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
        let raw = RawWriteSpec::new(spec)?;
        let raw_options = options.map(WriteOptions::to_raw);
        let status = match (create_file, raw_options.as_ref()) {
            (true, Some(options)) => unsafe {
                sys::arcadia_tio_ocb_create_with_options(path.as_ptr(), &raw.raw, options)
            },
            (false, Some(options)) => unsafe {
                sys::arcadia_tio_ocb_append_with_options(path.as_ptr(), &raw.raw, options)
            },
            (true, None) => unsafe { sys::arcadia_tio_ocb_create(path.as_ptr(), &raw.raw) },
            (false, None) => unsafe { sys::arcadia_tio_ocb_append(path.as_ptr(), &raw.raw) },
        };
        if status == sys::ARCADIA_TIO_ERROR_OK {
            Ok(())
        } else {
            Err(OcbError::last(if create_file {
                "OCB create failed"
            } else {
                "OCB append failed"
            }))
        }
    }

    struct RawWriteSpec {
        raw: sys::ArcadiaTioOcbWriteSpec,
        _column_names: Vec<CString>,
        _dictionary_names: Vec<CString>,
        _raw_columns: Vec<sys::ArcadiaTioOcbWriteColumn>,
        _raw_entries: Vec<Vec<sys::ArcadiaTioOcbDictionaryEntry>>,
        _raw_dictionaries: Vec<sys::ArcadiaTioOcbWriteDictionary>,
        _raw_validities: Vec<Vec<sys::ArcadiaTioOcbValidityBitmap>>,
        _raw_chunks: Vec<Vec<sys::ArcadiaTioOcbWriteColumnChunk>>,
        _raw_rows: Vec<sys::ArcadiaTioOcbWriteRowGroup>,
        _raw_ordering_keys: Vec<sys::ArcadiaTioOcbWriteOrderingKey>,
    }

    impl RawWriteSpec {
        fn new(spec: &WriteSpec) -> OcbResult<Self> {
            let column_names = spec
                .columns
                .iter()
                .map(|column| cstring(&column.name, "OCB column name"))
                .collect::<OcbResult<Vec<_>>>()?;
            let dictionary_names = spec
                .dictionaries
                .iter()
                .map(|dictionary| cstring(&dictionary.name, "OCB dictionary name"))
                .collect::<OcbResult<Vec<_>>>()?;

            validate_write_spec(spec)?;

            let raw_columns = spec
                .columns
                .iter()
                .zip(column_names.iter())
                .map(|(column, name)| sys::ArcadiaTioOcbWriteColumn {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteColumn>(),
                    name: name.as_ptr(),
                    physical_type: column.physical_type.to_raw(),
                    logical_kind: column.logical_kind.to_raw(),
                    has_dictionary_id: u8::from(column.dictionary_id.is_some()),
                    dictionary_id: column.dictionary_id.unwrap_or(0),
                    scale: column.scale,
                    nullable: u8::from(column.nullable),
                    reserved: [u64::from(column.physical_type.fixed_binary_width()), 0, 0],
                })
                .collect::<Vec<_>>();

            let raw_entries = spec
                .dictionaries
                .iter()
                .map(|dictionary| {
                    dictionary
                        .entries
                        .iter()
                        .map(|entry| sys::ArcadiaTioOcbDictionaryEntry {
                            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                            struct_size: mem::size_of::<sys::ArcadiaTioOcbDictionaryEntry>(),
                            data: entry.as_ptr(),
                            len: entry.len(),
                            reserved: [0; 3],
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            let raw_dictionaries = spec
                .dictionaries
                .iter()
                .zip(dictionary_names.iter())
                .zip(raw_entries.iter())
                .map(
                    |((dictionary, name), entries)| sys::ArcadiaTioOcbWriteDictionary {
                        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                        struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteDictionary>(),
                        dictionary_id: dictionary.dictionary_id,
                        name: name.as_ptr(),
                        code_physical_type: dictionary.code_physical_type.to_raw(),
                        value_kind: dictionary.value_kind.to_raw(),
                        fixed_width: dictionary.fixed_width,
                        entries: entries.as_ptr(),
                        entries_len: entries.len(),
                        reserved: [0; 3],
                    },
                )
                .collect::<Vec<_>>();

            let raw_validities = spec
                .row_groups
                .iter()
                .map(|row| {
                    row.columns
                        .iter()
                        .map(|chunk| match &chunk.validity {
                            Some(validity) => sys::ArcadiaTioOcbValidityBitmap {
                                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                                struct_size: mem::size_of::<sys::ArcadiaTioOcbValidityBitmap>(),
                                data: validity.bytes.as_ptr(),
                                len: validity.bytes.len(),
                                row_count: validity.row_count,
                                reserved: [0; 3],
                            },
                            None => sys::ArcadiaTioOcbValidityBitmap {
                                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                                struct_size: mem::size_of::<sys::ArcadiaTioOcbValidityBitmap>(),
                                data: ptr::null(),
                                len: 0,
                                row_count: 0,
                                reserved: [0; 3],
                            },
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            let raw_chunks = spec
                .row_groups
                .iter()
                .zip(raw_validities.iter())
                .map(|(row, validities)| {
                    row.columns
                        .iter()
                        .zip(validities.iter())
                        .map(|(chunk, validity)| sys::ArcadiaTioOcbWriteColumnChunk {
                            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                            struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteColumnChunk>(),
                            column_id: chunk.column_id,
                            values: chunk.values.to_raw(),
                            validity: if chunk.validity.is_some() {
                                validity as *const sys::ArcadiaTioOcbValidityBitmap
                            } else {
                                ptr::null()
                            },
                            reserved: [0; 3],
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();

            let raw_rows = raw_chunks
                .iter()
                .map(|chunks| sys::ArcadiaTioOcbWriteRowGroup {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteRowGroup>(),
                    columns: chunks.as_ptr(),
                    columns_len: chunks.len(),
                    reserved: [0; 3],
                })
                .collect::<Vec<_>>();

            let raw_ordering_keys = spec
                .ordering_keys
                .iter()
                .map(|key| sys::ArcadiaTioOcbWriteOrderingKey {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteOrderingKey>(),
                    column_id: key.column_id,
                    direction: key.direction.to_raw(),
                    null_order: key.null_order.to_raw(),
                    reserved: [0; 3],
                })
                .collect::<Vec<_>>();

            let raw = sys::ArcadiaTioOcbWriteSpec {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteSpec>(),
                columns: raw_columns.as_ptr(),
                columns_len: raw_columns.len(),
                dictionaries: raw_dictionaries.as_ptr(),
                dictionaries_len: raw_dictionaries.len(),
                row_groups: raw_rows.as_ptr(),
                row_groups_len: raw_rows.len(),
                ordering_keys: raw_ordering_keys.as_ptr(),
                ordering_keys_len: raw_ordering_keys.len(),
                reserved: [0; 4],
            };

            Ok(Self {
                raw,
                _column_names: column_names,
                _dictionary_names: dictionary_names,
                _raw_columns: raw_columns,
                _raw_entries: raw_entries,
                _raw_dictionaries: raw_dictionaries,
                _raw_validities: raw_validities,
                _raw_chunks: raw_chunks,
                _raw_rows: raw_rows,
                _raw_ordering_keys: raw_ordering_keys,
            })
        }
    }

    fn validate_write_spec(spec: &WriteSpec) -> OcbResult<()> {
        for (column_idx, column) in spec.columns.iter().enumerate() {
            if let PhysicalType::FixedBinary { width } = column.physical_type {
                if width == 0 {
                    return Err(OcbError::invalid_input(format!(
                        "OCB fixed-binary column {} ('{}') has zero width",
                        column_idx, column.name
                    )));
                }
                if column.logical_kind == LogicalKind::DictionaryCode {
                    return Err(OcbError::invalid_input(format!(
                        "OCB fixed-binary column {} ('{}') cannot be a dictionary-code column",
                        column_idx, column.name
                    )));
                }
            }
        }

        for (row_idx, row) in spec.row_groups.iter().enumerate() {
            for chunk in &row.columns {
                let Some(column) = spec.columns.get(chunk.column_id as usize) else {
                    continue;
                };
                match (&chunk.values, column.physical_type) {
                    (
                        PrimitiveValues::FixedBinary { width, bytes },
                        PhysicalType::FixedBinary {
                            width: schema_width,
                        },
                    ) => {
                        if *width == 0 {
                            return Err(OcbError::invalid_input(format!(
                                "OCB fixed-binary chunk row group {row_idx} column {} has zero width",
                                chunk.column_id
                            )));
                        }
                        if *width != schema_width {
                            return Err(OcbError::invalid_input(format!(
                                "OCB fixed-binary chunk row group {row_idx} column {} width {} does not match schema width {}",
                                chunk.column_id, width, schema_width
                            )));
                        }
                        if bytes.len() % *width as usize != 0 {
                            return Err(OcbError::invalid_input(format!(
                                "OCB fixed-binary chunk row group {row_idx} column {} byte length {} is not divisible by width {}",
                                chunk.column_id,
                                bytes.len(),
                                width
                            )));
                        }
                    }
                    (PrimitiveValues::FixedBinary { .. }, _) => {
                        return Err(OcbError::invalid_input(format!(
                            "OCB fixed-binary chunk row group {row_idx} column {} targets a non-fixed-binary schema column",
                            chunk.column_id
                        )));
                    }
                    (_, PhysicalType::FixedBinary { .. }) => {
                        return Err(OcbError::invalid_input(format!(
                            "OCB chunk row group {row_idx} column {} must provide fixed-binary values",
                            chunk.column_id
                        )));
                    }
                    _ => {}
                }
            }
        }

        Ok(())
    }

    struct RawReadRequest {
        raw: sys::ArcadiaTioOcbReadRequest,
        _column_names: Vec<CString>,
        _column_name_ptrs: Vec<*const c_char>,
        _predicate_columns: Vec<CString>,
        _predicates: Vec<sys::ArcadiaTioOcbRowGroupPredicate>,
    }

    impl RawReadRequest {
        fn new(request: &ReadRequest) -> OcbResult<Self> {
            let (column_names, column_name_ptrs, projection_kind) = match &request.projection {
                Projection::All => (Vec::new(), Vec::new(), sys::ARCADIA_TIO_OCB_PROJECTION_ALL),
                Projection::Names(names) => {
                    let column_names = names
                        .iter()
                        .map(|name| cstring(name, "OCB projection column"))
                        .collect::<OcbResult<Vec<_>>>()?;
                    let column_name_ptrs = column_names.iter().map(|name| name.as_ptr()).collect();
                    (
                        column_names,
                        column_name_ptrs,
                        sys::ARCADIA_TIO_OCB_PROJECTION_NAMES,
                    )
                }
            };
            let predicate_columns = request
                .predicates
                .iter()
                .map(|predicate| cstring(&predicate.column, "OCB predicate column"))
                .collect::<OcbResult<Vec<_>>>()?;
            let predicates = request
                .predicates
                .iter()
                .zip(predicate_columns.iter())
                .map(|(predicate, column)| sys::ArcadiaTioOcbRowGroupPredicate {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupPredicate>(),
                    column: column.as_ptr(),
                    has_lower: u8::from(predicate.lower.is_some()),
                    lower: predicate.lower.unwrap_or(PredicateValue::I32(0)).to_raw(),
                    has_upper: u8::from(predicate.upper.is_some()),
                    upper: predicate.upper.unwrap_or(PredicateValue::I32(0)).to_raw(),
                    reserved: [0; 3],
                })
                .collect::<Vec<_>>();
            let raw = sys::ArcadiaTioOcbReadRequest {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbReadRequest>(),
                projection_kind,
                column_names: column_name_ptrs.as_ptr(),
                column_names_len: column_name_ptrs.len(),
                predicates: predicates.as_ptr(),
                predicates_len: predicates.len(),
                max_threads: request.max_threads,
                validate_checksums: u8::from(request.validate_checksums),
                decode_dictionaries: u8::from(request.decode_dictionaries),
                reserved: [0; 4],
            };
            Ok(Self {
                raw,
                _column_names: column_names,
                _column_name_ptrs: column_name_ptrs,
                _predicate_columns: predicate_columns,
                _predicates: predicates,
            })
        }
    }

    struct RawFillRequest {
        raw: sys::ArcadiaTioOcbRowGroupFillRequest,
        _column_names: Vec<CString>,
        raw_columns: Vec<sys::ArcadiaTioOcbColumnFillBuffer>,
    }

    impl RawFillRequest {
        fn new(
            row_group_id: u32,
            buffers: &mut [ColumnFillBufferMut<'_>],
            options: ReadFillOptions,
        ) -> OcbResult<Self> {
            if buffers.is_empty() {
                return Err(OcbError::invalid_input(
                    "OCB fill request requires at least one column buffer",
                ));
            }
            let mut column_names = Vec::new();
            let mut selectors = Vec::with_capacity(buffers.len());
            for buffer in buffers.iter() {
                match column_fill_selector(buffer)? {
                    RawColumnFillSelector::Name(name) => {
                        column_names.push(cstring(name, "OCB fill column")?);
                        selectors.push((column_names.last().expect("just pushed").as_ptr(), None));
                    }
                    RawColumnFillSelector::Id(column_id) => {
                        selectors.push((ptr::null(), Some(column_id)));
                    }
                }
            }
            let mut raw_columns = buffers
                .iter_mut()
                .zip(selectors.iter().copied())
                .map(|(buffer, (name, column_id))| raw_column_fill_buffer(buffer, name, column_id))
                .collect::<Vec<_>>();
            let raw = sys::ArcadiaTioOcbRowGroupFillRequest {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupFillRequest>(),
                row_group_id,
                columns: raw_columns.as_mut_ptr(),
                columns_len: raw_columns.len(),
                validate_checksums: u8::from(options.validate_checksums),
                reserved: [0; 8],
            };
            Ok(Self {
                raw,
                _column_names: column_names,
                raw_columns,
            })
        }
    }

    enum RawColumnFillSelector<'a> {
        Name(&'a str),
        Id(u32),
    }

    fn column_fill_selector<'a>(
        buffer: &ColumnFillBufferMut<'a>,
    ) -> OcbResult<RawColumnFillSelector<'a>> {
        match buffer {
            ColumnFillBufferMut::I32 { name, .. }
            | ColumnFillBufferMut::I64 { name, .. }
            | ColumnFillBufferMut::F32 { name, .. }
            | ColumnFillBufferMut::F64 { name, .. }
            | ColumnFillBufferMut::FixedBinary { name, .. } => {
                Ok(RawColumnFillSelector::Name(name))
            }
            ColumnFillBufferMut::I32ById { column_id, .. }
            | ColumnFillBufferMut::I64ById { column_id, .. }
            | ColumnFillBufferMut::F32ById { column_id, .. }
            | ColumnFillBufferMut::F64ById { column_id, .. }
            | ColumnFillBufferMut::FixedBinaryById { column_id, .. } => {
                Ok(RawColumnFillSelector::Id(*column_id))
            }
        }
    }

    fn raw_column_fill_buffer(
        buffer: &mut ColumnFillBufferMut<'_>,
        name: *const c_char,
        column_id: Option<u32>,
    ) -> sys::ArcadiaTioOcbColumnFillBuffer {
        let mut raw = sys::ArcadiaTioOcbColumnFillBuffer {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnFillBuffer>(),
            column_name: name,
            column_id: column_id.unwrap_or(0),
            has_column_id: u8::from(column_id.is_some()),
            physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
            values: ptr::null_mut(),
            values_len: 0,
            validity_bytes: ptr::null_mut(),
            validity_bytes_len: 0,
            allow_nulls: 0,
            rows_filled: 0,
            validity_filled: 0,
            reserved: [0; 8],
        };
        match buffer {
            ColumnFillBufferMut::I32 {
                values,
                validity,
                allow_nulls,
                ..
            }
            | ColumnFillBufferMut::I32ById {
                values,
                validity,
                allow_nulls,
                ..
            } => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32;
                raw.values = values.as_mut_ptr().cast();
                raw.values_len = values.len();
                set_raw_validity(&mut raw, validity);
                raw.allow_nulls = u8::from(*allow_nulls);
            }
            ColumnFillBufferMut::I64 {
                values,
                validity,
                allow_nulls,
                ..
            }
            | ColumnFillBufferMut::I64ById {
                values,
                validity,
                allow_nulls,
                ..
            } => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64;
                raw.values = values.as_mut_ptr().cast();
                raw.values_len = values.len();
                set_raw_validity(&mut raw, validity);
                raw.allow_nulls = u8::from(*allow_nulls);
            }
            ColumnFillBufferMut::F32 {
                values,
                validity,
                allow_nulls,
                ..
            }
            | ColumnFillBufferMut::F32ById {
                values,
                validity,
                allow_nulls,
                ..
            } => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32;
                raw.values = values.as_mut_ptr().cast();
                raw.values_len = values.len();
                set_raw_validity(&mut raw, validity);
                raw.allow_nulls = u8::from(*allow_nulls);
            }
            ColumnFillBufferMut::F64 {
                values,
                validity,
                allow_nulls,
                ..
            }
            | ColumnFillBufferMut::F64ById {
                values,
                validity,
                allow_nulls,
                ..
            } => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64;
                raw.values = values.as_mut_ptr().cast();
                raw.values_len = values.len();
                set_raw_validity(&mut raw, validity);
                raw.allow_nulls = u8::from(*allow_nulls);
            }
            ColumnFillBufferMut::FixedBinary {
                width,
                bytes,
                validity,
                allow_nulls,
                ..
            }
            | ColumnFillBufferMut::FixedBinaryById {
                width,
                bytes,
                validity,
                allow_nulls,
                ..
            } => {
                raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY;
                raw.values = bytes.as_mut_ptr().cast();
                raw.values_len = bytes.len();
                raw.reserved[0] = u64::from(*width);
                set_raw_validity(&mut raw, validity);
                raw.allow_nulls = u8::from(*allow_nulls);
            }
        }
        raw
    }

    fn set_raw_validity(
        raw: &mut sys::ArcadiaTioOcbColumnFillBuffer,
        validity: &mut Option<&mut [u8]>,
    ) {
        if let Some(bytes) = validity.as_deref_mut() {
            raw.validity_bytes = bytes.as_mut_ptr();
            raw.validity_bytes_len = bytes.len();
        }
    }

    fn cstring(value: &str, label: &str) -> OcbResult<CString> {
        CString::new(value).map_err(|_| OcbError::invalid_input(format!("{label} contains NUL")))
    }

    fn empty_metadata() -> sys::ArcadiaTioOcbMetadata {
        sys::ArcadiaTioOcbMetadata {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbMetadata>(),
            format_name: ptr::null_mut(),
            appendable: 0,
            root_generation: 0,
            has_previous_root_generation: 0,
            previous_root_generation: 0,
            row_count: 0,
            row_group_count: 0,
            column_chunk_count: 0,
            columns: ptr::null_mut(),
            columns_len: 0,
            dictionaries: ptr::null_mut(),
            dictionaries_len: 0,
            ordering_keys: ptr::null_mut(),
            ordering_keys_len: 0,
            reserved: [0; 4],
        }
    }

    fn empty_dictionary_values() -> sys::ArcadiaTioOcbDictionaryValues {
        sys::ArcadiaTioOcbDictionaryValues {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbDictionaryValues>(),
            dictionary_id: 0,
            name: ptr::null_mut(),
            value_kind: sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8,
            fixed_width: 0,
            string_values: ptr::null_mut(),
            string_values_len: 0,
            byte_values: ptr::null_mut(),
            byte_values_len: 0,
            reserved: [0; 4],
        }
    }

    fn empty_read_report() -> sys::ArcadiaTioOcbReadReport {
        sys::ArcadiaTioOcbReadReport {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadReport>(),
            requested_threads: 0,
            effective_threads: 0,
            selected_row_groups: 0,
            pruned_row_groups: 0,
            selected_column_chunks: 0,
            fallback_reason: ptr::null_mut(),
            reserved: [0; 4],
        }
    }

    fn raw_read_cursor_options(options: ReadCursorOptions) -> sys::ArcadiaTioOcbReadCursorOptions {
        sys::ArcadiaTioOcbReadCursorOptions {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadCursorOptions>(),
            max_in_flight_row_groups: options.max_in_flight_row_groups,
            ordered: u8::from(options.ordered),
            reserved: [0; 8],
        }
    }

    fn empty_read_cursor_report() -> sys::ArcadiaTioOcbReadCursorReport {
        sys::ArcadiaTioOcbReadCursorReport {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadCursorReport>(),
            base_report: empty_read_report(),
            batches_yielded: 0,
            rows_yielded: 0,
            cancelled: 0,
            reserved: [0; 4],
        }
    }

    fn empty_read_fill_report() -> sys::ArcadiaTioOcbReadFillReport {
        sys::ArcadiaTioOcbReadFillReport {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadFillReport>(),
            row_group_id: 0,
            base_row: 0,
            row_count: 0,
            columns_filled: 0,
            reserved: [0; 8],
        }
    }

    fn empty_read_attribution() -> sys::ArcadiaTioOcbReadAttribution {
        sys::ArcadiaTioOcbReadAttribution {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadAttribution>(),
            plan_ns: 0,
            execute_wall_ns: 0,
            row_group_read_ns: 0,
            read_io_ns: 0,
            checksum_ns: 0,
            decompression_ns: 0,
            primitive_decode_ns: 0,
            has_native_to_c_copy_ns: 0,
            native_to_c_copy_ns: 0,
            has_wrapper_copy_ns: 0,
            wrapper_copy_ns: 0,
            bytes_read: 0,
            compressed_bytes: 0,
            uncompressed_bytes: 0,
            requested_threads: 0,
            effective_threads: 0,
            selected_row_groups: 0,
            pruned_row_groups: 0,
            selected_column_chunks: 0,
            fallback_reason: ptr::null_mut(),
            reserved: [0; 4],
        }
    }

    fn empty_read_outcome() -> sys::ArcadiaTioOcbReadOutcome {
        sys::ArcadiaTioOcbReadOutcome {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadOutcome>(),
            batches: ptr::null_mut(),
            batches_len: 0,
            report: empty_read_report(),
            reserved: [0; 4],
        }
    }

    fn empty_row_group_summaries() -> sys::ArcadiaTioOcbRowGroupSummaries {
        sys::ArcadiaTioOcbRowGroupSummaries {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupSummaries>(),
            row_groups: ptr::null_mut(),
            row_groups_len: 0,
            reserved: [0; 4],
        }
    }

    struct MetadataGuard(sys::ArcadiaTioOcbMetadata);
    impl Drop for MetadataGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_metadata_free(&mut self.0) };
        }
    }

    struct DictionaryValuesGuard(sys::ArcadiaTioOcbDictionaryValues);
    impl Drop for DictionaryValuesGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_dictionary_values_free(&mut self.0) };
        }
    }

    struct ReadReportGuard(sys::ArcadiaTioOcbReadReport);
    impl Drop for ReadReportGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_read_report_free(&mut self.0) };
        }
    }

    struct ReadAttributionGuard(sys::ArcadiaTioOcbReadAttribution);
    impl Drop for ReadAttributionGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_read_attribution_free(&mut self.0) };
        }
    }

    struct ReadCursorReportGuard(sys::ArcadiaTioOcbReadCursorReport);
    impl Drop for ReadCursorReportGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_read_cursor_report_free(&mut self.0) };
        }
    }

    struct ReadOutcomeGuard(sys::ArcadiaTioOcbReadOutcome);
    impl Drop for ReadOutcomeGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_read_outcome_free(&mut self.0) };
        }
    }

    struct RowGroupSummariesGuard(sys::ArcadiaTioOcbRowGroupSummaries);
    impl Drop for RowGroupSummariesGuard {
        fn drop(&mut self) {
            unsafe { sys::arcadia_tio_ocb_row_group_summaries_free(&mut self.0) };
        }
    }

    struct VisitCallback<'a, F>
    where
        F: FnMut(ColumnBatch) -> OcbResult<VisitControl>,
    {
        visitor: &'a mut F,
    }

    unsafe extern "C" fn visit_trampoline<F>(
        user: *mut c_void,
        batch: *const sys::ArcadiaTioOcbColumnBatch,
        out_continue: *mut u8,
    ) -> sys::ArcadiaTioErrorCode
    where
        F: FnMut(ColumnBatch) -> OcbResult<VisitControl>,
    {
        if user.is_null() || batch.is_null() || out_continue.is_null() {
            return sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT;
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            let callback = unsafe { &mut *user.cast::<VisitCallback<'_, F>>() };
            let batch = unsafe { column_batch_from_raw(&*batch) }?;
            match (callback.visitor)(batch)? {
                VisitControl::Continue => {
                    unsafe { ptr::write(out_continue, 1) };
                }
                VisitControl::Stop => {
                    unsafe { ptr::write(out_continue, 0) };
                }
            }
            Ok::<(), OcbError>(())
        }));
        match result {
            Ok(Ok(())) => sys::ARCADIA_TIO_ERROR_OK,
            Ok(Err(err)) => err.code().as_raw(),
            Err(_) => sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT,
        }
    }

    fn duration_to_ns(duration: std::time::Duration) -> u64 {
        duration.as_nanos().min(u128::from(u64::MAX)) as u64
    }

    fn ensure_plan_belongs_to_file(
        raw_file: NonNull<sys::ArcadiaTioOcbFile>,
        plan: &ReadPlan<'_>,
    ) -> OcbResult<()> {
        if plan.file_raw == raw_file {
            Ok(())
        } else {
            Err(OcbError::invalid_input(
                "OCB read plan belongs to a different file handle",
            ))
        }
    }

    fn read_plan_report(raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>) -> OcbResult<ReadReport> {
        let mut raw_report = empty_read_report();
        let status =
            unsafe { sys::arcadia_tio_ocb_read_plan_report(raw_plan.as_ptr(), &mut raw_report) };
        let guard = ReadReportGuard(raw_report);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB read_plan_report failed"));
        }
        Ok(read_report_from_raw(&guard.0))
    }

    fn read_plan_projected_column_ids(
        raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
    ) -> OcbResult<Vec<u32>> {
        read_plan_ids(
            raw_plan,
            sys::arcadia_tio_ocb_read_plan_projected_column_ids,
            "OCB read_plan_projected_column_ids failed",
        )
    }

    fn read_plan_row_group_ids(
        raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
    ) -> OcbResult<Vec<u32>> {
        read_plan_ids(
            raw_plan,
            sys::arcadia_tio_ocb_read_plan_row_group_ids,
            "OCB read_plan_row_group_ids failed",
        )
    }

    fn read_plan_ids(
        raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
        f: unsafe extern "C" fn(
            *const sys::ArcadiaTioOcbReadPlan,
            *mut u32,
            usize,
            *mut usize,
        ) -> sys::ArcadiaTioErrorCode,
        context: &str,
    ) -> OcbResult<Vec<u32>> {
        let mut required = 0usize;
        let status = unsafe { f(raw_plan.as_ptr(), ptr::null_mut(), 0, &mut required) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last(context));
        }
        let mut ids = vec![0u32; required];
        let status = unsafe {
            f(
                raw_plan.as_ptr(),
                ids.as_mut_ptr(),
                ids.len(),
                &mut required,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last(context));
        }
        ids.truncate(required);
        Ok(ids)
    }

    fn read_batches_from_plan(
        raw_file: NonNull<sys::ArcadiaTioOcbFile>,
        raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
        row_group_ids: Option<&[u32]>,
    ) -> OcbResult<ReadOutcome> {
        let mut raw_outcome = empty_read_outcome();
        let (ids_ptr, ids_len) = match row_group_ids {
            Some(ids) => (ids.as_ptr(), ids.len()),
            None => (ptr::null(), 0),
        };
        let status = unsafe {
            sys::arcadia_tio_ocb_read_batches_from_plan(
                raw_file.as_ptr(),
                raw_plan.as_ptr(),
                ids_ptr,
                ids_len,
                &mut raw_outcome,
            )
        };
        let guard = ReadOutcomeGuard(raw_outcome);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB read_batches_from_plan failed"));
        }
        unsafe { read_outcome_from_raw(&guard.0) }
    }

    unsafe fn metadata_from_raw(raw: &sys::ArcadiaTioOcbMetadata) -> OcbResult<Metadata> {
        let columns = unsafe { raw_slice(raw.columns, raw.columns_len) }
            .iter()
            .map(|column| ColumnDescriptor {
                id: column.id,
                name: raw_string(column.name.cast()),
                physical_type: PhysicalType::from_raw_with_width(column.physical_type, unsafe {
                    sys::arcadia_tio_ocb_column_descriptor_fixed_binary_width(column)
                }),
                logical_kind: LogicalKind::from_raw(column.logical_kind),
                dictionary_id: (column.has_dictionary_id != 0).then_some(column.dictionary_id),
                scale: column.scale,
                nullable: column.nullable != 0,
            })
            .collect();
        let dictionaries = unsafe { raw_slice(raw.dictionaries, raw.dictionaries_len) }
            .iter()
            .map(|dictionary| DictionaryDescriptor {
                dictionary_id: dictionary.dictionary_id,
                name: raw_string(dictionary.name.cast()),
                code_physical_type: PhysicalType::from_raw(dictionary.code_physical_type),
                value_kind: DictionaryValueKind::from_raw(dictionary.value_kind),
                entry_count: dictionary.entry_count,
            })
            .collect();
        let ordering_keys = unsafe { raw_slice(raw.ordering_keys, raw.ordering_keys_len) }
            .iter()
            .map(|key| OrderingKey {
                column_id: key.column_id,
                column_name: raw_string(key.column_name.cast()),
                direction: OrderingDirection::from_raw(key.direction),
                null_order: NullOrder::from_raw(key.null_order),
            })
            .collect();
        Ok(Metadata {
            format_name: raw_string(raw.format_name.cast()),
            appendable: raw.appendable != 0,
            root_generation: raw.root_generation,
            previous_root_generation: (raw.has_previous_root_generation != 0)
                .then_some(raw.previous_root_generation),
            row_count: raw.row_count,
            row_group_count: raw.row_group_count,
            column_chunk_count: raw.column_chunk_count,
            columns,
            dictionaries,
            ordering_keys,
        })
    }

    unsafe fn dictionary_values_from_raw(
        raw: &sys::ArcadiaTioOcbDictionaryValues,
    ) -> OcbResult<DictionaryValues> {
        let strings = unsafe { raw_string_array(raw.string_values, raw.string_values_len) };
        let bytes = unsafe { raw_byte_slices(raw.byte_values, raw.byte_values_len) };
        let values = match DictionaryValueKind::from_raw(raw.value_kind) {
            DictionaryValueKind::Utf8 => DecodedDictionaryValues::Utf8(strings),
            DictionaryValueKind::Bytes => DecodedDictionaryValues::Bytes(bytes),
            DictionaryValueKind::FixedBytes => DecodedDictionaryValues::FixedBytes {
                fixed_width: raw.fixed_width,
                values: bytes,
            },
            DictionaryValueKind::EnumLabels => DecodedDictionaryValues::EnumLabels(strings),
            DictionaryValueKind::Unknown(raw_kind) => DecodedDictionaryValues::Unknown {
                raw_kind,
                strings,
                bytes,
            },
        };
        Ok(DictionaryValues {
            dictionary_id: raw.dictionary_id,
            name: raw_string(raw.name.cast()),
            values,
        })
    }

    unsafe fn row_group_summaries_from_raw(
        raw: &sys::ArcadiaTioOcbRowGroupSummaries,
    ) -> OcbResult<Vec<RowGroupSummary>> {
        unsafe { raw_slice(raw.row_groups, raw.row_groups_len) }
            .iter()
            .map(|summary| unsafe { row_group_summary_from_raw(summary) })
            .collect()
    }

    unsafe fn row_group_summary_from_raw(
        raw: &sys::ArcadiaTioOcbRowGroupSummary,
    ) -> OcbResult<RowGroupSummary> {
        Ok(RowGroupSummary {
            row_group_id: raw.row_group_id,
            base_row: raw.base_row,
            row_count: raw.row_count,
            first_key_tuple_ref: (raw.has_first_key_tuple_ref != 0)
                .then(|| body_ref_summary_from_raw(&raw.first_key_tuple_ref)),
            last_key_tuple_ref: (raw.has_last_key_tuple_ref != 0)
                .then(|| body_ref_summary_from_raw(&raw.last_key_tuple_ref)),
            chunks: unsafe { raw_slice(raw.chunks, raw.chunks_len) }
                .iter()
                .map(|chunk| unsafe { column_chunk_summary_from_raw(chunk) })
                .collect::<OcbResult<Vec<_>>>()?,
            stats: unsafe { raw_slice(raw.stats, raw.stats_len) }
                .iter()
                .map(|stats| unsafe { column_stats_summary_from_raw(stats) })
                .collect::<OcbResult<Vec<_>>>()?,
        })
    }

    fn body_ref_summary_from_raw(raw: &sys::ArcadiaTioOcbBodyRefSummary) -> BodyRefSummary {
        BodyRefSummary {
            offset: raw.offset,
            length: raw.length,
            kind: BodyKind::from_raw(raw.kind),
            flags: raw.flags,
            checksum_kind: ChecksumKind::from_raw(raw.checksum_kind),
            checksum: raw.checksum,
        }
    }

    unsafe fn column_chunk_summary_from_raw(
        raw: &sys::ArcadiaTioOcbColumnChunkSummary,
    ) -> OcbResult<ColumnChunkSummary> {
        Ok(ColumnChunkSummary {
            row_group_id: raw.row_group_id,
            column_id: raw.column_id,
            column_name: raw_string(raw.column_name),
            physical_type: PhysicalType::from_raw_with_width(
                raw.physical_type,
                raw.fixed_binary_width,
            ),
            logical_kind: LogicalKind::from_raw(raw.logical_kind),
            fixed_binary_width: (raw.fixed_binary_width != 0).then_some(raw.fixed_binary_width),
            codec: ColumnChunkSummaryCodec::from_raw(raw.codec),
            row_count: raw.row_count,
            compressed_bytes: raw.compressed_bytes,
            uncompressed_bytes: raw.uncompressed_bytes,
            value_ref: body_ref_summary_from_raw(&raw.value_ref),
            validity_ref: (raw.has_validity_ref != 0)
                .then(|| body_ref_summary_from_raw(&raw.validity_ref)),
        })
    }

    unsafe fn column_stats_summary_from_raw(
        raw: &sys::ArcadiaTioOcbColumnStatsSummary,
    ) -> OcbResult<ColumnStatsSummary> {
        Ok(ColumnStatsSummary {
            row_group_id: raw.row_group_id,
            column_id: raw.column_id,
            column_name: raw_string(raw.column_name),
            physical_type: PhysicalType::from_raw(raw.physical_type),
            null_count: raw.null_count,
            min: predicate_value_from_raw(&raw.min),
            max: predicate_value_from_raw(&raw.max),
        })
    }

    fn predicate_value_from_raw(raw: &sys::ArcadiaTioOcbPredicateValue) -> PredicateValue {
        match raw.physical_type {
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32 => PredicateValue::I32(raw.i32_value),
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64 => PredicateValue::I64(raw.i64_value),
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32 => PredicateValue::F32(raw.f32_value),
            sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64 => PredicateValue::F64(raw.f64_value),
            _ => PredicateValue::I32(0),
        }
    }

    unsafe fn read_outcome_from_raw(raw: &sys::ArcadiaTioOcbReadOutcome) -> OcbResult<ReadOutcome> {
        let batches = unsafe { raw_slice(raw.batches, raw.batches_len) }
            .iter()
            .map(|batch| unsafe { column_batch_from_raw(batch) })
            .collect::<OcbResult<Vec<_>>>()?;
        Ok(ReadOutcome {
            batches,
            report: read_report_from_raw(&raw.report),
        })
    }

    fn read_report_from_raw(raw: &sys::ArcadiaTioOcbReadReport) -> ReadReport {
        ReadReport {
            requested_threads: raw.requested_threads,
            effective_threads: raw.effective_threads,
            selected_row_groups: raw.selected_row_groups,
            pruned_row_groups: raw.pruned_row_groups,
            selected_column_chunks: raw.selected_column_chunks,
            fallback_reason: raw_optional_string(raw.fallback_reason.cast()),
        }
    }

    fn read_cursor_report_from_raw(raw: &sys::ArcadiaTioOcbReadCursorReport) -> ReadCursorReport {
        ReadCursorReport {
            base_report: read_report_from_raw(&raw.base_report),
            batches_yielded: raw.batches_yielded,
            rows_yielded: raw.rows_yielded,
            cancelled: raw.cancelled != 0,
        }
    }

    fn read_fill_report_from_raw(
        raw: &sys::ArcadiaTioOcbReadFillReport,
        raw_columns: &[sys::ArcadiaTioOcbColumnFillBuffer],
    ) -> ReadFillReport {
        ReadFillReport {
            row_group_id: raw.row_group_id,
            base_row: raw.base_row,
            row_count: raw.row_count,
            columns: raw_columns
                .iter()
                .take(raw.columns_filled)
                .map(|column| ColumnFillReport {
                    column_id: column.column_id,
                    rows_filled: column.rows_filled,
                    validity_filled: column.validity_filled != 0,
                })
                .collect(),
        }
    }

    fn read_attribution_from_raw(raw: &sys::ArcadiaTioOcbReadAttribution) -> ReadAttribution {
        ReadAttribution {
            plan_ns: raw.plan_ns,
            execute_wall_ns: raw.execute_wall_ns,
            row_group_read_ns: raw.row_group_read_ns,
            read_io_ns: raw.read_io_ns,
            checksum_ns: raw.checksum_ns,
            decompression_ns: raw.decompression_ns,
            primitive_decode_ns: raw.primitive_decode_ns,
            native_to_c_copy_ns: (raw.has_native_to_c_copy_ns != 0)
                .then_some(raw.native_to_c_copy_ns),
            wrapper_copy_ns: (raw.has_wrapper_copy_ns != 0).then_some(raw.wrapper_copy_ns),
            bytes_read: raw.bytes_read,
            compressed_bytes: raw.compressed_bytes,
            uncompressed_bytes: raw.uncompressed_bytes,
            requested_threads: raw.requested_threads,
            effective_threads: raw.effective_threads,
            selected_row_groups: raw.selected_row_groups,
            pruned_row_groups: raw.pruned_row_groups,
            selected_column_chunks: raw.selected_column_chunks,
            fallback_reason: raw_optional_string(raw.fallback_reason.cast()),
        }
    }

    unsafe fn column_batch_from_raw(raw: &sys::ArcadiaTioOcbColumnBatch) -> OcbResult<ColumnBatch> {
        let columns = unsafe { raw_slice(raw.columns, raw.columns_len) }
            .iter()
            .map(|column| unsafe { column_array_from_raw(column) })
            .collect::<OcbResult<Vec<_>>>()?;
        Ok(ColumnBatch {
            row_group_id: raw.row_group_id,
            base_row: raw.base_row,
            row_count: raw.row_count,
            columns,
        })
    }

    unsafe fn column_array_from_raw(raw: &sys::ArcadiaTioOcbColumnArray) -> OcbResult<ColumnArray> {
        Ok(ColumnArray {
            column_id: raw.column_id,
            name: raw_string(raw.name.cast()),
            physical_type: PhysicalType::from_raw_with_width(raw.physical_type, unsafe {
                sys::arcadia_tio_ocb_column_array_fixed_binary_width(raw)
            }),
            logical_kind: LogicalKind::from_raw(raw.logical_kind),
            dictionary_id: (raw.has_dictionary_id != 0).then_some(raw.dictionary_id),
            values: unsafe { primitive_values_from_raw(&raw.values) }?,
            validity: if raw.has_validity != 0 {
                Some(ValidityBitmap {
                    bytes: unsafe { raw_bytes(raw.validity.data, raw.validity.len) },
                    row_count: raw.validity.row_count,
                })
            } else {
                None
            },
        })
    }

    unsafe fn primitive_values_from_raw(
        raw: &sys::ArcadiaTioOcbPrimitiveValues,
    ) -> OcbResult<PrimitiveValues> {
        match PhysicalType::from_raw_with_width(raw.physical_type, raw.reserved[0] as u32) {
            PhysicalType::I32 => Ok(PrimitiveValues::I32(unsafe {
                raw_typed(raw.data.cast(), raw.len)
            })),
            PhysicalType::I64 => Ok(PrimitiveValues::I64(unsafe {
                raw_typed(raw.data.cast(), raw.len)
            })),
            PhysicalType::F32 => Ok(PrimitiveValues::F32(unsafe {
                raw_typed(raw.data.cast(), raw.len)
            })),
            PhysicalType::F64 => Ok(PrimitiveValues::F64(unsafe {
                raw_typed(raw.data.cast(), raw.len)
            })),
            PhysicalType::FixedBinary { width } => {
                let byte_len = raw.len.checked_mul(width as usize).ok_or_else(|| {
                    OcbError::invalid_input("OCB fixed-binary byte length overflows")
                })?;
                Ok(PrimitiveValues::FixedBinary {
                    width,
                    bytes: unsafe { raw_bytes(raw.data.cast(), byte_len) },
                })
            }
            PhysicalType::Unknown(raw_type) => Err(OcbError::invalid_input(format!(
                "unknown OCB primitive physical type {raw_type}"
            ))),
        }
    }

    unsafe fn raw_slice<'a, T>(ptr: *const T, len: usize) -> &'a [T] {
        if ptr.is_null() || len == 0 {
            &[]
        } else {
            unsafe { slice::from_raw_parts(ptr, len) }
        }
    }

    unsafe fn raw_typed<T: Copy>(ptr: *const T, len: usize) -> Vec<T> {
        unsafe { raw_slice(ptr, len) }.to_vec()
    }

    fn raw_string(ptr: *const c_char) -> String {
        raw_optional_string(ptr).unwrap_or_default()
    }

    fn raw_optional_string(ptr: *const c_char) -> Option<String> {
        if ptr.is_null() {
            None
        } else {
            Some(
                unsafe { CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned(),
            )
        }
    }

    unsafe fn raw_string_array(ptr: *mut *mut c_char, len: usize) -> Vec<String> {
        unsafe { raw_slice(ptr.cast::<*mut c_char>(), len) }
            .iter()
            .map(|value| raw_string((*value).cast()))
            .collect()
    }

    unsafe fn raw_byte_slices(ptr: *mut sys::ArcadiaTioOcbByteSlice, len: usize) -> Vec<Vec<u8>> {
        unsafe { raw_slice(ptr, len) }
            .iter()
            .map(|value| unsafe { raw_bytes(value.data, value.len) })
            .collect()
    }

    unsafe fn raw_bytes(ptr: *const u8, len: usize) -> Vec<u8> {
        unsafe { raw_slice(ptr, len) }.to_vec()
    }
}

#[cfg(test)]
mod tests {
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
        let err = ops::tile(&scalar_like, &[usize::MAX])
            .expect_err("huge tile should not allocate or panic");
        assert_eq!(err.code(), ErrorCode::InvalidArgument);
    }

    #[test]
    fn tensor_ops_math_and_reductions_cover_public_dtypes() {
        let lhs =
            Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]).expect("lhs");
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
        let floats = Tensor::from_dense_f32(vec![2, 3], vec![3.0, 1.0, 2.0, 6.0, 5.0, 4.0])
            .expect("f32 tensor");
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
        let f32_tensor = Tensor::from_dense_f32(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])
            .expect("f32 tensor");
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
        let nan_tensor = Tensor::from_dense_f32(vec![2, 2], vec![f32::NAN, 1.0, 2.0, f32::NAN])
            .expect("nan tensor");
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
        let path =
            std::env::temp_dir().join("arcadia_tio_wrapper_unsupported_compression_codec.tio");
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
        let mapped = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) }
            .expect("lookup result maps");
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
        let variable = CoordinateLookupKeyV2::variable_string("abc")
            .expect_err("variable strings are deferred");
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
}
