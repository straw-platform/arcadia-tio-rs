//! Bounded, single-owner append sessions over the native writer ABI.

use super::*;

/// An unpublished append suffix holding the native mutation lock.
///
/// Create and use on one writer thread (neither `Send` nor `Sync`). Payload is
/// borrowed only during each call, encoded in bounded batches, and written in
/// input order; index/stat metadata still grows with the suffix. Only commit
/// publishes a new root. Any error is terminal, including Rust-side validation.
/// Drop best-effort rolls back an active session; use `abort` to observe errors.
/// This is not crash recovery. Matching native session symbols are required;
/// there is no fallback to repeated one-shot append.
pub struct AppendSession {
    raw: Option<NativePointerOutput<sys::ArcadiaTioOcbAppendSession>>,
    columns: Vec<WriteColumn>,
}

impl AppendSession {
    /// Maximum row groups per batch, not a payload-byte or metadata-memory limit.
    pub const MAX_BATCH_ROW_GROUPS: usize = sys::ARCADIA_TIO_OCB_APPEND_SESSION_MAX_ROW_GROUPS;

    /// Admit the frozen base once. `schema.row_groups` must be empty.
    pub fn begin(
        path: impl AsRef<Path>,
        schema: WriteSpec,
        options: WriteOptions,
    ) -> OcbResult<Self> {
        // This gates the native ABI before any session/init/error/free entry point.
        let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
        if !schema.row_groups.is_empty() {
            return Err(OcbError::invalid_input(
                "OCB append session schema must not contain row groups",
            ));
        }
        let raw_schema = RawWriteSpec::new(&schema)?;
        let raw_options = options.to_raw();
        let mut raw = NativePointerOutput::new(sys::arcadia_tio_ocb_append_session_free);
        let status = unsafe {
            sys::arcadia_tio_ocb_append_session_begin(
                path.as_ptr(),
                &raw_schema.raw,
                &raw_options,
                raw.out(),
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB append session begin failed"));
        }
        if raw.get().is_null() {
            return Err(OcbError::invalid_input(
                "OCB append session begin returned a null handle",
            ));
        }
        Ok(Self {
            raw: Some(raw),
            columns: schema.columns,
        })
    }

    fn take_active(&mut self) -> OcbResult<NativePointerOutput<sys::ArcadiaTioOcbAppendSession>> {
        self.raw
            .take()
            .ok_or_else(|| OcbError::invalid_input("OCB append session is no longer active"))
    }

    /// Append one row group, leaving the root unpublished.
    pub fn append_row_group(&mut self, row_group: WriteRowGroup) -> OcbResult<()> {
        self.append_batch(vec![row_group])
    }

    /// Encode 1..=32 groups using the selected threads and merge in input order.
    pub fn append_batch(&mut self, row_groups: Vec<WriteRowGroup>) -> OcbResult<()> {
        // RAII rollback/free on any error or unwind, including local conversion.
        let raw = self.take_active()?;
        if row_groups.is_empty() || row_groups.len() > Self::MAX_BATCH_ROW_GROUPS {
            return Err(OcbError::invalid_input(
                "OCB append batch requires 1..=32 row groups",
            ));
        }
        let spec = WriteSpec {
            columns: self.columns.clone(),
            dictionaries: Vec::new(),
            row_groups,
            ordering_keys: Vec::new(),
        };
        let raw_spec = RawWriteSpec::new(&spec)?;
        let status = unsafe {
            sys::arcadia_tio_ocb_append_session_append_batch(
                raw.get(),
                raw_spec.raw.row_groups,
                raw_spec.raw.row_groups_len,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB append session batch failed"));
        }
        self.raw = Some(raw);
        Ok(())
    }

    /// Publish one root and consume the session.
    /// An error during publication may have committed; recover/reopen, never retry blindly.
    pub fn commit(self) -> OcbResult<()> {
        self.commit_with_report().map(|_| ())
    }

    /// Publish one root and return terminal diagnostic counters on success only.
    /// An error during publication may have committed; recover/reopen, never retry blindly.
    pub fn commit_with_report(mut self) -> OcbResult<WriteReport> {
        let raw = self.take_active()?;
        let mut report = empty_write_report();
        let status = unsafe { sys::arcadia_tio_ocb_append_session_commit(raw.get(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB append session commit failed"));
        }
        Ok(write_report_from_raw(&report))
    }

    /// Consume an active session and report unpublished-tail rollback errors.
    pub fn abort(mut self) -> OcbResult<()> {
        let raw = self.take_active()?;
        let status = unsafe { sys::arcadia_tio_ocb_append_session_abort(raw.get()) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB append session abort failed"));
        }
        Ok(())
    }
}
