//! ADR-0191 step C: scenario tests of the designated-answer rules (`palw_tir_designated_answers_v1`), dormant fence.
//!
//! These are scenario tests of the pure rules, not end-to-end consensus runs. The end-to-end run on the fenced path (a node with the
//! fence armed) is NOT done here and is listed as remaining in the report.

use d::*;
use kaspa_consensus_core::palw_tir_designated_answers_v1 as d;

fn c(i: u8) -> PalwTirCustodianIdV1 {
    [i; 32]
}

fn request(designated: &[u8]) -> PalwTirDesignatedRequestV1 {
    PalwTirDesignatedRequestV1 {
        key: PalwTirRequestKeyV1 { claim: [1; 32], root: [2; 32], coordinates: 7 },
        designated: designated.iter().map(|i| c(*i)).collect(),
        obligation_daa: 10,
        opened_daa: 1_000,
    }
}

/// Whole designated set colludes (every designated custodian stays silent) and an external verifier signs a Valid that is later shown
/// false. The request defaults, the reward is refused, and the false-Valid signer stays liable.
#[test]
fn whole_collusion_defaults_and_an_external_verifier_conviction_is_kept_on_the_fenced_path() {
    let mut r = palw_tir_open_designated_v1(request(&[1, 2, 3])).unwrap();
    // The external verifier (not designated) answers with a body; it is ignored by the designated rules.
    assert_eq!(palw_tir_answer_designated_v1(&mut r, c(9), [4; 32], 1_005), Ok(false));
    // Its Valid is later shown false by the public evidence: the signer is recorded as liable.
    palw_tir_mark_false_valid_v1(&mut r, c(2));
    let outcome = palw_tir_designated_outcome_v1(&r, 1_031).unwrap();
    assert_eq!(outcome, PalwTirDesignatedOutcomeV1::DaDefault);
    assert!(palw_tir_reward_refused_v1(&outcome), "all designated silent: reward refused");
    assert!(r.liable.contains(&c(2)), "a false Valid is charged even though the request defaulted");
}

/// A designated custodian silent in its window is handed over; the next designated custodian's answer counts; the silent one is recorded
/// as missed; no slash and no reward refusal follows from a successful handover.
#[test]
fn silent_designated_custodian_hands_over_to_the_next_and_the_answer_counts() {
    let mut r = palw_tir_open_designated_v1(request(&[1, 2])).unwrap();
    assert_eq!(palw_tir_missed_designated_v1(&r, 1_011).unwrap(), vec![c(1)]);
    assert_eq!(palw_tir_designated_outcome_v1(&r, 1_011).unwrap(), PalwTirDesignatedOutcomeV1::Pending { current: 1 });
    assert_eq!(palw_tir_answer_designated_v1(&mut r, c(2), [8; 32], 1_015), Ok(true));
    let outcome = palw_tir_designated_outcome_v1(&r, 1_100).unwrap();
    assert_eq!(outcome, PalwTirDesignatedOutcomeV1::Answered { by: c(2), body: [8; 32] });
    assert!(!palw_tir_reward_refused_v1(&outcome));
    assert!(r.liable.is_empty(), "silence alone creates no liability");
}

/// A designated set that is silent to the finite deadline is a DA default with reward and credit refused; the default is reached at a
/// finite, computable DAA, and every designated custodian is listed as missed.
#[test]
fn all_designated_silent_reaches_a_finite_default_and_refuses_reward_and_credit() {
    let r = palw_tir_open_designated_v1(request(&[1, 2, 3])).unwrap();
    let deadline = r.request.default_deadline_daa().unwrap();
    assert_eq!(deadline, 1_000 + 3 * 10);
    assert_eq!(palw_tir_designated_outcome_v1(&r, deadline).unwrap(), PalwTirDesignatedOutcomeV1::Pending { current: 2 });
    let out = palw_tir_designated_outcome_v1(&r, deadline + 1).unwrap();
    assert_eq!(out, PalwTirDesignatedOutcomeV1::DaDefault);
    assert!(palw_tir_reward_refused_v1(&out));
    assert_eq!(palw_tir_missed_designated_v1(&r, deadline + 1).unwrap(), vec![c(1), c(2), c(3)]);
}

