//! Admission (`tir_admit_v1`, 04b §10.3 with §7 and §8): this crate's reading of the text against
//! the first implementation, as a black box.

mod common;

use common::bridge::*;
use common::*;
use misaka_palw_tir as first;
use misaka_palw_tir_ref2 as ref2;
use ref2::admit::AdmitInputs;

fn legacy() -> AdmitInputs {
    let c = first::admit::TirCeilingsV1::legacy_court_v1();
    AdmitInputs { tile_len: 64, h_chunk: 256, ceilings: ceilings_from(&c) }
}

#[test]
fn z_print_legacy_ceilings() {
    println!("legacy_court_v1: {:?}", first::admit::TirCeilingsV1::legacy_court_v1());
}

fn corpus() -> Vec<(String, Vec<u8>)> {
    let dir = vectors_dir().join("programs");
    let mut files: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .iter()
        .map(|f| {
            let d = read_json(f);
            (d["name"].as_str().unwrap().to_string(), hex_decode(d["program_borsh_hex"].as_str().unwrap()))
        })
        .collect()
}

/// Prints the first difference between two views.
pub fn diff(a: &AdmittedView, f: &AdmittedView) -> Vec<String> {
    let mut out = Vec::new();
    if a.intervals != f.intervals {
        for (b, (x, y)) in a.intervals.iter().zip(f.intervals.iter()).enumerate() {
            for (i, (u, v)) in x.iter().zip(y.iter()).enumerate() {
                if u != v {
                    out.push(format!("interval block {b} node {i}: ref2 {u:?} first {v:?}"));
                }
            }
        }
    }
    if a.node_costs != f.node_costs {
        for (b, (x, y)) in a.node_costs.iter().zip(f.node_costs.iter()).enumerate() {
            for (i, (u, v)) in x.iter().zip(y.iter()).enumerate() {
                if u != v {
                    out.push(format!("node cost block {b} node {i}: ref2 {u:?} first {v:?}"));
                }
            }
        }
    }
    if a.position != f.position {
        out.push(format!("position: ref2 {:?} first {:?}", a.position, f.position));
    }
    if a.cones.len() != f.cones.len() {
        out.push(format!("cone count: ref2 {} first {}", a.cones.len(), f.cones.len()));
    }
    for (x, y) in a.cones.iter().zip(f.cones.iter()) {
        if x != y {
            let mut parts = Vec::new();
            if (x.block, x.node) != (y.block, y.node) {
                parts.push(format!("key ref2 ({}, {}) first ({}, {})", x.block, x.node, y.block, y.node));
            }
            if x.nodes != y.nodes {
                parts.push(format!("nodes ref2 {:?} first {:?}", x.nodes, y.nodes));
            }
            if x.leaves != y.leaves {
                parts.push(format!("leaves ref2 {:?} first {:?}", x.leaves, y.leaves));
            }
            if x.whole != y.whole {
                parts.push(format!("whole ref2 {:?} first {:?}", x.whole, y.whole));
            }
            if x.tiles != y.tiles {
                parts.push(format!("tiles ref2 {} first {}", x.tiles, y.tiles));
            }
            if x.tile != y.tile {
                parts.push(format!("tile ref2 {:?} first {:?}", x.tile, y.tile));
            }
            if x.tile_opened_bytes != y.tile_opened_bytes {
                parts.push(format!("tile_opened ref2 {} first {}", x.tile_opened_bytes, y.tile_opened_bytes));
            }
            if x.operands != y.operands {
                parts.push(format!("operands ref2 {} first {}", x.operands, y.operands));
            }
            if x.h_reductions != y.h_reductions {
                parts.push(format!("h_reductions ref2 {:?} first {:?}", x.h_reductions, y.h_reductions));
            }
            if x.chunk != y.chunk {
                parts.push(format!("chunk ref2 {:?} first {:?}", x.chunk, y.chunk));
            }
            if x.chunk_opened_bytes != y.chunk_opened_bytes {
                parts.push(format!("chunk_opened ref2 {:?} first {:?}", x.chunk_opened_bytes, y.chunk_opened_bytes));
            }
            out.push(format!("cone ({}, {}): {}", x.block, x.node, parts.join("; ")));
        }
    }
    if a.states != f.states {
        out.push(format!("states: ref2 {:?}\n            first {:?}", a.states, f.states));
    }
    if a.checkpoint_interval != f.checkpoint_interval {
        out.push(format!("checkpoint_interval: ref2 {} first {}", a.checkpoint_interval, f.checkpoint_interval));
    }
    if a.cone_work != f.cone_work {
        out.push(format!("cone_work: ref2 {} first {}", a.cone_work, f.cone_work));
    }
    out
}

