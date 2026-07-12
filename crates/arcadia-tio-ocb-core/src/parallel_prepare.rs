//! Bounded parallel row-group preparation with deterministic ordered commit.

use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::column_bundle::{
    ColumnBatch, ColumnBundleReadAttribution, ColumnBundleReadCursorReport, ColumnBundleReadReport,
    ColumnBundleVisitControl, ReadAttributionAccumulator, attribution_from_accumulator,
    duration_to_ns,
};
use crate::{ArcadiaTioError, Result};

/// Options for bounded parallel preparation and deterministic ordered commit.
///
/// The worker budget is intentionally not duplicated here. It comes from the
/// `ColumnBundleReadOptions::max_threads` value captured by the read plan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleParallelPrepareOptions {
    /// Maximum number of launched row groups not yet retired by ordered commit.
    ///
    /// This bounds row-group slots, not the byte size of arbitrary caller-owned
    /// preparation results. Callers that need an absolute memory bound must also
    /// enforce a per-result row/byte budget.
    pub max_in_flight_row_groups: usize,
}

impl Default for ColumnBundleParallelPrepareOptions {
    fn default() -> Self {
        Self {
            max_in_flight_row_groups: 1,
        }
    }
}

/// Deterministic row-group context supplied to worker preparation and commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnBundleParallelPrepareContext {
    /// Zero-based ordinal in the actual plan-ordered row-group selection.
    pub selected_row_group_ordinal: usize,
    /// File-local row-group id.
    pub row_group_id: u32,
    /// Logical first row in the selected snapshot.
    pub base_row: u64,
    /// Exclusive logical end row in the selected snapshot.
    pub row_end: u64,
    /// Logical row count for this row group.
    pub row_count: u64,
    /// Stable invocation-local worker slot id.
    ///
    /// Worker ids are stable identities within one call; task-to-worker
    /// assignment is deliberately not deterministic across calls.
    pub worker_id: usize,
}

/// Per-worker preparation counters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleParallelPrepareWorkerReport {
    /// Stable invocation-local worker slot id.
    pub worker_id: usize,
    /// Row groups whose worker stage reached a success or failure result.
    pub row_groups_completed: usize,
    /// Rows decoded before caller preparation, including failed preparations.
    pub rows_completed: u64,
    /// Wall time spent in row-group reads on this worker.
    pub row_group_read_ns: u64,
    /// Wall time spent in caller-owned preparation on this worker.
    pub caller_prepare_ns: u64,
}

/// Instrumentation for bounded parallel preparation and ordered commit.
///
/// Every `*_ns` field is monotonic elapsed time in nanoseconds. Per-worker
/// read/preparation values are elapsed buckets that can overlap across workers;
/// their sum is aggregate worker read-and-prepare elapsed time, excluding idle
/// and bounded result-queue publication wait, not process or thread CPU time.
/// Process CPU is deliberately outside this report. Wait buckets can overlap
/// with one another and must not be treated as additive phase totals.
///
/// Wait buckets are diagnostic and may overlap. Pending-result maxima include
/// prepared results waiting to enter the result queue, buffered in that queue,
/// and retained in the ordinal reorder map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ColumnBundleParallelPrepareReport {
    /// Existing visitor-compatible progress report.
    pub cursor_report: ColumnBundleReadCursorReport,
    /// Existing read/checksum/decompression/primitive-decode attribution.
    pub attribution: ColumnBundleReadAttribution,
    /// Worker count requested by the read plan.
    pub requested_workers: usize,
    /// Fixed worker count started for this invocation.
    pub started_workers: usize,
    /// Maximum workers simultaneously executing read/preparation work.
    pub max_active_workers_observed: usize,
    /// Row-group tasks admitted to the bounded worker stage.
    pub row_groups_queued: usize,
    /// Row groups whose worker stage reached a success or failure result.
    pub row_groups_completed: usize,
    /// Row groups released through the ordered commit boundary.
    pub row_groups_ordered_committed: usize,
    /// Rows decoded before caller preparation, including failed preparations.
    pub rows_completed: u64,
    /// Rows released through the ordered commit boundary.
    pub rows_ordered_committed: u64,
    /// Maximum launched-but-not-ordered-retired row groups.
    pub max_in_flight_row_groups_observed: usize,
    /// Maximum completed results not yet ordered-committed.
    pub max_pending_results_observed: usize,
    /// Maximum successfully decoded rows in pending results.
    pub max_pending_rows_observed: u64,
    /// Number of times admission waited at the global in-flight cap.
    pub capacity_wait_count: usize,
    /// Time waiting for capacity at the global in-flight cap.
    pub capacity_wait_ns: u64,
    /// Number of bounded task-queue full wait episodes.
    pub task_queue_full_wait_count: usize,
    /// Time spent in bounded task-queue full wait episodes.
    pub task_queue_full_wait_ns: u64,
    /// Number of bounded result-queue full wait episodes.
    pub result_queue_full_wait_count: usize,
    /// Time spent in bounded result-queue full wait episodes.
    pub result_queue_full_wait_ns: u64,
    /// Number of blocking result receives while the next ordered ordinal was unresolved.
    pub ordered_frontier_wait_count: usize,
    /// Time waiting for the next ordered ordinal to resolve.
    pub ordered_frontier_wait_ns: u64,
    /// Summed worker time spent in caller preparation.
    pub caller_prepare_ns: u64,
    /// Time spent inside the single-threaded ordered commit callback.
    ///
    /// This is the same elapsed bucket exposed as
    /// `attribution.callback_wall_ns`; the two values are not additive.
    pub ordered_commit_ns: u64,
    /// True only when every selected ordinal crossed the ordered commit boundary.
    ///
    /// A callback `Stop` always leaves this false, including when it is returned
    /// for the final selected row group.
    pub ordered_terminal_completed: bool,
    /// Per-worker reports sorted by worker id.
    pub worker_reports: Vec<ColumnBundleParallelPrepareWorkerReport>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParallelPrepareTaskSpec {
    pub(crate) selected_row_group_ordinal: usize,
    pub(crate) row_group_id: u32,
    pub(crate) base_row: u64,
    pub(crate) row_end: u64,
    pub(crate) row_count: u64,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ParallelPrepareRuntimeOptions {
    task_queue_capacity: usize,
    result_queue_capacity: usize,
}

impl ParallelPrepareRuntimeOptions {
    pub(crate) fn production(started_workers: usize) -> Self {
        let capacity = started_workers.max(1);
        Self {
            task_queue_capacity: capacity,
            result_queue_capacity: capacity,
        }
    }

    #[cfg(test)]
    fn with_queue_capacities(task_queue_capacity: usize, result_queue_capacity: usize) -> Self {
        Self {
            task_queue_capacity: task_queue_capacity.max(1),
            result_queue_capacity: result_queue_capacity.max(1),
        }
    }
}

struct TaskQueueState {
    tasks: VecDeque<ParallelPrepareTaskSpec>,
    closed: bool,
}

struct TaskQueue {
    capacity: usize,
    state: Mutex<TaskQueueState>,
    available: Condvar,
}

impl TaskQueue {
    fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            state: Mutex::new(TaskQueueState {
                tasks: VecDeque::new(),
                closed: false,
            }),
            available: Condvar::new(),
        }
    }

    fn try_push(&self, task: ParallelPrepareTaskSpec) -> TaskPush {
        let mut state = lock_unpoisoned(&self.state);
        if state.closed {
            return TaskPush::Closed;
        }
        if state.tasks.len() >= self.capacity {
            return TaskPush::Full;
        }
        state.tasks.push_back(task);
        self.available.notify_one();
        TaskPush::Pushed
    }

    fn pop(&self, cancelled: &AtomicBool) -> Option<ParallelPrepareTaskSpec> {
        let mut state = lock_unpoisoned(&self.state);
        loop {
            if cancelled.load(Ordering::Acquire) {
                return None;
            }
            if let Some(task) = state.tasks.pop_front() {
                return Some(task);
            }
            if state.closed {
                return None;
            }
            state = wait_unpoisoned(&self.available, state);
        }
    }

    fn close(&self) {
        let mut state = lock_unpoisoned(&self.state);
        state.closed = true;
        self.available.notify_all();
    }
}

