//! **The court scope (ADR-0177 D2, RFC-0014 §16.4): what a court may DEMAND, what a verifier SUPPLIES from its own copy, and the
//! cumulative bound** — the one predicate every route's demand path consults (lane DA16; dormant behind `palw_provider_court_v1`).
//!
//! ```text
//! claim-specific  (a court may demand it; a withheld unit is the correct objective DA default)
//!   ClaimInput    job / prompt ids, stage inputs                       ClaimState    state writes, history rows, checkpoints
//!   ClaimTrace    committed node values that are NOT model bytes      ClaimOutput   committed outputs, logits rows, tokens
//!   ClaimWitness  commitment structure: roots, paths, node-leaf lists, tree-node hashes (hashes of anything, never its bytes)
//! model bytes     (NEVER demanded, at any count; only a verifier's own copy supplies it, authenticated against the REGISTERED root)
//!   ModelWeights  param instances, their rows / columns / tiles       ModelDerived  a node value that is a copy of model elements
//!   ModelFileRange artifact inventory leaves, file ranges, a           (a `Gather` of an embedding row) or a function of the model
//!                 retrieval snapshot's slices, the registered memory    alone (a dequantized weight)
//! ```
//!
//! **Allowed units.** A court's demand names a claim-specific unit or nothing ([`palw_court_demand_allowed_v1`]). A unit that is model
//! bytes reaches a court only as a VERIFIER-supplied operand ([`palw_verify_verifier_model_operand_v1`]): a row / column of a param
//! instance against the kernel class's registered `ParamCommitmentsV1` (the root the class registered, immutable under ADR-0175), or a
//! leaf of the V2 inventory against the class's registered `artifact_root`. The court never asks the producer for it.
//!
//! **Cumulative bound.** Per claim: model bytes disclosed through any court = 0, at any number of demands; every claim-specific unit at
//! most once (served ⇒ public ⇒ a later demand of it is refused); the claim's disclosure ⊆ its own committed material. Per model: model
//! bytes disclosed = 0 over every claim and every demand; claim material grows only with claims producers commit, never with demands.
//! So repeated demands can never return a weight, a file range or a copy of either — the model cannot be rebuilt from court BYTES.
//! **Relational residual (stated, not hidden):** a clear model-DEPENDENT trace value (an activation) is claim-specific, but enough of
//! them reveal a model relation (`y = x ⊙ g` at one position; `y = W x` at `K` positions, `K` the contracted dimension).
//! [`palw_court_model_exposure_v1`] measures it per program; Level 2 of [`NodeMaterialV1`] (owe model-dependent values as their
//! commitment hashes only) removes it at the cost of mid-claim spot checks under withholding — a decision for the user (design doc §7).
//!
//! **G14 reachability** (design doc §7.4): every restricted class keeps every terminal reachable for a verifier that holds the
//! registered model (ADR-0177 D7: G14 is conditional on that): a withheld claim-specific unit is still demandable (default); a wrong
//! model-derived value is recomputed from verifier-supplied operands and convicted against its commitment; an honest one dismisses.
//!
//! Lane K2S (K2 v4/v5) keeps its demand types and calls [`palw_court_node_materials_v1`] / [`palw_court_model_bytes_mask_v1`] where a
//! position's owed set is laid out and classified (design doc §7.6 lists the call sites).

use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_kernel::ledger::{ClaimBodyV1, KernelLedgerV1};
use misaka_palw_kernel::merkle::TensorOpeningV1;
use misaka_palw_kernel::trace::ParamCommitmentsV1;

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use crate::palw_public_material_v1::PublicUnitV1;

/// **The per-requester bound**: distinct units of ONE claim that ONE requester operator may demand through the kernel's `FileDemand`
/// (INTERIM, as R-core's DA-8 "16 ever" for a non-seat bond). It bounds each requester's draw on a claim's material; it never bounds
/// another requester's, so no producer, seat or Sybil can exhaust an honest prosecutor's allowance (G14 starvation-freedom). What one
/// prosecution needs is far below it: two positions (K2S §2) plus at most ten root probes under withholding (K2S §11.2), or one entry.
pub const PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1: usize = misaka_palw_kernel::scope::COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1;

