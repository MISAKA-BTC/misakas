//! **Totality: malformed bytes, mutated programs and hostile operands are errors, never panics.**
//!
//! A panic reachable from block validation is a remote chain-halt (the 2026-08-17 audit's B7), and
//! the court will run this evaluator on bytes and values a counterparty chose. So: random byte
//! strings, and thousands of byte mutations of every test program, go through `decode_canonical`;
//! every mutant that still decodes and validates is RUN, with params drawn from each declared
//! dtype's full range (weights are untrusted), including the type extremes. Nothing may panic.

mod common;

use std::collections::BTreeMap;
use std::panic::{AssertUnwindSafe, catch_unwind};

use common::Lcg;
use common::models::*;
use misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL;
use misaka_palw_tir::{ConeEnv, Interpreter, MapParams, Tensor, TirProgramV1};

/// Params from each declaration's full dtype range, a third of the lanes at the extremes.
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

fn small_enough(p: &TirProgramV1) -> bool {
    // The evaluator holds every element as an i128; admission's cost ceiling bounds real programs.
    // Mutants that declare huge tensors are skipped here rather than allocated.
    let max_elems = p
        .blocks
        .iter()
        .flat_map(|b| b.nodes.iter())
        .map(|n| n.out.elements_at(8))
        .chain(p.params.iter().map(|d| d.shape.iter().map(|x| *x as u64).product::<u64>()))
        .max()
        .unwrap_or(0);
    max_elems <= 1 << 16 && p.schedule.layers.len() <= 8
}

fn exercise(bytes: &[u8], seed: u64) -> Result<(), String> {
    let r = catch_unwind(AssertUnwindSafe(|| {
        let Ok(p) = TirProgramV1::decode_canonical(bytes) else { return };
        if !small_enough(&p) {
            return;
        }
        let Ok(interp) = Interpreter::new(&p) else { panic!("decode_canonical accepted what validate refuses") };
        let params = hostile_params(&p, seed);
        let _ = interp.run(&params, &[0, (seed % p.token_bound as u64) as u32, u32::MAX]);
        // A cone with nothing supplied, and a cone at a far position with an empty history.
        for (block, layer) in p.occurrences() {
            let nodes = p.blocks[block as usize].nodes.len() as u16;
            for target in [0, nodes / 2, nodes - 1] {
                let env = ConeEnv { token: Some(1), pos: (seed as u32) % HISTORY_BOUND_V1_SMALL, ..Default::default() };
                let _ = interp.eval_cone(block, layer, target, &params, &env);
            }
        }
        let _ = interp.eval_cone(250, None, 0, &params, &ConeEnv::default());
    }));
    r.map_err(|e| {
        format!("panicked: {:?}", e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())))
    })
}

#[test]
fn random_bytes_never_panic() {
    let mut rng = Lcg(0xdead);
    for len in 0..400usize {
        let bytes: Vec<u8> = (0..len).map(|_| rng.next_u64() as u8).collect();
        exercise(&bytes, len as u64).unwrap_or_else(|e| panic!("len {len}: {e}"));
    }
}

#[test]
fn mutated_programs_and_hostile_operands_never_panic() {
    let programs: Vec<TirProgramV1> = vec![
        dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0,
        dense(&[3, HISTORY_BOUND_V1_SMALL]).0,
        gdn_program(true).0,
        mamba2_program().0,
        moe_program().0,
    ];
    let mut rng = Lcg(0xbeef);
    let mut survived: BTreeMap<usize, usize> = BTreeMap::new();
    for (pi, p) in programs.iter().enumerate() {
        let good = p.encode();
        // The unmutated program under hostile params first.
        exercise(&good, 7).unwrap_or_else(|e| panic!("program {pi} unmutated: {e}"));
        for m in 0..600u64 {
            let mut b = good.clone();
            match m % 4 {
                0 => {
                    let i = (rng.next_u64() as usize) % b.len();
                    b[i] = rng.next_u64() as u8;
                }
                1 => {
                    for _ in 0..4 {
                        let i = (rng.next_u64() as usize) % b.len();
                        b[i] ^= 1 << (rng.next_u64() % 8);
                    }
                }
                2 => {
                    let i = (rng.next_u64() as usize) % b.len();
                    b.truncate(i);
                }
                _ => {
                    let i = (rng.next_u64() as usize) % b.len();
                    b.insert(i, rng.next_u64() as u8);
                }
            }
            if TirProgramV1::decode_canonical(&b).is_ok() {
                *survived.entry(pi).or_default() += 1;
            }
            exercise(&b, m).unwrap_or_else(|e| panic!("program {pi} mutation {m}: {e}"));
        }
    }
    // Some single-byte mutations (a weight shape's value, a Clamp bound, an attribute) keep the
    // program valid; they must have been run, not just decoded.
    assert!(survived.values().sum::<usize>() > 0, "no mutant validated, so none was run");
}

/// **The test programs are range-safe in practice**: under params drawn from each dtype's FULL range
/// with the extremes planted (weights are untrusted, spec 04b §7), every program runs every position
/// to completion — no exact primitive overflows. This is what a template must satisfy before
/// `tir_admit_v1` can prove it (Gate 2); two templates were fixed to meet it (the RoPE angle sum and
/// the write-shift negation widen to i128/i64, the wide norm bounds its eps mantissa).
#[test]
fn every_test_program_runs_to_completion_under_hostile_weights() {
    for (name, p) in [
        ("dense", dense(&[HISTORY_BOUND_V1_SMALL, HISTORY_BOUND_V1_SMALL]).0),
        ("sliding", dense(&[3, HISTORY_BOUND_V1_SMALL, 3]).0),
        ("gdn", gdn_program(true).0),
        ("gdn-tiled", gdn_program(false).0),
        ("mamba2", mamba2_program().0),
        ("moe", moe_program().0),
    ] {
        let interp = Interpreter::new(&p).unwrap();
        for seed in 0..6 {
            let params = hostile_params(&p, seed);
            let tokens: Vec<u32> = (0..5).map(|i| ((seed + i) % p.token_bound as u64) as u32).collect();
            if let Err(e) = interp.run(&params, &tokens) {
                panic!("{name} seed {seed}: {e}");
            }
        }
    }
}
