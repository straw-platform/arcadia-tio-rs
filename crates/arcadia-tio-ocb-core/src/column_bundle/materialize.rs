//! Schema resolution, reusable fills, projection, and primitive materialization.

use super::*;

pub(super) fn preflight_resolved_columns_materialized_bytes(
    metadata: &OcbMetadataV1,
) -> Result<u64> {
    let column_count = u64::try_from(metadata.schema.columns.len())
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB schema column count exceeds u64"))?;
    let descriptor_bytes = column_count
        .checked_mul(std::mem::size_of::<BundleColumn>() as u64)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB resolved column descriptor materialization size overflows",
        ))?;
    let mut seen_ids = try_column_bundle_hash_set_with_capacity(metadata.schema.columns.len())?;
    let mut seen_names = try_column_bundle_hash_set_with_capacity(metadata.schema.columns.len())?;
    let mut cloned_name_bytes = 0u64;
    for column in &metadata.schema.columns {
        if !seen_ids.insert(column.column_id) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column ids",
            ));
        }
        let name = metadata
            .string_table
            .strings
            .get(column.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column name string id is out of range",
            ))?;
        if !seen_names.insert(name.as_str()) {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB schema has duplicate column names",
            ));
        }
        column_physical_type_from_desc(column)?;
        let name_bytes = u64::try_from(name.len())
            .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB column name length exceeds u64"))?;
        cloned_name_bytes =
            cloned_name_bytes
                .checked_add(name_bytes)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB resolved column name materialization size overflows",
                ))?;
    }
    descriptor_bytes
        .checked_add(cloned_name_bytes)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB resolved column materialization size overflows",
        ))
}

pub(super) fn resolve_columns(metadata: &OcbMetadataV1) -> Result<Vec<BundleColumn>> {
    let mut columns = try_column_bundle_vec_with_capacity(metadata.schema.columns.len())?;
    for column in &metadata.schema.columns {
        let name = metadata
            .string_table
            .strings
            .get(column.name_string_id as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column name string id is out of range",
            ))?;
        columns.push(BundleColumn {
            id: column.column_id,
            name: clone_column_bundle_string_fallibly(name)?,
            physical_type: column_physical_type_from_desc(column)?,
            logical_kind: column.logical_kind.into(),
            dictionary_id: if column.dictionary_id == OCB_NULL_U32 {
                None
            } else {
                Some(column.dictionary_id)
            },
            scale: column.scale,
            nullable: column.nullability == OcbNullabilityV1::Nullable,
        });
    }
    columns.sort_by_key(|column| column.id);
    Ok(columns)
}

#[derive(Debug, Clone)]
pub(super) struct ResolvedColumnFill<'a> {
    buffer_index: usize,
    column: &'a BundleColumn,
    chunk: OcbColumnChunkDescV1,
}

