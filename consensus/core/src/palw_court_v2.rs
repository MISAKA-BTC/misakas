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

pub const PALW_COURT_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_COURT_V2_DOMAIN_PARTY_ID,
    // The OPEN context was missing from its own uniqueness check (audit M2-23) — the one court
    // move a stranger makes was the one whose domain nothing compared against the others.
    PALW_COURT_V2_MLDSA87_OPEN_CONTEXT,
    PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT,
    PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT,
];

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
    #[error("the ladder has not reached a terminal step — a close before the bisection ends decides a dispute nobody narrowed")]
    LadderNotTerminal,
    #[error(
        "the refutation opens leaf {opened}, but the ladder narrowed to {narrowed} — a close must answer the step the session is about"
    )]
    CloseIsNotTheNarrowedStep { opened: u64, narrowed: u64 },
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
        "the close carries {got} bytes of evidence; the ruleset admits at most {ceiling} \
         (ADR-0049 Decision C — the cost bound is what makes 'a full node can close this court' true)"
    )]
    CloseTooLarge { got: u64, ceiling: u64 },
    /// The close adjudicates a geometry the class never registered. A class id IS its
    /// `shape_profile_id`, so this is the profile arriving from somewhere other than the chain.
    #[error("the close declares shape profile {declared} but claim's class is {class_id}")]
    CloseProfileIsNotTheClass { class_id: Hash64, declared: Hash64 },
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
    // **A data-availability accusation does not close the arithmetic court** (ADR-0062 SA-7).
    //
    // The two courts answer different questions and a producer can be innocent of one and guilty
    // of the other. Reading `claim.phase` directly made an open `DefaultDisputed` un-challengeable,
    // so one accusation — from any bond, the producer's own second bond included — bought a
    // fraudulent producer the rest of its challenge window for `min_collateral_sompi`, against a
    // `CourtFraud` conviction that would have taken `claim.reserved` and voided the escrow.
    //
    // `palw_challenge_surface_phase_v2` is the identity on every other phase, so nothing changes on
    // a network where nothing can be disputed; `palw_da_paused_daa_v2` adds back the part of the
    // window the open session has consumed, so looking through the phase does not silently spend
    // the producer's clock either. Both are zero-cost while `palw_da_court` is dormant, because
    // `DefaultDisputed` is unconstructible there.
    let PalwClaimPhaseV2::ReceiptLicensed { licensed_daa } = *crate::palw_state_v2::palw_challenge_surface_phase_v2(&claim.phase)
    else {
        return Err(PalwCourtV2Error::NotChallengeable {
            claim: *claim_id,
            why: "only a ReceiptLicensed claim is in its challenge surface",
        });
    };
    let deadline = licensed_daa
        .checked_add(state_params.window_challenge())
        .and_then(|at| at.checked_add(crate::palw_state_v2::palw_da_paused_daa_v2(&claim.phase, ctx.daa_score)))
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
    /// ADR-0049 Decision E's terminal: the committed decode token at `position` is refuted from
    /// the claim's own committed logits. The binding pins the execution (its recomputed root must
    /// equal the claim's `execution_root`, which transitively pins `full_logits_trace_root`);
    /// the pin carries the integer class's logits rows and generated ids, authenticated by
    /// recomputing `base0_logits_trace_root_v1`; the verdict is one argmax under the pinned
    /// selection rule. No artifact opening is needed — the evidence is the commitment itself.
    DecodeToken {
        binding: crate::palw_step_leg::PalwStepBindingV2,
        pin: crate::palw_step_refute::PalwBase0DecodeTokensV1,
        position: u32,
    },
    /// The same terminal for a class whose profile registers the TILED logits commitment
    /// (`tiled_logits_scheme_id_v1`): the pin carries two tile openings and their paths instead
    /// of every row, which is what lets a 248,320-lane vocabulary's close ride one lifecycle
    /// carrier. The VARIANT does not choose the scheme — the class's `logits_scheme_id` does, and
    /// each check function refuses a pin that does not speak the class's scheme.
    DecodeTokenTiled { binding: crate::palw_step_leg::PalwStepBindingV2, pin: crate::palw_step_refute::PalwTiledDecodePinV1 },
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

