//! REQ tests of RFC-0017's dormant model eligibility (`palw_model_eligibility_v1`) and its gate math (§5) and §7 adversarial cases at
//! the level of the pure rules. Unverified source (2026-10-15): written, not built and not run — no result here is a PASS.
//!
//! The §7 cases are exercised against the pure functions the fold calls (stage, rights, probation grant, attempt counting,
//! commit-then-draw, verdict, renewal and lapse). What a pure test cannot show — node E2E, reorg/IBD, the coinbase allocation's
//! wiring (GAP-E1) — is RFC-0017 §10's list, not claimed here.

use super::*;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS};
use crate::network::{NetworkId, NetworkType};

/// One DAA per second (a fixture day; the fence derives a network's own from its block time).
const DAY: u64 = 86_400;
const FENCE: u64 = 10 * DAY;

fn policy() -> PalwModelEligibilityPolicyV1 {
    PalwModelEligibilityPolicyV1::design_unapproved_v1(DAY)
}

fn mirror() -> PalwModelEligibilityMirrorV1 {
    PalwModelEligibilityMirrorV1 { from_daa: FENCE, policy: policy() }
}

fn facts(registered_daa: u64, registry_admits: bool) -> PalwEligibilityClassFactsV1 {
    PalwEligibilityClassFactsV1 { genesis: false, registered_daa, registry_admits }
}

fn id(n: u8) -> EligDigestV1 {
    [n; 64]
}

fn probation_row(class: u8, lineage: u8, since: u64) -> PalwEligibilityModelRowV1 {
    palw_eligibility_new_probation_row_v1(id(class), id(lineage), id(200), FENCE + 1, since)
}

fn commit(class: u8, purpose: PalwEligibilityExamPurposeV1, claims: u32, attempt: u32) -> PalwEligibilityExamCommitV1 {
    PalwEligibilityExamCommitV1 {
        version: misaka_palw_kernel::model_eligibility_v1::ELIGIBILITY_EXAM_COMMIT_VERSION_V1,
        network: id(1),
        committer: id(2),
        class_id: id(class),
        purpose,
        claims_root: id(3),
        claims_count: claims,
        attempt_index: attempt,
    }
}

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

// =================================================================================================================================
// The fence: dormant everywhere, refused when armed, Some-only
// =================================================================================================================================

#[test]
fn req_elig_7_the_fence_is_dormant_everywhere_and_refused_when_armed() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, t12()] {
        assert_eq!(p.palw_model_eligibility_v1, None);
        assert_eq!(p.palw_model_eligibility_v1_from_daa(), None);
        assert!(!p.palw_model_eligibility_v1_active_at(u64::MAX));
        p.validate_palw_model_eligibility_v1().unwrap();
        assert!(p.palw_fences_v1().contains(&("palw_model_eligibility_v1", None)), "the exhaustive fence list names it");
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert!(bundle.state.model_eligibility_v1().is_none(), "every shipped mirror is None");
        }
    }
    let mut p = t12();
    p.palw_model_eligibility_v1 = Some(ForkActivation::never());
    p.validate_palw_model_eligibility_v1().unwrap();
    assert_eq!(p.palw_model_eligibility_v1_from_daa(), None, "never() is not a height");
    p.palw_model_eligibility_v1 = Some(ForkActivation::new(FENCE));
    p.sync_palw_model_eligibility_v1();
    assert!(p.validate_palw_model_eligibility_v1().is_err(), "refused when armed: the rule is unverified and its values unapproved");
    match &p.palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => {
            let m = bundle.state.model_eligibility_v1().expect("synced");
            assert_eq!(m.from_daa, FENCE);
            assert!(bundle.state.model_eligibility_v1_at(FENCE - 1).is_none(), "A-2: nothing below the fence");
            assert!(bundle.state.model_eligibility_v1_at(FENCE).is_some());
        }
        _ => panic!("testnet-12 is V2"),
    }
    // It restricts rewards and grants none: never an N3-gated reward profile.
    assert!(!crate::palw_beacon_bootstrap_v1::PALW_N3_GATED_REWARD_FENCES_V1.contains(&"palw_model_eligibility_v1"));
}

/// **U-2 (user decision 2026-10-15): the fence needs RFC-0018 Gate C at or below it** — so a Probation claim never advances the safe
/// frontier, settled-anchor depth or rule-E participation uncertified (its `F = 0` alone would not keep it out). The refusal names
/// U-2 before the generic "cannot be armed".
#[test]
fn u2_the_fence_needs_gate_c_at_or_below_it() {
    let mut p = t12();
    p.palw_model_eligibility_v1 = Some(ForkActivation::new(FENCE));
    p.sync_palw_model_eligibility_v1();
    let PalwModeV2Error::Invalid(why) = p.validate_palw_model_eligibility_v1().unwrap_err() else { panic!("Invalid") };
    assert!(why.contains("U-2") && why.contains("palw_independently_work_certified_v1"), "{why}");
    p.palw_independently_work_certified_v1 = Some(ForkActivation::new(FENCE));
    let PalwModeV2Error::Invalid(why) = p.validate_palw_model_eligibility_v1().unwrap_err() else { panic!("Invalid") };
    assert!(why.contains("cannot be armed"), "with Gate C at the same height only the generic refusal remains: {why}");
    p.palw_independently_work_certified_v1 = Some(ForkActivation::new(FENCE + 1));
    let PalwModeV2Error::Invalid(why) = p.validate_palw_model_eligibility_v1().unwrap_err() else { panic!("Invalid") };
    assert!(why.contains("U-2"), "Gate C above the fence leaves Probation claims below it uncovered: {why}");
}