pub(super) fn read_row_group_into(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    buffers: &mut [ColumnBundleColumnFillBuffer<'_>],
) -> Result<ColumnBundleReadFillReport> {
    if buffers.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read requires at least one column buffer",
        ));
    }
    let row_group =
        *metadata
            .row_group_by_id(row_group_id)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fill read references an unknown row group",
            ))?;
    let row_count = usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB fill read row count does not fit usize")
    })?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = try_column_bundle_hash_map_with_capacity(chunks.len())?;
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, *chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut by_id = try_column_bundle_hash_map_with_capacity(columns.len())?;
    by_id.extend(columns.iter().map(|column| (column.id, column)));
    let mut by_name = try_column_bundle_hash_map_with_capacity(columns.len())?;
    by_name.extend(columns.iter().map(|column| (column.name.as_str(), column)));
    let mut seen_columns = try_column_bundle_hash_set_with_capacity(buffers.len())?;
    let mut resolved = Vec::new();
    resolved
        .try_reserve_exact(buffers.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for (buffer_index, buffer) in buffers.iter().enumerate() {
        let Some(column) = resolve_fill_column(buffer, &by_id, &by_name)? else {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer must identify a column by name or id",
            ));
        };
        if !seen_columns.insert(column.id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill request contains duplicate column buffers",
            ));
        }
        if buffer.values.physical_type() != column.physical_type {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer dtype does not match column dtype",
            ));
        }
        buffer.values.validate_capacity(row_count)?;
        let chunk = *chunk_by_column
            .get(&column.id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        if chunk.physical_type != column.physical_type.ocb_physical_type() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk physical type does not match schema",
            ));
        }
        if chunk.row_count != row_group.row_count {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk row_count does not match row group",
            ));
        }
        if !column.nullable && !chunk.validity_ref.is_null() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        if !chunk.validity_ref.is_null() {
            if !buffer.allow_nulls {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer rejected nullable column chunk",
                ));
            }
            let Some(validity_bytes) = buffer.validity_bytes.as_deref() else {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage for nullable column chunk",
                ));
            };
            let expected_validity_bytes =
                usize::try_from(chunk.row_count.div_ceil(8)).map_err(|_| {
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB fill validity byte count does not fit usize",
                    )
                })?;
            if validity_bytes.len() < expected_validity_bytes {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer validity capacity is too small for row group",
                ));
            }
        }
        resolved.push(ResolvedColumnFill {
            buffer_index,
            column,
            chunk,
        });
    }

    let mut selected_column_ids = Vec::new();
    selected_column_ids
        .try_reserve_exact(resolved.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    selected_column_ids.extend(resolved.iter().map(|target| target.column.id));
    let footprint =
        selected_resource_footprint_for_row_group_desc(metadata, &row_group, &selected_column_ids)?;
    validate_footprint_totals(footprint, metadata.resource_limits)?;

    let mut reports = Vec::new();
    reports
        .try_reserve_exact(resolved.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for target in resolved {
        let report = fill_column_buffer(
            source,
            metadata,
            row_count,
            &target,
            &mut buffers[target.buffer_index],
        )?;
        reports.push(report);
    }
    Ok(ColumnBundleReadFillReport {
        row_group_id: row_group.row_group_id,
        base_row: row_group.base_row,
        row_count: row_group.row_count,
        columns: reports,
    })
}

pub(super) fn read_row_group_into_reusable(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    reusable: &mut ColumnBundleReusableBuffers,
) -> Result<ColumnBundleReadFillReport> {
    let row_count = row_count_for_row_group(metadata, row_group_id)?;
    reusable.prepare_for_rows(row_count)?;
    let mut fill_buffers = reusable.fill_buffers()?;
    read_row_group_into(source, metadata, columns, row_group_id, &mut fill_buffers)
}

pub(super) fn read_row_group_into_reusable_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    reusable: &mut ColumnBundleReusableBuffers,
) -> Result<(ColumnBundleReadFillReport, ReadAttributionAccumulator)> {
    let row_count = row_count_for_row_group(metadata, row_group_id)?;
    reusable.prepare_for_rows(row_count)?;
    let mut fill_buffers = reusable.fill_buffers()?;
    read_row_group_into_with_attribution(source, metadata, columns, row_group_id, &mut fill_buffers)
}

pub(super) fn row_count_for_row_group(
    metadata: &OcbMetadataV1,
    row_group_id: u32,
) -> Result<usize> {
    let row_group =
        metadata
            .row_group_by_id(row_group_id)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB reusable read references an unknown row group",
            ))?;
    usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB reusable read row count does not fit usize")
    })
}

