//! **Lane LG14-B: the legacy V2 route's public localizer and the producer's responder** (node policy; consensus-inert). Design:
//! `docs/design/palw/legacy-route-g14-held-da.md`; the rules are [`kaspa_consensus_core::palw_legacy_held_da_v2`]'s.
//!
//! **The verifier's half** — a bonded verifier OUTSIDE the claim's Panel, with its own copy of the registered model (ADR-0177: G14 is
//! conditional on it), reading only the chain:
//!
//! 1. its OWN honest replica of the claim's job, kept as a FOLD ([`PalwLegacyTreeV2::fold_v1`]: the retained level and one replayed
//!    block at a time) — never the whole capture, so the whole-capture cap ([C12] gap (a)) is never asked;
//! 2. the claim's binding, from the answer to a row-0 event demand (public, on chain);
//! 3. the descent ([`kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_descent_next_v2`]): each answered frontier (tag 158,
//!    authenticated by the fold before it landed) against its own nodes — never a re-derivation of the producer's served fold
//!    (gap (b));
//! 4. the terminal at the first divergent leaf: a non-fused step by the leaf recompute (tag 159, [`palw_legacy_leaf_recompute_v2`]
//!    — every opening built from its own tree and the frontiers, the model rows from its own copy); a fused-attention step by the
//!    committed witness (CKW, gap (c)) and then `ShardCourtAccused` → the held dissection ([`palw_legacy_fused_opening_v2`]).
//!
//! **The producer's half** — the answers an honest producer owes from its retention ([`palw_legacy_held_answer_v2`]): a node's
//! frontier and siblings from its fold (the retained level, a replayed block below it), a leaf's committed half from its own leaf
//! prover. The node's DA worker (`palw_panel::rcore_da_answers_v1`) now dispatches due legacy units to this responder under its
//! verified-material and memory-reservation path, then queues signed tag-158 carriers. LG14-A's common filer now owns an independent
//! [`PalwLegacyReplicaV2`] for base0-codec replays, reads authenticated public tag-158 history and queues these step terminals. Other
//! mismatch classes and fresh-node service completion remain separate acceptance conditions.

use kaspa_consensus_core::palw_backend::{PalwClaimRootsV1, PalwExecutionBackendV1};
use kaspa_consensus_core::palw_legacy_held_da_v2::{
    PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_VERSION_V2,
    PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2, PalwCommittedKernelWitnessV2, PalwLegacyHeldAnswerCarriageV2,
    PalwLegacyHeldAnswerV2, PalwLegacyHeldDemandV2, PalwLegacyHeldUnitV2, PalwLegacyLeafRecomputeV2, PalwLegacyNodeViewV2,
    PalwLegacyOwnTreeV2, palw_legacy_held_answer_message_v2, palw_legacy_held_demand_message_v2,
    palw_legacy_leaf_recompute_message_v2, palw_legacy_node_answer_from_oracle_v2, palw_legacy_recompute_placeholder_v2,
};
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use kaspa_consensus_core::palw_shard_court_v1::PalwLeafEvidenceV1;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2};
use kaspa_consensus_core::palw_step_leg::{PalwStepBindingV2, PalwStepOpeningV1, step_merkle_leaf_v1, step_merkle_node_v1};
use kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_width_v1;
use kaspa_hashes::Hash64;
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::sync::Arc;

/// How many replayed blocks a fold tree keeps at once (the descent and an opening read at most two edges at a time).
const PALW_LEGACY_TREE_BLOCK_CACHE_V2: usize = 4;

/// **A committed tree, read node by node** — the verifier's own replica, or an honest producer's retention.
pub enum PalwLegacyTreeV2<'a> {
    /// Every leaf hash in hand (a checkpoint tree, a small claim, a test's): levels folded once.
    Leaves { levels: Vec<Vec<Hash64>>, leaf_hashes: Vec<Hash64> },
    /// A base0 FOLD: the retained level and above in hand, a block below it replayed from the retention's own checkpoints
    /// (`held_step_range_answer_v1`) when a node there is read — the resident set is the retained vector and the cached blocks.
    Fold {
        backend: &'a dyn PalwExecutionBackendV1,
        capture: &'a [u8],
        prompt_ids: &'a [u32],
        leaf_count: u64,
        retain_level: u8,
        /// Levels `retain_level ..= height`, index 0 the retained level.
        upper: Vec<Vec<Hash64>>,
        blocks: RefCell<BTreeMap<u64, Vec<Hash64>>>,
        /// Blocks replayed so far, and the most leaf hashes held at once (the resource report).
        replays: Cell<u64>,
        peak_leaves: Cell<u64>,
    },
}

