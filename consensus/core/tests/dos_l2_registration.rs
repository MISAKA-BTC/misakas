//! dos_l2 — lane 2+3 audit (model-registration DoS, resource amplification) against testnet-12.
//!
//! Every number is produced by a `pub` consensus-core function on the t12 card. Nothing here
//! allocates from an attacker-controlled count: the largest structure built is a state with a few
//! thousand class rows (a few MB).
//!
//! **Converted to the fix (2026-09-24 DoS audit #12 (b)).** The fold runs t12's own audit fence
//! (`t12_extras` now carries `audit_2026_09_23_active`), under which a block folds at most
//! `PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1` = 4 bought registrations and each burns
//! `PALW_CLASS_REGISTRATION_BURN_SOMPI_V1` = 1 MSK of its registrant bond (collateral down, `slashed`
//! up — the burn obligation the bond's release spend must destroy). Genesis rows pay nothing and
//! count against nothing. The two held-hybrid measurements looked for a 40-layer genesis row t12 no
//! longer ships; they are re-targeted at the held hybrid a registrant can register on t12
//! (`qwen36_held_registration_v1` at n_ctx 512, the row t12 runs).

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{Params, palw_t12_shipped_params};
use kaspa_consensus_core::palw_class_admission_v2::{PalwClassAdmissionError, palw_admission_shape_at_v1, verify_class_admission_v9};
use kaspa_consensus_core::palw_economic_payout_v1::PalwEconomicPayoutFoldV1;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1, PalwModelRegistryFoldV1, palw_genesis_model_works_v1, palw_rc_typed_class_works_v1,
};
use kaspa_consensus_core::palw_reward_v2::PalwRewardParamsV2;
use kaspa_consensus_core::palw_state_v2::{
    PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1, PalwBlockContextV2, PalwBlockWorkV3,
    PalwBondKeyV2, PalwChainStateV2, PalwClassAdmissionCarriageV2, PalwConsensusObjectV2, PalwPwuRuleV2, PalwStateV2Error,
    PalwTransitionExtrasV1, apply_palw_transition_v7, palw_bond_burn_obligation_v2,
};
use kaspa_consensus_core::palw_step::{PalwShapeProfileV3, step_leaf_count_capped_v1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use kaspa_consensus_core::palw_work_target_v1::PalwWorkTargetFoldV1;
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
use std::time::Instant;

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
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
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
        // The fence t12 arms at genesis — and with it #12 (b)'s cap and burn.
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

/// The genesis classes' carriages, as the t12 bundle ships them.
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

/// The t12 admission gate, argument for argument as `processor.rs:6901` calls it, at DAA `daa`.
fn admit(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    daa: u64,
) -> Result<u64, PalwClassAdmissionError> {
    match admit_pwu(p, b, profile, job, daa, 1) {
        Err(PalwClassAdmissionError::PwuPerInferenceMismatch { counted, .. }) => admit_pwu(p, b, profile, job, daa, counted),
        other => other,
    }
}

fn admit_pwu(
    p: &Params,
    b: &PalwConsensusParamsV2,
    profile: &PalwShapeProfileV3,
    job: &PalwJobContextV2,
    daa: u64,
    pwu: u64,
) -> Result<u64, PalwClassAdmissionError> {
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

/// Mean wall time of `f` over `iters` runs, in microseconds.
fn time_us<T>(iters: u32, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut last = f();
    let start = Instant::now();
    for _ in 0..iters {
        last = f();
    }
    (start.elapsed().as_secs_f64() * 1e6 / iters as f64, last)
}

#[test]
fn dos_l2_t12_registration_terms() {
    let p = t12();
    let b = bundle_of(&p);
    let s = &b.state;
    println!("min_collateral_sompi            = {}", s.min_collateral_sompi());
    println!("registration_exposure_sompi     = {}", s.registration_exposure_sompi());
    println!("reclaim_epochs                  = {}", s.reclaim_epochs());
    println!("epoch_length (DAA)              = {}", s.epoch_length());
    println!("target_time_per_block (ms)      = {}", p.target_time_per_block());
    println!("fp_max_exposure_ratio_permille  = {}", s.fp_max_exposure_ratio_permille());
    println!("admission_independence          = {:?}", p.palw_admission_independence);
    println!("model_registry                  = {:?}", p.palw_model_registry.map(|f| f.daa_score()));
    println!("held_context armed at 0         = {}", p.palw_held_context_active_at(0));
    println!("court max_step_leaf_count       = {}", b.court.max_step_leaf_count());
    println!("max_block_mass                  = {:?}", p.max_block_mass);
    for (id, profile, job) in genesis_carriages(&b) {
        let carriage = PalwClassAdmissionCarriageV2 {
            profile: profile.clone(),
            canonical: job.clone(),
            registrant_bond: bond_key(ATTACKER),
            signature: vec![0u8; 4627],
        };
        let object = PalwConsensusObjectV2::ClassRegistered {
            class_id: id,
            artifact_root: h(1),
            slash_value_per_pwu: 5,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
            initial_target: 1,
            share_permille: 0,
            activation_daa: 0,
            admission: Some(Box::new(carriage.clone())),
        };
        let payload = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 { version: 1, object };
        println!(
            "class {id}: carriage-store row {} B; lifecycle payload {} B",
            borsh::to_vec(&carriage).unwrap().len(),
            borsh::to_vec(&payload).unwrap().len()
        );
        println!(
            "genesis class {id}: n_ctx {} layers {} held {} prefill {} decode {} carriage borsh {} B",
            profile.n_ctx,
            profile.layer_count,
            kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile),
            job.declared_prefill_tokens,
            job.exact_decode_tokens,
            borsh::to_vec(&profile).unwrap().len() + borsh::to_vec(&job).unwrap().len()
        );
    }
}

/// **What the registration gate costs every validating node, per object, on the shipped rows and on
/// the widest variant a registrant may declare.** The gate runs in `palw_v2_validate_objects`
/// (processor.rs:6901) for every `ClassRegistered` a block carries, and a refusal DROPS the object
/// while the block stands (processor.rs:5700) — so a refused registration costs the chain this CPU
/// and costs its carrier only the fee.
#[test]
fn dos_l2_admission_gate_cpu_per_registration() {
    let p = t12();
    let b = bundle_of(&p);
    for (id, profile, job) in genesis_carriages(&b) {
        let (us, verdict) = time_us(5, || admit(&p, &b, &profile, &job, 0));
        println!(
            "gate on genesis class {id} (n_ctx {}, layers {}): {:.1} us  -> {:?}",
            profile.n_ctx,
            profile.layer_count,
            us,
            verdict.map(|_| "ADMIT")
        );
        // The widest held variant: the same graph at the attention history bound (2^21).
        if kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile) {
            let mut wide = profile.clone();
            wide.n_ctx = kaspa_consensus_core::palw_state_chunk_map::palw_attn_history_bound_v1(&profile) as u32;
            let mut wjob = job.clone();
            wjob.shape_profile_id = wide.shape_profile_id();
            wjob.max_context_tokens = wide.n_ctx;
            let (us, verdict) = time_us(5, || admit(&p, &b, &wide, &wjob, 0));
            println!("   variant n_ctx {} : {:.1} us  -> {:?}", wide.n_ctx, us, verdict.map(|_| "ADMIT"));
            // A distinct class id per registration costs nothing: bump a free scalar.
            let mut v = profile.clone();
            v.n_threads = v.n_threads.wrapping_add(1).max(1);
            let mut vjob = job.clone();
            vjob.shape_profile_id = v.shape_profile_id();
            let (us, verdict) = time_us(5, || admit(&p, &b, &v, &vjob, 0));
            println!(
                "   variant n_threads {} (new id {}) : {:.1} us  -> {:?}",
                v.n_threads,
                v.shape_profile_id(),
                us,
                verdict.map(|_| "ADMIT")
            );
            // The widest layer count the shape admits (PALW_STEP_MAX_LAYERS = 1024), dtypes resized.
            let mut deep = profile.clone();
            deep.layer_count = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LAYERS;
            use kaspa_consensus_core::palw_step::PalwStepTableV1 as T;
            let spans = [
                (T::Pre, deep.table_layer_span(T::Pre)),
                (T::Gdn, deep.table_layer_span(T::Gdn)),
                (T::Attn, deep.table_layer_span(T::Attn)),
                (T::Post, deep.table_layer_span(T::Post)),
            ];
            for (t, span) in spans {
                let table = match t {
                    T::Pre => &mut deep.pre_nodes,
                    T::Gdn => &mut deep.gdn_nodes,
                    T::Attn => &mut deep.attn_nodes,
                    T::Post => &mut deep.post_nodes,
                };
                for n in table.iter_mut() {
                    if let Some(first) = n.weight_dtypes.first().copied() {
                        n.weight_dtypes = vec![first; span];
                    }
                }
            }
            let mut djob = job.clone();
            djob.shape_profile_id = deep.shape_profile_id();
            println!("   variant layers 1024 validate_shape: {:?}", deep.validate_shape());
            let (us, verdict) = time_us(3, || admit(&p, &b, &deep, &djob, 0));
            println!("   variant layers 1024 : {:.1} us  -> {:?}", us, verdict.map(|_| "ADMIT"));
        }
    }
}

