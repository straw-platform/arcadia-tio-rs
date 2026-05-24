# arcadia-tio-rs

Safe Rust wrapper over the compiled `arcadia_tio_capi` native C ABI library.

This crate is source-visible wrapper code only. It depends on
`arcadia-tio-sys` for raw FFI declarations and link discovery; it does not
depend on the private `arcadia-tio` Rust implementation crate in its normal
consumer build path.

The API slice is intentionally bounded but now covers the agreed public Rust
17-family parity scope for beta workflows: safe lifecycle ownership, owned
error strings, create/open metadata types, policy/inferred create helpers,
write-forward compression selection, bulk tensor I/O helpers, f32/f64/i32/i64
sparse-intent analysis and append helpers, universe-aware create/append
authoring, current read options and shape policies, historical
`read_at_commit` options and shape policies, retained-history head/list helpers,
f32/f64 rewrite, rewrite-slice, pop/pop-batched/revert and clear-block mutation
helpers, scoped reform and compaction workflows, index/chunk-plan inspection,
and metadata administration setters for dimension names, axis labels, and user
key/value metadata. Metadata/index mutation calls surface the native status;
currently unsupported native layouts return ordinary `Unimplemented` errors
without implying release readiness or a public maintainer workflow guarantee.
It also exposes opt-in current-read query-attribution helpers
(`read_with_options_attributed` and `read_with_options_dense_attributed`) that
return the normal tensor/dense output plus native diagnostic trace JSON copied
into Rust-owned memory, plus bounded low-level interop helpers for native
`read_index` selection and Arrow C Data value export. Query attribution and
Arrow export are API-completeness/interoperability surfaces only: they are not
benchmark evidence and do not create performance, phase-percentage, zero-copy,
storage, cache, layout, or release-readiness claims.

Append, sparse-intent analysis, mutation, reform, compaction, diagnostics, and
attributed read helpers borrow Rust slices/paths/trace-context strings only for
the duration of one bulk FFI call, validate dtype/rank/shape/data length before
crossing the ABI where possible, and return or surface the native status. Read
and report helpers copy
native-owned tensor/mask/report/trace outputs into Rust-owned values and
immediately free the C allocation. `read_values_arrow` is the exception: it
returns an `ArrowCData` RAII owner for the Arrow C Data release callbacks;
borrowed Arrow pointers are valid only while that owner is alive and are
released exactly once when it is dropped. This slice does not expose generic
zero-copy borrowed views over native buffers.

Inline numeric coordinate lookup is exposed for validated `i32`/`i64` axis
coordinates through `TensorFile::coordinate_index_i32/i64` and
`TensorFile::coordinate_range_i32/i64`. Exact lookup returns an axis index;
range lookup returns a half-open `Range<u32>` for an inclusive coordinate
interval. External, string/dictionary, timezone/calendar interpretation, and
index-accelerated coordinate lookup remain deferred.

## Example

```rust,no_run
use arcadia_tio_rs::{
    AxisKind, CompressionConfig, CoordinateEncoding, CoordinateKind, CoordinateMonotonicity,
    CoordinateOrdering, CoordinateSpec, CoordinateStorage, CoordinateSortedness,
    CoordinateUniqueness, CoordinateValues, CreateOptions, DType, DimSpec, TensorData, TensorFile,
};

# fn main() -> arcadia_tio_rs::Result<()> {
let path = std::env::temp_dir().join("example.tio");
let mut options = CreateOptions::streaming(
    DType::F64,
    vec![DimSpec::new(AxisKind::Time, 0), DimSpec::new(AxisKind::Symbol, 3)],
    0,
);
// Leaving `compression` as None inherits the native persisted Auto/Zstd
// write policy. Set an explicit override only when the file should force
// uncompressed writes or force zstd for future appends.
options.compression = Some(CompressionConfig::zstd_level(3));
options.coordinates.push(CoordinateSpec {
    axis: 1,
    name: Some("day".to_string()),
    kind: CoordinateKind::Date,
    encoding: CoordinateEncoding::DateYyyymmdd,
    storage: CoordinateStorage::Inline(CoordinateValues::I32(vec![20260514, 20260515, 20260516])),
    ordering: CoordinateOrdering {
        sorted: CoordinateSortedness::Ascending,
        monotonicity: CoordinateMonotonicity::StrictlyIncreasing,
        uniqueness: CoordinateUniqueness::Unique,
    },
    required: true,
});

{
    let mut file = TensorFile::create(&path, options)?;
    file.append_f64(&[1.0, 2.0, 3.0], &[1, 3])?;
}

let file = TensorFile::open(&path)?;
let tensor = file.read_all()?;
assert_eq!(tensor.shape, vec![1, 3]);
assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0]));
assert_eq!(file.coordinate_index_i32(1, 20260515)?, 1);
assert_eq!(file.coordinate_range_i32(1, 20260514, 20260516)?, 0..3);

let indexed = file.read_index(&[
    arcadia_tio_rs::ReadIndexItem::all(),
    arcadia_tio_rs::ReadIndexItem::slice(Some(1), None, 1)?,
])?;
assert_eq!(indexed.value.data, TensorData::F64(vec![2.0, 3.0]));

let head = file.head_commit()?;
let visible_commits = file.list_commits(Some(8))?;
assert_eq!(visible_commits.first().map(|commit| commit.commit_seq), Some(head.commit_seq));

let chunk_plan = file.chunk_plan()?;
assert_eq!(chunk_plan.block_sizes.len(), file.rank()?);

let arrow = file.read_values_arrow()?;
assert!(arrow.array().release.is_some());
assert!(arrow.schema().release.is_some());
drop(arrow); // releases the Arrow C Data callbacks exactly once.
# Ok(())
# }
```

