//! Compact fixed-binary L2 envelope facts used by OCB certification helpers.
//!
//! This module intentionally exposes only source-format header facts and
//! fail-closed validation primitives. It does not implement order-book replay,
//! factor/KOB logic, owner assignment, or runtime scheduling.

use crate::column_bundle::{ColumnBatch, PrimitiveColumnValues};
use crate::{ArcadiaTioError, OcbErrorKind, Result};

/// Stable public Rust-core OCB reader API version.
pub const OCB_CORE_READER_API_VERSION: u16 = 1;
/// Channel-sharded manifest schema version supported by this crate.
pub const CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1: u16 = 1;
/// Compact fixed-binary L2 payload schema version supported by this crate.
pub const COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1: u16 = 1;

/// Compact fixed-binary L2 payload format label.
pub const COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1: &str = "compact-fixed-binary-l2-v1";
/// Proposed typed/split physical compact-L2 payload format label.
///
/// This label is additive. It does not replace
/// [`COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1`]. Use the physical-v2
/// manifest/artifact certification helpers to validate this layout explicitly.
pub const COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT: &str = "compact-l2-physical-v2";
/// Common compact-L2 day key column name.
pub const COMPACT_L2_DAY_KEY_COLUMN_NAME: &str = "day_key";
/// Common compact-L2 channel id column name.
pub const COMPACT_L2_CHANNEL_ID_COLUMN_NAME: &str = "channel_id";
/// Common compact-L2 BizIndex column name.
pub const COMPACT_L2_BIZ_INDEX_COLUMN_NAME: &str = "biz_index";
/// Common compact-L2 receive timestamp column name.
pub const COMPACT_L2_RECEIVE_NANO_COLUMN_NAME: &str = "receive_nano";
/// Common compact-L2 source ordinal column name.
pub const COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME: &str = "source_ordinal";
/// Common compact-L2 record kind column name.
pub const COMPACT_L2_RECORD_KIND_COLUMN_NAME: &str = "record_kind";
/// Canonical payload column name used by compact-L2 OCB artifacts.
pub const COMPACT_L2_PAYLOAD_COLUMN_NAME: &str = "payload";
/// Physical-v2 column carrying v1 header bytes 11..12.
pub const COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME: &str =
    "payload_header_bytes_11_12";
/// Physical-v2 column carrying v1 payload exchange time.
pub const COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME: &str = "payload_exchange_time";
/// Physical-v2 column carrying the nine-byte v1 payload symbol field.
pub const COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME: &str = "payload_symbol";
/// Physical-v2 column carrying v1 body bytes 80..86.
pub const COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME: &str = "payload_body_bytes_80_86";
/// Physical-v2 body-word column for v1 payload bytes 88..96.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_88_COLUMN_NAME: &str = "payload_body_word_88";
/// Physical-v2 body-word column for v1 payload bytes 96..104.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_96_COLUMN_NAME: &str = "payload_body_word_96";
/// Physical-v2 body-word column for v1 payload bytes 104..112.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_104_COLUMN_NAME: &str = "payload_body_word_104";
/// Physical-v2 body-word column for v1 payload bytes 112..120.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_112_COLUMN_NAME: &str = "payload_body_word_112";
/// Physical-v2 body-word column for v1 payload bytes 120..128.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_120_COLUMN_NAME: &str = "payload_body_word_120";
/// Physical-v2 body-word column for v1 payload bytes 128..136.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_128_COLUMN_NAME: &str = "payload_body_word_128";
/// Physical-v2 body-word column for v1 payload bytes 136..144.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_136_COLUMN_NAME: &str = "payload_body_word_136";
/// Physical-v2 body-word column for v1 payload bytes 144..152.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_144_COLUMN_NAME: &str = "payload_body_word_144";
/// Physical-v2 body-word column for v1 payload bytes 152..160.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_152_COLUMN_NAME: &str = "payload_body_word_152";
/// Physical-v2 body-word column for v1 payload bytes 160..168.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_160_COLUMN_NAME: &str = "payload_body_word_160";
/// Physical-v2 body-word offsets copied from the v1 fixed-binary payload.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS: [u16; 10] =
    [88, 96, 104, 112, 120, 128, 136, 144, 152, 160];
/// Fixed-ingress compact L2 magic bytes (`ALIR`).
pub const COMPACT_L2_FIXED_INGRESS_MAGIC: [u8; 4] = *b"ALIR";
/// V1 compact-L2 common header length in bytes.
pub const COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1: u16 = 80;
/// V1 compact-L2 fixed record width in bytes.
pub const COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1: u32 = 168;
/// V1 order record kind code.
pub const COMPACT_L2_RECORD_KIND_ORDER: u8 = 1;
/// V1 trade record kind code.
pub const COMPACT_L2_RECORD_KIND_TRADE: u8 = 2;
/// LF/Kungfu source-family code.
pub const COMPACT_L2_SOURCE_FAMILY_LF_KUNGFU: u8 = 1;
/// Arcadia-native source-family code.
pub const COMPACT_L2_SOURCE_FAMILY_ARCADIA_NATIVE: u8 = 2;
/// Synthetic source-family code.
pub const COMPACT_L2_SOURCE_FAMILY_SYNTHETIC: u8 = 3;

/// One physical-v2 body word lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactL2PhysicalV2BodyWordColumn {
    /// Byte offset in the v1 fixed-binary payload.
    pub offset: u16,
    /// Stable physical-v2 column name for this lane.
    pub column_name: &'static str,
}

/// Physical-v2 body word lane descriptors in payload offset order.
pub const COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS: [CompactL2PhysicalV2BodyWordColumn; 10] = [
    CompactL2PhysicalV2BodyWordColumn {
        offset: 88,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_88_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 96,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_96_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 104,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_104_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 112,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_112_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 120,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_120_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 128,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_128_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 136,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_136_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 144,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_144_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 152,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_152_COLUMN_NAME,
    },
    CompactL2PhysicalV2BodyWordColumn {
        offset: 160,
        column_name: COMPACT_L2_PHYSICAL_V2_BODY_WORD_160_COLUMN_NAME,
    },
];

