//! **The range rules of spec 04b §7, checked against the reference evaluator.**
//!
//! Soundness: whenever a primitive's transfer function succeeds (its obligations hold), evaluating
//! it on ANY operands inside the operand intervals succeeds and lands inside the output interval —
//! checked on random intervals and random operands drawn from them, with the interval endpoints
//! planted. Programs: the five corpus test programs pass `analyze_ranges` (so no exact primitive of
//! theirs can overflow on any weights, token or position), every committed value of a hostile-weight
//! run lies inside its node's interval, and programs that CAN overflow are refused by name.

mod common;

use common::Lcg;
use common::models::*;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::eval::eval_primitive;
use misaka_palw_tir::interval::{Interval, analyze_ranges, transfer};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{Cmp, DType, Dim, Interpreter, MapParams, Prim, Ref, Rounding, Tensor, TensorType, TirErrorKind, TirProgramV1};

fn rand_interval(rng: &mut Lcg, d: DType) -> Interval {
    let (lo, hi) = (d.min_value().max(-(1i128 << 90)), d.max_value().min(1i128 << 90));
    let pick = |rng: &mut Lcg| match rng.next_u64() % 5 {
        0 => lo,
        1 => hi,
        2 => 0,
        _ => rng.range(lo, hi),
    };
    let (a, b) = (pick(rng), pick(rng));
    Interval::new(a.min(b), a.max(b))
}

fn rand_tensor(rng: &mut Lcg, d: DType, shape: &[usize], i: Interval) -> Tensor {
    let n: usize = shape.iter().product();
    let data = (0..n)
        .map(|k| match k % 4 {
            0 => i.lo,
            1 => i.hi,
            _ => rng.range(i.lo, i.hi),
        })
        .collect();
    Tensor::new(d, shape.to_vec(), data).unwrap()
}

fn fixed(d: DType, s: &[usize]) -> TensorType {
    TensorType::fixed(d, &s.iter().map(|x| *x as u32).collect::<Vec<_>>())
}

