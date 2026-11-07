//! **`tir-gpu-bench`: the device's kernels at realistic shapes, against the CPU executor's.**
//!
//! Each kernel runs on synthetic operands of a real model's shape — `i8` weight codes uniform in
//! `[−127, 127]`, `i16` activation codes — under the plan the CPU executor would hold for them
//! (operand intervals `[−127, 127]` × `[−32767, 32767]`, `Acc::Fast64`), and the device's result is
//! compared with the CPU executor's kernel BYTE FOR BYTE before either is timed. Timings: the
//! device's per dispatch over a batch of dispatches in one submission (weights resident, activation
//! resident: the steady state of a node), the CPU kernel's per call on the rayon pool this process
//! was given (`RAYON_NUM_THREADS`; the default is every core).
//!
//! ```text
//! cargo run --release --bin tir-gpu-bench               # every section
//! cargo run --release --bin tir-gpu-bench -- --quick    # fewer iterations
//! ```

use std::time::Instant;

use misaka_palw_tir::interval::Interval;
use misaka_palw_tir::{DType, Prim, TensorType};
use misaka_palw_tir_exec::elem::{Buf, Slice};
use misaka_palw_tir_exec::kernels::{Opd, Scratch, matmul};
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_exec::plan::{Acc, NodePlan};
use misaka_palw_tir_gpu::{DevTensor, Form, GpuDevice, Recorder};

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn w8(&mut self, n: usize) -> Vec<i8> {
        (0..n).map(|_| (((self.next() >> 32) % 255) as i64 - 127) as i8).collect()
    }
    fn codes(&mut self, n: usize) -> Vec<i16> {
        (0..n).map(|_| ((self.next() >> 33) % 65535) as i64 - 32767).map(|v| v as i16).collect()
    }
}

/// The MatMul plan a node holds for `i8` weights against `i16` codes, into `i64`.
fn mm_plan() -> NodePlan {
    let mut p = NodePlan::for_kernel(Prim::MatMul, DType::I64, Acc::Fast64);
    p.in_ivs = vec![Interval::new(-127, 127), Interval::new(-32767, 32767)];
    p.in_types = vec![TensorType::scalar(DType::I8), TensorType::scalar(DType::I16)];
    p
}

/// `(device ms per dispatch, CPU ms per call)` of `W[rows, k] · X[k, cols]`, after checking the
/// two equal.
fn matmul_case(dev: &GpuDevice, rows: usize, k: usize, cols: usize, iters: usize) -> (f64, f64) {
    let mut rng = Rng(0x5eed ^ (rows * 31 + k * 7 + cols) as u64);
    let w = rng.w8(rows * k);
    let x = rng.codes(k * cols);
    let node = mm_plan();
    // CPU: the executor's kernel.
    let a = Opd { data: Slice::I8(&w), layout: Layout::contiguous(&[rows, k]) };
    let b = Opd { data: Slice::I16(&x), layout: Layout::contiguous(&[k, cols]) };
    let mut out = Buf::default();
    let mut scratch = Scratch::default();
    matmul::matmul(&node, &a, &b, &[rows, cols], &mut out, &mut scratch).expect("the CPU kernel");
    let cpu_iters = iters.clamp(3, 50);
    let t = Instant::now();
    for _ in 0..cpu_iters {
        matmul::matmul(&node, &a, &b, &[rows, cols], &mut out, &mut scratch).unwrap();
    }
    let cpu = t.elapsed().as_secs_f64() * 1e3 / cpu_iters as f64;
    // Device: weights packed, codes as lanes (a computed i16 node's form).
    let wd = dev.upload(Slice::I8(&w), Form::P8, &[rows, k]);
    let xd = dev.upload(Slice::I16(&x), Form::S32, &[k, cols]);
    let mut rec = Recorder::new(dev, 1);
    let o = rec.node(&node, &[&wd, &xd], &[rows, cols], 0).expect("on the device");
    let kernel = rec.log.last().cloned().unwrap_or_default();
    assert_eq!(rec.finish()[0], 0);
    assert_eq!(dev.download(&o), out, "W[{rows},{k}]·X[{k},{cols}]: device ≠ CPU executor");
    let mut rec = Recorder::new(dev, 1);
    for _ in 0..iters {
        rec.node(&node, &[&wd, &xd], &[rows, cols], 0).unwrap();
    }
    let t = Instant::now();
    rec.finish();
    let gpu = t.elapsed().as_secs_f64() * 1e3 / iters as f64;
    let macs = (rows * k * cols) as f64;
    println!(
        "  [{rows:>6}, {k:>5}] · [{k:>5}, {cols:>4}]  {kernel:<11} device {gpu:8.3} ms ({:6.1} GB/s weights, {:6.1} GMAC/s) | CPU {cpu:8.3} ms ({:6.1} GMAC/s) | ×{:.1}  equal",
        (rows * k) as f64 / gpu / 1e6,
        macs / gpu / 1e6,
        macs / cpu / 1e6,
        cpu / gpu
    );
    (gpu, cpu)
}

