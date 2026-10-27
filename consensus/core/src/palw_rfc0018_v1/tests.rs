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
        fp_receipt_tail_daa: 0,
        attestor_liability_from_daa: None,
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
    let params_id = base.consensus_params_id();
    let mut never = base.clone();
    never.palw_dag_sibling_attribution_v1 = Some(ForkActivation::never());
    never.palw_panel_anchor_permissionless_v1 = Some(ForkActivation::never());
    never.palw_rule_e_participation_v2 = Some(ForkActivation::never());
    never.palw_independently_work_certified_v1 = Some(ForkActivation::never());
    never.palw_dag_width_schedule_v1 = Some(PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::never()));
    never.validate_palw_rfc0018_v1().unwrap();
    assert_eq!(never.consensus_params_id(), params_id, "Some(never()) hashes as None");
    // The identity two nodes compare normalizes `Some(never())` to absence; the schedule id is a REPORT, and like every Some-only
    // fence's (`for_each_fence` visits a present value, `never()` included) it names the never-armed fences.
    assert_eq!(never.consensus_identity_id(), base.consensus_identity_id(), "Some(never()) is absence to the identity");
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
    // Gate C's prerequisites at or below it (the frozen tier rule and the kernel route's aux 136), so the mirror is what the check
    // reaches.
    q.palw_model_tier_v1 = Some(crate::palw_model_tier_v1::PalwModelTierFenceV1::at(ForkActivation::new(FENCE)));
    q.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(FENCE));
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
    use crate::palw_annual_catalog_v1::{ApprovedModelV1, PalwAnnualCatalogChainV1};
    use crate::palw_fork_choice_rule_e_v1::{PalwRuleECertificationV1, PalwRuleENoBondBudgetV1};
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
    state.set_kernel_aux_row_for_tests(
        PALW_SECURITY_CERTIFICATION_TABLE_V1,
        certification_frontier_floor_key_v1(),
        Some(borsh::to_vec(&floor).unwrap()),
    );
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
    let bad = PalwDagWidthScheduleV1 {
        ghostdag_k: 4,
        target_time_per_block_ms: 120_000,
        max_block_parents: 10,
        mergeset_size_limit: 30,
        merge_depth: 30,
    };
    assert!(bad.refusal().is_some());
    assert_ne!(probe.schedule.id(), bad.id());
}

// =================================================================================================================================
// RFC-0023 / ADR-0186 stage 1: an open claim ends the resolved prefix only within its own liability bound
// =================================================================================================================================

fn stage1(p: PalwStateParamsV2) -> PalwStateParamsV2 {
    p.with_unresolved_bound_from_daa(Some(FENCE))
}

/// An open (licensed, undecided) attempt accepted at `daa` / `blue` whose unresolved bound is `daa + l`.
fn open_attempt(daa: u64, blue: u64, l: u64) -> PalwClaimStateV2 {
    let mut c = claim_at(daa, PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: daa + 1 });
    c.accepted_blue_score = blue;
    c.accepted_block = Hash64::from_u64_word(0xB10C_0000 + blue);
    c.trace_retention_daa = daa + l;
    c
}

fn certified_final(state: &mut PalwChainStateV2, n: u64, daa: u64, blue: u64, final_daa: u64, l: u64) {
    let mut c = claim_at(daa, PalwClaimPhaseV2::Final { final_daa });
    c.accepted_blue_score = blue;
    c.accepted_block = Hash64::from_u64_word(0xB10C_0000 + blue);
    c.trace_retention_daa = daa + l;
    state.set_false_valid_rows_for_tests(claim_id(n), Some(c), None, None, 0);
    certify(state, &claim_id(n), Some(PalwCertificationOutcomeV1::Certified));
}

