//! 04b §9.4's golden vectors (`consensus-vectors/tir-v1/demand/`), reproduced by this crate's
//! independent demand evaluator: for every case the values, the work and the grouped request set,
//! or the refusal's class.
//!
//! The source is built as §9.4 "Golden vectors" states: `node` from the commits of the program
//! vector's steps (the commit whose `(block, layer)` is the occurrence's), `token` from the steps,
//! `hist_row` from the committed node the row is, `param` from `params`, and `state` from the file's
//! (or the case's own) `states`, every other `(p, j, l)` answering `Replay`. Anything else is refused.

mod common;

use std::collections::BTreeMap;

use common::demsrc::{Model, grouped, hist_sources, mine_demand, occurrences, DRes};
use common::*;
use misaka_palw_tir_ref2::Program;
use misaka_palw_tir_ref2::codec::decode_canonical;
use misaka_palw_tir_ref2::demand::{Ctx, Limits, Question, Target};
use serde_json::Value;

fn layer_of(v: &Value) -> Option<u32> {
    if v.is_null() { None } else { Some(int_of(v) as u32) }
}

fn u64_of(v: &Value) -> u64 {
    match v {
        Value::String(s) => s.parse::<u64>().unwrap_or_else(|e| panic!("u64 {s}: {e}")),
        _ => int_of(v) as u64,
    }
}

/// The program vector's committed data as a model (without `states`).
fn base_model(prog: &Program, pv: &Value) -> Model {
    let mut m = Model { hist_src: hist_sources(prog), ..Default::default() };
    for e in pv["params"].as_array().unwrap() {
        let j = int_of(&e["param"]) as u16;
        let d = &prog.params[j as usize];
        let t = misaka_palw_tir_ref2::Tensor::from_le_bytes(
            d.dtype,
            d.shape.iter().map(|&x| x as u64).collect(),
            &hex_decode(e["le_hex"].as_str().unwrap()),
        )
        .unwrap();
        m.params.insert((j, layer_of(&e["layer"])), t.data);
    }
    // Slot ranges of the occurrences.
    let occ = occurrences(prog);
    let mut bases = Vec::new();
    let mut base = 0u64;
    for &(b, _) in &occ {
        bases.push(base);
        base += prog.blocks[b].nodes.len() as u64;
    }
    for s in pv["steps"].as_array().unwrap() {
        let pos = u64_of(&s["pos"]);
        m.tokens.insert(pos, u64_of(&s["token"]));
        for c in s["commits"].as_array().unwrap() {
            let slot = u64_of(&c["slot"]);
            let o = bases.iter().rposition(|&b| b <= slot).unwrap();
            let (b, layer) = occ[o];
            assert_eq!(b as u64, u64_of(&c["block"]));
            assert_eq!(layer, layer_of(&c["layer"]));
            assert_eq!(slot - bases[o], u64_of(&c["node"]));
            m.nodes.insert((pos, o as u32, u64_of(&c["node"]) as u16), tensor_of(&c["value"]).data);
        }
    }
    m
}

fn states_of(v: &Value) -> BTreeMap<(u64, u16, Option<u32>), Vec<i128>> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|e| ((u64_of(&e["pos"]), int_of(&e["state"]) as u16, layer_of(&e["layer"])), tensor_of(&e["value"]).data))
        .collect()
}

fn target_of(v: &Value) -> Target {
    if let Some(n) = v.get("node") {
        Target::Node { ctx: Ctx { pos: u64_of(&n["pos"]), occ: u64_of(&n["occurrence"]) as u32 }, node: u64_of(&n["node"]) as u16 }
    } else {
        let s = &v["state_after"];
        Target::StateAfter { pos: u64_of(&s["pos"]), state: u64_of(&s["state"]) as u16, layer: layer_of(&s["layer"]) }
    }
}

/// A withheld question (`requests` form without `indices`), as its grouping key.
fn question_of(v: &Value) -> Question {
    let l = |x: &Value| layer_of(x);
    if let Some(n) = v.get("node") {
        Question::Node { ctx: Ctx { pos: u64_of(&n["pos"]), occ: u64_of(&n["occurrence"]) as u32 }, node: u64_of(&n["node"]) as u16, i: 0 }
    } else if let Some(n) = v.get("param") {
        Question::Param { param: u64_of(&n["param"]) as u16, layer: l(&n["layer"]), i: 0 }
    } else if let Some(n) = v.get("state") {
        Question::State { pos: u64_of(&n["pos"]), state: u64_of(&n["state"]) as u16, layer: l(&n["layer"]), i: 0 }
    } else if let Some(n) = v.get("hist_row") {
        Question::HistRow {
            pos: u64_of(&n["pos"]),
            state: u64_of(&n["state"]) as u16,
            layer: l(&n["layer"]),
            row_pos: u64_of(&n["row_pos"]),
            i: 0,
        }
    } else {
        Question::Token { pos: u64_of(&v["token"]["pos"]) }
    }
}

