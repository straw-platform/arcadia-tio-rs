use std::ffi::c_int;
use std::fs;
use std::mem::{align_of, size_of};
use std::path::{Path, PathBuf};

use arcadia_tio_sys::*;

const SYS_LIB: &str = include_str!("../src/lib.rs");

const EXPECTED_MISSING_DEFERRED_C_ABI_FUNCTIONS: &[&str] = &[];

const EXPECTED_MISSING_DEFERRED_C_ABI_TYPES: &[&str] = &[];

const INTENTIONALLY_EXCLUDED_C_ABI_TYPES: &[&str] = &[
    // `arcadia_tio_read_values_arrow` uses ArrowArray and ArrowSchema only.
    // ArrowArrayStream is present in the vendored Arrow C Data header for
    // completeness, but it is not part of the current TIO C ABI surface.
    "ArrowArrayStream",
];

const _: () = {
    assert!(size_of::<ArcadiaTioDType>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioErrorCode>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioV4PreciseAccountingField>() == size_of::<c_int>());
    assert!(ARCADIA_TIO_DTYPE_F32 == 0);
    assert!(ARCADIA_TIO_DTYPE_F64 == 1);
    assert!(ARCADIA_TIO_DTYPE_I32 == 2);
    assert!(ARCADIA_TIO_DTYPE_I64 == 3);
    assert!(ARCADIA_TIO_ERROR_OK == 0);
    assert!(ARCADIA_TIO_AXIS_TIME == 0);
    assert!(ARCADIA_TIO_AXIS_SYMBOL == 1);
    assert!(ARCADIA_TIO_COORDINATE_DTYPE_I32 == 0);
    assert!(ARCADIA_TIO_COORDINATE_KIND_DATE == 2);
    assert!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD == 2);
    assert!(ARCADIA_TIO_COORDINATE_STORAGE_INLINE == 0);
    assert!(ARCADIA_TIO_HEADER_PROFILE_STREAMING == 0);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_ALL == 0);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_RANGE == 1);
    assert!(ARCADIA_TIO_ENTRY_SELECTOR_TAKE == 2);
    assert!(ARCADIA_TIO_READ_EXECUTION_SERIAL == 0);
    assert!(ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS == 1);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE == 0);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE == 6);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS == 7);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY == 0);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE == 1);
    assert!(ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT == 0);
    assert!(ARCADIA_TIO_COMPACTION_COPY_LIVE == 0);
    assert!(ARCADIA_TIO_COMPACTION_REBLOCK == 1);
    assert!(ARCADIA_TIO_REFORM_TARGET_PRESERVE_FAMILY == 0);
    assert!(ARCADIA_TIO_REFORM_TARGET_WHOLE_APPEND_UNIT == 1);
    assert!(ARCADIA_TIO_REFORM_TARGET_REGULAR_CHUNKED == 2);
    assert!(ARCADIA_TIO_V4_REPORT_COMPLETE == 0);
    assert!(ARCADIA_TIO_V4_REPORT_UNSUPPORTED == 1);
    assert!(ARCADIA_TIO_V4_REPORT_UNKNOWN == 2);
    assert!(ARCADIA_TIO_V4_COMPACTION_POLICY_COMPACT_TO_CURRENT_STATE == 0);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_UNREACHABLE_BYTES == 0);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RETAINED_HISTORY_REQUIRED_BYTES == 1);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_POPPED_SKIPPED_BYTES == 2);
    assert!(ARCADIA_TIO_V4_PRECISE_ACCOUNTING_RECLAIMABLE_BYTES == 3);
    assert!(ARCADIA_TIO_V4_RETAINED_HISTORY_RETAIN_LAST == 0);
    assert!(ARCADIA_TIO_SPARSE_DETECTOR_NULL_SUBTENSOR == 0);
    assert!(ARCADIA_TIO_SPARSE_DETECTOR_PREDICATE_SUBTENSOR == 1);
    assert!(ARCADIA_TIO_SPARSE_PREDICATE_NAN == 0);
    assert!(ARCADIA_TIO_SPARSE_PREDICATE_ZERO == 1);
    assert!(ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F32 == 2);
    assert!(ARCADIA_TIO_SPARSE_PREDICATE_EQUAL_F64 == 3);
    assert!(ARCADIA_TIO_SPARSE_FALLBACK_DENSE == 0);
    assert!(ARCADIA_TIO_SPARSE_APPEND_SPARSE_REGULAR_CHUNKED == 0);
    assert!(ARCADIA_TIO_SPARSE_APPEND_DENSE_FALLBACK == 1);
    assert!(ARCADIA_TIO_SPARSE_APPEND_REJECT == 2);
    assert!(ARCADIA_TIO_SPARSE_APPEND_SPARSE_CHUNK_TREE == 3);
    assert!(
        ARCADIA_TIO_SPARSE_REASON_CURRENT_SPARSE_LOWERING_NOT_YET_IMPLEMENTED_FOR_DETECTOR == 16
    );
    assert!(ARCADIA_TIO_READ_INDEX_ALL == 0);
    assert!(ARCADIA_TIO_READ_INDEX_ELLIPSIS == 4);
    assert!(ARCADIA_TIO_READ_INDEX_LOWERING_UNKNOWN == 0);
    assert!(ARCADIA_TIO_READ_INDEX_LOWERING_SELECTOR_READ_WITH_SHAPE_POSTPROCESS == 2);
    assert!(ARCADIA_TIO_STORAGE_BALANCED == 0);
    assert!(ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ == 1);
    assert!(ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT == 0);
    assert!(ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED == 0);
    assert!(ARCADIA_TIO_METADATA_STABILITY_STABLE == 0);
};

