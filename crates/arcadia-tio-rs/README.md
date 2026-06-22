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
shape/index/assembly/reordering/math/reduction tensor operation helpers for
f32/f64/i32/i64 dense payloads, public owned dtype-specific tensor wrappers and
typed operation forwarding, sparse-intent analysis and append helpers,
universe-aware create/append
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
`arrow`, `ndarray`, `csv`, and `parquet` Cargo features add owned-copy
`Tensor` conversion gates for dense f32/f64/i32/i64 payloads: Arrow
`RecordBatch`/IPC bytes, Rust `ndarray::ArrayD<T>`, and companion CSV/Parquet
layouts with explicit dtype/shape metadata. Query attribution, Arrow C Data
export, and owned conversion helpers are API-completeness/interoperability
surfaces only: they are not benchmark evidence and do not create performance,
phase-percentage, zero-copy, storage, cache, layout, external-format, or
release-readiness claims.

The non-default `format-ocb` feature exposes the appendable OCB (Ordered Column
Bundle) API in `arcadia_tio_rs::ocb`. Use `ocb::create` with a `WriteSpec` to
publish the first root, `ocb::append` to add sorted suffix commits that repeat
the frozen schema/dictionary/order declarations, `ColumnBundleFile::open` to
bind a handle to one committed snapshot, `open_with_options` when explicit
full-payload validation is required before reads, and `metadata`,
`dictionary_values`, and `read_batches` to copy native-owned OCB
metadata/dictionaries/batches into Rust-owned structs before the C buffers are
freed. For callers that need scheduling control before payload reads,
`plan_read` exposes snapshot-local projected column ids and row-group ids;
`read_plan_batches` executes the whole plan, and `read_plan_row_groups` executes
a duplicate/unknown-id-checked subset in deterministic plan order.
`read_batches_with_attribution` additionally returns diagnostic-only timing and
byte counters for planning, file reads, checksums, decompression, primitive
conversion, native C conversion, and wrapper copying. `visit_batches` consumes
row-group batches incrementally with `max_in_flight_row_groups` and callback
cancellation while still copying each callback batch into owned Rust values.
`read_row_group_into` fills caller-owned typed buffers selected by generic
column name or file-local column id for a single row-group id without constructing
a wrapper-owned `ReadOutcome`; discard buffers on error because partial writes
are unspecified. `clone_reader` cheaply
creates another handle for the same immutable selected snapshot, and the safe
wrapper marks `ColumnBundleFile` as `Send`/`Sync` for read-only multi-lane use.
`ocb::cleanup_orphan_tail` truncates orphan tail bytes after the latest valid
root. `OcbError` preserves the
ordinary C ABI error code plus OCB `ErrorKind` and optional `FailureCause` for
machine-readable handling. Dictionary-coded reads return primitive codes; use
`dictionary_values` for explicit decoded dictionary labels/bytes. OCB examples
and tests generate tiny project-local `.ocb` files and require an OCB-capable
`arcadia_tio_capi` native library with the `arcadia_tio_ocb_*` symbols exported;
if link fails with missing `arcadia_tio_ocb_create`, `arcadia_tio_ocb_append`,
`arcadia_tio_ocb_plan_read`, or related symbols, refresh the native library
before testing `format-ocb`.

OCB also supports generic fixed-width opaque byte columns for compact packed
payload storage. Declare the schema as `PhysicalType::FixedBinary { width }`,
write row-major bytes with `PrimitiveValues::FixedBinary { width, bytes }`, and
fill caller-owned buffers with `ColumnFillBufferMut::FixedBinary`; in all cases
`bytes.len()` must equal `row_count * width`. Fixed-binary columns are payload
columns only in the first slice: predicates, ordering keys, scalar statistics,
and dictionary-code fixed-binary columns fail closed, so scalar columns should
remain responsible for pruning and ordering. The `ocb_fixed_binary` example is a
minimal generic roundtrip:

```sh
cargo run --no-default-features --features format-ocb --example ocb_fixed_binary
```

