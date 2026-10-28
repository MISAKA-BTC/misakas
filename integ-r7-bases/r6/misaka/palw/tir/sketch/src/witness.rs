//! **What a producer serves for an algebraic check: the witness** (RFC-0007 Part II, §II.6).
//!
//! A TIR execution commits its commit points — carry-outs, the logits, every `TopK`, every
//! history row, and the further points a lowerer adds — as step leaves (spec 04b §10.1). Its
//! `MatMul` accumulators are never committed: they are `i64`/`i128` (PALW-TIR-5), and they are the
//! one thing a seat without the weights cannot recompute. The witness is exactly that missing part:
//! **the output of every `MatMul` the seat checks algebraically** (every weight product, and the
//! activation × activation products its policy serves), per position, plus the committed rows a
//! dense capture already serves (`TirCaptureV1`), so that the seat can name the first one it
//! disagrees with.
//!
//! The witness carries no commitment of its own and needs none for the seat: the seat accepts only
//! if every served value passes its check and the committed rows it recomputes from them equal the
//! claim's (here: [`tir_commit_root_v1`], standing in for the step-tree root). What a producer owes
//! is to SERVE it — a duty with the claim's licence as its only reward (RFC-0007 Part II, §II.7).
//!
//! Two producers build it and must agree byte for byte (`tests/soundness.rs`):
//! [`tir_witness_capture_v1`] runs the typed backend, as a node would;
//! [`tir_witness_produce_v1`] walks the reference evaluator node by node and lets a test change any
//! node's value as it is produced — a dishonest producer whose execution is consistent with its
//! lie everywhere downstream, which is the strongest one.

use blake2b_simd::Params;
use misaka_palw_tir::{Interpreter, ParamSource, Prim, Tensor, TirError, TirErrorKind, TirResult};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, TirParams, TirPlan};

use crate::analysis::{TirCheckPolicyV1, TirSketchAnalysisV1};
use crate::walk::{OccCtxV1, RunStateV1, decode_select, eval_node, occurrence_of, ref_value};

/// Key of [`tir_commit_root_v1`].
pub const TIR_SKETCH_COMMIT_ROOT_DOMAIN_V1: &[u8] = b"misaka-palw/tir/sketch/commit-root/v1";

/// A job: `prompt.len()` prompt positions, then `decode` tokens each selected by the logits of
/// the position before (positions `0 … P + decode − 2`, as the Phase F step space runs a job).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirSketchJobV1 {
    pub prompt: Vec<u32>,
    pub decode: u32,
}

impl TirSketchJobV1 {
    pub fn positions(&self) -> u32 {
        self.prompt.len() as u32 + self.decode.max(1) - 1
    }
}

/// One served `MatMul` output.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirWitnessValueV1 {
    pub occurrence: u16,
    pub node: u16,
    pub value: Tensor,
}

/// One committed row, at its node slot (spec 04b §3.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirCommitRowV1 {
    pub slot: u32,
    pub value: Tensor,
}

/// One position's witness.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirWitnessStepV1 {
    pub pos: u32,
    /// Every served `MatMul` output, in occurrence and node order.
    pub values: Vec<TirWitnessValueV1>,
    /// Every committed row, in slot order.
    pub commits: Vec<TirCommitRowV1>,
}

/// **A job's witness** (module note).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TirWitnessV1 {
    pub prompt_len: u32,
    /// The token each position read.
    pub tokens: Vec<u32>,
    /// The `decode` selected tokens.
    pub generated: Vec<u32>,
    pub steps: Vec<TirWitnessStepV1>,
    /// The committed rows' root (the claim's commitment, in this prototype).
    pub commit_root: [u8; 32],
}

impl TirWitnessV1 {
    /// `(elements, bytes)` of the served values at their declared width — what algebraic
    /// verification adds to a dense capture.
    pub fn served(&self) -> (u64, u64) {
        self.steps.iter().flat_map(|s| &s.values).fold((0, 0), |(e, b), v| {
            let n = v.value.data.len() as u64;
            (e + n, b + n * v.value.dtype.width() as u64)
        })
    }

