//! **Lane ARM-LEGACY (2026-10-27): the 17,000 arming (was 19,000)'s CANDIDATE list for the two legacy route families** —
//! `LegacyPanelAttempt` (LG14-A filer + LG14-B held DA v2) and `LegacyFusedLeaf` (+ court capacity), with the F03 outsider lane
//! `palw_court_outsider_lane_v1` in force at the same height.
//!
//! This is a candidate list in a test only: `PALW_T12_FULL_ACTIVATION_DAA` and the shipped activation list are NOT changed, and no
//! record is pinned in the binary. What it shows:
//!
//! * on testnet-12 as shipped, arming the candidates at DAA 17,000 passes the WHOLE validation (`validate_palw_v2_with_arming_records_v1`,
//!   no F02/F03 escape hatch) given one TEST arm-readiness record per gated fence — i.e. the only thing missing is the records
//!   (and, for court capacity, a measured `PalwCourtMeasurementV1`; the one here is a TEST measurement, never a network's record);
//! * without the records it is refused by name; without the outsider lane it is refused by F03 (`OneOutsiderCapacityIncomplete`);
//! * the route mirror then arms both families at 17,000 and neither below it;
//! * testnet-12's ids do not move (the candidates are applied to a clone).
//!
//! Run: cargo test -p kaspa-consensus-core --test g14_arm_legacy_17000

#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_G14_17000_FENCES_V1, Params, palw_t12_arm_after_int13_at_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_court_capacity_v1::*;
use kaspa_consensus_core::palw_g14_gates_v1::*;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;

/// The decision height the coordinator targets (coordinator 2026-10-27: moved from 17,000 to 17,000, freeze ~16,600). A candidate, not a schedule.
const AT: u64 = 17_000;

/// The candidate fences, in prerequisite order.
const CANDIDATES: [&str; 5] = [
    "palw_legacy_public_filer_v1",
    "palw_legacy_held_da_v2",
    "palw_court_outsider_lane_v1",
    "palw_court_capacity_v1",
    "palw_g14_route_required_v1",
];

/// A TEST court measurement (not a measurement of any node; V11's estimate lives in the value ledger, its record is the coordinator's).
fn test_measurement() -> PalwCourtMeasurementV1 {
    PalwCourtMeasurementV1 {
        version: PALW_COURT_MEASUREMENT_VERSION_V1,
        class_ns: [1_000, 42_000, 42_000, 42_000],
        class_bytes: [20_000; 4],
        class_memory: [20_000; 4],
        block_court_ns: 42_000,
        block_court_bytes: u64::MAX / 4,
        block_court_memory: u64::MAX / 4,
        rho_sompi: 1_000,
        ticket_window_daa: 0,
        block_one_move_ns: 42_000,
        block_one_move_bytes: u64::MAX / 4,
        reserve_permille: 500,
    }
}

fn record(fence: PalwG14GatedFenceV1, p: &Params, measured: Option<&PalwCourtMeasurementV1>) -> PalwG14ArmingRecordV1 {
    let mut evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x19 + i as u8; 32]).collect();
    if let Some(m) = measured {
        evidence[PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1] = m.digest();
    }
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis: p.genesis.hash,
        activation_daa: AT,
        source_commit: [0x19; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

/// testnet-12 with the candidates armed at [`AT`] (each on its `Params` field, mirrors synced), and the TEST records the gated ones need.
fn armed(outsider_lane: bool) -> (Params, Vec<PalwG14ArmingRecordV1>) {
    let mut p = palw_t12_shipped_params();
    // The candidate list is this test's ruleset. The shipped G14 arming stays on the launch params.
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, None);
    let at = Some(ForkActivation::new(AT));
    p.palw_legacy_public_filer_v1 = at;
    p.sync_palw_legacy_public_filer_v1();
    // Lane V02's full-collateral fence at or below the later of LG14-A and the lock-life fence (appendix E.2).
    if let Some(life) = p.palw_final_lock_life.filter(|f| *f != ForkActivation::never()) {
        let later = life.daa_score().max(AT);
        if p.palw_final_lock_full_collateral.is_none_or(|f| f == ForkActivation::never() || f.daa_score() > later) {
            p.palw_final_lock_full_collateral = Some(ForkActivation::new(later));
            p.sync_palw_final_lock_full_collateral();
        }
    }
    p.palw_legacy_held_da_v2 = at;
    if outsider_lane {
        p.palw_court_outsider_lane_v1 = at;
    }
    let m = test_measurement();
    p.palw_court_capacity_v1 = Some(PalwCourtCapacityFenceV1::measured_at(ForkActivation::new(AT), m).expect("a policy"));
    p.sync_palw_court_capacity_v1();
    p.palw_g14_route_required_v1 = at;
    p.sync_palw_g14_gates_v1();
    let records = vec![
        record(PalwG14GatedFenceV1::LegacyPublicFilerV1, &p, None),
        record(PalwG14GatedFenceV1::LegacyHeldDaV2, &p, None),
        record(PalwG14GatedFenceV1::CourtCapacityV1, &p, Some(&m)),
        record(PalwG14GatedFenceV1::G14RouteRequiredV1, &p, None),
    ];
    (p, records)
}

