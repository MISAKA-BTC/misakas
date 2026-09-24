//! **ADR-0099 Decision 5, built by ADR-0100 — a court opened at a NAMED leaf, decided in one
//! move.**
//!
//! Today every conviction needs a party that re-executed the whole job: the challenger opens a
//! court over the ruleset's whole step space and answers each rung of the ladder from its own
//! execution, until the bisection lands on one leaf and the terminal check runs. A seat that holds
//! one shard has no whole execution to answer rungs from — but it does not need one. ADR-0077
//! Decision 8 already says what it holds: "a row unequal: the seat files nothing and opens a
//! court at that leaf, as any bonded challenger may, holding the refutation's inputs already."
//! Its interval opening IS the refutation's inputs — the committed rows the leaf reads, opened
//! against the claim's step root — and its shard's artifact is the weight the leaf multiplies by.
//! The seat's own capture check (`fp_capture_samples_clear`) builds exactly this object today.
//!
//! So the one-move court is the terminal check with the search removed: one object names the leaf
//! and carries the refutation, and the chain adjudicates it with the adjudicator it already runs
//! (`check_execution_step_refutation_capped_v1`). A fault convicts the executor of the claim; a
//! refutation that does not prove one convicts the accuser of a false accusation, which is what a
//! bond stakes when it accuses. There is no responder and no clock, because nothing is asked of
//! the accused: the roots are theirs, the artifact is the class's, and the arithmetic is the
//! court's.
//!
//! **Who may accuse.** Any Active bond at or above the registry floor that is not the claim's own
//! — the DA court's rule (ADR-0062 SA-1), for its reason: an accusation is priced, not privileged.
//! The shard a seat holds is the FILER's business ([`palw_shard_court_leaf_is_the_shards_v1`]: a
//! seat never names a leaf it could not have replayed), not the chain's — the chain holds no plan,
//! and a leaf either recomputes or it does not, whoever names it.
//!
//! **What it cannot try, said first.** A fused attention leaf (graph v5's `AttnFused`) has no
//! single-tile terminal: its adjudication is ADR-0082's k-ary dissection over the history, which
//! needs the responder ADR-0093 specifies and no binary produces. [`palw_shard_court_verdict_v1`]
//! answers `NeedsDissection` for such a leaf, and the acceptance layer refuses the object rather
//! than folding a verdict that convicts nobody.
//!
//! **The four the fence named** (`Params::palw_shard_court`, ADR-0099 §3 Decision 5): the object
//! is `PalwConsensusObjectV2::ShardCourtAccused`; the acceptance rule is the processor's arm
//! (fence, bond, signature over [`palw_shard_court_session_id_v1`], shape, ceiling, the verdict
//! derived once); the fold is `palw_state_v2`'s arm (the verdict derived again, at the ladder the
//! block's extras carry, and applied); the signing context is in
//! `PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`, which is why the fence arms only over a bundle that
//! commits to that set. The fence is still `None` on every shipped preset.

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_shard_plan_v1::PalwShardV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PalwShapeProfileV3, PalwStepOpKindV1, canonical_step_coordinates};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_opened_capped_v1};
use crate::palw_v2::PalwJobContextV2;

pub const PALW_SHARD_COURT_VERSION_V1: u16 = 1;
pub const PALW_SHARD_COURT_DOMAIN_SESSION_V1: &[u8] = b"misaka-palw/shard-court/session/v1";
/// The ML-DSA-87 signing context of an accusation. In [`crate::palw_mode_v2::PALW_V2_SIGNATURE_CONTEXTS_COMPLETE_V3`]
/// and in no set a live network committed to: a network that arms `Params::palw_shard_court`
/// states V3 at genesis, and `validate_palw_v2` refuses the fence over any other root.
pub const PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT: &[u8] = b"misaka-palw/shard-court/accuse/mldsa87/v1";
/// Every keyed domain this family uses, for the cross-family uniqueness sweep and the derivation
/// of the committed context set.
pub const PALW_SHARD_COURT_ALL_DOMAINS: &[&[u8]] = &[PALW_SHARD_COURT_DOMAIN_SESSION_V1, PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT];

