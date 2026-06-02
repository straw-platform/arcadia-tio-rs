# arcadia-tio-rs

Safe Rust wrapper over the compiled `arcadia_tio_capi` native C ABI library.

This crate is source-visible wrapper code only. It depends on
`arcadia-tio-sys` for raw FFI declarations and link discovery; it does not
depend on the private `arcadia-tio` Rust implementation crate in its normal
consumer build path.

The API slice is intentionally bounded but now covers the agreed public Rust
17-family parity scope for beta workflows: safe lifecycle ownership, owned
error strings, create/open metadata types, policy/inferred create helpers,
write-forward compression selection, bulk tensor I/O helpers, owned in-memory
shape/index/math/reduction tensor operation helpers for f32/f64/i32/i64 dense
payloads, sparse-intent analysis and append helpers, universe-aware create/append
authoring, bounded Current coordinate create/read/lookup/append helpers,
current read options and shape policies, historical
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
`read_index` selection and Arrow C Data value export. Optional non-default
`arrow` and `ndarray` Cargo features add owned-copy `Tensor` conversions to
Arrow `RecordBatch`/IPC bytes and Rust `ndarray::ArrayD<T>` for dense
f32/f64/i32/i64 payloads. Query attribution, Arrow C Data export, and owned
conversion helpers are API-completeness/interoperability surfaces only: they are
not benchmark evidence and do not create performance, phase-percentage,
zero-copy, storage, cache, layout, or release-readiness claims.

Append, sparse-intent analysis, mutation, reform, compaction, diagnostics, and
attributed read helpers borrow Rust slices/paths/trace-context strings only for
the duration of one bulk FFI call, validate dtype/rank/shape/data length before
crossing the ABI where possible, and return or surface the native status. Read
and report helpers copy
native-owned tensor/mask/report/trace outputs into Rust-owned values and
immediately free the C allocation. `read_values_arrow` is the exception: it
returns an `ArrowCData` RAII owner for the Arrow C Data release callbacks;
borrowed Arrow pointers are valid only while that owner is alive and are
released exactly once when it is dropped. The public `ops` namespace provides
owned-copy in-memory helpers over dense `TensorData`; optional conversion
features are also owned-copy only. This slice still does not expose generic
zero-copy borrowed views over native buffers, private/core tensor view types,
Arrow CSV/Parquet adapters, direct Python NumPy integration, or C++ convenience
APIs.

Inline numeric coordinate authoring and lookup are exposed for validated
`i32`/`i64` axis coordinates. `CreateOptions::coordinates` can be used with
streaming/random-access create plus supported policy and inferred create helpers
for fixed non-append axes; the native API validates coordinate lengths against
the selected axis extent. `TensorFile::coordinate_index_i32/i64` and
`TensorFile::coordinate_range_i32/i64` provide exact and monotonic range lookup;
`TensorFile::read_at_coordinate_i32/i64` and
`TensorFile::read_coordinate_range_i32/i64` compose those lookups with ordinary
axis-range reads and return the same `Tensor` shapes as selector reads. Exact
lookup returns an axis index; range lookup returns a half-open `Range<u32>` for
an inclusive coordinate interval. The read conveniences are ergonomic helpers,
not coordinate-index acceleration.

