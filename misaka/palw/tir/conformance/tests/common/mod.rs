//! The harness: a PALW-TIR program whose one layer block runs a segment once per position, over
//! params (seeded operands, often a table indexed by the position so one program carries many
//! cases), `Fixed` and `Hist` states and the token — validated, checked ADMISSIBLE by the range
//! analysis, and run by the reference evaluator.
#![allow(dead_code)]

use misaka_palw_tir::builder::{BlockBuilder, ProgramBuilder};
use misaka_palw_tir::interval::analyze_ranges;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS};
use misaka_palw_tir::{DType, Dim, Interpreter, MapParams, Ref, RunState, Tensor, TensorType};

pub use kaspa_consensus_core::palw_base0_a16::A16QuantParams;

pub struct Arg {
    pub name: &'static str,
    pub dtype: DType,
    pub shape: Vec<u32>,
    pub values: Vec<i128>,
}

pub fn arg(name: &'static str, dtype: DType, shape: &[u32], values: Vec<i128>) -> Arg {
    assert_eq!(values.len(), shape.iter().map(|d| *d as usize).product::<usize>(), "{name}: element count");
    Arg { name, dtype, shape: shape.to_vec(), values }
}

pub enum St {
    Fixed(&'static str, DType, Vec<u32>, i64, i64),
    Hist(&'static str, DType, Vec<u32>),
}

/// Run a one-layer program for `tokens.len()` positions; `build` gets the block builder, the param
/// refs, the state indices and the position (`Input(1)`; [`row`] indexes a position table with it).
/// Returns the observed node's value per position.
pub fn run(
    args: &[Arg],
    states: &[St],
    tokens: &[u32],
    token_bound: u32,
    build: impl FnOnce(&mut BlockBuilder<'_>, &[Ref], &[u16], Ref) -> Ref,
) -> Vec<Vec<i128>> {
    let mut pb = ProgramBuilder::new(token_bound, HISTORY_BOUND_V1_SMALL);
    let refs: Vec<Ref> = args.iter().map(|a| pb.param(a.name, a.dtype, &a.shape, false)).collect();
    let sts: Vec<u16> = states
        .iter()
        .map(|s| match s {
            St::Fixed(n, dt, sh, lo, hi) => pb.fixed_state(n, *dt, sh, *lo, *hi, true),
            St::Hist(n, dt, row) => pb.hist_state(n, *dt, row, HISTORY_BOUND_V1_SMALL, true),
        })
        .collect();
    let carry = TensorType::fixed(DType::I16, &[1]);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let c = b.iota(DType::I16, &[Dim::Fixed(1)], 0, 0, 0);
        b.finish(&[c])
    };
    let (layer, node) = {
        let mut b = pb.block("layer", vec![carry.clone()]);
        let r = build(&mut b, &refs, &sts, Ref::Input(INPUT_POS));
        let dt = b.ty(r).dtype;
        let r = if dt.committable() { r } else { b.clamp(r, i32::MIN as i64, i32::MAX as i64, DType::I32) };
        b.commit(r);
        let Ref::Node(i) = r else { panic!("the observed value is a node") };
        let c = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        (b.finish(&[c]), i)
    };
    let post = {
        let mut b = pb.block("post", vec![carry]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    let program = pb.finish(pre, vec![layer], post, 0);
    let interp = Interpreter::new(&program).unwrap_or_else(|e| panic!("the segment validates: {e}"));
    if let Err(e) = analyze_ranges(&program) {
        panic!("the segment is admissible under spec 04b §7 at full param ranges: {e}");
    }
    let mut mp = MapParams::default();
    for (j, a) in args.iter().enumerate() {
        let shape = a.shape.iter().map(|d| *d as usize).collect();
        mp.tensors.insert((j as u16, None), Tensor::new(a.dtype, shape, a.values.clone()).expect("a value of its dtype"));
    }
    let mut st = RunState::default();
    tokens
        .iter()
        .map(|t| {
            let step = interp.step(&mp, &mut st, *t).unwrap_or_else(|e| panic!("position {}: {e}", st.pos));
            step.commits.iter().find(|c| c.block == layer && c.node == node).expect("observed").value.data.clone()
        })
        .collect()
}

/// [`run`] over `positions` positions with token 0.
pub fn run_pos(
    args: &[Arg],
    states: &[St],
    positions: u32,
    build: impl FnOnce(&mut BlockBuilder<'_>, &[Ref], &[u16], Ref) -> Ref,
) -> Vec<Vec<i128>> {
    run(args, states, &vec![0; positions as usize], 1, build)
}

/// Row `pos` of a `[positions, n]` table param. The position is clamped to the table's rows first:
/// a clamp that never fires, which the range analysis needs to prove the gather in range.
pub fn row(b: &mut BlockBuilder<'_>, table: Ref, pos: Ref) -> Ref {
    let Dim::Fixed(rows) = b.shape(table)[0] else { panic!("a static table") };
    let p = b.clamp(pos, 0, rows as i64 - 1, DType::Idx);
    b.gather(table, p, 0, 0)
}

/// The narrowing whose three operands are rows `pos` of `[positions, n]` tables (or `[positions, 1]`).
pub fn narrowing_at(b: &mut BlockBuilder<'_>, m: Ref, s: Ref, z: Ref, pos: Ref) -> Narrowing {
    let (m, s, z) = (row(b, m, pos), row(b, s, pos), row(b, z, pos));
    Narrowing::new(m, s, Some(z))
}

/// Deterministic pseudo-random values (a 64-bit LCG).
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
    /// A value of `[lo, hi]`, one time in four one of the listed extremes (clamped into the range).
    pub fn edgy(&mut self, lo: i128, hi: i128, extremes: &[i128]) -> i128 {
        if self.next_u64() % 4 == 0 && !extremes.is_empty() {
            extremes[(self.next_u64() % extremes.len() as u64) as usize].clamp(lo, hi)
        } else {
            self.range(lo, hi)
        }
    }
    pub fn vec(&mut self, n: usize, lo: i128, hi: i128, extremes: &[i128]) -> Vec<i128> {
        (0..n).map(|_| self.edgy(lo, hi, extremes)).collect()
    }
}

/// The extremes every kernel domain is probed at.
pub const I8X: &[i128] = &[-128, -127, -1, 0, 1, 127];
pub const CODEX: &[i128] = &[-32767, -32766, -1, 0, 1, 32766, 32767];
pub const I32X: &[i128] = &[i32::MIN as i128, i32::MIN as i128 + 1, -1, 0, 1, i32::MAX as i128 - 1, i32::MAX as i128];
pub const I64X: &[i128] = &[i64::MIN as i128, i64::MIN as i128 + 1, -1, 0, 1, 1 << 30, i64::MAX as i128 - 1, i64::MAX as i128];

/// A live A16 triple from three `i128`s.
pub fn triple(m: i128, s: i128, z: i128) -> A16QuantParams {
    A16QuantParams { multiplier: m as i64, shift: s as u8, zero: z as i64 }
}

pub fn i32s(v: &[i128]) -> Vec<i32> {
    v.iter().map(|x| *x as i32).collect()
}
pub fn i8s(v: &[i128]) -> Vec<i8> {
    v.iter().map(|x| *x as i8).collect()
}
pub fn wide<T: Copy + Into<i128>>(v: &[T]) -> Vec<i128> {
    v.iter().map(|x| (*x).into()).collect()
}
