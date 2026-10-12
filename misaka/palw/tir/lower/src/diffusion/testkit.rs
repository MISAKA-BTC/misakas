//! **Test harness for the lowerers**: a one-layer program whose layer block runs a lowerer over global params, is
//! validated, held to the range rules of spec 04b §7 at the params' FULL dtype ranges (a lowerer is admissible or
//! it is not), run once, and whose result node is returned. The library's `run_layer` (`tests/library.rs`) with the
//! params declared through a [`ParamSink`], so the lowerers' own declaration helpers are what the test uses.

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, Dim, Interpreter, Ref, RunState, Tensor, TensorType};

use super::sink::ParamSink;

/// A 64-bit LCG, so no test depends on an RNG crate's stream.
pub(crate) struct Lcg(pub u64);

impl Lcg {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        self.0 ^ (self.0 >> 29)
    }
    /// An integer in `[lo, hi]`.
    pub fn range(&mut self, lo: i128, hi: i128) -> i128 {
        lo + (self.next() as u128 % (hi - lo + 1) as u128) as i128
    }
    /// A float in `[-1, 1]`.
    pub fn unit(&mut self) -> f64 {
        (self.next() % 2_000_001) as f64 / 1_000_000.0 - 1.0
    }
}

/// Run `build` as the one layer of a program whose params `declare` declares, and return the node it returns
/// (narrowed to `i32` first if it is not committable). The program must validate and be admissible.
pub(crate) fn run_one_block<D>(
    declare: impl FnOnce(&mut ProgramBuilder, &mut ParamSink) -> D,
    build: impl FnOnce(&mut BlockBuilder<'_>, D) -> Ref,
) -> Tensor {
    run_steps(declare, build, 1).remove(0)
}

/// [`run_one_block`] over `n` positions (`Input(1)` is the position; a `Fixed` state the layer writes carries to
/// the next): the observed node's value at each.
pub(crate) fn run_steps<D>(
    declare: impl FnOnce(&mut ProgramBuilder, &mut ParamSink) -> D,
    build: impl FnOnce(&mut BlockBuilder<'_>, D) -> Ref,
    n: usize,
) -> Vec<Tensor> {
    run_multi(declare, |b, d| vec![build(b, d)], n).into_iter().map(|mut v| v.remove(0)).collect()
}

/// [`run_steps`] observing several nodes: `result[position][k]` is the `k`-th returned node's value.
pub(crate) fn run_multi<D>(
    declare: impl FnOnce(&mut ProgramBuilder, &mut ParamSink) -> D,
    build: impl FnOnce(&mut BlockBuilder<'_>, D) -> Vec<Ref>,
    n: usize,
) -> Vec<Vec<Tensor>> {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let mut sink = ParamSink::new();
    let declared = declare(&mut pb, &mut sink);
    let carry = TensorType::fixed(DType::I16, &[1]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let c = b.iota(DType::I16, &[Dim::Fixed(1)], 0, 0, 0);
        b.finish(&[c])
    };
    let (layer, nodes) = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let rs = build(&mut b, declared);
        let mut nodes = Vec::new();
        for r in rs {
            let dt = b.ty(r).dtype;
            let r = if dt.committable() { r } else { b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32) };
            b.commit(r);
            let Ref::Node(i) = r else { panic!("an observed value is a node") };
            nodes.push(i);
        }
        let c = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        (b.finish(&[c]), nodes)
    };
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    let names: Vec<String> = pb.params.iter().map(|p| p.name.clone()).collect();
    let program = pb.finish(pre, vec![layer], post, 0);
    let interp = Interpreter::new(&program).unwrap_or_else(|e| panic!("the program validates: {e}"));
    if let Err(e) = analyze_ranges(&program) {
        panic!("the program is admissible under 04b §7 at full param ranges: {e}");
    }
    let mp = sink.bind(names.iter().map(String::as_str));
    let mut st = RunState::default();
    (0..n)
        .map(|i| {
            let step = interp.step(&mp, &mut st, 0).unwrap_or_else(|e| panic!("step {i}: {e}"));
            nodes
                .iter()
                .map(|node| {
                    step.commits
                        .iter()
                        .find(|c| c.block == layer && c.node == *node)
                        .expect("an observed node is committed")
                        .value
                        .clone()
                })
                .collect()
        })
        .collect()
}