#[test]
fn corpus_programs_under_the_legacy_ceilings() {
    let inputs = legacy();
    for (name, bytes) in corpus() {
        let a = admit_mine(&bytes, &inputs);
        let f = admit_first(&bytes, &inputs);
        match (&a, &f) {
            (AdmitOutcome::Admitted(x), AdmitOutcome::Admitted(y)) => {
                let d = diff(x, y);
                println!("{name}: both admit; {} differences", d.len());
                for l in d.iter().take(12) {
                    println!("    {l}");
                }
            }
            _ => println!("{name}: ref2 {:?}\n      first {:?}", short(&a), short(&f)),
        }
    }
}

pub fn short(o: &AdmitOutcome) -> String {
    match o {
        AdmitOutcome::Admitted(v) => format!("admitted (C = {}, cone work {})", v.checkpoint_interval, v.cone_work),
        x => format!("{x:?}"),
    }
}

#[test]
fn qwen25_1_5b_shaped_decoder() {
    use ref2::codec::{decode_canonical, encode};
    for (label, commit) in [("committed attention", true), ("few commit points", false)] {
        let shape = common::qwen::Shape { commit_attention: commit, ..common::qwen::QWEN25_1_5B };
        let p = common::qwen::build(&shape);
        let bytes = encode(&p);
        decode_canonical(&bytes).unwrap_or_else(|e| panic!("ref2 refuses its own Qwen program: {e}"));
        for (tl, hc) in [(64u32, 256u32), (1, 1), (4096, 65536), (65536, 2)] {
            let mut inputs = legacy();
            inputs.tile_len = tl;
            inputs.h_chunk = hc;
            let t0 = std::time::Instant::now();
            let a = admit_mine(&bytes, &inputs);
            let t1 = t0.elapsed();
            let f = admit_first(&bytes, &inputs);
            match (&a, &f) {
                (AdmitOutcome::Admitted(x), AdmitOutcome::Admitted(y)) => {
                    let d = diff(x, y);
                    println!(
                        "qwen ({label}, {} bytes, tile_len {tl}, h_chunk {hc}): both admit, C = {}, cone work {}, {} cones; {} differences (ref2 {:?})",
                        bytes.len(),
                        x.checkpoint_interval,
                        x.cone_work,
                        x.cones.len(),
                        d.len(),
                        t1
                    );
                    for l in d.iter().take(10) {
                        println!("    {l}");
                    }
                    assert!(d.is_empty());
                }
                _ => {
                    println!("qwen ({label}, tile_len {tl}, h_chunk {hc}): ref2 {} / first {}", short(&a), short(&f));
                    assert_eq!(a, f);
                }
            }
        }
    }
}

