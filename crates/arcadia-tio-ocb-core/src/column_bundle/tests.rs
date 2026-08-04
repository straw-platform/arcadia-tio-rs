use std::fs;
use std::io::{Cursor, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::thread;

use super::*;
use crate::format::{
    OCB_BOOTSTRAP_PAGE_V1_LEN, OCB_FORMAT_MAJOR_V2, OCB_NULL_U32, OCB_ROOT_V1_LEN, OCB_ROOT_V2_LEN,
    OcbBodyKindV1, OcbBodyRefV2, OcbBootstrapPageV1, OcbBootstrapPageV2, OcbChunkCodecV1,
    OcbColumnChunkDescV1, OcbColumnChunkObjectV1, OcbColumnDescV1, OcbColumnStatsV1,
    OcbDictionaryDescV1, OcbDictionaryIndexV1, OcbDictionaryValueKindV1, OcbDictionaryValuesV1,
    OcbNullOrderV1, OcbOrderingDirectionV1, OcbOrderingKeyV1, OcbOrderingProofV1,
    OcbPhysicalTypeV1, OcbRootSlotV2, OcbRootV1, OcbRootV2, OcbRowGroupDescV1, OcbRowGroupIndexV1,
    OcbRowGroupOrderingProofV1, OcbSchemaV1, OcbStatScalarV1, OcbStringTableV1, crc32c,
};
use crate::read::{
    reset_row_group_index_lookup_count_for_test, row_group_index_lookup_count_for_test,
    uncompressed_fixed_binary_direct_fill_count_for_test,
};

#[test]
fn column_bundle_opens_one_file_and_parallel_reads_projected_batches() {
    let path = fixture_path("column_bundle_parallel_read");
    write_fixture(&path);

    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    assert_eq!(bundle.row_count(), 6);
    assert_eq!(bundle.row_group_count(), 2);
    assert_eq!(bundle.columns()[2].name, "category_code");
    assert_eq!(
        bundle.columns()[2].logical_kind,
        ColumnLogicalKind::DictionaryCode
    );

    let batches = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key", "category_code"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("read batches");
    assert_eq!(batches.len(), 2);
    assert_eq!(batches[0].row_group_id, 0);
    assert_eq!(batches[1].row_group_id, 1);
    assert_eq!(batches[0].base_row, 0);
    assert_eq!(batches[1].base_row, 3);
    assert_eq!(
        batches[0].columns[0].values,
        PrimitiveColumnValues::I32(vec![10, 10, 10])
    );
    assert_eq!(
        batches[0].columns[1].values,
        PrimitiveColumnValues::I64(vec![100, 101, 102])
    );
    assert_eq!(
        batches[1].columns[2].values,
        PrimitiveColumnValues::I32(vec![1, 2, 2])
    );

    cleanup(&path);
}

#[cfg(unix)]
#[test]
fn opened_handle_reads_original_identity_after_atomic_path_replacement() {
    let path = fixture_path("column_bundle_bound_file_identity");
    let replacement = fixture_path("column_bundle_replacement_identity");
    cleanup(&path);
    cleanup(&replacement);
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            rows_per_group: Some(3),
            ..FixtureOptions::default()
        },
    );
    write_fixture_with_options(
        &replacement,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            rows_per_group: Some(4),
            ..FixtureOptions::default()
        },
    );

    let opened = ColumnBundleFile::open(&path).expect("open original OCB identity");
    let opened_clone = opened.clone();
    assert_eq!(opened.row_count(), 6);
    fs::rename(&replacement, &path).expect("atomically replace the OCB pathname");

    let batches = opened_clone
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("opened handle must continue reading the original identity");
    assert_eq!(batches.len(), 2);
    assert_eq!(
        batches[0].columns[0].values,
        PrimitiveColumnValues::I32(vec![10, 10, 10])
    );
    let validity = batches[0].columns[0]
        .validity
        .as_ref()
        .expect("original validity bitmap remains bound");
    assert_eq!(validity.row_count, 3);
    assert_eq!(validity.bytes, vec![0b0000_0101]);

    let reopened = ColumnBundleFile::open(&path).expect("reopen replacement OCB identity");
    assert_eq!(reopened.row_count(), 8);
    drop(reopened);
    drop(opened_clone);
    drop(opened);
    cleanup(&path);
}

#[cfg(target_os = "linux")]
#[test]
fn cloned_parallel_projected_nullable_reads_keep_descriptor_count_bounded() {
    use std::os::unix::fs::MetadataExt;

    let path = fixture_path("column_bundle_descriptor_pressure");
    cleanup(&path);
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            rows_per_group: Some(64),
            ..FixtureOptions::default()
        },
    );
    let identity = fs::metadata(&path).expect("stat descriptor fixture");
    let matching_descriptors = || {
        fs::read_dir("/proc/self/fd")
            .expect("read Linux descriptor directory")
            .filter_map(std::result::Result::ok)
            .filter_map(|entry| fs::metadata(entry.path()).ok())
            .filter(|metadata| metadata.dev() == identity.dev() && metadata.ino() == identity.ino())
            .count()
    };
    let baseline = matching_descriptors();

    let bundle = ColumnBundleFile::open(&path).expect("open descriptor fixture");
    let clones = (0..16).map(|_| bundle.clone()).collect::<Vec<_>>();
    assert_eq!(matching_descriptors(), baseline + 1);
    for clone in &clones {
        let batches = clone
            .read_batches(ColumnBundleReadRequest {
                projection: ColumnProjection::names([
                    "partition_key",
                    "order_key",
                    "category_code",
                ]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(4),
            })
            .expect("parallel projected nullable read");
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].columns.len(), 3);
        assert_eq!(
            batches[0].columns[0]
                .validity
                .as_ref()
                .expect("nullable projection validity")
                .row_count,
            64
        );
    }
    assert_eq!(matching_descriptors(), baseline + 1);

    drop(clones);
    assert_eq!(matching_descriptors(), baseline + 1);
    drop(bundle);
    assert_eq!(matching_descriptors(), baseline);
    cleanup(&path);
}

#[test]
fn column_bundle_resource_limits_cover_owned_and_streaming_reads() {
    let path = fixture_path("column_bundle_resource_limits");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            rows_per_group: Some(1_024),
            ..FixtureOptions::default()
        },
    );

    let default_bundle = ColumnBundleFile::open(&path).expect("open with Policy A");
    assert_eq!(
        default_bundle.resource_limits(),
        OcbResourceLimits::policy_a()
    );
    let default_plan = default_bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan fixture read");
    let footprints = default_plan
        .row_group_ids
        .iter()
        .map(|row_group_id| {
            selected_resource_footprint_for_row_group(
                &default_bundle.metadata,
                *row_group_id,
                &default_plan.projected_column_ids,
            )
            .expect("resource footprint")
        })
        .collect::<Vec<_>>();
    let max_row_decoded = footprints
        .iter()
        .map(|footprint| footprint.decoded_materialized_bytes)
        .max()
        .expect("fixture row group");
    let max_row_compressed = footprints
        .iter()
        .map(|footprint| footprint.compressed_bytes)
        .max()
        .expect("fixture row group");
    let total_compressed = footprints
        .iter()
        .map(|footprint| footprint.compressed_bytes)
        .sum::<u64>();
    let total_decoded = footprints
        .iter()
        .map(|footprint| footprint.decoded_materialized_bytes)
        .sum::<u64>();
    let open_auxiliary_encoded_bytes = default_bundle.metadata.open_auxiliary_encoded_bytes;
    let open_metadata_materialized_bytes = default_bundle.open_metadata_materialized_bytes;
    assert!(total_compressed > open_auxiliary_encoded_bytes);
    assert!(total_decoded > open_metadata_materialized_bytes);
    let exact_open_limits = OcbResourceLimits::policy_a()
        .with_max_owned_selected_compressed_bytes(open_auxiliary_encoded_bytes)
        .expect("exact open auxiliary-object limit")
        .with_max_owned_decoded_materialized_bytes(open_metadata_materialized_bytes)
        .expect("exact open metadata limit");
    let exact_open_bundle = ColumnBundleFile::open_with_resource_limits(&path, exact_open_limits)
        .expect("open at exact independent open limits");
    assert_eq!(
        exact_open_bundle.metadata.open_auxiliary_encoded_bytes,
        open_auxiliary_encoded_bytes
    );
    assert_eq!(
        exact_open_bundle.open_metadata_materialized_bytes,
        open_metadata_materialized_bytes
    );
    let auxiliary_open_err = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact_open_limits
            .with_max_owned_selected_compressed_bytes(open_auxiliary_encoded_bytes - 1)
            .expect("open auxiliary-object limit+1 policy"),
    )
    .expect_err("open auxiliary-object limit+1 must fail independently");
    assert_eq!(
        auxiliary_open_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    let metadata_open_err = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact_open_limits
            .with_max_owned_decoded_materialized_bytes(open_metadata_materialized_bytes - 1)
            .expect("open metadata limit+1 policy"),
    )
    .expect_err("open metadata limit+1 must fail independently");
    assert_eq!(
        metadata_open_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    let single_column_decoded = selected_resource_footprint_for_row_group(
        &default_bundle.metadata,
        default_plan.row_group_ids[0],
        &[0],
    )
    .expect("single-column resource footprint")
    .decoded_materialized_bytes;
    let object_overhead = u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4;
    let max_chunk_compressed = default_bundle
        .metadata
        .row_group_index
        .column_chunks
        .iter()
        .map(|chunk| chunk.value_ref.length - object_overhead)
        .max()
        .expect("fixture chunk");
    let max_chunk_decoded = default_bundle
        .metadata
        .row_group_index
        .column_chunks
        .iter()
        .map(|chunk| chunk.uncompressed_bytes)
        .max()
        .expect("fixture chunk");
    let mut max_encoded_object = OCB_ROOT_V1_LEN as u64;
    for reference in [
        default_bundle.metadata.root.schema_ref,
        default_bundle.metadata.root.dictionary_index_ref,
        default_bundle.metadata.root.row_group_index_ref,
        default_bundle.metadata.root.ordering_proof_ref,
        default_bundle.metadata.root.debug_json_ref,
        default_bundle.metadata.schema.string_table_ref,
    ] {
        if !reference.is_null() {
            max_encoded_object = max_encoded_object.max(reference.length);
        }
    }
    if let Some(index) = &default_bundle.metadata.dictionary_index {
        for dictionary in &index.dictionaries {
            max_encoded_object = max_encoded_object.max(dictionary.values_ref.length);
        }
    }
    for row_group in &default_bundle.metadata.row_group_index.row_groups {
        for reference in [row_group.first_key_tuple_ref, row_group.last_key_tuple_ref] {
            if !reference.is_null() {
                max_encoded_object = max_encoded_object.max(reference.length);
            }
        }
    }
    for chunk in &default_bundle.metadata.row_group_index.column_chunks {
        max_encoded_object = max_encoded_object.max(chunk.value_ref.length);
        if !chunk.validity_ref.is_null() {
            max_encoded_object = max_encoded_object.max(chunk.validity_ref.length);
        }
    }

    let exact = OcbResourceLimits::new(
        max_encoded_object,
        max_chunk_compressed,
        max_chunk_decoded,
        max_row_decoded,
        total_compressed,
        total_decoded,
    )
    .expect("exact fixture limits");
    let exact_bundle = ColumnBundleFile::open_with_options_and_resource_limits(
        &path,
        ColumnBundleOpenOptions::default(),
        exact,
    )
    .expect("open with exact limits");
    assert_eq!(exact_bundle.resource_limits(), exact);
    let exact_plan = exact_bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan exact read");
    assert_eq!(
        exact_bundle
            .read_plan_batches(&exact_plan)
            .expect("read at exact limits")
            .batches
            .len(),
        2
    );
    exact_bundle
        .dictionary_values(0)
        .expect("dictionary cold path honors exact custom limits");
    let first_row_count =
        usize::try_from(default_bundle.metadata.row_group_index.row_groups[0].row_count)
            .expect("fixture row count fits usize");
    let mut exact_fill_values = vec![0i32; first_row_count];
    exact_bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: Some(0),
                values: PrimitiveColumnValuesMut::I32(&mut exact_fill_values),
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect("direct fill honors exact custom limits");
    let fill_limited = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_projected_row_group_bytes(single_column_decoded - 1)
            .expect("direct-fill limit+1 policy"),
    )
    .expect("direct-fill budget is projection-specific");
    let mut limited_fill_values = vec![0i32; first_row_count];
    let fill_err = fill_limited
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: Some(0),
                values: PrimitiveColumnValuesMut::I32(&mut limited_fill_values),
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("direct-fill limit+1 must fail before payload I/O");
    assert_eq!(
        fill_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    assert_eq!(
        exact_bundle
            .read_plan_batches_with_attribution(&exact_plan)
            .expect("attributed read at exact limits")
            .outcome
            .batches
            .len(),
        2
    );

    let object_err = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_encoded_object_bytes(max_encoded_object - 1)
            .expect("encoded-object limit+1 policy"),
    )
    .expect_err("encoded object limit+1 must fail at open");
    assert_eq!(
        object_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    let compressed_err = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_compressed_chunk_bytes(max_chunk_compressed - 1)
            .expect("compressed limit+1 policy"),
    )
    .expect_err("compressed limit+1 must fail at open");
    assert_eq!(
        compressed_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    let decoded_err = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_decompressed_chunk_bytes(max_chunk_decoded - 1)
            .expect("decoded limit+1 policy"),
    )
    .expect_err("decoded limit+1 must fail at open");
    assert_eq!(
        decoded_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    let row_limited = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_projected_row_group_bytes(max_row_decoded - 1)
            .expect("row limit+1 policy"),
    )
    .expect("row budget is projection-specific");
    let row_plan = row_limited
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan row-limited read");
    let row_err = row_limited
        .read_plan_batches(&row_plan)
        .expect_err("projected row limit+1 must fail before reads");
    assert_eq!(
        row_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    let compressed_owned_bundle = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_owned_selected_compressed_bytes(total_compressed - 1)
            .expect("owned compressed limit+1 policy"),
    )
    .expect("owned compressed budget is request-specific");
    let compressed_owned_plan = compressed_owned_bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan compressed-owned-limited read");
    let compressed_owned_err = compressed_owned_bundle
        .read_plan_batches(&compressed_owned_plan)
        .expect_err("owned compressed limit+1 must fail");
    assert_eq!(
        compressed_owned_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    let decoded_owned_bundle = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact
            .with_max_owned_decoded_materialized_bytes(total_decoded - 1)
            .expect("owned decoded limit+1 policy"),
    )
    .expect("owned decoded budget is request-specific");
    let decoded_owned_plan = decoded_owned_bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan decoded-owned-limited read");
    let decoded_owned_err = decoded_owned_bundle
        .read_plan_batches(&decoded_owned_plan)
        .expect_err("owned decoded limit+1 must fail");
    assert_eq!(
        decoded_owned_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    let streaming_limits = exact
        .with_max_owned_selected_compressed_bytes(max_row_compressed)
        .expect("streaming compressed budget")
        .with_max_owned_decoded_materialized_bytes(max_row_decoded)
        .expect("streaming decoded budget");
    let streaming_bundle = ColumnBundleFile::open_with_resource_limits(&path, streaming_limits)
        .expect("open streaming limits");
    assert_eq!(streaming_bundle.resource_limits(), streaming_limits);
    let streaming_plan = streaming_bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::All,
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan streaming read");
    let owned_err = streaming_bundle
        .read_plan_batches(&streaming_plan)
        .expect_err("owned lifetime total must fail");
    assert_eq!(
        owned_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    assert_eq!(
        streaming_bundle
            .read_plan_row_groups(&streaming_plan, &[streaming_plan.row_group_ids[0]])
            .expect("one-row-group subset fits streaming budget")
            .batches
            .len(),
        1
    );
    let mut visited = 0usize;
    let report = streaming_bundle
        .visit_plan_batches(
            &streaming_plan,
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ..ColumnBundleReadCursorOptions::default()
            },
            |_| {
                visited += 1;
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("streaming path charges one simultaneous row group");
    assert_eq!(visited, 2);
    assert_eq!(report.batches_yielded, 2);
    assert_eq!(
        streaming_bundle
            .reusable_buffer_pool_for_plan(&streaming_plan, 1, false)
            .expect("one reusable slot fits one-row-group budget")
            .len(),
        1
    );
    let pool_err = streaming_bundle
        .reusable_buffer_pool_for_plan(&streaming_plan, 2, false)
        .expect_err("two reusable slots must exceed one-row-group budget");
    assert_eq!(
        pool_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    let huge_pool_err = streaming_bundle
        .reusable_buffer_pool_for_plan(&streaming_plan, usize::MAX, false)
        .expect_err("unbounded reusable slot request must fail before allocation");
    assert_eq!(
        huge_pool_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );

    cleanup(&path);
}

#[test]
fn sliding_in_flight_accounting_matches_ordered_scheduler_windows() {
    let limits = OcbResourceLimits::new(64, 64, 64, 64, 8, 8).expect("window limits");
    let footprints = [
        SelectedResourceFootprint {
            compressed_bytes: 8,
            decoded_materialized_bytes: 8,
            row_count: 1,
        },
        SelectedResourceFootprint::default(),
        SelectedResourceFootprint::default(),
        SelectedResourceFootprint {
            compressed_bytes: 8,
            decoded_materialized_bytes: 8,
            row_count: 1,
        },
    ];
    validate_sliding_resource_footprints(&footprints, 2, limits)
        .expect("every ordered two-row window fits");
    let err = validate_sliding_resource_footprints(&footprints, 4, limits)
        .expect_err("four-row window exceeds aggregate limits");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
}

#[test]
fn resource_accounting_includes_selected_validity_bytes() {
    let path = fixture_path("column_bundle_resource_validity");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            rows_per_group: Some(1_024),
            ..FixtureOptions::default()
        },
    );
    let bundle = ColumnBundleFile::open(&path).expect("open nullable fixture");
    let plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan nullable projection");
    let footprints = selected_resource_footprints_for_plan(&bundle.metadata, &plan)
        .expect("nullable footprints");
    for (row_group_id, footprint) in plan.row_group_ids.iter().zip(footprints.iter()) {
        let chunk = bundle
            .metadata
            .row_group_index
            .column_chunks
            .iter()
            .find(|chunk| chunk.row_group_id == *row_group_id && chunk.column_id == 0)
            .expect("nullable selected chunk");
        let encoded_payload =
            chunk.value_ref.length - (u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN) + 4);
        assert_eq!(
            footprint.compressed_bytes,
            encoded_payload + chunk.validity_ref.length
        );
        assert_eq!(
            footprint.decoded_materialized_bytes,
            chunk.uncompressed_bytes + chunk.validity_ref.length
        );
    }
    let max_row = footprints
        .iter()
        .map(|footprint| footprint.decoded_materialized_bytes)
        .max()
        .unwrap();
    let total_compressed = footprints
        .iter()
        .map(|footprint| footprint.compressed_bytes)
        .sum();
    let total_decoded = footprints
        .iter()
        .map(|footprint| footprint.decoded_materialized_bytes)
        .sum();
    let limits = OcbResourceLimits::policy_a()
        .with_max_projected_row_group_bytes(max_row)
        .unwrap()
        .with_max_owned_selected_compressed_bytes(total_compressed)
        .unwrap()
        .with_max_owned_decoded_materialized_bytes(total_decoded)
        .unwrap();
    let exact = ColumnBundleFile::open_with_resource_limits(&path, limits)
        .expect("open exact validity policy");
    let exact_plan = exact
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .unwrap();
    assert_eq!(
        exact.read_plan_batches(&exact_plan).unwrap().batches.len(),
        2
    );
    cleanup(&path);
}

