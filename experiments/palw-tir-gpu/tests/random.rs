//! **Randomized conformance: every primitive, random typed operands, the rails and the ties.**
//!
//! Each case is one primitive application whose operands are params of a one-node program, run by
//! the reference evaluator, the CPU executor and — under the CPU executor's refined plan — the
//! device (`common::one_node`). Values are drawn to hit what can go wrong on a device: the dtype
//! rails (`MIN`, `MAX`, their neighbours), powers of two and their neighbours (the rounding shifts'
//! ties, the `Log2Floor` edges), exact halves of every divisor, `TopK` rows full of ties, gather
//! indices one past their axis, sums whose positive terms overflow although the total fits, and
//! contractions long enough to cross every `i32` chunk boundary. Every case the device runs must
//! equal the CPU executor (and the reference) byte for byte, or fail with the same class; a case it
//! does not run is counted by reason.

mod common;

use misaka_palw_tir::{Cmp, DType, Dim, Prim, Rounding, Tensor, TensorType};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;

use common::device;
use common::one_node::{NodeCase, Tally, run_case_forms};

struct Gen(ChaCha8Rng);

const PARAM_DTYPES: [DType; 5] = [DType::I8, DType::I16, DType::I32, DType::I64, DType::Idx];
const OUT_DTYPES: [DType; 6] = [DType::I8, DType::I16, DType::I32, DType::I64, DType::I128, DType::Idx];

