//! # AUDIT ACCEPTANCE TEST — the malicious model class, walked through every gate on testnet-12.
//!
//! The brief: deliberately construct a class whose REAL compute is at or below the liveness
//! floor's, but whose metadata makes it look like the maximum reward tier. Then walk it through
//! every gate in order and record, at each one, ACCEPTED or REJECTED-BY <function> <error>.
//!
//! **The construction.** Take the shipped BASE-0 liveness floor — the cheapest class the chain
//! registers — and change exactly two `u16`/`u32` scalars of its shape profile:
//!
//! ```text
//!   attn_heads    :     4  ->  65_535
//!   attn_head_dim :    64  ->  41_854
//! ```
//!
//! Nothing else moves. The node tables are byte-identical (asserted below), so every committed
//! row width (`out_len`), every tile, every kernel id and the whole leaf enumeration are the
//! floor's. What the two scalars DO move is the price, because
//! `palw_economic_compute_v1.rs:348-357` prices a matmul that reads the KV cache as
//!
//! ```text
//!   per_kv = attn_heads * attn_head_dim * cost.attention_mac
//! ```
//!
//! and never consults the node's own `out_len`. `PalwShapeProfileV3::validate_geometry`
//! (palw_step.rs:564-574) says in its own words that `attn_heads * attn_head_dim == hidden_dim`
//! is **"NOT enforced, deliberately"**. So for a non-fused class the ADJUDICATED row width and
//! the PRICED row width are two independent registrant-written numbers.
//!
//! Units, stated once and carried everywhere below:
//!   * declared leaves (`pwu_per_inference`)        — LEAVES
//!   * declared per-draw work (`economic_ccu_per_claim`, `provisional_scalar_v1`) — MAC-eq/draw
//!   * W0 (`palw_work_floor_v1`)                    — CCU (= MAC-eq)
//!   * escrow, priced_reward, reserved              — SOMPI (1 MSK = 1e8 sompi)
//!   * rate_sompi_per_giga                          — SOMPI per 1e9 MAC-eq
//!   * claim.pwu                                    — MAC-eq (expected_attempts x per-draw)
//!
//! Everything printed is produced by this worktree's runtime at detached HEAD 077d4c7f.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_admission_v2::{PalwAdmissionParamsV2, palw_attempt_derived_pwu_v1};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2,
    attempt_trace_manifest_root_v1, challenge_v2, execution_anchor_v3, execution_commitment_v3,
};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_CANONICAL, PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_model_work_from_carriage_v1,
    palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2,
    PalwPwuRuleV2, PalwStateParamsV2, PalwTransitionExtrasV1, apply_palw_transition_v7, palw_operator_id_v2,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::{PalwWorkTargetFoldV1, palw_work_floor_v1, palw_work_ticket_target_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

// =============================================================================================
// The card, and the numbers it fixes.
// =============================================================================================

/// The t12 block subsidy, measured upstream via `CoinbaseManager::calc_block_subsidy(0)`.
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

/// The escrow one t12 claim carries: the block subsidy through the overlay's worker carve.
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

// =============================================================================================
// The honest floor, and the malicious twin.
// =============================================================================================

fn floor_profile() -> PalwShapeProfileV3 {
    base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("the shipped BASE-0 floor")
}

fn floor_job() -> PalwJobContextV2 {
    rc_job_context(&floor_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)
}

/// **The malicious class.** The floor's graph, with two priced-but-unbound scalars raised.
/// `41_854` is one below the first value the court-carriage ceiling refuses (measured: at
/// `41_855` the gate answers `CourtCostExceedsCeiling { what: "court close bytes" }`), so this
/// is the most expensive-LOOKING class this construction can register.
fn malicious_profile() -> PalwShapeProfileV3 {
    let mut p = floor_profile();
    p.attn_heads = 65_535;
    p.attn_head_dim = 41_854;
    p
}

/// The malicious class's canonical job. Built from the floor's OWN profile and the floor's own
/// canonical `(8, 4)`, so the job context is the floor's in every field but the profile id.
fn malicious_job() -> PalwJobContextV2 {
    rc_job_context(&malicious_profile(), PALW_RC_BASE0_CANONICAL.0, PALW_RC_BASE0_CANONICAL.1)
}

/// One draw's DECLARED compute in MAC-eq — `economic_ccu_per_claim`, and, by the pin at
/// palw_canonical_work_v1.rs:889, also the fork-weight scalar `provisional_scalar_v1()`.
fn declared_ccu(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> u128 {
    palw_attempt_economic_compute_v1(profile, job, true, &PALW_ECONOMIC_COST_TABLE_V1).expect("priced")
}

/// One draw's REAL compute in MAC-eq: the same node tables priced under the HONEST geometry,
/// i.e. what an executor that produces the committed rows actually multiplies. The malicious
/// profile commits the same rows (asserted in gate 0), so this is its real cost too.
fn real_ccu() -> u128 {
    declared_ccu(&floor_profile(), &floor_job())
}

// =============================================================================================
// Fold fixtures — the REAL t12 state params, genesis objects and armed fence set.
// =============================================================================================

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
    }
}

