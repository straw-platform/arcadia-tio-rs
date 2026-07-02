//! Certification helpers for channel-sharded compact-L2 OCB artifacts.

use std::fs;
use std::io::Read;
use std::path::Path;

use sha2::{Digest, Sha256};

use crate::compact_l2::{
    COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1, COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
    COMPACT_L2_PAYLOAD_COLUMN_NAME, CompactL2RecordKind, decode_compact_l2_fixed_binary_header_v1,
};
use crate::manifest::{
    ChannelArtifactEntryV1, ChannelShardedManifestV1, resolve_manifest_relative_artifact_path,
    validate_hex_hash,
};
use crate::{
    ArcadiaTioError, ColumnBundleFile, ColumnBundleReadCursorOptions, ColumnBundleReadOptions,
    ColumnBundleReadRequest, ColumnBundleVisitControl, ColumnPhysicalType, ColumnProjection,
    OcbErrorKind, PrimitiveColumnValuesRef, Result,
};

/// Options for channel-sharded compact-L2 artifact certification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationOptions {
    /// Expected artifact format/layout label.
    pub artifact_format: String,
    /// Expected fixed-binary record width.
    pub expected_record_width: u32,
    /// Fixed-binary payload column name.
    pub payload_column_name: String,
    /// Validate compact-L2 payload headers and channel/BizIndex continuity.
    pub verify_payload_header: bool,
    /// Request CRC/checksum validation. Current OCB reads remain fail-closed and
    /// validate checksums even when this is false.
    pub verify_crc32c: bool,
    /// Optional aggregate row cap.
    pub max_rows: Option<u64>,
    /// Requested OCB read threads.
    pub read_threads: usize,
    /// Maximum reusable row groups in flight while certifying one artifact.
    pub max_in_flight_row_groups: usize,
    /// Validate optional manifest SHA-256/FNV fingerprints when present.
    pub verify_hashes: bool,
}

impl Default for CertificationOptions {
    fn default() -> Self {
        Self {
            artifact_format: COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1.to_owned(),
            expected_record_width: COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            payload_column_name: COMPACT_L2_PAYLOAD_COLUMN_NAME.to_owned(),
            verify_payload_header: true,
            verify_crc32c: true,
            max_rows: None,
            read_threads: 1,
            max_in_flight_row_groups: 1,
            verify_hashes: true,
        }
    }
}

/// Path-free certification report for a channel-sharded compact-L2 manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationReport {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Number of channels certified.
    pub channel_count: usize,
    /// Aggregate certified row count.
    pub row_count: u64,
    /// Aggregate certified row-group count.
    pub row_group_count: u64,
    /// Number of failed channels. This is zero for `Ok` reports; failures return
    /// an error and stop before exposing raw paths.
    pub failed_channel_count: usize,
    /// Per-channel path-free reports in manifest order.
    pub channel_reports: Vec<ChannelCertificationReport>,
    /// Safe aggregate summary with no absolute paths.
    pub safe_summary: SafeCertificationSummary,
}

/// Path-free per-channel certification report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelCertificationReport {
    /// ChannelID certified for this artifact.
    pub channel_id: u32,
    /// Observed row count.
    pub row_count: u64,
    /// Observed OCB row-group count.
    pub row_group_count: u32,
    /// First decoded BizIndex.
    pub first_biz_index: u64,
    /// Last decoded BizIndex.
    pub last_biz_index: u64,
    /// Minimum observed receive nano, when payload headers were verified.
    pub min_receive_nano: Option<i64>,
    /// Maximum observed receive nano, when payload headers were verified.
    pub max_receive_nano: Option<i64>,
    /// Observed order record count, when payload headers were verified.
    pub order_record_count: Option<u64>,
    /// Observed trade record count, when payload headers were verified.
    pub trade_record_count: Option<u64>,
    /// Whether at least one optional manifest hash/fingerprint check was present
    /// and passed.
    pub checksum_verified: bool,
}

/// Path-free aggregate certification summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeCertificationSummary {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Artifact format/layout label.
    pub artifact_format: String,
    /// Number of certified channels.
    pub channel_count: usize,
    /// Aggregate row count.
    pub row_count: u64,
    /// Aggregate OCB row-group count.
    pub row_group_count: u64,
    /// Whether certification completed.
    pub certified: bool,
    /// Always true: raw/absolute paths are excluded.
    pub path_redacted: bool,
}

