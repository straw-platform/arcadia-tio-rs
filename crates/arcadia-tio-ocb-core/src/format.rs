#![allow(dead_code)]

//! OCB binary format primitives.
//!
//! OCB is the one-file Ordered Column Bundle format family. It is deliberately
//! versioned separately from the V4 tensor runtime.

use std::io::{Cursor, ErrorKind, Read, Write};

use super::resource_limits::{
    MetadataMaterializationBudget, OCB_POLICY_A_MAX_COMPRESSED_CHUNK_BYTES,
    OCB_POLICY_A_MAX_DECOMPRESSED_CHUNK_BYTES, OcbResourceLimits,
};
#[cfg(test)]
use super::resource_limits::{
    OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES, OCB_POLICY_A_MAX_OWNED_DECODED_MATERIALIZED_BYTES,
};
use crate::{ArcadiaTioError, Result};

pub(crate) const OCB_FORMAT_MAJOR_V1: u16 = 1;
pub(crate) const OCB_FORMAT_MINOR_V1: u16 = 0;
pub(crate) const OCB_FORMAT_MAJOR_V2: u16 = 2;
pub(crate) const OCB_FORMAT_MINOR_V2: u16 = 0;
pub(crate) const OCB_BOOTSTRAP_PAGE_V1_LEN: usize = 4096;
pub(crate) const OCB_BOOTSTRAP_PAGE_V2_LEN: usize = OCB_BOOTSTRAP_PAGE_V1_LEN;
pub(crate) const OCB_ROOT_SLOT_V2_COUNT: usize = 2;
pub(crate) const OCB_ROOT_SLOT_V2_LEN: usize = 160;
pub(crate) const OCB_ROOT_SLOT_TABLE_V2_OFFSET: usize = 64;
pub(crate) const OCB_ROOT_SLOT_TABLE_V2_LEN: usize = OCB_ROOT_SLOT_V2_COUNT * OCB_ROOT_SLOT_V2_LEN;
pub(crate) const OCB_BODY_REF_V2_LEN: usize = 32;
pub(crate) const OCB_COLUMN_DESC_V1_LEN: usize = 32;
pub(crate) const OCB_ROW_GROUP_DESC_V1_LEN: usize = 120;
pub(crate) const OCB_COLUMN_CHUNK_DESC_V1_LEN: usize = 96;
pub(crate) const OCB_STAT_SCALAR_V1_LEN: usize = 16;
pub(crate) const OCB_COLUMN_STATS_V1_LEN: usize = 48;
pub(crate) const OCB_ORDERING_KEY_V1_LEN: usize = 8;
pub(crate) const OCB_ROW_GROUP_ORDERING_PROOF_V1_LEN: usize = 72;
// The writer accepts zstd levels 1..=22; level 22 can declare a 2^27-byte
// window even for a tiny payload. Keep that bounded compatibility floor so a
// small custom output limit does not make writer-produced bytes unreadable.
// Output allocation remains exact, while an accepted frame may require up to
// 128 MiB of decoder history; these limits are not a peak-RSS guarantee.
const OCB_ZSTD_WRITER_COMPAT_WINDOW_LOG: u32 = 27;
const OCB_ZSTD_PLATFORM_MAX_WINDOW_LOG: u32 = if usize::BITS <= 32 { 30 } else { 31 };

pub(crate) const OCB_BOOTSTRAP_MAGIC_V1: [u8; 8] = *b"TIOOCB1\0";
pub(crate) const OCB_BOOTSTRAP_MAGIC_V2: [u8; 8] = *b"TIOOCB2\0";
pub(crate) const OCB_ROOT_MAGIC_V1: [u8; 8] = *b"OCBROOT1";
pub(crate) const OCB_ROOT_MAGIC_V2: [u8; 8] = *b"OCBROOT2";
pub(crate) const OCB_ROOT_SLOT_MAGIC_V2: [u8; 8] = *b"OCBSLT2\0";
pub(crate) const OCB_SCHEMA_MAGIC_V1: [u8; 8] = *b"OCBSCH1\0";
pub(crate) const OCB_STRING_TABLE_MAGIC_V1: [u8; 8] = *b"OCBSTR1\0";
pub(crate) const OCB_DICTIONARY_INDEX_MAGIC_V1: [u8; 8] = *b"OCBDIX1\0";
pub(crate) const OCB_DICTIONARY_VALUES_MAGIC_V1: [u8; 8] = *b"OCBDVL1\0";
pub(crate) const OCB_ROW_GROUP_INDEX_MAGIC_V1: [u8; 8] = *b"OCBRGI1\0";
pub(crate) const OCB_ORDERING_PROOF_MAGIC_V1: [u8; 8] = *b"OCBORD1\0";
pub(crate) const OCB_COLUMN_CHUNK_MAGIC_V1: [u8; 8] = *b"OCBCHK1\0";
pub(crate) const OCB_ROW_GROUP_INDEX_DELTA_MAGIC_V1: [u8; 8] = *b"OCBRGD1\0";

pub(crate) const OCB_NULL_U32: u32 = u32::MAX;

pub(crate) const OCB_ROOT_V1_LEN: u16 = 220;
pub(crate) const OCB_ROOT_V2_LEN: u16 = 456;
pub(crate) const OCB_SCHEMA_V1_HEADER_LEN: u16 = 52;
pub(crate) const OCB_STRING_TABLE_V1_HEADER_LEN: u16 = 20;
pub(crate) const OCB_DICTIONARY_INDEX_V1_HEADER_LEN: u16 = 20;
pub(crate) const OCB_DICTIONARY_VALUES_V1_HEADER_LEN: u16 = 40;
pub(crate) const OCB_ROW_GROUP_INDEX_V1_HEADER_LEN: u16 = 32;
pub(crate) const OCB_ORDERING_PROOF_V1_HEADER_LEN: u16 = 28;
pub(crate) const OCB_COLUMN_CHUNK_V1_HEADER_LEN: u16 = 44;
pub(crate) const OCB_ROW_GROUP_INDEX_DELTA_V1_HEADER_LEN: u16 = 56;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbBodyKindV1 {
    Unknown = 0,
    Root = 1,
    Schema = 2,
    DictionaryIndex = 3,
    DictionaryValues = 4,
    RowGroupIndex = 5,
    OrderingProof = 6,
    ColumnChunk = 7,
    StringTable = 8,
    DebugJsonMetadata = 9,
    ValidityBitmap = 10,
    KeyTuple = 11,
    RowGroupIndexDelta = 12,
}

impl OcbBodyKindV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Root),
            2 => Ok(Self::Schema),
            3 => Ok(Self::DictionaryIndex),
            4 => Ok(Self::DictionaryValues),
            5 => Ok(Self::RowGroupIndex),
            6 => Ok(Self::OrderingProof),
            7 => Ok(Self::ColumnChunk),
            8 => Ok(Self::StringTable),
            9 => Ok(Self::DebugJsonMetadata),
            10 => Ok(Self::ValidityBitmap),
            11 => Ok(Self::KeyTuple),
            12 => Ok(Self::RowGroupIndexDelta),
            _ => Err(ArcadiaTioError::ocb_corrupt_file("unknown OCB body kind")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbChecksumKindV1 {
    None = 0,
    Crc32c = 1,
}

impl OcbChecksumKindV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Crc32c),
            _ => Err(ArcadiaTioError::ocb_corrupt_file(
                "unknown OCB checksum kind",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbPhysicalTypeV1 {
    I32 = 1,
    I64 = 2,
    F32 = 3,
    F64 = 4,
    FixedBinary = 5,
}

impl OcbPhysicalTypeV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::I32),
            2 => Ok(Self::I64),
            3 => Ok(Self::F32),
            4 => Ok(Self::F64),
            5 => Ok(Self::FixedBinary),
            _ => Err(ArcadiaTioError::ocb_corrupt_file(
                "unknown OCB physical type",
            )),
        }
    }

    pub(crate) const fn primitive_byte_width(self) -> Option<usize> {
        match self {
            Self::I32 | Self::F32 => Some(4),
            Self::I64 | Self::F64 => Some(8),
            Self::FixedBinary => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbLogicalKindV1 {
    Plain = 0,
    TimestampNanosLike = 1,
    ScaledInteger = 2,
    DictionaryCode = 3,
    EnumCode = 4,
    OpaqueKey = 5,
}

impl OcbLogicalKindV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::Plain),
            1 => Ok(Self::TimestampNanosLike),
            2 => Ok(Self::ScaledInteger),
            3 => Ok(Self::DictionaryCode),
            4 => Ok(Self::EnumCode),
            5 => Ok(Self::OpaqueKey),
            _ => Err(ArcadiaTioError::ocb_corrupt_file(
                "unknown OCB logical kind",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbNullabilityV1 {
    NonNull = 0,
    Nullable = 1,
}

impl OcbNullabilityV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::NonNull),
            1 => Ok(Self::Nullable),
            _ => Err(ArcadiaTioError::ocb_corrupt_file("unknown OCB nullability")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbChunkCodecV1 {
    None = 0,
    Zstd = 1,
}

impl OcbChunkCodecV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::Zstd),
            _ => Err(ArcadiaTioError::ocb_unsupported_format(
                "unknown OCB chunk codec",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum OcbDictionaryValueKindV1 {
    Utf8 = 1,
    Bytes = 2,
    FixedBytes = 3,
    EnumLabels = 4,
}

impl OcbDictionaryValueKindV1 {
    fn from_u16(value: u16) -> Result<Self> {
        match value {
            1 => Ok(Self::Utf8),
            2 => Ok(Self::Bytes),
            3 => Ok(Self::FixedBytes),
            4 => Ok(Self::EnumLabels),
            _ => Err(ArcadiaTioError::ocb_corrupt_file(
                "unknown OCB dictionary value kind",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum OcbOrderingDirectionV1 {
    Ascending = 0,
    Descending = 1,
}

impl OcbOrderingDirectionV1 {
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Ascending),
            1 => Ok(Self::Descending),
            _ => Err(ArcadiaTioError::ocb_corrupt_file(
                "unknown OCB ordering direction",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum OcbNullOrderV1 {
    NullsFirst = 0,
    NullsLast = 1,
    NoNulls = 2,
}

impl OcbNullOrderV1 {
    fn from_u8(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::NullsFirst),
            1 => Ok(Self::NullsLast),
            2 => Ok(Self::NoNulls),
            _ => Err(ArcadiaTioError::ocb_corrupt_file("unknown OCB null order")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbBodyRefV2 {
    pub(crate) offset: u64,
    pub(crate) length: u64,
    pub(crate) kind: OcbBodyKindV1,
    pub(crate) flags: u16,
    pub(crate) checksum_kind: OcbChecksumKindV1,
    pub(crate) reserved0: u16,
    pub(crate) checksum: u32,
    pub(crate) reserved1: u32,
}

impl OcbBodyRefV2 {
    pub(crate) const NULL: Self = Self {
        offset: 0,
        length: 0,
        kind: OcbBodyKindV1::Unknown,
        flags: 0,
        checksum_kind: OcbChecksumKindV1::None,
        reserved0: 0,
        checksum: 0,
        reserved1: 0,
    };

    pub(crate) fn new(offset: u64, length: u64, kind: OcbBodyKindV1, checksum: u32) -> Self {
        Self {
            offset,
            length,
            kind,
            flags: 0,
            checksum_kind: OcbChecksumKindV1::Crc32c,
            reserved0: 0,
            checksum,
            reserved1: 0,
        }
    }

    pub(crate) const fn is_null(self) -> bool {
        self.offset == 0 && self.length == 0 && matches!(self.kind, OcbBodyKindV1::Unknown)
    }

    pub(crate) fn validate(self, expected_kind: OcbBodyKindV1, file_len: u64) -> Result<()> {
        if self.is_null() {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB body reference is null",
            ));
        }
        if self.kind != expected_kind {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB body reference kind mismatch",
            ));
        }
        let end = self
            .offset
            .checked_add(self.length)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB body reference range overflows",
            ))?;
        if end > file_len {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB body reference extends beyond file length",
            ));
        }
        Ok(())
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u64(&mut writer, self.offset)?;
        write_u64(&mut writer, self.length)?;
        write_u16(&mut writer, self.kind as u16)?;
        write_u16(&mut writer, self.flags)?;
        write_u16(&mut writer, self.checksum_kind as u16)?;
        write_u16(&mut writer, self.reserved0)?;
        write_u32(&mut writer, self.checksum)?;
        write_u32(&mut writer, self.reserved1)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            offset: read_u64(&mut reader)?,
            length: read_u64(&mut reader)?,
            kind: OcbBodyKindV1::from_u16(read_u16(&mut reader)?)?,
            flags: read_u16(&mut reader)?,
            checksum_kind: OcbChecksumKindV1::from_u16(read_u16(&mut reader)?)?,
            reserved0: read_u16(&mut reader)?,
            checksum: read_u32(&mut reader)?,
            reserved1: read_u32(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbBootstrapPageV1 {
    pub(crate) format_major: u16,
    pub(crate) format_minor: u16,
    pub(crate) flags: u32,
    pub(crate) page_size: u32,
    pub(crate) file_uuid: [u8; 16],
    pub(crate) root_ref: OcbBodyRefV2,
    pub(crate) crc32c: u32,
}

impl OcbBootstrapPageV1 {
    pub(crate) fn new(file_uuid: [u8; 16], root_ref: OcbBodyRefV2) -> Self {
        Self {
            format_major: OCB_FORMAT_MAJOR_V1,
            format_minor: OCB_FORMAT_MINOR_V1,
            flags: 0,
            page_size: OCB_BOOTSTRAP_PAGE_V1_LEN as u32,
            file_uuid,
            root_ref,
            crc32c: 0,
        }
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
        read_exact_ocb(&mut reader, &mut buf)?;
        let actual_crc = read_u32_at_end(&buf)?;
        write_u32_at_end(&mut buf, 0);
        let expected_crc = crc32c(&buf);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB bootstrap crc mismatch",
            ));
        }

        let mut cursor = Cursor::new(buf);
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_BOOTSTRAP_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB bootstrap magic",
            ));
        }
        let format_major = read_u16(&mut cursor)?;
        let format_minor = read_u16(&mut cursor)?;
        if format_major != OCB_FORMAT_MAJOR_V1 || format_minor > OCB_FORMAT_MINOR_V1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB bootstrap format version",
            ));
        }
        let flags = read_u32(&mut cursor)?;
        let page_size = read_u32(&mut cursor)?;
        if page_size as usize != OCB_BOOTSTRAP_PAGE_V1_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB bootstrap page size",
            ));
        }
        let mut file_uuid = [0u8; 16];
        read_exact_ocb(&mut cursor, &mut file_uuid)?;
        let root_ref = OcbBodyRefV2::read_from(&mut cursor)?;

        Ok(Self {
            format_major,
            format_minor,
            flags,
            page_size,
            file_uuid,
            root_ref,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.page_size as usize != OCB_BOOTSTRAP_PAGE_V1_LEN {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB bootstrap page_size must match reserved bootstrap page length",
            ));
        }
        if !self.root_ref.is_null() && self.root_ref.kind != OcbBodyKindV1::Root {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB bootstrap root_ref must reference root object",
            ));
        }

        let mut buf = Vec::with_capacity(OCB_BOOTSTRAP_PAGE_V1_LEN);
        buf.extend_from_slice(&OCB_BOOTSTRAP_MAGIC_V1);
        write_u16(&mut buf, self.format_major)?;
        write_u16(&mut buf, self.format_minor)?;
        write_u32(&mut buf, self.flags)?;
        write_u32(&mut buf, self.page_size)?;
        buf.extend_from_slice(&self.file_uuid);
        self.root_ref.write_to(&mut buf)?;
        buf.resize(OCB_BOOTSTRAP_PAGE_V1_LEN - 4, 0);
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbRootSlotV2 {
    pub(crate) version: u16,
    pub(crate) slot_id: u16,
    pub(crate) flags: u16,
    pub(crate) generation: u64,
    pub(crate) root_ref: OcbBodyRefV2,
    pub(crate) previous_generation: u64,
    pub(crate) previous_root_ref: OcbBodyRefV2,
    pub(crate) commit_diagnostics_ref: OcbBodyRefV2,
    pub(crate) committed_unix_nanos: u64,
    pub(crate) writer_version: [u8; 16],
    pub(crate) crc32c: u32,
}

impl OcbRootSlotV2 {
    pub(crate) fn empty(slot_id: u16) -> Self {
        Self {
            version: OCB_FORMAT_MAJOR_V2,
            slot_id,
            flags: 0,
            generation: 0,
            root_ref: OcbBodyRefV2::NULL,
            previous_generation: 0,
            previous_root_ref: OcbBodyRefV2::NULL,
            commit_diagnostics_ref: OcbBodyRefV2::NULL,
            committed_unix_nanos: 0,
            writer_version: [0u8; 16],
            crc32c: 0,
        }
    }

