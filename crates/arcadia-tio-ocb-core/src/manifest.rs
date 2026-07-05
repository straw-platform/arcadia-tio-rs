//! Channel-sharded OCB artifact manifest model and path-safe parsing.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::compact_l2::{
    CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1, COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
    COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
};
use crate::{ArcadiaTioError, OcbErrorKind, Result};

/// Optional legacy file fingerprint metadata accepted during migration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelArtifactFingerprintV1 {
    /// File size in bytes.
    #[serde(default)]
    pub file_bytes: Option<u64>,
    /// Last modified timestamp in UNIX nanoseconds.
    #[serde(default)]
    pub modified_unix_ns: Option<u64>,
    /// Legacy single-stream FNV-1a64 content hash, lowercase hex.
    #[serde(default)]
    pub content_hash_fnv1a64: Option<String>,
}

/// One channel artifact entry in a channel-sharded compact-L2 manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelArtifactEntryV1 {
    /// Positive source ChannelID. Each channel appears at most once.
    pub channel_id: u32,
    /// Manifest-relative artifact path.
    #[serde(alias = "artifact")]
    pub relative_path: String,
    /// Total logical rows in the channel artifact.
    #[serde(alias = "rows")]
    pub row_count: u64,
    /// Number of OCB row groups in the channel artifact. Legacy manifests may
    /// omit this field; certification treats zero as unspecified.
    #[serde(default)]
    pub row_group_count: u32,
    /// First channel-local BizIndex in this artifact.
    pub first_biz_index: u64,
    /// Last channel-local BizIndex in this artifact.
    pub last_biz_index: u64,
    /// Minimum/first receive nano recorded for this channel, if available.
    #[serde(alias = "first_receive_nano", default)]
    pub min_receive_nano: Option<i64>,
    /// Maximum/last receive nano recorded for this channel, if available.
    #[serde(alias = "last_receive_nano", default)]
    pub max_receive_nano: Option<i64>,
    /// Optional order row count for diagnostics and certification reports.
    #[serde(default, alias = "order_records")]
    pub order_record_count: Option<u64>,
    /// Optional trade row count for diagnostics and certification reports.
    #[serde(default, alias = "trade_records")]
    pub trade_record_count: Option<u64>,
    /// Optional SHA-256 artifact hash as lowercase hex.
    #[serde(alias = "payload_hash", alias = "payload_hash_hex", default)]
    pub payload_sha256: Option<String>,
    /// Optional legacy fingerprint metadata.
    #[serde(default)]
    pub fingerprint: Option<ChannelArtifactFingerprintV1>,
}

/// Optional aggregate manifest counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChannelShardedManifestCountsV1 {
    /// Channel count.
    #[serde(default)]
    pub channels: Option<usize>,
    /// Aggregate row count.
    #[serde(default, alias = "rows")]
    pub row_count: Option<u64>,
    /// Aggregate order row count.
    #[serde(default)]
    pub order_records: Option<u64>,
    /// Aggregate trade row count.
    #[serde(default)]
    pub trade_records: Option<u64>,
}

/// Explicit non-readiness/performance claim flags preserved for path-safe reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ChannelShardedManifestClaimsV1 {
    /// Must remain false for certification manifests.
    #[serde(default)]
    pub default_readiness: bool,
    /// Must remain false for certification manifests.
    #[serde(default)]
    pub runtime_readiness: bool,
    /// Must remain false for certification manifests.
    #[serde(default)]
    pub performance_dominance: bool,
}

/// Channel-sharded compact-L2 OCB artifact manifest, version 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelShardedManifestV1 {
    /// Manifest schema version. JSON may use integer `1` or the legacy string
    /// `arcadia-lob-ocb-channel-sharded-artifact-manifest/v1`.
    #[serde(deserialize_with = "deserialize_manifest_schema_version")]
    pub schema_version: u16,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Artifact format/layout label. The manifest model can describe either
    /// compact fixed-binary v1 or compact-L2 physical-v2 artifacts; each
    /// certification entry point still validates the exact layout it accepts.
    #[serde(alias = "layout", default = "default_artifact_format")]
    pub artifact_format: String,
    /// Optional aggregate root hash as lowercase hex.
    #[serde(default)]
    pub root_hash: Option<String>,
    /// Optional payload width in bytes. If present, certification checks it.
    #[serde(default)]
    pub payload_width_bytes: Option<u32>,
    /// Optional selection scope label, e.g. `full-day` or `contiguous-prefix`.
    #[serde(default)]
    pub selection_scope: Option<String>,
    /// Whether channels are declared indivisible in this artifact.
    #[serde(default)]
    pub channel_indivisible: Option<bool>,
    /// Optional aggregate counts.
    #[serde(default)]
    pub counts: ChannelShardedManifestCountsV1,
    /// Per-channel artifact entries.
    pub channels: Vec<ChannelArtifactEntryV1>,
    /// Optional non-readiness claim flags. If present, readiness/performance
    /// assertions must be false.
    #[serde(default)]
    pub claims: ChannelShardedManifestClaimsV1,
}