/// **U-2: the GHOSTDAG / DAA effect of Probation REAL blocks is bounded** — whatever the number of Probation models, lineages and
/// principals, the blocks admitted under Probation in any probation epoch never exceed the network's fixed budget (20 ‰ of the
/// epoch's DAA slots at the design value) and one lineage's never exceed its stage cap (2 ‰). So Probation blocks are at most that
/// share of the blocks a DAA window holds, of the mergeset blues/reds GHOSTDAG colours and of the `W_work` step's realized-block
/// input — a bounded, not zero, effect (their WEIGHT is zero: `F = 0`, no `bounded_immature`, Gate C before any credit).
#[test]
fn u2_probation_real_blocks_have_a_bounded_dag_and_daa_effect() {
    let p = policy();
    let unit = crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1;
    let (network_budget, model_budget) = palw_probation_budgets_v1(&p);
    let epoch_slots = p.probation_epoch_daa;
    let mut network = PalwEligibilityNetworkRowV1::default();
    let mut admitted_blocks = 0u64;
    let mut per_lineage: std::collections::BTreeMap<u8, u64> = std::collections::BTreeMap::new();
    // 300 lineages of 50 principals hammer one block at a time, every DAA of one epoch.
    let mut lineages: Vec<PalwEligibilityLineageRowV1> = vec![PalwEligibilityLineageRowV1::default(); 300];
    for daa in (FENCE..FENCE + epoch_slots).step_by(100) {
        for (i, lineage) in lineages.iter_mut().enumerate() {
            if let Ok(units) = palw_probation_admit_v1(&p, lineage, &network, daa, unit) {
                let mut model = probation_row(i as u8, i as u8, FENCE);
                let mut principal = PalwEligibilityPrincipalRowV1::default();
                palw_probation_charge_v1(&p, daa, units, &mut model, lineage, &mut principal, &mut network);
                admitted_blocks += units / unit;
                *per_lineage.entry(i as u8).or_default() += units / unit;
            }
        }
    }
    assert!(admitted_blocks * unit <= network_budget, "the network's fixed budget binds however many models apply");
    assert!(per_lineage.values().all(|b| b * unit <= model_budget), "each lineage within its stage cap");
    // The DAG share: at most 20 ‰ of the epoch's DAA slots carry a Probation REAL block (one block per DAA at most).
    assert!(admitted_blocks * 1_000 <= epoch_slots * p.probation_network_permille as u64);
    assert_eq!(p.probation_network_permille, 20);
    assert_eq!(p.probation_model_permille, 2, "V3 (2026-10-15): the Probation per-model cap stays 2 ‰ (calibration)");
}

#[test]
fn the_design_policy_is_structurally_valid_and_its_guards_refuse_bad_shapes() {
    policy().validate().unwrap();
    assert!(!policy().probation_final_weight, "initial proposal: probation Finals create no consensus credit");
    let bad = |edit: fn(&mut PalwModelEligibilityPolicyV1)| {
        let mut p = policy();
        edit(&mut p);
        p.validate().is_err()
    };
    assert!(bad(|p| p.exam_k = 23), "k must be a strict majority of m (23 of 47 is not)");
    assert!(bad(|p| p.exam_k = 48), "k <= m");
    assert!(bad(|p| p.exam_anchor_delay_spans = 0), "the anchor must be strictly later than the commit");
    assert!(bad(|p| p.probation_model_permille = p.probation_network_permille + 1), "model cap within the network budget");
    assert!(bad(|p| p.attempts_network_per_epoch = 0));
    assert!(bad(|p| p.attempts_per_principal_per_epoch = p.attempts_network_per_epoch + 1));
    assert!(bad(|p| p.renewal_window_daa = p.renewal_epoch_daa + 1));
}

// =================================================================================================================================
// §5 gate math and §2 α amplification (analysis helpers)
// =================================================================================================================================

/// U-1 (user decision 2026-10-15): the default candidate is ECON's 24-of-47; 4-of-7 survives only as a TEST-ONLY constant.
#[test]
fn u1_the_default_exam_is_24_of_47() {
    let p = policy();
    assert_eq!((p.exam_m, p.exam_k), (47, 24));
    assert_eq!((PALW_ELIG_EXAM_M_TEST_ONLY_V1, PALW_ELIG_EXAM_K_TEST_ONLY_V1), (7, 4));
    // Same f, much smaller single-attempt pass-alone probability below f = 1/2 — never a proof of correct computation (U-3).
    assert!(palw_eligibility_pass_probability_f64(1.0 / 3.0, 47, 24) < palw_eligibility_pass_probability_f64(1.0 / 3.0, 7, 4));
}

