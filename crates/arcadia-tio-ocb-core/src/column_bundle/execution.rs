//! Resource admission plus row-group and column-chunk execution helpers.

use super::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct SelectedResourceFootprint {
    pub(super) compressed_bytes: u64,
    pub(super) decoded_materialized_bytes: u64,
    pub(super) row_count: u64,
}

pub(super) fn selected_resource_footprint_for_row_group(
    metadata: &OcbMetadataV1,
    row_group_id: u32,
    selected_column_ids: &[u32],
) -> Result<SelectedResourceFootprint> {
    let row_group =
        metadata
            .row_group_by_id(row_group_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB resource accounting row group not found",
            ))?;
    selected_resource_footprint_for_row_group_desc(metadata, row_group, selected_column_ids)
}

pub(super) fn selected_resource_footprint_for_row_group_desc(
    metadata: &OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
    selected_column_ids: &[u32],
) -> Result<SelectedResourceFootprint> {
    let chunks = chunks_for_row_group(
        metadata,
        row_group.chunk_desc_begin,
        row_group.chunk_desc_count,
    )?;
    let object_overhead = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    for column_id in selected_column_ids {
        let chunk = chunks
            .iter()
            .find(|chunk| chunk.column_id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB resource accounting selected chunk is missing",
            ))?;
        chunk
            .value_ref
            .validate(OcbBodyKindV1::ColumnChunk, metadata.file_len)?;
        chunk.value_ref.length.checked_sub(object_overhead).ok_or(
            ArcadiaTioError::ocb_corrupt_file(
                "OCB resource accounting column chunk object is too short",
            ),
        )?;
        if !chunk.validity_ref.is_null() {
            chunk
                .validity_ref
                .validate(OcbBodyKindV1::ValidityBitmap, metadata.file_len)?;
            if chunk.validity_ref.length != chunk.row_count.div_ceil(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB validity bitmap length does not match row count",
                ));
            }
        }
    }
    let mut footprint = SelectedResourceFootprint {
        row_count: row_group.row_count,
        ..SelectedResourceFootprint::default()
    };
    for column_id in selected_column_ids {
        let chunk = chunks
            .iter()
            .find(|chunk| chunk.column_id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB resource accounting selected chunk is missing",
            ))?;
        chunk
            .value_ref
            .validate(OcbBodyKindV1::ColumnChunk, metadata.file_len)?;
        let encoded_bytes = chunk.value_ref.length.checked_sub(object_overhead).ok_or(
            ArcadiaTioError::ocb_corrupt_file(
                "OCB resource accounting column chunk object is too short",
            ),
        )?;
        if chunk.value_ref.length > metadata.resource_limits.max_encoded_object_bytes() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB encoded object exceeds resource limit",
            ));
        }
        if encoded_bytes > metadata.resource_limits.max_compressed_chunk_bytes() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB encoded column chunk payload exceeds resource limit",
            ));
        }
        if chunk.uncompressed_bytes > metadata.resource_limits.max_decompressed_chunk_bytes() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB decoded column chunk payload exceeds resource limit",
            ));
        }
        footprint.compressed_bytes = footprint
            .compressed_bytes
            .checked_add(encoded_bytes)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected compressed byte accounting overflows",
            ))?;
        footprint.decoded_materialized_bytes = footprint
            .decoded_materialized_bytes
            .checked_add(chunk.uncompressed_bytes)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected decoded byte accounting overflows",
            ))?;

        if !chunk.validity_ref.is_null() {
            chunk
                .validity_ref
                .validate(OcbBodyKindV1::ValidityBitmap, metadata.file_len)?;
            let validity_bytes = chunk.row_count.div_ceil(8);
            if chunk.validity_ref.length != validity_bytes {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB validity bitmap length does not match row count",
                ));
            }
            if chunk.validity_ref.length > metadata.resource_limits.max_encoded_object_bytes() {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB encoded object exceeds resource limit",
                ));
            }
            footprint.compressed_bytes = footprint
                .compressed_bytes
                .checked_add(validity_bytes)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB selected compressed byte accounting overflows",
                ))?;
            footprint.decoded_materialized_bytes = footprint
                .decoded_materialized_bytes
                .checked_add(validity_bytes)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB selected decoded byte accounting overflows",
                ))?;
        }
    }
    if footprint.decoded_materialized_bytes
        > metadata.resource_limits.max_projected_row_group_bytes()
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB projected decoded row group exceeds resource limit",
        ));
    }
    Ok(footprint)
}