#[test]
fn rfc23s1_the_resolved_prefix_skips_only_open_claims_past_their_own_bound_and_is_the_identity_below_the_fence() {
    use crate::palw_unresolved_bound_v1::*;
    let gate_c = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let armed = stage1(gate_c.clone());
    let mut state = PalwChainStateV2::genesis();
    // A: open at blue 10, accepted at FENCE + 1, bound FENCE + 1 + 1_000. B: certified Final at blue 20.
    state.set_false_valid_rows_for_tests(claim_id(1), Some(open_attempt(FENCE + 1, 10, 1_000)), None, None, 0);
    state.set_unresolved_for_tests(10, claim_id(1), true);
    certified_final(&mut state, 2, FENCE + 2, 20, FENCE + 50, 1_000);
    for now in [FENCE + 10, FENCE + 1_001] {
        assert_eq!(palw_resolved_through_v1(&state, &gate_c, now), 9, "below the fence the oldest open claim always ends the prefix");
    }
    assert_eq!(palw_resolved_through_v1(&state, &armed, FENCE + 1_001), 9, "at its bound A still holds the prefix");
    assert_eq!(palw_resolved_through_v1(&state, &armed, FENCE + 1_002), u64::MAX, "past its bound A no longer does");
    // The frontier formula is unchanged: the deepest frontier-eligible Final inside the prefix.
    assert_eq!(palw_gate_c_frontier_v1(&state, &armed, palw_resolved_through_v1(&state, &armed, FENCE + 500)).0, 0);
    assert_eq!(palw_gate_c_frontier_v1(&state, &armed, palw_resolved_through_v1(&state, &armed, FENCE + 1_002)).0, 20);
    // A past its bound keeps its liability: still open in the index (pruning's first bound stays at it), never certified credit.
    assert_eq!(state.unresolved_first_blue_score_v1(), Some(10));
    assert!(palw_claim_lapsed_bound_v1(&armed, state.claim(&claim_id(1)).unwrap(), FENCE + 1_002));
    // A claim accepted below the fence is never governed (byte-identical behaviour for every pre-fence claim).
    let mut old = open_attempt(FENCE - 5, 5, 10);
    old.accepted_daa = FENCE - 5;
    assert!(!palw_claim_lapsed_bound_v1(&armed, &old, u64::MAX));
}

#[test]
fn rfc23s1_a_claim_final_only_after_its_bound_earns_no_frontier_credit() {
    use crate::palw_unresolved_bound_v1::*;
    let armed = stage1(with_mirror(PalwGateCRuleV1::SecurityCertifiedV1));
    let mut state = PalwChainStateV2::genesis();
    // Final within the bound: frontier-eligible. Final past the bound: never (even certified).
    certified_final(&mut state, 1, FENCE + 1, 10, FENCE + 900, 1_000);
    certified_final(&mut state, 2, FENCE + 2, 20, FENCE + 1_100, 1_000);
    assert!(palw_gate_c_advances_frontier_v1(&armed, &state, &claim_id(1), state.claim(&claim_id(1)).unwrap()));
    assert!(!palw_gate_c_advances_frontier_v1(&armed, &state, &claim_id(2), state.claim(&claim_id(2)).unwrap()));
    assert_eq!(palw_gate_c_frontier_v1(&state, &armed, u64::MAX).0, 10);
    // Below the fence the late Final is an ordinary certified Final.
    let gate_c = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    assert_eq!(palw_gate_c_frontier_v1(&state, &gate_c, u64::MAX).0, 20);
}

