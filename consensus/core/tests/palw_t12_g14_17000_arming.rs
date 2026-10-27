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
        // settlement once is taken off the 17,000 list (RFC08-2 open blocker; ACT2 recommendation (ii)).
        assert!(names.contains(&f.name) != (f.name == "palw_settlement_once_v1"), "slice entry {}", f.name);
    }
    let shipped = palw_t12_shipped_params();
    let live: Vec<&str> =
        shipped.palw_fences_v1().iter().filter(|(_, f)| f.is_some_and(|f| f.daa_score() < 17_000)).map(|(n, _)| *n).collect();
    // Each family fence is either on the list or already in force on testnet-12 below 17,000 (`palw_fp_tokenizer_match` @5,300,
    // `palw_panel_da` @0) — never both (re-arming a live fence would fork below 17,000).
    for fam in FAMILIES.into_iter().chain([F::FpPanelDaDisclosure]) {
        for n in palw_g14_route_family_fences_v1(fam) {
            assert!(names.contains(n) != live.contains(n), "{fam:?} needs {n} (listed {}, live {})", names.contains(n), live.contains(n));
        }
    }
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

/// The whole list at 17,000, peeled: validate; on a refusal, print it, set the first list fence the message names dormant, repeat.
/// The final line says whether the WHOLE list validates (`ACT2PEEL whole=Ok`).
#[test]
fn peel_the_whole_list_at_17000() {
    let at = PALW_T12_G14_17000_DAA_V1;
    let mut p = palw_t12_shipped_params();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, Some(at));
    println!("ACT2PEEL whole={:?}", p.validate_palw_v2());
    for _ in 0..PALW_T12_G14_17000_FENCES_V1.len() {
        let Err(e) = p.validate_palw_v2() else { break };
        let msg = format!("{e:?}");
        let named = PALW_T12_G14_17000_FENCES_V1
            .iter()
            .filter(|f| !palw_t12_fence_held_by_int13_v1(f))
            .filter_map(|f| msg.find(f.name).map(|i| (i, f)))
            .min_by_key(|(i, _)| *i);
        let Some((_, f)) = named else {
            println!("ACT2PEEL unattributed :: {msg}");
            break;
        };
        println!("ACT2PEEL drop {} :: {msg}", f.name);
        (f.set)(&mut p, None);
    }
    println!("ACT2PEEL rest={:?}", p.validate_palw_v2());
}

// ---------------------------------------------------------------------------------------------------------------------------------
// Typed waivers (lane ACT2, user decision 2026-10-27)
// ---------------------------------------------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_arming_waiver_v1::{
    PALW_ARMING_WAIVER_REVIEW_V1, PALW_ARMING_WAIVERS_V1, PalwArmingEvidenceV1, palw_arming_evidence_kind_v1,
};

