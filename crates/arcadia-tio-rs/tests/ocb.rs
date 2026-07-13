#![cfg(feature = "format-ocb")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::ocb::{
    self, BodyKind, ChecksumKind, ColumnBundleFile, ColumnChunkSummaryCodec,
    CompactL2PhysicalV2ArtifactCertificationOptions, CompatibilityStatus, DecodedDictionaryValues,
    DictionaryValueKind, HealthStatus, LogicalKind, ManifestBuildOptions, NullOrder,
    OpenOptions as OcbOpenOptions, OpenValidation, OrderingDirection, OrderingKeyRange,
    ParallelReadNext, ParallelReadOptions, ParallelReadSession, PhysicalType, PredicateValue,
    PrimitiveValues, Projection, ReadRequest, RowGroupPredicate, WriteColumn, WriteColumnChunk,
    WriteDictionary, WriteOptions, WriteOrderingKey, WriteRowGroup, WriteSpec,
};

#[test]
fn ocb_safe_wrapper_create_append_read_and_cleanup_roundtrip() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ColumnBundleFile>();
    assert_send_sync::<ParallelReadSession>();

    let path = unique_path("ocb-safe-wrapper-roundtrip.ocb");
    let export_path = unique_path("ocb-safe-wrapper-roundtrip-export.ocb");
    let manifest_copy_path = unique_path("ocb-safe-wrapper-roundtrip-manifest-copy.ocb");
    let manifest_path = unique_path("ocb-safe-wrapper-roundtrip-manifest.json");
    let _ = fs::remove_file(&path);
    let _ = fs::remove_file(&export_path);
    let _ = fs::remove_file(&manifest_copy_path);
    let _ = fs::remove_file(&manifest_path);

    let create_report = ocb::create_with_report(
        &path,
        &write_spec(&[10, 11], &[0, 1], &[1.5, 2.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("create OCB");
    assert_eq!(create_report.requested_write_threads, 2);
    assert!(create_report.effective_write_threads >= 1);
    assert_eq!(create_report.row_count, 2);
    assert_eq!(create_report.row_group_count, 1);
    assert_eq!(create_report.column_count, 3);
    assert_eq!(create_report.column_chunk_count, 3);
    assert!(create_report.payload_bytes > 0);
    assert!(create_report.row_group_object_bytes > 0);
    assert!(create_report.file_bytes > 0);
    assert_eq!(create_report.root_generation, 1);
    assert_eq!(create_report.previous_root_generation, 0);
    let create_snapshot = ColumnBundleFile::open(&path).expect("open create snapshot");
    let create_meta = create_snapshot.metadata().expect("create metadata");
    assert_eq!(create_meta.format_name, "OCB");
    assert!(create_meta.appendable);
    assert_eq!(create_meta.root_generation, 1);
    assert_eq!(create_meta.previous_root_generation, None);
    assert_eq!(create_meta.row_count, 2);
    assert_eq!(create_meta.columns.len(), 3);
    assert_eq!(create_meta.dictionaries.len(), 1);
    assert_eq!(create_meta.ordering_keys.len(), 1);

    let append_report = ocb::append_with_report(
        &path,
        &write_spec(&[12, 13], &[1, 0], &[3.5, 4.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("append OCB");
    assert_eq!(append_report.requested_write_threads, 2);
    assert!(append_report.effective_write_threads >= 1);
    assert_eq!(append_report.row_count, 2);
    assert_eq!(append_report.row_group_count, 1);
    assert_eq!(append_report.column_count, 3);
    assert_eq!(append_report.column_chunk_count, 3);
    assert!(append_report.payload_bytes > 0);
    assert!(append_report.row_group_object_bytes > 0);
    assert!(append_report.file_bytes > 0);
    assert!(append_report.tail_bytes > 0);
    assert_eq!(append_report.root_generation, 2);
    assert_eq!(append_report.previous_root_generation, 1);
    assert_eq!(
        create_snapshot
            .metadata()
            .expect("snapshot metadata")
            .row_count,
        2
    );

    let file = ColumnBundleFile::open(&path).expect("open append snapshot");
    let full_validated = ColumnBundleFile::open_with_options(
        &path,
        OcbOpenOptions {
            validation: OpenValidation::FullPayload,
        },
    )
    .expect("open append snapshot with full validation");
    assert_eq!(
        full_validated.metadata().expect("full metadata").row_count,
        4
    );
    let meta = file.metadata().expect("append metadata");
    assert_eq!(meta.root_generation, 2);
    assert_eq!(meta.previous_root_generation, Some(1));
    assert_eq!(meta.row_count, 4);
    assert_eq!(meta.row_group_count, 2);

    let cloned_reader = file.clone_reader().expect("clone selected snapshot reader");
    assert_eq!(
        cloned_reader.metadata().expect("cloned reader metadata"),
        meta
    );

    let dictionary = file.dictionary_values(0).expect("dictionary values");
    assert_eq!(dictionary.name, "category_dictionary");
    assert_eq!(
        dictionary.values,
        DecodedDictionaryValues::Utf8(vec!["alpha".to_string(), "beta".to_string()])
    );

    let summaries = file.row_group_summaries().expect("row group summaries");
    assert_eq!(summaries.len(), 2);
    assert_eq!(summaries[0].row_group_id, 0);
    assert_eq!(summaries[0].chunks.len(), 3);
    assert_eq!(summaries[0].stats.len(), 2);
    assert_eq!(summaries[0].chunks[0].value_ref.kind, BodyKind::ColumnChunk);
    assert_eq!(
        summaries[0].chunks[0].value_ref.checksum_kind,
        ChecksumKind::Crc32c
    );
    assert!(matches!(
        summaries[0].chunks[0].codec,
        ColumnChunkSummaryCodec::None | ColumnChunkSummaryCodec::Zstd
    ));

    let default_outcome = file
        .read_batches(&ReadRequest::default())
        .expect("default read batches");
    assert_eq!(default_outcome.report.requested_threads, 1);

    let request = ReadRequest {
        projection: Projection::Names(vec!["sequence_key".to_string(), "metric".to_string()]),
        max_threads: 2,
        ..ReadRequest::default()
    };
    let outcome = file.read_batches(&request).expect("read projected batches");
    assert_eq!(outcome.batches.len(), 2);
    assert_eq!(outcome.report.requested_threads, 2);
    assert_eq!(outcome.report.selected_row_groups, 2);
    assert_eq!(outcome.batches[0].base_row, 0);
    assert_eq!(outcome.batches[1].base_row, 2);
    assert_eq!(
        outcome.batches[0].columns[0].values,
        PrimitiveValues::I64(vec![10, 11])
    );
    assert_eq!(
        outcome.batches[1].columns[1].values,
        PrimitiveValues::F64(vec![3.5, 4.5])
    );

    let attributed = file
        .read_batches_with_attribution(&request)
        .expect("attributed projected read");
    assert_eq!(attributed.outcome.batches, outcome.batches);
    assert_eq!(attributed.attribution.selected_row_groups, 2);
    assert_eq!(attributed.attribution.selected_column_chunks, 4);
    assert!(attributed.attribution.execute_wall_ns > 0);
    assert!(attributed.attribution.row_group_read_ns > 0);
    assert!(attributed.attribution.read_io_ns > 0);
    assert!(attributed.attribution.bytes_read > 0);
    assert!(attributed.attribution.native_to_c_copy_ns.is_some());
    assert!(attributed.attribution.wrapper_copy_ns.is_some());

    let parallel = file
        .parallel_read_session(
            &request,
            &[],
            ParallelReadOptions {
                max_in_flight_row_groups: 2,
            },
        )
        .expect("create parallel read session");
    let mut parallel_ids = Vec::new();
    loop {
        match parallel.next().expect("next parallel batch") {
            ParallelReadNext::Batch(result) => {
                assert_eq!(
                    result.context.selected_row_group_ordinal,
                    parallel_ids.len()
                );
                assert_eq!(result.context.row_group_id, result.batch.row_group_id);
                parallel_ids.push(result.batch.row_group_id);
            }
            ParallelReadNext::End => break,
            ParallelReadNext::Cancelled => panic!("completed parallel session was cancelled"),
        }
    }
    assert_eq!(parallel_ids, vec![0, 1]);
    assert_eq!(
        parallel.next().expect("repeat parallel end"),
        ParallelReadNext::End
    );
    let parallel_report = parallel.report().expect("parallel terminal report");
    assert_eq!(parallel_report.cursor_report.batches_yielded, 2);
    assert_eq!(parallel_report.cursor_report.rows_yielded, 4);
    assert_eq!(parallel_report.row_groups_ordered_committed, 2);
    assert_eq!(parallel_report.rows_ordered_committed, 4);
    assert!(parallel_report.ordered_terminal_completed);
    assert!(parallel_report.max_in_flight_row_groups_observed <= 2);
    assert_eq!(
        parallel_report.worker_reports.len(),
        parallel_report.started_workers
    );

    let subset = file
        .parallel_read_session(
            &request,
            &[1, 0],
            ParallelReadOptions {
                max_in_flight_row_groups: 2,
            },
        )
        .expect("create parallel subset session");
    let mut subset_ids = Vec::new();
    loop {
        match subset.next().expect("next parallel subset batch") {
            ParallelReadNext::Batch(result) => subset_ids.push(result.batch.row_group_id),
            ParallelReadNext::End => break,
            ParallelReadNext::Cancelled => panic!("parallel subset was cancelled"),
        }
    }
    assert_eq!(subset_ids, vec![0, 1]);

    let cancelled = file
        .parallel_read_session(
            &request,
            &[],
            ParallelReadOptions {
                max_in_flight_row_groups: 2,
            },
        )
        .expect("create cancellable parallel session");
    cancelled.cancel().expect("cancel parallel session");
    cancelled.cancel().expect("repeat parallel cancellation");
    let cancel_terminal = cancelled.next().expect("cancel-race parallel terminal");
    let cancel_report = cancelled.report().expect("cancel-race parallel report");
    match cancel_terminal {
        ParallelReadNext::Cancelled => {
            assert!(cancel_report.cursor_report.cancelled);
            assert!(!cancel_report.ordered_terminal_completed);
        }
        ParallelReadNext::End => {
            assert!(!cancel_report.cursor_report.cancelled);
            assert!(cancel_report.ordered_terminal_completed);
        }
        ParallelReadNext::Batch(_) => panic!("cancel returned a non-terminal batch"),
    }

    let active = file
        .parallel_read_session(
            &request,
            &[],
            ParallelReadOptions {
                max_in_flight_row_groups: 1,
            },
        )
        .expect("create active unconsumed session");
    assert!(active.report().is_err());
    drop(active);

    let plan = file.plan_read(&request).expect("plan projected read");
    assert_eq!(plan.projected_column_ids, vec![0, 2]);
    assert_eq!(plan.row_group_ids, vec![0, 1]);
    assert_eq!(plan.report.selected_row_groups, 2);
    let plan_summaries = file
        .read_plan_row_group_summaries(&plan)
        .expect("plan row group summaries");
    assert_eq!(plan_summaries.len(), 2);
    assert_eq!(plan_summaries[0].chunks.len(), 2);
    let planned_outcome = file.read_plan_batches(&plan).expect("execute full plan");
    assert_eq!(planned_outcome.batches, outcome.batches);
    let subset_outcome = file
        .read_plan_row_groups(&plan, &[1, 0])
        .expect("execute subset in deterministic plan order");
    assert_eq!(subset_outcome.batches.len(), 2);
    assert_eq!(subset_outcome.batches[0].row_group_id, 0);
    assert_eq!(subset_outcome.batches[1].row_group_id, 1);
    let duplicate_subset = file
        .read_plan_row_groups(&plan, &[1, 1])
        .expect_err("duplicate subset rejects");
    assert!(duplicate_subset.message().contains("duplicate"));
    drop(plan);

    let mut visited = Vec::new();
    let cursor_report = file
        .visit_batches(
            &request,
            arcadia_tio_rs::ocb::ReadCursorOptions {
                max_in_flight_row_groups: 1,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(arcadia_tio_rs::ocb::VisitControl::Continue)
            },
        )
        .expect("visit projected batches");
    assert_eq!(visited, vec![0, 1]);
    assert_eq!(cursor_report.batches_yielded, 2);
    assert_eq!(cursor_report.rows_yielded, 4);
    assert!(!cursor_report.cancelled);

    let mut visited = Vec::new();
    let cursor_report = file
        .visit_batches(
            &request,
            arcadia_tio_rs::ocb::ReadCursorOptions {
                max_in_flight_row_groups: 2,
                ordered: true,
            },
            |batch| {
                visited.push(batch.row_group_id);
                Ok(arcadia_tio_rs::ocb::VisitControl::Stop)
            },
        )
        .expect("visit projected batches with stop");
    assert_eq!(visited, vec![0]);
    assert_eq!(cursor_report.batches_yielded, 1);
    assert!(cursor_report.cancelled);

    let mut filled_sequence = [0i64; 2];
    let mut filled_metric = [0.0f64; 2];
    let fill_report = file
        .read_row_group_into(
            1,
            &mut [
                arcadia_tio_rs::ocb::ColumnFillBufferMut::I64ById {
                    column_id: 0,
                    values: &mut filled_sequence,
                    validity: None,
                    allow_nulls: false,
                },
                arcadia_tio_rs::ocb::ColumnFillBufferMut::F64 {
                    name: "metric",
                    values: &mut filled_metric,
                    validity: None,
                    allow_nulls: true,
                },
            ],
            arcadia_tio_rs::ocb::ReadFillOptions::default(),
        )
        .expect("fill row group into caller buffers");
    assert_eq!(filled_sequence, [12, 13]);
    assert_eq!(filled_metric, [3.5, 4.5]);
    assert_eq!(fill_report.row_group_id, 1);
    assert_eq!(fill_report.base_row, 2);
    assert_eq!(fill_report.row_count, 2);
    assert_eq!(fill_report.columns.len(), 2);
    assert_eq!(fill_report.columns[0].column_id, 0);
    assert_eq!(fill_report.columns[0].rows_filled, 2);
    assert!(!fill_report.columns[0].validity_filled);

    let mut short_sequence = [0i64; 1];
    let fill_err = file
        .read_row_group_into(
            1,
            &mut [arcadia_tio_rs::ocb::ColumnFillBufferMut::I64 {
                name: "sequence_key",
                values: &mut short_sequence,
                validity: None,
                allow_nulls: false,
            }],
            arcadia_tio_rs::ocb::ReadFillOptions::default(),
        )
        .expect_err("short fill buffer rejects");
    assert!(fill_err.message().contains("capacity"));

    let predicate_request = ReadRequest {
        projection: Projection::Names(vec!["category_code".to_string()]),
        predicates: vec![RowGroupPredicate {
            column: "sequence_key".to_string(),
            lower: Some(arcadia_tio_rs::ocb::PredicateValue::I64(12)),
            upper: Some(arcadia_tio_rs::ocb::PredicateValue::I64(13)),
        }],
        max_threads: 1,
        ..ReadRequest::default()
    };
    let predicate_outcome = file
        .read_batches(&predicate_request)
        .expect("read predicate-pruned batches");
    assert_eq!(predicate_outcome.batches.len(), 1);
    assert_eq!(predicate_outcome.batches[0].base_row, 2);
    assert_eq!(
        predicate_outcome.batches[0].columns[0].values,
        PrimitiveValues::I32(vec![1, 0])
    );

    let range_request = ReadRequest::from_ordering_key_ranges(
        &meta,
        Projection::Names(vec!["category_code".to_string()]),
        vec![OrderingKeyRange::between(
            0,
            PredicateValue::I64(12),
            PredicateValue::I64(13),
        )],
    )
    .expect("build ordering-key range request");
    assert_eq!(range_request.predicates, predicate_request.predicates);
    let range_outcome = file
        .read_batches(&range_request)
        .expect("read ordering-key range request");
    assert_eq!(range_outcome.batches, predicate_outcome.batches);
    let inverted_range = ReadRequest::from_ordering_key_ranges(
        &meta,
        Projection::All,
        vec![OrderingKeyRange::between(
            0,
            PredicateValue::I64(13),
            PredicateValue::I64(12),
        )],
    )
    .expect_err("inverted ordering range rejects");
    assert!(inverted_range.message().contains("lower bound"));
    let wrong_dtype_range = ReadRequest::from_ordering_key_ranges(
        &meta,
        Projection::All,
        vec![OrderingKeyRange::equal(0, PredicateValue::I32(12))],
    )
    .expect_err("dtype-mismatched ordering range rejects");
    assert!(wrong_dtype_range.message().contains("dtype"));
    let preserved_options = ReadRequest {
        projection: Projection::Names(vec!["metric".to_string()]),
        max_threads: 4,
        validate_checksums: false,
        decode_dictionaries: true,
        ..ReadRequest::default()
    }
    .with_ordering_key_ranges(
        &meta,
        vec![OrderingKeyRange::equal(0, PredicateValue::I64(12))],
    )
    .expect("range helper preserves options");
    assert_eq!(preserved_options.max_threads, 4);
    assert!(!preserved_options.validate_checksums);
    assert!(preserved_options.decode_dictionaries);
    assert_eq!(
        preserved_options.projection,
        Projection::Names(vec!["metric".to_string()])
    );
    assert_eq!(preserved_options.predicates.len(), 1);
    assert!(
        ReadRequest::from_ordering_key_ranges(&meta, Projection::All, vec![])
            .expect_err("empty ordering range rejects")
            .message()
            .contains("at least one")
    );
    assert!(
        ReadRequest::from_ordering_key_ranges(
            &meta,
            Projection::All,
            vec![
                OrderingKeyRange::equal(0, PredicateValue::I64(12)),
                OrderingKeyRange::equal(0, PredicateValue::I64(13)),
            ],
        )
        .expect_err("duplicate ordering range rejects")
        .message()
        .contains("duplicate")
    );
    assert!(
        ReadRequest::from_ordering_key_ranges(
            &meta,
            Projection::All,
            vec![OrderingKeyRange::new(0, None, None)],
        )
        .expect_err("empty-sided ordering range rejects")
        .message()
        .contains("at least one side")
    );
    assert!(
        ReadRequest::from_ordering_key_ranges(
            &meta,
            Projection::All,
            vec![OrderingKeyRange::equal(9, PredicateValue::I64(12))],
        )
        .expect_err("unknown ordering key rejects")
        .message()
        .contains("unknown ordering key")
    );
    let mut fixed_binary_meta = meta.clone();
    fixed_binary_meta.columns[0].physical_type = PhysicalType::FixedBinary { width: 8 };
    assert!(
        ReadRequest::from_ordering_key_ranges(
            &fixed_binary_meta,
            Projection::All,
            vec![OrderingKeyRange::equal(0, PredicateValue::I64(12))],
        )
        .expect_err("fixed-binary ordering key rejects")
        .message()
        .contains("fixed-binary")
    );

    ocb::create_with_options(
        &manifest_copy_path,
        &write_spec(&[20, 21], &[0, 1], &[5.5, 6.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("create manifest-compatible OCB");
    ocb::append_with_options(
        &manifest_copy_path,
        &write_spec(&[22, 23], &[1, 0], &[7.5, 8.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("append manifest-compatible OCB");
    let manifest = ocb::build_manifest_from_files_with_options(
        &manifest_path,
        [&path, &manifest_copy_path],
        ManifestBuildOptions {
            validation: OpenValidation::FullPayload,
            compute_file_digest: true,
            generated_by_name: Some("rust_safe_wrapper_test".to_string()),
            generated_by_version: Some("1".to_string()),
            generated_at_unix_seconds: Some(1),
        },
    )
    .expect("build selected-snapshot manifest");
    assert_eq!(manifest.schema, "arcadia-tio.ocb.manifest.v1");
    assert_eq!(manifest.generated_by.name, "rust_safe_wrapper_test");
    assert_eq!(manifest.generated_by.version, "1");
    assert_eq!(manifest.generated_by.generated_at_unix_seconds, 1);
    assert_eq!(manifest.entries.len(), 2);
    assert!(manifest.entries[0].file_bytes.is_some());
    assert_eq!(
        manifest.entries[0]
            .digest
            .as_ref()
            .expect("digest")
            .algorithm,
        "sha256"
    );
    assert!(!manifest.entries[0].fingerprints.combined.is_empty());
    assert_eq!(manifest.entries[0].validation.status, "valid");

    let report = ocb::validate_manifest_files_with_options(
        &manifest_path,
        &manifest,
        OcbOpenOptions {
            validation: OpenValidation::FullPayload,
        },
    )
    .expect("validate selected-snapshot manifest");
    assert_eq!(report.status, CompatibilityStatus::Compatible);
    assert_eq!(report.validation, OpenValidation::FullPayload);
    assert_eq!(report.entries_checked, 2);
    assert!(report.issues.is_empty());

    let mut missing_manifest = manifest.clone();
    missing_manifest.entries[0].path =
        "rust-safe-wrapper-missing-manifest-entry-does-not-exist.ocb".to_string();
    let missing_report = ocb::validate_manifest_files(&manifest_path, &missing_manifest)
        .expect("missing file is an in-band manifest issue");
    assert_eq!(missing_report.status, CompatibilityStatus::Incompatible);
    assert_eq!(missing_report.entries_checked, 2);
    assert!(missing_report.issues.iter().any(|issue| {
        issue.code == "ocb.io.failure"
            && issue.field_path.as_deref() == Some("entries[0].path")
            && !issue.message.is_empty()
    }));

    drop(cloned_reader);
    drop(file);
    drop(create_snapshot);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open for orphan tail")
        .write_all(b"orphan-tail")
        .expect("write orphan tail");
    let export_report = ocb::copy_selected_snapshot(
        &path,
        &export_path,
        ocb::SnapshotExportOptions {
            validation: OpenValidation::FullPayload,
        },
    )
    .expect("export selected snapshot");
    assert_eq!(export_report.validation, OpenValidation::FullPayload);
    assert!(export_report.source_file_bytes > export_report.destination_file_bytes);
    assert_eq!(
        export_report.orphan_tail_bytes_excluded,
        export_report.source_file_bytes - export_report.destination_file_bytes
    );
    assert_eq!(
        export_report.bytes_copied,
        export_report.destination_file_bytes
    );
    assert_eq!(export_report.root_generation, 2);
    assert_eq!(export_report.previous_root_generation, Some(1));
    assert_eq!(export_report.row_count, 4);
    assert_eq!(export_report.row_group_count, 2);
    assert!(!export_report.fingerprints.algorithm.is_empty());
    assert!(!export_report.fingerprints.combined.is_empty());
    assert_eq!(
        ColumnBundleFile::open(&export_path)
            .expect("open exported selected snapshot")
            .metadata()
            .expect("export metadata")
            .row_count,
        4
    );
    let existing_export =
        ocb::copy_selected_snapshot(&path, &export_path, ocb::SnapshotExportOptions::default())
            .expect_err("existing destination rejects");
    assert!(
        existing_export
            .message()
            .contains("destination path already exists")
    );
    let maintenance = ocb::maintenance_analyze(&path).expect("maintenance analyze");
    assert_eq!(maintenance.status, HealthStatus::Valid);
    assert_eq!(maintenance.selected_root_generation, Some(2));
    assert_eq!(maintenance.previous_root_generation, Some(1));
    assert!(maintenance.selected_slot_id.is_some());
    assert!(maintenance.selected_root_end_offset.is_some());
    assert!(maintenance.selected_snapshot_end_offset.is_some());
    assert!(maintenance.orphan_tail_bytes.unwrap_or_default() > 0);
    assert!(maintenance.cleanup_recommended);
    assert!(!maintenance.root_candidate_rejection_observed);
    assert_eq!(maintenance.rejected_root_candidate_count, 0);
    assert!(maintenance.rejected_root_candidates.is_empty());
    assert!(maintenance.issues.is_empty());
    let cleanup_report =
        ocb::cleanup_orphan_tail_report(&path).expect("cleanup orphan tail report");
    assert!(cleanup_report.truncated);
    assert_eq!(cleanup_report.selected_root_generation, 2);
    assert_eq!(cleanup_report.previous_root_generation, Some(1));
    assert_eq!(
        cleanup_report.orphan_tail_bytes_before,
        maintenance.orphan_tail_bytes.unwrap()
    );
    assert_eq!(cleanup_report.orphan_tail_bytes_after, 0);
    assert_eq!(
        cleanup_report.bytes_removed,
        maintenance.orphan_tail_bytes.unwrap()
    );
    assert!(cleanup_report.issues.is_empty());
    let cleanup = ocb::cleanup_orphan_tail(&path).expect("cleanup orphan tail");
    assert!(!cleanup.truncated);
    assert_eq!(
        ColumnBundleFile::open(&path)
            .expect("reopen after cleanup")
            .metadata()
            .expect("metadata")
            .row_count,
        4
    );

    let _ = fs::remove_file(path);
    let _ = fs::remove_file(export_path);
    let _ = fs::remove_file(manifest_copy_path);
    let _ = fs::remove_file(manifest_path);
}

#[test]
fn ocb_safe_wrapper_fixed_binary_roundtrip_and_fill() {
    let path = unique_path("ocb-safe-wrapper-fixed-binary.ocb");
    let _ = fs::remove_file(&path);
    let payload: Vec<u8> = (0u8..8).collect();

    ocb::create(&path, &fixed_binary_spec(&[1, 2], payload.clone()))
        .expect("create fixed-binary OCB");
    let file = ColumnBundleFile::open(&path).expect("open fixed-binary OCB");
    let metadata = file.metadata().expect("fixed-binary metadata");
    assert_eq!(
        metadata.columns[1].physical_type,
        PhysicalType::FixedBinary { width: 4 }
    );

    let outcome = file
        .read_batches(&ReadRequest {
            projection: Projection::Names(vec!["payload".to_string()]),
            ..ReadRequest::default()
        })
        .expect("read fixed-binary payload");
    assert_eq!(
        outcome.batches[0].columns[0].values,
        PrimitiveValues::FixedBinary {
            width: 4,
            bytes: payload.clone(),
        }
    );

    let mut filled = [0u8; 8];
    let report = file
        .read_row_group_into(
            0,
            &mut [ocb::ColumnFillBufferMut::FixedBinary {
                name: "payload",
                width: 4,
                bytes: &mut filled,
                validity: None,
                allow_nulls: false,
            }],
            ocb::ReadFillOptions::default(),
        )
        .expect("fill fixed-binary payload");
    assert_eq!(filled, <[u8; 8]>::try_from(payload.as_slice()).unwrap());
    assert_eq!(report.columns[0].rows_filled, 2);

    let err = file
        .read_batches(&ReadRequest {
            projection: Projection::Names(vec!["payload".to_string()]),
            predicates: vec![RowGroupPredicate {
                column: "payload".to_string(),
                lower: Some(ocb::PredicateValue::I32(1)),
                upper: None,
            }],
            ..ReadRequest::default()
        })
        .expect_err("fixed-binary predicate rejects");
    assert!(err.message().contains("fixed-binary"));

    let _ = fs::remove_file(path);
}

#[test]
fn ocb_safe_wrapper_certifies_compact_l2_physical_v2_artifact() {
    let path = unique_path("ocb-safe-wrapper-physical-v2-artifact.ocb");
    let _ = fs::remove_file(&path);

    ocb::create(&path, &compact_l2_physical_v2_spec(7, &[1, 2]))
        .expect("create physical-v2 artifact");

    let report = ocb::certify_compact_l2_physical_v2_artifact(
        &path,
        CompactL2PhysicalV2ArtifactCertificationOptions {
            expected_row_count: Some(2),
            expected_trading_day: Some(20260702),
            expected_channel_id: Some(7),
            expected_first_biz_index: Some(1),
            expected_last_biz_index: Some(2),
            ..CompactL2PhysicalV2ArtifactCertificationOptions::default()
        },
    )
    .expect("certify physical-v2 artifact");
    assert_eq!(report.row_count, 2);
    assert_eq!(report.row_group_count, 1);
    assert_eq!(report.required_column_count, 20);
    assert!(report.selected_column_chunk_count > 0);
    assert!(report.selected_compressed_bytes > 0);
    assert!(report.selected_uncompressed_bytes > 0);
    assert_eq!(report.first_biz_index, Some(1));
    assert_eq!(report.last_biz_index, Some(2));
    assert_eq!(report.min_receive_nano, Some(1001));
    assert_eq!(report.max_receive_nano, Some(1002));
    assert_eq!(report.order_record_count, Some(2));
    assert_eq!(report.trade_record_count, Some(0));
    assert!(report.certified);
    assert!(report.path_redacted);
    assert!(!report.writes_transformed_artifacts);

    let err = ocb::certify_compact_l2_physical_v2_artifact(
        &path,
        CompactL2PhysicalV2ArtifactCertificationOptions {
            expected_channel_id: Some(8),
            ..CompactL2PhysicalV2ArtifactCertificationOptions::default()
        },
    )
    .expect_err("channel mismatch rejects");
    assert!(err.message().contains("ChannelID mismatch"));

    let _ = fs::remove_file(path);
}

#[test]
fn ocb_safe_wrapper_rejects_invalid_fixed_binary_writes_before_ffi() {
    let path = unique_path("ocb-safe-wrapper-bad-fixed-binary.ocb");
    let _ = fs::remove_file(&path);

    let mut zero_width_schema = fixed_binary_spec(&[1], vec![1, 2, 3, 4]);
    zero_width_schema.columns[1].physical_type = PhysicalType::FixedBinary { width: 0 };
    let err = ocb::create(&path, &zero_width_schema).expect_err("zero schema width rejects");
    assert!(err.message().contains("zero width"));

    let mut width_mismatch = fixed_binary_spec(&[1], vec![1, 2, 3, 4, 5]);
    if let PrimitiveValues::FixedBinary { width, .. } =
        &mut width_mismatch.row_groups[0].columns[1].values
    {
        *width = 5;
    }
    let err = ocb::create(&path, &width_mismatch).expect_err("value/schema width mismatch rejects");
    assert!(err.message().contains("does not match schema width"));

    let unaligned = fixed_binary_spec(&[1], vec![1, 2, 3]);
    let err = ocb::create(&path, &unaligned).expect_err("unaligned fixed-binary bytes reject");
    assert!(err.message().contains("not divisible by width"));

    assert!(!path.exists());
}

fn compact_l2_physical_v2_spec(channel_id: i64, biz_indexes: &[i64]) -> WriteSpec {
    let row_count = biz_indexes.len();
    let zero_words = vec![0; row_count];

    WriteSpec {
        columns: vec![
            physical_v2_column("day_key", PhysicalType::I32),
            physical_v2_column("channel_id", PhysicalType::I64),
            physical_v2_column("biz_index", PhysicalType::I64),
            physical_v2_column("receive_nano", PhysicalType::I64),
            physical_v2_column("source_ordinal", PhysicalType::I64),
            physical_v2_column("record_kind", PhysicalType::I32),
            physical_v2_column(
                "payload_header_bytes_11_12",
                PhysicalType::FixedBinary { width: 2 },
            ),
            physical_v2_column("payload_exchange_time", PhysicalType::I64),
            physical_v2_column("payload_symbol", PhysicalType::FixedBinary { width: 9 }),
            physical_v2_column(
                "payload_body_bytes_80_86",
                PhysicalType::FixedBinary { width: 7 },
            ),
            physical_v2_column("payload_body_word_88", PhysicalType::I64),
            physical_v2_column("payload_body_word_96", PhysicalType::I64),
            physical_v2_column("payload_body_word_104", PhysicalType::I64),
            physical_v2_column("payload_body_word_112", PhysicalType::I64),
            physical_v2_column("payload_body_word_120", PhysicalType::I64),
            physical_v2_column("payload_body_word_128", PhysicalType::I64),
            physical_v2_column("payload_body_word_136", PhysicalType::I64),
            physical_v2_column("payload_body_word_144", PhysicalType::I64),
            physical_v2_column("payload_body_word_152", PhysicalType::I64),
            physical_v2_column("payload_body_word_160", PhysicalType::I64),
        ],
        dictionaries: Vec::new(),
        row_groups: vec![WriteRowGroup {
            columns: vec![
                WriteColumnChunk {
                    column_id: 0,
                    values: PrimitiveValues::I32(vec![20260702; row_count]),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 1,
                    values: PrimitiveValues::I64(vec![channel_id; row_count]),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 2,
                    values: PrimitiveValues::I64(biz_indexes.to_vec()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 3,
                    values: PrimitiveValues::I64(
                        biz_indexes.iter().map(|value| value + 1000).collect(),
                    ),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 4,
                    values: PrimitiveValues::I64(biz_indexes.to_vec()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 5,
                    values: PrimitiveValues::I32(vec![1; row_count]),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 6,
                    values: PrimitiveValues::FixedBinary {
                        width: 2,
                        bytes: biz_indexes.iter().flat_map(|_| [3, 0]).collect(),
                    },
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 7,
                    values: PrimitiveValues::I64(
                        biz_indexes.iter().map(|value| value + 2000).collect(),
                    ),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 8,
                    values: PrimitiveValues::FixedBinary {
                        width: 9,
                        bytes: vec![0; row_count * 9],
                    },
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 9,
                    values: PrimitiveValues::FixedBinary {
                        width: 7,
                        bytes: vec![0; row_count * 7],
                    },
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 10,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 11,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 12,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 13,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 14,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 15,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 16,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 17,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 18,
                    values: PrimitiveValues::I64(zero_words.clone()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 19,
                    values: PrimitiveValues::I64(zero_words),
                    validity: None,
                },
            ],
        }],
        ordering_keys: vec![
            physical_v2_ordering_key(0),
            physical_v2_ordering_key(1),
            physical_v2_ordering_key(2),
            physical_v2_ordering_key(4),
        ],
    }
}

fn physical_v2_column(name: &str, physical_type: PhysicalType) -> WriteColumn {
    WriteColumn {
        name: name.to_string(),
        physical_type,
        logical_kind: LogicalKind::Plain,
        dictionary_id: None,
        scale: 0,
        nullable: false,
    }
}

fn physical_v2_ordering_key(column_id: u32) -> WriteOrderingKey {
    WriteOrderingKey {
        column_id,
        direction: OrderingDirection::Ascending,
        null_order: NullOrder::NoNulls,
    }
}

fn fixed_binary_spec(keys: &[i64], payload: Vec<u8>) -> WriteSpec {
    WriteSpec {
        columns: vec![
            WriteColumn {
                name: "key".to_string(),
                physical_type: PhysicalType::I64,
                logical_kind: LogicalKind::Plain,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "payload".to_string(),
                physical_type: PhysicalType::FixedBinary { width: 4 },
                logical_kind: LogicalKind::Plain,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
        ],
        dictionaries: Vec::new(),
        row_groups: vec![WriteRowGroup {
            columns: vec![
                WriteColumnChunk {
                    column_id: 0,
                    values: PrimitiveValues::I64(keys.to_vec()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 1,
                    values: PrimitiveValues::FixedBinary {
                        width: 4,
                        bytes: payload,
                    },
                    validity: None,
                },
            ],
        }],
        ordering_keys: vec![WriteOrderingKey {
            column_id: 0,
            direction: OrderingDirection::Ascending,
            null_order: NullOrder::NoNulls,
        }],
    }
}

fn write_spec(sequence_key: &[i64], category_code: &[i32], metric: &[f64]) -> WriteSpec {
    assert_eq!(sequence_key.len(), category_code.len());
    assert_eq!(sequence_key.len(), metric.len());
    WriteSpec {
        columns: vec![
            WriteColumn {
                name: "sequence_key".to_string(),
                physical_type: PhysicalType::I64,
                logical_kind: LogicalKind::OpaqueKey,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "category_code".to_string(),
                physical_type: PhysicalType::I32,
                logical_kind: LogicalKind::DictionaryCode,
                dictionary_id: Some(0),
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "metric".to_string(),
                physical_type: PhysicalType::F64,
                logical_kind: LogicalKind::Plain,
                dictionary_id: None,
                scale: 0,
                nullable: true,
            },
        ],
        dictionaries: vec![WriteDictionary {
            dictionary_id: 0,
            name: "category_dictionary".to_string(),
            code_physical_type: PhysicalType::I32,
            value_kind: DictionaryValueKind::Utf8,
            fixed_width: 0,
            entries: vec![b"alpha".to_vec(), b"beta".to_vec()],
        }],
        row_groups: vec![WriteRowGroup {
            columns: vec![
                WriteColumnChunk {
                    column_id: 0,
                    values: PrimitiveValues::I64(sequence_key.to_vec()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 1,
                    values: PrimitiveValues::I32(category_code.to_vec()),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 2,
                    values: PrimitiveValues::F64(metric.to_vec()),
                    validity: None,
                },
            ],
        }],
        ordering_keys: vec![WriteOrderingKey {
            column_id: 0,
            direction: OrderingDirection::Ascending,
            null_order: NullOrder::NoNulls,
        }],
    }
}

fn unique_path(name: &str) -> PathBuf {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/ocb-tests");
    fs::create_dir_all(&dir).expect("create project-local OCB test directory");
    dir.join(format!(
        "{}-{}-{name}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ))
}
