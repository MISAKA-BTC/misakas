//! The differential of RFC-0002 freeze criterion 4: this implementation against the first one
//! (`misaka-palw-tir`, a black box) on
//!   A. single primitives with random and range-extreme operands and random output types;
//!   B. random well-formed programs (this crate's generator): decode, every step's logits and
//!      commits, the run state, and cones built from honest steps;
//!   C. malformed bytes: random mutations of valid encodings;
//!   D. structural mutations of valid programs (every field class), re-encoded.
//! Both must agree on success versus failure (normative) and on every value; the error class is
//! diagnostic and only tallied. A panic on either side is a totality defect (PALW-TIR-34).
//!
//! `TIR_REF2_CASES` scales the case counts (default 1 = the counts below).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::bridge::*;
use common::progen::{GenCfg, R, gen_params, gen_program, mutate, pick, rand_profile, rand_value};
use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use rand::{Rng, SeedableRng};
use ref2::codec::{decode_canonical, decode_violations, encode};
use ref2::eval::{ConeEnv, Params, RunState, eval_cone, initial_state, step_traced, step_violations};
use ref2::{Class, Cmp, DType, Dim, Prim, Program, Ref, Rounding, StateKind, Tensor, TensorType, eval_primitive};

fn scale() -> usize {
    std::env::var("TIR_REF2_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

#[derive(Default, Debug)]
struct Tally {
    cases: usize,
    both_ok: usize,
    both_err: usize,
    class_same: usize,
    class_diff: BTreeMap<(Class, Class), usize>,
    class_examples: BTreeMap<(Class, Class), Vec<String>>,
    disagreements: Vec<String>,
    panics_first: Vec<String>,
    /// Both failed, and the first implementation's class is not the class of any rule the input
    /// breaks (§9.3, revision 2: "an input that breaks several reports the class of one of them").
    class_outside_set: Vec<String>,
    /// Cases whose violation set had exactly one class (the class is then fixed by the text).
    single_class: usize,
}

impl Tally {
    /// As [`Tally::record`], and when both fail, the first implementation's class must be the class
    /// of a rule the input breaks: one of `set` (this crate's violation classes), which must also
    /// contain this crate's own class.
    fn record_set<T: PartialEq + std::fmt::Debug>(
        &mut self,
        what: &str,
        a: &Outcome<T>,
        set: &std::collections::BTreeSet<Class>,
        f: &Outcome<T>,
    ) -> bool {
        let ok = self.record(what, a, f);
        if let (Outcome::Err(x), Outcome::Err(y)) = (a, f) {
            if set.len() == 1 {
                self.single_class += 1;
            }
            if !set.contains(x) {
                self.disagreements.push(format!("{what}: ref2's own class {} is not in its violation set {set:?}", x.name()));
            }
            if !set.contains(y) {
                self.class_outside_set.push(format!(
                    "{what}: first {} not among the broken rules' classes {set:?} (ref2 {})",
                    y.name(),
                    x.name()
                ));
                return false;
            }
        }
        ok
    }

    fn record<T: PartialEq + std::fmt::Debug>(&mut self, what: &str, a: &Outcome<T>, f: &Outcome<T>) -> bool {
        self.cases += 1;
        match (a, f) {
            (Outcome::Ok(x), Outcome::Ok(y)) => {
                if x == y {
                    self.both_ok += 1;
                    true
                } else {
                    self.disagreements.push(format!("{what}: values differ: ref2 {x:?} / first {y:?}"));
                    false
                }
            }
            (Outcome::Err(x), Outcome::Err(y)) => {
                self.both_err += 1;
                if x == y {
                    self.class_same += 1;
                } else {
                    *self.class_diff.entry((*x, *y)).or_default() += 1;
                    let ex = self.class_examples.entry((*x, *y)).or_default();
                    let cap = std::env::var("TIR_REF2_CLASS_EXAMPLES").ok().and_then(|v| v.parse().ok()).unwrap_or(4);
                    if ex.len() < cap {
                        ex.push(what.chars().take(160).collect());
                    }
                }
                true
            }
            (_, Outcome::Panic(s)) => {
                self.panics_first.push(format!("{what}: first panicked: {s}"));
                false
            }
            (Outcome::Panic(s), _) => {
                self.disagreements.push(format!("{what}: ref2 panicked: {s}"));
                false
            }
            _ => {
                self.disagreements.push(format!("{what}: ref2 {} / first {}", brief(a), brief(f)));
                false
            }
        }
    }

    fn report(&self, name: &str) {
        println!(
            "{name}: {} cases, {} both ok (identical), {} both failed ({} same class, {} single-class inputs), {} disagreements, {} classes outside the broken rules, {} first-impl panics",
            self.cases,
            self.both_ok,
            self.both_err,
            self.class_same,
            self.single_class,
            self.disagreements.len(),
            self.class_outside_set.len(),
            self.panics_first.len()
        );
        for d in self.class_outside_set.iter().take(15) {
            println!("  CLASS {d}");
        }
        for ((a, b), n) in &self.class_diff {
            println!("  class differs (both are classes of rules the input breaks) ref2 {} / first {}: {n}", a.name(), b.name());
            if std::env::var("TIR_REF2_CLASS_EXAMPLES").is_ok() {
                for e in &self.class_examples[&(*a, *b)] {
                    println!("      e.g. {e}");
                }
            }
        }
        for d in self.disagreements.iter().take(25) {
            println!("  DISAGREE {d}");
        }
        for d in self.panics_first.iter().take(10) {
            println!("  PANIC {d}");
        }
    }
}

fn brief<T: std::fmt::Debug>(o: &Outcome<T>) -> String {
    match o {
        Outcome::Ok(v) => {
            let s = format!("{v:?}");
            if s.len() > 300 { format!("ok {}…", &s[..300]) } else { format!("ok {s}") }
        }
        Outcome::Err(c) => format!("err {}", c.name()),
        Outcome::Panic(s) => format!("panic {s}"),
    }
}

// =============================================================== A. primitives

fn rand_shape(rng: &mut R, max_rank: usize) -> Vec<u64> {
    let r = rng.gen_range(0..=max_rank);
    (0..r).map(|_| pick(rng, &[1u64, 1, 2, 3, 4])).collect()
}

fn rand_t(rng: &mut R, dt: DType, shape: &[u64]) -> Tensor {
    let n: u64 = shape.iter().product();
    let prof = rand_profile(rng);
    let data = (0..n)
        .map(|_| {
            let pr = if rng.gen_bool(0.8) { prof } else { rand_profile(rng) };
            rand_value(rng, dt, pr)
        })
        .collect();
    Tensor::new(dt, shape.to_vec(), data).unwrap()
}

/// A random tensor of a random dtype.
fn rt(rng: &mut R, shape: &[u64]) -> Tensor {
    let dt = any_dt(rng);
    rand_t(rng, dt, shape)
}

fn any_dt(rng: &mut R) -> DType {
    pick(rng, &DType::ALL)
}

fn compat(rng: &mut R, s: &[u64]) -> Vec<u64> {
    let drop = if s.is_empty() { 0 } else { rng.gen_range(0..=s.len()) };
    s[drop..].iter().map(|&d| if rng.gen_bool(0.3) { 1 } else { d }).collect()
}

fn bshape(a: &[u64], b: &[u64]) -> Option<Vec<u64>> {
    let r = a.len().max(b.len());
    let mut out = vec![0; r];
    for i in 0..r {
        let da = if i + a.len() >= r { a[i + a.len() - r] } else { 1 };
        let db = if i + b.len() >= r { b[i + b.len() - r] } else { 1 };
        out[i] = if da == db || db == 1 {
            da
        } else if da == 1 {
            db
        } else {
            return None;
        };
    }
    Some(out)
}

/// Divisor/dividend pairs around exact halves.
fn half_case(rng: &mut R, dt_x: DType, dt_d: DType) -> (i128, i128) {
    let d = pick(
        rng,
        &[1i128, 2, 3, 4, 5, 7, 8, 16, 1 << 24, 1 << 31, (1 << 31) - 1, 1 << 62, i64::MAX as i128, i128::MAX, u32::MAX as i128],
    );
    let d = d.clamp(1, dt_d.max());
    let any: i128 = rng.gen_range(-1000..1000);
    let q: i128 = pick(rng, &[0i128, 1, 2, 3, 100, -1, -2, -3, -100, any]);
    let r: i128 = pick(rng, &[0i128, d / 2, d / 2 + 1, d / 2 + d % 2, d - 1, (d - 1) / 2]).min(d - 1);
    let x = q.checked_mul(d).and_then(|v: i128| v.checked_add(if q < 0 { -r } else { r })).unwrap_or(0);
    (x.clamp(dt_x.min(), dt_x.max()), d)
}

/// One random primitive case: (prim, operands, out dtype, out shape). Mostly well typed.
fn prim_case(rng: &mut R) -> (Prim, Vec<Tensor>, DType, Vec<u64>) {
    let k = rng.gen_range(0..23);
    let mut case = match k {
        0 => {
            let dt = any_dt(rng);
            let s = rand_shape(rng, 4);
            let n: u64 = s.iter().product();
            let mut out = Vec::new();
            let mut rem = n;
            while rem > 1 && out.len() < 3 {
                let divs: Vec<u64> = (1..=rem).filter(|d| rem.is_multiple_of(*d)).collect();
                let d = pick(rng, &divs);
                out.push(d);
                rem /= d;
            }
            if rem > 1 || rng.gen_bool(0.3) {
                out.push(rem);
            }
            (Prim::Reshape, vec![rand_t(rng, dt, &s)], dt, out)
        }
        1 => {
            let dt = any_dt(rng);
            let s = rand_shape(rng, 4);
            let mut perm: Vec<u8> = (0..s.len() as u8).collect();
            use rand::seq::SliceRandom;
            perm.shuffle(rng);
            let out = perm.iter().map(|&p| s[p as usize]).collect();
            (Prim::Transpose { perm }, vec![rand_t(rng, dt, &s)], dt, out)
        }
        2 => {
            let dt = any_dt(rng);
            let mut s = rand_shape(rng, 4);
            if s.is_empty() {
                s.push(3);
            }
            let a = rng.gen_range(0..s.len());
            let n = s[a];
            let len = rng.gen_range(1..=n);
            let start = rng.gen_range(0..=n - len) as u32;
            let mut out = s.clone();
            out[a] = len;
            (Prim::Slice { axis: a as u8, start }, vec![rand_t(rng, dt, &s)], dt, out)
        }
        3 => {
            let dt = any_dt(rng);
            let mut s = rand_shape(rng, 4);
            if s.is_empty() {
                s.push(2);
            }
            let a = rng.gen_range(0..s.len());
            let m = rng.gen_range(2..=8);
            let mut ins = Vec::new();
            let mut total = 0;
            for _ in 0..m {
                let mut si = s.clone();
                si[a] = pick(rng, &[1u64, 2, 3]);
                total += si[a];
                ins.push(rand_t(rng, dt, &si));
            }
            let mut out = s.clone();
            out[a] = total;
            (Prim::Concat { axis: a as u8 }, ins, dt, out)
        }
        4 => {
            let dt = any_dt(rng);
            let s = rand_shape(rng, 3);
            let extra = rng.gen_range(0..=4 - s.len());
            let mut out: Vec<u64> = (0..extra).map(|_| pick(rng, &[1u64, 2, 3])).collect();
            out.extend(s.iter().map(|&d| if d == 1 && rng.gen_bool(0.6) { pick(rng, &[2u64, 3, 4]) } else { d }));
            (Prim::Broadcast, vec![rand_t(rng, dt, &s)], dt, out)
        }
        5 => {
            let dt = any_dt(rng);
            let mut out = rand_shape(rng, 4);
            if out.is_empty() {
                out.push(4);
            }
            let axis = rng.gen_range(0..out.len()) as u8;
            let start = pick(rng, &[0i64, 1, -1, 127, -128, i64::MIN, i64::MAX, 1 << 31, -(1 << 31), 4294967295]);
            let step = pick(rng, &[0i64, 1, -1, 2, i64::MIN, i64::MAX, 1 << 32, -7]);
            (Prim::Iota { axis, start, step }, vec![], dt, out)
        }
        6 => {
            let dt = any_dt(rng);
            let rd = rng.gen_range(1..=3);
            let ds: Vec<u64> = (0..rd).map(|_| pick(rng, &[1u64, 2, 3, 4])).collect();
            let a = rng.gen_range(0..rd);
            let b = rng.gen_range(0..=a);
            let mut is: Vec<u64> = ds[..b].to_vec();
            for _ in 0..rng.gen_range(0..=(4 + b - rd).min(2)) {
                is.push(pick(rng, &[1u64, 2, 3]));
            }
            let idt = pick(rng, &[DType::Idx, DType::I8, DType::I16, DType::I32, DType::I64, DType::I128]);
            let n: u64 = is.iter().product();
            let ext = ds[a] as i128;
            let iv: Vec<i128> = (0..n)
                .map(|_| {
                    if rng.gen_bool(0.9) { rng.gen_range(0..ext) } else { pick(rng, &[-1, ext, i128::MIN, idt.max()]) }
                        .clamp(idt.min(), idt.max())
                })
                .collect();
            let mut out = ds[..a].to_vec();
            out.extend_from_slice(&is[b..]);
            out.extend_from_slice(&ds[a + 1..]);
            (
                Prim::Gather { axis: a as u8, batch_dims: b as u8 },
                vec![rand_t(rng, dt, &ds), Tensor::new(idt, is, iv).unwrap()],
                dt,
                out,
            )
        }
        7 => {
            let s = rand_shape(rng, 3);
            let (a, b) = (any_dt(rng), any_dt(rng));
            (Prim::Cast, vec![rand_t(rng, a, &s)], b, s)
        }
        8..=10 => {
            let s = rand_shape(rng, 3);
            let s2 = compat(rng, &s);
            let (s, s2) = if rng.gen_bool(0.5) { (s, s2) } else { (s2, s) };
            let out = bshape(&s, &s2).unwrap();
            let prim = [Prim::Add, Prim::Sub, Prim::Mul][k - 8].clone();
            let ins = vec![rt(rng, &s), rt(rng, &s2)];
            (prim, ins, any_dt(rng), out)
        }
        11 => {
            let ok = [DType::I8, DType::I16, DType::I32, DType::I64];
            let batch = rand_shape(rng, 2);
            let (m, kk, n) = (pick(rng, &[1u64, 2, 3]), pick(rng, &[1u64, 2, 3, 5]), pick(rng, &[1u64, 2, 3]));
            let mut sa = compat(rng, &batch);
            sa.extend([m, kk]);
            let mut sb = compat(rng, &batch);
            sb.extend([kk, n]);
            let mut out = bshape(&sa[..sa.len() - 2], &sb[..sb.len() - 2]).unwrap();
            out.extend([m, n]);
            let od = pick(rng, &[DType::I8, DType::I16, DType::I32, DType::I64, DType::I128]);
            let (da, db) = (pick(rng, &ok), pick(rng, &ok));
            let ins = vec![rand_t(rng, da, &sa), rand_t(rng, db, &sb)];
            (Prim::MatMul, ins, od, out)
        }
        12 | 13 => {
            let dt = any_dt(rng);
            let mut s = rand_shape(rng, 4);
            if s.is_empty() {
                s.push(4);
            }
            let a = rng.gen_range(0..s.len());
            let mut out = s.clone();
            out[a] = 1;
            if k == 12 {
                (Prim::ReduceSum { axis: a as u8 }, vec![rand_t(rng, dt, &s)], any_dt(rng), out)
            } else {
                (Prim::ReduceMax { axis: a as u8 }, vec![rand_t(rng, dt, &s)], dt, out)
            }
        }
        14 => {
            let s = rand_shape(rng, 3);
            let s2 = compat(rng, &s);
            let (xd, dd) = (any_dt(rng), any_dt(rng));
            let n1: u64 = s.iter().product();
            let n2: u64 = s2.iter().product();
            let mut xs = Vec::new();
            let mut ds = Vec::new();
            for _ in 0..n1.max(n2) {
                let (x, d): (i128, i128) = if rng.gen_bool(0.7) {
                    half_case(rng, xd, dd)
                } else {
                    let (p1, p2) = (rand_profile(rng), rand_profile(rng));
                    (rand_value(rng, xd, p1), rand_value(rng, dd, p2))
                };
                xs.push(x);
                ds.push(d.clamp(dd.min(), dd.max()));
            }
            xs.truncate(n1 as usize);
            ds.truncate(n2 as usize);
            let rule = pick(rng, &[Rounding::Floor, Rounding::HalfUp, Rounding::HalfAwayFromZero]);
            let out = bshape(&s, &s2).unwrap();
            (Prim::Div { rule }, vec![Tensor::new(xd, s, xs).unwrap(), Tensor::new(dd, s2, ds).unwrap()], any_dt(rng), out)
        }
        15 => {
            let s = rand_shape(rng, 3);
            let od = any_dt(rng);
            let (p1, p2) = (rand_profile(rng), rand_profile(rng));
            let lo = rand_value(rng, od, p1).clamp(i64::MIN as i128, i64::MAX as i128) as i64;
            let hi = rand_value(rng, od, p2).clamp(i64::MIN as i128, i64::MAX as i128) as i64;
            let (lo, hi) = if rng.gen_bool(0.9) { (lo.min(hi), lo.max(hi)) } else { (lo, hi) };
            (Prim::Clamp { lo, hi }, vec![rt(rng, &s)], od, s)
        }
        16..=19 => {
            let s = rand_shape(rng, 3);
            let prim = [Prim::Log2Floor, Prim::IntExp, Prim::IntRsqrt, Prim::IntLn][k - 16].clone();
            let xd = if k == 16 {
                any_dt(rng)
            } else {
                pick(rng, &[DType::I8, DType::I16, DType::I32, DType::I64, DType::Idx, DType::I64])
            };
            let x = if k == 17 {
                // IntExp's range-reduction bucket edges and the saturation threshold.
                let n: u64 = s.iter().product();
                let ln2 = 11_629_080i128;
                let v: Vec<i128> = (0..n)
                    .map(|_| {
                        let z = rng.gen_range(0..=32i128);
                        let any = rand_value(rng, xd, 1);
                        (pick(rng, &[-z * ln2, -z * ln2 + 1, -z * ln2 - 1, -31 * ln2, -31 * ln2 + 1, 0, 1, any]))
                            .clamp(xd.min(), xd.max())
                    })
                    .collect();
                Tensor::new(xd, s.clone(), v).unwrap()
            } else {
                rand_t(rng, xd, &s)
            };
            (prim, vec![x], pick(rng, &[DType::I32, DType::I64, DType::I128, DType::Idx, DType::I16, DType::I8]), s)
        }
        20 => {
            let s = rand_shape(rng, 3);
            let s2 = compat(rng, &s);
            let out = bshape(&s, &s2).unwrap();
            let cmp = pick(rng, &[Cmp::Eq, Cmp::Ne, Cmp::Lt, Cmp::Le, Cmp::Gt, Cmp::Ge]);
            (Prim::Compare { cmp }, vec![rt(rng, &s), rt(rng, &s2)], DType::I8, out)
        }
        21 => {
            let s = rand_shape(rng, 3);
            let s2 = compat(rng, &s);
            let s3 = compat(rng, &s);
            let out = bshape(&bshape(&s, &s2).unwrap(), &s3).unwrap();
            (Prim::Select, vec![rt(rng, &s), rt(rng, &s2), rt(rng, &s3)], any_dt(rng), out)
        }
        _ => {
            let dt = any_dt(rng);
            let mut s = rand_shape(rng, 3);
            if s.is_empty() {
                s.push(5);
            }
            let a = rng.gen_range(0..s.len());
            let kk = rng.gen_range(1..=s[a]);
            let mut out = s.clone();
            out[a] = kk;
            // Ties: small value ranges.
            let n: u64 = s.iter().product();
            let x =
                Tensor::new(
                    dt,
                    s.clone(),
                    (0..n)
                        .map(|_| {
                            if rng.gen_bool(0.7) {
                                rng.gen_range(-2..=2i128).clamp(dt.min(), dt.max())
                            } else {
                                rand_value(rng, dt, 2)
                            }
                        })
                        .collect(),
                )
                .unwrap();
            (Prim::TopK { axis: a as u8, k: kk as u32 }, vec![x], DType::Idx, out)
        }
    };
    // Occasionally perturb the output type (type rules).
    if rng.gen_bool(0.08) {
        case.2 = any_dt(rng);
    }
    if rng.gen_bool(0.05) && !case.3.is_empty() {
        let i = rng.gen_range(0..case.3.len());
        case.3[i] = pick(rng, &[1u64, 2, 3, 5]);
    }
    if rng.gen_bool(0.03) {
        case.3.push(1);
    }
    case
}

#[test]
fn a_primitive_differential() {
    let n = 40_000 * scale();
    let mut rng = R::seed_from_u64(0xA11CE);
    let mut t = Tally::default();
    let mut per_prim: BTreeMap<&'static str, (usize, usize)> = BTreeMap::new();
    for i in 0..n {
        let (prim, ins, od, os) = prim_case(&mut rng);
        let a = mine(eval_primitive(&prim, &ins, od, &os));
        let f = first_eval_primitive(&prim, &ins, od, &os);
        let e = per_prim.entry(prim.name()).or_default();
        e.0 += 1;
        if a.is_ok() {
            e.1 += 1;
        }
        let what = format!(
            "case {i} {prim:?} ins {:?} out {} {:?}",
            ins.iter().map(|x| (x.dtype.name(), x.shape.clone(), x.data.clone())).collect::<Vec<_>>(),
            od.name(),
            os
        );
        t.record(&what, &a, &f);
    }
    t.report("A. primitives");
    for (p, (c, ok)) in &per_prim {
        println!("  {p}: {c} cases, {ok} succeeded");
    }
    assert!(t.disagreements.is_empty() && t.panics_first.is_empty());
}

/// Accumulations at the exact edge of the order-free sum rule (§6.3, PALW-TIR-24): the largest
/// positive and negative totals that fit, one unit past them, long contractions, and totals that fit
/// while a partial sum does not.
#[test]
fn a2_maximal_accumulations() {
    let t = |dt: DType, shape: &[u64], v: Vec<i128>| Tensor::new(dt, shape.to_vec(), v).unwrap();
    let rep = |x: i128, n: usize| vec![x; n];
    let mut cases: Vec<(String, Prim, Vec<Tensor>, DType, Vec<u64>, bool)> = Vec::new();
    // ReduceSum of i8 into i16: 258·127 + 1 = 32,767 fits; + 2 does not; the negative side.
    for (extra, ok) in [(1i128, true), (2, false)] {
        let mut v = rep(127, 258);
        v.push(extra);
        cases.push((
            format!("ReduceSum 258·127 + {extra} into i16"),
            Prim::ReduceSum { axis: 0 },
            vec![t(DType::I8, &[259], v)],
            DType::I16,
            vec![1],
            ok,
        ));
    }
    for (n, ok) in [(256usize, true), (257, false)] {
        cases.push((
            format!("ReduceSum {n}·(−128) into i16"),
            Prim::ReduceSum { axis: 0 },
            vec![t(DType::I8, &[n as u64], rep(-128, n))],
            DType::I16,
            vec![1],
            ok,
        ));
    }
    // The total fits, a partial sum does not (positive and negative terms).
    let mut v = rep(127, 300);
    v.extend(rep(-128, 300));
    cases.push((
        "ReduceSum ±, total −300, positive part 38,100 above i16".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::I8, &[600], v)],
        DType::I16,
        vec![1],
        false,
    ));
    // MatMul i8·i8 with K = 2^17 into i32: 2^17·16,384 = 2^31 overflows; 2^17 − 1 fits.
    for (k, ok) in [((1u64 << 17) - 1, true), (1 << 17, false)] {
        cases.push((
            format!("MatMul (−128)² × {k} into i32"),
            Prim::MatMul,
            vec![t(DType::I8, &[1, k], rep(-128, k as usize)), t(DType::I8, &[k, 1], rep(-128, k as usize))],
            DType::I32,
            vec![1, 1],
            ok,
        ));
    }
    // i16 into i32 at both ends: (−32768)² × 2 = 2^31 overflows; the negative sum
    // −32768·(32767 + 32767 + 2) = −2^31 exactly fits, one more −32768 does not.
    cases.push((
        "MatMul i16 (−32768)·(−32768) × 2 into i32 (2^31)".into(),
        Prim::MatMul,
        vec![t(DType::I16, &[1, 2], rep(-32768, 2)), t(DType::I16, &[2, 1], rep(-32768, 2))],
        DType::I32,
        vec![1, 1],
        false,
    ));
    for (last, ok) in [(2i128, true), (3, false)] {
        cases.push((
            format!("MatMul i16 −32768·(32767 + 32767 + {last}) into i32"),
            Prim::MatMul,
            vec![t(DType::I16, &[1, 3], rep(-32768, 3)), t(DType::I16, &[3, 1], vec![32767, 32767, last])],
            DType::I32,
            vec![1, 1],
            ok,
        ));
    }
    // i64·i64 into i128: two products of 2^126 overflow; the most negative pair fits.
    cases.push((
        "MatMul i64::MIN² × 2 into i128 (2^127)".into(),
        Prim::MatMul,
        vec![t(DType::I64, &[1, 2], rep(i64::MIN as i128, 2)), t(DType::I64, &[2, 1], rep(i64::MIN as i128, 2))],
        DType::I128,
        vec![1, 1],
        false,
    ));
    cases.push((
        "MatMul i64::MIN·i64::MAX × 2 into i128".into(),
        Prim::MatMul,
        vec![t(DType::I64, &[1, 2], rep(i64::MIN as i128, 2)), t(DType::I64, &[2, 1], rep(i64::MAX as i128, 2))],
        DType::I128,
        vec![1, 1],
        true,
    ));
    // i32·i32 into i64 with K = 2: 2·2^62 = 2^63 overflows; (2^31 − 1)² · 2 fits.
    cases.push((
        "MatMul i32::MIN² × 2 into i64".into(),
        Prim::MatMul,
        vec![t(DType::I32, &[1, 2], rep(i32::MIN as i128, 2)), t(DType::I32, &[2, 1], rep(i32::MIN as i128, 2))],
        DType::I64,
        vec![1, 1],
        false,
    ));
    cases.push((
        "MatMul i32::MAX² × 2 into i64".into(),
        Prim::MatMul,
        vec![t(DType::I32, &[1, 2], rep(i32::MAX as i128, 2)), t(DType::I32, &[2, 1], rep(i32::MAX as i128, 2))],
        DType::I64,
        vec![1, 1],
        true,
    ));
    // ReduceSum of i128 extremes: MAX + MIN + … never fits once P > MAX.
    cases.push((
        "ReduceSum [i128::MIN, i128::MIN] into i128".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::I128, &[2], vec![i128::MIN, i128::MIN])],
        DType::I128,
        vec![1],
        false,
    ));
    cases.push((
        "ReduceSum [i128::MIN, 0, i128::MAX] into i128".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::I128, &[3], vec![i128::MIN, 0, i128::MAX])],
        DType::I128,
        vec![1],
        true,
    ));
    // idx output: any negative term fails even when the total is positive.
    cases.push((
        "ReduceSum [5, −1] into idx".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::I32, &[2], vec![5, -1])],
        DType::Idx,
        vec![1],
        false,
    ));
    cases.push((
        "ReduceSum [2^31, 2^31 − 1] into idx".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::Idx, &[2], vec![1 << 31, (1 << 31) - 1])],
        DType::Idx,
        vec![1],
        true,
    ));
    cases.push((
        "ReduceSum [2^31, 2^31] into idx".into(),
        Prim::ReduceSum { axis: 0 },
        vec![t(DType::Idx, &[2], vec![1 << 31, 1 << 31])],
        DType::Idx,
        vec![1],
        false,
    ));
    let mut tally = Tally::default();
    for (name, prim, ins, od, os, ok) in &cases {
        let a = mine(eval_primitive(prim, ins, *od, os));
        let f = first_eval_primitive(prim, ins, *od, os);
        println!(
            "a2 {name}: text {} ref2 {} first {}",
            if *ok { "fits" } else { "Overflow" },
            brief(&a).chars().take(90).collect::<String>(),
            brief(&f).chars().take(90).collect::<String>()
        );
        assert_eq!(a.is_ok(), *ok, "{name}: ref2 against the text");
        tally.record(name, &a, &f);
    }
    tally.report("A2. maximal accumulations");
    assert!(tally.disagreements.is_empty() && tally.panics_first.is_empty());
}

