//! **The evidence an IR court close carries, built by a node** (RFC-0002 Phase F, F9 over F5's
//! `palw_tir_court_v1`; design §2.6–§2.7).
//!
//! The court adjudicates one committed step leaf by demand-evaluating its cone over units the close
//! carries; which units is the court's own question (`build_tir_cone_refutation_v1` runs the court's
//! evaluation over a store and records what it reads). This module is the node's store:
//! [`TirEvidenceV1`] answers every [`PalwTirEvidenceStoreV1`] request from one execution —
//!
//! * **step leaves** by re-deriving them: a leaf's position is replayed from the retained resume
//!   point at or before it (design §2.6; the job's start when none is held) — or read from a dense
//!   capture, which holds every preimage;
//! * **step openings** from the step tree ([`TirStepTreeV1`]) — the whole tree for a party's own
//!   execution, or, for a challenger, the ACCUSED's tree as far as the challenger's own leaves before
//!   the disputed one and the accused's opening of it determine it;
//! * **artifact openings** from the class's inventory tree ([`super::inventory`]);
//! * the **prompt** in the network's form and the **decode pin** under the class's logits scheme,
//!   from the job and the committed trace.
//!
//! The same store answers both parties, as the legacy prover does: the executor defending an honest
//! leaf and a challenger accusing a false one assemble the one canonical object, and the court's
//! verdict is what tells them apart. The logits-consistency accusation and the decode-token door are
//! built here from the same retention.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_artifact::{PalwArtifactMultiproofV1, PalwArtifactOpeningV1};
use kaspa_consensus_core::palw_prompt_ids_v1::{
    PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, PalwPromptIdsOpeningV1, prompt_ids_opening_v1,
};
use kaspa_consensus_core::palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{
    PALW_LOGITS_TILE_LANES, PalwBase0DecodeTokensV1, PalwDecodeTokenPinV1, PalwTiledDecodePinV1, PalwTiledDecodeTokensV1,
    flat_logits_scheme_id_v1, tiled_decode_pin_v1, tiled_logits_row_root_v1, tiled_logits_rows_root_v1, tiled_logits_scheme_id_v1,
    tiled_logits_tile_leaf_v1,
};
use kaspa_consensus_core::palw_tir_class_v1::PalwTirClassV1;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, PalwTirCourtRulesV1, PalwTirEvidenceErrorV1, PalwTirEvidenceStoreV1, PalwTirLogitsConsistencyV1,
    PalwTirTraceLanesV1, build_tir_cone_refutation_bounded_v1, palw_tir_row_node_parts_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;

use super::inventory::TirParamOpenerV1;
use super::run::{TirClassRunnerV1, TirResumePointV1};
use super::tree::TirStepTreeV1;

/// **A job as its executor retains it**: the binding it committed, the job's prompt, the committed
/// logits trace, every leaf hash, and a resume point after every checkpoint position.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirRetainedJobV1 {
    pub binding: PalwTirStepBindingV1,
    pub prompt: Vec<u32>,
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
    pub leaf_hashes: Vec<Hash64>,
    pub points: Vec<TirResumePointV1>,
}

impl TirClassRunnerV1<'_> {
    /// **Run a job and retain it** — the producer's side: the run's roots bound into the IR
    /// binding (the class carried whole, its id recomputed from `artifact_root`), and what the
    /// evidence builder re-derives every leaf from.
    pub fn retain(
        &self,
        class: &PalwTirClassV1,
        artifact_root: Hash64,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
    ) -> Result<TirRetainedJobV1, String> {
        if class.class_id(&artifact_root) != self.class_id {
            return Err("the class and artifact root name another class than this runner's".into());
        }
        let (run, points) = self.run_recording(ctx, prompt, cap, &mut |_| {})?;
        let binding = PalwTirStepBindingV1 {
            version: PALW_TIR_STEP_BINDING_VERSION_V1,
            job_context: ctx.clone(),
            class: class.clone(),
            artifact_root,
            full_logits_trace_root: run.trace_root,
            step_leaf_count: run.leaf_count,
            step_merkle_root: run.step_merkle_root,
            committed_execution_root: run.execution_root,
        };
        Ok(TirRetainedJobV1 {
            binding,
            prompt: prompt.to_vec(),
            logits_rows: run.logits_rows,
            generated: run.generated,
            leaf_hashes: run.leaf_hashes,
            points,
        })
    }
}

