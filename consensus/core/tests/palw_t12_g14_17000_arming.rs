//! Lane ACT2 (user decision 2026-10-27): the DAA-17,000 list — Panel=1 kernel slice + the six G14 route families — on the shipped
//! testnet-12 ruleset. Diagnostic: lists every refusal of the node's own validation (`validate_palw_v2`, the binary's pinned records).
//! Nothing is waived here and nothing is skipped: a refusal is printed as it is.
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_G14_17000_DAA_V1, PALW_T12_G14_17000_FENCES_V1, PALW_T12_KERNEL_SLICE_FENCES_V1, palw_t12_arm_after_int13_at_v1,
    palw_t12_fence_held_by_int13_v1, palw_t12_shipped_params,
};
use kaspa_consensus_core::palw_g14_gates_v1::{PalwG14RouteFamilyV1 as F, palw_g14_route_family_fences_v1};

const FAMILIES: [F; 7] =
    [F::KernelRoutePanelOne, F::LegacyPanelAttempt, F::LegacyFusedLeaf, F::FpSelection, F::FpConstraint, F::FpPrefix, F::FpTokenizer];

#[test]
fn the_17000_list_covers_the_slice_six_families_and_panel_da() {
    let names: Vec<&str> = PALW_T12_G14_17000_FENCES_V1.iter().map(|f| f.name).collect();
    for f in PALW_T12_KERNEL_SLICE_FENCES_V1 {
        assert!(names.contains(&f.name), "slice entry {} missing", f.name);
    }
    for fam in FAMILIES {
        for n in palw_g14_route_family_fences_v1(fam) {
            assert!(names.contains(n), "{fam:?} needs {n}");
        }
    }
    for n in palw_g14_route_family_fences_v1(F::FpPanelDaDisclosure) {
        assert!(names.contains(n) || *n == "palw_panel_da", "PanelDa needs {n}");
    }
    assert!(palw_t12_shipped_params().palw_panel_da.is_some(), "palw_panel_da is in force on testnet-12");
    assert!(!names.contains(&"palw_claim_challenge_v2"), "RFC07 probe is not on the list");
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), names.len(), "duplicate entry");
}

/// Every refusal, one fence at a time in list order: a refused fence is left unarmed, so each later fence is judged against the
/// prefix that validated (a fence refused only because its prerequisite was refused says so in its own message).
#[test]
fn list_every_refusal_at_17000() {
    let at = PALW_T12_G14_17000_DAA_V1;
    let base = palw_t12_shipped_params();
    base.validate_palw_v2().expect("the shipped ruleset validates");
    let mut whole = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut whole, PALW_T12_G14_17000_FENCES_V1, Some(at));
    println!("ACT2WHOLE {:?}", whole.validate_palw_v2());
    let mut p = base.clone();
    let mut refused = 0;
    for f in PALW_T12_G14_17000_FENCES_V1 {
        if palw_t12_fence_held_by_int13_v1(f) {
            println!("ACT2HELD {} (int-13, 15,500)", f.name);
            continue;
        }
        let mut q = p.clone();
        (f.set)(&mut q, Some(ForkActivation::new(at)));
        match q.validate_palw_v2() {
            Ok(()) => {
                println!("ACT2OK {}", f.name);
                p = q;
            }
            Err(e) => {
                refused += 1;
                println!("ACT2REFUSE {} :: {e:?}", f.name);
            }
        }
    }
    println!("ACT2COUNT refused={refused} of {}", PALW_T12_G14_17000_FENCES_V1.len());
}
