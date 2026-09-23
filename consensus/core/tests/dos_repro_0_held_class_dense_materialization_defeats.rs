//! dos_repro_0 — "held-class dense materialization defeats ADR-0121 and the ledger".
//!
//! **FIXED (DoS audit 2026-09-24, fix #4 — node policy, not consensus).** The whole-capture cap is
//! no longer the network's ladder: every family's `materialize_cap()` is
//! `misaka-palw-base0::fp_interval::base0_materialize_cap_v1`, which is
//! `palw_whole_capture_leaf_cap_v1(network, tile_len, declared budget)` — at most the historical
//! `2^26` (`PALW_RC_COURT_MAX_STEP_LEAF_COUNT`), and at most what the operator's declared per-node
//! budget holds dense. [`dos_repro_0_held_class_dense_materialization_is_refused`] asserts the fixed
//! behaviour on the t12 card; the original measurement below is kept, `#[ignore]`d, as
//! [`dos_repro_0_held_class_dense_materialization_defeats_defect_record`]. The kaspad half (the
//! sampler's ledger reservation and its one re-execution per claim) is pinned in kaspad's own tests
//! (`palw_backends::tests`, `palw_panel::court_responder_coverage_pin`). None of it is consensus:
//! no root, price or verdict reads the cap, and a chain cannot make a node hold a reservation.
//!
//! CLAIM UNDER TEST (the defect record) (node-policy amplification, lane L2): on testnet-12 the whole-capture
//! materialization cap that ADR-0121 Decision 1 uses to refuse a dense re-execution is
//! `materialize_cap = network_ladder = bundle.court.max_step_leaf_count()`, and t12 mints the
//! court at `2^40`, which is exactly `PALW_HELD_STEP_LADDER_V1`. The refusal predicate a family's
//! whole-capture prover runs is
//!
//!     base0_whole_capture_refusal_v1(leaves, materialize_cap, class_ladder) =
//!         Some(_)  iff  leaves == 0 || leaves > class_ladder || leaves > materialize_cap
//!
//! (misaka-palw-base0/src/fp_interval.rs:4059). For a held class `class_ladder ==
//! palw_class_step_ladder_v1(network, profile) == network.max(2^40) == 2^40 == materialize_cap`,
//! so for ANY leaf count the class ladder admits (`leaves <= 2^40`) the refusal can never fire and
//! the seat re-executes the whole job DENSE (`dense_capture_from_fold_v1` → `DenseTiles`).
//!
//! REACHABILITY. `base0_whole_capture_refusal_v1` and the `DenseTiles` re-execution live in
//! `misaka-palw-base0` / `kaspad`, which are NOT dependencies of `kaspa-consensus-core`, so this
//! test cannot call them (that is why the sibling `dos_l2_registration.rs` also re-derives the
//! predicate inline). What IS reachable — and is what this test drives with REAL functions — is
//! every input the predicate reads and the defender-cost formula the resource-profile module was
//! written to compute:
//!   * `palw_t12_shipped_params()`                          — the actual t12 card / fold
//!   * `bundle.court.max_step_leaf_count()`                 — the network ladder (materialize_cap)
//!   * `palw_class_step_ladder_v1(network, profile)`        — the class ladder
//!   * `verify_class_admission_v9(..)`                      — the admission gate (canonical leaves)
//!   * `step_leaf_count_capped_v1(profile, job, ladder)`    — a job's step-leaf count
//!   * `palw_profile_max_tile_len_v1` / `palw_dense_capture_bytes_v1` — the dense-capture bytes
//!   * `palw_fp_commitment_price_v1(..)`                    — the attacker's reserve (leaves path)
//! The refusal predicate itself is evaluated with the numbers those real functions return, exactly
//! as fp_interval.rs:4059 evaluates it — a 3-term boolean over consensus-core numbers, not a
//! re-implementation of any nontrivial logic.
//!
//! MEMORY SAFETY. The defender RAM is computed ANALYTICALLY by the shipped formula
//! `palw_dense_capture_bytes_v1`. One SMALL synthetic allocation (< 0.5 GiB) validates that the
//! formula's per-leaf constant matches a real `Vec<Hash64>` leaf vector plus an `i32` tile buffer,
//! and the real figures are that formula extrapolated to the real leaf counts. Nothing here is
//! sized from an attacker-controlled count.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
use kaspa_consensus_core::palw_class_admission_v2::{palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_resource_profile_v1::{palw_dense_capture_bytes_v1, palw_profile_max_tile_len_v1};
use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_class_step_ladder_v1, palw_profile_is_held_v4};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwConsensusObjectV2, PalwFpPriceInputsV1, PalwPwuRuleV2,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_fp_commitment_price_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const GIB: f64 = (1u64 << 30) as f64;
const ATTACKER: u64 = 0xA77AC;

