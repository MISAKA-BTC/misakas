//! **RFC-0002 Phase F, step F5: the IR court** — one committed leaf of an IR execution,
//! adjudicated by evaluating exactly the elements it holds from exactly the units they read
//! (`docs/design/palw/tir/phase-f-integration.md` §2.6–§2.7).
//!
//! A legacy class's court is a table of hand-written kernels, one per node kind, each with its own
//! slicing. An IR class has one: the program's own cone, evaluated element by element with
//! [`misaka_palw_tir::demand::eval_demanded`] — which pulls each operand element through the
//! primitive's index map, so a vocabulary tile costs `tile × D` multiply-accumulates and a gated-delta
//! head replays only its own state — over a [`DemandSource`] that serves nothing but units the
//! refutation carries and the court has authenticated:
//!
//! * **step leaves** of the executor's own step tree (one range-proved row, ascending leaf index):
//!   other commit points of the position, the previous occurrence's carry-outs, the committed row
//!   of every history position, history tiles, and `Fixed`-state checkpoints;
//! * **artifact leaves** of the class's TIR inventory (F3's rule), against its `artifact_root`;
//! * the **prompt ids** (the whole list against a flat digest, or one opening per tile against a
//!   Merkle root) and the **generated ids** (pinned through the class's logits scheme).
//!
//! **Where each value comes from** is fixed, so the operand set is a function of the disputed leaf:
//!
//! * a `Fixed` state at the start of position `p` is zero at `p = 0`, the checkpoint leaf of `p − 1`
//!   when `p ≡ 0 (mod C)`, and otherwise its writer's output at `p − 1`, replayed (design §2.6);
//! * a history row appended at `r`, read at `p`, comes from the history tile holding it when that
//!   tile ended before `p` (`⌊r / h_tile⌋ · h_tile + h_tile − 1 < p`), and otherwise from the row's
//!   own commit point at `r` (design §2.6);
//! * a checkpoint leaf holds the state after its position (the writer's output there — a leaf when
//!   the writer is committed — or, for an unwritten instance, the value carried to it); a history
//!   tile holds the committed rows it concatenates.
//!
//! By the step space's invariant (design D7) every such unit is a leaf that PRECEDES the disputed
//! one, so the court never reads a leaf the bisection has not already agreed on; a request for one
//! that does not precede it is refused, never answered (and, as every refusal of a source is, it
//! fails the evaluation `Missing`, spec 04b §9.4).
//!
//! **The canonical operand set is exactly what the evaluation asked for.** Every request is
//! recorded; a unit asked for and not carried, or carried and not asked for, is
//! `InputSetNotCanonical` — refused, nobody slashed — so a refutation of one leaf is one object,
//! whoever builds it. [`build_tir_cone_refutation_v1`] builds it by running the same evaluation over
//! a builder's own retained units ([`PalwTirEvidenceStoreV1`]); that is the API a node's evidence
//! builder calls (Phase F step F9).
//!
//! **PALW-TIR-33.** Every lane of the disputed leaf and of every carried step leaf must lie inside
//! the interval the class's program PROVES for it (spec 04b §7): a commit point's node interval, a
//! written state's writer interval (zero for an unwritten one), a history tile's row interval. A lane
//! outside is a value no execution can produce — the executor committed it, so the executor is
//! convicted ([`PalwStepFaultV1::TirValueOutsideProvenInterval`]), whichever leaf was disputed. After
//! this check nothing the evaluation reads can leave its interval, so, by the soundness of the range
//! analysis, the evaluation cannot overflow; any evaluation error left is either a unit the
//! refutation did not carry (refused as non-canonical) or an interpreter defect (refused as
//! `Unadjudicable`, nobody slashed — the golden vectors and the second implementation exist to keep
//! that set empty).
//!
//! **The check order is normative** (the verdict and its evidence id must be the same on every
//! implementation): see [`check_tir_cone_refutation_v1`].

use std::collections::{BTreeMap, BTreeSet};

use crate::Hash64;
use crate::palw_artifact::{
    PalwArtifactMultiproofV1, PalwArtifactOpeningV1, palw_artifact_multiproof_borsh_len_v1, palw_artifact_multiproof_from_openings_v1,
    palw_artifact_multiproof_sibling_count_v1, palw_artifact_opening_path_len_v1, palw_artifact_operand_borsh_len_v1,
    verify_artifact_multiproof_v1,
};
use crate::palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, verify_prompt_ids_opening_v1};
use crate::palw_step::PalwStepCoordinateV1;
use crate::palw_step_leg::{
    PalwStepFaultV1, PalwStepOpeningV1, PalwStepRangeOpeningV1, PalwStepRefutationVerdictV1, PalwStepTileLeafV1,
    step_opening_root_capped_v1, step_range_opening_root_capped_v1, step_refutation_evidence_id, step_tile_leaf_hash_v1,
};
use crate::palw_step_refute::{
    PALW_DECODE_TOKEN_EVIDENCE_KIND, PALW_LOGITS_TILE_LANES, PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, PalwStepInputRowV1,
    PalwStepRefuteError, PalwTiledDecodePinV1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1, tiled_logits_outer_root_v1,
    tiled_logits_scheme_id_v1, tiled_tile_authenticate_v1,
};
use crate::palw_tir_artifact_v1::{
    PALW_TIR_ROW_PIECE_BYTES_V1, palw_tir_inventory_leaf_count_v1, palw_tir_param_instances_v1, palw_tir_tensor_bytes_v1,
};
use crate::palw_tir_step_v1::{
    PALW_TIR_STEP_BINDING_VERSION_V1, PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirJobShapeV1, PalwTirLeafKindV1, PalwTirLeafV1,
    PalwTirStepBindingV1, PalwTirStepSpaceV1, PalwTirVerifiedBindingV1, palw_tir_execution_root_v1,
};
use crate::palw_v2::PalwJobContextV2;
use misaka_palw_tir::demand::{
    DemandContext, DemandError, DemandLimits, DemandRequest, DemandSource, DemandTarget, StateSupply, eval_demanded, hist_row_node_v1,
    state_occurrence_v1, state_writer_v1,
};
use misaka_palw_tir::interval::{Interval, analyze_ranges};
use misaka_palw_tir::{DType, Prim, TirError, TirErrorKind, TirProgramV1, TirResult};

/// The §24.1 evidence kind of a conviction from the binding alone (the legacy shape arm's).
pub const PALW_TIR_EVIDENCE_KIND_BINDING: u8 = 0;
/// The evidence kind of a structural conviction on the disputed leaf (the legacy tile arm's).
pub const PALW_TIR_EVIDENCE_KIND_LEAF: u8 = 1;
/// The evidence kind of a cone conviction: a recomputation mismatch or a PALW-TIR-33 violation
/// (the legacy arithmetic arm's).
pub const PALW_TIR_EVIDENCE_KIND_CONE: u8 = 5;
/// The evidence kind of a logits-consistency conviction: a step tile and a trace tile of one row
/// that differ.
pub const PALW_TIR_EVIDENCE_KIND_LOGITS: u8 = 7;

/// What the court reads from the ruleset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwTirCourtRulesV1 {
    /// The ruleset's step-leaf ladder (the sibling caps of every opening).
    pub max_step_leaf_count: u64,
    /// The network's prompt-commitment form (ADR-0081 Decision 3).
    pub prompt_form: PalwPromptIdsFormV1,
    /// The most work one refutation's evaluation may do (computed elements and reduction terms).
    pub limits: DemandLimits,
}

/// **The refutation of one committed IR leaf** (`PalwCourtVerdictProofV2::TirCone`'s payload): the
/// binding, the disputed leaf, and exactly the units its evaluation reads.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirConeRefutationV1 {
    pub binding: PalwTirStepBindingV1,
    pub output_opening: PalwStepOpeningV1,
    pub output_preimage: PalwStepTileLeafV1,
    /// Every step leaf the evaluation reads, ascending leaf index, as one row: preimages plus one
    /// sibling set per contiguous run the indices derive.
    pub operands: PalwStepInputRowV1,
    /// Every inventory leaf the evaluation reads, as ONE multiproof against the class's
    /// `artifact_root` (ascending leaf index; a run of consecutive leaves shares its path, so a
    /// vocabulary or head tile pays two boundary paths, not a path per row — design §2.12.1);
    /// `None` exactly when the evaluation reads no inventory leaf.
    pub params: Option<PalwArtifactMultiproofV1>,
    /// Flat prompt form: the whole prompt when the evaluation reads a prompt token, else empty.
    pub prompt_token_ids: Vec<u32>,
    /// Merkle prompt form: one opening per prompt tile the evaluation reads, ascending tile.
    pub prompt_ids_openings: Vec<PalwPromptIdsOpeningV1>,
    /// The generated ids, pinned through the class's logits scheme, when the evaluation reads a
    /// decode position's token.
    pub decode_tokens: Option<PalwDecodeTokenPinV1>,
}

/// **What an evidence builder reads from its own retained execution** — the executor's committed
/// units, which a responder holds and a challenger obtains through the bisection's disclosures.
/// Every method answers `None` for what the builder does not hold.
pub trait PalwTirEvidenceStoreV1 {
    /// The preimage of step leaf `index`.
    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1>;
    /// The single-leaf opening of step leaf `index` in the step tree.
    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1>;
    /// The sibling set of the range opening of step leaves `[first, first + count)`
    /// (`step_merkle_range_siblings_v1`'s form).
    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>>;
    /// The artifact opening of inventory leaf `leaf`.
    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1>;
    /// **One multiproof of inventory leaves `leaves`** (ascending, distinct) against the class's
    /// `artifact_root` — what a close carries. The default assembles it from [`Self::param_opening`]'s
    /// paths ([`palw_artifact_multiproof_from_openings_v1`]); a store holding the inventory's leaf
    /// hashes may build it directly (`palw_artifact_multiproof_v1`) — the bytes are the same.
    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        let openings: Option<Vec<PalwArtifactOpeningV1>> = leaves.iter().map(|leaf| self.param_opening(*leaf)).collect();
        palw_artifact_multiproof_from_openings_v1(&openings?)
    }
    /// The whole prompt (flat form).
    fn prompt_token_ids(&self) -> Option<Vec<u32>>;
    /// The opening of prompt tile `tile` (Merkle form).
    fn prompt_ids_opening(&self, tile: u32) -> Option<PalwPromptIdsOpeningV1>;
    /// The generated ids' pin under the class's logits scheme.
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1>;
    /// **The tiled row pin of decode row `row`, aimed at lane `lane`** (the second IR fence's DA
    /// unit): the row opened in the rows tree, the committed token's tile and lane `lane`'s tile, each
    /// opened in the row's tile tree (`palw_step_refute::tiled_decode_pin_v1`'s shape). `None` by
    /// default — a store that holds the rows answers it.
    fn row_pin(&self, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
        let _ = (row, lane);
        None
    }
    /// **Step-tree node `(level, index)`'s frontier and opening** (the second IR fence's descent
    /// unit): the nodes [`PALW_TIR_STEP_NODE_DEPTH_V1`] levels below it (the leaf nodes, when nearer)
    /// and its siblings up to the root — [`palw_tir_step_node_parts_v1`] over the store's leaf
    /// hashes. `None` by default.
    fn step_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        let _ = (level, index);
        None
    }
    /// **The tiled logits trace's rows-tree node `(level, index)`'s frontier and opening** (the
    /// second IR fence's `TirRowNode`): at `level ≥ 1` the rows-tree nodes ten levels below it (the
    /// row leaves when nearer) and its siblings to the rows root; at level 0 row `index`'s tile
    /// leaves and the row's opening — [`palw_tir_row_node_parts_v1`] over the store's rows. `None`
    /// by default.
    fn row_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        let _ = (level, index);
        None
    }
}

/// Why a builder could not build a refutation.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PalwTirEvidenceErrorV1 {
    #[error("the binding does not verify: {0}")]
    Binding(String),
    #[error("leaf {0} is not a leaf of this execution")]
    NoSuchLeaf(u64),
    #[error("the store does not hold {0}")]
    Store(String),
    #[error("the evaluation failed: {0}")]
    Evaluation(String),
}

// ---------------------------------------------------------------------------------------------
// The TIR inventory's index arithmetic (F3's rule, precomputed once per class)
// ---------------------------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ParamIndexV1 {
    base: u64,
    layers: Vec<Option<u16>>,
    per_instance: u64,
    row_bytes: u64,
    pieces: u64,
    tensor_bytes: u64,
}

/// **Where every param byte of a class is committed** — `palw_tir_leaf_index_v1`'s closed form
/// with its per-param terms precomputed (the court asks it once per operand element), and its
/// inverse. `tests/palw_tir_court.rs` holds both equal to the inventory module's walk.
#[derive(Clone, Debug)]
pub struct PalwTirInventoryIndexV1 {
    params: Vec<ParamIndexV1>,
    leaf_count: u32,
}

/// One inventory leaf's coordinates: `(param, layer, byte offset in the tensor, bytes)`.
pub type PalwTirInventoryPieceV1 = (u16, Option<u16>, u32, u32);

impl PalwTirInventoryIndexV1 {
    /// The index of `program`'s inventory (empty for a program that declares no param), or `None`
    /// when the inventory refuses the program (a tensor past 4 GiB, or past a `u32` of leaves).
    pub fn new(program: &TirProgramV1) -> Option<Self> {
        if program.params.is_empty() {
            // Nothing to open: every param request is refused, as there is none to make.
            return Some(Self { params: Vec::new(), leaf_count: 0 });
        }
        let leaf_count = palw_tir_inventory_leaf_count_v1(program).ok()?;
        let mut params = Vec::with_capacity(program.params.len());
        let mut base = 0u64;
        for (j, layers) in palw_tir_param_instances_v1(program).into_iter().enumerate() {
            let d = &program.params[j];
            let tensor_bytes = palw_tir_tensor_bytes_v1(program, j as u16);
            let row_bytes = if d.shape.len() >= 2 {
                d.shape[1..].iter().map(|x| *x as u64).product::<u64>() * d.dtype.width() as u64
            } else {
                tensor_bytes
            };
            let pieces = row_bytes.div_ceil(PALW_TIR_ROW_PIECE_BYTES_V1);
            let per_instance = if tensor_bytes == 0 || row_bytes == 0 { 0 } else { (tensor_bytes / row_bytes) * pieces };
            let n = layers.len() as u64;
            params.push(ParamIndexV1 { base, layers, per_instance, row_bytes, pieces, tensor_bytes });
            base = base.checked_add(per_instance.checked_mul(n)?)?;
        }
        (base == leaf_count as u64).then_some(Self { params, leaf_count })
    }

    /// Leaves of the inventory.
    pub fn leaf_count(&self) -> u32 {
        self.leaf_count
    }

    /// The leaf holding byte `byte` of instance `(param, layer)`.
    pub fn leaf_of(&self, param: u16, layer: Option<u16>, byte: u64) -> Option<u32> {
        let p = self.params.get(param as usize)?;
        let k = p.layers.iter().position(|l| *l == layer)? as u64;
        if byte >= p.tensor_bytes || p.row_bytes == 0 {
            return None;
        }
        let (row, within) = (byte / p.row_bytes, byte % p.row_bytes);
        u32::try_from(p.base + k * p.per_instance + row * p.pieces + within / PALW_TIR_ROW_PIECE_BYTES_V1).ok()
    }

    /// The coordinates of leaf `leaf`.
    pub fn piece_of(&self, leaf: u32) -> Option<PalwTirInventoryPieceV1> {
        let leaf = leaf as u64;
        let j = self.params.partition_point(|p| p.base <= leaf).checked_sub(1)?;
        let p = &self.params[j];
        if p.per_instance == 0 {
            return None;
        }
        let rel = leaf - p.base;
        let k = (rel / p.per_instance) as usize;
        let layer = *p.layers.get(k)?;
        let rem = rel % p.per_instance;
        let (row, piece) = (rem / p.pieces, rem % p.pieces);
        let start_in_row = piece * PALW_TIR_ROW_PIECE_BYTES_V1;
        let len = (p.row_bytes - start_in_row).min(PALW_TIR_ROW_PIECE_BYTES_V1);
        let start = row * p.row_bytes + start_in_row;
        Some((j as u16, layer, u32::try_from(start).ok()?, len as u32))
    }
}

// ---------------------------------------------------------------------------------------------
// The binding, with the legacy structural arm's conviction semantics
// ---------------------------------------------------------------------------------------------

fn bad(msg: &'static str) -> PalwStepRefuteError {
    PalwStepRefuteError::InputSetNotCanonical(msg)
}

fn convict(committed: &Hash64, kind: u8, leaf_index: u64, fault: PalwStepFaultV1) -> PalwStepRefutationVerdictV1 {
    PalwStepRefutationVerdictV1 { fault, evidence_id: step_refutation_evidence_id(committed, kind, leaf_index, fault) }
}

/// What checking a binding established.
enum BindingOutcome {
    Verified(Box<PalwTirVerifiedBindingV1>),
    Convicted(PalwStepRefutationVerdictV1),
}

/// **The binding, as the court reads it.** Refused unless it provably speaks about the committed
/// execution (its version, the job context's shape, the class id the context names, the execution
/// root its parts produce) — evidence about some other commitment is never a verdict. Then the
/// class is decoded (a registered class always is: failure is `Unadjudicable`), and two faults
/// convict from the binding alone, as the legacy shape arm's do (evidence kind 0, leaf 0): a job the
/// class cannot run ([`PalwStepFaultV1::JobExceedsClassContext`]) and a step leaf count that is not
/// the job's canonical one ([`PalwStepFaultV1::StepLeafCountNotCanonical`], counted with the claim's
/// own count as the cap, so an honest count above the ladder is never "not canonical").
fn check_binding(binding: &PalwTirStepBindingV1) -> Result<BindingOutcome, PalwStepRefuteError> {
    if binding.version != PALW_TIR_STEP_BINDING_VERSION_V1 {
        return Err(bad("unsupported IR binding version"));
    }
    crate::palw_slash::check_job_context_shape(&binding.job_context).map_err(|_| bad("the job context is not well formed"))?;
    let class_id = binding.class.class_id(&binding.artifact_root);
    if binding.job_context.shape_profile_id != class_id {
        return Err(bad("the job context names another class"));
    }
    let context_hash = binding.job_context.context_hash();
    let root = palw_tir_execution_root_v1(
        &context_hash,
        &binding.full_logits_trace_root,
        &class_id,
        binding.step_leaf_count,
        &binding.step_merkle_root,
    );
    if root != binding.committed_execution_root {
        return Err(bad("the binding's parts do not produce its execution root"));
    }
    let space = PalwTirStepSpaceV1::new(&binding.class).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
    let committed = &binding.committed_execution_root;
    let job = match space.job_shape(&binding.job_context) {
        Ok(job) => job,
        Err(_) => {
            let fault = PalwStepFaultV1::JobExceedsClassContext;
            return Ok(BindingOutcome::Convicted(convict(committed, PALW_TIR_EVIDENCE_KIND_BINDING, 0, fault)));
        }
    };
    match space.leaf_count_capped(&binding.job_context, binding.step_leaf_count) {
        Ok(n) if n == binding.step_leaf_count => {}
        _ => {
            let fault = PalwStepFaultV1::StepLeafCountNotCanonical;
            return Ok(BindingOutcome::Convicted(convict(committed, PALW_TIR_EVIDENCE_KIND_BINDING, 0, fault)));
        }
    }
    Ok(BindingOutcome::Verified(Box::new(PalwTirVerifiedBindingV1 { space, class_id, context_hash, job })))
}

/// The disputed leaf, opened and structurally checked: `Ok(Ok(leaf))` when it is a canonical leaf
/// of the job, `Ok(Err(verdict))` when its own structure convicts (evidence kind 1), `Err` when the
/// opening is about another tree.
fn check_output_leaf(
    binding: &PalwTirStepBindingV1,
    v: &PalwTirVerifiedBindingV1,
    opening: &PalwStepOpeningV1,
    preimage: &PalwStepTileLeafV1,
    max_step_leaf_count: u64,
) -> Result<Result<PalwTirLeafV1, PalwStepRefutationVerdictV1>, PalwStepRefuteError> {
    let implied =
        step_opening_root_capped_v1(binding.step_leaf_count, opening, max_step_leaf_count).map_err(PalwStepRefuteError::Leg)?;
    if implied != binding.step_merkle_root {
        return Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::CommittedRootMismatch));
    }
    if step_tile_leaf_hash_v1(&v.context_hash, &v.class_id, preimage) != opening.leaf_hash {
        return Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::LeafPreimageMismatch { leaf: "IR step leaf" }));
    }
    let committed = &binding.committed_execution_root;
    let structural = |fault| Ok(Err(convict(committed, PALW_TIR_EVIDENCE_KIND_LEAF, opening.leaf_index, fault)));
    if preimage.version != PALW_TIR_STEP_LEAF_VERSION_V1 {
        return structural(PalwStepFaultV1::StepCoordinatesNotCanonical);
    }
    let Some(index) = v.space.leaf_index(&binding.job_context, &preimage.coord) else {
        return structural(PalwStepFaultV1::StepCoordinatesNotCanonical);
    };
    if index != opening.leaf_index {
        return structural(PalwStepFaultV1::StepLeafIndexNotCanonical);
    }
    if preimage.values_le.len() != 4 * preimage.value_count as usize {
        return structural(PalwStepFaultV1::StepBytesNotFourPerValue);
    }
    let leaf = v.space.leaf_at(&binding.job_context, index).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if leaf.value_count != preimage.value_count {
        return structural(PalwStepFaultV1::StepValueCountNotCanonical);
    }
    Ok(Ok(leaf))
}

// ---------------------------------------------------------------------------------------------
// PALW-TIR-33: the interval each leaf's lanes must lie in
// ---------------------------------------------------------------------------------------------

fn occurrence_block(space: &PalwTirStepSpaceV1, occurrence: u16) -> Option<u8> {
    space.occurrences().get(occurrence as usize).map(|(b, _)| *b)
}

