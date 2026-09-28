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
        K::Malformed => Class::Malformed,
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

pub fn env_to(e: &ConeEnv) -> first::interp::ConeEnv {
    first::interp::ConeEnv {
        token: e.token.map(|t| t as u32),
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

// ------------------------------------------------------------------ admission (black box)

use ref2::admit::{Admission, AdmitError, AdmitInputs, Ceilings, Cost, Leaf};

pub fn ceilings_from(c: &first::admit::TirCeilingsV1) -> Ceilings {
    Ceilings {
        max_tile_macs: c.max_tile_macs,
        max_tile_transcendentals: c.max_tile_transcendentals,
        max_tile_opened_bytes: c.max_tile_opened_bytes,
        max_tile_operands: c.max_tile_operands,
        max_position_macs: c.max_position_macs,
        max_position_transcendentals: c.max_position_transcendentals,
        max_state_bytes: c.max_state_bytes,
        max_step_leaves: c.max_step_leaves,
        max_checkpoint_interval: c.max_checkpoint_interval,
        max_cone_work: c.max_cone_work,
    }
}

pub fn ceilings_to(c: &Ceilings) -> first::admit::TirCeilingsV1 {
    first::admit::TirCeilingsV1 {
        max_tile_macs: c.max_tile_macs,
        max_tile_transcendentals: c.max_tile_transcendentals,
        max_tile_opened_bytes: c.max_tile_opened_bytes,
        max_tile_operands: c.max_tile_operands,
        max_position_macs: c.max_position_macs,
        max_position_transcendentals: c.max_position_transcendentals,
        max_state_bytes: c.max_state_bytes,
        max_step_leaves: c.max_step_leaves,
        max_checkpoint_interval: c.max_checkpoint_interval,
        max_cone_work: c.max_cone_work,
    }
}

pub fn inputs_to(i: &AdmitInputs) -> first::admit::TirAdmitInputsV1 {
    first::admit::TirAdmitInputsV1 { tile_len: i.tile_len, h_chunk: i.h_chunk, ceilings: ceilings_to(&i.ceilings) }
}

pub fn cost_from(c: &first::admit::CostV1) -> Cost {
    Cost {
        macs: c.macs,
        elementwise: c.elementwise,
        transcendentals: c.transcendentals,
        bytes_read: c.bytes_read,
        bytes_written: c.bytes_written,
    }
}

pub fn leaf_from(l: &first::admit::LeafV1) -> Leaf {
    use first::admit::LeafV1 as L;
    match *l {
        L::Commit(k) => Leaf::Commit(k),
        L::CarryIn(k) => Leaf::CarryIn(k),
        L::State(j) => Leaf::State(j),
        L::History(j) => Leaf::History(j),
        L::Param(j) => Leaf::Param(j),
        L::Const(j) => Leaf::Const(j),
        L::Input(j) => Leaf::Input(j),
    }
}

/// The outcome of an admission, reduced to comparable data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdmitOutcome {
    Admitted(Box<AdmittedView>),
    Program(Class),
    Exceeds(String, u64),
    Inputs,
    Panic(String),
}

/// Every derived quantity the first implementation's API exposes, in this crate's terms.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AdmittedView {
    pub intervals: Vec<Vec<(i128, i128)>>,
    pub node_costs: Vec<Vec<Cost>>,
    pub position: (Cost, u64, u64, u64, u64),
    /// (block, node) → (nodes, leaves, whole, tiles, tile, tile_opened, operands, h_reductions,
    /// chunk, chunk_opened); nodes and leaves sorted.
    pub cones: Vec<ConeView>,
    /// state → (closure sorted, groups, per_position, interval)
    pub states: Vec<(u16, Vec<u16>, u64, Cost, u32)>,
    pub checkpoint_interval: u32,
    pub cone_work: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConeView {
    pub block: u8,
    pub node: u16,
    pub nodes: Vec<u16>,
    pub leaves: Vec<Leaf>,
    pub whole: Cost,
    pub tiles: u64,
    pub tile: Cost,
    pub tile_opened_bytes: u64,
    pub operands: u64,
    pub h_reductions: Vec<u16>,
    pub chunk: Option<Cost>,
    pub chunk_opened_bytes: Option<u64>,
}

pub fn view_mine(a: &Admission) -> AdmittedView {
    AdmittedView {
        intervals: a.intervals.iter().map(|b| b.iter().map(|i| (i.lo, i.hi)).collect()).collect(),
        node_costs: a.node_costs.clone(),
        position: (
            a.position.cost,
            a.position.state_bytes,
            a.position.peak_live_bytes,
            a.position.commit_lanes,
            a.position.step_leaves,
        ),
        cones: a
            .cones
            .iter()
            .map(|c| {
                let mut nodes = c.nodes.clone();
                nodes.sort();
                let mut leaves = c.leaves.clone();
                leaves.sort();
                leaves.dedup();
                let mut hr = c.h_reductions.clone();
                hr.sort();
                ConeView {
                    block: c.block,
                    node: c.node,
                    nodes,
                    leaves,
                    whole: c.whole,
                    tiles: c.tiles,
                    tile: c.tile,
                    tile_opened_bytes: c.tile_opened_bytes,
                    operands: c.operands,
                    h_reductions: hr,
                    chunk: c.chunk,
                    chunk_opened_bytes: c.chunk_opened_bytes,
                }
            })
            .collect(),
        states: a
            .states
            .iter()
            .map(|s| {
                let mut cl = s.closure.clone();
                cl.sort();
                (s.state, cl, s.groups, s.per_position, s.interval)
            })
            .collect(),
        checkpoint_interval: a.checkpoint_interval,
        cone_work: a.cone_work,
    }
}

pub fn view_first(a: &first::admit::TirAdmissionV1) -> AdmittedView {
    AdmittedView {
        intervals: a.intervals.iter().map(|b| b.iter().map(|i| (i.lo, i.hi)).collect()).collect(),
        node_costs: a.node_costs.iter().map(|b| b.iter().map(cost_from).collect()).collect(),
        position: (
            cost_from(&a.position.cost),
            a.position.state_bytes,
            a.position.peak_live_bytes,
            a.position.commit_lanes,
            a.position.step_leaves,
        ),
        cones: a
            .cones
            .iter()
            .map(|c| {
                let mut nodes = c.nodes.clone();
                nodes.sort();
                let mut leaves: Vec<Leaf> = c.leaves.iter().map(leaf_from).collect();
                leaves.sort();
                leaves.dedup();
                let mut hr = c.h_reductions.clone();
                hr.sort();
                ConeView {
                    block: c.block,
                    node: c.node,
                    nodes,
                    leaves,
                    whole: cost_from(&c.whole),
                    tiles: c.tiles,
                    tile: cost_from(&c.tile),
                    tile_opened_bytes: c.tile_opened_bytes,
                    operands: c.operands,
                    h_reductions: hr,
                    chunk: c.chunk.as_ref().map(cost_from),
                    chunk_opened_bytes: c.chunk_opened_bytes,
                }
            })
            .collect(),
        states: a
            .states
            .iter()
            .map(|s| {
                let mut cl = s.closure.clone();
                cl.sort();
                (s.state, cl, s.groups, cost_from(&s.per_position), s.interval)
            })
            .collect(),
        checkpoint_interval: a.checkpoint_interval,
        cone_work: a.cone_work,
    }
}

pub fn admit_mine(bytes: &[u8], inputs: &AdmitInputs) -> AdmitOutcome {
    admit_mine_with(bytes, inputs, ref2::admit::Readings::default())
}

pub fn admit_mine_with(bytes: &[u8], inputs: &AdmitInputs, readings: ref2::admit::Readings) -> AdmitOutcome {
    match ref2::admit::admit_with(bytes, inputs, readings) {
        Ok(a) => AdmitOutcome::Admitted(Box::new(view_mine(&a))),
        Err(AdmitError::Program(e)) => AdmitOutcome::Program(e.class),
        Err(AdmitError::Exceeds { limit, value, .. }) => AdmitOutcome::Exceeds(limit.to_string(), value),
        Err(AdmitError::Inputs(_)) => AdmitOutcome::Inputs,
    }
}

pub fn admit_first(bytes: &[u8], inputs: &AdmitInputs) -> AdmitOutcome {
    let fi = inputs_to(inputs);
    let r = catch(|| Ok(first::admit::tir_admit_v1(bytes, &fi)));
    match r {
        Outcome::Ok(Ok(a)) => AdmitOutcome::Admitted(Box::new(view_first(&a))),
        Outcome::Ok(Err(first::admit::TirAdmitError::Program(e))) => AdmitOutcome::Program(class_from(e.kind)),
        Outcome::Ok(Err(first::admit::TirAdmitError::Exceeds { limit, value, .. })) => {
            AdmitOutcome::Exceeds(limit_name(limit).to_string(), value)
        }
        Outcome::Ok(Err(first::admit::TirAdmitError::Inputs(_))) => AdmitOutcome::Inputs,
        Outcome::Err(c) => AdmitOutcome::Program(c),
        Outcome::Panic(s) => AdmitOutcome::Panic(s),
    }
}

/// The first implementation's refusal names, mapped onto this crate's (04b names no limit strings;
/// this crate names each after its ceiling). A C_j of 0 is named once for MACs, transcendentals
/// and a zero interval cap alike, so it maps to a shared name on both sides.
pub fn limit_name(first_name: &str) -> &'static str {
    match first_name {
        "tile MACs" => "max_tile_macs",
        "tile transcendentals" => "max_tile_transcendentals",
        "tile opened bytes" => "max_tile_opened_bytes",
        "tile operands" => "max_tile_operands",
        "position MACs" => "max_position_macs",
        "position transcendentals" => "max_position_transcendentals",
        "state bytes" => "max_state_bytes",
        "step leaves per position" => "max_step_leaves",
        "cone work" => "max_cone_work",
        "one position's state replay (MACs or transcendentals)" => "state_replay",
        _ => "UNKNOWN LIMIT NAME",
    }
}
