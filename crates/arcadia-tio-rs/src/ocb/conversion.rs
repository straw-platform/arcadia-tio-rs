use super::*;

pub(super) fn write_path(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    create_file: bool,
    options: Option<WriteOptions>,
) -> OcbResult<()> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let raw = RawWriteSpec::new(spec)?;
    let raw_options = options.map(WriteOptions::to_raw);
    let status = match (create_file, raw_options.as_ref()) {
        (true, Some(options)) => unsafe {
            sys::arcadia_tio_ocb_create_with_options(path.as_ptr(), &raw.raw, options)
        },
        (false, Some(options)) => unsafe {
            sys::arcadia_tio_ocb_append_with_options(path.as_ptr(), &raw.raw, options)
        },
        (true, None) => unsafe { sys::arcadia_tio_ocb_create(path.as_ptr(), &raw.raw) },
        (false, None) => unsafe { sys::arcadia_tio_ocb_append(path.as_ptr(), &raw.raw) },
    };
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(())
    } else {
        Err(OcbError::last(if create_file {
            "OCB create failed"
        } else {
            "OCB append failed"
        }))
    }
}

pub(super) fn write_path_report(
    path: impl AsRef<Path>,
    spec: &WriteSpec,
    create_file: bool,
    options: WriteOptions,
) -> OcbResult<WriteReport> {
    let path = path_to_cstring(path).map_err(OcbError::from_tio_error)?;
    let raw = RawWriteSpec::new(spec)?;
    let raw_options = options.to_raw();
    let mut raw_report = empty_write_report();
    let status = if create_file {
        unsafe {
            sys::arcadia_tio_ocb_create_with_options_and_report(
                path.as_ptr(),
                &raw.raw,
                &raw_options,
                &mut raw_report,
            )
        }
    } else {
        unsafe {
            sys::arcadia_tio_ocb_append_with_options_and_report(
                path.as_ptr(),
                &raw.raw,
                &raw_options,
                &mut raw_report,
            )
        }
    };
    if status == sys::ARCADIA_TIO_ERROR_OK {
        Ok(write_report_from_raw(&raw_report))
    } else {
        Err(OcbError::last(if create_file {
            "OCB create_with_report failed"
        } else {
            "OCB append_with_report failed"
        }))
    }
}

pub(super) struct RawWriteSpec {
    pub(super) raw: sys::ArcadiaTioOcbWriteSpec,
    pub(super) _column_names: Vec<CString>,
    pub(super) _dictionary_names: Vec<CString>,
    pub(super) _raw_columns: Vec<sys::ArcadiaTioOcbWriteColumn>,
    pub(super) _raw_entries: Vec<Vec<sys::ArcadiaTioOcbDictionaryEntry>>,
    pub(super) _raw_dictionaries: Vec<sys::ArcadiaTioOcbWriteDictionary>,
    pub(super) _raw_validities: Vec<Vec<sys::ArcadiaTioOcbValidityBitmap>>,
    pub(super) _raw_chunks: Vec<Vec<sys::ArcadiaTioOcbWriteColumnChunk>>,
    pub(super) _raw_rows: Vec<sys::ArcadiaTioOcbWriteRowGroup>,
    pub(super) _raw_ordering_keys: Vec<sys::ArcadiaTioOcbWriteOrderingKey>,
}

