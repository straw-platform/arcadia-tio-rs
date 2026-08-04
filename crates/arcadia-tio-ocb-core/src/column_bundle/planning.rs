//! Snapshot fingerprinting, predicate selection, and metadata-plan validation.

use super::*;

pub(super) fn body_ref_summary(body_ref: OcbBodyRefV2) -> ColumnBundleBodyRefSummary {
    ColumnBundleBodyRefSummary {
        offset: body_ref.offset,
        length: body_ref.length,
        kind: body_ref.kind.into(),
        flags: body_ref.flags,
        checksum_kind: body_ref.checksum_kind.into(),
        checksum: body_ref.checksum,
    }
}

pub(super) fn checked_body_ref_summary(
    body_ref: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    file_len: u64,
) -> Result<ColumnBundleBodyRefSummary> {
    body_ref.validate(expected_kind, file_len)?;
    Ok(body_ref_summary(body_ref))
}

pub(super) fn checked_optional_body_ref_summary(
    body_ref: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    file_len: u64,
) -> Result<Option<ColumnBundleBodyRefSummary>> {
    if body_ref.is_null() {
        return Ok(None);
    }
    checked_body_ref_summary(body_ref, expected_kind, file_len).map(Some)
}

pub(super) fn snapshot_fingerprint_for_summaries(
    metadata: &ColumnBundleMetadata,
    row_groups: &[ColumnBundleRowGroupSummary],
) -> ColumnBundleSnapshotFingerprint {
    let schema = fingerprint_schema(metadata);
    let dictionaries = fingerprint_dictionaries(metadata);
    let ordering = fingerprint_ordering(metadata);
    let row_groups_fp = fingerprint_row_group_summaries(row_groups);
    let mut combined = Vec::new();
    fp_str(&mut combined, OCB_CERTIFICATION_FINGERPRINT_ALGORITHM);
    fp_str(&mut combined, &schema);
    fp_str(&mut combined, &dictionaries);
    fp_str(&mut combined, &ordering);
    fp_str(&mut combined, &row_groups_fp);
    ColumnBundleSnapshotFingerprint {
        algorithm: OCB_CERTIFICATION_FINGERPRINT_ALGORITHM,
        schema,
        dictionaries,
        ordering,
        row_groups: row_groups_fp,
        combined: fp_hex(&combined),
    }
}

pub(super) fn fingerprint_schema(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "schema");
    fp_str(&mut bytes, metadata.format_name);
    fp_bool(&mut bytes, metadata.appendable);
    fp_u64(&mut bytes, metadata.row_count);
    fp_u32(&mut bytes, metadata.row_group_count);
    fp_u32(&mut bytes, metadata.column_chunk_count);
    fp_u32(&mut bytes, metadata.columns.len() as u32);
    for column in &metadata.columns {
        fp_u32(&mut bytes, column.id);
        fp_str(&mut bytes, &column.name);
        fp_column_physical_type(&mut bytes, column.physical_type);
        fp_column_logical_kind(&mut bytes, column.logical_kind);
        fp_option_u32(&mut bytes, column.dictionary_id);
        fp_i32(&mut bytes, column.scale);
        fp_bool(&mut bytes, column.nullable);
    }
    fp_hex(&bytes)
}

pub(super) fn fingerprint_dictionaries(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "dictionaries");
    fp_u32(&mut bytes, metadata.dictionaries.len() as u32);
    for dictionary in &metadata.dictionaries {
        fp_u32(&mut bytes, dictionary.dictionary_id);
        fp_str(&mut bytes, &dictionary.name);
        fp_column_physical_type(&mut bytes, dictionary.code_physical_type);
        fp_dictionary_value_kind(&mut bytes, dictionary.value_kind);
        fp_u32(&mut bytes, dictionary.entry_count);
    }
    fp_hex(&bytes)
}

pub(super) fn fingerprint_ordering(metadata: &ColumnBundleMetadata) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "ordering");
    fp_u32(&mut bytes, metadata.ordering_keys.len() as u32);
    for key in &metadata.ordering_keys {
        fp_u32(&mut bytes, key.column_id);
        fp_str(&mut bytes, &key.column_name);
        fp_ordering_direction(&mut bytes, key.direction);
        fp_null_order(&mut bytes, key.null_order);
    }
    fp_hex(&bytes)
}

