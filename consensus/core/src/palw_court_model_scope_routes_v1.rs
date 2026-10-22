//! **D2's replacement routes (REQ-T6-4; DA16 design §7.5 items 1–2; ADR-0177 D2)** — for every ARMED t12 unit the dormant fence
//! `palw_court_model_scope_v1` closes for a new claim ([`crate::palw_ruling_fences_v1::PALW_D2_RESTRICTED_UNITS_V1`]), the route that
//! convicts the same lie from PUBLIC authenticated material with no model byte compelled from the accused. Unverified source
//! (2026-10-15, round 3): written, not built or run.
//!
//! ```text
//! restricted unit          replacement route (what the accused owes → who supplies the model bytes → the verdict that recomputes)
//! HeldStepLeaf             the leaf's CLAIM PART only (LG14-B `KernelWitness`, or a `StepLeaf` answer past palw_legacy_held_da_v2:
//!                          the committed half, no rows; silence is the DA default) → the ACCUSER's own rows, authenticated against
//!                          the registered artifact root → the one move (`ShardCourtAccused`, `palw_one_move_verdict_bound_v2`) or
//!                          tag 159's leaf recompute; a fused leaf continues as the attention row below
//! AttnRootClaimOperands    a coordinates-only root claim: the accused SIGNS `(session, root)` — the head, lanes, history and
//!                          (m*, S*, V*), claim values only (`palw_attn_root_claim_message_v1` never covered the operands) — and its
//!                          object carries the site's operands RELAYED from the challenger's opening accusation, which must publish
//!                          them (checked at opening: the site derives from them) → the attention dissection, unchanged
//! TirGenRootClaimParams    the same for an IR / pipeline / evaluation dissection: the accused signs `(version, elements, totals)`
//!                          (IR: `palw_tir_root_claim_message_v1` never covered the finalize); the finalize's param leaves are the
//!                          challenger's, published in its cone accusation (authenticated at opening) and relayed → F7, unchanged
//! SeatReadinessLeaves      a keyed DIGEST per drawn leaf, never the bytes → any model holder refutes a wrong digest with its own
//!                          authenticated opening ([`palw_d2_readiness_refute_v1`]) — a seating condition, not a claim's court
//! ```
//!
//! **Why a relay is not compulsion.** The operands a root claim needs are the registered model's bytes at coordinates the site fixes;
//! they authenticate against the registered root whoever carries them. Past the fence the CHALLENGER publishes them (its own copy,
//! `VerifierOwnCopy`) in the object that opens the dissection, so the accused can file its root claim from public chain material
//! alone — its own copy is never read, and the court's cumulative model-byte disclosure from the producer stays zero. A challenger that
//! does not publish them does not open the dissection (refused before any write, [`palw_d2_named_leaf_operands_admit_v1`]); a held
//! leaf challenge (tag 90, digests only) opens none on a new claim — the cone accusations (tags 62 / 88) do.
//!
//! **G14.** Producer and every Panel seat collude: nothing on these routes reads a Panel vote or a seat's material. A public bonded
//! verifier outside the Panel holding the registered model (ADR-0177 D7) demands the claim part (a withheld one is the DA default),
//! files the one move with its own rows, or opens the dissection publishing the site's operands and plays it with its own execution.
//!
//! **Residuals (named, not hidden).** R-D2-COVER: for an IR / pipeline / evaluation finalize the opening authenticates what the
//! challenger published but does not derive the finalize's exact read set, so a challenger that publishes too few leaves leaves the
//! accused to add them (bounded per accusation by one finalize's reads, and the challenger's exposure on a dismissed claim); a
//! composite artifact's and an evaluation close's sections are authenticated by the finalize, not at opening. R-D2-READY: the readiness
//! digests are committed by a seat readiness root, object tag 230 (`PalwConsensusObjectV2::SeatReadinessRootV1`, ADR-0188 B, landed
//! 2026-10-22 under `palw_court_model_scope_v1`), which admission refuses while the draw cap is unadopted; until it is adopted a class
//! registered past the fence takes no readiness proof and seats it by the possession floor's other conditions.