/// The tree's own fold of one level into the next: pairs, an odd last node promoted.
fn fold_level(level: &[Hash64]) -> Vec<Hash64> {
    let mut next = Vec::with_capacity(level.len().div_ceil(2));
    let mut pairs = level.chunks_exact(2);
    for pair in &mut pairs {
        next.push(step_merkle_node_v1(&pair[0], &pair[1]));
    }
    if let [odd] = pairs.remainder() {
        next.push(*odd);
    }
    next
}

impl<'a> PalwLegacyTreeV2<'a> {
    /// A tree over every leaf hash.
    pub fn leaves_v1(leaf_hashes: Vec<Hash64>) -> Self {
        let mut levels = vec![leaf_hashes.iter().enumerate().map(|(i, h)| step_merkle_leaf_v1(i as u64, h)).collect::<Vec<_>>()];
        while levels.last().is_some_and(|l| l.len() > 1) {
            let next = fold_level(levels.last().expect("a level"));
            levels.push(next);
        }
        Self::Leaves { levels, leaf_hashes }
    }

    /// **A tree over a base0 fold retention** (`MSKFPMV2`): its retained nodes folded up once; below them, a block replayed on
    /// demand through `backend` (this node's own instance — for the verifier, an honest replica of the claim's job).
    pub fn fold_v1(backend: &'a dyn PalwExecutionBackendV1, capture: &'a [u8], prompt_ids: &'a [u32]) -> Result<Self, String> {
        let material = misaka_palw_base0::produce::base0_fp_material_decode_v2(capture)
            .map_err(|e| format!("the retention is not a fold: {e:?}"))?;
        Self::from_fold_v1(backend, capture, prompt_ids, &material.step_tree)
    }

    fn from_fold_v1(
        backend: &'a dyn PalwExecutionBackendV1,
        capture: &'a [u8],
        prompt_ids: &'a [u32],
        tree: &misaka_palw_base0::fp_capture::Base0SparseStepTreeV1,
    ) -> Result<Self, String> {
        let retain_level = u8::try_from(tree.retain_level()).map_err(|_| "the retained level is past any tree".to_string())?;
        let mut upper = vec![tree.retained_nodes().to_vec()];
        while upper.last().is_some_and(|l| l.len() > 1) {
            let next = fold_level(upper.last().expect("a level"));
            upper.push(next);
        }
        Ok(Self::Fold {
            backend,
            capture,
            prompt_ids,
            leaf_count: tree.leaf_count(),
            retain_level,
            upper,
            blocks: RefCell::new(BTreeMap::new()),
            replays: Cell::new(0),
            peak_leaves: Cell::new(0),
        })
    }

    pub fn leaf_count(&self) -> u64 {
        match self {
            Self::Leaves { leaf_hashes, .. } => leaf_hashes.len() as u64,
            Self::Fold { leaf_count, .. } => *leaf_count,
        }
    }

    /// The root.
    pub fn root(&self) -> Option<Hash64> {
        let height = kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_tree_height_v1(self.leaf_count());
        self.own_node(height, 0)
    }

    /// Blocks replayed and the most leaf hashes held at once (a fold); `(0, n)` for a whole tree.
    pub fn resources(&self) -> (u64, u64) {
        match self {
            Self::Leaves { leaf_hashes, .. } => (0, leaf_hashes.len() as u64),
            Self::Fold { replays, peak_leaves, upper, .. } => {
                (replays.get(), peak_leaves.get() + upper.iter().map(|l| l.len() as u64).sum::<u64>())
            }
        }
    }