pub(super) fn fingerprint_row_group_summaries(
    row_groups: &[ColumnBundleRowGroupSummary],
) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "row_groups");
    fp_u32(&mut bytes, row_groups.len() as u32);
    for row_group in row_groups {
        fp_row_group_summary(&mut bytes, row_group);
    }
    fp_hex(&bytes)
}

pub(super) fn fingerprint_selected_chunks(row_groups: &[ColumnBundleRowGroupSummary]) -> String {
    let mut bytes = Vec::new();
    fp_str(&mut bytes, "selected_chunks");
    fp_u32(&mut bytes, row_groups.len() as u32);
    for row_group in row_groups {
        fp_u32(&mut bytes, row_group.row_group_id);
        fp_u64(&mut bytes, row_group.base_row);
        fp_u64(&mut bytes, row_group.row_count);
        fp_u32(&mut bytes, row_group.chunks.len() as u32);
        for chunk in &row_group.chunks {
            fp_column_chunk_summary(&mut bytes, chunk);
        }
    }
    fp_hex(&bytes)
}

pub(super) fn fp_row_group_summary(bytes: &mut Vec<u8>, row_group: &ColumnBundleRowGroupSummary) {
    fp_u32(bytes, row_group.row_group_id);
    fp_u64(bytes, row_group.base_row);
    fp_u64(bytes, row_group.row_count);
    fp_option_body_ref_summary(bytes, row_group.first_key_tuple_ref);
    fp_option_body_ref_summary(bytes, row_group.last_key_tuple_ref);
    fp_u32(bytes, row_group.chunks.len() as u32);
    for chunk in &row_group.chunks {
        fp_column_chunk_summary(bytes, chunk);
    }
    fp_u32(bytes, row_group.stats.len() as u32);
    for stat in &row_group.stats {
        fp_column_stats_summary(bytes, stat);
    }
}

pub(super) fn fp_column_chunk_summary(bytes: &mut Vec<u8>, chunk: &ColumnBundleColumnChunkSummary) {
    fp_u32(bytes, chunk.row_group_id);
    fp_u32(bytes, chunk.column_id);
    fp_str(bytes, &chunk.column_name);
    fp_column_physical_type(bytes, chunk.physical_type);
    fp_column_logical_kind(bytes, chunk.logical_kind);
    fp_option_u32(bytes, chunk.fixed_binary_width);
    fp_chunk_codec(bytes, chunk.codec);
    fp_u64(bytes, chunk.row_count);
    fp_u64(bytes, chunk.compressed_bytes);
    fp_u64(bytes, chunk.uncompressed_bytes);
    fp_body_ref_summary(bytes, chunk.value_ref);
    fp_option_body_ref_summary(bytes, chunk.validity_ref);
}

pub(super) fn fp_column_stats_summary(bytes: &mut Vec<u8>, stat: &ColumnBundleColumnStatsSummary) {
    fp_u32(bytes, stat.row_group_id);
    fp_u32(bytes, stat.column_id);
    fp_str(bytes, &stat.column_name);
    fp_column_physical_type(bytes, stat.physical_type);
    fp_u32(bytes, stat.null_count);
    fp_predicate_value(bytes, stat.min);
    fp_predicate_value(bytes, stat.max);
}

pub(super) fn fp_body_ref_summary(bytes: &mut Vec<u8>, body_ref: ColumnBundleBodyRefSummary) {
    fp_u64(bytes, body_ref.offset);
    fp_u64(bytes, body_ref.length);
    fp_body_kind(bytes, body_ref.kind);
    fp_u16(bytes, body_ref.flags);
    fp_checksum_kind(bytes, body_ref.checksum_kind);
    fp_u32(bytes, body_ref.checksum);
}

pub(super) fn fp_option_body_ref_summary(
    bytes: &mut Vec<u8>,
    value: Option<ColumnBundleBodyRefSummary>,
) {
    match value {
        Some(value) => {
            fp_u8(bytes, 1);
            fp_body_ref_summary(bytes, value);
        }
        None => fp_u8(bytes, 0),
    }
}

pub(super) fn fp_column_physical_type(bytes: &mut Vec<u8>, value: ColumnPhysicalType) {
    match value {
        ColumnPhysicalType::I32 => fp_u8(bytes, 1),
        ColumnPhysicalType::I64 => fp_u8(bytes, 2),
        ColumnPhysicalType::F32 => fp_u8(bytes, 3),
        ColumnPhysicalType::F64 => fp_u8(bytes, 4),
        ColumnPhysicalType::FixedBinary { width } => {
            fp_u8(bytes, 5);
            fp_u32(bytes, width);
        }
    }
}

