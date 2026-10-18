//! **ADR-0191 step B: designated custodian answers and the owner tenure split (DORMANT).**
//!
//! Pure rules. Nothing in the shipped consensus path calls them; they are armed only through the sibling fence
//! `palw_tir_designated_answers_v1`, which is `None` on every preset (see `config::params`). Every function is a pure function of
//! the record and the DAA time it is given, so a restart or a reorg replay derives the same state (no timers, no local clocks).
//!
//! Rules (ADR-0191 §3, ADR-0190 points 2 and 3):
//! * each request names its designated custodians in handover order; custodian `i` holds the window
//!   `[opened + i·obligation, opened + (i+1)·obligation)` and the request defaults at `opened + n·obligation`;
//! * only a designated custodian's answer counts; a non-designated custodian's silence or answer is never a fault;
//! * a designated custodian whose window closes without its answer is recorded in `missed` (no slash in this module), and the
//!   request passes to the next designated custodian; if every designated custodian is silent the request is `DaDefault` and the
//!   reward / credit of the claim is refused;
//! * an answer's body is reusable only for the SAME request key (claim, root, coordinates) and the same body digest;
//! * signers of a Valid that is later shown false stay liable: `liable` is never cleared by handover or default;
//! * tenure and evidence are separate periods: an owner awaiting an answer is not idle, and a handover keeps the pursuit state,
//!   the evidence horizon and the charges.

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

/// A custodian's identity (a 32-byte public key digest, as the rest of consensus names custodians).
pub type PalwTirCustodianIdV1 = [u8; 32];
/// A 32-byte digest (a claim id, a root, or an authenticated body digest). Digests identify data; they do not prove computation.
pub type PalwTirDigestV1 = [u8; 32];

/// The most designated custodians one request names (a bound on the handover chain).
pub const PALW_TIR_DESIGNATED_MAX_CUSTODIANS_V1: usize = 8;

/// The request key: the only thing an authenticated body may be reused for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub struct PalwTirRequestKeyV1 {
    pub claim: PalwTirDigestV1,
    pub root: PalwTirDigestV1,
    pub coordinates: u64,
}

/// One request with its designated custodians and obligation.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDesignatedRequestV1 {
    pub key: PalwTirRequestKeyV1,
    /// Designated custodians in handover order (no duplicates, 1..=`PALW_TIR_DESIGNATED_MAX_CUSTODIANS_V1`).
    pub designated: Vec<PalwTirCustodianIdV1>,
    /// The DAA window each designated custodian holds before the handover (the obligation's deadline spacing).
    pub obligation_daa: u64,
    pub opened_daa: u64,
}

/// The first answer that was accepted: who signed it, the body digest and the DAA it was accepted at.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirAcceptedAnswerV1 {
    pub by: PalwTirCustodianIdV1,
    pub body: PalwTirDigestV1,
    pub at_daa: u64,
}

/// The request's state at a given DAA time (a pure function of the record and the time).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirDesignatedOutcomeV1 {
    /// Still inside a designated window; nothing is decided.
    Pending { current: usize },
    /// A designated custodian answered; the body is the authenticated answer.
    Answered { by: PalwTirCustodianIdV1, body: PalwTirDigestV1 },
    /// Every designated custodian was silent to the deadline: DA default; reward and credit are refused.
    DaDefault,
}

/// A request's record: the request, its first accepted answer, and the liabilities that survive the handover.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDesignatedRecordV1 {
    pub request: PalwTirDesignatedRequestV1,
    pub answer: Option<PalwTirAcceptedAnswerV1>,
    /// Signers of a Valid later shown false. Never cleared (not by a handover, not by the default).
    pub liable: BTreeSet<PalwTirCustodianIdV1>,
}

/// The consensus-side registry of designated records, keyed by request. Written only through [`palw_tir_apply_designated_event_v1`].
#[derive(Clone, Debug, Default, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwTirDesignatedRegistryV1 {
    pub records: BTreeMap<PalwTirRequestKeyV1, PalwTirDesignatedRecordV1>,
}

