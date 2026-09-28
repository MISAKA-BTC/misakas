//! Targeted black-box probes of the places where 04b's text is silent or ambiguous. Each probe runs
//! the same program / input through this implementation and the first one and records what each
//! does; the observations are the evidence of `docs/design/palw/tir/ref2-findings.md`. A probe
//! asserts only what the text fixes; where the text is silent it prints both behaviours.

mod common;

use std::collections::BTreeMap;

use common::bridge::*;
use misaka_palw_tir_ref2 as ref2;
use ref2::build::{ProgBuilder, fixed, ty};
use ref2::codec::{decode_canonical, encode};
use ref2::eval::{ConeEnv, Params, RunState, eval_cone, initial_state};
use ref2::{Class, DType, Dim, Prim, Program, Ref, Rounding, Tensor, eval_primitive};

const HB: u32 = 1 << 18;

fn t(dtype: DType, shape: &[u64], data: &[i128]) -> Tensor {
    Tensor::new(dtype, shape.to_vec(), data.to_vec()).unwrap()
}

/// pre: s0 += token (saturating via StateWrite), carry = clamp(token + s0); post: logits.
fn base(post_writes_too: bool) -> Program {
    let mut b = ProgBuilder::new(HB, 16);
    let s0 = b.fixed_state("s0", DType::I32, &[2], -100, 100, false);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Input(0)], fixed(DType::I32, &[]), false);
    let n1 = b.node(pre, Prim::Broadcast, &[Ref::Node(n0)], fixed(DType::I32, &[2]), false);
    let n2 = b.node(pre, Prim::Add, &[Ref::Node(n1), Ref::State(s0)], fixed(DType::I32, &[2]), false);
    b.node(pre, Prim::StateWrite { state: s0 }, &[Ref::Node(n2)], fixed(DType::I32, &[2]), false);
    let n4 = b.node(pre, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::Node(n2)], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[n4]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let m0 = b.node(post, Prim::Add, &[Ref::CarryIn(0), Ref::State(s0)], fixed(DType::I64, &[2]), false);
    let m1 = b.node(post, Prim::Clamp { lo: -1_000_000, hi: 1_000_000 }, &[Ref::Node(m0)], fixed(DType::I32, &[2]), true);
    if post_writes_too {
        // A second StateWrite of the same global state, from post: NF-19 is per block.
        let k = b.konst(DType::I32, &[2], &[7, -7]);
        let m2 = b.node(post, Prim::Mul, &[Ref::Node(m1), Ref::Const(k)], fixed(DType::I64, &[2]), false);
        b.node(post, Prim::StateWrite { state: s0 }, &[Ref::Node(m2)], fixed(DType::I32, &[2]), false);
    }
    b.schedule(pre, &[], post, m1);
    b.finish()
}

fn both_decode(p: &Program) -> (Outcome<()>, Outcome<()>, Vec<u8>) {
    let bytes = encode(p);
    let a = mine(decode_canonical(&bytes)).map_unit();
    let f = first_decode(&bytes).map_unit();
    (a, f, bytes)
}

trait MapUnit {
    fn map_unit(self) -> Outcome<()>;
}
impl<T> MapUnit for Outcome<T> {
    fn map_unit(self) -> Outcome<()> {
        match self {
            Outcome::Ok(_) => Outcome::Ok(()),
            Outcome::Err(c) => Outcome::Err(c),
            Outcome::Panic(s) => Outcome::Panic(s),
        }
    }
}

/// Runs `tokens` through both, printing each step; returns (ref2 states, first states).
fn run_both(
    name: &str,
    p: &Program,
    params: &Params,
    tokens: &[u64],
) -> Vec<(Outcome<(Tensor, Commits)>, Outcome<(Tensor, Commits)>)> {
    let bytes = encode(p);
    let fp = match first_decode(&bytes) {
        Outcome::Ok(fp) => fp,
        o => panic!("{name}: first refuses: {o:?}"),
    };
    let fparams = params_to(params);
    let mut st = initial_state(p);
    let mut fst = first::interp::RunState::default();
    let mut out = Vec::new();
    for &tok in tokens {
        let (a, next) = ref2_step(p, params, &st, tok);
        let before = fst.clone();
        let f = first_step(&fp, &fparams, &mut fst, tok as u32);
        if !f.is_ok() && fst != before {
            println!("{name}: FIRST CHANGED ITS STATE ON A FAILED STEP");
        }
        println!("{name} tok {tok}: ref2 {:?}\n{:>w$}first {:?}", summary(&a), "", summary(&f), w = name.len() + 1);
        if let Some(n) = next {
            st = n;
        }
        let fs = state_from(&fst);
        if f.is_ok() && a.is_ok() && fs.fixed != st.fixed {
            println!("{name}: Fixed state differs after the step: ref2 {:?} first {:?}", st.fixed, fs.fixed);
        }
        if f.is_ok() && a.is_ok() && fs.hist != st.hist {
            println!("{name}: Hist rows differ after the step: ref2 {:?} first {:?}", st.hist, fs.hist);
        }
        out.push((a, f));
    }
    out
}

