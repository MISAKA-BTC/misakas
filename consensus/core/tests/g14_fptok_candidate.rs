//! **Lane FPTOK (2026-10-27): FpTokenizer's fences as a 17,000 candidate (coordinator 2026-10-27; was 19,000) — tests only.** The real arming lists are untouched; this
//! file builds a candidate list (the Panel=1 kernel slice followed by the FpTokenizer family's fences in their activation order:
//! `palw_fp_tokenizer_match` → LG14-A `palw_legacy_public_filer_v1` → `palw_staged_public_court_v1`, with F03's
//! `palw_court_outsider_lane_v1` at the same height) and shows that on testnet-12 it is refused without records and validates with a
//! TEST-ONLY record for every gated fence, so records are all that stand between it and arming.

#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    PALW_T12_FULL_COURT_OUTSIDER_LANE_ENTRY, PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY, PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY,
    PALW_T12_KERNEL_SLICE_FENCES_V1, PalwPostLaunchFenceV1, Params, palw_t12_arm_after_int13_at_v1, palw_t12_release_v7_params,
};
use kaspa_consensus_core::palw_fp_tokenizer_v1::PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY;
use kaspa_consensus_core::palw_g14_gates_v1::{
    PALW_G14_ARMING_RECORD_VERSION_V1, PalwG14ArmingRecordV1, PalwG14GatedFenceV1, PalwG14RouteFamilyV1 as F,
    palw_g14_route_family_fences_v1,
};
use kaspa_consensus_core::palw_kernel_route_arming_v1::{
    PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1, PalwKernelRouteArmingRecordV1, PalwKernelRouteGatedFenceV1,
};

const CANDIDATE_DAA: u64 = 17_000;

/// FpTokenizer's own entries, in activation order (the binding first, the staged court last; F03's outsider lane beside the court).
const FPTOK_ENTRIES: [PalwPostLaunchFenceV1; 4] = [
    PALW_DRILL_FP_TOKENIZER_MATCH_ENTRY,
    PALW_T12_FULL_LEGACY_PUBLIC_FILER_ENTRY,
    PALW_T12_FULL_COURT_OUTSIDER_LANE_ENTRY,
    PALW_T12_FULL_STAGED_PUBLIC_COURT_ENTRY,
];

fn candidate() -> Vec<PalwPostLaunchFenceV1> {
    // The kernel slice's last entry (`palw_g14_route_required_v1`) reads the route fences' heights, so FpTokenizer's go before it.
    let (route_required, head) = PALW_T12_KERNEL_SLICE_FENCES_V1.split_last().expect("non-empty slice");
    head.iter().copied().chain(FPTOK_ENTRIES).chain([*route_required]).collect()
}

fn armed() -> Params {
    let mut p = palw_t12_release_v7_params();
    palw_t12_arm_after_int13_at_v1(&mut p, &candidate(), Some(CANDIDATE_DAA));
    p.sync_palw_legacy_public_filer_v1();
    p.sync_palw_g14_gates_v1();
    p
}

/// TEST-ONLY records (fixed fill bytes; never pinned).
fn g14_records(p: &Params) -> Vec<PalwG14ArmingRecordV1> {
    PalwG14GatedFenceV1::ALL
        .into_iter()
        .filter_map(|fence| fence.armed(p).map(|at| (fence, at.daa_score())))
        .map(|(fence, at)| {
            let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x60 + i as u8; 32]).collect();
            PalwG14ArmingRecordV1 {
                version: PALW_G14_ARMING_RECORD_VERSION_V1,
                fence,
                genesis: p.genesis.hash,
                activation_daa: at,
                source_commit: [0x17; 20],
                evidence: Box::leak(evidence.into_boxed_slice()),
                review: [0xE7; 32],
            }
        })
        .collect()
}

