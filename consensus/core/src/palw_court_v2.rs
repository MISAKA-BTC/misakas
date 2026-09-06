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
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_capped_v1};
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

/// **ML-DSA-87 signing context for the RESPONDER's moves in a fused-attention dissection**
/// (ADR-0082 Decision 2): the root claim and every round of children.
///
/// Its own family domain, for the reason each one above has one — a signature must not be able to
/// cross meanings. Separate from the ladder's disclosure context because the two protocols narrow
/// different spaces: a ladder disclosure and a dissection round are both "the responder's answer",
/// and one shared context would let a move in the leaf ladder be replayed as a move in the history
/// dissection of the same session.
pub const PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT: &[u8] = b"misaka-palw/court-v2/attn-dissect/responder/mldsa87/v1";
/// The challenger's half: the child index it names each round. Separate from the responder's for
/// the reason the verdict context is separate from the disclosure's — the two moves are made by
/// parties with opposite interests.
pub const PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT: &[u8] = b"misaka-palw/court-v2/attn-dissect/challenger/mldsa87/v1";

/// **What a side signs to DECLARE a split close** (ADR-0080 design A, W6).
///
/// A declaration is a court move like any other: it asserts a verdict on behalf of one of the two
/// bonds the session id binds, it pins every byte that will follow it, and it suspends the mover's
/// rung while the chunks ride behind it. Until this constant existed there was nothing for that
/// side's key to bind, so `palw_v2_validate_objects` refused every `CourtCloseDeclared` outright
/// rather than trusting one — the state machine behind it being complete enough to act on a forgery
/// is exactly why the refusal was the right answer to have shipped.
///
/// Its own family domain, for the reason the disclosure and the verdict each have one: the two
/// parties have opposite interests, and one shared court context would let a responder's signature
/// over its own move be replayed as the other side's. Here the replay it forbids is narrower and
/// worse — a declaration is attributed to a SIDE, so one context shared with the rung messages
/// would let a challenger's verdict signature stand as the executor's close.
pub const PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT: &[u8] = b"misaka-palw/court-v2/close-declaration/mldsa87/v1";

pub const PALW_COURT_V2_ALL_DOMAINS: &[&[u8]] = &[
    PALW_COURT_V2_DOMAIN_PARTY_ID,
    // The OPEN context was missing from its own uniqueness check (audit M2-23) — the one court
    // move a stranger makes was the one whose domain nothing compared against the others.
    PALW_COURT_V2_MLDSA87_OPEN_CONTEXT,
    PALW_COURT_V2_MLDSA87_DISCLOSURE_CONTEXT,
    PALW_COURT_V2_MLDSA87_VERDICT_CONTEXT,
    PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT,
    PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT,
    PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT,
];

/// The two message tags that keep the responder's OWN two move kinds apart inside one context.
///
/// One context and two message shapes would be safe only as long as no root claim's encoding is
/// also a legal round's; that is a property of two Borsh layouts, which is exactly the kind of
/// coincidence a signature domain exists not to depend on. Tagged, the question does not arise.
pub const PALW_COURT_V2_ATTN_ROOT_MESSAGE_TAG_V1: &[u8] = b"misaka-palw/court-v2/attn-dissect/root/v1";
pub const PALW_COURT_V2_ATTN_ROUND_MESSAGE_TAG_V1: &[u8] = b"misaka-palw/court-v2/attn-dissect/round/v1";

/// What a responder signs to OPEN a dissection: the tag, the session, and the root claim.
pub fn palw_attn_root_claim_message_v1(session_id: &Hash64, root: &crate::palw_attn_dissect::PalwAttnRootClaimV1) -> Vec<u8> {
    let mut message = PALW_COURT_V2_ATTN_ROOT_MESSAGE_TAG_V1.to_vec();
    message.extend_from_slice(session_id.as_byte_slice());
    message.extend_from_slice(&borsh::to_vec(root).expect("a root claim is borsh-serializable"));
    message
}

/// What a responder signs for one ROUND. The round number is part of the message and comes from
/// the SESSION rather than the object: a round carries no index of its own, so without this a
/// disclosure could be replayed at a later round of the same dissection — signed, in domain, and
/// about a range nobody is disputing any more.
pub fn palw_attn_round_message_v1(
    session_id: &Hash64,
    round: u32,
    disclosure: &crate::palw_attn_dissect::PalwAttnDissectRoundV1,
) -> Vec<u8> {
    let mut message = PALW_COURT_V2_ATTN_ROUND_MESSAGE_TAG_V1.to_vec();
    message.extend_from_slice(session_id.as_byte_slice());
    message.extend_from_slice(&round.to_le_bytes());
    message.extend_from_slice(&borsh::to_vec(disclosure).expect("a dissection round is borsh-serializable"));
    message
}

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
    #[error("a {close} close is not a move on a network whose prompt commitment is {form} (ADR-0081 Decision 3)")]
    CloseFormIsNotTheNetworks { close: &'static str, form: &'static str },
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
    // ---------------------------------------------------------------------------------------
    // ADR-0082 Decision 2 — the dissection's own refusals. Every one names the quantity: a court
    // finding that reads "assertion failed" is a finding nobody can sequence.
    // ---------------------------------------------------------------------------------------
    #[error("court session {0} has no open dissection — a dissection move is not a legal move in it")]
    NoDissection(Hash64),
    #[error("court session {0} already has an open dissection; a root claim opens exactly one")]
    DissectionAlreadyOpen(Hash64),
    #[error("the k-ary court fence is dormant at this block: a fused-attention dissection is not admissible")]
    KaryCourtDormant,
    #[error("the binding does not verify: {0}")]
    BindingInvalid(String),
    #[error("leaf {0} is not a canonical step coordinate of the class this claim names")]
    NotACanonicalLeaf(u64),
    #[error("the ladder narrowed to a {op:?} leaf; a dissection adjudicates a fused attention site")]
    NotAFusedLeaf { op: crate::palw_step::PalwStepOpKindV1 },
    #[error(
        "the disputed output tile spans lanes {first}..+{count} of a {d_head}-wide head — a dissection is about ONE head, and a \
         class whose fused tile straddles two cannot be adjudicated by it"
    )]
    FusedTileStraddlesHeads { first: u64, count: u64, d_head: u64 },
    #[error("the class's fused site declares a geometry the court cannot serve: {0}")]
    FusedGeometryUnservable(&'static str),
    #[error("the class's registered narrowings for this fused site are not in the openings the close carries")]
    FusedParamsMissing,
    #[error("the class's state chunk map is not the tiled one a dissection's checkpoint route reads")]
    NotTheTiledMap,
    #[error("the class's tiled layout at {positions} positions does not exist: {why}")]
    TiledGeometryUnavailable { positions: u32, why: String },
    #[error("the dissection refused the move: {0}")]
    AttnCourt(#[from] crate::palw_attn_court_v1::PalwAttnCourtError),
    #[error("a dissection close is adjudicated inside its session, where the phase it answers lives")]
    DissectionCloseNeedsItsSession,
    #[error(
        "this ruleset admits no dissection arity that fits its own window ({window_court} DAA) over the widest row its classes \
         register — the court would run out of clock mid-prosecution"
    )]
    NoAdmissibleArity { window_court: u64 },
    #[error("the root claim declares dissection arity {declared}; this ruleset derives {derived} at this block")]
    ArityIsNotTheDerivedOne { declared: u8, derived: u8 },
    // ---------------------------------------------------------------------------------------
    // ADR-0080 design A, W6 — the split close's declaration
    // ---------------------------------------------------------------------------------------
    /// Named apart from [`Self::RungSignatureInvalid`] because the two are acted on differently by
    /// whoever reads the log: a rung's bad signature is one refused move in a live ladder, and this
    /// one is a whole carriage — a declaration plus every chunk behind it — that will never open a
    /// group.
    #[error(
        "court {session}: the {side} side's close declaration is not signed by that side's bond \
         — either party could otherwise write the other's close and pin it to a verdict it never asserted"
    )]
    CloseDeclarationSignatureInvalid { session: Hash64, side: &'static str },
    /// **The ruleset's own count.** The transition enforces the structural bound the `u64` bitmap
    /// can address ([`crate::palw_state_v2::PALW_COURT_CLOSE_MAX_CHUNKS`]); this is the NETWORK's,
    /// which is inside `palw_ruleset_id_v2` and is 1 on devnet — where a legal close still has to
    /// fit one carrier and the split path must be refused rather than engaged.
    #[error(
        "court {session}: a close declared in {count} chunks, and this ruleset pays to carry {ceiling} \
         (PalwCourtParamsV2::max_close_chunks, inside the ruleset id)"
    )]
    CloseDeclaresTooManyChunks { session: Hash64, count: u64, ceiling: u64 },
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
    /// **ADR-0082 Decision 2's terminal: one history TILE, opened and recomputed.**
    ///
    /// The dissection has narrowed the disputed head's history to a single tile, and this is the
    /// move that ends it: the head's query slice, the tile's K and V rows on either evidence
    /// route, and the committed output tile — each opened against something the claim committed —
    /// with the triple recomputed by the shipped kernels against the ROOT's `(m*, S*)`.
    ///
    /// It carries the same two things every other arm carries and for the same reasons: the
    /// `binding`, so the openings are against THIS claim's execution and not one the accuser
    /// invented, and the `operand_openings`, so the narrowings the recompute uses are the class's
    /// registered ones. The bottom itself is flat in the context (ADR-0082 R4) — one tile, four
    /// openings and their paths — which is the whole reason a fused site can be adjudicated at
    /// 131,072 positions at all.
    AttnDissection {
        binding: Box<crate::palw_step_leg::PalwStepBindingV2>,
        bottom: Box<crate::palw_attn_court_v1::PalwAttnDissectBottomV1>,
        operand_openings: Vec<PalwArtifactOpeningV1>,
    },
    /// **The arithmetic close on a network whose prompt commitment is a Merkle root** (ADR-0081
    /// Decision 3; private-prompts design, 2026-09-05).
    ///
    /// Identical to `Arithmetic` in everything but how the prompt reaches the court: the
    /// refutation carries NO ids (`prompt_token_ids` empty), and the one tile the disputed gather
    /// reads arrives as `prompt_ids_opening`, authenticated against the job context's
    /// `prompt_token_ids_hash` by `verify_prompt_ids_opening_v1`. That is what lets a court convict
    /// a wrong embedding on a hidden prompt without the prompt ever being published: 32 ids and a
    /// path, not the conversation.
    ///
    /// Appended AFTER `AttnDissection` on purpose: a borsh discriminant is positional, so a new
    /// variant anywhere else would re-number every close testnet-11 has accepted. Admissible only
    /// where `Params::palw_prompt_ids_form_v1()` is `MerkleV1` — on a flat network it is refused by
    /// name before its opening is read (`check_close_speaks_the_networks_prompt_form`), and the
    /// same gate refuses an `Arithmetic` that carries a whole id list on a Merkle network.
    ArithmeticOpened {
        refutation: PalwExecutionStepRefutationV1,
        operand_openings: Vec<PalwArtifactOpeningV1>,
        prompt_ids_opening: crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1,
    },
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