// =============================================================== B. programs

fn commits_of(o: &Outcome<(Tensor, Commits)>) -> Outcome<(Tensor, Commits)> {
    o.clone()
}

/// Normalises a run state for comparison: the first implementation materialises instances lazily
/// (an absent Fixed instance is all zeros, an absent history is empty), this one eagerly.
fn normalise(p: &Program, st: &RunState) -> RunState {
    let mut s = st.clone();
    s.fixed.retain(|(j, _), t| {
        let d = &p.states[*j as usize];
        let _ = d;
        t.data.iter().any(|&v| v != 0)
    });
    s.hist.retain(|_, rows| !rows.is_empty());
    s
}

#[derive(Default)]
struct ProgStats {
    programs: usize,
    decode: Tally,
    steps: Tally,
    states: Tally,
    cones: Tally,
    cones_subset: Tally,
    cone_matches_step: usize,
    cone_mismatch_step: Vec<String>,
    h_programs: usize,
    windows_hit: usize,
    ok_steps_with_h: usize,
    prim_seen: BTreeMap<&'static str, usize>,
    /// Features of programs with at least one step both implementations completed.
    features_ok: BTreeMap<String, usize>,
    fail_classes: BTreeMap<&'static str, usize>,
    refused_programs: usize,
}

/// The structural features of a program the differential should cover.
fn features(p: &Program) -> Vec<String> {
    let mut f = Vec::new();
    let is_h = |t: &TensorType| t.shape.contains(&Dim::H);
    for (bi, b) in p.blocks.iter().enumerate() {
        let role = ref2::normal_form::role_of(p, bi);
        let ty = |r: &Ref, i: usize| ref2::normal_form::ref_type(p, bi, i, r);
        for (i, n) in b.nodes.iter().enumerate() {
            let ins: Vec<TensorType> = n.inputs.iter().filter_map(|r| ty(r, i)).collect();
            let any_h = ins.iter().any(is_h) || is_h(&n.out);
            if any_h {
                f.push(format!("{} with H", n.prim.name()));
            }
            match &n.prim {
                Prim::MatMul if ins.len() == 2 && ins[0].shape.last() == Some(&Dim::H) => f.push("MatMul contracting H".into()),
                Prim::ReduceSum { axis } | Prim::ReduceMax { axis }
                    if ins.first().is_some_and(|t| t.shape.get(*axis as usize) == Some(&Dim::H)) =>
                {
                    f.push(format!("{} along H", n.prim.name()))
                }
                Prim::Gather { batch_dims, .. } if *batch_dims > 0 => f.push("Gather batch_dims > 0".into()),
                Prim::HistAppend { state } => {
                    let s = &p.states[*state as usize];
                    if let StateKind::Hist { window } = s.kind {
                        f.push(format!(
                            "HistAppend window {}",
                            if window >= 1 << 18 { "history_bound".to_string() } else { window.to_string() }
                        ));
                    }
                    f.push(format!("HistAppend {}", if s.per_layer { "per-layer" } else { "global" }));
                }
                Prim::StateWrite { state } => {
                    f.push(format!("StateWrite {}", if p.states[*state as usize].per_layer { "per-layer" } else { "global" }))
                }
                _ => {}
            }
            for r in &n.inputs {
                match r {
                    Ref::Input(0) => f.push("reads the token".into()),
                    Ref::Input(_) => f.push("reads pos".into()),
                    Ref::Param(j) if p.params[*j as usize].per_layer => f.push("per-layer param".into()),
                    Ref::State(_) => f.push(format!("State read in {:?}", role)),
                    _ => {}
                }
            }
        }
    }
    let globals_written: Vec<u16> = p.blocks[p.schedule.pre as usize]
        .nodes
        .iter()
        .filter_map(|n| if let Prim::StateWrite { state } = n.prim { Some(state) } else { None })
        .collect();
    if p.blocks[p.schedule.post as usize]
        .nodes
        .iter()
        .any(|n| matches!(n.prim, Prim::StateWrite { state } if globals_written.contains(&state)))
    {
        f.push("a global state written by pre and post".into());
    }
    if p.schedule.layers.len() > p.schedule.layers.iter().collect::<std::collections::BTreeSet<_>>().len() {
        f.push("a layer block at several layers".into());
    }
    f.sort();
    f.dedup();
    f
}

