use super::*;

/// RAII TensorFile handle over the native C ABI.
///
/// The wrapper closes the native handle exactly once in `Drop`. It deliberately does not
/// implement `Send` or `Sync` in this first slice because the C ABI handle thread-safety contract
/// is not documented for concurrent mutation.
pub struct TensorFile {
    pub(crate) raw: NonNull<sys::ArcadiaTioHandle>,
    pub(crate) _abi: AbiCompatible,
    pub(crate) _not_send_or_sync: PhantomData<Rc<()>>,
}

impl TensorFile {
    /// Creates a TensorFile from safe create options.
    pub fn create(path: impl AsRef<Path>, options: CreateOptions) -> Result<Self> {
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_coordinates(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    prepared.coordinate_ptr(),
                    prepared.coordinate_len(),
                ),
                CreateLayout::RandomAccess => {
                    sys::arcadia_tio_create_random_access_with_coordinates(
                        prepared.path.as_ptr(),
                        options.dtype.to_raw(),
                        prepared.dim_kinds.as_ptr(),
                        prepared.dim_lens.as_ptr(),
                        prepared.dim_lens.len(),
                        options.append_dim,
                        prepared.dim_name_ptr(),
                        prepared.dim_name_len(),
                        prepared.symbol_ptr(),
                        prepared.symbol_len(),
                        prepared.channel_ptr(),
                        prepared.channel_len(),
                        prepared.user_key_ptr(),
                        prepared.user_value_ptr(),
                        prepared.user_kv_len(),
                        prepared.coordinate_ptr(),
                        prepared.coordinate_len(),
                    )
                }
            }
        };
        let file = Self::from_raw_handle(raw, prepared.abi, "failed to create TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_with_coordinates_v2(path, options, coordinates, coordinate_options)
    }

    /// Creates a TensorFile from Coordinate v2 descriptors while leaving v1 `CoordinateSpec` helpers unchanged.
    pub fn create_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns common create strings/vectors and PreparedAxisCoordinateInputsV2
        // owns Coordinate v2 C strings/dictionary/external helper storage for the duration of this call.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_coordinates_v2(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    prepared_coordinates.ptr(),
                    prepared_coordinates.len(),
                    &raw_coordinate_options,
                ),
                CreateLayout::RandomAccess => {
                    sys::arcadia_tio_create_random_access_with_coordinates_v2(
                        prepared.path.as_ptr(),
                        options.dtype.to_raw(),
                        prepared.dim_kinds.as_ptr(),
                        prepared.dim_lens.as_ptr(),
                        prepared.dim_lens.len(),
                        options.append_dim,
                        prepared.dim_name_ptr(),
                        prepared.dim_name_len(),
                        prepared.symbol_ptr(),
                        prepared.symbol_len(),
                        prepared.channel_ptr(),
                        prepared.channel_len(),
                        prepared.user_key_ptr(),
                        prepared.user_value_ptr(),
                        prepared.user_kv_len(),
                        prepared_coordinates.ptr(),
                        prepared_coordinates.len(),
                        &raw_coordinate_options,
                    )
                }
            }
        };
        let file = Self::from_raw_handle(
            raw,
            prepared.abi,
            "failed to create Coordinate v2 TensorFile",
        )?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile with universe-aware axis identity options.
    ///
    /// Coordinate descriptors cannot be combined with universe create options in this wrapper slice
    /// because the current C ABI exposes separate coordinate and universe create families.
    pub fn create_with_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with universe create options yet",
            ));
        }
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            match options.layout {
                CreateLayout::Streaming => sys::arcadia_tio_create_streaming_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
                CreateLayout::RandomAccess => sys::arcadia_tio_create_random_access_with_universe(
                    prepared.path.as_ptr(),
                    options.dtype.to_raw(),
                    prepared.dim_kinds.as_ptr(),
                    prepared.dim_lens.as_ptr(),
                    prepared.dim_lens.len(),
                    options.append_dim,
                    prepared.dim_name_ptr(),
                    prepared.dim_name_len(),
                    prepared.symbol_ptr(),
                    prepared.symbol_len(),
                    prepared.channel_ptr(),
                    prepared.channel_len(),
                    prepared.user_key_ptr(),
                    prepared.user_value_ptr(),
                    prepared.user_kv_len(),
                    &raw_options,
                ),
            }
        };
        let file = Self::from_raw_handle(
            raw,
            prepared.abi,
            "failed to create universe-aware TensorFile",
        )?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a TensorFile using native inferred layout-family selection.
    ///
    /// Inline coordinate descriptors are accepted for fixed non-append axes; external
    /// coordinate storage and append-axis coordinate growth are rejected by the native API.
    pub fn create_inferred(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
    ) -> Result<Self> {
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            sys::arcadia_tio_create_inferred_with_coordinates(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                inferred_options.storage_access.to_raw(),
                inferred_options.open_pattern.to_raw(),
                inferred_options.file_population.to_raw(),
                inferred_options.metadata_stability.to_raw(),
                prepared.coordinate_ptr(),
                prepared.coordinate_len(),
            )
        };
        let file =
            Self::from_raw_handle(raw, prepared.abi, "failed to create inferred TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates an inferred-layout TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_inferred_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_inferred_with_coordinates_v2(
            path,
            options,
            inferred_options,
            coordinates,
            coordinate_options,
        )
    }

    /// Creates an inferred-layout TensorFile from Coordinate v2 descriptors.
    pub fn create_inferred_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        inferred_options: CreateInferredOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate and PreparedAxisCoordinateInputsV2 keep all borrowed raw pointers
        // valid until the C ABI create call returns.
        let raw = unsafe {
            sys::arcadia_tio_create_inferred_with_coordinates_v2(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                inferred_options.storage_access.to_raw(),
                inferred_options.open_pattern.to_raw(),
                inferred_options.file_population.to_raw(),
                inferred_options.metadata_stability.to_raw(),
                prepared_coordinates.ptr(),
                prepared_coordinates.len(),
                &raw_coordinate_options,
            )
        };
        let file = Self::from_raw_handle(
            raw,
            prepared.abi,
            "failed to create inferred Coordinate v2 TensorFile",
        )?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile using native policy-based chunking.
    ///
    /// Inline coordinate descriptors are accepted for fixed non-append axes; external
    /// coordinate storage and append-axis coordinate growth are rejected by the native API.
    pub fn create_with_policy(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
    ) -> Result<Self> {
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate owns all borrowed C strings/vectors for the duration of this call.
        // Pointers and lengths match the owned Rust slices in `prepared` and `options`.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_coordinates(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                prepared.coordinate_ptr(),
                prepared.coordinate_len(),
            )
        };
        let file = Self::from_raw_handle(raw, prepared.abi, "failed to create policy TensorFile")?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile from coordinate descriptors using the current coordinate API.
    pub fn create_with_policy_with_coordinates(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        coordinates: &[AxisCoordinateInput],
        coordinate_options: CoordinateOptions,
    ) -> Result<Self> {
        Self::create_with_policy_with_coordinates_v2(
            path,
            options,
            policy_options,
            coordinates,
            coordinate_options,
        )
    }

    /// Creates a RegularChunked TensorFile from Coordinate v2 descriptors.
    pub fn create_with_policy_with_coordinates_v2(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        coordinates: &[AxisCoordinateInputV2],
        coordinate_options: CoordinateV2Options,
    ) -> Result<Self> {
        validate_create_policy(&options, &policy_options)?;
        validate_create_with_coordinates_v2_options(&options, coordinate_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_coordinates =
            PreparedAxisCoordinateInputsV2::new(coordinates, options.dims.len())?;
        let raw_coordinate_options = coordinate_options.to_raw();
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        // SAFETY: PreparedCreate and PreparedAxisCoordinateInputsV2 keep all borrowed raw pointers
        // valid until the C ABI create call returns.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_coordinates_v2(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                prepared_coordinates.ptr(),
                prepared_coordinates.len(),
                &raw_coordinate_options,
            )
        };
        let file = Self::from_raw_handle(
            raw,
            prepared.abi,
            "failed to create policy Coordinate v2 TensorFile",
        )?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Creates a RegularChunked TensorFile with native policy chunking and universe-aware axes.
    ///
    /// Coordinate descriptors cannot be combined with policy+universe create in this wrapper slice
    /// because the current C ABI exposes no policy+universe+coordinate create family.
    pub fn create_with_policy_and_universe(
        path: impl AsRef<Path>,
        options: CreateOptions,
        policy_options: CreatePolicyOptions,
        universe_options: CreateUniverseOptions,
    ) -> Result<Self> {
        if !options.coordinates.is_empty() {
            return Err(TioError::invalid_argument(
                "coordinate descriptors cannot be combined with policy universe create options yet",
            ));
        }
        validate_create_policy(&options, &policy_options)?;
        let prepared = PreparedCreate::new(path, &options)?;
        let prepared_universe = PreparedCreateUniverseOptions::new(&universe_options);
        let compression = options
            .compression
            .map(CompressionConfig::validate)
            .transpose()?;
        let raw_universe_options = prepared_universe.raw_options();
        // SAFETY: PreparedCreate and PreparedCreateUniverseOptions own all borrowed C data for the
        // duration of this call. Pointers and lengths match the owned Rust slices.
        let raw = unsafe {
            sys::arcadia_tio_create_with_policy_with_universe(
                prepared.path.as_ptr(),
                options.dtype.to_raw(),
                prepared.dim_kinds.as_ptr(),
                prepared.dim_lens.as_ptr(),
                prepared.dim_lens.len(),
                options.append_dim,
                prepared.dim_name_ptr(),
                prepared.dim_name_len(),
                prepared.symbol_ptr(),
                prepared.symbol_len(),
                prepared.channel_ptr(),
                prepared.channel_len(),
                prepared.user_key_ptr(),
                prepared.user_value_ptr(),
                prepared.user_kv_len(),
                policy_options.chunk_axes.as_ptr(),
                policy_options.chunk_axes.len(),
                policy_options.storage_profile.to_raw(),
                policy_options.typical_query_sizes.as_ptr(),
                policy_options.typical_query_sizes.len(),
                &raw_universe_options,
            )
        };
        let file = Self::from_raw_handle(
            raw,
            prepared.abi,
            "failed to create policy universe TensorFile",
        )?;
        if let Some(compression) = compression {
            file.set_compression(compression)?;
        }
        Ok(file)
    }

    /// Set write-time compression for future appends on this handle.
    pub fn set_compression(&self, compression: CompressionConfig) -> Result<()> {
        let raw = compression.validate()?.to_raw();
        let status = unsafe { sys::arcadia_tio_set_compression_config(self.raw.as_ptr(), &raw) };
        status_result(status, "failed to set compression config")
    }

    /// Opens an existing TensorFile.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let abi = ensure_native_abi()?;
        let path = path_to_cstring(path)?;
        // SAFETY: The C string is valid for the duration of this call.
        let raw = unsafe { sys::arcadia_tio_open(path.as_ptr()) };
        Self::from_raw_handle(raw, abi, "failed to open TensorFile")
    }

    /// Loads metadata without keeping a TensorFile handle open.
    pub fn load_meta(path: impl AsRef<Path>) -> Result<FileMeta> {
        let path = path_to_cstring(path)?;
        let mut raw = NativeOutput::new(empty_file_meta_output(), sys::arcadia_tio_file_meta_free);
        // SAFETY: `raw` points to initialized output storage and the path is live for the call.
        let status = unsafe { sys::arcadia_tio_load_meta(path.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to load TensorFile metadata")?;
        copy_file_meta(raw.as_ref())
    }

    /// Loads coordinate metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_meta(path: impl AsRef<Path>) -> Result<Vec<CoordinateMeta>> {
        let path = path_to_cstring(path)?;
        let mut raw_meta = NativeArrayOutput::new(sys::arcadia_tio_axis_coordinate_meta_free);
        // SAFETY: The path C string and out pointers are valid for the duration of this call.
        let status = unsafe {
            sys::arcadia_tio_load_coordinate_meta(
                path.as_ptr(),
                raw_meta.ptr_out(),
                raw_meta.len_out(),
            )
        };
        status_result(status, "failed to load coordinate metadata")?;
        let (ptr, len) = raw_meta.parts();
        copy_coordinate_meta(ptr, len)
    }

    /// Loads current coordinate metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_metadata(path: impl AsRef<Path>) -> Result<Vec<AxisCoordinateMeta>> {
        Self::load_coordinate_meta_v2(path)
    }

    /// Loads Coordinate v2 metadata without keeping a TensorFile handle open.
    pub fn load_coordinate_meta_v2(path: impl AsRef<Path>) -> Result<Vec<AxisCoordinateMetaV2>> {
        let path = path_to_cstring(path)?;
        let mut raw_meta = NativeArrayOutput::new(sys::arcadia_tio_axis_coordinate_meta_v2_free);
        // SAFETY: The path C string and out pointers are valid for the duration of this call.
        let status = unsafe {
            sys::arcadia_tio_load_coordinate_meta_v2(
                path.as_ptr(),
                raw_meta.ptr_out(),
                raw_meta.len_out(),
            )
        };
        status_result(status, "failed to load Coordinate v2 metadata")?;
        let (ptr, len) = raw_meta.parts();
        copy_coordinate_meta_v2(ptr, len)
    }

    /// Returns the native C ABI version reported by the linked library.
    pub fn native_abi_version() -> u32 {
        // SAFETY: Version query has no preconditions.
        unsafe { sys::arcadia_tio_abi_version() }
    }

    /// Returns the tensor rank.
    pub fn rank(&self) -> Result<usize> {
        let mut rank = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_rank(self.raw.as_ptr(), &mut rank) };
        status_result(status, "failed to read TensorFile rank")?;
        Ok(rank)
    }

    /// Returns the payload dtype.
    pub fn dtype(&self) -> Result<DType> {
        let mut dtype = sys::ARCADIA_TIO_DTYPE_F32;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_dtype(self.raw.as_ptr(), &mut dtype) };
        status_result(status, "failed to read TensorFile dtype")?;
        DType::from_raw(dtype)
    }

    /// Returns the append-axis index.
    pub fn append_axis(&self) -> Result<usize> {
        let mut axis = 0usize;
        // SAFETY: `self.raw` is a live native handle and out pointer is valid.
        let status = unsafe { sys::arcadia_tio_append_axis(self.raw.as_ptr(), &mut axis) };
        status_result(status, "failed to read TensorFile append axis")?;
        Ok(axis)
    }

    /// Returns the current dimension lengths.
    pub fn dim_lens(&self) -> Result<Vec<u32>> {
        let rank = self.rank()?;
        let mut dims = vec![0u32; rank];
        // SAFETY: `dims` has exactly `rank` writable elements and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_dim_lens(self.raw.as_ptr(), dims.as_mut_ptr(), dims.len()) };
        status_result(status, "failed to read TensorFile dimension lengths")?;
        Ok(dims)
    }

    /// Returns the native index-checkpoint interval in commits.
    pub fn index_checkpoint_every_commits(&self) -> Result<u32> {
        let mut every_commits = 0u32;
        // SAFETY: `every_commits` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_index_checkpoint_every_commits(
                self.raw.as_ptr(),
                &mut every_commits,
            )
        };
        status_result(status, "failed to read index checkpoint interval")?;
        Ok(every_commits)
    }

    /// Updates the native index-checkpoint interval in commits.
    ///
    /// The interval must be at least one. Native implementations that do not support this
    /// metadata update return an ordinary wrapper error without changing the file.
    pub fn set_index_checkpoint_every_commits(&mut self, every_commits: u32) -> Result<()> {
        if every_commits == 0 {
            return Err(TioError::invalid_argument(
                "index checkpoint interval must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe {
            sys::arcadia_tio_set_index_checkpoint_every_commits(self.raw.as_ptr(), every_commits)
        };
        status_result(status, "failed to set index checkpoint interval")
    }

    /// Returns the native chunking plan copied into Rust-owned memory.
    pub fn chunk_plan(&self) -> Result<ChunkPlan> {
        let mut raw_plan = NativeChunkPlan::new();
        // SAFETY: `raw_plan` is a valid output pointer and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_chunk_plan(self.raw.as_ptr(), raw_plan.as_mut_ptr()) };
        status_result(status, "failed to read chunk plan")?;
        copy_chunk_plan(raw_plan.as_ref())
    }

    /// Updates or clears one dimension name through the native metadata administration API.
    ///
    /// Passing `None` clears the name. Native implementations that do not support metadata-only
    /// updates return an ordinary wrapper error without changing the file.
    pub fn set_dim_name(&mut self, axis: usize, name: Option<&str>) -> Result<()> {
        if matches!(name, Some("")) {
            return Err(TioError::invalid_argument("dimension name cannot be empty"));
        }
        let name = name
            .map(|value| string_to_cstring(value, "dimension name"))
            .transpose()?;
        let (ptr, has_name) = match name.as_ref() {
            Some(value) => (value.as_ptr(), 1),
            None => (ptr::null(), 0),
        };
        // SAFETY: Optional name CString, when present, outlives the call and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_set_dim_name(self.raw.as_ptr(), axis, ptr, has_name) };
        status_result(status, "failed to set dimension name")
    }

    /// Replaces Symbol-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_symbols<S: AsRef<str>>(&mut self, symbols: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(symbols, "symbol label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_symbols(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set symbol labels")
    }

    /// Replaces Channel-axis labels through the native metadata administration API.
    ///
    /// Native implementations may reject shrinking or unsupported metadata-only updates.
    pub fn set_channels<S: AsRef<str>>(&mut self, channels: &[S]) -> Result<()> {
        let prepared = PreparedStringList::new(channels, "channel label")?;
        // SAFETY: Prepared C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_channels(self.raw.as_ptr(), prepared.ptr(), prepared.len())
        };
        status_result(status, "failed to set channel labels")
    }

    /// Replaces user key/value metadata through the native metadata administration API.
    ///
    /// Passing an empty slice requests clearing all user metadata. Native implementations that do
    /// not support metadata-only updates return an ordinary wrapper error without changing the file.
    pub fn set_user_kv<K, V>(&mut self, user_kv: &[(K, V)]) -> Result<()>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let prepared = PreparedUserKvList::new(user_kv)?;
        // SAFETY: Prepared key/value C string pointers outlive the call and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_set_user_kv(
                self.raw.as_ptr(),
                prepared.key_ptr(),
                prepared.value_ptr(),
                prepared.len(),
            )
        };
        status_result(status, "failed to set user metadata")
    }

    /// Returns the native path snapshot for this handle.
    pub fn path(&self) -> Result<String> {
        let mut raw_path = NativePointerOutput::new(sys::arcadia_tio_string_free);
        // SAFETY: `raw_path` is a valid out pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_path(self.raw.as_ptr(), raw_path.out()) };
        status_result(status, "failed to read TensorFile path")?;
        required_c_string(raw_path.get().cast_const(), "TensorFile path")
    }

    /// Reads coordinate metadata from the open handle.
    pub fn coordinate_meta(&self) -> Result<Vec<CoordinateMeta>> {
        let mut raw_meta = NativeArrayOutput::new(sys::arcadia_tio_axis_coordinate_meta_free);
        // SAFETY: Out pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_coordinate_meta(
                self.raw.as_ptr(),
                raw_meta.ptr_out(),
                raw_meta.len_out(),
            )
        };
        status_result(status, "failed to read coordinate metadata")?;
        let (ptr, len) = raw_meta.parts();
        copy_coordinate_meta(ptr, len)
    }

    /// Reads current coordinate metadata from the open handle.
    pub fn coordinate_metadata(&self) -> Result<Vec<AxisCoordinateMeta>> {
        self.coordinate_meta_v2()
    }

    /// Reads Coordinate v2 metadata from the open handle.
    pub fn coordinate_meta_v2(&self) -> Result<Vec<AxisCoordinateMetaV2>> {
        let mut raw_meta = NativeArrayOutput::new(sys::arcadia_tio_axis_coordinate_meta_v2_free);
        // SAFETY: Out pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_coordinate_meta_v2(
                self.raw.as_ptr(),
                raw_meta.ptr_out(),
                raw_meta.len_out(),
            )
        };
        status_result(status, "failed to read Coordinate v2 metadata")?;
        let (ptr, len) = raw_meta.parts();
        copy_coordinate_meta_v2(ptr, len)
    }

    /// Analyzes how a sparse-intent f32 append would be handled by the native writer.
    pub fn analyze_sparse_append_f32(
        &self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f32(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent f64 append would be handled by the native writer.
    pub fn analyze_sparse_append_f64(
        &self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_f64(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i32 append would be handled by the native writer.
    pub fn analyze_sparse_append_i32(
        &self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append_v2(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i32_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Analyzes how a sparse-intent i64 append would be handled by the native writer.
    pub fn analyze_sparse_append_i64(
        &self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<SparseAppendAnalysis> {
        self.analyze_sparse_append_v2(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, raw| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // buffers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_analyze_sparse_append_i64_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        raw,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f32(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F32, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f32(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f32_with_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f32_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f32 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f32_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f32`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f32_returning_range(
        &mut self,
        data: &[f32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f32_with_range(data, shape, rule)
    }

    /// Appends f64 data using sparse-intent analysis without returning the assigned range.
    pub fn append_sparse_f64(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.append_sparse(DType::F64, data.len(), shape, rule, |handle, raw_rule| {
            // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, and rule buffers are
            // borrowed from Rust values that outlive this FFI call.
            unsafe {
                sys::arcadia_tio_append_sparse_f64(
                    handle,
                    data.as_ptr(),
                    shape.as_ptr(),
                    shape.len(),
                    raw_rule,
                )
            }
        })
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_f64_with_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range(
            DType::F64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_f64_with_range(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends f64 data using sparse-intent analysis and returns the assigned entry range.
    ///
    /// This is a readability alias for [`TensorFile::append_sparse_f64_with_range`].
    /// The unsuffixed [`TensorFile::append_sparse_f64`] method is kept as a
    /// compatibility-preserving status-only append.
    pub fn append_sparse_f64_returning_range(
        &mut self,
        data: &[f64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_f64_with_range(data, shape, rule)
    }

    /// Appends i32 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_i32(
        &mut self,
        data: &[i32],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range_v2(
            DType::I32,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i32_with_range_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends i64 data using sparse-intent analysis and returns the assigned entry range.
    pub fn append_sparse_i64(
        &mut self,
        data: &[i64],
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<AppendRange> {
        self.append_sparse_with_range_v2(
            DType::I64,
            data.len(),
            shape,
            rule,
            |handle, raw_rule, start, end| {
                // SAFETY: The wrapper validates dtype/shape/rule. Data, shape, rule, and output
                // pointers are borrowed from Rust values that outlive this FFI call.
                unsafe {
                    sys::arcadia_tio_append_sparse_i64_with_range_v2(
                        handle,
                        data.as_ptr(),
                        shape.as_ptr(),
                        shape.len(),
                        raw_rule,
                        start,
                        end,
                    )
                }
            },
        )
    }

    /// Appends a bulk f32 slice and returns the assigned append-entry range.
    pub fn append_f32(&mut self, data: &[f32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice and returns the assigned append-entry range.
    pub fn append_f64(&mut self, data: &[f64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice and returns the assigned append-entry range.
    pub fn append_i32(&mut self, data: &[i32], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice and returns the assigned append-entry range.
    pub fn append_i64(&mut self, data: &[i64], shape: &[u64]) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_range(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f32 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_f32_with_coordinates(
        &mut self,
        data: &[f32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_f32_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk f32 slice with Coordinate v2 append-axis values and returns the assigned range.
    ///
    /// Coordinate semantic validation (missing required values, wrong counts, descriptor/domain
    /// mismatches, dictionary/fixed-text conflicts, and publication conflicts) is delegated to the
    /// raw Coordinate v2 append call so native last-error details are preserved. The wrapper prepares
    /// borrowed coordinate buffers only for this synchronous call and never falls back to a payload-only
    /// append, preserving raw no-partial-publication semantics on failure.
    pub fn append_f32_with_coordinates_v2(
        &mut self,
        data: &[f32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_f64_with_coordinates(
        &mut self,
        data: &[f64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_f64_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk f64 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_f64_with_coordinates_v2(
        &mut self,
        data: &[f64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_i32_with_coordinates(
        &mut self,
        data: &[i32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_i32_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk i32 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_i32_with_coordinates_v2(
        &mut self,
        data: &[i32],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice with coordinate append-axis values and returns the assigned range.
    pub fn append_i64_with_coordinates(
        &mut self,
        data: &[i64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatch,
    ) -> Result<AppendRange> {
        self.append_i64_with_coordinates_v2(data, shape, coordinates)
    }

    /// Appends a bulk i64 slice with Coordinate v2 append-axis values and returns the assigned range.
    pub fn append_i64_with_coordinates_v2(
        &mut self,
        data: &[i64],
        shape: &[u64],
        coordinates: &AppendCoordinateBatchV2,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        let prepared = coordinates.prepare()?;
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_coordinates_v2(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                prepared.raw(),
                start,
                end,
            )
        })
    }

    /// Appends a bulk f32 slice with universe bindings and returns the assigned entry range.
    pub fn append_f32_with_universe(
        &mut self,
        data: &[f32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk f64 slice with universe bindings and returns the assigned entry range.
    pub fn append_f64_with_universe(
        &mut self,
        data: &[f64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::F64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_f64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i32 slice with universe bindings and returns the assigned entry range.
    pub fn append_i32_with_universe(
        &mut self,
        data: &[i32],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I32, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i32_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Appends a bulk i64 slice with universe bindings and returns the assigned entry range.
    pub fn append_i64_with_universe(
        &mut self,
        data: &[i64],
        shape: &[u64],
        options: &AppendWithUniverseOptions,
    ) -> Result<AppendRange> {
        self.validate_append(DType::I64, data.len(), shape)?;
        let prepared = PreparedAppendUniverseOptions::new(options);
        let raw_options = prepared.raw_options();
        self.append_with_range(shape, |handle, start, end| unsafe {
            sys::arcadia_tio_append_i64_with_universe(
                handle,
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
                &raw_options,
                start,
                end,
            )
        })
    }

    /// Rewrites a single native entry selector with f32 payload data.
    pub fn rewrite_f32(
        &mut self,
        selector: EntrySelector,
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f32(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 data")
    }

    /// Rewrites a single native entry selector with f64 payload data.
    pub fn rewrite_f64(
        &mut self,
        selector: EntrySelector,
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite")?;
        let prepared_selector = PreparedSingleSelector::new(&selector)?;
        // SAFETY: Prepared selector and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_f64(
                self.raw.as_ptr(),
                prepared_selector.ptr(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 data")
    }

    /// Rewrites a selector slice with f32 payload data.
    pub fn rewrite_slice_f32(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f32],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F32, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f32(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f32 selector slice")
    }

    /// Rewrites a selector slice with f64 payload data.
    pub fn rewrite_slice_f64(
        &mut self,
        selectors: &[EntrySelector],
        data: &[f64],
        shape: &[u64],
    ) -> Result<()> {
        self.validate_mutation_payload(DType::F64, data.len(), shape, "rewrite slice")?;
        let rank = self.rank()?;
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let prepared_selectors = PreparedSelectors::new(selectors, rank)?;
        // SAFETY: Prepared selector buffers and borrowed data/shape slices outlive the FFI call.
        let status = unsafe {
            sys::arcadia_tio_rewrite_slice_f64(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                data.as_ptr(),
                shape.as_ptr(),
                shape.len(),
            )
        };
        status_result(status, "failed to rewrite f64 selector slice")
    }

    /// Clears storage blocks identified by chunk keys.
    pub fn clear_blocks(&mut self, keys: &[ChunkKey]) -> Result<()> {
        let prepared_keys = PreparedChunkKeys::new(keys);
        // SAFETY: Prepared chunk-key buffers and their borrowed coordinate slices outlive the call.
        let status = unsafe {
            sys::arcadia_tio_clear_blocks(
                self.raw.as_ptr(),
                prepared_keys.ptr(),
                prepared_keys.len(),
            )
        };
        status_result(status, "failed to clear blocks")
    }

    /// Returns metadata for the current visible head commit.
    pub fn head_commit(&self) -> Result<CommitInfo> {
        let mut raw = MaybeUninit::<sys::ArcadiaTioCommitInfo>::uninit();
        // SAFETY: `raw` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_head_commit(self.raw.as_ptr(), raw.as_mut_ptr()) };
        status_result(status, "failed to read head commit")?;
        // SAFETY: Successful native call initialized the output commit.
        Ok(unsafe { raw.assume_init() }.into())
    }

    /// Lists visible commits in native order.
    ///
    /// A `limit` of `None` requests the native full visible list; `Some(0)` is rejected because the
    /// underlying C ABI uses zero as the unbounded sentinel.
    pub fn list_commits(&self, limit: Option<u32>) -> Result<Vec<CommitInfo>> {
        let raw_limit = match limit {
            Some(0) => {
                return Err(TioError::invalid_argument(
                    "commit list limit must be non-zero; use None for the full list",
                ));
            }
            Some(value) => value,
            None => 0,
        };
        let mut raw_list = NativeCommitList::new();
        // SAFETY: `raw_list` is a valid output pointer and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_list_commits(self.raw.as_ptr(), raw_limit, raw_list.as_mut_ptr())
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            return Err(TioError::from_last_error("failed to list commits"));
        }
        copy_commit_list(raw_list.as_ref())
    }

    /// Removes the current visible head commit.
    ///
    /// This mutates the open file in place and delegates all retention/underflow validation to the
    /// native history implementation.
    pub fn pop(&mut self) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop(self.raw.as_ptr()) };
        status_result(status, "failed to pop head commit")
    }

    /// Removes up to `n` visible head commits.
    ///
    /// This mutates the open file in place. Passing `0` is rejected by the safe wrapper because it
    /// cannot change history and is usually a caller bug.
    pub fn pop_batched(&mut self, n: u32) -> Result<()> {
        if n == 0 {
            return Err(TioError::invalid_argument(
                "pop_batched count must be non-zero",
            ));
        }
        // SAFETY: `self.raw` is a live native handle.
        let status = unsafe { sys::arcadia_tio_pop_batched(self.raw.as_ptr(), n) };
        status_result(status, "failed to pop batched commits")
    }

    /// Reverts the file to a visible target commit sequence.
    ///
    /// This mutates the open file in place and preserves native semantics for invalid or retained
    /// history targets.
    pub fn revert_commit(&mut self, target_commit_seq: u64) -> Result<()> {
        // SAFETY: `self.raw` is a live native handle.
        let status =
            unsafe { sys::arcadia_tio_revert_commit(self.raw.as_ptr(), target_commit_seq) };
        status_result(status, "failed to revert commit")
    }

    /// Returns shallow compatibility compaction statistics.
    pub fn analyze_compaction(&self) -> Result<CompactionStats> {
        let mut stats = sys::ArcadiaTioCompactionStats {
            live_bytes: 0,
            dead_bytes: 0,
            dead_ratio: 0.0,
            commit_count: 0,
        };
        // SAFETY: `stats` is a valid output pointer and the handle is live.
        let status = unsafe { sys::arcadia_tio_analyze_compaction(self.raw.as_ptr(), &mut stats) };
        status_result(status, "failed to analyze compaction")?;
        Ok(CompactionStats {
            live_bytes: stats.live_bytes,
            dead_bytes: stats.dead_bytes,
            dead_ratio: stats.dead_ratio,
            commit_count: stats.commit_count,
        })
    }

    /// Returns non-precise V4 source-file diagnostics.
    pub fn v4_diagnostics(&self) -> Result<V4DiagnosticsReport> {
        let mut report = new_v4_diagnostics_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status = unsafe { sys::arcadia_tio_v4_diagnostics(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to get V4 diagnostics"));
        }
        let copied = copy_v4_diagnostics_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_report_free(&mut report) };
        copied
    }

    /// Returns precise V4 source-file diagnostics with validity metadata.
    pub fn v4_diagnostics_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4DiagnosticsPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_diagnostics_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_v4_diagnostics_precise(self.raw.as_ptr(), &raw_options, &mut report)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to get precise V4 diagnostics",
            ));
        }
        let copied = copy_v4_diagnostics_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_diagnostics_precise_report_free(&mut report) };
        copied
    }

    /// Returns non-precise V4 current-state compaction analysis.
    pub fn analyze_v4_compaction(&self) -> Result<V4CompactionAnalysisReport> {
        let mut report = new_v4_compaction_analysis_report();
        // SAFETY: `report` is initialized for native output and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_analyze_v4_compaction(self.raw.as_ptr(), &mut report) };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
            return Err(TioError::from_last_error("failed to analyze V4 compaction"));
        }
        let copied = copy_v4_compaction_analysis_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_report_free(&mut report) };
        copied
    }

    /// Returns precise V4 current-state compaction analysis with validity metadata.
    pub fn analyze_v4_compaction_precise(
        &self,
        options: V4PreciseAccountingOptions,
    ) -> Result<V4CompactionAnalysisPreciseReport> {
        let raw_options = options.to_raw();
        let mut report = new_v4_compaction_analysis_precise_report();
        // SAFETY: Options, output report, and handle are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_analyze_v4_compaction_precise(
                self.raw.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to analyze precise V4 compaction",
            ));
        }
        let copied = copy_v4_compaction_analysis_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_compaction_analysis_precise_report_free(&mut report) };
        copied
    }

    /// Compacts live chunks into a destination file.
    pub fn compact_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        // SAFETY: Destination path C string and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.retain_commits,
                options.mode.to_raw(),
            )
        };
        status_result(status, "failed to compact TensorFile")
    }

    /// Conditionally compacts live chunks into a destination file.
    pub fn maybe_compact(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: CompactionOptions,
    ) -> Result<bool> {
        let dst_path = path_to_cstring(dst_path)?;
        let mut compacted = 0u8;
        // SAFETY: Destination path C string, output flag, and handle are live for this call.
        let status = unsafe {
            sys::arcadia_tio_maybe_compact(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                options.dead_ratio_threshold,
                options.min_dead_bytes,
                options.retain_commits,
                options.mode.to_raw(),
                &mut compacted,
            )
        };
        status_result(status, "failed to maybe compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        self.get_auto_compaction_config()
    }

    /// Reads auto-compaction metadata configuration, if present.
    pub fn get_auto_compaction_config(&self) -> Result<Option<AutoCompactionConfig>> {
        let mut config = new_auto_compaction_config();
        let mut has_config = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_get_auto_compaction_config(
                self.raw.as_ptr(),
                &mut config,
                &mut has_config,
            )
        };
        status_result(status, "failed to get auto-compaction config")?;
        if has_config == 0 {
            Ok(None)
        } else {
            copy_auto_compaction_config(config).map(Some)
        }
    }

    /// Updates or clears auto-compaction metadata configuration.
    pub fn set_auto_compaction_config(
        &mut self,
        config: Option<AutoCompactionConfig>,
    ) -> Result<()> {
        let raw = config.map(|cfg| cfg.to_raw());
        let (ptr, has_config) = match raw.as_ref() {
            Some(cfg) => (cfg as *const sys::ArcadiaTioAutoCompactionConfig, 1u8),
            None => (ptr::null(), 0u8),
        };
        // SAFETY: Optional config pointer is either null or points to a local value valid for this call.
        let status = unsafe {
            sys::arcadia_tio_set_auto_compaction_config(self.raw.as_ptr(), ptr, has_config)
        };
        status_result(status, "failed to set auto-compaction config")
    }

    /// Clears auto-compaction metadata configuration.
    pub fn clear_auto_compaction(&mut self) -> Result<()> {
        self.set_auto_compaction_config(None)
    }

    /// Reads auto-compaction state metadata, if present.
    pub fn compaction_state(&self) -> Result<Option<CompactionState>> {
        let mut state = sys::ArcadiaTioCompactionState {
            last_compacted_commit_seq: 0,
            last_compacted_at_unix_ms: 0,
        };
        let mut has_state = 0u8;
        // SAFETY: Output pointers are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_compaction_state(self.raw.as_ptr(), &mut state, &mut has_state)
        };
        status_result(status, "failed to read compaction state")?;
        if has_state == 0 {
            Ok(None)
        } else {
            Ok(Some(CompactionState {
                last_compacted_commit_seq: state.last_compacted_commit_seq,
                last_compacted_at_unix_ms: state.last_compacted_at_unix_ms,
            }))
        }
    }

    /// Runs metadata-configured auto-compaction if native thresholds trigger.
    pub fn maybe_compact_auto(&mut self) -> Result<bool> {
        let mut compacted = 0u8;
        // SAFETY: Output flag is valid and the handle is live.
        let status =
            unsafe { sys::arcadia_tio_maybe_compact_auto(self.raw.as_ptr(), &mut compacted) };
        status_result(status, "failed to maybe auto-compact TensorFile")?;
        Ok(compacted != 0)
    }

    /// Compacts a V4 file into a retained-history destination.
    pub fn compact_v4_retained_history_to(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: V4RetainedHistoryCompactionOptions,
    ) -> Result<V4RetainedHistoryCompactionReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_v4_retained_history_compaction_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history",
            ));
        }
        let copied = copy_v4_retained_history_compaction_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_report_free(&mut report) };
        copied
    }

    /// Compacts a V4 file into a retained-history destination with precise source accounting.
    pub fn compact_v4_retained_history_to_precise(
        &mut self,
        dst_path: impl AsRef<Path>,
        retention_options: V4RetainedHistoryCompactionOptions,
        precise_options: V4PreciseAccountingOptions,
    ) -> Result<V4RetainedHistoryCompactionPreciseReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_retention_options = retention_options.to_raw();
        let raw_precise_options = precise_options.to_raw();
        let mut report = new_v4_retained_history_compaction_precise_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_compact_v4_retained_history_to_precise(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_retention_options,
                &raw_precise_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report)
            };
            return Err(TioError::from_last_error(
                "failed to compact V4 retained history with precise accounting",
            ));
        }
        let copied = copy_v4_retained_history_compaction_precise_report(&report);
        // SAFETY: Native-owned strings/arrays in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_v4_retained_history_compaction_precise_report_free(&mut report) };
        copied
    }

    /// Reforms visible data into a destination file with an explicit target layout.
    pub fn reform_to(&mut self, dst_path: impl AsRef<Path>, options: ReformOptions) -> Result<()> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        // SAFETY: Inputs are valid for the duration of the FFI call.
        let status = unsafe {
            sys::arcadia_tio_reform_to(self.raw.as_ptr(), dst_path.as_ptr(), &raw_options)
        };
        status_result(status, "failed to reform TensorFile")
    }

    /// Reforms visible data into a destination file and returns native diagnostic metadata.
    pub fn reform_to_ex(
        &mut self,
        dst_path: impl AsRef<Path>,
        options: ReformOptions,
    ) -> Result<ReformReport> {
        let dst_path = path_to_cstring(dst_path)?;
        let raw_options = options.to_raw();
        let mut report = new_reform_report();
        // SAFETY: Inputs and initialized output report are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_reform_to_ex(
                self.raw.as_ptr(),
                dst_path.as_ptr(),
                &raw_options,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            let error = TioError::from_last_error("failed to reform TensorFile with report");
            let copied = copy_reform_report(&report);
            // SAFETY: Report was initialized by this wrapper and may be partially populated.
            unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
            return Err(match copied {
                Ok(report) => error.with_reform_report(&report),
                Err(_) => error,
            });
        }
        let copied = copy_reform_report(&report);
        // SAFETY: Native-owned strings in `report` are freed exactly once after copying.
        unsafe { sys::arcadia_tio_reform_report_free(&mut report) };
        copied
    }

    /// Reads the full tensor into Rust-owned buffers.
    pub fn read_all(&self) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe { sys::arcadia_tio_read_all(handle, out) })
    }

    /// Exports full tensor values through the Arrow C Data Interface.
    ///
    /// The returned [`ArrowCData`] owns the Arrow `release` callbacks and invokes them on drop.
    /// Borrowed C Data pointers are valid only while the returned value is alive.
    pub fn read_values_arrow(&self) -> Result<ArrowCData> {
        // SAFETY: All-zero Arrow C Data carriers represent empty caller-owned output slots with
        // null release callbacks before the native function writes initialized values.
        let mut raw_array: sys::ArrowArray = unsafe { mem::zeroed() };
        // SAFETY: See `raw_array` initialization above.
        let mut raw_schema: sys::ArrowSchema = unsafe { mem::zeroed() };
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_values_arrow(self.raw.as_ptr(), &mut raw_array, &mut raw_schema)
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Defensive cleanup for any partially initialized Arrow carriers.
            unsafe {
                release_arrow_array(&mut raw_array);
                release_arrow_schema(&mut raw_schema);
            }
            return Err(TioError::from_last_error(
                "failed to export tensor values as Arrow C Data",
            ));
        }
        Ok(ArrowCData {
            array: raw_array,
            schema: raw_schema,
            _not_send_or_sync: PhantomData,
        })
    }

    /// Reads the full tensor densely with a fill value and optional validity mask.
    pub fn read_all_dense(&self, fill_value: f64) -> Result<DenseTensor> {
        let mut raw_tensor = NativeOutput::new(
            sys::ArcadiaTioTensor::default(),
            sys::arcadia_tio_tensor_free,
        );
        let mut raw_mask =
            NativeOutput::new(sys::ArcadiaTioMask::default(), sys::arcadia_tio_mask_free);
        // SAFETY: Output structs are valid and the handle is live.
        let status = unsafe {
            sys::arcadia_tio_read_all_dense(
                self.raw.as_ptr(),
                fill_value,
                raw_tensor.as_mut_ptr(),
                raw_mask.as_mut_ptr(),
            )
        };
        status_result(status, "failed to read dense tensor")?;
        let tensor = copy_tensor(raw_tensor.as_ref());
        let mask = copy_mask(raw_mask.as_ref());
        Ok(DenseTensor {
            tensor: tensor?,
            mask: mask?,
        })
    }

    /// Reads current data through the native basic read-index lowering API.
    pub fn read_index(&self, items: &[ReadIndexItem]) -> Result<ReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_report = new_read_index_report();
        // SAFETY: Prepared read-index items outlive the call; outputs are initialized and valid.
        let status = unsafe {
            sys::arcadia_tio_read_index(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                &mut raw_tensor,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error("failed to read with read_index"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexResult {
            value: tensor?,
            report: report?,
        })
    }

    /// Reads current data through the native basic read-index API with dense fill materialization.
    pub fn read_index_dense(
        &self,
        items: &[ReadIndexItem],
        fill_value: f64,
    ) -> Result<ReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut raw_report = new_read_index_report();
        // SAFETY: Prepared read-index items outlive the call; outputs are initialized and valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_dense(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            report: report?,
        })
    }

    /// Reads current data through the native basic read-index API with a shape-policy domain.
    pub fn read_index_with_shape_policy(
        &self,
        items: &[ReadIndexItem],
        options: ReadWithShapePolicyOptions,
    ) -> Result<ReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_report = new_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_with_shape_policy(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                &mut raw_tensor,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error(
                "failed to read with read_index shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexResult {
            value: tensor?,
            report: report?,
        })
    }

    /// Reads current data through the native basic read-index API with a shape-policy domain and dense fill materialization.
    pub fn read_index_with_shape_policy_dense(
        &self,
        items: &[ReadIndexItem],
        options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<ReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut raw_report = new_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_with_shape_policy_dense(
                self.raw.as_ptr(),
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut raw_report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_index_report_free(&mut raw_report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with read_index shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let report = copy_read_index_report(&raw_report);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_index_report_free(&mut raw_report);
        }
        Ok(ReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            report: report?,
        })
    }

    /// Reads an axis range into Rust-owned buffers.
    pub fn read_axis_range(&self, axis: usize, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "axis range start must be <= end",
            ));
        }
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_range(handle, axis, start, end, out)
        })
    }

    /// Reads an axis take selection into Rust-owned buffers.
    pub fn read_axis_take(&self, axis: usize, indices: &[u32]) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_take(handle, axis, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads an append-entry range into Rust-owned buffers.
    pub fn read_entry_range(&self, start: u32, end: u32) -> Result<Tensor> {
        if start > end {
            return Err(TioError::invalid_argument(
                "entry range start must be <= end",
            ));
        }
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_entry_range(handle, start, end, out)
        })
    }

    /// Reads selected append entries into Rust-owned buffers.
    pub fn take_entries(&self, indices: &[u32]) -> Result<Tensor> {
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_take_entries(handle, indices.as_ptr(), indices.len(), out)
        })
    }

    /// Reads inline coordinate values for an axis into Rust-owned buffers.
    ///
    /// This is metadata-scope coordinate value access, not native exact/range coordinate lookup.
    pub fn read_axis_coordinates(&self, axis: usize) -> Result<Tensor> {
        self.validate_axis(axis)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_axis_coordinates(handle, axis, out)
        })
    }

    /// Reads current coordinate axis values into Rust-owned bytes while preserving status fields.
    pub fn read_coordinate_axis(
        &self,
        axis: usize,
        options: CoordinateOptions,
    ) -> Result<CoordinateValueSlice> {
        self.read_axis_coordinates_v2(axis, options)
    }

    /// Reads Coordinate v2 axis values into Rust-owned bytes while preserving status fields.
    pub fn read_axis_coordinates_v2(
        &self,
        axis: usize,
        options: CoordinateV2Options,
    ) -> Result<CoordinateValueSliceV2> {
        self.validate_axis(axis)?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateValueSliceV2::default();
        // SAFETY: `self.raw` is live, `raw_options` and `raw` are valid for the duration of the call.
        let status = unsafe {
            sys::arcadia_tio_read_axis_coordinates_v2(
                self.raw.as_ptr(),
                axis,
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to read Coordinate v2 axis values") {
            // SAFETY: The raw value carrier is either empty/default or native-owned partial output;
            // the paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_value_slice_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies data before free.
        let out = unsafe { CoordinateValueSliceV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_value_slice_v2_free(&mut raw) };
        out
    }

    /// Reads current coordinate dictionary metadata/entries into Rust-owned values.
    pub fn coordinate_dictionary(
        &self,
        axis: usize,
        options: CoordinateOptions,
    ) -> Result<CoordinateDictionary> {
        self.coordinate_dictionary_v2(axis, options)
    }

    /// Reads Coordinate v2 dictionary metadata/entries into Rust-owned values.
    pub fn coordinate_dictionary_v2(
        &self,
        axis: usize,
        options: CoordinateV2Options,
    ) -> Result<CoordinateDictionaryV2> {
        self.validate_axis(axis)?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateDictionaryV2::default();
        // SAFETY: `self.raw` is live, `raw_options` and `raw` are valid for the duration of the call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_dictionary_v2(
                self.raw.as_ptr(),
                axis,
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to read Coordinate v2 dictionary") {
            // SAFETY: The raw dictionary is either empty/default or native-owned partial output;
            // the paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_dictionary_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies data before free.
        let out = unsafe { CoordinateDictionaryV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_dictionary_v2_free(&mut raw) };
        out
    }

    /// Performs an exact coordinate lookup using a typed key.
    pub fn coordinate_lookup(
        &self,
        axis: usize,
        key: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_v2(axis, key, options)
    }

    /// Performs an exact Coordinate v2 lookup using a typed key.
    ///
    /// Transport/API misuse is returned as `Err(TioError)`. Ordinary Coordinate v2 outcomes such
    /// as missing, unavailable, duplicate, unsupported, invalid/stale index, or domain mismatch are
    /// preserved in the returned [`CoordinateLookupResultV2`]. Optional indexes are acceleration
    /// metadata only; pass [`CoordinateV2Options::authoritative_scan`] when callers explicitly allow
    /// fallback to authoritative selected-root coordinate values.
    pub fn coordinate_lookup_v2(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_key = key.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. `prepared_key`, `raw_options`, and `raw` remain valid for
        // the duration of the call and the raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_v2(
                self.raw.as_ptr(),
                axis,
                prepared_key.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to perform Coordinate v2 exact lookup") {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs an exact coordinate lookup against a retained historical commit.
    pub fn coordinate_lookup_at_commit(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, options)
    }

    /// Performs an exact Coordinate v2 lookup against a retained historical commit.
    ///
    /// The lookup binds Coordinate v2 values, dictionaries, and append-coordinate chunks to the
    /// target commit before evaluating the key. Result status semantics match
    /// [`Self::coordinate_lookup_v2`].
    pub fn coordinate_lookup_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_key = key.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. `prepared_key`, `raw_options`, and `raw` remain valid for
        // the duration of the call and the raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_at_commit_v2(
                self.raw.as_ptr(),
                commit_seq,
                axis,
                prepared_key.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(
            status,
            "failed to perform historical Coordinate v2 exact lookup",
        ) {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs a half-open coordinate range lookup using typed lower/upper keys.
    pub fn coordinate_lookup_range(
        &self,
        axis: usize,
        lower: &CoordinateLookupKey,
        upper: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_range_v2(axis, lower, upper, options)
    }

    /// Performs a half-open Coordinate v2 range lookup using typed lower/upper keys.
    ///
    /// Status-rich raw lookup outcomes are returned as [`CoordinateLookupResultV2`] instead of
    /// being collapsed into opaque errors. Optional indexes remain non-authoritative; callers must
    /// opt into authoritative scans through [`CoordinateV2Options`].
    pub fn coordinate_lookup_range_v2(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_lower = lower.prepare()?;
        let prepared_upper = upper.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. Prepared keys/options/output outlive the FFI call and the
        // raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_range_v2(
                self.raw.as_ptr(),
                axis,
                prepared_lower.raw(),
                prepared_upper.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(status, "failed to perform Coordinate v2 range lookup") {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs a half-open coordinate range lookup against a retained historical commit.
    pub fn coordinate_lookup_range_at_commit(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKey,
        upper: &CoordinateLookupKey,
        options: CoordinateOptions,
    ) -> Result<CoordinateLookupResult> {
        self.coordinate_lookup_range_at_commit_v2(commit_seq, axis, lower, upper, options)
    }

    /// Performs a half-open Coordinate v2 range lookup against a retained historical commit.
    ///
    /// The target commit is bound before Coordinate v2 lower-inclusive, upper-exclusive range
    /// semantics are evaluated. Result status semantics match
    /// [`Self::coordinate_lookup_range_v2`].
    pub fn coordinate_lookup_range_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        options: CoordinateV2Options,
    ) -> Result<CoordinateLookupResultV2> {
        self.validate_axis(axis)?;
        let prepared_lower = lower.prepare()?;
        let prepared_upper = upper.prepare()?;
        let raw_options = options.to_raw();
        let mut raw = sys::ArcadiaTioCoordinateLookupResultV2::default();
        // SAFETY: `self.raw` is live. Prepared keys/options/output outlive the FFI call and the
        // raw result is copied before being freed.
        let status = unsafe {
            sys::arcadia_tio_coordinate_lookup_range_at_commit_v2(
                self.raw.as_ptr(),
                commit_seq,
                axis,
                prepared_lower.raw(),
                prepared_upper.raw(),
                &raw_options,
                &mut raw,
            )
        };
        if let Err(err) = status_result(
            status,
            "failed to perform historical Coordinate v2 range lookup",
        ) {
            // SAFETY: The raw result is either default/empty or native-owned partial output; the
            // paired free function tolerates empty carriers and is called at most once here.
            unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
            return Err(err);
        }
        // SAFETY: Successful status initializes `raw`; from_raw_borrowed copies positions/reason.
        let out = unsafe { CoordinateLookupResultV2::from_raw_borrowed(&raw) };
        // SAFETY: `raw` is native-owned output and is freed exactly once after copying.
        unsafe { sys::arcadia_tio_coordinate_lookup_result_v2_free(&mut raw) };
        out
    }

    /// Performs current-head Coordinate v2 exact lookup and reads the matching axis slice.
    ///
    /// `Unique` lookup results read the half-open payload range `[position, position + 1)` at the
    /// current selected head. Ordinary Coordinate v2 non-answers such as missing, unavailable,
    /// duplicate, unsupported, many, or error statuses are preserved in `lookup` and return no
    /// payload read.
    pub fn read_at_coordinate_v2(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithOptions,
    ) -> Result<CoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_v2(axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_with_options(&selectors, read_options)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 range lookup and reads the matching axis range.
    ///
    /// `Range` lookup results read the returned half-open payload range at the current selected
    /// head. Zero-length ranges are passed through to the current read path unchanged. Ordinary
    /// Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_coordinate_range_v2(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithOptions,
    ) -> Result<CoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_range_v2(axis, lower, upper, coordinate_options)?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_with_options(&selectors, read_options)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 exact lookup and densely reads the matching axis slice.
    ///
    /// `Unique` lookup results read the half-open payload range `[position, position + 1)` at the
    /// current selected head, materializing nulls with `fill_value` and returning a validity mask.
    /// Ordinary Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_at_coordinate_v2_dense(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithOptions,
        fill_value: f64,
    ) -> Result<CoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_v2(axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_with_options_dense(&selectors, read_options, fill_value)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 range lookup and densely reads the matching axis range.
    ///
    /// `Range` lookup results read the returned half-open payload range at the current selected
    /// head, materializing nulls with `fill_value` and returning a validity mask. Ordinary
    /// Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_coordinate_range_v2_dense(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithOptions,
        fill_value: f64,
    ) -> Result<CoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_range_v2(axis, lower, upper, coordinate_options)?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_with_options_dense(&selectors, read_options, fill_value)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 exact lookup and reads the matching axis slice with shape policy.
    ///
    /// `Unique` lookup results read the half-open payload range `[position, position + 1)` at the
    /// current selected head with the requested shape policy. Ordinary Coordinate v2 non-answers
    /// are preserved in `lookup` and return no payload read.
    pub fn read_at_coordinate_v2_with_shape_policy(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithShapePolicyOptions,
    ) -> Result<CoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_v2(axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_with_shape_policy(&selectors, read_options)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 range lookup and reads the matching axis range with shape policy.
    ///
    /// `Range` lookup results read the returned half-open payload range at the current selected
    /// head with the requested shape policy. Ordinary Coordinate v2 non-answers are preserved in
    /// `lookup` and return no payload read.
    pub fn read_coordinate_range_v2_with_shape_policy(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithShapePolicyOptions,
    ) -> Result<CoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_range_v2(axis, lower, upper, coordinate_options)?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_with_shape_policy(&selectors, read_options)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 exact lookup and densely reads the matching axis slice with shape policy.
    ///
    /// `Unique` lookup results read the half-open payload range `[position, position + 1)` at the
    /// current selected head with the requested shape policy, materializing nulls with
    /// `fill_value` and returning a validity mask. Ordinary Coordinate v2 non-answers are
    /// preserved in `lookup` and return no payload read.
    pub fn read_at_coordinate_v2_with_shape_policy_dense(
        &self,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<CoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_v2(axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_with_shape_policy_dense(&selectors, read_options, fill_value)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs current-head Coordinate v2 range lookup and densely reads the matching axis range with shape policy.
    ///
    /// `Range` lookup results read the returned half-open payload range at the current selected
    /// head with the requested shape policy, materializing nulls with `fill_value` and returning a
    /// validity mask. Ordinary Coordinate v2 non-answers are preserved in `lookup` and return no
    /// payload read.
    pub fn read_coordinate_range_v2_with_shape_policy_dense(
        &self,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        read_options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<CoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_range_v2(axis, lower, upper, coordinate_options)?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_with_shape_policy_dense(&selectors, read_options, fill_value)?)
        } else {
            None
        };
        Ok(CoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 exact lookup and reads the matching axis slice.
    ///
    /// `Unique` lookup results read the half-open payload range `[position, position + 1)` at the
    /// same retained commit. Ordinary Coordinate v2 non-answers such as missing, unavailable,
    /// duplicate, unsupported, many, or error statuses are preserved in `lookup` and return no
    /// payload read.
    pub fn read_at_coordinate_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithOptions,
    ) -> Result<HistoricalCoordinateReadResult<Tensor>> {
        let lookup =
            self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_at_commit_with_options(commit_seq, &selectors, historical_options)?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 range lookup and reads the matching axis range.
    ///
    /// `Range` lookup results read the returned half-open payload range at the same retained
    /// commit. Zero-length ranges are passed through to the historical read path unchanged.
    /// Ordinary Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_coordinate_range_at_commit_v2(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithOptions,
    ) -> Result<HistoricalCoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_range_at_commit_v2(
            commit_seq,
            axis,
            lower,
            upper,
            coordinate_options,
        )?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_at_commit_with_options(commit_seq, &selectors, historical_options)?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 exact lookup and densely reads the matching axis slice.
    ///
    /// `Unique` lookup results read `[position, position + 1)` at the same retained commit,
    /// materializing nulls with `fill_value` and returning a validity mask. Ordinary Coordinate v2
    /// non-answers are preserved in `lookup` and return no payload read.
    pub fn read_at_coordinate_at_commit_v2_dense(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalCoordinateReadResult<DenseTensor>> {
        let lookup =
            self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_at_commit_with_options_dense(
                commit_seq,
                &selectors,
                historical_options,
                fill_value,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 range lookup and densely reads the matching axis range.
    ///
    /// `Range` lookup results read the returned half-open payload range at the same retained
    /// commit, materializing nulls with `fill_value` and returning a validity mask. Ordinary
    /// Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_coordinate_range_at_commit_v2_dense(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalCoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_range_at_commit_v2(
            commit_seq,
            axis,
            lower,
            upper,
            coordinate_options,
        )?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_at_commit_with_options_dense(
                commit_seq,
                &selectors,
                historical_options,
                fill_value,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 exact lookup and reads the matching axis slice with shape policy.
    ///
    /// `Unique` lookup results read `[position, position + 1)` at the same retained commit with the
    /// requested shape policy. Ordinary Coordinate v2 non-answers are preserved in `lookup` and
    /// return no payload read.
    pub fn read_at_coordinate_at_commit_v2_with_shape_policy(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalCoordinateReadResult<Tensor>> {
        let lookup =
            self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_at_commit_with_shape_policy(
                commit_seq,
                &selectors,
                historical_options,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 range lookup and reads the matching axis range with shape policy.
    ///
    /// `Range` lookup results read the returned half-open payload range at the same retained commit
    /// with the requested shape policy. Ordinary Coordinate v2 non-answers are preserved in
    /// `lookup` and return no payload read.
    pub fn read_coordinate_range_at_commit_v2_with_shape_policy(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalCoordinateReadResult<Tensor>> {
        let lookup = self.coordinate_lookup_range_at_commit_v2(
            commit_seq,
            axis,
            lower,
            upper,
            coordinate_options,
        )?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_at_commit_with_shape_policy(
                commit_seq,
                &selectors,
                historical_options,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 exact lookup and densely reads the matching axis slice with shape policy.
    ///
    /// `Unique` lookup results read `[position, position + 1)` at the same retained commit with the
    /// requested shape policy, materializing nulls with `fill_value` and returning a validity mask.
    /// Ordinary Coordinate v2 non-answers are preserved in `lookup` and return no payload read.
    pub fn read_at_coordinate_at_commit_v2_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        axis: usize,
        key: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalCoordinateReadResult<DenseTensor>> {
        let lookup =
            self.coordinate_lookup_at_commit_v2(commit_seq, axis, key, coordinate_options)?;
        let read = if lookup.status == CoordinateLookupResultStatusV2::Unique {
            let end = lookup.unique_position.checked_add(1).ok_or_else(|| {
                TioError::invalid_argument("Coordinate v2 unique position overflowed range end")
            })?;
            let selectors =
                self.coordinate_axis_range_selectors(axis, lookup.unique_position, end)?;
            Some(self.read_at_commit_with_shape_policy_dense(
                commit_seq,
                &selectors,
                historical_options,
                fill_value,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Performs historical Coordinate v2 range lookup and densely reads the matching axis range with shape policy.
    ///
    /// `Range` lookup results read the returned half-open payload range at the same retained commit
    /// with the requested shape policy, materializing nulls with `fill_value` and returning a
    /// validity mask. Ordinary Coordinate v2 non-answers are preserved in `lookup` and return no
    /// payload read.
    pub fn read_coordinate_range_at_commit_v2_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        axis: usize,
        lower: &CoordinateLookupKeyV2,
        upper: &CoordinateLookupKeyV2,
        coordinate_options: CoordinateV2Options,
        historical_options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalCoordinateReadResult<DenseTensor>> {
        let lookup = self.coordinate_lookup_range_at_commit_v2(
            commit_seq,
            axis,
            lower,
            upper,
            coordinate_options,
        )?;
        let read = if let Some(range) = lookup.range() {
            let selectors = self.coordinate_axis_range_selectors(axis, range.start, range.end)?;
            Some(self.read_at_commit_with_shape_policy_dense(
                commit_seq,
                &selectors,
                historical_options,
                fill_value,
            )?)
        } else {
            None
        };
        Ok(HistoricalCoordinateReadResult { lookup, read })
    }

    /// Looks up the unique axis index for an inline validated i32 coordinate value.
    pub fn coordinate_index_i32(&self, axis: usize, value: i32) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i32(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i32 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the unique axis index for an inline validated i64 coordinate value.
    pub fn coordinate_index_i64(&self, axis: usize, value: i64) -> Result<u32> {
        self.validate_axis(axis)?;
        let mut out_index = 0u32;
        // SAFETY: `self.raw` is live and `out_index` is a valid output pointer for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_index_i64(self.raw.as_ptr(), axis, value, &mut out_index)
        };
        status_result(status, "failed to look up i64 coordinate index")?;
        Ok(out_index)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i32 coordinate interval.
    pub fn coordinate_range_i32(
        &self,
        axis: usize,
        start: i32,
        end: i32,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i32(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i32 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Looks up the half-open axis-index range overlapping an inclusive i64 coordinate interval.
    pub fn coordinate_range_i64(
        &self,
        axis: usize,
        start: i64,
        end: i64,
    ) -> Result<std::ops::Range<u32>> {
        self.validate_axis(axis)?;
        let mut out_start = 0u32;
        let mut out_end = 0u32;
        // SAFETY: `self.raw` is live and both output pointers are valid for this call.
        let status = unsafe {
            sys::arcadia_tio_coordinate_range_i64(
                self.raw.as_ptr(),
                axis,
                start,
                end,
                &mut out_start,
                &mut out_end,
            )
        };
        status_result(status, "failed to look up i64 coordinate range")?;
        Ok(out_start..out_end)
    }

    /// Reads the one-position axis slice for an inline validated i32 coordinate value.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_index_i32`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_at_coordinate_i32(&self, axis: usize, value: i32) -> Result<Tensor> {
        let index = self.coordinate_index_i32(axis, value)?;
        let end = index.checked_add(1).ok_or_else(|| {
            TioError::invalid_argument("coordinate index cannot be converted to a one-item range")
        })?;
        self.read_axis_range(axis, index, end)
    }

    /// Reads the one-position axis slice for an inline validated i64 coordinate value.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_index_i64`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_at_coordinate_i64(&self, axis: usize, value: i64) -> Result<Tensor> {
        let index = self.coordinate_index_i64(axis, value)?;
        let end = index.checked_add(1).ok_or_else(|| {
            TioError::invalid_argument("coordinate index cannot be converted to a one-item range")
        })?;
        self.read_axis_range(axis, index, end)
    }

    /// Reads the axis slice overlapping an inclusive i32 coordinate interval.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_range_i32`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_coordinate_range_i32(&self, axis: usize, start: i32, end: i32) -> Result<Tensor> {
        let range = self.coordinate_range_i32(axis, start, end)?;
        self.read_axis_range(axis, range.start, range.end)
    }

    /// Reads the axis slice overlapping an inclusive i64 coordinate interval.
    ///
    /// This is a convenience wrapper over [`Self::coordinate_range_i64`] plus
    /// [`Self::read_axis_range`]. It does not use a coordinate index or change native read
    /// planning semantics.
    pub fn read_coordinate_range_i64(&self, axis: usize, start: i64, end: i64) -> Result<Tensor> {
        let range = self.coordinate_range_i64(axis, start, end)?;
        self.read_axis_range(axis, range.start, range.end)
    }

    /// Reads current selector data with execution options and metadata.
    pub fn read_with_options(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error("failed to read with options"));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with execution options and metadata.
    pub fn read_with_options_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            execution: execution?,
        })
    }

    /// Reads current selector data with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by
    /// itself.
    pub fn read_with_options_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
    ) -> Result<AttributedReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                &mut raw_tensor,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: tensor?,
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data densely with execution options, metadata, and diagnostic trace JSON.
    ///
    /// This opt-in API preserves ordinary `read_with_options_dense` semantics while returning native
    /// query-attribution JSON for diagnostics. It is not benchmark or performance evidence by itself.
    pub fn read_with_options_dense_attributed(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithOptions,
        trace_context: &QueryTraceContext,
        fill_value: f64,
    ) -> Result<AttributedReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithOptions::new(&options)?;
        let prepared_context = PreparedQueryTraceContext::new(trace_context)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let mut trace_json = new_query_trace_json();
        let raw_options = prepared_options.raw_options();
        let raw_context = prepared_context.raw_context();
        // SAFETY: Prepared selector, option, and context buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_options_dense_attributed(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &raw_context,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
                &mut trace_json,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
                sys::arcadia_tio_query_trace_json_free(&mut trace_json);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with options and query attribution",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        let trace = copy_query_trace_json(&trace_json);
        // SAFETY: Native-owned outputs are freed exactly once after copying.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
            sys::arcadia_tio_query_trace_json_free(&mut trace_json);
        }
        Ok(AttributedReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            execution: execution?,
            trace: trace?,
        })
    }

    /// Reads current selector data with a shape policy and execution metadata.
    pub fn read_with_shape_policy(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
    ) -> Result<ReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads current selector data densely with a shape policy and execution metadata.
    pub fn read_with_shape_policy_dense(
        &self,
        selectors: &[EntrySelector],
        options: ReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<ReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_with_shape_policy_dense(
                self.raw.as_ptr(),
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_read_execution_report_free(&mut report);
        }
        Ok(ReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit into Rust-owned buffers.
    pub fn read_at_commit(&self, commit_seq: u64, selectors: &[EntrySelector]) -> Result<Tensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        self.read_tensor(|handle, out| unsafe {
            sys::arcadia_tio_read_at_commit(
                handle,
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                out,
            )
        })
    }

    /// Reads selector data at a retained commit densely with a fill value.
    pub fn read_at_commit_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        fill_value: f64,
    ) -> Result<DenseTensor> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let mut raw_tensor = NativeOutput::new(
            sys::ArcadiaTioTensor::default(),
            sys::arcadia_tio_tensor_free,
        );
        let mut raw_mask =
            NativeOutput::new(sys::ArcadiaTioMask::default(), sys::arcadia_tio_mask_free);
        // SAFETY: Prepared selector buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                fill_value,
                raw_tensor.as_mut_ptr(),
                raw_mask.as_mut_ptr(),
            )
        };
        status_result(status, "failed to read dense tensor at commit")?;
        let tensor = copy_tensor(raw_tensor.as_ref());
        let mask = copy_mask(raw_mask.as_ref());
        Ok(DenseTensor {
            tensor: tensor?,
            mask: mask?,
        })
    }

    /// Reads selector data at a retained commit with execution options and metadata.
    pub fn read_at_commit_with_options(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with execution options and metadata.
    pub fn read_at_commit_with_options_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_options_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with options",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            execution: execution?,
        })
    }

    /// Reads retained historical data through the native basic read-index lowering API.
    pub fn read_index_at_commit_with_options(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithOptions,
    ) -> Result<HistoricalReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_options(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexResult {
            value: tensor?,
            report: copied_report?,
        })
    }

    /// Reads retained historical data through the native basic read-index API with dense fill materialization.
    pub fn read_index_at_commit_with_options_dense(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_options_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with read_index",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            report: copied_report?,
        })
    }

    /// Reads retained historical data through the native basic read-index API with a shape-policy domain.
    pub fn read_index_at_commit_with_shape_policy(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalReadIndexResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_shape_policy(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with read_index shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexResult {
            value: tensor?,
            report: copied_report?,
        })
    }

    /// Reads retained historical data through the native basic read-index API with a shape-policy domain and dense fill materialization.
    pub fn read_index_at_commit_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        items: &[ReadIndexItem],
        options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadIndexDenseResult> {
        let prepared_items = PreparedReadIndexItems::new(items, self.rank()?)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_index_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared item and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_index_at_commit_with_shape_policy_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_items.ptr(),
                prepared_items.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_index_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with read_index shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let copied_report = copy_historical_read_index_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_index_report_free(&mut report);
        }
        Ok(HistoricalReadIndexDenseResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            report: copied_report?,
        })
    }

    /// Reads selector data at a retained commit with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
    ) -> Result<HistoricalReadResult<Tensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                &mut raw_tensor,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: tensor?,
            execution: execution?,
        })
    }

    /// Reads selector data at a retained commit densely with a shape policy and execution metadata.
    pub fn read_at_commit_with_shape_policy_dense(
        &self,
        commit_seq: u64,
        selectors: &[EntrySelector],
        options: HistoricalReadWithShapePolicyOptions,
        fill_value: f64,
    ) -> Result<HistoricalReadResult<DenseTensor>> {
        let prepared_selectors = self.prepare_selectors(selectors)?;
        let prepared_options = PreparedHistoricalReadWithShapePolicyOptions::new(&options)?;
        let mut raw_tensor = sys::ArcadiaTioTensor::default();
        let mut raw_mask = sys::ArcadiaTioMask::default();
        let mut report = new_historical_read_execution_report();
        let raw_options = prepared_options.raw_options();
        // SAFETY: Prepared selector and option buffers outlive the call; outputs are valid.
        let status = unsafe {
            sys::arcadia_tio_read_at_commit_with_shape_policy_dense(
                self.raw.as_ptr(),
                commit_seq,
                prepared_selectors.ptr(),
                prepared_selectors.len(),
                &raw_options,
                fill_value,
                &mut raw_tensor,
                &mut raw_mask,
                &mut report,
            )
        };
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: Outputs were initialized by this wrapper and may be partially populated.
            unsafe {
                sys::arcadia_tio_tensor_free(&mut raw_tensor);
                sys::arcadia_tio_mask_free(&mut raw_mask);
                sys::arcadia_tio_historical_read_execution_report_free(&mut report);
            }
            return Err(TioError::from_last_error(
                "failed to read dense tensor at commit with shape policy",
            ));
        }
        let tensor = copy_tensor(&raw_tensor);
        let mask = copy_mask(&raw_mask);
        let execution = copy_historical_read_execution_report(&report);
        // SAFETY: Native-owned outputs are freed exactly once.
        unsafe {
            sys::arcadia_tio_tensor_free(&mut raw_tensor);
            sys::arcadia_tio_mask_free(&mut raw_mask);
            sys::arcadia_tio_historical_read_execution_report_free(&mut report);
        }
        Ok(HistoricalReadResult {
            value: DenseTensor {
                tensor: tensor?,
                mask: mask?,
            },
            execution: execution?,
        })
    }

    pub(crate) fn append_with_range(
        &mut self,
        shape: &[u64],
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut u32, *mut u32) -> i32,
    ) -> Result<AppendRange> {
        let mut start = 0u32;
        let mut end = 0u32;
        let status = call(self.raw.as_ptr(), &mut start, &mut end);
        status_result(status, "failed to append tensor data")?;
        let _ = shape;
        Ok(AppendRange { start, end })
    }

    pub(crate) fn analyze_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut sys::ArcadiaTioSparseAppendAnalysis,
        ) -> i32,
    ) -> Result<SparseAppendAnalysis> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let mut raw_analysis = empty_sparse_append_analysis();
        let status = call(self.raw.as_ptr(), &raw_rule, &mut raw_analysis);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: `raw_analysis` was initialized to an empty native-compatible value before the
            // call. If native populated reasons before returning an error, this releases them once.
            unsafe { sys::arcadia_tio_sparse_append_analysis_free(&mut raw_analysis) };
            return Err(TioError::from_last_error("failed to analyze sparse append"));
        }
        take_sparse_append_analysis(&mut raw_analysis)
    }

    pub(crate) fn append_sparse(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *const sys::ArcadiaTioSparseRule) -> i32,
    ) -> Result<()> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        let status = call(self.raw.as_ptr(), &raw_rule);
        status_result(status, "failed to append sparse tensor data")
    }

    pub(crate) fn append_sparse_with_range(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRule,
            *mut u32,
            *mut u32,
        ) -> i32,
    ) -> Result<AppendRange> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw();
        self.append_with_range(shape, |handle, start, end| {
            call(handle, &raw_rule, start, end)
        })
    }

    pub(crate) fn analyze_sparse_append_v2(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRuleV2,
            *mut sys::ArcadiaTioSparseAppendAnalysis,
        ) -> i32,
    ) -> Result<SparseAppendAnalysis> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw_v2();
        let mut raw_analysis = empty_sparse_append_analysis();
        let status = call(self.raw.as_ptr(), &raw_rule, &mut raw_analysis);
        if status != sys::ARCADIA_TIO_ERROR_OK {
            // SAFETY: `raw_analysis` was initialized to an empty native-compatible value before the
            // call. If native populated reasons before returning an error, this releases them once.
            unsafe { sys::arcadia_tio_sparse_append_analysis_free(&mut raw_analysis) };
            return Err(TioError::from_last_error("failed to analyze sparse append"));
        }
        take_sparse_append_analysis(&mut raw_analysis)
    }

    pub(crate) fn append_sparse_with_range_v2(
        &mut self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
        call: impl FnOnce(
            *mut sys::ArcadiaTioHandle,
            *const sys::ArcadiaTioSparseRuleV2,
            *mut u32,
            *mut u32,
        ) -> i32,
    ) -> Result<AppendRange> {
        self.validate_sparse_append(dtype, data_len, shape, rule)?;
        let prepared_rule = PreparedSparseRule::new(rule);
        let raw_rule = prepared_rule.raw_v2();
        self.append_with_range(shape, |handle, start, end| {
            call(handle, &raw_rule, start, end)
        })
    }

    pub(crate) fn prepare_selectors(
        &self,
        selectors: &[EntrySelector],
    ) -> Result<PreparedSelectors> {
        PreparedSelectors::new(selectors, self.rank()?)
    }

    pub(crate) fn read_tensor(
        &self,
        call: impl FnOnce(*mut sys::ArcadiaTioHandle, *mut sys::ArcadiaTioTensor) -> i32,
    ) -> Result<Tensor> {
        let mut raw = NativeOutput::new(
            sys::ArcadiaTioTensor::default(),
            sys::arcadia_tio_tensor_free,
        );
        let status = call(self.raw.as_ptr(), raw.as_mut_ptr());
        status_result(status, "failed to read tensor")?;
        copy_tensor(raw.as_ref())
    }

    pub(crate) fn validate_axis(&self, axis: usize) -> Result<()> {
        let rank = self.rank()?;
        if axis >= rank {
            Err(TioError::invalid_argument(format!(
                "axis {axis} out of range for rank {rank}"
            )))
        } else {
            Ok(())
        }
    }

    pub(crate) fn coordinate_axis_range_selectors(
        &self,
        axis: usize,
        start: u32,
        end: u32,
    ) -> Result<Vec<EntrySelector>> {
        let rank = self.rank()?;
        if axis >= rank {
            return Err(TioError::invalid_argument(format!(
                "axis {axis} out of range for rank {rank}"
            )));
        }
        if start > end {
            return Err(TioError::invalid_argument(
                "Coordinate v2 read range start must be <= end",
            ));
        }
        let mut selectors = vec![EntrySelector::All; rank];
        selectors[axis] = EntrySelector::Range { start, end };
        Ok(selectors)
    }

    pub(crate) fn validate_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "append")
    }

    pub(crate) fn validate_sparse_append(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        rule: &SparseRule,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, "sparse append")?;
        rule.validate_for_append(dtype, shape.len(), self.append_axis()?)
    }

    pub(crate) fn validate_mutation_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        self.validate_typed_payload(dtype, data_len, shape, operation)
    }

    pub(crate) fn validate_typed_payload(
        &self,
        dtype: DType,
        data_len: usize,
        shape: &[u64],
        operation: &str,
    ) -> Result<()> {
        let actual_dtype = self.dtype()?;
        if actual_dtype != dtype {
            return Err(TioError::invalid_argument(format!(
                "{operation} dtype {dtype:?} does not match file dtype {actual_dtype:?}"
            )));
        }
        let rank = self.rank()?;
        if shape.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "{operation} shape rank {} does not match file rank {rank}",
                shape.len()
            )));
        }
        let expected_len = shape_element_len(shape)?;
        if expected_len != data_len {
            return Err(TioError::invalid_argument(format!(
                "{operation} data length {data_len} does not match shape element count {expected_len}"
            )));
        }
        Ok(())
    }

    pub(crate) fn from_raw_handle(
        raw: *mut sys::ArcadiaTioHandle,
        abi: AbiCompatible,
        context: &str,
    ) -> Result<Self> {
        let raw = NonNull::new(raw).ok_or_else(|| TioError::from_last_error(context))?;
        Ok(Self {
            raw,
            _abi: abi,
            _not_send_or_sync: PhantomData,
        })
    }
}

