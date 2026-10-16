//! **G14 — the per-class detection policy** (`palw_detection_policy_v1`, dormant; `docs/design/palw/g14-detection-policy-2026-10-17.md`).
//!
//! What is pinned here, by name:
//! * the exact hypergeometric arithmetic (against exhaustive enumeration and the external audit's N = 1000 table);
//! * the deterrence theorem (`max_D D/p(D)` is at `D = N`) and the policy's collateral bound against every `D`;
//! * k coupled to bytes, replay units, retention, court/DA openings and deadlines — refused, never silently dropped;
//! * the v2 draw: post-commit anchors, distinct across rounds, re-derivable, bound to claim/model/job/policy/checker, a new domain
//!   with its own golden vector while the legacy draw's golden vector is unchanged;
//! * the fence: dormant on every preset, Some-only, collapsed from `never()`, refused when armed; regime and admission.

use std::collections::BTreeSet;

use kaspa_consensus_core::config::params::{
    ForkActivation, MAINNET_PARAMS, Params, SIMNET_PARAMS, TESTNET_PARAMS, devnet_shipped_params, mainnet_shipped_params,
    palw_rc_shipped_params, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_detection_policy_v1::*;
use kaspa_consensus_core::palw_fp_interval_v1::palw_fp_interval_draw_v1;
use kaspa_hashes::Hash64;

fn h(v: u64) -> Hash64 {
    Hash64::from_u64_word(v)
}

fn binom(n: u64, k: u64) -> u128 {
    if k > n {
        return 0;
    }
    let k = k.min(n - k);
    let mut r: u128 = 1;
    for i in 0..k {
        r = r * u128::from(n - i) / u128::from(i + 1);
    }
    r
}

/// Exact `miss ≤ num/den` with binomials (small N only).
fn exact_miss_at_most(n: u64, d: u64, k: u64, num: u128, den: u128) -> bool {
    binom(n - d.min(n), k) * den <= num * binom(n, k)
}

fn exact_k_min(n: u64, d: u64, num: u128, den: u128) -> u32 {
    (0..=n).find(|&k| exact_miss_at_most(n, d, k, num, den)).unwrap() as u32
}

// -------------------------------------------------------------------------------------------------------------------------------
// Arithmetic
// -------------------------------------------------------------------------------------------------------------------------------

/// The fixed-point bounds bracket the exact value, and k_min equals the exhaustive minimum for every small case.
#[test]
fn k_min_is_the_exact_hypergeometric_minimum_for_every_small_case() {
    let (mut cases, mut undecided) = (0, 0u32);
    for n in 1u32..=40 {
        for d in 1u32..=n {
            for (num, den) in [(1u128, 10u128), (1, 20), (1, 100), (3, 7)] {
                let (k, exact) = palw_detection_k_min_v1(n, d, num, den);
                let e = exact_k_min(n as u64, d as u64, num, den);
                // Exact, or — only when the fixed point met an exact tie it cannot decide — one above, never below.
                assert!(k == e || (!exact && k == e + 1), "n={n} d={d} ε={num}/{den}: {k} (exact={exact}) vs {e}");
                cases += 1;
                undecided += u32::from(!exact);
            }
            for k in 0..=n {
                let (lo, hi) = palw_detection_miss_bounds_v1(n, d, k);
                let (a, b) = (binom((n - d) as u64, k as u64), binom(n as u64, k as u64));
                // lo ≤ a/b·S ≤ hi with S = 10^21
                let s: u128 = 1_000_000_000_000_000_000_000;
                assert!(lo * b <= a * s && a * s <= hi * b, "bounds bracket n={n} d={d} k={k}");
            }
        }
    }
    assert_eq!(cases, 40 * 41 / 2 * 4);
    eprintln!("k_min: {cases} cases, {undecided} met an undecidable exact tie and were raised by one");
    assert!(undecided * 50 <= cases, "ties are rare");
}

/// The external audit's illustrative N = 1000 table (`rfc14_sampling_results.json`), reproduced in integer arithmetic.
#[test]
fn the_audits_n1000_table_is_reproduced() {
    for (d, k90, k99, det4_ppm) in [(1000u32, 1u32, 1u32, 1_000_000u64), (500, 4, 7, 937_875), (100, 22, 43, 344_338), (10, 205, 368, 39_462), (1, 900, 990, 4_000)] {
        assert_eq!(palw_detection_k_min_v1(1000, d, 1, 10).0, k90, "90% at D={d}");
        assert_eq!(palw_detection_k_min_v1(1000, d, 1, 100).0, k99, "99% at D={d}");
        let got = palw_detection_ppm_floor_v1(1000, d, 4);
        assert!(got.abs_diff(det4_ppm) <= 1, "k=4 detection at D={d}: {got} vs audit {det4_ppm}");
    }
}

// -------------------------------------------------------------------------------------------------------------------------------
// Economics
// -------------------------------------------------------------------------------------------------------------------------------

/// **The deterrence theorem**: for uniform distinct sampling of K ≥ 1 intervals, `D / p(D)` is maximised at `D = N` (where
/// `p = 1`), for every N ≤ 40 and every K. So `S ≥ N·c` deters every skip with gain `D·c`, independent of K.
#[test]
fn the_proportional_bound_binds_at_d_equals_n_for_every_k() {
    for n in 1u64..=40 {
        for k in 1..=n {
            let all = binom(n, k);
            for d in 1..=n {
                // p(D) = (C(N,K) − C(N−D,K)) / C(N,K);  D/p(D) ≤ N  ⇔  D·C(N,K) ≤ N·(C(N,K) − C(N−D,K)).
                let caught = all - binom(n - d, k);
                assert!(u128::from(d) * all <= u128::from(n) * caught, "n={n} k={k} d={d}");
                // And concavity: increments non-increasing.
                if d + 1 <= n {
                    let next = all - binom(n - d - 1, k);
                    let prev = all - binom(n - d + 1, k);
                    assert!(next - caught <= caught - prev, "concave n={n} k={k} d={d}");
                }
            }
        }
    }
}

fn base_policy() -> PalwDetectionPolicyV1 {
    palw_detection_genesis_design_v1(h(0xC1A55), PalwDetectionRouteV1::Base0A16, PalwIntervalCountRuleV1::HeldPositions { width: 4 })
}

fn facts_for(policy: &PalwDetectionPolicyV1, n: u32, collateral: u128) -> PalwDetectionClaimFactsV1 {
    let mut f = policy.synthetic_facts_v1(n).expect("the rule reaches n");
    f.collateral_sompi = collateral;
    f
}

/// At the policy's required collateral, the forger's expected advantage `D·c − p(D)·S` is ≤ 0 for EVERY D (exact rationals), and
/// one sompi less is refused at admission.
#[test]
fn at_the_required_collateral_no_skip_pays_and_one_sompi_less_is_refused() {
    let p = base_policy();
    for n in [1u32, 2, 7, 16, 64, 100, 257] {
        let plan = p.plan_v1(&facts_for(&p, n, u128::MAX)).expect("plannable");
        let s = plan.collateral_required_sompi;
        let c = u128::from(p.economics.cost_per_interval_sompi);
        let k = u64::from(plan.k_total);
        if n <= 40 {
            let all = binom(u64::from(n), k);
            for d in 1..=u64::from(n) {
                let caught = all - binom(u64::from(n) - d, k);
                // D·c ≤ p(D)·S  ⇔  D·c·C(N,K) ≤ S·caught
                assert!(u128::from(d) * c * all <= s * caught, "n={n} d={d}: S={s} does not deter");
            }
        }
        assert!(s >= u128::from(n) * c, "S covers the whole honest cost");
        assert!(plan.d1_proportional_sompi <= s, "the D = 1 figure never binds above D = N");
        let short = p.plan_v1(&facts_for(&p, n, s - 1));
        assert!(matches!(short, Err(PalwDetectionRefusalV1::CollateralBelowDeterrence { .. })), "n={n}: {short:?}");
    }
}

/// A declared fixed gain (a chosen output) is the D = 1 case: `S ≥ margin · G · N / K_tot`, which grows with N/K.
#[test]
fn a_fixed_gain_cover_costs_n_over_k_times_the_gain() {
    let mut p = base_policy();
    p.economics.fixed_gain_sompi = Some(1_000_000);
    let plan = p.plan_v1(&facts_for(&p, 4_096, u128::MAX)).unwrap();
    let want = (1_000_000u128 * 1_100).div_ceil(1_000) * 4_096;
    let want = want.div_ceil(u128::from(plan.k_total));
    assert_eq!(plan.collateral_required_sompi, want);
    assert!(plan.collateral_required_sompi > 4_096 * 1_000, "the fixed-gain bound dominates the proportional one here");
}

/// Coverage below 1 raises S (by 1/q) and can make the target unreachable; the shadow rate adds coverage to the deterrence bound.
#[test]
fn coverage_and_shadow_enter_the_bound() {
    let mut p = base_policy();
    let base = p.plan_v1(&facts_for(&p, 64, u128::MAX)).unwrap().collateral_required_sompi;
    p.economics.check_coverage_ppm = 500_000;
    p.shadow_rate_ppm = 0;
    assert!(matches!(p.plan_v1(&facts_for(&p, 64, u128::MAX)), Err(PalwDetectionRefusalV1::CoverageBelowTarget { .. })));
    p.epsilon = PalwRatioV1::new(6, 10);
    let half = p.plan_v1(&facts_for(&p, 64, u128::MAX)).unwrap();
    assert!(half.collateral_required_sompi >= 2 * u128::from(64u32) * 1_000);
    p.shadow_rate_ppm = 500_000;
    assert_eq!(p.effective_coverage_ppm_v1(), 750_000);
    let shadowed = p.plan_v1(&facts_for(&p, 64, u128::MAX)).unwrap();
    assert!(shadowed.collateral_required_sompi < half.collateral_required_sompi);
    assert!(base < half.collateral_required_sompi);
}

// -------------------------------------------------------------------------------------------------------------------------------
// N, the price and the resources
// -------------------------------------------------------------------------------------------------------------------------------

#[test]
fn n_comes_from_chain_counts_that_reprice_to_the_claim() {
    let p = palw_detection_genesis_design_v1(h(1), PalwDetectionRouteV1::Kernel, PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval: 100 });
    let mut f = facts_for(&p, 50, u128::MAX);
    assert_eq!(p.plan_v1(&f).unwrap().n, 50);
    f.context_priced_leaves = Some(f.work_leaves - 1);
    assert!(matches!(p.plan_v1(&f), Err(PalwDetectionRefusalV1::NotAtTheClaimsPrice { .. })), "a re-priced context is not this claim's");
    f.context_priced_leaves = None;
    assert!(matches!(p.plan_v1(&f), Err(PalwDetectionRefusalV1::NotAtTheClaimsPrice { .. })));
    // Held and call geometries are base0's chain arithmetic.
    let held = PalwIntervalCountRuleV1::HeldPositions { width: 4 };
    let calls = PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval: 1 };
    let f = PalwDetectionClaimFactsV1 { work_leaves: 0, context_priced_leaves: None, prompt_tokens: 40, decode_tokens_executed: 217, collateral_sompi: 0 };
    assert_eq!(held.interval_count_v1(&f), Some(64));
    assert_eq!(calls.interval_count_v1(&f), Some(216));
    let mut big = base_policy();
    big.n_max = 63;
    assert!(matches!(big.plan_v1(&PalwDetectionClaimFactsV1 { collateral_sompi: u128::MAX, ..f }), Err(PalwDetectionRefusalV1::IntervalCountAboveCeiling { n: 64, n_max: 63 })));
}

