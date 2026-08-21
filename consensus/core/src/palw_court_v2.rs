//! V2 court acceptance — which `CourtOpened` / `CourtClosed` objects a candidate chain admits
//! (ADR-0042 Decision 8, PR-07), over PR-03's session records and the B-series court machinery
//! (`palw_artifact` proofs, `palw_step_refute` adjudication, `palw_bisect` identity).
//!
//! ## What closes a court here, and what deliberately cannot yet
//!
//! A `CourtClosed` verdict is acceptable ONLY with a proof a full node can check from the
//! object and the candidate state alone:
//!
//! * **`Arithmetic`** — the P0-8 machinery, whole: operand openings prove against the CLASS's
//!   registered artifact root ([`PalwProvenOperandsV1`]), the refutation binds the CLAIM's
//!   committed trace root, and [`check_execution_step_refutation_v1`] recomputes one step on the
//!   CPU. Conviction ⇒ `ExecutorGuilty`; an honest recomputation (`NoFaultFound`) ⇒ the challenge
//!   lost on the merits, `ChallengerDefeated`. Anything that does not adjudicate — an
//!   out-of-catalog kernel, a non-canonical input set, a missing weight row — refuses the CLOSE
//!   rather than minting either verdict: an unadjudicable object convicts nobody (P0-8's rule),
//!   and it also acquits nobody.
//!
//! **Ladder no-show defaults are still not acceptable OBJECTS — and now they never will be.**
//!
//! The old reason was that the ladder was not chain state, so a validator could not tell a real
//! default from a forged one, and a forged executor-default would void any honest claim on
//! demand. The ladder is chain state now (`PalwCourtSessionStateV2::ladder`), and the fix that
//! made possible is better than admitting the object: **nobody submits a default at all.** The
//! rung deadline is a machine fact every node recomputes, silence past it is visible to all of
//! them, and `sweep_court_deadlines` closes the session against whichever party was due to move.
//! An offense produced by ABSENCE cannot be forged by presence — there is no message to forge.
//!
//! What IS acceptable, and signed, are the two rung moves themselves:
//! [`check_court_disclosure_acceptance_v2`] and [`check_court_verdict_acceptance_v2`]. Those need
//! signatures for the mirror-image reasons a default does not: each is a claim ATTRIBUTED to one
//! party, and either party could otherwise write the other's half of the dispute.
//!
//! A rung window only counts when it is strictly tighter than the session backstop — see
//! `PalwStateParamsV2::turn_deadline_daa`. A network that leaves it at the default has no
//! interactive ladder, and closes exactly as it did before: the backstop at `opened +
//! window_court`, challenger-side, because prosecution is the challenger's burden and an
//! unfinished challenge must not freeze an honest claim. The other two paths are unchanged:
//!
//! * data served → any holder finds the divergent step offline and convicts **arithmetically**
//!   (no ladder narrows needed when you hold the trace);
//! * data withheld pre-license → the PANEL's `Unavailable` quorum justifies `ProducerDefaulted`
//!   (PR-06).
//!
//! ## Session identity (Decision 8 item 1)
//!
//! The V2 session id IS [`bisect_session_id_v1`] over `(attempt_id, trace_root,
//! challenger_party, executor_party, space, space_size)` — the dispute is about one committed
//! root, between two named stakes, over one index space. Party ids are domain-separated hashes
//! of the bond outpoints, so both bonds are inside the id (a ladder must name the stake it
//! accuses — P0-9 item 5), and future rung messages bind the same id natively.

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_bisect::{PalwBisectSpaceV1, bisect_session_id_v1};
use crate::palw_state_v2::{
    PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwCourtVerdictV2, PalwStateParamsV2,
};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_v1};
use blake2b_simd::Params;

pub const PALW_COURT_V2_DOMAIN_PARTY_ID: &[u8] = b"misaka-palw/court-v2/party-id/v1";
/// ML-DSA-87 signing context for a responder's rung disclosure. Its own family domain, for the
/// P0-6 reason every other context has one: a signature must not be able to cross meanings.
pub const PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT: &[u8] = b"misaka-palw/court-v2/disclosure/mldsa87/v1";
/// ML-DSA-87 signing context for a challenger's rung verdict. Separate from the disclosure
/// context, not merely from the attempt's: the two rung messages are made by DIFFERENT parties
/// with opposite interests, so one shared court context would let a responder's signature over
/// its own disclosure be replayed as the challenger's verdict.
pub const PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT: &[u8] = b"misaka-palw/court-v2/verdict/mldsa87/v1";

/// What a CHALLENGER signs to open a session (audit M-01).
///
/// `CourtOpened` carried no signature and `validate_court_opened_v2` verified none: it checked the
/// claim's phase, the window, that the challenger bond exists and that the session id derives — all
/// facts about the bond, none about who spoke for it. So anyone could nominate a stranger's bond as
/// challenger, and the transition then disarms the claim's final deadline, freezing an honest
/// producer's path to `Final` under an identity that never agreed to prosecute. Its own domain, so
/// an opening cannot be replayed as a disclosure or a verdict.
pub const PALW_COURT_V2_MLDSA87_OPEN_CONTEXT: &[u8] = b"misaka-palw/court-v2/open/mldsa87/v1";

pub const PALW_COURT_V2_ALL_DOMAINS: &[&[u8]] =
    &[PALW_COURT_V2_DOMAIN_PARTY_ID, PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT];

