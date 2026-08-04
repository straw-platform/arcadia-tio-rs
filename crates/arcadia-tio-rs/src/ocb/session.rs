use super::*;

impl ParallelReadSession {
    /// Block on the calling thread for the next ordered batch or terminal state.
    pub fn next(&self) -> OcbResult<ParallelReadNext> {
        let mut raw_result = empty_parallel_read_result();
        let mut raw_status = sys::ARCADIA_TIO_OCB_PARALLEL_READ_NEXT_END;
        let status = unsafe {
            sys::arcadia_tio_ocb_parallel_read_session_next(
                self.raw.as_ptr(),
                &mut raw_status,
                &mut raw_result,
            )
        };
        let guard = ParallelReadResultGuard(raw_result);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB parallel read next failed"));
        }
        match raw_status {
            sys::ARCADIA_TIO_OCB_PARALLEL_READ_NEXT_BATCH => {
                let context = ParallelReadContext {
                    selected_row_group_ordinal: guard.0.context.selected_row_group_ordinal,
                    row_group_id: guard.0.context.row_group_id,
                    base_row: guard.0.context.base_row,
                    row_end: guard.0.context.row_end,
                    row_count: guard.0.context.row_count,
                    worker_id: guard.0.context.worker_id,
                };
                let batch = unsafe { column_batch_from_raw(&guard.0.batch) }?;
                Ok(ParallelReadNext::Batch(ParallelReadBatch {
                    context,
                    batch,
                }))
            }
            sys::ARCADIA_TIO_OCB_PARALLEL_READ_NEXT_END => Ok(ParallelReadNext::End),
            sys::ARCADIA_TIO_OCB_PARALLEL_READ_NEXT_CANCELLED => Ok(ParallelReadNext::Cancelled),
            other => Err(OcbError::invalid_input(format!(
                "unknown OCB parallel read next status {other}"
            ))),
        }
    }

    /// Request cancellation. This is idempotent and can run while `next` blocks.
    /// Successful completion may win the race; inspect the terminal status.
    pub fn cancel(&self) -> OcbResult<()> {
        let status =
            unsafe { sys::arcadia_tio_ocb_parallel_read_session_cancel(self.raw.as_ptr()) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB parallel read cancel failed"));
        }
        Ok(())
    }

    /// Copy the terminal report after `End` or `Cancelled`.
    pub fn report(&self) -> OcbResult<ParallelReadReport> {
        let mut raw_report = empty_parallel_read_report();
        let status = unsafe {
            sys::arcadia_tio_ocb_parallel_read_session_report(self.raw.as_ptr(), &mut raw_report)
        };
        let guard = ParallelReadReportGuard(raw_report);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB parallel read report failed"));
        }
        unsafe { parallel_read_report_from_raw(&guard.0) }
    }
}

impl Iterator for ParallelReadSession {
    type Item = OcbResult<ParallelReadBatch>;

    fn next(&mut self) -> Option<Self::Item> {
        match ParallelReadSession::next(self) {
            Ok(ParallelReadNext::Batch(batch)) => Some(Ok(batch)),
            Ok(ParallelReadNext::End | ParallelReadNext::Cancelled) => None,
            Err(error) => Some(Err(error)),
        }
    }
}

impl Drop for ParallelReadSession {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_parallel_read_session_free(self.raw.as_ptr()) };
    }
}

// SAFETY: Native session ownership moves with this wrapper. The C ABI
// serializes `next` and permits `cancel` concurrently with a blocked `next`.
unsafe impl Send for ParallelReadSession {}
// SAFETY: Shared calls are synchronized by the native session; dropping is
// exclusive under Rust ownership.
unsafe impl Sync for ParallelReadSession {}
