//! Tests the real legacy verdict boundary; these clocks are not a native P
//! subject or an authority to shorten an unresolved fraud branch.
use kaspa_consensus_core::{
    config::params::TESTNET_PARAMS,
    palw_model_eligibility_v1::{PalwEligibilityAttestationRowV1, PalwEligibilityVerdictV1},
    palw_positive_assignment_certification_v2::*,
    palw_security_certification_v1::*,
};

fn policy() -> PalwSecurityCertificationPolicyV1 {
    PalwSecurityCertificationPolicyV1 {
        m: 1,
        k: 1,
        veto: 0,
        anchor_delay_spans: 2,
        attest_window_daa: 5,
        prosecution_window_daa: 19,
        certify_by_daa: 7,
        verifier_maturity_daa: 3,
        verifier_min_collateral_sompi: 1,
        draws_per_epoch: 4,
        epoch_daa: 11,
    }
}
fn row() -> PalwClaimCertificationRowV1 {
    PalwClaimCertificationRowV1 {
        claim: [1; 64],
        class_id: [2; 64],
        producer_bond: [3; 64],
        producer_principal: [4; 64],
        attempt: true,
        final_daa: 100,
        committed_daa: 100,
        committed_span: 2,
        anchor: None,
        draws: vec![],
        drawn_daa: 0,
        deadline_daa: 0,
        attestations: vec![],
        held_reward: 0,
        payout_payload: [0; 64],
        outcome: None,
    }
}
#[test]
fn clocks_preserve_actual_inclusive_draw_and_exclusive_failure_boundaries() {
    let policy = policy();
    let clock = AssignmentGateCClockV2::from_existing_policy(&policy, 100).unwrap();
    assert_eq!((clock.latest_draw_daa, clock.no_draw_expiry_daa), (107, 108));
    assert_eq!((clock.latest_attestation_deadline_daa, clock.latest_failed_attestation_decision_daa), (112, 113));
    assert_eq!(clock.prosecution_not_before_daa(100).unwrap(), 119);
    let facts = PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: false };
    let mut row = row();
    assert_eq!(palw_certification_verdict_v1(&policy, &row, facts, 107), None);
    assert_eq!(palw_certification_verdict_v1(&policy, &row, facts, 108), Some(PalwCertificationOutcomeV1::Expired));
    row.anchor = Some((4, [6; 64]));
    row.drawn_daa = 107;
    row.deadline_daa = 112;
    assert_eq!(palw_certification_verdict_v1(&policy, &row, facts, 112), None);
    assert_eq!(palw_certification_verdict_v1(&policy, &row, facts, 113), Some(PalwCertificationOutcomeV1::NotCertified));
}
#[test]
fn sufficient_approval_does_not_bound_pending_fraud_or_create_a_reservation() {
    let policy = policy();
    let clock = AssignmentGateCClockV2::from_existing_policy(&policy, 100).unwrap();
    let mut row = row();
    row.anchor = Some((4, [6; 64]));
    row.draws.push([7; 64]);
    row.deadline_daa = 112;
    row.attestations.push(PalwEligibilityAttestationRowV1 {
        verifier: [7; 64],
        verdict: PalwEligibilityVerdictV1::Reproduced,
        transcript: [8; 64],
        attested_daa: 110,
    });
    let pending = PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: true };
    for now in [119, 120, 100_000, u64::MAX] {
        assert_eq!(palw_certification_verdict_v1(&policy, &row, pending, now), None);
    }
    assert_eq!(
        palw_certification_verdict_v1(&policy, &row, PalwCertificationClaimFactsV1 { accusation_pending: false, ..pending }, 119),
        Some(PalwCertificationOutcomeV1::Certified)
    );
    assert_eq!(clock.require_complete_native_bound(), Err(AssignmentGateCClockErrorV2::UnresolvedPendingFraudOwner));
}
#[test]
fn missing_actual_policy_and_arithmetic_overflow_never_turn_into_shorter_ttl() {
    assert_eq!(AssignmentGateCClockV2::resolve(&TESTNET_PARAMS, 100, 100), Err(AssignmentGateCClockErrorV2::MissingActualPolicy));
    assert_eq!(AssignmentGateCClockV2::from_existing_policy(&policy(), u64::MAX - 10), Err(AssignmentGateCClockErrorV2::Overflow));
    let mut bad = policy();
    bad.k = 0;
    assert_eq!(AssignmentGateCClockV2::from_existing_policy(&bad, 100), Err(AssignmentGateCClockErrorV2::InvalidPolicy));
}

