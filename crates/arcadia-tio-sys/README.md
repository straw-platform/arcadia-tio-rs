# arcadia-tio-sys

Unsafe Rust FFI declarations for the Arcadia TIO C ABI.

This crate is intentionally low level: it exposes `repr(C)` types, constants,
and `unsafe extern "C"` functions for a compiled `arcadia_tio_capi` native
library. It does not depend on the private Rust implementation crates and does
not provide safe high-level TensorFile behavior. With the optional
`format-ocb` feature enabled, the crate exposes raw appendable OCB C ABI
constants, `repr(C)` metadata/read/write structs, opaque file handles,
init/free helpers, manifest build/validate carriers, compact-L2 physical-v2
artifact certification carriers, and
open/create/append/read/dictionary/cleanup/manifest/certification declarations.
`ARCADIA_TIO_ABI_VERSION` is the authoritative base ABI expected by this sys
release and is exactly `3`. This unsafe crate declares symbols but does not
perform a runtime compatibility check; safe callers should use the gate in
`arcadia-tio-rs` before calling any symbol other than
`arcadia_tio_abi_version()`.
`ARCADIA_TIO_COMPACTION_ABI_VERSION` independently describes the additive
pointer-based compaction family and is exactly `1`. That family consists of
`arcadia_tio_compaction_abi_version`, `arcadia_tio_compact_to_ex`, and
`arcadia_tio_maybe_compact_ex`; the `_ex` functions borrow a non-null
`ArcadiaTioCompactionMode` pointer. The original by-value functions remain
declared for source and binary compatibility. A runtime loader must resolve the
family-version symbol optionally before assuming either `_ex` symbol exists;
this statically linked sys crate still requires every symbol that a consumer
references to exist in the selected native library.
The OCB declarations include the versioned `ArcadiaTioOcbResourceLimits`
carrier, its Policy A initializer, and the combined validation/resource-limit
open entry point. Reserved fields must remain zero, and reader clones retain
the selected policy. Its two aggregate owned-read fields also independently
bound unique dictionary/key-tuple bytes across the whole open and logical
metadata allocations for one root-candidate validation.
The 0.3.5 additions include the opaque bounded parallel read
session, initialized options/result/report carriers, blocking caller-thread
polling, idempotent cancellation, terminal reporting, and paired result/report/
session frees. The raw result owns its nested batch until the result free call;
reusing an owning output is invalid.
Generic write report/timing carriers are scalar initialized outputs and do not
require a free helper; callers must still uphold the C header ownership and
lifetime contract.
The raw sys surface
also includes the copy-only `arcadia_tio_tensor_*` structural tensor operations
and the float-only elementwise tensor operations over borrowed
`ArcadiaTioTensor` inputs and native-owned tensor outputs freed with
`arcadia_tio_tensor_free`.
Fixed-binary OCB columns reuse reserved ABI fields through the documented
`arcadia_tio_ocb_*fixed_binary_width` helpers; primitive `len` is row count,
while fixed-binary fill-buffer `values_len` is byte capacity. The linked native
library must export the matching `arcadia_tio_ocb_*` symbols when
`format-ocb` is enabled; missing-symbol link errors mean the native library is
older than the OCB C ABI surface. The same link-time rule applies to other
statically declared newer symbol families: Cargo features control which
declarations are referenced, but they do not weaken the mandatory base-ABI
check.

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
