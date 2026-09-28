//! **The demand-evaluation golden vectors** (`consensus-vectors/tir-v1/demand/`, spec 04b §9.4).
//!
//! One file per program vector. Each file names its program vector (whose `params` and `steps` —
//! tokens and every commit point of every position — are the source's committed data), lists the
//! `Fixed` values the source SUPPLIES (at every even position; every other position answers
//! `Replay`), and pins, for each case — a target, a demanded element list and the work limits — the
//! values, the work charged and the set of source requests, or the refusal class.
//!
//! The source of every case is built from the files alone (the program vector and this file), so an
//! independent implementation reproduces the vectors from §9.4 and the files, with no interpreter of
//! its own; the interpreter is used here only to derive the supplied `Fixed` values and to check that
//! the program vector's commits are its run.
//!
//! `cargo test -p misaka-palw-tir --test demand_vectors` regenerates every file and requires identical
//! bytes; `TIR_BLESS=1` rewrites them, which is a change of the semantics and is reviewed as one.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use misaka_palw_tir::demand::{
    DemandContext, DemandError, DemandLimits, DemandRequest, DemandTarget, MapSource, MapSourceRequest, eval_demanded,
    hist_row_node_v1, state_occurrence_v1,
};
use misaka_palw_tir::program::StateKind;
use misaka_palw_tir::{Interpreter, MapParams, Prim, Ref, RunState, Tensor, TirProgramV1};
use serde::Serialize;

const FORMAT: &str = "palw-tir-v1/demand-vectors/1";
const SPEC: &str = "docs/spec/palw/04b-tensor-ir.md §9.4";

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

// ---- JSON shapes (field order is the file's key order) --------------------------------------------

#[derive(Serialize)]
struct TensorJson {
    dtype: String,
    shape: Vec<u64>,
    data: Vec<String>,
}

