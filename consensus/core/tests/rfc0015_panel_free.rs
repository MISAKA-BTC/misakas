//! RFC-0015's fence `palw_panel_free_v1`: dormant on every preset, invisible to every identity while `None` or `Some(never())`,
//! committed to the parameter and schedule fingerprints once it carries a height, and refused when armed.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};
use kaspa_consensus_core::palw_panel_free_v1::PalwPanelFreeFenceV1;
use kaspa_hashes::Hash64;

fn presets() -> Vec<Params> {
    vec![
        mainnet_shipped_params(),
        devnet_shipped_params(),
        palw_rc_shipped_params(),
        palw_t12_shipped_params(),
        Params::from(TESTNET_PARAMS.net),
        Params::from(SIMNET_PARAMS.net),
    ]
}

#[test]
fn rfc0015_every_preset_is_dormant_and_every_real_activation_is_refused() {
    for p in presets() {
        assert!(p.palw_panel_free_v1.is_none());
        assert!(!p.palw_panel_free_active_at(u64::MAX));
        p.validate_palw_panel_free_v1().unwrap();
        for at in [0, 900, 1234, 9_137, u64::MAX - 1] {
            let mut armed = p.clone();
            armed.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::at(ForkActivation::new(at)));
            assert!(armed.validate_palw_panel_free_v1().is_err(), "{at}");
            assert!(armed.validate_palw_v2().is_err(), "{at}: the fence is part of the params validation");
            assert!(!armed.palw_panel_free_active_at(at) || armed.validate_palw_panel_free_v1().is_err());
        }
    }
}

#[test]
fn rfc0015_dormant_and_never_leave_the_handshake_identity_and_fork_ids_unchanged() {
    let p = palw_t12_shipped_params();
    let mut never = p.clone();
    never.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::at(ForkActivation::never()));
    never.validate_palw_panel_free_v1().unwrap();
    assert!(!never.palw_panel_free_active_at(u64::MAX));
    // Some(never()) collapses whole in the handshake identity, as if absent; like every plain fence it is an explicit, hashed choice
    // in the params and schedule fingerprints (a network that spells it is a different spelling of the same history).
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
    for at in [0, 750, 1234, u64::MAX - 1] {
        assert_eq!(fork_id_v1(&p, at), fork_id_v1(&never, at));
    }
}

#[test]
fn rfc0015_a_height_is_committed_to_the_fingerprints_and_a_node_without_it_would_refuse_the_fork() {
    let p = palw_t12_shipped_params();
    let mut armed = p.clone();
    armed.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::at(ForkActivation::new(9_137)));
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
    let mut other = p.clone();
    other.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::at(ForkActivation::new(9_001)));
    assert_ne!(armed.consensus_params_id(), other.consensus_params_id(), "the height itself is hashed");
    // The fence is a named fork like every other: before it the two networks agree, from it a node without it refuses.
    let before = fork_id_v1(&p, 8_999);
    let future = fork_id_v1(&armed, 8_999);
    assert_eq!(before.fired, future.fired);
    assert!(!evaluate_fork_id_v1(&armed, 8_999, &before.fired.as_bytes(), before.next).refuses());
    let unupgraded = fork_id_v1(&p, 9_137);
    assert!(evaluate_fork_id_v1(&armed, 9_137, &unupgraded.fired.as_bytes(), unupgraded.next).refuses());
}