#[test]
fn every_transfer_function_is_sound_against_the_evaluator() {
    let empty = TirProgramV1::decode_canonical(&dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0.encode()).unwrap();
    let mut rng = Lcg(0x1a7e);
    let dtypes = [DType::I8, DType::I16, DType::I32, DType::I64, DType::I128];
    let mut checked = 0usize;
    for round in 0..4000u64 {
        let da = dtypes[(rng.next_u64() % 5) as usize];
        let db = dtypes[(rng.next_u64() % 5) as usize];
        let dout = dtypes[(rng.next_u64() % 5) as usize];
        let (ia, ib) = (rand_interval(&mut rng, da), rand_interval(&mut rng, db));
        let (prim, ins, shapes, out_shape): (Prim, Vec<Interval>, Vec<Vec<usize>>, Vec<usize>) = match round % 12 {
            0 => (Prim::Add, vec![ia, ib], vec![vec![3], vec![3]], vec![3]),
            1 => (Prim::Sub, vec![ia, ib], vec![vec![3], vec![1]], vec![3]),
            2 => (Prim::Mul, vec![ia, ib], vec![vec![2, 2], vec![2]], vec![2, 2]),
            3 => (Prim::MatMul, vec![ia, ib], vec![vec![2, 5], vec![5, 3]], vec![2, 3]),
            4 => (Prim::ReduceSum { axis: 1 }, vec![ia], vec![vec![2, 6]], vec![2, 1]),
            5 => {
                let d = Interval::new(ib.lo.max(1), ib.hi.max(1));
                let rule = Rounding::ALL[(round / 12 % 3) as usize];
                (Prim::Div { rule }, vec![ia, d], vec![vec![4], vec![4]], vec![4])
            }
            6 => (Prim::Log2Floor, vec![ia], vec![vec![5]], vec![5]),
            7 => (Prim::IntExp, vec![Interval::new(ia.lo.max(i64::MIN as i128), ia.hi.min(i64::MAX as i128))], vec![vec![5]], vec![5]),
            8 => {
                (Prim::IntRsqrt, vec![Interval::new(ia.lo.max(i64::MIN as i128), ia.hi.min(i64::MAX as i128))], vec![vec![5]], vec![5])
            }
            9 => (Prim::IntLn, vec![Interval::new(ia.lo.max(i64::MIN as i128), ia.hi.min(i64::MAX as i128))], vec![vec![5]], vec![5]),
            10 => (Prim::Select, vec![Interval::new(0, 1), ia, ib], vec![vec![4], vec![4], vec![4]], vec![4]),
            _ => (Prim::Compare { cmp: Cmp::ALL[(round % 6) as usize] }, vec![ia, ib], vec![vec![3], vec![3]], vec![3]),
        };
        let dts: Vec<DType> = match (&prim, ins.len()) {
            (Prim::IntExp | Prim::IntRsqrt | Prim::IntLn, _) => vec![DType::I64],
            (Prim::MatMul, _) => {
                vec![if da == DType::I128 { DType::I64 } else { da }, if db == DType::I128 { DType::I64 } else { db }]
            }
            (Prim::Select, _) => vec![DType::I8, da, db],
            (_, 1) => vec![da],
            _ => vec![da, db],
        };
        // Operand intervals must be inside their dtypes.
        let ins: Vec<Interval> = ins
            .iter()
            .zip(&dts)
            .map(|(i, d)| {
                let c = |v: i128| v.clamp(d.min_value(), d.max_value());
                Interval::new(c(i.lo), c(i.hi))
            })
            .collect();
        let out_dt = if matches!(prim, Prim::Compare { .. }) { DType::I8 } else { dout };
        let tys: Vec<TensorType> = shapes.iter().zip(&dts).map(|(s, d)| fixed(*d, s)).collect();
        let out = fixed(out_dt, &out_shape);
        let Ok(oi) = transfer(&prim, &ins, &tys, &out, None, &empty) else { continue };
        for _ in 0..3 {
            let operands: Vec<Tensor> =
                ins.iter().zip(&dts).zip(&shapes).map(|((i, d), s)| rand_tensor(&mut rng, *d, s, *i)).collect();
            let r = eval_primitive(&prim, &operands, out_dt, &out_shape)
                .unwrap_or_else(|e| panic!("{} admitted {ins:?} → {oi:?} but failed: {e}", prim.name()));
            assert!(r.data.iter().all(|v| oi.contains(*v)), "{} value outside {oi:?}: {:?}", prim.name(), r.data);
            checked += 1;
        }
    }
    assert!(checked > 2000, "only {checked} admitted evaluations");
}

