//! PALW-TIR-35: every golden vector of `consensus-vectors/tir-v1/` (04b §12), reproduced by this
//! implementation byte for byte. The vectors are the first implementation's outputs, so a mismatch
//! here is a finding to adjudicate against the text (ref2-findings.md), not automatically a bug.
//!
//! Revision 2 of 04b (§9.3): every rule names the class of its refusal and every vector breaks one
//! rule, so a vector's `expect_error` class is required exactly, as are the bytes of every success.

mod common;

use std::collections::BTreeMap;

use common::*;
use misaka_palw_tir_ref2::codec::{decode_canonical, decode_prim, encode, encode_prim};
use misaka_palw_tir_ref2::eval::{ConeEnv, Params, eval_cone, run};
use misaka_palw_tir_ref2::{Class, Cmp, DType, Prim, Rounding, Tensor, eval_primitive};
use serde_json::Value;

fn attrs_of(p: &Prim) -> Vec<(String, String)> {
    let s = |k: &str, v: String| (k.to_string(), v);
    match p {
        Prim::Transpose { perm } => vec![s("perm", format!("{:?}", perm))],
        Prim::Slice { axis, start } => vec![s("axis", axis.to_string()), s("start", start.to_string())],
        Prim::Concat { axis } | Prim::ReduceSum { axis } | Prim::ReduceMax { axis } => vec![s("axis", axis.to_string())],
        Prim::Iota { axis, start, step } => {
            vec![s("axis", axis.to_string()), s("start", start.to_string()), s("step", step.to_string())]
        }
        Prim::Gather { axis, batch_dims } => vec![s("axis", axis.to_string()), s("batch_dims", batch_dims.to_string())],
        Prim::Div { rule } => vec![s(
            "rule",
            match rule {
                Rounding::Floor => "floor",
                Rounding::HalfUp => "half_up",
                Rounding::HalfAwayFromZero => "half_away_from_zero",
            }
            .to_string(),
        )],
        Prim::Clamp { lo, hi } => vec![s("lo", lo.to_string()), s("hi", hi.to_string())],
        Prim::Compare { cmp } => vec![s(
            "cmp",
            match cmp {
                Cmp::Eq => "eq",
                Cmp::Ne => "ne",
                Cmp::Lt => "lt",
                Cmp::Le => "le",
                Cmp::Gt => "gt",
                Cmp::Ge => "ge",
            }
            .to_string(),
        )],
        Prim::TopK { axis, k } => vec![s("axis", axis.to_string()), s("k", k.to_string())],
        Prim::StateWrite { state } | Prim::HistAppend { state } => vec![s("state", state.to_string())],
        _ => vec![],
    }
}

#[derive(Default)]
struct Tally {
    total: usize,
    reproduced: usize,
    class_disagreements: Vec<String>,
    failures: Vec<String>,
}

fn run_primitive_file(path: &std::path::Path, t: &mut Tally) {
    let d = read_json(path);
    assert_eq!(d["format"], "palw-tir-v1/primitive-vectors/1");
    let tag = int_of(&d["tag"]) as u8;
    for case in d["cases"].as_array().unwrap() {
        t.total += 1;
        let name = format!("{}::{}", path.file_name().unwrap().to_string_lossy(), case["name"].as_str().unwrap());
        // The Prim, from its canonical bytes; the name, tag and attributes must agree with them.
        let bytes = hex_decode(case["prim"]["borsh_hex"].as_str().unwrap());
        let prim = match decode_prim(&bytes) {
            Ok(p) => p,
            Err(e) => {
                t.failures.push(format!("{name}: borsh_hex does not decode: {e}"));
                continue;
            }
        };
        assert_eq!(encode_prim(&prim), bytes, "{name}: re-encoding");
        assert_eq!(prim.tag(), tag, "{name}: tag");
        assert_eq!(prim.name(), case["prim"]["name"].as_str().unwrap(), "{name}: name");
        let attrs: Vec<(String, String)> = case["prim"]["attrs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kv| (kv[0].as_str().unwrap().to_string(), kv[1].as_str().unwrap().to_string()))
            .collect();
        assert_eq!(attrs_of(&prim), attrs, "{name}: attrs");
        let ins: Vec<Tensor> = case["inputs"].as_array().unwrap().iter().map(tensor_of).collect();
        let out_dtype = dtype_of(&case["out"]["dtype"]);
        let out_shape = shape_of(&case["out"]["shape"]);
        let got = eval_primitive(&prim, &ins, out_dtype, &out_shape);
        match (&case.get("expect"), &case.get("expect_error"), got) {
            (Some(exp), _, Ok(v)) => {
                if tensor_of(exp) == v {
                    t.reproduced += 1;
                } else {
                    t.failures.push(format!("{name}: expected {} got {}", exp, tensor_json(&v)));
                }
            }
            (Some(exp), _, Err(e)) => t.failures.push(format!("{name}: expected {exp}, got error {e}")),
            (None, Some(cls), Err(e)) => {
                t.reproduced += 1;
                if cls.as_str() != Some(e.class.name()) {
                    // Revision 2 (§9.3): a vector breaks one rule, so its class is required.
                    t.class_disagreements.push(format!("{name}: vector {} / ref2 {}", cls, e));
                    t.failures.push(format!("{name}: class {cls} expected (§9.3), ref2 {e}"));
                }
            }
            (None, Some(cls), Ok(v)) => t.failures.push(format!("{name}: expected error {cls}, got {}", tensor_json(&v))),
            _ => panic!("{name}: neither expect nor expect_error"),
        }
    }
}

#[test]
fn primitive_vectors() {
    let mut t = Tally::default();
    let dir = vectors_dir().join("primitives");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 23, "one file per stateless primitive, tags 0..=22");
    for f in &files {
        run_primitive_file(f, &mut t);
    }
    println!("primitive vectors: {}/{} reproduced", t.reproduced, t.total);
    for c in &t.class_disagreements {
        println!("  class (diagnostic): {c}");
    }
    for f in &t.failures {
        println!("  FAIL {f}");
    }
    assert!(t.failures.is_empty(), "{} primitive vectors not reproduced", t.failures.len());
}

