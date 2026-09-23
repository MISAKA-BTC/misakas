//! Review of #12 (139c9215), adversarial probes on the burn and the per-block cap. Probe 1 (the
//! burn ignored live slashable locks) was fixed by d0542304 and is kept as a regression.
//! Helpers copied from `dos_l2_registration.rs` (t12 card, t12 audit extras, floor variants).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_economic_payout_v1::PalwEconomicPayoutFoldV1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2,
    PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_v2_apply_one_object_v1, palw_v2_pre_object_base_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
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
fn bond_key(v: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
}
fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}
fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: T12_BLOCK_SUBSIDY_SOMPI }
}
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
    }
}
fn t12_extras(p: &Params, b: &PalwConsensusParamsV2) -> PalwTransitionExtrasV1 {
    let fold = registry_fold(p, b);
    let _unused: Option<PalwEconomicPayoutFoldV1> = None;
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
        audit_2026_09_23_active: p.palw_audit_2026_09_23_active_at(0),
        settled_anchor_depth: if p.palw_audit_2026_09_23_active_at(0) { p.palw_settled_anchor_depth } else { None },
        prompt_ids_merkle: true,
        objective_offence_daa: p.palw_objective_offence.map(|f| f.daa_score()),
        seat_gate_possession_daa: p.palw_seat_gate_possession.map(|f| f.daa_score()),
        escrow_carve: Some(PalwRewardParamsV2::new(p.palw_overlay_carve.expect("carve").worker_carve_permille).expect("carve")),
        model_registry: Some(fold),
        ..Default::default()
    }
}
fn genesis_carriages(_b: &PalwConsensusParamsV2) -> Vec<(Hash64, PalwShapeProfileV3, PalwJobContextV2)> {
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor");
    let fjob = kaspa_consensus_core::palw_base0_profile::rc_job_context(
        &floor,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
    );
    vec![(floor.shape_profile_id(), floor, fjob)]
}
fn floor_variant(
    b: &PalwConsensusParamsV2,
    template: &(Hash64, PalwShapeProfileV3, PalwJobContextV2),
    n: u32,
    floor_slash: u64,
    floor_target: u128,
    registrant: PalwBondKeyV2,
) -> PalwConsensusObjectV2 {
    let mut prof = template.1.clone();
    prof.n_threads = n;
    let mut job = template.2.clone();
    job.shape_profile_id = prof.shape_profile_id();
    let leaves = step_leaf_count_capped_v1(&prof, &job, b.court.max_step_leaf_count()).unwrap_or(1);
    PalwConsensusObjectV2::ClassRegistered {
        class_id: prof.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(0xF000_0000 + n as u64),
        slash_value_per_pwu: floor_slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: floor_target,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: prof,
            canonical: job,
            registrant_bond: registrant,
            signature: vec![0u8; 4627],
        })),
    }
}
fn genesis_state_under(b: &PalwConsensusParamsV2, collateral: u64, extras: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
    let mut objects = b.genesis_objects.clone();
    objects.push(PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(ATTACKER),
        pubkey: vec![0xA7; 8],
        operator_pubkey: vec![0xA7; 16],
        collateral,
        payout_payload: h(ATTACKER),
        capable_classes: Default::default(),
        signature: Vec::new(),
    });
    apply_palw_transition_v7(
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
        extras,
    )
    .expect("genesis folds")
    .0
}
fn fold_one(
    b: &PalwConsensusParamsV2,
    base: &PalwChainStateV2,
    block: u64,
    objects: &[PalwConsensusObjectV2],
    extras: &PalwTransitionExtrasV1,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    apply_palw_transition_v7(
        base,
        &b.state,
        None,
        &ctx(block, block, block),
        objects,
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        extras,
    )
    .map(|(s, _, _)| s)
}
fn bought(b: &PalwConsensusParamsV2, base: &PalwChainStateV2, first: u32, n: u32) -> Vec<PalwConsensusObjectV2> {
    let floor = genesis_carriages(b).into_iter().next().expect("floor");
    let slash = base.class(&b.base_class_id).unwrap().slash_value_per_pwu;
    let target = base.class_target(&b.base_class_id).unwrap().target;
    (first..first + n).map(|k| floor_variant(b, &floor, k, slash, target, bond_key(ATTACKER))).collect()
}