Append, sparse-intent analysis, OCB create/append/read, mutation, reform,
compaction, diagnostics, and attributed read helpers borrow Rust
slices/paths/trace-context strings only for
the duration of one bulk FFI call, validate dtype/rank/shape/data length before
crossing the ABI where possible, and return or surface the native status. Read
and report helpers copy
native-owned tensor/mask/report/trace outputs into Rust-owned values and
immediately free the C allocation. `read_values_arrow` is the exception: it
returns an `ArrowCData` RAII owner for the Arrow C Data release callbacks;
borrowed Arrow pointers are valid only while that owner is alive and are
released exactly once when it is dropped. The public `ops` namespace provides
owned-copy in-memory helpers over dense `TensorData`; `TypedTensor<T>` and the
`typed_ops` namespace provide dtype-checked owned wrappers over that same public
`Tensor` model without depending on private core typed tensors; optional
conversion features are also owned-copy only. This slice still does not expose
generic zero-copy borrowed views over native buffers, private/core tensor view
semantics, typed file handles, native `.tio` file conversion shortcuts,
external-format storage claims, direct Python NumPy integration, or C++
convenience APIs.

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

## Write-forward compression controls

Create options inherit the native persisted Auto/Zstd write policy when
`CreateOptions::compression` is `None`. Prefer the safe constructors, enums, and
builder accessors when an application needs an explicit write-forward override:
`CompressionConfig::uncompressed()`, `CompressionConfig::try_zstd_level(level)`,
`CompressionConfig::auto_zstd()`, `CompressionConfig::auto_zstd_min_payload(bytes)`,
`CompressionMode`, and `CompressionCodec`.

```rust,no_run
# use arcadia_tio_rs::{CompressionCodec, CompressionConfig, CompressionMode};
# fn main() -> arcadia_tio_rs::Result<()> {
let compression = CompressionConfig::auto_zstd_min_payload(4096)
    .with_mode(CompressionMode::Auto)
    .with_codec(CompressionCodec::Zstd)
    .try_with_zstd_level(3)?;
assert_eq!(compression.mode()?, CompressionMode::Auto);
# Ok(())
# }
```

`CompressionConfig` also exposes its raw `mode`, `codec`, `min_payload_bytes`,
and `zstd_level` fields for low-level source compatibility with earlier wrapper
users and the C ABI. Treat those fields as an escape hatch: direct raw-field
construction is validated before native calls, unsupported raw codecs remain
errors, and this API documents write selection only rather than compression
ratio, storage-efficiency, or compressed-byte accounting claims.

## In-memory tensor operations

The `ops` module works entirely over Rust-owned `Tensor`/`TensorData` values and
is independent of the native C ABI after data has been read or constructed.
It supports dense `f32`, `f64`, `i32`, and `i64` payloads for row-major shape
helpers (`reshape`, `flatten`, `expand_dims`, `squeeze`, `permute_axes`,
`swap_axes`, `transpose`, `move_axis`, and `broadcast_to`), axis indexing
(`slice_axis`, `slice_axis_step`, `take_axis`, and `index_axis`), owned assembly
and reordering (`concat`, `stack`, `split`, `unstack`, `repeat`, `tile`, `flip`,
and `roll`), exact-dtype scalar/binary arithmetic with binary broadcasting
(`add`, `sub`, `mul`, `div` and the `*_scalar` variants), reductions
(`sum`, `mean`, `min`, `max`, `argmin`, `argmax`, `var`, and `std`) over
selected axes, and cumulative reductions (`cumsum` and `cumprod`).

```rust,no_run
# use arcadia_tio_rs::{ops, Scalar, Tensor, TensorData};
# fn main() -> arcadia_tio_rs::Result<()> {
let tensor = Tensor::from_dense_f64(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])?;
let transposed = ops::transpose(&tensor)?;
assert_eq!(transposed.shape, vec![3, 2]);
assert_eq!(transposed.data, TensorData::F64(vec![1.0, 4.0, 2.0, 5.0, 3.0, 6.0]));

let shifted = ops::add_scalar(&tensor, Scalar::F64(10.0))?;
assert_eq!(shifted.values_f64()?[0], 11.0);

let rows = ops::split(&tensor, 0, &[1, 1])?;
let restacked = ops::stack(&[&rows[0], &rows[1]], 0)?;
assert_eq!(restacked.data, tensor.data);

let rolled = ops::roll(&tensor, -1, 1)?;
assert_eq!(rolled.data, TensorData::F64(vec![3.0, 1.0, 2.0, 6.0, 4.0, 5.0]));

let row_sums = ops::sum(&tensor, Some(&[1]), false)?;
assert_eq!(row_sums.data, TensorData::F64(vec![6.0, 15.0]));

let row_argmax = ops::argmax(&tensor, Some(&[1]), false)?;
assert_eq!(row_argmax.data, TensorData::I64(vec![2, 2]));

let cumulative = ops::cumsum(&tensor, Some(-1))?;
assert_eq!(cumulative.data, TensorData::F64(vec![1.0, 3.0, 6.0, 4.0, 9.0, 15.0]));

let row_var = ops::var(&tensor, Some(&[1]), false)?;
assert_eq!(row_var.data, TensorData::F64(vec![2.0 / 3.0, 2.0 / 3.0]));
# Ok(())
# }
```

