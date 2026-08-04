# Public Wrapper Repository Guidelines

This checkout is the source-visible public Rust workspace for Arcadia TIO. It
contains the C-ABI-free OCB core reader crate and C-ABI-backed Rust wrapper
crates. It is not the broader private core implementation repository.

## Project Structure

- `crates/arcadia-tio-ocb-core/`: C-ABI-free Rust-core OCB reader, planning, bounded visitor, and attribution APIs.
- `crates/arcadia-tio-sys/`: unsafe C ABI declarations, constants, link discovery, and raw ownership boundaries.
- `crates/arcadia-tio-rs/`: safe Rust wrapper over `arcadia-tio-sys`.
- `crates/arcadia-tio-rs/src/`: thin crate facade plus error/ownership, tensor,
  operations, coordinates, TensorFile, conversion, and nested `ocb/*` families.
- `crates/arcadia-tio-sys/src/`: thin raw facade plus ABI-family declarations
  for common types, lifecycle/coordinates, reads, mutation, maintenance, OCB,
  metadata, and history.
- `crates/arcadia-tio-rs/examples/tutorials/`: Cargo tutorial example targets.
- `examples/tutorials/run/`: shell runners for source-only tutorial validation.
- `native/<target>/`: local-only native library/include layout for tests. Keep this ignored unless a release task explicitly approves otherwise.

## Agent Reading Map

- Start with this `AGENTS.md` for repository rules.
- Use `docs/agent/README.md` for progressive-disclosure routing.
- Read root `README.md` for public checkout boundaries and local test flow.
- Read `crates/arcadia-tio-ocb-core/README.md` before changing the C-ABI-free OCB reader path.
- Read `crates/arcadia-tio-rs/README.md` for the safe wrapper API scope.
- Read `crates/arcadia-tio-sys/README.md` before changing raw FFI or link discovery.

## Public Boundary Rules

- Do not add Cargo dependencies on private crates such as `arcadia-tio` or `arcadia-tio-capi`.
- Keep `arcadia-tio-ocb-core` C-ABI-free: it must not depend on `arcadia-tio-sys`, `arcadia-tio-capi`, native-link build scripts, or native runtime libraries.
- Do not copy private implementation source beyond the approved `arcadia-tio-ocb-core` OCB reader allowlist, and do not copy private maintainer hooks or private evidence into this checkout.
- Treat native libraries, generated `.tio` files, package archives, and release bundles as local artifacts unless an explicit release task approves them.
- Preserve documented caveats: examples and wrapper APIs are not benchmark, storage-efficiency, capacity, zero-copy, release-readiness, or production-performance evidence.

## Build and Test Commands

The C-ABI-free OCB core reader can be tested without native-library setup:

```sh
cargo make test-core-reader
cargo make test-core-reader-tree
cargo make module-size-check
```

Native-library setup is required for wrapper build/test commands. Prefer the
repo's cargo-make tasks after `ARCADIA_TIO_CAPI_LIB_DIR` or
`native/<target>/lib` is configured.

```sh
cargo make native-info
cargo make ci
cargo make test-matrix
bash examples/tutorials/run/run_rust.sh
```

Useful narrower checks:

```sh
cargo fmt --all -- --check
cargo make test-core-reader
cargo make test-default
cargo make test-no-default
cargo make test-arrow-ndarray
cargo make test-csv-parquet
cargo make test-all-features
cargo make module-size-check
```

## Coding Style

- Rust follows `rustfmt` defaults.
- Prefer small, reviewable changes with explicit ownership/error-boundary notes for FFI changes.
- Keep public API changes documented in the relevant crate README and examples when applicable.