/// `PalwTransitionExtrasV1` for t12 at `daa`, field for field as
/// `processor.rs:8242 palw_transition_extras` builds it, with `bits = 0` (the degenerate network
/// draw of one, which is the conservative reading for the attacker).
fn t12_extras(p: &Params, b: &PalwConsensusParamsV2, daa: u64) -> PalwTransitionExtrasV1 {
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
        escrow_carve: Some(PalwRewardParamsV2::new(
            p.palw_overlay_carve.expect("carve").worker_carve_permille,
        ).expect("the t12 worker carve")),
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

// =============================================================================================
// GATE 0 — the construction is what it claims to be.
// =============================================================================================

#[test]
fn gate_0_the_malicious_class_commits_the_floors_rows_and_prices_like_a_tier() {
    let p = t12();
    let honest = floor_profile();
    let evil = malicious_profile();

    println!("\n================ GATE 0: the construction ================");

    // The node tables — every committed row width, tile, kernel id and input ref — are identical.
    assert_eq!(honest.attn_nodes, evil.attn_nodes, "the attention node table is byte-identical");
    assert_eq!(honest.pre_nodes, evil.pre_nodes, "the pre node table is byte-identical");
    assert_eq!(honest.gdn_nodes, evil.gdn_nodes, "the gdn node table is byte-identical");
    assert_eq!(honest.post_nodes, evil.post_nodes, "the post node table is byte-identical");
    assert_eq!(honest.hidden_dim, evil.hidden_dim);
    assert_eq!(honest.layer_count, evil.layer_count);
    assert_eq!(honest.n_ctx, evil.n_ctx);
    println!("node tables (pre/gdn/attn/post)  : IDENTICAL to the floor");
    println!("hidden_dim / layer_count / n_ctx : IDENTICAL to the floor");
    println!("attn_heads     honest {:>6}  ->  evil {:>6}", honest.attn_heads, evil.attn_heads);
    println!("attn_head_dim  honest {:>6}  ->  evil {:>6}", honest.attn_head_dim, evil.attn_head_dim);

    // The leaf enumeration — what a court replays, and what `pwu_per_inference` must equal — is
    // driven by the node tables, so it does not move.
    let b = bundle_of(&p);
    let cap = b.court.max_step_leaf_count();
    let honest_leaves = step_leaf_count_capped_v1(&honest, &floor_job(), cap).expect("floor leaves");
    let evil_leaves = step_leaf_count_capped_v1(&evil, &malicious_job(), cap).expect("evil leaves");
    assert_eq!(honest_leaves, evil_leaves, "the adjudicated leaf count does not move");
    println!("declared leaves (LEAVES)         : honest {honest_leaves}  ==  evil {evil_leaves}");

    // The price does move.
    let honest_ccu = declared_ccu(&honest, &floor_job());
    let evil_ccu = declared_ccu(&evil, &malicious_job());
    println!("declared work (MAC-eq / draw)    : honest {honest_ccu}  ->  evil {evil_ccu}");
    println!("price inflation                  : {:.2}x", evil_ccu as f64 / honest_ccu as f64);
    assert!(evil_ccu > honest_ccu * 30_000, "the two scalars inflate the price by four orders of magnitude");

    // And the class identities differ, so this is a NEW class, not a re-registration.
    assert_ne!(honest.shape_profile_id(), evil.shape_profile_id());
    println!("class_id honest                  : {}", honest.shape_profile_id());
    println!("class_id evil                    : {}", evil.shape_profile_id());
}

// =============================================================================================
// THE WALK — every gate in order.
// =============================================================================================

/// Registration admission as the acceptance layer runs it (`processor.rs:6886`), at DAA 0 under
/// the t12 fence set. `share_permille` is 0 because `palw_admission_independence` is armed at 0.
fn run_registration_gate(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    artifact_root: Hash64,
) -> Result<u64, PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(p, b, profile, 0).expect("the t12 admission shape");
    let ladder_cap = match shape.ladder {
        Some(r) => r.ladder,
        None => kaspa_consensus_core::palw_state_chunk_map::palw_class_step_ladder_v1(b.court.max_step_leaf_count(), profile),
    };
    let counted = step_leaf_count_capped_v1(profile, job, ladder_cap).unwrap_or(u64::MAX);
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    verify_class_admission_v9(
        b,
        profile,
        job,
        &reg,
        &kaspa_consensus_core::palw_e2e_adjudicability::palw_rc_certified_families_v1(),
        &[],
        shape.ladder,
        shape.court,
        false,
        shape.token_lift,
        shape.fused_dissectable,
        p.palw_canonical_work_at(0),
        shape.held,
        shape.kimi_family,
        // 2026-09-23 audit C-4 fence, as the network resolves it.
        p.palw_audit_2026_09_23_active_at(0),
    )
    .map(|entry| entry.canonical_step_leaf_count)
}

