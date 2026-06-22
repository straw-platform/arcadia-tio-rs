use std::fs;

use arcadia_tio_rs::ocb::{
    self, ColumnBundleFile, ColumnFillBufferMut, LogicalKind, NullOrder, OrderingDirection,
    PhysicalType, PrimitiveValues, Projection, ReadFillOptions, ReadRequest, WriteColumn,
    WriteColumnChunk, WriteOrderingKey, WriteRowGroup, WriteSpec,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::current_dir()?.join("target/ocb-fixed-binary-example.ocb");
    let _ = fs::remove_file(&path);

    let width = 4;
    let payload: Vec<u8> = vec![
        0x10, 0x11, 0x12, 0x13, // row 0
        0x20, 0x21, 0x22, 0x23, // row 1
        0x30, 0x31, 0x32, 0x33, // row 2
    ];

    let spec = WriteSpec {
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
                name: "payload".to_string(),
                physical_type: PhysicalType::FixedBinary { width },
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
                    values: PrimitiveValues::I64(vec![1, 2, 3]),
                    validity: None,
                },
                WriteColumnChunk {
                    column_id: 1,
                    values: PrimitiveValues::FixedBinary {
                        width,
                        bytes: payload.clone(),
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
    };

    ocb::create(&path, &spec)?;
    let file = ColumnBundleFile::open(&path)?;
    let metadata = file.metadata()?;
    assert_eq!(
        metadata.columns[1].physical_type,
        PhysicalType::FixedBinary { width }
    );

    let read = file.read_batches(&ReadRequest {
        projection: Projection::Names(vec!["payload".to_string()]),
        ..ReadRequest::default()
    })?;
    assert_eq!(read.batches.len(), 1);
    assert_eq!(read.batches[0].row_count, 3);
    assert_eq!(
        read.batches[0].columns[0].values,
        PrimitiveValues::FixedBinary {
            width,
            bytes: payload.clone()
        }
    );

    let mut filled = vec![0u8; payload.len()];
    let fill_report = file.read_row_group_into(
        0,
        &mut [ColumnFillBufferMut::FixedBinary {
            name: "payload",
            width,
            bytes: &mut filled,
            validity: None,
            allow_nulls: false,
        }],
        ReadFillOptions::default(),
    )?;
    assert_eq!(fill_report.row_count, 3);
    assert_eq!(filled, payload);

    println!(
        "{} rows of {}-byte fixed-binary payloads",
        metadata.row_count, width
    );

    let _ = fs::remove_file(path);
    Ok(())
}