impl RawWriteSpec {
    pub(super) fn new(spec: &WriteSpec) -> OcbResult<Self> {
        let column_names = spec
            .columns
            .iter()
            .map(|column| cstring(&column.name, "OCB column name"))
            .collect::<OcbResult<Vec<_>>>()?;
        let dictionary_names = spec
            .dictionaries
            .iter()
            .map(|dictionary| cstring(&dictionary.name, "OCB dictionary name"))
            .collect::<OcbResult<Vec<_>>>()?;

        validate_write_spec(spec)?;

        let raw_columns = spec
            .columns
            .iter()
            .zip(column_names.iter())
            .map(|(column, name)| sys::ArcadiaTioOcbWriteColumn {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteColumn>(),
                name: name.as_ptr(),
                physical_type: column.physical_type.to_raw(),
                logical_kind: column.logical_kind.to_raw(),
                has_dictionary_id: u8::from(column.dictionary_id.is_some()),
                dictionary_id: column.dictionary_id.unwrap_or(0),
                scale: column.scale,
                nullable: u8::from(column.nullable),
                reserved: [u64::from(column.physical_type.fixed_binary_width()), 0, 0],
            })
            .collect::<Vec<_>>();

        let raw_entries = spec
            .dictionaries
            .iter()
            .map(|dictionary| {
                dictionary
                    .entries
                    .iter()
                    .map(|entry| sys::ArcadiaTioOcbDictionaryEntry {
                        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                        struct_size: mem::size_of::<sys::ArcadiaTioOcbDictionaryEntry>(),
                        data: entry.as_ptr(),
                        len: entry.len(),
                        reserved: [0; 3],
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let raw_dictionaries = spec
            .dictionaries
            .iter()
            .zip(dictionary_names.iter())
            .zip(raw_entries.iter())
            .map(
                |((dictionary, name), entries)| sys::ArcadiaTioOcbWriteDictionary {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteDictionary>(),
                    dictionary_id: dictionary.dictionary_id,
                    name: name.as_ptr(),
                    code_physical_type: dictionary.code_physical_type.to_raw(),
                    value_kind: dictionary.value_kind.to_raw(),
                    fixed_width: dictionary.fixed_width,
                    entries: entries.as_ptr(),
                    entries_len: entries.len(),
                    reserved: [0; 3],
                },
            )
            .collect::<Vec<_>>();

        let raw_validities = spec
            .row_groups
            .iter()
            .map(|row| {
                row.columns
                    .iter()
                    .map(|chunk| match &chunk.validity {
                        Some(validity) => sys::ArcadiaTioOcbValidityBitmap {
                            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                            struct_size: mem::size_of::<sys::ArcadiaTioOcbValidityBitmap>(),
                            data: validity.bytes.as_ptr(),
                            len: validity.bytes.len(),
                            row_count: validity.row_count,
                            reserved: [0; 3],
                        },
                        None => sys::ArcadiaTioOcbValidityBitmap {
                            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                            struct_size: mem::size_of::<sys::ArcadiaTioOcbValidityBitmap>(),
                            data: ptr::null(),
                            len: 0,
                            row_count: 0,
                            reserved: [0; 3],
                        },
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let raw_chunks = spec
            .row_groups
            .iter()
            .zip(raw_validities.iter())
            .map(|(row, validities)| {
                row.columns
                    .iter()
                    .zip(validities.iter())
                    .map(|(chunk, validity)| sys::ArcadiaTioOcbWriteColumnChunk {
                        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                        struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteColumnChunk>(),
                        column_id: chunk.column_id,
                        values: chunk.values.to_raw(),
                        validity: if chunk.validity.is_some() {
                            validity as *const sys::ArcadiaTioOcbValidityBitmap
                        } else {
                            ptr::null()
                        },
                        reserved: [0; 3],
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();

        let raw_rows = raw_chunks
            .iter()
            .map(|chunks| sys::ArcadiaTioOcbWriteRowGroup {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteRowGroup>(),
                columns: chunks.as_ptr(),
                columns_len: chunks.len(),
                reserved: [0; 3],
            })
            .collect::<Vec<_>>();

        let raw_ordering_keys = spec
            .ordering_keys
            .iter()
            .map(|key| sys::ArcadiaTioOcbWriteOrderingKey {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteOrderingKey>(),
                column_id: key.column_id,
                direction: key.direction.to_raw(),
                null_order: key.null_order.to_raw(),
                reserved: [0; 3],
            })
            .collect::<Vec<_>>();

        let raw = sys::ArcadiaTioOcbWriteSpec {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteSpec>(),
            columns: raw_columns.as_ptr(),
            columns_len: raw_columns.len(),
            dictionaries: raw_dictionaries.as_ptr(),
            dictionaries_len: raw_dictionaries.len(),
            row_groups: raw_rows.as_ptr(),
            row_groups_len: raw_rows.len(),
            ordering_keys: raw_ordering_keys.as_ptr(),
            ordering_keys_len: raw_ordering_keys.len(),
            reserved: [0; 4],
        };

        Ok(Self {
            raw,
            _column_names: column_names,
            _dictionary_names: dictionary_names,
            _raw_columns: raw_columns,
            _raw_entries: raw_entries,
            _raw_dictionaries: raw_dictionaries,
            _raw_validities: raw_validities,
            _raw_chunks: raw_chunks,
            _raw_rows: raw_rows,
            _raw_ordering_keys: raw_ordering_keys,
        })
    }
}

pub(super) fn validate_write_spec(spec: &WriteSpec) -> OcbResult<()> {
    for (column_idx, column) in spec.columns.iter().enumerate() {
        if let PhysicalType::FixedBinary { width } = column.physical_type {
            if width == 0 {
                return Err(OcbError::invalid_input(format!(
                    "OCB fixed-binary column {} ('{}') has zero width",
                    column_idx, column.name
                )));
            }
            if column.logical_kind == LogicalKind::DictionaryCode {
                return Err(OcbError::invalid_input(format!(
                    "OCB fixed-binary column {} ('{}') cannot be a dictionary-code column",
                    column_idx, column.name
                )));
            }
        }
    }

    for (row_idx, row) in spec.row_groups.iter().enumerate() {
        for chunk in &row.columns {
            let Some(column) = spec.columns.get(chunk.column_id as usize) else {
                continue;
            };
            match (&chunk.values, column.physical_type) {
                (
                    PrimitiveValues::FixedBinary { width, bytes },
                    PhysicalType::FixedBinary {
                        width: schema_width,
                    },
                ) => {
                    if *width == 0 {
                        return Err(OcbError::invalid_input(format!(
                            "OCB fixed-binary chunk row group {row_idx} column {} has zero width",
                            chunk.column_id
                        )));
                    }
                    if *width != schema_width {
                        return Err(OcbError::invalid_input(format!(
                            "OCB fixed-binary chunk row group {row_idx} column {} width {} does not match schema width {}",
                            chunk.column_id, width, schema_width
                        )));
                    }
                    if bytes.len() % *width as usize != 0 {
                        return Err(OcbError::invalid_input(format!(
                            "OCB fixed-binary chunk row group {row_idx} column {} byte length {} is not divisible by width {}",
                            chunk.column_id,
                            bytes.len(),
                            width
                        )));
                    }
                }
                (PrimitiveValues::FixedBinary { .. }, _) => {
                    return Err(OcbError::invalid_input(format!(
                        "OCB fixed-binary chunk row group {row_idx} column {} targets a non-fixed-binary schema column",
                        chunk.column_id
                    )));
                }
                (_, PhysicalType::FixedBinary { .. }) => {
                    return Err(OcbError::invalid_input(format!(
                        "OCB chunk row group {row_idx} column {} must provide fixed-binary values",
                        chunk.column_id
                    )));
                }
                _ => {}
            }
        }
    }

    Ok(())
}

pub(super) struct RawReadRequest {
    pub(super) raw: sys::ArcadiaTioOcbReadRequest,
    pub(super) _column_names: Vec<CString>,
    pub(super) _column_name_ptrs: Vec<*const c_char>,
    pub(super) _predicate_columns: Vec<CString>,
    pub(super) _predicates: Vec<sys::ArcadiaTioOcbRowGroupPredicate>,
}

impl RawReadRequest {
    pub(super) fn new(request: &ReadRequest) -> OcbResult<Self> {
        let (column_names, column_name_ptrs, projection_kind) = match &request.projection {
            Projection::All => (Vec::new(), Vec::new(), sys::ARCADIA_TIO_OCB_PROJECTION_ALL),
            Projection::Names(names) => {
                let column_names = names
                    .iter()
                    .map(|name| cstring(name, "OCB projection column"))
                    .collect::<OcbResult<Vec<_>>>()?;
                let column_name_ptrs = column_names.iter().map(|name| name.as_ptr()).collect();
                (
                    column_names,
                    column_name_ptrs,
                    sys::ARCADIA_TIO_OCB_PROJECTION_NAMES,
                )
            }
        };
        let predicate_columns = request
            .predicates
            .iter()
            .map(|predicate| cstring(&predicate.column, "OCB predicate column"))
            .collect::<OcbResult<Vec<_>>>()?;
        let predicates = request
            .predicates
            .iter()
            .zip(predicate_columns.iter())
            .map(|(predicate, column)| sys::ArcadiaTioOcbRowGroupPredicate {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupPredicate>(),
                column: column.as_ptr(),
                has_lower: u8::from(predicate.lower.is_some()),
                lower: predicate.lower.unwrap_or(PredicateValue::I32(0)).to_raw(),
                has_upper: u8::from(predicate.upper.is_some()),
                upper: predicate.upper.unwrap_or(PredicateValue::I32(0)).to_raw(),
                reserved: [0; 3],
            })
            .collect::<Vec<_>>();
        let raw = sys::ArcadiaTioOcbReadRequest {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbReadRequest>(),
            projection_kind,
            column_names: column_name_ptrs.as_ptr(),
            column_names_len: column_name_ptrs.len(),
            predicates: predicates.as_ptr(),
            predicates_len: predicates.len(),
            max_threads: request.max_threads,
            validate_checksums: u8::from(request.validate_checksums),
            decode_dictionaries: u8::from(request.decode_dictionaries),
            reserved: [0; 4],
        };
        Ok(Self {
            raw,
            _column_names: column_names,
            _column_name_ptrs: column_name_ptrs,
            _predicate_columns: predicate_columns,
            _predicates: predicates,
        })
    }
}

pub(super) struct RawFillRequest {
    pub(super) raw: sys::ArcadiaTioOcbRowGroupFillRequest,
    pub(super) _column_names: Vec<CString>,
    pub(super) raw_columns: Vec<sys::ArcadiaTioOcbColumnFillBuffer>,
}

impl RawFillRequest {
    pub(super) fn new(
        row_group_id: u32,
        buffers: &mut [ColumnFillBufferMut<'_>],
        options: ReadFillOptions,
    ) -> OcbResult<Self> {
        if buffers.is_empty() {
            return Err(OcbError::invalid_input(
                "OCB fill request requires at least one column buffer",
            ));
        }
        let mut column_names = Vec::new();
        let mut selectors = Vec::with_capacity(buffers.len());
        for buffer in buffers.iter() {
            match column_fill_selector(buffer)? {
                RawColumnFillSelector::Name(name) => {
                    column_names.push(cstring(name, "OCB fill column")?);
                    selectors.push((column_names.last().expect("just pushed").as_ptr(), None));
                }
                RawColumnFillSelector::Id(column_id) => {
                    selectors.push((ptr::null(), Some(column_id)));
                }
            }
        }
        let mut raw_columns = buffers
            .iter_mut()
            .zip(selectors.iter().copied())
            .map(|(buffer, (name, column_id))| raw_column_fill_buffer(buffer, name, column_id))
            .collect::<Vec<_>>();
        let raw = sys::ArcadiaTioOcbRowGroupFillRequest {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupFillRequest>(),
            row_group_id,
            columns: raw_columns.as_mut_ptr(),
            columns_len: raw_columns.len(),
            validate_checksums: u8::from(options.validate_checksums),
            reserved: [0; 8],
        };
        Ok(Self {
            raw,
            _column_names: column_names,
            raw_columns,
        })
    }
}

pub(super) enum RawColumnFillSelector<'a> {
    Name(&'a str),
    Id(u32),
}

pub(super) fn column_fill_selector<'a>(
    buffer: &ColumnFillBufferMut<'a>,
) -> OcbResult<RawColumnFillSelector<'a>> {
    match buffer {
        ColumnFillBufferMut::I32 { name, .. }
        | ColumnFillBufferMut::I64 { name, .. }
        | ColumnFillBufferMut::F32 { name, .. }
        | ColumnFillBufferMut::F64 { name, .. }
        | ColumnFillBufferMut::FixedBinary { name, .. } => Ok(RawColumnFillSelector::Name(name)),
        ColumnFillBufferMut::I32ById { column_id, .. }
        | ColumnFillBufferMut::I64ById { column_id, .. }
        | ColumnFillBufferMut::F32ById { column_id, .. }
        | ColumnFillBufferMut::F64ById { column_id, .. }
        | ColumnFillBufferMut::FixedBinaryById { column_id, .. } => {
            Ok(RawColumnFillSelector::Id(*column_id))
        }
    }
}

pub(super) fn raw_column_fill_buffer(
    buffer: &mut ColumnFillBufferMut<'_>,
    name: *const c_char,
    column_id: Option<u32>,
) -> sys::ArcadiaTioOcbColumnFillBuffer {
    let mut raw = sys::ArcadiaTioOcbColumnFillBuffer {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnFillBuffer>(),
        column_name: name,
        column_id: column_id.unwrap_or(0),
        has_column_id: u8::from(column_id.is_some()),
        physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
        values: ptr::null_mut(),
        values_len: 0,
        validity_bytes: ptr::null_mut(),
        validity_bytes_len: 0,
        allow_nulls: 0,
        rows_filled: 0,
        validity_filled: 0,
        reserved: [0; 8],
    };
    match buffer {
        ColumnFillBufferMut::I32 {
            values,
            validity,
            allow_nulls,
            ..
        }
        | ColumnFillBufferMut::I32ById {
            values,
            validity,
            allow_nulls,
            ..
        } => {
            raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32;
            raw.values = values.as_mut_ptr().cast();
            raw.values_len = values.len();
            set_raw_validity(&mut raw, validity);
            raw.allow_nulls = u8::from(*allow_nulls);
        }
        ColumnFillBufferMut::I64 {
            values,
            validity,
            allow_nulls,
            ..
        }
        | ColumnFillBufferMut::I64ById {
            values,
            validity,
            allow_nulls,
            ..
        } => {
            raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64;
            raw.values = values.as_mut_ptr().cast();
            raw.values_len = values.len();
            set_raw_validity(&mut raw, validity);
            raw.allow_nulls = u8::from(*allow_nulls);
        }
        ColumnFillBufferMut::F32 {
            values,
            validity,
            allow_nulls,
            ..
        }
        | ColumnFillBufferMut::F32ById {
            values,
            validity,
            allow_nulls,
            ..
        } => {
            raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32;
            raw.values = values.as_mut_ptr().cast();
            raw.values_len = values.len();
            set_raw_validity(&mut raw, validity);
            raw.allow_nulls = u8::from(*allow_nulls);
        }
        ColumnFillBufferMut::F64 {
            values,
            validity,
            allow_nulls,
            ..
        }
        | ColumnFillBufferMut::F64ById {
            values,
            validity,
            allow_nulls,
            ..
        } => {
            raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64;
            raw.values = values.as_mut_ptr().cast();
            raw.values_len = values.len();
            set_raw_validity(&mut raw, validity);
            raw.allow_nulls = u8::from(*allow_nulls);
        }
        ColumnFillBufferMut::FixedBinary {
            width,
            bytes,
            validity,
            allow_nulls,
            ..
        }
        | ColumnFillBufferMut::FixedBinaryById {
            width,
            bytes,
            validity,
            allow_nulls,
            ..
        } => {
            raw.physical_type = sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY;
            raw.values = bytes.as_mut_ptr().cast();
            raw.values_len = bytes.len();
            raw.reserved[0] = u64::from(*width);
            set_raw_validity(&mut raw, validity);
            raw.allow_nulls = u8::from(*allow_nulls);
        }
    }
    raw
}

pub(super) fn set_raw_validity(
    raw: &mut sys::ArcadiaTioOcbColumnFillBuffer,
    validity: &mut Option<&mut [u8]>,
) {
    if let Some(bytes) = validity.as_deref_mut() {
        raw.validity_bytes = bytes.as_mut_ptr();
        raw.validity_bytes_len = bytes.len();
    }
}

pub(super) fn cstring(value: &str, label: &str) -> OcbResult<CString> {
    CString::new(value).map_err(|_| OcbError::invalid_input(format!("{label} contains NUL")))
}

pub(super) fn cstring_ptr(value: &CString) -> *mut c_char {
    value.as_ptr() as *mut c_char
}

pub(super) fn optional_cstring_ptr(value: &Option<CString>) -> *mut c_char {
    value
        .as_ref()
        .map_or(ptr::null_mut(), |value| cstring_ptr(value))
}

pub(super) struct RawManifestBuildOptions {
    pub(super) raw: sys::ArcadiaTioOcbManifestBuildOptions,
    pub(super) _generated_by_name: Option<CString>,
    pub(super) _generated_by_version: Option<CString>,
}

impl RawManifestBuildOptions {
    pub(super) fn new(options: &ManifestBuildOptions) -> OcbResult<Self> {
        let generated_by_name = options
            .generated_by_name
            .as_deref()
            .map(|value| cstring(value, "OCB manifest generated_by_name"))
            .transpose()?;
        let generated_by_version = options
            .generated_by_version
            .as_deref()
            .map(|value| cstring(value, "OCB manifest generated_by_version"))
            .transpose()?;
        let mut raw = sys::ArcadiaTioOcbManifestBuildOptions {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestBuildOptions>(),
            validation: sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH,
            compute_file_digest: 0,
            generated_by_name: ptr::null(),
            generated_by_version: ptr::null(),
            has_generated_at_unix_seconds: 0,
            generated_at_unix_seconds: 0,
            reserved: [0; 4],
        };
        unsafe { sys::arcadia_tio_ocb_manifest_build_options_init(&mut raw) };
        raw.validation = options.validation.to_raw();
        raw.compute_file_digest = u8::from(options.compute_file_digest);
        raw.generated_by_name = generated_by_name
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        raw.generated_by_version = generated_by_version
            .as_ref()
            .map_or(ptr::null(), |value| value.as_ptr());
        raw.has_generated_at_unix_seconds = u8::from(options.generated_at_unix_seconds.is_some());
        raw.generated_at_unix_seconds = options.generated_at_unix_seconds.unwrap_or(0);
        Ok(Self {
            raw,
            _generated_by_name: generated_by_name,
            _generated_by_version: generated_by_version,
        })
    }
}

pub(super) struct RawManifest {
    pub(super) raw: sys::ArcadiaTioOcbManifest,
    pub(super) _schema: CString,
    pub(super) _tool_name: CString,
    pub(super) _tool_version: CString,
    pub(super) _entry_paths: Vec<CString>,
    pub(super) _entry_uris: Vec<Option<CString>>,
    pub(super) _digest_algorithms: Vec<Option<CString>>,
    pub(super) _digest_values: Vec<Option<CString>>,
    pub(super) _fingerprint_algorithms: Vec<CString>,
    pub(super) _fingerprint_schemas: Vec<CString>,
    pub(super) _fingerprint_dictionaries: Vec<CString>,
    pub(super) _fingerprint_orderings: Vec<CString>,
    pub(super) _fingerprint_combined: Vec<CString>,
    pub(super) _validation_modes: Vec<CString>,
    pub(super) _validation_statuses: Vec<CString>,
    pub(super) _issue_codes: Vec<Vec<CString>>,
    pub(super) _issue_field_paths: Vec<Vec<Option<CString>>>,
    pub(super) _issue_messages: Vec<Vec<CString>>,
    pub(super) _raw_issue_rows: Vec<Vec<sys::ArcadiaTioOcbManifestIssue>>,
    pub(super) _raw_validations: Vec<sys::ArcadiaTioOcbManifestEntryValidation>,
    pub(super) _raw_entries: Vec<sys::ArcadiaTioOcbManifestEntry>,
}

impl RawManifest {
    pub(super) fn new(manifest: &Manifest) -> OcbResult<Self> {
        let schema = cstring(&manifest.schema, "OCB manifest schema")?;
        let tool_name = cstring(&manifest.generated_by.name, "OCB manifest tool name")?;
        let tool_version = cstring(&manifest.generated_by.version, "OCB manifest tool version")?;

        let mut entry_paths = Vec::with_capacity(manifest.entries.len());
        let mut entry_uris = Vec::with_capacity(manifest.entries.len());
        let mut digest_algorithms = Vec::with_capacity(manifest.entries.len());
        let mut digest_values = Vec::with_capacity(manifest.entries.len());
        let mut fingerprint_algorithms = Vec::with_capacity(manifest.entries.len());
        let mut fingerprint_schemas = Vec::with_capacity(manifest.entries.len());
        let mut fingerprint_dictionaries = Vec::with_capacity(manifest.entries.len());
        let mut fingerprint_orderings = Vec::with_capacity(manifest.entries.len());
        let mut fingerprint_combined = Vec::with_capacity(manifest.entries.len());
        let mut validation_modes = Vec::with_capacity(manifest.entries.len());
        let mut validation_statuses = Vec::with_capacity(manifest.entries.len());
        let mut issue_codes = Vec::with_capacity(manifest.entries.len());
        let mut issue_field_paths = Vec::with_capacity(manifest.entries.len());
        let mut issue_messages = Vec::with_capacity(manifest.entries.len());
        let mut raw_issue_rows = Vec::with_capacity(manifest.entries.len());

        for (entry_index, entry) in manifest.entries.iter().enumerate() {
            entry_paths.push(cstring(&entry.path, "OCB manifest entry path")?);
            entry_uris.push(
                entry
                    .uri
                    .as_deref()
                    .map(|value| cstring(value, "OCB manifest entry URI"))
                    .transpose()?,
            );
            digest_algorithms.push(
                entry
                    .digest
                    .as_ref()
                    .map(|digest| cstring(&digest.algorithm, "OCB manifest digest algorithm"))
                    .transpose()?,
            );
            digest_values.push(
                entry
                    .digest
                    .as_ref()
                    .map(|digest| cstring(&digest.digest, "OCB manifest digest"))
                    .transpose()?,
            );
            fingerprint_algorithms.push(cstring(
                &entry.fingerprints.algorithm,
                "OCB manifest fingerprint algorithm",
            )?);
            fingerprint_schemas.push(cstring(
                &entry.fingerprints.schema,
                "OCB manifest schema fingerprint",
            )?);
            fingerprint_dictionaries.push(cstring(
                &entry.fingerprints.dictionaries,
                "OCB manifest dictionaries fingerprint",
            )?);
            fingerprint_orderings.push(cstring(
                &entry.fingerprints.ordering,
                "OCB manifest ordering fingerprint",
            )?);
            fingerprint_combined.push(cstring(
                &entry.fingerprints.combined,
                "OCB manifest combined fingerprint",
            )?);
            validation_modes.push(cstring(
                &entry.validation.mode,
                "OCB manifest validation mode",
            )?);
            validation_statuses.push(cstring(
                &entry.validation.status,
                "OCB manifest validation status",
            )?);

            let codes = entry
                .validation
                .issues
                .iter()
                .map(|issue| cstring(&issue.code, "OCB manifest issue code"))
                .collect::<OcbResult<Vec<_>>>()?;
            let field_paths = entry
                .validation
                .issues
                .iter()
                .map(|issue| {
                    issue
                        .field_path
                        .as_deref()
                        .map(|value| cstring(value, "OCB manifest issue field_path"))
                        .transpose()
                })
                .collect::<OcbResult<Vec<_>>>()?;
            let messages = entry
                .validation
                .issues
                .iter()
                .map(|issue| cstring(&issue.message, "OCB manifest issue message"))
                .collect::<OcbResult<Vec<_>>>()?;
            issue_codes.push(codes);
            issue_field_paths.push(field_paths);
            issue_messages.push(messages);

            let raw_issues = (0..entry.validation.issues.len())
                .map(|issue_index| sys::ArcadiaTioOcbManifestIssue {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestIssue>(),
                    code: cstring_ptr(&issue_codes[entry_index][issue_index]),
                    field_path: optional_cstring_ptr(&issue_field_paths[entry_index][issue_index]),
                    message: cstring_ptr(&issue_messages[entry_index][issue_index]),
                    reserved: [0; 4],
                })
                .collect::<Vec<_>>();
            raw_issue_rows.push(raw_issues);
        }

        let raw_validations = manifest
            .entries
            .iter()
            .enumerate()
            .map(|(index, _)| sys::ArcadiaTioOcbManifestEntryValidation {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestEntryValidation>(),
                mode: cstring_ptr(&validation_modes[index]),
                status: cstring_ptr(&validation_statuses[index]),
                issues: if raw_issue_rows[index].is_empty() {
                    ptr::null_mut()
                } else {
                    raw_issue_rows[index].as_mut_ptr()
                },
                issues_len: raw_issue_rows[index].len(),
                reserved: [0; 4],
            })
            .collect::<Vec<_>>();

        let mut raw_entries = manifest
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| sys::ArcadiaTioOcbManifestEntry {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestEntry>(),
                path: cstring_ptr(&entry_paths[index]),
                uri: optional_cstring_ptr(&entry_uris[index]),
                has_file_bytes: u8::from(entry.file_bytes.is_some()),
                file_bytes: entry.file_bytes.unwrap_or(0),
                has_digest: u8::from(entry.digest.is_some()),
                digest: sys::ArcadiaTioOcbManifestDigest {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestDigest>(),
                    algorithm: optional_cstring_ptr(&digest_algorithms[index]),
                    digest: optional_cstring_ptr(&digest_values[index]),
                    reserved: [0; 4],
                },
                root_generation: entry.root_generation,
                row_count: entry.row_count,
                row_group_count: entry.row_group_count,
                fingerprints: sys::ArcadiaTioOcbManifestFingerprints {
                    version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                    struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestFingerprints>(),
                    algorithm: cstring_ptr(&fingerprint_algorithms[index]),
                    schema: cstring_ptr(&fingerprint_schemas[index]),
                    dictionaries: cstring_ptr(&fingerprint_dictionaries[index]),
                    ordering: cstring_ptr(&fingerprint_orderings[index]),
                    combined: cstring_ptr(&fingerprint_combined[index]),
                    reserved: [0; 4],
                },
                validation: raw_validations[index],
                reserved: [0; 4],
            })
            .collect::<Vec<_>>();

        let raw = sys::ArcadiaTioOcbManifest {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbManifest>(),
            schema: cstring_ptr(&schema),
            generated_by: sys::ArcadiaTioOcbManifestTool {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestTool>(),
                name: cstring_ptr(&tool_name),
                version_text: cstring_ptr(&tool_version),
                generated_at_unix_seconds: manifest.generated_by.generated_at_unix_seconds,
                reserved: [0; 4],
            },
            entries: if raw_entries.is_empty() {
                ptr::null_mut()
            } else {
                raw_entries.as_mut_ptr()
            },
            entries_len: raw_entries.len(),
            reserved: [0; 4],
        };

        Ok(Self {
            raw,
            _schema: schema,
            _tool_name: tool_name,
            _tool_version: tool_version,
            _entry_paths: entry_paths,
            _entry_uris: entry_uris,
            _digest_algorithms: digest_algorithms,
            _digest_values: digest_values,
            _fingerprint_algorithms: fingerprint_algorithms,
            _fingerprint_schemas: fingerprint_schemas,
            _fingerprint_dictionaries: fingerprint_dictionaries,
            _fingerprint_orderings: fingerprint_orderings,
            _fingerprint_combined: fingerprint_combined,
            _validation_modes: validation_modes,
            _validation_statuses: validation_statuses,
            _issue_codes: issue_codes,
            _issue_field_paths: issue_field_paths,
            _issue_messages: issue_messages,
            _raw_issue_rows: raw_issue_rows,
            _raw_validations: raw_validations,
            _raw_entries: raw_entries,
        })
    }
}

pub(super) fn raw_open_options(options: OpenOptions) -> sys::ArcadiaTioOcbOpenOptions {
    let mut raw = sys::ArcadiaTioOcbOpenOptions {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbOpenOptions>(),
        validation: options.validation.to_raw(),
        reserved: [0; 4],
    };
    unsafe { sys::arcadia_tio_ocb_open_options_init(&mut raw) };
    raw.validation = options.validation.to_raw();
    raw
}

pub(super) fn raw_resource_limits(
    resource_limits: ResourceLimits,
) -> sys::ArcadiaTioOcbResourceLimits {
    // A fully initialized zero header makes an initializer defect fail
    // closed at native version/size validation without exposing Rust to
    // uninitialized memory.
    let mut raw = sys::ArcadiaTioOcbResourceLimits {
        version: 0,
        struct_size: 0,
        max_encoded_object_bytes: 0,
        max_compressed_chunk_bytes: 0,
        max_decompressed_chunk_bytes: 0,
        max_projected_row_group_bytes: 0,
        max_owned_selected_compressed_bytes: 0,
        max_owned_decoded_materialized_bytes: 0,
        reserved: [0; 4],
    };
    unsafe { sys::arcadia_tio_ocb_resource_limits_init(&mut raw) };
    raw.max_encoded_object_bytes = resource_limits.max_encoded_object_bytes;
    raw.max_compressed_chunk_bytes = resource_limits.max_compressed_chunk_bytes;
    raw.max_decompressed_chunk_bytes = resource_limits.max_decompressed_chunk_bytes;
    raw.max_projected_row_group_bytes = resource_limits.max_projected_row_group_bytes;
    raw.max_owned_selected_compressed_bytes = resource_limits.max_owned_selected_compressed_bytes;
    raw.max_owned_decoded_materialized_bytes = resource_limits.max_owned_decoded_materialized_bytes;
    raw
}

pub(super) fn raw_snapshot_export_options(
    options: SnapshotExportOptions,
) -> sys::ArcadiaTioOcbSnapshotExportOptions {
    let mut raw = sys::ArcadiaTioOcbSnapshotExportOptions {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbSnapshotExportOptions>(),
        validation: options.validation.to_raw(),
        reserved: [0; 4],
    };
    unsafe { sys::arcadia_tio_ocb_snapshot_export_options_init(&mut raw) };
    raw.validation = options.validation.to_raw();
    raw
}

pub(super) fn raw_compact_l2_physical_v2_artifact_certification_options(
    options: &CompactL2PhysicalV2ArtifactCertificationOptions,
    expected_legacy_payload_hash_fnv1a64: *const c_char,
) -> sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationOptions {
    let mut raw = sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationOptions {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<
            sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationOptions,
        >(),
        has_expected_row_count: u8::from(options.expected_row_count.is_some()),
        has_expected_trading_day: u8::from(options.expected_trading_day.is_some()),
        has_expected_channel_id: u8::from(options.expected_channel_id.is_some()),
        has_expected_first_biz_index: u8::from(options.expected_first_biz_index.is_some()),
        has_expected_last_biz_index: u8::from(options.expected_last_biz_index.is_some()),
        verify_scalar_continuity: u8::from(options.verify_scalar_continuity),
        verify_legacy_reconstruction: u8::from(options.verify_legacy_reconstruction),
        has_max_rows: u8::from(options.max_rows.is_some()),
        expected_row_count: options.expected_row_count.unwrap_or(0),
        expected_trading_day: options.expected_trading_day.unwrap_or(0),
        expected_channel_id: options.expected_channel_id.unwrap_or(0),
        expected_first_biz_index: options.expected_first_biz_index.unwrap_or(0),
        expected_last_biz_index: options.expected_last_biz_index.unwrap_or(0),
        max_rows: options.max_rows.unwrap_or(0),
        read_threads: options.read_threads,
        max_in_flight_row_groups: options.max_in_flight_row_groups,
        expected_legacy_payload_hash_fnv1a64,
        reserved: [0; 4],
    };
    unsafe {
        sys::arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_options_init(&mut raw)
    };
    raw.has_expected_row_count = u8::from(options.expected_row_count.is_some());
    raw.has_expected_trading_day = u8::from(options.expected_trading_day.is_some());
    raw.has_expected_channel_id = u8::from(options.expected_channel_id.is_some());
    raw.has_expected_first_biz_index = u8::from(options.expected_first_biz_index.is_some());
    raw.has_expected_last_biz_index = u8::from(options.expected_last_biz_index.is_some());
    raw.verify_scalar_continuity = u8::from(options.verify_scalar_continuity);
    raw.verify_legacy_reconstruction = u8::from(options.verify_legacy_reconstruction);
    raw.has_max_rows = u8::from(options.max_rows.is_some());
    raw.expected_row_count = options.expected_row_count.unwrap_or(0);
    raw.expected_trading_day = options.expected_trading_day.unwrap_or(0);
    raw.expected_channel_id = options.expected_channel_id.unwrap_or(0);
    raw.expected_first_biz_index = options.expected_first_biz_index.unwrap_or(0);
    raw.expected_last_biz_index = options.expected_last_biz_index.unwrap_or(0);
    raw.max_rows = options.max_rows.unwrap_or(0);
    raw.read_threads = options.read_threads;
    raw.max_in_flight_row_groups = options.max_in_flight_row_groups;
    raw.expected_legacy_payload_hash_fnv1a64 = expected_legacy_payload_hash_fnv1a64;
    raw
}

pub(super) fn empty_metadata() -> sys::ArcadiaTioOcbMetadata {
    sys::ArcadiaTioOcbMetadata {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbMetadata>(),
        format_name: ptr::null_mut(),
        appendable: 0,
        root_generation: 0,
        has_previous_root_generation: 0,
        previous_root_generation: 0,
        row_count: 0,
        row_group_count: 0,
        column_chunk_count: 0,
        columns: ptr::null_mut(),
        columns_len: 0,
        dictionaries: ptr::null_mut(),
        dictionaries_len: 0,
        ordering_keys: ptr::null_mut(),
        ordering_keys_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_dictionary_values() -> sys::ArcadiaTioOcbDictionaryValues {
    sys::ArcadiaTioOcbDictionaryValues {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbDictionaryValues>(),
        dictionary_id: 0,
        name: ptr::null_mut(),
        value_kind: sys::ARCADIA_TIO_OCB_DICTIONARY_VALUE_KIND_UTF8,
        fixed_width: 0,
        string_values: ptr::null_mut(),
        string_values_len: 0,
        byte_values: ptr::null_mut(),
        byte_values_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_write_report() -> sys::ArcadiaTioOcbWriteReport {
    let mut raw = sys::ArcadiaTioOcbWriteReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbWriteReport>(),
        requested_write_threads: 0,
        effective_write_threads: 0,
        row_count: 0,
        row_group_count: 0,
        column_count: 0,
        dictionary_count: 0,
        dictionary_coded_column_count: 0,
        column_chunk_count: 0,
        stat_count: 0,
        payload_bytes: 0,
        validity_bytes: 0,
        row_group_object_bytes: 0,
        file_bytes: 0,
        tail_bytes: 0,
        root_generation: 0,
        previous_root_generation: 0,
        parallel_batches: 0,
        worker_count: 0,
        timings: sys::ArcadiaTioOcbWritePhaseTimings {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbWritePhaseTimings>(),
            to_internal_ns: 0,
            validate_spec_ns: 0,
            validate_dictionary_codes_ns: 0,
            validate_ordering_ns: 0,
            append_base_read_ns: 0,
            append_base_validate_ns: 0,
            row_group_encode_ns: 0,
            row_group_merge_ns: 0,
            metadata_encode_ns: 0,
            file_write_ns: 0,
            sync_data_ns: 0,
            commit_validate_ns: 0,
            slot_publish_ns: 0,
            sync_all_ns: 0,
            rename_ns: 0,
            parent_sync_ns: 0,
            reserved: [0; 4],
        },
        reserved: [0; 4],
    };
    unsafe { sys::arcadia_tio_ocb_write_report_init(&mut raw) };
    raw
}

pub(super) fn empty_compact_l2_physical_v2_artifact_certification_report()
-> sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport {
    let mut raw = sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport>(
        ),
        row_count: 0,
        row_group_count: 0,
        required_column_count: 0,
        selected_column_chunk_count: 0,
        selected_compressed_bytes: 0,
        selected_uncompressed_bytes: 0,
        first_biz_index: 0,
        last_biz_index: 0,
        min_receive_nano: 0,
        max_receive_nano: 0,
        order_record_count: 0,
        trade_record_count: 0,
        legacy_payload_hash_fnv1a64: ptr::null_mut(),
        has_first_biz_index: 0,
        has_last_biz_index: 0,
        has_min_receive_nano: 0,
        has_max_receive_nano: 0,
        has_order_record_count: 0,
        has_trade_record_count: 0,
        has_legacy_payload_hash_fnv1a64: 0,
        legacy_payload_hash_verified: 0,
        certified: 0,
        path_redacted: 1,
        writes_transformed_artifacts: 0,
        reserved: [0; 4],
    };
    unsafe {
        sys::arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_report_init(&mut raw)
    };
    raw
}

pub(super) fn empty_maintenance_report() -> sys::ArcadiaTioOcbMaintenanceReport {
    sys::ArcadiaTioOcbMaintenanceReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbMaintenanceReport>(),
        path: ptr::null_mut(),
        status: sys::ARCADIA_TIO_OCB_HEALTH_STATUS_UNKNOWN,
        has_file_bytes: 0,
        file_bytes: 0,
        has_selected_root_generation: 0,
        selected_root_generation: 0,
        has_previous_root_generation: 0,
        previous_root_generation: 0,
        has_selected_slot_id: 0,
        selected_slot_id: 0,
        has_selected_root_end_offset: 0,
        selected_root_end_offset: 0,
        has_selected_snapshot_end_offset: 0,
        selected_snapshot_end_offset: 0,
        has_orphan_tail_bytes: 0,
        orphan_tail_bytes: 0,
        cleanup_recommended: 0,
        root_candidate_rejection_observed: 0,
        rejected_root_candidate_count: 0,
        rejected_root_candidates: ptr::null_mut(),
        rejected_root_candidates_len: 0,
        issues: ptr::null_mut(),
        issues_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_cleanup_report() -> sys::ArcadiaTioOcbCleanupReport {
    sys::ArcadiaTioOcbCleanupReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbCleanupReport>(),
        path: ptr::null_mut(),
        before_file_bytes: 0,
        after_file_bytes: 0,
        selected_root_generation: 0,
        has_previous_root_generation: 0,
        previous_root_generation: 0,
        selected_slot_id: 0,
        selected_root_end_offset: 0,
        selected_snapshot_end_offset: 0,
        orphan_tail_bytes_before: 0,
        orphan_tail_bytes_after: 0,
        bytes_removed: 0,
        truncated: 0,
        issues: ptr::null_mut(),
        issues_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_snapshot_export_report() -> sys::ArcadiaTioOcbSnapshotExportReport {
    sys::ArcadiaTioOcbSnapshotExportReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbSnapshotExportReport>(),
        source_path: ptr::null_mut(),
        destination_path: ptr::null_mut(),
        validation: sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH,
        source_file_bytes: 0,
        destination_file_bytes: 0,
        bytes_copied: 0,
        orphan_tail_bytes_excluded: 0,
        root_generation: 0,
        has_previous_root_generation: 0,
        previous_root_generation: 0,
        row_count: 0,
        row_group_count: 0,
        fingerprint_algorithm: ptr::null_mut(),
        schema_fingerprint: ptr::null_mut(),
        dictionaries_fingerprint: ptr::null_mut(),
        ordering_fingerprint: ptr::null_mut(),
        combined_fingerprint: ptr::null_mut(),
        reserved: [0; 4],
    }
}

pub(super) fn empty_manifest() -> sys::ArcadiaTioOcbManifest {
    sys::ArcadiaTioOcbManifest {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbManifest>(),
        schema: ptr::null_mut(),
        generated_by: sys::ArcadiaTioOcbManifestTool {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestTool>(),
            name: ptr::null_mut(),
            version_text: ptr::null_mut(),
            generated_at_unix_seconds: 0,
            reserved: [0; 4],
        },
        entries: ptr::null_mut(),
        entries_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_manifest_validation_report() -> sys::ArcadiaTioOcbManifestValidationReport {
    sys::ArcadiaTioOcbManifestValidationReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbManifestValidationReport>(),
        status: sys::ARCADIA_TIO_OCB_COMPATIBILITY_STATUS_UNKNOWN,
        validation: sys::ARCADIA_TIO_OCB_OPEN_VALIDATION_METADATA_GRAPH,
        entries_checked: 0,
        issues: ptr::null_mut(),
        issues_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_read_report() -> sys::ArcadiaTioOcbReadReport {
    sys::ArcadiaTioOcbReadReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadReport>(),
        requested_threads: 0,
        effective_threads: 0,
        selected_row_groups: 0,
        pruned_row_groups: 0,
        selected_column_chunks: 0,
        fallback_reason: ptr::null_mut(),
        reserved: [0; 4],
    }
}

pub(super) fn raw_read_cursor_options(
    options: ReadCursorOptions,
) -> sys::ArcadiaTioOcbReadCursorOptions {
    sys::ArcadiaTioOcbReadCursorOptions {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadCursorOptions>(),
        max_in_flight_row_groups: options.max_in_flight_row_groups,
        ordered: u8::from(options.ordered),
        reserved: [0; 8],
    }
}

pub(super) fn empty_read_cursor_report() -> sys::ArcadiaTioOcbReadCursorReport {
    sys::ArcadiaTioOcbReadCursorReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadCursorReport>(),
        base_report: empty_read_report(),
        batches_yielded: 0,
        rows_yielded: 0,
        cancelled: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_read_fill_report() -> sys::ArcadiaTioOcbReadFillReport {
    sys::ArcadiaTioOcbReadFillReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadFillReport>(),
        row_group_id: 0,
        base_row: 0,
        row_count: 0,
        columns_filled: 0,
        reserved: [0; 8],
    }
}

pub(super) fn empty_read_attribution() -> sys::ArcadiaTioOcbReadAttribution {
    sys::ArcadiaTioOcbReadAttribution {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadAttribution>(),
        plan_ns: 0,
        execute_wall_ns: 0,
        row_group_read_ns: 0,
        read_io_ns: 0,
        checksum_ns: 0,
        decompression_ns: 0,
        primitive_decode_ns: 0,
        has_native_to_c_copy_ns: 0,
        native_to_c_copy_ns: 0,
        has_wrapper_copy_ns: 0,
        wrapper_copy_ns: 0,
        bytes_read: 0,
        compressed_bytes: 0,
        uncompressed_bytes: 0,
        requested_threads: 0,
        effective_threads: 0,
        selected_row_groups: 0,
        pruned_row_groups: 0,
        selected_column_chunks: 0,
        fallback_reason: ptr::null_mut(),
        reserved: [0; 4],
    }
}

pub(super) fn empty_read_outcome() -> sys::ArcadiaTioOcbReadOutcome {
    sys::ArcadiaTioOcbReadOutcome {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbReadOutcome>(),
        batches: ptr::null_mut(),
        batches_len: 0,
        report: empty_read_report(),
        reserved: [0; 4],
    }
}

pub(super) fn empty_parallel_read_context() -> sys::ArcadiaTioOcbParallelReadContext {
    sys::ArcadiaTioOcbParallelReadContext {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbParallelReadContext>(),
        selected_row_group_ordinal: 0,
        row_group_id: 0,
        base_row: 0,
        row_end: 0,
        row_count: 0,
        worker_id: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_column_batch() -> sys::ArcadiaTioOcbColumnBatch {
    sys::ArcadiaTioOcbColumnBatch {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnBatch>(),
        row_group_id: 0,
        base_row: 0,
        row_count: 0,
        columns: ptr::null_mut(),
        columns_len: 0,
        reserved: [0; 4],
    }
}

pub(super) fn empty_parallel_read_result() -> sys::ArcadiaTioOcbParallelReadResult {
    sys::ArcadiaTioOcbParallelReadResult {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbParallelReadResult>(),
        context: empty_parallel_read_context(),
        batch: empty_column_batch(),
        reserved: [0; 4],
    }
}

pub(super) fn empty_parallel_read_report() -> sys::ArcadiaTioOcbParallelReadReport {
    sys::ArcadiaTioOcbParallelReadReport {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbParallelReadReport>(),
        cursor_report: empty_read_cursor_report(),
        attribution: empty_read_attribution(),
        requested_workers: 0,
        started_workers: 0,
        max_active_workers_observed: 0,
        row_groups_queued: 0,
        row_groups_completed: 0,
        row_groups_ordered_committed: 0,
        rows_completed: 0,
        rows_ordered_committed: 0,
        max_in_flight_row_groups_observed: 0,
        max_pending_results_observed: 0,
        max_pending_rows_observed: 0,
        capacity_wait_count: 0,
        capacity_wait_ns: 0,
        task_queue_full_wait_count: 0,
        task_queue_full_wait_ns: 0,
        result_queue_full_wait_count: 0,
        result_queue_full_wait_ns: 0,
        ordered_frontier_wait_count: 0,
        ordered_frontier_wait_ns: 0,
        caller_prepare_ns: 0,
        ordered_commit_ns: 0,
        ordered_terminal_completed: 0,
        worker_reports: ptr::null_mut(),
        worker_reports_len: 0,
        reserved: [0; 8],
    }
}

pub(super) fn empty_row_group_summaries() -> sys::ArcadiaTioOcbRowGroupSummaries {
    sys::ArcadiaTioOcbRowGroupSummaries {
        version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
        struct_size: mem::size_of::<sys::ArcadiaTioOcbRowGroupSummaries>(),
        row_groups: ptr::null_mut(),
        row_groups_len: 0,
        reserved: [0; 4],
    }
}

pub(super) struct MetadataGuard(pub(super) sys::ArcadiaTioOcbMetadata);
impl Drop for MetadataGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_metadata_free(&mut self.0) };
    }
}

pub(super) struct DictionaryValuesGuard(pub(super) sys::ArcadiaTioOcbDictionaryValues);
impl Drop for DictionaryValuesGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_dictionary_values_free(&mut self.0) };
    }
}

pub(super) struct ReadReportGuard(pub(super) sys::ArcadiaTioOcbReadReport);
impl Drop for ReadReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_read_report_free(&mut self.0) };
    }
}

pub(super) struct ReadAttributionGuard(pub(super) sys::ArcadiaTioOcbReadAttribution);
impl Drop for ReadAttributionGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_read_attribution_free(&mut self.0) };
    }
}

pub(super) struct ReadCursorReportGuard(pub(super) sys::ArcadiaTioOcbReadCursorReport);
impl Drop for ReadCursorReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_read_cursor_report_free(&mut self.0) };
    }
}

pub(super) struct ReadOutcomeGuard(pub(super) sys::ArcadiaTioOcbReadOutcome);
impl Drop for ReadOutcomeGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_read_outcome_free(&mut self.0) };
    }
}

