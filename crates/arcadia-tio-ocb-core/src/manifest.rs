//! Channel-sharded OCB artifact manifest model and path-safe parsing.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::compact_l2::{
    CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1, COMPACT_L2_FIXED_BINARY_ARTIFACT_FORMAT_V1,
    COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
};
use crate::read::OcbReadSource;
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
        // `declared-subset` declares that a channel carries only the in-scope subset of the
        // exchange sequence: the absent keys are declared out-of-scope events rather than
        // lost rows, so a channel may hold fewer rows than its key span. It never holds
        // more, and it never starts at key 1 by requirement (that stays a prefix-scope rule).
        let declared_subset_scope = self.selection_scope.as_deref() == Some("declared-subset");
        for channel in &self.channels {
            channel.validate_manifest_only(prefix_scope, declared_subset_scope)?;
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
    fn validate_manifest_only(&self, prefix_scope: bool, declared_subset_scope: bool) -> Result<()> {
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
        // A dense scope must use every key of the span. A declared subset may leave keys
        // absent, but can never claim more rows than the span holds.
        if declared_subset_scope {
            if self.row_count > expected_rows {
                return invalid_manifest(
                    "channel-sharded OCB manifest BizIndex range row mismatch",
                );
            }
        } else if expected_rows != self.row_count {
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
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB artifact path contains a platform prefix",
        ));
    }
    if path
        .split(['/', '\\'])
        .any(|component| component.is_empty() || matches!(component, "." | ".."))
    {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB artifact path contains an unsafe lexical component",
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

/// One canonical manifest-root authority used to bind artifact handles.
#[derive(Debug)]
pub(crate) struct ManifestArtifactResolver {
    root_canonical: PathBuf,
    root_directory: File,
    #[cfg(windows)]
    root_final_path: Vec<u16>,
}

impl ManifestArtifactResolver {
    pub(crate) fn new(manifest_path: &Path) -> Result<Self> {
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
        let expected_metadata = fs::metadata(&root_canonical).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root metadata could not be read",
            )
        })?;
        if !expected_metadata.is_dir() {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root is not a directory",
            ));
        }
        let root_directory = open_manifest_root_directory(&root_canonical)?;
        verify_opened_manifest_root(&root_canonical, &root_directory, &expected_metadata)?;
        if fs::canonicalize(root).ok().as_deref() != Some(root_canonical.as_path()) {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root changed during binding",
            ));
        }
        #[cfg(windows)]
        let root_final_path = windows_final_path(&root_directory)?;
        Ok(Self {
            root_canonical,
            root_directory,
            #[cfg(windows)]
            root_final_path,
        })
    }

    pub(crate) fn open_artifact(&self, relative_path: &str) -> Result<OcbReadSource> {
        self.open_artifact_inner(relative_path, || {})
    }

    pub(crate) fn open_artifact_with_resolution_hook(
        &self,
        relative_path: &str,
        resolution_hook: impl FnOnce(),
    ) -> Result<OcbReadSource> {
        self.open_artifact_inner(relative_path, resolution_hook)
    }

    fn open_artifact_inner(
        &self,
        relative_path: &str,
        resolution_hook: impl FnOnce(),
    ) -> Result<OcbReadSource> {
        validate_manifest_relative_path(relative_path)?;
        self.verify_root_path_identity()?;
        let diagnostic_path = self.root_canonical.join(relative_path);
        let artifact_canonical = fs::canonicalize(&diagnostic_path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::MissingArtifact,
                    "channel-sharded OCB artifact is missing",
                )
            } else {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact could not be resolved safely",
                )
            }
        })?;
        self.verify_root_path_identity()?;
        let canonical_relative = artifact_canonical
            .strip_prefix(&self.root_canonical)
            .map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact path escapes manifest root",
                )
            })?;
        if canonical_relative.as_os_str().is_empty()
            || canonical_relative
                .components()
                .any(|component| !matches!(component, Component::Normal(_)))
        {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB artifact did not resolve to a regular root-relative path",
            ));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let expected = fs::metadata(&artifact_canonical).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::MissingArtifact,
                    "channel-sharded OCB artifact metadata could not be read",
                )
            })?;
            if !expected.is_file() {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact is not a regular file",
                ));
            }
            resolution_hook();
            let file = open_unix_artifact_beneath(&self.root_directory, canonical_relative)?;
            let opened = file.metadata().map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB opened artifact metadata could not be read",
                )
            })?;
            if !opened.is_file()
                || expected.dev() != opened.dev()
                || expected.ino() != opened.ino()
                || expected.len() != opened.len()
            {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact changed during checked open",
                ));
            }
            OcbReadSource::from_file(file)
        }

        #[cfg(windows)]
        {
            let expected_file = File::open(&artifact_canonical).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::MissingArtifact,
                    "channel-sharded OCB artifact could not be opened for identity capture",
                )
            })?;
            let expected_metadata = expected_file.metadata().map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact identity could not be captured",
                )
            })?;
            if !expected_metadata.is_file()
                || !windows_path_is_beneath(
                    &windows_final_path(&expected_file)?,
                    &self.root_final_path,
                )
            {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact handle escapes manifest root",
                ));
            }
            let expected_identity = windows_file_identity(&expected_file)?;
            let expected_len = expected_metadata.len();
            resolution_hook();
            let file = File::open(&artifact_canonical).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::MissingArtifact,
                    "channel-sharded OCB artifact could not be opened safely",
                )
            })?;
            let opened_metadata = file.metadata().map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB opened artifact identity could not be read",
                )
            })?;
            if !opened_metadata.is_file()
                || windows_file_identity(&file)? != expected_identity
                || opened_metadata.len() != expected_len
                || !windows_path_is_beneath(&windows_final_path(&file)?, &self.root_final_path)
            {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB artifact changed during checked open",
                ));
            }
            OcbReadSource::from_file(file)
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = resolution_hook;
            Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "checked manifest artifact opening is unsupported on this platform",
            ))
        }
    }

    fn verify_root_path_identity(&self) -> Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;

            let bound = self.root_directory.metadata().map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "bound manifest root metadata could not be read",
                )
            })?;
            let visible = fs::metadata(&self.root_canonical).map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "visible manifest root metadata could not be read",
                )
            })?;
            if !visible.is_dir() || bound.dev() != visible.dev() || bound.ino() != visible.ino() {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB manifest root changed after binding",
                ));
            }
            Ok(())
        }
        #[cfg(windows)]
        {
            let visible = open_manifest_root_directory(&self.root_canonical)?;
            if windows_file_identity(&visible)? != windows_file_identity(&self.root_directory)? {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "channel-sharded OCB manifest root changed after binding",
                ));
            }
            Ok(())
        }
        #[cfg(not(any(unix, windows)))]
        {
            Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "checked manifest root identity is unsupported on this platform",
            ))
        }
    }
}

