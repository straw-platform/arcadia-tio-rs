#![cfg(feature = "format-ocb")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::ocb::{
    self, BodyKind, ChecksumKind, ColumnBundleFile, ColumnChunkSummaryCodec,
    DecodedDictionaryValues, DictionaryValueKind, LogicalKind, NullOrder,
    OpenOptions as OcbOpenOptions, OpenValidation, OrderingDirection, OrderingKeyRange,
    PhysicalType, PredicateValue, PrimitiveValues, Projection, ReadRequest, RowGroupPredicate,
    WriteColumn, WriteColumnChunk, WriteDictionary, WriteOptions, WriteOrderingKey, WriteRowGroup,
    WriteSpec,
};

#[test]
fn ocb_safe_wrapper_create_append_read_and_cleanup_roundtrip() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ColumnBundleFile>();

    let path = unique_path("ocb-safe-wrapper-roundtrip.ocb");
    let _ = fs::remove_file(&path);

    ocb::create_with_options(
        &path,
        &write_spec(&[10, 11], &[0, 1], &[1.5, 2.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("create OCB");
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

    ocb::append_with_options(
        &path,
        &write_spec(&[12, 13], &[1, 0], &[3.5, 4.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )
    .expect("append OCB");
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

    drop(cloned_reader);
    drop(file);
    drop(create_snapshot);
    OpenOptions::new()
        .append(true)
        .open(&path)
        .expect("open for orphan tail")
        .write_all(b"orphan-tail")
        .expect("write orphan tail");
    let cleanup = ocb::cleanup_orphan_tail(&path).expect("cleanup orphan tail");
    assert!(cleanup.truncated);
    assert_eq!(
        ColumnBundleFile::open(&path)
            .expect("reopen after cleanup")
            .metadata()
            .expect("metadata")
            .row_count,
        4
    );

    let _ = fs::remove_file(path);
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
