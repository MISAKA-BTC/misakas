//! 04b §9.4 differential: this crate's demand evaluator against `misaka_palw_tir::demand`, called as
//! a black box through its public API (`validate`, `eval_demanded` with a `DemandSource`), never its
//! source.
//!
//! Both implementations are asked the same request and see the same answers
//! (`common::demsrc::Model`). They must agree on success versus failure; on success, on the values,
//! the work and the request SET — §9.4: "the order it asks in is not part of the result", so the
//! order is counted, not required; on failure, each class must be one the text allows for that
//! request and those answers (`demand_outcomes`: the class of some failing element, or `WorkLimit`
//! when the work exceeds the limits).
//!
//! Families: random programs × random targets × random element sets; replay windows (states supplied
//! only at some positions, including none at 0); hostile sources (refusals, values outside the dtype
//! or `[lo, hi]`, `Replay` for a supplied value, tokens at `token_bound`, inconsistent values); the
//! work limits at the boundary and one past it; and hand-built edge programs (long replays, carried
//! values, frame-cap corners, request refusals).
//!
//! Scale with `TIR_REF2_CASES` (default 1; the findings report runs at 25).

mod common;

use std::collections::{BTreeMap, BTreeSet};

use common::bridge::{Outcome, catch_any, first_decode};
use common::demsrc::*;
use common::progen::{GenCfg, R, gen_program, pick};
use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use rand::{Rng, SeedableRng};
use ref2::build::{ProgBuilder, fixed};
use ref2::codec::encode;
use ref2::demand::{Ctx, Limits, Question, Target, Work};
use ref2::eval::{Params, initial_state};
use ref2::normal_form::block_window;
use ref2::tensor::count;
use ref2::{DType, Prim, Program, Ref};