use misaka_palw_tir as first;

fn summary(o: &Outcome<(Tensor, Commits)>) -> String {
    match o {
        Outcome::Ok((l, _)) => format!("ok logits {:?}", l.data),
        Outcome::Err(c) => format!("err {}", c.name()),
        Outcome::Panic(s) => format!("PANIC {s}"),
    }
}

#[test]
fn p01_two_statewrites_of_one_global_state_in_one_step() {
    let p = base(true);
    let (a, f, _) = both_decode(&p);
    println!("decode: ref2 {a:?} first {f:?}");
    assert_eq!(a, Outcome::Ok(()));
    if f.is_ok() {
        let r = run_both("p01", &p, &Params::new(), &[3, 5, 1]);
        for (a, f) in r {
            assert_eq!(a.is_ok(), f.is_ok());
        }
    }
}

#[test]
fn p02_two_histappends_of_one_global_history_in_one_step() {
    for window in [4u32, 2, 1] {
        p02_with_window(window);
    }
}

fn p02_with_window(window: u32) {
    println!("p02 window {window}");
    let mut b = ProgBuilder::new(HB, 16);
    let h = b.hist_state("h", DType::I32, &[1], window, false);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Input(0)], fixed(DType::I32, &[]), false);
    let n1 = b.node(pre, Prim::Reshape, &[Ref::Node(n0)], fixed(DType::I32, &[1]), true);
    b.node(pre, Prim::HistAppend { state: h }, &[Ref::Node(n1)], ty(DType::I32, &[Dim::H, Dim::Fixed(1)]), false);
    b.carry_out(pre, &[n1]);
    let post = b.block("post", vec![fixed(DType::I32, &[1])]);
    let m0 = b.node(post, Prim::HistAppend { state: h }, &[Ref::CarryIn(0)], ty(DType::I32, &[Dim::H, Dim::Fixed(1)]), false);
    let m1 = b.node(post, Prim::ReduceSum { axis: 0 }, &[Ref::Node(m0)], fixed(DType::I32, &[1, 1]), true);
    b.schedule(pre, &[], post, m1);
    let p = b.finish();
    let (a, f, _) = both_decode(&p);
    println!("decode: ref2 {a:?} first {f:?}");
    if a.is_ok() && f.is_ok() {
        run_both("p02", &p, &Params::new(), &[3, 5, 1, 2, 9]);
    }
}

fn cone_both(
    name: &str,
    p: &Program,
    params: &Params,
    block: u8,
    layer: Option<u32>,
    target: u16,
    env: &ConeEnv,
    token_supplied: bool,
) -> (Outcome<Tensor>, Outcome<Tensor>) {
    let fp = match first_decode(&encode(p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{name}: first refuses: {o:?}"),
    };
    let a = mine(eval_cone(p, params, block, layer, target, env));
    let f = first_cone(&fp, &params_to(params), block, layer, target, &env_to(env, token_supplied));
    println!("{name}: ref2 {:?}\n{:>w$}first {:?}", a.clone().map_tensor(), "", f.clone().map_tensor(), w = name.len() + 2);
    (a, f)
}

trait MapT {
    fn map_tensor(self) -> String;
}
impl MapT for Outcome<Tensor> {
    fn map_tensor(self) -> String {
        match self {
            Outcome::Ok(t) => format!("ok {:?}", t.data),
            Outcome::Err(c) => format!("err {}", c.name()),
            Outcome::Panic(s) => format!("PANIC {s}"),
        }
    }
}

fn honest_env(token: u64, pos: u64) -> ConeEnv {
    let mut e = ConeEnv { token, pos, ..Default::default() };
    e.fixed.insert(0, t(DType::I32, &[2], &[4, -4]));
    e
}

#[test]
fn p03_cone_edges() {
    let p = base(false);
    let params = Params::new();
    // Honest: the carry-out node 4 of pre from token 3, s0 = (4, −4).
    let (a, f) = cone_both("p03 honest", &p, &params, 0, None, 4, &honest_env(3, 2), true);
    assert_eq!(a, f);
    // The target itself supplied (with a wrong value): recomputed, or taken?
    let mut e = honest_env(3, 2);
    e.supplied.insert(4, t(DType::I32, &[2], &[99, 99]));
    cone_both("p03 target supplied", &p, &params, 0, None, 4, &e, true);
    // pos ≥ history_bound.
    cone_both("p03 pos = history_bound", &p, &params, 0, None, 4, &honest_env(3, HB as u64), true);
    cone_both("p03 pos = u32::MAX", &p, &params, 0, None, 4, &honest_env(3, u32::MAX as u64), true);
    // token ≥ token_bound, and no token supplied at all.
    cone_both("p03 token = token_bound", &p, &params, 0, None, 4, &honest_env(16, 2), true);
    cone_both("p03 no token", &p, &params, 0, None, 4, &honest_env(3, 2), false);
    // A supplied node that does not exist, and a wrong-typed supplied node outside the closure.
    let mut e = honest_env(3, 2);
    e.supplied.insert(40, t(DType::I32, &[2], &[1, 1]));
    cone_both("p03 supplied node 40 (no such node)", &p, &params, 0, None, 4, &e, true);
    let mut e = honest_env(3, 2);
    e.supplied.insert(3, t(DType::I8, &[7], &[1, 1, 1, 1, 1, 1, 1]));
    cone_both("p03 wrong-typed supplied node outside the closure", &p, &params, 0, None, 4, &e, true);
    // A wrong-typed supplied node inside the closure.
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, t(DType::I64, &[2], &[1, 1]));
    cone_both("p03 wrong-typed supplied node inside the closure", &p, &params, 0, None, 4, &e, true);
    // Fixed value out of [lo, hi], of the wrong shape, missing.
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, t(DType::I32, &[2], &[101, 0]));
    cone_both("p03 fixed outside [lo, hi]", &p, &params, 0, None, 4, &e, true);
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, t(DType::I32, &[3], &[1, 0, 0]));
    cone_both("p03 fixed wrong shape", &p, &params, 0, None, 4, &e, true);
    let mut e = honest_env(3, 2);
    e.fixed.clear();
    cone_both("p03 fixed missing", &p, &params, 0, None, 4, &e, true);
    // post: carry-in of the wrong dtype / shape / missing.
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, t(DType::I16, &[2], &[1, 2]));
    cone_both("p03 carry-in wrong dtype", &p, &params, 1, None, 1, &e, true);
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, t(DType::I32, &[1], &[1]));
    cone_both("p03 carry-in wrong shape", &p, &params, 1, None, 1, &e, true);
    let e = honest_env(3, 2);
    cone_both("p03 carry-in missing", &p, &params, 1, None, 1, &e, true);
    // Not an occurrence.
    cone_both("p03 (pre, Some(0))", &p, &params, 0, Some(0), 4, &honest_env(3, 2), true);
    cone_both("p03 block 7", &p, &params, 7, None, 4, &honest_env(3, 2), true);
    cone_both("p03 target 9", &p, &params, 0, None, 9, &honest_env(3, 2), true);
}