/// A bought registration of a distinct floor variant (`n_threads = n` makes a new class id and the
/// root is fresh), registered by `registrant` with a carriage — the object the acceptance walk
/// admits once the registrant has signed it.
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

/// Fold `count` registrations of distinct floor variants (distinct class id, distinct root), each
/// with a carriage, `per_block` to a block, and return the state plus the bytes of each delta.
fn fold_registrations(
    p: &Params,
    b: &PalwConsensusParamsV2,
    base: &PalwChainStateV2,
    count: u64,
    per_block: u64,
    start_block: u64,
    template: &(Hash64, PalwShapeProfileV3, PalwJobContextV2),
    nonce0: u32,
) -> (PalwChainStateV2, u64, u64) {
    let extras = t12_extras(p, b);
    let mut state = base.clone();
    let mut delta_bytes = 0u64;
    let mut block = start_block;
    let mut folded = 0u64;
    let mut n = nonce0 + 1000;
    let floor_target = state.class_target(&b.base_class_id).map(|t| t.target).unwrap_or(u128::MAX);
    let floor_slash = state.class(&b.base_class_id).map(|c| c.slash_value_per_pwu).unwrap_or(5);
    while folded < count {
        let mut objects = Vec::new();
        for _ in 0..per_block.min(count - folded) {
            n += 1;
            objects.push(floor_variant(b, template, n, floor_slash, floor_target, bond_key(ATTACKER)));
        }
        let k = objects.len() as u64;
        let (next, delta, _) = apply_palw_transition_v7(
            &state,
            &b.state,
            None,
            &ctx(block, block, block),
            &objects,
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        )
        .expect("registrations fold");
        delta_bytes += borsh::to_vec(&delta).unwrap().len() as u64;
        state = next;
        folded += k;
        block += 1;
    }
    (state, delta_bytes, block)
}

fn genesis_state(p: &Params, b: &PalwConsensusParamsV2, collateral: u64) -> PalwChainStateV2 {
    genesis_state_under(b, collateral, &t12_extras(p, b))
}

