use super::*;

impl ColumnBundleFile {
    /// Open an OCB file and bind this handle to the selected committed snapshot.
    pub fn open(path: impl AsRef<Path>) -> OcbResult<Self> {
        open(path)
    }

    /// Open an OCB file with explicit validation options.
    pub fn open_with_options(path: impl AsRef<Path>, options: OpenOptions) -> OcbResult<Self> {
        open_with_options(path, options)
    }

    /// Open an OCB file with explicit finite resource limits.
    pub fn open_with_resource_limits(
        path: impl AsRef<Path>,
        resource_limits: ResourceLimits,
    ) -> OcbResult<Self> {
        open_with_resource_limits(path, resource_limits)
    }

    /// Open an OCB file with explicit validation and finite resource limits.
    pub fn open_with_options_and_resource_limits(
        path: impl AsRef<Path>,
        options: OpenOptions,
        resource_limits: ResourceLimits,
    ) -> OcbResult<Self> {
        open_with_options_and_resource_limits(path, options, resource_limits)
    }

    /// Clone this selected-snapshot reader handle.
    ///
    /// The clone observes the same immutable committed OCB snapshot as this
    /// handle. Reopen the file path to observe later appends.
    pub fn clone_reader(&self) -> OcbResult<Self> {
        let mut raw_reader = NativePointerOutput::new(sys::arcadia_tio_ocb_close);
        let status =
            unsafe { sys::arcadia_tio_ocb_reader_clone(self.raw.as_ptr(), raw_reader.out()) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB reader_clone failed"));
        }
        NonNull::new(raw_reader.take())
            .map(|raw| ColumnBundleFile {
                raw,
                _abi: self._abi,
            })
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
            sys::arcadia_tio_ocb_read_batches(self.raw.as_ptr(), &raw_request.raw, &mut raw_outcome)
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
        let mut attribution = read_attribution_from_raw(&attribution_guard.0)?;
        attribution.wrapper_copy_ns = Some(duration_to_ns(wrapper_started.elapsed()));
        Ok(AttributedReadOutcome {
            outcome,
            attribution,
        })
    }

    /// Create a pull-driven bounded parallel OCB read session.
    ///
    /// An empty `row_group_ids` slice selects every row group chosen by the
    /// request. A nonempty slice is duplicate/unknown-id checked and then
    /// normalized to deterministic plan order. The request and slice are
    /// borrowed only during this call; the returned session owns its native
    /// selected-snapshot state.
    pub fn parallel_read_session(
        &self,
        request: &ReadRequest,
        row_group_ids: &[u32],
        options: ParallelReadOptions,
    ) -> OcbResult<ParallelReadSession> {
        let raw_request = RawReadRequest::new(request)?;
        let raw_options = sys::ArcadiaTioOcbParallelReadOptions {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbParallelReadOptions>(),
            max_in_flight_row_groups: options.max_in_flight_row_groups,
            reserved: [0; 8],
        };
        let mut raw_session =
            NativePointerOutput::new(sys::arcadia_tio_ocb_parallel_read_session_free);
        let status = unsafe {
            sys::arcadia_tio_ocb_parallel_read_session_create(
                self.raw.as_ptr(),
                &raw_request.raw,
                if row_group_ids.is_empty() {
                    ptr::null()
                } else {
                    row_group_ids.as_ptr()
                },
                row_group_ids.len(),
                &raw_options,
                raw_session.out(),
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB parallel_read_session failed"));
        }
        NonNull::new(raw_session.take())
            .map(|raw| ParallelReadSession {
                raw,
                _abi: self._abi,
            })
            .ok_or_else(|| OcbError::last("OCB parallel_read_session returned null session"))
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
        read_cursor_report_from_raw(&report_guard.0)
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
            sys::arcadia_tio_ocb_read_row_group_into(self.raw.as_ptr(), &raw.raw, &mut raw_report)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB read_row_group_into failed"));
        }
        read_fill_report_from_raw(&raw_report, &raw.raw_columns)
    }

    /// Plan a projected/pruned read without reading column payloads.
    pub fn plan_read<'a>(&'a self, request: &ReadRequest) -> OcbResult<ReadPlan<'a>> {
        let raw_request = RawReadRequest::new(request)?;
        let mut raw_plan = NativePointerOutput::new(sys::arcadia_tio_ocb_read_plan_free);
        let status = unsafe {
            sys::arcadia_tio_ocb_plan_read(self.raw.as_ptr(), &raw_request.raw, raw_plan.out())
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(OcbError::last("OCB plan_read failed"));
        }
        let raw = NonNull::new(raw_plan.get())
            .ok_or_else(|| OcbError::last("OCB plan_read returned null plan"))?;
        let report = read_plan_report(raw)?;
        let projected_column_ids = read_plan_projected_column_ids(raw)?;
        let row_group_ids = read_plan_row_group_ids(raw)?;
        let raw = NonNull::new(raw_plan.take()).expect("validated OCB read plan stays non-null");
        Ok(ReadPlan {
            raw,
            file_raw: self.raw,
            _abi: self._abi,
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
    let abi = ensure_native_abi().map_err(OcbError::from_tio_error)?;
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let raw_options = raw_open_options(options);
    let raw = unsafe { sys::arcadia_tio_ocb_open_with_options(path.as_ptr(), &raw_options) };
    NonNull::new(raw)
        .map(|raw| ColumnBundleFile { raw, _abi: abi })
        .ok_or_else(|| OcbError::last("OCB open failed"))
}

/// Open an OCB file with explicit finite resource limits.
pub fn open_with_resource_limits(
    path: impl AsRef<Path>,
    resource_limits: ResourceLimits,
) -> OcbResult<ColumnBundleFile> {
    open_with_options_and_resource_limits(path, OpenOptions::default(), resource_limits)
}

/// Open an OCB file with explicit validation and finite resource limits.
pub fn open_with_options_and_resource_limits(
    path: impl AsRef<Path>,
    options: OpenOptions,
    resource_limits: ResourceLimits,
) -> OcbResult<ColumnBundleFile> {
    let abi = ensure_native_abi().map_err(OcbError::from_tio_error)?;
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let raw_options = raw_open_options(options);
    let raw_resource_limits = raw_resource_limits(resource_limits);
    let raw = unsafe {
        sys::arcadia_tio_ocb_open_with_options_and_resource_limits(
            path.as_ptr(),
            &raw_options,
            &raw_resource_limits,
        )
    };
    NonNull::new(raw)
        .map(|raw| ColumnBundleFile { raw, _abi: abi })
        .ok_or_else(|| OcbError::last("OCB open failed"))
}