    /// The leaf hashes of block `block` of a fold (`2^retain_level` leaves, the last one short), replayed once and cached.
    fn block(&self, block: u64) -> Option<Vec<Hash64>> {
        let Self::Fold { backend, capture, prompt_ids, leaf_count, retain_level, blocks, replays, peak_leaves, .. } = self else {
            return None;
        };
        if let Some(leaves) = blocks.borrow().get(&block) {
            return Some(leaves.clone());
        }
        let first = block.checked_shl(u32::from(*retain_level))?;
        let count = (1u64 << *retain_level).min(leaf_count.checked_sub(first)?);
        let opening = backend.held_step_range_answer_v1(capture, prompt_ids, first, u32::try_from(count).ok()?).ok()?;
        if opening.leaf_hashes.len() as u64 != count {
            return None;
        }
        replays.set(replays.get() + 1);
        let mut cache = blocks.borrow_mut();
        while cache.len() >= PALW_LEGACY_TREE_BLOCK_CACHE_V2 {
            let first_key = *cache.keys().next().expect("non-empty");
            cache.remove(&first_key);
        }
        cache.insert(block, opening.leaf_hashes.clone());
        peak_leaves.set(peak_leaves.get().max(cache.values().map(|l| l.len() as u64).sum()));
        Some(opening.leaf_hashes)
    }

    /// The committed leaf hash of leaf `i`.
    pub fn leaf_hash(&self, i: u64) -> Option<Hash64> {
        match self {
            Self::Leaves { leaf_hashes, .. } => leaf_hashes.get(usize::try_from(i).ok()?).copied(),
            Self::Fold { retain_level, .. } => {
                let block = i >> retain_level;
                self.block(block)?.get(usize::try_from(i - (block << retain_level)).ok()?).copied()
            }
        }
    }
}

impl PalwLegacyOwnTreeV2 for PalwLegacyTreeV2<'_> {
    fn own_node(&self, level: u8, index: u64) -> Option<Hash64> {
        match self {
            Self::Leaves { levels, .. } => levels.get(level as usize)?.get(usize::try_from(index).ok()?).copied(),
            Self::Fold { leaf_count, retain_level, upper, .. } => {
                if level >= *retain_level {
                    return upper.get(usize::from(level - *retain_level))?.get(usize::try_from(index).ok()?).copied();
                }
                // Below the retained level: fold the node's covered leaves inside its block, by the tree's own rule (an odd last
                // node is promoted only at the level's global edge, which inside an aligned block is the block's own last node).
                let shift = u32::from(*retain_level - level);
                let block = index >> shift;
                let leaves = self.block(block)?;
                let block_first = block << *retain_level;
                let mut nodes: Vec<Hash64> =
                    leaves.iter().enumerate().map(|(k, h)| step_merkle_leaf_v1(block_first + k as u64, h)).collect();
                let mut start = block_first;
                for l in 0..level {
                    let width = palw_tir_step_tree_width_v1(*leaf_count, l)?;
                    let mut next = Vec::with_capacity(nodes.len().div_ceil(2));
                    let mut k = 0usize;
                    while k < nodes.len() {
                        let position = start + k as u64;
                        if position + 1 < width && k + 1 < nodes.len() {
                            next.push(step_merkle_node_v1(&nodes[k], &nodes[k + 1]));
                            k += 2;
                        } else {
                            next.push(nodes[k]);
                            k += 1;
                        }
                    }
                    nodes = next;
                    start /= 2;
                }
                nodes.get(usize::try_from(index.checked_sub(start)?).ok()?).copied()
            }
        }
    }
}

/// The public filer's own execution, held under its replay reservation. It owns no producer material. Borrowed tree readers
/// live inside each blocking step, so their block cache serves the whole descent without a self-referential or shared Cell tree.
pub(crate) struct PalwLegacyReplicaV2 {
    pub(crate) backend: Arc<dyn PalwExecutionBackendV1>,
    pub(crate) capture: Arc<Vec<u8>>,
    pub(crate) prompt_ids: Arc<Vec<u32>>,
    pub(crate) form: PalwPromptIdsFormV1,
    pub(crate) roots: PalwClaimRootsV1,
}

