//! The first implementation (`misaka-palw-tir`) as a black box: conversions between its public
//! types and this crate's, and a panic-catching call wrapper (a panic there is a PALW-TIR-34
//! totality defect, reported as its own outcome).
#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};

use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use ref2::eval::{ConeEnv, Params, RunState};
use ref2::{Class, DType, Prim, Tensor};

pub fn dtype_to(d: DType) -> first::types::DType {
    first::types::DType::from_name(d.name()).unwrap()
}

pub fn dtype_from(d: first::types::DType) -> DType {
    DType::from_name(d.name()).unwrap()
}

pub fn tensor_to(t: &Tensor) -> first::tensor::Tensor {
    first::tensor::Tensor { dtype: dtype_to(t.dtype), shape: t.shape.iter().map(|&d| d as usize).collect(), data: t.data.clone() }
}

pub fn tensor_from(t: &first::tensor::Tensor) -> Tensor {
    Tensor { dtype: dtype_from(t.dtype), shape: t.shape.iter().map(|&d| d as u64).collect(), data: t.data.clone() }
}

pub fn class_from(k: first::error::TirErrorKind) -> Class {
    use first::error::TirErrorKind as K;
    match k {
        K::Encoding => Class::Encoding,
        K::NormalForm => Class::NormalForm,
        K::Shape => Class::Shape,
        K::Overflow => Class::Overflow,
        K::Index => Class::Index,
        K::Divisor => Class::Divisor,
        K::Operand => Class::Operand,
        K::Missing => Class::Missing,
        K::Position => Class::Position,
    }
}

pub fn prim_to(p: &Prim) -> first::prim::Prim {
    use first::prim::{Cmp as C, Prim as P, Rounding as R};
    match p {
        Prim::Reshape => P::Reshape,
        Prim::Transpose { perm } => P::Transpose { perm: perm.clone() },
        Prim::Slice { axis, start } => P::Slice { axis: *axis, start: *start },
        Prim::Concat { axis } => P::Concat { axis: *axis },
        Prim::Broadcast => P::Broadcast,
        Prim::Iota { axis, start, step } => P::Iota { axis: *axis, start: *start, step: *step },
        Prim::Gather { axis, batch_dims } => P::Gather { axis: *axis, batch_dims: *batch_dims },
        Prim::Cast => P::Cast,
        Prim::Add => P::Add,
        Prim::Sub => P::Sub,
        Prim::Mul => P::Mul,
        Prim::MatMul => P::MatMul,
        Prim::ReduceSum { axis } => P::ReduceSum { axis: *axis },
        Prim::ReduceMax { axis } => P::ReduceMax { axis: *axis },
        Prim::Div { rule } => P::Div {
            rule: match rule {
                ref2::Rounding::Floor => R::Floor,
                ref2::Rounding::HalfUp => R::HalfUp,
                ref2::Rounding::HalfAwayFromZero => R::HalfAwayFromZero,
            },
        },
        Prim::Clamp { lo, hi } => P::Clamp { lo: *lo, hi: *hi },
        Prim::Log2Floor => P::Log2Floor,
        Prim::IntExp => P::IntExp,
        Prim::IntRsqrt => P::IntRsqrt,
        Prim::IntLn => P::IntLn,
        Prim::Compare { cmp } => P::Compare {
            cmp: match cmp {
                ref2::Cmp::Eq => C::Eq,
                ref2::Cmp::Ne => C::Ne,
                ref2::Cmp::Lt => C::Lt,
                ref2::Cmp::Le => C::Le,
                ref2::Cmp::Gt => C::Gt,
                ref2::Cmp::Ge => C::Ge,
            },
        },
        Prim::Select => P::Select,
        Prim::TopK { axis, k } => P::TopK { axis: *axis, k: *k },
        Prim::StateWrite { state } => P::StateWrite { state: *state },
        Prim::HistAppend { state } => P::HistAppend { state: *state },
    }
}

/// The outcome of a call: a value, an error of some class, or a panic (totality defect).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome<T> {
    Ok(T),
    Err(Class),
    Panic(String),
}

impl<T> Outcome<T> {
    pub fn is_ok(&self) -> bool {
        matches!(self, Outcome::Ok(_))
    }
    pub fn class(&self) -> Option<Class> {
        match self {
            Outcome::Err(c) => Some(*c),
            _ => None,
        }
    }
}

thread_local! {
    static SILENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Installs, once per test binary, a panic hook that stays quiet while a black-box call is being
/// caught on this thread and otherwise behaves as the default hook (so assertion messages of the
/// tests themselves are still printed).
fn install_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let default = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            if !SILENT.with(|s| s.get()) {
                default(info);
            }
        }));
    });
}

