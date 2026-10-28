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
    // Revision 2, NF-19: post writes no state, so pre and post cannot both write one (was F5).
    let p = base(true);
    let (a, f, _) = both_decode(&p);
    println!("decode: ref2 {a:?} first {f:?}");
    assert_eq!(a, Outcome::Err(Class::NormalForm));
    assert_eq!(f, Outcome::Err(Class::NormalForm));
    // Without the second writer the program runs, and post reads the start-of-position value.
    let p = base(false);
    let r = run_both("p01 pre writes, post reads", &p, &Params::new(), &[3, 5, 1]);
    assert!(r.iter().all(|(a, f)| a.is_ok() && a == f));
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
    // Revision 2, NF-19: refused whatever the window (was F6).
    assert_eq!(a, Outcome::Err(Class::NormalForm));
    assert_eq!(f, Outcome::Err(Class::NormalForm));
}

/// Runs a cone on both and asserts the verdict 04b (revision 2) gives: `None` = success (both
/// equal), `Some(class)` = refused with that class by both.
#[allow(clippy::too_many_arguments)]
fn cone_both(
    name: &str,
    p: &Program,
    params: &Params,
    block: u8,
    layer: Option<u32>,
    target: u16,
    env: &ConeEnv,
    text: Option<Class>,
) -> (Outcome<Tensor>, Outcome<Tensor>) {
    let fp = match first_decode(&encode(p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{name}: first refuses: {o:?}"),
    };
    let a = mine(eval_cone(p, params, block, layer, target, env));
    let f = first_cone(&fp, &params_to(params), block, layer, target, &env_to(env));
    println!(
        "{name}: text {} ref2 {:?} / first {:?}",
        text.map(|c| c.name()).unwrap_or("ok"),
        a.clone().map_tensor(),
        f.clone().map_tensor()
    );
    match text {
        None => assert!(a.is_ok() && a == f, "{name}"),
        Some(c) => {
            assert_eq!(a, Outcome::Err(c), "{name}: ref2");
            assert_eq!(f, Outcome::Err(c), "{name}: first");
        }
    }
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
    let mut e = ConeEnv { token: Some(token), pos, ..Default::default() };
    e.fixed.insert(0, t(DType::I32, &[2], &[4, -4]));
    e
}

#[test]
fn p03_cone_edges() {
    use Class::*;
    let p = base(false);
    let params = Params::new();
    // Honest: the carry-out node 4 of pre from token 3, s0 = (4, −4).
    let (a, _) = cone_both("p03 honest", &p, &params, 0, None, 4, &honest_env(3, 2), None);
    assert_eq!(a, Outcome::Ok(t(DType::I32, &[2], &[7, -1])));
    // The target itself supplied (F2): Malformed.
    let mut e = honest_env(3, 2);
    e.supplied.insert(4, t(DType::I32, &[2], &[99, 99]));
    cone_both("p03 target supplied", &p, &params, 0, None, 4, &e, Some(Malformed));
    // pos ≥ history_bound (F7).
    cone_both("p03 pos = history_bound", &p, &params, 0, None, 4, &honest_env(3, HB as u64), Some(Position));
    cone_both("p03 pos = u32::MAX", &p, &params, 0, None, 4, &honest_env(3, u32::MAX as u64), Some(Position));
    // The token: at token_bound, absent (F7, F10).
    cone_both("p03 token = token_bound", &p, &params, 0, None, 4, &honest_env(16, 2), Some(Operand));
    let mut e = honest_env(3, 2);
    e.token = None;
    cone_both("p03 no token", &p, &params, 0, None, 4, &e, Some(Missing));
    // A supplied index that is no node (F4): Malformed; a wrong-typed entry the closure does not
    // reach: ignored.
    let mut e = honest_env(3, 2);
    e.supplied.insert(40, t(DType::I32, &[2], &[1, 1]));
    cone_both("p03 supplied node 40 (no such node)", &p, &params, 0, None, 4, &e, Some(Malformed));
    let mut e = honest_env(3, 2);
    e.supplied.insert(3, t(DType::I8, &[7], &[1, 1, 1, 1, 1, 1, 1]));
    cone_both("p03 wrong-typed supplied node outside the closure", &p, &params, 0, None, 4, &e, None);
    // A wrong-typed supplied node inside the closure.
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, t(DType::I64, &[2], &[1, 1]));
    cone_both("p03 wrong-typed supplied node inside the closure", &p, &params, 0, None, 4, &e, Some(Operand));
    // Fixed value out of [lo, hi], of the wrong shape, missing (F1).
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, t(DType::I32, &[2], &[101, 0]));
    cone_both("p03 fixed outside [lo, hi]", &p, &params, 0, None, 4, &e, Some(Operand));
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, t(DType::I32, &[3], &[1, 0, 0]));
    cone_both("p03 fixed wrong shape", &p, &params, 0, None, 4, &e, Some(Operand));
    let mut e = honest_env(3, 2);
    e.fixed.clear();
    cone_both("p03 fixed missing", &p, &params, 0, None, 4, &e, Some(Missing));
    // post: carry-in of the wrong dtype / shape / missing (F8).
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, t(DType::I16, &[2], &[1, 2]));
    cone_both("p03 carry-in wrong dtype", &p, &params, 1, None, 1, &e, Some(Operand));
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, t(DType::I32, &[1], &[1]));
    cone_both("p03 carry-in wrong shape", &p, &params, 1, None, 1, &e, Some(Operand));
    let e = honest_env(3, 2);
    cone_both("p03 carry-in missing", &p, &params, 1, None, 1, &e, Some(Missing));
    // Not an occurrence, not a node.
    cone_both("p03 (pre, Some(0))", &p, &params, 0, Some(0), 4, &honest_env(3, 2), Some(Malformed));
    cone_both("p03 block 7", &p, &params, 7, None, 4, &honest_env(3, 2), Some(Malformed));
    cone_both("p03 target 9", &p, &params, 0, None, 9, &honest_env(3, 2), Some(Malformed));
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
    // Revision 2: §9.1(2) checks (Operand), the position (Position), and run-state completeness —
    // an absent Fixed instance is all zeros (F11).
    let text = [Some(Class::Operand), Some(Class::Operand), None, Some(Class::Position), None];
    for ((name, st), want) in cases.into_iter().zip(text) {
        let (a, _) = ref2_step(&p, &params, &st, 3);
        let mut fst = state_to(&st);
        let f = first_step(&fp, &fparams, &mut fst, 3);
        println!("p05 {name}: text {} ref2 {} / first {}", want.map(|c| c.name()).unwrap_or("ok"), summary(&a), summary(&f));
        match want {
            None => assert!(a.is_ok() && a == f, "{name}"),
            Some(c) => {
                assert_eq!(a.class(), Some(c), "{name}");
                assert_eq!(f.class(), Some(c), "{name}");
            }
        }
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
        // Revision 2 (§9.3): each case breaks one rule, so the class must agree too.
        let same = match (&a, &f) {
            (Outcome::Ok(x), Outcome::Ok(y)) => x == y,
            (Outcome::Err(x), Outcome::Err(y)) => x == y,
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
    assert_eq!(differ, 0);
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
    let check = |name: &str, params: &Params, tokens: &[u64], want: Option<Class>| {
        for (a, f) in run_both(name, &p, params, tokens) {
            match want {
                None => assert!(a.is_ok() && a == f, "{name}"),
                Some(c) => assert_eq!((a.class(), f.class()), (Some(c), Some(c)), "{name}"),
            }
        }
    };
    check("p08 exact", &mk(&[0, 2], DType::I8), &[1, 2], None);
    check("p08 extra instance at layer 1", &mk(&[0, 1, 2], DType::I8), &[1], None);
    check("p08 missing layer 2", &mk(&[0], DType::I8), &[1], Some(Class::Missing));
    check("p08 wrong dtype", &mk(&[0, 2], DType::I16), &[1], Some(Class::Operand));
    let mut extra = mk(&[0, 2], DType::I8);
    extra.insert((w, None), t(DType::I8, &[2], &[1, 1]));
    check("p08 extra global instance of a per-layer param", &extra, &[1], None);
    let mut extra = mk(&[0, 2], DType::I8);
    extra.insert((9, None), t(DType::I8, &[2], &[1, 1]));
    check("p08 a param that is not declared", &extra, &[1], None);
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
    use Class::*;
    let row = |x: i128| t(DType::I16, &[2], &[x, -x]);
    let env = |pos: u64, rows: Option<Vec<Tensor>>, fixed: Option<Tensor>| {
        let mut e = ConeEnv { token: Some(1), pos, ..Default::default() };
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
    let two = || Some(vec![row(1), row(2)]);
    let (a, _) = cone_both("p09 pos 4 honest (2 rows)", &p, &params, 1, Some(1), 5, &env(4, two(), Some(acc.clone())), None);
    assert_eq!(a, Outcome::Ok(t(DType::I16, &[2], &[11, -11])));
    cone_both("p09 pos 4 history missing", &p, &params, 1, Some(1), 5, &env(4, None, Some(acc.clone())), Some(Missing));
    cone_both("p09 pos 4 one row (short)", &p, &params, 1, Some(1), 5, &env(4, Some(vec![row(1)]), Some(acc.clone())), Some(Position));
    let three = Some(vec![row(1), row(2), row(3)]);
    cone_both("p09 pos 4 three rows (long)", &p, &params, 1, Some(1), 5, &env(4, three, Some(acc.clone())), Some(Position));
    cone_both("p09 pos 0 no rows", &p, &params, 1, Some(0), 5, &env(0, Some(vec![]), Some(acc.clone())), None);
    cone_both("p09 pos 0 history missing (F3)", &p, &params, 1, Some(0), 5, &env(0, None, Some(acc.clone())), Some(Missing));
    let bad_row = Some(vec![row(1), t(DType::I32, &[2], &[2, -2])]);
    cone_both("p09 pos 4 a row of the wrong dtype", &p, &params, 1, Some(1), 5, &env(4, bad_row, Some(acc.clone())), Some(Operand));
    cone_both("p09 pos 4 fixed missing (F1)", &p, &params, 1, Some(1), 5, &env(4, two(), None), Some(Missing));
    cone_both("p09 (layer, None)", &p, &params, 1, None, 5, &env(4, two(), Some(acc.clone())), Some(Malformed));
    cone_both("p09 (layer, Some(2)) past the schedule", &p, &params, 1, Some(2), 5, &env(4, two(), Some(acc)), Some(Malformed));
    // The HistAppend node itself as a target (its closure reads no Fixed value).
    cone_both("p09 target = HistAppend", &p, &params, 1, Some(1), 0, &env(4, two(), None), None);
    // Hostile run states for a step: history rows that do not match the position (Position), and
    // no history instance at pos 0 (run-state completeness: empty).
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{o:?}"),
    };
    let mut cases = Vec::new();
    let mut st = initial_state(&p);
    st.pos = 3;
    st.hist.insert((0, Some(0)), vec![row(1)]);
    st.hist.insert((0, Some(1)), vec![row(1), row(2)]);
    cases.push(("a short history at layer 0", st, Some(Position)));
    let mut st = initial_state(&p);
    st.pos = 1;
    st.hist.insert((0, Some(0)), vec![row(1), row(2), row(3)]);
    st.hist.insert((0, Some(1)), vec![row(1)]);
    cases.push(("a long history at layer 0", st, Some(Position)));
    let mut st = initial_state(&p);
    st.hist.clear();
    cases.push(("no history instance at pos 0", st, None));
    // An omitted instance is empty (run-state completeness), so at pos 2 it has too few rows.
    let mut st = initial_state(&p);
    st.pos = 2;
    st.hist.insert((0, Some(1)), vec![row(1), row(2)]);
    st.hist.remove(&(0, Some(0)));
    cases.push(("an omitted history instance at pos 2", st, Some(Position)));
    for (name, st, want) in cases {
        let (a, _) = ref2_step(&p, &params, &st, 1);
        let mut fst = state_to(&st);
        let f = first_step(&fp, &params_to(&params), &mut fst, 1);
        println!("p09 step with {name}: text {} ref2 {} / first {}", want.map(|c| c.name()).unwrap_or("ok"), summary(&a), summary(&f));
        match want {
            None => assert!(a.is_ok() && a == f),
            Some(c) => {
                assert_eq!(a.class(), Some(c));
                assert_eq!(f.class(), Some(c));
            }
        }
    }
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
    assert_eq!((r[0].0.class(), r[0].1.class()), (Some(Class::Operand), Some(Class::Operand)));
    params.insert((w, None), raw(DType::I8, &[2], &[1]));
    let r = run_both("p10 param with too few elements", &p, &params, &[1]);
    assert_eq!((r[0].0.class(), r[0].1.class()), (Some(Class::Operand), Some(Class::Operand)));
    // A Fixed state value outside its dtype, a carry-in outside its dtype, a supplied node outside.
    let p = base(false);
    let mut e = honest_env(3, 2);
    e.fixed.insert(0, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 fixed value outside i32", &p, &Params::new(), 0, None, 4, &e, Some(Class::Operand));
    let mut e = honest_env(3, 2);
    e.carry_in.insert(0, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 carry-in outside i32", &p, &Params::new(), 1, None, 1, &e, Some(Class::Operand));
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, raw(DType::I32, &[2], &[1i128 << 40, 0]));
    cone_both("p10 supplied node outside i32", &p, &Params::new(), 0, None, 4, &e, Some(Class::Operand));
    let mut e = honest_env(3, 2);
    e.supplied.insert(2, raw(DType::I32, &[2], &[1]));
    cone_both("p10 supplied node with too few elements", &p, &Params::new(), 0, None, 4, &e, Some(Class::Operand));
}

/// The largest H at run time: a history of window `history_bound` at pos `history_bound − 1`, and the
/// first position past it.
#[test]
fn p11_the_largest_window_at_the_last_position() {
    let mut b = ProgBuilder::new(HB, 16);
    let h = b.hist_state("h", DType::I8, &[1], HB, false);
    let pre = b.block("pre", vec![]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Input(0)], fixed(DType::I8, &[]), false);
    let n1 = b.node(pre, Prim::Reshape, &[Ref::Node(n0)], fixed(DType::I8, &[1]), true);
    let n2 = b.node(pre, Prim::HistAppend { state: h }, &[Ref::Node(n1)], ty(DType::I8, &[Dim::H, Dim::Fixed(1)]), false);
    let n3 = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], ty(DType::I32, &[Dim::H, Dim::Fixed(1)]), false);
    let n4 = b.node(pre, Prim::Mul, &[Ref::Node(n2), Ref::Node(n3)], ty(DType::I64, &[Dim::H, Dim::Fixed(1)]), false);
    let n5 = b.node(pre, Prim::ReduceSum { axis: 0 }, &[Ref::Node(n4)], fixed(DType::I64, &[1, 1]), false);
    let n6 = b.node(pre, Prim::Clamp { lo: i32::MIN as i64, hi: i32::MAX as i64 }, &[Ref::Node(n5)], fixed(DType::I32, &[1, 1]), true);
    b.carry_out(pre, &[n6]);
    let post = b.block("post", vec![fixed(DType::I32, &[1, 1])]);
    let m0 =
        b.node(post, Prim::Clamp { lo: i32::MIN as i64, hi: i32::MAX as i64 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1, 1]), true);
    b.schedule(pre, &[], post, m0);
    let p = b.finish();
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{o:?}"),
    };
    for pos in [HB as u64 - 1, HB as u64] {
        let mut st = initial_state(&p);
        st.pos = pos;
        let rows = (pos as usize).min(HB as usize - 1);
        st.hist.insert((h, None), (0..rows).map(|i| t(DType::I8, &[1], &[(i % 7) as i128 - 3])).collect());
        let (a, _) = ref2_step(&p, &Params::new(), &st, 5);
        let mut fst = state_to(&st);
        let f = first_step(&fp, &params_to(&Params::new()), &mut fst, 5);
        println!("p11 pos {pos} (H = {}): ref2 {} / first {}", (pos + 1).min(HB as u64), summary(&a), summary(&f));
        assert_eq!(a, f);
        assert_eq!(a.is_ok(), pos < HB as u64);
    }
}

