use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::time::Instant;

use arcadia_tio_ocb_core::{
    COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT, CompactL2PhysicalV2ChannelCertificationReport,
    CompactL2PhysicalV2ManifestCertificationOptions, SafeCertificationSummary,
    certify_compact_l2_physical_v2_manifest,
};
use serde::{Deserialize, Serialize};

type AppResult<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const REPORT_SCHEMA_VERSION: &str = "arcadia-tio.compact-l2-physical-v2-certification.v1";

fn main() -> AppResult<()> {
    let args = Args::parse(env::args_os().skip(1))?;
    let report = run_certification(&args)?;
    write_report(&args, &report)?;
    Ok(())
}

#[derive(Debug, Clone)]
struct Args {
    manifest: PathBuf,
    json_out: Option<PathBuf>,
    read_threads: usize,
    max_in_flight_row_groups: usize,
    max_rows: Option<u64>,
    verify_scalar_continuity: bool,
    verify_legacy_reconstruction: bool,
    verify_hashes: bool,
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
        let mut read_threads = 1usize;
        let mut max_in_flight_row_groups = 1usize;
        let mut max_rows = None;
        let mut verify_scalar_continuity = true;
        let mut verify_legacy_reconstruction = false;
        let mut verify_hashes = true;
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
                "--read-threads" => read_threads = next_usize(&mut args, "--read-threads")?,
                "--max-in-flight-row-groups" => {
                    max_in_flight_row_groups = next_usize(&mut args, "--max-in-flight-row-groups")?
                }
                "--max-rows" => max_rows = Some(next_u64(&mut args, "--max-rows")?),
                "--no-scalar-continuity" => verify_scalar_continuity = false,
                "--legacy-reconstruction" => verify_legacy_reconstruction = true,
                "--no-legacy-reconstruction" => verify_legacy_reconstruction = false,
                "--no-hashes" => verify_hashes = false,
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

        if let Some(config) = config {
            let text = fs::read_to_string(&config)?;
            let config: ArgsConfig = serde_json::from_str(&text)?;
            if manifest.is_none() {
                manifest = Some(config.manifest);
            }
            if json_out.is_none() {
                json_out = config.json_out;
            }
            if let Some(value) = config.read_threads {
                read_threads = value;
            }
            if let Some(value) = config.max_in_flight_row_groups {
                max_in_flight_row_groups = value;
            }
            if max_rows.is_none() {
                max_rows = config.max_rows;
            }
            if let Some(value) = config.verify_scalar_continuity {
                verify_scalar_continuity = value;
            }
            if let Some(value) = config.verify_legacy_reconstruction {
                verify_legacy_reconstruction = value;
            }
            if let Some(value) = config.verify_hashes {
                verify_hashes = value;
            }
            if let Some(value) = config.pretty_json {
                pretty_json = value;
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
            read_threads,
            max_in_flight_row_groups,
            max_rows,
            verify_scalar_continuity,
            verify_legacy_reconstruction,
            verify_hashes,
            pretty_json,
        })
    }
}

#[derive(Debug, Deserialize)]
struct ArgsConfig {
    manifest: PathBuf,
    #[serde(default)]
    json_out: Option<PathBuf>,
    #[serde(default)]
    read_threads: Option<usize>,
    #[serde(default)]
    max_in_flight_row_groups: Option<usize>,
    #[serde(default)]
    max_rows: Option<u64>,
    #[serde(default)]
    verify_scalar_continuity: Option<bool>,
    #[serde(default)]
    verify_legacy_reconstruction: Option<bool>,
    #[serde(default)]
    verify_hashes: Option<bool>,
    #[serde(default)]
    pretty_json: Option<bool>,
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

fn next_u64<I>(args: &mut I, flag: &str) -> AppResult<u64>
where
    I: Iterator<Item = std::ffi::OsString>,
{
    let value = next_string(args, flag)?;
    value
        .parse::<u64>()
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
        "usage: cargo run -p arcadia-tio-ocb-core --example compact_l2_physical_v2_certify -- \\\n             [--manifest manifest.json | --config run-local-config.json] \\\n             [--json-out redacted-summary.json] [--read-threads N] \\\n             [--max-in-flight-row-groups N] [--max-rows N] \\\n             [--legacy-reconstruction] [--no-hashes] [--compact-json]\n\nThe report is path-redacted and contains aggregate compact-L2 physical-v2 certification evidence only."
    );
}

#[derive(Debug, Serialize)]
struct CertificationCliReport {
    schema_version: &'static str,
    manifest: ManifestSummaryReport,
    options: OptionsReport,
    totals: TotalsReport,
    channels: Vec<ChannelReport>,
    elapsed_ms: u128,
    safety: SafetyReport,
}

