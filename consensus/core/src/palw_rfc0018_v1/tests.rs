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
        certification: crate::palw_security_certification_v1::PalwSecurityCertificationPolicyV1::design_unapproved_v1(720),
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
// Gate C — SecurityCertified (adopted 2026-10-15)
// =================================================================================================================================

use crate::palw_security_certification_v1::*;

const DAY: u64 = 720;

fn claim_id(n: u64) -> Hash64 {
    Hash64::from_u64_word(0x5EC0_0000 + n)
}

/// A certification row of `claim` with `outcome` written into `state`'s kernel route.
fn certify(state: &mut PalwChainStateV2, claim: &Hash64, outcome: Option<PalwCertificationOutcomeV1>) {
    let row = PalwClaimCertificationRowV1 {
        claim: claim.as_bytes(),
        class_id: class(1).as_bytes(),
        producer_bond: [1; 64],
        producer_principal: [2; 64],
        attempt: true,
        final_daa: FENCE + 10,
        committed_daa: FENCE + 10,
        committed_span: 0,
        anchor: None,
        draws: Vec::new(),
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: Vec::new(),
        held_reward: 0,
        payout_payload: [3; 64],
        outcome: outcome.map(|o| (o, FENCE + 99)),
    };
    state.set_kernel_aux_row_for_tests(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        certification_claim_key_v1(&palw_certification_id_v1(claim)),
        Some(borsh::to_vec(&row).unwrap()),
    );
}

#[test]
fn rfc18_gate_c_is_the_identity_below_its_fence_and_on_every_shipped_network() {
    let shipped = t12_state_params();
    let armed = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let state = PalwChainStateV2::genesis();
    for phase in [
        PalwClaimPhaseV2::Provisional,
        PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: 10 },
        PalwClaimPhaseV2::Final { final_daa: 20 },
    ] {
        let c = claim_at(FENCE + 1, phase.clone());
        assert!(palw_independently_work_certified_v1(&claim_id(1), &c, &PalwGateCViewV1::of(&shipped, &state)), "no mirror: legacy");
        assert_eq!(palw_gate_c_weight_v1(&shipped, &state, &claim_id(1), &c, 77), 77);
        assert_eq!(palw_claim_provisional_weight_v1(&shipped, &c), 300);
        assert!(palw_gate_c_settles_anchor_v1(&shipped, &c));
        // A claim accepted BELOW the fence keeps its old rule forever (A-2), whatever the rule past it.
        let old = claim_at(FENCE - 1, phase);
        assert!(palw_independently_work_certified_v1(&claim_id(2), &old, &PalwGateCViewV1::of(&armed, &state)));
        assert_eq!(palw_claim_provisional_weight_v1(&armed, &old), 300);
        assert!(palw_gate_c_settles_anchor_v1(&armed, &old));
    }
}