fn scale() -> usize {
    std::env::var("TIR_REF2_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

struct Prepared {
    prog: Program,
    fp: first::program::TirProgramV1,
    info: first::validate::ProgramInfo,
}

fn prepare(prog: &Program) -> Option<Prepared> {
    if ref2::normal_form::check(prog).is_err() {
        return None;
    }
    let Outcome::Ok(fp) = first_decode(&encode(prog)) else { return None };
    let info = catch_any(|| first::validate::validate(&fp)).ok()?.ok()?;
    Some(Prepared { prog: prog.clone(), fp, info })
}

fn show_elems(e: &[u64]) -> String {
    if e.len() <= 8 { format!("{e:?}") } else { format!("{:?}… ({} elements)", &e[..8], e.len()) }
}

type Honest = (Vec<i128>, Work, BTreeSet<Question>);

#[derive(Default)]
struct Tally {
    cases: usize,
    ok: usize,
    failed: BTreeMap<&'static str, usize>,
    same_order: usize,
    boundary: usize,
    hostile: usize,
    /// Successful cases by the target's primitive (`state_after` for that target).
    covered: BTreeMap<&'static str, usize>,
    disagreements: BTreeMap<String, (usize, Vec<String>)>,
}

impl Tally {
    fn disagree(&mut self, kind: &str, detail: String) {
        let e = self.disagreements.entry(kind.to_string()).or_default();
        e.0 += 1;
        if e.1.len() < 4 {
            e.1.push(detail);
        }
    }

    /// One request on both implementations; this crate's result when both succeed and agree.
    fn case(&mut self, tag: &str, pp: &Prepared, target: &Target, elements: &[u64], m: &Model, limits: Limits) -> Option<Honest> {
        self.cases += 1;
        let a = mine_demand(&pp.prog, target, elements, m, limits);
        let f = first_demand(&pp.fp, &pp.info, target, elements, m, limits);
        let allowed = catch_any(|| allowed(&pp.prog, target, elements, m, limits));
        let what = || {
            format!("{tag}: {target:?} elements {} limits ({}, {})", show_elems(elements), limits.max_elements, limits.max_terms)
        };
        let allowed = match allowed {
            Ok(x) => x,
            Err(msg) => {
                self.disagree("ref2 exploration panics", format!("{}: {msg}", what()));
                return None;
            }
        };
        match (&a.res, &f.res) {
            (DRes::Panic(msg), _) => {
                self.disagree("ref2 panics", format!("{}: {msg}", what()));
                None
            }
            (_, DRes::Panic(msg)) => {
                self.disagree("first panics", format!("{}: {msg}", what()));
                None
            }
            (DRes::Ok(v1, w1), DRes::Ok(v2, w2)) => {
                let mut bad = false;
                if v1 != v2 {
                    self.disagree("values", format!("{}: ref2 {v1:?} first {v2:?}", what()));
                    bad = true;
                }
                if w1 != w2 {
                    self.disagree("work", format!("{}: ref2 {w1:?} first {w2:?}", what()));
                    bad = true;
                }
                if a.asked != f.asked {
                    let only_a: Vec<String> = a.asked.difference(&f.asked).take(4).map(|q| format!("{q:?}")).collect();
                    let only_f: Vec<String> = f.asked.difference(&a.asked).take(4).map(|q| format!("{q:?}")).collect();
                    self.disagree("request set", format!("{}: only ref2 asks {only_a:?}; only first asks {only_f:?}", what()));
                    bad = true;
                }
                match &allowed {
                    Ok((v, w)) if v == v1 && w == w1 => {}
                    other => self.disagree("ref2 internal: exploration differs", format!("{}: {other:?}", what())),
                }
                if bad {
                    return None;
                }
                self.ok += 1;
                let prim = match *target {
                    Target::Node { ctx, node } => {
                        pp.prog.blocks[occurrences(&pp.prog)[ctx.occ as usize].0].nodes[node as usize].prim.name()
                    }
                    Target::StateAfter { .. } => "state_after",
                };
                *self.covered.entry(prim).or_default() += 1;
                if a.order == f.order {
                    self.same_order += 1;
                }
                Some((v1.clone(), *w1, a.asked))
            }
            (DRes::Err(e1), DRes::Err(e2)) => {
                match &allowed {
                    Err(set) => {
                        if !set.contains(e2) {
                            self.disagree(
                                "first's class is not one the text allows",
                                format!("{}: first {} ref2 {} allowed {:?}", what(), e2.name(), e1.name(), set),
                            );
                        }
                        if !set.contains(e1) {
                            self.disagree("ref2 internal: class outside its own set", format!("{}: {} vs {set:?}", what(), e1.name()));
                        }
                    }
                    Ok(_) => self.disagree("ref2 internal: exploration succeeds", what()),
                }
                *self.failed.entry(e2.name()).or_default() += 1;
                None
            }
            (DRes::Ok(v, w), DRes::Err(e)) => {
                self.disagree("ref2 succeeds, first refuses", format!("{}: first {}; ref2 {v:?} work {w:?}", what(), e.name()));
                None
            }
            (DRes::Err(e), DRes::Ok(v, w)) => {
                self.disagree(
                    "ref2 refuses, first succeeds",
                    format!("{}: ref2 {} (allowed {:?}); first {v:?} work {w:?}", what(), e.name(), allowed.err()),
                );
                None
            }
        }
    }

    /// The work limits at the boundary: exactly the work succeeds, one short in either count fails
    /// with `WorkLimit` (checked through `case`, whose allowed set is exactly that).
    fn boundary(&mut self, tag: &str, pp: &Prepared, target: &Target, elements: &[u64], m: &Model, w: Work) {
        self.boundary += 1;
        let exact = Limits { max_elements: w.elements, max_terms: w.terms };
        self.case(&format!("{tag} exact"), pp, target, elements, m, exact);
        if w.elements > 0 {
            self.case(&format!("{tag} one element short"), pp, target, elements, m, Limits { max_elements: w.elements - 1, ..exact });
        }
        if w.terms > 0 {
            self.case(&format!("{tag} one term short"), pp, target, elements, m, Limits { max_terms: w.terms - 1, ..exact });
        }
    }

    fn report(&self, name: &str) {
        println!(
            "{name}: {} cases, {} both succeed (same first-asked order in {}), agreed failures {:?}, {} boundary triples, {} hostile, {} disagreements",
            self.cases,
            self.ok,
            self.same_order,
            self.failed,
            self.boundary,
            self.hostile,
            self.disagreements.values().map(|d| d.0).sum::<usize>()
        );
        println!("    successes by target: {:?}", self.covered);
        for (k, (n, ex)) in &self.disagreements {
            println!("  DISAGREE [{k}] ×{n}");
            for e in ex {
                println!("      {e}");
            }
        }
    }

    fn clean(&self) -> bool {
        self.disagreements.is_empty()
    }
}

// ---------------------------------------------------------------- random material

#[derive(Clone, Debug)]
enum Pattern {
    Every,
    EveryK(u64),
    OnlyZero,
    Nothing,
    Some(BTreeSet<u64>),
}

impl Pattern {
    fn has(&self, p: u64) -> bool {
        match self {
            Pattern::Every => true,
            Pattern::EveryK(k) => p % k == 0,
            Pattern::OnlyZero => p == 0,
            Pattern::Nothing => false,
            Pattern::Some(s) => s.contains(&p),
        }
    }
}

fn rand_pattern(rng: &mut R, positions: u64) -> Pattern {
    match rng.gen_range(0..10) {
        0..=2 => Pattern::Every,
        3..=4 => Pattern::EveryK(rng.gen_range(2..=5)),
        5..=6 => Pattern::OnlyZero,
        7 => Pattern::Nothing,
        _ => Pattern::Some((0..=positions).filter(|_| rng.gen_bool(0.35)).collect()),
    }
}

fn model(rng: &mut R, p: &Program, params: &Params, positions: u64, pattern: &Pattern) -> Option<Model> {
    let bound = (p.token_bound as u64).clamp(1, 1000);
    let tokens: Vec<u64> = (0..positions).map(|_| rng.gen_range(0..bound)).collect();
    Model::from_run(p, params, &tokens, &|q| pattern.has(q))
}

fn rand_elements(rng: &mut R, c: u64) -> Vec<u64> {
    match rng.gen_range(0..40) {
        0..=3 => vec![],
        4 => vec![c + rng.gen_range(0..3)],
        5..=16 if c <= 64 => (0..c).collect(),
        _ => {
            let k = rng.gen_range(1..=6);
            let mut v: Vec<u64> = (0..k).map(|_| rng.gen_range(0..c.max(1))).collect();
            if rng.gen_bool(0.2) {
                let x = v[0];
                v.push(x);
            }
            v
        }
    }
}

fn h_of(p: &Program, b: usize, pos: u64) -> u64 {
    match block_window(p, b).ok().flatten() {
        Some(w) => (pos + 1).min(w as u64),
        None => 1,
    }
}

fn rand_node_target(rng: &mut R, p: &Program, positions: u64) -> (Target, Vec<u64>) {
    let occs = occurrences(p);
    let pos = if rng.gen_bool(0.93) { rng.gen_range(0..positions) } else { positions + rng.gen_range(0..2) };
    let o = rng.gen_range(0..occs.len());
    let b = occs[o].0;
    let n = rng.gen_range(0..p.blocks[b].nodes.len());
    let c = count(&p.blocks[b].nodes[n].out.extents(h_of(p, b, pos)));
    (Target::Node { ctx: Ctx { pos, occ: o as u32 }, node: n as u16 }, rand_elements(rng, c))
}

/// Every `Fixed` instance the run holds, plus (rarely) one no block references.
fn instances(p: &Program) -> Vec<(u16, Option<u32>)> {
    initial_state(p).fixed.keys().copied().collect()
}

fn rand_state_target(rng: &mut R, p: &Program, positions: u64) -> Option<(Target, Vec<u64>)> {
    let inst = instances(p);
    let pos = if rng.gen_bool(0.93) { rng.gen_range(0..positions) } else { positions + rng.gen_range(0..2) };
    let (j, l) = if !inst.is_empty() && rng.gen_bool(0.9) {
        pick(rng, &inst)
    } else if !p.states.is_empty() {
        // Any state with any layer: exercises the Malformed refusals and unreferenced instances.
        let j = rng.gen_range(0..p.states.len()) as u16;
        let l = if rng.gen_bool(0.5) { None } else { Some(rng.gen_range(0..=p.schedule.layers.len() as u32)) };
        (j, l)
    } else {
        return None;
    };
    let c: u64 = p.states[j as usize].shape.iter().map(|&d| d as u64).product();
    Some((Target::StateAfter { pos, state: j, layer: l }, rand_elements(rng, c)))
}

fn rand_limits(rng: &mut R) -> Limits {
    if rng.gen_bool(0.8) {
        Limits::UNLIMITED
    } else {
        Limits { max_elements: rng.gen_range(0..40), max_terms: rng.gen_range(0..80) }
    }
}

// ---------------------------------------------------------------- hostile answers

/// The dtype a question's answer must have.
fn answer_dtype(p: &Program, q: &Question) -> Option<DType> {
    let occs = occurrences(p);
    Some(match *q {
        Question::Node { ctx, node, .. } => p.blocks[occs.get(ctx.occ as usize)?.0].nodes.get(node as usize)?.out.dtype,
        Question::Param { param, .. } => p.params.get(param as usize)?.dtype,
        Question::State { state, .. } | Question::HistRow { state, .. } => p.states.get(state as usize)?.dtype,
        Question::Token { .. } => DType::Idx,
    })
}

/// A hostile answer for a question the honest evaluation asks.
fn rand_fault(rng: &mut R, p: &Program, m: &Model, q: &Question) -> Fault {
    let dt = answer_dtype(p, q).unwrap_or(DType::I32);
    let honest = m.answer(q);
    let inconsistent = |rng: &mut R, lo: i128, hi: i128| {
        let v = match honest {
            Some(Answer::Val(v)) => v,
            _ => 0,
        };
        let mut x = v;
        for _ in 0..8 {
            x = match rng.gen_range(0..4) {
                0 => (v + rng.gen_range(1..=3)).min(hi),
                1 => pick(rng, &[lo, hi, 0, -1, 1]),
                _ => rng.gen_range(lo..=hi.min(lo.saturating_add(1_000_000))),
            };
            if x != v {
                break;
            }
        }
        Fault::Value(x.clamp(lo, hi))
    };
    match *q {
        Question::Token { .. } => match rng.gen_range(0..3) {
            0 => Fault::Refuse,
            1 => Fault::Value(if p.token_bound < u32::MAX { p.token_bound as i128 } else { u32::MAX as i128 }),
            _ => inconsistent(rng, 0, (p.token_bound as i128 - 1).min(999)),
        },
        Question::State { state, .. } => {
            let (lo, hi) = fixed_range(p, state).unwrap_or((dt.min(), dt.max()));
            match rng.gen_range(0..6) {
                0 => Fault::Refuse,
                1 => Fault::Replay,
                2 if hi < dt.max() => Fault::Value(hi + 1),
                2 if lo > dt.min() => Fault::Value(lo - 1),
                3 => Fault::Value(if rng.gen_bool(0.5) { dt.max() + 1 } else { dt.min() - 1 }),
                _ => inconsistent(rng, lo, hi),
            }
        }
        _ => match rng.gen_range(0..4) {
            0 => Fault::Refuse,
            1 if dt != DType::I128 => Fault::Value(if rng.gen_bool(0.5) { dt.max() + 1 } else { dt.min() - 1 }),
            2 => inconsistent(rng, dt.min(), dt.max()),
            _ => inconsistent(rng, dt.min().max(-1_000_000), dt.max().min(1_000_000)),
        },
    }
}

fn hostile(t: &mut Tally, rng: &mut R, tag: &str, pp: &Prepared, target: &Target, elements: &[u64], m: &Model, honest: &Honest) {
    let asked: Vec<Question> = honest.2.iter().copied().collect();
    if asked.is_empty() {
        return;
    }
    for round in 0..3 {
        let mut bad = m.clone();
        let k = if round == 0 { 1 } else { rng.gen_range(1..=3) };
        for _ in 0..k {
            let q = asked[rng.gen_range(0..asked.len())];
            let f = rand_fault(rng, &pp.prog, m, &q);
            bad.faults.insert(q, f);
        }
        t.hostile += 1;
        let desc: Vec<String> = bad.faults.iter().map(|(q, f)| format!("{q:?}={f:?}")).collect();
        t.case(&format!("{tag} hostile {desc:?}"), pp, target, elements, &bad, Limits::UNLIMITED);
    }
}

// ---------------------------------------------------------------- the families

fn random_program(rng: &mut R) -> Option<(Prepared, Params)> {
    random_program_with(rng, true)
}

fn random_program_with(rng: &mut R, range_safe: bool) -> Option<(Prepared, Params)> {
    let g = gen_program(rng, GenCfg { range_safe, ..GenCfg::default() });
    let pp = prepare(&g.prog)?;
    Some((pp, g.params))
}

fn random_family(name: &str, base_seed: u64, programs: u64, range_safe: bool) {
    let mut t = Tally::default();
    let mut skipped = 0;
    for seed in 0..programs {
        let mut rng = R::seed_from_u64(base_seed + seed);
        let Some((pp, params)) = random_program_with(&mut rng, range_safe) else {
            skipped += 1;
            continue;
        };
        let positions = rng.gen_range(1..=5u64);
        let pattern = rand_pattern(&mut rng, positions);
        let Some(m) = model(&mut rng, &pp.prog, &params, positions, &pattern) else {
            skipped += 1;
            continue;
        };
        for k in 0..12 {
            let (target, elements) = if k % 4 == 3 {
                match rand_state_target(&mut rng, &pp.prog, positions) {
                    Some(x) => x,
                    None => rand_node_target(&mut rng, &pp.prog, positions),
                }
            } else {
                rand_node_target(&mut rng, &pp.prog, positions)
            };
            let limits = rand_limits(&mut rng);
            let tag = format!("seed {seed} #{k} pattern {pattern:?}");
            if let Some(h) = t.case(&tag, &pp, &target, &elements, &m, limits)
                && limits == Limits::UNLIMITED
            {
                t.boundary(&tag, &pp, &target, &elements, &m, h.1);
                if rng.gen_bool(if range_safe { 0.3 } else { 0.6 }) {
                    hostile(&mut t, &mut rng, &tag, &pp, &target, &elements, &m, &h);
                }
            }
        }
    }
    t.report(&format!("{name} ({skipped} programs skipped: not in normal form for both, or the honest run fails)"));
    assert!(t.clean());
}

#[test]
fn random_programs_targets_and_elements() {
    random_family("random range-safe programs × targets × element sets", 0xDE4A_0000, 400 * scale() as u64, true);
}

/// Programs without the range guarantee: hostile values reach Overflow, Divisor and Index.
#[test]
fn random_unguarded_programs() {
    random_family("random unguarded programs × targets × element sets", 0xDE4B_0000, 400 * scale() as u64, false);
}

/// States supplied only at some positions (none at 0 included): every instance after every
/// position, and nodes of the last positions, replayed across the gaps.
#[test]
fn replay_windows() {
    let n = 150 * scale() as u64;
    let mut t = Tally::default();
    let mut with_states = 0;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x2E91_0000 + seed);
        let Some((pp, params)) = random_program(&mut rng) else { continue };
        if instances(&pp.prog).is_empty() {
            continue;
        }
        with_states += 1;
        let positions = rng.gen_range(6..=20u64);
        let pattern = match rng.gen_range(0..4) {
            0 => Pattern::OnlyZero,
            1 => Pattern::EveryK(rng.gen_range(3..=7)),
            2 => Pattern::Some((0..=positions).filter(|_| rng.gen_bool(0.2)).collect()),
            _ => Pattern::Some((1..=positions).filter(|_| rng.gen_bool(0.3)).collect()),
        };
        let Some(m) = model(&mut rng, &pp.prog, &params, positions, &pattern) else { continue };
        let tag = format!("seed {seed} pattern {pattern:?}");
        for (j, l) in instances(&pp.prog) {
            let c: u64 = pp.prog.states[j as usize].shape.iter().map(|&d| d as u64).product();
            for pos in 0..positions {
                let target = Target::StateAfter { pos, state: j, layer: l };
                let elements: Vec<u64> = if c <= 8 { (0..c).collect() } else { vec![0, c - 1, rng.gen_range(0..c)] };
                if let Some(h) = t.case(&tag, &pp, &target, &elements, &m, Limits::UNLIMITED)
                    && rng.gen_bool(0.25)
                {
                    t.boundary(&tag, &pp, &target, &elements, &m, h.1);
                    if rng.gen_bool(0.3) {
                        hostile(&mut t, &mut rng, &tag, &pp, &target, &elements, &m, &h);
                    }
                }
            }
        }
        for _ in 0..6 {
            let (target, elements) = rand_node_target(&mut rng, &pp.prog, positions);
            let target = match target {
                Target::Node { ctx, node } => Target::Node { ctx: Ctx { pos: positions - 1 - ctx.pos % 3, ..ctx }, node },
                x => x,
            };
            let occs = occurrences(&pp.prog);
            let Target::Node { ctx, node } = target else { unreachable!() };
            let b = occs[ctx.occ as usize].0;
            let c = count(&pp.prog.blocks[b].nodes[node as usize].out.extents(h_of(&pp.prog, b, ctx.pos)));
            let elements: Vec<u64> = elements.into_iter().filter(|&e| e < c).collect();
            if let Some(h) = t.case(&tag, &pp, &target, &elements, &m, Limits::UNLIMITED) {
                t.boundary(&tag, &pp, &target, &elements, &m, h.1);
            }
        }
    }
    t.report(&format!("replay windows ({with_states} programs with Fixed states)"));
    assert!(t.clean());
}