/// **The responder's opening move in a dissection, under the responder's key** (ADR-0082
/// Decision 2). Mirrors [`check_court_disclosure_acceptance_v2`] exactly — same party, same
/// registry lookup, different context and message — because it IS a disclosure: the first one, of
/// the whole range.
pub fn check_court_attn_root_claim_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    root: &crate::palw_attn_dissect::PalwAttnRootClaimV1,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let (_session, claim) = resolve_court_session_v2(state, session_id)?;
    let bond = state.bond(&claim.bond).ok_or(PalwCourtV2Error::ChallengerMissing(claim.bond))?;
    let message = palw_attn_root_claim_message_v1(session_id, root);
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// **One round of children, under the responder's key.** The round number the signature covers is
/// the SESSION's — see [`palw_attn_round_message_v1`] — so a disclosure signed for round 3 is not
/// a legal move at round 4 even though its bytes are unchanged.
pub fn check_court_attn_round_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    disclosure: &crate::palw_attn_dissect::PalwAttnDissectRoundV1,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let (session, claim) = resolve_court_session_v2(state, session_id)?;
    let phase = session.dissection.as_ref().ok_or(PalwCourtV2Error::NoDissection(*session_id))?;
    let bond = state.bond(&claim.bond).ok_or(PalwCourtV2Error::ChallengerMissing(claim.bond))?;
    let message = palw_attn_round_message_v1(session_id, phase.round(), disclosure);
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// **The challenger's child index, under the challenger's key.** The choice carries its own
/// session id and round, so the message is the object.
pub fn check_court_attn_choice_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    choice: &crate::palw_attn_court_v1::PalwAttnDissectChoiceV1,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    let (session, _claim) = resolve_court_session_v2(state, session_id)?;
    if choice.session_id != *session_id {
        return Err(PalwCourtV2Error::SessionIdMismatch);
    }
    let bond = state.bond(&session.challenger_bond).ok_or(PalwCourtV2Error::ChallengerMissing(session.challenger_bond))?;
    let message = borsh::to_vec(choice).expect("a dissection choice is borsh-serializable");
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_ATTN_CHALLENGER_CONTEXT) {
        return Err(PalwCourtV2Error::RungSignatureInvalid);
    }
    Ok(())
}

/// **What a `CourtCloseDeclared` binds** (ADR-0080 design A, W6).
///
/// Every field of the object except the signature itself, in the object's own order, borsh-encoded
/// — the same construction the two rung messages use ("the message signed is the canonical encoding
/// of the rung itself"). Each term is load-bearing and none of them is decoration:
///
/// * `session_id` and `side` are WHO: the pair is the group's key, and a signature that did not
///   carry both could be lifted from one side of a session onto the other;
/// * `count` and `chunk_digests` are the BYTES: they are what makes a chunk's arrival checkable
///   against something the declarer committed to, and a signature over the count alone would let
///   anyone re-pin the parts;
/// * `close_digest` is the WHOLE, so a declaration cannot pin consistent parts of an inconsistent
///   whole and disown the assembly;
/// * `verdict` is the CLAIM: it is what W7 compares against what the assembled proof actually
///   adjudicates, so a declarer signs the outcome it says its own evidence produces.
///
/// A digest is deliberately not taken here. The signing context is the domain separation (ML-DSA-87
/// binds it into the signature), and hashing first would put a second keyed domain in the way of a
/// reader trying to check by hand what a declaration actually said.
pub fn palw_court_close_declaration_message_v1(
    session_id: &Hash64,
    side: crate::palw_state_v2::PalwCourtSideV1,
    count: u8,
    chunk_digests: &[Hash64],
    close_digest: &Hash64,
    verdict: PalwCourtVerdictV2,
) -> Vec<u8> {
    borsh::to_vec(&(*session_id, side, count, chunk_digests.to_vec(), *close_digest, verdict))
        .expect("every field of a close declaration is borsh-serializable")
}

/// **Who may declare a split close, and under whose key** (ADR-0080 design A, W6 — P0-9's forgery
/// half, in the one place the split close had left open).
///
/// The transition READS the declarer from the two bonds the session id already binds — the
/// challenger from the session record, the executor from the claim — so a declaration can never
/// name a third party and the row can never be squatted. What it cannot do is prove the side
/// AUTHORISED it, because the bond registry's keys live in the candidate state and the transition
/// arm has no verifier. That is this function, and it is the same split
/// [`check_court_disclosure_acceptance_v2`] and [`check_court_verdict_acceptance_v2`] already use.
///
/// Unsigned, the split close is strictly worse than absent. A declaration suspends the mover's rung
/// while the bytes ride behind it, is singular per `(session, side)` for the life of the session,
/// and pins a verdict; so either party could open the OTHER's group, spend its one declaration on a
/// count it cannot assemble, and take the failure the sweep hands a declarer that does not finish —
/// which for an executor is `void_and_slash`. One unsigned object would convict an honest producer.
///
/// The key is the bond's own registered `pubkey`, exactly as the rung checks read it: neither party
/// names its own key, and a bond that is not in the registry at this chain point is refused before
/// any signature is verified.
#[allow(clippy::too_many_arguments)]
pub fn check_court_close_declaration_acceptance_v2<V>(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    side: crate::palw_state_v2::PalwCourtSideV1,
    count: u8,
    chunk_digests: &[Hash64],
    close_digest: &Hash64,
    verdict: PalwCourtVerdictV2,
    signature: &[u8],
    verify_mldsa87: V,
) -> Result<(), PalwCourtV2Error>
where
    V: Fn(&[u8], &[u8], &[u8], &[u8]) -> bool,
{
    use crate::palw_state_v2::PalwCourtSideV1;
    let (session, claim) = resolve_court_session_v2(state, session_id)?;
    let declarer = match side {
        PalwCourtSideV1::Challenger => session.challenger_bond,
        PalwCourtSideV1::Executor => claim.bond,
    };
    let bond = state.bond(&declarer).ok_or(PalwCourtV2Error::ChallengerMissing(declarer))?;
    let message = palw_court_close_declaration_message_v1(session_id, side, count, chunk_digests, close_digest, verdict);
    if !verify_mldsa87(&bond.pubkey, &message, signature, PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT) {
        return Err(PalwCourtV2Error::CloseDeclarationSignatureInvalid { session: *session_id, side: side.name() });
    }
    Ok(())
}

