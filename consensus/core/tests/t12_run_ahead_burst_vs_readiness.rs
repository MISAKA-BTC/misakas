//! **A run-ahead burst against a readiness row, on the lifecycle fold** — the 2026-09-25
//! mainnet-values review's HIGH, measured rather than inferred. **Open for the user's decision
//! before launch: nothing here is a fix.**
//!
//! testnet-12's future-drift bound became mainnet's 1,620 s (user decision 2026-09-25). The clock
//! floor still spaces two tick STAMPS one interval apart, but nothing spaces them in wall time: a
//! producer holding the next slot the moment before it opens may mint every slot whose stamp is at
//! most `now + T` at once — `⌊T / I⌋ + 1` ticks, 14 at 1,620 s and 2 at the 132 s testnet-12 ran
//! before — each a DAA on testnet-12's one-DAA spans, with no other block, and so no possession
//! proof, between them (`t12_a_producer_runs_the_clock_at_most_the_drift_budget_ahead` mints them
//! through the pipeline: 12 from a reference its setup leaves up to one interval ahead). Afterwards
//! the chain waits for wall time to catch up, and the producer can do it again.
//!
//! A readiness row stands `H` DAA; the node re-proves once it is older than `H / 2`
//! (`palw_readiness_duty_due_v2`); the proof names its span, rides the next block and counts from the
//! chain block that merges it (the M1 escalation's two DAA). A burst holds every proof sent at or
//! after its first DAA until the first block after it. The class needs `seat_count` fresh rows at a
//! span boundary to stay drawable (`palw_lifecycle_step_v1`): one boundary short and an Active class
//! is HELD, and a HELD class comes back only as `Probation { 0 }` — ten probe claims and three stable
//! epochs again.
//!
//! Measured for testnet-12's eight genesis seats, open lane, with the burst timed where it hurts most
//! over one re-prove cycle: seats all proved at one DAA (a fleet that launched together) or staggered
//! over five DAA (the M1 network model's stagger). `H = 8` is the V2 horizon this build's fold runs,
//! through the fold's own functions; 24 is the readiness-horizon line's testnet-12 genesis param
//! (`3e9ae4ba`, merged on rcore/int-3 as `c07c6d49`), and 30 the ceiling that param admits, through
//! the same arithmetic (checked against the fold's at 8).
//!
//! Run: cargo test -p kaspa-consensus-core --test t12_run_ahead_burst_vs_readiness -- --nocapture

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS as I;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::{
    PALW_REGISTRY_GLOBALS_V1 as G, PalwDerivedProfileV1, PalwLifecycleObservationV1, PalwManifestVerdictV1Flag, PalwModelLifecycleV1,
    PalwSeatReadinessRowV1, palw_lifecycle_step_v1, palw_readiness_duty_due_v2, palw_readiness_max_age_daa_v1,
    palw_readiness_row_is_fresh_v1,
};
use kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2;

/// **The ticks one producer can mint with nothing between them**: from the moment before the next
/// slot opens (`r + I`), every slot `r + k·I` with `r + k·I ≤ (r + I) + T`.
fn burst(tolerance_s: u64) -> u64 {
    tolerance_s * 1_000 / I + 1
}

fn row(proved_daa: u64) -> PalwSeatReadinessRowV1 {
    PalwSeatReadinessRowV1 { proved_daa, proved_span: proved_daa, leaf_index: 0, proof_version: 2, chunks: 16 }
}

/// Which readiness rule the seats are judged and re-prove by.
#[derive(Clone, Copy, Debug)]
enum Rule {
    /// The fold's own functions, at this build's V2 horizon.
    Fold,
    /// A horizon of `h` DAA, the node re-proving once a row is older than `due` — `h / 2` is the
    /// node's rule; a smaller `due` is the "re-prove ahead of the burst" option.
    Horizon { h: u64, due: u64 },
}

impl Rule {
    fn horizon(self) -> u64 {
        match self {
            Rule::Fold => palw_readiness_max_age_daa_v1(1, &G, true),
            Rule::Horizon { h, .. } => h,
        }
    }
    fn fresh(self, proved: u64, now: u64) -> bool {
        match self {
            Rule::Fold => palw_readiness_row_is_fresh_v1(&row(proved), now, 1, &G, true),
            Rule::Horizon { h, .. } => now - proved <= h,
        }
    }
    fn due(self, proved: u64, now: u64) -> bool {
        match self {
            Rule::Fold => palw_readiness_duty_due_v2(Some(&row(proved)), now, now, None, 1, &G, true),
            Rule::Horizon { due, .. } => now - proved > due,
        }
    }
}