impl PalwLegacyReplicaV2 {
    fn with_step_tree<T>(&self, use_tree: impl FnOnce(&PalwLegacyTreeV2<'_>) -> Result<T, String>) -> Result<T, String> {
        use misaka_palw_base0::produce::{Base0RetentionV1, base0_dense_step_leaves_capped_v1, base0_material_decode_any_v1};
        let retention = base0_material_decode_any_v1(&self.capture).map_err(|e| format!("this replica has no legacy tree: {e:?}"))?;
        let binding = retention.binding();
        if binding.committed_execution_root != self.roots.execution_root || binding.full_logits_trace_root != self.roots.trace_root {
            return Err("the replica's capture is not its own execution".into());
        }
        let tree = match &retention {
            Base0RetentionV1::Folded(m) => {
                PalwLegacyTreeV2::from_fold_v1(self.backend.as_ref(), &self.capture, &self.prompt_ids, &m.step_tree)?
            }
            Base0RetentionV1::Dense((binding, tiles, ..)) => PalwLegacyTreeV2::leaves_v1(
                base0_dense_step_leaves_capped_v1(binding, tiles, binding.step_leaf_count)
                    .ok_or("the replica has no complete dense step tree")?,
            ),
        };
        if tree.leaf_count() != binding.step_leaf_count || tree.root() != Some(binding.step_merkle_root) {
            return Err("the replica's own tree does not reproduce its binding".into());
        }
        use_tree(&tree)
    }

    pub(crate) fn descent(
        &self,
        binding: &PalwStepBindingV2,
        frontiers: &[kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyFrontierV2],
    ) -> Result<kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyDescentStepV2, String> {
        self.with_step_tree(|tree| {
            if tree.leaf_count() != binding.step_leaf_count {
                return Err("the claim's step count is not this public job's: the job/count terminal is required".into());
            }
            Ok(kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_descent_next_v2(
                kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyTreeV2::Step,
                binding.step_leaf_count,
                &binding.step_merkle_root,
                tree,
                frontiers,
            ))
        })
    }

    /// Build an unsigned terminal only after the court's own predicate says it is guilty or needs the fused dissection.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn terminal(
        &self,
        binding: &PalwStepBindingV2,
        frontiers: &[kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyFrontierV2],
        witness: Option<&PalwCommittedKernelWitnessV2>,
        leaf: u64,
        claim: Hash64,
        bound_to: &kaspa_consensus_core::palw_shard_court_v1::PalwOneMoveClaimV2,
        trace_root: Hash64,
        producer: PalwBondKeyV2,
        accuser: PalwBondKeyV2,
        ladder: u64,
    ) -> Result<PalwConsensusObjectV2, String> {
        use kaspa_consensus_core::palw_legacy_held_da_v2::{palw_legacy_leaf_is_fused_v2, palw_legacy_leaf_recompute_verdict_v2};
        use kaspa_consensus_core::palw_shard_court_v1::PalwShardCourtVerdictV1;
        if palw_legacy_leaf_is_fused_v2(binding, leaf) {
            let witness = witness.ok_or("the fused terminal needs the public committed witness")?;
            if witness.refutation.binding != *binding || witness.refutation.output_opening.leaf_index != leaf {
                return Err("the public witness is not the located leaf of this claim".into());
            }
            let evidence = palw_legacy_fused_opening_v2(
                self.backend.as_ref(),
                &self.capture,
                &self.prompt_ids,
                witness,
                bound_to.artifact_root,
                ladder,
            )?
            .ok_or("the own history agrees with the committed fused tile: nothing guilty to file")?;
            match evidence.verdict_at_v2(bound_to, ladder, true).map_err(|e| format!("the fused terminal does not adjudicate: {e}"))? {
                PalwShardCourtVerdictV1::ExecutorGuilty | PalwShardCourtVerdictV1::NeedsDissection => {}
                _ => return Err("the court does not find this fused accusation actionable".into()),
            }
            let accusation = evidence.into_accusation_v1(claim, bound_to.execution_root, trace_root, producer, accuser);
            accusation.validate_shape(ladder).map_err(|e| format!("the fused accusation's shape: {e}"))?;
            return Ok(PalwConsensusObjectV2::ShardCourtAccused { accusation: Box::new(accusation) });
        }
        self.with_step_tree(|tree| {
            let committed = frontiers
                .iter()
                .find_map(|frontier| frontier.leaf_hash(leaf))
                .ok_or("no public bottom frontier commits the located leaf")?;
            let view = PalwLegacyNodeViewV2 { leaf_count: binding.step_leaf_count, divergent: leaf, frontiers, own: tree };
            let accusation = palw_legacy_leaf_recompute_v2(
                self.backend.as_ref(),
                &self.capture,
                &self.prompt_ids,
                self.roots,
                self.form,
                binding,
                &view,
                committed,
                leaf,
                claim,
                trace_root,
                producer,
                accuser,
            )?;
            match palw_legacy_leaf_recompute_verdict_v2(&accusation, bound_to, ladder)
                .map_err(|e| format!("the recompute terminal does not adjudicate: {e}"))?
            {
                PalwShardCourtVerdictV1::ExecutorGuilty => {
                    Ok(PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation: Box::new(accusation) })
                }
                _ => Err("the court does not find the located leaf guilty: nothing filed".into()),
            }
        })
    }
}