These helpers validate dtype/shape/payload consistency before operating and
materialize new owned row-major tensors with fallible allocation checks for large
outputs. Assembly helpers require exact dtype/rank compatibility, `stack`
requires identical input shapes, `split` sections must be non-empty and sum to
the selected axis length, `unstack` rejects rank-1 inputs that would produce
rank-0 tensors, and negative axes are normalized consistently with the other
axis helpers. Zero-repeat/tile or empty-axis operations produce owned empty
tensors when the resulting shape has zero elements. They do not propagate dense
validity masks, null bitmaps, Arrow arrays, borrowed native buffers, or private
view semantics. Binary operations require exact dtype matching; integer
arithmetic is checked; integer division by zero returns an error; integer
`mean`, `var`, and `std` return `f64` tensors because fractional results cannot
be represented in the original integer dtype; integer `cumsum`/`cumprod` use
checked arithmetic; and `argmin`/`argmax` return `i64` row-major offsets within
the reduced subspace. Variance and standard deviation are population statistics
(`ddof = 0`). Floating NaN comparisons use Rust `PartialOrd` semantics inherited
from `min`/`max`: unordered comparisons do not replace the current candidate.
All-axis reductions must use `keepdims = true` because public `Tensor` does not
represent rank-0 scalar outputs.

## Typed owned tensor wrappers and operations

Use `TypedTensor<T>` or the aliases `TensorF32`, `TensorF64`, `TensorI32`, and
`TensorI64` when a Rust call path should carry the expected dtype in the type
signature while remaining interoperable with existing untyped `Tensor` APIs.
The wrapper crate provides scalar support for dense `f32`, `f64`, `i32`, and
`i64` payloads. Constructors use the same public dense tensor validation as
`Tensor::from_dense_*`; `TryFrom<Tensor>`, `try_from_tensor`, `as_tensor`,
`inner`, and `into_tensor` provide fallible and consuming conversion boundaries.

```rust,no_run
# use arcadia_tio_rs::{typed_ops, DType, Tensor, TensorF64, TensorI32};
# fn main() -> arcadia_tio_rs::Result<()> {
let tensor = TensorF64::from_dense(vec![2, 3], vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0])?;
assert_eq!(tensor.dtype(), DType::F64);
assert_eq!(tensor.values()?, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

let shifted = typed_ops::add_scalar(&tensor, 10.0)?;
assert_eq!(shifted.values()?[0], 11.0);

let row_sums = typed_ops::sum(&tensor, Some(&[1]), false)?;
assert_eq!(row_sums.values()?, &[6.0, 15.0]);

let row_argmax = typed_ops::argmax(&tensor, Some(&[1]), false)?;
assert_eq!(row_argmax.values()?, &[2, 2]);

let raw: Tensor = row_sums.clone().into();
let rebuilt = TensorF64::try_from(raw)?;
assert_eq!(rebuilt, row_sums);

let ints = TensorI32::from_dense(vec![2, 2], vec![1, 2, 3, 4])?;
let cumulative = typed_ops::cumsum(&ints, Some(1))?;
assert_eq!(cumulative.values()?, &[1, 3, 3, 7]);
# Ok(())
# }
```

`typed_ops` forwards the bounded owned-copy operation slice from `ops` and then
validates result dtypes. It covers dtype-preserving shape/index/assembly,
reordering, scalar/binary arithmetic, `sum`/`min`/`max`, and cumulative helpers;
`argmin`/`argmax` return `TensorI64`. Dtype-promoting `mean`, `var`, and `std`
remain available through untyped `ops` on `typed.as_tensor()` because integer
inputs produce f64 outputs. Typed wrappers are not borrowed native views, do not
carry validity masks, do not expose `TypedTensorFile`/typed file handles, and do
not add a dependency on the private `arcadia-tio` core crate.

