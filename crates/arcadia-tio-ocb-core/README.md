# arcadia-tio-ocb-core

Source-visible Rust-core reader and bounded visitor APIs for Arcadia Ordered
Column Bundle (OCB) files.

This crate is intended for downstream Rust integrations that need OCB
selected-snapshot open, metadata inspection, read planning, projected/predicate
batch reads, explicit row-group visitors, reusable-buffer lower-copy visitors,
generic fixed-binary record field projection helpers and projected visitors,
read-plan certification summaries, and read attribution without linking the
native C ABI wrapper path.

It does not provide writer APIs, C/Python bindings, `TensorFile`, market-data or
L2 semantics, domain-specific compact-L2/fixed-ingress adapters, cryptographic
payload certification manifests, native libraries, release artifacts, or
performance/storage claims.

## 0.2.0 release boundary

The 0.2.0 public Rust workspace tag is a source release of the generic OCB
substrate. The `arcadia-tio-ocb-core` crate remains C-ABI-free and owns only
selected-snapshot reader, planning, visitor, fixed-binary projection, and generic
certification-substrate behavior. It is not a production/default Arcadia runtime
readiness claim and it does not move downstream channel, BizIndex,
fixed-ingress, compact-L2, replay, or order-book semantics into OCB.

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
plan order, but it intentionally stays generic and does not define channel or
market-data selection semantics.

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
shows this flow. These helpers do not add channel, BizIndex, fixed-ingress,
order-book, replay, or market-data semantics to OCB.

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