fn t12() -> Params {
    palw_t12_shipped_params()
}

fn bundle_of(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("t12 is a ConsensusV2 network"),
    }
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}

fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: 444_562_014_000 }
}

/// The genesis classes' carriages, exactly as the t12 bundle ships them, plus the floor row.
fn genesis_carriages(b: &PalwConsensusParamsV2) -> Vec<(Hash64, PalwShapeProfileV3, PalwJobContextV2)> {
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor");
    let fjob = kaspa_consensus_core::palw_base0_profile::rc_job_context(
        &floor,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
    );
    let mut out = vec![(floor.shape_profile_id(), floor, fjob)];
    out.extend(b.genesis_objects.iter().filter_map(|o| match o {
        PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(c), .. } => {
            Some((*class_id, c.profile.clone(), c.canonical.clone()))
        }
        _ => None,
    }));
    out
}

/// t12 transition extras, ported argument-for-argument from `dos_l2_registration.rs::t12_extras`
/// so the genesis fold installs the card's classes exactly as the acceptance path does. The FP lane
/// is leaves-priced on t12 (`canonical_work` is `None`, so `canonical_work_daa` / `fp_derived_work_daa`
/// are both `None`).
fn registry_fold(p: &Params, b: &PalwConsensusParamsV2) -> kaspa_consensus_core::palw_model_registry_v1::PalwModelRegistryFoldV1 {
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
    };
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
    }
}

fn t12_extras(p: &Params, b: &PalwConsensusParamsV2) -> PalwTransitionExtrasV1 {
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
        work_target: Some(kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1 {
            rate_sompi_per_giga: 1,
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
            kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2::new(p.palw_overlay_carve.expect("carve").worker_carve_permille)
                .expect("carve"),
        ),
        model_registry: Some(fold),
        ..Default::default()
    }
}

/// A genesis state that also holds the attacker's bond, so the FP price function can read a class.
fn genesis_state(p: &Params, b: &PalwConsensusParamsV2, collateral: u64) -> PalwChainStateV2 {
    let mut objects = b.genesis_objects.clone();
    objects.push(PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(ATTACKER),
        pubkey: vec![0xA7; 2592],
        operator_pubkey: vec![0xA7; 32],
        collateral,
        payout_payload: h(ATTACKER),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    let (state, _, _) = apply_palw_transition_v7(
        &PalwChainStateV2::genesis(),
        &b.state,
        None,
        &ctx(1, 0, 1),
        &objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &t12_extras(p, b),
    )
    .expect("t12 genesis folds");
    state
}

/// The admission gate, argument-for-argument as `processor.rs` calls it (copied from the sibling
/// `dos_l2_registration.rs::admit_pwu`), returning the canonical step-leaf count the chain prices.
fn admit_pwu(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    daa: u64,
    pwu: u64,
) -> Result<u64, kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError> {
    let shape = palw_admission_shape_at_v1(p, b, profile, daa).expect("admission shape");
    let reg = PalwConsensusObjectV2::ClassRegistered {
        class_id: profile.shape_profile_id(),
        artifact_root: h(0x5eed),
        slash_value_per_pwu: 5,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: pwu },
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
        p.palw_canonical_work_at(daa),
        shape.held,
        shape.kimi_family,
        p.palw_audit_2026_09_23_active_at(daa),
    )
    .map(|e| e.canonical_step_leaf_count)
}

fn admit(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    daa: u64,
) -> Result<u64, kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError> {
    use kaspa_consensus_core::palw_class_admission_v2::PalwClassAdmissionError;
    match admit_pwu(p, b, profile, job, daa, 1) {
        Err(PalwClassAdmissionError::PwuPerInferenceMismatch { counted, .. }) => admit_pwu(p, b, profile, job, daa, counted),
        other => other,
    }
}

/// The predicate `base0_whole_capture_refusal_v1` evaluates (fp_interval.rs:4059), spelled with the
/// three real consensus-core numbers this test computed. `true` = the whole-capture prover REFUSES
/// (routes to the streamed lane); `false` = it materializes the whole capture DENSE.
fn whole_capture_refused(leaves: u64, materialize_cap: u64, class_ladder: u64) -> bool {
    leaves == 0 || leaves > class_ladder || leaves > materialize_cap
}