/// **The accusation: a leaf, named, with what refutes it.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwShardCourtAccusationV1 {
    pub version: u16,
    /// The claim accused, and its committed roots as the accuser read them off the chain. The fold
    /// compares both to the claim's record; the refutation's own binding must carry the same
    /// execution root ([`Self::validate_shape`]).
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    /// The claim's bond — named so the object says whom it convicts, and refused if it is not the
    /// claim's.
    pub executor_bond: PalwBondKeyV2,
    /// The bond that stakes on this accusation: Active, at or above the floor, never the claim's.
    pub accuser_bond: PalwBondKeyV2,
    pub leaf_index: u64,
    /// The refutation — the leaf's committed output tile and its committed inputs, opened against
    /// the claim's roots — and the artifact rows it multiplies by, opened against the class's
    /// registered root.
    pub refutation: PalwExecutionStepRefutationV1,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    /// **The prompt's one tile, where the network commits the ids as a Merkle root** (ADR-0103
    /// Decision 1, carrying ADR-0081 Decision 3's opening into the one-move court). Under the
    /// Merkle form the refutation carries no id list and a prefill gather's tile rides here, opened
    /// against the job's root by the adjudicator before any id is read
    /// ([`crate::palw_step_refute::palw_refutation_prompt_carriage_v1`] builds the pair), so an
    /// accusation grows with a path and never with the prompt. `None` on a flat network and for a
    /// leaf that reads no prompt id. Outside the session id for the refutation's reason: it is
    /// bound by the job's own root, a wrong one is refused by the arithmetic rather than believed,
    /// and a different opening of the same tile does not exist.
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
    /// The accuser's ML-DSA-87 over [`palw_shard_court_session_id_v1`], under
    /// [`PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT`], verified against the bond's registered key.
    pub signature: Vec<u8>,
}

/// The session id: the network domain and every field but the signature and the refutation's
/// bytes — the refutation is bound by its own roots, and an accusation that carried a different
/// refutation for the same leaf is the same accusation (the leaf either recomputes or it does
/// not). The network domain is inside it for the reason every other signed message carries it:
/// a signature on one network must not verify on another.
pub fn palw_shard_court_session_id_v1(network_domain: &[u8], a: &PalwShardCourtAccusationV1) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_SHARD_COURT_DOMAIN_SESSION_V1).to_state();
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(a.execution_root.as_byte_slice());
    s.update(a.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&a.executor_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser_bond).expect("borsh"));
    s.update(&a.leaf_index.to_le_bytes());
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// The bytes a ceiling prices: the refutation, the artifact openings and the prompt tile, on the
/// wire.
pub fn palw_shard_court_accusation_bytes_v1(a: &PalwShardCourtAccusationV1) -> u64 {
    let refutation = borsh::to_vec(&a.refutation).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    let openings = borsh::to_vec(&a.artifact_openings).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    let prompt = a.prompt_ids_opening.as_ref().map_or(0, |o| borsh::to_vec(o).map(|b| b.len() as u64).unwrap_or(u64::MAX));
    refutation.saturating_add(openings).saturating_add(prompt)
}

/// What a false accusation costs its accuser: the claim's own reservation, capped at the registry
/// floor — the same figure `slash_seat` can take, so the charge is the stake and not a number
/// the clamp at collateral makes free (the DA court's `da3` rule, one move).
pub fn palw_shard_court_false_accusation_charge_v1(reserved: u128, min_collateral_sompi: u64) -> u128 {
    reserved.min(u128::from(min_collateral_sompi))
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwShardCourtError {
    #[error("accusation version {got}; this build adjudicates version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("leaf {leaf} is past the ruleset's ladder of {ladder}")]
    LeafPastTheLadder { leaf: u64, ladder: u64 },
    #[error("the accuser is the accused")]
    AccuserIsTheAccused,
    #[error("the refutation addresses leaf {refuted} and the accusation names leaf {named}")]
    RefutationNamesAnotherLeaf { named: u64, refuted: u64 },
    #[error("the refutation's binding commits to execution root {binding} and the accusation names {named}")]
    BindingRootMismatch { binding: Hash64, named: Hash64 },
    #[error("the refutation's profile hashes to {declared}, not to the claim's class {class}")]
    ClassMismatch { declared: Hash64, class: Hash64 },
    #[error("leaf {leaf} is not one of shard {shard}'s: its node slot is {slot}, the shard holds {first}..{end}")]
    LeafOutsideTheShard { leaf: u64, shard: u32, slot: u32, first: u32, end: u32 },
    #[error("leaf {leaf} has no coordinates in this job")]
    LeafHasNoCoordinates { leaf: u64 },
    #[error("the artifact openings do not prove against the class root: {0}")]
    ArtifactOpenings(String),
    #[error("the refutation is malformed: {0:?}")]
    Refutation(PalwStepRefuteError),
    // ---- appended by the bound verdict (`palw_one_move_verdict_bound_v2`); every one refuses ----
    #[error("the refutation's binding commits to execution root {binding} and the claim to {claim}")]
    NotTheClaimsExecution { binding: Hash64, claim: Hash64 },
    #[error("the refutation's binding does not recompute its committed execution root: {0}")]
    BindingDoesNotAuthenticate(crate::palw_step_leg::PalwStepLegError),
    #[error("the named leaf's output tile does not open under the claim's committed step root: {0}")]
    OutputTileNotCommitted(crate::palw_step_leg::PalwStepLegError),
    #[error(
        "leaf {leaf} is a fused-attention site: its history is the dissection's to carry, and the accusation carries \
         {input_rows} input row(s) and {anchors} checkpoint anchor(s)"
    )]
    FusedLeafCarriesTheHistory { leaf: u64, input_rows: usize, anchors: usize },
}

