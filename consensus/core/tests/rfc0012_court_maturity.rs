//! RFC12 C6: a court admitted before Final extends retention, holds Final and preserves that maturity after retirement.
//! This is the linear, floor-only fold fixture: signatures and native computation are acceptance seams; a class-window
//! registry row is installed by delta. Only extracted evidence is relabelled to a REAL class. No court, deadline, Final or
//! retention extension is planted. Post-Final admission is tested as a refusal, not asserted from a synthetic state.

#![allow(warnings)]
#[path = "dos_l5_common.rs"]
mod common;
#[path = "rfc0012_fold_fixture/mod.rs"]
mod fixture;
use common::*;
use fixture::*;
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::{
    PALW_BISECT_OBJECT_VERSION_V1, PalwBisectDisclosureV1, PalwBisectLadderV1, PalwBisectSpaceV1,
};
use kaspa_consensus_core::palw_court_v2::court_party_id_v2;
use kaspa_consensus_core::palw_native_settlement_v1::{
    PalwSettlementPolicyV1, SettlementStopV1, certify_native_effect_v1, native_delta_evidence_v1, native_facts_of_block_v1,
    native_open_sessions_v1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwDeltaEntryV2, PalwStateDeltaV2, PalwStateV2Error,
    apply_delta_v2, revert_delta_v2,
};
use std::collections::BTreeSet;

const MODEL_WINDOW: u64 = 9_000;

fn with_window(l: &Life, state: &PalwChainStateV2) -> PalwChainStateV2 {
    let delta = PalwStateDeltaV2 {
        point: *state.last_point().unwrap(),
        entries: vec![PalwDeltaEntryV2::ClassCourtWindow { key: l.class, old: None, new: Some(MODEL_WINDOW) }],
    };
    apply_delta_v2(state, &delta, &l.sp).unwrap()
}

fn court(l: &Life, state: &PalwChainStateV2, at: u64) -> (Hash64, PalwConsensusObjectV2) {
    let claim = state.claim(&l.id).unwrap();
    let challenger = l.valid_seats[0];
    let space = PalwBisectSpaceV1::StepLeaves;
    let space_size = state.class_step_ladder_v1(&l.class, bundle(&l.p).court.max_step_leaf_count());
    let ladder = PalwBisectLadderV1::open(
        &l.id,
        &claim.trace_root,
        &court_party_id_v2(&challenger),
        &court_party_id_v2(&l.executor_bond),
        space,
        space_size,
        at,
        at + l.sp.turn_deadline_daa(),
    )
    .unwrap();
    let id = ladder.session_id();
    (
        id,
        PalwConsensusObjectV2::CourtOpened {
            session_id: id,
            claim: l.id,
            challenger_bond: challenger,
            space,
            space_size,
            signature: vec![],
        },
    )
}

