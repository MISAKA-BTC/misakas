//! **Position-batched replay, position for position against the CPU executor.**
//!
//! [`BatchReplay`] replays a whole job a chunk of positions at a time; `TirExecutor` steps the same
//! tokens one position after another and stops at the first failure, as a seat replaying a job does.
//! Both must agree on how many positions succeed and on the failing position's class, and — at every
//! successful position — on every value the sink is handed (every node's in "every node" mode, the
//! commit points' otherwise; the batched replay's staged-lanes path) and on the logits. Programs: the
//! seven golden program vectors (dense GQA, sliding + global, GDN with 2 key / 4 value heads,
//! Mamba-2, top-2 MoE with a shared expert, Fixed-state saturation, a 3-row history window) at chunk
//! sizes that put chunk boundaries before, inside and after the windows' first slide; the CPU
//! executor's random-program generator (every primitive, windows 1, 2, 3, 5 and `history_bound`,
//! per-layer and global states, range-extreme operands, so that many replays fail and must fail
//! alike); and the Qwen2.5-shaped program at a small geometry, whose projections fold into GEMMs
//! and whose attention is the batched causal product.

mod common;
mod exec_common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use exec_common::progen::{GenCfg, R, gen_program};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, MapParams, Tensor, TirErrorKind};
use misaka_palw_tir_exec::elem::Slice;
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_gpu::batch::Mode;
use misaka_palw_tir_gpu::fixtures::{Geo, map_params, qwen2_program};
use misaka_palw_tir_gpu::{BatchReplay, GpuExecutor, ReplaySink};
use rand::{Rng, SeedableRng};
use serde_json::Value;

use common::device;

type Values = BTreeMap<u32, (bool, DType, Vec<usize>, Vec<i128>)>;

/// A whole replay: every successful position's values and logits, and the class of the failure
/// that ended it.
#[derive(Debug, Default, PartialEq, Eq)]
struct Run {
    positions: Vec<(Values, Vec<i128>)>,
    failure: Option<TirErrorKind>,
}

#[derive(Default)]
struct Collect {
    every: bool,
    values: Values,
}

impl StepSink for Collect {
    fn every_node(&self) -> bool {
        self.every
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        self.values.insert(v.slot, (v.commit, v.dtype, v.shape.to_vec(), v.data.to_i128s()));
    }
}

/// The CPU executor, a position after another, until the first failure.
fn cpu_run(plan: &TirPlan, tp: &TirParams<'_>, tokens: &[u32], every: bool) -> Run {
    let mut ex = TirExecutor::new(plan, tp).expect("every param bound");
    let mut run = Run::default();
    for tok in tokens {
        let mut c = Collect { every, ..Default::default() };
        match ex.step(*tok, &mut c) {
            Ok(()) => run.positions.push((c.values, ex.logits().1.to_i128s())),
            Err(e) => {
                run.failure = Some(e.kind);
                break;
            }
        }
    }
    run
}

struct Sink {
    every: bool,
    run: Run,
}

impl Sink {
    fn at(&mut self, pos: u32) -> &mut (Values, Vec<i128>) {
        let p = pos as usize;
        assert!(p + 1 >= self.run.positions.len(), "positions arrive in order");
        if p == self.run.positions.len() {
            self.run.positions.push(Default::default());
        }
        &mut self.run.positions[p]
    }
}

impl ReplaySink for Sink {
    fn every_node(&self) -> bool {
        self.every
    }
    fn node(&mut self, pos: u32, v: &NodeValue<'_>) {
        let prev = self.at(pos).0.insert(v.slot, (v.commit, v.dtype, v.shape.to_vec(), v.data.to_i128s()));
        assert!(prev.is_none(), "slot {} handed twice at position {pos}", v.slot);
    }
    fn logits(&mut self, pos: u32, _shape: &[usize], data: Slice<'_>) {
        self.at(pos).1 = data.to_i128s();
    }
}

fn gpu_run(r: &mut BatchReplay<'_>, tokens: &[u32], every: bool) -> Run {
    let mut s = Sink { every, run: Run::default() };
    let out = r.replay(tokens, &mut s);
    assert_eq!(s.run.positions.len(), out.positions as usize, "one entry per successful position");
    s.run.failure = out.failure.map(|e| e.kind);
    s.run
}

fn brief(v: Option<&(bool, DType, Vec<usize>, Vec<i128>)>) -> String {
    match v {
        Some((c, d, s, data)) => format!("commit {c} {d:?} {s:?} {:?}…", &data[..data.len().min(8)]),
        None => "absent".into(),
    }
}

