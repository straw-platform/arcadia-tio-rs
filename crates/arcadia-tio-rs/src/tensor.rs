use super::*;

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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioDType {
        match self {
            Self::F32 => sys::ARCADIA_TIO_DTYPE_F32,
            Self::F64 => sys::ARCADIA_TIO_DTYPE_F64,
            Self::I32 => sys::ARCADIA_TIO_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_DTYPE_I64,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioDType) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioAxisKind {
        match self {
            Self::Time => sys::ARCADIA_TIO_AXIS_TIME,
            Self::Symbol => sys::ARCADIA_TIO_AXIS_SYMBOL,
            Self::Channel => sys::ARCADIA_TIO_AXIS_CHANNEL,
            Self::Other => sys::ARCADIA_TIO_AXIS_OTHER,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioAxisKind) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioHeaderProfile) -> Result<Self> {
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

    pub(crate) fn from_dense_data(dtype: DType, shape: Vec<u64>, data: TensorData) -> Result<Self> {
        validate_tensor_parts(dtype, &shape, &data)?;
        Ok(Self { dtype, shape, data })
    }

    pub(crate) fn validate_dtype(&self, expected: DType) -> Result<()> {
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
    pub(crate) inner: Tensor,
    pub(crate) _marker: std::marker::PhantomData<T>,
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
pub(crate) const ARROW_META_DIM_LENS: &str = "arcadia_tio_dim_lens";
#[cfg(feature = "arrow")]
pub(crate) const ARROW_META_ORDER: &str = "arcadia_tio_order";

#[cfg(feature = "arrow")]
pub(crate) fn tensor_to_arrow_record_batch(tensor: &Tensor) -> Result<arrow_array::RecordBatch> {
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
pub(crate) fn tensor_from_arrow_record_batch(batch: &arrow_array::RecordBatch) -> Result<Tensor> {
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
pub(crate) fn arrow_row_width_for_shape(shape: &[u64]) -> Result<usize> {
    if shape.is_empty() {
        return Err(TioError::invalid_argument("tensor rank must be >= 1"));
    }
    shape_element_len(&shape[1..])
}

#[cfg(feature = "arrow")]
pub(crate) fn arrow_encode_dim_lens(shape: &[u64]) -> Result<String> {
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
pub(crate) fn arrow_parse_dim_lens(
    value: &str,
    entry_count: usize,
    list_size: usize,
) -> Result<Vec<u64>> {
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
pub(crate) fn arrow_infer_shape(entry_count: usize, list_size: usize) -> Result<Vec<u64>> {
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
pub(crate) fn arrow_u64_to_usize(value: u64, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| TioError::invalid_argument(format!("{label} exceeds usize")))
}

#[cfg(feature = "arrow")]
pub(crate) fn arrow_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError {
        code: ErrorCode::Io,
        message: err.to_string(),
    }
}

#[cfg(feature = "csv")]
pub(crate) const CSV_HEADER: [&str; 6] =
    ["record", "dtype", "shape", "order", "flat_index", "value"];
#[cfg(feature = "csv")]
pub(crate) const CSV_RECORD_METADATA: &str = "metadata";
#[cfg(feature = "csv")]
pub(crate) const CSV_RECORD_VALUE: &str = "value";
#[cfg(any(feature = "csv", feature = "parquet"))]
pub(crate) const TENSOR_ORDER_ROW_MAJOR: &str = "row-major";

#[cfg(feature = "csv")]
pub(crate) fn tensor_to_csv_string(tensor: &Tensor) -> Result<String> {
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
pub(crate) fn csv_write_value_rows<T: ToString>(
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
pub(crate) fn tensor_from_csv_bytes(bytes: &[u8]) -> Result<Tensor> {
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
pub(crate) fn csv_expect_record_len(record: &csv::StringRecord) -> Result<()> {
    if record.len() != CSV_HEADER.len() {
        return Err(TioError::invalid_argument(
            "Arcadia TIO tensor CSV records must have six fields",
        ));
    }
    Ok(())
}

#[cfg(feature = "csv")]
pub(crate) fn csv_field(record: &csv::StringRecord, index: usize) -> Result<&str> {
    record
        .get(index)
        .ok_or_else(|| TioError::invalid_argument("missing Arcadia TIO tensor CSV field"))
}

#[cfg(feature = "csv")]
pub(crate) fn csv_parse_scalar<T>(value: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse::<T>()
        .map_err(|_| TioError::invalid_argument("invalid Arcadia TIO tensor CSV scalar value"))
}

#[cfg(any(feature = "csv", feature = "parquet"))]
pub(crate) fn tensor_dtype_name(dtype: DType) -> &'static str {
    match dtype {
        DType::F32 => "f32",
        DType::F64 => "f64",
        DType::I32 => "i32",
        DType::I64 => "i64",
    }
}

#[cfg(any(feature = "csv", feature = "parquet"))]
pub(crate) fn tensor_dtype_from_name(value: &str) -> Result<DType> {
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
pub(crate) fn tensor_shape_string(shape: &[u64]) -> Result<String> {
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
pub(crate) fn tensor_shape_from_string(value: &str) -> Result<Vec<u64>> {
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
pub(crate) fn csv_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(err.to_string())
}

#[cfg(feature = "parquet")]
pub(crate) const PARQUET_TENSOR_FORMAT_KEY: &str = "arcadia_tio_format";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_TENSOR_FORMAT_VALUE: &str = "arcadia_tio_tensor_parquet_v1";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_TENSOR_DTYPE_KEY: &str = "arcadia_tio_dtype";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_TENSOR_SHAPE_KEY: &str = "arcadia_tio_shape";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_TENSOR_ORDER_KEY: &str = "arcadia_tio_order";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_COLUMN_FLAT_INDEX: &str = "flat_index";
#[cfg(feature = "parquet")]
pub(crate) const PARQUET_COLUMN_VALUE: &str = "value";

#[cfg(feature = "parquet")]
pub(crate) fn tensor_to_parquet_bytes(tensor: &Tensor) -> Result<Vec<u8>> {
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
pub(crate) fn tensor_from_parquet_bytes(input: &[u8]) -> Result<Tensor> {
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
pub(crate) fn parquet_tensor_schema(
    dtype: DType,
) -> Result<std::sync::Arc<parquet::schema::types::Type>> {
    let value_type = parquet_value_type_name(dtype);
    let message_type = format!(
        "message arcadia_tio_tensor {{ REQUIRED INT64 {PARQUET_COLUMN_FLAT_INDEX}; REQUIRED {value_type} {PARQUET_COLUMN_VALUE}; }}"
    );
    parquet::schema::parser::parse_message_type(&message_type)
        .map(std::sync::Arc::new)
        .map_err(parquet_err)
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_writer_properties(
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
pub(crate) fn parquet_value_type_name(dtype: DType) -> &'static str {
    match dtype {
        DType::F32 => "FLOAT",
        DType::F64 => "DOUBLE",
        DType::I32 => "INT32",
        DType::I64 => "INT64",
    }
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_physical_type(dtype: DType) -> parquet::basic::Type {
    match dtype {
        DType::F32 => parquet::basic::Type::FLOAT,
        DType::F64 => parquet::basic::Type::DOUBLE,
        DType::I32 => parquet::basic::Type::INT32,
        DType::I64 => parquet::basic::Type::INT64,
    }
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_flat_indices(len: usize) -> Result<Vec<i64>> {
    (0..len)
        .map(|idx| {
            i64::try_from(idx).map_err(|_| {
                TioError::invalid_argument("Arcadia TIO tensor Parquet flat index exceeds i64")
            })
        })
        .collect()
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_write_f32_column(
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
pub(crate) fn parquet_write_f64_column(
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
pub(crate) fn parquet_write_i32_column(
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
pub(crate) fn parquet_write_i64_column(
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
pub(crate) fn parquet_validate_written_column(
    column: &str,
    expected: usize,
    written: usize,
) -> Result<()> {
    if written != expected {
        return Err(TioError::invalid_argument(format!(
            "Arcadia TIO tensor Parquet column {column} wrote {written} values, expected {expected}"
        )));
    }
    Ok(())
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_tensor_metadata(
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
pub(crate) fn parquet_metadata_value<'a>(
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
pub(crate) fn parquet_validate_schema(
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
pub(crate) fn parquet_read_tensor_column_values<R, T, F>(
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
pub(crate) fn parquet_read_f32_column(
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
pub(crate) fn parquet_read_f64_column(
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
pub(crate) fn parquet_read_i32_column(
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
pub(crate) fn parquet_read_i64_column(
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
pub(crate) fn parquet_validate_read_column(
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
pub(crate) fn parquet_validate_flat_indices(
    indices: &[i64],
    expected_flat_index: &mut usize,
) -> Result<()> {
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
pub(crate) fn parquet_i64_to_usize(value: i64, label: &str) -> Result<usize> {
    usize::try_from(value).map_err(|_| TioError::invalid_argument(format!("{label} exceeds usize")))
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(format!("Arcadia TIO tensor Parquet error: {err}"))
}

#[cfg(feature = "parquet")]
pub(crate) fn parquet_io_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError {
        code: ErrorCode::Io,
        message: format!("Arcadia TIO tensor Parquet I/O error: {err}"),
    }
}

#[cfg(feature = "ndarray")]
pub(crate) fn tensor_to_ndarray<T: Clone>(
    shape: &[u64],
    values: &[T],
) -> Result<ndarray::ArrayD<T>> {
    let shape = ndarray_shape_to_usize(shape)?;
    ndarray::ArrayD::from_shape_vec(ndarray::IxDyn(&shape), values.to_vec()).map_err(ndarray_err)
}

#[cfg(feature = "ndarray")]
pub(crate) fn ndarray_shape_to_usize(shape: &[u64]) -> Result<Vec<usize>> {
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
pub(crate) fn ndarray_shape_to_tensor_shape(shape: &[usize]) -> Result<Vec<u64>> {
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
pub(crate) fn ndarray_err<E: std::fmt::Display>(err: E) -> TioError {
    TioError::invalid_argument(err.to_string())
}
