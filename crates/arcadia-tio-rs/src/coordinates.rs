use super::*;

/// Coordinate dtype supported by native coordinate metadata.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoordinateDType {
    /// 32-bit signed integer coordinates.
    I32,
    /// 64-bit signed integer coordinates.
    I64,
}

impl CoordinateDType {
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateDType {
        match self {
            Self::I32 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I32,
            Self::I64 => sys::ARCADIA_TIO_COORDINATE_DTYPE_I64,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateDType) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateKind {
        match self {
            Self::Position => sys::ARCADIA_TIO_COORDINATE_KIND_POSITION,
            Self::LabelId => sys::ARCADIA_TIO_COORDINATE_KIND_LABEL_ID,
            Self::Date => sys::ARCADIA_TIO_COORDINATE_KIND_DATE,
            Self::Timestamp => sys::ARCADIA_TIO_COORDINATE_KIND_TIMESTAMP,
            Self::DomainValue => sys::ARCADIA_TIO_COORDINATE_KIND_DOMAIN_VALUE,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateKind) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateEncoding {
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

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateEncoding) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateSortedness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_SORTED_UNKNOWN,
            Self::Ascending => sys::ARCADIA_TIO_COORDINATE_SORTED_ASCENDING,
            Self::Descending => sys::ARCADIA_TIO_COORDINATE_SORTED_DESCENDING,
            Self::Unsorted => sys::ARCADIA_TIO_COORDINATE_SORTED_UNSORTED,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateSortedness) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateMonotonicity {
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

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateMonotonicity) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateUniqueness {
        match self {
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNKNOWN,
            Self::Unique => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_UNIQUE,
            Self::HasDuplicates => sys::ARCADIA_TIO_COORDINATE_UNIQUENESS_HAS_DUPLICATES,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateUniqueness) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateStorageKind) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateSourceKind {
        match self {
            Self::SameFileObject => sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            Self::RelativePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_RELATIVE_PATH,
            Self::AbsolutePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_ABSOLUTE_PATH,
            Self::Uri => sys::ARCADIA_TIO_COORDINATE_SOURCE_URI,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateSourceKind) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateValidationStatus) -> Result<Self> {
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
    pub(crate) fn dtype(&self) -> CoordinateDType {
        match self {
            Self::I32(_) => CoordinateDType::I32,
            Self::I64(_) => CoordinateDType::I64,
        }
    }

    pub(crate) fn len(&self) -> usize {
        match self {
            Self::I32(values) => values.len(),
            Self::I64(values) => values.len(),
        }
    }

    pub(crate) fn as_ptr(&self) -> *const c_void {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateValueDomainV2 {
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

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateValueDomainV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateKeyDomainV2 {
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

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateKeyDomainV2) -> Result<Self> {
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
    pub(crate) fn size_bytes(self) -> usize {
        match self {
            Self::U8 => mem::size_of::<u8>(),
            Self::U16 => mem::size_of::<u16>(),
            Self::U32 => mem::size_of::<u32>(),
            Self::U64 => mem::size_of::<u64>(),
        }
    }

    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateCodeDTypeV2 {
        match self {
            Self::U8 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U8,
            Self::U16 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U16,
            Self::U32 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U32,
            Self::U64 => sys::ARCADIA_TIO_COORDINATE_CODE_DTYPE_V2_U64,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateCodeDTypeV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateFixedTextEncodingV2 {
        match self {
            Self::Ascii => sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_ENCODING_V2_ASCII,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateFixedTextEncodingV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateFixedTextPaddingV2 {
        match self {
            Self::RightSpace => sys::ARCADIA_TIO_COORDINATE_FIXED_TEXT_PADDING_V2_RIGHT_SPACE,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateFixedTextPaddingV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateSourceKindV2 {
        match self {
            Self::SameFileObject => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_SAME_FILE_OBJECT,
            Self::RelativePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_RELATIVE_PATH,
            Self::AbsolutePath => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_ABSOLUTE_PATH,
            Self::Uri => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_URI,
            Self::ApplicationRegistry => sys::ARCADIA_TIO_COORDINATE_SOURCE_V2_APPLICATION_REGISTRY,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateSourceKindV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateAvailabilityV2 {
        match self {
            Self::Available => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_AVAILABLE,
            Self::Absent => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_ABSENT,
            Self::Unknown => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNKNOWN,
            Self::Invalid => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_INVALID,
            Self::Unavailable => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNAVAILABLE,
            Self::Unsupported => sys::ARCADIA_TIO_COORDINATE_AVAILABILITY_V2_UNSUPPORTED,
        }
    }

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateAvailabilityV2) -> Result<Self> {
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
    pub(crate) fn to_raw(self) -> sys::ArcadiaTioCoordinateStatusCategoryV2 {
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

    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateStatusCategoryV2) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateIndexKindV2) -> Result<Self> {
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
    pub(crate) fn from_raw(
        value: sys::ArcadiaTioCoordinateIndexValidationStatusV2,
    ) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateIndexFallbackV2) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateIndexUseV2) -> Result<Self> {
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
    pub(crate) fn from_raw(value: sys::ArcadiaTioCoordinateLookupResultStatusV2) -> Result<Self> {
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

    pub(crate) fn from_raw(raw: sys::ArcadiaTioCoordinateFixedTextLayoutV2) -> Result<Self> {
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

    pub(crate) fn from_raw(raw: &sys::ArcadiaTioCoordinateDictionarySummaryV2) -> Result<Self> {
        Ok(Self {
            dictionary_id: optional_c_string(raw.dictionary_id)?,
            revision: raw.revision,
            code_dtype: CoordinateCodeDTypeV2::from_raw(raw.code_dtype)?,
            entry_count: raw.entry_count,
            stable_ids_unique: raw.stable_ids_unique != 0,
            display_labels_unique: raw.display_labels_unique != 0,
            aliases_unique: raw.aliases_unique != 0,
            codes_stable_across_revisions: raw.codes_stable_across_revisions != 0,
            content_id: optional_c_string(raw.content_id)?,
        })
    }

    pub(crate) fn prepare(&self) -> Result<PreparedCoordinateDictionarySummaryV2> {
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

    pub(crate) fn from_raw(raw: &sys::ArcadiaTioCoordinateDictionaryEntryV2) -> Result<Self> {
        // SAFETY: Native dictionary entry aliases are valid for `aliases_len` until the parent is freed.
        let aliases = unsafe {
            checked_slice(
                raw.aliases.cast_const(),
                raw.aliases_len,
                "Coordinate v2 dictionary aliases",
            )
        }?
        .iter()
        .map(|alias| required_c_string((*alias).cast_const(), "Coordinate v2 dictionary alias"))
        .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            code: raw.code,
            stable_id: optional_c_string(raw.stable_id.cast_const())?,
            display_label: optional_c_string(raw.display_label.cast_const())?,
            aliases,
        })
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
        ensure_native_abi()?;
        // SAFETY: Caller guarantees the native entry array is valid for `entries_len`.
        let entries = unsafe {
            checked_slice(
                raw.entries.cast_const(),
                raw.entries_len,
                "Coordinate v2 dictionary entries",
            )
        }?
        .iter()
        .map(CoordinateDictionaryEntryV2::from_raw)
        .collect::<Result<Vec<_>>>()?;
        let summary = CoordinateDictionarySummaryV2::from_raw(&raw.summary)?;
        let expected_entries = usize::try_from(summary.entry_count).map_err(|_| {
            TioError::conversion("Coordinate v2 dictionary entry count does not fit usize")
        })?;
        if entries.len() != expected_entries {
            return Err(TioError::conversion(format!(
                "Coordinate v2 dictionary summary count {expected_entries} does not match returned entry count {}",
                entries.len()
            )));
        }
        Ok(Self {
            summary,
            entries,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            reason: optional_c_string(raw.reason.cast_const())?,
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

    pub(crate) fn from_raw(raw: &sys::ArcadiaTioCoordinateExternalBindingV2) -> Result<Self> {
        Ok(Self {
            source_kind: CoordinateSourceKindV2::from_raw(raw.source_kind)?,
            logical_id: optional_c_string(raw.logical_id)?,
            privacy_safe_display: optional_c_string(raw.privacy_safe_display)?,
            content_id: optional_c_string(raw.content_id)?,
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            length: raw.length,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            required: raw.required != 0,
        })
    }

    pub(crate) fn prepare(&self) -> Result<PreparedCoordinateExternalBindingV2> {
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
    pub(crate) fn from_raw(raw: &sys::ArcadiaTioCoordinateIndexSourceBindingV2) -> Result<Self> {
        Ok(Self {
            descriptor_id: optional_c_string(raw.descriptor_id)?,
            descriptor_revision: raw.descriptor_revision,
            value_domain: CoordinateValueDomainV2::from_raw(raw.value_domain)?,
            value_object_id: optional_c_string(raw.value_object_id)?,
            dictionary_id: optional_c_string(raw.dictionary_id)?,
            dictionary_revision: raw.dictionary_revision,
            dictionary_content_id: optional_c_string(raw.dictionary_content_id)?,
            external_source_kind: CoordinateSourceKindV2::from_raw(raw.external_source_kind)?,
            external_logical_id: optional_c_string(raw.external_logical_id)?,
            external_content_id: optional_c_string(raw.external_content_id)?,
            root_id: optional_c_string(raw.root_id)?,
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
    pub(crate) fn from_raw(raw: &sys::ArcadiaTioCoordinateIndexSummaryV2) -> Result<Self> {
        Ok(Self {
            index_id: optional_c_string(raw.index_id)?,
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
            reason: optional_c_string(raw.reason)?,
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
    pub(crate) fn pointer_len_for_axis(
        &self,
        fixed_text_width: usize,
    ) -> Result<(*const c_void, usize)> {
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

    pub(crate) fn pointer_count_element_size(
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

pub(crate) fn buffer_ptr_len<T>(values: &[T]) -> (*const c_void, usize) {
    (buffer_ptr_for_count(values, values.len()), values.len())
}

pub(crate) fn buffer_ptr_count_element_size<T>(values: &[T]) -> (*const c_void, usize, usize) {
    (
        buffer_ptr_for_count(values, values.len()),
        values.len(),
        mem::size_of::<T>(),
    )
}

pub(crate) fn buffer_ptr_for_count<T>(values: &[T], count: usize) -> *const c_void {
    if count == 0 {
        ptr::null()
    } else {
        values.as_ptr().cast()
    }
}

pub(crate) fn validate_fixed_text_lookup_key(bytes_len: usize, width: usize) -> Result<()> {
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

pub(crate) fn fixed_text_value_count(bytes_len: usize, width: usize) -> Result<usize> {
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

    pub(crate) fn raw_fixed_text_layout(&self) -> sys::ArcadiaTioCoordinateFixedTextLayoutV2 {
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
    pub(crate) _descriptor_id: Option<CString>,
    pub(crate) _name: Option<CString>,
    pub(crate) _dictionary: Option<PreparedCoordinateDictionarySummaryV2>,
    pub(crate) _dictionary_entries: PreparedCoordinateDictionaryEntriesV2,
    pub(crate) _external_binding: Option<PreparedCoordinateExternalBindingV2>,
    pub(crate) raw: sys::ArcadiaTioAxisCoordinateInputV2,
    pub(crate) _values: PhantomData<&'a AxisCoordinateInputV2>,
}

impl<'a> PreparedAxisCoordinateInputV2<'a> {
    pub(crate) fn new(input: &'a AxisCoordinateInputV2) -> Result<Self> {
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

pub(crate) struct PreparedAxisCoordinateInputsV2<'a> {
    pub(crate) prepared: Vec<PreparedAxisCoordinateInputV2<'a>>,
    pub(crate) raw: Vec<sys::ArcadiaTioAxisCoordinateInputV2>,
}

impl<'a> PreparedAxisCoordinateInputsV2<'a> {
    pub(crate) fn new(inputs: &'a [AxisCoordinateInputV2], rank: usize) -> Result<Self> {
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

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioAxisCoordinateInputV2 {
        let _keep_alive = &self.prepared;
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
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
    pub(crate) fn from_raw(raw: &sys::ArcadiaTioAxisCoordinateMetaV2) -> Result<Self> {
        Ok(Self {
            axis: raw.axis,
            axis_name_snapshot: optional_c_string(raw.axis_name_snapshot.cast_const())?,
            descriptor_id: optional_c_string(raw.descriptor_id.cast_const())?,
            descriptor_revision: raw.descriptor_revision,
            name: optional_c_string(raw.name.cast_const())?,
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
            reason: optional_c_string(raw.reason.cast_const())?,
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
        ensure_native_abi()?;
        let value_domain = CoordinateValueDomainV2::from_raw(raw.value_domain)?;
        let numeric_dtype = CoordinateDType::from_raw(raw.numeric_dtype)?;
        let numeric_encoding = CoordinateEncoding::from_raw(raw.numeric_encoding)?;
        let code_dtype = CoordinateCodeDTypeV2::from_raw(raw.code_dtype)?;
        let expected_element_size = match value_domain {
            CoordinateValueDomainV2::InlineNumeric => match numeric_dtype {
                CoordinateDType::I32 => mem::size_of::<i32>(),
                CoordinateDType::I64 => mem::size_of::<i64>(),
            },
            CoordinateValueDomainV2::FixedText => raw.fixed_text_width,
            CoordinateValueDomainV2::DictionaryCode => code_dtype.size_bytes(),
            CoordinateValueDomainV2::AppendSequence
            | CoordinateValueDomainV2::ExternalReference => 0,
        };
        if raw.len != 0 {
            if expected_element_size == 0 || raw.element_size != expected_element_size {
                return Err(TioError::conversion(format!(
                    "Coordinate v2 value-slice element size {} is inconsistent with its value domain",
                    raw.element_size
                )));
            }
            if value_domain == CoordinateValueDomainV2::FixedText && raw.fixed_text_width == 0 {
                return Err(TioError::conversion(
                    "Coordinate v2 fixed-text value slice returned zero width",
                ));
            }
        }
        let byte_len = raw.len.checked_mul(raw.element_size).ok_or_else(|| {
            TioError::conversion("Coordinate v2 value slice byte length overflow")
        })?;
        // SAFETY: Caller guarantees the C ABI value buffer is valid for `byte_len` bytes.
        let data = unsafe {
            copy_checked_slice(
                raw.data.cast::<u8>(),
                byte_len,
                "Coordinate v2 value-slice data",
            )
        }?;
        Ok(Self {
            value_domain,
            numeric_dtype,
            numeric_encoding,
            code_dtype,
            data,
            len: raw.len,
            element_size: raw.element_size,
            fixed_text_width: raw.fixed_text_width,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            reason: optional_c_string(raw.reason.cast_const())?,
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
    pub(crate) _text: Option<CString>,
    pub(crate) raw: sys::ArcadiaTioCoordinateLookupKeyV2,
    pub(crate) _bytes: PhantomData<&'a CoordinateLookupKeyV2>,
}

impl<'a> PreparedCoordinateLookupKeyV2<'a> {
    pub(crate) fn new(key: &'a CoordinateLookupKeyV2) -> Result<Self> {
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
        ensure_native_abi()?;
        // SAFETY: Caller guarantees the C ABI positions buffer is valid for `positions_len`.
        let positions = unsafe {
            copy_checked_slice(
                raw.positions.cast_const(),
                raw.positions_len,
                "Coordinate v2 lookup positions",
            )
        }?;
        let status = CoordinateLookupResultStatusV2::from_raw(raw.status)?;
        if status == CoordinateLookupResultStatusV2::Many {
            if positions.is_empty() {
                return Err(TioError::conversion(
                    "Coordinate v2 many-result lookup returned no positions",
                ));
            }
        } else if !positions.is_empty() {
            return Err(TioError::conversion(
                "Coordinate v2 non-many lookup returned a positions array",
            ));
        }
        if status == CoordinateLookupResultStatusV2::Range && raw.range_start > raw.range_end {
            return Err(TioError::conversion(
                "Coordinate v2 lookup range start exceeds range end",
            ));
        }
        Ok(Self {
            status,
            status_category: CoordinateStatusCategoryV2::from_raw(raw.status_category)?,
            unique_position: raw.unique_position,
            range_start: raw.range_start,
            range_end: raw.range_end,
            positions,
            availability: CoordinateAvailabilityV2::from_raw(raw.availability)?,
            reason: optional_c_string(raw.reason.cast_const())?,
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
    pub(crate) _entries: Vec<PreparedAppendCoordinateEntryV2<'a>>,
    pub(crate) _raw_entries: Vec<sys::ArcadiaTioAppendCoordinateEntryV2>,
    pub(crate) raw: sys::ArcadiaTioAppendCoordinateBatchV2,
    pub(crate) _batch: PhantomData<&'a AppendCoordinateBatchV2>,
}

impl<'a> PreparedAppendCoordinateBatchV2<'a> {
    pub(crate) fn new(batch: &'a AppendCoordinateBatchV2) -> Result<Self> {
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

pub(crate) struct PreparedAppendCoordinateEntryV2<'a> {
    // Keep descriptor/name C strings and dictionary-entry strings alive for raw C ABI pointers in `raw`.
    pub(crate) _descriptor_id: Option<CString>,
    pub(crate) _name: Option<CString>,
    pub(crate) _dictionary_entries: PreparedCoordinateDictionaryEntriesV2,
    pub(crate) raw: sys::ArcadiaTioAppendCoordinateEntryV2,
    pub(crate) _entry: PhantomData<&'a AppendCoordinateEntryV2>,
}

impl<'a> PreparedAppendCoordinateEntryV2<'a> {
    pub(crate) fn new(entry: &'a AppendCoordinateEntryV2) -> Result<Self> {
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

pub(crate) struct PreparedCoordinateDictionarySummaryV2 {
    // Keep dictionary summary C strings alive for raw C ABI pointers in `raw`.
    pub(crate) _dictionary_id: Option<CString>,
    pub(crate) _content_id: Option<CString>,
    pub(crate) raw: Box<sys::ArcadiaTioCoordinateDictionarySummaryV2>,
}

impl PreparedCoordinateDictionarySummaryV2 {
    pub(crate) fn new(summary: &CoordinateDictionarySummaryV2) -> Result<Self> {
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

    pub(crate) fn raw_ptr(&self) -> *const sys::ArcadiaTioCoordinateDictionarySummaryV2 {
        self.raw.as_ref()
    }
}

pub(crate) struct PreparedCoordinateExternalBindingV2 {
    // Keep external-binding C strings alive for raw C ABI pointers in `raw`.
    pub(crate) _logical_id: Option<CString>,
    pub(crate) _privacy_safe_display: Option<CString>,
    pub(crate) _content_id: Option<CString>,
    pub(crate) raw: Box<sys::ArcadiaTioCoordinateExternalBindingV2>,
}

impl PreparedCoordinateExternalBindingV2 {
    pub(crate) fn new(binding: &CoordinateExternalBindingV2) -> Result<Self> {
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

    pub(crate) fn raw_ptr(&self) -> *const sys::ArcadiaTioCoordinateExternalBindingV2 {
        self.raw.as_ref()
    }
}

#[derive(Default)]
pub(crate) struct PreparedCoordinateDictionaryEntriesV2 {
    // Keep dictionary entry strings and alias pointer arrays alive for raw C ABI pointers in `raw`.
    pub(crate) _stable_ids: Vec<Option<CString>>,
    pub(crate) _display_labels: Vec<Option<CString>>,
    pub(crate) _aliases: Vec<Vec<CString>>,
    pub(crate) _alias_ptrs: Vec<Vec<*mut c_char>>,
    pub(crate) raw: Vec<sys::ArcadiaTioCoordinateDictionaryEntryV2>,
}

impl PreparedCoordinateDictionaryEntriesV2 {
    pub(crate) fn new(entries: &[CoordinateDictionaryEntryV2]) -> Result<Self> {
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

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioCoordinateDictionaryEntryV2 {
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.raw.len()
    }
}

pub(crate) fn default_coordinate_v2_descriptor_id(axis: usize, suffix: &str) -> String {
    format!("axis{axis}-{suffix}")
}

pub(crate) fn encode_fixed_text_ascii_values<I, S>(width: usize, values: I) -> Result<Vec<u8>>
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

pub(crate) fn validate_fixed_text_layout_v2(layout: CoordinateFixedTextLayoutV2) -> Result<()> {
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

pub(crate) fn validate_fixed_text_bytes_v2(
    bytes: &[u8],
    layout: CoordinateFixedTextLayoutV2,
) -> Result<()> {
    validate_fixed_text_layout_v2(layout)?;
    fixed_text_value_count(bytes.len(), layout.width)?;
    if layout.reject_non_ascii && !bytes.is_ascii() {
        return Err(TioError::invalid_argument(
            "Coordinate v2 fixed-text values must be ASCII",
        ));
    }
    Ok(())
}

pub(crate) fn validate_dictionary_values_v2(
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

pub(crate) fn validate_dictionary_descriptor_v2(input: &AxisCoordinateInputV2) -> Result<()> {
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

pub(crate) fn validate_dictionary_entry_v2(
    entry: &CoordinateDictionaryEntryV2,
    idx: usize,
) -> Result<()> {
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

pub(crate) fn validate_external_binding_v2(binding: &CoordinateExternalBindingV2) -> Result<()> {
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

pub(crate) fn validate_coordinate_input_v2(input: &AxisCoordinateInputV2) -> Result<()> {
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

pub(crate) fn validate_append_entry_v2(entry: &AppendCoordinateEntryV2) -> Result<()> {
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

pub(crate) fn copy_coordinate_index_summaries_v2(
    ptr: *mut sys::ArcadiaTioCoordinateIndexSummaryV2,
    len: usize,
) -> Result<Vec<CoordinateIndexSummaryV2>> {
    // SAFETY: Coordinate v2 index summary array is valid for `len` until the parent metadata is freed.
    unsafe { checked_slice(ptr.cast_const(), len, "Coordinate v2 index summaries") }?
        .iter()
        .map(CoordinateIndexSummaryV2::from_raw)
        .collect()
}

pub(crate) fn optional_owned_cstring(
    value: &Option<String>,
    label: &str,
) -> Result<Option<CString>> {
    value
        .as_deref()
        .map(|item| string_to_cstring(item, label))
        .transpose()
}

pub(crate) fn opt_cstring_ptr(value: &Option<CString>) -> *const c_char {
    value.as_ref().map_or(ptr::null(), |item| item.as_ptr())
}

pub(crate) fn opt_cstring_mut_ptr(value: &Option<CString>) -> *mut c_char {
    value
        .as_ref()
        .map_or(ptr::null_mut(), |item| item.as_ptr() as *mut c_char)
}
