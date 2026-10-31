//! Explicit, bounded fixed-length greedy execution with FLAT prompt IDs for the
//! adopted i64 Q24 profile. Other prompt forms require a separate explicit API.
//!
//! The ordinary CPU executor and borrowed observer are shared with V1. A job is
//! executed once; no narrow run or second replay constructs its commitments.
//! This is a producer/data API, not backend dispatch, native admission, a public
//! FP/V4/device adapter, a Court verdict or credit authorization.
use std::mem::size_of;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_decode_select_v2::PalwDecodeSamplingV2,
    palw_logits_q24_v2 as wide,
    palw_prompt_ids_v1::{PalwPromptIdsFormV1, prompt_token_ids_match_v1},
    palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1},
    palw_step_refute::PALW_LOGITS_TILE_LANES,
    palw_tir_step_v1::{PalwTirStepSpaceV2, palw_tir_execution_root_v1, palw_tir_leaf_preimage_v2},
    palw_v2::PalwJobContextV2,
};
use misaka_palw_tir::{StateKind, TirError, TirErrorKind, admit::node_cost};

use super::{
    memory::{self, TirMemoryLeaseV1},
    run::{ObservedSink, TirRunObserverV1},
};
use crate::{
    Elem, Slice, TirExecutor, TirParams, TirPlan,
    exec::{NodeValue, StepSink},
    with_slice,
};

/// Explicit logical request ceilings; no successful default or network preset.
#[derive(Clone, Copy, Debug)]
pub struct TirWideRunLimitsV2 {
    pub max_positions: u32,
    pub max_step_leaves: u64,
    pub max_row_lanes: u64,
    pub max_work: u64,
    pub max_requested_bytes: u64,
}

/// Simultaneous logical backing and symbolic work, not measured RSS/instructions.
/// Caller-owned plan/params/space/prompt and arbitrary callback retention are separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirWideRunQuoteV2 {
    pub leaf_count: u64,
    /// Conservative scheduled-node bound, including post at every position.
    pub node_visits: u64,
    /// Sum of checked symbolic kernel dimensions plus conservative host work.
    pub work: u64,
    pub executor_bytes: u64,
    pub retained_bytes: u64,
    pub scratch_bytes: u64,
    pub requested_bytes: u64,
}

/// A borrowed mixed-width preimage. Explicit copies require caller reservations.
pub struct TirLeafOutWideQ24V2<'a> {
    pub index: u64,
    pub position: u32,
    pub preimage: &'a PalwStepTileLeafV1,
    pub hash: Hash64,
}

/// Immutable native i64 rows and original-context commitments. The optional host
/// reservation remains owned until this result is dropped. No Clone/owned-buffer
/// extraction is supplied; copies through borrowed getters are caller-owned.
pub struct TirJobRunWideQ24V2 {
    leaf_count: u64,
    step_merkle_root: Hash64,
    logits_rows: Vec<Vec<i64>>,
    generated: Vec<u32>,
    trace_root: Hash64,
    execution_root: Hash64,
    output_root: Hash64,
    leaf_hashes: Vec<Hash64>,
    reserved_bytes: u64,
    _memory: Option<TirMemoryLeaseV1>,
}
impl std::fmt::Debug for TirJobRunWideQ24V2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TirJobRunWideQ24V2")
            .field("leaf_count", &self.leaf_count)
            .field("rows", &self.logits_rows.len())
            .field("generated", &self.generated)
            .field("execution_root", &self.execution_root)
            .field("reserved_bytes", &self.reserved_bytes)
            .finish()
    }
}
impl TirJobRunWideQ24V2 {
    pub fn leaf_count(&self) -> u64 {
        self.leaf_count
    }
    pub fn step_merkle_root(&self) -> Hash64 {
        self.step_merkle_root
    }
    pub fn logits_rows(&self) -> &[Vec<i64>] {
        &self.logits_rows
    }
    pub fn generated(&self) -> &[u32] {
        &self.generated
    }
    pub fn trace_root(&self) -> Hash64 {
        self.trace_root
    }
    pub fn execution_root(&self) -> Hash64 {
        self.execution_root
    }
    pub fn output_root(&self) -> Hash64 {
        self.output_root
    }
    pub fn leaf_hashes(&self) -> &[Hash64] {
        &self.leaf_hashes
    }
    pub fn reserved_bytes(&self) -> u64 {
        self.reserved_bytes
    }
}