// ---- materials and suppliers -------------------------------------------------------------------------------------------------

/// **What a unit's bytes are** (module doc).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum CourtMaterialV1 {
    ClaimInput = 0,
    ClaimTrace = 1,
    ClaimState = 2,
    ClaimOutput = 3,
    /// Commitment structure: hashes of anything (claim material, a bound tree), never the bytes under them.
    ClaimWitness = 4,
    ModelWeights = 5,
    ModelDerived = 6,
    ModelFileRange = 7,
}

impl CourtMaterialV1 {
    /// Weights, a copy or function of them, or a range of the registered files.
    pub const fn is_model_bytes(self) -> bool {
        matches!(self, Self::ModelWeights | Self::ModelDerived | Self::ModelFileRange)
    }

    pub const fn is_claim_specific(self) -> bool {
        !self.is_model_bytes()
    }
}

/// **Who puts a unit before a court.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CourtSupplierV1 {
    /// A party must supply it when a court demands it (silence is a default).
    Demanded,
    /// The producer's own opening move in a dissection (the dissection cannot proceed without it).
    ProducerOpening,
    /// A seat must publish it to be seated (a condition of a Panel role, not a court demand).
    SeatCondition,
    /// The filer (an accuser, a refuter, a verifier) supplies it from its own copy, authenticated against the chain's root.
    VerifierOwnCopy,
    /// Already public: on chain, or posted by any bond.
    Public,
}

impl CourtSupplierV1 {
    /// A party can be made to supply it (or lose something for not supplying it).
    pub const fn is_compelled(self) -> bool {
        matches!(self, Self::Demanded | Self::ProducerOpening | Self::SeatCondition)
    }
}

/// The route a unit belongs to, and whether that route is ARMED on testnet-12 (`params.rs`, int-12).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CourtRouteV1 {
    /// The kernel route (`palw_probabilistic_constraints_v1`; K2 v3, K2S's v4/v5, R4X's typed roots) — dormant.
    KernelRoute,
    /// Lane D's onboarding (tags 104–109) — dormant.
    Onboarding,
    /// This lane's provider court (tags 150–153) — dormant.
    ProviderCourt,
    /// R-core's `palw_da_court` and the held-context DA court — armed at DAA 0.
    RCoreDa,
    /// The TIR court (`palw_tir_fence2` 3,600) and the TIR shard court (`palw_tir_shard_v1` 5,300) — armed.
    TirCourt,
    /// The shard / attention / IR / generative dissections and the court close — armed.
    Dissection,
    /// Seat readiness proofs (`palw_readiness_v2`) — armed.
    Readiness,
}

impl CourtRouteV1 {
    pub const fn armed_on_testnet_12(self) -> bool {
        matches!(self, Self::RCoreDa | Self::TirCourt | Self::Dissection | Self::Readiness)
    }
}

// ---- the executable inventory --------------------------------------------------------------------------------------------------