#[test]
fn the_malicious_class_walked_through_every_gate_in_order() {
    let p = t12();
    let b = bundle_of(&p);
    let evil = malicious_profile();
    let evil_job = malicious_job();
    let evil_id = evil.shape_profile_id();
    let escrow = t12_escrow_sompi(&p);
    let rate = t12_rate_sompi_per_giga(&p);
    let w0 = palw_work_floor_v1(escrow, rate);
    let collateral = 51_642_979_663_480u64; // PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI

    println!("\n=========================================================================");
    println!("  THE WALK — a malicious class through every t12 gate, in order");
    println!("=========================================================================");
    println!("t12 escrow per claim  = {} sompi ({})", escrow, msk(escrow as u128));
    println!("t12 rate              = {rate} sompi per 1e9 MAC-eq");
    println!("W0 (work floor)       = {w0} CCU");
    println!("real work / draw      = {} MAC-eq (the BASE-0 floor's)", real_ccu());
    println!("declared work / draw  = {} MAC-eq", declared_ccu(&evil, &evil_job));

    let mut highest = "none";

    // ---------------------------------------------------------------- GATE 1: shape validation
    println!("\n--- GATE 1  PalwShapeProfileV3::validate_shape ---");
    match evil.validate_shape() {
        Ok(()) => {
            println!("GATE 1 shape validation                       : ACCEPTED");
            highest = "1 shape validation";
        }
        Err(e) => {
            println!("GATE 1 shape validation                       : REJECTED-BY validate_shape {e:?}");
            panic!("stop: gate 1 rejected");
        }
    }

    // ------------------------------------------------- GATE 2: registration / class admission
    println!("\n--- GATE 2  verify_class_admission_v9 (processor.rs:6886) ---");
    // The artifact root is the FLOOR's own derived root: BASE-0 has no file to host, and
    // `palw_artifact_root_ownership` keys ownership on (class_id, root), so a second class id
    // over the same root is a different key.
    let floor_root = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } if *class_id == b.base_class_id => {
                Some(*artifact_root)
            }
            _ => None,
        })
        .expect("the floor's registered artifact root");
    println!("artifact_root declared = the FLOOR's own root {floor_root}");
    let counted = match run_registration_gate(&p, &b, &evil, &evil_job, floor_root) {
        Ok(counted) => {
            println!("GATE 2 registration admission                 : ACCEPTED (canonical_step_leaf_count = {counted} LEAVES)");
            highest = "2 registration admission";
            counted
        }
        Err(e) => {
            println!("GATE 2 registration admission                 : REJECTED-BY verify_class_admission_v9 {e:?}");
            // **Past `palw_audit_2026_09_23` this is the acceptance criterion met, not a stop.** The
            // brief demanded that a class whose real compute is the floor's and whose metadata prices
            // like the top tier be refused at registration, claim, execution or reward; the fence's
            // admission twin refuses it at the first of those, by name, because the geometry the
            // price is computed from (65,535 x 41,854) is wider than the query row its graph reads.
            if matches!(e, kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError::AttentionGeometryWiderThanQueryRow { .. }) {
                println!("\nVERDICT: the malicious class is stopped at GATE 2 (registration admission) by");
                println!("         ATTENTION_GEOMETRY_WIDER_THAN_QUERY_ROW — it never reaches a claim, an");
                println!("         execution, a price or a reward. (Before the fence it reached gate 4.)");
                return;
            }
            panic!("stop: gate 2 rejected for an unexpected reason: {e:?}");
        }
    };

    // --------------------------------------------- GATE 3: artifact / manifest measurement
    println!("\n--- GATE 3  artifact-root measurement ---");
    println!("The registration declares a root. Nothing in consensus derives one:");
    println!("  - verify_class_admission_v9 took the root as an opaque Hash64 (gate 2 passed on");
    println!("    the FLOOR's root, for a class whose graph is not the floor's);");
    println!("  - the fold's only root rule is claim_artifact_root, keyed on (class_id, root).");
    println!("GATE 3 artifact measurement                   : ACCEPTED (no measurement exists)");
    highest = "3 artifact measurement";

    // ------------------------------------------------------ GATE 4: the fold accepts the object
    println!("\n--- GATE 4  apply_palw_transition_v7, the ClassRegistered arm ---");
    let state0 = t12_genesis_state(&p, &b, collateral);
    let carriage = PalwClassAdmissionCarriageV2 {
        profile: evil.clone(),
        canonical: evil_job.clone(),
        registrant_bond: bond_key(0xA77AC),
        signature: Vec::new(),
    };
    let registration = PalwConsensusObjectV2::ClassRegistered {
        class_id: evil_id,
        artifact_root: floor_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(carriage)),
    };
    let folded = apply_palw_transition_v7(
        &state0,
        &b.state,
        None,
        &ctx(2, 1, 2, T12_BLOCK_SUBSIDY_SOMPI),
        std::slice::from_ref(&registration),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &t12_extras(&p, &b, 1),
    );
    let state1 = match folded {
        Ok((s, _, _)) => {
            println!("GATE 4 state fold of ClassRegistered          : ACCEPTED");
            highest = "4 state fold of the registration";
            s
        }
        Err(e) => {
            println!("GATE 4 state fold of ClassRegistered          : REJECTED-BY apply_palw_transition_v7 {e:?}");
            print_verdict(highest, None, escrow);
            return;
        }
    };
    println!("  the class is in rooted state                : {}", state1.class(&evil_id).is_some());
    println!("  registration_exposure charged (sompi)       : {}", state1.registration_exposure(&bond_key(0xA77AC)));

    // ------------------------------------------------------------- GATE 5: the lifecycle row
    println!("\n--- GATE 5  the model-registry lifecycle row (open_model_lifecycle) ---");
    let row = state1.model_lifecycle(&evil_id).cloned();
    match &row {
        Some(r) => {
            println!("  lifecycle state                             : {:?}", r.state);
            println!("  economic_ccu_per_claim (MAC-eq/draw)        : {}", r.work.economic_ccu_per_claim);
            println!("  verification_ccu       (MAC-eq/job)         : {}", r.work.verification_ccu);
            println!("  admits_claims                               : {}", r.state.admits_claims());
            assert_eq!(
                r.work.economic_ccu_per_claim,
                declared_ccu(&evil, &evil_job),
                "the row's work is the registrant's own declared profile, walked"
            );
            if r.state.admits_claims() {
                println!("GATE 5 lifecycle admission                    : ACCEPTED");
                highest = "5 lifecycle admission";
            } else {
                println!("GATE 5 lifecycle admission                    : REJECTED-BY palw_lifecycle_step_v1 state={:?}", r.state);
            }
        }
        None => println!("  no lifecycle row written this block"),
    }

    // ------------------------------------------- GATE 6/7: claim construction + attempt admission
    println!("\n--- GATE 6/7  the attempt: check_class_admits_claim + check_palw_attempt_admission_v2 ---");
    let admission = PalwAdmissionParamsV2::new(b.state.fp_max_exposure_ratio_permille()).expect("admission params");
    let attempt_daa = 2u64;
    let declared = declared_ccu(&evil, &evil_job);
    let target = palw_work_ticket_target_v1(declared, w0);
    let pwu = palw_attempt_derived_pwu_v1(target, declared);
    println!("  class ticket target                         : {target}");
    println!("  (u128::MAX is {})", u128::MAX);
    println!("  derived attempt pwu (MAC-eq)                : {pwu}");
    let env = attempt_envelope(evil_id, floor_root, pwu, 7, 1_700_000_000);
    let key = execution_commitment_v3(&env.attempt, execution_anchor_v3(h(999), h(5), evil_id, &bond_key(0xA77AC).0, 7));
    let attempted = apply_palw_transition_v7(
        &state1,
        &b.state,
        Some(&admission),
        &ctx(3, attempt_daa, 3, T12_BLOCK_SUBSIDY_SOMPI),
        &[],
        PalwBlockWorkV3::Attempt(&env),
        &[],
        key,
        false,
        false,
        false,
        false,
        &t12_extras(&p, &b, attempt_daa),
    );
    let state2 = match attempted {
        Ok((s, _, _)) => {
            println!("GATE 6/7 attempt admission                    : ACCEPTED");
            highest = "6/7 attempt admission";
            Some(s)
        }
        Err(e) => {
            println!("GATE 6/7 attempt admission                    : REJECTED-BY apply_palw_transition_v7 {e}");
            None
        }
    };

    // --------------------------------------------------- GATE 8/9/10: pricing, reward, weight
    println!("\n--- GATE 8  work pricing (palw_work_ticket_target_v1 / palw_attempted_ccu_v1) ---");
    let honest_target = palw_work_ticket_target_v1(real_ccu(), w0);
    println!("  honest floor's ticket target                : {honest_target}");
    println!("  malicious class's ticket target             : {target}   (= u128::MAX: {})", target == u128::MAX);

    let mut reward_reached: Option<u128> = None;
    if let Some(state2) = &state2 {
        let claim_id = attempt_id_v2(&env.attempt);
        if let Some(claim) = state2.claim(&claim_id) {
            println!("  claim.pwu (MAC-eq)                          : {}", claim.pwu);
            println!("  claim.reserved (sompi)                      : {}", claim.reserved);
            println!("  claim.escrowed_reward (sompi)               : {}", claim.escrowed_reward);
            println!("\n--- GATE 9  reward (the rooted claim-economics snapshot) ---");
            match state2.claim_economics_of(&claim_id) {
                Some(econ) => {
                    let priced = econ.priced_reward(claim.escrowed_reward);
                    println!("  panel_share_permille                        : {}", econ.panel_share_permille);
                    println!("  priced_reward (sompi)                       : {priced}  ({})", msk(priced as u128));
                    println!("GATE 9 reward                                 : ACCEPTED");
                    highest = "9 reward (rooted claim economics)";
                    reward_reached = Some(priced as u128);
                }
                None => println!("  no claim-economics row (the class is the base class, or payout dormant)"),
            }
            println!("\n--- GATE 10  fork weight ---");
            println!("  the claim's weight-bearing pwu (MAC-eq)     : {}", claim.pwu);
            println!("  per MAC-eq REALLY executed                  : {:.2}x", claim.pwu as f64 / real_ccu() as f64);
        }
    }

    print_verdict(highest, reward_reached, escrow);
}

