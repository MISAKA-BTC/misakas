//! **Qwen3.6's own ops** (`palw_qwen36_ops`, ADR-0052) against their segments, each on the live
//! kernel's domain: codes where it calls `check_a16`, any `i32` where it does not, triples as
//! `from_wire` admits them, and the recurrence's own preconditions (`decay`, `beta` in `[0, ONE]`,
//! state multipliers `≤ 2^30`, write shifts in `[−62, 20]`).

mod common;

use common::*;
use kaspa_consensus_core::palw_base0_a16 as a16;
use kaspa_consensus_core::palw_qwen36_ops::{self as q36, Qwen36GdnParamsV1, Qwen36GdnStateV1};
use misaka_palw_tir::DType;
use misaka_palw_tir::arith::ONE;
use misaka_palw_tir::library::Narrowing;

const T: u32 = 36;

fn shifts(rng: &mut Lcg, n: usize) -> Vec<i128> {
    rng.vec(n, 0, 62, &[0, 1, 24, 30, 31, 62])
}

/// Per-row triples: `(m, s, z)` of `n` lanes at each of `T` positions.
fn triples(rng: &mut Lcg, n: u32) -> (Vec<i128>, Vec<i128>, Vec<i128>) {
    let m = rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    let s = shifts(rng, (T * n) as usize);
    let z = rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, I64X);
    (m, s, z)
}

fn triple_args(n: u32, t: &(Vec<i128>, Vec<i128>, Vec<i128>)) -> [Arg; 3] {
    [arg("m", DType::I64, &[T, n], t.0.clone()), arg("s", DType::I8, &[T, n], t.1.clone()), arg("z", DType::I64, &[T, n], t.2.clone())]
}

fn live_triples(t: &(Vec<i128>, Vec<i128>, Vec<i128>), p: usize, n: usize) -> Vec<a16::A16QuantParams> {
    (0..n).map(|i| triple(t.0[p * n + i], t.1[p * n + i], t.2[p * n + i])).collect()
}

fn grouped_case(n: u32, wide_out: bool, seed: u64) {
    let out = 5u32;
    let groups = n.div_ceil(32);
    let mut rng = Lcg(seed);
    let w = rng.vec((out * n) as usize, -128, 127, I8X);
    let e = rng.vec((out * groups) as usize, 0, 20, &[0, 1, 19, 20]);
    let xs = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    let tr = triples(&mut rng, out);
    let mut args = vec![
        arg("w", DType::I8, &[out, n], w.clone()),
        arg("e", DType::I8, &[out, groups], e.clone()),
        arg("x", DType::I16, &[T, n], xs.clone()),
    ];
    args.extend(triple_args(out, &tr));
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let x = row(b, r[2], pos);
        let nw = narrowing_at(b, r[3], r[4], r[5], pos);
        b.q36_matmul_grouped(r[0], r[1], x, &nw, wide_out)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        let params = live_triples(&tr, p, out as usize);
        let want = if wide_out {
            q36::q36_matmul_grouped_wide(&i8s(&w), &i8s(&e), &x, &params)
        } else {
            q36::q36_matmul_grouped(&i8s(&w), &i8s(&e), &x, &params)
        }
        .unwrap();
        assert_eq!(g, &wide(&want), "n {n} wide {wide_out} position {p}");
    }
}

#[test]
fn kdesc_q36_matmul_grouped() {
    for (n, seed) in [(64u32, 31u64), (45, 32), (32, 33)] {
        grouped_case(n, false, seed);
    }
}

#[test]
fn kdesc_q36_matmul_grouped_wide() {
    for (n, seed) in [(64u32, 34u64), (77, 35)] {
        grouped_case(n, true, seed);
    }
}

