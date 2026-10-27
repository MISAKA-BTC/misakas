//! Fixture verifier timing harness, ported from source commit
//! 2a6ea1a4ff934635978144a84f8ec96351d2bdb8 (g14/door-design).
//! Ignored by default; no production rules, prices, caps or fences are changed.
//!
//! Run ONE operation per fresh release process, e.g.:
//! MEASURE_OP=root_claim_admit MEASURE_N=10 cargo test --release
//! -p kaspa-consensus-core --test palw_tir_court_verify_cpu -- --ignored --nocapture
//!
//! Instant measures verifier WALL elapsed nanoseconds, NOT CPU time. Builds,
//! executions, evidence construction and mutable phase clones precede timing.
//! These are small fixture classes, never real-model/node saturation results.
//! MEASURE_N=0 constructs this operation's fixture/case setup, then runs no
//! verification; collect it separately as a setup baseline. Process user/sys CPU
//! and peak RSS require an external process measurement, which includes setup
//! (and test harness overhead); they are NOT per-verifier CPU/RSS. A difference
//! between independent N>0 and N=0 processes is descriptive, not an exact cost.
//! Example on macOS: build once, then invoke the exact test binary under
//! /usr/bin/time -l with the environment selector, separately for N=0 and N>0.
//! No benchmark runs or timing claims are implied by adding this harness.
//!
//! Lane values/p1-measure (2026-10-22): the first timed item is reported apart as `warmup_wall_ns`; median / p95 / p99 / max are
//! over the remaining items. New operations (test side only, no product change): the RFC-0024 position
//! close (honest / a lie moved into the middle), the two-level lookahead expansion, the D2 readiness demand open / answer
//! (disclosed, convicted) / deadline sweep, and one ML-DSA-87 verification (what each signed court object costs the validator).
//! `MEASURE_SCALE` sets the operation's size knob where its growth order matters (the position tile width, the D2 draw count, the
//! swept open cases). `fixture_setup_only` is the RSS / time baseline of the fixture construction.

#[path = "palw_tir_fixture_common.rs"]
mod fixture;
use fixture::*;

use std::time::Instant;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_bisect::PalwBisectTurnV1;
use kaspa_consensus_core::palw_court_model_scope_routes_v1::readiness_disclosure as dc;
use kaspa_consensus_core::palw_court_model_scope_routes_v1::readiness_root as rr;
use kaspa_consensus_core::palw_court_v2::PalwCourtVerdictProofV2;
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwCourtVerdictV2};
use kaspa_consensus_core::palw_step_leg::PalwStepFaultV1;
use kaspa_consensus_core::palw_step_refute::PalwStepRefuteError;
use kaspa_consensus_core::palw_tir_court_v1::{
    PalwTirConeRefutationV1, build_tir_dissect_bottom_v1, build_tir_dissect_position_bottom_v1, build_tir_dissect_position_round_v1,
    build_tir_dissect_round_v1, build_tir_root_claim_v1, check_tir_cone_refutation_v1, check_tir_dissect_bottom_v1,
    check_tir_dissect_position_bottom_v1, check_tir_root_claim_v1,
};
use kaspa_consensus_core::palw_tir_dissect_v1::{
    PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, PalwTirDissectPhaseV1, PalwTirDissectRoundV1, PalwTirDissectSiteV1, PalwTirFoldV1,
    PalwTirLookaheadDisclosureV2, PalwTirRootClaimV1, palw_tir_dissect_site_v1,
};
use kaspa_consensus_core::palw_tir_position_terminal_v2::{
    PALW_TIR_POSITION_OBJECT_VERSION_V2, PalwTirPositionChoiceV2, PalwTirPositionCloseV2, PalwTirPositionDissectionV2,
    check_tir_position_dissection_v2, palw_tir_position_dissection_digest_v2, palw_tir_position_dissection_for_v2,
    palw_tir_position_named_v2,
};
use std::num::NonZeroU32;

