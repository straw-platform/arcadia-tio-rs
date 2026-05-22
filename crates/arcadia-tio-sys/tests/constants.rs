use std::ffi::c_int;
use std::mem::{align_of, size_of};

use arcadia_tio_sys::*;

const _: () = {
    assert!(size_of::<ArcadiaTioDType>() == size_of::<c_int>());
    assert!(size_of::<ArcadiaTioErrorCode>() == size_of::<c_int>());
    assert!(ARCADIA_TIO_DTYPE_F32 == 0);
    assert!(ARCADIA_TIO_DTYPE_F64 == 1);
    assert!(ARCADIA_TIO_DTYPE_I32 == 2);
    assert!(ARCADIA_TIO_DTYPE_I64 == 3);
    assert!(ARCADIA_TIO_ERROR_OK == 0);
    assert!(ARCADIA_TIO_AXIS_TIME == 0);
    assert!(ARCADIA_TIO_AXIS_SYMBOL == 1);
    assert!(ARCADIA_TIO_COORDINATE_DTYPE_I32 == 0);
    assert!(ARCADIA_TIO_COORDINATE_KIND_DATE == 2);
    assert!(ARCADIA_TIO_COORDINATE_ENCODING_DATE_YYYYMMDD == 2);
    assert!(ARCADIA_TIO_COORDINATE_STORAGE_INLINE == 0);
    assert!(ARCADIA_TIO_HEADER_PROFILE_STREAMING == 0);
    assert!(ARCADIA_TIO_READ_EXECUTION_SERIAL == 0);
    assert!(ARCADIA_TIO_READ_EXECUTION_PARALLEL_THREADS == 1);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_FILE_ENVELOPE == 0);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE == 6);
    assert!(ARCADIA_TIO_READ_SHAPE_POLICY_EXPLICIT_UNIVERSE_AND_EXTENTS == 7);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_EXTENT_ONLY == 0);
    assert!(ARCADIA_TIO_AXIS_IDENTITY_UNIVERSE_AWARE == 1);
    assert!(ARCADIA_TIO_HISTORICAL_QUERY_SOURCE_RETAINED_VISIBLE_COMMIT == 0);
    assert!(ARCADIA_TIO_STORAGE_BALANCED == 0);
    assert!(ARCADIA_TIO_STORAGE_ACCESS_REMOTE_RANGE_READ == 1);
    assert!(ARCADIA_TIO_OPEN_PATTERN_METADATA_HOT == 0);
    assert!(ARCADIA_TIO_FILE_POPULATION_FEW_LONG_LIVED == 0);
    assert!(ARCADIA_TIO_METADATA_STABILITY_STABLE == 0);
};

#[test]
fn representative_raw_layouts_are_pointer_compatible() {
    assert_eq!(align_of::<ArcadiaTioTensor>(), align_of::<usize>());
    assert_eq!(
        align_of::<ArcadiaTioAxisCoordinateInput>(),
        align_of::<usize>()
    );

    #[cfg(target_pointer_width = "64")]
    {
        assert_eq!(size_of::<ArcadiaTioAxisCoordinateInput>(), 120);
        assert_eq!(size_of::<ArcadiaTioReadShapePolicyOptions>(), 72);
        assert_eq!(size_of::<ArcadiaTioReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioHistoricalReadWithOptionsOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioReadWithShapePolicyOptions>(), 104);
        assert_eq!(
            size_of::<ArcadiaTioHistoricalReadWithShapePolicyOptions>(),
            104
        );
        assert_eq!(size_of::<ArcadiaTioCreateWithUniverseOptions>(), 32);
        assert_eq!(size_of::<ArcadiaTioAppendWithUniverseOptions>(), 48);
    }

    #[cfg(target_pointer_width = "32")]
    assert!(size_of::<ArcadiaTioAxisCoordinateInput>() >= 72);
}