/// Compact fixed-binary L2 record kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactL2RecordKind {
    /// Order mutation or sequence-placeholder row.
    Order,
    /// Trade/cancel mutation or sequence-placeholder row.
    Trade,
}

impl CompactL2RecordKind {
    /// Return the stable on-wire code.
    pub const fn code(self) -> u8 {
        match self {
            Self::Order => COMPACT_L2_RECORD_KIND_ORDER,
            Self::Trade => COMPACT_L2_RECORD_KIND_TRADE,
        }
    }
}

/// Source-visible compact-L2 fixed-ingress header fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactL2FixedBinaryHeaderV1 {
    /// Header magic; valid records carry [`COMPACT_L2_FIXED_INGRESS_MAGIC`].
    pub magic: [u8; 4],
    /// Compact-L2 fixed-ingress schema version.
    pub schema_version: u16,
    /// Header length in bytes.
    pub header_len: u16,
    /// Full compact record width in bytes.
    pub record_len: u16,
    /// Order/trade record kind.
    pub record_kind: CompactL2RecordKind,
    /// Source-family code.
    pub source_family: u8,
    /// Exchange id code.
    pub exchange_id: u8,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Source ChannelID.
    pub channel_id: i32,
    /// Channel-local BizIndex.
    pub biz_index: i64,
    /// Stable source/session tie-breaker.
    pub source_ordinal: u64,
    /// Receive/scheduling timestamp in nanoseconds, or zero when absent.
    pub receive_nano: i64,
    /// Source business/event timestamp in nanoseconds, or zero when absent.
    pub exchange_time: i64,
}

/// Typed/split physical-v2 payload fields for one compact-L2 row.
///
/// The structure is intentionally physical, not semantic: order and trade rows
/// share the same lanes, and [`CompactL2RecordKind`] determines how downstream
/// business code interprets those lanes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactL2PhysicalV2Record {
    /// Order/trade row tag.
    pub record_kind: CompactL2RecordKind,
    /// Source-family byte from v1 payload offset 11.
    pub source_family: u8,
    /// Exchange id byte from v1 payload offset 12.
    pub exchange_id: u8,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Source ChannelID.
    pub channel_id: i32,
    /// Channel-local BizIndex.
    pub biz_index: i64,
    /// Stable source/session tie-breaker.
    pub source_ordinal: u64,
    /// Receive/scheduling timestamp in nanoseconds, or zero when absent.
    pub receive_nano: i64,
    /// Source business/event timestamp in nanoseconds, or zero when absent.
    pub exchange_time: i64,
    /// V1 payload symbol bytes from offsets 56..65.
    pub symbol: [u8; 9],
    /// V1 compact body bytes from offsets 80..87.
    pub body_bytes_80_86: [u8; 7],
    /// V1 compact body i64 lanes at offsets
    /// [`COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS`].
    pub body_words_88_160: [i64; 10],
}

impl CompactL2PhysicalV2Record {
    /// Split one validated v1 fixed-binary payload record into physical-v2 lanes.
    pub fn from_fixed_binary_v1(record: &[u8]) -> Result<Self> {
        let header = decode_compact_l2_fixed_binary_header_v1(record)?;
        let mut symbol = [0u8; 9];
        symbol.copy_from_slice(&record[56..65]);
        let mut body_bytes_80_86 = [0u8; 7];
        body_bytes_80_86.copy_from_slice(&record[80..87]);
        let mut body_words_88_160 = [0i64; 10];
        for (slot, offset) in COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS
            .iter()
            .copied()
            .enumerate()
        {
            let offset = usize::from(offset);
            body_words_88_160[slot] =
                i64::from_le_bytes(record[offset..offset + 8].try_into().expect("i64 lane"));
        }
        Ok(Self {
            record_kind: header.record_kind,
            source_family: header.source_family,
            exchange_id: header.exchange_id,
            trading_day: header.trading_day,
            channel_id: header.channel_id,
            biz_index: header.biz_index,
            source_ordinal: header.source_ordinal,
            receive_nano: header.receive_nano,
            exchange_time: header.exchange_time,
            symbol,
            body_bytes_80_86,
            body_words_88_160,
        })
    }

    /// Split one v1 payload record and verify duplicated scalar columns.
    ///
    /// This matches the compact-L2 v1 OCB shape where common scalar columns
    /// duplicate fixed-ingress header fields.
    pub fn from_fixed_binary_v1_with_scalar_columns(
        record: &[u8],
        day_key: i32,
        channel_id: i64,
        biz_index: i64,
        receive_nano: i64,
        source_ordinal: i64,
        record_kind: i32,
    ) -> Result<Self> {
        let decoded = Self::from_fixed_binary_v1(record)?;
        if u32::try_from(day_key).ok() != Some(decoded.trading_day) {
            return Err(scalar_payload_mismatch("day_key scalar/payload mismatch"));
        }
        if channel_id != i64::from(decoded.channel_id) {
            return Err(scalar_payload_mismatch(
                "channel_id scalar/payload mismatch",
            ));
        }
        if biz_index != decoded.biz_index {
            return Err(scalar_payload_mismatch("biz_index scalar/payload mismatch"));
        }
        if receive_nano != decoded.receive_nano {
            return Err(scalar_payload_mismatch(
                "receive_nano scalar/payload mismatch",
            ));
        }
        if u64::try_from(source_ordinal).ok() != Some(decoded.source_ordinal) {
            return Err(scalar_payload_mismatch(
                "source_ordinal scalar/payload mismatch",
            ));
        }
        if record_kind != i32::from(decoded.record_kind.code()) {
            return Err(scalar_payload_mismatch(
                "record_kind scalar/payload mismatch",
            ));
        }
        Ok(decoded)
    }