#[derive(Debug, Serialize)]
struct ManifestSummaryReport {
    schema_version: u16,
    trading_day: u32,
    artifact_format: String,
    channel_count: usize,
    row_count: u64,
    row_group_count: u64,
    certified: bool,
    path_redacted: bool,
}

impl From<SafeCertificationSummary> for ManifestSummaryReport {
    fn from(value: SafeCertificationSummary) -> Self {
        Self {
            schema_version: value.schema_version,
            trading_day: value.trading_day,
            artifact_format: value.artifact_format,
            channel_count: value.channel_count,
            row_count: value.row_count,
            row_group_count: value.row_group_count,
            certified: value.certified,
            path_redacted: value.path_redacted,
        }
    }
}

#[derive(Debug, Serialize)]
struct OptionsReport {
    read_threads: usize,
    max_in_flight_row_groups: usize,
    max_rows: Option<u64>,
    verify_scalar_continuity: bool,
    verify_legacy_reconstruction: bool,
    verify_hashes: bool,
}

#[derive(Debug, Serialize)]
struct TotalsReport {
    channel_count: usize,
    row_count: u64,
    row_group_count: u64,
    selected_column_chunk_count: u64,
    selected_compressed_bytes: u64,
    selected_uncompressed_bytes: u64,
    selected_compressed_bytes_per_row: f64,
    selected_uncompressed_bytes_per_row: f64,
    order_record_count: Option<u64>,
    trade_record_count: Option<u64>,
    checksum_verified_channel_count: usize,
    legacy_payload_hash_channel_count: usize,
}

#[derive(Debug, Serialize)]
struct ChannelReport {
    channel_id: u32,
    row_count: u64,
    row_group_count: u32,
    first_biz_index: Option<u64>,
    last_biz_index: Option<u64>,
    min_receive_nano: Option<i64>,
    max_receive_nano: Option<i64>,
    order_record_count: Option<u64>,
    trade_record_count: Option<u64>,
    required_column_count: usize,
    selected_column_chunk_count: u64,
    selected_compressed_bytes: u64,
    selected_uncompressed_bytes: u64,
    selected_compressed_bytes_per_row: f64,
    selected_uncompressed_bytes_per_row: f64,
    checksum_verified: bool,
    legacy_payload_hash_available: bool,
}

impl From<CompactL2PhysicalV2ChannelCertificationReport> for ChannelReport {
    fn from(value: CompactL2PhysicalV2ChannelCertificationReport) -> Self {
        let row_count = value.row_count.max(1);
        Self {
            channel_id: value.channel_id,
            row_count: value.row_count,
            row_group_count: value.row_group_count,
            first_biz_index: value.first_biz_index,
            last_biz_index: value.last_biz_index,
            min_receive_nano: value.min_receive_nano,
            max_receive_nano: value.max_receive_nano,
            order_record_count: value.order_record_count,
            trade_record_count: value.trade_record_count,
            required_column_count: value.required_column_count,
            selected_column_chunk_count: value.selected_column_chunk_count,
            selected_compressed_bytes: value.selected_compressed_bytes,
            selected_uncompressed_bytes: value.selected_uncompressed_bytes,
            selected_compressed_bytes_per_row: value.selected_compressed_bytes as f64
                / row_count as f64,
            selected_uncompressed_bytes_per_row: value.selected_uncompressed_bytes as f64
                / row_count as f64,
            checksum_verified: value.checksum_verified,
            legacy_payload_hash_available: value.legacy_payload_hash_fnv1a64.is_some(),
        }
    }
}

#[derive(Debug, Serialize)]
struct SafetyReport {
    path_redacted: bool,
    raw_records_included: bool,
    raw_payload_bytes_included: bool,
    writes_transformed_artifacts: bool,
    artifact_format_expected: &'static str,
}

