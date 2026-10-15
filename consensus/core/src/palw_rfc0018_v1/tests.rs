//! RFC-0018 unit tests over the pure rules (fences, Gate C, rule E participation v2, the canonical sibling order, the permissionless
//! anchor seed, the DAG width estimate). **Unverified source (2026-10-15): written, not built and not run — no result here is a
//! PASS.** What a pure test cannot show (three concurrent models on real nodes, partition/IBD convergence, a DNS-retired calibration
//! devnet) is RFC-0018 §10's real-process list (`scripts/drills/rfc0018_*.py`).

use super::*;
use crate::config::params::{MAINNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params};
use crate::palw_state_v2::{PalwClaimPhaseV2, PalwVoidReasonV2, palw_claim_template_v1};
use crate::tx::TransactionOutpoint;

const FENCE: u64 = 50_000;

fn bond(i: u32) -> PalwBondKeyV2 {
    PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0x18), i))
}

fn class(i: u64) -> Hash64 {
    Hash64::from_u64_word(0xC1A5_0000 + i)
}

fn t12_state_params() -> PalwStateParamsV2 {
    match palw_t12_shipped_params().palw_consensus_mode {
        PalwConsensusMode::ConsensusV2(bundle) => bundle.state,
        _ => panic!("testnet-12 is a ConsensusV2 network"),
    }
}

fn with_mirror(rule: PalwGateCRuleV1) -> PalwStateParamsV2 {
    t12_state_params().with_rfc0018_v1(Some(PalwRfc0018MirrorV1 {
        sibling_attribution_from_daa: Some(FENCE),
        panel_anchor_permissionless_from_daa: Some(FENCE),
        rule_e_participation_v2_from_daa: Some(FENCE),
        gate_c_from_daa: Some(FENCE),
        gate_c_rule: rule,
    }))
}

fn claim_at(daa: u64, phase: PalwClaimPhaseV2) -> PalwClaimStateV2 {
    let mut c = palw_claim_template_v1(class(1), bond(1), daa, 0, 1_000);
    c.pwu = 900;
    c.immature_contribution = 300;
    c.phase = phase;
    c
}

const NAMES: [&str; 5] = [
    "palw_dag_sibling_attribution_v1",
    "palw_panel_anchor_permissionless_v1",
    "palw_rule_e_participation_v2",
    "palw_independently_work_certified_v1",
    "palw_dag_width_schedule_v1",
];

// =================================================================================================================================
// Fences: dormant everywhere, Some-only, refused when armed
// =================================================================================================================================

#[test]
fn rfc18_fences_are_dormant_on_every_preset_and_named_in_the_exhaustive_list() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params()] {
        assert_eq!(p.palw_dag_sibling_attribution_v1, None);
        assert_eq!(p.palw_panel_anchor_permissionless_v1, None);
        assert_eq!(p.palw_rule_e_participation_v2, None);
        assert_eq!(p.palw_independently_work_certified_v1, None);
        assert_eq!(p.palw_dag_width_schedule_v1, None);
        p.validate_palw_rfc0018_v1().unwrap();
        assert_eq!(p.palw_rfc0018_expected_mirror_v1(), None);
        let fences = p.palw_fences_v1();
        for name in NAMES {
            assert!(fences.contains(&(name, None)), "the exhaustive fence list names {name}");
        }
        if let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode {
            assert!(bundle.state.rfc0018_v1().is_none(), "every shipped mirror is None");
        }
    }
}

