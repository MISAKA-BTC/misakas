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
use kaspa_consensus_core::palw_tir_step_v1::{PALW_TIR_STEP_LEAF_VERSION_V1, PalwTirStepSpaceV1, palw_tir_execution_root_v1};
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

/// Runs jobs of one IR class over one artifact's params.
pub struct TirClassRunnerV1<'a> {
    pub space: &'a PalwTirStepSpaceV1,
    pub plan: &'a TirPlan,
    pub params: &'a TirParams<'a>,
    pub class_id: Hash64,
    /// `tile_len` per `(block, node)`; 0 for a node that is not committed.
    tiles: Vec<Vec<u32>>,
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
        Ok(TirClassRunnerV1 { space, plan, params, class_id, tiles })
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
        let mut exec = TirExecutor::new(self.plan, self.params).map_err(|e| e.to_string())?;
        exec.set_hist_tail(space.layout.h_tile as usize);
        let mut emit = Emitter {
            ctx_hash: ctx.context_hash(),
            class_id: self.class_id,
            hashes: Vec::with_capacity(total.min(1 << 24) as usize),
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
        let mut generated: Vec<u32> = Vec::with_capacity(ctx.exact_decode_tokens as usize);
        let mut rows: Vec<Vec<i32>> = Vec::with_capacity(ctx.exact_decode_tokens as usize);
        for a in 0..job.positions {
            let first = emit.hashes.len() as u64;
            let token = if a < job.prefill { prompt[a as usize] } else { generated[(a - job.prefill) as usize] };
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
            if (a + 1) % space.layout.checkpoint_interval == 0 {
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
                let h_tile = space.layout.h_tile as usize;
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
        }
        if emit.hashes.len() as u64 != total {
            return Err(format!("{} leaves produced, the step space counts {total}", emit.hashes.len()));
        }
        let step_merkle_root = step_merkle_root_capped_v1(&emit.hashes, cap).map_err(|e| e.to_string())?;
        let trace_root = if tiled {
            tiled_logits_trace_root_v1(ctx, &rows, &generated).ok_or("the logits rows build no trace")?
        } else {
            base0_logits_trace_root_v1(ctx, &rows, &generated)
        };
        let execution_root = palw_tir_execution_root_v1(&emit.ctx_hash, &trace_root, &self.class_id, total, &step_merkle_root);
        let output_root = palw_attempt_output_root_v1(ctx, &generated);
        Ok(TirJobRunV1 {
            leaf_count: total,
            step_merkle_root,
            logits_rows: rows,
            generated,
            trace_root,
            execution_root,
            output_root,
            leaf_hashes: emit.hashes,
        })
    }
}

/// The leaf under construction and everything its hash binds.
struct Emitter {
    ctx_hash: Hash64,
    class_id: Hash64,
    hashes: Vec<Hash64>,
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
        let index = self.hashes.len() as u64;
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
