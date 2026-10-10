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
use misaka_palw_tir::Prim;
use misaka_palw_tir::program::{Ref, TirProgramV1};

use crate::Hash64;
use crate::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
use crate::palw_public_material_v1::PublicUnitV1;

/// **The per-requester bound**: distinct units of ONE claim that ONE requester operator may demand through the kernel's `FileDemand`
/// (INTERIM, as R-core's DA-8 "16 ever" for a non-seat bond). It bounds each requester's draw on a claim's material; it never bounds
/// another requester's, so no producer, seat or Sybil can exhaust an honest prosecutor's allowance (G14 starvation-freedom). What one
/// prosecution needs is far below it: two positions (K2S §2) plus at most ten root probes under withholding (K2S §11.2), or one entry.
pub const PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1: usize = 16;

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

/// **Record one demanded unit in a claim's per-requester tally** (`[(operator, units)]`, both ascending): a unit the operator already
/// named costs nothing again; a new one is refused once the operator holds [`PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1`].
pub fn palw_court_scope_record_v1(
    requested: &mut Vec<(Hash64, Vec<(u8, u32)>)>,
    operator: Hash64,
    unit: (u8, u32),
) -> Result<(), &'static str> {
    let at = match requested.binary_search_by(|(op, _)| op.cmp(&operator)) {
        Ok(at) => at,
        Err(at) => {
            requested.insert(at, (operator, Vec::new()));
            at
        }
    };
    let units = &mut requested[at].1;
    if let Err(i) = units.binary_search(&unit) {
        if units.len() >= PALW_COURT_SCOPE_MAX_UNITS_PER_REQUESTER_V1 {
            return Err("the requester's operator has named the most units of this claim one operator may (the court scope)");
        }
        units.insert(i, unit);
    }
    Ok(())
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

// ---- node values: which ones are model bytes -----------------------------------------------------------------------------------

/// **What one committed node value is**, by its sources (params below `model_params` are weights; params at or above it are a
/// pipeline stage's lifted inputs; consts are the registered program, public; token / position inputs, states and carries are the
/// claim's).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeMaterialV1 {
    /// From consts alone: anyone computes it from the public program.
    Public,
    /// Model-INDEPENDENT claim material (no weight reaches it).
    Claim,
    /// Model-DEPENDENT claim material (an activation): claim-specific, the relational residual (module doc).
    ModelDependent,
    /// Contains model elements by copy, selected by claim data (`Gather` of an embedding row, a slice / view / cast of a weight).
    ModelCopy,
    /// A function of the model alone (a dequantized or transposed weight): the same at every position — model bytes.
    ModelOnly,
}

impl NodeMaterialV1 {
    /// **Level 1** (ADR-0177 D2's ban on weights and file ranges): never owed in the clear by any court.
    pub const fn is_model_bytes(self) -> bool {
        matches!(self, Self::ModelCopy | Self::ModelOnly)
    }

    /// **Level 2** (also no relational reconstruction): owed only as commitment structure (hashes).
    pub const fn is_model_dependent(self) -> bool {
        matches!(self, Self::ModelDependent | Self::ModelCopy | Self::ModelOnly)
    }

    pub const fn material(self) -> CourtMaterialV1 {
        match self {
            Self::Public | Self::Claim | Self::ModelDependent => CourtMaterialV1::ClaimTrace,
            Self::ModelCopy | Self::ModelOnly => CourtMaterialV1::ModelDerived,
        }
    }
}

/// Dependency flags of a value: on the model, on the claim, and whether some element is a copy of a model element.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Taint {
    model: bool,
    claim: bool,
    copy: bool,
}

impl Taint {
    const PUBLIC: Taint = Taint { model: false, claim: false, copy: false };
    const CLAIM: Taint = Taint { model: false, claim: true, copy: false };
    const WEIGHT: Taint = Taint { model: true, claim: false, copy: true };

    fn join(self, o: Taint) -> Taint {
        Taint { model: self.model || o.model, claim: self.claim || o.claim, copy: self.copy || o.copy }
    }

    /// Copying this value copies model elements: it is (a copy of) model bytes itself.
    fn copies_model(self) -> bool {
        self.copy || (self.model && !self.claim)
    }