#[test]
fn p04_token_not_needed() {
    // A program that never reads Input(0): is a token ≥ token_bound refused by a step?
    let mut b = ProgBuilder::new(HB, 4);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Input(1)], fixed(DType::I32, &[]), true);
    b.carry_out(pre, &[n0]);
    let post = b.block("post", vec![fixed(DType::I32, &[])]);
    let m0 = b.node(post, Prim::Clamp { lo: 0, hi: 10 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[]), true);
    b.schedule(pre, &[], post, m0);
    let p = b.finish();
    run_both("p04", &p, &Params::new(), &[0, 4, 1_000_000, u32::MAX as u64]);
}

#[test]
fn p05_hostile_run_states() {
    let p = base(false);
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{o:?}"),
    };
    let params = Params::new();
    let fparams = params_to(&params);
    let cases: Vec<(&str, RunState)> = vec![
        ("fixed outside [lo, hi]", {
            let mut s = initial_state(&p);
            s.fixed.insert((0, None), t(DType::I32, &[2], &[500, 0]));
            s
        }),
        ("fixed wrong shape", {
            let mut s = initial_state(&p);
            s.fixed.insert((0, None), t(DType::I32, &[1], &[0]));
            s
        }),
        ("fixed missing", {
            let mut s = initial_state(&p);
            s.fixed.clear();
            s
        }),
        ("pos = history_bound", {
            let mut s = initial_state(&p);
            s.pos = HB as u64;
            s
        }),
        ("pos = history_bound − 1", {
            let mut s = initial_state(&p);
            s.pos = HB as u64 - 1;
            s
        }),
    ];
    for (name, st) in cases {
        let (a, _) = ref2_step(&p, &params, &st, 3);
        let mut fst = state_to(&st);
        let f = first_step(&fp, &fparams, &mut fst, 3);
        println!("p05 {}{name}: ref2 {} / first {}", if a.is_ok() == f.is_ok() { "" } else { "DIFFER " }, summary(&a), summary(&f));
    }
}

