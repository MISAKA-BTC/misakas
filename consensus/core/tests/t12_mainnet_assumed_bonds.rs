//! **testnet-12 runs the mainnet-assumed bonds** (user decision, 2026-09-24): a 13,000 MSK producer
//! floor, a 130,000 MSK panel seat floor (ten producer floors), and — `PALW_T12_DNS_PARAMS`, pinned
//! in `params.rs` — a 20,000,000 MSK DNS validator bond with six validators and 120,000,000 MSK of
//! active stake before DNS finality activates.
//!
//! What this file pins is the economics the floor is supposed to buy, measured through the
//! functions the chain and the producer actually run:
//!
//! * every genesis card (939,063.21 MSK a seat) clears every floor a seat is held to — the producer
//!   floor, the panel floor and the registry's readiness collateral (`floor × multiple`);
//! * **what option A leaves a floor-sized bond room for, measured**: the reservation a floor claim
//!   holds (its escrow plus its weight, the number `palw_producer_facts_v3` predicts and admission
//!   refuses on) is 3,200.95 MSK — the escrow (3,200.85) plus 0.11 MSK of weight — so a 13,000 MSK
//!   bond's 6,500 MSK ceiling (500 ‰) holds **two** concurrent floor claims (6,401.91 MSK) and not
//!   three. The brief expected exactly one ("reservation ≈ 6,401.7 MSK" is two claims' worth);
//!   exactly one would need a producer floor in [6,401.91, 12,803.82) MSK. Pinned as measured, and
//!   reported for the operator's decision.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::constants::SOMPI_PER_KASPA;
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionParamsV2, palw_work_lottery_floor_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwTransitionExtrasV1,
    apply_palw_transition_v7, palw_claim_escrow_v1, palw_work_floor_for_block_v1,
};
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

/// The subsidy testnet-12's first block pays (`PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI`).
const T12_BLOCK_SUBSIDY_SOMPI: u64 = kaspa_consensus_core::config::params::PALW_T12_GENESIS_BLOCK_SUBSIDY_SOMPI;
const MSK: u64 = SOMPI_PER_KASPA;

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn newcomer() -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0x13_000), index: 0 })
}

/// The registry fold as `palw_model_registry_fold_at` builds it for testnet-12.
fn registry_fold(p: &Params, b: &PalwConsensusParamsV2) -> PalwModelRegistryFoldV1 {
    let lane = p.palw_execution_lane.expect("t12 arms the execution lane");
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let mut works = palw_genesis_model_works_v1(&b.genesis_objects);
    for (id, work) in palw_rc_typed_class_works_v1() {
        works.entry(id).or_insert(work);
    }
    let activation = p.palw_model_registry.map(|f| f.daa_score()).unwrap_or(0);
    PalwModelRegistryFoldV1 {
        globals,
        span_daa: lane.schedule_span_daa,
        genesis_works: works,
        grace_until_daa: PalwModelRegistryFoldV1::grace_until_v1(activation, lane.schedule_span_daa, &globals),
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    }
}

fn carve(p: &Params) -> PalwRewardParamsV2 {
    PalwRewardParamsV2::new(p.palw_overlay_carve.expect("t12 arms the carve").worker_carve_permille).expect("the t12 worker carve")
}