#[test]
fn v1_and_v2_full_payload_validate_zstd_payloads() {
    for (fixture_name, rewrite_as_v2) in [
        ("column_bundle_v1_full_payload_zstd", false),
        ("column_bundle_v2_full_payload_zstd", true),
    ] {
        let path = fixture_path(fixture_name);
        write_summary_fixture(&path);
        if rewrite_as_v2 {
            rewrite_fixture_as_v2_with_dual_roots(&path);
        }

        ColumnBundleFile::open_with_options_and_resource_limits(
            &path,
            ColumnBundleOpenOptions {
                validation: ColumnBundleOpenValidation::FullPayload,
            },
            OcbResourceLimits::policy_a(),
        )
        .expect("valid zstd fixture must pass FullPayload validation");
        let bundle = ColumnBundleFile::open(&path).expect("open fixture metadata graph");
        let zstd_chunk = bundle
            .metadata
            .row_group_index
            .column_chunks
            .iter()
            .find(|chunk| chunk.codec == OcbChunkCodecV1::Zstd)
            .expect("zstd fixture chunk");
        let payload_offset =
            zstd_chunk.value_ref.offset + u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN);
        drop(bundle);

        let mut file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .expect("open fixture for corruption");
        file.seek(SeekFrom::Start(payload_offset))
            .expect("seek zstd payload");
        let mut byte = [0u8; 1];
        std::io::Read::read_exact(&mut file, &mut byte).expect("read zstd payload byte");
        file.seek(SeekFrom::Start(payload_offset))
            .expect("seek zstd payload again");
        file.write_all(&[byte[0] ^ 0x5a])
            .expect("corrupt zstd payload byte");
        drop(file);

        ColumnBundleFile::open(&path).expect("metadata graph does not read payload bodies");
        let err = ColumnBundleFile::open_with_options_and_resource_limits(
            &path,
            ColumnBundleOpenOptions {
                validation: ColumnBundleOpenValidation::FullPayload,
            },
            OcbResourceLimits::policy_a(),
        )
        .expect_err("FullPayload must validate zstd payload bytes");
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));

        cleanup(&path);
    }
}

#[test]
fn column_bundle_opens_v2_latest_root_slot() {
    let path = fixture_path("column_bundle_v2_latest_root");
    write_fixture(&path);
    rewrite_fixture_as_v2_with_dual_roots(&path);

    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    assert_eq!(bundle.row_count(), 6);
    assert_eq!(bundle.row_group_count(), 2);
    assert_eq!(bundle.columns()[2].name, "category_code");

    let batches = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key"]),
            predicates: vec![RowGroupPredicate::equal(
                "partition_key",
                ColumnPredicateValue::I32(11),
            )],
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("read latest v2 root");
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].row_group_id, 1);
    assert_eq!(
        batches[0].columns[1].values,
        PrimitiveColumnValues::I64(vec![200, 201, 202])
    );

    cleanup(&path);
}

#[test]
fn v2_fallback_propagates_policy_and_unsupported_errors() {
    let corrupt_path = fixture_path("column_bundle_v2_corrupt_latest_fallback");
    write_fixture(&corrupt_path);
    rewrite_fixture_as_v2_with_latest_case(&corrupt_path, V2LatestFixtureCase::Corrupt);
    let fallback = ColumnBundleFile::open(&corrupt_path)
        .expect("corrupt latest candidate must fall back to valid older root");
    assert_eq!(fallback.metadata.root_generation, 1);
    cleanup(&corrupt_path);

    let policy_path = fixture_path("column_bundle_v2_policy_no_fallback");
    write_fixture(&policy_path);
    rewrite_fixture_as_v2_with_latest_case(&policy_path, V2LatestFixtureCase::DebugObject(4096));
    let exact = OcbResourceLimits::policy_a()
        .with_max_encoded_object_bytes(4096)
        .expect("exact debug-object limit");
    let latest = ColumnBundleFile::open_with_resource_limits(&policy_path, exact)
        .expect("exact debug-object limit must select latest root");
    assert_eq!(latest.metadata.root_generation, 2);
    let policy_err = ColumnBundleFile::open_with_resource_limits(
        &policy_path,
        exact
            .with_max_encoded_object_bytes(4095)
            .expect("debug-object limit+1 policy"),
    )
    .expect_err("policy failure must not fall back to an older root");
    assert_eq!(
        policy_err.ocb_failure_cause(),
        Some(OcbFailureCause::InvalidInput)
    );
    cleanup(&policy_path);

    let unsupported_path = fixture_path("column_bundle_v2_unsupported_no_fallback");
    write_fixture(&unsupported_path);
    rewrite_fixture_as_v2_with_latest_case(
        &unsupported_path,
        V2LatestFixtureCase::UnsupportedVersion,
    );
    let unsupported_err = ColumnBundleFile::open(&unsupported_path)
        .expect_err("unsupported latest root must not fall back");
    assert_eq!(
        unsupported_err.ocb_failure_cause(),
        Some(OcbFailureCause::UnsupportedFormat)
    );
    cleanup(&unsupported_path);
}

#[test]
fn column_bundle_decodes_dictionary_values_on_cold_path() {
    let path = fixture_path("column_bundle_dictionary_values");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let dictionary = bundle.dictionary_values(0).expect("decode dictionary");
    assert_eq!(dictionary.dictionary_id, 0);
    assert_eq!(dictionary.name, "category_dictionary");
    assert_eq!(dictionary.value_kind, DictionaryValueKind::Utf8);
    assert_eq!(
        dictionary.values,
        DictionaryValues::Utf8(vec!["alpha".into(), "beta".into()])
    );

    let dictionary_materialized_bytes = 2 * std::mem::size_of::<Vec<u8>>() as u64
        + (b"alpha".len() + b"beta".len()) as u64
        + 2 * std::mem::size_of::<String>() as u64
        + "category_dictionary".len() as u64;
    let mut exact_bundle = bundle.clone();
    exact_bundle.resource_limits = exact_bundle
        .resource_limits
        .with_max_owned_decoded_materialized_bytes(dictionary_materialized_bytes)
        .expect("exact dictionary materialization limit");
    exact_bundle
        .dictionary_values(0)
        .expect("dictionary cold path fits exact materialization limit");
    let mut limited_bundle = bundle;
    limited_bundle.resource_limits = limited_bundle
        .resource_limits
        .with_max_owned_decoded_materialized_bytes(dictionary_materialized_bytes - 1)
        .expect("dictionary materialization limit+1 policy");
    let err = limited_bundle
        .dictionary_values(0)
        .expect_err("dictionary cold path limit+1 must fail");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));

    cleanup(&path);
}

#[test]
fn fixed_projection_resource_limits_charge_repeated_fields() {
    let path = fixture_path("column_bundle_fixed_projection_resource_limits");
    write_summary_fixture(&path);
    let default_bundle = ColumnBundleFile::open(&path).expect("open summary fixture");
    let request = ColumnBundleReadRequest {
        projection: ColumnProjection::names(["partition_key", "payload"]),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions::serial(),
    };
    let default_plan = default_bundle
        .plan_read(&request)
        .expect("plan summary fixture");
    let base_row_bytes =
        selected_resource_footprints_for_plan(&default_bundle.metadata, &default_plan)
            .expect("base projection footprints")
            .into_iter()
            .map(|footprint| footprint.decoded_materialized_bytes)
            .max()
            .expect("summary row group");
    let row_capacity = max_row_count_for_plan(&default_bundle.metadata, &default_plan)
        .expect("summary row capacity") as u64;
    let repeated_fields = 4u64;
    let projection = FixedBinaryRecordProjection::by_column_name("payload", 2).fields(
        (0..repeated_fields)
            .map(|index| {
                FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8)
                    .with_name(format!("duplicate_{index}"))
            })
            .collect::<Vec<_>>(),
    );
    let exact_projected_bytes = base_row_bytes + row_capacity * repeated_fields;
    let exact_limits = OcbResourceLimits::policy_a()
        .with_max_projected_row_group_bytes(exact_projected_bytes)
        .expect("exact fixed-projection policy");
    let exact_bundle = ColumnBundleFile::open_with_resource_limits(&path, exact_limits)
        .expect("open exact fixed-projection policy");
    let exact_plan = exact_bundle
        .plan_read(&request)
        .expect("plan exact projection");
    assert_eq!(
        exact_bundle
            .fixed_binary_projection_buffer_for_plan(&exact_plan, &projection)
            .expect("exact repeated-field projection")
            .fields
            .len(),
        repeated_fields as usize
    );

    let limited_bundle = ColumnBundleFile::open_with_resource_limits(
        &path,
        exact_limits
            .with_max_projected_row_group_bytes(exact_projected_bytes - 1)
            .expect("fixed-projection limit+1 policy"),
    )
    .expect("fixed-projection budget is request-specific");
    let limited_plan = limited_bundle
        .plan_read(&request)
        .expect("plan limited projection");
    let err = limited_bundle
        .fixed_binary_projection_buffer_for_plan(&limited_plan, &projection)
        .expect_err("repeated-field materialization limit+1 must fail");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
    cleanup(&path);
}