/// **The interval a leaf's every lane must lie in** (PALW-TIR-33): a commit point's node interval;
/// a checkpoint's writer interval (the clamped write) — or `{0}` for an instance nothing writes,
/// which holds its initial zero; a history tile's appended-row interval.
pub fn palw_tir_leaf_interval_v1(space: &PalwTirStepSpaceV1, intervals: &[Vec<Interval>], leaf: &PalwTirLeafV1) -> Option<Interval> {
    let program = &space.program;
    match leaf.kind {
        PalwTirLeafKindV1::Commit { block, node, .. } => intervals.get(block as usize)?.get(node as usize).copied(),
        PalwTirLeafKindV1::State { state, layer, .. } => match state_writer_v1(program, state, layer) {
            Some((occ, w)) => intervals.get(occurrence_block(space, occ)? as usize)?.get(w as usize).copied(),
            None => Some(Interval::point(0)),
        },
        PalwTirLeafKindV1::HistTile { state, layer, .. } => {
            let block = occurrence_block(space, state_occurrence_v1(program, state, layer)?)?;
            let append = program.blocks[block as usize]
                .nodes
                .iter()
                .position(|n| matches!(n.prim, Prim::HistAppend { state: s } if s == state))?;
            intervals.get(block as usize)?.get(append).copied()
        }
    }
}

fn lane(dtype: DType, bytes: &[u8], i: usize) -> Option<i128> {
    let b = bytes.get(i * 4..i * 4 + 4)?;
    let q = [b[0], b[1], b[2], b[3]];
    Some(if dtype == DType::Idx { u32::from_le_bytes(q) as i128 } else { i32::from_le_bytes(q) as i128 })
}

/// The first lane of `bytes` outside `interval`.
fn first_outside(dtype: DType, bytes: &[u8], interval: Interval) -> Option<u32> {
    (0..bytes.len() / 4).find(|i| lane(dtype, bytes, *i).is_none_or(|v| !interval.contains(v))).map(|i| i as u32)
}

// ---------------------------------------------------------------------------------------------
// The court's source: authenticated units, every request recorded
// ---------------------------------------------------------------------------------------------

/// The units a source serves. The court fills them from the refutation after authenticating each;
/// a builder fills them from its store on first use.
struct Units<'s> {
    steps: BTreeMap<u64, Vec<u8>>,
    params: BTreeMap<u32, (u32, Vec<u8>)>,
    prompt: BTreeMap<u32, u32>,
    generated: Option<Vec<u32>>,
    store: Option<&'s dyn PalwTirEvidenceStoreV1>,
    prompt_form: PalwPromptIdsFormV1,
}

impl Units<'_> {
    fn step(&mut self, index: u64) -> Option<&[u8]> {
        if !self.steps.contains_key(&index) {
            let preimage = self.store?.step_leaf(index)?;
            self.steps.insert(index, preimage.values_le);
        }
        self.steps.get(&index).map(|v| v.as_slice())
    }

    fn param(&mut self, leaf: u32) -> Option<&(u32, Vec<u8>)> {
        if !self.params.contains_key(&leaf) {
            let opening = self.store?.param_opening(leaf)?;
            self.params.insert(leaf, (opening.operand.row_start, opening.operand.bytes));
        }
        self.params.get(&leaf)
    }

    fn prompt_id(&mut self, pos: u32) -> Option<u32> {
        if !self.prompt.contains_key(&pos) {
            let store = self.store?;
            match self.prompt_form {
                PalwPromptIdsFormV1::Flat => {
                    for (p, id) in store.prompt_token_ids()?.into_iter().enumerate() {
                        self.prompt.insert(p as u32, id);
                    }
                }
                PalwPromptIdsFormV1::MerkleV1 => {
                    let opening = store.prompt_ids_opening(pos / PALW_PROMPT_IDS_TILE_LEN)?;
                    let base = opening.tile_index * PALW_PROMPT_IDS_TILE_LEN;
                    for (k, id) in opening.tile_ids.iter().enumerate() {
                        self.prompt.insert(base + k as u32, *id);
                    }
                }
            }
        }
        self.prompt.get(&pos).copied()
    }

    fn generated_id(&mut self, i: u32) -> Option<u32> {
        if self.generated.is_none() {
            self.generated = Some(match self.store?.decode_pin()? {
                PalwDecodeTokenPinV1::TiledV1(d) => d.generated_token_ids,
                PalwDecodeTokenPinV1::Base0V1(d) => d.generated_token_ids,
                PalwDecodeTokenPinV1::FloatV2(d) => d.generated_token_ids,
            });
        }
        self.generated.as_ref()?.get(i as usize).copied()
    }
}

/// What an evaluation asked for.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Requests {
    steps: BTreeSet<u64>,
    params: BTreeSet<u32>,
    prompt: BTreeSet<u32>,
    decode: bool,
}

struct TirSource<'a, 's> {
    space: &'a PalwTirStepSpaceV1,
    ctx: &'a PalwJobContextV2,
    job: PalwTirJobShapeV1,
    inventory: &'a PalwTirInventoryIndexV1,
    units: Units<'s>,
    /// Every step leaf served must precede this one (the disputed leaf).
    before: u64,
    used: Requests,
    leaf_cache: BTreeMap<(DemandContext, u16, u64), u64>,
    /// A range evaluation's supplied nodes (spec 04b §9.5): nodes of `supplied_ctx` whose elements
    /// are a dissection claim's values, by `(node, element)`, and which of them were read.
    supplied_ctx: Option<DemandContext>,
    supplied: BTreeMap<(u16, usize), i128>,
    used_supplied: BTreeSet<(u16, usize)>,
}

fn tir_err<T>(kind: TirErrorKind, msg: impl Into<String>) -> TirResult<T> {
    Err(TirError::new(kind, msg))
}

impl TirSource<'_, '_> {
    /// Serve lane `lane` of step leaf `coord`, decoded as `dtype`, recording the leaf.
    fn step_lane(&mut self, coord: PalwStepCoordinateV1, dtype: DType, lane_index: usize) -> TirResult<i128> {
        let Some(index) = self.space.leaf_index(self.ctx, &coord) else {
            return tir_err(TirErrorKind::Malformed, format!("{coord:?} is no leaf of this job"));
        };
        self.step_lane_at(index, dtype, lane_index)
    }

    fn step_lane_at(&mut self, index: u64, dtype: DType, lane_index: usize) -> TirResult<i128> {
        if index >= self.before {
            return tir_err(TirErrorKind::Malformed, format!("leaf {index} does not precede the disputed leaf {}", self.before));
        }
        let Some(bytes) = self.units.step(index) else {
            return tir_err(TirErrorKind::Missing, format!("step leaf {index}"));
        };
        let Some(v) = lane(dtype, bytes, lane_index) else {
            return tir_err(TirErrorKind::Malformed, format!("step leaf {index} has no lane {lane_index}"));
        };
        self.used.steps.insert(index);
        Ok(v)
    }

    fn commit_leaf(&mut self, ctx: DemandContext, node: u16, tile: u64) -> TirResult<u64> {
        if let Some(i) = self.leaf_cache.get(&(ctx, node, tile)) {
            return Ok(*i);
        }
        let Some(block) = occurrence_block(self.space, ctx.occurrence) else {
            return tir_err(TirErrorKind::Malformed, "no such occurrence");
        };
        let slot = self.space.node_slot(ctx.occurrence as usize, node);
        let (Some(slot), Some(_)) = (slot, self.space.commit_tile_len(block, node)) else {
            return tir_err(TirErrorKind::Malformed, format!("node {node} of block {block} is not a commit point"));
        };
        let (call_index, position) = self.job.call_position(ctx.pos);
        let coord = PalwStepCoordinateV1 { call_index, node_slot: slot, position, tile_index: tile as u32 };
        let Some(index) = self.space.leaf_index(self.ctx, &coord) else {
            return tir_err(TirErrorKind::Malformed, format!("{coord:?} is no leaf of this job"));
        };
        self.leaf_cache.insert((ctx, node, tile), index);
        Ok(index)
    }

    fn reserved_coord(&self, pos: u32, reserved: u32, tile: u64) -> PalwStepCoordinateV1 {
        let (call_index, position) = self.job.call_position(pos);
        PalwStepCoordinateV1 { call_index, node_slot: self.space.program_slots() + reserved, position, tile_index: tile as u32 }
    }
}

impl DemandSource for TirSource<'_, '_> {
    fn node(&mut self, ctx: DemandContext, node: u16, index: usize) -> TirResult<i128> {
        if self.supplied_ctx == Some(ctx) && self.supplied.range((node, 0)..=(node, usize::MAX)).next().is_some() {
            let Some(v) = self.supplied.get(&(node, index)).copied() else {
                return tir_err(TirErrorKind::Missing, format!("the claim has no value for element {index} of node {node}"));
            };
            self.used_supplied.insert((node, index));
            return Ok(v);
        }
        let Some(block) = occurrence_block(self.space, ctx.occurrence) else {
            return tir_err(TirErrorKind::Malformed, "no such occurrence");
        };
        let Some(tile_len) = self.space.commit_tile_len(block, node) else {
            return tir_err(TirErrorKind::Malformed, format!("node {node} of block {block} is not a commit point"));
        };
        let dtype = self.space.program.blocks[block as usize].nodes[node as usize].out.dtype;
        let tile = (index / tile_len as usize) as u64;
        let leaf = self.commit_leaf(ctx, node, tile)?;
        self.step_lane_at(leaf, dtype, index % tile_len as usize)
    }

    fn param(&mut self, param: u16, layer: Option<u16>, index: usize) -> TirResult<i128> {
        let Some(d) = self.space.program.params.get(param as usize) else {
            return tir_err(TirErrorKind::Malformed, "no such param");
        };
        let width = d.dtype.width();
        let dtype = d.dtype;
        let byte = (index as u64).saturating_mul(width as u64);
        let Some(leaf) = self.inventory.leaf_of(param, layer, byte) else {
            return tir_err(TirErrorKind::Malformed, format!("param {param} layer {layer:?} has no byte {byte}"));
        };
        let Some((row_start, bytes)) = self.units.param(leaf) else {
            return tir_err(TirErrorKind::Missing, format!("inventory leaf {leaf}"));
        };
        let at = byte.checked_sub(*row_start as u64).map(|a| a as usize);
        let Some(element) = at.and_then(|a| bytes.get(a..a + width)) else {
            return tir_err(TirErrorKind::Malformed, format!("inventory leaf {leaf} does not hold byte {byte}"));
        };
        let v = dtype.decode_le(element);
        self.used.params.insert(leaf);
        Ok(v)
    }

    fn state(&mut self, pos: u32, state: u16, layer: Option<u16>, index: usize) -> TirResult<StateSupply> {
        if pos == 0 {
            return Ok(StateSupply::Value(0));
        }
        if !pos.is_multiple_of(self.space.layout.checkpoint_interval) {
            return Ok(StateSupply::Replay);
        }
        let Some(k) = self.space.fixed_instance_index(state, layer) else {
            return tir_err(TirErrorKind::Malformed, format!("state {state} layer {layer:?} has no checkpoint"));
        };
        let inst = self.space.fixed_instances()[k];
        let tile = index as u64 / inst.tile_lanes as u64;
        let coord = self.reserved_coord(pos - 1, k as u32, tile);
        self.step_lane(coord, inst.dtype, index % inst.tile_lanes as usize).map(StateSupply::Value)
    }

    fn hist_row(&mut self, pos: u32, state: u16, layer: Option<u16>, row_pos: u32, index: usize) -> TirResult<i128> {
        let h_tile = self.space.layout.h_tile;
        let tile_start = row_pos - row_pos % h_tile;
        let tile_end = tile_start + (h_tile - 1);
        if tile_end < pos {
            let Some(k) = self.space.hist_instance_index(state, layer) else {
                return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} has no tiles"));
            };
            let inst = self.space.hist_instances()[k];
            let sub = index as u64 / inst.tile_lanes as u64;
            let first_lane = sub * inst.tile_lanes as u64;
            let row_lanes = (inst.elements - first_lane).min(inst.tile_lanes as u64);
            let coord = self.reserved_coord(tile_end, (self.space.fixed_instances().len() + k) as u32, sub);
            let lane_index = (row_pos - tile_start) as u64 * row_lanes + (index as u64 - first_lane);
            self.step_lane(coord, inst.dtype, lane_index as usize)
        } else {
            let Some((ctx, node)) = hist_row_node_v1(&self.space.program, state, layer, row_pos) else {
                return tir_err(TirErrorKind::Malformed, format!("history {state} layer {layer:?} appends nothing"));
            };
            self.node(ctx, node, index)
        }
    }

    fn token(&mut self, pos: u32) -> TirResult<u32> {
        if pos < self.job.prefill {
            let Some(id) = self.units.prompt_id(pos) else {
                return tir_err(TirErrorKind::Missing, format!("the prompt id at {pos}"));
            };
            self.used.prompt.insert(pos);
            Ok(id)
        } else {
            let Some(id) = self.units.generated_id(pos - self.job.prefill) else {
                return tir_err(TirErrorKind::Missing, format!("the generated id read at {pos}"));
            };
            self.used.decode = true;
            Ok(id)
        }
    }
}

/// **Evaluate the values a leaf must hold**, from the source.
fn evaluate_leaf(
    space: &PalwTirStepSpaceV1,
    leaf: &PalwTirLeafV1,
    source: &mut TirSource<'_, '_>,
    limits: &DemandLimits,
) -> Result<Vec<i128>, DemandError> {
    let count = leaf.value_count as u64;
    let range = |first: u64| -> Vec<usize> { (first..first + count).map(|e| e as usize).collect() };
    match leaf.kind {
        PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
            let elements = range(first_element);
            let request = DemandRequest {
                target: DemandTarget::Node { ctx: DemandContext { pos: leaf.position, occurrence: occurrence as u16 }, node },
                elements: &elements,
            };
            Ok(eval_demanded(&space.program, &space.info, &request, source, limits)?.0)
        }
        PalwTirLeafKindV1::State { state, layer, first_element, .. } => {
            let elements = range(first_element);
            let request = DemandRequest { target: DemandTarget::StateAfter { pos: leaf.position, state, layer }, elements: &elements };
            Ok(eval_demanded(&space.program, &space.info, &request, source, limits)?.0)
        }
        PalwTirLeafKindV1::HistTile { state, layer, first_lane, row_lanes, first_position, .. } => {
            // A history tile is the committed rows it concatenates, read from their commit points.
            let mut out = Vec::with_capacity(leaf.value_count as usize);
            for row_pos in first_position..=leaf.position {
                let (ctx, node) = hist_row_node_v1(&space.program, state, layer, row_pos)
                    .ok_or_else(|| DemandError::Tir(TirError::new(TirErrorKind::Malformed, "the history appends nothing")))?;
                for l in first_lane..first_lane + row_lanes as u64 {
                    out.push(source.node(ctx, node, l as usize)?);
                }
            }
            Ok(out)
        }
    }
}

fn evaluation_refusal(e: &DemandError) -> PalwStepRefuteError {
    match e {
        // Every refusal of the source is `Missing` (spec 04b §9.4), a unit not carried among them.
        DemandError::Tir(t) if t.kind == TirErrorKind::Missing => bad("the evaluation reads a unit the refutation does not carry"),
        // A work limit, a malformed request of the court's own making, or an evaluation error after
        // PALW-TIR-33 held on every operand: an interpreter defect. Nobody is slashed.
        _ => PalwStepRefuteError::Unadjudicable,
    }
}

// ---------------------------------------------------------------------------------------------
// The carriage: authenticate every carried unit
// ---------------------------------------------------------------------------------------------

/// The carried operand row, authenticated: its leaves (ascending, each a canonical leaf of the job
/// preceding the disputed one, well formed) against the step root by the runs their indices derive.
fn authenticate_operands(
    binding: &PalwTirStepBindingV1,
    v: &PalwTirVerifiedBindingV1,
    row: &PalwStepInputRowV1,
    before: u64,
    max_step_leaf_count: u64,
) -> Result<Vec<(u64, PalwTirLeafV1)>, PalwStepRefuteError> {
    let mut leaves: Vec<(u64, PalwTirLeafV1)> = Vec::with_capacity(row.preimages.len());
    let mut hashes = Vec::with_capacity(row.preimages.len());
    for preimage in &row.preimages {
        let index = v.space.leaf_index(&binding.job_context, &preimage.coord).ok_or(bad("an operand names no leaf of this job"))?;
        if leaves.last().is_some_and(|(prev, _)| *prev >= index) {
            return Err(bad("the operands are not in ascending leaf order"));
        }
        if index >= before {
            return Err(bad("an operand does not precede the disputed leaf"));
        }
        let leaf = v.space.leaf_at(&binding.job_context, index).ok_or(PalwStepRefuteError::Unadjudicable)?;
        if preimage.version != PALW_TIR_STEP_LEAF_VERSION_V1
            || preimage.value_count != leaf.value_count
            || preimage.values_le.len() != 4 * leaf.value_count as usize
        {
            return Err(bad("an operand leaf is not well formed"));
        }
        hashes.push(step_tile_leaf_hash_v1(&v.context_hash, &v.class_id, preimage));
        leaves.push((index, leaf));
    }
    let mut runs: Vec<(usize, usize)> = Vec::new();
    for (i, (index, _)) in leaves.iter().enumerate() {
        match runs.last_mut() {
            Some((start, len)) if leaves[*start].0 + *len as u64 == *index => *len += 1,
            _ => runs.push((i, 1)),
        }
    }
    if row.run_siblings.len() != runs.len() {
        return Err(bad("the operand row's run count is not the one its indices derive"));
    }
    for ((start, len), siblings) in runs.iter().zip(row.run_siblings.iter()) {
        let opening = PalwStepRangeOpeningV1 {
            first_leaf_index: leaves[*start].0,
            leaf_hashes: hashes[*start..*start + *len].to_vec(),
            siblings: siblings.clone(),
        };
        let implied = step_range_opening_root_capped_v1(binding.step_leaf_count, &opening, max_step_leaf_count)
            .map_err(PalwStepRefuteError::Leg)?;
        if implied != binding.step_merkle_root {
            return Err(PalwStepRefuteError::Leg(crate::palw_step_leg::PalwStepLegError::CommittedRootMismatch));
        }
    }
    Ok(leaves)
}

/// The carried multiproof, authenticated against the class's `artifact_root`: of this class's
/// inventory, ascending, each opened operand exactly one whole inventory leaf of the class, and the
/// proof reaching the root. `None` carries nothing. Total on hostile input: every malformed proof —
/// an empty one, another inventory's size, an index out of range, out of order or repeated, a
/// sibling short, extra or altered, an operand not its leaf's piece — is refused, never a panic.
fn authenticate_params(
    binding: &PalwTirStepBindingV1,
    v: &PalwTirVerifiedBindingV1,
    inventory: &PalwTirInventoryIndexV1,
    proof: Option<&PalwArtifactMultiproofV1>,
) -> Result<BTreeMap<u32, (u32, Vec<u8>)>, PalwStepRefuteError> {
    let mut out = BTreeMap::new();
    let Some(proof) = proof else { return Ok(out) };
    if proof.opened.is_empty() {
        return Err(bad("an artifact multiproof opens nothing (carry none instead)"));
    }
    if proof.leaf_count != inventory.leaf_count() {
        return Err(bad("the artifact multiproof is of another inventory"));
    }
    let mut last: Option<u32> = None;
    for (index, operand) in &proof.opened {
        if last.is_some_and(|l| l >= *index) {
            return Err(bad("the artifact multiproof's leaves are not in ascending order"));
        }
        last = Some(*index);
        let (param, layer, start, len) = inventory.piece_of(*index).ok_or(bad("the artifact multiproof names no leaf"))?;
        let d = v.space.program.params.get(param as usize).ok_or(bad("the artifact multiproof names no leaf"))?;
        if operand.tensor_name != d.name || operand.layer != layer || operand.row_start != start || operand.bytes.len() != len as usize {
            return Err(bad("an opened operand is not its leaf's canonical piece"));
        }
    }
    verify_artifact_multiproof_v1(proof, binding.artifact_root)
        .map_err(|_| bad("the artifact multiproof does not reach the class's root"))?;
    for (index, operand) in &proof.opened {
        out.insert(*index, (operand.row_start, operand.bytes.clone()));
    }
    Ok(out)
}

/// **How an IR close carries its parameter openings** — the parameter of
/// [`palw_tir_param_carriage_bytes_v1`], so an admission price can compare the two.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwTirParamCarriageV1 {
    /// One `PalwArtifactOpeningV1` per leaf, each with its whole path (the format before §2.12.1).
    PerLeafOpenings,
    /// ONE `PalwArtifactMultiproofV1` (the carried format, [`PalwTirConeRefutationV1::params`]).
    Multiproof,
}

/// **The borsh bytes of an IR close's parameter carriage** for inventory leaves `leaves` of
/// `program`, in `format` — byte for byte what [`build_tir_cone_refutation_v1`] carries in
/// [`PalwTirConeRefutationV1::params`] (`Multiproof`: the option's tag, then the proof, or the tag
/// alone for no leaf), or what the per-leaf vector was (`PerLeafOpenings`). The one price of
/// parameter bytes an admission rule may use (design §2.12.1). `leaves` ascending and distinct;
/// `None` when the program has no inventory index or a leaf is not in it.
pub fn palw_tir_param_carriage_bytes_v1(program: &TirProgramV1, format: PalwTirParamCarriageV1, leaves: &[u32]) -> Option<u64> {
    if leaves.windows(2).any(|w| w[0] >= w[1]) {
        return None;
    }
    let inventory = PalwTirInventoryIndexV1::new(program)?;
    let mut operand_lens = Vec::with_capacity(leaves.len());
    for leaf in leaves {
        let (param, layer, _, len) = inventory.piece_of(*leaf)?;
        let name = &program.params.get(param as usize)?.name;
        operand_lens.push(palw_artifact_operand_borsh_len_v1(name.len(), layer.is_some(), len as usize));
    }
    match format {
        PalwTirParamCarriageV1::Multiproof => {
            if leaves.is_empty() {
                return Some(1);
            }
            let siblings = palw_artifact_multiproof_sibling_count_v1(inventory.leaf_count(), leaves)?;
            Some(1u64.saturating_add(palw_artifact_multiproof_borsh_len_v1(operand_lens, siblings)))
        }
        PalwTirParamCarriageV1::PerLeafOpenings => {
            let mut total = 4u64;
            for (leaf, operand) in leaves.iter().zip(operand_lens) {
                let path = palw_artifact_opening_path_len_v1(inventory.leaf_count(), *leaf)?;
                total = total.saturating_add(operand + 4 + 4 + 4 + 64 * path);
            }
            Some(total)
        }
    }
}