#[derive(Debug, Clone, Copy, Default)]
struct ChannelPayloadScanState {
    rows: u64,
    order_rows: u64,
    trade_rows: u64,
    first_biz_index: Option<u64>,
    last_biz_index: Option<u64>,
    min_receive_nano: Option<i64>,
    max_receive_nano: Option<i64>,
}

/// Certify a channel-sharded compact-L2 artifact manifest.
///
/// The helper validates manifest schema/count/path invariants, confines
/// manifest-relative artifact paths, opens every listed OCB artifact through the
/// pure-Rust core reader, checks row/row-group/fixed-binary width metadata, and
/// by default validates compact-L2 payload headers for trading-day, ChannelID,
/// strict gap-free BizIndex continuity, record kind, receive-nano edges, and
/// optional manifest hashes/fingerprints.
pub fn certify_channel_sharded_artifact_v1(
    manifest_path: impl AsRef<Path>,
    options: &CertificationOptions,
) -> Result<CertificationReport> {
    let manifest_path = manifest_path.as_ref();
    let manifest = ChannelShardedManifestV1::from_path(manifest_path)?;
    if manifest.artifact_format != options.artifact_format {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsupportedFormat,
            "channel-sharded OCB artifact format does not match certification options",
        ));
    }
    if let Some(width) = manifest.payload_width_bytes {
        if width != options.expected_record_width {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "channel-sharded OCB manifest payload width mismatch: expected={} observed={width}",
                    options.expected_record_width
                ),
            ));
        }
    }
    let manifest_rows = manifest.channels.iter().try_fold(0u64, |acc, channel| {
        acc.checked_add(channel.row_count).ok_or_else(|| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                "channel-sharded OCB manifest row total overflows",
            )
        })
    })?;
    if let Some(max_rows) = options.max_rows {
        if manifest_rows > max_rows {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB manifest rows exceed max_rows: rows={manifest_rows} max_rows={max_rows}"
                ),
            ));
        }
    }

    let mut reports = Vec::with_capacity(manifest.channels.len());
    for channel in &manifest.channels {
        reports.push(certify_channel_artifact(
            manifest_path,
            &manifest,
            channel,
            options,
        )?);
    }

    let row_count = reports.iter().map(|report| report.row_count).sum();
    let row_group_count = reports
        .iter()
        .map(|report| u64::from(report.row_group_count))
        .sum();
    let safe_summary = SafeCertificationSummary {
        schema_version: manifest.schema_version,
        trading_day: manifest.trading_day,
        artifact_format: manifest.artifact_format.clone(),
        channel_count: reports.len(),
        row_count,
        row_group_count,
        certified: true,
        path_redacted: true,
    };
    Ok(CertificationReport {
        schema_version: manifest.schema_version,
        channel_count: reports.len(),
        row_count,
        row_group_count,
        failed_channel_count: 0,
        channel_reports: reports,
        safe_summary,
    })
}