/// Attention-shaped contractions of one position: scores `q[kv, g, d] · Kᵀ[kv, d, H]` and values
/// `P[kv, g, H] · V[kv, H, d]`, the history read through a transposed view as the program does.
fn attention_case(dev: &GpuDevice, kv: usize, g: usize, d: usize, h: usize, iters: usize) {
    let mut rng = Rng(77 + h as u64);
    let q = rng.codes(kv * g * d);
    let khist = rng.codes(h * kv * d);
    let probs: Vec<i32> = (0..kv * g * h).map(|_| (rng.next() % (1 << 24)) as i32).collect();
    let mut sc = mm_plan();
    sc.in_ivs = vec![Interval::new(-32767, 32767), Interval::new(-32767, 32767)];
    let mut pv = mm_plan();
    pv.in_ivs = vec![Interval::new(0, 1 << 25), Interval::new(-32767, 32767)];
    // K history [H, kv·d] viewed as [kv, d, H]; V history viewed as [kv, H, d].
    let kt = Layout::contiguous(&[h, kv, d]).transposed(&[1, 2, 0]);
    let vt = Layout::contiguous(&[h, kv, d]).transposed(&[1, 0, 2]);
    let mut scratch = Scratch::default();
    let (mut s_cpu, mut v_cpu) = (Buf::default(), Buf::default());
    let qa = Opd { data: Slice::I16(&q), layout: Layout::contiguous(&[kv, g, d]) };
    let ka = Opd { data: Slice::I16(&khist), layout: kt };
    let pa = Opd { data: Slice::I32(&probs), layout: Layout::contiguous(&[kv, g, h]) };
    let va = Opd { data: Slice::I16(&khist), layout: vt };
    let t = Instant::now();
    for _ in 0..iters.clamp(3, 50) {
        matmul::matmul(&sc, &qa, &ka, &[kv, g, h], &mut s_cpu, &mut scratch).unwrap();
        matmul::matmul(&pv, &pa, &va, &[kv, g, d], &mut v_cpu, &mut scratch).unwrap();
    }
    let cpu = t.elapsed().as_secs_f64() * 1e3 / iters.clamp(3, 50) as f64;
    let qd = dev.upload(Slice::I16(&q), Form::S32, &[kv, g, d]);
    let kd = dev.upload(Slice::I16(&khist), Form::S32, &[h, kv, d]);
    let pd = dev.upload(Slice::I32(&probs), Form::S32, &[kv, g, h]);
    let (kdv, vdv) = (kd.view(kt), kd.view(vt));
    let mut rec = Recorder::new(dev, 1);
    let so = rec.node(&sc, &[&qd, &kdv], &[kv, g, h], 0).unwrap();
    let vo = rec.node(&pv, &[&pd, &vdv], &[kv, g, d], 0).unwrap();
    let kernels = rec.log.join("+");
    assert_eq!(rec.finish()[0], 0);
    assert_eq!(dev.download(&so), s_cpu, "attention scores: device ≠ CPU");
    assert_eq!(dev.download(&vo), v_cpu, "attention values: device ≠ CPU");
    let mut rec = Recorder::new(dev, 1);
    for _ in 0..iters {
        rec.node(&sc, &[&qd, &kdv], &[kv, g, h], 0).unwrap();
        rec.node(&pv, &[&pd, &vdv], &[kv, g, d], 0).unwrap();
    }
    let t = Instant::now();
    rec.finish();
    let gpu = t.elapsed().as_secs_f64() * 1e3 / iters as f64;
    println!("  H {h:>6}: scores + values ({kernels}) device {gpu:7.3} ms | CPU {cpu:7.3} ms | ×{:.1}  equal", cpu / gpu);
}