#[derive(Serialize)]
struct StateJson {
    pos: String,
    state: String,
    layer: Option<String>,
    value: TensorJson,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum TargetJson {
    Node { pos: String, occurrence: String, node: String },
    StateAfter { pos: String, state: String, layer: Option<String> },
}

#[derive(Serialize)]
struct LimitsJson {
    max_elements: String,
    max_terms: String,
}

#[derive(Serialize)]
struct WorkJson {
    elements: String,
    terms: String,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum RequestKeyJson {
    Node { pos: String, occurrence: String, node: String },
    Param { param: String, layer: Option<String> },
    State { pos: String, state: String, layer: Option<String> },
    HistRow { pos: String, state: String, layer: Option<String>, row_pos: String },
    Token { pos: String },
}

#[derive(Serialize)]
struct RequestJson {
    #[serde(flatten)]
    key: RequestKeyJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    indices: Option<String>,
}

#[derive(Serialize)]
struct ExpectJson {
    values: Vec<String>,
    work: WorkJson,
    requests: Vec<RequestJson>,
}

#[derive(Serialize)]
struct CaseJson {
    name: String,
    target: TargetJson,
    elements: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    states: Option<Vec<StateJson>>,
    limits: LimitsJson,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect: Option<ExpectJson>,
    #[serde(skip_serializing_if = "Option::is_none")]
    expect_error: Option<String>,
}

#[derive(Serialize)]
struct FileJson {
    format: String,
    spec: String,
    name: String,
    program: String,
    states: Vec<StateJson>,
    cases: Vec<CaseJson>,
}

fn s<T: ToString>(v: T) -> String {
    v.to_string()
}

fn tj(t: &Tensor) -> TensorJson {
    TensorJson {
        dtype: t.dtype.name().into(),
        shape: t.shape.iter().map(|d| *d as u64).collect(),
        data: t.data.iter().map(s).collect(),
    }
}

/// `"0-3,7,9-10"`: sorted indices as inclusive runs.
fn ranges(indices: &[usize]) -> String {
    let mut out = Vec::new();
    let mut k = 0;
    while k < indices.len() {
        let mut e = k;
        while e + 1 < indices.len() && indices[e + 1] == indices[e] + 1 {
            e += 1;
        }
        out.push(if e == k { s(indices[k]) } else { format!("{}-{}", indices[k], indices[e]) });
        k = e + 1;
    }
    out.join(",")
}

/// The set of requests, grouped by everything but the element index.
fn requests_json(requests: &[MapSourceRequest]) -> Vec<RequestJson> {
    let set: BTreeSet<MapSourceRequest> = requests.iter().copied().collect();
    let mut groups: Vec<(RequestKeyJson, Vec<usize>, Option<usize>)> = Vec::new();
    let mut last: Option<MapSourceRequest> = None;
    for r in set {
        let (key, index) = match r {
            MapSourceRequest::Node { ctx, node, index } => {
                (RequestKeyJson::Node { pos: s(ctx.pos), occurrence: s(ctx.occurrence), node: s(node) }, Some(index))
            }
            MapSourceRequest::Param { param, layer, index } => {
                (RequestKeyJson::Param { param: s(param), layer: layer.map(s) }, Some(index))
            }
            MapSourceRequest::State { pos, state, layer, index } => {
                (RequestKeyJson::State { pos: s(pos), state: s(state), layer: layer.map(s) }, Some(index))
            }
            MapSourceRequest::HistRow { pos, state, layer, row_pos, index } => {
                (RequestKeyJson::HistRow { pos: s(pos), state: s(state), layer: layer.map(s), row_pos: s(row_pos) }, Some(index))
            }
            MapSourceRequest::Token { pos } => (RequestKeyJson::Token { pos: s(pos) }, None),
        };
        let same_key = |a: &MapSourceRequest, b: &MapSourceRequest| -> bool {
            use MapSourceRequest::*;
            match (a, b) {
                (Node { ctx: c1, node: n1, .. }, Node { ctx: c2, node: n2, .. }) => c1 == c2 && n1 == n2,
                (Param { param: p1, layer: l1, .. }, Param { param: p2, layer: l2, .. }) => p1 == p2 && l1 == l2,
                (State { pos: p1, state: s1, layer: l1, .. }, State { pos: p2, state: s2, layer: l2, .. }) => {
                    p1 == p2 && s1 == s2 && l1 == l2
                }
                (
                    HistRow { pos: p1, state: s1, layer: l1, row_pos: r1, .. },
                    HistRow { pos: p2, state: s2, layer: l2, row_pos: r2, .. },
                ) => p1 == p2 && s1 == s2 && l1 == l2 && r1 == r2,
                _ => false,
            }
        };
        match (last, index) {
            (Some(prev), Some(i)) if same_key(&prev, &r) => groups.last_mut().expect("a group").1.push(i),
            _ => groups.push((key, index.into_iter().collect(), index)),
        }
        last = Some(r);
    }
    groups.into_iter().map(|(key, indices, first)| RequestJson { key, indices: first.map(|_| ranges(&indices)) }).collect()
}

// ---- the program vectors, and the source built from the files ------------------------------------

struct ProgramVector {
    name: String,
    program: TirProgramV1,
    params: MapParams,
    /// `(pos, token)` and every commit `(block, layer, node) → value` per position.
    tokens: Vec<u32>,
    commits: Vec<BTreeMap<(u8, Option<u16>, u16), Tensor>>,
}

fn tensor_of(v: &serde_json::Value) -> Tensor {
    let dtype = misaka_palw_tir::DType::from_name(v["dtype"].as_str().unwrap()).expect("dtype");
    let shape: Vec<usize> = v["shape"].as_array().unwrap().iter().map(|d| d.as_u64().unwrap() as usize).collect();
    let data: Vec<i128> = v["data"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().parse().unwrap()).collect();
    Tensor::new(dtype, shape, data).expect("tensor")
}

fn program_vectors() -> Vec<ProgramVector> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(root().join("programs"))
        .expect("vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            let mut params = MapParams::default();
            for p in v["params"].as_array().unwrap() {
                let j = p["param"].as_u64().unwrap() as u16;
                let layer = p["layer"].as_u64().map(|l| l as u16);
                let d = &program.params[j as usize];
                let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                params
                    .tensors
                    .insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &unhex(p["le_hex"].as_str().unwrap())).unwrap());
            }
            let mut tokens = Vec::new();
            let mut commits = Vec::new();
            for step in v["steps"].as_array().unwrap() {
                tokens.push(step["token"].as_u64().unwrap() as u32);
                let mut c = BTreeMap::new();
                for x in step["commits"].as_array().unwrap() {
                    let block = x["block"].as_u64().unwrap() as u8;
                    let layer = x["layer"].as_u64().map(|l| l as u16);
                    c.insert((block, layer, x["node"].as_u64().unwrap() as u16), tensor_of(&x["value"]));
                }
                commits.push(c);
            }
            ProgramVector { name: v["name"].as_str().unwrap().into(), program, params, tokens, commits }
        })
        .collect()
}

