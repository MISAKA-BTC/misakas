//! **Pipeline-claim data availability** (spec 17 §17.14; RFC-0003's generative claims and RFC-0004's
//! evaluation claims — Phase F's half of RFC-0004), dormant under `palw_improvement_v1`.
//!
//! A pipeline claim's executor commits one step tree (`palw_gen_step_v1`): a keyed Merkle tree per
//! stage over that stage's leaves, and a step root over the stage roots. Its capture is never on chain,
//! so a withholding executor can neither be convicted (no seat holds the leaf a court move opens) nor
//! made to serve it. This module is the IR claim's remedy (`palw_tir_fence2`: `TirStepLeaf`,
//! `TirStepNode`, `DefaultAccusedTirStep`) for these claims:
//!
//! * the DA units `PipelineStepLeaf { stage, index }` and `PipelineStepNode { stage, level, index }`
//!   (`PalwDaUnitV1` tags 5 and 6) and their answers — [`PalwPipelineStepLeafDisclosureV1`],
//!   [`PalwPipelineStepNodeDisclosureV1`] and [`PalwPipelineOutOfRangeV1`] (`PalwDaAnswerV1` tags 7–9);
//! * the demand, object tag 83 (`DefaultAccusedPipelineStep`): [`PalwPipelineStepAccusationV1`], keyed by
//!   the claim alone;
//! * **the checks, by hash arithmetic over the claim's committed roots and nothing else.** A stage's
//!   root is `H(stage-root key, [stage] ‖ LE u64 count ‖ M)` with `M` the Merkle root of its leaf
//!   hashes, so the stage's leaf count is committed inside the stage root and an answer simply carries
//!   it. The fold derives no class, no program and no step space, and does not judge that a count is
//!   the job's canonical one (the court's `StepLeafCountNotCanonical`), nor a leaf's structure;
//! * a **compact binding** ([`PalwPipelineBindingV1`]): the parts of the claim's execution root and
//!   nothing else (a generative text claim's `palw_gen_execution_root_v1` inputs, a tensor claim's
//!   `palw_gen_tensor_execution_root_v1` inputs, an evaluation claim's `palw_improve_eval_execution_root_v1`
//!   inputs), so a node answer stays near its frontier's 64 KiB.
//!
//! The tree arithmetic is the IR step tree's (width, height, frontier, fold, walk — `palw_tir_court_v1`)
//! with the pipeline's node rule: raw leaf hashes at level 0, `palw_gen_step_v1`'s node hash above, an
//! odd last node promoted.

use crate::Hash64;
use crate::palw_da_rcore_v1::PalwDaUnitV1;
use crate::palw_gen_step_v1::{
    PALW_GEN_STAGE_ROOT_DOMAIN_V1, PALW_GEN_STEP_LEAF_DOMAIN_V1, PALW_GEN_STEP_NODE_DOMAIN_V1, PalwGenLeafCoordV1,
    palw_gen_step_root_v1,
};
use crate::palw_state_v2::PalwBondKeyV2;
use borsh::{BorshDeserialize, BorshSerialize};

/// Wire version of every object in this module.
pub const PALW_PIPELINE_DA_VERSION_V1: u16 = 1;
/// The most stages a pipeline has (spec 04b NF-P1).
pub const PALW_PIPELINE_MAX_STAGES_V1: usize = 16;
/// **Levels a `PipelineStepNode` answer covers**: its frontier is the nodes this many levels below the
/// demanded node (the leaf hashes, when nearer) — at most 2^10 hashes, 64 KiB. One session descends ten
/// levels, as the IR step node's does (`PALW_TIR_STEP_NODE_DEPTH_V1`).
pub const PALW_PIPELINE_STEP_NODE_DEPTH_V1: u8 = 10;
/// The accuser's ML-DSA-87 context for [`PalwPipelineStepAccusationV1`].
pub const PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1: &[u8] = b"misaka-palw/pipeline/da-step-accusation/mldsa87/v1";
/// The accusation message's own domain.
pub const PALW_PIPELINE_ACCUSATION_DOMAIN_V1: &[u8] = b"misaka-palw/pipeline/da-step-accusation/message/v1";

fn keyed64(key: &[u8], parts: &[&[u8]]) -> Hash64 {
    let mut state = blake2b_simd::Params::new().hash_length(64).key(key).to_state();
    for part in parts {
        state.update(part);
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(state.finalize().as_bytes());
    Hash64::from_bytes(out)
}

// ---------------------------------------------------------------------------------------------
// The reserved object tags
// ---------------------------------------------------------------------------------------------

/// **The payload of a reserved object tag** (84 and 85, spec 17 §17.0): uninhabited. A variant that
/// carries it can never be built, and decoding one fails — a block carrying tag 84 or 85 is undecodable,
/// exactly as an unknown tag is (A-2) — while the variants' existence keeps every later tag at its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReservedObjectV1 {}

impl BorshSerialize for PalwReservedObjectV1 {
    fn serialize<W: std::io::Write>(&self, _writer: &mut W) -> std::io::Result<()> {
        match *self {}
    }
}

impl BorshDeserialize for PalwReservedObjectV1 {
    fn deserialize_reader<R: std::io::Read>(_reader: &mut R) -> std::io::Result<Self> {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "a reserved object tag carries nothing"))
    }
}

// ---------------------------------------------------------------------------------------------
// The compact binding
// ---------------------------------------------------------------------------------------------

/// What kind of pipeline claim a claim is — the fold reads it from its own state.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPipelineKindV1 {
    /// An RFC-0003 generative claim (a V5 job on a `gen_classes` class).
    Gen,
    /// An RFC-0004 evaluation claim (a version-9 job).
    Eval,
}

/// **What the fold knows of the claim an answer is for**: its kind, its class and its execution root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPipelineClaimFactsV1 {
    pub kind: PalwPipelineKindV1,
    pub class_id: Hash64,
    pub execution_root: Hash64,
}

/// The parts of a generative claim's execution root (`palw_gen_execution_root_v1`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineGenPartsV1 {
    pub job_id: Hash64,
    pub class_id: Hash64,
    pub step_leaf_count: u64,
    /// Every stage's root, in stage order (the step root is over them).
    pub stage_roots: Vec<Hash64>,
    pub generated: Vec<u32>,
}

/// The parts of an evaluation claim's execution root (`palw_improve_eval_execution_root_v1`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineEvalPartsV1 {
    pub job_id: Hash64,
    pub subject_class: Hash64,
    pub step_leaf_count: u64,
    pub stage_roots: Vec<Hash64>,
    pub prompt_root: Hash64,
    pub prompt_tokens: u32,
    pub params: crate::palw_improve_eval_v1::PalwEvalStageParamsV1,
    pub generated_root: Hash64,
    pub finalized_root: Hash64,
    pub score: Vec<i32>,
}

