#![cfg(feature = "format-ocb")]

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::ocb::{
    self, ColumnBundleFile, DecodedDictionaryValues, DictionaryValueKind, LogicalKind, NullOrder,
    OpenOptions as OcbOpenOptions, OpenValidation, OrderingDirection, PhysicalType, PrimitiveValues, Projection,
    ReadRequest, RowGroupPredicate,
    WriteColumn, WriteColumnChunk, WriteDictionary, WriteOrderingKey, WriteRowGroup, WriteSpec,
};

#[test]
fn ocb_safe_wrapper_create_append_read_and_cleanup_roundtrip() {
    let path = unique_path("ocb-safe-wrapper-roundtrip.ocb");
    let _ = fs::remove_file(&path);

    ocb::create(&path, &write_spec(&[10, 11], &[0, 1], &[1.5, 2.5])).expect("create OCB");
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

    ocb::append(&path, &write_spec(&[12, 13], &[1, 0], &[3.5, 4.5])).expect("append OCB");
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
    assert_eq!(full_validated.metadata().expect("full metadata").row_count, 4);
    let meta = file.metadata().expect("append metadata");
    assert_eq!(meta.root_generation, 2);
    assert_eq!(meta.previous_root_generation, Some(1));
    assert_eq!(meta.row_count, 4);
    assert_eq!(meta.row_group_count, 2);

    let dictionary = file.dictionary_values(0).expect("dictionary values");
    assert_eq!(dictionary.name, "category_dictionary");
    assert_eq!(
        dictionary.values,
        DecodedDictionaryValues::Utf8(vec!["alpha".to_string(), "beta".to_string()])
    );

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