/// A bond outpoint as a 64-byte dispute-party identity (what the bisect session id space
/// expects). Domain-separated so a party id can never collide with an attempt id, a claim id, or
/// any other `Hash64` this ruleset mints.
pub fn court_party_id_v2(bond: &PalwBondKeyV2) -> Hash64 {
    let mut state = Params::new().hash_length(64).key(PALW_COURT_V2_DOMAIN_PARTY_ID).to_state();
    state.update(&borsh::to_vec(bond).expect("bond keys are borsh-serializable"));
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// The V2 court session id (see the module doc). `trace_root` is the claim's committed root —
/// the thing the dispute is about.
pub fn court_session_id_v2(
    claim_id: &Hash64,
    trace_root: &Hash64,
    executor_bond: &PalwBondKeyV2,
    challenger_bond: &PalwBondKeyV2,
    space: PalwBisectSpaceV1,
    space_size: u64,
) -> Hash64 {
    bisect_session_id_v1(
        claim_id,
        trace_root,
        &court_party_id_v2(challenger_bond),
        &court_party_id_v2(executor_bond),
        space,
        space_size,
    )
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwCourtV2Error {
    #[error("a rung message's signature does not verify under the party it is attributed to")]
    RungSignatureInvalid,
    #[error("claim {0} does not exist at this chain point")]
    MissingClaim(Hash64),
    #[error("claim {claim} is not challengeable: {why}")]
    NotChallengeable { claim: Hash64, why: &'static str },
    #[error("challenger bond {0:?} does not exist at the candidate chain point")]
    ChallengerMissing(PalwBondKeyV2),
    #[error("a bond cannot challenge its own claim — a self-challenge is a Final-delay lever, not a dispute")]
    SelfChallenge,
    #[error("the dispute space size {0} cannot host a bisection (need at least 2 indices)")]
    SpaceTooSmall(u64),
    #[error("the announced session id is not the one this dispute derives")]
    SessionIdMismatch,
    #[error("court session {0} does not exist at this chain point")]
    MissingSession(Hash64),
    #[error("the proof does not bind this claim's committed trace root")]
    TraceRootMismatch,
    #[error("the refutation's binding is not the execution this claim committed to")]
    ExecutionRootMismatch,
    #[error("the operand openings do not prove against the class's registered artifact root: {0}")]
    OperandProofInvalid(String),
    #[error("the refutation does not adjudicate ({0}) — an unadjudicable object convicts nobody and acquits nobody")]
    DoesNotAdjudicate(String),
    #[error("class {0} does not exist at this chain point")]
    MissingClass(Hash64),
    #[error(
        "the close carries {got} operand openings; the ruleset admits at most {ceiling} \
         (ADR-0049 Decision C — a bound the class was admitted under is a bound its evidence must meet)"
    )]
    TooManyOperands { got: u64, ceiling: u64 },
    #[error(
        "the close's openings carry {got} bytes; the ruleset admits at most {ceiling} \
         (ADR-0049 Decision C — the cost bound is what makes 'a full node can close this court' true)"
    )]
    OpeningTooLarge { got: u64, ceiling: u64 },
}

/// May THIS `CourtOpened` object be accepted at THIS chain point?
///
/// The claim must be inside its challenge surface — `ReceiptLicensed`, with the challenge window
/// not yet lapsed (exactly complementary to the Final sweep: the sweep finals strictly PAST the
/// deadline, so the last block AT it is the last block that can open). The challenger must be a
/// registered bond that is not the executor's own, and the announced session id must be exactly
/// the one this dispute derives for the declared space.
#[allow(clippy::too_many_arguments)]
pub fn validate_court_opened_v2(
    state: &PalwChainStateV2,
    state_params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    session_id: &Hash64,
    claim_id: &Hash64,
    challenger_bond: &PalwBondKeyV2,
    space: PalwBisectSpaceV1,
    space_size: u64,
    // The challenger's ML-DSA-87 signature over `session_id`, under
    // `PALW_COURT_V2_MLDSA87_OPEN_CONTEXT`. The id already binds the claim, the trace root, both
    // bonds and the space, so signing it is signing the whole opening.
    signature: &[u8],
    verify_mldsa87: impl Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
) -> Result<(), PalwCourtV2Error> {
    let claim = state.claim(claim_id).ok_or(PalwCourtV2Error::MissingClaim(*claim_id))?;
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = claim.phase else {
        return Err(PalwCourtV2Error::NotChallengeable {
            claim: *claim_id,
            why: "only a ReceiptLicensed claim is in its challenge surface",
        });
    };
    let deadline = licensed_daa
        .checked_add(state_params.window_challenge())
        .ok_or(PalwCourtV2Error::NotChallengeable { claim: *claim_id, why: "challenge deadline overflows the DAA score" })?;
    if ctx.daa_score > deadline {
        return Err(PalwCourtV2Error::NotChallengeable { claim: *claim_id, why: "the challenge window has lapsed" });
    }
    if state.bond(challenger_bond).is_none() {
        return Err(PalwCourtV2Error::ChallengerMissing(*challenger_bond));
    }
    if *challenger_bond == claim.bond {
        return Err(PalwCourtV2Error::SelfChallenge);
    }
    if space_size < 2 {
        return Err(PalwCourtV2Error::SpaceTooSmall(space_size));
    }
    let derived = court_session_id_v2(claim_id, &claim.trace_root, &claim.bond, challenger_bond, space, space_size);
    if derived != *session_id {
        return Err(PalwCourtV2Error::SessionIdMismatch);
    }
    // **Who spoke for this bond** (audit M-01). Everything above is a fact ABOUT the challenger
    // bond and none of it is a fact about the sender. Without this, opening a court under a
    // stranger's identity costs one transaction fee and freezes an honest claim: the transition
    // disarms the claim's final deadline while a session is open.
    let challenger = state.bond(challenger_bond).ok_or(PalwCourtV2Error::ChallengerMissing(*challenger_bond))?;
    if !verify_mldsa87(&challenger.pubkey, session_id.as_byte_slice(), signature, PALW_COURT_V2_MLDSA87_OPEN_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// The proof classes a `CourtClosed` may carry on the V2 lineage today. See the module doc for
/// why ladder defaults are absent — they are not a proof class at all now, they are a sweep.
///
/// Borsh-serializable because it RIDES the object: a `CourtClosed` carries its proof, and the
/// acceptance layer re-derives the verdict from it rather than believing the one declared beside
/// it. Before that, a close carried a bare verdict, so the pipeline had nothing to check and
/// refused every close outright — the court existed and could not be used.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwCourtVerdictProofV2 {
    /// Terminal arithmetic adjudication from carried proofs (Decision 8 / P0-8): every operand
    /// the recomputation touches arrives as an opening against the class's registered artifact
    /// root, and the refutation binds the claim's committed trace root.
    Arithmetic { refutation: PalwExecutionStepRefutationV1, operand_openings: Vec<PalwArtifactOpeningV1> },
}

/// **Who may post a rung, and under whose key (P0-9's forgery half).**
///
/// A disclosure is the RESPONDER's answer and a verdict is the CHALLENGER's, and both must be
/// signed by the party they are attributed to. Unsigned, the ladder is worse than absent:
///
/// * a challenger writing the responder's disclosures binds an honest executor to states it
///   never claimed, and then convicts it arithmetically at the terminal step;
/// * a responder writing the challenger's verdicts steers the interval away from its own
///   divergence and walks out acquitted.
///
/// Both keys come from the CANDIDATE state's bond registry — the executor's from the claim the
/// session disputes, the challenger's from the session record — so neither party can name its
/// own key, and the message signed is the canonical encoding of the rung itself, which already
/// carries the session id and the round.
pub fn check_court_disclosure_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    disclosure: &crate::palw_bisect::PalwBisectDisclosureV1,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let (_session, claim) = resolve_court_session_v2(state, session_id)?;
    if disclosure.session_id != *session_id {
        return Err(PalwCourtV2Error::SessionIdMismatch);
    }
    let bond = state.bond(&claim.bond).ok_or(PalwCourtV2Error::ChallengerMissing(claim.bond))?;
    let message = borsh::to_vec(disclosure).expect("a disclosure is borsh-serializable");
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// The challenger's half of [`check_court_disclosure_acceptance_v2`]. Same shape, other party,
/// other context.
pub fn check_court_verdict_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    verdict: &crate::palw_bisect::PalwBisectVerdictV1,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let (session, _claim) = resolve_court_session_v2(state, session_id)?;
    if verdict.session_id != *session_id {
        return Err(PalwCourtV2Error::SessionIdMismatch);
    }
    let bond = state.bond(&session.challenger_bond).ok_or(PalwCourtV2Error::ChallengerMissing(session.challenger_bond))?;
    let message = borsh::to_vec(verdict).expect("a verdict is borsh-serializable");
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// Resolve a session to the claim it disputes — the lookups every close shares, isolated so a
/// close against a session or claim that does not exist is a tested refusal, not an assumption.
pub fn resolve_court_session_v2<'a>(
    state: &'a PalwChainStateV2,
    session_id: &Hash64,
) -> Result<(&'a crate::palw_state_v2::PalwCourtSessionStateV2, &'a crate::palw_state_v2::PalwClaimStateV2), PalwCourtV2Error> {
    let session = state.court_session(session_id).ok_or(PalwCourtV2Error::MissingSession(*session_id))?;
    let claim = state.claim(&session.claim).ok_or(PalwCourtV2Error::MissingClaim(session.claim))?;
    Ok((session, claim))
}

