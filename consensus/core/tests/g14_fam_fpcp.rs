//! **G14 family lane FPCP** (`FpConstraint`, `FpPrefix`; 2026-10-26): the family fence list, the mirror that arms the families, the
//! one-claim-spans-several-families rule, the record requirements, and V9 (the staged court config covers a clock-v2 dispute).
//!
//! Run: `cargo test -p kaspa-consensus-core --test g14_fam_fpcp`

#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1, PALW_T12_FULL_ACTIVATION_FENCES_V1, PalwPostLaunchFenceV1, Params,
    palw_t12_arm_after_int13_at_v1, palw_t12_release_v7_params,
};
use kaspa_consensus_core::palw_court_clock_v2::palw_court_clock_v2_required_retention_with;
use kaspa_consensus_core::palw_g14_gates_v1::{
    PALW_FP_CONSTRAINT_PREFIX_ROUTE_EVIDENCE_V1, PalwG14GatedFenceV1, PalwG14RouteFamilyV1 as F, palw_g14_route_family_fences_v1,
};
use kaspa_consensus_core::palw_rendering_resources_v1::PalwStagedPublicCourtConfigV1;

const AT: u64 = 17_000;

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn names(list: &[PalwPostLaunchFenceV1]) -> Vec<&'static str> {
    list.iter().map(|f| f.name).collect()
}

/// Every fence a constraint / prefix job needs (its own family, plus selection + tokenizer, which every FP job needs) is on the list;
/// the list keeps the full list's order (so each entry's prerequisites stay earlier).
#[test]
fn fpcp_list_covers_every_family_a_constraint_or_prefix_job_needs() {
    let list = names(PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1);
    // `palw_fp_tokenizer_match` is already in force on testnet-12 from DAA 5,300. Re-listing it here would move that live fence.
    let release = palw_t12_release_v7_params();
    let already_live = |need: &str| {
        need == "palw_fp_tokenizer_match"
            && release.palw_fp_tokenizer_match.is_some_and(|fence| fence.is_active(AT.saturating_sub(1)))
    };
    for fam in [F::FpConstraint, F::FpPrefix, F::FpSelection, F::FpTokenizer] {
        for need in palw_g14_route_family_fences_v1(fam) {
            assert!(list.contains(need) || already_live(need), "{fam:?}: {need} is not on the FPCP list");
        }
    }
    let full = names(PALW_T12_FULL_ACTIVATION_FENCES_V1);
    let mut at = 0;
    for n in &list {
        let i = full[at..].iter().position(|f| f == n).unwrap_or_else(|| panic!("{n}: not in the full list's order"));
        at += i + 1;
    }
    assert!(!list.contains(&"palw_legacy_held_da_v2"), "no FP family reads LG14-B");
}

/// Armed at 17,000 (after int-13), the mirror arms the constraint and prefix families only from 17,000; a constraint job's reward
/// returns only when selection and tokenizer are armed too and the commitment is framed. Dormant again: every id is the fleet's.
#[test]
fn fpcp_list_arms_constraint_and_prefix_from_its_height_only() {
    let base = palw_t12_release_v7_params();
    assert!(base.palw_g14_route_required_v1_mirror().is_none());
    let mut p = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1, Some(AT));
    let m = p.palw_g14_route_required_v1_mirror().expect("route-required is on the list");
    for fam in [F::FpConstraint, F::FpPrefix, F::FpTokenizer] {
        assert!(!m.route_armed(fam, AT - 1, true), "{fam:?} below the height");
        assert!(m.route_armed(fam, AT, false), "{fam:?} at the height");
    }
    assert!(m.route_armed(F::FpSelection, AT, true), "framed selection: the TIR public frame resolves");
    assert!(!m.route_armed(F::FpSelection, AT, false), "unframed: no selection route");
    assert!(!m.route_armed(F::LegacyPanelAttempt, AT, false), "LG14-B is not on this list");
    assert!(!m.route_armed(F::KernelRoutePanelOne, AT, false));
    assert!(!m.route_armed(F::NonPublicEvidence, AT, true));
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_FP_CONSTRAINT_PREFIX_FENCES_V1, None);
    assert_eq!(ids(&p), ids(&base), "dormant: byte-identical ids");
}

/// The staged court record (keyed by fence) is a V1–V7 record that needs measured values; the family evidence names V2–V7 cases.
#[test]
fn fpcp_record_requirements_are_the_staged_court_validation_steps() {
    let f = PalwG14GatedFenceV1::StagedPublicCourtV1;
    assert_eq!(f.name(), "palw_staged_public_court_v1");
    assert_eq!(f.requirements().len(), 7);
    assert!(f.needs_measured_value());
    assert!(PalwG14GatedFenceV1::LegacyPublicFilerV1.needs_measured_value());
    assert_eq!(PALW_FP_CONSTRAINT_PREFIX_ROUTE_EVIDENCE_V1.len(), 5);
    assert!(PALW_FP_CONSTRAINT_PREFIX_ROUTE_EVIDENCE_V1.iter().any(|e| e.contains("DA default")));
}

/// V9: the staged court config's material horizon (acquisition + collection) covers `a + 10,000 + max(W_disclose, reorg_retention)`
/// for testnet-12's W_disclose = 1,200, so a clock-v2 dispute extension of a staged claim is not refused as a short Custody dependency.
#[test]
fn v9_staged_court_retention_covers_the_clock_v2_dispute() {
    let c = PalwStagedPublicCourtConfigV1::network_v1();
    c.check().expect("well-formed");
    assert!(c.reorg_retention_daa >= 360, "at least testnet-12's finality depth");
    for a in [0u64, 17_000, 1_000_000] {
        let need = palw_court_clock_v2_required_retention_with(a, 1_200, c.reorg_retention_daa).expect("no overflow");
        let have = a + c.acquisition_daa + c.collection_daa;
        assert!(have >= need, "a={a}: {have} < {need}");
    }
}