Current coordinate wrappers are available under unsuffixed canonical names, with
explicit `*_v2` aliases retained for source compatibility. Use
`AxisCoordinateInput` builders with `TensorFile::create_with_coordinates`,
`create_inferred_with_coordinates`, or `create_with_policy_with_coordinates` for
implemented source-only domains:
inline numeric values, fixed-width ASCII/right-space-padded text,
dictionary-code coordinates with create-time dictionary entries, append-axis
coordinate declarations, and descriptor-only external-reference summaries.
`AppendCoordinateEntry`/`AppendCoordinateBatch` carry append-time coordinate
values for append-axis descriptors: numeric `i32`/`i64` vectors, fixed-width
ASCII/right-space-padded byte buffers or strings, and dictionary code vectors
that may include append-time dictionary-extension entries attached with
`with_dictionary_entries`/`dictionary_codes_*_with_entries`.
`TensorFile::append_f32_with_coordinates`, `append_f64_with_coordinates`,
`append_i32_with_coordinates`, and `append_i64_with_coordinates` append the
payload plus a batch and return the native half-open `AppendRange`; missing
required coordinates, wrong counts, domain/dtype mismatches, and publication
conflicts are reported through the native status/last-error path and publish no
payload-only fallback root.
`TensorFile::coordinate_metadata`, `load_coordinate_metadata`,
`read_coordinate_axis`, `coordinate_dictionary`, `coordinate_lookup`, and
`coordinate_lookup_range` copy native-owned metadata/value/dictionary/lookup
outputs into Rust-owned structs/bytes before calling the paired C free function.
Build lookup keys with `CoordinateLookupKey::i32`, `i64`,
`fixed_text_ascii`/`fixed_text_bytes`,
`dictionary_code`, `stable_id`, `display_label`, `alias`, or `raw_time_i64`.
Lookup results preserve `status`, `status_category`, `availability`, `reason`,
unique positions, half-open ranges, and many-position vectors; ordinary missing,
unavailable, unsupported, duplicate, domain-mismatch, and index-status outcomes
are visible result statuses rather than opaque wrapper errors. Availability,
status category/reason, external summaries, dictionary summaries, and optional
index summaries remain visible as status/context; optional indexes are not
treated as authoritative coordinate truth, and callers must explicitly allow
selected-root authoritative scans with `CoordinateOptions::authoritative_scan`.
The public Rust wrapper does not dereference external references and does not add
variable-length strings, locale/collation/case folding, broad calendar or
resolver semantics, lookup-composed coordinate reads, or
benchmark/release/readiness claims.

## In-memory tensor operations

The `ops` module works entirely over Rust-owned `Tensor`/`TensorData` values and
is independent of the native C ABI after data has been read or constructed.
It supports dense `f32`, `f64`, `i32`, and `i64` payloads for row-major shape
helpers (`reshape`, `flatten`, `expand_dims`, `squeeze`, `permute_axes`,
`swap_axes`, `transpose`, `move_axis`, and `broadcast_to`), axis indexing
(`slice_axis`, `slice_axis_step`, `take_axis`, and `index_axis`), exact-dtype
scalar/binary arithmetic with binary broadcasting (`add`, `sub`, `mul`, `div`
and the `*_scalar` variants), and `sum`/`mean`/`min`/`max` reductions over
selected axes.

```rust,no_run
# use arcadia_tio_rs::{ops, Scalar, Tensor, TensorData};
# fn main() -> arcadia_tio_rs::Result<()> {
let tensor = Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])?;
let transposed = ops::transpose(&tensor)?;
assert_eq!(transposed.shape, vec![3, 2]);
assert_eq!(transposed.data, TensorData::F64(vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]));

let shifted = ops::add_scalar(&tensor, Scalar::F64(10.0))?;
assert_eq!(shifted.values_f64()?[0], 11.0);

let row_sums = ops::sum(&tensor, Some(&[1]), false)?;
assert_eq!(row_sums.data, TensorData::F64(vec![6.0, 15.0]));
# Ok(())
# }
```

These helpers validate dtype/shape/payload consistency before operating and
materialize new owned row-major tensors with fallible allocation checks for large
outputs. They do not propagate dense validity masks, null bitmaps, Arrow arrays,
borrowed native buffers, or private view semantics. Binary operations require
exact dtype matching; integer arithmetic is checked; integer division by zero
returns an error; integer `mean` returns an `f64` tensor because fractional
results cannot be represented in the original integer dtype; and all-axis
reductions must use `keepdims = true` because public `Tensor` does not represent
rank-0 scalar outputs.

## Optional Arrow and ndarray conversion features

Default features remain empty. Enable `arrow` and/or `ndarray` only when an
application wants the extra public Rust ecosystem dependencies:

```toml
[dependencies]
arcadia-tio-rs = { path = "crates/arcadia-tio-rs", features = ["arrow", "ndarray"] }
```

The `arrow` feature converts owned dense `Tensor` values to a companion Arrow
`RecordBatch` layout or Arrow IPC file bytes and back. The companion layout uses
one `time_id` column plus one positive-width fixed-size-list `values` column, so
zero-sized inner dimensions such as shape `[N, 0]` are rejected instead of being
encoded. This is separate from `TensorFile::read_values_arrow()`, which exports
native Arrow C Data pointers with RAII release callbacks tied to the returned
owner.

```rust,no_run
# use arcadia_tio_rs::{Tensor, TensorData};
# fn main() -> arcadia_tio_rs::Result<()> {
# #[cfg(feature = "arrow")]
# {
let tensor = Tensor::from_dense_f32(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0])?;
let batch = tensor.to_arrow_record_batch()?;
assert_eq!(batch.num_rows(), 2);

let ipc = tensor.to_arrow_ipc()?;
let decoded = Tensor::from_arrow_ipc(&ipc)?;
assert_eq!(decoded.data, TensorData::F32(vec![1.0, 2.0, 3.0, 4.0]));
# }
# Ok(())
# }
```