#[test]
fn gate_math_4_of_7_at_three_percent_and_ten_thousand_attempts() {
    let p = palw_eligibility_pass_probability_f64(0.03, 7, 4);
    assert!((p - 2.63594e-5).abs() < 1e-9, "P_pass(f = 3 %, 4-of-7) ≈ 0.00264 % per attempt: {p}");
    let any = palw_eligibility_any_pass_f64(p, 10_000);
    assert!((any - 0.2317).abs() < 5e-4, "10,000 independent attempts ≈ 23.2 % for at least one pass: {any}");
    // Monotone: more attempts never lower it, a larger k never raises P_pass.
    assert!(palw_eligibility_any_pass_f64(p, 64) < any);
    assert!(palw_eligibility_pass_probability_f64(0.03, 7, 5) < p);
    // f = 100 % of the DRAWN stake (draws only from the attacker's own holders): certain pass — why the population is global.
    assert!((palw_eligibility_pass_probability_f64(1.0, 7, 4) - 1.0).abs() < 1e-12);
}

#[test]
fn alpha_one_point_five_amplification_of_a_concentrated_three_percent() {
    let share = |n| palw_eligibility_alpha_share_f64(0.03, 0.97, n, 1.5);
    assert!((share(1) - 0.0054096).abs() < 1e-6, "n = 1: ≈ 0.54 %");
    assert!((share(100) - 0.0515849).abs() < 1e-6, "n = 100: ≈ 5.16 %");
    assert!((share(1_000) - 0.1467565).abs() < 1e-6, "n = 1000: ≈ 14.68 %");
    // α = 1: the share equals the capital share whatever n is (the amplification is α's, not the gate's).
    assert!((palw_eligibility_alpha_share_f64(0.03, 0.97, 1_000, 1.0) - 0.03).abs() < 1e-12);
}

// =================================================================================================================================
// A. Registered / A-2
// =================================================================================================================================

#[test]
fn req_elig_7_below_the_fence_and_for_genesis_every_right_is_the_old_rule() {
    let m = mirror();
    let f = facts(FENCE + 5, false);
    assert_eq!(palw_eligibility_rights_v1(None, &f, None, FENCE + 10), PalwEligibilityRightsV1::OLD_RULE, "no mirror");
    assert_eq!(palw_eligibility_rights_v1(Some(&m), &f, None, FENCE - 1), PalwEligibilityRightsV1::OLD_RULE, "below the fence");
    let genesis = PalwEligibilityClassFactsV1 { genesis: true, ..f };
    assert_eq!(palw_eligibility_rights_v1(Some(&m), &genesis, None, FENCE + 10), PalwEligibilityRightsV1::OLD_RULE, "genesis");
    assert_eq!(palw_eligibility_status_v1(None, &f, None, FENCE).0, "not-governed");
}

#[test]
fn req_elig_1_a_new_registration_is_a_job_only_model() {
    let m = mirror();
    let fresh = facts(FENCE + 5, false);
    assert_eq!(palw_eligibility_effective_stage_v1(Some(&m), &fresh, None, FENCE + 6), Some(PalwEligibilityStageV1::Registered));
    let r = palw_eligibility_rights_v1(Some(&m), &fresh, None, FENCE + 6);
    assert_eq!(r, PalwEligibilityRightsV1::JOB_ONLY, "user-paid jobs only: no subsidy, no block right, no Final weight");
    // Its own Panel being Ready, or its own claims finalizing, is not the registry's network-drawn admission.
    assert!(!palw_eligibility_registry_admits_v1(Some(crate::palw_model_registry_v1::PalwModelLifecycleV1::Candidate)));
    assert!(!palw_eligibility_registry_admits_v1(Some(crate::palw_model_registry_v1::PalwModelLifecycleV1::Prefetching)));
    assert!(!palw_eligibility_registry_admits_v1(None));
    // The coinbase allocation input of a job-only model is zero (no allocation and no dilution of others).
    assert_eq!(palw_eligibility_allocation_capital_v1(Some(PalwEligibilityStageV1::Registered), 1 << 60), 0);
    assert_eq!(palw_eligibility_allocation_capital_v1(Some(PalwEligibilityStageV1::Lapsed), 1 << 60), 0);
    assert_eq!(palw_eligibility_allocation_capital_v1(None, 7), 7, "ungoverned: unchanged");
}

// =================================================================================================================================
// B. Probation: bounded rights, fixed network budget, history survives split / re-registration
// =================================================================================================================================

#[test]
fn req_elig_2_probation_has_bounded_subsidy_and_block_rights_and_no_final_weight() {
    let m = mirror();
    let admitted = facts(FENCE + 5, true);
    assert_eq!(palw_eligibility_effective_stage_v1(Some(&m), &admitted, None, FENCE + 6), Some(PalwEligibilityStageV1::Probation));
    let r = palw_eligibility_rights_v1(Some(&m), &admitted, None, FENCE + 6);
    assert!(r.job_completion && r.subsidy && r.block_right && r.probation_bounded);
    assert!(!r.final_weight, "job completion is separated from the Final that creates consensus credit");
}