fn attempt_envelope(class_id: Hash64, artifact_root: Hash64, pwu: u64, nonce: u64, timestamp: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key(0xA77AC).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(999),
            challenge: challenge_v2(h(999), h(5), timestamp, nonce, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: vec![0xA7; 8],
            operator_id: palw_operator_id_v2(&[0xA7; 16]),
            artifact_root,
            trace_root: h(0x31),
            output_root: h(0x32),
            pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(0x31), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 999_999,
            execution_root: h(0x41),
        },
        signature: vec![0; 8],
    }
}

fn print_verdict(highest: &str, reward: Option<u128>, escrow: u64) {
    println!("\n=========================================================================");
    println!("  VERDICT");
    println!("=========================================================================");
    println!("highest gate reached : {highest}");
    match reward {
        Some(r) => {
            // The honest floor's cost of ONE paid claim: expected draws at its own target.
            let w0 = r; // placeholder guarded below
            let _ = w0;
            println!("reward obtained      : {r} sompi ({})", msk(r));
            println!("escrow ceiling       : {escrow} sompi ({})", msk(escrow as u128));
            println!("*** THE CLASS REACHED REWARD — CRITICAL ***");
        }
        None => println!("reward obtained      : NONE — the walk stopped before a reward was priced"),
    }
}

