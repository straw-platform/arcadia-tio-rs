use super::*;

unsafe extern "C" {
    /// Returns a borrowed pointer to the last error message for the current thread.
    pub fn arcadia_tio_last_error_message() -> *const c_char;
    /// Returns the last error code for the current thread.
    pub fn arcadia_tio_last_error_code() -> ArcadiaTioErrorCode;
    /// Returns the native library ABI version.
    pub fn arcadia_tio_abi_version() -> u32;
    /// Returns the pointer-based compaction function-family version.
    pub fn arcadia_tio_compaction_abi_version() -> u32;
}
