use super::*;

unsafe extern "C" {
    /// Appends f32 payload data.
    pub fn arcadia_tio_append_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends f32 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_f32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f32 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_f32_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data.
    pub fn arcadia_tio_append_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends f64 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_f64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_f64_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data.
    pub fn arcadia_tio_append_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends i32 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_i32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_i32_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data.
    pub fn arcadia_tio_append_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Appends i64 payload data and returns assigned entry range.
    pub fn arcadia_tio_append_i64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data with universe bindings and returns assigned entry range.
    pub fn arcadia_tio_append_i64_with_universe(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        options: *const ArcadiaTioAppendWithUniverseOptions,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f32 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_f32_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_f64_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_i32_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 payload data with Coordinate v2 append-axis batches.
    pub fn arcadia_tio_append_i64_with_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        coordinates: *const ArcadiaTioAppendCoordinateBatchV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;

    /// Analyzes how sparse-intent f32 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent f64 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i32 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i64 data would be appended.
    pub fn arcadia_tio_analyze_sparse_append_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Appends f32 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_f32(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends f32 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_f32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_f64(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends f64 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_f64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_i32(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends i32 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_i32_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 data using sparse-intent analysis and best-effort lowering.
    pub fn arcadia_tio_append_sparse_i64(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
    ) -> c_int;
    /// Appends i64 sparse-intent data and returns an optional assigned entry range.
    pub fn arcadia_tio_append_sparse_i64_with_range(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRule,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Analyzes how sparse-intent f32 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_f32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent f64 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_f64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i32 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_i32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Analyzes how sparse-intent i64 data would be appended using a V2 sparse rule.
    pub fn arcadia_tio_analyze_sparse_append_i64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_analysis: *mut ArcadiaTioSparseAppendAnalysis,
    ) -> c_int;
    /// Appends f32 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_f32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends f32 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_f32_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends f64 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_f64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends f64 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_f64_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i32 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_i32_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends i32 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_i32_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i32,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Appends i64 data using sparse-intent V2 analysis.
    pub fn arcadia_tio_append_sparse_i64_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
    ) -> c_int;
    /// Appends i64 sparse-intent data using a V2 sparse rule and returns an optional range.
    pub fn arcadia_tio_append_sparse_i64_with_range_v2(
        handle: *mut ArcadiaTioHandle,
        data: *const i64,
        shape: *const u64,
        rank: usize,
        rule: *const ArcadiaTioSparseRuleV2,
        out_start_entry: *mut u32,
        out_end_entry: *mut u32,
    ) -> c_int;
    /// Frees native-owned reason arrays in a sparse append analysis.
    pub fn arcadia_tio_sparse_append_analysis_free(analysis: *mut ArcadiaTioSparseAppendAnalysis);

    /// Rewrites one selected entry with f32 payload data.
    pub fn arcadia_tio_rewrite_f32(
        handle: *mut ArcadiaTioHandle,
        selector: *const ArcadiaTioEntrySelector,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites one selected entry with f64 payload data.
    pub fn arcadia_tio_rewrite_f64(
        handle: *mut ArcadiaTioHandle,
        selector: *const ArcadiaTioEntrySelector,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites a selector slice with f32 payload data.
    pub fn arcadia_tio_rewrite_slice_f32(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        data: *const c_float,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Rewrites a selector slice with f64 payload data.
    pub fn arcadia_tio_rewrite_slice_f64(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        data: *const c_double,
        shape: *const u64,
        rank: usize,
    ) -> c_int;
    /// Clears storage blocks for borrowed chunk keys.
    pub fn arcadia_tio_clear_blocks(
        handle: *mut ArcadiaTioHandle,
        keys: *const ArcadiaTioChunkKey,
        keys_len: usize,
    ) -> c_int;
}