    /// Reconstruct the legacy 168-byte fixed-binary payload.
    ///
    /// Production v2 readers should prefer typed lanes directly. This method is
    /// a compatibility and verification path for existing fixed-binary readers.
    pub fn to_fixed_binary_v1(
        &self,
    ) -> Result<[u8; COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize]> {
        let mut record = [0u8; COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize];
        record[0..4].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_MAGIC);
        record[4..6].copy_from_slice(&COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1.to_le_bytes());
        record[6..8].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1.to_le_bytes());
        record[8..10]
            .copy_from_slice(&(COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as u16).to_le_bytes());
        record[10] = self.record_kind.code();
        record[11] = self.source_family;
        record[12] = self.exchange_id;
        record[13] = 0;
        record[14..16].copy_from_slice(&0u16.to_le_bytes());
        record[16..20].copy_from_slice(&self.trading_day.to_le_bytes());
        record[20..24].copy_from_slice(&self.channel_id.to_le_bytes());
        record[24..32].copy_from_slice(&self.biz_index.to_le_bytes());
        record[32..40].copy_from_slice(&self.source_ordinal.to_le_bytes());
        record[40..48].copy_from_slice(&self.receive_nano.to_le_bytes());
        record[48..56].copy_from_slice(&self.exchange_time.to_le_bytes());
        record[56..65].copy_from_slice(&self.symbol);
        record[65..80].fill(0);
        record[80..87].copy_from_slice(&self.body_bytes_80_86);
        record[87] = 0;
        for (slot, offset) in COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS
            .iter()
            .copied()
            .enumerate()
        {
            let offset = usize::from(offset);
            record[offset..offset + 8].copy_from_slice(&self.body_words_88_160[slot].to_le_bytes());
        }
        decode_compact_l2_fixed_binary_header_v1(&record)?;
        Ok(record)
    }
}

/// Borrowed physical-v2 column views for one decoded OCB row-group batch.
///
/// This is a physical compatibility view over the compact-L2 v2 layout. It is
/// intentionally not an order/trade semantic schema; callers should use
/// `record_kind` to interpret body lanes when they need business meaning.
#[derive(Debug, Clone, Copy)]
pub struct CompactL2PhysicalV2BatchView<'a> {
    /// Number of rows in the batch.
    pub row_count: usize,
    /// Common scalar `day_key` column.
    pub day_key: &'a [i32],
    /// Common scalar `channel_id` column.
    pub channel_id: &'a [i64],
    /// Common scalar `biz_index` column.
    pub biz_index: &'a [i64],
    /// Common scalar `receive_nano` column.
    pub receive_nano: &'a [i64],
    /// Common scalar `source_ordinal` column.
    pub source_ordinal: &'a [i64],
    /// Common scalar `record_kind` column.
    pub record_kind: &'a [i32],
    /// Physical-v2 bytes for v1 payload offsets 11..13.
    pub header_bytes_11_12: &'a [u8],
    /// Physical-v2 exchange-time lane.
    pub exchange_time: &'a [i64],
    /// Physical-v2 symbol lane, width 9.
    pub symbol: &'a [u8],
    /// Physical-v2 body bytes for v1 payload offsets 80..87.
    pub body_bytes_80_86: &'a [u8],
    /// Physical-v2 body word lanes for offsets 88..160.
    pub body_words_88_160: [&'a [i64]; 10],
}

impl<'a> CompactL2PhysicalV2BatchView<'a> {
    /// Build a physical-v2 view from an OCB decoded row-group batch.
    pub fn from_column_batch(batch: &'a ColumnBatch) -> Result<Self> {
        let row_count = usize::try_from(batch.row_count).map_err(|_| {
            compact_l2_v2_batch_error("compact-L2 physical-v2 row count exceeds addressable memory")
        })?;
        let view = Self {
            row_count,
            day_key: compact_l2_column_i32(batch, COMPACT_L2_DAY_KEY_COLUMN_NAME, row_count)?,
            channel_id: compact_l2_column_i64(batch, COMPACT_L2_CHANNEL_ID_COLUMN_NAME, row_count)?,
            biz_index: compact_l2_column_i64(batch, COMPACT_L2_BIZ_INDEX_COLUMN_NAME, row_count)?,
            receive_nano: compact_l2_column_i64(
                batch,
                COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
                row_count,
            )?,
            source_ordinal: compact_l2_column_i64(
                batch,
                COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME,
                row_count,
            )?,
            record_kind: compact_l2_column_i32(
                batch,
                COMPACT_L2_RECORD_KIND_COLUMN_NAME,
                row_count,
            )?,
            header_bytes_11_12: compact_l2_column_fixed_binary(
                batch,
                COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
                row_count,
                2,
            )?,
            exchange_time: compact_l2_column_i64(
                batch,
                COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
                row_count,
            )?,
            symbol: compact_l2_column_fixed_binary(
                batch,
                COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME,
                row_count,
                9,
            )?,
            body_bytes_80_86: compact_l2_column_fixed_binary(
                batch,
                COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
                row_count,
                7,
            )?,
            body_words_88_160: COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS
                .map(|column| compact_l2_column_i64(batch, column.column_name, row_count))
                .into_iter()
                .collect::<Result<Vec<_>>>()?
                .try_into()
                .map_err(|_| {
                    compact_l2_v2_batch_error(
                        "compact-L2 physical-v2 body word lane count mismatch",
                    )
                })?,
        };
        Ok(view)
    }

    /// Append exact legacy fixed-binary v1 payload records into `out`.
    ///
    /// The output grows by `row_count * 168` bytes. Reconstruction validates
    /// the fixed-binary envelope after each row is materialized.
    pub fn append_fixed_binary_v1_payloads(&self, out: &mut Vec<u8>) -> Result<()> {
        self.validate()?;
        let additional = self
            .row_count
            .checked_mul(COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize)
            .ok_or_else(|| {
                compact_l2_v2_batch_error(
                    "compact-L2 physical-v2 reconstructed payload length overflows",
                )
            })?;
        out.reserve(additional);
        let start = out.len();
        out.resize(start + additional, 0);
        for row in 0..self.row_count {
            let base = start + row * COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize;
            self.write_fixed_binary_v1_payload_unchecked(
                row,
                &mut out[base..base + COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize],
            )?;
        }
        Ok(())
    }