pub(super) fn selected_resource_footprints_for_plan(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
) -> Result<Vec<SelectedResourceFootprint>> {
    let row_groups = selected_row_groups_for_plan(metadata, plan)?;
    let mut footprints = Vec::new();
    footprints
        .try_reserve_exact(row_groups.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for row_group in row_groups {
        footprints.push(selected_resource_footprint_for_row_group_desc(
            metadata,
            row_group,
            &plan.projected_column_ids,
        )?);
    }
    Ok(footprints)
}

pub(super) fn selected_row_groups_for_plan<'a>(
    metadata: &'a OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
) -> Result<Vec<&'a OcbRowGroupDescV1>> {
    let mut seen_row_group_ids = HashSet::new();
    seen_row_group_ids
        .try_reserve(plan.row_group_ids.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for row_group_id in &plan.row_group_ids {
        if !seen_row_group_ids.insert(*row_group_id) {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB read plan contains duplicate row group ids",
            ));
        }
    }

    let mut ordered = Vec::new();
    ordered
        .try_reserve_exact(plan.row_group_ids.len())
        .map_err(|_| resource_accounting_allocation_error())?;
    for row_group_id in &plan.row_group_ids {
        ordered.push(metadata.row_group_by_id(*row_group_id).ok_or(
            ArcadiaTioError::ocb_invalid_input("OCB read plan references an unknown row group id"),
        )?);
    }
    Ok(ordered)
}

pub(super) fn resource_accounting_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        std::io::ErrorKind::OutOfMemory,
        "OCB resource accounting allocation failed within resource limit",
    ))
}

pub(super) fn column_bundle_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        std::io::ErrorKind::OutOfMemory,
        "OCB column-bundle allocation failed within resource limit",
    ))
}

pub(super) fn try_column_bundle_vec_with_capacity<T>(capacity: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| column_bundle_allocation_error())?;
    Ok(values)
}

pub(super) fn try_column_bundle_hash_map_with_capacity<K: Eq + std::hash::Hash, V>(
    capacity: usize,
) -> Result<HashMap<K, V>> {
    let mut values = HashMap::new();
    values
        .try_reserve(capacity)
        .map_err(|_| column_bundle_allocation_error())?;
    Ok(values)
}

pub(super) fn try_column_bundle_hash_set_with_capacity<T: Eq + std::hash::Hash>(
    capacity: usize,
) -> Result<HashSet<T>> {
    let mut values = HashSet::new();
    values
        .try_reserve(capacity)
        .map_err(|_| column_bundle_allocation_error())?;
    Ok(values)
}

pub(super) fn zeroed_vec_fallibly<T: Clone + Default>(len: usize) -> Result<Vec<T>> {
    let mut values = try_column_bundle_vec_with_capacity(len)?;
    values.resize(len, T::default());
    Ok(values)
}

pub(super) fn resize_vec_fallibly<T: Clone>(
    values: &mut Vec<T>,
    len: usize,
    value: T,
) -> Result<()> {
    if len > values.len() {
        values
            .try_reserve_exact(len - values.len())
            .map_err(|_| column_bundle_allocation_error())?;
    }
    values.resize(len, value);
    Ok(())
}

pub(super) fn clone_column_bundle_string_fallibly(value: &str) -> Result<String> {
    let mut copy = String::new();
    copy.try_reserve_exact(value.len())
        .map_err(|_| column_bundle_allocation_error())?;
    copy.push_str(value);
    Ok(copy)
}

pub(super) fn clone_column_bundle_u32s_fallibly(values: &[u32]) -> Result<Vec<u32>> {
    let mut copy = try_column_bundle_vec_with_capacity(values.len())?;
    copy.extend_from_slice(values);
    Ok(copy)
}

pub(super) fn validate_footprint_totals(
    footprint: SelectedResourceFootprint,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    if footprint.compressed_bytes > resource_limits.max_owned_selected_compressed_bytes() {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB selected compressed bytes exceed resource limit",
        ));
    }
    if footprint.decoded_materialized_bytes > resource_limits.max_owned_decoded_materialized_bytes()
    {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB decoded and materialized bytes exceed resource limit",
        ));
    }
    Ok(())
}