#[test]
fn req_elig_2_the_network_probation_budget_is_fixed_whatever_the_number_of_models() {
    let p = policy();
    let (network_budget, model_cap) = palw_probation_budgets_v1(&p);
    assert!(model_cap > 0 && network_budget >= model_cap);
    for models in [1usize, 10, 100, 1_000] {
        let mut network = PalwEligibilityNetworkRowV1::default();
        let mut granted_total = 0u64;
        let daa = FENCE + 1;
        for i in 0..models {
            let mut lineage = PalwEligibilityLineageRowV1::default();
            let mut principal = PalwEligibilityPrincipalRowV1::default();
            let mut model = probation_row((i % 250) as u8, (i % 250) as u8, daa);
            // Each model asks its whole stage cap in one-block claims.
            loop {
                match palw_probation_admit_v1(&p, &lineage, &network, daa, crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1) {
                    Ok(units) => {
                        palw_probation_charge_v1(&p, daa, units, &mut model, &mut lineage, &mut principal, &mut network);
                        granted_total += units;
                    }
                    Err(_) => break,
                }
            }
        }
        assert!(granted_total <= network_budget, "{models} models: {granted_total} > the fixed network budget {network_budget}");
    }
}

#[test]
fn req_elig_2_re_registering_the_same_weights_shares_the_stage_cap_and_the_history() {
    let p = policy();
    let (_, model_cap) = palw_probation_budgets_v1(&p);
    let daa = FENCE + 1;
    let mut lineage = PalwEligibilityLineageRowV1::default(); // one artifact root, two class ids
    let mut network = PalwEligibilityNetworkRowV1::default();
    let mut principal = PalwEligibilityPrincipalRowV1::default();
    let (mut first, mut second) = (probation_row(10, 50, daa), probation_row(11, 50, daa));
    let unit = crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1;
    let mut granted = 0u64;
    for turn in 0.. {
        let model = if turn % 2 == 0 { &mut first } else { &mut second };
        match palw_probation_admit_v1(&p, &lineage, &network, daa, unit) {
            Ok(units) => {
                palw_probation_charge_v1(&p, daa, units, model, &mut lineage, &mut principal, &mut network);
                granted += units;
            }
            Err(_) => break,
        }
    }
    assert!(granted <= model_cap, "two registrations of one lineage share one stage cap ({granted} > {model_cap})");
    assert_eq!(lineage.probation_units_total, granted, "the lineage keeps the consumption history");
    // Capital split over principals changes nothing the network or the lineage counts.
    assert_eq!(principal.probation_units_total, granted);
    // The next epoch the in-epoch counters roll; the lifetime totals never do.
    let next = daa + p.probation_epoch_daa;
    assert!(palw_probation_admit_v1(&p, &lineage, &network, next, unit).is_ok());
    assert_eq!(lineage.probation_units_total, granted);
}

#[test]
fn req_elig_2_the_grant_is_the_minimum_of_the_three_caps() {
    assert_eq!(palw_probation_grant_v1(10, 3, 7, 9), 3, "the principal's common cap binds");
    assert_eq!(palw_probation_grant_v1(10, 8, 4, 9), 4, "the model's stage cap binds");
    assert_eq!(palw_probation_grant_v1(10, 8, 7, 2), 2, "the network budget binds");
    assert_eq!(palw_probation_grant_v1(1, 8, 7, 9), 1);
}

// =================================================================================================================================
// C. Promotion: attempts, commit-then-draw, verdict
// =================================================================================================================================

#[test]
fn req_elig_4_attempts_are_counted_at_commit_and_bounded_per_lineage_principal_and_network() {
    let p = policy();
    let since = FENCE + 1;
    let model = probation_row(20, 60, since);
    let now = since + p.probation_min_daa;
    let mut lineage = PalwEligibilityLineageRowV1::default();
    let mut principal = PalwEligibilityPrincipalRowV1::default();
    let mut network = PalwEligibilityNetworkRowV1::default();
    let c = commit(20, PalwEligibilityExamPurposeV1::Promotion, p.probation_min_claims, 0);
    let admit = |l: &PalwEligibilityLineageRowV1,
                 pr: &PalwEligibilityPrincipalRowV1,
                 n: &PalwEligibilityNetworkRowV1,
                 c: &PalwEligibilityExamCommitV1,
                 daa| {
        palw_eligibility_commit_admits_v1(&p, c, &model, PalwEligibilityStageV1::Probation, l, pr, n, daa)
    };
    use PalwEligibilityCommitRefusalV1 as R;
    assert_eq!(admit(&lineage, &principal, &network, &c, now - 1), Err(R::ProbationTooShort));
    assert_eq!(
        admit(&lineage, &principal, &network, &PalwEligibilityExamCommitV1 { claims_count: p.probation_min_claims - 1, ..c }, now),
        Err(R::TooFewClaims)
    );
    assert_eq!(admit(&lineage, &principal, &network, &c, now), Ok(()));
    palw_eligibility_count_attempt_v1(&p, now, &mut lineage, &mut principal, &mut network);
    // Withdrawing after seeing the draw refunds nothing: the attempt is already spent, and a replay of the same index is refused.
    assert_eq!(lineage.attempts_total, 1);
    assert_eq!(admit(&lineage, &principal, &network, &c, now + p.attempt_cooldown_daa), Err(R::AttemptIndexMismatch));
    let c1 = PalwEligibilityExamCommitV1 { attempt_index: 1, ..c };
    assert_eq!(admit(&lineage, &principal, &network, &c1, now + 1), Err(R::Cooldown), "no redraw-until-lucky inside the cooldown");
    // The principal's epoch bound.
    let mut pr = PalwEligibilityPrincipalRowV1 {
        attempt_epoch: (now + p.attempt_cooldown_daa) / p.attempt_epoch_daa,
        attempts_in_epoch: p.attempts_per_principal_per_epoch,
        ..Default::default()
    };
    assert_eq!(admit(&lineage, &pr, &network, &c1, now + p.attempt_cooldown_daa), Err(R::PrincipalAttemptsExhausted));
    pr.attempts_in_epoch = 0;
    // The network's epoch bound — Sybil principals do not raise it.
    let full = PalwEligibilityNetworkRowV1 {
        attempt_epoch: (now + p.attempt_cooldown_daa) / p.attempt_epoch_daa,
        attempts_in_epoch: p.attempts_network_per_epoch,
        ..Default::default()
    };
    assert_eq!(admit(&lineage, &pr, &full, &c1, now + p.attempt_cooldown_daa), Err(R::NetworkAttemptsExhausted));
    // The lineage's lifetime bound survives re-registration (the lineage is the weights, not the class id).
    let spent = PalwEligibilityLineageRowV1 { attempts_total: p.attempts_per_lineage_max, ..Default::default() };
    let cn = PalwEligibilityExamCommitV1 { attempt_index: p.attempts_per_lineage_max, ..c };
    assert_eq!(admit(&spent, &pr, &network, &cn, now), Err(R::LineageAttemptsExhausted));
}

