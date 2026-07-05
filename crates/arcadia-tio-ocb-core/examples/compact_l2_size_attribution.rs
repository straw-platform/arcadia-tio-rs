use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use arcadia_tio_ocb_core::{
    COMPACT_L2_PAYLOAD_COLUMN_NAME, CertificationOptions, ChannelArtifactEntryV1,
    ChannelShardedManifestV1, ColumnBundleColumnChunkSummary, ColumnBundleColumnChunkSummaryCodec,
    ColumnBundleFile, ColumnBundleReadCursorOptions, ColumnBundleReadOptions,
    ColumnBundleReadRequest, ColumnBundleVisitControl, ColumnProjection, SafeCertificationSummary,
    SafeManifestSummary, certify_channel_sharded_artifact_v1,
};
use serde::{Deserialize, Serialize};

type AppResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const REPORT_SCHEMA_VERSION: &str = "arcadia-tio.compact-l2-size-attribution.v1";

fn main() -> AppResult<()> {
    let args = Args::parse(env::args_os().skip(1))?;
    let report = build_report(&args)?;
    write_report(&args, &report)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Args {
    manifest: PathBuf,
    json_out: Option<PathBuf>,
    csv_prefix: Option<PathBuf>,
    read_sample_row_groups: usize,
    read_threads: usize,
    max_in_flight_row_groups: usize,
    read_sample_projection: SampleProjection,
    include_row_groups: bool,
    certify_metadata: bool,
    pretty_json: bool,
}

impl Args {
    fn parse<I>(raw_args: I) -> AppResult<Self>
    where
        I: IntoIterator<Item = std::ffi::OsString>,
    {
        let mut config = None;
        let mut manifest = None;
        let mut json_out = None;
        let mut csv_prefix = None;
        let mut read_sample_row_groups = 0usize;
        let mut read_threads = 1usize;
        let mut max_in_flight_row_groups = 1usize;
        let mut read_sample_projection = SampleProjection::Payload;
        let mut include_row_groups = true;
        let mut certify_metadata = false;
        let mut pretty_json = true;

        let mut args = raw_args.into_iter();
        while let Some(arg) = args.next() {
            let Some(arg) = arg.to_str() else {
                return Err("arguments must be valid UTF-8".into());
            };
            match arg {
                "-h" | "--help" => {
                    print_usage();
                    std::process::exit(0);
                }
                "--config" => config = Some(next_path(&mut args, "--config")?),
                "--manifest" => manifest = Some(next_path(&mut args, "--manifest")?),
                "--json-out" => json_out = Some(next_path(&mut args, "--json-out")?),
                "--csv-prefix" => csv_prefix = Some(next_path(&mut args, "--csv-prefix")?),
                "--read-sample-row-groups" => {
                    read_sample_row_groups = next_usize(&mut args, "--read-sample-row-groups")?
                }
                "--read-threads" => read_threads = next_usize(&mut args, "--read-threads")?,
                "--max-in-flight-row-groups" => {
                    max_in_flight_row_groups = next_usize(&mut args, "--max-in-flight-row-groups")?
                }
                "--read-sample-projection" => {
                    let value = next_string(&mut args, "--read-sample-projection")?;
                    read_sample_projection = SampleProjection::parse(&value)?;
                }
                "--certify-metadata" => certify_metadata = true,
                "--no-row-groups" => include_row_groups = false,
                "--no-certify-metadata" => certify_metadata = false,
                "--compact-json" => pretty_json = false,
                other if other.starts_with('-') => {
                    return Err(format!("unknown argument {other:?}").into());
                }
                path => {
                    if manifest.is_some() {
                        return Err("manifest path was provided more than once".into());
                    }
                    manifest = Some(PathBuf::from(path));
                }
            }
        }

        if manifest.is_none() {
            if let Some(config) = config {
                let text = fs::read_to_string(&config)?;
                let config: ArgsConfig = serde_json::from_str(&text)?;
                manifest = Some(config.manifest);
            }
        }

        let Some(manifest) = manifest else {
            print_usage();
            return Err("missing --manifest <path> or --config <path>".into());
        };
        if read_threads == 0 {
            return Err("--read-threads must be greater than zero".into());
        }
        if max_in_flight_row_groups == 0 {
            return Err("--max-in-flight-row-groups must be greater than zero".into());
        }

        Ok(Self {
            manifest,
            json_out,
            csv_prefix,
            read_sample_row_groups,
            read_threads,
            max_in_flight_row_groups,
            read_sample_projection,
            include_row_groups,
            certify_metadata,
            pretty_json,
        })
    }
}

fn next_path<I>(args: &mut I, flag: &str) -> AppResult<PathBuf>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    Ok(PathBuf::from(next_string(args, flag)?))
}

fn next_usize<I>(args: &mut I, flag: &str) -> AppResult<usize>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let value = next_string(args, flag)?;
    value
        .parse::<usize>()
        .map_err(|err| format!("invalid {flag} value {value:?}: {err}").into())
}

