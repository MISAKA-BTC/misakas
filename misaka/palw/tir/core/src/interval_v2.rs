//! **Range analysis of version-2 programs** (spec 04b §15.5, PALW-TIR-9).
//!
//! The version-1 transfer functions ([`crate::interval::transfer`]) run unchanged over the program's
//! version-1 view. The one difference is the leaves: an input is not a param of the dtype's full
//! range but a declared interval — `[lo, hi]` for an external input, `[0, 2^b − 1]` for a `Uniform`
//! random input, the Gaussian table's `[GAUSS_Q24_V1_MIN, GAUSS_Q24_V1_MAX]` for a `Normal` one.
//! A `post` `StateWrite` is the view's `Clamp` to the state's range, which is exactly the
//! saturation interval version 1 gives a `StateWrite`.
//!
//! PALW-TIR-33 carries over: an interval is also the domain of the node's committed values, and an
//! upstream stage's output bound to an external input is admitted only if its interval lies inside
//! the input's declared one ([`crate::pipeline`]).

use crate::error::{TirError, TirResult};
use crate::interval::{Interval, transfer};
use crate::program::{Ref, StateKind};
use crate::program_v2::TirProgramV2;
use crate::types::{DType, TensorType};
use crate::validate_v2::validate_v2;

/// The interval of every node of every block, or the first unmet obligation.
pub fn analyze_ranges_v2(p: &TirProgramV2) -> TirResult<Vec<Vec<Interval>>> {
    let info = validate_v2(p)?;
    let view = &info.view;
    let first = info.first_input_param;
    let mut out = Vec::with_capacity(view.blocks.len());
    for (bi, b) in view.blocks.iter().enumerate() {
        let window = info.v1.blocks[bi].window;
        let mut iv: Vec<Interval> = Vec::with_capacity(b.nodes.len());
        for (ni, n) in b.nodes.iter().enumerate() {
            let mut ins = Vec::with_capacity(n.inputs.len());
            let mut tys = Vec::with_capacity(n.inputs.len());
            for r in &n.inputs {
                let (i, t) = match *r {
                    Ref::Node(j) => (iv[j as usize], b.nodes[j as usize].out.clone()),
                    Ref::CarryIn(k) => (Interval::of(b.carry_in[k as usize].dtype), b.carry_in[k as usize].clone()),
                    Ref::Param(j) if j >= first => {
                        let d = &p.inputs[(j - first) as usize];
                        let (lo, hi) = d.interval();
                        (Interval::new(lo, hi), TensorType::fixed(d.dtype, &d.shape))
                    }
                    Ref::Param(j) => {
                        let d = &view.params[j as usize];
                        (Interval::of(d.dtype), TensorType::fixed(d.dtype, &d.shape))
                    }
                    Ref::Const(j) => {
                        let c = &view.consts[j as usize];
                        let vals: Vec<i128> = c.data.chunks_exact(c.dtype.width()).map(|e| c.dtype.decode_le(e)).collect();
                        (Interval::new(*vals.iter().min().unwrap(), *vals.iter().max().unwrap()), TensorType::fixed(c.dtype, &c.shape))
                    }
                    Ref::State(j) => {
                        let s = &view.states[j as usize];
                        let StateKind::Fixed { lo, hi } = s.kind else { unreachable!("validated: State refs name Fixed states") };
                        (Interval::new(lo as i128, hi as i128), TensorType::fixed(s.dtype, &s.shape))
                    }
                    Ref::Input(0) => (Interval::new(0, view.token_bound as i128 - 1), TensorType::scalar(DType::Idx)),
                    Ref::Input(_) => (Interval::new(0, view.history_bound as i128 - 1), TensorType::scalar(DType::Idx)),
                };
                ins.push(i);
                tys.push(t);
            }
            let r = transfer(&n.prim, &ins, &tys, &n.out, window, view)
                .map_err(|e| TirError::new(e.kind, format!("block {bi} node {ni} ({}): {}", n.prim.name(), e.msg)))?;
            iv.push(r);
        }
        out.push(iv);
    }
    Ok(out)
}

/// The interval of a program's output node — what a downstream stage may rely on.
pub fn output_interval_v2(p: &TirProgramV2) -> TirResult<Interval> {
    let ranges = analyze_ranges_v2(p)?;
    Ok(ranges[p.schedule.post as usize][p.output.node() as usize])
}