#[test]
fn column_bundle_prunes_row_groups_with_i32_stats_and_reports_plan() {
    let path = fixture_path("column_bundle_prune_i32");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let outcome = bundle
        .read_batches_with_report(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key"]),
            predicates: vec![RowGroupPredicate::equal(
                "partition_key",
                ColumnPredicateValue::I32(11),
            )],
            options: ColumnBundleReadOptions::parallel(4),
        })
        .expect("read pruned batches");

    assert_eq!(outcome.batches.len(), 1);
    assert_eq!(outcome.batches[0].row_group_id, 1);
    assert_eq!(
        outcome.batches[0].columns[0].values,
        PrimitiveColumnValues::I32(vec![11, 11, 11])
    );
    assert_eq!(outcome.report.requested_threads, 4);
    assert_eq!(outcome.report.effective_threads, 1);
    assert_eq!(outcome.report.selected_row_groups, 1);
    assert_eq!(outcome.report.pruned_row_groups, 1);
    assert_eq!(outcome.report.selected_column_chunks, 2);
    assert_eq!(
        outcome.report.fallback_reason,
        Some(OCB_FALLBACK_TOO_FEW_ROW_GROUPS)
    );
    cleanup(&path);
}

#[test]
fn column_bundle_prunes_row_groups_with_i64_range() {
    let path = fixture_path("column_bundle_prune_i64");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let outcome = bundle
        .read_batches_with_report(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["order_key"]),
            predicates: vec![RowGroupPredicate::between(
                "order_key",
                ColumnPredicateValue::I64(101),
                ColumnPredicateValue::I64(150),
            )],
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("read pruned batches");

    assert_eq!(outcome.batches.len(), 1);
    assert_eq!(outcome.batches[0].row_group_id, 0);
    assert_eq!(outcome.report.pruned_row_groups, 1);
    cleanup(&path);
}

