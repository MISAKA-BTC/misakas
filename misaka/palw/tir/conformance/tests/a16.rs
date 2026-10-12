//! **The A16 dense tier** (`palw_base0_a16`) and the fenced `RequantizeByToken`, against their
//! segments. Operands are A16 codes where the live kernel requires them (`as_a16`), any `i32`
//! where it does not; triples take any multiplier and zero point and every shift `from_wire`
//! admits (`0..=62`).

mod common;

use common::*;
use kaspa_consensus_core::palw_base0_a16::{self as a16, A16_MAX_ATTN_HISTORY_V1, A16AttnFusedParamsV1};
use kaspa_consensus_core::palw_base0_ops as ops;
use misaka_palw_tir::DType;
use misaka_palw_tir::library::Narrowing;
use misaka_palw_tir::program::INPUT_TOKEN;
use misaka_palw_tir::{Dim, Ref};

const T: u32 = 40;

fn shifts(rng: &mut Lcg, n: usize) -> Vec<i128> {
    rng.vec(n, 0, 62, &[0, 1, 24, 30, 31, 62])
}

#[test]
fn kdesc_a16_embed() {
    let (rows, dim) = (12u32, 6u32);
    let mut rng = Lcg(11);
    let table = rng.vec((rows * dim) as usize, -128, 127, I8X);
    let tokens: Vec<u32> = (0..rows).collect();
    let got = run(&[arg("table", DType::I8, &[rows, dim], table.clone())], &[], &tokens, rows, |b, r, _, _| {
        b.embed(r[0], Ref::Input(INPUT_TOKEN))
    });
    for (t, g) in tokens.iter().zip(&got) {
        // The arm reads `dim` bytes at `token · dim` and widens each as an `i8`.
        let want: Vec<i128> = table[(*t * dim) as usize..((*t + 1) * dim) as usize].to_vec();
        assert_eq!(g, &want, "token {t}");
        assert_eq!(want, wide(ops::embed_lookup(&i8s(&table), rows as usize, dim as usize, *t as usize).unwrap()));
    }
}

