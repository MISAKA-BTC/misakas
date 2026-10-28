//! **audit(compute-weight) L-1 — live testnet-12: the payout is priced by the class GRAPH; the weights a claim runs are never an input.**
//!
//! Evidence (passes on current code; it measures, it does not fail): on the shipped testnet-12 ruleset
//! (`palw_economic_payout`, `palw_model_lines`, `palw_artifact_root_ownership` armed — all three present in the int-12 release
//! 0b1c11b87) anyone with an Active bond may found a model line on a GENESIS class with ANY artifact root (ADR-0088/0175), every
//! attempt on the class may name any root in force on it (`class_roots_in_force`), and the claim's priced reward
//! (`snapshot_claim_economics` → `palw_claim_economics_snapshot_v1`) reads the class's registry work — derived from the shape profile
//! alone. So an artifact whose weights make the forward trivially computable (all-zero matrices: every matmul row is 0, the trace is
//! the embedding's norms and constant row hashes) is paid the full priced reward of the graph, every honest seat's replay agrees with
//! it, and no court can convict it: nothing in it is false. p(detect) = 0 independent of the honest fraction.
//!
//! There is no failing form of this test: the chain holds no weight content, and PRINCIPLES §9.3 forbids model-level review. The
//! adopted bound is ADR-0176/0177/0179 (bond-capped production, `A_m = c_m · S̄^1.5`, 0.3 tier and κ) — none armed on testnet-12, and
//! `palw_bond_budget_v1` / `palw_model_bond_allocation_v1` are not on `PALW_T12_FULL_ACTIVATION_FENCES_V1`.
//!
//! Run: cargo test --release -p kaspa-consensus-core --test audit_compute_weight -- --nocapture

#![allow(warnings)]
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClassStatusV2, PalwConsensusObjectV2,
    PalwStateCarriageV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
};
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

fn t12_escrow_sompi(p: &Params) -> u64 {
    let carve = p.palw_overlay_carve.expect("t12 arms the carve").worker_carve_permille as u64;
    T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve
}

fn t12_rate_sompi_per_giga(p: &Params) -> u64 {
    p.palw_economic_payout.expect("t12 arms the payout").rate_sompi_per_giga
}

fn msk(sompi: u128) -> String {
    format!("{}.{:08} MSK", sompi / 100_000_000, sompi % 100_000_000)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn ctx(block: u64, daa: u64, blue: u64, subsidy: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy }
}

/// The registry fold exactly as `processor.rs:8827 palw_model_registry_fold_at` builds it for
/// t12: the shipped globals with the bundle's seat count, the lane's span, and the works of the
/// genesis classes (bundle carriage first, then the typed catalog).
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
        genesis_works: works.clone(),
        grace_until_daa: PalwModelRegistryFoldV1::grace_until_v1(activation, lane.schedule_span_daa, &globals),
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
        bond_maturity: None,
    }
}

/// `PalwTransitionExtrasV1` for t12 at `daa`, field for field as
/// `processor.rs:8242 palw_transition_extras` builds it, with `bits = 0` (the degenerate network
/// draw of one, which is the conservative reading for the attacker).
fn t12_extras(p: &Params, b: &PalwConsensusParamsV2, _daa: u64) -> PalwTransitionExtrasV1 {
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
            rate_sompi_per_giga: t12_rate_sompi_per_giga(p),
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
        prompt_ids_merkle: true,
        objective_offence_daa: p.palw_objective_offence.map(|f| f.daa_score()),
        seat_gate_possession_daa: p.palw_seat_gate_possession.map(|f| f.daa_score()),
        escrow_carve: Some(
            PalwRewardParamsV2::new(p.palw_overlay_carve.expect("carve").worker_carve_permille).expect("the t12 worker carve"),
        ),
        model_registry: Some(fold),
        ..Default::default()
    }
}

