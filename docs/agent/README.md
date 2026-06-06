# Arcadia TIO Public Wrapper Agent Guide

This is the agent-readable entry point for the public `arcadia-tio-rs` checkout.
The README files remain human-facing user documentation; this page routes agents
to the smallest useful reading set for a change.

## Start here every time

1. Read `AGENTS.md`.
2. Read root `README.md` for public-checkout boundaries and native library setup.
3. Classify the task as safe-wrapper API, raw FFI/sys, examples/tutorials, build/linking, or docs-only.
4. Read the matching route below before editing.

## Routes by task type

### Safe Rust wrapper API

Read:

- `crates/arcadia-tio-rs/README.md`
- `crates/arcadia-tio-rs/src/lib.rs`
- matching tests under `crates/arcadia-tio-rs/tests/`
- matching tutorial example under `crates/arcadia-tio-rs/examples/tutorials/` when behavior is user-visible

Validate with the narrowest relevant test first, then:

```sh
cargo make test-default
cargo make test-all-features
```

### Raw FFI and link discovery

Read:

- `crates/arcadia-tio-sys/README.md`
- `crates/arcadia-tio-sys/src/lib.rs`
- `crates/arcadia-tio-sys/build.rs`
- `crates/arcadia-tio-sys/tests/`

Validate with:

```sh
cargo make native-info
cargo make test-all-features
```

### Optional feature integrations

For `arrow`, `ndarray`, `csv`, or `parquet` changes, read the optional-feature
sections in `crates/arcadia-tio-rs/README.md` and run the matching matrix tasks:

```sh
cargo make test-arrow-ndarray
cargo make test-csv-parquet
cargo make test-matrix
```

### Examples and tutorials

Read:

- `crates/arcadia-tio-rs/README.md` tutorial table
- `crates/arcadia-tio-rs/examples/tutorials/`
- `examples/tutorials/run/run_rust.sh`

Validate with:

```sh
bash examples/tutorials/run/run_rust.sh
```

### Build, native library, and CI plumbing

Read:

- `Makefile.toml`
- `.cargo/config.toml`
- `examples/tutorials/run/with_native_lib.sh`
- `crates/arcadia-tio-sys/build.rs`

Validate with:

```sh
cargo make native-info
cargo make ci
```

## Public boundary checklist

Before finishing any change, confirm:

- No private Rust crate dependency was added.
- No private implementation source or private evidence was copied into this checkout.
- Native libraries, generated `.tio` files, target output, package archives, and release bundles remain untracked unless explicitly approved.
- README/API caveats still match the exposed behavior.