pub(super) fn validate_owned_plan_resource_limits(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
) -> Result<()> {
    let mut total = SelectedResourceFootprint::default();
    for footprint in selected_resource_footprints_for_plan(metadata, plan)? {
        total.compressed_bytes = total
            .compressed_bytes
            .checked_add(footprint.compressed_bytes)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB owned compressed byte accounting overflows",
            ))?;
        total.decoded_materialized_bytes = total
            .decoded_materialized_bytes
            .checked_add(footprint.decoded_materialized_bytes)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB owned decoded byte accounting overflows",
            ))?;
    }
    validate_footprint_totals(total, metadata.resource_limits)
}

pub(super) fn validate_in_flight_plan_resource_limits(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
    max_in_flight_row_groups: usize,
) -> Result<()> {
    if plan.row_group_ids.is_empty() {
        return Ok(());
    }
    let reservation_count = max_in_flight_row_groups
        .max(1)
        .min(plan.row_group_ids.len());
    let mut footprints = selected_resource_footprints_for_plan(metadata, plan)?;
    footprints.sort_unstable_by(|left, right| right.compressed_bytes.cmp(&left.compressed_bytes));
    let compressed_bytes = footprints
        .iter()
        .take(reservation_count)
        .try_fold(0u64, |total, footprint| {
            total.checked_add(footprint.compressed_bytes)
        })
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB in-flight compressed byte accounting overflows",
        ))?;
    footprints.sort_unstable_by(|left, right| {
        right
            .decoded_materialized_bytes
            .cmp(&left.decoded_materialized_bytes)
    });
    let decoded_materialized_bytes = footprints
        .iter()
        .take(reservation_count)
        .try_fold(0u64, |total, footprint| {
            total.checked_add(footprint.decoded_materialized_bytes)
        })
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB in-flight decoded byte accounting overflows",
        ))?;
    validate_footprint_totals(
        SelectedResourceFootprint {
            compressed_bytes,
            decoded_materialized_bytes,
            row_count: 0,
        },
        metadata.resource_limits,
    )
}

pub(super) fn validate_contiguous_in_flight_plan_resource_limits(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
    max_in_flight_row_groups: usize,
) -> Result<()> {
    let wave_size = max_in_flight_row_groups.max(1);
    let footprints = selected_resource_footprints_for_plan(metadata, plan)?;
    for wave in footprints.chunks(wave_size) {
        let total = wave.iter().try_fold(
            SelectedResourceFootprint::default(),
            |mut total, footprint| {
                total.compressed_bytes = total
                    .compressed_bytes
                    .checked_add(footprint.compressed_bytes)?;
                total.decoded_materialized_bytes = total
                    .decoded_materialized_bytes
                    .checked_add(footprint.decoded_materialized_bytes)?;
                Some(total)
            },
        );
        let total = total.ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB in-flight resource accounting overflows",
        ))?;
        validate_footprint_totals(total, metadata.resource_limits)?;
    }
    Ok(())
}

pub(super) fn validate_sliding_in_flight_plan_resource_limits(
    metadata: &OcbMetadataV1,
    plan: &ColumnBundleReadPlan,
    max_in_flight_row_groups: usize,
) -> Result<()> {
    let footprints = selected_resource_footprints_for_plan(metadata, plan)?;
    validate_sliding_resource_footprints(
        &footprints,
        max_in_flight_row_groups,
        metadata.resource_limits,
    )
}

pub(super) fn validate_sliding_resource_footprints(
    footprints: &[SelectedResourceFootprint],
    max_in_flight_row_groups: usize,
    resource_limits: OcbResourceLimits,
) -> Result<()> {
    if footprints.is_empty() {
        return Ok(());
    }
    let window_size = max_in_flight_row_groups.max(1).min(footprints.len());
    let mut total = footprints[..window_size].iter().try_fold(
        SelectedResourceFootprint::default(),
        |mut total, footprint| {
            total.compressed_bytes = total
                .compressed_bytes
                .checked_add(footprint.compressed_bytes)?;
            total.decoded_materialized_bytes = total
                .decoded_materialized_bytes
                .checked_add(footprint.decoded_materialized_bytes)?;
            Some(total)
        },
    );
    let mut total = total.take().ok_or(ArcadiaTioError::ocb_corrupt_file(
        "OCB sliding in-flight resource accounting overflows",
    ))?;
    validate_footprint_totals(total, resource_limits)?;
    for next in window_size..footprints.len() {
        let previous = footprints[next - window_size];
        total.compressed_bytes = total
            .compressed_bytes
            .checked_sub(previous.compressed_bytes)
            .and_then(|bytes| bytes.checked_add(footprints[next].compressed_bytes))
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB sliding compressed byte accounting is inconsistent",
            ))?;
        total.decoded_materialized_bytes = total
            .decoded_materialized_bytes
            .checked_sub(previous.decoded_materialized_bytes)
            .and_then(|bytes| bytes.checked_add(footprints[next].decoded_materialized_bytes))
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB sliding decoded byte accounting is inconsistent",
            ))?;
        validate_footprint_totals(total, resource_limits)?;
    }
    Ok(())
}