fn certify_channel_artifact(
    manifest_path: &Path,
    manifest: &ChannelShardedManifestV1,
    channel: &ChannelArtifactEntryV1,
    options: &CertificationOptions,
) -> Result<ChannelCertificationReport> {
    let artifact_path =
        resolve_manifest_relative_artifact_path(manifest_path, &channel.relative_path)?;
    if !artifact_path.exists() {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::MissingArtifact,
            format!(
                "channel-sharded OCB artifact is missing: channel={}",
                channel.channel_id
            ),
        ));
    }
    let root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let root_canonical = fs::canonicalize(root).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB manifest root could not be canonicalized",
        )
    })?;
    let artifact_canonical = fs::canonicalize(&artifact_path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::MissingArtifact,
            format!(
                "channel-sharded OCB artifact could not be canonicalized: channel={}",
                channel.channel_id
            ),
        )
    })?;
    if !artifact_canonical.starts_with(&root_canonical) {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB artifact path escapes manifest root",
        ));
    }

    let checksum_verified = if options.verify_hashes {
        verify_optional_hashes(&artifact_canonical, channel)?
    } else {
        false
    };

    let file = ColumnBundleFile::open(&artifact_canonical)?;
    let metadata = file.metadata()?;
    if metadata.row_count != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB metadata row count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_count, metadata.row_count
            ),
        ));
    }
    if channel.row_group_count != 0 && metadata.row_group_count != channel.row_group_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB row-group count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_group_count, metadata.row_group_count
            ),
        ));
    }

    let request = ColumnBundleReadRequest {
        projection: ColumnProjection::names([options.payload_column_name.as_str()]),
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: options.read_threads.max(1),
            validate_checksums: true,
            decode_dictionaries: false,
        },
    };
    let plan = file.plan_read(&request)?;
    let row_group_summaries = file.read_plan_row_group_summaries(&plan)?;
    let mut summary_rows = 0u64;
    for row_group in &row_group_summaries {
        summary_rows = summary_rows
            .checked_add(row_group.row_count)
            .ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row-group row total overflows",
                )
            })?;
        if row_group.chunks.len() != 1 {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::InvalidManifest,
                format!(
                    "channel-sharded OCB row group projected unexpected chunk count: channel={} row_group={} chunks={}",
                    channel.channel_id,
                    row_group.row_group_id,
                    row_group.chunks.len()
                ),
            ));
        }
        let chunk = &row_group.chunks[0];
        if chunk.physical_type
            != (ColumnPhysicalType::FixedBinary {
                width: options.expected_record_width,
            })
            || chunk.fixed_binary_width != Some(options.expected_record_width)
        {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::FixedBinaryWidthMismatch,
                format!(
                    "channel-sharded OCB fixed-binary width mismatch: channel={} row_group={} expected={} observed={}",
                    channel.channel_id,
                    row_group.row_group_id,
                    options.expected_record_width,
                    chunk.fixed_binary_width.unwrap_or(0)
                ),
            ));
        }
        if chunk.validity_ref.is_some() {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB payload chunk must be non-nullable: channel={} row_group={}",
                    channel.channel_id, row_group.row_group_id
                ),
            ));
        }
    }
    if summary_rows != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB row-group rows mismatch: channel={} expected={} observed={summary_rows}",
                channel.channel_id, channel.row_count
            ),
        ));
    }

    let scan_state = if options.verify_payload_header {
        scan_payload_headers(&file, &plan, channel, manifest.trading_day, options)?
    } else {
        ChannelPayloadScanState {
            rows: channel.row_count,
            first_biz_index: Some(channel.first_biz_index),
            last_biz_index: Some(channel.last_biz_index),
            ..ChannelPayloadScanState::default()
        }
    };

    if scan_state.rows != channel.row_count {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::RowCountMismatch,
            format!(
                "channel-sharded OCB decoded row count mismatch: channel={} expected={} observed={}",
                channel.channel_id, channel.row_count, scan_state.rows
            ),
        ));
    }
    if scan_state.first_biz_index != Some(channel.first_biz_index)
        || scan_state.last_biz_index != Some(channel.last_biz_index)
    {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::BizIndexGap,
            format!(
                "channel-sharded OCB decoded BizIndex range mismatch: channel={} expected={}..={} observed={:?}..{:?}",
                channel.channel_id,
                channel.first_biz_index,
                channel.last_biz_index,
                scan_state.first_biz_index,
                scan_state.last_biz_index
            ),
        ));
    }
    if let Some(expected) = channel.min_receive_nano {
        if scan_state.min_receive_nano != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB first receive_nano mismatch: channel={} expected={} observed={:?}",
                    channel.channel_id, expected, scan_state.min_receive_nano
                ),
            ));
        }
    }
    if let Some(expected) = channel.max_receive_nano {
        if scan_state.max_receive_nano != Some(expected) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::PayloadHeaderMismatch,
                format!(
                    "channel-sharded OCB last receive_nano mismatch: channel={} expected={} observed={:?}",
                    channel.channel_id, expected, scan_state.max_receive_nano
                ),
            ));
        }
    }
    if let Some(expected) = channel.order_record_count {
        if scan_state.order_rows != expected {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB order count mismatch: channel={} expected={} observed={}",
                    channel.channel_id, expected, scan_state.order_rows
                ),
            ));
        }
    }
    if let Some(expected) = channel.trade_record_count {
        if scan_state.trade_rows != expected {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::RowCountMismatch,
                format!(
                    "channel-sharded OCB trade count mismatch: channel={} expected={} observed={}",
                    channel.channel_id, expected, scan_state.trade_rows
                ),
            ));
        }
    }

    Ok(ChannelCertificationReport {
        channel_id: channel.channel_id,
        row_count: channel.row_count,
        row_group_count: metadata.row_group_count,
        first_biz_index: channel.first_biz_index,
        last_biz_index: channel.last_biz_index,
        min_receive_nano: scan_state.min_receive_nano.or(channel.min_receive_nano),
        max_receive_nano: scan_state.max_receive_nano.or(channel.max_receive_nano),
        order_record_count: options
            .verify_payload_header
            .then_some(scan_state.order_rows),
        trade_record_count: options
            .verify_payload_header
            .then_some(scan_state.trade_rows),
        checksum_verified,
    })
}

