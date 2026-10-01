//! **RFC-0003 §I.4.7: a pipeline claim accused in one move** — the generative twin of RFC-0002 Phase F's
//! `TirShardCourtAccused` ([`crate::palw_tir_one_move_v1`]), for a chain that plays no bisection.
//!
//! Under the held regime (ADR-0103 Decision 1; testnet-12 from genesis) the acceptance path refuses
//! `CourtOpened` for every claim: "the held regime plays no bisection: accuse the leaf in one move". A claim
//! of a pipeline class (an image, an embedding, a vision-language text claim) has no legacy refutation and
//! its class is no IR program, so without this it had **no court at all** on such a chain — its reservation
//! could never be slashed, and the lane's one deterrent against a false execution was absent exactly where
//! the lane is meant to run. The accusation here carries a generative close — `GenCone` at a divergent leaf,
//! `GenOutputTile` for a canonical output that is not the claim's own step tree's, `GenDecodeToken` for a
//! generated id — and is adjudicated by exactly the function that adjudicates that proof in a court close
//! ([`crate::palw_court_v2::adjudicate_close_proof_v2`]), against the claim it names.
//!
//! **The verdict is declared and verified, as a court close's is.** The accusation carries the verdict its
//! proof supports; the acceptance layer re-derives it (the court's work limits are the ruleset's court
//! parameters, which the gate holds) and refuses an accusation whose proof does not produce the declared
//! verdict, in either direction. The fold then applies it: `ExecutorGuilty` convicts the claim as the legacy
//! one-move court does; `ChallengerDefeated` charges the accuser what a legacy false accusation costs
//! (`palw_shard_court_false_accusation_charge_v1`).
//!
//! **The class is referenced, not carried**: a generative close names its binding's job and the claim's
//! `execution_root`, and the chain holds the registered class (its `gen_classes` row: the pipeline, the
//! programs, the layouts and the weights' root), so an accusation fits one carrier whatever the class's size.
//!
//! **Signed over everything.** The accuser's ML-DSA-87 covers [`palw_gen_one_move_session_id_v1`] — the
//! network, every field and the proof's digest — under its own context, so an accusation cannot be
//! re-attributed, re-targeted or re-filled.
//!
//! **A dissected leaf is never tried in one move under the held regime** (RFC-0002 F7 composed, RFC-0003
//! §II.2.1; ADR-0103 Decision 5). A leaf whose cone reduces over the history costs `O(H)` to recompute, so a
//! cone accusation there is the CHALLENGE: bound to the claim, with the leaf opened under it and not
//! convicting on its face, it opens a session at `Terminal` on that leaf
//! ([`PalwGenOneMoveOutcomeV1::NeedsDissection`]), and the responder's generative root claim
//! (`CourtGenRootClaimed`, tag 69) is its first move. Such an accusation declares `ExecutorGuilty`.
//!
//! The object (`PalwConsensusObjectV2::GenShardCourtAccused`) is appended; below `palw_gen_v1` the
//! acceptance walk drops it by name and the fold refuses it. It spends the block's adjudication slot
//! (`PALW_COURT_CLOSE_MAX_PER_BLOCK`): its evaluation is bounded by the court's limits, not by its bytes.

use crate::Hash64;
use crate::palw_court_v2::{PalwCourtV2Error, PalwCourtVerdictProofV2};
use crate::palw_gen_close_v1::{PalwGenBindingOutcomeV1, PalwGenJobIdsV1, verify_gen_binding_any_v1};
use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimStateV2, PalwCourtVerdictV2};

pub const PALW_GEN_ONE_MOVE_VERSION_V1: u16 = 1;
/// The key of [`palw_gen_one_move_session_id_v1`].
pub const PALW_GEN_ONE_MOVE_DOMAIN_SESSION_V1: &[u8] = b"misaka-palw/gen/one-move/session/v1";
/// The ML-DSA-87 context the accuser signs the session id under.
pub const PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1: &[u8] = b"misaka-palw/gen/one-move/accuse/mldsa87/v1";

/// **A pipeline claim accused in one move.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwGenOneMoveAccusationV1 {
    /// [`PALW_GEN_ONE_MOVE_VERSION_V1`].
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
    /// A generative close proof ([`palw_gen_one_move_proof_is_admissible_v1`]), whose binding is the claim's.
    pub proof: PalwCourtVerdictProofV2,
    /// The accuser's ML-DSA-87 over [`palw_gen_one_move_session_id_v1`] under
    /// [`PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1`].
    pub signature: Vec<u8>,
}