/// Every slot of every pinned 17,000 record is a waiver (the Python digests match the Rust ones); none is reported as a run.
#[test]
fn every_pinned_17000_slot_is_a_typed_waiver_never_a_run() {
    use kaspa_consensus_core::palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1;
    use kaspa_consensus_core::palw_kernel_route_arming_v1::PALW_KERNEL_ROUTE_ARMING_RECORDS_V1;
    use kaspa_consensus_core::palw_rfc0018_v1::PALW_RFC0018_ARMING_RECORDS_V1;
    type Slots = (&'static str, kaspa_consensus_core::Hash64, u64, Vec<[u8; 32]>, [u8; 32]);
    let mut slots: Vec<Slots> = vec![];
    slots.extend(PALW_G14_ARMING_RECORDS_V1.iter().map(|r| (r.fence.name(), r.genesis, r.activation_daa, r.evidence.to_vec(), r.review)));
    slots.extend(
        PALW_KERNEL_ROUTE_ARMING_RECORDS_V1.iter().map(|r| (r.fence.name(), r.genesis, r.activation_daa, r.evidence.to_vec(), r.review)),
    );
    slots.extend(PALW_RFC0018_ARMING_RECORDS_V1.iter().map(|r| (r.fence.name(), r.genesis, r.activation_daa, r.evidence.to_vec(), r.review)));
    assert_eq!(slots.len(), 11);
    for (fence, g, h, ev, review) in slots {
        for (i, d) in ev.iter().enumerate() {
            let kind = palw_arming_evidence_kind_v1(fence, g, h, i as u16, d, PALW_ARMING_WAIVERS_V1).expect("well-formed");
            assert!(matches!(kind, PalwArmingEvidenceV1::Waived(_)), "{fence} slot {i} is not a pinned waiver");
        }
        let kind = palw_arming_evidence_kind_v1(fence, g, h, PALW_ARMING_WAIVER_REVIEW_V1, &review, PALW_ARMING_WAIVERS_V1).unwrap();
        assert!(matches!(kind, PalwArmingEvidenceV1::Waived(_)), "{fence} review is not a pinned waiver");
    }
    assert!(PALW_ARMING_WAIVERS_V1.iter().all(|w| w.decided_by == "user 2026-10-27" && w.form().is_ok()));
}

/// **A waiver admits its record AND lifts no safety refusal — at the same time.**
#[test]
fn a_waiver_admits_its_record_but_lifts_no_safety_refusal() {
    use kaspa_consensus_core::palw_g14_gates_v1::{
        PALW_G14_ARMING_RECORDS_V1, PalwG14ArmingRecordV1, PalwG14GatedFenceV1, palw_g14_arming_record_admits_v1,
        palw_g14_arming_record_form_v1,
    };
    let at = PALW_T12_G14_17000_DAA_V1;
    let base = palw_t12_shipped_params();
    let g = base.genesis.hash;
    let set = |p: &mut kaspa_consensus_core::config::params::Params, name: &str| {
        let f = PALW_T12_G14_17000_FENCES_V1.iter().find(|f| f.name == name).expect("on the list");
        (f.set)(p, Some(ForkActivation::new(at)));
    };
    // 1. The waived record admits exactly its own (fence, network, height).
    for fence in [PalwG14GatedFenceV1::LegacyPublicFilerV1, PalwG14GatedFenceV1::LegacyHeldDaV2, PalwG14GatedFenceV1::G14RouteRequiredV1] {
        assert_eq!(palw_g14_arming_record_admits_v1(fence, g, at, PALW_G14_ARMING_RECORDS_V1), Ok(()), "{fence:?}");
        assert!(palw_g14_arming_record_admits_v1(fence, g, at + 1, PALW_G14_ARMING_RECORDS_V1).is_err());
    }
    // 2. A waiver moved to another slot is refused.
    let r = PALW_G14_ARMING_RECORDS_V1.iter().find(|r| r.fence == PalwG14GatedFenceV1::G14RouteRequiredV1).unwrap();
    let swapped: &'static [[u8; 32]] = Box::leak(vec![r.evidence[1], r.evidence[0], r.evidence[2], r.evidence[3]].into_boxed_slice());
    assert!(palw_g14_arming_record_form_v1(&PalwG14ArmingRecordV1 { evidence: swapped, ..*r }).is_err());
    // 3. Ordering: held DA v2 has its waived record, but without the filer at or below it the refusal stands.
    let mut p = base.clone();
    set(&mut p, "palw_legacy_held_da_v2");
    let e = format!("{:?}", p.validate_palw_v2().unwrap_err());
    assert!(e.contains("needs palw_legacy_public_filer_v1"), "{e}");
    // 4. Missing value: court capacity has its waived record (and a UserDecision for the approved estimate), but the PLACEHOLDER
    //    policy (the full list's entry, not the estimate) is still refused.
    let mut p = base.clone();
    for name in ["palw_legacy_public_filer_v1", "palw_court_outsider_lane_v1"] {
        set(&mut p, name);
    }
    (kaspa_consensus_core::config::params::PALW_T12_FULL_COURT_CAPACITY_ENTRY.set)(&mut p, Some(ForkActivation::new(at)));
    let e = format!("{:?}", p.validate_palw_v2().unwrap_err());
    assert!(e.contains("palw_court_capacity_v1") && !e.contains("no arm-readiness record"), "{e}");
    // 5. Open blocker: settlement once with its prerequisites and records is still refused by RFC08-2.
    let mut p = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(at));
    let e = format!("{:?}", p.validate_palw_v2().unwrap_err());
    assert!(e.contains("open blocker") && e.contains("RFC08-2"), "{e}");
}