#[test]
fn z_limit_names_one_ceiling_at_a_time() {
    use ref2::codec::encode;
    let qwen = encode(&common::qwen::build(&common::qwen::QWEN25_1_5B));
    let mut progs = corpus();
    progs.push(("qwen".into(), qwen));
    let fields = [
        "max_tile_macs",
        "max_tile_transcendentals",
        "max_tile_opened_bytes",
        "max_tile_operands",
        "max_position_macs",
        "max_position_transcendentals",
        "max_state_bytes",
        "max_step_leaves",
        "max_checkpoint_interval",
        "max_cone_work",
    ];
    for f in fields {
        for cap in [0u64, 1, 1000] {
            let mut inputs = legacy();
            let c = &mut inputs.ceilings;
            match f {
                "max_tile_macs" => c.max_tile_macs = cap,
                "max_tile_transcendentals" => c.max_tile_transcendentals = cap,
                "max_tile_opened_bytes" => c.max_tile_opened_bytes = cap,
                "max_tile_operands" => c.max_tile_operands = cap,
                "max_position_macs" => c.max_position_macs = cap,
                "max_position_transcendentals" => c.max_position_transcendentals = cap,
                "max_state_bytes" => c.max_state_bytes = cap,
                "max_step_leaves" => c.max_step_leaves = cap,
                "max_checkpoint_interval" => c.max_checkpoint_interval = cap as u32,
                _ => c.max_cone_work = cap,
            }
            for (name, bytes) in &progs {
                let a = replay_name(admit_mine(bytes, &inputs));
                let fo = admit_first(bytes, &inputs);
                if a != fo {
                    println!("{f} = {cap}, {name}: ref2 {} / first {}", short(&a), short(&fo));
                }
                assert_eq!(a, fo, "{f} = {cap}, {name}");
            }
        }
    }
}

/// A replay refusal (C_j = 0) under the first implementation's single name.
pub fn replay_name(o: AdmitOutcome) -> AdmitOutcome {
    o
}

// =============================================================== random programs, inputs, mutations

use common::progen::{GenCfg, R, gen_program, pick};
use rand::{Rng, SeedableRng};
use std::collections::BTreeMap;

