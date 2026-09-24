//! **Through the carriage (a node's load, a reorg's rebuild) a class held to Final owes its licensed
//! claim until Final, and an ordinary class's licence releases it, a court re-charges it, a revert
//! re-charges it** — 2026-09-24 audit #4 review item 1 (and the cache lens the review kept).
//!
//! The rate rule's owed claims are a function of the state (`palw_panel_demand_read_v1`, the index
//! walked from the state), and `into_state_v3` rebuilds what it reads (`unresolved`,
//! `open_courts_by_claim`). At e93be0f2 a held class — the 2M regime, ADR-0152 T-2(b): "2M held to
//! Final" — was released at licence like every other class (5,368,709,120,000 → 0 scaled here).
//! This drives one attempt claim on a model class, through the carriage, across a licence, a court
//! session on the licensed claim (the court index rebuilt from the sessions), the court removed, the
//! licence reverted, and a DA accusation on the licensed claim
//! (`DefaultDisputed { resumed: ReceiptLicensed }`).
//!
//! Held to Final is ADR-0152's C7 by the window rule (`palw_panel_held_to_final_v1`: a verification
//! window of at least 1,000 spans), not ADR-0119's held regime (`class_is_held_v1`, a recorded step
//! ladder). So the walk runs over both inputs: the class's window at C7's threshold or as its work
//! derives it, each with the held ladder recorded and without it. The window alone decides.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_owed_claims_through_the_carriage_rebuild
#![allow(dead_code)]

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::{PALW_T12_SETTLED_ANCHOR_DEPTH, Params};
use kaspa_consensus_core::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_attempt_v2::{
    PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2,
    attempt_trace_manifest_root_v1, challenge_v2,
};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwConsensusObjectV2,
    PalwCourtSessionStateV2, PalwPwuRuleV2, PalwStateCarriageV2, PalwTransitionExtrasV1, apply_palw_transition_v7,
    palw_operator_id_v2, palw_panel_demand_read_v1,
};
use kaspa_consensus_core::palw_work_target_v1::{PALW_RCORE_C7_WINDOW_SPANS_V1, palw_panel_held_to_final_v1};
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

const NET: u64 = 0xD05_0012;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn bond_key(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(0xB0_0000 + n), index: 0 })
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

fn attempt(class_id: Hash64, pwu: u64, n: u64, seed: u64) -> PalwAttemptEnvelopeV2 {
    let bond = bond_key(n).0;
    PalwAttemptEnvelopeV2 {
        attempt: PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: h(NET),
            challenge: challenge_v2(h(NET), h(0x5EED_0000 + seed), 1_700_000_000 + seed, seed, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: pubkey(n),
            operator_id: palw_operator_id_v2(&op_pubkey(n)),
            artifact_root: h(0xA27),
            trace_root: h(0x1700_0000 + seed),
            output_root: h(0x2700_0000 + seed),
            pwu,
            trace_manifest_root: attempt_trace_manifest_root_v1(h(0x1700_0000 + seed), PALW_ATTEMPT_V2_TRACE_CHUNKS),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: 9_999_999,
            execution_root: h(0x3700_0000 + seed),
        },
        signature: vec![0u8; MLDSA87_SIGNATURE_LEN],
    }
}

fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
    PalwBlockContextV2 { block: h(0xC000_0000 + block), daa_score: daa, blue_score: blue, subsidy: 0 }
}

fn armed() -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        audit_2026_09_11_deep_active: true,
        audit_2026_09_23_active: true,
        panel_economy_active: true,
        objective_offence_daa: Some(0),
        settled_anchor_depth: Some(PALW_T12_SETTLED_ANCHOR_DEPTH),
        ..Default::default()
    }
}

/// What one model class owes, read at each step of the walk.
#[derive(Debug, PartialEq, Eq)]
struct Owed {
    provisional: u128,
    licensed: u128,
    courted: u128,
    court_closed: u128,
    reverted: u128,
    disputed: u128,
}

/// The claims `class` owes on `s`, as the gate and op 186 read them.
fn owed_by(s: &PalwChainStateV2, sp: &kaspa_consensus_core::palw_state_v2::PalwStateParamsV2, seat_count: u32, class: Hash64) -> u128 {
    palw_panel_demand_read_v1(s, sp, seat_count).0.get(&class).copied().unwrap_or(0)
}