/// **The continuous hand-over attack** (RFC-0023 §3.4): every `gap` DAA the attacker opens a claim and keeps it open to its bound;
/// honest certified claims finalize meanwhile. Past the fence the frontier lag (now − accepted DAA of the frontier claim) stays
/// within `L + gap + finality` in steady state — each old claim releases the prefix at its own bound and no successor can restart
/// it; below the fence the attacker holds the frontier for the whole run. Measured per cause.
#[test]
fn rfc23s1_a_continuous_hand_over_lags_the_frontier_by_at_most_one_bound() {
    use crate::palw_unresolved_bound_v1::*;
    let armed = stage1(with_mirror(PalwGateCRuleV1::SecurityCertifiedV1));
    let gate_c = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    // L_class: testnet-12's court-demanding retention (4W + W_disclose + bind + receipt = 14,568 DAA, §11.11) — the slow cause.
    let l = 14_568u64;
    let (gap, finality, horizon) = (600u64, 120u64, 120_000u64);
    let mut state = PalwChainStateV2::genesis();
    let (mut worst_armed, mut worst_old) = (0u64, 0u64);
    let mut n = 0u64;
    let mut now = FENCE + 1;
    while now < FENCE + horizon {
        // The attacker's open claim (held to its bound) and one honest claim certified `finality` later.
        n += 1;
        state.set_false_valid_rows_for_tests(claim_id(2 * n), Some(open_attempt(now, now, l)), None, None, 0);
        state.set_unresolved_for_tests(now, claim_id(2 * n), true);
        certified_final(&mut state, 2 * n + 1, now, now + 1, now + finality, l);
        // Past its bound an attacker claim is still open (no slash, collateral held) but lapsed.
        now += gap;
        let frontier = |p: &PalwStateParamsV2| palw_gate_c_frontier_v1(&state, p, palw_resolved_through_v1(&state, p, now)).0;
        let (fa, fo) = (frontier(&armed), frontier(&gate_c));
        if now > FENCE + 2 * l {
            worst_armed = worst_armed.max(now.saturating_sub(fa));
            worst_old = worst_old.max(now.saturating_sub(fo));
        }
    }
    println!("RFC-0023 stage 1 hand-over: L = {l}, gap = {gap}, finality = {finality}: steady-state lag {worst_armed} DAA past the fence, {worst_old} DAA below it");
    assert!(worst_armed <= l + gap + finality + 1, "past the fence the lag is one bound plus one gap ({worst_armed})");
    assert!(worst_old > horizon - 2 * gap, "below the fence the first attacker claim holds the frontier for the whole run ({worst_old})");
}

