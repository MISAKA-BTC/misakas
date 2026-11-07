//! **The device's 128-bit library equals Rust's `i128` and the reference's division.** Every wide
//! operation the kernels use — `+`, `−`, the low 128 bits of `·`, `Div` under the three rules (by a
//! power of two — the shifts, including amounts past 64 — and by any divisor through the 128-bit
//! long division), `Compare`, `Select`, `Clamp`, `Log2Floor` — through the elementwise kernel with
//! `i128` operands and output, on the rails (`i128::MIN`, `MAX`, `±2^63`, `±2^64`, …), powers of two
//! and their neighbours (the ties of every shift), and random values at every magnitude.

mod common;

use misaka_palw_tir::{DType, Rounding, arith};
use misaka_palw_tir_exec::elem::Slice;
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_gpu::wgsl::EwOp;
use misaka_palw_tir_gpu::{DevTensor, Form, GpuDevice, Recorder};

use common::{XorShift, device};

fn samples() -> Vec<i128> {
    let mut v: Vec<i128> = vec![i128::MIN, i128::MIN + 1, -1, 0, 1, 2, 3, i128::MAX, i128::MAX - 1];
    for b in 0..127 {
        for d in [-2i128, -1, 0, 1, 2] {
            let x = (1i128 << b).wrapping_add(d);
            v.push(x);
            v.push(x.wrapping_neg());
        }
    }
    let mut s = XorShift(0x0123_4567_89AB_CDEF);
    for _ in 0..20_000 {
        let r = ((s.next() as u128) << 64 | s.next() as u128) as i128;
        let shift = (s.next() % 127) as u32;
        v.push(r >> shift);
    }
    v
}

/// `op` over i128 operands, i128 out: `(values, status)`.
fn ew128(dev: &GpuDevice, op: EwOp, ins: &[&[i128]], consts: &[i64]) -> (Vec<i128>, u32) {
    let n = ins[0].len();
    let ts: Vec<DevTensor> = ins.iter().map(|v| dev.upload(Slice::I128(v), Form::I128, &[n])).collect();
    let out = DevTensor { buf: dev.alloc(Form::I128, n), form: Form::I128, dtype: DType::I128, layout: Layout::contiguous(&[n]) };
    let mut rec = Recorder::new(dev, 1);
    let ops: Vec<(&DevTensor, Layout)> = ts.iter().map(|t| (t, t.layout)).collect();
    rec.ew(op, &[n], (&out.buf, Form::I128, &out.layout), &ops, consts, &[], None, false, 0).unwrap();
    let st = rec.finish()[0];
    (dev.download(&out).to_i128s(), st)
}

#[test]
fn wide_add_sub_mul_compare_select_clamp_and_log2_equal_rust_i128() {
    let Some(dev) = device() else { return };
    let xs = samples();
    let mut ys = xs.clone();
    ys.rotate_left(7919);
    let (add, st) = ew128(&dev, EwOp::Add, &[&xs, &ys], &[]);
    assert_eq!(st, 0);
    let (sub, _) = ew128(&dev, EwOp::Sub, &[&xs, &ys], &[]);
    let (mul, _) = ew128(&dev, EwOp::Mul, &[&xs, &ys], &[]);
    for i in 0..xs.len() {
        let (x, y) = (xs[i], ys[i]);
        assert_eq!(add[i], x.wrapping_add(y), "{x} + {y}");
        assert_eq!(sub[i], x.wrapping_sub(y), "{x} − {y}");
        assert_eq!(mul[i], x.wrapping_mul(y), "{x} · {y} (low 128 bits)");
    }
    for (cmp, f) in [
        (0u8, (|a, b| a == b) as fn(i128, i128) -> bool),
        (1, |a, b| a != b),
        (2, |a, b| a < b),
        (3, |a, b| a <= b),
        (4, |a, b| a > b),
        (5, |a, b| a >= b),
    ] {
        let (got, _) = ew128(&dev, EwOp::Compare(cmp), &[&xs, &ys], &[]);
        for i in 0..xs.len() {
            assert_eq!(got[i], f(xs[i], ys[i]) as i128, "compare {cmp}: {} vs {}", xs[i], ys[i]);
        }
    }
    let cs: Vec<i128> = xs.iter().map(|x| x & 1).collect();
    let (sel, _) = ew128(&dev, EwOp::Select, &[&cs, &xs, &ys], &[]);
    for i in 0..xs.len() {
        assert_eq!(sel[i], if cs[i] != 0 { xs[i] } else { ys[i] });
    }
    for (lo, hi) in [(i64::MIN, i64::MAX), (-5, 7), (0, 1 << 40), (i64::MIN, -3)] {
        let (cl, _) = ew128(&dev, EwOp::Clamp, &[&xs], &[lo, hi]);
        for i in 0..xs.len() {
            assert_eq!(cl[i], xs[i].clamp(lo as i128, hi as i128), "clamp {} to [{lo}, {hi}]", xs[i]);
        }
    }
    let (lg, _) = ew128(&dev, EwOp::Log2Floor, &[&xs], &[]);
    for i in 0..xs.len() {
        assert_eq!(lg[i], arith::log2_floor(xs[i]), "log2({})", xs[i]);
    }
    eprintln!("{} samples: + − · compare select clamp log2 equal", xs.len());
}

#[test]
fn wide_division_equals_the_reference_under_every_rule() {
    let Some(dev) = device() else { return };
    let xs = samples();
    let mut ds: Vec<i128> = vec![
        1,
        3,
        5,
        7,
        1536,
        (1 << 31) + 1,
        (1 << 32) + 1,
        (1 << 63) - 1,
        1 << 63,
        (1 << 64) + 1,
        (1 << 100) + 3,
        i128::MAX - 1,
        i128::MAX,
    ];
    ds.extend((0..127).map(|b| 1i128 << b));
    for (rule, tag) in [(Rounding::Floor, 0u8), (Rounding::HalfUp, 1), (Rounding::HalfAwayFromZero, 2)] {
        let (mut x_all, mut d_all) = (Vec::new(), Vec::new());
        for x in xs.iter().step_by(3) {
            for d in &ds {
                x_all.push(*x);
                d_all.push(*d);
            }
        }
        // Exact halves of even divisors past 64 bits.
        for d in [(1i128 << 70) + 2, 6, (1i128 << 100) * 6] {
            for q in [-3i128, -1, 0, 1, 2] {
                x_all.push(q * d + d / 2);
                d_all.push(d);
                x_all.push(q * d - d / 2);
                d_all.push(d);
            }
        }
        let (got, st) = ew128(&dev, EwOp::Div(tag), &[&x_all, &d_all], &[]);
        assert_eq!(st, 0);
        for i in 0..x_all.len() {
            assert_eq!(got[i], arith::div_round(x_all[i], d_all[i], rule).unwrap(), "{rule:?}: {} / {}", x_all[i], d_all[i]);
        }
        eprintln!("Div {rule:?}: {} pairs equal", x_all.len());
    }
}