/// **The node-side bisection state at `index`** (the IR twin of the legacy families' prefix
/// state): a keyed hash over the job context and the first `index` leaf hashes. Two executions
/// agreeing on every leaf before `index` agree here, and two differing before it do not — the
/// property the bisection ladder converges by.
pub fn tir_bisect_prefix_state_v1(ctx: &PalwJobContextV2, leaves: &[Hash64], index: u64) -> Hash64 {
    const DOMAIN: &[u8] = b"misaka-palw/tir/bisect-prefix-state/v1";
    let take = (index as usize).min(leaves.len());
    let mut h = blake2b_simd::Params::new().hash_length(64).key(DOMAIN).to_state();
    h.update(ctx.context_hash().as_byte_slice());
    h.update(&index.to_le_bytes());
    h.update(&(take as u64).to_le_bytes());
    for leaf in &leaves[..take] {
        h.update(leaf.as_byte_slice());
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(h.finalize().as_bytes());
    Hash64::from_bytes(out)
}

/// **The first leaf two executions of one job differ at** — by comparing prefix states (the
/// ladder's own question), `None` when they agree everywhere. `O(log n)` prefix states.
pub fn tir_first_divergence_v1(ctx: &PalwJobContextV2, a: &[Hash64], b: &[Hash64]) -> Option<u64> {
    let n = a.len().max(b.len()) as u64;
    if tir_bisect_prefix_state_v1(ctx, a, n) == tir_bisect_prefix_state_v1(ctx, b, n) && a.len() == b.len() {
        return None;
    }
    // Invariant: the prefixes of length `lo` agree, those of length `hi` do not.
    let (mut lo, mut hi) = (0u64, n);
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if tir_bisect_prefix_state_v1(ctx, a, mid) == tir_bisect_prefix_state_v1(ctx, b, mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    Some(lo)
}

/// Where the store's step leaves come from.
enum Leaves<'a> {
    /// Every preimage, held (a dense capture).
    Dense(&'a [PalwStepTileLeafV1]),
    /// Only the authenticated preimages fetched for the current cell/court.
    Sparse(&'a BTreeMap<u64, PalwStepTileLeafV1>),
    /// Re-derived by replay from the resume points; the disputed leaf, when it is the accused's,
    /// overrides the replay's.
    Replay { points: &'a [TirResumePointV1], disputed: Option<(u64, PalwStepTileLeafV1)> },
}

/// The committed trace a decode pin authenticates against.
/// **The committed trace, as far as a store holds it.**
#[derive(Clone, Copy)]
pub enum TirTraceV1<'a> {
    /// Every committed row and id — a capture's, or a run's own.
    Rows { rows: &'a [Vec<i32>], generated: &'a [u32] },
    /// **What a served annex or an on-chain root claim carries of it** (the tiled scheme): the rows
    /// root and the ids (the trace root's preimage beside the row count), and the pins it holds —
    /// each one row's committed-token tile and the tile holding some lane, opened under the row, the
    /// row under the rows root. A pin answers every lane of its beat tile (the lane is an index into
    /// lanes the pin carries whole), so the logits door and the decode-token door are built from it
    /// without the rows.
    Summary { rows_root: Hash64, generated: &'a [u32], pins: &'a [PalwTiledDecodePinV1] },
    /// None of it: a store whose evaluations read no trace (a dissection's bottom whose root claim
    /// carried no decode pin). An evaluation that asks for it is refused, never guessed.
    Absent,
}

impl TirTraceV1<'_> {
    /// The pin of row `row` whose beat tile holds `lane`, re-aimed at `lane` — from the rows, or from
    /// a held pin of that row and tile.
    pub fn pin_v1(&self, ctx: &PalwJobContextV2, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
        self.pin(ctx, row, lane)
    }

    fn pin(&self, ctx: &PalwJobContextV2, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
        match self {
            Self::Rows { rows, generated } => tiled_decode_pin_v1(ctx, rows, generated, row, lane),
            Self::Absent => None,
            Self::Summary { pins, .. } => pins
                .iter()
                .find(|p| {
                    p.position == row
                        && p.beat_lane as usize / PALW_LOGITS_TILE_LANES == lane as usize / PALW_LOGITS_TILE_LANES
                        && (lane as usize % PALW_LOGITS_TILE_LANES) < p.beat_tile_lanes.len()
                })
                .map(|p| PalwTiledDecodePinV1 { beat_lane: lane, ..p.clone() }),
        }
    }
}

/// **The node's evidence store for one execution** — see the module doc.
pub struct TirEvidenceV1<'a> {
    runner: &'a TirClassRunnerV1<'a>,
    binding: &'a PalwTirStepBindingV1,
    prompt: &'a [u32],
    trace: TirTraceV1<'a>,
    tree: Box<dyn super::sparse_tree::TirProofTreeV1>,
    leaves: Leaves<'a>,
    params: &'a dyn TirParamOpenerV1,
    prompt_form: PalwPromptIdsFormV1,
    cap: u64,
    /// One target position's replayed leaves. The admission budget also includes the
    /// transient range hashes, logits rows, generated ids and native Vec headers.
    cache: RefCell<BTreeMap<u32, Vec<PalwStepTileLeafV1>>>,
    cache_budget: usize,
    max_replay_positions: u32,
    replay_positions_used: Cell<u64>,
}

impl<'a> TirEvidenceV1<'a> {
    /// **The executor's own store** over its retention (the responder, or a party whose execution
    /// IS the committed one).
    pub fn own(
        runner: &'a TirClassRunnerV1<'a>,
        job: &'a TirRetainedJobV1,
        params: &'a dyn TirParamOpenerV1,
        prompt_form: PalwPromptIdsFormV1,
        cap: u64,
    ) -> Result<Self, String> {
        let tree = Self::check(runner, &job.binding, &job.leaf_hashes)?;
        Ok(Self {
            runner,
            binding: &job.binding,
            prompt: &job.prompt,
            trace: TirTraceV1::Rows { rows: &job.logits_rows, generated: &job.generated },
            tree: Box::new(tree),
            leaves: Leaves::Replay { points: &job.points, disputed: None },
            params,
            prompt_form,
            cap,
            cache: RefCell::new(BTreeMap::new()),
            cache_budget: 1 << 30,
            max_replay_positions: 4096,
            replay_positions_used: Cell::new(0),
        })
    }

    /// **A store over a dense capture** — every preimage held, so no replay: the accused's own
    /// committed units, whoever holds them (the legacy families' "both sides through one prover").
    #[allow(clippy::too_many_arguments)]
    pub fn dense(
        runner: &'a TirClassRunnerV1<'a>,
        binding: &'a PalwTirStepBindingV1,
        prompt: &'a [u32],
        logits_rows: &'a [Vec<i32>],
        generated: &'a [u32],
        preimages: &'a [PalwStepTileLeafV1],
        params: &'a dyn TirParamOpenerV1,
        prompt_form: PalwPromptIdsFormV1,
        cap: u64,
    ) -> Result<Self, String> {
        let ctx_hash = binding.job_context.context_hash();
        let hashes: Vec<Hash64> = preimages.iter().map(|p| step_tile_leaf_hash_v1(&ctx_hash, &runner.class_id, p)).collect();
        let tree = Self::check(runner, binding, &hashes)?;
        Ok(Self {
            runner,
            binding,
            prompt,
            trace: TirTraceV1::Rows { rows: logits_rows, generated },
            tree: Box::new(tree),
            leaves: Leaves::Dense(preimages),
            params,
            prompt_form,
            cap,
            cache: RefCell::new(BTreeMap::new()),
            cache_budget: 0,
            max_replay_positions: 0,
            replay_positions_used: Cell::new(0),
        })
    }

    /// Sparse public evidence: the requested ranges and their Merkle siblings, with
    /// no N-leaf allocation or prefix replay. Missing units remain unavailable.
    #[allow(clippy::too_many_arguments)]
    pub fn runs(
        runner: &'a TirClassRunnerV1<'a>,
        capture: &'a super::backend::TirCaptureV1,
        inputs: &'a super::cell::TirRunInputsV1,
        params: &'a dyn TirParamOpenerV1,
        prompt_form: PalwPromptIdsFormV1,
        cap: u64,
    ) -> Result<Self, String> {
        super::backend::tir_authenticate_capture_metadata_v1(capture, runner.class_id, prompt_form, cap)?;
        let tree = inputs.proof_tree_v1(capture, runner.class_id, cap)?;
        Ok(Self {
            runner, binding: &capture.binding, prompt: &capture.prompt,
            trace: TirTraceV1::Rows { rows: &capture.logits_rows, generated: &capture.generated },
            tree: Box::new(tree), leaves: Leaves::Sparse(inputs.preimages_v1()), params,
            prompt_form, cap, cache: RefCell::new(BTreeMap::new()), cache_budget: 0, max_replay_positions: 0, replay_positions_used: Cell::new(0),
        })
    }

    /// **A challenger's store against a claim it re-executed honestly**: its own execution
    /// (`own`, the same job) supplies every leaf before the disputed one — equal to the accused's,
    /// as the bisection narrowed to the first leaf they differ at — and the accused supplies the
    /// disputed leaf (`disputed_preimage` and its opening against the accused's root) and the
    /// committed trace (`accused_rows`, `accused_generated`: what the accused's trace root commits).
    #[allow(clippy::too_many_arguments)]
    pub fn challenger(
        runner: &'a TirClassRunnerV1<'a>,
        accused: &'a PalwTirStepBindingV1,
        own: &'a TirRetainedJobV1,
        disputed_opening: &PalwStepOpeningV1,
        disputed_preimage: PalwStepTileLeafV1,
        accused_rows: &'a [Vec<i32>],
        accused_generated: &'a [u32],
        params: &'a dyn TirParamOpenerV1,
        prompt_form: PalwPromptIdsFormV1,
        cap: u64,
    ) -> Result<Self, String> {
        let trace = TirTraceV1::Rows { rows: accused_rows, generated: accused_generated };
        Self::challenger_with_trace(runner, accused, own, disputed_opening, disputed_preimage, trace, params, prompt_form, cap)
    }

    /// [`Self::challenger`] over the accused's committed trace as a SERVED ANNEX or an on-chain root
    /// claim carries it ([`TirTraceV1::Summary`]) — what a challenger holds of a claim whose capture no
    /// transport carries (RFC-0002's evidence transport, options B and D).
    #[allow(clippy::too_many_arguments)]
    pub fn challenger_with_trace(
        runner: &'a TirClassRunnerV1<'a>,
        accused: &'a PalwTirStepBindingV1,
        own: &'a TirRetainedJobV1,
        disputed_opening: &PalwStepOpeningV1,
        disputed_preimage: PalwStepTileLeafV1,
        trace: TirTraceV1<'a>,
        params: &'a dyn TirParamOpenerV1,
        prompt_form: PalwPromptIdsFormV1,
        cap: u64,
    ) -> Result<Self, String> {
        if own.binding.job_context != accused.job_context || own.binding.class != accused.class {
            return Err("the challenger's execution is not of the accused's job and class".into());
        }
        let ctx_hash = accused.job_context.context_hash();
        if step_tile_leaf_hash_v1(&ctx_hash, &runner.class_id, &disputed_preimage) != disputed_opening.leaf_hash {
            return Err("the disputed preimage is not the opened leaf".into());
        }
        let tree = TirStepTreeV1::prefix_with_opening(&own.leaf_hashes, accused.step_leaf_count, disputed_opening)?;
        if tree.root() != Some(accused.step_merkle_root) {
            return Err("the prefix and the accused's opening do not produce the accused's step root".into());
        }
        Self::check_binding(runner, accused)?;
        Ok(Self {
            runner,
            binding: accused,
            prompt: &own.prompt,
            trace,
            tree: Box::new(tree),
            leaves: Leaves::Replay { points: &own.points, disputed: Some((disputed_opening.leaf_index, disputed_preimage)) },
            params,
            prompt_form,
            cap,
            cache: RefCell::new(BTreeMap::new()),
            cache_budget: 1 << 30,
            max_replay_positions: 4096,
            replay_positions_used: Cell::new(0),
        })
    }

    fn check_binding(runner: &TirClassRunnerV1<'_>, binding: &PalwTirStepBindingV1) -> Result<(), String> {
        if binding.class.class_id(&binding.artifact_root) != runner.class_id || binding.job_context.shape_profile_id != runner.class_id
        {
            return Err("the binding is of another class than this runner's".into());
        }
        Ok(())
    }

    /// The held leaves' tree, refused unless it is the binding's.
    fn check(runner: &TirClassRunnerV1<'_>, binding: &PalwTirStepBindingV1, hashes: &[Hash64]) -> Result<TirStepTreeV1, String> {
        Self::check_binding(runner, binding)?;
        if hashes.len() as u64 != binding.step_leaf_count {
            return Err(format!("{} leaves held for a binding of {}", hashes.len(), binding.step_leaf_count));
        }
        let tree = TirStepTreeV1::full(hashes);
        if tree.root() != Some(binding.step_merkle_root) {
            return Err("the held leaves do not produce the binding's step root".into());
        }
        Ok(tree)
    }

    pub fn binding(&self) -> &PalwTirStepBindingV1 {
        self.binding
    }

    /// **The canonical cone refutation of step leaf `leaf`** (F5's builder over this store).
    pub fn cone_refutation(&self, leaf: u64, rules: &PalwTirCourtRulesV1) -> Result<PalwTirConeRefutationV1, PalwTirEvidenceErrorV1> {
        build_tir_cone_refutation_bounded_v1(self.binding, leaf, self, rules,
            super::backend::tir_court_builder_limits_v1(rules)).map(|(object, _)| object)
    }

    /// **The logits-consistency accusation over logits step leaf `leaf`**: the step tile and the
    /// same row's lanes in the committed trace, in the class's scheme.
    pub fn logits_consistency(&self, leaf: u64) -> Result<PalwTirLogitsConsistencyV1, String> {
        let ctx = &self.binding.job_context;
        let space = self.runner.space;
        let l = space.leaf_at(ctx, leaf).ok_or_else(|| format!("leaf {leaf} is not a leaf of this job"))?;
        let PalwTirLeafKindV1::Commit { first_element, .. } = l.kind else {
            return Err(format!("leaf {leaf} is not a commit tile"));
        };
        let prefill = self.binding.job_context.declared_prefill_tokens;
        let row = (l.position + 1).checked_sub(prefill).ok_or("a leaf before the first selecting position")?;
        let scheme = Hash64::from_bytes(space.program.logits_scheme_id);
        let trace = if scheme == flat_logits_scheme_id_v1() {
            let TirTraceV1::Rows { rows, generated } = self.trace else {
                return Err("the flat scheme's door carries every row, and this store holds a summary".into());
            };
            PalwTirTraceLanesV1::Flat(PalwBase0DecodeTokensV1 { logits_rows: rows.to_vec(), generated_token_ids: generated.to_vec() })
        } else if scheme == tiled_logits_scheme_id_v1() {
            let beat = u32::try_from(first_element).map_err(|_| "a logits lane past u32")?;
            let pin = self.trace.pin(ctx, row, beat).ok_or("the trace this store holds has no such row or lane")?;
            debug_assert_eq!(beat as usize / PALW_LOGITS_TILE_LANES, pin.beat_lane as usize / PALW_LOGITS_TILE_LANES);
            PalwTirTraceLanesV1::Tiled {
                generated_token_ids: pin.generated_token_ids,
                row_root: pin.row_root,
                row_opening: pin.row_opening,
                tile_lanes: pin.beat_tile_lanes,
                tile_opening: pin.beat_opening,
            }
        } else {
            return Err("the program names no logits scheme".into());
        };
        Ok(PalwTirLogitsConsistencyV1 {
            binding: self.binding.clone(),
            step_opening: self.tree.opening(leaf).ok_or("the step tree does not open that leaf")?,
            step_preimage: self.step_leaf(leaf).ok_or("the store does not hold that leaf")?,
            trace,
        })
    }

    /// **The tiled decode-token pin** for decode row `row`, the lane `beat_lane` said to beat the
    /// committed token (the tiled scheme's door).
    pub fn decode_token_pin(&self, row: u32, beat_lane: u32) -> Option<PalwTiledDecodePinV1> {
        self.trace.pin(&self.binding.job_context, row, beat_lane)
    }

    /// Override local replay admission for a tool/resource probe. Actual node stores
    /// default to at most 4096 positions and 1 GiB of requested replay buffers.
    pub fn with_replay_limits_v1(mut self, positions: u32, requested_bytes: usize) -> Self {
        self.max_replay_positions = positions;
        self.cache_budget = requested_bytes;
        self.replay_positions_used.set(0);
        self.cache.borrow_mut().clear();
        self
    }

    pub fn replay_positions_used_v1(&self) -> u64 { self.replay_positions_used.get() }

    /// Requested native buffer storage for one replay: range hashes, selecting logits
    /// rows, generated ids and the target position's leaf headers/lanes. Executor
    /// tensors, retained resume points, the immutable proof tree and allocator/RSS
    /// overhead are separate; the fixed 16 KiB allowance covers the single-entry map.
    pub fn replay_requested_storage_v1(&self, a: u32) -> Option<usize> {
        let Leaves::Replay { points, .. } = &self.leaves else { return None };
        let ctx = &self.binding.job_context;
        let space = self.runner.space;
        let job = space.job_shape(ctx).ok()?;
        if a >= job.positions { return None; }
        let first = points.iter().rev().find(|p| p.position < a)
            .map_or(Some(0), |p| p.position.checked_add(1))?;
        let end = a.checked_add(1)?;
        if end.checked_sub(first)? > self.max_replay_positions { return None; }
        let hashes = space.running_total(&job, end).checked_sub(space.running_total(&job, first))?;
        let selected = end.saturating_sub(job.prefill - 1) as u128;
        let rows = selected.checked_sub(first.saturating_sub(job.prefill - 1) as u128)?;
        let post = space.program.schedule.post as usize;
        let width = space.program.blocks.get(post)?.nodes.get(space.program.logits as usize)?.out.elements_at(1) as u128;
        let leaf_headers = u128::from(space.leaves_at_position(&job, a))
            .checked_mul(std::mem::size_of::<PalwStepTileLeafV1>() as u128)?;
        let leaf_lanes = space.lane_count_at_position_v1(&job, a).checked_mul(4)?;
        let trace = rows.checked_mul(width.checked_mul(4)?.checked_add(std::mem::size_of::<Vec<i32>>() as u128)?)?;
        // drive converts one selecting row through i128 before retaining i32, and
        // reuses an emitter buffer whose canonical leaf has at most 65,536 lanes.
        let row_conversion = if rows == 0 { 0 } else { width.checked_mul(16)? };
        let emitter = (kaspa_consensus_core::palw_step::PALW_STEP_MAX_TILE_LEN as u128).checked_mul(8)?;
        let bytes = hashes.checked_mul(std::mem::size_of::<Hash64>() as u128)?
            .checked_add(trace)?.checked_add(row_conversion)?.checked_add(emitter)?
            .checked_add(selected.checked_mul(4)?)?
            .checked_add(leaf_headers)?.checked_add(leaf_lanes)?.checked_add(16 << 10)?;
        usize::try_from(bytes).ok()
    }

    /// Replay only after bounded admission, keeping the target position's preimages.
    /// Earlier replayed positions are consumed by the executor and never cloned here.
    fn replay_through(&self, a: u32, points: &[TirResumePointV1], disputed: Option<&(u64, PalwStepTileLeafV1)>) -> Option<()> {
        let requested = self.replay_requested_storage_v1(a)?;
        if requested > self.cache_budget { return None; }
        let point = points.iter().rev().find(|p| p.position < a);
        let first = point.map_or(Some(0), |p| p.position.checked_add(1))?;
        let span = u64::from(a.checked_add(1)?.checked_sub(first)?);
        let total = self.replay_positions_used.get().checked_add(span)?;
        if total > u64::from(self.max_replay_positions) { return None; }
        // Failed work consumes the move's budget too; cache misses cannot multiply
        // 4096 positions by an unbounded number of logical source reads.
        self.replay_positions_used.set(total);
        let ctx = &self.binding.job_context;
        let job = self.runner.space.job_shape(ctx).ok()?;
        let count = usize::try_from(self.runner.space.leaves_at_position(&job, a)).ok()?;
        let lane_bytes = usize::try_from(self.runner.space.lane_count_at_position_v1(&job, a).checked_mul(4)?).ok()?;
        // Drop all prior positions before transient buffers are allocated: even one
        // oversized position is refused, rather than retained beyond the cache cap.
        self.cache.borrow_mut().clear();
        let mut fresh = Vec::new();
        fresh.try_reserve_exact(count).ok()?;
        let (mut copied_bytes, mut malformed) = (0usize, false);
        self.runner
            .replay(ctx, self.prompt, self.cap, point, Some(a.checked_add(1)?), &mut |leaf| {
                if leaf.position != a || malformed { return; }
                let preimage = match disputed {
                    Some((index, accused)) if *index == leaf.index => accused,
                    _ => leaf.preimage,
                };
                let Some(next) = copied_bytes.checked_add(preimage.values_le.len()) else { malformed = true; return; };
                if fresh.len() == count || next > lane_bytes { malformed = true; return; }
                copied_bytes = next;
                fresh.push(preimage.clone());
            }).ok()?;
        if malformed || fresh.len() != count || copied_bytes != lane_bytes { return None; }
        self.cache.borrow_mut().insert(a, fresh);
        Some(())
    }

}

impl PalwTirEvidenceStoreV1 for TirEvidenceV1<'_> {
    fn begin_builder_move_v1(&self) { self.replay_positions_used.set(0); }

    fn step_leaf(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        match &self.leaves {
            Leaves::Dense(all) => all.get(index as usize).cloned(),
            Leaves::Sparse(all) => all.get(&index).cloned(),
            Leaves::Replay { points, disputed } => {
                if let Some((i, accused)) = disputed {
                    if *i == index {
                        return Some(accused.clone());
                    }
                    if index > *i {
                        // Past the disputed leaf the challenger holds nothing of the accused's.
                        return None;
                    }
                }
                let ctx = &self.binding.job_context;
                let space = self.runner.space;
                let leaf = space.leaf_at(ctx, index)?;
                let a = leaf.position;
                if !self.cache.borrow().contains_key(&a) {
                    self.replay_through(a, points, disputed.as_ref())?;
                }
                let job = space.job_shape(ctx).ok()?;
                let first = space.running_total(&job, a) as u64;
                self.cache.borrow().get(&a)?.get((index - first) as usize).cloned()
            }
        }
    }

    fn step_opening(&self, index: u64) -> Option<PalwStepOpeningV1> {
        self.tree.opening(index)
    }

    fn step_range_siblings(&self, first: u64, count: u64) -> Option<Vec<Hash64>> {
        self.tree.range_siblings(first, count)
    }

    fn param_opening(&self, leaf: u32) -> Option<PalwArtifactOpeningV1> {
        self.params.param_opening(leaf)
    }

    /// One multiproof read off the held inventory's levels (the builder's bytes; see
    /// [`super::inventory::TirInventoryTreeV1::multiproof`]) — not assembled from a path per leaf.
    fn param_multiproof(&self, leaves: &[u32]) -> Option<PalwArtifactMultiproofV1> {
        self.params.param_multiproof(leaves)
    }

    /// The carriage the class's artifact makes: one multiproof, or — a composite candidate's — the
    /// two sub-roots' (RFC-0004 §6.3).
    fn param_carriage(&self, leaves: &[u32]) -> Option<kaspa_consensus_core::palw_tir_court_v1::PalwTirParamOpeningV1> {
        self.params.param_carriage(leaves)
    }

    fn prompt_token_ids(&self) -> Option<Vec<u32>> {
        Some(self.prompt.to_vec())
    }

    fn prompt_ids_opening(&self, tile: u32) -> Option<PalwPromptIdsOpeningV1> {
        if self.prompt_form != PalwPromptIdsFormV1::MerkleV1 {
            return None;
        }
        prompt_ids_opening_v1(self.prompt, tile.checked_mul(PALW_PROMPT_IDS_TILE_LEN)?).ok()
    }

    fn decode_pin(&self) -> Option<PalwDecodeTokenPinV1> {
        let scheme = Hash64::from_bytes(self.runner.space.program.logits_scheme_id);
        match (self.trace, scheme) {
            (TirTraceV1::Rows { rows, generated }, s) if s == tiled_logits_scheme_id_v1() => {
                Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 {
                    rows_root: tiled_logits_rows_root_v1(&self.binding.job_context, rows)?,
                    generated_token_ids: generated.to_vec(),
                }))
            }
            (TirTraceV1::Summary { rows_root, generated, .. }, s) if s == tiled_logits_scheme_id_v1() => {
                Some(PalwDecodeTokenPinV1::TiledV1(PalwTiledDecodeTokensV1 { rows_root, generated_token_ids: generated.to_vec() }))
            }
            (TirTraceV1::Rows { rows, generated }, s) if s == flat_logits_scheme_id_v1() => {
                Some(PalwDecodeTokenPinV1::Base0V1(PalwBase0DecodeTokensV1 {
                    logits_rows: rows.to_vec(),
                    generated_token_ids: generated.to_vec(),
                }))
            }
            _ => None,
        }
    }

    /// **The tiled row pin of decode row `row`, aimed at lane `lane`** (the second IR fence's
    /// `TirStepLeaf` answer): from the rows (a capture's, or this node's own run), or from a held pin
    /// of that row whose beat tile holds the lane (a served annex's, an on-chain disclosure's).
    fn row_pin(&self, row: u32, lane: u32) -> Option<PalwTiledDecodePinV1> {
        self.trace.pin(&self.binding.job_context, row, lane)
    }

    /// **Step-tree node `(level, index)`'s frontier and opening** (the second IR fence's descent
    /// unit), off this store's tree — whole for the executor's own capture or run; a challenger's
    /// partial tree answers only the nodes it holds.
    fn step_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        self.tree.node_parts(level, index)
    }

    /// **Rows-tree node `(level, index)` of the committed tiled trace** (the second IR fence's
    /// `TirRowNode`): from the rows this store holds — a capture's, or this node's own run
    /// (`palw_tir_row_node_parts_v1`). A store holding only a summary of the trace answers none.
    fn row_node(&self, level: u8, index: u64) -> Option<(Vec<Hash64>, Vec<Hash64>)> {
        match self.trace {
            TirTraceV1::Rows { rows, .. } => palw_tir_row_node_parts_v1(&self.binding.job_context, rows, level, index),
            TirTraceV1::Summary { .. } | TirTraceV1::Absent => None,
        }
    }
}

