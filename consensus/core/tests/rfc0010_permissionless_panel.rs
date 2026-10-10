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
fn rfc0010_an_uncommitted_or_stale_state_mirror_is_refused() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let mut p = palw_t12_shipped_params();
    p.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(1234), policy: policy() });
    p.sync_palw_permissionless_panel_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
    let mirror = *bundle.state.panel_v3().unwrap();
    assert!(p.validate_palw_permissionless_panel_v1().unwrap_err().to_string().contains("no certified Panel beacon"));
    p.sync_palw_permissionless_panel_v1();
    assert!(p.validate_palw_permissionless_panel_v1().unwrap_err().to_string().contains("no certified Panel beacon"));

    // The actual processor reads the mirror even if no top-level fence commits it.
    let mut orphan = p.clone();
    orphan.palw_permissionless_panel_v1 = None;
    assert!(orphan.validate_palw_permissionless_panel_v1().unwrap_err().to_string().contains("state mirror"));
    assert!(orphan.validate_palw_v2().is_err());
    let mut never = p.clone();
    never.palw_permissionless_panel_v1.as_mut().unwrap().activation = ForkActivation::never();
    assert!(never.validate_palw_permissionless_panel_v1().unwrap_err().to_string().contains("state mirror"));

    for field in 0..4 {
        let mut altered = p.clone();
        let PalwConsensusMode::ConsensusV2(bundle) = &mut altered.palw_consensus_mode else { unreachable!() };
        let mut changed = mirror;
        match field {
            0 => changed.from_daa += 1,
            1 => changed.policy.receipt_window_daa += 1,
            2 => changed.network = h(1),
            _ => changed.ruleset = h(2),
        }
        bundle.state = bundle.state.clone().with_panel_v3(Some(changed));
        assert!(altered.validate_palw_permissionless_panel_v1().unwrap_err().to_string().contains("state mirror"));
        assert!(altered.validate_palw_v2().is_err());
    }
    orphan.sync_palw_permissionless_panel_v1();
    orphan.validate_palw_permissionless_panel_v1().unwrap();
    assert_eq!(orphan.consensus_params_id(), palw_t12_shipped_params().consensus_params_id());
    assert_eq!(orphan.consensus_schedule_id(), palw_t12_shipped_params().consensus_schedule_id());
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

#[test]
fn rfc0010_observation_distinguishes_a_dormant_engine_from_release_support() {
    let p = palw_t12_shipped_params();
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("V2") };
    let state = kaspa_consensus_core::palw_state_v2::PalwChainStateV2::genesis();
    let observation = panel_v3_observation_v1(&state, &bundle.state, &[h(1)], 0);
    assert!(!observation.overview.active);
    assert!(!observation.release.activation_supported);
    assert!(observation.release.approved_beacon_schemes.is_empty());
    assert_eq!(observation.unknown, vec![h(1)]);
    let json = serde_json::to_value(observation).unwrap();
    assert_eq!(json["release"]["activationSupported"], false);
    assert_eq!(json["release"]["approvedBeaconSchemes"], serde_json::json!([]));
}
