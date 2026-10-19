//! **Measurement (not a conformance test): what one court submission costs the node to verify.**
//!
//! `#[ignore]`d. Run one operation per process so that peak RSS is attributable:
//!
//! ```text
//! MEASURE_OP=close_cone_acquit MEASURE_N=1000 cargo test --release -p kaspa-consensus-core \
//!   --test palw_tir_court_verify_cpu -- --ignored --nocapture
//! ```
//!
//! Only node-side verification functions are timed. Every case (an execution, a refutation, a
//! root claim, a dissection phase and its round) is built by the prover side BEFORE the clock starts.
//! `MEASURE_N=0` builds the fixtures and nothing else, so its peak RSS is the setup baseline.
//! Output: one `MEASURE ...` line on stderr per run.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, build_tir_dissect_bottom_v1, build_tir_dissect_round_v1, build_tir_root_claim_v1,
    check_tir_cone_refutation_v1, check_tir_dissect_bottom_v1, check_tir_root_claim_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1, PalwTirRootClaimV1, palw_tir_dissect_site_v1,
};

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);
const W: u64 = 10;
/// Distinct cases built per operation; N larger than this cycles over them (reported).
const DISTINCT_CAP: usize = 1000;

fn dissected(f: &Fixture) -> Vec<(u64, PalwTirDissectSiteV1)> {
    let intervals = f.intervals.as_ref().expect("admissible");
    f.leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .collect()
}

/// Times `check` over `n` items drawn from `inputs` (cycling), returns the stats line parts.
struct Stats {
    median: u128,
    p99: u128,
    max: u128,
    total: u128,
    ok: usize,
}

fn run<T>(inputs: &mut [T], n: usize, mut check: impl FnMut(&mut T) -> bool) -> Stats {
    let mut times = Vec::with_capacity(n);
    let mut ok = 0usize;
    let len = inputs.len();
    for i in 0..n {
        let item = &mut inputs[i % len];
        let t = Instant::now();
        let good = check(item);
        times.push(t.elapsed().as_nanos());
        ok += good as usize;
    }
    times.sort_unstable();
    let pick = |q: f64| if times.is_empty() { 0 } else { times[((times.len() as f64 - 1.0) * q).round() as usize] };
    Stats { median: pick(0.5), p99: pick(0.99), max: times.last().copied().unwrap_or(0), total: times.iter().sum(), ok }
}

fn report(op: &str, n: usize, distinct: usize, bytes: u64, s: &Stats) {
    eprintln!(
        "MEASURE op={op} n={n} distinct={distinct} bytes={bytes} median_ns={} p99_ns={} max_ns={} total_ns={} expected_ok={}",
        s.median, s.p99, s.max, s.total, s.ok
    );
}

fn forged_cone_cases(fixtures: &[Fixture]) -> Vec<(PalwTirConeRefutationV1, u64)> {
    let mut out = Vec::new();
    'outer: for f in fixtures {
        for i in 0..f.leaves.len() {
            let leaf = &f.leaves[i];
            let lane = leaf.value_count as usize / 2;
            let v = f.values[i][lane];
            let iv = f.interval(leaf);
            let Some(forged) = [v + 1, v - 1].into_iter().find(|w| iv.contains(*w)) else { continue };
            let mut values = f.values.clone();
            values[i][lane] = forged;
            let x = f.commit(&values, &f.rows, &f.generated);
            out.push((refute(f, &x, i as u64), lane as u64));
            if out.len() >= DISTINCT_CAP {
                break 'outer;
            }
        }
    }
    out
}

fn honest_cone_cases(fixtures: &[Fixture]) -> Vec<PalwTirConeRefutationV1> {
    let mut out = Vec::new();
    'outer: for f in fixtures {
        let x = f.honest();
        for i in 0..f.leaves.len() {
            out.push(refute(f, &x, i as u64));
            if out.len() >= DISTINCT_CAP {
                break 'outer;
            }
        }
    }
    out
}

