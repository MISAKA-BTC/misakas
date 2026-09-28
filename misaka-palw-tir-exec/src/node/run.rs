//! **One job of an IR class, committed** — the node's side of Phase F step F4
//! (`kaspa_consensus_core::palw_tir_step_v1`, design §2.5–§2.6).
//!
//! A job of `P` prompt positions and `D` generated tokens runs the program at `a = 0 … P + D − 2`:
//! position `a < P` reads prompt token `a`; from `a = P − 1` on, `post` runs and its logits row
//! selects the next token (`base0_decode_token_select_v1`: argmax, ties to the lowest index), which
//! position `a + 1` reads. Every leaf of the step space is produced in the ONE order F4 enumerates —
//! per position: the committed nodes' tiles in occurrence and node order (`post` only where its
//! logits are consumed), then every `Fixed` instance's checkpoint tiles when `(a + 1) % C == 0`,
//! then every history's last `h_tile` rows, one leaf per sub-row, when `(a + 1) % h_tile == 0` —
//! hashed as it is produced (nothing is retained but the leaf hashes), and the roots a claim commits
//! are formed from them:
//!
//! * the step leg: `step_merkle_root_capped_v1` over the leaf hashes (each the legacy step tile
//!   leaf, hashed with the IR class id in the profile's place);
//! * the logits trace, under the program's `logits_scheme_id` (flat or tiled, the legacy schemes);
//! * the IR execution root (`palw_tir_execution_root_v1`) and the attempt output root.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1;
use kaspa_consensus_core::palw_step::PalwStepCoordinateV1;
use kaspa_consensus_core::palw_step_leg::{PalwStepTileLeafV1, step_merkle_root_capped_v1, step_tile_leaf_hash_v1};
use kaspa_consensus_core::palw_step_refute::{
    base0_decode_token_select_v1, base0_logits_trace_root_v1, flat_logits_scheme_id_v1, tiled_logits_scheme_id_v1,
    tiled_logits_trace_root_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::{
    PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirJobShapeV1, PalwTirStepSpaceV1, palw_tir_execution_root_v1,
};
use kaspa_consensus_core::palw_v2::PalwJobContextV2;
use misaka_palw_tir::DType;

use crate::elem::{Elem, Slice};
use crate::exec::{NodeValue, StepSink, TirExecutor};
use crate::params::TirParams;
use crate::plan::TirPlan;
use crate::with_slice;

/// One produced leaf: its index in the job's step space, the position it belongs to, its preimage
/// and its hash.
pub struct TirLeafOutV1<'l> {
    pub index: u64,
    pub position: u32,
    pub preimage: &'l PalwStepTileLeafV1,
    pub hash: Hash64,
}

/// What a job commits.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirJobRunV1 {
    pub leaf_count: u64,
    pub step_merkle_root: Hash64,
    /// The logits rows that selected the generated tokens (positions `P − 1 … P + D − 2`).
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
    pub trace_root: Hash64,
    pub execution_root: Hash64,
    /// The attempt lane's output root (`CoreV1`, an IR class's rule).
    pub output_root: Hash64,
    /// Every leaf's hash, in order (the bisection's prefix states and the openings read them).
    pub leaf_hashes: Vec<Hash64>,
}

/// **A resume point**: everything a run needs to continue after position `position` exactly as
/// the uninterrupted run would — what the step leaves of a checkpoint position open (design §2.6):
/// every `Fixed` instance's value after the position (its state leaves), every history's most
/// recent rows (its history tiles and the rows committed since), and the tokens generated so far.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirResumePointV1 {
    /// The last position the point covers (a checkpoint position: `(position + 1) % C == 0`).
    pub position: u32,
    /// Per `Fixed` instance, in the step space's order: its value after `position`.
    pub fixed: Vec<Vec<i128>>,
    /// Per history instance, in the step space's order: its last rows, oldest first — at least
    /// `max(min(position + 1, W − 1), (position + 1) % h_tile)` of them (the next window, and the
    /// part of the next history tile that precedes the resume).
    pub hist: Vec<Vec<Vec<i32>>>,
    /// The tokens selected at positions `P − 1 ..= position` (empty before `P − 1`).
    pub generated: Vec<u32>,
}

