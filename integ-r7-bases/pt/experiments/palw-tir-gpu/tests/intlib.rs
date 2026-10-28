//! **The device's integer library equals the reference evaluator's arithmetic** — every lossy
//! scalar function of spec 04b §6.4–6.5 as the kernels run it (through the elementwise kernel,
//! `i64` in and out), against `misaka_palw_tir::arith` (the `i128` definitions the court runs).
//!
//! `IntExp` is compared on EVERY input of its non-trivial domain `(−31·LN2_Q, 0]` (inputs made on
//! the device by `Iota`), the others on the CPU executor's own sample set: the rails, every power
//! of two and its neighbours, every range-reduction edge, 200,000 values at every magnitude. `Div`
//! is compared under all three rules over divisors that take every path of `div_rule64` — powers
//! of two (the shift), 32-bit divisors (the hardware division), wide and near-`2^63` divisors (the
//! long division, and its `d ≥ 2^63` shortcut).

mod common;

use misaka_palw_tir::DType;
use misaka_palw_tir::arith;
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_gpu::wgsl::EwOp;
use misaka_palw_tir_gpu::{DevTensor, Form, Recorder};
use rayon::prelude::*;

use common::{device, ew_i64, samples_i64};

#[test]
fn log2_floor_and_the_transcendentals_equal_the_reference_on_every_sample() {
    let Some(dev) = device() else { return };
    let xs = samples_i64();
    for (op, name, f) in [
        (EwOp::Log2Floor, "Log2Floor", arith::log2_floor as fn(i128) -> i128),
        (EwOp::IntExp, "IntExp", arith::int_exp),
        (EwOp::IntRsqrt, "IntRsqrt", arith::int_rsqrt),
        (EwOp::IntLn, "IntLn", arith::int_ln),
    ] {
        let (got, status) = ew_i64(&dev, op, &[&xs], &[]);
        assert_eq!(status, 0, "{name} reported a failure");
        let bad: Vec<_> = xs.iter().zip(&got).filter(|(x, g)| f(**x as i128) != **g as i128).take(5).collect();
        assert!(bad.is_empty(), "{name}: device ≠ reference at {bad:?}");
        eprintln!("{name}: {} samples equal", xs.len());
    }
}

#[test]
fn int_exp_equals_the_reference_on_every_input_of_its_domain() {
    let Some(dev) = device() else { return };
    // (−31·LN2_Q − 2) ..= 2: every input whose value is not the constant 0 of the far tail.
    let (lo, hi) = (-31 * 11_629_080i64 - 2, 2i64);
    let chunk = 1usize << 24;
    let total = (hi - lo + 1) as usize;
    let mut done = 0usize;
    let mut start = lo;
    while start <= hi {
        let n = chunk.min((hi - start + 1) as usize);
        let xs = DevTensor { buf: dev.alloc(Form::I64, n), form: Form::I64, dtype: DType::I64, layout: Layout::contiguous(&[n]) };
        let out = DevTensor { buf: dev.alloc(Form::I64, n), form: Form::I64, dtype: DType::I64, layout: Layout::contiguous(&[n]) };
        let mut rec = Recorder::new(&dev, 1);
        // Iota on the device: the inputs never cross the bus.
        rec.ew(EwOp::Iota, &[n], (&xs.buf, Form::I64, &xs.layout), &[], &[start, 1], &[3, 0], None, false, 0).unwrap();
        rec.ew(EwOp::IntExp, &[n], (&out.buf, Form::I64, &out.layout), &[(&xs, xs.layout)], &[], &[], None, false, 0).unwrap();
        assert_eq!(rec.finish()[0], 0);
        let got = dev.download(&out).to_i128s();
        let first_bad = (0..n).into_par_iter().find_first(|i| arith::int_exp((start + *i as i64) as i128) != got[*i]);
        assert!(first_bad.is_none(), "IntExp({}) differs", start + first_bad.unwrap() as i64);
        done += n;
        start += n as i64;
    }
    assert_eq!(done, total);
    eprintln!("IntExp: all {total} inputs of (−31·LN2_Q − 2) ..= 2 equal the reference");
}

#[test]
fn division_equals_the_reference_under_every_rule_and_every_path() {
    let Some(dev) = device() else { return };
    let xs = samples_i64();
    let mut ds: Vec<i64> = vec![
        1,
        3,
        5,
        7,
        1536,
        8960,
        11_629_080,
        (1 << 31) - 1,
        1 << 31,
        (1 << 31) + 1,
        (1 << 32) - 1,
        (1 << 32) + 1,
        3 << 40,
        (1 << 62) + 1,
        i64::MAX - 1,
        i64::MAX,
    ];
    ds.extend((0..63).map(|b| 1i64 << b));
    for (rule, tag) in [
        (misaka_palw_tir::Rounding::Floor, 0u8),
        (misaka_palw_tir::Rounding::HalfUp, 1),
        (misaka_palw_tir::Rounding::HalfAwayFromZero, 2),
    ] {
        let mut x_all = Vec::new();
        let mut d_all = Vec::new();
        for (i, x) in xs.iter().enumerate().step_by(5) {
            for d in &ds {
                x_all.push(*x);
                d_all.push(*d);
            }
            // The exact ties of the odd divisors: (2q+1)·d/2 has remainder d/2 only for even d.
            if i % 997 == 0 {
                for d in [6i64, 10, 1 << 20, (1 << 33) + 2] {
                    let half = d / 2;
                    for q in [-3i64, -1, 0, 1, 2] {
                        x_all.push(q.wrapping_mul(d).wrapping_add(half));
                        d_all.push(d);
                        x_all.push(q.wrapping_mul(d).wrapping_sub(half));
                        d_all.push(d);
                    }
                }
            }
        }
        let (got, status) = ew_i64(&dev, EwOp::Div(tag), &[&x_all, &d_all], &[]);
        assert_eq!(status, 0, "{rule:?}: a divisor ≥ 1 never fails");
        let bad = (0..x_all.len())
            .into_par_iter()
            .find_first(|i| arith::div_round(x_all[*i] as i128, d_all[*i] as i128, rule).expect("d ≥ 1") != got[*i] as i128);
        assert!(bad.is_none(), "{rule:?}: {} / {} differs", x_all[bad.unwrap()], d_all[bad.unwrap()]);
        eprintln!("Div {rule:?}: {} pairs equal", x_all.len());
    }
}

#[test]
fn a_divisor_below_one_fails_with_divisor_at_the_first_such_element() {
    let Some(dev) = device() else { return };
    let x = [10i64, 11, 12, 13];
    let d = [3i64, 1, 0, -5];
    let (_, status) = ew_i64(&dev, EwOp::Div(0), &[&x, &d], &[]);
    let f = misaka_palw_tir_gpu::DeviceFailure::decode(status).expect("a failure");
    assert_eq!(f.element, 2);
    assert_eq!(f.kind, misaka_palw_tir::TirErrorKind::Divisor);
}
