//! **The demand evaluator is the cone evaluator, element for element — and its replay is the run**
//! (RFC-0002 Phase F, step F5).
//!
//! 1. For every program of `consensus-vectors/tir-v1/programs` (the five corpus models and the two
//!    state programs), at every position, in every block occurrence, for EVERY node: `eval_demanded`
//!    over tiles of several lengths, single elements and shuffled element sets returns exactly the
//!    elements `Interpreter::eval_cone` computes for the whole node from the same supplied values —
//!    the honest step's commit points as leaves, its carry, its `Fixed` values and its history — and
//!    every leaf request names a committed node other than the target.
//! 2. With the `Fixed` values supplied only every `C` positions (a checkpoint) and REPLAYED in
//!    between, every node of every position still evaluates to the interpreter's value, and every
//!    state instance after every position is the interpreter's state, for several `C`.
//! 3. No native recursion: a 3,000-position replay and a 500-node chain evaluate on a thread with a
//!    256 KiB stack.
//! 4. The work charged is a function of the demanded set, not of the order it was asked in.
//! 5. The refusals: each an error, never a panic or an invented value.

use std::collections::BTreeMap;
use std::path::PathBuf;

use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::demand::{
    DemandContext, DemandError, DemandLimits, DemandRequest, DemandTarget, MapSource, MapSourceRequest, eval_demanded,
};
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_TOKEN, StateKind};
use misaka_palw_tir::{ConeEnv, DType, Interpreter, MapParams, Ref, RunState, Tensor, TensorType, TirErrorKind, TirProgramV1};

fn programs_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs")
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex")).collect()
}

