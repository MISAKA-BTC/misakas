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
    palw_tir_dissect_value_bound_v1,
};
use kaspa_consensus_core::palw_tir_step_v1::PalwTirLeafKindV1;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, TirProgramV1};
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// The tile length the layout declares for committed node `node` of block `block`.
fn commit_tile_len(program: &TirProgramV1, tiles: &[u32], block: u8, node: u16) -> u32 {
    let index = program
        .blocks
        .iter()
        .enumerate()
        .flat_map(|(bi, b)| b.nodes.iter().enumerate().filter(|(_, n)| n.commit).map(move |(ni, _)| (bi, ni)))
        .position(|(bi, ni)| bi == block as usize && ni == node as usize)
        .expect("a committed node");
    tiles[index]
}

type Body = fn(&mut misaka_palw_tir::builder::BlockBuilder<'_>, Ref) -> Ref;

/// A one-layer program over a 4-lane history of clamped rows, whose layer commits `body`'s cone.
fn synthetic_program(name: &str, body: Body) -> (String, TirProgramV1, MapParams, Vec<u32>) {
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let embed = pb.param("embed", DType::I8, &[8, 4], false);
    let head = pb.param("head", DType::I8, &[8, 4], false);
    let hist = pb.hist_state("rows", DType::I32, &[4], 64, true);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let x = b.gather(embed, Ref::Input(0), 0, 0);
        let x = b.cast(x, DType::I32);
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("layer", carry.clone());
        let row = b.clamp(Ref::CarryIn(0), -64, 64, DType::I32);
        let rows = b.hist_append(hist, row);
        let out = body(&mut b, rows);
        let out = b.clamp(out, -1000, 1000, DType::I32);
        b.finish(&[out])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let x = b.reshape_fixed(Ref::CarryIn(0), &[4, 1]);
        let l = b.matmul(head, x, DType::I64);
        let l = b.clamp(l, i32::MIN as i64, i32::MAX as i64, DType::I32);
        let l = b.reshape_fixed(l, &[8]);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let program = pb.finish(pre, vec![layer], post, logits);
    let mut params = MapParams::default();
    let fill = |n: usize, k: i128| (0..n as i128).map(|i| ((i * 37 + k) % 255) - 127).collect::<Vec<_>>();
    params.tensors.insert((0, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 5)).unwrap());
    params.tensors.insert((1, None), Tensor::new(DType::I8, vec![8, 4], fill(32, 17)).unwrap());
    (name.to_string(), program, params, vec![1, 6, 3, 2, 5])
}

/// **The two programs ref2's findings H1 and H2 are about**, carried inline in their vector files.
///
/// * `h1-concat-maxima`: two maxima over the history, concatenated and committed at the fixture
///   layout's 6-lane tile — the second tile reads only the second maximum, so its root claim's list
///   for the first is EMPTY.
/// * `h2-matmul-const-first`: a maximum `m1` over the history, the history shifted by it and
///   transposed, contracted with a constant `[2, 4]` (a `MatMul` whose first operand is a leaf), and a
///   maximum of that over the history — whose closure reads all four elements of `m1`.
fn synthetic() -> Vec<(String, TirProgramV1, MapParams, Vec<u32>)> {
    fn h1(b: &mut misaka_palw_tir::builder::BlockBuilder<'_>, rows: Ref) -> Ref {
        let m1 = b.reduce_max(rows, 0); // [1, 4]
        let seven = b.c(DType::I32, 7);
        let shifted = b.add(rows, seven, DType::I32);
        let m2 = b.reduce_max(shifted, 0); // [1, 4]
        let cat = b.concat(&[m1, m2], 1); // [1, 8]
        let cat = b.commit(cat);
        let half = b.slice(cat, 1, 0, 4);
        b.reshape_fixed(half, &[4])
    }
    fn h2(b: &mut misaka_palw_tir::builder::BlockBuilder<'_>, rows: Ref) -> Ref {
        let m1 = b.reduce_max(rows, 0); // [1, 4]
        let d = b.sub(rows, m1, DType::I32); // [H, 4]
        let xt = b.transpose(d, &[1, 0]); // [4, H]
        let a = b.pb.konst(DType::I32, &[2, 4], &[1, -2, 3, -1, 2, 1, -3, 2]);
        let mm = b.matmul(a, xt, DType::I64); // [2, H]: the first operand a constant
        let r2 = b.reduce_max(mm, 1); // [2, 1]
        let r2 = b.clamp(r2, -100_000, 100_000, DType::I32);
        let r2 = b.commit(r2);
        let r2 = b.reshape_fixed(r2, &[2]);
        let m = b.reshape_fixed(m1, &[4]);
        let head = b.slice(m, 0, 0, 2);
        let tail = b.slice(m, 0, 2, 2);
        let head = b.add(head, r2, DType::I32);
        b.concat(&[head, tail], 0)
    }
    vec![synthetic_program("h1-concat-maxima", h1), synthetic_program("h2-matmul-const-first", h2)]
}