#[cfg(unix)]
fn open_manifest_root_directory(path: &Path) -> Result<File> {
    File::open(path).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "channel-sharded OCB manifest root could not be opened",
        )
    })
}

#[cfg(windows)]
fn open_manifest_root_directory(path: &Path) -> Result<File> {
    use std::fs::OpenOptions;
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)
        .map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root could not be opened",
            )
        })
}

#[cfg(not(any(unix, windows)))]
fn open_manifest_root_directory(_path: &Path) -> Result<File> {
    Err(ArcadiaTioError::ocb_diagnostic(
        OcbErrorKind::UnsafeManifestPath,
        "checked manifest root opening is unsupported on this platform",
    ))
}

fn verify_opened_manifest_root(_path: &Path, opened: &File, expected: &fs::Metadata) -> Result<()> {
    let opened_metadata = opened.metadata().map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "opened manifest root metadata could not be read",
        )
    })?;
    if !opened_metadata.is_dir() {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "opened manifest root is not a directory",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;

        if expected.dev() != opened_metadata.dev() || expected.ino() != opened_metadata.ino() {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root changed during open",
            ));
        }
    }
    #[cfg(windows)]
    {
        let checked = open_manifest_root_directory(_path)?;
        if windows_file_identity(&checked)? != windows_file_identity(opened)? {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "channel-sharded OCB manifest root changed during open",
            ));
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (_path, expected);
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "checked manifest root identity is unsupported on this platform",
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn open_unix_artifact_beneath(root: &File, relative: &Path) -> Result<File> {
    use std::ffi::CString;
    use std::os::fd::{AsRawFd, FromRawFd};
    use std::os::unix::ffi::OsStrExt;

    let mut directory = root.try_clone().map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "bound manifest root descriptor could not be cloned",
        )
    })?;
    let mut components = relative.components().peekable();
    while let Some(component) = components.next() {
        let Component::Normal(component) = component else {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "resolved artifact path contains an unsafe component",
            ));
        };
        let component = CString::new(component.as_bytes()).map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "resolved artifact path contains an invalid component",
            )
        })?;
        let is_final = components.peek().is_none();
        let mut flags = libc::O_RDONLY | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NONBLOCK;
        if !is_final {
            flags |= libc::O_DIRECTORY;
        }
        // SAFETY: `directory` owns a live directory descriptor, `component` is
        // NUL-terminated, and a successful returned descriptor is immediately
        // transferred into exactly one `File` owner.
        let descriptor = unsafe { libc::openat(directory.as_raw_fd(), component.as_ptr(), flags) };
        if descriptor < 0 {
            return Err(classify_checked_open_error(std::io::Error::last_os_error()));
        }
        // SAFETY: `openat` returned a fresh owned descriptor above.
        let opened = unsafe { File::from_raw_fd(descriptor) };
        let metadata = opened.metadata().map_err(|_| {
            ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "checked artifact component metadata could not be read",
            )
        })?;
        if is_final {
            if !metadata.is_file() {
                return Err(ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "checked manifest artifact is not a regular file",
                ));
            }
            return Ok(opened);
        }
        if !metadata.is_dir() {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "checked manifest artifact component is not a directory",
            ));
        }
        directory = opened;
    }
    Err(ArcadiaTioError::ocb_diagnostic(
        OcbErrorKind::UnsafeManifestPath,
        "checked manifest artifact path is empty",
    ))
}

