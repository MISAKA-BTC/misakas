//! **RFC-0006: a seat verifies a CELL — a layer shard over a position segment — from the claim's committed boundary rows
//! and only that shard's weights** (`docs/rfc/0006-palw-layer-sharded-panels.md` §1–§3, `docs/spec/palw/18-layer-sharded-panels.md`).
//!
//! A cell is a contiguous range of occurrences (`pre` on shard 0, a range of layers, `post` on the last shard) crossed with a
//! position range. The verifier walks the cell's positions in order. At each it:
//!
//! 1. reads the **carry-in** — the committed carry-out leaves of the occurrence before the cell's first (the cell's boundary
//!    row; shard 0 reads the job's tokens instead);
//! 2. runs the cell's occurrences with the executor ([`TirExecutor::step_cell`]) over the shard's own params only;
//! 3. hashes every commit-point tile it recomputed, and every `Fixed` checkpoint and `Hist` tile of the instances the cell
//!    writes, with the claim's own leaf hash, and compares each with the **committed** hash of the same leaf.
//!
//! **The detection lemma** (RFC §2.1): the first leaf of the claim's tree that differs from the honest execution lies in exactly
//! one cell, and that cell's seat finds it — every input of its recomputation precedes the leaf and is honest. A consistent
//! lie at a boundary row is found by the UPSTREAM cell that computes the row, never the downstream one that is handed it.
//!
//! What a cell reads is a [`TirCellInputsV1`]: the committed hash of every leaf of the cell, and the committed preimage of the
//! leaves it is HANDED (the carry-in rows and, resuming a segment, the checkpoint and history tiles). Every value the source
//! gives is authenticated against the claim's step root before the verifier sees it: [`TirCaptureInputsV1`] (a dense
//! capture, its root recomputed) and [`TirRunInputsV1`] (range openings, `TirStepRun`'s form) are the two sources.
//!
//! [`KernelBackendV1`] is the dispatch seam of `docs/design/palw/tir/gpu-integer-backend.md` §8 at cell grain: the CPU backend
//! ([`CpuKernelBackendV1`]) is the function above; a device backend implements the same trait and REFUSES what it cannot run
//! (refusing is always allowed and always correct — B-1), whereupon the caller verifies on the CPU.

use std::collections::BTreeMap;
use std::ops::Range;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::{
    PalwStepRangeOpeningV1, PalwStepTileLeafV1, step_range_opening_root_capped_v1, step_tile_leaf_hash_v1,
};
use kaspa_consensus_core::palw_step_refute::base0_decode_token_select_v1;
use kaspa_consensus_core::palw_tir_step_v1::{PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirLeafKindV1, PalwTirLeafV1, PalwTirStepSpaceV1};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{DType, Prim, RunState, Tensor};

use crate::cellstep::{CpuCellStepperV1, TirCellStepperV1, TirDeviceV1};
use crate::elem::Slice;
use crate::exec::{NodeValue, StepSink, TirExecutor};
use crate::params::TirParams;
use crate::plan::TirPlan;

/// **A cell**: the occurrences `occ` (indices into the program's occurrences: `0` is `pre`, `1 + l` layer `l`, the last `post`)
/// over the absolute positions `positions`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirCellV1 {
    pub shard: u16,
    pub occ: Range<usize>,
    pub positions: Range<u32>,
}

/// **Where a cell's committed inputs come from.** Every answer is authenticated against the claim's step root by the
/// implementation: the verifier never opens a path itself.
pub trait TirCellInputsV1 {
    /// The committed hash of leaf `index` (a leaf the cell recomputes).
    fn leaf_hash(&self, index: u64) -> Option<Hash64>;
    /// The committed preimage of leaf `index` (a leaf the cell is HANDED: a carry-in row, a checkpoint tile, a history tile).
    fn leaf_preimage(&self, index: u64) -> Option<PalwStepTileLeafV1>;
}

/// What a cell's verification came to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TirCellVerdictV1 {
    /// Every leaf the cell produces equals the PALW-TIR function of the cell's committed inputs: the seat may sign `Valid`
    /// for the cell (`leaves` compared over `positions` positions).
    Verified { leaves: u64, positions: u32 },
    /// The first leaf (lowest index) whose recomputed hash differs from the committed one: the cell's finding. It is the input
    /// of the IR one-move court (`TirShardCourtAccused`).
    Faulted { leaf: u64, position: u32 },
    /// A generated id is not the greedy selection over the committed logits of the position that selects it.
    TokenFault { position: u32 },
    /// An input the source cannot supply: the seat abstains (`Unavailable`, not liable).
    Unavailable { leaf: u64 },
    /// The executor refused a step over the committed inputs (or the cell is not one this verifier can run): the seat signs
    /// nothing — an upstream cell holds the divergence, or the request is malformed.
    Refused(String),
}

/// A backend that cannot run a cell says so; the caller verifies on the CPU.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KernelRefusedV1(pub String);