/// The carried prompt ids, authenticated in the network's form: position → id.
fn authenticate_prompt(
    ctx: &PalwJobContextV2,
    form: PalwPromptIdsFormV1,
    ids: &[u32],
    openings: &[PalwPromptIdsOpeningV1],
) -> Result<(BTreeMap<u32, u32>, BTreeSet<u32>), PalwStepRefuteError> {
    let mut prompt = BTreeMap::new();
    let mut tiles = BTreeSet::new();
    match form {
        PalwPromptIdsFormV1::Flat => {
            if !openings.is_empty() {
                return Err(bad("a flat network's refutation carries the prompt whole, never an opening"));
            }
            if !ids.is_empty() {
                if crate::palw_v2::prompt_token_ids_hash_v2(ids) != ctx.prompt_token_ids_hash {
                    return Err(bad("the carried prompt ids are not the ones the job context commits to"));
                }
                prompt.extend(ids.iter().enumerate().map(|(p, id)| (p as u32, *id)));
            }
        }
        PalwPromptIdsFormV1::MerkleV1 => {
            if !ids.is_empty() {
                return Err(bad("a Merkle network's refutation carries prompt openings, never the whole list"));
            }
            for o in openings {
                if tiles.last().is_some_and(|t| *t >= o.tile_index) {
                    return Err(bad("the prompt openings are not in ascending tile order"));
                }
                let window = verify_prompt_ids_opening_v1(&ctx.prompt_token_ids_hash, ctx.declared_prefill_tokens, o)
                    .map_err(|e| bad(e.refusal()))?;
                let base = o.tile_index * PALW_PROMPT_IDS_TILE_LEN;
                for k in 0..o.tile_ids.len() as u32 {
                    if let Some(id) = window.at(base + k) {
                        prompt.insert(base + k, id);
                    }
                }
                tiles.insert(o.tile_index);
            }
        }
    }
    Ok((prompt, tiles))
}

/// The logits node's lanes: its element count (the class's vocabulary) and its scheme.
fn logits_shape(space: &PalwTirStepSpaceV1) -> (usize, Hash64) {
    let p = &space.program;
    let logits = &p.blocks[p.schedule.post as usize].nodes[p.logits as usize];
    (logits.out.elements_at(1) as usize, Hash64::from_bytes(p.logits_scheme_id))
}

/// The carried decode pin, authenticated against the claim's own trace root under the class's
/// scheme: the generated ids.
fn authenticate_decode_pin(
    binding: &PalwTirStepBindingV1,
    space: &PalwTirStepSpaceV1,
    pin: &PalwDecodeTokenPinV1,
) -> Result<Vec<u32>, PalwStepRefuteError> {
    let ctx = &binding.job_context;
    let decode = ctx.exact_decode_tokens as usize;
    let (vocab, scheme) = logits_shape(space);
    match pin {
        PalwDecodeTokenPinV1::TiledV1(d) if scheme == tiled_logits_scheme_id_v1() => {
            if d.generated_token_ids.len() != decode {
                return Err(bad("the pin's id count is not the context's decode count"));
            }
            if tiled_logits_outer_root_v1(ctx, decode as u64, &d.rows_root, &d.generated_token_ids) != binding.full_logits_trace_root {
                return Err(bad("the carried ids and rows root do not reproduce the claim's own tiled trace root"));
            }
            Ok(d.generated_token_ids.clone())
        }
        PalwDecodeTokenPinV1::Base0V1(d) if scheme == flat_logits_scheme_id_v1() => {
            check_flat_pin(binding, vocab, d)?;
            Ok(d.generated_token_ids.clone())
        }
        _ => Err(bad("the decode-token pin does not speak the class's logits scheme")),
    }
}

fn check_flat_pin(binding: &PalwTirStepBindingV1, vocab: usize, d: &PalwBase0DecodeTokensV1) -> Result<(), PalwStepRefuteError> {
    let ctx = &binding.job_context;
    let decode = ctx.exact_decode_tokens as usize;
    if d.logits_rows.len() != decode || d.generated_token_ids.len() != decode {
        return Err(bad("the pin's row or id count is not the context's decode count"));
    }
    if d.logits_rows.iter().any(|row| row.len() != vocab) {
        return Err(bad("a pinned logits row is not the class's vocabulary wide"));
    }
    if base0_logits_trace_root_v1(ctx, &d.logits_rows, &d.generated_token_ids) != binding.full_logits_trace_root {
        return Err(bad("the carried logits do not reproduce the claim's own flat trace root"));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The check
// ---------------------------------------------------------------------------------------------

/// **Adjudicate one committed IR leaf.** `Ok(verdict)` convicts the executor; `Err(NoFaultFound)`
/// acquits (the leaf recomputes to its committed lanes); any other `Err` refuses the refutation and
/// slashes nobody.
///
/// The order is normative — every implementation must return the same verdict and evidence id:
///
/// 1. **the binding** — refused unless it speaks about the committed execution; the class decoded
///    (`Unadjudicable` if not); convictions from the binding alone (kind 0, leaf 0): a job outside
///    the class ([`PalwStepFaultV1::JobExceedsClassContext`]), a step leaf count that is not the
///    job's ([`PalwStepFaultV1::StepLeafCountNotCanonical`]);
/// 2. **the disputed leaf** — its opening and preimage refused unless they reach the step root; its
///    structure convicts (kind 1, the leaf's index): version or coordinates, index, bytes, count;
/// 3. **the program's intervals** (`analyze_ranges`; `Unadjudicable` if they do not exist);
/// 4. **PALW-TIR-33 on the disputed leaf** — the first lane outside convicts (kind 5, the leaf);
/// 5. **the carriage**, each part refused unless it authenticates: the operand row, the artifact
///    openings, the prompt carriage in the network's form, the decode pin in the class's scheme;
/// 6. **PALW-TIR-33 on every carried step leaf**, ascending leaf index, lanes in order — the first
///    lane outside convicts (kind 5, THAT leaf's index);
/// 7. **the evaluation** of the disputed leaf's values — a refusal of the court's source (a unit the
///    refutation does not carry, or one the court will not serve) is `Missing` and so
///    `InputSetNotCanonical`; any other failure `Unadjudicable`;
/// 8. **the canonical set** — every carried unit was read, and the prompt and decode carriages are
///    present exactly when read; otherwise `InputSetNotCanonical`;
/// 9. **the comparison** — the first differing lane convicts
///    ([`PalwStepFaultV1::ComputationMismatch`], kind 5, the leaf); none is `NoFaultFound`.
pub fn check_tir_cone_refutation_v1(
    refutation: &PalwTirConeRefutationV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    // 1–6.
    let c = match check_carriage(refutation, rules)? {
        CarriageStage::Convicted(verdict) => return Ok(verdict),
        CarriageStage::Checked(c) => c,
    };
    // 7.
    let mut source = c.source(rules);
    let values = evaluate_leaf(&c.v.space, &c.leaf, &mut source, &rules.limits).map_err(|e| evaluation_refusal(&e))?;
    // 8.
    c.check_canonical(&source.used, rules)?;
    // 9.
    match c.first_difference(&values) {
        Some(i) => {
            let fault = PalwStepFaultV1::ComputationMismatch { value_index: i as u32 };
            Ok(convict(&refutation.binding.committed_execution_root, PALW_TIR_EVIDENCE_KIND_CONE, c.out_index, fault))
        }
        None => Err(PalwStepRefuteError::NoFaultFound),
    }
}

/// A refutation-form carriage after steps 1–6 of [`check_tir_cone_refutation_v1`]: every unit
/// authenticated and every carried step leaf inside its proven interval — or the conviction one of
/// those steps found.
enum CarriageStage<'r> {
    Convicted(PalwStepRefutationVerdictV1),
    Checked(Box<CheckedCarriage<'r>>),
}

struct CheckedCarriage<'r> {
    refutation: &'r PalwTirConeRefutationV1,
    v: Box<PalwTirVerifiedBindingV1>,
    leaf: PalwTirLeafV1,
    out_index: u64,
    intervals: Vec<Vec<Interval>>,
    inventory: PalwTirInventoryIndexV1,
    operands: Vec<(u64, PalwTirLeafV1)>,
    params: BTreeMap<u32, (u32, Vec<u8>)>,
    prompt: BTreeMap<u32, u32>,
    prompt_tiles: BTreeSet<u32>,
    generated: Option<Vec<u32>>,
}

/// Steps 1–6 of [`check_tir_cone_refutation_v1`], shared by every IR close and by the dissection's
/// root claim: the binding, the disputed leaf, the intervals, PALW-TIR-33 on the leaf, the carriage,
/// PALW-TIR-33 on every carried step leaf.
fn check_carriage<'r>(
    refutation: &'r PalwTirConeRefutationV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<CarriageStage<'r>, PalwStepRefuteError> {
    let binding = &refutation.binding;
    // 1.
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(verdict) => return Ok(CarriageStage::Convicted(verdict)),
        BindingOutcome::Verified(v) => v,
    };
    let committed = &binding.committed_execution_root;
    // 2.
    let leaf =
        match check_output_leaf(binding, &v, &refutation.output_opening, &refutation.output_preimage, rules.max_step_leaf_count)? {
            Ok(leaf) => leaf,
            Err(verdict) => return Ok(CarriageStage::Convicted(verdict)),
        };
    let out_index = refutation.output_opening.leaf_index;
    // 3.
    let intervals = analyze_ranges(&v.space.program).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
    // 4.
    let out_interval = palw_tir_leaf_interval_v1(&v.space, &intervals, &leaf).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if let Some(i) = first_outside(leaf.dtype, &refutation.output_preimage.values_le, out_interval) {
        let fault = PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: i };
        return Ok(CarriageStage::Convicted(convict(committed, PALW_TIR_EVIDENCE_KIND_CONE, out_index, fault)));
    }
    // 5.
    let operands = authenticate_operands(binding, &v, &refutation.operands, out_index, rules.max_step_leaf_count)?;
    let inventory = PalwTirInventoryIndexV1::new(&v.space.program).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let params = authenticate_params(binding, &v, &inventory, refutation.params.as_ref())?;
    let (prompt, prompt_tiles) =
        authenticate_prompt(&binding.job_context, rules.prompt_form, &refutation.prompt_token_ids, &refutation.prompt_ids_openings)?;
    let generated = match &refutation.decode_tokens {
        Some(pin) => Some(authenticate_decode_pin(binding, &v.space, pin)?),
        None => None,
    };
    // 6.
    for ((index, operand_leaf), preimage) in operands.iter().zip(refutation.operands.preimages.iter()) {
        let iv = palw_tir_leaf_interval_v1(&v.space, &intervals, operand_leaf).ok_or(PalwStepRefuteError::Unadjudicable)?;
        if let Some(i) = first_outside(operand_leaf.dtype, &preimage.values_le, iv) {
            let fault = PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: i };
            return Ok(CarriageStage::Convicted(convict(committed, PALW_TIR_EVIDENCE_KIND_CONE, *index, fault)));
        }
    }
    Ok(CarriageStage::Checked(Box::new(CheckedCarriage {
        refutation,
        v,
        leaf,
        out_index,
        intervals,
        inventory,
        operands,
        params,
        prompt,
        prompt_tiles,
        generated,
    })))
}

impl CheckedCarriage<'_> {
    /// The court's source over the authenticated units (step 7).
    fn source(&self, rules: &PalwTirCourtRulesV1) -> TirSource<'_, 'static> {
        TirSource {
            space: &self.v.space,
            ctx: &self.refutation.binding.job_context,
            job: self.v.job,
            inventory: &self.inventory,
            units: Units {
                steps: self
                    .operands
                    .iter()
                    .zip(self.refutation.operands.preimages.iter())
                    .map(|((i, _), p)| (*i, p.values_le.clone()))
                    .collect(),
                params: self.params.clone(),
                prompt: self.prompt.clone(),
                generated: self.generated.clone(),
                store: None,
                prompt_form: rules.prompt_form,
            },
            before: self.out_index,
            used: Requests::default(),
            leaf_cache: BTreeMap::new(),
            supplied_ctx: None,
            supplied: BTreeMap::new(),
            used_supplied: BTreeSet::new(),
        }
    }

    /// Step 8: every carried unit was read, and the prompt and decode carriages are present exactly
    /// when read.
    fn check_canonical(&self, used: &Requests, rules: &PalwTirCourtRulesV1) -> Result<(), PalwStepRefuteError> {
        let r = self.refutation;
        if used.steps.len() != self.operands.len() || !self.operands.iter().all(|(i, _)| used.steps.contains(i)) {
            return Err(bad("the operand row is not the set the evaluation reads"));
        }
        if used.params.len() != r.params.as_ref().map_or(0, |p| p.opened.len()) {
            return Err(bad("the artifact multiproof is not the set the evaluation reads"));
        }
        match rules.prompt_form {
            PalwPromptIdsFormV1::Flat => {
                if used.prompt.is_empty() != r.prompt_token_ids.is_empty() {
                    return Err(bad("the prompt ids ride exactly when the evaluation reads one"));
                }
            }
            PalwPromptIdsFormV1::MerkleV1 => {
                let read: BTreeSet<u32> = used.prompt.iter().map(|p| p / PALW_PROMPT_IDS_TILE_LEN).collect();
                if read != self.prompt_tiles {
                    return Err(bad("the prompt openings are not the tiles the evaluation reads"));
                }
            }
        }
        if used.decode != r.decode_tokens.is_some() {
            return Err(bad("the decode pin rides exactly when the evaluation reads a generated token"));
        }
        Ok(())
    }

    /// Step 9: the first lane of the disputed leaf the evaluation disagrees with.
    fn first_difference(&self, values: &[i128]) -> Option<usize> {
        let committed: Vec<i128> = (0..self.leaf.value_count as usize)
            .map(|i| lane(self.leaf.dtype, &self.refutation.output_preimage.values_le, i).unwrap_or(i128::MIN))
            .collect();
        (values.len() != committed.len()).then_some(0).or_else(|| values.iter().zip(committed.iter()).position(|(a, b)| a != b))
    }
}

// ---------------------------------------------------------------------------------------------
// The builder (the API a node's evidence builder calls)
// ---------------------------------------------------------------------------------------------