/// An event on the designated-answer path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwTirDesignatedEventV1 {
    Open(PalwTirDesignatedRequestV1),
    Answer { key: PalwTirRequestKeyV1, custodian: PalwTirCustodianIdV1, body: PalwTirDigestV1, at_daa: u64 },
    FalseValid { key: PalwTirRequestKeyV1, signer: PalwTirCustodianIdV1 },
}

/// Why a designated event is refused by the registry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirDesignatedRegistryErrorV1 {
    Rule(PalwTirDesignatedErrorV1),
    AlreadyOpen,
    UnknownRequest,
}

/// **The fold step (DORMANT).** Before the fence is active the registry is not written at all: the call is a strict no-op, so
/// pre-fence state is byte-identical. Once active, each event is applied by the pure rules above; a refused event changes nothing.
pub fn palw_tir_apply_designated_event_v1(
    registry: &mut PalwTirDesignatedRegistryV1,
    fence_active: bool,
    event: PalwTirDesignatedEventV1,
) -> Result<(), PalwTirDesignatedRegistryErrorV1> {
    if !fence_active {
        return Ok(());
    }
    match event {
        PalwTirDesignatedEventV1::Open(request) => {
            if registry.records.contains_key(&request.key) {
                return Err(PalwTirDesignatedRegistryErrorV1::AlreadyOpen);
            }
            let record = palw_tir_open_designated_v1(request).map_err(PalwTirDesignatedRegistryErrorV1::Rule)?;
            registry.records.insert(record.request.key, record);
            Ok(())
        }
        PalwTirDesignatedEventV1::Answer { key, custodian, body, at_daa } => {
            let record = registry.records.get_mut(&key).ok_or(PalwTirDesignatedRegistryErrorV1::UnknownRequest)?;
            palw_tir_answer_designated_v1(record, custodian, body, at_daa).map(|_| ()).map_err(PalwTirDesignatedRegistryErrorV1::Rule)
        }
        PalwTirDesignatedEventV1::FalseValid { key, signer } => {
            let record = registry.records.get_mut(&key).ok_or(PalwTirDesignatedRegistryErrorV1::UnknownRequest)?;
            palw_tir_mark_false_valid_v1(record, signer);
            Ok(())
        }
    }
}

/// Why a request or an answer is refused (a refusal leaves the record unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirDesignatedErrorV1 {
    NoDesignatedCustodian,
    TooManyDesignatedCustodians,
    DuplicateDesignatedCustodian,
    ZeroObligation,
    AnswerAfterDefault,
    AnswerBeforeOpen,
    AnswerAlreadyAccepted,
}

impl PalwTirDesignatedRequestV1 {
    /// The DAA at which the request defaults if nobody answers (`opened + n·obligation`).
    pub fn default_deadline_daa(&self) -> Result<u64, PalwTirDesignatedErrorV1> {
        let n = self.designated.len() as u64;
        n.checked_mul(self.obligation_daa)
            .and_then(|span| self.opened_daa.checked_add(span))
            .ok_or(PalwTirDesignatedErrorV1::ZeroObligation)
    }

    /// The end of custodian `index`'s window (`opened + (index+1)·obligation`).
    pub fn window_end_daa(&self, index: usize) -> Option<u64> {
        let steps = (index as u64).checked_add(1)?.checked_mul(self.obligation_daa)?;
        self.opened_daa.checked_add(steps)
    }
}

/// Open a request record. Validates the designated set and the obligation.
pub fn palw_tir_open_designated_v1(request: PalwTirDesignatedRequestV1) -> Result<PalwTirDesignatedRecordV1, PalwTirDesignatedErrorV1> {
    if request.designated.is_empty() {
        return Err(PalwTirDesignatedErrorV1::NoDesignatedCustodian);
    }
    if request.designated.len() > PALW_TIR_DESIGNATED_MAX_CUSTODIANS_V1 {
        return Err(PalwTirDesignatedErrorV1::TooManyDesignatedCustodians);
    }
    let distinct: BTreeSet<_> = request.designated.iter().collect();
    if distinct.len() != request.designated.len() {
        return Err(PalwTirDesignatedErrorV1::DuplicateDesignatedCustodian);
    }
    if request.obligation_daa == 0 {
        return Err(PalwTirDesignatedErrorV1::ZeroObligation);
    }
    request.default_deadline_daa()?;
    Ok(PalwTirDesignatedRecordV1 { request, answer: None, liable: BTreeSet::new() })
}

