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
//! that does not precede it is refused as an interpreter defect, never answered.
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
use crate::palw_artifact::{PalwArtifactOpeningV1, verify_artifact_opening_v1};
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
    /// The artifact openings of every inventory leaf the evaluation reads, ascending leaf index.
    pub params: Vec<PalwArtifactOpeningV1>,
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
    /// The whole prompt (flat form).
    fn prompt_token_ids(&self) -> Option<Vec<u32>>;
    /// The opening of prompt tile `tile` (Merkle form).
    fn prompt_ids_opening(&self, tile: u32) -> Option<PalwPromptIdsOpeningV1>;
    /// The generated ids' pin under the class's logits scheme.
    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1>;
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

/// The carried artifact openings, authenticated against the class's `artifact_root`: ascending,
/// each exactly one whole inventory leaf of the class.
fn authenticate_params(
    binding: &PalwTirStepBindingV1,
    v: &PalwTirVerifiedBindingV1,
    inventory: &PalwTirInventoryIndexV1,
    openings: &[PalwArtifactOpeningV1],
) -> Result<BTreeMap<u32, (u32, Vec<u8>)>, PalwStepRefuteError> {
    let mut out = BTreeMap::new();
    let mut last: Option<u32> = None;
    for o in openings {
        if last.is_some_and(|l| l >= o.leaf_index) {
            return Err(bad("the artifact openings are not in ascending leaf order"));
        }
        last = Some(o.leaf_index);
        if o.leaf_count != inventory.leaf_count() {
            return Err(bad("an artifact opening is of another inventory"));
        }
        let (param, layer, start, len) = inventory.piece_of(o.leaf_index).ok_or(bad("an artifact opening names no leaf"))?;
        let d = &v.space.program.params[param as usize];
        if o.operand.tensor_name != d.name
            || o.operand.layer != layer
            || o.operand.row_start != start
            || o.operand.bytes.len() != len as usize
        {
            return Err(bad("an artifact opening is not its leaf's canonical piece"));
        }
        verify_artifact_opening_v1(o, binding.artifact_root)
            .map_err(|_| bad("an artifact opening does not reach the class's root"))?;
        out.insert(o.leaf_index, (o.operand.row_start, o.operand.bytes.clone()));
    }
    Ok(out)
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
/// 7. **the evaluation** of the disputed leaf's values — a unit it needs and the refutation does not
///    carry is `InputSetNotCanonical`; any other failure `Unadjudicable`;
/// 8. **the canonical set** — every carried unit was read, and the prompt and decode carriages are
///    present exactly when read; otherwise `InputSetNotCanonical`;
/// 9. **the comparison** — the first differing lane convicts
///    ([`PalwStepFaultV1::ComputationMismatch`], kind 5, the leaf); none is `NoFaultFound`.
pub fn check_tir_cone_refutation_v1(
    refutation: &PalwTirConeRefutationV1,
    rules: &PalwTirCourtRulesV1,
) -> Result<PalwStepRefutationVerdictV1, PalwStepRefuteError> {
    let binding = &refutation.binding;
    // 1.
    let v = match check_binding(binding)? {
        BindingOutcome::Convicted(verdict) => return Ok(verdict),
        BindingOutcome::Verified(v) => v,
    };
    let committed = &binding.committed_execution_root;
    // 2.
    let leaf =
        match check_output_leaf(binding, &v, &refutation.output_opening, &refutation.output_preimage, rules.max_step_leaf_count)? {
            Ok(leaf) => leaf,
            Err(verdict) => return Ok(verdict),
        };
    let out_index = refutation.output_opening.leaf_index;
    // 3.
    let intervals = analyze_ranges(&v.space.program).map_err(|_| PalwStepRefuteError::Unadjudicable)?;
    // 4.
    let out_interval = palw_tir_leaf_interval_v1(&v.space, &intervals, &leaf).ok_or(PalwStepRefuteError::Unadjudicable)?;
    if let Some(i) = first_outside(leaf.dtype, &refutation.output_preimage.values_le, out_interval) {
        let fault = PalwStepFaultV1::TirValueOutsideProvenInterval { value_index: i };
        return Ok(convict(committed, PALW_TIR_EVIDENCE_KIND_CONE, out_index, fault));
    }
    // 5.
    let operands = authenticate_operands(binding, &v, &refutation.operands, out_index, rules.max_step_leaf_count)?;
    let inventory = PalwTirInventoryIndexV1::new(&v.space.program).ok_or(PalwStepRefuteError::Unadjudicable)?;
    let params = authenticate_params(binding, &v, &inventory, &refutation.params)?;
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
            return Ok(convict(committed, PALW_TIR_EVIDENCE_KIND_CONE, *index, fault));
        }
    }
    // 7.
    let mut source = TirSource {
        space: &v.space,
        ctx: &binding.job_context,
        job: v.job,
        inventory: &inventory,
        units: Units {
            steps: operands.iter().zip(refutation.operands.preimages.iter()).map(|((i, _), p)| (*i, p.values_le.clone())).collect(),
            params,
            prompt,
            generated,
            store: None,
            prompt_form: rules.prompt_form,
        },
        before: out_index,
        used: Requests::default(),
        leaf_cache: BTreeMap::new(),
    };
    let values = evaluate_leaf(&v.space, &leaf, &mut source, &rules.limits).map_err(|e| evaluation_refusal(&e))?;
    // 8.
    let used = &source.used;
    if used.steps.len() != operands.len() || !operands.iter().all(|(i, _)| used.steps.contains(i)) {
        return Err(bad("the operand row is not the set the evaluation reads"));
    }
    if used.params.len() != refutation.params.len() {
        return Err(bad("the artifact openings are not the set the evaluation reads"));
    }
    match rules.prompt_form {
        PalwPromptIdsFormV1::Flat => {
            if used.prompt.is_empty() != refutation.prompt_token_ids.is_empty() {
                return Err(bad("the prompt ids ride exactly when the evaluation reads one"));
            }
        }
        PalwPromptIdsFormV1::MerkleV1 => {
            let read: BTreeSet<u32> = used.prompt.iter().map(|p| p / PALW_PROMPT_IDS_TILE_LEN).collect();
            if read != prompt_tiles {
                return Err(bad("the prompt openings are not the tiles the evaluation reads"));
            }
        }
    }
    if used.decode != refutation.decode_tokens.is_some() {
        return Err(bad("the decode pin rides exactly when the evaluation reads a generated token"));
    }
    // 9.
    let committed_values: Vec<i128> = (0..leaf.value_count as usize)
        .map(|i| lane(leaf.dtype, &refutation.output_preimage.values_le, i).unwrap_or(i128::MIN))
        .collect();
    if let Some(i) = values.iter().zip(committed_values.iter()).position(|(a, b)| a != b) {
        let fault = PalwStepFaultV1::ComputationMismatch { value_index: i as u32 };
        return Ok(convict(committed, PALW_TIR_EVIDENCE_KIND_CONE, out_index, fault));
    }
    Err(PalwStepRefuteError::NoFaultFound)
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
    let mut source = TirSource {
        space: &v.space,
        ctx: &binding.job_context,
        job: v.job,
        inventory: &inventory,
        units: Units {
            steps: BTreeMap::new(),
            params: BTreeMap::new(),
            prompt: BTreeMap::new(),
            generated: None,
            store: Some(store),
            prompt_form: rules.prompt_form,
        },
        before: output_leaf,
        used: Requests::default(),
        leaf_cache: BTreeMap::new(),
    };
    evaluate_leaf(&v.space, &leaf, &mut source, &rules.limits).map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?;
    let used = source.used;
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
    let mut params = Vec::with_capacity(used.params.len());
    for leaf in &used.params {
        params.push(store.param_opening(*leaf).ok_or_else(|| missing(format!("inventory leaf {leaf}")))?);
    }
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
            // The layout tiles the logits node at the scheme's width (F4), so step tile t is trace tile t.
            let tile = first_element / PALW_LOGITS_TILE_LANES as u64;
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
            tile_lanes.iter().map(|x| *x as i128).collect()
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
