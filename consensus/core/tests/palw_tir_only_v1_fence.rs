//! **RFC-0002 Phase H — `palw_tir_only_v1`: the IR is the admission path for new classes (mainnet step; dormant, armed nowhere).**
//!
//! Implemented and tested, never armed: a bare height, `None` on every preset and in no flag-day list, hashed Some-only into both
//! fingerprints and collapsed from `Some(never())`; mirrored on the bundle (`tir_only_from_daa`); `validate_palw_v2` refuses arming
//! without `palw_tir_v1` at or below it. Past it the fold refuses a post-genesis legacy-family registration (a `ClassRegistered` that
//! carries an admission carriage) by name, and below it — and on every network that does not arm it — the same object is not
//! refused for this reason.

#[path = "rcore_common.rs"]
mod rcore;
use rcore::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_shipped_params};
use kaspa_consensus_core::palw_state_v2::PalwStateV2Error;
use kaspa_consensus_core::palw_tir_v1::PalwTirFenceV1;

const AT: u64 = 1_100;

fn armed(tir_only_at: u64) -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(ForkActivation::new(AT)));
    p.sync_palw_tir_v1();
    p.palw_tir_only_v1 = Some(ForkActivation::new(tir_only_at));
    p.sync_palw_tir_only_v1();
    p.validate_palw_v2().unwrap_or_else(|e| panic!("tir-only at {tir_only_at}: {e}"));
    p
}

#[test]
fn it_is_dormant_on_every_shipped_preset_and_armed_nowhere() {
    use kaspa_consensus_core::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, TESTNET11_PARAMS};
    for params in [&DEVNET_PARAMS, &MAINNET_PARAMS, &SIMNET_PARAMS, &TESTNET_PARAMS, &TESTNET11_PARAMS] {
        assert_eq!(params.palw_tir_only_v1, None);
    }
    let shipped = palw_t12_shipped_params();
    assert_eq!(shipped.palw_tir_only_v1, None, "testnet-12 ships it dormant");
    assert!(!shipped.palw_tir_only_active_at(u64::MAX - 1));
    // The int-11 release (the last flag day) does not arm it either.
    let release = kaspa_consensus_core::config::params::palw_t12_release_v5_params();
    assert_eq!(release.palw_tir_only_v1, None);
}

#[test]
fn arming_it_needs_the_ir_fence_below_it_and_a_consistent_mirror() {
    // Without palw_tir_v1.
    let mut p = palw_t12_shipped_params();
    p.palw_tir_only_v1 = Some(ForkActivation::new(AT));
    p.sync_palw_tir_only_v1();
    let why = p.validate_palw_v2().unwrap_err().to_string();
    assert!(why.contains("palw_tir_only_v1 needs palw_tir_v1"), "{why}");
    // With the IR fence ABOVE it.
    let mut p = armed(AT + 10);
    p.palw_tir_only_v1 = Some(ForkActivation::new(AT - 1));
    p.sync_palw_tir_only_v1();
    assert!(p.validate_palw_v2().unwrap_err().to_string().contains("palw_tir_only_v1 needs palw_tir_v1"));
    // An unsynced mirror.
    let mut p = armed(AT);
    p.palw_tir_only_v1 = Some(ForkActivation::new(AT + 5));
    assert!(p.validate_palw_v2().unwrap_err().to_string().contains("mirror"));
    // Armed: active from its height.
    let p = armed(AT + 20);
    assert!(!p.palw_tir_only_active_at(AT + 19) && p.palw_tir_only_active_at(AT + 20));
}

#[test]
fn it_is_hashed_some_only_and_never_collapses_into_the_dormant_identity() {
    let a = armed(AT + 20);
    let mut baseline = a.clone();
    baseline.palw_tir_only_v1 = None;
    baseline.sync_palw_tir_only_v1();
    assert_ne!(a.consensus_params_id(), baseline.consensus_params_id());
    assert_ne!(a.consensus_schedule_id(), baseline.consensus_schedule_id());
    assert_eq!(a.consensus_identity_id(), baseline.consensus_identity_id(), "a scheduled fence is normalised out of the identity");
    let mut never = a.clone();
    never.palw_tir_only_v1 = Some(ForkActivation::never());
    never.sync_palw_tir_only_v1();
    assert_eq!(never.consensus_identity_id(), baseline.consensus_identity_id());
    never.validate_palw_v2().expect("a dormant value validates");
}

#[test]
fn past_the_fence_a_legacy_family_registration_is_refused_by_name_and_below_it_is_not() {
    let p = armed(AT + 20);
    let sp = bundle(&p).state.clone();
    let (_, _, object) = kaspa_consensus_core::palw_qwen36_profile::qwen36_held_registration_v1(
        h(0x36A7),
        512,
        0,
        5,
        u128::MAX / 2,
        &bundle(&p),
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        floor_producer(&p).0,
    )
    .expect("the held hybrid row derives at 512");
    let chain = Chain::new(p.clone());
    let fold_at = |daa: u64| chain.try_fold(&chain.s, &ctx(0xCA_0000 + daa, daa, daa, 0), &[object.clone()], PalwBlockWorkV3::None, Hash64::default());
    let past = fold_at(AT + 20);
    match past {
        Err(PalwStateV2Error::TirRegistrationRefused(why)) => assert!(why.contains("palw_tir_only_v1"), "{why}"),
        other => panic!("expected the Phase H refusal, got {:?}", other.map(|_| ())),
    }
    // Below the fence the same object is not refused for this reason (it folds, or fails for something else).
    match fold_at(AT + 19) {
        Err(PalwStateV2Error::TirRegistrationRefused(why)) if why.contains("palw_tir_only_v1") => panic!("refused below the fence: {why}"),
        _ => {}
    }
    // And the shipped ruleset, which never arms it, never refuses it for this reason, at any height.
    let shipped = palw_t12_shipped_params();
    let chain = Chain::new(shipped);
    match chain.try_fold(&chain.s, &ctx(0xCA_0000 + AT + 40, AT + 40, AT + 40, 0), &[object], PalwBlockWorkV3::None, Hash64::default()) {
        Err(PalwStateV2Error::TirRegistrationRefused(why)) if why.contains("palw_tir_only_v1") => panic!("the dormant ruleset refuses it: {why}"),
        _ => {}
    }
    let _ = sp;
}