## Sparse-intent analysis and append

The safe wrapper exposes the native sparse-intent surface for f32/f64 payloads
and the bounded i32/i64 zero/null first slice:

- `SparseRule::null_subtensor(...)` and
  `SparseRule::predicate_subtensor(..., SparseValuePredicate::Zero | Nan |
  EqualF32(_) | EqualF64(_))` build owned rules. Integer payloads accept
  `null_subtensor` and `Zero` only; integer `Nan`, floating exact predicates,
  and arbitrary exact integer predicates are rejected/deferred.
- `TensorFile::analyze_sparse_append_f32` / `analyze_sparse_append_f64` /
  `analyze_sparse_append_i32` / `analyze_sparse_append_i64` return a Rust-owned
  `SparseAppendAnalysis` with `SparseAppendOutcome` and `SparseAppendReason`
  values copied from the native analysis and freed on the C side before
  returning.
- `append_sparse_f32` / `append_sparse_f64` keep the compatibility status-only
  shape. `append_sparse_f32_returning_range` /
  `append_sparse_f64_returning_range` are readability aliases for the older
  `_with_range` methods when callers need the assigned `AppendRange`.
  Newly-added `append_sparse_i32` / `append_sparse_i64` return the assigned
  `AppendRange` directly. All sparse append helpers execute the same native
  sparse-intent decision path and either append through the selected/fallback
  path or return a wrapper error for native rejects.

```rust,no_run
# use arcadia_tio_rs::{AxisKind, CreateOptions, DType, DimSpec, SparseAppendOutcome, SparseRule, SparseValuePredicate, TensorFile};
# fn main() -> arcadia_tio_rs::Result<()> {
# let path = std::env::temp_dir().join("sparse-example.tio");
let options = CreateOptions::streaming(
    DType::F32,
    vec![DimSpec::new(AxisKind::Time, 0), DimSpec::new(AxisKind::Channel, 2)],
    0,
);
let mut file = TensorFile::create(&path, options)?;
let rule = SparseRule::predicate_subtensor(vec![1], SparseValuePredicate::Zero);
let analysis = file.analyze_sparse_append_f32(&[0.0, 0.0, 1.0, 2.0], &[2, 2], &rule)?;
assert!(matches!(
    analysis.outcome,
    SparseAppendOutcome::DenseFallback
        | SparseAppendOutcome::SparseRegularChunked
        | SparseAppendOutcome::SparseChunkTree
));
let range = file.append_sparse_f32_returning_range(&[0.0, 0.0, 1.0, 2.0], &[2, 2], &rule)?;
assert_eq!((range.start, range.end), (0, 2));
# Ok(())
# }
```

Sparse-intent integer support remains deliberately narrow: i32/i64 wrappers are
limited to zero/null first-slice behavior and do not add exact integer predicate
carriers. Analysis outcomes are diagnostics for the current native lowering
decision, and append helpers preserve native semantics only: they are not
storage-efficiency, compression-ratio, physical-layout, capacity, or release
readiness claims.

## Parity caveats

Within the maintained API parity matrix, this crate reaches 17/17 source-visible
public Rust capability families for the agreed beta workflow scope. TP-359's
no-release handoff review reran the raw C-header/sys inventory, refreshed the
generated API signature snapshots for recent coordinate lookup additions, and
accepted no runtime ownership/parity blocker for this wrapper slice. This is not
broad parity with every private Rust maintainer hook. It currently covers bulk
create/open/append/read, f32/f64 sparse-intent analysis/append plus bounded
i32/i64 zero/null sparse-intent analysis/append, RegularChunked policy create,
inferred create, universe-aware authoring, current and historical read options, current and
historical read-shape policies, write-forward compression controls (default
create options inherit the native persisted Auto/Zstd policy unless callers
explicitly request uncompressed or zstd), metadata helpers, retained-history list/head helpers, scoped
f32/f64 rewrite, pop/pop-batched/revert, and clear-block mutation helpers,
index checkpoint/chunk-plan administration, non-precise reform/compaction
workflows including retained-history compaction reports, and V4
diagnostics/precise-accounting report APIs. Diagnostic current
query-attribution helpers are available as API-completeness access to native
trace JSON, and bounded read-index/Arrow C Data helpers expose native interop
vocabulary outside the original 17-family score. These remain outside
benchmark/performance evidence. This crate does not expose generic zero-copy
native views, exact integer sparse predicates, or compressed storage-accounting
eligibility claims. Coordinate lookup remains inline numeric-only: external coordinate value
resolution, string/dictionary coordinates, timezone/calendar interpretation, and
lookup acceleration are deferred. Pop/revert, metadata setter, index
checkpoint setter, clear-block, and unsupported auto-compaction calls
intentionally surface native policy/layout support errors.

## Local test/runtime library setup

Supply or copy the `arcadia_tio_capi` native shared library, then
point Cargo/linker discovery at the directory containing it:

```sh
LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
ARCADIA_TIO_CAPI_LIB_DIR="$LIB_DIR" \
LD_LIBRARY_PATH="$LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  cargo test -p arcadia-tio-rs
```

Use `DYLD_LIBRARY_PATH` instead of `LD_LIBRARY_PATH` on macOS. On Windows, make
sure the directory containing `arcadia_tio_capi.dll` is on `PATH` and set
`ARCADIA_TIO_CAPI_LIB_DIR` to the directory containing the import/native
library used at link time. Applications may also choose platform rpath,
install-name, or DLL-colocation strategies; runtime lookup remains the
consumer application's responsibility.