#[test]
fn rfc18_never_collapses_and_fingerprints_are_some_only() {
    let base = palw_t12_shipped_params();
    let (params_id, schedule_id) = (base.consensus_params_id(), base.consensus_schedule_id());
    let mut never = base.clone();
    never.palw_dag_sibling_attribution_v1 = Some(ForkActivation::never());
    never.palw_panel_anchor_permissionless_v1 = Some(ForkActivation::never());
    never.palw_rule_e_participation_v2 = Some(ForkActivation::never());
    never.palw_independently_work_certified_v1 = Some(ForkActivation::never());
    never.palw_dag_width_schedule_v1 = Some(PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::never()));
    never.validate_palw_rfc0018_v1().unwrap();
    assert_eq!(never.consensus_params_id(), params_id, "Some(never()) hashes as None");
    assert_eq!(never.consensus_schedule_id(), schedule_id);
    for (i, name) in NAMES.iter().enumerate() {
        let mut armed = base.clone();
        let at = ForkActivation::new(FENCE + i as u64);
        match i {
            0 => armed.palw_dag_sibling_attribution_v1 = Some(at),
            1 => armed.palw_panel_anchor_permissionless_v1 = Some(at),
            2 => {
                armed.palw_fork_choice_rule_e_v1 = Some(ForkActivation::new(FENCE));
                armed.palw_rule_e_participation_v2 = Some(at)
            }
            3 => armed.palw_independently_work_certified_v1 = Some(at),
            _ => armed.palw_dag_width_schedule_v1 = Some(PalwDagWidthScheduleFenceV1::unapproved_probe_at(at)),
        }
        assert_ne!(armed.consensus_params_id(), params_id, "{name} armed is a different ruleset");
        armed.sync_palw_rfc0018_v1();
        assert!(armed.validate_palw_rfc0018_v1().is_err(), "{name} is refused when armed");
    }
}

#[test]
fn rfc18_rule_e_v2_needs_rule_e_at_or_below_it_and_the_mirror_must_agree() {
    let mut p = palw_t12_shipped_params();
    p.palw_rule_e_participation_v2 = Some(ForkActivation::new(FENCE));
    p.sync_palw_rfc0018_v1();
    assert!(matches!(p.validate_palw_rfc0018_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("needs palw_fork_choice_rule_e_v1")));
    let mut q = palw_t12_shipped_params();
    q.palw_independently_work_certified_v1 = Some(ForkActivation::new(FENCE));
    // Not synced: the mirror disagrees.
    assert!(matches!(q.validate_palw_rfc0018_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("mirror")));
}

// =================================================================================================================================
// Gate C
// =================================================================================================================================

#[test]
fn rfc18_gate_c_is_the_identity_below_its_fence_and_on_every_shipped_network() {
    let shipped = t12_state_params();
    for phase in [
        PalwClaimPhaseV2::Provisional,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 10 },
        PalwClaimPhaseV2::Final { final_daa: 20 },
    ] {
        let c = claim_at(FENCE + 1, phase.clone());
        assert!(palw_independently_work_certified_v1(&c, &PalwGateCViewV1::of(&shipped)), "no mirror: legacy");
        assert_eq!(palw_gate_c_weight_v1(&shipped, &c, 77), 77);
        assert_eq!(palw_gate_c_reward_v1(&shipped, &c, 55), 55);
        assert_eq!(palw_claim_provisional_weight_v1(&shipped, &c), 300);
        // A claim accepted BELOW the fence keeps its old rule forever (A-2), whatever the rule past it.
        let armed = with_mirror(PalwGateCRuleV1::Undecided);
        let old = claim_at(FENCE - 1, phase);
        assert!(palw_independently_work_certified_v1(&old, &PalwGateCViewV1::of(&armed)));
        assert_eq!(palw_claim_provisional_weight_v1(&armed, &old), 300);
    }
}

