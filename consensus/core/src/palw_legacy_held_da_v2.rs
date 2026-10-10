//! **Lane LG14-B (RFC-0014 §4–§5; user decision GAP-80, 2026-10-10): the legacy V2 Panel route's public descent and its held/fused
//! DA units** — behind the dormant fence `Params::palw_legacy_held_da_v2` (object tags 157–159). Design:
//! `docs/design/palw/legacy-route-g14-held-da.md`.
//!
//! **What is missing on the V2 route, and what this adds.** Every terminal a lie on the V2 route needs already exists and is armed on
//! testnet-12 (the one-move court, `ExecutorRefuted`'s contradictions, `CheckpointAccused`, the held dissection). What a public
//! bonded verifier OUTSIDE the Panel lacks is the material: every builder of those terminals reads the producer's served capture
//! (seats only), and on a held class even a seat cannot — the three canonical 8k gaps [C12]: (a) a canonical capture is past the
//! whole-capture cap, (b) a lying fold's flat prefix rung is unreadable, (c) no unit discloses a committed fused tile. So:
//!
//! * **Descent units** (tag 157 demands, tag 158 answers): `StepNode { level, index }` / `CheckpointNode { level, index }` — an
//!   interior node of the claim's committed step / checkpoint tree, answered by its frontier [`PALW_LEGACY_HELD_NODE_DEPTH_V2`] levels
//!   down and its siblings to the root (hash arithmetic, [`palw_legacy_held_check_answer_v2`]). The tree is the claim's OWN
//!   commitment — no second execution root. A verifier compares each authenticated frontier with its OWN honest tree and names the
//!   first node that differs ([`palw_legacy_descent_next_v2`]): three rounds reach the first divergent leaf of a 2^27-leaf job.
//! * **`KernelWitness { leaf }`** (CKW, tags 157/158): the committed half of one leaf's refutation — the output tile and its
//!   opening, the canonical input rows (none at a fused site), the id carriages — checked by MEMBERSHIP alone
//!   ([`crate::palw_step_refute::check_execution_step_committed_half_v1`]). Retrieval, never adjudication: a correct answer passes
//!   to the exact court (`ShardCourtAccused` → the held dissection at a fused leaf), no answer is the objective DA default (DA-7),
//!   and bytes that do not authenticate are not an answer. Never at a leaf whose output copies the model (the `EmbedLookup`
//!   gather), and never with artifact openings: model bytes are never compelled (ADR-0177 D2).
//! * **Tag 159, the leaf recompute** ([`palw_legacy_leaf_recompute_verdict_v2`]): a non-fused step convicted from its committed
//!   leaf HASH, its committed inputs and the accuser's OWN model rows — the one-move court's kernel, without the output preimage
//!   ([`crate::palw_step_refute::check_execution_step_leaf_hash_v1`]). The verifier builds every opening itself from its own tree
//!   and the revealed frontiers ([`PalwLegacyNodeViewV2`]): nothing more is asked of the producer.
//!
//! The descent and CKW units enter R-core+'s DA court as `PalwDaUnitV1::LegacyHeldV2`: every gate, budget (DA-8), clock, exposure
//! (DA-6), default (DA-7) and record of that court applies unchanged; a covering `Valid` signer is not charged S4 for them (the
//! seats' automatic answering does not build them).
//!
//! **A-2.** Below the fence an object of tags 157–159 — and any int-12 object carrying the appended unit — is dropped by name in the
//! acceptance walk, charged nothing, exactly as the live int-12 build skips bytes it cannot decode; the fold refuses it as the second
//! lock ([`crate::palw_state_v2::palw_object_is_legacy_held_da_v2`]).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::Hash64;
use crate::config::params::{ForkActivation, Params};
use crate::palw_artifact::{PalwArtifactOpeningV1, PalwProvenOperandsV1};
use crate::palw_mode_v2::PalwModeV2Error;
use crate::palw_prompt_ids_v1::PalwPromptIdsOpeningV1;
use crate::palw_shard_court_v1::{PalwOneMoveClaimV2, PalwShardCourtVerdictV1};
use crate::palw_state_v2::PalwBondKeyV2;
use crate::palw_step::{PalwStepCoordinateV1, PalwStepOpKindV1, canonical_step_coordinates};
use crate::palw_step_leg::{PalwStepBindingV2, PalwStepTileLeafV1, step_merkle_leaf_v1, step_merkle_node_v1, verify_binding_v1};
use crate::palw_step_refute::{PalwExecutionStepRefutationV1, PalwStepRefuteError};
use crate::palw_tir_court_v1::{
    palw_step_node_frontier_at_depth_v1, palw_step_node_reaches_at_depth_v1, palw_tir_step_tree_height_v1, palw_tir_step_tree_width_v1,
};

// ---- allocations ----------------------------------------------------------------------------------------------------------------

/// The object tags (the Lead's allocation, registry §2: LG14-B 157–159).
pub const PALW_LEGACY_HELD_DEMAND_TAG_V2: u8 = 157;
pub const PALW_LEGACY_HELD_ANSWER_TAG_V2: u8 = 158;
pub const PALW_LEGACY_LEAF_RECOMPUTE_TAG_V2: u8 = 159;

/// The wire version of every payload here.
pub const PALW_LEGACY_HELD_VERSION_V2: u16 = 2;

/// **Levels one node answer covers**: its frontier is the nodes this many levels below the demanded node (the leaf nodes, when
/// nearer) — at most 2^9 hashes, 32 KiB, so the answer rides under the ruleset's 80 KiB close ceiling with the claim's binding (whose
/// shape profile is carried in full) and its siblings. Three rounds descend 27 levels: the canonical 8k row's ≈105.5M leaves (height
/// 27) reach the first divergent leaf in three node sessions; four reach 2^36.
pub const PALW_LEGACY_HELD_NODE_DEPTH_V2: u8 = 9;

/// The accuser's ML-DSA-87 context over a demand (tag 157). Not in a live network's committed set: the Some-only fence covers it.
pub const PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/demand/mldsa87/v1";
/// The discloser's ML-DSA-87 context over an answer (tag 158).
pub const PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/answer/mldsa87/v1";
/// The accuser's ML-DSA-87 context over a leaf recompute (tag 159).
pub const PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/recompute/mldsa87/v1";
const PALW_LEGACY_HELD_DEMAND_DOMAIN_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/demand-message";
const PALW_LEGACY_HELD_ANSWER_DOMAIN_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/answer-message";
const PALW_LEGACY_LEAF_RECOMPUTE_DOMAIN_V2: &[u8] = b"misaka-palw/legacy-held-da/v2/recompute-message";

/// Every keyed domain and context this family uses (the cross-family uniqueness sweep).
pub const PALW_LEGACY_HELD_ALL_DOMAINS_V2: &[&[u8]] = &[
    PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2,
    PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2,
    PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2,
    PALW_LEGACY_HELD_DEMAND_DOMAIN_V2,
    PALW_LEGACY_HELD_ANSWER_DOMAIN_V2,
    PALW_LEGACY_LEAF_RECOMPUTE_DOMAIN_V2,
];

// ---- the fence ------------------------------------------------------------------------------------------------------------------

impl Params {
    /// Whether the legacy held DA units and the leaf recompute are in force at `daa_score` — never, in this binary.
    pub fn palw_legacy_held_da_v2_active_at(&self, daa_score: u64) -> bool {
        self.palw_legacy_held_da_v2.is_some_and(|f| f != ForkActivation::never() && f.is_active(daa_score))
    }

    /// **The fence's refusal**: the route it opens is one half of RFC-0014 on the V2 route (LG14-A's filer, reservation and Final
    /// hold are the other), economics, review and activation are not in place, so any armed height is refused.
    pub fn validate_palw_legacy_held_da_v2(&self) -> Result<(), PalwModeV2Error> {
        match self.palw_legacy_held_da_v2 {
            Some(f) if f != ForkActivation::never() => Err(PalwModeV2Error::Invalid(
                "palw_legacy_held_da_v2 cannot be armed: the legacy route's public descent and held DA units ship only with RFC-0014's \
                 non-seat reservation and Final hold (LG14-A), reviewed economics and the full-activation release",
            )),
            _ => Ok(()),
        }
    }
}

// ---- units, answers, objects ----------------------------------------------------------------------------------------------------

/// **One unit a legacy held demand names** (appended to `PalwDaUnitV1` as `LegacyHeldV2`). Ordered — sessions and the claim's
/// `answered` set iterate it in consensus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum PalwLegacyHeldUnitV2 {
    /// An interior node (`level ≥ 1`) of the claim's step tree, `index` from the left.
    StepNode { level: u8, index: u64 },
    /// An interior node (`level ≥ 1`) of the claim's checkpoint tree.
    CheckpointNode { level: u8, index: u64 },
    /// The committed half of step leaf `leaf`'s refutation (CKW).
    KernelWitness { leaf: u64 },
}