/// **Every demand, opening and challenge unit across the routes** (design doc §7.1, the inventory) — one variant per unit kind, so the
/// classification below is the inventory and its test pins it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CourtUnitKindV1 {
    // -- the kernel route (dormant) --
    /// `FileDemand` / `Respond` of a committed stage position (K2 v3; K2S's v4 parts): the node values the position owes.
    KernelPosition,
    /// The stage inputs of a pipeline position (served with it).
    KernelStageInput,
    /// K2S v4 part 0: a position root and its path to the segment root; v4 node-leaf lists.
    KernelPositionRoot,
    /// K2S v4 `PostPromptTile` (inner tag 18): a tile of a job's prompt ids under its prompt root — any bond posts it.
    KernelPromptTile,
    /// A row-tiled opening (v3 `LeafOpeningV3`) of a COMMITTED node value in a filing (read from served or demanded material).
    KernelValueOpening,
    /// A param row / column / tile opening in a `FileProof` or an element court (`InstanceRecompute`, `ElementRecompute`).
    KernelParamOpening,
    /// R4X: a slice of a retrieval class's registered snapshot (stages `0x80 + s`) — the snapshot is registered model content.
    TypedSnapshotSlice,
    /// R4X: a retrieval court's opening of ONE snapshot item (`WrongItem`, `MissedBetter`).
    TypedSnapshotItemOpening,
    /// GAP-52 (this lane): a retrieval claim's retrieved ENTRY (stage `0xC0 + s`) — the item the claim says it retrieved, opened against
    /// the snapshot root: the claim's own output.
    TypedRetrievalEntry,
    /// R4X: a memory claim's pre-state (stage `0x40`) when the line head is the registered `M0` (`pre_source == None`).
    TypedMemoryRegisteredState,
    /// R4X: a memory claim's pre-state carried by an earlier claim (`pre_source == Some`): that claim's public post-state.
    TypedMemoryCarriedState,
    // -- onboarding (dormant) --
    /// Tag 105 `ArtifactMismatchProofV1::Instances`: the bound commitments map (digests).
    BindingCommitments,
    /// Tag 105 `ArtifactMismatchProofV1::Row`: a V2 leaf of the true bytes (the refuter's copy) and a row of the BOUND tensor.
    BindingMismatchRow,
    /// Tag 109 `Post`: the registrant's conformance evidence — digests of outcomes, never the leaves (the chain carries no multiproof).
    ConformancePost,
    /// Tag 109 `Refute::LeafDecode`: a selected artifact leaf, opened by the refuter against the registered artifact root.
    ConformanceLeafDecode,
    /// Tag 109 `Refute::VectorTokens`: a Final kernel claim's tokens.
    ConformanceVectorTokens,
    // -- this lane's provider court (dormant) --
    /// A committed position of a transferred kernel claim (tag 151 `ClaimPosition`, answered by tag 152).
    ProviderClaimPosition,
    /// WITHDRAWN (ADR-0177 D1): an artifact leaf of an `Artifact` lease subject.
    ProviderArtifactLeaf,
    /// WITHDRAWN: the kernel commitments of an `Artifact` lease subject.
    ProviderKernelCommitments,
    /// WITHDRAWN: a run of row-tree node hashes of an `Artifact` lease subject.
    ProviderKernelRowNodes,
    /// WITHDRAWN: a row of a bound kernel tensor of an `Artifact` lease subject.
    ProviderKernelRow,
    // -- R-core / held DA court (ARMED on t12) --
    /// `PalwDaUnitV1::Event`: a logits row / tile against the trace root.
    RCoreEvent,
    /// `PalwHeldMissingV1::PromptIdsTile`.
    HeldPromptIdsTile,
    /// `PalwHeldMissingV1::StateChunk`: a checkpoint state chunk.
    HeldStateChunk,
    /// `PalwHeldMissingV1::StepRange`: ≤ 1,024 step-leaf hashes.
    HeldStepRange,
    /// `PalwHeldMissingV1::StepLeaf`: a step leaf whose answer (`PalwLeafEvidenceV1`) also carries `artifact_openings` — the weight rows
    /// the leaf reads, from the PRODUCER, judged by the one-move verdict.
    HeldStepLeaf,
    /// TIR step leaf / node / row node / step run (≤ 256 leaves): trace material against the step / trace root.
    TirStepMaterial,
    /// Pipeline step leaf / node against the stage root.
    PipelineStepMaterial,
    /// `CourtAttnRootClaimedHeld.operand_openings`: the attention site's registered quantization rows, in the producer's root claim.
    AttnRootClaimOperands,
    /// `CourtTirRootClaimed` / `CourtGenRootClaimed` and the finalize `params`: the parameter leaves the disputed tile reads, in the
    /// producer's root claim.
    TirGenRootClaimParams,
    /// Accuser-supplied openings: `ShardCourtAccused.artifact_openings`, `TirShardCourtAccused` params, the held leaf challenge's close,
    /// `ObjectiveOffence` operands, the court close's `Arithmetic` / `AttnDissection` proofs.
    AccuserArtifactOpenings,
    /// `SeatReadinessProvedV2` / `TirSeatReadinessProved`: ~16 plaintext artifact leaves a seat publishes per (class, bond, span).
    SeatReadinessLeaves,
}

/// What [`palw_court_unit_scope_v1`] says of a unit kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CourtUnitScopeV1 {
    pub material: CourtMaterialV1,
    pub supplier: CourtSupplierV1,
    pub route: CourtRouteV1,
}