#[test]
fn kdesc_q36_rope_partial() {
    let (heads, hd) = (3u32, 8u32);
    for (rotary, seed) in [(4u32, 36u64), (8, 37), (2, 38)] {
        let pairs = rotary / 2;
        let mut rng = Lcg(seed);
        let xs = rng.vec((T * heads * hd) as usize, -32767, 32767, CODEX);
        let c = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
        let s = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
        let tr = triples(&mut rng, 1);
        let mut args = vec![
            arg("x", DType::I16, &[T, heads * hd], xs.clone()),
            arg("cos", DType::I32, &[T, pairs], c.clone()),
            arg("sin", DType::I32, &[T, pairs], s.clone()),
        ];
        args.extend(triple_args(1, &tr));
        let got = run_pos(&args, &[], T, |b, r, _, pos| {
            let (x, cc, ss) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
            let nw = narrowing_at(b, r[3], r[4], r[5], pos);
            b.q36_rope_partial(x, hd, rotary, cc, ss, &nw)
        });
        for (p, g) in got.iter().enumerate() {
            let x = i32s(&xs[p * (heads * hd) as usize..(p + 1) * (heads * hd) as usize]);
            let (cc, ss) =
                (i32s(&c[p * pairs as usize..(p + 1) * pairs as usize]), i32s(&s[p * pairs as usize..(p + 1) * pairs as usize]));
            let want = q36::q36_rope_partial(&x, hd as usize, rotary as usize, &cc, &ss, live_triples(&tr, p, 1)[0]).unwrap();
            assert_eq!(g, &wide(&want), "rotary {rotary} position {p}");
        }
    }
}

#[test]
fn kdesc_q36_ssm_conv() {
    let c = 9u32;
    let mut rng = Lcg(39);
    let win = rng.vec((T * 4 * c) as usize, -32767, 32767, CODEX);
    let taps = rng.vec((c * 4) as usize, -128, 127, I8X);
    let tr = triples(&mut rng, c);
    let mut args = vec![arg("window", DType::I16, &[T, 4 * c], win.clone()), arg("taps", DType::I8, &[c, 4], taps.clone())];
    args.extend(triple_args(c, &tr));
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let w = row(b, r[0], pos);
        let w = b.reshape_fixed(w, &[4, c]);
        let nw = narrowing_at(b, r[2], r[3], r[4], pos);
        b.q36_ssm_conv(w, r[1], &nw)
    });
    for (p, g) in got.iter().enumerate() {
        let w = i32s(&win[p * (4 * c) as usize..(p + 1) * (4 * c) as usize]);
        let want = q36::q36_ssm_conv(&w, &i32s(&taps), c as usize, &live_triples(&tr, p, c as usize)).unwrap();
        assert_eq!(g, &wide(&want), "position {p}");
    }
}

#[test]
fn kdesc_q36_l2_norm() {
    let (heads, hd) = (4u32, 8u32);
    let mut rng = Lcg(40);
    let mut xs = rng.vec((T * heads * hd) as usize, -32767, 32767, CODEX);
    xs[..hd as usize].iter_mut().for_each(|v| *v = 0);
    xs[hd as usize..2 * hd as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 1 { 1 } else { 0 });
    let got = run_pos(&[arg("x", DType::I16, &[T, heads * hd], xs.clone())], &[], T, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        let x = b.reshape_fixed(x, &[heads, hd]);
        let y = b.l2_norm_q15(x);
        b.reshape_fixed(y, &[heads * hd])
    });
    for (p, g) in got.iter().enumerate() {
        let want: Vec<i32> = xs[p * (heads * hd) as usize..(p + 1) * (heads * hd) as usize]
            .chunks(hd as usize)
            .flat_map(|h| q36::q36_l2_norm(&i32s(h)).unwrap())
            .collect();
        assert_eq!(g, &wide(&want), "position {p}");
    }
}

#[test]
fn kdesc_q36_sigmoid() {
    let n = 15u32;
    let mut rng = Lcg(41);
    let xs = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let got = run_pos(&[arg("x", DType::I32, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        b.int_sigmoid(x)
    });
    for (p, g) in got.iter().enumerate() {
        assert_eq!(g, &wide(&q36::q36_sigmoid_gate(&i32s(&xs[p * n as usize..(p + 1) * n as usize]))), "position {p}");
    }
}

#[test]
fn kdesc_q36_gate_apply() {
    let n = 10u32;
    let mut rng = Lcg(42);
    let y = rng.vec((T * n) as usize, -32767, 32767, CODEX);
    let g = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let tr = triples(&mut rng, 1);
    let mut args = vec![arg("y", DType::I16, &[T, n], y.clone()), arg("g", DType::I32, &[T, n], g.clone())];
    args.extend(triple_args(1, &tr));
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (yy, gg) = (row(b, r[0], pos), row(b, r[1], pos));
        let nw = narrowing_at(b, r[2], r[3], r[4], pos);
        b.q36_gate_apply(yy, gg, &nw)
    });
    for (p, out) in got.iter().enumerate() {
        let (yy, gg) = (i32s(&y[p * n as usize..(p + 1) * n as usize]), i32s(&g[p * n as usize..(p + 1) * n as usize]));
        assert_eq!(out, &wide(&q36::q36_gate_apply(&yy, &gg, live_triples(&tr, p, 1)[0]).unwrap()), "position {p}");
    }
}