    pub(crate) fn new(
        slot_id: u16,
        generation: u64,
        root_ref: OcbBodyRefV2,
        previous_generation: u64,
        previous_root_ref: OcbBodyRefV2,
        commit_diagnostics_ref: OcbBodyRefV2,
    ) -> Self {
        Self {
            version: OCB_FORMAT_MAJOR_V2,
            slot_id,
            flags: 0,
            generation,
            root_ref,
            previous_generation,
            previous_root_ref,
            commit_diagnostics_ref,
            committed_unix_nanos: 0,
            writer_version: [0u8; 16],
            crc32c: 0,
        }
    }

    pub(crate) const fn is_empty(&self) -> bool {
        self.root_ref.is_null()
    }

    pub(crate) fn validate_candidate(&self, expected_slot_id: u16, file_len: u64) -> Result<()> {
        if self.slot_id != expected_slot_id {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root slot id does not match table position",
            ));
        }
        if self.is_empty() {
            return Err(ArcadiaTioError::ocb_corrupt_file("OCB root slot is empty"));
        }
        self.root_ref.validate(OcbBodyKindV1::Root, file_len)?;
        if self.root_ref.length != OCB_ROOT_V2_LEN as u64 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root slot root_ref length is invalid",
            ));
        }
        if !self.previous_root_ref.is_null() {
            self.previous_root_ref
                .validate(OcbBodyKindV1::Root, file_len)?;
        }
        validate_optional_ref_any_kind(
            self.commit_diagnostics_ref,
            &[
                OcbBodyKindV1::DebugJsonMetadata,
                OcbBodyKindV1::RowGroupIndexDelta,
            ],
            file_len,
        )?;
        Ok(())
    }

    pub(crate) fn validate_root(&self, root: &OcbRootV2) -> Result<()> {
        if root.generation != self.generation {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root generation does not match root slot",
            ));
        }
        if root.previous_generation != self.previous_generation {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root previous_generation does not match root slot",
            ));
        }
        if root.previous_root_ref != self.previous_root_ref {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root previous_root_ref does not match root slot",
            ));
        }
        if root.commit_diagnostics_ref != self.commit_diagnostics_ref {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root commit diagnostics ref does not match root slot",
            ));
        }
        Ok(())
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = vec![0u8; OCB_ROOT_SLOT_V2_LEN];
        read_exact_ocb(&mut reader, &mut buf)?;
        let actual_crc = read_u32_at_end(&buf)?;
        write_u32_at_end(&mut buf, 0);
        let expected_crc = crc32c(&buf);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB root slot crc mismatch",
            ));
        }

        let mut cursor = Cursor::new(buf);
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ROOT_SLOT_MAGIC_V2 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB root slot magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != OCB_FORMAT_MAJOR_V2 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB root slot version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes as usize != OCB_ROOT_SLOT_V2_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB root slot header length",
            ));
        }
        let slot_id = read_u16(&mut cursor)?;
        if slot_id as usize >= OCB_ROOT_SLOT_V2_COUNT {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB root slot id",
            ));
        }
        let flags = read_u16(&mut cursor)?;
        let generation = read_u64(&mut cursor)?;
        let root_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let previous_generation = read_u64(&mut cursor)?;
        let previous_root_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let commit_diagnostics_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let committed_unix_nanos = read_u64(&mut cursor)?;
        let mut writer_version = [0u8; 16];
        read_exact_ocb(&mut cursor, &mut writer_version)?;
        let _reserved0 = read_u32(&mut cursor)?;

        Ok(Self {
            version,
            slot_id,
            flags,
            generation,
            root_ref,
            previous_generation,
            previous_root_ref,
            commit_diagnostics_ref,
            committed_unix_nanos,
            writer_version,
            crc32c: actual_crc,
        })
    }

    fn to_bytes(&self) -> Result<[u8; OCB_ROOT_SLOT_V2_LEN]> {
        let mut bytes = Vec::with_capacity(OCB_ROOT_SLOT_V2_LEN);
        self.write_to(&mut bytes)?;
        let mut out = [0u8; OCB_ROOT_SLOT_V2_LEN];
        out.copy_from_slice(&bytes);
        Ok(out)
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.version != OCB_FORMAT_MAJOR_V2 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root slot version must be 2",
            ));
        }
        if self.slot_id as usize >= OCB_ROOT_SLOT_V2_COUNT {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root slot id is out of range",
            ));
        }
        if !self.root_ref.is_null() {
            if self.root_ref.kind != OcbBodyKindV1::Root {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB root slot root_ref must reference root object",
                ));
            }
            if self.root_ref.length != OCB_ROOT_V2_LEN as u64 {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB root slot root_ref length must match v2 root length",
                ));
            }
        }
        if !self.previous_root_ref.is_null() && self.previous_root_ref.kind != OcbBodyKindV1::Root {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root slot previous_root_ref must reference root object",
            ));
        }
        if !self.commit_diagnostics_ref.is_null()
            && self.commit_diagnostics_ref.kind != OcbBodyKindV1::DebugJsonMetadata
            && self.commit_diagnostics_ref.kind != OcbBodyKindV1::RowGroupIndexDelta
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root slot commit_diagnostics_ref must reference debug JSON metadata or row-group index delta",
            ));
        }

        let mut buf = Vec::with_capacity(OCB_ROOT_SLOT_V2_LEN);
        buf.extend_from_slice(&OCB_ROOT_SLOT_MAGIC_V2);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ROOT_SLOT_V2_LEN as u16)?;
        write_u16(&mut buf, self.slot_id)?;
        write_u16(&mut buf, self.flags)?;
        write_u64(&mut buf, self.generation)?;
        self.root_ref.write_to(&mut buf)?;
        write_u64(&mut buf, self.previous_generation)?;
        self.previous_root_ref.write_to(&mut buf)?;
        self.commit_diagnostics_ref.write_to(&mut buf)?;
        write_u64(&mut buf, self.committed_unix_nanos)?;
        buf.extend_from_slice(&self.writer_version);
        write_u32(&mut buf, 0)?;
        write_u32(&mut buf, 0)?;
        debug_assert_eq!(buf.len(), OCB_ROOT_SLOT_V2_LEN);
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbBootstrapPageV2 {
    pub(crate) format_major: u16,
    pub(crate) format_minor: u16,
    pub(crate) flags: u32,
    pub(crate) page_size: u32,
    pub(crate) file_uuid: [u8; 16],
    root_slot_bytes: [[u8; OCB_ROOT_SLOT_V2_LEN]; OCB_ROOT_SLOT_V2_COUNT],
    pub(crate) crc32c: u32,
}

impl OcbBootstrapPageV2 {
    pub(crate) fn new(
        file_uuid: [u8; 16],
        root_slots: [OcbRootSlotV2; OCB_ROOT_SLOT_V2_COUNT],
    ) -> Result<Self> {
        let mut root_slot_bytes = [[0u8; OCB_ROOT_SLOT_V2_LEN]; OCB_ROOT_SLOT_V2_COUNT];
        for (idx, slot) in root_slots.iter().enumerate() {
            root_slot_bytes[idx] = slot.to_bytes()?;
        }
        Ok(Self {
            format_major: OCB_FORMAT_MAJOR_V2,
            format_minor: OCB_FORMAT_MINOR_V2,
            flags: 0,
            page_size: OCB_BOOTSTRAP_PAGE_V2_LEN as u32,
            file_uuid,
            root_slot_bytes,
            crc32c: 0,
        })
    }

    pub(crate) fn decoded_root_slots(&self) -> Vec<Result<OcbRootSlotV2>> {
        self.root_slot_bytes
            .iter()
            .map(|bytes| OcbRootSlotV2::read_from(Cursor::new(bytes.as_slice())))
            .collect()
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = Self::bootstrap_crc32c(&buf)?;
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = vec![0u8; OCB_BOOTSTRAP_PAGE_V2_LEN];
        read_exact_ocb(&mut reader, &mut buf)?;
        let actual_crc = read_u32_at_end(&buf)?;
        let expected_crc = Self::bootstrap_crc32c(&buf)?;
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB bootstrap crc mismatch",
            ));
        }

        let mut cursor = Cursor::new(buf.clone());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_BOOTSTRAP_MAGIC_V2 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB bootstrap magic",
            ));
        }
        let format_major = read_u16(&mut cursor)?;
        let format_minor = read_u16(&mut cursor)?;
        if format_major != OCB_FORMAT_MAJOR_V2 || format_minor > OCB_FORMAT_MINOR_V2 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB bootstrap format version",
            ));
        }
        let flags = read_u32(&mut cursor)?;
        let page_size = read_u32(&mut cursor)?;
        if page_size as usize != OCB_BOOTSTRAP_PAGE_V2_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB bootstrap page size",
            ));
        }
        let mut file_uuid = [0u8; 16];
        read_exact_ocb(&mut cursor, &mut file_uuid)?;
        let mut root_slot_bytes = [[0u8; OCB_ROOT_SLOT_V2_LEN]; OCB_ROOT_SLOT_V2_COUNT];
        for (idx, slot_bytes) in root_slot_bytes.iter_mut().enumerate() {
            let start = OCB_ROOT_SLOT_TABLE_V2_OFFSET + idx * OCB_ROOT_SLOT_V2_LEN;
            let end = start + OCB_ROOT_SLOT_V2_LEN;
            slot_bytes.copy_from_slice(&buf[start..end]);
        }

        Ok(Self {
            format_major,
            format_minor,
            flags,
            page_size,
            file_uuid,
            root_slot_bytes,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.page_size as usize != OCB_BOOTSTRAP_PAGE_V2_LEN {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB bootstrap page_size must match reserved bootstrap page length",
            ));
        }
        if self.format_major != OCB_FORMAT_MAJOR_V2 || self.format_minor > OCB_FORMAT_MINOR_V2 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB bootstrap format version",
            ));
        }

        let mut buf = Vec::with_capacity(OCB_BOOTSTRAP_PAGE_V2_LEN);
        buf.extend_from_slice(&OCB_BOOTSTRAP_MAGIC_V2);
        write_u16(&mut buf, self.format_major)?;
        write_u16(&mut buf, self.format_minor)?;
        write_u32(&mut buf, self.flags)?;
        write_u32(&mut buf, self.page_size)?;
        buf.extend_from_slice(&self.file_uuid);
        buf.resize(OCB_ROOT_SLOT_TABLE_V2_OFFSET, 0);
        for slot_bytes in &self.root_slot_bytes {
            buf.extend_from_slice(slot_bytes);
        }
        buf.resize(OCB_BOOTSTRAP_PAGE_V2_LEN - 4, 0);
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }

    fn bootstrap_crc32c(bytes: &[u8]) -> Result<u32> {
        if bytes.len() != OCB_BOOTSTRAP_PAGE_V2_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB bootstrap checksum requires one full bootstrap page",
            ));
        }
        let mut stable = bytes.to_vec();
        let slot_start = OCB_ROOT_SLOT_TABLE_V2_OFFSET;
        let slot_end = slot_start + OCB_ROOT_SLOT_TABLE_V2_LEN;
        stable[slot_start..slot_end].fill(0);
        write_u32_at_end(&mut stable, 0);
        Ok(crc32c(&stable))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbRootV2 {
    pub(crate) version: u16,
    pub(crate) flags: u32,
    pub(crate) generation: u64,
    pub(crate) previous_generation: u64,
    pub(crate) previous_root_ref: OcbBodyRefV2,
    pub(crate) append_base_row: u64,
    pub(crate) append_row_count: u64,
    pub(crate) append_base_row_group: u32,
    pub(crate) append_row_group_count: u32,
    pub(crate) row_count: u64,
    pub(crate) column_count: u32,
    pub(crate) row_group_count: u32,
    pub(crate) dictionary_count: u32,
    pub(crate) column_chunk_count: u32,
    pub(crate) schema_ref: OcbBodyRefV2,
    pub(crate) dictionary_index_ref: OcbBodyRefV2,
    pub(crate) row_group_index_ref: OcbBodyRefV2,
    pub(crate) ordering_proof_ref: OcbBodyRefV2,
    pub(crate) debug_json_ref: OcbBodyRefV2,
    pub(crate) first_key_tuple_ref: OcbBodyRefV2,
    pub(crate) last_key_tuple_ref: OcbBodyRefV2,
    pub(crate) append_first_key_tuple_ref: OcbBodyRefV2,
    pub(crate) append_last_key_tuple_ref: OcbBodyRefV2,
    pub(crate) commit_diagnostics_ref: OcbBodyRefV2,
    pub(crate) created_unix_nanos: u64,
    pub(crate) content_flags: u64,
    pub(crate) crc32c: u32,
}