fn scale() -> usize {
    std::env::var("TIR_REF2_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

/// Inputs: the legacy ceilings with other tile/chunk sizes, random ceilings (0, 1, small, the
/// legacy value, unbounded — to trip each ceiling), and sometimes inputs out of range.
fn rand_inputs(rng: &mut R) -> AdmitInputs {
    let mut i = legacy();
    i.tile_len = pick(rng, &[64u32, 64, 1, 7, 1000, 65536]);
    i.h_chunk = pick(rng, &[256u32, 256, 1, 2, 64, 65536]);
    if rng.gen_bool(0.5) {
        let c = &mut i.ceilings;
        let pickc = |rng: &mut R, legacy: u64| -> u64 {
            match rng.gen_range(0..10) {
                0 => 0,
                1 => 1,
                2 => rng.gen_range(2..1000),
                3 => rng.gen_range(1000..100_000),
                4 => u64::MAX,
                _ => legacy,
            }
        };
        c.max_tile_macs = pickc(rng, c.max_tile_macs);
        c.max_tile_transcendentals = pickc(rng, c.max_tile_transcendentals);
        c.max_tile_opened_bytes = pickc(rng, c.max_tile_opened_bytes);
        c.max_tile_operands = pickc(rng, c.max_tile_operands);
        c.max_position_macs = pickc(rng, c.max_position_macs);
        c.max_position_transcendentals = pickc(rng, c.max_position_transcendentals);
        c.max_state_bytes = pickc(rng, c.max_state_bytes);
        c.max_step_leaves = pickc(rng, c.max_step_leaves);
        c.max_checkpoint_interval = pickc(rng, c.max_checkpoint_interval as u64).min(u32::MAX as u64) as u32;
        c.max_cone_work = pickc(rng, c.max_cone_work);
    }
    if rng.gen_bool(0.02) {
        i.tile_len = pick(rng, &[0u32, 65537, u32::MAX]);
    }
    if rng.gen_bool(0.02) {
        i.h_chunk = pick(rng, &[0u32, 3, 131072, 100]);
    }
    i
}

#[derive(Default)]
struct AdmitTally {
    cases: usize,
    both_admit: usize,
    both_program: usize,
    both_exceeds: usize,
    both_inputs: usize,
    exceeds_same: usize,
    exceeds_in_set: usize,
    program_same_class: usize,
    by_limit: BTreeMap<String, usize>,
    disagreements: Vec<String>,
    /// Coverage of the admitted programs: cones, dissected cones, states, split replays,
    /// multi-state closures, cones with a history leaf.
    cov: BTreeMap<&'static str, usize>,
}

impl AdmitTally {
    fn record(&mut self, what: &str, bytes: &[u8], inputs: &AdmitInputs) {
        self.cases += 1;
        let a = admit_mine(bytes, inputs);
        let f = admit_first(bytes, inputs);
        self.record_outcomes(what, bytes, inputs, &a, &f)
    }

    fn record_outcomes(&mut self, what: &str, bytes: &[u8], inputs: &AdmitInputs, a: &AdmitOutcome, f: &AdmitOutcome) {
        match (a, f) {
            (AdmitOutcome::Admitted(x), AdmitOutcome::Admitted(y)) => {
                self.both_admit += 1;
                *self.cov.entry("cones").or_default() += x.cones.len();
                *self.cov.entry("dissected cones").or_default() += x.cones.iter().filter(|c| c.chunk.is_some()).count();
                *self.cov.entry("cones with a history leaf").or_default() +=
                    x.cones.iter().filter(|c| c.leaves.iter().any(|l| matches!(l, ref2::admit::Leaf::History(_)))).count();
                *self.cov.entry("Fixed states replayed").or_default() += x.states.len();
                *self.cov.entry("replays split into groups").or_default() += x.states.iter().filter(|s| s.2 > 1).count();
                *self.cov.entry("closures of ≥ 2 states").or_default() += x.states.iter().filter(|s| s.1.len() > 1).count();
                *self.cov.entry("C_j below the cap").or_default() += x.states.iter().filter(|s| s.4 < 65536).count();
                let d = diff(x, y);
                if !d.is_empty() {
                    self.disagreements.push(format!(
                        "{what}: both admit, {} differences: {}",
                        d.len(),
                        d[..d.len().min(3)].join(" | ")
                    ));
                }
            }
            (AdmitOutcome::Program(x), AdmitOutcome::Program(y)) => {
                self.both_program += 1;
                if x == y {
                    self.program_same_class += 1;
                } else {
                    // A program that breaks several rules may report any of their classes.
                    let set = ref2::codec::decode_violations(bytes).err().unwrap_or_default();
                    if !set.contains(y) {
                        self.disagreements.push(format!("{what}: program refusals ref2 {x:?} first {y:?} (violation set {set:?})"));
                    }
                }
            }
            (AdmitOutcome::Exceeds(l1, v1), AdmitOutcome::Exceeds(l2, v2)) => {
                self.both_exceeds += 1;
                *self.by_limit.entry(l2.clone()).or_default() += 1;
                if (l1, v1) == (l2, v2) {
                    self.exceeds_same += 1;
                } else {
                    // Since the A2 fix the check order is stated, so the name and the value are
                    // determined; a mismatch is a disagreement even when the first implementation's
                    // limit is another broken ceiling (which alone would not be normative).
                    let p = ref2::codec::decode_canonical(bytes).unwrap();
                    let set = ref2::admit::broken_ceilings(&p, inputs);
                    if set.iter().any(|(l, v)| *l == l2.as_str() && v == v2) {
                        self.exceeds_in_set += 1;
                    }
                    self.disagreements.push(format!("{what}: ref2 {l1} {v1} / first {l2} {v2}; ref2's broken ceilings {set:?}"));
                }
            }
            (AdmitOutcome::Inputs, AdmitOutcome::Inputs) => self.both_inputs += 1,
            _ => self.disagreements.push(format!("{what}: ref2 {} / first {}", short(a), short(f))),
        }
    }

    fn report(&self, name: &str) {
        println!(
            "{name}: {} cases — {} both admit (every derived quantity compared), {} both refuse the program ({} same class), {} both refuse past a ceiling ({} same limit and value, {} another broken ceiling — each a disagreement), {} both refuse the inputs; {} disagreements",
            self.cases,
            self.both_admit,
            self.both_program,
            self.program_same_class,
            self.both_exceeds,
            self.exceeds_same,
            self.exceeds_in_set,
            self.both_inputs,
            self.disagreements.len()
        );
        println!("    refusals by limit: {:?}", self.by_limit);
        println!("    coverage of the admitted programs: {:?}", self.cov);
        for d in self.disagreements.iter().take(20) {
            println!("  DISAGREE {d}");
        }
    }
}

#[test]
fn random_programs_and_inputs() {
    let n = 1500 * scale() as u64;
    let mut t = AdmitTally::default();
    let mut legacy_t = AdmitTally::default();
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0xAD41_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let bytes = ref2::codec::encode(&g.prog);
        legacy_t.record(&format!("seed {seed} legacy"), &bytes, &legacy());
        for k in 0..4 {
            let inputs = rand_inputs(&mut rng);
            t.record(&format!("seed {seed} inputs #{k} {inputs:?}"), &bytes, &inputs);
        }
    }
    legacy_t.report("random programs, legacy ceilings");
    t.report("random programs, random inputs");
    assert!(legacy_t.disagreements.is_empty() && t.disagreements.is_empty());
}

#[test]
#[ignore]
fn investigate() {
    let seeds: Vec<u64> = std::env::var("SEEDS").unwrap_or("12".into()).split(',').map(|s| s.parse().unwrap()).collect();
    let safe = std::env::var("SAFE").is_ok();
    for seed in seeds {
        let mut rng = R::seed_from_u64(if safe { 0x5AFE_0000 } else { 0xAD41_0000 } + seed);
        let g = gen_program(&mut rng, GenCfg { range_safe: safe, ..GenCfg::default() });
        let p = &g.prog;
        let bytes = ref2::codec::encode(p);
        let inputs = legacy();
        let a = admit_mine(&bytes, &inputs);
        let f = admit_first(&bytes, &inputs);
        println!("=== seed {seed}");
        for (b, blk) in p.blocks.iter().enumerate() {
            for (i, n) in blk.nodes.iter().enumerate() {
                let (nodes, leaves) = ref2::admit::cone(p, b, i);
                let work: u64 = nodes.iter().map(|&k| 1 + blk.nodes[k as usize].inputs.len() as u64).sum();
                let tag = if n.commit { "C" } else { " " };
                println!(
                    "  b{b} n{i:<3} {tag} {:<24} ins {:?} {}",
                    format!("{:?}", n.prim).chars().take(24).collect::<String>(),
                    n.inputs,
                    if n.commit || matches!(n.prim, ref2::Prim::StateWrite { .. }) {
                        format!("cone {:?} work {work} leaves {:?}", nodes, leaves)
                    } else {
                        String::new()
                    }
                );
            }
        }
        for (j, s) in p.states.iter().enumerate() {
            println!("  state {j}: {:?} {:?} {:?} per_layer {}", s.kind, s.dtype, s.shape, s.per_layer);
        }
        println!("  schedule pre {} layers {:?} post {}", p.schedule.pre, p.schedule.layers, p.schedule.post);
        if let (AdmitOutcome::Admitted(x), AdmitOutcome::Admitted(y)) = (&a, &f) {
            println!("  cone work ref2 {} first {}", x.cone_work, y.cone_work);
            println!("  states ref2  {:?}", x.states);
            println!("  states first {:?}", y.states);
        } else {
            println!("  ref2 {} first {}", short(&a), short(&f));
        }
    }
}

/// The §10.3 group rule on hand-built update cones: which of free / aligned nodes permit a split.
#[test]
fn group_rule_probes() {
    use ref2::build::{ProgBuilder, fixed};
    use ref2::{DType, Prim, Ref};
    // S: Fixed i32 [4, 3]; P: param i32 [4, 3]; Q: param i32 [3] (free, broadcast).
    let build = |case: &str| {
        let mut b = ProgBuilder::new(1 << 18, 16);
        let s = b.fixed_state("s", DType::I32, &[4, 3], -100, 100, false);
        let pp = if case.contains('P') { b.param("p", DType::I32, &[4, 3], false) } else { 0 };
        let q = if case.contains('Q') { b.param("q", DType::I32, &[3], false) } else { 0 };
        let pre = b.block("pre", vec![]);
        let t = fixed(DType::I32, &[4, 3]);
        let w = fixed(DType::I64, &[4, 3]);
        let x = match case {
            // the update depends on no closure state
            "free: StateWrite(Clamp(P))" => b.node(pre, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::Param(pp)], t.clone(), false),
            // aligned: S + Clamp(P) (a free node inside)
            "S + Clamp(P)" => {
                let c = b.node(pre, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::Param(pp)], t.clone(), false);
                b.node(pre, Prim::Add, &[Ref::State(s), Ref::Node(c)], w.clone(), false)
            }
            // aligned: S + P
            "S + P" => b.node(pre, Prim::Add, &[Ref::State(s), Ref::Param(pp)], w.clone(), false),
            // broadcast of a free rank-1 operand
            "S + Q (broadcast)" => b.node(pre, Prim::Add, &[Ref::State(s), Ref::Param(q)], w.clone(), false),
            // not aligned: a reduction over axis 0
            "S + ReduceSum0(S)" => {
                let r = b.node(pre, Prim::ReduceSum { axis: 0 }, &[Ref::State(s)], fixed(DType::I64, &[1, 3]), false);
                b.node(pre, Prim::Add, &[Ref::State(s), Ref::Node(r)], w.clone(), false)
            }
            // a free node whose axis 0 is not G, broadcast into an aligned one
            "S + Clamp(Q)" => {
                let c = b.node(pre, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::Param(q)], fixed(DType::I32, &[3]), false);
                b.node(pre, Prim::Add, &[Ref::State(s), Ref::Node(c)], w.clone(), false)
            }
            _ => unreachable!(),
        };
        b.node(pre, Prim::StateWrite { state: s }, &[Ref::Node(x)], t.clone(), false);
        let o = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], fixed(DType::I32, &[2]), true);
        b.carry_out(pre, &[o]);
        let post = b.block("post", vec![fixed(DType::I32, &[2])]);
        let m = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
        b.schedule(pre, &[], post, m);
        ref2::codec::encode(&b.finish())
    };
    for case in ["free: StateWrite(Clamp(P))", "S + Clamp(P)", "S + P", "S + Q (broadcast)", "S + ReduceSum0(S)", "S + Clamp(Q)"] {
        let bytes = build(case);
        let a = admit_mine(&bytes, &legacy());
        let f = admit_first(&bytes, &legacy());
        let g = |o: &AdmitOutcome| match o {
            AdmitOutcome::Admitted(v) => format!("groups {}", v.states[0].2),
            x => format!("{x:?}"),
        };
        println!("{case:32} ref2 {} / first {}", g(&a), g(&f));
    }
}