#[test]
fn kdesc_q36_mul_wide_and_kdesc_q36_rescale_row() {
    let n = 12u32;
    let mut rng = Lcg(43);
    let a = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let bb = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let tr = triples(&mut rng, n);
    for mul in [true, false] {
        let mut args = vec![arg("a", DType::I32, &[T, n], a.clone()), arg("b", DType::I32, &[T, n], bb.clone())];
        args.extend(triple_args(n, &tr));
        let got = run_pos(&args, &[], T, |b, r, _, pos| {
            let (x, y) = (row(b, r[0], pos), row(b, r[1], pos));
            let nw = narrowing_at(b, r[2], r[3], r[4], pos);
            if mul {
                b.q36_mul_wide(x, y, &nw)
            } else {
                // The rescale reads one row; the second is used so every param stays used (NF-11).
                let s = b.q36_rescale_row(x, &nw);
                let t = b.clamp(y, 0, 0, DType::I32);
                b.add(s, t, DType::I32)
            }
        });
        for (p, g) in got.iter().enumerate() {
            let (x, y) = (i32s(&a[p * n as usize..(p + 1) * n as usize]), i32s(&bb[p * n as usize..(p + 1) * n as usize]));
            let params = live_triples(&tr, p, n as usize);
            let want = if mul { q36::q36_mul_wide(&x, &y, &params) } else { q36::q36_rescale_row(&x, &params) }.unwrap();
            assert_eq!(g, &wide(&want), "mul {mul} position {p}");
        }
    }
}

#[test]
fn kdesc_q36_rms_norm_wide() {
    let hd = 16u32;
    let mut rng = Lcg(44);
    let mut xs = rng.vec((T * hd) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    xs[..hd as usize].iter_mut().for_each(|v| *v = 0);
    xs[hd as usize..2 * hd as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 2 { 3 } else { 0 });
    // `from_wire`'s domain for the epsilon: any non-negative mantissa, a shift of at most 62.
    let ez = rng.vec(T as usize, 0, i64::MAX as i128, &[0, 1, 1 << 30, i64::MAX as i128]);
    let es = shifts(&mut rng, T as usize);
    let args = [
        arg("x", DType::I32, &[T, hd], xs.clone()),
        arg("ez", DType::I64, &[T, 1], ez.clone()),
        arg("es", DType::I8, &[T, 1], es.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, z, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.rms_norm_wide_q36_exact(x, z, s)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * hd as usize..(p + 1) * hd as usize]);
        let want = q36::q36_rms_norm_wide(&x, triple(1, es[p], ez[p])).unwrap();
        assert_eq!(g, &wide(&want), "eps {}·2^{} position {p}", ez[p], es[p]);
    }
}

#[test]
fn kdesc_q36_router_topk() {
    let e = 8u32;
    let mut rng = Lcg(45);
    // The live router reads A16 codes (it refuses any other lane).
    let mut xs = rng.vec((T * e) as usize, -32767, 32767, CODEX);
    // Ties and a flat row: the lowest index wins and the committed set is in index order.
    xs[..e as usize].iter_mut().for_each(|v| *v = 7);
    xs[e as usize..2 * e as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i % 3 == 0 { 100 } else { -100 });
    for k in [1u32, 2, 4, 8] {
        for up in [0u8, 3, 10, 30, 62] {
            let got = run_pos(&[arg("logits", DType::I32, &[T, e], xs.clone())], &[], T, |b, r, _, pos| {
                let x = row(b, r[0], pos);
                let (idx, w) = b.router_topk_q36(x, k, up as u32);
                let idx = b.cast(idx, DType::I32);
                b.concat(&[idx, w], 0)
            });
            for (p, g) in got.iter().enumerate() {
                let routed = q36::q36_router_topk(&i32s(&xs[p * e as usize..(p + 1) * e as usize]), k as usize, up).unwrap();
                let want: Vec<i128> =
                    routed.iter().map(|r| r.expert as i128).chain(routed.iter().map(|r| r.weight_q as i128)).collect();
                assert_eq!(g, &want, "k {k} up {up} position {p}");
            }
        }
    }
}

