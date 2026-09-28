//! A small program builder for this crate's tests (not the first implementation's builder).

use crate::program::*;
use crate::types::{DType, Dim, TensorType};

pub fn fixed(dtype: DType, shape: &[u32]) -> TensorType {
    TensorType::fixed(dtype, shape)
}

pub fn ty(dtype: DType, shape: &[Dim]) -> TensorType {
    TensorType { dtype, shape: shape.to_vec() }
}

pub struct ProgBuilder {
    pub p: Program,
}

impl ProgBuilder {
    pub fn new(history_bound: u32, token_bound: u32) -> Self {
        ProgBuilder {
            p: Program {
                version: 1,
                prim_set_id: [0; 64],
                token_bound,
                history_bound,
                params: vec![],
                consts: vec![],
                states: vec![],
                blocks: vec![],
                schedule: Schedule { pre: 0, layers: vec![], post: 0 },
                logits: 0,
                logits_scheme_id: [0; 64],
            },
        }
    }

    pub fn param(&mut self, name: &str, dtype: DType, shape: &[u32], per_layer: bool) -> u16 {
        self.p.params.push(ParamDecl { name: name.into(), dtype, shape: shape.to_vec(), per_layer });
        (self.p.params.len() - 1) as u16
    }

    pub fn konst(&mut self, dtype: DType, shape: &[u32], values: &[i128]) -> u16 {
        let mut data = Vec::new();
        for &v in values {
            let u = v as u128;
            for i in 0..dtype.width() {
                data.push((u >> (8 * i)) as u8);
            }
        }
        let c = ConstDecl { dtype, shape: shape.to_vec(), data };
        if let Some(i) = self.p.consts.iter().position(|x| *x == c) {
            return i as u16;
        }
        self.p.consts.push(c);
        (self.p.consts.len() - 1) as u16
    }

    pub fn fixed_state(&mut self, name: &str, dtype: DType, shape: &[u32], lo: i64, hi: i64, per_layer: bool) -> u16 {
        self.p.states.push(StateDecl { name: name.into(), kind: StateKind::Fixed { lo, hi }, dtype, shape: shape.to_vec(), per_layer });
        (self.p.states.len() - 1) as u16
    }

    pub fn hist_state(&mut self, name: &str, dtype: DType, row: &[u32], window: u32, per_layer: bool) -> u16 {
        self.p.states.push(StateDecl { name: name.into(), kind: StateKind::Hist { window }, dtype, shape: row.to_vec(), per_layer });
        (self.p.states.len() - 1) as u16
    }

    pub fn block(&mut self, name: &str, carry_in: Vec<TensorType>) -> usize {
        self.p.blocks.push(Block { name: name.into(), carry_in, nodes: vec![], carry_out: vec![] });
        self.p.blocks.len() - 1
    }

    pub fn node(&mut self, b: usize, prim: Prim, inputs: &[Ref], out: TensorType, commit: bool) -> u16 {
        self.p.blocks[b].nodes.push(Node { prim, inputs: inputs.to_vec(), out, commit });
        (self.p.blocks[b].nodes.len() - 1) as u16
    }

    pub fn carry_out(&mut self, b: usize, nodes: &[u16]) {
        self.p.blocks[b].carry_out = nodes.to_vec();
    }

    pub fn schedule(&mut self, pre: usize, layers: &[usize], post: usize, logits: u16) {
        self.p.schedule = Schedule { pre: pre as u8, layers: layers.iter().map(|&l| l as u8).collect(), post: post as u8 };
        self.p.logits = logits;
    }

    pub fn finish(self) -> Program {
        self.p
    }
}