impl ChannelShardedManifestV1 {
    /// Parse a manifest from a JSON file.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self> {
        let text = std::fs::read_to_string(path.as_ref()).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::MissingArtifact,
                    "manifest file is missing",
                )
            } else {
                ArcadiaTioError::ocb_diagnostic(OcbErrorKind::Io, "manifest file could not be read")
            }
        })?;
        let manifest: Self = serde_json::from_str(&text).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::InvalidManifest,
                "channel-sharded OCB manifest JSON is invalid",
            )
        })?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Return per-channel entries.
    pub fn channels(&self) -> &[ChannelArtifactEntryV1] {
        &self.channels
    }

    /// Validate manifest-only invariants without opening artifact files.
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1 {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsupportedSchemaVersion,
                format!(
                    "unsupported channel-sharded OCB manifest schema version: {}",
                    self.schema_version
                ),
            ));
        }
        if self.trading_day == 0 {
            return invalid_manifest("channel-sharded OCB manifest trading day is zero");
        }
        if !is_supported_artifact_format(&self.artifact_format) {
            return invalid_manifest("unsupported channel-sharded OCB artifact format");
        }
        if let Some(false) = self.channel_indivisible {
            return invalid_manifest(
                "channel-sharded OCB manifest must declare channel_indivisible = true",
            );
        }
        if self.claims.default_readiness
            || self.claims.runtime_readiness
            || self.claims.performance_dominance
        {
            return invalid_manifest(
                "channel-sharded OCB manifest must not assert readiness/performance claims",
            );
        }
        if self.channels.is_empty() {
            return invalid_manifest("channel-sharded OCB manifest has no channels");
        }
        if let Some(expected) = self.counts.channels {
            if expected != self.channels.len() {
                return invalid_manifest("channel-sharded OCB manifest channel count mismatch");
            }
        }

        let mut seen_channels = BTreeSet::new();
        let mut last_channel_id = None::<u32>;
        let mut rows = 0u64;
        let mut order_rows = 0u64;
        let mut trade_rows = 0u64;
        let prefix_scope = matches!(
            self.selection_scope.as_deref(),
            Some("full-day" | "contiguous-prefix")
        );
        for channel in &self.channels {
            channel.validate_manifest_only(prefix_scope)?;
            if !seen_channels.insert(channel.channel_id) {
                return invalid_manifest("channel-sharded OCB manifest has duplicate ChannelID");
            }
            if let Some(previous) = last_channel_id {
                if channel.channel_id <= previous {
                    return invalid_manifest(
                        "channel-sharded OCB manifest channels must be sorted by ChannelID",
                    );
                }
            }
            last_channel_id = Some(channel.channel_id);
            rows = rows.checked_add(channel.row_count).ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::InvalidManifest,
                    "channel-sharded OCB manifest aggregate row count overflows",
                )
            })?;
            if let Some(value) = channel.order_record_count {
                order_rows = order_rows.saturating_add(value);
            }
            if let Some(value) = channel.trade_record_count {
                trade_rows = trade_rows.saturating_add(value);
            }
        }
        if let Some(expected) = self.counts.row_count {
            if expected != rows {
                return invalid_manifest(
                    "channel-sharded OCB manifest aggregate row count mismatch",
                );
            }
        }
        if let Some(expected) = self.counts.order_records {
            if expected != order_rows {
                return invalid_manifest(
                    "channel-sharded OCB manifest aggregate order count mismatch",
                );
            }
        }
        if let Some(expected) = self.counts.trade_records {
            if expected != trade_rows {
                return invalid_manifest(
                    "channel-sharded OCB manifest aggregate trade count mismatch",
                );
            }
        }
        Ok(())
    }

    /// Produce a deterministic path-free manifest summary.
    pub fn safe_summary(&self) -> SafeManifestSummary {
        SafeManifestSummary {
            schema_version: self.schema_version,
            trading_day: self.trading_day,
            artifact_format: self.artifact_format.clone(),
            channel_count: self.channels.len(),
            row_count: self.channels.iter().map(|channel| channel.row_count).sum(),
            row_group_count: self
                .channels
                .iter()
                .map(|channel| u64::from(channel.row_group_count))
                .sum(),
            first_channel_id: self.channels.first().map(|channel| channel.channel_id),
            last_channel_id: self.channels.last().map(|channel| channel.channel_id),
            path_redacted: true,
        }
    }
}