#[test]
fn rfc18_gate_c_undecided_is_fail_closed_and_the_candidate_reads_the_licence() {
    let undecided = with_mirror(PalwGateCRuleV1::Undecided);
    let candidate = with_mirror(PalwGateCRuleV1::CandidateLicensedOrFinalV1);
    let provisional = claim_at(FENCE, PalwClaimPhaseV2::Provisional);
    let licensed = claim_at(FENCE, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: FENCE + 30 });
    let fin = claim_at(FENCE, PalwClaimPhaseV2::Final { final_daa: FENCE + 60 });
    let voided = claim_at(FENCE, PalwClaimPhaseV2::Voided { voided_daa: FENCE + 5, reason: PalwVoidReasonV2::BindTimeout });
    for c in [&provisional, &licensed, &fin, &voided] {
        assert!(!palw_independently_work_certified_v1(c, &PalwGateCViewV1::of(&undecided)), "nothing counts until defined");
        assert_eq!(palw_gate_c_weight_v1(&undecided, c, 9), 0);
    }
    assert!(!palw_independently_work_certified_v1(&provisional, &PalwGateCViewV1::of(&candidate)));
    assert!(palw_independently_work_certified_v1(&licensed, &PalwGateCViewV1::of(&candidate)));
    assert!(palw_independently_work_certified_v1(&fin, &PalwGateCViewV1::of(&candidate)));
    assert!(!palw_independently_work_certified_v1(&voided, &PalwGateCViewV1::of(&candidate)), "a void is never certified");
    // The licence site asks as licensed; the others ask the record they hold.
    assert!(palw_gate_c_settles_anchor_v1(&candidate, &provisional, true));
    assert!(!palw_gate_c_settles_anchor_v1(&candidate, &provisional, false));
    assert!(!palw_gate_c_settles_anchor_v1(&undecided, &provisional, true));
}

/// The provisional site must give ONE number over a claim's life, or `bounded_immature` would add 0 and later subtract the weight.
#[test]
fn rfc18_gate_c_provisional_weight_is_stable_over_the_claims_life() {
    for rule in [PalwGateCRuleV1::Undecided, PalwGateCRuleV1::CandidateLicensedOrFinalV1] {
        let p = with_mirror(rule);
        let phases = [
            PalwClaimPhaseV2::Provisional,
            PalwClaimPhaseV2::PanelBound { bound_daa: FENCE + 3 },
            PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: FENCE + 30 },
            PalwClaimPhaseV2::Final { final_daa: FENCE + 60 },
            PalwClaimPhaseV2::Voided { voided_daa: FENCE + 9, reason: PalwVoidReasonV2::BindTimeout },
        ];
        let answers: Vec<u128> = phases.iter().map(|ph| palw_claim_provisional_weight_v1(&p, &claim_at(FENCE, ph.clone()))).collect();
        assert!(answers.windows(2).all(|w| w[0] == w[1]), "{rule:?}: one answer for every phase, got {answers:?}");
    }
}

// =================================================================================================================================
// Rule E participation v2
// =================================================================================================================================

#[test]
fn rfc18_participation_v2_is_capital_split_neutral_and_once_per_bond() {
    const C: u64 = 1_300_000 * 100_000_000;
    // One bond of capital C with three attempts.
    let mut whole = PalwRuleEParticipationV2Acc::default();
    for _ in 0..3 {
        whole.push(bond(1), true, true, Some(C));
    }
    assert_eq!((whole.capital(), whole.bonds()), (C as u128, 1), "a bond counts once whatever its claim count");
    // The same capital split over ten bonds, each attempting once: the same credit, never more.
    let mut split = PalwRuleEParticipationV2Acc::default();
    for i in 0..10 {
        split.push(bond(100 + i), true, true, Some(C / 10));
    }
    assert_eq!(split.capital(), C as u128, "splitting buys nothing");
    // Split, but only half the pieces attempt: less, never more.
    let mut half = PalwRuleEParticipationV2Acc::default();
    for i in 0..5 {
        half.push(bond(200 + i), true, true, Some(C / 10));
    }
    assert!(half.capital() < whole.capital());
}

#[test]
fn rfc18_participation_v2_excludes_voided_uncertified_non_attempt_and_non_common() {
    let mut acc = PalwRuleEParticipationV2Acc::default();
    acc.push(bond(1), true, false, Some(10)); // voided or uncertified
    acc.push(bond(2), false, true, Some(10)); // a free-prompt record
    acc.push(bond(3), true, true, None); // registered after the fork
    assert_eq!(acc.capital(), 0);
    acc.push(bond(4), true, true, Some(7));
    assert_eq!(acc.capital(), 7);
    assert_eq!(palw_rule_e_even_split_min_capital_v2(0), 1, "a heartbeat-only period never reaches the split");
    assert_eq!(palw_rule_e_even_split_min_capital_v2(10), 4);
}