fn kernel_records(p: &Params) -> Vec<PalwKernelRouteArmingRecordV1> {
    PalwKernelRouteGatedFenceV1::ALL
        .into_iter()
        .filter_map(|fence| fence.armed(p).map(|at| (fence, at.daa_score())))
        .map(|(fence, at)| {
            let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x70 + i as u8; 32]).collect();
            PalwKernelRouteArmingRecordV1 {
                version: PALW_KERNEL_ROUTE_ARMING_RECORD_VERSION_V1,
                fence,
                genesis: p.genesis.hash,
                activation_daa: at,
                source_commit: [0x18; 20],
                evidence: Box::leak(evidence.into_boxed_slice()),
                review: [0xE8; 32],
            }
        })
        .collect()
}

#[test]
fn fptok_candidate_covers_the_family_in_activation_order() {
    let names: Vec<&str> = candidate().iter().map(|f| f.name).collect();
    for need in palw_g14_route_family_fences_v1(F::FpTokenizer) {
        assert!(names.contains(need), "{need} is not on the candidate");
    }
    let at = |n: &str| names.iter().position(|x| *x == n).unwrap();
    assert!(at("palw_fp_tokenizer_match") < at("palw_legacy_public_filer_v1"));
    assert!(at("palw_legacy_public_filer_v1") < at("palw_staged_public_court_v1"));
    assert!(at("palw_court_outsider_lane_v1") <= at("palw_staged_public_court_v1"));
    assert_eq!(*names.last().unwrap(), "palw_g14_route_required_v1");
    let p = armed();
    let m = p.palw_g14_route_required_v1_mirror().expect("route-required is on the candidate");
    assert!(!m.route_armed(F::FpTokenizer, CANDIDATE_DAA - 1, true));
    assert!(m.route_armed(F::FpTokenizer, CANDIDATE_DAA, true), "{m:?}");
    assert!(m.route_armed(F::KernelRoutePanelOne, CANDIDATE_DAA, false));
}

#[test]
fn fptok_candidate_is_refused_without_records_and_adds_no_refusal_beyond_the_slice() {
    let p = armed();
    let why = p.validate_palw_v2().expect_err("nothing is pinned: refused");
    eprintln!("[FPTOK candidate] without records: {why:?}");
    let g = g14_records(&p);
    let k = kernel_records(&p);
    eprintln!("[FPTOK candidate] test-only records: g14 {} kernel {}", g.len(), k.len());
    // The slice alone at the same height, with its own test-only records: whatever still refuses it is the slice's.
    let mut slice = palw_t12_release_v7_params();
    palw_t12_arm_after_int13_at_v1(&mut slice, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(CANDIDATE_DAA));
    slice.sync_palw_g14_gates_v1();
    let slice_result = slice.validate_palw_v2_with_g14_and_kernel_route_records_v1(&g14_records(&slice), &kernel_records(&slice));
    let result = p.validate_palw_v2_with_g14_and_kernel_route_records_v1(&g, &k);
    eprintln!("[FPTOK candidate] with records: {result:?}");
    eprintln!("[FPTOK candidate] slice alone with records: {slice_result:?}");
    assert_eq!(
        result.as_ref().map_err(|e| format!("{e:?}")),
        slice_result.as_ref().map_err(|e| format!("{e:?}")),
        "FpTokenizer's fences add no refusal beyond the kernel slice's own"
    );
    // Each FPTOK record is needed (checkable only once the slice itself validates).
    if result.is_err() {
        return;
    }
    for i in 0..g.len() {
        let mut fewer = g.clone();
        let gone = fewer.remove(i);
        if !matches!(gone.fence, PalwG14GatedFenceV1::LegacyPublicFilerV1 | PalwG14GatedFenceV1::StagedPublicCourtV1) {
            continue;
        }
        let r = p.validate_palw_v2_with_g14_and_kernel_route_records_v1(&fewer, &k);
        assert!(r.is_err(), "{:?} unrecorded", gone.fence);
    }
}