// =============================================================================================
// THE REWARD THE CLASS COLLECTS, IF THE LIFECYCLE IS SATISFIED.
//
// The walk above stopped at GATE 5 (`palw_lifecycle_step_v1` state=Candidate). That gate is
// about SEAT INDEPENDENCE (an outside jury must find the class runnable) and PROBE SUCCESS — it
// reads nothing about the class's declared work, its target or its price. It is the same walk
// every honest class makes. So the mispricing is fully written into rooted state (GATE 5 printed
// `economic_ccu_per_claim = 789_977_328_320`, 36,475x the real 21_657_728), and the only thing
// between it and reward is a Sybil check the malicious class passes the moment it is genuinely
// run — because it IS the floor's graph, an honest seat that verifies it does the floor's work,
// sees every committed row match, and votes Valid.
//
// This test prices the reward the class's Final would collect, using the SAME function the fold
// calls (`palw_claim_economics_snapshot_v1`, from `snapshot_claim_economics`,
// palw_state_v2.rs:9919), against the row GATE 5 actually wrote — and compares it to the honest
// floor's reward for the SAME real work.
// =============================================================================================

#[test]
fn the_reward_the_malicious_class_collects_for_floor_work() {
    let p = t12();
    let b = bundle_of(&p);
    let escrow = t12_escrow_sompi(&p);
    let rate = t12_rate_sompi_per_giga(&p);
    let w0 = palw_work_floor_v1(escrow, rate);
    let seat_count = b.panel.seat_count();

    // The malicious class's row, exactly as GATE 5 wrote it into rooted state.
    let evil = malicious_profile();
    let evil_job = malicious_job();
    let evil_work = palw_model_work_from_carriage_v1(&evil, &evil_job).expect("the row the fold writes");
    let evil_ccu = evil_work.economic_ccu_per_claim; // 789_977_328_320 MAC-eq
    let evil_target = palw_work_ticket_target_v1(evil_ccu, w0);

    // The honest floor, for reference.
    let real = real_ccu(); // 21_657_728 MAC-eq
    let floor_target = palw_work_ticket_target_v1(real, w0);

    println!("\n=========================================================================");
    println!("  THE REWARD — malicious class vs the honest floor, SAME real compute");
    println!("=========================================================================");

    // The malicious class is NOT the base class, so it takes the economic-payout snapshot (the
    // base class is paid its escrow whole and is exempt — palw_state_v2.rs:9913). bits = 0.
    let fold = p.palw_economic_payout.expect("payout").fold_v1(0);
    let snap = kaspa_consensus_core::palw_economic_payout_v1::palw_claim_economics_snapshot_v1(
        &fold, &evil_work, seat_count, evil_target, 0,
    );
    let evil_reward = snap.priced_reward(escrow);
    let evil_attempted = snap.attempted_ccu();

    // Expected draws a win costs each class = attempted_ccu / draw_ccu.
    let evil_draws = evil_attempted / evil_ccu;
    // The floor is the base class: paid its escrow whole, and a win costs W0/real draws.
    let floor_draws = (w0 / real).max(1);
    let floor_reward = escrow; // base class: escrow paid whole (work_priced_escrow returns it whole)

    println!("malicious class:");
    println!("  declared work / draw   = {evil_ccu} MAC-eq   (real executed: {real} MAC-eq, {}x lie)", evil_ccu / real);
    println!("  ticket target          = {evil_target}  (u128::MAX: {})", evil_target == u128::MAX);
    println!("  expected draws / win   = {evil_draws}");
    println!("  attempted_ccu (priced) = {evil_attempted} MAC-eq");
    println!("  priced_reward          = {evil_reward} sompi ({})", msk(evil_reward as u128));
    println!("  REAL compute / paid win= {} MAC-eq  ({} draws x {real})", evil_draws * real, evil_draws);
    println!("honest floor (base class):");
    println!("  work / draw            = {real} MAC-eq");
    println!("  expected draws / win   = {floor_draws}");
    println!("  reward / paid win      = {floor_reward} sompi ({})", msk(floor_reward as u128));
    println!("  REAL compute / paid win= {} MAC-eq  ({} draws x {real})", floor_draws * real, floor_draws);

    let evil_real_per_win = (evil_draws * real).max(1);
    let floor_real_per_win = (floor_draws * real).max(1);
    let reward_per_mac_evil = evil_reward as f64 / evil_real_per_win as f64;
    let reward_per_mac_floor = floor_reward as f64 / floor_real_per_win as f64;
    println!("\nreward per REAL MAC-eq:");
    println!("  malicious : {:.6e} sompi/MAC-eq", reward_per_mac_evil);
    println!("  floor     : {:.6e} sompi/MAC-eq", reward_per_mac_floor);
    println!("  ADVANTAGE : {:.2}x", reward_per_mac_evil / reward_per_mac_floor);

    // The malicious class collects the FULL escrow for ONE floor inference.
    assert_eq!(evil_reward, escrow, "the malicious class collects the whole escrow");
    assert_eq!(evil_draws, 1, "for a single draw (its target is u128::MAX)");
    assert!(
        reward_per_mac_evil / reward_per_mac_floor > 10_000.0,
        "reward per real MAC-eq is >10,000x the floor's"
    );
    println!("\n*** IF the lifecycle is satisfied, the class collects {} for {} MAC-eq of real work,", msk(evil_reward as u128), evil_real_per_win);
    println!("    the same escrow the honest floor earns for {}x more compute. ***", floor_real_per_win / evil_real_per_win);
}