/// **The inventory** (design doc §7.1): each unit kind's material, who supplies it, and its route.
pub const fn palw_court_unit_scope_v1(kind: CourtUnitKindV1) -> CourtUnitScopeV1 {
    use CourtMaterialV1 as M;
    use CourtRouteV1 as R;
    use CourtSupplierV1 as S;
    use CourtUnitKindV1 as K;
    let (material, supplier, route) = match kind {
        K::KernelPosition => (M::ClaimTrace, S::Demanded, R::KernelRoute),
        K::KernelStageInput => (M::ClaimInput, S::Demanded, R::KernelRoute),
        K::KernelPositionRoot => (M::ClaimWitness, S::Demanded, R::KernelRoute),
        K::KernelPromptTile => (M::ClaimInput, S::Public, R::KernelRoute),
        K::KernelValueOpening => (M::ClaimTrace, S::VerifierOwnCopy, R::KernelRoute),
        K::KernelParamOpening => (M::ModelWeights, S::VerifierOwnCopy, R::KernelRoute),
        K::TypedSnapshotSlice => (M::ModelFileRange, S::Demanded, R::KernelRoute),
        K::TypedSnapshotItemOpening => (M::ModelFileRange, S::VerifierOwnCopy, R::KernelRoute),
        K::TypedRetrievalEntry => (M::ClaimOutput, S::Demanded, R::KernelRoute),
        K::TypedMemoryRegisteredState => (M::ModelFileRange, S::Demanded, R::KernelRoute),
        K::TypedMemoryCarriedState => (M::ClaimState, S::Demanded, R::KernelRoute),
        K::BindingCommitments => (M::ClaimWitness, S::VerifierOwnCopy, R::Onboarding),
        K::BindingMismatchRow => (M::ModelWeights, S::VerifierOwnCopy, R::Onboarding),
        K::ConformancePost => (M::ClaimWitness, S::Demanded, R::Onboarding),
        K::ConformanceLeafDecode => (M::ModelFileRange, S::VerifierOwnCopy, R::Onboarding),
        K::ConformanceVectorTokens => (M::ClaimOutput, S::Public, R::Onboarding),
        K::ProviderClaimPosition => (M::ClaimTrace, S::Demanded, R::ProviderCourt),
        K::ProviderArtifactLeaf => (M::ModelFileRange, S::Demanded, R::ProviderCourt),
        K::ProviderKernelCommitments => (M::ClaimWitness, S::Demanded, R::ProviderCourt),
        K::ProviderKernelRowNodes => (M::ClaimWitness, S::Demanded, R::ProviderCourt),
        K::ProviderKernelRow => (M::ModelWeights, S::Demanded, R::ProviderCourt),
        K::RCoreEvent => (M::ClaimOutput, S::Demanded, R::RCoreDa),
        K::HeldPromptIdsTile => (M::ClaimInput, S::Demanded, R::RCoreDa),
        K::HeldStateChunk => (M::ClaimState, S::Demanded, R::RCoreDa),
        K::HeldStepRange => (M::ClaimWitness, S::Demanded, R::RCoreDa),
        // The strictest component: the answer carries the producer's weight rows beside the claim's step openings.
        K::HeldStepLeaf => (M::ModelWeights, S::Demanded, R::RCoreDa),
        K::TirStepMaterial => (M::ClaimTrace, S::Demanded, R::TirCourt),
        K::PipelineStepMaterial => (M::ClaimTrace, S::Demanded, R::TirCourt),
        K::AttnRootClaimOperands => (M::ModelWeights, S::ProducerOpening, R::Dissection),
        K::TirGenRootClaimParams => (M::ModelWeights, S::ProducerOpening, R::Dissection),
        K::AccuserArtifactOpenings => (M::ModelWeights, S::VerifierOwnCopy, R::Dissection),
        K::SeatReadinessLeaves => (M::ModelFileRange, S::SeatCondition, R::Readiness),
    };
    CourtUnitScopeV1 { material, supplier, route }
}

/// Why a court may not compel a unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CourtScopeRefusalV1 {
    /// The unit is model bytes: only a verifier's own copy supplies those (ADR-0177 D2).
    ModelBytes(CourtMaterialV1),
}