/// The dispute is about THIS claim's committed root; a refutation against any other tree is
/// about some other execution and never reaches arithmetic.
pub fn check_arithmetic_close_binding(claim_trace_root: Hash64, refutation_root: Hash64) -> Result<(), PalwCourtV2Error> {
    if refutation_root != claim_trace_root { Err(PalwCourtV2Error::TraceRootMismatch) } else { Ok(()) }
}

/// The binding must be the EXECUTION THE CLAIM COMMITTED TO — not merely one that mentions the
/// claim's public trace root (audit C3).
///
/// The trace-root check alone left every other field of `PalwStepBindingV2` in the accuser's
/// hands, and `check_step_refutation_v1` reads a whole family of faults out of the binding alone:
/// a `shape_profile` that fails `validate_shape`, a `step_leaf_count` that is not the canonical
/// function of (profile, context), a `checkpoint_count` that disagrees with the interval. Those
/// convict the EXECUTOR for a shape it never claimed. Reproduced end to end: any registered bond
/// could copy the public `trace_root`, attach a deliberately invalid profile with no operand
/// openings at all, and take `Ok(ExecutorGuilty)` — voiding an honest claim as `CourtFraud` and
/// slashing its bond, at the cost of one message.
///
/// `committed_execution_root` closes it in one comparison because `verify_binding` RECOMPUTES it
/// from the job context, both profile hashes, the leaf and checkpoint counts and their roots, and
/// refuses a binding whose parts do not produce it. Pin the root to the claim's own and every
/// part is pinned with it — so a shape fault means the executor really did commit to a
/// non-canonical shape, which is a conviction it has earned.
pub fn check_execution_root_binding(claim_execution_root: Hash64, binding_root: Hash64) -> Result<(), PalwCourtV2Error> {
    if binding_root != claim_execution_root { Err(PalwCourtV2Error::ExecutionRootMismatch) } else { Ok(()) }
}

/// Adjudicate a proposed close against the candidate state, returning the ONLY verdict that
/// proof supports. The caller (the acceptance pipeline) then applies the state machine's
/// `CourtClosed { verdict, proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic { refutation: crate::palw_step_refute::tests::skeleton_refutation(), operand_openings: Vec::new(),} }` — with the verdict this function returned, never one the object
/// merely announced.
pub fn adjudicate_court_close_v2(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    proof: &PalwCourtVerdictProofV2,
    // **ADR-0049 Decision C's bounds, applied to the OBJECT** (audit H-03).
    //
    // The four cost bounds live in `PalwCourtParamsV2` and are inside `palw_ruleset_id_v2`, and
    // `verify_class_admission_v2` checks a CLASS against them: "this class's geometry never needs
    // an opening bigger than X". Nothing checked the evidence that actually arrives. A class
    // admitted at 32 KiB could be challenged with a close carrying a million openings, and every
    // validating node would verify every Merkle path before the adjudication refused it — the
    // bound the ruleset id commits to, unenforced at the only place it is spendable.
    //
    // Checked FIRST, before a single path is walked, because the whole point of a cost bound is
    // that exceeding it costs nothing to detect.
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    // The cost gate runs before ANY state is read, which is the cheapest-first ordering a cost
    // bound has to have: an oversized object must be refusable without a lookup, a decode or a
    // hash. A close for a session that does not exist and also breaks the ceiling therefore reports
    // the ceiling, and that is the right answer — the object was inadmissible on its face.
    check_close_cost_v2(proof, court)?;
    let (_session, claim) = resolve_court_session_v2(state, session_id)?;
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings } => {
            check_arithmetic_close_binding(claim.trace_root, refutation.binding.step_merkle_root)?;
            check_execution_root_binding(claim.execution_root, refutation.binding.committed_execution_root)?;
            let class = state.class(&claim.class_id).ok_or(PalwCourtV2Error::MissingClass(claim.class_id))?;
            let operands = PalwProvenOperandsV1::from_openings_v1(operand_openings, class.artifact_root)
                .map_err(|e| PalwCourtV2Error::OperandProofInvalid(e.to_string()))?;
            map_refutation_outcome(check_execution_step_refutation_v1(refutation, &operands))
        }
    }
}