#[test]
fn arm_legacy_17000_candidates_pass_every_gate_but_the_records() {
    let shipped = palw_t12_shipped_params();
    let ids = (shipped.consensus_params_id(), shipped.consensus_identity_id(), shipped.consensus_schedule_id());
    assert!(
        PALW_G14_ARMING_RECORDS_V1
            .iter()
            .all(|r| r.activation_daa == 18_400 && r.genesis == kaspa_consensus_core::config::genesis::PALW_T12_GENESIS.hash),
        "no record is pinned in this binary"
    );

    // The candidates are exactly the two families' fences plus F03's outsider lane.
    let mut want: Vec<&str> = palw_g14_route_family_fences_v1(PalwG14RouteFamilyV1::LegacyPanelAttempt).to_vec();
    want.extend(palw_g14_route_family_fences_v1(PalwG14RouteFamilyV1::LegacyFusedLeaf));
    want.push("palw_court_outsider_lane_v1");
    want.sort();
    want.dedup();
    let mut have = CANDIDATES.to_vec();
    have.sort();
    assert_eq!(have, want);

    let (p, records) = armed(true);
    for name in CANDIDATES {
        let f = p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score());
        assert_eq!(f, Some(AT), "{name} armed at {AT}");
    }
    // Every gate passes with the records — the WHOLE validation, F02 and F03 included.
    p.validate_palw_v2_with_arming_records_v1(&records)
        .unwrap_or_else(|e| panic!("the 17,000 candidates validate with records: {e:?}"));
    // ... and only the records are missing.
    let unpinned = p.validate_palw_v2().expect_err("no record is pinned");
    assert!(format!("{unpinned:?}").contains("no arm-readiness record"), "{unpinned:?}");
    for i in 0..records.len() {
        let mut fewer = records.clone();
        fewer.remove(i);
        assert!(p.validate_palw_v2_with_arming_records_v1(&fewer).is_err(), "record {i} is load-bearing");
    }

    // The route mirror: both families armed from 17,000, neither below.
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!("testnet-12 is ConsensusV2") };
    let route = bundle.state.g14_route_required_v1().expect("option (b)'s mirror");
    for family in [PalwG14RouteFamilyV1::LegacyPanelAttempt, PalwG14RouteFamilyV1::LegacyFusedLeaf] {
        assert!(route.route_armed(family, AT, false), "{family:?} armed at {AT}");
        assert!(!route.route_armed(family, AT - 1, false), "{family:?} not armed below {AT}");
    }

    // Without the outsider lane, F03 refuses by name even with every record.
    let (no_lane, records) = armed(false);
    let e = no_lane.validate_palw_v2_with_arming_records_v1(&records).expect_err("F03 needs the outsider lane");
    assert!(format!("{e:?}").contains("OneOutsiderCapacityIncomplete"), "{e:?}");

    // testnet-12 as shipped did not move.
    let after = palw_t12_shipped_params();
    assert_eq!((after.consensus_params_id(), after.consensus_identity_id(), after.consensus_schedule_id()), ids);
}