pub(super) fn read_row_group(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    selected_column_ids: &[u32],
) -> Result<ColumnBatch> {
    let row_group = metadata
        .row_group_by_id(row_group_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row group not found"))?;
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
        if chunk_by_column.insert(chunk.column_id, chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut arrays = try_column_bundle_vec_with_capacity(selected_column_ids.len())?;
    for column_id in selected_column_ids {
        let column = columns
            .iter()
            .find(|column| column.id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected column not found",
            ))?;
        let chunk = chunk_by_column
            .get(column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        arrays.push(read_column_array(
            source,
            metadata,
            column,
            chunk,
            row_group.row_count,
        )?);
    }

    Ok(ColumnBatch {
        row_group_id: row_group.row_group_id,
        base_row: row_group.base_row,
        row_count: row_group.row_count,
        columns: arrays,
    })
}

pub(super) fn read_row_group_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    columns: &[BundleColumn],
    row_group_id: u32,
    selected_column_ids: &[u32],
) -> Result<(ColumnBatch, ReadAttributionAccumulator)> {
    let row_group_started = Instant::now();
    let row_group = metadata
        .row_group_by_id(row_group_id)
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row group not found"))?;
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
        if chunk_by_column.insert(chunk.column_id, chunk).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate chunk descriptors for a column",
            ));
        }
    }

    let mut arrays = try_column_bundle_vec_with_capacity(selected_column_ids.len())?;
    let mut attribution = ReadAttributionAccumulator::default();
    for column_id in selected_column_ids {
        let column = columns
            .iter()
            .find(|column| column.id == *column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB selected column not found",
            ))?;
        let chunk = chunk_by_column
            .get(column_id)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group is missing a selected column chunk",
            ))?;
        let (array, column_attr) = read_column_array_with_attribution(
            source,
            metadata,
            column,
            chunk,
            row_group.row_count,
        )?;
        attribution.add(column_attr);
        arrays.push(array);
    }

    attribution.row_group_read += row_group_started.elapsed();
    attribution.row_groups_materialized = 1;
    attribution.column_chunks_materialized = arrays.len();
    Ok((
        ColumnBatch {
            row_group_id: row_group.row_group_id,
            base_row: row_group.base_row,
            row_count: row_group.row_count,
            columns: arrays,
        },
        attribution,
    ))
}

pub(super) fn chunks_for_row_group(
    metadata: &OcbMetadataV1,
    begin: u64,
    count: u32,
) -> Result<&[OcbColumnChunkDescV1]> {
    let begin = usize::try_from(begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group chunk descriptor begin is too large")
    })?;
    let count = count as usize;
    let end = begin
        .checked_add(count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .column_chunks
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group chunk descriptor range is out of bounds",
        ))
}

pub(super) fn read_column_array(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    expected_rows: u64,
) -> Result<ColumnArray> {
    if !column.nullable && !chunk.validity_ref.is_null() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB non-null column chunk cannot have a validity bitmap",
        ));
    }
    if chunk.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk physical type does not match schema",
        ));
    }
    if chunk.row_count != expected_rows {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }
    let object = read_column_chunk_from_source_with_resource_limits(
        source,
        metadata.file_len,
        chunk,
        metadata.resource_limits,
    )?;
    let payload =
        validate_and_decode_chunk_object(object, column, chunk, metadata.resource_limits)?;
    let validity = read_validity_bitmap(source, metadata, chunk)?;
    Ok(ColumnArray {
        column_id: column.id,
        name: clone_column_bundle_string_fallibly(&column.name)?,
        physical_type: column.physical_type,
        logical_kind: column.logical_kind,
        dictionary_id: column.dictionary_id,
        values: decode_primitive_values(column.physical_type, payload)?,
        validity,
    })
}

