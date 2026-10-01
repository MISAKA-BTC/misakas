//! Cone evaluation (spec 04b §9.2, PALW-TIR-30) on the typed backend: one node of one occurrence
//! from supplied values, evaluating only the backward closure of the target that stops at
//! supplied nodes — the reference evaluator's `Interpreter::eval_cone`, value for value, failure
//! for failure, and class for class (spec 04b revision 2, §9.3):
//!
//! * a request that names no occurrence or no node, or an environment that supplies the target
//!   itself or an index that is no node of the block — `Malformed`, before anything else;
//! * `pos ≥ history_bound` — `Position`; a token the closure reads that is absent — `Missing`, at or
//!   past `token_bound` — `Operand`; both before any node runs;
//! * a `Fixed` value or a history the closure reads is never implied: absent is `Missing`, even when
//!   the history needs zero rows; a carry-in or param absent is `Missing`; any of them of the wrong
//!   dtype, shape or values (a `Fixed` value outside `[lo, hi]`) is `Operand`; a history with the
//!   wrong number of rows is `Position`.

use std::collections::BTreeMap;

use misaka_palw_tir::program::{INPUT_TOKEN, StateKind};
use misaka_palw_tir::{ConeEnv, Prim, Ref, Tensor, TensorType, TirError, TirErrorKind, TirResult};

use crate::elem::Buf;
use crate::exec::{HistBuf, Reader, Src, Val, eval_compute, ref_val, resolve};
use crate::kernels::{Scratch, elementwise, materialize};
use crate::layout::Layout;
use crate::params::TirParams;
use crate::plan::TirPlan;

fn operand_err(what: String) -> TirError {
    TirError::new(TirErrorKind::Operand, what)
}

/// `t` is a value of `want` at `h` (dtype, shape, every element inside the dtype).
fn check_value(t: &Tensor, want: &TensorType, h: usize, what: &str) -> TirResult<()> {
    if t.dtype != want.dtype || t.shape != want.resolve(h) {
        return Err(operand_err(format!(
            "{what}: got {} {:?}, declared {} {:?}",
            t.dtype.name(),
            t.shape,
            want.dtype.name(),
            want.resolve(h)
        )));
    }
    if t.data.len() != t.shape.iter().product::<usize>() || t.data.iter().any(|v| !t.dtype.contains(*v)) {
        return Err(operand_err(format!("{what}: a value outside {}", t.dtype.name())));
    }
    Ok(())
}