/// **Which committed tree a descent walks.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PalwLegacyTreeV2 {
    Step,
    Checkpoint,
}

impl PalwLegacyTreeV2 {
    /// The tree's leaf count, from the claim's binding.
    pub fn leaf_count(self, binding: &PalwStepBindingV2) -> u64 {
        match self {
            Self::Step => binding.step_leaf_count,
            Self::Checkpoint => u64::from(binding.checkpoint_count),
        }
    }

    /// The tree's root, from the claim's binding.
    pub fn root(self, binding: &PalwStepBindingV2) -> Hash64 {
        match self {
            Self::Step => binding.step_merkle_root,
            Self::Checkpoint => binding.checkpoint_merkle_root,
        }
    }

    /// The unit that demands node `(level, index)` of this tree.
    pub fn node_unit(self, level: u8, index: u64) -> PalwLegacyHeldUnitV2 {
        match self {
            Self::Step => PalwLegacyHeldUnitV2::StepNode { level, index },
            Self::Checkpoint => PalwLegacyHeldUnitV2::CheckpointNode { level, index },
        }
    }
}

impl PalwLegacyHeldUnitV2 {
    /// The tree and node a descent unit names; `None` for a CKW.
    pub fn node(&self) -> Option<(PalwLegacyTreeV2, u8, u64)> {
        match *self {
            Self::StepNode { level, index } => Some((PalwLegacyTreeV2::Step, level, index)),
            Self::CheckpointNode { level, index } => Some((PalwLegacyTreeV2::Checkpoint, level, index)),
            Self::KernelWitness { .. } => None,
        }
    }
}

/// **The committed half of one leaf's refutation** (CKW): the refutation the one-move court reads, minus what the verifier supplies
/// itself (the artifact rows, from its own copy). At a fused-attention site `inputs` is empty and `kv_checkpoint` is `None` — the
/// held dissection carries the history.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwCommittedKernelWitnessV2 {
    pub refutation: PalwExecutionStepRefutationV1,
    /// The prompt's one tile a gather reads, under a Merkle prompt commitment (the one-move court's carriage).
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
}

/// **An answer, in the unit's own form.**
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwLegacyHeldAnswerV2 {
    /// A node unit: the frontier `min(level, PALW_LEGACY_HELD_NODE_DEPTH_V2)` levels down and the node's siblings to the root. Above
    /// level 0 the frontier is tree nodes; AT level 0 it is the committed leaf HASHES themselves (whose leaf nodes are
    /// `step_merkle_leaf_v1(i, hash)`), so the bottom round hands the verifier the hash a leaf recompute opens.
    Node { frontier: Vec<Hash64>, siblings: Vec<Hash64> },
    /// A CKW.
    KernelWitness(Box<PalwCommittedKernelWitnessV2>),
}

/// **Tag 157's payload: a demand of one legacy held unit**, with the claim's binding (authenticated against its `execution_root`,
/// which bounds the unit). Any Active bond may file it under DA-8's budgets; it opens an R-core+ DA session naming exactly that
/// unit (no draws).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegacyHeldDemandV2 {
    pub version: u16,
    pub claim: Hash64,
    pub unit: PalwLegacyHeldUnitV2,
    pub accuser: PalwBondKeyV2,
    pub binding: PalwStepBindingV2,
    /// The accuser's ML-DSA-87 over [`palw_legacy_held_demand_message_v2`] under [`PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2`].
    pub signature: Vec<u8>,
}

/// **Tag 158's payload: the answer to every open session that demands the unit.** The discloser is the producer or a bond still
/// liable on the claim (X7), exactly as tag 55.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegacyHeldAnswerCarriageV2 {
    pub version: u16,
    pub claim: Hash64,
    pub unit: PalwLegacyHeldUnitV2,
    pub binding: PalwStepBindingV2,
    pub answer: PalwLegacyHeldAnswerV2,
    pub discloser: PalwBondKeyV2,
    /// The discloser's ML-DSA-87 over [`palw_legacy_held_answer_message_v2`] under [`PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2`].
    pub signature: Vec<u8>,
}

/// **Tag 159's payload: a non-fused step convicted by its committed leaf hash** — the one-move court's object with the output
/// preimage EMPTY ([`palw_legacy_recompute_placeholder_v2`]): `refutation.output_opening` opens the committed leaf hash, the
/// inputs and id carriages are the canonical set, and `artifact_openings` are the accuser's own rows against the class's registered
/// root.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwLegacyLeafRecomputeV2 {
    pub version: u16,
    pub claim: Hash64,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub executor_bond: PalwBondKeyV2,
    pub accuser_bond: PalwBondKeyV2,
    pub leaf_index: u64,
    pub refutation: PalwExecutionStepRefutationV1,
    pub artifact_openings: Vec<PalwArtifactOpeningV1>,
    pub prompt_ids_opening: Option<PalwPromptIdsOpeningV1>,
    /// The accuser's ML-DSA-87 over [`palw_legacy_leaf_recompute_message_v2`] under
    /// [`PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2`].
    pub signature: Vec<u8>,
}

/// **The empty output preimage a leaf recompute carries** — the one encoding of "not carried": version 0, the zero coordinate, no
/// values. The court refuses anything else in the slot, so one accusation has one encoding.
pub fn palw_legacy_recompute_placeholder_v2() -> PalwStepTileLeafV1 {
    PalwStepTileLeafV1 {
        version: 0,
        coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
        value_count: 0,
        values_le: Vec::new(),
    }
}

// ---- messages -------------------------------------------------------------------------------------------------------------------

fn keyed(domain: &[u8], network_domain: &[u8]) -> blake2b_simd::State {
    let mut s = blake2b_simd::Params::new().hash_length(64).key(domain).to_state();
    s.update(&(network_domain.len() as u32).to_le_bytes());
    s.update(network_domain);
    s
}

fn finish(state: blake2b_simd::State) -> Hash64 {
    Hash64::from_bytes(state.finalize().as_bytes().try_into().expect("64 bytes"))
}

/// What a demand's accuser signs: the network, the version, the claim, the unit, the accuser and the execution root its binding
/// opens (the binding itself is bound by that root).
pub fn palw_legacy_held_demand_message_v2(network_domain: &[u8], d: &PalwLegacyHeldDemandV2) -> Hash64 {
    let mut s = keyed(PALW_LEGACY_HELD_DEMAND_DOMAIN_V2, network_domain);
    s.update(&d.version.to_le_bytes());
    s.update(d.claim.as_byte_slice());
    s.update(&borsh::to_vec(&d.unit).expect("borsh"));
    s.update(&borsh::to_vec(&d.accuser).expect("borsh"));
    s.update(d.binding.committed_execution_root.as_byte_slice());
    finish(s)
}

