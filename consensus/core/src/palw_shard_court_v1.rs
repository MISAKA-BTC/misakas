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
use crate::palw_shard_plan_v1::PalwShardV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PalwShapeProfileV3, PalwStepOpKindV1, canonical_step_coordinates};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_capped_v1};
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

/// The bytes a ceiling prices: the refutation and the artifact openings, on the wire.
pub fn palw_shard_court_accusation_bytes_v1(a: &PalwShardCourtAccusationV1) -> u64 {
    let refutation = borsh::to_vec(&a.refutation).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    let openings = borsh::to_vec(&a.artifact_openings).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    refutation.saturating_add(openings)
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
    let profile = &accusation.refutation.binding.shape_profile;
    let declared = profile.shape_profile_id();
    if declared != class_id {
        return Err(PalwShardCourtError::ClassMismatch { declared, class: class_id });
    }
    let coord = canonical_step_coordinates(profile, &accusation.refutation.binding.job_context, accusation.leaf_index)
        .ok_or(PalwShardCourtError::LeafHasNoCoordinates { leaf: accusation.leaf_index })?;
    if let Some((node, _)) = profile.resolve_node_slot(coord.node_slot)
        && node.op_kind == PalwStepOpKindV1::AttnFused
    {
        return Ok(PalwShardCourtVerdictV1::NeedsDissection);
    }
    let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, artifact_root)
        .map_err(|e| PalwShardCourtError::ArtifactOpenings(format!("{e:?}")))?;
    match check_execution_step_refutation_capped_v1(&accusation.refutation, &proven, ladder) {
        Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwStepRefuteError::NoFaultFound) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(PalwShardCourtError::Refutation(other)),
    }
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
