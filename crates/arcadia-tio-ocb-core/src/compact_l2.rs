//! Compact fixed-binary L2 envelope facts used by OCB certification helpers.
//!
//! This module intentionally exposes only source-format header facts and
//! fail-closed validation primitives. It does not implement order-book replay,
//! factor/KOB logic, owner assignment, or runtime scheduling.

use crate::{ArcadiaTioError, OcbErrorKind, Result};

/// Stable public Rust-core OCB reader API version.
pub const OCB_CORE_READER_API_VERSION: u16 = 1;
/// Channel-sharded manifest schema version supported by this crate.
pub const CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1: u16 = 1;
/// Compact fixed-binary L2 payload schema version supported by this crate.
pub const COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1: u16 = 1;

/// Compact fixed-binary L2 payload format label.
pub const COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1: &str = "compact-fixed-binary-l2-v1";
/// Canonical payload column name used by compact-L2 OCB artifacts.
pub const COMPACT_L2_PAYLOAD_COLUMN_NAME: &str = "payload";
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