/// **The ruleset's own carriage count, applied to a declaration** (ADR-0080 design A, W6).
///
/// `PalwCourtParamsV2::max_close_chunks` is inside `palw_ruleset_id_v2`, it is what class admission
/// prices a class against, and W5 deliberately did not compare it: the transition has no court
/// parameters and enforces only the structural bound its `u64` bitmap can address
/// (`PALW_COURT_CLOSE_MAX_CHUNKS = 32`). So the network's number is checked HERE, where the ruleset
/// is in hand — and it is the tighter of the two on both shipped bundles (27 on the RC, **1 on
/// devnet**, where the pre-ADR-0080 byte ceiling frames to a single carrier and a close that does
/// not fit one is refused rather than split).
pub fn check_close_declared_chunk_count_v2(
    session_id: &Hash64,
    count: u8,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
) -> Result<(), PalwCourtV2Error> {
    if u64::from(count) > court.max_close_chunks() {
        return Err(PalwCourtV2Error::CloseDeclaresTooManyChunks {
            session: *session_id,
            count: u64::from(count),
            ceiling: court.max_close_chunks(),
        });
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

/// **A close speaks the network's prompt-commitment form or it is not a move** (ADR-0081
/// Decision 3). `ArithmeticOpened` on a flat network, and an `Arithmetic` that carries a whole id
/// list on a Merkle network, are refused BY NAME here rather than left to fail inside the checker
/// as `InputSetNotCanonical`: the arithmetic already cannot be fooled (a flat digest is no Merkle
/// root and a Merkle root is no flat digest), so what this buys is a refusal an operator can read
/// and a court that admits exactly one spelling of "here is the prompt" per network. An
/// `Arithmetic` with NO ids is a move on either network — it addresses no gather.
fn check_close_speaks_the_networks_prompt_form(
    proof: &PalwCourtVerdictProofV2,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<(), PalwCourtV2Error> {
    match (proof, form) {
        (PalwCourtVerdictProofV2::ArithmeticOpened { .. }, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat) => {
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { close: "ArithmeticOpened", form: "the flat digest" })
        }
        (PalwCourtVerdictProofV2::Arithmetic { refutation, .. }, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1)
            if !refutation.prompt_token_ids.is_empty() =>
        {
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { close: "whole-id-list Arithmetic", form: "a Merkle root" })
        }
        _ => Ok(()),
    }
}

/// The binding every close carries, whichever scheme it uses.
fn binding_of(proof: &PalwCourtVerdictProofV2) -> &crate::palw_step_leg::PalwStepBindingV2 {
    match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, .. } | PalwCourtVerdictProofV2::ArithmeticOpened { refutation, .. } => {
            &refutation.binding
        }
        PalwCourtVerdictProofV2::DecodeToken { binding, .. } => binding,
        PalwCourtVerdictProofV2::DecodeTokenTiled { binding, .. } => binding,
        PalwCourtVerdictProofV2::AttnDissection { binding, .. } => binding,
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
    // **ADR-0084 U-08: the step ladder this adjudication walks at.** `PALW_STEP_LEG_MAX_LEAVES`
    // before `Params::palw_court_ladder`, the ruleset's `max_step_leaf_count()` past it — decided
    // by the caller at the BLOCK's DAA (`palw_court_step_ladder_at`), never read here from a
    // constant, so two nodes grading one close proof read one ladder.
    step_ladder: u64,
    // ADR-0081 Decision 3: which spelling of "here is the prompt" this network admits — decided by
    // the caller at the block's DAA (`palw_prompt_ids_form_at`), genesis-only so it cannot differ.
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
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
        PalwCourtVerdictProofV2::Arithmetic { refutation, .. } | PalwCourtVerdictProofV2::ArithmeticOpened { refutation, .. } => {
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
        // **ADR-0082 Decision 2: the fused terminal is not a recompute, it is the end of an
        // exchange — so it is adjudicated HERE, where the phase it answers lives.**
        //
        // Every other arm can be graded from the claim alone, which is why `adjudicate_close_proof_v2`
        // takes one. This one is graded against `session.dissection`: the tile it may open, the
        // claim it is compared with and the `(m*, S*)` it is computed against are all facts of the
        // phase, and a bottom judged without them would be a recompute of a range nobody narrowed.
        PalwCourtVerdictProofV2::AttnDissection { binding, bottom, operand_openings } => {
            // **The fence is read off the SESSION, not off a DAA the caller supplies.** A phase
            // exists only because `PalwAttnDissectPhaseV1::open` was given an active fence at the
            // block that opened it, and a fork activation is monotonic — so `Some` IS the record
            // that the k-ary court was armed for this dispute, and resolving it a second time at
            // the closing block's DAA would be a second answer to a settled question.
            let phase = session.dissection.as_ref().ok_or(PalwCourtV2Error::NoDissection(*session_id))?;
            let class = state.class(&claim.class_id).ok_or(PalwCourtV2Error::MissingClass(claim.class_id))?;
            let operands = PalwProvenOperandsV1::from_openings_v1(operand_openings, class.artifact_root)
                .map_err(|e| PalwCourtV2Error::OperandProofInvalid(e.to_string()))?;
            let derived = palw_attn_dispute_site_v2(claim, binding, &operands, narrowed, bottom.anchor.as_ref())?;
            // The claim's own trace root, the same pin the arithmetic arm applies — so a bottom
            // cannot open rows of an execution that merely shares a class.
            check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(binding))?;
            // **The committed output row is checked HERE and nowhere else.**
            //
            // The phase admitted the responder's root claim by finalizing `V*` to a tile; the
            // tile it was finalized against was opened against this binding when the phase opened
            // (`CourtAttnRootClaimed`), so by the time the bottom arrives the loop is already
            // closed. What remains is that the bottom answers the SAME leaf — the one the ladder
            // narrowed to — which is the procedural rule every arm above applies in its own
            // spelling.
            if bottom.out_tile.opening.leaf_index != narrowed {
                return Err(PalwCourtV2Error::CloseIsNotTheNarrowedStep { opened: bottom.out_tile.opening.leaf_index, narrowed });
            }
            let verdict =
                crate::palw_attn_court_v1::check_attn_dissect_bottom_v1(phase, bottom, &derived.binding, &derived.site, true)?;
            return Ok(match verdict {
                crate::palw_attn_court_v1::PalwAttnCourtVerdictV1::ExecutorGuilty => PalwCourtVerdictV2::ExecutorGuilty,
                crate::palw_attn_court_v1::PalwAttnCourtVerdictV1::ChallengerDefeated => PalwCourtVerdictV2::ChallengerDefeated,
            });
        }
    }
    adjudicate_close_proof_v2(state, claim, proof, court, step_ladder, prompt_ids_form)
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
    // **ADR-0084 U-08: the step ladder this adjudication walks at.** `PALW_STEP_LEG_MAX_LEAVES`
    // before `Params::palw_court_ladder`, the ruleset's `max_step_leaf_count()` past it — decided
    // by the caller at the BLOCK's DAA (`palw_court_step_ladder_at`), never read here from a
    // constant, so two nodes grading one close proof read one ladder.
    step_ladder: u64,
    // ADR-0081 Decision 3: which spelling of "here is the prompt" this network admits — decided by
    // the caller at the block's DAA (`palw_prompt_ids_form_at`), genesis-only so it cannot differ.
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    check_close_cost_v2(proof, court)?;
    check_close_speaks_the_networks_prompt_form(proof, prompt_ids_form)?;
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
            map_refutation_outcome(check_execution_step_refutation_capped_v1(refutation, &operands, step_ladder))
        }
        // The same close with the prompt tile opened instead of carried (ADR-0081 Decision 3):
        // the same two pins, the same operand proofs, and the checker's opened entry, which
        // authenticates the tile against the job context's Merkle commitment before a single id
        // is read. `prompt_ids_form` has already said this arm is the network's.
        PalwCourtVerdictProofV2::ArithmeticOpened { refutation, operand_openings, prompt_ids_opening } => {
            check_arithmetic_close_binding(claim.trace_root, binding_logits_root_of(&refutation.binding))?;
            check_execution_root_binding(claim.execution_root, refutation.binding.committed_execution_root)?;
            let class = state.class(&claim.class_id).ok_or(PalwCourtV2Error::MissingClass(claim.class_id))?;
            let operands = PalwProvenOperandsV1::from_openings_v1(operand_openings, class.artifact_root)
                .map_err(|e| PalwCourtV2Error::OperandProofInvalid(e.to_string()))?;
            map_refutation_outcome(crate::palw_step_refute::check_execution_step_refutation_opened_capped_v1(
                refutation,
                &operands,
                Some(prompt_ids_opening),
                step_ladder,
            ))
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
            // **…and the same ladder.** The two arms above take `step_ladder` and this one kept
            // the default-ladder name — one predicate, four arms, one missed (ADR-0084 U-08;
            // mainnet audit 2026-09-06 H-4). It is byte-identical on every network that exists,
            // because `palw_refutation_leaf_cap_v2` IS `PALW_STEP_LEG_MAX_LEAVES` while
            // `Params::palw_court_ladder` is dormant, and it is the correct rule past the fence.
            map_refutation_outcome(crate::palw_step_refute::check_tiled_decode_token_refutation_capped_v1(
                binding,
                pin,
                step_ladder,
            ))
        }
        // The one arm that cannot be graded from the claim alone — see `adjudicate_court_close_v2`,
        // which returns before reaching here. Refused rather than silently acquitted, because an
        // arm that fell through to "no fault found" would read as `ChallengerDefeated`.
        PalwCourtVerdictProofV2::AttnDissection { .. } => Err(PalwCourtV2Error::DissectionCloseNeedsItsSession),
    }
}

