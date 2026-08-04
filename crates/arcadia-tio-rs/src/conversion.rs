use super::*;

pub(crate) fn shape_element_len(shape: &[u64]) -> Result<usize> {
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

pub(crate) fn validate_tensor_parts(dtype: DType, shape: &[u64], data: &TensorData) -> Result<()> {
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

pub(crate) fn validate_create_with_coordinates_v2_options(
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

pub(crate) fn validate_create_policy(
    options: &CreateOptions,
    policy: &CreatePolicyOptions,
) -> Result<()> {
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

pub(crate) fn copy_shape(raw: &sys::ArcadiaTioTensor) -> Result<Vec<u64>> {
    // SAFETY: Native tensor shape pointer is valid for `rank` while the tensor output is alive.
    unsafe { copy_checked_slice(raw.shape.cast_const(), raw.rank, "tensor shape") }
}

pub(crate) fn copy_tensor(raw: &sys::ArcadiaTioTensor) -> Result<Tensor> {
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
    let data = match dtype {
        // SAFETY: The C ABI guarantees the allocation remains alive through this conversion; the
        // shared helper validates null/length, alignment, and byte-size preconditions before copy.
        DType::F32 => TensorData::F32(unsafe {
            copy_checked_slice(raw.data.cast::<f32>(), element_count, "f32 tensor values")
        }?),
        DType::F64 => TensorData::F64(unsafe {
            copy_checked_slice(raw.data.cast::<f64>(), element_count, "f64 tensor values")
        }?),
        DType::I32 => TensorData::I32(unsafe {
            copy_checked_slice(raw.data.cast::<i32>(), element_count, "i32 tensor values")
        }?),
        DType::I64 => TensorData::I64(unsafe {
            copy_checked_slice(raw.data.cast::<i64>(), element_count, "i64 tensor values")
        }?),
    };
    Ok(Tensor { dtype, shape, data })
}

pub(crate) fn copy_mask(raw: &sys::ArcadiaTioMask) -> Result<Option<Vec<u8>>> {
    if raw.len == 0 {
        return Ok(None);
    }
    // SAFETY: The C ABI returns a native-owned mask with `len` bytes while the mask output is alive.
    Ok(Some(unsafe {
        copy_checked_slice(raw.data.cast_const(), raw.len, "dense validity mask")
    }?))
}

pub(crate) struct NativeCommitList {
    pub(crate) raw: sys::ArcadiaTioCommitList,
}

impl NativeCommitList {
    pub(crate) fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioCommitList {
                items: ptr::null_mut(),
                len: 0,
            },
        }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioCommitList {
        &mut self.raw
    }

    pub(crate) fn as_ref(&self) -> &sys::ArcadiaTioCommitList {
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

pub(crate) struct NativeChunkPlan {
    pub(crate) raw: sys::ArcadiaTioChunkPlan,
}

impl NativeChunkPlan {
    pub(crate) fn new() -> Self {
        Self {
            raw: sys::ArcadiaTioChunkPlan {
                block_sizes: ptr::null_mut(),
                len: 0,
            },
        }
    }

    pub(crate) fn as_mut_ptr(&mut self) -> *mut sys::ArcadiaTioChunkPlan {
        &mut self.raw
    }

    pub(crate) fn as_ref(&self) -> &sys::ArcadiaTioChunkPlan {
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

pub(crate) fn copy_commit_list(raw: &sys::ArcadiaTioCommitList) -> Result<Vec<CommitInfo>> {
    // SAFETY: The C ABI returns `len` commit records owned by the commit-list output while alive.
    Ok(
        unsafe { checked_slice(raw.items.cast_const(), raw.len, "commit list") }?
            .iter()
            .copied()
            .map(CommitInfo::from)
            .collect(),
    )
}

pub(crate) fn copy_chunk_plan(raw: &sys::ArcadiaTioChunkPlan) -> Result<ChunkPlan> {
    // SAFETY: The C ABI returns `len` block-size entries owned by the chunk-plan output while alive.
    Ok(ChunkPlan {
        block_sizes: unsafe {
            copy_checked_slice(
                raw.block_sizes.cast_const(),
                raw.len,
                "chunk-plan block sizes",
            )
        }?,
    })
}

pub(crate) fn new_query_trace_json() -> sys::ArcadiaTioQueryTraceJson {
    sys::ArcadiaTioQueryTraceJson {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceJson>(),
        json: ptr::null_mut(),
    }
}

pub(crate) fn copy_query_trace_json(raw: &sys::ArcadiaTioQueryTraceJson) -> Result<QueryTraceJson> {
    let json = required_c_string(raw.json.cast_const(), "query trace JSON")?;
    Ok(QueryTraceJson { json })
}

pub(crate) fn new_read_execution_report() -> sys::ArcadiaTioReadExecutionReport {
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

pub(crate) fn new_read_index_report() -> sys::ArcadiaTioReadIndexReport {
    sys::ArcadiaTioReadIndexReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReadIndexReport>(),
        lowering_kind: sys::ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN,
        used_full_tensor_fallback: 0,
        reserved0: [0; 7],
    }
}

pub(crate) fn new_historical_read_execution_report() -> sys::ArcadiaTioHistoricalReadExecutionReport
{
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

pub(crate) fn new_historical_read_index_report() -> sys::ArcadiaTioHistoricalReadIndexReport {
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

pub(crate) fn copy_read_execution_report(
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
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const())?,
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        )?,
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const())?,
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        )?,
    })
}

pub(crate) fn copy_read_index_report(
    raw: &sys::ArcadiaTioReadIndexReport,
) -> Result<ReadIndexReport> {
    Ok(ReadIndexReport {
        lowering_kind: ReadIndexLoweringKind::from_raw(raw.lowering_kind)?,
        used_full_tensor_fallback: raw.used_full_tensor_fallback != 0,
    })
}

pub(crate) fn copy_historical_read_execution_report(
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
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const())?,
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        )?,
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const())?,
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        )?,
    };
    Ok(HistoricalReadExecutionReport {
        execution,
        query_source_kind: HistoricalQuerySourceKind::from_raw(raw.query_source_kind)?,
        query_commit_seq: raw.query_commit_seq,
    })
}

