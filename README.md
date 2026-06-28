# Arcadia TIO Rust wrappers

This public checkout contains source-visible Rust crates for Arcadia TIO: the
C-ABI-free OCB core reader crate plus C-ABI-backed Rust wrapper crates. Broader
private core implementation source is not part of this repository, and these
crates must not depend on private Rust crates such as `arcadia-tio` or
`arcadia-tio-capi` through Cargo.

## Layout

- `crates/arcadia-tio-ocb-core/` — source-visible Rust-core OCB reader and bounded visitor APIs with no native C ABI dependency.
- `crates/arcadia-tio-sys/` — unsafe C ABI declarations and link discovery.
- `crates/arcadia-tio-rs/` — safe Rust wrapper over `arcadia-tio-sys`.
- `native/x86_64-unknown-linux-gnu/lib/` — optional local-only native library copy for C-ABI wrapper tests.
- `examples/tutorials/run/run_rust.sh` — source-only tutorial runner for local validation.

The `arcadia-tio-ocb-core` crate covers the clean Rust-core OCB reader boundary:
selected-snapshot open, metadata/dictionary/row-group summaries, read planning,
projected/predicate reads, explicit plan-local row-group visitors,
reusable-buffer lower-copy visitors, generic fixed-binary record field projection
helpers and projected visitors, read-plan certification summaries/fingerprints,
fixed-payload projection attribution, callback-wall attribution, observed
max-in-flight reporting, and stable duplicate/unknown row-group subset error
constants. It does not depend on
`arcadia-tio-sys`, `arcadia-tio-capi`, a native library, or native-link build
scripts.

## 0.2.0 release boundary

The 0.2.0 public Rust workspace tag is a source release for the generic OCB
Rust-core reader boundary plus the existing C-ABI-backed wrapper source. It does
not publish crates.io packages, native libraries, signed artifacts, benchmark
evidence, storage/capacity/performance claims, or production/default runtime
readiness.

For OCB-core runtime gates, `ocb.generic.crc32c.v1` fingerprints are
deterministic compatibility identifiers, not cryptographic digests. Downstream
payload-only use should be manifest-gated and fail closed by comparing the
snapshot `combined` fingerprint, root/previous-root generation identifiers,
selected row-group ids/base rows/counts, selected chunk summaries/checksums,
selected compressed/uncompressed byte totals, the plan report, and
`selected_chunk_fingerprint`. Full-file artifact digests remain offline/operator
recertification evidence rather than normal startup validation.

For coalesced downstream scans, build one `ReadPlan`, union the needed
plan-local row-group ids, execute `read_plan_row_groups(...)` or
`visit_plan_row_groups_into_with_attribution(...)` once, and demultiplex in the
application. OCB keeps these APIs generic; channel, BizIndex, fixed-ingress,
compact-L2, replay, and order-book semantics remain downstream.

The C-ABI-backed safe wrapper covers the agreed source-visible public Rust beta scope:
create/open metadata, policy and inferred create helpers, inline numeric
coordinate metadata/lookup/read conveniences, bounded source-visible Coordinate
v2 create/metadata/value/dictionary/lookup/append wrappers for implemented
private-storage domains, bulk f32/f64/i32/i64 append/read helpers, owned
in-memory tensor operations and typed tensor wrappers, optional owned Arrow
RecordBatch/IPC, Rust ndarray, and CSV/Parquet companion conversion features,
bounded exact-integer sparse-intent analysis and append helpers, appendable OCB
(Ordered Column Bundle) create/append/open/metadata/dictionary/read/cleanup
wrappers behind the non-default `format-ocb` feature, universe-aware authoring,
current and historical read options and shape policies, write-forward
compression controls, scoped f32/f64 rewrite/clear-block mutation helpers,
scoped reform/compaction workflows, and V4 diagnostics/precise-accounting
reports. External value resolution, arbitrary
dereference, zero-copy native views, coordinate-index acceleration,
generic/private native maintainer hooks, direct NumPy/Python integration, native
artifact publication, release actions, and performance/storage/capacity claims
are outside this source export.

## Production integration checklist

Before using the C-ABI-backed public Rust wrapper in an application build:

