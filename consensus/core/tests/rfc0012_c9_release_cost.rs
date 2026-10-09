//! **RFC-0012 C9 - the release-build cost of the native-settlement path at 100,000+ blocks. A MEASUREMENT DESIGN, NOT RUN in wave 2** (the
//! coordinator: design it, do not run it). Everything is `#[ignore]`; it compiles with the other tests so it cannot rot unseen.
//!
//! **Why it is needed.** Policy proposal section 6 quotes a DEBUG-build measurement (83 ms for 160,000 effects and 80,000 facts) and an
//! extrapolation (the cold rebuild of `NativeRowCache` at 10 s per 100,000 blocks on one SSD: "estimate, not measured on the node"). The
//! sweep runs on every virtual change; the readiness explanation runs on demand, memoized per sink. Neither figure is a number to arm on.
//!
//! **How to run it (the protocol - all of it matters).**
//! * The fleet's hardware class (the same CPU generation and the same disk as a t12 seat), the **release** profile, the machine otherwise
//!   idle: `uptime` load below 1 per core, no other build or node running; record `uname -a`, `rustc -V`, the commit.
//! * `cargo test --release --offline -p kaspa-consensus-core --test rfc0012_c9_release_cost -- --ignored --nocapture --test-threads=1`
//! * Three runs; keep the median of each figure; the raw lines go into the implementation record verbatim.
//!
//! **What each test measures, and what would make the answer "no".**
//! * `c9_a`: `certify_native_prefix_v1` at N = 10k ... 1.28M effects, in the DENSE shape of section 6 (F = N/2 facts - every second block
//!   finalizes something, a stress shape) and the MEASURED shape (one REAL claim finalizing per 123 blocks, F = N/123: the fold's own `F_off`
//!   and an honest producer's rate). PROPOSED gates: <= 40 ms at 160k dense; <= 5 ms at 160k measured-density; growth per 4x input <= 4.6x
//!   (near-linear). A superlinear step at the top size means the O(N log N) sort or the anchor map dominates and the sweep needs chunking.
//! * `c9_b`: `native_safe_readiness_v1` (the explanation) on the same chains with 10,000 open claims and 100 sessions. PROPOSED gate: <= 2x
//!   the sweep (it re-derives per-effect state for two effects, not N).
//!
//! **Not in this file - the two figures that need a node (`rfc12_c9_cold_rebuild` in the consensus lib tests, to be written when run).**
//! * *Cold rebuild of `NativeRowCache`*: a devnet database (the drill network, or the harness filled with N synthetic delta rows via
//!   `palw_state_v2_store.set_delta_record_for_tests` and N EVM headers) opened fresh; time the FIRST `native_evaluate` (every row is a
//!   store read plus a delta decode) and the SECOND (all cache hits). Run with the OS page cache cold (`sudo purge` on macOS /
//!   `echo 3 > /proc/sys/vm/drop_caches` on Linux) and warm. PROPOSED gate: cold first evaluation <= 30 s per 100k blocks on the fleet disk
//!   and the second <= 50 ms. A node that takes longer than one block interval to answer a virtual change when warm is the failing case.
//! * *Memory*: RSS before and after the cold rebuild; the design number is 352 B per row (+ map entry) = 42 MB per 100k blocks.
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_native_readiness_v1::{
    ClaimStageV1, FinalizedReadinessV1, FinalizedWaitV1, NativeReadinessInputV1, OpenClaimV1, OpenSessionV1, native_safe_readiness_v1,
};
use kaspa_consensus_core::palw_native_settlement_v1::{
    MatureUsefulWorkV1, NativeEffectV1, PalwSettlementPolicyV1, SkippedEvidenceV1, certify_native_prefix_v1,
};
use std::time::{Duration, Instant};

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn policy() -> PalwSettlementPolicyV1 {
    // Proposed shape (section 4): D = 3, a work floor, 400/1000 permille. The numbers are the proposal's, used only to give the sweep
    // realistic work; this file measures cost, not whether a value is right.
    PalwSettlementPolicyV1 {
        settled_anchor_depth: 3,
        unique_mature_work: 6_331_093_440,
        max_operator_permille: 400,
        max_class_permille: 1000,
    }
}

/// `n` effects at one block per DAA, `facts` facts spread evenly over the chain, `operators` operators round-robin, every fact mature.
fn chain(n: u64, facts: u64) -> (Vec<NativeEffectV1>, Vec<MatureUsefulWorkV1>) {
    let effects: Vec<NativeEffectV1> =
        (1..=n).map(|i| NativeEffectV1 { daa: i, blue: i, frontier_covers: true, lifecycle_closed: true }).collect();
    let stride = (n / facts.max(1)).max(1);
    let facts: Vec<MatureUsefulWorkV1> = (0..facts)
        .map(|k| {
            let at = (1 + k * stride).min(n);
            MatureUsefulWorkV1 {
                identity: h(0x1_0000_0000 + k),
                anchor: h(0x2_0000_0000 + k),
                operator: h(k % 8),
                class: h(0x100 + k % 2),
                anchor_blue: at,
                accepted_blue: at,
                anchor_daa: at,
                accepted_daa: at,
                matured_daa: at,
                work: 2_110_364_480,
            }
        })
        .collect();
    (effects, facts)
}