pub(super) struct ParallelReadResultGuard(pub(super) sys::ArcadiaTioOcbParallelReadResult);
impl Drop for ParallelReadResultGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_parallel_read_result_free(&mut self.0) };
    }
}

pub(super) struct ParallelReadReportGuard(pub(super) sys::ArcadiaTioOcbParallelReadReport);
impl Drop for ParallelReadReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_parallel_read_report_free(&mut self.0) };
    }
}

pub(super) struct RowGroupSummariesGuard(pub(super) sys::ArcadiaTioOcbRowGroupSummaries);
impl Drop for RowGroupSummariesGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_row_group_summaries_free(&mut self.0) };
    }
}

pub(super) struct SnapshotExportReportGuard(pub(super) sys::ArcadiaTioOcbSnapshotExportReport);
impl Drop for SnapshotExportReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_snapshot_export_report_free(&mut self.0) };
    }
}

pub(super) struct CompactL2PhysicalV2ArtifactCertificationReportGuard(
    pub(super) sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport,
);
impl Drop for CompactL2PhysicalV2ArtifactCertificationReportGuard {
    fn drop(&mut self) {
        unsafe {
            sys::arcadia_tio_ocb_compact_l2_physical_v2_artifact_certification_report_free(
                &mut self.0,
            )
        };
    }
}