    fn material(self) -> NodeMaterialV1 {
        match (self.model, self.claim) {
            (false, false) => NodeMaterialV1::Public,
            (false, true) => NodeMaterialV1::Claim,
            (true, false) => NodeMaterialV1::ModelOnly,
            (true, true) if self.copy => NodeMaterialV1::ModelCopy,
            (true, true) => NodeMaterialV1::ModelDependent,
        }
    }
}

/// The inputs whose ELEMENTS a primitive's output copies (exactly, or by an exact conversion / saturation); `None`: arithmetic.
fn copied_inputs(prim: &Prim) -> Option<&'static [usize]> {
    match prim {
        Prim::Reshape
        | Prim::Transpose { .. }
        | Prim::Slice { .. }
        | Prim::Broadcast
        | Prim::Cast
        | Prim::Clamp { .. }
        | Prim::ReduceMax { .. }
        | Prim::StateWrite { .. }
        | Prim::HistAppend { .. } => Some(&[0]),
        // The data, not the index.
        Prim::Gather { .. } => Some(&[0]),
        // `a` and `b`, not the condition.
        Prim::Select => Some(&[1, 2]),
        Prim::Concat { .. } => Some(&[0, 1, 2, 3, 4, 5, 6, 7]),
        _ => None,
    }
}