#[test]
fn delayed_final_delays_actual_prosecution_without_moving_result_commit_or_draw() {
    let clock = AssignmentGateCClockV2::from_existing_policy(&policy(), 100).unwrap();
    assert_eq!(clock.prosecution_not_before_daa(150), Ok(169));
    assert_eq!(clock.latest_draw_daa, 107);
    assert_eq!(clock.prosecution_not_before_daa(99), Err(AssignmentGateCClockErrorV2::InvalidClockOrder));
    assert_eq!(clock.prosecution_not_before_daa(u64::MAX - 18), Err(AssignmentGateCClockErrorV2::Overflow));
    let mut row = row();
    row.final_daa = 150;
    row.anchor = Some((4, [6; 64]));
    row.draws.push([7; 64]);
    row.attestations.push(PalwEligibilityAttestationRowV1 {
        verifier: [7; 64],
        verdict: PalwEligibilityVerdictV1::Reproduced,
        transcript: [8; 64],
        attested_daa: 110,
    });
    let facts = PalwCertificationClaimFactsV1 { still_final: true, accusation_pending: false };
    assert_eq!(palw_certification_verdict_v1(&policy(), &row, facts, 119), None);
    assert_eq!(palw_certification_verdict_v1(&policy(), &row, facts, 168), None);
    assert_eq!(palw_certification_verdict_v1(&policy(), &row, facts, 169), Some(PalwCertificationOutcomeV1::Certified));
}

#[test]
fn fixed_anchor_registration_bound_covers_every_future_commit_without_freezing_pool() {
    for maturity in 0..=9 {
        let mut p = policy();
        p.verifier_maturity_daa = maturity;
        for anchor in 0..=9 {
            for before in 1..=19 {
                let bound = assignment_verifier_registration_cutoff_v2(&p, anchor, before);
                if before <= anchor {
                    assert_eq!(bound, Err(AssignmentGateCClockErrorV2::InvalidClockOrder));
                    continue;
                }
                let oracle = (0..=32)
                    .filter(|reg| {
                        (anchor..before).any(|commit| palw_certification_pool_admits_v1(&p, commit, &[1; 64], true, *reg, 1, &[2; 64]))
                    })
                    .max();
                if oracle.is_some_and(|n| n >= anchor) {
                    assert_eq!(bound, Err(AssignmentGateCClockErrorV2::UnboundedVerifierUniverse));
                } else {
                    assert_eq!(bound.unwrap(), oracle);
                }
            }
        }
    }
    // A currently ineligible old bond must remain in the resource universe:
    // future active status/collateral are assessed by the actual draw owner.
    let p = policy();
    assert!(!palw_certification_pool_admits_v1(&p, 10, &[1; 64], false, 7, 0, &[2; 64]));
    assert!(palw_certification_pool_admits_v1(&p, 10, &[1; 64], true, 7, 1, &[2; 64]));
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, 9, 11), Ok(Some(7)));
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, 9, 13), Err(AssignmentGateCClockErrorV2::UnboundedVerifierUniverse));
}
#[test]
fn verifier_universe_cutoff_preserves_exclusive_end_and_never_wraps_maturity() {
    let mut p = policy();
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, 0, 1), Ok(None));
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, 9, 0), Err(AssignmentGateCClockErrorV2::InvalidClockOrder));
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, 9, u64::MAX), Err(AssignmentGateCClockErrorV2::InvalidClockOrder));
    p.verifier_maturity_daa = u64::MAX;
    assert_eq!(assignment_verifier_registration_cutoff_v2(&p, u64::MAX - 2, u64::MAX - 1), Ok(None));
    assert!(!palw_certification_pool_admits_v1(&p, u64::MAX - 2, &[1; 64], true, u64::MAX - 3, 1, &[2; 64]));
}
