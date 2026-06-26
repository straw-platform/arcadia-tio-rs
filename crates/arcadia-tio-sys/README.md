# arcadia-tio-sys

Unsafe Rust FFI declarations for the Arcadia TIO C ABI.

This crate is intentionally low level: it exposes `repr(C)` types, constants,
and `unsafe extern "C"` functions for a compiled `arcadia_tio_capi` native
library. It does not depend on the private Rust implementation crates and does
not provide safe high-level TensorFile behavior. With the optional
`format-ocb` feature enabled, the crate exposes raw appendable OCB C ABI
constants, `repr(C)` metadata/read/write structs, opaque file handles,
init/free helpers, and open/create/append/read/dictionary/cleanup declarations;
callers must still uphold the C header ownership and lifetime contract.
Fixed-binary OCB columns reuse reserved ABI fields through the documented
`arcadia_tio_ocb_*fixed_binary_width` helpers; primitive `len` is row count,
while fixed-binary fill-buffer `values_len` is byte capacity. The linked native
library must export the matching `arcadia_tio_ocb_*` symbols when
`format-ocb` is enabled; missing-symbol link errors mean the native library is
older than the OCB C ABI surface.

## Link discovery

`build.rs` links the native library once through Cargo/linker directives; it
does not use per-call runtime symbol lookup. Discovery order is:

1. `ARCADIA_TIO_CAPI_LIB_DIR=/absolute/path/to/lib` (or compatibility alias
   `ARCADIA_TIO_NATIVE_LIB_DIR`) plus optional `ARCADIA_TIO_CAPI_INCLUDE_DIR`.
2. Vendored `native/<target>/lib` and optional `native/<target>/include` inside
   this crate.
3. Explicit opt-in system fallback with `ARCADIA_TIO_CAPI_SYSTEM_FALLBACK=1`.

Set `ARCADIA_TIO_CAPI_LINK_KIND=dylib|static` to choose link kind; `dylib` is
the default. Dynamic linking still requires the platform loader to find the
shared library at runtime (`LD_LIBRARY_PATH`, `DYLD_LIBRARY_PATH`, Windows
`PATH`, rpath/install-name, or library colocation as appropriate).

## Local tests

Supply or copy the native C ABI library, then set `LIB_DIR` to the directory containing it:

```sh
LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
```

Then run the sys crate tests with the native library directory selected
explicitly. From the repository root on Linux:

```sh
LIB_DIR="$PWD/native/x86_64-unknown-linux-gnu/lib"
ARCADIA_TIO_CAPI_LIB_DIR="$LIB_DIR" \
LD_LIBRARY_PATH="$LIB_DIR${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}" \
  cargo test -p arcadia-tio-sys
```

On macOS, use `DYLD_LIBRARY_PATH` instead of `LD_LIBRARY_PATH`. On Windows, add
the directory containing `arcadia_tio_capi.dll` to `PATH` and set
`ARCADIA_TIO_CAPI_LIB_DIR` to the import-library/native-library directory.

`ARCADIA_TIO_NATIVE_LIB_DIR` is accepted as a compatibility alias for early task
examples, but new users should prefer `ARCADIA_TIO_CAPI_LIB_DIR`. Use
`ARCADIA_TIO_CAPI_INCLUDE_DIR` when a consumer also needs the C headers from a
prebuilt bundle.