/// **ADR-0049 Decision C's ceilings, applied to a close's own payload (audit H-03).**
///
/// The bounds live in `PalwCourtParamsV2` and are therefore inside `palw_ruleset_id_v2` — the
/// network's identity. `verify_class_admission_v2` checks a CLASS against them ("this class's
/// geometry never needs an opening bigger than X"), and nothing checked the evidence that
/// actually arrives: a class admitted at 32 KiB could be challenged with a close carrying a
/// million openings, and every validating node would verify every Merkle path before the
/// adjudication refused it on the merits.
///
/// A pure function of the object, so it needs no chain state and costs two length comparisons.
pub fn check_close_cost_v2(
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Result<(), PalwCourtV2Error> {
    let PalwCourtVerdictProofV2::Arithmetic { operand_openings, .. } = proof;
    let count = operand_openings.len() as u64;
    let operand_ceiling = u64::from(court.max_operand_count());
    if count > operand_ceiling {
        return Err(PalwCourtV2Error::TooManyOperands { got: count, ceiling: operand_ceiling });
    }
    // The opened BYTES, which is what a node pays to hold and to hash. Merkle path elements are
    // counted with them: a path element is 64 bytes a peer chose, and an opening whose path is
    // longer than its payload is still an opening someone has to walk.
    let bytes: u64 = operand_openings
        .iter()
        .map(|o| o.operand.bytes.len() as u64 + (o.path.len() as u64) * 64)
        .fold(0u64, |a, b| a.saturating_add(b));
    if bytes > court.max_opening_bytes() {
        return Err(PalwCourtV2Error::OpeningTooLarge { got: bytes, ceiling: court.max_opening_bytes() });
    }
    Ok(())
}

/// The outcome mapping, isolated so every arm is unit-testable without a full refutation
/// fixture: a conviction is `ExecutorGuilty`; an honest recomputation is the challenge losing on
/// the merits (`ChallengerDefeated`); everything else does not adjudicate and REFUSES the close.
pub fn map_refutation_outcome(
    outcome: Result<crate::palw_step_leg::PalwStepRefutationVerdictV1, PalwStepRefuteError>,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    match outcome {
        Ok(_conviction) => Ok(PalwCourtVerdictV2::ExecutorGuilty),
        Err(PalwStepRefuteError::NoFaultFound)
        | Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::NoFaultFound)) => {
            Ok(PalwCourtVerdictV2::ChallengerDefeated)
        }
        Err(other) => Err(PalwCourtV2Error::DoesNotAdjudicate(other.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_attempt_v2::{PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, attempt_id_v2, challenge_v2};
    use crate::palw_state_v2::{PalwConsensusObjectV2, PalwPanelSeatV2, PalwPwuRuleV2, apply_palw_transition_v2};
    use crate::palw_step_leg::PalwStepLegError;
    use crate::tx::{TransactionId, TransactionOutpoint};

    /// Operator identities are DERIVED from a key now, so the fixtures carry a key and let the
    /// state machine mint the id — the same path a real registration takes.
    fn op_key(v: u64) -> Vec<u8> {
        vec![v as u8; 8]
    }

    fn op_id(v: u64) -> Hash64 {
        crate::palw_state_v2::palw_operator_id_v2(&op_key(v))
    }

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// The ruleset's court, at the shipped cost ceilings — what a close's evidence is measured
    /// against (ADR-0049 Decision C, audit H-03).
    fn court() -> crate::palw_mode_v2::PalwCourtParamsV2 {
        crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("the shipped court parameters are valid")
    }

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h64(1), 4, 1000, 100, 1000, 0).unwrap()
    }

    fn bond_key(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue, subsidy: 0 }
    }

    fn attempt(pwu: u64, nonce: u64) -> PalwAttemptEnvelopeV2 {
        let bond = bond_key(1).0;
        PalwAttemptEnvelopeV2 {
            attempt: PalwAttemptUnsignedV2 {
                version: PALW_ATTEMPT_V2_VERSION,
                network_domain: h64(999),
                challenge: challenge_v2(h64(999), h64(5), 1_700, nonce, h64(1), &bond),
                class_id: h64(1),
                executor_bond: bond,
                executor_pubkey: vec![7; 4],
                operator_id: op_id(0x21),
                artifact_root: h64(11),
                trace_root: h64(31),
                output_root: h64(32),
                pwu,
                trace_manifest_root: h64(33),
                trace_chunk_count: 4,
                trace_retention_daa: 999_999,
                execution_root: h64(41),
            },
            signature: vec![0x5A; crate::dns_finality::STAKE_ATTESTATION_SIG_LEN],
        }
    }

    /// Class + executor bond + a challenger bond, with one claim walked to ReceiptLicensed at
    /// daa 103 (challenge window 20 → surface through daa 123).
    fn licensed_state() -> (PalwChainStateV2, Hash64) {
        let p = params();
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root: h64(11),
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(1),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0x21),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
        let env = attempt(40, 1);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(2), operator_id: h64(0x22) }];
        let (s3, _) = apply_palw_transition_v2(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }],
            None,
        )
        .unwrap();
        let (s4, _) = apply_palw_transition_v2(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
            None,
        )
        .unwrap();
        (s4, claim_id)
    }

    #[test]
    fn opening_is_validated_end_to_end() {
        let (state, claim_id) = licensed_state();
        let p = params();
        let claim = state.claim(&claim_id).unwrap();
        let sid = court_session_id_v2(&claim_id, &claim.trace_root, &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);

        // Conforming: inside the window (licensed 103 + 20 = 123), real challenger, exact id.
        assert!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 123, 5),
                &sid,
                &claim_id,
                &bond_key(2),
                PalwBisectSpaceV1::StepLeaves,
                64,
                &[],
                |_, _, _, _| true
            )
            .is_ok(),
            "the last block AT the deadline can still open"
        );

        // **Audit M-01: who SPOKE for the challenger bond.** Everything above is a fact about the
        // bond — it exists, it is not the claim's own, the id derives — and none of it is a fact
        // about the sender. Unsigned, opening a court under a stranger's identity cost one
        // transaction fee, and the transition disarms the claim's final deadline while a session is
        // open: an honest producer's claim frozen by an accuser who never agreed to accuse.
        assert!(
            matches!(
                validate_court_opened_v2(
                    &state,
                    &p,
                    &ctx(5, 123, 5),
                    &sid,
                    &claim_id,
                    &bond_key(2),
                    PalwBisectSpaceV1::StepLeaves,
                    64,
                    &[],
                    // The verifier that actually answers, rather than the fixture's `true`.
                    |_, _, _, _| false,
                ),
                Err(PalwCourtV2Error::RungSignatureInvalid)
            ),
            "an opening nobody signed is an opening nobody authorised"
        );
        // And the signature is over the SESSION ID under the opening's own context, so an opening
        // cannot be replayed as a disclosure or a verdict.
        let seen: std::cell::RefCell<Option<(Vec<u8>, Vec<u8>)>> = std::cell::RefCell::new(None);
        let _ = validate_court_opened_v2(
            &state,
            &p,
            &ctx(5, 123, 5),
            &sid,
            &claim_id,
            &bond_key(2),
            PalwBisectSpaceV1::StepLeaves,
            64,
            &[9; 4],
            |_key, message, _sig, context| {
                *seen.borrow_mut() = Some((message.to_vec(), context.to_vec()));
                true
            },
        );
        let (message, context) = seen.into_inner().expect("the verifier was consulted");
        assert_eq!(message, sid.as_byte_slice(), "the challenger signs the session id");
        assert_eq!(context, PALW_COURT_V2_MLDSA87_OPEN_CONTEXT, "in its own domain");
        // Window lapsed.
        assert!(matches!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 124, 5),
                &sid,
                &claim_id,
                &bond_key(2),
                PalwBisectSpaceV1::StepLeaves,
                64,
                &[],
                |_, _, _, _| true
            ),
            Err(PalwCourtV2Error::NotChallengeable { .. })
        ));
        // Self-challenge.
        let self_sid =
            court_session_id_v2(&claim_id, &claim.trace_root, &bond_key(1), &bond_key(1), PalwBisectSpaceV1::StepLeaves, 64);
        assert!(matches!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 110, 5),
                &self_sid,
                &claim_id,
                &bond_key(1),
                PalwBisectSpaceV1::StepLeaves,
                64,
                &[],
                |_, _, _, _| true,
            ),
            Err(PalwCourtV2Error::SelfChallenge)
        ));
        // Unregistered challenger.
        assert!(matches!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 110, 5),
                &sid,
                &claim_id,
                &bond_key(9),
                PalwBisectSpaceV1::StepLeaves,
                64,
                &[],
                |_, _, _, _| true
            ),
            Err(PalwCourtV2Error::ChallengerMissing(_))
        ));
        // Space too small to bisect.
        assert!(matches!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 110, 5),
                &sid,
                &claim_id,
                &bond_key(2),
                PalwBisectSpaceV1::StepLeaves,
                1,
                &[],
                |_, _, _, _| true
            ),
            Err(PalwCourtV2Error::SpaceTooSmall(1))
        ));
        // Announced id from a different space size: not this dispute.
        assert!(matches!(
            validate_court_opened_v2(
                &state,
                &p,
                &ctx(5, 110, 5),
                &sid,
                &claim_id,
                &bond_key(2),
                PalwBisectSpaceV1::StepLeaves,
                65,
                &[],
                |_, _, _, _| true
            ),
            Err(PalwCourtV2Error::SessionIdMismatch)
        ));
        // A merely-Provisional claim is not in its challenge surface.
        let env2 = attempt(7, 2);
        let claim2 = attempt_id_v2(&env2.attempt);
        let (with_prov, _) = apply_palw_transition_v2(&state, &p, &ctx(5, 104, 5), &[], Some(&env2)).unwrap();
        let sid2 = court_session_id_v2(
            &claim2,
            &with_prov.claim(&claim2).unwrap().trace_root,
            &bond_key(1),
            &bond_key(2),
            PalwBisectSpaceV1::StepLeaves,
            64,
        );
        assert!(matches!(
            validate_court_opened_v2(
                &with_prov,
                &p,
                &ctx(6, 105, 6),
                &sid2,
                &claim2,
                &bond_key(2),
                PalwBisectSpaceV1::StepLeaves,
                64,
                &[],
                |_, _, _, _| true
            ),
            Err(PalwCourtV2Error::NotChallengeable { .. })
        ));
    }

    /// The session id names everything Decision 8 item 1 demands: attempt, root, both stakes,
    /// space — moving any one of them is a different dispute.
    #[test]
    fn the_session_id_binds_the_whole_dispute() {
        let base = court_session_id_v2(&h64(1), &h64(2), &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);
        for (name, other) in [
            ("claim", court_session_id_v2(&h64(9), &h64(2), &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64)),
            ("root", court_session_id_v2(&h64(1), &h64(9), &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64)),
            ("executor", court_session_id_v2(&h64(1), &h64(2), &bond_key(9), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64)),
            ("challenger", court_session_id_v2(&h64(1), &h64(2), &bond_key(1), &bond_key(9), PalwBisectSpaceV1::StepLeaves, 64)),
            ("space", court_session_id_v2(&h64(1), &h64(2), &bond_key(1), &bond_key(2), PalwBisectSpaceV1::TraceEvents, 64)),
            ("size", court_session_id_v2(&h64(1), &h64(2), &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 65)),
        ] {
            assert_ne!(base, other, "{name} must move the session id");
        }
        // Swapping the parties is a different dispute too (who accuses whom is not symmetric).
        let swapped = court_session_id_v2(&h64(1), &h64(2), &bond_key(2), &bond_key(1), PalwBisectSpaceV1::StepLeaves, 64);
        assert_ne!(base, swapped);
    }

    /// Every arm of the outcome mapping, without a deep fixture: conviction convicts, honest
    /// recomputation defeats the challenger, and everything unadjudicable refuses the close.
    #[test]
    fn the_outcome_mapping_convicts_defeats_or_refuses() {
        assert_eq!(
            map_refutation_outcome(Err(PalwStepRefuteError::NoFaultFound)),
            Ok(PalwCourtVerdictV2::ChallengerDefeated),
            "an honest step defeats the challenge on the merits"
        );
        assert_eq!(
            map_refutation_outcome(Err(PalwStepRefuteError::Leg(PalwStepLegError::NoFaultFound))),
            Ok(PalwCourtVerdictV2::ChallengerDefeated),
        );
        assert!(
            matches!(map_refutation_outcome(Err(PalwStepRefuteError::Unadjudicable)), Err(PalwCourtV2Error::DoesNotAdjudicate(_))),
            "unadjudicable convicts nobody and acquits nobody"
        );
        assert!(
            matches!(map_refutation_outcome(Err(PalwStepRefuteError::WeightUnavailable)), Err(PalwCourtV2Error::DoesNotAdjudicate(_))),
            "a missing operand row refuses the close — the proof was not proof-carrying"
        );
        assert!(matches!(
            map_refutation_outcome(Err(PalwStepRefuteError::InputSetNotCanonical("x"))),
            Err(PalwCourtV2Error::DoesNotAdjudicate(_))
        ));
    }

    /// The close path's own bindings, piece by piece (the composed call is a thin chain of
    /// these): a session that does not exist refuses, a session resolves to its claim, and a
    /// refutation naming any other committed root never reaches arithmetic.
    #[test]
    fn a_close_must_bind_the_claims_root_and_name_a_real_session() {
        let (state, claim_id) = licensed_state();
        let p = params();
        let claim_root = state.claim(&claim_id).unwrap().trace_root;
        let sid = court_session_id_v2(&claim_id, &claim_root, &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);

        // Before any court exists: the session does not resolve.
        assert!(matches!(resolve_court_session_v2(&state, &sid), Err(PalwCourtV2Error::MissingSession(_))));

        let (in_court, _) = apply_palw_transition_v2(
            &state,
            &p,
            &ctx(5, 110, 5),
            &[PalwConsensusObjectV2::CourtOpened {
                session_id: sid,
                claim: claim_id,
                challenger_bond: bond_key(2),
                space: PalwBisectSpaceV1::StepLeaves,
                space_size: 64,
                signature: Vec::new(),
            }],
            None,
        )
        .unwrap();
        let (session, claim) = resolve_court_session_v2(&in_court, &sid).expect("the opened session resolves");
        assert_eq!(session.claim, claim_id);
        assert_eq!(claim.trace_root, claim_root, "the state carries the committed root the court binds");

        // The binding check: the claim's own root passes, any other root is another execution.
        assert!(check_arithmetic_close_binding(claim_root, claim_root).is_ok());
        assert!(matches!(check_arithmetic_close_binding(claim_root, h64(0xBAD)), Err(PalwCourtV2Error::TraceRootMismatch)));

        // **Audit C3.** The trace root is PUBLIC — it rides in every attempt envelope and sits in
        // every node's claim record — so copying it proves nothing about who wrote the binding.
        // The execution root is the executor's own commitment, and pinning it is what stops an
        // accuser from authoring a whole `PalwStepBindingV2` (non-canonical shape profile, no
        // operand openings at all) and harvesting a shape-family conviction against a producer
        // that never claimed that shape.
        assert_eq!(claim.execution_root, h64(41), "the state carries the execution root the court binds");
        assert!(check_execution_root_binding(claim.execution_root, claim.execution_root).is_ok());
        assert!(
            matches!(
                check_execution_root_binding(claim.execution_root, claim.trace_root),
                Err(PalwCourtV2Error::ExecutionRootMismatch)
            ),
            "the public trace root must not stand in for the executor's execution commitment"
        );
        assert!(matches!(
            check_execution_root_binding(claim.execution_root, h64(0xBAD)),
            Err(PalwCourtV2Error::ExecutionRootMismatch)
        ));
    }

    /// The C3 attack, at the one function that decides it: an accuser-written binding is refused
    /// BEFORE any fault can be read out of it.
    ///
    /// The attacker's material is exactly what the audit reproduced — the claim's public trace
    /// root copied into `step_merkle_root`, an execution root of its own choosing (any value it
    /// likes, since it is authoring the binding), and `operand_openings: vec![]`. The close must
    /// die at the execution-root binding, so `check_step_refutation_v1`'s shape family is never
    /// consulted and no verdict — guilty OR acquitting — is minted.
    #[test]
    fn an_accuser_written_binding_cannot_convict_an_honest_executor() {
        let (state, claim_id) = licensed_state();
        let claim = state.claim(&claim_id).unwrap();
        // The accuser knows both of these: they are in the envelope every node relays.
        let public_trace_root = claim.trace_root;
        assert!(check_arithmetic_close_binding(claim.trace_root, public_trace_root).is_ok(), "copying the public root still passes");
        // What it cannot produce is the executor's execution commitment.
        let forged_execution_root = h64(0xF0_09ED);
        assert_ne!(forged_execution_root, claim.execution_root);
        assert!(
            matches!(
                check_execution_root_binding(claim.execution_root, forged_execution_root),
                Err(PalwCourtV2Error::ExecutionRootMismatch)
            ),
            "an accuser-authored binding must be refused before any fault is read from it"
        );
    }

    /// **P0-8's owed end-to-end: a MatMul fraud convicts through the WHOLE path, with no model.**
    ///
    /// The register asks for exactly this and said no test anywhere did it: "give a full node with
    /// no model a proof-carrying refutation of a wrong MatMul step; assert a conviction, not
    /// `Unadjudicable`." The arithmetic layer's half lives in `palw_step_refute`
    /// (`palw_v2_matmul_fraud_convicts_without_model`); this is the half that matters for a
    /// network — the claim really is voided as `CourtFraud` and the executor's bond really is
    /// debited, through `adjudicate_court_close_v2` and the state transition.
    ///
    /// Every root here is REAL: the claim's `trace_root` and `execution_root` are the fraudulent
    /// execution's own committed roots, and the class's `artifact_root` is the inventory the
    /// weight opening proves against. Nothing reads a model file at any point — the only weights
    /// that exist are bytes a Merkle path binds to the class's registration.
    #[test]
    fn palw_v2_matmul_fraud_convicts_a_claim_and_slashes_its_bond_without_a_model() {
        let (refutation, openings, artifact_root) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let trace_root = refutation.binding.step_merkle_root;
        let execution_root = refutation.binding.committed_execution_root;
        let p = params();

        // A class registered at the fraud's own artifact root, and a claim carrying the fraud's
        // own committed roots — the two bindings `adjudicate_court_close_v2` checks before any
        // fault may be read (audit C3).
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(1),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0x21),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.artifact_root = artifact_root;
        env.attempt.trace_root = trace_root;
        env.attempt.execution_root = execution_root;
        env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, h64(1), &bond_key(1).0);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(2), operator_id: h64(0x22) }];
        let (s3, _) = apply_palw_transition_v2(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }],
            None,
        )
        .unwrap();
        let (s4, _) = apply_palw_transition_v2(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }],
            None,
        )
        .unwrap();
        assert_eq!(s4.bond(&bond_key(1)).unwrap().slashed, 0, "nothing is charged before a verdict");

        let sid = court_session_id_v2(&claim_id, &trace_root, &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);
        let (in_court, _) = apply_palw_transition_v2(
            &s4,
            &p,
            &ctx(5, 110, 5),
            &[PalwConsensusObjectV2::CourtOpened {
                session_id: sid,
                claim: claim_id,
                challenger_bond: bond_key(2),
                space: PalwBisectSpaceV1::StepLeaves,
                space_size: 64,
                signature: Vec::new(),
            }],
            None,
        )
        .unwrap();

        // The conviction, derived from the carried proof.
        let proof = PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings: openings };
        let verdict = adjudicate_court_close_v2(&in_court, &sid, &proof, &court()).expect("a recomputable step adjudicates");
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "a wrong MatMul is a conviction, not an Unadjudicable");

        // …and the chain acts on it: the claim is void as CourtFraud and the bond is debited.
        let (closed, _) = apply_palw_transition_v2(
            &in_court,
            &p,
            &ctx(6, 111, 6),
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }],
            None,
        )
        .unwrap();
        match closed.claim(&claim_id).unwrap().phase {
            crate::palw_state_v2::PalwClaimPhaseV2::Voided { reason: crate::palw_state_v2::PalwVoidReasonV2::CourtFraud, .. } => {}
            ref other => panic!("a proven fraud must void the claim as CourtFraud, got {other:?}"),
        }
        assert!(closed.bond(&bond_key(1)).unwrap().slashed > 0, "and the executor pays for it");
        assert!(closed.court_session(&sid).is_none(), "the session is closed");
    }

    /// **Condition 11: an honest producer is not slashed, end to end.**
    ///
    /// The mirror of the conviction test above, and the harder half: a court that convicted on
    /// every challenge would pass every fraud test ever written. This walks the SAME path with an
    /// honest execution and asserts the four things that must happen — the proof adjudicates
    /// `ChallengerDefeated` rather than refusing, the claim survives instead of being voided, the
    /// producer's bond is untouched, and the claim goes on to reach `Final` and certify its work.
    ///
    /// The last one matters on its own: a claim left alive but frozen would satisfy "not slashed"
    /// while still costing an honest producer everything it had earned.
    ///
    /// Verified non-vacuous by injection: mapping `NoFaultFound` to `ExecutorGuilty` — a court
    /// that convicts on everything — reddens it at the verdict.
    #[test]
    fn palw_v2_an_honest_producer_survives_a_challenge_and_keeps_its_stake() {
        let (refutation, openings, artifact_root) = crate::palw_step_refute::tests::base0_honest_case();
        let trace_root = refutation.binding.step_merkle_root;
        let execution_root = refutation.binding.committed_execution_root;
        let p = params();

        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: h64(1),
                artifact_root,
                slash_value_per_pwu: 5,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(1_000_000),
                initial_target: u128::MAX / 2,
                share_permille: 1000,
                activation_daa: 0,
                admission: None,
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(1),
                pubkey: vec![7; 4],
                operator_pubkey: op_key(0x21),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.artifact_root = artifact_root;
        env.attempt.trace_root = trace_root;
        env.attempt.execution_root = execution_root;
        env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, h64(1), &bond_key(1).0);
        let claim_id = attempt_id_v2(&env.attempt);
        let (s2, _) = apply_palw_transition_v2(&s1, &p, &ctx(2, 101, 2), &[], Some(&env)).unwrap();
        let seats = vec![PalwPanelSeatV2 { bond: bond_key(2), operator_id: h64(0x22) }];
        let (s3, _) = apply_palw_transition_v2(
            &s2,
            &p,
            &ctx(3, 102, 3),
            &[PalwConsensusObjectV2::PanelBound { claim: claim_id, anchor: h64(77), seats }],
            None,
        )
        .unwrap();
        let (s4, _) = apply_palw_transition_v2(
            &s3,
            &p,
            &ctx(4, 103, 4),
            &[PalwConsensusObjectV2::ReceiptLicensed {
                claim: claim_id,
                receipts: vec![crate::palw_panel_v2::PalwSeatReceiptV2 {
                    claim: Hash64::default(),
                    verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                    seat_bond: bond_key(2),
                    signed_daa: 0,
                    signature: Vec::new(),
                }],
            }],
            None,
        )
        .unwrap();

        let sid = court_session_id_v2(&claim_id, &trace_root, &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);
        let (in_court, _) = apply_palw_transition_v2(
            &s4,
            &p,
            &ctx(5, 110, 5),
            &[PalwConsensusObjectV2::CourtOpened {
                session_id: sid,
                claim: claim_id,
                challenger_bond: bond_key(2),
                space: PalwBisectSpaceV1::StepLeaves,
                space_size: 64,
                signature: Vec::new(),
            }],
            None,
        )
        .unwrap();

        // The challenge is answered on the MERITS: the court recomputed the step and found it
        // correct. `ChallengerDefeated`, not a refusal — a refusal would mean the court could not
        // check, which is a different and much weaker kind of safety.
        let proof = PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings: openings };
        let verdict = adjudicate_court_close_v2(&in_court, &sid, &proof, &court()).expect("an honest step adjudicates");
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest producer wins on the merits");

        let (closed, _) = apply_palw_transition_v2(
            &in_court,
            &p,
            &ctx(6, 111, 6),
            &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }],
            None,
        )
        .unwrap();

        assert!(
            matches!(closed.claim(&claim_id).unwrap().phase, crate::palw_state_v2::PalwClaimPhaseV2::ReceiptLicensed { .. }),
            "the claim survives the challenge"
        );
        assert_eq!(closed.bond(&bond_key(1)).unwrap().slashed, 0, "the producer's stake is untouched");
        assert!(closed.court_session(&sid).is_none());

        // …and it goes on to certify its work. A claim left alive but frozen would satisfy "not
        // slashed" while costing an honest producer everything it had earned.
        let (finalized, _) = apply_palw_transition_v2(&closed, &p, &ctx(7, 140, 7), &[], None).unwrap();
        assert!(
            matches!(finalized.claim(&claim_id).unwrap().phase, crate::palw_state_v2::PalwClaimPhaseV2::Final { .. }),
            "the challenge cost the producer nothing, including time"
        );
        assert_eq!(finalized.safe_weight(), 40, "and its work is certified");
        assert_eq!(finalized.bond(&bond_key(1)).unwrap().slashed, 0);
    }

    /// **The close carries its proof, and the verdict is derived from it — not believed.**
    ///
    /// Before this, `CourtClosed` carried a bare verdict, so the pipeline had nothing to check and
    /// refused every close outright: "a court close with no proof carriage cannot be
    /// adjudicated". The court existed and could not be used. What that refusal was protecting
    /// against is exactly what this measures: an ASSERTED verdict.
    ///
    /// Both directions matter. An asserted conviction voids an honest claim and slashes its bond;
    /// an asserted acquittal lets a proven fraud walk. `adjudicate_court_close_v2` mints neither
    /// from a proof that does not adjudicate.
    #[test]
    fn a_close_is_adjudicated_from_its_carried_proof_not_from_its_claim() {
        let (state, claim_id) = licensed_state();
        let p = params();
        let claim_root = state.claim(&claim_id).unwrap().trace_root;
        let sid = court_session_id_v2(&claim_id, &claim_root, &bond_key(1), &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64);
        let (in_court, _) = apply_palw_transition_v2(
            &state,
            &p,
            &ctx(5, 110, 5),
            &[PalwConsensusObjectV2::CourtOpened {
                session_id: sid,
                claim: claim_id,
                challenger_bond: bond_key(2),
                space: PalwBisectSpaceV1::StepLeaves,
                space_size: 64,
                signature: Vec::new(),
            }],
            None,
        )
        .unwrap();

        // A skeleton refutation binds some other execution entirely, so it adjudicates NOTHING —
        // the close is refused rather than resolved either way.
        let bogus = PalwCourtVerdictProofV2::Arithmetic {
            refutation: crate::palw_step_refute::tests::skeleton_refutation(),
            operand_openings: Vec::new(),
        };
        let outcome = adjudicate_court_close_v2(&in_court, &sid, &bogus, &court());
        assert!(
            matches!(outcome, Err(PalwCourtV2Error::TraceRootMismatch)),
            "a proof about another execution must not produce a verdict at all, got {outcome:?}"
        );

        // Which is what a declared verdict runs into: `palw_v2_validate_objects` compares the
        // object's `verdict` against this function's answer, and there is no answer here to
        // agree with. (That comparison lives in the pipeline because that is where the object
        // and the candidate state meet; what this file owns is the refusal it rests on.)

        // A close naming a session that does not exist never reaches arithmetic either.
        assert!(matches!(
            adjudicate_court_close_v2(&in_court, &h64(0xDEAD), &bogus, &court()),
            Err(PalwCourtV2Error::MissingSession(_))
        ));
    }

    #[test]
    fn the_court_domains_are_distinct_from_nothing_by_accident() {
        // The party id, and the two rung signing contexts. The count is pinned so ADDING a domain
        // is a decision someone makes here rather than a line that slips in; the cross-family
        // collision test reads the list itself.
        assert_eq!(PALW_COURT_V2_ALL_DOMAINS.len(), 3);
        let unique: std::collections::BTreeSet<_> = PALW_COURT_V2_ALL_DOMAINS.iter().collect();
        assert_eq!(unique.len(), PALW_COURT_V2_ALL_DOMAINS.len(), "a repeated domain is a collision inside one family");
        // The two rung contexts in particular: the parties have opposite interests, so one shared
        // context would let a responder's own signature be replayed as the challenger's verdict.
        assert_ne!(PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT, PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT);
    }

    /// **Audit H-03: the ruleset's cost bound is checked where it is spent, not only where the
    /// class was admitted.**
    ///
    /// ADR-0049 Decision C put four cost ceilings inside `PalwCourtParamsV2`, and therefore inside
    /// `palw_ruleset_id_v2` — the network's own identity. `verify_class_admission_v2` checks a
    /// CLASS against them ("this class's geometry never needs an opening bigger than X"). Nothing
    /// checked the evidence that actually arrives, so a class admitted at 32 KiB could be
    /// challenged with a close carrying a million openings, and every validating node would verify
    /// every Merkle path before the adjudication refused it on the merits.
    ///
    /// The refusal now costs a length comparison, which is the only cost a cost bound may have.
    #[test]
    fn a_close_that_exceeds_the_rulesets_cost_ceilings_is_refused_before_any_path_is_walked() {
        use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1};

        let (state, sid) = (PalwChainStateV2::genesis(), h64(0xC0FFEE));
        let opening = |bytes: usize, path: usize| PalwArtifactOpeningV1 {
            operand: PalwArtifactOperandV1 {
                tensor_name: "blk.0.attn_q.weight".to_string(),
                layer: Some(0),
                row_start: 0,
                bytes: vec![7u8; bytes],
            },
            leaf_index: 0,
            leaf_count: 1,
            path: vec![h64(0xAB); path],
        };
        let proof = |openings: Vec<PalwArtifactOpeningV1>| PalwCourtVerdictProofV2::Arithmetic {
            refutation: crate::palw_step_refute::tests::skeleton_refutation(),
            operand_openings: openings,
        };

        // A tiny court, so the fixture states the rule rather than the shipped numbers.
        let tight =
            crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2, 1_024, 1_000, 2)
                .unwrap();

        // Too many openings — refused by count, whatever they contain.
        let many = proof(vec![opening(1, 0), opening(1, 0), opening(1, 0)]);
        assert!(
            matches!(
                adjudicate_court_close_v2(&state, &sid, &many, &tight),
                Err(PalwCourtV2Error::TooManyOperands { got: 3, ceiling: 2 })
            ),
            "three openings against a ceiling of two"
        );

        // Within the count and over the bytes — refused by size.
        let fat = proof(vec![opening(2_048, 0)]);
        assert!(
            matches!(adjudicate_court_close_v2(&state, &sid, &fat, &tight), Err(PalwCourtV2Error::OpeningTooLarge { .. })),
            "2 KiB against a 1 KiB ceiling"
        );

        // And the Merkle path counts toward it: a path is 64 bytes a peer chose, and a node has to
        // walk every one. Sixteen path elements is 1 KiB on their own.
        let long_path = proof(vec![opening(1, 17)]);
        assert!(
            matches!(adjudicate_court_close_v2(&state, &sid, &long_path, &tight), Err(PalwCourtV2Error::OpeningTooLarge { .. })),
            "a long path is an opening someone still has to walk"
        );

        // Inside both ceilings, the bound says nothing and the close proceeds to the questions
        // that need chain state — here `MissingSession`, which is the point: the cost gate is out
        // of the way, and it never became the reason for anything else.
        let small = proof(vec![opening(4, 0)]);
        assert!(
            matches!(adjudicate_court_close_v2(&state, &sid, &small, &tight), Err(PalwCourtV2Error::MissingSession(_))),
            "a proof inside the ceilings must reach the state questions"
        );

        // The gate is cheapest-first: an oversized object is refused without the session lookup
        // that would otherwise report first. `state` here holds no session at all.
        assert!(matches!(adjudicate_court_close_v2(&state, &sid, &many, &tight), Err(PalwCourtV2Error::TooManyOperands { .. })));
    }
}