impl PalwShardCourtAccusationV1 {
    /// The shape rules a node applies before any arithmetic — the cheap half.
    pub fn validate_shape(&self, ladder: u64) -> Result<(), PalwShardCourtError> {
        if self.version != PALW_SHARD_COURT_VERSION_V1 {
            return Err(PalwShardCourtError::Version { got: self.version, expected: PALW_SHARD_COURT_VERSION_V1 });
        }
        if self.leaf_index >= ladder {
            return Err(PalwShardCourtError::LeafPastTheLadder { leaf: self.leaf_index, ladder });
        }
        if self.accuser_bond == self.executor_bond {
            return Err(PalwShardCourtError::AccuserIsTheAccused);
        }
        if self.refutation.output_opening.leaf_index != self.leaf_index {
            return Err(PalwShardCourtError::RefutationNamesAnotherLeaf {
                named: self.leaf_index,
                refuted: self.refutation.output_opening.leaf_index,
            });
        }
        if self.refutation.binding.committed_execution_root != self.execution_root {
            return Err(PalwShardCourtError::BindingRootMismatch {
                binding: self.refutation.binding.committed_execution_root,
                named: self.execution_root,
            });
        }
        Ok(())
    }
}

/// What the one move decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwShardCourtVerdictV1 {
    /// The committed tile does not recompute from its committed inputs and the registered
    /// weights: the executor of the claim is guilty, and the claim is void.
    ExecutorGuilty,
    /// The tile recomputes: the accusation is false, and the accuser pays what it staked.
    FalseAccusation,
    /// The leaf is a fused attention site; its terminal is ADR-0082's dissection, which needs
    /// ADR-0093's responder. Neither party is convicted here, and the object is refused.
    NeedsDissection,
}

/// **The one move, adjudicated.** Shape first; then the refutation's own profile must hash to the
/// claim's class (the profile travels in the binding, as a court close's does, and the id IS the
/// declaration); then a fused site is named as such; then the artifact openings must prove
/// against the class's registered root; then the court's own terminal check runs at the
/// ruleset's ladder. The refutation's binding is checked inside that call against the roots the
/// refutation itself carries — the CALLER compares those to the claim's committed roots, as the
/// fold does, because this function holds no chain state.
pub fn palw_shard_court_verdict_v1(
    accusation: &PalwShardCourtAccusationV1,
    class_id: Hash64,
    artifact_root: Hash64,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
    accusation.validate_shape(ladder)?;
    palw_one_move_verdict_v1(
        &accusation.refutation,
        &accusation.artifact_openings,
        accusation.prompt_ids_opening.as_ref(),
        accusation.leaf_index,
        class_id,
        artifact_root,
        ladder,
    )
}

/// **The one move, on its content alone** — the adjudication behind both a `ShardCourtAccused`
/// accusation and a leaf's evidence disclosed to the held DA court (ADR-0111 Decision 1): one
/// function, so a leaf cannot read one way inside an accusation and another inside a disclosure.
/// The refutation's own profile must hash to the claim's class; a fused site is named as such; the
/// artifact openings must prove against the class's registered root; then the court's terminal
/// check runs at the ruleset's ladder, with the prompt tile (where one rides) opened against the
/// job's own root inside it. The caller compares the binding's roots to the claim's.
pub fn palw_one_move_verdict_v1(
    refutation: &PalwExecutionStepRefutationV1,
    artifact_openings: &[PalwArtifactOpeningV1],
    prompt_ids_opening: Option<&PalwPromptIdsOpeningV1>,
    leaf_index: u64,
    class_id: Hash64,
    artifact_root: Hash64,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
    if leaf_index >= ladder {
        return Err(PalwShardCourtError::LeafPastTheLadder { leaf: leaf_index, ladder });
    }
    if refutation.output_opening.leaf_index != leaf_index {
        return Err(PalwShardCourtError::RefutationNamesAnotherLeaf {
            named: leaf_index,
            refuted: refutation.output_opening.leaf_index,
        });
    }
    let profile = &refutation.binding.shape_profile;
    let declared = profile.shape_profile_id();
    if declared != class_id {
        return Err(PalwShardCourtError::ClassMismatch { declared, class: class_id });
    }
    let coord = canonical_step_coordinates(profile, &refutation.binding.job_context, leaf_index)
        .ok_or(PalwShardCourtError::LeafHasNoCoordinates { leaf: leaf_index })?;
    if let Some((node, _)) = profile.resolve_node_slot(coord.node_slot)
        && node.op_kind == PalwStepOpKindV1::AttnFused
    {
        return Ok(PalwShardCourtVerdictV1::NeedsDissection);
    }
    let proven = PalwProvenOperandsV1::from_openings_v1(artifact_openings, artifact_root)
        .map_err(|e| PalwShardCourtError::ArtifactOpenings(format!("{e:?}")))?;
    // The prompt tile, where one rides, is opened against the job's own root inside the check
    // (the commitment is the discriminator: a flat network's root refuses a Merkle opening by name).
    match check_execution_step_refutation_opened_capped_v1(refutation, &proven, prompt_ids_opening, ladder) {
        Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwStepRefuteError::NoFaultFound) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(PalwShardCourtError::Refutation(other)),
    }
}