/// **Every committed node value's material, per occurrence** — `[occurrence][node]`, aligned with the kernel's `derived_mask_v1`.
/// `model_params`: how many of the program's params are weights (`program.params.len()` for a single program; a pipeline stage's
/// real count for a TIR v2 stage, whose higher params are its lifted inputs). States and carries are solved to a fixpoint over
/// positions (a state written from model bytes is model bytes at the next position).
pub fn palw_court_node_materials_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<NodeMaterialV1>> {
    let occurrences = program.occurrences();
    let mut states = vec![Taint::PUBLIC; program.states.len()];
    loop {
        let mut written = states.clone();
        let mut out = Vec::with_capacity(occurrences.len());
        let mut carry: Vec<Taint> = Vec::new();
        for (b, _) in &occurrences {
            let Some(block) = program.blocks.get(*b as usize) else {
                out.push(Vec::new());
                continue;
            };
            let mut taints: Vec<Taint> = Vec::with_capacity(block.nodes.len());
            for node in &block.nodes {
                let source = |r: &Ref, taints: &[Taint]| -> Taint {
                    match *r {
                        Ref::Node(i) => taints.get(i as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::CarryIn(k) => carry.get(k as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::Param(j) if (j as usize) < model_params => Taint::WEIGHT,
                        Ref::Param(_) => Taint::CLAIM,
                        Ref::Const(_) => Taint::PUBLIC,
                        Ref::State(j) => states.get(j as usize).copied().unwrap_or(Taint::PUBLIC),
                        Ref::Input(_) => Taint::CLAIM,
                    }
                };
                let inputs: Vec<Taint> = node.inputs.iter().map(|r| source(r, &taints)).collect();
                let mut t = inputs.iter().fold(Taint::PUBLIC, |acc, x| acc.join(*x));
                t.copy = match copied_inputs(&node.prim) {
                    Some(data) => data.iter().filter_map(|i| inputs.get(*i)).any(|x| x.copies_model()),
                    None => false,
                };
                if let Prim::StateWrite { state } = node.prim
                    && let Some(w) = written.get_mut(state as usize)
                {
                    *w = w.join(t);
                }
                taints.push(t);
            }
            carry = block.carry_out.iter().map(|i| taints.get(*i as usize).copied().unwrap_or(Taint::PUBLIC)).collect();
            out.push(taints.into_iter().map(Taint::material).collect());
        }
        if written == states {
            return out;
        }
        states = written;
    }
}

/// **Level 1's mask** — `[occurrence][node]`: the values no court owes in the clear (a copy of model elements, or a function of the
/// model alone). K2S ORs it into a position's omitted set beside `derived_mask_v1` (design doc §7.6).
pub fn palw_court_model_bytes_mask_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<bool>> {
    palw_court_node_materials_v1(program, model_params)
        .into_iter()
        .map(|o| o.into_iter().map(|m| m.is_model_bytes()).collect())
        .collect()
}

/// **Level 2's mask**: every model-dependent value (owed as commitment hashes only).
pub fn palw_court_model_dependent_mask_v1(program: &TirProgramV1, model_params: usize) -> Vec<Vec<bool>> {
    palw_court_node_materials_v1(program, model_params)
        .into_iter()
        .map(|o| o.into_iter().map(|m| m.is_model_dependent()).collect())
        .collect()
}

/// **The relational residual of Level 1**, per program: how few clear positions reveal a model relation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelExposureV1 {
    /// Elementwise relations between a model operand and a claim operand (`x ⊙ g`, `x + b`): ONE clear position reveals `g`.
    pub elementwise_relations: usize,
    /// The smallest contracted dimension `K` of a `MatMul` with a model operand: `K` clear positions determine its weight.
    pub min_linear_inner_dim: Option<u64>,
}

/// [`ModelExposureV1`] of a program (shapes at `H = 1`).
pub fn palw_court_model_exposure_v1(program: &TirProgramV1, model_params: usize) -> ModelExposureV1 {
    let materials = palw_court_node_materials_v1(program, model_params);
    let mut out = ModelExposureV1::default();
    for ((b, _), mats) in program.occurrences().iter().zip(&materials) {
        let Some(block) = program.blocks.get(*b as usize) else { continue };
        let shape_of = |r: &Ref| -> Option<Vec<u64>> {
            match *r {
                Ref::Param(j) => program.params.get(j as usize).map(|p| p.shape.iter().map(|d| *d as u64).collect()),
                Ref::Node(i) => block.nodes.get(i as usize).map(|n| n.out.resolve(1).into_iter().map(|d| d as u64).collect()),
                _ => None,
            }
        };
        // Is this operand model bytes (a weight, or a model-only / model-copy value)?
        let is_model = |r: &Ref| match *r {
            Ref::Param(j) => (j as usize) < model_params,
            Ref::Node(i) => mats.get(i as usize).is_some_and(|m| m.is_model_bytes()),
            _ => false,
        };
        for (n, node) in block.nodes.iter().enumerate() {
            if mats.get(n) != Some(&NodeMaterialV1::ModelDependent) {
                continue;
            }
            match node.prim {
                Prim::MatMul => {
                    let (a, bb) = (node.inputs.first(), node.inputs.get(1));
                    let k = match (a, bb) {
                        (Some(_), Some(bb)) if is_model(bb) => shape_of(bb).and_then(|s| s.len().checked_sub(2).map(|i| s[i])),
                        (Some(a), Some(_)) if is_model(a) => shape_of(a).and_then(|s| s.last().copied()),
                        _ => None,
                    };
                    if let Some(k) = k {
                        out.min_linear_inner_dim = Some(out.min_linear_inner_dim.map_or(k, |m| m.min(k)));
                    }
                }
                Prim::Add | Prim::Sub | Prim::Mul | Prim::Div { .. } if node.inputs.iter().any(&is_model) => {
                    out.elementwise_relations += 1;
                }
                _ => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::{DType, Dim};

    /// pre: `e = Gather(E, token)` (an embedding row), `t = Transpose(W)` (a view of a weight), `h = MatMul(e, W)`, `g = Mul(h, G)`,
    /// `p = Cast(pos)`, `k = Add(c, c)` (consts); one layer block reading the carry; post: `logits = MatMul(x, U)`.
    fn toy() -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(16, 8);
        let emb = pb.param("emb", DType::I32, &[16, 4], false);
        let w = pb.param("w", DType::I32, &[4, 4], false);
        let g = pb.param("g", DType::I32, &[1, 4], false);
        let u = pb.param("u", DType::I32, &[4, 16], false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let e = b.gather(emb, Ref::Input(0), 0, 0); // 0: ModelCopy
            let e2 = b.reshape_fixed(e, &[1, 4]); // 1: ModelCopy (a view of the copy)
            let _t = b.transpose(w, &[1, 0]); // 2: ModelOnly
            let h = b.matmul(e2, w, DType::I32); // 3: ModelDependent (an embedding row times a weight — model-only data, but
            //    selected by the token: claim AND model, arithmetic)
            let gh = b.mul(h, g, DType::I32); // 4: ModelDependent
            let _p = b.cast(Ref::Input(1), DType::I32); // 5: Claim
            let c = b.c(DType::I32, 3); // 6: Public (an Iota-free const view)
            let _k = b.add(c, c, DType::I32); // 7: Public
            b.finish(&[gh])
        };
        let layer = {
            let ty = misaka_palw_tir::types::TensorType::new(DType::I32, vec![Dim::Fixed(1), Dim::Fixed(4)]);
            let mut b = pb.block("layer", vec![ty]);
            let x = b.add(Ref::CarryIn(0), Ref::CarryIn(0), DType::I32);
            b.finish(&[x])
        };
        let post = {
            let ty = misaka_palw_tir::types::TensorType::new(DType::I32, vec![Dim::Fixed(1), Dim::Fixed(4)]);
            let mut b = pb.block("post", vec![ty]);
            let l = b.matmul(Ref::CarryIn(0), u, DType::I32);
            b.finish(&[l])
        };
        pb.finish(pre, vec![layer, layer], post, 0)
    }

    #[test]
    fn the_node_materials_separate_weights_copies_and_activations() {
        use NodeMaterialV1 as N;
        let p = toy();
        let m = palw_court_node_materials_v1(&p, p.params.len());
        assert_eq!(m.len(), 4, "pre, two layer occurrences, post");
        // The pre block's committed values, in node order (`b.c` is a const reference, not a node):
        let pre: Vec<N> = m[0].clone();
        assert_eq!(pre[0], N::ModelCopy, "an embedding row selected by the token is model bytes");
        assert_eq!(pre[1], N::ModelCopy, "a view of that copy too");
        assert_eq!(pre[2], N::ModelOnly, "a transposed weight is the model alone");
        assert_eq!(pre[3], N::ModelDependent, "a product of model data is an activation, not a copy");
        assert_eq!(pre[4], N::ModelDependent);
        assert_eq!(pre[5], N::Claim, "the position, cast: model-independent claim material");
        assert!(pre[6..].iter().all(|x| *x == N::Public), "consts are the registered program: public");
        assert_eq!(m[1], vec![N::ModelDependent], "a carry of an activation stays one, across occurrences");
        assert_eq!(m[3], vec![N::ModelDependent], "the logits");
        let mask = palw_court_model_bytes_mask_v1(&p, p.params.len());
        assert_eq!(mask[0][..3], [true, true, true]);
        assert!(!mask[0][3] && !mask[1][0] && !mask[3][0], "Level 1 owes activations in the clear");
        let strong = palw_court_model_dependent_mask_v1(&p, p.params.len());
        assert!(strong[0][3] && strong[1][0] && strong[3][0] && !strong[0][5], "Level 2 owes them as hashes; claim-only stays clear");
        // The same program with every param lifted to a stage input (`model_params = 0`): nothing is model bytes.
        assert!(palw_court_node_materials_v1(&p, 0).iter().flatten().all(|x| matches!(x, N::Claim | N::Public)));
    }

    #[test]
    fn a_state_written_from_model_bytes_is_model_bytes_at_the_next_position() {
        let mut pb = ProgramBuilder::new(16, 8);
        let w = pb.param("w", DType::I32, &[4], false);
        let s = pb.fixed_state("s", DType::I32, &[4], -1_000, 1_000, false);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let r = b.add(Ref::State(s), Ref::State(s), DType::I32); // 0: reads the state
            let _ = b.state_write(s, w); // 1: writes a copy of the weight
            b.finish(&[r])
        };
        let p = pb.finish(pre, vec![], pre, 0);
        let m = palw_court_node_materials_v1(&p, p.params.len());
        assert_eq!(m[0][1], NodeMaterialV1::ModelOnly, "the write is the weight");
        assert_eq!(m[0][0], NodeMaterialV1::ModelOnly, "the next position reads it: the fixpoint carries it");
    }

    #[test]
    fn the_exposure_names_the_one_position_and_the_k_position_relations() {
        let p = toy();
        let e = palw_court_model_exposure_v1(&p, p.params.len());
        // pre's `MatMul(e2, W)` (W is [4, 4]: K = 4), post's `MatMul(x, U)` (U is [4, 16]: K = 4); `Mul(h, G)` is elementwise.
        assert_eq!(e.min_linear_inner_dim, Some(4));
        assert_eq!(e.elementwise_relations, 1);
    }

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
        use misaka_palw_tir::{MapParams, Tensor};
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