/// **The dispatch seam at cell grain** (`gpu-integer-backend.md` §8; node software, nothing here reaches consensus).
pub trait KernelBackendV1 {
    /// "cpu" or "wgpu/metal Apple M1 Max" — logs and the gate.
    fn name(&self) -> String;
    /// Verify `cell` or refuse it. A refusal costs nothing and is always correct; a verdict is byte-identical to the CPU
    /// backend's (B-2, B-3).
    fn verify_cell(&mut self, req: &TirCellRequestV1<'_>) -> Result<TirCellVerdictV1, KernelRefusedV1>;
    /// Bytes this backend holds now (what the ledger's device slot reserved).
    fn resident_bytes(&self) -> u64 {
        0
    }
    /// The device memory this backend may use, when it is a device: what the node arms its ledger's device pool with
    /// (`arm_device_share_v1`). `None` for the CPU.
    fn device_capacity_bytes(&self) -> Option<u64> {
        None
    }
}

/// Everything a backend reads to verify one cell.
pub struct TirCellRequestV1<'r> {
    pub space: &'r PalwTirStepSpaceV1,
    pub plan: &'r TirPlan,
    /// The shard's params (at least the cell's occurrences' instances).
    pub params: &'r TirParams<'r>,
    pub class_id: Hash64,
    pub ctx: &'r PalwJobContextV2,
    pub cell: &'r TirCellV1,
    /// The committed token of every position `0 .. job positions` (prompt ids, then the generated ids).
    pub tokens: &'r [u32],
    pub inputs: &'r dyn TirCellInputsV1,
    /// Run the fused kernels (byte-identical, off by default).
    pub fused: bool,
}

/// **The CPU backend: the reference function of the module doc.**
#[derive(Clone, Copy, Debug, Default)]
pub struct CpuKernelBackendV1;

impl KernelBackendV1 for CpuKernelBackendV1 {
    fn name(&self) -> String {
        "cpu".to_string()
    }
    fn verify_cell(&mut self, req: &TirCellRequestV1<'_>) -> Result<TirCellVerdictV1, KernelRefusedV1> {
        Ok(verify_cell_v1(req))
    }
}

/// A leaf's lanes as values: `idx` as a `u32`, every other dtype as an `i32` (PALW-TIR-5).
fn values_of_lanes(dtype: DType, le: &[u8]) -> Vec<i128> {
    le.chunks_exact(4)
        .map(|c| {
            let word = [c[0], c[1], c[2], c[3]];
            if dtype == DType::Idx { u32::from_le_bytes(word) as i128 } else { i32::from_le_bytes(word) as i128 }
        })
        .collect()
}

fn lanes_of(data: Slice<'_>, from: usize, n: usize, out: &mut Vec<u8>) {
    crate::cellstep::lanes_le_of_v1(data, from, n, out);
}

/// What the cell's sink collected at one position: every committed tile, by `(node slot, tile)`.
#[derive(Default)]
struct Collected {
    /// `(slot, tile) -> lanes`.
    tiles: BTreeMap<(u32, u32), (u32, Vec<u8>)>,
    /// The tile length of every committed node by slot.
    tile_len: BTreeMap<u32, usize>,
    error: Option<String>,
}

struct CellSink<'s> {
    c: &'s mut Collected,
}

impl StepSink for CellSink<'_> {
    fn node(&mut self, v: &NodeValue<'_>) {
        if !v.commit {
            return;
        }
        let Some(&t) = self.c.tile_len.get(&v.slot) else {
            self.c.error = Some(format!("slot {} is committed without a tile length", v.slot));
            return;
        };
        let n = v.data.len();
        for (tile, from) in (0..n).step_by(t).enumerate() {
            let count = t.min(n - from);
            let mut lanes = Vec::new();
            lanes_of(v.data, from, count, &mut lanes);
            self.c.tiles.insert((v.slot, tile as u32), (count as u32, lanes));
        }
    }
}

/// **Which state instances a cell WRITES** (`Fixed` and `Hist` indices into the step space's lists): the instances written by
/// an occurrence of the cell. A checkpoint or history tile belongs to the cell that writes the state.
fn written_instances(space: &PalwTirStepSpaceV1, occ: &Range<usize>) -> (Vec<usize>, Vec<usize>) {
    let writes = |o: usize, state: u16, layer: Option<u16>| -> bool {
        if !occ.contains(&o) {
            return false;
        }
        let (block, ol) = space.occurrences()[o];
        let wrote = space.program.blocks[block as usize]
            .nodes
            .iter()
            .any(|n| matches!(n.prim, Prim::StateWrite { state: s } | Prim::HistAppend { state: s } if s == state));
        wrote && (layer.is_none() || layer == ol)
    };
    let owned = |instances: &[kaspa_consensus_core::palw_tir_step_v1::PalwTirStateInstanceV1]| -> Vec<usize> {
        instances
            .iter()
            .enumerate()
            .filter(|(_, i)| (0..space.occurrences().len()).any(|o| writes(o, i.state, i.layer)))
            .map(|(k, _)| k)
            .collect()
    };
    (owned(space.fixed_instances()), owned(space.hist_instances()))
}

/// **Verify one cell** on the CPU (the module doc). Pure given its inputs: the same request, the same verdict, on any build.
pub fn verify_cell_v1(req: &TirCellRequestV1<'_>) -> TirCellVerdictV1 {
    let mut exec = match TirExecutor::new_cell(req.plan, req.params, req.cell.occ.clone()) {
        Ok(e) => e,
        Err(e) => return TirCellVerdictV1::Refused(e.to_string()),
    };
    if req.fused {
        exec.set_fused(true);
    }
    exec.set_hist_tail(req.space.layout.h_tile as usize);
    verify_cell_stepping_v1(req, &mut CpuCellStepperV1(exec))
}