pub(super) struct MaintenanceReportGuard(pub(super) sys::ArcadiaTioOcbMaintenanceReport);
impl Drop for MaintenanceReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_maintenance_report_free(&mut self.0) };
    }
}

pub(super) struct CleanupReportGuard(pub(super) sys::ArcadiaTioOcbCleanupReport);
impl Drop for CleanupReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_cleanup_report_free(&mut self.0) };
    }
}

pub(super) struct ManifestGuard(pub(super) sys::ArcadiaTioOcbManifest);
impl Drop for ManifestGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_manifest_free(&mut self.0) };
    }
}

pub(super) struct ManifestValidationReportGuard(
    pub(super) sys::ArcadiaTioOcbManifestValidationReport,
);
impl Drop for ManifestValidationReportGuard {
    fn drop(&mut self) {
        unsafe { sys::arcadia_tio_ocb_manifest_validation_report_free(&mut self.0) };
    }
}

pub(super) struct VisitCallback<'a, F>
where
    F: FnMut(ColumnBatch) -> OcbResult<VisitControl>,
{
    pub(super) visitor: &'a mut F,
}

pub(super) unsafe extern "C" fn visit_trampoline<F>(
    user: *mut c_void,
    batch: *const sys::ArcadiaTioOcbColumnBatch,
    out_continue: *mut u8,
) -> sys::ArcadiaTioErrorCode
where
    F: FnMut(ColumnBatch) -> OcbResult<VisitControl>,
{
    if user.is_null() || batch.is_null() || out_continue.is_null() {
        return sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT;
    }
    let result = catch_unwind(AssertUnwindSafe(|| {
        let callback = unsafe { &mut *user.cast::<VisitCallback<'_, F>>() };
        let batch = unsafe { column_batch_from_raw(&*batch) }?;
        match (callback.visitor)(batch)? {
            VisitControl::Continue => {
                unsafe { ptr::write(out_continue, 1) };
            }
            VisitControl::Stop => {
                unsafe { ptr::write(out_continue, 0) };
            }
        }
        Ok::<(), OcbError>(())
    }));
    match result {
        Ok(Ok(())) => sys::ARCADIA_TIO_ERROR_OK,
        Ok(Err(err)) => err.code().as_raw(),
        Err(_) => sys::ARCADIA_TIO_ERROR_INVALID_ARGUMENT,
    }
}

pub(super) fn duration_to_ns(duration: std::time::Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

pub(super) fn ensure_plan_belongs_to_file(
    raw_file: NonNull<sys::ArcadiaTioOcbFile>,
    plan: &ReadPlan<'_>,
) -> OcbResult<()> {
    if plan.file_raw == raw_file {
        Ok(())
    } else {
        Err(OcbError::invalid_input(
            "OCB read plan belongs to a different file handle",
        ))
    }
}

pub(super) fn read_plan_report(
    raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
) -> OcbResult<ReadReport> {
    let mut raw_report = empty_read_report();
    let status =
        unsafe { sys::arcadia_tio_ocb_read_plan_report(raw_plan.as_ptr(), &mut raw_report) };
    let guard = ReadReportGuard(raw_report);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB read_plan_report failed"));
    }
    read_report_from_raw(&guard.0)
}

pub(super) fn read_plan_projected_column_ids(
    raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
) -> OcbResult<Vec<u32>> {
    read_plan_ids(
        raw_plan,
        sys::arcadia_tio_ocb_read_plan_projected_column_ids,
        "OCB read_plan_projected_column_ids failed",
    )
}

pub(super) fn read_plan_row_group_ids(
    raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
) -> OcbResult<Vec<u32>> {
    read_plan_ids(
        raw_plan,
        sys::arcadia_tio_ocb_read_plan_row_group_ids,
        "OCB read_plan_row_group_ids failed",
    )
}

pub(super) fn read_plan_ids(
    raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
    f: unsafe extern "C" fn(
        *const sys::ArcadiaTioOcbReadPlan,
        *mut u32,
        usize,
        *mut usize,
    ) -> sys::ArcadiaTioErrorCode,
    context: &str,
) -> OcbResult<Vec<u32>> {
    let mut required = 0usize;
    let status = unsafe { f(raw_plan.as_ptr(), ptr::null_mut(), 0, &mut required) };
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last(context));
    }
    let mut ids = vec![0u32; required];
    let status = unsafe {
        f(
            raw_plan.as_ptr(),
            ids.as_mut_ptr(),
            ids.len(),
            &mut required,
        )
    };
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last(context));
    }
    ids.truncate(required);
    Ok(ids)
}

