//! Off-chain JSON view of the frozen v1 grammar. No serde representation enters consensus.
use crate::{LowerError, Result};
use misaka_palw_tir::prim::{Cmp, Rounding};
use misaka_palw_tir::program::{Ref, StateKind};
use misaka_palw_tir::types::Dim;
use misaka_palw_tir::{DType, Prim, TensorType, TirProgramV1};
use serde::{Deserialize, Serialize};

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn unhex(text: &str, max: usize) -> Result<Vec<u8>> {
    if text.len() % 2 != 0 || !text.is_ascii() || text.len() / 2 > max {
        return Err(LowerError::bad("FRONTEND_ENCODING: invalid or oversized hex"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|b| {
            let s = std::str::from_utf8(b).expect("ASCII checked");
            u8::from_str_radix(s, 16).map_err(|_| LowerError::bad("FRONTEND_ENCODING: invalid hex"))
        })
        .collect()
}

fn hash(text: &str) -> Result<[u8; 64]> {
    unhex(text, 64)?.try_into().map_err(|_| LowerError::bad("FRONTEND_ENCODING: identity must be 64 bytes"))
}

fn dtype(name: &str) -> Result<DType> {
    DType::from_name(name).ok_or_else(|| LowerError::not_lowerable(format!("KERNEL_EXTENSION_REQUIRED: dtype {name}")))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dimension {
    Fixed(u32),
    Dynamic(String),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Type {
    pub dtype: String,
    pub shape: Vec<Dimension>,
}

impl Type {
    fn compile(self) -> Result<TensorType> {
        Ok(TensorType {
            dtype: dtype(&self.dtype)?,
            shape: self
                .shape
                .into_iter()
                .map(|d| match d {
                    Dimension::Fixed(n) => Ok(Dim::Fixed(n)),
                    Dimension::Dynamic(s) if s == "H" => Ok(Dim::H),
                    Dimension::Dynamic(s) => Err(LowerError::not_lowerable(format!("KERNEL_EXTENSION_REQUIRED: dimension {s}"))),
                })
                .collect::<Result<_>>()?,
        })
    }
    fn of(t: &TensorType) -> Self {
        Self {
            dtype: t.dtype.name().into(),
            shape: t
                .shape
                .iter()
                .map(|d| match d {
                    Dim::Fixed(n) => Dimension::Fixed(*n),
                    Dim::H => Dimension::Dynamic("H".into()),
                })
                .collect(),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Reference {
    Node(u16),
    CarryIn(u8),
    Param(u16),
    Const(u16),
    State(u16),
    Input(u8),
}
impl Reference {
    fn compile(self) -> Ref {
        match self {
            Self::Node(n) => Ref::Node(n),
            Self::CarryIn(n) => Ref::CarryIn(n),
            Self::Param(n) => Ref::Param(n),
            Self::Const(n) => Ref::Const(n),
            Self::State(n) => Ref::State(n),
            Self::Input(n) => Ref::Input(n),
        }
    }
    fn of(r: Ref) -> Self {
        match r {
            Ref::Node(n) => Self::Node(n),
            Ref::CarryIn(n) => Self::CarryIn(n),
            Ref::Param(n) => Self::Param(n),
            Ref::Const(n) => Self::Const(n),
            Ref::State(n) => Self::State(n),
            Ref::Input(n) => Self::Input(n),
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Round {
    Floor,
    HalfUp,
    HalfAwayFromZero,
}
impl Round {
    pub fn compile(self) -> Rounding {
        match self {
            Self::Floor => Rounding::Floor,
            Self::HalfUp => Rounding::HalfUp,
            Self::HalfAwayFromZero => Rounding::HalfAwayFromZero,
        }
    }
    fn of(r: Rounding) -> Self {
        match r {
            Rounding::Floor => Self::Floor,
            Rounding::HalfUp => Self::HalfUp,
            Rounding::HalfAwayFromZero => Self::HalfAwayFromZero,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Compare {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}
impl Compare {
    fn compile(self) -> Cmp {
        match self {
            Self::Eq => Cmp::Eq,
            Self::Ne => Cmp::Ne,
            Self::Lt => Cmp::Lt,
            Self::Le => Cmp::Le,
            Self::Gt => Cmp::Gt,
            Self::Ge => Cmp::Ge,
        }
    }
    fn of(c: Cmp) -> Self {
        match c {
            Cmp::Eq => Self::Eq,
            Cmp::Ne => Self::Ne,
            Cmp::Lt => Self::Lt,
            Cmp::Le => Self::Le,
            Cmp::Gt => Self::Gt,
            Cmp::Ge => Self::Ge,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Operation {
    Reshape {},
    Transpose { perm: Vec<u8> },
    Slice { axis: u8, start: u32 },
    Concat { axis: u8 },
    Broadcast {},
    Iota { axis: u8, start: i64, step: i64 },
    Gather { axis: u8, batch_dims: u8 },
    Cast {},
    Add {},
    Sub {},
    Mul {},
    MatMul {},
    ReduceSum { axis: u8 },
    ReduceMax { axis: u8 },
    Div { rule: Round },
    Clamp { lo: i64, hi: i64 },
    Log2Floor {},
    IntExp {},
    IntRsqrt {},
    IntLn {},
    Compare { cmp: Compare },
    Select {},
    TopK { axis: u8, k: u32 },
    StateWrite { state: u16 },
    HistAppend { state: u16 },
}
impl Operation {
    pub fn compile(self) -> Prim {
        match self {
            Self::Reshape {} => Prim::Reshape,
            Self::Transpose { perm } => Prim::Transpose { perm },
            Self::Slice { axis, start } => Prim::Slice { axis, start },
            Self::Concat { axis } => Prim::Concat { axis },
            Self::Broadcast {} => Prim::Broadcast,
            Self::Iota { axis, start, step } => Prim::Iota { axis, start, step },
            Self::Gather { axis, batch_dims } => Prim::Gather { axis, batch_dims },
            Self::Cast {} => Prim::Cast,
            Self::Add {} => Prim::Add,
            Self::Sub {} => Prim::Sub,
            Self::Mul {} => Prim::Mul,
            Self::MatMul {} => Prim::MatMul,
            Self::ReduceSum { axis } => Prim::ReduceSum { axis },
            Self::ReduceMax { axis } => Prim::ReduceMax { axis },
            Self::Div { rule } => Prim::Div { rule: rule.compile() },
            Self::Clamp { lo, hi } => Prim::Clamp { lo, hi },
            Self::Log2Floor {} => Prim::Log2Floor,
            Self::IntExp {} => Prim::IntExp,
            Self::IntRsqrt {} => Prim::IntRsqrt,
            Self::IntLn {} => Prim::IntLn,
            Self::Compare { cmp } => Prim::Compare { cmp: cmp.compile() },
            Self::Select {} => Prim::Select,
            Self::TopK { axis, k } => Prim::TopK { axis, k },
            Self::StateWrite { state } => Prim::StateWrite { state },
            Self::HistAppend { state } => Prim::HistAppend { state },
        }
    }
    pub fn of(p: &Prim) -> Self {
        match p {
            Prim::Reshape => Self::Reshape {},
            Prim::Transpose { perm } => Self::Transpose { perm: perm.clone() },
            Prim::Slice { axis, start } => Self::Slice { axis: *axis, start: *start },
            Prim::Concat { axis } => Self::Concat { axis: *axis },
            Prim::Broadcast => Self::Broadcast {},
            Prim::Iota { axis, start, step } => Self::Iota { axis: *axis, start: *start, step: *step },
            Prim::Gather { axis, batch_dims } => Self::Gather { axis: *axis, batch_dims: *batch_dims },
            Prim::Cast => Self::Cast {},
            Prim::Add => Self::Add {},
            Prim::Sub => Self::Sub {},
            Prim::Mul => Self::Mul {},
            Prim::MatMul => Self::MatMul {},
            Prim::ReduceSum { axis } => Self::ReduceSum { axis: *axis },
            Prim::ReduceMax { axis } => Self::ReduceMax { axis: *axis },
            Prim::Div { rule } => Self::Div { rule: Round::of(*rule) },
            Prim::Clamp { lo, hi } => Self::Clamp { lo: *lo, hi: *hi },
            Prim::Log2Floor => Self::Log2Floor {},
            Prim::IntExp => Self::IntExp {},
            Prim::IntRsqrt => Self::IntRsqrt {},
            Prim::IntLn => Self::IntLn {},
            Prim::Compare { cmp } => Self::Compare { cmp: Compare::of(*cmp) },
            Prim::Select => Self::Select {},
            Prim::TopK { axis, k } => Self::TopK { axis: *axis, k: *k },
            Prim::StateWrite { state } => Self::StateWrite { state: *state },
            Prim::HistAppend { state } => Self::HistAppend { state: *state },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub name: String,
    pub dtype: String,
    pub shape: Vec<u32>,
    pub per_layer: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Constant {
    pub dtype: String,
    pub shape: Vec<u32>,
    pub data: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StateType {
    Fixed { lo: i64, hi: i64 },
    Hist { window: u32 },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub name: String,
    pub kind: StateType,
    pub dtype: String,
    pub shape: Vec<u32>,
    pub per_layer: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Instruction {
    pub prim: Operation,
    pub inputs: Vec<Reference>,
    pub out: Type,
    pub commit: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub name: String,
    pub carry_in: Vec<Type>,
    pub nodes: Vec<Instruction>,
    pub carry_out: Vec<u16>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Order {
    pub pre: u8,
    pub layers: Vec<u8>,
    pub post: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Program {
    pub version: u16,
    pub prim_set_id: String,
    pub token_bound: u32,
    pub history_bound: u32,
    pub params: Vec<Parameter>,
    pub consts: Vec<Constant>,
    pub states: Vec<State>,
    pub blocks: Vec<Region>,
    pub schedule: Order,
    pub logits: u16,
    pub logits_scheme_id: String,
}

impl Program {
    pub fn compile(self) -> Result<TirProgramV1> {
        use misaka_palw_tir::program::*;
        let constant_bytes = self.consts.iter().try_fold(0usize, |n, c| n.checked_add(c.data.len().div_ceil(2)));
        let names_ok = self.params.iter().all(|p| p.name.len() <= MAX_NAME_BYTES && p.shape.len() <= 4)
            && self.states.iter().all(|s| s.name.len() <= MAX_NAME_BYTES && s.shape.len() <= 4);
        if self.params.len() > MAX_PARAMS
            || self.states.len() > MAX_STATES
            || self.blocks.len() > MAX_BLOCKS
            || self.schedule.layers.len() > MAX_LAYERS
            || !names_ok
            || !constant_bytes.is_some_and(|n| n <= MAX_CONST_BYTES)
            || self.consts.len() > u16::MAX as usize
            || self.consts.iter().any(|c| c.shape.len() > 4)
            || self.blocks.iter().any(|b| {
                b.nodes.len() > MAX_NODES_PER_BLOCK
                    || b.name.len() > MAX_NAME_BYTES
                    || b.carry_in.len() > MAX_CARRY
                    || b.carry_out.len() > MAX_CARRY
                    || b.carry_in.iter().any(|t| t.shape.len() > 4)
                    || b.nodes.iter().any(|n| n.inputs.len() > MAX_NODE_INPUTS || n.out.shape.len() > 4)
            })
        {
            return Err(LowerError::bad("FRONTEND_EXPANSION_LIMIT: program structural limit"));
        }
        let p = TirProgramV1 {
            version: self.version,
            prim_set_id: hash(&self.prim_set_id)?,
            token_bound: self.token_bound,
            history_bound: self.history_bound,
            params: self
                .params
                .into_iter()
                .map(|p| Ok(ParamDecl { name: p.name, dtype: dtype(&p.dtype)?, shape: p.shape, per_layer: p.per_layer }))
                .collect::<Result<_>>()?,
            consts: self
                .consts
                .into_iter()
                .map(|c| Ok(ConstDecl { dtype: dtype(&c.dtype)?, shape: c.shape, data: unhex(&c.data, MAX_CONST_BYTES)? }))
                .collect::<Result<_>>()?,
            states: self
                .states
                .into_iter()
                .map(|s| {
                    Ok(StateDecl {
                        name: s.name,
                        kind: match s.kind {
                            StateType::Fixed { lo, hi } => StateKind::Fixed { lo, hi },
                            StateType::Hist { window } => StateKind::Hist { window },
                        },
                        dtype: dtype(&s.dtype)?,
                        shape: s.shape,
                        per_layer: s.per_layer,
                    })
                })
                .collect::<Result<_>>()?,
            blocks: self
                .blocks
                .into_iter()
                .map(|b| {
                    Ok(Block {
                        name: b.name,
                        carry_in: b.carry_in.into_iter().map(Type::compile).collect::<Result<_>>()?,
                        nodes: b
                            .nodes
                            .into_iter()
                            .map(|n| {
                                Ok(Node {
                                    prim: n.prim.compile(),
                                    inputs: n.inputs.into_iter().map(Reference::compile).collect(),
                                    out: n.out.compile()?,
                                    commit: n.commit,
                                })
                            })
                            .collect::<Result<_>>()?,
                        carry_out: b.carry_out,
                    })
                })
                .collect::<Result<_>>()?,
            schedule: Schedule { pre: self.schedule.pre, layers: self.schedule.layers, post: self.schedule.post },
            logits: self.logits,
            logits_scheme_id: hash(&self.logits_scheme_id)?,
        };
        if p.prim_set_id != misaka_palw_tir::prim::PRIM_SET_ID_V1 {
            return Err(LowerError::not_lowerable("KERNEL_EXTENSION_REQUIRED: primitive set"));
        }
        TirProgramV1::decode_canonical(&p.encode()).map_err(|e| LowerError::not_lowerable(format!("TIR_ADMISSION: {e}")))
    }

    /// Allows an independent compiler to publish the same bytes through a declarative template.
    pub fn of(p: &TirProgramV1) -> Self {
        Self {
            version: p.version,
            prim_set_id: hex(&p.prim_set_id),
            token_bound: p.token_bound,
            history_bound: p.history_bound,
            params: p
                .params
                .iter()
                .map(|p| Parameter {
                    name: p.name.clone(),
                    dtype: p.dtype.name().into(),
                    shape: p.shape.clone(),
                    per_layer: p.per_layer,
                })
                .collect(),
            consts: p
                .consts
                .iter()
                .map(|c| Constant { dtype: c.dtype.name().into(), shape: c.shape.clone(), data: hex(&c.data) })
                .collect(),
            states: p
                .states
                .iter()
                .map(|s| State {
                    name: s.name.clone(),
                    kind: match s.kind {
                        StateKind::Fixed { lo, hi } => StateType::Fixed { lo, hi },
                        StateKind::Hist { window } => StateType::Hist { window },
                    },
                    dtype: s.dtype.name().into(),
                    shape: s.shape.clone(),
                    per_layer: s.per_layer,
                })
                .collect(),
            blocks: p
                .blocks
                .iter()
                .map(|b| Region {
                    name: b.name.clone(),
                    carry_in: b.carry_in.iter().map(Type::of).collect(),
                    nodes: b
                        .nodes
                        .iter()
                        .map(|n| Instruction {
                            prim: Operation::of(&n.prim),
                            inputs: n.inputs.iter().copied().map(Reference::of).collect(),
                            out: Type::of(&n.out),
                            commit: n.commit,
                        })
                        .collect(),
                    carry_out: b.carry_out.clone(),
                })
                .collect(),
            schedule: Order { pre: p.schedule.pre, layers: p.schedule.layers.clone(), post: p.schedule.post },
            logits: p.logits,
            logits_scheme_id: hex(&p.logits_scheme_id),
        }
    }
}
