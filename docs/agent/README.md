# TIO Rust wrapper focused reading map

Local API map only. Start with [Ops](../../../arcadia-ops/AGENTS.md) and the
[wrapper business contract](../../BUSINESS_CONTRACT.md), once per context.
Read only the affected route. Shared execution policy is not duplicated here.

| Change | Read next | Focused validation surface |
| --- | --- | --- |
| C-ABI-free OCB reader | [Core README](../../crates/arcadia-tio-ocb-core/README.md), lib/column_bundle facade and affected owner | Core reader behavior and no-native dependency tree |
| Safe wrapper | [Wrapper README](../../crates/arcadia-tio-rs/README.md), affected ownership/tensor/coordinates/file/ocb family | Relevant API, error and ownership tests |
| Raw FFI/link discovery | [Sys README](../../crates/arcadia-tio-sys/README.md), affected ABI family, build.rs | Public constants/linking and affected native smoke |
| Optional Arrow/ndarray/CSV/Parquet | Matching feature documentation and conversion owner | Only affected feature combinations; release matrix separately |
| Tutorial/examples | Wrapper tutorial table and selected example/runner | Changed example against its required native interface |
| Structure | scripts/check_module_size_budgets.py and affected stable facade/owner | Module-size ratchets; no blanket budget increase |
| Build/CI | Makefile.toml, .cargo/config.toml, native runner/build.rs | Changed source-mode, native-link and feature assumptions |
| Docs only | Affected README/contract and source declaration | Links, consistency and whitespace; no automatic compile/matrix |

Reader-only gates require no native library. Safe/raw wrapper gates require
explicit compatible native-library and loader setup. Private C-header parity is
not an ordinary public check: it requires a supplied checked absolute include
root and native library. Missing inputs are refused/unverified, never a pass.

Use external targets and output roots under Ops policy. Retain only approved
public source, never private implementation/evidence or implicitly published
native artifacts. Existing performance/readiness caveats remain binding.