/// One elementwise node over `n` elements: device ns per element (and its kernel), CPU ns per element.
fn elementwise_case(dev: &GpuDevice, name: &str, node: &NodePlan, ins: &[(Buf, Form)], n: usize, iters: usize) {
    use misaka_palw_tir_exec::kernels::elementwise;
    let opds: Vec<Opd<'_>> = ins.iter().map(|(b, _)| Opd { data: b.slice(), layout: Layout::contiguous(&[b.len()]) }).collect();
    let mut out = Buf::default();
    let mut scratch = Scratch::default();
    let run_cpu = |out: &mut Buf, scratch: &mut Scratch| match opds.len() {
        1 => elementwise::unary(node, &opds[0], out, scratch, &[]).unwrap(),
        _ => elementwise::binary(node, &opds[0], &opds[1], &[n], out, scratch).unwrap(),
    };
    run_cpu(&mut out, &mut scratch);
    let t = Instant::now();
    for _ in 0..iters {
        run_cpu(&mut out, &mut scratch);
    }
    let cpu = t.elapsed().as_secs_f64() * 1e9 / (iters * n) as f64;
    let devs: Vec<DevTensor> = ins.iter().map(|(b, f)| dev.upload(b.slice(), *f, &[b.len()])).collect();
    let refs: Vec<&DevTensor> = devs.iter().collect();
    let mut rec = Recorder::new(dev, 1);
    let o = rec.node(node, &refs, &[n], 0).expect("on the device");
    assert_eq!(rec.finish()[0], 0);
    assert_eq!(dev.download(&o), out, "{name}: device ≠ CPU");
    let mut rec = Recorder::new(dev, 1);
    for _ in 0..iters {
        rec.node(node, &refs, &[n], 0).unwrap();
    }
    let t = Instant::now();
    rec.finish();
    let gpu = t.elapsed().as_secs_f64() * 1e9 / (iters * n) as f64;
    println!("  {name:<34} n {n:>7}: device {gpu:6.3} ns/element | CPU {cpu:6.3} ns/element | ×{:.1}  equal", cpu / gpu);
}