/// The attacker's bond registered at the registration floor (the producer floor on t12,
/// `palw_bond_registration_floor_v1`), then carried at `net` sompi as if slashed down to it: the
/// shape a bond below the floor can only have on t12 after it registered.
fn genesis_state_worn_to(p: &Params, b: &PalwConsensusParamsV2, net: u64) -> PalwChainStateV2 {
    use kaspa_consensus_core::palw_state_v2::{PalwStateCarriageV2, palw_bond_registration_floor_v1};
    let floor = palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true);
    assert!(net <= floor, "worn DOWN to {net} from the floor {floor}");
    let s0 = genesis_state(p, b, floor);
    let mut carriage = PalwStateCarriageV2::from_state(&s0);
    let row = carriage.bonds.get_mut(&bond_key(ATTACKER)).expect("the attacker's bond");
    row.slashed += floor - net;
    row.collateral = net;
    carriage.into_state(&b.state, None).expect("consistent")
}

/// [`genesis_state`] folded under `extras` (the fence off, for the below-the-fence halves).
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
        extras,
    )
    .expect("genesis folds");
    state
}

/// **Rooted state per registration, what it costs every later block, and whether it ever leaves.**
#[test]
fn dos_l2_registration_rooted_state_growth_and_reclamation() {
    let p = t12();
    let b = bundle_of(&p);
    let floor = genesis_carriages(&b).into_iter().next().expect("floor carriage");
    println!("floor profile id == base class id: {}", floor.0 == b.base_class_id);
    // A bond at the minimum collateral: how many live registrations does it buy?
    let min = b.state.min_collateral_sompi();
    let price = b.state.registration_exposure_sompi();
    println!("a min bond ({min} sompi) buys {} live registrations at {price} sompi each", min / price.max(1));

    let collateral = 51_642_979_663_480u64; // the t12 genesis seat's collateral
    let s0 = genesis_state(&p, &b, collateral);
    let (root_us0, _) = time_us(20, || s0.state_root());
    let (clone_us0, _) = time_us(20, || s0.clone());
    println!("baseline: {} classes; state_root {:.1} us; clone {:.1} us", s0.classes_iter().count(), root_us0, clone_us0);
    let base_blob = borsh::to_vec(&kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&s0)).unwrap().len();
    println!("baseline snapshot blob {base_blob} B");

    let mut state = s0.clone();
    let mut block = 2u64;
    let mut total = 0u64;
    let mut total_delta = 0u64;
    // #12 (b): four bought registrations a block at most (they were folded fifty at a time).
    let per_block = PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1 as u64;
    for step in [100u64, 300, 400] {
        let (next, delta_bytes, nb) = fold_registrations(&p, &b, &state, step, per_block, block, &floor, total as u32);
        state = next;
        block = nb;
        total += step;
        total_delta += delta_bytes;
        let (root_us, _) = time_us(10, || state.state_root());
        let (clone_us, _) = time_us(10, || state.clone());
        // What every virtual resolution does to the whole state (store `set_tip_batch` + `load_tip`).
        let (ser_us, blob) =
            time_us(5, || borsh::to_vec(&kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&state)).unwrap());
        let expected = state.state_root();
        let (load_us, _) = time_us(3, || {
            borsh::from_slice::<kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2>(&blob)
                .unwrap()
                .into_state_v3(&b.state, Some(expected), true, p.palw_canonical_work_daa())
                .is_ok()
        });
        let extras_e = t12_extras(&p, &b);
        let (empty_us, _) = time_us(3, || {
            apply_palw_transition_v7(
                &state,
                &b.state,
                None,
                &ctx(block + 1, block + 1, block + 1),
                &[],
                PalwBlockWorkV3::None,
                &[],
                Hash64::default(),
                false,
                false,
                false,
                false,
                &extras_e,
            )
            .is_ok()
        });
        println!(
            "   snapshot blob {} B ({:.0} B/registration over baseline); set_tip serialize {:.1} us; load_tip decode+into_state {:.1} us; empty-block fold {:.1} us",
            blob.len(),
            (blob.len() as f64 - base_blob as f64) / total as f64,
            ser_us,
            load_us,
            empty_us
        );
        println!(
            "after {total} registrations: delta bytes/registration {:.0}; state_root {:.1} us (+{:.3} us/registration); clone {:.1} us (+{:.3} us/registration); registration_exposure {}",
            total_delta as f64 / total as f64,
            root_us,
            (root_us - root_us0) / total as f64,
            clone_us,
            (clone_us - clone_us0) / total as f64,
            state.registration_exposure(&bond_key(ATTACKER))
        );
    }
    let probe = state.classes_iter().filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER))).count();
    println!("attacker classes in rooted state: {probe}");
    // **#12 (b): every one of them burned 1 MSK of the registrant's bond, for good.** The exposure
    // comes back at reclamation; the burn does not — it is `slashed`, which the bond's release spend
    // must destroy (`palw_bond_burn_obligation_v2`), so it is never minted back to anyone.
    let before = s0.bond(&bond_key(ATTACKER)).unwrap().clone();
    let after = state.bond(&bond_key(ATTACKER)).unwrap().clone();
    let burned = total * PALW_CLASS_REGISTRATION_BURN_SOMPI_V1;
    println!("registrant burned {burned} sompi ({} MSK) for {total} registrations", burned / 100_000_000);
    assert_eq!(probe as u64, total);
    assert_eq!(before.collateral - after.collateral, burned, "each registration burns 1 MSK of collateral");
    assert_eq!(after.slashed - before.slashed, burned, "into `slashed`, the amount the release spend must destroy");
    assert_eq!(palw_bond_burn_obligation_v2(&after) - palw_bond_burn_obligation_v2(&before), burned);

    // Advance empty blocks across (reclaim_epochs + 2) epochs and see whether any leaves.
    let epoch = b.state.epoch_length();
    let epochs = u64::from(b.state.reclaim_epochs()) + 2;
    let extras = t12_extras(&p, &b);
    let mut daa = block;
    for _ in 0..epochs * 2 {
        daa += epoch / 2;
        block += 1;
        let (next, _, _) = apply_palw_transition_v7(
            &state,
            &b.state,
            None,
            &ctx(block, daa, block),
            &[],
            PalwBlockWorkV3::None,
            &[],
            Hash64::default(),
            false,
            false,
            false,
            false,
            &extras,
        )
        .expect("empty block folds");
        state = next;
    }
    let dormant = state
        .classes_iter()
        .filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER)))
        .filter(|(_, c)| matches!(c.status, kaspa_consensus_core::palw_state_v2::PalwClassStatusV2::Dormant { .. }))
        .count();
    let still = state.classes_iter().filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER))).count();
    let lifecycle_rows = state.model_lifecycles_iter().count();
    println!(
        "after {epochs} epochs ({} DAA) of silence: attacker rows still rooted {still}, of which Dormant {dormant}; registration_exposure {}; lifecycle rows {lifecycle_rows}",
        daa - 2,
        state.registration_exposure(&bond_key(ATTACKER))
    );
    // The reservation came back; the burn did not.
    assert_eq!(state.bond(&bond_key(ATTACKER)).unwrap().collateral, after.collateral, "idling returns no burned sompi");
    for (id, c) in state.classes_iter().filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER))).take(1) {
        println!(
            "sample attacker class {id}: status {:?}; share {:?}; lifecycle {:?}",
            c.status,
            state.class_share_permille(id),
            state.model_lifecycle(id).map(|r| r.state)
        );
    }
}