/// A golden program: the program, its params and the token of each position.
fn load(path: &PathBuf) -> (String, TirProgramV1, MapParams, Vec<u32>) {
    let v: serde_json::Value = serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json");
    let program = TirProgramV1::decode_canonical(&unhex(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
    let mut params = MapParams::default();
    for p in v["params"].as_array().unwrap() {
        let j = p["param"].as_u64().unwrap() as u16;
        let layer = p["layer"].as_u64().map(|l| l as u16);
        let d = &program.params[j as usize];
        let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
        let t = Tensor::from_le_bytes(d.dtype, &shape, &unhex(p["le_hex"].as_str().unwrap())).expect("param bytes");
        params.tensors.insert((j, layer), t);
    }
    let tokens = v["steps"].as_array().unwrap().iter().map(|s| s["token"].as_u64().unwrap() as u32).collect();
    (v["name"].as_str().unwrap_or("?").to_string(), program, params, tokens)
}

fn all_programs() -> Vec<(String, TirProgramV1, MapParams, Vec<u32>)> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(programs_dir())
        .expect("vectors")
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(files.len() >= 7, "the five corpus models and the two state programs");
    files.iter().map(load).collect()
}

fn occurrence_of(program: &TirProgramV1, block: u8, layer: Option<u16>) -> u16 {
    match layer {
        Some(l) => l + 1,
        None if block == program.schedule.pre => 0,
        None => program.schedule.layers.len() as u16 + 1,
    }
}

/// Every `Fixed` and `Hist` instance of the program: `(state, layer)`.
fn instances(program: &TirProgramV1, hist: bool) -> Vec<(u16, Option<u16>)> {
    let mut out = Vec::new();
    for (j, s) in program.states.iter().enumerate() {
        if matches!(s.kind, StateKind::Hist { .. }) != hist {
            continue;
        }
        if s.per_layer {
            out.extend((0..program.schedule.layers.len() as u16).map(|l| (j as u16, Some(l))));
        } else {
            out.push((j as u16, None));
        }
    }
    out
}

/// An honest run: per position, the state at its start and after it, and every commit.
struct Run {
    before: Vec<RunState>,
    after: Vec<RunState>,
    commits: BTreeMap<(DemandContext, u16), Tensor>,
    tokens: Vec<u32>,
}

fn run(program: &TirProgramV1, params: &MapParams, tokens: &[u32]) -> Run {
    let interp = Interpreter::new(program).expect("valid");
    let mut state = RunState::default();
    let (mut before, mut after, mut commits) = (Vec::new(), Vec::new(), BTreeMap::new());
    for token in tokens {
        before.push(state.clone());
        let step = interp.step(params, &mut state, *token).expect("an honest step");
        for c in step.commits {
            let ctx = DemandContext { pos: step.pos, occurrence: occurrence_of(program, c.block, c.layer) };
            commits.insert((ctx, c.node), c.value);
        }
        after.push(state.clone());
    }
    Run { before, after, commits, tokens: tokens.to_vec() }
}

fn fixed_value(program: &TirProgramV1, rs: &RunState, j: u16, layer: Option<u16>) -> Vec<i128> {
    let s = &program.states[j as usize];
    rs.fixed.get(&(j, layer)).map(|t| t.data.clone()).unwrap_or_else(|| vec![0; s.shape.iter().map(|d| *d as usize).product()])
}

/// A source over the whole run, with the `Fixed` values supplied at the positions `supply` names.
fn source(program: &TirProgramV1, params: &MapParams, r: &Run, supply: &dyn Fn(u32) -> bool) -> MapSource {
    let mut s = MapSource {
        tokens: r.tokens.iter().enumerate().map(|(p, t)| (p as u32, *t)).collect(),
        nodes: r.commits.iter().map(|(k, t)| (*k, t.data.clone())).collect(),
        params: params.tensors.iter().map(|(k, t)| (*k, t.data.clone())).collect(),
        ..Default::default()
    };
    for (p, rs) in r.before.iter().enumerate() {
        if supply(p as u32) {
            for (j, l) in instances(program, false) {
                s.states.insert((p as u32, j, l), fixed_value(program, rs, j, l));
            }
        }
    }
    for (p, rs) in r.after.iter().enumerate() {
        for (j, l) in instances(program, true) {
            if let Some(rows) = rs.hist.get(&(j, l))
                && let Some(last) = rows.back()
            {
                s.hist_rows.insert((j, l, p as u32), last.data.clone());
            }
        }
    }
    s
}

/// The cone environment of `(block, layer)` at position `pos`, from the honest run.
fn cone_env(program: &TirProgramV1, r: &Run, pos: u32, block: u8, layer: Option<u16>, target: u16) -> ConeEnv {
    let occ = occurrence_of(program, block, layer);
    let ctx = DemandContext { pos, occurrence: occ };
    let supplied: BTreeMap<u16, Tensor> =
        r.commits.iter().filter(|((c, n), _)| *c == ctx && *n != target).map(|((_, n), t)| (*n, t.clone())).collect();
    let mut carry_in: BTreeMap<u8, Tensor> = BTreeMap::new();
    if occ > 0 {
        let (pb, _) = program.occurrences()[occ as usize - 1];
        let prev = DemandContext { pos, occurrence: occ - 1 };
        for (k, n) in program.blocks[pb as usize].carry_out.iter().enumerate() {
            carry_in.insert(k as u8, r.commits[&(prev, *n)].clone());
        }
    }
    let before = &r.before[pos as usize];
    let is_layer = layer.is_some();
    let (mut fixed, mut hist_prior) = (BTreeMap::new(), BTreeMap::new());
    for (j, s) in program.states.iter().enumerate() {
        if s.per_layer != is_layer {
            continue;
        }
        let key = (j as u16, if s.per_layer { layer } else { None });
        let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
        match s.kind {
            StateKind::Fixed { .. } => {
                fixed.insert(j as u16, before.fixed.get(&key).cloned().unwrap_or_else(|| Tensor::zeros(s.dtype, &shape)));
            }
            StateKind::Hist { .. } => {
                hist_prior.insert(j as u16, before.hist.get(&key).map(|rows| rows.iter().cloned().collect()).unwrap_or_default());
            }
        }
    }
    ConeEnv { token: Some(r.tokens[pos as usize]), pos, carry_in, fixed, hist_prior, supplied }
}

#[test]
fn eval_demanded_equals_eval_cone_everywhere() {
    let mut checked_elements = 0u64;
    for (name, program, params, tokens) in all_programs() {
        let interp = Interpreter::new(&program).expect("valid");
        let r = run(&program, &params, &tokens);
        for pos in 0..tokens.len() as u32 {
            // Every state value supplied at this position: no replay in this test.
            let mut src = source(&program, &params, &r, &|p| p == pos);
            for (block, layer) in program.occurrences() {
                let occ = occurrence_of(&program, block, layer);
                let ctx = DemandContext { pos, occurrence: occ };
                for target in 0..program.blocks[block as usize].nodes.len() as u16 {
                    let env = cone_env(&program, &r, pos, block, layer, target);
                    let full = interp
                        .eval_cone(block, layer, target, &params, &env)
                        .unwrap_or_else(|e| panic!("{name} pos {pos} block {block} node {target}: eval_cone {e}"));
                    if let Some(c) = r.commits.get(&(ctx, target)) {
                        assert_eq!(&full, c, "{name}: the cone reproduces the committed value");
                    }
                    let n = full.data.len();
                    let mut sets: Vec<Vec<usize>> = Vec::new();
                    for tile in [1usize, 3, 8] {
                        for start in (0..n).step_by(tile) {
                            sets.push((start..(start + tile).min(n)).collect());
                        }
                    }
                    sets.push((0..n).rev().collect());
                    sets.push((0..n).filter(|i| i % 3 == 1).chain((0..n).filter(|i| i % 3 == 0)).collect());
                    src.requests.clear();
                    let mut work_of_all = None;
                    for elements in sets {
                        let request = DemandRequest { target: DemandTarget::Node { ctx, node: target }, elements: &elements };
                        let (values, work) = eval_demanded(&program, &interp.info, &request, &mut src, &DemandLimits::UNLIMITED)
                            .unwrap_or_else(|e| panic!("{name} pos {pos} block {block} node {target}: eval_demanded {e}"));
                        let expect: Vec<i128> = elements.iter().map(|e| full.data[*e]).collect();
                        assert_eq!(
                            values, expect,
                            "{name} pos {pos} block {block} layer {layer:?} node {target} elements {elements:?}"
                        );
                        if elements.len() == n {
                            // The work is the demanded set's, whatever order it was asked in.
                            match work_of_all {
                                None => work_of_all = Some(work),
                                Some(w) => assert_eq!(w, work, "{name} pos {pos} node {target}: work depends on the order"),
                            }
                        }
                        checked_elements += elements.len() as u64;
                    }
                    for q in &src.requests {
                        match q {
                            MapSourceRequest::Node { ctx: c, node, .. } => {
                                assert!(!(*c == ctx && *node == target), "{name}: the target is never a leaf");
                                let (b, _) = program.occurrences()[c.occurrence as usize];
                                assert!(program.blocks[b as usize].nodes[*node as usize].commit, "{name}: a leaf is a commit point");
                                assert!(
                                    c.pos == pos && (c.occurrence == occ || c.occurrence + 1 == occ),
                                    "{name}: a leaf of this step"
                                );
                            }
                            MapSourceRequest::State { pos: p, .. } => assert_eq!(*p, pos, "{name}: supplied, never replayed"),
                            _ => {}
                        }
                    }
                }
            }
        }
    }
    assert!(checked_elements > 100_000, "the comparison covered {checked_elements} elements");
}

/// Tokens past the vector's own: its tokens, repeated.
fn longer(tokens: &[u32], n: usize) -> Vec<u32> {
    (0..n).map(|i| tokens[i % tokens.len()]).collect()
}

#[test]
fn replay_between_checkpoints_is_the_run() {
    let mut replayed = 0u64;
    for (name, program, params, tokens) in all_programs() {
        if instances(&program, false).is_empty() {
            continue;
        }
        let interp = Interpreter::new(&program).expect("valid");
        let tokens = longer(&tokens, 9);
        let r = run(&program, &params, &tokens);
        for c in [1u32, 3, 100] {
            // A checkpoint value at the start of every position `p ≡ 0 (mod C)`; replay elsewhere.
            let mut src = source(&program, &params, &r, &|p| p % c == 0);
            // Every state instance after every position.
            for q in 0..tokens.len() as u32 {
                for (j, l) in instances(&program, false) {
                    let n = fixed_value(&program, &r.after[q as usize], j, l).len();
                    let elements: Vec<usize> = (0..n).collect();
                    let request =
                        DemandRequest { target: DemandTarget::StateAfter { pos: q, state: j, layer: l }, elements: &elements };
                    let (values, _) = eval_demanded(&program, &interp.info, &request, &mut src, &DemandLimits::UNLIMITED)
                        .unwrap_or_else(|e| panic!("{name} C {c} state {j}@{l:?} after {q}: {e}"));
                    assert_eq!(values, fixed_value(&program, &r.after[q as usize], j, l), "{name} C {c} state {j}@{l:?} after {q}");
                    replayed += n as u64;
                }
            }
            // Every node of every occurrence of every position, its states replayed.
            for pos in 0..tokens.len() as u32 {
                for (block, layer) in program.occurrences() {
                    let ctx = DemandContext { pos, occurrence: occurrence_of(&program, block, layer) };
                    for target in 0..program.blocks[block as usize].nodes.len() as u16 {
                        let env = cone_env(&program, &r, pos, block, layer, target);
                        let full = interp.eval_cone(block, layer, target, &params, &env).expect("honest cone");
                        let elements: Vec<usize> = (0..full.data.len()).collect();
                        let request = DemandRequest { target: DemandTarget::Node { ctx, node: target }, elements: &elements };
                        let (values, _) = eval_demanded(&program, &interp.info, &request, &mut src, &DemandLimits::UNLIMITED)
                            .unwrap_or_else(|e| panic!("{name} C {c} pos {pos} block {block} node {target}: {e}"));
                        assert_eq!(values, full.data, "{name} C {c} pos {pos} block {block} layer {layer:?} node {target}");
                    }
                }
            }
            // A supplied value is only ever read at a checkpoint; between them the source said `Replay`.
            for q in &src.requests {
                if let MapSourceRequest::State { pos, .. } = q {
                    assert!(pos % c == 0 || !src.states.keys().any(|(p, _, _)| p == pos), "{name} C {c}: replay between checkpoints");
                }
            }
        }
    }
    assert!(replayed > 100, "replayed {replayed} state elements");
}

/// A state program (`S ← sat(S + w(token))` per layer, the golden `fixed-state-saturation`).
fn fixed_state_program() -> (TirProgramV1, MapParams) {
    let path = programs_dir().join("fixed-state-saturation.json");
    let (_, program, params, _) = load(&path);
    (program, params)
}

fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new().stack_size(256 * 1024).spawn(f).expect("spawn").join().expect("no stack overflow")
}

