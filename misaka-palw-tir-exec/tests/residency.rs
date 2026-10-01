//! **A residency computes what the mapping computes** (ADR-0112 I-1 for IR classes, the executor's
//! half; `docs/design/palw/tir/runtime-residency.md`).
//!
//! Every routed and gathered param served by rows (`TirParams::from_map_served`, tiers at
//! `pin_below_bytes = 0`, so every row-addressed param of these small programs is served), the rest
//! bound — against the REFERENCE evaluator, through the differential harness: logits, every commit
//! point, the run state after every step, success versus failure and the failure's class, and on
//! some runs every node's value. Over 6,000 random programs (whose gathers take every axis, batch
//! split and index source, out-of-range indices included), the corpus programs under calibrated and
//! hostile weights, the golden program vectors, and a 64-expert mixture whose route selects two
//! stacks a layer. Off the `every_node` sink — the one reader the tiers do not plan for — no served
//! instance is ever read whole.
//!
//! And the tiers themselves, read off the dataflow: routed when an index is computed from the
//! weights, gathered when it is a function of the token, the position or a history of tokens,
//! pinned otherwise, with the reasons.

#[path = "../../misaka-palw-tir/tests/common/mod.rs"]
mod tircommon;

mod common;
mod residency_common;

use std::path::PathBuf;

use common::progen::{GenCfg, R, gen_program, pick};
use common::{Opts, Outcome, differential};
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, INPUT_POS, INPUT_TOKEN};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, TirProgramV1};
use misaka_palw_tir_exec::tiers::{TirPinnedWhyV1, weight_taint_v1};
use misaka_palw_tir_exec::{TirParams, TirPlan, TirTierRulesV1, TirTierV1, TirTiersV1};
use rand::{Rng, SeedableRng};
use residency_common::many_expert_moe;
use tircommon::Lcg;
use tircommon::models::*;

const ALL_ROWS: TirTierRulesV1 = TirTierRulesV1 { pin_below_bytes: 0 };

fn rows(every_node: bool) -> Opts {
    Opts { every_node, rows: true, ..Default::default() }
}

fn tokens(p: &TirProgramV1, n: usize, seed: u64) -> Vec<u32> {
    let mut rng = Lcg(seed ^ 0x5eed);
    (0..n).map(|_| (rng.next_u64() % p.token_bound as u64) as u32).collect()
}

/// The golden program vectors (`consensus-vectors/tir-v1/programs`), with their params.
fn goldens() -> Vec<(String, TirProgramV1, MapParams)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../consensus-vectors/tir-v1/programs");
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir).expect("vectors").map(|e| e.unwrap().path()).collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            let program =
                TirProgramV1::decode_canonical(&common::hex_decode(v["program_borsh_hex"].as_str().unwrap())).expect("canonical");
            let mut params = MapParams::default();
            for p in v["params"].as_array().unwrap() {
                let j = p["param"].as_u64().unwrap() as u16;
                let layer = p["layer"].as_u64().map(|l| l as u16);
                let d = &program.params[j as usize];
                let shape: Vec<usize> = d.shape.iter().map(|x| *x as usize).collect();
                let bytes = common::hex_decode(p["le_hex"].as_str().unwrap());
                params.tensors.insert((j, layer), Tensor::from_le_bytes(d.dtype, &shape, &bytes).unwrap());
            }
            (v["name"].as_str().unwrap().to_string(), program, params)
        })
        .collect()
}

fn tier_of(p: &TirProgramV1, tiers: &TirTiersV1, name: &str) -> TirTierV1 {
    let j = p.param_index(name).unwrap_or_else(|| panic!("no param {name}"));
    tiers.params[j as usize].tier
}