/// Evaluate node `target` of `block` at `layer` from `env` — the court's entry point, here on the
/// typed backend (evidence building, the responder's side of a dispute).
pub fn eval_cone(
    plan: &TirPlan,
    params: &TirParams<'_>,
    block: u8,
    layer: Option<u16>,
    target: u16,
    env: &ConeEnv,
) -> TirResult<Tensor> {
    let p = &plan.program;
    let malformed = |what: &str| TirError::new(TirErrorKind::Malformed, what.to_string());
    let bp = plan.blocks.get(block as usize).ok_or_else(|| malformed("no such block"))?;
    let n = bp.nodes.len();
    if target as usize >= n {
        return Err(malformed("no such node"));
    }
    match layer {
        Some(l) if !bp.is_layer || l as usize >= p.schedule.layers.len() || p.schedule.layers[l as usize] != block => {
            return Err(malformed("the block does not run at that layer"));
        }
        None if bp.is_layer => return Err(malformed("a layer block needs its layer")),
        _ => {}
    }
    // The target is always recomputed; an entry that names no node is about another program.
    if env.supplied.contains_key(&target) {
        return Err(malformed("the environment supplies the target itself"));
    }
    if env.supplied.keys().any(|k| *k as usize >= n) {
        return Err(malformed("the environment supplies an index that is no node of the block"));
    }
    if env.pos >= p.history_bound {
        return Err(TirError::new(TirErrorKind::Position, "position ≥ history_bound"));
    }
    let h = bp.window.map(|w| (env.pos as usize + 1).min(w as usize)).unwrap_or(1);
    // The closure: backward from the target, stopping at supplied nodes.
    let mut needed = vec![false; n];
    let mut stack = vec![target as usize];
    while let Some(i) = stack.pop() {
        if std::mem::replace(&mut needed[i], true) || env.supplied.contains_key(&(i as u16)) {
            continue;
        }
        for r in &bp.nodes[i].inputs {
            if let Ref::Node(j) = r {
                stack.push(*j as usize);
            }
        }
    }
    // The token, when the evaluated part reads it: present and inside `token_bound`, first.
    let evaluated = |i: usize| needed[i] && !env.supplied.contains_key(&(i as u16));
    if (0..n).filter(|i| evaluated(*i)).any(|i| bp.nodes[i].inputs.contains(&Ref::Input(INPUT_TOKEN))) {
        match env.token {
            None => return Err(TirError::new(TirErrorKind::Missing, "token")),
            Some(t) if t >= p.token_bound => return Err(operand_err(format!("token {t} ≥ token_bound {}", p.token_bound))),
            Some(_) => {}
        }
    }
    // Every leaf the evaluated part reads, checked as the reference checks it when it reads it.
    let mut fixed = vec![Buf::default(); plan.instances.len()];
    let mut carry: Vec<Buf> = Vec::new();
    for ni in (0..n).filter(|i| needed[*i] && !env.supplied.contains_key(&(*i as u16))) {
        let node = &bp.nodes[ni];
        for r in &node.inputs {
            match *r {
                Ref::CarryIn(k) => {
                    let t = env.carry_in.get(&k).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("carry-in {k}")))?;
                    check_value(t, &bp.carry_in[k as usize], h, &format!("carry-in {k}"))?;
                    if carry.len() <= k as usize {
                        carry.resize_with(k as usize + 1, Buf::default);
                    }
                    carry[k as usize] = Buf::from_i128s(t.dtype, &t.data);
                }
                Ref::State(j) => {
                    let s = &p.states[j as usize];
                    let inst = plan.instance(j, layer).ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("state {j}")))?;
                    // Opened from a commitment, never implied (not zeros, not the initial value).
                    let t = env.fixed.get(&j).ok_or_else(|| {
                        TirError::new(TirErrorKind::Missing, format!("state {} (a cone's Fixed value is supplied)", s.name))
                    })?;
                    check_value(t, &TensorType::fixed(s.dtype, &s.shape), h, &format!("state {}", s.name))?;
                    if let StateKind::Fixed { lo, hi } = s.kind
                        && t.data.iter().any(|v| *v < lo as i128 || *v > hi as i128)
                    {
                        return Err(operand_err(format!("state {}: a value outside [{lo}, {hi}]", s.name)));
                    }
                    fixed[inst as usize] = Buf::from_i128s(s.dtype, &t.data);
                }
                Ref::Param(j) => {
                    if !params.has(j, layer) {
                        return Err(TirError::new(
                            TirErrorKind::Missing,
                            format!("param {} (layer {layer:?})", p.params[j as usize].name),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    let inputs = [env.token.unwrap_or(0), env.pos];
    let fixed_next = vec![Buf::default(); plan.instances.len()];
    let no_hist: Vec<HistBuf> = Vec::new();
    let mut slots: Vec<Buf> = vec![Buf::default(); n];
    let mut vals: Vec<Val> = vec![Val::default(); n];
    let mut scratch = Scratch::default();
    let supplied: BTreeMap<u16, &Tensor> = env.supplied.iter().map(|(k, v)| (*k, v)).collect();
    for ni in 0..n {
        if !needed[ni] {
            continue;
        }
        let node = &bp.nodes[ni];
        let (shape, rank) = resolve(&node.out.shape, h);
        let out_shape = &shape[..rank];
        if let Some(t) = supplied.get(&(ni as u16)) {
            check_value(t, &node.out, h, &format!("supplied node {ni}"))?;
            slots[ni] = Buf::from_i128s(t.dtype, &t.data);
            vals[ni] = Val { src: Src::Slot(ni as u16), layout: Layout::contiguous(out_shape) };
            continue;
        }
        let mut out = std::mem::take(&mut slots[ni]);
        let r = {
            let rd = Reader {
                plan,
                params,
                layer,
                fixed: &fixed,
                fixed_next: &fixed_next,
                hist: &no_hist,
                carry: &carry,
                slots: &slots,
                inputs: &inputs,
            };
            match node.prim {
                Prim::StateWrite { .. } => rd
                    .opd(ref_val(plan, bp, node.inputs[0], &vals, layer)?)
                    .and_then(|x| elementwise::unary(node, &x, &mut out, &mut scratch, &p.states))
                    .map(|_| None),
                Prim::HistAppend { state } => {
                    let s = &p.states[state as usize];
                    let StateKind::Hist { window } = s.kind else {
                        return Err(TirError::new(TirErrorKind::Shape, "HistAppend on a Fixed state"));
                    };
                    // Supplied even when no row is needed: absent is `Missing`.
                    let prior: Vec<Tensor> = env.hist_prior.get(&state).cloned().ok_or_else(|| {
                        TirError::new(
                            TirErrorKind::Missing,
                            format!("history {} (a cone's prior rows are supplied, even when none)", s.name),
                        )
                    })?;
                    let want = (env.pos as usize).min(window as usize - 1);
                    if prior.len() != want {
                        return Err(TirError::new(
                            TirErrorKind::Position,
                            format!("history {}: {} prior rows, want {want}", s.name, prior.len()),
                        ));
                    }
                    let row_t = TensorType::fixed(s.dtype, &s.shape);
                    let mut all: Vec<i128> = Vec::new();
                    for r in &prior {
                        check_value(r, &row_t, h, &format!("history {} row", s.name))?;
                        all.extend_from_slice(&r.data);
                    }
                    let row = rd.opd(ref_val(plan, bp, node.inputs[0], &vals, layer)?)?;
                    let mut rb = Buf::default();
                    materialize(&row, &mut rb);
                    all.extend(rb.to_i128s());
                    out = Buf::from_i128s(s.dtype, &all);
                    Ok(None)
                }
                _ => eval_compute(plan, bp, node, (block, ni as u16), &rd, &vals, out_shape, &mut out, &mut scratch),
            }
        };
        slots[ni] = out;
        vals[ni] = match r? {
            Some(view) => view,
            None => Val { src: Src::Slot(ni as u16), layout: Layout::contiguous(out_shape) },
        };
    }
    let rd = Reader {
        plan,
        params,
        layer,
        fixed: &fixed,
        fixed_next: &fixed_next,
        hist: &no_hist,
        carry: &carry,
        slots: &slots,
        inputs: &inputs,
    };
    let v = vals[target as usize];
    let mut b = Buf::default();
    materialize(&rd.opd(v)?, &mut b);
    Ok(Tensor { dtype: bp.nodes[target as usize].out.dtype, shape: v.layout.shape().to_vec(), data: b.to_i128s() })
}