/// **Build the canonical refutation of step leaf `output_leaf`** from a store of the executor's
/// committed units: the same evaluation the court runs, over the store, recording what it reads;
/// then exactly that, assembled in the canonical order. Whether the leaf is honest does not matter
/// here — the court decides; a responder builds the acquitting close with the same call.
pub fn build_tir_cone_refutation_v1(
    binding: &PalwTirStepBindingV1,
    output_leaf: u64,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirConeRefutationV1, PalwTirEvidenceErrorV1> {
    let (v, leaf, inventory) = builder_prelude(binding, output_leaf)?;
    let mut source = store_source(&v, &binding.job_context, &inventory, store, output_leaf, rules);
    evaluate_leaf(&v.space, &leaf, &mut source, &rules.limits).map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
    assemble_carriage(binding, output_leaf, &source.used, store, rules)
}

/// A builder's verified binding, the leaf, and the class's inventory index.
fn builder_prelude(
    binding: &PalwTirStepBindingV1,
    output_leaf: u64,
) -> Result<(Box<PalwTirVerifiedBindingV1>, PalwTirLeafV1, PalwTirInventoryIndexV1), PalwTirEvidenceErrorV1> {
    let v = match check_binding(binding) {
        Ok(BindingOutcome::Verified(v)) => v,
        Ok(BindingOutcome::Convicted(verdict)) => {
            return Err(PalwTirEvidenceErrorV1::Binding(format!("the binding itself convicts ({:?})", verdict.fault)));
        }
        Err(e) => return Err(PalwTirEvidenceErrorV1::Binding(e.to_string())),
    };
    let leaf = v.space.leaf_at(&binding.job_context, output_leaf).ok_or(PalwTirEvidenceErrorV1::NoSuchLeaf(output_leaf))?;
    let inventory = PalwTirInventoryIndexV1::new(&v.space.program)
        .ok_or_else(|| PalwTirEvidenceErrorV1::Binding("the class has no inventory".into()))?;
    Ok((v, leaf, inventory))
}

/// A source over a builder's store, recording what it reads.
fn store_source<'a, 's>(
    v: &'a PalwTirVerifiedBindingV1,
    ctx: &'a PalwJobContextV2,
    inventory: &'a PalwTirInventoryIndexV1,
    store: &'s dyn PalwTirEvidenceStoreV1,
    before: u64,
    rules: &PalwTirCourtRulesV1,
) -> TirSource<'a, 's> {
    TirSource {
        space: &v.space,
        ctx,
        job: v.job,
        inventory,
        units: Units {
            steps: BTreeMap::new(),
            params: BTreeMap::new(),
            prompt: BTreeMap::new(),
            generated: None,
            store: Some(store),
            prompt_form: rules.prompt_form,
        },
        before,
        used: Requests::default(),
        leaf_cache: BTreeMap::new(),
        supplied_ctx: None,
        supplied: BTreeMap::new(),
        used_supplied: BTreeSet::new(),
    }
}

/// **The carriage of `used`**, in the canonical order: the step leaves as one range-proved row, the
/// artifact openings, the prompt in the network's form, the decode pin.
fn assemble_carriage(
    binding: &PalwTirStepBindingV1,
    output_leaf: u64,
    used: &Requests,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirConeRefutationV1, PalwTirEvidenceErrorV1> {
    let missing = |what: String| PalwTirEvidenceErrorV1::Store(what);
    let output_opening = store.step_opening(output_leaf).ok_or_else(|| missing(format!("the opening of leaf {output_leaf}")))?;
    let output_preimage = store.step_leaf(output_leaf).ok_or_else(|| missing(format!("step leaf {output_leaf}")))?;
    let indices: Vec<u64> = used.steps.iter().copied().collect();
    let mut preimages = Vec::with_capacity(indices.len());
    for i in &indices {
        preimages.push(store.step_leaf(*i).ok_or_else(|| missing(format!("step leaf {i}")))?);
    }
    let mut run_siblings = Vec::new();
    let mut k = 0;
    while k < indices.len() {
        let mut len = 1;
        while k + len < indices.len() && indices[k + len] == indices[k] + len as u64 {
            len += 1;
        }
        run_siblings.push(
            store
                .step_range_siblings(indices[k], len as u64)
                .ok_or_else(|| missing(format!("the range siblings of leaves {}..{}", indices[k], indices[k] + len as u64)))?,
        );
        k += len;
    }
    let params = if used.params.is_empty() {
        None
    } else {
        let leaves: Vec<u32> = used.params.iter().copied().collect();
        let proof = store
            .param_multiproof(&leaves)
            .ok_or_else(|| missing(format!("the multiproof of inventory leaves {}..={}", leaves[0], leaves[leaves.len() - 1])))?;
        // A store whose paths disagree with the class's root would build a close the court refuses:
        // say so here, where the store can be named.
        if proof.opened.iter().map(|(i, _)| *i).ne(leaves.iter().copied())
            || verify_artifact_multiproof_v1(&proof, binding.artifact_root).is_err()
        {
            return Err(missing("a multiproof of the read leaves that reaches the class's artifact root".into()));
        }
        Some(proof)
    };
    let (mut prompt_token_ids, mut prompt_ids_openings) = (Vec::new(), Vec::new());
    if !used.prompt.is_empty() {
        match rules.prompt_form {
            PalwPromptIdsFormV1::Flat => {
                prompt_token_ids = store.prompt_token_ids().ok_or_else(|| missing("the prompt".into()))?;
            }
            PalwPromptIdsFormV1::MerkleV1 => {
                let tiles: BTreeSet<u32> = used.prompt.iter().map(|p| p / PALW_PROMPT_IDS_TILE_LEN).collect();
                for t in tiles {
                    prompt_ids_openings.push(store.prompt_ids_opening(t).ok_or_else(|| missing(format!("prompt tile {t}")))?);
                }
            }
        }
    }
    let decode_tokens = if used.decode { Some(store.decode_pin().ok_or_else(|| missing("the decode pin".into()))?) } else { None };
    Ok(PalwTirConeRefutationV1 {
        binding: binding.clone(),
        output_opening,
        output_preimage,
        operands: PalwStepInputRowV1 { preimages, run_siblings },
        params,
        prompt_token_ids,
        prompt_ids_openings,
        decode_tokens,
    })
}

// ---------------------------------------------------------------------------------------------
// The history dissection (F7): the root claim's finalize, the rounds, the bottom
// ---------------------------------------------------------------------------------------------

use crate::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1, PalwTirRangeClaimV1,
    PalwTirRootClaimV1, palw_tir_dissect_check_claim_v1, palw_tir_dissect_site_v1,
};
use misaka_palw_tir::demand::{DemandRangeRequest, eval_demanded_range};

/// The supplied values of `reductions` (by `(node, element)`) from a claim aligned with `elements`.
fn supplied_values(
    reductions: &[u16],
    elements: &[Vec<u32>],
    claim: &PalwTirRangeClaimV1,
    skip: Option<u16>,
) -> BTreeMap<(u16, usize), i128> {
    let mut out = BTreeMap::new();
    for ((node, es), vs) in reductions.iter().zip(elements).zip(&claim.partials) {
        if Some(*node) == skip {
            continue;
        }
        for (e, v) in es.iter().zip(vs) {
            out.insert((*node, *e as usize), *v);
        }
    }
    out
}

fn leaf_elements(leaf: &PalwTirLeafV1) -> Vec<usize> {
    match leaf.kind {
        PalwTirLeafKindV1::Commit { first_element, .. } => {
            (first_element..first_element + leaf.value_count as u64).map(|e| e as usize).collect()
        }
        _ => Vec::new(),
    }
}

/// **The finalize and the claim's element closure** (spec 04b §9.5): the leaf evaluated with every
/// reduction over `H` supplied; then, until nothing new is read, one position's term (`H` range
/// `0..1`) of each reduction's elements read so far, the other reductions supplied — the values of
/// another reduction an `H`-local node reads (the softmax's `m*` inside `exp(s − m*)`) are read by
/// every term alike, so one term names them. Everything read is recorded in `source`: the units in
/// `used`, the supplied values in `used_supplied`, which is the claim's element set.
fn finalize_and_closure(
    space: &PalwTirStepSpaceV1,
    site: &PalwTirDissectSiteV1,
    leaf: &PalwTirLeafV1,
    source: &mut TirSource<'_, '_>,
    limits: &DemandLimits,
) -> Result<Vec<i128>, DemandError> {
    let values = finalize_values(space, site, &leaf_elements(leaf), source, limits)?;
    loop {
        let before = source.used_supplied.len();
        for node in &site.reductions {
            let demanded: Vec<usize> = source.used_supplied.iter().filter(|(n, _)| n == node).map(|(_, e)| *e).collect();
            if demanded.is_empty() {
                continue;
            }
            let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
            let request =
                DemandRangeRequest { ctx: site.ctx, target: *node, elements: &demanded, supplied: &supplied, range: Some((0, 1)) };
            eval_demanded_range(&space.program, &space.info, &request, source, limits)?;
        }
        if source.used_supplied.len() == before {
            return Ok(values);
        }
    }
}

/// **The finalize** (spec 04b §9.5.3): the tile's `elements` evaluated with every reduction over `H`
/// supplied from the source's claimed values. A commit point that itself reduces over `H` is its
/// own finalize — the tile IS that reduction's totals, so the claimed values are the values, read
/// through the source, which records them as read (it cannot be both the target and a supplied node
/// of one range request: §9.5.2 refuses that, and the honest responder's root claim would be refused
/// with it).
fn finalize_values(
    space: &PalwTirStepSpaceV1,
    site: &PalwTirDissectSiteV1,
    elements: &[usize],
    source: &mut TirSource<'_, '_>,
    limits: &DemandLimits,
) -> Result<Vec<i128>, DemandError> {
    if site.reductions.contains(&site.node) {
        return elements.iter().map(|e| source.node(site.ctx, site.node, *e).map_err(DemandError::from)).collect();
    }
    let request = DemandRangeRequest { ctx: site.ctx, target: site.node, elements, supplied: &site.reductions, range: None };
    Ok(eval_demanded_range(&space.program, &space.info, &request, source, limits)?.0)
}

/// **Admit a root claim** (spec 04b §9.5; the acceptance layer's check, which holds the court's work
/// limits): its carriage checks as a cone close's does, without a conviction (a leaf that convicts on
/// its own is closed, not dissected); the leaf is the one the ladder narrowed to and is dissected;
/// the claim's shape and values are the site's; and the leaf evaluated with every reduction over `H`
/// SUPPLIED from the claim reproduces the committed leaf, reading exactly the carried units and
/// exactly the claimed values. Returns the site the phase opens on.
pub fn check_tir_root_claim_v1(
    root: &PalwTirRootClaimV1,
    narrowed: u64,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirDissectSiteV1, String> {
    if root.version != PALW_TIR_DISSECT_OBJECT_VERSION_V1 {
        return Err(format!("root claim version {} is not {PALW_TIR_DISSECT_OBJECT_VERSION_V1}", root.version));
    }
    let c = match check_carriage(&root.finalize, rules).map_err(|e| e.to_string())? {
        CarriageStage::Convicted(verdict) => {
            return Err(format!("the leaf convicts on its own ({:?}): it is closed, not dissected", verdict.fault));
        }
        CarriageStage::Checked(c) => c,
    };
    if c.out_index != narrowed {
        return Err(format!("the root claim opens leaf {}, the ladder narrowed to {narrowed}", c.out_index));
    }
    let site = palw_tir_dissect_site_v1(&c.v.space, &c.intervals, &c.leaf).ok_or("the narrowed leaf is not dissected")?;
    palw_tir_dissect_check_claim_v1(&site, &root.elements, &root.totals).map_err(|e| e.to_string())?;
    let mut source = c.source(rules);
    source.supplied_ctx = Some(site.ctx);
    source.supplied = supplied_values(&site.reductions, &root.elements, &root.totals, None);
    let values = finalize_and_closure(&c.v.space, &site, &c.leaf, &mut source, &rules.limits)
        .map_err(|e| format!("the finalize does not evaluate: {e}"))?;
    c.check_canonical(&source.used, rules).map_err(|e| e.to_string())?;
    if source.used_supplied.len() != source.supplied.len() {
        return Err("the claim carries values the dissection never reads".into());
    }
    if c.first_difference(&values).is_some() {
        return Err("the root claim does not finalize to the committed leaf".into());
    }
    Ok(site)
}

/// **The bottom of an IR dissection** (`PalwCourtVerdictProofV2::TirDissection`): the carriage checks
/// as a cone close's does (its convictions stand); the leaf is the phase's and the site unchanged;
/// then every reduction `r_i` is evaluated over the terminal tile's positions only, the other
/// reductions supplied from the ROOT's totals (spec 04b §9.5), reading exactly the carried units, and
/// compared with the claim the dissection narrowed to: the first differing value (in reduction, then
/// element order) convicts ([`PalwStepFaultV1::ComputationMismatch`], kind 5, the leaf); none is
/// `NoFaultFound`.
pub fn check_tir_dissect_bottom_v1(
    phase: &PalwTirDissectPhaseV1,
    bottom: &PalwTirConeRefutationV1,
    narrowed: u64,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    let range = phase.terminal_range().ok_or(bad("the dissection has not narrowed to one tile"))?;
    let c = match check_carriage(bottom, rules)? {
        CarriageStage::Convicted(verdict) => return Ok(verdict),
        CarriageStage::Checked(c) => c,
    };
    if c.out_index != narrowed || narrowed != phase.leaf_index() {
        return Err(bad("the bottom opens another leaf than the dissection's"));
    }
    let site = palw_tir_dissect_site_v1(&c.v.space, &c.intervals, &c.leaf).ok_or(bad("the leaf is not dissected"))?;
    if site.reductions != phase.reductions() || site.history_positions != phase.history_positions() {
        return Err(bad("the bottom's site is not the phase's"));
    }
    let mut source = c.source(rules);
    source.supplied_ctx = Some(site.ctx);
    let mut partials: Vec<i128> = Vec::new();
    for (i, node) in site.reductions.iter().enumerate() {
        source.supplied = supplied_values(&site.reductions, phase.elements(), phase.root(), Some(*node));
        let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
        let elements: Vec<usize> = phase.elements()[i].iter().map(|e| *e as usize).collect();
        let request =
            DemandRangeRequest { ctx: site.ctx, target: *node, elements: &elements, supplied: &supplied, range: Some(range) };
        let (values, _) = eval_demanded_range(&c.v.space.program, &c.v.space.info, &request, &mut source, &rules.limits)
            .map_err(|e| evaluation_refusal(&e))?;
        partials.extend(values);
    }
    c.check_canonical(&source.used, rules)?;
    let claimed: Vec<i128> = phase.claim().partials.iter().flatten().copied().collect();
    match partials.iter().zip(claimed.iter()).position(|(a, b)| a != b).or((partials.len() != claimed.len()).then_some(0)) {
        Some(i) => {
            let fault = PalwStepFaultV1::ComputationMismatch { value_index: i as u32 };
            Ok(convict(&bottom.binding.committed_execution_root, PALW_TIR_EVIDENCE_KIND_CONE, narrowed, fault))
        }
        None => Err(PalwStepRefuteError::NoFaultFound),
    }
}

/// **The site a root claim opens on, as the fold derives it** (spec 04b §9.5.1): the binding the
/// claim carries verified at its own canonical count, the leaf it opens the one the ladder narrowed
/// to, and that leaf's site from the class's program — no carriage read, no evaluation. The claim's
/// pins (class, artifact root, roots) are the caller's, and the finalize is the acceptance layer's
/// ([`check_tir_root_claim_v1`], at the court's limits); what the fold needs is the site, which
/// nothing a mover supplies may choose.
pub fn palw_tir_root_claim_site_v1(root: &PalwTirRootClaimV1, narrowed: u64) -> Result<PalwTirDissectSiteV1, String> {
    let binding = &root.finalize.binding;
    let v = crate::palw_tir_step_v1::verify_tir_binding_v1(binding, binding.step_leaf_count).map_err(|e| e.to_string())?;
    if root.finalize.output_opening.leaf_index != narrowed {
        return Err(format!(
            "the root claim opens leaf {}, the ladder narrowed to {narrowed}",
            root.finalize.output_opening.leaf_index
        ));
    }
    let leaf = v.space.leaf_at(&binding.job_context, narrowed).ok_or_else(|| format!("{narrowed} is not a leaf of this execution"))?;
    let intervals = analyze_ranges(&v.space.program).map_err(|e| e.to_string())?;
    palw_tir_dissect_site_v1(&v.space, &intervals, &leaf).ok_or_else(|| "the narrowed leaf is not dissected".to_string())
}

/// **Is a cone close's leaf a dissected leaf of its execution?** (spec 04b §9.5.1; the held
/// regime's one-move court, ADR-0103 Decision 5.) Steps 1–4 of [`check_tir_cone_refutation_v1`] at
/// the binding's own canonical count — the binding verifies, the leaf opens under it, and neither
/// the binding, the leaf's structure nor its lanes' interval convicts on its face — then the leaf's
/// site: `Some(leaf index)` when its cone reduces over `H`. `Ok(None)` for a leaf that is not
/// dissected and for one those steps convict (the whole close convicts it, cheaply, without an
/// evaluation); `Err` for evidence about another execution. Reads no operand: the accusation names
/// the leaf, and the dissection it opens is where its cone is argued.
pub fn palw_tir_named_dissected_leaf_v1(refutation: &PalwTirConeRefutationV1) -> Result<Option<u64>, PalwStepRefuteError> {
    let binding = &refutation.binding;
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(_) => return Ok(None),
        BindingOutcome::Verified(v) => v,
    };
    let leaf = match check_output_leaf(binding, &v, &refutation.output_opening, &refutation.output_preimage, binding.step_leaf_count)?
    {
        Ok(leaf) => leaf,
        Err(_) => return Ok(None),
    };
    let intervals = analyze_ranges(&v.space.program).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
    let out = palw_tir_leaf_interval_v1(&v.space, &intervals, &leaf).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if first_outside(leaf.dtype, &refutation.output_preimage.values_le, out).is_some() {
        return Ok(None);
    }
    Ok(palw_tir_dissect_site_v1(&v.space, &intervals, &leaf).map(|_| refutation.output_opening.leaf_index))
}

/// **A cone close that names a leaf and carries nothing else** — the binding, the leaf's opening
/// and preimage, no operand: the proof of a one-move accusation at a dissected leaf under the held
/// regime, where the accusation opens a dissection rather than being adjudicated
/// ([`palw_tir_named_dissected_leaf_v1`]).
pub fn build_tir_named_leaf_refutation_v1(
    binding: &PalwTirStepBindingV1,
    leaf: u64,
    store: &dyn PalwTirEvidenceStoreV1,
) -> Result<PalwTirConeRefutationV1, PalwTirEvidenceErrorV1> {
    let missing = |what: String| PalwTirEvidenceErrorV1::Store(what);
    Ok(PalwTirConeRefutationV1 {
        binding: binding.clone(),
        output_opening: store.step_opening(leaf).ok_or_else(|| missing(format!("the opening of leaf {leaf}")))?,
        output_preimage: store.step_leaf(leaf).ok_or_else(|| missing(format!("step leaf {leaf}")))?,
        operands: PalwStepInputRowV1 { preimages: Vec::new(), run_siblings: Vec::new() },
        params: None,
        prompt_token_ids: Vec::new(),
        prompt_ids_openings: Vec::new(),
        decode_tokens: None,
    })
}

/// The site of `leaf` for a builder.
fn builder_site(v: &PalwTirVerifiedBindingV1, leaf: &PalwTirLeafV1) -> Result<PalwTirDissectSiteV1, PalwTirEvidenceErrorV1> {
    let intervals = analyze_ranges(&v.space.program).map_err(|e| PalwTirEvidenceErrorV1::Binding(e.to_string()))?;
    palw_tir_dissect_site_v1(&v.space, &intervals, leaf)
        .ok_or_else(|| PalwTirEvidenceErrorV1::Binding("the leaf is not dissected".into()))
}

/// **Build the responder's root claim** for the narrowed leaf from its own execution: every
/// reduction's honest totals, the elements the finalize reads, and the finalize's carriage.
pub fn build_tir_root_claim_v1(
    binding: &PalwTirStepBindingV1,
    narrowed: u64,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirRootClaimV1, PalwTirEvidenceErrorV1> {
    let (v, leaf, inventory) = builder_prelude(binding, narrowed)?;
    let site = builder_site(&v, &leaf)?;
    let eval_err = |e: DemandError| PalwTirEvidenceErrorV1::Evaluation(e.to_string());
    // Every element of every reduction, computed whole (the builder's own work, not the court's).
    let mut all = BTreeMap::new();
    for (node, count) in site.reductions.iter().zip(&site.counts) {
        let mut source = store_source(&v, &binding.job_context, &inventory, store, narrowed, rules);
        let elements: Vec<usize> = (0..*count as usize).collect();
        let request = DemandRequest { target: DemandTarget::Node { ctx: site.ctx, node: *node }, elements: &elements };
        let (values, _) =
            eval_demanded(&v.space.program, &v.space.info, &request, &mut source, &DemandLimits::UNLIMITED).map_err(eval_err)?;
        for (e, value) in values.into_iter().enumerate() {
            all.insert((*node, e), value);
        }
    }
    // The finalize and its closure, with every total supplied: what they read is the claim's element set.
    let mut source = store_source(&v, &binding.job_context, &inventory, store, narrowed, rules);
    source.supplied_ctx = Some(site.ctx);
    source.supplied = all;
    finalize_and_closure(&v.space, &site, &leaf, &mut source, &rules.limits).map_err(eval_err)?;
    let mut claim_elements = vec![Vec::new(); site.reductions.len()];
    let mut totals = vec![Vec::new(); site.reductions.len()];
    for (node, e) in &source.used_supplied {
        let i = site.reductions.iter().position(|r| r == node).expect("a reduction of the site");
        claim_elements[i].push(*e as u32);
        totals[i].push(source.supplied[&(*node, *e)]);
    }
    let finalize = assemble_carriage(binding, narrowed, &source.used, store, rules)?;
    Ok(PalwTirRootClaimV1 {
        version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
        elements: claim_elements,
        totals: PalwTirRangeClaimV1 { partials: totals },
        finalize: Box::new(finalize),
    })
}

/// **What a root claim finalizes to**: the leaf's values evaluated with every reduction over `H`
/// supplied from `(elements, totals)` — the lanes the leaf must hold for the claim to be admitted.
/// A tool's function; the court runs the same evaluation inside [`check_tir_root_claim_v1`].
pub fn tir_root_claim_finalizes_to_v1(
    binding: &PalwTirStepBindingV1,
    narrowed: u64,
    elements: &[Vec<u32>],
    totals: &PalwTirRangeClaimV1,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<Vec<i128>, PalwTirEvidenceErrorV1> {
    let (v, leaf, inventory) = builder_prelude(binding, narrowed)?;
    let site = builder_site(&v, &leaf)?;
    let mut source = store_source(&v, &binding.job_context, &inventory, store, narrowed, rules);
    source.supplied_ctx = Some(site.ctx);
    source.supplied = supplied_values(&site.reductions, elements, totals, None);
    finalize_values(&v.space, &site, &leaf_elements(&leaf), &mut source, &rules.limits)
        .map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))
}

/// The partials of every reduction over the history positions `range`, the others supplied from the
/// phase's root — the responder's round and the bottom's evaluation, over a builder's store.
fn builder_partials(
    binding: &PalwTirStepBindingV1,
    phase: &PalwTirDissectPhaseV1,
    range: (usize, usize),
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<(PalwTirRangeClaimV1, Requests), PalwTirEvidenceErrorV1> {
    let (v, leaf, inventory) = builder_prelude(binding, phase.leaf_index())?;
    let site = builder_site(&v, &leaf)?;
    let mut source = store_source(&v, &binding.job_context, &inventory, store, phase.leaf_index(), rules);
    source.supplied_ctx = Some(site.ctx);
    let mut partials = Vec::with_capacity(site.reductions.len());
    for (i, node) in site.reductions.iter().enumerate() {
        source.supplied = supplied_values(&site.reductions, phase.elements(), phase.root(), Some(*node));
        let supplied: Vec<u16> = site.reductions.iter().copied().filter(|r| r != node).collect();
        let elements: Vec<usize> = phase.elements()[i].iter().map(|e| *e as usize).collect();
        let request =
            DemandRangeRequest { ctx: site.ctx, target: *node, elements: &elements, supplied: &supplied, range: Some(range) };
        let (values, _) = eval_demanded_range(&v.space.program, &v.space.info, &request, &mut source, &rules.limits)
            .map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
        partials.push(values);
    }
    Ok((PalwTirRangeClaimV1 { partials }, source.used))
}

/// **Build the responder's round**: every child of the disputed range, its partials over the
/// child's positions.
pub fn build_tir_dissect_round_v1(
    binding: &PalwTirStepBindingV1,
    phase: &PalwTirDissectPhaseV1,
    tile_positions: u32,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirDissectRoundV1, PalwTirEvidenceErrorV1> {
    let h = phase.history_positions() as u64;
    let mut children = Vec::new();
    for (first, count) in phase.child_ranges() {
        let from = first * tile_positions as u64;
        let to = ((first + count) * tile_positions as u64).min(h);
        children.push(builder_partials(binding, phase, (from as usize, to as usize), store, rules)?.0);
    }
    Ok(PalwTirDissectRoundV1 { version: PALW_TIR_DISSECT_OBJECT_VERSION_V1, children })
}

/// **Build the bottom close's carriage** over the terminal tile.
pub fn build_tir_dissect_bottom_v1(
    binding: &PalwTirStepBindingV1,
    phase: &PalwTirDissectPhaseV1,
    store: &dyn PalwTirEvidenceStoreV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwTirConeRefutationV1, PalwTirEvidenceErrorV1> {
    let range = phase.terminal_range().ok_or_else(|| PalwTirEvidenceErrorV1::Binding("the dissection has no bottom yet".into()))?;
    let (_, used) = builder_partials(binding, phase, range, store, rules)?;
    assemble_carriage(binding, phase.leaf_index(), &used, store, rules)
}

// ---------------------------------------------------------------------------------------------
// Logits consistency (fault 20) and the decode-token door
// ---------------------------------------------------------------------------------------------

/// **The same row's lanes in the committed logits trace**, in the class's scheme.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwTirTraceLanesV1 {
    /// The flat scheme: every row and id (one keyed hash over all of them — no row opens alone).
    Flat(PalwBase0DecodeTokensV1),
    /// The tiled scheme: the ids, the row's root and its opening in the rows tree, and the tile's
    /// lanes and their opening in the row's tile tree.
    Tiled {
        generated_token_ids: Vec<u32>,
        row_root: Hash64,
        row_opening: PalwStepOpeningV1,
        tile_lanes: Vec<i32>,
        tile_opening: PalwStepOpeningV1,
    },
}

/// **An IR execution's two commitments to one logits row, disagreeing**
/// (`PalwCourtVerdictProofV2::TirLogits`'s payload). The logits node is a commit point (NF-6) and
/// the trace commits the same rows under the class's scheme; the tokens the job generated are read
/// from the trace, the program's arithmetic is adjudicated on the step leaves, and this is what ties
/// the two: a step tile and a trace tile of one row that differ convict the executor, who committed
/// both.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirLogitsConsistencyV1 {
    pub binding: PalwTirStepBindingV1,
    /// A tile of the logits node, at a position whose logits select a token (`a ≥ P − 1`).
    pub step_opening: PalwStepOpeningV1,
    pub step_preimage: PalwStepTileLeafV1,
    pub trace: PalwTirTraceLanesV1,
}

/// **Adjudicate a logits-consistency accusation.** `Ok(verdict)` convicts
/// ([`PalwStepFaultV1::TirLogitsTraceMismatch`], evidence kind 7, the step leaf's index) at the
/// first differing lane; `Err(NoFaultFound)` acquits; any other `Err` refuses. The binding and the
/// step leaf are checked as a cone close checks them (the same convictions, first); the step leaf
/// must be a tile of the logits node, and the trace lanes must authenticate against the claim's own
/// trace root under the class's scheme.
pub fn check_tir_logits_consistency_v1(
    accusation: &PalwTirLogitsConsistencyV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    let binding = &accusation.binding;
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(verdict) => return Ok(verdict),
        BindingOutcome::Verified(v) => v,
    };
    let leaf = match check_output_leaf(binding, &v, &accusation.step_opening, &accusation.step_preimage, rules.max_step_leaf_count)? {
        Ok(leaf) => leaf,
        Err(verdict) => return Ok(verdict),
    };
    let p = &v.space.program;
    let post_occurrence = (v.space.occurrences().len() - 1) as u32;
    let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
        return Err(bad("the step leaf is not a tile of the logits node"));
    };
    if occurrence != post_occurrence || node != p.logits {
        return Err(bad("the step leaf is not a tile of the logits node"));
    }
    let row = leaf.position + 1 - v.job.prefill; // `a ≥ P − 1` for every post leaf
    let step_lanes: Vec<i128> = (0..leaf.value_count as usize)
        .map(|i| lane(leaf.dtype, &accusation.step_preimage.values_le, i).unwrap_or(i128::MIN))
        .collect();
    let (vocab, scheme) = logits_shape(&v.space);
    let trace_lanes: Vec<i128> = match &accusation.trace {
        PalwTirTraceLanesV1::Flat(pin) => {
            if scheme != flat_logits_scheme_id_v1() {
                return Err(bad("the flat form is not the class's scheme"));
            }
            check_flat_pin(binding, vocab, pin)?;
            let r = pin.logits_rows.get(row as usize).ok_or(bad("the row is past the decode count"))?;
            let first = first_element as usize;
            r.get(first..first + leaf.value_count as usize)
                .ok_or(bad("the tile is past the row"))?
                .iter()
                .map(|x| *x as i128)
                .collect()
        }
        PalwTirTraceLanesV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening } => {
            if scheme != tiled_logits_scheme_id_v1() {
                return Err(bad("the tiled form is not the class's scheme"));
            }
            let ctx = &binding.job_context;
            let decode = ctx.exact_decode_tokens as u64;
            if generated_token_ids.len() as u64 != decode || row as u64 >= decode {
                return Err(bad("the ids or the row are not the context's decode count"));
            }
            let rows_root = step_opening_root_capped_v1(decode, row_opening, rules.max_step_leaf_count)
                .map_err(|_| bad("the row opening does not walk"))?;
            if row_opening.leaf_index != row as u64 || row_opening.leaf_hash != *row_root {
                return Err(bad("the row opening does not open the row's root"));
            }
            if tiled_logits_outer_root_v1(ctx, decode, &rows_root, generated_token_ids) != binding.full_logits_trace_root {
                return Err(bad("the carried material does not reproduce the claim's own tiled trace root"));
            }
            // The layout tiles the logits node at a divisor of the scheme's width (F4, decision (1) of
            // 2026-09-28), so a step tile lies inside one trace tile, at an offset in it.
            let tile = first_element / PALW_LOGITS_TILE_LANES as u64;
            let offset = (first_element % PALW_LOGITS_TILE_LANES as u64) as usize;
            let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64;
            tiled_tile_authenticate_v1(
                &v.context_hash,
                row,
                vocab,
                tiles,
                row_root,
                tile,
                tile_lanes,
                tile_opening,
                rules.max_step_leaf_count,
            )?;
            tile_lanes
                .get(offset..offset + leaf.value_count as usize)
                .ok_or(bad("the step tile is past its trace tile"))?
                .iter()
                .map(|x| *x as i128)
                .collect()
        }
    };
    if trace_lanes.len() != step_lanes.len() {
        return Err(bad("the trace tile is not the step tile's width"));
    }
    if let Some(i) = step_lanes.iter().zip(trace_lanes.iter()).position(|(a, b)| a != b) {
        let fault = PalwStepFaultV1::TirLogitsTraceMismatch { value_index: i as u32 };
        return Ok(convict(
            &binding.committed_execution_root,
            PALW_TIR_EVIDENCE_KIND_LOGITS,
            accusation.step_opening.leaf_index,
            fault,
        ));
    }
    Err(PalwStepRefuteError::NoFaultFound)
}

