//! Focused end-to-end coverage for the documented single-tensor daily artifact
//! workflow: one logical tensor per `.tio` file, append axis 0 = trading day,
//! one whole dense entry per appended day with explicit `NaN` sentinels, a
//! cheap application-level duplicate-day check that reads only the last entry,
//! middle-day revision through the narrow rewrite envelope, and a caller-owned
//! `analyze_compaction` + `compact_to` + rename swap that leaves exactly one
//! artifact behind.
//!
//! This is focused workflow coverage; it is not benchmark, capacity,
//! storage-efficiency, or release-readiness evidence.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use arcadia_tio_rs::{
    AxisKind, CompactionOptions, CreateOptions, DType, DimSpec, EntrySelector, ErrorCode,
    TensorData, TensorFile, V4ReportStatus,
};

const SYMBOLS: usize = 3;
const METRICS: usize = 2;
/// Row width of one whole day entry: symbols x metrics plus one trailing day label.
const ENTRY_WIDTH: usize = SYMBOLS * METRICS + 1;

const DAY_ONE: u32 = 2026_06_01;
const DAY_TWO: u32 = 2026_06_02;
const DAY_THREE: u32 = 2026_06_03;
const DAY_FOUR: u32 = 2026_06_04;

const MAPPING_IDENTITY_KEY: &str = "artifact.mapping_identity";
const MAPPING_IDENTITY: &str = "mapping-20260926";

/// Builds one dense day entry with a trailing day label and no nulls; invalid
/// cells are explicit `NaN` sentinels owned by the application.
fn day_entry(day: u32, seed: f64) -> Vec<f64> {
    let mut entry = Vec::with_capacity(ENTRY_WIDTH);
    for symbol in 0..SYMBOLS {
        for metric in 0..METRICS {
            entry.push(seed + (symbol * METRICS + metric) as f64);
        }
    }
    entry.push(f64::from(day));
    entry
}

fn entry_shape() -> [u64; 2] {
    [1, ENTRY_WIDTH as u64]
}

fn daily_options() -> CreateOptions {
    let mut options = CreateOptions::streaming(
        DType::F64,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("day"),
            DimSpec::new(AxisKind::Channel, ENTRY_WIDTH as u32).with_name("payload"),
        ],
        0,
    );
    options.user_kv = vec![
        (
            "artifact.layout".to_string(),
            "daily-single-tensor/v1".to_string(),
        ),
        (
            MAPPING_IDENTITY_KEY.to_string(),
            MAPPING_IDENTITY.to_string(),
        ),
    ];
    options
}

/// Documented duplicate-day check: read only the last appended entry through
/// `dim_lens` + `read_entry_range` and return its trailing day label.
fn last_appended_day(file: &TensorFile) -> arcadia_tio_rs::Result<Option<u32>> {
    let entries = file.dim_lens()?[0];
    if entries == 0 {
        return Ok(None);
    }
    let tensor = file.read_entry_range(entries - 1, entries)?;
    let TensorData::F64(values) = tensor.data else {
        panic!("daily artifact payload must be f64");
    };
    let day = *values
        .last()
        .expect("a day entry must carry its trailing day label");
    Ok(Some(day as u32))
}

/// Documented application-level append-order check: `day` must be strictly
/// newer than the stored last day. TIO does not interpret the day value.
fn day_is_appendable(file: &TensorFile, day: u32) -> arcadia_tio_rs::Result<bool> {
    Ok(match last_appended_day(file)? {
        Some(last) => day > last,
        None => day > 0,
    })
}

fn assert_f64_cells_eq(actual: &[f64], expected: &[f64], context: &str) {
    assert_eq!(actual.len(), expected.len(), "{context}: cell count");
    for (index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        let matches = (*actual == *expected) || (actual.is_nan() && expected.is_nan());
        assert!(
            matches,
            "{context}: cell {index} expected {expected} got {actual}"
        );
    }
}