/// **The claim a one-move verdict is bound to** — read off the chain by the caller (the claim's
/// record and its class's), never off the accusation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwOneMoveClaimV2 {
    /// The claim's committed `execution_root`: the refutation's binding must RECOMPUTE to it.
    pub execution_root: Hash64,
    pub class_id: Hash64,
    pub artifact_root: Hash64,
    // HOOK(F1): the claim's job anchor joins here when F1 lands, and the marked check in
    // `palw_one_move_verdict_bound_v2` compares `binding.job_context.job_id` to it. This tree's
    // claim record carries no anchor, and this struct does not invent one.
}

/// Is `leaf` a fused-attention site of the job `binding` commits? One spelling for the bound
/// verdict and for the filer that strips a fused accusation's history
/// ([`PalwLeafEvidenceV1::for_the_one_move_v2`]).
pub fn palw_leaf_is_fused_v2(binding: &crate::palw_step_leg::PalwStepBindingV2, leaf: u64) -> bool {
    canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf)
        .and_then(|coord| binding.shape_profile.resolve_node_slot(coord.node_slot))
        .is_some_and(|(node, _)| node.op_kind == PalwStepOpKindV1::AttnFused)
}

/// **A refutation as the one move carries it under the bound verdict**: at a fused site its history
/// (the input rows and the checkpoint anchor) is taken out — the dissection carries that, and the
/// bound verdict refuses it here — and at every other leaf it is returned untouched.
pub fn palw_one_move_refutation_v2(mut refutation: PalwExecutionStepRefutationV1) -> PalwExecutionStepRefutationV1 {
    if palw_leaf_is_fused_v2(&refutation.binding, refutation.output_opening.leaf_index) {
        refutation.inputs.clear();
        refutation.kv_checkpoint = None;
    }
    refutation
}

/// **The one move, bound to the claim before anything is deferred** (t12, `palw_audit_2026_09_23`).
///
/// [`palw_one_move_verdict_v1`] answered `NeedsDissection` for a fused site after checking only
/// that the carried profile hashed to the class — before the binding was recomputed, before the
/// output tile was opened, before the artifact rows were proved. The carried job context decided
/// which node the leaf was, and nothing tied that context to the claim but an echoed root, so any
/// bond could open a held dissection on any claim with a hand-built object. Here every check that
/// does not need the dissection runs first, in this order:
///
/// 1. the leaf is inside the ladder and is the one the refutation opens;
/// 2. the carried profile is the claim's class; the leaf has coordinates;
/// 3. the binding's root is the claim's, and the binding RECOMPUTES to it (`verify_binding_v1`) —
///    from here the job context, and so which node the leaf is, is the claim's;
/// 4. (HOOK F1) the job is the claim's anchor's;
/// 5. the artifact openings prove against the class root (all or nothing);
/// 6. at a non-fused leaf: [`check_execution_step_refutation_opened_capped_v1`], exactly as
///    [`palw_one_move_verdict_v1`] runs it — so the non-fused verdict is v1's on every accusation
///    whose roots are the claim's;
/// 7. at a fused leaf: the structural pass at the named leaf (the output tile opened under the
///    committed step root, its preimage the leaf's, the binding's and the tile's shape rules — a
///    fault there convicts, exactly as it does at any other leaf); then the history is ABSENT (the
///    input rows and the checkpoint anchor are what the dissection replaces: the one move cannot
///    derive a fused site's canonical set without walking the history, and evidence it cannot
///    check it must not carry); then the id carriages against the binding. Only then
///    `NeedsDissection` — the one question left, whether the committed tile recomputes over the
///    history, which is the dissection's.
pub fn palw_one_move_verdict_bound_v2(
    refutation: &PalwExecutionStepRefutationV1,
    artifact_openings: &[PalwArtifactOpeningV1],
    prompt_ids_opening: Option<&PalwPromptIdsOpeningV1>,
    leaf_index: u64,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
    if leaf_index >= ladder {
        return Err(PalwShardCourtError::LeafPastTheLadder { leaf: leaf_index, ladder });
    }
    if refutation.output_opening.leaf_index != leaf_index {
        return Err(PalwShardCourtError::RefutationNamesAnotherLeaf {
            named: leaf_index,
            refuted: refutation.output_opening.leaf_index,
        });
    }
    let binding = &refutation.binding;
    let profile = &binding.shape_profile;
    let declared = profile.shape_profile_id();
    if declared != claim.class_id {
        return Err(PalwShardCourtError::ClassMismatch { declared, class: claim.class_id });
    }
    let coord = canonical_step_coordinates(profile, &binding.job_context, leaf_index)
        .ok_or(PalwShardCourtError::LeafHasNoCoordinates { leaf: leaf_index })?;
    if binding.committed_execution_root != claim.execution_root {
        return Err(PalwShardCourtError::NotTheClaimsExecution {
            binding: binding.committed_execution_root,
            claim: claim.execution_root,
        });
    }
    crate::palw_step_leg::verify_binding_v1(binding).map_err(PalwShardCourtError::BindingDoesNotAuthenticate)?;
    // HOOK(F1): `binding.job_context.job_id == claim.job_anchor` goes HERE — after the binding
    // authenticates (so the job id is the claim's committed one) and before any leaf is read.
    let proven = PalwProvenOperandsV1::from_openings_v1(artifact_openings, claim.artifact_root)
        .map_err(|e| PalwShardCourtError::ArtifactOpenings(format!("{e:?}")))?;
    let fused = profile.resolve_node_slot(coord.node_slot).is_some_and(|(node, _)| node.op_kind == PalwStepOpKindV1::AttnFused);
    if !fused {
        return match check_execution_step_refutation_opened_capped_v1(refutation, &proven, prompt_ids_opening, ladder) {
            Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
            Err(PalwStepRefuteError::NoFaultFound) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
            Err(other) => Err(PalwShardCourtError::Refutation(other)),
        };
    }
    // The fused site: everything but the recomputation.
    let structural = crate::palw_step_leg::PalwStepRefutationV1 {
        binding: binding.clone(),
        evidence: crate::palw_step_leg::PalwStepEvidenceV1::StepTile {
            opening: refutation.output_opening.clone(),
            preimage: refutation.output_preimage.clone(),
        },
    };
    match crate::palw_step_leg::check_step_refutation_capped_v1(&structural, ladder) {
        Ok(_) => return Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(crate::palw_step_leg::PalwStepLegError::NoFaultFound) => {}
        Err(e) => return Err(PalwShardCourtError::OutputTileNotCommitted(e)),
    }
    if !refutation.inputs.is_empty() || refutation.kv_checkpoint.is_some() {
        return Err(PalwShardCourtError::FusedLeafCarriesTheHistory {
            leaf: leaf_index,
            input_rows: refutation.inputs.len(),
            anchors: usize::from(refutation.kv_checkpoint.is_some()),
        });
    }
    crate::palw_step_refute::check_refutation_id_carriage_v1(refutation, prompt_ids_opening)
        .map_err(PalwShardCourtError::Refutation)?;
    Ok(PalwShardCourtVerdictV1::NeedsDissection)
}