/// **Which root of the binding a claim's `trace_root` is.**
///
/// A claim commits the LOGITS trace root: `palw_producer.rs` sets `trace_root` from
/// `Base0ExecutionV1::trace_root`, which `misaka-palw-base0/src/produce.rs` computes as
/// `base0_logits_trace_root_v1`, and which `base0_binding_from_capture_v1` then carries into the
/// binding as `full_logits_trace_root`. `step_merkle_root` is a DIFFERENT root over the step
/// leaves; it is pinned, but transitively — `committed_execution_root` is
/// `execution_commitment_root_v2(context, full_logits_trace_root, activation, checkpoint, step)`,
/// so the execution-root check beside this one already binds it.
///
/// Both close arms used to compare `claim.trace_root` against `step_merkle_root`, which are never
/// equal for a real execution: every close failed `TraceRootMismatch` before reading any evidence,
/// so no producer could be convicted and no honest producer could clear itself. It was not red
/// because the test built its claim by assigning `attempt.trace_root = binding.step_merkle_root` —
/// the reverse of what production does — which is why the test below builds from a real capture
/// instead.
fn binding_logits_root_of(binding: &crate::palw_step_leg::PalwStepBindingV2) -> Hash64 {
    binding.full_logits_trace_root
}

/// The binding every close carries, whichever scheme it uses.
fn binding_of(proof: &PalwCourtVerdictProofV2) -> &crate::palw_step_leg::PalwStepBindingV2 {
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, .. } => &refutation.binding,
        PalwCourtVerdictProofV2::DecodeToken { binding, .. } => binding,
        PalwCourtVerdictProofV2::DecodeTokenTiled { binding, .. } => binding,
    }
}