impl OcbRootV2 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = vec![0u8; OCB_ROOT_V2_LEN as usize];
        read_exact_ocb(&mut reader, &mut buf)?;
        let actual_crc = read_u32_at_end(&buf)?;
        write_u32_at_end(&mut buf, 0);
        let expected_crc = crc32c(&buf);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file("OCB root crc mismatch"));
        }

        let mut cursor = Cursor::new(buf);
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ROOT_MAGIC_V2 {
            return Err(ArcadiaTioError::ocb_corrupt_file("invalid OCB root magic"));
        }
        let version = read_u16(&mut cursor)?;
        if version != OCB_FORMAT_MAJOR_V2 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB root version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_ROOT_V2_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB root header length",
            ));
        }
        let flags = read_u32(&mut cursor)?;
        let generation = read_u64(&mut cursor)?;
        let previous_generation = read_u64(&mut cursor)?;
        let previous_root_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let append_base_row = read_u64(&mut cursor)?;
        let append_row_count = read_u64(&mut cursor)?;
        let append_base_row_group = read_u32(&mut cursor)?;
        let append_row_group_count = read_u32(&mut cursor)?;
        let row_count = read_u64(&mut cursor)?;
        let column_count = read_u32(&mut cursor)?;
        let row_group_count = read_u32(&mut cursor)?;
        let dictionary_count = read_u32(&mut cursor)?;
        let column_chunk_count = read_u32(&mut cursor)?;
        let schema_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let dictionary_index_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let row_group_index_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let ordering_proof_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let debug_json_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let first_key_tuple_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let last_key_tuple_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let append_first_key_tuple_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let append_last_key_tuple_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let commit_diagnostics_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let created_unix_nanos = read_u64(&mut cursor)?;
        let content_flags = read_u64(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;

        Ok(Self {
            version,
            flags,
            generation,
            previous_generation,
            previous_root_ref,
            append_base_row,
            append_row_count,
            append_base_row_group,
            append_row_group_count,
            row_count,
            column_count,
            row_group_count,
            dictionary_count,
            column_chunk_count,
            schema_ref,
            dictionary_index_ref,
            row_group_index_ref,
            ordering_proof_ref,
            debug_json_ref,
            first_key_tuple_ref,
            last_key_tuple_ref,
            append_first_key_tuple_ref,
            append_last_key_tuple_ref,
            commit_diagnostics_ref,
            created_unix_nanos,
            content_flags,
            crc32c: actual_crc,
        })
    }

    pub(crate) fn validate_references(&self, file_len: u64) -> Result<()> {
        self.validate_ref_kinds()?;
        self.schema_ref.validate(OcbBodyKindV1::Schema, file_len)?;
        self.row_group_index_ref
            .validate(OcbBodyKindV1::RowGroupIndex, file_len)?;
        validate_optional_ref(
            self.dictionary_index_ref,
            OcbBodyKindV1::DictionaryIndex,
            file_len,
        )?;
        validate_optional_ref(
            self.ordering_proof_ref,
            OcbBodyKindV1::OrderingProof,
            file_len,
        )?;
        validate_optional_ref(
            self.debug_json_ref,
            OcbBodyKindV1::DebugJsonMetadata,
            file_len,
        )?;
        validate_optional_ref(self.first_key_tuple_ref, OcbBodyKindV1::KeyTuple, file_len)?;
        validate_optional_ref(self.last_key_tuple_ref, OcbBodyKindV1::KeyTuple, file_len)?;
        validate_optional_ref(
            self.append_first_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            file_len,
        )?;
        validate_optional_ref(
            self.append_last_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            file_len,
        )?;
        validate_optional_ref_any_kind(
            self.commit_diagnostics_ref,
            &[
                OcbBodyKindV1::DebugJsonMetadata,
                OcbBodyKindV1::RowGroupIndexDelta,
            ],
            file_len,
        )?;
        validate_optional_ref(self.previous_root_ref, OcbBodyKindV1::Root, file_len)?;
        Ok(())
    }

    pub(crate) fn to_v1_root(&self) -> OcbRootV1 {
        OcbRootV1 {
            version: 1,
            flags: self.flags,
            row_count: self.row_count,
            column_count: self.column_count,
            row_group_count: self.row_group_count,
            dictionary_count: self.dictionary_count,
            schema_ref: self.schema_ref,
            dictionary_index_ref: self.dictionary_index_ref,
            row_group_index_ref: self.row_group_index_ref,
            ordering_proof_ref: self.ordering_proof_ref,
            debug_json_ref: self.debug_json_ref,
            created_unix_nanos: self.created_unix_nanos,
            content_flags: self.content_flags,
            crc32c: self.crc32c,
        }
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.version != OCB_FORMAT_MAJOR_V2 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root version must be 2",
            ));
        }
        self.validate_ref_kinds()?;
        let append_row_end = self
            .append_base_row
            .checked_add(self.append_row_count)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB root append row range overflows",
            ))?;
        if append_row_end > self.row_count {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root append row range exceeds total rows",
            ));
        }
        let append_group_end = self
            .append_base_row_group
            .checked_add(self.append_row_group_count)
            .ok_or(ArcadiaTioError::ocb_invalid_input(
                "OCB root append row-group range overflows",
            ))?;
        if append_group_end > self.row_group_count {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root append row-group range exceeds total row groups",
            ));
        }

        let mut buf = Vec::with_capacity(OCB_ROOT_V2_LEN as usize);
        buf.extend_from_slice(&OCB_ROOT_MAGIC_V2);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ROOT_V2_LEN)?;
        write_u32(&mut buf, self.flags)?;
        write_u64(&mut buf, self.generation)?;
        write_u64(&mut buf, self.previous_generation)?;
        self.previous_root_ref.write_to(&mut buf)?;
        write_u64(&mut buf, self.append_base_row)?;
        write_u64(&mut buf, self.append_row_count)?;
        write_u32(&mut buf, self.append_base_row_group)?;
        write_u32(&mut buf, self.append_row_group_count)?;
        write_u64(&mut buf, self.row_count)?;
        write_u32(&mut buf, self.column_count)?;
        write_u32(&mut buf, self.row_group_count)?;
        write_u32(&mut buf, self.dictionary_count)?;
        write_u32(&mut buf, self.column_chunk_count)?;
        self.schema_ref.write_to(&mut buf)?;
        self.dictionary_index_ref.write_to(&mut buf)?;
        self.row_group_index_ref.write_to(&mut buf)?;
        self.ordering_proof_ref.write_to(&mut buf)?;
        self.debug_json_ref.write_to(&mut buf)?;
        self.first_key_tuple_ref.write_to(&mut buf)?;
        self.last_key_tuple_ref.write_to(&mut buf)?;
        self.append_first_key_tuple_ref.write_to(&mut buf)?;
        self.append_last_key_tuple_ref.write_to(&mut buf)?;
        self.commit_diagnostics_ref.write_to(&mut buf)?;
        write_u64(&mut buf, self.created_unix_nanos)?;
        write_u64(&mut buf, self.content_flags)?;
        write_u32(&mut buf, 0)?;
        write_u32(&mut buf, 0)?;
        debug_assert_eq!(buf.len(), OCB_ROOT_V2_LEN as usize);
        Ok(buf)
    }

    fn validate_ref_kinds(&self) -> Result<()> {
        if self.schema_ref.kind != OcbBodyKindV1::Schema {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root schema_ref must reference schema",
            ));
        }
        if self.row_group_index_ref.kind != OcbBodyKindV1::RowGroupIndex {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root row_group_index_ref must reference row-group index",
            ));
        }
        validate_optional_ref_kind(
            self.dictionary_index_ref,
            OcbBodyKindV1::DictionaryIndex,
            "OCB root dictionary_index_ref must reference dictionary index",
        )?;
        validate_optional_ref_kind(
            self.ordering_proof_ref,
            OcbBodyKindV1::OrderingProof,
            "OCB root ordering_proof_ref must reference ordering proof",
        )?;
        validate_optional_ref_kind(
            self.debug_json_ref,
            OcbBodyKindV1::DebugJsonMetadata,
            "OCB root debug_json_ref must reference debug JSON metadata",
        )?;
        validate_optional_ref_kind(
            self.first_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            "OCB root first_key_tuple_ref must reference key tuple",
        )?;
        validate_optional_ref_kind(
            self.last_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            "OCB root last_key_tuple_ref must reference key tuple",
        )?;
        validate_optional_ref_kind(
            self.append_first_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            "OCB root append_first_key_tuple_ref must reference key tuple",
        )?;
        validate_optional_ref_kind(
            self.append_last_key_tuple_ref,
            OcbBodyKindV1::KeyTuple,
            "OCB root append_last_key_tuple_ref must reference key tuple",
        )?;
        validate_optional_ref_kinds(
            self.commit_diagnostics_ref,
            &[
                OcbBodyKindV1::DebugJsonMetadata,
                OcbBodyKindV1::RowGroupIndexDelta,
            ],
            "OCB root commit_diagnostics_ref must reference debug JSON metadata or row-group index delta",
        )?;
        validate_optional_ref_kind(
            self.previous_root_ref,
            OcbBodyKindV1::Root,
            "OCB root previous_root_ref must reference root object",
        )?;
        Ok(())
    }
}

fn validate_optional_ref(
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    file_len: u64,
) -> Result<()> {
    validate_optional_ref_any_kind(reference, &[expected_kind], file_len)
}

fn validate_optional_ref_any_kind(
    reference: OcbBodyRefV2,
    expected_kinds: &[OcbBodyKindV1],
    file_len: u64,
) -> Result<()> {
    if reference.is_null() {
        Ok(())
    } else if expected_kinds.contains(&reference.kind) {
        reference.validate(reference.kind, file_len)
    } else {
        Err(ArcadiaTioError::ocb_invalid_input(
            "OCB body reference has unexpected kind",
        ))
    }
}

fn validate_optional_ref_kind(
    reference: OcbBodyRefV2,
    expected_kind: OcbBodyKindV1,
    message: &'static str,
) -> Result<()> {
    validate_optional_ref_kinds(reference, &[expected_kind], message)
}

fn validate_optional_ref_kinds(
    reference: OcbBodyRefV2,
    expected_kinds: &[OcbBodyKindV1],
    message: &'static str,
) -> Result<()> {
    if !reference.is_null() && !expected_kinds.contains(&reference.kind) {
        Err(ArcadiaTioError::ocb_invalid_input(message))
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbRootV1 {
    pub(crate) version: u16,
    pub(crate) flags: u32,
    pub(crate) row_count: u64,
    pub(crate) column_count: u32,
    pub(crate) row_group_count: u32,
    pub(crate) dictionary_count: u32,
    pub(crate) schema_ref: OcbBodyRefV2,
    pub(crate) dictionary_index_ref: OcbBodyRefV2,
    pub(crate) row_group_index_ref: OcbBodyRefV2,
    pub(crate) ordering_proof_ref: OcbBodyRefV2,
    pub(crate) debug_json_ref: OcbBodyRefV2,
    pub(crate) created_unix_nanos: u64,
    pub(crate) content_flags: u64,
    pub(crate) crc32c: u32,
}

impl OcbRootV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut buf = vec![0u8; OCB_ROOT_V1_LEN as usize];
        read_exact_ocb(&mut reader, &mut buf)?;
        let actual_crc = read_u32_at_end(&buf)?;
        write_u32_at_end(&mut buf, 0);
        let expected_crc = crc32c(&buf);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file("OCB root crc mismatch"));
        }

        let mut cursor = Cursor::new(buf);
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ROOT_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file("invalid OCB root magic"));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB root version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_ROOT_V1_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB root header length",
            ));
        }
        let flags = read_u32(&mut cursor)?;
        let row_count = read_u64(&mut cursor)?;
        let column_count = read_u32(&mut cursor)?;
        let row_group_count = read_u32(&mut cursor)?;
        let dictionary_count = read_u32(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;
        let schema_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let dictionary_index_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let row_group_index_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let ordering_proof_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let debug_json_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let created_unix_nanos = read_u64(&mut cursor)?;
        let content_flags = read_u64(&mut cursor)?;

        Ok(Self {
            version,
            flags,
            row_count,
            column_count,
            row_group_count,
            dictionary_count,
            schema_ref,
            dictionary_index_ref,
            row_group_index_ref,
            ordering_proof_ref,
            debug_json_ref,
            created_unix_nanos,
            content_flags,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.schema_ref.kind != OcbBodyKindV1::Schema {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root schema_ref must reference schema",
            ));
        }
        if self.row_group_index_ref.kind != OcbBodyKindV1::RowGroupIndex {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root row_group_index_ref must reference row-group index",
            ));
        }
        if !self.dictionary_index_ref.is_null()
            && self.dictionary_index_ref.kind != OcbBodyKindV1::DictionaryIndex
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root dictionary_index_ref must reference dictionary index",
            ));
        }
        if !self.ordering_proof_ref.is_null()
            && self.ordering_proof_ref.kind != OcbBodyKindV1::OrderingProof
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root ordering_proof_ref must reference ordering proof",
            ));
        }
        if !self.debug_json_ref.is_null()
            && self.debug_json_ref.kind != OcbBodyKindV1::DebugJsonMetadata
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB root debug_json_ref must reference debug JSON metadata",
            ));
        }

        let mut buf = Vec::with_capacity(OCB_ROOT_V1_LEN as usize);
        buf.extend_from_slice(&OCB_ROOT_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ROOT_V1_LEN)?;
        write_u32(&mut buf, self.flags)?;
        write_u64(&mut buf, self.row_count)?;
        write_u32(&mut buf, self.column_count)?;
        write_u32(&mut buf, self.row_group_count)?;
        write_u32(&mut buf, self.dictionary_count)?;
        write_u32(&mut buf, 0)?;
        self.schema_ref.write_to(&mut buf)?;
        self.dictionary_index_ref.write_to(&mut buf)?;
        self.row_group_index_ref.write_to(&mut buf)?;
        self.ordering_proof_ref.write_to(&mut buf)?;
        self.debug_json_ref.write_to(&mut buf)?;
        write_u64(&mut buf, self.created_unix_nanos)?;
        write_u64(&mut buf, self.content_flags)?;
        write_u32(&mut buf, 0)?;
        debug_assert_eq!(buf.len(), OCB_ROOT_V1_LEN as usize);
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbStringTableV1 {
    pub(crate) version: u16,
    pub(crate) strings: Vec<String>,
    pub(crate) crc32c: u32,
}

