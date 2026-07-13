//! Appendable OCB and bounded ordered parallel polling.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::ocb::{
    self, ColumnBundleFile, DecodedDictionaryValues, DictionaryValueKind, LogicalKind, NullOrder,
    OrderingDirection, ParallelReadNext, ParallelReadOptions, PhysicalType, PredicateValue,
    PrimitiveValues, Projection, ReadRequest, RowGroupPredicate, WriteColumn, WriteColumnChunk,
    WriteDictionary, WriteOptions, WriteOrderingKey, WriteRowGroup, WriteSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TutorialTempDir::new("ocb-roundtrip-parallel")?;
    let path = temp.path().join("tutorial.ocb");

    ocb::create_with_options(
        &path,
        &write_spec(&[1, 2], &[0, 1], &[1.5, 2.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )?;
    let first_snapshot = ColumnBundleFile::open(&path)?;
    assert_eq!(first_snapshot.metadata()?.row_count, 2);

    ocb::append_with_options(
        &path,
        &write_spec(&[10, 11], &[1, 0], &[10.5, 11.5]),
        WriteOptions::zstd(3).with_write_threads(2),
    )?;
    assert_eq!(first_snapshot.metadata()?.row_count, 2);

    let file = ColumnBundleFile::open(&path)?;
    let metadata = file.metadata()?;
    assert_eq!((metadata.row_count, metadata.row_group_count), (4, 2));
    assert_eq!(
        file.dictionary_values(0)?.values,
        DecodedDictionaryValues::Utf8(vec!["alpha".into(), "beta".into()])
    );

    let selected_request = ReadRequest {
        projection: Projection::Names(vec!["sequence_key".into(), "metric".into()]),
        predicates: vec![RowGroupPredicate {
            column: "sequence_key".into(),
            lower: Some(PredicateValue::I64(10)),
            upper: Some(PredicateValue::I64(11)),
        }],
        max_threads: 2,
        ..ReadRequest::default()
    };
    assert_eq!(file.row_group_summaries()?.len(), 2);
    let selected = file.read_batches(&selected_request)?;
    assert_eq!(selected.report.selected_row_groups, 1);
    assert_eq!(selected.report.pruned_row_groups, 1);
    assert_eq!(selected.batches[0].row_group_id, 1);
    assert_eq!(
        selected.batches[0].columns[1].values,
        PrimitiveValues::F64(vec![10.5, 11.5])
    );

    let projected_request = ReadRequest {
        projection: Projection::Names(vec!["sequence_key".into(), "metric".into()]),
        max_threads: 2,
        ..ReadRequest::default()
    };
    let plan = file.plan_read(&projected_request)?;
    assert_eq!(plan.projected_column_ids, vec![0, 2]);
    assert_eq!(plan.row_group_ids, vec![0, 1]);
    let subset = file.read_plan_row_groups(&plan, &[1, 0])?;
    assert_eq!(
        subset
            .batches
            .iter()
            .map(|batch| batch.row_group_id)
            .collect::<Vec<_>>(),
        vec![0, 1]
    );

    let session = file.parallel_read_session(
        &projected_request,
        &[],
        ParallelReadOptions {
            max_in_flight_row_groups: 2,
        },
    )?;
    let mut ids = Vec::new();
    loop {
        match session.next()? {
            ParallelReadNext::Batch(result) => ids.push(result.batch.row_group_id),
            ParallelReadNext::End => break,
            ParallelReadNext::Cancelled => panic!("completed session reported cancellation"),
        }
    }
    assert_eq!(ids, vec![0, 1]);
    let report = session.report()?;
    assert!(report.ordered_terminal_completed);
    assert!(report.max_in_flight_row_groups_observed <= 2);

    let cancelled = file.parallel_read_session(
        &projected_request,
        &[],
        ParallelReadOptions {
            max_in_flight_row_groups: 2,
        },
    )?;
    cancelled.cancel()?;
    cancelled.cancel()?;
    let cancel_terminal = cancelled.next()?;
    let cancel_report = cancelled.report()?;
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

    println!("OCB tutorial ok: append, projection/pruning, ordered polling, cancellation");
    Ok(())
}

fn write_spec(sequence_key: &[i64], category_code: &[i32], metric: &[f64]) -> WriteSpec {
    assert_eq!(sequence_key.len(), category_code.len());
    assert_eq!(sequence_key.len(), metric.len());
    WriteSpec {
        columns: vec![
            WriteColumn {
                name: "sequence_key".into(),
                physical_type: PhysicalType::I64,
                logical_kind: LogicalKind::OpaqueKey,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "category_code".into(),
                physical_type: PhysicalType::I32,
                logical_kind: LogicalKind::DictionaryCode,
                dictionary_id: Some(0),
                scale: 0,
                nullable: false,
            },
            WriteColumn {
                name: "metric".into(),
                physical_type: PhysicalType::F64,
                logical_kind: LogicalKind::Plain,
                dictionary_id: None,
                scale: 0,
                nullable: false,
            },
        ],
        dictionaries: vec![WriteDictionary {
            dictionary_id: 0,
            name: "categories".into(),
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

struct TutorialTempDir {
    path: PathBuf,
}

impl TutorialTempDir {
    fn new(name: &str) -> std::io::Result<Self> {
        let root = std::env::var_os("TIO_TUTORIAL_TMPDIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(".tmp/tutorials/rust")
            });
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let path = root.join(format!("{name}-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TutorialTempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
