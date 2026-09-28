//! **Every golden vector of `consensus-vectors/tir-v1/` on the typed backend** (spec 04b §12,
//! PALW-TIR-35).
//!
//! * `primitives/` — each case becomes a one-node program (operands as consts, and again as
//!   params, so both the tight-interval and the full-range code paths run) evaluated by the
//!   executor; the node's value must be `expect`, or the case must fail where it records
//!   `expect_error` (only success versus failure is normative, PALW-TIR-34).
//! * `programs/` — every step's logits and every commit point of every position; every cone case
//!   and every cone refusal (spec 04b revision 2, §9.2) through the backend's cone evaluator.
//! * `encoding.json` — acceptance of the byte strings (decode, then the plan's validation).
//! * `demand/` — every demand-evaluation case that pins values holds the executor's values there.
//!
//! Where a vector pins an error class (spec 04b §9.3: one class per rule, and every vector breaks
//! one rule) the backend must report that class.

mod common;

use std::path::PathBuf;

use common::Collect;
use misaka_palw_tir::prim::PRIM_SET_ID_V1;
use misaka_palw_tir::program::{Block, ConstDecl, HISTORY_BOUND_V1_SMALL, Node, ParamDecl, Schedule, TirProgramV1};
use misaka_palw_tir::{ConeEnv, DType, MapParams, Prim, Ref, Tensor, TensorType, TirError};
use misaka_palw_tir_exec::{TirExecutor, TirParams, TirPlan, eval_cone};
use serde_json::Value;

fn vectors() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("consensus-vectors").join("tir-v1")
}

fn read(path: &std::path::Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))).unwrap()
}

fn int(v: &Value) -> i128 {
    match v {
        Value::String(s) => s.parse().unwrap(),
        Value::Number(n) => n.as_i64().map(|x| x as i128).or(n.as_u64().map(|x| x as i128)).unwrap(),
        _ => panic!("not an integer: {v}"),
    }
}

fn tensor(v: &Value) -> Tensor {
    let dtype = DType::from_name(v["dtype"].as_str().unwrap()).unwrap();
    let shape = v["shape"].as_array().unwrap().iter().map(|d| int(d) as usize).collect();
    let data = v["data"].as_array().unwrap().iter().map(int).collect();
    Tensor::new(dtype, shape, data).unwrap()
}

fn opt_u16(v: &Value) -> Option<u16> {
    if v.is_null() { None } else { Some(int(v) as u16) }
}

/// One primitive as a one-node program: operands as consts (or as params, except `i128`, which
/// can only be a const), the node committed when its dtype allows, a committed `Clamp` marker
/// otherwise, and a trivial post block.
fn primitive_program(prim: &Prim, inputs: &[Tensor], out: &TensorType, as_params: bool) -> (TirProgramV1, MapParams) {
    let mut params = Vec::new();
    let mut consts: Vec<ConstDecl> = Vec::new();
    let mut map = MapParams::default();
    let mut refs = Vec::new();
    for (i, t) in inputs.iter().enumerate() {
        let shape: Vec<u32> = t.shape.iter().map(|d| *d as u32).collect();
        if as_params && t.dtype != DType::I128 {
            params.push(ParamDecl { name: format!("in{i}"), dtype: t.dtype, shape, per_layer: false });
            let j = (params.len() - 1) as u16;
            map.tensors.insert((j, None), t.clone());
            refs.push(Ref::Param(j));
        } else {
            let c = ConstDecl { dtype: t.dtype, shape, data: t.to_le_bytes() };
            let j = consts.iter().position(|x| *x == c).unwrap_or_else(|| {
                consts.push(c);
                consts.len() - 1
            });
            refs.push(Ref::Const(j as u16));
        }
    }
    let committable = out.dtype.committable();
    let mut nodes = vec![Node { prim: prim.clone(), inputs: refs, out: out.clone(), commit: committable }];
    let root = if committable {
        0u16
    } else {
        let marker = TensorType::new(DType::I32, out.shape.clone());
        nodes.push(Node {
            prim: Prim::Clamp { lo: i32::MIN as i64, hi: i32::MAX as i64 },
            inputs: vec![Ref::Node(0)],
            out: marker,
            commit: true,
        });
        1
    };
    let carry = nodes[root as usize].out.clone();
    let pre = Block { name: "pre".into(), carry_in: vec![], nodes, carry_out: vec![root] };
    let post = Block {
        name: "post".into(),
        carry_in: vec![carry.clone()],
        nodes: vec![Node { prim: Prim::Reshape, inputs: vec![Ref::CarryIn(0)], out: carry, commit: true }],
        carry_out: vec![],
    };
    let program = TirProgramV1 {
        version: 1,
        prim_set_id: PRIM_SET_ID_V1,
        token_bound: 16,
        history_bound: HISTORY_BOUND_V1_SMALL,
        params,
        consts,
        states: vec![],
        blocks: vec![pre, post],
        schedule: Schedule { pre: 0, layers: vec![], post: 1 },
        logits: 0,
        logits_scheme_id: [0; 64],
    };
    (program, map)
}