pub(super) fn fp_column_logical_kind(bytes: &mut Vec<u8>, value: ColumnLogicalKind) {
    fp_u8(
        bytes,
        match value {
            ColumnLogicalKind::Plain => 0,
            ColumnLogicalKind::TimestampNanosLike => 1,
            ColumnLogicalKind::ScaledInteger => 2,
            ColumnLogicalKind::DictionaryCode => 3,
            ColumnLogicalKind::EnumCode => 4,
            ColumnLogicalKind::OpaqueKey => 5,
        },
    );
}

pub(super) fn fp_dictionary_value_kind(bytes: &mut Vec<u8>, value: DictionaryValueKind) {
    fp_u8(
        bytes,
        match value {
            DictionaryValueKind::Utf8 => 1,
            DictionaryValueKind::Bytes => 2,
            DictionaryValueKind::FixedBytes => 3,
            DictionaryValueKind::EnumLabels => 4,
        },
    );
}

pub(super) fn fp_ordering_direction(bytes: &mut Vec<u8>, value: BundleOrderingDirection) {
    fp_u8(
        bytes,
        match value {
            BundleOrderingDirection::Ascending => 1,
            BundleOrderingDirection::Descending => 2,
        },
    );
}

pub(super) fn fp_null_order(bytes: &mut Vec<u8>, value: BundleNullOrder) {
    fp_u8(
        bytes,
        match value {
            BundleNullOrder::NullsFirst => 1,
            BundleNullOrder::NullsLast => 2,
            BundleNullOrder::NoNulls => 3,
        },
    );
}

pub(super) fn fp_chunk_codec(bytes: &mut Vec<u8>, value: ColumnBundleColumnChunkSummaryCodec) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleColumnChunkSummaryCodec::None => 0,
            ColumnBundleColumnChunkSummaryCodec::Zstd => 1,
        },
    );
}

pub(super) fn fp_body_kind(bytes: &mut Vec<u8>, value: ColumnBundleBodyKind) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleBodyKind::Unknown => 0,
            ColumnBundleBodyKind::Root => 1,
            ColumnBundleBodyKind::Schema => 2,
            ColumnBundleBodyKind::DictionaryIndex => 3,
            ColumnBundleBodyKind::DictionaryValues => 4,
            ColumnBundleBodyKind::RowGroupIndex => 5,
            ColumnBundleBodyKind::OrderingProof => 6,
            ColumnBundleBodyKind::ColumnChunk => 7,
            ColumnBundleBodyKind::StringTable => 8,
            ColumnBundleBodyKind::DebugJsonMetadata => 9,
            ColumnBundleBodyKind::ValidityBitmap => 10,
            ColumnBundleBodyKind::KeyTuple => 11,
            ColumnBundleBodyKind::RowGroupIndexDelta => 12,
        },
    );
}

pub(super) fn fp_checksum_kind(bytes: &mut Vec<u8>, value: ColumnBundleChecksumKind) {
    fp_u8(
        bytes,
        match value {
            ColumnBundleChecksumKind::None => 0,
            ColumnBundleChecksumKind::Crc32c => 1,
        },
    );
}

pub(super) fn fp_predicate_value(bytes: &mut Vec<u8>, value: ColumnPredicateValue) {
    match value {
        ColumnPredicateValue::I32(value) => {
            fp_u8(bytes, 1);
            fp_i32(bytes, value);
        }
        ColumnPredicateValue::I64(value) => {
            fp_u8(bytes, 2);
            fp_i64(bytes, value);
        }
        ColumnPredicateValue::F32(value) => {
            fp_u8(bytes, 3);
            fp_u32(bytes, value.to_bits());
        }
        ColumnPredicateValue::F64(value) => {
            fp_u8(bytes, 4);
            fp_u64(bytes, value.to_bits());
        }
    }
}

pub(super) fn fp_option_u32(bytes: &mut Vec<u8>, value: Option<u32>) {
    match value {
        Some(value) => {
            fp_u8(bytes, 1);
            fp_u32(bytes, value);
        }
        None => fp_u8(bytes, 0),
    }
}