#[test]
fn p06_primitive_extremes() {
    let i128min = i128::MIN;
    let i128max = i128::MAX;
    let cases: Vec<(&str, Prim, Vec<Tensor>, DType, Vec<u64>)> = vec![
        (
            "HAFZ(i128::MIN, 1)",
            Prim::Div { rule: Rounding::HalfAwayFromZero },
            vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[1])],
            DType::I128,
            vec![],
        ),
        (
            "HalfUp(i128::MIN, i128::MAX)",
            Prim::Div { rule: Rounding::HalfUp },
            vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[i128max])],
            DType::I128,
            vec![],
        ),
        (
            "HalfUp(i128::MAX, i128::MAX)",
            Prim::Div { rule: Rounding::HalfUp },
            vec![t(DType::I128, &[], &[i128max]), t(DType::I128, &[], &[i128max])],
            DType::I128,
            vec![],
        ),
        (
            "HalfUp(i128::MAX − 1, i128::MAX)",
            Prim::Div { rule: Rounding::HalfUp },
            vec![t(DType::I128, &[], &[i128max - 1]), t(DType::I128, &[], &[i128max])],
            DType::I128,
            vec![],
        ),
        (
            "HAFZ(i128::MIN, i128::MAX)",
            Prim::Div { rule: Rounding::HalfAwayFromZero },
            vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[i128max])],
            DType::I128,
            vec![],
        ),
        (
            "HAFZ(i128::MAX, 2^126 + 1)",
            Prim::Div { rule: Rounding::HalfAwayFromZero },
            vec![t(DType::I128, &[], &[i128max]), t(DType::I128, &[], &[(1i128 << 126) + 1])],
            DType::I128,
            vec![],
        ),
        (
            "Floor(i128::MIN, 3)",
            Prim::Div { rule: Rounding::Floor },
            vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[3])],
            DType::I128,
            vec![],
        ),
        ("Mul(i128::MIN, −1)", Prim::Mul, vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[-1])], DType::I128, vec![]),
        (
            "Mul(i128::MIN, i128::MIN)",
            Prim::Mul,
            vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[i128min])],
            DType::I128,
            vec![],
        ),
        ("Sub(i128::MIN, 1)", Prim::Sub, vec![t(DType::I128, &[], &[i128min]), t(DType::I128, &[], &[1])], DType::I128, vec![]),
        (
            "Add(i128::MAX, i128::MIN)",
            Prim::Add,
            vec![t(DType::I128, &[], &[i128max]), t(DType::I128, &[], &[i128min])],
            DType::I128,
            vec![],
        ),
        (
            "Sub(i128::MAX, i128::MIN)",
            Prim::Sub,
            vec![t(DType::I128, &[], &[i128max]), t(DType::I128, &[], &[i128min])],
            DType::I128,
            vec![],
        ),
        (
            "MatMul i64::MIN² × 3 into i128",
            Prim::MatMul,
            vec![t(DType::I64, &[1, 3], &[i64::MIN as i128; 3]), t(DType::I64, &[3, 1], &[i64::MIN as i128; 3])],
            DType::I128,
            vec![1, 1],
        ),
        (
            "MatMul ±i64::MIN² alternating into i128 (total 2^126, P = 2·2^126)",
            Prim::MatMul,
            vec![
                t(DType::I64, &[1, 3], &[i64::MIN as i128, i64::MIN as i128, i64::MIN as i128]),
                t(DType::I64, &[3, 1], &[i64::MIN as i128, i64::MAX as i128, i64::MIN as i128]),
            ],
            DType::I128,
            vec![1, 1],
        ),
        (
            "ReduceSum into idx, total ≥ 0 with a negative term",
            Prim::ReduceSum { axis: 0 },
            vec![t(DType::I32, &[3], &[5, -1, 2])],
            DType::Idx,
            vec![1],
        ),
        (
            "ReduceSum i128 extremes",
            Prim::ReduceSum { axis: 0 },
            vec![t(DType::I128, &[3], &[i128max, i128min, i128max])],
            DType::I128,
            vec![1],
        ),
        ("ReduceSum i128 two maxima", Prim::ReduceSum { axis: 0 }, vec![t(DType::I128, &[2], &[i128max, 1])], DType::I128, vec![1]),
        (
            "Iota i64::MIN step i64::MIN into i128",
            Prim::Iota { axis: 0, start: i64::MIN, step: i64::MIN },
            vec![],
            DType::I128,
            vec![3],
        ),
        ("Iota i64::MAX step i64::MAX into i64", Prim::Iota { axis: 0, start: i64::MAX, step: i64::MAX }, vec![], DType::I64, vec![2]),
        (
            "Slice start = u32::MAX",
            Prim::Slice { axis: 0, start: u32::MAX },
            vec![t(DType::I32, &[4], &[1, 2, 3, 4])],
            DType::I32,
            vec![1],
        ),
        (
            "Slice start = 2^32 − 3, len 3, extent 4",
            Prim::Slice { axis: 0, start: u32::MAX - 2 },
            vec![t(DType::I32, &[4], &[1, 2, 3, 4])],
            DType::I32,
            vec![3],
        ),
        ("Log2Floor i128::MAX into i8", Prim::Log2Floor, vec![t(DType::I128, &[], &[i128max])], DType::I8, vec![]),
        ("Log2Floor 0 into idx", Prim::Log2Floor, vec![t(DType::I32, &[], &[0])], DType::Idx, vec![]),
        ("IntExp of idx 5", Prim::IntExp, vec![t(DType::Idx, &[], &[5])], DType::I32, vec![]),
        ("IntExp of i64::MIN", Prim::IntExp, vec![t(DType::I64, &[], &[i64::MIN as i128])], DType::I32, vec![]),
        ("IntRsqrt of i64::MAX", Prim::IntRsqrt, vec![t(DType::I64, &[], &[i64::MAX as i128])], DType::I64, vec![]),
        ("IntRsqrt of 1 into i32 (2^36 − 2^12)", Prim::IntRsqrt, vec![t(DType::I64, &[], &[1])], DType::I32, vec![]),
        ("IntLn of i64::MAX", Prim::IntLn, vec![t(DType::I64, &[], &[i64::MAX as i128])], DType::I64, vec![]),
        ("IntLn of idx 2^32 − 1", Prim::IntLn, vec![t(DType::Idx, &[], &[u32::MAX as i128])], DType::I64, vec![]),
        ("IntExp shape [2] → [1, 2]", Prim::IntExp, vec![t(DType::I32, &[2], &[0, -1])], DType::I32, vec![1, 2]),
        ("IntLn shape [2] → [2, 1]", Prim::IntLn, vec![t(DType::I32, &[2], &[1, 2])], DType::I64, vec![2, 1]),
        ("Log2Floor shape [2] → [1, 2]", Prim::Log2Floor, vec![t(DType::I32, &[2], &[1, 2])], DType::I32, vec![1, 2]),
        ("Transpose rank 0", Prim::Transpose { perm: vec![] }, vec![t(DType::I8, &[], &[5])], DType::I8, vec![]),
        ("Reshape [1,1,1,1] → []", Prim::Reshape, vec![t(DType::I8, &[1, 1, 1, 1], &[5])], DType::I8, vec![]),
        ("Broadcast [] → [2,1,3,1]", Prim::Broadcast, vec![t(DType::I8, &[], &[5])], DType::I8, vec![2, 1, 3, 1]),
        (
            "Gather batch_dims = rank(indices)",
            Prim::Gather { axis: 1, batch_dims: 1 },
            vec![t(DType::I32, &[2, 3], &[1, 2, 3, 4, 5, 6]), t(DType::I32, &[2], &[2, 0])],
            DType::I32,
            vec![2],
        ),
        (
            "Gather i128 data",
            Prim::Gather { axis: 0, batch_dims: 0 },
            vec![t(DType::I128, &[2], &[i128min, i128max]), t(DType::Idx, &[], &[1])],
            DType::I128,
            vec![],
        ),
        (
            "Clamp idx [0, 2^32 − 1] of i128::MIN",
            Prim::Clamp { lo: 0, hi: u32::MAX as i64 },
            vec![t(DType::I128, &[], &[i128min])],
            DType::Idx,
            vec![],
        ),
        (
            "Select c = idx",
            Prim::Select,
            vec![t(DType::Idx, &[2], &[0, 7]), t(DType::I8, &[], &[1]), t(DType::I128, &[2], &[i128max, -3])],
            DType::I8,
            vec![2],
        ),
        (
            "Select unchosen value overflows",
            Prim::Select,
            vec![t(DType::I8, &[2], &[1, 1]), t(DType::I8, &[], &[1]), t(DType::I128, &[2], &[i128max, -3])],
            DType::I8,
            vec![2],
        ),
        (
            "Compare idx vs i8",
            Prim::Compare { cmp: ref2::Cmp::Gt },
            vec![t(DType::Idx, &[2], &[0, u32::MAX as i128]), t(DType::I8, &[1], &[-1])],
            DType::I8,
            vec![2],
        ),
        ("TopK k = n with ties", Prim::TopK { axis: 0, k: 4 }, vec![t(DType::I8, &[4], &[1, 1, 1, 1])], DType::Idx, vec![4]),
        (
            "TopK k = 2 ties at the cut",
            Prim::TopK { axis: 1, k: 2 },
            vec![t(DType::I8, &[2, 4], &[3, 5, 5, 5, -1, -1, -2, -1])],
            DType::Idx,
            vec![2, 2],
        ),
        ("Concat of 8", Prim::Concat { axis: 0 }, (0..8).map(|i| t(DType::I8, &[1], &[i])).collect(), DType::I8, vec![8]),
        ("Concat of 9", Prim::Concat { axis: 0 }, (0..9).map(|i| t(DType::I8, &[1], &[i])).collect(), DType::I8, vec![9]),
        ("Cast i128::MIN → i128", Prim::Cast, vec![t(DType::I128, &[], &[i128min])], DType::I128, vec![]),
        ("Out dim 0", Prim::Iota { axis: 0, start: 0, step: 1 }, vec![], DType::I8, vec![0]),
        ("Out rank 5", Prim::Iota { axis: 0, start: 0, step: 0 }, vec![], DType::I8, vec![1, 1, 1, 1, 1]),
        ("Out 2^28 + elements", Prim::Iota { axis: 0, start: 0, step: 0 }, vec![], DType::I8, vec![1 << 14, 1 << 14, 2]),
        (
            "MatMul K mismatch with a broadcastable 1",
            Prim::MatMul,
            vec![t(DType::I8, &[1, 1], &[1]), t(DType::I8, &[3, 1], &[1, 1, 1])],
            DType::I32,
            vec![1, 1],
        ),
    ];
    let mut differ = 0;
    for (name, prim, ins, od, os) in cases {
        let a = mine(eval_primitive(&prim, &ins, od, &os));
        let f = first_eval_primitive(&prim, &ins, od, &os);
        let same = match (&a, &f) {
            (Outcome::Ok(x), Outcome::Ok(y)) => x == y,
            (Outcome::Err(_), Outcome::Err(_)) => true,
            _ => false,
        };
        if !same {
            differ += 1;
        }
        println!(
            "p06 {}{name}: ref2 {} / first {}",
            if same { "" } else { "DIFFER " },
            a.clone().map_tensor(),
            f.clone().map_tensor()
        );
    }
    println!("p06: {differ} disagreements");
}

