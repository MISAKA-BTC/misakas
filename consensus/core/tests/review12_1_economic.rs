//! review12_1 — adversarial economic review of 139c9215 (2026-09-24 DoS audit #12), t12 card. The
//! readiness double-count was fixed by d0542304 and is kept as a regression.
//!
//! Fixtures copied from `dos_l2_registration.rs` so the two files fold the same states.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, PalwSeatReadinessRowV1, palw_genesis_model_works_v1,
    palw_rc_typed_class_works_v1, palw_seat_not_ready_reason_v1,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2,
    PalwClassAdmissionCarriageV2, PalwClassStatusV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_bond_registration_floor_v1,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

const SUBSIDY: u64 = 444_562_014_000;
const REGISTRANT: u64 = 0xA77AC;
const CONTROL: u64 = 0xC0C0;

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
    PalwBlockContextV2 { block: h(block), daa_score: daa, blue_score: blue, subsidy: SUBSIDY }
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

fn floor_carriage() -> (Hash64, PalwShapeProfileV3, PalwJobContextV2) {
    let floor =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("floor");
    let fjob = kaspa_consensus_core::palw_base0_profile::rc_job_context(
        &floor,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.0,
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL.1,
    );
    (floor.shape_profile_id(), floor, fjob)
}

/// A bought registration of a distinct floor variant, exactly as `dos_l2_registration::floor_variant`.
fn registration(b: &PalwConsensusParamsV2, base: &PalwChainStateV2, n: u32, registrant: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let template = floor_carriage();
    let slash = base.class(&b.base_class_id).unwrap().slash_value_per_pwu;
    let target = base.class_target(&b.base_class_id).unwrap().target;
    let mut prof = template.1.clone();
    prof.n_threads = n;
    let mut job = template.2.clone();
    job.shape_profile_id = prof.shape_profile_id();
    let leaves = step_leaf_count_capped_v1(&prof, &job, b.court.max_step_leaf_count()).unwrap_or(1);
    PalwConsensusObjectV2::ClassRegistered {
        class_id: prof.shape_profile_id(),
        artifact_root: Hash64::from_u64_word(0xF000_0000 + n as u64),
        slash_value_per_pwu: slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: target,
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

fn bond_reg(v: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(v),
        pubkey: vec![(v & 0xff) as u8; 8],
        operator_pubkey: vec![(v & 0xff) as u8 ^ 0x5a; 16],
        collateral,
        payout_payload: h(v),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn genesis_with(b: &PalwConsensusParamsV2, bonds: &[(u64, u64)], extras: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
    let mut objects = b.genesis_objects.clone();
    objects.extend(bonds.iter().map(|(v, c)| bond_reg(*v, *c)));
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

fn fold(
    b: &PalwConsensusParamsV2,
    base: &PalwChainStateV2,
    block: u64,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    extras: &PalwTransitionExtrasV1,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    apply_palw_transition_v7(
        base,
        &b.state,
        None,
        &ctx(block, daa, block),
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

/// **Regression (was a finding, fixed by d0542304): the registration burn is counted ONCE by the
/// readiness predicate.**
///
/// `burn_registration_fee` writes the burn through the same write as a slash: `collateral` falls
/// AND `slashed` rises. The readiness predicate — consensus `model_registry_seat_is_ready` and its
/// RPC twin `palw_seat_not_ready_reason_v1`, the same formula — used to compute
/// `free = collateral − slashed − held`, subtracting every burned MSK a second time, so two bonds
/// holding the SAME collateral differed in readiness only because one had paid a registration.
/// Past the audit fence both read `palw_bond_free_collateral_v1`, which treats `collateral` as
/// already net. Asserted: the registrant and the control, same collateral, are both ready.
#[test]
fn review12_1_registration_burn_is_counted_once_by_seat_readiness() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    assert!(extras.audit_2026_09_23_active);
    let reg_floor = palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true);
    let price = b.state.registration_exposure_sompi();
    // The registrant posts 1.2 MSK; the control posts what the registrant will have left.
    let registrant_collateral = 120_000_000u64;
    let left = registrant_collateral - PALW_CLASS_REGISTRATION_BURN_SOMPI_V1;
    assert!(left >= reg_floor, "the control can register at the floor");
    let s0 = genesis_with(&b, &[(REGISTRANT, registrant_collateral), (CONTROL, left)], &extras);
    let s1 = fold(&b, &s0, 2, 2, &[registration(&b, &s0, 70_001, bond_key(REGISTRANT))], &extras).expect("registration folds");
    let r = s1.bond(&bond_key(REGISTRANT)).unwrap();
    let c = s1.bond(&bond_key(CONTROL)).unwrap();
    println!(
        "registrant: collateral {} slashed {} registration_exposure {}; control: collateral {} slashed {}",
        r.collateral,
        r.slashed,
        s1.registration_exposure(&bond_key(REGISTRANT)),
        c.collateral,
        c.slashed
    );
    assert_eq!(r.collateral, left, "the burn left the collateral");
    assert_eq!(r.slashed, PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, "and was recorded in `slashed`");
    let fold_v = registry_fold(&p, &b);
    let needed = u128::from(b.state.min_collateral_sompi()) * u128::from(fold_v.globals.readiness_collateral_multiple);
    let true_free = u128::from(r.collateral) - s1.registration_exposure(&bond_key(REGISTRANT));
    println!("readiness needs {needed} sompi free; the registrant truly has {true_free} free (price {price})");
    assert!(true_free >= needed, "the registrant really holds far more than readiness asks");
    let now = 3u64;
    let row = PalwSeatReadinessRowV1 { proved_daa: now, proved_span: 0, leaf_index: 0, proof_version: 2, chunks: 8 };
    let registrant_reason = palw_seat_not_ready_reason_v1(&s1, &b.state, &bond_key(REGISTRANT), &row, now, &fold_v);
    let control_reason = palw_seat_not_ready_reason_v1(&s1, &b.state, &bond_key(CONTROL), &row, now, &fold_v);
    println!("registrant not-ready reason: {registrant_reason:?}; control: {control_reason:?}");
    assert_eq!(control_reason, None, "the control, same collateral, is ready");
    assert_eq!(
        registrant_reason, None,
        "the registrant, same collateral and more than enough free, is ready too: the burn is subtracted once (d0542304)"
    );
}

/// **Finding: a signed registration never goes stale, so any stranger can replay it.**
///
/// `palw_class_registration_message_v2` (palw_state_v2.rs:3335) binds no nonce and no DAA other
/// than `activation_daa`, and the fold bounds `activation_daa` only from ABOVE
/// (`ClassActivationTooFarAhead`, palw_state_v2.rs:16936): a registration whose activation lies
/// thousands of DAA in the past folds. The acceptance walk (processor.rs:5663-5680) charges one of
/// the four per-block slots to any `ClassRegistered` whose registrant's ML-DSA signature verifies
/// and whose bond is Active — BEFORE `palw_v2_validate_objects` and the fold — so a replay of any
/// public registration (its own class already Active) is charged a slot, then refused by the fold
/// as `DuplicateClass` (shown here). Four replays per block, four carrier fees and no bond, and no
/// honest registration fits in the block.
#[test]
fn review12_1_a_public_registration_replays_for_ever_and_the_fold_refuses_it_only_after_the_slot() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    let s0 = genesis_with(&b, &[(REGISTRANT, 51_642_979_663_480)], &extras);
    let signed = registration(&b, &s0, 71_001, bond_key(REGISTRANT));
    let s1 = fold(&b, &s0, 2, 2, &[signed.clone()], &extras).expect("the honest registration folds");
    // The same bytes, 20,000 DAA later: the fold's only verdict is the class's status.
    let replay = fold(&b, &s1, 3, 20_000, &[signed.clone()], &extras);
    println!("replay of an Active class 20,000 DAA later: {replay:?}");
    assert!(matches!(replay, Err(PalwStateV2Error::DuplicateClass(_))), "{replay:?}");
    // And a registration whose activation lies 20,000 DAA in the past is not stale to the fold.
    let old = registration(&b, &s1, 71_002, bond_key(REGISTRANT));
    let PalwConsensusObjectV2::ClassRegistered { activation_daa, .. } = &old else { unreachable!() };
    assert_eq!(*activation_daa, 0);
    fold(&b, &s1, 3, 20_000, &[old], &extras).expect("an activation 20,000 DAA in the past folds: nothing bounds it from below");
}

/// **Finding (conditional): replaying a Dormant class's old registration re-registers it and
/// burns its registrant's bond again, without the registrant.** The fold admits `Dormant` as the
/// one re-registrable status (palw_state_v2.rs:16856) and charges the burn to
/// `carriage.registrant_bond`; the signature the acceptance layer checks is the original one.
#[test]
fn review12_1_a_dormant_class_replay_burns_the_registrant_again() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    let s0 = genesis_with(&b, &[(REGISTRANT, 51_642_979_663_480)], &extras);
    let signed = registration(&b, &s0, 72_001, bond_key(REGISTRANT));
    let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = &signed else { unreachable!() };
    let class_id = *class_id;
    let mut state = fold(&b, &s0, 2, 2, &[signed.clone()], &extras).expect("registers");
    let epoch = b.state.epoch_length();
    let epochs = u64::from(b.state.reclaim_epochs()) + 3;
    let (mut block, mut daa) = (2u64, 2u64);
    for _ in 0..epochs * 2 {
        daa += epoch / 2;
        block += 1;
        state = fold(&b, &state, block, daa, &[], &extras).expect("empty block");
        if matches!(state.class(&class_id).map(|c| &c.status), Some(PalwClassStatusV2::Dormant { .. })) {
            break;
        }
    }
    let status = state.class(&class_id).map(|c| c.status.clone());
    println!("class status after {daa} DAA of silence: {status:?}");
    if !matches!(status, Some(PalwClassStatusV2::Dormant { .. })) {
        println!("the class never went Dormant on this card; the replay-burn needs a Dormant class");
        return;
    }
    // The acceptance layer's M2-12 rule (processor.rs `initial_target != base_target.target`) is
    // what a replay must also pass there: print whether the base target moved in this idle run.
    let PalwConsensusObjectV2::ClassRegistered { initial_target, .. } = &signed else { unreachable!() };
    println!(
        "signed initial_target {initial_target}; base class live target now {:?} (equal: {})",
        state.class_target(&b.base_class_id).map(|t| t.target),
        state.class_target(&b.base_class_id).map(|t| t.target) == Some(*initial_target)
    );
    let before = state.bond(&bond_key(REGISTRANT)).unwrap().clone();
    block += 1;
    daa += 1;
    let after = fold(&b, &state, block, daa, &[signed], &extras).expect("the replay re-registers the Dormant class");
    let a = after.bond(&bond_key(REGISTRANT)).unwrap();
    println!("replay burned {} sompi of the registrant", before.collateral - a.collateral);
    assert_eq!(before.collateral - a.collateral, PALW_CLASS_REGISTRATION_BURN_SOMPI_V1);
}

/// **Residual of (c): a withdrawn bond's row is permanent and the floor it costs is recyclable.**
/// No tombstone (the implementer's own "remaining"), so the registry row outlives the retirement
/// and the withdrawal delay; the same 4,000,000 sompi registers the next key. Measures the rooted
/// bytes one floor bond leaves after it has retired and every delay has run out.
#[test]
fn review12_1_a_retired_floor_bond_row_is_permanent() {
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    let floor = palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true);
    let delay = match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.bond.withdrawal_delay_daa(),
        _ => unreachable!(),
    };
    let s0 = genesis_with(&b, &[], &extras);
    let n = 25u64; // 25 x floor = 1 MSK
    let regs: Vec<_> = (0..n)
        .map(|i| PalwConsensusObjectV2::BondRegistered {
            bond: bond_key(0xB000 + i),
            pubkey: vec![(i & 0xff) as u8; 2592],
            operator_pubkey: {
                let mut v = vec![0x11u8; 32];
                v[..8].copy_from_slice(&i.to_le_bytes());
                v
            },
            collateral: floor,
            payout_payload: h(0xB000 + i),
            capable_classes: Default::default(),
            signature: Vec::new(),
        })
        .collect();
    let s1 = fold(&b, &s0, 2, 2, &regs, &extras).expect("floor bonds register");
    let retire: Vec<_> =
        (0..n).map(|i| PalwConsensusObjectV2::BondRetireRequested { bond: bond_key(0xB000 + i), signature: vec![0; 8] }).collect();
    let s2 = fold(&b, &s1, 3, 3, &retire, &extras).expect("they retire");
    let far = 3 + 4 * delay + 100_000;
    let s3 = fold(&b, &s2, 4, far, &[], &extras).expect("long after every delay");
    let blob = |s: &PalwChainStateV2| {
        borsh::to_vec(&kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(s)).unwrap().len() as u64
    };
    let still = (0..n).filter(|i| s3.bond(&bond_key(0xB000 + i)).is_some()).count();
    let per = (blob(&s3) - blob(&s0)) / n;
    println!(
        "floor {floor} sompi, withdrawal delay {delay} DAA; after {far} DAA {still}/{n} retired rows still rooted, {per} B each; \
         {} B of permanent rooted state per MSK per withdrawal delay",
        per * (100_000_000 / floor)
    );
    assert_eq!(still as u64, n, "every retired floor bond's row stays rooted");
}
