use super::*;

unsafe extern "C" {
    /// Returns shallow compatibility compaction stats for an open handle.
    pub fn arcadia_tio_analyze_compaction(
        handle: *mut ArcadiaTioHandle,
        out_stats: *mut ArcadiaTioCompactionStats,
    ) -> c_int;
    /// Returns non-precise V4 source-file diagnostics.
    pub fn arcadia_tio_v4_diagnostics(
        handle: *mut ArcadiaTioHandle,
        out_report: *mut ArcadiaTioV4DiagnosticsReport,
    ) -> c_int;
    /// Frees native-owned strings in a V4 diagnostics report.
    pub fn arcadia_tio_v4_diagnostics_report_free(report: *mut ArcadiaTioV4DiagnosticsReport);
    /// Returns precise V4 source-file diagnostics.
    pub fn arcadia_tio_v4_diagnostics_precise(
        handle: *mut ArcadiaTioHandle,
        options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4DiagnosticsPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise V4 diagnostics report.
    pub fn arcadia_tio_v4_diagnostics_precise_report_free(
        report: *mut ArcadiaTioV4DiagnosticsPreciseReport,
    );
    /// Returns non-precise V4 current-state compaction analysis.
    pub fn arcadia_tio_analyze_v4_compaction(
        handle: *mut ArcadiaTioHandle,
        out_report: *mut ArcadiaTioV4CompactionAnalysisReport,
    ) -> c_int;
    /// Frees native-owned strings in a V4 compaction analysis report.
    pub fn arcadia_tio_v4_compaction_analysis_report_free(
        report: *mut ArcadiaTioV4CompactionAnalysisReport,
    );
    /// Returns precise V4 current-state compaction analysis.
    pub fn arcadia_tio_analyze_v4_compaction_precise(
        handle: *mut ArcadiaTioHandle,
        options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4CompactionAnalysisPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise V4 compaction analysis report.
    pub fn arcadia_tio_v4_compaction_analysis_precise_report_free(
        report: *mut ArcadiaTioV4CompactionAnalysisPreciseReport,
    );
    /// Compacts live chunks into a destination file.
    pub fn arcadia_tio_compact_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        retain_commits: u32,
        mode: ArcadiaTioCompactionMode,
    ) -> c_int;
    /// Compacts live chunks through the portable pointer-based family.
    pub fn arcadia_tio_compact_to_ex(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        retain_commits: u32,
        mode: *const ArcadiaTioCompactionMode,
    ) -> c_int;
    /// Conditionally compacts live chunks into a destination file.
    pub fn arcadia_tio_maybe_compact(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        dead_ratio_threshold: c_double,
        min_dead_bytes: u64,
        retain_commits: u32,
        mode: ArcadiaTioCompactionMode,
        out_compacted: *mut u8,
    ) -> c_int;
    /// Conditionally compacts through the portable pointer-based family.
    pub fn arcadia_tio_maybe_compact_ex(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        dead_ratio_threshold: c_double,
        min_dead_bytes: u64,
        retain_commits: u32,
        mode: *const ArcadiaTioCompactionMode,
        out_compacted: *mut u8,
    ) -> c_int;
    /// Reads auto-compaction metadata configuration.
    pub fn arcadia_tio_get_auto_compaction_config(
        handle: *mut ArcadiaTioHandle,
        out_config: *mut ArcadiaTioAutoCompactionConfig,
        out_has_config: *mut u8,
    ) -> c_int;
    /// Updates auto-compaction metadata configuration.
    pub fn arcadia_tio_set_auto_compaction_config(
        handle: *mut ArcadiaTioHandle,
        config: *const ArcadiaTioAutoCompactionConfig,
        has_config: u8,
    ) -> c_int;
    /// Reads auto-compaction state metadata.
    pub fn arcadia_tio_compaction_state(
        handle: *mut ArcadiaTioHandle,
        out_state: *mut ArcadiaTioCompactionState,
        out_has_state: *mut u8,
    ) -> c_int;
    /// Runs metadata-configured auto-compaction if thresholds trigger.
    pub fn arcadia_tio_maybe_compact_auto(
        handle: *mut ArcadiaTioHandle,
        out_compacted: *mut u8,
    ) -> c_int;
    /// Reforms visible data into a destination file.
    pub fn arcadia_tio_reform_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioReformOptions,
    ) -> c_int;
    /// Reforms visible data into a destination file with diagnostic report output.
    pub fn arcadia_tio_reform_to_ex(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioReformOptions,
        out_report: *mut ArcadiaTioReformReport,
    ) -> c_int;
    /// Frees native-owned strings in a reform report.
    pub fn arcadia_tio_reform_report_free(report: *mut ArcadiaTioReformReport);
    /// Compacts a V4 file while retaining bounded visible commit history.
    pub fn arcadia_tio_compact_v4_retained_history_to(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        options: *const ArcadiaTioV4RetainedHistoryCompactionOptions,
        out_report: *mut ArcadiaTioV4RetainedHistoryCompactionReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a retained-history compaction report.
    pub fn arcadia_tio_v4_retained_history_compaction_report_free(
        report: *mut ArcadiaTioV4RetainedHistoryCompactionReport,
    );
    /// Compacts a V4 file while retaining bounded history and precise source accounting.
    pub fn arcadia_tio_compact_v4_retained_history_to_precise(
        handle: *mut ArcadiaTioHandle,
        dst_path: *const c_char,
        retention_options: *const ArcadiaTioV4RetainedHistoryCompactionOptions,
        precise_options: *const ArcadiaTioV4PreciseAccountingOptions,
        out_report: *mut ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
    ) -> c_int;
    /// Frees native-owned strings and arrays in a precise retained-history compaction report.
    pub fn arcadia_tio_v4_retained_history_compaction_precise_report_free(
        report: *mut ArcadiaTioV4RetainedHistoryCompactionPreciseReport,
    );
}
