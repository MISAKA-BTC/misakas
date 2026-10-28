//! 04b §12's admission vectors (`consensus-vectors/tir-v1/admission.json`), reproduced by this
//! crate's independent admission: every derived quantity of an admitted program — the checkpoint
//! interval, the cone work, the per-position quantities, every written state's closure, groups,
//! per-position cost and interval, every cone, every node's §8 cost and §7 interval — and every
//! refusal's kind, limit, value and cap (or class).

mod common;

use common::*;
use misaka_palw_tir_ref2::admit::{Admission, AdmitError, AdmitInputs, Ceilings, Cost, Leaf, admit};
use serde_json::Value;

fn u64_of(v: &Value) -> u64 {
    int_of(v) as u64
}

fn cost_of(v: &Value) -> Cost {
    Cost {
        macs: u64_of(&v["macs"]),
        elementwise: u64_of(&v["elementwise"]),
        transcendentals: u64_of(&v["transcendentals"]),
        bytes_read: u64_of(&v["bytes_read"]),
        bytes_written: u64_of(&v["bytes_written"]),
    }
}

fn inputs_of(v: &Value) -> AdmitInputs {
    let c = &v["ceilings"];
    AdmitInputs {
        tile_len: u64_of(&v["tile_len"]) as u32,
        h_chunk: u64_of(&v["h_chunk"]) as u32,
        ceilings: Ceilings {
            max_tile_macs: u64_of(&c["max_tile_macs"]),
            max_tile_transcendentals: u64_of(&c["max_tile_transcendentals"]),
            max_tile_opened_bytes: u64_of(&c["max_tile_opened_bytes"]),
            max_tile_operands: u64_of(&c["max_tile_operands"]),
            max_position_macs: u64_of(&c["max_position_macs"]),
            max_position_transcendentals: u64_of(&c["max_position_transcendentals"]),
            max_state_bytes: u64_of(&c["max_state_bytes"]),
            max_step_leaves: u64_of(&c["max_step_leaves"]),
            max_checkpoint_interval: u64_of(&c["max_checkpoint_interval"]) as u32,
            max_cone_work: u64_of(&c["max_cone_work"]),
        },
    }
}

fn leaf_str(l: &Leaf) -> String {
    match l {
        Leaf::Commit(n) => format!("commit:{n}"),
        Leaf::CarryIn(k) => format!("carry_in:{k}"),
        Leaf::State(j) => format!("state:{j}"),
        Leaf::History(j) => format!("history:{j}"),
        Leaf::Param(j) => format!("param:{j}"),
        Leaf::Const(j) => format!("const:{j}"),
        Leaf::Input(j) => format!("input:{j}"),
    }
}

fn u64s(v: &Value) -> Vec<u64> {
    v.as_array().unwrap().iter().map(u64_of).collect()
}