fn next_string<I>(args: &mut I, flag: &str) -> AppResult<String>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    args.next()
        .ok_or_else(|| format!("{flag} requires a value").into())
        .and_then(|value| {
            value
                .into_string()
                .map_err(|_| format!("{flag} value must be valid UTF-8").into())
        })
}

fn print_usage() {
    eprintln!(
        "usage: cargo run -p arcadia-tio-ocb-core --example compact_l2_size_attribution -- \\\n             [--manifest manifest.json | --config run-local-config.json] \\\n             [--json-out redacted-summary.json] [--csv-prefix redacted-summary] \\\n             [--read-sample-row-groups N] [--read-sample-projection payload|all] \\\n             [--read-threads N] [--max-in-flight-row-groups N] \\\n             [--no-row-groups] [--certify-metadata]\n\nThe report is path-redacted and contains aggregate OCB size/read attribution only."
    );
}

#[derive(Debug, Deserialize)]
struct ArgsConfig {
    manifest: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum SampleProjection {
    Payload,
    All,
}

impl SampleProjection {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "payload" | "payload-only" => Ok(Self::Payload),
            "all" | "scalar+payload" | "scalar-plus-payload" => Ok(Self::All),
            other => Err(format!(
                "unknown --read-sample-projection {other:?}; expected payload or all"
            )
            .into()),
        }
    }
}

#[derive(Debug, Serialize)]
struct SizeAttributionReport {
    schema_version: &'static str,
    manifest: ManifestReport,
    metadata_certification: Option<CertificationReportSummary>,
    totals: TotalsReport,
    artifacts: Vec<ArtifactReport>,
    columns: Vec<ColumnReport>,
    row_groups: Vec<RowGroupReport>,
    read_attribution_samples: Vec<ReadAttributionSampleReport>,
    safety: SafetyReport,
    recommendation: RecommendationReport,
}

#[derive(Debug, Serialize)]
struct ManifestReport {
    schema_version: u16,
    trading_day: u32,
    artifact_format: String,
    channel_count: usize,
    manifest_row_count: u64,
    manifest_row_group_count: u64,
    first_channel_id: Option<u32>,
    last_channel_id: Option<u32>,
    path_redacted: bool,
}

impl From<SafeManifestSummary> for ManifestReport {
    fn from(value: SafeManifestSummary) -> Self {
        Self {
            schema_version: value.schema_version,
            trading_day: value.trading_day,
            artifact_format: value.artifact_format,
            channel_count: value.channel_count,
            manifest_row_count: value.row_count,
            manifest_row_group_count: value.row_group_count,
            first_channel_id: value.first_channel_id,
            last_channel_id: value.last_channel_id,
            path_redacted: value.path_redacted,
        }
    }
}

#[derive(Debug, Serialize)]
struct CertificationReportSummary {
    schema_version: u16,
    trading_day: u32,
    artifact_format: String,
    channel_count: usize,
    row_count: u64,
    row_group_count: u64,
    certified: bool,
    payload_header_scan: bool,
    optional_hash_scan: bool,
    path_redacted: bool,
}

impl From<SafeCertificationSummary> for CertificationReportSummary {
    fn from(value: SafeCertificationSummary) -> Self {
        Self {
            schema_version: value.schema_version,
            trading_day: value.trading_day,
            artifact_format: value.artifact_format,
            channel_count: value.channel_count,
            row_count: value.row_count,
            row_group_count: value.row_group_count,
            certified: value.certified,
            payload_header_scan: false,
            optional_hash_scan: false,
            path_redacted: value.path_redacted,
        }
    }
}

#[derive(Debug, Serialize)]
struct TotalsReport {
    artifact_count: usize,
    row_count: u64,
    row_group_count: u64,
    column_chunk_count: u64,
    file_bytes: u64,
    compressed_value_bytes: u64,
    uncompressed_value_bytes: u64,
    payload_compressed_bytes: u64,
    payload_uncompressed_bytes: u64,
    scalar_compressed_bytes: u64,
    scalar_uncompressed_bytes: u64,
    container_overhead_bytes: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
    file_bytes_per_row: f64,
    compressed_value_bytes_per_row: f64,
    payload_compressed_bytes_per_row: f64,
    scalar_compressed_bytes_per_row: f64,
    container_overhead_bytes_per_row: f64,
    payload_share_of_file_bytes: f64,
    scalar_share_of_file_bytes: f64,
    container_overhead_share_of_file_bytes: f64,
    value_compression_ratio: Option<f64>,
}

#[derive(Debug, Serialize)]
struct ArtifactReport {
    artifact_index: usize,
    channel_id: u32,
    file_bytes: u64,
    row_count: u64,
    row_group_count: u32,
    column_chunk_count: u32,
    compressed_value_bytes: u64,
    uncompressed_value_bytes: u64,
    payload_compressed_bytes: u64,
    payload_uncompressed_bytes: u64,
    scalar_compressed_bytes: u64,
    scalar_uncompressed_bytes: u64,
    container_overhead_bytes: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
    file_bytes_per_row: f64,
    compressed_value_bytes_per_row: f64,
    payload_compressed_bytes_per_row: f64,
}