#[test]
fn rfc0012_c6_final_claim_refuses_new_legacy_model_court() {
    let l = live_life();
    let state = with_window(&l, &l.at_final);
    let at = l.final_daa + 2_900;
    let (_, object) = court(&l, &state, at);
    let refusal = try_step_for(&l, &state, at, &[object], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap_err();
    assert!(matches!(refusal, PalwStateV2Error::WrongPhase { claim, edge: "CourtOpened" } if claim == l.id));
    assert_eq!(state.claim(&l.id).unwrap().trace_retention_daa, l.retention_daa);
}

#[test]
fn rfc0012_c6_admitted_court_holds_final_and_frozen_maturity_survives_retirement() {
    let l = live_life();
    let parent = with_window(&l, &l.at_licensed);
    // Admit at the last DAA before the original Final sweep, so the court must actually hold that boundary.
    let at = l.final_daa - 1;
    let (session_id, object) = court(&l, &parent, at);
    let (opened, opened_delta) = try_step_for(&l, &parent, at, &[object], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap();
    let extended_until = at + MODEL_WINDOW;
    assert_eq!(opened.claim(&l.id).unwrap().trace_retention_daa, extended_until);
    assert_eq!(opened.court_session(&session_id).unwrap().deadline_daa, extended_until);
    assert_eq!(apply_delta_v2(&parent, &opened_delta, &l.sp).unwrap(), opened);
    assert_eq!(revert_delta_v2(&opened, &opened_delta, &l.sp).unwrap(), parent);
    assert_eq!(rebuild(&l.p, kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2::from_state(&opened)), opened);

    // The responder answers on its inclusive deadline; the challenger then defaults on its own turn.
    let ladder = &opened.court_session(&session_id).unwrap().ladder;
    let answered_at = ladder.last_deadline_daa();
    let disclosure = PalwConsensusObjectV2::CourtDisclosed {
        session_id,
        disclosure: PalwBisectDisclosureV1 {
            version: PALW_BISECT_OBJECT_VERSION_V1,
            session_id,
            round: ladder.round(),
            midpoint: ladder.expected_midpoint().unwrap(),
            mid_state: h(0xC6),
        },
        signature: vec![],
    };
    let (answered, _) = try_step_for(&l, &opened, answered_at, &[disclosure], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap();
    let deadline = answered.court_session(&session_id).unwrap().ladder.last_deadline_daa();
    assert!(deadline >= l.final_daa, "the live court crosses old Final {} with deadline {deadline}", l.final_daa);
    let (held, _) = try_step_for(&l, &answered, deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap();
    assert!(matches!(held.claim(&l.id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(native_open_sessions_v1(&held), vec![(l.id, extended_until)]);
    let (closed, _) = try_step_for(&l, &held, deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap();
    assert!(closed.court_session(&session_id).is_none());
    assert!(matches!(closed.claim(&l.id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(closed.bond(&l.executor_bond).unwrap().slashed, parent.bond(&l.executor_bond).unwrap().slashed);
    assert!(closed.bond(&l.valid_seats[0]).unwrap().slashed > parent.bond(&l.valid_seats[0]).unwrap().slashed);
    let (finalized, final_delta) = try_step_for(&l, &closed, deadline + 2, &[], PalwBlockWorkV3::None, Hash64::default(), 0).unwrap();
    let PalwClaimPhaseV2::Final { final_daa } = finalized.claim(&l.id).unwrap().phase else {
        panic!("the honest responder resumes Final")
    };
    let (retired, _) =
        try_step_for(&l, &finalized, final_daa + l.sp.claim_retirement_daa() + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0)
            .unwrap();
    assert!(retired.claim(&l.id).is_none());
    assert!(retired.last_point().unwrap().daa_score < extended_until);
    let mut evidence = native_delta_evidence_v1(&final_delta);
    assert_eq!(evidence.finalized_attempts.len(), 1);
    assert_eq!(evidence.finalized_attempts[0].1.trace_retention_daa, extended_until);
    evidence.finalized_attempts[0].1.class_id = genesis_classes(&l.p)[1].0;
    let facts = native_facts_of_block_v1(
        &rules(&l, &retired, &BTreeSet::new()),
        &evidence,
        &BTreeSet::new(),
        (l.accepted_daa, l.accepted_daa),
    );
    assert_eq!(facts.len(), 1);
    assert_eq!(facts[0].matured_daa, extended_until, "the actual Final delta preserves the extension after live claim retirement");
    // Certificate arithmetic seam: floor work is relabelled, with D=W=1 and explicit frontier/lifecycle premises.
    let policy = PalwSettlementPolicyV1 {
        settled_anchor_depth: 1,
        unique_mature_work: 1,
        max_operator_permille: 1000,
        max_class_permille: 1000,
    };
    let certify = |at| certify_native_effect_v1(policy, (l.accepted_daa, l.accepted_daa), at, true, true, true, true, &facts);
    assert_eq!(certify(l.retention_daa), Err(SettlementStopV1::InsufficientDepth), "baseline retention is too early");
    assert_eq!(certify(extended_until - 1), Err(SettlementStopV1::InsufficientDepth));
    assert!(certify(extended_until).is_ok(), "only the recorded extended maturity supplies this work");
    eprintln!(
        "[C6] old Final {}, opening {at}, answered {answered_at}, challenger deadline {deadline}, resumed Final {final_daa}, retention/maturity {extended_until}; claim retired before maturity",
        l.final_daa
    );
}
