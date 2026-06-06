# Arcadia TIO Rust wrappers

This public checkout contains the source-visible Rust wrapper crates for the
Arcadia TIO C ABI binary library. The private core implementation source is not
part of this repository, and these crates must not depend on private Rust crates
such as `arcadia-tio` or `arcadia-tio-capi` through Cargo.

## Layout

- `crates/arcadia-tio-sys/` — unsafe C ABI declarations and link discovery.
- `crates/arcadia-tio-rs/` — safe Rust wrapper over `arcadia-tio-sys`.
- `native/x86_64-unknown-linux-gnu/lib/` — optional local-only native library copy for tests.
- `examples/tutorials/run/run_rust.sh` — source-only tutorial runner for local validation.

The safe wrapper covers the agreed source-visible public Rust beta scope:
create/open metadata, policy and inferred create helpers, inline numeric
coordinate metadata/lookup/read conveniences, bounded source-visible Coordinate
v2 create/metadata/value/dictionary/lookup/append wrappers for implemented
private-storage domains, bulk f32/f64/i32/i64 append/read helpers, owned
in-memory tensor operations and typed tensor wrappers, optional owned Arrow
RecordBatch/IPC, Rust ndarray, and CSV/Parquet companion conversion features,
bounded exact-integer
sparse-intent analysis and append helpers, universe-aware authoring, current and
historical read options and shape policies, write-forward compression controls,
scoped f32/f64
rewrite/clear-block mutation helpers, scoped reform/compaction workflows, and V4
diagnostics/precise-accounting reports. External value resolution, arbitrary
dereference, zero-copy native views, coordinate-index acceleration,
generic/private native maintainer hooks, direct NumPy/Python integration, native
artifact publication, release actions, and performance/storage/capacity claims
are outside this source export.

## Production integration checklist

Before using the public Rust wrapper in an application build:

1. Build or obtain the operator-approved `arcadia_tio_capi` native library for the target platform.
2. Set `ARCADIA_TIO_CAPI_LIB_DIR` for link discovery and configure the platform runtime loader separately (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, rpath/install-name, `PATH`, or DLL colocation as appropriate).
3. Run `cargo make ci` (format, all-feature check, and the default/no-default/optional/all-feature test matrix) plus `bash examples/tutorials/run/run_rust.sh` against that native library. A committed Cargo target runner automatically adds `ARCADIA_TIO_CAPI_LIB_DIR` or `native/x86_64-unknown-linux-gnu/lib` to the runtime loader path for common Linux/macOS `cargo run` and `cargo test` invocations.
4. Keep generated `.tio` files, native libraries, package archives, and local `native/` copies out of source control unless a separate release task approves them.
5. Treat Coordinate external references as metadata/status summaries only; this wrapper does not add dereference, variable-length string, broad calendar/session, lookup-acceleration, or release/performance claims.

## Using from another Rust project

Add the safe wrapper as a path dependency when working from a local checkout:

```toml
[dependencies]
arcadia-tio-rs = { path = "arcadia-tio-rs/crates/arcadia-tio-rs" }
```

Or use a git dependency once the desired commit is pushed:

```toml
[dependencies]
arcadia-tio-rs = { git = "https://github.com/Jacobbishopxy/arcadia-tio-rs.git" }
```

Default features are empty. Enable optional public Rust conversion dependencies
only when needed:

```toml
[dependencies]
arcadia-tio-rs = { path = "arcadia-tio-rs/crates/arcadia-tio-rs", features = ["arrow", "ndarray", "csv", "parquet"] }
```

`arrow` provides owned `Tensor` conversions to/from Arrow `RecordBatch` and IPC
bytes; it is separate from native Arrow C Data `read_values_arrow()` ownership.
`ndarray` provides owned `ndarray::ArrayD<T>` conversions for dense
f32/f64/i32/i64 tensor payloads and does not add NumPy, PyO3, or Python
bindings. `csv` and `parquet` provide companion owned `Tensor` conversions with
explicit dtype/shape/order metadata; they are not native `.tio` storage formats
or file-to-file native conversion shortcuts.

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

Supply a locally built native C ABI library, either by setting
`ARCADIA_TIO_CAPI_LIB_DIR` explicitly or by copying it into the ignored
`native/x86_64-unknown-linux-gnu/lib/` directory. When using the local native layout on Linux:

```sh
export ARCADIA_TIO_CAPI_LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
export LD_LIBRARY_PATH="$ARCADIA_TIO_CAPI_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo make native-info
cargo make ci
cargo make test-matrix
bash examples/tutorials/run/run_rust.sh
cargo run -p arcadia-tio-rs --example tutorial_01_quickstart_create_append_read
cargo run -p arcadia-tio-rs --features arrow,ndarray,csv,parquet --example tutorial_09_tensor_ops_conversions
cargo make test-csv-parquet
```

`cargo make native-info` validates that an expected local native library file is
present and prints its resolved path, size, and SHA-256 checksum for local
freshness checks.

The public cargo-make matrix runs `test-default`, explicit `test-no-default`,
`test-arrow-ndarray`, `test-csv-parquet`, and `test-all-features`; `ci` runs
`fmt`, all-feature `check`, and that matrix. The feature-gated tensor
ops/conversions tutorial uses owned tensor ops, typed wrappers, owned Arrow
RecordBatch/IPC, ndarray, and CSV/Parquet companion conversions with tiny
deterministic data. These examples are not performance, storage, zero-copy,
native `.tio` file-conversion, or NumPy/Python integration evidence.

The native library path is local-only. The committed Cargo target runner mirrors
`ARCADIA_TIO_CAPI_LIB_DIR` or `native/x86_64-unknown-linux-gnu/lib` into the runtime loader path
for common Linux/macOS `cargo run` and `cargo test` invocations; if your target
or deployment launcher bypasses Cargo's runner, set `LD_LIBRARY_PATH`,
`DYLD_LIBRARY_PATH`, `PATH`, rpath/install-name, or DLL colocation yourself.
Do not commit, push, publish, upload, sign, tag, or release native libraries,
package archives, or generated bundles from this checkout without a later
explicit release task approving that scope.