/// **RFC-0023 §3.4 per cause** (stage 1, dormant `palw_unresolved_bound_v1`): the steady-state frontier lag when a stream of claims
/// (one every `gap` DAA) is each held open by one cause for `hold` DAA before it resolves (a conviction or a settled Final), beside an
/// honest claim certified `finality` later. Hold times are testnet-12's values for each cause (W = 3,042, W_disclose = 1,200, the
/// design attest window 2 days = 1,440 DAA at 120 s, L = 14,568): slow material response (licence then one full answer window),
/// a legitimate court (W_disclose + 4W), false accusations re-filed every W (unbounded without stage 1: a new accusation never
/// restarts the clock), Gate C redraws (unbounded chain of attest windows), court capacity saturation (admitted rows decided by
/// L − W_disclose), and continuous slow claims at the bound. The lag is read from the real `palw_resolved_through_v1` /
/// `palw_gate_c_frontier_v1`; it is a simulation over the state rows, not a node measurement.
#[test]
fn rfc23s1_per_cause_steady_state_lag_is_bounded_by_min_hold_and_l_plus_gap() {
    use crate::palw_unresolved_bound_v1::*;
    let armed = stage1(with_mirror(PalwGateCRuleV1::SecurityCertifiedV1));
    let gate_c = with_mirror(PalwGateCRuleV1::SecurityCertifiedV1);
    let (w, w_disclose, attest, l) = (3_042u64, 1_200u64, 1_440u64, 14_568u64);
    let (gap, finality, horizon) = (600u64, 120u64, 90_000u64);
    let causes: [(&str, u64); 6] = [
        ("slow material response", w_disclose + w),
        ("legitimate court", w_disclose + 4 * w),
        ("false accusation (re-filed every W)", u64::MAX),
        ("Gate C redraw chain", u64::MAX),
        ("court capacity saturation", l - w_disclose),
        ("continuous slow claims at the bound", l),
    ];
    let _ = attest;
    let mut table = Vec::new();
    for (cause, hold) in causes {
        let mut state = PalwChainStateV2::genesis();
        let mut open: Vec<(u64, u64, Hash64)> = Vec::new(); // (resolve_at, blue, id)
        let (mut worst_armed, mut worst_old, mut n, mut now) = (0u64, 0u64, 0u64, FENCE + 1);
        while now < FENCE + horizon {
            n += 1;
            let id = claim_id(2 * n);
            state.set_false_valid_rows_for_tests(id, Some(open_attempt(now, now, l)), None, None, 0);
            state.set_unresolved_for_tests(now, id, true);
            open.push((now.saturating_add(hold), now, id));
            certified_final(&mut state, 2 * n + 1, now, now + 1, now + finality, l);
            now += gap;
            // Resolution by the cause's own end (a conviction voids the attacker claim, which leaves the unresolved set).
            open.retain(|(at, blue, id)| {
                if *at <= now {
                    let mut c = open_attempt(*blue, *blue, l);
                    c.phase = PalwClaimPhaseV2::Voided { voided_daa: *at, reason: PalwVoidReasonV2::BindTimeout };
                    state.set_false_valid_rows_for_tests(*id, Some(c), None, None, 0);
                    state.set_unresolved_for_tests(*blue, *id, false);
                    false
                } else {
                    true
                }
            });
            let frontier = |p: &PalwStateParamsV2| palw_gate_c_frontier_v1(&state, p, palw_resolved_through_v1(&state, p, now)).0;
            if now > FENCE + 2 * l {
                worst_armed = worst_armed.max(now.saturating_sub(frontier(&armed)));
                worst_old = worst_old.max(now.saturating_sub(frontier(&gate_c)));
            }
        }
        let cap = hold.min(l) + gap + finality + 1;
        println!("RFC-0023 §3.4 cause '{cause}': hold {} DAA → lag {worst_armed} DAA past the fence (cap {cap}), {worst_old} DAA below it",
            if hold == u64::MAX { "unbounded".to_string() } else { hold.to_string() });
        assert!(worst_armed <= cap, "{cause}: lag {worst_armed} exceeds min(hold, L) + gap + finality = {cap}");
        if hold == u64::MAX {
            assert!(worst_old > horizon - 2 * gap, "{cause}: below the fence the first held claim stops the frontier for the whole run");
        }
        table.push((cause, worst_armed, worst_old));
    }
    // Bounded causes resolve by themselves at or below L; the unbounded ones are cut at L by stage 1 only.
    assert!(table.iter().all(|(_, a, _)| *a <= l + gap + finality + 1));
}

/// The pre-2026-10-18 class order (classes ranked by their FIRST member's priority) — the reference the fairness test measures.
fn first_member_order(works: &[PalwSiblingKeyV1]) -> Vec<usize> {
    let mut by_class: BTreeMap<Hash64, Vec<(Hash64, usize)>> = BTreeMap::new();
    for (i, w) in works.iter().enumerate() {
        by_class.entry(w.class_id.unwrap()).or_default().push((palw_sibling_priority_v1(&w.execution_key), i));
    }
    let mut classes: Vec<Vec<(Hash64, usize)>> = by_class.into_values().map(|mut m| { m.sort(); m }).collect();
    classes.sort_by(|a, b| a[0].cmp(&b[0]));
    let mut out = Vec::new();
    let mut round = 0;
    while out.len() < works.len() {
        for c in &classes {
            if let Some((_, i)) = c.get(round) {
                out.push(*i);
            }
        }
        round += 1;
    }
    out
}