#[test]
fn p07_size_caps() {
    // A Hist row of 2048 i8 with window 2^18: the HistAppend output is 2^29 elements at H = W.
    let build = |row: u32, window: u32| {
        let mut b = ProgBuilder::new(HB, 16);
        let h = b.hist_state("h", DType::I8, &[row], window, false);
        let pre = b.block("pre", vec![]);
        let n0 = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 0 }, &[], fixed(DType::I8, &[row]), true);
        let n1 = b.node(pre, Prim::HistAppend { state: h }, &[Ref::Node(n0)], ty(DType::I8, &[Dim::H, Dim::Fixed(row)]), false);
        let n2 = b.node(pre, Prim::ReduceMax { axis: 0 }, &[Ref::Node(n1)], fixed(DType::I8, &[1, row]), true);
        b.carry_out(pre, &[n2]);
        let post = b.block("post", vec![fixed(DType::I8, &[1, row])]);
        let m0 = b.node(post, Prim::ReduceSum { axis: 1 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1, 1]), true);
        b.schedule(pre, &[], post, m0);
        b.finish()
    };
    for (row, window) in [(1024u32, 1u32 << 18), (1025, 1 << 18), (2048, 1 << 18), (2048, 1 << 17)] {
        let (a, f, _) = both_decode(&build(row, window));
        println!("p07 hist row {row} window {window}: ref2 {a:?} / first {f:?}");
    }
    // A computed tensor of 2^29 elements in a block without H.
    let mut b = ProgBuilder::new(HB, 16);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 0 }, &[], fixed(DType::I8, &[1 << 14, 1 << 14, 2]), false);
    let n1 = b.node(pre, Prim::ReduceMax { axis: 0 }, &[Ref::Node(n0)], fixed(DType::I8, &[1, 1 << 14, 2]), false);
    let n2 = b.node(pre, Prim::ReduceMax { axis: 1 }, &[Ref::Node(n1)], fixed(DType::I8, &[1, 1, 2]), true);
    b.carry_out(pre, &[n2]);
    let post = b.block("post", vec![fixed(DType::I8, &[1, 1, 2])]);
    let m0 = b.node(post, Prim::Clamp { lo: -1, hi: 1 }, &[Ref::CarryIn(0)], fixed(DType::I8, &[1, 1, 2]), true);
    b.schedule(pre, &[], post, m0);
    let (a, f, _) = both_decode(&b.finish());
    println!("p07 computed 2^29 elements: ref2 {a:?} / first {f:?}");
    // A param of 2^30 elements (NF-8 allows up to 2^40).
    let mut b = ProgBuilder::new(HB, 16);
    let w = b.param("emb", DType::I8, &[1 << 16, 1 << 14], false);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Gather { axis: 0, batch_dims: 0 }, &[Ref::Param(w), Ref::Input(0)], fixed(DType::I8, &[1 << 14]), true);
    b.carry_out(pre, &[n0]);
    let post = b.block("post", vec![fixed(DType::I8, &[1 << 14])]);
    let m0 = b.node(post, Prim::Clamp { lo: -1, hi: 1 }, &[Ref::CarryIn(0)], fixed(DType::I8, &[1 << 14]), true);
    b.schedule(pre, &[], post, m0);
    let (a, f, _) = both_decode(&b.finish());
    println!("p07 param of 2^30 elements: ref2 {a:?} / first {f:?}");
    assert_eq!(a, f);
}