#[test]
fn a_long_replay_needs_no_native_stack() {
    let (program, params) = fixed_state_program();
    let tokens = longer(&[0, 3, 1, 2, 3, 3, 0, 1], 3_000);
    let r = run(&program, &params, &tokens);
    // Only the initial value at position 0 is supplied: the state after the last position is a
    // replay over all 3,000 positions.
    let src = source(&program, &params, &r, &|p| p == 0);
    let last = tokens.len() as u32 - 1;
    let want = fixed_value(&program, &r.after[last as usize], 0, Some(1));
    let got = on_small_stack(move || {
        let mut src = src;
        let info = misaka_palw_tir::validate::validate(&program).expect("valid");
        let request = DemandRequest { target: DemandTarget::StateAfter { pos: last, state: 0, layer: Some(1) }, elements: &[0, 1] };
        eval_demanded(&program, &info, &request, &mut src, &DemandLimits::UNLIMITED).expect("replayed").0
    });
    assert_eq!(got, want);
}

#[test]
fn a_long_chain_needs_no_native_stack() {
    // pre: token → 500 chained adds → carry; one pass-through layer; post: the logits.
    let mut pb = ProgramBuilder::new(8, HISTORY_BOUND_V1_SMALL);
    let carry = vec![TensorType::fixed(DType::I32, &[1])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let tk = b.cast(Ref::Input(INPUT_TOKEN), DType::I32);
        let mut x = b.reshape_fixed(tk, &[1]);
        let one = b.c(DType::I32, 1);
        for _ in 0..500 {
            x = b.add(x, one, DType::I32);
        }
        b.finish(&[x])
    };
    let layer = {
        let mut b = pb.block("pass", carry.clone());
        let y = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.finish(&[y])
    };
    let post = {
        let mut b = pb.block("post", carry);
        let l = b.reshape_fixed(Ref::CarryIn(0), &[1]);
        b.commit(l);
        b.finish(&[])
    };
    let program = pb.finish(pre, vec![layer], post, 0);
    let params = MapParams::default();
    let r = run(&program, &params, &[5]);
    let src = source(&program, &params, &r, &|_| true);
    let target = program.blocks[pre as usize].carry_out[0];
    let got = on_small_stack(move || {
        let mut src = src;
        let info = misaka_palw_tir::validate::validate(&program).expect("valid");
        let request = DemandRequest {
            target: DemandTarget::Node { ctx: DemandContext { pos: 0, occurrence: 0 }, node: target },
            elements: &[0],
        };
        eval_demanded(&program, &info, &request, &mut src, &DemandLimits::UNLIMITED).expect("evaluated")
    });
    assert_eq!(got.0, vec![505]);
    assert_eq!(got.1.elements, 502, "the reshape, the 500 adds, and the cast; the const is no node");
}