/// A C_j of 0 (§10.3): the value a refusal reports when the interval cap is 0, or a component is
/// past the tile ceiling, for MAC-only, transcendental-only and mixed replays.
#[test]
fn replay_refusal_values() {
    use ref2::build::{ProgBuilder, fixed};
    use ref2::{DType, Prim, Ref};
    let build = |case: &str| {
        let mut b = ProgBuilder::new(1 << 18, 16);
        let s = b.fixed_state("s", DType::I32, &[2, 2], -100, 100, false);
        let pre = b.block("pre", vec![]);
        let x = match case {
            "macs" => b.node(pre, Prim::MatMul, &[Ref::State(s), Ref::State(s)], fixed(DType::I64, &[2, 2]), false),
            "trans" => b.node(pre, Prim::IntExp, &[Ref::State(s)], fixed(DType::I32, &[2, 2]), false),
            _ => {
                let m = b.node(pre, Prim::MatMul, &[Ref::State(s), Ref::State(s)], fixed(DType::I64, &[2, 2]), false);
                b.node(pre, Prim::IntExp, &[Ref::Node(m)], fixed(DType::I32, &[2, 2]), false)
            }
        };
        b.node(pre, Prim::StateWrite { state: s }, &[Ref::Node(x)], fixed(DType::I32, &[2, 2]), false);
        let o = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], fixed(DType::I32, &[2]), true);
        b.carry_out(pre, &[o]);
        let post = b.block("post", vec![fixed(DType::I32, &[2])]);
        let m = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
        b.schedule(pre, &[], post, m);
        ref2::codec::encode(&b.finish())
    };
    for case in ["macs", "trans", "mixed"] {
        let bytes = build(case);
        for (label, f) in [
            (
                "interval cap 0",
                Box::new(|i: &mut AdmitInputs| i.ceilings.max_checkpoint_interval = 0) as Box<dyn Fn(&mut AdmitInputs)>,
            ),
            ("tile MACs 1", Box::new(|i: &mut AdmitInputs| i.ceilings.max_tile_macs = 1)),
            ("tile transcendentals 1", Box::new(|i: &mut AdmitInputs| i.ceilings.max_tile_transcendentals = 1)),
        ] {
            let mut inputs = legacy();
            f(&mut inputs);
            let a = admit_mine(&bytes, &inputs);
            let fo = admit_first(&bytes, &inputs);
            println!("{case:6} {label:24} ref2 {} / first {}", short(&a), short(&fo));
        }
    }
}