// ---------------------------------------------------------------- hand-built edges

const HB: u32 = 1 << 18;

/// pre: a counter `s` (global, uncommitted writer unless `commit_writer`) and a global state `u`
/// nothing writes; a layer block with a per-layer state `v` it reads and writes; post: logits.
fn edge_program(commit_writer: bool) -> Program {
    let mut b = ProgBuilder::new(HB, 16);
    let s = b.fixed_state("s", DType::I32, &[2], -(1 << 30), 1 << 30, false);
    let u = b.fixed_state("u", DType::I16, &[2], -50, 50, false);
    let v = b.fixed_state("v", DType::I16, &[1], -7, 7, true);
    let pre = b.block("pre", vec![]);
    let one = b.konst(DType::I32, &[2], &[1, 2]);
    let n0 = b.node(pre, Prim::Add, &[Ref::State(s), Ref::Const(one)], fixed(DType::I32, &[2]), false);
    let n1 = b.node(pre, Prim::StateWrite { state: s }, &[Ref::Node(n0)], fixed(DType::I32, &[2]), commit_writer);
    let n2 = b.node(pre, Prim::Add, &[Ref::Node(n1), Ref::State(u)], fixed(DType::I64, &[2]), false);
    let n3 = b.node(pre, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::Node(n2)], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[n3]);
    let lay = b.block("layer", vec![fixed(DType::I32, &[2])]);
    let m0 = b.node(lay, Prim::ReduceSum { axis: 0 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1]), false);
    let m1 = b.node(lay, Prim::Add, &[Ref::Node(m0), Ref::State(v)], fixed(DType::I64, &[1]), false);
    b.node(lay, Prim::StateWrite { state: v }, &[Ref::Node(m1)], fixed(DType::I16, &[1]), false);
    let m3 = b.node(lay, Prim::Broadcast, &[Ref::Node(m1)], fixed(DType::I64, &[2]), false);
    let m4 = b.node(lay, Prim::Clamp { lo: -1000, hi: 1000 }, &[Ref::Node(m3)], fixed(DType::I32, &[2]), true);
    b.carry_out(lay, &[m4]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let k0 = b.node(post, Prim::Add, &[Ref::CarryIn(0), Ref::State(u)], fixed(DType::I64, &[2]), false);
    let k1 = b.node(post, Prim::Clamp { lo: -1_000_000, hi: 1_000_000 }, &[Ref::Node(k0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[lay, lay], post, k1);
    b.finish()
}

#[test]
fn edges_long_replays_carried_values_and_refusals() {
    let mut t = Tally::default();
    for commit_writer in [false, true] {
        let prog = edge_program(commit_writer);
        let pp = prepare(&prog).expect("the edge program is in normal form for both");
        let params = Params::new();
        let tag = format!("edge program (writer committed: {commit_writer})");
        // A long run, states supplied at 0 only: replays across the whole run.
        let positions = 1500u64;
        let tokens: Vec<u64> = (0..positions).map(|i| i % 16).collect();
        let m = Model::from_run(&prog, &params, &tokens, &|q| q == 0).expect("the run succeeds");
        for &pos in &[0u64, 1, 2, 7, 700, positions - 1] {
            for (j, l) in [(0u16, None), (1, None), (2, Some(0)), (2, Some(1))] {
                let target = Target::StateAfter { pos, state: j, layer: l };
                let els: Vec<u64> = if j == 2 { vec![0] } else { vec![0, 1] };
                if let Some(h) = t.case(&tag, &pp, &target, &els, &m, Limits::UNLIMITED) {
                    t.boundary(&tag, &pp, &target, &els, &m, h.1);
                }
            }
            for (occ, node) in [(0u32, 3u16), (1, 4), (2, 4), (3, 1), (0, 1), (0, 2)] {
                let target = Target::Node { ctx: Ctx { pos, occ }, node };
                if let Some(h) = t.case(&tag, &pp, &target, &[0, 1], &m, Limits::UNLIMITED) {
                    t.boundary(&tag, &pp, &target, &[0, 1], &m, h.1);
                }
            }
        }
        // Zero work at zero limits: a supplied carried value and a committed writer's leaf.
        for target in [
            Target::StateAfter { pos: 0, state: 1, layer: None },
            Target::StateAfter { pos: 0, state: 0, layer: None },
        ] {
            t.case(&format!("{tag} zero limits"), &pp, &target, &[0, 1], &m, Limits { max_elements: 0, max_terms: 0 });
        }
        // Repeats cost nothing, whatever the frame cap.
        let rep: Vec<u64> = (0..5000).map(|i| i % 2).collect();
        for target in [Target::Node { ctx: Ctx { pos: 3, occ: 0 }, node: 1 }, Target::StateAfter { pos: 5, state: 1, layer: None }] {
            if let Some(h) = t.case(&format!("{tag} repeats"), &pp, &target, &[0, 1], &m, Limits::UNLIMITED) {
                t.case(&format!("{tag} 5000 repeats at exact limits"), &pp, &target, &rep, &m, Limits { max_elements: h.1.elements, max_terms: h.1.terms });
            }
        }
        // The empty request.
        t.case(&format!("{tag} empty"), &pp, &Target::Node { ctx: Ctx { pos: 2, occ: 1 }, node: 4 }, &[], &m, Limits { max_elements: 0, max_terms: 0 });
        // Replay at position 0, and nothing supplied at all.
        let none = Model::from_run(&prog, &params, &tokens[..20], &|_| false).unwrap();
        for target in [
            Target::StateAfter { pos: 0, state: 0, layer: None },
            Target::StateAfter { pos: 9, state: 1, layer: None },
            Target::StateAfter { pos: 9, state: 2, layer: Some(1) },
            Target::Node { ctx: Ctx { pos: 9, occ: 3 }, node: 1 },
        ] {
            t.case(&format!("{tag} nothing supplied"), &pp, &target, &[0], &none, Limits::UNLIMITED);
        }
        // Request refusals, alone and together.
        let hb = HB as u64;
        let refusals = [
            (Target::Node { ctx: Ctx { pos: hb, occ: 0 }, node: 0 }, vec![0u64]),
            (Target::Node { ctx: Ctx { pos: hb - 1, occ: 0 }, node: 0 }, vec![0]),
            (Target::Node { ctx: Ctx { pos: 0, occ: 4 }, node: 0 }, vec![0]),
            (Target::Node { ctx: Ctx { pos: hb, occ: 4 }, node: 0 }, vec![0]),
            (Target::Node { ctx: Ctx { pos: 0, occ: 0 }, node: 4 }, vec![0]),
            (Target::Node { ctx: Ctx { pos: 0, occ: 0 }, node: 0 }, vec![2]),
            (Target::Node { ctx: Ctx { pos: 0, occ: 0 }, node: 0 }, vec![u64::MAX]),
            (Target::Node { ctx: Ctx { pos: hb, occ: 0 }, node: 0 }, vec![7]),
            (Target::StateAfter { pos: 0, state: 0, layer: Some(0) }, vec![0]),
            (Target::StateAfter { pos: 0, state: 2, layer: None }, vec![0]),
            (Target::StateAfter { pos: 0, state: 2, layer: Some(2) }, vec![0]),
            (Target::StateAfter { pos: 0, state: 3, layer: None }, vec![0]),
            (Target::StateAfter { pos: 0, state: 1, layer: None }, vec![2]),
            (Target::StateAfter { pos: hb, state: 1, layer: None }, vec![0]),
            (Target::StateAfter { pos: hb, state: 9, layer: None }, vec![0]),
        ];
        for (target, els) in refusals {
            t.case(&format!("{tag} refusal"), &pp, &target, &els, &m, Limits::UNLIMITED);
        }
    }
    t.report("edges: long replays, carried values, zero limits, repeats, refusals");
    assert!(t.clean());
}

/// A Hist program: rows read through `hist_row` at every window size, hostile rows included.
#[test]
fn history_rows() {
    let mut t = Tally::default();
    let mut rng = R::seed_from_u64(0x4157);
    let hdim = ref2::Dim::H;
    for window in [1u32, 2, 3, 5, HB] {
        // pre appends a committed node's row to a global history; a layer block (two layers)
        // appends its CARRY-IN to a per-layer history of another window.
        let mut b = ProgBuilder::new(HB, 8);
        let hs = b.hist_state("h", DType::I16, &[2], window, false);
        let w2 = if window == HB { 4 } else { window + 1 };
        let hl = b.hist_state("hl", DType::I32, &[2], w2, true);
        let pre = b.block("pre", vec![]);
        let n0 = b.node(pre, Prim::Cast, &[Ref::Input(0)], fixed(DType::I16, &[]), false);
        let n1 = b.node(pre, Prim::Broadcast, &[Ref::Node(n0)], fixed(DType::I16, &[2]), true);
        let n2 = b.node(pre, Prim::HistAppend { state: hs }, &[Ref::Node(n1)], ref2::build::ty(DType::I16, &[hdim, ref2::Dim::Fixed(2)]), false);
        let n3 = b.node(pre, Prim::ReduceSum { axis: 0 }, &[Ref::Node(n2)], fixed(DType::I32, &[1, 2]), false);
        let n4 = b.node(pre, Prim::Reshape, &[Ref::Node(n3)], fixed(DType::I32, &[2]), true);
        b.carry_out(pre, &[n4]);
        let lay = b.block("layer", vec![fixed(DType::I32, &[2])]);
        let m0 = b.node(lay, Prim::HistAppend { state: hl }, &[Ref::CarryIn(0)], ref2::build::ty(DType::I32, &[hdim, ref2::Dim::Fixed(2)]), false);
        let m1 = b.node(lay, Prim::ReduceMax { axis: 0 }, &[Ref::Node(m0)], fixed(DType::I32, &[1, 2]), false);
        let m2 = b.node(lay, Prim::Reshape, &[Ref::Node(m1)], fixed(DType::I32, &[2]), false);
        let m3 = b.node(lay, Prim::Add, &[Ref::Node(m2), Ref::CarryIn(0)], fixed(DType::I64, &[2]), false);
        let m4 = b.node(lay, Prim::Clamp { lo: -100_000, hi: 100_000 }, &[Ref::Node(m3)], fixed(DType::I32, &[2]), true);
        b.carry_out(lay, &[m4]);
        let post = b.block("post", vec![fixed(DType::I32, &[2])]);
        let k0 = b.node(post, Prim::Clamp { lo: -100_000, hi: 100_000 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
        b.schedule(pre, &[lay, lay], post, k0);
        let prog = b.finish();
        let Some(pp) = prepare(&prog) else {
            t.disagree("edge program refused", format!("window {window}"));
            continue;
        };
        let tokens: Vec<u64> = (0..12).map(|i| (i * 3 + 1) % 8).collect();
        let m = Model::from_run(&prog, &Params::new(), &tokens, &|_| true).unwrap();
        let tag = format!("hist window {window}/{w2}");
        for pos in 0..12u64 {
            let h = (pos + 1).min(window as u64);
            let h2 = (pos + 1).min(w2 as u64);
            let mut targets: Vec<(u32, u16, u64)> = vec![(0, n2, 2 * h), (0, n3, 2), (0, n4, 2), (3, k0, 2)];
            for occ in [1u32, 2] {
                targets.extend([(occ, m0, 2 * h2), (occ, m1, 2), (occ, m4, 2)]);
            }
            for (occ, node, c) in targets {
                let els: Vec<u64> = (0..c).collect();
                let target = Target::Node { ctx: Ctx { pos, occ }, node };
                if let Some(hn) = t.case(&tag, &pp, &target, &els, &m, Limits::UNLIMITED) {
                    t.boundary(&tag, &pp, &target, &els, &m, hn.1);
                    hostile(&mut t, &mut rng, &tag, &pp, &target, &els, &m, &hn);
                }
            }
        }
    }
    t.report("history rows (a committed node's row, and a carry-in's row per layer)");
    assert!(t.clean());
}

/// One request, both results printed (for the findings), and checked like every other case.
fn show(t: &mut Tally, what: &str, pp: &Prepared, target: &Target, elements: &[u64], m: &Model, limits: Limits) {
    let a = mine_demand(&pp.prog, target, elements, m, limits);
    let f = first_demand(&pp.fp, &pp.info, target, elements, m, limits);
    let brief = |r: &DRes| match r {
        DRes::Ok(v, w) => format!("ok {:?} work ({}, {})", &v[..v.len().min(4)], w.elements, w.terms),
        DRes::Err(e) => format!("refused {}", e.name()),
        DRes::Panic(m) => format!("PANIC {m}"),
    };
    let allowed = match allowed(&pp.prog, target, elements, m, limits) {
        Ok(_) => "success".to_string(),
        Err(set) => format!("{:?}", set.iter().map(|e| e.name()).collect::<Vec<_>>()),
    };
    println!("  {what}\n      ref2  {}\n      first {}\n      text allows {allowed}", brief(&a.res), brief(&f.res));
    t.case(what, pp, target, elements, m, limits);
}

/// The places §9.4 leaves open, each run on both implementations with the answer printed.
#[test]
fn text_gap_probes() {
    let mut t = Tally::default();
    // D1: a committed writer's value in its dtype but outside [lo, hi].
    let mut b = ProgBuilder::new(HB, 16);
    let w = b.fixed_state("w", DType::I16, &[1], -5, 5, false);
    let pre = b.block("pre", vec![]);
    let one = b.konst(DType::I16, &[1], &[1]);
    let n0 = b.node(pre, Prim::Add, &[Ref::State(w), Ref::Const(one)], fixed(DType::I32, &[1]), false);
    let n1 = b.node(pre, Prim::StateWrite { state: w }, &[Ref::Node(n0)], fixed(DType::I16, &[1]), true);
    let n2 = b.node(pre, Prim::Cast, &[Ref::Node(n1)], fixed(DType::I32, &[1]), true);
    b.carry_out(pre, &[n2]);
    let post = b.block("post", vec![fixed(DType::I32, &[1])]);
    let k0 = b.node(post, Prim::Clamp { lo: -100, hi: 100 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1]), true);
    b.schedule(pre, &[], post, k0);
    let prog = b.finish();
    let pp = prepare(&prog).expect("normal form");
    let tokens = [3u64, 1, 4, 1, 5];
    let honest = Model::from_run(&prog, &Params::new(), &tokens, &|q| q == 0).unwrap();
    let mut bad = honest.clone();
    bad.faults.insert(Question::Node { ctx: Ctx { pos: 2, occ: 0 }, node: n1, i: 0 }, Fault::Value(100));
    println!("D1 — a committed writer answering 100 for a state in [-5, 5]:");
    show(&mut t, "state_after(2, w): the writer's leaf is the target's element", &pp, &Target::StateAfter { pos: 2, state: w, layer: None }, &[0], &bad, Limits::UNLIMITED);
    show(&mut t, "node(3, pre, Add): State(w) at 3 replays the writer's leaf at 2", &pp, &Target::Node { ctx: Ctx { pos: 3, occ: 0 }, node: n0 }, &[0], &bad, Limits::UNLIMITED);
    show(&mut t, "node(2, pre, Cast): reads the writer's leaf as a Node operand", &pp, &Target::Node { ctx: Ctx { pos: 2, occ: 0 }, node: n2 }, &[0], &bad, Limits::UNLIMITED);

    // D3: a carried walk that never finds a value, under a small max_terms.
    let prog = edge_program(false);
    let pp = prepare(&prog).expect("normal form");
    let tokens: Vec<u64> = (0..20).collect();
    let none = Model::from_run(&prog, &Params::new(), &tokens, &|_| false).unwrap();
    let some = Model::from_run(&prog, &Params::new(), &tokens, &|q| q == 0).unwrap();
    println!("D3 — an instance nothing writes, carried from position 9:");
    for (m, what) in [(&none, "nothing supplied"), (&some, "supplied at 0")] {
        for limits in [Limits::UNLIMITED, Limits { max_elements: 0, max_terms: 3 }, Limits { max_elements: 0, max_terms: 9 }] {
            show(
                &mut t,
                &format!("state_after(9, u), {what}, limits ({}, {})", limits.max_elements, limits.max_terms),
                &pp,
                &Target::StateAfter { pos: 9, state: 1, layer: None },
                &[0, 1],
                m,
                limits,
            );
        }
    }
    // The same need twice: two elements of one position are distinct needs, one position twice is not.
    show(&mut t, "state_after(9, u) elements [0, 0, 1, 1]", &pp, &Target::StateAfter { pos: 9, state: 1, layer: None }, &[0, 0, 1, 1], &some, Limits::UNLIMITED);
    show(&mut t, "node(9, post, Add): reads State(u) twice (two elements)", &pp, &Target::Node { ctx: Ctx { pos: 9, occ: 3 }, node: 0 }, &[0, 1], &some, Limits::UNLIMITED);

    // A replay across the whole history bound: position history_bound − 1, supplied at 0 only.
    let deep = Model::from_run(&prog, &Params::new(), &tokens[..3], &|q| q == 0).unwrap();
    println!("A replay from history_bound − 1 down to 0 (supplied at 0 only, the source knows nothing else):");
    show(&mut t, "state_after(hb − 1, s)", &pp, &Target::StateAfter { pos: HB as u64 - 1, state: 0, layer: None }, &[0, 1], &deep, Limits::UNLIMITED);
    show(&mut t, "state_after(hb − 1, u)", &pp, &Target::StateAfter { pos: HB as u64 - 1, state: 1, layer: None }, &[0, 1], &deep, Limits::UNLIMITED);

    // D10: an instance its per_layer has but no block references.
    let mut b = ProgBuilder::new(HB, 16);
    let v = b.fixed_state("v", DType::I16, &[1], -7, 7, true);
    let pre = b.block("pre", vec![]);
    let c = b.konst(DType::I32, &[1], &[5]);
    let n0 = b.node(pre, Prim::Cast, &[Ref::Const(c)], fixed(DType::I32, &[1]), true);
    b.carry_out(pre, &[n0]);
    let la = b.block("la", vec![fixed(DType::I32, &[1])]);
    let a0 = b.node(la, Prim::Add, &[Ref::CarryIn(0), Ref::State(v)], fixed(DType::I64, &[1]), false);
    b.node(la, Prim::StateWrite { state: v }, &[Ref::Node(a0)], fixed(DType::I16, &[1]), false);
    let a2 = b.node(la, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::Node(a0)], fixed(DType::I32, &[1]), true);
    b.carry_out(la, &[a2]);
    let lb = b.block("lb", vec![fixed(DType::I32, &[1])]);
    let b0 = b.node(lb, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1]), true);
    b.carry_out(lb, &[b0]);
    let post = b.block("post", vec![fixed(DType::I32, &[1])]);
    let k0 = b.node(post, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[1]), true);
    b.schedule(pre, &[la, lb], post, k0);
    let prog = b.finish();
    let pp = prepare(&prog).expect("normal form");
    let m = Model::from_run(&prog, &Params::new(), &[0, 0, 0, 0], &|_| true).unwrap();
    let mut zeros = m.clone();
    for p in 0..=4u64 {
        zeros.states.insert((p, v, Some(1)), vec![0]);
    }
    println!("D10 — state_after of instance (v, layer 1), which its per_layer has and no block references:");
    show(&mut t, "state_after(3, v, 1), the source knows nothing of it", &pp, &Target::StateAfter { pos: 3, state: v, layer: Some(1) }, &[0], &m, Limits::UNLIMITED);
    show(&mut t, "state_after(3, v, 1), the source answers zeros", &pp, &Target::StateAfter { pos: 3, state: v, layer: Some(1) }, &[0], &zeros, Limits::UNLIMITED);
    show(&mut t, "state_after(3, v, 0), the referenced instance", &pp, &Target::StateAfter { pos: 3, state: v, layer: Some(0) }, &[0], &m, Limits::UNLIMITED);

    t.report("text-gap probes");
    assert!(t.clean());
}
