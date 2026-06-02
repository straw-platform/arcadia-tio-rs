//! Public Rust reform, compaction, and diagnostics tutorial.
//!
//! The workflow uses separate destination files under a temporary directory.
//! Reports are native diagnostics copied into owned Rust values; byte counts are
//! sanity checks for API shape only, not storage-efficiency or release-readiness
//! evidence.

use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::time::{SystemTime, UNIX_EPOCH};

use arcadia_tio_rs::{
    AxisKind, CompactionMode, CompactionOptions, CreateOptions, DType, DimSpec, EntrySelector,
    ReformOptions, TensorData, TensorFile, V4CompactionAnalysisPolicy, V4PreciseAccountingOptions,
    V4ReportStatus, V4RetainedHistoryCompactionOptions,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Use one staging directory and move through source -> reform -> compaction.
    let temp = TutorialTempDir::new("reform_compaction_diagnostics")?;
    let paths = Paths::new(temp.path());

    create_source(&paths.source)?;
    demonstrate_reform(&paths)?;
    demonstrate_compaction_and_reports(&paths)?;

    println!(
        "reform/compaction/diagnostics ok: separate destinations written under {}",
        temp.path().display()
    );
    Ok(())
}

fn create_source(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    // Build a small source file with a rewrite to ensure reform input has history.
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );
    let mut file = TensorFile::create(path, options)?;
    file.append_f32(&[1.0, 2.0], &[1, 2])?;
    file.append_f32(&[3.0, 4.0], &[1, 2])?;
    file.rewrite_f32(EntrySelector::Take(vec![0]), &[10.0, 11.0], &[1, 2])?;
    assert_eq!(
        file.read_all()?.data,
        TensorData::F32(vec![10.0, 11.0, 3.0, 4.0])
    );
    Ok(())
}

fn demonstrate_reform(paths: &Paths) -> Result<(), Box<dyn std::error::Error>> {
    // Reform across two target layouts and assert output parity.
    let mut source = TensorFile::open(&paths.source)?;
    source.reform_to(
        &paths.regular_reform,
        ReformOptions::regular_chunked(vec![1, 2]),
    )?;
    // Re-open each destination to validate deterministic shape and values.
    let mut regular = TensorFile::open(&paths.regular_reform)?;
    regular.reform_to(&paths.wau_reform, ReformOptions::whole_append_unit())?;

    for path in [&paths.regular_reform, &paths.wau_reform] {
        let reformed = TensorFile::open(path)?;
        assert_eq!(reformed.read_all()?.shape, vec![2, 2]);
        assert_eq!(
            reformed.read_all()?.data,
            TensorData::F32(vec![10.0, 11.0, 3.0, 4.0])
        );
    }

    // Trigger a diagnostically reported invalid reform to verify failure is
    // returned as status-bearing error path.
    let invalid_report = regular
        .reform_to_ex(
            &paths.invalid_reform,
            ReformOptions::regular_chunked(vec![0, 2]),
        )
        .expect_err("invalid reform shape should return a diagnostic wrapper error");
    assert!(invalid_report.message().contains("v4.reform"));

    Ok(())
}

fn demonstrate_compaction_and_reports(paths: &Paths) -> Result<(), Box<dyn std::error::Error>> {
    // Inspect analysis/report outputs before compaction so diagnostics can be
    // compared against post-compaction file state.
    let mut source = TensorFile::open(&paths.source)?;

    let shallow = source.analyze_compaction()?;
    assert!(shallow.commit_count >= 1);

    let diagnostics = source.v4_diagnostics()?;
    assert_eq!(diagnostics.status, V4ReportStatus::Complete);
    assert!(diagnostics.current_head.payload_bytes > 0);
    assert!(diagnostics.omitted_unreachable_bytes);
    assert!(diagnostics.omitted_unreachable_bytes_reason.is_some());

    let precise_diagnostics =
        source.v4_diagnostics_precise(V4PreciseAccountingOptions::default())?;
    assert_eq!(precise_diagnostics.status, V4ReportStatus::Complete);
    assert_eq!(
        precise_diagnostics.reason_code.as_deref(),
        Some("v4.precise.complete")
    );
    assert!(
        precise_diagnostics
            .precise_accounting
            .retained_history_required_bytes
            .is_some()
    );
    assert!(
        precise_diagnostics
            .precise_accounting
            .reclaimable_bytes
            .is_some()
    );

    let analysis = source.analyze_v4_compaction()?;
    assert_eq!(analysis.status, V4ReportStatus::Complete);
    assert_eq!(
        analysis.policy,
        V4CompactionAnalysisPolicy::CompactToCurrentState
    );
    assert!(analysis.source_file_bytes > 0);
    assert!(analysis.current_state_required_bytes > 0);

    let precise = source.analyze_v4_compaction_precise(V4PreciseAccountingOptions::all())?;
    assert_eq!(precise.status, V4ReportStatus::Complete);
    assert!(precise.source_file_bytes >= analysis.source_file_bytes);

    source.compact_to(
        &paths.compact_copy,
        CompactionOptions {
            retain_commits: 1,
            mode: CompactionMode::CopyLive,
            ..CompactionOptions::default()
        },
    )?;
    let compacted = TensorFile::open(&paths.compact_copy)?;
    assert_eq!(compacted.read_all()?.shape, vec![2, 2]);
    assert_eq!(
        compacted.read_all()?.data,
        TensorData::F32(vec![10.0, 11.0, 3.0, 4.0])
    );

    let maybe_compacted = source.maybe_compact(
        &paths.maybe_compact,
        CompactionOptions {
            dead_ratio_threshold: 2.0,
            ..CompactionOptions::default()
        },
    )?;
    assert!(!maybe_compacted);

    let retained = source.compact_v4_retained_history_to(
        &paths.retained_compact,
        V4RetainedHistoryCompactionOptions::retain_last(1),
    )?;
    assert_eq!(retained.status, V4ReportStatus::Complete);
    assert!(retained.destination_file_bytes > 0);
    let retained_file = TensorFile::open(&paths.retained_compact)?;
    assert_eq!(retained_file.read_all()?.shape, vec![2, 2]);

    Ok(())
}

struct Paths {
    source: PathBuf,
    regular_reform: PathBuf,
    wau_reform: PathBuf,
    invalid_reform: PathBuf,
    compact_copy: PathBuf,
    maybe_compact: PathBuf,
    retained_compact: PathBuf,
}

impl Paths {
    fn new(root: &Path) -> Self {
        Self {
            source: root.join("source.tio"),
            regular_reform: root.join("regular_reform.tio"),
            wau_reform: root.join("wau_reform.tio"),
            invalid_reform: root.join("invalid_reform.tio"),
            compact_copy: root.join("compact_copy.tio"),
            maybe_compact: root.join("maybe_compact.tio"),
            retained_compact: root.join("retained_compact.tio"),
        }
    }
}

struct TutorialTempDir {
    path: PathBuf,
}

impl TutorialTempDir {
    fn new(label: &str) -> std::io::Result<Self> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "arcadia_tio_rust_tutorial_{label}_{}_{}",
            process::id(),
            nanos
        ));
        fs::create_dir(&path)?;
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
