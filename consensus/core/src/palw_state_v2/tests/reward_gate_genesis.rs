//! The BASE_FLOOR exemption must hold in registration as well as activation.
use super::*;
use crate::palw_kernel_route_v1::{PalwKernelOpvExtrasV1, PalwKernelRouteExtrasV1, PalwRewardGateTermsV1};
use crate::palw_panel_free_v1::PalwPanelFreeFenceV1;

#[test]
fn opv_at_genesis_keeps_the_floor_active_and_other_classes_behind_the_actual_reward_gate() {
    let p = params();
    let policy = PalwPanelFreeFenceV1::at(crate::config::params::ForkActivation::new(0));
    let extras = PalwTransitionExtrasV1 {
        kernel_route: Some(PalwKernelRouteExtrasV1 {
            network_domain: h64(10),
            ruleset_digest: h64(11),
            chain_genesis: h64(12),
            opv: Some(PalwKernelOpvExtrasV1 {
                policy: policy.opv_policy(),
                denied_classes: Vec::new(),
                min_effective_bits: 128,
                sampled_conformance_gates_reward: false,
                test_eligible: Vec::new(),
                reward_gate: Some(PalwRewardGateTermsV1 { fence_activation_daa: 0 }),
                budget_required: false,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut other = registration(h64(2), 1, None);
    let PalwConsensusObjectV2::ClassRegistered { artifact_root, .. } = &mut other else { unreachable!() };
    *artifact_root = h64(0xA2);
    let (state, _) = apply_palw_transition_v2_with_extras(
        &PalwChainStateV2::genesis(),
        &p,
        &ctx(1, 0, 0),
        &[registration(p.base_class_id, 1000, None), other],
        None,
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("a reward-gated genesis must actually boot its liveness floor");
    assert!(matches!(state.class(&p.base_class_id).unwrap().status, PalwClassStatusV2::Active));
    assert_eq!(state.class_shares.get(&p.base_class_id), Some(&1000));
    assert!(matches!(state.class(&h64(2)).unwrap().status, PalwClassStatusV2::Registered { .. }));
    assert!(!state.class_shares.contains_key(&h64(2)), "no unverified useful class gets an exemption");
    state.assert_internal_consistency(&p).unwrap();
}