impl OcbStringTableV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB string table crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_STRING_TABLE_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB string table magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB string table version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_STRING_TABLE_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB string table header length",
            ));
        }
        let string_count = read_u32(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;
        let string_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(string_count),
            4,
            "OCB string table string count",
        )?;
        let strings_start = cursor.position();
        let mut string_bytes = 0u64;
        for _ in 0..string_count {
            let len = inspect_string_u32_bounded(&mut cursor, bytes.len())?;
            string_bytes = string_bytes
                .checked_add(u64::try_from(len).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file("OCB string length exceeds u64")
                })?)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB string table materialized byte count overflows",
                ))?;
        }
        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<String>(string_count)?,
            string_bytes,
        ])?;
        budget.charge(retained_bytes)?;

        cursor.set_position(strings_start);
        let mut strings = try_vec_with_capacity(
            string_count,
            "OCB string-table allocation failed within resource limit",
        )?;
        for _ in 0..string_count {
            strings.push(read_string_u32_bounded(&mut cursor, bytes.len())?);
        }
        Ok(Self {
            version,
            strings,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let string_count = u32::try_from(self.strings.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB string table has too many strings")
        })?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_STRING_TABLE_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_STRING_TABLE_V1_HEADER_LEN)?;
        write_u32(&mut buf, string_count)?;
        write_u32(&mut buf, 0)?;
        for value in &self.strings {
            write_string_u32(&mut buf, value)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbColumnDescV1 {
    pub(crate) column_id: u32,
    pub(crate) name_string_id: u32,
    pub(crate) physical_type: OcbPhysicalTypeV1,
    pub(crate) logical_kind: OcbLogicalKindV1,
    pub(crate) flags: u32,
    pub(crate) dictionary_id: u32,
    pub(crate) scale: i32,
    pub(crate) nullability: OcbNullabilityV1,
    pub(crate) reserved0: u16,
    pub(crate) fixed_binary_width: u32,
}

impl OcbColumnDescV1 {
    pub(crate) fn value_byte_width(&self) -> Result<u32> {
        match self.physical_type {
            OcbPhysicalTypeV1::FixedBinary => {
                if self.fixed_binary_width == 0 {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB fixed-binary column requires fixed width",
                    ));
                }
                Ok(self.fixed_binary_width)
            }
            _ => {
                if self.fixed_binary_width != 0 {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB primitive column has unexpected fixed-binary width",
                    ));
                }
                Ok(self
                    .physical_type
                    .primitive_byte_width()
                    .expect("primitive physical type has byte width") as u32)
            }
        }
    }

    pub(crate) fn expected_value_bytes(&self, row_count: u64) -> Result<u64> {
        row_count
            .checked_mul(u64::from(self.value_byte_width()?))
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB column chunk value byte length overflows",
            ))
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        if matches!(self.logical_kind, OcbLogicalKindV1::DictionaryCode)
            && self.dictionary_id == OCB_NULL_U32
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB dictionary-coded column must reference a dictionary",
            ));
        }
        match self.physical_type {
            OcbPhysicalTypeV1::FixedBinary if self.fixed_binary_width == 0 => {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-binary column requires fixed width",
                ));
            }
            OcbPhysicalTypeV1::FixedBinary => {}
            _ if self.fixed_binary_width != 0 => {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB primitive column has unexpected fixed-binary width",
                ));
            }
            _ => {}
        }
        write_u32(&mut writer, self.column_id)?;
        write_u32(&mut writer, self.name_string_id)?;
        write_u16(&mut writer, self.physical_type as u16)?;
        write_u16(&mut writer, self.logical_kind as u16)?;
        write_u32(&mut writer, self.flags)?;
        write_u32(&mut writer, self.dictionary_id)?;
        write_i32(&mut writer, self.scale)?;
        write_u16(&mut writer, self.nullability as u16)?;
        write_u16(&mut writer, self.reserved0)?;
        write_u32(&mut writer, self.fixed_binary_width)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            column_id: read_u32(&mut reader)?,
            name_string_id: read_u32(&mut reader)?,
            physical_type: OcbPhysicalTypeV1::from_u16(read_u16(&mut reader)?)?,
            logical_kind: OcbLogicalKindV1::from_u16(read_u16(&mut reader)?)?,
            flags: read_u32(&mut reader)?,
            dictionary_id: read_u32(&mut reader)?,
            scale: read_i32(&mut reader)?,
            nullability: OcbNullabilityV1::from_u16(read_u16(&mut reader)?)?,
            reserved0: read_u16(&mut reader)?,
            fixed_binary_width: read_u32(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbSchemaV1 {
    pub(crate) version: u16,
    pub(crate) string_table_ref: OcbBodyRefV2,
    pub(crate) columns: Vec<OcbColumnDescV1>,
    pub(crate) crc32c: u32,
}

impl OcbSchemaV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file("OCB schema crc mismatch"));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_SCHEMA_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB schema magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB schema version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_SCHEMA_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB schema header length",
            ));
        }
        let column_count = read_u32(&mut cursor)?;
        let string_table_ref = OcbBodyRefV2::read_from(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;
        let column_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(column_count),
            OCB_COLUMN_DESC_V1_LEN,
            "OCB schema column count",
        )?;
        let columns_start = cursor.position();
        validate_record_sequence(&mut cursor, column_count, |reader| {
            OcbColumnDescV1::read_from(reader)
        })?;
        budget.charge(checked_materialized_vec_bytes::<OcbColumnDescV1>(
            column_count,
        )?)?;

        cursor.set_position(columns_start);
        let columns = read_record_sequence(
            &mut cursor,
            column_count,
            "OCB schema allocation failed within resource limit",
            |reader| OcbColumnDescV1::read_from(reader),
        )?;
        Ok(Self {
            version,
            string_table_ref,
            columns,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        if self.string_table_ref.kind != OcbBodyKindV1::StringTable {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB schema string_table_ref must reference string table",
            ));
        }
        let column_count = u32::try_from(self.columns.len())
            .map_err(|_| ArcadiaTioError::ocb_invalid_input("OCB schema has too many columns"))?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_SCHEMA_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_SCHEMA_V1_HEADER_LEN)?;
        write_u32(&mut buf, column_count)?;
        self.string_table_ref.write_to(&mut buf)?;
        write_u32(&mut buf, 0)?;
        for column in &self.columns {
            column.write_to(&mut buf)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbDictionaryDescV1 {
    pub(crate) dictionary_id: u32,
    pub(crate) name_string_id: u32,
    pub(crate) code_physical_type: OcbPhysicalTypeV1,
    pub(crate) value_kind: OcbDictionaryValueKindV1,
    pub(crate) flags: u32,
    pub(crate) values_ref: OcbBodyRefV2,
    pub(crate) entry_count: u32,
    pub(crate) reserved0: u32,
}

impl OcbDictionaryDescV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        if self.values_ref.kind != OcbBodyKindV1::DictionaryValues {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB dictionary values_ref must reference dictionary values",
            ));
        }
        write_u32(&mut writer, self.dictionary_id)?;
        write_u32(&mut writer, self.name_string_id)?;
        write_u16(&mut writer, self.code_physical_type as u16)?;
        write_u16(&mut writer, self.value_kind as u16)?;
        write_u32(&mut writer, self.flags)?;
        self.values_ref.write_to(&mut writer)?;
        write_u32(&mut writer, self.entry_count)?;
        write_u32(&mut writer, self.reserved0)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            dictionary_id: read_u32(&mut reader)?,
            name_string_id: read_u32(&mut reader)?,
            code_physical_type: OcbPhysicalTypeV1::from_u16(read_u16(&mut reader)?)?,
            value_kind: OcbDictionaryValueKindV1::from_u16(read_u16(&mut reader)?)?,
            flags: read_u32(&mut reader)?,
            values_ref: OcbBodyRefV2::read_from(&mut reader)?,
            entry_count: read_u32(&mut reader)?,
            reserved0: read_u32(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbDictionaryIndexV1 {
    pub(crate) version: u16,
    pub(crate) dictionaries: Vec<OcbDictionaryDescV1>,
    pub(crate) crc32c: u32,
}

impl OcbDictionaryIndexV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary index crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_DICTIONARY_INDEX_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB dictionary index magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB dictionary index version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_DICTIONARY_INDEX_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB dictionary index header length",
            ));
        }
        let dictionary_count = read_u32(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;
        let dictionary_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(dictionary_count),
            56,
            "OCB dictionary index count",
        )?;
        let dictionaries_start = cursor.position();
        validate_record_sequence(&mut cursor, dictionary_count, |reader| {
            OcbDictionaryDescV1::read_from(reader)
        })?;
        budget.charge(checked_materialized_vec_bytes::<OcbDictionaryDescV1>(
            dictionary_count,
        )?)?;

        cursor.set_position(dictionaries_start);
        let dictionaries = read_record_sequence(
            &mut cursor,
            dictionary_count,
            "OCB dictionary-index allocation failed within resource limit",
            |reader| OcbDictionaryDescV1::read_from(reader),
        )?;
        Ok(Self {
            version,
            dictionaries,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let dictionary_count = u32::try_from(self.dictionaries.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB dictionary index has too many dictionaries")
        })?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_DICTIONARY_INDEX_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_DICTIONARY_INDEX_V1_HEADER_LEN)?;
        write_u32(&mut buf, dictionary_count)?;
        write_u32(&mut buf, 0)?;
        for dictionary in &self.dictionaries {
            dictionary.write_to(&mut buf)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbDictionaryValuesV1 {
    pub(crate) version: u16,
    pub(crate) value_kind: OcbDictionaryValueKindV1,
    pub(crate) fixed_width: u32,
    pub(crate) values: Vec<Vec<u8>>,
    pub(crate) crc32c: u32,
}

impl OcbDictionaryValuesV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_DICTIONARY_VALUES_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB dictionary values magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB dictionary values version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_DICTIONARY_VALUES_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB dictionary values header length",
            ));
        }
        let value_kind = OcbDictionaryValueKindV1::from_u16(read_u16(&mut cursor)?)?;
        let _reserved0 = read_u16(&mut cursor)?;
        let entry_count = read_u32(&mut cursor)?;
        let fixed_width = read_u32(&mut cursor)?;
        let data_bytes = read_u64(&mut cursor)?;
        let _flags = read_u32(&mut cursor)?;
        let _reserved1 = read_u32(&mut cursor)?;

        let offset_count = entry_count
            .checked_add(1)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values offset count overflows",
            ))?;
        let offset_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(offset_count),
            8,
            "OCB dictionary values offset count",
        )?;
        let entry_count = offset_count.saturating_sub(1);
        let offsets_start = cursor.position();
        let mut previous_offset = read_u64(&mut cursor)?;
        if previous_offset != 0 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values offsets do not cover data bytes",
            ));
        }
        for _ in 0..entry_count {
            let next_offset = read_u64(&mut cursor)?;
            if previous_offset > next_offset || next_offset > data_bytes {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB dictionary values offsets are invalid",
                ));
            }
            if value_kind == OcbDictionaryValueKindV1::FixedBytes
                && next_offset - previous_offset != u64::from(fixed_width)
            {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-byte dictionary value length does not match fixed width",
                ));
            }
            previous_offset = next_offset;
        }
        if previous_offset != data_bytes {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary values offsets do not cover data bytes",
            ));
        }
        let data_len = checked_payload_len(
            bytes.len(),
            cursor.position(),
            data_bytes,
            "OCB dictionary values data length",
        )?;
        let data_start = usize::try_from(cursor.position()).map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file(
                "OCB dictionary data offset exceeds addressable memory",
            )
        })?;
        let data_end =
            data_start
                .checked_add(data_len)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB dictionary data range overflows",
                ))?;
        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<Vec<u8>>(entry_count)?,
            u64::try_from(data_len).map_err(|_| {
                ArcadiaTioError::ocb_corrupt_file("OCB dictionary data length exceeds u64")
            })?,
        ])?;
        budget.charge(retained_bytes)?;

        let data = &bytes[data_start..data_end];
        let mut values = try_vec_with_capacity(
            entry_count,
            "OCB dictionary-value allocation failed within resource limit",
        )?;
        cursor.set_position(offsets_start);
        let mut previous_offset = read_u64(&mut cursor)?;
        for _ in 0..entry_count {
            let next_offset = read_u64(&mut cursor)?;
            let value_start = usize::try_from(previous_offset).map_err(|_| {
                ArcadiaTioError::ocb_corrupt_file("OCB dictionary value offset exceeds usize")
            })?;
            let value_end = usize::try_from(next_offset).map_err(|_| {
                ArcadiaTioError::ocb_corrupt_file("OCB dictionary value offset exceeds usize")
            })?;
            let value = copy_bytes_fallibly(
                &data[value_start..value_end],
                "OCB dictionary-entry allocation failed within resource limit",
            )?;
            values.push(value);
            previous_offset = next_offset;
        }
        Ok(Self {
            version,
            value_kind,
            fixed_width,
            values,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let entry_count = u32::try_from(self.values.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB dictionary values have too many entries")
        })?;
        if self.value_kind == OcbDictionaryValueKindV1::FixedBytes && self.fixed_width == 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB fixed-byte dictionary values require fixed_width > 0",
            ));
        }
        if self.value_kind != OcbDictionaryValueKindV1::FixedBytes && self.fixed_width != 0 {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB non-fixed dictionary values must use fixed_width 0",
            ));
        }

        let mut offsets = Vec::with_capacity(self.values.len() + 1);
        offsets.push(0u64);
        let mut data = Vec::new();
        for value in &self.values {
            if self.value_kind == OcbDictionaryValueKindV1::FixedBytes
                && value.len() as u32 != self.fixed_width
            {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB fixed-byte dictionary value length does not match fixed width",
                ));
            }
            data.extend_from_slice(value);
            offsets.push(data.len() as u64);
        }

        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_DICTIONARY_VALUES_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_DICTIONARY_VALUES_V1_HEADER_LEN)?;
        write_u16(&mut buf, self.value_kind as u16)?;
        write_u16(&mut buf, 0)?;
        write_u32(&mut buf, entry_count)?;
        write_u32(&mut buf, self.fixed_width)?;
        write_u64(&mut buf, data.len() as u64)?;
        write_u32(&mut buf, 0)?;
        write_u32(&mut buf, 0)?;
        for offset in offsets {
            write_u64(&mut buf, offset)?;
        }
        buf.extend_from_slice(&data);
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum OcbStatScalarV1 {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl OcbStatScalarV1 {
    pub(crate) const fn physical_type(self) -> OcbPhysicalTypeV1 {
        match self {
            Self::I32(_) => OcbPhysicalTypeV1::I32,
            Self::I64(_) => OcbPhysicalTypeV1::I64,
            Self::F32(_) => OcbPhysicalTypeV1::F32,
            Self::F64(_) => OcbPhysicalTypeV1::F64,
        }
    }

    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u16(&mut writer, self.physical_type() as u16)?;
        write_u16(&mut writer, 0)?;
        match self {
            Self::I32(value) => write_u64(&mut writer, *value as i64 as u64)?,
            Self::I64(value) => write_u64(&mut writer, *value as u64)?,
            Self::F32(value) => write_u64(&mut writer, value.to_bits() as u64)?,
            Self::F64(value) => write_u64(&mut writer, value.to_bits())?,
        }
        write_u32(&mut writer, 0)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let physical_type = OcbPhysicalTypeV1::from_u16(read_u16(&mut reader)?)?;
        let _reserved0 = read_u16(&mut reader)?;
        let raw = read_u64(&mut reader)?;
        let _reserved1 = read_u32(&mut reader)?;
        Ok(match physical_type {
            OcbPhysicalTypeV1::I32 => Self::I32(raw as i64 as i32),
            OcbPhysicalTypeV1::I64 => Self::I64(raw as i64),
            OcbPhysicalTypeV1::F32 => Self::F32(f32::from_bits(raw as u32)),
            OcbPhysicalTypeV1::F64 => Self::F64(f64::from_bits(raw)),
            OcbPhysicalTypeV1::FixedBinary => {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB fixed-binary columns do not support scalar stats",
                ));
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OcbColumnStatsV1 {
    pub(crate) row_group_id: u32,
    pub(crate) column_id: u32,
    pub(crate) physical_type: OcbPhysicalTypeV1,
    pub(crate) flags: u16,
    pub(crate) null_count: u32,
    pub(crate) min_value: OcbStatScalarV1,
    pub(crate) max_value: OcbStatScalarV1,
}

impl OcbColumnStatsV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        if self.min_value.physical_type() != self.physical_type
            || self.max_value.physical_type() != self.physical_type
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB stats scalar type must match stats physical type",
            ));
        }
        write_u32(&mut writer, self.row_group_id)?;
        write_u32(&mut writer, self.column_id)?;
        write_u16(&mut writer, self.physical_type as u16)?;
        write_u16(&mut writer, self.flags)?;
        write_u32(&mut writer, self.null_count)?;
        self.min_value.write_to(&mut writer)?;
        self.max_value.write_to(&mut writer)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let row_group_id = read_u32(&mut reader)?;
        let column_id = read_u32(&mut reader)?;
        let physical_type = OcbPhysicalTypeV1::from_u16(read_u16(&mut reader)?)?;
        let flags = read_u16(&mut reader)?;
        let null_count = read_u32(&mut reader)?;
        let min_value = OcbStatScalarV1::read_from(&mut reader)?;
        let max_value = OcbStatScalarV1::read_from(&mut reader)?;
        if min_value.physical_type() != physical_type || max_value.physical_type() != physical_type
        {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB stats scalar type does not match stats physical type",
            ));
        }
        Ok(Self {
            row_group_id,
            column_id,
            physical_type,
            flags,
            null_count,
            min_value,
            max_value,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbRowGroupDescV1 {
    pub(crate) row_group_id: u32,
    pub(crate) flags: u32,
    pub(crate) base_row: u64,
    pub(crate) row_count: u64,
    pub(crate) chunk_desc_begin: u64,
    pub(crate) chunk_desc_count: u32,
    pub(crate) stat_begin: u64,
    pub(crate) stat_count: u32,
    pub(crate) first_key_tuple_ref: OcbBodyRefV2,
    pub(crate) last_key_tuple_ref: OcbBodyRefV2,
}

impl OcbRowGroupDescV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u32(&mut writer, self.row_group_id)?;
        write_u32(&mut writer, self.flags)?;
        write_u64(&mut writer, self.base_row)?;
        write_u64(&mut writer, self.row_count)?;
        write_u64(&mut writer, self.chunk_desc_begin)?;
        write_u32(&mut writer, self.chunk_desc_count)?;
        write_u32(&mut writer, 0)?;
        write_u64(&mut writer, self.stat_begin)?;
        write_u32(&mut writer, self.stat_count)?;
        write_u32(&mut writer, 0)?;
        self.first_key_tuple_ref.write_to(&mut writer)?;
        self.last_key_tuple_ref.write_to(&mut writer)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            row_group_id: read_u32(&mut reader)?,
            flags: read_u32(&mut reader)?,
            base_row: read_u64(&mut reader)?,
            row_count: read_u64(&mut reader)?,
            chunk_desc_begin: read_u64(&mut reader)?,
            chunk_desc_count: read_u32(&mut reader)?,
            stat_begin: {
                let _reserved0 = read_u32(&mut reader)?;
                read_u64(&mut reader)?
            },
            stat_count: read_u32(&mut reader)?,
            first_key_tuple_ref: {
                let _reserved1 = read_u32(&mut reader)?;
                OcbBodyRefV2::read_from(&mut reader)?
            },
            last_key_tuple_ref: OcbBodyRefV2::read_from(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbColumnChunkDescV1 {
    pub(crate) row_group_id: u32,
    pub(crate) column_id: u32,
    pub(crate) physical_type: OcbPhysicalTypeV1,
    pub(crate) codec: OcbChunkCodecV1,
    pub(crate) flags: u32,
    pub(crate) value_ref: OcbBodyRefV2,
    pub(crate) validity_ref: OcbBodyRefV2,
    pub(crate) row_count: u64,
    pub(crate) uncompressed_bytes: u64,
}

impl OcbColumnChunkDescV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        if self.value_ref.kind != OcbBodyKindV1::ColumnChunk {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB chunk value_ref must reference column chunk",
            ));
        }
        if !self.validity_ref.is_null() && self.validity_ref.kind != OcbBodyKindV1::ValidityBitmap {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB chunk validity_ref must reference validity bitmap",
            ));
        }
        write_u32(&mut writer, self.row_group_id)?;
        write_u32(&mut writer, self.column_id)?;
        write_u16(&mut writer, self.physical_type as u16)?;
        write_u16(&mut writer, self.codec as u16)?;
        write_u32(&mut writer, self.flags)?;
        self.value_ref.write_to(&mut writer)?;
        self.validity_ref.write_to(&mut writer)?;
        write_u64(&mut writer, self.row_count)?;
        write_u64(&mut writer, self.uncompressed_bytes)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            row_group_id: read_u32(&mut reader)?,
            column_id: read_u32(&mut reader)?,
            physical_type: OcbPhysicalTypeV1::from_u16(read_u16(&mut reader)?)?,
            codec: OcbChunkCodecV1::from_u16(read_u16(&mut reader)?)?,
            flags: read_u32(&mut reader)?,
            value_ref: OcbBodyRefV2::read_from(&mut reader)?,
            validity_ref: OcbBodyRefV2::read_from(&mut reader)?,
            row_count: read_u64(&mut reader)?,
            uncompressed_bytes: read_u64(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OcbRowGroupIndexV1 {
    pub(crate) version: u16,
    pub(crate) flags: u32,
    pub(crate) row_groups: Vec<OcbRowGroupDescV1>,
    pub(crate) column_chunks: Vec<OcbColumnChunkDescV1>,
    pub(crate) stats: Vec<OcbColumnStatsV1>,
    pub(crate) crc32c: u32,
}

impl OcbRowGroupIndexV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ROW_GROUP_INDEX_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB row-group index magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB row-group index version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_ROW_GROUP_INDEX_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB row-group index header length",
            ));
        }
        let row_group_count = read_u32(&mut cursor)?;
        let column_chunk_count = read_u32(&mut cursor)?;
        let stat_count = read_u32(&mut cursor)?;
        let flags = read_u32(&mut cursor)?;
        let _reserved0 = read_u32(&mut cursor)?;

        let row_group_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(row_group_count),
            OCB_ROW_GROUP_DESC_V1_LEN,
            "OCB row-group index row-group count",
        )?;
        let records_start = cursor.position();
        validate_record_sequence(&mut cursor, row_group_count, |reader| {
            OcbRowGroupDescV1::read_from(reader)
        })?;
        let column_chunk_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(column_chunk_count),
            OCB_COLUMN_CHUNK_DESC_V1_LEN,
            "OCB row-group index column-chunk count",
        )?;
        validate_record_sequence(&mut cursor, column_chunk_count, |reader| {
            OcbColumnChunkDescV1::read_from(reader)
        })?;
        let stat_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(stat_count),
            OCB_COLUMN_STATS_V1_LEN,
            "OCB row-group index stat count",
        )?;
        validate_record_sequence(&mut cursor, stat_count, |reader| {
            OcbColumnStatsV1::read_from(reader)
        })?;
        budget.charge(checked_materialized_sum(&[
            checked_materialized_vec_bytes::<OcbRowGroupDescV1>(row_group_count)?,
            checked_materialized_vec_bytes::<OcbColumnChunkDescV1>(column_chunk_count)?,
            checked_materialized_vec_bytes::<OcbColumnStatsV1>(stat_count)?,
        ])?)?;

        cursor.set_position(records_start);
        let row_groups = read_record_sequence(
            &mut cursor,
            row_group_count,
            "OCB row-group allocation failed within resource limit",
            |reader| OcbRowGroupDescV1::read_from(reader),
        )?;
        let column_chunks = read_record_sequence(
            &mut cursor,
            column_chunk_count,
            "OCB column-chunk descriptor allocation failed within resource limit",
            |reader| OcbColumnChunkDescV1::read_from(reader),
        )?;
        let stats = read_record_sequence(
            &mut cursor,
            stat_count,
            "OCB column-stat allocation failed within resource limit",
            |reader| OcbColumnStatsV1::read_from(reader),
        )?;
        Ok(Self {
            version,
            flags,
            row_groups,
            column_chunks,
            stats,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let row_group_count = u32::try_from(self.row_groups.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB row-group index has too many row groups")
        })?;
        let column_chunk_count = u32::try_from(self.column_chunks.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB row-group index has too many column chunks")
        })?;
        let stat_count = u32::try_from(self.stats.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB row-group index has too many stats")
        })?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_ROW_GROUP_INDEX_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ROW_GROUP_INDEX_V1_HEADER_LEN)?;
        write_u32(&mut buf, row_group_count)?;
        write_u32(&mut buf, column_chunk_count)?;
        write_u32(&mut buf, stat_count)?;
        write_u32(&mut buf, self.flags)?;
        write_u32(&mut buf, 0)?;
        for row_group in &self.row_groups {
            row_group.write_to(&mut buf)?;
        }
        for chunk in &self.column_chunks {
            chunk.write_to(&mut buf)?;
        }
        for stat in &self.stats {
            stat.write_to(&mut buf)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct OcbRowGroupIndexDeltaV1 {
    pub(crate) version: u16,
    pub(crate) flags: u32,
    pub(crate) base_row_group_count: u32,
    pub(crate) base_column_chunk_count: u32,
    pub(crate) base_stat_count: u32,
    pub(crate) base_ordering_proof_count: u32,
    pub(crate) row_groups: Vec<OcbRowGroupDescV1>,
    pub(crate) column_chunks: Vec<OcbColumnChunkDescV1>,
    pub(crate) stats: Vec<OcbColumnStatsV1>,
    pub(crate) ordering_keys: Vec<OcbOrderingKeyV1>,
    pub(crate) row_group_ordering_proofs: Vec<OcbRowGroupOrderingProofV1>,
    pub(crate) crc32c: u32,
}

impl OcbRowGroupIndexDeltaV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB row-group index delta crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ROW_GROUP_INDEX_DELTA_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB row-group index delta magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB row-group index delta version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_ROW_GROUP_INDEX_DELTA_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB row-group index delta header length",
            ));
        }
        let base_row_group_count = read_u32(&mut cursor)?;
        let base_column_chunk_count = read_u32(&mut cursor)?;
        let base_stat_count = read_u32(&mut cursor)?;
        let base_ordering_proof_count = read_u32(&mut cursor)?;
        let row_group_count = read_u32(&mut cursor)?;
        let column_chunk_count = read_u32(&mut cursor)?;
        let stat_count = read_u32(&mut cursor)?;
        let ordering_key_count = read_u16(&mut cursor)?;
        let _reserved0 = read_u16(&mut cursor)?;
        let ordering_proof_count = read_u32(&mut cursor)?;
        let flags = read_u32(&mut cursor)?;
        let _reserved1 = read_u32(&mut cursor)?;

        let row_group_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(row_group_count),
            OCB_ROW_GROUP_DESC_V1_LEN,
            "OCB row-group index delta row-group count",
        )?;
        let records_start = cursor.position();
        validate_record_sequence(&mut cursor, row_group_count, |reader| {
            OcbRowGroupDescV1::read_from(reader)
        })?;
        let column_chunk_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(column_chunk_count),
            OCB_COLUMN_CHUNK_DESC_V1_LEN,
            "OCB row-group index delta column-chunk count",
        )?;
        validate_record_sequence(&mut cursor, column_chunk_count, |reader| {
            OcbColumnChunkDescV1::read_from(reader)
        })?;
        let stat_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(stat_count),
            OCB_COLUMN_STATS_V1_LEN,
            "OCB row-group index delta stat count",
        )?;
        validate_record_sequence(&mut cursor, stat_count, |reader| {
            OcbColumnStatsV1::read_from(reader)
        })?;
        let ordering_key_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(ordering_key_count),
            OCB_ORDERING_KEY_V1_LEN,
            "OCB row-group index delta ordering-key count",
        )?;
        validate_record_sequence(&mut cursor, ordering_key_count, |reader| {
            OcbOrderingKeyV1::read_from(reader)
        })?;
        let ordering_proof_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(ordering_proof_count),
            OCB_ROW_GROUP_ORDERING_PROOF_V1_LEN,
            "OCB row-group index delta ordering-proof count",
        )?;
        validate_record_sequence(&mut cursor, ordering_proof_count, |reader| {
            OcbRowGroupOrderingProofV1::read_from(reader)
        })?;
        budget.charge(checked_materialized_sum(&[
            checked_materialized_vec_bytes::<OcbRowGroupDescV1>(row_group_count)?,
            checked_materialized_vec_bytes::<OcbColumnChunkDescV1>(column_chunk_count)?,
            checked_materialized_vec_bytes::<OcbColumnStatsV1>(stat_count)?,
            checked_materialized_vec_bytes::<OcbOrderingKeyV1>(ordering_key_count)?,
            checked_materialized_vec_bytes::<OcbRowGroupOrderingProofV1>(ordering_proof_count)?,
        ])?)?;

        cursor.set_position(records_start);
        let row_groups = read_record_sequence(
            &mut cursor,
            row_group_count,
            "OCB row-group delta allocation failed within resource limit",
            |reader| OcbRowGroupDescV1::read_from(reader),
        )?;
        let column_chunks = read_record_sequence(
            &mut cursor,
            column_chunk_count,
            "OCB column-chunk delta allocation failed within resource limit",
            |reader| OcbColumnChunkDescV1::read_from(reader),
        )?;
        let stats = read_record_sequence(
            &mut cursor,
            stat_count,
            "OCB column-stat delta allocation failed within resource limit",
            |reader| OcbColumnStatsV1::read_from(reader),
        )?;
        let ordering_keys = read_record_sequence(
            &mut cursor,
            ordering_key_count,
            "OCB ordering-key delta allocation failed within resource limit",
            |reader| OcbOrderingKeyV1::read_from(reader),
        )?;
        let row_group_ordering_proofs = read_record_sequence(
            &mut cursor,
            ordering_proof_count,
            "OCB ordering-proof delta allocation failed within resource limit",
            |reader| OcbRowGroupOrderingProofV1::read_from(reader),
        )?;
        Ok(Self {
            version,
            flags,
            base_row_group_count,
            base_column_chunk_count,
            base_stat_count,
            base_ordering_proof_count,
            row_groups,
            column_chunks,
            stats,
            ordering_keys,
            row_group_ordering_proofs,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let row_group_count = u32::try_from(self.row_groups.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB row-group index delta has too many row groups")
        })?;
        let column_chunk_count = u32::try_from(self.column_chunks.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input(
                "OCB row-group index delta has too many column chunks",
            )
        })?;
        let stat_count = u32::try_from(self.stats.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB row-group index delta has too many stats")
        })?;
        let ordering_key_count = u16::try_from(self.ordering_keys.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input(
                "OCB row-group index delta has too many ordering keys",
            )
        })?;
        let ordering_proof_count =
            u32::try_from(self.row_group_ordering_proofs.len()).map_err(|_| {
                ArcadiaTioError::ocb_invalid_input(
                    "OCB row-group index delta has too many ordering proofs",
                )
            })?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_ROW_GROUP_INDEX_DELTA_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ROW_GROUP_INDEX_DELTA_V1_HEADER_LEN)?;
        write_u32(&mut buf, self.base_row_group_count)?;
        write_u32(&mut buf, self.base_column_chunk_count)?;
        write_u32(&mut buf, self.base_stat_count)?;
        write_u32(&mut buf, self.base_ordering_proof_count)?;
        write_u32(&mut buf, row_group_count)?;
        write_u32(&mut buf, column_chunk_count)?;
        write_u32(&mut buf, stat_count)?;
        write_u16(&mut buf, ordering_key_count)?;
        write_u16(&mut buf, 0)?;
        write_u32(&mut buf, ordering_proof_count)?;
        write_u32(&mut buf, self.flags)?;
        write_u32(&mut buf, 0)?;
        for row_group in &self.row_groups {
            row_group.write_to(&mut buf)?;
        }
        for chunk in &self.column_chunks {
            chunk.write_to(&mut buf)?;
        }
        for stat in &self.stats {
            stat.write_to(&mut buf)?;
        }
        for key in &self.ordering_keys {
            key.write_to(&mut buf)?;
        }
        for proof in &self.row_group_ordering_proofs {
            proof.write_to(&mut buf)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbOrderingKeyV1 {
    pub(crate) column_id: u32,
    pub(crate) direction: OcbOrderingDirectionV1,
    pub(crate) null_order: OcbNullOrderV1,
    pub(crate) reserved0: u16,
}

impl OcbOrderingKeyV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        write_u32(&mut writer, self.column_id)?;
        writer.write_all(&[self.direction as u8, self.null_order as u8])?;
        write_u16(&mut writer, self.reserved0)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let column_id = read_u32(&mut reader)?;
        let mut pair = [0u8; 2];
        read_exact_ocb(&mut reader, &mut pair)?;
        Ok(Self {
            column_id,
            direction: OcbOrderingDirectionV1::from_u8(pair[0])?,
            null_order: OcbNullOrderV1::from_u8(pair[1])?,
            reserved0: read_u16(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OcbRowGroupOrderingProofV1 {
    pub(crate) row_group_id: u32,
    pub(crate) flags: u32,
    pub(crate) first_tuple_ref: OcbBodyRefV2,
    pub(crate) last_tuple_ref: OcbBodyRefV2,
}

impl OcbRowGroupOrderingProofV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        if !self.first_tuple_ref.is_null() && self.first_tuple_ref.kind != OcbBodyKindV1::KeyTuple {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB ordering first_tuple_ref must reference key tuple",
            ));
        }
        if !self.last_tuple_ref.is_null() && self.last_tuple_ref.kind != OcbBodyKindV1::KeyTuple {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB ordering last_tuple_ref must reference key tuple",
            ));
        }
        write_u32(&mut writer, self.row_group_id)?;
        write_u32(&mut writer, self.flags)?;
        self.first_tuple_ref.write_to(&mut writer)?;
        self.last_tuple_ref.write_to(&mut writer)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        Ok(Self {
            row_group_id: read_u32(&mut reader)?,
            flags: read_u32(&mut reader)?,
            first_tuple_ref: OcbBodyRefV2::read_from(&mut reader)?,
            last_tuple_ref: OcbBodyRefV2::read_from(&mut reader)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbOrderingProofV1 {
    pub(crate) version: u16,
    pub(crate) flags: u16,
    pub(crate) keys: Vec<OcbOrderingKeyV1>,
    pub(crate) row_group_proofs: Vec<OcbRowGroupOrderingProofV1>,
    pub(crate) crc32c: u32,
}

impl OcbOrderingProofV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(bytes: Vec<u8>) -> Result<Self> {
        let mut budget = policy_a_metadata_materialization_budget();
        Self::read_from_bytes_with_budget(bytes, &mut budget)
    }

    pub(crate) fn read_from_bytes_with_budget(
        mut bytes: Vec<u8>,
        budget: &mut MetadataMaterializationBudget,
    ) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB ordering proof crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_ORDERING_PROOF_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB ordering proof magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB ordering proof version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_ORDERING_PROOF_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB ordering proof header length",
            ));
        }
        let key_count = read_u16(&mut cursor)?;
        let flags = read_u16(&mut cursor)?;
        let row_group_count = read_u32(&mut cursor)?;
        let _reserved0 = read_u64(&mut cursor)?;
        let key_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(key_count),
            OCB_ORDERING_KEY_V1_LEN,
            "OCB ordering proof key count",
        )?;
        let records_start = cursor.position();
        validate_record_sequence(&mut cursor, key_count, |reader| {
            OcbOrderingKeyV1::read_from(reader)
        })?;
        let row_group_count = checked_record_count(
            bytes.len(),
            cursor.position(),
            u64::from(row_group_count),
            OCB_ROW_GROUP_ORDERING_PROOF_V1_LEN,
            "OCB ordering proof row-group count",
        )?;
        validate_record_sequence(&mut cursor, row_group_count, |reader| {
            OcbRowGroupOrderingProofV1::read_from(reader)
        })?;
        budget.charge(checked_materialized_sum(&[
            checked_materialized_vec_bytes::<OcbOrderingKeyV1>(key_count)?,
            checked_materialized_vec_bytes::<OcbRowGroupOrderingProofV1>(row_group_count)?,
        ])?)?;

        cursor.set_position(records_start);
        let keys = read_record_sequence(
            &mut cursor,
            key_count,
            "OCB ordering-key allocation failed within resource limit",
            |reader| OcbOrderingKeyV1::read_from(reader),
        )?;
        let row_group_proofs = read_record_sequence(
            &mut cursor,
            row_group_count,
            "OCB ordering-proof allocation failed within resource limit",
            |reader| OcbRowGroupOrderingProofV1::read_from(reader),
        )?;
        Ok(Self {
            version,
            flags,
            keys,
            row_group_proofs,
            crc32c: actual_crc,
        })
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let key_count = u16::try_from(self.keys.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB ordering proof has too many keys")
        })?;
        let row_group_count = u32::try_from(self.row_group_proofs.len()).map_err(|_| {
            ArcadiaTioError::ocb_invalid_input("OCB ordering proof has too many row groups")
        })?;
        let mut buf = Vec::new();
        buf.extend_from_slice(&OCB_ORDERING_PROOF_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_ORDERING_PROOF_V1_HEADER_LEN)?;
        write_u16(&mut buf, key_count)?;
        write_u16(&mut buf, self.flags)?;
        write_u32(&mut buf, row_group_count)?;
        write_u64(&mut buf, 0)?;
        for key in &self.keys {
            key.write_to(&mut buf)?;
        }
        for proof in &self.row_group_proofs {
            proof.write_to(&mut buf)?;
        }
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OcbColumnChunkObjectV1 {
    pub(crate) version: u16,
    pub(crate) physical_type: OcbPhysicalTypeV1,
    pub(crate) codec: OcbChunkCodecV1,
    pub(crate) flags: u32,
    pub(crate) row_group_id: u32,
    pub(crate) column_id: u32,
    pub(crate) row_count: u64,
    pub(crate) uncompressed_bytes: u64,
    pub(crate) payload: Vec<u8>,
    pub(crate) crc32c: u32,
}