/// The executor's value of node 0, or the failure.
fn run_primitive(prim: &Prim, inputs: &[Tensor], out: &TensorType, as_params: bool) -> Result<Tensor, TirError> {
    // Outside a program there is no normal form: a wrong number of operands fails the
    // primitive's type rule, `Shape` (spec 04b §9.3). Wrapped in a program it would be NF-14's.
    let (lo, hi) = prim.arity();
    if inputs.len() < lo || inputs.len() > hi {
        return Err(TirError::new(misaka_palw_tir::TirErrorKind::Shape, format!("{} operands, arity {lo}..={hi}", inputs.len())));
    }
    let (program, map) = primitive_program(prim, inputs, out, as_params);
    let plan = TirPlan::compile(&program)?;
    let params = TirParams::from_map(&plan, &map)?;
    let mut exec = TirExecutor::new(&plan, &params)?;
    let mut sink = Collect::new(true);
    exec.step(0, &mut sink)?;
    Ok(sink.values.iter().find(|r| r.block == 0 && r.node == 0).expect("node 0 delivered").value.clone())
}

#[test]
fn every_primitive_vector() {
    let dir = vectors().join("primitives");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 23, "one file per stateless primitive (tags 0–22)");
    let (mut ok, mut err) = (0usize, 0usize);
    for f in &files {
        let v = read(f);
        for case in v["cases"].as_array().unwrap() {
            let name = case["name"].as_str().unwrap();
            let hex = case["prim"]["borsh_hex"].as_str().unwrap();
            let prim: Prim = borsh::from_slice(&common::hex_decode(hex)).unwrap();
            let inputs: Vec<Tensor> = case["inputs"].as_array().unwrap().iter().map(tensor).collect();
            let out_dt = DType::from_name(case["out"]["dtype"].as_str().unwrap()).unwrap();
            let out_shape: Vec<u32> = case["out"]["shape"].as_array().unwrap().iter().map(|d| int(d) as u32).collect();
            let out = TensorType::fixed(out_dt, &out_shape);
            for as_params in [false, true] {
                let got = run_primitive(&prim, &inputs, &out, as_params);
                match (&case["expect"], &case["expect_error"]) {
                    (e, Value::Null) => {
                        let want = tensor(e);
                        assert_eq!(got.as_ref(), Ok(&want), "{} / {name} (params: {as_params})", f.display());
                    }
                    (Value::Null, class) => {
                        let class = class.as_str().unwrap();
                        match &got {
                            Err(e) => {
                                assert_eq!(format!("{:?}", e.kind), class, "{} / {name} (params: {as_params}): {e}", f.display())
                            }
                            Ok(v) => panic!("{} / {name} (params: {as_params}): expected {class}, got {v:?}", f.display()),
                        }
                    }
                    _ => panic!("a case has both expect and expect_error"),
                }
            }
            if case["expect"].is_null() { err += 1 } else { ok += 1 }
        }
    }
    eprintln!("primitive vectors: {ok} succeed, {err} fail, each run with const and with param operands");
    assert!(ok + err >= 130);
}

