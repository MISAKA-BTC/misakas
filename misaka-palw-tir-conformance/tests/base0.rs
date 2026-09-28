//! **BASE-0** (ADR-0040 D + H): the ten kernels of the int8 tier against their segments.

mod common;

use common::*;
use kaspa_consensus_core::palw_base0_ops::{self as ops, QuantParams, ScaleParams};
use misaka_palw_tir::DType;
use misaka_palw_tir::Ref;
use misaka_palw_tir::program::INPUT_TOKEN;

const T: u32 = 48;

#[test]
fn kdesc_base0_embed() {
    let (rows, dim) = (16u32, 8u32);
    let mut rng = Lcg(1);
    let table = rng.vec((rows * dim) as usize, -128, 127, I8X);
    let tokens: Vec<u32> = (0..rows).chain([0, 15, 7]).collect();
    let got = run(&[arg("table", DType::I8, &[rows, dim], table.clone())], &[], &tokens, rows, |b, r, _, _| {
        b.embed(r[0], Ref::Input(INPUT_TOKEN))
    });
    for (t, g) in tokens.iter().zip(&got) {
        let table8 = i8s(&table);
        let want = ops::embed_lookup(&table8, rows as usize, dim as usize, *t as usize).unwrap();
        assert_eq!(g, &wide(want), "token {t}");
    }
}

#[test]
fn kdesc_base0_matmul() {
    let (out, n) = (5u32, 23u32);
    let mut rng = Lcg(2);
    let w = rng.vec((out * n) as usize, -128, 127, I8X);
    let xs = rng.vec((T * n) as usize, -128, 127, I8X);
    let got =
        run_pos(&[arg("w", DType::I8, &[out, n], w.clone()), arg("x", DType::I8, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[1], pos);
            b.base0_matmul(r[0], x)
        });
    for (p, g) in got.iter().enumerate() {
        let x = &xs[p * n as usize..(p + 1) * n as usize];
        assert_eq!(g, &wide(&ops::matmul_quant(&i8s(&w), &i8s(x), out as usize).unwrap()), "position {p}");
    }
}

#[test]
fn kdesc_base0_requantize() {
    let n = 9u32;
    let mut rng = Lcg(3);
    let mult_x: Vec<i128> = [I32X, &[1 << 29, 1 << 30]].concat();
    let acc = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let m = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, &mult_x);
    // The kernel's domain: `rounding_shift_right` asserts `s ≤ 31`, and the court's Requantize arm
    // refuses a registered shift past 31 as non-canonical.
    let s = rng.vec((T * n) as usize, 0, 31, &[0, 1, 30, 31]);
    let z = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let args = [
        arg("acc", DType::I32, &[T, n], acc.clone()),
        arg("m", DType::I32, &[T, n], m.clone()),
        arg("s", DType::I16, &[T, n], s.clone()),
        arg("z", DType::I32, &[T, n], z.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (a, m, s, z) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos), row(b, r[3], pos));
        b.requantize_base0_t(a, m, s, z)
    });
    for (p, g) in got.iter().enumerate() {
        let at = |v: &Vec<i128>, i: usize| v[p * n as usize + i];
        let params: Vec<QuantParams> = (0..n as usize)
            .map(|i| QuantParams { multiplier: at(&m, i) as i32, shift: at(&s, i) as u8, zero: at(&z, i) as i32 })
            .collect();
        let a: Vec<i32> = (0..n as usize).map(|i| at(&acc, i) as i32).collect();
        assert_eq!(g, &wide(&ops::requantize_row(&a, &params).unwrap()), "position {p}");
    }
}