    /// `(elements, bytes)` of the committed rows as 4-byte lanes — what a dense capture serves today.
    pub fn committed(&self) -> (u64, u64) {
        self.steps.iter().flat_map(|s| &s.commits).fold((0, 0), |(e, b), c| {
            let n = c.value.data.len() as u64;
            (e + n, b + 4 * n)
        })
    }
}

/// **The committed rows' root**: keyed BLAKE2b-256 over every position's rows as 4-byte lanes
/// (`i8`/`i16`/`i32` little-endian two's complement, `idx` unsigned) — the prototype's stand-in for
/// the step tree's root, which hashes the same lanes in tiles.
pub fn tir_commit_root_v1(steps: &[TirWitnessStepV1]) -> [u8; 32] {
    let mut h = Params::new().hash_length(32).key(TIR_SKETCH_COMMIT_ROOT_DOMAIN_V1).to_state();
    h.update(&(steps.len() as u32).to_le_bytes());
    for s in steps {
        h.update(&s.pos.to_le_bytes());
        h.update(&(s.commits.len() as u32).to_le_bytes());
        for c in &s.commits {
            h.update(&c.slot.to_le_bytes());
            h.update(&(c.value.data.len() as u32).to_le_bytes());
            for v in &c.value.data {
                h.update(&(*v as u32).to_le_bytes());
            }
        }
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(h.finalize().as_bytes());
    out
}

/// Where a tamper hook is called: one node of one occurrence of one position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TirTamperSiteV1 {
    pub pos: u32,
    pub occurrence: u16,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
}

/// The history length of `block` at position `pos`.
pub(crate) fn h_at(plan: &TirPlan, block: u8, pos: u32) -> usize {
    plan.blocks[block as usize].window.map(|w| (pos as usize + 1).min(w as usize)).unwrap_or(1)
}

/// **The reference producer**: every node by [`Interpreter::eval_cone`], with `tamper` called on
/// each value as it is produced (a no-op for an honest producer). The witness is the values the
/// analysis serves under `policy`, the committed rows, and the tokens the logits selected.
pub fn tir_witness_produce_v1(
    plan: &TirPlan,
    analysis: &TirSketchAnalysisV1,
    params: &dyn ParamSource,
    job: &TirSketchJobV1,
    policy: &TirCheckPolicyV1,
    tamper: &mut dyn FnMut(&TirTamperSiteV1, &mut Tensor),
) -> TirResult<TirWitnessV1> {
    let p = &plan.program;
    let interp = Interpreter::new(p)?;
    let prompt_len = job.prompt.len() as u32;
    if prompt_len == 0 {
        return Err(TirError::new(TirErrorKind::Malformed, "a job has at least one prompt token"));
    }
    let positions = job.positions();
    let mut tokens = job.prompt.clone();
    let mut generated = Vec::new();
    let mut run = RunStateV1::default();
    let mut steps = Vec::with_capacity(positions as usize);
    let post_occ = plan.occurrences.len() - 1;
    for pos in 0..positions {
        let token = tokens[pos as usize];
        let mut step = TirWitnessStepV1 { pos, values: Vec::new(), commits: Vec::new() };
        let (mut writes, mut appends) = (Vec::new(), Vec::new());
        let mut carry: Vec<Tensor> = Vec::new();
        for (occ, &(block, layer)) in plan.occurrences.iter().enumerate() {
            if occ == post_occ && pos + 1 < prompt_len {
                break;
            }
            let b = &p.blocks[block as usize];
            let h = h_at(plan, block, pos);
            let mut values: Vec<Option<Tensor>> = vec![None; b.nodes.len()];
            let ctx = OccCtxV1 { pos, token, block, layer, carry: &carry };
            let site = |node: u16| TirTamperSiteV1 { pos, occurrence: occ as u16, block, layer, node };
            for ni in 0..b.nodes.len() {
                let mut v = eval_node(&interp, params, &ctx, &run, &values, ni)?;
                tamper(&site(ni as u16), &mut v);
                values[ni] = Some(v);
            }
            let base = plan.slot_bases[occ];
            for (ni, node) in b.nodes.iter().enumerate() {
                let v = values[ni].as_ref().expect("every node was evaluated");
                if analysis.witnessed(p, block, ni as u16, h, policy) {
                    step.values.push(TirWitnessValueV1 { occurrence: occ as u16, node: ni as u16, value: v.clone() });
                }
                if node.commit {
                    step.commits.push(TirCommitRowV1 { slot: base + ni as u32, value: v.clone() });
                }
                match node.prim {
                    Prim::StateWrite { state } => writes.push((state, layer, v.clone())),
                    Prim::HistAppend { state } => {
                        appends.push((state, layer, ref_value(&interp, params, &ctx, &run, &values, node.inputs[0])?))
                    }
                    _ => {}
                }
            }
            if occ == post_occ {
                let logits = values[p.logits as usize].as_ref().expect("evaluated");
                let g = decode_select(logits);
                generated.push(g);
                if pos + 1 < positions {
                    tokens.push(g);
                }
            } else {
                carry = b.carry_out.iter().map(|c| values[*c as usize].clone().expect("evaluated")).collect();
            }
        }
        run.apply(p, writes, appends);
        steps.push(step);
    }
    let commit_root = tir_commit_root_v1(&steps);
    Ok(TirWitnessV1 { prompt_len, tokens, generated, steps, commit_root })
}