impl CourtScopeRefusalV1 {
    pub const fn why(self) -> &'static str {
        match self {
            Self::ModelBytes(_) => {
                "the unit is model bytes (weights, a copy of them or a registered file range): no court compels them — a verifier supplies them from its own copy, authenticated against the registered root (ADR-0177 D2)"
            }
        }
    }
}

/// **The rule**: a unit a party can be compelled to supply is claim-specific. Model bytes reach a court only from a verifier's own copy.
pub const fn palw_court_demand_allowed_v1(kind: CourtUnitKindV1) -> Result<(), CourtScopeRefusalV1> {
    let scope = palw_court_unit_scope_v1(kind);
    if scope.supplier.is_compelled() && scope.material.is_model_bytes() {
        Err(CourtScopeRefusalV1::ModelBytes(scope.material))
    } else {
        Ok(())
    }
}

// ---- the units this lane and the kernel route name -----------------------------------------------------------------------------

/// A provider-court (tag 151) unit's kind: a claim position, or one of the withdrawn artifact units.
pub const fn palw_public_unit_kind_v1(unit: &PublicUnitV1) -> CourtUnitKindV1 {
    match unit {
        PublicUnitV1::ArtifactLeaf { .. } => CourtUnitKindV1::ProviderArtifactLeaf,
        PublicUnitV1::KernelCommitments => CourtUnitKindV1::ProviderKernelCommitments,
        PublicUnitV1::KernelRowNodes { .. } => CourtUnitKindV1::ProviderKernelRowNodes,
        PublicUnitV1::KernelRow { .. } => CourtUnitKindV1::ProviderKernelRow,
        PublicUnitV1::ClaimPosition { .. } => CourtUnitKindV1::ProviderClaimPosition,
    }
}

/// **What a kernel `FileDemand { claim, stage, position }` names**, from the ledger's rows (`None`: no unit the ledger would accept a
/// demand of — the kernel refuses it on its own). The same precedence as the kernel's: a committed position first, then a typed
/// snapshot slice.
pub fn palw_kernel_demand_unit_v1(ledger: &KernelLedgerV1, claim: &[u8; 64], stage: u8, position: u32) -> Option<CourtUnitKindV1> {
    use misaka_palw_kernel::spec::composite::ComponentV1;
    use misaka_palw_kernel::spec::{
        MEMORY_PRE_STATE_STAGE_V1, RETRIEVAL_ENTRY_STAGE_BASE_V1, SNAPSHOT_STAGE_BASE_V1, SpecClaimV1, SpecClassKindV1,
    };
    let row = ledger.claims.get(claim)?;
    if let ClaimBodyV1::Spec(body) = &row.body {
        let kind = ledger.typed.classes.get(&row.class_binding_id).map(|c| &c.kind);
        if stage == MEMORY_PRE_STATE_STAGE_V1 && matches!(kind, Some(SpecClassKindV1::Memory { .. })) {
            body.position(stage, position)?;
            // `None`: the line head is the registered M0 — the class's own memory, part of the registered model.
            return Some(match body.pre_source {
                None => CourtUnitKindV1::TypedMemoryRegisteredState,
                Some(_) => CourtUnitKindV1::TypedMemoryCarriedState,
            });
        }
        if body.position(stage, position).is_some() {
            return Some(CourtUnitKindV1::KernelPosition);
        }
        if let Some(s) = stage.checked_sub(RETRIEVAL_ENTRY_STAGE_BASE_V1) {
            use misaka_palw_kernel::spec::composite::StageClaimV1;
            let entries = match (kind?, &body.claim) {
                (SpecClassKindV1::Retrieval { .. }, SpecClaimV1::Retrieval(c)) if s == 0 => c.result.len(),
                (SpecClassKindV1::Composite { components, .. }, SpecClaimV1::Composite(c)) => {
                    match (components.get(s as usize)?, c.stages.get(s as usize)?) {
                        (ComponentV1::Retrieval(_), StageClaimV1::Retrieval { result, .. }) => result.len(),
                        _ => return None,
                    }
                }
                _ => return None,
            };
            return ((position as usize) < entries).then_some(CourtUnitKindV1::TypedRetrievalEntry);
        }
        let slice = stage.checked_sub(SNAPSHOT_STAGE_BASE_V1).and_then(|s| match kind? {
            SpecClassKindV1::Retrieval { root } if s == 0 => Some(root),
            SpecClassKindV1::Composite { components, .. } => match components.get(s as usize)? {
                ComponentV1::Retrieval(r) => Some(r),
                ComponentV1::Model(_) => None,
            },
            _ => None,
        });
        return slice.filter(|r| (position as u64) < r.snapshot.slices()).map(|_| CourtUnitKindV1::TypedSnapshotSlice);
    }
    row.body.position(stage, position).map(|_| CourtUnitKindV1::KernelPosition)
}