pub mod readiness_root;

use borsh::BorshSerialize;

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwArtifactOperandV1, PalwProvenOperandsV1};
use crate::palw_court_scope_v1::{
    CourtMaterialV1, CourtSupplierV1, CourtUnitKindV1, RegisteredModelRootsV1, VerifierModelOperandV1,
    palw_verify_verifier_model_operand_v1,
};
use crate::palw_shard_court_v1::{
    PalwLeafEvidenceV1, PalwOneMoveClaimV2, PalwShardCourtError, PalwShardCourtVerdictV1, palw_one_move_verdict_bound_v2,
};
use crate::palw_state_v2::{PalwBondKeyV2, PalwClaimStateV2, PalwConsensusObjectV2};

// =================================================================================================================================
// The replacement inventory
// =================================================================================================================================

/// **Every unit a replacement route moves** — what the accused, the challenger or a seat puts before the chain on the routes that
/// replace the four restricted units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwD2ReplacementUnitV1 {
    /// A leaf's committed half — the output tile and its opening, the canonical input rows (none at a fused site), the id carriages —
    /// with NO artifact openings: LG14-B's `KernelWitness { leaf }`, or a `StepLeaf` answer past `palw_legacy_held_da_v2`. Demanded;
    /// a withheld one is the objective DA default.
    HeldLeafClaimPart,
    /// The weight rows a leaf's recompute reads, from the ACCUSER's own copy, authenticated against the registered artifact root
    /// (`ShardCourtAccused.artifact_openings`, tag 159's rows).
    AccuserLeafRows,
    /// What the accused signs in a root claim: the attention `(session, root)`, the IR / pipeline `(version, elements, totals)`, with
    /// the claim-specific openings beside it (binding, committed output tile, anchor, slice sub-roots, cone operands).
    RootClaimCoordinates,
    /// The site's model operands (attention) or the finalize's param leaves (IR / pipeline / evaluation), published by the challenger
    /// in the object that opens the named-leaf dissection.
    ChallengerPublishedOperands,
    /// The same bytes, relayed into the root-claim object from the chain: public before the accused's turn begins.
    RelayedRootOperands,
    /// A seat's keyed digest of one drawn readiness leaf ([`palw_d2_readiness_digest_v1`]) — a hash, never the bytes.
    ReadinessDigest,
    /// A verifier's own opening of a drawn leaf, refuting a wrong digest.
    ReadinessRefutationLeaf,
}

/// What a replacement unit's bytes are and who supplies them (the court scope's own vocabulary).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwD2UnitScopeV1 {
    pub material: CourtMaterialV1,
    pub supplier: CourtSupplierV1,
}

/// **The replacement units' scope.** Every model-bytes unit here is supplied by a verifier's own copy or is already public.
pub const fn palw_d2_replacement_unit_scope_v1(unit: PalwD2ReplacementUnitV1) -> PalwD2UnitScopeV1 {
    use CourtMaterialV1 as M;
    use CourtSupplierV1 as S;
    use PalwD2ReplacementUnitV1 as U;
    let (material, supplier) = match unit {
        U::HeldLeafClaimPart => (M::ClaimTrace, S::Demanded),
        U::AccuserLeafRows => (M::ModelWeights, S::VerifierOwnCopy),
        U::RootClaimCoordinates => (M::ClaimTrace, S::ProducerOpening),
        U::ChallengerPublishedOperands => (M::ModelWeights, S::VerifierOwnCopy),
        U::RelayedRootOperands => (M::ModelWeights, S::Public),
        U::ReadinessDigest => (M::ClaimWitness, S::SeatCondition),
        U::ReadinessRefutationLeaf => (M::ModelFileRange, S::VerifierOwnCopy),
    };
    PalwD2UnitScopeV1 { material, supplier }
}