#[test]
fn range_safe_programs_and_mutations() {
    let n = 1500 * scale() as u64;
    let mut t = AdmitTally::default();
    let mut m = AdmitTally::default();
    let cfg = GenCfg { range_safe: true, ..GenCfg::default() };
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x5AFE_0000 + seed);
        let g = gen_program(&mut rng, cfg);
        let bytes = ref2::codec::encode(&g.prog);
        t.record(&format!("seed {seed} legacy"), &bytes, &legacy());
        for k in 0..3 {
            let inputs = rand_inputs(&mut rng);
            t.record(&format!("seed {seed} inputs #{k} {inputs:?}"), &bytes, &inputs);
        }
        // Mutations: each a different program (or a refused one) under the same inputs.
        for k in 0..4 {
            let mut p = g.prog.clone();
            let what = common::progen::mutate(&mut rng, &mut p);
            let bytes = ref2::codec::encode(&p);
            let inputs = if rng.gen_bool(0.5) { legacy() } else { rand_inputs(&mut rng) };
            m.record(&format!("seed {seed} mutation #{k} {what}"), &bytes, &inputs);
        }
    }
    t.report("range-safe random programs");
    m.report("mutations of range-safe programs");
    assert!(t.disagreements.is_empty() && m.disagreements.is_empty());
}