/// Every difference between this crate's admission and the vector's.
fn compare(a: &Admission, e: &Value) -> Vec<String> {
    let mut d = Vec::new();
    let mut check = |what: String, same: bool| {
        if !same {
            d.push(what);
        }
    };
    check(
        format!("checkpoint_interval {} vs {}", a.checkpoint_interval, e["checkpoint_interval"]),
        a.checkpoint_interval as u64 == u64_of(&e["checkpoint_interval"]),
    );
    check(format!("cone_work {} vs {}", a.cone_work, e["cone_work"]), a.cone_work == u64_of(&e["cone_work"]));
    let p = &e["position"];
    let pos = &a.position;
    check(format!("position cost {:?} vs {}", pos.cost, p["cost"]), pos.cost == cost_of(&p["cost"]));
    for (name, mine) in [
        ("state_bytes", pos.state_bytes),
        ("peak_live_bytes", pos.peak_live_bytes),
        ("commit_lanes", pos.commit_lanes),
        ("step_leaves", pos.step_leaves),
    ] {
        check(format!("position {name} {mine} vs {}", p[name]), mine == u64_of(&p[name]));
    }
    // States.
    let es = e["states"].as_array().unwrap();
    check(format!("{} states vs {}", a.states.len(), es.len()), a.states.len() == es.len());
    for (s, x) in a.states.iter().zip(es) {
        let mut cl = s.closure.clone();
        cl.sort();
        let same = s.state as u64 == u64_of(&x["state"])
            && cl.iter().map(|&c| c as u64).collect::<Vec<_>>() == u64s(&x["closure"])
            && s.groups == u64_of(&x["groups"])
            && s.per_position == cost_of(&x["per_position"])
            && s.interval as u64 == u64_of(&x["interval"]);
        check(format!("state {s:?} vs {x}"), same);
    }
    // Cones.
    let ec = e["cones"].as_array().unwrap();
    check(format!("{} cones vs {}", a.cones.len(), ec.len()), a.cones.len() == ec.len());
    for (c, x) in a.cones.iter().zip(ec) {
        let at = format!("cone b{} n{}", c.block, c.node);
        let mut f = |what: &str, same: bool| {
            if !same {
                d.push(format!("{at}: {what}"));
            }
        };
        f("block/node", c.block as u64 == u64_of(&x["block"]) && c.node as u64 == u64_of(&x["node"]));
        f("nodes", c.nodes.iter().map(|&n| n as u64).collect::<Vec<_>>() == u64s(&x["nodes"]));
        let mine: Vec<String> = c.leaves.iter().map(leaf_str).collect();
        let theirs: Vec<String> = x["leaves"].as_array().unwrap().iter().map(|l| l.as_str().unwrap().to_string()).collect();
        let mut ms = mine.clone();
        let mut ts = theirs.clone();
        ms.sort();
        ts.sort();
        f(&format!("leaves {mine:?} vs {theirs:?}"), ms == ts);
        f(&format!("leaf order {mine:?} vs {theirs:?}"), mine == theirs);
        f(&format!("whole {:?} vs {}", c.whole, x["whole"]), c.whole == cost_of(&x["whole"]));
        f(&format!("tiles {} vs {}", c.tiles, x["tiles"]), c.tiles == u64_of(&x["tiles"]));
        f(&format!("tile {:?} vs {}", c.tile, x["tile"]), c.tile == cost_of(&x["tile"]));
        f("tile_opened_bytes", c.tile_opened_bytes == u64_of(&x["tile_opened_bytes"]));
        f("operands", c.operands == u64_of(&x["operands"]));
        f("h_reductions", c.h_reductions.iter().map(|&n| n as u64).collect::<Vec<_>>() == u64s(&x["h_reductions"]));
        f(
            &format!("chunk {:?} vs {}", c.chunk, x["chunk"]),
            match (&c.chunk, x["chunk"].is_null()) {
                (None, true) => true,
                (Some(k), false) => *k == cost_of(&x["chunk"]),
                _ => false,
            },
        );
        f(
            "chunk_opened_bytes",
            match (c.chunk_opened_bytes, x["chunk_opened_bytes"].is_null()) {
                (None, true) => true,
                (Some(k), false) => k == u64_of(&x["chunk_opened_bytes"]),
                _ => false,
            },
        );
    }
    // Every node's cost and interval.
    let en = e["node_costs"].as_array().unwrap();
    for (b, (mine, theirs)) in a.node_costs.iter().zip(en).enumerate() {
        for (i, (m, t)) in mine.iter().zip(theirs.as_array().unwrap()).enumerate() {
            if *m != cost_of(t) {
                d.push(format!("node cost b{b} n{i}: {m:?} vs {t}"));
            }
        }
    }
    let ei = e["intervals"].as_array().unwrap();
    for (b, (mine, theirs)) in a.intervals.iter().zip(ei).enumerate() {
        for (i, (m, t)) in mine.iter().zip(theirs.as_array().unwrap()).enumerate() {
            let t = t.as_array().unwrap();
            if (m.lo, m.hi) != (int_of(&t[0]), int_of(&t[1])) {
                d.push(format!("interval b{b} n{i}: [{}, {}] vs {t:?}", m.lo, m.hi));
            }
        }
    }
    d
}

#[test]
fn admission_vectors() {
    let v = read_json(&vectors_dir().join("admission.json"));
    assert_eq!(v["format"], "palw-tir-v1/admission-vectors/1");
    let (mut total, mut ok) = (0, 0);
    let mut fails = Vec::new();
    let mut leaf_order_only = 0;
    for c in v["cases"].as_array().unwrap() {
        total += 1;
        let name = c["name"].as_str().unwrap();
        let bytes = hex_decode(c["program_borsh_hex"].as_str().unwrap());
        let inputs = inputs_of(&c["inputs"]);
        let r = admit(&bytes, &inputs);
        match (c["expect"].as_str().unwrap(), &r) {
            ("admitted", Ok(a)) => {
                let d = compare(a, &c["admission"]);
                let real: Vec<&String> = d.iter().filter(|x| !x.contains(": leaf order ")).collect();
                if d.len() > real.len() {
                    leaf_order_only += 1;
                }
                if real.is_empty() {
                    ok += 1;
                } else {
                    fails.push(format!(
                        "{name}: {} differences: {}",
                        real.len(),
                        real.iter().take(4).map(|s| s.as_str()).collect::<Vec<_>>().join(" | ")
                    ));
                }
            }
            ("refused", Err(e)) => {
                let x = &c["refusal"];
                let same = match (x["kind"].as_str().unwrap(), e) {
                    ("exceeds", AdmitError::Exceeds { limit, value, cap, .. }) => {
                        *limit == x["limit"].as_str().unwrap() && *value == u64_of(&x["value"]) && *cap == u64_of(&x["cap"])
                    }
                    ("program", AdmitError::Program(t)) => t.class.name() == x["class"].as_str().unwrap(),
                    ("inputs", AdmitError::Inputs(_)) => true,
                    _ => false,
                };
                if same {
                    ok += 1;
                } else {
                    fails.push(format!("{name}: expected {x}, got {e:?}"));
                }
            }
            (want, got) => fails.push(format!(
                "{name}: expected {want}, got {}",
                match got {
                    Ok(_) => "admitted".to_string(),
                    Err(e) => format!("{e:?}"),
                }
            )),
        }
    }
    println!("admission vectors: {ok}/{total} reproduced ({leaf_order_only} admitted cases list some cone's leaves in another order)");
    for f in &fails {
        println!("  FAIL {f}");
    }
    assert!(fails.is_empty(), "{} admission vectors not reproduced", fails.len());
}