/// **The geometry a close adjudicates must be the geometry the CLASS registered** (mainnet audit).
///
/// `check_execution_root_binding` pins the binding to the claim's committed execution root, and
/// that is the right pin against an ACCUSER who invents a geometry. It is not a pin against the
/// defendant, because a producer that committed a poisoned `shape_profile` in the first place
/// carries a binding whose `committed_execution_root` matches it honestly — the root is a function
/// of the binding, so a consistent lie is consistent.
///
/// What the lie then reaches is every arm that reads geometry out of the binding before anything
/// bounds it: unbounded GDN dimensions multiplying into an allocation, tile widths dividing,
/// enumerations walking. Those sites are bounded individually now, but bounding each consumer is a
/// race against whoever adds the next one. This is the bound that does not have to be repeated: a
/// class id IS its `shape_profile_id` (`qwen36_class_id_v3` is literally
/// `profile.shape_profile_id()`), so the class the claim names decides the geometry, and a profile
/// nobody registered cannot enter the court at all.
///
/// One `permissionless` bond could otherwise register a class, produce a claim under it, open a
/// court against itself and close it with a profile no node can survive — which is the cheapest
/// possible path to stopping every node at once.
fn check_close_profile_is_the_registered_class(
    claim_class_id: Hash64,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
) -> Result<(), PalwCourtV2Error> {
    let declared = binding.shape_profile.shape_profile_id();
    if declared != claim_class_id {
        return Err(PalwCourtV2Error::CloseProfileIsNotTheClass { class_id: claim_class_id, declared });
    }
    Ok(())
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
    let (session, claim) = resolve_court_session_v2(state, session_id)?;
    // **A close is a move IN this session, not a fresh argument beside it.**
    //
    // The session used to be resolved and thrown away (`let (_session, claim) = …`), so nothing
    // tied the step being adjudicated to the step the bisection had narrowed to. A trace is up to
    // `PALW_STEP_MAX_LEAVES` leaves and one wrong leaf is enough to be guilty, so an executor
    // could answer with a leaf it had computed CORRECTLY, take `NoFaultFound` — which
    // `map_refutation_outcome` turns into `ChallengerDefeated` — and buy its own acquittal for one
    // transaction, deleting the session and re-arming the claim for `Final`.
    //
    // The ladder already knew which step the dispute was about; it was simply never asked. That is
    // also what `palw_v2_the_ladder_narrows_across_blocks` says in words — "at which point the
    // ladder is `Terminal` and only an arithmetic close can finish the job" — so this makes the
    // code agree with the sentence the ladder was built for.
    let narrowed = session.ladder.terminal_index().ok_or(PalwCourtV2Error::LadderNotTerminal)?;
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, .. } => {
            let opened = refutation.output_opening.leaf_index;
            if opened != narrowed {
                return Err(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened, narrowed });
            }
        }
        // **The same rule, in the arm that was exempt from it.**
        //
        // The paragraph above describes exactly this attack and then closed only one of the two
        // doors: a `DecodeToken` close names its own `position`, so the accused could pick a token
        // it had emitted correctly, take `NoFaultFound` — which reads as `ChallengerDefeated` —
        // and buy its own acquittal for one transaction, at a position the bisection never chose.
        // A ladder that narrows to a step and then lets either party be judged somewhere else is
        // not a bisection; it is a formality in front of a free choice.
        //
        // The two spaces meet at the decode CALL: a leaf index maps to a coordinate whose
        // `call_index` is the decode call that produced the token, and `position` indexes the same
        // calls. Binding at call granularity is the right grain — one call emits one token, and
        // the token is the whole subject of this arm.
        //
        // The mapping reads the close's own profile, which is not yet pinned to the claim here —
        // and does not need to be. A close that forges a geometry to make its chosen position land
        // on the narrowed step still meets `check_execution_root_binding` in
        // `adjudicate_close_proof_v2` immediately below, so no verdict is ever produced from an
        // unpinned binding. Checking the procedural rule first is also what keeps the two arms
        // symmetric: both answer "is this a legal move in this session" before anything is read
        // for its merits.
        PalwCourtVerdictProofV2::DecodeToken { binding, position, .. } => {
            let coord = crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, narrowed)
                .ok_or(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened: u64::from(*position), narrowed })?;
            if coord.call_index != *position {
                return Err(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened: u64::from(*position), narrowed });
            }
        }
        // **And the tiled arm, which is the same door.**
        //
        // `DecodeTokenTiled` names its own `position` exactly as the flat arm does. The rule above
        // was written against the arm that existed when it was written; a new commitment scheme
        // that skipped it would be a free choice of position again, and the fix would have closed
        // one of three doors instead of one of two. Two schemes, one procedural rule.
        PalwCourtVerdictProofV2::DecodeTokenTiled { binding, pin } => {
            let coord = crate::palw_step::canonical_step_coordinates(&binding.shape_profile, &binding.job_context, narrowed)
                .ok_or(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened: u64::from(pin.position), narrowed })?;
            if coord.call_index != pin.position {
                return Err(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened: u64::from(pin.position), narrowed });
            }
        }
    }
    adjudicate_close_proof_v2(state, claim, proof, court)
}