impl ChannelArtifactEntryV1 {
    fn validate_manifest_only(&self, prefix_scope: bool) -> Result<()> {
        if self.channel_id == 0 {
            return invalid_manifest("channel-sharded OCB manifest has invalid ChannelID");
        }
        validate_manifest_relative_path(&self.relative_path)?;
        if self.row_count == 0 {
            return invalid_manifest("channel-sharded OCB manifest channel has zero rows");
        }
        if self.first_biz_index == 0 || self.last_biz_index < self.first_biz_index {
            return invalid_manifest("channel-sharded OCB manifest BizIndex range is invalid");
        }
        if prefix_scope && self.first_biz_index != 1 {
            return invalid_manifest(
                "channel-sharded OCB prefix manifest channel must start at BizIndex 1",
            );
        }
        let expected_rows = self
            .last_biz_index
            .checked_sub(self.first_biz_index)
            .and_then(|span| span.checked_add(1))
            .ok_or_else(|| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::InvalidManifest,
                    "channel-sharded OCB manifest BizIndex range overflows",
                )
            })?;
        if expected_rows != self.row_count {
            return invalid_manifest("channel-sharded OCB manifest BizIndex range row mismatch");
        }
        if let (Some(order), Some(trade)) = (self.order_record_count, self.trade_record_count) {
            if order.saturating_add(trade) != self.row_count {
                return invalid_manifest("channel-sharded OCB manifest order/trade count mismatch");
            }
        }
        if let Some(hash) = &self.payload_sha256 {
            validate_hex_hash(hash, 64, OcbErrorKind::InvalidManifest)?;
        }
        if let Some(fingerprint) = &self.fingerprint {
            if let Some(hash) = &fingerprint.content_hash_fnv1a64 {
                validate_hex_hash(hash, 16, OcbErrorKind::InvalidManifest)?;
            }
        }
        Ok(())
    }
}

/// Path-free manifest summary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SafeManifestSummary {
    /// Manifest schema version.
    pub schema_version: u16,
    /// Trading day as `YYYYMMDD`.
    pub trading_day: u32,
    /// Artifact format/layout label.
    pub artifact_format: String,
    /// Number of channels.
    pub channel_count: usize,
    /// Aggregate manifest row count.
    pub row_count: u64,
    /// Aggregate manifest row-group count; zero when legacy entries omit counts.
    pub row_group_count: u64,
    /// First channel id in deterministic manifest order.
    pub first_channel_id: Option<u32>,
    /// Last channel id in deterministic manifest order.
    pub last_channel_id: Option<u32>,
    /// Always true: raw paths are excluded from this summary.
    pub path_redacted: bool,
}

/// Validate a manifest-relative artifact path without touching the filesystem.
pub fn validate_manifest_relative_path(path: &str) -> Result<()> {
    let relative = Path::new(path);
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB artifact path must be non-empty and relative",
        ));
    }
    for component in relative.components() {
        if !matches!(component, Component::Normal(_)) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB artifact path contains an unsafe component",
            ));
        }
    }
    Ok(())
}

/// Resolve a manifest-relative artifact path against the manifest's parent.
pub fn resolve_manifest_relative_artifact_path(
    manifest_path: impl AsRef<Path>,
    relative_path: &str,
) -> Result<PathBuf> {
    validate_manifest_relative_path(relative_path)?;
    let manifest_path = manifest_path.as_ref();
    let root = manifest_path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    Ok(root.join(relative_path))
}

pub(crate) fn validate_hex_hash(hash: &str, expected_len: usize, kind: OcbErrorKind) -> Result<()> {
    if hash.len() != expected_len || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ArcadiaTioError::ocb_diagnostic(
            kind,
            "channel-sharded OCB manifest hash is invalid",
        ));
    }
    Ok(())
}

fn invalid_manifest(message: &'static str) -> Result<()> {
    Err(ArcadiaTioError::ocb_diagnostic(
        OcbErrorKind::InvalidManifest,
        message,
    ))
}

fn default_artifact_format() -> String {
    COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1.to_owned()
}

fn is_supported_artifact_format(value: &str) -> bool {
    matches!(
        value,
        COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1 | COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT
    )
}

fn deserialize_manifest_schema_version<'de, D>(
    deserializer: D,
) -> std::result::Result<u16, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum VersionRepr {
        U16(u16),
        String(String),
    }

    match VersionRepr::deserialize(deserializer)? {
        VersionRepr::U16(version) => Ok(version),
        VersionRepr::String(value) => match value.as_str() {
            "arcadia-lob-ocb-channel-sharded-artifact-manifest/v1" => {
                Ok(CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1)
            }
            other => other.parse::<u16>().or(Ok(0)),
        },
    }
}
