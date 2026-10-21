//! Contract component tests, not T01–T15 node acceptance evidence.
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_court_policy_v1::*;
use kaspa_consensus_core::palw_g14_gates_v1::*;

#[test]
fn dependency_horizon_boundary_and_shared_bond_liability() {
    // Neither a short followup nor a short retention may hide the longer
    // obligation. Other claims' liabilities keep the bond, not this path.
    for (retention, followup, required) in [(50, 20, 10_150), (20, 50, 10_150), (50, 50, 10_150), (0, 0, 10_100)] {
        for delta in [-1_i64, 0, 1] {
            let result = CourtClockBounds::derive(100, retention, followup, (required + delta) as u64, 20_000);
            if delta < 0 {
                assert_eq!(result, Err(CourtPolicyError::DependencyTooShort));
            } else {
                assert_eq!(
                    result.unwrap(),
                    CourtClockBounds {
                        notice_boundary: 3_220,
                        hard_deadline: 10_100,
                        material_until: 10_100 + retention,
                        bond_exit_after: 20_000,
                    }
                );
            }
        }
    }
}

#[test]
fn clock_overflow_never_clamps_or_wraps() {
    for (a, retention, followup) in [(u64::MAX, 0, 0), (u64::MAX - 9_999, 0, 0), (u64::MAX - 10_000, 1, 0), (u64::MAX - 10_000, 0, 1)]
    {
        assert_eq!(CourtClockBounds::derive(a, retention, followup, u64::MAX, 0), Err(CourtPolicyError::Overflow));
    }
}

#[test]
fn readiness_record_cannot_bypass_unfinished_outsider_path() {
    for fence in [PalwG14GatedFenceV1::TirPublicConvictionV1, PalwG14GatedFenceV1::LegacyPublicFilerV1] {
        let mut p = palw_t12_shipped_params();
        p.validate_palw_court_policy_completeness_v1(fence).unwrap();
        match fence {
            PalwG14GatedFenceV1::TirPublicConvictionV1 => p.palw_tir_public_conviction_v1 = Some(ForkActivation::new(20_000)),
            _ => p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(20_000)),
        }
        let evidence: Vec<[u8; 32]> = (1..=fence.requirements().len()).map(|i| [i as u8; 32]).collect();
        let record = PalwG14ArmingRecordV1 {
            version: PALW_G14_ARMING_RECORD_VERSION_V1,
            fence,
            genesis: p.genesis.hash,
            activation_daa: 20_000,
            source_commit: [1; 20],
            evidence: Box::leak(evidence.into_boxed_slice()),
            review: [255; 32],
        };
        palw_g14_arming_record_form_v1(&record).unwrap();
        let error = p.validate_palw_g14_fence_record_in_v1(fence, &[record]).unwrap_err();
        assert!(format!("{error:?}").contains("OneOutsiderCapacityIncomplete"));
        assert!(format!("{error:?}").contains("ServiceOpportunityIncomplete"));
    }
}

#[test]
fn shared_initial_notice_includes_original_terminal_daa_without_hard_overflow() {
    for a in [0, 100, u64::MAX - DISPUTE_WINDOW_DAA] {
        if let Some(before) = a.checked_sub(1) {
            assert_eq!(shared_initial_notice_daa_in_window_v1(a, before), Ok(false));
        }
        for offset in [0, NOTICE_WINDOW_DAA - 1, NOTICE_WINDOW_DAA] {
            assert_eq!(shared_initial_notice_daa_in_window_v1(a, a + offset), Ok(true));
        }
        for offset in [NOTICE_WINDOW_DAA + 1, DISPUTE_WINDOW_DAA] {
            assert_eq!(shared_initial_notice_daa_in_window_v1(a, a + offset), Ok(false));
        }
    }
    for a in [u64::MAX - DISPUTE_WINDOW_DAA + 1, u64::MAX - NOTICE_WINDOW_DAA, u64::MAX] {
        assert_eq!(shared_initial_notice_daa_in_window_v1(a, a), Err(CourtPolicyError::Overflow));
    }
    // A late retry is still measured from the original, never from that retry.
    assert_eq!(shared_initial_notice_daa_in_window_v1(100, 3_221), Ok(false));
}