fn params_of(prog: &misaka_palw_tir_ref2::Program, v: &Value) -> Params {
    let mut params = Params::new();
    for e in v.as_array().unwrap() {
        let j = int_of(&e["param"]) as u16;
        let layer = if e["layer"].is_null() { None } else { Some(int_of(&e["layer"]) as u32) };
        let d = &prog.params[j as usize];
        let t =
            Tensor::from_le_bytes(d.dtype, d.shape.iter().map(|&x| x as u64).collect(), &hex_decode(e["le_hex"].as_str().unwrap()))
                .expect("param bytes");
        params.insert((j, layer), t);
    }
    params
}

fn indexed(v: &Value, key: &str) -> BTreeMap<u64, Tensor> {
    v.as_array().unwrap().iter().map(|e| (int_of(&e[key]) as u64, tensor_of(&e["value"]))).collect()
}

fn run_program_file(path: &std::path::Path) -> (usize, usize, Vec<String>) {
    let d = read_json(path);
    assert_eq!(d["format"], "palw-tir-v1/program-vectors/3");
    let name = d["name"].as_str().unwrap().to_string();
    let bytes = hex_decode(d["program_borsh_hex"].as_str().unwrap());
    let prog = decode_canonical(&bytes).unwrap_or_else(|e| panic!("{name}: the program is refused: {e}"));
    assert_eq!(encode(&prog), bytes);
    // §3.6: the identity, computed from the stated definition.
    let root = hex_encode(&misaka_palw_tir_ref2::graph_ir_root(&prog));
    assert_eq!(root, d["graph_ir_root_hex"].as_str().unwrap(), "{name}: graph_ir_root");
    if name == "fixed-state-saturation" {
        // The value §3.6 prints.
        assert_eq!(
            root,
            concat!(
                "dcc4422a07d843bd575c689b18c8b2e41812d69589ca97749d722b2ba987ea80",
                "eb8ad3ffa4dcac6672befc8ecc0c909f6f0bc09711336095def307d6cfa89c88"
            )
        );
        assert_eq!(bytes.len(), 470);
    }
    let params = params_of(&prog, &d["params"]);
    let mut total = 0;
    let mut ok = 0;
    let mut fails = Vec::new();
    // Steps: a run from the initial state, positions 0 … T−1.
    let steps = d["steps"].as_array().unwrap();
    let tokens: Vec<u64> = steps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            assert_eq!(int_of(&s["pos"]) as usize, i, "{name}: steps are consecutive from 0");
            int_of(&s["token"]) as u64
        })
        .collect();
    match run(&prog, &params, &tokens) {
        Err(e) => {
            total += steps.len();
            fails.push(format!("{name}: run failed: {e}"));
        }
        Ok(outs) => {
            for (s, o) in steps.iter().zip(outs.iter()) {
                total += 1;
                let mut good = tensor_of(&s["logits"]) == o.logits;
                if !good {
                    fails.push(format!("{name} pos {}: logits {} vs ref2 {}", s["pos"], s["logits"], tensor_json(&o.logits)));
                }
                let commits = s["commits"].as_array().unwrap();
                if commits.len() != o.commits.len() {
                    good = false;
                    fails.push(format!("{name} pos {}: {} commits vs ref2 {}", s["pos"], commits.len(), o.commits.len()));
                }
                for (c, m) in commits.iter().zip(o.commits.iter()) {
                    let layer = if c["layer"].is_null() { None } else { Some(int_of(&c["layer"]) as u32) };
                    let same = int_of(&c["slot"]) as u64 == m.slot
                        && int_of(&c["block"]) as u8 == m.block
                        && layer == m.layer
                        && int_of(&c["node"]) as u16 == m.node
                        && tensor_of(&c["value"]) == m.value;
                    if !same {
                        good = false;
                        fails.push(format!(
                            "{name} pos {}: commit slot {} (block {} layer {:?} node {}) differs: {} vs ref2 slot {} {}",
                            s["pos"],
                            c["slot"],
                            c["block"],
                            layer,
                            c["node"],
                            c["value"],
                            m.slot,
                            tensor_json(&m.value)
                        ));
                    }
                }
                if good {
                    ok += 1;
                }
            }
        }
    }
    // Cones.
    for (ci, c) in d["cones"].as_array().unwrap().iter().enumerate() {
        total += 1;
        let layer = if c["layer"].is_null() { None } else { Some(int_of(&c["layer"]) as u32) };
        let env = env_of(c);
        let got = eval_cone(&prog, &params, int_of(&c["block"]) as u8, layer, int_of(&c["target"]) as u16, &env);
        match (c.get("expect"), got) {
            (Some(exp), Ok(v)) if tensor_of(exp) == v => ok += 1,
            (Some(exp), Ok(v)) => fails.push(format!("{name} cone {ci}: expected {exp} got {}", tensor_json(&v))),
            (Some(exp), Err(e)) => fails.push(format!("{name} cone {ci}: expected {exp} got error {e}")),
            (None, r) => fails.push(format!("{name} cone {ci}: no expect, got {r:?}")),
        }
    }
    // Refusals (revision 2): an honest environment with one defect, and the §9.3 class.
    for (ri, c) in d["refusals"].as_array().unwrap().iter().enumerate() {
        total += 1;
        let layer = if c["layer"].is_null() { None } else { Some(int_of(&c["layer"]) as u32) };
        let env = env_of(c);
        let got = eval_cone(&prog, &params, int_of(&c["block"]) as u8, layer, int_of(&c["target"]) as u16, &env);
        let want = c["expect_error"].as_str().unwrap();
        match got {
            Err(e) if e.class.name() == want => ok += 1,
            Err(e) => fails.push(format!("{name} refusal {ri} ({}): expected {want}, got {e}", c["what"])),
            Ok(v) => fails.push(format!("{name} refusal {ri} ({}): expected {want}, got {}", c["what"], tensor_json(&v))),
        }
    }
    (total, ok, fails)
}