/// **k grows → every coupled budget must grow, or the class is refused.** The legacy pool's 512 pairs cannot carry k ≈ 44 for 32
/// claims in flight; the court must be able to compel every drawn interval; bytes, replay and the clock likewise.
#[test]
fn every_resource_is_coupled_to_k_and_refuses_rather_than_drops() {
    let p = base_policy();
    let n = 32_768;
    let plan = p.plan_v1(&facts_for(&p, n, u128::MAX)).unwrap();
    assert!(plan.k_round > 4, "the 99%/10% target needs more than the legacy four: {}", plan.k_round);
    assert_eq!(plan.retention_pairs, u64::from(plan.k_round) * 32);

    let mut q = p;
    q.resources.retention_pairs_cap = 512;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::RetentionBelowDraw { .. })), "512 pairs refuse k·32");
    let mut q = p;
    q.resources.court_openings_per_claim = plan.k_round - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::CourtBudgetBelowDraw { .. })));
    let mut q = p;
    q.resources.da_bytes_per_claim = u64::from(plan.k_round) * q.resources.opening_bytes - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::CourtBudgetBelowDraw { .. })));
    let mut q = p;
    q.resources.max_claim_fetch_bytes = u64::from(plan.k_total) * q.resources.opening_bytes - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::FetchBytesOverBudget { .. })));
    let mut q = p;
    q.resources.max_claim_replay_units = u64::from(plan.k_total) - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::ReplayUnitsOverBudget { .. })));
    let mut q = p;
    q.deadlines.inspection_deadline_daa = plan.round_daa - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::DeadlineUnreachable { .. })));
    let mut q = p;
    q.k_max = plan.k_round - 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::DetectionTargetUnreachable { .. })));
    // Rounds split k and must each fit the spacing.
    let mut q = p;
    q.deadlines.rounds = 4;
    q.deadlines.round_spacing_daa = 1;
    assert!(matches!(q.plan_v1(&facts_for(&q, n, u128::MAX)), Err(PalwDetectionRefusalV1::DeadlineUnreachable { .. })));
    q.deadlines.round_spacing_daa = 100;
    let split = q.plan_v1(&facts_for(&q, n, u128::MAX)).unwrap();
    assert_eq!(split.k_round, plan.k_needed.div_ceil(4));
    assert!(split.k_total >= plan.k_needed);
}