// =================================================================================================
// ADR-0082 Decision 3 — the arity a ruleset derives at activation
// =================================================================================================

/// **May a dissection move ride a block at all?** (ADR-0082 Decision 3.)
///
/// The rule lives here, beside the court rules it belongs to, rather than inline in the
/// acceptance walk that reads it — the walk is where a rule is APPLIED, and a fence spelled at
/// its application site is a fence the next application site has to be told about.
///
/// `false` on every shipped preset, so the answer is a refusal BY NAME: the arms exist in the
/// binary and the rule does not exist on this chain. The block that carried it still stands; the
/// object is what is refused, which is the drop-not-invalidate shape admission on the lifecycle
/// band requires.
pub fn palw_attn_move_is_admissible_v2(
    object: &crate::palw_state_v2::PalwConsensusObjectV2,
    kary_court_active: bool,
) -> Result<(), PalwCourtV2Error> {
    use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;
    let is_dissection =
        matches!(object, Obj::CourtAttnRootClaimed { .. } | Obj::CourtAttnDissected { .. } | Obj::CourtAttnChildChosen { .. });
    if is_dissection && !kary_court_active { Err(PalwCourtV2Error::KaryCourtDormant) } else { Ok(()) }
}

/// **The two quantities the arity derivation reads off the REGISTERED classes** — the widest
/// history any admitted row disputes, and the widest output tile it disputes it at.
///
/// Walked over the genesis set's class admissions, because that is the set the ruleset id commits
/// to. A class registered later is admitted through `verify_class_admission_v2`, which applies
/// `palw_attn_court_admits_row_v1` against the arity in force — so a row wider than this walk saw
/// is refused at ITS admission rather than silently widening a court that is already open.
///
/// A ruleset with no fused attention site answers `(0, 0)`: no dissection has rounds, and the
/// derivation then returns whatever the leaf ladder alone needs.
pub fn palw_attn_widest_registered_site_v2(bundle: &crate::palw_mode_v2::PalwConsensusParamsV2) -> (u64, usize) {
    use crate::palw_state_v2::PalwConsensusObjectV2;
    use crate::palw_step::PalwStepOpKindV1;
    let mut history = 0u64;
    let mut lanes = 0usize;
    for object in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { admission: Some(carriage), .. } = object else { continue };
        let profile = &carriage.profile;
        let mut fused = false;
        for slot in 0..profile.global_node_count() {
            let Some((node, _)) = profile.resolve_node_slot(slot) else { continue };
            if node.op_kind != PalwStepOpKindV1::AttnFused {
                continue;
            }
            fused = true;
            // The disputed window is one tile of the fused output row, and never wider than the
            // head it lives in — the dissection is about one head's softmax.
            lanes = lanes.max((node.tile_len as usize).min(profile.attn_head_dim as usize));
        }
        if fused {
            history = history.max(u64::from(profile.n_ctx));
        }
    }
    (history, lanes)
}

/// **The leaf ladder the refutation walkers open against, as a function of the fence** — the one
/// spelling `Params` and the acceptance pipeline share (ADR-0084 U-08).
///
/// Dormant: [`crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES`], the executor's constant every
/// shipped block has been judged under. Armed: the ruleset's own `max_step_leaf_count` — what
/// admission priced the class at, so a class is prosecuted at the ladder it was admitted at. The
/// processor's `palw_court_step_ladder_at` is this function at the block's own DAA; nothing else
/// may spell the choice.
pub fn palw_refutation_leaf_cap_v2(court: &crate::palw_mode_v2::PalwCourtParamsV2, court_ladder_active: bool) -> u64 {
    if court_ladder_active { court.max_step_leaf_count() } else { crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES }
}

/// **ADR-0082 Decision 3 at activation: the court a session is judged under.**
///
/// Under a dormant `Params::palw_kary_court` this is the bundle's own court, byte for byte — the
/// dormancy property every shipped preset is tested for. Under an armed one the arity becomes
/// [`crate::palw_mode_v2::palw_court_arity_v1`] of the ruleset's own quantities: the window and
/// the SA-4 deadline, the ladder's `max_step_leaf_count`, the terminal move count, and the widest
/// row the registered classes admit, at the class map's tile.
///
/// **`None` is a refusal, never a fallback to 2.** An arity of 2 is a legal value the derivation
/// can RETURN; using it when the derivation returns nothing would mean a window that cannot hold
/// the dispute it admits quietly running a court that overruns it, which is the exact failure
/// ADR-0082 Z4 exists to make impossible. The startup gate is where this belongs as a refusal to
/// ASSEMBLE (`Params::validate_palw_v2`); here it is a refusal to judge.
pub fn palw_court_params_at_v2(
    bundle: &crate::palw_mode_v2::PalwConsensusParamsV2,
    kary_court_active: bool,
) -> Result<crate::palw_mode_v2::PalwCourtParamsV2, PalwCourtV2Error> {
    if !kary_court_active {
        return Ok(bundle.court);
    }
    let court = bundle.court;
    let (history_max, widest_lane_count) = palw_attn_widest_registered_site_v2(bundle);
    let arity = crate::palw_mode_v2::palw_court_arity_v1(
        bundle.state.window_court(),
        court.turn_deadline_daa(),
        court.max_step_leaf_count(),
        history_max,
        crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4,
        court.terminal_rounds(),
        widest_lane_count,
        // The reserve is the window's, so the search that chooses the shape the window must hold
        // reads the same chunk count the admission bound does (audit D H-2c).
        court.max_close_chunks(),
    )
    .ok_or(PalwCourtV2Error::NoAdmissibleArity { window_court: bundle.state.window_court() })?;
    court.with_dissection_arity(arity).map_err(|e| PalwCourtV2Error::BindingInvalid(e.to_string()))
}

// =================================================================================================
// ADR-0082 Decision 2 — the site a dissection is about, DERIVED
// =================================================================================================

/// **Everything a dissection needs to know about the disputed site, read off the CLASS.**
///
/// [`crate::palw_attn_court_v1`] is a pure checker: it takes the site and the binding as structs
/// and states, in its own words, that "`site` is the leaf the LADDER terminated on … the caller
/// derives it from the terminal leaf and the class's profile". This is that caller's half — the
/// arm's job named in ADR-0082 U-03 — and it is derived from three things a mover cannot choose:
/// the leaf the session already narrowed to, the profile the claim's `class_id` pins, and the
/// operand rows the class's `artifact_root` pins.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwAttnDisputeSiteV2 {
    pub site: crate::palw_attn_court_v1::PalwAttnBottomSiteV1,
    /// `(head, lane_first, lane_count)` — the triple the phase refuses a root claim against.
    pub head_lanes: (u16, u16, u16),
    /// The history the site reads at the disputed position — `kv_len` at that coordinate.
    pub history_positions: u32,
    /// The court's history tile, read off the class's own map rather than typed.
    pub tile_positions: u32,
    pub binding: crate::palw_attn_court_v1::PalwAttnBottomBindingV1,
}

/// One `A16QuantParams` triple from a proven operand row, at the class's registered name.
fn attn_fused_triple_v2(
    operands: &PalwProvenOperandsV1,
    name: &str,
    layer: Option<u16>,
) -> Result<crate::palw_base0_a16::A16QuantParams, PalwCourtV2Error> {
    use crate::palw_step_refute::PalwWeightOracleV1;
    let width = crate::palw_base0_a16::A16QuantParams::WIRE_BYTES as u32;
    let bytes = operands.operand_bytes(name, layer, 0, width).ok_or(PalwCourtV2Error::FusedParamsMissing)?;
    crate::palw_base0_a16::A16QuantParams::from_wire(&bytes).map_err(|_| PalwCourtV2Error::FusedParamsMissing)
}

