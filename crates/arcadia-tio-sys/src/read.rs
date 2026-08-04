use super::*;

unsafe extern "C" {
    /// Reads an axis range into an owned tensor.
    pub fn arcadia_tio_read_axis_range(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        start: u32,
        end: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads an axis take selection into an owned tensor.
    pub fn arcadia_tio_read_axis_take(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        indices: *const u32,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads one axis index into an owned tensor.
    pub fn arcadia_tio_read_axis_one(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        index: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads an append-entry range into an owned tensor.
    pub fn arcadia_tio_read_entry_range(
        handle: *mut ArcadiaTioHandle,
        start: u32,
        end: u32,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Takes append entries into an owned tensor.
    pub fn arcadia_tio_take_entries(
        handle: *mut ArcadiaTioHandle,
        indices: *const u32,
        indices_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads one scalar value.
    pub fn arcadia_tio_read_scalar(
        handle: *mut ArcadiaTioHandle,
        indices: *const u32,
        indices_len: usize,
        out_value: *mut ArcadiaTioScalar,
    ) -> c_int;
    /// Reads selector data at a commit into an owned tensor.
    pub fn arcadia_tio_read_at_commit(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
    ) -> c_int;
    /// Reads selector data at a commit into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
    ) -> c_int;
    /// Frees native-owned strings in a current read execution report.
    pub fn arcadia_tio_read_execution_report_free(report: *mut ArcadiaTioReadExecutionReport);
    /// Frees owned fields in a current Coordinate v2 read result.
    pub fn arcadia_tio_coordinate_read_result_v2_free(
        result: *mut ArcadiaTioCoordinateReadResultV2,
    );
    /// Frees owned fields in a current dense Coordinate v2 read result.
    pub fn arcadia_tio_coordinate_dense_read_result_v2_free(
        result: *mut ArcadiaTioCoordinateDenseReadResultV2,
    );
    /// Frees native-owned JSON strings in an attributed query trace.
    pub fn arcadia_tio_query_trace_json_free(trace_json: *mut ArcadiaTioQueryTraceJson);
    /// Frees native-owned strings in a historical read execution report.
    pub fn arcadia_tio_historical_read_execution_report_free(
        report: *mut ArcadiaTioHistoricalReadExecutionReport,
    );
    /// Frees owned fields in a historical Coordinate v2 read result.
    pub fn arcadia_tio_historical_coordinate_read_result_v2_free(
        result: *mut ArcadiaTioHistoricalCoordinateReadResultV2,
    );
    /// Frees owned fields in a historical dense Coordinate v2 read result.
    pub fn arcadia_tio_historical_coordinate_dense_read_result_v2_free(
        result: *mut ArcadiaTioHistoricalCoordinateDenseReadResultV2,
    );
    /// Frees native-owned strings in a historical read-index report.
    pub fn arcadia_tio_historical_read_index_report_free(
        report: *mut ArcadiaTioHistoricalReadIndexReport,
    );
    /// Frees native-owned strings in a read-index report.
    pub fn arcadia_tio_read_index_report_free(report: *mut ArcadiaTioReadIndexReport);
    /// Reads data through low-level read-index items into an owned tensor.
    pub fn arcadia_tio_read_index(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads data through low-level read-index items into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_dense(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads current data through low-level read-index items with a shape-policy domain.
    pub fn arcadia_tio_read_index_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads current data through low-level read-index items with a shape-policy domain into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items with execution options.
    pub fn arcadia_tio_read_index_at_commit_with_options(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_at_commit_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items with a shape-policy domain.
    pub fn arcadia_tio_read_index_at_commit_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
    ) -> c_int;
    /// Reads historical data through low-level read-index items with a shape-policy domain into a dense tensor and optional mask.
    pub fn arcadia_tio_read_index_at_commit_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        items: *const ArcadiaTioReadIndexItem,
        items_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadIndexReport,
    ) -> c_int;
    /// Reads current selector data with execution options into an owned tensor.
    pub fn arcadia_tio_read_with_options(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with execution options into a dense tensor and optional mask.
    pub fn arcadia_tio_read_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 exact lookup and reads the matching axis slice.
    pub fn arcadia_tio_read_at_coordinate_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithOptionsOptions,
        out_result: *mut ArcadiaTioCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 exact lookup and densely reads the matching axis slice.
    pub fn arcadia_tio_read_at_coordinate_v2_dense(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithOptionsOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 range lookup and reads the matching axis range.
    pub fn arcadia_tio_read_coordinate_range_v2(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithOptionsOptions,
        out_result: *mut ArcadiaTioCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 range lookup and densely reads the matching axis range.
    pub fn arcadia_tio_read_coordinate_range_v2_dense(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithOptionsOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 exact lookup and reads the matching axis slice with shape policy.
    pub fn arcadia_tio_read_at_coordinate_v2_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithShapePolicyOptions,
        out_result: *mut ArcadiaTioCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 exact lookup and densely reads the matching axis slice with shape policy.
    pub fn arcadia_tio_read_at_coordinate_v2_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithShapePolicyOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 range lookup and reads the matching axis range with shape policy.
    pub fn arcadia_tio_read_coordinate_range_v2_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithShapePolicyOptions,
        out_result: *mut ArcadiaTioCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a current-head Coordinate v2 range lookup and densely reads the matching axis range with shape policy.
    pub fn arcadia_tio_read_coordinate_range_v2_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioReadWithShapePolicyOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Reads current selector data with execution options and query attribution.
    pub fn arcadia_tio_read_with_options_attributed(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        trace_context: *const ArcadiaTioQueryTraceContext,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
        out_trace_json: *mut ArcadiaTioQueryTraceJson,
    ) -> c_int;
    /// Reads current dense selector data with execution options and query attribution.
    pub fn arcadia_tio_read_with_options_dense_attributed(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithOptionsOptions,
        trace_context: *const ArcadiaTioQueryTraceContext,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
        out_trace_json: *mut ArcadiaTioQueryTraceJson,
    ) -> c_int;
    /// Reads historical selector data with execution options into an owned tensor.
    pub fn arcadia_tio_read_at_commit_with_options(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with execution options into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_with_options_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Performs a historical Coordinate v2 exact lookup and reads the matching axis slice.
    pub fn arcadia_tio_read_at_coordinate_at_commit_v2(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_result: *mut ArcadiaTioHistoricalCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 exact lookup and densely reads the matching axis slice.
    pub fn arcadia_tio_read_at_coordinate_at_commit_v2_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioHistoricalCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 range lookup and reads the matching axis range.
    pub fn arcadia_tio_read_coordinate_range_at_commit_v2(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        out_result: *mut ArcadiaTioHistoricalCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 range lookup and densely reads the matching axis range.
    pub fn arcadia_tio_read_coordinate_range_at_commit_v2_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithOptionsOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioHistoricalCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 exact lookup and reads the matching axis slice with shape policy.
    pub fn arcadia_tio_read_at_coordinate_at_commit_v2_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        out_result: *mut ArcadiaTioHistoricalCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 exact lookup and densely reads the matching axis slice with shape policy.
    pub fn arcadia_tio_read_at_coordinate_at_commit_v2_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        key: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioHistoricalCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 range lookup and reads the matching axis range with shape policy.
    pub fn arcadia_tio_read_coordinate_range_at_commit_v2_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        out_result: *mut ArcadiaTioHistoricalCoordinateReadResultV2,
    ) -> c_int;
    /// Performs a historical Coordinate v2 range lookup and densely reads the matching axis range with shape policy.
    pub fn arcadia_tio_read_coordinate_range_at_commit_v2_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        axis: usize,
        lower: *const ArcadiaTioCoordinateLookupKeyV2,
        upper: *const ArcadiaTioCoordinateLookupKeyV2,
        coordinate_options: *const ArcadiaTioCoordinateV2Options,
        read_options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        fill_value: c_double,
        out_result: *mut ArcadiaTioHistoricalCoordinateDenseReadResultV2,
    ) -> c_int;
    /// Reads current selector data with a shape policy into an owned tensor.
    pub fn arcadia_tio_read_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads current selector data with a shape policy into a dense tensor and optional mask.
    pub fn arcadia_tio_read_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with a shape policy into an owned tensor.
    pub fn arcadia_tio_read_at_commit_with_shape_policy(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        out_tensor: *mut ArcadiaTioTensor,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
    /// Reads historical selector data with a shape policy into a dense tensor and optional mask.
    pub fn arcadia_tio_read_at_commit_with_shape_policy_dense(
        handle: *mut ArcadiaTioHandle,
        commit_seq: u64,
        selectors: *const ArcadiaTioEntrySelector,
        selectors_len: usize,
        options: *const ArcadiaTioHistoricalReadWithShapePolicyOptions,
        fill_value: c_double,
        out_tensor: *mut ArcadiaTioTensor,
        out_mask: *mut ArcadiaTioMask,
        out_report: *mut ArcadiaTioHistoricalReadExecutionReport,
    ) -> c_int;
}