#[test]
fn kdesc_q36_moe_combine() {
    let (k, width) = (4u32, 6u32);
    let mut rng = Lcg(46);
    let ys = rng.vec((T * k * width) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    // The weights are the router's: in [0, 2^25] (a kept probability, renormalised).
    let ws = rng.vec((T * k) as usize, 0, 1 << 25, &[0, 1, ONE, 1 << 25]);
    let tr = triples(&mut rng, 1);
    let mut args = vec![arg("y", DType::I32, &[T, k * width], ys.clone()), arg("w", DType::I32, &[T, k], ws.clone())];
    args.extend(triple_args(1, &tr));
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let y = row(b, r[0], pos);
        let y = b.reshape_fixed(y, &[k, width]);
        // In a program the weights are the router's output, which `router_topk_q36` types as
        // `[0, 2^25]`; the clamp states that here, where they arrive as a raw param.
        let w = row(b, r[1], pos);
        let w = b.clamp(w, 0, 1 << 25, DType::I32);
        let nw = narrowing_at(b, r[2], r[3], r[4], pos);
        let p2 = b.pow2_of(nw.s);
        b.moe_combine_q36(y, w, nw.m, p2, nw.z.unwrap(), -32767, 32767, DType::I16)
    });
    for (p, g) in got.iter().enumerate() {
        let y = i32s(&ys[p * (k * width) as usize..(p + 1) * (k * width) as usize]);
        let w = i32s(&ws[p * k as usize..(p + 1) * k as usize]);
        assert_eq!(g, &wide(&q36::q36_moe_combine(&y, &w, width as usize, live_triples(&tr, p, 1)[0]).unwrap()), "position {p}");
    }
}

#[test]
fn kdesc_q36_decay() {
    let n = 9u32;
    let mut rng = Lcg(47);
    let v = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    // The court's arm adds the registered `dt_bias` (an `i32` at the conversion's scale) first,
    // saturating in `i32`; the coefficient is any `i64` (`c ≤ 0` gives one).
    let bias = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let c =
        rng.vec((T * n) as usize, i64::MIN as i128, i64::MAX as i128, &[i64::MIN as i128, -1, 0, 1, ONE, 1 << 40, i64::MAX as i128]);
    let args = [
        arg("v", DType::I32, &[T, n], v.clone()),
        arg("bias", DType::I32, &[T, n], bias.clone()),
        arg("c", DType::I64, &[T, n], c.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (vv, bb, cc) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        let dt = b.add(vv, bb, DType::I64);
        let dt = b.clamp(dt, i32::MIN as i64, i32::MAX as i64, DType::I32);
        b.decay_q36(dt, cc)
    });
    for (p, g) in got.iter().enumerate() {
        let want: Vec<i128> = (0..n as usize)
            .map(|i| {
                let j = p * n as usize + i;
                q36::q36_decay((v[j] as i32).saturating_add(bias[j] as i32), c[j] as i64) as i128
            })
            .collect();
        assert_eq!(g, &want, "position {p}");
    }
}

#[test]
fn kdesc_q36_head_rms_norm() {
    let (heads, hd) = (3u32, 8u32);
    let mut rng = Lcg(48);
    let xs = rng.vec((T * heads * hd) as usize, -32767, 32767, CODEX);
    for eps in [1i64, 1 << 20, 1 << 40] {
        let got = run_pos(&[arg("x", DType::I16, &[T, heads * hd], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[0], pos);
            let x = b.reshape_fixed(x, &[heads, hd]);
            let y = b.rms_norm_a16(x, eps);
            b.reshape_fixed(y, &[heads * hd])
        });
        for (p, g) in got.iter().enumerate() {
            let want: Vec<i32> = xs[p * (heads * hd) as usize..(p + 1) * (heads * hd) as usize]
                .chunks(hd as usize)
                .flat_map(|h| a16::a16_rms_norm(&i32s(h), eps).unwrap())
                .collect();
            assert_eq!(g, &wide(&want), "eps {eps} position {p}");
        }
    }
}