/// **One 1.5B layer's seven projections over a batch of positions** — the shape a layer shard's
/// verification has (RFC-0006 §1.4) — against the same projections one position at a time, both on
/// the device; each batched result byte-checked against the CPU executor's kernel first.
fn layer_batch(dev: &GpuDevice, positions: usize, iters: usize) {
    let shapes: [(&str, usize, usize); 7] = [
        ("q", 1536, 1536),
        ("k", 256, 1536),
        ("v", 256, 1536),
        ("o", 1536, 1536),
        ("gate", 8960, 1536),
        ("up", 8960, 1536),
        ("down", 1536, 8960),
    ];
    let node = mm_plan();
    let (mut batched, mut single) = (0.0f64, 0.0f64);
    for (name, rows, k) in shapes {
        let mut rng = Rng(0xba7c ^ (rows * 31 + k) as u64);
        let w = rng.w8(rows * k);
        let x = rng.codes(k * positions);
        let wd = dev.upload(Slice::I8(&w), Form::P8, &[rows, k]);
        let xd = dev.upload(Slice::I16(&x), Form::S32, &[k, positions]);
        // The batched product, checked against the CPU kernel.
        let a = Opd { data: Slice::I8(&w), layout: Layout::contiguous(&[rows, k]) };
        let b = Opd { data: Slice::I16(&x), layout: Layout::contiguous(&[k, positions]) };
        let mut out = Buf::default();
        matmul::matmul(&node, &a, &b, &[rows, positions], &mut out, &mut Scratch::default()).expect("the CPU kernel");
        let mut rec = Recorder::new(dev, 1);
        let o = rec.node(&node, &[&wd, &xd], &[rows, positions], 0).expect("on the device");
        let kernel = rec.log.last().cloned().unwrap_or_default();
        assert_eq!(rec.finish()[0], 0);
        assert_eq!(dev.download(&o), out, "{name}: device ≠ CPU executor");
        let mut rec = Recorder::new(dev, 1);
        for _ in 0..iters {
            rec.node(&node, &[&wd, &xd], &[rows, positions], 0).unwrap();
        }
        let t = Instant::now();
        rec.finish();
        let tb = t.elapsed().as_secs_f64() * 1e3 / iters as f64;
        // One position: the same weights against one column.
        let x1 = dev.upload(Slice::I16(&x[..k]), Form::S32, &[k, 1]);
        let mut rec = Recorder::new(dev, 1);
        for _ in 0..iters {
            rec.node(&node, &[&wd, &x1], &[rows, 1], 0).unwrap();
        }
        let t = Instant::now();
        rec.finish();
        let ts = t.elapsed().as_secs_f64() * 1e3 / iters as f64;
        println!(
            "  {name:>4} [{rows:>5}, {k:>5}]: {positions} positions batched ({kernel}) {tb:7.3} ms = {:6.4} ms a position | one position {ts:6.3} ms",
            tb / positions as f64
        );
        batched += tb;
        single += ts;
    }
    println!(
        "  → one layer, {positions} positions: batched {:.2} ms ({:.4} ms a position) vs {:.3} ms a position one at a time: ×{:.1}",
        batched,
        batched / positions as f64,
        single,
        single * positions as f64 / batched
    );
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let quick = args.iter().any(|a| a == "--quick");
    let it = |n: usize| if quick { n.div_ceil(5).max(3) } else { n };
    let dev = GpuDevice::new().expect("a TIR device");
    println!("device: {}; CPU pool: {} threads", dev.describe(), rayon::current_num_threads());
    if let Some(p) = args.iter().position(|a| a == "--layer-batch").and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()) {
        println!("\nOne Qwen2.5-1.5B layer's projections over a batch of positions (RFC-0006 §8)");
        layer_batch(&dev, p, it(20));
        return;
    }

    println!("\nDecode projections, Qwen2.5-1.5B (d 1536, kv 256, ff 8960, vocab 151,936): W·x, one position");
    let mut layer = (0.0, 0.0);
    for (rows, k, n_in_layer) in [(1536usize, 1536usize, 2usize), (256, 1536, 2), (8960, 1536, 2), (1536, 8960, 1)] {
        let (g, c) = matmul_case(&dev, rows, k, 1, it(200));
        layer.0 += g * n_in_layer as f64;
        layer.1 += c * n_in_layer as f64;
    }
    println!("  → the seven projections of one layer: device {:.3} ms | CPU {:.3} ms | ×{:.1}", layer.0, layer.1, layer.1 / layer.0);
    matmul_case(&dev, 151_936, 1536, 1, it(20));

    println!("\nAn MoE expert, Qwen3-30B-A3B (d 2048, expert ff 768): gate/up and down, one position");
    matmul_case(&dev, 768, 2048, 1, it(300));
    matmul_case(&dev, 2048, 768, 1, it(300));

    println!("\nA batch of positions (prefill, or a seat replaying a committed job): W·X");
    for p in [16usize, 64, 256] {
        matmul_case(&dev, 8960, 1536, p, it(20));
    }
    for p in [64usize, 256] {
        matmul_case(&dev, 1536, 8960, p, it(20));
    }
    matmul_case(&dev, 768, 2048, 256, it(40));

    println!("\nAttention of one position over its history (kv 2, groups 6, head 128)");
    for h in [512usize, 4096, 32_768] {
        attention_case(&dev, 2, 6, 128, h, it(50));
    }

    println!("\nElementwise sites of a narrowing and a softmax");
    let n = 151_936usize;
    let mut rng = Rng(9);
    let acc: Vec<i64> = (0..n).map(|_| (rng.next() >> 28) as i64 - (1 << 35)).collect();
    let mults: Vec<i64> = (0..n).map(|_| 256 + (rng.next() % 256) as i64).collect();
    let mut mul = NodePlan::for_kernel(Prim::Mul, DType::I64, Acc::Fast64);
    mul.out = TensorType::fixed(DType::I64, &[n as u32]);
    elementwise_case(&dev, "Mul i64·i64 (x·m)", &mul, &[(Buf::I64(acc.clone()), Form::I64), (Buf::I64(mults), Form::I64)], n, it(200));
    let mut div = NodePlan::for_kernel(Prim::Div { rule: misaka_palw_tir::Rounding::HalfAwayFromZero }, DType::I64, Acc::Fast64);
    div.out = TensorType::fixed(DType::I64, &[n as u32]);
    let shifts: Vec<i64> = (0..n).map(|i| 1i64 << (12 + i % 8)).collect();
    elementwise_case(
        &dev,
        "Div HAFZ by 2^s per channel",
        &div,
        &[(Buf::I64(acc.clone()), Form::I64), (Buf::I64(shifts), Form::I64)],
        n,
        it(200),
    );
    let odd: Vec<i64> = (0..n).map(|i| 1535 + 2 * (i % 7) as i64).collect();
    elementwise_case(
        &dev,
        "Div Floor by an odd divisor",
        &div,
        &[(Buf::I64(acc.clone()), Form::I64), (Buf::I64(odd), Form::I64)],
        n,
        it(50),
    );
    let mut clamp = NodePlan::for_kernel(Prim::Clamp { lo: -32767, hi: 32767 }, DType::I16, Acc::Fast64);
    clamp.out = TensorType::fixed(DType::I16, &[n as u32]);
    elementwise_case(&dev, "Clamp to codes (i64 → i16)", &clamp, &[(Buf::I64(acc.clone()), Form::I64)], n, it(200));
    let mut exp = NodePlan::for_kernel(Prim::IntExp, DType::I32, Acc::Fast64);
    exp.out = TensorType::fixed(DType::I32, &[n as u32]);
    let logits: Vec<i64> = (0..n).map(|_| -((rng.next() % (31 * 11_629_080)) as i64)).collect();
    elementwise_case(&dev, "IntExp (a softmax's exponents)", &exp, &[(Buf::I64(logits), Form::I64)], n, it(100));
    let mut rs = NodePlan::for_kernel(Prim::IntRsqrt, DType::I64, Acc::Fast64);
    rs.out = TensorType::fixed(DType::I64, &[n as u32]);
    let pos: Vec<i64> = (0..n).map(|_| (rng.next() >> 20) as i64).collect();
    elementwise_case(&dev, "IntRsqrt", &rs, &[(Buf::I64(pos), Form::I64)], n, it(100));
    println!("\nDispatch cost: a 16-element Copy, many dispatches in one submission");
    {
        let x = dev.upload(Slice::I32(&[7i32; 16]), Form::S32, &[16]);
        let mut cp = NodePlan::for_kernel(Prim::Cast, DType::I32, Acc::Fast64);
        cp.out = TensorType::fixed(DType::I32, &[16]);
        for n in [1usize, 100, 1000] {
            let mut rec = Recorder::new(&dev, 1);
            for _ in 0..n {
                rec.node(&cp, &[&x], &[16], 0).unwrap();
            }
            let t = Instant::now();
            rec.finish();
            let total = t.elapsed().as_secs_f64() * 1e3;
            println!(
                "  {n:>5} dispatches: {total:8.3} ms in all, {:7.1} µs each (submission and wait included)",
                total * 1e3 / n as f64
            );
        }
        let mut rec = Recorder::new(&dev, 1);
        let t = Instant::now();
        for _ in 0..1000 {
            rec.node(&cp, &[&x], &[16], 0).unwrap();
        }
        println!(
            "  recording 1000 dispatches (params, bind groups, outputs) on the host: {:.1} µs each",
            t.elapsed().as_secs_f64() * 1e6 / 1000.0
        );
        rec.finish();
    }
    println!("\npipelines compiled: {}", dev.pipeline_count());
}
