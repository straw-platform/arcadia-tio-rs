# Release notes

## 0.3.6 - repository transfer and contract documentation

Tag: `0.3.6`

This source-only patch release carries forward the 0.3.5 bounded parallel OCB
session and adds the public Rust tutorial, explicit cancellation-race guidance,
and canonical `straw-platform/arcadia-tio-rs` repository metadata. There is no
intentional public Rust API or C ABI change from 0.3.5; the calibrated safe
wrapper, raw sys, and C-ABI-free OCB-core surface counts remain unchanged.

This tag does not publish crates.io packages, native libraries, signed
artifacts, or benchmark evidence, and creates no performance, storage,
capacity, production-default, or release-readiness claim.

## 0.3.5 - poll-based bounded parallel OCB session

Tag: `0.3.5`

This source release promotes bounded parallel OCB preparation through
ten C ABI functions and seven opaque/carrier/status types, exact raw sys
declarations, and a safe Rust RAII iterator/session. Results are owned and
ordered, cancellation is idempotent and may race successful completion, and
callers inspect the observed terminal status and matching report. Terminal
reports expose worker/queue/attribution facts, and active drop cancels and
joins. Rust owns the worker threads; generic `Send + 'static` callback
preparation remains available only in the separate C-ABI-free OCB core crate.

The calibrated public safe-wrapper count moves from **776** to **787** and raw
sys coverage moves from **522 / 522** to **539 / 539**. The separate public OCB
core stays **279**. This source tag does not publish crates.io packages, native
libraries, signed artifacts, or benchmark evidence, and creates no performance,
storage, capacity, or production-default claim.

## 0.3.4 — bounded parallel OCB preparation

Tag: `0.3.4`
Commit: see `git rev-parse 0.3.4`

### Scope

This source-only public Rust release adds an opt-in bounded worker path to the
C-ABI-free `arcadia-tio-ocb-core` crate:

- Adds generic row-group parallel preparation with deterministic caller-thread
  ordered commit.
- Adds a one-channel compact-L2 physical-v2 preparation helper without nested
  channel and row-group worker pools.
- Requires worker callbacks to return owned `Send + 'static` results so decoded
  batch borrows cannot escape their invocation.
- Adds boundedness, worker, queue-pressure, attribution, cancellation, panic,
  deterministic-error, and external-consumer contract coverage.
- Preserves existing reader behavior and the C ABI-backed Rust wrapper API.

The calibrated public OCB-core user-facing surface moves from **270** items in
0.3.3 to **279** in 0.3.4: eight crate-root exports and
`ColumnBundleFile::parallel_prepare_plan_row_groups`. This count is separate
from the existing **776-item** C-ABI-backed safe wrapper and does not change the
17-family cross-language parity score. C ABI coverage remains **522 / 522**;
Haskell remains **498 wrapped / 24 not applicable / 0 gaps**.

### Operational contract

The in-flight option bounds row groups by count, not arbitrary caller-owned
result bytes. Consumers needing an absolute temporary-memory limit must enforce
a per-result byte budget. Ordered commit is a sequencing boundary rather than a
transaction; fail-closed consumers must publish invocation-local staging only
after a terminally completed successful report.

### Non-goals

This release does not change the C ABI, Python bindings, C++ wrapper, or Haskell
wrapper. It does not publish crates.io packages, native libraries, signed
artifacts, benchmark evidence, storage/capacity claims, or production/default
runtime readiness.

### Validation summary

Maintainer validation before tagging includes:

- `cargo fmt --all -- --check`;
- `cargo metadata --format-version 1 --no-deps`;
- `cargo test -p arcadia-tio-ocb-core`;
- `cargo check -p arcadia-tio-ocb-core --examples`;
- `cargo make test-core-reader-tree`;
- `cargo make ci` when the operator-approved native C ABI library is available.

## 0.3.3 — documentation and project-structure cleanup

Tag: `0.3.3`
Commit: see `git rev-parse 0.3.3`

### Scope

This is a source-only maintenance release of the public Rust wrapper workspace
after the 0.3.2 stable source boundary:

- Aligns workspace and crate package metadata to `0.3.3`.
- Refreshes public README dependency instructions to use the `0.3.3` source
  tag.
- Preserves the 0.3.2 OCB Rust-core reader boundary and C-ABI-backed wrapper
  API surface with no intentional public API or ABI changes.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, Arcadia LOB replay semantics, or production/default runtime readiness.

### Validation summary

Maintainer validation before tagging should include:

- `cargo fmt --all -- --check`;
- `cargo metadata --format-version 1 --no-deps`;
- native-library-backed `cargo make ci` when an operator-approved C ABI library
  is supplied.

## 0.3.2 — compact-L2 physical-v2 cross-language parity

Tag: `0.3.2`
Commit: see `git rev-parse 0.3.2`

### Scope

This is a source-only release of the public Rust wrapper workspace focused on
aligning the public Rust source boundary with the OCB certification APIs exposed
through the private C ABI and other language wrappers:

- Keeps the C-ABI-free `arcadia-tio-ocb-core` physical-v2 reader and
  certification surface from the 0.3.1 candidate.
- Aligns workspace and crate package metadata to `0.3.2`.
- Keeps `arcadia-tio-sys` and `arcadia-tio-rs` version constraints consistent
  with the workspace source tag.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, Arcadia LOB replay semantics, or production/default runtime readiness.

### Validation summary

Maintainer validation before tagging should include:

- `cargo fmt --all -- --check`;
- `cargo metadata --format-version 1 --no-deps`;
- `cargo check -p arcadia-tio-ocb-core`;
- native-library-backed wrapper checks when an operator-approved C ABI library
  is supplied.

## 0.3.1 — compact-L2 physical-v2 OCB core candidate

Tag: `0.3.1`
Commit: see `git rev-parse 0.3.1`

### Scope

This is a source-only release of the public Rust wrapper workspace focused on
the C-ABI-free `arcadia-tio-ocb-core` crate:

- Adds compact-L2 `compact-l2-physical-v2` support as an explicit additive
  physical layout candidate.
- Adds stable physical-v2 column constants and v1 fixed-binary lane mapping.
- Adds exact in-memory reconstruction of the legacy 168-byte payload for
  compatibility and certification.
- Adds physical-v2 artifact and manifest certification helpers for
  channel-sharded OCB sets.
- Adds a bounded channel-parallel typed reader helper for physical-v2
  manifests.
- Adds path-redacted compact-L2 size-attribution and physical-v2 certification
  examples for private/operator diagnostics.

### OCB-core guidance

- Physical-v2 is a physical storage layout, not a unified order/trade business
  schema. `record_kind` tells downstream code how to interpret the shared body
  lanes.
- Physical-v2 is additive and does not replace `compact-fixed-binary-l2-v1` or
  make itself the downstream runtime default.
- Downstream still owns replay scheduling, owner assignment, order-book
  mutation, factor/KOB logic, shm-ring transport, LIVE orchestration, rollout
  policy, fallback policy, and production-readiness claims.
- Public reports and examples must remain aggregate/path-redacted and must not
  log raw private records or payload bytes.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, Arcadia LOB replay semantics, or production/default runtime readiness.
It does not change the C ABI, Python binding, C++ wrapper, or Haskell wrapper
surface.

### Validation summary

Maintainer validation before tagging included:

- `cargo fmt --all -- --check`;
- `cargo check -p arcadia-tio-ocb-core`;
- `cargo check -p arcadia-tio-ocb-core --examples`;
- `cargo test -p arcadia-tio-ocb-core`;
- `cargo test -p arcadia-tio-ocb-core --no-default-features`;
- `cargo make test-core-reader-tree`;
- `cargo make test-core-reader-no-cabi`.

## 0.3.0 — channel-sharded compact-L2 OCB certification

Tag: `0.3.0`
Commit: see `git rev-parse 0.3.0`

### Scope

This is a source-only release of the public Rust wrapper workspace focused on
`arcadia-tio-ocb-core`:

- Adds pure-Rust channel-sharded compact-L2 manifest parsing and safe
  manifest-relative artifact path resolution.
- Adds fixed-ingress compact-L2 binary header constants/decoding for source
  format validation.
- Adds `certify_channel_sharded_artifact_v1(...)` with path-redacted reports for
  manifest/schema/count invariants, payload width, payload header fields,
  constant ChannelID, strict gap-free BizIndex continuity, receive-nano edges,
  and optional manifest hash/fingerprint checks.
- Adds structured OCB diagnostic kinds for unsafe manifest paths, missing
  artifacts, payload/header mismatches, ChannelID mismatches, BizIndex gaps or
  duplicates, checksum mismatches, and I/O diagnostics.

### OCB-core guidance

- The new compact-L2 helpers remain source-format certification facts only. They
  do not define replay scheduling, owner assignment, order-book mutation,
  factor/KOB logic, shm-ring transport, LIVE orchestration, runtime policy,
  benchmark claims, or production/default readiness.
- Public certification summaries and diagnostics avoid raw absolute paths and
  manifest-relative artifact path disclosure. Consumers should log the returned
  safe summaries and keep raw path evidence in private/operator scopes only.
- Optional manifest hashes are verified when present. `checksum_verified` is
  true only when at least one optional artifact hash/fingerprint check was
  present and passed.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, Arcadia LOB replay semantics, or production/default runtime readiness.

### Validation summary

Maintainer validation before tagging included:

- `cargo fmt --all -- --check`;
- `cargo check -p arcadia-tio-ocb-core`;
- `cargo check -p arcadia-tio-ocb-core --examples`;
- `cargo test -p arcadia-tio-ocb-core`;
- `cargo test -p arcadia-tio-ocb-core --no-default-features`;
- `cargo make test-core-reader-no-cabi` in the public workspace;
- downstream `arcadia-lob-player-runtime` targeted check/test review before
  downstream pin notification.

## 0.2.0 — public Rust wrapper source release

Tag: `0.2.0`
Commit: `3071a41`

### Scope

This is a source-only release of the public Rust wrapper workspace:

- `arcadia-tio-ocb-core` — C-ABI-free generic OCB selected-snapshot reader,
  read-planning, row-group visitor, reusable-buffer visitor, fixed-binary
  projection, attribution, and generic certification-substrate APIs.
- `arcadia-tio-sys` / `arcadia-tio-rs` — C-ABI-backed raw/safe wrapper source
  crates for consumers that supply an operator-approved native
  `arcadia_tio_capi` library.

### OCB-core guidance

- The OCB-core boundary stays generic: no channel, BizIndex, fixed-ingress,
  compact-L2, replay, order-book, or market-data semantics are defined upstream.
- Certification identity values are deterministic compatibility identifiers
  under `ocb.generic.crc32c.v1`, not cryptographic file digests.
- Downstream payload-only runtime use should be manifest-gated and fail closed by
  comparing the selected snapshot fingerprint, root/previous-root generation,
  selected row-group ids/base rows/counts, selected chunk summaries/checksums,
  selected compressed/uncompressed byte totals, plan report, and
  `selected_chunk_fingerprint`.
- For row-group-coalesced reads, build one plan, union the needed plan-local
  row-group ids, execute `read_plan_row_groups(...)` or
  `visit_plan_row_groups_into_with_attribution(...)`, and demultiplex in the
  application.

### Non-goals

This release does not publish crates.io packages, native libraries, signed
artifacts, package-manager/system installs, benchmark evidence, storage/capacity
claims, or production/default runtime readiness.

### Validation summary

Maintainer validation before tagging included:

- `cargo make ci` in the public workspace with a locally supplied native C ABI
  library;
- `cargo test -p arcadia-tio-ocb-core`;
- `cargo check -p arcadia-tio-ocb-core --examples`;
- `cargo make test-core-reader-no-cabi`;
- downstream temporary pin smoke against `arcadia-lob-player-runtime` with
  `mock-live-ocb-core-reader`.
