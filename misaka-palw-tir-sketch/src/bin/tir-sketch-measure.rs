//! **`tir-sketch-measure`** — RFC-0007 Part II's measurements: where seat-local algebraic
//! verification pays.
//!
//! ```text
//! cargo run --release -p misaka-palw-tir-sketch --features measure --bin tir-sketch-measure [-- --kernels | --classes]
//! ```
//!
//! Two halves, printed as Markdown:
//!
//! * **kernels** — this machine's rates: the typed backend's `MatMul` GEMV (`i8 [8960, 1536] ·
//!   i16 [1536, 1]`, the Qwen2.5-1.5B MLP shape) on one thread and on every thread; the Freivalds
//!   check's field multiply-add over `P61`; a sketch built by streaming an `i8` weight row by row; a
//!   narrowing's elementwise rate; a streaming read of 512 MiB.
//! * **classes** — real `config.json` files (`misaka-palw-tir-lower/tests/configs/real/`) lowered to
//!   PALW-TIR WITHOUT weights, and the cost model of `misaka_palw_tir_sketch::cost` run on the program:
//!   what a recompute and an algebraic check cost per decode token and per prefill job, the bytes a
//!   seat must obtain, the sketch store against the weights.
//!
//! No weight of any real model is read or written; the kernels run on synthetic buffers of at most
//! 512 MiB.

use std::hint::black_box;
use std::time::Instant;

use misaka_palw_tir::{DType, Prim};
use misaka_palw_tir_exec::kernels::{Opd, Scratch, matmul};
use misaka_palw_tir_exec::layout::Layout;
use misaka_palw_tir_exec::plan::{Acc, NodePlan};
use misaka_palw_tir_exec::{Buf, Slice, TirPlan};
use misaka_palw_tir_sketch::cost::{TirClassCostV1, TirPositionCostV1, tir_class_cost_v1, tir_position_cost_v1};
use misaka_palw_tir_sketch::{TirActActPolicyV1, TirCheckPolicyV1, TirSketchAnalysisV1, TirSketchModulusV1};
use rayon::prelude::*;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

/// This machine's rates (per second).
#[derive(Clone, Copy, Debug)]
struct Rates {
    threads: usize,
    mac_1t: f64,
    mac_all: f64,
    check_1t: f64,
    check_all: f64,
    sketch_1t: f64,
    elem_1t: f64,
    mem_1t: f64,
    mem_all: f64,
}

fn timed(mut f: impl FnMut(), min_secs: f64) -> f64 {
    f();
    let t = Instant::now();
    let mut n = 0u64;
    while t.elapsed().as_secs_f64() < min_secs {
        f();
        n += 1;
    }
    t.elapsed().as_secs_f64() / n as f64
}

fn single<R: Send>(f: impl FnOnce() -> R + Send) -> R {
    rayon::ThreadPoolBuilder::new().num_threads(1).build().expect("a pool").install(f)
}

