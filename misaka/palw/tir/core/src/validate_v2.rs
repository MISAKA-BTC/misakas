//! **Normal form of version-2 programs** (spec 04b §15.2, NF-23 … NF-29).
//!
//! The version-2 rules are checked first, each with its own refusal; then version-1 normal form and
//! type inference run, unchanged, over the program's version-1 view
//! ([`TirProgramV2::v1_view`]); then each `post` `StateWrite` is held to the `StateWrite` type rule
//! the view replaced with a `Clamp`. After [`validate_v2`] passes, every rule of version 1 holds for
//! the view, so the version-1 evaluator and range analysis are total on it.

use std::collections::BTreeSet;

use crate::error::{TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::program::*;
use crate::program_v2::*;
use crate::types::{DType, MAX_DIM, MAX_ELEMENTS, MAX_RANK, TensorType};
use crate::validate::{ProgramInfo, check_node_type, validate};

/// What validation of a version-2 program learned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramInfoV2 {
    /// The version-1 view the evaluator and the range analysis run over.
    pub view: TirProgramV1,
    /// Version-1 validation of the view.
    pub v1: ProgramInfo,
    /// The `post` `StateWrite`s of a `Rows`/`Final` program: `(node, state)`.
    pub post_writes: Vec<(u16, u16)>,
    /// The view's param index of input 0 (`= |params|`).
    pub first_input_param: u16,
}

fn nf<T>(msg: impl Into<String>) -> TirResult<T> {
    err(TirErrorKind::NormalForm, msg)
}

fn check_input_shape(k: usize, shape: &[u32]) -> TirResult<()> {
    if shape.len() > MAX_RANK {
        return nf(format!("input {k}: rank {} exceeds {MAX_RANK}", shape.len()));
    }
    let mut n = 1u64;
    for d in shape {
        if *d == 0 || *d > MAX_DIM {
            return nf(format!("input {k}: dimension {d} outside [1, 2^24]"));
        }
        n = n.saturating_mul(*d as u64);
    }
    if n > MAX_ELEMENTS {
        return nf(format!("input {k}: {n} elements exceed 2^28"));
    }
    Ok(())
}

/// The type of an operand of the view (whose refs version-1 validation has already checked).
fn ref_type(view: &TirProgramV1, block: usize, r: Ref) -> TensorType {
    let b = &view.blocks[block];
    match r {
        Ref::Node(i) => b.nodes[i as usize].out.clone(),
        Ref::CarryIn(k) => b.carry_in[k as usize].clone(),
        Ref::Param(j) => TensorType::fixed(view.params[j as usize].dtype, &view.params[j as usize].shape),
        Ref::Const(j) => TensorType::fixed(view.consts[j as usize].dtype, &view.consts[j as usize].shape),
        Ref::State(j) => TensorType::fixed(view.states[j as usize].dtype, &view.states[j as usize].shape),
        Ref::Input(_) => TensorType::scalar(DType::Idx),
    }
}