/// **The D2 rule on a replacement unit**: a unit a party can be compelled to supply is claim-specific (`palw_court_demand_allowed_v1`'s
/// rule, over this inventory).
pub const fn palw_d2_replacement_unit_admissible_v1(unit: PalwD2ReplacementUnitV1) -> bool {
    let scope = palw_d2_replacement_unit_scope_v1(unit);
    !(scope.supplier.is_compelled() && scope.material.is_model_bytes())
}

/// **The verdict a replacement route ends in** — each an existing adjudicator, re-used with challenger-supplied model bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwD2VerdictRouteV1 {
    /// `palw_one_move_verdict_bound_v2` (`ShardCourtAccused`), or LG14-B's tag 159 leaf recompute — the producer's committed half,
    /// the accuser's rows ([`palw_d2_one_move_on_claim_part_v1`]).
    OneMove,
    /// The fused attention dissection (ADR-0082, ADR-0152 §4-ter), opened only with the site's operands published
    /// ([`palw_d2_named_leaf_operands_admit_v1`]); its bottom is the accuser's close.
    AttnDissection,
    /// F7's IR / pipeline / evaluation dissection, opened only with the finalize's params published.
    ConeDissection,
    /// A readiness digest, refuted by a verifier's own opening ([`palw_d2_readiness_refute_v1`]).
    ReadinessDigest,
}

/// **One restricted unit's replacement route.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwD2ReplacementRouteV1 {
    pub restricted: CourtUnitKindV1,
    /// The units the route moves, in order.
    pub units: &'static [PalwD2ReplacementUnitV1],
    pub verdict: PalwD2VerdictRouteV1,
    /// The objects and fold arms that carry it (for the record and the source pins).
    pub carriers: &'static str,
}

/// **The four routes**, one per restricted unit, in [`crate::palw_ruling_fences_v1::PALW_D2_RESTRICTED_UNITS_V1`]'s order.
pub static PALW_D2_REPLACEMENT_ROUTES_V1: [PalwD2ReplacementRouteV1; 4] = {
    use PalwD2ReplacementUnitV1 as U;
    [
        PalwD2ReplacementRouteV1 {
            restricted: CourtUnitKindV1::HeldStepLeaf,
            units: &[U::HeldLeafClaimPart, U::AccuserLeafRows],
            verdict: PalwD2VerdictRouteV1::OneMove,
            carriers: "tag 157/158 KernelWitness (or a StepLeaf answer past palw_legacy_held_da_v2) → ShardCourtAccused / tag 159",
        },
        PalwD2ReplacementRouteV1 {
            restricted: CourtUnitKindV1::AttnRootClaimOperands,
            units: &[U::ChallengerPublishedOperands, U::RootClaimCoordinates, U::RelayedRootOperands, U::AccuserLeafRows],
            verdict: PalwD2VerdictRouteV1::AttnDissection,
            carriers: "ShardCourtAccused (operands published) → CourtAttnRootClaimedHeld (relayed) → rounds → CourtClosed AttnDissection",
        },
        PalwD2ReplacementRouteV1 {
            restricted: CourtUnitKindV1::TirGenRootClaimParams,
            units: &[U::ChallengerPublishedOperands, U::RootClaimCoordinates, U::RelayedRootOperands, U::AccuserLeafRows],
            verdict: PalwD2VerdictRouteV1::ConeDissection,
            carriers: "TirShardCourtAccused / GenShardCourtAccused (params published) → CourtTirRootClaimed / CourtGenRootClaimed \
                       / CourtEvalRootClaimed (relayed) → F7 rounds → the bottom close",
        },
        PalwD2ReplacementRouteV1 {
            restricted: CourtUnitKindV1::SeatReadinessLeaves,
            units: &[U::ReadinessDigest, U::ReadinessRefutationLeaf],
            verdict: PalwD2VerdictRouteV1::ReadinessDigest,
            carriers: "palw_d2_readiness_digest_v1 / palw_d2_readiness_refute_v1 (object tag 230 SeatReadinessRootV1 root+path, ADR-0188 B; refused while the draw cap is unadopted)",
        },
    ]
};