/// What an answer's discloser signs: the network, the version, the claim, the unit, the discloser, the binding's root and the
/// answer's bytes.
pub fn palw_legacy_held_answer_message_v2(network_domain: &[u8], a: &PalwLegacyHeldAnswerCarriageV2) -> Hash64 {
    let mut s = keyed(PALW_LEGACY_HELD_ANSWER_DOMAIN_V2, network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(&borsh::to_vec(&a.unit).expect("borsh"));
    s.update(&borsh::to_vec(&a.discloser).expect("borsh"));
    s.update(a.binding.committed_execution_root.as_byte_slice());
    let answer = borsh::to_vec(&a.answer).expect("borsh");
    s.update(&(answer.len() as u64).to_le_bytes());
    s.update(&answer);
    finish(s)
}

/// What a leaf recompute's accuser signs — the one-move court's session id's fields: the network, the version, the claim, its roots,
/// both bonds and the leaf. Not the evidence's bytes: a leaf either recomputes or it does not, whoever names it, and every authentic
/// evidence of one leaf yields one verdict.
pub fn palw_legacy_leaf_recompute_message_v2(network_domain: &[u8], a: &PalwLegacyLeafRecomputeV2) -> Hash64 {
    let mut s = keyed(PALW_LEGACY_LEAF_RECOMPUTE_DOMAIN_V2, network_domain);
    s.update(&a.version.to_le_bytes());
    s.update(a.claim.as_byte_slice());
    s.update(a.execution_root.as_byte_slice());
    s.update(a.trace_root.as_byte_slice());
    s.update(&borsh::to_vec(&a.executor_bond).expect("borsh"));
    s.update(&borsh::to_vec(&a.accuser_bond).expect("borsh"));
    s.update(&a.leaf_index.to_le_bytes());
    finish(s)
}

/// The bytes the close ceiling prices: the answer's, or the recompute's evidence (refutation, artifact rows, prompt tile).
pub fn palw_legacy_held_answer_bytes_v2(answer: &PalwLegacyHeldAnswerV2) -> u64 {
    borsh::to_vec(answer).map(|b| b.len() as u64).unwrap_or(u64::MAX)
}

/// See [`palw_legacy_held_answer_bytes_v2`].
pub fn palw_legacy_leaf_recompute_bytes_v2(a: &PalwLegacyLeafRecomputeV2) -> u64 {
    let refutation = borsh::to_vec(&a.refutation).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    let openings = borsh::to_vec(&a.artifact_openings).map(|b| b.len() as u64).unwrap_or(u64::MAX);
    let prompt = a.prompt_ids_opening.as_ref().map_or(0, |o| borsh::to_vec(o).map(|b| b.len() as u64).unwrap_or(u64::MAX));
    refutation.saturating_add(openings).saturating_add(prompt)
}

/// **A node answer's frontier as tree nodes**: above level 0 it is carried as nodes; at level 0 as the committed leaf hashes, each
/// bound to its index here (`step_merkle_leaf_v1`), exactly as the tree binds it.
pub fn palw_legacy_frontier_nodes_v2(below: u8, first: u64, carried: &[Hash64]) -> Vec<Hash64> {
    if below == 0 {
        carried.iter().enumerate().map(|(k, hash)| step_merkle_leaf_v1(first + k as u64, hash)).collect()
    } else {
        carried.to_vec()
    }
}

/// **The prover's half of a node answer, from a tree's ordered leaf hashes** — the frontier in its carried form (leaf hashes at
/// level 0) and the siblings to the root. For the trees a node can hold whole (the checkpoint tree; a test's); a fold's producer
/// derives the same answer from its retained level and the replayed block ([`PalwLegacyOwnTreeV2`]'s implementors).
pub fn palw_legacy_node_answer_from_leaves_v2(leaf_hashes: &[Hash64], level: u8, index: u64) -> Option<PalwLegacyHeldAnswerV2> {
    let (frontier, siblings) =
        crate::palw_tir_court_v1::palw_step_node_parts_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, leaf_hashes, level, index)?;
    let (below, first, end) =
        palw_step_node_frontier_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, leaf_hashes.len() as u64, level, index)?;
    let frontier = if below == 0 { leaf_hashes.get(first as usize..end as usize)?.to_vec() } else { frontier };
    Some(PalwLegacyHeldAnswerV2::Node { frontier, siblings })
}

/// **The prover's half of a node answer, from any node oracle** — what a producer whose retention is a fold serves: the frontier's
/// nodes (leaf hashes at level 0, from `leaf_hash`), and the siblings up to the root, each read from `node`.
pub fn palw_legacy_node_answer_from_oracle_v2(
    leaf_count: u64,
    level: u8,
    index: u64,
    node: &dyn Fn(u8, u64) -> Option<Hash64>,
    leaf_hash: &dyn Fn(u64) -> Option<Hash64>,
) -> Option<PalwLegacyHeldAnswerV2> {
    let (below, first, end) = palw_step_node_frontier_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, leaf_count, level, index)?;
    let frontier: Vec<Hash64> = if below == 0 {
        (first..end).map(leaf_hash).collect::<Option<Vec<_>>>()?
    } else {
        (first..end).map(|p| node(below, p)).collect::<Option<Vec<_>>>()?
    };
    let mut siblings = Vec::new();
    let (mut l, mut position) = (level, index);
    loop {
        let width = palw_tir_step_tree_width_v1(leaf_count, l)?;
        if width == 1 {
            break;
        }
        let promoted = width % 2 == 1 && position == width - 1;
        if !promoted {
            siblings.push(node(l, position ^ 1)?);
        }
        position /= 2;
        l += 1;
    }
    Some(PalwLegacyHeldAnswerV2::Node { frontier, siblings })
}

// ---- checks (hash arithmetic; the fold's and the filer's one spelling) ----------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwLegacyHeldError {
    #[error("version {got}; this build reads version {expected}")]
    Version { got: u16, expected: u16 },
    #[error("the binding commits to {binding} and the claim to {claim}")]
    NotTheClaimsExecution { binding: Hash64, claim: Hash64 },
    #[error("the binding does not authenticate: {0}")]
    Binding(String),
    #[error("the unit is outside what the claim committed: {0}")]
    OutsideTheCommitment(&'static str),
    #[error("the court never compels this unit: {0}")]
    OutOfScope(&'static str),
    #[error("the answer is not in the unit demanded")]
    AnswerIsAnotherUnit,
    #[error("the node answer does not reach the claim's root: {0}")]
    NodeNotCommitted(&'static str),
    #[error("the witness is not the claim's committed half: {0}")]
    WitnessNotCommitted(String),
    #[error("the recompute is malformed: {0}")]
    Recompute(String),
}

/// The binding, authenticated against the claim's own `execution_root`.
fn authenticated(binding: &PalwStepBindingV2, claim_execution_root: &Hash64) -> Result<(), PalwLegacyHeldError> {
    if binding.committed_execution_root != *claim_execution_root {
        return Err(PalwLegacyHeldError::NotTheClaimsExecution {
            binding: binding.committed_execution_root,
            claim: *claim_execution_root,
        });
    }
    verify_binding_v1(binding).map(|_| ()).map_err(|e| PalwLegacyHeldError::Binding(e.to_string()))
}

/// **Does step leaf `leaf`'s output COPY the registered model?** The `EmbedLookup` gather's tile is a row of the embedding table for
/// an honest producer: a court never compels it (ADR-0177 D2). Such a leaf is tried by the leaf recompute (tag 159), whose model
/// operand is the verifier's own.
pub fn palw_legacy_ckw_leaf_is_model_copy_v2(binding: &PalwStepBindingV2, leaf: u64) -> bool {
    canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf)
        .and_then(|coord| binding.shape_profile.resolve_node_slot(coord.node_slot))
        .is_some_and(|(node, _)| node.op_kind == PalwStepOpKindV1::EmbedLookup)
}

/// **Is step leaf `leaf` a fused-attention site?** (The one spelling, [`crate::palw_shard_court_v1::palw_leaf_is_fused_v2`].)
pub fn palw_legacy_leaf_is_fused_v2(binding: &PalwStepBindingV2, leaf: u64) -> bool {
    crate::palw_shard_court_v1::palw_leaf_is_fused_v2(binding, leaf)
}

/// **Is `unit` a unit this claim committed, and one a court may compel?** The demand half: the binding authenticates against the
/// claim's root; a node is an interior node (`level ≥ 1`) inside the tree; a CKW names a leaf inside the step space with
/// coordinates, never a model copy (ADR-0177 D2).
pub fn palw_legacy_held_check_demand_v2(
    claim_execution_root: &Hash64,
    unit: &PalwLegacyHeldUnitV2,
    binding: &PalwStepBindingV2,
) -> Result<(), PalwLegacyHeldError> {
    use PalwLegacyHeldError::OutsideTheCommitment as outside;
    authenticated(binding, claim_execution_root)?;
    match *unit {
        PalwLegacyHeldUnitV2::StepNode { level, index } | PalwLegacyHeldUnitV2::CheckpointNode { level, index } => {
            let tree = unit.node().expect("a node unit").0;
            if level == 0 {
                return Err(outside("a node is an interior node: a leaf's material is a KernelWitness, a StateChunk or a StepRange"));
            }
            let width = palw_tir_step_tree_width_v1(tree.leaf_count(binding), level).ok_or(outside("the tree has no such level"))?;
            if index >= width {
                return Err(outside("the level has no such node"));
            }
        }
        PalwLegacyHeldUnitV2::KernelWitness { leaf } => {
            if leaf >= binding.step_leaf_count {
                return Err(outside("the step space ends before the leaf"));
            }
            if canonical_step_coordinates(&binding.shape_profile, &binding.job_context, leaf).is_none() {
                return Err(outside("the leaf has no coordinates in this job"));
            }
            if palw_legacy_ckw_leaf_is_model_copy_v2(binding, leaf) {
                return Err(PalwLegacyHeldError::OutOfScope(
                    "an embedding gather's committed tile copies the registered model: it is tried by the leaf recompute (tag 159) \
                     from the verifier's own copy",
                ));
            }
        }
    }
    Ok(())
}

/// **Is this the unit, and is it the claim's?** The answer half — hash arithmetic against the claim's committed roots, never an
/// execution. A node answer's frontier folds to the node and its siblings walk it to the tree's root; a CKW's committed half opens
/// against the step root by membership alone, its binding the demand's, and it carries no artifact row.
pub fn palw_legacy_held_check_answer_v2(
    claim_execution_root: &Hash64,
    unit: &PalwLegacyHeldUnitV2,
    binding: &PalwStepBindingV2,
    answer: &PalwLegacyHeldAnswerV2,
    max_step_leaf_count: u64,
) -> Result<(), PalwLegacyHeldError> {
    palw_legacy_held_check_demand_v2(claim_execution_root, unit, binding)?;
    match (unit, answer) {
        (
            PalwLegacyHeldUnitV2::StepNode { level, index } | PalwLegacyHeldUnitV2::CheckpointNode { level, index },
            PalwLegacyHeldAnswerV2::Node { frontier, siblings },
        ) => {
            let tree = unit.node().expect("a node unit").0;
            let leaf_count = tree.leaf_count(binding);
            let (below, first, _) = palw_step_node_frontier_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, leaf_count, *level, *index)
                .ok_or(PalwLegacyHeldError::NodeNotCommitted("the node is not in the tree"))?;
            let nodes = palw_legacy_frontier_nodes_v2(below, first, frontier);
            palw_step_node_reaches_at_depth_v1(
                PALW_LEGACY_HELD_NODE_DEPTH_V2,
                leaf_count,
                &tree.root(binding),
                *level,
                *index,
                &nodes,
                siblings,
            )
            .map_err(PalwLegacyHeldError::NodeNotCommitted)
        }
        (PalwLegacyHeldUnitV2::KernelWitness { leaf }, PalwLegacyHeldAnswerV2::KernelWitness(witness)) => {
            if witness.refutation.output_opening.leaf_index != *leaf {
                return Err(PalwLegacyHeldError::AnswerIsAnotherUnit);
            }
            if witness.refutation.binding != *binding {
                return Err(PalwLegacyHeldError::WitnessNotCommitted("the witness carries another binding than the claim's".into()));
            }
            crate::palw_step_refute::check_execution_step_committed_half_v1(
                &witness.refutation,
                witness.prompt_ids_opening.as_ref(),
                max_step_leaf_count,
            )
            .map_err(|e| PalwLegacyHeldError::WitnessNotCommitted(e.to_string()))
        }
        _ => Err(PalwLegacyHeldError::AnswerIsAnotherUnit),
    }
}