// ---- the producer's half -----------------------------------------------------------------------------------------------------

/// Build a tag-158 answer from a verified family capture on the node's DA worker. All base0-codec families share this path:
/// folded captures keep the retained level and replay blocks on demand, while dense captures rebuild their own committed tree.
/// The public answer is checked with the consensus predicate before it is signed; unsupported codecs are explicit refusals.
pub fn palw_legacy_held_capture_answer_v2(
    backend: &dyn PalwExecutionBackendV1,
    capture: &[u8],
    prompt_ids: &[u32],
    roots: PalwClaimRootsV1,
    form: PalwPromptIdsFormV1,
    unit: &PalwLegacyHeldUnitV2,
) -> Result<(PalwStepBindingV2, PalwLegacyHeldAnswerV2), String> {
    use misaka_palw_base0::produce::{Base0RetentionV1, base0_dense_step_leaves_capped_v1, base0_material_decode_any_v1};
    let retention = base0_material_decode_any_v1(capture).map_err(|e| format!("no legacy held responder for this capture: {e:?}"))?;
    let binding = retention.binding();
    if binding.committed_execution_root != roots.execution_root || binding.full_logits_trace_root != roots.trace_root {
        return Err("the legacy held capture is not the claim's binding".into());
    }
    let (context_hash, _, checkpoint_profile_hash) = kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding)
        .map_err(|e| format!("the legacy held binding does not verify: {e}"))?;
    kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_demand_v2(&roots.execution_root, unit, binding)
        .map_err(|e| format!("the legacy held unit cannot be compelled: {e}"))?;
    let step_tree = match &retention {
        Base0RetentionV1::Folded(m) => PalwLegacyTreeV2::from_fold_v1(backend, capture, prompt_ids, &m.step_tree)?,
        Base0RetentionV1::Dense((binding, tiles, ..)) => PalwLegacyTreeV2::leaves_v1(
            base0_dense_step_leaves_capped_v1(binding, tiles, binding.step_leaf_count)
                .ok_or("the dense legacy held capture has no complete step tree")?,
        ),
    };
    if step_tree.leaf_count() != binding.step_leaf_count || step_tree.root() != Some(binding.step_merkle_root) {
        return Err("the legacy held retention does not reproduce the claim's step tree".into());
    }
    // Other units do not read the checkpoint tree. Avoid rebuilding a dense capture's state chunks for every step-node demand.
    let checkpoint_hashes = if matches!(unit, PalwLegacyHeldUnitV2::CheckpointNode { .. }) {
        match &retention {
            Base0RetentionV1::Folded(m) => m
                .checkpoint_leaves
                .iter()
                .map(|leaf| {
                    kaspa_consensus_core::palw_step_leg::checkpoint_leaf_hash_v2(
                        &context_hash,
                        &checkpoint_profile_hash,
                        &binding.state_chunk_map_id,
                        leaf,
                    )
                })
                .collect(),
            Base0RetentionV1::Dense((_, _, _, _, chunks)) => {
                misaka_palw_base0::legs::Base0CheckpointCaptureV1::from_chunks_v1(
                    &binding.job_context,
                    &binding.shape_profile,
                    &binding.checkpoint_profile,
                    chunks,
                )
                .map_err(|e| format!("the dense checkpoint leg does not rebuild: {e:?}"))?
                .leaf_hashes
            }
        }
    } else {
        Vec::new()
    };
    let checkpoint_tree = PalwLegacyTreeV2::leaves_v1(checkpoint_hashes);
    if matches!(unit, PalwLegacyHeldUnitV2::CheckpointNode { .. })
        && (checkpoint_tree.leaf_count() != u64::from(binding.checkpoint_count)
            || checkpoint_tree.root() != Some(binding.checkpoint_merkle_root))
    {
        return Err("the legacy held retention does not reproduce the claim's checkpoint tree".into());
    }
    let answer = palw_legacy_held_answer_v2(unit, &step_tree, &checkpoint_tree, backend, capture, prompt_ids, roots, form)?;
    kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v2(
        &roots.execution_root,
        unit,
        binding,
        &answer,
        binding.step_leaf_count,
    )
    .map_err(|e| format!("the built legacy held answer does not authenticate: {e}"))?;
    Ok((binding.clone(), answer))
}