/// The walk on a model class with ADR-0119's held ladder recorded or not (`ladder`), and its window
/// at C7's 1,000 spans or as its work derives it (`c7_window`).
fn walk(ladder: bool, c7_window: bool) -> Owed {
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
    let span = 10;
    let mut globals = PALW_REGISTRY_GLOBALS_V1;
    globals.seat_count = b.panel.seat_count();
    let seat_count = globals.seat_count as u32;
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
        model_registry: Some(registry.clone()),
        round_lane: Some(PalwExecLaneFoldV1 { schedule_span_daa: span, ..Default::default() }),
        ..armed()
    };
    let big = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let (class_id, leaves, target, slash) = floor;
    let mut objects = vec![PalwConsensusObjectV2::ClassRegistered {
        class_id,
        artifact_root: h(0xA27),
        slash_value_per_pwu: slash,
        pwu_rule: PalwPwuRuleV2::DerivedV1 { pwu_per_inference: leaves },
        initial_target: target,
        share_permille: 1000,
        activation_daa: 0,
        admission: None,
    }];
    objects.push(register_bond(1, big));
    objects.push(register_bond(2, big));
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
    let fold = |s: &PalwChainStateV2, daa: u64, objs: &[PalwConsensusObjectV2], own: PalwBlockWorkV3<'_>| {
        apply_palw_transition_v7(
            s,
            sp,
            Some(&b.admission),
            &ctx(
                daa,
                if daa == 2 {
                    110
                } else if daa == 1 {
                    101
                } else {
                    108 + daa
                },
                daa,
            ),
            objs,
            own,
            &[],
            h(0xE0 + daa),
            false,
            false,
            false,
            false,
            &extras,
        )
    };
    let (s1, _, _) = fold(&PalwChainStateV2::genesis(), 1, &objects, PalwBlockWorkV3::None).expect("genesis");
    let (s2, _, _) = fold(&s1, 2, &[], PalwBlockWorkV3::None).expect("span boundary");
    // The class Active. Held to Final, its window is C7's (as testnet-12's 2M row's is); under
    // ADR-0119's held regime its ladder is recorded (as both of testnet-12's genesis model rows' are).
    // No span boundary is crossed before the reads below, so the edited window stands.
    let mut c = PalwStateCarriageV2::from_state(&s2);
    let row = c.model_lifecycles.get_mut(&model_id).expect("the model row");
    row.state = PalwModelLifecycleV1::Active;
    row.profile.max_inflight_claims = 16;
    if c7_window {
        row.profile.verification_window_spans = PALW_RCORE_C7_WINDOW_SPANS_V1;
    }
    if ladder {
        c.class_step_ladders.insert(model_id, kaspa_consensus_core::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1);
    }
    let s = c.into_state_v3(sp, None, false, None).expect("rebuilds");
    assert_eq!(s.class_is_held_v1(&model_id), ladder, "the premise: the ladder");
    assert_eq!(palw_panel_held_to_final_v1(s.model_lifecycle(&model_id).unwrap()), c7_window, "the premise: the window");
    let canonical = model_rule.canonical_leaves_v1();
    let tgt = s.class_target(&model_id).expect("the class target").target;
    let env = attempt(model_id, kaspa_consensus_core::palw_pwu::palw_pwu_v1(tgt, canonical), 2, 0x77);
    let (s3, _, _) = fold(&s, 3, &[], PalwBlockWorkV3::Attempt(&env)).expect("an attempt on the model class");
    let (claim_id, claim) = s3
        .claims_iter()
        .find(|(_, c)| c.class_id == model_id && matches!(c.source, PalwClaimSourceV2::Attempt) && !c.phase.is_terminal())
        .map(|(k, c)| (*k, c.clone()))
        .expect("the attempt's claim");
    let provisional = owed_by(&s3, sp, seat_count, model_id);

    // Licence, through the carriage (what a node loads / a reorg rebuilds).
    let with_phase = |base: &PalwChainStateV2, phase: PalwClaimPhaseV2| {
        let mut c = PalwStateCarriageV2::from_state(base);
        c.claims.get_mut(&claim_id).unwrap().phase = phase;
        c.into_state_v3(sp, None, false, None)
    };
    let licensed = with_phase(&s3, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 3 }).expect("licensed state rebuilds");
    let licensed_owes = owed_by(&licensed, sp, seat_count, model_id);

    // A court session on the licensed claim, rebuilt into `open_courts_by_claim` by the carriage.
    let ladder = kaspa_consensus_core::palw_bisect::PalwBisectLadderV1::open(
        &claim_id,
        &claim.trace_root,
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&bond_key(1)),
        &kaspa_consensus_core::palw_court_v2::court_party_id_v2(&claim.bond),
        kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        16,
        3,
        50,
    )
    .expect("a ladder");
    let mut c = PalwStateCarriageV2::from_state(&licensed);
    *c.reserved_exposure.entry(bond_key(1)).or_insert(0) += claim.reserved;
    c.court_sessions.insert(
        ladder.session_id(),
        PalwCourtSessionStateV2 {
            claim: claim_id,
            challenger_bond: bond_key(1),
            opened_daa: 3,
            deadline_daa: 500,
            ladder,
            dissection: None,
        },
    );
    let courted = c.into_state_v3(sp, None, false, None).expect("a court session on the licensed claim rebuilds");
    let courted_owes = owed_by(&courted, sp, seat_count, model_id);
    let mut c = PalwStateCarriageV2::from_state(&courted);
    c.court_sessions.clear();
    c.reserved_exposure = PalwStateCarriageV2::from_state(&licensed).reserved_exposure;
    let closed = c.into_state_v3(sp, None, false, None).expect("court removed");
    let court_closed = owed_by(&closed, sp, seat_count, model_id);

    // Un-licensed back to its pre-licence phase (a reorg across the licence).
    let back = with_phase(&licensed, claim.phase.clone()).expect("un-licensed state rebuilds");
    assert_eq!(back, s3, "the un-licensed state is the pre-licence state");
    let reverted = owed_by(&back, sp, seat_count, model_id);

    // A DA accusation on the licensed claim: not a court, but the phase is no longer ReceiptLicensed.
    let mut dc = PalwStateCarriageV2::from_state(&licensed);
    *dc.reserved_exposure.entry(bond_key(1)).or_insert(0) += 1;
    dc.claims.get_mut(&claim_id).unwrap().phase = PalwClaimPhaseV2::DefaultDisputed {
        accused_daa: 4,
        missing_event_index: 0,
        accuser: bond_key(1),
        accuser_exposure: 1,
        resumed: Box::new(PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 3 }),
    };
    let disputed = dc.into_state_v3(sp, None, false, None).expect("the accused licensed claim rebuilds");
    Owed {
        provisional,
        licensed: licensed_owes,
        courted: courted_owes,
        court_closed,
        reverted,
        disputed: owed_by(&disputed, sp, seat_count, model_id),
    }
}

/// Held to Final by the window, with or without the ladder (without it: the review's HELD-2 case).
#[test]
fn a_class_held_to_final_owes_its_licensed_claim_until_final_through_the_carriage() {
    for ladder in [false, true] {
        let owed = walk(ladder, true);
        println!("window {PALW_RCORE_C7_WINDOW_SPANS_V1} spans, held ladder {ladder}: {owed:?}");
        assert_eq!(
            owed,
            Owed { provisional: 1, licensed: 1, courted: 1, court_closed: 1, reverted: 1, disputed: 1 },
            "no licence releases a held class's claim, and a court or an accusation adds nothing to it (held ladder {ladder})"
        );
    }
}

/// A window under C7's, with or without the held ladder: an ordinary class.
#[test]
fn an_ordinary_class_is_released_at_licence_and_recharged_by_a_court_a_revert_or_an_accusation() {
    for ladder in [false, true] {
        let owed = walk(ladder, false);
        println!("the derived window, held ladder {ladder}: {owed:?}");
        assert_eq!(
            owed,
            Owed { provisional: 1, licensed: 0, courted: 1, court_closed: 0, reverted: 1, disputed: 1 },
            "held ladder {ladder}"
        );
    }
}