/// **The site the ladder narrowed to, as the registered class describes it** (ADR-0082
/// Decision 2 / Decision 4).
///
/// The two pins run FIRST and they are the shipped close's own: the profile must be the claim's
/// class (a class id IS its `shape_profile_id`), and the binding must recompute to the claim's
/// committed `execution_root`. Everything below is then read out of material the chain pinned:
///
/// * the coordinate of `narrowed_leaf` under `(profile, job_context)` — which decode call, which
///   node slot, which position, which output tile;
/// * the node at that slot, which must be `AttnFused` or this is not a dissection's dispute;
/// * the head and the lane window, from the tile index and the profile's head width. A tile that
///   straddles two heads is REFUSED rather than split: the whole protocol is "one head's row max,
///   one head's exponent sum", and a claim about two heads has no `(m*, S*)`;
/// * `kv_off`, the head's GQA slice within a cache row, by the same `h / (heads / kv_heads)` map
///   `a16_attn_fused_via_tiles_v1` uses — one mapping, not two;
/// * the history `kv_len`, which the canonical enumeration already fixes per coordinate;
/// * the four narrowings, from the operand openings, proven against the class's artifact root.
///
/// `anchor` is the checkpoint a `Checkpoint`-route bottom reads from, and it contributes ONE
/// thing: how many positions that checkpoint covers, from which the class's tiled layout follows.
/// WHICH checkpoint may be the anchor is deliberately not decided here — `palw_step_refute`'s
/// `verify_kv_anchor` owns that rule, and one refusal for it belongs in one place.
///
/// **The opening cap is the STRUCTURAL one**, `PALW_STEP_MAX_LEAVES`, and not the ruleset's
/// `max_step_leaf_count`. It bounds how deep a Merkle path may be, and by the time an opening is
/// walked here the binding has already been pinned to `claim.execution_root` — so
/// `binding.step_leaf_count` is the claim's own committed count, which admission already held
/// under the ruleset's ceiling. Reading the ruleset's number instead would put a bundle quantity
/// in a derivation the CHAIN's fold has to reproduce without one, and two layers resolving it
/// differently is a state-root split, not a tighter bound.
pub fn palw_attn_dispute_site_v2(
    claim: &crate::palw_state_v2::PalwClaimStateV2,
    binding: &crate::palw_step_leg::PalwStepBindingV2,
    operands: &PalwProvenOperandsV1,
    narrowed_leaf: u64,
    anchor: Option<&crate::palw_attn_court_v1::PalwAttnCheckpointAnchorV1>,
) -> Result<PalwAttnDisputeSiteV2, PalwCourtV2Error> {
    use crate::palw_step::PalwStepOpKindV1;
    check_close_profile_is_the_registered_class(claim.class_id, binding)?;
    check_execution_root_binding(claim.execution_root, binding.committed_execution_root)?;
    // `verify_binding` is what makes `committed_execution_root` a PIN rather than a field: it
    // recomputes the root from the job context, both profile hashes, the leaf and checkpoint
    // counts and their roots, so pinning the root pins every part the derivation below reads.
    let (job_context_hash, shape_profile_hash, checkpoint_profile_hash) =
        crate::palw_step_leg::verify_binding_v1(binding).map_err(|e| PalwCourtV2Error::BindingInvalid(e.to_string()))?;
    let profile = &binding.shape_profile;

    let coord = crate::palw_step::canonical_step_coordinates(profile, &binding.job_context, narrowed_leaf)
        .ok_or(PalwCourtV2Error::NotACanonicalLeaf(narrowed_leaf))?;
    let (node, layer) = profile.resolve_node_slot(coord.node_slot).ok_or(PalwCourtV2Error::NotACanonicalLeaf(narrowed_leaf))?;
    if node.op_kind != PalwStepOpKindV1::AttnFused {
        return Err(PalwCourtV2Error::NotAFusedLeaf { op: node.op_kind });
    }

    let heads = u64::from(profile.attn_heads);
    let kv_heads = u64::from(profile.attn_kv_heads);
    let d_head = u64::from(profile.attn_head_dim);
    if heads == 0 || kv_heads == 0 || d_head == 0 || !heads.is_multiple_of(kv_heads) {
        return Err(PalwCourtV2Error::FusedGeometryUnservable(
            "a fused site needs heads, kv heads that divide them, and a head width",
        ));
    }
    let row = heads.checked_mul(d_head).ok_or(PalwCourtV2Error::FusedGeometryUnservable("the fused output row overflows"))?;
    let tile_len = u64::from(node.tile_len);
    // The lane window this leaf commits, in the FULL output row: the enumeration cuts the row
    // into `tile_len`-wide tiles and the coordinate names which one.
    let global_first = u64::from(coord.tile_index)
        .checked_mul(tile_len)
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("the fused tile index overflows the row"))?;
    if global_first >= row {
        return Err(PalwCourtV2Error::NotACanonicalLeaf(narrowed_leaf));
    }
    let lane_span = tile_len.min(row - global_first);
    // **One head, or nothing.** `(m*, S*)` is a property of ONE softmax row; a tile covering two
    // heads is two rows, and there is no claim the dissection could be about. It cannot happen on
    // a graph whose fused tile divides `d_head`, which is what the shipped v5 rows do — so this is
    // a refusal about the class, named, not a case to handle.
    let head = global_first / d_head;
    let lane_first = global_first % d_head;
    if lane_first + lane_span > d_head {
        return Err(PalwCourtV2Error::FusedTileStraddlesHeads { first: global_first, count: lane_span, d_head });
    }
    let group = heads / kv_heads;
    let kv_off = (head / group)
        .checked_mul(d_head)
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("the head's cache slice overflows the row"))?;
    let kv_dim = kv_heads.checked_mul(d_head).ok_or(PalwCourtV2Error::FusedGeometryUnservable("the cache row overflows"))?;

    // ONE spelling of the absolute position (`palw_absolute_position_v1`): the history a fused
    // site at this coordinate reads is every position up to and including its own.
    let history_positions = u64::from(
        crate::palw_context_ladder::palw_absolute_position_v1(&binding.job_context, coord.call_index, coord.position)
            .and_then(|p| p.checked_add(1))
            .ok_or(PalwCourtV2Error::NotACanonicalLeaf(narrowed_leaf))?,
    );
    let history_positions =
        u32::try_from(history_positions).map_err(|_| PalwCourtV2Error::FusedGeometryUnservable("the history is wider than a u32"))?;
    if history_positions == 0 {
        return Err(PalwCourtV2Error::NotACanonicalLeaf(narrowed_leaf));
    }

    // The four registered narrowings, from ONE description shared with the engine's plan compiler
    // and the whole-row adjudication arm.
    let tensors = crate::palw_step_refute::palw_attn_fused_tensors_v1(node.weight_name.as_str())
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("the fused node's weight name is not a registered softmax spelling"))?;
    let up_bits = {
        use crate::palw_step_refute::PalwWeightOracleV1;
        let bytes = operands.operand_bytes(tensors.softmax_up.as_str(), layer, 0, 1).ok_or(PalwCourtV2Error::FusedParamsMissing)?;
        *bytes.first().ok_or(PalwCourtV2Error::FusedParamsMissing)?
    };
    let params = crate::palw_base0_a16::A16AttnFusedParamsV1 {
        scores: attn_fused_triple_v2(operands, tensors.scores.as_str(), layer)?,
        probs: attn_fused_triple_v2(operands, tensors.probs.as_str(), layer)?,
        values: attn_fused_triple_v2(operands, tensors.values.as_str(), layer)?,
        up_bits: up_bits.min(62),
    };

    // **The court's tile is the CLASS's**, at the disputed history — the same `min(tile, positions)`
    // the tiled map derives, so a checkpoint chunk and a dissection child are the same span by
    // construction rather than by a shared constant two files spell separately.
    // The court's tile is the CLASS's map's tile, not the tiled map's assumed: a fused class that
    // registered a map addressing no attention cache is refused by name rather than dissected at
    // a tile its map cannot open.
    // (`history_positions` is already `u32`; clippy 1.93.0 refuses the conversion that said so.)
    let tile_positions = crate::palw_state_chunk_map::palw_map_history_tile_positions_v1(profile, history_positions)
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("this class's map addresses no attention cache"))?;

    // The anchor's layout, only when one is in evidence. `anchor_positions` is the checkpoint's
    // own coverage; a geometry that described a different history would let a chunk index point
    // at another position's rows, and `verified_anchor_v1` refuses exactly that mismatch.
    // **The rotated-query node, and where this head's slice lives in its committed row** (audit
    // A C-3 / E C-3). The fused node's first input ref IS the query row (the fusion writes
    // `nodes[i].input_refs[0]` there, and the projection test asserts it), so the slot, the tile
    // and the offset inside that tile are all facts of the CLASS. Without them the bottom read
    // any committed `d_head`-wide leaf as the query: another head's row, another position's.
    let q_ref = *node.input_refs.first().ok_or(PalwCourtV2Error::FusedGeometryUnservable("a fused node with no query input"))?;
    if q_ref >= crate::palw_step::PALW_STEP_INPUT_SENTINEL_MIN {
        return Err(PalwCourtV2Error::FusedGeometryUnservable("the fused node's query input is a sentinel, not a committed row"));
    }
    let layer_index =
        layer.ok_or(PalwCourtV2Error::FusedGeometryUnservable("a fused site outside a layer table has no cache layer"))?;
    let table = profile.layer_table(layer_index);
    let q_node = table
        .get(q_ref as usize)
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("the fused node's query input names no node of this layer"))?;
    let query_slot = profile
        .global_node_slot(crate::palw_step::PalwStepTableV1::Attn, layer_index, q_ref as usize)
        .ok_or(PalwCourtV2Error::FusedGeometryUnservable("the query node has no global slot"))?;
    let crate::palw_step::PalwStepOutLenV1::Fixed { elements: q_row_len } = q_node.out_len else {
        return Err(PalwCourtV2Error::FusedGeometryUnservable(
            "the query row is context-shaped; a graph-v5 class commits no such row",
        ));
    };
    let q_tile_len = u64::from(q_node.tile_len);
    if q_tile_len == 0 || u64::from(q_row_len) < row {
        return Err(PalwCourtV2Error::FusedGeometryUnservable("the query row is narrower than the heads it must supply"));
    }
    let q_lane = head.checked_mul(d_head).ok_or(PalwCourtV2Error::FusedGeometryUnservable("the head's query offset overflows"))?;
    let query_tile_index = q_lane / q_tile_len;
    let query_lane_offset = q_lane % q_tile_len;
    let query_tile_lanes = q_tile_len.min(u64::from(q_row_len) - query_tile_index * q_tile_len);
    // **One opening, one head.** A tile narrower than `d_head` — or a head straddling two tiles —
    // makes the head's query slice several openings, which this bottom cannot carry: refused by
    // name here rather than surfacing as a width mismatch at the first dispute (the shape is a
    // property of the registered class, so the admission gate should refuse it too — patch note).
    if query_lane_offset + d_head > query_tile_lanes {
        return Err(PalwCourtV2Error::FusedGeometryUnservable(
            "the head's query slice is not inside one tile of the rotated-query row: a bottom carries one query opening",
        ));
    }

    // **The nodes that WRITE the two caches**, by the role the graph declares — the same
    // resolution `palw_step_refute` gives the `KV_K` / `KV_V` input sentinels. Exactly one node
    // may hold each role; two would make "the K cache" ambiguous and a court that had to choose
    // would be choosing the evidence.
    let by_role = |want: crate::palw_step::PalwStepNodeRoleV1| -> Option<(u32, u32)> {
        let mut found = table.iter().enumerate().filter(|(_, n)| n.role == want);
        let (idx, n) = found.next()?;
        if found.next().is_some() {
            return None;
        }
        profile.global_node_slot(crate::palw_step::PalwStepTableV1::Attn, layer_index, idx).map(|slot| (slot, n.tile_len))
    };
    let k_writer = by_role(crate::palw_step::PalwStepNodeRoleV1::KCacheWrite);
    let v_writer = by_role(crate::palw_step::PalwStepNodeRoleV1::VCacheWrite);
    // The two writers must tile alike or their leaves are not the same kind of row; the court
    // reads one width, and a class whose two series disagree about it has no cache-write route.
    let kv_tile_lanes = match (k_writer, v_writer) {
        (Some((_, kt)), Some((_, vt))) if kt == vt => kt as usize,
        _ => 0,
    };

    // **Which checkpoint this step's evidence must anchor at** — the one rule for it, asked here
    // and refused at the bottom by name.
    let anchor_covered = crate::palw_context_ladder::palw_checkpoint_covered_for_step_v1(
        profile,
        &binding.job_context,
        coord.call_index,
        coord.position,
    );

    let (anchor_geometry, anchor_positions) = match anchor {
        None => (None, 0),
        Some(_) => {
            // Only the pure attention map enumerates chunks the way the bottom reads them. A
            // hybrid's composed map interleaves recurrence chunks, so its indices are not this
            // enumeration's — refused by name rather than read through the wrong layout.
            // The composed hybrid map's attention slice IS the tiled map's enumeration, at chunk
            // indices `0..attn.chunk_count()`; its recurrence chunks follow and this bottom never
            // reads them — so the v5 hybrid row carries a dissection anchor like the dense one.
            if profile.state_chunk_map_id != crate::palw_state_chunk_map::tiled_kv_state_chunk_map_id_v3()
                && profile.state_chunk_map_id != crate::palw_state_chunk_map::hybrid_state_chunk_map_id_v3()
            {
                return Err(PalwCourtV2Error::NotTheTiledMap);
            }
            // **At the cadence the CLASS's map runs** (ADR-0082 Decision 4, amended). On a
            // per-decode-call class this IS `integer_kv_positions_at_v1`; on a class whose map
            // addresses history tiles the leaf's counter already IS a position count, and reading
            // it through the per-call rule would describe a history `prefill` rows longer than the
            // checkpoint holds — a chunk index pointing at another position's rows, which is the
            // mismatch `verified_anchor_v1` exists to refuse.
            // **From the DERIVED anchor, never from the one in evidence** (ADR-0082 Decision 4;
            // audit A C-4). Reading the coverage off the supplied leaf made the geometry a
            // function of the challenger's choice — every anchor "described" its own history, so
            // the shape checks below could never disagree with it. The anchor a step's evidence
            // must carry is `palw_checkpoint_covered_for_step_v1`, exactly, and the bottom refuses
            // any other by name (`WrongAnchor`).
            let covered = anchor_covered.ok_or(PalwCourtV2Error::FusedGeometryUnservable(
                "no checkpoint of this class covers the disputed position, so a checkpoint-route bottom has no anchor",
            ))?;
            let positions = crate::palw_context_ladder::palw_checkpoint_positions_at_v1(profile, &binding.job_context, covered);
            let geometry = crate::palw_state_chunk_map::tiled_kv_state_geometry_v3(profile, positions)
                .map_err(|e| PalwCourtV2Error::TiledGeometryUnavailable { positions, why: e.to_string() })?;
            (Some(geometry), positions)
        }
    };

    let as_u16 = |v: u64, what: &'static str| u16::try_from(v).map_err(|_| PalwCourtV2Error::FusedGeometryUnservable(what));
    let as_u32 = |v: u64, what: &'static str| u32::try_from(v).map_err(|_| PalwCourtV2Error::FusedGeometryUnservable(what));
    Ok(PalwAttnDisputeSiteV2 {
        site: crate::palw_attn_court_v1::PalwAttnBottomSiteV1 {
            params,
            kv_dim: kv_dim as usize,
            kv_off: kv_off as usize,
            d_head: d_head as usize,
            attn_layer: layer_index,
            disputed: coord,
            prefill_positions: binding.job_context.declared_prefill_tokens,
            query_slot,
            query_tile_index: as_u32(query_tile_index, "the query tile index")?,
            query_tile_lanes: query_tile_lanes as usize,
            query_lane_offset: query_lane_offset as usize,
            k_slot: k_writer.map(|(slot, _)| slot),
            v_slot: v_writer.map(|(slot, _)| slot),
            kv_tile_lanes,
            anchor_covered_decode_call: anchor_covered,
            anchor_geometry,
            anchor_positions,
            // **Read from the class, never from the wire** (ADR-0082 Decision 4, amended). A class
            // whose map addresses history tiles has a checkpoint after every position, so the
            // unsound cache-write evidence route has no position left to serve and the bottom
            // checker refuses it.
            every_position_is_checkpointed: matches!(
                crate::palw_context_ladder::palw_checkpoint_cadence_v1(profile),
                crate::palw_context_ladder::PalwCheckpointCadenceV1::PerPosition
            ),
        },
        head_lanes: (as_u16(head, "the head index")?, as_u16(lane_first, "the lane offset")?, as_u16(lane_span, "the lane count")?),
        history_positions,
        tile_positions,
        binding: crate::palw_attn_court_v1::PalwAttnBottomBindingV1 {
            job_context_hash,
            shape_profile_hash,
            step_root: binding.step_merkle_root,
            step_leaf_count: binding.step_leaf_count,
            max_step_leaf_count: crate::palw_step::PALW_STEP_MAX_LEAVES,
            checkpoint_merkle_root: binding.checkpoint_merkle_root,
            checkpoint_leaf_count: u64::from(binding.checkpoint_count),
            checkpoint_profile_hash,
            state_chunk_map_id: binding.state_chunk_map_id,
        },
    })
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
        PalwCourtVerdictProofV2::Arithmetic { operand_openings, .. }
        | PalwCourtVerdictProofV2::ArithmeticOpened { operand_openings, .. } => operand_openings,
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
        // **The bottom, measured as it rides.** The object is a nest of openings, chunk bytes and
        // paths whose count depends on which of the two evidence routes each kind took, so the
        // only honest measure of it is its own encoding — the same encoding the carrier pays for.
        // Every other arm counts a payload it can decompose; this one cannot be decomposed
        // without re-deriving the route, and a cost gate that re-derives is a cost gate that
        // costs.
        PalwCourtVerdictProofV2::AttnDissection { bottom, .. } => {
            let bytes = borsh::to_vec(bottom.as_ref()).map(|b| b.len() as u64).unwrap_or(u64::MAX);
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
    let (refutation, operand_openings, prompt_ids_opening) = match proof {
        PalwCourtVerdictProofV2::Arithmetic { refutation, operand_openings } => (refutation, operand_openings, None),
        PalwCourtVerdictProofV2::ArithmeticOpened { refutation, operand_openings, prompt_ids_opening } => {
            (refutation, operand_openings, Some(prompt_ids_opening))
        }
        _ => return None,
    };
    // The opened prompt tile rides with the close and is charged like the operand paths beside it
    // (ADR-0081 Decision 3's admission price, `prompt_ids_close_bytes_v1`, is this same measure
    // taken before any opening exists).
    let mut bytes: u64 = prompt_ids_opening.map(crate::palw_prompt_ids_v1::prompt_ids_opening_bytes_v1).unwrap_or(0);
    bytes = bytes.saturating_add(
        operand_openings
            .iter()
            .map(|o| (o.operand.bytes.len() as u64).saturating_add((o.path.len() as u64).saturating_mul(64)))
            .fold(0u64, |a, b| a.saturating_add(b)),
    );
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
    /// ADR-0084 U-08: every test that predates the fence adjudicates at the shipped ladder.
    const LADDER: u64 = crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES;
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

    /// **ADR-0081 Decision 3: a close speaks the network's prompt-commitment form or it is not a
    /// move** (private-prompts design, 2026-09-05).
    ///
    /// Four cases, two per network. On a flat network an `ArithmeticOpened` is refused by name
    /// before its opening is read; on a Merkle network an `Arithmetic` that carries a whole id
    /// list is refused the same way, and one that carries none is a move on either (it addresses
    /// no gather). The gate names the refusal; the arithmetic is what makes it sound — a flat
    /// digest is no Merkle root — and the cost bound charges the opening like the operand paths.
    #[test]
    fn a_close_speaks_the_networks_prompt_form_or_it_is_not_a_move() {
        use crate::palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_ids_opening_v1};
        let (refutation, openings, _) = crate::palw_step_refute::tests::base0_matmul_fraud();
        let opening = prompt_ids_opening_v1(&[7u32, 8], 0).expect("opens");

        let mut listed = refutation.clone();
        listed.prompt_token_ids = vec![7, 8];
        let bare = PalwCourtVerdictProofV2::Arithmetic { refutation: refutation.clone(), operand_openings: openings.clone() };
        let listed = PalwCourtVerdictProofV2::Arithmetic { refutation: listed, operand_openings: openings.clone() };
        let opened = PalwCourtVerdictProofV2::ArithmeticOpened {
            refutation: refutation.clone(),
            operand_openings: openings.clone(),
            prompt_ids_opening: opening.clone(),
        };

        assert!(check_close_speaks_the_networks_prompt_form(&bare, PalwPromptIdsFormV1::Flat).is_ok());
        assert!(check_close_speaks_the_networks_prompt_form(&listed, PalwPromptIdsFormV1::Flat).is_ok());
        assert!(matches!(
            check_close_speaks_the_networks_prompt_form(&opened, PalwPromptIdsFormV1::Flat),
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { close: "ArithmeticOpened", .. })
        ));
        assert!(check_close_speaks_the_networks_prompt_form(&bare, PalwPromptIdsFormV1::MerkleV1).is_ok());
        assert!(matches!(
            check_close_speaks_the_networks_prompt_form(&listed, PalwPromptIdsFormV1::MerkleV1),
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { close: "whole-id-list Arithmetic", .. })
        ));
        assert!(check_close_speaks_the_networks_prompt_form(&opened, PalwPromptIdsFormV1::MerkleV1).is_ok());

        // The opened close is priced as the bare one plus its opening — and nothing else moved.
        let bare_bytes = arithmetic_close_bytes_v2(&bare).expect("an arithmetic close measures");
        let opened_bytes = arithmetic_close_bytes_v2(&opened).expect("an opened close measures");
        assert_eq!(opened_bytes, bare_bytes + crate::palw_prompt_ids_v1::prompt_ids_opening_bytes_v1(&opening));
        assert_eq!(binding_of(&opened), binding_of(&bare), "the opened close carries the same binding");
    }

    /// **The opened arm adjudicates a claim whose prompt commitment is a Merkle root** — the
    /// routing half of ADR-0081 Decision 3, on a real claim state: the same two pins the flat
    /// arm runs, then the opened checker with the tile. The opening is verified against the job
    /// context's root before the step is judged (the step layer's own test shows a lying tile
    /// refused); here the honest one lets an honest step be judged on its merits, which is
    /// `ChallengerDefeated`, and the SAME close is refused by name on a flat network, as the
    /// whole-list form is on this one.
    #[test]
    fn an_opened_arithmetic_close_adjudicates_on_a_merkle_network_and_nowhere_else() {
        use crate::palw_prompt_ids_v1::PalwPromptIdsFormV1;
        let (refutation, opening) = crate::palw_step_refute::tests::base0_merkle_prompt_honest();
        let trace_root = refutation.binding.full_logits_trace_root;
        let execution_root = refutation.binding.committed_execution_root;
        let cid = refutation.binding.shape_profile.shape_profile_id();
        let p = params_for(cid);
        let artifact_root = h64(0xA7);
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
        let claim_rec = s2.claim(&claim_id).expect("the claim exists");

        let opened = PalwCourtVerdictProofV2::ArithmeticOpened {
            refutation: refutation.clone(),
            operand_openings: Vec::new(),
            prompt_ids_opening: opening,
        };
        assert_eq!(
            adjudicate_close_proof_v2(&s2, claim_rec, &opened, &court(), LADDER, PalwPromptIdsFormV1::MerkleV1),
            Ok(PalwCourtVerdictV2::ChallengerDefeated),
            "an honest step, judged on its merits through the opened arm"
        );
        assert!(matches!(
            adjudicate_close_proof_v2(&s2, claim_rec, &opened, &court(), LADDER, PalwPromptIdsFormV1::Flat),
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { .. })
        ));
        let mut listed = refutation;
        listed.prompt_token_ids = vec![7, 8];
        let listed = PalwCourtVerdictProofV2::Arithmetic { refutation: listed, operand_openings: Vec::new() };
        assert!(matches!(
            adjudicate_close_proof_v2(&s2, claim_rec, &listed, &court(), LADDER, PalwPromptIdsFormV1::MerkleV1),
            Err(PalwCourtV2Error::CloseFormIsNotTheNetworks { .. })
        ));
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
        let verdict = adjudicate_close_proof_v2(
            &in_court,
            claim_rec,
            &proof,
            &court(),
            LADDER,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("a recomputable step adjudicates");
        assert_eq!(verdict, PalwCourtVerdictV2::ExecutorGuilty, "a wrong MatMul is a conviction, not an Unadjudicable");

        // ADR-0084 U-08: the ladder the court walks at is an ARGUMENT, and this refutation's own
        // leaf count is its boundary — one short and the court does not adjudicate, by name and
        // in neither direction; at it, the conviction above.
        let leaves = match &proof {
            PalwCourtVerdictProofV2::Arithmetic { refutation, .. } => refutation.binding.step_leaf_count,
            _ => unreachable!("the proof built above is Arithmetic"),
        };
        assert!(
            matches!(
                adjudicate_close_proof_v2(
                    &in_court,
                    claim_rec,
                    &proof,
                    &court(),
                    leaves - 1,
                    crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
                ),
                Err(PalwCourtV2Error::DoesNotAdjudicate(_))
            ),
            "a ladder one leaf short of the refutation's space refuses rather than acquits or convicts"
        );
        assert_eq!(
            adjudicate_close_proof_v2(
                &in_court,
                claim_rec,
                &proof,
                &court(),
                leaves,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            )
            .expect("at its own space it adjudicates"),
            PalwCourtVerdictV2::ExecutorGuilty
        );

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
            let verdict = adjudicate_close_proof_v2(
                &in_court,
                claim_rec,
                &proof,
                &court(),
                LADDER,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            )
            .expect("a carried pin adjudicates");
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
        let verdict = adjudicate_close_proof_v2(
            &in_court,
            claim_rec,
            &proof,
            &court(),
            LADDER,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        )
        .expect("an honest step adjudicates");
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
        let outcome = adjudicate_close_proof_v2(
            &in_court,
            claim_rec,
            &bogus,
            &court(),
            LADDER,
            crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        );
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
        let procedural =
            adjudicate_court_close_v2(&in_court, &sid, &bogus, &court(), LADDER, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat);
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
            adjudicate_court_close_v2(
                &in_court,
                &h64(0xDEAD),
                &bogus,
                &court(),
                LADDER,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
            ),
            Err(PalwCourtV2Error::MissingSession(_))
        ));
    }

    #[test]
    fn the_court_domains_are_distinct_from_nothing_by_accident() {
        // The party id, and the two rung signing contexts. The count is pinned so ADDING a domain
        // is a decision someone makes here rather than a line that slips in; the cross-family
        // collision test reads the list itself.
        // Six: the party id, the opening, the two ladder rungs, and ADR-0082's two dissection
        // contexts (the responder's moves and the challenger's).
        assert_eq!(PALW_COURT_V2_ALL_DOMAINS.len(), 7);
        assert!(
            PALW_COURT_V2_ALL_DOMAINS.contains(&PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT),
            "misaka-cli probes this list for the close context; a constant that is not IN it is a context nothing can be signed under"
        );
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
                adjudicate_court_close_v2(&state, &sid, &many, &tight, LADDER, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
                Err(PalwCourtV2Error::TooManyOperands { got: 3, ceiling: 2 })
            ),
            "three openings against a ceiling of two"
        );

        // Within the count and over the bytes — refused by size.
        let fat = proof(vec![opening(2_048, 0)]);
        assert!(
            matches!(
                adjudicate_court_close_v2(&state, &sid, &fat, &tight, LADDER, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
                Err(PalwCourtV2Error::CloseTooLarge { .. })
            ),
            "2 KiB against a 1 KiB ceiling"
        );

        // And the Merkle path counts toward it: a path is 64 bytes a peer chose, and a node has to
        // walk every one. Sixteen path elements is 1 KiB on their own.
        let long_path = proof(vec![opening(1, 17)]);
        assert!(
            matches!(
                adjudicate_court_close_v2(
                    &state,
                    &sid,
                    &long_path,
                    &tight,
                    LADDER,
                    crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat
                ),
                Err(PalwCourtV2Error::CloseTooLarge { .. })
            ),
            "a long path is an opening someone still has to walk"
        );

        // Inside both ceilings, the bound says nothing and the close proceeds to the questions
        // that need chain state — here `MissingSession`, which is the point: the cost gate is out
        // of the way, and it never became the reason for anything else.
        let small = proof(vec![opening(4, 0)]);
        assert!(
            matches!(
                adjudicate_court_close_v2(&state, &sid, &small, &tight, LADDER, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
                Err(PalwCourtV2Error::MissingSession(_))
            ),
            "a proof inside the ceilings must reach the state questions"
        );

        // The gate is cheapest-first: an oversized object is refused without the session lookup
        // that would otherwise report first. `state` here holds no session at all.
        assert!(matches!(
            adjudicate_court_close_v2(&state, &sid, &many, &tight, LADDER, crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat),
            Err(PalwCourtV2Error::TooManyOperands { .. })
        ));
    }
    // =============================================================================================
    // ADR-0082 Decision 3 — the fence, and the arity at activation
    // =============================================================================================

    /// Every shipped ruleset that actually carries a V2 bundle — the four network presets and the
    /// RC parameters a testnet-11 build ships. The RC is in the list because the four presets do
    /// not all carry a bundle, and a dormancy test over an empty list is a test that passes
    /// because it ran nothing.
    fn shipped_bundles() -> Vec<(&'static str, crate::palw_mode_v2::PalwConsensusParamsV2)> {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS, TESTNET_PARAMS, palw_rc_shipped_params};
        let bundles: Vec<_> = [
            ("mainnet", MAINNET_PARAMS),
            ("testnet", TESTNET_PARAMS),
            ("simnet", SIMNET_PARAMS),
            ("devnet", DEVNET_PARAMS),
            ("rc", palw_rc_shipped_params()),
        ]
        .into_iter()
        .filter_map(|(name, preset)| match &preset.palw_consensus_mode {
            crate::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => Some((name, bundle.clone())),
            _ => None,
        })
        .collect();
        assert!(!bundles.is_empty(), "no shipped ruleset carries a V2 bundle — the dormancy tests would prove nothing");
        bundles
    }

    /// **Under a dormant fence every shipped preset's court is byte-identical** (ADR-0082
    /// Decision 3's dormancy clause).
    ///
    /// This is the whole of what "nothing is armed" means for the court: the arity a session's
    /// children are cut at, the ceilings a close is measured by, and the clock a rung runs on are
    /// the values the bundle already carries — so a build with this code and a build without it
    /// judge the same disputes identically.
    #[test]
    fn a_dormant_fence_leaves_every_shipped_presets_court_byte_identical() {
        for (name, bundle) in shipped_bundles() {
            let dormant = palw_court_params_at_v2(&bundle, false).expect("a dormant fence never refuses");
            assert_eq!(dormant, bundle.court, "{name}: a dormant k-ary fence moved the court");
            assert_eq!(dormant.dissection_arity(), 2, "{name}: a shipped preset runs the binary ladder");
        }
    }

    /// **Armed, the arity is the DERIVATION's — recomputed here from the same ruleset quantities,
    /// so the test cannot agree with a value the code invented.**
    #[test]
    fn an_armed_fence_takes_the_arity_the_ruleset_derives() {
        for (name, bundle) in shipped_bundles() {
            let (history_max, lanes) = palw_attn_widest_registered_site_v2(&bundle);
            let expected = crate::palw_mode_v2::palw_court_arity_v1(
                bundle.state.window_court(),
                bundle.court.turn_deadline_daa(),
                bundle.court.max_step_leaf_count(),
                history_max,
                crate::palw_state_chunk_map::PALW_ATTN_HISTORY_TILE_V4,
                bundle.court.terminal_rounds(),
                lanes,
                bundle.court.max_close_chunks(),
            );
            match (palw_court_params_at_v2(&bundle, true), expected) {
                (Ok(court), Some(k)) => {
                    assert_eq!(court.dissection_arity(), k, "{name}: the armed court is not the derived arity");
                    // Everything BUT the arity is the bundle's own: arming swaps one court
                    // parameter, which is the `palw_context_ladder` shape this fence copies.
                    assert_eq!(
                        court.with_dissection_arity(bundle.court.dissection_arity()).unwrap(),
                        bundle.court,
                        "{name}: arming moved a court parameter other than the arity"
                    );
                }
                (Err(PalwCourtV2Error::NoAdmissibleArity { .. }), None) => {}
                (got, want) => panic!("{name}: armed court {got:?} against derivation {want:?}"),
            }
        }
    }

    /// **`None` is a refusal, never a fallback to 2.**
    ///
    /// Two is a legal value the derivation can RETURN, so a court that answered 2 when the
    /// derivation answered nothing would be indistinguishable from one whose window really did fit
    /// the binary ladder — and the window that cannot hold its own dispute is exactly the state
    /// ADR-0082 Z4 exists to make impossible.
    #[test]
    fn a_window_that_cannot_hold_its_dispute_refuses_rather_than_falling_back_to_two() {
        let (_, mut bundle) = shipped_bundles().into_iter().next().expect("a shipped V2 bundle");
        // A deadline as long as the whole window: one move spends it, so no arity fits.
        bundle.court = crate::palw_mode_v2::PalwCourtParamsV2::new(1 << 22, bundle.state.window_court(), 2).expect("a court");
        let err = palw_court_params_at_v2(&bundle, true).expect_err("no arity fits a one-move window");
        assert!(matches!(err, PalwCourtV2Error::NoAdmissibleArity { .. }), "{err}");
        // And dormant, the same ruleset is untouched — the refusal is the fence's, not the court's.
        assert_eq!(palw_court_params_at_v2(&bundle, false).unwrap(), bundle.court);
    }

    /// **A dissection move on a chain that never armed the fence is refused by name**, and every
    /// other object is untouched by the rule.
    #[test]
    fn a_dissection_move_under_a_dormant_fence_is_refused_by_name() {
        use crate::palw_state_v2::PalwConsensusObjectV2 as Obj;
        let choice = Obj::CourtAttnChildChosen {
            session_id: Hash64::from_u64_word(1),
            choice: crate::palw_attn_court_v1::PalwAttnDissectChoiceV1 {
                version: crate::palw_attn_court_v1::PALW_ATTN_COURT_OBJECT_VERSION_V1,
                session_id: Hash64::from_u64_word(1),
                round: 0,
                child: 0,
            },
            signature: vec![0xBB; 8],
        };
        let err = palw_attn_move_is_admissible_v2(&choice, false).expect_err("a dormant chain has no dissection");
        assert!(matches!(err, PalwCourtV2Error::KaryCourtDormant), "{err}");
        assert!(format!("{err}").contains("k-ary court fence is dormant"), "the refusal names the fence: {err}");
        assert!(palw_attn_move_is_admissible_v2(&choice, true).is_ok(), "armed, the same move is admissible");
        // The rule is about these three objects and nothing else.
        let other = Obj::CourtCloseChunk {
            session_id: Hash64::from_u64_word(1),
            side: crate::palw_state_v2::PalwCourtSideV1::Executor,
            index: 0,
            bytes: vec![1],
        };
        assert!(palw_attn_move_is_admissible_v2(&other, false).is_ok(), "the fence does not reach objects it is not about");
    }
}