// ---- the verifier's own model operand ------------------------------------------------------------------------------------------

/// **The roots a model operand is authenticated against** — the class's REGISTERED roots (immutable under ADR-0175): the kernel class's
/// registered param root (`ParamCommitmentsV1::root`, the root its registration carried) and / or the V2 class's `artifact_root`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RegisteredModelRootsV1 {
    pub kernel_param_root: Option<Hash64>,
    pub artifact_root: Option<Hash64>,
    /// The V2 inventory's leaf count (the program's closed form), when `artifact_root` is given.
    pub artifact_leaf_count: Option<u32>,
}

/// **A model operand a verifier supplies from its own copy** — never demanded from the producer.
#[derive(Clone, Copy, Debug)]
pub enum VerifierModelOperandV1<'a> {
    /// A row or a column of param instance `(param, layer)` under the registered kernel commitments (`commitments` are the registered
    /// map itself: on chain in the kernel class's registration, so the verifier copies them, it does not need the producer).
    KernelTensor { commitments: &'a ParamCommitmentsV1, param: u16, layer: Option<u16>, opening: &'a TensorOpeningV1 },
    /// A leaf of the registered V2 artifact inventory.
    ArtifactLeaf { opening: &'a PalwArtifactOpeningV1 },
}

/// **Authenticate a verifier-supplied model operand against the registered root** — hash arithmetic only; the bytes are the
/// verifier's, the root is the chain's. `Ok` is "these are the registered model's bytes at these coordinates", nothing more (root
/// equality is not proof of the producer's computation: the court still evaluates the relation).
pub fn palw_verify_verifier_model_operand_v1(
    roots: &RegisteredModelRootsV1,
    operand: &VerifierModelOperandV1<'_>,
) -> Result<(), &'static str> {
    match operand {
        VerifierModelOperandV1::KernelTensor { commitments, param, layer, opening } => {
            let root = roots.kernel_param_root.ok_or("the class registered no kernel param root")?;
            if commitments.root() != root.as_bytes() {
                return Err("the commitments are not the registered ones");
            }
            let c = commitments.by_instance.get(&(*param, *layer)).ok_or("the registered commitments hold no such instance")?;
            opening.authenticates(c).then_some(()).ok_or("the opening does not authenticate against the registered instance")
        }
        VerifierModelOperandV1::ArtifactLeaf { opening } => {
            let root = roots.artifact_root.ok_or("the class registered no artifact root")?;
            if roots.artifact_leaf_count.is_some_and(|n| n != opening.leaf_count) {
                return Err("the opening is of another inventory size");
            }
            verify_artifact_opening_v1(opening, root).map_err(|_| "the leaf does not reach the registered artifact root")
        }
    }
}

// ---- node values, the clear budget, the tally: the kernel's predicate (re-exported) --------------------------------------------

pub use misaka_palw_kernel::scope::{
    COURT_HIDING_LEAF_ELEMENTS_V1, DisclosureV1, ModelExposureV1, NodeMaterialV1,
    court_clear_position_budget_v1 as palw_court_clear_position_budget_v1,
    court_model_bytes_mask_v1 as palw_court_model_bytes_mask_v1, court_model_dependent_mask_v1 as palw_court_model_dependent_mask_v1,
    court_model_exposure_v1 as palw_court_model_exposure_v1, court_node_materials_v1 as palw_court_node_materials_v1,
    court_position_disclosure_v1 as palw_court_position_disclosure_v1, court_withheld_mask_v1 as palw_court_withheld_mask_v1,
};