#[test]
fn daily_artifact_multi_run_append_read_back_and_duplicate_check() {
    let path = unique_path("multi-run.tio");
    {
        let mut file = TensorFile::create(&path, daily_options()).expect("create daily artifact");
        file.append_f64(&day_entry(DAY_ONE, 1.0), &entry_shape())
            .expect("append first day");
    }
    {
        let mut file = TensorFile::open(&path).expect("reopen for second run");
        assert_eq!(file.dim_lens().expect("dimension lengths")[0], 1);
        assert_eq!(last_appended_day(&file).expect("last day"), Some(DAY_ONE));
        assert!(
            !day_is_appendable(&file, DAY_ONE).expect("duplicate day check"),
            "the stored day must fail the duplicate check"
        );
        assert!(
            !day_is_appendable(&file, DAY_ONE - 1).expect("out-of-order day check"),
            "an older day must fail the order check"
        );
        assert!(
            day_is_appendable(&file, DAY_TWO).expect("next day check"),
            "the next day must be appendable"
        );
        file.append_f64(&day_entry(DAY_TWO, 10.0), &entry_shape())
            .expect("append second day");
    }
    let mut file = TensorFile::open(&path).expect("reopen for read-back");
    assert_eq!(file.rank().expect("rank"), 2);
    assert_eq!(file.append_axis().expect("append axis"), 0);
    assert_eq!(file.dim_lens().expect("dimension lengths")[0], 2);

    let whole = file.read_all().expect("read whole artifact");
    assert_eq!(whole.shape, vec![2, ENTRY_WIDTH as u64]);
    let TensorData::F64(values) = whole.data else {
        panic!("daily artifact payload must be f64");
    };
    let mut expected = day_entry(DAY_ONE, 1.0);
    expected.extend(day_entry(DAY_TWO, 10.0));
    assert_f64_cells_eq(&values, &expected, "whole artifact read");

    let tail = file.read_entry_range(1, 2).expect("read last entry");
    let TensorData::F64(tail_values) = tail.data else {
        panic!("daily artifact payload must be f64");
    };
    assert_f64_cells_eq(&tail_values, &day_entry(DAY_TWO, 10.0), "last-N entry read");

    // A failed append publishes nothing: the visible entry count and last day
    // are unchanged after a rejected append request.
    let before = file.dim_lens().expect("dimension lengths")[0];
    let err = file
        .append_f64(&[0.0; ENTRY_WIDTH - 1], &entry_shape())
        .expect_err("a short append payload must be rejected");
    assert!(!err.message().is_empty());
    assert_eq!(
        file.dim_lens().expect("dimension lengths")[0],
        before,
        "a failed append must not publish an entry"
    );
    assert_eq!(last_appended_day(&file).expect("last day"), Some(DAY_TWO));
    drop(file);
    cleanup_file(&path);
}

#[test]
fn daily_artifact_reader_binds_open_snapshot_until_reopen() {
    let path = unique_path("snapshot.tio");
    {
        let mut file = TensorFile::create(&path, daily_options()).expect("create daily artifact");
        file.append_f64(&day_entry(DAY_ONE, 1.0), &entry_shape())
            .expect("append first day");
    }
    let bound_reader = TensorFile::open(&path).expect("open bound reader");
    {
        let mut writer = TensorFile::open(&path).expect("open second handle");
        writer
            .append_f64(&day_entry(DAY_TWO, 10.0), &entry_shape())
            .expect("append second day through the second handle");
    }
    assert_eq!(
        bound_reader.read_all().expect("bound snapshot read").shape,
        vec![1, ENTRY_WIDTH as u64],
        "an open handle reads the snapshot it bound at open"
    );
    assert_eq!(
        last_appended_day(&bound_reader).expect("bound last day"),
        Some(DAY_ONE)
    );
    drop(bound_reader);

    let refreshed = TensorFile::open(&path).expect("reopen refreshed reader");
    assert_eq!(
        refreshed.read_all().expect("refreshed read").shape,
        vec![2, ENTRY_WIDTH as u64]
    );
    assert_eq!(
        last_appended_day(&refreshed).expect("refreshed last day"),
        Some(DAY_TWO)
    );
    drop(refreshed);
    cleanup_file(&path);
}