pub(super) fn read_batches_from_plan(
    raw_file: NonNull<sys::ArcadiaTioOcbFile>,
    raw_plan: NonNull<sys::ArcadiaTioOcbReadPlan>,
    row_group_ids: Option<&[u32]>,
) -> OcbResult<ReadOutcome> {
    let mut raw_outcome = empty_read_outcome();
    let (ids_ptr, ids_len) = match row_group_ids {
        Some(ids) => (ids.as_ptr(), ids.len()),
        None => (ptr::null(), 0),
    };
    let status = unsafe {
        sys::arcadia_tio_ocb_read_batches_from_plan(
            raw_file.as_ptr(),
            raw_plan.as_ptr(),
            ids_ptr,
            ids_len,
            &mut raw_outcome,
        )
    };
    let guard = ReadOutcomeGuard(raw_outcome);
    if status != sys::ARCADIA_TIO_ERROR_OK {
        return Err(OcbError::last("OCB read_batches_from_plan failed"));
    }
    unsafe { read_outcome_from_raw(&guard.0) }
}

pub(super) unsafe fn metadata_from_raw(raw: &sys::ArcadiaTioOcbMetadata) -> OcbResult<Metadata> {
    let columns = unsafe { raw_slice(raw.columns, raw.columns_len, "OCB metadata columns") }?
        .iter()
        .map(|column| {
            let physical_type = PhysicalType::from_raw_with_width(column.physical_type, unsafe {
                sys::arcadia_tio_ocb_column_descriptor_fixed_binary_width(column)
            });
            if matches!(physical_type, PhysicalType::FixedBinary { width: 0 }) {
                return Err(OcbError::invalid_input(
                    "OCB fixed-binary column descriptor returned zero width",
                ));
            }
            Ok(ColumnDescriptor {
                id: column.id,
                name: raw_string(column.name.cast(), "OCB column name")?,
                physical_type,
                logical_kind: LogicalKind::from_raw(column.logical_kind),
                dictionary_id: (column.has_dictionary_id != 0).then_some(column.dictionary_id),
                scale: column.scale,
                nullable: column.nullable != 0,
            })
        })
        .collect::<OcbResult<Vec<_>>>()?;
    let dictionaries = unsafe {
        raw_slice(
            raw.dictionaries,
            raw.dictionaries_len,
            "OCB metadata dictionaries",
        )
    }?
    .iter()
    .map(|dictionary| {
        Ok(DictionaryDescriptor {
            dictionary_id: dictionary.dictionary_id,
            name: raw_string(dictionary.name.cast(), "OCB dictionary name")?,
            code_physical_type: PhysicalType::from_raw(dictionary.code_physical_type),
            value_kind: DictionaryValueKind::from_raw(dictionary.value_kind),
            entry_count: dictionary.entry_count,
        })
    })
    .collect::<OcbResult<Vec<_>>>()?;
    let ordering_keys = unsafe {
        raw_slice(
            raw.ordering_keys,
            raw.ordering_keys_len,
            "OCB metadata ordering keys",
        )
    }?
    .iter()
    .map(|key| {
        Ok(OrderingKey {
            column_id: key.column_id,
            column_name: raw_string(key.column_name.cast(), "OCB ordering-key name")?,
            direction: OrderingDirection::from_raw(key.direction),
            null_order: NullOrder::from_raw(key.null_order),
        })
    })
    .collect::<OcbResult<Vec<_>>>()?;
    Ok(Metadata {
        format_name: raw_string(raw.format_name.cast(), "OCB format name")?,
        appendable: raw.appendable != 0,
        root_generation: raw.root_generation,
        previous_root_generation: (raw.has_previous_root_generation != 0)
            .then_some(raw.previous_root_generation),
        row_count: raw.row_count,
        row_group_count: raw.row_group_count,
        column_chunk_count: raw.column_chunk_count,
        columns,
        dictionaries,
        ordering_keys,
    })
}

pub(super) unsafe fn dictionary_values_from_raw(
    raw: &sys::ArcadiaTioOcbDictionaryValues,
) -> OcbResult<DictionaryValues> {
    let strings = unsafe {
        raw_string_array(
            raw.string_values,
            raw.string_values_len,
            "OCB dictionary string values",
        )
    }?;
    let bytes = unsafe {
        raw_byte_slices(
            raw.byte_values,
            raw.byte_values_len,
            "OCB dictionary byte values",
        )
    }?;
    let values = match DictionaryValueKind::from_raw(raw.value_kind) {
        DictionaryValueKind::Utf8 => {
            if !bytes.is_empty() {
                return Err(OcbError::invalid_input(
                    "OCB UTF-8 dictionary returned unexpected byte values",
                ));
            }
            DecodedDictionaryValues::Utf8(strings)
        }
        DictionaryValueKind::Bytes => {
            if !strings.is_empty() {
                return Err(OcbError::invalid_input(
                    "OCB bytes dictionary returned unexpected string values",
                ));
            }
            DecodedDictionaryValues::Bytes(bytes)
        }
        DictionaryValueKind::FixedBytes => {
            if raw.fixed_width == 0 {
                return Err(OcbError::invalid_input(
                    "OCB fixed-bytes dictionary returned zero width",
                ));
            }
            if !strings.is_empty()
                || bytes
                    .iter()
                    .any(|value| value.len() != raw.fixed_width as usize)
            {
                return Err(OcbError::invalid_input(
                    "OCB fixed-bytes dictionary values do not match fixed width",
                ));
            }
            DecodedDictionaryValues::FixedBytes {
                fixed_width: raw.fixed_width,
                values: bytes,
            }
        }
        DictionaryValueKind::EnumLabels => {
            if !bytes.is_empty() {
                return Err(OcbError::invalid_input(
                    "OCB enum-label dictionary returned unexpected byte values",
                ));
            }
            DecodedDictionaryValues::EnumLabels(strings)
        }
        DictionaryValueKind::Unknown(raw_kind) => DecodedDictionaryValues::Unknown {
            raw_kind,
            strings,
            bytes,
        },
    };
    Ok(DictionaryValues {
        dictionary_id: raw.dictionary_id,
        name: raw_string(raw.name.cast(), "OCB dictionary name")?,
        values,
    })
}

