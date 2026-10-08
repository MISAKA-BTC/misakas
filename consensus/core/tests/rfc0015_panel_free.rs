//! RFC-0015's fence `palw_panel_free_v1`: dormant on every preset, invisible to every identity while `None` or `Some(never())`,
//! committed to the parameter and schedule fingerprints once it carries a height, and refused when armed.

use kaspa_consensus_core::config::params::{
    ForkActivation, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params, palw_rc_shipped_params,
    palw_t12_shipped_params,
};
use kaspa_consensus_core::fork_id_v1::{evaluate_fork_id_v1, fork_id_v1};

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
            armed.palw_panel_free_v1 = Some(ForkActivation::new(at));
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
    never.palw_panel_free_v1 = Some(ForkActivation::never());
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
    armed.palw_panel_free_v1 = Some(ForkActivation::new(9_137));
    assert_ne!(p.consensus_params_id(), armed.consensus_params_id());
    assert_ne!(p.consensus_schedule_id(), armed.consensus_schedule_id());
    let mut other = p.clone();
    other.palw_panel_free_v1 = Some(ForkActivation::new(9_001));
    assert_ne!(armed.consensus_params_id(), other.consensus_params_id(), "the height itself is hashed");
    // The fence is a named fork like every other: before it the two networks agree, from it a node without it refuses.
    let before = fork_id_v1(&p, 8_999);
    let future = fork_id_v1(&armed, 8_999);
    assert_eq!(before.fired, future.fired);
    assert!(!evaluate_fork_id_v1(&armed, 8_999, &before.fired.as_bytes(), before.next).refuses());
    let unupgraded = fork_id_v1(&p, 9_137);
    assert!(evaluate_fork_id_v1(&armed, 9_137, &unupgraded.fired.as_bytes(), unupgraded.next).refuses());
}