/// **RFC-0018 (ii) long-run cross-model fairness under the 16-carve emission budget** (ADR-0164 F-EM: at most 16 claim-bearing carves
/// per accepting DAA) and the mergeset round robin. Twenty models share each accepting block; model 0 contributes ONE attempt, the
/// nineteen others flood 32 each. Ranked by their first member, the floods' minima always precede model 0's single priority, so the
/// 16 round-0 slots go to floods and model 0 starves (measured: admitted in 121 of 2,000 blocks, 6 % against a fair 80 %). Ranked by `H(set seed ‖ class)`
/// model 0 is admitted in about 16/20 of the blocks, as every other model. Structural simulation over the real order function; no
/// economics value changes (the 16-carve budget, the round robin and per-class order are unchanged).
#[test]
fn rfc18_long_run_fairness_a_one_attempt_model_is_not_starved_by_flooding_models_under_the_16_carve_budget() {
    const CAP: usize = 16;
    let (blocks, models, flood) = (2_000u64, 20u64, 32u64);
    let mut key = 0u64;
    let mut next = || {
        key += 1;
        let mut h = blake2b_simd::Params::new().hash_length(64).to_state();
        h.update(&key.to_le_bytes());
        Hash64::from_bytes(h.finalize().as_bytes().try_into().unwrap())
    };
    let (mut old_hits, mut new_hits) = (0u64, 0u64);
    let mut new_per_model = vec![0u64; models as usize];
    for _ in 0..blocks {
        let mut works = vec![PalwSiblingKeyV1 { class_id: Some(class(1_000)), execution_key: next() }];
        for m in 1..models {
            for _ in 0..flood {
                works.push(PalwSiblingKeyV1 { class_id: Some(class(1_000 + m)), execution_key: next() });
            }
        }
        let admitted = |order: Vec<usize>| order.into_iter().take(CAP).map(|i| works[i].class_id.unwrap()).collect::<Vec<_>>();
        let old = admitted(first_member_order(&works));
        let new = admitted(palw_sibling_canonical_order_v1(&works));
        old_hits += old.contains(&class(1_000)) as u64;
        new_hits += new.contains(&class(1_000)) as u64;
        for m in 0..models {
            new_per_model[m as usize] += new.contains(&class(1_000 + m)) as u64;
        }
    }
    let (min, max) = (*new_per_model.iter().min().unwrap(), *new_per_model.iter().max().unwrap());
    println!(
        "RFC-0018 (ii) fairness: one-attempt model admitted in {old_hits}/{blocks} blocks by first-member rank, {new_hits}/{blocks} by set-seed rank; \
         per-model admissions under the set-seed rank {min}..{max} (expected {} each)",
        blocks * CAP as u64 / models
    );
    assert!(old_hits * 5 < expect_share(blocks, CAP as u64, models), "the first-member rank starves the one-attempt model below a fifth of its share");
    let expect = blocks * CAP as u64 / models;
    assert!(new_hits * 10 >= expect * 9 && new_hits * 10 <= expect * 11, "the one-attempt model gets its share ({new_hits} vs {expect})");
    assert!(min * 10 >= expect * 9 && max * 10 <= expect * 11, "every model within ±10% of its share ({min}..{max})");
}

