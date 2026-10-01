//! Shared test helpers: the device (or a skip on a host without one), and one-kernel runs.

#![allow(dead_code)]

pub mod one_node;

use misaka_palw_tir::DType;
use misaka_palw_tir_exec::elem::Slice;
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_gpu::wgsl::EwOp;
use misaka_palw_tir_gpu::{DevTensor, Form, GpuDevice, Recorder};

/// The TIR device, or `None` (the test then passes vacuously and says so): the conformance suite
/// is about the device; a host without one has nothing to conform.
pub fn device() -> Option<GpuDevice> {
    match GpuDevice::new() {
        Ok(d) => {
            eprintln!("device: {}", d.describe());
            Some(d)
        }
        Err(e) => {
            eprintln!("SKIPPED — no TIR device: {e}");
            None
        }
    }
}

/// `op` over `ins` (each `i64`, `n` elements), out `i64`: `(values, status word)`.
pub fn ew_i64(dev: &GpuDevice, op: EwOp, ins: &[&[i64]], consts: &[i64]) -> (Vec<i64>, u32) {
    let n = ins.first().map(|v| v.len()).unwrap_or(0);
    let tensors: Vec<DevTensor> = ins.iter().map(|v| dev.upload(Slice::I64(v), Form::I64, &[v.len()])).collect();
    let out = DevTensor { buf: dev.alloc(Form::I64, n), form: Form::I64, dtype: DType::I64, layout: Layout::contiguous(&[n]) };
    let mut rec = Recorder::new(dev, 1);
    let ops: Vec<(&DevTensor, Layout)> = tensors.iter().map(|t| (t, t.layout)).collect();
    rec.ew(op, &[n], (&out.buf, Form::I64, &out.layout), &ops, consts, &[], None, false, 0).expect("an i64 kernel");
    let status = rec.finish();
    let vals = dev.download(&out).to_i128s().iter().map(|v| *v as i64).collect();
    (vals, status[0])
}

/// xorshift64: deterministic samples without a dependency.
pub struct XorShift(pub u64);
impl XorShift {
    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// The `i64` samples `misaka_palw_tir_exec::scalar`'s own tests use: the rails, every power of two
/// and its neighbours (both signs), every `IntExp` range-reduction bucket edge, and 200,000
/// xorshift values at every magnitude.
pub fn samples_i64() -> Vec<i64> {
    let ln2_q = 11_629_080i64;
    let mut v: Vec<i64> = vec![i64::MIN, i64::MIN + 1, -1, 0, 1, 2, 3, i64::MAX, i64::MAX - 1];
    for b in 0..63 {
        for d in [-2i64, -1, 0, 1, 2] {
            let x = (1i64 << b).wrapping_add(d);
            v.push(x);
            v.push(x.wrapping_neg());
        }
    }
    for z in 0..33i64 {
        for d in -3..=3 {
            v.push(-z * ln2_q + d);
        }
    }
    let mut s = XorShift(0x9E37_79B9_7F4A_7C15);
    for _ in 0..200_000 {
        let r = s.next();
        let shift = (r >> 58) as u32;
        v.push((r as i64) >> shift);
    }
    v
}