/// **The one-move verdict at the rule in force** — `bound` is `palw_audit_2026_09_23` at the
/// caller's DAA (the processor's, the fold's extras, a seat's params). Below it,
/// [`palw_shard_court_verdict_v1`], byte for byte.
pub fn palw_shard_court_verdict_at_v2(
    accusation: &PalwShardCourtAccusationV1,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
    bound: bool,
) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
    if !bound {
        return palw_shard_court_verdict_v1(accusation, claim.class_id, claim.artifact_root, ladder);
    }
    accusation.validate_shape(ladder)?;
    palw_one_move_verdict_bound_v2(
        &accusation.refutation,
        &accusation.artifact_openings,
        accusation.prompt_ids_opening.as_ref(),
        accusation.leaf_index,
        claim,
        ladder,
    )
}

/// **A leaf's evidence** (ADR-0111 Decision 1) — a `ShardCourtAccused` accusation's content
/// without the accuser: the refutation's committed half (the output tile and its opening, the
/// canonical input rows, the KV anchor and the decode pin where the leaf reads them), the artifact
/// rows its recomputation reads, and the prompt tile its gather reads. What an executor serves a
/// seat off chain and discloses to the held DA court on chain; adjudicated by
/// [`palw_one_move_verdict_v1`] and nothing else.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwLeafEvidenceV1 {
    pub refutation: PalwExecutionStepRefutationV1,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
}

impl PalwLeafEvidenceV1 {
    /// The leaf this evidence is of.
    pub fn leaf_index(&self) -> u64 {
        self.refutation.output_opening.leaf_index
    }