/// The genesis fold's extras: every t12 fence armed (as `palw_transition_extras_for` resolves them).
fn genesis_extras(p: &Params, b: &PalwConsensusParamsV2) -> PalwTransitionExtrasV1 {
    let fold = registry_fold(p, b);
    PalwTransitionExtrasV1 {
        model_lines_active: true,
        model_benefits_active: true,
        evm_market_active: true,
        model_leg_v2_active: true,
        model_seed_v2_active: true,
        court_responder_coverage_active: true,
        fp_da_pins_active: true,
        share_growth_final_active: true,
        epoch_budget_release_active: true,
        panel_economy_active: true,
        work_priced_reward_active: true,
        panel_reward_multiple_permille: p.palw_panel_exposure_floor.map(|f| f.reward_multiple_permille).unwrap_or(0),
        economic_payout: p.palw_economic_payout.map(|f| f.fold_v1(0)),
        work_target: Some(PalwWorkTargetFoldV1 {
            rate_sompi_per_giga: p.palw_economic_payout.expect("t12 arms the payout").rate_sompi_per_giga,
            block_bits: 0,
            max_factor: b.state.class_daa_max_factor(),
            works: fold.genesis_works.clone(),
        }),
        work_target_active: true,
        artifact_root_ownership_active: true,
        operator_id_unique_active: true,
        canonical_work_daa: p.palw_canonical_work_daa(),
        admission_independence_daa: p.palw_admission_independence.map(|f| f.daa_score()),
        fp_derived_work_daa: p.palw_fp_derived_work.map(|f| f.daa_score()),
        single_lottery_active: true,
        verification_v2_active: true,
        verification_s3_active: true,
        verification_s2_active: true,
        readiness_v2_active: true,
        attn_anchored_root_active: true,
        audit_2026_09_11_active: true,
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        prompt_ids_merkle: true,
        objective_offence_daa: p.palw_objective_offence.map(|f| f.daa_score()),
        seat_gate_possession_daa: p.palw_seat_gate_possession.map(|f| f.daa_score()),
        escrow_carve: Some(carve(p)),
        model_registry: Some(fold),
        ..Default::default()
    }
}

/// The t12 genesis bundle plus one newcomer bond holding exactly `collateral`, folded.
fn genesis_with_newcomer(p: &Params, b: &PalwConsensusParamsV2, collateral: u64) -> PalwChainStateV2 {
    let mut objects = b.genesis_objects.clone();
    objects.push(PalwConsensusObjectV2::BondRegistered {
        bond: newcomer(),
        pubkey: vec![0x13; 8],
        operator_pubkey: vec![0x13; 16],
        collateral,
        payout_payload: h(0x13_000),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    let (state, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &b.state,
        None,
        &PalwBlockContextV2 { block: h(1), daa_score: 0, blue_score: 1, subsidy: 0 },
        &objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &genesis_extras(p, b),
    )
    .expect("the t12 genesis bundle folds");
    state
}

/// **Every genesis card clears every floor a seat is held to, and the floors are the mainnet ones.**
#[test]
fn testnet_12_states_the_mainnet_floors_and_every_genesis_card_clears_them() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let floor = b.state.min_collateral_sompi();
    assert_eq!(floor, 13_000 * MSK, "the producer floor is the mainnet one");
    assert_eq!(floor, kaspa_consensus_core::palw_fp_devnet_v3::PALW_MAINNET_MIN_COLLATERAL_SOMPI);
    let panel_floor = p.palw_seat_economy_at(0).expect("t12 states the seat economy").panel_floor_sompi;
    assert_eq!(panel_floor, 130_000 * MSK, "the panel seat floor is ten producer floors");
    let readiness_needed = floor * registry_fold(&p, &b).globals.readiness_collateral_multiple as u64;
    assert_eq!(readiness_needed, 39_000 * MSK, "a ready seat holds three producer floors free");

    let card = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let declared: Vec<u64> = b
        .genesis_objects
        .iter()
        .filter_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { collateral, .. } => Some(*collateral),
            _ => None,
        })
        .collect();
    assert_eq!(declared.len(), 8);
    for c in declared {
        assert_eq!(c, card, "each card declares the premine's carve");
        assert!(c >= floor && c >= panel_floor && c >= readiness_needed, "{c} clears the producer, panel and readiness floors");
    }
    // testnet-11 keeps its policy floor: its bundle and fingerprint do not move.
    let t11 =
        Params::from(kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 11));
    assert_eq!(
        bundle_of(&t11).state.min_collateral_sompi(),
        kaspa_consensus_core::palw_fp_devnet_v3::PALW_POLICY_MIN_COLLATERAL_SOMPI
    );
}