impl Gen {
    fn new(seed: u64) -> Self {
        Gen(ChaCha8Rng::seed_from_u64(seed))
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.0.gen_range(0..xs.len())]
    }
    fn chance(&mut self, p: f64) -> bool {
        self.0.gen_bool(p)
    }
    fn range(&mut self, lo: i128, hi: i128) -> i128 {
        if lo >= hi {
            return lo;
        }
        let span = (hi - lo) as u128;
        let r = ((self.0.r#gen::<u64>() as u128) << 64 | self.0.r#gen::<u64>() as u128) % (span + 1);
        lo + r as i128
    }
    /// A value of `d`: the rails, powers of two ± 1, small values, or uniform — within `[lo, hi]`.
    fn value_in(&mut self, d: DType, lo: i128, hi: i128) -> i128 {
        let (lo, hi) = (lo.max(d.min_value()), hi.min(d.max_value()));
        let v = match self.0.gen_range(0..8) {
            0 => self.pick(&[lo, hi, lo + 1, hi - 1, 0, 1, -1]),
            1 | 2 => {
                let b = self.0.gen_range(0..127u32);
                let s = self.pick(&[-1i128, 0, 1]);
                let p = 1i128.checked_shl(b).unwrap_or(0).saturating_add(s);
                if self.chance(0.5) { p } else { -p }
            }
            3 | 4 => self.range(-200, 200),
            _ => self.range(lo, hi),
        };
        v.clamp(lo, hi)
    }
    fn value(&mut self, d: DType) -> i128 {
        self.value_in(d, d.min_value(), d.max_value())
    }
    /// Values of `d` in a narrower band (so that some cases fit and run unchecked).
    fn moderate(&mut self, d: DType) -> i128 {
        let w = self.pick(&[7u32, 15, 20, 31, 40]);
        self.value_in(d, -(1i128 << w), (1i128 << w) - 1)
    }
    fn shape(&mut self, rank: usize, max_dim: usize) -> Vec<usize> {
        (0..rank).map(|_| self.0.gen_range(1..=max_dim)).collect()
    }
    fn tensor_with(&mut self, d: DType, shape: &[usize], f: &mut dyn FnMut(&mut Gen) -> i128) -> Tensor {
        let n: usize = shape.iter().product();
        let data = (0..n).map(|_| f(self).clamp(d.min_value(), d.max_value())).collect();
        Tensor { dtype: d, shape: shape.to_vec(), data }
    }
    fn tensor(&mut self, d: DType, shape: &[usize], moderate: bool) -> Tensor {
        self.tensor_with(d, shape, &mut |g| if moderate { g.moderate(d) } else { g.value(d) })
    }
    /// A shape that broadcasts to `out`: some dims 1, some leading dims dropped.
    fn bcast_from(&mut self, out: &[usize]) -> Vec<usize> {
        let drop = if out.is_empty() { 0 } else { self.0.gen_range(0..=out.len()) * usize::from(self.chance(0.3)) };
        out[drop.min(out.len())..].iter().map(|d| if self.chance(0.25) { 1 } else { *d }).collect()
    }
}

fn fixed(shape: &[usize]) -> Vec<Dim> {
    shape.iter().map(|d| Dim::Fixed(*d as u32)).collect()
}

fn case(prim: Prim, inputs: Vec<Tensor>, dtype: DType, shape: &[usize]) -> NodeCase {
    NodeCase { prim, inputs, out: TensorType::new(dtype, fixed(shape)) }
}

fn elementwise_case(g: &mut Gen) -> NodeCase {
    let rank = g.0.gen_range(0..=4);
    let out = g.shape(rank, 4);
    let prim = match g.0.gen_range(0..8) {
        0 => Prim::Add,
        1 => Prim::Sub,
        2 => Prim::Mul,
        3 | 4 => Prim::Div { rule: g.pick(&[Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero]) },
        5 => Prim::Compare { cmp: g.pick(&[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge]) },
        _ => Prim::Select,
    };
    let moderate = g.chance(0.6);
    let (sa, sb) = (g.bcast_from(&out), g.bcast_from(&out));
    let (da, db) = (g.pick(&PARAM_DTYPES), g.pick(&PARAM_DTYPES));
    let mut a = g.tensor(da, &sa, moderate);
    let mut b = g.tensor(db, &sb, moderate);
    let out_dtype = match prim {
        Prim::Compare { .. } => DType::I8,
        _ => g.pick(&OUT_DTYPES),
    };
    if let Prim::Div { .. } = prim {
        // Divisors: mostly ≥ 1 (powers of two — the shifts — and odd values, whose halves are the
        // ties), now and then one below 1.
        let mut bad = g.chance(0.15);
        b = g.tensor_with(db, &sb, &mut |g| {
            if bad && g.chance(0.3) {
                bad = false;
                return g.pick(&[0, -1, -7]);
            }
            match g.0.gen_range(0..4) {
                0 => 1i128 << g.0.gen_range(0..40u32),
                1 => g.range(1, 9),
                2 => g.value_in(db, 1, db.max_value()),
                _ => g.range(1, 1 << 20) * 2 + 1,
            }
        });
        // Exact halves: x = q·d ± d/2 for even d.
        if g.chance(0.3) && b.data.len() == 1 && b.data[0] % 2 == 0 {
            let d = b.data[0];
            a = g.tensor_with(da, &sa, &mut |g| g.range(-5, 5) * d + g.pick(&[d / 2, -(d / 2)]));
        }
    }
    if let Prim::Select = prim {
        let c_shape = g.bcast_from(&out);
        let dc = g.pick(&PARAM_DTYPES);
        let c = g.tensor_with(dc, &c_shape, &mut |g| if g.chance(0.7) { g.range(0, 1) } else { g.value(dc) });
        return case(prim, vec![c, a, b], out_dtype, &out);
    }
    case(prim, vec![a, b], out_dtype, &out)
}

fn unary_case(g: &mut Gen) -> NodeCase {
    let rank = g.0.gen_range(0..=3);
    let shape = g.shape(rank, 5);
    let dx = g.pick(&PARAM_DTYPES);
    let moderate = g.chance(0.5);
    match g.0.gen_range(0..7) {
        0 => case(Prim::Cast, vec![g.tensor(dx, &shape, moderate)], g.pick(&OUT_DTYPES), &shape),
        1 => {
            let od = g.pick(&OUT_DTYPES);
            let a = g.value_in(od, i64::MIN as i128, i64::MAX as i128);
            let b = g.value_in(od, i64::MIN as i128, i64::MAX as i128);
            let (lo, hi) = (a.min(b) as i64, a.max(b) as i64);
            case(Prim::Clamp { lo, hi }, vec![g.tensor(dx, &shape, moderate)], od, &shape)
        }
        2 => case(Prim::Log2Floor, vec![g.tensor(dx, &shape, moderate)], g.pick(&[DType::I8, DType::I32, DType::I64]), &shape),
        3 | 4 => {
            // IntExp's live domain is (−31·LN2_Q, 0]; around it, the bucket edges.
            let x = g.tensor_with(dx, &shape, &mut |g| match g.0.gen_range(0..3) {
                0 => -(g.range(0, 31)) * 11_629_080 + g.range(-2, 2),
                1 => g.range(-31 * 11_629_080, 10),
                _ => g.value(dx),
            });
            case(Prim::IntExp, vec![x], g.pick(&[DType::I32, DType::I64, DType::I16]), &shape)
        }
        5 => case(Prim::IntRsqrt, vec![g.tensor(dx, &shape, moderate)], g.pick(&[DType::I32, DType::I64, DType::I128]), &shape),
        _ => case(Prim::IntLn, vec![g.tensor(dx, &shape, moderate)], g.pick(&[DType::I32, DType::I64]), &shape),
    }
}

fn matmul_case(g: &mut Gen) -> NodeCase {
    let kind = g.0.gen_range(0..4);
    let (m, k, n) = match kind {
        // A decode projection: packed weight rows against one column (K a multiple of 16 half the
        // time: the vec4 kernel).
        0 => (g.0.gen_range(1..80), if g.chance(0.5) { 16 * g.0.gen_range(1..80) } else { 4 * g.0.gen_range(1..300) }, 1),
        // A batch of positions: the tiled kernel.
        1 => (g.0.gen_range(64..100), g.0.gen_range(1..90), g.0.gen_range(48..80)),
        // Long contractions: every i32 chunk boundary.
        2 => (g.0.gen_range(1..4), g.0.gen_range(500..2100), g.0.gen_range(1..3)),
        _ => (g.0.gen_range(1..9), g.0.gen_range(1..17), g.0.gen_range(1..9)),
    };
    let da = if kind == 0 { DType::I8 } else { g.pick(&[DType::I8, DType::I16, DType::I32, DType::I64]) };
    let db = g.pick(&[DType::I8, DType::I16, DType::I32, DType::I64]);
    let batch: Vec<usize> = if kind == 3 && g.chance(0.5) {
        let r = g.0.gen_range(1..=2);
        g.shape(r, 3)
    } else {
        vec![]
    };
    let a_batch: Vec<usize> = batch.iter().map(|d| if g.chance(0.3) { 1 } else { *d }).collect();
    let b_batch: Vec<usize> = batch.iter().map(|d| if g.chance(0.3) { 1 } else { *d }).collect();
    let mut sa = a_batch;
    sa.extend([m, k]);
    let mut sb = b_batch;
    sb.extend([k, n]);
    let mut out = batch.clone();
    out.extend([m, n]);
    let moderate = g.chance(0.6);
    let a = g.tensor(da, &sa, moderate && da != DType::I8);
    let b = g.tensor(db, &sb, moderate);
    // Sums whose positive terms overflow although the total fits: i32 out over balanced terms.
    let od = g.pick(&[DType::I64, DType::I64, DType::I32, DType::I128, DType::I16]);
    case(Prim::MatMul, vec![a, b], od, &out)
}

fn reduce_case(g: &mut Gen) -> NodeCase {
    let rank = g.0.gen_range(1..=3);
    let mut shape = g.shape(rank, 6);
    let axis = g.0.gen_range(0..rank);
    if g.chance(0.25) {
        shape[axis] = g.0.gen_range(500..2500);
    }
    let dx = g.pick(&PARAM_DTYPES);
    let moderate = g.chance(0.5);
    let x = g.tensor(dx, &shape, moderate);
    let mut out = shape.clone();
    out[axis] = 1;
    if g.chance(0.5) {
        case(Prim::ReduceMax { axis: axis as u8 }, vec![x], dx, &out)
    } else {
        case(Prim::ReduceSum { axis: axis as u8 }, vec![x], g.pick(&[DType::I64, DType::I32, DType::I128, DType::I16]), &out)
    }
}

fn gather_case(g: &mut Gen) -> NodeCase {
    let rd = g.0.gen_range(1..=3);
    let dshape = g.shape(rd, 5);
    let axis = g.0.gen_range(0..rd);
    let b = g.0.gen_range(0..=axis);
    let extra = g.0.gen_range(0..=2usize);
    let mut ishape: Vec<usize> = dshape[..b].to_vec();
    ishape.extend(g.shape(extra, 4));
    let out_rank = axis + (ishape.len() - b) + (rd - axis - 1);
    if out_rank > 4 || ishape.len() > 4 {
        return gather_case(g);
    }
    let dd = g.pick(&PARAM_DTYPES);
    let data = g.tensor(dd, &dshape, false);
    let di = g.pick(&PARAM_DTYPES);
    let n = dshape[axis] as i128;
    let wild = g.chance(0.15);
    let idx = g.tensor_with(di, &ishape, &mut |g| if wild && g.chance(0.2) { g.pick(&[n, -1, n + 5]) } else { g.range(0, n - 1) });
    let mut out = dshape[..axis].to_vec();
    out.extend_from_slice(&ishape[b..]);
    out.extend_from_slice(&dshape[axis + 1..]);
    case(Prim::Gather { axis: axis as u8, batch_dims: b as u8 }, vec![data, idx], dd, &out)
}

fn topk_case(g: &mut Gen) -> NodeCase {
    let rank = g.0.gen_range(1..=3);
    let mut shape = g.shape(rank, 6);
    let axis = g.0.gen_range(0..rank);
    if g.chance(0.3) {
        shape[axis] = g.0.gen_range(16..300);
    }
    let n = shape[axis];
    let k = g.0.gen_range(1..=n) as u32;
    let dx = g.pick(&PARAM_DTYPES);
    // Ties: values from a tiny band most of the time.
    let ties = g.chance(0.6);
    let x = g.tensor_with(dx, &shape, &mut |g| if ties { g.range(-2, 2) } else { g.value(dx) });
    let mut out = shape.clone();
    out[axis] = k as usize;
    case(Prim::TopK { axis: axis as u8, k }, vec![x], DType::Idx, &out)
}

fn structure_case(g: &mut Gen) -> NodeCase {
    let rank = g.0.gen_range(1..=4);
    let shape = g.shape(rank, 4);
    let d = g.pick(&PARAM_DTYPES);
    let x = g.tensor(d, &shape, false);
    match g.0.gen_range(0..6) {
        0 => {
            let n: usize = shape.iter().product();
            let to = if n.is_multiple_of(2) { vec![2, n / 2] } else { vec![n] };
            case(Prim::Reshape, vec![x], d, &to)
        }
        1 => {
            let mut perm: Vec<u8> = (0..rank as u8).collect();
            for i in (1..perm.len()).rev() {
                let j = g.0.gen_range(0..=i);
                perm.swap(i, j);
            }
            let out: Vec<usize> = perm.iter().map(|p| shape[*p as usize]).collect();
            case(Prim::Transpose { perm }, vec![x], d, &out)
        }
        2 => {
            let axis = g.0.gen_range(0..rank);
            let start = g.0.gen_range(0..shape[axis]);
            let len = g.0.gen_range(1..=shape[axis] - start);
            let mut out = shape.clone();
            out[axis] = len;
            case(Prim::Slice { axis: axis as u8, start: start as u32 }, vec![x], d, &out)
        }
        3 => {
            let axis = g.0.gen_range(0..rank);
            let parts = g.0.gen_range(2..=4);
            let mut ins = vec![x];
            let mut total = shape[axis];
            for _ in 1..parts {
                let mut s = shape.clone();
                s[axis] = g.0.gen_range(1..4);
                total += s[axis];
                ins.push(g.tensor(d, &s, false));
            }
            let mut out = shape.clone();
            out[axis] = total;
            case(Prim::Concat { axis: axis as u8 }, ins, d, &out)
        }
        4 => {
            let from = g.bcast_from(&shape);
            let x = g.tensor(d, &from, false);
            case(Prim::Broadcast, vec![x], d, &shape)
        }
        _ => {
            let axis = g.0.gen_range(0..rank);
            let od = g.pick(&OUT_DTYPES);
            let start = g.value_in(DType::I64, i64::MIN as i128, i64::MAX as i128) as i64;
            let step = g.pick(&[0i64, 1, -1, 3, 1 << 20, i64::MAX / 4]);
            case(Prim::Iota { axis: axis as u8, start, step }, vec![], od, &shape)
        }
    }
}

fn run_family(name: &str, seed: u64, n: usize, gen_case: fn(&mut Gen) -> NodeCase) -> Tally {
    let Some(dev) = device() else { return Tally::default() };
    let mut g = Gen::new(seed);
    let mut tally = Tally::default();
    for i in 0..n {
        let c = gen_case(&mut g);
        // Every operand in its param form (packed) or its computed form (lanes), at random.
        let forms = g.0.r#gen::<u32>();
        let r = run_case_forms(&dev, &c, forms);
        if let Some(r) = &r {
            r.assert_agree(&format!(
                "{name} case {i}: {:?} out {:?} ins {:?}",
                c.prim,
                c.out,
                c.inputs.iter().map(|t| (t.dtype, &t.shape)).collect::<Vec<_>>()
            ));
        }
        tally.add(&r);
    }
    eprintln!("{name}: {n} cases, {tally:?}; {} pipelines compiled", dev.pipeline_count());
    tally
}

#[test]
fn random_elementwise_binary_and_select_cases_equal_the_cpu_executor() {
    let t = run_family("elementwise", 1, 1500, elementwise_case);
    assert!(t.on_device == 0 || t.on_device > 500);
}

#[test]
fn random_unary_and_transcendental_cases_equal_the_cpu_executor() {
    run_family("unary", 2, 1200, unary_case);
}

#[test]
fn random_matmul_cases_equal_the_cpu_executor() {
    run_family("matmul", 3, 500, matmul_case);
}

#[test]
fn random_reductions_equal_the_cpu_executor() {
    run_family("reduce", 4, 600, reduce_case);
}

#[test]
fn random_gathers_equal_the_cpu_executor() {
    run_family("gather", 5, 600, gather_case);
}

#[test]
fn random_topk_rows_with_ties_equal_the_cpu_executor() {
    run_family("topk", 6, 500, topk_case);
}

#[test]
fn random_structure_and_iota_cases_equal_the_cpu_executor() {
    run_family("structure", 7, 800, structure_case);
}