pub(super) fn read_column_array_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    expected_rows: u64,
) -> Result<(ColumnArray, ReadAttributionAccumulator)> {
    if !column.nullable && !chunk.validity_ref.is_null() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB non-null column chunk cannot have a validity bitmap",
        ));
    }
    if chunk.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk physical type does not match schema",
        ));
    }
    if chunk.row_count != expected_rows {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB chunk row_count does not match row group",
        ));
    }

    let mut attribution = ReadAttributionAccumulator::default();
    let object = read_column_chunk_attributed(
        source,
        metadata.file_len,
        chunk,
        &mut attribution,
        metadata.resource_limits,
    )?;
    let (payload, decompression) = validate_and_decode_chunk_object_attributed(
        object,
        column,
        chunk,
        metadata.resource_limits,
    )?;
    attribution.decompression += decompression;
    let validity =
        read_validity_bitmap_with_attribution(source, metadata, chunk, &mut attribution)?;
    let decode_started = Instant::now();
    let values = decode_primitive_values(column.physical_type, payload)?;
    record_value_materialization_time(
        &mut attribution,
        column.physical_type,
        decode_started.elapsed(),
    );
    Ok((
        ColumnArray {
            column_id: column.id,
            name: clone_column_bundle_string_fallibly(&column.name)?,
            physical_type: column.physical_type,
            logical_kind: column.logical_kind,
            dictionary_id: column.dictionary_id,
            values,
            validity,
        },
        attribution,
    ))
}

pub(super) fn read_column_chunk_attributed(
    source: &OcbReadSource,
    file_len: u64,
    chunk: &OcbColumnChunkDescV1,
    attribution: &mut ReadAttributionAccumulator,
    resource_limits: OcbResourceLimits,
) -> Result<OcbColumnChunkObjectV1> {
    let mut object_attr = OcbReadObjectAttribution::default();
    let object = read_column_chunk_from_source_with_attribution_and_resource_limits(
        source,
        file_len,
        chunk,
        &mut object_attr,
        resource_limits,
    )?;
    attribution.add_object(object_attr);
    attribution.compressed_bytes = attribution
        .compressed_bytes
        .saturating_add(object.payload.len() as u64);
    attribution.uncompressed_bytes = attribution
        .uncompressed_bytes
        .saturating_add(chunk.uncompressed_bytes);
    Ok(object)
}

pub(super) fn read_validity_bitmap(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    chunk: &OcbColumnChunkDescV1,
) -> Result<Option<ValidityBitmap>> {
    if chunk.validity_ref.is_null() {
        return Ok(None);
    }
    let expected_bytes = chunk.row_count.div_ceil(8);
    let mut file = source.cursor();
    let bytes = read_object_bytes_with_resource_limits(
        &mut file,
        metadata.file_len,
        chunk.validity_ref,
        OcbBodyKindV1::ValidityBitmap,
        metadata.resource_limits,
    )?;
    if bytes.len() as u64 != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB validity bitmap length does not match row count",
        ));
    }
    Ok(Some(ValidityBitmap {
        row_count: chunk.row_count,
        bytes,
    }))
}

pub(super) fn read_validity_bitmap_with_attribution(
    source: &OcbReadSource,
    metadata: &OcbMetadataV1,
    chunk: &OcbColumnChunkDescV1,
    attribution: &mut ReadAttributionAccumulator,
) -> Result<Option<ValidityBitmap>> {
    if chunk.validity_ref.is_null() {
        return Ok(None);
    }
    let expected_bytes = chunk.row_count.div_ceil(8);
    let mut file = source.cursor();
    let mut object_attr = OcbReadObjectAttribution::default();
    let bytes = read_object_bytes_with_attribution_and_resource_limits(
        &mut file,
        metadata.file_len,
        chunk.validity_ref,
        OcbBodyKindV1::ValidityBitmap,
        &mut object_attr,
        metadata.resource_limits,
    )?;
    attribution.add_object(object_attr);
    if bytes.len() as u64 != expected_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB validity bitmap length does not match row count",
        ));
    }
    Ok(Some(ValidityBitmap {
        row_count: chunk.row_count,
        bytes,
    }))
}