/// Validate a version-2 program; return its view and what the evaluator needs.
pub fn validate_v2(p: &TirProgramV2) -> TirResult<ProgramInfoV2> {
    // ---- NF-23: the version ----------------------------------------------------------------------
    if p.version != TIR_PROGRAM_VERSION_V2 {
        return nf(format!("version {} is not {TIR_PROGRAM_VERSION_V2}", p.version));
    }
    // ---- NF-24: the input declarations ---------------------------------------------------------------
    if p.inputs.len() > MAX_INPUTS_V2 {
        return nf(format!("at most {MAX_INPUTS_V2} inputs"));
    }
    if p.params.len() + p.inputs.len() > MAX_PARAMS {
        return nf(format!("params and inputs share the {MAX_PARAMS}-tensor cap"));
    }
    let param_names: BTreeSet<&str> = p.params.iter().map(|d| d.name.as_str()).collect();
    let mut input_names = BTreeSet::new();
    let mut random_domains = BTreeSet::new();
    for (k, inp) in p.inputs.iter().enumerate() {
        if inp.name.is_empty() || inp.name.len() > MAX_NAME_BYTES {
            return nf(format!("input {k}: a name is 1..={MAX_NAME_BYTES} bytes"));
        }
        if param_names.contains(inp.name.as_str()) || !input_names.insert(inp.name.as_str()) {
            return nf(format!("input {k}: the name {} is declared twice (inputs and params share one name space)", inp.name));
        }
        check_input_shape(k, &inp.shape)?;
        match inp.source {
            // ---- NF-25: an external input ------------------------------------------------------
            InputSource::External { lo, hi } => {
                if !matches!(inp.dtype, DType::I8 | DType::I16 | DType::I32 | DType::Idx) {
                    return nf(format!("input {k}: an external input is i8, i16, i32 or idx"));
                }
                if lo > hi || !inp.dtype.contains(lo as i128) || !inp.dtype.contains(hi as i128) {
                    return nf(format!("input {k}: [{lo}, {hi}] is not an interval inside {}", inp.dtype.name()));
                }
            }
            // ---- NF-26: a random input ---------------------------------------------------------
            InputSource::Random { domain, dist, per_step } => {
                let Some((bits, rule)) = random_input_domain_v2(domain) else {
                    return nf(format!("input {k}: domain {domain} is not a registered program-input domain"));
                };
                match dist {
                    RandomDist::Uniform { bits: b } => {
                        if inp.dtype != DType::Idx || b != bits {
                            return nf(format!("input {k}: Uniform is idx with the domain's {bits}-bit words"));
                        }
                    }
                    RandomDist::Normal => {
                        if inp.dtype != DType::I32 || bits != 16 {
                            return nf(format!("input {k}: Normal is i32 over a domain of 16-bit words"));
                        }
                    }
                }
                let step_ok = match rule {
                    RandStepRule::Zero => !per_step,
                    RandStepRule::PerStep => per_step,
                    RandStepRule::Declared => true,
                };
                if !step_ok {
                    return nf(format!("input {k}: per_step = {per_step} contradicts domain {domain}'s step rule"));
                }
                if !random_domains.insert(domain) {
                    return nf(format!("input {k}: domain {domain} is declared twice (one random input per domain)"));
                }
            }
        }
    }
    // ---- NF-27: references to inputs, and every input used ------------------------------------------
    let mut used = vec![false; p.inputs.len()];
    for (bi, b) in p.blocks.iter().enumerate() {
        for (ni, n) in b.nodes.iter().enumerate() {
            for r in &n.inputs {
                if let Ref::Input(j) = *r
                    && j >= FIRST_INPUT_REF_V2
                {
                    let k = (j - FIRST_INPUT_REF_V2) as usize;
                    match used.get_mut(k) {
                        Some(u) => *u = true,
                        None => return nf(format!("block {bi} node {ni}: no input {j}")),
                    }
                }
            }
        }
    }
    if let Some(k) = used.iter().position(|u| !u) {
        return nf(format!("input {k} is never used"));
    }
    // ---- NF-28: the output node is a committed node of post ------------------------------------------
    // Checked on the version-2 program itself: the view marks every post write committed, so a view
    // check alone would accept an uncommitted write as the output.
    let committed_output =
        p.blocks.get(p.schedule.post as usize).and_then(|b| b.nodes.get(p.output.node() as usize)).map(|n| n.commit);
    match committed_output {
        None => return nf(format!("the {} output node {} does not exist in post", p.output.name(), p.output.node())),
        Some(false) => return nf(format!("the {} output node {} is not a commit point", p.output.name(), p.output.node())),
        Some(true) => {}
    }
    // ---- NF-29: post effects -------------------------------------------------------------------------
    let post_writes = p.post_writes();
    if !post_writes.is_empty() {
        let pre = p.blocks.get(p.schedule.pre as usize);
        let post_nodes = &p.blocks[p.schedule.post as usize].nodes;
        let mut seen = BTreeSet::new();
        for (node, state) in &post_writes {
            let ok = p.states.get(*state as usize).is_some_and(|s| matches!(s.kind, StateKind::Fixed { .. }) && !s.per_layer);
            if !ok {
                return nf(format!("post node {node}: a post StateWrite targets a global Fixed state"));
            }
            // A post write is a commit point: the state's next value is then a leaf of the step tree
            // (the court reads the latent at `p` as the committed write at `p − 1`, never a replay of
            // `post`), and the view's commit points are exactly the program's.
            if !post_nodes[*node as usize].commit {
                return nf(format!("post node {node}: a post StateWrite is a commit point"));
            }
            if !seen.insert(*state) {
                return nf(format!("post writes state {state} twice"));
            }
            if pre.is_some_and(|b| b.nodes.iter().any(|n| n.prim == Prim::StateWrite { state: *state })) {
                return nf(format!("state {state} is written by pre and by post (one writer per instance per step)"));
            }
            let read = p.blocks.iter().any(|b| b.nodes.iter().any(|n| n.inputs.contains(&Ref::State(*state))));
            if !read {
                return nf(format!("state {state} is written by post and read by nobody"));
            }
        }
    }
    // ---- Version-1 normal form and types over the view (NF-1 … NF-22, NF-28 through NF-6) ------------
    let view = p.v1_view();
    let v1 = validate(&view)?;
    // ---- The StateWrite type rule for the post writes the view expressed as Clamps -------------------
    let post = p.schedule.post as usize;
    for (node, state) in &post_writes {
        let n = &view.blocks[post].nodes[*node as usize];
        let ins: Vec<TensorType> = n.inputs.iter().map(|r| ref_type(&view, post, *r)).collect();
        check_node_type(&Prim::StateWrite { state: *state }, &ins, &n.out, &p.states)
            .or_else(|m| err(TirErrorKind::Shape, format!("post node {node} (StateWrite): {m}")))?;
    }
    Ok(ProgramInfoV2 { view, v1, post_writes, first_input_param: p.params.len() as u16 })
}
