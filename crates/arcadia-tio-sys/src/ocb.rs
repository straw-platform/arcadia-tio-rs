use super::*;

unsafe extern "C" {
    /// Returns machine-readable OCB error kind for the current thread.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_last_error_kind() -> ArcadiaTioOcbErrorKind;
    /// Returns machine-readable OCB failure cause for the current thread.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_last_error_cause() -> ArcadiaTioOcbFailureCause;
    /// Opens an appendable OCB file and binds the handle to the selected committed snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open(path: *const c_char) -> *mut ArcadiaTioOcbFile;
    /// Opens an appendable OCB file with explicit validation options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open_with_options(
        path: *const c_char,
        options: *const ArcadiaTioOcbOpenOptions,
    ) -> *mut ArcadiaTioOcbFile;
    /// Opens an appendable OCB file with explicit validation and finite resource limits.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open_with_options_and_resource_limits(
        path: *const c_char,
        options: *const ArcadiaTioOcbOpenOptions,
        resource_limits: *const ArcadiaTioOcbResourceLimits,
    ) -> *mut ArcadiaTioOcbFile;
    /// Clones an immutable selected-snapshot OCB reader handle.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_reader_clone(
        file: *mut ArcadiaTioOcbFile,
        out_reader: *mut *mut ArcadiaTioOcbFile,
    ) -> ArcadiaTioErrorCode;
    /// Closes an OCB handle.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_close(file: *mut ArcadiaTioOcbFile);
    /// Reads selected-snapshot OCB metadata.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_metadata(
        file: *mut ArcadiaTioOcbFile,
        out_metadata: *mut ArcadiaTioOcbMetadata,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB metadata result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_metadata_free(metadata: *mut ArcadiaTioOcbMetadata);
    /// Decodes one OCB dictionary on the explicit cold path.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_values(
        file: *mut ArcadiaTioOcbFile,
        dictionary_id: u32,
        out_values: *mut ArcadiaTioOcbDictionaryValues,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB dictionary-values result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_values_free(values: *mut ArcadiaTioOcbDictionaryValues);
    /// Reads projected/pruned OCB batches from the selected snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
    ) -> ArcadiaTioErrorCode;
    /// Reads projected/pruned OCB batches and attribution diagnostics.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches_with_attribution(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
        out_attribution: *mut ArcadiaTioOcbReadAttribution,
    ) -> ArcadiaTioErrorCode;
    /// Visits projected/pruned OCB batches incrementally.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_visit_batches(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        options: *const ArcadiaTioOcbReadCursorOptions,
        visitor: ArcadiaTioOcbBatchVisitor,
        user: *mut c_void,
        out_report: *mut ArcadiaTioOcbReadCursorReport,
    ) -> ArcadiaTioErrorCode;
    /// Creates a pull-driven bounded parallel OCB read session.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_session_create(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        row_group_ids: *const u32,
        row_group_ids_len: usize,
        options: *const ArcadiaTioOcbParallelReadOptions,
        out_session: *mut *mut ArcadiaTioOcbParallelReadSession,
    ) -> ArcadiaTioErrorCode;
    /// Blocks for the next ordered parallel OCB batch or terminal state.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_session_next(
        session: *mut ArcadiaTioOcbParallelReadSession,
        out_status: *mut ArcadiaTioOcbParallelReadNextStatus,
        out_result: *mut ArcadiaTioOcbParallelReadResult,
    ) -> ArcadiaTioErrorCode;
    /// Requests idempotent cancellation of a parallel OCB read session.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_session_cancel(
        session: *mut ArcadiaTioOcbParallelReadSession,
    ) -> ArcadiaTioErrorCode;
    /// Copies the terminal report after end or cancellation.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_session_report(
        session: *mut ArcadiaTioOcbParallelReadSession,
        out_report: *mut ArcadiaTioOcbParallelReadReport,
    ) -> ArcadiaTioErrorCode;
    /// Cancels, drains, joins, and frees a parallel OCB read session.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_session_free(
        session: *mut ArcadiaTioOcbParallelReadSession,
    );
    /// Reads one row group into caller-owned buffers.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_row_group_into(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbRowGroupFillRequest,
        out_report: *mut ArcadiaTioOcbReadFillReport,
    ) -> ArcadiaTioErrorCode;
    /// Plans an OCB read without reading payload chunks.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_plan_read(
        file: *mut ArcadiaTioOcbFile,
        request: *const ArcadiaTioOcbReadRequest,
        out_plan: *mut *mut ArcadiaTioOcbReadPlan,
    ) -> ArcadiaTioErrorCode;
    /// Copies a read-plan report into caller-provided output.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_report(
        plan: *const ArcadiaTioOcbReadPlan,
        out_report: *mut ArcadiaTioOcbReadReport,
    ) -> ArcadiaTioErrorCode;
    /// Copies projected column ids from a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_projected_column_ids(
        plan: *const ArcadiaTioOcbReadPlan,
        out_ids: *mut u32,
        out_ids_len: usize,
        out_required_len: *mut usize,
    ) -> ArcadiaTioErrorCode;
    /// Copies row-group ids from a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_row_group_ids(
        plan: *const ArcadiaTioOcbReadPlan,
        out_ids: *mut u32,
        out_ids_len: usize,
        out_required_len: *mut usize,
    ) -> ArcadiaTioErrorCode;
    /// Returns owned row-group summaries for a selected OCB snapshot.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries(
        file: *mut ArcadiaTioOcbFile,
        out_summaries: *mut ArcadiaTioOcbRowGroupSummaries,
    ) -> ArcadiaTioErrorCode;
    /// Returns owned row-group summaries for a read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_row_group_summaries(
        file: *mut ArcadiaTioOcbFile,
        plan: *const ArcadiaTioOcbReadPlan,
        out_summaries: *mut ArcadiaTioOcbRowGroupSummaries,
    ) -> ArcadiaTioErrorCode;
    /// Reads OCB batches from an existing read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_batches_from_plan(
        file: *mut ArcadiaTioOcbFile,
        plan: *const ArcadiaTioOcbReadPlan,
        row_group_ids: *const u32,
        row_group_ids_len: usize,
        out_outcome: *mut ArcadiaTioOcbReadOutcome,
    ) -> ArcadiaTioErrorCode;
    /// Frees owned fields inside an OCB read report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_report_free(report: *mut ArcadiaTioOcbReadReport);
    /// Frees owned fields inside an OCB read attribution result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_attribution_free(attribution: *mut ArcadiaTioOcbReadAttribution);
    /// Frees owned fields inside an OCB read cursor report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_report_free(report: *mut ArcadiaTioOcbReadCursorReport);
    /// Frees an opaque OCB read plan.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_plan_free(plan: *mut ArcadiaTioOcbReadPlan);
    /// Frees owned fields inside an OCB read outcome.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_outcome_free(outcome: *mut ArcadiaTioOcbReadOutcome);
    /// Frees owned fields inside a parallel OCB read result and resets it.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_result_free(result: *mut ArcadiaTioOcbParallelReadResult);
    /// Frees owned fields inside a parallel OCB read report and resets it.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_report_free(report: *mut ArcadiaTioOcbParallelReadReport);
    /// Initializes OCB primitive values.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_primitive_values_init(values: *mut ArcadiaTioOcbPrimitiveValues);
    /// Initializes an OCB validity bitmap.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_validity_bitmap_init(bitmap: *mut ArcadiaTioOcbValidityBitmap);
    /// Initializes an OCB predicate value.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_predicate_value_init(value: *mut ArcadiaTioOcbPredicateValue);
    /// Initializes an OCB row-group predicate.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_predicate_init(predicate: *mut ArcadiaTioOcbRowGroupPredicate);
    /// Initializes an OCB read request.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_request_init(request: *mut ArcadiaTioOcbReadRequest);
    /// Initializes an OCB read report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_report_init(report: *mut ArcadiaTioOcbReadReport);
    /// Initializes an OCB read attribution result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_attribution_init(attribution: *mut ArcadiaTioOcbReadAttribution);
    /// Initializes OCB read cursor options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_options_init(options: *mut ArcadiaTioOcbReadCursorOptions);
    /// Initializes an OCB read cursor report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_cursor_report_init(report: *mut ArcadiaTioOcbReadCursorReport);
    /// Initializes parallel OCB read options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_options_init(
        options: *mut ArcadiaTioOcbParallelReadOptions,
    );
    /// Initializes a parallel OCB read result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_result_init(result: *mut ArcadiaTioOcbParallelReadResult);
    /// Initializes a parallel OCB read report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_parallel_read_report_init(report: *mut ArcadiaTioOcbParallelReadReport);
    /// Initializes an OCB column fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_init(buffer: *mut ArcadiaTioOcbColumnFillBuffer);
    /// Initializes an OCB row-group fill request.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_fill_request_init(
        request: *mut ArcadiaTioOcbRowGroupFillRequest,
    );
    /// Initializes an OCB read fill report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_fill_report_init(report: *mut ArcadiaTioOcbReadFillReport);
    /// Initializes an OCB read outcome.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_read_outcome_init(outcome: *mut ArcadiaTioOcbReadOutcome);
    /// Initializes OCB open options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_open_options_init(options: *mut ArcadiaTioOcbOpenOptions);
    /// Initializes OCB resource limits to Policy A.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_resource_limits_init(resource_limits: *mut ArcadiaTioOcbResourceLimits);
    /// Initializes an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_init(column: *mut ArcadiaTioOcbWriteColumn);
    /// Initializes an OCB dictionary entry.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_dictionary_entry_init(entry: *mut ArcadiaTioOcbDictionaryEntry);
    /// Initializes an OCB write dictionary.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_dictionary_init(dictionary: *mut ArcadiaTioOcbWriteDictionary);
    /// Initializes an OCB write column chunk.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_chunk_init(chunk: *mut ArcadiaTioOcbWriteColumnChunk);
    /// Initializes an OCB write row group.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_row_group_init(row_group: *mut ArcadiaTioOcbWriteRowGroup);
    /// Initializes an OCB write ordering key.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_ordering_key_init(key: *mut ArcadiaTioOcbWriteOrderingKey);
    /// Initializes an OCB write spec.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_spec_init(spec: *mut ArcadiaTioOcbWriteSpec);
    /// Initializes OCB write options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_options_init(options: *mut ArcadiaTioOcbWriteOptions);
    /// Initializes an OCB write report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_report_init(report: *mut ArcadiaTioOcbWriteReport);
    /// Initializes an OCB row-group summary output container.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries_init(summaries: *mut ArcadiaTioOcbRowGroupSummaries);
    /// Frees an OCB row-group summary output container.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_row_group_summaries_free(summaries: *mut ArcadiaTioOcbRowGroupSummaries);
    /// Initializes an OCB cleanup result.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_result_init(result: *mut ArcadiaTioOcbCleanupResult);
    /// Initializes an OCB maintenance report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_maintenance_report_init(report: *mut ArcadiaTioOcbMaintenanceReport);
    /// Initializes an OCB cleanup report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_report_init(report: *mut ArcadiaTioOcbCleanupReport);
    /// Initializes OCB selected-snapshot export-copy options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_snapshot_export_options_init(
        options: *mut ArcadiaTioOcbSnapshotExportOptions,
    );
    /// Initializes an OCB selected-snapshot export-copy report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_snapshot_export_report_init(
        report: *mut ArcadiaTioOcbSnapshotExportReport,
    );
    /// Initializes OCB manifest build options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_build_options_init(
        options: *mut ArcadiaTioOcbManifestBuildOptions,
    );
    /// Initializes an OCB manifest carrier.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_init(manifest: *mut ArcadiaTioOcbManifest);
    /// Initializes an OCB manifest validation report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_validation_report_init(
        report: *mut ArcadiaTioOcbManifestValidationReport,
    );
    /// Initializes compact-L2 certification options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_options_init(
        options: *mut ArcadiaTioOcbCompactL2CertificationOptions,
    );
    /// Initializes a compact-L2 certification report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_report_init(
        report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    );
    /// Initializes compact-L2 physical-v2 layout facts.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_layout_facts_init(
        facts: *mut ArcadiaTioOcbCompactL2PhysicalV2LayoutFacts,
    );
    /// Initializes compact-L2 physical-v2 certification options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_certification_options_init(
        options: *mut ArcadiaTioOcbCompactL2PhysicalV2CertificationOptions,
    );
    /// Initializes compact-L2 physical-v2 artifact certification options.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_options_init(
        options: *mut ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationOptions,
    );
    /// Initializes a compact-L2 physical-v2 artifact certification report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_report_init(
        report: *mut ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport,
    );
    /// Initializes a compact-L2 physical-v2 certification report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_certification_report_init(
        report: *mut ArcadiaTioOcbCompactL2PhysicalV2CertificationReport,
    );
    /// Sets fixed-binary width metadata on an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_set_fixed_binary_width(
        column: *mut ArcadiaTioOcbWriteColumn,
        width: u32,
    );
    /// Reads fixed-binary width metadata from an OCB write column.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_write_column_fixed_binary_width(
        column: *const ArcadiaTioOcbWriteColumn,
    ) -> u32;
    /// Sets fixed-binary width metadata on an OCB fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_set_fixed_binary_width(
        buffer: *mut ArcadiaTioOcbColumnFillBuffer,
        width: u32,
    );
    /// Reads fixed-binary width metadata from an OCB fill buffer.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_fill_buffer_fixed_binary_width(
        buffer: *const ArcadiaTioOcbColumnFillBuffer,
    ) -> u32;
    /// Reads fixed-binary width metadata from an OCB column descriptor.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_descriptor_fixed_binary_width(
        column: *const ArcadiaTioOcbColumnDescriptor,
    ) -> u32;
    /// Reads fixed-binary width metadata from an OCB read column array.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_column_array_fixed_binary_width(
        column: *const ArcadiaTioOcbColumnArray,
    ) -> u32;
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
    /// Truncates orphan tail bytes after the latest valid OCB root.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_orphan_tail(
        path: *const c_char,
        out_result: *mut ArcadiaTioOcbCleanupResult,
    ) -> ArcadiaTioErrorCode;
    /// Analyzes OCB selected-snapshot maintenance state without mutating the file.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_maintenance_analyze(
        path: *const c_char,
        out_report: *mut ArcadiaTioOcbMaintenanceReport,
    ) -> ArcadiaTioErrorCode;
    /// Truncates orphan tail bytes and returns a structured cleanup report.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_orphan_tail_report(
        path: *const c_char,
        out_report: *mut ArcadiaTioOcbCleanupReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned OCB maintenance report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_maintenance_report_free(report: *mut ArcadiaTioOcbMaintenanceReport);
    /// Frees native-owned OCB cleanup report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_cleanup_report_free(report: *mut ArcadiaTioOcbCleanupReport);
    /// Copies one source file's selected committed OCB snapshot to a new destination.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_copy_selected_snapshot(
        source_path: *const c_char,
        destination_path: *const c_char,
        options: *const ArcadiaTioOcbSnapshotExportOptions,
        out_report: *mut ArcadiaTioOcbSnapshotExportReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned selected-snapshot export-copy report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_snapshot_export_report_free(
        report: *mut ArcadiaTioOcbSnapshotExportReport,
    );
    /// Builds a generic selected-snapshot OCB manifest from local files.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_build_from_files(
        manifest_path: *const c_char,
        input_paths: *const *const c_char,
        input_paths_len: usize,
        options: *const ArcadiaTioOcbManifestBuildOptions,
        out_manifest: *mut ArcadiaTioOcbManifest,
    ) -> ArcadiaTioErrorCode;
    /// Validates a generic selected-snapshot OCB manifest against local files.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_validate_files(
        manifest_path: *const c_char,
        manifest: *const ArcadiaTioOcbManifest,
        options: *const ArcadiaTioOcbOpenOptions,
        out_report: *mut ArcadiaTioOcbManifestValidationReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned OCB manifest strings and arrays.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_free(manifest: *mut ArcadiaTioOcbManifest);
    /// Frees native-owned OCB manifest validation report issues.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_manifest_validation_report_free(
        report: *mut ArcadiaTioOcbManifestValidationReport,
    );
    /// Certifies a channel-sharded compact-L2 manifest.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_certify_compact_l2_manifest(
        manifest_path: *const c_char,
        options: *const ArcadiaTioOcbCompactL2CertificationOptions,
        out_report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned compact-L2 certification report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_certification_report_free(
        report: *mut ArcadiaTioOcbCompactL2CertificationReport,
    );
    /// Returns compact-L2 physical-v2 layout facts.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_layout_facts(
        out_facts: *mut ArcadiaTioOcbCompactL2PhysicalV2LayoutFacts,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned compact-L2 physical-v2 layout facts data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_layout_facts_free(
        facts: *mut ArcadiaTioOcbCompactL2PhysicalV2LayoutFacts,
    );
    /// Certifies one local compact-L2 physical-v2 artifact.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_certify_compact_l2_physical_v2_artifact(
        artifact_path: *const c_char,
        options: *const ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationOptions,
        out_report: *mut ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned compact-L2 physical-v2 artifact certification report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_report_free(
        report: *mut ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport,
    );
    /// Certifies a compact-L2 physical-v2 channel-sharded manifest.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_certify_compact_l2_physical_v2_manifest(
        manifest_path: *const c_char,
        options: *const ArcadiaTioOcbCompactL2PhysicalV2CertificationOptions,
        out_report: *mut ArcadiaTioOcbCompactL2PhysicalV2CertificationReport,
    ) -> ArcadiaTioErrorCode;
    /// Frees native-owned compact-L2 physical-v2 certification report data.
    #[cfg(feature = "format-ocb")]
    pub fn arcadia_tio_ocb_compact_l2_physical_v2_certification_report_free(
        report: *mut ArcadiaTioOcbCompactL2PhysicalV2CertificationReport,
    );
}