const SESSION: Hash64 = Hash64::from_bytes([0x5E; 64]);
const W: u64 = 10;
/// Distinct cases built per operation; N larger than this cycles over them (reported).
const DISTINCT_CAP: usize = 1000;
/// Position-close cases are costly to build (every terminal phase is played out); a smaller distinct set keeps setup bounded.
const POSITION_DISTINCT_CAP: usize = 128;
/// Test-harness allocation/work bound (not a consensus limit). Round application
/// pre-clones N mutable phases outside the timer; this also bounds its setup.
const MAX_MEASURE_ITERATIONS: usize = 10_000;
const OPS: [&str; 14] = [
    "close_cone_acquit",
    "close_cone_forged_last_stage",
    "root_claim_admit",
    "dissect_round_apply",
    "dissect_bottom_honest",
    "position_close_honest",
    "position_close_forged",
    "lookahead_expand",
    "d2_demand_open",
    "d2_answer_disclosed",
    "d2_answer_convicted",
    "d2_sweep",
    "mldsa87_verify",
    "fixture_setup_only",
];
/// ML-DSA-87 signature length (FIPS 204), what every signed court object carries.
const SIG: usize = 4627;
/// Test-side lookahead frontier ceiling (the production bound is typed-missing; this only lets the expansion run).
const LOOKAHEAD_TEST_CAP: Option<usize> = Some(1 << 20);
const D2_FEES: Option<dc::PalwD2DisclosureFeeScheduleV1> = Some(dc::PalwD2DisclosureFeeScheduleV1 { demand_fee_sompi: 7 });
const D2_AT: u64 = 1_000;

fn dissected(f: &Fixture) -> Vec<(u64, PalwTirDissectSiteV1)> {
    let intervals = f.intervals.as_ref().expect("admissible");
    f.leaves
        .iter()
        .enumerate()
        .filter_map(|(i, leaf)| palw_tir_dissect_site_v1(&f.space, intervals, leaf).map(|s| (i as u64, s)))
        .collect()
}

/// Times `check` over `n` items drawn from `inputs` (cycling), returns the stats line parts.
#[derive(Default)]
struct Stats {
    warmup: u128,
    median: u128,
    p95: u128,
    p99: u128,
    max: u128,
    total: u128,
    ok: usize,
}

/// The first item is the warm-up (reported apart); the order statistics are over items 2..n.
fn run<T>(inputs: &mut [T], n: usize, mut check: impl FnMut(&mut T) -> bool) -> Stats {
    let mut times = Vec::with_capacity(n);
    let mut ok = 0usize;
    let len = inputs.len();
    assert!(len > 0, "selected fixture operation has no native cases");
    for i in 0..n {
        let item = &mut inputs[i % len];
        let t = Instant::now();
        let good = check(item);
        times.push(t.elapsed().as_nanos());
        ok += good as usize;
    }
    let total = times.iter().sum();
    let warmup = times.first().copied().unwrap_or(0);
    let mut rest: Vec<u128> = if times.len() > 1 { times[1..].to_vec() } else { times.clone() };
    rest.sort_unstable();
    let pick = |q: f64| if rest.is_empty() { 0 } else { rest[((rest.len() as f64 - 1.0) * q).round() as usize] };
    Stats { warmup, median: pick(0.5), p95: pick(0.95), p99: pick(0.99), max: rest.last().copied().unwrap_or(0), total, ok }
}

fn report(op: &str, n: usize, distinct: usize, bytes: u64, s: &Stats) {
    report_x(op, n, distinct, bytes, s, "")
}

fn report_x(op: &str, n: usize, distinct: usize, bytes: u64, s: &Stats, extra: &str) {
    eprintln!(
        "MEASURE fixture_only=true clock=wall_elapsed stats_exclude_warmup=true op={op} n={n} distinct={distinct} exemplar_body_bytes={bytes} warmup_wall_ns={} median_wall_ns={} p95_wall_ns={} p99_wall_ns={} max_wall_ns={} total_wall_ns={} observed_expected_outcomes={}{extra}",
        s.warmup, s.median, s.p95, s.p99, s.max, s.total, s.ok
    );
    assert!(distinct > 0, "selected operation produced no native cases");
    assert_eq!(s.ok, n, "timed verifier did not produce its expected outcome");
}

fn scale(default: usize) -> usize {
    std::env::var("MEASURE_SCALE").ok().map(|v| v.parse().expect("MEASURE_SCALE usize")).unwrap_or(default)
}

fn bond(n: u64) -> PalwBondKeyV2 {
    PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
}