// =============================================================================================
// BYPASSING GATE 5 (the lifecycle Candidate gate) — three ways.
// =============================================================================================

/// The rejecting check, isolated: `check_class_admits_claim` fails because a bought class opens
/// `Candidate` under `palw_admission_independence`, and `Candidate.admits_claims() == false`.
///
/// BYPASS 1 — register with the GENESIS SENTINEL bond. `open_model_lifecycle` opens a class
/// `Candidate` only when it is `bought` (`record.registrant_bond.is_some()`,
/// palw_state_v2.rs:10177). The fold maps the sentinel outpoint to `registrant_bond: None`
/// (palw_state_v2.rs:15782), so a carriage naming the sentinel is NOT bought and opens
/// `Prefetching` — skipping the independence jury entirely.
#[test]
fn bypass_1_the_genesis_sentinel_bond_skips_the_independence_jury() {
    let p = t12();
    let b = bundle_of(&p);
    let evil = malicious_profile();
    let evil_job = malicious_job();
    let evil_id = evil.shape_profile_id();
    let floor_root = floor_registered_root(&b);

    println!("\n================ BYPASS 1: the genesis sentinel bond ================");
    let state0 = t12_genesis_state(&p, &b, 51_642_979_663_480);
    let carriage = PalwClassAdmissionCarriageV2 {
        profile: evil.clone(),
        canonical: evil_job.clone(),
        // the sentinel — the zero outpoint no transaction can create
        registrant_bond: kaspa_consensus_core::palw_state_v2::palw_genesis_registrant_bond_v1(),
        signature: Vec::new(),
    };
    let counted = step_leaf_count_capped_v1(&evil, &evil_job, b.court.max_step_leaf_count()).unwrap();
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: evil_id,
        artifact_root: floor_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(carriage)),
    };
    let folded = apply_palw_transition_v7(
        &state0, &b.state, None, &ctx(2, 1, 2, T12_BLOCK_SUBSIDY_SOMPI), std::slice::from_ref(&reg),
        PalwBlockWorkV3::None, &[], Hash64::default(), false, false, false, false, &t12_extras(&p, &b, 1),
    );
    match folded {
        Ok((s, _, _)) => {
            let row = s.model_lifecycle(&evil_id).expect("row");
            println!("fold verdict                         : ACCEPTED");
            println!("lifecycle state                      : {:?}", row.state);
            println!("record.registrant_bond               : {:?} (sentinel -> None means NOT bought)", s.class(&evil_id).and_then(|c| c.registrant_bond));
            println!("admits_claims                        : {}", row.state.admits_claims());
            println!("RESULT: the fold opens the class at {:?} instead of Candidate — the jury is skipped.", row.state);
            println!("        BUT the acceptance layer (processor.rs:6816) verifies the carriage signature");
            println!("        under the registrant bond's REGISTERED key; the sentinel outpoint has no");
            println!("        registered bond, so a real node REJECTS this registration before the fold.");
            println!("        The bypass is open in the pure fold, closed by the acceptance layer.");
        }
        Err(e) => println!("fold verdict                         : REJECTED-BY apply_palw_transition_v7 {e}"),
    }
}