/// Below the v2 fence the order is v1's exactly; past it capital decides where participation counts.
#[test]
fn rfc18_rule_e_order_v2_is_v1_below_its_fence() {
    use crate::palw_fork_choice_rule_e_v1::{PalwRuleESideV1, palw_rule_e_order_v1, palw_rule_e_order_v2};
    let side = |participation: u32, capital: u128, frontier: u64| PalwRuleESideV1 {
        participation,
        safe_frontier_blue_score: frontier,
        safe_weight: 0,
        live_total: 0,
        exclusive_claims: 0,
        participation_capital_v2: capital,
    };
    let sides = [side(5, 10, 0), side(3, 900, 0), side(3, 900, 7), side(0, 0, 9)];
    for a in &sides {
        for b in &sides {
            for counts in [false, true] {
                assert_eq!(palw_rule_e_order_v2(a, b, counts, false), palw_rule_e_order_v1(a, b, counts));
                assert_eq!(palw_rule_e_order_v2(a, b, counts, true), palw_rule_e_order_v2(b, a, counts, true).reverse());
            }
        }
    }
    // Many minimum bonds (5) against a few large ones (3, more capital): v1 prefers the count, v2 the capital.
    assert_eq!(palw_rule_e_order_v1(&sides[0], &sides[1], true), core::cmp::Ordering::Greater);
    assert_eq!(palw_rule_e_order_v2(&sides[0], &sides[1], true, true), core::cmp::Ordering::Less);
}

// =================================================================================================================================
// Canonical sibling order
// =================================================================================================================================

fn attempt(c: u64, k: u64) -> PalwSiblingKeyV1 {
    PalwSiblingKeyV1 { class_id: Some(class(c)), execution_key: Hash64::from_u64_word(k) }
}

fn other(k: u64) -> PalwSiblingKeyV1 {
    PalwSiblingKeyV1 { class_id: None, execution_key: Hash64::from_u64_word(k) }
}

fn applied(works: &[PalwSiblingKeyV1]) -> Vec<PalwSiblingKeyV1> {
    palw_sibling_canonical_order_v1(works).into_iter().map(|i| works[i]).collect()
}

#[test]
fn rfc18_sibling_order_is_a_permutation_keeping_non_attempt_slots() {
    let works = [other(1), attempt(1, 10), attempt(2, 20), other(2), attempt(1, 11), attempt(3, 30)];
    let order = palw_sibling_canonical_order_v1(&works);
    let mut sorted = order.clone();
    sorted.sort();
    assert_eq!(sorted, (0..works.len()).collect::<Vec<_>>(), "a permutation");
    assert_eq!((order[0], order[3]), (0, 3), "non-attempt works keep their slots");
}

/// The admitted sequence is a function of the SET: any arrival/merge order of the same siblings gives the same sequence.
#[test]
fn rfc18_sibling_order_is_independent_of_merge_order() {
    let base = vec![attempt(1, 10), attempt(2, 20), attempt(3, 30), attempt(1, 11), attempt(2, 21), attempt(3, 31)];
    let reference = applied(&base);
    let mut rotated = base.clone();
    for _ in 0..base.len() {
        rotated.rotate_left(1);
        assert_eq!(applied(&rotated), reference);
    }
    let mut reversed = base.clone();
    reversed.reverse();
    assert_eq!(applied(&reversed), reference);
}

/// Round robin: with three models, the first three admitted are one per model (a contended cap of 3 admits all three models).
#[test]
fn rfc18_sibling_order_round_robins_over_models() {
    let works = vec![attempt(1, 10), attempt(1, 11), attempt(1, 12), attempt(2, 20), attempt(3, 30)];
    let first_three: BTreeSet<Hash64> = applied(&works).iter().take(3).map(|w| w.class_id.unwrap()).collect();
    assert_eq!(first_three.len(), 3, "each model's sibling is admitted before any model's second");
}

// =================================================================================================================================
// Permissionless anchor seed
// =================================================================================================================================

