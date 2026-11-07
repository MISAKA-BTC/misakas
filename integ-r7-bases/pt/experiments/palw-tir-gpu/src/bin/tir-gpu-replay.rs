//! **`tir-gpu-replay`: a whole job's replay — the CPU executor stepping it, the device replaying it
//! a layer at a time over all of its positions.**
//!
//! The Qwen2.5-1.5B-shaped program of `tir-gpu-layers` (`tir-exec-bench`'s: the dense lowering's
//! conventions, synthetic weights of the real shapes, `--layers` layers). The CPU executor steps the
//! longest job once, position after position; the time to each shorter job is read off the same
//! run, since a job is a prefix of a longer one. [`BatchReplay`] replays each job from the initial
//! state, in chunks of `--chunk` positions (a chunk's values are on the device together: the LM
//! head's `i64` products alone are `vocab × chunk × 8` bytes). The device step executor
//! ([`GpuExecutor`], one position at a time) runs the first `--step` positions for scale. At every
//! position the committed values and the logits — a digest of each position's values in the sink's
//! order — must agree with the CPU executor's before any time is printed.
//!
//! ```text
//! cargo run --release --bin tir-gpu-replay -- [--layers 2] [--positions 256,512,1024] [--chunk 256] [--step 32]
//! ```

use std::time::Instant;

use misaka_palw_tir_exec::elem::{Elem, Slice};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, TirParams, TirPlan, with_slice};
use misaka_palw_tir_gpu::fixtures::{Geo, Rng, fill, qwen2_program};
use misaka_palw_tir_gpu::{BatchReplay, GpuDevice, GpuExecutor, ReplaySink};

