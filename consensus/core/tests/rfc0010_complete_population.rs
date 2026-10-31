//! Separate complete-population profile: all presets dormant, activation refused, identities versioned.
#![allow(warnings)]
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
        max_candidates: MAX_COMPLETE_CANDIDATES_V1,
        max_pending: 8,
        max_pending_per_bond: 4,
        max_assignments_per_block: 1,
        max_admissions_per_block: 1,
        max_tracked_claims: 100,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 100,
        beacon_scheme: h(777),
    }
}

#[test]
fn rfc0010_complete_all_presets_are_dormant_and_every_real_activation_is_refused() {
    for p in [
        mainnet_shipped_params(),
        devnet_shipped_params(),
        palw_rc_shipped_params(),
        palw_t12_shipped_params(),
        Params::from(TESTNET_PARAMS.net),
        Params::from(SIMNET_PARAMS.net),
    ] {
        assert!(p.palw_permissionless_panel_complete_v1.is_none());
        p.validate_palw_permissionless_panel_v1().unwrap();
        for at in [0, 900, 1234, u64::MAX - 1] {
            let mut armed = p.clone();
            armed.palw_permissionless_panel_complete_v1 =
                Some(PalwPermissionlessPanelCompleteV1 { activation: ForkActivation::new(at), policy: policy() });
            assert!(armed.validate_palw_permissionless_panel_v1().is_err());
            assert!(armed.validate_palw_v2().is_err());
        }
    }
}

#[test]
fn rfc0010_complete_dormant_and_never_leave_identities_and_fork_ids_unchanged() {
    let p = palw_t12_shipped_params();
    let mut never = p.clone();
    never.palw_permissionless_panel_complete_v1 =
        Some(PalwPermissionlessPanelCompleteV1 { activation: ForkActivation::never(), policy: policy() });
    assert_eq!(p.consensus_params_id(), never.consensus_params_id());
    assert_eq!(p.consensus_schedule_id(), never.consensus_schedule_id());
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
    for at in [0, 750, 1234, u64::MAX - 1] {
        assert_eq!(fork_id_v1(&p, at), fork_id_v1(&never, at));
    }
}

#[test]
fn rfc0010_complete_policy_values_and_height_are_committed_and_future_fork_is_explicit() {
    let p = palw_t12_shipped_params();
    let mut armed = p.clone();
    armed.palw_permissionless_panel_complete_v1 =
        Some(PalwPermissionlessPanelCompleteV1 { activation: ForkActivation::new(1234), policy: policy() });
    assert_eq!(&armed.palw_permissionless_panel_complete_v1.unwrap().commitment_bytes()[..2], &WIRE_VERSION_COMPLETE_V1.to_le_bytes());
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
    changed.palw_permissionless_panel_complete_v1.as_mut().unwrap().policy.receipt_window_daa += 1;
    assert_ne!(armed.consensus_params_id(), changed.consensus_params_id());
    assert_ne!(armed.consensus_schedule_id(), changed.consensus_schedule_id());
}

#[test]
fn rfc0010_complete_mirror_preserves_the_historical_profile_and_refuses_invalid_policy() {
    use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
    let mut params = palw_t12_shipped_params();
    let mut historical = policy();
    historical.max_candidates = 16;
    params.palw_permissionless_panel_v1 =
        Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(1234), policy: historical });
    params.sync_palw_permissionless_panel_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("V2");
    };
    assert_eq!(bundle.state.panel_v3().unwrap().wire_version, WIRE_VERSION_V2);
    params.palw_permissionless_panel_v1 = None;
    params.palw_permissionless_panel_complete_v1 =
        Some(PalwPermissionlessPanelCompleteV1 { activation: ForkActivation::new(2345), policy: policy() });
    params.sync_palw_permissionless_panel_v1();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("V2");
    };
    let mirror = bundle.state.panel_v3().unwrap();
    assert_eq!((mirror.wire_version, mirror.from_daa), (WIRE_VERSION_COMPLETE_V1, 2345));
    assert!(!bundle.state.panel_v3_rule_at(2344));
    assert!(bundle.state.panel_v3_rule_at(2345));
    assert_eq!(params.palw_permissionless_panel_activation_v1(), Some(ForkActivation::new(2345)));
    assert!(params.validate_palw_permissionless_panel_v1().is_err());
    params.palw_permissionless_panel_complete_v1.as_mut().unwrap().policy.max_assignments_per_block = 2;
    assert!(params.validate_palw_permissionless_panel_v1().is_err());
}
