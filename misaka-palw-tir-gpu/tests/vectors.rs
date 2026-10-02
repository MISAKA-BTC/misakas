//! **The golden primitive vectors (`consensus-vectors/tir-v1/primitives/`, spec 04b §12) on the
//! device.** Every case of every stateless primitive that is a program node (a type error is
//! refused by `validate` before any backend runs; params are never `i128`) is run through the
//! reference, the CPU executor and the device under the CPU executor's refined plan; the device
//! must reproduce the vector's `expect` byte for byte, or fail with its `expect_error` class.

mod common;

use std::path::PathBuf;

use misaka_palw_tir::{DType, Dim, Prim, Tensor, TensorType};
use serde_json::Value;

use common::device;
use common::one_node::{NodeCase, Outcome, Tally, run_case};

fn vectors_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/primitives")
}

fn hex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

fn tensor(v: &Value) -> Tensor {
    let dtype = DType::from_name(v["dtype"].as_str().unwrap()).unwrap();
    let shape: Vec<usize> = v["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect();
    let data: Vec<i128> = v["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
    Tensor { dtype, shape, data }
}

#[test]
fn the_device_reproduces_every_golden_primitive_vector_that_is_a_program_node() {
    let Some(dev) = device() else { return };
    let mut files: Vec<_> = std::fs::read_dir(vectors_dir()).expect("the vectors").map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 23, "one file per stateless primitive (tags 0–22)");
    let mut tally = Tally::default();
    let mut cases = 0usize;
    for f in &files {
        let doc: Value = serde_json::from_str(&std::fs::read_to_string(f).unwrap()).unwrap();
        for c in doc["cases"].as_array().unwrap() {
            cases += 1;
            let name = format!("{}/{}", f.file_name().unwrap().to_string_lossy(), c["name"].as_str().unwrap());
            let prim: Prim = borsh::from_slice(&hex(c["prim"]["borsh_hex"].as_str().unwrap())).expect("a Prim");
            let inputs: Vec<Tensor> = c["inputs"].as_array().unwrap().iter().map(tensor).collect();
            let out_dtype = DType::from_name(c["out"]["dtype"].as_str().unwrap()).unwrap();
            let out_shape: Vec<Dim> =
                c["out"]["shape"].as_array().unwrap().iter().map(|d| Dim::Fixed(d.as_u64().unwrap() as u32)).collect();
            let case = NodeCase { prim, inputs, out: TensorType::new(out_dtype, out_shape) };
            let report = run_case(&dev, &case);
            tally.add(&report);
            let Some(r) = report else { continue };
            r.assert_agree(&name);
            let want = if let Some(e) = c.get("expect") {
                Outcome::Value(tensor(e).data)
            } else {
                let class = c["expect_error"].as_str().unwrap();
                Outcome::Error(match class {
                    "Overflow" => misaka_palw_tir::TirErrorKind::Overflow,
                    "Index" => misaka_palw_tir::TirErrorKind::Index,
                    "Divisor" => misaka_palw_tir::TirErrorKind::Divisor,
                    "Operand" => misaka_palw_tir::TirErrorKind::Operand,
                    other => panic!("{name}: a program node cannot fail with {other}"),
                })
            };
            assert_eq!(r.reference, want, "{name}: the reference evaluator ≠ the vector");
            if let Ok(g) = &r.gpu {
                assert_eq!(*g, want, "{name}: the DEVICE ≠ the vector");
            }
        }
    }
    eprintln!("golden primitive vectors: {cases} cases; {tally:?}");
    assert!(tally.on_device > 0);
}