#[cfg(unix)]
fn classify_checked_open_error(error: std::io::Error) -> ArcadiaTioError {
    match error.raw_os_error() {
        Some(libc::ENOENT) => ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::MissingArtifact,
            "checked manifest artifact component is missing",
        ),
        Some(libc::ELOOP) | Some(libc::ENOTDIR) => ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "checked manifest artifact component changed or is a symlink",
        ),
        _ => ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "checked manifest artifact component could not be opened safely",
        ),
    }
}

#[cfg(windows)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowsFileIdentity {
    volume_serial_number: u32,
    file_index: u64,
}

#[cfg(windows)]
fn windows_file_identity(file: &File) -> Result<WindowsFileIdentity> {
    use std::ffi::c_void;
    use std::mem::MaybeUninit;
    use std::os::windows::io::AsRawHandle;

    #[allow(non_snake_case)]
    #[repr(C)]
    struct ByHandleFileInformation {
        dwFileAttributes: u32,
        ftCreationTimeLow: u32,
        ftCreationTimeHigh: u32,
        ftLastAccessTimeLow: u32,
        ftLastAccessTimeHigh: u32,
        ftLastWriteTimeLow: u32,
        ftLastWriteTimeHigh: u32,
        dwVolumeSerialNumber: u32,
        nFileSizeHigh: u32,
        nFileSizeLow: u32,
        nNumberOfLinks: u32,
        nFileIndexHigh: u32,
        nFileIndexLow: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            file: *mut c_void,
            information: *mut ByHandleFileInformation,
        ) -> i32;
    }
    let mut information = MaybeUninit::<ByHandleFileInformation>::uninit();
    // SAFETY: the raw handle belongs to `file` for the duration of the call and
    // the OS initializes `information` when it returns nonzero.
    let success = unsafe {
        GetFileInformationByHandle(file.as_raw_handle().cast(), information.as_mut_ptr())
    };
    if success == 0 {
        return Err(ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "Windows artifact file identity could not be queried",
        ));
    }
    // SAFETY: successful `GetFileInformationByHandle` initialized the value.
    let information = unsafe { information.assume_init() };
    Ok(WindowsFileIdentity {
        volume_serial_number: information.dwVolumeSerialNumber,
        file_index: (u64::from(information.nFileIndexHigh) << 32)
            | u64::from(information.nFileIndexLow),
    })
}