/// **Regression (was PROBE 1, fixed by d0542304): the burn leaves live slashable locks covered.**
/// The burn's affordability check in the `ClassRegistered` arm used to sum reserved + registration
/// + declared exposure and ignore the registrant's LIVE slashable locks, so a registration could
/// burn collateral a live Valid lock stood on (a conviction then took `min(lock, collateral)`).
/// Past the fix the live locks are in `already`: a registration whose burn would cut into a lock is
/// refused `ClassRegistrationBurnUnaffordable` and the bond is untouched; posted with room for the
/// lock, the registration and the burn, it folds and leaves the lock fully covered.
#[test]
fn review12_0_burn_leaves_live_slashable_locks_covered() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    assert!(extras.audit_2026_09_23_active);
    let price = b.state.registration_exposure_sompi() as u128;
    let burn = PALW_CLASS_REGISTRATION_BURN_SOMPI_V1 as u128;
    // A live Valid lock on some claim, standing on everything but the registration's reservation.
    let with_lock = |collateral: u64, lock_amount: u128| {
        let s0 = genesis_state_under(&b, collateral, &extras);
        let mut carriage = PalwStateCarriageV2::from_state(&s0);
        carriage.slashable_locks.insert(
            (bond_key(ATTACKER), h(0x10C4)),
            PalwSlashableLockV1 { claim: h(0x10C4), amount: lock_amount, expiry_daa: 1_000_000, settled_at_final: 0 },
        );
        carriage.into_state(&b.state, None).expect("consistent")
    };

    // (1) The probe's bond: 10 MSK, a lock on all of it but the registration's price.
    let collateral: u64 = 10 * 100_000_000;
    let lock_amount = collateral as u128 - price;
    let s0 = with_lock(collateral, lock_amount);
    let avail_before = s0.slashable_available_v2(&bond_key(ATTACKER), 2, None);
    let refused = fold_one(&b, &s0, 2, &bought(&b, &s0, 70_000, 1), &extras);
    println!("10 MSK bond, live lock {lock_amount}, slashable available {avail_before}: {refused:?}");
    match &refused {
        Err(PalwStateV2Error::ClassRegistrationBurnUnaffordable { burn: b_, already, collateral: c, .. }) => {
            assert_eq!(*b_ as u128, burn, "the refused burn is the registration burn");
            assert!(*already >= lock_amount, "the live lock is counted as already standing on the bond: {already} < {lock_amount}");
            assert_eq!(*c, collateral);
        }
        other => panic!("a burn that would cut into a live lock must be refused ClassRegistrationBurnUnaffordable: {other:?}"),
    }

    // (2) Posted with room for the lock, the registration's price and the burn: it folds, and the
    // bond still covers the lock afterwards.
    let roomy = u64::try_from(lock_amount + price + burn).unwrap();
    let s0 = with_lock(roomy, lock_amount);
    let s1 = fold_one(&b, &s0, 2, &bought(&b, &s0, 70_000, 1), &extras).expect("a bond with room for lock + price + burn registers");
    let after = s1.bond(&bond_key(ATTACKER)).unwrap();
    println!("{roomy} sompi bond: collateral -> {}, slashed {}, live lock {lock_amount}", after.collateral, after.slashed);
    assert_eq!(after.collateral as u128, roomy as u128 - burn, "the burn is taken whole");
    assert!(after.collateral as u128 >= lock_amount + price, "the bond still covers its live lock and the registration's reservation");
}

/// PROBE 2: the fold's `ClassRegistrationsPerBlockExceeded` is a hard error of the whole transition,
/// while the acceptance rehearsal (`palw_v2_pre_object_base_v1` + `palw_v2_apply_one_object_v1`, a
/// fresh TransitionBuilder per object) never sees the counter. Five bought registrations each pass
/// the rehearsal; the only thing between them and a disqualified block is the processor walk's own
/// counter (processor.rs `class_registrations_charged`).
#[test]
fn review12_0_rehearsal_cannot_see_the_fold_cap() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    let s0 = genesis_state_under(&b, 51_642_979_663_480, &extras);
    let five = bought(&b, &s0, 80_000, 5);
    let point = ctx(2, 2, 2);
    let mut folded = palw_v2_pre_object_base_v1(&s0, &b.state, &point, false, false, false, false, &extras).expect("base");
    for (i, object) in five.iter().enumerate() {
        folded = palw_v2_apply_one_object_v1(&folded, &b.state, &point, object, false, false, false, false, &extras)
            .unwrap_or_else(|e| panic!("rehearsal refused object {i}: {e:?}"));
    }
    let whole = fold_one(&b, &s0, 2, &five, &extras);
    assert!(
        matches!(whole, Err(PalwStateV2Error::ClassRegistrationsPerBlockExceeded { max: 4, .. })),
        "the rehearsal accepts all five, the fold refuses the block: {whole:?}"
    );
}