/// The t12 genesis objects, plus one bond for the attacker with the genesis seat's collateral.
fn genesis_plus_attacker_bond(b: &PalwConsensusParamsV2, collateral: u64) -> Vec<PalwConsensusObjectV2> {
    let mut objects = b.genesis_objects.clone();
    objects.push(PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(0xA77AC),
        pubkey: vec![0xA7; 8],
        operator_pubkey: vec![0xA7; 16],
        collateral,
        payout_payload: h(0xA77AC),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    objects
}

/// Fold the t12 genesis bundle (+ the attacker's bond) into a real `PalwChainStateV2`.
fn t12_genesis_state(p: &Params, b: &PalwConsensusParamsV2, collateral: u64) -> PalwChainStateV2 {
    let (state, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &b.state,
        None,
        &ctx(1, 0, 1, 0),
        &genesis_plus_attacker_bond(b, collateral),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &t12_extras(p, b, 0),
    )
    .expect("the t12 genesis bundle folds");
    state
}

#[test]
fn audit_l1_a_new_root_on_a_genesis_graph_is_admitted_and_priced_as_the_graph() {
    let p = t12();
    let b = bundle_of(&p);
    for (name, armed) in [
        ("palw_economic_payout", p.palw_economic_payout.map(|f| f.activation)),
        ("palw_model_lines", p.palw_model_lines),
        ("palw_artifact_root_ownership", p.palw_artifact_root_ownership),
        ("palw_model_registry", p.palw_model_registry),
        ("palw_fp_derived_work", p.palw_fp_derived_work),
        ("palw_bond_budget_v1", p.palw_bond_budget_v1.as_ref().map(|f| f.activation)),
        ("palw_model_bond_allocation_v1", p.palw_model_bond_allocation_v1.as_ref().map(|f| f.activation)),
    ] {
        println!("t12 shipped: {name:<34} {armed:?}");
    }
    assert!(p.palw_economic_payout.is_some() && p.palw_model_lines.is_some() && p.palw_artifact_root_ownership.is_some());
    assert!(p.palw_bond_budget_v1.is_none() && p.palw_model_bond_allocation_v1.is_none(), "no bond cap on testnet-12");

    let attacker = bond_key(0xA77AC);
    let state = t12_genesis_state(&p, &b, 1_000_000 * 100_000_000);
    let base = b.state.base_class_id();
    let carriage = PalwStateCarriageV2::from_state(&state);
    let (class_id, class) = carriage
        .classes
        .iter()
        .filter(|(id, c)| **id != base && c.registrant_bond.is_none() && matches!(c.status, PalwClassStatusV2::Active))
        .map(|(id, c)| (*id, c.clone()))
        .next()
        .expect("testnet-12 genesis registers an Active model class");
    let zero_weights = Hash64::from_u64_word(0x2E20_2E20);
    assert_ne!(zero_weights, class.artifact_root);

    let daa = 100;
    let (after, _, _) = apply_palw_transition_v7(
        &state,
        &b.state,
        None,
        &ctx(2, daa, 2, 0),
        &[PalwConsensusObjectV2::ModelLineFounded {
            class_id,
            name: b"all-zero weights".to_vec(),
            founder: attacker,
            root: zero_weights,
            signature: Vec::new(),
        }],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &t12_extras(&p, &b, daa),
    )
    .expect("a model line with any root folds on a genesis class (rent and signature are the carrier's)");
    let roots = after.class_roots_in_force(&class_id, daa + 1);
    println!("class {class_id}: roots in force {roots:?}");
    assert!(roots.contains(&zero_weights), "an attempt naming the new root passes `ArtifactRootMismatch`");
    assert!(roots.contains(&class.artifact_root));

    // The price: the class's registry work (shape profile only) — no root, no weights.
    let fold = registry_fold(&p, &b);
    let work = fold.genesis_works.get(&class_id).copied().expect("the genesis class has registry work");
    let target = PalwStateCarriageV2::from_state(&after).class_targets.get(&class_id).map(|t| t.target).unwrap_or(u128::MAX);
    let payout = p.palw_economic_payout.unwrap().fold_v1(0);
    let snap = kaspa_consensus_core::palw_economic_payout_v1::palw_claim_economics_snapshot_v1(
        &payout,
        &work,
        b.panel.seat_count(),
        target,
        0,
    );
    let escrow = t12_escrow_sompi(&p);
    let priced = snap.priced_reward(escrow);
    println!(
        "draw_ccu {} MAC-eq, attempted {} MAC-eq, rate {} sompi/G, escrow {} -> priced reward {} for EITHER root",
        work.economic_ccu_per_claim,
        snap.attempted_ccu(),
        t12_rate_sompi_per_giga(&p),
        msk(escrow as u128),
        msk(priced as u128)
    );
    assert!(priced > 0, "a model-class Final is priced");
}
