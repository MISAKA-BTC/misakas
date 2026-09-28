//! Shared helpers for the integration tests: evaluate a composite GRAPH (built with the builder,
//! validated, run by the interpreter's cone evaluator) on vectors supplied as params.
#![allow(dead_code)]

pub mod models;

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{ConeEnv, DType, Interpreter, MapParams, Ref, Tensor, TirResult};

pub struct Arg {
    pub name: &'static str,
    pub dtype: DType,
    pub values: Vec<i128>,
}

pub fn arg(name: &'static str, dtype: DType, values: Vec<i128>) -> Arg {
    Arg { name, dtype, values }
}

/// Build a one-block program whose pre block computes `build(args)` over rank-1 params of equal
/// length, and evaluate that node through `Interpreter::eval_cone`. The node's value is returned
/// whatever its dtype (it need not be committable: the cone evaluator returns any node).
pub fn eval_graph(args: &[Arg], build: impl FnOnce(&mut BlockBuilder<'_>, &[Ref]) -> Ref) -> TirResult<Tensor> {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let refs: Vec<Ref> = args.iter().map(|a| pb.param(a.name, a.dtype, &[a.values.len() as u32], false)).collect();
    let (root, pre) = {
        let mut b = pb.block("pre", vec![]);
        let r = build(&mut b, &refs);
        let shape = b.shape(r);
        // A committed, committable root so the node is live under normal form.
        let marker = b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let _ = shape;
        let Ref::Node(root) = r else { panic!("the composite's result is a node") };
        (root, b.finish(&[marker]))
    };
    let carry = pb.blocks[pre as usize].nodes.last().unwrap().out.clone();
    let post = {
        let mut b = pb.block("post", vec![carry.clone()]);
        let l = b.reshape(Ref::CarryIn(0), &carry.shape);
        b.commit(l);
        b.finish(&[])
    };
    let program = pb.finish(pre, vec![], post, 0);
    let interp = Interpreter::new(&program)?;
    let mut params = MapParams::default();
    for (j, a) in args.iter().enumerate() {
        params.tensors.insert((j as u16, None), Tensor::new(a.dtype, vec![a.values.len()], a.values.clone())?);
    }
    interp.eval_cone(pre, None, root, &params, &ConeEnv { token: Some(0), pos: 0, ..Default::default() })
}

/// Deterministic pseudo-random values (a 64-bit LCG), so no test depends on an RNG crate's stream.
pub struct Lcg(pub u64);

impl Lcg {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
    pub fn range(&mut self, lo: i128, hi: i128) -> i128 {
        let span = (hi - lo + 1) as u128;
        lo + ((self.next_u64() as u128 * 0x9E37_79B9 + self.next_u64() as u128) % span) as i128
    }
}
