//! Shared test harness: run a program on the reference evaluator and on the typed backend side by
//! side and require byte-identity — logits, every commit point, the run state after every step,
//! success versus failure of every step, and (optionally) the value of every uncommitted node,
//! checked against the reference's own cone evaluation from the step's committed values.
#![allow(dead_code)]

pub mod progen;
pub mod typing;

use std::collections::{BTreeMap, VecDeque};

use misaka_palw_tir::interp::CommitRecord;
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, RunState, Tensor};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, TirParams, TirPlan};

/// One node value the executor produced.
#[derive(Clone, Debug)]
pub struct Rec {
    pub slot: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub node: u16,
    pub commit: bool,
    pub value: Tensor,
}

/// Records what a step delivers.
pub struct Collect {
    pub every: bool,
    pub values: Vec<Rec>,
}

impl Collect {
    pub fn new(every: bool) -> Self {
        Collect { every, values: Vec::new() }
    }
}

impl StepSink for Collect {
    fn every_node(&self) -> bool {
        self.every
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        self.values.push(Rec { slot: v.slot, block: v.block, layer: v.layer, node: v.node, commit: v.commit, value: v.to_tensor() });
    }
}

#[derive(Clone, Debug, Default)]
pub struct Opts {
    /// Check every uncommitted node against the reference's cone evaluation.
    pub every_node: bool,
    /// Start both from this state instead of the initial one.
    pub start: Option<RunState>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub steps_ok: usize,
    pub steps_err: usize,
    pub commits: usize,
    pub nodes: usize,
    /// The program itself was refused by both (normal form / types).
    pub refused: bool,
}

impl std::ops::AddAssign for Outcome {
    fn add_assign(&mut self, o: Self) {
        self.steps_ok += o.steps_ok;
        self.steps_err += o.steps_err;
        self.commits += o.commits;
        self.nodes += o.nodes;
        self.refused |= o.refused;
    }
}

fn zeros_like(t: &Tensor) -> Tensor {
    Tensor { dtype: t.dtype, shape: t.shape.clone(), data: vec![0; t.data.len()] }
}

/// The reference's run state against the executor's (absent instances are initial ones).
pub fn same_state(reference: &RunState, exec: &RunState) -> Result<(), String> {
    if reference.pos != exec.pos {
        return Err(format!("pos {} vs {}", reference.pos, exec.pos));
    }
    for (key, mine) in &exec.fixed {
        let theirs = reference.fixed.get(key).cloned().unwrap_or_else(|| zeros_like(mine));
        if &theirs != mine {
            return Err(format!("fixed {key:?}: reference {:?} executor {:?}", theirs.data, mine.data));
        }
    }
    for key in reference.fixed.keys() {
        if !exec.fixed.contains_key(key) {
            return Err(format!("fixed {key:?} missing in the executor"));
        }
    }
    for (key, mine) in &exec.hist {
        let theirs = reference.hist.get(key).cloned().unwrap_or_default();
        if &theirs != mine {
            return Err(format!("hist {key:?}: reference {} rows, executor {} rows (or values differ)", theirs.len(), mine.len()));
        }
    }
    for (key, rows) in &reference.hist {
        if !exec.hist.contains_key(key) && !rows.is_empty() {
            return Err(format!("hist {key:?} missing in the executor"));
        }
    }
    Ok(())
}

fn occurrence_of(bases: &[u32], slot: u32) -> usize {
    bases.iter().rposition(|b| *b <= slot).expect("slot 0 is the first base")
}

/// The reference cone environment of one occurrence of an honest step.
fn cone_env(program: &TirProgramV1, before: &RunState, commits: &[CommitRecord], occ: usize, target: u16, token: u32) -> ConeEnv {
    let bases = program.occurrence_slot_bases();
    let occs = program.occurrences();
    let (block, layer) = occs[occ];
    let len = program.blocks[block as usize].nodes.len() as u32;
    let supplied: BTreeMap<u16, Tensor> = commits
        .iter()
        .filter(|c| c.slot >= bases[occ] && c.slot < bases[occ] + len && c.node != target)
        .map(|c| (c.node, c.value.clone()))
        .collect();
    let carry_in: BTreeMap<u8, Tensor> = if occ == 0 {
        BTreeMap::new()
    } else {
        let (pb, _) = occs[occ - 1];
        program.blocks[pb as usize]
            .carry_out
            .iter()
            .enumerate()
            .map(|(k, n)| {
                (
                    k as u8,
                    commits.iter().find(|c| c.slot == bases[occ - 1] + *n as u32).expect("carry-outs are committed").value.clone(),
                )
            })
            .collect()
    };
    let fixed = before.fixed.iter().filter(|((_, l), _)| *l == layer).map(|((j, _), v)| (*j, v.clone())).collect();
    let hist_prior =
        before.hist.iter().filter(|((_, l), _)| *l == layer).map(|((j, _), v)| (*j, v.iter().cloned().collect())).collect();
    ConeEnv { token: Some(token), pos: before.pos, carry_in, fixed, hist_prior, supplied }
}