#[test]
fn p08_param_binding() {
    // A per-layer param used by a block running at layers 0 and 2 only (layer 1 runs another block).
    let mut b = ProgBuilder::new(HB, 16);
    let w = b.param("w", DType::I8, &[2], true);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Iota { axis: 0, start: 1, step: 1 }, &[], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[n0]);
    let la = b.block("la", vec![fixed(DType::I32, &[2])]);
    let a0 = b.node(la, Prim::Add, &[Ref::CarryIn(0), Ref::Param(w)], fixed(DType::I64, &[2]), false);
    let a1 = b.node(la, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::Node(a0)], fixed(DType::I32, &[2]), true);
    b.carry_out(la, &[a1]);
    let lb = b.block("lb", vec![fixed(DType::I32, &[2])]);
    let b0 = b.node(lb, Prim::Mul, &[Ref::CarryIn(0), Ref::Input(1)], fixed(DType::I64, &[2]), false);
    let b1 = b.node(lb, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::Node(b0)], fixed(DType::I32, &[2]), true);
    b.carry_out(lb, &[b1]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let m0 = b.node(post, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[la, lb, la], post, m0);
    let p = b.finish();
    let mk = |layers: &[u32], dt: DType| -> Params {
        let mut m = BTreeMap::new();
        for &l in layers {
            m.insert((w, Some(l)), Tensor::new(dt, vec![2], vec![l as i128, -(l as i128)]).unwrap());
        }
        m
    };
    run_both("p08 exact", &p, &mk(&[0, 2], DType::I8), &[1, 2]);
    run_both("p08 extra instance at layer 1", &p, &mk(&[0, 1, 2], DType::I8), &[1]);
    run_both("p08 missing layer 2", &p, &mk(&[0], DType::I8), &[1]);
    run_both("p08 wrong dtype", &p, &mk(&[0, 2], DType::I16), &[1]);
    let mut extra = mk(&[0, 2], DType::I8);
    extra.insert((w, None), t(DType::I8, &[2], &[1, 1]));
    run_both("p08 extra global instance of a per-layer param", &p, &extra, &[1]);
    let mut extra = mk(&[0, 2], DType::I8);
    extra.insert((9, None), t(DType::I8, &[2], &[1, 1]));
    run_both("p08 a param that is not declared", &p, &extra, &[1]);
    let _ = Class::Missing;
}