impl OcbColumnChunkObjectV1 {
    pub(crate) fn write_to<W: Write>(&self, mut writer: W) -> Result<()> {
        let mut buf = self.encode_without_checksum()?;
        let checksum = crc32c(&buf);
        write_u32_at_end(&mut buf, checksum);
        writer.write_all(&buf)?;
        Ok(())
    }

    pub(crate) fn read_from<R: Read>(mut reader: R) -> Result<Self> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        Self::read_from_bytes(bytes)
    }

    pub(crate) fn read_from_bytes(mut bytes: Vec<u8>) -> Result<Self> {
        let actual_crc = read_u32_at_end(&bytes)?;
        write_u32_at_end(&mut bytes, 0);
        let expected_crc = crc32c(&bytes);
        if actual_crc != expected_crc {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB column chunk crc mismatch",
            ));
        }
        let mut cursor = Cursor::new(bytes.as_slice());
        let mut magic = [0u8; 8];
        read_exact_ocb(&mut cursor, &mut magic)?;
        if magic != OCB_COLUMN_CHUNK_MAGIC_V1 {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB column chunk magic",
            ));
        }
        let version = read_u16(&mut cursor)?;
        if version != 1 {
            return Err(ArcadiaTioError::ocb_unsupported_format(
                "unsupported OCB column chunk version",
            ));
        }
        let header_bytes = read_u16(&mut cursor)?;
        if header_bytes != OCB_COLUMN_CHUNK_V1_HEADER_LEN {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "invalid OCB column chunk header length",
            ));
        }
        let physical_type = OcbPhysicalTypeV1::from_u16(read_u16(&mut cursor)?)?;
        let codec = OcbChunkCodecV1::from_u16(read_u16(&mut cursor)?)?;
        let flags = read_u32(&mut cursor)?;
        let row_group_id = read_u32(&mut cursor)?;
        let column_id = read_u32(&mut cursor)?;
        let row_count = read_u64(&mut cursor)?;
        let value_bytes = read_u64(&mut cursor)?;
        if let Some(byte_width) = physical_type.primitive_byte_width() {
            let expected_value_bytes = row_count.checked_mul(byte_width as u64).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB column chunk value byte length overflows"),
            )?;
            if value_bytes != expected_value_bytes {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB column chunk value byte length does not match row count and physical type",
                ));
            }
        }
        let payload_len = remaining_before_crc(bytes.len(), cursor.position())?;
        if codec == OcbChunkCodecV1::None && payload_len as u64 != value_bytes {
            return Err(ArcadiaTioError::ocb_corrupt_file(
                "OCB uncompressed column chunk value byte length does not match header",
            ));
        }
        let payload_start = usize::try_from(cursor.position()).map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file(
                "OCB column chunk payload offset exceeds addressable memory",
            )
        })?;
        let payload_end =
            payload_start
                .checked_add(payload_len)
                .ok_or(ArcadiaTioError::ocb_corrupt_file(
                    "OCB column chunk payload range overflows",
                ))?;
        drop(cursor);
        bytes.copy_within(payload_start..payload_end, 0);
        bytes.truncate(payload_len);
        Ok(Self {
            version,
            physical_type,
            codec,
            flags,
            row_group_id,
            column_id,
            row_count,
            uncompressed_bytes: value_bytes,
            payload: bytes,
            crc32c: actual_crc,
        })
    }

    pub(crate) fn uncompressed_value_bytes(&self) -> Result<u64> {
        if let Some(byte_width) = self.physical_type.primitive_byte_width() {
            let expected = self.row_count.checked_mul(byte_width as u64).ok_or(
                ArcadiaTioError::ocb_corrupt_file("OCB column chunk value byte length overflows"),
            )?;
            if self.uncompressed_bytes != expected {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB column chunk value byte length does not match row count and physical type",
                ));
            }
        }
        Ok(self.uncompressed_bytes)
    }

    pub(crate) fn decode_payload(&self) -> Result<Vec<u8>> {
        self.decode_payload_with_limits(
            OCB_POLICY_A_MAX_COMPRESSED_CHUNK_BYTES,
            OCB_POLICY_A_MAX_DECOMPRESSED_CHUNK_BYTES,
        )
    }

    pub(crate) fn decode_payload_with_limits(
        &self,
        max_compressed_bytes: u64,
        max_decompressed_bytes: u64,
    ) -> Result<Vec<u8>> {
        let expected_value_bytes = self.uncompressed_value_bytes()?;
        match self.codec {
            OcbChunkCodecV1::None => {
                if self.payload.len() as u64 != expected_value_bytes {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB uncompressed column chunk value byte length does not match header",
                    ));
                }
                let encoded_bytes = u64::try_from(self.payload.len()).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file(
                        "OCB uncompressed column chunk payload exceeds addressable length",
                    )
                })?;
                if encoded_bytes > max_compressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB encoded column chunk exceeds resource limit",
                    ));
                }
                if expected_value_bytes > max_decompressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB decoded column chunk exceeds resource limit",
                    ));
                }
                copy_bytes_fallibly(
                    &self.payload,
                    "OCB decoded payload allocation failed within resource limit",
                )
            }
            OcbChunkCodecV1::Zstd => {
                if self.payload.is_empty() {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB zstd column chunk payload must not be empty",
                    ));
                }
                let compressed_bytes = u64::try_from(self.payload.len()).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file(
                        "OCB zstd column chunk payload exceeds addressable length",
                    )
                })?;
                if compressed_bytes > max_compressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB compressed column chunk exceeds resource limit",
                    ));
                }
                decode_zstd_payload_bounded(
                    &self.payload,
                    expected_value_bytes,
                    max_decompressed_bytes,
                )
            }
        }
    }

    pub(crate) fn into_decoded_payload_with_limits(
        self,
        max_compressed_bytes: u64,
        max_decompressed_bytes: u64,
    ) -> Result<Vec<u8>> {
        let expected_value_bytes = self.uncompressed_value_bytes()?;
        match self.codec {
            OcbChunkCodecV1::None => {
                if self.payload.len() as u64 != expected_value_bytes {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB uncompressed column chunk value byte length does not match header",
                    ));
                }
                let encoded_bytes = u64::try_from(self.payload.len()).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file(
                        "OCB uncompressed column chunk payload exceeds addressable length",
                    )
                })?;
                if encoded_bytes > max_compressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB encoded column chunk exceeds resource limit",
                    ));
                }
                if expected_value_bytes > max_decompressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB decoded column chunk exceeds resource limit",
                    ));
                }
                Ok(self.payload)
            }
            OcbChunkCodecV1::Zstd => {
                if self.payload.is_empty() {
                    return Err(ArcadiaTioError::ocb_corrupt_file(
                        "OCB zstd column chunk payload must not be empty",
                    ));
                }
                let compressed_bytes = u64::try_from(self.payload.len()).map_err(|_| {
                    ArcadiaTioError::ocb_corrupt_file(
                        "OCB zstd column chunk payload exceeds addressable length",
                    )
                })?;
                if compressed_bytes > max_compressed_bytes {
                    return Err(ArcadiaTioError::ocb_invalid_input(
                        "OCB compressed column chunk exceeds resource limit",
                    ));
                }
                decode_zstd_payload_bounded(
                    &self.payload,
                    expected_value_bytes,
                    max_decompressed_bytes,
                )
            }
        }
    }

    fn encode_without_checksum(&self) -> Result<Vec<u8>> {
        let expected_value_bytes = self.uncompressed_bytes;
        if let Some(byte_width) = self.physical_type.primitive_byte_width() {
            let expected = self.row_count.checked_mul(byte_width as u64).ok_or(
                ArcadiaTioError::ocb_invalid_input("OCB column chunk value byte length overflows"),
            )?;
            if expected_value_bytes != expected {
                return Err(ArcadiaTioError::ocb_invalid_input(
                    "OCB column chunk value byte length does not match row count and physical type",
                ));
            }
        }
        if self.codec == OcbChunkCodecV1::None && self.payload.len() as u64 != expected_value_bytes
        {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB column chunk payload length does not match row count and physical type",
            ));
        }
        if self.codec == OcbChunkCodecV1::Zstd && self.payload.is_empty() {
            return Err(ArcadiaTioError::ocb_invalid_input(
                "OCB zstd column chunk payload must not be empty",
            ));
        }
        let mut buf =
            Vec::with_capacity(OCB_COLUMN_CHUNK_V1_HEADER_LEN as usize + self.payload.len() + 4);
        buf.extend_from_slice(&OCB_COLUMN_CHUNK_MAGIC_V1);
        write_u16(&mut buf, self.version)?;
        write_u16(&mut buf, OCB_COLUMN_CHUNK_V1_HEADER_LEN)?;
        write_u16(&mut buf, self.physical_type as u16)?;
        write_u16(&mut buf, self.codec as u16)?;
        write_u32(&mut buf, self.flags)?;
        write_u32(&mut buf, self.row_group_id)?;
        write_u32(&mut buf, self.column_id)?;
        write_u64(&mut buf, self.row_count)?;
        write_u64(&mut buf, expected_value_bytes)?;
        buf.extend_from_slice(&self.payload);
        write_u32(&mut buf, 0)?;
        Ok(buf)
    }
}

