# arcadia-tio-rs

Safe Rust wrapper over the compiled `arcadia_tio_capi` native C ABI library.

This crate is source-visible wrapper code only. It depends on
`arcadia-tio-sys` for raw FFI declarations and link discovery; it does not
depend on the private `arcadia-tio` Rust implementation crate in its normal
consumer build path.

The API slice is intentionally bounded but includes the beta read-shape and
layout-create surface: safe lifecycle ownership, owned error strings,
create/open metadata types, policy/inferred create helpers, write-forward
compression selection, bulk tensor I/O helpers, universe-aware create/append
authoring, current read options and shape policies, historical `read_at_commit`
options and shape policies, and dense mask materialization. Append helpers
borrow Rust slices only for the duration of one bulk FFI call, validate
dtype/rank/shape/data length before crossing the ABI, and return the assigned
append-entry range. Read helpers copy native-owned tensor/mask/report outputs
into Rust-owned values and immediately free the C allocation; this slice does
not expose zero-copy borrowed views over native buffers.

Native coordinate lookup helpers are deferred until the C ABI exposes a clear
lookup ownership/error contract.

## Example

```rust,no_run
use arcadia_tio_rs::{
    AxisKind, CompressionConfig, CreateOptions, DType, DimSpec, TensorData, TensorFile,
};

# fn main() -> arcadia_tio_rs::Result<()> {
let path = std::env::temp_dir().join("example.tio");
let mut options = CreateOptions::streaming(
    DType::F64,
    vec![DimSpec::new(AxisKind::Time, 0)],
    0,
);
// Defaults stay uncompressed. Set this only when future appends should write zstd refs.
options.compression = Some(CompressionConfig::zstd_level(3));

{
    let mut file = TensorFile::create(&path, options)?;
    file.append_f64(&[1.0, 2.0, 3.0], &[3])?;
}

let file = TensorFile::open(&path)?;
let tensor = file.read_all()?;
assert_eq!(tensor.shape, vec![3]);
assert_eq!(tensor.data, TensorData::F64(vec![1.0, 2.0, 3.0]));
# Ok(())
# }
```

## Parity caveats

This is not broad parity with the private Rust crate. It currently covers bulk
create/open/append/read, RegularChunked policy create, inferred create,
universe-aware authoring, current and historical read options, current and
historical read-shape policies, write-forward uncompressed/zstd compression
controls, and metadata helpers. It does not expose rewrite, reform, compaction,
retained-history compaction workflows, query-attribution, zero-copy native views,
native exact/range coordinate lookup helpers, or compressed storage-accounting
eligibility claims.

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