/// Runs jobs of one IR class over one artifact's params.
pub struct TirClassRunnerV1<'a> {
    pub space: &'a PalwTirStepSpaceV1,
    pub plan: &'a TirPlan,
    pub params: &'a TirParams<'a>,
    pub class_id: Hash64,
    /// `tile_len` per `(block, node)`; 0 for a node that is not committed.
    tiles: Vec<Vec<u32>>,
    /// Run the fused kernels (RFC-0002 §7, `TirExecutor::set_fused`): byte-identical, off by default.
    fused: bool,
}

/// The leaf lanes of values of `dtype` (PALW-TIR-5): `i8`/`i16`/`i32` as little-endian `i32`,
/// `idx` as little-endian `u32`.
fn lanes_le(data: Slice<'_>, from: usize, n: usize, out: &mut Vec<u8>) {
    out.clear();
    out.reserve(n * 4);
    with_slice!(data, v => lanes_typed(&v[from..from + n], out));
}

fn lanes_typed<T: Elem>(v: &[T], out: &mut Vec<u8>) {
    // A committed value is i8, i16, i32 or idx (NF-17); each is exactly one 4-byte lane.
    if T::DTYPE == DType::Idx {
        for x in v {
            out.extend_from_slice(&(x.to_i64() as u32).to_le_bytes());
        }
    } else {
        for x in v {
            out.extend_from_slice(&(x.to_i64() as i32).to_le_bytes());
        }
    }
}

impl<'a> TirClassRunnerV1<'a> {
    pub fn new(space: &'a PalwTirStepSpaceV1, plan: &'a TirPlan, params: &'a TirParams<'a>, class_id: Hash64) -> Result<Self, String> {
        if space.program != plan.program {
            return Err("the step space and the plan are of different programs".into());
        }
        let mut commit_tiles = space.layout.commit_tiles.iter();
        let mut tiles = Vec::with_capacity(plan.program.blocks.len());
        for b in &plan.program.blocks {
            let mut row = vec![0u32; b.nodes.len()];
            for (ni, n) in b.nodes.iter().enumerate() {
                if n.commit {
                    row[ni] = *commit_tiles.next().ok_or("the layout has fewer commit tiles than committed nodes")?;
                }
            }
            tiles.push(row);
        }
        Ok(TirClassRunnerV1 { space, plan, params, class_id, tiles, fused: false })
    }

    /// **With the fused kernels on (or off)** — every executor this runner drives runs the regions
    /// its plan matched fused (`TirExecutor::set_fused`); byte-identical either way (tir-lower's
    /// `fused_gate`), and a step whose sink reads every node runs generic throughout.
    pub fn with_fused(mut self, on: bool) -> Self {
        self.fused = on;
        self
    }