#[test]
fn deferred_c_abi_gap_matches_expected_inventory() {
    let Some(headers) = load_private_c_headers() else {
        eprintln!(
            "private C headers not present in this source-visible checkout; skipping header/sys inventory comparison"
        );
        return;
    };

    let header_functions = collect_c_functions(&headers.functions);
    let sys_functions = collect_sys_functions(SYS_LIB);
    let missing_functions = sorted_difference(&header_functions, &sys_functions);
    assert_eq!(
        missing_functions, EXPECTED_MISSING_DEFERRED_C_ABI_FUNCTIONS,
        "C header/sys function gap changed; classify new drift or update declarations"
    );

    let mut header_types = collect_c_types(&headers.types);
    header_types.extend(collect_arrow_types(&headers.arrow));
    let sys_types = collect_sys_types(SYS_LIB);
    let mut missing_types = sorted_difference(&header_types, &sys_types);
    missing_types.retain(|name| !INTENTIONALLY_EXCLUDED_C_ABI_TYPES.contains(&name.as_str()));
    assert_eq!(
        missing_types, EXPECTED_MISSING_DEFERRED_C_ABI_TYPES,
        "C header/sys type gap changed; classify new drift or update declarations"
    );
}

struct PrivateCHeaders {
    functions: String,
    types: String,
    arrow: String,
}

fn load_private_c_headers() -> Option<PrivateCHeaders> {
    let root = private_repo_root()?;
    Some(PrivateCHeaders {
        functions: fs::read_to_string(
            root.join("crates/arcadia-tio-capi/include/arcadia/tio/functions.h"),
        )
        .ok()?,
        types: fs::read_to_string(root.join("crates/arcadia-tio-capi/include/arcadia/tio/types.h"))
            .ok()?,
        arrow: fs::read_to_string(
            root.join("crates/arcadia-tio-capi/include/arcadia/tio/arrow_c_data.h"),
        )
        .ok()?,
    })
}

fn private_repo_root() -> Option<PathBuf> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.ancestors().nth(3).map(Path::to_path_buf)
}

fn collect_c_functions(header: &str) -> std::collections::BTreeSet<String> {
    let mut functions = std::collections::BTreeSet::new();
    for (idx, _) in header.match_indices("arcadia_tio_") {
        let tail = &header[idx..];
        let name: String = tail
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
            .collect();
        let next = tail[name.len()..].trim_start().chars().next();
        if next == Some('(') {
            functions.insert(name);
        }
    }
    functions
}

fn collect_sys_functions(sys: &str) -> std::collections::BTreeSet<String> {
    sys.lines()
        .filter_map(|line| line.trim_start().strip_prefix("pub fn "))
        .filter_map(|tail| tail.split('(').next())
        .map(str::trim)
        .filter(|name| name.starts_with("arcadia_tio_"))
        .map(str::to_owned)
        .collect()
}

fn collect_c_types(header: &str) -> std::collections::BTreeSet<String> {
    let mut types = std::collections::BTreeSet::new();
    for marker in ["typedef struct ", "typedef enum "] {
        for (idx, _) in header.match_indices(marker) {
            let after_marker = &header[idx + marker.len()..];
            let name: String = after_marker
                .chars()
                .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
                .collect();
            if name.starts_with("ArcadiaTio") {
                types.insert(name);
            }
        }
    }
    types
}

fn collect_arrow_types(header: &str) -> std::collections::BTreeSet<String> {
    header
        .lines()
        .filter_map(|line| line.trim_start().strip_prefix("struct "))
        .filter_map(|tail| tail.split_whitespace().next())
        .map(|name| name.trim_matches(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_')))
        .filter(|name| name.starts_with("Arrow"))
        .map(str::to_owned)
        .collect()
}

fn collect_sys_types(sys: &str) -> std::collections::BTreeSet<String> {
    let mut types = std::collections::BTreeSet::new();
    for line in sys.lines().map(str::trim_start) {
        for marker in ["pub type ", "pub struct ", "pub enum "] {
            let Some(tail) = line.strip_prefix(marker) else {
                continue;
            };
            let Some(name) = tail
                .split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '_'))
                .next()
            else {
                continue;
            };
            if name.starts_with("ArcadiaTio") || name.starts_with("Arrow") {
                types.insert(name.to_owned());
            }
        }
    }
    types
}