fn kernels() -> Rates {
    let threads = rayon::current_num_threads();
    let (rows, cols) = (8960usize, 1536usize);
    let mut rng = Rng(0x9E37_79B9);
    let w: Vec<i8> = (0..rows * cols).map(|_| (rng.next() % 255) as i8).collect();
    let x: Vec<i16> = (0..cols).map(|_| (rng.next() % 65_535) as i16).collect();
    // 1. The typed backend's GEMV — what a recompute runs per projection per position.
    let gemv = || {
        let a = Opd { data: Slice::I8(&w), layout: Layout::contiguous(&[rows, cols]) };
        let b = Opd { data: Slice::I16(&x), layout: Layout::contiguous(&[cols, 1]) };
        let node = NodePlan::for_kernel(Prim::MatMul, DType::I64, Acc::Fast64);
        let (mut out, mut scratch) = (Buf::default(), Scratch::default());
        timed(|| matmul::matmul(&node, &a, &b, &[rows, 1], &mut out, &mut scratch).expect("a GEMV"), 1.0)
    };
    let mac_1t = (rows * cols) as f64 / single(gemv);
    let mac_all = (rows * cols) as f64 / gemv();
    // 2. The check's inner loop: LHS over the 8960 outputs, RHS over the 1536 inputs.
    let md = TirSketchModulusV1::P61;
    let y: Vec<i64> = (0..rows).map(|_| (rng.next() % (1 << 36)) as i64 - (1 << 35)).collect();
    let xi: Vec<i64> = x.iter().map(|v| *v as i64).collect();
    let v: Vec<u64> = (0..rows).map(|_| rng.next() & ((1 << 61) - 2)).collect();
    let s: Vec<u64> = (0..cols).map(|_| rng.next() & ((1 << 61) - 2)).collect();
    // The seat knows each operand's proven interval: a 36-bit accumulator, 16-bit codes.
    let check = || black_box(md.add(md.dot_i64_bounded(black_box(&y), &v, 36), md.dot_i64_bounded(black_box(&xi), &s, 16)));
    let terms = (rows + cols) as f64;
    let check_1t = terms / timed(|| _ = check(), 1.0);
    let batch = threads * 8;
    let check_all = terms * batch as f64 / timed(|| _ = (0..batch).into_par_iter().map(|_| check()).reduce(|| 0, |a, b| a ^ b), 1.0);
    // 3. A sketch built by streaming the weight row by row (left weight: S += v[r]·W[r, :]),
    // one i128 lane per column reduced once at the end.
    let sketch = || {
        let mut acc = vec![0i128; cols];
        for (r, row) in w.chunks_exact(cols).enumerate() {
            let vr = v[r] as i128;
            for (a, wv) in acc.iter_mut().zip(row) {
                *a += vr * *wv as i128;
            }
        }
        black_box(acc.iter().map(|a| md.reduce_i128(*a)).collect::<Vec<u64>>())
    };
    let sketch_1t = (rows * cols) as f64 / timed(|| _ = sketch(), 1.0);
    // 4. A narrowing's elementwise work: HAFZ(x·m / 2^s), clamped to codes.
    let xs: Vec<i64> = (0..1 << 20).map(|_| (rng.next() % (1 << 40)) as i64 - (1 << 39)).collect();
    let narrow = || {
        let mut out = vec![0i16; xs.len()];
        for (o, v) in out.iter_mut().zip(&xs) {
            let p = *v as i128 * 777;
            let q = (p.abs() + (1 << 21)) >> 22;
            *o = (q * p.signum()).clamp(-32767, 32767) as i16;
        }
        black_box(out)
    };
    let elem_1t = xs.len() as f64 / timed(|| _ = narrow(), 0.5);
    // 5. A streaming read of 512 MiB.
    let big: Vec<u64> = (0..(64usize << 20)).map(|i| i as u64).collect();
    let bytes = (big.len() * 8) as f64;
    let mem_1t = bytes / timed(|| _ = black_box(big.iter().copied().fold(0u64, u64::wrapping_add)), 1.0);
    let mem_all = bytes
        / timed(
            || {
                _ = black_box(
                    big.par_chunks(1 << 16).map(|c| c.iter().copied().fold(0u64, u64::wrapping_add)).reduce(|| 0, u64::wrapping_add),
                )
            },
            1.0,
        );
    Rates { threads, mac_1t, mac_all, check_1t, check_all, sketch_1t, elem_1t, mem_1t, mem_all }
}

fn print_rates(r: &Rates) {
    println!("## Kernel rates on this machine ({} rayon threads)\n", r.threads);
    println!("| kernel | one thread | all threads |");
    println!("| --- | --- | --- |");
    println!(
        "| typed-backend GEMV `i8 [8960,1536]·i16 [1536,1] → i64` | {:.2} GMAC/s | {:.2} GMAC/s |",
        r.mac_1t / 1e9,
        r.mac_all / 1e9
    );
    println!(
        "| Freivalds check, `P61` multiply-add (`dot_i64`) | {:.2} G terms/s | {:.2} G terms/s |",
        r.check_1t / 1e9,
        r.check_all / 1e9
    );
    println!("| sketch build, streamed `i8` rows | {:.2} G weights/s | — |", r.sketch_1t / 1e9);
    println!("| narrowing (HAFZ + clamp) | {:.2} G elements/s | — |", r.elem_1t / 1e9);
    println!("| streaming read, 512 MiB | {:.1} GB/s | {:.1} GB/s |\n", r.mem_1t / 1e9, r.mem_all / 1e9);
    println!(
        "The time model prices a seat of {SEAT_CORES} cores at the one-thread rates, RAM at the one-thread read rate and an NVMe at {:.0} GB/s.\n",
        NVME / 1e9
    );
}

