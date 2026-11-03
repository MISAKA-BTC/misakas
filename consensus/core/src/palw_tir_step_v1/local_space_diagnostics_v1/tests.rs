//! Actual constructor errors, not a second spelling of the census formula.
use super::*;
use crate as geometry_core;
#[path = "../../../tests/common/local_step_geometry_fixture_v1.rs"]
mod fixture;

#[test]
fn constructor_diagnostics_cover_actual_narrow_and_wide_error_allocations() {
    let q = quote_local_tir_step_space_fixed_requests_v1().unwrap();
    for wide in [false, true] {
        for invalid in 0..4 {
            let mut class = fixture::class(wide, 3);
            match invalid {
                0 => class.layout.version = u16::MAX,
                1 => class.layout.commit_tiles[0] = u32::MAX,
                2 => class.layout.state_tiles[0] = u32::MAX,
                3 => class.layout.state_tiles[1] = u32::MAX,
                _ => unreachable!(),
            }
            // Count the original Result root conservatively alongside the actual
            // retained error String. No ingress/authority or execution implied.
            let (error, result_root) = if wide {
                let result = PalwTirStepSpaceV2::new(&class);
                (result.unwrap_err(), size_of::<Result<PalwTirStepSpaceV2, PalwTirStepError>>())
            } else {
                let result = PalwTirStepSpaceV1::new(&class);
                (result.unwrap_err(), size_of::<Result<PalwTirStepSpaceV1, PalwTirStepError>>())
            };
            let PalwTirStepError::Layout(ref text) = error else { panic!("fixture must reach the existing layout error") };
            let observed = (size_of::<PalwTirStepError>() + result_root + text.capacity()) as u64;
            assert!(observed <= q.constructor_diagnostic_bytes, "actual constructor diagnostic exceeds its named quote");
        }
    }
}

#[test]
fn private_native_coverage_diagnostics_fit_without_sdk_type_name_copies() {
    let q = quote_local_tir_step_space_fixed_requests_v1().unwrap();
    // Exercise the actual existing private helper failure, with a cardinality
    // deliberately outside admitted Source. This tests diagnostic storage only.
    let error = super::super::step_space_requests_from_counts_v1(u64::MAX, u64::MAX, u64::MAX, 1, 1, 1, 1).unwrap_err();
    let NativeCoverageV1::Overflow { component, native_type } = &error else {
        panic!("expected an actual private overflow diagnostic")
    };
    let observed = (size_of::<NativeCoverageV1>() + component.capacity() + native_type.capacity()) as u64;
    assert!(observed <= q.coverage_diagnostic_bytes);
}