/// The parts of a tensor claim's execution root (`palw_gen_tensor_execution_root_v1`, RFC-0003 §I.3): an image or
/// an embedding — its output is a digest, not generated ids.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineTensorPartsV1 {
    pub job_id: Hash64,
    pub class_id: Hash64,
    pub step_leaf_count: u64,
    pub stage_roots: Vec<Hash64>,
    pub output_root: Hash64,
}

/// **What pins a pipeline claim's trees for data availability** — the parts of its execution root. A generative
/// class's claim is a text claim (`Gen`) or a tensor claim (`Tensor`): the execution root's own domain says
/// which, so the fold needs no profile to tell them apart.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPipelineBindingV1 {
    Gen(PalwPipelineGenPartsV1),
    Eval(PalwPipelineEvalPartsV1),
    Tensor(PalwPipelineTensorPartsV1),
}

impl PalwPipelineBindingV1 {
    /// The compact binding of a generative claim's full binding.
    pub fn of_gen(binding: &crate::palw_gen_close_v1::PalwGenStepBindingV1) -> Self {
        Self::Gen(PalwPipelineGenPartsV1 {
            job_id: crate::palw_fp_job_v5::fp_job_id_v5(&binding.job),
            class_id: binding.job.v4.class_id,
            step_leaf_count: binding.step_leaf_count,
            stage_roots: binding.stage_roots.clone(),
            generated: binding.generated.clone(),
        })
    }

    /// The compact binding of an evaluation claim's full binding.
    pub fn of_eval(binding: &crate::palw_improve_eval_v1::PalwEvalBindingV1) -> Self {
        Self::Eval(PalwPipelineEvalPartsV1 {
            job_id: binding.job.id(),
            subject_class: binding.subject_class,
            step_leaf_count: binding.step_leaf_count,
            stage_roots: binding.stage_roots.clone(),
            prompt_root: binding.prompt_root,
            prompt_tokens: binding.prompt_tokens,
            params: binding.params,
            generated_root: binding.generated_root(),
            finalized_root: binding.finalized_root(),
            score: binding.score.clone(),
        })
    }

    /// The compact binding of a tensor claim's full binding.
    pub fn of_tensor(binding: &crate::palw_gen_close_v1::PalwGenTensorBindingV1) -> Self {
        Self::Tensor(PalwPipelineTensorPartsV1 {
            job_id: binding.job.id(),
            class_id: binding.job.envelope.class_id,
            step_leaf_count: binding.step_leaf_count,
            stage_roots: binding.stage_roots.clone(),
            output_root: binding.output_root,
        })
    }

    pub fn kind(&self) -> PalwPipelineKindV1 {
        match self {
            Self::Gen(_) | Self::Tensor(_) => PalwPipelineKindV1::Gen,
            Self::Eval(_) => PalwPipelineKindV1::Eval,
        }
    }

    /// The class the claim ran: a generative claim's, an evaluation claim's subject.
    pub fn class_id(&self) -> Hash64 {
        match self {
            Self::Gen(g) => g.class_id,
            Self::Eval(e) => e.subject_class,
            Self::Tensor(t) => t.class_id,
        }
    }

    pub fn stage_roots(&self) -> &[Hash64] {
        match self {
            Self::Gen(g) => &g.stage_roots,
            Self::Eval(e) => &e.stage_roots,
            Self::Tensor(t) => &t.stage_roots,
        }
    }

    pub fn step_leaf_count(&self) -> u64 {
        match self {
            Self::Gen(g) => g.step_leaf_count,
            Self::Eval(e) => e.step_leaf_count,
            Self::Tensor(t) => t.step_leaf_count,
        }
    }

    /// The step root over the carried stage roots.
    pub fn step_root(&self) -> Hash64 {
        palw_gen_step_root_v1(self.stage_roots())
    }

    /// The execution root the parts produce.
    pub fn execution_root(&self) -> Hash64 {
        match self {
            Self::Gen(g) => crate::palw_gen_close_v1::palw_gen_execution_root_v1(
                &g.job_id,
                &g.class_id,
                g.step_leaf_count,
                &palw_gen_step_root_v1(&g.stage_roots),
                &g.generated,
            ),
            Self::Eval(e) => crate::palw_improve_eval_v1::palw_improve_eval_execution_root_v1(
                &e.job_id,
                &e.subject_class,
                e.step_leaf_count,
                &palw_gen_step_root_v1(&e.stage_roots),
                &e.prompt_root,
                e.prompt_tokens,
                &e.params,
                &e.generated_root,
                &e.finalized_root,
                &e.score,
            ),
            Self::Tensor(t) => crate::palw_gen_close_v1::palw_gen_tensor_execution_root_v1(
                &t.job_id,
                &t.class_id,
                t.step_leaf_count,
                &palw_gen_step_root_v1(&t.stage_roots),
                &t.output_root,
            ),
        }
    }

