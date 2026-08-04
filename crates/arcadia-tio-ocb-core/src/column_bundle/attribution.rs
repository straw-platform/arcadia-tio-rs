//! Read-attribution accumulation and stable report conversion.

//! Read timing and byte-attribution aggregation for the stable reader facade.

use super::*;

impl ReadAttributionAccumulator {
    pub(crate) fn add(&mut self, other: Self) {
        self.row_group_read += other.row_group_read;
        self.read_io += other.read_io;
        self.checksum += other.checksum;
        self.decompression += other.decompression;
        self.primitive_decode += other.primitive_decode;
        self.fixed_payload_decode += other.fixed_payload_decode;
        self.copy_materialization += other.copy_materialization;
        self.callback += other.callback;
        self.row_groups_materialized = self
            .row_groups_materialized
            .saturating_add(other.row_groups_materialized);
        self.column_chunks_materialized = self
            .column_chunks_materialized
            .saturating_add(other.column_chunks_materialized);
        self.bytes_read = self.bytes_read.saturating_add(other.bytes_read);
        self.compressed_bytes = self.compressed_bytes.saturating_add(other.compressed_bytes);
        self.uncompressed_bytes = self
            .uncompressed_bytes
            .saturating_add(other.uncompressed_bytes);
    }

    pub(super) fn add_object(&mut self, object: OcbReadObjectAttribution) {
        self.read_io += object.read_io;
        self.checksum += object.checksum;
        self.bytes_read = self.bytes_read.saturating_add(object.bytes_read);
    }

    pub(crate) fn add_callback(&mut self, duration: Duration) {
        self.callback += duration;
    }
}

pub(crate) fn duration_to_ns(duration: Duration) -> u64 {
    duration.as_nanos().min(u128::from(u64::MAX)) as u64
}

pub(super) fn record_value_materialization_time(
    attribution: &mut ReadAttributionAccumulator,
    physical_type: ColumnPhysicalType,
    elapsed: Duration,
) {
    match physical_type {
        ColumnPhysicalType::FixedBinary { .. } => attribution.copy_materialization += elapsed,
        _ => attribution.primitive_decode += elapsed,
    }
}

pub(crate) fn attribution_from_accumulator(
    accumulator: ReadAttributionAccumulator,
    report: &ColumnBundleReadReport,
    plan_ns: u64,
    execute_wall_ns: u64,
) -> ColumnBundleReadAttribution {
    ColumnBundleReadAttribution {
        plan_ns,
        execute_wall_ns,
        callback_wall_ns: duration_to_ns(accumulator.callback),
        row_group_read_ns: duration_to_ns(accumulator.row_group_read),
        read_io_ns: duration_to_ns(accumulator.read_io),
        checksum_ns: duration_to_ns(accumulator.checksum),
        decompression_ns: duration_to_ns(accumulator.decompression),
        primitive_decode_ns: duration_to_ns(accumulator.primitive_decode),
        fixed_payload_decode_ns: duration_to_ns(accumulator.fixed_payload_decode),
        copy_materialization_ns: duration_to_ns(accumulator.copy_materialization),
        native_to_c_copy_ns: None,
        wrapper_copy_ns: None,
        bytes_read: accumulator.bytes_read,
        compressed_bytes: accumulator.compressed_bytes,
        uncompressed_bytes: accumulator.uncompressed_bytes,
        requested_threads: report.requested_threads,
        effective_threads: report.effective_threads,
        selected_row_groups: accumulator.row_groups_materialized,
        pruned_row_groups: report.pruned_row_groups,
        selected_column_chunks: accumulator.column_chunks_materialized,
        fallback_reason: report.fallback_reason,
    }
}