pub(super) fn fp_str(bytes: &mut Vec<u8>, value: &str) {
    fp_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

pub(super) fn fp_bool(bytes: &mut Vec<u8>, value: bool) {
    fp_u8(bytes, u8::from(value));
}

pub(super) fn fp_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

pub(super) fn fp_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn fp_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn fp_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn fp_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn fp_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn fp_hex(bytes: &[u8]) -> String {
    format!("{:08x}", crc32c(bytes))
}

pub(super) fn column_chunk_compressed_payload_bytes(value_ref: OcbBodyRefV2) -> Result<u64> {
    let object_overhead = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    value_ref
        .length
        .checked_sub(object_overhead)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB column chunk body reference is too short",
        ))
}

pub(super) fn row_group_matches_predicates(
    metadata: &OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
    predicates: &[ResolvedRowGroupPredicate],
) -> Result<bool> {
    if predicates.is_empty() {
        return Ok(true);
    }
    let stats_by_column = stats_by_column_for_row_group(metadata, row_group)?;

    for predicate in predicates {
        let Some(stat) = stats_by_column.get(&predicate.column_id) else {
            // Missing stats are conservative: keep the row group.
            continue;
        };
        if scalar_column_physical_type(stat.physical_type)? != predicate.physical_type {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB stat dtype does not match predicate column dtype",
            ));
        }
        let min = ColumnPredicateValue::from_stat(stat.min_value);
        let max = ColumnPredicateValue::from_stat(stat.max_value);
        if let Some(lower) = predicate.lower {
            if max.cmp_same_type(lower)? == Ordering::Less {
                return Ok(false);
            }
        }
        if let Some(upper) = predicate.upper {
            if min.cmp_same_type(upper)? == Ordering::Greater {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

pub(super) fn column_chunks_for_row_group(
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

pub(super) fn stats_by_column_for_row_group<'a>(
    metadata: &'a OcbMetadataV1,
    row_group: &OcbRowGroupDescV1,
) -> Result<HashMap<u32, &'a OcbColumnStatsV1>> {
    let stats = stats_for_row_group(metadata, row_group.stat_begin, row_group.stat_count)?;
    let mut stats_by_column = try_column_bundle_hash_map_with_capacity(stats.len())?;
    for stat in stats {
        if stat.row_group_id != row_group.row_group_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group stat references a different row group",
            ));
        }
        if stats_by_column.insert(stat.column_id, stat).is_some() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row group has duplicate stats for a column",
            ));
        }
    }
    Ok(stats_by_column)
}

pub(super) fn stats_for_row_group(
    metadata: &OcbMetadataV1,
    begin: u64,
    count: u32,
) -> Result<&[OcbColumnStatsV1]> {
    let begin = usize::try_from(begin).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file("OCB row-group stat descriptor begin is too large")
    })?;
    let count = count as usize;
    let end = begin
        .checked_add(count)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range overflows",
        ))?;
    metadata
        .row_group_index
        .stats
        .get(begin..end)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB row-group stat descriptor range is out of bounds",
        ))
}

pub(super) fn validate_metadata(metadata: &OcbMetadataV1) -> Result<()> {
    if metadata.root.column_count as usize != metadata.schema.columns.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root column_count does not match schema",
        ));
    }
    if metadata.root.row_group_count as usize != metadata.row_group_index.row_groups.len() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_group_count does not match row-group index",
        ));
    }
    if metadata.root.dictionary_count > 0 && metadata.dictionary_index.is_none() {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root dictionary_count requires dictionary index",
        ));
    }
    if let Some(dictionary_index) = &metadata.dictionary_index {
        if metadata.root.dictionary_count as usize != dictionary_index.dictionaries.len() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root dictionary_count does not match dictionary index",
            ));
        }
    }
    if let Some(ordering_proof) = &metadata.ordering_proof {
        if ordering_proof.row_group_proofs.len() != metadata.row_group_index.row_groups.len() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof row-group count does not match row-group index",
            ));
        }
    }
    let total_rows = metadata
        .row_group_index
        .row_groups
        .iter()
        .try_fold(0u64, |acc, row_group| acc.checked_add(row_group.row_count))
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB row_count overflows"))?;
    if total_rows != metadata.root.row_count {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB root row_count does not match row groups",
        ));
    }
    Ok(())
}