#[test]
fn req_elig_3_the_draw_waits_for_an_anchor_strictly_after_the_commit() {
    let p = policy();
    assert!(!palw_eligibility_anchor_admissible_v1(&p, 40, 40), "the anchor known at commit can never draw it");
    assert!(palw_eligibility_anchor_admissible_v1(&p, 40, 41));
    // The seed binds the exam (model, claim set, attempt) and the anchor: another exam, another anchor, another seed.
    let e = commit(20, PalwEligibilityExamPurposeV1::Promotion, 16, 0).id();
    let s = palw_eligibility_exam_seed_v1(&e, 41, &id(9));
    assert_ne!(s, palw_eligibility_exam_seed_v1(&e, 42, &id(9)));
    assert_ne!(s, palw_eligibility_exam_seed_v1(&e, 41, &id(8)));
    assert_ne!(s, palw_eligibility_exam_seed_v1(&commit(20, PalwEligibilityExamPurposeV1::Promotion, 16, 1).id(), 41, &id(9)));
}

#[test]
fn req_elig_3_the_population_is_cut_at_the_commit_and_excludes_the_committer() {
    let p = policy();
    let committed = FENCE + 30 * DAY;
    let me = id(7);
    let admits = |registered: u64, operator: &EligDigestV1, active: bool| {
        palw_eligibility_population_admits_v1(&p, committed, &me, active, registered, 1_000, operator)
    };
    assert!(admits(committed - p.verifier_maturity_daa, &id(1), true));
    assert!(!admits(committed - p.verifier_maturity_daa + 1, &id(1), true), "a bond created after the cut is never drawn");
    assert!(!admits(committed - 10 * DAY, &id(1), false), "only Active bonds");
    assert!(!admits(committed - 10 * DAY, &me, true), "the committer's operator (cosmetic against Sybil keys; §5 counts it in f)");
}

#[test]
fn req_elig_3_draws_are_stake_proportional_and_sybil_neutral() {
    // 30 % of the stake as one bond, then the same 30 % split over ten bonds: the attacker's expected positions do not move.
    let honest: Vec<PalwEligibilityVerifierV1> =
        (0..70u8).map(|i| PalwEligibilityVerifierV1 { bond: id(100 + i), operator: id(100 + i), stake: 1_000 }).collect();
    let whole = PalwEligibilityVerifierV1 { bond: id(1), operator: id(1), stake: 30_000 };
    let split: Vec<PalwEligibilityVerifierV1> =
        (0..10u8).map(|i| PalwEligibilityVerifierV1 { bond: id(10 + i), operator: id(10 + i), stake: 3_000 }).collect();
    let attacker_whole = |d: &[EligDigestV1]| d.iter().filter(|b| **b == id(1)).count();
    let attacker_split = |d: &[EligDigestV1]| d.iter().filter(|b| (10u8..20).contains(&b[0])).count();
    let (mut a, mut b) = (0usize, 0usize);
    let trials = 4_000u32;
    for t in 0..trials {
        let seed = palw_eligibility_exam_seed_v1(&id(5), t as u64, &id(6));
        let mut pop1 = honest.clone();
        pop1.push(whole);
        let mut pop2 = honest.clone();
        pop2.extend(split.iter().copied());
        a += attacker_whole(&palw_eligibility_draw_v1(&seed, &id(5), &pop1, 7).unwrap());
        b += attacker_split(&palw_eligibility_draw_v1(&seed, &id(5), &pop2, 7).unwrap());
    }
    let expected = 0.3 * 7.0 * trials as f64;
    for got in [a, b] {
        assert!((got as f64 - expected).abs() < 0.05 * expected, "positions {got} vs expected {expected}");
    }
    // The caller's order never matters, and an empty or zero-stake population draws nothing.
    let seed = id(42);
    let mut rev = honest.clone();
    rev.reverse();
    assert_eq!(palw_eligibility_draw_v1(&seed, &id(5), &honest, 7), palw_eligibility_draw_v1(&seed, &id(5), &rev, 7));
    assert_eq!(palw_eligibility_draw_v1(&seed, &id(5), &[], 7), None);
}

