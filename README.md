# Arcadia TIO Rust wrappers

This public checkout contains the source-visible Rust wrapper crates for the
Arcadia TIO C ABI binary library. The private core implementation source is not
part of this repository, and these crates must not depend on private Rust crates
such as `arcadia-tio` or `arcadia-tio-capi` through Cargo.

## Layout

- `crates/arcadia-tio-sys/` — unsafe C ABI declarations and link discovery.
- `crates/arcadia-tio-rs/` — safe Rust wrapper over `arcadia-tio-sys`.
- `native/x86_64-unknown-linux-gnu/lib/` — optional local-only native library copy for tests.

The safe wrapper covers the agreed source-visible public Rust beta scope:
create/open metadata, policy and inferred create helpers, inline numeric
coordinate metadata/lookup/read conveniences, bulk f32/f64/i32/i64 append/read
helpers, bounded exact-integer sparse-intent analysis and append helpers,
universe-aware authoring, current and historical read options and shape policies,
write-forward compression controls, scoped f32/f64 rewrite/clear-block mutation
helpers, scoped reform/compaction workflows, and V4 diagnostics/precise-accounting
reports. Query attribution, zero-copy native views, coordinate-index acceleration,
generic/private native maintainer hooks, native artifact publication, and
performance/storage/capacity claims are outside this source export.

## Local test flow

Supply a locally built native C ABI library, either by setting
`ARCADIA_TIO_CAPI_LIB_DIR` explicitly or by copying it into the ignored
`native/x86_64-unknown-linux-gnu/lib/` directory. When using the local native layout on Linux:

```sh
export ARCADIA_TIO_CAPI_LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
export LD_LIBRARY_PATH="$ARCADIA_TIO_CAPI_LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo test --workspace
cargo run -p arcadia-tio-rs --example roundtrip
```

The native library path is local-only. Do not commit, push, publish, upload,
sign, tag, or release native libraries, package archives, or generated bundles
from this checkout without a later explicit release task approving that scope.