#[derive(Debug, Serialize)]
struct ColumnReport {
    column_name: String,
    bucket: &'static str,
    physical_type: String,
    fixed_binary_width: Option<u32>,
    row_count: u64,
    chunk_count: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
    compressed_bytes_per_row: f64,
    uncompressed_bytes_per_row: f64,
    compression_ratio: Option<f64>,
    share_of_file_bytes: f64,
}

#[derive(Debug, Serialize)]
struct RowGroupReport {
    artifact_index: usize,
    channel_id: u32,
    row_group_id: u32,
    base_row: u64,
    row_count: u64,
    compressed_value_bytes: u64,
    uncompressed_value_bytes: u64,
    payload_compressed_bytes: u64,
    payload_uncompressed_bytes: u64,
    scalar_compressed_bytes: u64,
    scalar_uncompressed_bytes: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
    compressed_value_bytes_per_row: f64,
    payload_compressed_bytes_per_row: f64,
}

#[derive(Debug, Serialize)]
struct ReadAttributionSampleReport {
    artifact_index: usize,
    channel_id: u32,
    projection: SampleProjection,
    requested_row_groups: usize,
    yielded_row_groups: usize,
    yielded_rows: u64,
    selected_column_chunks: usize,
    effective_threads: usize,
    max_in_flight_row_groups_observed: usize,
    fallback_reason: Option<&'static str>,
    plan_ns: u64,
    execute_wall_ns: u64,
    callback_wall_ns: u64,
    row_group_read_ns: u64,
    read_io_ns: u64,
    checksum_ns: u64,
    decompression_ns: u64,
    primitive_decode_ns: u64,
    fixed_payload_decode_ns: u64,
    copy_materialization_ns: u64,
    bytes_read: u64,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
}

#[derive(Debug, Serialize)]
struct SafetyReport {
    path_redacted: bool,
    raw_payload_bytes_included: bool,
    raw_records_included: bool,
    private_paths_included: bool,
    claim_scope: &'static str,
    limitations: &'static str,
}

#[derive(Debug, Serialize)]
struct RecommendationReport {
    next_experiment: &'static str,
    reason: String,
}

#[derive(Debug, Default)]
struct MutableTotals {
    row_count: u64,
    row_group_count: u64,
    column_chunk_count: u64,
    file_bytes: u64,
    compressed_value_bytes: u64,
    uncompressed_value_bytes: u64,
    payload_compressed_bytes: u64,
    payload_uncompressed_bytes: u64,
    scalar_compressed_bytes: u64,
    scalar_uncompressed_bytes: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
}

impl MutableTotals {
    fn add_chunk(&mut self, chunk: &ColumnBundleColumnChunkSummary) {
        self.column_chunk_count = self.column_chunk_count.saturating_add(1);
        self.compressed_value_bytes = self
            .compressed_value_bytes
            .saturating_add(chunk.compressed_bytes);
        self.uncompressed_value_bytes = self
            .uncompressed_value_bytes
            .saturating_add(chunk.uncompressed_bytes);
        match chunk.codec {
            ColumnBundleColumnChunkSummaryCodec::Zstd => {
                self.zstd_chunk_count = self.zstd_chunk_count.saturating_add(1)
            }
            ColumnBundleColumnChunkSummaryCodec::None => {
                self.none_chunk_count = self.none_chunk_count.saturating_add(1)
            }
        }
        if chunk.column_name == COMPACT_L2_PAYLOAD_COLUMN_NAME {
            self.payload_compressed_bytes = self
                .payload_compressed_bytes
                .saturating_add(chunk.compressed_bytes);
            self.payload_uncompressed_bytes = self
                .payload_uncompressed_bytes
                .saturating_add(chunk.uncompressed_bytes);
        } else {
            self.scalar_compressed_bytes = self
                .scalar_compressed_bytes
                .saturating_add(chunk.compressed_bytes);
            self.scalar_uncompressed_bytes = self
                .scalar_uncompressed_bytes
                .saturating_add(chunk.uncompressed_bytes);
        }
    }
}

#[derive(Debug, Default)]
struct MutableColumnReport {
    column_name: String,
    physical_type: String,
    fixed_binary_width: Option<u32>,
    row_count: u64,
    chunk_count: u64,
    zstd_chunk_count: u64,
    none_chunk_count: u64,
    compressed_bytes: u64,
    uncompressed_bytes: u64,
}

impl MutableColumnReport {
    fn add_chunk(&mut self, chunk: &ColumnBundleColumnChunkSummary) {
        if self.column_name.is_empty() {
            self.column_name = chunk.column_name.clone();
            self.physical_type = format!("{:?}", chunk.physical_type);
            self.fixed_binary_width = chunk.fixed_binary_width;
        }
        self.row_count = self.row_count.saturating_add(chunk.row_count);
        self.chunk_count = self.chunk_count.saturating_add(1);
        self.compressed_bytes = self.compressed_bytes.saturating_add(chunk.compressed_bytes);
        self.uncompressed_bytes = self
            .uncompressed_bytes
            .saturating_add(chunk.uncompressed_bytes);
        match chunk.codec {
            ColumnBundleColumnChunkSummaryCodec::Zstd => {
                self.zstd_chunk_count = self.zstd_chunk_count.saturating_add(1)
            }
            ColumnBundleColumnChunkSummaryCodec::None => {
                self.none_chunk_count = self.none_chunk_count.saturating_add(1)
            }
        }
    }

