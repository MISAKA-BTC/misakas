//! The program `TirProgramV1` (04b §3) as plain data.

use crate::types::{DType, TensorType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Program {
    pub version: u16,
    pub prim_set_id: [u8; 64],
    pub token_bound: u32,
    pub history_bound: u32,
    pub params: Vec<ParamDecl>,
    pub consts: Vec<ConstDecl>,
    pub states: Vec<StateDecl>,
    pub blocks: Vec<Block>,
    pub schedule: Schedule,
    pub logits: u16,
    pub logits_scheme_id: [u8; 64],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParamDecl {
    pub name: String,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub per_layer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstDecl {
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub data: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateKind {
    Fixed { lo: i64, hi: i64 },
    Hist { window: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateDecl {
    pub name: String,
    pub kind: StateKind,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub per_layer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub name: String,
    pub carry_in: Vec<TensorType>,
    pub nodes: Vec<Node>,
    pub carry_out: Vec<u16>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    pub prim: Prim,
    pub inputs: Vec<Ref>,
    pub out: TensorType,
    pub commit: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ref {
    Node(u16),
    CarryIn(u8),
    Param(u16),
    Const(u16),
    State(u16),
    Input(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Schedule {
    pub pre: u8,
    pub layers: Vec<u8>,
    pub post: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rounding {
    Floor,
    HalfUp,
    HalfAwayFromZero,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

/// 04b §4.3: the twenty-five primitives with their attributes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prim {
    Reshape,
    Transpose { perm: Vec<u8> },
    Slice { axis: u8, start: u32 },
    Concat { axis: u8 },
    Broadcast,
    Iota { axis: u8, start: i64, step: i64 },
    Gather { axis: u8, batch_dims: u8 },
    Cast,
    Add,
    Sub,
    Mul,
    MatMul,
    ReduceSum { axis: u8 },
    ReduceMax { axis: u8 },
    Div { rule: Rounding },
    Clamp { lo: i64, hi: i64 },
    Log2Floor,
    IntExp,
    IntRsqrt,
    IntLn,
    Compare { cmp: Cmp },
    Select,
    TopK { axis: u8, k: u32 },
    StateWrite { state: u16 },
    HistAppend { state: u16 },
}

impl Prim {
    pub fn tag(&self) -> u8 {
        match self {
            Prim::Reshape => 0,
            Prim::Transpose { .. } => 1,
            Prim::Slice { .. } => 2,
            Prim::Concat { .. } => 3,
            Prim::Broadcast => 4,
            Prim::Iota { .. } => 5,
            Prim::Gather { .. } => 6,
            Prim::Cast => 7,
            Prim::Add => 8,
            Prim::Sub => 9,
            Prim::Mul => 10,
            Prim::MatMul => 11,
            Prim::ReduceSum { .. } => 12,
            Prim::ReduceMax { .. } => 13,
            Prim::Div { .. } => 14,
            Prim::Clamp { .. } => 15,
            Prim::Log2Floor => 16,
            Prim::IntExp => 17,
            Prim::IntRsqrt => 18,
            Prim::IntLn => 19,
            Prim::Compare { .. } => 20,
            Prim::Select => 21,
            Prim::TopK { .. } => 22,
            Prim::StateWrite { .. } => 23,
            Prim::HistAppend { .. } => 24,
        }
    }

    pub fn name(&self) -> &'static str {
        PRIM_NAMES[self.tag() as usize]
    }

    /// The arity of §6: `(min, max)` number of inputs.
    pub fn arity(&self) -> (usize, usize) {
        match self {
            Prim::Iota { .. } => (0, 0),
            Prim::Concat { .. } => (2, 8),
            Prim::Gather { .. }
            | Prim::Add
            | Prim::Sub
            | Prim::Mul
            | Prim::MatMul
            | Prim::Div { .. }
            | Prim::Compare { .. } => (2, 2),
            Prim::Select => (3, 3),
            _ => (1, 1),
        }
    }
}

/// The names in tag order, as in the prim-set descriptor of §6.0.
pub const PRIM_NAMES: [&str; 25] = [
    "Reshape",
    "Transpose",
    "Slice",
    "Concat",
    "Broadcast",
    "Iota",
    "Gather",
    "Cast",
    "Add",
    "Sub",
    "Mul",
    "MatMul",
    "ReduceSum",
    "ReduceMax",
    "Div",
    "Clamp",
    "Log2Floor",
    "IntExp",
    "IntRsqrt",
    "IntLn",
    "Compare",
    "Select",
    "TopK",
    "StateWrite",
    "HistAppend",
];

/// The prim-set descriptor of §6.0, rebuilt from the table rather than copied.
pub fn prim_set_descriptor() -> String {
    let prims: Vec<String> = PRIM_NAMES.iter().enumerate().map(|(i, n)| format!("{i}:{n}")).collect();
    format!("palw-tir/v1/spec=04b-tensor-ir/rev1/q=24/prims={}", prims.join(","))
}