struct Run {
    /// The fewest fresh rows at any boundary.
    min_fresh: usize,
    /// The first boundary the class was HELD at, and where it stood at the end.
    held_at: Option<u64>,
    end: PalwModelLifecycleV1,
}

/// One class, `phases.len()` seats, one DAA a span. Honest blocks carry every proof in flight; the
/// burst's `b` blocks after DAA `s0` carry none. A proof sent at DAA `t` rides the first honest block
/// past `t` and counts from the chain block after that. The class starts Active and is stepped at
/// every boundary from the burst on.
fn run(rule: Rule, phases: &[u64], s0: u64, b: u64, required: u32) -> Run {
    let horizon = rule.horizon();
    let start = 10 * horizon;
    let mut proved: Vec<u64> = phases.iter().map(|p| start - p).collect();
    // (span named, the DAA it counts from once carried)
    let mut in_flight: Vec<Option<(u64, Option<u64>)>> = vec![None; phases.len()];
    let profile = PalwDerivedProfileV1 { required_ready_seats: required, ..Default::default() };
    let mut state = PalwModelLifecycleV1::Active;
    let mut out = Run { min_fresh: usize::MAX, held_at: None, end: state };
    for t in start + 1..=s0 + b + 4 * horizon {
        let honest = !(s0 + 1..=s0 + b).contains(&t);
        for s in 0..phases.len() {
            if let Some((span, counts)) = in_flight[s] {
                match counts {
                    Some(at) if at == t => {
                        proved[s] = span;
                        in_flight[s] = None;
                    }
                    None if honest => in_flight[s] = Some((span, Some(t + 1))),
                    _ => {}
                }
            }
        }
        let fresh = (0..phases.len()).filter(|s| rule.fresh(proved[*s], t)).count();
        if t > s0 {
            out.min_fresh = out.min_fresh.min(fresh);
            let obs = PalwLifecycleObservationV1 {
                manifest: PalwManifestVerdictV1Flag::Valid,
                ready_seats: fresh as u32,
                collateral_ok: true,
                cap_ok: true,
                window_fits_receipt: true,
                span_stable: true,
                ..Default::default()
            };
            state = palw_lifecycle_step_v1(state, &obs, &profile, &G);
            if state == PalwModelLifecycleV1::Held && out.held_at.is_none() {
                out.held_at = Some(t);
            }
        }
        // Each seat, having seen block `t`, sends its proof if one is due and none is in flight.
        for s in 0..phases.len() {
            if in_flight[s].is_none() && rule.due(proved[s], t) {
                in_flight[s] = Some((t, None));
            }
        }
    }
    out.end = state;
    out
}

/// The burst timed where it hurts most: every start over two re-prove cycles, the worst kept.
fn worst(rule: Rule, phases: &[u64], b: u64, required: u32) -> (u64, Run) {
    let horizon = rule.horizon();
    (12 * horizon..12 * horizon + horizon + 4)
        .map(|s0| (s0, run(rule, phases, s0, b, required)))
        .min_by_key(|(s0, r)| (r.min_fresh, *s0))
        .unwrap()
}