/// BYPASS 2 — register the class WITHOUT a carriage (`admission: None`), the genesis form.
/// A None registration writes no lifecycle row from a graph; `step_model_registry` writes an
/// inert zero row (`Registered`) for a class the build's `genesis_works` does not name. Does a
/// row-less / zero-work class admit claims on t12?
#[test]
fn bypass_2_a_carriageless_registration() {
    let p = t12();
    let b = bundle_of(&p);
    let evil = malicious_profile();
    let evil_job = malicious_job();
    let evil_id = evil.shape_profile_id();
    let floor_root = floor_registered_root(&b);

    println!("\n================ BYPASS 2: a carriageless (admission: None) registration ================");
    let state0 = t12_genesis_state(&p, &b, 51_642_979_663_480);
    let counted = step_leaf_count_capped_v1(&evil, &evil_job, b.court.max_step_leaf_count()).unwrap();
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: evil_id,
        artifact_root: floor_root,
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: counted },
        initial_target: u128::MAX,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    };
    let folded = apply_palw_transition_v7(
        &state0, &b.state, None, &ctx(2, 1, 2, T12_BLOCK_SUBSIDY_SOMPI), std::slice::from_ref(&reg),
        PalwBlockWorkV3::None, &[], Hash64::default(), false, false, false, false, &t12_extras(&p, &b, 1),
    );
    match folded {
        Ok((s, _, _)) => {
            println!("fold verdict                         : ACCEPTED");
            match s.model_lifecycle(&evil_id) {
                Some(row) => {
                    println!("lifecycle state                      : {:?}", row.state);
                    println!("economic_ccu_per_claim               : {}", row.work.economic_ccu_per_claim);
                    println!("admits_claims                        : {}", row.state.admits_claims());
                }
                None => println!("lifecycle row                        : NONE this block (opens at next span boundary as inert Registered)"),
            }
            // Now try to admit a claim of it: on t12 work_target_active, a class with no usable row
            // is refused with "no row".
            let admission = PalwAdmissionParamsV2::new(b.state.fp_max_exposure_ratio_permille()).unwrap();
            let declared = declared_ccu(&evil, &evil_job);
            let pwu = palw_attempt_derived_pwu_v1(palw_work_ticket_target_v1(declared, palw_work_floor_v1(t12_escrow_sompi(&p), t12_rate_sompi_per_giga(&p))), declared);
            let env = attempt_envelope(evil_id, floor_root, pwu, 7, 1_700_000_000);
            let key = execution_commitment_v3(&env.attempt, execution_anchor_v3(h(999), h(5), evil_id, &bond_key(0xA77AC).0, 7));
            let attempted = apply_palw_transition_v7(
                &s, &b.state, Some(&admission), &ctx(3, 2, 3, T12_BLOCK_SUBSIDY_SOMPI), &[],
                PalwBlockWorkV3::Attempt(&env), &[], key, false, false, false, false, &t12_extras(&p, &b, 2),
            );
            match attempted {
                Ok(_) => println!("attempt admission                    : ACCEPTED — a carriageless class admitted a claim"),
                Err(e) => println!("attempt admission                    : REJECTED-BY apply_palw_transition_v7 {e}"),
            }
            println!("RESULT: a carriageless class gets no derived row, so on t12 (work_target armed)");
            println!("        it either has no row or a zero-work row and admits no claim.");
        }
        Err(e) => println!("fold verdict                         : REJECTED-BY apply_palw_transition_v7 {e}"),
    }
}