    /// Reconstruct one row into an exact legacy fixed-binary v1 payload buffer.
    pub fn write_fixed_binary_v1_payload(&self, row: usize, out: &mut [u8]) -> Result<()> {
        self.validate()?;
        self.write_fixed_binary_v1_payload_unchecked(row, out)
    }

    /// Validate that every borrowed lane has the expected physical length.
    pub fn validate(&self) -> Result<()> {
        compact_l2_validate_lane_len(
            COMPACT_L2_DAY_KEY_COLUMN_NAME,
            self.day_key.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_CHANNEL_ID_COLUMN_NAME,
            self.channel_id.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_BIZ_INDEX_COLUMN_NAME,
            self.biz_index.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
            self.receive_nano.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME,
            self.source_ordinal.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_RECORD_KIND_COLUMN_NAME,
            self.record_kind.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
            self.header_bytes_11_12.len(),
            self.row_count.checked_mul(2).ok_or_else(|| {
                compact_l2_v2_batch_error(
                    "compact-L2 physical-v2 header byte lane length overflows",
                )
            })?,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
            self.exchange_time.len(),
            self.row_count,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME,
            self.symbol.len(),
            self.row_count.checked_mul(9).ok_or_else(|| {
                compact_l2_v2_batch_error("compact-L2 physical-v2 symbol lane length overflows")
            })?,
        )?;
        compact_l2_validate_lane_len(
            COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
            self.body_bytes_80_86.len(),
            self.row_count.checked_mul(7).ok_or_else(|| {
                compact_l2_v2_batch_error("compact-L2 physical-v2 body byte lane length overflows")
            })?,
        )?;
        for (slot, column) in self.body_words_88_160.iter().enumerate() {
            compact_l2_validate_lane_len(
                COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS[slot].column_name,
                column.len(),
                self.row_count,
            )?;
        }
        Ok(())
    }

    fn write_fixed_binary_v1_payload_unchecked(&self, row: usize, out: &mut [u8]) -> Result<()> {
        if row >= self.row_count {
            return Err(compact_l2_v2_batch_error(
                "compact-L2 physical-v2 row index out of bounds",
            ));
        }
        if out.len() != COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "compact-L2 physical-v2 output width mismatch: expected={} observed={}",
                    COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
                    out.len()
                ),
            ));
        }

        out.fill(0);
        out[0..4].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_MAGIC);
        out[4..6].copy_from_slice(&COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1.to_le_bytes());
        out[6..8].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1.to_le_bytes());
        out[8..10].copy_from_slice(&(COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as u16).to_le_bytes());
        out[10] = u8::try_from(self.record_kind[row]).map_err(|_| {
            compact_l2_v2_batch_error("compact-L2 physical-v2 record_kind does not fit u8")
        })?;
        out[11..13].copy_from_slice(&self.header_bytes_11_12[row * 2..row * 2 + 2]);
        out[16..20].copy_from_slice(&u32_from_day_key(self.day_key[row])?.to_le_bytes());
        out[20..24].copy_from_slice(&i32_from_channel_id(self.channel_id[row])?.to_le_bytes());
        out[24..32].copy_from_slice(&self.biz_index[row].to_le_bytes());
        out[32..40]
            .copy_from_slice(&u64_from_source_ordinal(self.source_ordinal[row])?.to_le_bytes());
        out[40..48].copy_from_slice(&self.receive_nano[row].to_le_bytes());
        out[48..56].copy_from_slice(&self.exchange_time[row].to_le_bytes());
        out[56..65].copy_from_slice(&self.symbol[row * 9..row * 9 + 9]);
        out[80..87].copy_from_slice(&self.body_bytes_80_86[row * 7..row * 7 + 7]);
        for (slot, column) in self.body_words_88_160.iter().enumerate() {
            let offset = usize::from(COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS[slot]);
            out[offset..offset + 8].copy_from_slice(&column[row].to_le_bytes());
        }
        decode_compact_l2_fixed_binary_header_v1(out)?;
        Ok(())
    }
}

/// Return the stable physical-v2 body-word column name for a v1 byte offset.
pub const fn compact_l2_physical_v2_body_word_column_name(offset: u16) -> Option<&'static str> {
    match offset {
        88 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_88_COLUMN_NAME),
        96 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_96_COLUMN_NAME),
        104 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_104_COLUMN_NAME),
        112 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_112_COLUMN_NAME),
        120 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_120_COLUMN_NAME),
        128 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_128_COLUMN_NAME),
        136 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_136_COLUMN_NAME),
        144 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_144_COLUMN_NAME),
        152 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_152_COLUMN_NAME),
        160 => Some(COMPACT_L2_PHYSICAL_V2_BODY_WORD_160_COLUMN_NAME),
        _ => None,
    }
}