#[test]
#[ignore]
fn court_verify_cpu_measure() {
    let op = std::env::var("MEASURE_OP").expect("MEASURE_OP");
    let n: usize = std::env::var("MEASURE_N").expect("MEASURE_N").parse().expect("usize");
    let fixtures = fixtures();

    match op.as_str() {
        // (c) close verification, honest leaf: the whole cone recomputation, ends NoFaultFound.
        "close_cone_acquit" => {
            let mut cases = honest_cone_cases(&fixtures);
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats { median: 0, p99: 0, max: 0, total: 0, ok: 0 });
            }
            let bytes = borsh::to_vec(&cases[0]).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |r| check_tir_cone_refutation_v1(r, &RULES) == Err(PalwStepRefuteError::NoFaultFound));
            report(&op, n, distinct, bytes, &s);
        }
        // (d) close verification, forged leaf: openings and carriage pass, the last recomputation
        //     disagrees with the committed lane. Convicts as ComputationMismatch.
        "close_cone_forged_last_stage" => {
            let mut cases = forged_cone_cases(&fixtures);
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats { median: 0, p99: 0, max: 0, total: 0, ok: 0 });
            }
            let bytes = borsh::to_vec(&cases[0].0).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |(r, lane)| match check_tir_cone_refutation_v1(r, &RULES) {
                Ok(v) => v.fault == PalwStepFaultV1::ComputationMismatch { value_index: *lane as u32 },
                Err(_) => false,
            });
            report(&op, n, distinct, bytes, &s);
        }
        // (c') the root claim on a dissected leaf: opening and admission of the responder's claim.
        "root_claim_admit" => {
            let mut cases: Vec<(PalwTirRootClaimV1, u64, PalwTirDissectSiteV1)> = Vec::new();
            'o: for f in &fixtures {
                let x = f.honest();
                let store = Store { f, x: &x };
                for (leaf, site) in dissected(f) {
                    let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("root");
                    cases.push((root, leaf, site));
                    if cases.len() >= DISTINCT_CAP {
                        break 'o;
                    }
                }
            }
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats { median: 0, p99: 0, max: 0, total: 0, ok: 0 });
            }
            let bytes = borsh::to_vec(&cases[0].0).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |(root, leaf, site)| check_tir_root_claim_v1(root, *leaf, &RULES).as_ref() == Ok(site));
            report(&op, n, distinct, bytes, &s);
        }
        // (b) one dissection move: the node checks one round (the children) against the phase.
        //     Phases are cloned in setup; only `apply_round` is timed.
        "dissect_round_apply" => {
            let mut cases: Vec<(PalwTirDissectPhaseV1, PalwTirDissectRoundV1)> = Vec::new();
            'o: for f in &fixtures {
                let x = f.honest();
                let store = Store { f, x: &x };
                for (leaf, site) in dissected(f) {
                    let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("root");
                    let phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
                    if phase.turn() != PalwBisectTurnV1::AwaitDisclosure {
                        continue; // a single-tile range opens already terminal: no round to apply
                    }
                    let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("round");
                    cases.push((phase, round));
                    if cases.len() >= DISTINCT_CAP {
                        break 'o;
                    }
                }
            }
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats { median: 0, p99: 0, max: 0, total: 0, ok: 0 });
            }
            let bytes = borsh::to_vec(&cases[0].1).unwrap().len() as u64;
            let distinct = cases.len();
            let mut phases: Vec<(PalwTirDissectPhaseV1, PalwTirDissectRoundV1)> = (0..n).map(|i| cases[i % distinct].clone()).collect();
            let s = run(&mut phases, n, |(phase, round)| phase.apply_round(round, 1, W).is_ok());
            report(&op, n, distinct, bytes, &s);
        }
        // (c'') the dissection bottom: the last reductions recomputed over the terminal tile.
        "dissect_bottom_honest" => {
            let mut cases: Vec<(PalwTirDissectPhaseV1, PalwTirConeRefutationV1, u64)> = Vec::new();
            'o: for f in &fixtures {
                let x = f.honest();
                let store = Store { f, x: &x };
                for (leaf, site) in dissected(f) {
                    let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("root");
                    let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
                    let mut t = 1;
                    while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
                        let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("round");
                        phase.apply_round(&round, t, W).expect("honest round");
                        phase.apply_choice(&choice(phase.round(), 0), t + 1, W).expect("choice");
                        t += 2;
                    }
                    let bottom = build_tir_dissect_bottom_v1(&x.binding, &phase, &store, &RULES).expect("bottom");
                    cases.push((phase, bottom, leaf));
                    if cases.len() >= DISTINCT_CAP {
                        break 'o;
                    }
                }
            }
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats { median: 0, p99: 0, max: 0, total: 0, ok: 0 });
            }
            let bytes = borsh::to_vec(&cases[0].1).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |(phase, bottom, leaf)| {
                check_tir_dissect_bottom_v1(phase, bottom, *leaf, &RULES) == Err(PalwStepRefuteError::NoFaultFound)
            });
            report(&op, n, distinct, bytes, &s);
        }
        other => panic!("unknown MEASURE_OP {other}"),
    }
}

fn choice(round: u32, child: u8) -> kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectChoiceV1 {
    kaspa_consensus_core::palw_tir_dissect_v1::PalwTirDissectChoiceV1 {
        version: kaspa_consensus_core::palw_tir_dissect_v1::PALW_TIR_DISSECT_OBJECT_VERSION_V1,
        session_id: SESSION,
        round,
        child,
    }
}
