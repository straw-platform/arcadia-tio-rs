use super::*;

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
    pub(crate) fn from_raw(value: i32) -> Self {
        match value {
            sys::ARCADIA_TIO_ERROR_OK => Self::Ok,
            sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT => Self::InvalidArgument,
            sys::ARCADIA_TIO_ERROR_UNIMPLEMENTED => Self::Unimplemented,
            sys::ARCADIA_TIO_ERROR_IO => Self::Io,
            sys::ARCADIA_TIO_ERROR_FLATBUFFERS => Self::Flatbuffers,
            other => Self::Unknown(other),
        }
    }

    pub(crate) fn as_raw(self) -> i32 {
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
    pub(crate) code: ErrorCode,
    pub(crate) message: String,
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

    pub(crate) fn invalid_argument(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::InvalidArgument,
            message: message.into(),
        }
    }

    pub(crate) fn unimplemented(message: impl Into<String>) -> Self {
        Self {
            code: ErrorCode::Unimplemented,
            message: message.into(),
        }
    }

    pub(crate) fn from_last_error(fallback: &str) -> Self {
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

    pub(crate) fn conversion(message: impl Into<String>) -> Self {
        Self::invalid_argument(message)
    }

    pub(crate) fn with_reform_report(mut self, report: &ReformReport) -> Self {
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

pub(crate) fn status_result(status: i32, context: &str) -> Result<()> {
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(())
    } else {
        Err(TioError::from_last_error(context))
    }
}

/// Oldest native base ABI supported by this safe-wrapper release.
pub const MIN_SUPPORTED_NATIVE_ABI_VERSION: u32 = 3;

/// Newest native base ABI supported by this safe-wrapper release.
pub const MAX_SUPPORTED_NATIVE_ABI_VERSION: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AbiCompatible(u32);

#[derive(Debug)]
pub(crate) struct AbiGate {
    pub(crate) result: OnceLock<Result<AbiCompatible>>,
}

impl AbiGate {
    pub(crate) const fn new() -> Self {
        Self {
            result: OnceLock::new(),
        }
    }

    pub(crate) fn get_or_query(&self, query: impl FnOnce() -> u32) -> Result<AbiCompatible> {
        self.result
            .get_or_init(|| validate_native_abi_version(query()))
            .clone()
    }
}

pub(crate) fn validate_native_abi_version(version: u32) -> Result<AbiCompatible> {
    if (MIN_SUPPORTED_NATIVE_ABI_VERSION..=MAX_SUPPORTED_NATIVE_ABI_VERSION).contains(&version) {
        Ok(AbiCompatible(version))
    } else {
        Err(TioError::unimplemented(format!(
            "native Arcadia TIO ABI {version} is incompatible with the safe wrapper's supported range {MIN_SUPPORTED_NATIVE_ABI_VERSION}..={MAX_SUPPORTED_NATIVE_ABI_VERSION}"
        )))
    }
}

pub(crate) fn ensure_native_abi() -> Result<AbiCompatible> {
    static NATIVE_ABI_GATE: AbiGate = AbiGate::new();
    NATIVE_ABI_GATE.get_or_query(|| {
        // SAFETY: This is the sole native function permitted before base-ABI compatibility is
        // established. Its fixed signature returns the native base ABI version without ownership.
        unsafe { sys::arcadia_tio_abi_version() }
    })
}

/// Checks the linked native base ABI once and returns its compatible version.
///
/// ABI `3` is the only base ABI supported by this wrapper release. The result,
/// including an incompatibility error, is cached for the process lifetime. The
/// compatibility error is constructed locally and does not consult native
/// last-error storage.
pub fn check_native_abi_compatibility() -> Result<u32> {
    Ok(ensure_native_abi()?.0)
}
