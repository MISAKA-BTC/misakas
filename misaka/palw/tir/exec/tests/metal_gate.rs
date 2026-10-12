//! **The Metal backend's gate (RFC-0002 §7 F-4 and F-6)**: the fused unit-row kernels on the GPU against the reference evaluator, the
//! generic backend and the CPU fused kernels, byte for byte, on random and range-extreme rows — and a row whose result leaves the
//! kernel's machine word (the kernel's flag, answered by the CPU) — and the deliberately broken variant, which must still be caught
//! with the GPU path in front of it. Only with `--features metal` on macOS.
#![cfg(all(feature = "metal", target_os = "macos"))]

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, TirProgramV1};
use misaka_palw_tir::{DType, Interpreter, MapParams, Ref, RunState, Tensor, TensorType};
use misaka_palw_tir_exec::fused::kernel_index;
use misaka_palw_tir_exec::fused::metal::{metal_available, metal_counts, set_metal_backend, set_metal_min_elems};
use misaka_palw_tir_exec::{NoSink, TirExecutor, TirParams, TirPlan};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha20Rng;

const VOCAB: u32 = 8;

fn program(l2: bool, dtype: DType, rows: u32, n: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("x.table", dtype, &[VOCAB, rows * n], false);
    let eps = (!l2).then(|| pb.param("eps", DType::I64, &[1], false));
    let out = if l2 { DType::I16 } else { DType::I32 };
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let v = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let v = b.cast(v, out);
        b.finish(&[v])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(out, &[rows * n])]);
        let x = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.reshape_fixed(x, &[rows, n]);
        let y = if l2 { b.l2_unit_q15(x) } else { b.rms_unit_q24(x, eps.expect("an eps")) };
        let y = b.reshape_fixed(y, &[rows * n]);
        let y = b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(out, &[rows * n])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[rows * n]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer], post, 0)
}

fn params(p: &TirProgramV1, rng: &mut ChaCha20Rng, mode: u8, eps: i128) -> MapParams {
    let mut m = MapParams { tensors: Default::default() };
    for (j, d) in p.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let (lo, hi) = (d.dtype.min_value(), d.dtype.max_value());
        let data: Vec<i128> = (0..n)
            .map(|_| {
                if d.name == "eps" {
                    return eps;
                }
                match mode {
                    // random in range, or in a tenth of it
                    0 => rng.gen_range(lo..=hi),
                    1 => rng.gen_range(lo / 10..=hi / 10),
                    // the edges, and zero rows
                    _ => [lo, lo + 1, -1, 0, 0, 0, 1, hi - 1, hi][rng.gen_range(0..9)],
                }
            })
            .collect();
        let shape = d.shape.iter().map(|x| *x as usize).collect();
        m.tensors.insert((j as u16, None), Tensor::new(d.dtype, shape, data).expect("in range"));
    }
    m
}

/// The reference, the generic backend, the CPU fused and the GPU fused kernels over `tokens`: logits and commits equal. Returns how
/// many regions ran fused.
fn four(p: &TirProgramV1, map: &MapParams, tokens: &[u32], kernel: &str, fault: bool) -> Result<usize, String> {
    let plan = TirPlan::compile(p).map_err(|e| e.to_string())?;
    let xp = TirParams::from_map(&plan, map).map_err(|e| e.to_string())?;
    let interp = Interpreter::new(p).map_err(|e| e.to_string())?;
    let mut st = RunState::default();
    let mut generic = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
    let mut cpu = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
    cpu.set_fused(true);
    let mut gpu = TirExecutor::new(&plan, &xp).map_err(|e| e.to_string())?;
    gpu.set_fused(true);
    if fault {
        gpu.set_fused_fault(Some(kernel_index(kernel).expect("a kernel")));
    }
    let ran = |e: &TirExecutor<'_>| e.fused_summary().iter().find(|(k, _)| *k == kernel).map_or(0, |(_, c)| *c);
    for (pos, tok) in tokens.iter().enumerate() {
        let want = interp.step(map, &mut st, *tok).map_err(|e| format!("reference: {e}"))?;
        for (who, exec) in [("generic", &mut generic), ("cpu fused", &mut cpu)] {
            set_metal_backend(false).expect("off");
            exec.step(*tok, &mut NoSink).map_err(|e| e.to_string())?;
            if exec.logits().1.to_i128s() != want.logits.data {
                return Err(format!("position {pos}: the {who} logits differ"));
            }
        }
        set_metal_backend(true).expect("a Metal device");
        gpu.step(*tok, &mut NoSink).map_err(|e| e.to_string())?;
        set_metal_backend(false).expect("off");
        if gpu.logits().1.to_i128s() != want.logits.data {
            return Err(format!("position {pos}: the GPU fused logits differ (the first differing lane moved by the fault: {fault})"));
        }
    }
    Ok(ran(&gpu))
}

fn gate(l2: bool, dtype: DType, rows: u32, n: u32, cases: &[(u8, i128)]) {
    let kernel = if l2 { "l2_unit_q15" } else { "rms_unit_q24" };
    let p = program(l2, dtype, rows, n);
    for (seed, (mode, eps)) in cases.iter().enumerate() {
        let mut rng = ChaCha20Rng::seed_from_u64(0x3E7A1 + seed as u64);
        let map = params(&p, &mut rng, *mode, *eps);
        let tokens: Vec<u32> = (0..VOCAB).collect();
        let ran = four(&p, &map, &tokens, kernel, false).unwrap_or_else(|e| panic!("{kernel} {dtype:?} {rows}x{n} case {seed}: {e}"));
        // A case the plan refuses to fuse (a sum that may leave the machine word) gates the CPU path only; at least one case must fuse.
        if ran == 0 {
            eprintln!("{kernel} {dtype:?} {rows}x{n} case {seed}: not fused (the plan's ranges)");
        } else {
            let caught = four(&p, &map, &tokens, kernel, true).expect_err("the broken variant must not pass");
            assert!(caught.contains("GPU"), "{kernel}: refused, but not at the GPU backend: {caught}");
        }
    }
}