/// **Tag 159, adjudicated** — on its content and the claim it is bound to (read off the chain by the caller, never off the object).
///
/// 1. the version; the leaf inside the ladder and the one the refutation opens; the output preimage the empty placeholder;
/// 2. the binding the claim's (its root) and recomputing to it (`verify_binding_v1`); the carried profile the claim's class;
/// 3. the leaf has coordinates and is not a fused-attention site (whose terminal is the held dissection);
/// 4. the artifact rows prove against the class's registered root (all or nothing) — the accuser's own copy;
/// 5. [`crate::palw_step_refute::check_execution_step_leaf_hash_v1`]: the canonical inputs against the step root, the id carriages,
///    the kernel's own recomputation, the canonical tile's hash against the committed leaf hash.
///
/// `ExecutorGuilty` convicts the claim's executor; `FalseAccusation` costs the accuser the one-move court's charge.
pub fn palw_legacy_leaf_recompute_verdict_v2(
    accusation: &PalwLegacyLeafRecomputeV2,
    claim: &PalwOneMoveClaimV2,
    ladder: u64,
) -> Result<PalwShardCourtVerdictV1, PalwLegacyHeldError> {
    let malformed = |why: &str| PalwLegacyHeldError::Recompute(why.to_string());
    if accusation.version != PALW_LEGACY_HELD_VERSION_V2 {
        return Err(PalwLegacyHeldError::Version { got: accusation.version, expected: PALW_LEGACY_HELD_VERSION_V2 });
    }
    let refutation = &accusation.refutation;
    if accusation.leaf_index >= ladder {
        return Err(malformed("the leaf is past the ladder"));
    }
    if refutation.output_opening.leaf_index != accusation.leaf_index {
        return Err(malformed("the refutation opens another leaf than the one named"));
    }
    if refutation.output_preimage != palw_legacy_recompute_placeholder_v2() {
        return Err(malformed("a leaf recompute carries no output preimage: the court recomputes it"));
    }
    if accusation.execution_root != claim.execution_root {
        return Err(PalwLegacyHeldError::NotTheClaimsExecution { binding: accusation.execution_root, claim: claim.execution_root });
    }
    authenticated(&refutation.binding, &claim.execution_root)?;
    let declared = refutation.binding.shape_profile.shape_profile_id();
    if declared != claim.class_id {
        return Err(malformed("the carried profile is not the claim's class"));
    }
    if canonical_step_coordinates(&refutation.binding.shape_profile, &refutation.binding.job_context, accusation.leaf_index).is_none()
    {
        return Err(malformed("the leaf has no coordinates in this job"));
    }
    if palw_legacy_leaf_is_fused_v2(&refutation.binding, accusation.leaf_index) {
        return Err(malformed("a fused-attention leaf is tried by its held dissection (KernelWitness, then ShardCourtAccused)"));
    }
    let proven = PalwProvenOperandsV1::from_openings_v1(&accusation.artifact_openings, claim.artifact_root)
        .map_err(|e| PalwLegacyHeldError::Recompute(format!("the artifact rows do not prove against the class root: {e:?}")))?;
    match crate::palw_step_refute::check_execution_step_leaf_hash_v1(
        refutation,
        &proven,
        accusation.prompt_ids_opening.as_ref(),
        ladder,
    ) {
        Ok(_) => Ok(PalwShardCourtVerdictV1::ExecutorGuilty),
        Err(PalwStepRefuteError::NoFaultFound) => Ok(PalwShardCourtVerdictV1::FalseAccusation),
        Err(other) => Err(PalwLegacyHeldError::Recompute(other.to_string())),
    }
}

// ---- court scope (ADR-0177 D2; DA16's central predicate is not on a branch yet — the switch is in the design note §7) -----------

/// **What a unit's bytes are, and who supplies them** — this route's rows of the court-scope inventory, in DA16's vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwLegacyHeldScopeV2 {
    /// `"ClaimWitness"` (hashes of claim material), `"ClaimTrace"` (committed activations), `"ModelWeights"` (registered model rows).
    pub material: &'static str,
    /// `"Demanded"` (a party is compelled; silence is the default) or `"VerifierOwnCopy"` (the filer's own copy, authenticated against
    /// the registered root; never compelled).
    pub supplier: &'static str,
    /// `true` iff the bytes are model bytes — which no demand may compel, at any count.
    pub model_bytes: bool,
}

/// The inventory rows: a node unit is commitment structure, a CKW committed claim trace (never a model copy, never artifact rows),
/// and tag 159's operands the verifier's own model rows.
pub const fn palw_legacy_held_unit_scope_v2(unit: &PalwLegacyHeldUnitV2) -> PalwLegacyHeldScopeV2 {
    match unit {
        PalwLegacyHeldUnitV2::StepNode { .. } | PalwLegacyHeldUnitV2::CheckpointNode { .. } => {
            PalwLegacyHeldScopeV2 { material: "ClaimWitness", supplier: "Demanded", model_bytes: false }
        }
        PalwLegacyHeldUnitV2::KernelWitness { .. } => {
            PalwLegacyHeldScopeV2 { material: "ClaimTrace", supplier: "Demanded", model_bytes: false }
        }
    }
}

/// Tag 159's model operand: the verifier's own rows, never compelled.
pub const PALW_LEGACY_LEAF_RECOMPUTE_OPERAND_SCOPE_V2: PalwLegacyHeldScopeV2 =
    PalwLegacyHeldScopeV2 { material: "ModelWeights", supplier: "VerifierOwnCopy", model_bytes: true };

// ---- the verifier's half: descent and openings from public material (node policy; consensus-inert) ------------------------------

/// **The verifier's OWN tree** — an honest replica of the claim's job, read node by node (level 0 = the leaf NODE
/// `step_merkle_leaf_v1(i, leaf_hash)`, as the committed tree's). `None` where the verifier cannot produce it (its replay failed).
pub trait PalwLegacyOwnTreeV2 {
    fn own_node(&self, level: u8, index: u64) -> Option<Hash64>;
}