/// **Verify one cell over any stepper** — the one verifier. `stepper` is at position 0 with the cell's instances initial (a
/// device builds it from the cell's occurrences' params), or resumable ([`TirCellStepperV1::resume`]).
pub fn verify_cell_stepping_v1(req: &TirCellRequestV1<'_>, exec: &mut dyn TirCellStepperV1) -> TirCellVerdictV1 {
    let (space, ctx, cell) = (req.space, req.ctx, req.cell);
    let job = match space.job_shape(ctx) {
        Ok(job) => job,
        Err(e) => return TirCellVerdictV1::Refused(format!("the job: {e}")),
    };
    let n_occ = space.occurrences().len();
    if cell.occ.start >= cell.occ.end || cell.occ.end > n_occ {
        return TirCellVerdictV1::Refused(format!("a cell of occurrences {:?} of {n_occ}", cell.occ));
    }
    let (p, q) = (cell.positions.start, cell.positions.end.min(job.positions));
    if p >= q {
        return TirCellVerdictV1::Verified { leaves: 0, positions: 0 };
    }
    if req.tokens.len() < job.positions as usize {
        return TirCellVerdictV1::Refused(format!("{} committed tokens for a job of {} positions", req.tokens.len(), job.positions));
    }
    let ctx_hash = ctx.context_hash();
    let (owned_fixed, owned_hist) = written_instances(space, &cell.occ);
    let h_tile = space.layout.h_tile as usize;
    let c_interval = space.layout.checkpoint_interval;
    // The commit tile length of every committed node slot of the cell's occurrences.
    let mut collected = Collected::default();
    for o in cell.occ.clone() {
        let (block, _) = space.occurrences()[o];
        for (ni, node) in space.program.blocks[block as usize].nodes.iter().enumerate() {
            if node.commit
                && let (Some(slot), Some(t)) = (space.node_slot(o, ni as u16), space.commit_tile_len(block, ni as u16))
            {
                collected.tile_len.insert(slot, t as usize);
            }
        }
    }
    let keep = |state: u16, layer: Option<u16>| -> bool {
        owned_fixed.iter().any(|k| {
            let i = &space.fixed_instances()[*k];
            i.state == state && i.layer == layer
        }) || owned_hist.iter().any(|k| {
            let i = &space.hist_instances()[*k];
            i.state == state && i.layer == layer
        })
    };
    // Resuming a segment: the checkpoint and history rows of the cell's own instances, handed as committed leaves.
    if p > 0 {
        match restore_state(req, &job, p, &owned_fixed, &owned_hist) {
            Ok(st) => {
                if let Err(e) = exec.resume(&st.0, &keep, &st.1) {
                    return TirCellVerdictV1::Refused(format!("the cell's state at position {p}: {e}"));
                }
            }
            Err(v) => return v,
        }
    }
    let mut leaves_checked = 0u64;
    for a in p..q {
        let listed = space.leaves_of_position(ctx, a);
        let runs_post = job.runs_post(a);
        let end = if runs_post { cell.occ.end } else { cell.occ.end.min(n_occ - 1) };
        // The leaves of this position the cell recomputes, by coordinate.
        let mut own: BTreeMap<(u32, u32), (u64, u32)> = BTreeMap::new();
        let mut own_state: Vec<(PalwTirLeafV1, bool)> = Vec::new();
        for leaf in &listed {
            match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, .. } if cell.occ.contains(&(occurrence as usize)) => {
                    own.insert((leaf.coord.node_slot, leaf.coord.tile_index), (leaf.index, leaf.value_count));
                }
                PalwTirLeafKindV1::State { instance, .. } if owned_fixed.contains(&(instance as usize)) => own_state.push((*leaf, false)),
                PalwTirLeafKindV1::HistTile { instance, .. } if owned_hist.contains(&(instance as usize)) => own_state.push((*leaf, true)),
                _ => {}
            }
        }
        let (call, position) = job.call_position(a);
        let mut faulted: Option<u64> = None;
        let mut note = |index: u64| {
            faulted = Some(faulted.map_or(index, |f| f.min(index)));
        };
        if end > cell.occ.start {
            // The carry-in: the committed carry-out of the occurrence before the cell's first.
            let carry_in: Vec<Vec<i128>> = if cell.occ.start == 0 {
                Vec::new()
            } else {
                match carry_in_of(req, &listed, cell.occ.start - 1) {
                    Ok(c) => c,
                    Err(leaf) => return TirCellVerdictV1::Unavailable { leaf },
                }
            };
            collected.tiles.clear();
            collected.error = None;
            let token = req.tokens[a as usize];
            let r = exec.step_cell(token, cell.occ.start..end, &carry_in, &mut CellSink { c: &mut collected });
            if let Err(e) = r {
                return TirCellVerdictV1::Refused(format!("position {a}: {e}"));
            }
            if let Some(e) = collected.error.take() {
                return TirCellVerdictV1::Refused(e);
            }
            // Every committed tile the cell recomputed, against the committed hash of the same leaf.
            for ((slot, tile), (count, lanes)) in &collected.tiles {
                let Some(&(index, expected_count)) = own.get(&(*slot, *tile)) else {
                    return TirCellVerdictV1::Refused(format!("position {a}: a tile (slot {slot}, tile {tile}) the step space does not list"));
                };
                if *count != expected_count {
                    note(index);
                    continue;
                }
                let leaf = PalwStepTileLeafV1 {
                    version: PALW_TIR_STEP_LEAF_VERSION_V1,
                    coord: PalwStepCoordinateV1 { call_index: call, node_slot: *slot, position, tile_index: *tile },
                    value_count: *count,
                    values_le: lanes.clone(),
                };
                let mine = step_tile_leaf_hash_v1(&ctx_hash, &req.class_id, &leaf);
                match req.inputs.leaf_hash(index) {
                    None => return TirCellVerdictV1::Unavailable { leaf: index },
                    Some(committed) if committed != mine => note(index),
                    Some(_) => {}
                }
                leaves_checked += 1;
            }
            if collected.tiles.len() != own.len() {
                // A committed tile of the cell that the executor did not produce: the step space and the executor disagree.
                return TirCellVerdictV1::Refused(format!(
                    "position {a}: {} tiles recomputed, the step space lists {} for the cell",
                    collected.tiles.len(),
                    own.len()
                ));
            }
            // The generated id this position selects: the greedy selection over the committed logits (the last shard's).
            if end == n_occ && runs_post {
                let row: Vec<i32> = exec.logits_lanes();
                if !row.is_empty() && a + 1 < job.positions && a + 1 >= job.prefill {
                    let want = base0_decode_token_select_v1(&row) as u32;
                    if want != req.tokens[(a + 1) as usize] {
                        return TirCellVerdictV1::TokenFault { position: a };
                    }
                }
            }
        }
        // `Fixed` checkpoints and `Hist` tiles of the instances the cell writes, after this position's write.
        for (leaf, is_hist) in &own_state {
            let mine = match (*is_hist, leaf.kind) {
                (false, PalwTirLeafKindV1::State { instance, first_element, .. }) => {
                    let inst = &space.fixed_instances()[instance as usize];
                    let mut lanes = Vec::new();
                    if let Err(e) = exec.fixed_lanes(inst.state, inst.layer, first_element as usize, leaf.value_count as usize, &mut lanes) {
                        return TirCellVerdictV1::Refused(e);
                    }
                    lanes
                }
                (true, PalwTirLeafKindV1::HistTile { instance, first_lane, row_lanes, .. }) => {
                    let inst = &space.hist_instances()[instance as usize];
                    let mut lanes = Vec::with_capacity(leaf.value_count as usize * 4);
                    if let Err(e) =
                        exec.hist_tile_lanes(inst.state, inst.layer, h_tile, first_lane as usize, row_lanes as usize, &mut lanes)
                    {
                        return TirCellVerdictV1::Refused(e);
                    }
                    lanes
                }
                _ => continue,
            };
            let committed_leaf = PalwStepTileLeafV1 {
                version: PALW_TIR_STEP_LEAF_VERSION_V1,
                coord: leaf.coord,
                value_count: leaf.value_count,
                values_le: mine,
            };
            let mine = step_tile_leaf_hash_v1(&ctx_hash, &req.class_id, &committed_leaf);
            match req.inputs.leaf_hash(leaf.index) {
                None => return TirCellVerdictV1::Unavailable { leaf: leaf.index },
                Some(committed) if committed != mine => note(leaf.index),
                Some(_) => {}
            }
            leaves_checked += 1;
        }
        let _ = c_interval;
        if let Some(leaf) = faulted {
            return TirCellVerdictV1::Faulted { leaf, position: a };
        }
    }
    TirCellVerdictV1::Verified { leaves: leaves_checked, positions: q - p }
}