fn scan_payload_headers(
    file: &ColumnBundleFile,
    plan: &crate::ColumnBundleReadPlan,
    channel: &ChannelArtifactEntryV1,
    trading_day: u32,
    options: &CertificationOptions,
) -> Result<ChannelPayloadScanState> {
    let row_group_ids = plan.row_group_ids.clone();
    let mut buffers =
        file.reusable_buffer_pool_for_plan(plan, options.max_in_flight_row_groups.max(1), false)?;
    let mut state = ChannelPayloadScanState::default();
    let mut expected_biz_index = channel.first_biz_index;
    file.visit_plan_row_groups_into_with_attribution(
        plan,
        &row_group_ids,
        ColumnBundleReadCursorOptions {
            max_in_flight_row_groups: options.max_in_flight_row_groups.max(1),
            ordered: true,
        },
        &mut buffers,
        |batch| {
            let column = batch.column(0)?;
            let (width, bytes) = match column.values {
                PrimitiveColumnValuesRef::FixedBinary { width, bytes } => (width, bytes),
                _ => {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::FixedBinaryWidthMismatch,
                        "channel-sharded OCB payload column is not fixed-binary",
                    ));
                }
            };
            if width != options.expected_record_width {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::FixedBinaryWidthMismatch,
                    format!(
                        "channel-sharded OCB payload width mismatch: channel={} row_group={} expected={} observed={width}",
                        channel.channel_id,
                        batch.row_group_id(),
                        options.expected_record_width
                    ),
                ));
            }
            let row_count = usize::try_from(batch.row_count()).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row group row count exceeds usize",
                )
            })?;
            let expected_bytes = row_count.checked_mul(width as usize).ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    "channel-sharded OCB row group payload byte count overflows",
                )
            })?;
            if bytes.len() < expected_bytes {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::RowCountMismatch,
                    format!(
                        "channel-sharded OCB row group payload byte count mismatch: channel={} row_group={}",
                        channel.channel_id,
                        batch.row_group_id()
                    ),
                ));
            }
            for record in bytes[..expected_bytes].chunks_exact(width as usize) {
                let header = decode_compact_l2_fixed_binary_header_v1(record)?;
                if header.trading_day != trading_day {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::PayloadHeaderMismatch,
                        format!(
                            "channel-sharded OCB trading day mismatch: channel={} expected={} observed={}",
                            channel.channel_id, trading_day, header.trading_day
                        ),
                    ));
                }
                if u32::try_from(header.channel_id).ok() != Some(channel.channel_id) {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::ChannelIdMismatch,
                        format!(
                            "channel-sharded OCB ChannelID mismatch: expected={} observed={}",
                            channel.channel_id, header.channel_id
                        ),
                    ));
                }
                let biz_index = u64::try_from(header.biz_index).map_err(|_| {
                    ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::PayloadHeaderMismatch,
                        "channel-sharded OCB BizIndex is negative",
                    )
                })?;
                if biz_index < expected_biz_index {
                    let kind = if expected_biz_index.saturating_sub(biz_index) == 1 {
                        OcbErrorKind::BizIndexDuplicate
                    } else {
                        OcbErrorKind::BizIndexRegression
                    };
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        kind,
                        format!(
                            "channel-sharded OCB BizIndex regressed: channel={} expected={} observed={biz_index}",
                            channel.channel_id, expected_biz_index
                        ),
                    ));
                }
                if biz_index > expected_biz_index {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        format!(
                            "channel-sharded OCB BizIndex gap: channel={} expected={} observed={biz_index}",
                            channel.channel_id, expected_biz_index
                        ),
                    ));
                }
                if biz_index > channel.last_biz_index {
                    return Err(ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        format!(
                            "channel-sharded OCB BizIndex exceeds manifest range: channel={} last={} observed={biz_index}",
                            channel.channel_id, channel.last_biz_index
                        ),
                    ));
                }
                state.first_biz_index.get_or_insert(biz_index);
                state.last_biz_index = Some(biz_index);
                state.min_receive_nano = Some(match state.min_receive_nano {
                    Some(current) => current.min(header.receive_nano),
                    None => header.receive_nano,
                });
                state.max_receive_nano = Some(match state.max_receive_nano {
                    Some(current) => current.max(header.receive_nano),
                    None => header.receive_nano,
                });
                match header.record_kind {
                    CompactL2RecordKind::Order => state.order_rows = state.order_rows.saturating_add(1),
                    CompactL2RecordKind::Trade => state.trade_rows = state.trade_rows.saturating_add(1),
                }
                state.rows = state.rows.saturating_add(1);
                expected_biz_index = expected_biz_index.checked_add(1).ok_or_else(|| {
                    ArcadiaTioError::ocb_diagnostic(
                        OcbErrorKind::BizIndexGap,
                        "channel-sharded OCB BizIndex sequence overflows",
                    )
                })?;
            }
            Ok(ColumnBundleVisitControl::Continue)
        },
    )?;
    if expected_biz_index != channel.last_biz_index.saturating_add(1) {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::BizIndexGap,
            format!(
                "channel-sharded OCB final BizIndex mismatch: channel={} expected_next={} manifest_last={}",
                channel.channel_id, expected_biz_index, channel.last_biz_index
            ),
        ));
    }
    Ok(state)
}