/// **The decode-token door for an IR class, tiled scheme** — the legacy tiled arm's rule over the IR
/// binding: the pin authenticates against the claim's own trace root, the committed token and the
/// beating lane are read from their opened tiles, and the token is refuted when the lane beats it
/// (greedy selection: strictly greater, ties to the lower index). A committed token outside the
/// vocabulary is itself refuted — no selection over the row produces it.
pub fn check_tir_decode_token_tiled_v1(
    binding: &PalwTirStepBindingV1,
    pin: &PalwTiledDecodePinV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(verdict) => return Ok(verdict),
        BindingOutcome::Verified(v) => v,
    };
    let (vocab, scheme) = logits_shape(&v.space);
    if scheme != tiled_logits_scheme_id_v1() {
        return Err(bad("this class does not commit tiled logits"));
    }
    let ctx = &binding.job_context;
    let decode = ctx.exact_decode_tokens as u64;
    if pin.generated_token_ids.len() as u64 != decode || pin.position as u64 >= decode {
        return Err(bad("the ids or the challenged position are not the context's decode count"));
    }
    let rows_root = step_opening_root_capped_v1(decode, &pin.row_opening, rules.max_step_leaf_count)
        .map_err(|_| bad("the row opening does not walk"))?;
    if pin.row_opening.leaf_index != pin.position as u64 || pin.row_opening.leaf_hash != pin.row_root {
        return Err(bad("the row opening does not open the challenged row's root"));
    }
    if tiled_logits_outer_root_v1(ctx, decode, &rows_root, &pin.generated_token_ids) != binding.full_logits_trace_root {
        return Err(bad("the carried material does not reproduce the claim's own tiled trace root"));
    }
    let fault = PalwStepFaultV1::DecodeTokenMismatch { position: pin.position };
    let verdict = || convict(&binding.committed_execution_root, PALW_DECODE_TOKEN_EVIDENCE_KIND, pin.position as u64, fault);
    let committed = pin.generated_token_ids[pin.position as usize] as usize;
    if committed >= vocab {
        return Ok(verdict());
    }
    let beat = pin.beat_lane as usize;
    if beat >= vocab {
        return Err(bad("the beating lane is past the vocabulary"));
    }
    let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64;
    let read = |lanes: &[i32], opening: &PalwStepOpeningV1, lane: usize| -> Result<i32, PalwStepRefuteError> {
        let tile = (lane / PALW_LOGITS_TILE_LANES) as u64;
        tiled_tile_authenticate_v1(
            &v.context_hash,
            pin.position,
            vocab,
            tiles,
            &pin.row_root,
            tile,
            lanes,
            opening,
            rules.max_step_leaf_count,
        )?;
        Ok(lanes[lane % PALW_LOGITS_TILE_LANES])
    };
    let v_committed = read(&pin.committed_tile_lanes, &pin.committed_opening, committed)?;
    let v_beat = read(&pin.beat_tile_lanes, &pin.beat_opening, beat)?;
    let greedy = crate::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY;
    if crate::palw_decode_select_v2::decode_lane_beats_v2(
        greedy.lane_key(v_beat, pin.position, beat),
        beat,
        greedy.lane_key(v_committed, pin.position, committed),
        committed,
    ) {
        return Ok(verdict());
    }
    Err(PalwStepRefuteError::NoFaultFound)
}

/// **The decode-token door for an IR class, flat scheme**: the whole pin authenticates against the
/// claim's own trace root, and the token at `position` is refuted unless it is the greedy selection
/// over its own row.
pub fn check_tir_decode_token_flat_v1(
    binding: &PalwTirStepBindingV1,
    pin: &PalwBase0DecodeTokensV1,
    position: u32,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(verdict) => return Ok(verdict),
        BindingOutcome::Verified(v) => v,
    };
    let (vocab, scheme) = logits_shape(&v.space);
    if scheme != flat_logits_scheme_id_v1() {
        return Err(bad("this class does not commit flat logits"));
    }
    check_flat_pin(binding, vocab, pin)?;
    let row = pin.logits_rows.get(position as usize).ok_or(bad("the challenged position is outside the job's decode calls"))?;
    let expected = crate::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY.select(row, position) as u32;
    if pin.generated_token_ids[position as usize] != expected {
        let fault = PalwStepFaultV1::DecodeTokenMismatch { position };
        return Ok(convict(&binding.committed_execution_root, PALW_DECODE_TOKEN_EVIDENCE_KIND, position as u64, fault));
    }
    Err(PalwStepRefuteError::NoFaultFound)
}

// ---------------------------------------------------------------------------------------------
// Data availability: one event of an IR claim's logits trace, disclosed (F6 D)
// ---------------------------------------------------------------------------------------------

/// **What a data-availability answer opens for an IR claim: one event of its committed logits
/// trace, in the form the class's scheme names** — the IR twin of
/// [`crate::palw_step_refute::PalwTraceEventDisclosureV1`] (ADR-0062 SA-2), carrying the IR binding
/// in place of the legacy one.
///
/// A separate type, carried by `PalwDaAnswerV1::TirEvent` (appended), rather than variants appended
/// to the legacy disclosure: that one also rides offence evidence (`LogitsNotStepOutput`) and replay
/// refutations, whose readers decode it with the tags they have — an IR variant there would be a
/// payload one build decodes and another does not, inside objects whose other readers never learn
/// about the fence. Here it rides exactly one object, `MaterialDisclosedV2`, which the acceptance
/// layer drops by name below `palw_tir_v1`.
///
/// Every variant carries the claim's own binding, checked as the legacy disclosure's is: it must
/// verify ([`verify_tir_binding_v1`]) and name the claim's trace root and execution root. Nothing
/// here runs a model. An event is `(row, tile)`: a decode position and a tile of that row's logits.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub enum PalwTirTraceEventDisclosureV1 {
    /// The flat scheme: every row and every id (one keyed hash over all of them — no row opens
    /// alone), authenticated against the claim's flat trace root.
    Flat { binding: Box<PalwTirStepBindingV1>, pin: PalwBase0DecodeTokensV1 },
    /// The tiled scheme: the generated ids (keyed into the outer root), the accused row's root and
    /// its opening in the rows tree, and the accused tile's lanes and their opening in the row's
    /// tile tree.
    Tiled {
        binding: Box<PalwTirStepBindingV1>,
        generated_token_ids: Vec<u32>,
        row_root: Hash64,
        row_opening: PalwStepOpeningV1,
        tile_lanes: Vec<i32>,
        tile_opening: PalwStepOpeningV1,
    },
    /// The accused event is not in the committed run: a row past the decode count, or a tile past
    /// the row's vocabulary (any tile but 0 on the flat scheme). The binding alone proves it.
    OutOfRange { binding: Box<PalwTirStepBindingV1> },
}

impl PalwTirTraceEventDisclosureV1 {
    /// The IR binding every variant carries — what the identity rule (J-5) reads.
    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        match self {
            Self::Flat { binding, .. } | Self::Tiled { binding, .. } | Self::OutOfRange { binding } => binding,
        }
    }

    /// A flat answer carries every row, so it answers every in-run event at once.
    pub fn is_flat(&self) -> bool {
        matches!(self, Self::Flat { .. })
    }

    fn binding_mut(&mut self) -> &mut PalwTirStepBindingV1 {
        match self {
            Self::Flat { binding, .. } | Self::Tiled { binding, .. } | Self::OutOfRange { binding } => binding,
        }
    }

    /// **Empties the carried program** — what a discloser does before the answer rides (the chain
    /// holds the registered class's program and refuses a carried one).
    pub fn strip_program_v1(&mut self) {
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(self.binding_mut());
    }

    /// The disclosure with the registered class's program put back
    /// ([`crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1`]); refused when it carried one.
    pub fn with_program_v1(&self, record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1) -> Result<Self, &'static str> {
        let mut filled = self.clone();
        let slot = filled.binding_mut();
        *slot = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(slot, record)?;
        Ok(filled)
    }
}

/// **Verify an IR claim's data-availability disclosure against the claim — by hash arithmetic,
/// never by execution** (the IR twin of `check_trace_event_disclosure_v1`).
///
/// `claim_trace_root` and `claim_execution_root` are the claim record's pinned fields; `row` and
/// `tile` the accused event. `Ok(())` refutes the accusation. An `Err` is a disclosure that is not an
/// answer, never a verdict against the producer. In order: the binding verifies at
/// `max_step_leaf_count` and names the claim's two roots; then the variant's own rule under the
/// class's scheme (a variant of the other scheme is refused).
pub fn check_tir_trace_event_disclosure_v1(
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    row: u32,
    tile: u8,
    disclosure: &PalwTirTraceEventDisclosureV1,
    max_step_leaf_count: u64,
) -> Result<(), PalwStepRefuteError> {
    let binding = disclosure.binding();
    let v = crate::palw_tir_step_v1::verify_tir_binding_v1(binding, max_step_leaf_count)
        .map_err(|_| bad("the IR binding does not verify"))?;
    if binding.full_logits_trace_root != claim_trace_root {
        return Err(bad("the disclosure binds to another trace root than the claim committed"));
    }
    if binding.committed_execution_root != claim_execution_root {
        return Err(bad("the disclosure binds to another execution root than the claim committed"));
    }
    let ctx = &binding.job_context;
    let decode = ctx.exact_decode_tokens;
    let (vocab, scheme) = logits_shape(&v.space);
    let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64;
    match disclosure {
        PalwTirTraceEventDisclosureV1::Flat { pin, .. } => {
            if scheme != flat_logits_scheme_id_v1() {
                return Err(bad("this class does not commit flat logits"));
            }
            check_flat_pin(binding, vocab, pin)?;
            if tile != 0 {
                return Err(bad("the flat scheme has one tile per row; an accused tile past it is answered by OutOfRange"));
            }
            if row >= decode {
                return Err(bad("the accused row is past the committed run; it is answered by OutOfRange, not opened"));
            }
            Ok(())
        }
        PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } => {
            if scheme != tiled_logits_scheme_id_v1() {
                return Err(bad("this class does not commit tiled logits"));
            }
            if generated_token_ids.len() as u64 != decode as u64 {
                return Err(bad("the id count is not the context's decode count"));
            }
            if row >= decode {
                return Err(bad("the row is past the decode count"));
            }
            let rows_root = step_opening_root_capped_v1(decode as u64, row_opening, max_step_leaf_count)
                .map_err(|_| bad("the row opening does not walk"))?;
            if row_opening.leaf_index != row as u64 || row_opening.leaf_hash != *row_root {
                return Err(bad("the row opening does not open the named row's root"));
            }
            if tiled_logits_outer_root_v1(ctx, decode as u64, &rows_root, generated_token_ids) != binding.full_logits_trace_root {
                return Err(bad("the carried material does not reproduce the claim's own trace root"));
            }
            tiled_tile_authenticate_v1(
                &v.context_hash,
                row,
                vocab,
                tiles,
                row_root,
                tile as u64,
                tile_lanes,
                tile_opening,
                max_step_leaf_count,
            )
        }
        PalwTirTraceEventDisclosureV1::OutOfRange { .. } => {
            let out = if scheme == flat_logits_scheme_id_v1() {
                row >= decode || tile != 0
            } else if scheme == tiled_logits_scheme_id_v1() {
                row >= decode || tile as u64 >= tiles
            } else {
                return Err(bad("this class commits under a scheme no disclosure form names"));
            };
            if out {
                Ok(())
            } else {
                Err(bad("the accused event is inside the committed run; it must be opened, not declared absent"))
            }
        }
    }
}

/// **Build the disclosure of event `(row, logits_tile)` from the rows the producer retained** —
/// the prover's half of [`check_tir_trace_event_disclosure_v1`] (the IR twin of
/// `logits_event_disclosure_v1`): `Flat` with every row and id for the flat scheme (its one tile is
/// 0), `Tiled` with the row and tile openings otherwise. `None` when the event is not in the rows
/// (the caller then answers `OutOfRange`), the program does not decode, or its scheme is neither.
pub fn tir_logits_event_disclosure_v1(
    binding: &PalwTirStepBindingV1,
    logits_rows: &[Vec<i32>],
    generated: &[u32],
    row: u32,
    logits_tile: u8,
) -> Option<PalwTirTraceEventDisclosureV1> {
    let program = binding.class.decode_program().ok()?;
    let scheme = Hash64::from_bytes(program.logits_scheme_id);
    if scheme == flat_logits_scheme_id_v1() {
        if logits_tile != 0 || row as usize >= logits_rows.len() {
            return None;
        }
        return Some(PalwTirTraceEventDisclosureV1::Flat {
            binding: Box::new(binding.clone()),
            pin: PalwBase0DecodeTokensV1 { logits_rows: logits_rows.to_vec(), generated_token_ids: generated.to_vec() },
        });
    }
    if scheme != tiled_logits_scheme_id_v1() {
        return None;
    }
    let (row_root, row_opening, tile_lanes, tile_opening) =
        crate::palw_step_refute::tiled_trace_event_disclosure_v1(&binding.job_context, logits_rows, row, logits_tile)?;
    Some(PalwTirTraceEventDisclosureV1::Tiled {
        binding: Box::new(binding.clone()),
        generated_token_ids: generated.to_vec(),
        row_root,
        row_opening,
        tile_lanes,
        tile_opening,
    })
}

/// A tiny IR class and an honest (or single-lane forged) execution of it, for the court's wiring
/// tests elsewhere in the crate: an embedding, one layer with a saturating running sum (a `Fixed`
/// state, so checkpoints and replay occur) and a projection, and a tiled-logits head.
// ---------------------------------------------------------------------------------------------
// Data availability: one committed step leaf of an IR claim, disclosed (the second IR fence)
// ---------------------------------------------------------------------------------------------

/// **What a `TirStepLeaf { index }` data-availability answer opens** (past `Params::palw_tir_fence2`;
/// evidence transport C): committed step leaf `index` — its preimage and its opening under the step
/// root — with what a challenger builds its court moves from beside the leaf: the claim's generated
/// ids in the class's scheme, and, for a tile of the post block's logits node at a decode row under the
/// tiled scheme, that row's pin aimed at the tile's first lane (the logits door's material). The
/// binding rides with its program EMPTY (the chain holds the class's).
///
/// So a lie a seat's replay finds at leaf `index` becomes a cone close, a named-leaf challenge or a
/// logits door the seat can build from its own execution plus this answer, and a producer that does
/// not answer inside `W_disclose` defaults.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirStepLeafDisclosureV1 {
    pub binding: PalwTirStepBindingV1,
    pub preimage: PalwStepTileLeafV1,
    pub opening: PalwStepOpeningV1,
    /// The generated ids under the class's scheme: `TiledV1 { rows_root, ids }`, or `Base0V1` with
    /// every row (the flat scheme), authenticated against the claim's trace root.
    pub decode: PalwDecodeTokenPinV1,
    /// Exactly for a tile of the logits node at a decode row, tiled scheme: the row's pin aimed at the
    /// tile's first lane; `None` otherwise.
    pub row_pin: Option<PalwTiledDecodePinV1>,
}

impl PalwTirStepLeafDisclosureV1 {
    /// **Empties the carried program** — what a discloser does before the answer rides.
    pub fn strip_program_v1(&mut self) {
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut self.binding);
    }

    /// The disclosure with the registered class's program put back; refused when it carried one.
    pub fn with_program_v1(&self, record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1) -> Result<Self, &'static str> {
        let mut filled = self.clone();
        filled.binding = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(&self.binding, record)?;
        Ok(filled)
    }
}

// ---------------------------------------------------------------------------------------------
// The step tree's nodes: the second IR fence's descent unit `TirStepNode { level, index }`
// ---------------------------------------------------------------------------------------------

/// **Levels a `TirStepNode` answer covers**: its frontier is the nodes this many levels below the
/// demanded node (the leaf nodes, when nearer) — at most 2^10 hashes, 64 KiB, so the answer rides one
/// 100,000-byte carrier with its opening, binding and signature — so one session descends ten levels
/// and one seat's four sessions reach the first divergent leaf of a 2^30-leaf execution: three node
/// sessions and a leaf session ([`PALW_TIR_DA_SEAT_REACH_LEAVES_V1`]). A real-size class's job is
/// 2^24–2^29 leaves (Llama-3.1-70B at 2,048 positions and 64-lane tiles: 450 M).
pub const PALW_TIR_STEP_NODE_DEPTH_V1: u8 = 10;

/// **The most step leaves one seat's data-availability budget descends** — its sessions a claim
/// (`PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1`, four) less the leaf's, each [`PALW_TIR_STEP_NODE_DEPTH_V1`]
/// levels: 2^30. Past `palw_tir_fence2` admission refuses a class whose canonical job commits more
/// (`TirExceeds { limit: "IR DA seat reach" }`), so one honest seat can reach every leaf of every
/// attempt of every class the chain admits.
pub const PALW_TIR_DA_SEAT_REACH_LEAVES_V1: u64 =
    1 << ((crate::palw_da_rcore_v1::PALW_DA_SESSIONS_PER_SEAT_PER_CLAIM_V1 as u32 - 1) * PALW_TIR_STEP_NODE_DEPTH_V1 as u32);