    /// **Is this the claim's own binding?** The kind the fold reads from its state, the claim's class, at
    /// most 16 stage roots, and the execution root the parts produce — the claim's.
    pub fn answers_claim_v1(&self, claim: &PalwPipelineClaimFactsV1) -> Result<(), &'static str> {
        if self.kind() != claim.kind {
            return Err("the binding is of another kind of pipeline claim");
        }
        if self.class_id() != claim.class_id {
            return Err("the binding names another class than the claim's");
        }
        if self.stage_roots().is_empty() || self.stage_roots().len() > PALW_PIPELINE_MAX_STAGES_V1 {
            return Err("a pipeline has one to sixteen stages");
        }
        if self.execution_root() != claim.execution_root {
            return Err("the binding's parts do not produce the claim's execution root");
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------
// The stage tree's arithmetic
// ---------------------------------------------------------------------------------------------

fn node_hash(left: &Hash64, right: &Hash64) -> Hash64 {
    keyed64(PALW_GEN_STEP_NODE_DOMAIN_V1, &[left.as_byte_slice(), right.as_byte_slice()])
}

/// **A stage's root** from its leaf count and the Merkle root of its leaf hashes — exactly
/// `palw_gen_step_v1::palw_gen_stage_root_v1`'s construction.
pub fn palw_pipeline_stage_root_v1(stage: u8, leaf_count: u64, merkle_root: &Hash64) -> Hash64 {
    keyed64(PALW_GEN_STAGE_ROOT_DOMAIN_V1, &[&[stage], &leaf_count.to_le_bytes(), merkle_root.as_byte_slice()])
}

/// **A leaf's hash**: `H(leaf key, borsh(coord) ‖ LE u32 n ‖ lanes)` with `n` the number of 4-byte lanes —
/// exactly `palw_gen_step_leaf_hash_v1`'s construction over a leaf's lanes. `None` for lanes that are not
/// whole 4-byte lanes.
pub fn palw_pipeline_leaf_hash_v1(coord: &PalwGenLeafCoordV1, lanes_le: &[u8]) -> Option<Hash64> {
    if !lanes_le.len().is_multiple_of(4) {
        return None;
    }
    let n = u32::try_from(lanes_le.len() / 4).ok()?;
    let coord = borsh::to_vec(coord).expect("a coordinate is borsh-serializable");
    Some(keyed64(PALW_GEN_STEP_LEAF_DOMAIN_V1, &[&coord, &n.to_le_bytes(), lanes_le]))
}

/// **The tree's width at `level`** (0 = the leaf hashes; each level `⌈w / 2⌉`, an odd last node
/// promoted), or `None` past the root or for an empty tree.
pub fn palw_pipeline_tree_width_v1(leaf_count: u64, level: u8) -> Option<u64> {
    if leaf_count == 0 {
        return None;
    }
    let mut width = leaf_count;
    for _ in 0..level {
        if width == 1 {
            return None;
        }
        width = width.div_ceil(2);
    }
    Some(width)
}

/// **The root's level**: how many times the leaf hashes fold to one.
pub fn palw_pipeline_tree_height_v1(leaf_count: u64) -> u8 {
    let mut level = 0u8;
    let mut width = leaf_count.max(1);
    while width > 1 {
        width = width.div_ceil(2);
        level += 1;
    }
    level
}

/// The frontier level of node `(level, ·)`'s answer.
fn frontier_level(level: u8) -> u8 {
    level.saturating_sub(PALW_PIPELINE_STEP_NODE_DEPTH_V1)
}

/// The positions node `(level, index)` covers at `below ≤ level`: `[lo, hi)`, or `None` when the node is
/// past the tree.
fn covered(leaf_count: u64, level: u8, index: u64, below: u8) -> Option<(u64, u64)> {
    if index >= palw_pipeline_tree_width_v1(leaf_count, level)? {
        return None;
    }
    let shift = u32::from(level - below);
    let lo = index.checked_shl(shift)?;
    let hi = index.checked_add(1)?.checked_shl(shift)?.min(palw_pipeline_tree_width_v1(leaf_count, below)?);
    (lo < hi).then_some((lo, hi))
}

/// **Where node `(level, index)`'s frontier sits**: `(frontier level, first, end)` — the level
/// `level − 10` (or 0, the leaf hashes, when nearer) and the positions `[first, end)` it covers there;
/// `None` for a leaf or a node past the tree. A seat descending names next the first frontier node its
/// own tree disagrees with: `PipelineStepNode { level: frontier level, index: first + k }`, or, at level 0,
/// `PipelineStepLeaf { index: first + k }`.
pub fn palw_pipeline_node_frontier_v1(leaf_count: u64, level: u8, index: u64) -> Option<(u8, u64, u64)> {
    if level == 0 {
        return None;
    }
    let below = frontier_level(level);
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    Some((below, lo, hi))
}

/// Node `(level, index)`'s hash from its frontier at `below` — the tree's own fold over exactly the
/// positions it covers; `None` when the frontier is not that many nodes.
fn fold_frontier(leaf_count: u64, level: u8, index: u64, below: u8, frontier: &[Hash64]) -> Option<Hash64> {
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    if frontier.len() as u64 != hi - lo {
        return None;
    }
    let mut nodes = frontier.to_vec();
    let mut start = lo;
    for l in below..level {
        let width = palw_pipeline_tree_width_v1(leaf_count, l)?;
        let mut next = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut k = 0usize;
        while k < nodes.len() {
            let position = start + k as u64;
            if !position.is_multiple_of(2) {
                return None;
            }
            if position + 1 < width {
                let right = nodes.get(k + 1)?;
                next.push(node_hash(&nodes[k], right));
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

/// The root node `(level, index)` with hash `node` reaches through `siblings` — the opening's walk from an
/// interior node, promotion as the tree folds; `None` when the siblings are short or long.
fn walk_to_root(leaf_count: u64, level: u8, index: u64, node: Hash64, siblings: &[Hash64]) -> Option<Hash64> {
    let (mut current, mut position, mut l) = (node, index, level);
    let mut supplied = siblings.iter();
    loop {
        let width = palw_pipeline_tree_width_v1(leaf_count, l)?;
        if width == 1 {
            break;
        }
        let promoted = width % 2 == 1 && position == width - 1;
        if !promoted {
            let sibling = supplied.next()?;
            current = if position % 2 == 0 { node_hash(&current, sibling) } else { node_hash(sibling, &current) };
        }
        position /= 2;
        l += 1;
    }
    supplied.next().is_none().then_some(current)
}

/// **The Merkle root of a stage's leaf hashes** (`M` of the stage root): the zero hash for an empty
/// stage, as `palw_gen_step_v1` does.
pub fn palw_pipeline_merkle_root_v1(leaf_hashes: &[Hash64]) -> Hash64 {
    if leaf_hashes.is_empty() {
        return Hash64::from_bytes([0; 64]);
    }
    let mut level = leaf_hashes.to_vec();
    while level.len() > 1 {
        level = level.chunks(2).map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] }).collect();
    }
    level[0]
}

/// **The prover's half: node `(level, index)`'s frontier and opening** from a stage's leaf hashes. `None`
/// when the node is not in the tree or `level` is 0 (a leaf is a `PipelineStepLeaf`).
pub fn palw_pipeline_node_parts_v1(leaf_hashes: &[Hash64], level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
    let leaf_count = leaf_hashes.len() as u64;
    if level == 0 {
        return None;
    }
    let below = frontier_level(level);
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    let mut levels: Vec<Vec<Hash64>> = vec![leaf_hashes.to_vec()];
    while levels.last()?.len() > 1 {
        let last = levels.last()?;
        let mut next = Vec::with_capacity(last.len().div_ceil(2));
        let mut pairs = last.chunks_exact(2);
        for pair in &mut pairs {
            next.push(node_hash(&pair[0], &pair[1]));
        }
        if let [odd] = pairs.remainder() {
            next.push(*odd);
        }
        levels.push(next);
    }
    let frontier = levels.get(below as usize)?.get(lo as usize..hi as usize)?.to_vec();
    let mut siblings = Vec::new();
    let mut position = index;
    for row in levels.iter().skip(level as usize) {
        let width = row.len() as u64;
        if width == 1 {
            break;
        }
        let promoted = width % 2 == 1 && position == width - 1;
        if !promoted {
            siblings.push(row[(position ^ 1) as usize]);
        }
        position /= 2;
    }
    Some((frontier, siblings))
}

/// **The prover's half: leaf `index`'s opening** (its siblings, bottom-up) from a stage's leaf hashes —
/// `palw_gen_step_v1::palw_gen_leaf_path_v1`'s path.
pub fn palw_pipeline_leaf_siblings_v1(leaf_hashes: &[Hash64], index: u64) -> Option<Vec<Hash64>> {
    let leaf_count = leaf_hashes.len() as u64;
    if index >= leaf_count {
        return None;
    }
    let mut siblings = Vec::new();
    let (mut level, mut position) = (leaf_hashes.to_vec(), index);
    while level.len() > 1 {
        let width = level.len() as u64;
        let promoted = width % 2 == 1 && position == width - 1;
        if !promoted {
            siblings.push(level[(position ^ 1) as usize]);
        }
        level = level.chunks(2).map(|p| if p.len() == 2 { node_hash(&p[0], &p[1]) } else { p[0] }).collect();
        position /= 2;
    }
    Some(siblings)
}

// ---------------------------------------------------------------------------------------------
// The answers
// ---------------------------------------------------------------------------------------------

/// **What a `PipelineStepLeaf { stage, index }` answer discloses**: the leaf's preimage (its
/// coordinate and 4-byte lanes) and its opening to the stage's Merkle root, with the stage's leaf count
/// (committed in the stage root) and the claim's compact binding.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineStepLeafDisclosureV1 {
    pub version: u16,
    pub binding: PalwPipelineBindingV1,
    pub stage_leaf_count: u64,
    pub coord: PalwGenLeafCoordV1,
    pub lanes_le: Vec<u8>,
    pub siblings: Vec<Hash64>,
}

/// **What a `PipelineStepNode { stage, level, index }` answer discloses**: the node's frontier — the
/// nodes [`PALW_PIPELINE_STEP_NODE_DEPTH_V1`] levels below it, or the leaf hashes when nearer — and its
/// siblings up to the stage's Merkle root, with the stage's leaf count and the compact binding.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineStepNodeDisclosureV1 {
    pub version: u16,
    pub binding: PalwPipelineBindingV1,
    pub stage_leaf_count: u64,
    pub frontier: Vec<Hash64>,
    pub siblings: Vec<Hash64>,
}

/// A stage tree's shape as its root commits it: the leaf count and the Merkle root `M`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineStageTreeV1 {
    pub leaf_count: u64,
    pub merkle_root: Hash64,
}