/// The refusals: a context, node or element that is not the program's, a missing value, a replay
/// with nothing before it, a Hist instance named as Fixed, and the work limit — each an error.
#[test]
fn eval_demanded_refuses_what_it_cannot_honestly_answer() {
    let path = programs_dir().join("dense-gqa-2layer.json");
    let (_, program, params, tokens) = load(&path);
    let info = misaka_palw_tir::validate::validate(&program).expect("valid");
    let r = run(&program, &params, &tokens[..1]);
    let mut src = source(&program, &params, &r, &|_| true);
    let pre = program.schedule.pre;
    let target = program.blocks[pre as usize].carry_out[0];
    let at = |occurrence: u16, node: u16| DemandTarget::Node { ctx: DemandContext { pos: 0, occurrence }, node };
    let ok = DemandRequest { target: at(0, target), elements: &[0] };
    assert!(eval_demanded(&program, &info, &ok, &mut src, &DemandLimits::UNLIMITED).is_ok());

    let malformed = |res: Result<_, DemandError>| matches!(res, Err(DemandError::Tir(e)) if e.kind == TirErrorKind::Malformed);
    let occurrences = program.occurrences().len() as u16;
    let no_occurrence = DemandRequest { target: at(occurrences, 0), ..ok };
    assert!(malformed(eval_demanded(&program, &info, &no_occurrence, &mut src, &DemandLimits::UNLIMITED)));
    let no_node = DemandRequest { target: at(0, program.blocks[pre as usize].nodes.len() as u16), ..ok };
    assert!(malformed(eval_demanded(&program, &info, &no_node, &mut src, &DemandLimits::UNLIMITED)));
    let far = [usize::MAX / 2];
    let outside = DemandRequest { elements: &far, ..ok };
    assert!(malformed(eval_demanded(&program, &info, &outside, &mut src, &DemandLimits::UNLIMITED)));

    let mut empty = MapSource::default();
    assert!(matches!(
        eval_demanded(&program, &info, &ok, &mut empty, &DemandLimits::UNLIMITED),
        Err(DemandError::Tir(e)) if e.kind == TirErrorKind::Missing
    ));
    let tiny = DemandLimits { max_elements: 1, max_terms: 0 };
    assert!(matches!(eval_demanded(&program, &info, &ok, &mut src, &tiny), Err(DemandError::WorkLimit(_))));
    let far_pos = DemandRequest { target: DemandTarget::Node { ctx: DemandContext { pos: u32::MAX, occurrence: 0 }, node: 0 }, ..ok };
    assert!(eval_demanded(&program, &info, &far_pos, &mut src, &DemandLimits::UNLIMITED).is_err());

    // A state read at position 0 the source answers `Replay` for: nothing precedes position 0.
    let (sp, sparams) = fixed_state_program();
    let sinfo = misaka_palw_tir::validate::validate(&sp).expect("valid");
    let sr = run(&sp, &sparams, &[1]);
    let mut replay_at_zero = source(&sp, &sparams, &sr, &|_| false);
    let writer = sp.blocks[sp.schedule.layers[0] as usize]
        .nodes
        .iter()
        .position(|n| matches!(n.prim, misaka_palw_tir::Prim::StateWrite { .. }))
        .unwrap() as u16;
    let at_zero = DemandRequest { target: at(1, writer), elements: &[0] };
    assert!(matches!(
        eval_demanded(&sp, &sinfo, &at_zero, &mut replay_at_zero, &DemandLimits::UNLIMITED),
        Err(DemandError::Tir(e)) if e.kind == TirErrorKind::Missing
    ));
    // A Hist state named as a Fixed instance.
    let (hp, hparams, htokens) = {
        let (_, p, params, t) = load(&programs_dir().join("hist-window.json"));
        (p, params, t)
    };
    let hinfo = misaka_palw_tir::validate::validate(&hp).expect("valid");
    let hr = run(&hp, &hparams, &htokens[..1]);
    let mut hsrc = source(&hp, &hparams, &hr, &|_| true);
    let hist_as_fixed = DemandRequest { target: DemandTarget::StateAfter { pos: 0, state: 0, layer: Some(0) }, elements: &[0] };
    assert!(malformed(eval_demanded(&hp, &hinfo, &hist_as_fixed, &mut hsrc, &DemandLimits::UNLIMITED)));
}