/// A cone environment from a vector (`token` null = absent).
fn env_of(c: &Value) -> ConeEnv {
    ConeEnv {
        token: if c["token"].is_null() { None } else { Some(int_of(&c["token"]) as u64) },
        pos: int_of(&c["pos"]) as u64,
        carry_in: indexed(&c["carry_in"], "index").into_iter().map(|(k, v)| (k as u8, v)).collect(),
        fixed: indexed(&c["fixed"], "index").into_iter().map(|(k, v)| (k as u16, v)).collect(),
        hist_prior: c["hist_prior"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| (int_of(&h["state"]) as u16, h["rows"].as_array().unwrap().iter().map(tensor_of).collect()))
            .collect(),
        supplied: indexed(&c["supplied"], "index").into_iter().map(|(k, v)| (k as u16, v)).collect(),
    }
}

#[test]
fn program_vectors() {
    let dir = vectors_dir().join("programs");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 7);
    let (mut total, mut ok, mut fails) = (0, 0, Vec::new());
    for f in &files {
        let (t, o, fl) = run_program_file(f);
        println!("{}: {o}/{t}", f.file_name().unwrap().to_string_lossy());
        total += t;
        ok += o;
        fails.extend(fl);
    }
    println!("program vectors (steps + cones + refusals): {ok}/{total} reproduced");
    for f in &fails {
        println!("  FAIL {f}");
    }
    assert!(fails.is_empty(), "{} program vector items not reproduced", fails.len());
}

#[test]
fn encoding_vectors() {
    let d = read_json(&vectors_dir().join("encoding.json"));
    assert_eq!(d["format"], "palw-tir-v1/encoding-vectors/1");
    let cases = d["cases"].as_array().unwrap();
    let mut fails = Vec::new();
    let mut classes = Vec::new();
    for c in cases {
        let name = c["name"].as_str().unwrap();
        let bytes = hex_decode(c["hex"].as_str().unwrap());
        let got = decode_canonical(&bytes);
        let expect = c["expect"].as_str().unwrap();
        match (expect, got) {
            ("ok", Ok(p)) => assert_eq!(encode(&p), bytes),
            ("ok", Err(e)) => fails.push(format!("{name}: expected ok, got {e}")),
            (cls, Ok(_)) => fails.push(format!("{name}: expected {cls}, accepted")),
            (cls, Err(e)) => {
                assert!(Class::from_name(cls).is_some(), "{name}: unknown class {cls}");
                if cls != e.class.name() {
                    classes.push(format!("{name}: vector {cls} / ref2 {e}"));
                    fails.push(format!("{name}: class {cls} expected (§9.3), ref2 {e}"));
                }
            }
        }
    }
    println!("encoding vectors: {}/{} reproduced", cases.len() - fails.len(), cases.len());
    for c in &classes {
        println!("  class (diagnostic): {c}");
    }
    for f in &fails {
        println!("  FAIL {f}");
    }
    assert!(fails.is_empty());
    let _ = DType::I8;
}
