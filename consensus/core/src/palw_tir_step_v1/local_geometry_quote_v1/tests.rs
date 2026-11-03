//! Actual metadata capacity regression, not a model-execution/host-lease test.
use super::quote_local_tir_step_space_additional_v1;
use crate::palw_tir_step_v1::{BlockSpaceV1, CommitNodeV1, PalwTirStateInstanceV1, PalwTirStepSpaceV1, PalwTirStepSpaceV2};
#[path = "../../../tests/common/local_step_geometry_fixture_v1.rs"]
mod fixture;
use crate as geometry_core;
#[test]
fn same_source_mixed_fixed_hist_and_repeated_layer_geometry_fits_actual_capacities() {
    for wide in [false, true] {
        for layers in [1usize, 3] {
            let class = fixture::class(wide, layers);
            let source = fixture::source(&class);
            let quote = quote_local_tir_step_space_additional_v1(&source).unwrap();
            if wide {
                let strict = PalwTirStepSpaceV2::new(&class).unwrap();
                assert_eq!(strict.program(), source.program());
                assert_eq!(strict.fixed_instances().len(), 1 + layers);
                assert_eq!(strict.hist_instances().len(), 1 + layers);
            } else {
                let strict = PalwTirStepSpaceV1::new(&class).unwrap();
                assert_eq!(&strict.program, source.program());
            }
            // Inspect EXACTLY the private shared geometry backend used by BOTH
            // constructors. This test-private call is not a V1 wide acceptance
            // route, execution grant or replacement ProgramInfo authority.
            let program = source.program().clone();
            let info = misaka_palw_tir::validate::validate(&program).unwrap();
            let space = PalwTirStepSpaceV1::from_program_geometry(program, info, class.layout.clone()).unwrap();
            assert_eq!(space.fixed_instances().len(), 1 + layers);
            assert_eq!(space.hist_instances().len(), 1 + layers);
            assert_eq!(space.occurrences().len(), layers + 2);
            let mut actual = space.blocks.capacity() * std::mem::size_of::<BlockSpaceV1>();
            actual += space.blocks.iter().map(|b| b.commits.capacity() * std::mem::size_of::<CommitNodeV1>()).sum::<usize>();
            actual += space.occurrences.capacity() * std::mem::size_of::<(u8, Option<u16>)>();
            actual += space.slot_bases.capacity() * std::mem::size_of::<u32>();
            actual += (space.fixed.capacity() + space.hist.capacity()) * std::mem::size_of::<PalwTirStateInstanceV1>();
            assert!(quote >= actual as u64, "wide={wide} layers={layers} quote={quote} actual={actual}");
        }
    }
}