/// `Final` is not `SecurityCertified`: a governed claim adds nothing until its certification row certifies; a Panel licence, `Final`
/// alone, a failed or vetoed certification and a void never certify.
#[test]
fn rfc18_gate_c_final_is_not_security_certified() {
    let p = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let mut state = PalwChainStateV2::genesis();
    let fin = claim_at(FENCE, PalwClaimPhaseV2::Final { final_daa: FENCE + 60 });
    let licensed = claim_at(FENCE, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: FENCE + 30 });
    let view = |p: &PalwStateParamsV2, s: &PalwChainStateV2, id: u64, c: &PalwClaimStateV2| {
        palw_independently_work_certified_v1(&claim_id(id), c, &PalwGateCViewV1::of(p, s))
    };
    assert!(!view(&p, &state, 1, &fin), "Final without a certification row");
    assert!(!view(&p, &state, 1, &licensed), "a Panel licence is no certification");
    for failed in [
        PalwCertificationOutcomeV1::NotCertified,
        PalwCertificationOutcomeV1::Vetoed,
        PalwCertificationOutcomeV1::NoPopulation,
        PalwCertificationOutcomeV1::Expired,
        PalwCertificationOutcomeV1::Withdrawn,
    ] {
        certify(&mut state, &claim_id(1), Some(failed));
        assert!(!view(&p, &state, 1, &fin), "{failed:?} certifies nothing");
        assert_eq!(palw_gate_c_weight_v1(&p, &state, &claim_id(1), &fin, 9), 0);
    }
    certify(&mut state, &claim_id(1), None);
    assert!(!view(&p, &state, 1, &fin), "undecided certifies nothing");
    certify(&mut state, &claim_id(1), Some(PalwCertificationOutcomeV1::Certified));
    assert!(view(&p, &state, 1, &fin));
    assert_eq!(palw_gate_c_weight_v1(&p, &state, &claim_id(1), &fin, 9), 9);
    let voided = claim_at(FENCE, PalwClaimPhaseV2::Voided { voided_daa: FENCE + 70, reason: PalwVoidReasonV2::BindTimeout });
    assert!(!view(&p, &state, 1, &voided), "a reversed claim is never certified, whatever its row says");
    let closed = with_mirror(PalwGateCRuleV1::FailClosedV1);
    assert!(!view(&closed, &state, 1, &fin), "the fail-closed fixture certifies nothing");
    // Provisional weight is 0 past the fence for EVERY phase (one number over the claim's life), and no licence/Final site ticks the
    // settled-anchor clock for a governed claim (it ticks once, at certification).
    for c in [&fin, &licensed, &voided] {
        assert_eq!(palw_claim_provisional_weight_v1(&p, c), 0);
        assert!(!palw_gate_c_settles_anchor_v1(&p, c));
    }
}

/// ONE consensus-credit rule for frontier and settlement: a SecurityCertified ATTEMPT in Final. The "only a Free-Prompt Final exists"
/// case moves neither, certified or not.
#[test]
fn rfc18_one_rule_for_frontier_and_settlement_and_the_free_prompt_only_case() {
    use crate::palw_state_v2::PalwClaimSourceV2;
    let fin = PalwClaimPhaseV2::Final { final_daa: 1 };
    let fp = PalwClaimSourceV2::FreePrompt { quanta: 8, spent: Default::default() };
    assert!(!palw_consensus_credit_anchor_v1(true, true, &fp, &fin), "a certified free-prompt Final anchors nothing");
    assert!(!palw_consensus_credit_anchor_v1(true, false, &PalwClaimSourceV2::Attempt, &fin));
    assert!(palw_consensus_credit_anchor_v1(true, true, &PalwClaimSourceV2::Attempt, &fin));
    assert!(palw_consensus_credit_anchor_v1(false, false, &fp, &fin), "below the fence the released frontier takes any source");
    // The frontier site agrees with the pure rule on a real state.
    let p = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let mut state = PalwChainStateV2::genesis();
    let mut fp_claim = claim_at(FENCE, fin.clone());
    fp_claim.source = fp;
    certify(&mut state, &claim_id(5), Some(PalwCertificationOutcomeV1::Certified));
    assert!(!palw_gate_c_advances_frontier_v1(&p, &state, &claim_id(5), &fp_claim));
    let attempt = claim_at(FENCE, fin);
    assert!(palw_gate_c_advances_frontier_v1(&p, &state, &claim_id(5), &attempt));
    assert!(!palw_gate_c_advances_frontier_v1(&p, &state, &claim_id(6), &attempt), "another claim's row certifies nothing");
}