/// The gated delta rule over positions: the segment's `Fixed` state against the live kernel's
/// `Qwen36GdnStateV1`, one head at a time, the output compared at every position.
#[test]
fn kdesc_q36_gdn_step() {
    let (h, dv, dk) = (2u32, 3u32, 4u32);
    for seed in [49u64, 50, 51] {
        let mut rng = Lcg(seed);
        let k = rng.vec((T * h * dk) as usize, -32767, 32767, CODEX);
        let v = rng.vec((T * h * dv) as usize, -32767, 32767, CODEX);
        let q = rng.vec((T * h * dk) as usize, -32767, 32767, CODEX);
        let decay = rng.vec((T * h) as usize, 0, ONE, &[0, 1, ONE - 1, ONE]);
        let beta = rng.vec((T * h) as usize, 0, ONE, &[0, 1, ONE - 1, ONE]);
        let mult = |rng: &mut Lcg| rng.vec(h as usize, -(1 << 30), 1 << 30, &[-(1 << 30), -1, 0, 1, 1 << 30]);
        let (rm, dm, om) = (mult(&mut rng), mult(&mut rng), mult(&mut rng));
        let sh = |rng: &mut Lcg| rng.vec(h as usize, 0, 62, &[0, 20, 24, 40, 62]);
        let (rs, ds, os) = (sh(&mut rng), sh(&mut rng), sh(&mut rng));
        let zz =
            |rng: &mut Lcg| rng.vec(h as usize, i64::MIN as i128, i64::MAX as i128, &[0, -3, 5, i64::MIN as i128, i64::MAX as i128]);
        let (rz, dz, oz) = (zz(&mut rng), zz(&mut rng), zz(&mut rng));
        let ws = rng.vec(h as usize, -62, 20, &[-62, -1, 0, 1, 20]);
        let args = vec![
            arg("k", DType::I16, &[T, h * dk], k.clone()),
            arg("v", DType::I16, &[T, h * dv], v.clone()),
            arg("q", DType::I16, &[T, h * dk], q.clone()),
            arg("decay", DType::I32, &[T, h], decay.clone()),
            arg("beta", DType::I32, &[T, h], beta.clone()),
            arg("read.m", DType::I64, &[h], rm.clone()),
            arg("read.s", DType::I8, &[h], rs.clone()),
            arg("read.z", DType::I64, &[h], rz.clone()),
            arg("delta.m", DType::I64, &[h], dm.clone()),
            arg("delta.s", DType::I8, &[h], ds.clone()),
            arg("delta.z", DType::I64, &[h], dz.clone()),
            arg("ws", DType::I32, &[h], ws.clone()),
            arg("out.m", DType::I64, &[h], om.clone()),
            arg("out.s", DType::I8, &[h], os.clone()),
            arg("out.z", DType::I64, &[h], oz.clone()),
        ];
        let smax = i32::MAX as i64;
        let got = run_pos(&args, &[St::Fixed("S", DType::I32, vec![h, dv, dk], -smax, smax)], T, |b, r, st, pos| {
            let kk = row(b, r[0], pos);
            let kk = b.reshape_fixed(kk, &[h, dk]);
            let vv = row(b, r[1], pos);
            let vv = b.reshape_fixed(vv, &[h, dv]);
            let qq = row(b, r[2], pos);
            let qq = b.reshape_fixed(qq, &[h, dk]);
            let dd = row(b, r[3], pos);
            let bb = row(b, r[4], pos);
            let tri = |b: &mut misaka_palw_tir::builder::BlockBuilder<'_>, i: usize| {
                let n = Narrowing::new(r[i], r[i + 1], Some(r[i + 2]));
                (n.m, b.pow2_of(n.s), r[i + 2])
            };
            let (read, delta, out) = (tri(b, 5), tri(b, 8), tri(b, 12));
            let o = b.gdn_step_q36(st[0], kk, vv, qq, dd, bb, read, delta, r[11], out);
            b.reshape_fixed(o, &[h * dv])
        });
        let mut states: Vec<Qwen36GdnStateV1> =
            (0..h).map(|_| Qwen36GdnStateV1 { d_v: dv as usize, d_k: dk as usize, s: vec![0; (dv * dk) as usize] }).collect();
        for (p, g) in got.iter().enumerate() {
            let mut want = Vec::new();
            for hh in 0..h as usize {
                let sl = |x: &[i128], w: u32| i32s(&x[(p * h as usize + hh) * w as usize..(p * h as usize + hh + 1) * w as usize]);
                let params = Qwen36GdnParamsV1 {
                    read: triple(rm[hh], rs[hh], rz[hh]),
                    delta: triple(dm[hh], ds[hh], dz[hh]),
                    write_shift: ws[hh] as i32,
                    out: triple(om[hh], os[hh], oz[hh]),
                };
                let o = q36::q36_gdn_step(
                    &mut states[hh],
                    &sl(&k, dk),
                    &sl(&v, dv),
                    &sl(&q, dk),
                    decay[p * h as usize + hh] as i64,
                    beta[p * h as usize + hh] as i64,
                    params,
                )
                .unwrap();
                want.extend(o);
            }
            assert_eq!(g, &wide(&want), "seed {seed} position {p}");
        }
    }
}

