//! Proposed clock discrimination and arming refusal, not a calibrated profile.
#![allow(warnings)]
use kaspa_consensus_core::palw_tir_frame_cost_v1::*;
use kaspa_consensus_core::{
    Hash64,
    config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params},
};

#[test]
fn clock_v2_changes_configured_fingerprints_but_is_not_a_shipped_or_reviewed_profile() {
    for p in [MAINNET_PARAMS, DEVNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params()] {
        assert!(p.palw_tir_frame_cost_v1.is_none());
    }
    let cost = palw_tir_frame_test_only_cost_v1();
    let mut v2 = cost;
    v2.clock.version = PALW_TIR_FRAME_CLOCK_VERSION_V2;
    let mut first = palw_t12_shipped_params();
    // Fingerprint-only fixtures, not validated/armed Params.
    first.palw_tir_frame_cost_v1 = Some(cost);
    let mut second = first.clone();
    second.palw_tir_frame_cost_v1 = Some(v2);
    assert_ne!(first.consensus_params_id(), second.consensus_params_id());
    assert_ne!(first.consensus_schedule_id(), second.consensus_schedule_id());
    assert!(
        PALW_TIR_FRAME_COST_RECORDS_V1
            .iter()
            .all(|r| r.activation_daa == 17_400 && r.genesis == kaspa_consensus_core::config::genesis::PALW_T12_GENESIS.hash)
    );
    assert_eq!(
        palw_tir_frame_cost_record_admits_v1(Hash64::from_u64_word(1), 16_000, Some(&v2), PALW_TIR_FRAME_COST_RECORDS_V1),
        Err(PalwTirFrameCostRefusalV1::NoRecord)
    );
    // Synthetic record fixture only. An old clock record cannot authorize the new rule.
    let old = PalwTirFrameCostRecordV1 {
        version: PALW_TIR_FRAME_COST_RECORD_VERSION_V1,
        genesis: Hash64::from_u64_word(1),
        activation_daa: 16_000,
        source_commit: [1; 20],
        measurement: [2; 32],
        review: [3; 32],
        cost,
    };
    assert_eq!(
        palw_tir_frame_cost_record_admits_v1(old.genesis, old.activation_daa, Some(&v2), &[old]),
        Err(PalwTirFrameCostRefusalV1::Differs)
    );
}