pub(super) fn write_report_from_raw(raw: &sys::ArcadiaTioOcbWriteReport) -> WriteReport {
    WriteReport {
        requested_write_threads: raw.requested_write_threads,
        effective_write_threads: raw.effective_write_threads,
        row_count: raw.row_count,
        row_group_count: raw.row_group_count,
        column_count: raw.column_count,
        dictionary_count: raw.dictionary_count,
        dictionary_coded_column_count: raw.dictionary_coded_column_count,
        column_chunk_count: raw.column_chunk_count,
        stat_count: raw.stat_count,
        payload_bytes: raw.payload_bytes,
        validity_bytes: raw.validity_bytes,
        row_group_object_bytes: raw.row_group_object_bytes,
        file_bytes: raw.file_bytes,
        tail_bytes: raw.tail_bytes,
        root_generation: raw.root_generation,
        previous_root_generation: raw.previous_root_generation,
        parallel_batches: raw.parallel_batches,
        worker_count: raw.worker_count,
        timings: WritePhaseTimings {
            to_internal_ns: raw.timings.to_internal_ns,
            validate_spec_ns: raw.timings.validate_spec_ns,
            validate_dictionary_codes_ns: raw.timings.validate_dictionary_codes_ns,
            validate_ordering_ns: raw.timings.validate_ordering_ns,
            append_base_read_ns: raw.timings.append_base_read_ns,
            append_base_validate_ns: raw.timings.append_base_validate_ns,
            row_group_encode_ns: raw.timings.row_group_encode_ns,
            row_group_merge_ns: raw.timings.row_group_merge_ns,
            metadata_encode_ns: raw.timings.metadata_encode_ns,
            file_write_ns: raw.timings.file_write_ns,
            sync_data_ns: raw.timings.sync_data_ns,
            commit_validate_ns: raw.timings.commit_validate_ns,
            slot_publish_ns: raw.timings.slot_publish_ns,
            sync_all_ns: raw.timings.sync_all_ns,
            rename_ns: raw.timings.rename_ns,
            parent_sync_ns: raw.timings.parent_sync_ns,
        },
    }
}

pub(super) unsafe fn compact_l2_physical_v2_artifact_certification_report_from_raw(
    raw: &sys::ArcadiaTioOcbCompactL2PhysicalV2ArtifactCertificationReport,
) -> OcbResult<CompactL2PhysicalV2ArtifactCertificationReport> {
    Ok(CompactL2PhysicalV2ArtifactCertificationReport {
        row_count: raw.row_count,
        row_group_count: raw.row_group_count,
        required_column_count: raw.required_column_count,
        selected_column_chunk_count: raw.selected_column_chunk_count,
        selected_compressed_bytes: raw.selected_compressed_bytes,
        selected_uncompressed_bytes: raw.selected_uncompressed_bytes,
        first_biz_index: (raw.has_first_biz_index != 0).then_some(raw.first_biz_index),
        last_biz_index: (raw.has_last_biz_index != 0).then_some(raw.last_biz_index),
        min_receive_nano: (raw.has_min_receive_nano != 0).then_some(raw.min_receive_nano),
        max_receive_nano: (raw.has_max_receive_nano != 0).then_some(raw.max_receive_nano),
        order_record_count: (raw.has_order_record_count != 0).then_some(raw.order_record_count),
        trade_record_count: (raw.has_trade_record_count != 0).then_some(raw.trade_record_count),
        legacy_payload_hash_fnv1a64: if raw.has_legacy_payload_hash_fnv1a64 != 0 {
            Some(raw_string(
                raw.legacy_payload_hash_fnv1a64.cast(),
                "OCB legacy payload hash",
            )?)
        } else {
            None
        },
        legacy_payload_hash_verified: raw.legacy_payload_hash_verified != 0,
        certified: raw.certified != 0,
        path_redacted: raw.path_redacted != 0,
        writes_transformed_artifacts: raw.writes_transformed_artifacts != 0,
    })
}

pub(super) unsafe fn row_group_summaries_from_raw(
    raw: &sys::ArcadiaTioOcbRowGroupSummaries,
) -> OcbResult<Vec<RowGroupSummary>> {
    unsafe {
        raw_slice(
            raw.row_groups,
            raw.row_groups_len,
            "OCB row-group summaries",
        )
    }?
    .iter()
    .map(|summary| unsafe { row_group_summary_from_raw(summary) })
    .collect()
}

pub(super) unsafe fn row_group_summary_from_raw(
    raw: &sys::ArcadiaTioOcbRowGroupSummary,
) -> OcbResult<RowGroupSummary> {
    Ok(RowGroupSummary {
        row_group_id: raw.row_group_id,
        base_row: raw.base_row,
        row_count: raw.row_count,
        first_key_tuple_ref: (raw.has_first_key_tuple_ref != 0)
            .then(|| body_ref_summary_from_raw(&raw.first_key_tuple_ref)),
        last_key_tuple_ref: (raw.has_last_key_tuple_ref != 0)
            .then(|| body_ref_summary_from_raw(&raw.last_key_tuple_ref)),
        chunks: unsafe { raw_slice(raw.chunks, raw.chunks_len, "OCB row-group chunk summaries") }?
            .iter()
            .map(|chunk| unsafe { column_chunk_summary_from_raw(chunk) })
            .collect::<OcbResult<Vec<_>>>()?,
        stats: unsafe { raw_slice(raw.stats, raw.stats_len, "OCB row-group statistics") }?
            .iter()
            .map(|stats| unsafe { column_stats_summary_from_raw(stats) })
            .collect::<OcbResult<Vec<_>>>()?,
    })
}

pub(super) fn body_ref_summary_from_raw(raw: &sys::ArcadiaTioOcbBodyRefSummary) -> BodyRefSummary {
    BodyRefSummary {
        offset: raw.offset,
        length: raw.length,
        kind: BodyKind::from_raw(raw.kind),
        flags: raw.flags,
        checksum_kind: ChecksumKind::from_raw(raw.checksum_kind),
        checksum: raw.checksum,
    }
}

pub(super) unsafe fn column_chunk_summary_from_raw(
    raw: &sys::ArcadiaTioOcbColumnChunkSummary,
) -> OcbResult<ColumnChunkSummary> {
    let physical_type =
        PhysicalType::from_raw_with_width(raw.physical_type, raw.fixed_binary_width);
    if matches!(physical_type, PhysicalType::FixedBinary { width: 0 }) {
        return Err(OcbError::invalid_input(
            "OCB fixed-binary chunk summary returned zero width",
        ));
    }
    Ok(ColumnChunkSummary {
        row_group_id: raw.row_group_id,
        column_id: raw.column_id,
        column_name: raw_string(raw.column_name, "OCB chunk-summary column name")?,
        physical_type,
        logical_kind: LogicalKind::from_raw(raw.logical_kind),
        fixed_binary_width: (raw.fixed_binary_width != 0).then_some(raw.fixed_binary_width),
        codec: ColumnChunkSummaryCodec::from_raw(raw.codec),
        row_count: raw.row_count,
        compressed_bytes: raw.compressed_bytes,
        uncompressed_bytes: raw.uncompressed_bytes,
        value_ref: body_ref_summary_from_raw(&raw.value_ref),
        validity_ref: (raw.has_validity_ref != 0)
            .then(|| body_ref_summary_from_raw(&raw.validity_ref)),
    })
}

pub(super) unsafe fn column_stats_summary_from_raw(
    raw: &sys::ArcadiaTioOcbColumnStatsSummary,
) -> OcbResult<ColumnStatsSummary> {
    Ok(ColumnStatsSummary {
        row_group_id: raw.row_group_id,
        column_id: raw.column_id,
        column_name: raw_string(raw.column_name, "OCB statistics column name")?,
        physical_type: PhysicalType::from_raw(raw.physical_type),
        null_count: raw.null_count,
        min: predicate_value_from_raw(&raw.min)?,
        max: predicate_value_from_raw(&raw.max)?,
    })
}

pub(super) fn predicate_value_from_raw(
    raw: &sys::ArcadiaTioOcbPredicateValue,
) -> OcbResult<PredicateValue> {
    match raw.physical_type {
        sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32 => Ok(PredicateValue::I32(raw.i32_value)),
        sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I64 => Ok(PredicateValue::I64(raw.i64_value)),
        sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F32 => Ok(PredicateValue::F32(raw.f32_value)),
        sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_F64 => Ok(PredicateValue::F64(raw.f64_value)),
        other => Err(OcbError::invalid_input(format!(
            "unknown OCB predicate physical type {other}"
        ))),
    }
}

pub(super) unsafe fn read_outcome_from_raw(
    raw: &sys::ArcadiaTioOcbReadOutcome,
) -> OcbResult<ReadOutcome> {
    let batches = unsafe { raw_slice(raw.batches, raw.batches_len, "OCB read batches") }?
        .iter()
        .map(|batch| unsafe { column_batch_from_raw(batch) })
        .collect::<OcbResult<Vec<_>>>()?;
    if let Some(expected_columns) = batches.first().map(|batch| batch.columns.len())
        && batches
            .iter()
            .any(|batch| batch.columns.len() != expected_columns)
    {
        return Err(OcbError::invalid_input(
            "OCB read batches returned inconsistent column counts",
        ));
    }
    if raw.report.selected_row_groups != batches.len() {
        return Err(OcbError::invalid_input(format!(
            "OCB read report selected-row-group count {} does not match batch count {}",
            raw.report.selected_row_groups,
            batches.len()
        )));
    }
    let returned_column_chunks = batches.iter().try_fold(0usize, |total, batch| {
        total.checked_add(batch.columns.len()).ok_or_else(|| {
            OcbError::invalid_input("OCB returned column-chunk count overflows usize")
        })
    })?;
    if raw.report.selected_column_chunks != returned_column_chunks {
        return Err(OcbError::invalid_input(format!(
            "OCB read report selected-column-chunk count {} does not match returned column count {returned_column_chunks}",
            raw.report.selected_column_chunks
        )));
    }
    Ok(ReadOutcome {
        batches,
        report: read_report_from_raw(&raw.report)?,
    })
}

pub(super) fn read_report_from_raw(raw: &sys::ArcadiaTioOcbReadReport) -> OcbResult<ReadReport> {
    Ok(ReadReport {
        requested_threads: raw.requested_threads,
        effective_threads: raw.effective_threads,
        selected_row_groups: raw.selected_row_groups,
        pruned_row_groups: raw.pruned_row_groups,
        selected_column_chunks: raw.selected_column_chunks,
        fallback_reason: raw_optional_string(raw.fallback_reason.cast())?,
    })
}

pub(super) fn read_cursor_report_from_raw(
    raw: &sys::ArcadiaTioOcbReadCursorReport,
) -> OcbResult<ReadCursorReport> {
    Ok(ReadCursorReport {
        base_report: read_report_from_raw(&raw.base_report)?,
        batches_yielded: raw.batches_yielded,
        rows_yielded: raw.rows_yielded,
        cancelled: raw.cancelled != 0,
    })
}

pub(super) fn read_fill_report_from_raw(
    raw: &sys::ArcadiaTioOcbReadFillReport,
    raw_columns: &[sys::ArcadiaTioOcbColumnFillBuffer],
) -> OcbResult<ReadFillReport> {
    let row_count = usize::try_from(raw.row_count)
        .map_err(|_| OcbError::invalid_input("OCB fill row count does not fit usize"))?;
    if raw.columns_filled > raw_columns.len() {
        return Err(OcbError::invalid_input(format!(
            "OCB fill report columns_filled {} exceeds supplied column count {}",
            raw.columns_filled,
            raw_columns.len()
        )));
    }
    if raw_columns
        .iter()
        .take(raw.columns_filled)
        .any(|column| column.rows_filled > row_count)
    {
        return Err(OcbError::invalid_input(
            "OCB fill report rows_filled exceeds row-group row count",
        ));
    }
    Ok(ReadFillReport {
        row_group_id: raw.row_group_id,
        base_row: raw.base_row,
        row_count: raw.row_count,
        columns: raw_columns
            .iter()
            .take(raw.columns_filled)
            .map(|column| ColumnFillReport {
                column_id: column.column_id,
                rows_filled: column.rows_filled,
                validity_filled: column.validity_filled != 0,
            })
            .collect(),
    })
}

pub(super) fn read_attribution_from_raw(
    raw: &sys::ArcadiaTioOcbReadAttribution,
) -> OcbResult<ReadAttribution> {
    Ok(ReadAttribution {
        plan_ns: raw.plan_ns,
        execute_wall_ns: raw.execute_wall_ns,
        row_group_read_ns: raw.row_group_read_ns,
        read_io_ns: raw.read_io_ns,
        checksum_ns: raw.checksum_ns,
        decompression_ns: raw.decompression_ns,
        primitive_decode_ns: raw.primitive_decode_ns,
        native_to_c_copy_ns: (raw.has_native_to_c_copy_ns != 0).then_some(raw.native_to_c_copy_ns),
        wrapper_copy_ns: (raw.has_wrapper_copy_ns != 0).then_some(raw.wrapper_copy_ns),
        bytes_read: raw.bytes_read,
        compressed_bytes: raw.compressed_bytes,
        uncompressed_bytes: raw.uncompressed_bytes,
        requested_threads: raw.requested_threads,
        effective_threads: raw.effective_threads,
        selected_row_groups: raw.selected_row_groups,
        pruned_row_groups: raw.pruned_row_groups,
        selected_column_chunks: raw.selected_column_chunks,
        fallback_reason: raw_optional_string(raw.fallback_reason.cast())?,
    })
}