/// The carry-in of a cell whose first occurrence is `first_occ + 1`: occurrence `first_occ`'s carry-out nodes' committed tiles,
/// concatenated per carry. `Err(leaf)` names a leaf the source could not supply.
fn carry_in_of(req: &TirCellRequestV1<'_>, listed: &[PalwTirLeafV1], first_occ: usize) -> Result<Vec<Vec<i128>>, u64> {
    let (block, _) = req.space.occurrences()[first_occ];
    let carry_out = &req.space.program.blocks[block as usize].carry_out;
    let mut out = Vec::with_capacity(carry_out.len());
    for node in carry_out {
        let mut lanes: Vec<i128> = Vec::new();
        for leaf in listed {
            if let PalwTirLeafKindV1::Commit { occurrence, node: n, .. } = leaf.kind
                && occurrence as usize == first_occ
                && n == *node
            {
                let pre = req.inputs.leaf_preimage(leaf.index).ok_or(leaf.index)?;
                lanes.extend(values_of_lanes(leaf.dtype, &pre.values_le));
            }
        }
        out.push(lanes);
    }
    Ok(out)
}

/// The run state at the start of position `p` for the cell's own instances, from the committed checkpoint leaves of position
/// `p − 1` and the history tiles ending at it. `p` is a segment start: a multiple of `lcm(C, h_tile)`.
#[allow(clippy::type_complexity)]
fn restore_state(
    req: &TirCellRequestV1<'_>,
    job: &kaspa_consensus_core::palw_tir_step_v1::PalwTirJobShapeV1,
    p: u32,
    owned_fixed: &[usize],
    owned_hist: &[usize],
) -> Result<(RunState, Vec<(u16, Option<u16>, Vec<Vec<i32>>)>), TirCellVerdictV1> {
    let (space, ctx) = (req.space, req.ctx);
    let _ = job;
    let layout = &space.layout;
    let mut st = RunState { pos: p, ..Default::default() };
    let mut tails = Vec::new();
    if !(p).is_multiple_of(layout.checkpoint_interval) || !p.is_multiple_of(layout.h_tile) {
        return Err(TirCellVerdictV1::Refused(format!(
            "a segment starting at position {p}: not a multiple of the checkpoint interval {} and the history tile {}",
            layout.checkpoint_interval, layout.h_tile
        )));
    }
    let at_prev = space.leaves_of_position(ctx, p - 1);
    // Fixed instances: the checkpoint tiles at position p − 1, concatenated in tile order.
    for k in owned_fixed {
        let inst = &space.fixed_instances()[*k];
        let mut values: Vec<i128> = Vec::new();
        for leaf in &at_prev {
            if let PalwTirLeafKindV1::State { instance, .. } = leaf.kind
                && instance as usize == *k
            {
                let pre = req.inputs.leaf_preimage(leaf.index).ok_or(TirCellVerdictV1::Unavailable { leaf: leaf.index })?;
                values.extend(values_of_lanes(inst.dtype, &pre.values_le));
            }
        }
        let s = &space.program.states[inst.state as usize];
        let StateKind::Fixed { .. } = s.kind else { continue };
        let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
        st.fixed.insert((inst.state, inst.layer), Tensor { dtype: s.dtype, shape, data: values });
    }
    // History instances: the rows of positions [p − need, p), from the tiles at positions p − 1, p − 1 − h_tile, ….
    for k in owned_hist {
        let inst = &space.hist_instances()[*k];
        let s = &space.program.states[inst.state as usize];
        let StateKind::Hist { window } = s.kind else { continue };
        let need = (p as usize).min(window as usize - 1);
        let kept = need.max(layout.h_tile as usize);
        let tiles_needed = kept.div_ceil(layout.h_tile as usize).min((p / layout.h_tile) as usize);
        let row_elems = inst.elements as usize;
        let mut rows_old_first: Vec<Vec<i128>> = Vec::new();
        let mut tail_rows: Vec<Vec<i32>> = Vec::new();
        for t in (0..tiles_needed).rev() {
            let last = p - 1 - t as u32 * layout.h_tile;
            let at = space.leaves_of_position(ctx, last);
            // The tile's sub-row leaves of this instance, in order.
            let mut parts: Vec<(u64, u32, Vec<i128>)> = Vec::new();
            for leaf in &at {
                if let PalwTirLeafKindV1::HistTile { instance, first_lane, row_lanes, .. } = leaf.kind
                    && instance as usize == *k
                {
                    let pre = req.inputs.leaf_preimage(leaf.index).ok_or(TirCellVerdictV1::Unavailable { leaf: leaf.index })?;
                    parts.push((first_lane, row_lanes, values_of_lanes(inst.dtype, &pre.values_le)));
                }
            }
            for r in 0..layout.h_tile as usize {
                let mut row = vec![0i128; row_elems];
                for (first, lanes, vals) in &parts {
                    let lanes = *lanes as usize;
                    row[*first as usize..*first as usize + lanes].copy_from_slice(&vals[r * lanes..(r + 1) * lanes]);
                }
                tail_rows.push(row.iter().map(|v| *v as i32).collect());
                rows_old_first.push(row);
            }
        }
        let keep_from = rows_old_first.len().saturating_sub(need);
        let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
        let rows: std::collections::VecDeque<Tensor> =
            rows_old_first[keep_from..].iter().map(|r| Tensor { dtype: s.dtype, shape: shape.clone(), data: r.clone() }).collect();
        st.hist.insert((inst.state, inst.layer), rows);
        tails.push((inst.state, inst.layer, tail_rows));
    }
    Ok((st, tails))
}