struct Class {
    name: &'static str,
    config: &'static str,
}

const CLASSES: &[Class] = &[
    Class { name: "Qwen2.5-1.5B", config: "qwen2.5-1.5b-instruct" },
    Class { name: "Qwen3-30B-A3B", config: "qwen3-30b-a3b" },
    Class { name: "Qwen3-Next-80B-A3B", config: "qwen3-next-80b-a3b-instruct" },
    Class { name: "DeepSeek-V3", config: "deepseek-v3-bf16" },
];

fn lowered(c: &Class) -> Result<TirPlan, String> {
    let path = format!("{}/../misaka-palw-tir-lower/tests/configs/real/{}.json", env!("CARGO_MANIFEST_DIR"), c.config);
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?;
    let spec = misaka_palw_tir_lower::parse_config_str(&text).map_err(|e| e.to_string())?;
    let hl = misaka_palw_tir_lower::hl::build_program(&spec).map_err(|e| e.to_string())?;
    let opts = misaka_palw_tir_lower::lower::LowerOpts::default();
    let lw = misaka_palw_tir_lower::lower::lower(&hl, &opts).map_err(|e| e.to_string())?;
    TirPlan::compile(&lw.program).map_err(|e| e.to_string())
}

fn policy(served_from: Option<u32>) -> TirCheckPolicyV1 {
    TirCheckPolicyV1 {
        act_act: match served_from {
            None => TirActActPolicyV1::Recompute,
            Some(0) => TirActActPolicyV1::Served,
            Some(k) => TirActActPolicyV1::ServedFrom { k },
        },
        weight_min_k: 64,
    }
}

fn gib(b: f64) -> String {
    if b >= 1e9 {
        format!("{:.2} GB", b / 1e9)
    } else if b >= 1e6 {
        format!("{:.1} MB", b / 1e6)
    } else {
        format!("{:.1} KB", b / 1e3)
    }
}

fn secs(s: f64) -> String {
    if s >= 1.0 { format!("{s:.2} s") } else { format!("{:.1} ms", s * 1e3) }
}

/// One scenario's seat times.
struct Times {
    /// Recompute, weights in RAM / weights streamed from an NVMe at `NVME`.
    recompute_ram: f64,
    recompute_nvme: f64,
    /// The check's compute, and the link time of the served bytes per link.
    check: f64,
    link: [f64; 3],
}

const NVME: f64 = 3.0e9;
/// The seat the time model prices: this many cores at the one-thread rates measured here (the
/// all-thread rates on a machine at load average ~90 are contention, not capacity), and RAM read at
/// the one-thread streaming rate (a conservative figure for a quiet host).
const SEAT_CORES: f64 = 8.0;
const LINKS: [(&str, f64); 3] = [("1 Gbps", 1.0e9 / 8.0), ("100 Mbps", 1.0e8 / 8.0), ("5 Mbps", 5.0e6 / 8.0)];

fn times(c: &TirPositionCostV1, weight_bytes: f64, r: &Rates) -> Times {
    let (mac, check, elem) = (r.mac_1t * SEAT_CORES, r.check_1t * SEAT_CORES, r.elem_1t * SEAT_CORES);
    // The exact elementwise work (norms, narrowings, softmax, state updates) is the same on both
    // paths: a recompute does it too.
    let shared = c.exact_elements as f64 / elem;
    let macs = (c.recompute_weight_macs + c.recompute_act_macs) as f64;
    let compute = macs / mac;
    let check = (c.check_weight_terms + c.check_fresh_terms) as f64 / check + (c.exact_act_macs + c.exact_weight_macs) as f64 / mac;
    Times {
        recompute_ram: compute.max(weight_bytes / r.mem_1t) + shared,
        recompute_nvme: compute.max(weight_bytes / NVME) + shared,
        check: check + shared,
        link: LINKS.map(|(_, b)| c.served_bytes as f64 / b),
    }
}