fn drawn_row(positive: usize, negative: usize, m: usize) -> PalwClaimCertificationRowV1 {
    use crate::palw_model_eligibility_v1::{PalwEligibilityAttestationRowV1, PalwEligibilityVerdictV1};
    let draws: Vec<[u8; 64]> = (0..m).map(|i| [i as u8 + 10; 64]).collect();
    let mut attestations = Vec::new();
    for (i, d) in draws.iter().enumerate() {
        if i < positive {
            attestations.push(PalwEligibilityAttestationRowV1 {
                verifier: *d,
                verdict: PalwEligibilityVerdictV1::Reproduced,
                transcript: [7; 64],
                attested_daa: 1,
            });
        } else if i < positive + negative {
            attestations.push(PalwEligibilityAttestationRowV1 {
                verifier: *d,
                verdict: PalwEligibilityVerdictV1::Unavailable,
                transcript: [0; 64],
                attested_daa: 1,
            });
        }
    }
    PalwClaimCertificationRowV1 {
        claim: [1; 64],
        class_id: [2; 64],
        producer_bond: [3; 64],
        producer_principal: [4; 64],
        attempt: true,
        final_daa: 1_000,
        committed_daa: 1_000,
        committed_span: 0,
        anchor: Some((1, [5; 64])),
        draws,
        drawn_daa: 1_010,
        deadline_daa: 1_010 + 2 * DAY,
        attestations,
        held_reward: 0,
        payout_payload: [6; 64],
        outcome: None,
    }
}

#[test]
fn rfc18_certification_verdict_m_of_k_veto_deadline_and_prosecution_window() {
    let policy = PalwSecurityCertificationPolicyV1::design_unapproved_v1(DAY);
    let (k, m) = (policy.k as usize, policy.m as usize);
    let ok = PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: false };
    let elapsed = 1_000 + policy.prosecution_window_daa;
    // k positives, prosecution window not elapsed: wait — never fail at the attestation deadline.
    assert_eq!(palw_certification_verdict_v1(&policy, &drawn_row(k, 0, m), ok, 1_100), None);
    assert_eq!(palw_certification_verdict_v1(&policy, &drawn_row(k, 0, m), ok, elapsed), Some(PalwCertificationOutcomeV1::Certified));
    // An open accusation holds the decision.
    let pending = PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: true };
    assert_eq!(palw_certification_verdict_v1(&policy, &drawn_row(k, 0, m), pending, elapsed), None);
    // The adopted rule is m-of-k alone (no veto): negatives beside k positives do not refuse.
    assert_eq!(policy.veto, 0);
    assert_eq!(palw_certification_verdict_v1(&policy, &drawn_row(k, 2, m), ok, elapsed), Some(PalwCertificationOutcomeV1::Certified));
    // The veto proposal (U-6), where a policy sets it: honest negatives stop certification even with k positives.
    let vetoing = PalwSecurityCertificationPolicyV1 { veto: 2, ..policy };
    assert_eq!(
        palw_certification_verdict_v1(&vetoing, &drawn_row(k, 2, m), ok, elapsed),
        Some(PalwCertificationOutcomeV1::Vetoed)
    );
    // Fewer than k by the deadline.
    let row = drawn_row(k - 1, 0, m);
    assert_eq!(palw_certification_verdict_v1(&policy, &row, ok, row.deadline_daa), None);
    assert_eq!(palw_certification_verdict_v1(&policy, &row, ok, row.deadline_daa + 1), Some(PalwCertificationOutcomeV1::NotCertified));
    // No longer Final (a conviction reversed it).
    let gone = PalwCertificationClaimFactsV1 { still_final: false, accusation_pending: false };
    assert_eq!(palw_certification_verdict_v1(&policy, &drawn_row(k, 0, m), gone, elapsed), Some(PalwCertificationOutcomeV1::Withdrawn));
    // Never drawn by `certify_by_daa`.
    let mut undrawn = drawn_row(0, 0, 0);
    undrawn.anchor = None;
    assert_eq!(palw_certification_verdict_v1(&policy, &undrawn, ok, 1_000 + policy.certify_by_daa), None);
    assert_eq!(
        palw_certification_verdict_v1(&policy, &undrawn, ok, 1_001 + policy.certify_by_daa),
        Some(PalwCertificationOutcomeV1::Expired)
    );
    assert!(PALW_SECURITY_PUBLIC_CHECK_CLASSES_PINNED_V1.is_empty(), "no class certifies by the public-check path in this binary");
}