/// A layer block with a per-layer Hist (window 3) and a per-layer Fixed state: cone evaluation
/// with the history prior missing, short, long, or of the wrong row type.
fn hist_program() -> Program {
    let mut b = ProgBuilder::new(HB, 64);
    let h = b.hist_state("kv", DType::I16, &[2], 3, true);
    let s = b.fixed_state("acc", DType::I32, &[2], -50, 50, true);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Input(0)], fixed(DType::I16, &[]), false);
    let n1 = b.node(pre, Prim::Broadcast, &[Ref::Node(n0)], fixed(DType::I16, &[2]), true);
    b.carry_out(pre, &[n1]);
    let lay = b.block("layer", vec![fixed(DType::I16, &[2])]);
    let a0 = b.node(lay, Prim::HistAppend { state: h }, &[Ref::CarryIn(0)], ty(DType::I16, &[Dim::H, Dim::Fixed(2)]), false);
    let a1 = b.node(lay, Prim::ReduceSum { axis: 0 }, &[Ref::Node(a0)], fixed(DType::I32, &[1, 2]), false);
    let a2 = b.node(lay, Prim::Reshape, &[Ref::Node(a1)], fixed(DType::I32, &[2]), false);
    let a3 = b.node(lay, Prim::Add, &[Ref::Node(a2), Ref::State(s)], fixed(DType::I32, &[2]), false);
    b.node(lay, Prim::StateWrite { state: s }, &[Ref::Node(a3)], fixed(DType::I32, &[2]), false);
    let a5 = b.node(lay, Prim::Clamp { lo: -30000, hi: 30000 }, &[Ref::Node(a3)], fixed(DType::I16, &[2]), true);
    b.carry_out(lay, &[a5]);
    let post = b.block("post", vec![fixed(DType::I16, &[2])]);
    let m0 = b.node(post, Prim::Cast, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[lay, lay], post, m0);
    b.finish()
}