/// Environment entries §9.2 (revision 2) does not mention: entries the closure never reads, keys
/// that name no state or no carry-in, entries of the wrong state kind. The text only says that
/// supplied entries the closure does not reach are ignored.
#[test]
fn p12_env_entries_the_closure_never_reads() {
    let p = hist_program();
    let params = Params::new();
    let row = |x: i128| t(DType::I16, &[2], &[x, -x]);
    let honest = || {
        let mut e = ConeEnv { token: Some(1), pos: 4, ..Default::default() };
        e.carry_in.insert(0, row(5));
        e.hist_prior.insert(0, vec![row(1), row(2)]);
        e.fixed.insert(1, t(DType::I32, &[2], &[3, -3]));
        e
    };
    let fp = match first_decode(&encode(&p)) {
        Outcome::Ok(fp) => fp,
        o => panic!("{o:?}"),
    };
    let cases: Vec<(&str, u16, Box<dyn Fn(&mut ConeEnv)>)> = vec![
        (
            "a Fixed entry for a state index that does not exist",
            5,
            Box::new(|e| {
                e.fixed.insert(9, t(DType::I8, &[1], &[0]));
            }),
        ),
        (
            "a history entry for a state index that does not exist",
            5,
            Box::new(|e| {
                e.hist_prior.insert(9, vec![]);
            }),
        ),
        (
            "a carry-in entry past the block's carry-ins",
            5,
            Box::new(|e| {
                e.carry_in.insert(7, t(DType::I8, &[1], &[0]));
            }),
        ),
        (
            "a Fixed entry keyed by a Hist state",
            5,
            Box::new(|e| {
                e.fixed.insert(0, t(DType::I16, &[2], &[0, 0]));
            }),
        ),
        (
            "a history entry keyed by a Fixed state",
            5,
            Box::new(|e| {
                e.hist_prior.insert(1, vec![]);
            }),
        ),
        (
            "a wrong-typed Fixed value the closure does not read (target 0)",
            0,
            Box::new(|e| {
                e.fixed.insert(1, t(DType::I8, &[7], &[0; 7]));
            }),
        ),
        (
            "a wrong-length history the closure does not read (target 4, node 3 supplied)",
            4,
            Box::new(|e| {
                e.hist_prior.insert(0, vec![]);
                e.supplied.insert(3, t(DType::I32, &[2], &[1, 1]));
            }),
        ),
    ];
    for (name, target, f) in cases {
        let mut e = honest();
        f(&mut e);
        let a = mine(eval_cone(&p, &params, 1, Some(1), target, &e));
        let fo = first_cone(&fp, &params_to(&params), 1, Some(1), target, &env_to(&e));
        println!(
            "p12 {}{name}: ref2 {} / first {}",
            if a == fo { "" } else { "DIFFER " },
            a.clone().map_tensor(),
            fo.clone().map_tensor()
        );
        // Unread entries are ignored by both (the text says so only for supplied nodes).
        assert!(a.is_ok() && a == fo, "{name}");
    }
    // An arity error outside a program (eval_primitive): NF-14 names arity a normal-form rule of a
    // node; a lone primitive has no node.
    let ins: Vec<Tensor> = (0..9).map(|i| t(DType::I8, &[1], &[i])).collect();
    let a = mine(eval_primitive(&Prim::Concat { axis: 0 }, &ins, DType::I8, &[9]));
    let f = first_eval_primitive(&Prim::Concat { axis: 0 }, &ins, DType::I8, &[9]);
    println!("p12 eval_primitive Concat of 9: ref2 {} / first {}", a.clone().map_tensor(), f.clone().map_tensor());
    let a = mine(eval_primitive(&Prim::Add, &ins[..1], DType::I8, &[1]));
    let f = first_eval_primitive(&Prim::Add, &ins[..1], DType::I8, &[1]);
    println!("p12 eval_primitive Add of 1: ref2 {} / first {}", a.clone().map_tensor(), f.clone().map_tensor());
}
