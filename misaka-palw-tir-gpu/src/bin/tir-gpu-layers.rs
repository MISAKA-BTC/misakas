//! **`tir-gpu-layers`: a Qwen2.5-1.5B-shaped program on the device executor and the CPU executor.**
//!
//! The program is `tir-exec-bench`'s (`misaka-palw-tir-exec/src/bin/tir-exec-bench.rs`, the dense
//! lowering's conventions): an `i32` residual carry, `i8` weights `[out, in]` read by
//! `MatMul(W, x[in, 1]) → i64` and narrowed per channel `(m, s, z)`, the wide RMS norm, RoPE by
//! `rotate_half` from two-level angle tables, grouped-query attention over `k`/`v` histories with a
//! two-pass Q24 softmax, SiLU as a 65,536-entry code table, the GLU product narrowed to codes, the LM
//! head over the whole vocabulary — with synthetic weights of the real shapes, over `--layers`
//! layers (default 2: the whole model does not fit the shared host's test budget). Both executors
//! step the same tokens; the logits must be equal at every position before any time is reported.
//!
//! ```text
//! cargo run --release --bin tir-gpu-layers -- [--layers 2] [--positions 24]
//! ```

use std::time::Instant;

use misaka_palw_tir_exec::{NoSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_gpu::fixtures::{Geo, Rng, fill, qwen2_program};
use misaka_palw_tir_gpu::{GpuDevice, GpuExecutor};

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = Geo { layers: arg(&args, "--layers", 2), d: 1536, heads: 12, kv: 2, hd: 128, ff: 8960, vocab: 151_936 };
    let positions: usize = arg(&args, "--positions", 24);
    let dev = GpuDevice::new().expect("a TIR device");
    println!("device: {}; CPU pool: {} threads", dev.describe(), rayon::current_num_threads());
    let (program, fills) = qwen2_program(&g);
    let plan = TirPlan::compile(&program).expect("the program validates");
    println!(
        "Qwen2.5-1.5B geometry, {} layers: {} nodes per position, {} bytes of program",
        g.layers,
        plan.slots_per_position(),
        program.encode().len()
    );
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut params = TirParams::new(&plan);
    let mut bytes = 0usize;
    for &(j, layer) in &plan.param_instances {
        let d = &program.params[j as usize];
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        bytes += n * d.dtype.width();
        params.insert(&plan, j, layer, fill(fills[j as usize], n, &mut rng)).expect("a param of its declaration");
    }
    println!("params: {:.2} GiB synthetic", bytes as f64 / (1u64 << 30) as f64);
    let mut cpu = TirExecutor::new(&plan, &params).expect("bound");
    let t = Instant::now();
    let mut gpu = GpuExecutor::new(&dev, &plan, &params).expect("bound");
    println!("device executor: params uploaded in {:.2} s", t.elapsed().as_secs_f64());
    let tokens: Vec<u32> = (0..positions).map(|i| ((i as u64 * 7919 + 1013) % g.vocab as u64) as u32).collect();
    let (mut tc, mut tg) = (Vec::new(), Vec::new());
    for (i, tok) in tokens.iter().enumerate() {
        let s = Instant::now();
        cpu.step(*tok, &mut NoSink).expect("a CPU step");
        tc.push(s.elapsed().as_secs_f64() * 1e3);
        let s = Instant::now();
        gpu.step(*tok, &mut NoSink).expect("a device step");
        tg.push(s.elapsed().as_secs_f64() * 1e3);
        assert_eq!(cpu.logits().1.to_i128s(), gpu.logits().1.to_i128s(), "position {i}: logits differ");
    }
    let warm = positions / 4;
    println!(
        "{} positions, logits equal at every one; median step after {warm}: CPU executor {:.1} ms, device executor {:.1} ms",
        positions,
        median(tc[warm..].to_vec()),
        median(tg[warm..].to_vec())
    );
    let st = &gpu.stats;
    let per = |x: u64| x as f64 / st.steps as f64;
    println!(
        "device executor per position: {:.0} device nodes, {:.0} views, {:.1} CPU fallbacks ({:.1} syncs), {:.0} dispatches",
        per(st.device_nodes),
        per(st.views),
        per(st.fallback.values().sum()),
        per(st.syncs),
        per(st.dispatches)
    );
    for (k, v) in &st.fallback {
        println!("  fallback {k}: {:.1} per position", per(*v));
    }
    println!(
        "  of which: {:.1} ms recording on the host, {:.1} ms submission to readback",
        per(st.record_ns) / 1e6,
        per(st.wait_ns) / 1e6
    );
}