/// Record an answer. Only a DESIGNATED custodian's answer is accepted, and only once, and only between the opening and the default
/// deadline. A refused answer (including a non-designated one) changes nothing and is never a fault of the answerer.
pub fn palw_tir_answer_designated_v1(
    record: &mut PalwTirDesignatedRecordV1,
    custodian: PalwTirCustodianIdV1,
    body: PalwTirDigestV1,
    at_daa: u64,
) -> Result<bool, PalwTirDesignatedErrorV1> {
    if at_daa < record.request.opened_daa {
        return Err(PalwTirDesignatedErrorV1::AnswerBeforeOpen);
    }
    if at_daa > record.request.default_deadline_daa()? {
        return Err(PalwTirDesignatedErrorV1::AnswerAfterDefault);
    }
    if record.answer.is_some() {
        return Err(PalwTirDesignatedErrorV1::AnswerAlreadyAccepted);
    }
    if !record.request.designated.contains(&custodian) {
        return Ok(false);
    }
    record.answer = Some(PalwTirAcceptedAnswerV1 { by: custodian, body, at_daa });
    Ok(true)
}

/// Record that a signer of a Valid was later shown false. The liability stays for any handover, default or later answer.
pub fn palw_tir_mark_false_valid_v1(record: &mut PalwTirDesignatedRecordV1, signer: PalwTirCustodianIdV1) {
    record.liable.insert(signer);
}

/// The request's outcome at `now_daa` (a pure function; the same at every replay).
pub fn palw_tir_designated_outcome_v1(record: &PalwTirDesignatedRecordV1, now_daa: u64) -> Result<PalwTirDesignatedOutcomeV1, PalwTirDesignatedErrorV1> {
    if let Some(answer) = record.answer {
        return Ok(PalwTirDesignatedOutcomeV1::Answered { by: answer.by, body: answer.body });
    }
    if now_daa > record.request.default_deadline_daa()? {
        return Ok(PalwTirDesignatedOutcomeV1::DaDefault);
    }
    let mut current = record.request.designated.len() - 1;
    for index in 0..record.request.designated.len() {
        if record.request.window_end_daa(index).ok_or(PalwTirDesignatedErrorV1::ZeroObligation)? >= now_daa {
            current = index;
            break;
        }
    }
    Ok(PalwTirDesignatedOutcomeV1::Pending { current })
}

/// The designated custodians whose window had closed by `now_daa` without their own answer in it. Recorded only; never a slash here.
/// A non-designated custodian is never in this list.
pub fn palw_tir_missed_designated_v1(record: &PalwTirDesignatedRecordV1, now_daa: u64) -> Result<Vec<PalwTirCustodianIdV1>, PalwTirDesignatedErrorV1> {
    let mut missed = Vec::new();
    for (index, custodian) in record.request.designated.iter().enumerate() {
        let end = record.request.window_end_daa(index).ok_or(PalwTirDesignatedErrorV1::ZeroObligation)?;
        if end >= now_daa {
            break;
        }
        let answered_in_window = record.answer.is_some_and(|a| a.by == *custodian && a.at_daa <= end);
        if !answered_in_window {
            missed.push(*custodian);
        }
    }
    Ok(missed)
}

/// Whether an authenticated body may be reused for `to`: the same key and the same body digest, and nothing else.
pub fn palw_tir_body_reusable_v1(from: &PalwTirRequestKeyV1, from_body: &PalwTirDigestV1, to: &PalwTirRequestKeyV1, to_body: &PalwTirDigestV1) -> bool {
    from == to && from_body == to_body
}

/// The reward / credit of the claim is refused exactly when the request defaulted.
pub fn palw_tir_reward_refused_v1(outcome: &PalwTirDesignatedOutcomeV1) -> bool {
    matches!(outcome, PalwTirDesignatedOutcomeV1::DaDefault)
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Owner tenure split (ADR-0191 §3 / ADR-0190 point 2): slot occupancy is not evidence retention.
// ---------------------------------------------------------------------------------------------------------------------------------

/// An owner's tenure on a claim. `awaiting` is the open request the owner waits for (if any); `evidence_until_daa` is the horizon
/// the evidence and the charges must be kept to; neither is shortened by a tenure ending or a handover.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwTirOwnerTenureV1 {
    pub claim: PalwTirDigestV1,
    pub awaiting: Option<PalwTirRequestKeyV1>,
    pub last_progress_daa: u64,
    pub pursuit_step: u64,
    pub evidence_until_daa: u64,
    pub charges: u64,
}