#[test]
fn every_program_vector() {
    let dir = vectors().join("programs");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 7);
    let (mut steps, mut commits, mut cones, mut refusals) = (0usize, 0usize, 0usize, 0usize);
    for f in &files {
        let v = read(f);
        let program = TirProgramV1::decode_canonical(&common::hex_decode(v["program_borsh_hex"].as_str().unwrap())).unwrap();
        let plan = TirPlan::compile(&program).unwrap();
        let mut map = MapParams::default();
        for p in v["params"].as_array().unwrap() {
            let j = int(&p["param"]) as u16;
            let d = &program.params[j as usize];
            let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
            let t = Tensor::from_le_bytes(d.dtype, &shape, &common::hex_decode(p["le_hex"].as_str().unwrap())).unwrap();
            map.tensors.insert((j, opt_u16(&p["layer"])), t);
        }
        let params = TirParams::from_map(&plan, &map).unwrap();
        let mut exec = TirExecutor::new(&plan, &params).unwrap();
        for s in v["steps"].as_array().unwrap() {
            assert_eq!(int(&s["pos"]) as u32, exec.pos());
            let mut sink = Collect::new(false);
            exec.step(int(&s["token"]) as u32, &mut sink).unwrap_or_else(|e| panic!("{}: step failed: {e}", f.display()));
            let (shape, data) = exec.logits();
            let want = tensor(&s["logits"]);
            assert_eq!(shape, &want.shape[..], "{}", f.display());
            assert_eq!(data.to_i128s(), want.data, "{}: logits at pos {}", f.display(), int(&s["pos"]));
            let expect = s["commits"].as_array().unwrap();
            assert_eq!(sink.values.len(), expect.len(), "{}: commit count", f.display());
            for (m, c) in sink.values.iter().zip(expect) {
                assert_eq!(m.slot, int(&c["slot"]) as u32);
                assert_eq!(m.block, int(&c["block"]) as u8);
                assert_eq!(m.layer, opt_u16(&c["layer"]));
                assert_eq!(m.node, int(&c["node"]) as u16);
                assert_eq!(m.value, tensor(&c["value"]), "{}: slot {} at pos {}", f.display(), m.slot, int(&s["pos"]));
                commits += 1;
            }
            steps += 1;
        }
        let lenient = TirParams::from_map_lenient(&plan, &map).unwrap();
        for c in v["cones"].as_array().unwrap() {
            let got = eval_cone(&plan, &lenient, int(&c["block"]) as u8, opt_u16(&c["layer"]), int(&c["target"]) as u16, &cone_env(c))
                .unwrap_or_else(|e| panic!("{}: cone failed: {e}", f.display()));
            assert_eq!(got, tensor(&c["expect"]), "{}: cone of node {}", f.display(), int(&c["target"]));
            cones += 1;
        }
        for c in v["refusals"].as_array().unwrap() {
            let what = c["what"].as_str().unwrap();
            let got = eval_cone(&plan, &lenient, int(&c["block"]) as u8, opt_u16(&c["layer"]), int(&c["target"]) as u16, &cone_env(c));
            match got {
                Err(e) => assert_eq!(format!("{:?}", e.kind), c["expect_error"].as_str().unwrap(), "{}: {what}: {e}", f.display()),
                Ok(t) => panic!("{}: {what}: expected {}, got {t:?}", f.display(), c["expect_error"]),
            }
            refusals += 1;
        }
    }
    eprintln!("program vectors: {steps} steps, {commits} commit points, {cones} cones, {refusals} refusals");
}

