# arcadia-tio-ocb-core

Source-visible Rust-core reader and bounded visitor APIs for Arcadia Ordered
Column Bundle (OCB) files.

This crate is intended for downstream Rust integrations that need OCB
selected-snapshot open, metadata inspection, read planning, projected/predicate
batch reads, explicit row-group visitors, reusable-buffer lower-copy visitors,
generic fixed-binary record field projection helpers and projected visitors,
read-plan certification summaries, channel-sharded compact-L2 manifest parsing,
fixed-ingress header validation, artifact certification helpers, and read
attribution without linking the native C ABI wrapper path.

It does not provide writer APIs, C/Python bindings, `TensorFile`, order-book
replay, owner assignment, factor/KOB logic, shm-ring transport, production LIVE
orchestration, native libraries, release artifacts, or performance/storage
claims.

## 0.3.4 release boundary

The 0.3.4 public Rust workspace source boundary adds opt-in bounded parallel
row-group preparation with deterministic caller-thread ordered commit. Worker
callbacks borrow decoded batches only for the callback invocation and return
owned `Send + 'static` results. One in-flight row-group cap bounds queued,
active, completed, and pending work by count; consumers that require an
absolute memory limit must also enforce a per-result byte budget. Existing
reader paths and all C ABI-backed language surfaces remain unchanged.

## 0.3.2 release boundary

The 0.3.2 public Rust workspace source boundary includes compact-L2
`compact-l2-physical-v2` support as an explicit, additive physical layout
candidate. It does not replace `compact-fixed-binary-l2-v1` and does not make
physical-v2 the downstream runtime default.

The OCB core owns only physical facts for this layout:

- stable physical-v2 column names and v1 fixed-binary lane mapping;
- exact in-memory reconstruction of the legacy 168-byte payload for
  compatibility and certification;
- artifact and manifest certification helpers for physical-v2 channel-sharded
  OCB sets;
- a bounded channel-parallel typed reader helper for physical-v2 manifests.

The layout is intentionally physical, not a unified order/trade business
schema. `record_kind` tells downstream code how to interpret the shared body
lanes. Downstream still owns replay, owner assignment, order-book mutation,
factor/KOB logic, scheduling, runtime policy, rollout/fallback, and any
production-readiness claim.

## 0.3.0 release boundary

The 0.3.0 public Rust workspace tag is a source release of the OCB core reader
substrate. The `arcadia-tio-ocb-core` crate remains C-ABI-free and owns only
selected-snapshot reader, planning, visitor, fixed-binary projection, generic
certification-substrate behavior, and source-visible channel-sharded compact-L2
artifact facts. It is not a production/default Arcadia runtime readiness claim:
upstream certification validates manifest, path, checksum, payload-header,
ChannelID, and BizIndex continuity facts, while downstream still owns replay,
owner assignment, order-book semantics, factor/KOB logic, and runtime policy.

Certification fingerprints are deterministic compatibility identifiers under
`ocb.generic.crc32c.v1`, not cryptographic digests. Fail-closed downstream gates
that enable payload-only reads should persist and compare the snapshot
`combined` fingerprint, root and previous-root generation identifiers, selected
row-group ids/base rows/counts, selected chunk summaries/checksums, selected
compressed and uncompressed byte totals, the plan report, and
`selected_chunk_fingerprint`. Full-file artifact digests remain offline/operator
recertification evidence rather than normal runtime startup validation.

For row-group-coalesced scans, build one `ReadPlan`, union the plan-local
row-group ids needed by downstream windows/channels, read them once with
`read_plan_row_groups(...)` or
`visit_plan_row_groups_into_with_attribution(...)`, and demultiplex in the
application using payload fields it owns. OCB validates the subset and preserves
plan order. The compact-L2 certification helpers validate source-format channel
and BizIndex continuity, but they intentionally do not define replay scheduling,
book mutation, factor output, or market-data runtime policy.

## Visitor contract