fn sorted_difference(
    left: &std::collections::BTreeSet<String>,
    right: &std::collections::BTreeSet<String>,
) -> Vec<String> {
    left.difference(right).cloned().collect()
}

#[test]
fn sparse_integer_append_symbols_are_declared() {
    for name in [
        "arcadia_tio_analyze_sparse_append_i32",
        "arcadia_tio_analyze_sparse_append_i64",
        "arcadia_tio_append_sparse_i32",
        "arcadia_tio_append_sparse_i64",
        "arcadia_tio_append_sparse_i32_with_range",
        "arcadia_tio_append_sparse_i64_with_range",
    ] {
        assert!(
            SYS_LIB.contains(&format!("pub fn {name}(")),
            "missing sys declaration for {name}"
        );
    }
}

#[test]
fn representative_raw_layouts_are_pointer_compatible() {
    assert_eq!(align_of::<ArcadiaTioTensor>(), align_of::<usize>());
    assert_eq!(
        align_of::<ArcadiaTioAxisCoordinateInput>(),
        align_of::<usize>()
    );

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<ArcadiaTioAxisCoordinateInput>(), 120);
        assert_eq!(size_of::<ArcadiaTioEntrySelector>(), 32);
        assert_eq!(size_of::<ArcadiaTioChunkKey>(), 16);
        assert_eq!(size_of::<ArcadiaTioReadShapePolicyOptions>(), 72);
        assert_eq!(size_of::<ArcadiaTioReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioHistoricalReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioReadWithShapePolicyOptions>(), 104);
        assert_eq!(
            size_of::<ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            104
        );
        assert_eq!(size_of::<ArcadiaTioCreateWithUniverseOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioAppendWithUniverseOptions>(), 48);
        assert_eq!(size_of::<ArcadiaTioCompactionMode>(), 8);
        assert_eq!(size_of::<ArcadiaTioCompactionStats>(), 32);
        assert_eq!(size_of::<ArcadiaTioReformOptions>(), 40);
        assert_eq!(size_of::<ArcadiaTioReformReport>(), 40);
        assert_eq!(size_of::<ArcadiaTioV4PreciseAccountingOptions>(), 24);
        assert_eq!(size_of::<ArcadiaTioV4OmittedPreciseAccountingField>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4PreciseAccountingBytes>(), 112);
        assert_eq!(size_of::<ArcadiaTioV4CurrentHeadBytes>(), 40);
        assert_eq!(size_of::<ArcadiaTioV4AuditBytes>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4PayloadReuseBytes>(), 16);
        assert_eq!(size_of::<ArcadiaTioV4SupersededBytes>(), 32);
        assert_eq!(size_of::<ArcadiaTioV4DiagnosticsReport>(), 176);
        assert_eq!(size_of::<ArcadiaTioV4DiagnosticsPreciseReport>(), 280);
        assert_eq!(size_of::<ArcadiaTioV4CompactionAnalysisReport>(), 88);
        assert_eq!(
            size_of::<ArcadiaTioV4CompactionAnalysisPreciseReport>(),
            192
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionOptions>(),
            24
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionReport>(),
            104
        );
        assert_eq!(
            size_of::<ArcadiaTioV4RetainedHistoryCompactionPreciseReport>(),
            208
        );
        assert_eq!(size_of::<ArcadiaTioAutoCompactionConfig>(), 40);
        assert_eq!(size_of::<ArcadiaTioCompactionState>(), 16);
        assert_eq!(size_of::<ArrowArray>(), 80);
        assert_eq!(size_of::<ArrowSchema>(), 72);
        assert_eq!(size_of::<ArcadiaTioSparseValuePredicate>(), 16);
        assert_eq!(size_of::<ArcadiaTioSparseRule>(), 64);
        assert_eq!(size_of::<ArcadiaTioSparseAppendAnalysis>(), 48);
        assert_eq!(size_of::<ArcadiaTioQueryTraceContext>(), 80);
        assert_eq!(size_of::<ArcadiaTioQueryTraceJson>(), 24);
        assert_eq!(size_of::<ArcadiaTioReadIndexItem>(), 48);
        assert_eq!(size_of::<ArcadiaTioReadIndexReport>(), 32);
        assert_eq!(size_of::<ArcadiaTioChunkPlan>(), 16);
        assert_eq!(size_of::<ArcadiaTioCommitInfo>(), 24);
        assert_eq!(size_of::<ArcadiaTioCommitList>(), 16);
    }

    #[cfg(target_pointer_width = "32")]
    {
        assert!(size_of::<ArcadiaTioAxisCoordinateInput>() >= 72);
        assert_eq!(size_of::<ArcadiaTioChunkKey>(), 8);
    }
}
