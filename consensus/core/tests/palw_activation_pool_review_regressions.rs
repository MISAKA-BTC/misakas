//! **The Activation Pool review's probes P1–P5, adopted as regressions** (ADR-0152-adjacent:
//! Activation Pool; the review of feat/t12-activation-pool, MERGE-WITH-FIXES, and its fix round).
//!
//! Each probe demonstrated a hole on the reviewed branch; each test here states the fixed behaviour.
//! The fold-level halves live beside the fold: F2's jury and F4's landing in
//! `palw_state_v2::tests::adr0135::admission_independence::activation_pool_v1`, F5's queue in
//! `palw_state_v2::tests::vesting_fold_v1::pool_payouts_scheduled_never_stall_a_six_key_vesting_move`.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_activation_pool_v1::{palw_activation_sink_binding_refusal_v1, palw_activation_sink_spk_v1};
use kaspa_consensus_core::palw_lifecycle_objects_v2::{
    PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2, palw_activation_pool_binds_its_carrier_v1, palw_model_carrier_refund_v1,
};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{
    palw_admission_jury_seed_v1, palw_admission_jury_seed_v2, palw_readiness_landing_spans_v1,
};
use kaspa_consensus_core::palw_panel_v2::palw_admission_jury_ticket_v1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwChainStateV2, PalwConsensusObjectV2, PalwStateV2Error, PalwTransitionExtrasV1,
    apply_palw_transition_v7, palw_activation_pool_admits_v1,
};
use kaspa_consensus_core::palw_vesting_v1::{PALW_V2_VESTING_LEGS_PER_BLOCK, PALW_V2_VESTING_MARKET_RESERVE, palw_vesting_budget_v1};
use kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE;
use kaspa_consensus_core::tx::{Transaction, TransactionOutput};

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

/// ADR-0147's draw over operator ids: the `seats` lowest tickets.
fn jury(seed: &Hash64, operators: &[Hash64], seats: usize) -> Vec<Hash64> {
    let mut ranked: Vec<(Hash64, Hash64)> = operators.iter().map(|op| (palw_admission_jury_ticket_v1(seed, op), *op)).collect();
    ranked.sort();
    ranked.into_iter().take(seats).map(|(_, op)| op).collect()
}

/// **P1 (F2): past the pool's fence the anchor producer cannot grind the jury.** The v1 seed hashes
/// the anchor chain block, which its producer re-rolls for free: over free re-rolls five colluding
/// operators of twenty seat a majority on a measurable share of hashes. The v2 seed the fold uses
/// past `palw_activation_pool` takes no block hash, so every re-roll draws the same jury — moving it
/// costs another execution that wins its draw.
#[test]
fn p1_the_admission_jury_past_the_pool_fence_cannot_be_ground_by_the_anchor_block_hash() {
    let class = h(0xC1A5);
    let span = 4_321u64;
    let execution_key = h(0xE7E7);
    let operators: Vec<Hash64> = (1..=20u64).map(|n| h(0x0900 + n)).collect();
    let party: Vec<Hash64> = operators[..5].to_vec();
    let tries = 20_000u64;
    let majorities = |seed_of: &dyn Fn(u64) -> Hash64| {
        (0..tries).filter(|nonce| jury(&seed_of(*nonce), &operators, 5).iter().filter(|op| party.contains(op)).count() >= 3).count()
    };
    let v1 = majorities(&|nonce| palw_admission_jury_seed_v1(&class, span, &h(0xB10C_0000_0000 + nonce), &execution_key));
    println!("v1 (below the fence): {v1} of {tries} anchor hashes seat the party's majority");
    assert!(v1 > 0, "the v1 seed can be ground by the block hash — the hole the fence closes");
    let fixed = jury(&palw_admission_jury_seed_v2(&class, span, &execution_key), &operators, 5);
    let v2 = majorities(&|_| palw_admission_jury_seed_v2(&class, span, &execution_key));
    assert!(v2 == 0 || v2 as u64 == tries, "v2: one jury for every block hash — nothing to grind");
    assert_eq!(fixed.len(), 5);
    assert_ne!(
        palw_admission_jury_seed_v2(&class, span, &execution_key),
        palw_admission_jury_seed_v1(&class, span, &h(0), &execution_key)
    );
}