/// **A producer's answer to one legacy held unit, from its own retention** — a node's frontier (leaf hashes at level 0) and its
/// siblings from `tree` (the step tree, or the checkpoint tree for a `CheckpointNode`), or a leaf's committed half from the family's
/// own leaf prover over `capture` (`fp_leaf_refutation_v1`), stripped of the history at a fused site and put in the network's
/// prompt carriage — and NEVER with artifact rows (the verifier supplies those from its own copy, ADR-0177 D2).
#[allow(clippy::too_many_arguments)]
pub fn palw_legacy_held_answer_v2(
    unit: &PalwLegacyHeldUnitV2,
    step_tree: &PalwLegacyTreeV2<'_>,
    checkpoint_tree: &PalwLegacyTreeV2<'_>,
    backend: &dyn PalwExecutionBackendV1,
    capture: &[u8],
    prompt_ids: &[u32],
    roots: PalwClaimRootsV1,
    form: PalwPromptIdsFormV1,
) -> Result<PalwLegacyHeldAnswerV2, String> {
    fn node_answer(tree: &PalwLegacyTreeV2<'_>, level: u8, index: u64) -> Result<PalwLegacyHeldAnswerV2, String> {
        palw_legacy_node_answer_from_oracle_v2(tree.leaf_count(), level, index, &|l, i| tree.own_node(l, i), &|i| tree.leaf_hash(i))
            .ok_or_else(|| format!("node ({level}, {index}) is not in this retention's tree"))
    }
    match *unit {
        PalwLegacyHeldUnitV2::StepNode { level, index } => node_answer(step_tree, level, index),
        PalwLegacyHeldUnitV2::CheckpointNode { level, index } => node_answer(checkpoint_tree, level, index),
        PalwLegacyHeldUnitV2::KernelWitness { leaf } => {
            let work_leaves = step_tree.leaf_count();
            // The family's own leaf prover first (a fold builds it from the tree and the blocks the step reads), the whole-capture
            // prover after it — `palw_leaf_evidence_from_capture_v1`'s order.
            let refutation = match backend.fp_leaf_refutation_v1(
                capture,
                prompt_ids,
                PalwClaimRootsV1 { output_root: None, ..roots },
                work_leaves,
                leaf,
            ) {
                Ok(refutation) => refutation,
                Err(_) => backend.refutation_for_free_prompt_index(capture, leaf, prompt_ids)?,
            };
            let refutation = kaspa_consensus_core::palw_shard_court_v1::palw_one_move_refutation_v2(refutation);
            let (refutation, prompt_ids_opening) =
                kaspa_consensus_core::palw_step_refute::palw_refutation_prompt_carriage_v1(form, refutation)
                    .map_err(|e| format!("the prompt tile does not open: {e}"))?;
            Ok(PalwLegacyHeldAnswerV2::KernelWitness(Box::new(PalwCommittedKernelWitnessV2 { refutation, prompt_ids_opening })))
        }
    }
}

// ---- the objects -------------------------------------------------------------------------------------------------------------