1. Build or obtain the operator-approved `arcadia_tio_capi` native library for the target platform.
2. Set `ARCADIA_TIO_CAPI_LIB_DIR` for link discovery and configure the platform runtime loader separately (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, rpath/install-name, `PATH`, or DLL colocation as appropriate).
3. Run `cargo make ci` (format, all-feature check, OCB feature smoke, and the default/no-default/optional/all-feature test matrix) plus `bash examples/tutorials/run/run_rust.sh` against that native library. A committed Cargo target runner automatically adds `ARCADIA_TIO_CAPI_LIB_DIR` or `native/x86_64-unknown-linux-gnu/lib` to the runtime loader path for common Linux/macOS `cargo run` and `cargo test` invocations.
4. Keep generated `.tio` files, native libraries, package archives, and local `native/` copies out of source control unless a separate release task approves them.
5. Treat Coordinate external references as metadata/status summaries only; this wrapper does not add dereference, variable-length string, broad calendar/session, lookup-acceleration, or release/performance claims.
6. Treat OCB as one appendable Ordered Column Bundle format; these crates do not expose public binary revision names, market-data/domain-specific APIs, or performance/storage/capacity/layout claims.

For the Rust-core OCB reader-only path, depend on `arcadia-tio-ocb-core`; no native C ABI library, `ARCADIA_TIO_CAPI_LIB_DIR`, or runtime loader configuration is required for that crate.

## Using from another Rust project

For the C-ABI-free Rust-core OCB reader/visitor path, depend directly on the
core reader crate:

```toml
[dependencies]
arcadia-tio-ocb-core = { path = "arcadia-tio-rs/crates/arcadia-tio-ocb-core" }
```

Or use the 0.2.0 public source tag:

```toml
[dependencies]
arcadia-tio-ocb-core = { git = "https://github.com/Jacobbishopxy/arcadia-tio-rs.git", tag = "0.2.0", package = "arcadia-tio-ocb-core" }
```

For the C-ABI-backed safe wrapper, add the wrapper as a path dependency when working from a local checkout:

```toml
[dependencies]
arcadia-tio-rs = { path = "arcadia-tio-rs/crates/arcadia-tio-rs" }
```

Or use the 0.2.0 public source tag:

```toml
[dependencies]
arcadia-tio-rs = { git = "https://github.com/Jacobbishopxy/arcadia-tio-rs.git", tag = "0.2.0" }
```

Default wrapper features are empty. Enable optional public Rust conversion dependencies
only when needed:

```toml
[dependencies]
arcadia-tio-rs = { path = "arcadia-tio-rs/crates/arcadia-tio-rs", features = ["arrow", "ndarray", "csv", "parquet", "format-ocb"] }
```

`arrow` provides owned `Tensor` conversions to/from Arrow `RecordBatch` and IPC
bytes; it is separate from native Arrow C Data `read_values_arrow()` ownership.
`ndarray` provides owned `ndarray::ArrayD<T>` conversions for dense
f32/f64/i32/i64 tensor payloads and does not add NumPy, PyO3, or Python
bindings. `csv` and `parquet` provide companion owned `Tensor` conversions with
explicit dtype/shape/order metadata; they are not native `.tio` storage formats
or file-to-file native conversion shortcuts. `format-ocb` exposes
`arcadia_tio_rs::ocb` safe wrappers and matching raw sys declarations for OCB
create/append/open/metadata/dictionary/summary/read/cleanup. OCB read requests
can be built from generic ordering-key ranges for row-group pruning. OCB read
results own copied Rust values, dictionary-coded columns return primitive codes,
and decoded dictionaries are explicit via `dictionary_values`. OCB fixed-binary
payload columns are available as `PhysicalType::FixedBinary { width }` and
`PrimitiveValues::FixedBinary { width, bytes }`, with `bytes.len() == rows *
width`; predicates and ordering remain scalar-column responsibilities in this
first slice. See `cargo run --no-default-features --features format-ocb
--example ocb_fixed_binary` for a generic opaque-byte roundtrip. Enabling
`format-ocb` requires an OCB-capable native library with the
`arcadia_tio_ocb_*` symbols exported; missing-symbol link errors mean the native
library predates this OCB C ABI.

The Rust wrapper links to the native C ABI library, so the consuming build must
also provide `libarcadia_tio_capi`/`arcadia_tio_capi`. Point link discovery and
the platform runtime loader at the native library directory, for example:

```sh
export ARCADIA_TIO_CAPI_LIB_DIR="/path/to/arcadia-tio-rs/native/x86_64-unknown-linux-gnu/lib"
export LD_LIBRARY_PATH="$ARCADIA_TIO_CAPI_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
```

Use `DYLD_LIBRARY_PATH` on macOS, or add the DLL directory to `PATH` on Windows,
unless the application configures rpath/install-name or colocates the native
library by another deployment mechanism.

Minimal usage:

```rust
use arcadia_tio_rs::{AxisKind, CreateOptions, DType, DimSpec, TensorFile};

fn main() -> arcadia_tio_rs::Result<()> {
    let options = CreateOptions::streaming(
        DType::F32,
        vec![
            DimSpec::new(AxisKind::Time, 0).with_name("time"),
            DimSpec::new(AxisKind::Channel, 2).with_name("channel"),
        ],
        0,
    );

    let mut file = TensorFile::create("example.tio", options)?;
    file.append_f32(&[1.0, 2.0], &[1, 2])?;
    Ok(())
}
```