/// Non-designated silence is never a fault: a request whose designated custodian answers in time has no missed entry for anyone
/// outside the designated set, however many non-designated custodians stay silent.
#[test]
fn non_designated_silence_is_not_punished() {
    let mut r = palw_tir_open_designated_v1(request(&[1])).unwrap();
    assert_eq!(palw_tir_answer_designated_v1(&mut r, c(1), [3; 32], 1_004), Ok(true));
    let missed = palw_tir_missed_designated_v1(&r, 2_000).unwrap();
    assert!(missed.is_empty(), "the designated custodian answered; silent outsiders are not in the list");
    assert!(r.liable.is_empty());
}

/// A false-Valid signer is charged (kept liable) regardless of whether it was designated, and regardless of the handover.
#[test]
fn a_false_valid_signer_is_charged_through_handover_and_default() {
    let mut r = palw_tir_open_designated_v1(request(&[1, 2])).unwrap();
    palw_tir_mark_false_valid_v1(&mut r, c(1));
    palw_tir_mark_false_valid_v1(&mut r, c(9));
    assert_eq!(palw_tir_designated_outcome_v1(&r, 1_050).unwrap(), PalwTirDesignatedOutcomeV1::DaDefault);
    assert!(r.liable.contains(&c(1)) && r.liable.contains(&c(9)));
}

/// Restart and reorg replay: the same event sequence, replayed from the opened record, gives the same outcome, missed set and liability
/// at every time; a reorg that removes an answer (replay without it) returns the request to the default-or-pending state, not to a
/// stale answer.
#[test]
fn restart_and_reorg_replay_is_identical_across_the_fence() {
    let events = |r: &mut PalwTirDesignatedRecordV1| {
        palw_tir_mark_false_valid_v1(r, c(2));
        palw_tir_answer_designated_v1(r, c(2), [6; 32], 1_014).unwrap();
    };
    let mut first = palw_tir_open_designated_v1(request(&[1, 2])).unwrap();
    let mut replay = palw_tir_open_designated_v1(request(&[1, 2])).unwrap();
    events(&mut first);
    events(&mut replay);
    assert_eq!(first, replay, "restart replay rebuilds the same record");
    for now in [1_000, 1_010, 1_014, 1_020, 1_030, 5_000] {
        assert_eq!(palw_tir_designated_outcome_v1(&first, now), palw_tir_designated_outcome_v1(&replay, now));
        assert_eq!(palw_tir_missed_designated_v1(&first, now), palw_tir_missed_designated_v1(&replay, now));
    }
    // Reorg: the answer's block is orphaned; the replay without it has no answer and reaches the default.
    let mut reorged = palw_tir_open_designated_v1(request(&[1, 2])).unwrap();
    palw_tir_mark_false_valid_v1(&mut reorged, c(2));
    assert_eq!(palw_tir_designated_outcome_v1(&reorged, 1_030).unwrap(), PalwTirDesignatedOutcomeV1::DaDefault);
    assert!(reorged.liable.contains(&c(2)), "the liability is an event of its own and survives the reorg of the answer");
}

/// The owner tenure split: an honest owner awaiting an answer is never idle; a handover keeps the pursuit, the evidence horizon and the
/// charges; the evidence horizon is never shortened by the tenure ending.
#[test]
fn owner_tenure_split_keeps_an_awaited_owner_alive_and_the_evidence_horizon() {
    let key = PalwTirRequestKeyV1 { claim: [1; 32], root: [2; 32], coordinates: 7 };
    let t = PalwTirOwnerTenureV1 {
        claim: [1; 32],
        awaiting: Some(key),
        last_progress_daa: 1_000,
        pursuit_step: 4,
        evidence_until_daa: 9_000,
        charges: 3,
    };
    assert!(!palw_tir_owner_idle_v1(&t, 50_000, 100), "awaiting is never idle, however long");
    let h = palw_tir_handover_tenure_v1(&t, 2_000);
    assert_eq!((h.pursuit_step, h.evidence_until_daa, h.charges), (4, 9_000, 3));
    assert_eq!(palw_tir_evidence_horizon_v1(&h, Some(3_000)), 9_000);
}