enum TaskPush {
    Pushed,
    Full,
    Closed,
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

struct ParallelPrepareMessage<T> {
    context: ColumnBundleParallelPrepareContext,
    prepared: Result<T>,
}

struct WorkerExecutionReport {
    worker_id: usize,
    row_groups_completed: usize,
    rows_completed: u64,
    row_group_read: Duration,
    caller_prepare: Duration,
    attribution: ReadAttributionAccumulator,
}

impl WorkerExecutionReport {
    fn new(worker_id: usize) -> Self {
        Self {
            worker_id,
            row_groups_completed: 0,
            rows_completed: 0,
            row_group_read: Duration::ZERO,
            caller_prepare: Duration::ZERO,
            attribution: ReadAttributionAccumulator::default(),
        }
    }
}

struct ActiveWorkerGuard<'a> {
    active_workers: &'a AtomicUsize,
}

impl Drop for ActiveWorkerGuard<'_> {
    fn drop(&mut self) {
        self.active_workers.fetch_sub(1, Ordering::AcqRel);
    }
}

struct WorkerPoolCleanup {
    cancelled: Arc<AtomicBool>,
    task_queue: Arc<TaskQueue>,
}

impl Drop for WorkerPoolCleanup {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        self.task_queue.close();
    }
}

#[derive(Default)]
struct CoordinatorReport {
    row_groups_queued: usize,
    row_groups_ordered_committed: usize,
    rows_ordered_committed: u64,
    max_in_flight_row_groups_observed: usize,
    capacity_wait_count: usize,
    capacity_wait: Duration,
    task_queue_full_wait_count: usize,
    task_queue_full_wait: Duration,
    ordered_frontier_wait_count: usize,
    ordered_frontier_wait: Duration,
    ordered_commit: Duration,
    cancelled: bool,
}

struct SharedObservations {
    active_workers: AtomicUsize,
    max_active_workers: AtomicUsize,
    pending_results: AtomicUsize,
    pending_rows: AtomicU64,
    max_pending_results: AtomicUsize,
    max_pending_rows: AtomicU64,
    result_queue_full_wait_count: AtomicUsize,
    result_queue_full_wait_ns: AtomicU64,
}

impl SharedObservations {
    fn new() -> Self {
        Self {
            active_workers: AtomicUsize::new(0),
            max_active_workers: AtomicUsize::new(0),
            pending_results: AtomicUsize::new(0),
            pending_rows: AtomicU64::new(0),
            max_pending_results: AtomicUsize::new(0),
            max_pending_rows: AtomicU64::new(0),
            result_queue_full_wait_count: AtomicUsize::new(0),
            result_queue_full_wait_ns: AtomicU64::new(0),
        }
    }

    fn worker_started(&self) -> ActiveWorkerGuard<'_> {
        let active = self.active_workers.fetch_add(1, Ordering::AcqRel) + 1;
        observe_max_usize(&self.max_active_workers, active);
        ActiveWorkerGuard {
            active_workers: &self.active_workers,
        }
    }

    fn result_completed(&self, rows: u64) {
        let pending_rows = self.pending_rows.fetch_add(rows, Ordering::AcqRel) + rows;
        let pending_results = self.pending_results.fetch_add(1, Ordering::AcqRel) + 1;
        observe_max_usize(&self.max_pending_results, pending_results);
        observe_max_u64(&self.max_pending_rows, pending_rows);
    }

    fn result_committed(&self, rows: u64) {
        self.pending_rows.fetch_sub(rows, Ordering::AcqRel);
        self.pending_results.fetch_sub(1, Ordering::AcqRel);
    }
}