    fn finish(self, total_file_bytes: u64) -> ColumnReport {
        ColumnReport {
            bucket: bucket_for_column(&self.column_name),
            column_name: self.column_name,
            physical_type: self.physical_type,
            fixed_binary_width: self.fixed_binary_width,
            row_count: self.row_count,
            chunk_count: self.chunk_count,
            zstd_chunk_count: self.zstd_chunk_count,
            none_chunk_count: self.none_chunk_count,
            compressed_bytes: self.compressed_bytes,
            uncompressed_bytes: self.uncompressed_bytes,
            compressed_bytes_per_row: per_row(self.compressed_bytes, self.row_count),
            uncompressed_bytes_per_row: per_row(self.uncompressed_bytes, self.row_count),
            compression_ratio: ratio(self.uncompressed_bytes, self.compressed_bytes),
            share_of_file_bytes: share(self.compressed_bytes, total_file_bytes),
        }
    }
}

fn build_report(args: &Args) -> AppResult<SizeAttributionReport> {
    let manifest = ChannelShardedManifestV1::from_path(&args.manifest)?;
    let manifest_report = ManifestReport::from(manifest.safe_summary());
    let metadata_certification = if args.certify_metadata {
        let mut options = CertificationOptions::default();
        options.verify_hashes = false;
        options.read_threads = args.read_threads;
        options.max_in_flight_row_groups = args.max_in_flight_row_groups;
        Some(CertificationReportSummary::from(
            certify_channel_sharded_artifact_v1(&args.manifest, &options)?.safe_summary,
        ))
    } else {
        None
    };

    let mut totals = MutableTotals::default();
    let mut columns = BTreeMap::<String, MutableColumnReport>::new();
    let mut artifacts = Vec::new();
    let mut row_groups = Vec::new();
    let mut read_attribution_samples = Vec::new();

    for (artifact_index, channel) in manifest.channels().iter().enumerate() {
        let artifact_path = arcadia_tio_ocb_core::resolve_manifest_relative_artifact_path(
            &args.manifest,
            &channel.relative_path,
        )?;
        let artifact_file_bytes = fs::metadata(&artifact_path)?.len();
        let file = ColumnBundleFile::open(&artifact_path)?;
        let metadata = file.metadata()?;
        let summaries = file.row_group_summaries()?;
        let mut artifact_totals = MutableTotals::default();
        artifact_totals.row_count = metadata.row_count;
        artifact_totals.row_group_count = u64::from(metadata.row_group_count);
        artifact_totals.file_bytes = artifact_file_bytes;

        for row_group in &summaries {
            let mut row_group_totals = MutableTotals::default();
            row_group_totals.row_count = row_group.row_count;
            row_group_totals.row_group_count = 1;
            for chunk in &row_group.chunks {
                artifact_totals.add_chunk(chunk);
                row_group_totals.add_chunk(chunk);
                columns
                    .entry(chunk.column_name.clone())
                    .or_default()
                    .add_chunk(chunk);
            }
            if args.include_row_groups {
                row_groups.push(RowGroupReport {
                    artifact_index,
                    channel_id: channel.channel_id,
                    row_group_id: row_group.row_group_id,
                    base_row: row_group.base_row,
                    row_count: row_group.row_count,
                    compressed_value_bytes: row_group_totals.compressed_value_bytes,
                    uncompressed_value_bytes: row_group_totals.uncompressed_value_bytes,
                    payload_compressed_bytes: row_group_totals.payload_compressed_bytes,
                    payload_uncompressed_bytes: row_group_totals.payload_uncompressed_bytes,
                    scalar_compressed_bytes: row_group_totals.scalar_compressed_bytes,
                    scalar_uncompressed_bytes: row_group_totals.scalar_uncompressed_bytes,
                    zstd_chunk_count: row_group_totals.zstd_chunk_count,
                    none_chunk_count: row_group_totals.none_chunk_count,
                    compressed_value_bytes_per_row: per_row(
                        row_group_totals.compressed_value_bytes,
                        row_group.row_count,
                    ),
                    payload_compressed_bytes_per_row: per_row(
                        row_group_totals.payload_compressed_bytes,
                        row_group.row_count,
                    ),
                });
            }
        }

        let overhead = artifact_file_bytes.saturating_sub(artifact_totals.compressed_value_bytes);
        artifacts.push(ArtifactReport {
            artifact_index,
            channel_id: channel.channel_id,
            file_bytes: artifact_file_bytes,
            row_count: metadata.row_count,
            row_group_count: metadata.row_group_count,
            column_chunk_count: metadata.column_chunk_count,
            compressed_value_bytes: artifact_totals.compressed_value_bytes,
            uncompressed_value_bytes: artifact_totals.uncompressed_value_bytes,
            payload_compressed_bytes: artifact_totals.payload_compressed_bytes,
            payload_uncompressed_bytes: artifact_totals.payload_uncompressed_bytes,
            scalar_compressed_bytes: artifact_totals.scalar_compressed_bytes,
            scalar_uncompressed_bytes: artifact_totals.scalar_uncompressed_bytes,
            container_overhead_bytes: overhead,
            zstd_chunk_count: artifact_totals.zstd_chunk_count,
            none_chunk_count: artifact_totals.none_chunk_count,
            file_bytes_per_row: per_row(artifact_file_bytes, metadata.row_count),
            compressed_value_bytes_per_row: per_row(
                artifact_totals.compressed_value_bytes,
                metadata.row_count,
            ),
            payload_compressed_bytes_per_row: per_row(
                artifact_totals.payload_compressed_bytes,
                metadata.row_count,
            ),
        });

        totals.row_count = totals.row_count.saturating_add(metadata.row_count);
        totals.row_group_count = totals
            .row_group_count
            .saturating_add(u64::from(metadata.row_group_count));
        totals.column_chunk_count = totals
            .column_chunk_count
            .saturating_add(u64::from(metadata.column_chunk_count));
        totals.file_bytes = totals.file_bytes.saturating_add(artifact_file_bytes);
        totals.compressed_value_bytes = totals
            .compressed_value_bytes
            .saturating_add(artifact_totals.compressed_value_bytes);
        totals.uncompressed_value_bytes = totals
            .uncompressed_value_bytes
            .saturating_add(artifact_totals.uncompressed_value_bytes);
        totals.payload_compressed_bytes = totals
            .payload_compressed_bytes
            .saturating_add(artifact_totals.payload_compressed_bytes);
        totals.payload_uncompressed_bytes = totals
            .payload_uncompressed_bytes
            .saturating_add(artifact_totals.payload_uncompressed_bytes);
        totals.scalar_compressed_bytes = totals
            .scalar_compressed_bytes
            .saturating_add(artifact_totals.scalar_compressed_bytes);
        totals.scalar_uncompressed_bytes = totals
            .scalar_uncompressed_bytes
            .saturating_add(artifact_totals.scalar_uncompressed_bytes);
        totals.zstd_chunk_count = totals
            .zstd_chunk_count
            .saturating_add(artifact_totals.zstd_chunk_count);
        totals.none_chunk_count = totals
            .none_chunk_count
            .saturating_add(artifact_totals.none_chunk_count);

        if args.read_sample_row_groups > 0 {
            read_attribution_samples.push(read_attribution_sample(
                args,
                &file,
                artifact_index,
                channel,
            )?);
        }
    }

    let column_reports = columns
        .into_values()
        .map(|column| column.finish(totals.file_bytes))
        .collect::<Vec<_>>();
    let container_overhead_bytes = totals
        .file_bytes
        .saturating_sub(totals.compressed_value_bytes);
    let totals_report = TotalsReport {
        artifact_count: artifacts.len(),
        row_count: totals.row_count,
        row_group_count: totals.row_group_count,
        column_chunk_count: totals.column_chunk_count,
        file_bytes: totals.file_bytes,
        compressed_value_bytes: totals.compressed_value_bytes,
        uncompressed_value_bytes: totals.uncompressed_value_bytes,
        payload_compressed_bytes: totals.payload_compressed_bytes,
        payload_uncompressed_bytes: totals.payload_uncompressed_bytes,
        scalar_compressed_bytes: totals.scalar_compressed_bytes,
        scalar_uncompressed_bytes: totals.scalar_uncompressed_bytes,
        container_overhead_bytes,
        zstd_chunk_count: totals.zstd_chunk_count,
        none_chunk_count: totals.none_chunk_count,
        file_bytes_per_row: per_row(totals.file_bytes, totals.row_count),
        compressed_value_bytes_per_row: per_row(totals.compressed_value_bytes, totals.row_count),
        payload_compressed_bytes_per_row: per_row(
            totals.payload_compressed_bytes,
            totals.row_count,
        ),
        scalar_compressed_bytes_per_row: per_row(totals.scalar_compressed_bytes, totals.row_count),
        container_overhead_bytes_per_row: per_row(container_overhead_bytes, totals.row_count),
        payload_share_of_file_bytes: share(totals.payload_compressed_bytes, totals.file_bytes),
        scalar_share_of_file_bytes: share(totals.scalar_compressed_bytes, totals.file_bytes),
        container_overhead_share_of_file_bytes: share(container_overhead_bytes, totals.file_bytes),
        value_compression_ratio: ratio(
            totals.uncompressed_value_bytes,
            totals.compressed_value_bytes,
        ),
    };
    let recommendation = recommendation_for(&totals_report);

    Ok(SizeAttributionReport {
        schema_version: REPORT_SCHEMA_VERSION,
        manifest: manifest_report,
        metadata_certification,
        totals: totals_report,
        artifacts,
        columns: column_reports,
        row_groups,
        read_attribution_samples,
        safety: SafetyReport {
            path_redacted: true,
            raw_payload_bytes_included: false,
            raw_records_included: false,
            private_paths_included: false,
            claim_scope: "diagnostic_only",
            limitations: "aggregate_size_attribution_only_not_runtime_readiness_or_layout_superiority_evidence",
        },
        recommendation,
    })
}

