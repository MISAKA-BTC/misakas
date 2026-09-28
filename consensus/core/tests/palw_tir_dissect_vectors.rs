//! **Spec 04b §9.5.8: the history dissection's golden vectors** (`consensus-vectors/tir-v1/dissect/`).
//!
//! Regenerated from the court fixture's job over every corpus model with a dissected leaf, and
//! compared byte for byte with the files; `TIR_BLESS=1` rewrites them, which is a change of the
//! semantics and is reviewed as one. Each case is also checked against the court: the finalize
//! reproduces the committed tile, the cut folds to the totals, and the bottom finds no fault.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use std::path::PathBuf;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_root_claim_v1, check_tir_dissect_bottom_v1,
    check_tir_root_claim_v1, tir_root_claim_finalizes_to_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_DISSECT_OBJECT_VERSION_V1, PalwTirDissectChoiceV1, PalwTirDissectPhaseV1, PalwTirFoldV1, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
use serde_json::{Value, json};

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);

fn s<T: ToString>(v: T) -> Value {
    Value::String(v.to_string())
}

fn list<T: ToString + Copy>(v: &[T]) -> Value {
    Value::Array(v.iter().map(|x| s(*x)).collect())
}

fn lists<T: ToString + Copy>(v: &[Vec<T>]) -> Value {
    Value::Array(v.iter().map(|x| list(x)).collect())
}

/// The vector file of one fixture, or `None` for a model with no dissected leaf.
fn vectors(f: &Fixture) -> Option<Value> {
    let intervals = f.intervals.as_ref()?;
    let x = f.honest();
    let store = Store { f, x: &x };
    let positions = PREFILL + DECODE - 1;
    let mut cases = Vec::new();
    for (i, leaf) in f.leaves.iter().enumerate() {
        let Some(site) = palw_tir_dissect_site_v1(&f.space, intervals, leaf) else { continue };
        // The last two positions (the widest histories) keep the files reviewable.
        if leaf.position + 2 < positions {
            continue;
        }
        let PalwTirLeafKindV1::Commit { occurrence, node, first_element, .. } = leaf.kind else {
            unreachable!("a dissected leaf commits")
        };
        let index = i as u64;
        let root = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the honest root claim");
        assert_eq!(check_tir_root_claim_v1(&root, index, &RULES).as_ref(), Ok(&site), "{}: leaf {i} is admitted", f.name);
        let finalize =
            tir_root_claim_finalizes_to_v1(&x.binding, index, &root.elements, &root.totals, &store, &RULES).expect("finalizes");
        assert_eq!(finalize, f.values[i], "{}: leaf {i}: the finalize is the committed tile", f.name);
        // The first cut at arity 2, then the last child all the way down to its bottom.
        let mut phase = PalwTirDissectPhaseV1::open(SESSION, index, &site, &root, 2, 0, 10).expect("opens");
        let mut cut = Vec::new();
        let mut daa = 1;
        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("a round");
            let ranges = phase.child_ranges();
            if phase.round() == 0 {
                for ((first, count), child) in ranges.iter().zip(&round.children) {
                    let from = first * site.tile_positions as u64;
                    let to = ((first + count) * site.tile_positions as u64).min(site.history_positions as u64);
                    cut.push(json!({
                        "tiles": [s(first), s(count)],
                        "positions": [s(from), s(to)],
                        "partials": lists(&child.partials),
                    }));
                }
            }
            phase.apply_round(&round, daa, 10).expect("an honest round folds");
            let last = (ranges.len() - 1) as u8;
            let choice = PalwTirDissectChoiceV1 {
                version: PALW_TIR_DISSECT_OBJECT_VERSION_V1,
                session_id: SESSION,
                round: phase.round(),
                child: last,
            };
            phase.apply_choice(&choice, daa + 1, 10).expect("a legal choice");
            daa += 2;
        }
        let (from, to) = phase.terminal_range().expect("one tile");
        let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("the bottom");
        assert_eq!(check_tir_dissect_bottom_v1(&phase, &bottom, index, &RULES), Err(PalwStepRefuteError::NoFaultFound));
        cases.push(json!({
            "name": format!("leaf {i}: pos {} occurrence {occurrence} node {node} from element {first_element}", leaf.position),
            "leaf": {
                "index": s(i),
                "pos": s(leaf.position),
                "occurrence": s(occurrence),
                "node": s(node),
                "first_element": s(first_element),
                "values": s(leaf.value_count),
            },
            "site": {
                "reductions": list(&site.reductions),
                "folds": site.folds.iter().map(|f| match f { PalwTirFoldV1::Sum => "sum", PalwTirFoldV1::Max => "max" }).collect::<Vec<_>>(),
                "bounds": site.bounds.iter().map(|b| json!({ "lo": s(b.lo), "hi": s(b.hi) })).collect::<Vec<_>>(),
                "h": s(site.history_positions),
                "h_tile": s(site.tile_positions),
                "counts": list(&site.counts),
            },
            "elements": lists(&root.elements),
            "totals": lists(&root.totals.partials),
            "finalize": list(&finalize),
            "cut": cut,
            "bottom": {
                "positions": [s(from), s(to)],
                "partials": lists(&phase.claim().partials),
            },
        }));
    }
    if cases.is_empty() {
        return None;
    }
    Some(json!({
        "format": "palw-tir-v1/dissect-vectors/1",
        "spec": "docs/spec/palw/04b-tensor-ir.md §9.5",
        "name": f.name,
        "program": format!("programs/{}.json", f.name),
        "job": {
            "prefill": s(PREFILL),
            "decode": s(DECODE),
            "prompt": list(&f.prompt),
            "generated": list(&f.generated),
            "h_tile": s(f.class.layout.h_tile),
        },
        "cases": cases,
    }))
}

#[test]
fn the_dissect_vectors_are_the_court_s() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../consensus-vectors/tir-v1/dissect");
    let bless = std::env::var("TIR_BLESS").is_ok_and(|v| v == "1");
    if bless {
        std::fs::create_dir_all(&dir).expect("the vector directory");
    }
    let mut written = Vec::new();
    for f in fixtures() {
        let Some(v) = vectors(&f) else { continue };
        let bytes = serde_json::to_string_pretty(&v).expect("json") + "\n";
        let path = dir.join(format!("{}.json", f.name));
        if bless {
            std::fs::write(&path, &bytes).expect("written");
        } else {
            let on_disk = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e} (TIR_BLESS=1 writes it)", path.display()));
            assert!(on_disk == bytes, "{} differs from the court's (TIR_BLESS=1 rewrites it)", path.display());
        }
        written.push((f.name.clone(), v["cases"].as_array().map(Vec::len).unwrap_or(0)));
    }
    eprintln!("dissect vectors: {written:?}");
    assert!(written.len() >= 2, "the dense and the sliding + global models have dissected leaves");
    let on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("the vector directory")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert_eq!(on_disk.len(), written.len(), "no stale file: {on_disk:?}");
}
