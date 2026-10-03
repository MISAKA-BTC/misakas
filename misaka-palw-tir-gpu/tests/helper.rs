//! **The real helper process on the real device** (Metal here): the node's client spawns `palw-tir-gpu-helper`, mirrored — every
//! answer checked against the CPU executor's — and runs the golden programs' cells through it, equal to the CPU stepper's value for
//! value; a helper that cannot open a device is a refusal, never a different answer.

mod common;

use std::collections::BTreeMap;
use std::path::PathBuf;

use misaka_palw_tir::{MapParams, Tensor};
use misaka_palw_tir::program::TirProgramV1;
use misaka_palw_tir_exec::{CpuDeviceV1, NodeValue, StepSink, TirCellStepperV1, TirDeviceV1, TirParams, TirPlan, TirProcessDeviceV1};
use serde_json::Value;

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

#[derive(Default)]
struct Collect(BTreeMap<u32, Vec<i128>>);
impl StepSink for Collect {
    fn node(&mut self, v: &NodeValue<'_>) {
        if v.commit {
            self.0.insert(v.slot, v.data.to_i128s());
        }
    }
}

#[test]
fn the_gpu_helper_process_serves_cells_equal_to_the_cpu_under_the_mirror() {
    if common::device().is_none() {
        return;
    }
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_palw-tir-gpu-helper"));
    let device = TirProcessDeviceV1::spawn(&helper, &[], true).expect("the helper starts and says hello");
    eprintln!("device: {}", device.name());
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    let (mut cells, mut positions) = (0, 0);
    for f in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        let p = TirProgramV1::decode_canonical(&hex(doc["program_borsh_hex"].as_str().unwrap())).unwrap();
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
        let n = plan.occurrences.len();
        if n < 3 {
            continue;
        }
        let cut = 1 + (n - 2) / 2;
        for occ in [0..cut, cut..n] {
            let mut cpu = CpuDeviceV1.cell_stepper(&plan, &tp, occ.clone()).unwrap();
            let mut prefix = (occ.start > 0).then(|| CpuDeviceV1.cell_stepper(&plan, &tp, 0..occ.start).unwrap());
            let Ok(mut dev) = device.cell_stepper(&plan, &tp, occ.clone()) else { panic!("the helper refuses {occ:?}") };
            cells += 1;
            for tok in &tokens {
                let carry_in = match prefix.as_mut() {
                    Some(pre) => pre.step_cell(*tok, 0..occ.start, &[], &mut Collect::default()).unwrap(),
                    None => Vec::new(),
                };
                let (mut sc, mut sd) = (Collect::default(), Collect::default());
                let rc = cpu.step_cell(*tok, occ.clone(), &carry_in, &mut sc);
                let rd = dev.step_cell(*tok, occ.clone(), &carry_in, &mut sd);
                match (rc, rd) {
                    (Ok(a), Ok(b)) => {
                        assert_eq!(a, b);
                        assert_eq!(sc.0, sd.0);
                        positions += 1;
                    }
                    (Err(a), Err(b)) => {
                        let _ = (a, b); // both fail (the mirror reports its own difference as an error too: checked below)
                    }
                    (a, b) => panic!("CPU {:?} vs helper {:?}", a.is_ok(), b.is_ok()),
                }
            }
        }
    }
    assert!(!device.poisoned(), "no mirror difference on any step");
    eprintln!("{cells} cells, {positions} positions through the helper process");
    assert!(cells > 0);
}

#[test]
fn a_helper_that_cannot_start_is_an_error_the_node_falls_back_from() {
    assert!(TirProcessDeviceV1::spawn(std::path::Path::new("/nonexistent/palw-tir-gpu-helper"), &[], true).is_err());
}