/// **Tag 157, signed** by `accuser` over its message under the demand context.
pub fn palw_legacy_held_demand_object_v2(
    network_domain: &Hash64,
    claim: Hash64,
    unit: PalwLegacyHeldUnitV2,
    accuser: PalwBondKeyV2,
    binding: PalwStepBindingV2,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    let mut demand =
        PalwLegacyHeldDemandV2 { version: PALW_LEGACY_HELD_VERSION_V2, claim, unit, accuser, binding, signature: Vec::new() };
    let message = palw_legacy_held_demand_message_v2(network_domain.as_byte_slice(), &demand);
    demand.signature = sign(message.as_byte_slice(), PALW_LEGACY_HELD_DEMAND_MLDSA87_CONTEXT_V2)
        .filter(|s| !s.is_empty())
        .ok_or("no signing key for the demand")?;
    Ok(PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand: Box::new(demand) })
}

/// **Tag 158, signed** by `discloser` over its message under the answer context.
pub fn palw_legacy_held_answer_object_v2(
    network_domain: &Hash64,
    claim: Hash64,
    unit: PalwLegacyHeldUnitV2,
    binding: PalwStepBindingV2,
    answer: PalwLegacyHeldAnswerV2,
    discloser: PalwBondKeyV2,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    let mut carriage = PalwLegacyHeldAnswerCarriageV2 {
        version: PALW_LEGACY_HELD_VERSION_V2,
        claim,
        unit,
        binding,
        answer,
        discloser,
        signature: Vec::new(),
    };
    let message = palw_legacy_held_answer_message_v2(network_domain.as_byte_slice(), &carriage);
    carriage.signature = sign(message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2)
        .filter(|s| !s.is_empty())
        .ok_or("no signing key for the answer")?;
    Ok(PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer: Box::new(carriage) })
}

/// **Tag 159, signed** by its accuser over its message under the recompute context.
pub fn palw_legacy_leaf_recompute_object_v2(
    network_domain: &Hash64,
    mut accusation: PalwLegacyLeafRecomputeV2,
    sign: impl FnOnce(&[u8], &[u8]) -> Option<Vec<u8>>,
) -> Result<PalwConsensusObjectV2, String> {
    let message = palw_legacy_leaf_recompute_message_v2(network_domain.as_byte_slice(), &accusation);
    accusation.signature = sign(message.as_byte_slice(), PALW_LEGACY_LEAF_RECOMPUTE_MLDSA87_CONTEXT_V2)
        .filter(|s| !s.is_empty())
        .ok_or("no signing key for the recompute")?;
    Ok(PalwConsensusObjectV2::LegacyLeafRecomputedV2 { accusation: Box::new(accusation) })
}

// ---- the verifier's terminals ------------------------------------------------------------------------------------------------