#[test]
fn kdesc_base0_rescale() {
    let n = 7u32;
    let mut rng = Lcg(4);
    let acc = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let m = rng.vec(T as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let s = rng.vec(T as usize, 0, 62, &[0, 1, 30, 31, 32, 61, 62]);
    let args = [
        arg("acc", DType::I32, &[T, n], acc.clone()),
        arg("m", DType::I32, &[T, 1], m.clone()),
        arg("s", DType::I16, &[T, 1], s.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (a, m, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.rescale_base0_t(a, m, s)
    });
    for (p, g) in got.iter().enumerate() {
        let a: Vec<i32> = i32s(&acc[p * n as usize..(p + 1) * n as usize]);
        let want = ops::rescale_row(&a, ScaleParams { multiplier: m[p] as i32, shift: s[p] as u8 });
        assert_eq!(g, &wide(&want), "position {p}");
    }
}

#[test]
fn kdesc_base0_rms_norm() {
    let n = 17u32;
    let mut rng = Lcg(5);
    let mut xs = rng.vec((T * n) as usize, -128, 127, I8X);
    // A zero row and a one-hot row: the epsilon alone, and the sparse row whose product saturates.
    xs[..n as usize].iter_mut().for_each(|v| *v = 0);
    xs[n as usize..2 * n as usize].iter_mut().enumerate().for_each(|(i, v)| *v = if i == 3 { -128 } else { 0 });
    for eps in [0i64, 1, 1 << 20, -(1 << 40), i64::MAX] {
        let got = run_pos(&[arg("x", DType::I8, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
            let x = row(b, r[0], pos);
            b.base0_rms_norm(x, eps)
        });
        for (p, g) in got.iter().enumerate() {
            let x = i8s(&xs[p * n as usize..(p + 1) * n as usize]);
            assert_eq!(g, &wide(&ops::rms_norm(&x, eps).unwrap()), "eps {eps} position {p}");
        }
    }
}

#[test]
fn kdesc_base0_rope() {
    let pairs = 4u32;
    let mut rng = Lcg(6);
    let xs = rng.vec((T * 2 * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let c = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let s = rng.vec((T * pairs) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let args = [
        arg("x", DType::I32, &[T, 2 * pairs], xs.clone()),
        arg("cos", DType::I32, &[T, pairs], c.clone()),
        arg("sin", DType::I32, &[T, pairs], s.clone()),
    ];
    let got = run_pos(&args, &[], T, |b, r, _, pos| {
        let (x, c, s) = (row(b, r[0], pos), row(b, r[1], pos), row(b, r[2], pos));
        b.rope_pairs_wide(x, c, s, i32::MIN as i64, i32::MAX as i64, DType::I32)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * (2 * pairs) as usize..(p + 1) * (2 * pairs) as usize]);
        let cc = i32s(&c[p * pairs as usize..(p + 1) * pairs as usize]);
        let ss = i32s(&s[p * pairs as usize..(p + 1) * pairs as usize]);
        assert_eq!(g, &wide(&ops::rope_table(&x, &cc, &ss).unwrap()), "position {p}");
    }
}

#[test]
fn kdesc_base0_softmax() {
    let n = 11u32;
    let mut rng = Lcg(7);
    let xs = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let got = run_pos(&[arg("x", DType::I32, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        b.softmax_shifted(x, 0)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        assert_eq!(g, &wide(&ops::softmax(&x).unwrap()), "position {p}");
    }
}

/// Also `KDESC_Q36_SILU`: the hybrid's `Silu` arm is this kernel on the challenged lanes.
#[test]
fn kdesc_base0_silu_and_kdesc_q36_silu() {
    let n = 13u32;
    let mut rng = Lcg(8);
    let xs = rng.vec((T * n) as usize, i32::MIN as i128, i32::MAX as i128, I32X);
    let got = run_pos(&[arg("x", DType::I32, &[T, n], xs.clone())], &[], T, |b, r, _, pos| {
        let x = row(b, r[0], pos);
        b.silu(x)
    });
    for (p, g) in got.iter().enumerate() {
        let x = i32s(&xs[p * n as usize..(p + 1) * n as usize]);
        assert_eq!(g, &wide(&ops::silu(&x)), "position {p}");
    }
}

#[test]
fn kdesc_base0_mul_elem_and_add_elem() {
    let n = 10u32;
    let mut rng = Lcg(9);
    let a = rng.vec((T * n) as usize, -128, 127, I8X);
    let bb = rng.vec((T * n) as usize, -128, 127, I8X);
    for mul in [true, false] {
        let got =
            run_pos(&[arg("a", DType::I8, &[T, n], a.clone()), arg("b", DType::I8, &[T, n], bb.clone())], &[], T, |b, r, _, pos| {
                let (x, y) = (row(b, r[0], pos), row(b, r[1], pos));
                if mul { b.base0_mul_elem(x, y) } else { b.base0_add_elem(x, y) }
            });
        for (p, g) in got.iter().enumerate() {
            let x = i8s(&a[p * n as usize..(p + 1) * n as usize]);
            let y = i8s(&bb[p * n as usize..(p + 1) * n as usize]);
            let want = if mul { ops::mul_elem(&x, &y).unwrap() } else { ops::add_elem(&x, &y).unwrap() };
            assert_eq!(g, &wide(&want), "mul {mul} position {p}");
        }
    }
}
