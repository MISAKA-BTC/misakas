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
//! **Ladder no-show defaults are NOT acceptable objects on the V2 lineage yet — deliberately.**
//! A `PalwBisectNoShowV1` is "only mintable from the machine's own state", and the machine's
//! state is the rung stream; until the ladder itself is carried in `PalwChainStateV2`, a
//! candidate-scoped validator cannot distinguish a real default from a FORGED one — and a forged
//! executor-default would void any honest claim on demand. That is a critical, not a feature
//! gap. The system stays closed without it for the RC's BASE-0 class:
//!
//! * data served → any holder finds the divergent step offline and convicts **arithmetically**
//!   (no ladder narrows needed when you hold the trace);
//! * data withheld pre-license → the PANEL's `Unavailable` quorum justifies `ProducerDefaulted`
//!   (PR-06);
//! * a challenge nobody finishes → the state machine's court backstop closes it
//!   challenger-side at `opened + window_court`, and the claim's path to `Final` re-arms.
//!
//! The interactive ladder (with its per-rung deadlines and attributable offenses) becomes
//! acceptable exactly when its state is chain-carried; that lands with the reorg-equivalence
//! work, not before.
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

pub const PALW_COURT_V2_ALL_DOMAINS: &[&[u8]] = &[PALW_COURT_V2_DOMAIN_PARTY_ID];

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
    Ok(())
}

/// The proof classes a `CourtClosed` may carry on the V2 lineage today. See the module doc for
/// why ladder defaults are absent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwCourtVerdictProofV2 {
    /// Terminal arithmetic adjudication from carried proofs (Decision 8 / P0-8): every operand
    /// the recomputation touches arrives as an opening against the class's registered artifact
    /// root, and the refutation binds the claim's committed trace root.
    Arithmetic { refutation: PalwExecutionStepRefutationV1, operand_openings: Vec<PalwArtifactOpeningV1> },
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
/// `CourtClosed { verdict }` — with the verdict this function returned, never one the object
/// merely announced.
pub fn adjudicate_court_close_v2(
    state: &PalwChainStateV2,
    session_id: &Hash64,
    proof: &PalwCourtVerdictProofV2,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
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

    fn params() -> PalwStateParamsV2 {
        PalwStateParamsV2::new(
            100,
            10,
            10,
            20,
            500,
            1000,
            h64(1),
            4,
            1000,
            100,
            1000,
            0,
        )
        .unwrap()
    }

    fn bond_key(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }

    fn ctx(block: u64, daa: u64, blue: u64) -> PalwBlockContextV2 {
        PalwBlockContextV2 { block: crate::BlockHash::from_u64_word(block), daa_score: daa, blue_score: blue }
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
            },
            PalwConsensusObjectV2::BondRegistered { bond: bond_key(1), pubkey: vec![7; 4], operator_pubkey: op_key(0x21), collateral: 1_000 },
            PalwConsensusObjectV2::BondRegistered { bond: bond_key(2), pubkey: vec![8; 4], operator_pubkey: op_key(0x22), collateral: 1_000 },
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
        let (s4, _) =
            apply_palw_transition_v2(&s3, &p, &ctx(4, 103, 4), &[PalwConsensusObjectV2::ReceiptLicensed { claim: claim_id, receipts: Vec::new() }], None)
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
            validate_court_opened_v2(&state, &p, &ctx(5, 123, 5), &sid, &claim_id, &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64)
                .is_ok(),
            "the last block AT the deadline can still open"
        );
        // Window lapsed.
        assert!(matches!(
            validate_court_opened_v2(&state, &p, &ctx(5, 124, 5), &sid, &claim_id, &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64),
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
                64
            ),
            Err(PalwCourtV2Error::SelfChallenge)
        ));
        // Unregistered challenger.
        assert!(matches!(
            validate_court_opened_v2(&state, &p, &ctx(5, 110, 5), &sid, &claim_id, &bond_key(9), PalwBisectSpaceV1::StepLeaves, 64),
            Err(PalwCourtV2Error::ChallengerMissing(_))
        ));
        // Space too small to bisect.
        assert!(matches!(
            validate_court_opened_v2(&state, &p, &ctx(5, 110, 5), &sid, &claim_id, &bond_key(2), PalwBisectSpaceV1::StepLeaves, 1),
            Err(PalwCourtV2Error::SpaceTooSmall(1))
        ));
        // Announced id from a different space size: not this dispute.
        assert!(matches!(
            validate_court_opened_v2(&state, &p, &ctx(5, 110, 5), &sid, &claim_id, &bond_key(2), PalwBisectSpaceV1::StepLeaves, 65),
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
            validate_court_opened_v2(&with_prov, &p, &ctx(6, 105, 6), &sid2, &claim2, &bond_key(2), PalwBisectSpaceV1::StepLeaves, 64),
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
            &[PalwConsensusObjectV2::CourtOpened { session_id: sid, claim: claim_id, challenger_bond: bond_key(2) }],
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
            matches!(check_execution_root_binding(claim.execution_root, claim.trace_root), Err(PalwCourtV2Error::ExecutionRootMismatch)),
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

    #[test]
    fn the_court_domains_are_distinct_from_nothing_by_accident() {
        // One domain today; the list exists so the cross-family collision test sees it.
        assert_eq!(PALW_COURT_V2_ALL_DOMAINS.len(), 1);
    }
}