#[test]
fn column_bundle_row_group_summaries_include_projected_chunks_stats_and_fixed_binary_bytes() {
    let path = fixture_path("column_bundle_row_group_summaries");
    write_summary_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB summary fixture");

    let summaries = bundle.row_group_summaries().expect("summarize row groups");
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].row_group_id, 0);
    assert_eq!(summaries[0].base_row, 0);
    assert_eq!(summaries[0].row_count, 3);
    assert_eq!(summaries[1].row_group_id, 1);
    assert_eq!(summaries[1].base_row, 3);
    assert_eq!(summaries[0].chunks.len(), 2);
    assert_eq!(summaries[0].stats.len(), 1);
    assert_eq!(summaries[0].stats[0].column_name, "partition_key");
    assert_eq!(summaries[0].stats[0].min, ColumnPredicateValue::I32(10));
    assert_eq!(summaries[0].stats[0].max, ColumnPredicateValue::I32(10));

    let payload = summaries[0]
        .chunks
        .iter()
        .find(|chunk| chunk.column_name == "payload")
        .expect("payload chunk summary");
    assert_eq!(
        payload.physical_type,
        ColumnPhysicalType::FixedBinary { width: 2 }
    );
    assert_eq!(payload.fixed_binary_width, Some(2));
    assert_eq!(payload.codec, ColumnBundleColumnChunkSummaryCodec::Zstd);
    assert_eq!(payload.row_count, 3);
    assert_eq!(payload.uncompressed_bytes, 6);
    assert!(payload.compressed_bytes > 0);
    assert_eq!(payload.value_ref.kind, ColumnBundleBodyKind::ColumnChunk);
    assert_eq!(
        payload.value_ref.checksum_kind,
        ColumnBundleChecksumKind::Crc32c
    );
    assert!(payload.validity_ref.is_none());

    let plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["payload"]),
            predicates: vec![RowGroupPredicate::equal(
                "partition_key",
                ColumnPredicateValue::I32(11),
            )],
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan payload read");
    let plan_summaries = bundle
        .read_plan_row_group_summaries(&plan)
        .expect("summarize plan row groups");
    assert_eq!(plan_summaries.len(), 1);
    assert_eq!(plan_summaries[0].row_group_id, 1);
    assert_eq!(plan_summaries[0].chunks.len(), 1);
    assert_eq!(plan_summaries[0].chunks[0].column_name, "payload");
    assert_eq!(plan_summaries[0].stats[0].column_name, "partition_key");

    let fingerprint = bundle.snapshot_fingerprint().expect("snapshot fingerprint");
    assert_eq!(
        fingerprint.algorithm,
        OCB_CERTIFICATION_FINGERPRINT_ALGORITHM
    );
    assert_eq!(fingerprint.schema.len(), 8);
    assert_eq!(fingerprint.row_groups.len(), 8);
    let reopened_fingerprint = ColumnBundleFile::open(&path)
        .expect("reopen summary fixture")
        .snapshot_fingerprint()
        .expect("reopened snapshot fingerprint");
    assert_eq!(fingerprint, reopened_fingerprint);

    let certification = bundle
        .read_plan_certification(&plan)
        .expect("plan certification");
    assert_eq!(certification.snapshot_fingerprint, fingerprint);
    assert_eq!(certification.root_generation, 0);
    assert_eq!(certification.row_count, 6);
    assert_eq!(certification.row_group_count, 2);
    assert_eq!(certification.report.selected_row_groups, 1);
    assert_eq!(certification.row_groups.len(), 1);
    assert_eq!(certification.row_groups[0].row_group_id, 1);
    assert_eq!(certification.selected_uncompressed_bytes, 6);
    assert!(certification.selected_compressed_bytes > 0);
    assert_eq!(certification.selected_chunk_fingerprint.len(), 8);
    let reopened_certification = ColumnBundleFile::open(&path)
        .expect("reopen certification fixture")
        .read_plan_certification(&plan)
        .expect("reopened plan certification");
    assert_eq!(reopened_certification, certification);

    let full_plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "payload"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("plan fixed-binary projection read");
    let projection = FixedBinaryRecordProjection::by_column_name("payload", 2)
        .field(FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8).with_name("first"))
        .field(FixedBinaryProjectedField::new(1, FixedBinaryFieldType::U8).with_name("second"));
    let mut reusable = bundle
        .reusable_buffer_pool_for_plan(&full_plan, 2, false)
        .expect("reusable pool");
    let mut projection_buffer = bundle
        .fixed_binary_projection_buffer_for_plan(&full_plan, &projection)
        .expect("fixed-binary projection buffer");
    let mut visited = Vec::new();
    let projected = bundle
        .visit_plan_row_groups_project_fixed_binary_with_attribution(
            &full_plan,
            &[1, 0],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            &mut reusable,
            &projection,
            &mut projection_buffer,
            |batch, projected| {
                assert_eq!(batch.row_count(), projected.row_count);
                let first = projected.field_by_name("first")?;
                let second = projected.field(1)?;
                assert_eq!(first.values.field_type(), FixedBinaryFieldType::U8);
                visited.push((
                    projected.row_group_id,
                    first.values.as_u8()?.to_vec(),
                    second.values.as_u8()?.to_vec(),
                ));
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit fixed-binary projected row groups");
    assert_eq!(visited.len(), 2);
    assert_eq!(visited[0], (0, b"abc".to_vec(), b"abc".to_vec()));
    assert_eq!(visited[1], (1, b"def".to_vec(), b"def".to_vec()));
    assert_eq!(projected.cursor_report.batches_yielded, 2);
    assert_eq!(projected.cursor_report.rows_yielded, 6);
    assert_eq!(projected.cursor_report.max_in_flight_row_groups_observed, 2);
    assert_eq!(projected.attribution.selected_row_groups, 2);
    let _fixed_payload_decode_ns = projected.attribution.fixed_payload_decode_ns;
    let _copy_materialization_ns = projected.attribution.copy_materialization_ns;

    let missing_source_plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("plan without fixed-binary source");
    let missing_source_err = bundle
        .fixed_binary_projection_buffer_for_plan(&missing_source_plan, &projection)
        .expect_err("fixed-binary source must be part of the plan projection");
    assert!(
        missing_source_err
            .to_string()
            .contains("not in the read plan")
    );

    let wrong_width = FixedBinaryRecordProjection::by_column_name("payload", 3)
        .field(FixedBinaryProjectedField::new(0, FixedBinaryFieldType::U8));
    let wrong_width_err = bundle
        .fixed_binary_projection_buffer_for_plan(&full_plan, &wrong_width)
        .expect_err("wrong fixed-binary width rejected");
    assert!(wrong_width_err.to_string().contains("expected_width"));

    let field_overrun = FixedBinaryRecordProjection::by_column_name("payload", 2).field(
        FixedBinaryProjectedField::new(1, FixedBinaryFieldType::U16Le),
    );
    let field_overrun_err = bundle
        .fixed_binary_projection_buffer_for_plan(&full_plan, &field_overrun)
        .expect_err("field overrun rejected before payload reads");
    assert!(
        field_overrun_err
            .to_string()
            .contains("extends past record width")
    );

    let mut bad_projection_buffer = projection_buffer.clone();
    bad_projection_buffer.fields[0].offset = 99;
    let mut bad_reusable = bundle
        .reusable_buffer_pool_for_plan(&full_plan, 1, false)
        .expect("bad reusable pool");
    let mut callbacks = 0usize;
    let bad_buffer_err = bundle
        .visit_plan_row_groups_project_fixed_binary_with_attribution(
            &full_plan,
            &[0],
            ColumnBundleReadCursorOptions::default(),
            &mut bad_reusable,
            &projection,
            &mut bad_projection_buffer,
            |_, _| {
                callbacks = callbacks.saturating_add(1);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect_err("mismatched projected-field buffer rejected before callbacks");
    assert_eq!(callbacks, 0);
    assert!(
        bad_buffer_err
            .to_string()
            .contains("buffer field does not match")
    );

    cleanup(&path);
}

#[test]
fn fixed_binary_record_projection_decodes_little_endian_fields() {
    let width = 24usize;
    let mut bytes = vec![0u8; width * 2];
    for (row, (kind, side, key, sequence, ordinal)) in [
        (7u8, -3i8, 1234i32, 10_000_000_001i64, 42u64),
        (9u8, 4i8, -5678i32, -10_000_000_002i64, 43u64),
    ]
    .into_iter()
    .enumerate()
    {
        let base = row * width;
        bytes[base] = kind;
        bytes[base + 1] = side as u8;
        bytes[base + 4..base + 8].copy_from_slice(&key.to_le_bytes());
        bytes[base + 8..base + 16].copy_from_slice(&sequence.to_le_bytes());
        bytes[base + 16..base + 24].copy_from_slice(&ordinal.to_le_bytes());
    }

    let primitive = PrimitiveColumnValuesRef::FixedBinary {
        width: width as u32,
        bytes: bytes.as_slice(),
    };
    let records = primitive
        .fixed_binary_records()
        .expect("fixed-binary record view");
    assert_eq!(records.len(), 2);
    assert_eq!(records.row(1).expect("second row")[0], 9);

    let mut kinds = [0u8; 2];
    let mut sides = [0i8; 2];
    let mut keys = [0i32; 2];
    let mut sequences = [0i64; 2];
    let mut ordinals = [0u64; 2];
    let projected = records
        .project_fields(&mut [
            FixedBinaryFieldProjectionMut {
                offset: 0,
                values: FixedBinaryFieldValuesMut::U8(&mut kinds),
            },
            FixedBinaryFieldProjectionMut {
                offset: 1,
                values: FixedBinaryFieldValuesMut::I8(&mut sides),
            },
            FixedBinaryFieldProjectionMut {
                offset: 4,
                values: FixedBinaryFieldValuesMut::I32(&mut keys),
            },
            FixedBinaryFieldProjectionMut {
                offset: 8,
                values: FixedBinaryFieldValuesMut::I64(&mut sequences),
            },
            FixedBinaryFieldProjectionMut {
                offset: 16,
                values: FixedBinaryFieldValuesMut::U64(&mut ordinals),
            },
        ])
        .expect("project fixed-binary fields");
    assert_eq!(projected, 2);
    let mut reported_keys = [0i32; 2];
    let projection_report = records
        .project_fields_with_report(&mut [FixedBinaryFieldProjectionMut {
            offset: 4,
            values: FixedBinaryFieldValuesMut::I32(&mut reported_keys),
        }])
        .expect("project fixed-binary fields with report");
    assert_eq!(projection_report.rows_projected, 2);
    assert_eq!(projection_report.fields_projected, 1);
    assert_eq!(reported_keys, [1234, -5678]);
    let _projection_wall_ns = projection_report.projection_wall_ns;
    assert_eq!(kinds, [7, 9]);
    assert_eq!(sides, [-3, 4]);
    assert_eq!(keys, [1234, -5678]);
    assert_eq!(sequences, [10_000_000_001, -10_000_000_002]);
    assert_eq!(ordinals, [42, 43]);

    let err =
        FixedBinaryRecordView::new(3, &[1, 2, 3, 4]).expect_err("unaligned record bytes rejected");
    assert!(err.to_string().contains("not aligned"));

    let mut too_small = [0i32; 1];
    let err = records
        .project_fields(&mut [FixedBinaryFieldProjectionMut {
            offset: 4,
            values: FixedBinaryFieldValuesMut::I32(&mut too_small),
        }])
        .expect_err("small output rejected");
    assert!(err.to_string().contains("output buffer is too small"));

    let mut out = [0i64; 2];
    let err = records
        .project_fields(&mut [FixedBinaryFieldProjectionMut {
            offset: 20,
            values: FixedBinaryFieldValuesMut::I64(&mut out),
        }])
        .expect_err("field overrun rejected");
    assert!(err.to_string().contains("extends past record width"));
}

#[test]
fn column_bundle_strict_planning_fails_closed_and_validates_summary_plan_ids() {
    let path = fixture_path("column_bundle_strict_planning");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let strict_plan = bundle
        .plan_read_strict(
            &ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: vec![RowGroupPredicate::equal(
                    "partition_key",
                    ColumnPredicateValue::I32(11),
                )],
                options: ColumnBundleReadOptions::parallel(4),
            },
            ColumnBundleStrictReadPlanningOptions::new(1),
        )
        .expect("strict plan with complete stats");
    assert_eq!(strict_plan.row_group_ids, vec![1]);
    assert_eq!(strict_plan.report.selected_row_groups, 1);
    assert_eq!(strict_plan.report.pruned_row_groups, 1);

    let broad_scan = bundle
        .plan_read_strict(
            &ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::serial(),
            },
            ColumnBundleStrictReadPlanningOptions::new(1),
        )
        .expect_err("strict plan rejects broad scan over cap");
    assert_eq!(
        broad_scan.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(broad_scan.to_string().contains("caller cap"));

    let missing_stats = bundle
        .plan_read_strict(
            &ColumnBundleReadRequest {
                projection: ColumnProjection::names(["category_code"]),
                predicates: vec![RowGroupPredicate::equal(
                    "category_code",
                    ColumnPredicateValue::I32(2),
                )],
                options: ColumnBundleReadOptions::serial(),
            },
            ColumnBundleStrictReadPlanningOptions::new(2),
        )
        .expect_err("strict plan rejects unavailable predicate stats");
    assert_eq!(
        missing_stats.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(missing_stats.to_string().contains("stats"));

    let mut duplicate_plan = strict_plan.clone();
    duplicate_plan.row_group_ids = vec![1, 1];
    let duplicate = bundle
        .read_plan_row_group_summaries(&duplicate_plan)
        .expect_err("duplicate summary plan row group ids reject");
    assert_eq!(
        duplicate.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(duplicate.to_string().contains("duplicate"));

    let mut unknown_plan = strict_plan.clone();
    unknown_plan.row_group_ids = vec![99];
    let unknown = bundle
        .read_plan_row_group_summaries(&unknown_plan)
        .expect_err("unknown summary plan row group ids reject");
    assert_eq!(
        unknown.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(unknown.to_string().contains("unknown row group"));

    cleanup(&path);

    let path = fixture_path("column_bundle_strict_fixed_binary_predicate");
    write_summary_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB summary fixture");
    let fixed_binary_predicate = bundle
        .plan_read_strict(
            &ColumnBundleReadRequest {
                projection: ColumnProjection::names(["payload"]),
                predicates: vec![RowGroupPredicate::equal(
                    "payload",
                    ColumnPredicateValue::I32(1),
                )],
                options: ColumnBundleReadOptions::serial(),
            },
            ColumnBundleStrictReadPlanningOptions::new(1),
        )
        .expect_err("strict plan rejects unsupported fixed-binary predicate metadata");
    assert_eq!(
        fixed_binary_predicate.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(fixed_binary_predicate.to_string().contains("fixed-binary"));
    cleanup(&path);
}

#[test]
fn column_bundle_default_reads_remain_conservative_without_strict_planning() {
    let path = fixture_path("column_bundle_default_read_after_strict");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let ordinary = bundle
        .read_batches_with_report(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["category_code"]),
            predicates: vec![RowGroupPredicate::equal(
                "category_code",
                ColumnPredicateValue::I32(2),
            )],
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("ordinary reads still keep row groups with missing stats");
    assert_eq!(ordinary.batches.len(), 2);
    assert_eq!(ordinary.report.selected_row_groups, 2);
    assert_eq!(ordinary.report.pruned_row_groups, 0);
    assert_eq!(ordinary.report.effective_threads, 2);
    assert_eq!(
        ordinary.batches[0].columns[0].values,
        PrimitiveColumnValues::I32(vec![1, 1, 2])
    );
    assert_eq!(
        ordinary.batches[1].columns[0].values,
        PrimitiveColumnValues::I32(vec![1, 2, 2])
    );

    let strict = bundle
        .plan_read_strict(
            &ColumnBundleReadRequest {
                projection: ColumnProjection::names(["category_code"]),
                predicates: vec![RowGroupPredicate::equal(
                    "category_code",
                    ColumnPredicateValue::I32(2),
                )],
                options: ColumnBundleReadOptions::parallel(2),
            },
            ColumnBundleStrictReadPlanningOptions::new(2),
        )
        .expect_err("strict helper fails where ordinary read remains conservative");
    assert_eq!(
        strict.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );

    cleanup(&path);
}

#[test]
fn column_bundle_executes_read_plan_and_row_group_subsets() {
    let path = fixture_path("column_bundle_read_plan_subset");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(4),
        })
        .expect("plan read");
    assert_eq!(plan.projected_column_ids, vec![0, 1]);
    assert_eq!(plan.row_group_ids, vec![0, 1]);
    assert_eq!(plan.report.selected_row_groups, 2);
    assert_eq!(plan.report.effective_threads, 2);

    let all = bundle.read_plan_batches(&plan).expect("execute plan");
    assert_eq!(all.batches.len(), 2);
    assert_eq!(all.batches[0].row_group_id, 0);
    assert_eq!(all.batches[1].row_group_id, 1);
    assert_eq!(all.report.selected_row_groups, 2);

    let subset = bundle
        .read_plan_row_groups(&plan, &[1, 0])
        .expect("execute subset in plan order");
    assert_eq!(subset.batches.len(), 2);
    assert_eq!(subset.batches[0].row_group_id, 0);
    assert_eq!(subset.batches[1].row_group_id, 1);

    let subset = bundle
        .read_plan_row_groups(&plan, &[1])
        .expect("execute one row group subset");
    assert_eq!(subset.batches.len(), 1);
    assert_eq!(subset.batches[0].row_group_id, 1);
    assert_eq!(subset.report.selected_row_groups, 1);
    assert_eq!(subset.report.effective_threads, 1);
    assert_eq!(subset.report.selected_column_chunks, 2);

    let mut visited = Vec::new();
    let visit_report = bundle
        .visit_plan_row_groups(
            &plan,
            &[1, 0],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit subset in plan order");
    assert_eq!(visited, vec![0, 1]);
    assert_eq!(visit_report.batches_yielded, 2);
    assert_eq!(visit_report.rows_yielded, 6);
    assert_eq!(visit_report.max_in_flight_row_groups_observed, 1);
    assert!(!visit_report.cancelled);
    assert_eq!(visit_report.base_report.selected_row_groups, 2);
    assert_eq!(visit_report.base_report.selected_column_chunks, 4);

    let mut visited = Vec::new();
    let visit_report = bundle
        .visit_plan_row_groups(
            &plan,
            &[1, 0],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(ColumnBundleVisitControl::Stop)
            },
        )
        .expect("cancel subset visitor");
    assert_eq!(visited, vec![0]);
    assert_eq!(visit_report.batches_yielded, 1);
    assert_eq!(visit_report.rows_yielded, 3);
    assert_eq!(visit_report.max_in_flight_row_groups_observed, 2);
    assert!(visit_report.cancelled);

    let mut visited = Vec::new();
    let attributed_visit = bundle
        .visit_plan_row_groups_with_attribution(
            &plan,
            &[1],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                thread::sleep(Duration::from_millis(1));
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit subset with attribution");
    assert_eq!(visited, vec![1]);
    assert_eq!(attributed_visit.cursor_report.batches_yielded, 1);
    assert_eq!(attributed_visit.cursor_report.rows_yielded, 3);
    assert_eq!(
        attributed_visit
            .cursor_report
            .max_in_flight_row_groups_observed,
        1
    );
    assert!(!attributed_visit.cursor_report.cancelled);
    assert_eq!(attributed_visit.attribution.selected_row_groups, 1);
    assert_eq!(attributed_visit.attribution.selected_column_chunks, 2);
    assert_eq!(attributed_visit.attribution.effective_threads, 1);
    assert!(attributed_visit.attribution.execute_wall_ns > 0);
    assert!(attributed_visit.attribution.callback_wall_ns > 0);
    assert!(attributed_visit.attribution.row_group_read_ns > 0);
    assert!(attributed_visit.attribution.read_io_ns > 0);
    assert!(attributed_visit.attribution.bytes_read > 0);
    assert!(attributed_visit.attribution.compressed_bytes > 0);
    assert!(attributed_visit.attribution.uncompressed_bytes > 0);
    assert_eq!(attributed_visit.attribution.native_to_c_copy_ns, None);
    assert_eq!(attributed_visit.attribution.wrapper_copy_ns, None);

    let mut pool = bundle
        .reusable_buffer_pool_for_plan(&plan, 2, false)
        .expect("allocate reusable buffers");
    let mut reusable_visited = Vec::new();
    let reusable_report = bundle
        .visit_plan_row_groups_into(
            &plan,
            &[1, 0],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            &mut pool,
            |view| {
                reusable_visited.push(view.row_group_id());
                assert_eq!(view.column_count(), 2);
                let partition = view.column(0)?;
                assert_eq!(partition.name, "partition_key");
                match partition.values {
                    PrimitiveColumnValuesRef::I32(values) if view.row_group_id() == 0 => {
                        assert_eq!(values, &[10, 10, 10]);
                    }
                    PrimitiveColumnValuesRef::I32(values) if view.row_group_id() == 1 => {
                        assert_eq!(values, &[11, 11, 11]);
                    }
                    _ => panic!("unexpected reusable partition values"),
                }
                assert!(partition.validity.is_none());
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit subset into reusable buffers");
    assert_eq!(reusable_visited, vec![0, 1]);
    assert_eq!(reusable_report.batches_yielded, 2);
    assert_eq!(reusable_report.rows_yielded, 6);
    assert_eq!(reusable_report.max_in_flight_row_groups_observed, 2);
    assert!(!reusable_report.cancelled);

    let mut mismatched_pool = pool.clone();
    mismatched_pool.buffers[0].columns[0].column_id = 999;
    let mismatch_err = bundle
        .visit_plan_row_groups_into(
            &plan,
            &[0],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            &mut mismatched_pool,
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("mismatched reusable buffers are rejected");
    assert!(
        mismatch_err
            .to_string()
            .contains("buffer column does not match plan projection")
    );

    let mut pool = bundle
        .reusable_buffer_pool_for_plan(&plan, 1, false)
        .expect("allocate attributed reusable buffers");
    let reusable_attributed = bundle
        .visit_plan_row_groups_into_with_attribution(
            &plan,
            &[1],
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            &mut pool,
            |view| {
                assert_eq!(view.row_group_id(), 1);
                thread::sleep(Duration::from_millis(1));
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit subset into reusable buffers with attribution");
    assert_eq!(reusable_attributed.cursor_report.batches_yielded, 1);
    assert_eq!(reusable_attributed.cursor_report.rows_yielded, 3);
    assert_eq!(
        reusable_attributed
            .cursor_report
            .max_in_flight_row_groups_observed,
        1
    );
    assert_eq!(reusable_attributed.attribution.selected_row_groups, 1);
    assert_eq!(reusable_attributed.attribution.selected_column_chunks, 2);
    assert!(reusable_attributed.attribution.callback_wall_ns > 0);
    assert!(reusable_attributed.attribution.row_group_read_ns > 0);
    assert!(reusable_attributed.attribution.read_io_ns > 0);

    let duplicate = bundle
        .read_plan_row_groups(&plan, &[1, 1])
        .expect_err("duplicate subset ids reject");
    assert_eq!(
        duplicate.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(
        duplicate
            .to_string()
            .contains(OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR)
    );

    let unknown = bundle
        .read_plan_row_groups(&plan, &[99])
        .expect_err("unknown subset ids reject");
    assert_eq!(
        unknown.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(
        unknown
            .to_string()
            .contains(OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR)
    );

    let duplicate = bundle
        .visit_plan_row_groups(
            &plan,
            &[1, 1],
            ColumnBundleReadCursorOptions::default(),
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("duplicate visitor subset ids reject");
    assert_eq!(
        duplicate.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(
        duplicate
            .to_string()
            .contains(OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR)
    );

    let unknown = bundle
        .visit_plan_row_groups(
            &plan,
            &[99],
            ColumnBundleReadCursorOptions::default(),
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("unknown visitor subset ids reject");
    assert_eq!(
        unknown.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(
        unknown
            .to_string()
            .contains(OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR)
    );

    cleanup(&path);
}

#[test]
fn retained_row_group_index_preserves_arbitrary_ids_order_errors_and_lookup_counts() {
    let path = fixture_path("column_bundle_retained_row_group_index");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            row_group_ids: Some([41, 7]),
            ..FixtureOptions::default()
        },
    );
    let bundle = ColumnBundleFile::open(&path).expect("open arbitrary-id OCB fixture");
    assert_eq!(bundle.metadata.row_group_positions_by_id.len(), 2);
    assert_eq!(bundle.metadata.row_group_positions_by_id.get(&41), Some(&0));
    assert_eq!(bundle.metadata.row_group_positions_by_id.get(&7), Some(&1));

    let plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("plan arbitrary-id fixture");
    assert_eq!(plan.row_group_ids, vec![41, 7]);

    reset_row_group_index_lookup_count_for_test();
    bundle
        .validate_read_plan(&plan)
        .expect("validate arbitrary-id plan");
    assert_eq!(row_group_index_lookup_count_for_test(), 2);

    reset_row_group_index_lookup_count_for_test();
    let tasks = bundle
        .bounded_ordered_tasks_for_plan(&plan)
        .expect("construct arbitrary-id scheduler tasks");
    assert_eq!(
        tasks
            .iter()
            .map(|task| task.row_group_id)
            .collect::<Vec<_>>(),
        vec![41, 7]
    );
    assert_eq!(row_group_index_lookup_count_for_test(), 2);

    reset_row_group_index_lookup_count_for_test();
    let summaries = bundle
        .build_row_group_summaries(plan.row_group_ids.iter().copied(), None)
        .expect("summarize arbitrary-id row groups");
    assert_eq!(
        summaries
            .iter()
            .map(|summary| summary.row_group_id)
            .collect::<Vec<_>>(),
        vec![41, 7]
    );
    assert_eq!(row_group_index_lookup_count_for_test(), 2);

    reset_row_group_index_lookup_count_for_test();
    let footprints = selected_resource_footprints_for_plan(&bundle.metadata, &plan)
        .expect("account arbitrary-id row groups");
    assert_eq!(footprints.len(), 2);
    assert_eq!(row_group_index_lookup_count_for_test(), 2);

    reset_row_group_index_lookup_count_for_test();
    let batch = read_row_group(
        &bundle.source,
        &bundle.metadata,
        &bundle.columns,
        41,
        &plan.projected_column_ids,
    )
    .expect("materialize arbitrary-id row group");
    assert_eq!(batch.row_group_id, 41);
    assert_eq!(row_group_index_lookup_count_for_test(), 1);

    reset_row_group_index_lookup_count_for_test();
    let mut values = vec![0i32; 3];
    let fill_report = bundle
        .read_row_group_into(
            7,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: Some(0),
                values: PrimitiveColumnValuesMut::I32(&mut values),
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect("fill arbitrary-id row group");
    assert_eq!(fill_report.row_group_id, 7);
    assert_eq!(values, vec![11, 11, 11]);
    assert_eq!(row_group_index_lookup_count_for_test(), 1);

    let subset = bundle
        .read_plan_row_groups(&plan, &[7, 41])
        .expect("restore reverse subset request to original plan order");
    assert_eq!(
        subset
            .batches
            .iter()
            .map(|batch| batch.row_group_id)
            .collect::<Vec<_>>(),
        vec![41, 7]
    );

    let mut unknown_first = plan.clone();
    unknown_first.row_group_ids = vec![99, 99];
    let err = bundle
        .validate_read_plan(&unknown_first)
        .expect_err("first unknown plan id precedes its later duplicate");
    assert!(
        err.to_string()
            .contains("OCB read plan references an unknown row group id")
    );

    let mut duplicate_first = plan.clone();
    duplicate_first.row_group_ids = vec![41, 41, 99];
    let err = bundle
        .validate_read_plan(&duplicate_first)
        .expect_err("earlier duplicate plan id precedes later unknown id");
    assert!(
        err.to_string()
            .contains("OCB read plan contains duplicate row group ids")
    );

    let err = selected_row_groups_for_plan(&bundle.metadata, &unknown_first)
        .expect_err("resource accounting preserves duplicate-before-unknown pre-scan");
    assert!(
        err.to_string()
            .contains("OCB read plan contains duplicate row group ids")
    );

    cleanup(&path);
}

#[test]
#[ignore = "Phase 5A source-free release-mode remote scaling gate"]
fn phase5a_row_group_index_scaling_current_control() {
    const ROW_GROUP_COUNT: usize = 16_384;
    const SELECTED_COUNT: usize = 4_096;
    const SAMPLES: usize = 5;

    let row_group_ids = (0..ROW_GROUP_COUNT)
        .map(|index| u32::try_from(index * 2 + 1).expect("synthetic id fits u32"))
        .collect::<Vec<_>>();
    let selected_ids = row_group_ids
        .iter()
        .rev()
        .step_by(4)
        .take(SELECTED_COUNT)
        .copied()
        .collect::<Vec<_>>();
    let positions = row_group_ids
        .iter()
        .copied()
        .enumerate()
        .map(|(position, row_group_id)| (row_group_id, position))
        .collect::<HashMap<_, _>>();

    let current = || {
        let started = Instant::now();
        let mut checksum = 0u64;
        let mut lookups = 0usize;
        for row_group_id in std::hint::black_box(&selected_ids) {
            let position = *positions
                .get(row_group_id)
                .expect("selected id exists in retained index");
            checksum = checksum.wrapping_add(position as u64);
            lookups += 1;
        }
        (started.elapsed(), std::hint::black_box(checksum), lookups)
    };
    let control = || {
        let started = Instant::now();
        let mut checksum = 0u64;
        let mut comparisons = 0usize;
        for selected_id in std::hint::black_box(&selected_ids) {
            let mut found = None;
            for (position, candidate) in row_group_ids.iter().enumerate() {
                comparisons += 1;
                if candidate == selected_id {
                    found = Some(position);
                    break;
                }
            }
            checksum = checksum
                .wrapping_add(found.expect("selected id exists in legacy descriptor scan") as u64);
        }
        (
            started.elapsed(),
            std::hint::black_box(checksum),
            comparisons,
        )
    };

    let (_, warm_current_checksum, warm_lookups) = current();
    let (_, warm_control_checksum, warm_comparisons) = control();
    assert_eq!(warm_current_checksum, warm_control_checksum);
    assert_eq!(warm_lookups, SELECTED_COUNT);
    assert!(warm_comparisons >= 16_777_216);

    let mut current_samples = Vec::with_capacity(SAMPLES);
    let mut control_samples = Vec::with_capacity(SAMPLES);
    for sample in 0..SAMPLES {
        let (current_result, control_result) = if sample % 2 == 0 {
            (current(), control())
        } else {
            let control_result = control();
            let current_result = current();
            (current_result, control_result)
        };
        assert_eq!(current_result.1, control_result.1);
        assert_eq!(current_result.2, SELECTED_COUNT);
        assert!(control_result.2 >= 16_777_216);
        current_samples.push(current_result.0);
        control_samples.push(control_result.0);
    }
    current_samples.sort_unstable();
    control_samples.sort_unstable();
    let current_median = current_samples[SAMPLES / 2];
    let control_median = control_samples[SAMPLES / 2];
    let current_maximum = *current_samples.last().expect("current sample");
    assert!(
        current_median.as_nanos().saturating_mul(4) <= control_median.as_nanos(),
        "retained index median {current_median:?} exceeds 25% of control {control_median:?}"
    );
    assert!(
        current_maximum <= Duration::from_millis(500),
        "retained index maximum {current_maximum:?} exceeds 500 ms"
    );
    eprintln!(
        "phase5a_row_group_index current_median_ns={} current_maximum_ns={} control_median_ns={} selected={} descriptors={}",
        current_median.as_nanos(),
        current_maximum.as_nanos(),
        control_median.as_nanos(),
        SELECTED_COUNT,
        ROW_GROUP_COUNT,
    );
}

#[test]
fn column_bundle_parallel_prepare_matches_worker_counts_and_plan_order() {
    let path = fixture_path("column_bundle_parallel_prepare");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    let mut expected = None;

    for workers in [1, 2, 4, 8] {
        let plan = bundle
            .plan_read(&ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(workers),
            })
            .expect("plan parallel preparation");
        let mut committed = Vec::new();
        let report = bundle
            .parallel_prepare_plan_row_groups(
                &plan,
                &[1, 0],
                ColumnBundleParallelPrepareOptions {
                    max_in_flight_row_groups: 2,
                },
                |context, batch| {
                    assert_eq!(context.row_group_id, batch.row_group_id);
                    assert_eq!(context.base_row, batch.base_row);
                    assert_eq!(context.row_count, batch.row_count);
                    assert_eq!(context.row_end, batch.base_row + batch.row_count);
                    let order_keys = match &batch.columns[1].values {
                        PrimitiveColumnValues::I64(values) => values.clone(),
                        _ => panic!("order key must be i64"),
                    };
                    Ok((
                        context.selected_row_group_ordinal,
                        context.row_group_id,
                        context.base_row,
                        context.row_end,
                        context.row_count,
                        order_keys,
                    ))
                },
                |context, prepared| {
                    assert_eq!(context.selected_row_group_ordinal, prepared.0);
                    committed.push(prepared);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("parallel prepare row groups");
        assert_eq!(
            committed
                .iter()
                .map(|prepared| prepared.1)
                .collect::<Vec<_>>(),
            vec![0, 1]
        );
        if let Some(expected) = &expected {
            assert_eq!(&committed, expected);
        } else {
            expected = Some(committed);
        }
        assert_eq!(report.requested_workers, workers);
        assert_eq!(report.started_workers, workers.min(2));
        assert_eq!(report.row_groups_queued, 2);
        assert_eq!(report.row_groups_completed, 2);
        assert_eq!(report.row_groups_ordered_committed, 2);
        assert_eq!(report.rows_ordered_committed, 6);
        assert!(report.ordered_terminal_completed);
        assert!(!report.cursor_report.cancelled);
        assert_eq!(report.attribution.selected_row_groups, 2);
        assert_eq!(report.attribution.selected_column_chunks, 4);
        assert!(report.attribution.read_io_ns > 0);
        assert!(report.attribution.primitive_decode_ns > 0);
        assert_eq!(
            report.caller_prepare_ns,
            report
                .worker_reports
                .iter()
                .map(|worker| worker.caller_prepare_ns)
                .sum::<u64>()
        );
    }

    let plan = bundle
        .plan_read(&ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(4),
        })
        .expect("plan stopped preparation");
    let stopped = bundle
        .parallel_prepare_plan_row_groups(
            &plan,
            &plan.row_group_ids,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 2,
            },
            |context, _| Ok(context.row_group_id),
            |context, row_group_id| {
                assert_eq!(context.row_group_id, row_group_id);
                Ok(ColumnBundleVisitControl::Stop)
            },
        )
        .expect("stop ordered commit");
    assert!(stopped.cursor_report.cancelled);
    assert_eq!(stopped.row_groups_ordered_committed, 1);
    assert!(!stopped.ordered_terminal_completed);
    assert!(stopped.row_groups_ordered_committed <= stopped.row_groups_completed);
    assert!(stopped.row_groups_completed <= stopped.row_groups_queued);

    cleanup(&path);
}

#[test]
fn column_bundle_read_attribution_reports_diagnostic_counters() {
    let path = fixture_path("column_bundle_read_attribution");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let attributed = bundle
        .read_batches_with_attribution(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key", "order_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("attributed read");

    assert_eq!(attributed.outcome.batches.len(), 2);
    assert_eq!(attributed.attribution.selected_row_groups, 2);
    assert_eq!(attributed.attribution.selected_column_chunks, 4);
    assert_eq!(attributed.attribution.requested_threads, 2);
    assert_eq!(attributed.attribution.effective_threads, 2);
    assert!(attributed.attribution.execute_wall_ns > 0);
    assert_eq!(attributed.attribution.callback_wall_ns, 0);
    assert!(attributed.attribution.row_group_read_ns > 0);
    assert!(attributed.attribution.read_io_ns > 0);
    assert!(attributed.attribution.bytes_read > 0);
    assert!(attributed.attribution.compressed_bytes > 0);
    assert!(attributed.attribution.uncompressed_bytes > 0);
    assert_eq!(attributed.attribution.native_to_c_copy_ns, None);
    assert_eq!(attributed.attribution.wrapper_copy_ns, None);

    let mut visited = Vec::new();
    let attributed_cursor = bundle
        .visit_batches_with_attribution(
            ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key", "order_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(2),
            },
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("attributed visitor read");
    assert_eq!(visited, vec![0, 1]);
    assert_eq!(attributed_cursor.cursor_report.batches_yielded, 2);
    assert_eq!(attributed_cursor.cursor_report.rows_yielded, 6);
    assert_eq!(
        attributed_cursor
            .cursor_report
            .max_in_flight_row_groups_observed,
        2
    );
    assert_eq!(attributed_cursor.attribution.selected_row_groups, 2);
    assert_eq!(attributed_cursor.attribution.selected_column_chunks, 4);
    assert_eq!(attributed_cursor.attribution.requested_threads, 2);
    assert_eq!(attributed_cursor.attribution.effective_threads, 2);
    assert!(attributed_cursor.attribution.execute_wall_ns > 0);
    assert!(attributed_cursor.attribution.row_group_read_ns > 0);
    assert!(attributed_cursor.attribution.read_io_ns > 0);
    assert!(attributed_cursor.attribution.bytes_read > 0);

    cleanup(&path);
}

#[test]
fn column_bundle_visit_batches_bounds_and_can_cancel() {
    let path = fixture_path("column_bundle_visit_batches");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let mut visited = Vec::new();
    let report = bundle
        .visit_batches(
            ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(4),
            },
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(ColumnBundleVisitControl::Continue)
            },
        )
        .expect("visit all batches");
    assert_eq!(visited, vec![0, 1]);
    assert_eq!(report.batches_yielded, 2);
    assert_eq!(report.rows_yielded, 6);
    assert_eq!(report.max_in_flight_row_groups_observed, 1);
    assert!(!report.cancelled);
    assert_eq!(report.base_report.selected_row_groups, 2);

    let mut visited = Vec::new();
    let report = bundle
        .visit_batches(
            ColumnBundleReadRequest {
                projection: ColumnProjection::names(["partition_key"]),
                predicates: Vec::new(),
                options: ColumnBundleReadOptions::parallel(4),
            },
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(ColumnBundleVisitControl::Stop)
            },
        )
        .expect("cancel visit");
    assert_eq!(visited, vec![0]);
    assert_eq!(report.batches_yielded, 1);
    assert_eq!(report.rows_yielded, 3);
    assert_eq!(report.max_in_flight_row_groups_observed, 2);
    assert!(report.cancelled);

    let err = bundle
        .visit_batches(
            ColumnBundleReadRequest::default(),
            ColumnBundleReadCursorOptions {
                max_in_flight_row_groups: 0,
                ordered: true,
            },
            |_| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect_err("zero in-flight rejects");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );

    cleanup(&path);
}

#[test]
fn column_bundle_read_row_group_into_fills_caller_buffers() {
    let path = fixture_path("column_bundle_read_row_group_into");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let mut partition = [0i32; 3];
    let mut order = [0i64; 3];
    let report = bundle
        .read_row_group_into(
            1,
            &mut [
                ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I32(&mut partition),
                    validity_bytes: None,
                    allow_nulls: false,
                },
                ColumnBundleColumnFillBuffer {
                    column_name: Some("order_key"),
                    column_id: Some(1),
                    values: PrimitiveColumnValuesMut::I64(&mut order),
                    validity_bytes: None,
                    allow_nulls: false,
                },
            ],
            ColumnBundleReadFillOptions::default(),
        )
        .expect("fill row group");

    assert_eq!(partition, [11, 11, 11]);
    assert_eq!(order, [200, 201, 202]);
    assert_eq!(report.row_group_id, 1);
    assert_eq!(report.base_row, 3);
    assert_eq!(report.row_count, 3);
    assert_eq!(report.columns.len(), 2);
    assert_eq!(report.columns[0].column_id, 0);
    assert_eq!(report.columns[0].rows_filled, 3);
    assert!(!report.columns[0].validity_filled);

    cleanup(&path);
}

#[test]
fn uncompressed_fixed_binary_fill_reads_directly_and_preserves_crc_error_order() {
    let path = fixture_path("column_bundle_direct_fixed_binary_fill");
    write_summary_fixture_with_codec(&path, OcbChunkCodecV1::None);
    let bundle = ColumnBundleFile::open(&path).expect("open uncompressed fixture");
    let chunk = *bundle
        .metadata
        .row_group_index
        .column_chunks
        .iter()
        .find(|chunk| chunk.row_group_id == 0 && chunk.column_id == 1)
        .expect("fixed-binary chunk");

    let direct_before = uncompressed_fixed_binary_direct_fill_count_for_test();
    let mut payload = [0u8; 6];
    bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("payload"),
                column_id: Some(1),
                values: PrimitiveColumnValuesMut::FixedBinary {
                    width: 2,
                    bytes: &mut payload,
                },
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect("direct fixed-binary fill");
    assert_eq!(&payload, b"aabbcc");
    assert_eq!(
        uncompressed_fixed_binary_direct_fill_count_for_test(),
        direct_before + 1
    );

    let attributed_before = uncompressed_fixed_binary_direct_fill_count_for_test();
    let mut attributed_payload = [0u8; 6];
    let (_, attribution) = read_row_group_into_with_attribution(
        bundle.read_source(),
        &bundle.metadata,
        &bundle.columns,
        0,
        &mut [ColumnBundleColumnFillBuffer {
            column_name: Some("payload"),
            column_id: Some(1),
            values: PrimitiveColumnValuesMut::FixedBinary {
                width: 2,
                bytes: &mut attributed_payload,
            },
            validity_bytes: None,
            allow_nulls: false,
        }],
    )
    .expect("attributed direct fixed-binary fill");
    assert_eq!(&attributed_payload, b"aabbcc");
    assert_eq!(
        uncompressed_fixed_binary_direct_fill_count_for_test(),
        attributed_before + 1
    );
    assert_eq!(attribution.bytes_read, chunk.value_ref.length);
    assert_eq!(attribution.compressed_bytes, 6);
    assert_eq!(attribution.uncompressed_bytes, 6);
    assert_eq!(attribution.copy_materialization, Duration::ZERO);

    let mut file = fs::OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("open direct-fill fixture for header corruption");
    file.seek(SeekFrom::Start(chunk.value_ref.offset + 24))
        .expect("seek chunk column id");
    file.write_all(&99u32.to_le_bytes())
        .expect("corrupt chunk column id");
    drop(file);
    let mut rejected = [0u8; 6];
    let err = bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("payload"),
                column_id: Some(1),
                values: PrimitiveColumnValuesMut::FixedBinary {
                    width: 2,
                    bytes: &mut rejected,
                },
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("structural header mismatch must reject before CRC checks");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
    assert!(err.to_string().contains("object does not match descriptor"));
    cleanup(&path);

    let crc_path = fixture_path("column_bundle_direct_fixed_binary_crc");
    write_summary_fixture_with_codec(&crc_path, OcbChunkCodecV1::None);
    let mut crc_bundle = ColumnBundleFile::open(&crc_path).expect("open direct-fill CRC fixture");
    let chunk_index = crc_bundle
        .metadata
        .row_group_index
        .column_chunks
        .iter()
        .position(|chunk| chunk.row_group_id == 0 && chunk.column_id == 1)
        .expect("fixed-binary CRC chunk");
    let crc_chunk = crc_bundle.metadata.row_group_index.column_chunks[chunk_index];
    let mut file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&crc_path)
        .expect("open direct-fill fixture for payload corruption");
    let payload_offset = crc_chunk.value_ref.offset + u64::from(OCB_COLUMN_CHUNK_V1_HEADER_LEN);
    file.seek(SeekFrom::Start(payload_offset))
        .expect("seek direct-fill payload");
    let mut first_byte = [0u8; 1];
    std::io::Read::read_exact(&mut file, &mut first_byte).expect("read direct-fill payload byte");
    file.seek(SeekFrom::Start(payload_offset))
        .expect("seek direct-fill payload again");
    file.write_all(&[first_byte[0] ^ 0x5a])
        .expect("corrupt direct-fill payload");
    drop(file);

    let mut rejected = [0u8; 6];
    let err = crc_bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("payload"),
                column_id: Some(1),
                values: PrimitiveColumnValuesMut::FixedBinary {
                    width: 2,
                    bytes: &mut rejected,
                },
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("body-reference checksum must reject corrupted direct payload");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
    assert!(err.to_string().contains("body reference checksum mismatch"));

    let file_bytes = fs::read(&crc_path).expect("read corrupted chunk object");
    let object_start = crc_chunk.value_ref.offset as usize;
    let object_end = object_start + crc_chunk.value_ref.length as usize;
    Arc::make_mut(&mut crc_bundle.metadata)
        .row_group_index
        .column_chunks[chunk_index]
        .value_ref
        .checksum = crc32c(&file_bytes[object_start..object_end]);
    let err = crc_bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("payload"),
                column_id: Some(1),
                values: PrimitiveColumnValuesMut::FixedBinary {
                    width: 2,
                    bytes: &mut rejected,
                },
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("embedded chunk CRC must reject after body checksum passes");
    assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::CorruptFile));
    assert!(err.to_string().contains("column chunk crc mismatch"));
    cleanup(&crc_path);
}

#[test]
fn column_bundle_read_row_group_into_rejects_bad_buffers() {
    let path = fixture_path("column_bundle_read_row_group_into_bad_buffers");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let mut short = [0i32; 2];
    let err = bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: None,
                values: PrimitiveColumnValuesMut::I32(&mut short),
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("short buffer rejects");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("capacity"));

    let mut wrong_dtype = [0i64; 3];
    let err = bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: None,
                values: PrimitiveColumnValuesMut::I64(&mut wrong_dtype),
                validity_bytes: None,
                allow_nulls: false,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("wrong dtype rejects");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("dtype"));

    let mut first = [0i32; 3];
    let mut second = [0i32; 3];
    let err = bundle
        .read_row_group_into(
            0,
            &mut [
                ColumnBundleColumnFillBuffer {
                    column_name: Some("partition_key"),
                    column_id: None,
                    values: PrimitiveColumnValuesMut::I32(&mut first),
                    validity_bytes: None,
                    allow_nulls: false,
                },
                ColumnBundleColumnFillBuffer {
                    column_name: None,
                    column_id: Some(0),
                    values: PrimitiveColumnValuesMut::I32(&mut second),
                    validity_bytes: None,
                    allow_nulls: false,
                },
            ],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("duplicate buffer rejects");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("duplicate"));

    cleanup(&path);
}

#[test]
fn column_bundle_read_row_group_into_fills_validity_bitmap() {
    let path = fixture_path("column_bundle_read_row_group_into_validity");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            ..FixtureOptions::default()
        },
    );
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let mut values = [0i32; 3];
    let mut validity = [0u8; 1];
    let report = bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: None,
                values: PrimitiveColumnValuesMut::I32(&mut values),
                validity_bytes: Some(&mut validity),
                allow_nulls: true,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect("fill nullable row group");
    assert_eq!(values, [10, 10, 10]);
    assert_eq!(validity, [0b0000_0101]);
    assert!(report.columns[0].validity_filled);

    let mut values_without_validity = [0i32; 3];
    let err = bundle
        .read_row_group_into(
            0,
            &mut [ColumnBundleColumnFillBuffer {
                column_name: Some("partition_key"),
                column_id: None,
                values: PrimitiveColumnValuesMut::I32(&mut values_without_validity),
                validity_bytes: None,
                allow_nulls: true,
            }],
            ColumnBundleReadFillOptions::default(),
        )
        .expect_err("missing validity rejects");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("validity"));

    cleanup(&path);
}

#[test]
fn column_bundle_missing_stats_keep_row_groups_conservatively() {
    let path = fixture_path("column_bundle_missing_stats_keep");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let outcome = bundle
        .read_batches_with_report(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["category_code"]),
            predicates: vec![RowGroupPredicate::equal(
                "category_code",
                ColumnPredicateValue::I32(2),
            )],
            options: ColumnBundleReadOptions::parallel(2),
        })
        .expect("read conservatively kept batches");

    assert_eq!(outcome.batches.len(), 2);
    assert_eq!(outcome.report.selected_row_groups, 2);
    assert_eq!(outcome.report.pruned_row_groups, 0);
    assert_eq!(outcome.report.effective_threads, 2);
    assert_eq!(outcome.report.fallback_reason, None);
    cleanup(&path);
}

#[test]
fn column_bundle_rejects_predicate_dtype_mismatch() {
    let path = fixture_path("column_bundle_predicate_dtype_mismatch");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let err = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: vec![RowGroupPredicate::equal(
                "partition_key",
                ColumnPredicateValue::I64(11),
            )],
            options: ColumnBundleReadOptions::serial(),
        })
        .expect_err("dtype mismatch");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("dtype"));
    cleanup(&path);
}