impl Drop for TensorFile {
    fn drop(&mut self) {
        // SAFETY: `TensorFile` owns this non-null handle and Drop runs at most once.
        unsafe { sys::arcadia_tio_close(self.raw.as_ptr()) };
    }
}

pub(crate) struct PreparedStringList {
    pub(crate) _strings: Vec<CString>,
    pub(crate) ptrs: Vec<*const c_char>,
}

impl PreparedStringList {
    pub(crate) fn new<S: AsRef<str>>(values: &[S], label: &str) -> Result<Self> {
        let strings = values
            .iter()
            .map(|value| string_to_cstring(value.as_ref(), label))
            .collect::<Result<Vec<_>>>()?;
        let ptrs = strings.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _strings: strings,
            ptrs,
        })
    }

    pub(crate) fn ptr(&self) -> *const *const c_char {
        if self.ptrs.is_empty() {
            ptr::null()
        } else {
            self.ptrs.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.ptrs.len()
    }
}

pub(crate) struct PreparedUserKvList {
    pub(crate) _keys: Vec<CString>,
    pub(crate) _values: Vec<CString>,
    pub(crate) key_ptrs: Vec<*const c_char>,
    pub(crate) value_ptrs: Vec<*const c_char>,
}

impl PreparedUserKvList {
    pub(crate) fn new<K, V>(values: &[(K, V)]) -> Result<Self>
    where
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let keys = values
            .iter()
            .map(|(key, _)| string_to_cstring(key.as_ref(), "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = values
            .iter()
            .map(|(_, value)| string_to_cstring(value.as_ref(), "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let key_ptrs = keys.iter().map(|value| value.as_ptr()).collect();
        let value_ptrs = user_values.iter().map(|value| value.as_ptr()).collect();
        Ok(Self {
            _keys: keys,
            _values: user_values,
            key_ptrs,
            value_ptrs,
        })
    }

    pub(crate) fn key_ptr(&self) -> *const *const c_char {
        if self.key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.key_ptrs.as_ptr()
        }
    }

    pub(crate) fn value_ptr(&self) -> *const *const c_char {
        if self.value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.value_ptrs.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.key_ptrs.len()
    }
}

pub(crate) struct PreparedCreate<'a> {
    pub(crate) abi: AbiCompatible,
    pub(crate) path: CString,
    pub(crate) dim_kinds: Vec<sys::ArcadiaTioAxisKind>,
    pub(crate) dim_lens: Vec<u32>,
    pub(crate) _dim_name_strings: Vec<CString>,
    pub(crate) dim_name_ptrs: Vec<*const c_char>,
    pub(crate) _symbols: Vec<CString>,
    pub(crate) symbol_ptrs: Vec<*const c_char>,
    pub(crate) _channels: Vec<CString>,
    pub(crate) channel_ptrs: Vec<*const c_char>,
    pub(crate) _user_keys: Vec<CString>,
    pub(crate) _user_values: Vec<CString>,
    pub(crate) user_key_ptrs: Vec<*const c_char>,
    pub(crate) user_value_ptrs: Vec<*const c_char>,
    pub(crate) _coordinate_names: Vec<Option<CString>>,
    pub(crate) _coordinate_external_uris: Vec<Option<CString>>,
    pub(crate) coordinate_inputs: Vec<sys::ArcadiaTioAxisCoordinateInput>,
    pub(crate) _coordinate_values: PhantomData<&'a [CoordinateSpec]>,
}

impl<'a> PreparedCreate<'a> {
    pub(crate) fn new(path: impl AsRef<Path>, options: &'a CreateOptions) -> Result<Self> {
        if options.dims.is_empty() {
            return Err(TioError::invalid_argument("rank must be > 0"));
        }
        if options.append_dim >= options.dims.len() {
            return Err(TioError::invalid_argument("append_dim out of range"));
        }
        if options.dims.len() > usize::MAX / 2 {
            return Err(TioError::invalid_argument("rank is too large"));
        }
        for (idx, dim) in options.dims.iter().enumerate() {
            if matches!(dim.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "dimension {idx} name cannot be empty"
                )));
            }
        }

        let abi = ensure_native_abi()?;
        let path = path_to_cstring(path)?;
        let dim_kinds = options
            .dims
            .iter()
            .map(|dim| dim.kind.to_raw())
            .collect::<Vec<_>>();
        let dim_lens = options.dims.iter().map(|dim| dim.len).collect::<Vec<_>>();

        let dim_name_strings = options
            .dims
            .iter()
            .filter_map(|dim| dim.name.as_ref())
            .map(|name| string_to_cstring(name, "dimension name"))
            .collect::<Result<Vec<_>>>()?;
        let mut dim_name_iter = dim_name_strings.iter();
        let dim_name_ptrs = options
            .dims
            .iter()
            .map(|dim| {
                if dim.name.is_some() {
                    dim_name_iter.next().expect("name count matches").as_ptr()
                } else {
                    ptr::null()
                }
            })
            .collect::<Vec<_>>();

        let symbols = options
            .symbols
            .iter()
            .map(|value| string_to_cstring(value, "symbol label"))
            .collect::<Result<Vec<_>>>()?;
        let symbol_ptrs = symbols
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let channels = options
            .channels
            .iter()
            .map(|value| string_to_cstring(value, "channel label"))
            .collect::<Result<Vec<_>>>()?;
        let channel_ptrs = channels
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_keys = options
            .user_kv
            .iter()
            .map(|(key, _)| string_to_cstring(key, "user metadata key"))
            .collect::<Result<Vec<_>>>()?;
        let user_values = options
            .user_kv
            .iter()
            .map(|(_, value)| string_to_cstring(value, "user metadata value"))
            .collect::<Result<Vec<_>>>()?;
        let user_key_ptrs = user_keys
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();
        let user_value_ptrs = user_values
            .iter()
            .map(|value| value.as_ptr())
            .collect::<Vec<_>>();

        for (idx, coord) in options.coordinates.iter().enumerate() {
            if coord.axis >= options.dims.len() {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} axis out of range"
                )));
            }
            if matches!(coord.name.as_deref(), Some("")) {
                return Err(TioError::invalid_argument(format!(
                    "coordinate {idx} name cannot be empty"
                )));
            }
        }
        let coordinate_names = options
            .coordinates
            .iter()
            .map(|coord| {
                coord
                    .name
                    .as_deref()
                    .map(|name| string_to_cstring(name, "coordinate name"))
                    .transpose()
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_external_uris = options
            .coordinates
            .iter()
            .map(|coord| match &coord.storage {
                CoordinateStorage::Inline(_) => Ok(None),
                CoordinateStorage::External { uri, .. } => {
                    string_to_cstring(uri, "external coordinate URI").map(Some)
                }
            })
            .collect::<Result<Vec<_>>>()?;
        let coordinate_inputs = options
            .coordinates
            .iter()
            .enumerate()
            .map(|(idx, coord)| {
                coordinate_input(
                    coord,
                    coordinate_names[idx].as_ref(),
                    coordinate_external_uris[idx].as_ref(),
                )
            })
            .collect::<Vec<_>>();

        Ok(Self {
            abi,
            path,
            dim_kinds,
            dim_lens,
            _dim_name_strings: dim_name_strings,
            dim_name_ptrs,
            _symbols: symbols,
            symbol_ptrs,
            _channels: channels,
            channel_ptrs,
            _user_keys: user_keys,
            _user_values: user_values,
            user_key_ptrs,
            user_value_ptrs,
            _coordinate_names: coordinate_names,
            _coordinate_external_uris: coordinate_external_uris,
            coordinate_inputs,
            _coordinate_values: PhantomData,
        })
    }

    pub(crate) fn dim_name_ptr(&self) -> *const *const c_char {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            ptr::null()
        } else {
            self.dim_name_ptrs.as_ptr()
        }
    }

    pub(crate) fn dim_name_len(&self) -> usize {
        if self.dim_name_ptrs.iter().all(|ptr| ptr.is_null()) {
            0
        } else {
            self.dim_name_ptrs.len()
        }
    }

    pub(crate) fn symbol_ptr(&self) -> *const *const c_char {
        if self.symbol_ptrs.is_empty() {
            ptr::null()
        } else {
            self.symbol_ptrs.as_ptr()
        }
    }

    pub(crate) fn symbol_len(&self) -> usize {
        self.symbol_ptrs.len()
    }

    pub(crate) fn channel_ptr(&self) -> *const *const c_char {
        if self.channel_ptrs.is_empty() {
            ptr::null()
        } else {
            self.channel_ptrs.as_ptr()
        }
    }

    pub(crate) fn channel_len(&self) -> usize {
        self.channel_ptrs.len()
    }

    pub(crate) fn user_key_ptr(&self) -> *const *const c_char {
        if self.user_key_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_key_ptrs.as_ptr()
        }
    }

    pub(crate) fn user_value_ptr(&self) -> *const *const c_char {
        if self.user_value_ptrs.is_empty() {
            ptr::null()
        } else {
            self.user_value_ptrs.as_ptr()
        }
    }

    pub(crate) fn user_kv_len(&self) -> usize {
        self.user_key_ptrs.len()
    }

    pub(crate) fn coordinate_ptr(&self) -> *const sys::ArcadiaTioAxisCoordinateInput {
        if self.coordinate_inputs.is_empty() {
            ptr::null()
        } else {
            self.coordinate_inputs.as_ptr()
        }
    }

    pub(crate) fn coordinate_len(&self) -> usize {
        self.coordinate_inputs.len()
    }
}