/// Decode and validate one V1 compact fixed-binary L2 record header.
///
/// The input must be exactly [`COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1`] bytes.
/// Validation is limited to binary envelope/header facts and variant padding; it
/// deliberately does not interpret order/trade business payload fields.
pub fn decode_compact_l2_fixed_binary_header_v1(
    record: &[u8],
) -> Result<CompactL2FixedBinaryHeaderV1> {
    if record.len() != COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as usize {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::FixedBinaryWidthMismatch,
            format!(
                "compact-L2 fixed-binary record width mismatch: expected={} observed={}",
                COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
                record.len()
            ),
        ));
    }

    let mut magic = [0u8; 4];
    magic.copy_from_slice(&record[0..4]);
    if magic != COMPACT_L2_FIXED_INGRESS_MAGIC {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress magic mismatch",
        ));
    }
    let schema_version = u16::from_le_bytes(record[4..6].try_into().expect("schema width"));
    if schema_version != COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1 {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsupportedSchemaVersion,
            format!("unsupported compact-L2 fixed-ingress schema version: {schema_version}"),
        ));
    }
    let header_len = u16::from_le_bytes(record[6..8].try_into().expect("header width"));
    if header_len != COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress header length mismatch",
        ));
    }
    let record_len = u16::from_le_bytes(record[8..10].try_into().expect("record width"));
    if record_len != COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as u16 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress record length mismatch",
        ));
    }
    let record_kind = match record[10] {
        COMPACT_L2_RECORD_KIND_ORDER => CompactL2RecordKind::Order,
        COMPACT_L2_RECORD_KIND_TRADE => CompactL2RecordKind::Trade,
        _ => {
            return Err(payload_header_error(
                "compact-L2 fixed-ingress record kind mismatch",
            ));
        }
    };
    let source_family = record[11];
    if !matches!(
        source_family,
        COMPACT_L2_SOURCE_FAMILY_LF_KUNGFU
            | COMPACT_L2_SOURCE_FAMILY_ARCADIA_NATIVE
            | COMPACT_L2_SOURCE_FAMILY_SYNTHETIC
    ) {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress source family mismatch",
        ));
    }
    let exchange_id = record[12];
    if exchange_id > 2 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress exchange id mismatch",
        ));
    }
    if record[13] != 0 || u16::from_le_bytes(record[14..16].try_into().expect("flags")) != 0 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress reserved header field is nonzero",
        ));
    }
    if record[65..80].iter().any(|byte| *byte != 0) {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress reserved header tail is nonzero",
        ));
    }

    let trading_day = u32::from_le_bytes(record[16..20].try_into().expect("day width"));
    let channel_id = i32::from_le_bytes(record[20..24].try_into().expect("channel width"));
    let biz_index = i64::from_le_bytes(record[24..32].try_into().expect("biz width"));
    let source_ordinal = u64::from_le_bytes(record[32..40].try_into().expect("ordinal width"));
    let receive_nano = i64::from_le_bytes(record[40..48].try_into().expect("receive width"));
    let exchange_time = i64::from_le_bytes(record[48..56].try_into().expect("exchange width"));

    if trading_day == 0 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress trading day is zero",
        ));
    }
    if channel_id <= 0 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress ChannelID is invalid",
        ));
    }
    if biz_index <= 0 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress BizIndex is invalid",
        ));
    }

    // Compact variant padding checks. These are source-format envelope facts,
    // not order-book or trade business interpretation.
    if record[87] != 0 {
        return Err(payload_header_error(
            "compact-L2 fixed-ingress reserved compact field is nonzero",
        ));
    }
    match record_kind {
        CompactL2RecordKind::Order => {
            if record[82] != 0
                || i64::from_le_bytes(record[104..112].try_into().expect("order pad width")) != 0
                || i64::from_le_bytes(record[112..120].try_into().expect("order pad width")) != 0
                || i64::from_le_bytes(record[152..160].try_into().expect("order pad width")) != 0
            {
                return Err(payload_header_error(
                    "compact-L2 fixed-ingress order variant padding is nonzero",
                ));
            }
        }
        CompactL2RecordKind::Trade => {
            if record[83] != 0 || record[86] != 0 {
                return Err(payload_header_error(
                    "compact-L2 fixed-ingress trade variant padding is nonzero",
                ));
            }
        }
    }

    Ok(CompactL2FixedBinaryHeaderV1 {
        magic,
        schema_version,
        header_len,
        record_len,
        record_kind,
        source_family,
        exchange_id,
        trading_day,
        channel_id,
        biz_index,
        source_ordinal,
        receive_nano,
        exchange_time,
    })
}

fn payload_header_error(message: &'static str) -> ArcadiaTioError {
    ArcadiaTioError::ocb_diagnostic(OcbErrorKind::PayloadHeaderMismatch, message)
}

fn scalar_payload_mismatch(message: &'static str) -> ArcadiaTioError {
    ArcadiaTioError::ocb_diagnostic(OcbErrorKind::PayloadHeaderMismatch, message)
}

fn compact_l2_v2_batch_error(message: impl Into<String>) -> ArcadiaTioError {
    ArcadiaTioError::ocb_diagnostic(OcbErrorKind::PayloadHeaderMismatch, message)
}

fn compact_l2_validate_lane_len(
    name: &str,
    observed_len: usize,
    expected_len: usize,
) -> Result<()> {
    if observed_len == expected_len {
        Ok(())
    } else {
        Err(compact_l2_v2_batch_error(format!(
            "compact-L2 physical-v2 column {name:?} length mismatch: expected={expected_len} observed={observed_len}"
        )))
    }
}

fn compact_l2_column_i32<'a>(
    batch: &'a ColumnBatch,
    name: &str,
    row_count: usize,
) -> Result<&'a [i32]> {
    let column = batch
        .columns
        .iter()
        .find(|column| column.name == name)
        .ok_or_else(|| compact_l2_v2_batch_error(format!("compact-L2 column {name:?} missing")))?;
    match &column.values {
        PrimitiveColumnValues::I32(values) if values.len() == row_count => Ok(values),
        PrimitiveColumnValues::I32(values) => Err(compact_l2_v2_batch_error(format!(
            "compact-L2 column {name:?} row count mismatch: expected={row_count} observed={}",
            values.len()
        ))),
        _ => Err(compact_l2_v2_batch_error(format!(
            "compact-L2 column {name:?} has unexpected physical type"
        ))),
    }
}