fn run_certification(args: &Args) -> AppResult<CertificationCliReport> {
    let started = Instant::now();
    let report = certify_compact_l2_physical_v2_manifest(
        &args.manifest,
        &CompactL2PhysicalV2ManifestCertificationOptions {
            artifact_format: COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT.to_owned(),
            verify_scalar_continuity: args.verify_scalar_continuity,
            verify_legacy_reconstruction: args.verify_legacy_reconstruction,
            verify_hashes: args.verify_hashes,
            max_rows: args.max_rows,
            read_threads: args.read_threads,
            max_in_flight_row_groups: args.max_in_flight_row_groups,
        },
    )?;
    let elapsed_ms = started.elapsed().as_millis();
    let manifest = ManifestSummaryReport::from(report.safe_summary);
    let channels = report
        .channel_reports
        .into_iter()
        .map(ChannelReport::from)
        .collect::<Vec<_>>();
    let totals = build_totals(&channels, manifest.channel_count, manifest.row_count);
    Ok(CertificationCliReport {
        schema_version: REPORT_SCHEMA_VERSION,
        manifest,
        options: OptionsReport {
            read_threads: args.read_threads,
            max_in_flight_row_groups: args.max_in_flight_row_groups,
            max_rows: args.max_rows,
            verify_scalar_continuity: args.verify_scalar_continuity,
            verify_legacy_reconstruction: args.verify_legacy_reconstruction,
            verify_hashes: args.verify_hashes,
        },
        totals,
        channels,
        elapsed_ms,
        safety: SafetyReport {
            path_redacted: true,
            raw_records_included: false,
            raw_payload_bytes_included: false,
            writes_transformed_artifacts: false,
            artifact_format_expected: COMPACT_L2_PHYSICAL_V2_ARTIFACT_FORMAT,
        },
    })
}

fn build_totals(channels: &[ChannelReport], channel_count: usize, row_count: u64) -> TotalsReport {
    let selected_column_chunk_count = channels
        .iter()
        .map(|channel| channel.selected_column_chunk_count)
        .sum();
    let selected_compressed_bytes = channels
        .iter()
        .map(|channel| channel.selected_compressed_bytes)
        .sum();
    let selected_uncompressed_bytes = channels
        .iter()
        .map(|channel| channel.selected_uncompressed_bytes)
        .sum();
    let row_count_nonzero = row_count.max(1);
    TotalsReport {
        channel_count,
        row_count,
        row_group_count: channels
            .iter()
            .map(|channel| u64::from(channel.row_group_count))
            .sum(),
        selected_column_chunk_count,
        selected_compressed_bytes,
        selected_uncompressed_bytes,
        selected_compressed_bytes_per_row: selected_compressed_bytes as f64
            / row_count_nonzero as f64,
        selected_uncompressed_bytes_per_row: selected_uncompressed_bytes as f64
            / row_count_nonzero as f64,
        order_record_count: sum_optional(channels.iter().map(|channel| channel.order_record_count)),
        trade_record_count: sum_optional(channels.iter().map(|channel| channel.trade_record_count)),
        checksum_verified_channel_count: channels
            .iter()
            .filter(|channel| channel.checksum_verified)
            .count(),
        legacy_payload_hash_channel_count: channels
            .iter()
            .filter(|channel| channel.legacy_payload_hash_available)
            .count(),
    }
}

fn sum_optional(values: impl Iterator<Item = Option<u64>>) -> Option<u64> {
    let mut any = false;
    let mut total = 0u64;
    for value in values {
        let value = value?;
        any = true;
        total = total.saturating_add(value);
    }
    any.then_some(total)
}

fn write_report(args: &Args, report: &CertificationCliReport) -> AppResult<()> {
    match &args.json_out {
        Some(path) => {
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    fs::create_dir_all(parent)?;
                }
            }
            let file = File::create(path)?;
            let mut writer = BufWriter::new(file);
            write_json(&mut writer, report, args.pretty_json)?;
            writer.write_all(b"\n")?;
        }
        None => {
            let stdout = std::io::stdout();
            let mut handle = stdout.lock();
            write_json(&mut handle, report, args.pretty_json)?;
            handle.write_all(b"\n")?;
        }
    }
    Ok(())
}

fn write_json(
    mut writer: impl Write,
    report: &CertificationCliReport,
    pretty: bool,
) -> AppResult<()> {
    if pretty {
        serde_json::to_writer_pretty(&mut writer, report)?;
    } else {
        serde_json::to_writer(&mut writer, report)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse_defaults_are_safe_and_read_only() {
        let args = Args::parse(["--manifest", "manifest.json"].map(std::ffi::OsString::from))
            .expect("parse args");
        assert_eq!(args.manifest, PathBuf::from("manifest.json"));
        assert_eq!(args.read_threads, 1);
        assert_eq!(args.max_in_flight_row_groups, 1);
        assert!(args.verify_scalar_continuity);
        assert!(!args.verify_legacy_reconstruction);
        assert!(args.verify_hashes);
        assert!(args.pretty_json);
    }

    #[test]
    fn sum_optional_requires_all_values_to_be_available() {
        assert_eq!(sum_optional([Some(1), Some(2)].into_iter()), Some(3));
        assert_eq!(sum_optional([Some(1), None].into_iter()), None);
        assert_eq!(sum_optional(std::iter::empty()), None);
    }
}