/// **What a `PipelineStepOutOfRange` answer proves**: the claim's binding, and — for a stage the
/// claim has — that stage's tree, so the unit is shown not to be in it. A unit of a stage past the
/// claim's stages is shown by the binding alone (`stage_tree` empty).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineOutOfRangeV1 {
    pub version: u16,
    pub binding: PalwPipelineBindingV1,
    pub stage_tree: Option<PalwPipelineStageTreeV1>,
}

fn check_version(version: u16) -> Result<(), &'static str> {
    if version == PALW_PIPELINE_DA_VERSION_V1 { Ok(()) } else { Err("an unsupported pipeline DA version") }
}

/// The stage's root from the binding, or why there is none.
fn stage_root_of(binding: &PalwPipelineBindingV1, stage: u8) -> Result<Hash64, &'static str> {
    binding.stage_roots().get(stage as usize).copied().ok_or("the claim has no such stage: answered by the out-of-range proof")
}

/// **Verify a `PipelineStepLeaf { stage, index }` answer against the claim — by hash arithmetic.** The
/// binding is the claim's own; `stage` is one of its stages; `index` is a leaf of the stage's tree of
/// `stage_leaf_count` leaves; the leaf's hash, walked up by its siblings from position `index`, reaches a
/// Merkle root that the stage root commits with that count.
pub fn check_pipeline_step_leaf_v1(
    claim: &PalwPipelineClaimFactsV1,
    stage: u8,
    index: u64,
    d: &PalwPipelineStepLeafDisclosureV1,
) -> Result<(), &'static str> {
    check_version(d.version)?;
    d.binding.answers_claim_v1(claim)?;
    let root = stage_root_of(&d.binding, stage)?;
    if index >= d.stage_leaf_count {
        return Err("the leaf is past the stage's tree: answered by the out-of-range proof");
    }
    if d.coord.stage != stage {
        return Err("the leaf's coordinate names another stage");
    }
    let leaf = palw_pipeline_leaf_hash_v1(&d.coord, &d.lanes_le).ok_or("the leaf's lanes are not whole 4-byte lanes")?;
    let merkle =
        walk_to_root(d.stage_leaf_count, 0, index, leaf, &d.siblings).ok_or("the leaf's siblings do not walk to a stage root")?;
    if palw_pipeline_stage_root_v1(stage, d.stage_leaf_count, &merkle) != root {
        return Err("the leaf does not reach the claim's stage root");
    }
    Ok(())
}

/// **The hash arithmetic of a `PipelineStepNode` answer, its binding aside**: in a tree of `leaf_count`
/// leaves, `(level, index)` is an interior node (`level ≥ 1`); `frontier` is exactly the nodes it covers
/// `min(level, 10)` levels down and folds to it by the tree's own rule; `siblings` walk it to a Merkle root,
/// which is returned (what the stage root then commits with the count). What a seat asks of each answer as
/// it descends.
pub fn palw_pipeline_node_reaches_v1(
    leaf_count: u64,
    level: u8,
    index: u64,
    frontier: &[Hash64],
    siblings: &[Hash64],
) -> Result<Hash64, &'static str> {
    if level == 0 {
        return Err("a step node is an interior node: a leaf is demanded as a PipelineStepLeaf");
    }
    let node = fold_frontier(leaf_count, level, index, frontier_level(level), frontier)
        .ok_or("the frontier is not the node's, or the node is not in the tree")?;
    walk_to_root(leaf_count, level, index, node, siblings).ok_or("the node's siblings do not walk to a stage root")
}

/// **Verify a `PipelineStepNode { stage, level, index }` answer against the claim — by hash arithmetic**:
/// the binding is the claim's; the node's frontier and siblings reach a Merkle root that the stage root
/// commits with `stage_leaf_count`.
pub fn check_pipeline_step_node_v1(
    claim: &PalwPipelineClaimFactsV1,
    stage: u8,
    level: u8,
    index: u64,
    d: &PalwPipelineStepNodeDisclosureV1,
) -> Result<(), &'static str> {
    check_version(d.version)?;
    d.binding.answers_claim_v1(claim)?;
    let root = stage_root_of(&d.binding, stage)?;
    let merkle = palw_pipeline_node_reaches_v1(d.stage_leaf_count, level, index, &d.frontier, &d.siblings)?;
    if palw_pipeline_stage_root_v1(stage, d.stage_leaf_count, &merkle) != root {
        return Err("the node does not reach the claim's stage root");
    }
    Ok(())
}