fn observe_max_usize(target: &AtomicUsize, value: usize) {
    let mut observed = target.load(Ordering::Acquire);
    while value > observed {
        match target.compare_exchange_weak(observed, value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(actual) => observed = actual,
        }
    }
}

fn observe_max_u64(target: &AtomicU64, value: u64) {
    let mut observed = target.load(Ordering::Acquire);
    while value > observed {
        match target.compare_exchange_weak(observed, value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(actual) => observed = actual,
        }
    }
}

fn atomic_saturating_add_u64(target: &AtomicU64, value: u64) {
    let _ = target.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_add(value))
    });
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn execute_parallel_prepare<T, Read, Prepare, Commit>(
    tasks: Vec<ParallelPrepareTaskSpec>,
    base_report: ColumnBundleReadReport,
    plan_ns: u64,
    options: ColumnBundleParallelPrepareOptions,
    read: Read,
    prepare: Prepare,
    ordered_commit: Commit,
) -> Result<ColumnBundleParallelPrepareReport>
where
    T: Send + 'static,
    Read: Fn(u32) -> Result<(ColumnBatch, ReadAttributionAccumulator)> + Sync,
    Prepare: Fn(ColumnBundleParallelPrepareContext, &ColumnBatch) -> Result<T> + Sync,
    Commit: FnMut(ColumnBundleParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    let started_workers = base_report
        .effective_threads
        .min(options.max_in_flight_row_groups)
        .min(tasks.len());
    execute_parallel_prepare_with_runtime(
        tasks,
        base_report,
        plan_ns,
        options,
        ParallelPrepareRuntimeOptions::production(started_workers),
        read,
        prepare,
        ordered_commit,
        |_| {},
        |_| {},
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_parallel_prepare_with_runtime<T, Read, Prepare, Commit, BeforePublish, AfterPublish>(
    tasks: Vec<ParallelPrepareTaskSpec>,
    mut base_report: ColumnBundleReadReport,
    plan_ns: u64,
    options: ColumnBundleParallelPrepareOptions,
    runtime: ParallelPrepareRuntimeOptions,
    read: Read,
    prepare: Prepare,
    mut ordered_commit: Commit,
    before_publish: BeforePublish,
    after_publish: AfterPublish,
) -> Result<ColumnBundleParallelPrepareReport>
where
    T: Send + 'static,
    Read: Fn(u32) -> Result<(ColumnBatch, ReadAttributionAccumulator)> + Sync,
    Prepare: Fn(ColumnBundleParallelPrepareContext, &ColumnBatch) -> Result<T> + Sync,
    Commit: FnMut(ColumnBundleParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
    BeforePublish: Fn(ColumnBundleParallelPrepareContext) + Sync,
    AfterPublish: Fn(ColumnBundleParallelPrepareContext) + Sync,
{
    if options.max_in_flight_row_groups == 0 {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB parallel prepare max_in_flight_row_groups must be greater than zero",
        ));
    }
    if tasks.len() != base_report.selected_row_groups {
        return Err(ArcadiaTioError::ocb_invalid_input(
            "OCB parallel prepare task count does not match execution report",
        ));
    }

    let execute_started = Instant::now();
    let requested_workers = base_report.requested_threads;
    let started_workers = base_report
        .effective_threads
        .min(options.max_in_flight_row_groups)
        .min(tasks.len());
    base_report.effective_threads = started_workers;
    if tasks.is_empty() {
        return Ok(empty_report(
            base_report,
            plan_ns,
            requested_workers,
            execute_started.elapsed(),
        ));
    }

    let task_queue = Arc::new(TaskQueue::new(runtime.task_queue_capacity));
    let cancelled = Arc::new(AtomicBool::new(false));
    let observations = Arc::new(SharedObservations::new());
    let (result_sender, result_receiver) =
        mpsc::sync_channel::<ParallelPrepareMessage<T>>(runtime.result_queue_capacity.max(1));

    let (coordinator_result, worker_reports) = thread::scope(|scope| {
        let _cleanup = WorkerPoolCleanup {
            cancelled: Arc::clone(&cancelled),
            task_queue: Arc::clone(&task_queue),
        };
        let mut handles = Vec::with_capacity(started_workers);
        for worker_id in 0..started_workers {
            let task_queue = Arc::clone(&task_queue);
            let cancelled = Arc::clone(&cancelled);
            let observations = Arc::clone(&observations);
            let sender = result_sender.clone();
            let read = &read;
            let prepare = &prepare;
            let before_publish = &before_publish;
            let after_publish = &after_publish;
            handles.push(scope.spawn(move || {
                worker_loop(
                    worker_id,
                    task_queue,
                    cancelled,
                    observations,
                    sender,
                    read,
                    prepare,
                    before_publish,
                    after_publish,
                )
            }));
        }
        drop(result_sender);

        let coordinator_result = coordinate_ordered_commit(
            &tasks,
            options.max_in_flight_row_groups,
            &task_queue,
            &cancelled,
            &observations,
            &result_receiver,
            &mut ordered_commit,
        );
        if coordinator_result.is_err()
            || coordinator_result
                .as_ref()
                .is_ok_and(|report| report.cancelled)
        {
            cancelled.store(true, Ordering::Release);
        }
        task_queue.close();

        let mut worker_reports = Vec::with_capacity(handles.len());
        let mut join_error = None;
        for handle in handles {
            match handle.join() {
                Ok(report) => worker_reports.push(report),
                Err(_) if join_error.is_none() => {
                    join_error = Some(ArcadiaTioError::Io(std::io::Error::other(
                        "OCB parallel-prepare worker panicked outside task execution",
                    )));
                }
                Err(_) => {}
            }
        }
        let coordinator_result = match (coordinator_result, join_error) {
            (Ok(_), Some(err)) => Err(err),
            (result, _) => result,
        };
        (coordinator_result, worker_reports)
    });

    let coordinator = coordinator_result?;
    let execute_wall_ns = duration_to_ns(execute_started.elapsed());
    let mut attribution_accumulator = ReadAttributionAccumulator::default();
    attribution_accumulator.add_callback(coordinator.ordered_commit);
    let mut public_worker_reports = Vec::with_capacity(worker_reports.len());
    let mut row_groups_completed = 0usize;
    let mut rows_completed = 0u64;
    let mut caller_prepare = Duration::ZERO;
    for worker in worker_reports {
        row_groups_completed = row_groups_completed.saturating_add(worker.row_groups_completed);
        rows_completed = rows_completed.saturating_add(worker.rows_completed);
        caller_prepare += worker.caller_prepare;
        attribution_accumulator.add(worker.attribution);
        public_worker_reports.push(ColumnBundleParallelPrepareWorkerReport {
            worker_id: worker.worker_id,
            row_groups_completed: worker.row_groups_completed,
            rows_completed: worker.rows_completed,
            row_group_read_ns: duration_to_ns(worker.row_group_read),
            caller_prepare_ns: duration_to_ns(worker.caller_prepare),
        });
    }
    public_worker_reports.sort_by_key(|worker| worker.worker_id);
    let attribution = attribution_from_accumulator(
        attribution_accumulator,
        &base_report,
        plan_ns,
        execute_wall_ns,
    );
    let ordered_terminal_completed =
        !coordinator.cancelled && coordinator.row_groups_ordered_committed == tasks.len();
    let cursor_report = ColumnBundleReadCursorReport {
        base_report,
        batches_yielded: coordinator.row_groups_ordered_committed,
        rows_yielded: coordinator.rows_ordered_committed,
        max_in_flight_row_groups_observed: coordinator.max_in_flight_row_groups_observed,
        cancelled: coordinator.cancelled,
    };

    Ok(ColumnBundleParallelPrepareReport {
        cursor_report,
        attribution,
        requested_workers,
        started_workers,
        max_active_workers_observed: observations.max_active_workers.load(Ordering::Acquire),
        row_groups_queued: coordinator.row_groups_queued,
        row_groups_completed,
        row_groups_ordered_committed: coordinator.row_groups_ordered_committed,
        rows_completed,
        rows_ordered_committed: coordinator.rows_ordered_committed,
        max_in_flight_row_groups_observed: coordinator.max_in_flight_row_groups_observed,
        max_pending_results_observed: observations.max_pending_results.load(Ordering::Acquire),
        max_pending_rows_observed: observations.max_pending_rows.load(Ordering::Acquire),
        capacity_wait_count: coordinator.capacity_wait_count,
        capacity_wait_ns: duration_to_ns(coordinator.capacity_wait),
        task_queue_full_wait_count: coordinator.task_queue_full_wait_count,
        task_queue_full_wait_ns: duration_to_ns(coordinator.task_queue_full_wait),
        result_queue_full_wait_count: observations
            .result_queue_full_wait_count
            .load(Ordering::Acquire),
        result_queue_full_wait_ns: observations
            .result_queue_full_wait_ns
            .load(Ordering::Acquire),
        ordered_frontier_wait_count: coordinator.ordered_frontier_wait_count,
        ordered_frontier_wait_ns: duration_to_ns(coordinator.ordered_frontier_wait),
        caller_prepare_ns: duration_to_ns(caller_prepare),
        ordered_commit_ns: duration_to_ns(coordinator.ordered_commit),
        ordered_terminal_completed,
        worker_reports: public_worker_reports,
    })
}

fn empty_report(
    base_report: ColumnBundleReadReport,
    plan_ns: u64,
    requested_workers: usize,
    execute_wall: Duration,
) -> ColumnBundleParallelPrepareReport {
    let attribution = attribution_from_accumulator(
        ReadAttributionAccumulator::default(),
        &base_report,
        plan_ns,
        duration_to_ns(execute_wall),
    );
    ColumnBundleParallelPrepareReport {
        cursor_report: ColumnBundleReadCursorReport {
            base_report,
            batches_yielded: 0,
            rows_yielded: 0,
            max_in_flight_row_groups_observed: 0,
            cancelled: false,
        },
        attribution,
        requested_workers,
        started_workers: 0,
        max_active_workers_observed: 0,
        row_groups_queued: 0,
        row_groups_completed: 0,
        row_groups_ordered_committed: 0,
        rows_completed: 0,
        rows_ordered_committed: 0,
        max_in_flight_row_groups_observed: 0,
        max_pending_results_observed: 0,
        max_pending_rows_observed: 0,
        capacity_wait_count: 0,
        capacity_wait_ns: 0,
        task_queue_full_wait_count: 0,
        task_queue_full_wait_ns: 0,
        result_queue_full_wait_count: 0,
        result_queue_full_wait_ns: 0,
        ordered_frontier_wait_count: 0,
        ordered_frontier_wait_ns: 0,
        caller_prepare_ns: 0,
        ordered_commit_ns: 0,
        ordered_terminal_completed: true,
        worker_reports: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn worker_loop<T, Read, Prepare, BeforePublish, AfterPublish>(
    worker_id: usize,
    task_queue: Arc<TaskQueue>,
    cancelled: Arc<AtomicBool>,
    observations: Arc<SharedObservations>,
    result_sender: mpsc::SyncSender<ParallelPrepareMessage<T>>,
    read: &Read,
    prepare: &Prepare,
    before_publish: &BeforePublish,
    after_publish: &AfterPublish,
) -> WorkerExecutionReport
where
    T: Send + 'static,
    Read: Fn(u32) -> Result<(ColumnBatch, ReadAttributionAccumulator)> + Sync,
    Prepare: Fn(ColumnBundleParallelPrepareContext, &ColumnBatch) -> Result<T> + Sync,
    BeforePublish: Fn(ColumnBundleParallelPrepareContext) + Sync,
    AfterPublish: Fn(ColumnBundleParallelPrepareContext) + Sync,
{
    let mut report = WorkerExecutionReport::new(worker_id);
    while let Some(task) = task_queue.pop(&cancelled) {
        if cancelled.load(Ordering::Acquire) {
            break;
        }
        let context = ColumnBundleParallelPrepareContext {
            selected_row_group_ordinal: task.selected_row_group_ordinal,
            row_group_id: task.row_group_id,
            base_row: task.base_row,
            row_end: task.row_end,
            row_count: task.row_count,
            worker_id,
        };
        let active_guard = observations.worker_started();
        let mut decoded_rows = 0u64;
        let prepared = catch_unwind(AssertUnwindSafe(|| {
            let read_started = Instant::now();
            let read_result = read(task.row_group_id);
            report.row_group_read += read_started.elapsed();
            let (batch, attribution) = read_result?;
            if batch.row_group_id != context.row_group_id
                || batch.base_row != context.base_row
                || batch.row_count != context.row_count
            {
                return Err(ArcadiaTioError::ocb_corrupt_file(
                    "OCB parallel prepare decoded batch context does not match row-group metadata",
                ));
            }
            decoded_rows = batch.row_count;
            report.attribution.add(attribution);
            let prepare_started = Instant::now();
            let prepared = prepare(context, &batch);
            report.caller_prepare += prepare_started.elapsed();
            before_publish(context);
            prepared
        }))
        .unwrap_or_else(|_| {
            Err(ArcadiaTioError::Io(std::io::Error::other(format!(
                "OCB parallel-prepare worker panicked at selected row-group ordinal {}",
                context.selected_row_group_ordinal
            ))))
        });
        drop(active_guard);

        report.row_groups_completed = report.row_groups_completed.saturating_add(1);
        report.rows_completed = report.rows_completed.saturating_add(decoded_rows);
        observations.result_completed(decoded_rows);
        let mut message = ParallelPrepareMessage { context, prepared };
        let mut full_wait_started: Option<Instant> = None;
        loop {
            if cancelled.load(Ordering::Acquire) {
                if let Some(started) = full_wait_started {
                    atomic_saturating_add_u64(
                        &observations.result_queue_full_wait_ns,
                        duration_to_ns(started.elapsed()),
                    );
                }
                return report;
            }
            match result_sender.try_send(message) {
                Ok(()) => {
                    if let Some(started) = full_wait_started {
                        atomic_saturating_add_u64(
                            &observations.result_queue_full_wait_ns,
                            duration_to_ns(started.elapsed()),
                        );
                    }
                    after_publish(context);
                    break;
                }
                Err(mpsc::TrySendError::Full(returned)) => {
                    message = returned;
                    if full_wait_started.is_none() {
                        observations
                            .result_queue_full_wait_count
                            .fetch_add(1, Ordering::AcqRel);
                        full_wait_started = Some(Instant::now());
                    }
                    thread::yield_now();
                }
                Err(mpsc::TrySendError::Disconnected(_)) => return report,
            }
        }
    }
    report
}

#[allow(clippy::too_many_arguments)]
fn coordinate_ordered_commit<T, Commit>(
    tasks: &[ParallelPrepareTaskSpec],
    max_in_flight_row_groups: usize,
    task_queue: &TaskQueue,
    cancelled: &AtomicBool,
    observations: &SharedObservations,
    result_receiver: &mpsc::Receiver<ParallelPrepareMessage<T>>,
    ordered_commit: &mut Commit,
) -> Result<CoordinatorReport>
where
    Commit: FnMut(ColumnBundleParallelPrepareContext, T) -> Result<ColumnBundleVisitControl>,
{
    let mut report = CoordinatorReport::default();
    let mut next_to_queue = 0usize;
    let mut next_to_commit = 0usize;
    let mut pending: BTreeMap<usize, ParallelPrepareMessage<T>> = BTreeMap::new();
    let mut stop_launching = false;
    let mut task_full_wait_started = None;

    while next_to_commit < tasks.len() {
        while let Some(message) = pending.remove(&next_to_commit) {
            match message.prepared {
                Ok(prepared) => {
                    let commit_started = Instant::now();
                    let control = catch_unwind(AssertUnwindSafe(|| {
                        ordered_commit(message.context, prepared)
                    }))
                    .unwrap_or_else(|_| {
                        Err(ArcadiaTioError::Io(std::io::Error::other(format!(
                            "OCB ordered commit panicked at selected row-group ordinal {}",
                            message.context.selected_row_group_ordinal
                        ))))
                    });
                    report.ordered_commit += commit_started.elapsed();
                    match control? {
                        ColumnBundleVisitControl::Continue => {}
                        ColumnBundleVisitControl::Stop => report.cancelled = true,
                    }
                    report.row_groups_ordered_committed =
                        report.row_groups_ordered_committed.saturating_add(1);
                    report.rows_ordered_committed = report
                        .rows_ordered_committed
                        .saturating_add(message.context.row_count);
                    observations.result_committed(message.context.row_count);
                    next_to_commit = next_to_commit.saturating_add(1);
                    if report.cancelled {
                        cancelled.store(true, Ordering::Release);
                        close_task_full_wait(&mut task_full_wait_started, &mut report);
                        return Ok(report);
                    }
                }
                Err(err) => {
                    cancelled.store(true, Ordering::Release);
                    close_task_full_wait(&mut task_full_wait_started, &mut report);
                    return Err(err);
                }
            }
        }
        if next_to_commit == tasks.len() {
            break;
        }

        let in_flight = report.row_groups_queued.saturating_sub(next_to_commit);
        let can_queue =
            !stop_launching && next_to_queue < tasks.len() && in_flight < max_in_flight_row_groups;
        if can_queue {
            match task_queue.try_push(tasks[next_to_queue]) {
                TaskPush::Pushed => {
                    close_task_full_wait(&mut task_full_wait_started, &mut report);
                    next_to_queue = next_to_queue.saturating_add(1);
                    report.row_groups_queued = report.row_groups_queued.saturating_add(1);
                    report.max_in_flight_row_groups_observed = report
                        .max_in_flight_row_groups_observed
                        .max(report.row_groups_queued.saturating_sub(next_to_commit));
                    if next_to_queue == tasks.len() {
                        task_queue.close();
                    }
                    continue;
                }
                TaskPush::Full => {
                    if task_full_wait_started.is_none() {
                        report.task_queue_full_wait_count =
                            report.task_queue_full_wait_count.saturating_add(1);
                        task_full_wait_started = Some(Instant::now());
                    }
                    match result_receiver.try_recv() {
                        Ok(message) => {
                            close_task_full_wait(&mut task_full_wait_started, &mut report);
                            stop_launching |= message.prepared.is_err();
                            insert_pending(&mut pending, message)?;
                        }
                        Err(mpsc::TryRecvError::Empty) => thread::yield_now(),
                        Err(mpsc::TryRecvError::Disconnected) => {
                            return Err(disconnected_worker_error());
                        }
                    }
                    continue;
                }
                TaskPush::Closed => return Err(disconnected_worker_error()),
            }
        }

        close_task_full_wait(&mut task_full_wait_started, &mut report);
        if !stop_launching && next_to_queue < tasks.len() && in_flight >= max_in_flight_row_groups {
            report.capacity_wait_count = report.capacity_wait_count.saturating_add(1);
        }
        report.ordered_frontier_wait_count = report.ordered_frontier_wait_count.saturating_add(1);
        let wait_started = Instant::now();
        let message = result_receiver
            .recv()
            .map_err(|_| disconnected_worker_error())?;
        let waited = wait_started.elapsed();
        report.ordered_frontier_wait += waited;
        if !stop_launching && next_to_queue < tasks.len() && in_flight >= max_in_flight_row_groups {
            report.capacity_wait += waited;
        }
        stop_launching |= message.prepared.is_err();
        insert_pending(&mut pending, message)?;
    }
    close_task_full_wait(&mut task_full_wait_started, &mut report);
    Ok(report)
}

fn insert_pending<T>(
    pending: &mut BTreeMap<usize, ParallelPrepareMessage<T>>,
    message: ParallelPrepareMessage<T>,
) -> Result<()> {
    if pending
        .insert(message.context.selected_row_group_ordinal, message)
        .is_some()
    {
        return Err(ArcadiaTioError::Io(std::io::Error::other(
            "OCB parallel prepare produced a duplicate row-group ordinal",
        )));
    }
    Ok(())
}

fn close_task_full_wait(started: &mut Option<Instant>, report: &mut CoordinatorReport) {
    if let Some(started) = started.take() {
        report.task_queue_full_wait += started.elapsed();
    }
}

fn disconnected_worker_error() -> ArcadiaTioError {
    ArcadiaTioError::Io(std::io::Error::other(
        "OCB parallel-prepare workers ended before the ordered frontier resolved",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::sync::Barrier;

    fn tasks(count: usize) -> Vec<ParallelPrepareTaskSpec> {
        (0..count)
            .map(|ordinal| ParallelPrepareTaskSpec {
                selected_row_group_ordinal: ordinal,
                row_group_id: ordinal as u32,
                base_row: ordinal as u64 * 2,
                row_end: ordinal as u64 * 2 + 2,
                row_count: 2,
            })
            .collect()
    }

    fn report(workers: usize, row_groups: usize) -> ColumnBundleReadReport {
        ColumnBundleReadReport {
            requested_threads: workers,
            effective_threads: workers.min(row_groups).max(1),
            selected_row_groups: row_groups,
            pruned_row_groups: 0,
            selected_column_chunks: 0,
            fallback_reason: None,
        }
    }

    fn read(row_group_id: u32) -> Result<(ColumnBatch, ReadAttributionAccumulator)> {
        Ok((
            ColumnBatch {
                row_group_id,
                base_row: u64::from(row_group_id) * 2,
                row_count: 2,
                columns: Vec::new(),
            },
            ReadAttributionAccumulator::default(),
        ))
    }

    #[test]
    fn worker_counts_produce_identical_ordered_output() {
        let mut expected = None;
        for workers in [1, 2, 4, 8] {
            let mut output = Vec::new();
            let outcome = execute_parallel_prepare(
                tasks(8),
                report(workers, 8),
                0,
                ColumnBundleParallelPrepareOptions {
                    max_in_flight_row_groups: 8,
                },
                read,
                |context, batch| {
                    Ok((
                        context.selected_row_group_ordinal,
                        batch.row_group_id,
                        batch.base_row,
                        batch.row_count,
                    ))
                },
                |context, prepared| {
                    assert_eq!(context.selected_row_group_ordinal, prepared.0);
                    output.push(prepared);
                    Ok(ColumnBundleVisitControl::Continue)
                },
            )
            .expect("parallel prepare");
            assert_eq!(outcome.row_groups_queued, 8);
            assert_eq!(outcome.row_groups_completed, 8);
            assert_eq!(outcome.row_groups_ordered_committed, 8);
            assert!(outcome.ordered_terminal_completed);
            assert_eq!(
                outcome.attribution.callback_wall_ns,
                outcome.ordered_commit_ns
            );
            assert_eq!(outcome.started_workers, workers);
            assert_eq!(outcome.worker_reports.len(), outcome.started_workers);
            assert_eq!(
                outcome
                    .worker_reports
                    .iter()
                    .map(|worker| worker.worker_id)
                    .collect::<Vec<_>>(),
                (0..outcome.started_workers).collect::<Vec<_>>()
            );
            assert_eq!(
                outcome
                    .worker_reports
                    .iter()
                    .map(|worker| worker.row_groups_completed)
                    .sum::<usize>(),
                8
            );
            assert_eq!(
                outcome.caller_prepare_ns,
                outcome
                    .worker_reports
                    .iter()
                    .map(|worker| worker.caller_prepare_ns)
                    .sum::<u64>()
            );
            if let Some(expected) = &expected {
                assert_eq!(&output, expected);
            } else {
                expected = Some(output);
            }
        }
    }

    #[test]
    fn empty_selection_starts_no_workers_and_completes_terminally() {
        let outcome = execute_parallel_prepare(
            tasks(0),
            report(8, 0),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 8,
            },
            read,
            |_, _| Ok(0usize),
            |_, _| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect("empty preparation");
        assert_eq!(outcome.started_workers, 0);
        assert_eq!(outcome.cursor_report.base_report.effective_threads, 0);
        assert_eq!(outcome.attribution.effective_threads, 0);
        assert_eq!(outcome.row_groups_queued, 0);
        assert!(outcome.ordered_terminal_completed);
    }

    #[test]
    fn out_of_order_publication_waits_for_ordered_frontier() {
        let published = Arc::new((Mutex::new(BTreeSet::new()), Condvar::new()));
        let before_state = Arc::clone(&published);
        let after_state = Arc::clone(&published);
        let mut committed = Vec::new();
        let outcome = execute_parallel_prepare_with_runtime(
            tasks(4),
            report(4, 4),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            ParallelPrepareRuntimeOptions::with_queue_capacities(4, 4),
            read,
            |context, _| Ok(context.selected_row_group_ordinal),
            |context, prepared| {
                assert_eq!(context.selected_row_group_ordinal, prepared);
                committed.push(prepared);
                Ok(ColumnBundleVisitControl::Continue)
            },
            move |context| {
                if context.selected_row_group_ordinal == 0 {
                    let (lock, ready) = &*before_state;
                    let mut published = lock_unpoisoned(lock);
                    while !(1..4).all(|ordinal| published.contains(&ordinal)) {
                        published = wait_unpoisoned(ready, published);
                    }
                }
            },
            move |context| {
                let (lock, ready) = &*after_state;
                lock_unpoisoned(lock).insert(context.selected_row_group_ordinal);
                ready.notify_all();
            },
        )
        .expect("out-of-order preparation");
        assert_eq!(committed, vec![0, 1, 2, 3]);
        assert_eq!(outcome.max_pending_results_observed, 4);
        assert!(outcome.ordered_frontier_wait_count > 0);
        assert!(outcome.ordered_terminal_completed);
    }

    #[test]
    fn multiple_failures_return_earliest_selected_ordinal() {
        let published = Arc::new((Mutex::new(BTreeSet::new()), Condvar::new()));
        let before_state = Arc::clone(&published);
        let after_state = Arc::clone(&published);
        let mut committed = Vec::new();
        let error = execute_parallel_prepare_with_runtime(
            tasks(4),
            report(4, 4),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            ParallelPrepareRuntimeOptions::with_queue_capacities(4, 4),
            read,
            |context, _| match context.selected_row_group_ordinal {
                1 => Err(ArcadiaTioError::InvalidArgument("ordinal one failed")),
                3 => Err(ArcadiaTioError::InvalidArgument("ordinal three failed")),
                ordinal => Ok(ordinal),
            },
            |_, prepared| {
                committed.push(prepared);
                Ok(ColumnBundleVisitControl::Continue)
            },
            move |context| {
                let required = match context.selected_row_group_ordinal {
                    0 => Some(1),
                    1 => Some(3),
                    _ => None,
                };
                if let Some(required) = required {
                    let (lock, ready) = &*before_state;
                    let mut published = lock_unpoisoned(lock);
                    while !published.contains(&required) {
                        published = wait_unpoisoned(ready, published);
                    }
                }
            },
            move |context| {
                let (lock, ready) = &*after_state;
                lock_unpoisoned(lock).insert(context.selected_row_group_ordinal);
                ready.notify_all();
            },
        )
        .expect_err("worker failures must fail");
        assert!(error.to_string().contains("ordinal one failed"));
        assert_eq!(committed, vec![0]);
    }

    #[test]
    fn later_failure_requires_discarding_the_partial_ordered_stage() {
        let mut staged = Vec::new();
        let result = execute_parallel_prepare(
            tasks(4),
            report(4, 4),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            read,
            |context, _| {
                if context.selected_row_group_ordinal == 2 {
                    Err(ArcadiaTioError::InvalidArgument("ordinal two failed"))
                } else {
                    Ok(context.selected_row_group_ordinal)
                }
            },
            |_, prepared| {
                staged.push(prepared);
                Ok(ColumnBundleVisitControl::Continue)
            },
        );

        let published = result
            .as_ref()
            .ok()
            .filter(|report| report.ordered_terminal_completed)
            .map(|_| staged.clone())
            .unwrap_or_default();
        let error = result.expect_err("later preparation failure must fail the call");
        assert!(error.to_string().contains("ordinal two failed"));
        assert_eq!(staged, vec![0, 1]);
        assert!(published.is_empty());
    }

    #[test]
    fn global_in_flight_cap_is_reached_without_being_exceeded() {
        let first_wave = Arc::new(Barrier::new(3));
        let prepare_barrier = Arc::clone(&first_wave);
        let outcome = execute_parallel_prepare(
            tasks(8),
            report(4, 8),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 3,
            },
            read,
            move |context, _| {
                if context.selected_row_group_ordinal < 3 {
                    prepare_barrier.wait();
                }
                Ok(context.selected_row_group_ordinal)
            },
            |_, _| Ok(ColumnBundleVisitControl::Continue),
        )
        .expect("bounded prepare");
        assert_eq!(outcome.started_workers, 3);
        assert_eq!(outcome.cursor_report.base_report.effective_threads, 3);
        assert_eq!(outcome.attribution.effective_threads, 3);
        assert_eq!(outcome.max_active_workers_observed, 3);
        assert_eq!(outcome.max_in_flight_row_groups_observed, 3);
        assert!(outcome.max_pending_results_observed <= 3);
        assert!(outcome.max_pending_rows_observed <= 6);
        assert!(outcome.capacity_wait_count > 0);
    }

    #[test]
    fn task_queue_saturation_is_bounded_and_drains_in_order() {
        let queue = TaskQueue::new(1);
        let cancelled = AtomicBool::new(false);
        let task_specs = tasks(2);

        assert!(matches!(queue.try_push(task_specs[0]), TaskPush::Pushed));
        assert!(matches!(queue.try_push(task_specs[1]), TaskPush::Full));
        assert_eq!(
            queue
                .pop(&cancelled)
                .expect("first bounded task")
                .selected_row_group_ordinal,
            0
        );
        assert!(matches!(queue.try_push(task_specs[1]), TaskPush::Pushed));
        queue.close();
        assert_eq!(
            queue
                .pop(&cancelled)
                .expect("queued task remains drainable after close")
                .selected_row_group_ordinal,
            1
        );
        assert!(queue.pop(&cancelled).is_none());
    }

    #[test]
    fn bounded_queues_terminate_under_result_backpressure() {
        let first_wave = Arc::new(Barrier::new(4));
        let prepare_barrier = Arc::clone(&first_wave);
        let published = Arc::new((Mutex::new(0usize), Condvar::new()));
        let publish_state = Arc::clone(&published);
        let commit_state = Arc::clone(&published);
        let outcome = execute_parallel_prepare_with_runtime(
            tasks(8),
            report(4, 8),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            ParallelPrepareRuntimeOptions::with_queue_capacities(1, 1),
            read,
            move |context, _| {
                if context.selected_row_group_ordinal < 4 {
                    prepare_barrier.wait();
                }
                Ok(context.selected_row_group_ordinal)
            },
            move |context, _| {
                if context.selected_row_group_ordinal == 0 {
                    let (lock, ready) = &*commit_state;
                    let mut count = lock_unpoisoned(lock);
                    while *count < 2 {
                        count = wait_unpoisoned(ready, count);
                    }
                }
                Ok(ColumnBundleVisitControl::Continue)
            },
            |_| {},
            move |_| {
                let (lock, ready) = &*publish_state;
                let mut count = lock_unpoisoned(lock);
                *count = count.saturating_add(1);
                ready.notify_all();
            },
        )
        .expect("backpressured prepare");
        assert!(outcome.result_queue_full_wait_count > 0);
        assert!(outcome.capacity_wait_count > 0);
        assert_eq!(outcome.row_groups_ordered_committed, 8);
        assert!(outcome.ordered_terminal_completed);
    }

    #[test]
    fn stop_is_coherent_and_never_terminal() {
        let mut staged = Vec::new();
        let outcome = execute_parallel_prepare(
            tasks(8),
            report(4, 8),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            read,
            |context, _| Ok(context.selected_row_group_ordinal),
            |context, prepared| {
                assert_eq!(context.selected_row_group_ordinal, 0);
                staged.push(prepared);
                Ok(ColumnBundleVisitControl::Stop)
            },
        )
        .expect("stopped prepare");
        let published = if outcome.ordered_terminal_completed {
            staged.clone()
        } else {
            Vec::new()
        };
        assert!(outcome.cursor_report.cancelled);
        assert!(!outcome.ordered_terminal_completed);
        assert_eq!(staged, vec![0]);
        assert!(published.is_empty());
        assert_eq!(outcome.row_groups_ordered_committed, 1);
        assert_eq!(outcome.rows_ordered_committed, 2);
        assert!(outcome.row_groups_ordered_committed <= outcome.row_groups_completed);
        assert!(outcome.row_groups_completed <= outcome.row_groups_queued);
        assert!(outcome.row_groups_queued <= 8);
        assert_eq!(
            outcome
                .worker_reports
                .iter()
                .map(|worker| worker.row_groups_completed)
                .sum::<usize>(),
            outcome.row_groups_completed
        );
        assert!(outcome.max_in_flight_row_groups_observed <= 4);
        assert!(outcome.max_pending_results_observed <= 4);
    }

    #[test]
    fn ordered_commit_panic_returns_error_without_deadlock() {
        let error = execute_parallel_prepare(
            tasks(8),
            report(4, 8),
            0,
            ColumnBundleParallelPrepareOptions {
                max_in_flight_row_groups: 4,
            },
            read,
            |context, _| Ok(context.selected_row_group_ordinal),
            |_, _| -> Result<ColumnBundleVisitControl> { panic!("injected ordered commit panic") },
        )
        .expect_err("ordered commit panic must become an error");
        assert!(error.to_string().contains("ordered commit panicked"));
    }
}