// ---- tir/lower's lean forms: the same values in fewer nodes ------------------------------------

/// **The 21-node unit row is the template's value, and the live kernel's**, on
/// [`kdesc_q36_rms_norm_wide`]'s operand set — every row, every eps whose `mantissa · 2^shift` fits
/// the one `i64` param the lean form takes (a mantissa past it is cut to the largest that does, at
/// the same shift, so every shift of the set is still probed).
#[test]
fn kdesc_q36_rms_norm_wide_lean_form() {
    let hd = 16u32;
    let mut rng = Lcg(44);
    let mut xs = rng.vec((T * hd) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    xs[..hd as usize].iter_mut().for_each(|v| *v = 0);
    xs[hd as usize..2 * hd as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 2 { 3 } else { 0 });
    let ez = rng.vec(T as usize, 0, i64::MAX as i128, &[0, 1, 1 << 30, i64::MAX as i128]);
    let es = shifts(&mut rng, T as usize);
    let ez: Vec<i128> = ez.iter().zip(&es).map(|(z, s)| (*z).min(i64::MAX as i128 >> s)).collect();
    let eps: Vec<i128> = ez.iter().zip(&es).map(|(z, s)| z << s).collect();
    assert!(eps.iter().all(|e| *e <= i64::MAX as i128));
    let lean = run_pos(
        &[arg("x", DType::I32, &[T, hd], xs.clone()), arg("eps", DType::I64, &[T, 1], eps.clone())],
        &[],
        T,
        |b, r, _, pos| {
            let (x, e) = (row(b, r[0], pos), row(b, r[1], pos));
            let e = b.reshape_fixed(e, &[]);
            b.rms_unit_q24(x, e)
        },
    );
    let args = [
        arg("x", DType::I32, &[T, hd], xs.clone()),
        arg("ez", DType::I64, &[T, 1], ez.clone()),
        arg("es", DType::I8, &[T, 1], es.clone()),
    ];
    let template = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, z, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.rms_norm_wide_q36_exact(x, z, s)
    });
    for p in 0..T as usize {
        let x = i32s(&xs[p * hd as usize..(p + 1) * hd as usize]);
        let live = wide(&q36::q36_rms_norm_wide(&x, triple(1, es[p], ez[p])).unwrap());
        assert_eq!(lean[p], template[p], "eps {}·2^{} position {p}: the lean form is the template's value", ez[p], es[p]);
        assert_eq!(lean[p], live, "position {p}: and the live kernel's");
    }
}

/// **The 17-node L2 row is the template's value, and the live kernel's**, on
/// [`kdesc_q36_l2_norm`]'s operand set (a zero head and a one-lane head included).
#[test]
fn kdesc_q36_l2_norm_lean_form() {
    let (heads, hd) = (4u32, 8u32);
    let mut rng = Lcg(40);
    let mut xs = rng.vec((T * heads * hd) as usize, -32767, 32767, CODEX);
    xs[..hd as usize].iter_mut().for_each(|v| *v = 0);
    xs[hd as usize..2 * hd as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 1 { 1 } else { 0 });
    let form = |lean: bool| {
        run_pos(&[arg("x", DType::I16, &[T, heads * hd], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[0], pos);
            let x = b.reshape_fixed(x, &[heads, hd]);
            let y = if lean { b.l2_unit_q15(x) } else { b.l2_norm_q15(x) };
            b.reshape_fixed(y, &[heads * hd])
        })
    };
    let (lean, template) = (form(true), form(false));
    for p in 0..T as usize {
        let live: Vec<i32> = xs[p * (heads * hd) as usize..(p + 1) * (heads * hd) as usize]
            .chunks(hd as usize)
            .flat_map(|h| q36::q36_l2_norm(&i32s(h)).unwrap())
            .collect();
        assert_eq!(lean[p], template[p], "position {p}: the lean form is the template's value");
        assert_eq!(lean[p], wide(&live), "position {p}: and the live kernel's");
    }
}