/// The genesis design templates are armable at their own extremes; the target holds at every probe N.
#[test]
fn the_genesis_design_templates_are_armable_and_meet_the_target() {
    for (route, rule) in [
        (PalwDetectionRouteV1::Base0A16, PalwIntervalCountRuleV1::HeldPositions { width: 4 }),
        (PalwDetectionRouteV1::Base0A16, PalwIntervalCountRuleV1::DecodeCalls { checkpoint_interval: 1 }),
        (PalwDetectionRouteV1::PalwQ361, PalwIntervalCountRuleV1::HeldPositions { width: 16 }),
        (PalwDetectionRouteV1::TirV1, PalwIntervalCountRuleV1::PricedLeaves { leaves_per_interval: 1 }),
    ] {
        let p = palw_detection_genesis_design_v1(h(7), route, rule);
        p.validate_armable_v1().unwrap_or_else(|e| panic!("{route:?} {rule:?}: {e}"));
        let mut worst = 0u32;
        for n in [1u32, 2, 3, 10, 64, 100, 1_000, 4_096, 32_768] {
            let plan = p.plan_v1(&facts_for(&p, n, u128::MAX)).unwrap();
            assert!(plan.miss_at_d_min_ppm_ceil <= 10_000, "{route:?} n={n}: miss {} ppm", plan.miss_at_d_min_ppm_ceil);
            worst = worst.max(plan.k_round);
        }
        eprintln!("{route:?} {rule:?}: largest k per claim over the probes = {worst}");
    }
}