fn drawn_exam(draws: Vec<EligDigestV1>, deadline: u64) -> PalwEligibilityExamRowV1 {
    PalwEligibilityExamRowV1 {
        commit: commit(20, PalwEligibilityExamPurposeV1::Promotion, 16, 0),
        committer_principal: id(7),
        committed_daa: FENCE + 1,
        committed_span: 40,
        anchor: Some((41, id(9))),
        examined: vec![0, 3, 5, 9],
        draws,
        drawn_daa: FENCE + 2,
        deadline_daa: deadline,
        attestations: Vec::new(),
        outcome: None,
    }
}

fn attest(exam: &mut PalwEligibilityExamRowV1, bond: EligDigestV1, verdict: PalwEligibilityVerdictV1) {
    exam.attestations.push(PalwEligibilityAttestationRowV1 { verifier: bond, verdict, transcript: id(0), attested_daa: FENCE + 3 });
}

#[test]
fn req_elig_3_k_positive_positions_pass_and_anything_else_is_not_promoted_never_slashed() {
    let p = policy();
    let draws: Vec<EligDigestV1> = (1..=7u8).map(id).collect();
    let mut exam = drawn_exam(draws.clone(), FENCE + 100);
    assert_eq!(palw_eligibility_exam_verdict_v1(&p, &exam, FENCE + 50), None, "open");
    for b in 1..=3u8 {
        attest(&mut exam, id(b), PalwEligibilityVerdictV1::Reproduced);
    }
    for b in 4..=7u8 {
        attest(&mut exam, id(b), PalwEligibilityVerdictV1::Unavailable);
    }
    assert_eq!(palw_eligibility_exam_verdict_v1(&p, &exam, FENCE + 100), None, "3 of 4 needed, deadline not passed");
    assert_eq!(
        palw_eligibility_exam_verdict_v1(&p, &exam, FENCE + 101),
        Some(PalwEligibilityExamOutcomeV1::NotPromoted),
        "honest verifiers that cannot obtain the model issue no positive attestation: not promoted — a condition, not fraud"
    );
    // A bond drawn twice holds two positions: multiplicity counts (stake-proportional draws).
    let mut twice = drawn_exam(vec![id(1), id(1), id(2), id(3), id(4), id(5), id(6)], FENCE + 100);
    attest(&mut twice, id(1), PalwEligibilityVerdictV1::Reproduced);
    attest(&mut twice, id(2), PalwEligibilityVerdictV1::Reproduced);
    attest(&mut twice, id(3), PalwEligibilityVerdictV1::Reproduced);
    assert_eq!(twice.positive_positions(), 4);
    assert_eq!(palw_eligibility_exam_verdict_v1(&p, &twice, FENCE + 10), Some(PalwEligibilityExamOutcomeV1::Passed));
}

#[test]
fn req_elig_6_a_per_claim_mismatch_never_decides_the_model_verdict() {
    // Verifiers that reproduced the model and found one Miner's claim wrong: the model is promoted on the model-level verdict; the
    // claim goes to its own court (the attestation's mismatch list is the verifier's accusation material, never counted here).
    let p = policy();
    let mut exam = drawn_exam((1..=7u8).map(id).collect(), FENCE + 100);
    for b in 1..=4u8 {
        attest(&mut exam, id(b), PalwEligibilityVerdictV1::Reproduced);
    }
    assert_eq!(palw_eligibility_exam_verdict_v1(&p, &exam, FENCE + 10), Some(PalwEligibilityExamOutcomeV1::Passed));
    let a = PalwEligibilityAttestationV1 {
        version: 1,
        network: id(1),
        exam: exam.commit.id(),
        verifier: id(1),
        verdict: PalwEligibilityVerdictV1::Reproduced,
        transcript: id(0),
        claim_mismatches: vec![3],
    };
    a.validate().unwrap();
}

// =================================================================================================================================
// D. Normal operation: renewal and lapse
// =================================================================================================================================