fn read_attribution_sample(
    args: &Args,
    file: &ColumnBundleFile,
    artifact_index: usize,
    channel: &ChannelArtifactEntryV1,
) -> AppResult<ReadAttributionSampleReport> {
    let projection = match args.read_sample_projection {
        SampleProjection::Payload => ColumnProjection::names([COMPACT_L2_PAYLOAD_COLUMN_NAME]),
        SampleProjection::All => ColumnProjection::All,
    };
    let request = ColumnBundleReadRequest {
        projection,
        predicates: Vec::new(),
        options: ColumnBundleReadOptions {
            max_threads: args.read_threads,
            validate_checksums: true,
            decode_dictionaries: false,
        },
    };
    let plan = file.plan_read(&request)?;
    let row_group_ids = plan
        .row_group_ids
        .iter()
        .copied()
        .take(args.read_sample_row_groups)
        .collect::<Vec<_>>();
    let report = file.visit_plan_row_groups_with_attribution(
        &plan,
        &row_group_ids,
        ColumnBundleReadCursorOptions {
            max_in_flight_row_groups: args.max_in_flight_row_groups,
            ordered: true,
        },
        |_batch| Ok(ColumnBundleVisitControl::Continue),
    )?;
    Ok(ReadAttributionSampleReport {
        artifact_index,
        channel_id: channel.channel_id,
        projection: args.read_sample_projection,
        requested_row_groups: row_group_ids.len(),
        yielded_row_groups: report.cursor_report.batches_yielded,
        yielded_rows: report.cursor_report.rows_yielded,
        selected_column_chunks: report.attribution.selected_column_chunks,
        effective_threads: report.attribution.effective_threads,
        max_in_flight_row_groups_observed: report.cursor_report.max_in_flight_row_groups_observed,
        fallback_reason: report.attribution.fallback_reason,
        plan_ns: report.attribution.plan_ns,
        execute_wall_ns: report.attribution.execute_wall_ns,
        callback_wall_ns: report.attribution.callback_wall_ns,
        row_group_read_ns: report.attribution.row_group_read_ns,
        read_io_ns: report.attribution.read_io_ns,
        checksum_ns: report.attribution.checksum_ns,
        decompression_ns: report.attribution.decompression_ns,
        primitive_decode_ns: report.attribution.primitive_decode_ns,
        fixed_payload_decode_ns: report.attribution.fixed_payload_decode_ns,
        copy_materialization_ns: report.attribution.copy_materialization_ns,
        bytes_read: report.attribution.bytes_read,
        compressed_bytes: report.attribution.compressed_bytes,
        uncompressed_bytes: report.attribution.uncompressed_bytes,
    })
}