/// The source §9.4's vector paragraph defines: committed nodes and tokens from the program vector's
/// steps, params from its params, history rows from the committed node the row is, and `Fixed`
/// values only where `states` lists them.
fn source(pv: &ProgramVector, states: &[(u32, u16, Option<u16>, Tensor)]) -> MapSource {
    let occ = pv.program.occurrences();
    let mut src = MapSource {
        tokens: pv.tokens.iter().enumerate().map(|(p, t)| (p as u32, *t)).collect(),
        params: pv.params.tensors.iter().map(|(k, t)| (*k, t.data.clone())).collect(),
        ..Default::default()
    };
    for (pos, commits) in pv.commits.iter().enumerate() {
        for (o, (block, layer)) in occ.iter().enumerate() {
            for ((b, l, n), t) in commits {
                if b == block && l == layer {
                    src.nodes.insert((DemandContext { pos: pos as u32, occurrence: o as u16 }, *n), t.data.clone());
                }
            }
        }
    }
    for (j, st) in pv.program.states.iter().enumerate() {
        if !matches!(st.kind, StateKind::Hist { .. }) {
            continue;
        }
        let layers: Vec<Option<u16>> =
            if st.per_layer { (0..pv.program.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
        for layer in layers {
            for r in 0..pv.tokens.len() as u32 {
                if let Some((ctx, node)) = hist_row_node_v1(&pv.program, j as u16, layer, r)
                    && let Some(v) = src.nodes.get(&(ctx, node))
                {
                    src.hist_rows.insert((j as u16, layer, r), v.clone());
                }
            }
        }
    }
    for (pos, j, l, t) in states {
        src.states.insert((*pos, *j, *l), t.data.clone());
    }
    src
}

/// Every `Fixed` instance a run holds: `(state, layer)` for each layer whose block references it.
fn fixed_instances(p: &TirProgramV1) -> Vec<(u16, Option<u16>)> {
    let mut out = Vec::new();
    for (j, st) in p.states.iter().enumerate() {
        if !matches!(st.kind, StateKind::Fixed { .. }) {
            continue;
        }
        let j = j as u16;
        let refs = |block: u8| {
            p.blocks[block as usize].nodes.iter().any(|n| {
                n.inputs.contains(&Ref::State(j))
                    || matches!(n.prim, Prim::StateWrite { state } | Prim::HistAppend { state } if state == j)
            })
        };
        if st.per_layer {
            for (l, b) in p.schedule.layers.iter().enumerate() {
                if refs(*b) {
                    out.push((j, Some(l as u16)));
                }
            }
        } else {
            out.push((j, None));
        }
    }
    out
}

fn pick(n: usize) -> Vec<usize> {
    if n <= 6 {
        return (0..n).collect();
    }
    let set: BTreeSet<usize> = [0, 1, n / 2, n - 2, n - 1].into_iter().collect();
    set.into_iter().collect()
}

const UNLIMITED: DemandLimits = DemandLimits::UNLIMITED;

fn limits_json(l: &DemandLimits) -> LimitsJson {
    LimitsJson { max_elements: s(l.max_elements), max_terms: s(l.max_terms) }
}

fn run_case(
    pv: &ProgramVector,
    states: &[(u32, u16, Option<u16>, Tensor)],
    name: String,
    target: DemandTarget,
    elements: Vec<usize>,
    limits: DemandLimits,
    own_states: bool,
) -> (CaseJson, Option<(u64, u64)>) {
    let info = misaka_palw_tir::validate::validate(&pv.program).expect("valid");
    let mut src = source(pv, states);
    let request = DemandRequest { target, elements: &elements };
    let outcome = eval_demanded(&pv.program, &info, &request, &mut src, &limits);
    let target = match target {
        DemandTarget::Node { ctx, node } => TargetJson::Node { pos: s(ctx.pos), occurrence: s(ctx.occurrence), node: s(node) },
        DemandTarget::StateAfter { pos, state, layer } => TargetJson::StateAfter { pos: s(pos), state: s(state), layer: layer.map(s) },
    };
    let states_json = own_states
        .then(|| states.iter().map(|(p, j, l, t)| StateJson { pos: s(p), state: s(j), layer: l.map(s), value: tj(t) }).collect());
    let (expect, expect_error, work) = match outcome {
        Ok((values, work)) => (
            Some(ExpectJson {
                values: values.iter().map(s).collect(),
                work: WorkJson { elements: s(work.elements), terms: s(work.terms) },
                requests: requests_json(&src.requests),
            }),
            None,
            Some((work.elements, work.terms)),
        ),
        Err(DemandError::WorkLimit(_)) => (None, Some("WorkLimit".to_string()), None),
        Err(DemandError::Tir(e)) => (None, Some(format!("{:?}", e.kind)), None),
    };
    (
        CaseJson {
            name,
            target,
            elements: elements.iter().map(s).collect(),
            states: states_json,
            limits: limits_json(&limits),
            expect,
            expect_error,
        },
        work,
    )
}

fn file_for(pv: &ProgramVector) -> String {
    let p = &pv.program;
    let interp = Interpreter::new(p).expect("valid");
    // The run: the supplied Fixed values, and a check that the vector's commits are its commits.
    let mut state = RunState::default();
    let mut before = Vec::new();
    for (pos, token) in pv.tokens.iter().enumerate() {
        before.push(state.clone());
        let step = interp.step(&pv.params, &mut state, *token).expect("the vector's run");
        for c in step.commits {
            assert_eq!(pv.commits[pos][&(c.block, c.layer, c.node)], c.value, "{}: the program vector is its run", pv.name);
        }
    }
    let instances = fixed_instances(p);
    let mut states: Vec<(u32, u16, Option<u16>, Tensor)> = Vec::new();
    for (pos, rs) in before.iter().enumerate() {
        if pos % 2 != 0 {
            continue;
        }
        for (j, l) in &instances {
            let st = &p.states[*j as usize];
            let shape: Vec<usize> = st.shape.iter().map(|d| *d as usize).collect();
            let t = rs.fixed.get(&(*j, *l)).cloned().unwrap_or_else(|| Tensor::zeros(st.dtype, &shape));
            states.push((pos as u32, *j, *l, t));
        }
    }
    let occurrences = p.occurrences();
    let positions = pv.tokens.len() as u32;
    let mut cases = Vec::new();
    let mut heaviest: Option<(usize, u64, u64)> = None;
    let mut push = |cases: &mut Vec<CaseJson>, (case, work): (CaseJson, Option<(u64, u64)>)| {
        if let Some((e, t)) = work
            && heaviest.is_none_or(|(_, _, ht)| t > ht)
        {
            heaviest = Some((cases.len(), e, t));
        }
        cases.push(case);
    };
    // Every commit point of every position, a spread of its elements.
    for pos in 0..positions {
        for (o, (block, _)) in occurrences.iter().enumerate() {
            let b = &p.blocks[*block as usize];
            for (n, node) in b.nodes.iter().enumerate() {
                if !node.commit {
                    continue;
                }
                let count = pv.commits[pos as usize][&(*block, occurrences[o].1, n as u16)].data.len();
                let ctx = DemandContext { pos, occurrence: o as u16 };
                let target = DemandTarget::Node { ctx, node: n as u16 };
                let case =
                    run_case(pv, &states, format!("commit pos {pos} occurrence {o} node {n}"), target, pick(count), UNLIMITED, false);
                push(&mut cases, case);
            }
        }
    }
    // Internal (uncommitted) nodes at the last position: every seventh.
    let last = positions - 1;
    for (o, (block, _)) in occurrences.iter().enumerate() {
        let b = &p.blocks[*block as usize];
        let window = interp.info.blocks[*block as usize].window;
        let h = window.map(|w| (last as usize + 1).min(w as usize)).unwrap_or(1);
        for (n, node) in b.nodes.iter().enumerate() {
            if node.commit || n % 7 != 3 {
                continue;
            }
            let count: usize = node.out.resolve(h).iter().product();
            let ctx = DemandContext { pos: last, occurrence: o as u16 };
            let target = DemandTarget::Node { ctx, node: n as u16 };
            let case =
                run_case(pv, &states, format!("internal pos {last} occurrence {o} node {n}"), target, pick(count), UNLIMITED, false);
            push(&mut cases, case);
        }
    }
    // Every Fixed instance after every position (odd positions replay from the even one before).
    for pos in 0..positions {
        for (j, l) in &instances {
            let n: usize = p.states[*j as usize].shape.iter().map(|d| *d as usize).product();
            let target = DemandTarget::StateAfter { pos, state: *j, layer: *l };
            let at = l.map_or("global".to_string(), |l| format!("layer {l}"));
            let case = run_case(pv, &states, format!("state {j} {at} after pos {pos}"), target, pick(n), UNLIMITED, false);
            push(&mut cases, case);
        }
    }
    // The work boundary: the heaviest case at exactly its work, and one short in each dimension.
    let (hi, we, wt) = heaviest.expect("a case with work");
    let base_target = match &cases[hi].target {
        TargetJson::Node { pos, occurrence, node } => DemandTarget::Node {
            ctx: DemandContext { pos: pos.parse().unwrap(), occurrence: occurrence.parse().unwrap() },
            node: node.parse().unwrap(),
        },
        TargetJson::StateAfter { pos, state, layer } => DemandTarget::StateAfter {
            pos: pos.parse().unwrap(),
            state: state.parse().unwrap(),
            layer: layer.as_ref().map(|l| l.parse().unwrap()),
        },
    };
    let base_elements: Vec<usize> = cases[hi].elements.iter().map(|e| e.parse().unwrap()).collect();
    let base_name = cases[hi].name.clone();
    for (label, limits) in [
        ("at exactly its work", DemandLimits { max_elements: we, max_terms: wt }),
        ("one element short", DemandLimits { max_elements: we - 1, max_terms: wt }),
        ("one term short", DemandLimits { max_elements: we, max_terms: wt.saturating_sub(1) }),
    ] {
        let case = run_case(pv, &states, format!("{base_name}, {label}"), base_target, base_elements.clone(), limits, false);
        cases.push(case.0);
    }
    // Refusals.
    let (b0, _) = occurrences[0];
    let pre_nodes = p.blocks[b0 as usize].nodes.len() as u16;
    let at = |pos: u32, occurrence: u16, node: u16| DemandTarget::Node { ctx: DemandContext { pos, occurrence }, node };
    let first_commit = p.blocks[b0 as usize].nodes.iter().position(|n| n.commit).expect("pre commits its carry") as u16;
    let refusals: Vec<(&str, DemandTarget, Vec<usize>)> = vec![
        ("an element outside the target", at(0, 0, first_commit), vec![usize::MAX / 4]),
        ("an occurrence the schedule does not have", at(0, occurrences.len() as u16, 0), vec![0]),
        ("a node the block does not have", at(0, 0, pre_nodes), vec![0]),
        ("a position at history_bound", at(p.history_bound, 0, first_commit), vec![0]),
        // The logits read the last layer's carry-out, a leaf the run never committed there.
        ("a position the run did not reach", at(positions, occurrences.len() as u16 - 1, p.logits), vec![0]),
    ];
    for (label, target, elements) in refusals {
        cases.push(run_case(pv, &states, format!("refused: {label}"), target, elements, UNLIMITED, false).0);
    }
    if let Some((j, l)) = instances
        .iter()
        .find(|(j, l)| misaka_palw_tir::demand::state_writer_v1(p, *j, *l).is_some() && state_occurrence_v1(p, *j, *l).is_some())
    {
        let target = DemandTarget::StateAfter { pos: 0, state: *j, layer: *l };
        let case =
            run_case(pv, &[], "refused: Replay at position 0 (the source supplies nothing)".into(), target, vec![0], UNLIMITED, true);
        cases.push(case.0);
    }
    if let Some(j) = p.states.iter().position(|st| matches!(st.kind, StateKind::Hist { .. })) {
        let layer = p.states[j].per_layer.then_some(0u16);
        let target = DemandTarget::StateAfter { pos: 0, state: j as u16, layer };
        cases.push(run_case(pv, &states, "refused: a Hist state named as Fixed".into(), target, vec![0], UNLIMITED, false).0);
    }
    let file = FileJson {
        format: FORMAT.into(),
        spec: SPEC.into(),
        name: pv.name.clone(),
        program: format!("programs/{}.json", pv.name),
        states: states.iter().map(|(pos, j, l, t)| StateJson { pos: s(pos), state: s(j), layer: l.map(s), value: tj(t) }).collect(),
        cases,
    };
    serde_json::to_string_pretty(&file).unwrap() + "\n"
}

#[test]
fn the_demand_vectors_are_the_evaluator() {
    let bless = std::env::var("TIR_BLESS").is_ok_and(|v| v == "1");
    let mut mismatched = Vec::new();
    let vectors = program_vectors();
    assert_eq!(vectors.len(), 7);
    let mut expected_files = Vec::new();
    for pv in &vectors {
        let rel = format!("demand/{}.json", pv.name);
        let content = file_for(pv);
        let path = root().join(&rel);
        expected_files.push(rel.clone());
        if bless {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &content).unwrap();
            continue;
        }
        match std::fs::read(&path) {
            Ok(bytes) if bytes == content.as_bytes() => {}
            _ => mismatched.push(rel),
        }
    }
    assert!(
        mismatched.is_empty(),
        "demand vectors differ from what this implementation produces: {mismatched:?} (TIR_BLESS=1 rewrites them)"
    );
    for e in std::fs::read_dir(root().join("demand")).unwrap() {
        let f = format!("demand/{}", e.unwrap().file_name().to_string_lossy());
        assert!(expected_files.contains(&f), "{f} is not generated by this test");
    }
}