// -------------------------------------------------------------------------------------------------------------------------------
// The draw
// -------------------------------------------------------------------------------------------------------------------------------

fn binding(seat: u8) -> PalwDetectionDrawBindingV2 {
    PalwDetectionDrawBindingV2 {
        network_domain: h(0x11),
        claim_id: h(0x33),
        class_id: h(0x44),
        job_id: h(0x55),
        policy_id: h(0x66),
        checker: PalwDetectionCheckerV1::Seat(seat),
    }
}

/// **The legacy draw did not move**, and the new domain has its own frozen vector.
#[test]
fn the_legacy_golden_vector_is_unchanged_and_v2_has_its_own() {
    assert_eq!(palw_fp_interval_draw_v1(&h(0x11), &h(0x22), &h(0x33), 2, 4, 1_000), vec![645, 20, 372, 227]);
    let v2 = palw_detection_draw_round_v2(&binding(2), 0, &h(0x22), 4, 1_000, &BTreeSet::new()).unwrap();
    assert_eq!(v2, V2_GOLDEN.to_vec(), "re-take deliberately, with a new domain suffix");
    assert_ne!(v2, vec![645, 20, 372, 227]);
}
const V2_GOLDEN: [u32; 4] = [211, 748, 819, 858];

#[test]
fn every_binding_input_moves_the_draw_and_it_is_pure() {
    let base = palw_detection_draw_round_v2(&binding(0), 0, &h(2), 8, 100_000, &BTreeSet::new()).unwrap();
    assert_eq!(base, palw_detection_draw_round_v2(&binding(0), 0, &h(2), 8, 100_000, &BTreeSet::new()).unwrap(), "pure");
    let moved = |b: PalwDetectionDrawBindingV2, round: u32, anchor: Hash64| {
        palw_detection_draw_round_v2(&b, round, &anchor, 8, 100_000, &BTreeSet::new()).unwrap()
    };
    assert_ne!(base, moved(binding(0), 0, h(9)), "anchor");
    assert_ne!(base, moved(binding(0), 1, h(2)), "round");
    assert_ne!(base, moved(binding(1), 0, h(2)), "seat");
    for f in 0..5 {
        let mut b = binding(0);
        match f {
            0 => b.network_domain = h(99),
            1 => b.claim_id = h(99),
            2 => b.class_id = h(99),
            3 => b.job_id = h(99),
            _ => b.policy_id = h(99),
        }
        assert_ne!(base, moved(b, 0, h(2)), "field {f}");
    }
    let mut b = binding(0);
    b.checker = PalwDetectionCheckerV1::PublicBond(h(5));
    assert_ne!(base, moved(b, 0, h(2)), "public verifier");
}