## Local test flow

The C-ABI-free core reader can be checked without native libraries:

```sh
cargo make test-core-reader
cargo make test-core-reader-tree
cargo make test-core-reader-no-cabi
cargo run -p arcadia-tio-ocb-core --example project_fixed_binary -- <file.ocb> <fixed-binary-column> <record-width>
```

For C-ABI-backed wrapper tests, supply a locally built native C ABI library,
either by setting `ARCADIA_TIO_CAPI_LIB_DIR` explicitly or by copying it into the
ignored `native/x86_64-unknown-linux-gnu/lib/` directory. When using the local native layout on
Linux:

```sh
export ARCADIA_TIO_CAPI_LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
export LD_LIBRARY_PATH="$ARCADIA_TIO_CAPI_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo make native-info
cargo make test-core-reader
cargo make ci
cargo make test-matrix
cargo make test-ocb
bash examples/tutorials/run/run_rust.sh
cargo run -p arcadia-tio-rs --example tutorial_01_quickstart_create_append_read
cargo run -p arcadia-tio-rs --features arrow,ndarray,csv,parquet --example tutorial_09_tensor_ops_conversions
cargo test -p arcadia-tio-rs --features format-ocb --test ocb
cargo run -p arcadia-tio-rs --features format-ocb --example ocb_roundtrip
cargo check -p arcadia-tio-rs --features format-ocb,parquet --example l2_parquet_to_ocb
cargo check -p arcadia-tio-rs --features format-ocb --example l2_ocb_load
cargo make test-csv-parquet
```

`cargo make native-info` validates that an expected local native library file is
present and prints its resolved path, size, SHA-256 checksum, and whether the
common OCB C ABI symbols are visible when `nm` is available. Set
`ARCADIA_TIO_REQUIRE_OCB_SYMBOLS=1` with `cargo make native-info` when you want
to fail fast on a stale native library before running `format-ocb` builds.

The public cargo-make matrix runs `test-default`, explicit `test-no-default`,
`test-arrow-ndarray`, `test-csv-parquet`, explicit `test-ocb`, and
`test-all-features`; OCB can also be exercised directly with
`--features format-ocb`; `ci` runs
`fmt`, C-ABI-free `test-core-reader`, the no-C-ABI dependency guard,
all-feature `check`, and that matrix. The feature-gated tensor
ops/conversions tutorial uses owned tensor ops, typed wrappers, owned Arrow
RecordBatch/IPC, ndarray, and CSV/Parquet companion conversions with tiny
deterministic data. These examples are not performance, storage, zero-copy,
native `.tio` file-conversion, or NumPy/Python integration evidence.

### L2 Parquet OCB conversion example

`l2_parquet_to_ocb` is a bounded L2 order/trade and market-data Parquet conversion example.
It reads `L2ORDER.journal` + `L2TRADE.journal` into one normalized order/trade
OCB and `L2MD.journal` into a separate market-data OCB:

```sh
cargo run -p arcadia-tio-rs --features format-ocb,parquet --example l2_parquet_to_ocb -- \
  --day-dir /path/to/l2_parquet/YYYYMMDD \
  --output-dir target/l2-parquet-ocb-example \
  --row-limit 10000 \
  --overwrite
```

The example uses only the public safe OCB wrapper and materializes rows before
`ocb::create`; keep it bounded for smoke tests unless the caller has budgeted
memory.

`l2_ocb_load` is the read-side companion for applications that consume OCB files
directly. It opens the order/trade and market-data OCB files, projects the
normalized columns, applies row-group predicates, and copies returned batches
into application-owned structs:

```sh
cargo run -p arcadia-tio-rs --features format-ocb --example l2_ocb_load -- \
  --input-dir target/l2-parquet-ocb-example \
  --day-key YYYYMMDD \
  --max-rows 20
```

Use `--channel` for order/trade row-group pruning and `--symbol-code` for
market-data row-group pruning when loading large shards.

The native library path is local-only. The committed Cargo target runner mirrors
`ARCADIA_TIO_CAPI_LIB_DIR` or `native/x86_64-unknown-linux-gnu/lib` into the runtime loader path
for common Linux/macOS `cargo run` and `cargo test` invocations; if your target
or deployment launcher bypasses Cargo's runner, set `LD_LIBRARY_PATH`,
`DYLD_LIBRARY_PATH`, `PATH`, rpath/install-name, or DLL colocation yourself.
Do not commit, push, publish, upload, sign, tag, or release native libraries,
package archives, or generated bundles from this checkout without a later
explicit release task approving that scope.