/// Two update cones of one replay closure sharing a node: is it costed once or per cone?
#[test]
fn shared_node_in_a_replay_closure() {
    use ref2::build::{ProgBuilder, fixed};
    use ref2::{DType, Prim, Ref};
    let mut b = ProgBuilder::new(1 << 18, 16);
    let s1 = b.fixed_state("s1", DType::I32, &[4, 3], -100, 100, false);
    let s2 = b.fixed_state("s2", DType::I32, &[4, 3], -100, 100, false);
    let pre = b.block("pre", vec![]);
    let t = fixed(DType::I32, &[4, 3]);
    let a = b.node(pre, Prim::Add, &[Ref::State(s1), Ref::State(s2)], fixed(DType::I64, &[4, 3]), false);
    let x = b.node(pre, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::Node(a)], t.clone(), false); // shared
    b.node(pre, Prim::StateWrite { state: s1 }, &[Ref::Node(x)], t.clone(), false);
    let y = b.node(pre, Prim::Mul, &[Ref::Node(x), Ref::Node(x)], fixed(DType::I64, &[4, 3]), false);
    b.node(pre, Prim::StateWrite { state: s2 }, &[Ref::Node(y)], t.clone(), false);
    let o = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], fixed(DType::I32, &[2]), true);
    b.carry_out(pre, &[o]);
    let post = b.block("post", vec![fixed(DType::I32, &[2])]);
    let m = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
    b.schedule(pre, &[], post, m);
    let bytes = ref2::codec::encode(&b.finish());
    let am = admit_mine(&bytes, &legacy());
    let af = admit_first(&bytes, &legacy());
    for (who, o) in [("ref2", &am), ("first", &af)] {
        if let AdmitOutcome::Admitted(v) = o {
            println!("{who}: states {:?} cone work {}", v.states, v.cone_work);
        } else {
            println!("{who}: {}", short(o));
        }
    }
    assert_eq!(am, af);
}