/// **Rounds never re-check an interval**; the union is exactly Σ k; restart and reorg re-derive from anchors alone.
#[test]
fn rounds_are_distinct_rederivable_and_never_double_count() {
    let anchors: Vec<PalwDetectionRoundAnchorV1> = (0..5).map(|r| PalwDetectionRoundAnchorV1 { block: h(100 + r), daa: 10 + r * 7 }).collect();
    let (rounds, union) = palw_detection_draw_rounds_v2(&binding(3), 9, &anchors, 6, 40).unwrap();
    assert_eq!(rounds.len(), 5);
    assert_eq!(union.len(), 30, "5 × 6 distinct");
    let mut seen = BTreeSet::new();
    for r in &rounds {
        assert_eq!(r.len(), 6);
        for i in r {
            assert!(*i < 40 && seen.insert(*i), "interval {i} drawn twice");
        }
    }
    // Restart: the same anchors give the same sets (nothing persisted).
    assert_eq!(palw_detection_draw_rounds_v2(&binding(3), 9, &anchors, 6, 40).unwrap(), (rounds.clone(), union.clone()));
    // Reorg of round 2's anchor: rounds 0 and 1 are unchanged, round 2 onward re-derive, still distinct.
    let mut reorged = anchors.clone();
    reorged[2].block = h(777);
    let (r2, u2) = palw_detection_draw_rounds_v2(&binding(3), 9, &reorged, 6, 40).unwrap();
    assert_eq!(&r2[..2], &rounds[..2]);
    assert_ne!(r2[2], rounds[2]);
    assert_eq!(u2.len(), 30);
    // Exhaustion: 8 rounds of 6 over 40 intervals → the last takes what is left, then nothing.
    let many: Vec<_> = (0..8).map(|r| PalwDetectionRoundAnchorV1 { block: h(200 + r), daa: 10 + r }).collect();
    let (r3, u3) = palw_detection_draw_rounds_v2(&binding(3), 9, &many, 6, 40).unwrap();
    assert_eq!(u3.len(), 40);
    assert_eq!(r3[6].len(), 4);
    assert!(r3[7].is_empty());
    // Post-commit: an anchor at or before the claim's acceptance, or not after the previous round's, is refused.
    assert!(matches!(palw_detection_draw_rounds_v2(&binding(3), 10, &anchors, 6, 40), Err(PalwDetectionDrawRefusalV2::AnchorNotPostCommit { round: 0, .. })));
    let mut stale = anchors.clone();
    stale[3].daa = stale[2].daa;
    assert!(matches!(palw_detection_draw_rounds_v2(&binding(3), 9, &stale, 6, 40), Err(PalwDetectionDrawRefusalV2::AnchorNotPostCommit { round: 3, .. })));
}