/// **One answered node unit, as public material**: node `(level, index)` and its authenticated frontier at `below`, covering
/// `[first, first + nodes.len())` there. Built from a tag-158 answer the fold accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLegacyFrontierV2 {
    pub level: u8,
    pub index: u64,
    pub below: u8,
    pub first: u64,
    /// The frontier as tree nodes (leaf nodes at level 0).
    pub nodes: Vec<Hash64>,
    /// At level 0, the committed leaf hashes the answer carried (`None` above it).
    pub leaf_hashes: Option<Vec<Hash64>>,
}

impl PalwLegacyFrontierV2 {
    /// The frontier a node answer carries for `(level, index)` of a tree of `leaf_count` leaves; `None` where the answer is not
    /// that node's shape.
    pub fn of_answer(leaf_count: u64, level: u8, index: u64, frontier: &[Hash64]) -> Option<Self> {
        let (below, first, end) = palw_step_node_frontier_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, leaf_count, level, index)?;
        (frontier.len() as u64 == end - first).then(|| Self {
            level,
            index,
            below,
            first,
            nodes: palw_legacy_frontier_nodes_v2(below, first, frontier),
            leaf_hashes: (below == 0).then(|| frontier.to_vec()),
        })
    }

    /// The committed leaf hash of leaf `leaf`, where this bottom frontier carried it.
    pub fn leaf_hash(&self, leaf: u64) -> Option<Hash64> {
        self.leaf_hashes.as_ref()?.get(usize::try_from(leaf.checked_sub(self.first)?).ok()?).copied()
    }
}

/// **What a descent asks next** ([`palw_legacy_descent_next_v2`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwLegacyDescentStepV2 {
    /// The claim's root is the verifier's own: nothing to localize in this tree.
    Agrees,
    /// Demand this unit next.
    Demand(PalwLegacyHeldUnitV2),
    /// The first leaf (in the pinned enumeration) whose committed node is not the verifier's own.
    FirstDivergentLeaf(u64),
    /// An authenticated frontier agrees everywhere with the verifier's own nodes below a node that does not: the verifier's OWN
    /// replica is inconsistent (its replay is faulty) — it files nothing.
    OwnTreeInconsistent { level: u8, index: u64 },
    /// The verifier cannot produce its own node here.
    OwnTreeUnavailable { level: u8, index: u64 },
}

/// **The descent, from public material alone.** Starting at the root (which differs from the verifier's own), each answered
/// frontier is compared with the verifier's OWN nodes and the first that differs is the next node; an unanswered node is the next
/// demand; a differing leaf node is the first divergent leaf. The frontiers are the chain's (authenticated before they landed); the
/// verifier never re-derives the producer's view.
pub fn palw_legacy_descent_next_v2(
    tree: PalwLegacyTreeV2,
    leaf_count: u64,
    claim_root: &Hash64,
    own: &dyn PalwLegacyOwnTreeV2,
    answered: &[PalwLegacyFrontierV2],
) -> PalwLegacyDescentStepV2 {
    let height = palw_tir_step_tree_height_v1(leaf_count);
    let Some(own_root) = own.own_node(height, 0) else {
        return PalwLegacyDescentStepV2::OwnTreeUnavailable { level: height, index: 0 };
    };
    if own_root == *claim_root {
        return PalwLegacyDescentStepV2::Agrees;
    }
    if height == 0 {
        return PalwLegacyDescentStepV2::FirstDivergentLeaf(0);
    }
    let (mut level, mut index) = (height, 0u64);
    loop {
        let Some(frontier) = answered.iter().find(|f| f.level == level && f.index == index) else {
            return PalwLegacyDescentStepV2::Demand(tree.node_unit(level, index));
        };
        let mut next = None;
        for (k, node) in frontier.nodes.iter().enumerate() {
            let position = frontier.first + k as u64;
            match own.own_node(frontier.below, position) {
                Some(mine) if mine == *node => {}
                Some(_) => {
                    next = Some(position);
                    break;
                }
                None => return PalwLegacyDescentStepV2::OwnTreeUnavailable { level: frontier.below, index: position },
            }
        }
        let Some(position) = next else {
            return PalwLegacyDescentStepV2::OwnTreeInconsistent { level, index };
        };
        if frontier.below == 0 {
            return PalwLegacyDescentStepV2::FirstDivergentLeaf(position);
        }
        (level, index) = (frontier.below, position);
    }
}

/// **The committed tree as far as public material reaches it, given the first divergent leaf** (RFC-0014 §4.2). A node wholly before
/// the divergent leaf is the verifier's own (every leaf below it agrees); a node under an answered node, at or above that answer's
/// frontier level, is folded from the authenticated frontier. Every sibling an opening of a leaf `j ≤ divergent` needs is one of the
/// two — a node wholly after the divergent leaf is never on such a path — so the verifier builds every opening the terminal reads
/// itself.
pub struct PalwLegacyNodeViewV2<'a> {
    pub leaf_count: u64,
    pub divergent: u64,
    pub frontiers: &'a [PalwLegacyFrontierV2],
    pub own: &'a dyn PalwLegacyOwnTreeV2,
}

impl PalwLegacyNodeViewV2<'_> {
    /// Node `(level, index)` of the CLAIM's tree, or `None` where public material does not reach it.
    pub fn node(&self, level: u8, index: u64) -> Option<Hash64> {
        let width = palw_tir_step_tree_width_v1(self.leaf_count, level)?;
        if index >= width {
            return None;
        }
        let last_leaf = (index.checked_add(1)?.checked_shl(u32::from(level)).unwrap_or(u64::MAX)).min(self.leaf_count);
        if last_leaf <= self.divergent {
            return self.own.own_node(level, index);
        }
        let frontier = self
            .frontiers
            .iter()
            .find(|f| f.below <= level && level <= f.level && (index >> u32::from(f.level - level)) == f.index)?;
        self.fold(frontier, level, index)
    }

    /// Node `(level, index)` folded from `frontier`'s nodes (the tree's own rule: pairs, an odd last node promoted at the level's
    /// global width).
    fn fold(&self, frontier: &PalwLegacyFrontierV2, level: u8, index: u64) -> Option<Hash64> {
        let shift = u32::from(level - frontier.below);
        let lo = index.checked_shl(shift)?;
        let hi = index
            .checked_add(1)?
            .checked_shl(shift)
            .unwrap_or(u64::MAX)
            .min(palw_tir_step_tree_width_v1(self.leaf_count, frontier.below)?);
        let take = |p: u64| frontier.nodes.get(usize::try_from(p.checked_sub(frontier.first)?).ok()?).copied();
        let mut nodes: Vec<Hash64> = (lo..hi).map(take).collect::<Option<Vec<_>>>()?;
        let mut start = lo;
        for l in frontier.below..level {
            let width = palw_tir_step_tree_width_v1(self.leaf_count, l)?;
            let mut next = Vec::with_capacity(nodes.len().div_ceil(2));
            let mut k = 0usize;
            while k < nodes.len() {
                let position = start + k as u64;
                if position + 1 < width {
                    next.push(step_merkle_node_v1(&nodes[k], nodes.get(k + 1)?));
                    k += 2;
                } else {
                    next.push(nodes[k]);
                    k += 1;
                }
            }
            nodes = next;
            start /= 2;
        }
        (nodes.len() == 1).then(|| nodes[0])
    }

    /// **The siblings of leaf `j`'s opening** (`step_opening_root_capped_v1`'s order: bottom-up, a promoted odd node skipped).
    pub fn leaf_siblings(&self, mut j: u64) -> Option<Vec<Hash64>> {
        if j >= self.leaf_count {
            return None;
        }
        let mut out = Vec::new();
        let (mut level, mut width) = (0u8, self.leaf_count);
        while width > 1 {
            let promoted = width % 2 == 1 && j == width - 1;
            if !promoted {
                out.push(self.node(level, j ^ 1)?);
            }
            j /= 2;
            width = width.div_ceil(2);
            level += 1;
        }
        Some(out)
    }

    /// **The siblings of a range opening of `[first, first + count)`** (`step_range_opening_root_capped_v1`'s order, as
    /// `step_merkle_range_siblings_v1` builds them from a whole tree).
    pub fn range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        let end = first.checked_add(count)?;
        if count == 0 || end > self.leaf_count {
            return None;
        }
        let (mut a, mut b, mut width, mut level) = (first, end, self.leaf_count, 0u8);
        let mut out = Vec::new();
        while width > 1 {
            if a % 2 == 1 {
                out.push(self.node(level, a - 1)?);
            }
            if b % 2 == 1 {
                let promoted = width % 2 == 1 && b == width;
                if !promoted {
                    out.push(self.node(level, b)?);
                }
            }
            a /= 2;
            b = b.div_ceil(2);
            width = width.div_ceil(2);
            level += 1;
        }
        Some(out)
    }
}