fn decode_zstd_payload_bounded(
    encoded: &[u8],
    expected_bytes: u64,
    max_decoded_bytes: u64,
) -> Result<Vec<u8>> {
    expected_bytes
        .checked_add(1)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB decoded column chunk byte length overflows",
        ))?;
    let expected_len = usize::try_from(expected_bytes).map_err(|_| {
        ArcadiaTioError::ocb_corrupt_file(
            "OCB decoded column chunk byte length exceeds addressable memory",
        )
    })?;
    if expected_bytes > max_decoded_bytes {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB decoded column chunk exceeds resource limit",
        ));
    }

    let mut decoded = allocate_zeroed_bytes_fallibly(
        expected_len,
        "OCB decoded payload allocation failed within resource limit",
    )?;
    let mut decoder = zstd::stream::read::Decoder::new(Cursor::new(encoded))
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB zstd column chunk decode failed"))?;
    decoder
        .window_log_max(zstd_window_log_for_decoded_limit(max_decoded_bytes))
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB zstd column chunk decode failed"))?;
    if !decoded.is_empty() {
        decoder.read_exact(&mut decoded).map_err(|_| {
            ArcadiaTioError::ocb_corrupt_file("OCB zstd column chunk decode failed")
        })?;
    }
    let mut extra = [0u8; 1];
    match decoder.read(&mut extra) {
        Ok(0) => Ok(decoded),
        Ok(_) => Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB zstd column chunk decoded byte length does not match header",
        )),
        Err(_) => Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB zstd column chunk decode failed",
        )),
    }
}

fn zstd_window_log_for_decoded_limit(max_decoded_bytes: u64) -> u32 {
    let floor_log = if max_decoded_bytes == 0 {
        0
    } else {
        u64::BITS - 1 - max_decoded_bytes.leading_zeros()
    };
    floor_log.clamp(
        OCB_ZSTD_WRITER_COMPAT_WINDOW_LOG,
        OCB_ZSTD_PLATFORM_MAX_WINDOW_LOG,
    )
}

fn copy_bytes_fallibly(bytes: &[u8], allocation_message: &'static str) -> Result<Vec<u8>> {
    let mut copy = Vec::new();
    copy.try_reserve_exact(bytes.len())
        .map_err(|_| ocb_allocation_error(allocation_message))?;
    copy.extend_from_slice(bytes);
    Ok(copy)
}

fn try_vec_with_capacity<T>(capacity: usize, allocation_message: &'static str) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| ocb_allocation_error(allocation_message))?;
    Ok(values)
}

fn allocate_zeroed_bytes_fallibly(len: usize, allocation_message: &'static str) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(len)
        .map_err(|_| ocb_allocation_error(allocation_message))?;
    bytes.resize(len, 0);
    Ok(bytes)
}

fn ocb_allocation_error(message: &'static str) -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::new(ErrorKind::OutOfMemory, message))
}

fn write_u16<W: Write>(writer: &mut W, value: u16) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_i32<W: Write>(writer: &mut W, value: i32) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u64<W: Write>(writer: &mut W, value: u64) -> Result<()> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn read_exact_ocb<R: Read>(reader: &mut R, buf: &mut [u8]) -> Result<()> {
    reader.read_exact(buf).map_err(|err| {
        if err.kind() == ErrorKind::UnexpectedEof {
            ArcadiaTioError::ocb_corrupt_file("OCB object is truncated")
        } else {
            ArcadiaTioError::Io(err)
        }
    })
}

fn read_u16<R: Read>(reader: &mut R) -> Result<u16> {
    let mut buf = [0u8; 2];
    read_exact_ocb(reader, &mut buf)?;
    Ok(u16::from_le_bytes(buf))
}

fn read_u32<R: Read>(reader: &mut R) -> Result<u32> {
    let mut buf = [0u8; 4];
    read_exact_ocb(reader, &mut buf)?;
    Ok(u32::from_le_bytes(buf))
}

fn read_i32<R: Read>(reader: &mut R) -> Result<i32> {
    let mut buf = [0u8; 4];
    read_exact_ocb(reader, &mut buf)?;
    Ok(i32::from_le_bytes(buf))
}

fn read_u64<R: Read>(reader: &mut R) -> Result<u64> {
    let mut buf = [0u8; 8];
    read_exact_ocb(reader, &mut buf)?;
    Ok(u64::from_le_bytes(buf))
}

fn write_bytes_u32<W: Write>(writer: &mut W, bytes: &[u8]) -> Result<()> {
    let len = u32::try_from(bytes.len())
        .map_err(|_| ArcadiaTioError::ocb_invalid_input("OCB byte payload length exceeds u32"))?;
    write_u32(writer, len)?;
    writer.write_all(bytes)?;
    Ok(())
}

fn read_bytes_u32<R: Read>(reader: &mut R) -> Result<Vec<u8>> {
    let len = read_u32(reader)? as usize;
    let mut bytes = allocate_zeroed_bytes_fallibly(
        len,
        "OCB byte-payload allocation failed within resource limit",
    )?;
    read_exact_ocb(reader, &mut bytes)?;
    Ok(bytes)
}

fn write_string_u32<W: Write>(writer: &mut W, value: &str) -> Result<()> {
    write_bytes_u32(writer, value.as_bytes())
}

fn read_string_u32_bounded(reader: &mut Cursor<&[u8]>, total_len: usize) -> Result<String> {
    let _ = checked_payload_len(total_len, reader.position(), 4, "OCB string byte length")?;
    let len = read_u32(reader)?;
    let len = checked_payload_len(
        total_len,
        reader.position(),
        u64::from(len),
        "OCB string byte length",
    )?;
    let mut bytes =
        allocate_zeroed_bytes_fallibly(len, "OCB string allocation failed within resource limit")?;
    read_exact_ocb(reader, &mut bytes)?;
    String::from_utf8(bytes)
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB string is not valid UTF-8"))
}

fn inspect_string_u32_bounded(reader: &mut Cursor<&[u8]>, total_len: usize) -> Result<usize> {
    let _ = checked_payload_len(total_len, reader.position(), 4, "OCB string byte length")?;
    let len = read_u32(reader)?;
    let len = checked_payload_len(
        total_len,
        reader.position(),
        u64::from(len),
        "OCB string byte length",
    )?;
    let start = usize::try_from(reader.position())
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB string offset exceeds usize"))?;
    let end = start
        .checked_add(len)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB string byte range overflows",
        ))?;
    std::str::from_utf8(&reader.get_ref()[start..end])
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB string is not valid UTF-8"))?;
    reader.set_position(end as u64);
    Ok(len)
}

fn validate_record_sequence<T>(
    reader: &mut Cursor<&[u8]>,
    count: usize,
    mut read_record: impl FnMut(&mut Cursor<&[u8]>) -> Result<T>,
) -> Result<()> {
    for _ in 0..count {
        let _ = read_record(reader)?;
    }
    Ok(())
}

fn read_record_sequence<T>(
    reader: &mut Cursor<&[u8]>,
    count: usize,
    allocation_message: &'static str,
    mut read_record: impl FnMut(&mut Cursor<&[u8]>) -> Result<T>,
) -> Result<Vec<T>> {
    let mut records = try_vec_with_capacity(count, allocation_message)?;
    for _ in 0..count {
        records.push(read_record(reader)?);
    }
    Ok(records)
}

fn checked_materialized_vec_bytes<T>(count: usize) -> Result<u64> {
    let count = u64::try_from(count)
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB metadata vector count exceeds u64"))?;
    let item_bytes = u64::try_from(std::mem::size_of::<T>())
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB metadata record size exceeds u64"))?;
    count
        .checked_mul(item_bytes)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB metadata materialized byte count overflows",
        ))
}

fn checked_materialized_sum(parts: &[u64]) -> Result<u64> {
    parts.iter().try_fold(0u64, |total, part| {
        total
            .checked_add(*part)
            .ok_or(ArcadiaTioError::ocb_corrupt_file(
                "OCB metadata materialized byte count overflows",
            ))
    })
}

fn policy_a_metadata_materialization_budget() -> MetadataMaterializationBudget {
    MetadataMaterializationBudget::from_limits(OcbResourceLimits::policy_a())
}

fn checked_record_count(
    total_len: usize,
    position: u64,
    count: u64,
    record_len: usize,
    what: &'static str,
) -> Result<usize> {
    let count = usize::try_from(count)
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB record count exceeds usize"))?;
    let needed = count
        .checked_mul(record_len)
        .ok_or(ArcadiaTioError::ocb_corrupt_file(
            "OCB record byte count overflows",
        ))?;
    let remaining = remaining_before_crc(total_len, position)?;
    if needed > remaining {
        return Err(ArcadiaTioError::ocb_corrupt_file(what));
    }
    Ok(count)
}

fn checked_payload_len(
    total_len: usize,
    position: u64,
    len: u64,
    what: &'static str,
) -> Result<usize> {
    let len = usize::try_from(len)
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB payload length exceeds usize"))?;
    let remaining = remaining_before_crc(total_len, position)?;
    if len > remaining {
        return Err(ArcadiaTioError::ocb_corrupt_file(what));
    }
    Ok(len)
}

fn remaining_before_crc(total_len: usize, position: u64) -> Result<usize> {
    let position = usize::try_from(position)
        .map_err(|_| ArcadiaTioError::ocb_corrupt_file("OCB cursor position exceeds usize"))?;
    let payload_end = total_len
        .checked_sub(4)
        .ok_or(ArcadiaTioError::ocb_corrupt_file("OCB object is too short"))?;
    if position > payload_end {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "OCB cursor advanced beyond object payload",
        ));
    }
    Ok(payload_end - position)
}

