//! **Random programs on the reference evaluator and the typed backend** — the independent second
//! implementation's generator (`tests/common/progen.rs`, ported from `tir/ref2`), which draws every
//! primitive, broadcast and view shapes, histories of windows 1, 2, 3, 5 and `history_bound`,
//! attention with a `MatMul` contracting `H`, per-layer and global states and params, and operand
//! values from range-extreme profiles (so many steps overflow, divide by zero or index out of
//! range — and must fail in both).
//!
//! Every step: success versus failure, logits, every commit point, the run state; on a third of the
//! programs every uncommitted node against the reference's cone evaluation; and runs started from a
//! random mid-run state — up to the last position below `history_bound` — so full and compacted
//! history buffers and the position bound are exercised.
//!
//! `TIR_EXEC_CASES=k` multiplies the number of programs (default 1: 2,000 programs + 600 mid-run).

mod common;

use std::collections::BTreeMap;

use common::progen::{GenCfg, R, gen_program, pick, rand_profile, rand_value};
use common::{Opts, Outcome, differential};
use misaka_palw_tir::program::{StateKind, TirProgramV1};
use misaka_palw_tir::{Prim, Ref, RunState, Tensor};
use rand::{Rng, SeedableRng};

fn scale() -> u64 {
    std::env::var("TIR_EXEC_CASES").ok().and_then(|s| s.parse().ok()).unwrap_or(1)
}

fn tokens(rng: &mut R, p: &TirProgramV1, n: usize) -> Vec<u32> {
    (0..n)
        .map(|_| {
            if rng.gen_bool(0.9) || p.token_bound == u32::MAX {
                rng.gen_range(0..p.token_bound.min(64))
            } else {
                pick(rng, &[p.token_bound, u32::MAX])
            }
        })
        .collect()
}

/// Every `(state, layer)` instance some occurrence touches.
fn instances(p: &TirProgramV1) -> Vec<(u16, Option<u16>)> {
    let mut out = std::collections::BTreeSet::new();
    for (block, layer) in p.occurrences() {
        for n in &p.blocks[block as usize].nodes {
            let mut touch = |j: u16| {
                let l = if p.states[j as usize].per_layer { layer } else { None };
                out.insert((j, l));
            };
            for r in &n.inputs {
                if let Ref::State(j) = r {
                    touch(*j);
                }
            }
            if let Prim::StateWrite { state } | Prim::HistAppend { state } = n.prim {
                touch(state);
            }
        }
    }
    out.into_iter().collect()
}

/// A valid run state at a random position: Fixed values inside `[lo, hi]` (extremes planted),
/// histories of exactly `min(pos, window − 1)` rows of arbitrary dtype values.
fn random_state(rng: &mut R, p: &TirProgramV1) -> RunState {
    let max_window = p.states.iter().filter_map(|s| if let StateKind::Hist { window } = s.kind { Some(window) } else { None }).max();
    let pos = match max_window {
        Some(w) if w > 64 => rng.gen_range(0..40),
        _ => match rng.gen_range(0..4) {
            0 => p.history_bound - 1,
            1 => p.history_bound - 2,
            _ => rng.gen_range(0..2000),
        },
    };
    let mut st = RunState { pos, fixed: BTreeMap::new(), hist: BTreeMap::new() };
    for (j, l) in instances(p) {
        let s = &p.states[j as usize];
        let shape: Vec<usize> = s.shape.iter().map(|d| *d as usize).collect();
        let n: usize = shape.iter().product();
        match s.kind {
            StateKind::Fixed { lo, hi } => {
                let prof = rand_profile(rng);
                let data = (0..n).map(|_| rand_value(rng, s.dtype, prof).clamp(lo as i128, hi as i128)).collect();
                st.fixed.insert((j, l), Tensor::new(s.dtype, shape, data).unwrap());
            }
            StateKind::Hist { window } => {
                let rows = (pos as usize).min(window as usize - 1);
                let prof = rand_profile(rng);
                let rows = (0..rows)
                    .map(|_| Tensor::new(s.dtype, shape.clone(), (0..n).map(|_| rand_value(rng, s.dtype, prof)).collect()).unwrap())
                    .collect();
                st.hist.insert((j, l), rows);
            }
        }
    }
    st
}

fn report(name: &str, programs: u64, refused: u64, t: &Outcome) {
    eprintln!(
        "{name}: {programs} programs ({refused} refused by both), {} steps ok, {} steps failed in both, {} commit points, {} node values; {} steps failed with different classes",
        t.steps_ok, t.steps_err, t.commits, t.nodes, t.class_diffs
    );
    for e in &t.class_examples {
        eprintln!("    {e}");
    }
}

#[test]
fn random_programs_agree_step_for_step() {
    let n = 2000 * scale();
    let (mut total, mut refused) = (Outcome::default(), 0u64);
    for seed in 0..n {
        let mut rng = R::seed_from_u64(seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let steps = rng.gen_range(1..=8);
        let toks = tokens(&mut rng, &g.prog, steps);
        let o = differential(&g.prog, &g.params, &toks, &Opts { every_node: seed % 3 == 0, ..Default::default() })
            .unwrap_or_else(|e| panic!("seed {seed}: {e}"));
        refused += o.refused as u64;
        total += o;
    }
    report("random programs", n, refused, &total);
    assert!(refused * 20 < n, "the generator's programs are in normal form");
    assert!(total.steps_ok > n as usize, "many steps succeed");
    assert!(total.steps_err > n as usize / 4, "many steps fail (range-extreme values)");
    assert_eq!(total.class_diffs, 0, "the same class as the reference for every failing step");
}

#[test]
fn random_programs_agree_from_random_mid_run_states() {
    let n = 600 * scale();
    let (mut total, mut refused) = (Outcome::default(), 0u64);
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x7e57_0000 + seed);
        let g = gen_program(&mut rng, GenCfg::default());
        let start = random_state(&mut rng, &g.prog);
        let steps = rng.gen_range(1..=6);
        let toks = tokens(&mut rng, &g.prog, steps);
        let o = differential(&g.prog, &g.params, &toks, &Opts { every_node: seed % 4 == 0, start: Some(start) })
            .unwrap_or_else(|e| panic!("mid-run seed {seed}: {e}"));
        refused += o.refused as u64;
        total += o;
    }
    report("random programs from mid-run states", n, refused, &total);
    assert!(total.steps_ok > 0);
    assert_eq!(total.class_diffs, 0, "the same class as the reference for every failing step");
}

/// NF-19 (spec 04b revision 2): programs whose post block writes a state are refused — by the
/// backend's plan exactly as by the reference.
#[test]
fn programs_with_post_writes_are_refused_by_both() {
    let n = 300 * scale();
    let mut refused = 0u64;
    for seed in 0..n {
        let mut rng = R::seed_from_u64(0x9057_0000 + seed);
        let g = gen_program(&mut rng, GenCfg { post_writes: true, ..GenCfg::default() });
        let toks = tokens(&mut rng, &g.prog, 2);
        let o = differential(&g.prog, &g.params, &toks, &Opts::default()).unwrap_or_else(|e| panic!("post-writes seed {seed}: {e}"));
        refused += o.refused as u64;
    }
    eprintln!("post writes: {refused} of {n} programs refused by both");
    assert!(refused * 4 > n);
}