// =================================================================================================
// The sources of a cell's committed inputs
// =================================================================================================

/// **A dense capture as a cell's inputs**: every leaf preimage in step order, hashed once, the root recomputed against the
/// binding's step root before anything is read (so a lying capture is refused whole, never believed). The drill's source and the
/// source of a producer verifying its own cells; a real-size class's seat uses [`TirRunInputsV1`] instead.
pub struct TirCaptureInputsV1<'c> {
    leaves: &'c [PalwStepTileLeafV1],
    hashes: Vec<Hash64>,
}

impl<'c> TirCaptureInputsV1<'c> {
    pub fn new(
        leaves: &'c [PalwStepTileLeafV1],
        ctx: &PalwJobContextV2,
        class_id: &Hash64,
        step_merkle_root: &Hash64,
        cap: u64,
    ) -> Result<Self, String> {
        let ctx_hash = ctx.context_hash();
        let hashes: Vec<Hash64> = leaves.iter().map(|l| step_tile_leaf_hash_v1(&ctx_hash, class_id, l)).collect();
        let root = kaspa_consensus_core::palw_step_leg::step_merkle_root_capped_v1(&hashes, cap).map_err(|e| e.to_string())?;
        if root != *step_merkle_root {
            return Err("the capture's leaves do not reach the claim's step root".to_string());
        }
        Ok(Self { leaves, hashes })
    }
}