/// An owner is idle (its slot may be reclaimed) only when it awaits no answer and has made no progress for `grace_daa`.
/// An honest owner waiting for an answer is never idle, however long the wait.
pub fn palw_tir_owner_idle_v1(tenure: &PalwTirOwnerTenureV1, now_daa: u64, grace_daa: u64) -> bool {
    tenure.awaiting.is_none() && now_daa.saturating_sub(tenure.last_progress_daa) >= grace_daa
}

/// Hand a tenure to a new owner: the pursuit state, the evidence horizon and the charges pass on unchanged; the progress clock restarts.
pub fn palw_tir_handover_tenure_v1(previous: &PalwTirOwnerTenureV1, now_daa: u64) -> PalwTirOwnerTenureV1 {
    PalwTirOwnerTenureV1 {
        claim: previous.claim,
        awaiting: previous.awaiting,
        last_progress_daa: now_daa,
        pursuit_step: previous.pursuit_step,
        evidence_until_daa: previous.evidence_until_daa,
        charges: previous.charges,
    }
}

/// The evidence horizon of a tenure: never earlier than the awaited request's default deadline, and never earlier than the
/// horizon already set (a tenure ending cannot shorten it).
pub fn palw_tir_evidence_horizon_v1(tenure: &PalwTirOwnerTenureV1, awaited_default_daa: Option<u64>) -> u64 {
    tenure.evidence_until_daa.max(awaited_default_daa.unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(i: u8) -> PalwTirCustodianIdV1 {
        [i; 32]
    }
    fn key() -> PalwTirRequestKeyV1 {
        PalwTirRequestKeyV1 { claim: [7; 32], root: [9; 32], coordinates: 42 }
    }
    fn request(n: u8) -> PalwTirDesignatedRequestV1 {
        PalwTirDesignatedRequestV1 { key: key(), designated: (1..=n).map(c).collect(), obligation_daa: 10, opened_daa: 100 }
    }

    #[test]
    fn a_designated_answer_is_accepted_once_and_a_non_designated_one_is_ignored() {
        let mut r = palw_tir_open_designated_v1(request(2)).unwrap();
        assert_eq!(palw_tir_answer_designated_v1(&mut r, c(99), [1; 32], 105), Ok(false), "non-designated: ignored, not an error");
        assert!(r.answer.is_none());
        assert_eq!(palw_tir_answer_designated_v1(&mut r, c(2), [1; 32], 105), Ok(true));
        assert_eq!(palw_tir_answer_designated_v1(&mut r, c(1), [1; 32], 106), Err(PalwTirDesignatedErrorV1::AnswerAlreadyAccepted));
    }

    #[test]
    fn non_designated_silence_is_never_a_fault() {
        let r = palw_tir_open_designated_v1(request(1)).unwrap();
        // Only c(1) is designated; c(99) is never named and never appears in the missed list.
        let missed = palw_tir_missed_designated_v1(&r, 1_000).unwrap();
        assert_eq!(missed, vec![c(1)]);
        assert!(!missed.contains(&c(99)));
    }

    #[test]
    fn silent_designated_custodian_hands_over_then_the_answer_of_the_next_counts() {
        let mut r = palw_tir_open_designated_v1(request(2)).unwrap();
        // window of c(1): (100, 110]; at 111 it is missed and c(2) holds the request.
        assert_eq!(palw_tir_designated_outcome_v1(&r, 111).unwrap(), PalwTirDesignatedOutcomeV1::Pending { current: 1 });
        assert_eq!(palw_tir_missed_designated_v1(&r, 111).unwrap(), vec![c(1)]);
        palw_tir_answer_designated_v1(&mut r, c(2), [5; 32], 115).unwrap();
        assert_eq!(palw_tir_designated_outcome_v1(&r, 200).unwrap(), PalwTirDesignatedOutcomeV1::Answered { by: c(2), body: [5; 32] });
        assert_eq!(palw_tir_missed_designated_v1(&r, 200).unwrap(), vec![c(1)], "the handed-over custodian stays recorded");
    }

    #[test]
    fn all_designated_silent_gives_da_default_and_refuses_reward() {
        let r = palw_tir_open_designated_v1(request(3)).unwrap();
        let deadline = r.request.default_deadline_daa().unwrap();
        assert_eq!(deadline, 100 + 3 * 10);
        assert_eq!(palw_tir_designated_outcome_v1(&r, deadline).unwrap(), PalwTirDesignatedOutcomeV1::Pending { current: 2 });
        let out = palw_tir_designated_outcome_v1(&r, deadline + 1).unwrap();
        assert_eq!(out, PalwTirDesignatedOutcomeV1::DaDefault);
        assert!(palw_tir_reward_refused_v1(&out));
        assert_eq!(palw_tir_missed_designated_v1(&r, deadline + 1).unwrap(), vec![c(1), c(2), c(3)]);
    }

    #[test]
    fn an_answer_after_the_default_deadline_is_refused() {
        let mut r = palw_tir_open_designated_v1(request(1)).unwrap();
        assert_eq!(palw_tir_answer_designated_v1(&mut r, c(1), [1; 32], 111), Err(PalwTirDesignatedErrorV1::AnswerAfterDefault));
    }

    #[test]
    fn a_false_valid_signer_stays_liable_through_handover_and_default() {
        let mut r = palw_tir_open_designated_v1(request(2)).unwrap();
        palw_tir_mark_false_valid_v1(&mut r, c(1));
        assert_eq!(palw_tir_designated_outcome_v1(&r, 500).unwrap(), PalwTirDesignatedOutcomeV1::DaDefault);
        assert!(r.liable.contains(&c(1)), "a default does not clear the liability");
    }

    #[test]
    fn a_body_is_reused_only_for_the_same_key_and_digest() {
        let k = key();
        let other = PalwTirRequestKeyV1 { coordinates: 43, ..k };
        assert!(palw_tir_body_reusable_v1(&k, &[1; 32], &k, &[1; 32]));
        assert!(!palw_tir_body_reusable_v1(&k, &[1; 32], &other, &[1; 32]), "a different coordinate is another request");
        assert!(!palw_tir_body_reusable_v1(&k, &[1; 32], &k, &[2; 32]), "a different body is not reused");
    }

    #[test]
    fn the_record_is_a_pure_function_of_its_events_and_time() {
        let mut a = palw_tir_open_designated_v1(request(3)).unwrap();
        let mut b = a.clone();
        palw_tir_answer_designated_v1(&mut a, c(2), [4; 32], 112).unwrap();
        palw_tir_answer_designated_v1(&mut b, c(2), [4; 32], 112).unwrap();
        for now in [100, 111, 112, 125, 131, 500] {
            assert_eq!(palw_tir_designated_outcome_v1(&a, now), palw_tir_designated_outcome_v1(&b, now));
            assert_eq!(palw_tir_missed_designated_v1(&a, now), palw_tir_missed_designated_v1(&b, now));
        }
    }

    #[test]
    fn the_designated_set_is_bounded_and_duplicate_free() {
        assert_eq!(palw_tir_open_designated_v1(request(0)).unwrap_err(), PalwTirDesignatedErrorV1::NoDesignatedCustodian);
        assert_eq!(
            palw_tir_open_designated_v1(request(PALW_TIR_DESIGNATED_MAX_CUSTODIANS_V1 as u8 + 1)).unwrap_err(),
            PalwTirDesignatedErrorV1::TooManyDesignatedCustodians
        );
        let mut dup = request(2);
        dup.designated = vec![c(1), c(1)];
        assert_eq!(palw_tir_open_designated_v1(dup).unwrap_err(), PalwTirDesignatedErrorV1::DuplicateDesignatedCustodian);
    }

    #[test]
    fn an_honest_owner_awaiting_an_answer_is_not_idle_and_handover_keeps_evidence() {
        let t = PalwTirOwnerTenureV1 { claim: [7; 32], awaiting: Some(key()), last_progress_daa: 0, pursuit_step: 3, evidence_until_daa: 900, charges: 5 };
        assert!(!palw_tir_owner_idle_v1(&t, 10_000, 50), "awaiting is never idle");
        let mut waiting_done = t.clone();
        waiting_done.awaiting = None;
        assert!(palw_tir_owner_idle_v1(&waiting_done, 10_000, 50));
        let h = palw_tir_handover_tenure_v1(&t, 700);
        assert_eq!((h.pursuit_step, h.evidence_until_daa, h.charges, h.awaiting), (3, 900, 5, Some(key())));
        assert_eq!(palw_tir_evidence_horizon_v1(&t, Some(400)), 900, "a tenure ending never shortens the horizon");
        assert_eq!(palw_tir_evidence_horizon_v1(&t, Some(1_200)), 1_200);
    }

    #[test]
    fn the_fold_step_is_a_strict_no_op_before_the_fence() {
        let mut reg = PalwTirDesignatedRegistryV1::default();
        let before = reg.clone();
        palw_tir_apply_designated_event_v1(&mut reg, false, PalwTirDesignatedEventV1::Open(request(2))).unwrap();
        palw_tir_apply_designated_event_v1(&mut reg, false, PalwTirDesignatedEventV1::FalseValid { key: key(), signer: c(1) }).unwrap();
        assert_eq!(reg, before, "pre-fence: nothing is written");
        assert_eq!(borsh::to_vec(&reg).unwrap(), borsh::to_vec(&before).unwrap(), "pre-fence bytes are identical");
    }

    #[test]
    fn armed_events_go_through_the_rules_and_a_refused_event_changes_nothing() {
        let mut reg = PalwTirDesignatedRegistryV1::default();
        palw_tir_apply_designated_event_v1(&mut reg, true, PalwTirDesignatedEventV1::Open(request(2))).unwrap();
        assert_eq!(
            palw_tir_apply_designated_event_v1(&mut reg, true, PalwTirDesignatedEventV1::Open(request(2))),
            Err(PalwTirDesignatedRegistryErrorV1::AlreadyOpen)
        );
        let snapshot = reg.clone();
        // A non-designated answer is ignored (Ok, no change); a late answer is refused (no change).
        palw_tir_apply_designated_event_v1(&mut reg, true, PalwTirDesignatedEventV1::Answer { key: key(), custodian: c(9), body: [1; 32], at_daa: 1_005 }).unwrap();
        assert_eq!(reg, snapshot, "non-designated answer: no state change");
        assert_eq!(
            palw_tir_apply_designated_event_v1(&mut reg, true, PalwTirDesignatedEventV1::Answer { key: key(), custodian: c(1), body: [1; 32], at_daa: 9_999 }),
            Err(PalwTirDesignatedRegistryErrorV1::Rule(PalwTirDesignatedErrorV1::AnswerAfterDefault))
        );
        assert_eq!(reg, snapshot, "refused answer: no state change");
    }

    #[test]
    fn the_registry_round_trips_through_borsh_and_replays_identically() {
        let events = |reg: &mut PalwTirDesignatedRegistryV1| {
            palw_tir_apply_designated_event_v1(reg, true, PalwTirDesignatedEventV1::Open(request(2))).unwrap();
            palw_tir_apply_designated_event_v1(reg, true, PalwTirDesignatedEventV1::FalseValid { key: key(), signer: c(2) }).unwrap();
            palw_tir_apply_designated_event_v1(reg, true, PalwTirDesignatedEventV1::Answer { key: key(), custodian: c(2), body: [3; 32], at_daa: 1_015 }).unwrap();
        };
        let mut a = PalwTirDesignatedRegistryV1::default();
        let mut b = PalwTirDesignatedRegistryV1::default();
        events(&mut a);
        events(&mut b);
        assert_eq!(borsh::to_vec(&a).unwrap(), borsh::to_vec(&b).unwrap(), "replay gives identical bytes");
        let back = PalwTirDesignatedRegistryV1::try_from_slice(&borsh::to_vec(&a).unwrap()).unwrap();
        assert_eq!(back, a);
    }
}