/// **The rows tree of a tiled logits trace, held level by level** — the step tree's own shape over
/// the row roots (`tiled_logits_row_root_v1`), which is what a claim's `TirRowNode` answers open: a
/// seat whose own steps are the claim's descends the claim's rows tree against this one. `None` for
/// no rows, or rows that build no root.
pub fn tir_rows_tree_v1(ctx: &PalwJobContextV2, rows: &[Vec<i32>]) -> Option<TirStepTreeV1> {
    let ctx_hash = ctx.context_hash();
    let roots = rows
        .iter()
        .enumerate()
        .map(|(r, row)| tiled_logits_row_root_v1(&ctx_hash, u32::try_from(r).ok()?, row))
        .collect::<Option<Vec<Hash64>>>()?;
    (!roots.is_empty()).then(|| TirStepTreeV1::full(&roots))
}

/// **Row `row`'s tile leaves** (`tiled_logits_tile_leaf_v1`, one per 4,096 lanes) — the frontier of
/// a level-0 `TirRowNode` answer, set against a claim's to name the tile it disputes.
pub fn tir_row_tile_leaves_v1(ctx: &PalwJobContextV2, rows: &[Vec<i32>], row: u32) -> Option<Vec<Hash64>> {
    let ctx_hash = ctx.context_hash();
    let lanes = rows.get(row as usize)?;
    Some(
        lanes
            .chunks(PALW_LOGITS_TILE_LANES)
            .enumerate()
            .map(|(t, tile)| tiled_logits_tile_leaf_v1(&ctx_hash, row, t as u32, tile))
            .collect(),
    )
}