    /// **Run one job** and commit to it. `prompt` is the job's `P` prompt ids; `cap` the class's
    /// step ladder. `on_leaf` sees every leaf as it is produced (a capture, a seat's stream).
    /// With `verify`, every position's leaves are checked against F4's own enumeration
    /// (`leaves_of_position`) as they are produced — the producer and the step space cannot drift.
    pub fn run(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
        verify: bool,
        on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>),
    ) -> Result<TirJobRunV1, String> {
        let d = self.drive(ctx, prompt, cap, verify, None, None, None, on_leaf)?;
        self.commit(ctx, cap, d)
    }

    /// [`Self::run`], also recording a [`TirResumePointV1`] after every checkpoint position.
    pub fn run_recording(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
        on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>),
    ) -> Result<(TirJobRunV1, Vec<TirResumePointV1>), String> {
        let mut points = Vec::new();
        let d = self.drive(ctx, prompt, cap, false, None, None, Some(&mut points), on_leaf)?;
        Ok((self.commit(ctx, cap, d)?, points))
    }

    /// A whole job's roots over what its drive produced.
    fn commit(&self, ctx: &PalwJobContextV2, cap: u64, d: Driven) -> Result<TirJobRunV1, String> {
        debug_assert_eq!(d.first_index, 0);
        let total = d.hashes.len() as u64;
        let step_merkle_root = step_merkle_root_capped_v1(&d.hashes, cap).map_err(|e| e.to_string())?;
        let trace_root = if d.tiled {
            tiled_logits_trace_root_v1(ctx, &d.rows, &d.generated).ok_or("the logits rows build no trace")?
        } else {
            base0_logits_trace_root_v1(ctx, &d.rows, &d.generated)
        };
        let execution_root = palw_tir_execution_root_v1(&ctx.context_hash(), &trace_root, &self.class_id, total, &step_merkle_root);
        let output_root = palw_attempt_output_root_v1(ctx, &d.generated);
        Ok(TirJobRunV1 {
            leaf_count: total,
            step_merkle_root,
            logits_rows: d.rows,
            generated: d.generated,
            trace_root,
            execution_root,
            output_root,
            leaf_hashes: d.hashes,
        })
    }

    /// **Resume after a checkpoint**: positions `point.position + 1 ..` of the job, from the
    /// point's state — the same leaves, at the same indices, as the uninterrupted run
    /// (design §2.6: a seat resumes from state leaves and history tiles, not from genesis).
    pub fn resume(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
        point: &TirResumePointV1,
        on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>),
    ) -> Result<TirRangeRunV1, String> {
        self.replay(ctx, prompt, cap, Some(point), None, on_leaf)
    }

    /// **Replay part of a job**: from `from` (the job's start when `None`) through position
    /// `until − 1` (the job's end when `None`) — the leaves, at their indices, of the uninterrupted
    /// run over those positions. What an evidence builder re-derives a retained job's leaves with.
    pub fn replay(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
        from: Option<&TirResumePointV1>,
        until: Option<u32>,
        on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>),
    ) -> Result<TirRangeRunV1, String> {
        let d = self.drive(ctx, prompt, cap, false, from, until, None, on_leaf)?;
        Ok(TirRangeRunV1 { first_index: d.first_index, leaf_hashes: d.hashes, logits_rows: d.rows, generated: d.generated })
    }

    #[allow(clippy::too_many_arguments)]
    fn drive(
        &self,
        ctx: &PalwJobContextV2,
        prompt: &[u32],
        cap: u64,
        verify: bool,
        start: Option<&TirResumePointV1>,
        until: Option<u32>,
        mut record: Option<&mut Vec<TirResumePointV1>>,
        on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>),
    ) -> Result<Driven, String> {
        let space = self.space;
        if ctx.shape_profile_id != self.class_id {
            return Err("the job context names another class".into());
        }
        let job = space.job_shape(ctx).map_err(|e| e.to_string())?;
        if prompt.len() != job.prefill as usize {
            return Err(format!("{} prompt ids for a job of {} prompt positions", prompt.len(), job.prefill));
        }
        let total = space.leaf_count_capped(ctx, cap).map_err(|e| e.to_string())?;
        let scheme = Hash64::from_bytes(self.plan.program.logits_scheme_id);
        let tiled = if scheme == tiled_logits_scheme_id_v1() {
            true
        } else if scheme == flat_logits_scheme_id_v1() {
            false
        } else {
            return Err("the program names no logits scheme this build commits under".into());
        };
        let h_tile = space.layout.h_tile as usize;
        let mut exec = TirExecutor::new(self.plan, self.params).map_err(|e| e.to_string())?;
        exec.set_hist_tail(h_tile);
        if self.fused {
            exec.set_fused(true);
        }
        let (first_position, mut generated) = match start {
            None => (0u32, Vec::with_capacity(ctx.exact_decode_tokens as usize)),
            Some(point) => {
                self.restore(&mut exec, point, &job)?;
                let selected = (0..=point.position).filter(|a| job.runs_post(*a)).count();
                if point.generated.len() != selected {
                    return Err(format!("a resume point with {} tokens after {selected} selections", point.generated.len()));
                }
                (point.position + 1, point.generated.clone())
            }
        };
        let first_index = space.running_total(&job, first_position) as u64;
        let mut emit = Emitter {
            ctx_hash: ctx.context_hash(),
            class_id: self.class_id,
            hashes: Vec::with_capacity((total - first_index).min(1 << 24) as usize),
            first_index,
            leaf: PalwStepTileLeafV1 {
                version: PALW_TIR_STEP_LEAF_VERSION_V1,
                coord: PalwStepCoordinateV1 { call_index: 0, node_slot: 0, position: 0, tile_index: 0 },
                value_count: 0,
                values_le: Vec::new(),
            },
            position: 0,
            call: 0,
            call_position: 0,
            error: None,
            meta: verify.then(Vec::new),
        };
        let mut rows: Vec<Vec<i32>> = Vec::with_capacity(ctx.exact_decode_tokens as usize);
        let end = match until {
            None => job.positions,
            Some(u) if u >= first_position && u <= job.positions => u,
            Some(u) => return Err(format!("a replay to position {u} from {first_position} of a job of {}", job.positions)),
        };
        for a in first_position..end {
            let first = emit.first_index + emit.hashes.len() as u64;
            let token = if a < job.prefill {
                prompt[a as usize]
            } else {
                *generated.get((a - job.prefill) as usize).ok_or("a resume point without the tokens its positions read")?
            };
            let (call, position) = job.call_position(a);
            (emit.position, emit.call, emit.call_position) = (a, call, position);
            let run_post = job.runs_post(a);
            {
                let mut sink = CommitSink { runner: self, emit: &mut emit, on_leaf };
                exec.step_opt(token, &mut sink, run_post).map_err(|e| format!("position {a}: {e}"))?;
            }
            if let Some(e) = emit.error.take() {
                return Err(e);
            }
            if run_post {
                let (_, logits) = exec.logits();
                if logits.dtype() == DType::Idx {
                    return Err("an idx logits row has no place in the logits schemes".into());
                }
                let row: Vec<i32> = logits.to_i128s().into_iter().map(|v| v as i32).collect();
                if row.is_empty() {
                    return Err("an empty logits row selects no token".into());
                }
                generated.push(base0_decode_token_select_v1(&row) as u32);
                rows.push(row);
            }
            // Fixed-state checkpoints: every instance after this position's write.
            let checkpoint = (a + 1) % space.layout.checkpoint_interval == 0;
            if checkpoint {
                for (k, inst) in space.fixed_instances().iter().enumerate() {
                    let v = exec
                        .fixed_value(inst.state, inst.layer)
                        .ok_or_else(|| format!("no Fixed instance {:?}", (inst.state, inst.layer)))?;
                    let (n, t) = (inst.elements as usize, inst.tile_lanes as usize);
                    for (tile, from) in (0..n).step_by(t).enumerate() {
                        let count = t.min(n - from);
                        lanes_le(v, from, count, &mut emit.leaf.values_le);
                        emit.push(space.program_slots() + k as u32, tile as u32, count as u32, on_leaf);
                    }
                }
            }
            // History tiles: the last h_tile rows of every history, one leaf per sub-row.
            if (a + 1) % space.layout.h_tile == 0 {
                let base = space.program_slots() + space.fixed_instances().len() as u32;
                for (k, inst) in space.hist_instances().iter().enumerate() {
                    let tail = exec
                        .hist_tail(inst.state, inst.layer)
                        .ok_or_else(|| format!("no history instance {:?}", (inst.state, inst.layer)))?;
                    if tail.len() < h_tile {
                        return Err(format!(
                            "history {:?}: {} rows kept for a tile of {h_tile}",
                            (inst.state, inst.layer),
                            tail.len()
                        ));
                    }
                    let (n, t) = (inst.elements as usize, inst.tile_lanes as usize);
                    for (tile, from) in (0..n).step_by(t).enumerate() {
                        let lanes = t.min(n - from);
                        let values = &mut emit.leaf.values_le;
                        values.clear();
                        for row in tail.iter().skip(tail.len() - h_tile) {
                            for x in &row[from..from + lanes] {
                                values.extend_from_slice(&x.to_le_bytes());
                            }
                        }
                        emit.push(base + k as u32, tile as u32, (lanes * h_tile) as u32, on_leaf);
                    }
                }
            }
            if let Some(meta) = emit.meta.as_mut() {
                check_position(space, ctx, a, first, meta)?;
                meta.clear();
            }
            if checkpoint && let Some(points) = record.as_mut() {
                points.push(self.capture(&exec, a, &generated)?);
            }
        }
        if end == job.positions && emit.first_index + emit.hashes.len() as u64 != total {
            return Err(format!("{} leaves produced, the step space counts {total}", emit.first_index + emit.hashes.len() as u64));
        }
        Ok(Driven { first_index, hashes: emit.hashes, rows, generated, tiled })
    }

    /// The resume point after position `a` of a running executor.
    fn capture(&self, exec: &TirExecutor<'_>, a: u32, generated: &[u32]) -> Result<TirResumePointV1, String> {
        let space = self.space;
        let fixed = space
            .fixed_instances()
            .iter()
            .map(|inst| exec.fixed_value(inst.state, inst.layer).map(|v| v.to_i128s()).ok_or("a Fixed instance"))
            .collect::<Result<Vec<_>, _>>()?;
        let state = exec.export_state();
        let mut hist = Vec::with_capacity(space.hist_instances().len());
        for inst in space.hist_instances() {
            // The window's rows and the kept tail are two suffixes of one append sequence: the
            // longer covers both the next window and the next tile's earlier rows.
            let window: Vec<Vec<i32>> = state
                .hist
                .get(&(inst.state, inst.layer))
                .map(|rows| rows.iter().map(|t| t.data.iter().map(|v| *v as i32).collect()).collect())
                .unwrap_or_default();
            let tail: Vec<Vec<i32>> = exec.hist_tail(inst.state, inst.layer).map(|t| t.iter().cloned().collect()).unwrap_or_default();
            hist.push(if window.len() >= tail.len() { window } else { tail });
        }
        Ok(TirResumePointV1 { position: a, fixed, hist, generated: generated.to_vec() })
    }

    /// Put a fresh executor at the state after `point.position`.
    fn restore(&self, exec: &mut TirExecutor<'_>, point: &TirResumePointV1, job: &PalwTirJobShapeV1) -> Result<(), String> {
        let space = self.space;
        let p = &self.plan.program;
        let pos = point.position.checked_add(1).filter(|pos| *pos <= job.positions).ok_or("a resume point past the job's end")?;
        if point.fixed.len() != space.fixed_instances().len() || point.hist.len() != space.hist_instances().len() {
            return Err("a resume point of another step space".into());
        }
        let mut st = misaka_palw_tir::RunState { pos, ..Default::default() };
        for (inst, v) in space.fixed_instances().iter().zip(&point.fixed) {
            let s = &p.states[inst.state as usize];
            let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
            let t = misaka_palw_tir::Tensor::new(s.dtype, shape, v.clone()).map_err(|e| e.to_string())?;
            st.fixed.insert((inst.state, inst.layer), t);
        }
        for (inst, rows) in space.hist_instances().iter().zip(&point.hist) {
            let s = &p.states[inst.state as usize];
            let misaka_palw_tir::StateKind::Hist { window } = s.kind else { return Err("a history instance of a Fixed state".into()) };
            let need = (pos as usize).min(window as usize - 1);
            if rows.len() < need {
                return Err(format!("history {:?}: {} rows for a window that needs {need}", (inst.state, inst.layer), rows.len()));
            }
            let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
            // A row's lanes are its values' 4-byte lanes (an idx value is its u32).
            let lane = |v: &i32| if s.dtype == DType::Idx { *v as u32 as i128 } else { *v as i128 };
            let window_rows = rows[rows.len() - need..]
                .iter()
                .map(|r| misaka_palw_tir::Tensor::new(s.dtype, shape.clone(), r.iter().map(lane).collect()))
                .collect::<Result<std::collections::VecDeque<_>, _>>()
                .map_err(|e| e.to_string())?;
            st.hist.insert((inst.state, inst.layer), window_rows);
        }
        if pos == job.positions {
            // After the job's last position: nothing is left to run.
            return Ok(());
        }
        exec.import_state(&st).map_err(|e| e.to_string())?;
        for (inst, rows) in space.hist_instances().iter().zip(&point.hist) {
            exec.set_hist_tail_rows(inst.state, inst.layer, rows).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

/// What one drive produced.
struct Driven {
    first_index: u64,
    hashes: Vec<Hash64>,
    rows: Vec<Vec<i32>>,
    generated: Vec<u32>,
    tiled: bool,
}

/// A resumed run's part of the job: the leaves from `first_index` on, the logits rows it produced,
/// and every token generated so far (the point's and its own).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirRangeRunV1 {
    pub first_index: u64,
    pub leaf_hashes: Vec<Hash64>,
    pub logits_rows: Vec<Vec<i32>>,
    pub generated: Vec<u32>,
}

/// The leaf under construction and everything its hash binds.
struct Emitter {
    ctx_hash: Hash64,
    class_id: Hash64,
    hashes: Vec<Hash64>,
    /// The index of `hashes[0]` in the job's step space (0 unless resumed).
    first_index: u64,
    leaf: PalwStepTileLeafV1,
    position: u32,
    call: u32,
    call_position: u32,
    error: Option<String>,
    /// With verification on: every leaf's `(coordinate, value count)`, for the position's check.
    meta: Option<Vec<(PalwStepCoordinateV1, u32)>>,
}

impl Emitter {
    /// Hash the leaf whose lanes are in `self.leaf.values_le`.
    fn push(&mut self, node_slot: u32, tile: u32, value_count: u32, on_leaf: &mut dyn FnMut(&TirLeafOutV1<'_>)) {
        self.leaf.coord = PalwStepCoordinateV1 { call_index: self.call, node_slot, position: self.call_position, tile_index: tile };
        self.leaf.value_count = value_count;
        if let Some(m) = self.meta.as_mut() {
            m.push((self.leaf.coord, value_count));
        }
        let hash = step_tile_leaf_hash_v1(&self.ctx_hash, &self.class_id, &self.leaf);
        let index = self.first_index + self.hashes.len() as u64;
        self.hashes.push(hash);
        on_leaf(&TirLeafOutV1 { index, position: self.position, preimage: &self.leaf, hash });
    }
}

/// Receives the step's committed values in slot order and emits their tiles.
struct CommitSink<'r, 'e, 'f> {
    runner: &'r TirClassRunnerV1<'r>,
    emit: &'e mut Emitter,
    on_leaf: &'f mut dyn FnMut(&TirLeafOutV1<'_>),
}