fn compact_l2_column_i64<'a>(
    batch: &'a ColumnBatch,
    name: &str,
    row_count: usize,
) -> Result<&'a [i64]> {
    let column = batch
        .columns
        .iter()
        .find(|column| column.name == name)
        .ok_or_else(|| compact_l2_v2_batch_error(format!("compact-L2 column {name:?} missing")))?;
    match &column.values {
        PrimitiveColumnValues::I64(values) if values.len() == row_count => Ok(values),
        PrimitiveColumnValues::I64(values) => Err(compact_l2_v2_batch_error(format!(
            "compact-L2 column {name:?} row count mismatch: expected={row_count} observed={}",
            values.len()
        ))),
        _ => Err(compact_l2_v2_batch_error(format!(
            "compact-L2 column {name:?} has unexpected physical type"
        ))),
    }
}

fn compact_l2_column_fixed_binary<'a>(
    batch: &'a ColumnBatch,
    name: &str,
    row_count: usize,
    expected_width: u32,
) -> Result<&'a [u8]> {
    let column = batch
        .columns
        .iter()
        .find(|column| column.name == name)
        .ok_or_else(|| compact_l2_v2_batch_error(format!("compact-L2 column {name:?} missing")))?;
    match &column.values {
        PrimitiveColumnValues::FixedBinary { width, bytes } if *width == expected_width => {
            let expected_len = row_count
                .checked_mul(expected_width as usize)
                .ok_or_else(|| {
                    compact_l2_v2_batch_error(
                        "compact-L2 fixed-binary column expected length overflows",
                    )
                })?;
            if bytes.len() == expected_len {
                Ok(bytes)
            } else {
                Err(compact_l2_v2_batch_error(format!(
                    "compact-L2 column {name:?} byte length mismatch: expected={expected_len} observed={}",
                    bytes.len()
                )))
            }
        }
        PrimitiveColumnValues::FixedBinary { width, .. } => Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::FixedBinaryWidthMismatch,
            format!(
                "compact-L2 column {name:?} fixed-binary width mismatch: expected={expected_width} observed={width}"
            ),
        )),
        _ => Err(compact_l2_v2_batch_error(format!(
            "compact-L2 column {name:?} has unexpected physical type"
        ))),
    }
}

fn u32_from_day_key(value: i32) -> Result<u32> {
    u32::try_from(value).map_err(|_| compact_l2_v2_batch_error("compact-L2 day_key is negative"))
}

fn i32_from_channel_id(value: i64) -> Result<i32> {
    i32::try_from(value)
        .map_err(|_| compact_l2_v2_batch_error("compact-L2 channel_id does not fit i32"))
}