fn write_report(args: &Args, report: &SizeAttributionReport) -> AppResult<()> {
    match &args.json_out {
        Some(path) => {
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            if args.pretty_json {
                serde_json::to_writer_pretty(&mut writer, report)?;
            } else {
                serde_json::to_writer(&mut writer, report)?;
            }
            writer.write_all(b"\n")?;
        }
        None => {
            let stdout = std::io::stdout();
            let mut writer = BufWriter::new(stdout.lock());
            if args.pretty_json {
                serde_json::to_writer_pretty(&mut writer, report)?;
            } else {
                serde_json::to_writer(&mut writer, report)?;
            }
            writer.write_all(b"\n")?;
        }
    }
    if let Some(prefix) = &args.csv_prefix {
        write_csv_reports(prefix, report)?;
    }
    Ok(())
}

fn write_csv_reports(prefix: &Path, report: &SizeAttributionReport) -> AppResult<()> {
    write_artifacts_csv(&suffixed_path(prefix, "artifacts.csv"), &report.artifacts)?;
    write_columns_csv(&suffixed_path(prefix, "columns.csv"), &report.columns)?;
    write_row_groups_csv(&suffixed_path(prefix, "row_groups.csv"), &report.row_groups)?;
    if !report.read_attribution_samples.is_empty() {
        write_read_samples_csv(
            &suffixed_path(prefix, "read_samples.csv"),
            &report.read_attribution_samples,
        )?;
    }
    Ok(())
}

fn suffixed_path(prefix: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}.{}", prefix.display(), suffix))
}

