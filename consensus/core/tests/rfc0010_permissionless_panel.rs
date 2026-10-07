use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::{
    Hash64,
    config::params::{
        ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
        palw_t12_shipped_params,
    },
    palw_permissionless_panel_v1::*,
};

fn h(i: u64) -> Hash64 {
    Hash64::from_u64_word(i)
}
fn policy() -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 10,
        beacon_wait_daa: 2,
        assignment_delay_daa: 1,
        receipt_window_daa: 3,
        seat_count: 2,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 100,
        max_candidates: 16,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 100,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 100,
        beacon_scheme: h(777),
    }
}

#[test]
fn rfc0010_all_presets_are_dormant_and_every_real_activation_is_refused() {
    for p in [
        mainnet_shipped_params(),
        devnet_shipped_params(),
        palw_rc_shipped_params(),
        palw_t12_shipped_params(),
        Params::from(TESTNET_PARAMS.net),
        Params::from(SIMNET_PARAMS.net),
    ] {
        assert!(p.palw_permissionless_panel_v1.is_none());
        p.validate_palw_permissionless_panel_v1().unwrap();
        for at in [0, 900, 1234, u64::MAX - 1] {
            let mut armed = p.clone();
            armed.palw_permissionless_panel_v1 =
                Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(at), policy: policy() });
            assert!(armed.validate_palw_permissionless_panel_v1().is_err());
            assert!(armed.validate_palw_v2().is_err());
        }
    }
}

#[test]
fn rfc0010_dormant_and_never_leave_identities_and_fork_ids_unchanged() {
    let p = palw_t12_shipped_params();
    let mut never = p.clone();
    never.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::never(), policy: policy() });
    assert_eq!(p.consensus_params_id(), never.consensus_params_id());
    assert_eq!(p.consensus_schedule_id(), never.consensus_schedule_id());
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
    for at in [0, 750, 1234, u64::MAX - 1] {
        assert_eq!(fork_id_v1(&p, at), fork_id_v1(&never, at));
    }
}

#[test]
fn rfc0010_policy_values_and_height_are_committed_and_future_fork_is_explicit() {
    let p = palw_t12_shipped_params();
    let mut armed = p.clone();
    armed.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(1234), policy: policy() });
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
    assert_eq!(p.consensus_identity_id(), armed.consensus_identity_id());
    let before = fork_id_v1(&p, 1233);
    let future = fork_id_v1(&armed, 1233);
    assert_eq!(before.fired, future.fired);
    assert!(!evaluate_fork_id_v1(&armed, 1233, &before.fired.as_bytes(), before.next).refuses());
    assert!(!evaluate_fork_id_v1(&p, 1233, &future.fired.as_bytes(), future.next).refuses());
    let unupgraded = fork_id_v1(&p, 1234);
    assert!(evaluate_fork_id_v1(&armed, 1234, &unupgraded.fired.as_bytes(), unupgraded.next).refuses());
    let mut changed = armed.clone();
    changed.palw_permissionless_panel_v1.as_mut().unwrap().policy.receipt_window_daa += 1;
    assert_ne!(armed.consensus_params_id(), changed.consensus_params_id());
    assert_ne!(armed.consensus_schedule_id(), changed.consensus_schedule_id());
}

#[test]
fn rfc0010_old_claims_keep_lane_a_after_the_fence_and_retry() {
    let f = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(1234), policy: policy() });
    assert_eq!(panel_claim_rule_v1(f, 1233), PanelClaimRuleV1::HistoricalLaneA);
    assert_eq!(panel_claim_rule_v1(f, 1234), PanelClaimRuleV1::PermissionlessV3);
    assert_eq!(panel_claim_rule_v1(None, u64::MAX - 1), PanelClaimRuleV1::HistoricalLaneA);
}

#[test]
fn rfc0010_explicit_carrier_must_commit_every_derived_binding() {
    let mut events = PanelFoldEventsV1::default();
    let b = PanelBoundV3 {
        claim_seal_id: h(1),
        panel_snapshot_root: h(2),
        beacon_id: h(3),
        panel_seed_v3: h(4),
        entropy_ready_daa: 10,
        assignment_point: 11,
        binding_block: h(5),
        bound_daa: 11,
        retry_index: 0,
        seats: vec![BondIdV1 { transaction: h(6), index: 0 }],
        exposure: 100,
    };
    events.bindings.insert(h(7), b);
    validate_panel_binding_facts_v3(&events, &events.bindings).unwrap();
    assert!(validate_panel_binding_facts_v3(&events, &Default::default()).is_err());
    let mut wrong = events.bindings.clone();
    wrong.get_mut(&h(7)).unwrap().exposure += 1;
    assert!(validate_panel_binding_facts_v3(&events, &wrong).is_err());
}
