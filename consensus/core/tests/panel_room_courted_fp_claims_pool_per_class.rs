//! **Free-prompt claims under a court pool with the class's pending ones before they are counted in
//! whole jobs** — 2026-09-24 audit #4 review item 3.
//!
//! The rate rule charges a class's owed free-prompt claims as `⌈Σ quanta / per_job⌉` jobs (the #11
//! pooling). At e93be0f2 the court add-back charged each LICENSED claim with a court open on it on
//! its own, `⌈quanta / per_job⌉` each: eight one-quantum claims, one job while pending, released at
//! licence, came back as EIGHT jobs with a court on each — 8× what the same quanta ever cost, at a
//! per-court reserve about 8× lower than a whole-job claim's. Past the review the courted claims are
//! pooled with the pending ones per class, so they come back as the one job they were.
//!
//! Measured through the real fold on testnet-12's params: a model class whose window is under
//! ADR-0152's C7 threshold (so a licence releases it, T-2(a)), one-quantum commitments,
//! bound, licensed, and a `CourtOpened` on each; what the class owes is read with
//! `palw_panel_demand_read_v1` (the read the gate and op 186 share). Also: two courts on one
//! licensed claim charge it once.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_courted_fp_claims_pool_per_class
#![allow(dead_code)]

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_SETTLED_ANCHOR_DEPTH, Params};
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1;
use kaspa_consensus_core::palw_court_v2::court_session_id_v2;
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwPanelSeatV2,
    PalwPwuRuleV2, PalwStateCarriageV2, PalwTransitionExtrasV1, apply_palw_transition_v7, palw_inflight_claims_counted_v1,
    palw_operator_id_v2, palw_panel_demand_read_v1,
};
use kaspa_consensus_core::palw_work_target_v1::{palw_panel_demand_term_v1, palw_panel_held_to_final_v1};
use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

fn bundle(p: &Params) -> PalwConsensusParamsV2 {
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(b) => b.clone(),
        _ => panic!("testnet-12 is ConsensusV2"),
    }
}

fn floor_row(b: &PalwConsensusParamsV2) -> (Hash64, u64, u128, u64) {
    for o in b.genesis_objects.iter() {
        if let PalwConsensusObjectV2::ClassRegistered {
            class_id,
            pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference },
            initial_target,
            slash_value_per_pwu,
            ..
        } = o
            && *class_id == b.base_class_id
        {
            return (*class_id, *pwu_per_inference, *initial_target, *slash_value_per_pwu);
        }
    }
    panic!("t12 registers a DerivedV1 floor")
}

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xC0_0000 + n), index: 0 })
}

fn pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(1); 32]
}

fn op_pubkey(n: u64) -> Vec<u8> {
    vec![(n as u8).wrapping_add(101); 32]
}

fn register_bond(n: u64, collateral: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::BondRegistered {
        bond: bond_key(n),
        pubkey: pubkey(n),
        operator_pubkey: op_pubkey(n),
        collateral,
        payout_payload: h(0x9A00 + n),
        capable_classes: Default::default(),
        signature: Vec::new(),
    }
}

fn ctx(daa: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(0xC000_0000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 }
}

fn fp_commitment(class_id: Hash64, leaves: u64, n: u64, seed: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF0_0000 + seed),
        class_id,
        bond: bond_key(n),
        executor_pubkey: pubkey(n),
        work_leaves: leaves,
        prompt_token_ids_hash: h(0x7E_0000 + seed),
        prompt_tokens: 0,
        prompt_token_ids: Vec::new(),
        decode_tokens_executed: 1,
        trace_root: h(0x1F00_0000 + seed),
        output_root: h(0x2F00_0000 + seed),
        execution_root: h(0x3F00_0000 + seed),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1::genesis(class_id),
    }
}

fn seats(ns: &[u64]) -> Vec<PalwPanelSeatV2> {
    ns.iter().map(|n| PalwPanelSeatV2 { bond: bond_key(*n), operator_id: palw_operator_id_v2(&op_pubkey(*n)) }).collect()
}

fn receipts(claim: Hash64, ns: &[u64]) -> Vec<PalwSeatReceiptV2> {
    ns.iter()
        .map(|n| PalwSeatReceiptV2 {
            claim,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: bond_key(*n),
            signed_daa: 0,
            signature: Vec::new(),
        })
        .collect()
}

