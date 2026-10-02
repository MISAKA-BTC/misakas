//! **RFC-0006: a device's cells, value for value against the CPU executor's.**
//!
//! [`GpuCellStepper`] and [`CpuCellStepperV1`] run the same cell — occurrences `[a, b)` of the golden programs over their tokens,
//! each position from the carry-out the other shard committed (here: the whole-schedule CPU run's) — and must agree on everything
//! the node's cell verifier reads: every committed value reaching the sink, the carry-out, the logits row, every `Fixed` lane and
//! every history tile of the cell's own instances after every position; or fail alike. A device refusal (`new_cell` on params it
//! does not hold, a history short of rows) is a correct answer and is counted, never a difference.
//!
//! The cut is every `(a, b)` of a 2-shard and a 3-shard layer partition with `pre` and `post` where the node puts them.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::{MapParams, Tensor};
use misaka_palw_tir_exec::{
    CpuCellStepperV1, NodeValue, StepSink, TirCellStepperV1, TirDeviceV1, TirExecutor, TirParams, TirPlan,
};
use misaka_palw_tir_gpu::GpuDeviceV1;
use serde_json::Value;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

#[derive(Default)]
struct Collect(BTreeMap<u32, (bool, Vec<i128>)>);

impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.insert(v.slot, (v.commit, v.data.to_i128s()));
        }
    }
}

/// Occurrence ranges of an `s`-shard partition of a program with `n` occurrences (`pre`, layers, `post`).
fn cuts(n: usize, s: usize) -> Vec<std::ops::Range<usize>> {
    let layers = n - 2;
    if s > layers.max(1) {
        return vec![0..n];
    }
    let b: Vec<usize> = (0..=s).map(|i| 1 + layers * i / s).collect();
    (0..s).map(|i| (if i == 0 { 0 } else { b[i] })..(if i + 1 == s { n } else { b[i + 1] })).collect()
}

#[test]
fn a_device_cell_is_the_cpu_cell_value_for_value() {
    let device = match GpuDeviceV1::open() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("SKIPPED — no TIR device: {e}");
            return;
        }
    };
    eprintln!("device: {}", device.name());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut cells, mut positions, mut refused, mut tiles) = (0usize, 0usize, 0usize, 0usize);
    for f in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let name = f.file_name().unwrap().to_string_lossy().to_string();
        let p: TirProgramV1 = TirProgramV1::decode_canonical(&hex(doc["program_borsh_hex"].as_str().unwrap())).expect("canonical");
        let mut params = MapParams::default();
        for e in doc["params"].as_array().unwrap() {
            let j = e["param"].as_u64().unwrap() as u16;
            let layer = e["layer"].as_u64().map(|l| l as u16);
            let d = &p.params[j as usize];
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            params.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &hex(e["le_hex"].as_str().unwrap())).unwrap());
        }
        let tokens: Vec<u32> = doc["steps"].as_array().unwrap().iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
        let Ok(plan) = TirPlan::compile(&p) else { continue };
        let Ok(tp) = TirParams::from_map(&plan, &params) else { continue };
        let n_occ = plan.occurrences.len();
        if n_occ < 3 {
            continue;
        }
        for s in [2usize, 3] {
            for occ in cuts(n_occ, s) {
                // The carry-in of a cell [a, b) with a > 0: the carry-out of occurrence a - 1 at each position, from the CPU
                // cell [0, a) run (a prefix executor stepped alongside).
                let mut prefix = (occ.start > 0).then(|| CpuCellStepperV1(TirExecutor::new_cell(&plan, &tp, 0..occ.start).unwrap()));
                let mut cpu = CpuCellStepperV1(TirExecutor::new_cell(&plan, &tp, occ.clone()).unwrap());
                cpu.0.set_hist_tail(2);
                let gpu = device.cell_stepper(&plan, &tp, occ.clone());
                let Ok(mut gpu) = gpu else {
                    refused += 1;
                    continue;
                };
                cells += 1;
                for (a, tok) in tokens.iter().enumerate() {
                    let carry_in = match prefix.as_mut() {
                        Some(pre) => pre.step_cell(*tok, 0..occ.start, &[], &mut Collect::default()).expect("the prefix runs"),
                        None => Vec::new(),
                    };
                    let (mut sc, mut sg) = (Collect::default(), Collect::default());
                    let rc = cpu.step_cell(*tok, occ.clone(), &carry_in, &mut sc);
                    let rg = gpu.step_cell(*tok, occ.clone(), &carry_in, &mut sg);
                    match (rc, rg) {
                        (Ok(cc), Ok(cg)) => {
                            assert_eq!(cc, cg, "{name} {occ:?} position {a}: the carry-out");
                            assert_eq!(sc.0, sg.0, "{name} {occ:?} position {a}: the committed values");
                            if occ.end == n_occ {
                                assert_eq!(cpu.logits_lanes(), gpu.logits_lanes(), "{name} {occ:?} position {a}: the logits row");
                            }
                            for inst in &plan.instances {
                                // The cell's own instances: those some occurrence of the cell reads (Fixed) — lanes after the write.
                                if let StateKind::Fixed { .. } = inst.kind {
                                    let n: usize = inst.shape.iter().product::<usize>().max(1);
                                    let (mut lc, mut lg) = (Vec::new(), Vec::new());
                                    let (ec, eg) = (cpu.fixed_lanes(inst.state, inst.layer, 0, n, &mut lc), gpu.fixed_lanes(inst.state, inst.layer, 0, n, &mut lg));
                                    // An instance no occurrence of the cell touched is initial on both (or absent): compare when both answer.
                                    if ec.is_ok() && eg.is_ok() {
                                        assert_eq!(lc, lg, "{name} {occ:?} position {a}: Fixed {:?}", (inst.state, inst.layer));
                                    }
                                }
                            }
                            // The newest two history rows of every history instance, where both hold them (the window keeps
                            // `window − 1` on a device; the CPU keeps a tail of its own).
                            for inst in &plan.instances {
                                if let StateKind::Hist { .. } = inst.kind {
                                    let n: usize = inst.shape.iter().product::<usize>().max(1);
                                    let (mut hc, mut hg) = (Vec::new(), Vec::new());
                                    let (ec, eg) = (
                                        cpu.hist_tile_lanes(inst.state, inst.layer, 2, 0, n, &mut hc),
                                        gpu.hist_tile_lanes(inst.state, inst.layer, 2, 0, n, &mut hg),
                                    );
                                    if ec.is_ok() && eg.is_ok() {
                                        assert_eq!(hc, hg, "{name} {occ:?} position {a}: history {:?}", (inst.state, inst.layer));
                                        tiles += 1;
                                    }
                                }
                            }
                            positions += 1;
                        }
                        (Err(ec), Err(eg)) => assert_eq!(ec.kind, eg.kind, "{name} {occ:?} position {a}: a failure's class"),
                        (c, g) => panic!("{name} {occ:?} position {a}: CPU {:?} vs device {:?}", c.is_ok(), g.is_ok()),
                    }
                }
            }
        }
    }
    eprintln!("{cells} cells, {positions} positions, {tiles} history tiles equal to the CPU; {refused} refused by the device");
    assert!(cells > 0);
}