/// **The held hybrid a registrant can register on t12** — Qwen3.6 graph-v7 at n_ctx 512, the row
/// t12 runs (`qwen36_held_registration_v1`, the builder the add-a-model runbook uses). The
/// measurements below used to take it from the genesis list, which carried a 40-layer hybrid row
/// then; t12's genesis carries the floor and the two dense rows only, so it is built here the way a
/// registration builds it.
fn registered_hybrid(b: &PalwConsensusParamsV2) -> (Hash64, PalwShapeProfileV3, PalwJobContextV2) {
    let (profile, _, object) = kaspa_consensus_core::palw_qwen36_profile::qwen36_held_registration_v1(
        h(0x36A7),
        512,
        0,
        5,
        u128::MAX / 2,
        b,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        bond_key(ATTACKER),
    )
    .expect("the held hybrid row derives at 512");
    let PalwConsensusObjectV2::ClassRegistered { class_id, admission: Some(carriage), .. } = object else {
        unreachable!("a held registration carries its profile")
    };
    assert_eq!(profile.layer_count, carriage.profile.layer_count);
    (class_id, profile, carriage.canonical.clone())
}

/// `profile` with `layer_count = PALW_STEP_MAX_LAYERS` and every node's dtype list resized to the
/// new span — a legal shape (validate_shape passes), a distinct class id.
fn deepen(profile: &PalwShapeProfileV3, job: &PalwJobContextV2) -> (PalwShapeProfileV3, PalwJobContextV2) {
    use kaspa_consensus_core::palw_step::PalwStepTableV1 as T;
    let mut deep = profile.clone();
    deep.layer_count = kaspa_consensus_core::palw_step::PALW_STEP_MAX_LAYERS;
    let spans = [
        (T::Pre, deep.table_layer_span(T::Pre)),
        (T::Gdn, deep.table_layer_span(T::Gdn)),
        (T::Attn, deep.table_layer_span(T::Attn)),
        (T::Post, deep.table_layer_span(T::Post)),
    ];
    for (t, span) in spans {
        let table = match t {
            T::Pre => &mut deep.pre_nodes,
            T::Gdn => &mut deep.gdn_nodes,
            T::Attn => &mut deep.attn_nodes,
            T::Post => &mut deep.post_nodes,
        };
        for n in table.iter_mut() {
            if let Some(first) = n.weight_dtypes.first().copied() {
                n.weight_dtypes = vec![first; span];
            }
        }
    }
    let mut djob = job.clone();
    djob.shape_profile_id = deep.shape_profile_id();
    (deep, djob)
}