#[test]
fn column_bundle_rejects_invalid_predicate_requests_as_input() {
    let path = fixture_path("column_bundle_invalid_predicate_requests");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");

    let err = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: vec![RowGroupPredicate::equal(
                "missing",
                ColumnPredicateValue::I32(11),
            )],
            options: ColumnBundleReadOptions::serial(),
        })
        .expect_err("unknown predicate column");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("unknown column"));

    let err = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: vec![RowGroupPredicate::new("partition_key", None, None)],
            options: ColumnBundleReadOptions::serial(),
        })
        .expect_err("empty predicate");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("at least one bound"));
    cleanup(&path);
}

#[test]
fn column_bundle_rejects_unknown_projection() {
    let path = fixture_path("column_bundle_unknown_projection");
    write_fixture(&path);
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    let err = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["missing"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect_err("unknown projection");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::InvalidInput)
    );
    assert!(err.to_string().contains("unknown column"));
    cleanup(&path);
}

#[test]
fn column_bundle_reads_nullable_column_without_bitmap_as_all_valid() {
    let path = fixture_path("column_bundle_nullable_all_valid");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            ..FixtureOptions::default()
        },
    );
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    let batches = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("read nullable all-valid column");
    assert!(batches[0].columns[0].validity.is_none());
    cleanup(&path);
}