#[test]
fn req_elig_5_a_pass_is_never_permanent() {
    let p = policy();
    let m = mirror();
    let mut model = probation_row(20, 60, FENCE + 1);
    let passed_at = FENCE + 20 * DAY;
    palw_eligibility_apply_outcome_v1(
        &p,
        &mut model,
        PalwEligibilityExamPurposeV1::Promotion,
        PalwEligibilityExamOutcomeV1::Passed,
        passed_at,
    );
    assert_eq!(model.stage, PalwEligibilityStageV1::Eligible);
    assert_eq!(model.next_renewal_daa, passed_at + p.renewal_epoch_daa);
    let f = facts(FENCE + 1, true);
    let lapse_at = model.next_renewal_daa + p.lapse_grace_daa;
    assert_eq!(palw_eligibility_effective_stage_v1(Some(&m), &f, Some(&model), lapse_at), Some(PalwEligibilityStageV1::Eligible));
    assert_eq!(
        palw_eligibility_effective_stage_v1(Some(&m), &f, Some(&model), lapse_at + 1),
        Some(PalwEligibilityStageV1::Lapsed),
        "read as lapsed before the sweep writes it"
    );
    // A renewal passed inside the window extends from the later of now and the current renewal, by one epoch.
    let mut renewed = model.clone();
    let early = model.next_renewal_daa - p.renewal_window_daa;
    palw_eligibility_apply_outcome_v1(
        &p,
        &mut renewed,
        PalwEligibilityExamPurposeV1::Renewal,
        PalwEligibilityExamOutcomeV1::Passed,
        early,
    );
    assert_eq!(renewed.next_renewal_daa, model.next_renewal_daa + p.renewal_epoch_daa);
    // A failed renewal changes nothing now; the sweep lapses it after the grace.
    let mut failed = model.clone();
    palw_eligibility_apply_outcome_v1(
        &p,
        &mut failed,
        PalwEligibilityExamPurposeV1::Renewal,
        PalwEligibilityExamOutcomeV1::NotPromoted,
        early,
    );
    assert_eq!(failed.next_renewal_daa, model.next_renewal_daa);
    assert!(!palw_eligibility_lapse_v1(&p, &mut failed, lapse_at));
    assert!(palw_eligibility_lapse_v1(&p, &mut failed, lapse_at + 1));
    assert_eq!(palw_eligibility_rights_of_stage_v1(failed.stage, &p), PalwEligibilityRightsV1::JOB_ONLY);
}

#[test]
fn req_elig_5_a_pre_fence_class_is_grandfathered_for_one_renewal_epoch_only() {
    let p = policy();
    let m = mirror();
    let old = facts(FENCE - 1, true);
    let until = FENCE + p.renewal_epoch_daa + p.lapse_grace_daa;
    assert_eq!(palw_eligibility_effective_stage_v1(Some(&m), &old, None, until), Some(PalwEligibilityStageV1::Eligible));
    assert_eq!(palw_eligibility_effective_stage_v1(Some(&m), &old, None, until + 1), Some(PalwEligibilityStageV1::Lapsed));
    let row = palw_eligibility_grandfathered_row_v1(&m, id(30), id(31), id(32), FENCE - 1);
    assert_eq!(row.next_renewal_daa, FENCE + p.renewal_epoch_daa);
}

// =================================================================================================================================
// §7 — the mandatory 300M cases, at the level of the pure rules
// =================================================================================================================================

/// §7 (5) honest at first, then closed and forging after promotion: the renewal draws global verifiers who cannot obtain the model,
/// so no positive attestation is issued; the model lapses at `next_renewal + grace`. The outflow window is bounded by one renewal
/// epoch plus the grace (the per-bond caps of ADR-0176 bound it inside that window; this module does not edit them).
#[test]
fn s7_5_closed_after_promotion_lapses_at_the_next_renewal() {
    let p = policy();
    let mut model = probation_row(40, 70, FENCE + 1);
    let promoted = FENCE + 30 * DAY;
    palw_eligibility_apply_outcome_v1(
        &p,
        &mut model,
        PalwEligibilityExamPurposeV1::Promotion,
        PalwEligibilityExamOutcomeV1::Passed,
        promoted,
    );
    // Closed right after: every drawn honest verifier reports Unavailable.
    let mut renewal = drawn_exam((1..=7u8).map(id).collect(), model.next_renewal_daa);
    for b in 1..=7u8 {
        attest(&mut renewal, id(b), PalwEligibilityVerdictV1::Unavailable);
    }
    let outcome = palw_eligibility_exam_verdict_v1(&p, &renewal, model.next_renewal_daa + 1).unwrap();
    assert_eq!(outcome, PalwEligibilityExamOutcomeV1::NotPromoted);
    let closed_at = model.next_renewal_daa + 1;
    palw_eligibility_apply_outcome_v1(&p, &mut model, PalwEligibilityExamPurposeV1::Renewal, outcome, closed_at);
    let window_end = model.next_renewal_daa + p.lapse_grace_daa;
    assert!(palw_eligibility_lapse_v1(&p, &mut model, window_end + 1));
    assert_eq!(window_end - promoted, p.renewal_epoch_daa + p.lapse_grace_daa, "the post-promotion exposure window");
}