/// BYPASS 3 — the honest lifecycle walk. The class genuinely runs the floor's graph, so the
/// independence jury and the probe claims it must pass are things it CAN pass without any forgery.
/// This test does not spin up a jury (that needs seat-readiness proofs and a panel draw); it
/// establishes the compute-agnosticism of the gate that stopped the walk, which is what makes the
/// walk available to the attacker: NOTHING on the Candidate->Active path reads the declared work,
/// the target, or the price.
#[test]
fn bypass_3_the_lifecycle_gate_is_compute_agnostic() {
    // The three predicates `palw_lifecycle_step_v1` reads to leave Candidate and reach Active are
    // `admission_jury_seated`, `ready_seats`, `probes_passed`, `collateral_ok`, `cap_ok`,
    // `window_fits_receipt`, `span_stable`, `panel_drawable`. None is a function of
    // `economic_ccu_per_claim`, `verification_ccu`, the class target or the price. We assert that
    // by reading the malicious row's work and showing the step function's inputs do not include it.
    println!("\n================ BYPASS 3: the lifecycle gate is compute-agnostic ================");
    let evil = malicious_profile();
    let evil_job = malicious_job();
    let work = palw_model_work_from_carriage_v1(&evil, &evil_job).expect("row");
    println!("the malicious row the walk carries:");
    println!("  economic_ccu_per_claim = {} (36,475x the floor)", work.economic_ccu_per_claim);
    println!("  ops_supported          = {}", work.ops_supported);
    println!("The Candidate->Active transitions (palw_model_registry_v1.rs:492-548) read:");
    println!("  admission_jury_seated, ready_seats, probes_passed/failed, collateral_ok,");
    println!("  cap_ok, window_fits_receipt, span_stable, panel_drawable.");
    println!("NONE reads economic_ccu_per_claim, verification_ccu, the class target or the price.");
    println!("The class runs the FLOOR's graph (identical node tables, gate 0), so:");
    println!("  - ops_supported = {} (the executor can run every op)", work.ops_supported);
    println!("  - a seat proving readiness reconstructs the FLOOR's artifact root (the declared root)");
    println!("  - probe claims execute the FLOOR's work and every committed row matches -> Valid");
    println!("RESULT: an attacker who operates ordinary floor-serving seats walks this class to");
    println!("        Active with no forgery — the lie is priced, not executed, so verification");
    println!("        confirms it. The gate that stopped the direct walk does not look at the lie.");
    assert!(work.ops_supported, "the malicious class's ops are all supported — it is genuinely runnable");
}

fn floor_registered_root(b: &PalwConsensusParamsV2) -> Hash64 {
    b.genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, artifact_root, .. } if *class_id == b.base_class_id => {
                Some(*artifact_root)
            }
            _ => None,
        })
        .expect("floor root")
}