fn read_u32_at_end(bytes: &[u8]) -> Result<u32> {
    let len = bytes.len();
    if len < 4 {
        return Err(ArcadiaTioError::ocb_corrupt_file(
            "buffer too short for trailing crc",
        ));
    }
    Ok(u32::from_le_bytes([
        bytes[len - 4],
        bytes[len - 3],
        bytes[len - 2],
        bytes[len - 1],
    ]))
}

fn write_u32_at_end(bytes: &mut [u8], value: u32) {
    let len = bytes.len();
    bytes[len - 4..].copy_from_slice(&value.to_le_bytes());
}

const CRC32C_POLY: u32 = 0x82F63B78;
const CRC32C_TABLE: [u32; 256] = build_crc32c_table();

const fn build_crc32c_table() -> [u32; 256] {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut crc = i as u32;
        let mut j = 0;
        while j < 8 {
            if (crc & 1) != 0 {
                crc = (crc >> 1) ^ CRC32C_POLY;
            } else {
                crc >>= 1;
            }
            j += 1;
        }
        table[i] = crc;
        i += 1;
    }
    table
}

pub(crate) const fn crc32c_init() -> u32 {
    0xFFFF_FFFFu32
}

pub(crate) fn crc32c_update(mut state: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        let idx = ((state ^ byte as u32) & 0xFF) as usize;
        state = CRC32C_TABLE[idx] ^ (state >> 8);
    }
    state
}

pub(crate) const fn crc32c_finish(state: u32) -> u32 {
    !state
}