pub(super) unsafe fn parallel_read_report_from_raw(
    raw: &sys::ArcadiaTioOcbParallelReadReport,
) -> OcbResult<ParallelReadReport> {
    let worker_reports = unsafe {
        raw_slice(
            raw.worker_reports,
            raw.worker_reports_len,
            "OCB parallel worker reports",
        )
    }?
    .iter()
    .map(|worker| ParallelReadWorkerReport {
        worker_id: worker.worker_id,
        row_groups_completed: worker.row_groups_completed,
        rows_completed: worker.rows_completed,
        row_group_read_ns: worker.row_group_read_ns,
        caller_prepare_ns: worker.caller_prepare_ns,
    })
    .collect();
    Ok(ParallelReadReport {
        cursor_report: read_cursor_report_from_raw(&raw.cursor_report)?,
        attribution: read_attribution_from_raw(&raw.attribution)?,
        requested_workers: raw.requested_workers,
        started_workers: raw.started_workers,
        max_active_workers_observed: raw.max_active_workers_observed,
        row_groups_queued: raw.row_groups_queued,
        row_groups_completed: raw.row_groups_completed,
        row_groups_ordered_committed: raw.row_groups_ordered_committed,
        rows_completed: raw.rows_completed,
        rows_ordered_committed: raw.rows_ordered_committed,
        max_in_flight_row_groups_observed: raw.max_in_flight_row_groups_observed,
        max_pending_results_observed: raw.max_pending_results_observed,
        max_pending_rows_observed: raw.max_pending_rows_observed,
        capacity_wait_count: raw.capacity_wait_count,
        capacity_wait_ns: raw.capacity_wait_ns,
        task_queue_full_wait_count: raw.task_queue_full_wait_count,
        task_queue_full_wait_ns: raw.task_queue_full_wait_ns,
        result_queue_full_wait_count: raw.result_queue_full_wait_count,
        result_queue_full_wait_ns: raw.result_queue_full_wait_ns,
        ordered_frontier_wait_count: raw.ordered_frontier_wait_count,
        ordered_frontier_wait_ns: raw.ordered_frontier_wait_ns,
        caller_prepare_ns: raw.caller_prepare_ns,
        ordered_commit_ns: raw.ordered_commit_ns,
        ordered_terminal_completed: raw.ordered_terminal_completed != 0,
        worker_reports,
    })
}

pub(super) unsafe fn maintenance_report_from_raw(
    raw: &sys::ArcadiaTioOcbMaintenanceReport,
) -> OcbResult<MaintenanceReport> {
    Ok(MaintenanceReport {
        path: PathBuf::from(raw_string(raw.path.cast(), "OCB maintenance path")?),
        status: HealthStatus::from_raw(raw.status),
        file_bytes: (raw.has_file_bytes != 0).then_some(raw.file_bytes),
        selected_root_generation: (raw.has_selected_root_generation != 0)
            .then_some(raw.selected_root_generation),
        previous_root_generation: (raw.has_previous_root_generation != 0)
            .then_some(raw.previous_root_generation),
        selected_slot_id: (raw.has_selected_slot_id != 0).then_some(raw.selected_slot_id),
        selected_root_end_offset: (raw.has_selected_root_end_offset != 0)
            .then_some(raw.selected_root_end_offset),
        selected_snapshot_end_offset: (raw.has_selected_snapshot_end_offset != 0)
            .then_some(raw.selected_snapshot_end_offset),
        orphan_tail_bytes: (raw.has_orphan_tail_bytes != 0).then_some(raw.orphan_tail_bytes),
        cleanup_recommended: raw.cleanup_recommended != 0,
        root_candidate_rejection_observed: raw.root_candidate_rejection_observed != 0,
        rejected_root_candidate_count: raw.rejected_root_candidate_count,
        rejected_root_candidates: unsafe {
            root_candidate_diagnostics_from_raw(
                raw.rejected_root_candidates,
                raw.rejected_root_candidates_len,
            )
        }?,
        issues: unsafe { issues_from_raw(raw.issues, raw.issues_len) }?,
    })
}

pub(super) unsafe fn cleanup_report_from_raw(
    raw: &sys::ArcadiaTioOcbCleanupReport,
) -> OcbResult<CleanupReport> {
    Ok(CleanupReport {
        path: PathBuf::from(raw_string(raw.path.cast(), "OCB cleanup path")?),
        before_file_bytes: raw.before_file_bytes,
        after_file_bytes: raw.after_file_bytes,
        selected_root_generation: raw.selected_root_generation,
        previous_root_generation: (raw.has_previous_root_generation != 0)
            .then_some(raw.previous_root_generation),
        selected_slot_id: raw.selected_slot_id,
        selected_root_end_offset: raw.selected_root_end_offset,
        selected_snapshot_end_offset: raw.selected_snapshot_end_offset,
        orphan_tail_bytes_before: raw.orphan_tail_bytes_before,
        orphan_tail_bytes_after: raw.orphan_tail_bytes_after,
        bytes_removed: raw.bytes_removed,
        truncated: raw.truncated != 0,
        issues: unsafe { issues_from_raw(raw.issues, raw.issues_len) }?,
    })
}

pub(super) unsafe fn root_candidate_diagnostics_from_raw(
    ptr: *const sys::ArcadiaTioOcbRootCandidateDiagnostic,
    len: usize,
) -> OcbResult<Vec<RootCandidateDiagnostic>> {
    unsafe { raw_slice(ptr, len, "OCB rejected root candidates") }?
        .iter()
        .map(|diagnostic| {
            Ok(RootCandidateDiagnostic {
                slot_id: (diagnostic.has_slot_id != 0).then_some(diagnostic.slot_id),
                generation: (diagnostic.has_generation != 0).then_some(diagnostic.generation),
                issue: issue_from_raw(&diagnostic.issue)?,
            })
        })
        .collect()
}

pub(super) unsafe fn issues_from_raw(
    ptr: *const sys::ArcadiaTioOcbIssue,
    len: usize,
) -> OcbResult<Vec<Issue>> {
    unsafe { raw_slice(ptr, len, "OCB issues") }?
        .iter()
        .map(issue_from_raw)
        .collect()
}

pub(super) fn issue_from_raw(raw: &sys::ArcadiaTioOcbIssue) -> OcbResult<Issue> {
    Ok(Issue {
        code: raw_string(raw.code.cast(), "OCB issue code")?,
        field_path: raw_optional_string(raw.field_path.cast())?,
        message: raw_string(raw.message.cast(), "OCB issue message")?,
    })
}

pub(super) unsafe fn snapshot_export_report_from_raw(
    raw: &sys::ArcadiaTioOcbSnapshotExportReport,
) -> OcbResult<SnapshotExportReport> {
    Ok(SnapshotExportReport {
        source_path: PathBuf::from(raw_string(
            raw.source_path.cast(),
            "OCB snapshot source path",
        )?),
        destination_path: PathBuf::from(raw_string(
            raw.destination_path.cast(),
            "OCB snapshot destination path",
        )?),
        validation: OpenValidation::from_raw(raw.validation)?,
        source_file_bytes: raw.source_file_bytes,
        destination_file_bytes: raw.destination_file_bytes,
        bytes_copied: raw.bytes_copied,
        orphan_tail_bytes_excluded: raw.orphan_tail_bytes_excluded,
        root_generation: raw.root_generation,
        previous_root_generation: (raw.has_previous_root_generation != 0)
            .then_some(raw.previous_root_generation),
        row_count: raw.row_count,
        row_group_count: raw.row_group_count,
        fingerprints: SnapshotFingerprints {
            algorithm: raw_string(
                raw.fingerprint_algorithm.cast(),
                "OCB fingerprint algorithm",
            )?,
            schema: raw_string(raw.schema_fingerprint.cast(), "OCB schema fingerprint")?,
            dictionaries: raw_string(
                raw.dictionaries_fingerprint.cast(),
                "OCB dictionaries fingerprint",
            )?,
            ordering: raw_string(raw.ordering_fingerprint.cast(), "OCB ordering fingerprint")?,
            combined: raw_string(raw.combined_fingerprint.cast(), "OCB combined fingerprint")?,
        },
    })
}

pub(super) unsafe fn manifest_from_raw(raw: &sys::ArcadiaTioOcbManifest) -> OcbResult<Manifest> {
    let entries = unsafe { raw_slice(raw.entries, raw.entries_len, "OCB manifest entries") }?
        .iter()
        .map(|entry| unsafe { manifest_entry_from_raw(entry) })
        .collect::<OcbResult<Vec<_>>>()?;
    Ok(Manifest {
        schema: raw_string(raw.schema.cast(), "OCB manifest schema")?,
        generated_by: ManifestTool {
            name: raw_string(raw.generated_by.name.cast(), "OCB manifest tool name")?,
            version: raw_string(
                raw.generated_by.version_text.cast(),
                "OCB manifest tool version",
            )?,
            generated_at_unix_seconds: raw.generated_by.generated_at_unix_seconds,
        },
        entries,
    })
}

pub(super) unsafe fn manifest_entry_from_raw(
    raw: &sys::ArcadiaTioOcbManifestEntry,
) -> OcbResult<ManifestEntry> {
    Ok(ManifestEntry {
        path: raw_string(raw.path.cast(), "OCB manifest entry path")?,
        uri: raw_optional_string(raw.uri.cast())?,
        file_bytes: (raw.has_file_bytes != 0).then_some(raw.file_bytes),
        digest: if raw.has_digest != 0 {
            Some(manifest_digest_from_raw(&raw.digest)?)
        } else {
            None
        },
        root_generation: raw.root_generation,
        row_count: raw.row_count,
        row_group_count: raw.row_group_count,
        fingerprints: manifest_fingerprints_from_raw(&raw.fingerprints)?,
        validation: unsafe { manifest_entry_validation_from_raw(&raw.validation) }?,
    })
}

pub(super) fn manifest_digest_from_raw(
    raw: &sys::ArcadiaTioOcbManifestDigest,
) -> OcbResult<ManifestDigest> {
    Ok(ManifestDigest {
        algorithm: raw_string(raw.algorithm.cast(), "OCB manifest digest algorithm")?,
        digest: raw_string(raw.digest.cast(), "OCB manifest digest")?,
    })
}

pub(super) fn manifest_fingerprints_from_raw(
    raw: &sys::ArcadiaTioOcbManifestFingerprints,
) -> OcbResult<ManifestFingerprints> {
    Ok(ManifestFingerprints {
        algorithm: raw_string(raw.algorithm.cast(), "OCB manifest fingerprint algorithm")?,
        schema: raw_string(raw.schema.cast(), "OCB manifest schema fingerprint")?,
        dictionaries: raw_string(
            raw.dictionaries.cast(),
            "OCB manifest dictionaries fingerprint",
        )?,
        ordering: raw_string(raw.ordering.cast(), "OCB manifest ordering fingerprint")?,
        combined: raw_string(raw.combined.cast(), "OCB manifest combined fingerprint")?,
    })
}

pub(super) unsafe fn manifest_entry_validation_from_raw(
    raw: &sys::ArcadiaTioOcbManifestEntryValidation,
) -> OcbResult<ManifestEntryValidation> {
    Ok(ManifestEntryValidation {
        mode: raw_string(raw.mode.cast(), "OCB manifest validation mode")?,
        status: raw_string(raw.status.cast(), "OCB manifest validation status")?,
        issues: unsafe { manifest_issues_from_raw(raw.issues, raw.issues_len) }?,
    })
}

pub(super) unsafe fn manifest_issues_from_raw(
    ptr: *const sys::ArcadiaTioOcbManifestIssue,
    len: usize,
) -> OcbResult<Vec<ManifestIssue>> {
    unsafe { raw_slice(ptr, len, "OCB manifest issues") }?
        .iter()
        .map(|issue| {
            Ok(ManifestIssue {
                code: raw_string(issue.code.cast(), "OCB manifest issue code")?,
                field_path: raw_optional_string(issue.field_path.cast())?,
                message: raw_string(issue.message.cast(), "OCB manifest issue message")?,
            })
        })
        .collect()
}

pub(super) unsafe fn manifest_validation_report_from_raw(
    raw: &sys::ArcadiaTioOcbManifestValidationReport,
) -> OcbResult<ManifestValidationReport> {
    Ok(ManifestValidationReport {
        status: CompatibilityStatus::from_raw(raw.status),
        validation: OpenValidation::from_raw(raw.validation)?,
        entries_checked: raw.entries_checked,
        issues: unsafe { manifest_issues_from_raw(raw.issues, raw.issues_len) }?,
    })
}