/// Strict V2 geometry cannot be converted into a legacy runner/step space.
/// No custom selector, fused/drill toggle, device or public route is installed.
pub struct TirClassRunnerWideQ24V2<'a> {
    space: &'a PalwTirStepSpaceV2,
    plan: &'a TirPlan,
    params: &'a TirParams<'a>,
    class_id: Hash64,
}

fn add(a: u64, b: u64) -> Result<u64, String> {
    a.checked_add(b).ok_or_else(|| "wide request arithmetic overflow".into())
}
fn mul(a: u64, b: u64) -> Result<u64, String> {
    a.checked_mul(b).ok_or_else(|| "wide request arithmetic overflow".into())
}
fn count(n: usize) -> Result<u64, String> {
    u64::try_from(n).map_err(|_| "wide request coordinate overflow".into())
}
fn coordinate(n: usize) -> Result<u32, String> {
    u32::try_from(n).map_err(|_| "wide wire coordinate overflow".into())
}
fn slot(base: u32, offset: usize) -> Result<u32, String> {
    base.checked_add(coordinate(offset)?).ok_or_else(|| "wide wire slot overflow".into())
}
fn reserve<T>(v: &mut Vec<T>, n: usize) -> Result<(), String> {
    v.try_reserve_exact(n).map_err(|_| "cannot reserve wide run backing")?;
    if v.capacity() != n {
        return Err("wide run allocator exceeded exact backing request".into());
    }
    Ok(())
}

impl<'a> TirClassRunnerWideQ24V2<'a> {
    pub fn new(space: &'a PalwTirStepSpaceV2, plan: &'a TirPlan, params: &'a TirParams<'a>, class_id: Hash64) -> Result<Self, String> {
        if space.program() != &plan.program || !plan.program.wide_logits_q24() {
            return Err("wide runner requires the actual strict-wide program and matching plan".into());
        }
        Ok(Self { space, plan, params, class_id })
    }

    /// Pure closed-form preflight. No position/leaf list or encoded program is allocated.
    pub fn quote(&self, ctx: &PalwJobContextV2, limits: TirWideRunLimitsV2) -> Result<TirWideRunQuoteV2, String> {
        if ctx.shape_profile_id != self.class_id {
            return Err("wide job names another class".into());
        }
        let job = self.space.job_shape(ctx).map_err(|e| e.to_string())?;
        if limits.max_positions == 0 || job.positions > limits.max_positions || limits.max_work == 0 || limits.max_requested_bytes == 0
        {
            return Err("wide run position/work/backing ceiling".into());
        }
        let leaf_count = self.space.leaf_count_capped(ctx, limits.max_step_leaves).map_err(|e| e.to_string())?;
        let p = &self.plan.program;
        let vocab = p.blocks[p.schedule.post as usize].nodes[p.logits as usize].out.elements_at(1);
        if vocab == 0 || vocab > u64::from(u32::MAX) || vocab > limits.max_row_lanes {
            return Err("wide logits row ceiling".into());
        }
        usize::try_from(leaf_count).map_err(|_| "wide leaf count exceeds native indices")?;
        usize::try_from(vocab).map_err(|_| "wide vocabulary exceeds native indices")?;
        let positions = u64::from(job.positions);
        let mut per_position = 0;
        let mut nodes = 0;
        for &(block, _) in self.space.occurrences() {
            let b = block as usize;
            let h = u64::from(self.space.info().blocks[b].window.unwrap_or(1));
            for n in 0..p.blocks[b].nodes.len() {
                nodes = add(nodes, 1)?;
                let c = node_cost(p, b, n, h);
                for cost in [c.macs, c.elementwise, c.transcendentals, c.bytes_read, c.bytes_written] {
                    if cost == u64::MAX {
                        return Err("wide symbolic kernel cost saturated".into());
                    }
                    per_position = add(per_position, cost)?;
                }
            }
        }
        let node_visits = mul(nodes, positions)?;
        let state_rows = self.plan.instances.iter().try_fold(0, |n, i| {
            add(
                n,
                match i.kind {
                    StateKind::Fixed { .. } => 1,
                    StateKind::Hist { window } => u64::from(window.min(job.positions)),
                },
            )
        })?;
        // Empty scheduled blocks and per-position observer state scans still
        // cost host work even when they emit no node or checkpoint leaf.
        let structural = mul(
            mul(positions, add(count(self.space.occurrences().len())?, add(state_rows, count(self.plan.instances.len())?)?)?)?,
            64,
        )?;
        let encoded = borsh::object_length(p).map_err(|e| format!("wide program length: {e}"))? as u64;
        let geometry = add(
            add(encoded, nodes)?,
            add(
                count(self.space.occurrences().len())?,
                add(count(self.space.fixed_instances().len())?, count(self.space.hist_instances().len())?)?,
            )?,
        )?;
        // Re-deriving each canonical leaf may scan the program and binary-search
        // u32 positions. Include metadata/identity, byte hashing and selection.
        let payload = self.space.maximum_leaf_payload_bytes_v2(ctx).map_err(|e| e.to_string())?;
        let host = add(
            add(
                mul(add(leaf_count, 1)?, mul(add(geometry, 1024)?, 128)?)?,
                mul(mul(vocab, u64::from(ctx.exact_decode_tokens))?, 128)?,
            )?,
            add(mul(mul(leaf_count, payload)?, 8)?, mul(u64::from(job.prefill), 64)?)?,
        )?;
        let work = add(mul(per_position, positions)?, add(host, structural)?)?;
        if work > limits.max_work {
            return Err("wide symbolic work ceiling".into());
        }
        let executor_bytes = memory::tir_executor_storage_need_wide_q24_v2(self.space, self.plan, ctx)?;
        let retained_bytes = add(
            mul(leaf_count, size_of::<Hash64>() as u64)?,
            mul(u64::from(ctx.exact_decode_tokens), add(mul(vocab, 8)?, (size_of::<Vec<i64>>() + size_of::<u32>()) as u64)?)?,
        )?;
        let tiles = vocab.div_ceil(PALW_LOGITS_TILE_LANES as u64);
        let scratch_bytes = [
            memory::tir_full_tree_storage_need_v1(leaf_count)?,
            memory::tir_full_tree_storage_need_v1(u64::from(ctx.exact_decode_tokens))?,
            memory::tir_full_tree_storage_need_v1(tiles)?,
            add(mul(payload, 5)?, 512)?,     // i128 lanes + preimage + hash writer growth
            mul(u64::from(job.prefill), 8)?, // flat prompt writer and capacity growth
            add(mul(encoded, 2)?, 16384)?,
        ]
        .into_iter()
        .try_fold(0, add)?;
        let requested_bytes = add(executor_bytes, add(retained_bytes, scratch_bytes)?)?;
        if requested_bytes > limits.max_requested_bytes {
            return Err("wide simultaneous backing ceiling".into());
        }
        Ok(TirWideRunQuoteV2 { leaf_count, node_visits, work, executor_bytes, retained_bytes, scratch_bytes, requested_bytes })
    }