#[test]
fn the_tiers_are_read_off_the_dataflow_of_the_corpus() {
    let (moe, _) = moe_program();
    let t = TirTiersV1::of(&moe, ALL_ROWS);
    for name in ["blk.gate_exps.w", "blk.up_exps.w", "blk.down_exps.w"] {
        assert_eq!(tier_of(&moe, &t, name), TirTierV1::Routed, "{name}: the route's stacks");
    }
    assert_eq!(tier_of(&moe, &t, "tok_embd"), TirTierV1::Gathered, "the embedding is gathered by the token");
    for name in ["tok_embd.lift", "blk.norm.g", "blk.router.w", "blk.shared_gate.w", "output.w"] {
        assert_eq!(tier_of(&moe, &t, name), TirTierV1::Pinned(TirPinnedWhyV1::Dense), "{name}: read whole every forward");
    }
    // Two of the corpus's four experts a token, in each of its two layers.
    let gate = &t.params[moe.param_index("blk.gate_exps.w").unwrap() as usize];
    assert_eq!((gate.rows, gate.per_forward, gate.instances.len()), (4, 2, 2));
    // At the node's default rules a fixture this small is pinned whole: no row is worth a read.
    let small = TirTiersV1::of(&moe, TirTierRulesV1::default());
    assert!(small.params.iter().all(|p| !p.tier.is_rows()), "every instance of the corpus mixture is under a MiB");
    assert_eq!(tier_of(&moe, &small, "blk.gate_exps.w"), TirTierV1::Pinned(TirPinnedWhyV1::Small));

    let (dense_p, _) = dense(&[HISTORY_BOUND_V1_SMALL, 3]);
    let t = TirTiersV1::of(&dense_p, ALL_ROWS);
    assert_eq!(tier_of(&dense_p, &t, "tok_embd"), TirTierV1::Gathered);
    // The two-level RoPE tables are gathered by the position's high and low bits.
    for name in ["rope.g.cos_hi", "rope.g.sin_lo", "rope.l.cos_lo"] {
        assert_eq!(tier_of(&dense_p, &t, name), TirTierV1::Gathered, "{name}: a table of positions");
    }
    assert!(t.params.iter().all(|p| p.tier != TirTierV1::Routed), "a dense decoder routes nothing");
    for (name, p) in [("gdn", gdn_program(true).0), ("mamba2", mamba2_program().0)] {
        let t = TirTiersV1::of(&p, ALL_ROWS);
        assert!(t.params.iter().all(|x| x.tier != TirTierV1::Routed), "{name}: a recurrence routes nothing");
    }
}

#[test]
fn a_many_expert_mixture_routes_its_stacks_and_its_reshaped_scales_and_floors_at_one_token() {
    let (p, _) = many_expert_moe(64, 2, 4);
    let t = TirTiersV1::of(&p, ALL_ROWS);
    for name in ["blk.gate_exps.w", "blk.gate_exps.m", "blk.up_exps.w", "blk.down_exps.w"] {
        assert_eq!(tier_of(&p, &t, name), TirTierV1::Routed, "{name}");
    }
    // The flat scales are routed through their [E, EF] view: a row is one expert's EF multipliers.
    let m = &t.params[p.param_index("blk.gate_exps.m").unwrap() as usize];
    assert_eq!((m.rows, m.unit, m.per_forward), (64, 8, 2));
    let a = t.arithmetic();
    // One expert: gate, up and down codes (128 bytes each) and its eight i64 multipliers (64).
    let expert = 3 * 128 + 64;
    assert_eq!(a.routed_bytes, 4 * 64 * expert, "four layers of 64 experts");
    assert_eq!(a.routed_token_bytes, 4 * 2 * expert, "two experts in each of four layers");
    assert_eq!(a.in_flight_bytes, 2 * expert, "one route group's admission: a layer's two experts");
    assert_eq!(a.floor_bytes, a.pinned_bytes + a.routed_token_bytes + a.in_flight_bytes);
    assert_eq!(a.gathered_bytes, 48 * 16, "the embedding table");
    assert_eq!(a.weight_bytes, a.pinned_bytes + a.routed_bytes + a.gathered_bytes);
    assert!(a.floor_bytes < a.fifth_bytes, "a fifth of this mixture holds more than its floor");
    assert!(a.routed_capacity(a.fifth_bytes) < a.routed_bytes / 4, "and at a fifth well under a quarter of its experts");
    // The expected union of the experts nine forwards read: 64 · (1 − (62/64)^9) of each stack.
    let union = t.routed_union_bytes(9) as f64;
    let want = 4.0 * 64.0 * (1.0 - (62.0f64 / 64.0).powi(9)) * expert as f64;
    assert!((union - want).abs() <= 1.0, "{union} vs {want}");
}