fn verify_optional_hashes(path: &Path, channel: &ChannelArtifactEntryV1) -> Result<bool> {
    let mut verified = false;
    if let Some(expected) = &channel.payload_sha256 {
        validate_hex_hash(expected, 64, OcbErrorKind::ChecksumMismatch)?;
        let actual = sha256_file_hex(path)?;
        if !expected.eq_ignore_ascii_case(&actual) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::ChecksumMismatch,
                format!(
                    "channel-sharded OCB SHA-256 mismatch: channel={}",
                    channel.channel_id
                ),
            ));
        }
        verified = true;
    }
    if let Some(fingerprint) = &channel.fingerprint {
        if let Some(expected_bytes) = fingerprint.file_bytes {
            let actual_bytes = fs::metadata(path)
                .map_err(|_| {
                    ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "stat artifact failed")
                })?
                .len();
            if actual_bytes != expected_bytes {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChecksumMismatch,
                    format!(
                        "channel-sharded OCB fingerprint file size mismatch: channel={}",
                        channel.channel_id
                    ),
                ));
            }
            verified = true;
        }
        if let Some(expected) = &fingerprint.content_hash_fnv1a64 {
            validate_hex_hash(expected, 16, OcbErrorKind::ChecksumMismatch)?;
            let actual = fnv1a64_file_hex(path)?;
            if !expected.eq_ignore_ascii_case(&actual) {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::ChecksumMismatch,
                    format!(
                        "channel-sharded OCB FNV-1a64 mismatch: channel={}",
                        channel.channel_id
                    ),
                ));
            }
            verified = true;
        }
    }
    Ok(verified)
}

fn sha256_file_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "open artifact for sha256 failed")
    })?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "read artifact for sha256 failed")
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex_bytes(&hasher.finalize()))
}

fn fnv1a64_file_hex(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "open artifact for fnv failed")
    })?;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "read artifact for fnv failed")
        })?;
        if read == 0 {
            break;
        }
        for byte in &buffer[..read] {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    Ok(format!("{hash:016x}"))
}

