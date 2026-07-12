use arcadia_tio_ocb_core::{
    ArcadiaTioError, ColumnBatch, ColumnBundleFile, ColumnBundleParallelPrepareContext,
    ColumnBundleParallelPrepareOptions, ColumnBundleParallelPrepareReport, ColumnBundleReadPlan,
    ColumnBundleVisitControl, CompactL2PhysicalV2ChannelReadInput,
    CompactL2PhysicalV2ParallelPrepareContext, CompactL2PhysicalV2ParallelPrepareOptions,
    CompactL2PhysicalV2ParallelPrepareReport, PrimitiveColumnValues, Result,
    parallel_prepare_compact_l2_physical_v2_channel,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct OwnedPreparedRowGroup {
    selected_ordinal: usize,
    row_group_id: u32,
    values: Vec<i64>,
}

fn prepare_owned(
    context: ColumnBundleParallelPrepareContext,
    batch: &ColumnBatch,
) -> Result<OwnedPreparedRowGroup> {
    let values = match batch.columns.first().map(|column| &column.values) {
        Some(PrimitiveColumnValues::I64(values)) => values.clone(),
        _ => {
            return Err(ArcadiaTioError::InvalidArgument(
                "external smoke expected one i64 projection",
            ));
        }
    };
    Ok(OwnedPreparedRowGroup {
        selected_ordinal: context.selected_row_group_ordinal,
        row_group_id: context.row_group_id,
        values,
    })
}

fn call_from_an_external_consumer(
    file: &ColumnBundleFile,
    plan: &ColumnBundleReadPlan,
    row_group_ids: &[u32],
    staged: &mut Vec<OwnedPreparedRowGroup>,
) -> Result<ColumnBundleParallelPrepareReport> {
    file.parallel_prepare_plan_row_groups(
        plan,
        row_group_ids,
        ColumnBundleParallelPrepareOptions {
            max_in_flight_row_groups: 2,
        },
        prepare_owned,
        |context, prepared| {
            assert_eq!(
                context.selected_row_group_ordinal,
                prepared.selected_ordinal
            );
            assert_eq!(context.row_group_id, prepared.row_group_id);
            staged.push(prepared);
            Ok(ColumnBundleVisitControl::Continue)
        },
    )
}

fn publish_only_after_terminal_success(
    report: &ColumnBundleParallelPrepareReport,
    staged: Vec<OwnedPreparedRowGroup>,
) -> Option<Vec<OwnedPreparedRowGroup>> {
    report.ordered_terminal_completed.then_some(staged)
}

fn read_stable_public_report_contract(report: &ColumnBundleParallelPrepareReport) {
    let _aggregate_counters = (
        report.requested_workers,
        report.started_workers,
        report.max_active_workers_observed,
        report.row_groups_queued,
        report.row_groups_completed,
        report.row_groups_ordered_committed,
        report.rows_completed,
        report.rows_ordered_committed,
        report.max_in_flight_row_groups_observed,
        report.max_pending_results_observed,
        report.max_pending_rows_observed,
        report.capacity_wait_count,
        report.capacity_wait_ns,
        report.task_queue_full_wait_count,
        report.task_queue_full_wait_ns,
        report.result_queue_full_wait_count,
        report.result_queue_full_wait_ns,
        report.ordered_frontier_wait_count,
        report.ordered_frontier_wait_ns,
        report.caller_prepare_ns,
        report.ordered_commit_ns,
        report.ordered_terminal_completed,
    );
    let _timing_contract = (
        report.attribution.execute_wall_ns,
        report.attribution.callback_wall_ns,
        report.attribution.row_group_read_ns,
    );
    let _worker_series = report
        .worker_reports
        .iter()
        .map(|worker| {
            (
                worker.worker_id,
                worker.row_groups_completed,
                worker.rows_completed,
                worker.row_group_read_ns,
                worker.caller_prepare_ns,
            )
        })
        .collect::<Vec<_>>();
}

fn call_physical_v2_from_an_external_consumer(
    input: CompactL2PhysicalV2ChannelReadInput,
    staged: &mut Vec<OwnedPreparedRowGroup>,
) -> Result<CompactL2PhysicalV2ParallelPrepareReport> {
    parallel_prepare_compact_l2_physical_v2_channel(
        input,
        CompactL2PhysicalV2ParallelPrepareOptions {
            workers: 2,
            max_in_flight_row_groups: 2,
            validate_checksums: true,
        },
        |context, view| {
            Ok(OwnedPreparedRowGroup {
                selected_ordinal: context.row_group.selected_row_group_ordinal,
                row_group_id: context.row_group.row_group_id,
                values: view.biz_index.to_vec(),
            })
        },
        |context: CompactL2PhysicalV2ParallelPrepareContext, prepared| {
            assert_eq!(
                context.row_group.selected_row_group_ordinal,
                prepared.selected_ordinal
            );
            staged.push(prepared);
            Ok(ColumnBundleVisitControl::Continue)
        },
    )
}

fn assert_send_static<T: Send + 'static>() {}

#[test]
fn public_parallel_prepare_contract_compiles_for_an_external_consumer() {
    assert_send_static::<OwnedPreparedRowGroup>();
    assert_eq!(
        ColumnBundleParallelPrepareOptions::default().max_in_flight_row_groups,
        1
    );

    // Integration tests compile as an external crate. Taking these function
    // items type-checks the public generic callback shape and terminal publish
    // gate without relying on private fixture-writing helpers.
    let _call = call_from_an_external_consumer;
    let _physical_v2_call = call_physical_v2_from_an_external_consumer;
    let _publish = publish_only_after_terminal_success;
    let _report_contract = read_stable_public_report_contract;
}