/// **Where the registration gate's CPU goes, and what the fold adds**, on the shipped held hybrid
/// row and on its 1024-layer twin. PwuPerInferenceMismatch is the gate's LAST check
/// (palw_class_admission_v2.rs:2454), so a registration that declares a wrong `pwu_per_inference`
/// on purpose costs every node the whole gate and is then dropped with the block standing.
#[test]
fn dos_l2_admission_breakdown_and_fold_cost() {
    use kaspa_consensus_core::palw_class_admission_v2::derive_court_cost_shaped_v1;
    let p = t12();
    let b = bundle_of(&p);
    let hybrid = registered_hybrid(&b);
    let (deep, djob) = deepen(&hybrid.1, &hybrid.2);
    let name0 = format!("hybrid@{}", hybrid.1.layer_count);
    for (name, profile, job) in [(name0.as_str(), hybrid.1.clone(), hybrid.2.clone()), ("hybrid@1024", deep.clone(), djob.clone())] {
        let (t_shape, shape) = time_us(3, || palw_admission_shape_at_v1(&p, &b, &profile, 0).unwrap());
        let (t_valid, _) = time_us(3, || profile.validate_shape());
        let (t_cov, _) = time_us(3, || kaspa_consensus_core::palw_catalog_coverage::verify_profile_coverage_v1(&profile));
        let form = kaspa_consensus_core::palw_prompt_ids_v1::palw_prompt_ids_form_of_class_v1(
            shape.court.map_or(kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat, |k| k.prompt_ids_form),
            &profile,
        );
        let (t_fit, _) = time_us(3, || {
            kaspa_consensus_core::palw_model_fit_v1::palw_model_fit_v2(
                &profile,
                &b,
                shape.court,
                form,
                kaspa_consensus_core::palw_model_fit_v1::palw_fit_regime_for_v1(shape.held, &profile),
            )
        });
        let (t_layout, _) =
            time_us(3, || kaspa_consensus_core::palw_state_chunk_map::palw_state_layout_v4(&profile, profile.n_ctx).is_ok());
        let ladder = shape.ladder.map(|r| r.ladder).unwrap_or(b.court.max_step_leaf_count());
        let (t_worst, worst) =
            time_us(3, || kaspa_consensus_core::palw_step::worst_case_step_leaf_count_deepest_job_capped_v1(&profile, ladder));
        let (t_count, counted) = time_us(3, || step_leaf_count_capped_v1(&profile, &job, ladder));
        let cost_shape = shape
            .ladder
            .map(|r| r.cost_shape)
            .unwrap()
            .with_decode_bound_v1(kaspa_consensus_core::palw_v2::PALW_V2_MAX_TRACE_EVENTS as u64);
        let (t_cost, _) = time_us(3, || derive_court_cost_shaped_v1(&profile, cost_shape).is_ok());
        let (t_work, _) =
            time_us(3, || kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&profile, &job));
        let (t_gate, verdict) = time_us(3, || admit_pwu(&p, &b, &profile, &job, 0, 1));
        println!(
            "{name}: shape+ladder {t_shape:.0} us | validate_shape {t_valid:.0} | coverage {t_cov:.0} | model_fit_v2 {t_fit:.0} | layout {t_layout:.0} | \
             worst-case count {t_worst:.0} ({worst:?}) | canonical count {t_count:.0} ({counted:?}) | court cost {t_cost:.0} | fold work {t_work:.0} || \
             whole gate refused-at-last-check {t_gate:.0} us -> {verdict:?}"
        );
        let payload = kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2 {
            version: 1,
            object: PalwConsensusObjectV2::ClassRegistered {
                class_id: profile.shape_profile_id(),
                artifact_root: h(1),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: 1 },
                initial_target: 1,
                share_permille: 0,
                activation_daa: 0,
                admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
                    profile: profile.clone(),
                    canonical: job.clone(),
                    registrant_bond: bond_key(ATTACKER),
                    signature: vec![0u8; 4627],
                })),
            },
        };
        println!("   {name}: lifecycle payload {} B", borsh::to_vec(&payload).unwrap().len());
    }
}

/// The same gate on the hybrid row widened in BOTH declared dimensions a held class may raise:
/// 1024 layers and the 2^21 attention-history bound, with the canonical job at the ladder's floor.
#[test]
fn dos_l2_admission_widest_hybrid() {
    let p = t12();
    let b = bundle_of(&p);
    let hybrid = registered_hybrid(&b);
    for (layers_deep, n_ctx) in [(false, 1u32 << 21), (true, 1 << 16), (true, 1 << 21)] {
        let (mut prof, mut job) = if layers_deep { deepen(&hybrid.1, &hybrid.2) } else { (hybrid.1.clone(), hybrid.2.clone()) };
        prof.n_ctx = n_ctx;
        let shape = palw_admission_shape_at_v1(&p, &b, &prof, 0).unwrap();
        let floor = shape.ladder.map(|r| r.canonical_footprint_floor).unwrap_or(1);
        job.shape_profile_id = prof.shape_profile_id();
        job.max_context_tokens = n_ctx;
        job.declared_prefill_tokens = (floor as u32).max(job.declared_prefill_tokens).min(n_ctx - 2);
        job.exact_decode_tokens = 2;
        let (t, v) = time_us(1, || admit_pwu(&p, &b, &prof, &job, 0, 1));
        println!(
            "hybrid layers {} n_ctx {n_ctx} prefill {}: gate {:.0} us -> {:?}",
            prof.layer_count, job.declared_prefill_tokens, t, v
        );
    }
}