/// **The step tree's width at `level`** (0 = the leaf nodes; each level `⌈w / 2⌉`, an odd last node
/// promoted), or `None` past the root or for an empty tree.
pub fn palw_tir_step_tree_width_v1(leaf_count: u64, level: u8) -> Option<u64> {
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

/// **The root's level**: how many times the leaf nodes fold to one.
pub fn palw_tir_step_tree_height_v1(leaf_count: u64) -> u8 {
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
    level.saturating_sub(PALW_TIR_STEP_NODE_DEPTH_V1)
}

/// The positions node `(level, index)` covers at `below ≤ level`: `[lo, hi)`, or `None` when the node
/// is past the tree.
fn covered(leaf_count: u64, level: u8, index: u64, below: u8) -> Option<(u64, u64)> {
    if index >= palw_tir_step_tree_width_v1(leaf_count, level)? {
        return None;
    }
    let shift = u32::from(level - below);
    let lo = index.checked_shl(shift)?;
    let hi = index.checked_add(1)?.checked_shl(shift)?.min(palw_tir_step_tree_width_v1(leaf_count, below)?);
    (lo < hi).then_some((lo, hi))
}

/// **Node `(level, index)`'s hash from its frontier at `below`** — the tree's own fold (pairs, an odd
/// last node promoted at the level's global width) over exactly the positions it covers; `None` when
/// the frontier is not that many nodes.
fn fold_frontier(leaf_count: u64, level: u8, index: u64, below: u8, frontier: &[Hash64]) -> Option<Hash64> {
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    if frontier.len() as u64 != hi - lo {
        return None;
    }
    let mut nodes = frontier.to_vec();
    let mut start = lo;
    for l in below..level {
        let width = palw_tir_step_tree_width_v1(leaf_count, l)?;
        let mut next = Vec::with_capacity(nodes.len().div_ceil(2));
        let mut k = 0usize;
        while k < nodes.len() {
            let position = start + k as u64;
            if position % 2 != 0 {
                return None;
            }
            if position + 1 < width {
                let right = nodes.get(k + 1)?;
                next.push(crate::palw_step_leg::step_merkle_node_v1(&nodes[k], right));
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

/// **The root node `(level, index)` with hash `node` reaches through `siblings`** — the opening's
/// walk from an interior node, promotion as the tree folds; `None` when the siblings are short or long.
fn walk_to_root(leaf_count: u64, level: u8, index: u64, node: Hash64, siblings: &[Hash64]) -> Option<Hash64> {
    let (mut current, mut position, mut l) = (node, index, level);
    let mut supplied = siblings.iter();
    loop {
        let width = palw_tir_step_tree_width_v1(leaf_count, l)?;
        if width == 1 {
            break;
        }
        let promoted = width % 2 == 1 && position == width - 1;
        if !promoted {
            let sibling = supplied.next()?;
            current = if position % 2 == 0 {
                crate::palw_step_leg::step_merkle_node_v1(&current, sibling)
            } else {
                crate::palw_step_leg::step_merkle_node_v1(sibling, &current)
            };
        }
        position /= 2;
        l += 1;
    }
    supplied.next().is_none().then_some(current)
}

/// **The prover's half: node `(level, index)`'s frontier and opening** from the execution's ordered
/// step-leaf hashes (the leaf hashes, not the tree's index-bound leaf nodes). `None` when the node is
/// not in the tree or `level` is 0 (a leaf is a `TirStepLeaf`).
pub fn palw_tir_step_node_parts_v1(ordered_leaf_hashes: &[Hash64], level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
    let leaf_count = ordered_leaf_hashes.len() as u64;
    if level == 0 {
        return None;
    }
    let below = frontier_level(level);
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    let mut levels: Vec<Vec<Hash64>> = vec![
        ordered_leaf_hashes.iter().enumerate().map(|(i, leaf)| crate::palw_step_leg::step_merkle_leaf_v1(i as u64, leaf)).collect(),
    ];
    while levels.last()?.len() > 1 {
        let last = levels.last()?;
        let mut next = Vec::with_capacity(last.len().div_ceil(2));
        let mut pairs = last.chunks_exact(2);
        for pair in &mut pairs {
            next.push(crate::palw_step_leg::step_merkle_node_v1(&pair[0], &pair[1]));
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

/// **What a `TirStepNode { level, index }` answer opens** (past `Params::palw_tir_fence2`): the node's
/// frontier — the nodes [`PALW_TIR_STEP_NODE_DEPTH_V1`] levels below it, or the leaf nodes when nearer
/// — and its siblings up to the step root, with the claim's binding (program EMPTY). A challenger
/// compares the frontier with its own execution's tree and names the first node that differs next:
/// ten levels a session, then the leaf.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirStepNodeDisclosureV1 {
    pub binding: PalwTirStepBindingV1,
    pub frontier: Vec<Hash64>,
    pub siblings: Vec<Hash64>,
}

impl PalwTirStepNodeDisclosureV1 {
    /// **Empties the carried program** — what a discloser does before the answer rides.
    pub fn strip_program_v1(&mut self) {
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut self.binding);
    }

    #[cfg(test)]
    fn with_program_v1_for_tests(&self, program: &[u8]) -> Self {
        let mut filled = self.clone();
        filled.binding.class.program = program.to_vec();
        filled
    }

    /// The disclosure with the registered class's program put back; refused when it carried one.
    pub fn with_program_v1(&self, record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1) -> Result<Self, &'static str> {
        let mut filled = self.clone();
        filled.binding = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(&self.binding, record)?;
        Ok(filled)
    }
}

/// The binding verified and naming the claim's two roots — the first step of every step answer.
fn claims_binding(
    binding: &PalwTirStepBindingV1,
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    max_step_leaf_count: u64,
) -> Result<PalwTirVerifiedBindingV1, PalwStepRefuteError> {
    let v = crate::palw_tir_step_v1::verify_tir_binding_v1(binding, max_step_leaf_count)
        .map_err(|_| bad("the IR binding does not verify"))?;
    if binding.full_logits_trace_root != claim_trace_root {
        return Err(bad("the disclosure binds to another trace root than the claim committed"));
    }
    if binding.committed_execution_root != claim_execution_root {
        return Err(bad("the disclosure binds to another execution root than the claim committed"));
    }
    Ok(v)
}

/// **Verify a `TirStepNode` answer against the claim — by hash arithmetic.** The binding verifies and
/// names the claim's roots; `(level, index)` is an interior node of its tree (`level ≥ 1`); the
/// frontier is exactly the nodes it covers `min(level, 8)` levels down and folds to the node, and the
/// siblings walk the node to the claim's step root.
pub fn check_tir_step_node_disclosure_v1(
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    level: u8,
    index: u64,
    disclosure: &PalwTirStepNodeDisclosureV1,
    max_step_leaf_count: u64,
) -> Result<(), PalwStepRefuteError> {
    let binding = &disclosure.binding;
    claims_binding(binding, claim_trace_root, claim_execution_root, max_step_leaf_count)?;
    palw_tir_step_node_reaches_v1(
        binding.step_leaf_count,
        &binding.step_merkle_root,
        level,
        index,
        &disclosure.frontier,
        &disclosure.siblings,
    )
    .map_err(bad)
}

/// **The hash arithmetic of a `TirStepNode` answer, its binding aside**: in the step tree over
/// `leaf_count` leaves, `(level, index)` is an interior node (`level ≥ 1`); `frontier` is exactly the
/// nodes it covers `min(level, 8)` levels down, and folds to it by the tree's own rule; `siblings` walk
/// it to `step_root`. What [`check_tir_step_node_disclosure_v1`] asks once the binding names the
/// claim, and what a seat asks of each answer as it descends.
pub fn palw_tir_step_node_reaches_v1(
    leaf_count: u64,
    step_root: &Hash64,
    level: u8,
    index: u64,
    frontier: &[Hash64],
    siblings: &[Hash64],
) -> Result<(), &'static str> {
    if level == 0 {
        return Err("a step node is an interior node: a leaf is demanded as a TirStepLeaf");
    }
    let node = fold_frontier(leaf_count, level, index, frontier_level(level), frontier)
        .ok_or("the frontier is not the node's, or the node is not in the tree")?;
    let root = walk_to_root(leaf_count, level, index, node, siblings).ok_or("the node's siblings do not walk to a root")?;
    if root != *step_root {
        return Err("the node does not reach the claim's step root");
    }
    Ok(())
}

/// **Where node `(level, index)`'s frontier sits**: `(frontier level, first, end)` — the level
/// `level − 8` (or 0, the leaf nodes, when nearer) and the positions `[first, end)` it covers there;
/// `None` for a leaf or a node past the tree. A seat descending names next the first frontier node its
/// own tree disagrees with: `TirStepNode { level: frontier level, index: first + k }`, or, at level 0,
/// `TirStepLeaf { index: first + k }`.
pub fn palw_tir_step_node_frontier_v1(leaf_count: u64, level: u8, index: u64) -> Option<(u8, u64, u64)> {
    if level == 0 {
        return None;
    }
    let below = frontier_level(level);
    let (lo, hi) = covered(leaf_count, level, index, below)?;
    Some((below, lo, hi))
}

/// **Verify a `TirStepOutOfRange` answer**: the claim's binding (roots, program empty on the wire)
/// proves the demanded unit is not in its execution — a leaf at or past its leaf count, a node past
/// the root or the level's width.
pub fn check_tir_step_out_of_range_v1(
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    unit: &crate::palw_da_rcore_v1::PalwDaUnitV1,
    binding: &PalwTirStepBindingV1,
    max_step_leaf_count: u64,
) -> Result<(), PalwStepRefuteError> {
    use crate::palw_da_rcore_v1::PalwDaUnitV1;
    let v = claims_binding(binding, claim_trace_root, claim_execution_root, max_step_leaf_count)?;
    let count = binding.step_leaf_count;
    let past = match unit {
        PalwDaUnitV1::TirStepLeaf { index } => *index >= count,
        PalwDaUnitV1::TirStepNode { level, index } => palw_tir_step_tree_width_v1(count, *level).is_none_or(|width| *index >= width),
        // A rows-tree unit is past a trace that has no rows tree (the flat scheme hashes every row at
        // once), and past a tiled trace's rows (a row at or past the decode count, a node past the
        // tree).
        PalwDaUnitV1::TirRowNode { level, index } => {
            let rows = u64::from(binding.job_context.exact_decode_tokens);
            logits_shape(&v.space).1 != tiled_logits_scheme_id_v1()
                || palw_tir_step_tree_width_v1(rows, *level).is_none_or(|width| *index >= width)
        }
        _ => return Err(bad("an out-of-range answer answers an IR step unit")),
    };
    if !past {
        return Err(bad("the unit is in the execution: it is answered by its disclosure"));
    }
    Ok(())
}

/// **A `TirStepNode` answer, built from what the answering node holds**, checked before it is
/// returned; the program stripped.
pub fn build_tir_step_node_disclosure_v1(
    binding: &PalwTirStepBindingV1,
    level: u8,
    index: u64,
    store: &dyn PalwTirEvidenceStoreV1,
    max_step_leaf_count: u64,
) -> Result<PalwTirStepNodeDisclosureV1, PalwTirEvidenceErrorV1> {
    let (frontier, siblings) =
        store.step_node(level, index).ok_or_else(|| PalwTirEvidenceErrorV1::Store(format!("step node ({level}, {index})")))?;
    let mut disclosure = PalwTirStepNodeDisclosureV1 { binding: binding.clone(), frontier, siblings };
    check_tir_step_node_disclosure_v1(
        binding.full_logits_trace_root,
        binding.committed_execution_root,
        level,
        index,
        &disclosure,
        max_step_leaf_count,
    )
    .map_err(|e| PalwTirEvidenceErrorV1::Store(format!("the store's node ({level}, {index}) does not answer: {e}")))?;
    disclosure.strip_program_v1();
    Ok(disclosure)
}

// ---------------------------------------------------------------------------------------------
// The tiled logits trace's rows tree: the second IR fence's descent unit `TirRowNode { level, index }`
// ---------------------------------------------------------------------------------------------

/// **The prover's half of a `TirRowNode`** from the run's logits rows (the tiled scheme): at
/// `level ≥ 1` the rows tree's node — the step tree's own shape over the row roots — with its
/// frontier and opening ([`palw_tir_step_node_parts_v1`]); at level 0 row `index`'s tile leaves
/// (`tiled_logits_tile_leaf_v1`, one per 4,096 lanes) and the row's opening in the rows tree. `None`
/// for a row or node past the trace, or rows that build no tree.
pub fn palw_tir_row_node_parts_v1(
    ctx: &PalwJobContextV2,
    rows: &[Vec<i32>],
    level: u8,
    index: u64,
) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
    let ctx_hash = ctx.context_hash();
    let row_roots: Vec<Hash64> = rows
        .iter()
        .enumerate()
        .map(|(r, row)| crate::palw_step_refute::tiled_logits_row_root_v1(&ctx_hash, r as u32, row))
        .collect::<Option<_>>()?;
    if level >= 1 {
        return palw_tir_step_node_parts_v1(&row_roots, level, index);
    }
    let row = rows.get(usize::try_from(index).ok()?)?;
    let tiles: Vec<Hash64> = row
        .chunks(PALW_LOGITS_TILE_LANES)
        .enumerate()
        .map(|(t, lanes)| crate::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx_hash, index as u32, t as u32, lanes))
        .collect();
    // The rows tree is bounded by its own row count — the job's decode count — not by the class's step
    // ladder: the prover holds no ruleset here, and the checker walks the opening at the claim's
    // ladder (`check_tir_row_node_disclosure_v1`), which a tree of rows the prover holds never
    // exceeds. The default ladder (2^22) is not this tree's bound (ADR-0084 U-08's guard).
    let opening = crate::palw_step_leg::step_opening_capped_v1(&row_roots, index, row_roots.len() as u64).ok()?;
    Some((tiles, opening.siblings))
}

/// **What a `TirRowNode { level, index }` answer opens** (past `Params::palw_tir_fence2`): a node of
/// the claim's tiled logits trace's rows tree — at `level ≥ 1` its frontier ten levels down (the row
/// leaves when nearer) and its siblings to the rows root; at level 0, row `index`'s tile leaves and
/// the row's siblings — with the generated ids that, with the rows root, reproduce the claim's trace
/// root, and the claim's binding (program EMPTY). A seat whose replay disagrees with the claim's trace
/// but not its steps descends here to the first row, then the first tile, it disputes; the tile's lanes
/// are the event unit's, and the step tree's logits leaf beside them convicts (`TirLogits`).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwTirRowNodeDisclosureV1 {
    pub binding: PalwTirStepBindingV1,
    pub generated_token_ids: Vec<u32>,
    pub frontier: Vec<Hash64>,
    pub siblings: Vec<Hash64>,
}

impl PalwTirRowNodeDisclosureV1 {
    /// **Empties the carried program** — what a discloser does before the answer rides.
    pub fn strip_program_v1(&mut self) {
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut self.binding);
    }

    /// The disclosure with the registered class's program put back; refused when it carried one.
    pub fn with_program_v1(&self, record: &crate::palw_tir_admission_v1::PalwTirClassRecordV1) -> Result<Self, &'static str> {
        let mut filled = self.clone();
        filled.binding = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(&self.binding, record)?;
        Ok(filled)
    }
}

/// **Verify a `TirRowNode` answer against the claim — by hash arithmetic.** The binding verifies and
/// names the claim's roots, and its class commits its logits under the tiled scheme; the ids are the
/// decode count; the answer reaches a rows root — at `level ≥ 1` its frontier folds to the node and its
/// siblings walk it up; at level 0 the tile leaves (the row's tile count of them) build the row root,
/// and its opening walks the row up — and that rows root, with the ids, reproduces the claim's trace
/// root.
pub fn check_tir_row_node_disclosure_v1(
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    level: u8,
    index: u64,
    disclosure: &PalwTirRowNodeDisclosureV1,
    max_step_leaf_count: u64,
) -> Result<(), PalwStepRefuteError> {
    let binding = &disclosure.binding;
    let v = claims_binding(binding, claim_trace_root, claim_execution_root, max_step_leaf_count)?;
    let (vocab, scheme) = logits_shape(&v.space);
    if scheme != tiled_logits_scheme_id_v1() {
        return Err(bad("a rows tree is the tiled scheme's: a flat trace answers a row demand by its out-of-range proof"));
    }
    let ctx = &binding.job_context;
    let rows = u64::from(ctx.exact_decode_tokens);
    if disclosure.generated_token_ids.len() as u64 != rows {
        return Err(bad("the id count is not the context's decode count"));
    }
    let rows_root = if level >= 1 {
        let node = fold_frontier(rows, level, index, frontier_level(level), &disclosure.frontier)
            .ok_or(bad("the frontier is not the node's, or the node is not in the rows tree"))?;
        walk_to_root(rows, level, index, node, &disclosure.siblings).ok_or(bad("the node's siblings do not walk to a rows root"))?
    } else {
        if index >= rows {
            return Err(bad("the row is past the decode count"));
        }
        if disclosure.frontier.len() != vocab.div_ceil(PALW_LOGITS_TILE_LANES) {
            return Err(bad("a row's frontier is its tile leaves, one per 4,096 lanes of the vocabulary"));
        }
        let row_root = crate::palw_step_leg::step_merkle_root_capped_v1(&disclosure.frontier, max_step_leaf_count)
            .map_err(|_| bad("the row's tiles build no tree"))?;
        let opening = PalwStepOpeningV1 { leaf_index: index, leaf_hash: row_root, siblings: disclosure.siblings.clone() };
        step_opening_root_capped_v1(rows, &opening, max_step_leaf_count).map_err(|_| bad("the row's opening does not walk"))?
    };
    if tiled_logits_outer_root_v1(ctx, rows, &rows_root, &disclosure.generated_token_ids) != binding.full_logits_trace_root {
        return Err(bad("the rows root and the ids do not reproduce the claim's trace root"));
    }
    Ok(())
}

/// **A `TirRowNode` answer, built from what the answering node holds**: the ids from its decode pin
/// (the tiled scheme's), the frontier and opening from its rows; checked before it is returned, the
/// program stripped.
pub fn build_tir_row_node_disclosure_v1(
    binding: &PalwTirStepBindingV1,
    level: u8,
    index: u64,
    store: &dyn PalwTirEvidenceStoreV1,
    max_step_leaf_count: u64,
) -> Result<PalwTirRowNodeDisclosureV1, PalwTirEvidenceErrorV1> {
    let missing = |what: String| PalwTirEvidenceErrorV1::Store(what);
    let Some(PalwDecodeTokenPinV1::TiledV1(pin)) = store.decode_pin() else {
        return Err(missing("the tiled scheme's decode pin".into()));
    };
    let (frontier, siblings) = store.row_node(level, index).ok_or_else(|| missing(format!("rows-tree node ({level}, {index})")))?;
    let mut disclosure =
        PalwTirRowNodeDisclosureV1 { binding: binding.clone(), generated_token_ids: pin.generated_token_ids, frontier, siblings };
    check_tir_row_node_disclosure_v1(
        binding.full_logits_trace_root,
        binding.committed_execution_root,
        level,
        index,
        &disclosure,
        max_step_leaf_count,
    )
    .map_err(|e| missing(format!("the store's rows-tree node ({level}, {index}) does not answer: {e}")))?;
    disclosure.strip_program_v1();
    Ok(disclosure)
}

/// The decode row of a leaf of the post block's logits node — `a − (P − 1)` for a position with a row
/// — and the leaf's first lane; `None` for any other leaf.
fn logits_leaf_row(space: &PalwTirStepSpaceV1, ctx: &PalwJobContextV2, leaf: &PalwTirLeafV1) -> Option<(u32, u32)> {
    let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else { return None };
    let program = &space.program;
    let block = occurrence_block(space, u16::try_from(occurrence).ok()?)?;
    if block != program.schedule.post || node != program.logits {
        return None;
    }
    let first_row_position = ctx.declared_prefill_tokens.checked_sub(1)?;
    let row = leaf.position.checked_sub(first_row_position)?;
    (row < ctx.exact_decode_tokens).then_some((row, u32::try_from(first_element).ok()?))
}

/// **Verify a `TirStepLeaf` answer against the claim — by hash arithmetic** (the second IR fence).
/// `Ok(())` answers the unit; an `Err` is a disclosure that is not an answer. In order: the binding
/// verifies at `max_step_leaf_count` and names the claim's two roots; `index` is a leaf of it and the
/// opening opens it; the preimage hashes to the opened leaf; the ids authenticate against the trace
/// root in the class's scheme; the row pin is present exactly for a logits tile at a decode row under
/// the tiled scheme, and then opens that row under the ids' rows root, carries the same ids, and
/// authenticates the committed token's tile and the leaf's first lane's tile. The leaf's STRUCTURE is
/// not judged here: a malformed committed leaf is answered (it is what was committed) and convicted by
/// the court from this answer.
pub fn check_tir_step_leaf_disclosure_v1(
    claim_trace_root: Hash64,
    claim_execution_root: Hash64,
    index: u64,
    disclosure: &PalwTirStepLeafDisclosureV1,
    max_step_leaf_count: u64,
) -> Result<(), PalwStepRefuteError> {
    let binding = &disclosure.binding;
    let v = claims_binding(binding, claim_trace_root, claim_execution_root, max_step_leaf_count)?;
    if index >= binding.step_leaf_count || disclosure.opening.leaf_index != index {
        return Err(bad("the opening does not open the demanded leaf of this execution"));
    }
    let implied = step_opening_root_capped_v1(binding.step_leaf_count, &disclosure.opening, max_step_leaf_count)
        .map_err(|_| bad("the leaf's opening does not walk"))?;
    if implied != binding.step_merkle_root {
        return Err(bad("the leaf's opening does not reach the claim's step root"));
    }
    if step_tile_leaf_hash_v1(&v.context_hash, &v.class_id, &disclosure.preimage) != disclosure.opening.leaf_hash {
        return Err(bad("the preimage is not the opened leaf"));
    }
    let ids = authenticate_decode_pin(binding, &v.space, &disclosure.decode)?;
    let ctx = &binding.job_context;
    let (vocab, scheme) = logits_shape(&v.space);
    let leaf = v.space.leaf_at(ctx, index).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let aimed = if scheme == tiled_logits_scheme_id_v1() { logits_leaf_row(&v.space, ctx, &leaf) } else { None };
    match (aimed, &disclosure.row_pin) {
        (None, None) => Ok(()),
        (None, Some(_)) => Err(bad("a row pin rides only for a logits tile at a decode row, tiled scheme")),
        (Some(_), None) => Err(bad("a logits tile at a decode row is answered with its row's pin")),
        (Some((row, lane)), Some(pin)) => {
            let PalwDecodeTokenPinV1::TiledV1(tiled) = &disclosure.decode else {
                return Err(bad("the tiled scheme's ids ride as TiledV1"));
            };
            let decode = ctx.exact_decode_tokens as u64;
            if pin.position != row || pin.beat_lane != lane || pin.generated_token_ids != ids {
                return Err(bad("the row pin is not aimed at this leaf's row and first lane, or carries other ids"));
            }
            let rows_root = step_opening_root_capped_v1(decode, &pin.row_opening, max_step_leaf_count)
                .map_err(|_| bad("the row opening does not walk"))?;
            if rows_root != tiled.rows_root || pin.row_opening.leaf_index != row as u64 || pin.row_opening.leaf_hash != pin.row_root {
                return Err(bad("the row pin does not open this row under the claim's rows root"));
            }
            let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES) as u64;
            let open = |lanes: &[i32], opening: &PalwStepOpeningV1, at: usize| {
                tiled_tile_authenticate_v1(
                    &v.context_hash,
                    row,
                    vocab,
                    tiles,
                    &pin.row_root,
                    (at / PALW_LOGITS_TILE_LANES) as u64,
                    lanes,
                    opening,
                    max_step_leaf_count,
                )
            };
            let committed = ids[row as usize] as usize;
            if committed < vocab {
                open(&pin.committed_tile_lanes, &pin.committed_opening, committed)?;
            }
            if (lane as usize) >= vocab {
                return Err(bad("the leaf's first lane is past the vocabulary"));
            }
            open(&pin.beat_tile_lanes, &pin.beat_opening, lane as usize)
        }
    }
}