fn contribution(daa: u64, block: u64, b: u32, exec: u64) -> PalwAnchorContributionV1 {
    PalwAnchorContributionV1 {
        daa_score: daa,
        block: Hash64::from_u64_word(block),
        bond: bond(b),
        execution: Hash64::from_u64_word(exec),
        admitted: true,
        bond_matured: true,
    }
}

#[test]
fn rfc18_anchor_seed_needs_two_distinct_bonds_and_ignores_unadmitted_and_immature() {
    let policy = PalwPermissionlessAnchorPolicyV1::design_unapproved_v1();
    let one = [contribution(100, 1, 1, 11), contribution(101, 2, 1, 12)];
    assert_eq!(palw_permissionless_anchor_seed_v1(&one, 100, None, &policy), None, "one bond cannot seed alone");
    let mut junk = contribution(100, 3, 2, 21);
    junk.admitted = false;
    let mut young = contribution(100, 4, 3, 31);
    young.bond_matured = false;
    assert_eq!(palw_permissionless_anchor_seed_v1(&[one[0], junk, young], 100, None, &policy), None);
    let two = [one[0], contribution(102, 5, 4, 41)];
    let (draw_daa, _) = palw_permissionless_anchor_seed_v1(&two, 100, None, &policy).expect("two bonds seed");
    assert_eq!(draw_daa, 100, "the draw resolves at the earliest contribution");
    // The producer's own bond is excluded where the caller knows it.
    assert_eq!(palw_permissionless_anchor_seed_v1(&two, 100, Some(&bond(4)), &policy), None);
    // Below the slot nothing counts.
    assert_eq!(palw_permissionless_anchor_seed_v1(&two, 101, None, &policy), None);
}

/// Order- and colour-independent; a bond's later attempts neither re-roll nor add.
#[test]
fn rfc18_anchor_seed_is_a_function_of_the_per_bond_first_contributions() {
    let policy = PalwPermissionlessAnchorPolicyV1::design_unapproved_v1();
    let set = vec![contribution(100, 1, 1, 11), contribution(100, 2, 2, 21), contribution(101, 3, 3, 31)];
    let reference = palw_permissionless_anchor_seed_v1(&set, 100, None, &policy).unwrap();
    let mut reversed = set.clone();
    reversed.reverse();
    assert_eq!(palw_permissionless_anchor_seed_v1(&reversed, 100, None, &policy), Some(reference));
    // Bond 1 publishes a later win: its earliest contribution stands, the seed does not move.
    let mut extra = set.clone();
    extra.push(contribution(103, 9, 1, 99));
    assert_eq!(palw_permissionless_anchor_seed_v1(&extra, 100, None, &policy), Some(reference));
    // A different honest execution moves it (the unpredictable input).
    let mut moved = set.clone();
    moved[1].execution = Hash64::from_u64_word(22);
    assert_ne!(palw_permissionless_anchor_seed_v1(&moved, 100, None, &policy), Some(reference));
}

// =================================================================================================================================
// DAG width (stub)
// =================================================================================================================================

#[test]
fn rfc18_dag_width_estimate_is_monotone_and_the_schedule_is_structurally_checked() {
    // testnet-12: 120 s blocks, k = 1. A 2 s propagation delay versus a 5-minute inference.
    let short = palw_dag_width_red_permille_v1(2_000, 120_000, 1);
    let long = palw_dag_width_red_permille_v1(300_000, 120_000, 1);
    assert!(short < long, "a longer effective delay reds more ({short}‰ vs {long}‰)");
    assert!(palw_dag_width_red_permille_v1(300_000, 120_000, 4) < long, "a wider k reds less");
    let probe = PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::new(FENCE));
    assert_eq!(probe.schedule.refusal(), None);
    let bad = PalwDagWidthScheduleV1 { ghostdag_k: 4, target_time_per_block_ms: 120_000, max_block_parents: 10, mergeset_size_limit: 30 };
    assert!(bad.refusal().is_some());
    assert_ne!(probe.schedule.id(), bad.id());
}