    /// Decision 1's verdict on this evidence alone.
    pub fn verdict_v1(
        &self,
        class_id: Hash64,
        artifact_root: Hash64,
        ladder: u64,
    ) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
        palw_one_move_verdict_v1(
            &self.refutation,
            &self.artifact_openings,
            self.prompt_ids_opening.as_ref(),
            self.leaf_index(),
            class_id,
            artifact_root,
            ladder,
        )
    }

    /// [`Self::verdict_v1`] at the rule in force — see [`palw_shard_court_verdict_at_v2`].
    pub fn verdict_at_v2(
        &self,
        claim: &PalwOneMoveClaimV2,
        ladder: u64,
        bound: bool,
    ) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
        if !bound {
            return self.verdict_v1(claim.class_id, claim.artifact_root, ladder);
        }
        palw_one_move_verdict_bound_v2(
            &self.refutation,
            &self.artifact_openings,
            self.prompt_ids_opening.as_ref(),
            self.leaf_index(),
            claim,
            ladder,
        )
    }

    /// **What a filer carries into the one move at a fused site, under the bound verdict: the leaf
    /// and not its history.** The input rows and the checkpoint anchor are what the dissection
    /// exists to replace — the bound verdict refuses them there (`FusedLeafCarriesTheHistory`), and
    /// at a held class's positions the anchor alone is past every close ceiling. Every other leaf's
    /// evidence is returned untouched.
    pub fn for_the_one_move_v2(mut self) -> Self {
        self.refutation = palw_one_move_refutation_v2(self.refutation);
        self
    }

    /// The accusation a seat files from this evidence: every content field is the evidence's.
    #[allow(clippy::too_many_arguments)]
    pub fn into_accusation_v1(
        self,
        claim: Hash64,
        execution_root: Hash64,
        trace_root: Hash64,
        executor_bond: PalwBondKeyV2,
        accuser_bond: PalwBondKeyV2,
    ) -> PalwShardCourtAccusationV1 {
        PalwShardCourtAccusationV1 {
            version: PALW_SHARD_COURT_VERSION_V1,
            claim,
            execution_root,
            trace_root,
            executor_bond,
            accuser_bond,
            leaf_index: self.leaf_index(),
            refutation: self.refutation,
            artifact_openings: self.artifact_openings,
            prompt_ids_opening: self.prompt_ids_opening,
            signature: Vec::new(),
        }
    }
}