#[test]
fn column_bundle_reads_nullable_validity_bitmap() {
    let path = fixture_path("column_bundle_validity_bitmap");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            nullable_column_id: Some(0),
            validity_ref_column_id: Some(0),
            ..FixtureOptions::default()
        },
    );
    let bundle = ColumnBundleFile::open(&path).expect("open OCB fixture");
    let batches = bundle
        .read_batches(ColumnBundleReadRequest {
            projection: ColumnProjection::names(["partition_key"]),
            predicates: Vec::new(),
            options: ColumnBundleReadOptions::serial(),
        })
        .expect("read validity bitmap");
    let validity = batches[0].columns[0]
        .validity
        .as_ref()
        .expect("validity bitmap present");
    assert_eq!(validity.row_count, 3);
    assert_eq!(validity.bytes, vec![0b0000_0101]);
    assert!(validity.is_valid(0).expect("row 0 validity"));
    assert!(!validity.is_valid(1).expect("row 1 validity"));
    assert!(validity.is_valid(2).expect("row 2 validity"));
    cleanup(&path);
}

#[test]
fn column_bundle_rejects_non_null_validity_refs() {
    let path = fixture_path("column_bundle_validity_ref_rejected");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            validity_ref_column_id: Some(0),
            ..FixtureOptions::default()
        },
    );
    let err = ColumnBundleFile::open(&path)
        .expect_err("metadata graph must reject non-null validity on non-null column");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::CorruptFile)
    );
    assert!(err.to_string().contains("non-null column"));
    cleanup(&path);
}

#[test]
fn column_bundle_rejects_row_group_pointing_at_other_group_chunks() {
    let path = fixture_path("column_bundle_wrong_row_group_chunks");
    write_fixture_with_options(
        &path,
        FixtureOptions {
            row_group0_chunk_desc_begin: Some(3),
            ..FixtureOptions::default()
        },
    );
    let err = ColumnBundleFile::open(&path)
        .expect_err("metadata graph must reject wrong row-group chunk range");
    assert_eq!(
        err.ocb_failure_cause(),
        Some(crate::OcbFailureCause::CorruptFile)
    );
    assert!(err.to_string().contains("row-group chunk descriptor"));
    cleanup(&path);
}