/// **RFC-0018 (iii): a slow model-A claim stops only the shared frontier** (bounded by RFC-0023 stage 1), never model B's own
/// progress. A (class 1) stays open — material slow, court pending — from FENCE + 1 to its bound; B (class 2), accepted later,
/// reaches Final and its Gate C certification while A is open. B's certification, B's receipt clock (the certified-payment gate)
/// and B's eligibility to stand as frontier are read with A open; only the resolved prefix waits for A, and past A's bound it no
/// longer does.
#[test]
fn rfc18_a_slow_model_a_claim_stops_only_the_shared_frontier_not_model_b_final_certification_or_payment() {
    use crate::palw_unresolved_bound_v1::*;
    let armed = stage1(with_mirror(PalwGateCRuleV1::SecurityCertifiedV1));
    let l = 14_568u64;
    let mut state = PalwChainStateV2::genesis();
    // Model A: open at blue 10 (its material answers slowly), bound FENCE + 1 + L.
    state.set_false_valid_rows_for_tests(claim_id(1), Some(open_attempt(FENCE + 1, 10, l)), None, None, 0);
    state.set_unresolved_for_tests(10, claim_id(1), true);
    // Model B: accepted at FENCE + 2 (blue 20), Final at FENCE + 200 and certified.
    certified_final(&mut state, 2, FENCE + 2, 20, FENCE + 200, l);
    let mut b = state.claim(&claim_id(2)).unwrap().clone();
    b.class_id = class(2);
    state.set_false_valid_rows_for_tests(claim_id(2), Some(b.clone()), None, None, 0);
    let view = PalwGateCViewV1::of(&armed, &state);
    // B's own progress does not read A: certified, its receipt/payment clock runs, it is frontier-eligible by itself.
    assert!(palw_independently_work_certified_v1(&claim_id(2), &b, &view), "B is SecurityCertified while A is open");
    assert!(palw_fp_receipt_clock_v1(&armed, &state, &claim_id(2), &b, FENCE + 200).is_ok(), "B's certified payment gate opens");
    assert!(palw_gate_c_advances_frontier_v1(&armed, &state, &claim_id(2), &b), "B may stand as frontier on its own");
    // Only the shared frontier waits for A — within A's bound.
    let frontier = |now: u64| palw_gate_c_frontier_v1(&state, &armed, palw_resolved_through_v1(&state, &armed, now)).0;
    assert_eq!(frontier(FENCE + 300), 0, "the shared frontier waits for A while A is within its bound");
    assert_eq!(frontier(FENCE + 1 + l), 0, "at A's bound it still waits");
    assert_eq!(frontier(FENCE + 2 + l), 20, "past A's bound B is the frontier without A resolving");
    // A keeps its liability (still open, never certified credit): stage 1 cuts the wait, not the accusation.
    assert_eq!(state.unresolved_first_blue_score_v1(), Some(10));
}

fn expect_share(blocks: u64, cap: u64, models: u64) -> u64 {
    blocks * cap / models
}

#[test]
fn rfc18_width_histories_switch_at_the_parent_and_restore_on_reorg() {
    let mut p = crate::config::params::MAINNET_PARAMS.clone();
    let base = (p.ghostdag_k(), p.max_block_parents() as u16, p.mergeset_size_limit(), p.merge_depth());
    let mut fence = PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::new(100));
    fence.schedule.merge_depth = base.3 + 17;
    p.palw_dag_width_schedule_v1 = Some(fence);
    for daa in [0, 99, 100, 101, 99, 100] {
        let expected = if daa < 100 { base } else { (3, 16, 360, base.3 + 17) };
        assert_eq!(
            (
                p.ghostdag_k_history().get(daa),
                p.max_block_parents_history().get(daa),
                p.mergeset_size_limit_history().get(daa),
                p.merge_depth_history().get(daa)
            ),
            expected
        );
    }
    fence.activation = ForkActivation::never();
    p.palw_dag_width_schedule_v1 = Some(fence);
    for daa in [0, 99, 100, u64::MAX] {
        assert_eq!(
            (
                p.ghostdag_k_history().get(daa),
                p.max_block_parents_history().get(daa),
                p.mergeset_size_limit_history().get(daa),
                p.merge_depth_history().get(daa)
            ),
            base
        );
    }
}

#[test]
fn rfc18_width_merge_depth_is_committed_and_unmeasured_steps_cannot_arm() {
    let mut f = PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::new(100));
    let id = f.schedule.id();
    f.schedule.merge_depth += 1;
    assert_ne!(id, f.schedule.id());
    let mut p = crate::config::params::MAINNET_PARAMS.clone();
    p.palw_dag_width_schedule_v1 = Some(f);
    assert!(p.validate_palw_rfc0018_v1().is_err());
    f.schedule.merge_depth = 0;
    assert!(f.schedule.refusal().is_some());
}