`ColumnBundleFile::visit_plan_row_groups_with_attribution(...)` validates the
explicit row-group ids against the supplied plan before payload reads. Unknown
and duplicate row-group ids fail closed with `ArcadiaTioErrorCode::InvalidArgument`,
`OcbFailureCause::InvalidInput`, and stable message constants:

- `OCB_READ_PLAN_SUBSET_DUPLICATE_ROW_GROUP_ERROR`
- `OCB_READ_PLAN_SUBSET_UNKNOWN_ROW_GROUP_ERROR`

Batches are yielded in original plan order, not caller subset order. Decoded
materialization is bounded by `min(max_in_flight_row_groups, effective_threads)`.
`callback_wall_ns` and `max_in_flight_row_groups_observed` are available for
visitor diagnostics.

`ColumnBundleFile::parallel_prepare_plan_row_groups(...)` is the additive
fixed-pool path for caller-owned row-group preparation. It uses the read plan's
`max_threads` as its only worker budget and applies one
`max_in_flight_row_groups` admission window across queued tasks, active decoded
batches, bounded results, and the ordered pending map. Worker preparation gets
deterministic row-group context plus a callback-lifetime `ColumnBatch` borrow and
must return an owned `Send + 'static` result. A single caller-thread `FnMut`
callback releases those results in selected-plan order. Later worker failures
remain pending until every earlier ordinal resolves, so the earliest selected
row-group error wins deterministically. `Stop` commits the current result but
leaves `ordered_terminal_completed` false.

The ordered callback is a sequencing boundary, not a transaction or publication
hook. It can run for earlier ordinals before a later worker failure is known, and
TIO cannot roll back side effects performed by that callback. A fail-closed
consumer must therefore write callback output only into invocation-local staging,
publish that staging only after the call returns `Ok(report)` with
`report.ordered_terminal_completed == true`, and discard it on `Err` or on an
`Ok` report with terminal completion false. This is also the required pattern for
replay-visible state: `Stop` and failure must never publish the partial stage.

The returned `ColumnBundleParallelPrepareReport` retains the existing OCB
read/checksum/decompression/decode attribution and adds requested/started/max
active workers, queued/completed/ordered-committed row groups, bounded in-flight
and pending maxima, capacity and queue pressure, ordered-frontier waits, and
per-worker read/preparation totals. The singular
`parallel_prepare_compact_l2_physical_v2_channel(...)` helper adds ChannelID to
the same context and validates the physical-v2 view inside the worker. It does
not call the channel-parallel reader and therefore does not create a nested
worker pool.

Parallel-prepare instrumentation has the following stable interpretation for
downstream reports:

- every `*_ns` value is monotonic elapsed time in nanoseconds;
- `attribution.execute_wall_ns` is the invocation wall span after planning;
  `ordered_commit_ns` and `attribution.callback_wall_ns` describe the same
  caller-thread ordered-callback bucket and must not be added together;
- summing each worker report's `row_group_read_ns + caller_prepare_ns` gives
  aggregate worker read-and-prepare elapsed time. It excludes worker idle and
  bounded result-queue publication wait. The measured buckets overlap across
  workers, may exceed invocation wall time, and are not operating-system thread
  CPU time;
- TIO does not report process CPU. A consumer that measures it must keep a
  separately named and unit-bearing field such as `process_cpu_seconds` or
  `process_cpu_ns`; it must not relabel aggregate worker elapsed time as CPU;
- capacity, task-queue, result-queue, and ordered-frontier wait buckets can
  overlap, so they are diagnostic series rather than additive phase totals;
- `worker_reports` is sorted by stable invocation-local `worker_id`; requested,
  started, active, queued, completed, ordered-committed, in-flight, and pending
  counters retain their field meanings across consumers.