fn fixture_path(name: &str) -> PathBuf {
    let root = PathBuf::from(".tmp/ocb-tests");
    fs::create_dir_all(&root).expect("create test tmp dir");
    root.join(format!("{name}-{}.tio", std::process::id()))
}

fn cleanup(path: &Path) {
    let _ = fs::remove_file(path);
}

#[derive(Debug, Clone, Copy, Default)]
struct FixtureOptions {
    nullable_column_id: Option<u32>,
    validity_ref_column_id: Option<u32>,
    row_group0_chunk_desc_begin: Option<u64>,
    rows_per_group: Option<usize>,
    row_group_ids: Option<[u32; 2]>,
}

#[derive(Debug, Clone, Copy)]
enum V2LatestFixtureCase {
    Corrupt,
    DebugObject(usize),
    UnsupportedVersion,
}

fn write_fixture(path: &Path) {
    write_fixture_with_options(path, FixtureOptions::default());
}

fn rewrite_fixture_as_v2_with_dual_roots(path: &Path) {
    let mut file_bytes = fs::read(path).expect("read v1 fixture bytes");
    let bootstrap = OcbBootstrapPageV1::read_from(Cursor::new(file_bytes.as_slice()))
        .expect("read v1 bootstrap");
    let root_start = bootstrap.root_ref.offset as usize;
    let root_end = root_start + bootstrap.root_ref.length as usize;
    let base_root =
        OcbRootV1::read_from(Cursor::new(&file_bytes[root_start..root_end])).expect("read v1 root");

    let mut stale_root = root_v2_from_v1(&base_root, 1, 0, OcbBodyRefV2::NULL);
    stale_root.row_count = 999;
    let stale_root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
        stale_root.write_to(buf)
    });
    assert_eq!(stale_root_ref.length, OCB_ROOT_V2_LEN as u64);

    let latest_root = root_v2_from_v1(&base_root, 2, 1, stale_root_ref);
    let latest_root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
        latest_root.write_to(buf)
    });
    assert_eq!(latest_root_ref.length, OCB_ROOT_V2_LEN as u64);

    let bootstrap = OcbBootstrapPageV2::new(
        [77u8; 16],
        [
            OcbRootSlotV2::new(
                0,
                1,
                stale_root_ref,
                0,
                OcbBodyRefV2::NULL,
                OcbBodyRefV2::NULL,
            ),
            OcbRootSlotV2::new(1, 2, latest_root_ref, 1, stale_root_ref, OcbBodyRefV2::NULL),
        ],
    )
    .expect("build v2 bootstrap");
    let mut bootstrap_bytes = Vec::new();
    bootstrap
        .write_to(&mut bootstrap_bytes)
        .expect("write v2 bootstrap");
    file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
    fs::write(path, file_bytes).expect("write v2 fixture bytes");
}

fn rewrite_fixture_as_v2_with_latest_case(path: &Path, case: V2LatestFixtureCase) {
    let mut file_bytes = fs::read(path).expect("read v1 fixture bytes");
    let bootstrap = OcbBootstrapPageV1::read_from(Cursor::new(file_bytes.as_slice()))
        .expect("read v1 bootstrap");
    let root_start = bootstrap.root_ref.offset as usize;
    let root_end = root_start + bootstrap.root_ref.length as usize;
    let base_root =
        OcbRootV1::read_from(Cursor::new(&file_bytes[root_start..root_end])).expect("read v1 root");

    let older_root = root_v2_from_v1(&base_root, 1, 0, OcbBodyRefV2::NULL);
    let older_root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
        older_root.write_to(buf)
    });
    let mut latest_root = root_v2_from_v1(&base_root, 2, 1, older_root_ref);
    match case {
        V2LatestFixtureCase::Corrupt => latest_root.row_count = 999,
        V2LatestFixtureCase::DebugObject(len) => {
            latest_root.debug_json_ref = append_raw_object(
                &mut file_bytes,
                OcbBodyKindV1::DebugJsonMetadata,
                vec![b'x'; len],
            );
        }
        V2LatestFixtureCase::UnsupportedVersion => {}
    }
    let latest_root_ref = if matches!(case, V2LatestFixtureCase::UnsupportedVersion) {
        let mut object = Vec::new();
        latest_root
            .write_to(&mut object)
            .expect("encode latest root");
        object[8..10].copy_from_slice(&99u16.to_le_bytes());
        let checksum_offset = object.len() - 4;
        object[checksum_offset..].fill(0);
        let checksum = crc32c(&object);
        object[checksum_offset..].copy_from_slice(&checksum.to_le_bytes());
        append_raw_object(&mut file_bytes, OcbBodyKindV1::Root, object)
    } else {
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            latest_root.write_to(buf)
        })
    };

    let bootstrap = OcbBootstrapPageV2::new(
        [78u8; 16],
        [
            OcbRootSlotV2::new(
                0,
                1,
                older_root_ref,
                0,
                OcbBodyRefV2::NULL,
                OcbBodyRefV2::NULL,
            ),
            OcbRootSlotV2::new(1, 2, latest_root_ref, 1, older_root_ref, OcbBodyRefV2::NULL),
        ],
    )
    .expect("build v2 bootstrap");
    let mut bootstrap_bytes = Vec::new();
    bootstrap
        .write_to(&mut bootstrap_bytes)
        .expect("write v2 bootstrap");
    file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
    fs::write(path, file_bytes).expect("write v2 fallback fixture bytes");
}

fn root_v2_from_v1(
    root: &OcbRootV1,
    generation: u64,
    previous_generation: u64,
    previous_root_ref: OcbBodyRefV2,
) -> OcbRootV2 {
    OcbRootV2 {
        version: OCB_FORMAT_MAJOR_V2,
        flags: root.flags,
        generation,
        previous_generation,
        previous_root_ref,
        append_base_row: 0,
        append_row_count: root.row_count,
        append_base_row_group: 0,
        append_row_group_count: root.row_group_count,
        row_count: root.row_count,
        column_count: root.column_count,
        row_group_count: root.row_group_count,
        dictionary_count: root.dictionary_count,
        column_chunk_count: root.column_count * root.row_group_count,
        schema_ref: root.schema_ref,
        dictionary_index_ref: root.dictionary_index_ref,
        row_group_index_ref: root.row_group_index_ref,
        ordering_proof_ref: root.ordering_proof_ref,
        debug_json_ref: root.debug_json_ref,
        first_key_tuple_ref: OcbBodyRefV2::NULL,
        last_key_tuple_ref: OcbBodyRefV2::NULL,
        append_first_key_tuple_ref: OcbBodyRefV2::NULL,
        append_last_key_tuple_ref: OcbBodyRefV2::NULL,
        commit_diagnostics_ref: OcbBodyRefV2::NULL,
        created_unix_nanos: root.created_unix_nanos,
        content_flags: root.content_flags,
        crc32c: 0,
    }
}

fn write_summary_fixture(path: &Path) {
    write_summary_fixture_with_codec(path, OcbChunkCodecV1::Zstd);
}

fn write_summary_fixture_with_codec(path: &Path, fixed_binary_codec: OcbChunkCodecV1) {
    let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];

    let rg0_partition = append_chunk(&mut file_bytes, 0, 0, OcbPhysicalTypeV1::I32, &[10, 10, 10]);
    let rg0_payload = append_fixed_binary_chunk(
        &mut file_bytes,
        0,
        1,
        2,
        &[b"aa".as_slice(), b"bb".as_slice(), b"cc".as_slice()],
        fixed_binary_codec,
    );
    let rg1_partition = append_chunk(&mut file_bytes, 1, 0, OcbPhysicalTypeV1::I32, &[11, 11, 11]);
    let rg1_payload = append_fixed_binary_chunk(
        &mut file_bytes,
        1,
        1,
        2,
        &[b"dd".as_slice(), b"ee".as_slice(), b"ff".as_slice()],
        fixed_binary_codec,
    );

    let string_table = OcbStringTableV1 {
        version: 1,
        strings: vec!["partition_key".into(), "payload".into()],
        crc32c: 0,
    };
    let string_table_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
            string_table.write_to(buf)
        });

    let schema = OcbSchemaV1 {
        version: 1,
        string_table_ref,
        columns: vec![
            column_desc(
                0,
                0,
                OcbPhysicalTypeV1::I32,
                OcbLogicalKindV1::OpaqueKey,
                OCB_NULL_U32,
            ),
            fixed_binary_column_desc(1, 1, 2),
        ],
        crc32c: 0,
    };
    let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
        schema.write_to(buf)
    });

    let row_group_index = OcbRowGroupIndexV1 {
        version: 1,
        flags: 0,
        row_groups: vec![
            OcbRowGroupDescV1 {
                row_group_id: 0,
                flags: 0,
                base_row: 0,
                row_count: 3,
                chunk_desc_begin: 0,
                chunk_desc_count: 2,
                stat_begin: 0,
                stat_count: 1,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            },
            OcbRowGroupDescV1 {
                row_group_id: 1,
                flags: 0,
                base_row: 3,
                row_count: 3,
                chunk_desc_begin: 2,
                chunk_desc_count: 2,
                stat_begin: 1,
                stat_count: 1,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            },
        ],
        column_chunks: vec![
            chunk_desc(0, 0, OcbPhysicalTypeV1::I32, rg0_partition, 3),
            chunk_desc_with_codec_bytes(
                0,
                1,
                OcbPhysicalTypeV1::FixedBinary,
                fixed_binary_codec,
                rg0_payload,
                3,
                6,
            ),
            chunk_desc(1, 0, OcbPhysicalTypeV1::I32, rg1_partition, 3),
            chunk_desc_with_codec_bytes(
                1,
                1,
                OcbPhysicalTypeV1::FixedBinary,
                fixed_binary_codec,
                rg1_payload,
                3,
                6,
            ),
        ],
        stats: vec![stats_i32(0, 0, 10, 10), stats_i32(1, 0, 11, 11)],
        crc32c: 0,
    };
    let row_group_index_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
            row_group_index.write_to(buf)
        });

    let ordering = OcbOrderingProofV1 {
        version: 1,
        flags: 0b1,
        keys: vec![ordering_key(0)],
        row_group_proofs: vec![
            OcbRowGroupOrderingProofV1 {
                row_group_id: 0,
                flags: 1,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            },
            OcbRowGroupOrderingProofV1 {
                row_group_id: 1,
                flags: 1,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            },
        ],
        crc32c: 0,
    };
    let ordering_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::OrderingProof, |buf| {
            ordering.write_to(buf)
        });

    let root = OcbRootV1 {
        version: 1,
        flags: 0,
        row_count: 6,
        column_count: 2,
        row_group_count: 2,
        dictionary_count: 0,
        schema_ref,
        dictionary_index_ref: OcbBodyRefV2::NULL,
        row_group_index_ref,
        ordering_proof_ref: ordering_ref,
        debug_json_ref: OcbBodyRefV2::NULL,
        created_unix_nanos: 0,
        content_flags: 0,
        crc32c: 0,
    };
    let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
        root.write_to(buf)
    });
    assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);

    let bootstrap = OcbBootstrapPageV1::new([43u8; 16], root_ref);
    let mut bootstrap_bytes = Vec::new();
    bootstrap
        .write_to(&mut bootstrap_bytes)
        .expect("write bootstrap");
    file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);

    let mut file = fs::File::create(path).expect("create summary fixture");
    file.write_all(&file_bytes).expect("write summary fixture");
}