fn court(s: &PalwChainStateV2, claim: Hash64, challenger: u64) -> PalwConsensusObjectV2 {
    let c = s.claim(&claim).unwrap();
    let space = PalwBisectSpaceV1::TraceEvents;
    let space_size = 16;
    let session_id = court_session_id_v2(&claim, &c.trace_root, &c.bond, &bond_key(challenger), space, space_size);
    PalwConsensusObjectV2::CourtOpened {
        session_id,
        claim,
        challenger_bond: bond_key(challenger),
        space,
        space_size,
        signature: Vec::new(),
    }
}

/// The class's term of the per-span demand (scaled), as the gate and op 186 read it.
fn demand(s: &PalwChainStateV2, sp: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2, seat_count: u32, class: Hash64) -> u128 {
    palw_panel_demand_read_v1(s, sp, seat_count).1.get(&class).copied().unwrap_or(0)
}

/// The claims (whole jobs) the class owes.
fn owed_jobs(
    s: &PalwChainStateV2,
    sp: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2,
    seat_count: u32,
    class: Hash64,
) -> u128 {
    palw_panel_demand_read_v1(s, sp, seat_count).0.get(&class).copied().unwrap_or(0)
}

#[test]
fn courts_on_licensed_one_quantum_claims_charge_the_one_job_the_claims_pool_to() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1;
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleV1, PalwModelRegistryFoldV1, PalwModelWorkV1,
    };
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let floor = floor_row(&b);
    let certified = sp.fp_certified_classes().expect("t12 lane-certifies its classes");
    let (model_id, model_rule, model_target) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, .. }
                if *class_id != b.base_class_id && certified.contains(class_id) =>
            {
                Some((*class_id, pwu_rule.clone(), *initial_target))
            }
            _ => None,
        })
        .expect("t12 lane-certifies a model class");
    let span = 1_000;
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let seat_count = b.panel.seat_count() as u128;
    let work = PalwModelWorkV1 { verification_ccu: 1_000, economic_ccu_per_claim: 500, ops_supported: true, ..Default::default() };
    let registry = PalwModelRegistryFoldV1 {
        globals,
        span_daa: span,
        genesis_works: [(floor.0, work), (model_id, work)].into_iter().collect(),
        grace_until_daa: 0,
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    };
    let extras = PalwTransitionExtrasV1 {
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        panel_economy_active: true,
        objective_offence_daa: Some(0),
        settled_anchor_depth: Some(PALW_T12_SETTLED_ANCHOR_DEPTH),
        model_registry: Some(registry.clone()),
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: span, ..Default::default() }),
        ..Default::default()
    };
    let fold = |s: &PalwChainStateV2, daa: u64, objs: &[PalwConsensusObjectV2]| {
        apply_palw_transition_v7(
            s,
            sp,
            Some(&b.admission),
            &ctx(daa),
            objs,
            PalwBlockWorkV3::None,
            &[],
            h(0xE0 + daa),
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap_or_else(|e| panic!("fold at {daa}: {e:?}"))
        .0
    };
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id: floor.0,
        artifact_root: h(0xA27),
        slash_value_per_pwu: floor.3,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: floor.1 },
        initial_target: floor.2,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    for n in 1..=8u64 {
        objects.push(register_bond(n, big));
    }
    objects.push(PalwConsensusObjectV2::ClassRegistered {
        class_id: model_id,
        artifact_root: h(0xB27),
        slash_value_per_pwu: floor.3,
        pwu_rule: model_rule.clone(),
        initial_target: model_target,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    });
    let s1 = fold(&PalwChainStateV2::genesis(), 1, &objects);
    let s2 = fold(&s1, 1_000, &[]);
    let mut c = PalwStateCarriageV2::from_state(&s2);
    let row = c.model_lifecycles.get_mut(&model_id).expect("the model row");
    row.state = PalwModelLifecycleV1::Active;
    row.profile.max_inflight_claims = 64;
    let window = row.profile.verification_window_spans as u64;
    let s = c.into_state_v3(sp, None, false, None).expect("rebuilds");
    assert!(
        !palw_panel_held_to_final_v1(s.model_lifecycle(&model_id).unwrap()),
        "the premise: a window under 1,000 spans, the class is not held to Final, so a licence releases it"
    );
    let per_job = sp.fp_quanta_per_canonical_job() as u64;
    let canonical = model_rule.canonical_leaves_v1();
    let one = kaspa_consensus_core::palw_freeprompt_v3::fp_class_quantum_leaves_v1(canonical, sp.fp_quanta_per_canonical_job());

    // `per_job` one-quantum commitments by bond 1: one whole job, pooled.
    let n = per_job;
    let commits: Vec<_> = (0..n).map(|i| fp_commitment(model_id, one, 1, 0x300 + i)).collect();
    let mut st = fold(&s, 1_001, &commits);
    let ids: Vec<Hash64> = (0..n).map(|i| h(0xF0_0300 + i)).collect();
    for id in &ids {
        let q = match &st.claim(id).unwrap().source {
            kaspa_consensus_core::palw_state_v2::PalwClaimSourceV2::FreePrompt { quanta, .. } => *quanta,
            _ => panic!(),
        };
        assert_eq!(q, 1, "a one-quantum commitment");
    }
    let unit = palw_panel_demand_term_v1(1, 500 * seat_count, window);
    let jobs = |s: &PalwChainStateV2| demand(s, sp, seat_count as u32, model_id) as f64 / unit as f64;
    let pooled = jobs(&st);

    let panel = [2u64, 3, 4, 5, 6];
    let binds: Vec<_> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| PalwConsensusObjectV2::PanelBound { claim: *id, anchor: h(0xAB00 + i as u64), seats: seats(&panel) })
        .collect();
    st = fold(&st, 1_002, &binds);
    let lic: Vec<_> =
        ids.iter().map(|id| PalwConsensusObjectV2::ReceiptLicensed { claim: *id, receipts: receipts(*id, &panel) }).collect();
    st = fold(&st, 1_003, &lic);
    for id in &ids {
        assert!(matches!(st.claim(id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed");
    }
    let licensed = jobs(&st);

    // A court on each licensed one-quantum claim (challenger bond 7).
    let courts: Vec<_> = ids.iter().map(|id| court(&st, *id, 7)).collect();
    st = fold(&st, 1_004, &courts);
    let courted = jobs(&st);
    let challenger_reserved = st.reserved_exposure(&bond_key(7));
    let one_claim_reserved = st.claim(&ids[0]).unwrap().reserved;

    // What the rule before the licence release (every live claim, pooled, to Final) charges for them.
    let to_final = palw_inflight_claims_counted_v1(0, n, n, per_job, true);

    println!("class {model_id}: window {window} spans, per_job {per_job}");
    println!(
        "{n} one-quantum FP claims: unlicensed (pooled) = {pooled:.3} jobs; licensed = {licensed:.3}; licensed + a court on each = {courted:.3} jobs"
    );
    println!("the same {n} claims counted pooled to Final = {to_final} job(s)");
    println!("challenger reserved {challenger_reserved} sompi for {n} courts ({one_claim_reserved} each)");
    assert!((pooled - 1.0).abs() < 1e-6, "pooled: one job");
    assert!(licensed.abs() < 1e-6, "licensed: released");
    assert_eq!(to_final, 1);
    assert!(
        (courted - to_final as f64).abs() < 1e-6,
        "courts on {n} one-quantum licensed FP claims charge the {to_final} job the same claims pool to, not {n}: got {courted}"
    );
    assert_eq!(owed_jobs(&st, sp, seat_count as u32, model_id), 1, "the class owes one job");
}

#[test]
fn two_courts_on_one_licensed_claim_charge_it_once() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecLaneFoldV1;
    use kaspa_consensus_core::palw_model_registry_v1::{
        PALW_REGISTRY_GLOBALS_V1, PalwModelLifecycleV1, PalwModelRegistryFoldV1, PalwModelWorkV1,
    };
    let p = t12();
    let b = bundle(&p);
    let sp = &b.state;
    let floor = floor_row(&b);
    let certified = sp.fp_certified_classes().expect("t12 lane-certifies its classes");
    let (model_id, model_rule, model_target) = b
        .genesis_objects
        .iter()
        .find_map(|o| match o {
            PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, .. }
                if *class_id != b.base_class_id && certified.contains(class_id) =>
            {
                Some((*class_id, pwu_rule.clone(), *initial_target))
            }
            _ => None,
        })
        .unwrap();
    let span = 1_000;
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let seat_count = b.panel.seat_count() as u128;
    let work = PalwModelWorkV1 { verification_ccu: 1_000, economic_ccu_per_claim: 500, ops_supported: true, ..Default::default() };
    let registry = PalwModelRegistryFoldV1 {
        globals,
        span_daa: span,
        genesis_works: [(floor.0, work), (model_id, work)].into_iter().collect(),
        grace_until_daa: 0,
        admission_audit_period_daa: p.palw_admission_audit_period_daa,
        readiness_v2_active: p.palw_readiness_v2_at(0),
    };
    let extras = PalwTransitionExtrasV1 {
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        panel_economy_active: true,
        objective_offence_daa: Some(0),
        settled_anchor_depth: Some(PALW_T12_SETTLED_ANCHOR_DEPTH),
        model_registry: Some(registry.clone()),
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: span, ..Default::default() }),
        ..Default::default()
    };
    let fold = |s: &PalwChainStateV2, daa: u64, objs: &[PalwConsensusObjectV2]| {
        apply_palw_transition_v7(
            s,
            sp,
            Some(&b.admission),
            &ctx(daa),
            objs,
            PalwBlockWorkV3::None,
            &[],
            h(0xE0 + daa),
            false,
            false,
            false,
            false,
            &extras,
        )
        .unwrap_or_else(|e| panic!("fold at {daa}: {e:?}"))
        .0
    };
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id: floor.0,
        artifact_root: h(0xA27),
        slash_value_per_pwu: floor.3,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: floor.1 },
        initial_target: floor.2,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    for n in 1..=9u64 {
        objects.push(register_bond(n, big));
    }
    objects.push(PalwConsensusObjectV2::ClassRegistered {
        class_id: model_id,
        artifact_root: h(0xB27),
        slash_value_per_pwu: floor.3,
        pwu_rule: model_rule.clone(),
        initial_target: model_target,
        share_permille: 0,
        activation_daa: 0,
        admission: None,
    });
    let s1 = fold(&PalwChainStateV2::genesis(), 1, &objects);
    let s2 = fold(&s1, 1_000, &[]);
    let mut c = PalwStateCarriageV2::from_state(&s2);
    let row = c.model_lifecycles.get_mut(&model_id).expect("the model row");
    row.state = PalwModelLifecycleV1::Active;
    row.profile.max_inflight_claims = 64;
    let window = row.profile.verification_window_spans as u64;
    let s = c.into_state_v3(sp, None, false, None).expect("rebuilds");
    let canonical = model_rule.canonical_leaves_v1();
    let id = h(0xF0_0400);
    let mut st = fold(&s, 1_001, &[fp_commitment(model_id, canonical, 1, 0x400)]);
    let panel = [2u64, 3, 4, 5, 6];
    st = fold(&st, 1_002, &[PalwConsensusObjectV2::PanelBound { claim: id, anchor: h(0xAC00), seats: seats(&panel) }]);
    st = fold(&st, 1_003, &[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: receipts(id, &panel) }]);
    let unit = palw_panel_demand_term_v1(1, 500 * seat_count, window);
    let jobs = |s: &PalwChainStateV2| demand(s, sp, seat_count as u32, model_id) as f64 / unit as f64;
    let licensed = jobs(&st);
    st = fold(&st, 1_004, &[court(&st, id, 7)]);
    let one_court = jobs(&st);
    st = fold(&st, 1_005, &[court(&st, id, 8)]);
    let two_courts = jobs(&st);
    println!("whole-job FP claim: licensed {licensed:.3}, one court {one_court:.3}, two courts {two_courts:.3} jobs");
    assert!(licensed.abs() < 1e-6);
    assert!((one_court - 1.0).abs() < 1e-6);
    assert!((two_courts - 1.0).abs() < 1e-6, "two courts on one claim charge it once");
}
