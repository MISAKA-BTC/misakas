//! **One node at a time, by the court's own evaluator** — the exact half of a check, and the
//! producer this prototype lies with.
//!
//! Every exactly evaluated node is the reference evaluator's [`Interpreter::eval_cone`] of that
//! node with each of its node operands SUPPLIED (spec 04b §9.2): the closure is the node itself, so
//! a position costs one evaluation per node, and each value is exactly what the court would compute
//! from the same operands. The run state between positions — `Fixed` values, history rows — is the
//! one spec 04b §9.1 defines: effects apply after the whole step.

use std::collections::{BTreeMap, VecDeque};

use misaka_palw_tir::program::{Ref, StateKind, TirProgramV1};
use misaka_palw_tir::{ConeEnv, Interpreter, ParamSource, Prim, Tensor, TirError, TirErrorKind, TirResult};

/// The run state between positions.
#[derive(Clone, Debug, Default)]
pub(crate) struct RunStateV1 {
    pub fixed: BTreeMap<(u16, Option<u16>), Tensor>,
    pub hist: BTreeMap<(u16, Option<u16>), VecDeque<Tensor>>,
}

fn instance(p: &TirProgramV1, j: u16, layer: Option<u16>) -> (u16, Option<u16>) {
    (j, if p.states[j as usize].per_layer { layer } else { None })
}

impl RunStateV1 {
    fn fixed_value(&self, p: &TirProgramV1, j: u16, layer: Option<u16>) -> Tensor {
        let s = &p.states[j as usize];
        self.fixed
            .get(&instance(p, j, layer))
            .cloned()
            .unwrap_or_else(|| Tensor::zeros(s.dtype, &s.shape.iter().map(|d| *d as usize).collect::<Vec<_>>()))
    }

    fn prior_rows(&self, p: &TirProgramV1, j: u16, layer: Option<u16>) -> Vec<Tensor> {
        self.hist.get(&instance(p, j, layer)).map(|r| r.iter().cloned().collect()).unwrap_or_default()
    }

    /// Apply one position's effects: the state writes' values, the appended rows.
    pub fn apply(&mut self, p: &TirProgramV1, writes: Vec<(u16, Option<u16>, Tensor)>, appends: Vec<(u16, Option<u16>, Tensor)>) {
        for (j, layer, v) in writes {
            self.fixed.insert(instance(p, j, layer), v);
        }
        for (j, layer, row) in appends {
            let StateKind::Hist { window } = p.states[j as usize].kind else { continue };
            let rows = self.hist.entry(instance(p, j, layer)).or_default();
            rows.push_back(row);
            while rows.len() > (window as usize).saturating_sub(1) {
                rows.pop_front();
            }
        }
    }
}

/// What one occurrence of one position reads besides its nodes.
pub(crate) struct OccCtxV1<'c> {
    pub pos: u32,
    pub token: u32,
    pub block: u8,
    pub layer: Option<u16>,
    pub carry: &'c [Tensor],
}

/// **Node `ni` of the occurrence, exactly**, from the values its node operands already have.
pub(crate) fn eval_node(
    interp: &Interpreter<'_>,
    params: &dyn ParamSource,
    ctx: &OccCtxV1<'_>,
    run: &RunStateV1,
    values: &[Option<Tensor>],
    ni: usize,
) -> TirResult<Tensor> {
    let p = interp.program;
    let node = &p.blocks[ctx.block as usize].nodes[ni];
    let mut env = ConeEnv { token: Some(ctx.token), pos: ctx.pos, ..Default::default() };
    for r in &node.inputs {
        match *r {
            Ref::Node(j) => {
                let v = values[j as usize]
                    .clone()
                    .ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("node {j}, operand of node {ni}")))?;
                env.supplied.insert(j, v);
            }
            Ref::CarryIn(k) => {
                let v =
                    ctx.carry.get(k as usize).cloned().ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("carry-in {k}")))?;
                env.carry_in.insert(k, v);
            }
            Ref::State(j) => {
                env.fixed.insert(j, run.fixed_value(p, j, ctx.layer));
            }
            _ => {}
        }
    }
    if let Prim::HistAppend { state } = node.prim {
        env.hist_prior.insert(state, run.prior_rows(p, state, ctx.layer));
    }
    interp.eval_cone(ctx.block, ctx.layer, ni as u16, params, &env)
}

/// The value a `Ref` names in an occurrence whose node values are `values`.
pub(crate) fn ref_value(
    interp: &Interpreter<'_>,
    params: &dyn ParamSource,
    ctx: &OccCtxV1<'_>,
    run: &RunStateV1,
    values: &[Option<Tensor>],
    r: Ref,
) -> TirResult<Tensor> {
    let p = interp.program;
    match r {
        Ref::Node(j) => values[j as usize].clone().ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("node {j}"))),
        Ref::CarryIn(k) => {
            ctx.carry.get(k as usize).cloned().ok_or_else(|| TirError::new(TirErrorKind::Missing, format!("carry-in {k}")))
        }
        Ref::State(j) => Ok(run.fixed_value(p, j, ctx.layer)),
        Ref::Input(0) => Tensor::scalar(misaka_palw_tir::DType::Idx, ctx.token as i128),
        Ref::Input(_) => Tensor::scalar(misaka_palw_tir::DType::Idx, ctx.pos as i128),
        Ref::Param(_) | Ref::Const(_) => crate::sketch::static_value(interp, params, ctx.block, ctx.layer, r),
    }
}

/// The occurrence index of `(block, layer)` in a step (`pre` 0, layer `l` at `1 + l`, `post` last).
pub(crate) fn occurrence_of(p: &TirProgramV1, block: u8, layer: Option<u16>) -> u16 {
    match layer {
        Some(l) => 1 + l,
        None if block == p.schedule.pre => 0,
        None => p.schedule.layers.len() as u16 + 1,
    }
}

/// `argmax` of a logits row, ties to the lowest index (`base0_decode_token_select_v1`).
pub(crate) fn decode_select(logits: &Tensor) -> u32 {
    let mut best = 0usize;
    for (i, v) in logits.data.iter().enumerate() {
        if *v > logits.data[best] {
            best = i;
        }
    }
    best as u32
}