fn class_tables(r: &Rates) {
    for c in CLASSES {
        let plan = match lowered(c) {
            Ok(p) => p,
            Err(e) => {
                println!("## {}: not lowered ({e})\n", c.name);
                continue;
            }
        };
        let analysis = TirSketchAnalysisV1::of(&plan.program);
        let pol = policy(Some(1024));
        let k: TirClassCostV1 = tir_class_cost_v1(&plan, &analysis, &pol, 32_768);
        let nodes: usize = plan.program.blocks.iter().map(|b| b.nodes.len()).sum();
        println!(
            "## {} — {} blocks, {} nodes, {} layers\n",
            c.name,
            plan.program.blocks.len(),
            nodes,
            plan.program.schedule.layers.len()
        );
        println!("| class figure | value |");
        println!("| --- | --- |");
        println!("| params (every instance) | {} |", gib(k.param_bytes as f64));
        println!("| held by a sketching seat | {} |", gib(k.held_param_bytes as f64));
        println!("| stood for by sketches | {} |", gib(k.sketched_param_bytes as f64));
        println!(
            "| sketch store (8-byte entries, every modulus) | {} = {:.2} % of the sketched weights |",
            gib(k.sketch_entries as f64 * 8.0),
            100.0 * k.sketch_entries as f64 * 8.0 / k.sketched_weight_bytes.max(1) as f64
        );
        println!("| weight sites (per occurrence) / needing ≥ 2 moduli | {} / {} |", k.weight_sites, k.wide_sites);
        println!("| largest single served `MatMul`'s weight read (the escalation fetch) | {} |", gib(k.max_site_weight_bytes as f64));
        // The committed tiles of a 512-position prefill job at 64 lanes a tile: the N a random leaf
        // audit samples from (RFC-0007 Part IV.1).
        let tiles: u64 =
            (1..=512usize).map(|h| tir_position_cost_v1(&plan, &analysis, &pol, h, h == 512, true).committed_lanes.div_ceil(64)).sum();
        println!(
            "| committed tiles of a 512-position job, 64 lanes a tile (lanes per decode token at H = 1,024) | {tiles} ({}) |\n",
            tir_position_cost_v1(&plan, &analysis, &pol, 1024, true, true).committed_lanes
        );
        // Decode: one token at three context lengths, three policies.
        println!(
            "**Decode, one token** (post run). Policies: R = activation products recomputed; S = served with per-row history sketches; S≥1024 = `P·V` served from H ≥ 1024, `Q·Kᵀ` recomputed.\n"
        );
        println!(
            "| H | policy | recompute MACs | weight bytes read | check terms | exact elems | exact MACs | served bytes (packed) | RAM recompute | NVMe recompute | check compute | link 1 Gbps / 100 Mbps / 5 Mbps |"
        );
        println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |");
        for h in [1024usize, 8192, 32_768] {
            for (label, sf) in [("R", None), ("S", Some(0)), ("S≥1024", Some(1024))] {
                let cost = tir_position_cost_v1(&plan, &analysis, &policy(sf), h, true, true);
                let t = times(&cost, cost.weight_bytes_read as f64, r);
                println!(
                    "| {h} | {label} | {:.2} G | {} | {:.1} M | {:.2} M | {:.1} M | {} ({}) | {} | {} | {} | {} / {} / {} |",
                    (cost.recompute_weight_macs + cost.recompute_act_macs) as f64 / 1e9,
                    gib(cost.weight_bytes_read as f64),
                    (cost.check_weight_terms + cost.check_fresh_terms) as f64 / 1e6,
                    cost.exact_elements as f64 / 1e6,
                    (cost.exact_act_macs + cost.exact_weight_macs) as f64 / 1e6,
                    gib(cost.served_bytes as f64),
                    gib(cost.served_packed_bits as f64 / 8.0),
                    secs(t.recompute_ram),
                    secs(t.recompute_nvme),
                    secs(t.check),
                    secs(t.link[0]),
                    secs(t.link[1]),
                    secs(t.link[2]),
                );
            }
        }
        println!();
        // Prefill: M positions, post only at the last; the weights are read once for the batch.
        println!("**Prefill job of M positions** (policy S≥1024; a batched recompute reads every weight once).\n");
        println!(
            "| M | recompute MACs | check terms | served bytes (packed) | recompute (RAM) | check compute | link 1 Gbps / 100 Mbps / 5 Mbps |"
        );
        println!("| --- | --- | --- | --- | --- | --- | --- |");
        for m in [512usize, 1024] {
            let mut sum = TirPositionCostV1::default();
            for h in 1..=m {
                sum.add(&tir_position_cost_v1(&plan, &analysis, &pol, h, h == m, true));
            }
            let t = times(&sum, k.param_bytes as f64, r);
            println!(
                "| {m} | {:.1} T | {:.2} G | {} ({}) | {} | {} | {} / {} / {} |",
                (sum.recompute_weight_macs + sum.recompute_act_macs) as f64 / 1e12,
                (sum.check_weight_terms + sum.check_fresh_terms) as f64 / 1e9,
                gib(sum.served_bytes as f64),
                gib(sum.served_packed_bits as f64 / 8.0),
                secs(t.recompute_ram),
                secs(t.check),
                secs(t.link[0]),
                secs(t.link[1]),
                secs(t.link[2]),
            );
        }
        println!();
        // The verdict: the link speed above which serving the witness beats recomputing.
        println!(
            "**Break-even link speed** (policy S≥1024): the witness pays when the link carries its bytes in the time a recompute spends beyond the check — `served bytes / (recompute − check)`. Raw = 8-byte accumulators; packed = each value at its proven interval's width.\n"
        );
        println!("| scenario | recompute | check | served raw / packed | break-even raw / packed |");
        println!("| --- | --- | --- | --- | --- |");
        let verdict = |label: &str, rc: f64, ck: f64, raw: f64, packed: f64| {
            let be = |b: f64| if rc > ck { format!("{:.2} Gbps", b * 8.0 / (rc - ck) / 1e9) } else { "never".to_string() };
            println!("| {label} | {} | {} | {} / {} | {} / {} |", secs(rc), secs(ck), gib(raw), gib(packed), be(raw), be(packed));
        };
        for h in [1024usize, 32_768] {
            let cost = tir_position_cost_v1(&plan, &analysis, &pol, h, true, true);
            let t = times(&cost, cost.weight_bytes_read as f64, r);
            let (raw, packed) = (cost.served_bytes as f64, cost.served_packed_bits as f64 / 8.0);
            verdict(&format!("decode token, H = {h}, weights in RAM"), t.recompute_ram, t.check, raw, packed);
            verdict(&format!("decode token, H = {h}, weights on NVMe"), t.recompute_nvme, t.check, raw, packed);
        }
        let mut sum = TirPositionCostV1::default();
        for h in 1..=1024usize {
            sum.add(&tir_position_cost_v1(&plan, &analysis, &pol, h, h == 1024, true));
        }
        let t = times(&sum, k.param_bytes as f64, r);
        let (raw, packed) = (sum.served_bytes as f64, sum.served_packed_bits as f64 / 8.0);
        verdict("prefill job, M = 1024, weights in RAM", t.recompute_ram, t.check, raw, packed);
        verdict("prefill job, M = 1024, weights on NVMe", t.recompute_nvme, t.check, raw, packed);
        println!();
    }
}