/// **P2 (F4): the landing window is what made `proved_span ≤ S − 2` a claim about the wrong span.**
/// A proof lands up to this many spans after the span it names; (a) now pays on the span the proof
/// LANDED in (`PalwChainStateV2::activation_readiness_landed`), which the fold records and a proof
/// cannot choose.
#[test]
fn p2_a_proof_may_land_after_the_span_it_names_so_a_names_nothing() {
    let t12 = palw_t12_shipped_params();
    let span_daa = t12.palw_execution_lane.expect("lane").schedule_span_daa;
    assert!(palw_readiness_landing_spans_v1(span_daa) >= 2, "a proof naming S − 2 may land at S − 1");
}

/// **P3 (F5): the width the pool's flush takes is what vesting and the market leave.** A 6-key row
/// fits the full width less the market's reserve; the pool's rows are flushed after vesting within
/// `8 − non-market − min(2, market)`, so at the next 3d nothing non-market waits and the arithmetic
/// below is the one every block sees.
#[test]
fn p3_vesting_keeps_its_six_keys_with_the_market_waiting() {
    let market_waiting = 2usize;
    assert_eq!(palw_vesting_budget_v1(PALW_V2_VESTING_LEGS_PER_BLOCK, market_waiting), 6, "a 6-key row fits");
    let pool_width_after_a_six_key_move = PALW_V2_VESTING_LEGS_PER_BLOCK - 6 - market_waiting.min(PALW_V2_VESTING_MARKET_RESERVE);
    assert_eq!(pool_width_after_a_six_key_move, 0, "the pool waits that block; it never takes vesting's width");
}

/// **P4 (F6 i): a top-up carrier a refusal could not pay back is not block-valid.** It binds, rides,
/// and names no refund payee — so the sink rule refuses it at isolation instead of letting a refused
/// top-up's MSK stay in the sink.
#[test]
fn p4_an_unrefundable_top_up_is_not_block_valid() {
    let class = h(0xC1A5);
    let amount = 5 * 100_000_000u64;
    let object = PalwConsensusObjectV2::ActivationPoolFunded { class_id: class, amount, sink_index: 0 };
    let tx = Transaction::new(
        0,
        vec![],
        vec![TransactionOutput::new(amount, palw_activation_sink_spk_v1(&class))],
        0,
        SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object: object.clone() }).unwrap(),
    );
    assert!(palw_activation_pool_binds_its_carrier_v1(&tx, &object).is_ok(), "it would bind");
    assert!(palw_model_carrier_refund_v1(&tx, &object).is_none(), "a refusal would have nobody to pay back");
    let refusal = palw_activation_sink_binding_refusal_v1(&tx);
    assert!(refusal.is_some_and(|(index, why)| index == 0 && why.contains("P2PKH-ML-DSA-87")), "{refusal:?}");
}

/// **P5 (F3): the floor takes no top-up** — its row is never a Candidate's and never leaves Active,
/// so no rule could pay its pool; the fold refuses it (and the P-B1 route pays it back). Every other
/// genesis class of testnet-12 is admitted.
#[test]
fn p5_the_floor_takes_no_top_up() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(b) = &p.palw_consensus_mode else { panic!("v2") };
    let extras = PalwTransitionExtrasV1 { activation_pool: p.palw_activation_pool_at(0), ..Default::default() };
    let ctx = PalwBlockContextV2 { block: h(1), daa_score: 0, blue_score: 1, subsidy: 0 };
    let genesis = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &b.state,
        None,
        &ctx,
        &b.genesis_objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .expect("t12 genesis folds")
    .0;
    let base = b.base_class_id;
    assert!(matches!(
        palw_activation_pool_admits_v1(&genesis, &b.state, &extras, &base, 100_000_000),
        Err(PalwStateV2Error::ActivationPoolOnFloor(id)) if id == base
    ));
    for (id, _) in genesis.classes_iter().filter(|(id, _)| **id != base) {
        assert!(
            palw_activation_pool_admits_v1(&genesis, &b.state, &extras, id, 100_000_000).is_ok(),
            "genesis class {id} takes a sponsor"
        );
    }
}