impl TirCellInputsV1 for TirCaptureInputsV1<'_> {
    fn leaf_hash(&self, index: u64) -> Option<Hash64> {
        self.hashes.get(usize::try_from(index).ok()?).copied()
    }
    fn leaf_preimage(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        self.leaves.get(usize::try_from(index).ok()?).cloned()
    }
}

/// **A cell's inputs as runs** (`TirStepRun`'s form, RFC §3): range openings of the contiguous runs of leaves the cell reads,
/// each walked to the claim's step root, and the preimages of the leaves the cell is handed. A leaf outside every run is
/// unavailable. What a served annex and an on-chain `TirStepRun` answer both build.
#[derive(Clone, Debug, Default)]
pub struct TirRunInputsV1 {
    hashes: BTreeMap<u64, Hash64>,
    preimages: BTreeMap<u64, PalwStepTileLeafV1>,
}

impl TirRunInputsV1 {
    /// Add one run: its range opening is walked to `step_root` over `leaf_count` leaves (refused otherwise); `preimages` are the
    /// run's leaves from its first, each checked against its opened hash (a run may carry preimages for a prefix of itself).
    pub fn add_run(
        &mut self,
        leaf_count: u64,
        step_root: &Hash64,
        opening: &PalwStepRangeOpeningV1,
        preimages: &[PalwStepTileLeafV1],
        ctx: &PalwJobContextV2,
        class_id: &Hash64,
        cap: u64,
    ) -> Result<(), String> {
        let root = step_range_opening_root_capped_v1(leaf_count, opening, cap).map_err(|e| e.to_string())?;
        if root != *step_root {
            return Err("the run does not reach the claim's step root".to_string());
        }
        if preimages.len() > opening.leaf_hashes.len() {
            return Err("more preimages than leaves in the run".to_string());
        }
        let ctx_hash = ctx.context_hash();
        for (i, pre) in preimages.iter().enumerate() {
            if step_tile_leaf_hash_v1(&ctx_hash, class_id, pre) != opening.leaf_hashes[i] {
                return Err(format!("preimage {i} of the run is not its opened leaf"));
            }
            self.preimages.insert(opening.first_leaf_index + i as u64, pre.clone());
        }
        for (i, h) in opening.leaf_hashes.iter().enumerate() {
            self.hashes.insert(opening.first_leaf_index + i as u64, *h);
        }
        Ok(())
    }

    pub fn leaves(&self) -> usize {
        self.hashes.len()
    }
}

impl TirCellInputsV1 for TirRunInputsV1 {
    fn leaf_hash(&self, index: u64) -> Option<Hash64> {
        self.hashes.get(&index).copied()
    }
    fn leaf_preimage(&self, index: u64) -> Option<PalwStepTileLeafV1> {
        self.preimages.get(&index).cloned()
    }
}