pub(super) fn read_row_group_into_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    buffers: &mut [ColumnBundleColumnFillBuffer<'_>],
) -> Result<(ColumnBundleReadFillReport, ReadAttributionAccumulator)> {
    let row_group_started = Instant::now();
    if buffers.is_empty() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill read requires at least one column buffer",
        ));
    }
    let row_group =
        *metadata
            .row_group_by_id(row_group_id)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fill read references an unknown row group",
            ))?;
    let row_count = usize::try_from(row_group.row_count).map_err(|_| {
        ArcadiaTioError::ocb_invalid_input("OCB fill read row count does not fit usize")
    })?;
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let mut chunk_by_column = try_column_bundle_hash_map_with_capacity(chunks.len())?;
    for chunk in chunks {
        if chunk.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group chunk descriptor references a different row group",
            ));
        }
        if chunk_by_column.insert(chunk.column_id, *chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut by_id = try_column_bundle_hash_map_with_capacity(columns.len())?;
    by_id.extend(columns.iter().map(|column| (column.id, column)));
    let mut by_name = try_column_bundle_hash_map_with_capacity(columns.len())?;
    by_name.extend(columns.iter().map(|column| (column.name.as_str(), column)));
    let mut seen_columns = try_column_bundle_hash_set_with_capacity(buffers.len())?;
    let mut resolved = Vec::new();
    resolved
        .try_reserve_exact(buffers.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for (buffer_index, buffer) in buffers.iter().enumerate() {
        let Some(column) = resolve_fill_column(buffer, &by_id, &by_name)? else {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer must identify a column by name or id",
            ));
        };
        if !seen_columns.insert(column.id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill request contains duplicate column buffers",
            ));
        }
        if buffer.values.physical_type() != column.physical_type {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fill buffer dtype does not match column dtype",
            ));
        }
        buffer.values.validate_capacity(row_count)?;
        let chunk = *chunk_by_column
            .get(&column.id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        if chunk.physical_type != column.physical_type.ocb_physical_type() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk physical type does not match schema",
            ));
        }
        if chunk.row_count != row_group.row_count {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB chunk row_count does not match row group",
            ));
        }
        if !column.nullable && !chunk.validity_ref.is_null() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB non-null column chunk cannot have a validity bitmap",
            ));
        }
        if !chunk.validity_ref.is_null() {
            if !buffer.allow_nulls {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer rejected nullable column chunk",
                ));
            }
            let Some(validity_bytes) = buffer.validity_bytes.as_deref() else {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage for nullable column chunk",
                ));
            };
            let expected_validity_bytes =
                usize::try_from(chunk.row_count.div_ceil(8)).map_err(|_| {
                    ArcadiaTioError::ocb_invalid_input(
                        "OCB fill validity byte count does not fit usize",
                    )
                })?;
            if validity_bytes.len() < expected_validity_bytes {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer validity capacity is too small for row group",
                ));
            }
        }
        resolved.push(ResolvedColumnFill {
            buffer_index,
            column,
            chunk,
        });
    }

    let mut selected_column_ids = Vec::new();
    selected_column_ids
        .try_reserve_exact(resolved.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    selected_column_ids.extend(resolved.iter().map(|target| target.column.id));
    let footprint =
        selected_resource_footprint_for_row_group_desc(metadata, &row_group, &selected_column_ids)?;
    validate_footprint_totals(footprint, metadata.resource_limits)?;

    let mut reports = Vec::new();
    reports
        .try_reserve_exact(resolved.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    let mut attribution = ReadAttributionAccumulator::default();
    for target in resolved {
        let report = fill_column_buffer_with_attribution(
            source,
            metadata,
            row_count,
            &target,
            &mut buffers[target.buffer_index],
            &mut attribution,
        )?;
        reports.push(report);
    }
    attribution.row_group_read += row_group_started.elapsed();
    attribution.row_groups_materialized = 1;
    attribution.column_chunks_materialized = reports.len();
    Ok((
        ColumnBundleReadFillReport {
            row_group_id: row_group.row_group_id,
            base_row: row_group.base_row,
            row_count: row_group.row_count,
            columns: reports,
        },
        attribution,
    ))
}

pub(super) fn resolve_fill_column<'a>(
    buffer: &ColumnBundleColumnFillBuffer<'_>,
    by_id: &HashMap<u32, &'a BundleColumn>,
    by_name: &HashMap<&str, &'a BundleColumn>,
) -> Result<Option<&'a BundleColumn>> {
    let by_name_column = match buffer.column_name {
        Some(name) => Some(*by_name.get(name).ok_or(ArcadiaTioError::ocb_invalid_input(
            "OCB fill buffer references an unknown column name",
        ))?),
        None => None,
    };
    let by_id_column = match buffer.column_id {
        Some(column_id) => Some(*by_id.get(&column_id).ok_or(
            ArcadiaTioError::ocb_invalid_input("OCB fill buffer references an unknown column id"),
        )?),
        None => None,
    };
    match (by_name_column, by_id_column) {
        (Some(left), Some(right)) if left.id != right.id => Err(
            ArcadiaTioError::ocb_invalid_input("OCB fill buffer column name and id do not match"),
        ),
        (Some(column), _) | (_, Some(column)) => Ok(Some(column)),
        (None, None) => Ok(None),
    }
}