/// **Verify a `PipelineStepOutOfRange` answer**: the binding is the claim's; the unit names a stage past
/// the claim's (the binding alone proves it) or a stage whose tree — its count and Merkle root, committed
/// in the stage root — it is past: a leaf at or past the count, a node above the root or at or past its
/// level's width.
pub fn check_pipeline_out_of_range_v1(
    claim: &PalwPipelineClaimFactsV1,
    unit: &PalwDaUnitV1,
    d: &PalwPipelineOutOfRangeV1,
) -> Result<(), &'static str> {
    check_version(d.version)?;
    d.binding.answers_claim_v1(claim)?;
    let stage = match *unit {
        PalwDaUnitV1::PipelineStepLeaf { stage, .. } | PalwDaUnitV1::PipelineStepNode { stage, .. } => stage,
        _ => return Err("an out-of-range answer answers a pipeline step unit"),
    };
    let Some(root) = d.binding.stage_roots().get(stage as usize) else {
        return if d.stage_tree.is_none() { Ok(()) } else { Err("a stage past the claim's stages carries no tree") };
    };
    let tree = d.stage_tree.ok_or("a stage of the claim is proven past by its tree")?;
    if palw_pipeline_stage_root_v1(stage, tree.leaf_count, &tree.merkle_root) != *root {
        return Err("the stage's tree is not the one its root commits");
    }
    let past = match *unit {
        PalwDaUnitV1::PipelineStepLeaf { index, .. } => index >= tree.leaf_count,
        PalwDaUnitV1::PipelineStepNode { level, index, .. } => {
            palw_pipeline_tree_width_v1(tree.leaf_count, level).is_none_or(|width| index >= width)
        }
        _ => unreachable!("checked above"),
    };
    if !past {
        return Err("the unit is in the execution: it is answered by its disclosure");
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The demand
// ---------------------------------------------------------------------------------------------

/// **Could `unit` be in SOME pipeline execution?** A stage below sixteen; a leaf below the widest ladder
/// any network runs ([`crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1`]); an interior node
/// (`level ≥ 1`) of the widest tree at that ladder. What the gate and the fold check when they open a
/// binding-less demand — a unit past every execution is refused at the door rather than opening a session
/// only an out-of-range proof ends; a unit past the claim's own stage is the accused's to prove.
pub fn palw_pipeline_step_unit_is_admissible_v1(unit: &PalwDaUnitV1) -> Result<(), &'static str> {
    let widest = crate::palw_state_chunk_map::PALW_HELD_STEP_LADDER_V1.max(1);
    match *unit {
        PalwDaUnitV1::PipelineStepLeaf { stage, .. } | PalwDaUnitV1::PipelineStepNode { stage, .. }
            if stage as usize >= PALW_PIPELINE_MAX_STAGES_V1 =>
        {
            Err("a pipeline has at most sixteen stages")
        }
        PalwDaUnitV1::PipelineStepLeaf { index, .. } if index < widest => Ok(()),
        PalwDaUnitV1::PipelineStepLeaf { .. } => Err("a step leaf past every execution's leaves"),
        PalwDaUnitV1::PipelineStepNode { level: 0, .. } => {
            Err("a step node is an interior node: a leaf is demanded as a PipelineStepLeaf")
        }
        PalwDaUnitV1::PipelineStepNode { level, index, .. } => match palw_pipeline_tree_width_v1(widest, level) {
            Some(width) if index < width => Ok(()),
            _ => Err("a step node past every execution's tree"),
        },
        _ => Err("a pipeline step demand names a step leaf or a step node"),
    }
}

/// **A data-availability demand for one unit of a pipeline claim's step tree** (`DefaultAccusedPipelineStep`,
/// object tag 83), keyed by the claim alone: it carries no binding — a seat facing a producer that served
/// nothing holds none, and the claim's own roots are what every answer is checked against. Named only: no
/// draws. Signed by the accuser's bond over [`palw_pipeline_step_accusation_message_v1`].
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwPipelineStepAccusationV1 {
    pub claim: Hash64,
    pub unit: PalwDaUnitV1,
    pub accuser: PalwBondKeyV2,
    pub signature: Vec<u8>,
}

/// **What a pipeline step demand is signed over**: `H(domain ‖ network ‖ claim ‖ borsh(unit) ‖ accuser)`. A
/// replay of an answered demand opens nothing (the M3 review's F3 rule).
pub fn palw_pipeline_step_accusation_message_v1(
    network_domain: Hash64,
    claim: &Hash64,
    unit: &PalwDaUnitV1,
    accuser: &PalwBondKeyV2,
) -> Hash64 {
    keyed64(
        PALW_PIPELINE_ACCUSATION_DOMAIN_V1,
        &[
            network_domain.as_byte_slice(),
            claim.as_byte_slice(),
            &borsh::to_vec(unit).expect("a unit serializes"),
            &borsh::to_vec(accuser).expect("a bond key serializes"),
        ],
    )
}