#[test]
fn rfc18_width_cannot_change_the_clock_shrink_a_cluster_or_outrun_pruning() {
    let base = palw_t12_shipped_params();
    let mut p = base.clone();
    let mut f = PalwDagWidthScheduleFenceV1::unapproved_probe_at(ForkActivation::new(100));
    f.schedule.target_time_per_block_ms += 1;
    p.palw_dag_width_schedule_v1 = Some(f);
    assert!(matches!(p.validate_palw_rfc0018_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("block time")));
    f.schedule.target_time_per_block_ms = base.target_time_per_block();
    p.blockrate.ghostdag_k = f.schedule.ghostdag_k + 1;
    p.palw_dag_width_schedule_v1 = Some(f);
    assert!(matches!(p.validate_palw_rfc0018_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("decrease k")));
    p.blockrate.ghostdag_k = base.ghostdag_k();
    f.schedule.merge_depth = base.finality_depth() + 1;
    p.palw_dag_width_schedule_v1 = Some(f);
    assert!(matches!(p.validate_palw_rfc0018_v1(), Err(PalwModeV2Error::Invalid(why)) if why.contains("geometry")));
}

// =================================================================================================================================
// Lane gatec-decouple (2026-10-22): Gate C without RFC-0017's exam; the arm-readiness record gate
// =================================================================================================================================

fn gate_c_ready() -> Params {
    let mut p = palw_t12_shipped_params();
    p.palw_independently_work_certified_v1 = Some(ForkActivation::new(FENCE));
    p.palw_model_tier_v1 = Some(crate::palw_model_tier_v1::PalwModelTierFenceV1::at(ForkActivation::new(FENCE)));
    p.palw_probabilistic_constraints_v1 = Some(ForkActivation::new(FENCE));
    p.sync_palw_rfc0018_v1();
    p
}

const RFC18_DIGESTS: [[u8; 32]; 8] = [[1; 32], [2; 32], [3; 32], [4; 32], [5; 32], [6; 32], [7; 32], [8; 32]];

fn record_for(p: &Params, fence: PalwRfc0018GatedFenceV1) -> PalwRfc0018ArmingRecordV1 {
    PalwRfc0018ArmingRecordV1 {
        version: PALW_RFC0018_ARMING_RECORD_VERSION_V1,
        fence,
        genesis: p.genesis.hash,
        activation_daa: FENCE,
        source_commit: [0xAB; 20],
        evidence: &RFC18_DIGESTS[..fence.requirements().len()],
        review: [0xEE; 32],
    }
}

#[test]
fn gatec_decouple_gate_c_validates_without_model_eligibility_past_the_fence() {
    let p = gate_c_ready();
    assert_eq!(p.palw_model_eligibility_v1, None, "RFC-0017's exam fence is not set");
    // With no pinned record it is still refused, by the record gate and nothing else.
    assert!(matches!(p.validate_palw_rfc0018_v1(),
        Err(PalwModeV2Error::Invalid(why)) if why == PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1.no_record_refusal()));
    let rec = [record_for(&p, PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1)];
    p.validate_palw_rfc0018_with_records_v1(&rec).unwrap();
    // The mirror governs Gate C past the fence: claim-level certification is required.
    let mirror = p.palw_rfc0018_expected_mirror_v1().expect("Gate C armed");
    assert!(!mirror.gate_c_governs(FENCE - 1));
    assert!(mirror.gate_c_governs(FENCE));
    assert_eq!(mirror.gate_c_rule, PalwGateCRuleV1::SecurityCertifiedV1);
}