/// **The runs a cell reads, for a producer to serve or a seat to demand**: for each position of the cell, the contiguous leaf
/// ranges `(first, count, carry_in_preimages)` — one run over `[the previous occurrence's carry-out tiles ‖ the cell's own
/// leaves]` where they are adjacent, plus the checkpoint and history runs of the instances the cell writes. A seat builds its
/// `TirStepRun` demands from these; a producer builds the answers.
pub fn cell_runs_v1(
    space: &PalwTirStepSpaceV1,
    ctx: &PalwJobContextV2,
    cell: &TirCellV1,
) -> Result<Vec<(u64, u32, usize)>, String> {
    let job = space.job_shape(ctx).map_err(|e| e.to_string())?;
    let (owned_fixed, owned_hist) = written_instances(space, &cell.occ);
    let n_occ = space.occurrences().len();
    let mut runs: Vec<(u64, u32, usize)> = Vec::new();
    for a in cell.positions.start..cell.positions.end.min(job.positions) {
        let listed = space.leaves_of_position(ctx, a);
        let mut wanted: Vec<(u64, bool)> = Vec::new(); // (index, preimage needed)
        let carry_nodes: Vec<u16> = if cell.occ.start > 0 {
            let (block, _) = space.occurrences()[cell.occ.start - 1];
            space.program.blocks[block as usize].carry_out.clone()
        } else {
            Vec::new()
        };
        for leaf in &listed {
            match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, .. } => {
                    let o = occurrence as usize;
                    if cell.occ.contains(&o) && !(o + 1 == n_occ && !job.runs_post(a)) {
                        wanted.push((leaf.index, false));
                    } else if cell.occ.start > 0 && o + 1 == cell.occ.start && carry_nodes.contains(&node) {
                        wanted.push((leaf.index, true));
                    }
                }
                PalwTirLeafKindV1::State { instance, .. } if owned_fixed.contains(&(instance as usize)) => wanted.push((leaf.index, false)),
                PalwTirLeafKindV1::HistTile { instance, .. } if owned_hist.contains(&(instance as usize)) => wanted.push((leaf.index, false)),
                _ => {}
            }
        }
        wanted.sort_unstable();
        let mut i = 0;
        while i < wanted.len() {
            let mut j = i + 1;
            while j < wanted.len() && wanted[j].0 == wanted[j - 1].0 + 1 {
                j += 1;
            }
            // A run's preimages are a PREFIX of it: the carry-in tiles lead it (they sit just before the cell's own leaves).
            let pre = wanted[i..j].iter().take_while(|(_, needs)| *needs).count();
            runs.push((wanted[i].0, (j - i) as u32, pre));
            i = j;
        }
    }
    // Restoring a segment: the checkpoint and history tiles of position p − 1 (and the tiles before it a window needs) are handed
    // whole, preimages included.
    let p = cell.positions.start;
    if p > 0 {
        let layout = &space.layout;
        let mut handed: Vec<u64> = Vec::new();
        for leaf in space.leaves_of_position(ctx, p - 1) {
            if let PalwTirLeafKindV1::State { instance, .. } = leaf.kind
                && owned_fixed.contains(&(instance as usize))
            {
                handed.push(leaf.index);
            }
        }
        for k in &owned_hist {
            let inst = &space.hist_instances()[*k];
            let StateKind::Hist { window } = space.program.states[inst.state as usize].kind else { continue };
            let need = (p as usize).min(window as usize - 1).max(layout.h_tile as usize);
            let tiles = need.div_ceil(layout.h_tile as usize).min((p / layout.h_tile) as usize);
            for t in 0..tiles {
                let last = p - 1 - t as u32 * layout.h_tile;
                for leaf in space.leaves_of_position(ctx, last) {
                    if let PalwTirLeafKindV1::HistTile { instance, .. } = leaf.kind
                        && instance as usize == *k
                    {
                        handed.push(leaf.index);
                    }
                }
            }
        }
        handed.sort_unstable();
        handed.dedup();
        let mut i = 0;
        while i < handed.len() {
            let mut j = i + 1;
            while j < handed.len() && handed[j] == handed[j - 1] + 1 {
                j += 1;
            }
            runs.push((handed[i], (j - i) as u32, j - i));
            i = j;
        }
    }
    Ok(runs)
}

// ---------------------------------------------------------------------------------------------
// A seat's duty: the cells of a plan over a class backend (RFC-0006 §4)
// ---------------------------------------------------------------------------------------------

use super::backend::{TirBackendV1, TirCaptureV1};
use kaspa_consensus_core::palw_tir_shard_v1 as shard_rules;
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;

/// The layer partition of a class under `s_l` shards, exactly as the fold derives it (`palw_tir_shard_weights_v1` over the class's
/// `max_context`, `palw_tir_shard_partition_v1`), and the segment alignment `lcm(C, h_tile)`.
pub fn tir_shard_geometry_v1(tir: &TirBackendV1, s_l: u16) -> Result<(Vec<Range<usize>>, u32), String> {
    let space = tir.space();
    let weights = shard_rules::palw_tir_shard_weights_v1(&space.program, tir.class().layout.max_context);
    let parts = shard_rules::palw_tir_shard_partition_v1(&weights, s_l)
        .ok_or_else(|| format!("a plan of {s_l} shards over {} layers", weights.layer_bytes.len()))?;
    let align = shard_rules::palw_tir_shard_segment_align_v1(space.layout.checkpoint_interval, space.layout.h_tile)
        .ok_or("the class's checkpoint interval and history tile have no alignment")?;
    Ok((parts, align))
}

/// **The bytes a shard holds** (`PalwTirShardWeightsV1::of_range` over the shard's layers: its per-layer params and state, the
/// globals its layers read, `pre`'s and `post`'s extras where it holds them) — what the node's ledger reserves for the shard's
/// cells, on the host or on a device.
pub fn tir_shard_weight_bytes_v1(tir: &TirBackendV1, shard: u16, s_l: u16) -> Result<u128, String> {
    let weights = shard_rules::palw_tir_shard_weights_v1(&tir.space().program, tir.class().layout.max_context);
    let (parts, _) = tir_shard_geometry_v1(tir, s_l)?;
    let part = parts.get(usize::from(shard)).ok_or_else(|| format!("shard {shard} of {s_l}"))?;
    Ok(weights.of_range(part.start, part.end))
}

/// **The cells of one seat's duty**: shard `shard` of an `s_l`-shard plan, over the segments of `mask` of an `s_p`-segment cut of a
/// job of `positions` positions. Empty segments (a job shorter than the alignment) are not cells.
pub fn tir_shard_cells_v1(
    tir: &TirBackendV1,
    positions: u32,
    shard: u16,
    s_l: u16,
    s_p: u16,
    mask: PalwSegmentMaskV2,
) -> Result<Vec<TirCellV1>, String> {
    let (parts, align) = tir_shard_geometry_v1(tir, s_l)?;
    let layers = tir.space().program.schedule.layers.len();
    let part = parts.get(usize::from(shard)).ok_or_else(|| format!("shard {shard} of {s_l}"))?;
    let occ = shard_rules::palw_tir_shard_occurrences_v1(part, layers);
    let mut cells = Vec::new();
    for j in 0..s_p {
        if !mask.covers(j) {
            continue;
        }
        let seg = shard_rules::palw_tir_shard_segment_positions_v1(positions, align, s_p, j).ok_or("a segment out of range")?;
        if seg.is_empty() {
            continue;
        }
        cells.push(TirCellV1 { shard, occ: occ.clone(), positions: seg });
    }
    Ok(cells)
}