/// **The worst-case descent**: node rounds to the first divergent leaf of a tree of `leaf_count` leaves, at
/// [`PALW_LEGACY_HELD_NODE_DEPTH_V2`] levels a round — `⌈height / depth⌉`.
pub fn palw_legacy_descent_rounds_v2(leaf_count: u64) -> u32 {
    u32::from(palw_tir_step_tree_height_v1(leaf_count)).div_ceil(u32::from(PALW_LEGACY_HELD_NODE_DEPTH_V2))
}

/// **A verifier's resident set for the descent, in bytes**: its own retained vector at `retain_level` (64 B a node), one block of
/// leaf hashes in flight, and the largest frontier it holds — never the whole capture.
pub fn palw_legacy_verifier_resident_bytes_v2(leaf_count: u64, retain_level: u32) -> u64 {
    let block = 1u64 << retain_level.min(40);
    let retained = leaf_count.div_ceil(block);
    let frontier = 1u64 << PALW_LEGACY_HELD_NODE_DEPTH_V2;
    (retained + block + frontier) * 64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::params::TESTNET_PARAMS;
    use crate::palw_step_leg::{step_merkle_leaf_v1, step_merkle_path_v1, step_merkle_range_siblings_v1, step_merkle_root_capped_v1};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    /// A whole tree as a node oracle (level 0 = leaf nodes).
    struct Whole {
        levels: Vec<Vec<Hash64>>,
    }

    impl Whole {
        fn of(leaves: &[Hash64]) -> Self {
            let mut levels = vec![leaves.iter().enumerate().map(|(i, l)| step_merkle_leaf_v1(i as u64, l)).collect::<Vec<_>>()];
            while levels.last().unwrap().len() > 1 {
                let last = levels.last().unwrap();
                let mut next = Vec::new();
                let mut pairs = last.chunks_exact(2);
                for pair in &mut pairs {
                    next.push(step_merkle_node_v1(&pair[0], &pair[1]));
                }
                if let [odd] = pairs.remainder() {
                    next.push(*odd);
                }
                levels.push(next);
            }
            Self { levels }
        }
        fn root(&self) -> Hash64 {
            self.levels.last().unwrap()[0]
        }
    }

    impl PalwLegacyOwnTreeV2 for Whole {
        fn own_node(&self, level: u8, index: u64) -> Option<Hash64> {
            self.levels.get(level as usize)?.get(index as usize).copied()
        }
    }

    /// A deterministic pseudo-random stream (no RNG crate in this module's tests).
    fn mix(seed: u64, i: u64) -> u64 {
        let mut x = seed ^ i.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        x ^= x >> 33;
        x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
        x ^= x >> 33;
        x
    }

    /// The producer's answer to a node unit, from its (lying) whole tree — what a responder serves.
    fn answer(liar: &[Hash64], tree_leaves: u64, level: u8, index: u64) -> PalwLegacyFrontierV2 {
        let Some(PalwLegacyHeldAnswerV2::Node { frontier, siblings }) = palw_legacy_node_answer_from_leaves_v2(liar, level, index)
        else {
            panic!("a node of the tree");
        };
        let whole = Whole::of(liar);
        let oracle = palw_legacy_node_answer_from_oracle_v2(tree_leaves, level, index, &|l, i| whole.own_node(l, i), &|i| {
            liar.get(i as usize).copied()
        });
        assert_eq!(
            oracle,
            Some(PalwLegacyHeldAnswerV2::Node { frontier: frontier.clone(), siblings: siblings.clone() }),
            "two provers"
        );
        let (below, first, _) =
            palw_step_node_frontier_at_depth_v1(PALW_LEGACY_HELD_NODE_DEPTH_V2, tree_leaves, level, index).unwrap();
        palw_step_node_reaches_at_depth_v1(
            PALW_LEGACY_HELD_NODE_DEPTH_V2,
            tree_leaves,
            &whole.root(),
            level,
            index,
            &palw_legacy_frontier_nodes_v2(below, first, &frontier),
            &siblings,
        )
        .expect("an honest-to-its-own-tree answer reaches its root");
        PalwLegacyFrontierV2::of_answer(tree_leaves, level, index, &frontier).unwrap()
    }

    /// **The descent finds the FIRST divergent leaf from authenticated frontiers alone, in ⌈height / 9⌉ rounds**, over random trees
    /// and random lies (one leaf, a run of leaves, every leaf from a point on — the consistent-garbage shape), and the node view then
    /// rebuilds every opening of every leaf up to the divergent one — single-leaf paths and range openings — byte for byte as the
    /// whole tree builds them, though the verifier holds only its own tree and the three frontiers.
    #[test]
    fn the_descent_reaches_the_first_divergent_leaf_and_the_view_opens_every_earlier_leaf() {
        for (case, n) in [1u64, 2, 3, 5, 512, 513, 700, 1025, 4097, 9_000].into_iter().enumerate() {
            for shape in 0..3u64 {
                let honest: Vec<Hash64> = (0..n).map(|i| h(mix(case as u64, i))).collect();
                let i = mix(77 + case as u64, shape) % n;
                let mut liar = honest.clone();
                match shape {
                    0 => liar[i as usize] = h(0xBAD),
                    1 => (i..(i + 7).min(n)).for_each(|k| liar[k as usize] = h(0xBAD0 + k)),
                    _ => (i..n).for_each(|k| liar[k as usize] = h(0xBAD00 + k)),
                }
                let own = Whole::of(&honest);
                let claim_root = Whole::of(&liar).root();
                let mut answered = Vec::new();
                let mut rounds = 0u32;
                let found = loop {
                    match palw_legacy_descent_next_v2(PalwLegacyTreeV2::Step, n, &claim_root, &own, &answered) {
                        PalwLegacyDescentStepV2::Demand(PalwLegacyHeldUnitV2::StepNode { level, index }) => {
                            rounds += 1;
                            answered.push(answer(&liar, n, level, index));
                        }
                        PalwLegacyDescentStepV2::FirstDivergentLeaf(leaf) => break leaf,
                        other => panic!("n {n} shape {shape}: {other:?}"),
                    }
                };
                assert_eq!(found, i, "n {n} shape {shape}: the first divergent leaf");
                if n > 1 {
                    let bottom = answered.iter().find(|f| f.below == 0).expect("the bottom round");
                    assert_eq!(bottom.leaf_hash(i), Some(liar[i as usize]), "the bottom round carries the committed leaf hash");
                }
                assert!(rounds <= palw_legacy_descent_rounds_v2(n), "n {n}: {rounds} rounds");
                let view = PalwLegacyNodeViewV2 { leaf_count: n, divergent: i, frontiers: &answered, own: &own };
                // Every leaf up to the divergent one on a small tree; a stride and the edges on a large one (a whole-tree path
                // is O(n) to build, so checking every j is quadratic).
                let stride = (i / 64).max(1);
                for j in (0..=i).filter(|j| *j % stride == 0 || *j + 2 >= i || *j < 2) {
                    assert_eq!(
                        view.leaf_siblings(j).as_deref(),
                        step_merkle_path_v1(&liar, j as usize).ok().as_deref(),
                        "n {n} j {j}"
                    );
                }
                for (first, count) in [(0, 1), (i.saturating_sub(3), 3.min(i + 1)), (i / 2, i - i / 2 + 1)] {
                    if count == 0 || first + count > i + 1 {
                        continue;
                    }
                    assert_eq!(
                        view.range_siblings(first, count).as_deref(),
                        step_merkle_range_siblings_v1(&liar, first as usize, count as usize).ok().as_deref(),
                        "n {n} range {first}+{count}"
                    );
                }
                if n > 1 {
                    assert_eq!(
                        step_merkle_root_capped_v1(&liar, 1 << 40).unwrap(),
                        claim_root,
                        "the oracle tree is the step leg's own"
                    );
                }
            }
        }
    }

    /// **An honest claim is never localized**, and a faulty verifier's replica is caught by the descent itself: the root agrees
    /// (nothing is demanded), and a replica that differs only above a frontier it agrees with ends `OwnTreeInconsistent`.
    #[test]
    fn an_honest_claim_agrees_and_a_faulty_replica_files_nothing() {
        let leaves: Vec<Hash64> = (0..1_000).map(|i| h(mix(5, i))).collect();
        let own = Whole::of(&leaves);
        assert_eq!(
            palw_legacy_descent_next_v2(PalwLegacyTreeV2::Step, 1_000, &own.root(), &own, &[]),
            PalwLegacyDescentStepV2::Agrees
        );
        let claim = Whole::of(&leaves);
        let mut faulty = Whole::of(&leaves);
        let top = faulty.levels.len() - 1;
        faulty.levels[top][0] = h(1);
        let first = answer(&leaves, 1_000, top as u8, 0);
        assert_eq!(
            palw_legacy_descent_next_v2(PalwLegacyTreeV2::Step, 1_000, &claim.root(), &faulty, &[first]),
            PalwLegacyDescentStepV2::OwnTreeInconsistent { level: top as u8, index: 0 }
        );
    }

    /// **Node answers are hash arithmetic against the claim's own roots**, over the held fixture's real binding: a step node and a
    /// checkpoint node answer with their frontiers; a tampered frontier, a short sibling list, a leaf (`level 0`), a node past the
    /// tree and another claim's root are refused by name.
    #[test]
    fn node_answers_are_checked_against_the_claims_step_and_checkpoint_roots() {
        let fx = crate::palw_checkpoint_court_v1::tests::held_fixture(true, 20, None);
        let b = &fx.binding;
        let root = b.committed_execution_root;
        const LADDER: u64 = 1 << 26;
        let n = b.step_leaf_count;
        assert_eq!(fx.leaves.len() as u64, n);
        let height = palw_tir_step_tree_height_v1(n);
        let unit = PalwLegacyHeldUnitV2::StepNode { level: height, index: 0 };
        let Some(PalwLegacyHeldAnswerV2::Node { frontier, siblings }) = palw_legacy_node_answer_from_leaves_v2(&fx.leaves, height, 0)
        else {
            panic!("the root's answer");
        };
        let good = PalwLegacyHeldAnswerV2::Node { frontier: frontier.clone(), siblings: siblings.clone() };
        palw_legacy_held_check_answer_v2(&root, &unit, b, &good, LADDER).expect("the root's frontier");
        let mut tampered = frontier.clone();
        tampered[0] = h(9);
        assert!(matches!(
            palw_legacy_held_check_answer_v2(
                &root,
                &unit,
                b,
                &PalwLegacyHeldAnswerV2::Node { frontier: tampered, siblings: siblings.clone() },
                LADDER
            ),
            Err(PalwLegacyHeldError::NodeNotCommitted(_))
        ));
        // A deeper node: its siblings walk it to the root, and dropping one is refused.
        if height > PALW_LEGACY_HELD_NODE_DEPTH_V2 {
            let level = height - PALW_LEGACY_HELD_NODE_DEPTH_V2;
            let unit = PalwLegacyHeldUnitV2::StepNode { level, index: 1 };
            let Some(PalwLegacyHeldAnswerV2::Node { frontier: f, siblings: mut s }) =
                palw_legacy_node_answer_from_leaves_v2(&fx.leaves, level, 1)
            else {
                panic!("an inner node's answer");
            };
            palw_legacy_held_check_answer_v2(
                &root,
                &unit,
                b,
                &PalwLegacyHeldAnswerV2::Node { frontier: f.clone(), siblings: s.clone() },
                LADDER,
            )
            .expect("an inner node's frontier and opening");
            s.pop();
            assert!(
                palw_legacy_held_check_answer_v2(&root, &unit, b, &PalwLegacyHeldAnswerV2::Node { frontier: f, siblings: s }, LADDER)
                    .is_err()
            );
        }
        for bad in [
            PalwLegacyHeldUnitV2::StepNode { level: 0, index: 0 },
            PalwLegacyHeldUnitV2::StepNode { level: height, index: 1 },
            PalwLegacyHeldUnitV2::StepNode { level: height + 1, index: 0 },
            PalwLegacyHeldUnitV2::CheckpointNode { level: 0, index: 0 },
            PalwLegacyHeldUnitV2::KernelWitness { leaf: n },
        ] {
            assert!(
                matches!(palw_legacy_held_check_demand_v2(&root, &bad, b), Err(PalwLegacyHeldError::OutsideTheCommitment(_))),
                "{bad:?}"
            );
        }
        assert!(matches!(
            palw_legacy_held_check_demand_v2(&h(1), &PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 }, b),
            Err(PalwLegacyHeldError::NotTheClaimsExecution { .. })
        ));
        // The checkpoint tree: its leaves are the checkpoint leaf hashes the binding's root commits.
        let c = u64::from(b.checkpoint_count);
        if c > 1 {
            let level = palw_tir_step_tree_height_v1(c);
            let unit = PalwLegacyHeldUnitV2::CheckpointNode { level, index: 0 };
            let Some(PalwLegacyHeldAnswerV2::Node { frontier: f, siblings: s }) =
                palw_legacy_node_answer_from_leaves_v2(&fx.checkpoint_hashes, level, 0)
            else {
                panic!("the checkpoint root's answer");
            };
            palw_legacy_held_check_answer_v2(
                &root,
                &unit,
                b,
                &PalwLegacyHeldAnswerV2::Node { frontier: f.clone(), siblings: s.clone() },
                LADDER,
            )
            .expect("the checkpoint root's frontier");
            // A step-node answer is not a checkpoint-node answer.
            assert!(
                palw_legacy_held_check_answer_v2(
                    &root,
                    &PalwLegacyHeldUnitV2::StepNode { level, index: 0 },
                    b,
                    &PalwLegacyHeldAnswerV2::Node { frontier: f, siblings: s },
                    LADDER
                )
                .is_err()
            );
        }
        // An answer of the wrong form.
        assert_eq!(
            palw_legacy_held_check_answer_v2(&root, &PalwLegacyHeldUnitV2::KernelWitness { leaf: 0 }, b, &good, LADDER)
                .err()
                .map(|e| matches!(e, PalwLegacyHeldError::AnswerIsAnotherUnit | PalwLegacyHeldError::OutOfScope(_))),
            Some(true)
        );
    }

    /// **ADR-0177 D2: no unit compels model bytes, at any count.** Node units are hashes; a CKW is claim trace and is refused at
    /// the embedding gather (the fixture's leaf 0 is the position-0 gather); only tag 159's operand is model rows, and it is the
    /// verifier's own.
    #[test]
    fn no_unit_compels_model_bytes() {
        let fx = crate::palw_checkpoint_court_v1::tests::held_fixture(true, 20, None);
        let b = &fx.binding;
        let root = b.committed_execution_root;
        for unit in [
            PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 },
            PalwLegacyHeldUnitV2::CheckpointNode { level: 1, index: 0 },
            PalwLegacyHeldUnitV2::KernelWitness { leaf: 0 },
        ] {
            let scope = palw_legacy_held_unit_scope_v2(&unit);
            assert!(!scope.model_bytes && scope.supplier == "Demanded", "{unit:?}");
        }
        assert!(PALW_LEGACY_LEAF_RECOMPUTE_OPERAND_SCOPE_V2.model_bytes);
        assert_eq!(PALW_LEGACY_LEAF_RECOMPUTE_OPERAND_SCOPE_V2.supplier, "VerifierOwnCopy");
        assert!(palw_legacy_ckw_leaf_is_model_copy_v2(b, 0), "leaf 0 is the position-0 embedding gather");
        assert!(matches!(
            palw_legacy_held_check_demand_v2(&root, &PalwLegacyHeldUnitV2::KernelWitness { leaf: 0 }, b),
            Err(PalwLegacyHeldError::OutOfScope(_))
        ));
        let gathers = (0..b.step_leaf_count).filter(|leaf| palw_legacy_ckw_leaf_is_model_copy_v2(b, *leaf)).count() as u64;
        let demandable = (0..b.step_leaf_count)
            .filter(|leaf| palw_legacy_held_check_demand_v2(&root, &PalwLegacyHeldUnitV2::KernelWitness { leaf: *leaf }, b).is_ok())
            .count() as u64;
        assert_eq!(gathers + demandable, b.step_leaf_count, "every other leaf of the step space is demandable");
    }

    /// **A-2's second lock and the predicate.** Below the fence (the extras flag unset — every shipped network) the fold refuses each
    /// of tags 157–159 by name before reading it; with the flag set it reads past the lock (here, to a claim the chain does not have).
    /// The acceptance walk's drop-by-name predicate names the three tags and every int-12 object carrying the appended unit, and
    /// nothing else.
    #[test]
    fn below_the_fence_the_fold_refuses_every_legacy_held_object_and_the_predicate_names_the_carriers() {
        use crate::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
        use crate::palw_state_v2::{
            PalwBlockContextV2, PalwChainStateV2, PalwConsensusObjectV2 as O, PalwStateParamsV2, PalwStateV2Error,
            PalwTransitionExtrasV1, apply_palw_transition_v2_with_extras, palw_object_is_legacy_held_da_v2,
        };
        let fx = crate::palw_checkpoint_court_v1::tests::held_fixture(true, 20, None);
        let bond = |v: u64| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(v), 0));
        let unit = PalwLegacyHeldUnitV2::StepNode { level: 1, index: 0 };
        let objects = [
            O::LegacyHeldDemandedV2 {
                demand: Box::new(PalwLegacyHeldDemandV2 {
                    version: PALW_LEGACY_HELD_VERSION_V2,
                    claim: h(1),
                    unit,
                    accuser: bond(2),
                    binding: fx.binding.clone(),
                    signature: vec![1],
                }),
            },
            O::LegacyHeldAnsweredV2 {
                answer: Box::new(PalwLegacyHeldAnswerCarriageV2 {
                    version: PALW_LEGACY_HELD_VERSION_V2,
                    claim: h(1),
                    unit,
                    binding: fx.binding.clone(),
                    answer: PalwLegacyHeldAnswerV2::Node { frontier: vec![h(3)], siblings: Vec::new() },
                    discloser: bond(1),
                    signature: vec![1],
                }),
            },
        ];
        let p = PalwStateParamsV2::new(100, 10, 10, 20, 600, 1000, h(1), 4, 1000, 10_000, 1000, 0).unwrap();
        let ctx = PalwBlockContextV2 { block: h(0x4E1D), daa_score: 10, blue_score: 10, subsidy: 10_000 };
        let genesis = PalwChainStateV2::genesis();
        let fold = |object: &O, armed: bool| {
            let extras = PalwTransitionExtrasV1 { legacy_held_da_v2_active: armed, ..Default::default() };
            apply_palw_transition_v2_with_extras(
                &genesis,
                &p,
                &ctx,
                std::slice::from_ref(object),
                None,
                false,
                false,
                false,
                true,
                &extras,
            )
            .map(|_| ())
        };
        for object in &objects {
            assert!(palw_object_is_legacy_held_da_v2(object));
            assert!(matches!(fold(object, false), Err(PalwStateV2Error::LegacyHeldDaDormant)), "{:?}", fold(object, false));
            let armed = fold(object, true);
            assert!(armed.is_err() && !matches!(armed, Err(PalwStateV2Error::LegacyHeldDaDormant)), "past the lock: {armed:?}");
        }
        // An int-12 answer carrying the appended unit is the predicate's; the same answer naming an int-12 unit is not.
        let event = PalwDaAnswerV1::Held(Box::new(crate::palw_held_da_v1::PalwHeldDisclosureCarriageV1 {
            version: crate::palw_held_da_v1::PALW_HELD_DA_VERSION_V1,
            claim: h(1),
            missing: crate::palw_held_da_v1::PalwHeldMissingV1::StepRange { first: 0, count: 1 },
            binding: fx.binding.clone(),
            disclosure: crate::palw_held_da_v1::PalwHeldDisclosureV1::StepRange {
                opening: crate::palw_step_leg::PalwStepRangeOpeningV1 {
                    first_leaf_index: 0,
                    leaf_hashes: vec![h(1)],
                    siblings: Vec::new(),
                },
            },
            signature: Vec::new(),
        }));
        let carrying = O::MaterialDisclosedV2 {
            claim: h(1),
            unit: PalwDaUnitV1::LegacyHeldV2(unit),
            answer: event.clone(),
            discloser: bond(1),
            signature: vec![1],
        };
        let plain = O::MaterialDisclosedV2 {
            claim: h(1),
            unit: PalwDaUnitV1::Event { row: 0, tile: 0 },
            answer: event,
            discloser: bond(1),
            signature: vec![1],
        };
        assert!(palw_object_is_legacy_held_da_v2(&carrying) && !palw_object_is_legacy_held_da_v2(&plain));
        assert!(!palw_object_is_legacy_held_da_v2(&O::DaTransferV1 { claim: h(1), producer: bond(1), signature: vec![1] }));
        assert!(PalwDaUnitV1::LegacyHeldV2(unit).is_legacy_held_v2() && !PalwDaUnitV1::Event { row: 0, tile: 0 }.is_legacy_held_v2());
        // DA-7: no mask covers a legacy held unit (the seats' answering does not build it).
        assert!(!crate::palw_da_rcore_v1::palw_da_unit_covered_by_v1(
            &PalwDaUnitV1::LegacyHeldV2(unit),
            crate::palw_verification_v2::PalwSegmentMaskV2::full(4),
            4
        ));
    }

    /// The fence: dormant everywhere, hashed only when set, refused when armed, collapsed from `never()`, listed and probed.
    #[test]
    fn the_fence_is_dormant_everywhere_hashed_when_set_and_refused_when_armed() {
        use crate::config::params::{DEVNET_PARAMS, MAINNET_PARAMS, SIMNET_PARAMS};
        for p in [MAINNET_PARAMS, TESTNET_PARAMS, SIMNET_PARAMS, DEVNET_PARAMS] {
            assert_eq!(p.palw_legacy_held_da_v2, None);
            assert!(!p.palw_legacy_held_da_v2_active_at(u64::MAX));
            p.validate_palw_legacy_held_da_v2().unwrap();
            assert!(p.palw_fences_v1().contains(&("palw_legacy_held_da_v2", None)), "the exhaustive list names it");
        }
        let mut p = TESTNET_PARAMS;
        let (id, schedule) = (p.consensus_params_id(), p.consensus_schedule_id());
        p.palw_legacy_held_da_v2 = Some(ForkActivation::never());
        p.validate_palw_legacy_held_da_v2().unwrap();
        assert!(!p.palw_legacy_held_da_v2_active_at(u64::MAX));
        p.palw_legacy_held_da_v2 = Some(ForkActivation::new(9_000));
        assert!(p.validate_palw_legacy_held_da_v2().is_err() && p.validate_palw_v2().is_err());
        assert!(p.palw_legacy_held_da_v2_active_at(9_000) && !p.palw_legacy_held_da_v2_active_at(8_999));
        assert_ne!(p.consensus_params_id(), id, "an armed height is another network");
        assert_ne!(p.consensus_schedule_id(), schedule, "and another schedule");
        let mut q = TESTNET_PARAMS;
        q.palw_legacy_held_da_v2 = Some(ForkActivation::new(9_001));
        assert_ne!(q.consensus_params_id(), p.consensus_params_id(), "the height is in the id");
    }

    /// The tags and the domains are the allocated ones and distinct; the messages bind every signed field.
    #[test]
    fn the_tags_domains_and_messages_are_the_allocated_ones() {
        assert_eq!(
            (PALW_LEGACY_HELD_DEMAND_TAG_V2, PALW_LEGACY_HELD_ANSWER_TAG_V2, PALW_LEGACY_LEAF_RECOMPUTE_TAG_V2),
            (157, 158, 159)
        );
        let mut all = PALW_LEGACY_HELD_ALL_DOMAINS_V2.to_vec();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), PALW_LEGACY_HELD_ALL_DOMAINS_V2.len());
        let fx = crate::palw_checkpoint_court_v1::tests::held_fixture(true, 20, None);
        let bond = |v: u64| PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::tx::TransactionId::from_u64_word(v), 0));
        let d = PalwLegacyHeldDemandV2 {
            version: PALW_LEGACY_HELD_VERSION_V2,
            claim: h(1),
            unit: PalwLegacyHeldUnitV2::StepNode { level: 3, index: 1 },
            accuser: bond(2),
            binding: fx.binding.clone(),
            signature: Vec::new(),
        };
        let m = palw_legacy_held_demand_message_v2(b"net", &d);
        assert_ne!(m, palw_legacy_held_demand_message_v2(b"other", &d), "the network is in the message");
        assert_ne!(m, palw_legacy_held_demand_message_v2(b"net", &PalwLegacyHeldDemandV2 { accuser: bond(3), ..d.clone() }));
        assert_ne!(
            m,
            palw_legacy_held_demand_message_v2(
                b"net",
                &PalwLegacyHeldDemandV2 { unit: PalwLegacyHeldUnitV2::StepNode { level: 3, index: 2 }, ..d.clone() }
            )
        );
        assert_eq!(palw_legacy_descent_rounds_v2(105_500_000), 3, "the canonical 8k row: three node rounds");
        assert_eq!(palw_legacy_descent_rounds_v2(1 << 27), 3);
        assert_eq!(palw_legacy_descent_rounds_v2((1 << 27) + 1), 4);
        // The canonical 8k verifier's resident set at retain level 12: ≈ 1.95 MB, against 6.75 GB of leaf hashes for the whole capture.
        let resident = palw_legacy_verifier_resident_bytes_v2(105_500_000, 12);
        assert!(resident < 2_100_000, "{resident}");
        assert!(105_500_000u64 * 64 > 3_000 * resident);
    }
}
