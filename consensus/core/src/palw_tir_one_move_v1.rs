//! **RFC-0002 Phase F, step F6: the IR one-move court** — an IR claim accused at a named leaf and
//! decided in one move, the IR twin of ADR-0099/0100's `ShardCourtAccused`.
//!
//! Under the held regime (ADR-0103 Decision 1; testnet-12) the acceptance path refuses `CourtOpened`
//! for every claim — "the held regime plays no bisection: accuse the leaf in one move". The legacy
//! one-move accusation carries a `PalwExecutionStepRefutationV1` over a legacy binding, which no IR
//! execution has, so without this an IR claim had no court on such a chain. Here the accusation
//! carries an IR close proof — `TirCone`, `TirLogits`, `TirDecodeTokenTiled` or `TirDecodeToken` — and
//! is adjudicated by exactly the function that adjudicates that proof in a court close
//! (`palw_court_v2::adjudicate_close_proof_v2`), against the claim it names.
//!
//! **The verdict is declared and verified, as a court close's is.** The accusation carries the
//! verdict its proof supports; the acceptance layer re-derives it (the IR court's work limits are the
//! ruleset's court parameters, which the gate holds) and refuses an accusation whose proof does not
//! produce the declared verdict, in either direction. The fold then applies it: `ExecutorGuilty`
//! convicts the claim as the legacy one-move court does; `ChallengerDefeated` charges the accuser what
//! a legacy false accusation costs (`palw_shard_court_false_accusation_charge_v1`).
//!
//! **Signed over everything.** The accuser's ML-DSA-87 covers [`palw_tir_one_move_session_id_v1`] —
//! the network, every field and the proof's digest — under its own context, so an accusation cannot be
//! re-attributed, re-targeted or re-filled.
//!
//! **A dissected leaf is never tried in one move under the held regime** (RFC-0002 F7; ADR-0103
//! Decision 5 for an IR program). A leaf whose cone reduces over the history (spec 04b §9.5.1) costs
//! `O(H)` to recompute, so a cone accusation there is the CHALLENGE: bound to the claim — the binding
//! the claim's, the leaf opened under it and not convicting on its face — it opens a session at
//! `Terminal` on that leaf ([`PalwTirOneMoveOutcomeV1::NeedsDissection`]), and the responder's IR root
//! claim is its first move. Such an accusation declares `ExecutorGuilty` and carries only the leaf
//! ([`crate::palw_tir_court_v1::build_tir_named_leaf_refutation_v1`]).
//!
//! The object (`PalwConsensusObjectV2::TirShardCourtAccused`, tag 62) is appended; below
//! `palw_tir_v1` the acceptance walk drops it by name and the fold refuses it. It spends the block's
//! adjudication slot (`PALW_COURT_CLOSE_MAX_PER_BLOCK`): its evaluation is bounded by the IR court's
//! limits, not by its bytes.

use crate::Hash64;
use crate::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimStateV2, PalwCourtVerdictV2};

pub const PALW_TIR_ONE_MOVE_VERSION_V1: u16 = 1;
/// The key of [`palw_tir_one_move_session_id_v1`].
pub const PALW_TIR_ONE_MOVE_DOMAIN_SESSION_V1: &[u8] = b"misaka-palw/tir/one-move/session/v1";
/// The ML-DSA-87 context the accuser signs the session id under.
pub const PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1: &[u8] = b"misaka-palw/tir/one-move/accuse/mldsa87/v1";

/// **An IR claim accused in one move.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirOneMoveAccusationV1 {
    /// [`PALW_TIR_ONE_MOVE_VERSION_V1`].
    pub version: u16,
    /// The claim accused, and its committed roots as the accuser read them off the chain.
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The claim's bond — named so the object says whom it convicts, and refused if not the claim's.
    pub executor_bond: PalwBondKeyV2,
    /// The bond that stakes on the accusation: Active, at or above the floor, never the claim's.
    pub accuser_bond: PalwBondKeyV2,
    /// The verdict the proof supports; the acceptance layer re-derives it and refuses a mismatch.
    pub verdict: PalwCourtVerdictV2,
    /// An IR close proof (`PalwCourtVerdictProofV2::is_tir_v1`), whose binding is the claim's.
    pub proof: PalwCourtVerdictProofV2,
    /// The accuser's ML-DSA-87 over [`palw_tir_one_move_session_id_v1`] under
    /// [`PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// **What the accuser signs**: the network domain and every field of the accusation but the