/// **The fix, asserted on the t12 card.** The whole-capture cap a t12 node applies is the host's
/// `2^26`, not the `2^40` network ladder; every held genesis class's canonical and deepest captures
/// are past it and refused by the predicate `base0_whole_capture_refusal_v1` evaluates, so no
/// sampling seat lays them out dense; under a declared 24 GiB budget (this Mac) nothing the cap
/// admits at any leaf count is a dense capture past that budget; the non-held floor is untouched.
#[test]
fn dos_repro_0_held_class_dense_materialization_is_refused() {
    use kaspa_consensus_core::palw_resource_profile_v1::{PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, palw_whole_capture_leaf_cap_v1};
    let p = t12();
    let b = bundle_of(&p);
    let network = b.court.max_step_leaf_count();
    assert_eq!(network, PALW_HELD_STEP_LADDER_V1, "t12 still walks at the held regime's ladder — that is not what moved");
    const BUDGET: u64 = 24 << 30;

    let mut held_rows = 0;
    for (id, profile, job) in genesis_carriages(&b) {
        let held = palw_profile_is_held_v4(&profile);
        let class_ladder = palw_class_step_ladder_v1(network, &profile);
        let tile = palw_profile_max_tile_len_v1(&profile);
        let cap = palw_whole_capture_leaf_cap_v1(network, tile, None);
        let budgeted = palw_whole_capture_leaf_cap_v1(network, tile, Some(BUDGET));
        assert_eq!(cap, PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1, "class {id}: the cap is the host's 2^26, not the network's 2^40");
        assert!(palw_dense_capture_bytes_v1(budgeted, tile) <= BUDGET, "class {id}: the budgeted cap's own capture fits the budget");

        let canonical_leaves = step_leaf_count_capped_v1(&profile, &job, class_ladder).expect("the canonical job");
        let mut deep = job.clone();
        deep.declared_prefill_tokens = profile.n_ctx.saturating_sub(1);
        deep.exact_decode_tokens = 1;
        let deep_leaves = step_leaf_count_capped_v1(&profile, &deep, class_ladder).expect("the deepest job");
        println!(
            "class {} | held {held} | tile {tile} | cap {cap} (budgeted {budgeted}) | canonical {canonical_leaves} refused {} | deepest {deep_leaves} refused {}",
            &id.to_string()[..8],
            whole_capture_refused(canonical_leaves, cap, class_ladder),
            whole_capture_refused(deep_leaves, cap, class_ladder)
        );
        if held {
            held_rows += 1;
            admit(&p, &b, &profile, &job, 0).expect("the chain admits the held row — the claim is fileable");
            assert!(whole_capture_refused(canonical_leaves, cap, class_ladder), "held class {id}: the canonical capture is refused");
            assert!(whole_capture_refused(deep_leaves, cap, class_ladder), "held class {id}: the deepest capture is refused");
            // Whatever the leaf count, a capture the cap admits is bounded — by 2^26 leaves unbudgeted,
            // and by the declared budget when there is one.
            for leaves in [1u64, 1 << 20, budgeted, budgeted + 1, cap, cap + 1, canonical_leaves, deep_leaves, class_ladder] {
                if !whole_capture_refused(leaves, budgeted, class_ladder) {
                    assert!(
                        palw_dense_capture_bytes_v1(leaves, tile) <= BUDGET,
                        "held class {id}: {leaves} leaves admitted past the budget"
                    );
                }
                if !whole_capture_refused(leaves, cap, class_ladder) {
                    assert!(leaves <= PALW_WHOLE_CAPTURE_DEFAULT_LEAF_CAP_V1);
                }
            }
        } else {
            assert!(
                !whole_capture_refused(canonical_leaves, cap, class_ladder),
                "class {id}: the floor's capture is laid out whole as before"
            );
        }
    }
    assert_eq!(held_rows, 2, "the t12 card ships two held rows");
}