/// The replacement route of `kind`, `None` for a unit D2 does not restrict.
pub fn palw_d2_replacement_route_v1(kind: CourtUnitKindV1) -> Option<&'static PalwD2ReplacementRouteV1> {
    PALW_D2_REPLACEMENT_ROUTES_V1.iter().find(|route| route.restricted == kind)
}

// =================================================================================================================================
// Route 1 — HeldStepLeaf: the claim part, the accuser's rows, the one move
// =================================================================================================================================

/// Why a replacement route refused what it was handed.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwD2RouteErrorV1 {
    #[error("D2: the accused's claim part carries model bytes ({0} artifact openings); the court takes the rows from the accuser")]
    ClaimPartCarriesModelBytes(usize),
    #[error(
        "D2: the named-leaf dissection opens on a new claim only if the opening publishes the root claim's model operands \
         (ShardCourtAccused rows, TirShardCourtAccused / GenShardCourtAccused params)"
    )]
    NoChallengerOperands,
    #[error("D2: the published operands do not authenticate: {0}")]
    Operands(String),
    #[error("D2: the published operands do not derive the site: {0}")]
    Site(String),
    #[error("D2: the one move: {0}")]
    OneMove(PalwShardCourtError),
    #[error("D2: {0}")]
    Relay(&'static str),
}

