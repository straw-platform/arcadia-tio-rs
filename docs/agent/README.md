# Arcadia TIO Public Wrapper Agent Guide

This is the agent-readable entry point for the public `arcadia-tio-rs` checkout.
The README files remain human-facing user documentation; this page routes agents
to the smallest useful reading set for a change.

## Start here every time

1. Read `AGENTS.md`.
2. Read root `README.md` for public-checkout boundaries and native library setup.
3. Classify the task as OCB core reader, safe-wrapper API, raw FFI/sys, examples/tutorials, build/linking, or docs-only.
4. Read the matching route below before editing.

## Routes by task type

### C-ABI-free OCB core reader

Read:

- `crates/arcadia-tio-ocb-core/README.md`
- `crates/arcadia-tio-ocb-core/src/lib.rs`
- `crates/arcadia-tio-ocb-core/src/column_bundle.rs`
- the matching owner under `src/column_bundle/{planning,execution,materialize,attribution}.rs`
  and `src/{format,read,parallel_prepare,parallel_read}.rs`

Validate without native-library setup:

```sh
cargo make test-core-reader
cargo make test-core-reader-tree
```

The dependency tree for this crate must not include `arcadia-tio-sys`,
`arcadia-tio-capi`, native-link build scripts, or native runtime libraries.

### Safe Rust wrapper API

Read:

- `crates/arcadia-tio-rs/README.md`
- `crates/arcadia-tio-rs/src/lib.rs` (stable re-export facade)
- the matching owner under `src/{error,ownership,tensor,ops,file_types,coordinates,tensor_file,conversion}.rs`
  or `src/ocb/{model,read,session,write,maintenance,conversion}.rs`
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
- `crates/arcadia-tio-sys/src/lib.rs` (stable re-export facade)
- the matching ABI family under
  `src/{common,types,tensor,lifecycle_coordinates,read,mutation,maintenance,ocb,metadata,history}.rs`
- `crates/arcadia-tio-sys/build.rs`
- `crates/arcadia-tio-sys/tests/`

Validate with:

```sh
cargo make native-info
cargo make test-sys-public-constants
cargo make test-all-features
```

Private C header/sys parity is not part of the public check. Run
`cargo make test-private-c-header-parity` only with an explicitly supplied
checked `ARCADIA_TIO_CAPI_INCLUDE_DIR` and configured native library; absence is
a refusal, never a pass. Supply the absolute path in the host shell's native
form (`/...` on Unix or `C:/...` on Windows); the Rust gate checks it again.

### Structure, routing, and docs-only changes

Read:

- this routing map and root `AGENTS.md`
- `scripts/check_module_size_budgets.py`
- the facade and focused owner being documented

Validate with:

```sh
cargo make module-size-check
cargo fmt --all -- --check
```

The budgets are exact post-split ratchets. Split within an existing ownership
family when one is reached; do not raise a limit as routine growth allowance.

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
- `arcadia-tio-ocb-core` remains C-ABI-free and does not depend on `arcadia-tio-sys` or `arcadia-tio-capi`.
- No private implementation source outside the approved OCB core reader allowlist, and no private evidence, was copied into this checkout.
- Native libraries, generated `.tio` files, target output, package archives, and release bundles remain untracked unless explicitly approved.
- README/API caveats still match the exposed behavior.