## Optional owned conversion features

Default features remain empty. Enable `arrow`, `ndarray`, `csv`, and/or
`parquet` only when an application wants the extra public Rust ecosystem
dependencies:

```toml
[dependencies]
arcadia-tio-rs = { path = "crates/arcadia-tio-rs", features = ["arrow", "ndarray", "csv", "parquet"] }
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

The `csv` and `parquet` feature gates are for companion owned-tensor conversion
layouts with explicit dtype, shape, row-major order, and flat-index metadata.
The `csv` feature exposes `Tensor::to_csv_string`, `to_csv_bytes`,
`from_csv_str`, and `from_csv_bytes`; the `parquet` feature exposes
`Tensor::to_parquet_bytes`, `to_parquet_file`, `from_parquet_bytes`, and
`from_parquet_file`. They are not native `.tio` storage formats, not Arrow
CSV/Parquet adapters, not benchmark or storage-efficiency evidence, and not
shortcuts for converting a native `TensorFile` path into another file format.

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

```rust,no_run
# use arcadia_tio_rs::{Tensor, TensorData};
# fn main() -> arcadia_tio_rs::Result<()> {
# #[cfg(all(feature = "csv", feature = "parquet"))]
# {
let tensor = Tensor::from_dense_f64(vec![2, 2], vec![1.0, 2.0, 3.0, 4.0])?;

let csv_text = tensor.to_csv_string()?;
let from_csv = Tensor::from_csv_str(&csv_text)?;
assert_eq!(from_csv.data, TensorData::F64(vec![1.0, 2.0, 3.0, 4.0]));

let parquet_bytes = tensor.to_parquet_bytes()?;
let from_parquet = Tensor::from_parquet_bytes(&parquet_bytes)?;
assert_eq!(from_parquet, tensor);
# }
# Ok(())
# }
```

Run feature tests explicitly when using these conversions. The exported public
checkout provides `cargo make ci` and `cargo make test-matrix` to cover default,
explicit `--no-default-features`, `arrow,ndarray`, `csv,parquet`, and
`--all-features` combinations once the local native library path is configured:

```sh
cargo make ci
cargo make test-matrix
cargo make test-arrow-ndarray
cargo make test-csv-parquet
```

From the private source repository, equivalent direct feature checks can target
this manifest:

```sh
cargo test -p arcadia-tio-rs --no-default-features --features arrow,ndarray
cargo test -p arcadia-tio-rs --no-default-features --features csv,parquet
```

## Example

```rust,no_run
use arcadia_tio_rs::{
    AxisKind, CompressionCodec, CompressionConfig, CompressionMode, CoordinateEncoding,
    CoordinateKind, CoordinateMonotonicity, CoordinateOrdering, CoordinateSpec, CoordinateStorage,
    CoordinateSortedness, CoordinateUniqueness, CoordinateValues, CreateOptions, DType, DimSpec,
    TensorData, TensorFile,
};

# fn main() -> arcadia_tio_rs::Result<()> {
let path = std::env::temp_dir().join("example.tio");
let mut options = CreateOptions::streaming(
    DType::F64,
    vec![DimSpec::new(AxisKind::Time, 0), DimSpec::new(AxisKind::Symbol, 3)],
    0,
);
// Leaving `compression` as None inherits the native persisted Auto/Zstd
// write policy. Safe builders/enums avoid raw `arcadia_tio_sys` constants when
// an explicit Auto/Zstd threshold or forced zstd policy is needed.
options.compression = Some(
    CompressionConfig::auto_zstd_min_payload(4096)
        .with_mode(CompressionMode::Auto)
        .with_codec(CompressionCodec::Zstd)
        .try_with_zstd_level(3)?,
);
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
zero-copy tensor view, typed file wrapper/private typed view semantics, Arrow
append/import, native `.tio` file conversion shortcuts for CSV/Parquet, Python
NumPy/Arrow convenience, or C++ DenseTensor helper.
It currently covers bulk create/open/append/read, owned in-memory
shape/index/assembly/reordering/math/reduction tensor ops over dense
f32/f64/i32/i64 `TensorData`, owned typed tensor wrappers and dtype-preserving
typed operation forwarding, f32/f64 sparse-intent analysis/append plus bounded i32/i64
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
vocabulary outside the original 17-family score, and optional Arrow/ndarray plus
CSV/Parquet feature conversions expose owned-copy Rust ecosystem and companion
format handoffs. These remain
outside benchmark/performance evidence. Tensor ops, typed wrappers, and
conversion helpers are owned-copy conveniences only and do not expose generic
zero-copy native views, mask/null propagation,
typed file handles, direct NumPy integration, or compressed storage-accounting
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
| `tutorial_09_tensor_ops_conversions` | Owned dense tensor ops, typed wrappers/`typed_ops`, optional Arrow RecordBatch/IPC plus ndarray conversions, and CSV/Parquet companion conversions |