The `ndarray` feature converts owned dense f32/f64/i32/i64 tensors to and from
Rust `ndarray::ArrayD<T>` values with dtype, rank, shape, and payload-length
validation. It does not add NumPy, PyO3, or Python bindings.

```rust,no_run
# use arcadia_tio_rs::{Tensor, TensorData};
# fn main() -> arcadia_tio_rs::Result<()> {
# #[cfg(feature = "ndarray")]
# {
let tensor = Tensor::from_dense_i64(vec![2, 2], vec![10, 20, 30, 40])?;
let array = tensor.to_ndarray_i64()?;
assert_eq!(array.shape(), &[2, 2]);

let rebuilt = Tensor::from_ndarray_i64(array)?;
assert_eq!(rebuilt.data, TensorData::I64(vec![10, 20, 30, 40]));
# }
# Ok(())
# }
```

Run feature tests explicitly when using these conversions:

```sh
cargo test -p arcadia-tio-rs --features arrow,ndarray
```

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
assert_eq!(file.read_at_coordinate_i32(1, 20260515)?.data, TensorData::F64(vec![2.0]));
assert_eq!(file.read_coordinate_range_i32(1, 20260514, 20260516)?.shape, vec![1, 3]);

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
and the bounded i32/i64 zero/null/exact-integer first slice:

- `SparseRule::null_subtensor(...)` and
  `SparseRule::predicate_subtensor(..., SparseValuePredicate::Zero | Nan |
  EqualF32(_) | EqualF64(_) | EqualI32(_) | EqualI64(_))` build owned rules.
  Integer payloads accept `null_subtensor`, `Zero`, and matching exact
  `EqualI32`/`EqualI64` predicates; integer `Nan`, floating exact predicates,
  and mismatched integer predicates are rejected.
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

Sparse-intent integer support remains deliberately bounded: i32/i64 wrappers
support zero/null first-slice behavior plus exact `EqualI32`/`EqualI64`
predicate carriers. Analysis outcomes are diagnostics for the current native
lowering decision, and append helpers preserve native semantics only: they are
not storage-efficiency, compression-ratio, physical-layout, capacity, or release
readiness claims.

## Parity caveats

Within the maintained API parity matrix, this crate reaches 17/17 source-visible
public Rust capability families for the agreed beta workflow scope. TP-370's
source-only handoff review rechecked coordinate authoring, exact integer sparse
predicates, coordinate read conveniences, generated signatures, and FFI
ownership, and accepted no runtime ownership/parity blocker for this wrapper
slice. This is not broad parity with every private Rust maintainer hook, private
zero-copy tensor view, typed tensor wrapper, Arrow CSV/Parquet adapters beyond
the owned RecordBatch/IPC feature, Python NumPy/Arrow convenience, or C++
DenseTensor helper.
It currently covers bulk create/open/append/read, owned in-memory
shape/index/math/reduction tensor ops over dense f32/f64/i32/i64 `TensorData`,
f32/f64 sparse-intent analysis/append plus bounded i32/i64
zero/null/exact-integer sparse-intent analysis/append, RegularChunked policy
create, inferred create, inline numeric coordinate-bearing policy/inferred create,
universe-aware authoring, current and historical read options, current and
historical read-shape policies, write-forward compression controls (default
create options inherit the native persisted Auto/Zstd policy unless callers
explicitly request uncompressed or zstd), metadata helpers, retained-history list/head helpers, scoped
f32/f64 rewrite, pop/pop-batched/revert, and clear-block mutation helpers,
index checkpoint/chunk-plan administration, non-precise reform/compaction
workflows including retained-history compaction reports, and V4
diagnostics/precise-accounting report APIs. Diagnostic current
query-attribution helpers are available as API-completeness access to native
trace JSON, bounded read-index/Arrow C Data helpers expose native interop
vocabulary outside the original 17-family score, and optional Arrow/ndarray
feature conversions expose owned-copy Rust ecosystem handoffs. These remain
outside benchmark/performance evidence. Tensor ops and conversion helpers are
owned-copy conveniences only and do not expose generic zero-copy native views,
mask/null propagation, direct NumPy integration, or compressed storage-accounting
eligibility claims. Legacy numeric coordinate lookup/read
conveniences remain inline numeric-only for fixed axes:
exact/range coordinate read helpers compose lookup with ordinary axis-range reads
and do not imply coordinate-index acceleration. Current coordinate create/read
wrappers cover only the implemented descriptor/value/dictionary/status/lookup
surfaces listed above; external value resolution, variable-length strings, broad
calendar/timezone interpretation, lookup-composed coordinate reads, and
authoritative index acceleration are deferred. Pop/revert, metadata setter, index
checkpoint setter, clear-block, and unsupported auto-compaction calls
intentionally surface native policy/layout support errors.