/// The material class of a committed node value (model bytes are `ModelDerived`).
pub const fn palw_node_material_class_v1(m: NodeMaterialV1) -> CourtMaterialV1 {
    if m.is_model_bytes() { CourtMaterialV1::ModelDerived } else { CourtMaterialV1::ClaimTrace }
}

/// **Record one demanded unit in a claim's per-requester tally**, keyed by the requester's OPERATOR (the kernel's predicate).
pub fn palw_court_scope_record_v1(
    requested: &mut Vec<(Hash64, Vec<(u8, u32)>)>,
    operator: Hash64,
    unit: (u8, u32),
) -> Result<(), &'static str> {
    let mut raw: Vec<([u8; 64], Vec<(u8, u32)>)> = requested.iter().map(|(h, u)| (h.as_bytes(), u.clone())).collect();
    misaka_palw_kernel::scope::court_scope_record_v1(&mut raw, operator.as_bytes(), unit)?;
    *requested = raw.into_iter().map(|(h, u)| (Hash64::from_bytes(h), u)).collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_inventory_compels_no_model_bytes_on_the_dormant_routes_and_names_the_armed_ones() {
        use CourtUnitKindV1 as K;
        let all = [
            K::KernelPosition,
            K::KernelStageInput,
            K::KernelPositionRoot,
            K::KernelPromptTile,
            K::KernelValueOpening,
            K::KernelParamOpening,
            K::TypedSnapshotSlice,
            K::TypedSnapshotItemOpening,
            K::TypedRetrievalEntry,
            K::TypedMemoryRegisteredState,
            K::TypedMemoryCarriedState,
            K::BindingCommitments,
            K::BindingMismatchRow,
            K::ConformancePost,
            K::ConformanceLeafDecode,
            K::ConformanceVectorTokens,
            K::ProviderClaimPosition,
            K::ProviderArtifactLeaf,
            K::ProviderKernelCommitments,
            K::ProviderKernelRowNodes,
            K::ProviderKernelRow,
            K::RCoreEvent,
            K::HeldPromptIdsTile,
            K::HeldStateChunk,
            K::HeldStepRange,
            K::HeldStepLeaf,
            K::TirStepMaterial,
            K::PipelineStepMaterial,
            K::AttnRootClaimOperands,
            K::TirGenRootClaimParams,
            K::AccuserArtifactOpenings,
            K::SeatReadinessLeaves,
        ];
        let refused: Vec<K> = all.iter().copied().filter(|k| palw_court_demand_allowed_v1(*k).is_err()).collect();
        // The dormant routes: what this rule refuses (DA16 withdrew the artifact subject; the fold refuses the typed ones).
        let dormant: Vec<K> = refused.iter().copied().filter(|k| !palw_court_unit_scope_v1(*k).route.armed_on_testnet_12()).collect();
        assert_eq!(
            dormant,
            vec![K::TypedSnapshotSlice, K::TypedMemoryRegisteredState, K::ProviderArtifactLeaf, K::ProviderKernelRow],
            "the dormant units that compelled model bytes"
        );
        // The ARMED t12 routes that compel model bytes: left as they are (a new dormant fence restricts them, design doc §7.5).
        let armed: Vec<K> = refused.iter().copied().filter(|k| palw_court_unit_scope_v1(*k).route.armed_on_testnet_12()).collect();
        assert_eq!(armed, vec![K::HeldStepLeaf, K::AttnRootClaimOperands, K::TirGenRootClaimParams, K::SeatReadinessLeaves]);
        // Every model-bytes unit that remains is the verifier's own copy.
        for k in all {
            let s = palw_court_unit_scope_v1(k);
            if s.material.is_model_bytes() && palw_court_demand_allowed_v1(k).is_ok() {
                assert_eq!(s.supplier, CourtSupplierV1::VerifierOwnCopy, "{k:?}");
            }
        }
        assert_eq!(palw_public_unit_kind_v1(&PublicUnitV1::ClaimPosition { stage: 0, position: 1 }), K::ProviderClaimPosition);
        assert!(palw_court_demand_allowed_v1(palw_public_unit_kind_v1(&PublicUnitV1::ArtifactLeaf { index: 0 })).is_err());
    }

    #[test]
    fn the_per_requester_tally_bounds_each_operator_and_never_another() {
        let (a, b) = (Hash64::from_bytes([1; 64]), Hash64::from_bytes([2; 64]));
        let mut t = Vec::new();
        for p in 0..PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 as u32 {
            palw_court_scope_record_v1(&mut t, b, (0, p)).unwrap();
        }
        palw_court_scope_record_v1(&mut t, b, (0, 3)).expect("a unit already named costs nothing again");
        assert!(palw_court_scope_record_v1(&mut t, b, (0, 999)).is_err(), "the operator's allowance is spent");
        // Another operator — an honest prosecutor — is untouched by what `b` spent (G14 starvation-freedom).
        palw_court_scope_record_v1(&mut t, a, (0, 999)).unwrap();
        assert_eq!(t.iter().map(|(op, u)| (*op, u.len())).collect::<Vec<_>>(), vec![(a, 1), (b, 16)], "ascending, bounded");
    }

    #[test]
    fn a_verifier_operand_authenticates_only_against_the_registered_root() {
        use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaves_v1};
        use misaka_palw_tir::{DType, MapParams, Tensor};
        let mut params = MapParams::default();
        params.tensors.insert((0, None), Tensor::new(DType::I32, vec![3, 4], (0..12).collect()).unwrap());
        params.tensors.insert((1, Some(0)), Tensor::new(DType::I32, vec![2, 2], vec![5, 6, 7, 8]).unwrap());
        let commitments = ParamCommitmentsV1::of(&params);
        let roots = RegisteredModelRootsV1 { kernel_param_root: Some(Hash64::from_bytes(commitments.root())), ..Default::default() };
        let opening = TensorOpeningV1::row(&params.tensors[&(0, None)], 1).unwrap();
        let op = VerifierModelOperandV1::KernelTensor { commitments: &commitments, param: 0, layer: None, opening: &opening };
        palw_verify_verifier_model_operand_v1(&roots, &op).unwrap();
        // Another instance's commitment, a forged row, another root, no root at all: refused.
        let other = VerifierModelOperandV1::KernelTensor { commitments: &commitments, param: 1, layer: Some(0), opening: &opening };
        assert!(palw_verify_verifier_model_operand_v1(&roots, &other).is_err());
        let mut forged = params.clone();
        forged.tensors.get_mut(&(0, None)).unwrap().data[5] += 1;
        let bad = TensorOpeningV1::row(&forged.tensors[&(0, None)], 1).unwrap();
        let op = VerifierModelOperandV1::KernelTensor { commitments: &commitments, param: 0, layer: None, opening: &bad };
        assert!(palw_verify_verifier_model_operand_v1(&roots, &op).is_err());
        let wrong = ParamCommitmentsV1::of(&forged);
        let op = VerifierModelOperandV1::KernelTensor { commitments: &wrong, param: 0, layer: None, opening: &bad };
        assert!(palw_verify_verifier_model_operand_v1(&roots, &op).is_err(), "self-consistent, but not the registered commitments");
        assert!(palw_verify_verifier_model_operand_v1(&RegisteredModelRootsV1::default(), &op).is_err());

        let leaves: Vec<PalwArtifactOperandV1> = (0..3u8)
            .map(|i| PalwArtifactOperandV1 { tensor_name: format!("t{i}"), layer: None, row_start: 0, bytes: vec![i; 8] })
            .collect();
        let root = artifact_root_v1(&leaves.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
        let roots = RegisteredModelRootsV1 { artifact_root: Some(root), artifact_leaf_count: Some(3), ..Default::default() };
        let mut opened = open_artifact_leaves_v1(&leaves, &[2]).unwrap();
        palw_verify_verifier_model_operand_v1(&roots, &VerifierModelOperandV1::ArtifactLeaf { opening: &opened[0] }).unwrap();
        opened[0].operand.bytes[0] ^= 1;
        assert!(palw_verify_verifier_model_operand_v1(&roots, &VerifierModelOperandV1::ArtifactLeaf { opening: &opened[0] }).is_err());
    }
}