    pub fn run(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        expected_execution: Hash64,
        limits: TirWideRunLimitsV2,
        on_leaf: &mut dyn FnMut(&TirLeafOutWideQ24V2<'_>),
    ) -> Result<TirJobRunWideQ24V2, String> {
        self.drive(ctx, prompt, expected_execution, limits, on_leaf, None)
    }

    /// Every-node/state/native-output observation within this same single run.
    /// Error invalidates all partial observations and prevents further execution.
    #[allow(clippy::too_many_arguments)]
    pub fn run_observed(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        expected_execution: Hash64,
        limits: TirWideRunLimitsV2,
        on_leaf: &mut dyn FnMut(&TirLeafOutWideQ24V2<'_>),
        observer: &mut dyn TirRunObserverV1,
    ) -> Result<TirJobRunWideQ24V2, String> {
        self.drive(ctx, prompt, expected_execution, limits, on_leaf, Some(observer))
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        expected_execution: Hash64,
        limits: TirWideRunLimitsV2,
        on_leaf: &mut dyn FnMut(&TirLeafOutWideQ24V2<'_>),
        mut observer: Option<&mut dyn TirRunObserverV1>,
    ) -> Result<TirJobRunWideQ24V2, String> {
        let q = self.quote(ctx, limits)?;
        if prompt.len() != ctx.declared_prefill_tokens as usize {
            return Err("wide prompt count differs from actual job".into());
        }
        if prompt.iter().any(|id| *id >= self.plan.program.token_bound) {
            return Err("wide prompt ID exceeds the original program token bound".into());
        }
        // Bounded hashing follows simultaneous request admission; caller pins the
        // independently resolved expected execution, never a delivered replacement.
        let lease = memory::reserve_v1(&memory::memory_reserver_v1(), "tir-wide-q24-v2", self.class_id, ctx, q.requested_bytes)?;
        let context = ctx.context_hash();
        if context != expected_execution {
            return Err("wide original execution pin differs".into());
        }
        if !prompt_token_ids_match_v1(PalwPromptIdsFormV1::Flat, prompt, &ctx.prompt_token_ids_hash) {
            return Err("wide fixed-length flat prompt differs from original job".into());
        }
        let job = self.space.job_shape(ctx).map_err(|e| e.to_string())?;
        let mut hashes = Vec::new();
        reserve(&mut hashes, usize::try_from(q.leaf_count).map_err(|_| "wide leaf count width")?)?;
        let mut rows = Vec::new();
        reserve(&mut rows, ctx.exact_decode_tokens as usize)?;
        let mut generated = Vec::new();
        reserve(&mut generated, ctx.exact_decode_tokens as usize)?;
        let mut emit = WideEmitter { space: self.space, ctx, class_id: self.class_id, context, position: 0, hashes, on_leaf };
        let mut exec = TirExecutor::new(self.plan, self.params).map_err(|e| e.to_string())?;
        exec.set_hist_tail(self.space.layout().h_tile as usize);
        for a in 0..job.positions {
            let token = if a < job.prefill { prompt[a as usize] } else { generated[(a - job.prefill) as usize] };
            emit.position = a;
            let post = job.runs_post(a);
            {
                let mut sink = WideCommitSink { emit: &mut emit };
                if let Some(o) = observer.as_mut() {
                    exec.step_opt(token, &mut ObservedSink::new(&mut sink, &mut **o, a), post)
                } else {
                    exec.step_opt(token, &mut sink, post)
                }
                .map_err(|e| format!("wide position {a}: {e}"))?;
            }
            if let Some(o) = observer.as_mut() {
                exec.visit_state(&mut |state, layer, history, dtype, shape, data| {
                    o.state(a, state, layer, history, dtype, shape, data)
                })?;
            }
            if post {
                let (shape, logits) = exec.logits();
                let declared = self.plan.program.blocks[self.plan.program.schedule.post as usize].nodes
                    [self.plan.program.logits as usize]
                    .out
                    .dtype;
                let mut row = Vec::new();
                reserve(&mut row, logits.len())?;
                with_slice!(logits, values => {
                    for v in values {
                        let v = v.to_i128();
                        if !declared.contains(v) { return Err("wide native output exceeds declared dtype".into()); }
                        row.push(i64::try_from(v).map_err(|_| "wide native output exceeds i64")?);
                    }
                });
                let expected_lanes = self.plan.program.blocks[self.plan.program.schedule.post as usize].nodes
                    [self.plan.program.logits as usize]
                    .out
                    .elements_at(1);
                if row.len() as u64 != expected_lanes {
                    return Err("wide output shape differs from original program".into());
                }
                let selected =
                    wide::select(&row, &PalwDecodeSamplingV2::GREEDY, coordinate(generated.len())?).ok_or("empty wide output")?;
                let selected = u32::try_from(selected).map_err(|_| "wide selected coordinate overflow")?;
                if let Some(o) = observer.as_mut() {
                    o.last_output(a, declared, shape, logits, selected)?;
                }
                rows.push(row);
                generated.push(selected);
            }
            if (a + 1) % self.space.layout().checkpoint_interval == 0 {
                for (k, inst) in self.space.fixed_instances().iter().enumerate() {
                    let data = exec.fixed_value(inst.state, inst.layer).ok_or("missing wide-run narrow Fixed state")?;
                    let elements = usize::try_from(inst.elements).map_err(|_| "wide Fixed native extent overflow")?;
                    for (tile, from) in (0..elements).step_by(inst.tile_lanes as usize).enumerate() {
                        let lanes = (inst.tile_lanes as usize).min(elements - from);
                        emit.native(slot(self.space.program_slots(), k)?, coordinate(tile)?, data, from, lanes)?;
                    }
                }
            }
            let h = self.space.layout().h_tile as usize;
            if (a + 1) % self.space.layout().h_tile == 0 {
                let base = slot(self.space.program_slots(), self.space.fixed_instances().len())?;
                for (k, inst) in self.space.hist_instances().iter().enumerate() {
                    let tail = exec.hist_tail(inst.state, inst.layer).ok_or("missing wide-run narrow history")?;
                    if tail.len() < h {
                        return Err("wide-run history lacks canonical rows".into());
                    }
                    let elements = usize::try_from(inst.elements).map_err(|_| "wide history native extent overflow")?;
                    for (tile, from) in (0..elements).step_by(inst.tile_lanes as usize).enumerate() {
                        let lanes = (inst.tile_lanes as usize).min(elements - from);
                        let mut values = Vec::new();
                        reserve(&mut values, lanes.checked_mul(h).ok_or("wide history count overflow")?)?;
                        for row in tail.iter().skip(tail.len() - h) {
                            // The unchanged validator permits only I8/I16/I32
                            // states; these are signed old four-byte lanes.
                            values.extend(row[from..from + lanes].iter().map(|v| i128::from(*v)));
                        }
                        emit.values(slot(base, k)?, coordinate(tile)?, &values)?;
                    }
                }
            }
            if count(emit.hashes.len())?
                != u64::try_from(self.space.running_total(&job, a + 1)).map_err(|_| "wide running total overflow")?
            {
                return Err("wide execution and canonical position geometry differ".into());
            }
        }
        if emit.hashes.len() as u64 != q.leaf_count {
            return Err("wide execution did not complete every canonical leaf".into());
        }
        let step_merkle_root = step_merkle_root_capped_v1(&emit.hashes, limits.max_step_leaves).map_err(|e| e.to_string())?;
        let trace_root = wide::trace_root(ctx, &rows, &generated).ok_or("wide rows do not form the original trace")?;
        let execution_root = palw_tir_execution_root_v1(&context, &trace_root, &self.class_id, q.leaf_count, &step_merkle_root);
        let output_root = palw_attempt_output_root_v1(ctx, &generated);
        Ok(TirJobRunWideQ24V2 {
            leaf_count: q.leaf_count,
            step_merkle_root,
            logits_rows: rows,
            generated,
            trace_root,
            execution_root,
            output_root,
            leaf_hashes: emit.hashes,
            reserved_bytes: q.requested_bytes,
            _memory: lease,
        })
    }
}

struct WideEmitter<'s, 'f> {
    space: &'s PalwTirStepSpaceV2,
    ctx: &'s PalwJobContextV2,
    class_id: Hash64,
    context: Hash64,
    position: u32,
    hashes: Vec<Hash64>,
    on_leaf: &'f mut dyn FnMut(&TirLeafOutWideQ24V2<'_>),
}
impl WideEmitter<'_, '_> {
    fn native(&mut self, slot: u32, tile: u32, data: Slice<'_>, from: usize, lanes: usize) -> Result<(), String> {
        let end = from.checked_add(lanes).filter(|e| *e <= data.len()).ok_or("wide tile outside native value")?;
        let mut values = Vec::new();
        reserve(&mut values, lanes)?;
        with_slice!(data, v => { values.extend(v[from..end].iter().map(|x| x.to_i128())); });
        self.values(slot, tile, &values)
    }
    fn values(&mut self, slot: u32, tile: u32, values: &[i128]) -> Result<(), String> {
        let index = self.hashes.len() as u64;
        let leaf = self.space.leaf_at(self.ctx, index).ok_or("extra wide execution leaf")?;
        let job = self.space.job_shape(self.ctx).map_err(|e| e.to_string())?;
        let (call, pos) = job.call_position(self.position);
        if leaf.position != self.position
            || leaf.coord.call_index != call
            || leaf.coord.position != pos
            || leaf.coord.node_slot != slot
            || leaf.coord.tile_index != tile
        {
            return Err("wide execution leaf is not the next original canonical coordinate".into());
        }
        // A1 alone selects the eight-byte final leaf versus old narrow bytes.
        let preimage = palw_tir_leaf_preimage_v2(self.space, self.ctx, &leaf, values).map_err(|e| e.to_string())?;
        let hash = step_tile_leaf_hash_v1(&self.context, &self.class_id, &preimage);
        self.hashes.push(hash); // exact whole-job reservation already admitted
        (self.on_leaf)(&TirLeafOutWideQ24V2 { index, position: self.position, preimage: &preimage, hash });
        Ok(())
    }
}
struct WideCommitSink<'a, 's, 'f> {
    emit: &'a mut WideEmitter<'s, 'f>,
}
impl WideCommitSink<'_, '_, '_> {
    fn commit(&mut self, value: &NodeValue<'_>) -> Result<(), String> {
        if !value.commit {
            return Ok(());
        }
        let lanes = self
            .emit
            .space
            .commit_tile_len(value.block, value.node)
            .filter(|n| *n != 0)
            .ok_or("wide committed node lacks original tile")? as usize;
        for (tile, from) in (0..value.data.len()).step_by(lanes).enumerate() {
            self.emit.native(value.slot, coordinate(tile)?, value.data, from, lanes.min(value.data.len() - from))?;
        }
        Ok(())
    }
}
impl StepSink for WideCommitSink<'_, '_, '_> {
    fn node(&mut self, _value: &NodeValue<'_>) {
        unreachable!("wide commit requires the fallible executor sink");
    }
    fn try_node(&mut self, value: &NodeValue<'_>) -> misaka_palw_tir::TirResult<()> {
        self.commit(value).map_err(|e| TirError::new(TirErrorKind::Operand, e))
    }
}