/// **A `TirStepLeaf` answer, built from what the answering node holds** (its own capture, or the
/// accused's through the store): the leaf, its opening, the ids, and the row pin exactly where the
/// fold asks for one; the program stripped. Checked with [`check_tir_step_leaf_disclosure_v1`]
/// before it is returned, so a node never pays a carrier for an answer the fold refuses.
pub fn build_tir_step_leaf_disclosure_v1(
    binding: &PalwTirStepBindingV1,
    index: u64,
    store: &dyn PalwTirEvidenceStoreV1,
    max_step_leaf_count: u64,
) -> Result<PalwTirStepLeafDisclosureV1, PalwTirEvidenceErrorV1> {
    let missing = |what: String| PalwTirEvidenceErrorV1::Store(what);
    let v = crate::palw_tir_step_v1::verify_tir_binding_v1(binding, max_step_leaf_count)
        .map_err(|e| PalwTirEvidenceErrorV1::Binding(e.to_string()))?;
    let preimage = store.step_leaf(index).ok_or_else(|| missing(format!("step leaf {index}")))?;
    let opening = store.step_opening(index).ok_or_else(|| missing(format!("the opening of leaf {index}")))?;
    let decode = store.decode_pin().ok_or_else(|| missing("the decode pin".into()))?;
    let leaf = v.space.leaf_at(&binding.job_context, index).ok_or_else(|| missing(format!("leaf {index} in the step space")))?;
    let (_, scheme) = logits_shape(&v.space);
    let row_pin =
        match (scheme == tiled_logits_scheme_id_v1()).then(|| logits_leaf_row(&v.space, &binding.job_context, &leaf)).flatten() {
            Some((row, lane)) => Some(store.row_pin(row, lane).ok_or_else(|| missing(format!("row {row}'s pin at lane {lane}")))?),
            None => None,
        };
    let mut disclosure = PalwTirStepLeafDisclosureV1 { binding: binding.clone(), preimage, opening, decode, row_pin };
    check_tir_step_leaf_disclosure_v1(
        binding.full_logits_trace_root,
        binding.committed_execution_root,
        index,
        &disclosure,
        max_step_leaf_count,
    )
    .map_err(|e| PalwTirEvidenceErrorV1::Store(format!("the store's leaf {index} does not answer: {e}")))?;
    disclosure.strip_program_v1();
    Ok(disclosure)
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1, open_artifact_leaf_v1};
    use crate::palw_step_leg::{step_merkle_range_siblings_v1, step_merkle_root_v1, step_opening_v1};
    use crate::palw_step_refute::{PalwTiledDecodeTokensV1, base0_decode_token_select_v1, tiled_logits_rows_root_v1};
    use crate::palw_tir_artifact_v1::{PalwTirTensorSourceV1, palw_tir_inventory_operands_v1};
    use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PALW_TIR_LAYOUT_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1};
    use crate::palw_tir_step_v1::palw_tir_leaf_preimage_v1;
    use misaka_palw_tir::builder::ProgramBuilder;
    use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
    use misaka_palw_tir::{Interpreter, MapParams, Ref, RunState, Tensor, TensorType};
    use std::borrow::Cow;

    pub(crate) const PREFILL: u32 = 3;
    pub(crate) const DECODE: u32 = 2;

    /// A committed IR execution and everything a store answers from.
    pub(crate) struct TinyExecution {
        pub(crate) binding: PalwTirStepBindingV1,
        pub(crate) preimages: Vec<PalwStepTileLeafV1>,
        pub(crate) hashes: Vec<Hash64>,
        pub(crate) ops: Vec<PalwArtifactOperandV1>,
        pub(crate) prompt: Vec<u32>,
        pub(crate) rows: Vec<Vec<i32>>,
        pub(crate) generated: Vec<u32>,
    }

    impl PalwTirEvidenceStoreV1 for TinyExecution {
        fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
            self.preimages.get(index as usize).cloned()
        }
        fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
            step_opening_v1(&self.hashes, index).ok()
        }
        fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
            step_merkle_range_siblings_v1(&self.hashes, first as usize, count as usize).ok()
        }
        fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
            open_artifact_leaf_v1(&self.ops, leaf)
        }
        fn prompt_token_ids(&self) -> Option<Vec<u32>> {
            Some(self.prompt.clone())
        }
        fn prompt_ids_opening(&self, _tile: u32) -> Option<PalwPromptIdsOpeningV1> {
            None
        }
        fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
            Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
                rows_root: tiled_logits_rows_root_v1(&self.binding.job_context, &self.rows)?,
                generated_token_ids: self.generated.clone(),
            }))
        }
        fn row_pin(&self, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
            crate::palw_step_refute::tiled_decode_pin_v1(&self.binding.job_context, &self.rows, &self.generated, row, lane)
        }
        fn step_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
            palw_tir_step_node_parts_v1(&self.hashes, level, index)
        }
        fn row_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
            palw_tir_row_node_parts_v1(&self.binding.job_context, &self.rows, level, index)
        }
    }

    struct Src<'a>(&'a MapParams);
    impl PalwTirTensorSourceV1 for Src<'_> {
        fn tensor_bytes(&self, param: u16, layer: Option<u16>) -> Option<Cow<'_, [u8]>> {
            self.0.tensors.get(&(param, layer)).map(|t| Cow::Owned(t.to_le_bytes()))
        }
    }

    fn program_and_params() -> (TirProgramV1, MapParams) {
        let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
        let embed = pb.param("embed", DType::I8, &[8, 4], false);
        let w = pb.param("w", DType::I8, &[4, 4], true);
        let head = pb.param("head", DType::I8, &[8, 4], false);
        let sum = pb.fixed_state("sum", DType::I32, &[4], -1000, 1000, true);
        let carry = vec![TensorType::fixed(DType::I32, &[4])];
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let x = b.gather(embed, Ref::Input(0), 0, 0);
            let x = b.cast(x, DType::I32);
            b.finish(&[x])
        };
        let layer = {
            let mut b = pb.block("layer", carry.clone());
            let s = b.add(Ref::State(sum), Ref::CarryIn(0), DType::I64);
            let s = b.state_write(sum, s);
            let s = b.reshape_fixed(s, &[4, 1]);
            let y = b.matmul(w, s, DType::I32);
            let y = b.reshape_fixed(y, &[4]);
            b.finish(&[y])
        };
        let (post, logits) = {
            let mut b = pb.block("post", carry);
            let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
            let l = b.matmul(head, x, DType::I64);
            let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
            let l = b.reshape_fixed(l, &[8]);
            let l = b.commit(l);
            let Ref::Node(i) = l else { unreachable!() };
            (b.finish(&[]), i)
        };
        let mut program = pb.finish(pre, vec![layer], post, logits);
        program.logits_scheme_id.copy_from_slice(crate::palw_step_refute::tiled_logits_scheme_id_v1().as_byte_slice());
        let mut params = MapParams::default();
        let fill = |n: usize, k: i128| (0..n as i128).map(|i| ((i * 37 + k) % 255) - 127).collect::<Vec<_>>();
        params.tensors.insert((0, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 5)).unwrap());
        params.tensors.insert((1, Some(0)), Tensor::new(DType::I8, vec![4, 4], fill(16, 11)).unwrap());
        params.tensors.insert((2, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 17)).unwrap());
        (program, params)
    }

    /// The class, the job and an execution — honest, or with lane `lane` of leaf `forge` moved by
    /// one (inside its proven interval).
    pub(crate) fn tiny_execution(forge: Option<(usize, usize)>) -> TinyExecution {
        tiny_execution_in(
            |class, class_id, prompt| {
                let z = Hash64::from_bytes([0u8; 64]);
                PalwJobContextV2 {
                    version: 2,
                    network_id: b"testnet-12".to_vec(),
                    job_id: Hash64::from_bytes([5; 64]),
                    job_nullifier: z,
                    assignment_id: z,
                    execution_seed: [0u8; 32],
                    model_profile_id: z,
                    runtime_manifest_hash: z,
                    runtime_class_id: z,
                    shape_profile_id: class_id,
                    trace_scheme_id: tiled_logits_scheme_id_v1(),
                    cu_ruleset_id: z,
                    tokenizer_id: class.tokenizer_id,
                    prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(prompt),
                    declared_prefill_tokens: PREFILL,
                    exact_decode_tokens: DECODE,
                    max_context_tokens: 64,
                }
            },
            forge,
        )
    }

    /// [`tiny_execution`] in the job context `ctx_of(class, class_id, prompt)` builds — a
    /// `(PREFILL, DECODE)` job over the tiny class, whose layout allows exactly the
    /// `PREFILL + DECODE − 1` positions the job touches.
    pub(crate) fn tiny_execution_in(
        ctx_of: impl FnOnce(&PalwTirClassV1, Hash64, &[u32]) -> PalwJobContextV2,
        forge: Option<(usize, usize)>,
    ) -> TinyExecution {
        let (program, params) = program_and_params();
        let bytes = program.encode();
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: bytes,
            layout: PalwTirLayoutV1 {
                version: PALW_TIR_LAYOUT_VERSION_V1,
                max_context: PREFILL + DECODE - 1,
                checkpoint_interval: 2,
                h_tile: 1,
                commit_tiles: vec![4, 4, 4096],
                state_tiles: vec![4],
            },
            tokenizer_id: Hash64::from_bytes([3; 64]),
        };
        let ops = palw_tir_inventory_operands_v1(&program, &Src(&params)).expect("inventory");
        let artifact_root = artifact_root_v1(&ops.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).expect("root");
        let class_id = class.class_id(&artifact_root);
        let prompt = vec![1u32, 6, 3];
        let interp = Interpreter::new(&program).expect("valid");
        let mut state = RunState::default();
        let positions = PREFILL + DECODE - 1;
        let (mut commits, mut after, mut rows, mut generated) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for a in 0..positions {
            let token = if a < PREFILL { prompt[a as usize] } else { generated[(a - PREFILL) as usize] };
            let step = interp.step(&params, &mut state, token).expect("an honest step");
            commits.push(step.commits);
            after.push(state.clone());
            if a + 1 >= PREFILL {
                let row: Vec<i32> = step.logits.data.iter().map(|v| *v as i32).collect();
                generated.push(base0_decode_token_select_v1(&row) as u32);
                rows.push(row);
            }
        }
        let ctx = ctx_of(&class, class_id, &prompt);
        assert_eq!((ctx.declared_prefill_tokens, ctx.exact_decode_tokens), (PREFILL, DECODE), "the tiny job");
        let space = PalwTirStepSpaceV1::new(&class).expect("the layout fits");
        let occ = space.occurrences().to_vec();
        let mut preimages = Vec::new();
        for a in 0..positions {
            for leaf in space.leaves_of_position(&ctx, a) {
                let n = leaf.value_count as usize;
                let mut values: Vec<i128> = match leaf.kind {
                    PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } => {
                        let (block, layer) = occ[occurrence as usize];
                        let c = commits[a as usize].iter().find(|c| c.block == block && c.layer == layer && c.node == node).unwrap();
                        c.value.data[first_element as usize..first_element as usize + n].to_vec()
                    }
                    PalwTirLeafKindV1::State { state: j, layer, first_element, .. } => {
                        let t = after[a as usize].fixed.get(&(j, layer)).expect("written every position");
                        t.data[first_element as usize..first_element as usize + n].to_vec()
                    }
                    PalwTirLeafKindV1::HistTile { .. } => unreachable!("no history"),
                };
                if let Some((i, lane)) = forge
                    && i == preimages.len()
                {
                    values[lane] += if values[lane] < 0 { 1 } else { -1 };
                }
                preimages.push(palw_tir_leaf_preimage_v1(&leaf, &values).expect("a lane of its dtype"));
            }
        }
        let ctx_hash = ctx.context_hash();
        let hashes: Vec<Hash64> = preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &class_id, p)).collect();
        let root = step_merkle_root_v1(&hashes).expect("root");
        let trace = crate::palw_step_refute::tiled_logits_trace_root_v1(&ctx, &rows, &generated).expect("trace");
        let count = hashes.len() as u64;
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            class,
            artifact_root,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: root,
            committed_execution_root: palw_tir_execution_root_v1(&ctx_hash, &trace, &class_id, count, &root),
        };
        TinyExecution { binding, preimages, hashes, ops, prompt, rows, generated }
    }

    /// A binding whose parts verify (the leaf count is the job's and the root recomputes), over a
    /// class with a canonical job: the identity rule needs no execution, only a binding.
    /// A binding whose parts verify (the leaf count is the job's and the root recomputes) over the
    /// tiny class widened to 64 positions (so it has a canonical job), at the anchor's J5 context with
    /// `edit` applied: the identity rule needs no execution, only a binding.
    pub(crate) fn attempt_binding(
        anchor: Hash64,
        edit: impl FnOnce(&mut PalwJobContextV2),
    ) -> (PalwTirStepBindingV1, crate::palw_tir_attempt_v1::PalwTirJobFactsV1) {
        use crate::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_context_v1, palw_tir_attempt_prompt_root_v1};
        const MAX: u64 = 1 << 26;
        let x = tiny_execution(None);
        let mut class = x.binding.class.clone();
        class.layout.max_context = 64;
        let class_id = class.class_id(&x.binding.artifact_root);
        let facts = PalwTirJobFactsV1::of_class(&class, class_id).unwrap();
        let canonical = crate::palw_tir_attempt_v1::palw_tir_attempt_canonical_v1(&class).unwrap();
        let root = palw_tir_attempt_prompt_root_v1(&facts, &anchor, canonical.0, PalwPromptIdsFormV1::Flat).unwrap();
        let mut ctx = palw_tir_attempt_context_v1(&facts, &anchor, canonical, root);
        edit(&mut ctx);
        let space = PalwTirStepSpaceV1::new(&class).unwrap();
        let count = space.leaf_count_capped(&ctx, MAX).unwrap();
        let (trace, step_root) = (Hash64::from_bytes([0x71; 64]), Hash64::from_bytes([0x72; 64]));
        let execution = palw_tir_execution_root_v1(&ctx.context_hash(), &trace, &class_id, count, &step_root);
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx,
            class,
            artifact_root: x.binding.artifact_root,
            full_logits_trace_root: trace,
            step_leaf_count: count,
            step_merkle_root: step_root,
            committed_execution_root: execution,
        };
        (binding, facts)
    }

    #[test]
    fn the_tiny_execution_is_adjudicable_both_ways() {
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: DemandLimits::UNLIMITED,
        };
        let honest = tiny_execution(None);
        for leaf in 0..honest.preimages.len() as u64 {
            let r = build_tir_cone_refutation_v1(&honest.binding, leaf, &honest, &rules).expect("buildable");
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "leaf {leaf}");
        }
        let forged = tiny_execution(Some((7, 1)));
        let r = build_tir_cone_refutation_v1(&forged.binding, 7, &forged, &rules).expect("buildable");
        assert_eq!(
            check_tir_cone_refutation_v1(&r, &rules).expect("convicted").fault,
            PalwStepFaultV1::ComputationMismatch { value_index: 1 }
        );
    }

    /// **The multiproof vectors on the tiny program** (design §2.12.1): its closes carry a single
    /// inventory leaf (a `Fixed`-state checkpoint whose update reads the gathered embedding row), a run
    /// (a matmul's whole weight) and none; every one is acquitted, is the builder's multiproof of the
    /// leaves read, and is priced to the byte by `palw_tir_param_carriage_bytes_v1`.
    #[test]
    fn the_tiny_program_s_closes_carry_a_single_leaf_a_run_and_none() {
        let rules =
            PalwTirCourtRulesV1 { max_step_leaf_count: 1 << 26, prompt_form: PalwPromptIdsFormV1::Flat, limits: DemandLimits::UNLIMITED };
        let x = tiny_execution(None);
        let program = x.binding.class.decode_program().expect("decodes");
        let hashes: Vec<Hash64> = x.ops.iter().map(crate::palw_artifact::artifact_leaf_v1).collect();
        let (mut singles, mut runs, mut none) = (0, 0, 0);
        for leaf in 0..x.preimages.len() as u64 {
            let r = build_tir_cone_refutation_v1(&x.binding, leaf, &x, &rules).expect("buildable");
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "leaf {leaf}");
            let leaves: Vec<u32> = r.params.iter().flat_map(|p| p.opened.iter().map(|(l, _)| *l)).collect();
            if !leaves.is_empty() {
                let opened: Vec<_> = leaves.iter().map(|l| (*l, x.ops[*l as usize].clone())).collect();
                assert_eq!(r.params, crate::palw_artifact::palw_artifact_multiproof_v1(&hashes, &opened), "leaf {leaf}");
            }
            let carried = borsh::to_vec(&r.params).unwrap().len() as u64;
            assert_eq!(palw_tir_param_carriage_bytes_v1(&program, PalwTirParamCarriageV1::Multiproof, &leaves), Some(carried));
            match leaves.len() {
                0 => none += 1,
                1 => singles += 1,
                _ if leaves.windows(2).all(|w| w[1] == w[0] + 1) => runs += 1,
                _ => {}
            }
        }
        assert!(singles > 0 && runs > 0 && none > 0, "{singles} single-leaf closes, {runs} runs, {none} with none");
    }

    /// **A job at exactly `layout.max_context` positions, in the CANONICAL job context, end to end**
    /// (ref2's Phase F observation, `tir/ref2` 533e7b7fa, item 7). The context carries a TOKEN budget
    /// (`prefill + decode ≤ max_context_tokens`, the v2 family's rule every court path checks) and the
    /// layout a POSITION bound (`prefill + decode − 1 ≤ max_context`: the last emitted token is never
    /// fed back), so the canonical context states `max_context + 1` tokens. Here the job touches all
    /// `max_context` positions: the context passes the family's shape rule, the binding verifies, every
    /// honest leaf — the last position's logits included — is acquitted, a lie at the last position is
    /// convicted, and one more decode token is refused by both rules at the same boundary.
    #[test]
    fn a_job_at_exactly_max_context_positions_runs_end_to_end_in_the_canonical_context() {
        use crate::palw_tir_attempt_v1::palw_tir_canonical_context_v1;
        const MAX: u64 = 1 << 26;
        let rules = PalwTirCourtRulesV1 {
            max_step_leaf_count: MAX,
            prompt_form: PalwPromptIdsFormV1::Flat,
            limits: DemandLimits::UNLIMITED,
        };
        let canonical = |class: &PalwTirClassV1, class_id: Hash64, prompt: &[u32]| {
            let mut ctx = palw_tir_canonical_context_v1(class, class_id, (PREFILL, DECODE)).expect("the tiny program decodes");
            ctx.prompt_token_ids_hash = crate::palw_v2::prompt_token_ids_hash_v2(prompt);
            ctx
        };
        let honest = tiny_execution_in(canonical, None);
        let b = &honest.binding;
        let max_context = b.class.layout.max_context;
        assert_eq!(PREFILL + DECODE - 1, max_context, "the job touches exactly max_context positions");
        assert_eq!(b.job_context.max_context_tokens, max_context + 1, "the canonical budget in tokens: positions + 1");
        crate::palw_slash::check_job_context_shape(&b.job_context).expect("the family's token budget admits the longest job");
        let verified = crate::palw_tir_step_v1::verify_tir_binding_v1(b, MAX).expect("the binding verifies at max_context positions");
        assert_eq!(verified.job.positions, max_context);
        for leaf in 0..honest.preimages.len() as u64 {
            let r = build_tir_cone_refutation_v1(b, leaf, &honest, &rules).expect("buildable");
            assert_eq!(check_tir_cone_refutation_v1(&r, &rules), Err(PalwStepRefuteError::NoFaultFound), "leaf {leaf}");
        }
        // A lie in the last position's first leaf.
        let space = PalwTirStepSpaceV1::new(&b.class).expect("the layout fits");
        let last = honest.preimages.len() - space.leaves_of_position(&b.job_context, max_context - 1).len();
        let forged = tiny_execution_in(canonical, Some((last, 0)));
        let r = build_tir_cone_refutation_v1(&forged.binding, last as u64, &forged, &rules).expect("buildable");
        check_tir_cone_refutation_v1(&r, &rules).expect("a lie at the last position is convicted");
        // One more decode token: refused by the token budget and by the position bound alike.
        let mut over = b.job_context.clone();
        over.exact_decode_tokens += 1;
        assert!(crate::palw_slash::check_job_context_shape(&over).is_err(), "over the token budget");
        assert!(space.job_shape(&over).is_err(), "over the position bound");
        // …and the two rules agree on every split of the budget.
        for prefill in 1..=max_context {
            let mut ctx = b.job_context.clone();
            ctx.declared_prefill_tokens = prefill;
            for decode in 1..=max_context + 2 {
                ctx.exact_decode_tokens = decode;
                assert_eq!(
                    crate::palw_slash::check_job_context_shape(&ctx).is_ok(),
                    space.job_shape(&ctx).is_ok(),
                    "({prefill}, {decode}): the token budget and the position bound disagree"
                );
            }
        }
    }
}

/// **The second IR fence's DA unit: one committed step leaf, disclosed** (evidence transport C) —
/// built from the tiny execution's store for every leaf, checked, and refused in every other shape.
#[cfg(test)]
mod step_leaf_da_tests {
    use super::test_support::{TinyExecution, tiny_execution};
    use super::*;

    const MAX: u64 = 1 << 26;

    fn check(x: &TinyExecution, index: u64, d: &PalwTirStepLeafDisclosureV1) -> Result<(), PalwStepRefuteError> {
        check_tir_step_leaf_disclosure_v1(x.binding.full_logits_trace_root, x.binding.committed_execution_root, index, d, MAX)
    }

    /// The disclosure with its program put back, as the fold reads it.
    fn filled(x: &TinyExecution, d: &PalwTirStepLeafDisclosureV1) -> PalwTirStepLeafDisclosureV1 {
        let mut f = d.clone();
        f.binding = x.binding.clone();
        f
    }