#[test]
fn p09_cone_histories_and_layer_instances() {
    let p = hist_program();
    let params = Params::new();
    let (a, f, _) = both_decode(&p);
    assert_eq!((a.is_ok(), f.is_ok()), (true, true));
    let r = run_both("p09 run", &p, &params, &[1, 2, 3, 4, 5]);
    assert!(r.iter().all(|(a, f)| a == f));
    let row = |x: i128| t(DType::I16, &[2], &[x, -x]);
    let env = |pos: u64, rows: Option<Vec<Tensor>>, fixed: Option<Tensor>| {
        let mut e = ConeEnv { token: 1, pos, ..Default::default() };
        e.carry_in.insert(0, row(5));
        if let Some(r) = rows {
            e.hist_prior.insert(0, r);
        }
        if let Some(fx) = fixed {
            e.fixed.insert(1, fx);
        }
        e
    };
    let acc = t(DType::I32, &[2], &[3, -3]);
    let (a, f) = cone_both(
        "p09 pos 4 honest (2 rows)",
        &p,
        &params,
        1,
        Some(1),
        5,
        &env(4, Some(vec![row(1), row(2)]), Some(acc.clone())),
        true,
    );
    assert_eq!(a, f);
    cone_both("p09 pos 4 history missing", &p, &params, 1, Some(1), 5, &env(4, None, Some(acc.clone())), true);
    cone_both("p09 pos 4 one row (short)", &p, &params, 1, Some(1), 5, &env(4, Some(vec![row(1)]), Some(acc.clone())), true);
    cone_both(
        "p09 pos 4 three rows (long)",
        &p,
        &params,
        1,
        Some(1),
        5,
        &env(4, Some(vec![row(1), row(2), row(3)]), Some(acc.clone())),
        true,
    );
    cone_both("p09 pos 0 no rows", &p, &params, 1, Some(0), 5, &env(0, Some(vec![]), Some(acc.clone())), true);
    cone_both("p09 pos 0 history missing", &p, &params, 1, Some(0), 5, &env(0, None, Some(acc.clone())), true);
    cone_both(
        "p09 pos 4 a row of the wrong dtype",
        &p,
        &params,
        1,
        Some(1),
        5,
        &env(4, Some(vec![row(1), t(DType::I32, &[2], &[2, -2])]), Some(acc.clone())),
        true,
    );
    cone_both("p09 pos 4 fixed missing", &p, &params, 1, Some(1), 5, &env(4, Some(vec![row(1), row(2)]), None), true);
    cone_both("p09 (layer, None)", &p, &params, 1, None, 5, &env(4, Some(vec![row(1), row(2)]), Some(acc.clone())), true);
    cone_both(
        "p09 (layer, Some(2)) past the schedule",
        &p,
        &params,
        1,
        Some(2),
        5,
        &env(4, Some(vec![row(1), row(2)]), Some(acc)),
        true,
    );
    // The HistAppend node itself as a target.
    cone_both("p09 target = HistAppend", &p, &params, 1, Some(1), 0, &env(4, Some(vec![row(1), row(2)]), None), true);
    // A hostile RunState for a step: history rows that do not match the position.
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{o:?}"),
    };
    let mut st = initial_state(&p);
    st.pos = 3;
    st.hist.insert((0, Some(0)), vec![row(1)]);
    st.hist.insert((0, Some(1)), vec![row(1), row(2)]);
    let (a, _) = ref2_step(&p, &params, &st, 1);
    let mut fst = state_to(&st);
    let f = first_step(&fp, &params_to(&params), &mut fst, 1);
    println!("p09 step with a short history at layer 0: ref2 {} / first {}", summary(&a), summary(&f));
    let mut st = initial_state(&p);
    st.pos = 1;
    st.hist.insert((0, Some(0)), vec![row(1), row(2), row(3)]);
    st.hist.insert((0, Some(1)), vec![row(1)]);
    let (a, _) = ref2_step(&p, &params, &st, 1);
    let mut fst = state_to(&st);
    let f = first_step(&fp, &params_to(&params), &mut fst, 1);
    println!("p09 step with a long history at layer 0: ref2 {} / first {}", summary(&a), summary(&f));
    let mut st = initial_state(&p);
    st.hist.clear();
    let (a, _) = ref2_step(&p, &params, &st, 1);
    let mut fst = state_to(&st);
    let f = first_step(&fp, &params_to(&params), &mut fst, 1);
    println!("p09 step with no history instance: ref2 {} / first {}", summary(&a), summary(&f));
}

/// Tensors whose values are outside their dtype, handed to both evaluators directly (bypassing
/// this crate's checked constructor): as a param, a Fixed state, a carry-in and a supplied node.
#[test]
fn p10_values_outside_their_dtype() {
    let raw = |dtype: DType, shape: &[u64], data: &[i128]| Tensor { dtype, shape: shape.to_vec(), data: data.to_vec() };
    // A param i8[2] holding 200.
    let mut b = ProgBuilder::new(HB, 16);
    let w = b.param("w", DType::I8, &[2], false);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Param(w)], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[n0]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let m0 = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[], post, m0);
    let p = b.finish();
    let mut params = Params::new();
    params.insert((w, None), raw(DType::I8, &[2], &[200, 1]));
    let r = run_both("p10 param i8 holding 200", &p, &params, &[1]);
    assert!(!r[0].0.is_ok() && !r[0].1.is_ok());
    params.insert((w, None), raw(DType::I8, &[2], &[1]));
    let r = run_both("p10 param with too few elements", &p, &params, &[1]);
    assert!(!r[0].0.is_ok() && !r[0].1.is_ok());
    // A Fixed state value outside its dtype, a carry-in outside its dtype, a supplied node outside.
    let p = base(false);
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 fixed value outside i32", &p, &Params::new(), 0, None, 4, &e, true);
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 carry-in outside i32", &p, &Params::new(), 1, None, 1, &e, true);
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 supplied node outside i32", &p, &Params::new(), 0, None, 4, &e, true);
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, raw(DType::I32, &[2], &[1]));
    cone_both("p10 supplied node with too few elements", &p, &Params::new(), 0, None, 4, &e, true);
}