/// **Verify a seat's cells over a dense capture** (the claim's material, already matched to the claim's roots by
/// `verify_material`): the capture's leaves are authenticated against its own step root, then each cell is run on `backend`
/// (a refusal falls back to the CPU). The first cell that is not `Verified` is the answer; otherwise the sum.
pub fn tir_verify_capture_cells_v1(
    tir: &TirBackendV1,
    material: &[u8],
    cells: &[TirCellV1],
    backend: &mut dyn KernelBackendV1,
) -> Result<TirCellVerdictV1, String> {
    let capture = tir.decode_capture(material)?;
    if !capture.is_dense() {
        return Err("a fold carries no preimages: a cell needs the committed rows".into());
    }
    let ctx = &capture.binding.job_context;
    let class_id = tir.class_id();
    let inputs = TirCaptureInputsV1::new(&capture.leaves, ctx, &class_id, &capture.binding.step_merkle_root, u64::MAX)?;
    let tokens = tokens_of_capture_v1(&capture);
    let params = tir.artifact().params();
    let mut total = (0u64, 0u32);
    for cell in cells {
        let req = TirCellRequestV1 {
            space: tir.space(),
            plan: tir.artifact().plan(),
            params,
            class_id,
            ctx,
            cell,
            tokens: &tokens,
            inputs: &inputs,
            fused: tir.fused_kernels(),
        };
        let verdict = match backend.verify_cell(&req) {
            Ok(v) => v,
            Err(_) => CpuKernelBackendV1.verify_cell(&req).expect("the CPU backend runs every cell"),
        };
        match verdict {
            TirCellVerdictV1::Verified { leaves, positions } => {
                total.0 += leaves;
                total.1 += positions;
            }
            other => return Ok(other),
        }
    }
    Ok(TirCellVerdictV1::Verified { leaves: total.0, positions: total.1 })
}

/// The committed token of every position: the prompt's ids, then the generated ids (the last generated id feeds no position).
pub fn tokens_of_capture_v1(capture: &TirCaptureV1) -> Vec<u32> {
    let mut t = capture.prompt.clone();
    t.extend(capture.generated.iter().copied());
    t
}

// ---------------------------------------------------------------------------------------------
// The device plug-in (gpu-integer-backend.md §8)
// ---------------------------------------------------------------------------------------------

/// **A registered device as a [`KernelBackendV1`]**: it refuses a cell of a segment after the first (a device keeps no committed
/// boundary state to resume from) and any cell its stepper cannot carry out, and otherwise runs the node's one verifier over the
/// device's stepper — so its verdict is the CPU's by construction.
pub struct DeviceKernelBackendV1(pub std::sync::Arc<dyn TirDeviceV1>);

impl KernelBackendV1 for DeviceKernelBackendV1 {
    fn name(&self) -> String {
        self.0.name()
    }
    fn verify_cell(&mut self, req: &TirCellRequestV1<'_>) -> Result<TirCellVerdictV1, KernelRefusedV1> {
        if req.cell.positions.start != 0 {
            return Err(KernelRefusedV1("a cell of a segment after the first: the device does not resume".to_string()));
        }
        let mut stepper = self.0.cell_stepper(req.plan, req.params, req.cell.occ.clone()).map_err(KernelRefusedV1)?;
        match verify_cell_stepping_v1(req, stepper.as_mut()) {
            // The device could not carry the cell out (a history short of rows, a step it refused): the CPU runs it.
            TirCellVerdictV1::Refused(why) => Err(KernelRefusedV1(why)),
            verdict => Ok(verdict),
        }
    }
    fn device_capacity_bytes(&self) -> Option<u64> {
        self.0.capacity_bytes()
    }
}

static DEVICE_V1: std::sync::OnceLock<std::sync::Arc<dyn TirDeviceV1>> = std::sync::OnceLock::new();

/// Register the device. Once per process; a second registration is refused (`false`).
pub fn register_device_v1(device: std::sync::Arc<dyn TirDeviceV1>) -> bool {
    DEVICE_V1.set(device).is_ok()
}

/// **The backend a node runs its cells on**: the registered device when `want_device` and one is registered, otherwise the CPU. The
/// `bool` is whether the answer is a device — a node that asked for one and got the CPU says so.
pub fn tir_kernel_backend_v1(want_device: bool) -> (Box<dyn KernelBackendV1 + Send>, bool) {
    if want_device && let Some(device) = DEVICE_V1.get() {
        return (Box::new(DeviceKernelBackendV1(std::sync::Arc::clone(device))), true);
    }
    (Box::new(CpuKernelBackendV1), false)
}

/// Whether a device is registered in this process.
pub fn tir_kernel_backend_registered_v1() -> bool {
    DEVICE_V1.get().is_some()
}