/// **The verifier's leaf recompute at a non-fused first divergent leaf** (tag 159), from public material and its own copy:
///
/// * its OWN honest evidence at the leaf (`palw_leaf_evidence_from_capture_v1` over its own replica: the canonical input rows — at
///   a first divergent leaf every input is a leaf before it, so its own values ARE the committed ones — the id carriages, and the
///   artifact rows from its own copy of the registered model);
/// * re-bound to the CLAIM: the claim's binding (from the row-0 answer); the committed leaf HASH (the bottom round's) with its path
///   in the claim's tree; every input run re-opened in the claim's tree ([`PalwLegacyNodeViewV2`]); the output preimage EMPTY.
///
/// The caller asks the court's own verdict of it before filing (an own replica that is wrong files nothing).
#[allow(clippy::too_many_arguments)]
pub fn palw_legacy_leaf_recompute_v2(
    own_backend: &dyn PalwExecutionBackendV1,
    own_capture: &[u8],
    prompt_ids: &[u32],
    own_roots: PalwClaimRootsV1,
    form: PalwPromptIdsFormV1,
    claim_binding: &PalwStepBindingV2,
    view: &PalwLegacyNodeViewV2<'_>,
    committed_leaf_hash: Hash64,
    leaf: u64,
    claim: Hash64,
    trace_root: Hash64,
    executor_bond: PalwBondKeyV2,
    accuser_bond: PalwBondKeyV2,
) -> Result<PalwLegacyLeafRecomputeV2, String> {
    let evidence = kaspa_consensus_core::palw_leaf_evidence_v1::palw_leaf_evidence_from_capture_v1(
        own_backend,
        own_capture,
        prompt_ids,
        PalwClaimRootsV1 { output_root: None, ..own_roots },
        view.leaf_count,
        leaf,
        form,
    )?;
    let PalwLeafEvidenceV1 { mut refutation, artifact_openings, prompt_ids_opening } = evidence;
    if refutation.kv_checkpoint.is_some() && refutation.binding.checkpoint_merkle_root != claim_binding.checkpoint_merkle_root {
        return Err(
            "the step reads a checkpoint anchor and the claim's checkpoint leg is not this replica's: its state is localized first"
                .into(),
        );
    }
    refutation.binding = claim_binding.clone();
    refutation.output_opening = PalwStepOpeningV1 {
        leaf_index: leaf,
        leaf_hash: committed_leaf_hash,
        siblings: view.leaf_siblings(leaf).ok_or("the leaf's path is not reached by public material")?,
    };
    refutation.output_preimage = palw_legacy_recompute_placeholder_v2();
    let (profile, ctx) = (&claim_binding.shape_profile, &claim_binding.job_context);
    for row in refutation.inputs.iter_mut() {
        let indices = row
            .preimages
            .iter()
            .map(|p| kaspa_consensus_core::palw_step::canonical_step_leaf_index(profile, ctx, &p.coord))
            .collect::<Option<Vec<u64>>>()
            .ok_or("an input leaf has no canonical index")?;
        let mut runs: Vec<(usize, u64)> = Vec::new();
        for (k, index) in indices.iter().enumerate() {
            match runs.last_mut() {
                Some((start, len)) if indices[*start] + *len == *index => *len += 1,
                _ => runs.push((k, 1)),
            }
        }
        row.run_siblings = runs
            .iter()
            .map(|(start, len)| view.range_siblings(indices[*start], *len))
            .collect::<Option<Vec<_>>>()
            .ok_or("an input run is not reached by public material")?;
    }
    Ok(PalwLegacyLeafRecomputeV2 {
        version: PALW_LEGACY_HELD_VERSION_V2,
        claim,
        execution_root: claim_binding.committed_execution_root,
        trace_root,
        executor_bond,
        accuser_bond,
        leaf_index: leaf,
        refutation,
        artifact_openings,
        prompt_ids_opening,
        signature: Vec::new(),
    })
}

/// **What a verifier makes of a committed fused tile** (the CKW the chain holds): the one move's evidence for `ShardCourtAccused` —
/// the committed half as disclosed, the attention site's artifact rows from ITS OWN copy — once the court's own kernel, over its OWN
/// honest history (N1 on its replica), finalizes to a tile other than the committed one. `Ok(None)`: the committed tile IS the
/// attention of the history (an honest claim; or its own replica is the faulty one) — nothing is filed.
pub fn palw_legacy_fused_opening_v2(
    own_backend: &dyn PalwExecutionBackendV1,
    own_capture: &[u8],
    prompt_ids: &[u32],
    witness: &PalwCommittedKernelWitnessV2,
    artifact_root: Hash64,
    opening_cap: u64,
) -> Result<Option<PalwLeafEvidenceV1>, String> {
    let leaf = witness.refutation.output_opening.leaf_index;
    let own = own_backend.attn_site_evidence_held_v1(own_capture, leaf, Some(prompt_ids), None)?;
    let site = own.site_v1(artifact_root, false, opening_cap).map_err(|e| format!("this replica's held site: {e}"))?;
    let root = own.evidence.root_claim_v1(&site).map_err(|e| format!("this replica's honest root: {e}"))?;
    let honest = kaspa_consensus_core::palw_base0_a16::a16_attn_finalize_v1(&root.claim.v_acc, site.site.params.values);
    let committed: Vec<i32> =
        witness.refutation.output_preimage.values_le.chunks_exact(4).map(|q| i32::from_le_bytes([q[0], q[1], q[2], q[3]])).collect();
    if honest == committed {
        return Ok(None);
    }
    let artifact_openings = own_backend.operand_openings_for(&witness.refutation).unwrap_or_default();
    Ok(Some(PalwLeafEvidenceV1 {
        refutation: witness.refutation.clone(),
        artifact_openings,
        prompt_ids_opening: witness.prompt_ids_opening.clone(),
    }))
}