pub(super) unsafe fn column_batch_from_raw(
    raw: &sys::ArcadiaTioOcbColumnBatch,
) -> OcbResult<ColumnBatch> {
    let row_count = usize::try_from(raw.row_count)
        .map_err(|_| OcbError::invalid_input("OCB batch row count does not fit usize"))?;
    let columns = unsafe { raw_slice(raw.columns, raw.columns_len, "OCB batch columns") }?
        .iter()
        .map(|column| unsafe { column_array_from_raw(column, raw.row_count) })
        .collect::<OcbResult<Vec<_>>>()?;
    if columns
        .iter()
        .any(|column| column.values.len() != row_count)
    {
        return Err(OcbError::invalid_input(
            "OCB column value count does not match batch row count",
        ));
    }
    Ok(ColumnBatch {
        row_group_id: raw.row_group_id,
        base_row: raw.base_row,
        row_count: raw.row_count,
        columns,
    })
}

pub(super) unsafe fn column_array_from_raw(
    raw: &sys::ArcadiaTioOcbColumnArray,
    expected_row_count: u64,
) -> OcbResult<ColumnArray> {
    let physical_type = PhysicalType::from_raw_with_width(raw.physical_type, unsafe {
        sys::arcadia_tio_ocb_column_array_fixed_binary_width(raw)
    });
    if matches!(physical_type, PhysicalType::FixedBinary { width: 0 }) {
        return Err(OcbError::invalid_input(
            "OCB fixed-binary column returned zero width",
        ));
    }
    if raw.values.physical_type != raw.physical_type {
        return Err(OcbError::invalid_input(
            "OCB column physical type does not match primitive values",
        ));
    }
    let values = unsafe { primitive_values_from_raw(&raw.values) }?;
    let expected_len = usize::try_from(expected_row_count)
        .map_err(|_| OcbError::invalid_input("OCB column row count does not fit usize"))?;
    if values.len() != expected_len {
        return Err(OcbError::invalid_input(format!(
            "OCB column value count {} does not match batch row count {expected_row_count}",
            values.len()
        )));
    }
    if let (
        PhysicalType::FixedBinary {
            width: column_width,
        },
        PrimitiveValues::FixedBinary {
            width: values_width,
            ..
        },
    ) = (physical_type, &values)
        && column_width != *values_width
    {
        return Err(OcbError::invalid_input(
            "OCB fixed-binary column width does not match primitive values width",
        ));
    }
    let validity = if raw.has_validity != 0 {
        if raw.validity.row_count != expected_row_count {
            return Err(OcbError::invalid_input(
                "OCB validity row count does not match batch row count",
            ));
        }
        let expected_validity_len = usize::try_from(expected_row_count.div_ceil(8))
            .map_err(|_| OcbError::invalid_input("OCB validity length does not fit usize"))?;
        if raw.validity.len != expected_validity_len {
            return Err(OcbError::invalid_input(format!(
                "OCB validity byte length {} does not match expected {expected_validity_len}",
                raw.validity.len
            )));
        }
        Some(ValidityBitmap {
            bytes: unsafe {
                raw_bytes(raw.validity.data, raw.validity.len, "OCB validity bitmap")
            }?,
            row_count: raw.validity.row_count,
        })
    } else {
        if raw.validity.len != 0 || raw.validity.row_count != 0 || !raw.validity.data.is_null() {
            return Err(OcbError::invalid_input(
                "OCB column returned validity payload while has_validity is false",
            ));
        }
        None
    };
    Ok(ColumnArray {
        column_id: raw.column_id,
        name: raw_string(raw.name.cast(), "OCB column-array name")?,
        physical_type,
        logical_kind: LogicalKind::from_raw(raw.logical_kind),
        dictionary_id: (raw.has_dictionary_id != 0).then_some(raw.dictionary_id),
        values,
        validity,
    })
}

pub(super) unsafe fn primitive_values_from_raw(
    raw: &sys::ArcadiaTioOcbPrimitiveValues,
) -> OcbResult<PrimitiveValues> {
    let fixed_binary_width = u32::try_from(raw.reserved[0]).map_err(|_| {
        OcbError::invalid_input("OCB fixed-binary width does not fit the public u32 contract")
    })?;
    match PhysicalType::from_raw_with_width(raw.physical_type, fixed_binary_width) {
        PhysicalType::I32 => Ok(PrimitiveValues::I32(unsafe {
            raw_typed(raw.data.cast(), raw.len, "OCB i32 primitive values")
        }?)),
        PhysicalType::I64 => Ok(PrimitiveValues::I64(unsafe {
            raw_typed(raw.data.cast(), raw.len, "OCB i64 primitive values")
        }?)),
        PhysicalType::F32 => Ok(PrimitiveValues::F32(unsafe {
            raw_typed(raw.data.cast(), raw.len, "OCB f32 primitive values")
        }?)),
        PhysicalType::F64 => Ok(PrimitiveValues::F64(unsafe {
            raw_typed(raw.data.cast(), raw.len, "OCB f64 primitive values")
        }?)),
        PhysicalType::FixedBinary { width } => {
            if width == 0 {
                return Err(OcbError::invalid_input(
                    "OCB fixed-binary primitive values returned zero width",
                ));
            }
            let byte_len = raw
                .len
                .checked_mul(width as usize)
                .ok_or_else(|| OcbError::invalid_input("OCB fixed-binary byte length overflows"))?;
            Ok(PrimitiveValues::FixedBinary {
                width,
                bytes: unsafe {
                    raw_bytes(
                        raw.data.cast(),
                        byte_len,
                        "OCB fixed-binary primitive values",
                    )
                }?,
            })
        }
        PhysicalType::Unknown(raw_type) => Err(OcbError::invalid_input(format!(
            "unknown OCB primitive physical type {raw_type}"
        ))),
    }
}

pub(super) unsafe fn raw_slice<'a, T>(
    ptr: *const T,
    len: usize,
    label: &str,
) -> OcbResult<&'a [T]> {
    unsafe { checked_slice(ptr, len, label) }.map_err(OcbError::from_tio_error)
}

pub(super) unsafe fn raw_typed<T: Copy>(
    ptr: *const T,
    len: usize,
    label: &str,
) -> OcbResult<Vec<T>> {
    unsafe { copy_checked_slice(ptr, len, label) }.map_err(OcbError::from_tio_error)
}

pub(super) fn raw_string(ptr: *const c_char, label: &str) -> OcbResult<String> {
    required_c_string(ptr, label).map_err(OcbError::from_tio_error)
}

pub(super) fn raw_optional_string(ptr: *const c_char) -> OcbResult<Option<String>> {
    optional_c_string(ptr).map_err(OcbError::from_tio_error)
}

pub(super) unsafe fn raw_string_array(
    ptr: *mut *mut c_char,
    len: usize,
    label: &str,
) -> OcbResult<Vec<String>> {
    unsafe { raw_slice(ptr.cast::<*mut c_char>(), len, label) }?
        .iter()
        .map(|value| raw_string((*value).cast(), "OCB required string-array entry"))
        .collect()
}

pub(super) unsafe fn raw_byte_slices(
    ptr: *mut sys::ArcadiaTioOcbByteSlice,
    len: usize,
    label: &str,
) -> OcbResult<Vec<Vec<u8>>> {
    unsafe { raw_slice(ptr, len, label) }?
        .iter()
        .map(|value| unsafe { raw_bytes(value.data, value.len, "OCB dictionary byte value") })
        .collect()
}

pub(super) unsafe fn raw_bytes(ptr: *const u8, len: usize, label: &str) -> OcbResult<Vec<u8>> {
    unsafe { raw_typed(ptr, len, label) }
}

#[cfg(test)]
mod conversion_tests {
    use super::*;

    fn raw_primitive_i32(data: *const i32, len: usize) -> sys::ArcadiaTioOcbPrimitiveValues {
        sys::ArcadiaTioOcbPrimitiveValues {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbPrimitiveValues>(),
            physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
            data: data.cast(),
            len,
            reserved: [0; 3],
        }
    }

    fn raw_column_i32(
        name: *mut c_char,
        data: *const i32,
        len: usize,
    ) -> sys::ArcadiaTioOcbColumnArray {
        sys::ArcadiaTioOcbColumnArray {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnArray>(),
            column_id: 0,
            name,
            physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_I32,
            logical_kind: sys::ARCADIA_TIO_OCB_LOGICAL_KIND_PLAIN,
            has_dictionary_id: 0,
            dictionary_id: 0,
            values: raw_primitive_i32(data, len),
            has_validity: 0,
            validity: sys::ArcadiaTioOcbValidityBitmap {
                version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
                struct_size: mem::size_of::<sys::ArcadiaTioOcbValidityBitmap>(),
                data: ptr::null(),
                len: 0,
                row_count: 0,
                reserved: [0; 3],
            },
            reserved: [0; 4],
        }
    }

    #[test]
    fn metadata_rejects_null_required_format_name() {
        let raw = empty_metadata();
        let result = unsafe { metadata_from_raw(&raw) };
        assert!(
            result.is_err(),
            "null required format name must fail closed"
        );
    }

    #[test]
    fn metadata_rejects_invalid_utf8_required_format_name() {
        let mut invalid_utf8 = [0xff_u8, 0];
        let mut raw = empty_metadata();
        raw.format_name = invalid_utf8.as_mut_ptr().cast();
        let result = unsafe { metadata_from_raw(&raw) };
        assert!(
            result.is_err(),
            "invalid UTF-8 required format name must fail closed"
        );
    }

    #[test]
    fn metadata_rejects_null_columns_with_nonzero_length() {
        let format_name = CString::new("OCB").expect("cstring");
        let mut raw = empty_metadata();
        raw.format_name = format_name.as_ptr().cast_mut();
        raw.columns_len = 1;
        let result = unsafe { metadata_from_raw(&raw) };
        assert!(result.is_err(), "null/nonzero columns must fail closed");
    }

    #[test]
    fn dictionary_values_reject_null_required_string_entry() {
        let name = CString::new("symbols").expect("cstring");
        let mut strings = [ptr::null_mut()];
        let mut raw = empty_dictionary_values();
        raw.name = name.as_ptr().cast_mut();
        raw.string_values = strings.as_mut_ptr();
        raw.string_values_len = strings.len();
        let result = unsafe { dictionary_values_from_raw(&raw) };
        assert!(
            result.is_err(),
            "null required dictionary value must fail closed"
        );
    }

    #[test]
    fn primitive_values_reject_null_data_with_nonzero_length() {
        let raw = raw_primitive_i32(ptr::null(), 1);
        let result = unsafe { primitive_values_from_raw(&raw) };
        assert!(
            result.is_err(),
            "null/nonzero primitive data must fail closed"
        );
    }

    #[test]
    fn column_batch_rejects_value_count_different_from_row_count() {
        let name = CString::new("price").expect("cstring");
        let values = [7i32];
        let mut column = raw_column_i32(name.as_ptr().cast_mut(), values.as_ptr(), values.len());
        let raw = sys::ArcadiaTioOcbColumnBatch {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnBatch>(),
            row_group_id: 0,
            base_row: 0,
            row_count: 2,
            columns: &mut column,
            columns_len: 1,
            reserved: [0; 4],
        };
        let result = unsafe { column_batch_from_raw(&raw) };
        assert!(result.is_err(), "column values must match batch row count");
    }

    #[test]
    fn column_batch_rejects_inconsistent_validity_bitmap() {
        let name = CString::new("price").expect("cstring");
        let values = [0i32; 9];
        let validity = [0xffu8];
        let mut column = raw_column_i32(name.as_ptr().cast_mut(), values.as_ptr(), values.len());
        column.has_validity = 1;
        column.validity.data = validity.as_ptr();
        column.validity.len = validity.len();
        column.validity.row_count = values.len() as u64;
        let raw = sys::ArcadiaTioOcbColumnBatch {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbColumnBatch>(),
            row_group_id: 0,
            base_row: 0,
            row_count: values.len() as u64,
            columns: &mut column,
            columns_len: 1,
            reserved: [0; 4],
        };
        let result = unsafe { column_batch_from_raw(&raw) };
        assert!(
            result.is_err(),
            "validity byte length must cover every row exactly"
        );
    }

    #[test]
    fn fixed_binary_values_reject_zero_width() {
        let byte = [0u8];
        let raw = sys::ArcadiaTioOcbPrimitiveValues {
            version: sys::ARCADIA_TIO_OCB_ABI_VERSION,
            struct_size: mem::size_of::<sys::ArcadiaTioOcbPrimitiveValues>(),
            physical_type: sys::ARCADIA_TIO_OCB_PHYSICAL_TYPE_FIXED_BINARY,
            data: byte.as_ptr().cast(),
            len: 1,
            reserved: [0; 3],
        };
        let result = unsafe { primitive_values_from_raw(&raw) };
        assert!(result.is_err(), "fixed-binary width zero must fail closed");
    }
}