/// A per-layer state written by two layer blocks with different update costs (§10.3: "over the
/// blocks that write j, the smallest C_j counts") — which block's closure and cost are reported.
#[test]
fn a_state_written_by_two_layer_blocks() {
    use ref2::build::{ProgBuilder, fixed};
    use ref2::{DType, Prim, Ref};
    for heavy_first in [true, false] {
        let mut b = ProgBuilder::new(1 << 18, 16);
        let s = b.fixed_state("s", DType::I32, &[4, 3], -100, 100, true);
        let w = b.param("w", DType::I8, &[3, 3], true);
        let pre = b.block("pre", vec![]);
        let o = b.node(pre, Prim::Iota { axis: 0, start: 0, step: 1 }, &[], fixed(DType::I32, &[2]), true);
        b.carry_out(pre, &[o]);
        let layer = |b: &mut ProgBuilder, name: &str, heavy: bool| {
            let l = b.block(name, vec![fixed(DType::I32, &[2])]);
            let x = if heavy {
                let m = b.node(l, Prim::MatMul, &[Ref::State(s), Ref::Param(w)], fixed(DType::I64, &[4, 3]), false);
                b.node(l, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::Node(m)], fixed(DType::I32, &[4, 3]), false)
            } else {
                b.node(l, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::State(s)], fixed(DType::I32, &[4, 3]), false)
            };
            b.node(l, Prim::StateWrite { state: s }, &[Ref::Node(x)], fixed(DType::I32, &[4, 3]), false);
            let c = b.node(l, Prim::Clamp { lo: -9, hi: 9 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
            b.carry_out(l, &[c]);
            l
        };
        let (la, lb) = if heavy_first {
            (layer(&mut b, "heavy", true), layer(&mut b, "light", false))
        } else {
            (layer(&mut b, "light", false), layer(&mut b, "heavy", true))
        };
        let post = b.block("post", vec![fixed(DType::I32, &[2])]);
        let m = b.node(post, Prim::Clamp { lo: -5, hi: 5 }, &[Ref::CarryIn(0)], fixed(DType::I32, &[2]), true);
        b.schedule(pre, &[la, lb], post, m);
        let bytes = ref2::codec::encode(&b.finish());
        let mut inputs = legacy();
        inputs.ceilings.max_tile_macs = 1000; // makes C_j differ between the two blocks
        let a = admit_mine(&bytes, &inputs);
        let f = admit_first(&bytes, &inputs);
        for (who, o) in [("ref2", &a), ("first", &f)] {
            if let AdmitOutcome::Admitted(v) = o {
                println!("heavy first {heavy_first}: {who} states {:?} C {}", v.states, v.checkpoint_interval);
            } else {
                println!("heavy first {heavy_first}: {who} {}", short(o));
            }
        }
        assert_eq!(a, f);
    }
}