#[test]
fn t12_a_run_ahead_burst_outlasts_a_readiness_row() {
    let t12 = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let PalwConsensusMode::ConsensusV2(bundle) = &t12.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let seats = bundle.genesis_objects.iter().filter(|o| matches!(o, PalwConsensusObjectV2::BondRegistered { .. })).count();
    assert_eq!(seats, 8, "testnet-12's eight genesis seats");
    let (seat_count, required) = (G.seat_count as u32, (G.seat_count + G.spare_seats) as u32);
    assert_eq!((seat_count, required), (5, 7), "drawable at five fresh rows, back to Probation at seven");
    assert_eq!(t12.timestamp_deviation_tolerance, 1_620, "testnet-12 runs mainnet's tolerance");
    let (b_shipped, b_before) = (burst(t12.timestamp_deviation_tolerance), burst(132));
    assert_eq!((b_shipped, b_before), (14, 2), "one producer's burst: 14 DAA at 1,620 s, 2 at 132 s");

    // The arithmetic the other horizons run is the fold's own at 8.
    let h8 = Rule::Fold.horizon();
    assert_eq!(h8, 8, "this build's V2 horizon on one-DAA spans");
    let local = Rule::Horizon { h: 8, due: 4 };
    for proved in 90..=100u64 {
        for now in 100..=112u64 {
            assert_eq!(Rule::Fold.fresh(proved, now), local.fresh(proved, now), "fresh at age {}", now - proved);
            assert_eq!(Rule::Fold.due(proved, now), local.due(proved, now), "due at age {}", now - proved);
        }
    }

    let aligned = vec![0u64; seats];
    let staggered: Vec<u64> = (0..seats as u64).map(|s| s % 5).collect();
    let cases = [
        ("132 s, H 8 (before 2026-09-25)", Rule::Fold, b_before),
        ("1,620 s, H 8 (this build)", Rule::Fold, b_shipped),
        ("1,620 s, H 24 (rcore/int-3's t12 horizon)", Rule::Horizon { h: 24, due: 12 }, b_shipped),
        ("1,620 s, H 30 (the horizon's ceiling)", Rule::Horizon { h: 30, due: 15 }, b_shipped),
        ("1,620 s, H 24, re-proving past H − B − 2 = 8", Rule::Horizon { h: 24, due: 24 - b_shipped - 2 }, b_shipped),
    ];
    let mut measured = Vec::new();
    for (name, rule, b) in cases {
        for (layout, phases) in [("aligned", &aligned), ("staggered", &staggered)] {
            let (s0, r) = worst(rule, phases, b, required);
            println!(
                "[burst] {name}, {layout}: B = {b}, worst start DAA {s0}: {} of {seats} rows lapsed, class {} -> {:?}",
                seats - r.min_fresh,
                r.held_at.map_or("never HELD".to_string(), |at| format!(
                    "HELD at DAA {at} = s0 + {} (the burst is s0 + 1 ..= s0 + {b})",
                    at - s0
                )),
                r.end
            );
            measured.push((name, layout, seats - r.min_fresh, r.held_at.is_some(), r.end));
        }
    }
    let at = |name: &str, layout: &str| *measured.iter().find(|m| m.0 == name && m.1 == layout).unwrap();
    for layout in ["aligned", "staggered"] {
        // At 132 s the burst fits the escalation's two-DAA margin: no row lapses.
        let before = at("132 s, H 8 (before 2026-09-25)", layout);
        assert_eq!((before.2, before.3, before.4), (0, false, PalwModelLifecycleV1::Active), "{layout}: 132 s, H 8");
        // At 1,620 s and this build's horizon every row lapses, and the class restarts its probation.
        let now = at("1,620 s, H 8 (this build)", layout);
        assert_eq!(
            (now.2, now.3, now.4),
            (seats, true, PalwModelLifecycleV1::Probation { probes_passed: 0 }),
            "{layout}: 1,620 s, H 8"
        );
        // Re-proving far enough ahead of the horizon closes it at 24.
        let ahead = at("1,620 s, H 24, re-proving past H − B − 2 = 8", layout);
        assert_eq!((ahead.2, ahead.3), (0, false), "{layout}: re-proving at age 9 of 24");
    }
    // The integrated horizon alone does not close it: a fleet proved together lapses whole, and
    // even a staggered one loses more than three seats.
    for layout in ["aligned", "staggered"] {
        let h24 = at("1,620 s, H 24 (rcore/int-3's t12 horizon)", layout);
        assert!(h24.3, "{layout}: 1,620 s, H 24 still HOLDS the class ({} rows lapsed)", h24.2);
    }
    // At the ceiling only the seats that sent at the burst's first DAA lapse — all of an aligned
    // fleet, two of the staggered one.
    let (a30, s30) =
        (at("1,620 s, H 30 (the horizon's ceiling)", "aligned"), at("1,620 s, H 30 (the horizon's ceiling)", "staggered"));
    assert!(a30.3 && a30.2 == seats, "aligned at H 30: every row lapses for a DAA");
    assert!(!s30.3, "staggered at H 30: {} rows lapse and the class stays drawable", s30.2);
}