    #[test]
    fn every_leaf_is_disclosed_and_a_logits_tile_carries_its_row_pin() {
        let x = tiny_execution(None);
        let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
        let (mut pinned, mut plain) = (0, 0);
        for index in 0..x.preimages.len() as u64 {
            let d = build_tir_step_leaf_disclosure_v1(&x.binding, index, &x, MAX).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
            assert!(d.binding.class.program.is_empty(), "it rides without the program");
            let f = filled(&x, &d);
            check(&x, index, &f).unwrap_or_else(|e| panic!("leaf {index}: {e}"));
            let leaf = space.leaf_at(&x.binding.job_context, index).unwrap();
            let is_logits_row = logits_leaf_row(&space, &x.binding.job_context, &leaf).is_some();
            assert_eq!(d.row_pin.is_some(), is_logits_row, "leaf {index}: a row pin exactly for a logits tile at a decode row");
            if is_logits_row {
                pinned += 1;
            } else {
                plain += 1;
            }
            // Another index than the opening's is not an answer.
            if index + 1 < x.preimages.len() as u64 {
                assert!(check(&x, index + 1, &f).is_err(), "leaf {index} does not answer leaf {}", index + 1);
            }
        }
        assert!(pinned > 0 && plain > 0, "{pinned} logits tiles, {plain} others");
    }

    #[test]
    fn a_disclosure_in_any_other_shape_is_refused() {
        let x = tiny_execution(None);
        let space = PalwTirStepSpaceV1::new(&x.binding.class).unwrap();
        let logits = (0..x.preimages.len() as u64)
            .find(|i| logits_leaf_row(&space, &x.binding.job_context, &space.leaf_at(&x.binding.job_context, *i).unwrap()).is_some())
            .expect("a logits tile at a decode row");
        let plain = (0..x.preimages.len() as u64).find(|i| *i != logits && !x.preimages.is_empty()).unwrap();
        for index in [plain, logits] {
            let honest = filled(&x, &build_tir_step_leaf_disclosure_v1(&x.binding, index, &x, MAX).unwrap());
            check(&x, index, &honest).expect("honest");
            let mut edits: Vec<(&str, PalwTirStepLeafDisclosureV1)> = Vec::new();
            let mut d = honest.clone();
            d.preimage.values_le[0] ^= 1;
            edits.push(("a preimage byte", d));
            let mut d = honest.clone();
            d.opening = crate::palw_step_leg::step_opening_v1(&x.hashes, (index + 1) % x.hashes.len() as u64).unwrap();
            edits.push(("another leaf's opening", d));
            let mut d = honest.clone();
            if let PalwDecodeTokenPinV1::TiledV1(t) = &mut d.decode {
                t.generated_token_ids[0] ^= 1;
            }
            edits.push(("other ids", d));
            let mut d = honest.clone();
            d.binding.full_logits_trace_root = Hash64::from_bytes([9; 64]);
            edits.push(("another trace root", d));
            let mut d = honest.clone();
            d.row_pin = if index == logits { None } else { crate::palw_step_refute::tiled_decode_pin_v1(&x.binding.job_context, &x.rows, &x.generated, 0, 0) };
            edits.push(("the row pin where it does not belong, or missing", d));
            if index == logits {
                let mut d = honest.clone();
                let pin = d.row_pin.as_mut().unwrap();
                pin.beat_lane = (pin.beat_lane + 1) % 8;
                edits.push(("a pin aimed at another lane", d));
                let mut d = honest.clone();
                d.row_pin.as_mut().unwrap().beat_tile_lanes[0] ^= 1;
                edits.push(("a pinned tile lane", d));
            }
            for (what, d) in edits {
                assert!(check(&x, index, &d).is_err(), "leaf {index}: {what} is not an answer");
            }
        }
        // A leaf past the execution.
        let past = x.preimages.len() as u64;
        let honest = filled(&x, &build_tir_step_leaf_disclosure_v1(&x.binding, 0, &x, MAX).unwrap());
        assert!(check(&x, past, &honest).is_err());
    }

    /// **Every interior node of the tiny execution's step tree is disclosed** (its frontier and its
    /// opening, the program stripped) and checks against the claim's roots; no node's answer answers
    /// its neighbour or a leaf; a unit past the execution is proven so by the binding, and one inside
    /// it is not.
    #[test]
    fn every_node_is_disclosed_and_a_unit_past_the_execution_is_proven_so() {
        use crate::palw_da_rcore_v1::PalwDaUnitV1;
        let x = tiny_execution(None);
        let count = x.binding.step_leaf_count;
        let height = palw_tir_step_tree_height_v1(count);
        assert!(height >= 2, "{count} leaves");
        let (trace, execution) = (x.binding.full_logits_trace_root, x.binding.committed_execution_root);
        let filled_node = |d: &PalwTirStepNodeDisclosureV1| d.with_program_v1_for_tests(&x.binding.class.program);
        let past = |unit: PalwDaUnitV1| check_tir_step_out_of_range_v1(trace, execution, &unit, &x.binding, MAX).is_ok();
        for level in 1..=height {
            let width = palw_tir_step_tree_width_v1(count, level).unwrap();
            for index in 0..width {
                let d = build_tir_step_node_disclosure_v1(&x.binding, level, index, &x, MAX)
                    .unwrap_or_else(|e| panic!("({level}, {index}): {e}"));
                assert!(d.binding.class.program.is_empty(), "it rides without the program");
                let f = filled_node(&d);
                check_tir_step_node_disclosure_v1(trace, execution, level, index, &f, MAX)
                    .unwrap_or_else(|e| panic!("({level}, {index}): {e}"));
                if index + 1 < width {
                    assert!(
                        check_tir_step_node_disclosure_v1(trace, execution, level, index + 1, &f, MAX).is_err(),
                        "a neighbour's answer"
                    );
                }
                assert!(check_tir_step_node_disclosure_v1(trace, execution, 0, index, &f, MAX).is_err(), "a leaf is not a node");
                assert!(!past(PalwDaUnitV1::TirStepNode { level, index }), "({level}, {index}) is in the execution");
            }
            assert!(build_tir_step_node_disclosure_v1(&x.binding, level, width, &x, MAX).is_err(), "past level {level}");
            assert!(past(PalwDaUnitV1::TirStepNode { level, index: width }), "past level {level}'s width");
        }
        assert!(past(PalwDaUnitV1::TirStepNode { level: height + 1, index: 0 }), "above the root");
        assert!(past(PalwDaUnitV1::TirStepLeaf { index: count }), "past the leaves");
        assert!(!past(PalwDaUnitV1::TirStepLeaf { index: count - 1 }), "the last leaf is in the execution");
        assert!(!past(PalwDaUnitV1::Event { row: 0, tile: 0 }), "an out-of-range proof answers an IR step unit only");
        let other_roots = check_tir_step_out_of_range_v1(
            Hash64::from_bytes([9; 64]),
            execution,
            &PalwDaUnitV1::TirStepLeaf { index: count },
            &x.binding,
            MAX,
        );
        assert!(other_roots.is_err(), "the binding must be the claim's");
    }

    /// **Every node of the tiny execution's rows tree is disclosed** — rows-tree nodes with their
    /// frontiers, rows with their tile leaves, each with the ids — and checks against the claim's trace
    /// root; tampered answers, a neighbour's, and a unit past the trace are refused, which its
    /// out-of-range proof answers.
    #[test]
    fn every_rows_tree_node_is_disclosed_and_a_row_past_the_trace_is_proven_so() {
        use crate::palw_da_rcore_v1::PalwDaUnitV1;
        let x = tiny_execution(None);
        let rows = x.rows.len() as u64;
        assert!(rows >= 2, "{rows} rows");
        let (trace, execution) = (x.binding.full_logits_trace_root, x.binding.committed_execution_root);
        let filled = |d: &PalwTirRowNodeDisclosureV1| {
            let mut f = d.clone();
            f.binding.class.program = x.binding.class.program.clone();
            f
        };
        let check = |level: u8, index: u64, d: &PalwTirRowNodeDisclosureV1| {
            check_tir_row_node_disclosure_v1(trace, execution, level, index, d, MAX)
        };
        let height = palw_tir_step_tree_height_v1(rows);
        for level in 0..=height {
            let width = palw_tir_step_tree_width_v1(rows, level).unwrap();
            for index in 0..width {
                let d = build_tir_row_node_disclosure_v1(&x.binding, level, index, &x, MAX)
                    .unwrap_or_else(|e| panic!("rows node ({level}, {index}): {e}"));
                assert!(d.binding.class.program.is_empty(), "it rides without the program");
                let f = filled(&d);
                check(level, index, &f).unwrap_or_else(|e| panic!("rows node ({level}, {index}): {e}"));
                if index + 1 < width {
                    assert!(check(level, index + 1, &f).is_err(), "({level}, {index}) does not answer its neighbour");
                }
                let mut ids = f.clone();
                ids.generated_token_ids[0] ^= 1;
                assert!(check(level, index, &ids).is_err(), "another id breaks the trace root");
                let mut frontier = f.clone();
                frontier.frontier[0] = Hash64::from_bytes([0xF0; 64]);
                assert!(check(level, index, &frontier).is_err(), "a tampered frontier");
                if !f.siblings.is_empty() {
                    let mut siblings = f.clone();
                    siblings.siblings[0] = Hash64::from_bytes([0xF1; 64]);
                    assert!(check(level, index, &siblings).is_err(), "a tampered sibling");
                }
                let past =
                    check_tir_step_out_of_range_v1(trace, execution, &PalwDaUnitV1::TirRowNode { level, index }, &x.binding, MAX);
                assert!(past.is_err(), "({level}, {index}) is in the trace");
            }
            let past =
                check_tir_step_out_of_range_v1(trace, execution, &PalwDaUnitV1::TirRowNode { level, index: width }, &x.binding, MAX);
            assert!(past.is_ok(), "past level {level}'s width");
            assert!(build_tir_row_node_disclosure_v1(&x.binding, level, width, &x, MAX).is_err());
        }
        assert!(
            check_tir_step_out_of_range_v1(
                trace,
                execution,
                &PalwDaUnitV1::TirRowNode { level: height + 1, index: 0 },
                &x.binding,
                MAX
            )
            .is_ok()
        );
        // A row's frontier is exactly its tiles: one short is refused.
        let row = build_tir_row_node_disclosure_v1(&x.binding, 0, 0, &x, MAX).unwrap();
        let mut short = filled(&row);
        short.frontier.push(Hash64::from_bytes([0xF2; 64]));
        assert!(check(0, 0, &short).is_err(), "a row carries exactly its tile leaves");
    }
}

/// **F6 D: an IR claim's data-availability answer, and the identity rule over an IR binding.**
#[cfg(test)]
mod da_tests {
    use super::test_support::{DECODE, TinyExecution, attempt_binding, tiny_execution};
    use super::*;
    use crate::palw_offence_attribution_v1::{
        PalwClaimSourceKindV1, PalwIdentityFaultV1, PalwIdentityRulesV1, PalwOffenceTargetV1, palw_tir_binding_identity_fault_v1,
    };
    use crate::palw_offence_v1::PalwOffenceVerifyError;
    use crate::palw_tir_attempt_v1::palw_tir_attempt_prompt_root_v1;

    const MAX: u64 = 1 << 26;

    /// The same execution re-committed under the flat scheme: the class id, the context, every leaf
    /// hash and the trace root follow the program's scheme bytes.
    fn flat_of(x: &TinyExecution) -> PalwTirStepBindingV1 {
        let mut b = x.binding.clone();
        let mut program = b.class.decode_program().unwrap();
        program.logits_scheme_id.copy_from_slice(flat_logits_scheme_id_v1().as_byte_slice());
        b.class.program = program.encode();
        let class_id = b.class.class_id(&b.artifact_root);
        b.job_context.shape_profile_id = class_id;
        b.job_context.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2();
        let ctx_hash = b.job_context.context_hash();
        let hashes: Vec<Hash64> = x.preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &class_id, p)).collect();
        b.step_merkle_root = crate::palw_step_leg::step_merkle_root_v1(&hashes).unwrap();
        b.full_logits_trace_root = base0_logits_trace_root_v1(&b.job_context, &x.rows, &x.generated);
        b.committed_execution_root =
            palw_tir_execution_root_v1(&ctx_hash, &b.full_logits_trace_root, &class_id, b.step_leaf_count, &b.step_merkle_root);
        b
    }

    fn check(b: &PalwTirStepBindingV1, row: u32, tile: u8, d: &PalwTirTraceEventDisclosureV1) -> Result<(), PalwStepRefuteError> {
        check_tir_trace_event_disclosure_v1(b.full_logits_trace_root, b.committed_execution_root, row, tile, d, MAX)
    }

    #[test]
    fn an_ir_event_is_opened_in_its_class_s_scheme_and_nothing_else_answers() {
        let x = tiny_execution(None);
        let b = &x.binding;
        let out = PalwTirTraceEventDisclosureV1::OutOfRange { binding: Box::new(b.clone()) };
        // Tiled: every row's one tile opens; a bent lane, another row's claim and another claim's
        // roots are refused.
        for row in 0..DECODE {
            let d = tir_logits_event_disclosure_v1(b, &x.rows, &x.generated, row, 0).expect("the event is in the run");
            assert!(!d.is_flat());
            assert_eq!(check(b, row, 0, &d), Ok(()), "row {row}");
            assert!(check(b, (row + 1) % DECODE, 0, &d).is_err(), "row {row}: opened as another row");
            assert!(check(b, row, 0, &out).is_err(), "row {row}: an event in the run is not declared absent");
            let PalwTirTraceEventDisclosureV1::Tiled { mut tile_lanes, .. } = d.clone() else { unreachable!() };
            tile_lanes[0] ^= 1;
            let bent = match d.clone() {
                PalwTirTraceEventDisclosureV1::Tiled { binding, generated_token_ids, row_root, row_opening, tile_opening, .. } => {
                    PalwTirTraceEventDisclosureV1::Tiled {
                        binding,
                        generated_token_ids,
                        row_root,
                        row_opening,
                        tile_lanes,
                        tile_opening,
                    }
                }
                _ => unreachable!(),
            };
            assert!(check(b, row, 0, &bent).is_err(), "row {row}: a bent lane");
            let mut other = Hash64::from_bytes([9; 64]);
            assert!(
                check_tir_trace_event_disclosure_v1(other, b.committed_execution_root, row, 0, &d, MAX).is_err(),
                "another trace root"
            );
            other = Hash64::from_bytes([8; 64]);
            assert!(
                check_tir_trace_event_disclosure_v1(b.full_logits_trace_root, other, row, 0, &d, MAX).is_err(),
                "another execution root"
            );
        }
        // Out of the run: a row past the decode count, a tile past the vocabulary's one tile.
        assert!(tir_logits_event_disclosure_v1(b, &x.rows, &x.generated, DECODE, 0).is_none());
        assert!(tir_logits_event_disclosure_v1(b, &x.rows, &x.generated, 0, 1).is_none());
        assert_eq!(check(b, DECODE, 0, &out), Ok(()));
        assert_eq!(check(b, 0, 1, &out), Ok(()));
        // Flat: the whole pin answers every row's tile 0; the tiled form is not its scheme.
        let f = flat_of(&x);
        let d = tir_logits_event_disclosure_v1(&f, &x.rows, &x.generated, 1, 0).expect("in the run");
        assert!(d.is_flat());
        for row in 0..DECODE {
            assert_eq!(check(&f, row, 0, &d), Ok(()), "flat row {row}");
        }
        assert!(check(&f, 0, 1, &d).is_err(), "the flat scheme has one tile");
        let out_flat = PalwTirTraceEventDisclosureV1::OutOfRange { binding: Box::new(f.clone()) };
        assert_eq!(check(&f, 0, 1, &out_flat), Ok(()));
        assert_eq!(check(&f, DECODE, 0, &out_flat), Ok(()));
        assert!(check(&f, 0, 0, &out_flat).is_err());
        let tiled = tir_logits_event_disclosure_v1(b, &x.rows, &x.generated, 0, 0).unwrap();
        let PalwTirTraceEventDisclosureV1::Tiled { generated_token_ids, row_root, row_opening, tile_lanes, tile_opening, .. } = tiled
        else {
            unreachable!()
        };
        let wrong_form = PalwTirTraceEventDisclosureV1::Tiled {
            binding: Box::new(f.clone()),
            generated_token_ids,
            row_root,
            row_opening,
            tile_lanes,
            tile_opening,
        };
        assert!(check(&f, 0, 0, &wrong_form).is_err(), "a tiled opening on a flat class");
        // A binding that does not verify answers nothing.
        let mut broken = f.clone();
        broken.step_leaf_count += 1;
        let d = PalwTirTraceEventDisclosureV1::OutOfRange { binding: Box::new(broken.clone()) };
        assert!(check(&broken, DECODE, 0, &d).is_err());
    }

    fn target_of(b: &PalwTirStepBindingV1, identity: Hash64) -> PalwOffenceTargetV1 {
        PalwOffenceTargetV1 {
            claim_id: Hash64::from_bytes([0xC1; 64]),
            class_id: b.job_context.shape_profile_id,
            artifact_root: b.artifact_root,
            executor_bond: crate::palw_state_v2::PALW_BOND_KEY_V2_MIN,
            execution_root: b.committed_execution_root,
            lane: Some(PalwClaimSourceKindV1::Attempt),
            segment_count: None,
            phase: None,
            job_identity: identity,
            trace_root: b.full_logits_trace_root,
            output_root: Hash64::default(),
        }
    }

    #[test]
    fn the_identity_rule_over_an_ir_binding() {
        let rules = PalwIdentityRulesV1 {
            prompt_ids_form: PalwPromptIdsFormV1::Flat,
            base_class_id: Hash64::default(),
            da_signer_liability: false,
        };
        let anchor = Hash64::from_bytes([0xA5; 64]);
        let fault = |b: &PalwTirStepBindingV1, t: &PalwOffenceTargetV1| palw_tir_binding_identity_fault_v1(t, b, rules, true, MAX);
        let (honest, _) = attempt_binding(anchor, |_| {});
        let target = target_of(&honest, anchor);
        assert_eq!(fault(&honest, &target), Ok(None), "the anchor's own canonical job");
        // J2: another class recorded.
        let mut t = target.clone();
        t.class_id = Hash64::from_bytes([1; 64]);
        assert_eq!(fault(&honest, &t), Ok(Some(PalwIdentityFaultV1::ClassNotTheClaims)));
        // J1 / J3: the anchor is not the job's, or seeds another.
        let other = Hash64::from_bytes([0xA6; 64]);
        let (b, _) = attempt_binding(anchor, |ctx| ctx.job_id = other);
        assert_eq!(fault(&b, &target_of(&b, anchor)), Ok(Some(PalwIdentityFaultV1::JobNotTheClaims)));
        let (b, _) = attempt_binding(anchor, |ctx| ctx.execution_seed[0] ^= 1);
        assert_eq!(fault(&b, &target_of(&b, anchor)), Ok(Some(PalwIdentityFaultV1::SeedNotTheJobs)));
        // J5a: any other field of the context moved.
        for (what, edit) in [
            ("the decode count", Box::new(|c: &mut PalwJobContextV2| c.exact_decode_tokens = 2) as Box<dyn Fn(&mut PalwJobContextV2)>),
            ("the network", Box::new(|c: &mut PalwJobContextV2| c.network_id = b"testnet-12".to_vec())),
            ("the tokenizer", Box::new(|c: &mut PalwJobContextV2| c.tokenizer_id = Hash64::from_bytes([3; 64]))),
            ("the scheme", Box::new(|c: &mut PalwJobContextV2| c.trace_scheme_id = crate::palw_v2::trace_scheme_id_v2())),
        ] {
            let (b, _) = attempt_binding(anchor, edit);
            assert_eq!(fault(&b, &target_of(&b, anchor)), Ok(Some(PalwIdentityFaultV1::ContextNotCanonical)), "{what}");
        }
        // J5b: the prompt root is another anchor's.
        let (b, facts) = attempt_binding(anchor, |_| {});
        let wrong = palw_tir_attempt_prompt_root_v1(&facts, &other, 7, PalwPromptIdsFormV1::Flat).unwrap();
        let (b2, _) = attempt_binding(anchor, |ctx| ctx.prompt_token_ids_hash = wrong);
        assert_eq!(fault(&b2, &target_of(&b2, anchor)), Ok(Some(PalwIdentityFaultV1::PromptNotTheAnchors)));
        assert_eq!(palw_tir_binding_identity_fault_v1(&target_of(&b2, anchor), &b2, rules, false, MAX), Ok(None), "J5b left out");
        // J4: the claim committed another trace root.
        let mut t = target_of(&b, anchor);
        t.trace_root = Hash64::from_bytes([2; 64]);
        assert_eq!(fault(&b, &t), Ok(Some(PalwIdentityFaultV1::TraceNotTheClaims)));
        // Refusals: another execution, no identity, a binding that does not verify.
        let mut t = target_of(&b, anchor);
        t.execution_root = Hash64::from_bytes([4; 64]);
        assert_eq!(fault(&b, &t), Err(PalwOffenceVerifyError::PanelFalseValidWorkMismatch));
        assert_eq!(fault(&b, &target_of(&b, Hash64::default())), Err(PalwOffenceVerifyError::IdentityNotRecorded));
        let mut broken = b.clone();
        broken.step_leaf_count += 1;
        assert_eq!(fault(&broken, &target_of(&b, anchor)), Err(PalwOffenceVerifyError::BindingUnverified));
        // A class too narrow for the formula: every other check runs, then it is not derivable.
        let x = tiny_execution(None);
        let mut ctx = x.binding.job_context.clone();
        ctx.job_id = anchor;
        ctx.execution_seed.copy_from_slice(&anchor.as_byte_slice()[..32]);
        let mut narrow = x.binding.clone();
        narrow.job_context = ctx;
        let class_id = narrow.class.class_id(&narrow.artifact_root);
        narrow.committed_execution_root = palw_tir_execution_root_v1(
            &narrow.job_context.context_hash(),
            &narrow.full_logits_trace_root,
            &class_id,
            narrow.step_leaf_count,
            &narrow.step_merkle_root,
        );
        assert_eq!(fault(&narrow, &target_of(&narrow, anchor)), Err(PalwOffenceVerifyError::IdentityNotDerivable));
    }
}