/// **What the accuser signs**: the network domain and every field of the accusation but the signature —
/// the proof by the keyed digest of its encoding.
pub fn palw_gen_one_move_session_id_v1(network_domain: &[u8], a: &PalwGenOneMoveAccusationV1) -> Hash64 {
    let proof = borsh::to_vec(&a.proof).expect("a proof is borsh-serializable");
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_GEN_ONE_MOVE_DOMAIN_SESSION_V1).to_state();
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
pub fn palw_gen_one_move_bytes_v1(a: &PalwGenOneMoveAccusationV1) -> u64 {
    borsh::to_vec(&a.proof).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

/// **Which proofs a one-move accusation may carry**: the three generative closes that decide a claim on
/// their own — `GenCone`, `GenOutputTile` and `GenDecodeToken`. A dissection's bottom (`GenDissection`) is
/// graded against its session's phase and has no session here.
pub fn palw_gen_one_move_proof_is_admissible_v1(proof: &PalwCourtVerdictProofV2) -> bool {
    matches!(
        proof,
        PalwCourtVerdictProofV2::GenCone { .. } | PalwCourtVerdictProofV2::GenOutputTile { .. } | PalwCourtVerdictProofV2::GenDecodeToken { .. }
    )
}

/// **The execution root and the step root a generative close's binding commits** — what the accusation's
/// roots are read against. `None` for a proof that is no generative close.
pub fn palw_gen_proof_roots_v1(proof: &PalwCourtVerdictProofV2) -> Option<(Hash64, Hash64)> {
    match proof {
        PalwCourtVerdictProofV2::GenCone { close } => Some((close.binding.committed_execution_root(), close.binding.step_root())),
        PalwCourtVerdictProofV2::GenOutputTile { close } => Some((close.binding.committed_execution_root, close.binding.step_root())),
        PalwCourtVerdictProofV2::GenDecodeToken { close } => Some((close.binding.committed_execution_root, close.binding.step_root())),
        PalwCourtVerdictProofV2::GenDissection { bottom } => Some((bottom.binding.committed_execution_root(), bottom.binding.step_root())),
        _ => None,
    }
}

/// **The accusation's own shape** — stateless, asked where it rides and again at acceptance: the version, a
/// signature, a generative close the one-move court takes, and a proof whose binding speaks about the
/// execution the accusation names.
pub fn palw_gen_one_move_shape_v1(a: &PalwGenOneMoveAccusationV1) -> Result<(), &'static str> {
    if a.version != PALW_GEN_ONE_MOVE_VERSION_V1 {
        return Err("a generative one-move accusation is version 1");
    }
    if a.signature.is_empty() {
        return Err("a generative one-move accusation must carry the accuser's signature");
    }
    if !palw_gen_one_move_proof_is_admissible_v1(&a.proof) {
        return Err("a generative one-move accusation carries a GenCone, GenOutputTile or GenDecodeToken close");
    }
    let Some((execution_root, _)) = palw_gen_proof_roots_v1(&a.proof) else {
        return Err("a generative one-move accusation carries a generative close");
    };
    if execution_root != a.execution_root {
        return Err("the proof's binding speaks about another execution than the accusation names");
    }
    if a.executor_bond == a.accuser_bond {
        return Err("an executor does not accuse its own claim");
    }
    Ok(())
}

/// **The verdict the accusation's proof supports against `claim`** — `adjudicate_close_proof_v2`, the court
/// close's own adjudication: its cost ceiling, the network's prompt form, the claim's `gen_classes` row and
/// the binding's job and roots against the claim's, and the generative court at the ruleset's limits.
pub fn palw_gen_one_move_verdict_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwGenOneMoveAccusationV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
) -> Result<PalwCourtVerdictV2, PalwCourtV2Error> {
    if !palw_gen_one_move_proof_is_admissible_v1(&a.proof) {
        return Err(PalwCourtV2Error::DoesNotAdjudicate("not a generative close a one-move accusation may carry".into()));
    }
    crate::palw_court_v2::adjudicate_close_proof_v2(state, claim, &a.proof, court, step_ladder, prompt_ids_form)
}