fn write_fixture_with_options(path: &Path, options: FixtureOptions) {
    let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
    let [row_group0_id, row_group1_id] = options.row_group_ids.unwrap_or([0, 1]);
    let rows_per_group = options.rows_per_group.unwrap_or(3);
    assert!(rows_per_group > 0, "fixture row groups must be non-empty");
    let row_count = u64::try_from(rows_per_group).expect("fixture row count fits u64");
    let total_row_count = row_count
        .checked_mul(2)
        .expect("fixture total row count does not overflow");
    let rg0_partition_values = vec![10; rows_per_group];
    let rg0_order_values = (0..rows_per_group)
        .map(|index| 100 + i64::try_from(index).expect("fixture index fits i64"))
        .collect::<Vec<_>>();
    let rg0_category_values = (0..rows_per_group)
        .map(|index| [1, 1, 2][index % 3])
        .collect::<Vec<_>>();
    let rg1_partition_values = vec![11; rows_per_group];
    let rg1_order_values = (0..rows_per_group)
        .map(|index| 200 + i64::try_from(index).expect("fixture index fits i64"))
        .collect::<Vec<_>>();
    let rg1_category_values = (0..rows_per_group)
        .map(|index| [1, 2, 2][index % 3])
        .collect::<Vec<_>>();

    let rg0_partition = append_chunk(
        &mut file_bytes,
        row_group0_id,
        0,
        OcbPhysicalTypeV1::I32,
        &rg0_partition_values,
    );
    let rg0_order = append_chunk_i64(&mut file_bytes, row_group0_id, 1, &rg0_order_values);
    let rg0_category = append_chunk(
        &mut file_bytes,
        row_group0_id,
        2,
        OcbPhysicalTypeV1::I32,
        &rg0_category_values,
    );
    let rg1_partition = append_chunk(
        &mut file_bytes,
        row_group1_id,
        0,
        OcbPhysicalTypeV1::I32,
        &rg1_partition_values,
    );
    let rg1_order = append_chunk_i64(&mut file_bytes, row_group1_id, 1, &rg1_order_values);
    let rg1_category = append_chunk(
        &mut file_bytes,
        row_group1_id,
        2,
        OcbPhysicalTypeV1::I32,
        &rg1_category_values,
    );

    let dictionary_values = OcbDictionaryValuesV1 {
        version: 1,
        value_kind: OcbDictionaryValueKindV1::Utf8,
        fixed_width: 0,
        values: vec![b"alpha".to_vec(), b"beta".to_vec()],
        crc32c: 0,
    };
    let dictionary_values_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::DictionaryValues, |buf| {
            dictionary_values.write_to(buf)
        });

    let string_table = OcbStringTableV1 {
        version: 1,
        strings: vec![
            "partition_key".into(),
            "order_key".into(),
            "category_code".into(),
            "category_dictionary".into(),
        ],
        crc32c: 0,
    };
    let string_table_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
            string_table.write_to(buf)
        });

    let mut columns = vec![
        column_desc(
            0,
            0,
            OcbPhysicalTypeV1::I32,
            OcbLogicalKindV1::OpaqueKey,
            OCB_NULL_U32,
        ),
        column_desc(
            1,
            1,
            OcbPhysicalTypeV1::I64,
            OcbLogicalKindV1::OpaqueKey,
            OCB_NULL_U32,
        ),
        column_desc(
            2,
            2,
            OcbPhysicalTypeV1::I32,
            OcbLogicalKindV1::DictionaryCode,
            0,
        ),
    ];
    if let Some(nullable_column_id) = options.nullable_column_id {
        columns
            .iter_mut()
            .find(|column| column.column_id == nullable_column_id)
            .expect("fixture nullable column exists")
            .nullability = OcbNullabilityV1::Nullable;
    }
    let schema = OcbSchemaV1 {
        version: 1,
        string_table_ref,
        columns,
        crc32c: 0,
    };
    let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
        schema.write_to(buf)
    });

    let dictionary_index = OcbDictionaryIndexV1 {
        version: 1,
        dictionaries: vec![OcbDictionaryDescV1 {
            dictionary_id: 0,
            name_string_id: 3,
            code_physical_type: OcbPhysicalTypeV1::I32,
            value_kind: OcbDictionaryValueKindV1::Utf8,
            flags: 0,
            values_ref: dictionary_values_ref,
            entry_count: 2,
            reserved0: 0,
        }],
        crc32c: 0,
    };
    let dictionary_index_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::DictionaryIndex, |buf| {
            dictionary_index.write_to(buf)
        });

    let forced_validity_ref = if options.validity_ref_column_id.is_some() {
        let mut validity = vec![0u8; rows_per_group.div_ceil(8)];
        for row in (0..rows_per_group).step_by(2) {
            validity[row / 8] |= 1 << (row % 8);
        }
        append_raw_object(&mut file_bytes, OcbBodyKindV1::ValidityBitmap, validity)
    } else {
        OcbBodyRefV2::NULL
    };
    let validity_ref_for = |row_group_id: u32, column_id: u32| {
        if row_group_id == row_group0_id && options.validity_ref_column_id == Some(column_id) {
            forced_validity_ref
        } else {
            OcbBodyRefV2::NULL
        }
    };

    let row_group_index = OcbRowGroupIndexV1 {
        version: 1,
        flags: 0,
        row_groups: vec![
            OcbRowGroupDescV1 {
                row_group_id: row_group0_id,
                flags: 0,
                base_row: 0,
                row_count,
                chunk_desc_begin: options.row_group0_chunk_desc_begin.unwrap_or(0),
                chunk_desc_count: 3,
                stat_begin: 0,
                stat_count: 2,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            },
            OcbRowGroupDescV1 {
                row_group_id: row_group1_id,
                flags: 0,
                base_row: row_count,
                row_count,
                chunk_desc_begin: 3,
                chunk_desc_count: 3,
                stat_begin: 2,
                stat_count: 2,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            },
        ],
        column_chunks: vec![
            chunk_desc_with_validity(
                row_group0_id,
                0,
                OcbPhysicalTypeV1::I32,
                rg0_partition,
                validity_ref_for(row_group0_id, 0),
                row_count,
            ),
            chunk_desc_with_validity(
                row_group0_id,
                1,
                OcbPhysicalTypeV1::I64,
                rg0_order,
                validity_ref_for(row_group0_id, 1),
                row_count,
            ),
            chunk_desc_with_validity(
                row_group0_id,
                2,
                OcbPhysicalTypeV1::I32,
                rg0_category,
                validity_ref_for(row_group0_id, 2),
                row_count,
            ),
            chunk_desc(
                row_group1_id,
                0,
                OcbPhysicalTypeV1::I32,
                rg1_partition,
                row_count,
            ),
            chunk_desc(
                row_group1_id,
                1,
                OcbPhysicalTypeV1::I64,
                rg1_order,
                row_count,
            ),
            chunk_desc(
                row_group1_id,
                2,
                OcbPhysicalTypeV1::I32,
                rg1_category,
                row_count,
            ),
        ],
        stats: vec![
            stats_i32(row_group0_id, 0, 10, 10),
            stats_i64(
                row_group0_id,
                1,
                100,
                *rg0_order_values
                    .last()
                    .expect("fixture row group is non-empty"),
            ),
            stats_i32(row_group1_id, 0, 11, 11),
            stats_i64(
                row_group1_id,
                1,
                200,
                *rg1_order_values
                    .last()
                    .expect("fixture row group is non-empty"),
            ),
        ],
        crc32c: 0,
    };
    let row_group_index_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
            row_group_index.write_to(buf)
        });

    let ordering = OcbOrderingProofV1 {
        version: 1,
        flags: 0b11,
        keys: vec![ordering_key(0), ordering_key(1)],
        row_group_proofs: vec![
            OcbRowGroupOrderingProofV1 {
                row_group_id: row_group0_id,
                flags: 1,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            },
            OcbRowGroupOrderingProofV1 {
                row_group_id: row_group1_id,
                flags: 1,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            },
        ],
        crc32c: 0,
    };
    let ordering_ref =
        append_encoded_object(&mut file_bytes, OcbBodyKindV1::OrderingProof, |buf| {
            ordering.write_to(buf)
        });

    let root = OcbRootV1 {
        version: 1,
        flags: 0,
        row_count: total_row_count,
        column_count: 3,
        row_group_count: 2,
        dictionary_count: 1,
        schema_ref,
        dictionary_index_ref,
        row_group_index_ref,
        ordering_proof_ref: ordering_ref,
        debug_json_ref: OcbBodyRefV2::NULL,
        created_unix_nanos: 0,
        content_flags: 0,
        crc32c: 0,
    };
    let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
        root.write_to(buf)
    });
    assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);

    let bootstrap = OcbBootstrapPageV1::new([42u8; 16], root_ref);
    let mut bootstrap_bytes = Vec::new();
    bootstrap
        .write_to(&mut bootstrap_bytes)
        .expect("write bootstrap");
    file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);

    let mut file = fs::File::create(path).expect("create fixture");
    file.write_all(&file_bytes).expect("write fixture");
}

fn append_chunk(
    file_bytes: &mut Vec<u8>,
    row_group_id: u32,
    column_id: u32,
    physical_type: OcbPhysicalTypeV1,
    values: &[i32],
) -> OcbBodyRefV2 {
    let mut payload = Vec::with_capacity(values.len() * 4);
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    let chunk = OcbColumnChunkObjectV1 {
        version: 1,
        physical_type,
        codec: OcbChunkCodecV1::None,
        flags: 0,
        row_group_id,
        column_id,
        row_count: values.len() as u64,
        uncompressed_bytes: (values.len() * 4) as u64,
        payload,
        crc32c: 0,
    };
    append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
        chunk.write_to(buf)
    })
}

fn append_chunk_i64(
    file_bytes: &mut Vec<u8>,
    row_group_id: u32,
    column_id: u32,
    values: &[i64],
) -> OcbBodyRefV2 {
    let mut payload = Vec::with_capacity(values.len() * 8);
    for value in values {
        payload.extend_from_slice(&value.to_le_bytes());
    }
    let chunk = OcbColumnChunkObjectV1 {
        version: 1,
        physical_type: OcbPhysicalTypeV1::I64,
        codec: OcbChunkCodecV1::None,
        flags: 0,
        row_group_id,
        column_id,
        row_count: values.len() as u64,
        uncompressed_bytes: (values.len() * 8) as u64,
        payload,
        crc32c: 0,
    };
    append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
        chunk.write_to(buf)
    })
}

fn append_fixed_binary_chunk(
    file_bytes: &mut Vec<u8>,
    row_group_id: u32,
    column_id: u32,
    width: u32,
    values: &[&[u8]],
    codec: OcbChunkCodecV1,
) -> OcbBodyRefV2 {
    let mut raw_payload = Vec::with_capacity(values.len() * width as usize);
    for value in values {
        assert_eq!(value.len(), width as usize);
        raw_payload.extend_from_slice(value);
    }
    let payload = match codec {
        OcbChunkCodecV1::None => raw_payload.clone(),
        OcbChunkCodecV1::Zstd => zstd::stream::encode_all(Cursor::new(raw_payload.as_slice()), 1)
            .expect("compress fixed-binary chunk"),
    };
    let chunk = OcbColumnChunkObjectV1 {
        version: 1,
        physical_type: OcbPhysicalTypeV1::FixedBinary,
        codec,
        flags: 0,
        row_group_id,
        column_id,
        row_count: values.len() as u64,
        uncompressed_bytes: raw_payload.len() as u64,
        payload,
        crc32c: 0,
    };
    append_encoded_object(file_bytes, OcbBodyKindV1::ColumnChunk, |buf| {
        chunk.write_to(buf)
    })
}

fn append_encoded_object(
    file_bytes: &mut Vec<u8>,
    kind: OcbBodyKindV1,
    write: impl FnOnce(&mut Vec<u8>) -> Result<()>,
) -> OcbBodyRefV2 {
    let mut object = Vec::new();
    write(&mut object).expect("encode object");
    append_raw_object(file_bytes, kind, object)
}

fn append_raw_object(
    file_bytes: &mut Vec<u8>,
    kind: OcbBodyKindV1,
    object: Vec<u8>,
) -> OcbBodyRefV2 {
    align_file(file_bytes, 8);
    let offset = file_bytes.len() as u64;
    let length = object.len() as u64;
    let checksum = crc32c(&object);
    file_bytes.extend_from_slice(&object);
    OcbBodyRefV2::new(offset, length, kind, checksum)
}

fn align_file(file_bytes: &mut Vec<u8>, alignment: usize) {
    let rem = file_bytes.len() % alignment;
    if rem != 0 {
        file_bytes.resize(file_bytes.len() + (alignment - rem), 0);
    }
}

fn column_desc(
    column_id: u32,
    name_string_id: u32,
    physical_type: OcbPhysicalTypeV1,
    logical_kind: OcbLogicalKindV1,
    dictionary_id: u32,
) -> OcbColumnDescV1 {
    OcbColumnDescV1 {
        column_id,
        name_string_id,
        physical_type,
        logical_kind,
        flags: 0,
        dictionary_id,
        scale: 0,
        nullability: OcbNullabilityV1::NonNull,
        reserved0: 0,
        fixed_binary_width: 0,
    }
}

fn fixed_binary_column_desc(column_id: u32, name_string_id: u32, width: u32) -> OcbColumnDescV1 {
    OcbColumnDescV1 {
        column_id,
        name_string_id,
        physical_type: OcbPhysicalTypeV1::FixedBinary,
        logical_kind: OcbLogicalKindV1::OpaqueKey,
        flags: 0,
        dictionary_id: OCB_NULL_U32,
        scale: 0,
        nullability: OcbNullabilityV1::NonNull,
        reserved0: 0,
        fixed_binary_width: width,
    }
}

fn chunk_desc(
    row_group_id: u32,
    column_id: u32,
    physical_type: OcbPhysicalTypeV1,
    value_ref: OcbBodyRefV2,
    row_count: u64,
) -> OcbColumnChunkDescV1 {
    chunk_desc_with_validity(
        row_group_id,
        column_id,
        physical_type,
        value_ref,
        OcbBodyRefV2::NULL,
        row_count,
    )
}

fn chunk_desc_with_codec_bytes(
    row_group_id: u32,
    column_id: u32,
    physical_type: OcbPhysicalTypeV1,
    codec: OcbChunkCodecV1,
    value_ref: OcbBodyRefV2,
    row_count: u64,
    uncompressed_bytes: u64,
) -> OcbColumnChunkDescV1 {
    OcbColumnChunkDescV1 {
        row_group_id,
        column_id,
        physical_type,
        codec,
        flags: 0,
        value_ref,
        validity_ref: OcbBodyRefV2::NULL,
        row_count,
        uncompressed_bytes,
    }
}

fn chunk_desc_with_validity(
    row_group_id: u32,
    column_id: u32,
    physical_type: OcbPhysicalTypeV1,
    value_ref: OcbBodyRefV2,
    validity_ref: OcbBodyRefV2,
    row_count: u64,
) -> OcbColumnChunkDescV1 {
    OcbColumnChunkDescV1 {
        row_group_id,
        column_id,
        physical_type,
        codec: OcbChunkCodecV1::None,
        flags: 0,
        value_ref,
        validity_ref,
        row_count,
        uncompressed_bytes: row_count
            * physical_type
                .primitive_byte_width()
                .expect("fixture uses primitive physical types") as u64,
    }
}

fn stats_i32(row_group_id: u32, column_id: u32, min: i32, max: i32) -> OcbColumnStatsV1 {
    OcbColumnStatsV1 {
        row_group_id,
        column_id,
        physical_type: OcbPhysicalTypeV1::I32,
        flags: 0,
        null_count: 0,
        min_value: OcbStatScalarV1::I32(min),
        max_value: OcbStatScalarV1::I32(max),
    }
}

fn stats_i64(row_group_id: u32, column_id: u32, min: i64, max: i64) -> OcbColumnStatsV1 {
    OcbColumnStatsV1 {
        row_group_id,
        column_id,
        physical_type: OcbPhysicalTypeV1::I64,
        flags: 0,
        null_count: 0,
        min_value: OcbStatScalarV1::I64(min),
        max_value: OcbStatScalarV1::I64(max),
    }
}

fn ordering_key(column_id: u32) -> OcbOrderingKeyV1 {
    OcbOrderingKeyV1 {
        column_id,
        direction: OcbOrderingDirectionV1::Ascending,
        null_order: OcbNullOrderV1::NoNulls,
        reserved0: 0,
    }
}