pub(crate) struct PreparedCreateUniverseOptions {
    pub(crate) axis_identities: Vec<sys::ArcadiaTioAxisIdentityInput>,
}

impl PreparedCreateUniverseOptions {
    pub(crate) fn new(options: &CreateUniverseOptions) -> Self {
        let axis_identities = options
            .axis_identities
            .iter()
            .map(|identity| sys::ArcadiaTioAxisIdentityInput {
                version: 1,
                struct_size: mem::size_of::<sys::ArcadiaTioAxisIdentityInput>(),
                axis: identity.axis,
                mode: identity.mode.to_raw(),
            })
            .collect();
        Self { axis_identities }
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioCreateWithUniverseOptions {
        sys::ArcadiaTioCreateWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioCreateWithUniverseOptions>(),
            axis_identities: if self.axis_identities.is_empty() {
                ptr::null()
            } else {
                self.axis_identities.as_ptr()
            },
            axis_identities_len: self.axis_identities.len(),
        }
    }
}

pub(crate) struct PreparedAppendUniverseOptions<'a> {
    pub(crate) slot_axes: Vec<Vec<sys::ArcadiaTioUniverseBindingInput>>,
    pub(crate) slots: Vec<sys::ArcadiaTioSlotUniverseBindingInput>,
    pub(crate) remap_axes: Vec<Vec<sys::ArcadiaTioUniverseRemapInput>>,
    pub(crate) remap_slots: Vec<sys::ArcadiaTioSlotUniverseRemapInput>,
    pub(crate) _borrowed: PhantomData<&'a AppendWithUniverseOptions>,
}

