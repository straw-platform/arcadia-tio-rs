//! OCB one-shot and session writer declarations.

use super::*;

/// Maximum row groups per append-session batch (not a byte budget).
pub const ARCADIA_TIO_OCB_APPEND_SESSION_MAX_ROW_GROUPS: usize = 32;

/// Opaque, single-owner unpublished append session.
#[repr(C)]
pub struct ArcadiaTioOcbAppendSession {
    _private: [u8; 0],
}

unsafe extern "C" {
    /// Admit one base and hold its mutation lock. Schema row_groups must be empty.
    /// Inputs are borrowed only for the call. Output is null on failure.
    /// Requires the matching session symbol family in the selected native library.
    pub fn arcadia_tio_ocb_append_session_begin(
        path: *const c_char,
        schema: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
        out_session: *mut *mut ArcadiaTioOcbAppendSession,
    ) -> ArcadiaTioErrorCode;
    /// Append 1..=32 groups, borrowing arrays only until return. No root publication.
    /// Calls on a handle must not overlap. Any error leaves the handle terminal.
    pub fn arcadia_tio_ocb_append_session_append_batch(
        session: *mut ArcadiaTioOcbAppendSession,
        row_groups: *const ArcadiaTioOcbWriteRowGroup,
        row_groups_len: usize,
    ) -> ArcadiaTioErrorCode;
    /// Publish once and fill an initialized scalar report; read it only on success.
    /// Always terminal. A publication error may have committed: recover, do not retry.
    pub fn arcadia_tio_ocb_append_session_commit(
        session: *mut ArcadiaTioOcbAppendSession,
        out_report: *mut ArcadiaTioOcbWriteReport,
    ) -> ArcadiaTioErrorCode;
    /// Terminal rollback with error reporting; free is still required.
    pub fn arcadia_tio_ocb_append_session_abort(
        session: *mut ArcadiaTioOcbAppendSession,
    ) -> ArcadiaTioErrorCode;
    /// Free exactly once, best-effort rolling back an active session. Null is a no-op.
    /// No concurrent calls or subsequent use of the freed handle are allowed.
    pub fn arcadia_tio_ocb_append_session_free(session: *mut ArcadiaTioOcbAppendSession);

    /// Creates an appendable OCB file.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_create(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
    ) -> ArcadiaTioErrorCode;
    /// Creates an appendable OCB file with explicit writer options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_create_with_options(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
    ) -> ArcadiaTioErrorCode;
    /// Creates an appendable OCB file and returns diagnostic counters.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_create_with_options_and_report(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
        out_report: *mut ArcadiaTioOcbWriteReport,
    ) -> ArcadiaTioErrorCode;
    /// Appends one sorted suffix commit to an existing appendable OCB file.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_append(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
    ) -> ArcadiaTioErrorCode;
    /// Appends to an OCB file with explicit writer options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_append_with_options(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
    ) -> ArcadiaTioErrorCode;
    /// Appends to an OCB file and returns diagnostic counters.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_append_with_options_and_report(
        path: *const c_char,
        spec: *const ArcadiaTioOcbWriteSpec,
        options: *const ArcadiaTioOcbWriteOptions,
        out_report: *mut ArcadiaTioOcbWriteReport,
    ) -> ArcadiaTioErrorCode;
}