fn matmul_case(wide_out: bool, seed: u64) {
    let (out, n) = (6u32, 37u32);
    let mut rng = Lcg(seed);
    let w = rng.vec((out * n) as usize, -128, 127, I8X);
    let xs = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    let m = rng.vec((T * out) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let s = shifts(&mut rng, (T * out) as usize);
    let z = rng.vec((T * out) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let args = [
        arg("w", DType::I8, &[out, n], w.clone()),
        arg("x", DType::I16, &[T, n], xs.clone()),
        arg("m", DType::I64, &[T, out], m.clone()),
        arg("s", DType::I8, &[T, out], s.clone()),
        arg("z", DType::I64, &[T, out], z.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let x = row(b, r[1], pos);
        let nw = narrowing_at(b, r[2], r[3], r[4], pos);
        b.a16_matmul(r[0], x, &nw, wide_out)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        let params: Vec<_> =
            (0..out as usize).map(|c| triple(m[p * out as usize + c], s[p * out as usize + c], z[p * out as usize + c])).collect();
        let want =
            if wide_out { a16::a16_matmul_rescale(&i8s(&w), &x, &params) } else { a16::a16_matmul_requant(&i8s(&w), &x, &params) }
                .unwrap();
        assert_eq!(g, &wide(&want), "wide {wide_out} position {p}");
    }
}

#[test]
fn kdesc_a16_matmul_requant() {
    matmul_case(false, 12);
}

#[test]
fn kdesc_a16_matmul_rescale() {
    matmul_case(true, 13);
}

#[test]
fn kdesc_a16_rms_norm() {
    let n = 19u32;
    let mut rng = Lcg(14);
    let mut xs = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    xs[..n as usize].iter_mut().for_each(|v| *v = 0);
    xs[n as usize..2 * n as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 0 { 32767 } else { 0 });
    // `validate_geometry` bounds the registered epsilon to `[0, 2^40]`.
    for eps in [0i64, 1, 7, 1 << 20, 1 << 40] {
        let got = run_pos(&[arg("x", DType::I16, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[0], pos);
            b.rms_norm_a16(x, eps)
        });
        for (p, g) in got.iter().enumerate() {
            let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
            assert_eq!(g, &wide(&a16::a16_rms_norm(&x, eps).unwrap()), "eps {eps} position {p}");
        }
    }
}

#[test]
fn kdesc_a16_requantize() {
    let n = 11u32;
    let mut rng = Lcg(15);
    let xs = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let m = rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let s = shifts(&mut rng, (T * n) as usize);
    let z = rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let args = [
        arg("x", DType::I32, &[T, n], xs.clone()),
        arg("m", DType::I64, &[T, n], m.clone()),
        arg("s", DType::I8, &[T, n], s.clone()),
        arg("z", DType::I64, &[T, n], z.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        let nw = narrowing_at(b, r[1], r[2], r[3], pos);
        b.narrow_codes(x, &nw)
    });
    for (p, g) in got.iter().enumerate() {
        let at = |v: &Vec<i128>, i: usize| v[p * n as usize + i];
        let params: Vec<_> = (0..n as usize).map(|i| triple(at(&m, i), at(&s, i), at(&z, i))).collect();
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        assert_eq!(g, &wide(&a16::a16_requant(&x, &params).unwrap()), "position {p}");
    }
}

/// The fenced `RequantizeByToken` (ADR-0102): the row narrowed by the TOKEN's triple.
#[test]
fn kdesc_a16_requantize_by_token() {
    let (vocab, n) = (10u32, 9u32);
    let mut rng = Lcg(16);
    let tokens: Vec<u32> = (0..vocab).chain([3, 3, 9]).collect();
    let xs = rng.vec(tokens.len() * n as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let m = rng.vec(vocab as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let s = shifts(&mut rng, vocab as usize);
    let z = rng.vec(vocab as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let args = [
        arg("x", DType::I32, &[tokens.len() as u32, n], xs.clone()),
        arg("lift.m", DType::I64, &[vocab], m.clone()),
        arg("lift.s", DType::I8, &[vocab], s.clone()),
        arg("lift.z", DType::I64, &[vocab], z.clone()),
    ];
    let got = run(&args, &[], &tokens, vocab, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        b.requantize_by_token(x, r[1], r[2], r[3], Ref::Input(INPUT_TOKEN))
    });
    for (p, (t, g)) in tokens.iter().zip(&got).enumerate() {
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        let tr = triple(m[*t as usize], s[*t as usize], z[*t as usize]);
        assert_eq!(g, &wide(&a16::a16_requant(&x, &vec![tr; x.len()]).unwrap()), "token {t}");
    }
}

#[test]
fn kdesc_a16_add_elem_and_mul_elem() {
    let n = 12u32;
    let mut rng = Lcg(17);
    let a = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    let bb = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    for mul in [false, true] {
        let got =
            run_pos(&[arg("a", DType::I16, &[T, n], a.clone()), arg("b", DType::I16, &[T, n], bb.clone())], &[], T, |b, r, _, pos| {
                let (x, y) = (row(b, r[0], pos), row(b, r[1], pos));
                if mul { b.a16_mul_elem(x, y) } else { b.a16_add_elem(x, y) }
            });
        for (p, g) in got.iter().enumerate() {
            let x = i32s(&a[p * n as usize..(p + 1) * n as usize]);
            let y = i32s(&bb[p * n as usize..(p + 1) * n as usize]);
            let want = if mul { a16::a16_mul_elem(&x, &y) } else { a16::a16_add_elem(&x, &y) }.unwrap();
            assert_eq!(g, &wide(&want), "mul {mul} position {p}");
        }
    }
}

#[test]
fn kdesc_a16_softmax() {
    let (rows, len) = (3u32, 7u32);
    let mut rng = Lcg(18);
    let xs = rng.vec((T * rows * len) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    for up in [0u8, 2, 14, 31, 47, 48, 62] {
        let got = run_pos(&[arg("x", DType::I32, &[T, rows * len], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[0], pos);
            let x = b.reshape_fixed(x, &[rows, len]);
            let p = b.softmax_shifted(x, up as u32);
            b.reshape_fixed(p, &[rows * len])
        });
        for (p, g) in got.iter().enumerate() {
            let x = i32s(&xs[p * (rows * len) as usize..(p + 1) * (rows * len) as usize]);
            assert_eq!(g, &wide(&a16::a16_softmax_rows(&x, len as usize, up).unwrap()), "up {up} position {p}");
        }
    }
}

#[test]
fn kdesc_a16_rope() {
    let pairs = 5u32;
    let mut rng = Lcg(19);
    let xs = rng.vec((T * 2 * pairs) as usize, -32767, 32767, CODEX);
    let c = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let s = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let args = [
        arg("x", DType::I16, &[T, 2 * pairs], xs.clone()),
        arg("cos", DType::I32, &[T, pairs], c.clone()),
        arg("sin", DType::I32, &[T, pairs], s.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, c, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.rope_pairs(x, c, s, -32767, 32767, DType::I16)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * (2 * pairs) as usize..(p + 1) * (2 * pairs) as usize]);
        let (cc, ss) =
            (i32s(&c[p * pairs as usize..(p + 1) * pairs as usize]), i32s(&s[p * pairs as usize..(p + 1) * pairs as usize]));
        assert_eq!(g, &wide(&a16::a16_rope(&x, &cc, &ss).unwrap()), "position {p}");
    }
}

/// The attention kernels over a GROWING history: at position `p` the window holds rows `0..=p`,
/// and the live kernel reads the same rows as its series.
struct Attn {
    heads: u32,
    kv: u32,
    d: u32,
    q: Vec<i128>,
    k: Vec<i128>,
    v: Vec<i128>,
}

impl Attn {
    fn new(seed: u64) -> Self {
        let (heads, kv, d) = (4u32, 2u32, 8u32);
        let mut rng = Lcg(seed);
        Self {
            heads,
            kv,
            d,
            q: rng.vec((T * heads * d) as usize, -32767, 32767, CODEX),
            k: rng.vec((T * kv * d) as usize, -32767, 32767, CODEX),
            v: rng.vec((T * kv * d) as usize, -32767, 32767, CODEX),
        }
    }
    fn args(&self, triples: &[(i128, i128, i128)]) -> Vec<Arg> {
        let mut a = vec![
            arg("q", DType::I16, &[T, self.heads * self.d], self.q.clone()),
            arg("k", DType::I16, &[T, self.kv * self.d], self.k.clone()),
            arg("v", DType::I16, &[T, self.kv * self.d], self.v.clone()),
        ];
        let names = [["t0.m", "t0.s", "t0.z"], ["t1.m", "t1.s", "t1.z"], ["t2.m", "t2.s", "t2.z"]];
        for (i, (m, s, z)) in triples.iter().enumerate() {
            a.push(arg(names[i][0], DType::I64, &[1], vec![*m]));
            a.push(arg(names[i][1], DType::I8, &[1], vec![*s]));
            a.push(arg(names[i][2], DType::I64, &[1], vec![*z]));
        }
        a
    }
    fn states(&self) -> Vec<St> {
        vec![St::Hist("k", DType::I16, vec![self.kv * self.d]), St::Hist("v", DType::I16, vec![self.kv * self.d])]
    }
    fn q_at(&self, p: usize) -> Vec<i32> {
        let w = (self.heads * self.d) as usize;
        i32s(&self.q[p * w..(p + 1) * w])
    }
    fn series(&self, of: &[i128], p: usize) -> Vec<i32> {
        i32s(&of[..(p + 1) * (self.kv * self.d) as usize])
    }
}

fn narrowing_of(r: &[Ref], i: usize) -> Narrowing {
    Narrowing::new(r[3 + 3 * i], r[4 + 3 * i], Some(r[5 + 3 * i]))
}

const ATTN_TRIPLES: [(i128, i128, i128); 3] = [(1 << 10, 30, 3), (1 << 15, 24, 0), (1, 22, -5)];
const ATTN_TRIPLES_ADVERSE: [(i128, i128, i128); 3] =
    [(i64::MAX as i128, 62, i64::MIN as i128), (i64::MIN as i128, 0, 7), (-(1 << 40), 33, i64::MAX as i128)];

#[test]
fn kdesc_a16_attn_scores_and_kdesc_a16_attn_values() {
    for (seed, triples) in [(21u64, ATTN_TRIPLES), (22, ATTN_TRIPLES_ADVERSE)] {
        let a = Attn::new(seed);
        let (heads, kv, d) = (a.heads, a.kv, a.d);
        // Scores alone, then the values of those scores' codes (the live values kernel reads codes).
        for values in [false, true] {
            // Only the triples a case uses are declared (NF-11): scores, and the values' for `values`.
            let used = if values { vec![triples[0], triples[2]] } else { vec![triples[0]] };
            let got = run_pos(&a.args(&used), &a.states(), T, |b, r, st, pos| {
                let q = row(b, r[0], pos);
                let k = row(b, r[1], pos);
                let v = row(b, r[2], pos);
                let kw = b.hist_append(st[0], k);
                let vw = b.hist_append(st[1], v);
                let ns = narrowing_of(r, 0);
                let s = b.a16_attn_scores(q, kw, heads, kv, d, &ns);
                if values {
                    let nv = narrowing_of(r, 1);
                    b.a16_attn_values(s, vw, heads, kv, d, &nv)
                } else {
                    // The scores row is [heads, H]; flatten for the comparison.
                    b.reshape(s, &[Dim::Fixed(heads), Dim::H])
                }
            });
            for (p, g) in got.iter().enumerate() {
                let kv_len = p + 1;
                let ts = triple(triples[0].0, triples[0].1, triples[0].2);
                let scores = a16::a16_attn_scores(
                    &a.q_at(p),
                    &a.series(&a.k, p),
                    heads as usize,
                    kv as usize,
                    d as usize,
                    &vec![ts; heads as usize * kv_len],
                )
                .unwrap();
                if values {
                    let tv = triple(triples[2].0, triples[2].1, triples[2].2);
                    let want = a16::a16_attn_values_within(
                        &scores,
                        &a.series(&a.v, p),
                        heads as usize,
                        kv as usize,
                        d as usize,
                        &vec![tv; (heads * d) as usize],
                        A16_MAX_ATTN_HISTORY_V1,
                    )
                    .unwrap();
                    assert_eq!(g, &wide(&want), "values seed {seed} position {p}");
                } else {
                    assert_eq!(g, &wide(&scores), "scores seed {seed} position {p}");
                }
            }
        }
    }
}

#[test]
fn kdesc_a16_attn_fused() {
    for (seed, triples) in [(23u64, ATTN_TRIPLES), (24, ATTN_TRIPLES_ADVERSE)] {
        for up in [0u8, 2, 47, 62] {
            let a = Attn::new(seed);
            let (heads, kv, d) = (a.heads, a.kv, a.d);
            let got = run_pos(&a.args(&triples), &a.states(), T, |b, r, st, pos| {
                let q = row(b, r[0], pos);
                let k = row(b, r[1], pos);
                let v = row(b, r[2], pos);
                let kw = b.hist_append(st[0], k);
                let vw = b.hist_append(st[1], v);
                let (ns, np, nv) = (narrowing_of(r, 0), narrowing_of(r, 1), narrowing_of(r, 2));
                b.a16_attn_fused(q, kw, vw, heads, kv, d, &ns, &np, &nv, up as u32)
            });
            let params = A16AttnFusedParamsV1 {
                scores: triple(triples[0].0, triples[0].1, triples[0].2),
                probs: triple(triples[1].0, triples[1].1, triples[1].2),
                values: triple(triples[2].0, triples[2].1, triples[2].2),
                up_bits: up,
            };
            for (p, g) in got.iter().enumerate() {
                let want = a16::a16_attn_fused_reference_within_v1(
                    &a.q_at(p),
                    &a.series(&a.k, p),
                    &a.series(&a.v, p),
                    heads as usize,
                    kv as usize,
                    d as usize,
                    params,
                    A16_MAX_ATTN_HISTORY_V1,
                )
                .unwrap();
                assert_eq!(g, &wide(&want), "seed {seed} up {up} position {p}");
            }
        }
    }
}

/// **The narrowing without a zero term, in three nodes** (tir/lower's request): `narrow` with
/// `z: None` is `Clamp[lo, hi](HAFZ(x·m / 2^s))`, the template's value with `z = 0`
/// (`narrow_a16(.., 0, ..)`) and the live `a16_requant`'s with every zero point 0 — on
/// [`kdesc_a16_requantize`]'s rows, multipliers and shifts.
#[test]
fn kdesc_a16_requantize_lean_form_without_a_zero_term() {
    let n = 11u32;
    let mut rng = Lcg(15);
    let xs = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let m = rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let s = shifts(&mut rng, (T * n) as usize);
    let args =
        [arg("x", DType::I32, &[T, n], xs.clone()), arg("m", DType::I64, &[T, n], m.clone()), arg("s", DType::I8, &[T, n], s.clone())];
    let lean = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, m, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.narrow_codes(x, &Narrowing::new(m, s, None))
    });
    let template = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, m, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        let p2 = b.pow2_of(s);
        let zero = b.c(DType::I64, 0);
        b.narrow_a16(x, m, p2, zero, -32767, 32767, DType::I16)
    });
    for p in 0..T as usize {
        let at = |v: &Vec<i128>, i: usize| v[p * n as usize + i];
        let params: Vec<_> = (0..n as usize).map(|i| triple(at(&m, i), at(&s, i), 0)).collect();
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        assert_eq!(lean[p], template[p], "position {p}: the three-node form is the template's value");
        assert_eq!(lean[p], wide(&a16::a16_requant(&x, &params).unwrap()), "position {p}: and the live kernel's");
    }
}