/// The vector file of one fixture, or `None` for a model with no dissected leaf; `inline` carries a
/// synthetic program's params (its bytes are the fixture's).
fn vectors(f: &Fixture, inline: Option<&MapParams>) -> Option<Value> {
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
        let PalwTirLeafKindV1::Commit { occurrence, block, node, first_element, .. } = leaf.kind else {
            unreachable!("a dissected leaf commits")
        };
        let index = i as u64;
        let root = build_tir_root_claim_v1(&x.binding, index, &store, &RULES).expect("the honest root claim");
        // O-5's bound on the values a claim carries, at the node's tile length: never below the
        // closure the claim actually carries (ref2's H2).
        let program = &f.space.program;
        let tile_len = commit_tile_len(program, &f.class.layout.commit_tiles, block, node);
        let value_bound = palw_tir_dissect_value_bound_v1(program, &program.blocks[block as usize], node, tile_len);
        let carried: u64 = root.elements.iter().map(|e| e.len() as u64).sum();
        assert!(carried <= value_bound, "{}: leaf {i}: the claim carries {carried} values, the bound is {value_bound}", f.name);
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
            "value_bound": s(value_bound),
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
    let (program, inline) = match inline {
        None => (json!(format!("programs/{}.json", f.name)), Value::Null),
        Some(params) => {
            let mut rows: Vec<Value> = params
                .tensors
                .iter()
                .map(|((j, layer), t)| json!({ "param": s(j), "layer": layer.map(s), "le_hex": hex(&t.to_le_bytes()) }))
                .collect();
            rows.sort_by_key(|v| v.to_string());
            (Value::Null, json!({ "program_borsh_hex": hex(&f.space.program.encode()), "params": rows }))
        }
    };
    Some(json!({
        "format": "palw-tir-v1/dissect-vectors/1",
        "spec": "docs/spec/palw/04b-tensor-ir.md §9.5",
        "name": f.name,
        "program": program,
        "inline": inline,
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
    let corpus = fixtures().into_iter().map(|f| (f, None));
    let synthetic = synthetic().into_iter().map(|(name, program, params, tokens)| {
        let f = fixture(name, program, params.clone(), tokens);
        assert!(f.intervals.is_some(), "{}: the range analysis proves it", f.name);
        (f, Some(params))
    });
    for (f, inline) in corpus.chain(synthetic) {
        let Some(v) = vectors(&f, inline.as_ref()) else { continue };
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
    // ref2's H1 and H2, pinned: an admitted claim with an EMPTY list, and a `V` through a constant
    // `MatMul` operand equal to the closure it bounds.
    let read = |name: &str| -> Value {
        serde_json::from_str(&std::fs::read_to_string(dir.join(format!("{name}.json"))).expect("the file")).expect("json")
    };
    let h1 = read("h1-concat-maxima");
    assert!(
        h1["cases"].as_array().unwrap().iter().any(|c| c["elements"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.as_array().unwrap().is_empty())),
        "H1: some admitted root claim carries an empty list"
    );
    let h2 = read("h2-matmul-const-first");
    assert!(
        h2["cases"].as_array().unwrap().iter().any(|c| {
            let carried: usize = c["elements"].as_array().unwrap().iter().map(|e| e.as_array().unwrap().len()).sum();
            c["site"]["reductions"].as_array().unwrap().len() == 2 && c["value_bound"] == s(carried) && carried == 6
        }),
        "H2: V counts d · K through the constant operand and equals the closure (6)"
    );
    assert!(written.len() >= 4, "the dense and the sliding + global models, and ref2's H1 and H2 programs");
    let on_disk: Vec<String> = std::fs::read_dir(&dir)
        .expect("the vector directory")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    assert_eq!(on_disk.len(), written.len(), "no stale file: {on_disk:?}");
}