pub(super) fn fill_column_buffer(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    row_count: usize,
    target: &ResolvedColumnFill<'_>,
    buffer: &mut ColumnBundleColumnFillBuffer<'_>,
) -> Result<ColumnBundleColumnFillReport> {
    if let Some(out) =
        uncompressed_fixed_binary_direct_output(row_count, target, &mut buffer.values)?
    {
        read_uncompressed_fixed_binary_chunk_from_source_into_with_resource_limits(
            source,
            metadata.file_len,
            &target.chunk,
            out,
            metadata.resource_limits,
        )?;
    } else {
        let object = read_column_chunk_from_source_with_resource_limits(
            source,
            metadata.file_len,
            &target.chunk,
            metadata.resource_limits,
        )?;
        let payload = validate_and_decode_chunk_object(
            object,
            target.column,
            &target.chunk,
            metadata.resource_limits,
        )?;
        fill_primitive_values(&payload, row_count, &mut buffer.values)?;
    }
    let mut validity_filled = false;
    if !target.chunk.validity_ref.is_null() {
        let validity = read_validity_bitmap(source, metadata, &target.chunk)?.ok_or(
            ArcadiaTioError::ocb_corrupt_file("OCB validity bitmap is missing after validation"),
        )?;
        let validity_bytes =
            buffer
                .validity_bytes
                .as_deref_mut()
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage",
                ))?;
        validity_bytes[..validity.bytes.len()].copy_from_slice(&validity.bytes);
        validity_filled = true;
    }
    Ok(ColumnBundleColumnFillReport {
        column_id: target.column.id,
        rows_filled: row_count,
        validity_filled,
    })
}

pub(super) fn fill_column_buffer_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    row_count: usize,
    target: &ResolvedColumnFill<'_>,
    buffer: &mut ColumnBundleColumnFillBuffer<'_>,
    attribution: &mut ReadAttributionAccumulator,
) -> Result<ColumnBundleColumnFillReport> {
    if let Some(out) =
        uncompressed_fixed_binary_direct_output(row_count, target, &mut buffer.values)?
    {
        let mut object_attr = OcbReadObjectAttribution::default();
        read_uncompressed_fixed_binary_chunk_from_source_into_with_attribution_and_resource_limits(
            source,
            metadata.file_len,
            &target.chunk,
            out,
            &mut object_attr,
            metadata.resource_limits,
        )?;
        attribution.add_object(object_attr);
        attribution.compressed_bytes = attribution
            .compressed_bytes
            .saturating_add(target.chunk.uncompressed_bytes);
        attribution.uncompressed_bytes = attribution
            .uncompressed_bytes
            .saturating_add(target.chunk.uncompressed_bytes);
    } else {
        let object = read_column_chunk_attributed(
            source,
            metadata.file_len,
            &target.chunk,
            attribution,
            metadata.resource_limits,
        )?;
        let (payload, decompression) = validate_and_decode_chunk_object_attributed(
            object,
            target.column,
            &target.chunk,
            metadata.resource_limits,
        )?;
        attribution.decompression += decompression;
        let decode_started = Instant::now();
        fill_primitive_values(&payload, row_count, &mut buffer.values)?;
        record_value_materialization_time(
            attribution,
            target.column.physical_type,
            decode_started.elapsed(),
        );
    }
    let mut validity_filled = false;
    if !target.chunk.validity_ref.is_null() {
        let validity =
            read_validity_bitmap_with_attribution(source, metadata, &target.chunk, attribution)?
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB validity bitmap is missing after validation",
                ))?;
        let validity_bytes =
            buffer
                .validity_bytes
                .as_deref_mut()
                .ok_or(ArcadiaTioError::ocb_invalid_input(
                    "OCB fill buffer needs validity storage",
                ))?;
        validity_bytes[..validity.bytes.len()].copy_from_slice(&validity.bytes);
        validity_filled = true;
    }
    Ok(ColumnBundleColumnFillReport {
        column_id: target.column.id,
        rows_filled: row_count,
        validity_filled,
    })
}