fn u64_from_source_ordinal(value: i64) -> Result<u64> {
    u64::try_from(value)
        .map_err(|_| compact_l2_v2_batch_error("compact-L2 source_ordinal is negative"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::column_bundle::{ColumnArray, ColumnLogicalKind, ColumnPhysicalType};
    use crate::{ArcadiaTioErrorCode, OcbFailureCause};

    #[test]
    fn physical_v2_body_word_column_names_are_stable() {
        assert_eq!(
            COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS
                .iter()
                .map(|column| (column.offset, column.column_name))
                .collect::<Vec<_>>(),
            vec![
                (88, "payload_body_word_88"),
                (96, "payload_body_word_96"),
                (104, "payload_body_word_104"),
                (112, "payload_body_word_112"),
                (120, "payload_body_word_120"),
                (128, "payload_body_word_128"),
                (136, "payload_body_word_136"),
                (144, "payload_body_word_144"),
                (152, "payload_body_word_152"),
                (160, "payload_body_word_160"),
            ]
        );
        assert_eq!(
            compact_l2_physical_v2_body_word_column_name(120),
            Some("payload_body_word_120")
        );
        assert_eq!(compact_l2_physical_v2_body_word_column_name(121), None);
    }

    #[test]
    fn physical_v2_record_roundtrips_order_and_trade_fixed_binary_payloads() {
        for kind in [CompactL2RecordKind::Order, CompactL2RecordKind::Trade] {
            let record = compact_l2_record(kind);
            let decoded = CompactL2PhysicalV2Record::from_fixed_binary_v1(&record).unwrap();
            assert_eq!(decoded.record_kind, kind);
            assert_eq!(decoded.trading_day, 20240102);
            assert_eq!(decoded.channel_id, 2011);
            assert_eq!(decoded.biz_index, 42);
            assert_eq!(decoded.source_ordinal, 77);
            assert_eq!(decoded.receive_nano, 123_456_789);
            assert_eq!(decoded.exchange_time, 987_654_321);
            assert_eq!(&decoded.symbol, b"IF2512\0\0\0");

            let reconstructed = decoded.to_fixed_binary_v1().unwrap();
            assert_eq!(reconstructed.as_slice(), record.as_slice());
        }
    }

    #[test]
    fn physical_v2_record_roundtrips_synthetic_header_matrix() {
        let mut cases = 0usize;
        for source_family in [
            COMPACT_L2_SOURCE_FAMILY_LF_KUNGFU,
            COMPACT_L2_SOURCE_FAMILY_ARCADIA_NATIVE,
            COMPACT_L2_SOURCE_FAMILY_SYNTHETIC,
        ] {
            for exchange_id in 0..=2u8 {
                for kind in [CompactL2RecordKind::Order, CompactL2RecordKind::Trade] {
                    let seed = cases as i64 + 1;
                    let record = compact_l2_record_variant(kind, source_family, exchange_id, seed);
                    let decoded = CompactL2PhysicalV2Record::from_fixed_binary_v1(&record)
                        .expect("variant record decodes");
                    assert_eq!(decoded.record_kind, kind);
                    assert_eq!(decoded.source_family, source_family);
                    assert_eq!(decoded.exchange_id, exchange_id);
                    assert_eq!(decoded.to_fixed_binary_v1().unwrap().as_slice(), record);
                    cases += 1;
                }
            }
        }
        assert_eq!(cases, 18);
    }

    #[test]
    fn physical_v2_record_validates_duplicated_scalar_columns() {
        let record = compact_l2_record(CompactL2RecordKind::Trade);
        let decoded = CompactL2PhysicalV2Record::from_fixed_binary_v1_with_scalar_columns(
            &record,
            20240102,
            2011,
            42,
            123_456_789,
            77,
            2,
        )
        .unwrap();
        assert_eq!(decoded.record_kind, CompactL2RecordKind::Trade);

        let err = CompactL2PhysicalV2Record::from_fixed_binary_v1_with_scalar_columns(
            &record,
            20240102,
            2011,
            43,
            123_456_789,
            77,
            2,
        )
        .expect_err("mismatched BizIndex should fail closed");
        assert_eq!(err.code(), ArcadiaTioErrorCode::InvalidArgument);
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
    }

    #[test]
    fn physical_v2_batch_view_reconstructs_fixed_binary_payloads() {
        let records = [
            compact_l2_record_variant(
                CompactL2RecordKind::Order,
                COMPACT_L2_SOURCE_FAMILY_LF_KUNGFU,
                1,
                0,
            ),
            compact_l2_record_variant(
                CompactL2RecordKind::Trade,
                COMPACT_L2_SOURCE_FAMILY_ARCADIA_NATIVE,
                2,
                1,
            ),
        ];
        let batch = compact_l2_physical_v2_batch(&records);
        let view = CompactL2PhysicalV2BatchView::from_column_batch(&batch).unwrap();

        let mut payloads = Vec::new();
        view.append_fixed_binary_v1_payloads(&mut payloads).unwrap();
        assert_eq!(payloads.len(), records.len() * 168);
        assert_eq!(&payloads[0..168], records[0].as_slice());
        assert_eq!(&payloads[168..336], records[1].as_slice());

        let mut second = [0u8; 168];
        view.write_fixed_binary_v1_payload(1, &mut second).unwrap();
        assert_eq!(second, records[1]);

        let mut bad_view = view;
        bad_view.symbol = &view.symbol[..view.symbol.len() - 1];
        let mut ignored = Vec::new();
        let err = bad_view
            .append_fixed_binary_v1_payloads(&mut ignored)
            .expect_err("short symbol lane should fail without panicking");
        assert_eq!(err.code(), ArcadiaTioErrorCode::InvalidArgument);
    }

    #[test]
    fn physical_v2_batch_view_rejects_wrong_fixed_binary_lane_width() {
        let records = [compact_l2_record(CompactL2RecordKind::Trade)];
        let mut batch = compact_l2_physical_v2_batch(&records);
        let symbol = batch
            .columns
            .iter_mut()
            .find(|column| column.name == COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME)
            .expect("symbol column");
        symbol.physical_type = ColumnPhysicalType::FixedBinary { width: 8 };
        symbol.values = PrimitiveColumnValues::FixedBinary {
            width: 8,
            bytes: vec![0; 8],
        };

        let err = CompactL2PhysicalV2BatchView::from_column_batch(&batch)
            .expect_err("wrong symbol lane width must fail closed");
        assert_eq!(err.code(), ArcadiaTioErrorCode::InvalidArgument);
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
    }

    #[test]
    fn physical_v2_reconstruction_rejects_variant_padding_mismatch() {
        let record = compact_l2_record(CompactL2RecordKind::Order);
        let mut decoded = CompactL2PhysicalV2Record::from_fixed_binary_v1(&record).unwrap();
        let offset_104_slot = COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS
            .iter()
            .position(|offset| *offset == 104)
            .expect("offset 104 lane");
        decoded.body_words_88_160[offset_104_slot] = 1;
        let err = decoded
            .to_fixed_binary_v1()
            .expect_err("order padding word must remain zero");
        assert_eq!(err.code(), ArcadiaTioErrorCode::InvalidArgument);
    }

    #[test]
    fn physical_v2_record_rejects_malformed_v1_payloads() {
        let record = compact_l2_record(CompactL2RecordKind::Trade);
        assert_invalid_record(&record[..167]);

        let mut bad_magic = record;
        bad_magic[0] = b'X';
        assert_invalid_record(&bad_magic);

        let mut bad_source_family = record;
        bad_source_family[11] = 99;
        assert_invalid_record(&bad_source_family);

        let mut bad_exchange = record;
        bad_exchange[12] = 3;
        assert_invalid_record(&bad_exchange);

        let mut bad_reserved_header = record;
        bad_reserved_header[65] = 1;
        assert_invalid_record(&bad_reserved_header);

        let mut bad_trade_padding = record;
        bad_trade_padding[83] = 1;
        assert_invalid_record(&bad_trade_padding);
    }

    fn compact_l2_record(kind: CompactL2RecordKind) -> [u8; 168] {
        compact_l2_record_variant(kind, COMPACT_L2_SOURCE_FAMILY_LF_KUNGFU, 2, 0)
    }

    fn compact_l2_record_variant(
        kind: CompactL2RecordKind,
        source_family: u8,
        exchange_id: u8,
        seed: i64,
    ) -> [u8; 168] {
        let mut record = [0u8; 168];
        let seed_u32 = u32::try_from(seed).expect("synthetic seed fits u32");
        let seed_u64 = u64::try_from(seed).expect("synthetic seed fits u64");
        record[0..4].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_MAGIC);
        record[4..6].copy_from_slice(&COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1.to_le_bytes());
        record[6..8].copy_from_slice(&COMPACT_L2_FIXED_INGRESS_HEADER_LEN_V1.to_le_bytes());
        record[8..10]
            .copy_from_slice(&(COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1 as u16).to_le_bytes());
        record[10] = kind.code();
        record[11] = source_family;
        record[12] = exchange_id;
        record[16..20].copy_from_slice(&(20240102u32 + seed_u32).to_le_bytes());
        record[20..24].copy_from_slice(&(2011i32 + seed as i32).to_le_bytes());
        record[24..32].copy_from_slice(&(42i64 + seed).to_le_bytes());
        record[32..40].copy_from_slice(&(77u64 + seed_u64).to_le_bytes());
        record[40..48].copy_from_slice(&(123_456_789i64 + seed).to_le_bytes());
        record[48..56].copy_from_slice(&(987_654_321i64 + seed).to_le_bytes());
        record[56..65].copy_from_slice(b"IF2512\0\0\0");
        record[80] = (10 + seed) as u8;
        record[81] = (11 + seed) as u8;
        record[82] = (12 + seed) as u8;
        record[83] = (13 + seed) as u8;
        record[84] = (14 + seed) as u8;
        record[85] = (15 + seed) as u8;
        record[86] = (16 + seed) as u8;
        for offset in COMPACT_L2_PHYSICAL_V2_BODY_WORD_OFFSETS {
            let offset = usize::from(offset);
            record[offset..offset + 8]
                .copy_from_slice(&((offset as i64 * 10) + seed).to_le_bytes());
        }
        match kind {
            CompactL2RecordKind::Order => {
                record[82] = 0;
                record[104..112].fill(0);
                record[112..120].fill(0);
                record[152..160].fill(0);
            }
            CompactL2RecordKind::Trade => {
                record[83] = 0;
                record[86] = 0;
            }
        }
        decode_compact_l2_fixed_binary_header_v1(&record).unwrap();
        record
    }

    fn assert_invalid_record(record: &[u8]) {
        let err = CompactL2PhysicalV2Record::from_fixed_binary_v1(record)
            .expect_err("malformed compact-L2 payload should fail closed");
        assert_eq!(err.code(), ArcadiaTioErrorCode::InvalidArgument);
        assert_eq!(err.ocb_failure_cause(), Some(OcbFailureCause::InvalidInput));
    }

    fn compact_l2_physical_v2_batch(records: &[[u8; 168]]) -> ColumnBatch {
        let decoded = records
            .iter()
            .map(|record| CompactL2PhysicalV2Record::from_fixed_binary_v1(record).unwrap())
            .collect::<Vec<_>>();
        let mut header_bytes = Vec::with_capacity(decoded.len() * 2);
        let mut symbols = Vec::with_capacity(decoded.len() * 9);
        let mut body_bytes = Vec::with_capacity(decoded.len() * 7);
        let mut body_words: [Vec<i64>; 10] = std::array::from_fn(|_| Vec::new());

        for record in &decoded {
            header_bytes.push(record.source_family);
            header_bytes.push(record.exchange_id);
            symbols.extend_from_slice(&record.symbol);
            body_bytes.extend_from_slice(&record.body_bytes_80_86);
            for (slot, word) in record.body_words_88_160.iter().copied().enumerate() {
                body_words[slot].push(word);
            }
        }

        let mut columns = vec![
            i32_column(
                COMPACT_L2_DAY_KEY_COLUMN_NAME,
                decoded
                    .iter()
                    .map(|record| i32::try_from(record.trading_day).unwrap())
                    .collect(),
            ),
            i64_column(
                COMPACT_L2_CHANNEL_ID_COLUMN_NAME,
                decoded
                    .iter()
                    .map(|record| i64::from(record.channel_id))
                    .collect(),
            ),
            i64_column(
                COMPACT_L2_BIZ_INDEX_COLUMN_NAME,
                decoded.iter().map(|record| record.biz_index).collect(),
            ),
            i64_column(
                COMPACT_L2_RECEIVE_NANO_COLUMN_NAME,
                decoded.iter().map(|record| record.receive_nano).collect(),
            ),
            i64_column(
                COMPACT_L2_SOURCE_ORDINAL_COLUMN_NAME,
                decoded
                    .iter()
                    .map(|record| i64::try_from(record.source_ordinal).unwrap())
                    .collect(),
            ),
            i32_column(
                COMPACT_L2_RECORD_KIND_COLUMN_NAME,
                decoded
                    .iter()
                    .map(|record| i32::from(record.record_kind.code()))
                    .collect(),
            ),
            fixed_binary_column(
                COMPACT_L2_PHYSICAL_V2_HEADER_BYTES_11_12_COLUMN_NAME,
                2,
                header_bytes,
            ),
            i64_column(
                COMPACT_L2_PHYSICAL_V2_EXCHANGE_TIME_COLUMN_NAME,
                decoded.iter().map(|record| record.exchange_time).collect(),
            ),
            fixed_binary_column(COMPACT_L2_PHYSICAL_V2_SYMBOL_COLUMN_NAME, 9, symbols),
            fixed_binary_column(
                COMPACT_L2_PHYSICAL_V2_BODY_BYTES_80_86_COLUMN_NAME,
                7,
                body_bytes,
            ),
        ];
        for (slot, lane) in body_words.into_iter().enumerate() {
            columns.push(i64_column(
                COMPACT_L2_PHYSICAL_V2_BODY_WORD_COLUMNS[slot].column_name,
                lane,
            ));
        }

        ColumnBatch {
            row_group_id: 0,
            base_row: 0,
            row_count: records.len() as u64,
            columns,
        }
    }

    fn i32_column(name: &str, values: Vec<i32>) -> ColumnArray {
        column(
            name,
            ColumnPhysicalType::I32,
            PrimitiveColumnValues::I32(values),
        )
    }

    fn i64_column(name: &str, values: Vec<i64>) -> ColumnArray {
        column(
            name,
            ColumnPhysicalType::I64,
            PrimitiveColumnValues::I64(values),
        )
    }

    fn fixed_binary_column(name: &str, width: u32, bytes: Vec<u8>) -> ColumnArray {
        column(
            name,
            ColumnPhysicalType::FixedBinary { width },
            PrimitiveColumnValues::FixedBinary { width, bytes },
        )
    }

    fn column(
        name: &str,
        physical_type: ColumnPhysicalType,
        values: PrimitiveColumnValues,
    ) -> ColumnArray {
        ColumnArray {
            column_id: 0,
            name: name.to_owned(),
            physical_type,
            logical_kind: ColumnLogicalKind::Plain,
            dictionary_id: None,
            values,
            validity: None,
        }
    }
}