impl StepSink for CommitSink<'_, '_, '_> {
    fn node(&mut self, v: &NodeValue<'_>) {
        if !v.commit {
            return;
        }
        let t = self.runner.tiles[v.block as usize][v.node as usize] as usize;
        if t == 0 {
            self.emit.error = Some(format!("block {} node {} is committed without a tile length", v.block, v.node));
            return;
        }
        let n = v.data.len();
        for (tile, from) in (0..n).step_by(t).enumerate() {
            let count = t.min(n - from);
            lanes_le(v.data, from, count, &mut self.emit.leaf.values_le);
            self.emit.push(v.slot, tile as u32, count as u32, self.on_leaf);
        }
    }
}

/// The leaves just produced for position `a` are F4's for that position: the same first index,
/// and leaf by leaf the same coordinate and value count (what the hashes bind besides the lanes).
fn check_position(
    space: &PalwTirStepSpaceV1,
    ctx: &PalwJobContextV2,
    a: u32,
    first: u64,
    meta: &[(PalwStepCoordinateV1, u32)],
) -> Result<(), String> {
    let listed = space.leaves_of_position(ctx, a);
    if listed.len() != meta.len() {
        return Err(format!("position {a}: {} leaves produced, the step space has {}", meta.len(), listed.len()));
    }
    for (k, (l, (coord, count))) in listed.iter().zip(meta).enumerate() {
        if l.index != first + k as u64 || l.coord != *coord || l.value_count != *count {
            return Err(format!(
                "position {a}, leaf {k}: produced {coord:?} × {count}, the step space has {:?} × {} at {}",
                l.coord, l.value_count, l.index
            ));
        }
    }
    Ok(())
}