#[test]
#[ignore = "PRE-FENCE DEFECT RECORD: the audit's measurement with materialize_cap == the t12 network ladder (2^40), i.e. the node policy before fix #4 (the cap is now base0_materialize_cap_v1); kept for its numbers"]
fn dos_repro_0_held_class_dense_materialization_defeats_defect_record() {
    let p = t12();
    let b = bundle_of(&p);
    let network = b.court.max_step_leaf_count();

    println!("=== dos_repro_0: held-class dense materialization defeats ADR-0121 and the ledger (t12) ===\n");
    println!("[1] the two ladders are one number on t12");
    println!("    court.max_step_leaf_count() (materialize_cap) = {network} = 2^{:.1}", (network as f64).log2());
    println!(
        "    PALW_HELD_STEP_LADDER_V1                        = {PALW_HELD_STEP_LADDER_V1} = 2^{:.1}",
        (PALW_HELD_STEP_LADDER_V1 as f64).log2()
    );
    assert_eq!(network, PALW_HELD_STEP_LADDER_V1, "t12's network ladder is the held class ladder; the refusal cannot separate them");

    // The FP lane is only reachable for classes the card certified. Prove the held rows are certified,
    // i.e. the attacker CAN file a free-prompt claim against them.
    let certified = b.state.fp_certified_classes().cloned().unwrap_or_default();
    println!("    fp_certified_classes = {} class(es)\n", certified.len());

    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let s0 = genesis_state(&p, &b, collateral);

    println!("[2] per genesis class: admission, the refusal that never fires, and the dense-capture RAM it costs a sampling seat\n");
    let mut worst_gib = 0f64;
    let mut proved_a_held_bug = false;

    for (id, profile, job) in genesis_carriages(&b) {
        let held = palw_profile_is_held_v4(&profile);
        let class_ladder = palw_class_step_ladder_v1(network, &profile);
        let tile = palw_profile_max_tile_len_v1(&profile);

        // REAL admission: is this a class the chain accepts, and at what canonical leaf count?
        let admitted = admit(&p, &b, &profile, &job, 0);
        // REAL leaf count of the class's own canonical job at the class ladder.
        let canonical_leaves = step_leaf_count_capped_v1(&profile, &job, class_ladder).unwrap_or(0);
        // The deepest single-decode job the class admits: prefill n_ctx-1, one decode.
        let mut deep = job.clone();
        deep.declared_prefill_tokens = profile.n_ctx.saturating_sub(1);
        deep.exact_decode_tokens = 1;
        let deep_leaves = step_leaf_count_capped_v1(&profile, &deep, class_ladder).unwrap_or(0);

        // The refusal, as fp_interval.rs:4059 computes it, with materialize_cap = network.
        let canon_refused = whole_capture_refused(canonical_leaves, network, class_ladder);
        let deep_refused = whole_capture_refused(deep_leaves, network, class_ladder);

        // Defender cost: the dense capture a sampling seat materializes, by the shipped formula.
        let canon_gib = palw_dense_capture_bytes_v1(canonical_leaves, tile) as f64 / GIB;
        let deep_gib = palw_dense_capture_bytes_v1(deep_leaves, tile) as f64 / GIB;

        println!(
            "  class {} | held {} | class ladder 2^{:.0} | tile_len {} | admission {}",
            &id.to_string()[..8],
            held,
            (class_ladder as f64).log2(),
            tile,
            match admitted {
                Ok(l) => format!("Ok({l} canonical leaves)"),
                Err(ref e) => format!("{e:?}"),
            }
        );
        println!(
            "      canonical job ({}+{}) = {} leaves -> whole-capture refused? {} -> DENSE re-exec {:.2} GiB",
            job.declared_prefill_tokens, job.exact_decode_tokens, canonical_leaves, canon_refused, canon_gib
        );
        println!(
            "      deepest job   ({}+1)  = {} leaves -> whole-capture refused? {} -> DENSE re-exec {:.1} GiB",
            deep.declared_prefill_tokens, deep_leaves, deep_refused, deep_gib
        );

        if held {
            // THE BUG: a held class's own canonical claim, which the chain admits and the class ladder
            // covers, is NOT refused by the whole-capture prover — so a sampling seat re-executes it
            // dense. On t12 this holds for every held genesis class.
            assert!(!canon_refused, "held class {id}: the canonical claim IS refused — the fix would show here");
            assert!(!deep_refused, "held class {id}: the deepest claim IS refused — the fix would show here");
            assert!(canon_gib > 24.0, "held class {id}: dense re-exec {canon_gib:.2} GiB does not exceed this 24 GiB Mac");
            worst_gib = worst_gib.max(deep_gib);
            proved_a_held_bug = true;

            // The attacker's cost for THIS held claim: one FP commitment's reserve (leaves path),
            // priced by the REAL fold price function against the real genesis state.
            let price = palw_fp_commitment_price_v1(
                &s0,
                &b.state,
                PalwFpPriceInputsV1 { fp_derived_work_daa: None, canonical_work_daa: None, daa_score: 0, receipt_rights: None },
                &h(0xC1A1),
                &id,
                &[1u32; 4],
                4,
                1,
                canonical_leaves,
            );
            match price {
                Ok(pp) => {
                    let msk = pp.reserved as f64 / 1e8;
                    println!(
                        "      ATTACKER cost: 1 FP claim, reserve {} sompi = {:.6} MSK (leaves path, quanta {} capped) + 1 honest capture",
                        pp.reserved, msk, pp.quanta
                    );
                    println!(
                        "      AMPLIFICATION: {:.2} GiB defender RAM / {:.6} MSK reserve = {:.3e} GiB-per-MSK (reserve is REFUNDED; it is not spent)\n",
                        canon_gib,
                        msk,
                        canon_gib / msk.max(1e-12)
                    );
                }
                Err(e) => println!(
                    "      ATTACKER cost: FP price refused for this class ({e:?}); reserve is bounded by the leaves-path cap regardless\n"
                ),
            }
        } else {
            println!();
        }
    }

    assert!(proved_a_held_bug, "no held genesis class was found — the t12 card should ship the 2M dense and the Qwen3.6 held rows");

    println!("[3] SMALL synthetic measurement validating the defender-cost formula (bounded < 0.5 GiB)");
    // Validate that `palw_dense_capture_bytes_v1`'s per-leaf accounting matches a REAL leaf vector
    // (`Vec<Hash64>`, the type `Base0StepCaptureV1.leaves` holds) plus an `i32` tile buffer.
    let synth_tile: u32 = 512; // the Qwen3.6 held row's tile_len
    let synth_leaves: u64 = 180_000; // 180k * (64 + 4*512) = ~380 MiB, well under 1 GiB
    let leaf_vec: Vec<Hash64> = vec![Hash64::default(); synth_leaves as usize];
    let mut tile_buf: Vec<i32> = vec![0i32; (synth_leaves * synth_tile as u64) as usize];
    // Touch pages so the allocation is resident, not just reserved.
    let last = tile_buf.len() - 1;
    tile_buf[0] = 1;
    tile_buf[last] = 1;
    let measured_data_bytes = (leaf_vec.capacity() as u64) * 64 + (tile_buf.capacity() as u64) * 4;
    // The formula also charges 56 B/leaf for the tile object header; the two DATA terms are what a
    // real Vec allocates, so measured == formula - 56*leaves.
    let formula_bytes = palw_dense_capture_bytes_v1(synth_leaves, synth_tile);
    let formula_data_bytes = formula_bytes - 56 * synth_leaves;
    println!(
        "    synthetic: {synth_leaves} leaves, tile {synth_tile} -> real Vec<Hash64>+i32 buffer = {} bytes ({:.1} MiB); formula's data terms = {} bytes; formula total = {} bytes",
        measured_data_bytes,
        measured_data_bytes as f64 / (1u64 << 20) as f64,
        formula_data_bytes,
        formula_bytes
    );
    assert_eq!(measured_data_bytes, formula_data_bytes, "the shipped dense-capture formula's data terms match a real allocation");
    std::hint::black_box(&leaf_vec);
    std::hint::black_box(&tile_buf);
    drop(leaf_vec);
    drop(tile_buf);

    println!(
        "\n[4] VERDICT: on t12 the whole-capture refusal (fp_interval.rs:4059) can never fire for a held claim the class ladder admits,\n    because materialize_cap == class_ladder == 2^40 (this test, step 1). A single free-prompt claim on a held genesis class\n    (attacker: 1 fee + a REFUNDED bounded reserve + one honest capture) makes a sampling full-seat materialize a DENSE capture of\n    the whole job (defender: {worst_gib:.0} GiB peak RAM for the deepest 2M job, >= 41 GiB for the smallest held canonical), and the\n    sampler path (kaspad fp_capture_samples_clear) takes NO reserve_replay_v1 reservation — source-verified by grep, not runnable here."
    );
    // The whole point: bounded attacker input, unbounded (RAM-exceeding) defender allocation.
    assert!(worst_gib > 24.0, "the deepest held job's dense capture {worst_gib:.1} GiB must exceed this host's RAM");
}