fn hex_bytes(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use serde_json::json;

    use super::*;
    use crate::compact_l2::{
        COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1, COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
        COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE,
    };
    use crate::format::{
        OCB_BOOTSTRAP_PAGE_V1_LEN, OCB_NULL_U32, OCB_ROOT_V1_LEN, OcbBodyKindV1, OcbBodyRefV2,
        OcbBootstrapPageV1, OcbChunkCodecV1, OcbColumnChunkDescV1, OcbColumnChunkObjectV1,
        OcbColumnDescV1, OcbLogicalKindV1, OcbNullabilityV1, OcbPhysicalTypeV1, OcbRootV1,
        OcbRowGroupDescV1, OcbRowGroupIndexV1, OcbSchemaV1, OcbStringTableV1, crc32c,
    };

    #[test]
    fn valid_channel_sharded_fixture_certifies_path_free_summary() {
        let root = fixture_root("valid_channel_sharded_fixture_certifies");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 2, 3],
            &[
                COMPACT_L2_RECORD_KIND_ORDER,
                COMPACT_L2_RECORD_KIND_TRADE,
                COMPACT_L2_RECORD_KIND_ORDER,
            ],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 3, 2, 1, 2, 1);

        let report =
            certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
                .expect("valid fixture certifies");
        assert_eq!(report.channel_count, 1);
        assert_eq!(report.row_count, 3);
        assert_eq!(report.row_group_count, 2);
        assert_eq!(report.safe_summary.trading_day, 20260702);
        assert!(report.safe_summary.path_redacted);
        assert!(!format!("{:?}", report.safe_summary).contains(&root.display().to_string()));
        cleanup_root(&root);
    }

    #[test]
    fn biz_index_gap_fixture_fails_with_expected_kind() {
        let root = fixture_root("biz_index_gap_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 3],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("gap fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::BizIndexGap)
        );
        cleanup_root(&root);
    }

    #[test]
    fn biz_index_duplicate_fixture_fails_with_expected_kind() {
        let root = fixture_root("biz_index_duplicate_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 1],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("duplicate fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::BizIndexDuplicate)
        );
        cleanup_root(&root);
    }

    #[test]
    fn channel_id_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("channel_id_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1, 2],
            &[COMPACT_L2_RECORD_KIND_ORDER, COMPACT_L2_RECORD_KIND_TRADE],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            Some(2012),
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 2, 1, 1, 1, 1);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("channel mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::ChannelIdMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn record_width_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("record_width_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1],
            &[COMPACT_L2_RECORD_KIND_ORDER],
            16,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest(&manifest, 20260702, 2011, 1, 1, 1, 1, 0);

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("width mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::FixedBinaryWidthMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn checksum_mismatch_fixture_fails_with_expected_kind() {
        let root = fixture_root("checksum_mismatch_fixture");
        let artifact = root.join("channels/2011/l2_mutations.ocb");
        write_compact_l2_fixture(
            &artifact,
            20260702,
            2011,
            &[1],
            &[COMPACT_L2_RECORD_KIND_ORDER],
            COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            None,
        );
        let manifest = root.join("manifest.json");
        write_manifest_with_payload_sha256(
            &manifest,
            20260702,
            2011,
            1,
            1,
            1,
            1,
            0,
            Some("0000000000000000000000000000000000000000000000000000000000000000"),
        );

        let err = certify_channel_sharded_artifact_v1(&manifest, &CertificationOptions::default())
            .expect_err("checksum mismatch fails closed");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::ChecksumMismatch)
        );
        cleanup_root(&root);
    }

    #[test]
    fn unsafe_manifest_relative_path_fails_with_expected_kind() {
        let root = fixture_root("unsafe_manifest_path_fixture");
        let manifest = root.join("manifest.json");
        let manifest_json = json!({
            "schema_version": 1,
            "trading_day": 20260702,
            "artifact_format": COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
            "payload_width_bytes": COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            "channel_indivisible": true,
            "channels": [{
                "channel_id": 2011,
                "relative_path": "../escape.ocb",
                "row_count": 1,
                "row_group_count": 1,
                "first_biz_index": 1,
                "last_biz_index": 1
            }]
        });
        fs::write(
            &manifest,
            serde_json::to_vec_pretty(&manifest_json).unwrap(),
        )
        .unwrap();
        let err = ChannelShardedManifestV1::from_path(&manifest).expect_err("unsafe path rejected");
        assert_eq!(
            OcbErrorKind::from_error(&err),
            Some(OcbErrorKind::UnsafeManifestPath)
        );
        cleanup_root(&root);
    }

    fn fixture_root(name: &str) -> PathBuf {
        let root =
            PathBuf::from(".tmp/ocb-cert-tests").join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        root
    }

    fn cleanup_root(root: &Path) {
        let _ = fs::remove_dir_all(root);
    }

    fn write_manifest(
        manifest: &Path,
        trading_day: u32,
        channel_id: u32,
        rows: u64,
        row_groups: u32,
        first_biz: u64,
        orders: u64,
        trades: u64,
    ) {
        let parent = manifest.parent().expect("manifest parent");
        fs::create_dir_all(parent).expect("create manifest parent");
        write_manifest_with_payload_sha256(
            manifest,
            trading_day,
            channel_id,
            rows,
            row_groups,
            first_biz,
            orders,
            trades,
            None,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn write_manifest_with_payload_sha256(
        manifest: &Path,
        trading_day: u32,
        channel_id: u32,
        rows: u64,
        row_groups: u32,
        first_biz: u64,
        orders: u64,
        trades: u64,
        payload_sha256: Option<&str>,
    ) {
        let parent = manifest.parent().expect("manifest parent");
        fs::create_dir_all(parent).expect("create manifest parent");
        let last_biz = first_biz + rows - 1;
        let mut channel_entry = json!({
            "channel_id": channel_id,
            "relative_path": format!("channels/{channel_id}/l2_mutations.ocb"),
            "row_count": rows,
            "row_group_count": row_groups,
            "first_biz_index": first_biz,
            "last_biz_index": last_biz,
            "min_receive_nano": 1_000_000_i64 + first_biz as i64,
            "max_receive_nano": 1_000_000_i64 + last_biz as i64,
            "order_record_count": orders,
            "trade_record_count": trades
        });
        if let Some(payload_sha256) = payload_sha256 {
            channel_entry["payload_sha256"] = json!(payload_sha256);
        }
        let manifest_json = json!({
            "schema_version": 1,
            "trading_day": trading_day,
            "artifact_format": COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
            "payload_width_bytes": COMPACT_L2_FIXED_BINARY_RECORD_WIDTH_V1,
            "selection_scope": "contiguous-prefix",
            "channel_indivisible": true,
            "counts": {"channels": 1, "row_count": rows, "order_records": orders, "trade_records": trades},
            "channels": [channel_entry],
            "claims": {"default_readiness": false, "runtime_readiness": false, "performance_dominance": false}
        });
        fs::write(manifest, serde_json::to_vec_pretty(&manifest_json).unwrap())
            .expect("write manifest");
    }

    fn write_compact_l2_fixture(
        path: &Path,
        trading_day: u32,
        channel_id: u32,
        biz_indexes: &[u64],
        record_kinds: &[u8],
        width: u32,
        override_channel_id: Option<u32>,
    ) {
        assert_eq!(biz_indexes.len(), record_kinds.len());
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create artifact parent");
        }
        let mut records = Vec::new();
        for (biz_index, record_kind) in biz_indexes
            .iter()
            .copied()
            .zip(record_kinds.iter().copied())
        {
            let actual_channel = override_channel_id.unwrap_or(channel_id);
            let full = compact_l2_record(trading_day, actual_channel, biz_index, record_kind);
            records.push(full[..width as usize].to_vec());
        }

        let mut file_bytes = vec![0u8; OCB_BOOTSTRAP_PAGE_V1_LEN];
        let mut row_groups = Vec::new();
        let mut chunk_descs = Vec::new();
        let row_group_size = 2usize;
        for (row_group_id, chunk_records) in records.chunks(row_group_size).enumerate() {
            let row_group_id = row_group_id as u32;
            let slices = chunk_records.iter().map(Vec::as_slice).collect::<Vec<_>>();
            let value_ref = append_fixed_binary_chunk(
                &mut file_bytes,
                row_group_id,
                0,
                width,
                &slices,
                OcbChunkCodecV1::None,
            );
            let row_count = chunk_records.len() as u64;
            row_groups.push(OcbRowGroupDescV1 {
                row_group_id,
                flags: 0,
                base_row: (row_group_id as u64) * row_group_size as u64,
                row_count,
                chunk_desc_begin: u64::from(row_group_id),
                chunk_desc_count: 1,
                stat_begin: 0,
                stat_count: 0,
                first_key_tuple_ref: OcbBodyRefV2::NULL,
                last_key_tuple_ref: OcbBodyRefV2::NULL,
            });
            chunk_descs.push(OcbColumnChunkDescV1 {
                row_group_id,
                column_id: 0,
                physical_type: OcbPhysicalTypeV1::FixedBinary,
                codec: OcbChunkCodecV1::None,
                flags: 0,
                value_ref,
                validity_ref: OcbBodyRefV2::NULL,
                row_count,
                uncompressed_bytes: row_count * u64::from(width),
            });
        }

        let string_table = OcbStringTableV1 {
            version: 1,
            strings: vec!["payload".into()],
            crc32c: 0,
        };
        let string_table_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::StringTable, |buf| {
                string_table.write_to(buf)
            });
        let schema = OcbSchemaV1 {
            version: 1,
            string_table_ref,
            columns: vec![fixed_binary_column_desc(0, 0, width)],
            crc32c: 0,
        };
        let schema_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Schema, |buf| {
            schema.write_to(buf)
        });
        let row_group_index = OcbRowGroupIndexV1 {
            version: 1,
            flags: 0,
            row_groups,
            column_chunks: chunk_descs,
            stats: Vec::new(),
            crc32c: 0,
        };
        let row_group_index_ref =
            append_encoded_object(&mut file_bytes, OcbBodyKindV1::RowGroupIndex, |buf| {
                row_group_index.write_to(buf)
            });
        let root = OcbRootV1 {
            version: 1,
            flags: 0,
            row_count: records.len() as u64,
            column_count: 1,
            row_group_count: ((records.len() + row_group_size - 1) / row_group_size) as u32,
            dictionary_count: 0,
            schema_ref,
            dictionary_index_ref: OcbBodyRefV2::NULL,
            row_group_index_ref,
            ordering_proof_ref: OcbBodyRefV2::NULL,
            debug_json_ref: OcbBodyRefV2::NULL,
            created_unix_nanos: 0,
            content_flags: 0,
            crc32c: 0,
        };
        let root_ref = append_encoded_object(&mut file_bytes, OcbBodyKindV1::Root, |buf| {
            root.write_to(buf)
        });
        assert_eq!(root_ref.length, OCB_ROOT_V1_LEN as u64);
        let bootstrap = OcbBootstrapPageV1::new([88u8; 16], root_ref);
        let mut bootstrap_bytes = Vec::new();
        bootstrap
            .write_to(&mut bootstrap_bytes)
            .expect("write bootstrap");
        file_bytes[..OCB_BOOTSTRAP_PAGE_V1_LEN].copy_from_slice(&bootstrap_bytes);
        let mut file = fs::File::create(path).expect("create compact L2 fixture");
        file.write_all(&file_bytes)
            .expect("write compact L2 fixture");
    }

    fn compact_l2_record(
        trading_day: u32,
        channel_id: u32,
        biz_index: u64,
        record_kind: u8,
    ) -> [u8; 168] {
        let mut bytes = [0u8; 168];
        bytes[0..4].copy_from_slice(b"ALIR");
        bytes[4..6].copy_from_slice(&1u16.to_le_bytes());
        bytes[6..8].copy_from_slice(&80u16.to_le_bytes());
        bytes[8..10].copy_from_slice(&168u16.to_le_bytes());
        bytes[10] = record_kind;
        bytes[11] = 1;
        bytes[12] = 1;
        bytes[16..20].copy_from_slice(&trading_day.to_le_bytes());
        bytes[20..24].copy_from_slice(&(channel_id as i32).to_le_bytes());
        bytes[24..32].copy_from_slice(&(biz_index as i64).to_le_bytes());
        bytes[32..40].copy_from_slice(&biz_index.to_le_bytes());
        bytes[40..48].copy_from_slice(&(1_000_000_i64 + biz_index as i64).to_le_bytes());
        bytes[48..56].copy_from_slice(&(2_000_000_i64 + biz_index as i64).to_le_bytes());
        bytes[56..65].copy_from_slice(b"600000.SH");
        bytes
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
            OcbChunkCodecV1::Zstd => {
                zstd::stream::encode_all(std::io::Cursor::new(raw_payload.as_slice()), 1)
                    .expect("compress fixed-binary chunk")
            }
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

    fn fixed_binary_column_desc(
        column_id: u32,
        name_string_id: u32,
        width: u32,
    ) -> OcbColumnDescV1 {
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
}