pub(crate) fn copy_historical_read_index_report(
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
        query_parallel_runtime: optional_c_string(raw.query_parallel_runtime.cast_const())?,
        query_parallel_fallback_reason: optional_c_string(
            raw.query_parallel_fallback_reason.cast_const(),
        )?,
        query_parallel_reason_code: optional_c_string(raw.query_parallel_reason_code.cast_const())?,
        query_parallel_reason_code_taxonomy: optional_c_string(
            raw.query_parallel_reason_code_taxonomy.cast_const(),
        )?,
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

pub(crate) fn new_v4_precise_accounting_bytes() -> sys::ArcadiaTioV4PreciseAccountingBytes {
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

pub(crate) fn copy_v4_precise_accounting_bytes(
    raw: &sys::ArcadiaTioV4PreciseAccountingBytes,
) -> Result<V4PreciseAccountingBytes> {
    if raw.omitted_fields_len != raw.omitted_field_reason_codes_len {
        return Err(TioError::conversion(
            "native V4 precise-accounting field and reason-code lengths differ",
        ));
    }
    // SAFETY: Native report owns both aligned arrays until the parent report is freed.
    let fields = unsafe {
        checked_slice(
            raw.omitted_fields.cast_const(),
            raw.omitted_fields_len,
            "V4 omitted precise-accounting fields",
        )
    }?;
    let reason_codes = unsafe {
        checked_slice(
            raw.omitted_field_reason_codes.cast_const(),
            raw.omitted_field_reason_codes_len,
            "V4 omitted precise-accounting reason codes",
        )
    }?;
    let omitted_fields = fields
        .iter()
        .zip(reason_codes)
        .map(|(field, reason_code)| {
            Ok(V4OmittedPreciseAccountingField {
                field: V4PreciseAccountingField::from_raw(field.field),
                reason: optional_c_string(field.reason.cast_const())?,
                reason_code: optional_c_string((*reason_code).cast_const())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(V4PreciseAccountingBytes {
        unreachable_bytes: (raw.has_unreachable_bytes != 0).then_some(raw.unreachable_bytes),
        retained_history_required_bytes: (raw.has_retained_history_required_bytes != 0)
            .then_some(raw.retained_history_required_bytes),
        popped_skipped_bytes: (raw.has_popped_skipped_bytes != 0)
            .then_some(raw.popped_skipped_bytes),
        reclaimable_bytes: (raw.has_reclaimable_bytes != 0).then_some(raw.reclaimable_bytes),
        omitted_fields,
    })
}

pub(crate) fn copy_v4_current_head_bytes(
    raw: sys::ArcadiaTioV4CurrentHeadBytes,
) -> V4CurrentHeadBytes {
    V4CurrentHeadBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
        commit_bytes: raw.commit_bytes,
    }
}

pub(crate) fn copy_v4_audit_bytes(raw: sys::ArcadiaTioV4AuditBytes) -> V4AuditBytes {
    V4AuditBytes {
        commit_bytes: raw.commit_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

pub(crate) fn copy_v4_payload_reuse_bytes(
    raw: sys::ArcadiaTioV4PayloadReuseBytes,
) -> V4PayloadReuseBytes {
    V4PayloadReuseBytes {
        resurrected_payload_bytes: raw.resurrected_payload_bytes,
        shared_payload_bytes: raw.shared_payload_bytes,
    }
}

pub(crate) fn copy_v4_superseded_bytes(raw: sys::ArcadiaTioV4SupersededBytes) -> V4SupersededBytes {
    V4SupersededBytes {
        payload_bytes: raw.payload_bytes,
        index_bytes: raw.index_bytes,
        epoch_bytes: raw.epoch_bytes,
        aux_bytes: raw.aux_bytes,
    }
}

pub(crate) fn new_v4_diagnostics_report() -> sys::ArcadiaTioV4DiagnosticsReport {
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

pub(crate) fn copy_v4_diagnostics_report(
    raw: &sys::ArcadiaTioV4DiagnosticsReport,
) -> Result<V4DiagnosticsReport> {
    Ok(V4DiagnosticsReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        )?,
    })
}

pub(crate) fn new_v4_diagnostics_precise_report() -> sys::ArcadiaTioV4DiagnosticsPreciseReport {
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

pub(crate) fn copy_v4_diagnostics_precise_report(
    raw: &sys::ArcadiaTioV4DiagnosticsPreciseReport,
) -> Result<V4DiagnosticsPreciseReport> {
    Ok(V4DiagnosticsPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        current_head: copy_v4_current_head_bytes(raw.current_head),
        visible_chain_audit: copy_v4_audit_bytes(raw.visible_chain_audit),
        payload_reuse: copy_v4_payload_reuse_bytes(raw.payload_reuse),
        superseded: copy_v4_superseded_bytes(raw.superseded),
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting)?,
        reason_code: optional_c_string(raw.reason_code.cast_const())?,
    })
}

pub(crate) fn new_v4_compaction_analysis_report() -> sys::ArcadiaTioV4CompactionAnalysisReport {
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

pub(crate) fn copy_v4_compaction_analysis_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisReport,
) -> Result<V4CompactionAnalysisReport> {
    Ok(V4CompactionAnalysisReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        )?,
    })
}

pub(crate) fn new_v4_compaction_analysis_precise_report()
-> sys::ArcadiaTioV4CompactionAnalysisPreciseReport {
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

pub(crate) fn copy_v4_compaction_analysis_precise_report(
    raw: &sys::ArcadiaTioV4CompactionAnalysisPreciseReport,
) -> Result<V4CompactionAnalysisPreciseReport> {
    Ok(V4CompactionAnalysisPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        policy: V4CompactionAnalysisPolicy::from_raw(raw.policy)?,
        source_file_bytes: raw.source_file_bytes,
        current_state_required_bytes: raw.current_state_required_bytes,
        ordinary_reclaimable_bytes: raw.ordinary_reclaimable_bytes,
        unknown_bytes: raw.unknown_bytes,
        precise_accounting: copy_v4_precise_accounting_bytes(&raw.precise_accounting)?,
        reason_code: optional_c_string(raw.reason_code.cast_const())?,
    })
}

pub(crate) fn new_v4_retained_history_compaction_report()
-> sys::ArcadiaTioV4RetainedHistoryCompactionReport {
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

pub(crate) fn copy_retained_commit_seqs(ptr: *mut u64, len: usize) -> Result<Vec<u64>> {
    // SAFETY: Native report owns `len` entries until the parent report is freed.
    unsafe { copy_checked_slice(ptr.cast_const(), len, "retained commit sequences") }
}

pub(crate) fn copy_v4_retained_history_compaction_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionReport,
) -> Result<V4RetainedHistoryCompactionReport> {
    if raw.retained_commit_seqs_len != raw.retained_commit_count as usize {
        return Err(TioError::conversion(
            "native retained-history report count does not match sequence length",
        ));
    }
    Ok(V4RetainedHistoryCompactionReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        )?,
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        omitted_unreachable_bytes: raw.omitted_unreachable_bytes != 0,
        omitted_unreachable_bytes_reason: optional_c_string(
            raw.omitted_unreachable_bytes_reason.cast_const(),
        )?,
    })
}

pub(crate) fn new_v4_retained_history_compaction_precise_report()
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

pub(crate) fn copy_v4_retained_history_compaction_precise_report(
    raw: &sys::ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
) -> Result<V4RetainedHistoryCompactionPreciseReport> {
    if raw.retained_commit_seqs_len != raw.retained_commit_count as usize {
        return Err(TioError::conversion(
            "native precise retained-history report count does not match sequence length",
        ));
    }
    Ok(V4RetainedHistoryCompactionPreciseReport {
        status: V4ReportStatus::from_raw(raw.status),
        reason: optional_c_string(raw.reason.cast_const())?,
        retained_commit_count: raw.retained_commit_count,
        retained_commit_seqs: copy_retained_commit_seqs(
            raw.retained_commit_seqs,
            raw.retained_commit_seqs_len,
        )?,
        unretained_older_commit_count: (raw.has_unretained_older_commit_count != 0)
            .then_some(raw.unretained_older_commit_count),
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        precise_source_accounting: copy_v4_precise_accounting_bytes(
            &raw.precise_source_accounting,
        )?,
        reason_code: optional_c_string(raw.reason_code.cast_const())?,
    })
}

pub(crate) fn new_reform_report() -> sys::ArcadiaTioReformReport {
    sys::ArcadiaTioReformReport {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioReformReport>(),
        reason_code: ptr::null_mut(),
        reason_code_taxonomy: ptr::null_mut(),
        reason: ptr::null_mut(),
    }
}

pub(crate) fn copy_reform_report(raw: &sys::ArcadiaTioReformReport) -> Result<ReformReport> {
    Ok(ReformReport {
        reason_code: optional_c_string(raw.reason_code.cast_const())?,
        reason_code_taxonomy: optional_c_string(raw.reason_code_taxonomy.cast_const())?,
        reason: optional_c_string(raw.reason.cast_const())?,
    })
}

pub(crate) fn new_auto_compaction_config() -> sys::ArcadiaTioAutoCompactionConfig {
    AutoCompactionConfig::default().to_raw()
}

pub(crate) fn copy_auto_compaction_config(
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

pub(crate) fn copy_axis_labels(
    ptr: *mut sys::ArcadiaTioAxisLabel,
    len: usize,
    label: &str,
) -> Result<Vec<AxisLabel>> {
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { checked_slice(ptr.cast_const(), len, label) }?
        .iter()
        .map(|item| {
            Ok(AxisLabel {
                id: item.id,
                name: required_c_string(item.name.cast_const(), "axis-label name")?,
            })
        })
        .collect()
}

pub(crate) fn copy_user_kv(ptr: *mut sys::ArcadiaTioUserKv, len: usize) -> Result<Vec<UserKv>> {
    // SAFETY: Metadata arrays are valid for `len` while the native metadata object is alive.
    unsafe { checked_slice(ptr.cast_const(), len, "user metadata entries") }?
        .iter()
        .map(|item| {
            Ok(UserKv {
                key: required_c_string(item.key.cast_const(), "user metadata key")?,
                value: required_c_string(item.value.cast_const(), "user metadata value")?,
            })
        })
        .collect()
}

pub(crate) fn empty_file_meta_output() -> sys::ArcadiaTioFileMeta {
    sys::ArcadiaTioFileMeta {
        dtype: sys::ARCADIA_TIO_DTYPE_F32,
        dims: ptr::null_mut(),
        rank: 0,
        append_dim: 0,
        symbols: ptr::null_mut(),
        symbols_len: 0,
        channels: ptr::null_mut(),
        channels_len: 0,
        user_kv: ptr::null_mut(),
        user_kv_len: 0,
        effective_profile: sys::ARCADIA_TIO_HEADER_PROFILE_STREAMING,
        commit_seq: 0,
    }
}

pub(crate) fn copy_file_meta(raw: &sys::ArcadiaTioFileMeta) -> Result<FileMeta> {
    if raw.rank == 0 {
        return Err(TioError::conversion(
            "native file metadata returned zero rank",
        ));
    }
    if raw.append_dim >= raw.rank {
        return Err(TioError::conversion(format!(
            "native file metadata append dimension {} is out of range for rank {}",
            raw.append_dim, raw.rank
        )));
    }
    // SAFETY: Metadata dimension array is valid for `rank` while the native metadata object is alive.
    let dims = unsafe { checked_slice(raw.dims.cast_const(), raw.rank, "file dimensions") }?
        .iter()
        .map(|dim| {
            Ok(DimSpec {
                kind: AxisKind::from_raw(dim.kind)?,
                len: dim.len,
                name: optional_c_string(dim.name.cast_const())?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(FileMeta {
        dtype: DType::from_raw(raw.dtype)?,
        dims,
        append_dim: raw.append_dim,
        symbols: copy_axis_labels(raw.symbols, raw.symbols_len, "symbol labels")?,
        channels: copy_axis_labels(raw.channels, raw.channels_len, "channel labels")?,
        user_kv: copy_user_kv(raw.user_kv, raw.user_kv_len)?,
        effective_profile: HeaderProfile::from_raw(raw.effective_profile)?,
        commit_seq: raw.commit_seq,
    })
}

pub(crate) fn copy_coordinate_meta(
    ptr: *mut sys::ArcadiaTioAxisCoordinateMeta,
    len: usize,
) -> Result<Vec<CoordinateMeta>> {
    // SAFETY: Coordinate metadata array is valid for `len` until freed by the caller.
    unsafe { checked_slice(ptr.cast_const(), len, "coordinate metadata") }?
        .iter()
        .map(|item| {
            Ok(CoordinateMeta {
                axis: item.axis,
                axis_name_snapshot: optional_c_string(item.axis_name_snapshot.cast_const())?,
                name: optional_c_string(item.name.cast_const())?,
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
                external_uri: optional_c_string(item.external_uri.cast_const())?,
                required: item.required != 0,
                validation_status: CoordinateValidationStatus::from_raw(item.validation_status)?,
            })
        })
        .collect()
}

pub(crate) fn copy_coordinate_meta_v2(
    ptr: *mut sys::ArcadiaTioAxisCoordinateMetaV2,
    len: usize,
) -> Result<Vec<AxisCoordinateMetaV2>> {
    // SAFETY: Coordinate v2 metadata array is valid for `len` until freed by the caller.
    unsafe { checked_slice(ptr.cast_const(), len, "Coordinate v2 metadata") }?
        .iter()
        .map(AxisCoordinateMetaV2::from_raw)
        .collect()
}
