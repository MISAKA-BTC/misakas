//! **ADR-0099 Decision 5 — a court opened at a NAMED leaf, decided in one move.**
//!
//! Today every conviction needs a party that re-executed the whole job: the challenger opens a
//! court over the ruleset's whole step space and answers each rung of the ladder from its own
//! execution, until the bisection lands on one leaf and the terminal check runs. A seat that holds
//! one shard has no whole execution to answer rungs from — but it does not need one. ADR-0077
//! Decision 8 already says what it holds: "a row unequal: the seat files nothing and opens a
//! court at that leaf, as any bonded challenger may, holding the refutation's inputs already."
//! Its interval opening IS the refutation's inputs — the committed rows the leaf reads, opened
//! against the claim's step root — and its shard's artifact is the weight the leaf multiplies by.
//! The seat's own capture check (`fp_capture_samples_clear`) builds exactly this object today and
//! throws it away.
//!
//! So the shard court is the terminal check with the search removed: one object names the leaf
//! and carries the refutation, and the chain adjudicates it with the adjudicator it already runs
//! (`check_execution_step_refutation_capped_v1`). A fault convicts the executor of the claim; a
//! refutation that does not prove one convicts the accuser of a false accusation, which is
//! what a bond stakes when it accuses (the `da3_…` rule). There is no responder and no clock,
//! because nothing is asked of the accused: the roots are theirs, the artifact is the class's,
//! and the arithmetic is the court's.
//!
//! **What it cannot try, said first.** A fused attention leaf (graph v5's `AttnFused`) has no
//! single-tile terminal: its adjudication is ADR-0082's k-ary dissection over the history, which
//! needs the responder ADR-0093 specifies and no binary produces. [`palw_shard_court_verdict_v1`]
//! answers `NeedsDissection` for such a leaf rather than pretending.
//!
//! **Not a consensus object yet.** These are the types, the session id, the shape rules and the
//! verdict function; the acceptance rule that would take the object, the fold that would slash on
//! its verdict, and the wire are behind `Params::palw_shard_court`, which is declared, `None` on
//! every preset, and refused at assembly on this build (ADR-0099 Decision 5).

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_shard_plan_v1::PalwShardV1;
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PalwShapeProfileV3, PalwStepOpKindV1, canonical_step_coordinates};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError, check_execution_step_refutation_capped_v1};
use crate::palw_v2::PalwJobContextV2;

pub const PALW_SHARD_COURT_VERSION_V1: u16 = 1;
pub const PALW_SHARD_COURT_DOMAIN_SESSION_V1: &[u8] = b"misaka-palw/shard-court/session/v1";
/// The ML-DSA-87 signing context of an accusation. **Not yet in the bundle's signature-context
/// registry** (`signature_contexts_root`): it enters it in the same ruleset move that arms
/// `Params::palw_shard_court`.
pub const PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT: &[u8] = b"misaka-palw/shard-court/accuse/v1";

/// **The accusation: a leaf, named, with what refutes it.**
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwShardCourtAccusationV1 {
    pub version: u16,
    /// The claim accused, and its committed roots as the accuser read them off the chain.
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    /// The bond that stakes on this accusation — a seat of the claim's panel, or any active bond.
    pub accuser_bond: PalwBondKeyV2,
    /// The plan the accuser holds a shard of, and which shard: the leaf must be one of that
    /// shard's, so an accuser never names a leaf it could not have replayed.
    pub shard_count: u32,
    pub shard_index: u32,
    pub leaf_index: u64,
    /// The refutation — the leaf's committed output tile and its committed inputs, opened
    /// against the claim's roots — and the artifact rows it multiplies by, opened against the
    /// class's registered root.
    pub refutation: PalwExecutionStepRefutationV1,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    /// Over [`palw_shard_court_session_id_v1`], under [`PALW_SHARD_COURT_MLDSA87_ACCUSE_CONTEXT`].
    pub signature: Vec<u8>,
}

/// The session id: every field but the signature and the refutation's bytes — the refutation is
/// bound by its own roots, and an accusation that carried a different refutation for the same
/// leaf is the same accusation (the leaf either recomputes or it does not).
pub fn palw_shard_court_session_id_v1(a: &PalwShardCourtAccusationV1) -> Hash64 {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(PALW_SHARD_COURT_DOMAIN_SESSION_V1).to_state();
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(a.execution_root.as_byte_slice());
    s.update(a.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&a.executor_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser_bond).expect("borsh"));
    s.update(&a.shard_count.to_le_bytes());
    s.update(&a.shard_index.to_le_bytes());
    s.update(&a.leaf_index.to_le_bytes());
    Hash64::from_bytes(s.finalize().as_bytes().try_into().expect("64 bytes"))
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwShardCourtError {
    #[error("accusation version {got}; this build adjudicates version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("a plan of zero shards")]
    NoShards,
    #[error("shard {shard} of a {count}-shard plan")]
    ShardOutOfRange { shard: u32, count: u32 },
    #[error("leaf {leaf} is past the ruleset's ladder of {ladder}")]
    LeafPastTheLadder { leaf: u64, ladder: u64 },
    #[error("the accuser is the accused")]
    AccuserIsTheAccused,
    #[error("the refutation addresses leaf {refuted} and the accusation names leaf {named}")]
    RefutationNamesAnotherLeaf { named: u64, refuted: u64 },
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
        if self.shard_count == 0 {
            return Err(PalwShardCourtError::NoShards);
        }
        if self.shard_index >= self.shard_count {
            return Err(PalwShardCourtError::ShardOutOfRange { shard: self.shard_index, count: self.shard_count });
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
    /// ADR-0093's responder. Neither party is convicted here.
    NeedsDissection,
}

/// **The one move, adjudicated.** Shape first; then the leaf must be one of the accuser's shard's
/// (the coordinates' node slot inside the shard's slot range); then the artifact openings must
/// prove against the class's registered root; then the court's own terminal check runs at the
/// ruleset's ladder. The refutation's binding is checked inside that call against the roots the
/// refutation itself carries — the CALLER compares those to the claim's committed roots, as the
/// court's close path does, because this function holds no chain state.
pub fn palw_shard_court_verdict_v1(
    accusation: &PalwShardCourtAccusationV1,
    profile: &PalwShapeProfileV3,
    context: &PalwJobContextV2,
    shard: &PalwShardV1,
    artifact_root: Hash64,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwShardCourtError> {
    accusation.validate_shape(ladder)?;
    let coord = canonical_step_coordinates(profile, context, accusation.leaf_index)
        .ok_or(PalwShardCourtError::LeafHasNoCoordinates { leaf: accusation.leaf_index })?;
    let end = shard.first_slot + shard.slot_count;
    if coord.node_slot < shard.first_slot || coord.node_slot >= end {
        return Err(PalwShardCourtError::LeafOutsideTheShard {
            leaf: accusation.leaf_index,
            shard: accusation.shard_index,
            slot: coord.node_slot,
            first: shard.first_slot,
            end,
        });
    }
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