/// The expected grouping, printed as `grouped` prints ours.
fn expected_requests(v: &Value) -> Vec<String> {
    let l = |x: &Value| if x.is_null() { "null".to_string() } else { u64_of(x).to_string() };
    v.as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let idx = r.get("indices").and_then(|i| i.as_str()).unwrap_or("");
            let head = if let Some(n) = r.get("node") {
                format!("node(pos {}, occ {}, node {})", u64_of(&n["pos"]), u64_of(&n["occurrence"]), u64_of(&n["node"]))
            } else if let Some(n) = r.get("param") {
                format!("param({}, layer {})", u64_of(&n["param"]), l(&n["layer"]))
            } else if let Some(n) = r.get("state") {
                format!("state(pos {}, state {}, layer {})", u64_of(&n["pos"]), u64_of(&n["state"]), l(&n["layer"]))
            } else if let Some(n) = r.get("hist_row") {
                format!(
                    "hist_row(pos {}, state {}, layer {}, row {})",
                    u64_of(&n["pos"]),
                    u64_of(&n["state"]),
                    l(&n["layer"]),
                    u64_of(&n["row_pos"])
                )
            } else {
                format!("token(pos {})", u64_of(&r["token"]["pos"]))
            };
            format!("{head} {idx}")
        })
        .collect()
}

#[test]
fn demand_vectors() {
    let dir = vectors_dir().join("demand");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    assert_eq!(files.len(), 7);
    let (mut total, mut ok) = (0usize, 0usize);
    let mut fails = Vec::new();
    for f in &files {
        let d = read_json(f);
        assert_eq!(d["format"], "palw-tir-v1/demand-vectors/1");
        let pv = read_json(&vectors_dir().join(d["program"].as_str().unwrap()));
        let prog = decode_canonical(&hex_decode(pv["program_borsh_hex"].as_str().unwrap())).unwrap();
        let base = base_model(&prog, &pv);
        let file_states = states_of(&d["states"]);
        let (mut ft, mut fo) = (0, 0);
        for c in d["cases"].as_array().unwrap() {
            ft += 1;
            let name = format!("{} / {}", d["name"].as_str().unwrap(), c["name"].as_str().unwrap());
            let mut m = base.clone();
            m.states = match c.get("states") {
                Some(s) => states_of(s),
                None => file_states.clone(),
            };
            if let Some(w) = c.get("withhold") {
                for q in w.as_array().unwrap() {
                    m.withheld.insert(question_of(q));
                }
            }
            let target = target_of(&c["target"]);
            let elements: Vec<u64> = c["elements"].as_array().unwrap().iter().map(u64_of).collect();
            let limits = Limits { max_elements: u64_of(&c["limits"]["max_elements"]), max_terms: u64_of(&c["limits"]["max_terms"]) };
            let run = mine_demand(&prog, &target, &elements, &m, limits);
            match (c.get("expect"), &run.res) {
                (Some(exp), DRes::Ok(vals, work)) => {
                    let want_vals: Vec<i128> = exp["values"].as_array().unwrap().iter().map(int_of).collect();
                    let want_work = (u64_of(&exp["work"]["elements"]), u64_of(&exp["work"]["terms"]));
                    let want_req = expected_requests(&exp["requests"]);
                    let got_req = grouped(&run.asked);
                    let mut bad = Vec::new();
                    if *vals != want_vals {
                        bad.push(format!("values {vals:?} vs {want_vals:?}"));
                    }
                    if (work.elements, work.terms) != want_work {
                        bad.push(format!("work ({}, {}) vs {want_work:?}", work.elements, work.terms));
                    }
                    if got_req != want_req {
                        bad.push(format!("requests\n      ours   {got_req:?}\n      vector {want_req:?}"));
                    }
                    if bad.is_empty() {
                        fo += 1;
                    } else {
                        fails.push(format!("{name}: {}", bad.join("; ")));
                    }
                }
                (Some(_), other) => fails.push(format!("{name}: expected success, got {other:?}")),
                (None, DRes::Err(e)) if e.name() == c["expect_error"].as_str().unwrap() => fo += 1,
                (None, other) => fails.push(format!("{name}: expected {}, got {other:?}", c["expect_error"])),
            }
        }
        println!("{}: {fo}/{ft}", f.file_name().unwrap().to_string_lossy());
        total += ft;
        ok += fo;
    }
    println!("demand vectors: {ok}/{total} reproduced");
    for f in &fails {
        println!("  FAIL {f}");
    }
    assert!(fails.is_empty(), "{} demand vectors not reproduced", fails.len());
}