/// OPV-BOOT: the fence carries the network's DENY-list, the effective-bits floor and the OPV terms — never an admission list (OPV
/// eligibility is derived from chain state, `palw_opv_bootstrap_v1`). All three are identity (a node with another list, floor or
/// terms is on another network), the value is checked before the arming refusal names itself, the interim floor is the ruled 128
/// effective bits, and the kernel ledger's policy is the fence's terms activating at its height.
#[test]
fn rfc0015_the_fence_carries_the_deny_list_the_floor_and_the_terms_and_all_are_identity() {
    let p = palw_t12_shipped_params();
    let (a, b) = (Hash64::from_u64_word(3), Hash64::from_u64_word(9));
    let fence = |classes: Vec<Hash64>| PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(9_137), classes);
    assert!(fence(vec![b, a]).denied_classes == vec![a, b], "the constructor canonicalizes the list");
    assert!(fence(vec![]).denied_classes.is_empty(), "the initial deny-list is empty");
    assert_eq!(fence(vec![]).min_effective_bits, 128, "the interim floor is the ruled effective 128 bits");
    assert_eq!(PalwPanelFreeFenceV1::at(ForkActivation::new(1)).min_effective_bits, 128);
    fence(vec![a, b]).validate_value().unwrap();

    let id = |f: PalwPanelFreeFenceV1| {
        let mut q = p.clone();
        q.palw_panel_free_v1 = Some(f);
        (q.consensus_params_id(), q.consensus_schedule_id())
    };
    assert_ne!(id(fence(vec![a])), id(fence(vec![a, b])), "another deny-list is another network");
    assert_ne!(id(fence(vec![a])), id(fence(vec![b])));
    let mut floor = fence(vec![a]);
    floor.min_effective_bits = 2;
    assert_ne!(id(fence(vec![a])), id(floor), "another floor is another network");
    assert!(
        !fence(vec![]).grandfather_panel_route_classes,
        "G14-for-rewards: live Panel-route classes are not grandfathered by default"
    );
    let mut grandfather = fence(vec![a]);
    grandfather.grandfather_panel_route_classes = true;
    assert_ne!(id(fence(vec![a])), id(grandfather), "grandfathering is a user decision, and identity");
    let mut terms = fence(vec![a]);
    terms.economics.default_burn_permille += 1;
    assert_ne!(id(fence(vec![a])), id(terms), "other terms are another network");
    assert_eq!(id(fence(vec![a])), id(fence(vec![a])), "and the value is deterministic");

    // The value is checked, in the params validation, before the arming refusal.
    let mut bad_order = fence(vec![a, b]);
    bad_order.denied_classes = vec![b, a];
    assert!(bad_order.validate_value().is_err());
    let mut bad_terms = fence(vec![a]);
    bad_terms.economics.reservation_per_claim = 1;
    assert!(bad_terms.validate_value().is_err(), "a reservation below the maximum gain is refused");
    for f in [bad_order, bad_terms, fence(vec![a])] {
        let mut q = p.clone();
        q.palw_panel_free_v1 = Some(f);
        assert!(q.validate_palw_panel_free_v1().is_err() && q.validate_palw_v2().is_err());
    }

    // The ledger's policy is the terms, activating at the fence's height; a never() fence activates nowhere.
    let policy = fence(vec![a]).opv_policy();
    assert_eq!(policy.activation_daa, Some(9_137));
    assert_eq!(PalwPanelFreeFenceV1::at(ForkActivation::never()).opv_policy().activation_daa, None);
}

/// RFC-0009 G-EXPIRY / G-RULESET (lane D phase 3): the signed-registration envelope's own fence is dormant on every preset, invisible to
/// the handshake identity while `None` or `Some(never())`, committed to the fingerprints with a height, and refused when armed.
#[test]
fn the_signed_registration_fence_is_dormant_hashed_when_set_and_refused_when_armed() {
    for p in presets() {
        assert!(p.palw_signed_registration_v1.is_none() && !p.palw_signed_registration_active_at(u64::MAX));
        p.validate_palw_signed_registration_v1().unwrap();
        let mut armed = p.clone();
        armed.palw_signed_registration_v1 = Some(ForkActivation::new(1_000));
        assert!(armed.validate_palw_signed_registration_v1().is_err() && armed.validate_palw_v2().is_err());
    }
    let p = palw_t12_shipped_params();
    let mut never = p.clone();
    never.palw_signed_registration_v1 = Some(ForkActivation::never());
    never.validate_palw_signed_registration_v1().unwrap();
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id(), "Some(never()) collapses whole in the handshake identity");
    let mut armed = p.clone();
    armed.palw_signed_registration_v1 = Some(ForkActivation::new(9_000));
    let mut other = p.clone();
    other.palw_signed_registration_v1 = Some(ForkActivation::new(9_001));
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(armed.consensus_params_id(), other.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
}