pub(super) fn uncompressed_fixed_binary_direct_output<'a>(
    row_count: usize,
    target: &ResolvedColumnFill<'_>,
    values: &'a mut PrimitiveColumnValuesMut<'_>,
) -> Result<Option<&'a mut [u8]>> {
    if target.chunk.codec != OcbChunkCodecV1::None
        || target.chunk.physical_type != OcbPhysicalTypeV1::FixedBinary
    {
        return Ok(None);
    }
    let ColumnPhysicalType::FixedBinary {
        width: column_width,
    } = target.column.physical_type
    else {
        return Ok(None);
    };
    let PrimitiveColumnValuesMut::FixedBinary {
        width: buffer_width,
        bytes,
    } = values
    else {
        return Ok(None);
    };
    if *buffer_width != column_width {
        return Ok(None);
    }
    let expected_bytes =
        row_count
            .checked_mul(column_width as usize)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-binary fill byte count overflows",
            ))?;
    if target.chunk.uncompressed_bytes != expected_bytes as u64 {
        // Preserve the established corruption/error ordering for malformed
        // descriptor-to-schema lengths by using the full object decoder.
        return Ok(None);
    }
    Ok(Some(&mut bytes[..expected_bytes]))
}

pub(super) fn fill_primitive_values(
    payload: &[u8],
    row_count: usize,
    values: &mut PrimitiveColumnValuesMut<'_>,
) -> Result<()> {
    match values {
        PrimitiveColumnValuesMut::I32(out) => fill_i32_values(payload, row_count, out),
        PrimitiveColumnValuesMut::I64(out) => fill_i64_values(payload, row_count, out),
        PrimitiveColumnValuesMut::F32(out) => fill_f32_values(payload, row_count, out),
        PrimitiveColumnValuesMut::F64(out) => fill_f64_values(payload, row_count, out),
        PrimitiveColumnValuesMut::FixedBinary { width, bytes } => {
            fill_fixed_binary_values(payload, row_count, *width, bytes)
        }
    }
}

pub(super) fn fill_fixed_binary_values(
    payload: &[u8],
    row_count: usize,
    width: u32,
    out: &mut [u8],
) -> Result<()> {
    if width == 0 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary column requires fixed width",
        ));
    }
    let expected_bytes =
        row_count
            .checked_mul(width as usize)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB fixed-binary payload byte length overflows",
            ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB fixed-binary payload length does not match row count",
        ));
    }
    if out.len() < expected_bytes {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB fill buffer value capacity is too small for row group",
        ));
    }
    out[..expected_bytes].copy_from_slice(payload);
    Ok(())
}

pub(super) fn fill_i32_values(payload: &[u8], row_count: usize, out: &mut [i32]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB i32 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB i32 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(4)) {
        *dst = i32::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

pub(super) fn fill_i64_values(payload: &[u8], row_count: usize, out: &mut [i64]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(8)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB i64 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB i64 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(8)) {
        *dst = i64::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

pub(super) fn fill_f32_values(payload: &[u8], row_count: usize, out: &mut [f32]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB f32 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB f32 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(4)) {
        *dst = f32::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}

pub(super) fn fill_f64_values(payload: &[u8], row_count: usize, out: &mut [f64]) -> Result<()> {
    let expected_bytes = row_count
        .checked_mul(8)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB f64 payload byte length overflows",
        ))?;
    if payload.len() != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB f64 payload length does not match row count",
        ));
    }
    for (dst, chunk) in out[..row_count].iter_mut().zip(payload.chunks_exact(8)) {
        *dst = f64::from_le_bytes(chunk.try_into().expect("chunk length"));
    }
    Ok(())
}