/// **On t12 the "materialization cap" IS the held ladder.** Every whole-capture prover in the engine
/// (`misaka-palw-base0::fp_interval::base0_whole_capture_refusal_v1(leaves, materialize_cap,
/// class_ladder)`) refuses a capture past `materialize_cap = network_ladder =
/// bundle.court.max_step_leaf_count()` and hands it to the streamed routes. That refusal was sized
/// for a network ladder of 2^26 under a held class ladder of 2^40. t12 mints the court at 2^40, so the
/// two ladders are one number and the refusal can never fire for a claim the class ladder admits:
/// every folded held capture a seat samples is re-executed DENSE, with the dense sink's leaf vector
/// and tiles, and nothing in that path reserves memory.
#[test]
fn dos_l2_t12_materialization_cap_equals_the_held_ladder() {
    use kaspa_consensus_core::palw_resource_profile_v1::{palw_dense_capture_bytes_v1, palw_profile_max_tile_len_v1};
    use kaspa_consensus_core::palw_state_chunk_map::{PALW_HELD_STEP_LADDER_V1, palw_class_step_ladder_v1};
    let p = t12();
    let b = bundle_of(&p);
    let network = b.court.max_step_leaf_count();
    println!(
        "t12 network ladder (materialize_cap) = {network} = 2^{:.1}; PALW_HELD_STEP_LADDER_V1 = {PALW_HELD_STEP_LADDER_V1}",
        (network as f64).log2()
    );
    assert_eq!(network, PALW_HELD_STEP_LADDER_V1, "t12's network ladder is the held ladder");
    println!("fp_certified_classes = {:?}", b.state.fp_certified_classes().map(|s| s.len()));
    const GIB: f64 = (1u64 << 30) as f64;
    for (id, profile, job) in genesis_carriages(&b) {
        let class_ladder = palw_class_step_ladder_v1(network, &profile);
        let tile = palw_profile_max_tile_len_v1(&profile);
        let canonical_leaves = step_leaf_count_capped_v1(&profile, &job, class_ladder).unwrap_or(0);
        // The deepest single-call-free job the class admits: prefill n_ctx - 1, one decode.
        let mut deep = job.clone();
        deep.declared_prefill_tokens = profile.n_ctx.saturating_sub(1);
        deep.exact_decode_tokens = 1;
        let deep_leaves = step_leaf_count_capped_v1(&profile, &deep, class_ladder).unwrap_or(0);
        // `base0_whole_capture_refusal_v1`, as written: refuse iff leaves == 0 || leaves > class_ladder
        // || leaves > materialize_cap.
        let refused = |l: u64| l == 0 || l > class_ladder || l > network;
        println!(
            "class {}: held {} class ladder 2^{:.0}; canonical {} leaves -> whole-capture refusal {} -> dense re-exec {:.2} GiB; \
             deepest job {} leaves -> refusal {} -> dense {:.1} GiB",
            &id.to_string()[..8],
            kaspa_consensus_core::palw_state_chunk_map::palw_profile_is_held_v4(&profile),
            (class_ladder as f64).log2(),
            canonical_leaves,
            refused(canonical_leaves),
            palw_dense_capture_bytes_v1(canonical_leaves, tile) as f64 / GIB,
            deep_leaves,
            refused(deep_leaves),
            palw_dense_capture_bytes_v1(deep_leaves, tile) as f64 / GIB,
        );
    }
}

/// Which rooted collections ONE bought registration writes, and how many bytes each row is.
#[test]
fn dos_l2_one_registration_rows() {
    let p = t12();
    let b = bundle_of(&p);
    let floor = genesis_carriages(&b).into_iter().next().expect("floor");
    let s0 = genesis_state(&p, &b, 51_642_979_663_480);
    let extras = t12_extras(&p, &b);
    let mut prof = floor.1.clone();
    prof.n_threads = 7777;
    let mut job = floor.2.clone();
    job.shape_profile_id = prof.shape_profile_id();
    let leaves = step_leaf_count_capped_v1(&prof, &job, b.court.max_step_leaf_count()).unwrap();
    let object = PalwConsensusObjectV2::ClassRegistered {
        class_id: prof.shape_profile_id(),
        artifact_root: h(0xFEED),
        slash_value_per_pwu: s0.class(&b.base_class_id).unwrap().slash_value_per_pwu,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: s0.class_target(&b.base_class_id).unwrap().target,
        share_permille: 0,
        activation_daa: 0,
        admission: Some(Box::new(PalwClassAdmissionCarriageV2 {
            profile: prof,
            canonical: job,
            registrant_bond: bond_key(ATTACKER),
            signature: vec![0; 4627],
        })),
    };
    let (_, delta, _) = apply_palw_transition_v7(
        &s0,
        &b.state,
        None,
        &ctx(2, 2, 2),
        &[object],
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    )
    .unwrap();
    for e in &delta.entries {
        let dbg = format!("{e:?}");
        let kind: String = dbg.chars().take_while(|c| c.is_alphanumeric()).collect();
        println!("row {kind}: {} B", borsh::to_vec(e).unwrap().len());
    }
}

/// One block of `objects` on `base` under `extras`, the fold's verdict returned.
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

/// `n` bought floor-variant registrations by the attacker, class ids distinct from every other test.
fn bought(b: &PalwConsensusParamsV2, base: &PalwChainStateV2, first: u32, n: u32) -> Vec<PalwConsensusObjectV2> {
    let floor = genesis_carriages(b).into_iter().next().expect("floor");
    let slash = base.class(&b.base_class_id).unwrap().slash_value_per_pwu;
    let target = base.class_target(&b.base_class_id).unwrap().target;
    (first..first + n).map(|k| floor_variant(b, &floor, k, slash, target, bond_key(ATTACKER))).collect()
}

/// **#12 (b): four bought registrations fold in one block; a fifth is refused** — the fold's second
/// lock on the number the acceptance walk drops at (`PALW_CLASS_REGISTRATION_MAX_PER_BLOCK_V1`).
/// Below the fence the same five fold. Genesis rows never count: t12's genesis list folds its three
/// classes in one block and burns nothing.
///
/// Fails without the fix: five fold past the fence.
#[test]
fn dos_l2_fix12_four_registrations_a_block_and_not_five() {
    let p = t12();
    let b = bundle_of(&p);
    let s0 = genesis_state(&p, &b, 51_642_979_663_480);
    let extras = t12_extras(&p, &b);
    assert!(extras.audit_2026_09_23_active, "t12 arms the audit fence");
    let four = bought(&b, &s0, 50_000, 4);
    let s4 = fold_one(&b, &s0, 2, &four, &extras).expect("four bought registrations fold");
    assert_eq!(s4.classes_iter().filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER))).count(), 4);
    let five = bought(&b, &s0, 50_100, 5);
    let refused = fold_one(&b, &s0, 2, &five, &extras);
    assert!(
        matches!(refused, Err(PalwStateV2Error::ClassRegistrationsPerBlockExceeded { max: 4, .. })),
        "the fifth is refused: {refused:?}"
    );
    // The next block has its own four.
    let s8 = fold_one(&b, &s4, 3, &bought(&b, &s4, 50_200, 4), &extras).expect("the next block folds four more");
    assert_eq!(s8.classes_iter().filter(|(_, c)| c.registrant_bond == Some(bond_key(ATTACKER))).count(), 8);
    // Below the fence nothing changed.
    let dormant = PalwTransitionExtrasV1 { audit_2026_09_23_active: false, settled_anchor_depth: None, ..extras.clone() };
    let s0d = genesis_state_under(&b, 51_642_979_663_480, &dormant);
    fold_one(&b, &s0d, 2, &bought(&b, &s0d, 50_300, 5), &dormant).expect("below the fence five fold");
}

