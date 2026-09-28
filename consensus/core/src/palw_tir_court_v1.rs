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
        if used.params.len() != r.params.len() {
            return Err(bad("the artifact openings are not the set the evaluation reads"));
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
    let elements = leaf_elements(leaf);
    let request =
        DemandRangeRequest { ctx: site.ctx, target: site.node, elements: &elements, supplied: &site.reductions, range: None };
    let (values, _) = eval_demanded_range(&space.program, &space.info, &request, source, limits)?;
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
    let lanes = leaf_elements(&leaf);
    let request = DemandRangeRequest { ctx: site.ctx, target: site.node, elements: &lanes, supplied: &site.reductions, range: None };
    Ok(eval_demanded_range(&v.space.program, &v.space.info, &request, &mut source, &rules.limits)
        .map_err(|e| PalwTirEvidenceErrorV1::Evaluation(e.to_string()))?
        .0)
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
        let z = Hash64::from_bytes([0u8; 64]);
        let ctx = PalwJobContextV2 {
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
            prompt_token_ids_hash: crate::palw_v2::prompt_token_ids_hash_v2(&prompt),
            declared_prefill_tokens: PREFILL,
            exact_decode_tokens: DECODE,
            max_context_tokens: 64,
        };
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
}

/// **F6 D: an IR claim's data-availability answer, and the identity rule over an IR binding.**
#[cfg(test)]
mod da_tests {
    use super::test_support::{DECODE, TinyExecution, tiny_execution};
    use super::*;
    use crate::palw_offence_attribution_v1::{
        PalwClaimSourceKindV1, PalwIdentityFaultV1, PalwIdentityRulesV1, PalwOffenceTargetV1, palw_tir_binding_identity_fault_v1,
    };
    use crate::palw_offence_v1::PalwOffenceVerifyError;
    use crate::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_context_v1, palw_tir_attempt_prompt_root_v1};

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

    /// A binding whose parts verify (the leaf count is the job's and the root recomputes), over a
    /// class with a canonical job: the identity rule needs no execution, only a binding.
    fn attempt_binding(anchor: Hash64, edit: impl FnOnce(&mut PalwJobContextV2)) -> (PalwTirStepBindingV1, PalwTirJobFactsV1) {
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