/// Over many anchors every interval is drawn at the uniform rate k/N (chi-square-free check: each count within 5σ).
#[test]
fn the_v2_draw_is_uniform_over_intervals() {
    let (n, k, trials) = (50u32, 5u32, 20_000u64);
    let mut counts = vec![0u64; n as usize];
    for t in 0..trials {
        for i in palw_detection_draw_round_v2(&binding(0), 0, &h(1_000_000 + t), k, n, &BTreeSet::new()).unwrap() {
            counts[i as usize] += 1;
        }
    }
    let mean = trials as f64 * f64::from(k) / f64::from(n);
    let sd = (mean * (1.0 - f64::from(k) / f64::from(n))).sqrt();
    for (i, c) in counts.iter().enumerate() {
        assert!((*c as f64 - mean).abs() <= 5.0 * sd, "interval {i}: {c} vs {mean}");
    }
}

#[test]
fn the_shadow_rate_selects_its_share() {
    let p = base_policy();
    let shadowed = (0..100_000u64).filter(|c| palw_detection_is_shadowed_v2(&p, &h(1), &h(2), &h(10_000 + c))).count();
    // 1% ± 5σ (σ ≈ 31.5).
    assert!((842..=1_158).contains(&shadowed), "{shadowed}");
    let mut off = p;
    off.shadow_rate_ppm = 0;
    assert!(!palw_detection_is_shadowed_v2(&off, &h(1), &h(2), &h(3)));
}

// -------------------------------------------------------------------------------------------------------------------------------
// The fence
// -------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_fence_is_dormant_on_every_preset() {
    for (name, p) in [
        ("mainnet const", MAINNET_PARAMS),
        ("testnet const", TESTNET_PARAMS),
        ("simnet const", SIMNET_PARAMS),
        ("mainnet shipped", mainnet_shipped_params()),
        ("t12 shipped", palw_t12_shipped_params()),
        ("rc shipped", palw_rc_shipped_params()),
        ("devnet shipped", devnet_shipped_params()),
    ] {
        assert!(p.palw_detection_policy_v1.is_none(), "{name}");
        assert!(p.validate_palw_detection_policy_v1().is_ok(), "{name}");
    }
}

/// `Some(never())` hashes as `None`; an armed height moves the params id (Some-only) and is refused.
#[test]
fn the_fence_is_some_only_collapses_from_never_and_is_refused_when_armed() {
    let base = palw_t12_shipped_params();
    let mut never = base.clone();
    never.palw_detection_policy_v1 = Some(PalwDetectionPolicyFenceV1::at(ForkActivation::never()));
    assert_eq!(never.consensus_params_id(), base.consensus_params_id());
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id());
    // The schedule id is a report, not a gate: like every `if let Some` fence (ADR-0179's, RFC-0018's) it visits `Some(never())`
    // as `u64::MAX` and skips `None`, so an unset preset's schedule id does not move (`palw_the_release_did_not_move`).
    assert!(never.validate_palw_detection_policy_v1().is_ok());

    let mut armed: Params = base.clone();
    armed.palw_detection_policy_v1 = Some(PalwDetectionPolicyFenceV1 { activation: ForkActivation::new(16_000), policies: vec![base_policy()] });
    assert_ne!(armed.consensus_params_id(), base.consensus_params_id(), "Some-only: the armed value is in the ruleset");
    let refusal = armed.validate_palw_detection_policy_v1().unwrap_err().to_string();
    assert!(refusal.contains("cannot be armed"), "{refusal}");
    // Two different policy values are two different ids.
    let mut other = armed.clone();
    other.palw_detection_policy_v1.as_mut().unwrap().policies[0].epsilon = PalwRatioV1::new(1, 20);
    assert_ne!(other.consensus_params_id(), armed.consensus_params_id());
    // A malformed or un-armable value is named before the blanket refusal.
    let mut bad = armed.clone();
    bad.palw_detection_policy_v1.as_mut().unwrap().policies[0].economics.collateral_per_interval_sompi = 1;
    let named = bad.validate_palw_detection_policy_v1().unwrap_err().to_string();
    assert!(named.contains("DETECTION_COLLATERAL_BELOW_DETERRENCE_BOUND"), "{named}");
    let mut dup = armed.clone();
    let p0 = dup.palw_detection_policy_v1.as_ref().unwrap().policies[0];
    dup.palw_detection_policy_v1.as_mut().unwrap().policies.push(p0);
    assert!(dup.validate_palw_detection_policy_v1().unwrap_err().to_string().contains("two policies"));
}