#[test]
fn gatec_decouple_gate_c_needs_the_tier_fence_and_aux_136_and_a_set_eligibility_fence_below_it() {
    let mut p = gate_c_ready();
    let rec = [record_for(&p, PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1)];
    p.palw_model_tier_v1 = None;
    assert!(matches!(p.validate_palw_rfc0018_with_records_v1(&rec), Err(PalwModeV2Error::Invalid(why)) if why.contains("palw_model_tier_v1")));
    let mut p = gate_c_ready();
    p.palw_model_tier_v1 = Some(crate::palw_model_tier_v1::PalwModelTierFenceV1::at(ForkActivation::new(FENCE + 1)));
    assert!(matches!(p.validate_palw_rfc0018_with_records_v1(&rec), Err(PalwModeV2Error::Invalid(why)) if why.contains("palw_model_tier_v1")));
    let mut p = gate_c_ready();
    p.palw_probabilistic_constraints_v1 = None;
    assert!(matches!(p.validate_palw_rfc0018_with_records_v1(&rec), Err(PalwModeV2Error::Invalid(why)) if why.contains("aux 136")));
    // The old dependency is kept where the eligibility fence is set: above Gate C it is refused...
    let mut p = gate_c_ready();
    p.palw_model_eligibility_v1 = Some(ForkActivation::new(FENCE + 1));
    assert!(matches!(p.validate_palw_rfc0018_with_records_v1(&rec), Err(PalwModeV2Error::Invalid(why)) if why.contains("palw_model_eligibility_v1")));
    // ...at or below it this check passes (RFC-0017's own validation still refuses arming it).
    let mut p = gate_c_ready();
    p.palw_model_eligibility_v1 = Some(ForkActivation::new(FENCE));
    p.validate_palw_rfc0018_with_records_v1(&rec).unwrap();
    assert!(p.validate_palw_model_eligibility_v1().is_err(), "RFC-0017's exam can never be armed");
    // Some(never()) eligibility is absence.
    let mut p = gate_c_ready();
    p.palw_model_eligibility_v1 = Some(ForkActivation::never());
    p.validate_palw_rfc0018_with_records_v1(&rec).unwrap();
}

#[test]
fn gatec_decouple_rfc18_record_gate_is_exact_and_well_formed() {
    let p = gate_c_ready();
    let good = record_for(&p, PalwRfc0018GatedFenceV1::IndependentlyWorkCertifiedV1);
    let refused = |r: PalwRfc0018ArmingRecordV1| p.validate_palw_rfc0018_with_records_v1(&[r]).is_err();
    assert!(!refused(good));
    assert!(refused(PalwRfc0018ArmingRecordV1 { activation_daa: FENCE + 1, ..good }), "another height");
    assert!(refused(PalwRfc0018ArmingRecordV1 { genesis: Hash64::from_u64_word(9), ..good }), "another network");
    assert!(refused(PalwRfc0018ArmingRecordV1 { fence: PalwRfc0018GatedFenceV1::DagWidthScheduleV1, ..good }), "another fence");
    assert!(refused(PalwRfc0018ArmingRecordV1 { version: 2, ..good }));
    assert!(refused(PalwRfc0018ArmingRecordV1 { source_commit: [0; 20], ..good }));
    assert!(refused(PalwRfc0018ArmingRecordV1 { review: [0; 32], ..good }));
    assert!(refused(PalwRfc0018ArmingRecordV1 { evidence: &RFC18_DIGESTS[..1], ..good }));
    const DUP: [[u8; 32]; 6] = [[1; 32], [1; 32], [3; 32], [4; 32], [5; 32], [6; 32]];
    assert!(refused(PalwRfc0018ArmingRecordV1 { evidence: &DUP, ..good }));
    // The binary's list is empty: nothing is armable by this lane.
    assert!(PALW_RFC0018_ARMING_RECORDS_V1.iter().all(|r| r.activation_daa == 17_400 && r.genesis == crate::config::genesis::PALW_T12_GENESIS.hash));
    for f in PalwRfc0018GatedFenceV1::ALL {
        assert!(NAMES.contains(&f.name()));
        assert!(f.requirements().len() >= 2);
    }
}

#[test]
fn gatec_decouple_shipped_presets_identity_and_validation_unchanged() {
    for p in [MAINNET_PARAMS, TESTNET_PARAMS, palw_t12_shipped_params()] {
        for f in PalwRfc0018GatedFenceV1::ALL {
            assert_eq!(f.armed(&p), None);
        }
        p.validate_palw_rfc0018_v1().unwrap();
        p.validate_palw_rfc0018_with_records_v1(&[]).unwrap();
    }
}