fn median(mut runs: Vec<Duration>) -> Duration {
    runs.sort();
    runs[runs.len() / 2]
}

fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> Duration {
    median(
        (0..runs)
            .map(|_| {
                let started = Instant::now();
                std::hint::black_box(f());
                started.elapsed()
            })
            .collect(),
    )
}

#[test]
#[ignore = "release-build measurement on fleet-class hardware; see the module doc for the protocol"]
fn rfc0012_c9_a_the_sweep_at_100k_and_more_blocks() {
    assert!(!cfg!(debug_assertions), "this measurement is meaningful only in the release profile");
    let sizes = [10_000u64, 40_000, 160_000, 640_000, 1_280_000];
    for (shape, density) in [("dense (F = N/2)", 2u64), ("measured (F = N/123)", 123)] {
        let mut previous: Option<(u64, Duration)> = None;
        for n in sizes {
            let (effects, facts) = chain(n, n / density);
            let took = time(7, || certify_native_prefix_v1(policy(), &effects, n, &facts));
            let growth = previous.map(|(pn, pt)| (n as f64 / pn as f64, took.as_secs_f64() / pt.as_secs_f64().max(1e-9)));
            eprintln!(
                "[c9-a] {shape:<22} N = {n:>9}: {took:>10.3?}  ({:.1} ns/effect){}",
                took.as_nanos() as f64 / n as f64,
                growth.map_or(String::new(), |(x, t)| format!("  input x{x:.0} -> time x{t:.2}"))
            );
            if let Some((x, t)) = growth {
                assert!(t <= x * 1.15, "{shape} N = {n}: time grew x{t:.2} for an input x{x:.0}: not near-linear");
            }
            if n == 160_000 {
                let budget = if density == 2 { Duration::from_millis(40) } else { Duration::from_millis(5) };
                assert!(took <= budget, "{shape}: 160k effects took {took:?}, the proposed gate is {budget:?}");
            }
            previous = Some((n, took));
        }
    }
}

#[test]
#[ignore = "release-build measurement on fleet-class hardware; see the module doc for the protocol"]
fn rfc0012_c9_b_the_readiness_explanation_at_100k_and_more_blocks() {
    assert!(!cfg!(debug_assertions), "this measurement is meaningful only in the release profile");
    for n in [10_000u64, 160_000, 640_000] {
        let (effects, facts) = chain(n, n / 123);
        let pairs: Vec<(Hash64, NativeEffectV1)> = effects.iter().enumerate().map(|(i, e)| (h(0xB000_0000 + i as u64), *e)).collect();
        let prefix = certify_native_prefix_v1(policy(), &effects, n, &facts);
        let claims: Vec<OpenClaimV1> = (0..10_000u64)
            .map(|k| OpenClaimV1 {
                claim: h(0xC000_0000 + k),
                stage: ClaimStageV1::Final,
                accepted_blue: n.saturating_sub(5_000) + k / 2,
                retention_daa: n + 1 + k,
                next_deadline_daa: None,
            })
            .collect();
        let sessions: Vec<OpenSessionV1> = (0..100u64)
            .map(|k| OpenSessionV1 { claim: h(0xD000_0000 + k), claim_accepted_blue: Some(n - 10 - k), deadline_daa: n + 600 + k })
            .collect();
        let input = NativeReadinessInputV1 {
            generation: h(0x51),
            sink_daa: n,
            sink_blue: n,
            policy: policy(),
            claim_retirement_daa: 3_000,
            quantum_maturity_daa: 120,
            chain: &pairs,
            prefix: &prefix,
            facts: &facts,
            frontier_blue: n,
            frontier_on_branch: true,
            open_claims: &claims,
            open_sessions: &sessions,
            skipped: SkippedEvidenceV1::default(),
            finalized: FinalizedReadinessV1 {
                finalized: None,
                pruning_point: h(1),
                pruning_blue: Some(0),
                wait: Some(FinalizedWaitV1::NoSafePrefix),
                withdrawn_from: None,
            },
        };
        let sweep = time(7, || certify_native_prefix_v1(policy(), &effects, n, &facts));
        let explanation = time(7, || native_safe_readiness_v1(&input));
        eprintln!(
            "[c9-b] N = {n:>9}: sweep {sweep:>10.3?}, explanation {explanation:>10.3?} ({:.2}x the sweep)",
            explanation.as_secs_f64() / sweep.as_secs_f64().max(1e-9)
        );
        assert!(
            explanation.as_secs_f64() <= 2.0 * sweep.as_secs_f64().max(1e-4),
            "N = {n}: the explanation costs more than twice the sweep"
        );
    }
}