```sh
cargo run --example tutorial_01_quickstart_create_append_read
cargo run --example tutorial_08_compression_interop
cargo run --features arrow,ndarray,csv,parquet --example tutorial_09_tensor_ops_conversions
```

Use the native-library environment below when running them. Default tutorial
examples build with empty default features; `tutorial_09_tensor_ops_conversions`
requires the opt-in `arrow,ndarray,csv,parquet` features. The examples create
tiny `.tio` files and, for the feature-gated tutorial, tiny Arrow IPC and Parquet
companion payloads under temporary directories and clean them up; CSV
string/bytes roundtrips stay in memory. Do not copy native libraries, Cargo
build output, generated `.tio` data, or generated IPC/CSV/Parquet data into the
tutorial tree or source-only public checkout.

## L2 Parquet OCB conversion example

`l2_parquet_to_ocb` is a bounded integration example for schema-compatible L2
Parquet days. It reads `L2ORDER.journal` + `L2TRADE.journal` into one normalized
order/trade event OCB and reads `L2MD.journal` into a separate market-data OCB:

```sh
cargo run --features format-ocb,parquet --example l2_parquet_to_ocb -- \
  --day-dir /path/to/l2_parquet/YYYYMMDD \
  --output-dir target/l2-parquet-ocb-example \
  --row-limit 10000 \
  --overwrite
```

The example intentionally uses only the public safe OCB wrapper. It materializes
rows before `ocb::create`, so keep `--row-limit` small for smoke tests; pass
`--all-rows` only for small input days or after budgeting memory in the calling
application.

`l2_ocb_load` is the read-side companion for applications that consume OCB files
directly. It opens the order/trade and market-data OCB files, projects the
normalized columns, applies row-group predicates, and copies returned batches
into application-owned structs:

```sh
cargo run --features format-ocb --example l2_ocb_load -- \
  --input-dir target/l2-parquet-ocb-example \
  --day-key YYYYMMDD \
  --max-rows 20
```

Use `--channel` for order/trade row-group pruning and `--symbol-code` for
market-data row-group pruning when loading large shards.

## Production integration checklist

Before shipping an application that uses this crate:

- Validate against the exact native `arcadia_tio_capi` library you intend to deploy.
- Set `ARCADIA_TIO_CAPI_LIB_DIR` for link discovery and configure runtime loader lookup separately.
- Run the workspace tests, public cargo-make matrix, and tutorial examples with that library. The public checkout includes `cargo make ci` (`fmt`, all-feature `check`, and default/no-default/optional/all-feature wrapper tests) plus a Cargo target runner that automatically mirrors `ARCADIA_TIO_CAPI_LIB_DIR` or `native/<target>/lib` into the platform runtime-loader path for common Linux/macOS `cargo run` and `cargo test` invocations.
- Keep generated `.tio` data and native/package artifacts out of this source-only checkout unless a separate release task approves them.
- Preserve the documented API boundaries: coordinate external summaries are not dereferenced, optional indexes are not authoritative truth, and examples are not benchmark, storage, compression, capacity, or release-readiness evidence.

## Local test/runtime library setup

Supply or copy the `arcadia_tio_capi` native shared library, then
point Cargo/linker discovery at the directory containing it:

```sh
LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
ARCADIA_TIO_CAPI_LIB_DIR="$LIB_DIR" \
LD_LIBRARY_PATH="$LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  cargo test -p arcadia-tio-rs --all-features
```

The exported public checkout provides matching cargo-make conveniences once a
local native library is available through `ARCADIA_TIO_CAPI_LIB_DIR` or the
ignored `native/<target>/lib` layout:

```sh
cargo make native-info
cargo make ci
cargo make test-matrix
cargo make test-no-default
cargo make test-arrow-ndarray
cargo make test-csv-parquet
cargo make test-ocb
cargo make test-all-features
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