pub(super) fn validate_and_decode_chunk_object(
    object: OcbColumnChunkObjectV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<Vec<u8>> {
    if object.row_group_id != chunk.row_group_id || object.column_id != chunk.column_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object identity does not match descriptor",
        ));
    }
    if object.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object physical type does not match schema",
        ));
    }
    if object.codec != chunk.codec {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object codec does not match descriptor",
        ));
    }
    if object.row_count != chunk.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object row_count does not match descriptor",
        ));
    }
    if object.uncompressed_bytes != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    let payload = object.into_decoded_payload_with_limits(
        resource_limits.max_compressed_chunk_bytes(),
        resource_limits.max_decompressed_chunk_bytes(),
    )?;
    if payload.len() as u64 != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    Ok(payload)
}

pub(super) fn validate_and_decode_chunk_object_attributed(
    object: OcbColumnChunkObjectV1,
    column: &BundleColumn,
    chunk: &OcbColumnChunkDescV1,
    resource_limits: OcbResourceLimits,
) -> Result<(Vec<u8>, Duration)> {
    if object.row_group_id != chunk.row_group_id || object.column_id != chunk.column_id {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object identity does not match descriptor",
        ));
    }
    if object.physical_type != column.physical_type.ocb_physical_type() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object physical type does not match schema",
        ));
    }
    if object.codec != chunk.codec {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object codec does not match descriptor",
        ));
    }
    if object.row_count != chunk.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object row_count does not match descriptor",
        ));
    }
    if object.uncompressed_bytes != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    let decode_started = Instant::now();
    let codec = object.codec;
    let payload = object.into_decoded_payload_with_limits(
        resource_limits.max_compressed_chunk_bytes(),
        resource_limits.max_decompressed_chunk_bytes(),
    )?;
    let decompression = if codec == OcbChunkCodecV1::Zstd {
        decode_started.elapsed()
    } else {
        Duration::ZERO
    };
    if payload.len() as u64 != chunk.uncompressed_bytes {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk object byte length does not match descriptor",
        ));
    }
    Ok((payload, decompression))
}

pub(super) fn decode_primitive_values(
    physical_type: ColumnPhysicalType,
    payload: Vec<u8>,
) -> Result<PrimitiveColumnValues> {
    match physical_type {
        ColumnPhysicalType::I32 => {
            if !payload.len().is_multiple_of(4) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB i32 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::I32(
                decode_primitive_chunks_fallibly(&payload, 4, |chunk| {
                    i32::from_le_bytes(chunk.try_into().expect("chunk length"))
                })?,
            ))
        }
        ColumnPhysicalType::I64 => {
            if !payload.len().is_multiple_of(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB i64 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::I64(
                decode_primitive_chunks_fallibly(&payload, 8, |chunk| {
                    i64::from_le_bytes(chunk.try_into().expect("chunk length"))
                })?,
            ))
        }
        ColumnPhysicalType::F32 => {
            if !payload.len().is_multiple_of(4) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB f32 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::F32(
                decode_primitive_chunks_fallibly(&payload, 4, |chunk| {
                    f32::from_le_bytes(chunk.try_into().expect("chunk length"))
                })?,
            ))
        }
        ColumnPhysicalType::F64 => {
            if !payload.len().is_multiple_of(8) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB f64 payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::F64(
                decode_primitive_chunks_fallibly(&payload, 8, |chunk| {
                    f64::from_le_bytes(chunk.try_into().expect("chunk length"))
                })?,
            ))
        }
        ColumnPhysicalType::FixedBinary { width } => {
            if width == 0 {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary column requires fixed width",
                ));
            }
            if !payload.len().is_multiple_of(width as usize) {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary payload length is not aligned",
                ));
            }
            Ok(PrimitiveColumnValues::FixedBinary {
                width,
                bytes: payload,
            })
        }
    }
}

pub(super) fn decode_primitive_chunks_fallibly<T>(
    payload: &[u8],
    width: usize,
    mut decode: impl FnMut(&[u8]) -> T,
) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(payload.len() / width)
        .map_err(|_| decoded_values_allocation_error())?;
    for chunk in payload.chunks_exact(width) {
        values.push(decode(chunk));
    }
    Ok(values)
}

pub(super) fn decoded_values_allocation_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(
        std::io::ErrorKind::OutOfMemory,
        "OCB decoded-value allocation failed within resource limit",
    ))
}