fn write_artifacts_csv(path: &Path, rows: &[ArtifactReport]) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        "artifact_index,channel_id,file_bytes,row_count,row_group_count,column_chunk_count,compressed_value_bytes,uncompressed_value_bytes,payload_compressed_bytes,payload_uncompressed_bytes,scalar_compressed_bytes,scalar_uncompressed_bytes,container_overhead_bytes,zstd_chunk_count,none_chunk_count,file_bytes_per_row,compressed_value_bytes_per_row,payload_compressed_bytes_per_row"
    )?;
    for row in rows {
        writeln!(
            writer,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{:.9},{:.9},{:.9}",
            row.artifact_index,
            row.channel_id,
            row.file_bytes,
            row.row_count,
            row.row_group_count,
            row.column_chunk_count,
            row.compressed_value_bytes,
            row.uncompressed_value_bytes,
            row.payload_compressed_bytes,
            row.payload_uncompressed_bytes,
            row.scalar_compressed_bytes,
            row.scalar_uncompressed_bytes,
            row.container_overhead_bytes,
            row.zstd_chunk_count,
            row.none_chunk_count,
            row.file_bytes_per_row,
            row.compressed_value_bytes_per_row,
            row.payload_compressed_bytes_per_row
        )?;
    }
    Ok(())
}

fn write_columns_csv(path: &Path, rows: &[ColumnReport]) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        "column_name,bucket,physical_type,fixed_binary_width,row_count,chunk_count,zstd_chunk_count,none_chunk_count,compressed_bytes,uncompressed_bytes,compressed_bytes_per_row,uncompressed_bytes_per_row,compression_ratio,share_of_file_bytes"
    )?;
    for row in rows {
        writeln!(
            writer,
            "{},{},{},{},{},{},{},{},{},{},{:.9},{:.9},{},{}",
            csv_escape(&row.column_name),
            row.bucket,
            csv_escape(&row.physical_type),
            row.fixed_binary_width
                .map(|value| value.to_string())
                .unwrap_or_default(),
            row.row_count,
            row.chunk_count,
            row.zstd_chunk_count,
            row.none_chunk_count,
            row.compressed_bytes,
            row.uncompressed_bytes,
            row.compressed_bytes_per_row,
            row.uncompressed_bytes_per_row,
            row.compression_ratio
                .map(|value| format!("{value:.9}"))
                .unwrap_or_default(),
            format!("{:.9}", row.share_of_file_bytes)
        )?;
    }
    Ok(())
}

fn write_row_groups_csv(path: &Path, rows: &[RowGroupReport]) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        "artifact_index,channel_id,row_group_id,base_row,row_count,compressed_value_bytes,uncompressed_value_bytes,payload_compressed_bytes,payload_uncompressed_bytes,scalar_compressed_bytes,scalar_uncompressed_bytes,zstd_chunk_count,none_chunk_count,compressed_value_bytes_per_row,payload_compressed_bytes_per_row"
    )?;
    for row in rows {
        writeln!(
            writer,
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{:.9},{:.9}",
            row.artifact_index,
            row.channel_id,
            row.row_group_id,
            row.base_row,
            row.row_count,
            row.compressed_value_bytes,
            row.uncompressed_value_bytes,
            row.payload_compressed_bytes,
            row.payload_uncompressed_bytes,
            row.scalar_compressed_bytes,
            row.scalar_uncompressed_bytes,
            row.zstd_chunk_count,
            row.none_chunk_count,
            row.compressed_value_bytes_per_row,
            row.payload_compressed_bytes_per_row
        )?;
    }
    Ok(())
}

fn write_read_samples_csv(path: &Path, rows: &[ReadAttributionSampleReport]) -> AppResult<()> {
    let mut writer = BufWriter::new(File::create(path)?);
    writeln!(
        writer,
        "artifact_index,channel_id,projection,requested_row_groups,yielded_row_groups,yielded_rows,selected_column_chunks,effective_threads,max_in_flight_row_groups_observed,fallback_reason,plan_ns,execute_wall_ns,callback_wall_ns,row_group_read_ns,read_io_ns,checksum_ns,decompression_ns,primitive_decode_ns,fixed_payload_decode_ns,copy_materialization_ns,bytes_read,compressed_bytes,uncompressed_bytes"
    )?;
    for row in rows {
        writeln!(
            writer,
            "{},{},{:?},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
            row.artifact_index,
            row.channel_id,
            row.projection,
            row.requested_row_groups,
            row.yielded_row_groups,
            row.yielded_rows,
            row.selected_column_chunks,
            row.effective_threads,
            row.max_in_flight_row_groups_observed,
            row.fallback_reason.unwrap_or(""),
            row.plan_ns,
            row.execute_wall_ns,
            row.callback_wall_ns,
            row.row_group_read_ns,
            row.read_io_ns,
            row.checksum_ns,
            row.decompression_ns,
            row.primitive_decode_ns,
            row.fixed_payload_decode_ns,
            row.copy_materialization_ns,
            row.bytes_read,
            row.compressed_bytes,
            row.uncompressed_bytes
        )?;
    }
    Ok(())
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_owned()
    }
}

fn bucket_for_column(column_name: &str) -> &'static str {
    if column_name == COMPACT_L2_PAYLOAD_COLUMN_NAME {
        "payload"
    } else {
        "scalar"
    }
}

fn per_row(bytes: u64, rows: u64) -> f64 {
    if rows == 0 {
        0.0
    } else {
        bytes as f64 / rows as f64
    }
}

fn share(part: u64, whole: u64) -> f64 {
    if whole == 0 {
        0.0
    } else {
        part as f64 / whole as f64
    }
}