The global admission window is a stable row-group-count bound, not an absolute
byte allocator. `max_pending_rows_observed` counts decoded rows in completed
results waiting at or beyond the result boundary; it does not include the byte
size of arbitrary caller-owned `T`. Launched row groups are bounded by
`max_in_flight_row_groups` only while they have not yet retired through the
ordered boundary; total `row_groups_queued` over a completed call can be larger.
The consumer remains responsible for a per-result row/byte budget. Consequently,
a downstream absolute temporary-memory contract must combine that per-result
budget with the TIO in-flight cap and must report its own observed temporary rows
and bytes. Before ordered handoff, the in-flight caller-result portion is bounded by
`max_in_flight_row_groups * per_result_bound` when that per-result bound is
enforced. Results moved into invocation-local staging have crossed the TIO
retirement boundary and are no longer covered by that cap; the consumer must
separately bound, stream, aggregate, or account for the staged state before
claiming an absolute temporary-memory bound.

For lower-copy reads, allocate a reusable pool with
`ColumnBundleFile::reusable_buffer_pool_for_plan(...)` and call
`visit_plan_row_groups_into(...)` or
`visit_plan_row_groups_into_with_attribution(...)`. The callback receives a
`ColumnBundleReusableBatchView<'_>` whose borrowed slices are valid only for the
callback duration and are overwritten when the pool slot is reused.

For packed fixed-width binary columns, `PrimitiveColumnValuesRef::fixed_binary_records`
and `FixedBinaryRecordView::{project_fields, project_fields_with_report}` can
decode little-endian primitive fields at caller-supplied byte offsets into
caller-owned buffers with optional projection-wall attribution. Builder helpers on
`FixedBinaryRecordProjection` and `FixedBinaryProjectedField` keep caller layouts
concise, and `FixedBinaryProjectedBatchView::field_by_name(...)` plus
`FixedBinaryFieldValuesRef::as_*()` provide fail-closed typed extraction. For the
bounded visitor path, `fixed_binary_projection_buffer_for_plan(...)` plus
`visit_plan_row_groups_project_fixed_binary_with_attribution(...)` performs the
same generic projection inside TIO before the callback and reports
`fixed_payload_decode_ns`. The example
`cargo run -p arcadia-tio-ocb-core --example project_fixed_binary -- <file> <column> <width>`
shows this flow. These generic helpers do not add order-book, replay, factor, or
market-data runtime semantics to OCB.

## Channel-sharded compact-L2 manifests and certification

`ChannelShardedManifestV1::from_path(...)` parses the upstream JSON manifest
schema and rejects unsafe absolute/traversing artifact paths. The public
constants `OCB_CORE_READER_API_VERSION`,
`CHANNEL_SHARDED_MANIFEST_SCHEMA_VERSION_V1`, and
`COMPACT_L2_FIXED_BINARY_SCHEMA_VERSION_V1` identify the supported reader and
source-format contract.

`certify_channel_sharded_artifact_v1(...)` opens every manifest-relative OCB
artifact through the pure-Rust reader and validates row counts, row-group counts
when present, fixed-binary payload width, non-null payload chunks, optional
SHA-256/FNV fingerprints, compact-L2 payload headers, trading day, constant
ChannelID per artifact, and strict gap-free BizIndex continuity. Reports include
`SafeCertificationSummary` with aggregate counts and no absolute paths.

For fail-closed payload-only gates, `snapshot_fingerprint()` and
`read_plan_certification(...)` expose deterministic generic metadata
fingerprints, projected row-group/chunk summaries, selected payload byte totals,
and selected chunk checksum fingerprints. A downstream gate should persist and
compare the snapshot `combined` fingerprint, the plan's row-group ids, selected
row-group/chunk summaries, selected byte totals, and `selected_chunk_fingerprint`
before enabling payload-only reads. These are certification substrates, not
cryptographic file digests or application-semantic guarantees.

Attribution fields distinguish summed TIO worker time from callback/handoff time:
`primitive_decode_ns` covers typed primitive decode, `copy_materialization_ns`
covers fixed-binary byte materialization into caller buffers,
`fixed_payload_decode_ns` covers generic fixed-binary field projection, and
`callback_wall_ns` covers time spent inside the caller visitor.