/// Run `tokens` on both implementations and require them identical.
pub fn differential(program: &TirProgramV1, params: &MapParams, tokens: &[u32], opts: &Opts) -> Result<Outcome, String> {
    let mut out = Outcome::default();
    let reference = Interpreter::new(program);
    let plan = TirPlan::compile(program);
    let (interp, plan) = match (reference, plan) {
        (Ok(i), Ok(p)) => (i, p),
        (Err(_), Err(_)) => {
            out.refused = true;
            return Ok(out);
        }
        (a, b) => return Err(format!("validation differs: reference {:?}, executor {:?}", a.err(), b.err())),
    };
    let mut ref_state = opts.start.clone().unwrap_or_default();
    let exec_params = TirParams::from_map(&plan, params);
    let exec_params = match exec_params {
        Ok(p) => p,
        Err(e) => {
            // The executor refuses the params: every reference step must fail.
            for &t in tokens {
                if let Ok(s) = interp.step(params, &mut ref_state, t) {
                    return Err(format!("executor refused params ({e}) but the reference stepped pos {}", s.pos));
                }
                out.steps_err += 1;
            }
            return Ok(out);
        }
    };
    let mut exec = TirExecutor::new(&plan, &exec_params).map_err(|e| format!("executor: {e}"))?;
    if let Some(st) = &opts.start {
        exec.import_state(st).map_err(|e| format!("import: {e}"))?;
    }
    same_state(&ref_state, &exec.export_state()).map_err(|e| format!("initial state: {e}"))?;
    for (i, &t) in tokens.iter().enumerate() {
        let before = ref_state.clone();
        let r = interp.step(params, &mut ref_state, t);
        let mut sink = Collect::new(opts.every_node);
        let e = exec.step(t, &mut sink);
        match (r, e) {
            (Ok(step), Ok(())) => {
                out.steps_ok += 1;
                let (shape, data) = exec.logits();
                let logits = Tensor { dtype: step.logits.dtype, shape: shape.to_vec(), data: data.to_i128s() };
                if logits != step.logits {
                    return Err(format!(
                        "step {i} (pos {}): logits differ\n reference {:?}\n executor  {:?}",
                        step.pos, step.logits, logits
                    ));
                }
                let mine: Vec<&Rec> = sink.values.iter().filter(|r| r.commit).collect();
                if mine.len() != step.commits.len() {
                    return Err(format!("step {i}: {} commits vs {}", step.commits.len(), mine.len()));
                }
                for (c, m) in step.commits.iter().zip(&mine) {
                    if (c.slot, c.block, c.layer, c.node) != (m.slot, m.block, m.layer, m.node) || c.value != m.value {
                        return Err(format!(
                            "step {i} (pos {}): commit slot {} (block {} layer {:?} node {}) differs\n reference {:?}\n executor  {:?}",
                            step.pos, c.slot, c.block, c.layer, c.node, c.value, m.value
                        ));
                    }
                }
                out.commits += mine.len();
                if opts.every_node {
                    let bases = program.occurrence_slot_bases();
                    for r in sink.values.iter().filter(|r| !r.commit) {
                        let occ = occurrence_of(&bases, r.slot);
                        let env = cone_env(program, &before, &step.commits, occ, r.node, t);
                        let want =
                            interp.eval_cone(r.block, r.layer, r.node, params, &env).map_err(|e| format!("reference cone: {e}"))?;
                        if want != r.value {
                            return Err(format!(
                                "step {i} (pos {}): node {} of block {} layer {:?} differs\n reference {:?}\n executor  {:?}",
                                step.pos, r.node, r.block, r.layer, want, r.value
                            ));
                        }
                        out.nodes += 1;
                    }
                }
            }
            (Err(_), Err(_)) => out.steps_err += 1,
            (Ok(s), Err(e)) => return Err(format!("step {i} (pos {}): reference ok, executor failed: {e}", s.pos)),
            (Err(e), Ok(())) => return Err(format!("step {i} (pos {}): reference failed ({e}), executor ok", before.pos)),
        }
        same_state(&ref_state, &exec.export_state()).map_err(|e| format!("after step {i}: {e}"))?;
    }
    Ok(out)
}

/// A run state from rows and values, for starting both implementations mid-run.
pub fn hist_rows(rows: Vec<Tensor>) -> VecDeque<Tensor> {
    rows.into()
}

pub fn hex_decode(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}