/// Panic with the first difference.
fn compare(what: &str, cpu: &Run, gpu: &Run) {
    if cpu == gpu {
        return;
    }
    for (p, ((a, la), (b, lb))) in cpu.positions.iter().zip(&gpu.positions).enumerate() {
        if a != b {
            let slot = a.keys().chain(b.keys()).copied().find(|s| a.get(s) != b.get(s)).expect("some slot differs");
            panic!("{what}: position {p}, slot {slot}: CPU {} vs device {}", brief(a.get(&slot)), brief(b.get(&slot)));
        }
        if la != lb {
            panic!("{what}: position {p}: the logits differ");
        }
    }
    panic!(
        "{what}: the CPU executor ran {} positions then {:?}; the batched replay {} then {:?}",
        cpu.positions.len(),
        cpu.failure,
        gpu.positions.len(),
        gpu.failure
    );
}

#[derive(Default, Debug)]
struct Totals {
    replays: usize,
    refused: usize,
    positions: usize,
    failed: usize,
    batched_occurrences: u64,
    per_position_occurrences: u64,
    device_nodes: u64,
    views: u64,
    fallback: BTreeMap<String, u64>,
    sequential: BTreeMap<String, u64>,
}

impl Totals {
    fn add(&mut self, r: &BatchReplay<'_>, run: &Run) {
        self.replays += 1;
        self.positions += run.positions.len();
        self.failed += run.failure.is_some() as usize;
        let st = &r.stats;
        self.batched_occurrences += st.batched_occurrences;
        self.per_position_occurrences += st.per_position_occurrences;
        self.device_nodes += st.device_nodes;
        self.views += st.views;
        for (k, v) in &st.fallback {
            *self.fallback.entry(k.clone()).or_default() += v;
        }
        for (k, v) in &st.sequential {
            *self.sequential.entry(k.clone()).or_default() += v;
        }
    }
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

#[test]
fn the_golden_program_vectors_replay_batched_as_the_cpu_executor_steps_them() {
    let Some(dev) = device() else { return };
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let mut t = Totals::default();
    for f in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let p: TirProgramV1 =
            TirProgramV1::decode_canonical(&hex(doc["program_borsh_hex"].as_str().unwrap())).expect("a canonical program");
        let mut params = MapParams::default();
        for e in doc["params"].as_array().unwrap() {
            let j = e["param"].as_u64().unwrap() as u16;
            let layer = e["layer"].as_u64().map(|l| l as u16);
            let d = &p.params[j as usize];
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            params.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &hex(e["le_hex"].as_str().unwrap())).unwrap());
        }
        let plan = TirPlan::compile(&p).expect("a vector compiles");
        let tp = TirParams::from_map(&plan, &params).expect("a vector binds");
        let steps = doc["steps"].as_array().unwrap();
        let tokens: Vec<u32> = steps.iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
        let mut modes = Vec::new();
        for every in [true, false] {
            let cpu = cpu_run(&plan, &tp, &tokens, every);
            assert!(cpu.failure.is_none(), "{name}: a vector step fails on the CPU");
            for chunk in [1, 2, 3, 5, tokens.len()] {
                let mut r = BatchReplay::new(&dev, &plan, &tp).expect("bound");
                r.chunk = chunk;
                let gpu = gpu_run(&mut r, &tokens, every);
                compare(&format!("{name} (chunk {chunk}, every node {every})"), &cpu, &gpu);
                assert!(r.stats.sequential.is_empty(), "{name}: replayed by the step executor: {:?}", r.stats.sequential);
                t.add(&r, &gpu);
                modes = r.modes().to_vec();
            }
            // The batched replay's committed values ARE the vector's.
            for (s, (values, logits)) in steps.iter().zip(&cpu.positions) {
                for c in s["commits"].as_array().unwrap() {
                    let slot = c["slot"].as_u64().unwrap() as u32;
                    let want: Vec<i128> =
                        c["value"]["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
                    assert_eq!(values[&slot].3, want, "{name}: slot {slot} ≠ the vector");
                }
                let want: Vec<i128> =
                    s["logits"]["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
                assert_eq!(*logits, want, "{name}: logits ≠ the vector");
            }
        }
        eprintln!("{name}: {} positions equal at chunks 1, 2, 3, 5 and all, both sink modes; occurrences {modes:?}", tokens.len());
    }
    eprintln!("program vectors: {t:?}");
    assert!(t.batched_occurrences > 0 && t.per_position_occurrences > 0, "both occurrence modes ran");
}

fn tokens(rng: &mut R, p: &TirProgramV1, n: usize) -> Vec<u32> {
    (0..n)
        .map(|_| if rng.gen_bool(0.97) || p.token_bound == u32::MAX { rng.gen_range(0..p.token_bound.min(64)) } else { p.token_bound })
        .collect()
}