/// **#12 (b): a bought registration burns 1 MSK of its registrant's bond, and the burn is in the
/// supply accounting** — collateral down by exactly `SOMPI_PER_KASPA`, `slashed` (the burn
/// obligation the release spend must destroy, never minted back) up by the same; the exposure
/// reservation is separate and returns at reclamation. A genesis registration burns nothing, a
/// registrant that cannot pay the burn on top of what it backs is refused, and below the fence
/// nothing is burned.
///
/// Fails without the fix: the registrant's collateral and burn obligation do not move.
#[test]
fn dos_l2_fix12_a_bought_registration_burns_one_msk() {
    let p = t12();
    let b = bundle_of(&p);
    assert_eq!(PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, kaspa_consensus_core::constants::SOMPI_PER_KASPA);
    assert_eq!(PALW_CLASS_REGISTRATION_BURN_SOMPI_V1, 100_000_000);
    let extras = t12_extras(&p, &b);
    let s0 = genesis_state(&p, &b, 51_642_979_663_480);
    // Genesis: the three t12 classes registered in one block, and not a sompi burned by anyone.
    for (_, bond) in s0.bonds_iter() {
        assert_eq!(bond.slashed, 0, "a genesis registration burns nothing");
    }
    let before = s0.bond(&bond_key(ATTACKER)).unwrap().clone();
    let s1 = fold_one(&b, &s0, 2, &bought(&b, &s0, 60_000, 1), &extras).expect("one bought registration");
    let after = s1.bond(&bond_key(ATTACKER)).unwrap().clone();
    assert_eq!(before.collateral - after.collateral, 100_000_000, "1 MSK leaves the registrant's collateral");
    assert_eq!(after.slashed - before.slashed, 100_000_000, "and is recorded as destroyed");
    assert_eq!(
        palw_bond_burn_obligation_v2(&after),
        palw_bond_burn_obligation_v2(&before) + 100_000_000,
        "the release spend must burn it"
    );
    assert_eq!(
        s1.registration_exposure(&bond_key(ATTACKER)) - s0.registration_exposure(&bond_key(ATTACKER)),
        u128::from(b.state.registration_exposure_sompi()),
        "the reservation is taken beside the burn, not instead of it"
    );
    // A registrant that can post the reservation but not the burn on top of it is refused, whole.
    // Past the t12 merge a bond REGISTERS at the producer floor (13,000 MSK, far above
    // 1 MSK + the price), so the poor/exact bond is one registered at the floor and later worn down
    // (slashed) to the tested net collateral — the arithmetic the check reads is the same.
    let price = b.state.registration_exposure_sompi();
    let poor = genesis_state_worn_to(&p, &b, 100_000_000 + price - 1);
    let refused = fold_one(&b, &poor, 2, &bought(&b, &poor, 60_100, 1), &extras);
    assert!(
        matches!(refused, Err(PalwStateV2Error::ClassRegistrationBurnUnaffordable { burn: 100_000_000, already: 0, .. })),
        "{refused:?}"
    );
    let exact = genesis_state_worn_to(&p, &b, 100_000_000 + price);
    let s = fold_one(&b, &exact, 2, &bought(&b, &exact, 60_200, 1), &extras).expect("exactly the burn plus the reservation");
    assert_eq!(s.bond(&bond_key(ATTACKER)).unwrap().collateral, price, "what is left backs the reservation");
    // Below the fence: no burn.
    let dormant = PalwTransitionExtrasV1 { audit_2026_09_23_active: false, settled_anchor_depth: None, ..extras.clone() };
    let d0 = genesis_state_under(&b, 51_642_979_663_480, &dormant);
    let d1 = fold_one(&b, &d0, 2, &bought(&b, &d0, 60_300, 1), &dormant).expect("folds below the fence");
    assert_eq!(d1.bond(&bond_key(ATTACKER)).unwrap().collateral, 51_642_979_663_480, "below the fence nothing is burned");
}

