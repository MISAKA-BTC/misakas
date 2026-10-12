//! `TirProgramV1` — a class program as data (RFC §4, spec 04b §3).
//!
//! A program computes ONE position: the logits for `token` at `pos`, reading and writing its
//! states. Its encoding is Borsh in declaration order; [`TirProgramV1::decode_canonical`] refuses
//! anything that is not the unique encoding of a program in normal form, so the bytes ARE the
//! program and `graph_ir_root = H(bytes)` has no free choices (PALW-TIR-7).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::error::{TirErrorKind, TirResult, err};
use crate::prim::Prim;
use crate::types::{DType, TensorType};

/// The only version this crate reads.
pub const TIR_PROGRAM_VERSION_V1: u16 = 1;
/// The two history bounds a program may declare (ADR-0103/0116).
pub const HISTORY_BOUND_V1_SMALL: u32 = 1 << 18;
pub const HISTORY_BOUND_V1_HELD: u32 = 1 << 21;

// Size caps (RFC §5.4, proposed; Phase D sizes them). Structural caps are checked by
// `validate`; the byte cap by `decode_canonical`.
pub const MAX_PROGRAM_BYTES: usize = 256 * 1024;
pub const MAX_BLOCKS: usize = 16;
pub const MAX_NODES_PER_BLOCK: usize = 512;
pub const MAX_LAYERS: usize = 1024;
pub const MAX_NODE_INPUTS: usize = 8;
pub const MAX_CONST_BYTES: usize = 64 * 1024;
pub const MAX_PARAMS: usize = 4096;
pub const MAX_STATES: usize = 64;
pub const MAX_STATES_PER_LAYER: usize = 16;
pub const MAX_CARRY: usize = 8;
pub const MAX_NAME_BYTES: usize = 128;

/// Where a node's operand comes from. Tags are frozen: `Node 0, CarryIn 1, Param 2, Const 3,
/// State 4, Input 5`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
pub enum Ref {
    /// An earlier node of the same block (strictly backward).
    Node(u16),
    /// The block's `k`-th carry-in.
    CarryIn(u8),
    /// Param `j`, at the running layer if it is per-layer.
    Param(u16),
    /// Const `j`.
    Const(u16),
    /// `Fixed` state `j`'s value at the START of this position (at the running layer if per-layer).
    State(u16),
    /// Input 0 is `token`, input 1 is `pos`; both `idx` scalars.
    Input(u8),
}

pub const INPUT_TOKEN: u8 = 0;
pub const INPUT_POS: u8 = 1;

/// A tensor bound to the artifact inventory by `name` (per layer: `name` at each layer that runs a
/// block referencing it). Params take their dtype's full range: weights are not trusted.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ParamDecl {
    pub name: String,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub per_layer: bool,
}

/// A small inline constant, little-endian elements of `dtype`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ConstDecl {
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub data: Vec<u8>,
}

/// `Fixed` state carries a recurrence and saturates on write; `Hist` state is an append-only
/// history read through a window. Tags: `Fixed 0, Hist 1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum StateKind {
    Fixed { lo: i64, hi: i64 },
    Hist { window: u32 },
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct StateDecl {
    pub name: String,
    pub kind: StateKind,
    pub dtype: DType,
    /// The state's shape (`Fixed`) or one row's shape (`Hist`).
    pub shape: Vec<u32>,
    pub per_layer: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Node {
    pub prim: Prim,
    pub inputs: Vec<Ref>,
    pub out: TensorType,
    /// A commit point: its output is materialised as committed tiles in the step leg.
    pub commit: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Block {
    pub name: String,
    pub carry_in: Vec<TensorType>,
    pub nodes: Vec<Node>,
    /// Node indices whose outputs are the block's carry-out, in order.
    pub carry_out: Vec<u16>,
}

/// Which block runs where: `pre`, then `layers[0..L)`, then `post` (the layer loop is a schedule,
/// not a loop).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct Schedule {
    pub pre: u8,
    pub layers: Vec<u8>,
    pub post: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TirProgramV1 {
    pub version: u16,
    /// Must equal the network's hash of [`crate::prim::prim_set_descriptor_v1`]; checked by the
    /// caller, which owns the hash.
    pub prim_set_id: [u8; 64],
    /// Tokens are `idx` values in `[0, token_bound)`.
    pub token_bound: u32,
    /// Positions are in `[0, history_bound)`; one of [`HISTORY_BOUND_V1_SMALL`], [`HISTORY_BOUND_V1_HELD`].
    pub history_bound: u32,
    pub params: Vec<ParamDecl>,
    pub consts: Vec<ConstDecl>,
    pub states: Vec<StateDecl>,
    pub blocks: Vec<Block>,
    pub schedule: Schedule,
    /// The logits node, an index into the `post` block.
    pub logits: u16,
    pub logits_scheme_id: [u8; 64],
}

impl TirProgramV1 {
    /// The canonical bytes. Borsh is deterministic, so this is the only encoding of `self`.
    pub fn encode(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("encoding into a Vec cannot fail")
    }

    /// Decode bytes that must be the unique encoding of a program in normal form:
    /// strict Borsh (no trailing byte, every tag known, every bool 0/1, UTF-8 names), re-encoding
    /// byte-identical, within [`MAX_PROGRAM_BYTES`], and [`crate::validate::validate`] passing.
    pub fn decode_canonical(bytes: &[u8]) -> TirResult<Self> {
        if bytes.len() > MAX_PROGRAM_BYTES {
            return err(TirErrorKind::Encoding, format!("{} bytes exceed the {MAX_PROGRAM_BYTES}-byte cap", bytes.len()));
        }
        let program: TirProgramV1 =
            borsh::from_slice(bytes).map_err(|e| crate::error::TirError::new(TirErrorKind::Encoding, e.to_string()))?;
        if program.encode() != bytes {
            return err(TirErrorKind::Encoding, "re-encoding differs: not the canonical encoding");
        }
        crate::validate::validate(&program)?;
        Ok(program)
    }

    pub fn param_index(&self, name: &str) -> Option<u16> {
        self.params.iter().position(|p| p.name == name).map(|i| i as u16)
    }

    /// The block occurrences of one position in execution order: `(block, layer)`.
    pub fn occurrences(&self) -> Vec<(u8, Option<u16>)> {
        let mut out = Vec::with_capacity(self.schedule.layers.len() + 2);
        out.push((self.schedule.pre, None));
        for (l, b) in self.schedule.layers.iter().enumerate() {
            out.push((*b, Some(l as u16)));
        }
        out.push((self.schedule.post, None));
        out
    }

    /// The unrolled index of the first node of each occurrence; a node's commitment coordinate
    /// `node_slot` is this plus its index in the block (RFC §6).
    pub fn occurrence_slot_bases(&self) -> Vec<u32> {
        let mut base = 0u32;
        self.occurrences()
            .iter()
            .map(|(b, _)| {
                let this = base;
                base += self.blocks[*b as usize].nodes.len() as u32;
                this
            })
            .collect()
    }
}