#[cfg(windows)]
fn windows_final_path(file: &File) -> Result<Vec<u16>> {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFinalPathNameByHandleW(
            file: *mut c_void,
            path: *mut u16,
            path_len: u32,
            flags: u32,
        ) -> u32;
    }
    const MAX_WINDOWS_FINAL_PATH_UNITS: usize = 32_768;
    let mut path = Vec::new();
    path.try_reserve_exact(1024).map_err(|_| {
        ArcadiaTioError::ocb_diagnostic(
            OcbErrorKind::UnsafeManifestPath,
            "Windows artifact final-path allocation failed",
        )
    })?;
    path.resize(1024, 0u16);
    loop {
        // SAFETY: `path` exposes `path.len()` initialized writable code units
        // and the raw handle remains valid for the duration of the call.
        let written = unsafe {
            GetFinalPathNameByHandleW(
                file.as_raw_handle().cast(),
                path.as_mut_ptr(),
                path.len() as u32,
                0,
            )
        } as usize;
        if written == 0 {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "Windows artifact final path could not be queried",
            ));
        }
        if written < path.len() {
            path.truncate(written);
            return Ok(path);
        }
        let needed = written.saturating_add(1);
        if needed > MAX_WINDOWS_FINAL_PATH_UNITS {
            return Err(ArcadiaTioError::ocb_diagnostic(
                OcbErrorKind::UnsafeManifestPath,
                "Windows artifact final path exceeds the safety bound",
            ));
        }
        path.try_reserve_exact(needed.saturating_sub(path.len()))
            .map_err(|_| {
                ArcadiaTioError::ocb_diagnostic(
                    OcbErrorKind::UnsafeManifestPath,
                    "Windows artifact final-path allocation failed",
                )
            })?;
        path.resize(needed, 0u16);
    }
}

#[cfg(windows)]
fn windows_path_is_beneath(path: &[u16], root: &[u16]) -> bool {
    let root_len = root
        .iter()
        .rposition(|unit| *unit != b'\\' as u16 && *unit != b'/' as u16)
        .map_or(0, |index| index + 1);
    path.len() > root_len
        && path[..root_len]
            .iter()
            .zip(&root[..root_len])
            .all(|(left, right)| windows_path_unit_eq(*left, *right))
        && matches!(path[root_len], unit if unit == b'\\' as u16 || unit == b'/' as u16)
}

