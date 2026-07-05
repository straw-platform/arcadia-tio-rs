# Release notes

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