/// Collects one position's served values and committed rows from the typed executor.
struct CaptureSink<'s> {
    plan: &'s TirPlan,
    analysis: &'s TirSketchAnalysisV1,
    policy: &'s TirCheckPolicyV1,
    pos: u32,
    step: TirWitnessStepV1,
}

impl StepSink for CaptureSink<'_> {
    fn every_node(&self) -> bool {
        true
    }

    fn node(&mut self, v: &NodeValue<'_>) {
        let p = &self.plan.program;
        let h = h_at(self.plan, v.block, self.pos);
        if self.analysis.witnessed(p, v.block, v.node, h, self.policy) {
            let occurrence = occurrence_of(p, v.block, v.layer);
            self.step.values.push(TirWitnessValueV1 { occurrence, node: v.node, value: v.to_tensor() });
        }
        if v.commit {
            self.step.commits.push(TirCommitRowV1 { slot: v.slot, value: v.to_tensor() });
        }
    }
}

/// **The node's producer**: the job on the typed backend ([`TirExecutor`]), every node's value
/// delivered to a sink that keeps the served ones and the committed rows. Equal to
/// [`tir_witness_produce_v1`] with no tamper (`tests/soundness.rs`).
pub fn tir_witness_capture_v1(
    plan: &TirPlan,
    analysis: &TirSketchAnalysisV1,
    params: &TirParams<'_>,
    job: &TirSketchJobV1,
    policy: &TirCheckPolicyV1,
) -> TirResult<TirWitnessV1> {
    let prompt_len = job.prompt.len() as u32;
    if prompt_len == 0 {
        return Err(TirError::new(TirErrorKind::Malformed, "a job has at least one prompt token"));
    }
    let positions = job.positions();
    let mut exec = TirExecutor::new(plan, params)?;
    let mut tokens = job.prompt.clone();
    let mut generated = Vec::new();
    let mut steps = Vec::with_capacity(positions as usize);
    for pos in 0..positions {
        let run_post = pos + 1 >= prompt_len;
        let mut sink =
            CaptureSink { plan, analysis, policy, pos, step: TirWitnessStepV1 { pos, values: Vec::new(), commits: Vec::new() } };
        exec.step_opt(tokens[pos as usize], &mut sink, run_post)?;
        if run_post {
            let (shape, data) = exec.logits();
            let logits = Tensor {
                dtype: plan.program.blocks[plan.program.schedule.post as usize].nodes[plan.program.logits as usize].out.dtype,
                shape: shape.to_vec(),
                data: data.to_i128s(),
            };
            let g = decode_select(&logits);
            generated.push(g);
            if pos + 1 < positions {
                tokens.push(g);
            }
        }
        steps.push(sink.step);
    }
    let commit_root = tir_commit_root_v1(&steps);
    Ok(TirWitnessV1 { prompt_len, tokens, generated, steps, commit_root })
}