pub fn catch<T>(f: impl FnOnce() -> Result<T, first::error::TirError>) -> Outcome<T> {
    install_hook();
    SILENT.with(|s| s.set(true));
    let r = catch_unwind(AssertUnwindSafe(f));
    SILENT.with(|s| s.set(false));
    match r {
        Ok(Ok(v)) => Outcome::Ok(v),
        Ok(Err(e)) => Outcome::Err(class_from(e.kind)),
        Err(p) => Outcome::Panic(
            p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default(),
        ),
    }
}

pub fn mine<T>(r: ref2::Res<T>) -> Outcome<T> {
    match r {
        Ok(v) => Outcome::Ok(v),
        Err(e) => Outcome::Err(e.class),
    }
}

pub fn params_to(p: &Params) -> first::interp::MapParams {
    first::interp::MapParams { tensors: p.iter().map(|((j, l), t)| ((*j, l.map(|x| x as u16)), tensor_to(t))).collect() }
}

pub fn state_to(s: &RunState) -> first::interp::RunState {
    first::interp::RunState {
        pos: s.pos as u32,
        fixed: s.fixed.iter().map(|((j, l), t)| ((*j, l.map(|x| x as u16)), tensor_to(t))).collect(),
        hist: s
            .hist
            .iter()
            .map(|((j, l), rows)| ((*j, l.map(|x| x as u16)), rows.iter().map(tensor_to).collect::<VecDeque<_>>()))
            .collect(),
    }
}

pub fn state_from(s: &first::interp::RunState) -> RunState {
    RunState {
        pos: s.pos as u64,
        fixed: s.fixed.iter().map(|((j, l), t)| ((*j, l.map(|x| x as u32)), tensor_from(t))).collect(),
        hist: s.hist.iter().map(|((j, l), rows)| ((*j, l.map(|x| x as u32)), rows.iter().map(tensor_from).collect())).collect(),
    }
}

pub fn env_to(e: &ConeEnv, token_supplied: bool) -> first::interp::ConeEnv {
    first::interp::ConeEnv {
        token: if token_supplied { Some(e.token as u32) } else { None },
        pos: e.pos as u32,
        carry_in: e.carry_in.iter().map(|(k, t)| (*k, tensor_to(t))).collect(),
        fixed: e.fixed.iter().map(|(k, t)| (*k, tensor_to(t))).collect(),
        hist_prior: e.hist_prior.iter().map(|(k, rows)| (*k, rows.iter().map(tensor_to).collect())).collect(),
        supplied: e.supplied.iter().map(|(k, t)| (*k, tensor_to(t))).collect(),
    }
}

/// A step's result in this crate's terms: logits and `(slot, block, layer, node, value)` commits.
pub type Commits = Vec<(u64, u8, Option<u32>, u16, Tensor)>;

pub fn first_step(
    prog: &first::program::TirProgramV1,
    params: &first::interp::MapParams,
    state: &mut first::interp::RunState,
    token: u32,
) -> Outcome<(Tensor, Commits)> {
    catch(|| {
        let it = first::interp::Interpreter::new(prog)?;
        let o = it.step(params, state, token)?;
        Ok((
            tensor_from(&o.logits),
            o.commits.iter().map(|c| (c.slot as u64, c.block, c.layer.map(|l| l as u32), c.node, tensor_from(&c.value))).collect(),
        ))
    })
}

pub fn ref2_step(p: &ref2::Program, params: &Params, st: &RunState, token: u64) -> (Outcome<(Tensor, Commits)>, Option<RunState>) {
    match ref2::eval::step(p, params, st, token) {
        Ok((o, next)) => {
            (Outcome::Ok((o.logits, o.commits.into_iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value)).collect())), Some(next))
        }
        Err(e) => (Outcome::Err(e.class), None),
    }
}

pub fn first_decode(bytes: &[u8]) -> Outcome<first::program::TirProgramV1> {
    catch(|| first::program::TirProgramV1::decode_canonical(bytes))
}

pub fn first_cone(
    prog: &first::program::TirProgramV1,
    params: &first::interp::MapParams,
    block: u8,
    layer: Option<u32>,
    target: u16,
    env: &first::interp::ConeEnv,
) -> Outcome<Tensor> {
    catch(|| {
        let it = first::interp::Interpreter::new(prog)?;
        it.eval_cone(block, layer.map(|l| l as u16), target, params, env).map(|t| tensor_from(&t))
    })
}

pub fn first_eval_primitive(p: &Prim, ins: &[Tensor], out_dtype: DType, out_shape: &[u64]) -> Outcome<Tensor> {
    let fp = prim_to(p);
    let fins: Vec<first::tensor::Tensor> = ins.iter().map(tensor_to).collect();
    let shape: Vec<usize> = out_shape.iter().map(|&d| d as usize).collect();
    catch(|| first::eval::eval_primitive(&fp, &fins, dtype_to(out_dtype), &shape).map(|t| tensor_from(&t)))
}

pub fn map_keys<K: Ord + Clone, V>(m: &BTreeMap<K, V>) -> Vec<K> {
    m.keys().cloned().collect()
}
