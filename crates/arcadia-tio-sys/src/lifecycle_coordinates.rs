use super::*;

unsafe extern "C" {
    /// Sets write-time compression for future appends.
    pub fn arcadia_tio_set_compression_config(
        handle: *mut ArcadiaTioHandle,
        config: *const ArcadiaTioCompressionConfig,
    ) -> c_int;
    /// Gets write-time compression for future appends.
    pub fn arcadia_tio_get_compression_config(
        handle: *const ArcadiaTioHandle,
        out_config: *mut ArcadiaTioCompressionConfig,
    ) -> c_int;

    /// Creates a random-access V4 TensorFile.
    pub fn arcadia_tio_create_random_access(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with metadata overrides.
    pub fn arcadia_tio_create_random_access_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with universe-aware axis identity options.
    pub fn arcadia_tio_create_random_access_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile.
    pub fn arcadia_tio_create_streaming(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with metadata overrides.
    pub fn arcadia_tio_create_streaming_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with universe-aware axis identity options.
    pub fn arcadia_tio_create_streaming_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile using inferred layout-family selection.
    pub fn arcadia_tio_create_inferred(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile using inferred layout-family selection and metadata overrides.
    pub fn arcadia_tio_create_inferred_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile using inferred layout-family selection, metadata overrides, and coordinate descriptors.
    pub fn arcadia_tio_create_inferred_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking.
    pub fn arcadia_tio_create_with_policy(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking and metadata overrides.
    pub fn arcadia_tio_create_with_policy_ex(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking, metadata overrides, and coordinate descriptors.
    pub fn arcadia_tio_create_with_policy_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with policy-based chunking and universe options.
    pub fn arcadia_tio_create_with_policy_with_universe(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
        options: *const ArcadiaTioCreateWithUniverseOptions,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with coordinate descriptors.
    pub fn arcadia_tio_create_random_access_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with coordinate descriptors.
    pub fn arcadia_tio_create_streaming_with_coordinates(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInput,
        coordinates_len: usize,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a RegularChunked V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_with_policy_with_coordinates_v2(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        chunk_axes: *const usize,
        chunk_axes_len: usize,
        storage_profile: ArcadiaTioStorageProfile,
        typical_query_sizes: *const u32,
        typical_query_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a V4 TensorFile with inferred layout selection and Coordinate v2 descriptors.
    pub fn arcadia_tio_create_inferred_with_coordinates_v2(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        storage_access: ArcadiaTioStorageAccessKind,
        open_pattern: ArcadiaTioOpenPattern,
        file_population: ArcadiaTioFilePopulation,
        metadata_stability: ArcadiaTioMetadataStability,
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a random-access V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_random_access_with_coordinates_v2(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Creates a streaming V4 TensorFile with Coordinate v2 descriptors.
    pub fn arcadia_tio_create_streaming_with_coordinates_v2(
        path: *const c_char,
        dtype: ArcadiaTioDType,
        dim_kinds: *const ArcadiaTioAxisKind,
        dim_lens: *const u32,
        rank: usize,
        append_dim: usize,
        dim_names: *const *const c_char,
        dim_names_len: usize,
        symbols: *const *const c_char,
        symbols_len: usize,
        channels: *const *const c_char,
        channels_len: usize,
        user_kv_keys: *const *const c_char,
        user_kv_values: *const *const c_char,
        user_kv_len: usize,
        coordinates: *const ArcadiaTioAxisCoordinateInputV2,
        coordinates_len: usize,
        options: *const ArcadiaTioCoordinateV2Options,
    ) -> *mut ArcadiaTioHandle;
    /// Opens an existing TensorFile.
    pub fn arcadia_tio_open(path: *const c_char) -> *mut ArcadiaTioHandle;
    /// Closes a handle returned by create/open functions.
    pub fn arcadia_tio_close(handle: *mut ArcadiaTioHandle);

    /// Loads file metadata without opening a handle.
    pub fn arcadia_tio_load_meta(path: *const c_char, out_meta: *mut ArcadiaTioFileMeta) -> c_int;
    /// Reads coordinate descriptors from an open handle.
    pub fn arcadia_tio_coordinate_meta(
        handle: *mut ArcadiaTioHandle,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMeta,
        out_len: *mut usize,
    ) -> c_int;
    /// Loads coordinate descriptors without opening a handle.
    pub fn arcadia_tio_load_coordinate_meta(
        path: *const c_char,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMeta,
        out_len: *mut usize,
    ) -> c_int;
    /// Frees coordinate metadata arrays returned by metadata APIs.
    pub fn arcadia_tio_axis_coordinate_meta_free(
        meta: *mut ArcadiaTioAxisCoordinateMeta,
        len: usize,
    );
    /// Reads inline axis coordinate values into an owned tensor.
    pub fn arcadia_tio_read_axis_coordinates(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        out_values: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Looks up the unique axis index for an inline validated i32 coordinate value.
    pub fn arcadia_tio_coordinate_index_i32(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        value: i32,
        out_index: *mut u32,
    ) -> c_int;
    /// Looks up the unique axis index for an inline validated i64 coordinate value.
    pub fn arcadia_tio_coordinate_index_i64(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        value: i64,
        out_index: *mut u32,
    ) -> c_int;
    /// Looks up the half-open axis-index range for an inclusive i32 coordinate interval.
    pub fn arcadia_tio_coordinate_range_i32(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: i32,
        end: i32,
        out_start: *mut u32,
        out_end: *mut u32,
    ) -> c_int;
    /// Looks up the half-open axis-index range for an inclusive i64 coordinate interval.
    pub fn arcadia_tio_coordinate_range_i64(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: i64,
        end: i64,
        out_start: *mut u32,
        out_end: *mut u32,
    ) -> c_int;

    /// Reads Coordinate v2 descriptors from an open handle.
    pub fn arcadia_tio_coordinate_meta_v2(
        handle: *mut ArcadiaTioHandle,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMetaV2,
        out_len: *mut usize,
    ) -> c_int;
    /// Loads Coordinate v2 descriptors without opening a handle.
    pub fn arcadia_tio_load_coordinate_meta_v2(
        path: *const c_char,
        out_meta: *mut *mut ArcadiaTioAxisCoordinateMetaV2,
        out_len: *mut usize,
    ) -> c_int;
    /// Frees Coordinate v2 metadata arrays returned by metadata APIs.
    pub fn arcadia_tio_axis_coordinate_meta_v2_free(
        meta: *mut ArcadiaTioAxisCoordinateMetaV2,
        len: usize,
    );
    /// Reads Coordinate v2 values for one axis into an owned value carrier.
    pub fn arcadia_tio_read_axis_coordinates_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        options: *const ArcadiaTioCoordinateV2Options,
        out_values: *mut ArcadiaTioCoordinateValueSliceV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 value slice.
    pub fn arcadia_tio_coordinate_value_slice_v2_free(
        values: *mut ArcadiaTioCoordinateValueSliceV2,
    );
    /// Reads Coordinate v2 dictionary metadata and entries.
    pub fn arcadia_tio_coordinate_dictionary_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        options: *const ArcadiaTioCoordinateV2Options,
        out_dictionary: *mut ArcadiaTioCoordinateDictionaryV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 dictionary result.
    pub fn arcadia_tio_coordinate_dictionary_v2_free(
        dictionary: *mut ArcadiaTioCoordinateDictionaryV2,
    );
    /// Performs an exact Coordinate v2 lookup.
    pub fn arcadia_tio_coordinate_lookup_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Performs a half-open range Coordinate v2 lookup.
    pub fn arcadia_tio_coordinate_lookup_range_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Performs an exact Coordinate v2 lookup against a retained historical commit.
    pub fn arcadia_tio_coordinate_lookup_at_commit_v2(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Performs a half-open range Coordinate v2 lookup against a retained historical commit.
    pub fn arcadia_tio_coordinate_lookup_range_at_commit_v2(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        options: *const ArcadiaTioCoordinateV2Options,
        out_result: *mut ArcadiaTioCoordinateLookupResultV2,
    ) -> c_int;
    /// Frees an owned Coordinate v2 lookup result.
    pub fn arcadia_tio_coordinate_lookup_result_v2_free(
        result: *mut ArcadiaTioCoordinateLookupResultV2,
    );
}