/// The bytes a leaf's evidence occupies on the wire — the same ceiling an accusation meets.
pub fn palw_leaf_evidence_bytes_v1(evidence: &PalwLeafEvidenceV1) -> u64 {
    borsh::to_vec(evidence).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

/// **The filer's rule, not the chain's:** a seat holding `shard` names only a leaf whose node
/// slot the shard holds — the leaf it replayed. The chain holds no plan and asks nothing of the
/// kind; a seat that named another shard's leaf would be accusing on arithmetic it did not run.
pub fn palw_shard_court_leaf_is_the_shards_v1(
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    shard: &PalwShardV1,
    leaf: u64,
) -> Result<(), PalwShardCourtError> {
    let coord = canonical_step_coordinates(profile, context, leaf).ok_or(PalwShardCourtError::LeafHasNoCoordinates { leaf })?;
    let end = shard.first_slot + shard.slot_count;
    if coord.node_slot < shard.first_slot || coord.node_slot >= end {
        return Err(PalwShardCourtError::LeafOutsideTheShard {
            leaf,
            shard: shard.index,
            slot: coord.node_slot,
            first: shard.first_slot,
            end,
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_checkpoint_court_v1::tests::{HeldFixture, fixture_prompt_ids, held_fixture};
    use crate::palw_step::{PalwStepCoordinateV1, PalwStepOutLenV1, PalwStepTableV1, canonical_step_leaf_index};
    use crate::palw_step_leg::{PalwStepLegError, PalwStepOpeningV1, PalwStepTileLeafV1};

    const LADDER: u64 = 1 << 26;
    const ARTIFACT_ROOT: u64 = 0xA7;

    fn h64(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// Layer 1's fused site at prefill position 12, and its canonical tile-0 width.
    fn fused_site(fx: &HeldFixture) -> (PalwStepCoordinateV1, u32) {
        let profile = &fx.binding.shape_profile;
        let fused = profile.attn_nodes.iter().position(|n| n.op_kind == PalwStepOpKindV1::AttnFused).expect("a fused site");
        let slot = profile.global_node_slot(PalwStepTableV1::Attn, 1, fused).expect("a slot");
        let (node, _) = profile.resolve_node_slot(slot).expect("the node");
        let PalwStepOutLenV1::Fixed { elements } = node.out_len else { panic!("a fused site commits a fixed row") };
        (PalwStepCoordinateV1 { call_index: 0, node_slot: slot, position: 12, tile_index: 0 }, elements.min(node.tile_len))
    }

    /// The fixture with a tile committed at the fused site (canonical unless `count` says otherwise).
    fn fused_fixture(count: Option<u32>) -> (HeldFixture, u64) {
        let fx = held_fixture(true, 20, None);
        let (coord, canonical) = fused_site(&fx);
        let count = count.unwrap_or(canonical);
        let fx = fx.with_committed_tile(coord, count, vec![0u8; 4 * count as usize]);
        let leaf = canonical_step_leaf_index(&fx.binding.shape_profile, &fx.binding.job_context, &coord).expect("a leaf");
        (fx, leaf)
    }

    fn claim_of(fx: &HeldFixture) -> PalwOneMoveClaimV2 {
        PalwOneMoveClaimV2 {
            execution_root: fx.binding.committed_execution_root,
            class_id: fx.class_id(),
            artifact_root: h64(ARTIFACT_ROOT),
        }
    }

    /// The honest fused accusation's content: the claim's binding, the committed tile and its path,
    /// the prompt's tile at the leaf's position, no history, no rows.
    fn honest_evidence(fx: &HeldFixture, leaf: u64) -> PalwLeafEvidenceV1 {
        let preimage = fx.preimages.get(&leaf).expect("the committed tile").clone();
        let position = preimage.coord.position;
        PalwLeafEvidenceV1 {
            refutation: PalwExecutionStepRefutationV1 {
                binding: fx.binding.clone(),
                output_opening: fx.opening(leaf),
                output_preimage: preimage,
                inputs: vec![],
                prompt_token_ids: vec![],
                decode_tokens: None,
                kv_checkpoint: None,
            },
            artifact_openings: vec![],
            prompt_ids_opening: Some(
                crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&fixture_prompt_ids(20), position).expect("the tile"),
            ),
        }
    }

    fn accusation(fx: &HeldFixture, evidence: PalwLeafEvidenceV1) -> PalwShardCourtAccusationV1 {
        let bond = |v: u64| {
            PalwBondKeyV2(crate::tx::TransactionOutpoint { transaction_id: crate::tx::TransactionId::from_u64_word(v), index: 0 })
        };
        let mut a = evidence.into_accusation_v1(h64(0xC1), fx.binding.committed_execution_root, h64(0x7A), bond(1), bond(2));
        a.signature = vec![9; 8];
        a
    }

    /// **The hole, closed: a fused accusation is bound to the claim before it is deferred.** Each
    /// row is one way the pre-fence verdict let an unbound object open a held dissection; bound, each
    /// is refused by name, and the honest object still defers.
    #[test]
    fn a_fused_accusation_is_bound_to_the_claim_before_it_is_deferred() {
        let (fx, leaf) = fused_fixture(None);
        let claim = claim_of(&fx);
        let v1 = |a: &PalwShardCourtAccusationV1| palw_shard_court_verdict_v1(a, claim.class_id, claim.artifact_root, LADDER);
        let v2 = |a: &PalwShardCourtAccusationV1| palw_shard_court_verdict_at_v2(a, &claim, LADDER, true);

        let honest = accusation(&fx, honest_evidence(&fx, leaf));
        assert_eq!(v2(&honest), Ok(PalwShardCourtVerdictV1::NeedsDissection), "the honest object still opens the dissection");
        assert_eq!(v1(&honest), Ok(PalwShardCourtVerdictV1::NeedsDissection));

        // The grief as filed: the claim's own binding, a tile and a path that are nothing.
        let mut garbage = honest.clone();
        garbage.refutation.output_opening.siblings.clear();
        garbage.refutation.output_preimage =
            PalwStepTileLeafV1 { version: 1, coord: garbage.refutation.output_preimage.coord, value_count: 0, values_le: vec![] };
        garbage.prompt_ids_opening = None;
        assert_eq!(v1(&garbage), Ok(PalwShardCourtVerdictV1::NeedsDissection), "pre-fence: the hole");
        assert!(matches!(v2(&garbage), Err(PalwShardCourtError::OutputTileNotCommitted(_))), "{:?}", v2(&garbage));

        // A binding that echoes the claim's root and commits to nothing.
        let mut unbound = honest.clone();
        unbound.refutation.binding.step_merkle_root = h64(0xDEAD);
        assert_eq!(v1(&unbound), Ok(PalwShardCourtVerdictV1::NeedsDissection), "pre-fence: the hole");
        assert_eq!(v2(&unbound), Err(PalwShardCourtError::BindingDoesNotAuthenticate(PalwStepLegError::CommittedRootMismatch)));

        // A context of the attacker's choosing (another job over the same class), root echoed.
        let mut forged_job = honest.clone();
        forged_job.refutation.binding.job_context.job_id = h64(0xBAD);
        assert!(matches!(v2(&forged_job), Err(PalwShardCourtError::BindingDoesNotAuthenticate(_))), "{:?}", v2(&forged_job));

        // Another execution's authentic binding: not the claim's.
        let other = held_fixture(true, 19, None);
        let mut elsewhere = honest_evidence(&fx, leaf);
        elsewhere.refutation.binding = other.binding.clone();
        assert!(matches!(elsewhere.verdict_at_v2(&claim, LADDER, true), Err(PalwShardCourtError::NotTheClaimsExecution { .. })));

        // A row that does not prove against the class.
        let mut rows = honest.clone();
        rows.artifact_openings = vec![crate::palw_artifact::PalwArtifactOpeningV1 {
            operand: crate::palw_artifact::PalwArtifactOperandV1 {
                tensor_name: "blk.1.attn_softmax".into(),
                layer: Some(1),
                row_start: 0,
                bytes: vec![1],
            },
            leaf_index: 0,
            leaf_count: 2,
            path: vec![h64(3)],
        }];
        assert!(matches!(v2(&rows), Err(PalwShardCourtError::ArtifactOpenings(_))), "{:?}", v2(&rows));

        // A prompt tile of another prompt.
        let mut tile = honest.clone();
        let mut ids = fixture_prompt_ids(20);
        ids[12] ^= 1;
        tile.prompt_ids_opening = Some(crate::palw_prompt_ids_v1::prompt_ids_opening_v1(&ids, 12).expect("a tile"));
        assert!(
            matches!(v2(&tile), Err(PalwShardCourtError::Refutation(PalwStepRefuteError::InputSetNotCanonical(_)))),
            "{:?}",
            v2(&tile)
        );

        // The history rides with the dissection, not the accusation — and the filer's strip is the cure.
        let mut history = honest_evidence(&fx, leaf);
        history.refutation.inputs = vec![crate::palw_step_refute::PalwStepInputRowV1 { preimages: vec![], run_siblings: vec![] }];
        assert!(matches!(
            history.verdict_at_v2(&claim, LADDER, true),
            Err(PalwShardCourtError::FusedLeafCarriesTheHistory { input_rows: 1, anchors: 0, .. })
        ));
        assert_eq!(
            history.clone().for_the_one_move_v2().verdict_at_v2(&claim, LADDER, true),
            Ok(PalwShardCourtVerdictV1::NeedsDissection)
        );

        // The class, unchanged.
        assert!(matches!(
            palw_shard_court_verdict_at_v2(&honest, &PalwOneMoveClaimV2 { class_id: h64(77), ..claim }, LADDER, true),
            Err(PalwShardCourtError::ClassMismatch { .. })
        ));

        // Below the fence every one of these is v1's, byte for byte.
        for a in [&honest, &garbage, &unbound, &forged_job, &rows, &tile] {
            assert_eq!(palw_shard_court_verdict_at_v2(a, &claim, LADDER, false), v1(a));
        }
    }

    /// **A fault the claim committed at a fused site's TILE needs no dissection**: the structural
    /// pass runs at every leaf, and a tile of the wrong width is the executor's on its own root.
    #[test]
    fn a_malformed_tile_committed_at_a_fused_site_convicts_in_one_move() {
        let (fx, leaf) = fused_fixture(Some(1));
        let claim = claim_of(&fx);
        let a = accusation(&fx, honest_evidence(&fx, leaf));
        assert_eq!(palw_shard_court_verdict_at_v2(&a, &claim, LADDER, true), Ok(PalwShardCourtVerdictV1::ExecutorGuilty));
        assert_eq!(palw_shard_court_verdict_at_v2(&a, &claim, LADDER, false), Ok(PalwShardCourtVerdictV1::NeedsDissection));
    }

    /// **The non-fused verdict is v1's.** A cache-write leaf (real tile, real path): its canonical
    /// inputs are not carried, so both read the same refusal; a malformed committed tile convicts in
    /// both; a garbage path is refused in both.
    #[test]
    fn a_non_fused_leaf_reads_the_same_verdict_either_side_of_the_fence() {
        let fx = held_fixture(true, 20, None);
        let claim = claim_of(&fx);
        let (&leaf, preimage) = fx.preimages.iter().next().expect("a cache-write tile");
        assert!(!palw_leaf_is_fused_v2(&fx.binding, leaf));
        let evidence = |fx: &HeldFixture, leaf: u64, preimage: PalwStepTileLeafV1| PalwLeafEvidenceV1 {
            refutation: PalwExecutionStepRefutationV1 {
                binding: fx.binding.clone(),
                output_opening: fx.opening(leaf),
                output_preimage: preimage,
                inputs: vec![],
                prompt_token_ids: vec![],
                decode_tokens: None,
                kv_checkpoint: None,
            },
            artifact_openings: vec![],
            prompt_ids_opening: None,
        };
        let same = |a: &PalwShardCourtAccusationV1, claim: &PalwOneMoveClaimV2| {
            let (one, two) = (
                palw_shard_court_verdict_v1(a, claim.class_id, claim.artifact_root, LADDER),
                palw_shard_court_verdict_at_v2(a, claim, LADDER, true),
            );
            assert_eq!(one.is_ok(), two.is_ok(), "{one:?} / {two:?}");
            if let (Ok(one), Ok(two)) = (&one, &two) {
                assert_eq!(one, two);
            }
            two
        };
        let a = accusation(&fx, evidence(&fx, leaf, preimage.clone()));
        assert!(same(&a, &claim).is_err(), "no inputs carried: refused on both sides");
        let mut garbage = a.clone();
        garbage.refutation.output_opening = PalwStepOpeningV1 { siblings: vec![], ..garbage.refutation.output_opening };
        assert!(same(&garbage, &claim).is_err());
        // A malformed tile the claim committed at a non-fused leaf convicts on both sides.
        let bad = held_fixture(true, 20, None).with_committed_tile(preimage.coord, 1, vec![0; 4]);
        let bad_claim = claim_of(&bad);
        let a = accusation(&bad, evidence(&bad, leaf, bad.preimages[&leaf].clone()));
        assert_eq!(same(&a, &bad_claim), Ok(PalwShardCourtVerdictV1::ExecutorGuilty));
    }
}