fn ratio(uncompressed: u64, compressed: u64) -> Option<f64> {
    (compressed != 0).then(|| uncompressed as f64 / compressed as f64)
}

fn recommendation_for(totals: &TotalsReport) -> RecommendationReport {
    if totals.payload_share_of_file_bytes >= 0.60 {
        RecommendationReport {
            next_experiment: "fixed_binary_byte_lane_transpose",
            reason: format!(
                "payload chunks account for {:.2}% of file bytes; improve the opaque FixedBinary<168> physical representation before tuning scalars",
                totals.payload_share_of_file_bytes * 100.0
            ),
        }
    } else if totals.scalar_share_of_file_bytes >= 0.20 {
        RecommendationReport {
            next_experiment: "scalar_encoding",
            reason: format!(
                "scalar chunks account for {:.2}% of file bytes; constant/range/delta encodings are the smallest low-risk next step",
                totals.scalar_share_of_file_bytes * 100.0
            ),
        }
    } else if totals.container_overhead_share_of_file_bytes >= 0.15 {
        RecommendationReport {
            next_experiment: "row_group_size_matrix",
            reason: format!(
                "container overhead accounts for {:.2}% of file bytes; row-group sizing and chunk/object overhead should be measured next",
                totals.container_overhead_share_of_file_bytes * 100.0
            ),
        }
    } else {
        RecommendationReport {
            next_experiment: "zstd_row_group_matrix",
            reason:
                "no single bucket dominates strongly; run the bounded zstd-level and row-group-size matrix before larger layout work"
                    .to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn recommendation_prefers_payload_when_payload_dominates() {
        let totals = TotalsReport {
            artifact_count: 1,
            row_count: 10,
            row_group_count: 1,
            column_chunk_count: 1,
            file_bytes: 100,
            compressed_value_bytes: 80,
            uncompressed_value_bytes: 800,
            payload_compressed_bytes: 70,
            payload_uncompressed_bytes: 700,
            scalar_compressed_bytes: 10,
            scalar_uncompressed_bytes: 100,
            container_overhead_bytes: 20,
            zstd_chunk_count: 1,
            none_chunk_count: 0,
            file_bytes_per_row: 10.0,
            compressed_value_bytes_per_row: 8.0,
            payload_compressed_bytes_per_row: 7.0,
            scalar_compressed_bytes_per_row: 1.0,
            container_overhead_bytes_per_row: 2.0,
            payload_share_of_file_bytes: 0.70,
            scalar_share_of_file_bytes: 0.10,
            container_overhead_share_of_file_bytes: 0.20,
            value_compression_ratio: Some(10.0),
        };
        assert_eq!(
            recommendation_for(&totals).next_experiment,
            "fixed_binary_byte_lane_transpose"
        );
    }

    #[test]
    fn csv_escape_quotes_commas_and_quotes() {
        assert_eq!(csv_escape("plain"), "plain");
        assert_eq!(csv_escape("a,b\"c"), "\"a,b\"\"c\"");
    }

    #[test]
    fn args_parse_reads_manifest_from_config() {
        let root = test_root("args_parse_reads_manifest_from_config");
        fs::create_dir_all(&root).expect("create test root");
        let config = root.join("config.json");
        fs::write(&config, "{\"manifest\":\"manifest.json\"}\n").expect("write config");

        let args = Args::parse([
            std::ffi::OsString::from("--config"),
            config.into_os_string(),
            std::ffi::OsString::from("--read-sample-row-groups"),
            std::ffi::OsString::from("2"),
            std::ffi::OsString::from("--read-sample-projection"),
            std::ffi::OsString::from("all"),
            std::ffi::OsString::from("--no-row-groups"),
        ])
        .expect("parse args");

        assert_eq!(args.manifest, PathBuf::from("manifest.json"));
        assert_eq!(args.read_sample_row_groups, 2);
        assert_eq!(args.read_sample_projection, SampleProjection::All);
        assert!(!args.include_row_groups);
        assert!(!args.certify_metadata);
    }

    #[test]
    fn args_parse_enables_metadata_certification_only_when_requested() {
        let args = Args::parse([
            std::ffi::OsString::from("--manifest"),
            std::ffi::OsString::from("manifest.json"),
            std::ffi::OsString::from("--certify-metadata"),
        ])
        .expect("parse args");

        assert!(args.certify_metadata);
    }

    #[test]
    fn safety_report_serializes_no_private_material_flags() {
        let safety = SafetyReport {
            path_redacted: true,
            raw_payload_bytes_included: false,
            raw_records_included: false,
            private_paths_included: false,
            claim_scope: "diagnostic_only",
            limitations: "aggregate_size_attribution_only",
        };
        let json = serde_json::to_string(&safety).expect("serialize safety report");
        assert!(json.contains("\"path_redacted\":true"));
        assert!(json.contains("\"raw_payload_bytes_included\":false"));
        assert!(json.contains("\"private_paths_included\":false"));
        assert!(json.contains("\"diagnostic_only\""));
    }

    fn test_root(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        PathBuf::from("target")
            .join("compact_l2_size_attribution_tests")
            .join(format!("{name}-{stamp}"))
    }
}