/// signature — the proof by the keyed digest of its encoding.
pub fn palw_tir_one_move_session_id_v1(network_domain: &[u8], a: &PalwTirOneMoveAccusationV1) -> Hash64 {
    let proof = borsh::to_vec(&a.proof).expect("a proof is borsh-serializable");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_TIR_ONE_MOVE_DOMAIN_SESSION_V1).to_state();
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(a.execution_root.as_byte_slice());
    s.update(a.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&a.executor_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.verdict).expect("borsh"));
    s.update(&(proof.len() as u64).to_le_bytes());
    s.update(&proof);
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// The bytes a ceiling prices: the proof, on the wire.
pub fn palw_tir_one_move_bytes_v1(a: &PalwTirOneMoveAccusationV1) -> u64 {
    borsh::to_vec(&a.proof).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

/// **The accusation's own shape** — stateless, asked where it rides and again at acceptance: the
/// version, a signature, an IR proof, and a proof whose binding speaks about the roots the
/// accusation names.
pub fn palw_tir_one_move_shape_v1(a: &PalwTirOneMoveAccusationV1) -> Result<(), &'static str> {
    if a.version != PALW_TIR_ONE_MOVE_VERSION_V1 {
        return Err("an IR one-move accusation is version 1");
    }
    if a.signature.is_empty() {
        return Err("an IR one-move accusation must carry the accuser's signature");
    }
    let Some(binding) = a.proof.tir_binding_v1() else {
        return Err("an IR one-move accusation carries an IR close proof");
    };
    if binding.committed_execution_root != a.execution_root || binding.full_logits_trace_root != a.trace_root {
        return Err("the proof's binding speaks about other roots than the accusation names");
    }
    if a.executor_bond == a.accuser_bond {
        return Err("an executor does not accuse its own claim");
    }
    Ok(())
}

/// **The verdict the accusation's proof supports against `claim`** — `adjudicate_close_proof_v2`,
/// the court close's own adjudication: its cost ceiling, the network's prompt form, the binding's
/// class, artifact root and roots against the claim's, and the IR court at `step_ladder` with the
/// ruleset's limits.
pub fn palw_tir_one_move_verdict_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwTirOneMoveAccusationV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    if !a.proof.is_tir_v1() {
        return Err(PalwCourtV2Error::DoesNotAdjudicate("not an IR close".into()));
    }
    crate::palw_court_v2::adjudicate_close_proof_v2(state, claim, &a.proof, court, step_ladder, prompt_ids_form)
}

/// **What an IR one-move accusation decides**: a verdict, or — a cone accusation at a dissected leaf
/// under the held regime — the dissection it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirOneMoveOutcomeV1 {
    /// The proof adjudicates; the fold applies this verdict.
    Verdict(PalwCourtVerdictV2),
    /// The named leaf is dissected: the accusation opens a dissection at `leaf`.
    NeedsDissection { leaf: u64 },
}

/// **The dissected leaf a cone accusation names, if it names one** — the proof a `TirCone`, its
/// binding the claim's ([`crate::palw_court_v2::check_tir_binding_is_the_claims_v1`]), and the leaf
/// a dissected leaf of that execution ([`crate::palw_tir_court_v1::palw_tir_named_dissected_leaf_v1`]).
/// `Ok(None)` for any other proof and any other leaf. No evaluation: the fold asks it too.
pub fn palw_tir_one_move_dissected_leaf_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwTirOneMoveAccusationV1,
) -> Result<Option<u64>, PalwCourtV2Error> {
    let PalwCourtVerdictProofV2::TirCone { refutation } = &a.proof else {
        return Ok(None);
    };
    crate::palw_court_v2::check_tir_binding_is_the_claims_v1(state, claim, &refutation.binding)?;
    crate::palw_tir_court_v1::palw_tir_named_dissected_leaf_v1(refutation)
        .map_err(|e| PalwCourtV2Error::DoesNotAdjudicate(format!("the named leaf is not this claim's: {e}")))
}

/// **What the accusation decides against `claim`** — under the held regime (`held_regime`, the
/// caller's `palw_held_context` at the block), a cone accusation at a dissected leaf opens a
/// dissection there after the close ceiling; every other accusation is
/// [`palw_tir_one_move_verdict_v1`]'s.
pub fn palw_tir_one_move_outcome_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwTirOneMoveAccusationV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    held_regime: bool,
) -> Result<PalwTirOneMoveOutcomeV1, PalwCourtV2Error> {
    if !a.proof.is_tir_v1() {
        return Err(PalwCourtV2Error::DoesNotAdjudicate("not an IR close".into()));
    }
    if held_regime {
        crate::palw_court_v2::check_close_cost_v2(&a.proof, court)?;
        if let Some(leaf) = palw_tir_one_move_dissected_leaf_v1(state, claim, a)? {
            return Ok(PalwTirOneMoveOutcomeV1::NeedsDissection { leaf });
        }
    }
    palw_tir_one_move_verdict_v1(state, claim, a, court, step_ladder, prompt_ids_form).map(PalwTirOneMoveOutcomeV1::Verdict)
}

/// **An accusation to sign** — the node's builder: the claim's roots and bond, the accuser, the
/// proof and the verdict it supports, with an empty signature (sign
/// [`palw_tir_one_move_session_id_v1`] under [`PALW_TIR_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1`], then set it).
pub fn palw_tir_one_move_accusation_v1(
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
    accuser_bond: PalwBondKeyV2,
    verdict: PalwCourtVerdictV2,
    proof: PalwCourtVerdictProofV2,
) -> PalwTirOneMoveAccusationV1 {
    PalwTirOneMoveAccusationV1 {
        version: PALW_TIR_ONE_MOVE_VERSION_V1,
        claim: claim_id,
        execution_root: claim.execution_root,
        trace_root: claim.trace_root,
        executor_bond: claim.bond,
        accuser_bond,
        verdict,
        proof,
        signature: Vec::new(),
    }
}