pub(crate) fn crc32c(bytes: &[u8]) -> u32 {
    crc32c_finish(crc32c_update(crc32c_init(), bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_ref(kind: OcbBodyKindV1, offset: u64, length: u64) -> OcbBodyRefV2 {
        OcbBodyRefV2::new(offset, length, kind, 0xABCD_1234)
    }

    fn write_object<T>(mut f: impl FnMut(&mut Vec<u8>) -> Result<T>) -> Vec<u8> {
        let mut buf = Vec::new();
        f(&mut buf).expect("write object");
        buf
    }

    fn refresh_crc(bytes: &mut [u8]) {
        write_u32_at_end(bytes, 0);
        let checksum = crc32c(bytes);
        write_u32_at_end(bytes, checksum);
    }

    fn root_v2(
        generation: u64,
        previous_generation: u64,
        previous_root_ref: OcbBodyRefV2,
    ) -> OcbRootV2 {
        OcbRootV2 {
            version: OCB_FORMAT_MAJOR_V2,
            flags: 0,
            generation,
            previous_generation,
            previous_root_ref,
            append_base_row: 6,
            append_row_count: 3,
            append_base_row_group: 2,
            append_row_group_count: 1,
            row_count: 9,
            column_count: 2,
            row_group_count: 3,
            dictionary_count: 0,
            column_chunk_count: 6,
            schema_ref: body_ref(OcbBodyKindV1::Schema, 4096, 128),
            dictionary_index_ref: OcbBodyRefV2::NULL,
            row_group_index_ref: body_ref(OcbBodyKindV1::RowGroupIndex, 8192, 256),
            ordering_proof_ref: body_ref(OcbBodyKindV1::OrderingProof, 12_288, 96),
            debug_json_ref: OcbBodyRefV2::NULL,
            first_key_tuple_ref: OcbBodyRefV2::NULL,
            last_key_tuple_ref: OcbBodyRefV2::NULL,
            append_first_key_tuple_ref: OcbBodyRefV2::NULL,
            append_last_key_tuple_ref: OcbBodyRefV2::NULL,
            commit_diagnostics_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 123,
            content_flags: 0,
            crc32c: 0,
        }
    }

    #[test]
    fn body_ref_roundtrip_and_validation() {
        let reference = body_ref(OcbBodyKindV1::Schema, 4096, 128);
        let mut bytes = Vec::new();
        reference.write_to(&mut bytes).expect("write body ref");
        assert_eq!(bytes.len(), OCB_BODY_REF_V2_LEN);
        let decoded = OcbBodyRefV2::read_from(Cursor::new(bytes)).expect("read body ref");
        assert_eq!(decoded, reference);
        decoded
            .validate(OcbBodyKindV1::Schema, 5000)
            .expect("valid range");
        assert!(
            decoded
                .validate(OcbBodyKindV1::Root, 5000)
                .expect_err("kind mismatch")
                .to_string()
                .contains("kind mismatch")
        );
        assert!(
            decoded
                .validate(OcbBodyKindV1::Schema, 4100)
                .expect_err("out of range")
                .to_string()
                .contains("beyond file length")
        );
    }

    #[test]
    fn bootstrap_roundtrip_validates_crc_and_root_kind() {
        let root_ref = body_ref(OcbBodyKindV1::Root, 8192, OCB_ROOT_V1_LEN as u64);
        let bootstrap = OcbBootstrapPageV1::new([7u8; 16], root_ref);
        let mut bytes = Vec::new();
        bootstrap.write_to(&mut bytes).expect("write bootstrap");
        assert_eq!(bytes.len(), OCB_BOOTSTRAP_PAGE_V1_LEN);

        let decoded =
            OcbBootstrapPageV1::read_from(Cursor::new(bytes.clone())).expect("read bootstrap");
        assert_eq!(decoded.format_major, OCB_FORMAT_MAJOR_V1);
        assert_eq!(decoded.root_ref, root_ref);
        assert_ne!(decoded.crc32c, 0);

        bytes[32] ^= 0x01;
        assert!(
            OcbBootstrapPageV1::read_from(Cursor::new(bytes))
                .expect_err("crc catches mutation")
                .to_string()
                .contains("crc mismatch")
        );

        let bad = OcbBootstrapPageV1::new(
            [0u8; 16],
            body_ref(OcbBodyKindV1::Schema, 8192, OCB_ROOT_V1_LEN as u64),
        );
        assert!(
            bad.write_to(Vec::new())
                .expect_err("bad root kind")
                .to_string()
                .contains("root_ref")
        );
    }

    #[test]
    fn v2_root_slot_and_bootstrap_roundtrip() {
        let previous_ref = body_ref(OcbBodyKindV1::Root, 7000, OCB_ROOT_V2_LEN as u64);
        let root_ref = body_ref(OcbBodyKindV1::Root, 8192, OCB_ROOT_V2_LEN as u64);
        let slot0 = OcbRootSlotV2::new(0, 7, root_ref, 6, previous_ref, OcbBodyRefV2::NULL);
        let slot1 = OcbRootSlotV2::empty(1);
        let bootstrap = OcbBootstrapPageV2::new([3u8; 16], [slot0.clone(), slot1.clone()])
            .expect("build v2 bootstrap");
        let mut bytes = Vec::new();
        bootstrap.write_to(&mut bytes).expect("write v2 bootstrap");
        assert_eq!(bytes.len(), OCB_BOOTSTRAP_PAGE_V2_LEN);

        let decoded =
            OcbBootstrapPageV2::read_from(Cursor::new(bytes.clone())).expect("read v2 bootstrap");
        assert_eq!(decoded.format_major, OCB_FORMAT_MAJOR_V2);
        assert_eq!(decoded.file_uuid, [3u8; 16]);
        assert_ne!(decoded.crc32c, 0);
        let slots = decoded.decoded_root_slots();
        let decoded_slot0 = slots[0].as_ref().expect("decode slot 0");
        assert_eq!(decoded_slot0.generation, 7);
        assert_eq!(decoded_slot0.root_ref, root_ref);
        decoded_slot0
            .validate_candidate(0, 20_000)
            .expect("slot candidate validates");
        decoded_slot0
            .validate_root(&root_v2(7, 6, previous_ref))
            .expect("slot/root generation metadata matches");
        assert!(slots[1].as_ref().expect("decode slot 1").is_empty());

        bytes[OCB_ROOT_SLOT_TABLE_V2_OFFSET + 24] ^= 0x01;
        let decoded = OcbBootstrapPageV2::read_from(Cursor::new(bytes))
            .expect("bootstrap checksum excludes mutable slots");
        assert!(decoded.decoded_root_slots()[0].is_err());
    }

    #[test]
    fn v2_root_roundtrip_and_generation_validation() {
        let previous_ref = body_ref(OcbBodyKindV1::Root, 7000, OCB_ROOT_V2_LEN as u64);
        let root = root_v2(5, 4, previous_ref);
        let bytes = write_object(|buf| root.write_to(buf));
        assert_eq!(bytes.len(), OCB_ROOT_V2_LEN as usize);
        let decoded = OcbRootV2::read_from(Cursor::new(bytes)).expect("read v2 root");
        assert_eq!(decoded.generation, 5);
        assert_eq!(decoded.previous_generation, 4);
        assert_eq!(decoded.append_base_row, 6);
        assert_eq!(decoded.append_row_count, 3);
        decoded
            .validate_references(20_000)
            .expect("v2 root references are in bounds");
        let v1_authority = decoded.to_v1_root();
        assert_eq!(v1_authority.row_count, 9);
        assert_eq!(v1_authority.row_group_count, 3);

        let bad = OcbRootV2 {
            append_base_row: 8,
            ..root.clone()
        };
        assert!(
            bad.write_to(Vec::new())
                .expect_err("append range exceeds total rows")
                .to_string()
                .contains("append row range")
        );

        let bad_ref = OcbRootV2 {
            schema_ref: body_ref(OcbBodyKindV1::ColumnChunk, 4096, 128),
            ..root
        };
        assert!(
            bad_ref
                .write_to(Vec::new())
                .expect_err("schema ref kind is required")
                .to_string()
                .contains("schema_ref")
        );
    }

    #[test]
    fn unsupported_versions_are_rejected_on_read() {
        let root_ref = body_ref(OcbBodyKindV1::Root, 8192, OCB_ROOT_V1_LEN as u64);
        let bootstrap = OcbBootstrapPageV1 {
            format_major: 99,
            ..OcbBootstrapPageV1::new([7u8; 16], root_ref)
        };
        let mut bytes = Vec::new();
        bootstrap.write_to(&mut bytes).expect("write bootstrap");
        let err = OcbBootstrapPageV1::read_from(Cursor::new(bytes))
            .expect_err("unsupported bootstrap version");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::UnsupportedFormat)
        );
        assert!(
            err.to_string()
                .contains("unsupported OCB bootstrap format version")
        );

        let strings = OcbStringTableV1 {
            version: 99,
            strings: vec!["column".into()],
            crc32c: 0,
        };
        let bytes = write_object(|buf| strings.write_to(buf));
        let err = OcbStringTableV1::read_from(Cursor::new(bytes))
            .expect_err("unsupported object version");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::UnsupportedFormat)
        );
        assert!(
            err.to_string()
                .contains("unsupported OCB string table version")
        );
    }

    #[test]
    fn object_readers_classify_short_bodies_as_corrupt_ocb() {
        let err = OcbBootstrapPageV1::read_from(Cursor::new(vec![0u8; 16]))
            .expect_err("short bootstrap is corrupt OCB");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert!(err.to_string().contains("truncated"));
    }

    #[test]
    fn object_readers_reject_counts_that_exceed_object_body() {
        let strings = OcbStringTableV1 {
            version: 1,
            strings: vec!["column".into()],
            crc32c: 0,
        };
        let mut bytes = write_object(|buf| strings.write_to(buf));
        bytes[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        refresh_crc(&mut bytes);
        let err = OcbStringTableV1::read_from(Cursor::new(bytes))
            .expect_err("string count beyond object body rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert!(err.to_string().contains("string count"));

        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::I32,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id: 0,
            column_id: 0,
            row_count: 1,
            uncompressed_bytes: 4,
            payload: 1_i32.to_le_bytes().to_vec(),
            crc32c: 0,
        };
        let mut bytes = write_object(|buf| chunk.write_to(buf));
        bytes[28..36].copy_from_slice(&1_000_000_000_u64.to_le_bytes());
        bytes[36..44].copy_from_slice(&4_000_000_000_u64.to_le_bytes());
        refresh_crc(&mut bytes);
        let err = OcbColumnChunkObjectV1::read_from(Cursor::new(bytes))
            .expect_err("chunk value bytes beyond object body rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert!(err.to_string().contains("value byte length"));
    }

    #[test]
    fn schema_and_dictionary_metadata_roundtrip() {
        let strings = OcbStringTableV1 {
            version: 1,
            strings: vec![
                "event_kind_code".into(),
                "symbol_code".into(),
                "symbols".into(),
            ],
            crc32c: 0,
        };
        let string_bytes = write_object(|buf| strings.write_to(buf));
        let decoded_strings =
            OcbStringTableV1::read_from(Cursor::new(string_bytes)).expect("read string table");
        assert_eq!(decoded_strings.strings[1], "symbol_code");

        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref: body_ref(OcbBodyKindV1::StringTable, 4096, 128),
            columns: vec![
                OcbColumnDescV1 {
                    column_id: 0,
                    name_string_id: 0,
                    physical_type: OcbPhysicalTypeV1::I32,
                    logical_kind: OcbLogicalKindV1::EnumCode,
                    flags: 0,
                    dictionary_id: OCB_NULL_U32,
                    scale: 0,
                    nullability: OcbNullabilityV1::NonNull,
                    reserved0: 0,
                    fixed_binary_width: 0,
                },
                OcbColumnDescV1 {
                    column_id: 1,
                    name_string_id: 1,
                    physical_type: OcbPhysicalTypeV1::I32,
                    logical_kind: OcbLogicalKindV1::DictionaryCode,
                    flags: 0,
                    dictionary_id: 0,
                    scale: 0,
                    nullability: OcbNullabilityV1::NonNull,
                    reserved0: 0,
                    fixed_binary_width: 0,
                },
            ],
            crc32c: 0,
        };
        let schema_bytes = write_object(|buf| schema.write_to(buf));
        let decoded = OcbSchemaV1::read_from(Cursor::new(schema_bytes)).expect("read schema");
        assert_eq!(decoded.columns.len(), 2);
        assert_eq!(
            decoded.columns[1].logical_kind,
            OcbLogicalKindV1::DictionaryCode
        );
        assert_eq!(decoded.columns[1].dictionary_id, 0);

        let bad_schema = OcbSchemaV1 {
            columns: vec![OcbColumnDescV1 {
                logical_kind: OcbLogicalKindV1::DictionaryCode,
                dictionary_id: OCB_NULL_U32,
                ..schema.columns[0]
            }],
            ..schema
        };
        let err = bad_schema
            .write_to(Vec::new())
            .expect_err("dictionary reference required");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert!(err.to_string().contains("dictionary-coded"));
    }

    #[test]
    fn dictionary_values_roundtrip_validates_offsets_and_fixed_width() {
        let values = OcbDictionaryValuesV1 {
            version: 1,
            value_kind: OcbDictionaryValueKindV1::Utf8,
            fixed_width: 0,
            values: vec![b"alpha".to_vec(), b"beta".to_vec()],
            crc32c: 0,
        };
        let bytes = write_object(|buf| values.write_to(buf));
        let decoded =
            OcbDictionaryValuesV1::read_from(Cursor::new(bytes)).expect("read dictionary values");
        assert_eq!(decoded.value_kind, OcbDictionaryValueKindV1::Utf8);
        assert_eq!(decoded.values, vec![b"alpha".to_vec(), b"beta".to_vec()]);

        let bad_fixed = OcbDictionaryValuesV1 {
            version: 1,
            value_kind: OcbDictionaryValueKindV1::FixedBytes,
            fixed_width: 4,
            values: vec![b"abc".to_vec()],
            crc32c: 0,
        };
        assert!(
            bad_fixed
                .write_to(Vec::new())
                .expect_err("fixed width mismatch")
                .to_string()
                .contains("fixed-byte")
        );
    }

    #[test]
    fn row_group_index_and_ordering_proof_roundtrip() {
        let chunk_ref = body_ref(OcbBodyKindV1::ColumnChunk, 65_536, 128);
        let row_group_index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups: vec![OcbRowGroupDescV1 {
                row_group_id: 0,
                flags: 0,
                base_row: 0,
                row_count: 4,
                chunk_desc_begin: 0,
                chunk_desc_count: 1,
                stat_begin: 0,
                stat_count: 1,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            }],
            column_chunks: vec![OcbColumnChunkDescV1 {
                row_group_id: 0,
                column_id: 0,
                physical_type: OcbPhysicalTypeV1::I64,
                codec: OcbChunkCodecV1::None,
                flags: 0,
                value_ref: chunk_ref,
                validity_ref: OcbBodyRefV2::NULL,
                row_count: 4,
                uncompressed_bytes: 32,
            }],
            stats: vec![OcbColumnStatsV1 {
                row_group_id: 0,
                column_id: 0,
                physical_type: OcbPhysicalTypeV1::I64,
                flags: 0,
                null_count: 0,
                min_value: OcbStatScalarV1::I64(10),
                max_value: OcbStatScalarV1::I64(13),
            }],
            crc32c: 0,
        };
        let bytes = write_object(|buf| row_group_index.write_to(buf));
        let decoded = OcbRowGroupIndexV1::read_from(Cursor::new(bytes)).expect("read index");
        assert_eq!(decoded.row_groups[0].row_count, 4);
        assert_eq!(decoded.column_chunks[0].value_ref, chunk_ref);
        assert_eq!(decoded.stats[0].min_value, OcbStatScalarV1::I64(10));

        let ordering = OcbOrderingProofV1 {
            version: 1,
            flags: 0b11,
            keys: vec![OcbOrderingKeyV1 {
                column_id: 0,
                direction: OcbOrderingDirectionV1::Ascending,
                null_order: OcbNullOrderV1::NoNulls,
                reserved0: 0,
            }],
            row_group_proofs: vec![OcbRowGroupOrderingProofV1 {
                row_group_id: 0,
                flags: 1,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            }],
            crc32c: 0,
        };
        let bytes = write_object(|buf| ordering.write_to(buf));
        let decoded = OcbOrderingProofV1::read_from(Cursor::new(bytes)).expect("read ordering");
        assert_eq!(decoded.keys[0].direction, OcbOrderingDirectionV1::Ascending);
        assert_eq!(decoded.row_group_proofs[0].row_group_id, 0);
    }

    #[test]
    fn string_table_materialization_budget_is_exact_aggregate_and_structure_first() {
        let strings = OcbStringTableV1 {
            version: 1,
            strings: vec!["alpha".into(), "beta".into()],
            crc32c: 0,
        };
        let bytes = write_object(|buf| strings.write_to(buf));
        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<String>(2).expect("string descriptors"),
            9,
        ])
        .expect("string-table retained bytes");

        let mut exact = MetadataMaterializationBudget::new(retained_bytes);
        let decoded = OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut exact)
            .expect("exact metadata budget");
        assert_eq!(decoded.strings, strings.strings);
        assert_eq!(exact.charged_bytes(), retained_bytes);

        let mut short = MetadataMaterializationBudget::new(retained_bytes - 1);
        let err = OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut short)
            .expect_err("exact-minus-one metadata budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert_eq!(short.charged_bytes(), 0);

        let aggregate_limit = retained_bytes.checked_mul(2).expect("two allocations");
        let mut aggregate = MetadataMaterializationBudget::new(aggregate_limit);
        OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut aggregate)
            .expect("first aggregate allocation");
        OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut aggregate)
            .expect("second aggregate allocation at exact limit");
        assert_eq!(aggregate.charged_bytes(), aggregate_limit);

        let mut aggregate_short = MetadataMaterializationBudget::new(aggregate_limit - 1);
        OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut aggregate_short)
            .expect("first aggregate allocation");
        let err =
            OcbStringTableV1::read_from_bytes_with_budget(bytes.clone(), &mut aggregate_short)
                .expect_err("second aggregate allocation exceeds shared limit");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert_eq!(aggregate_short.charged_bytes(), retained_bytes);

        let mut oversized_length = bytes.clone();
        oversized_length[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        refresh_crc(&mut oversized_length);
        let mut zero = MetadataMaterializationBudget::new(0);
        let err = OcbStringTableV1::read_from_bytes_with_budget(oversized_length, &mut zero)
            .expect_err("truncated string length wins over zero policy budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert_eq!(zero.charged_bytes(), 0);

        let mut malformed = bytes;
        malformed[24] = 0xFF;
        refresh_crc(&mut malformed);
        let mut zero = MetadataMaterializationBudget::new(0);
        let err = OcbStringTableV1::read_from_bytes_with_budget(malformed, &mut zero)
            .expect_err("malformed UTF-8 wins over zero policy budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert_eq!(zero.charged_bytes(), 0);
    }

    #[test]
    fn dictionary_values_materialization_budget_is_exact_and_structure_first() {
        let values = OcbDictionaryValuesV1 {
            version: 1,
            value_kind: OcbDictionaryValueKindV1::Utf8,
            fixed_width: 0,
            values: vec![b"alpha".to_vec(), b"beta".to_vec()],
            crc32c: 0,
        };
        let bytes = write_object(|buf| values.write_to(buf));
        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<Vec<u8>>(2).expect("value descriptors"),
            9,
        ])
        .expect("dictionary retained bytes");

        let mut exact = MetadataMaterializationBudget::new(retained_bytes);
        let decoded = OcbDictionaryValuesV1::read_from_bytes_with_budget(bytes.clone(), &mut exact)
            .expect("exact dictionary budget");
        assert_eq!(decoded.values, values.values);
        assert_eq!(exact.charged_bytes(), retained_bytes);

        let mut short = MetadataMaterializationBudget::new(retained_bytes - 1);
        let err = OcbDictionaryValuesV1::read_from_bytes_with_budget(bytes.clone(), &mut short)
            .expect_err("exact-minus-one dictionary budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert_eq!(short.charged_bytes(), 0);

        let mut malformed = bytes;
        malformed[48..56].copy_from_slice(&10u64.to_le_bytes());
        refresh_crc(&mut malformed);
        let mut zero = MetadataMaterializationBudget::new(0);
        let err = OcbDictionaryValuesV1::read_from_bytes_with_budget(malformed, &mut zero)
            .expect_err("malformed offsets win over zero policy budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::CorruptFile)
        );
        assert_eq!(zero.charged_bytes(), 0);
    }

    #[test]
    fn row_group_index_materialization_budget_covers_fixed_record_vectors() {
        let index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups: vec![OcbRowGroupDescV1 {
                row_group_id: 0,
                flags: 0,
                base_row: 0,
                row_count: 0,
                chunk_desc_begin: 0,
                chunk_desc_count: 0,
                stat_begin: 0,
                stat_count: 0,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            }],
            column_chunks: Vec::new(),
            stats: Vec::new(),
            crc32c: 0,
        };
        let bytes = write_object(|buf| index.write_to(buf));
        let retained_bytes =
            checked_materialized_vec_bytes::<OcbRowGroupDescV1>(1).expect("row-group bytes");

        let mut exact = MetadataMaterializationBudget::new(retained_bytes);
        let decoded = OcbRowGroupIndexV1::read_from_bytes_with_budget(bytes.clone(), &mut exact)
            .expect("exact fixed-record budget");
        assert_eq!(decoded.row_groups.len(), 1);
        assert_eq!(exact.charged_bytes(), retained_bytes);

        let mut short = MetadataMaterializationBudget::new(retained_bytes - 1);
        let err = OcbRowGroupIndexV1::read_from_bytes_with_budget(bytes, &mut short)
            .expect_err("fixed-record exact-minus-one budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert_eq!(short.charged_bytes(), 0);
    }

    #[test]
    fn ordering_proof_materialization_budget_covers_fixed_record_vectors() {
        let ordering = OcbOrderingProofV1 {
            version: 1,
            flags: 0,
            keys: vec![OcbOrderingKeyV1 {
                column_id: 0,
                direction: OcbOrderingDirectionV1::Ascending,
                null_order: OcbNullOrderV1::NoNulls,
                reserved0: 0,
            }],
            row_group_proofs: vec![OcbRowGroupOrderingProofV1 {
                row_group_id: 0,
                flags: 0,
                first_tuple_ref: OcbBodyRefV2::NULL,
                last_tuple_ref: OcbBodyRefV2::NULL,
            }],
            crc32c: 0,
        };
        let bytes = write_object(|buf| ordering.write_to(buf));
        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<OcbOrderingKeyV1>(1).expect("ordering-key bytes"),
            checked_materialized_vec_bytes::<OcbRowGroupOrderingProofV1>(1)
                .expect("ordering-proof bytes"),
        ])
        .expect("ordering metadata bytes");

        let mut exact = MetadataMaterializationBudget::new(retained_bytes);
        let decoded = OcbOrderingProofV1::read_from_bytes_with_budget(bytes.clone(), &mut exact)
            .expect("exact ordering budget");
        assert_eq!(decoded.keys.len(), 1);
        assert_eq!(decoded.row_group_proofs.len(), 1);
        assert_eq!(exact.charged_bytes(), retained_bytes);

        let mut short = MetadataMaterializationBudget::new(retained_bytes - 1);
        let err = OcbOrderingProofV1::read_from_bytes_with_budget(bytes, &mut short)
            .expect_err("ordering exact-minus-one budget");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::OcbFailureCause::InvalidInput)
        );
        assert_eq!(short.charged_bytes(), 0);
    }

    #[test]
    fn policy_a_accepts_inferred_yearly_index_materialization_by_arithmetic_only() {
        const ROW_GROUP_COUNT: usize = 342_582;
        const COLUMN_CHUNK_COUNT: usize = 4_796_148;
        const STAT_COUNT: usize = 4_796_148;

        // Storage A permits only fixed-width arithmetic here: this test neither
        // creates nor opens a yearly OCB artifact.
        let encoded_bytes = 32u64
            + ROW_GROUP_COUNT as u64 * OCB_ROW_GROUP_DESC_V1_LEN as u64
            + COLUMN_CHUNK_COUNT as u64 * OCB_COLUMN_CHUNK_DESC_V1_LEN as u64
            + STAT_COUNT as u64 * OCB_COLUMN_STATS_V1_LEN as u64
            + 4;
        assert_eq!(encoded_bytes, 731_755_188);
        assert!(encoded_bytes <= OCB_POLICY_A_MAX_ENCODED_OBJECT_BYTES);

        let retained_bytes = checked_materialized_sum(&[
            checked_materialized_vec_bytes::<OcbRowGroupDescV1>(ROW_GROUP_COUNT)
                .expect("row-group materialization"),
            checked_materialized_vec_bytes::<OcbColumnChunkDescV1>(COLUMN_CHUNK_COUNT)
                .expect("chunk materialization"),
            checked_materialized_vec_bytes::<OcbColumnStatsV1>(STAT_COUNT)
                .expect("stat materialization"),
        ])
        .expect("inferred retained bytes");
        assert!(std::mem::size_of::<OcbRowGroupDescV1>() <= OCB_ROW_GROUP_DESC_V1_LEN);
        assert!(std::mem::size_of::<OcbColumnChunkDescV1>() <= OCB_COLUMN_CHUNK_DESC_V1_LEN);
        assert!(std::mem::size_of::<OcbColumnStatsV1>() <= OCB_COLUMN_STATS_V1_LEN);
        assert!(retained_bytes <= encoded_bytes - 36);
        assert!(retained_bytes <= OCB_POLICY_A_MAX_OWNED_DECODED_MATERIALIZED_BYTES);

        let mut policy_a = policy_a_metadata_materialization_budget();
        policy_a
            .charge(retained_bytes)
            .expect("inferred fixed-width index fits Policy A");
        assert_eq!(policy_a.charged_bytes(), retained_bytes);
    }

    #[test]
    fn column_chunk_roundtrip_validates_payload_length() {
        let mut payload = Vec::new();
        for value in [1_i64, 2, 3, 4] {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::I64,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id: 3,
            column_id: 7,
            row_count: 4,
            uncompressed_bytes: 32,
            payload,
            crc32c: 0,
        };
        let bytes = write_object(|buf| chunk.write_to(buf));
        let decoded = OcbColumnChunkObjectV1::read_from(Cursor::new(bytes)).expect("read chunk");
        assert_eq!(decoded.row_group_id, 3);
        assert_eq!(decoded.column_id, 7);
        assert_eq!(decoded.payload.len(), 32);

        let bad = OcbColumnChunkObjectV1 {
            payload: vec![0; 7],
            ..chunk
        };
        assert!(
            bad.write_to(Vec::new())
                .expect_err("payload length mismatch")
                .to_string()
                .contains("payload length")
        );
    }

    #[test]
    fn bounded_zstd_decode_rejects_malformed_and_over_limit_output() {
        fn encoded(bytes: &[u8]) -> Vec<u8> {
            zstd::stream::encode_all(Cursor::new(bytes), 1).expect("encode zstd fixture")
        }

        let exact = encoded(&[1, 2, 3]);
        assert_eq!(
            decode_zstd_payload_bounded(&exact, 3, 3).expect("exact decode"),
            [1, 2, 3]
        );
        assert_eq!(
            decode_zstd_payload_bounded(&encoded(&[1]), 1, 1)
                .expect("tiny output limit retains writer window compatibility"),
            [1]
        );
        let high_compression =
            zstd::stream::encode_all(Cursor::new([2]), 22).expect("level-22 zstd fixture");
        assert_eq!(
            decode_zstd_payload_bounded(&high_compression, 1, 1)
                .expect("level-22 tiny payload remains readable"),
            [2]
        );

        let empty = encoded(&[]);
        assert!(
            decode_zstd_payload_bounded(&empty, 0, 0)
                .expect("empty decode")
                .is_empty()
        );

        let mut concatenated = encoded(&[4, 5]);
        concatenated.extend_from_slice(&encoded(&[6, 7]));
        assert_eq!(
            decode_zstd_payload_bounded(&concatenated, 4, 4).expect("concatenated frame decode"),
            [4, 5, 6, 7]
        );

        let extra = encoded(&[8, 9]);
        let err = decode_zstd_payload_bounded(&extra, 1, 1)
            .expect_err("expected+1 output must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let bomb = encoded(&vec![0; 4096]);
        let err = decode_zstd_payload_bounded(&bomb, 1, 4096)
            .expect_err("tiny compressed expansion must be bounded");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let mut truncated = encoded(&[10, 11, 12]);
        truncated.truncate(truncated.len() - 2);
        let err = decode_zstd_payload_bounded(&truncated, 3, 3)
            .expect_err("truncated frame must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let err = decode_zstd_payload_bounded(&[0, 1, 2, 3], 1, 1)
            .expect_err("invalid frame must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let err = decode_zstd_payload_bounded(&exact, 3, 2)
            .expect_err("decoded policy limit+1 must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::InvalidInput)
        );

        let err = decode_zstd_payload_bounded(&empty, u64::MAX, u64::MAX)
            .expect_err("expected+1 arithmetic overflow must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        assert_eq!(zstd_window_log_for_decoded_limit(0), 27);
        assert_eq!(zstd_window_log_for_decoded_limit(1), 27);
        assert_eq!(zstd_window_log_for_decoded_limit(1 << 10), 27);
        assert_eq!(zstd_window_log_for_decoded_limit(1 << 27), 27);
        assert_eq!(zstd_window_log_for_decoded_limit((1 << 29) - 1), 28);
        assert_eq!(zstd_window_log_for_decoded_limit(1 << 29), 29);
        assert_eq!(zstd_window_log_for_decoded_limit(1 << 30), 30);

        // Standard zstd frame: no content-size field, a 2^30-byte window,
        // and one empty final raw block. Reject the window before allocating
        // decoder history for it.
        let oversized_window = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0xA0, 0x01, 0x00, 0x00];
        let err = decode_zstd_payload_bounded(&oversized_window, 0, 0)
            .expect_err("oversized zstd window must be rejected");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );

        let err = allocate_zeroed_bytes_fallibly(
            usize::MAX,
            "OCB decoded payload allocation failed within resource limit",
        )
        .expect_err("capacity overflow must be reported without allocation");
        assert!(matches!(err, ArcadiaTioError::Io(ref io) if io.kind() == ErrorKind::OutOfMemory));

        let err = try_vec_with_capacity::<u64>(
            usize::MAX,
            "OCB parser allocation failed within resource limit",
        )
        .expect_err("parser capacity overflow must be reported without allocation");
        assert!(matches!(err, ArcadiaTioError::Io(ref io) if io.kind() == ErrorKind::OutOfMemory));
    }

    #[test]
    fn uncompressed_chunk_enforces_encoded_and_decoded_limits() {
        let chunk = OcbColumnChunkObjectV1 {
            version: 1,
            physical_type: OcbPhysicalTypeV1::I64,
            codec: OcbChunkCodecV1::None,
            flags: 0,
            row_group_id: 0,
            column_id: 0,
            row_count: 1,
            uncompressed_bytes: 8,
            payload: vec![0; 8],
            crc32c: 0,
        };
        assert_eq!(
            chunk
                .decode_payload_with_limits(8, 8)
                .expect("exact limits"),
            [0; 8]
        );

        let compressed = chunk
            .decode_payload_with_limits(7, 8)
            .expect_err("encoded limit+1");
        assert_eq!(
            compressed.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::InvalidInput)
        );

        let decoded = chunk
            .decode_payload_with_limits(8, 7)
            .expect_err("decoded limit+1");
        assert_eq!(
            decoded.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::InvalidInput)
        );

        let structurally_invalid = OcbColumnChunkObjectV1 {
            payload: vec![0; 7],
            ..chunk
        };
        let err = structurally_invalid
            .decode_payload_with_limits(0, 0)
            .expect_err("structure wins over policy");
        assert_eq!(
            err.ocb_failure_cause(),
            Some(crate::error::OcbFailureCause::CorruptFile)
        );
    }
}
