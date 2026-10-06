# TIO Rust wrappers business contract

This source-visible repository owns the exported C-ABI-free OCB reader plus
raw and safe Rust wrappers over a separately supplied native TIO library.
It is not the broader private TIO implementation repository.

- arcadia-tio-ocb-core stays C-ABI-free: no sys/capi dependency, native-link
  build script or runtime library. Reader-only users need no native setup.
- Only the approved canonical reader allowlist may be promoted from private
  TIO. Do not turn a public-core fix into a divergent private/public fork:
  coordinate the canonical owner and export/parity checks explicitly.
- arcadia-tio-sys owns raw ABI declarations/link discovery; arcadia-tio-rs owns
  safe Rust lifetimes, validation, RAII and owned conversions. Neither gains a
  Cargo dependency on private arcadia-tio or arcadia-tio-capi.
- Gate native compatibility before other native entry paths; preserve supported
  ABI checks and exact-once cleanup of malformed/partial native outputs.
  Optional symbols are real link requirements, not compatibility fallbacks.
- Parallel sessions retain move-owned lifecycle, terminal-only reports and
  cancel/drain/join on active drop. Cancellation can race successful completion;
  report the observed terminal state. Row-group slot caps are not byte bounds
  on caller-retained batches.
- Preserve reader plan order, snapshot semantics, borrowed callback lifetimes,
  finite limits, physical-format facts and stable error classification.
  Book/factor/replay/scheduling and production policy remain downstream.
- Native libraries, generated datasets and release bundles are local artifacts,
  not source exports. Source/tests/examples do not establish performance,
  storage efficiency, capacity, zero-copy or production/release readiness.
- Keep feature gates and no-default/native-free behavior meaningful. Stable
  facades and module-size ratchets are compatibility constraints, not permission
  for broad reorganization during a small API change.
- Existing application format-integrity algorithms are not a license to use
  fingerprints or checksums for agent coordination/source/evidence identity.
  Follow Ops' purpose-specific NO HASH rule and report conflicting legacy steps.

## Read by change

Use the [public wrapper route map](docs/agent/README.md) to select the relevant
core-reader, safe-wrapper, raw-FFI, optional-feature, tutorial or linking surface.
It is a local API map, not a second operating policy.

Reader tests are native-free. Wrapper/FFI tests require the explicitly selected
compatible library and loader setup. Private-header parity requires an explicit
checked include root; an absent dependency is unverified/refused, not a pass.
Use only the affected feature/ABI matrix; full release matrices remain separate.