/// **Option A on a 13,000 MSK bond: room for two concurrent floor claims, not three** (measured;
/// the brief expected one — see the module doc).
///
/// The reservation is the producer's own headroom prediction, `palw_producer_facts_v3` — the same
/// expression admission's ceiling and the ledger reserve (its escrow plus its weight times the
/// attempts it carries), fed exactly what the processor feeds it at testnet-12's first blocks. The
/// ceiling is `collateral × max_exposure_ratio_permille / 1000`, admission's own arithmetic.
#[test]
fn option_a_leaves_a_floor_sized_bond_room_for_two_floor_claims_and_not_three() {
    let p = palw_t12_shipped_params();
    let b = bundle_of(&p);
    let floor = b.state.min_collateral_sompi();
    let state = genesis_with_newcomer(&p, &b, floor);
    let admission = PalwAdmissionParamsV2::new(b.state.fp_max_exposure_ratio_permille()).expect("admission params");
    assert_eq!(b.state.fp_max_exposure_ratio_permille(), 500, "the 500 ‰ ceiling");

    let daa = 2u64;
    let rate = p.palw_economic_payout.expect("t12 arms the payout").rate_sompi_per_giga;
    let w0 = palw_work_floor_for_block_v1(&b.state, T12_BLOCK_SUBSIDY_SOMPI, Some(carve(&p)), rate);
    let escrow = palw_claim_escrow_v1(&b.state, T12_BLOCK_SUBSIDY_SOMPI, Some(carve(&p)));
    let fold = registry_fold(&p, &b);
    let base_known_draw = fold.genesis_works.get(&b.base_class_id).map(|w| w.economic_ccu_per_claim).filter(|d| *d > 0);
    let facts = kaspa_consensus_core::palw_producer_v2::palw_producer_facts_v3(
        &state,
        &b.state,
        &admission,
        h(1),
        daa,
        b.base_class_id,
        Some(&newcomer()),
        palw_work_lottery_floor_v1(&state, Some(w0), true),
        p.palw_canonical_work_daa(),
        base_known_draw,
        true,
        escrow,
    )
    .expect("the floor has producer facts at testnet-12's first blocks");
    let bond = facts.bond.expect("the newcomer's bond is known");
    let reservation = bond.claim_exposure;
    let ceiling = bond.exposure_ceiling;
    let msk = |s: u128| s as f64 / MSK as f64;
    println!(
        "floor claim: escrow {:.8} MSK + weight {:.8} MSK = reservation {:.8} MSK; ceiling of a {:.0} MSK bond {:.8} MSK",
        msk(escrow as u128),
        msk(reservation - escrow as u128),
        msk(reservation),
        msk(floor as u128),
        msk(ceiling)
    );
    assert_eq!(bond.collateral, floor);
    assert_eq!(ceiling, floor as u128 * 500 / 1000, "6,500 MSK of room");
    assert_eq!(bond.reserved_exposure, 0, "nothing reserved yet");
    // One floor claim reserves its escrow and a sliver of weight: 3,200.95 MSK.
    assert!(reservation > escrow as u128, "the escrow and the weight");
    assert!((reservation as i128 - 320_095 * MSK as i128 / 100).abs() < MSK as i128 / 10, "≈ 3,200.95 MSK, got {}", msk(reservation));
    assert!(2 * reservation <= ceiling, "two concurrent floor claims fit: {} <= {}", msk(2 * reservation), msk(ceiling));
    assert!(3 * reservation > ceiling, "a third does not: {} > {}", msk(3 * reservation), msk(ceiling));
    // Exactly one would need a producer floor in [2r, 4r) at 500 ‰ — below 12,803.82 MSK.
    assert!(4 * reservation > 12_803 * MSK as u128 && 4 * reservation < 12_804 * MSK as u128);
}