/// **Review of #12: the registration burn must leave every LIVE slashable lock covered.**
///
/// A seat's Valid lock is admitted against `collateral − live locks`, not against the exposure
/// reservations the burn's first check sums, so a bond holding 10 MSK under a live 9.9996 MSK lock
/// used to pay the burn, fall to 9 MSK, and leave a later `PanelFalseValid` on that lock only what
/// was left to take (`slash_bond` clamps). Past the fence it is refused
/// (`ClassRegistrationBurnUnaffordable`, `already` = the live locked sum); a lock that leaves room
/// for exactly the burn folds; a lock dead on both clocks does not count; below the fence nothing is
/// burned and nothing is asked.
///
/// Fails without the fix: the first registration folds and the bond ends below its live lock.
#[test]
fn dos_l2_fix12_review_the_burn_leaves_live_locks_covered() {
    use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
    use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
    let p = t12();
    let b = bundle_of(&p);
    let extras = t12_extras(&p, &b);
    assert!(extras.audit_2026_09_23_active);
    // 10 MSK, or the registration floor where that is higher (the producer floor, 13,000 MSK on
    // testnet-12's regenesis params): every amount below is relative to it.
    let collateral: u64 = (10 * 100_000_000u64)
        .max(kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true));
    let burn = PALW_CLASS_REGISTRATION_BURN_SOMPI_V1 as u128;
    let with_lock = |extras: &PalwTransitionExtrasV1, amount: u128, expiry_daa: u64| {
        let s0 = genesis_state_under(&b, collateral, extras);
        let mut carriage = PalwStateCarriageV2::from_state(&s0);
        carriage.slashable_locks.insert(
            (bond_key(ATTACKER), h(0x10C4)),
            PalwSlashableLockV1 { claim: h(0x10C4), amount, expiry_daa, settled_at_final: 0 },
        );
        carriage.into_state(&b.state, None).expect("consistent")
    };
    // A live lock on all but the reservation: the burn would eat into it — refused, whole.
    let price = b.state.registration_exposure_sompi() as u128;
    let s0 = with_lock(&extras, collateral as u128 - price, 1_000_000);
    let refused = fold_one(&b, &s0, 2, &bought(&b, &s0, 90_000, 1), &extras);
    match &refused {
        Err(PalwStateV2Error::ClassRegistrationBurnUnaffordable { already, collateral: c, .. }) => {
            assert_eq!(*already, collateral as u128 - price, "`already` names the live locked sum");
            assert_eq!(*c, collateral);
        }
        other => panic!("the burn must not eat a live lock: {other:?}"),
    }
    // Exactly the burn's room beside the lock: folds, and the lock is still fully backed.
    let s0 = with_lock(&extras, collateral as u128 - burn, 1_000_000);
    let s1 = fold_one(&b, &s0, 2, &bought(&b, &s0, 90_100, 1), &extras).expect("the burn fits beside the lock");
    assert_eq!(s1.bond(&bond_key(ATTACKER)).unwrap().collateral as u128, collateral as u128 - burn);
    // A lock dead on both clocks and past its horizon stands behind nothing.
    let dead = with_lock(&extras, collateral as u128 - price, 1);
    let far = 1 + 2 * b.state.window_court() + 10;
    let ok = apply_palw_transition_v7(
        &dead,
        &b.state,
        None,
        &ctx(2, far, 2),
        &bought(&b, &dead, 90_200, 1),
        PalwBlockWorkV3::None,
        &[],
        Hash64::default(),
        false,
        false,
        false,
        false,
        &extras,
    );
    assert!(ok.is_ok(), "an expired lock does not hold the burn back: {:?}", ok.err());
    // Below the fence: no burn, so nothing to refuse.
    let dormant = PalwTransitionExtrasV1 { audit_2026_09_23_active: false, settled_anchor_depth: None, ..extras.clone() };
    let d0 = with_lock(&dormant, collateral as u128 - price, 1_000_000);
    let d1 = fold_one(&b, &d0, 2, &bought(&b, &d0, 90_300, 1), &dormant).expect("below the fence it folds as before");
    assert_eq!(d1.bond(&bond_key(ATTACKER)).unwrap().collateral, collateral, "and nothing is burned");
}

/// **Review of #12: the registry's RPC twin of readiness reads net collateral too.**
/// `palw_seat_not_ready_reason_v1` / `palw_model_registry_ready_seats_v1` say they answer "as the
/// fold counts them"; past the fence (the t12 bundle carries its copy) a bond that paid a
/// registration burn and holds the bar net is ready there as it is in the fold.
///
/// Fails without the fix: the registrant reads "collateral short" while the control, holding the
/// identical collateral, is ready.
#[test]
fn dos_l2_fix12_review_the_readiness_view_does_not_count_the_burn_twice() {
    use kaspa_consensus_core::palw_model_registry_v1::{PalwSeatReadinessRowV1, palw_seat_not_ready_reason_v1};
    let p = t12();
    let b = bundle_of(&p);
    assert!(b.state.bond_collateral_is_net_v1(), "the t12 bundle carries the audit fence's copy");
    let extras = t12_extras(&p, &b);
    // 1.2 MSK (after the burn 0.2 MSK, far above the old readiness bar) — or, where the
    // registration floor and the readiness bar are higher (the producer floor, 13,000 MSK on
    // testnet-12's regenesis params), exactly the bar plus the reservation plus the burn, so the
    // registrant holds the bar net only if the burn is counted once.
    let fold_v = registry_fold(&p, &b);
    let needed = u128::from(b.state.min_collateral_sompi()) * u128::from(fold_v.globals.readiness_collateral_multiple);
    let reg_floor = kaspa_consensus_core::palw_state_v2::palw_bond_registration_floor_v1(b.state.min_collateral_sompi(), true);
    let registrant = 120_000_000u64.max(
        u64::try_from(needed).unwrap().max(reg_floor) + b.state.registration_exposure_sompi() + PALW_CLASS_REGISTRATION_BURN_SOMPI_V1,
    );
    let s0 = genesis_state(&p, &b, registrant);
    let s1 = fold_one(&b, &s0, 2, &bought(&b, &s0, 91_000, 1), &extras).expect("the registration folds");
    let r = s1.bond(&bond_key(ATTACKER)).unwrap();
    assert_eq!(r.slashed, PALW_CLASS_REGISTRATION_BURN_SOMPI_V1);
    let free = u128::from(r.collateral) - s1.registration_exposure(&bond_key(ATTACKER));
    assert!(free >= needed, "the premise: the registrant holds the bar ({free} >= {needed})");
    let now = 3u64;
    let row = PalwSeatReadinessRowV1 { proved_daa: now, proved_span: 0, leaf_index: 0, proof_version: 2, chunks: 8 };
    assert_eq!(palw_seat_not_ready_reason_v1(&s1, &b.state, &bond_key(ATTACKER), &row, now, &fold_v), None);
}