impl<'a> PreparedAppendUniverseOptions<'a> {
    pub(crate) fn new(options: &'a AppendWithUniverseOptions) -> Self {
        let slot_axes = options
            .slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseBindingInput {
                        axis: axis.axis,
                        family_uuid: axis.family_uuid,
                        version_uuid: axis.version_uuid,
                        length: axis.length,
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let slots = slot_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseBindingInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        let remap_axes = options
            .remap_slots
            .iter()
            .map(|slot| {
                slot.axes
                    .iter()
                    .map(|axis| sys::ArcadiaTioUniverseRemapInput {
                        version: 1,
                        struct_size: mem::size_of::<sys::ArcadiaTioUniverseRemapInput>(),
                        axis: axis.axis,
                        target_family_uuid: axis.target_family_uuid,
                        target_version_uuid: axis.target_version_uuid,
                        target_length: axis.target_length,
                        source_to_target: if axis.source_to_target.is_empty() {
                            ptr::null()
                        } else {
                            axis.source_to_target.as_ptr()
                        },
                        source_to_target_len: axis.source_to_target.len(),
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let remap_slots = remap_axes
            .iter()
            .map(|axes| sys::ArcadiaTioSlotUniverseRemapInput {
                axes: if axes.is_empty() {
                    ptr::null()
                } else {
                    axes.as_ptr()
                },
                axes_len: axes.len(),
            })
            .collect::<Vec<_>>();
        Self {
            slot_axes,
            slots,
            remap_axes,
            remap_slots,
            _borrowed: PhantomData,
        }
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioAppendWithUniverseOptions {
        let _ = (&self.slot_axes, &self.remap_axes);
        sys::ArcadiaTioAppendWithUniverseOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioAppendWithUniverseOptions>(),
            slots: if self.slots.is_empty() {
                ptr::null()
            } else {
                self.slots.as_ptr()
            },
            slots_len: self.slots.len(),
            remap_slots: if self.remap_slots.is_empty() {
                ptr::null()
            } else {
                self.remap_slots.as_ptr()
            },
            remap_slots_len: self.remap_slots.len(),
        }
    }
}

pub(crate) struct PreparedSparseRule {
    pub(crate) sparse_axes: Vec<usize>,
    pub(crate) detector_kind: sys::ArcadiaTioSparseDetectorKind,
    pub(crate) predicate: sys::ArcadiaTioSparseValuePredicate,
    pub(crate) predicate_v2: sys::ArcadiaTioSparseValuePredicateV2,
    pub(crate) min_absent_fraction: f64,
    pub(crate) min_absent_subtensors: u64,
    pub(crate) fallback: sys::ArcadiaTioSparseFallbackPolicy,
}

impl PreparedSparseRule {
    pub(crate) fn new(rule: &SparseRule) -> Self {
        Self {
            sparse_axes: rule.sparse_axes.clone(),
            detector_kind: rule.detector.to_raw(),
            predicate: rule.predicate.to_raw(),
            predicate_v2: rule.predicate.to_raw_v2(),
            min_absent_fraction: rule.min_absent_fraction,
            min_absent_subtensors: rule.min_absent_subtensors,
            fallback: rule.fallback.to_raw(),
        }
    }

    pub(crate) fn raw(&self) -> sys::ArcadiaTioSparseRule {
        sys::ArcadiaTioSparseRule {
            detector_kind: self.detector_kind,
            sparse_axes: if self.sparse_axes.is_empty() {
                ptr::null()
            } else {
                self.sparse_axes.as_ptr()
            },
            sparse_axes_len: self.sparse_axes.len(),
            predicate: self.predicate,
            min_absent_fraction: self.min_absent_fraction,
            min_absent_subtensors: self.min_absent_subtensors,
            fallback: self.fallback,
        }
    }

    pub(crate) fn raw_v2(&self) -> sys::ArcadiaTioSparseRuleV2 {
        sys::ArcadiaTioSparseRuleV2 {
            struct_size: mem::size_of::<sys::ArcadiaTioSparseRuleV2>() as u32,
            detector_kind: self.detector_kind,
            sparse_axes: if self.sparse_axes.is_empty() {
                ptr::null()
            } else {
                self.sparse_axes.as_ptr()
            },
            sparse_axes_len: self.sparse_axes.len(),
            predicate: self.predicate_v2,
            min_absent_fraction: self.min_absent_fraction,
            min_absent_subtensors: self.min_absent_subtensors,
            fallback: self.fallback,
        }
    }
}

pub(crate) struct PreparedSingleSelector {
    pub(crate) take_indices: Option<Vec<u32>>,
    pub(crate) selector: sys::ArcadiaTioEntrySelector,
}

impl PreparedSingleSelector {
    pub(crate) fn new(selector: &EntrySelector) -> Result<Self> {
        let (take_indices, selector) = match selector {
            EntrySelector::All => (
                None,
                sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
            ),
            EntrySelector::Range { start, end } => {
                if start > end {
                    return Err(TioError::invalid_argument(
                        "selector range start must be <= end",
                    ));
                }
                (
                    None,
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    },
                )
            }
            EntrySelector::Take(indices) => {
                let values = indices.clone();
                let selector = sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                    start: 0,
                    end: 0,
                    indices: if values.is_empty() {
                        ptr::null()
                    } else {
                        values.as_ptr()
                    },
                    indices_len: values.len(),
                };
                (Some(values), selector)
            }
        };
        Ok(Self {
            take_indices,
            selector,
        })
    }

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        &self.selector
    }
}

pub(crate) struct PreparedChunkKeys<'a> {
    pub(crate) keys: &'a [ChunkKey],
    pub(crate) raw: Vec<sys::ArcadiaTioChunkKey>,
}

impl<'a> PreparedChunkKeys<'a> {
    pub(crate) fn new(keys: &'a [ChunkKey]) -> Self {
        let raw = keys
            .iter()
            .map(|key| sys::ArcadiaTioChunkKey {
                coords: if key.coords.is_empty() {
                    ptr::null()
                } else {
                    key.coords.as_ptr()
                },
                len: key.coords.len(),
            })
            .collect();
        Self { keys, raw }
    }

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioChunkKey {
        let _ = &self.keys;
        if self.raw.is_empty() {
            ptr::null()
        } else {
            self.raw.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.raw.len()
    }
}

pub(crate) struct PreparedReadIndexItems {
    pub(crate) items: Vec<sys::ArcadiaTioReadIndexItem>,
}

impl PreparedReadIndexItems {
    pub(crate) fn new(items: &[ReadIndexItem], rank: usize) -> Result<Self> {
        let mut ellipsis_count = 0usize;
        let mut consuming = 0usize;
        let mut output_rank_without_ellipsis_fill = 0usize;
        for item in items {
            match item {
                ReadIndexItem::All | ReadIndexItem::Slice { .. } => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Index(_) => {
                    consuming = consuming
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::NewAxis => {
                    output_rank_without_ellipsis_fill = output_rank_without_ellipsis_fill
                        .checked_add(1)
                        .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
                }
                ReadIndexItem::Ellipsis => {
                    ellipsis_count += 1;
                    if ellipsis_count > 1 {
                        return Err(TioError::invalid_argument(
                            "read_index supports at most one ellipsis",
                        ));
                    }
                }
            }
        }
        if consuming > rank {
            return Err(TioError::invalid_argument(
                "read_index has too many axis-consuming items for file rank",
            ));
        }
        let ellipsis_or_padding_fill = rank - consuming;
        let output_rank = output_rank_without_ellipsis_fill
            .checked_add(ellipsis_or_padding_fill)
            .ok_or_else(|| TioError::invalid_argument("read_index rank overflow"))?;
        if output_rank == 0 {
            return Err(TioError::invalid_argument(
                "read_index scalar output is unsupported by the C ABI first slice",
            ));
        }
        let items = items
            .iter()
            .map(ReadIndexItem::to_raw)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { items })
    }

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioReadIndexItem {
        if self.items.is_empty() {
            ptr::null()
        } else {
            self.items.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
}

pub(crate) struct PreparedSelectors {
    pub(crate) take_indices: Vec<Vec<u32>>,
    pub(crate) selectors: Vec<sys::ArcadiaTioEntrySelector>,
}

impl PreparedSelectors {
    pub(crate) fn new(selectors: &[EntrySelector], rank: usize) -> Result<Self> {
        if selectors.is_empty() {
            return Ok(Self {
                take_indices: Vec::new(),
                selectors: Vec::new(),
            });
        }
        if selectors.len() != rank {
            return Err(TioError::invalid_argument(format!(
                "selector count {} does not match file rank {rank}",
                selectors.len()
            )));
        }
        let take_indices = selectors
            .iter()
            .filter_map(|selector| match selector {
                EntrySelector::Take(indices) => Some(indices.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut next_take = 0usize;
        let mut raw = Vec::with_capacity(selectors.len());
        for selector in selectors {
            let item = match selector {
                EntrySelector::All => sys::ArcadiaTioEntrySelector {
                    kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_ALL,
                    start: 0,
                    end: 0,
                    indices: ptr::null(),
                    indices_len: 0,
                },
                EntrySelector::Range { start, end } => {
                    if start > end {
                        return Err(TioError::invalid_argument(
                            "selector range start must be <= end",
                        ));
                    }
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_RANGE,
                        start: *start,
                        end: *end,
                        indices: ptr::null(),
                        indices_len: 0,
                    }
                }
                EntrySelector::Take(_) => {
                    let values = &take_indices[next_take];
                    next_take += 1;
                    sys::ArcadiaTioEntrySelector {
                        kind: sys::ARCADIA_TIO_ENTRY_SELECTOR_TAKE,
                        start: 0,
                        end: 0,
                        indices: if values.is_empty() {
                            ptr::null()
                        } else {
                            values.as_ptr()
                        },
                        indices_len: values.len(),
                    }
                }
            };
            raw.push(item);
        }
        Ok(Self {
            take_indices,
            selectors: raw,
        })
    }

    pub(crate) fn ptr(&self) -> *const sys::ArcadiaTioEntrySelector {
        let _ = &self.take_indices;
        if self.selectors.is_empty() {
            ptr::null()
        } else {
            self.selectors.as_ptr()
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.selectors.len()
    }
}

pub(crate) struct PreparedQueryTraceContext {
    pub(crate) run_id: CString,
    pub(crate) row_id: CString,
    pub(crate) phase: CString,
    pub(crate) language: CString,
    pub(crate) api_surface: CString,
    pub(crate) operation: CString,
    pub(crate) trace_clock: CString,
    pub(crate) repeat_index: u32,
}

impl PreparedQueryTraceContext {
    pub(crate) fn new(context: &QueryTraceContext) -> Result<Self> {
        Ok(Self {
            run_id: non_empty_cstring(&context.run_id, "query trace run_id")?,
            row_id: non_empty_cstring(&context.row_id, "query trace row_id")?,
            phase: non_empty_cstring(&context.phase, "query trace phase")?,
            language: non_empty_cstring(&context.language, "query trace language")?,
            api_surface: non_empty_cstring(&context.api_surface, "query trace api_surface")?,
            operation: non_empty_cstring(&context.operation, "query trace operation")?,
            trace_clock: non_empty_cstring(&context.trace_clock, "query trace trace_clock")?,
            repeat_index: context.repeat_index,
        })
    }

    pub(crate) fn raw_context(&self) -> sys::ArcadiaTioQueryTraceContext {
        sys::ArcadiaTioQueryTraceContext {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioQueryTraceContext>(),
            run_id: self.run_id.as_ptr(),
            row_id: self.row_id.as_ptr(),
            repeat_index: self.repeat_index,
            phase: self.phase.as_ptr(),
            language: self.language.as_ptr(),
            api_surface: self.api_surface.as_ptr(),
            operation: self.operation.as_ptr(),
            trace_clock: self.trace_clock.as_ptr(),
        }
    }
}

pub(crate) fn non_empty_cstring(value: &str, label: &str) -> Result<CString> {
    if value.is_empty() {
        return Err(TioError::invalid_argument(format!(
            "{label} must not be empty"
        )));
    }
    string_to_cstring(value, label)
}

pub(crate) struct PreparedReadWithOptions {
    pub(crate) mode: sys::ArcadiaTioReadExecutionMode,
    pub(crate) max_threads: usize,
}

impl PreparedReadWithOptions {
    pub(crate) fn new(options: &ReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioReadWithOptionsOptions {
        sys::ArcadiaTioReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

pub(crate) struct PreparedHistoricalReadWithOptions {
    pub(crate) mode: sys::ArcadiaTioReadExecutionMode,
    pub(crate) max_threads: usize,
}

impl PreparedHistoricalReadWithOptions {
    pub(crate) fn new(options: &HistoricalReadWithOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self { mode, max_threads })
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithOptionsOptions {
        sys::ArcadiaTioHistoricalReadWithOptionsOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithOptionsOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
        }
    }
}

pub(crate) struct PreparedReadShapePolicy {
    pub(crate) explicit_extents: Vec<u64>,
    pub(crate) explicit_universe_axes: Vec<sys::ArcadiaTioExplicitUniverseAxisTarget>,
    pub(crate) explicit_extent_axes: Vec<sys::ArcadiaTioExplicitExtentAxisTarget>,
    pub(crate) policy: sys::ArcadiaTioReadShapePolicyTag,
}

impl PreparedReadShapePolicy {
    pub(crate) fn new(policy: &ReadShapePolicy) -> Self {
        let explicit_extents = match policy {
            ReadShapePolicy::ExplicitExtents(extents) => extents.clone(),
            _ => Vec::new(),
        };
        let explicit_universe_axes = match policy {
            ReadShapePolicy::ExplicitUniverse(axes) => axes.iter().map(raw_universe_axis).collect(),
            ReadShapePolicy::ExplicitUniverseAndExtents { universe_axes, .. } => {
                universe_axes.iter().map(raw_universe_axis).collect()
            }
            _ => Vec::new(),
        };
        let explicit_extent_axes = match policy {
            ReadShapePolicy::ExplicitUniverseAndExtents { extent_axes, .. } => extent_axes
                .iter()
                .map(|axis| sys::ArcadiaTioExplicitExtentAxisTarget {
                    axis: axis.axis,
                    length: axis.length,
                })
                .collect(),
            _ => Vec::new(),
        };
        Self {
            explicit_extents,
            explicit_universe_axes,
            explicit_extent_axes,
            policy: policy.to_raw_tag(),
        }
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioReadShapePolicyOptions {
        sys::ArcadiaTioReadShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadShapePolicyOptions>(),
            policy: self.policy,
            explicit_extents: if self.explicit_extents.is_empty() {
                ptr::null()
            } else {
                self.explicit_extents.as_ptr()
            },
            explicit_extents_len: self.explicit_extents.len(),
            explicit_universe_axes: if self.explicit_universe_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_universe_axes.as_ptr()
            },
            explicit_universe_axes_len: self.explicit_universe_axes.len(),
            explicit_extent_axes: if self.explicit_extent_axes.is_empty() {
                ptr::null()
            } else {
                self.explicit_extent_axes.as_ptr()
            },
            explicit_extent_axes_len: self.explicit_extent_axes.len(),
        }
    }
}

pub(crate) fn raw_universe_axis(
    axis: &ExplicitUniverseAxisTarget,
) -> sys::ArcadiaTioExplicitUniverseAxisTarget {
    sys::ArcadiaTioExplicitUniverseAxisTarget {
        axis: axis.axis,
        family_uuid: axis.family_uuid,
        version_uuid: axis.version_uuid,
        length: axis.length,
    }
}

pub(crate) struct PreparedReadWithShapePolicyOptions {
    pub(crate) mode: sys::ArcadiaTioReadExecutionMode,
    pub(crate) max_threads: usize,
    pub(crate) shape_policy: PreparedReadShapePolicy,
}

impl PreparedReadWithShapePolicyOptions {
    pub(crate) fn new(options: &ReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioReadWithShapePolicyOptions {
        sys::ArcadiaTioReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

pub(crate) struct PreparedHistoricalReadWithShapePolicyOptions {
    pub(crate) mode: sys::ArcadiaTioReadExecutionMode,
    pub(crate) max_threads: usize,
    pub(crate) shape_policy: PreparedReadShapePolicy,
}

impl PreparedHistoricalReadWithShapePolicyOptions {
    pub(crate) fn new(options: &HistoricalReadWithShapePolicyOptions) -> Result<Self> {
        let (mode, max_threads) = options.mode.to_raw()?;
        Ok(Self {
            mode,
            max_threads,
            shape_policy: PreparedReadShapePolicy::new(&options.shape_policy),
        })
    }

    pub(crate) fn raw_options(&self) -> sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
        sys::ArcadiaTioHistoricalReadWithShapePolicyOptions {
            version: 1,
            struct_size: mem::size_of::<sys::ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            mode: self.mode,
            max_threads: self.max_threads,
            shape_policy: self.shape_policy.raw_options(),
        }
    }
}

pub(crate) fn coordinate_input(
    coord: &CoordinateSpec,
    name: Option<&CString>,
    external_uri: Option<&CString>,
) -> sys::ArcadiaTioAxisCoordinateInput {
    let (
        storage_kind,
        external_source_kind,
        external_uri_ptr,
        external_dtype,
        external_length,
        values_ptr,
        values_len,
        dtype,
    ) = match &coord.storage {
        CoordinateStorage::Inline(values) => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_INLINE,
            sys::ARCADIA_TIO_COORDINATE_SOURCE_SAME_FILE_OBJECT,
            ptr::null(),
            values.dtype().to_raw(),
            0,
            values.as_ptr(),
            values.len(),
            values.dtype(),
        ),
        CoordinateStorage::External {
            source_kind,
            uri: _,
            dtype,
            length,
        } => (
            sys::ARCADIA_TIO_COORDINATE_STORAGE_EXTERNAL,
            source_kind.to_raw(),
            external_uri.map_or(ptr::null(), |value| value.as_ptr()),
            dtype.to_raw(),
            *length,
            ptr::null(),
            0,
            *dtype,
        ),
    };
    sys::ArcadiaTioAxisCoordinateInput {
        version: 1,
        struct_size: mem::size_of::<sys::ArcadiaTioAxisCoordinateInput>(),
        axis: coord.axis,
        name: name.map_or(ptr::null(), |value| value.as_ptr()),
        kind: coord.kind.to_raw(),
        dtype: dtype.to_raw(),
        encoding: coord.encoding.to_raw(),
        values: values_ptr,
        values_len,
        sorted: coord.ordering.sorted.to_raw(),
        monotonicity: coord.ordering.monotonicity.to_raw(),
        uniqueness: coord.ordering.uniqueness.to_raw(),
        storage_kind,
        external_source_kind,
        external_uri: external_uri_ptr,
        external_dtype,
        external_length,
        required: u8::from(coord.required),
    }
}