/// **REQ-T6-4, HeldStepLeaf's route: the one move on the accused's public claim part with the accuser's own rows.** The claim part
/// (`claim_part`) is what the accused served on chain — the committed half, NO artifact openings (a claim part carrying rows is refused:
/// the court never takes model bytes from the accused). `accuser_rows` are the challenger's own copy, authenticated against
/// `claim.artifact_root` inside the verdict. Then the bound one-move verdict, unchanged: `ExecutorGuilty` convicts the producer,
/// `FalseAccusation` charges the accuser, `NeedsDissection` (a fused site) continues as [`PalwD2VerdictRouteV1::AttnDissection`].
pub fn palw_d2_one_move_on_claim_part_v1(
    claim_part: &PalwLeafEvidenceV1,
    accuser_rows: &[PalwArtifactOpeningV1],
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwD2RouteErrorV1> {
    if !claim_part.artifact_openings.is_empty() {
        return Err(PalwD2RouteErrorV1::ClaimPartCarriesModelBytes(claim_part.artifact_openings.len()));
    }
    palw_one_move_verdict_bound_v2(
        &claim_part.refutation,
        accuser_rows,
        claim_part.prompt_ids_opening.as_ref(),
        claim_part.leaf_index(),
        claim,
        ladder,
    )
    .map_err(PalwD2RouteErrorV1::OneMove)
}

// =================================================================================================================================
// Routes 2–3 — the named-leaf dissections: operands published at opening, relayed into the root claim
// =================================================================================================================================

/// **What the object opening a named-leaf dissection publishes for the root claim to relay.**
#[derive(Clone, Copy, Debug)]
pub enum PalwD2OpeningOperandsV1<'a> {
    /// Nothing (a held leaf challenge, tag 90, carries digests only).
    None,
    /// A fused attention leaf's accusation (`ShardCourtAccused`): its binding (bound to the claim by the one-move verdict) and its
    /// artifact openings — which must derive the site the root claim is about.
    Attn { binding: &'a crate::palw_step_leg::PalwStepBindingV2, openings: &'a [PalwArtifactOpeningV1] },
    /// An IR cone accusation's param carriage (`TirShardCourtAccused`'s `TirCone`).
    TirParams(&'a crate::palw_tir_court_v1::PalwTirParamOpeningV1),
    /// A pipeline cone accusation's params (`GenShardCourtAccused`'s `GenCone`), against the pipeline row's artifact root.
    GenParams(&'a [PalwArtifactOpeningV1]),
    /// Carried, authenticated where the finalize reads it (an evaluation close's params; R-D2-COVER).
    CarriedUnchecked,
}

/// **D2 at a named-leaf opening on a NEW claim (the caller asks only where D2 governs the claim)**: the opening must publish the
/// operands the root claim will need, authenticated against the registered roots — for attention, exactly: the site derives from
/// them (`palw_attn_dispute_site_v3` at the leaf, no anchor, the court's opening cap), so the accused can relay them; for an IR /
/// pipeline cone, what is carried authenticates (R-D2-COVER for the exact read set). `proven_root` is the root the fold proves
/// operands against (the claim's recorded root, else the class's); `gen_root` the pipeline row's, where the class is one.
pub fn palw_d2_named_leaf_operands_admit_v1(
    claim: &PalwClaimStateV2,
    leaf: u64,
    operands: &PalwD2OpeningOperandsV1<'_>,
    proven_root: Hash64,
    gen_root: Option<Hash64>,
    opening_cap: u64,
) -> Result<(), PalwD2RouteErrorV1> {
    use crate::palw_tir_court_v1::PalwTirParamOpeningV1 as P;
    match operands {
        PalwD2OpeningOperandsV1::None => Err(PalwD2RouteErrorV1::NoChallengerOperands),
        PalwD2OpeningOperandsV1::Attn { binding, openings } => {
            let proven = PalwProvenOperandsV1::from_openings_v1(openings, proven_root)
                .map_err(|e| PalwD2RouteErrorV1::Operands(e.to_string()))?;
            crate::palw_court_v2::palw_attn_dispute_site_v3(claim, binding, &proven, leaf, None, opening_cap)
                .map(|_| ())
                .map_err(|e| PalwD2RouteErrorV1::Site(e.to_string()))
        }
        PalwD2OpeningOperandsV1::TirParams(params) => match params {
            // The challenger states the finalize reads no inventory leaf: the root claim is coordinates-only as it stands.
            P::None => Ok(()),
            P::Single(proof) => crate::palw_artifact::verify_artifact_multiproof_v1(proof, proven_root)
                .map_err(|e| PalwD2RouteErrorV1::Operands(e.to_string())),
            // A composite artifact's sections authenticate under its sub-roots, which the finalize reads (R-D2-COVER).
            P::Composite(_) => Ok(()),
        },
        PalwD2OpeningOperandsV1::GenParams(openings) => {
            let root = gen_root.ok_or(PalwD2RouteErrorV1::Relay("a pipeline accusation on a claim whose class is no pipeline"))?;
            PalwProvenOperandsV1::from_openings_v1(openings, root).map(|_| ()).map_err(|e| PalwD2RouteErrorV1::Operands(e.to_string()))
        }
        PalwD2OpeningOperandsV1::CarriedUnchecked => Ok(()),
    }
}

/// **The relay, for a filer** (the accused, or any party carrying its signed root claim): an attention root claim object with its
/// operand openings replaced by the challenger's published ones. The accused's signature covers `(session, root)` only
/// (`palw_court_v2::palw_attn_root_claim_message_v1`), so the filled object carries the same signature; the fold authenticates the
/// openings against the registered root exactly as before. Refuses any object that is not an attention root claim.
pub fn palw_d2_attn_root_claim_relayed_v1(
    signed: &PalwConsensusObjectV2,
    published: &[PalwArtifactOpeningV1],
) -> Result<PalwConsensusObjectV2, PalwD2RouteErrorV1> {
    let mut relayed = signed.clone();
    match &mut relayed {
        PalwConsensusObjectV2::CourtAttnRootClaimed { operand_openings, .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedAnchored { operand_openings, .. }
        | PalwConsensusObjectV2::CourtAttnRootClaimedHeld { operand_openings, .. } => {
            *operand_openings = published.to_vec();
            Ok(relayed)
        }
        _ => Err(PalwD2RouteErrorV1::Relay("not an attention root claim")),
    }
}

/// **The relay for an IR root claim**: the finalize's params replaced by the challenger's published ones. The accused signs
/// `(version, elements, totals)` only (`palw_tir_dissect_v1::palw_tir_root_claim_message_v1`), so the signature is unchanged.
pub fn palw_d2_tir_root_claim_relayed_v1(
    signed: &crate::palw_tir_dissect_v1::PalwTirRootClaimV1,
    published: &crate::palw_tir_court_v1::PalwTirParamOpeningV1,
) -> crate::palw_tir_dissect_v1::PalwTirRootClaimV1 {
    let mut relayed = signed.clone();
    relayed.finalize.params = published.clone();
    relayed
}

// =================================================================================================================================
// Route 4 — SeatReadinessLeaves: a keyed digest per drawn leaf, refuted by a verifier's own opening
// =================================================================================================================================

/// **The top-level object tag reserved for the seat readiness digest** (2026-10-15; allocation row in
/// `crate::palw_lifecycle_objects_v2::PALW_A2_TAG_ALLOCATIONS_V1`, A-2 row not landed). Collision-checked against the G14
/// public-conviction and RFC02 branches before reservation.
pub const PALW_D2_SEAT_READINESS_DIGEST_TAG_V1: u8 = 230;

/// The keyed domain of a readiness digest.
pub const PALW_D2_READINESS_DIGEST_DOMAIN_V1: &[u8] = b"MISAKA/PALW/D2/READINESS-DIGEST/V1";

/// **What one readiness digest binds**: the draw's beacon, the class, the seat's bond, the span, the drawn leaf's index and its bytes.
/// The beacon is unknown before the draw, so a seat cannot precompute digests without holding the leaves.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwD2ReadinessDigestInputV1 {
    pub beacon: Hash64,
    pub class_id: Hash64,
    pub seat: PalwBondKeyV2,
    pub span: u64,
    pub leaf_index: u32,
    pub leaf: PalwArtifactOperandV1,
}

/// **A seat's readiness digest of one drawn leaf** — published in place of the leaf (no model byte reaches the chain).
pub fn palw_d2_readiness_digest_v1(input: &PalwD2ReadinessDigestInputV1) -> Hash64 {
    Hash64::from_bytes(misaka_palw_challenge::hash::object_id(PALW_D2_READINESS_DIGEST_DOMAIN_V1, input))
}

/// What a readiness refutation decides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwD2ReadinessVerdictV1 {
    /// The seat's digest is not the registered leaf's: it does not hold what it claimed to (the readiness slash).
    SeatRefuted,
    /// The digest is the registered leaf's: the refutation fails (the refuter's charge).
    DigestHolds,
}

/// **Refute a readiness digest with a verifier's own opening**: the leaf must be one the draw named, the opening must authenticate
/// against the class's REGISTERED artifact root ([`palw_verify_verifier_model_operand_v1`]), and the digest is recomputed over the
/// verifier's bytes. Pure hash arithmetic — any model holder can run it; no seat or producer is asked for anything.
#[allow(clippy::too_many_arguments)]
pub fn palw_d2_readiness_refute_v1(
    roots: &RegisteredModelRootsV1,
    beacon: Hash64,
    class_id: Hash64,
    seat: PalwBondKeyV2,
    span: u64,
    drawn: &[u32],
    claimed_digest: Hash64,
    verifier_opening: &PalwArtifactOpeningV1,
) -> Result<PalwD2ReadinessVerdictV1, &'static str> {
    if !drawn.contains(&verifier_opening.leaf_index) {
        return Err("the refuted leaf is not one the readiness draw named");
    }
    palw_verify_verifier_model_operand_v1(roots, &VerifierModelOperandV1::ArtifactLeaf { opening: verifier_opening })?;
    let honest = palw_d2_readiness_digest_v1(&PalwD2ReadinessDigestInputV1 {
        beacon,
        class_id,
        seat,
        span,
        leaf_index: verifier_opening.leaf_index,
        leaf: verifier_opening.operand.clone(),
    });
    Ok(if honest == claimed_digest { PalwD2ReadinessVerdictV1::DigestHolds } else { PalwD2ReadinessVerdictV1::SeatRefuted })
}

#[cfg(test)]
mod tests;