#[test]
fn the_regime_and_admission_follow_the_fence() {
    let p = base_policy();
    let fence = PalwDetectionPolicyFenceV1 { activation: ForkActivation::new(16_000), policies: vec![p] };
    assert_eq!(palw_detection_regime_v1(None, &p.class_id, 0, 20_000), PalwDetectionRegimeV1::LegacyV1);
    assert_eq!(palw_detection_regime_v1(Some(&fence), &p.class_id, 0, 15_999), PalwDetectionRegimeV1::LegacyV1);
    assert_eq!(palw_detection_regime_v1(Some(&fence), &p.class_id, 0, 16_000), PalwDetectionRegimeV1::Policy(&fence.policies[0]));
    assert_eq!(palw_detection_regime_v1(Some(&fence), &p.class_id, 1, 16_000), PalwDetectionRegimeV1::NoPolicy, "kernel version bound");
    assert_eq!(palw_detection_regime_v1(Some(&fence), &h(9), 0, 16_000), PalwDetectionRegimeV1::NoPolicy);
    let never = PalwDetectionPolicyFenceV1 { activation: ForkActivation::never(), policies: vec![p] };
    assert_eq!(palw_detection_regime_v1(Some(&never), &p.class_id, 0, u64::MAX - 1), PalwDetectionRegimeV1::LegacyV1);
    let f = facts_for(&p, 64, u128::MAX);
    assert_eq!(palw_detection_admit_claim_v1(PalwDetectionRegimeV1::LegacyV1, &f), Ok(None));
    assert_eq!(palw_detection_admit_claim_v1(PalwDetectionRegimeV1::NoPolicy, &f), Err(PalwDetectionRefusalV1::NoPolicyForClass));
    assert!(palw_detection_admit_claim_v1(PalwDetectionRegimeV1::Policy(&p), &f).unwrap().is_some());
}

// -------------------------------------------------------------------------------------------------------------------------------
// The funnel
// -------------------------------------------------------------------------------------------------------------------------------

#[test]
fn the_funnel_keeps_losses_apart_and_its_order() {
    use PalwDetectionStageV1::*;
    let mut f = PalwDetectionFunnelV1::default();
    for _ in 0..10 {
        f.record(Eligible);
    }
    for _ in 0..8 {
        f.record(Checked);
    }
    f.record_loss(PalwDetectionLossV1::Unassigned);
    f.record_loss(PalwDetectionLossV1::CapacityRefusal);
    for s in [Detected, Localized, Filed, Accepted, Convicted, Collected, Invalidated] {
        f.record(s);
    }
    assert!(f.is_consistent());
    assert_eq!(f.effective_rate_ppm(), 100_000);
    assert_eq!(f.loss(PalwDetectionLossV1::Unassigned), 1);
    let mut g = f;
    g.record(Convicted);
    assert!(!g.is_consistent(), "more convictions than accepted filings is a recorder bug");
    let mut m = PalwDetectionFunnelV1::default();
    m.merge(&f);
    m.merge(&f);
    assert_eq!(m.stage(Eligible), 20);
    assert_eq!(PALW_DETECTION_STAGE_NAMES_V1.len(), 10);
    assert_eq!(PALW_DETECTION_LOSS_NAMES_V1.len(), 7);
}