/// **What a generative one-move accusation decides**: a verdict, or — a cone accusation at a dissected
/// leaf under the held regime — the dissection it opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwGenOneMoveOutcomeV1 {
    /// The proof adjudicates; the fold applies this verdict.
    Verdict(PalwCourtVerdictV2),
    /// The named leaf is dissected: the accusation opens a dissection at `leaf`.
    NeedsDissection { leaf: u64 },
}

/// **The dissected leaf a cone accusation names, if it names one** — the proof a `GenCone`, its binding the
/// claim's and verified against the registry (no evaluation), and the leaf a dissected point of that
/// execution (`palw_gen_dissect_site_v1`). `Ok(None)` for any other proof, any other leaf, and a binding that
/// convicts on its face (that leaf is closed, not dissected). The fold asks it too.
pub fn palw_gen_one_move_dissected_leaf_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwGenOneMoveAccusationV1,
) -> Result<Option<u64>, PalwCourtV2Error> {
    let PalwCourtVerdictProofV2::GenCone { close } = &a.proof else {
        return Ok(None);
    };
    let row = state
        .gen_class_v1(&claim.class_id)
        .ok_or_else(|| PalwCourtV2Error::DoesNotAdjudicate(format!("claim {}'s class is not a generative class", claim.class_id)))?;
    // The site needs the job's trips, never its ids' values: zeros of their lengths stand in.
    let verified = match verify_gen_binding_any_v1(&close.binding, row, &claim.class_id, &claim.execution_root, PalwGenJobIdsV1::default())
        .map_err(|e| PalwCourtV2Error::DoesNotAdjudicate(e.to_string()))?
    {
        PalwGenBindingOutcomeV1::Verified(v) => v,
        PalwGenBindingOutcomeV1::Convicted(_) => return Ok(None),
    };
    let coord = close.disputed.coord;
    let index = verified
        .space
        .global_index(&coord)
        .ok_or_else(|| PalwCourtV2Error::DoesNotAdjudicate(format!("{coord:?} is not a leaf of this claim's execution")))?;
    Ok(crate::palw_gen_court_v1::palw_gen_dissect_site_v1(&verified.space.stages[coord.stage as usize], &coord).map(|_| index))
}

/// **What the accusation decides against `claim`** — under the held regime (`held_regime`, the caller's
/// `palw_held_context` at the block), a cone accusation at a dissected leaf opens a dissection there after
/// the close ceiling; every other accusation is [`palw_gen_one_move_verdict_v1`]'s.
pub fn palw_gen_one_move_outcome_v1(
    state: &PalwChainStateV2,
    claim: &PalwClaimStateV2,
    a: &PalwGenOneMoveAccusationV1,
    court: &crate::palw_mode_v2::PalwCourtParamsV2,
    step_ladder: u64,
    prompt_ids_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    held_regime: bool,
) -> Result<PalwGenOneMoveOutcomeV1, PalwCourtV2Error> {
    if !palw_gen_one_move_proof_is_admissible_v1(&a.proof) {
        return Err(PalwCourtV2Error::DoesNotAdjudicate("not a generative close a one-move accusation may carry".into()));
    }
    if held_regime {
        crate::palw_court_v2::check_close_cost_v2(&a.proof, court)?;
        if let Some(leaf) = palw_gen_one_move_dissected_leaf_v1(state, claim, a)? {
            return Ok(PalwGenOneMoveOutcomeV1::NeedsDissection { leaf });
        }
    }
    palw_gen_one_move_verdict_v1(state, claim, a, court, step_ladder, prompt_ids_form).map(PalwGenOneMoveOutcomeV1::Verdict)
}

/// **An accusation to sign** — the node's builder: the claim's roots and bond, the accuser, the proof and
/// the verdict it supports, with an empty signature (sign [`palw_gen_one_move_session_id_v1`] under
/// [`PALW_GEN_ONE_MOVE_MLDSA87_ACCUSE_CONTEXT_V1`], then set it).
pub fn palw_gen_one_move_accusation_v1(
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
    accuser_bond: PalwBondKeyV2,
    verdict: PalwCourtVerdictV2,
    proof: PalwCourtVerdictProofV2,
) -> PalwGenOneMoveAccusationV1 {
    PalwGenOneMoveAccusationV1 {
        version: PALW_GEN_ONE_MOVE_VERSION_V1,
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