/// The arithmetic half of a close: given the CLAIM the dispute is about, what verdict does this
/// proof support?
///
/// Split out from [`adjudicate_court_close_v2`] so the two properties can be tested apart. The
/// outer function answers "is this close a legal move in this session"; this one answers "and what
/// does the evidence say". Mixing them meant every arithmetic test had to drive a full bisection,
/// and the procedural rule had nowhere to be tested on its own.
pub fn adjudicate_close_proof_v2(
    state: &PalwChainStateV2,
    claim: &crate::palw_state_v2::PalwClaimStateV2,
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    check_close_cost_v2(proof, court)?;
    // Before ANY arm reads geometry out of the binding. See the function's own docs: this is the
    // bound that does not have to be repeated at each consumer.
    check_close_profile_is_the_registered_class(claim.class_id, binding_of(proof))?;
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings } => {
            check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(&refutation.binding))?;
            check_execution_root_binding(claim.execution_root, refutation.binding.committed_execution_root)?;
            let class = state.class(&claim.class_id).ok_or(PalwCourtV2Error::MissingClass(claim.class_id))?;
            let operands = PalwProvenOperandsV1::from_openings_v1(operand_openings, class.artifact_root)
                .map_err(|e| PalwCourtV2Error::OperandProofInvalid(e.to_string()))?;
            map_refutation_outcome(check_execution_step_refutation_v1(refutation, &operands))
        }
        PalwCourtVerdictProofV2::DecodeToken { binding, pin, position } => {
            // The same two pins the arithmetic close runs, for the same reason: the dispute is
            // about THIS claim's committed execution, and `verify_binding` inside the check
            // refuses a binding whose parts do not produce `committed_execution_root` — so the
            // pinned `full_logits_trace_root` is the claim's own, not the accuser's.
            check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(binding))?;
            check_execution_root_binding(claim.execution_root, binding.committed_execution_root)?;
            map_refutation_outcome(crate::palw_step_refute::check_base0_decode_token_refutation_v1(binding, pin, *position))
        }
        PalwCourtVerdictProofV2::DecodeTokenTiled { binding, pin } => {
            // The same two pins, the same reason; the scheme gate inside the check refuses a
            // tiled pin against a class that registered the flat commitment.
            check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(binding))?;
            check_execution_root_binding(claim.execution_root, binding.committed_execution_root)?;
            map_refutation_outcome(crate::palw_step_refute::check_tiled_decode_token_refutation_v1(binding, pin))
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
/// A pure function of the object, so it needs no chain state and costs a walk over lengths — no
/// re-encoding, no hashing, nothing that could itself be the expensive thing a cost bound exists
/// to avoid paying.
pub fn check_close_cost_v2(
    proof: &PalwCourtVerdictProofV2,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Result<(), PalwCourtV2Error> {
    let operand_openings: &[PalwArtifactOpeningV1] = match proof {
        PalwCourtVerdictProofV2::Arithmetic { operand_openings, .. } => operand_openings,
        // A decode-token close opens nothing from the artifact; its whole payload is the pin,
        // measured by the same byte ceiling an opening set answers to — the cost of a close is the
        // cost of a close, whichever arm carries it.
        PalwCourtVerdictProofV2::DecodeToken { pin, .. } => {
            let bytes = base0_decode_bytes_v2(pin);
            if bytes > court.max_close_bytes() {
                return Err(PalwCourtV2Error::CloseTooLarge { got: bytes, ceiling: court.max_close_bytes() });
            }
            return Ok(());
        }
        PalwCourtVerdictProofV2::DecodeTokenTiled { pin, .. } => {
            // Two tiles, three paths and the ids — the payload the tiled scheme exists to bound.
            let bytes: u64 = (pin.committed_tile_lanes.len() as u64 * 4)
                .saturating_add(pin.beat_tile_lanes.len() as u64 * 4)
                .saturating_add(
                    (pin.committed_opening.siblings.len() + pin.beat_opening.siblings.len() + pin.row_opening.siblings.len()) as u64
                        * 64,
                )
                .saturating_add(pin.generated_token_ids.len() as u64 * 4);
            if bytes > court.max_close_bytes() {
                return Err(PalwCourtV2Error::CloseTooLarge { got: bytes, ceiling: court.max_close_bytes() });
            }
            return Ok(());
        }
    };
    let count = operand_openings.len() as u64;
    let operand_ceiling = u64::from(court.max_operand_count());
    if count > operand_ceiling {
        return Err(PalwCourtV2Error::TooManyOperands { got: count, ceiling: operand_ceiling });
    }
    // **The whole close, not the artifact half of it.**
    //
    // A path element is 64 bytes a peer chose, so paths are counted with the payload they prove.
    // So is the REFUTATION: its input openings are the KV history of the disputed step, and on the
    // shipped floor they are twenty-three times the weight bytes beside them (750,716 against
    // 32,768 at a 64/64 job). A ceiling that saw only the artifact half admitted classes whose
    // evidence could not be mined and called them adjudicable.
    let bytes = arithmetic_close_bytes_v2(proof).unwrap_or(u64::MAX);
    if bytes > court.max_close_bytes() {
        return Err(PalwCourtV2Error::CloseTooLarge { got: bytes, ceiling: court.max_close_bytes() });
    }
    Ok(())
}

/// One step opening: the siblings a verifier must hash, at 64 bytes each.
fn step_opening_bytes_v2(opening: &crate::palw_step_leg::PalwStepOpeningV1) -> u64 {
    (opening.siblings.len() as u64).saturating_mul(64)
}

/// The integer lane's generated-token pin: every logits row it carries, plus the ids.
fn base0_decode_bytes_v2(pin: &crate::palw_step_refute::PalwBase0DecodeTokensV1) -> u64 {
    pin.logits_rows
        .iter()
        .map(|row| (row.len() as u64).saturating_mul(4))
        .fold((pin.generated_token_ids.len() as u64).saturating_mul(4), |a, b| a.saturating_add(b))
}

/// The generated-token pin, whichever lane's form it takes.
fn decode_pin_bytes_v2(pin: &crate::palw_step_refute::PalwDecodeTokenPinV1) -> u64 {
    use crate::palw_step_refute::PalwDecodeTokenPinV1 as Pin;
    match pin {
        Pin::FloatV2(d) => (d.generated_token_ids.len() as u64).saturating_mul(4),
        Pin::Base0V1(d) => base0_decode_bytes_v2(d),
        // The rows-tree root plus the ids — the tiled pin's whole point is that no row rides in it.
        Pin::TiledV1(d) => (d.generated_token_ids.len() as u64).saturating_mul(4).saturating_add(64),
    }
}

/// **What an arithmetic close weighs**, in the units [`crate::palw_mode_v2::DEFAULT_MAX_CLOSE_BYTES`]
/// is denominated in: opened payload plus every Merkle path element on it, artifact side and step
/// side alike. `None` for a non-arithmetic proof.
///
/// Public because `derive_court_cost_v1` must bound the SAME quantity from a class's graph, and a
/// ceiling whose two sides measure different things is a ceiling that cannot be reasoned about.
pub fn arithmetic_close_bytes_v2(proof: &PalwCourtVerdictProofV2) -> Option<u64> {
    let PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings } = proof else { return None };
    let mut bytes: u64 = operand_openings
        .iter()
        .map(|o| (o.operand.bytes.len() as u64).saturating_add((o.path.len() as u64).saturating_mul(64)))
        .fold(0u64, |a, b| a.saturating_add(b));
    bytes = bytes
        .saturating_add(step_opening_bytes_v2(&refutation.output_opening))
        .saturating_add(refutation.output_preimage.values_le.len() as u64);
    for input in &refutation.inputs {
        // A row costs its lanes plus one sibling set per derived run — the range form's whole
        // point. Preimage headers ride at the borsh level and are bounded by the same count.
        for preimage in &input.preimages {
            bytes = bytes.saturating_add(preimage.values_le.len() as u64).saturating_add(24);
        }
        for run in &input.run_siblings {
            bytes = bytes.saturating_add((run.len() as u64).saturating_mul(64));
        }
    }
    // **The anchored KV history** (ADR-0030 §3). The checkpoint replaces `prefill + call` step
    // openings per ref with one opening and that checkpoint's state chunks — cheaper, and not free:
    // the chunks ARE the history, at one byte per int8 element. Counting the openings it replaced
    // and not the bytes it carries would price the cheap form as if it were empty.
    if let Some(anchor) = refutation.kv_checkpoint.as_ref() {
        bytes = bytes.saturating_add(step_opening_bytes_v2(&anchor.opening));
        for chunk in &anchor.chunks {
            bytes = bytes.saturating_add(chunk.len() as u64);
        }
    }
    bytes = bytes.saturating_add((refutation.prompt_token_ids.len() as u64).saturating_mul(4));
    if let Some(pin) = refutation.decode_tokens.as_ref() {
        bytes = bytes.saturating_add(decode_pin_bytes_v2(pin));
    }
    Some(bytes)
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
    /// **The space a court opens over is the RULESET's, not the claim's** — and an opener that got
    /// that wrong died silently on a live chain.
    ///
    /// `palw_court_v2`'s own rule is that a space the accuser chose is a ladder depth the accuser
    /// chose, so acceptance demands `max_step_leaf_count`. The first challenger written against
    /// this opened at the disputed job's OWN step count — 7,900 leaves — and every `CourtOpened`
    /// was dropped with `declares a 7900-wide StepLeaves space; the ruleset's is 4194304`. Nothing
    /// else surfaced it: the challenger logged a submission, the mempool took the transaction, and
    /// the object was discarded while the block stood. The responder never saw a duty because
    /// there was no session.
    ///
    /// The padding this creates is not a problem, and the second assertion is why: above the real
    /// leaf count both parties commit to the same full prefix, so a divergence in the real leaves
    /// keeps producing disagreement until the interval narrows back into range.
    #[test]
    fn a_court_opens_over_the_rulesets_space_not_the_claims() {
        let c = court();
        assert_eq!(
            c.max_step_leaf_count(),
            crate::palw_step::PALW_STEP_MAX_LEAVES,
            "the ruleset's space is the step-space ceiling, which is what an opener must declare"
        );
        // A real BASE-0 job is far smaller than that ceiling, which is exactly the trap: the
        // number an opener has in hand is not the number it must declare.
        let real_job_leaves = 7_900u64;
        assert!(
            real_job_leaves < c.max_step_leaf_count(),
            "an opener holding the job's own leaf count holds a number acceptance will refuse"
        );
    }

    fn court() -> crate::palw_mode_v2::PalwCourtParamsV2 {
        crate::palw_mode_v2::PalwCourtParamsV2::new(crate::palw_step::PALW_STEP_MAX_LEAVES, 4, 2)
            .expect("the shipped court parameters are valid")
    }

    fn params() -> PalwStateParamsV2 {
        params_for(h64(1))
    }

    /// The same params with a chosen base class id.
    ///
    /// Needed because a class id IS its `shape_profile_id` — `verify_class_admission_v2` refuses
    /// any other pairing with `ClassIdIsNotTheProfileId` — so a fixture that registers a synthetic
    /// `h64(1)` alongside a real binding is describing a state the chain cannot produce. The
    /// close's profile pin is what made that visible.
    fn params_for(base_class_id: Hash64) -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, base_class_id, 4, 1000, 100, 1000, 0).unwrap()
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
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
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
        // The claim's `trace_root` is the LOGITS root, which is exactly what the binding carries
        // as `full_logits_trace_root`. Taking `step_merkle_root` here was the reverse of what a
        // producer does, and it is what let the close binding stay wrong without a red test.
        let trace_root = refutation.binding.full_logits_trace_root;
        let execution_root = refutation.binding.committed_execution_root;
        // A class id IS its `shape_profile_id`; `verify_class_admission_v2` refuses any other
        // pairing, so the fixture registers the binding's own geometry.
        let cid = refutation.binding.shape_profile.shape_profile_id();
        let p = params_for(cid);

        // A class registered at the fraud's own artifact root, and a claim carrying the fraud's
        // own committed roots — the two bindings `adjudicate_court_close_v2` checks before any
        // fault may be read (audit C3).
        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: cid,
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
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.class_id = cid;
        env.attempt.artifact_root = artifact_root;
        env.attempt.trace_root = trace_root;
        env.attempt.execution_root = execution_root;
        env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, cid, &bond_key(1).0);
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
        // The arithmetic half: what the evidence says about this claim. Whether the close is a
        // legal move in the session is `a_close_must_be_the_step_the_ladder_narrowed_to`.
        let claim_rec = in_court.claim(&claim_id).expect("the claim is in state");
        let verdict = adjudicate_close_proof_v2(&in_court, claim_rec, &proof, &court()).expect("a recomputable step adjudicates");
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

    /// **ADR-0049 Decision E through the WHOLE money path: a lying committed decode token voids
    /// the claim and slashes the bond — and an honest one survives the same close.**
    ///
    /// The producer's generated ids ride inside its own committed integer trace root, beside the
    /// very logits rows they were selected from — so the conviction needs no artifact opening at
    /// all: the evidence is the commitment itself, and the verdict is one argmax under the
    /// pinned selection rule.
    #[test]
    fn palw_v2_a_lying_decode_token_convicts_through_the_court_close() {
        let run_close = |lying: bool| {
            let (_hb, _m, _r, honest_pin) = crate::palw_step_refute::tests::base0_honest_decode_commitment();
            let (binding, _m2, _r2, pin) = if lying {
                let mut ids = honest_pin.generated_token_ids.clone();
                ids[1] = ids[1].wrapping_add(1);
                crate::palw_step_refute::tests::base0_binding_with_decode_root(honest_pin.logits_rows.clone(), ids)
            } else {
                crate::palw_step_refute::tests::base0_binding_with_decode_root(
                    honest_pin.logits_rows.clone(),
                    honest_pin.generated_token_ids.clone(),
                )
            };
            // See above: the claim commits the logits root, not the step root.
            let trace_root = binding.full_logits_trace_root;
            let execution_root = binding.committed_execution_root;
            // A class id IS its `shape_profile_id`; `verify_class_admission_v2` refuses any other
            // pairing, so the fixture registers the binding's own geometry.
            let cid = binding.shape_profile.shape_profile_id();
            let p = params_for(cid);
            let objects = vec![
                PalwConsensusObjectV2::ClassRegistered {
                    class_id: cid,
                    artifact_root: h64(0xA1),
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
                    capable_classes: Default::default(),
                    signature: Vec::new(),
                },
                PalwConsensusObjectV2::BondRegistered {
                    bond: bond_key(2),
                    pubkey: vec![8; 4],
                    operator_pubkey: op_key(0x22),
                    collateral: 1_000,
                    payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                    capable_classes: Default::default(),
                    signature: Vec::new(),
                },
            ];
            let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
            let mut env = attempt(40, 1);
            env.attempt.class_id = cid;
            env.attempt.artifact_root = h64(0xA1);
            env.attempt.trace_root = trace_root;
            env.attempt.execution_root = execution_root;
            env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, cid, &bond_key(1).0);
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
            let proof = PalwCourtVerdictProofV2::DecodeToken { binding, pin, position: 1 };
            let claim_rec = in_court.claim(&claim_id).expect("the claim is in state");
            let verdict = adjudicate_close_proof_v2(&in_court, claim_rec, &proof, &court()).expect("a carried pin adjudicates");
            let (closed, _) = apply_palw_transition_v2(
                &in_court,
                &p,
                &ctx(6, 111, 6),
                &[PalwConsensusObjectV2::CourtClosed { session_id: sid, verdict, proof }],
                None,
            )
            .unwrap();
            (verdict, closed, claim_id)
        };

        let (verdict, closed, claim_id) = run_close(true);
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "a committed token the rule refutes is a conviction");
        match closed.claim(&claim_id).unwrap().phase {
            crate::palw_state_v2::PalwClaimPhaseV2::Voided { reason: crate::palw_state_v2::PalwVoidReasonV2::CourtFraud, .. } => {}
            ref other => panic!("the lying claim must void as CourtFraud, got {other:?}"),
        }
        assert!(closed.bond(&bond_key(1)).unwrap().slashed > 0, "and the executor pays for it");

        let (verdict, closed, claim_id) = run_close(false);
        assert_eq!(verdict, PalwCourtVerdictV2::ChallengerDefeated, "an honest committed token defeats the challenge");
        assert!(
            !matches!(closed.claim(&claim_id).unwrap().phase, crate::palw_state_v2::PalwClaimPhaseV2::Voided { .. }),
            "the honest claim survives"
        );
        assert_eq!(closed.bond(&bond_key(1)).unwrap().slashed, 0, "and its bond is untouched");
    }

    /// **The cost gate covers the new arm.** A decode-token close's payload is its pin; a pin
    /// bigger than the court's opening-byte ceiling is refused before any state is read, exactly
    /// as an oversized opening set is.
    #[test]
    fn an_oversized_decode_pin_is_refused_by_the_cost_gate() {
        let c = court();
        let lanes_per_row = (c.max_close_bytes() / 4) as usize + 1;
        let proof = PalwCourtVerdictProofV2::DecodeToken {
            binding: crate::palw_step_refute::tests::base0_honest_decode_commitment().0,
            pin: crate::palw_step_refute::PalwBase0DecodeTokensV1 {
                logits_rows: vec![vec![0i32; lanes_per_row]],
                generated_token_ids: vec![0],
            },
            position: 0,
        };
        assert!(
            matches!(check_close_cost_v2(&proof, &c), Err(PalwCourtV2Error::CloseTooLarge { .. })),
            "a pin past the byte ceiling is refused on its face"
        );
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
        // The claim's `trace_root` is the LOGITS root, which is exactly what the binding carries
        // as `full_logits_trace_root`. Taking `step_merkle_root` here was the reverse of what a
        // producer does, and it is what let the close binding stay wrong without a red test.
        let trace_root = refutation.binding.full_logits_trace_root;
        let execution_root = refutation.binding.committed_execution_root;
        // A class id IS its `shape_profile_id`; `verify_class_admission_v2` refuses any other
        // pairing, so the fixture registers the binding's own geometry.
        let cid = refutation.binding.shape_profile.shape_profile_id();
        let p = params_for(cid);

        let objects = vec![
            PalwConsensusObjectV2::ClassRegistered {
                class_id: cid,
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
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
            PalwConsensusObjectV2::BondRegistered {
                bond: bond_key(2),
                pubkey: vec![8; 4],
                operator_pubkey: op_key(0x22),
                collateral: 1_000,
                payout_payload: kaspa_hashes::Hash64::from_u64_word(0x9A11),
                capable_classes: Default::default(),
                signature: Vec::new(),
            },
        ];
        let (s1, _) = apply_palw_transition_v2(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None).unwrap();
        let mut env = attempt(40, 1);
        env.attempt.class_id = cid;
        env.attempt.artifact_root = artifact_root;
        env.attempt.trace_root = trace_root;
        env.attempt.execution_root = execution_root;
        env.attempt.challenge = challenge_v2(h64(999), h64(5), 1_700, 1, cid, &bond_key(1).0);
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
        let claim_rec = in_court.claim(&claim_id).expect("the claim is in state");
        let verdict = adjudicate_close_proof_v2(&in_court, claim_rec, &proof, &court()).expect("an honest step adjudicates");
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
        // Asked of the proof itself, so the reason under test is the binding rather than the
        // procedure. The full close refuses this even earlier now — the ladder has not narrowed —
        // and that is asserted separately below.
        let claim_rec = in_court.claim(&claim_id).expect("the claim is in state");
        let outcome = adjudicate_close_proof_v2(&in_court, claim_rec, &bogus, &court());
        // **Refused, and now for a sharper reason than when this test was written.**
        //
        // It used to land on `TraceRootMismatch`: the skeleton's roots are not the claim's. The
        // profile pin runs first and answers a stronger question — the skeleton's geometry is not
        // the class's geometry either, and a class id IS its `shape_profile_id`. Both are the same
        // verdict for this proof (it adjudicates nothing), so the property under test is intact;
        // what changed is that a close carrying a geometry nobody registered is now refused before
        // any arm reads that geometry, which is what stops a poisoned profile from reaching an
        // allocation or a shift.
        assert!(
            matches!(outcome, Err(PalwCourtV2Error::CloseProfileIsNotTheClass { .. }) | Err(PalwCourtV2Error::TraceRootMismatch)),
            "a proof about another execution must not produce a verdict at all, got {outcome:?}"
        );
        let procedural = adjudicate_court_close_v2(&in_court, &sid, &bogus, &court());
        assert!(
            matches!(procedural, Err(PalwCourtV2Error::LadderNotTerminal)),
            "and the full close refuses it before the evidence, because no step was narrowed to, got {procedural:?}"
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
        assert_eq!(PALW_COURT_V2_ALL_DOMAINS.len(), 4);
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
            matches!(adjudicate_court_close_v2(&state, &sid, &fat, &tight), Err(PalwCourtV2Error::CloseTooLarge { .. })),
            "2 KiB against a 1 KiB ceiling"
        );

        // And the Merkle path counts toward it: a path is 64 bytes a peer chose, and a node has to
        // walk every one. Sixteen path elements is 1 KiB on their own.
        let long_path = proof(vec![opening(1, 17)]);
        assert!(
            matches!(adjudicate_court_close_v2(&state, &sid, &long_path, &tight), Err(PalwCourtV2Error::CloseTooLarge { .. })),
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