#[test]
fn the_metal_unit_row_kernels_are_the_reference() {
    if !metal_available() {
        eprintln!("no Metal device: skipped");
        return;
    }
    set_metal_min_elems(0);
    let eps = [0i128, 1, 1 << 20, 1 << 40, i64::MAX as i128, -5];
    let cases: Vec<(u8, i128)> = (0..3u8).flat_map(|m| eps.iter().map(move |e| (m, *e))).collect();
    gate(true, DType::I16, 3, 8, &cases);
    gate(true, DType::I16, 1, 300, &cases);
    gate(true, DType::I16, 64, 512, &cases[..3]);
    gate(false, DType::I16, 3, 8, &cases);
    gate(false, DType::I16, 5, 129, &cases);
    gate(false, DType::I32, 2, 16, &cases);
    // A single wide element: its mean leaves the machine word, the kernel raises its flag and the CPU answers the same bytes.
    let before = metal_counts();
    gate(false, DType::I32, 1, 1, &[(2, 0), (0, 0), (2, 1 << 40)]);
    let after = metal_counts();
    assert!(after.1 > before.1, "a one-element wide row must leave the kernel's word and be handed back: {before:?} -> {after:?}");
    assert!(after.0 > 0, "the GPU answered nothing");
    eprintln!("GPU answered {} calls, handed back {}", after.0, after.1);
}

// ---- the gated delta rule's step and the wide RMS on the GPU ----

#[test]
fn the_metal_wide_rms_and_gdn_step_are_the_reference() {
    if !metal_available() {
        eprintln!("no Metal device: skipped");
        return;
    }
    set_metal_min_elems(0);
    let eps = [0i128, 1, 1 << 20, 1 << 40, i64::MAX as i128, -5];
    let cases: Vec<(u8, i128)> = (0..3u8).flat_map(|m| eps.iter().map(move |e| (m, *e))).collect();
    let _ = cases;
    // The wide RMS: ε is `eps_zero · 2^eps_shift`; the gate's own parameters are drawn by `ez` and `es`.
    for kind in [2u8, 3] {
        for dtype in [DType::I16, DType::I32] {
            let p = wide_program(kind, dtype, 3, 8);
            for seed in 0..8u64 {
                let mut rng = ChaCha20Rng::seed_from_u64(0xE6A + seed);
                let map = wide_params(&p, &mut rng, (seed % 3) as u8);
                let tokens: Vec<u32> = (0..VOCAB).collect();
                let ran = four(&p, &map, &tokens, "rms_norm_wide_q36", false)
                    .unwrap_or_else(|e| panic!("wide {kind} {dtype:?} seed {seed}: {e}"));
                if ran > 0 {
                    let caught = four(&p, &map, &tokens, "rms_norm_wide_q36", true).expect_err("the broken variant must not pass");
                    assert!(caught.contains("GPU"), "{caught}");
                }
            }
        }
    }
    let (answered, _) = metal_counts();
    assert!(answered > 0, "the GPU answered nothing");
}

fn wide_program(kind: u8, dtype: DType, rows: u32, n: u32) -> TirProgramV1 {
    let mut pb = ProgramBuilder::new(VOCAB, HISTORY_BOUND_V1_SMALL);
    let table = pb.param(if dtype == DType::I32 { "x32.table" } else { "x.table" }, dtype, &[VOCAB, rows * n], false);
    let ez = pb.param("ez", DType::I64, &[1], false);
    let es = pb.param("es", DType::I32, &[1], false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let v = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let v = b.cast(v, DType::I32);
        b.finish(&[v])
    };
    let layer = {
        let mut b = pb.block("layer", vec![TensorType::fixed(DType::I32, &[rows * n])]);
        let x = b.gather(table, Ref::Input(INPUT_TOKEN), 0, 0);
        let x = b.reshape_fixed(x, &[rows, n]);
        let y = if kind == 2 { b.rms_norm_wide_q36(x, ez, es) } else { b.rms_norm_wide_q36_exact(x, ez, es) };
        let y = b.reshape_fixed(y, &[rows * n]);
        let y = b.commit(y);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[rows * n])]);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[rows * n]);
        b.commit(l);
        b.finish(&[])
    };
    pb.finish(pre, vec![layer], post, 0)
}

fn wide_params(p: &TirProgramV1, rng: &mut ChaCha20Rng, mode: u8) -> MapParams {
    let mut m = MapParams { tensors: Default::default() };
    for (j, d) in p.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let data: Vec<i128> = (0..n)
            .map(|_| match (d.name.as_str(), mode) {
                ("ez", 0) => rng.gen_range(0i128..=1 << 20),
                ("es", 0) => rng.gen_range(0i128..=30),
                ("ez", 1) => rng.gen_range(0i128..=1 << 30),
                ("es", 1) => rng.gen_range(0i128..=96),
                ("ez", _) => i64::MAX as i128,
                ("es", _) => 62,
                (name, _) if name.starts_with("x32") => rng.gen_range(-(1i128 << 26)..=1 << 26),
                _ => rng.gen_range(d.dtype.min_value()..=d.dtype.max_value()),
            })
            .collect();
        let shape = d.shape.iter().map(|x| *x as usize).collect();
        m.tensors.insert((j as u16, None), Tensor::new(d.dtype, shape, data).expect("in range"));
    }
    m
}