fn arg<T: std::str::FromStr>(args: &[String], name: &str, default: T) -> T {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn mix(h: u64, x: u64) -> u64 {
    (h.rotate_left(5) ^ x).wrapping_mul(0x517c_c1b7_2722_0a95)
}

fn mix_slice(mut h: u64, data: Slice<'_>) -> u64 {
    h = mix(h, data.len() as u64);
    with_slice!(data, v => for x in v.iter() { h = mix(h, x.to_i64() as u64); });
    h
}

/// One digest per position: every value handed to the sink (slot, then elements), then the logits.
#[derive(Default)]
struct Digests {
    pos: usize,
    per: Vec<u64>,
}

impl Digests {
    fn at(&mut self, p: usize) -> &mut u64 {
        while self.per.len() <= p {
            self.per.push(0x9e37_79b9_7f4a_7c15);
        }
        &mut self.per[p]
    }
    fn value(&mut self, p: usize, v: &NodeValue<'_>) {
        let h = self.at(p);
        *h = mix_slice(mix(*h, v.slot as u64), v.data);
    }
    fn logits(&mut self, p: usize, data: Slice<'_>) {
        let h = self.at(p);
        *h = mix_slice(mix(*h, u64::MAX), data);
    }
}

impl StepSink for Digests {
    fn node(&mut self, v: &NodeValue<'_>) {
        self.value(self.pos, v);
    }
}

impl ReplaySink for Digests {
    fn node(&mut self, pos: u32, v: &NodeValue<'_>) {
        self.value(pos as usize, v);
    }
    fn logits(&mut self, pos: u32, _shape: &[usize], data: Slice<'_>) {
        Digests::logits(self, pos as usize, data);
    }
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let g = Geo { layers: arg(&args, "--layers", 2), d: 1536, heads: 12, kv: 2, hd: 128, ff: 8960, vocab: 151_936 };
    let jobs: Vec<usize> = arg(&args, "--positions", "256,512,1024".to_string())
        .split(',')
        .map(|s| s.trim().parse().expect("a position count"))
        .collect();
    let chunk: usize = arg(&args, "--chunk", 256);
    let steps: usize = arg(&args, "--step", 32);
    let longest = *jobs.iter().max().expect("a job");
    let dev = GpuDevice::new().expect("a TIR device");
    println!("device: {}; CPU pool: {} threads", dev.describe(), rayon::current_num_threads());
    let (program, fills) = qwen2_program(&g);
    let plan = TirPlan::compile(&program).expect("the program validates");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut params = TirParams::new(&plan);
    for &(j, layer) in &plan.param_instances {
        let d = &program.params[j as usize];
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        params.insert(&plan, j, layer, fill(fills[j as usize], n, &mut rng)).expect("a param of its declaration");
    }
    println!(
        "Qwen2.5-1.5B geometry, {} layers: {} nodes per position; jobs of {jobs:?} positions; device chunks of {chunk}",
        g.layers,
        plan.slots_per_position()
    );
    let tokens: Vec<u32> = (0..longest).map(|i| ((i as u64 * 7919 + 1013) % g.vocab as u64) as u32).collect();

    // The CPU executor, position after position, once over the longest job.
    let mut cpu = TirExecutor::new(&plan, &params).expect("bound");
    let mut want = Digests::default();
    let mut cpu_ms = Vec::with_capacity(longest);
    let t0 = Instant::now();
    for (i, tok) in tokens.iter().enumerate() {
        want.pos = i;
        let s = Instant::now();
        cpu.step(*tok, &mut want).expect("a CPU step");
        want.logits(i, cpu.logits().1);
        cpu_ms.push(s.elapsed().as_secs_f64() * 1e3);
    }
    println!("CPU executor: {longest} positions stepped in {:.1} s", t0.elapsed().as_secs_f64());

    // The device step executor, for scale.
    let mut step = GpuExecutor::new(&dev, &plan, &params).expect("bound");
    let mut step_ms = Vec::new();
    let mut got = Digests::default();
    for (i, tok) in tokens.iter().take(steps).enumerate() {
        got.pos = i;
        let s = Instant::now();
        step.step(*tok, &mut got).expect("a device step");
        got.logits(i, step.logits().1.slice());
        step_ms.push(s.elapsed().as_secs_f64() * 1e3);
        assert_eq!(got.per[i], want.per[i], "position {i}: the step executor differs from the CPU executor");
    }
    let step_med = if step_ms.is_empty() { f64::NAN } else { median(step_ms.clone()) };
    println!(
        "device step executor: {steps} positions equal; median {step_med:.1} ms a position, {:.0} dispatches a position",
        step.stats.dispatches as f64 / steps.max(1) as f64
    );
    drop(step);

    // The batched replay, each job from the initial state.
    let mut r = BatchReplay::new(&dev, &plan, &params).expect("bound");
    r.chunk = chunk;
    let mut warm = Digests::default();
    let w = r.replay(&tokens[..8.min(longest)], &mut warm);
    assert!(w.failure.is_none(), "the warm-up replay succeeds");
    println!("batched replay: pipelines compiled in a warm-up of {} positions ({} pipelines)", w.positions, dev.pipeline_count());
    println!();
    println!(
        "| positions | CPU executor | device step executor (median × positions) | batched replay | × CPU | × step | dispatches | recording / waiting / delivery |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- |");
    let mut breakdown = Vec::new();
    for &n in &jobs {
        let before = r.stats.clone();
        let mut got = Digests::default();
        let t = Instant::now();
        let out = r.replay(&tokens[..n], &mut got);
        let secs = t.elapsed().as_secs_f64();
        assert!(out.failure.is_none() && out.positions as usize == n, "the replay succeeds: {:?}", out.failure);
        for p in 0..n {
            assert_eq!(got.per[p], want.per[p], "position {p}: the batched replay differs from the CPU executor");
        }
        let cpu_s: f64 = cpu_ms[..n].iter().sum::<f64>() / 1e3;
        let step_s = step_med * n as f64 / 1e3;
        let st = &r.stats;
        let occ: Vec<String> = st
            .occurrence_ns
            .iter()
            .zip(before.occurrence_ns.iter().chain(std::iter::repeat(&0)))
            .map(|(a, b)| format!("{:.2}", (a - b) as f64 / 1e9))
            .collect();
        breakdown.push(format!("{n} positions: per occurrence (pre, layers…, post) {} s", occ.join(" / ")));
        println!(
            "| {n} | {cpu_s:.1} s | {step_s:.1} s | {secs:.2} s | {:.0}× | {:.0}× | {} ({:.1} a position) | {:.2} / {:.2} / {:.2} s |",
            cpu_s / secs,
            step_s / secs,
            st.dispatches - before.dispatches,
            (st.dispatches - before.dispatches) as f64 / n as f64,
            (st.record_ns - before.record_ns) as f64 / 1e9,
            (st.wait_ns - before.wait_ns) as f64 / 1e9,
            (st.deliver_ns - before.deliver_ns) as f64 / 1e9,
        );
    }
    println!();
    for b in &breakdown {
        println!("{b}");
    }
    println!(
        "every position's committed values and logits equal to the CPU executor's; occurrences {:?}; fallbacks {:?}; sequential {:?}",
        r.modes(),
        r.stats.fallback,
        r.stats.sequential
    );
}