#[test]
fn random_programs_replay_batched_as_the_cpu_executor_steps_them() {
    let Some(dev) = device() else { return };
    let n: u64 = std::env::var("TIR_GPU_BATCH_PROGRAMS").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    let mut t = Totals::default();
    let mut long = 0usize;
    for seed in (0..n).chain(REGRESSIONS.iter().copied().filter(|s| *s >= n)) {
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| one_random(&dev, seed, &mut t, &mut long)));
        if let Err(e) = run {
            let msg = e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()));
            panic!("seed {seed}: {}", msg.unwrap_or_default());
        }
    }
    eprintln!("random programs: {t:?}; {long} replays spanning more than one chunk; {} pipelines", dev.pipeline_count());
    assert!(t.positions > n as usize, "many positions succeed");
    assert!(t.failed > 0, "some replays fail (range-extreme values, a bad token), and fail alike");
    assert!(t.batched_occurrences > 2 * t.per_position_occurrences, "most occurrences run batched");
    assert!(long > n as usize / 10, "many replays cross chunk boundaries");
}

/// Seeds past the default count that once failed, run every time. 2525: a `TopK` over a transposed
/// `i8` param (a packed, strided view), which the kernel materialised in the packed form it cannot
/// write — `GpuExecutor` alike.
const REGRESSIONS: &[u64] = &[2525];

fn one_random(dev: &misaka_palw_tir_gpu::GpuDevice, seed: u64, t: &mut Totals, long: &mut usize) {
    let mut rng = R::seed_from_u64(seed);
    let g = gen_program(&mut rng, GenCfg::default());
    let steps = rng.gen_range(1..=12);
    let toks = tokens(&mut rng, &g.prog, steps);
    let chunk = [1usize, 2, 3, 4, 7, 64][rng.gen_range(0..6)];
    let every = seed.is_multiple_of(3);
    let Ok(plan) = TirPlan::compile(&g.prog) else {
        t.refused += 1;
        return;
    };
    let Ok(tp) = TirParams::from_map(&plan, &g.params) else {
        t.refused += 1;
        return;
    };
    let cpu = cpu_run(&plan, &tp, &toks, every);
    let mut r = BatchReplay::new(dev, &plan, &tp).expect("bound");
    r.chunk = chunk;
    let gpu = gpu_run(&mut r, &toks, every);
    compare(&format!("seed {seed} (chunk {chunk}, every node {every}, {} tokens)", toks.len()), &cpu, &gpu);
    *long += (cpu.positions.len() > chunk) as usize;
    t.add(&r, &gpu);
}

#[test]
fn a_qwen2_shaped_program_replays_batched_with_folded_projections_and_batched_attention() {
    let Some(dev) = device() else { return };
    let g = Geo { layers: 2, d: 64, heads: 4, kv: 2, hd: 16, ff: 96, vocab: 384 };
    let (program, fills) = qwen2_program(&g);
    let plan = TirPlan::compile(&program).expect("the program validates");
    let tp = TirParams::from_map(&plan, &map_params(&program, &fills, 7)).expect("bound");
    let positions = 40usize;
    let toks: Vec<u32> = (0..positions).map(|i| ((i * 7919 + 13) % g.vocab as usize) as u32).collect();
    // The step executor's dispatches, for scale.
    let mut step = GpuExecutor::new(&dev, &plan, &tp).expect("bound");
    for tok in &toks {
        step.step(*tok, &mut misaka_palw_tir_exec::NoSink).expect("a step");
    }
    for every in [false, true] {
        let cpu = cpu_run(&plan, &tp, &toks, every);
        assert!(cpu.failure.is_none(), "the CPU executor runs every position");
        let distinct: std::collections::BTreeSet<i128> = cpu.positions.iter().flat_map(|(_, l)| l.iter().copied()).collect();
        assert!(distinct.len() > 100, "the logits are not degenerate ({} distinct values)", distinct.len());
        for chunk in [7, 16, positions] {
            let mut r = BatchReplay::new(&dev, &plan, &tp).expect("bound");
            r.chunk = chunk;
            assert!(r.modes().iter().all(|m| *m == Mode::Batched), "every occurrence batches: {:?}", r.modes());
            let gpu = gpu_run(&mut r, &toks, every);
            compare(&format!("qwen2-shaped (chunk {chunk}, every node {every})"), &cpu, &gpu);
            assert!(r.stats.sequential.is_empty() && r.stats.fallback.is_empty(), "{:?}", r.stats);
            eprintln!(
                "qwen2-shaped, {positions} positions in chunks of {chunk} (every node {every}): equal; {} dispatches ({:.1} a position) against the step executor's {:.0} a position",
                r.stats.dispatches,
                r.stats.dispatches as f64 / positions as f64,
                step.stats.dispatches as f64 / positions as f64
            );
            assert!(r.stats.dispatches * 4 < step.stats.dispatches, "batching cuts the dispatches");
        }
    }
}