/// Terminal tile phases reached by naming the first / the last child at every round (as in `palw_tir_position_terminal_v2.rs`).
fn terminal_phases(x: &Execution, store: &Store<'_>, leaf: u64, site: &PalwTirDissectSiteV1) -> Vec<PalwTirDissectPhaseV1> {
    let mut out = Vec::new();
    for last in [false, true] {
        let root = build_tir_root_claim_v1(&x.binding, leaf, store, &RULES).expect("honest root");
        let mut phase = PalwTirDissectPhaseV1::open(SESSION, leaf, site, &root, 2, 0, W).expect("opens");
        let mut t = 1;
        while phase.turn() == PalwBisectTurnV1::AwaitDisclosure {
            let round = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, store, &RULES).expect("a round");
            phase.apply_round(&round, t, W).expect("honest round folds");
            let pick = if last { phase.child_ranges().len() as u8 - 1 } else { 0 };
            phase.apply_choice(&choice(phase.round(), pick), t + 1, W).expect("a legal choice");
            t += 2;
        }
        if phase.turn() == PalwBisectTurnV1::Terminal && !out.contains(&phase) {
            out.push(phase);
        }
    }
    out
}

/// One position-close case: the terminal phase, the signed-object dissection, the challenger's choice, the bottom, the leaf.
type PositionCase = (PalwTirDissectPhaseV1, PalwTirPositionDissectionV2, PalwTirPositionChoiceV2, PalwTirConeRefutationV1, u64);

fn position_close_bytes(d: &PalwTirPositionDissectionV2, c: &PalwTirPositionChoiceV2, bottom: &PalwTirConeRefutationV1) -> u64 {
    let close = PalwTirPositionCloseV2 {
        version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
        session_id: SESSION,
        verdict: PalwCourtVerdictV2::ChallengerDefeated,
        dissection: d.clone(),
        responder_signature: vec![0; SIG],
        choice: c.clone(),
        challenger_signature: vec![0; SIG],
        proof: PalwCourtVerdictProofV2::TirDissection { bottom: Box::new(bottom.clone()) },
        step_multiproof: None,
    };
    borsh::to_vec(&close).unwrap().len() as u64
}

fn position_cases(fixtures: &[Fixture], tile: u32, forged: bool) -> Vec<PositionCase> {
    let mut out = Vec::new();
    'o: for f in fixtures {
        let x = f.honest();
        let store = Store { f, x: &x };
        for (leaf, mut site) in dissected(f) {
            site.tile_positions = tile;
            let k_sum = site.folds.iter().position(|fold| *fold == PalwTirFoldV1::Sum);
            for phase in terminal_phases(&x, &store, leaf, &site) {
                let narrowing = phase.position_narrowing_v1().expect("terminal");
                if !narrowing.awaits_position_round() {
                    continue;
                }
                let (from, to) = narrowing.tile_range();
                let width = to - from;
                let honest = build_tir_dissect_position_round_v1(&x.binding, &narrowing, &store, &RULES).expect("position round");
                let (round, child) = if forged {
                    let Some(k) = k_sum else { continue };
                    if width < 3 || phase.elements()[k].is_empty() {
                        continue;
                    }
                    let middle = (width / 2) as usize;
                    let mut lie = honest.clone();
                    lie.children[middle].partials[k][0] += 1;
                    lie.children[0].partials[k][0] -= 1;
                    (lie, middle as u8)
                } else {
                    (honest, 0u8)
                };
                let Some(d) = palw_tir_position_dissection_for_v2(&phase, round) else { continue };
                if check_tir_position_dissection_v2(&phase, &d).is_err() {
                    continue;
                }
                let c = PalwTirPositionChoiceV2 {
                    version: PALW_TIR_POSITION_OBJECT_VERSION_V2,
                    session_id: SESSION,
                    tile_round: phase.round(),
                    dissection_digest: palw_tir_position_dissection_digest_v2(&d),
                    position_child: child,
                };
                let named = palw_tir_position_named_v2(&phase, &d, &c).expect("names a position");
                let bottom = build_tir_dissect_position_bottom_v1(&x.binding, &named, &store, &RULES).expect("position bottom");
                out.push((phase, d, c, bottom, leaf));
                if out.len() >= POSITION_DISTINCT_CAP {
                    break 'o;
                }
            }
        }
    }
    out
}

/// A seat's readiness root over `count` committed digests, and the digests.
fn d2_root(count: u32, beacon: u64) -> (rr::SeatReadinessRootV1, Vec<(u32, Hash64)>) {
    let ctx =
        rr::ReadinessContextV1 { beacon: Hash64::from_u64_word(beacon), class_id: Hash64::from_u64_word(12), seat: bond(3), span: 4 };
    let ds: Vec<_> = (0..count).map(|i| (i * 2, Hash64::from_u64_word(500 + u64::from(i)))).collect();
    (rr::SeatReadinessRootV1::from_digests(ctx, &ds, NonZeroU32::new(count)).unwrap(), ds)
}