/// The pool is the GLOBAL registry: maturity and collateral bound it, the producer's operator is out, and nothing about holding the
/// model is read.
#[test]
fn rfc18_certification_pool_is_global_and_excludes_the_producer() {
    let policy = PalwSecurityCertificationPolicyV1::design_unapproved_v1(DAY);
    let commit = 100 * DAY;
    let producer = [9u8; 64];
    assert!(palw_certification_pool_admits_v1(&policy, commit, &producer, true, 0, 1, &[1; 64]));
    assert!(!palw_certification_pool_admits_v1(&policy, commit, &producer, true, 0, 1, &producer), "never the producer's operator");
    assert!(!palw_certification_pool_admits_v1(&policy, commit, &producer, false, 0, 1, &[1; 64]), "Active only");
    assert!(
        !palw_certification_pool_admits_v1(&policy, commit, &producer, true, commit - 1, 1, &[1; 64]),
        "a bond registered after the claim cannot be drawn for it"
    );
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
fn rfc18_anchor_seed_excludes_the_producer_and_ignores_unadmitted_contributions() {
    let policy = PalwPermissionlessAnchorPolicyV1::design_unapproved_v1();
    assert_eq!(policy.min_distinct_bonds, 1, "one bond other than the producer");
    let own = [contribution(100, 1, 1, 11), contribution(101, 2, 1, 12)];
    // The producer's own contributions never seed its own Panel, however many.
    assert_eq!(palw_permissionless_anchor_seed_v1(&own, 100, Some(&bond(1)), &policy), None);
    assert!(!palw_permissionless_anchors_claim_v1(&own, 100, &bond(1)));
    // Another bond's single contribution does.
    assert!(palw_permissionless_anchors_claim_v1(&own, 100, &bond(9)));
    let mut junk = contribution(100, 3, 2, 21);
    junk.admitted = false;
    assert_eq!(palw_permissionless_anchor_seed_v1(&[own[0], junk], 100, Some(&bond(1)), &policy), None, "an unadmitted header is nothing");
    let two = [own[0], contribution(102, 5, 4, 41)];
    let (draw_daa, _) = palw_permissionless_anchor_seed_v1(&two, 100, Some(&bond(1)), &policy).expect("another bond seeds");
    assert_eq!(draw_daa, 102, "the draw resolves at the earliest contribution of another bond");
    // Below the slot nothing counts.
    assert_eq!(palw_permissionless_anchor_seed_v1(&two, 103, Some(&bond(1)), &policy), None);
    // Reach is claim-independent: any one contribution.
    assert_eq!(palw_permissionless_anchor_reach_v1(&own, 500), Some(101));
}

/// Contributions are COMMITTED fold output: attempt claims accepted below the anchor block (blue or red carriers alike), phase-
/// independent, never a free-prompt claim, never a claim the anchor block itself accepted.
#[test]
fn rfc18_anchor_contributions_are_committed_fold_output() {
    use crate::palw_state_v2::PalwClaimSourceV2;
    let mut state = PalwChainStateV2::genesis();
    let mut red = claim_at(FENCE, PalwClaimPhaseV2::Provisional);
    red.accepted_blue_score = 40;
    red.accepted_block = Hash64::from_u64_word(0x8ED);
    red.bond = bond(2);
    let mut voided = claim_at(FENCE, PalwClaimPhaseV2::Voided { voided_daa: FENCE + 1, reason: PalwVoidReasonV2::BindTimeout });
    voided.accepted_blue_score = 41;
    voided.bond = bond(3);
    let mut fp = claim_at(FENCE, PalwClaimPhaseV2::Final { final_daa: FENCE + 9 });
    fp.source = PalwClaimSourceV2::FreePrompt { quanta: 8, spent: Default::default() };
    fp.accepted_blue_score = 42;
    let mut same_block = claim_at(FENCE, PalwClaimPhaseV2::Provisional);
    same_block.accepted_blue_score = 50;
    for (n, c) in [(1u64, red), (2, voided), (3, fp), (4, same_block)] {
        state.set_false_valid_rows_for_tests(claim_id(n), Some(c), None, None, 0);
    }
    let contributions = palw_permissionless_contributions_from_state_v1(&state, 50);
    let blocks: BTreeSet<Hash64> = contributions.iter().map(|c| c.block).collect();
    assert_eq!(contributions.len(), 2, "the red carrier and the later-voided attempt; not the FP claim, not the anchor block's own");
    assert!(blocks.contains(&Hash64::from_u64_word(0x8ED)), "a RED carrier is recognized");
    assert!(contributions.iter().all(|c| c.admitted));
}

// =================================================================================================================================
// Round 5 (user decision 2026-10-15): no model review; per-claim certification for EVERY model tier
// =================================================================================================================================

/// **Model-level review is abolished and U-2's Probation bar removed; the per-claim drawn certification is the ONE safety gate, the
/// same for every tier** (ADR-0179; the user: "これは残す。そうじゃないと claim の検証ができない"). A claim of an APPROVED model (genesis /
/// annual fork, 1.0), of a REGISTERED model (fixed 0.3) and of an untiered network earns frontier, settlement depth, rule-E
/// participation, settled-anchor and weight credit exactly when it is SecurityCertified — never for its tier.
#[test]
fn rfc18_round5_every_tier_needs_security_certified_and_none_is_exempt() {
    use crate::palw_fork_choice_rule_e_v1::{PalwRuleECertificationV1, PalwRuleENoBondBudgetV1};
    use crate::palw_annual_catalog_v1::{ApprovedModelV1, PalwAnnualCatalogChainV1};
    use crate::palw_model_tier_v1::{PalwModelTierFenceV1, palw_model_tier_of_v1};
    let root = Hash64::from_u64_word(0xA11);
    // RFC-0019: listed in the genesis catalog (edition 0) …
    let mut listed = PalwModelTierFenceV1::at(crate::config::params::ForkActivation::new(FENCE));
    listed.catalogs = PalwAnnualCatalogChainV1::genesis_only(vec![ApprovedModelV1::of(class(1), root)]);
    // … or by an annual catalog edition at the fence's next DAA: neither exempts a claim from Gate C.
    let mut annual = PalwModelTierFenceV1::at(crate::config::params::ForkActivation::new(FENCE));
    annual.catalogs = annual.catalogs.clone().with_edition(FENCE + 1, vec![ApprovedModelV1::of(class(1), root)]);
    let unlisted = PalwModelTierFenceV1::at(crate::config::params::ForkActivation::new(FENCE));
    for (tag, tier) in [
        ("approved", Some(listed.mirror())),
        ("approved", Some(annual.mirror())),
        ("registered", Some(unlisted.mirror())),
        ("untiered", None),
    ] {
        let p = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1).with_model_tier_v1(tier.clone());
        if let Some(mirror) = tier.as_ref() {
            let t = palw_model_tier_of_v1(Some(mirror), &class(1), &root, false, FENCE + 1);
            assert_eq!(t.is_approved(), tag == "approved", "{tag}: the tier is what the lists say");
        }
        for certified in [false, true] {
            let mut state = PalwChainStateV2::genesis();
            let id = claim_id(77);
            let claim = final_attempt(60);
            state.set_false_valid_rows_for_tests(id, Some(claim.clone()), None, None, 0);
            certify(&mut state, &id, certified.then_some(PalwCertificationOutcomeV1::Certified));
            let earns = certified;
            let view = PalwGateCViewV1::of(&p, &state);
            assert_eq!(palw_independently_work_certified_v1(&id, &claim, &view), earns, "{tag}/{certified}: the single predicate");
            assert_eq!(palw_gate_c_weight_v1(&p, &state, &id, &claim, 500), if earns { 500 } else { 0 }, "{tag}: final weight");
            assert_eq!(palw_claim_provisional_weight_v1(&p, &claim), 0, "{tag}: provisional weight is 0 past the fence");
            assert!(!palw_gate_c_settles_anchor_v1(&p, &claim), "{tag}: the settled-anchor tick is the certification's");
            assert_eq!(palw_gate_c_advances_frontier_v1(&p, &state, &id, &claim), earns, "{tag}: safe frontier");
            let settlement = crate::palw_settlement_v1::palw_settlement_v1(&state, &p, None, 0).unwrap();
            assert_eq!(settlement.uncertified_final, if earns { 0 } else { 1 }, "{tag}: reported uncertified");
            let record = state
                .palw_rule_e_record_v1(&p, false, None, &Default::default(), &PalwRuleENoBondBudgetV1, id, &claim)
                .unwrap();
            assert_eq!(
                record.certification,
                if earns { PalwRuleECertificationV1::Certified } else { PalwRuleECertificationV1::Uncertified },
                "{tag}: rule E reads the same predicate"
            );
        }
    }
}