/// `--sites NAME`: every `MatMul` of every block of one class, as the analysis sees it (h = 1).
fn sites(name: &str) {
    let Some(c) = CLASSES.iter().find(|c| c.name == name) else { return };
    let plan = lowered(c).expect("lowered");
    let analysis = TirSketchAnalysisV1::of(&plan.program);
    for (bi, b) in plan.program.blocks.iter().enumerate() {
        let runs = plan.occurrences.iter().filter(|(x, _)| *x as usize == bi).count();
        println!("block {bi} `{}` × {runs}: {} nodes", b.name, b.nodes.len());
        for s in &analysis.blocks[bi].matmuls {
            let n = &b.nodes[s.node as usize];
            let np = &plan.blocks[bi].nodes[s.node as usize];
            println!(
                "  node {:>3} {:?} out {} {:?} K {:?} commit {} | a {:?} b {:?}",
                s.node,
                s.kind,
                n.out.dtype.name(),
                n.out.shape,
                np.in_types[0].shape.last(),
                n.commit,
                np.in_types[0].shape,
                np.in_types[1].shape
            );
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--sites") {
        sites(args.get(i + 1).map(String::as_str).unwrap_or("Qwen2.5-1.5B"));
        return;
    }
    let only_kernels = args.iter().any(|a| a == "--kernels");
    let only_classes = args.iter().any(|a| a == "--classes");
    let rates = kernels();
    if !only_classes {
        print_rates(&rates);
    }
    if !only_kernels {
        class_tables(&rates);
    }
}
