//! **Whole programs on the device executor, node for node against the CPU executor.**
//!
//! [`GpuExecutor`] and `TirExecutor` run the same program, the same params and the same tokens;
//! every step must succeed in both or fail in both with the same class, and every value they hand
//! their sinks — every node's in "every node" mode, every commit point's otherwise (the device's
//! staged-lanes path) — and the logits must be equal. Programs: the seven golden program vectors
//! (dense GQA, sliding + global, GDN with 2 key / 4 value heads, Mamba-2, top-2 MoE with a shared
//! expert, Fixed-state saturation, a 3-row history window), whose committed values are also checked
//! against the vectors; and the CPU executor's random-program generator — every primitive,
//! histories of windows 1, 2, 3, 5 and `history_bound`, attention contracting `H`, per-layer and
//! global states and params, range-extreme operands (so that many steps fail, and must fail alike).

mod common;
mod exec_common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use exec_common::progen::{GenCfg, R, gen_program};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir::{DType, MapParams, Tensor, TirErrorKind};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, TirParams, TirPlan};
use misaka_palw_tir_gpu::{GpuDevice, GpuExecutor};
use rand::{Rng, SeedableRng};
use serde_json::Value;

use common::device;

/// Every value a sink was handed, by slot.
#[derive(Default)]
struct Collect {
    every: bool,
    values: BTreeMap<u32, (bool, DType, Vec<usize>, Vec<i128>)>,
}

impl StepSink for Collect {
    fn every_node(&self) -> bool {
        self.every
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        self.values.insert(v.slot, (v.commit, v.dtype, v.shape.to_vec(), v.data.to_i128s()));
    }
}

/// What one step produced.
#[derive(Debug, PartialEq, Eq)]
enum StepOut {
    Ok { values: BTreeMap<u32, (bool, DType, Vec<usize>, Vec<i128>)>, logits: Vec<i128> },
    Err(TirErrorKind),
}

#[derive(Default, Debug)]
struct Totals {
    programs: usize,
    refused: usize,
    steps_ok: usize,
    steps_err: usize,
    values: usize,
    device_nodes: u64,
    views: u64,
    fallback: BTreeMap<String, u64>,
}

/// Run `tokens` through both executors; panic on the first difference.
fn differential(
    dev: &GpuDevice,
    p: &TirProgramV1,
    params: &MapParams,
    tokens: &[u32],
    every: bool,
    what: &str,
    t: &mut Totals,
) -> Vec<StepOut> {
    t.programs += 1;
    let Ok(plan) = TirPlan::compile(p) else {
        t.refused += 1;
        return Vec::new();
    };
    let Ok(tp) = TirParams::from_map(&plan, params) else {
        t.refused += 1;
        return Vec::new();
    };
    let mut cpu = TirExecutor::new(&plan, &tp).expect("every param bound");
    let mut gpu = GpuExecutor::new(dev, &plan, &tp).expect("every param bound");
    let mut outs = Vec::new();
    for (i, tok) in tokens.iter().enumerate() {
        let (mut sc, mut sg) = (Collect { every, ..Default::default() }, Collect { every, ..Default::default() });
        let rc = cpu.step(*tok, &mut sc);
        let rg = gpu.step(*tok, &mut sg);
        let oc = match rc {
            Ok(()) => StepOut::Ok { values: sc.values, logits: cpu.logits().1.to_i128s() },
            Err(e) => StepOut::Err(e.kind),
        };
        let og = match rg {
            Ok(()) => StepOut::Ok { values: sg.values, logits: gpu.logits().1.to_i128s() },
            Err(e) => StepOut::Err(e.kind),
        };
        if oc != og {
            let detail = match (&oc, &og) {
                (StepOut::Ok { values: a, logits: la }, StepOut::Ok { values: b, logits: lb }) => {
                    let first = a.iter().find(|(s, v)| b.get(s) != Some(v)).map(|(s, v)| (s, v, b.get(s)));
                    format!("first differing slot: {first:?}; logits equal: {}", la == lb)
                }
                _ => format!("CPU {:?} vs device {:?}", matches!(oc, StepOut::Ok { .. }), og),
            };
            panic!("{what}: step {i} (token {tok}) differs — {detail}");
        }
        match &oc {
            StepOut::Ok { values, .. } => {
                t.steps_ok += 1;
                t.values += values.len();
            }
            StepOut::Err(_) => t.steps_err += 1,
        }
        outs.push(oc);
        if matches!(outs.last(), Some(StepOut::Err(_))) {
            // A failed step changes nothing; the run continues with the next token in both.
        }
    }
    t.device_nodes += gpu.stats.device_nodes;
    t.views += gpu.stats.views;
    for (k, v) in &gpu.stats.fallback {
        *t.fallback.entry(k.clone()).or_default() += v;
    }
    outs
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

#[test]
fn the_golden_program_vectors_run_on_the_device_node_for_node() {
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
        let steps = doc["steps"].as_array().unwrap();
        let tokens: Vec<u32> = steps.iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
        for every in [true, false] {
            let outs = differential(&dev, &p, &params, &tokens, every, &name, &mut t);
            // The device's committed values ARE the vector's.
            for (s, o) in steps.iter().zip(&outs) {
                let StepOut::Ok { values, logits } = o else { panic!("{name}: a vector step fails") };
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
        eprintln!("{name}: {} positions equal to the CPU executor and the vector, in both sink modes", tokens.len());
    }
    eprintln!("program vectors: {t:?}");
}

fn tokens(rng: &mut R, p: &TirProgramV1, n: usize) -> Vec<u32> {
    (0..n)
        .map(|_| if rng.gen_bool(0.9) || p.token_bound == u32::MAX { rng.gen_range(0..p.token_bound.min(64)) } else { p.token_bound })
        .collect()
}

#[test]
fn random_programs_agree_with_the_cpu_executor_step_for_step() {
    let Some(dev) = device() else { return };
    let n: u64 = std::env::var("TIR_GPU_PROGRAMS").ok().and_then(|s| s.parse().ok()).unwrap_or(400);
    let mut t = Totals::default();
    for seed in 0..n {
        let mut rng = R::seed_from_u64(seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let steps = rng.gen_range(1..=8);
        let toks = tokens(&mut rng, &g.prog, steps);
        differential(&dev, &g.prog, &g.params, &toks, seed % 3 == 0, &format!("seed {seed}"), &mut t);
    }
    eprintln!("random programs: {t:?}; {} pipelines", dev.pipeline_count());
    assert!(t.steps_ok > n as usize, "many steps succeed");
    assert!(t.steps_err > 0, "some steps fail (range-extreme values), in both");
    assert!(t.device_nodes > 0);
}
