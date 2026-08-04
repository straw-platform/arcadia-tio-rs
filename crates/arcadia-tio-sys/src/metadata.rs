use super::*;

unsafe extern "C" {
    /// Reads rank for an open handle.
    pub fn arcadia_tio_rank(handle: *mut ArcadiaTioHandle, out_rank: *mut usize) -> c_int;
    /// Reads dtype for an open handle.
    pub fn arcadia_tio_dtype(
        handle: *mut ArcadiaTioHandle,
        out_dtype: *mut ArcadiaTioDType,
    ) -> c_int;
    /// Reads append-axis index for an open handle.
    pub fn arcadia_tio_append_axis(
        handle: *mut ArcadiaTioHandle,
        out_append_axis: *mut usize,
    ) -> c_int;
    /// Reads index-checkpoint interval metadata.
    pub fn arcadia_tio_get_index_checkpoint_every_commits(
        handle: *mut ArcadiaTioHandle,
        out_every_commits: *mut u32,
    ) -> c_int;
    /// Updates index-checkpoint interval metadata.
    pub fn arcadia_tio_set_index_checkpoint_every_commits(
        handle: *mut ArcadiaTioHandle,
        every_commits: u32,
    ) -> c_int;
    /// Updates or clears one dimension name.
    pub fn arcadia_tio_set_dim_name(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        name: *const c_char,
        has_name: u8,
    ) -> c_int;
    /// Replaces Symbol-axis labels from borrowed strings.
    pub fn arcadia_tio_set_symbols(
        handle: *mut ArcadiaTioHandle,
        symbols: *const *const c_char,
        symbols_len: usize,
    ) -> c_int;
    /// Replaces Channel-axis labels from borrowed strings.
    pub fn arcadia_tio_set_channels(
        handle: *mut ArcadiaTioHandle,
        channels: *const *const c_char,
        channels_len: usize,
    ) -> c_int;
    /// Replaces user metadata key/value pairs from borrowed strings.
    pub fn arcadia_tio_set_user_kv(
        handle: *mut ArcadiaTioHandle,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> c_int;
    /// Reads current dimension lengths.
    pub fn arcadia_tio_dim_lens(
        handle: *mut ArcadiaTioHandle,
        out_dim_lens: *mut u32,
        out_dim_lens_len: usize,
    ) -> c_int;
    /// Reads the native chunk plan into a native-owned plan carrier.
    pub fn arcadia_tio_chunk_plan(
        handle: *mut ArcadiaTioHandle,
        out_plan: *mut ArcadiaTioChunkPlan,
    ) -> c_int;
    /// Reads current file path into a native-owned string.
    pub fn arcadia_tio_path(handle: *mut ArcadiaTioHandle, out_path: *mut *mut c_char) -> c_int;
    /// Frees native-owned strings returned by string APIs.
    pub fn arcadia_tio_string_free(value: *mut c_char);
    /// Frees native-owned chunk plan arrays.
    pub fn arcadia_tio_chunk_plan_free(plan: *mut ArcadiaTioChunkPlan);
    /// Frees native-owned file metadata.
    pub fn arcadia_tio_file_meta_free(meta: *mut ArcadiaTioFileMeta);
}