// =================================================================================================================================
// Frontier rollback (RFC-0018 §7.5a)
// =================================================================================================================================

fn final_attempt(blue: u64) -> PalwClaimStateV2 {
    let mut c = claim_at(FENCE, PalwClaimPhaseV2::Final { final_daa: FENCE + 60 });
    c.accepted_blue_score = blue;
    c.accepted_block = Hash64::from_u64_word(0xB10C_0000 + blue);
    c
}

/// A certified claim later convicted (voided) stops standing as the frontier; the frontier falls back to the deepest remaining
/// certified claim, never below the floor; a retirement never moves it (the floor takes it); pruning stays at or below the
/// shallowest claim a conviction could still make the frontier.
#[test]
fn rfc18_frontier_rolls_back_on_conviction_and_pruning_never_passes_a_rollback_target() {
    let p = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let mut state = PalwChainStateV2::genesis();
    for (n, blue) in [(10u64, 10u64), (20, 20), (30, 30)] {
        state.set_false_valid_rows_for_tests(claim_id(n), Some(final_attempt(blue)), None, None, 0);
        certify(&mut state, &claim_id(n), Some(PalwCertificationOutcomeV1::Certified));
    }
    // An uncertified Final above them is no frontier.
    state.set_false_valid_rows_for_tests(claim_id(40), Some(final_attempt(40)), None, None, 0);
    certify(&mut state, &claim_id(40), None);
    assert_eq!(palw_gate_c_frontier_v1(&state, &p, u64::MAX).0, 30);
    assert_eq!(palw_frontier_rollback_bound_v1(&state, &p), Some(10), "the shallowest rollback target");
    // The unresolved bound still applies.
    assert_eq!(palw_gate_c_frontier_v1(&state, &p, 25).0, 20);
    // Conviction of the frontier claim: it is voided, and the frontier falls back to 20.
    let mut convicted = final_attempt(30);
    convicted.phase = PalwClaimPhaseV2::Voided { voided_daa: FENCE + 90, reason: PalwVoidReasonV2::BindTimeout };
    state.set_false_valid_rows_for_tests(claim_id(30), Some(convicted), None, None, 0);
    assert_eq!(palw_gate_c_frontier_v1(&state, &p, u64::MAX).0, 20);
    // The claim at 10 retires: the floor takes it; the frontier is unchanged; the rollback bound moves up to 20.
    let floor = PalwCertifiedFrontierFloorV1 { blue_score: 10, block: Hash64::from_u64_word(0xB10C_0000 + 10).as_bytes() };
    state.set_kernel_aux_row_for_tests(PALW_SECURITY_CERTIFICATION_TABLE_V1, certification_frontier_floor_key_v1(), Some(borsh::to_vec(&floor).unwrap()));
    state.set_false_valid_rows_for_tests(claim_id(10), None, None, None, 0);
    assert_eq!(palw_gate_c_frontier_v1(&state, &p, u64::MAX).0, 20);
    assert_eq!(palw_frontier_rollback_bound_v1(&state, &p), Some(20));
    // Convicting the last certified claim leaves the floor.
    let mut convicted = final_attempt(20);
    convicted.phase = PalwClaimPhaseV2::Voided { voided_daa: FENCE + 95, reason: PalwVoidReasonV2::BindTimeout };
    state.set_false_valid_rows_for_tests(claim_id(20), Some(convicted), None, None, 0);
    assert_eq!(palw_gate_c_frontier_v1(&state, &p, u64::MAX).0, 10, "never below the floor");
    assert_eq!(palw_frontier_rollback_bound_v1(&state, &p), None, "no convictable target left: the floor never moves back");
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