## Tutorial examples

User-facing tutorials live under `examples/tutorials/` and are registered as
Cargo example targets so nested source files remain runnable from this crate
manifest:

| Cargo example target | Scenario |
| --- | --- |
| `tutorial_01_quickstart_create_append_read` | Quickstart create/append/read/metadata |
| `tutorial_02_layouts_reads_history` | Layouts, selectors, shape policies, dense masks, and retained history |
| `tutorial_03_coordinates_numeric` | Numeric coordinate descriptors, values, exact/range lookup, and lookup-composed reads |
| `tutorial_04_coordinates_full_surface` | Current coordinate bounded create/read/lookup/append/status surfaces |
| `tutorial_05_sparse_append` | Sparse append analysis and f32/f64/i32/i64 zero/null/exact-integer predicates |
| `tutorial_06_mutation_history_universe` | Mutation/history helpers and explicit universe-aware remap reads |
| `tutorial_07_reform_compaction_diagnostics` | Reform, compaction, and native diagnostic report wrappers |
| `tutorial_08_compression_interop` | Compression controls, read-index lowering, and Arrow C Data interop |
| `tutorial_09_tensor_ops_conversions` | Owned dense tensor ops plus optional Arrow RecordBatch/IPC and ndarray conversions |

```sh
cargo run --example tutorial_01_quickstart_create_append_read
cargo run --example tutorial_08_compression_interop
cargo run --features arrow,ndarray --example tutorial_09_tensor_ops_conversions
```

Use the native-library environment below when running them. Default tutorial
examples build with empty default features; `tutorial_09_tensor_ops_conversions`
requires the opt-in `arrow,ndarray` features. The examples create tiny `.tio`
files and, for the feature-gated tutorial, a tiny Arrow IPC payload under OS temp
directories and clean them up; do not copy native libraries, Cargo build output,
generated `.tio` data, or generated IPC data into the tutorial tree or
source-only public checkout.

## Production integration checklist

Before shipping an application that uses this crate:

- Validate against the exact native `arcadia_tio_capi` library you intend to deploy.
- Set `ARCADIA_TIO_CAPI_LIB_DIR` for link discovery and configure runtime loader lookup separately.
- Run the workspace tests and tutorial examples with that library. The public checkout includes a Cargo target runner that automatically mirrors `ARCADIA_TIO_CAPI_LIB_DIR` or `native/<target>/lib` into the platform runtime-loader path for common Linux/macOS `cargo run` and `cargo test` invocations.
- Keep generated `.tio` data and native/package artifacts out of this source-only checkout unless a separate release task approves them.
- Preserve the documented API boundaries: coordinate external summaries are not dereferenced, optional indexes are not authoritative truth, and examples are not benchmark, storage, compression, capacity, or release-readiness evidence.

## Local test/runtime library setup

Supply or copy the `arcadia_tio_capi` native shared library, then
point Cargo/linker discovery at the directory containing it:

```sh
LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
ARCADIA_TIO_CAPI_LIB_DIR="$LIB_DIR" \
LD_LIBRARY_PATH="$LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  cargo test -p arcadia-tio-rs
```

The public checkout's Cargo target runner does the runtime-loader environment
step automatically for common Linux/macOS `cargo run` and `cargo test`
invocations when `ARCADIA_TIO_CAPI_LIB_DIR` or `native/<target>/lib` is present.
Use `DYLD_LIBRARY_PATH` instead of `LD_LIBRARY_PATH` on macOS when launching
outside Cargo's runner. On Windows, make sure the directory containing
`arcadia_tio_capi.dll` is on `PATH` and set `ARCADIA_TIO_CAPI_LIB_DIR` to the
directory containing the import/native library used at link time. Applications
may also choose platform rpath, install-name, or DLL-colocation strategies;
runtime lookup remains the consumer application's responsibility.