fn run_program_case(seed: u64, cfg: GenCfg, st: &mut ProgStats) {
    let mut rng = R::seed_from_u64(seed);
    let g = gen_program(&mut rng, cfg);
    let bytes = encode(&g.prog);
    st.programs += 1;
    let (a, set) = match decode_violations(&bytes) {
        Ok(_) => (Outcome::Ok(()), BTreeSet::new()),
        Err(set) => (Outcome::Err(decode_canonical(&bytes).err().unwrap().class), set),
    };
    if !cfg.post_writes && !a.is_ok() {
        panic!("seed {seed}: this crate's generator made a program this crate refuses: {:?}", decode_canonical(&bytes).err());
    }
    if !a.is_ok() {
        st.refused_programs += 1;
    }
    let f = first_decode(&bytes);
    let fa: Outcome<()> = match &f {
        Outcome::Ok(_) => Outcome::Ok(()),
        Outcome::Err(c) => Outcome::Err(*c),
        Outcome::Panic(s) => Outcome::Panic(s.clone()),
    };
    if !st.decode.record_set(&format!("seed {seed} decode"), &a, &set, &fa) || !a.is_ok() {
        return;
    }
    let Outcome::Ok(fp) = f else { return };
    // The first implementation's re-encoding must be byte-identical (§4 is the interface).
    assert_eq!(fp.encode(), bytes, "seed {seed}: first re-encodes differently");
    let p = &g.prog;
    for b in &p.blocks {
        for n in &b.nodes {
            *st.prim_seen.entry(n.prim.name()).or_default() += 1;
        }
    }
    let has_hist = p.states.iter().any(|s| matches!(s.kind, StateKind::Hist { .. }));
    if has_hist {
        st.h_programs += 1;
    }
    let fparams = params_to(&g.params);
    let mut mst = initial_state(p);
    let mut fst = first::interp::RunState::default();
    let steps = rng.gen_range(1..=7);
    let mut counted = false;
    for s in 0..steps {
        let token: u64 = if rng.gen_bool(0.9) || p.token_bound == u32::MAX {
            rng.gen_range(0..p.token_bound.min(64) as u64)
        } else {
            pick(&mut rng, &[p.token_bound as u64, u32::MAX as u64])
        };
        let traced = step_traced(p, &g.params, &mst, token);
        let a: Outcome<(Tensor, Commits)> = match &traced {
            Ok((o, _, _)) => Outcome::Ok((
                o.logits.clone(),
                o.commits.iter().map(|c| (c.slot, c.block, c.layer, c.node, c.value.clone())).collect(),
            )),
            Err(e) => Outcome::Err(e.class),
        };
        let before = fst.clone();
        let f = first_step(&fp, &fparams, &mut fst, token as u32);
        let what = format!("seed {seed} step {s} pos {} token {token}", mst.pos);
        if let Outcome::Err(c) = &a {
            *st.fail_classes.entry(c.name()).or_default() += 1;
        }
        if a.is_ok() && f.is_ok() && !counted {
            counted = true;
            for x in features(p) {
                *st.features_ok.entry(x).or_default() += 1;
            }
        }
        let set = if a.is_ok() { BTreeSet::new() } else { step_violations(p, &g.params, &mst, token) };
        st.steps.record_set(&what, &commits_of(&a), &set, &f);
        if !f.is_ok() && fst != before {
            st.steps.disagreements.push(format!("{what}: first changed its run state on a failed step"));
        }
        if let Ok((_, next, trace)) = traced {
            if has_hist {
                st.ok_steps_with_h += 1;
            }
            // Cones from this honest step.
            cones_for_step(seed, p, &g.params, &fp, &fparams, &mst, token, &trace, &mut rng, st);
            mst = next;
            if f.is_ok() {
                let fs = state_from(&fst);
                let (x, y) = (normalise(p, &mst), normalise(p, &fs));
                st.states.record(&format!("{what} run state"), &Outcome::Ok(x), &Outcome::Ok(y));
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn cones_for_step(
    seed: u64,
    p: &Program,
    params: &Params,
    fp: &first::program::TirProgramV1,
    fparams: &first::interp::MapParams,
    st: &RunState,
    token: u64,
    trace: &[ref2::eval::OccTrace],
    rng: &mut R,
    stats: &mut ProgStats,
) {
    for occ in trace {
        let b = occ.block as usize;
        let block = &p.blocks[b];
        let inst = |j: u16| if p.states[j as usize].per_layer { occ.layer } else { None };
        let mut env = ConeEnv { token: Some(token), pos: st.pos, carry_in: occ.carry_in.clone(), ..Default::default() };
        for n in &block.nodes {
            for r in &n.inputs {
                if let Ref::State(j) = *r
                    && let Some(v) = st.fixed.get(&(j, inst(j)))
                {
                    env.fixed.insert(j, v.clone());
                }
            }
            if let Prim::HistAppend { state } = n.prim {
                env.hist_prior.insert(state, st.hist.get(&(state, inst(state))).cloned().unwrap_or_default());
            }
        }
        // Targets: every commit point (the court's case) and a few random nodes.
        let mut targets: Vec<u16> = (0..block.nodes.len() as u16).filter(|&i| block.nodes[i as usize].commit).collect();
        for _ in 0..2 {
            targets.push(rng.gen_range(0..block.nodes.len()) as u16);
        }
        for &target in &targets {
            // (1) The court's environment: every other commit point supplied.
            let mut e = env.clone();
            for (i, n) in block.nodes.iter().enumerate() {
                if n.commit && i as u16 != target {
                    e.supplied.insert(i as u16, occ.values[i].clone());
                }
            }
            let what = format!("seed {seed} pos {} cone ({}, {:?}) target {target}", st.pos, occ.block, occ.layer);
            let a = mine(eval_cone(p, params, occ.block, occ.layer, target, &e));
            let f = first_cone(fp, fparams, occ.block, occ.layer, target, &env_to(&e));
            stats.cones.record(&what, &a, &f);
            match &a {
                Outcome::Ok(v) if *v == occ.values[target as usize] => stats.cone_matches_step += 1,
                o => stats.cone_mismatch_step.push(format!("{what}: ref2 cone {} vs step value", brief(o))),
            }
            // (2) A random subset of honest node values supplied (never the target).
            let mut e = env.clone();
            for (i, v) in occ.values.iter().enumerate() {
                if i as u16 != target && rng.gen_bool(0.3) {
                    e.supplied.insert(i as u16, v.clone());
                }
            }
            let a = mine(eval_cone(p, params, occ.block, occ.layer, target, &e));
            let f = first_cone(fp, fparams, occ.block, occ.layer, target, &env_to(&e));
            stats.cones_subset.record(&format!("{what} (random supplied subset)"), &a, &f);
            if let Outcome::Ok(v) = &a
                && *v != occ.values[target as usize]
            {
                stats.cone_mismatch_step.push(format!("{what}: ref2 subset cone differs from the step"));
            }
        }
    }
}

#[test]
fn b_program_differential() {
    let n = 1500 * scale() as u64;
    let mut st = ProgStats::default();
    for seed in 0..n {
        run_program_case(seed, GenCfg::default(), &mut st);
    }
    println!(
        "B. {} random programs ({} with histories, {} successful steps of those)",
        st.programs, st.h_programs, st.ok_steps_with_h
    );
    st.decode.report("B. decode");
    st.steps.report("B. steps");
    st.states.report("B. run states after each step");
    st.cones.report("B. cones (court env: every other commit point supplied)");
    st.cones_subset.report("B. cones (random honest subset supplied)");
    println!("B. ref2 cones equal to the step's value: {} ({} not)", st.cone_matches_step, st.cone_mismatch_step.len());
    for m in st.cone_mismatch_step.iter().take(10) {
        println!("  {m}");
    }
    println!("B. primitives exercised: {:?}", st.prim_seen);
    println!("B. ref2 failure classes of steps: {:?}", st.fail_classes);
    println!("B. features of programs with at least one step both completed:");
    for (k, v) in &st.features_ok {
        println!("    {v:6}  {k}");
    }
    let _ = st.windows_hit;
    assert!(st.cone_mismatch_step.is_empty());
    for t in [&st.decode, &st.steps, &st.states, &st.cones, &st.cones_subset] {
        assert!(t.disagreements.is_empty() && t.panics_first.is_empty() && t.class_outside_set.is_empty());
    }
}

#[test]
fn b2_post_writes_are_refused() {
    // Revision 2's NF-19: post contains no StateWrite and no HistAppend (so no global state has two
    // writers). Programs whose post writes states — also states pre writes, also histories — must be
    // refused by both, with NF-19's class (NormalForm) or the class of another rule they break.
    let n = 300 * scale() as u64;
    let mut st = ProgStats::default();
    for seed in 0..n {
        run_program_case(1_000_000 + seed, GenCfg { post_writes: true, ..GenCfg::default() }, &mut st);
    }
    println!("B2. {} programs, {} of them with a state write in post (refused by ref2)", st.programs, st.refused_programs);
    st.decode.report("B2. decode");
    st.steps.report("B2. steps of the programs without post writes");
    assert!(st.refused_programs > st.programs / 4);
    assert!(st.decode.disagreements.is_empty() && st.decode.class_outside_set.is_empty() && st.decode.panics_first.is_empty());
    assert!(st.steps.disagreements.is_empty() && st.steps.class_outside_set.is_empty());
}

// =============================================================== C. malformed bytes

#[test]
fn c_malformed_bytes() {
    let n = 400 * scale() as u64;
    let mut t = Tally::default();
    let mut both_accept_reencode_ok = 0;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xC0DE_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let bytes = encode(&g.prog);
        for m in 0..40 {
            let mut b = bytes.clone();
            let kind = rng.gen_range(0..8);
            match kind {
                0 => {
                    let i = rng.gen_range(0..b.len());
                    b[i] ^= 1 << rng.gen_range(0..8);
                }
                1 => {
                    let i = rng.gen_range(0..b.len());
                    b[i] = rng.r#gen();
                }
                2 => {
                    let i = rng.gen_range(0..=b.len());
                    b.truncate(i);
                }
                3 => {
                    let i = rng.gen_range(0..=b.len());
                    b.insert(i, rng.r#gen());
                }
                4 => {
                    if !b.is_empty() {
                        let i = rng.gen_range(0..b.len());
                        b.remove(i);
                    }
                }
                5 => b.push(rng.r#gen()),
                6 => {
                    // A small integer change to a 4-byte little-endian field (counts, dims).
                    if b.len() > 4 {
                        let i = rng.gen_range(0..b.len() - 4);
                        let v = u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
                        let v = v.wrapping_add(pick(&mut rng, &[1u32, u32::MAX, 2, 1 << 24]));
                        b[i..i + 4].copy_from_slice(&v.to_le_bytes());
                    }
                }
                _ => {
                    // Tags and bools: a byte set to 0..=30.
                    let i = rng.gen_range(0..b.len());
                    b[i] = rng.gen_range(0..=30);
                }
            }
            if b == bytes {
                continue;
            }
            let a = mine(decode_canonical(&b));
            let f = first_decode(&b);
            let (a2, f2): (Outcome<Vec<u8>>, Outcome<Vec<u8>>) = (
                match &a {
                    Outcome::Ok(p) => Outcome::Ok(encode(p)),
                    Outcome::Err(c) => Outcome::Err(*c),
                    Outcome::Panic(s) => Outcome::Panic(s.clone()),
                },
                match &f {
                    Outcome::Ok(p) => Outcome::Ok(p.encode()),
                    Outcome::Err(c) => Outcome::Err(*c),
                    Outcome::Panic(s) => Outcome::Panic(s.clone()),
                },
            );
            let set = decode_violations(&b).err().unwrap_or_default();
            if t.record_set(&format!("seed {seed} mutation {m} kind {kind} ({} bytes)", b.len()), &a2, &set, &f2) && a2.is_ok() {
                both_accept_reencode_ok += 1;
            }
        }
    }
    t.report("C. malformed bytes");
    println!("C. mutations both accepted (and both re-encode to the input): {both_accept_reencode_ok}");
    assert!(t.disagreements.is_empty() && t.panics_first.is_empty() && t.class_outside_set.is_empty());
}

// =============================================================== D. structural mutations

#[test]
fn d_structural_mutations() {
    let n = 1500 * scale() as u64;
    let mut t = Tally::default();
    let mut runs = Tally::default();
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xD00D_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        for m in 0..6 {
            let mut p = g.prog.clone();
            let what = mutate(&mut rng, &mut p);
            let bytes = encode(&p);
            let a = mine(decode_canonical(&bytes));
            let f = first_decode(&bytes);
            let (a2, f2): (Outcome<()>, Outcome<()>) = (
                match &a {
                    Outcome::Ok(_) => Outcome::Ok(()),
                    Outcome::Err(c) => Outcome::Err(*c),
                    Outcome::Panic(s) => Outcome::Panic(s.clone()),
                },
                match &f {
                    Outcome::Ok(_) => Outcome::Ok(()),
                    Outcome::Err(c) => Outcome::Err(*c),
                    Outcome::Panic(s) => Outcome::Panic(s.clone()),
                },
            );
            let set = decode_violations(&bytes).err().unwrap_or_default();
            if std::env::var("TIR_REF2_SHOW_SETS").is_ok()
                && let (Outcome::Err(x), Outcome::Err(y)) = (&a2, &f2)
                && x != y
            {
                let rules: Vec<String> = ref2::codec::decode_structural(&bytes)
                    .map(|q| ref2::normal_form::violations(&q).into_iter().map(|v| format!("{} {}", v.rule, v.reason)).collect())
                    .unwrap_or_default();
                println!("  SET seed {seed} #{m} {what}: ref2 {} first {}: {rules:?}", x.name(), y.name());
            }
            t.record_set(&format!("seed {seed} #{m} {what}"), &a2, &set, &f2);
            // A mutant both accept is a different valid program: run it on both.
            if let (Outcome::Ok(mp), Outcome::Ok(fp)) = (a, f) {
                let params = gen_params(&mut rng, &mp);
                let fparams = params_to(&params);
                let mut mst = initial_state(&mp);
                let mut fst = first::interp::RunState::default();
                for s in 0..3 {
                    let token = rng.gen_range(0..mp.token_bound.min(16) as u64);
                    let (x, next) = ref2_step(&mp, &params, &mst, token);
                    let y = first_step(&fp, &fparams, &mut fst, token as u32);
                    let set = if x.is_ok() { BTreeSet::new() } else { step_violations(&mp, &params, &mst, token) };
                    runs.record_set(&format!("seed {seed} #{m} {what} step {s}"), &x, &set, &y);
                    if let Some(nx) = next {
                        mst = nx;
                    }
                }
            }
        }
    }
    t.report("D. structural mutations (decode)");
    runs.report("D. steps of mutants both accept");
    assert!(t.disagreements.is_empty() && t.panics_first.is_empty() && t.class_outside_set.is_empty());
    assert!(runs.disagreements.is_empty() && runs.panics_first.is_empty() && runs.class_outside_set.is_empty());
}

// =============================================================== B3. hostile cone environments

/// The nodes a cone evaluates: the backward closure of `target` stopping at supplied nodes (the
/// target itself always evaluated), in this crate's reading of §9.2.
fn closure(p: &Program, b: usize, target: u16, supplied: &BTreeMap<u16, Tensor>) -> Vec<bool> {
    let nodes = &p.blocks[b].nodes;
    let mut todo = vec![false; nodes.len()];
    todo[target as usize] = true;
    for i in (0..=target as usize).rev() {
        if todo[i] {
            for r in &nodes[i].inputs {
                if let Ref::Node(k) = *r
                    && !supplied.contains_key(&k)
                {
                    todo[k as usize] = true;
                }
            }
        }
    }
    todo
}

#[test]
fn b3_hostile_cone_envs() {
    // Each perturbation of an honest court environment and the verdict 04b §9.2/§9.3 (revision 2)
    // gives it: `None` = succeeds with the honest value, `Some(class)` = refused with that class.
    use Class::*;
    const KINDS: [(&str, Option<Class>); 18] = [
        ("drop a needed carry-in", Some(Missing)),
        ("drop a needed Fixed value", Some(Missing)),
        ("drop a needed history of 0 rows (pos 0 or window 1)", Some(Missing)),
        ("supply the target with a wrong value", Some(Malformed)),
        ("supply an index that is no node", Some(Malformed)),
        ("a wrong-shaped supplied node inside the closure", Some(Operand)),
        ("pos = history_bound", Some(Position)),
        ("one history row too many", Some(Position)),
        ("a wrong-shaped supplied node outside the closure", None),
        ("a Fixed value outside [lo, hi]", Some(Operand)),
        ("drop a needed history of ≥ 1 row", Some(Missing)),
        ("no token, the closure reads it", Some(Missing)),
        ("token = token_bound, the closure reads it", Some(Operand)),
        ("no token, the closure does not read it", None),
        ("a history row of the wrong dtype", Some(Operand)),
        ("a carry-in of the wrong shape", Some(Operand)),
        ("a request that is no occurrence", Some(Malformed)),
        ("drop a param the closure reads", Some(Missing)),
    ];
    let n = 600 * scale() as u64;
    // (applicable, ref2 = text, first = text, first = ref2)
    let mut per: Vec<(usize, usize, usize, usize)> = vec![(0, 0, 0, 0); KINDS.len()];
    let mut examples: Vec<Vec<String>> = vec![Vec::new(); KINDS.len()];
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xB3B3_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let p = &g.prog;
        let Outcome::Ok(fp) = first_decode(&encode(p)) else { continue };
        let fparams = params_to(&g.params);
        let mut st = initial_state(p);
        for _ in 0..rng.gen_range(1..=4) {
            let token = rng.gen_range(0..p.token_bound.min(64) as u64);
            let Ok((_, next, trace)) = step_traced(p, &g.params, &st, token) else { break };
            for occ in &trace {
                let b = occ.block as usize;
                let block = &p.blocks[b];
                let commits: Vec<u16> = (0..block.nodes.len() as u16).filter(|&i| block.nodes[i as usize].commit).collect();
                if commits.is_empty() {
                    continue;
                }
                let target = pick(&mut rng, &commits);
                let inst = |j: u16| if p.states[j as usize].per_layer { occ.layer } else { None };
                let mut env = ConeEnv { token: Some(token), pos: st.pos, carry_in: occ.carry_in.clone(), ..Default::default() };
                for nd in &block.nodes {
                    for r in &nd.inputs {
                        if let Ref::State(j) = *r
                            && let Some(v) = st.fixed.get(&(j, inst(j)))
                        {
                            env.fixed.insert(j, v.clone());
                        }
                    }
                    if let Prim::HistAppend { state } = nd.prim {
                        env.hist_prior.insert(state, st.hist.get(&(state, inst(state))).cloned().unwrap_or_default());
                    }
                }
                for &c in &commits {
                    if c != target {
                        env.supplied.insert(c, occ.values[c as usize].clone());
                    }
                }
                let todo = closure(p, b, target, &env.supplied);
                let reads =
                    |f: &dyn Fn(&Ref) -> bool| block.nodes.iter().enumerate().any(|(i, nd)| todo[i] && nd.inputs.iter().any(f));
                let needed_carry: Vec<u8> = block
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| todo[*i])
                    .flat_map(|(_, nd)| nd.inputs.iter().filter_map(|r| if let Ref::CarryIn(k) = r { Some(*k) } else { None }))
                    .collect();
                let needed_fixed: Vec<u16> = block
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| todo[*i])
                    .flat_map(|(_, nd)| nd.inputs.iter().filter_map(|r| if let Ref::State(j) = r { Some(*j) } else { None }))
                    .collect();
                let needed_param: Vec<u16> = block
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| todo[*i])
                    .flat_map(|(_, nd)| nd.inputs.iter().filter_map(|r| if let Ref::Param(j) = r { Some(*j) } else { None }))
                    .collect();
                let needed_hist: Vec<u16> = block
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| todo[*i])
                    .filter_map(|(_, nd)| if let Prim::HistAppend { state } = nd.prim { Some(state) } else { None })
                    .collect();
                let reads_token = reads(&|r| *r == Ref::Input(0));
                let rows_needed = |j: u16| match p.states[j as usize].kind {
                    StateKind::Hist { window } => st.pos.min(window as u64 - 1),
                    _ => 0,
                };
                let outside: Vec<u16> =
                    (0..block.nodes.len() as u16).filter(|&i| !todo[i as usize] && !env.supplied.contains_key(&i)).collect();
                let inside: Vec<u16> = (0..target).filter(|&i| todo[i as usize]).collect();
                let mut k = rng.gen_range(0..KINDS.len());
                if k == 2 && needed_hist.first().is_some_and(|&j| rows_needed(j) > 0) {
                    k = 10;
                } else if k == 10 && needed_hist.first().is_some_and(|&j| rows_needed(j) == 0) {
                    k = 2;
                }
                if k == 11 && !reads_token {
                    k = 13;
                } else if k == 13 && reads_token {
                    k = 11;
                }
                let mut e = env.clone();
                let mut params = g.params.clone();
                let (mut blk_req, mut layer_req) = (occ.block, occ.layer);
                let wrong = Tensor::new(DType::I8, vec![7], vec![1; 7]).unwrap();
                let applicable = match k {
                    0 => needed_carry.first().map(|c| e.carry_in.remove(c)).is_some(),
                    1 => needed_fixed.first().map(|j| e.fixed.remove(j)).is_some(),
                    2 | 10 => needed_hist.first().map(|j| e.hist_prior.remove(j)).is_some(),
                    3 => {
                        let mut v = occ.values[target as usize].clone();
                        if let Some(x) = v.data.first_mut() {
                            *x = if *x == v.dtype.min() { *x + 1 } else { *x - 1 };
                        }
                        e.supplied.insert(target, v);
                        true
                    }
                    4 => {
                        e.supplied.insert(block.nodes.len() as u16 + 3, wrong.clone());
                        true
                    }
                    5 => inside.first().map(|&i| e.supplied.insert(i, wrong.clone())).is_some(),
                    6 => {
                        e.pos = p.history_bound as u64;
                        true
                    }
                    7 => match needed_hist.first() {
                        Some(j) => {
                            e.hist_prior.get_mut(j).unwrap().push(occ.values[0].clone());
                            true
                        }
                        None => false,
                    },
                    8 => outside.first().map(|&i| e.supplied.insert(i, wrong.clone())).is_some(),
                    9 => {
                        let mut done = false;
                        if let Some(&j) = needed_fixed.first()
                            && let StateKind::Fixed { hi, .. } = p.states[j as usize].kind
                            && (hi as i128) < p.states[j as usize].dtype.max()
                        {
                            e.fixed.get_mut(&j).unwrap().data[0] = hi as i128 + 1;
                            done = true;
                        }
                        done
                    }
                    11 | 13 => {
                        e.token = None;
                        true
                    }
                    12 => {
                        e.token = Some(p.token_bound as u64);
                        reads_token
                    }
                    14 => match needed_hist.first() {
                        Some(j) if rows_needed(*j) > 0 => {
                            let rows = e.hist_prior.get_mut(j).unwrap();
                            let r0 = &rows[0];
                            let other = if r0.dtype == DType::I32 { DType::I16 } else { DType::I32 };
                            rows[0] = Tensor::new(other, r0.shape.clone(), vec![0; r0.data.len()]).unwrap();
                            true
                        }
                        _ => false,
                    },
                    15 => match needed_carry.first() {
                        Some(c) => {
                            let t = e.carry_in.get_mut(c).unwrap();
                            let mut shape = t.shape.clone();
                            shape.push(2);
                            *t = Tensor::zeros(t.dtype, shape);
                            true
                        }
                        None => false,
                    },
                    16 => {
                        // A layer that runs another block, or pre/post with a layer.
                        match occ.layer {
                            None => layer_req = Some(0),
                            Some(_) => {
                                blk_req = p.schedule.pre;
                            }
                        }
                        let _ = &mut blk_req;
                        true
                    }
                    _ => match needed_param.first() {
                        Some(&j) => {
                            let key = (j, if p.params[j as usize].per_layer { occ.layer } else { None });
                            params.remove(&key).is_some()
                        }
                        None => false,
                    },
                };
                if !applicable {
                    continue;
                }
                let a = mine(eval_cone(p, &params, blk_req, layer_req, target, &e));
                let f = first_cone(&fp, &params_to(&params), blk_req, layer_req, target, &env_to(&e));
                let want: Outcome<Tensor> = match KINDS[k].1 {
                    None => Outcome::Ok(occ.values[target as usize].clone()),
                    Some(c) => Outcome::Err(c),
                };
                let slot = &mut per[k];
                slot.0 += 1;
                slot.1 += (a == want) as usize;
                slot.2 += (f == want) as usize;
                slot.3 += (f == a) as usize;
                if (a != want || f != want) && examples[k].len() < 3 {
                    examples[k].push(format!(
                        "seed {seed} pos {}: text {} ref2 {} first {}",
                        st.pos,
                        brief(&want),
                        brief(&a),
                        brief(&f)
                    ));
                }
                let _ = &fparams;
            }
            st = next;
        }
    }
    println!("B3. hostile cone environments (an honest court env with one defect; revision 2 fixes each verdict):");
    println!("    {:55} {:10} {:>6} {:>9} {:>9} {:>9}", "defect", "04b", "cases", "ref2=text", "first=text", "first=ref2");
    let mut bad = Vec::new();
    for (i, (name, says)) in KINDS.iter().enumerate() {
        let (c, a, f, same) = per[i];
        let says = says.map(|c| c.name().to_string()).unwrap_or_else(|| "ok".into());
        println!("    {:55} {:10} {:>6} {:>9} {:>9} {:>9}", name, says, c, a, f, same);
        for e in &examples[i] {
            println!("        e.g. {e}");
        }
        if a != c || f != c {
            bad.push(*name);
        }
        assert!(c > 0 || scale() == 0, "perturbation {name} never applied");
    }
    assert!(bad.is_empty(), "verdicts off the text: {bad:?}");
}
