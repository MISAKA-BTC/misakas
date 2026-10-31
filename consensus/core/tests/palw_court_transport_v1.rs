//! Dormant transport activation and identity regression; no production calibration.
#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    DEVNET_PARAMS, ForkActivation, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_court_transport_v1::PalwCourtTransportFenceV1;
use kaspa_consensus_core::palw_g14_gates_v1::palw_g14_genesis_test_params_v1;

#[test]
fn transport_is_dormant_and_never_keeps_t12_identity() {
    for p in [MAINNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params()] {
        assert!(p.palw_court_transport_v1.is_none());
    }
    let p = palw_t12_shipped_params();
    let mut never = p.clone();
    never.palw_court_transport_v1 = Some(PalwCourtTransportFenceV1::unapproved_probe_at(ForkActivation::never()));
    never.sync_palw_court_transport_v1();
    assert_eq!(p.consensus_params_id(), never.consensus_params_id());
    assert_eq!(p.consensus_identity_id(), never.consensus_identity_id());
    assert_eq!(p.consensus_schedule_id(), never.consensus_schedule_id());
}

#[test]
fn transport_policy_is_identity_bound_and_real_activation_is_refused() {
    let mut p = palw_g14_genesis_test_params_v1();
    p.palw_court_transport_v1 = Some(PalwCourtTransportFenceV1::unapproved_probe_at(ForkActivation::new(0)));
    p.sync_palw_court_transport_v1();
    p.validate_palw_court_transport_v1().expect("isolated synthetic profile only");
    let mut changed = p.clone();
    changed.palw_court_transport_v1.as_mut().unwrap().policy.base_memory += 1;
    changed.sync_palw_court_transport_v1();
    assert_ne!(p.consensus_params_id(), changed.consensus_params_id());
    let mut real = p.clone();
    real.genesis = palw_t12_shipped_params().genesis;
    real.sync_palw_court_transport_v1();
    assert!(format!("{:?}", real.validate_palw_court_transport_v1().unwrap_err()).contains("OneOutsiderCapacityIncomplete"));
}