#[test]
fn the_taint_tells_a_route_from_a_history_of_tokens() {
    let mut pb = ProgramBuilder::new(64, HISTORY_BOUND_V1_SMALL);
    let table = pb.param("ngram.table", DType::I8, &[257, 4], false);
    let routed = pb.param("routed.table", DType::I8, &[64, 4], false);
    let stated = pb.param("stated.table", DType::I8, &[64, 4], false);
    let w = pb.param("w", DType::I8, &[64, 4], false);
    let window = pb.hist_state("tokens", DType::I32, &[1], 4, false);
    let acc = pb.fixed_state("acc", DType::I32, &[1], 0, 63, false);
    let carry = vec![TensorType::fixed(DType::I32, &[4])];
    let pre = {
        let mut b = pb.block("pre", vec![]);
        // A hash of the last tokens: from the token and its history, never from a weight.
        let t = b.cast(Ref::Input(INPUT_TOKEN), DType::I32);
        let t = b.reshape_fixed(t, &[1]);
        let hist = b.hist_append(window, t);
        let s = b.reduce_sum(hist, 0, DType::I64);
        let id = b.clamp(s, 0, 256, DType::Idx);
        let id = b.reshape_fixed(id, &[]);
        let row = b.gather(table, id, 0, 0);
        // A row chosen by the weights: TopK over a projection.
        let x = b.cast(row, DType::I32);
        let xc = b.reshape_fixed(x, &[4, 1]);
        let logits = b.matmul(w, xc, DType::I64);
        let logits = b.reshape_fixed(logits, &[64]);
        let pick = b.topk(logits, 0, 1);
        let r = b.gather(routed, pick, 0, 0);
        let r = b.reshape_fixed(r, &[4]);
        // A row chosen by a state the weights wrote.
        let prev = b.clamp(Ref::State(acc), 0, 63, DType::Idx);
        let prev = b.reshape_fixed(prev, &[]);
        let st = b.gather(stated, prev, 0, 0);
        let first = b.slice(r, 0, 0, 1);
        let first = b.cast(first, DType::I32);
        let next = b.clamp(first, 0, 63, DType::I32);
        b.state_write(acc, next);
        let sum = b.cast(st, DType::I32);
        let y = b.add(sum, r, DType::I32);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", carry);
        let l = b.cast(Ref::CarryIn(0), DType::I32);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let p = pb.finish(pre, vec![], post, logits);
    let taint = weight_taint_v1(&p);
    assert!(!taint.states[window as usize], "the token history is not computed from the weights");
    assert!(taint.states[acc as usize], "a state written from a weight-derived value is");
    let t = TirTiersV1::of(&p, ALL_ROWS);
    assert_eq!(tier_of(&p, &t, "ngram.table"), TirTierV1::Gathered, "an n-gram id is a function of the tokens");
    assert_eq!(tier_of(&p, &t, "routed.table"), TirTierV1::Routed, "a TopK over a projection is a route");
    assert_eq!(tier_of(&p, &t, "stated.table"), TirTierV1::Routed, "so is a state the weights wrote");
    assert_eq!(tier_of(&p, &t, "w"), TirTierV1::Pinned(TirPinnedWhyV1::Dense));
    // And the program runs the reference's values with all three served by rows.
    let mut params = MapParams::default();
    let mut rng = Lcg(5);
    for (j, d) in p.params.iter().enumerate() {
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        let data = (0..n).map(|_| rng.range(-128, 127)).collect();
        params.tensors.insert((j as u16, None), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
    }
    let o = differential(&p, &params, &tokens(&p, 12, 3), &rows(false)).unwrap();
    assert_eq!((o.steps_ok, o.served, o.whole_reads), (12, 3, 0), "{o:?}");
    assert_eq!(o.rows_gathered, 3 * 12, "one row of each table a position");
}

#[test]
fn the_reasons_a_row_addressed_param_is_pinned() {
    let mut pb = ProgramBuilder::new(16, HISTORY_BOUND_V1_SMALL);
    let every = pb.param("every", DType::I8, &[2, 4], false);
    let uneven = pb.param("uneven", DType::I8, &[4, 4], false);
    let observed = pb.param("observed", DType::I8, &[16, 4], false);
    let pre = {
        let mut b = pb.block("pre", vec![]);
        let idx = b.iota(DType::Idx, &[misaka_palw_tir::Dim::Fixed(2)], 0, 0, 1);
        let a = b.gather(every, idx, 0, 0);
        let a = b.reshape_fixed(a, &[8]);
        let a = b.cast(a, DType::I32);
        let u1 = b.gather(uneven, Ref::Input(INPUT_TOKEN), 0, 0);
        let v = b.reshape_fixed(uneven, &[8, 2]);
        let u2 = b.gather(v, Ref::Input(INPUT_POS), 0, 0);
        let u1 = b.cast(u1, DType::I32);
        let u2 = b.cast(u2, DType::I32);
        let u2 = b.reshape_fixed(u2, &[2]);
        let both = b.concat(&[u1, u2], 0);
        let ov = b.reshape_fixed(observed, &[64]);
        let ov = b.commit(ov);
        let ov = b.cast(ov, DType::I32);
        let ov = b.slice(ov, 0, 0, 8);
        let s = b.add(a, ov, DType::I32);
        let both = b.slice(both, 0, 0, 6);
        let s = b.slice(s, 0, 0, 6);
        let y = b.add(s, both, DType::I32);
        b.finish(&[y])
    };
    let (post, logits) = {
        let mut b = pb.block("post", vec![TensorType::fixed(DType::I32, &[6])]);
        let l = b.cast(Ref::CarryIn(0), DType::I32);
        let l = b.commit(l);
        let Ref::Node(i) = l else { unreachable!() };
        (b.finish(&[]), i)
    };
    let p = pb.finish(pre, vec![], post, logits);
    let t = TirTiersV1::of(&p, ALL_ROWS);
    assert_eq!(tier_of(&p, &t, "every"), TirTierV1::Pinned(TirPinnedWhyV1::EveryRow), "both of its two rows every forward");
    assert_eq!(tier_of(&p, &t, "uneven"), TirTierV1::Pinned(TirPinnedWhyV1::UnevenRows), "rows of 4 and rows of 2");
    assert_eq!(tier_of(&p, &t, "observed"), TirTierV1::Pinned(TirPinnedWhyV1::Dense), "a committed view is read whole");
}

#[test]
fn the_row_path_computes_the_reference_on_random_programs() {
    // Six thousand programs: about one in twenty-five draws a gather the tiers serve by rows
    // (axis 0, no batch split, of a param), so this is a couple of hundred programs on the row path.
    let n = 6000u64;
    let mut total = Outcome::default();
    let mut with_rows = 0u64;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x7e5_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let steps = rng.gen_range(1..=8);
        let toks: Vec<u32> = (0..steps)
            .map(|_| {
                if rng.gen_bool(0.9) {
                    rng.gen_range(0..g.prog.token_bound.min(64))
                } else {
                    pick(&mut rng, &[g.prog.token_bound, u32::MAX])
                }
            })
            .collect();
        let every = seed % 5 == 0;
        let o = differential(&g.prog, &g.params, &toks, &rows(every)).unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        if !every {
            assert_eq!(o.whole_reads, 0, "seed {seed}: a served instance read whole off the every-node sink");
        }
        with_rows += (o.rows_gathered > 0) as u64;
        total += o;
    }
    eprintln!("random programs, rows: {with_rows} of {n} programs gathered rows; {total:?}");
    assert!(with_rows * 40 > n, "a fortieth of the programs at least run the row path");
    assert!(total.steps_err > 0, "and failing steps among them");
    assert_eq!(total.class_diffs, 0, "the same failure class as the reference");
}

#[test]
fn the_row_path_computes_the_reference_on_the_corpus_and_the_goldens() {
    let mut total = Outcome::default();
    let (moe_p, moe_g) = moe_program();
    let (many_p, many_g) = many_expert_moe(64, 2, 4);
    let (dense_p, dense_g) = dense(&[HISTORY_BOUND_V1_SMALL, 3]);
    for (name, p, gens) in [("moe", moe_p, moe_g), ("many experts", many_p, many_g), ("dense", dense_p, dense_g)] {
        for seed in [11u64, 12] {
            let params = materialize(&p, &gens, seed);
            let toks = tokens(&p, 9, seed);
            let o = differential(&p, &params, &toks, &rows(seed == 12)).unwrap_or_else(|e| panic!("{name} seed {seed}: {e}"));
            assert_eq!(o.steps_ok, toks.len(), "{name}: the calibrated weights run every position");
            assert!(o.served > 0 && o.rows_gathered > 0, "{name}: {o:?}");
            if seed == 11 {
                assert_eq!(o.whole_reads, 0, "{name}");
            }
            total += o;
        }
    }
    for (name, p, params) in goldens() {
        let toks = tokens(&p, 5, 9);
        let o = differential(&p, &params, &toks, &rows(false)).unwrap_or_else(|e| panic!("golden {name}: {e}"));
        assert_eq!(o.whole_reads, 0, "golden {name}");
        total += o;
    }
    eprintln!("corpus and goldens, rows: {total:?}");
    assert_eq!(total.class_diffs, 0);
}

#[test]
fn a_params_range_is_computed_once_and_a_rebind_recomputes_it() {
    let (p, gens) = moe_program();
    let plan = TirPlan::compile(&p).unwrap();
    let params = materialize(&p, &gens, 1);
    let mut tp = TirParams::from_map(&plan, &params).unwrap();
    let j = p.param_index("output.w").unwrap();
    let first = tp.range(j, None).unwrap();
    assert_eq!(tp.range(j, None), Some(first), "the memo answers again");
    let d = &p.params[j as usize];
    let n: usize = d.shape.iter().map(|x| *x as usize).product();
    let zeros = misaka_palw_tir_exec::ParamData::from_buf(misaka_palw_tir_exec::Buf::zeros(d.dtype, n)).unwrap();
    tp.insert(&plan, j, None, zeros).unwrap();
    assert_eq!(tp.range(j, None), Some(misaka_palw_tir::interval::Interval::new(0, 0)), "a rebind is a new instance");
}