/// §7 (6) withdraw or re-register after seeing the draw: the attempt was counted at commit; a re-registration of the same weights
/// inherits the lineage's count; the replayed index is refused.
#[test]
fn s7_6_withdrawing_or_re_registering_after_the_draw_keeps_the_attempt_spent() {
    let p = policy();
    let mut lineage = PalwEligibilityLineageRowV1::default();
    let mut principal = PalwEligibilityPrincipalRowV1::default();
    let mut network = PalwEligibilityNetworkRowV1::default();
    palw_eligibility_count_attempt_v1(&p, FENCE + 1, &mut lineage, &mut principal, &mut network);
    // Re-registered under a new class id, same artifact root: same lineage row, so the next commit must name index 1.
    let re = probation_row(41, 71, FENCE + 1);
    let now = FENCE + 1 + p.probation_min_daa.max(p.attempt_cooldown_daa);
    let stale = commit(41, PalwEligibilityExamPurposeV1::Promotion, p.probation_min_claims, 0);
    assert_eq!(
        palw_eligibility_commit_admits_v1(&p, &stale, &re, PalwEligibilityStageV1::Probation, &lineage, &principal, &network, now),
        Err(PalwEligibilityCommitRefusalV1::AttemptIndexMismatch)
    );
}

/// §7 (2)/(3)/(4): the attacker's chance to pass ALONE is P_pass at its share of the DRAWN stake, summed over the attempts the
/// chain admits. The network attempt budget bounds the sum however the 300M is split over models, bonds and principals; a small
/// honest verifier stake raises f (3 % of supply is not 3 % of verifier stake).
#[test]
fn s7_2_3_4_union_bound_over_admitted_attempts() {
    let p = policy();
    let attempts_90d = (p.attempts_network_per_epoch as u64) * (90 * DAY).div_ceil(p.attempt_epoch_daa);
    for (f, label) in [(0.03, "3 % of drawn stake"), (0.50, "300M vs 300M honest bonded"), (0.10, "small honest verifier stake")] {
        let pp = palw_eligibility_pass_probability_f64(f, p.exam_m as u32, p.exam_k as u32);
        let bound = (pp * attempts_90d as f64).min(1.0);
        let exact = palw_eligibility_any_pass_f64(pp, attempts_90d);
        assert!(exact <= bound + 1e-12, "{label}: 1-(1-p)^A <= A·p");
    }
}

/// §7 (8) 1-day / 7-day mode switching over 30 and 90 days: no eligibility counter is keyed on the verification mode, so switching
/// modes re-acquires no probation budget and no attempts (the budgets roll on DAA epochs only).
#[test]
fn s7_8_mode_switching_re_acquires_nothing() {
    let p = policy();
    let unit = crate::palw_bond_budget_v1::PALW_BUDGET_BLOCK_UNIT_V1;
    for days in [30u64, 90] {
        let mut lineage = PalwEligibilityLineageRowV1::default();
        let mut network = PalwEligibilityNetworkRowV1::default();
        let mut principal = PalwEligibilityPrincipalRowV1::default();
        let mut model = probation_row(50, 80, FENCE + 1);
        let mut total = 0u64;
        for d in 0..days {
            // Day parity stands for the Panel=1 (1-day hold) / Panel=0 (7-day hold) mode: it is not an input to any rule here.
            let daa = FENCE + 1 + d * DAY;
            while let Ok(units) = palw_probation_admit_v1(&p, &lineage, &network, daa, unit) {
                palw_probation_charge_v1(&p, daa, units, &mut model, &mut lineage, &mut principal, &mut network);
                total += units;
            }
        }
        let (_, model_cap) = palw_probation_budgets_v1(&p);
        let epochs = (days * DAY).div_ceil(p.probation_epoch_daa);
        assert!(total <= model_cap * epochs, "{days} days: {total} > {model_cap} × {epochs}");
    }
}

/// **GAP-E3: the attempt fee** — non-zero by validation, due whole from free collateral, and the price of filling the network's
/// attempt budget is `attempts_network_per_epoch × fee` per epoch (burned). The VALUE is user decision U-4.
#[test]
fn gap_e3_an_attempt_costs_a_burned_fee_and_griefing_the_budget_has_a_price() {
    let p = policy();
    assert_eq!(p.attempt_fee_sompi, PALW_ELIG_ATTEMPT_FEE_DESIGN_V1);
    let mut free = p;
    free.attempt_fee_sompi = 0;
    assert!(free.validate().is_err(), "a free attempt makes the network attempt budget free to grief");
    assert!(palw_eligibility_fee_affordable_v1(p.attempt_fee_sompi as u128, p.attempt_fee_sompi));
    assert!(!palw_eligibility_fee_affordable_v1(p.attempt_fee_sompi as u128 - 1, p.attempt_fee_sompi));
    let per_epoch = palw_eligibility_griefing_cost_v1(&p, 1);
    assert_eq!(per_epoch, (p.attempts_network_per_epoch as u128) * (p.attempt_fee_sompi as u128));
    assert_eq!(palw_eligibility_griefing_cost_v1(&p, 3), 3 * per_epoch, "90 days of 30-day epochs");
    let (mut l, mut pr, mut n) =
        (PalwEligibilityLineageRowV1::default(), PalwEligibilityPrincipalRowV1::default(), PalwEligibilityNetworkRowV1::default());
    palw_eligibility_record_fee_v1(7, &mut l, &mut pr, &mut n);
    palw_eligibility_record_fee_v1(7, &mut l, &mut pr, &mut n);
    assert_eq!((l.fees_burned_sompi, pr.fees_burned_sompi, n.fees_burned_sompi), (14, 14, 14));
}
