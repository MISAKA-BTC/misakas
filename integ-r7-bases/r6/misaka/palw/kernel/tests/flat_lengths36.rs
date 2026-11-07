//! Versioned length contract36 (unverified source checkpoint, 2026-10-15): a length
//! change is a new version; v1 keeps its frozen bound; v8 windows bind their plan;
//! RFC03 media axes decode but stay refused until their own fence.
use misaka_palw_kernel::{
    flat_history8::{HistoryNodeV8, HistorySelectionPlanV8},
    flat_lengths7::{FlatJobLengthsV7, FlatLengthAxisV7},
    flat_lengths36::*,
};
use misaka_palw_tir::{DType, program::HISTORY_BOUND_V1_SMALL};

fn token(maximum: u32) -> ModalityLengthV36 {
    ModalityLengthV36 { axis: FlatLengthAxisV7::Token, maximum, unit_bytes: 4, profile: [0; 64] }
}
fn plan(positions: u32) -> HistorySelectionPlanV8 {
    HistorySelectionPlanV8 {
        version: 8,
        graph_root: [23; 64],
        source: HistoryNodeV8 { occurrence: 0, node: 0, slot: 0 },
        output: HistoryNodeV8 { occurrence: 0, node: 1, slot: 1 },
        nodes_per_position: 2,
        dtype: DType::I32,
        row_shape: vec![2],
        window: 4096,
        selection_cap: 64,
        max_positions: positions,
    }
}
fn v1(maximum_positions: u32) -> LengthContractV36 {
    LengthContractV36 {
        version: 36,
        program_root: [1; 64],
        rule: HistoryRuleV36::FrozenV1 { history_bound: HISTORY_BOUND_V1_SMALL },
        maximum_positions,
        token: token(maximum_positions),
        patch: None,
        audio: None,
    }
}

#[test]
fn v1_keeps_its_frozen_history_bound_and_actual_lengths_cannot_grow() {
    let c = v1(1024).check(HISTORY_BOUND_V1_SMALL, None).unwrap();
    assert!(c.check_actual(FlatJobLengthsV7 { token: 1000, patch: 0, audio: 0 }, 1000).is_ok());
    assert!(c.check_actual(FlatJobLengthsV7 { token: 1025, patch: 0, audio: 0 }, 1025).is_err());
    assert!(c.check_actual(FlatJobLengthsV7 { token: 10, patch: 1, audio: 0 }, 10).is_err(), "absent axis stays zero");
    // Raising positions past the frozen v1 bound is not a v1 contract.
    assert!(v1(HISTORY_BOUND_V1_SMALL + 1).check(HISTORY_BOUND_V1_SMALL, None).is_err());
    // A different program bound is a different contract, not a reinterpretation.
    assert!(v1(1024).check(HISTORY_BOUND_V1_SMALL * 2, None).is_err());
}

#[test]
fn window8_binds_its_plan_and_derives_cost_at_the_requested_length() {
    let positions = 1 << 20;
    let p = plan(positions);
    let mut c = v1(positions);
    c.rule = HistoryRuleV36::Window8 { plan: p.root(), window: 4096, selection_cap: 64 };
    let checked = c.check(HISTORY_BOUND_V1_SMALL, Some(&p)).unwrap();
    assert!(checked.history_scope().is_some());
    let mut other = p.clone();
    other.window = 2048;
    assert!(c.check(HISTORY_BOUND_V1_SMALL, Some(&other)).is_err(), "plan root binds the window");
    assert!(c.check(HISTORY_BOUND_V1_SMALL, None).is_err());
}

#[test]
fn media_axes_decode_but_are_refused_until_their_fence() {
    let mut c = v1(1024);
    c.patch = Some(ModalityLengthV36 { axis: FlatLengthAxisV7::Patch, maximum: 256, unit_bytes: 3 * 14 * 14, profile: [5; 64] });
    let bytes = borsh::to_vec(&c).unwrap();
    assert_eq!(LengthContractV36::from_bytes(&bytes).unwrap(), c);
    assert!(!MEDIA_LENGTHS_ARMABLE_V36);
    assert!(c.check(HISTORY_BOUND_V1_SMALL, None).is_err());
    assert!(LengthContractV36::from_bytes(&vec![0; LENGTH_CONTRACT_MAX_BYTES_V36 + 1]).is_err());
}
