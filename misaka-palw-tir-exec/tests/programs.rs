//! **The five whole programs of `misaka-palw-tir/tests/programs.rs`** — a 2-layer dense GQA
//! decoder, a sliding-window + global schedule, a GDN layer with `k_heads ≠ v_heads` (grouped and
//! tiled), a Mamba2 layer and a top-2 MoE with a shared expert — on the reference evaluator and the
//! typed backend side by side: logits, every commit point, the run state after every step and the
//! value of EVERY node (checked against the reference's cone evaluation from the step's commits).
//!
//! The builders are the reference crate's own (`#[path]`), so the programs are the ones its tests
//! and golden vectors pin. Beyond their calibrated weights: params from each dtype's full range
//! with the extremes planted (the totality suite's hostile weights), long runs that slide windows
//! and compact histories, and byte-mutated programs that still validate — where every success
//! and every failure must agree.

#[path = "../../misaka-palw-tir/tests/common/mod.rs"]
mod tircommon;

mod common;

use common::{Opts, Outcome, differential};
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{MapParams, Tensor, TirProgramV1};
use tircommon::Lcg;
use tircommon::models::*;

fn corpus() -> Vec<(&'static str, TirProgramV1, std::collections::BTreeMap<u16, Gen>)> {
    let (dense_p, dense_g) = dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]);
    let (sliding_p, sliding_g) = dense(&[3, HISTORY_BOUND_V1_SMALL, 3]);
    let (gdn_p, gdn_g) = gdn_program(true);
    let (gdnt_p, gdnt_g) = gdn_program(false);
    let (mamba_p, mamba_g) = mamba2_program();
    let (moe_p, moe_g) = moe_program();
    vec![
        ("dense", dense_p, dense_g),
        ("sliding", sliding_p, sliding_g),
        ("gdn-grouped", gdn_p, gdn_g),
        ("gdn-tiled", gdnt_p, gdnt_g),
        ("mamba2", mamba_p, mamba_g),
        ("moe", moe_p, moe_g),
    ]
}

/// Params from each declaration's full dtype range, a third of the lanes at the extremes (the
/// reference totality suite's generator).
fn hostile_params(p: &TirProgramV1, seed: u64) -> MapParams {
    let mut rng = Lcg(seed);
    let mut out = MapParams::default();
    for (j, d) in p.params.iter().enumerate() {
        let layers: Vec<Option<u16>> = if d.per_layer { (0..p.schedule.layers.len() as u16).map(Some).collect() } else { vec![None] };
        let n: usize = d.shape.iter().map(|x| *x as usize).product();
        for l in layers {
            let data = (0..n)
                .map(|i| match i % 3 {
                    0 => [d.dtype.min_value(), d.dtype.max_value(), 0, 1, (-1i128).max(d.dtype.min_value())][(i / 3) % 5],
                    _ => rng.range(d.dtype.min_value().max(-(1 << 100)), d.dtype.max_value().min(1 << 100)),
                })
                .collect();
            out.tensors.insert((j as u16, l), Tensor::new(d.dtype, d.shape.iter().map(|x| *x as usize).collect(), data).unwrap());
        }
    }
    out
}

fn tokens(p: &TirProgramV1, n: usize, seed: u64) -> Vec<u32> {
    let mut rng = Lcg(seed ^ 0x5eed);
    (0..n).map(|_| (rng.next_u64() % p.token_bound as u64) as u32).collect()
}

#[test]
fn the_five_programs_agree_at_every_node() {
    let mut total = Outcome::default();
    for (name, p, gens) in corpus() {
        for seed in [101u64, 202, 303] {
            let params = materialize(&p, &gens, seed);
            let toks = tokens(&p, 7, seed);
            let o = differential(&p, &params, &toks, &Opts { every_node: true, ..Default::default() })
                .unwrap_or_else(|e| panic!("{name} seed {seed}: {e}"));
            assert_eq!(o.steps_ok, toks.len(), "{name}: the calibrated weights run every position");
            total += o;
        }
    }
    eprintln!("corpus, calibrated: {total:?}");
    assert!(total.nodes > 10_000);
}

#[test]
fn the_five_programs_agree_under_hostile_weights() {
    let mut total = Outcome::default();
    for (name, p, _) in corpus() {
        for seed in 0..6u64 {
            let params = hostile_params(&p, seed);
            let toks = tokens(&p, 6, seed);
            let o = differential(&p, &params, &toks, &Opts { every_node: seed < 2, ..Default::default() })
                .unwrap_or_else(|e| panic!("{name} hostile seed {seed}: {e}"));
            total += o;
        }
    }
    eprintln!("corpus, hostile weights: {total:?}");
    assert!(total.steps_ok > 0);
}

/// Long enough for the sliding windows to slide many times over (window 3: the history buffer
/// compacts every few positions) and for the recurrences to wander.
#[test]
fn long_runs_agree_step_for_step() {
    for (name, p, gens) in corpus() {
        let params = materialize(&p, &gens, 7);
        let toks = tokens(&p, 48, 7);
        let o = differential(&p, &params, &toks, &Opts::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(o.steps_ok, 48, "{name}");
    }
}

/// Byte mutations of every corpus program; the ones that still decode and validate run on both
/// evaluators under hostile weights, and every success and failure must agree.
#[test]
fn mutated_programs_agree_on_success_and_value() {
    let mut rng = Lcg(0xb17e);
    let (mut ran, mut total) = (0usize, Outcome::default());
    for (name, p, _) in corpus() {
        let good = p.encode();
        for m in 0..400u64 {
            let mut b = good.clone();
            match m % 3 {
                0 => {
                    let i = (rng.next_u64() as usize) % b.len();
                    b[i] = rng.next_u64() as u8;
                }
                1 => {
                    for _ in 0..3 {
                        let i = (rng.next_u64() as usize) % b.len();
                        b[i] ^= 1 << (rng.next_u64() % 8);
                    }
                }
                _ => {
                    let i = (rng.next_u64() as usize) % b.len();
                    b[i] = b[i].wrapping_add(1);
                }
            }
            let Ok(q) = TirProgramV1::decode_canonical(&b) else { continue };
            // The reference holds every element as an i128: skip mutants that declare huge tensors.
            let big = q
                .blocks
                .iter()
                .flat_map(|b| b.nodes.iter())
                .map(|n| n.out.elements_at(8))
                .chain(q.params.iter().map(|d| d.shape.iter().map(|x| *x as u64).product::<u64>()))
                .max()
                .unwrap_or(0);
            if big > 1 << 16 || q.schedule.layers.len() > 8 {
                continue;
            }
            let params = hostile_params(&q, m);
            let toks = tokens(&q, 4, m);
            let o = differential(&q, &params, &toks, &Opts { every_node: m % 4 == 0, ..Default::default() })
                .unwrap_or_else(|e| panic!("{name} mutation {m}: {e}"));
            total += o;
            ran += 1;
        }
    }
    eprintln!("mutants run: {ran}, {total:?}");
    assert!(ran > 20, "some mutants must validate and run");
}