#[test]
fn the_corpus_test_programs_pass_the_range_rules_and_their_commits_stay_inside() {
    for (name, p) in [
        ("dense", dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0),
        ("sliding", dense(&[3, HISTORY_BOUND_V1_SMALL, 3]).0),
        ("gdn", gdn_program(true).0),
        ("mamba2", mamba2_program().0),
        ("moe", moe_program().0),
    ] {
        let ranges = analyze_ranges(&p).unwrap_or_else(|e| panic!("{name}: {e}"));
        let interp = Interpreter::new(&p).unwrap();
        for seed in 0..3u64 {
            let mut rng = Lcg(seed + 99);
            let mut params = MapParams::default();
            for (j, d) in p.params.iter().enumerate() {
                let ls: Vec<Option<u16>> =
                    if d.per_layer { (0..p.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
                for l in ls {
                    let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                    params.tensors.insert((j as u16, l), rand_tensor(&mut rng, d.dtype, &shape, Interval::of(d.dtype)));
                }
            }
            let outs = interp.run(&params, &[0, 1, 2, 3]).unwrap_or_else(|e| panic!("{name}: admitted but failed: {e}"));
            for o in &outs {
                for c in &o.commits {
                    let iv = ranges[c.block as usize][c.node as usize];
                    assert!(c.value.data.iter().all(|v| iv.contains(*v)), "{name}: node {} outside {iv:?}", c.node);
                }
            }
        }
    }
}

/// Programs that can overflow on some weights are refused, by the obligation that fails.
#[test]
fn programs_that_can_overflow_are_refused() {
    // One pre block computing `f(params)` into an i32 carry, a trivial post.
    fn one(build: impl FnOnce(&mut misaka_palw_tir::builder::BlockBuilder<'_>) -> Ref) -> TirProgramV1 {
        let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
        let pre = {
            let mut b = pb.block("pre", vec![]);
            let r = build(&mut b);
            b.finish(&[r])
        };
        let carry = pb.blocks[0].nodes.last().unwrap().out.clone();
        let post = {
            let mut b = pb.block("post", vec![carry.clone()]);
            let l = b.reshape(Ref::CarryIn(0), &carry.shape);
            b.commit(l);
            b.finish(&[])
        };
        pb.finish(pre, vec![], post, 0)
    }
    // i16·i16 over 4 terms into i32: 4·2^30 = 2^32 > i32::MAX.
    let p = one(|b| {
        let a = b.pb.param("a", DType::I16, &[1, 4], false);
        let w = b.pb.param("w", DType::I16, &[4, 1], false);
        b.matmul(a, w, DType::I32)
    });
    assert_eq!(analyze_ranges(&p).unwrap_err().kind, TirErrorKind::Overflow);
    // The same into i64 is fine.
    let p = one(|b| {
        let a = b.pb.param("a", DType::I16, &[1, 4], false);
        let w = b.pb.param("w", DType::I16, &[4, 1], false);
        let m = b.matmul(a, w, DType::I64);
        b.clamp(m, i32::MIN as i64, i32::MAX as i64, DType::I32)
    });
    assert!(analyze_ranges(&p).is_ok());
    // A gather by an untrusted index param.
    let p = one(|b| {
        let t = b.pb.param("t", DType::I32, &[8], false);
        let i = b.pb.param("i", DType::Idx, &[2], false);
        b.gather(t, i, 0, 0)
    });
    assert_eq!(analyze_ranges(&p).unwrap_err().kind, TirErrorKind::Index);
    // ... made safe by a clamp.
    let p = one(|b| {
        let t = b.pb.param("t", DType::I32, &[8], false);
        let i = b.pb.param("i", DType::Idx, &[2], false);
        let i = b.clamp(i, 0, 7, DType::Idx);
        b.gather(t, i, 0, 0)
    });
    assert!(analyze_ranges(&p).is_ok());
    // A division by an untrusted divisor (which may be 0).
    let p = one(|b| {
        let x = b.pb.param("x", DType::I32, &[2], false);
        let d = b.pb.param("d", DType::I32, &[2], false);
        b.div(x, d, Rounding::Floor, DType::I32)
    });
    assert_eq!(analyze_ranges(&p).unwrap_err().kind, TirErrorKind::Divisor);
    // The legacy rope_partial pattern: two i32·i32 products summed in i64.
    let p = one(|b| {
        let x = b.pb.param("x", DType::I32, &[2], false);
        let c = b.pb.param("c", DType::I32, &[2], false);
        let ac = b.mul(x, c, DType::I64);
        let s = b.add(ac, ac, DType::I64);
        b.clamp(s, i32::MIN as i64, i32::MAX as i64, DType::I32)
    });
    assert_eq!(analyze_ranges(&p).unwrap_err().kind, TirErrorKind::Overflow);
    // A history reduction's worst case is the WINDOW, not the current H.
    let mut pb = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let hs = pb.hist_state("h", DType::I16, &[1], HISTORY_BOUND_V1_SMALL, false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let t = b.cast(Ref::Input(0), DType::I16);
        let t = b.reshape_fixed(t, &[1]);
        let t = b.commit(t);
        let w = b.hist_append(hs, t);
        let s = b.reduce_sum(w, 0, DType::I32); // rows in [0, token_bound): 2^18 · 3 fits i32
        b.finish(&[s])
    };
    let carry = pb.blocks[0].nodes.last().unwrap().out.clone();
    let post = {
        let mut b = pb.block("post", vec![carry.clone()]);
        let l = b.reshape(Ref::CarryIn(0), &carry.shape);
        b.commit(l);
        b.finish(&[])
    };
    let p = pb.finish(pre, vec![], post, 0);
    let _ = Dim::H;
    // The token is bounded by token_bound (4), so the history rows are in [0, 3]: 2^18 · 3 fits.
    assert!(analyze_ranges(&p).is_ok());
}