/// **The ONE builder of a `DefaultAccusedPipelineStep`** — what a seat files when its replay disputes the
/// claim below `unit` and the producer has not served it: the unit held to
/// [`palw_pipeline_step_unit_is_admissible_v1`], signed by `sign(message, context)` with the accuser's key,
/// held to the ride rule.
pub fn palw_pipeline_step_accusation_object_v1(
    network_domain: &Hash64,
    claim: Hash64,
    unit: PalwDaUnitV1,
    accuser: PalwBondKeyV2,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<crate::palw_state_v2::PalwConsensusObjectV2, crate::palw_da_rcore_v1::PalwDaAccusationBuildErrorV1> {
    use crate::palw_da_rcore_v1::PalwDaAccusationBuildErrorV1 as E;
    palw_pipeline_step_unit_is_admissible_v1(&unit).map_err(|why| E::NotAPipelineStepUnit(unit, why))?;
    let message = palw_pipeline_step_accusation_message_v1(*network_domain, &claim, &unit, &accuser);
    let signature = sign(message.as_byte_slice(), PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1)
        .filter(|signature| !signature.is_empty())
        .ok_or(E::Unsigned)?;
    let object = crate::palw_state_v2::PalwConsensusObjectV2::DefaultAccusedPipelineStep {
        accusation: Box::new(PalwPipelineStepAccusationV1 { claim, unit, accuser, signature }),
    };
    crate::palw_lifecycle_objects_v2::palw_lifecycle_object_may_ride_v2(&object).map_err(E::CannotRide)?;
    Ok(object)
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_gen_step_v1::{PalwGenLeafKindV1, palw_gen_leaf_path_v1, palw_gen_stage_root_v1};

    fn h(n: u64) -> Hash64 {
        keyed64(b"misaka-palw/pipeline-da-tests", &[&n.to_le_bytes()])
    }

    fn coord(stage: u8, i: u64) -> PalwGenLeafCoordV1 {
        PalwGenLeafCoordV1 {
            stage,
            pos: (i / 3) as u32,
            kind: PalwGenLeafKindV1::Commit { occurrence: 0, node: (i % 3) as u16 },
            tile: (i % 5) as u32,
        }
    }

    fn lanes(i: u64) -> Vec<u8> {
        (0..(1 + i % 4)).flat_map(|k| ((i * 7 + k) as u32).to_le_bytes()).collect()
    }

    /// A stage's leaves: their preimages and hashes.
    fn stage_leaves(stage: u8, n: u64) -> Vec<Hash64> {
        (0..n).map(|i| palw_pipeline_leaf_hash_v1(&coord(stage, i), &lanes(i)).expect("whole lanes")).collect()
    }

    /// A generative claim over three stages of `counts` leaves: its binding and the claim's facts.
    fn gen_claim(counts: [u64; 3]) -> (PalwPipelineBindingV1, PalwPipelineClaimFactsV1, Vec<Vec<Hash64>>) {
        let leaves: Vec<Vec<Hash64>> = counts.iter().enumerate().map(|(s, n)| stage_leaves(s as u8, *n)).collect();
        let stage_roots: Vec<Hash64> = leaves.iter().enumerate().map(|(s, l)| palw_gen_stage_root_v1(s as u8, l)).collect();
        let binding = PalwPipelineBindingV1::Gen(PalwPipelineGenPartsV1 {
            job_id: h(1),
            class_id: h(2),
            step_leaf_count: counts.iter().sum(),
            stage_roots,
            generated: vec![5, 6, 7],
        });
        let facts =
            PalwPipelineClaimFactsV1 { kind: PalwPipelineKindV1::Gen, class_id: h(2), execution_root: binding.execution_root() };
        (binding, facts, leaves)
    }

    fn leaf_answer(binding: &PalwPipelineBindingV1, leaves: &[Hash64], stage: u8, i: u64) -> PalwPipelineStepLeafDisclosureV1 {
        PalwPipelineStepLeafDisclosureV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_leaf_count: leaves.len() as u64,
            coord: coord(stage, i),
            lanes_le: lanes(i),
            siblings: palw_pipeline_leaf_siblings_v1(leaves, i).expect("in the tree"),
        }
    }

    fn node_answer(binding: &PalwPipelineBindingV1, leaves: &[Hash64], level: u8, i: u64) -> PalwPipelineStepNodeDisclosureV1 {
        let (frontier, siblings) = palw_pipeline_node_parts_v1(leaves, level, i).expect("a node of the tree");
        PalwPipelineStepNodeDisclosureV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_leaf_count: leaves.len() as u64,
            frontier,
            siblings,
        }
    }

    const COUNTS: [u64; 22] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 33, 64, 65, 255, 256, 257, 1000, 1023, 1025, 3000];

    /// **The stage root, the leaf hash and the path are the generative lane's own**: the pipeline's
    /// arithmetic reproduces `palw_gen_step_v1` byte for byte, for every size.
    #[test]
    fn the_stage_root_the_leaf_hash_and_the_path_are_the_gen_lanes() {
        for n in COUNTS {
            let leaves = stage_leaves(3, n);
            assert_eq!(
                palw_pipeline_stage_root_v1(3, n, &palw_pipeline_merkle_root_v1(&leaves)),
                palw_gen_stage_root_v1(3, &leaves),
                "{n} leaves: the stage root"
            );
            for i in [0, n / 2, n - 1] {
                assert_eq!(
                    palw_pipeline_leaf_siblings_v1(&leaves, i),
                    palw_gen_leaf_path_v1(&leaves, i as usize),
                    "{n} leaves, leaf {i}"
                );
            }
            assert_eq!(palw_pipeline_leaf_siblings_v1(&leaves, n), None);
        }
        let empty: Vec<Hash64> = Vec::new();
        assert_eq!(
            palw_pipeline_stage_root_v1(2, 0, &palw_pipeline_merkle_root_v1(&empty)),
            palw_gen_stage_root_v1(2, &empty),
            "an empty stage"
        );
        // A leaf's hash: its coordinate, its lane count and its lanes.
        assert_eq!(palw_pipeline_leaf_hash_v1(&coord(0, 4), &[1, 2, 3]), None, "lanes are whole 4-byte lanes");
        assert_ne!(
            palw_pipeline_leaf_hash_v1(&coord(0, 4), &lanes(4)),
            palw_pipeline_leaf_hash_v1(&coord(1, 4), &lanes(4)),
            "the coordinate is in the preimage"
        );
    }

    /// **Every node of every small tree is answered and reaches its stage root**, with the frontier the
    /// arithmetic names; a tampered answer never does.
    #[test]
    fn every_node_of_every_small_tree_is_answered_and_a_tampered_one_is_refused() {
        for n in COUNTS {
            let (binding, facts, trees) = gen_claim([n, 7, 2]);
            let leaves = &trees[0];
            let height = palw_pipeline_tree_height_v1(n);
            for level in 1..=height {
                let width = palw_pipeline_tree_width_v1(n, level).expect("a level of the tree");
                let indices: Vec<u64> = if width <= 6 { (0..width).collect() } else { vec![0, 1, width / 2, width - 2, width - 1] };
                for index in indices {
                    let d = node_answer(&binding, leaves, level, index);
                    check_pipeline_step_node_v1(&facts, 0, level, index, &d)
                        .unwrap_or_else(|e| panic!("{n} leaves ({level}, {index}): {e}"));
                    let (below, first, end) = palw_pipeline_node_frontier_v1(n, level, index).expect("a frontier");
                    assert_eq!(below, level.saturating_sub(PALW_PIPELINE_STEP_NODE_DEPTH_V1));
                    assert_eq!(d.frontier.len() as u64, end - first);
                    assert!(d.frontier.len() <= 1 << PALW_PIPELINE_STEP_NODE_DEPTH_V1);
                    // Tampering: any frontier node or sibling flipped, dropped or added; the wrong node,
                    // level, stage or count.
                    let mut bad = d.clone();
                    bad.frontier[0] = h(99);
                    assert!(check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(), "a flipped frontier node");
                    let mut bad = d.clone();
                    bad.frontier.pop();
                    assert!(check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(), "a short frontier");
                    if let Some(first_sibling) = d.siblings.first() {
                        let mut bad = d.clone();
                        bad.siblings[0] = h(98);
                        assert_ne!(*first_sibling, h(98));
                        assert!(check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(), "a flipped sibling");
                        let mut bad = d.clone();
                        bad.siblings.pop();
                        assert!(check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(), "a short opening");
                    }
                    let mut bad = d.clone();
                    bad.siblings.push(h(97));
                    assert!(check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(), "a long opening");
                    assert!(check_pipeline_step_node_v1(&facts, 1, level, index, &d).is_err(), "another stage's root");
                    let mut bad = d.clone();
                    bad.stage_leaf_count += 1;
                    assert!(
                        check_pipeline_step_node_v1(&facts, 0, level, index, &bad).is_err(),
                        "another count: the stage root commits it"
                    );
                    if index + 1 < width {
                        assert!(check_pipeline_step_node_v1(&facts, 0, level, index + 1, &d).is_err(), "another node's answer");
                    }
                }
            }
            assert!(matches!(palw_pipeline_node_parts_v1(leaves, height + 1, 0), None), "above the root");
            assert!(palw_pipeline_node_parts_v1(leaves, 0, 0).is_none(), "a leaf is a PipelineStepLeaf");
        }
    }

    /// **Every leaf of every small tree is answered and reaches its stage root**; a tampered one never does.
    #[test]
    fn every_leaf_of_every_small_tree_is_answered_and_a_tampered_one_is_refused() {
        for n in COUNTS {
            let (binding, facts, trees) = gen_claim([4, n, 1]);
            let leaves = &trees[1];
            let indices: Vec<u64> = if n <= 40 { (0..n).collect() } else { vec![0, 1, n / 2, n - 2, n - 1] };
            for i in indices {
                let d = leaf_answer(&binding, leaves, 1, i);
                check_pipeline_step_leaf_v1(&facts, 1, i, &d).unwrap_or_else(|e| panic!("{n} leaves, leaf {i}: {e}"));
                let mut bad = d.clone();
                bad.lanes_le[0] ^= 1;
                assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "other lanes");
                let mut bad = d.clone();
                bad.coord.tile += 1;
                assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "another coordinate");
                let mut bad = d.clone();
                bad.coord.stage = 0;
                assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "a coordinate of another stage");
                let mut bad = d.clone();
                bad.lanes_le.push(0);
                assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "lanes that are not whole");
                if !d.siblings.is_empty() {
                    let mut bad = d.clone();
                    bad.siblings.pop();
                    assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "a short opening");
                }
                let mut bad = d.clone();
                bad.siblings.push(h(7));
                assert!(check_pipeline_step_leaf_v1(&facts, 1, i, &bad).is_err(), "a long opening");
                assert!(check_pipeline_step_leaf_v1(&facts, 0, i, &d).is_err(), "another stage's root");
                if i + 1 < n {
                    assert!(check_pipeline_step_leaf_v1(&facts, 1, i + 1, &d).is_err(), "another leaf's answer");
                }
                assert!(check_pipeline_step_leaf_v1(&facts, 1, n, &d).is_err(), "past the tree");
            }
        }
    }

    /// **The binding is the claim's own, or the answer is refused**: another execution root, another
    /// class, another kind, a stage root changed, no stages and more than sixteen.
    #[test]
    fn the_binding_must_be_the_claims_own() {
        let (binding, facts, trees) = gen_claim([9, 3, 2]);
        let good = leaf_answer(&binding, &trees[0], 0, 4);
        check_pipeline_step_leaf_v1(&facts, 0, 4, &good).expect("the claim's own binding");
        let refuse = |facts: &PalwPipelineClaimFactsV1, d: &PalwPipelineStepLeafDisclosureV1, what: &str| {
            assert!(check_pipeline_step_leaf_v1(facts, 0, 4, d).is_err(), "{what}");
        };
        refuse(&PalwPipelineClaimFactsV1 { execution_root: h(50), ..facts }, &good, "another execution root");
        refuse(&PalwPipelineClaimFactsV1 { class_id: h(51), ..facts }, &good, "another class");
        refuse(&PalwPipelineClaimFactsV1 { kind: PalwPipelineKindV1::Eval, ..facts }, &good, "another kind");
        let mut bad = good.clone();
        let PalwPipelineBindingV1::Gen(parts) = &mut bad.binding else { unreachable!() };
        parts.stage_roots[2] = h(52);
        refuse(&facts, &bad, "a stage root changed: the execution root moves");
        let mut bad = good.clone();
        let PalwPipelineBindingV1::Gen(parts) = &mut bad.binding else { unreachable!() };
        parts.generated.push(1);
        refuse(&facts, &bad, "the generated ids changed");
        let mut bad = good.clone();
        bad.version = 2;
        refuse(&facts, &bad, "another version");
        // Stage counts: none, and past sixteen (each with the claim's root recomputed over them).
        for roots in [Vec::new(), vec![h(1); PALW_PIPELINE_MAX_STAGES_V1 + 1]] {
            let PalwPipelineBindingV1::Gen(parts) = &binding else { unreachable!() };
            let odd = PalwPipelineBindingV1::Gen(PalwPipelineGenPartsV1 { stage_roots: roots, ..parts.clone() });
            let odd_facts = PalwPipelineClaimFactsV1 { execution_root: odd.execution_root(), ..facts };
            let mut d = good.clone();
            d.binding = odd;
            refuse(&odd_facts, &d, "a pipeline has one to sixteen stages");
        }
    }

    fn unit_leaf(stage: u8, index: u64) -> PalwDaUnitV1 {
        PalwDaUnitV1::PipelineStepLeaf { stage, index }
    }

    fn unit_node(stage: u8, level: u8, index: u64) -> PalwDaUnitV1 {
        PalwDaUnitV1::PipelineStepNode { stage, level, index }
    }

    fn out_of_range(binding: &PalwPipelineBindingV1, leaves: &[Hash64]) -> PalwPipelineOutOfRangeV1 {
        PalwPipelineOutOfRangeV1 {
            version: PALW_PIPELINE_DA_VERSION_V1,
            binding: binding.clone(),
            stage_tree: Some(PalwPipelineStageTreeV1 {
                leaf_count: leaves.len() as u64,
                merkle_root: palw_pipeline_merkle_root_v1(leaves),
            }),
        }
    }

    /// **A unit past the execution is proven so by the binding**; one inside it is not.
    #[test]
    fn a_unit_past_the_tree_is_proven_so_and_one_inside_it_is_not() {
        let (binding, facts, trees) = gen_claim([9, 1030, 2]);
        // Stage 0 holds 9 leaves (height 4); stage 1 holds 1,030 (height 11).
        let proof = |stage: usize| out_of_range(&binding, &trees[stage]);
        let ok = |unit: PalwDaUnitV1, d: &PalwPipelineOutOfRangeV1| check_pipeline_out_of_range_v1(&facts, &unit, d);
        // Leaves.
        ok(unit_leaf(0, 9), &proof(0)).expect("a leaf at the count");
        ok(unit_leaf(0, 1 << 39), &proof(0)).expect("a leaf far past it");
        assert!(ok(unit_leaf(0, 8), &proof(0)).is_err(), "the last leaf is in the tree");
        assert!(ok(unit_leaf(1, 1029), &proof(1)).is_err(), "the last leaf of 1,030");
        ok(unit_leaf(1, 1030), &proof(1)).expect("a leaf at 1,030");
        // Nodes: above the root, and past the level's width.
        ok(unit_node(0, 5, 0), &proof(0)).expect("above the root of 9 leaves (height 4)");
        assert!(ok(unit_node(0, 4, 0), &proof(0)).is_err(), "the root is in the tree");
        ok(unit_node(0, 4, 1), &proof(0)).expect("past the root level's width");
        ok(unit_node(0, 1, 5), &proof(0)).expect("past level 1's width of 5");
        assert!(ok(unit_node(0, 1, 4), &proof(0)).is_err(), "the promoted last node is in the tree");
        // A stage past the claim's stages: the binding alone.
        let none = PalwPipelineOutOfRangeV1 { stage_tree: None, ..proof(0) };
        ok(unit_leaf(3, 0), &none).expect("the claim has three stages");
        ok(unit_node(15, 1, 0), &none).expect("a stage past them");
        assert!(ok(unit_leaf(2, 5), &none).is_err(), "a stage of the claim needs its tree");
        assert!(ok(unit_leaf(3, 0), &proof(0)).is_err(), "a stage past the stages carries no tree");
        // A wrong tree: another count or another Merkle root does not hash to the stage root.
        let mut bad = proof(0);
        bad.stage_tree = Some(PalwPipelineStageTreeV1 { leaf_count: 8, ..bad.stage_tree.unwrap() });
        assert!(ok(unit_leaf(0, 8), &bad).is_err(), "a count the stage root does not commit");
        let mut bad = proof(0);
        bad.stage_tree = Some(PalwPipelineStageTreeV1 { merkle_root: h(60), ..bad.stage_tree.unwrap() });
        assert!(ok(unit_leaf(0, 9), &bad).is_err(), "a Merkle root the stage root does not commit");
        // Another stage's tree for this stage's unit.
        assert!(ok(unit_leaf(0, 9), &proof(1)).is_err(), "the tree of another stage");
        // An empty stage: every unit is past it.
        let (binding, facts, trees) = gen_claim([4, 0, 3]);
        let empty = out_of_range(&binding, &trees[1]);
        check_pipeline_out_of_range_v1(&facts, &unit_leaf(1, 0), &empty).expect("an empty stage holds no leaf");
        check_pipeline_out_of_range_v1(&facts, &unit_node(1, 1, 0), &empty).expect("nor a node");
        // Units that are not pipeline units, and a wrong kind of claim.
        assert!(ok(PalwDaUnitV1::Event { row: 0, tile: 0 }, &proof(0)).is_err());
        assert!(
            check_pipeline_out_of_range_v1(
                &PalwPipelineClaimFactsV1 { kind: PalwPipelineKindV1::Eval, ..facts },
                &unit_leaf(0, 9),
                &proof(0)
            )
            .is_err()
        );
    }

    /// **The door**: a stage below sixteen, a leaf below 2^40, an interior node of the widest tree.
    #[test]
    fn the_door_admits_what_some_execution_could_hold() {
        let ok = |unit| palw_pipeline_step_unit_is_admissible_v1(&unit);
        ok(unit_leaf(0, 0)).expect("leaf 0");
        ok(unit_leaf(15, (1 << 40) - 1)).expect("the widest ladder's last leaf of the last stage");
        assert!(ok(unit_leaf(16, 0)).is_err(), "a seventeenth stage");
        assert!(ok(unit_leaf(0, 1 << 40)).is_err(), "a leaf past 2^40");
        ok(unit_node(0, 1, 0)).expect("level 1");
        ok(unit_node(3, 40, 0)).expect("the widest root");
        assert!(ok(unit_node(0, 0, 0)).is_err(), "a leaf named as a node");
        assert!(ok(unit_node(0, 41, 0)).is_err(), "above the widest tree");
        assert!(ok(unit_node(0, 40, 1)).is_err(), "past the root's width");
        ok(unit_node(0, 20, (1 << 20) - 1)).expect("the last node of level 20");
        assert!(ok(unit_node(0, 20, 1 << 20)).is_err(), "past level 20's width");
        assert!(ok(unit_node(16, 1, 0)).is_err(), "a seventeenth stage's node");
        assert!(ok(PalwDaUnitV1::Event { row: 0, tile: 0 }).is_err(), "an event unit");
    }

    /// **A seat reaches a leaf of a tree of up to 2^30 leaves in at most four sessions**: ten levels a
    /// session, then the leaf — by the frontier rule alone (no tree is built), always down the last node of
    /// each frontier, whose span is the rightmost — and the widest frontier is 1,024 hashes.
    #[test]
    fn a_descent_is_ten_levels_a_session() {
        let walk = |count: u64| -> (u32, u64) {
            let (mut level, mut index) = (palw_pipeline_tree_height_v1(count), 0u64);
            let (mut sessions, mut widest) = (0u32, 0u64);
            while level > 0 {
                let (below, first, end) = palw_pipeline_node_frontier_v1(count, level, index).expect("a node");
                sessions += 1;
                widest = widest.max(end - first);
                (level, index) = (below, end - 1);
            }
            (sessions + 1, widest)
        };
        for count in [1u64 << 30, (1 << 30) - 1, (1 << 29) + 17, 450_000_000, 112_495_104, 1 << 20, 1 << 10, 1000, 5] {
            let (sessions, widest) = walk(count);
            assert!(sessions <= 4, "{count} leaves: {sessions} sessions");
            assert!(widest <= 1 << PALW_PIPELINE_STEP_NODE_DEPTH_V1, "{count} leaves: a frontier of {widest}");
        }
        assert_eq!(walk(1 << 30), (4, 1 << 10), "thirty levels: three node sessions and the leaf");
        assert_eq!(walk(1 << 10), (2, 1 << 10), "ten levels: one node session and the leaf");
        assert_eq!(walk((1 << 30) + 1).0, 5, "a thirty-first level is a fifth session");
    }

    /// **The demand's message** separates by domain, network, claim, unit and accuser.
    #[test]
    fn the_accusation_message_binds_everything_it_names() {
        let bond = |n: u32| PalwBondKeyV2(crate::config::premine::premine_outpoint(n));
        let base = palw_pipeline_step_accusation_message_v1(h(1), &h(2), &unit_leaf(1, 5), &bond(1));
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(9), &h(2), &unit_leaf(1, 5), &bond(1)), "network");
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(1), &h(9), &unit_leaf(1, 5), &bond(1)), "claim");
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(1), &h(2), &unit_leaf(1, 6), &bond(1)), "index");
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(1), &h(2), &unit_leaf(2, 5), &bond(1)), "stage");
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(1), &h(2), &unit_node(1, 1, 5), &bond(1)), "kind of unit");
        assert_ne!(base, palw_pipeline_step_accusation_message_v1(h(1), &h(2), &unit_leaf(1, 5), &bond(2)), "accuser");
        assert_ne!(PALW_PIPELINE_ACCUSATION_DOMAIN_V1, crate::palw_da_rcore_v1::PALW_TIR_STEP_ACCUSATION_DOMAIN_V1);
        assert_ne!(PALW_PIPELINE_ACCUSATION_MLDSA87_CONTEXT_V1, crate::palw_da_rcore_v1::PALW_TIR_STEP_ACCUSATION_MLDSA87_CONTEXT_V1);
    }
}