/// **The demand-evaluation vectors** (`demand/`, spec 04b §9.4): every case that pins values — a
/// node's elements at a position, or a `Fixed` instance's after one — holds the value the typed
/// backend computes there on the program vector's run (the demand evaluator pulls exactly those
/// elements; the executor computes the whole node, and the two must agree element for element).
/// Work limits, requests and refusals are the demand evaluator's own and are not the executor's.
#[test]
fn every_demand_vector_value() {
    let dir = vectors().join("demand");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert!(files.len() >= 7, "{} demand files", files.len());
    let (mut nodes, mut states) = (0usize, 0usize);
    for f in &files {
        let d = read(f);
        let v = read(&vectors().join(d["program"].as_str().unwrap()));
        let program = TirProgramV1::decode_canonical(&common::hex_decode(v["program_borsh_hex"].as_str().unwrap())).unwrap();
        let plan = TirPlan::compile(&program).unwrap();
        let mut map = MapParams::default();
        for p in v["params"].as_array().unwrap() {
            let j = int(&p["param"]) as u16;
            let decl = &program.params[j as usize];
            let shape: Vec<usize> = decl.shape.iter().map(|x| *x as usize).collect();
            let t = Tensor::from_le_bytes(decl.dtype, &shape, &common::hex_decode(p["le_hex"].as_str().unwrap())).unwrap();
            map.tensors.insert((j, opt_u16(&p["layer"])), t);
        }
        let params = TirParams::from_map(&plan, &map).unwrap();
        let mut exec = TirExecutor::new(&plan, &params).unwrap();
        // Every node's value at every position, and every Fixed instance after it.
        let mut at: Vec<std::collections::BTreeMap<(u8, Option<u16>, u16), Tensor>> = Vec::new();
        let mut fixed_after: Vec<std::collections::BTreeMap<(u16, Option<u16>), Vec<i128>>> = Vec::new();
        for s in v["steps"].as_array().unwrap() {
            let mut sink = Collect::new(true);
            exec.step(int(&s["token"]) as u32, &mut sink).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            at.push(sink.values.into_iter().map(|r| ((r.block, r.layer, r.node), r.value)).collect());
            let mut fx = std::collections::BTreeMap::new();
            for (j, st) in program.states.iter().enumerate() {
                let layers: Vec<Option<u16>> =
                    if st.per_layer { (0..program.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
                for l in layers {
                    if let Some(val) = exec.fixed_value(j as u16, l) {
                        fx.insert((j as u16, l), val.to_i128s());
                    }
                }
            }
            fixed_after.push(fx);
        }
        for c in d["cases"].as_array().unwrap() {
            let Some(values) = c.get("expect").map(|e| e["values"].as_array().unwrap()) else { continue };
            let want: Vec<i128> = values.iter().map(int).collect();
            let elements: Vec<usize> = c["elements"].as_array().unwrap().iter().map(|e| int(e) as usize).collect();
            let name = c["name"].as_str().unwrap();
            let got: Vec<i128> = if let Some(t) = c["target"].get("node") {
                let pos = int(&t["pos"]) as usize;
                let (block, layer) = plan.occurrences[int(&t["occurrence"]) as usize];
                let value = at[pos].get(&(block, layer, int(&t["node"]) as u16)).unwrap_or_else(|| panic!("{}: {name}", f.display()));
                nodes += 1;
                elements.iter().map(|e| value.data[*e]).collect()
            } else {
                let t = &c["target"]["state_after"];
                let pos = int(&t["pos"]) as usize;
                let key = (int(&t["state"]) as u16, opt_u16(&t["layer"]));
                let value = fixed_after[pos].get(&key).unwrap_or_else(|| panic!("{}: {name}", f.display()));
                states += 1;
                elements.iter().map(|e| value[*e]).collect()
            };
            assert_eq!(got, want, "{}: {name}", f.display());
        }
    }
    eprintln!("demand vectors: {nodes} node cases and {states} state cases hold the executor's values");
    assert!(nodes > 500 && states > 30);
}

/// A cone case's environment (`token` null when absent).
fn cone_env(c: &Value) -> ConeEnv {
    let named = |key: &str| -> std::collections::BTreeMap<u16, Tensor> {
        c[key].as_array().unwrap().iter().map(|e| (int(&e["index"]) as u16, tensor(&e["value"]))).collect()
    };
    ConeEnv {
        token: if c["token"].is_null() { None } else { Some(int(&c["token"]) as u32) },
        pos: int(&c["pos"]) as u32,
        carry_in: named("carry_in").into_iter().map(|(k, v)| (k as u8, v)).collect(),
        fixed: named("fixed"),
        hist_prior: c["hist_prior"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| (int(&h["state"]) as u16, h["rows"].as_array().unwrap().iter().map(tensor).collect()))
            .collect(),
        supplied: named("supplied"),
    }
}

#[test]
fn every_encoding_vector() {
    let v = read(&vectors().join("encoding.json"));
    let mut n = 0;
    for c in v["cases"].as_array().unwrap() {
        let bytes = common::hex_decode(c["hex"].as_str().unwrap());
        let r = TirProgramV1::decode_canonical(&bytes).and_then(|p| TirPlan::compile(&p).map(|_| ()));
        let got = match r {
            Ok(()) => "ok".to_string(),
            Err(e) => format!("{:?}", e.kind),
        };
        assert_eq!(got, c["expect"].as_str().unwrap(), "{}", c["name"]);
        n += 1;
    }
    assert!(n >= 12);
}