fn d2_demand(rk: Hash64, ordinal: u32) -> dc::PalwD2DisclosureDemandV1 {
    dc::PalwD2DisclosureDemandV1 { root_key: rk, ordinal, challenger: bond(9), fee_sompi: 7, valid_until_daa: D2_AT + 5 }
}

fn forged_cone_cases(fixtures: &[Fixture]) -> Vec<(PalwTirConeRefutationV1, u64)> {
    let mut out = Vec::new();
    'outer: for f in fixtures {
        for i in 0..f.leaves.len() {
            let leaf = &f.leaves[i];
            let lane = leaf.value_count as usize / 2;
            let v = f.values[i][lane];
            let iv = f.interval(leaf);
            let Some(forged) = [v.checked_add(1), v.checked_sub(1)].into_iter().flatten().find(|w| iv.contains(*w)) else { continue };
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
    assert!(OPS.contains(&op.as_str()), "unknown MEASURE_OP {op}; valid: {OPS:?}");
    assert!(n <= MAX_MEASURE_ITERATIONS, "MEASURE_N exceeds bounded fixture setup ({MAX_MEASURE_ITERATIONS})");
    let fixtures = fixtures();

    match op.as_str() {
        // (c) close verification, honest leaf: the whole cone recomputation, ends NoFaultFound.
        "close_cone_acquit" => {
            let mut cases = honest_cone_cases(&fixtures);
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats::default());
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
                return report(&op, 0, cases.len(), 0, &Stats::default());
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
                return report(&op, 0, cases.len(), 0, &Stats::default());
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
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let bytes = borsh::to_vec(&cases[0].1).unwrap().len() as u64;
            let distinct = cases.len();
            let mut phases: Vec<(PalwTirDissectPhaseV1, PalwTirDissectRoundV1)> =
                (0..n).map(|i| cases[i % distinct].clone()).collect();
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
                        let round =
                            build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("round");
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
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let bytes = borsh::to_vec(&cases[0].1).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |(phase, bottom, leaf)| {
                check_tir_dissect_bottom_v1(phase, bottom, *leaf, &RULES) == Err(PalwStepRefuteError::NoFaultFound)
            });
            // The dissection rounds the honest descent took to its terminal tile (B_localize's round count on these classes).
            let max_round = cases.iter().map(|c| c.0.round()).max().unwrap_or(0);
            let max_bytes = cases.iter().map(|c| borsh::to_vec(&c.1).unwrap().len() as u64).max().unwrap_or(0);
            report_x(&op, n, distinct, bytes, &s, &format!(" max_rounds_to_terminal={max_round} max_bottom_bytes={max_bytes}"));
        }
        // RFC-0024 §4: the position close at a terminal tile of width MEASURE_SCALE (default 4): the V2 dissection binds to the
        // phase, the challenger's choice names a position, and the court recomputes that one position (honest: NoFaultFound; a
        // lie moved into the middle: convicted). Timed: bind + name + position bottom check. Bytes: the whole close object with
        // two ML-DSA-87 signatures (the witness the position budget V8 bounds).
        "position_close_honest" | "position_close_forged" => {
            let tile = scale(4) as u32;
            let forged = op == "position_close_forged";
            let mut cases = position_cases(&fixtures, tile, forged);
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let sizes: Vec<u64> = cases.iter().map(|(_, d, c, b, _)| position_close_bytes(d, c, b)).collect();
            let bytes = sizes[0];
            let max_bytes = *sizes.iter().max().unwrap();
            let distinct = cases.len();
            let s = run(&mut cases, n, |(phase, d, c, bottom, leaf)| {
                if check_tir_position_dissection_v2(phase, d).is_err() {
                    return false;
                }
                let Ok(named) = palw_tir_position_named_v2(phase, d, c) else { return false };
                let verdict = check_tir_dissect_position_bottom_v1(phase, &named, bottom, *leaf, &RULES);
                if forged { verdict.is_ok() } else { verdict == Err(PalwStepRefuteError::NoFaultFound) }
            });
            report_x(
                &op,
                n,
                distinct,
                bytes,
                &s,
                &format!(" tile_positions={tile} max_close_bytes={max_bytes} signature_bytes={}", 2 * SIG),
            );
        }
        // RFC-0024 §5: the depth-2 lookahead disclosure expanded and checked against the phase (Σ/max reconstruction of the
        // omitted level). Test-side frontier ceiling only (the production bound V8 is typed-missing).
        "lookahead_expand" => {
            let mut cases: Vec<(PalwTirDissectPhaseV1, PalwTirLookaheadDisclosureV2, u64)> = Vec::new();
            'o: for f in &fixtures {
                let x = f.honest();
                let store = Store { f, x: &x };
                for (leaf, site) in dissected(f) {
                    let root = build_tir_root_claim_v1(&x.binding, leaf, &store, &RULES).expect("root");
                    let phase = PalwTirDissectPhaseV1::open(SESSION, leaf, &site, &root, 2, 0, W).expect("opens");
                    if phase.turn() != PalwBisectTurnV1::AwaitDisclosure {
                        continue;
                    }
                    let level1 = build_tir_dissect_round_v1(&x.binding, &phase, site.tile_positions, &store, &RULES).expect("level 1");
                    let mut frontier = Vec::new();
                    for (i, &(_, count)) in phase.child_ranges().iter().enumerate() {
                        if count <= 1 {
                            frontier.push(vec![level1.children[i].clone()]);
                        } else {
                            let child = phase.project_unrooted_child_v1(&level1, i as u8).expect("projects");
                            let level2 =
                                build_tir_dissect_round_v1(&x.binding, &child, site.tile_positions, &store, &RULES).expect("level 2");
                            frontier.push(level2.children);
                        }
                    }
                    let values: u64 = frontier.iter().flatten().map(|c| c.partials.iter().map(|v| v.len() as u64).sum::<u64>()).sum();
                    cases.push((
                        phase,
                        PalwTirLookaheadDisclosureV2 { version: PALW_TIR_LOOKAHEAD_OBJECT_VERSION_V2, frontier },
                        values,
                    ));
                    if cases.len() >= DISTINCT_CAP {
                        break 'o;
                    }
                }
            }
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let bytes = borsh::to_vec(&cases[0].1).unwrap().len() as u64;
            let max_bytes = cases.iter().map(|c| borsh::to_vec(&c.1).unwrap().len() as u64).max().unwrap();
            let max_values = cases.iter().map(|c| c.2).max().unwrap();
            let distinct = cases.len();
            let s = run(&mut cases, n, |(phase, msg, _)| phase.expand_lookahead_v2(msg, LOOKAHEAD_TEST_CAP).is_ok());
            report_x(&op, n, distinct, bytes, &s, &format!(" max_disclosure_bytes={max_bytes} max_frontier_values={max_values}"));
        }
        // ADR-0200 C1 (tag 231): open a disclosure demand on a recorded root of MEASURE_SCALE (default 256) committed digests.
        // Test-side cap and fee (production values are typed-missing). The validator's ML-DSA-87 check is `mldsa87_verify`.
        "d2_demand_open" => {
            let count = scale(256) as u32;
            let (root, _) = d2_root(count, 11);
            let cap = NonZeroU32::new(count);
            let mut registry = dc::PalwD2ReadinessRegistryV1::new();
            let (k, e) = dc::palw_d2_readiness_record_v1(&registry, &root, cap, D2_AT).unwrap();
            registry.insert(k, e);
            let rk = rr::palw_d2_readiness_root_key_v1(&root.context());
            let mut cases: Vec<dc::PalwD2DisclosureDemandV1> = (0..count.min(DISTINCT_CAP as u32)).map(|o| d2_demand(rk, o)).collect();
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let bytes = borsh::to_vec(&cases[0]).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |d| dc::palw_d2_disclosure_open_v1(&registry, d, cap, D2_FEES, D2_AT + 1).is_ok());
            report_x(&op, n, distinct, bytes, &s, &format!(" draw_count={count} signed_bytes={}", bytes + SIG as u64));
        }
        // Tag 232: the seat's answer — the authenticated digest path of the demanded ordinal (disclosed), or a wrong path its own
        // root refutes (convicted). Every ordinal has an open case.
        "d2_answer_disclosed" | "d2_answer_convicted" => {
            let count = scale(256) as u32;
            let (root, ds) = d2_root(count, 11);
            let cap = NonZeroU32::new(count);
            let mut registry = dc::PalwD2ReadinessRegistryV1::new();
            let (k, e) = dc::palw_d2_readiness_record_v1(&registry, &root, cap, D2_AT).unwrap();
            registry.insert(k, e);
            let rk = rr::palw_d2_readiness_root_key_v1(&root.context());
            let convicted = op == "d2_answer_convicted";
            let mut cases = Vec::new();
            for o in 0..count.min(DISTINCT_CAP as u32) {
                let (k, e) = dc::palw_d2_disclosure_open_v1(&registry, &d2_demand(rk, o), cap, D2_FEES, D2_AT + 1).unwrap();
                registry.insert(k, e);
                let mut path = rr::palw_d2_readiness_path_v1(&root.context(), &ds, o as usize);
                if convicted {
                    path.claimed_digest = Hash64::from_u64_word(999_999);
                }
                cases.push(dc::PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: o, path });
            }
            if n == 0 {
                return report(&op, 0, cases.len(), 0, &Stats::default());
            }
            let bytes = borsh::to_vec(&cases[0]).unwrap().len() as u64;
            let distinct = cases.len();
            let s = run(&mut cases, n, |a| match dc::palw_d2_disclosure_answer_v1(&registry, a, D2_AT + 2) {
                Ok(w) => w.len() == if convicted { 2 } else { 1 },
                Err(_) => false,
            });
            let extra = format!(" draw_count={count} registry_entries={} signed_bytes={}", registry.len(), bytes + SIG as u64);
            report_x(&op, n, distinct, bytes, &s, &extra);
        }
        // The block's D2 deadline sweep over a registry holding MEASURE_SCALE (default 256) open cases (64 per root), all past
        // their deadline: one sample = one whole sweep (its per-block cost grows with the open cases).
        "d2_sweep" => {
            let open = scale(256);
            let per_root = 64u32;
            let mut registry = dc::PalwD2ReadinessRegistryV1::new();
            let mut made = 0usize;
            for r in 0..open.div_ceil(per_root as usize) {
                let (root, _) = d2_root(per_root, 11 + r as u64);
                let cap = NonZeroU32::new(per_root);
                let (k, e) = dc::palw_d2_readiness_record_v1(&registry, &root, cap, D2_AT).unwrap();
                registry.insert(k, e);
                let rk = rr::palw_d2_readiness_root_key_v1(&root.context());
                for o in 0..per_root {
                    if made == open {
                        break;
                    }
                    let (k, e) = dc::palw_d2_disclosure_open_v1(&registry, &d2_demand(rk, o), cap, D2_FEES, D2_AT + 1).unwrap();
                    registry.insert(k, e);
                    made += 1;
                }
            }
            let after = D2_AT + 1 + dc::PALW_D2_DISCLOSURE_TURN_DAA_V1 + 1;
            if n == 0 {
                return report(&op, 0, 1, 0, &Stats::default());
            }
            let mut cases = vec![()];
            let s = run(&mut cases, n, |_| dc::palw_d2_disclosure_sweep_v1(&registry, after, true).len() >= made);
            report_x(&op, n, 1, 0, &s, &format!(" open_cases={made} registry_entries={}", registry.len()));
        }
        // One ML-DSA-87 verification of a 64-byte message digest: what the validator spends on each signed court object (a
        // demand, an answer, a dissection, a choice, a material intake step, a shared bottom).
        "mldsa87_verify" => {
            let distinct_n = 64usize;
            let mut cases: Vec<(Vec<u8>, Vec<u8>, Vec<u8>)> = (0..distinct_n)
                .map(|i| {
                    let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([i as u8; 32]);
                    let msg = Hash64::from_u64_word(i as u64 + 1).as_byte_slice().to_vec();
                    let sig = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &msg, b"misaka-palw/measure/v1", [7; 32]).unwrap();
                    (kp.verification_key.as_ref().to_vec(), msg, sig.as_ref().to_vec())
                })
                .collect();
            if n == 0 {
                return report(&op, 0, distinct_n, 0, &Stats::default());
            }
            let bytes = cases[0].2.len() as u64;
            let pk_bytes = cases[0].0.len();
            let s = run(&mut cases, n, |(pk, msg, sig)| {
                kaspa_consensus_core::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1(
                    pk,
                    msg,
                    sig,
                    b"misaka-palw/measure/v1",
                )
            });
            report_x(&op, n, distinct_n, bytes, &s, &format!(" public_key_bytes={pk_bytes}"));
        }
        // The fixture construction alone (the RSS / time baseline every op above includes).
        "fixture_setup_only" => {
            report(&op, 0, fixtures.len(), 0, &Stats::default());
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