#[test]
fn daily_artifact_middle_day_override_compaction_and_atomic_swap() {
    let directory = unique_directory("override-swap");
    let path = directory.join("daily-artifact.tio");
    let staged = directory.join("daily-artifact.staged.tio");
    {
        let mut file = TensorFile::create(&path, daily_options()).expect("create override source");
        // Day two starts with an explicit NaN sentinel that the revision repairs.
        let mut day_two = day_entry(DAY_TWO, 10.0);
        day_two[0] = f64::NAN;
        file.append_f64(&day_entry(DAY_ONE, 1.0), &entry_shape())
            .expect("append day one");
        file.append_f64(&day_two, &entry_shape())
            .expect("append day two with NaN sentinel");
        file.append_f64(&day_entry(DAY_THREE, 20.0), &entry_shape())
            .expect("append day three");
    }
    {
        let mut file = TensorFile::open(&path).expect("reopen for override");
        assert_eq!(last_appended_day(&file).expect("last day"), Some(DAY_THREE));
        // The explicit NaN sentinel is stored and readable before the repair.
        let sentinel = file.read_entry_range(1, 2).expect("read sentinel entry");
        let TensorData::F64(sentinel_values) = sentinel.data else {
            panic!("daily artifact payload must be f64");
        };
        assert!(
            sentinel_values[0].is_nan(),
            "the stored NaN sentinel must round-trip"
        );
        // Revise the middle day entry only; its day label stays unchanged.
        let repaired = day_entry(DAY_TWO, 50.0);
        file.rewrite_f64(EntrySelector::Take(vec![1]), &repaired, &entry_shape())
            .expect("rewrite middle day entry");
        // `analyze_compaction` is the retained shallow compatibility analysis;
        // `analyze_v4_compaction` carries the status-aware pressure report.
        let stats = file.analyze_compaction().expect("analyze compaction");
        assert!(stats.live_bytes > 0, "analysis must report live bytes");
        let analysis = file
            .analyze_v4_compaction()
            .expect("analyze V4 compaction pressure");
        assert_eq!(analysis.status, V4ReportStatus::Complete);
        assert!(analysis.source_file_bytes > 0);
        file.compact_to(&staged, CompactionOptions::default())
            .expect("compact to staged destination");
    }

    // Caller-owned durable swap: fsync the staged artifact, atomically rename it
    // over the source, then fsync the parent directory.
    fs::File::open(&staged)
        .expect("open staged artifact")
        .sync_all()
        .expect("fsync staged artifact");
    fs::rename(&staged, &path).expect("atomically rename staged artifact over the source");
    fs::File::open(&directory)
        .expect("open artifact directory")
        .sync_all()
        .expect("fsync artifact directory");
    assert!(
        !staged.exists(),
        "the staged intermediate must not remain after the swap"
    );
    let artifacts = fs::read_dir(&directory)
        .expect("read artifact directory")
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|ext| ext == std::ffi::OsStr::new("tio"))
        })
        .count();
    assert_eq!(artifacts, 1, "the swap must leave exactly one artifact");

    let mut file = TensorFile::open(&path).expect("reopen swapped artifact");
    let mut expected = day_entry(DAY_ONE, 1.0);
    expected.extend(day_entry(DAY_TWO, 50.0));
    expected.extend(day_entry(DAY_THREE, 20.0));
    let whole = file.read_all().expect("read swapped artifact");
    assert_eq!(whole.shape, vec![3, ENTRY_WIDTH as u64]);
    let TensorData::F64(values) = whole.data else {
        panic!("daily artifact payload must be f64");
    };
    assert_f64_cells_eq(&values, &expected, "swapped artifact read-back");

    let meta = TensorFile::load_meta(&path).expect("load swapped artifact metadata");
    for (key, value) in [
        ("artifact.layout", "daily-single-tensor/v1"),
        (MAPPING_IDENTITY_KEY, MAPPING_IDENTITY),
    ] {
        assert!(
            meta.user_kv
                .iter()
                .any(|pair| pair.key == key && pair.value == value),
            "file-level metadata {key}={value} must survive the override swap"
        );
    }
    assert_eq!(meta.dims[0].name.as_deref(), Some("day"));
    assert_eq!(meta.dims[1].name.as_deref(), Some("payload"));
    assert_eq!(meta.append_dim, 0);
    assert_eq!(
        last_appended_day(&file).expect("swapped last day"),
        Some(DAY_THREE)
    );
    assert!(day_is_appendable(&file, DAY_FOUR).expect("appendability check"));
    file.append_f64(&day_entry(DAY_FOUR, 30.0), &entry_shape())
        .expect("append after the swap");
    assert_eq!(
        file.read_all().expect("read after post-swap append").shape,
        vec![4, ENTRY_WIDTH as u64]
    );

    // In-place/automatic compaction remains explicitly unsupported.
    let err = file
        .maybe_compact_auto()
        .expect_err("automatic in-place compaction is unsupported");
    assert_eq!(err.code(), ErrorCode::Unimplemented);

    drop(file);
    let _ = fs::remove_dir_all(&directory);
}

fn unique_path(name: &str) -> PathBuf {
    unique_directory("daily-artifact-tests").join(format!("{}-{}", unique_counter(), name))
}

fn unique_directory(name: &str) -> PathBuf {
    let dir = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"))
        .join(name);
    fs::create_dir_all(&dir).expect("create project-local daily artifact test directory");
    dir
}

fn cleanup_file(path: &Path) {
    let _ = fs::remove_file(path);
    // The V4 mutation lock is the adjacent `<artifact>.lock` path.
    let mut lock_path = path.as_os_str().to_os_string();
    lock_path.push(".lock");
    let _ = fs::remove_file(PathBuf::from(lock_path));
}

fn unique_counter() -> usize {
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