#[cfg(windows)]
fn windows_path_unit_eq(left: u16, right: u16) -> bool {
    if left <= u16::from(u8::MAX) && right <= u16::from(u8::MAX) {
        (left as u8).eq_ignore_ascii_case(&(right as u8))
    } else {
        left == right
    }
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::Read;
    use std::path::{Path, PathBuf};

    use super::*;

    fn channel(
        channel_id: u32,
        row_count: u64,
        first_biz_index: u64,
        last_biz_index: u64,
    ) -> ChannelArtifactEntryV1 {
        ChannelArtifactEntryV1 {
            channel_id,
            relative_path: format!("channels/{channel_id}/artifact.ocb"),
            row_count,
            row_group_count: 0,
            first_biz_index,
            last_biz_index,
            min_receive_nano: None,
            max_receive_nano: None,
            order_record_count: Some(row_count),
            trade_record_count: Some(0),
            payload_sha256: None,
            fingerprint: None,
        }
    }

    fn manifest(
        scope: Option<&str>,
        channels: Vec<ChannelArtifactEntryV1>,
    ) -> ChannelShardedManifestV1 {
        let rows = channels.iter().map(|channel| channel.row_count).sum();
        ChannelShardedManifestV1 {
            schema_version: 1,
            trading_day: 20260901,
            artifact_format: "compact-l2-physical-v2".to_owned(),
            root_hash: None,
            payload_width_bytes: None,
            selection_scope: scope.map(ToOwned::to_owned),
            channel_indivisible: Some(true),
            counts: ChannelShardedManifestCountsV1 {
                channels: Some(channels.len()),
                row_count: Some(rows),
                order_records: Some(rows),
                trade_records: Some(0),
            },
            channels,
            claims: ChannelShardedManifestClaimsV1::default(),
        }
    }

    #[test]
    fn a_declared_subset_channel_may_leave_keys_absent_but_never_exceed_its_span() {
        // The scoped contract: keys 1..=8 with five absent (three present) is admissible.
        manifest(Some("declared-subset"), vec![channel(1, 3, 1, 8)])
            .validate()
            .expect("a declared subset channel with absent keys is admissible");
        // A late start is equally admissible, because the prefix rule belongs to full-day.
        manifest(Some("declared-subset"), vec![channel(1, 3, 663, 665)])
            .validate()
            .expect("a declared subset channel may start after key 1");
        // More rows than the span can hold is never admissible, in any scope.
        let error = manifest(Some("declared-subset"), vec![channel(1, 9, 1, 8)])
            .validate()
            .expect_err("more rows than keys must refuse");
        assert_eq!(
            OcbErrorKind::from_error(&error),
            Some(OcbErrorKind::InvalidManifest)
        );
    }

    #[test]
    fn a_dense_scope_still_requires_every_key_of_the_span() {
        // The historic scopes are unchanged: a hole refuses, and the prefix rule still applies.
        assert!(manifest(Some("full-day"), vec![channel(1, 3, 1, 8)]).validate().is_err());
        assert!(manifest(None, vec![channel(1, 3, 1, 8)]).validate().is_err());
        assert!(manifest(Some("full-day"), vec![channel(1, 3, 3, 5)]).validate().is_err());
        manifest(Some("full-day"), vec![channel(1, 3, 1, 3)])
            .validate()
            .expect("a dense full-day channel remains valid");
    }

    #[test]
    fn manifest_relative_path_validation_rejects_lexical_escape_forms() {
        for path in [
            "",
            "/absolute.ocb",
            "../escape.ocb",
            "./artifact.ocb",
            "channels//artifact.ocb",
            "channels/./artifact.ocb",
            "channels/../artifact.ocb",
            "channels/artifact.ocb/",
            r"C:\artifact.ocb",
            r"\\server\share\artifact.ocb",
        ] {
            let error = validate_manifest_relative_path(path)
                .expect_err("unsafe lexical manifest path must fail");
            assert_eq!(
                OcbErrorKind::from_error(&error),
                Some(OcbErrorKind::UnsafeManifestPath),
                "unexpected error kind for {path:?}"
            );
        }
        validate_manifest_relative_path("channels/2011/artifact.ocb")
            .expect("normal relative manifest path remains valid");
    }

    #[cfg(unix)]
    #[test]
    fn checked_resolver_accepts_in_root_symlinks_and_rejects_escapes() {
        use std::os::unix::fs::symlink;

        let root = fixture_root("symlink_policy");
        let outside = fixture_root("symlink_policy_outside");
        let manifest = root.join("manifest.json");
        fs::write(&manifest, b"{}").expect("write manifest placeholder");
        let target = root.join("actual/2011/artifact.ocb");
        let outside_target = outside.join("artifact.ocb");
        fs::create_dir_all(target.parent().unwrap()).expect("create in-root target parent");
        fs::write(&target, b"in-root-artifact").expect("write in-root target");
        fs::write(&outside_target, b"outside-artifact").expect("write outside target");

        let root_canonical = fs::canonicalize(&root).expect("canonical fixture root");
        let outside_canonical = fs::canonicalize(&outside).expect("canonical outside root");
        let target_canonical = fs::canonicalize(&target).expect("canonical in-root target");
        let outside_target_canonical =
            fs::canonicalize(&outside_target).expect("canonical outside target");
        symlink(root_canonical.join("actual"), root.join("channels"))
            .expect("create in-root intermediate symlink");
        symlink(&target_canonical, root.join("final-safe.ocb"))
            .expect("create in-root final symlink");
        symlink(&outside_canonical, root.join("escaping-channels"))
            .expect("create escaping intermediate symlink");
        symlink(&outside_target_canonical, root.join("final-escape.ocb"))
            .expect("create escaping final symlink");

        let resolver = ManifestArtifactResolver::new(&manifest).expect("bind manifest root");
        let intermediate_source = resolver
            .open_artifact("channels/2011/artifact.ocb")
            .expect("in-root intermediate symlink is supported");
        let mut through_intermediate = intermediate_source.cursor();
        let mut bytes = Vec::new();
        through_intermediate
            .read_to_end(&mut bytes)
            .expect("read bound intermediate target");
        assert_eq!(bytes, b"in-root-artifact");

        resolver
            .open_artifact("final-safe.ocb")
            .expect("in-root final symlink is supported");
        for path in ["escaping-channels/artifact.ocb", "final-escape.ocb"] {
            let error = resolver
                .open_artifact(path)
                .expect_err("escaping symlink must fail closed");
            assert_eq!(
                OcbErrorKind::from_error(&error),
                Some(OcbErrorKind::UnsafeManifestPath)
            );
        }

        cleanup_root(&root);
        cleanup_root(&outside);
    }

    #[cfg(unix)]
    #[test]
    fn checked_resolver_descriptor_lifetime_is_bounded_by_live_sources() {
        let root = fixture_root("descriptor_lifetime");
        let manifest = root.join("manifest.json");
        let artifact = root.join("artifact.ocb");
        fs::write(&manifest, b"{}").expect("write manifest placeholder");
        fs::write(&artifact, b"descriptor-fixture").expect("write artifact fixture");
        let resolver = ManifestArtifactResolver::new(&manifest).expect("bind manifest root");
        let baseline = descriptor_count();

        let sources = (0..128)
            .map(|_| {
                resolver
                    .open_artifact("artifact.ocb")
                    .expect("bind artifact")
            })
            .collect::<Vec<_>>();
        if let (Some(baseline), Some(during)) = (baseline, descriptor_count()) {
            assert!(
                during <= baseline + 130,
                "unexpected descriptor amplification"
            );
        }
        drop(sources);
        if let (Some(baseline), Some(after)) = (baseline, descriptor_count()) {
            assert!(
                after <= baseline + 2,
                "bound source descriptors did not close"
            );
        }

        cleanup_root(&root);
    }

    fn fixture_root(name: &str) -> PathBuf {
        let root = PathBuf::from(".tmp/ocb-manifest-resolver-tests")
            .join(format!("{name}-{}", std::process::id()));
        cleanup_root(&root);
        fs::create_dir_all(&root).expect("create manifest resolver fixture root");
        root
    }

    fn cleanup_root(path: &Path) {
        let _ = fs::remove_dir_all(path);
    }

    #[cfg(unix)]
    fn descriptor_count() -> Option<usize> {
        ["/proc/self/fd", "/dev/fd"]
            .into_iter()
            .find_map(|path| fs::read_dir(path).ok().map(|entries| entries.count()))
    }
}
