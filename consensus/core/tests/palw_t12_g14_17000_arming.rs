//! Lane ACT2 (user decision 2026-10-27): the DAA-17,000 list — Panel=1 kernel slice + the six G14 route families — on the shipped
//! testnet-12 ruleset. Diagnostic: lists every refusal of the node's own validation (`validate_palw_v2`, the binary's pinned records).
//! Nothing is waived here and nothing is skipped: a refusal is printed as it is.
#![allow(warnings)]
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_G14_17000_DAA_V1, PALW_T12_G14_17000_FENCES_V1, PALW_T12_KERNEL_SLICE_FENCES_V1,
    palw_t12_arm_after_int13_at_v1, palw_t12_fence_held_by_int13_v1, palw_t12_shipped_params,
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
        shipped.palw_fences_v1().iter().filter(|(_, f)| f.is_some_and(|f| f.daa_score() < 18_400)).map(|(n, _)| *n).collect();
    // Each family fence is either on the list or already in force on testnet-12 below 17,000 (`palw_fp_tokenizer_match` @5,300,
    // `palw_panel_da` @0) — never both (re-arming a live fence would fork below 17,000).
    for fam in FAMILIES.into_iter().chain([F::FpPanelDaDisclosure]) {
        for n in palw_g14_route_family_fences_v1(fam) {
            assert!(
                names.contains(n) != live.contains(n),
                "{fam:?} needs {n} (listed {}, live {})",
                names.contains(n),
                live.contains(n)
            );
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
    let base = kaspa_consensus_core::config::params::palw_t12_release_v7_params();
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

#[test]
/// **DEPLOY GATE (E2E-K, 2026-10-28): the re-armed preset passes startup validation.** The shipped testnet-12 params with the G14
/// list at 18,400 (what `PALW_T12_G14_17000_FLAG_DAY_DAA = Some(18_400)` assembles) must pass `validate_palw_v2()` — the check the
/// preset assembly and `ConfigBuilder::build` run at node startup — with the binary's PINNED records: every gated fence of the list
/// has its record (G14, kernel route, RFC-0018 / Gate C, model tier) and the TIR frame cost record names a real source commit.
/// RED until the final evidence run is pinned; the flag day is not re-armed until it is green.
fn t12_preset_rearmed_at_17400_validates_like_startup() {
    use kaspa_consensus_core::palw_g14_gates_v1::{PALW_G14_ARMING_RECORDS_V1, PalwG14GatedFenceV1};
    use kaspa_consensus_core::palw_kernel_route_arming_v1::{PALW_KERNEL_ROUTE_ARMING_RECORDS_V1, PalwKernelRouteGatedFenceV1};
    use kaspa_consensus_core::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_RECORDS_V1;
    use kaspa_consensus_core::palw_rfc0018_v1::{PALW_RFC0018_ARMING_RECORDS_V1, PalwRfc0018GatedFenceV1};
    use kaspa_consensus_core::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORDS_V1;
    let at = PALW_T12_G14_17000_DAA_V1;
    let mut p = palw_t12_shipped_params();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, Some(at));
    let names: Vec<&str> = PALW_T12_G14_17000_FENCES_V1.iter().map(|f| f.name).collect();
    let mut missing = vec![];
    for f in PalwG14GatedFenceV1::ALL.into_iter().filter(|f| names.contains(&f.name())) {
        if !PALW_G14_ARMING_RECORDS_V1.iter().any(|r| r.fence == f && r.activation_daa == at) {
            missing.push(f.name());
        }
    }
    for f in PalwKernelRouteGatedFenceV1::ALL.into_iter().filter(|f| names.contains(&f.name())) {
        if !PALW_KERNEL_ROUTE_ARMING_RECORDS_V1.iter().any(|r| r.fence == f && r.activation_daa == at) {
            missing.push(f.name());
        }
    }
    for f in PalwRfc0018GatedFenceV1::ALL.into_iter().filter(|f| names.contains(&f.name())) {
        if !PALW_RFC0018_ARMING_RECORDS_V1.iter().any(|r| r.fence == f && r.activation_daa == at) {
            missing.push(f.name());
        }
    }
    if !PALW_MODEL_TIER_ARMING_RECORDS_V1.iter().any(|r| r.activation_daa == at) {
        missing.push("palw_model_tier_v1");
    }
    if !PALW_TIR_FRAME_COST_RECORDS_V1.iter().any(|r| r.activation_daa == at && r.source_commit != [0; 20]) {
        missing.push("palw_tir_frame_cost_v1");
    }
    // The re-armed preset's fingerprint (the kit's EXPECT_FP), printed before any assertion so a red gate still reports it.
    println!("DEPLOYGATE EXPECT_FP={}", p.consensus_params_id());
    assert!(missing.is_empty(), "no pinned record (or a zero source commit) for: {missing:?}");
    assert_eq!(p.validate_palw_v2(), Ok(()), "the re-armed preset must pass startup validation");
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
// Typed waivers (lane ACT2, user decisions 2026-10-27; F1: only the user-permitted kinds, each bound to its slot class)
// ---------------------------------------------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_arming_waiver_v1::{
    PALW_ARMING_SLOT_CLASS_OVERRIDES_V1, PALW_ARMING_WAIVER_REVIEW_V1, PALW_ARMING_WAIVERS_V1, PalwArmingEvidenceV1,
    PalwArmingSlotClassV1, PalwArmingWaiverKindV1, PalwArmingWaiverV1, palw_arming_evidence_kind_v1, palw_arming_slot_class_for_v1,
    palw_arming_slot_class_v1,
};

/// The requirement text of `fence`'s slot `slot` ("" for the review slot; the frame cost record's one slot is a measured value).
fn requirement_text(fence: &str, slot: u16) -> &'static str {
    use kaspa_consensus_core::palw_g14_gates_v1::PalwG14GatedFenceV1;
    use kaspa_consensus_core::palw_kernel_route_arming_v1::PalwKernelRouteGatedFenceV1;
    use kaspa_consensus_core::palw_rfc0018_v1::PalwRfc0018GatedFenceV1;
    if slot == PALW_ARMING_WAIVER_REVIEW_V1 {
        return "";
    }
    let i = slot as usize;
    if fence == "palw_tir_frame_cost_v1" {
        return "measured: the TIR frame cost model";
    }
    if fence == "palw_model_tier_v1" {
        return kaspa_consensus_core::palw_model_tier_v1::PALW_MODEL_TIER_ARMING_REQUIREMENTS_V1[i];
    }
    if let Some(f) = PalwG14GatedFenceV1::ALL.iter().find(|f| f.name() == fence) {
        return f.requirements()[i];
    }
    if let Some(f) = PalwKernelRouteGatedFenceV1::ALL.iter().find(|f| f.name() == fence) {
        return f.requirements()[i];
    }
    if let Some(f) = PalwRfc0018GatedFenceV1::ALL.iter().find(|f| f.name() == fence) {
        return f.requirements()[i];
    }
    panic!("no requirement list for {fence}")
}

/// Every pinned waiver is of a permitted kind for its slot (F1), carries its value where its kind needs one, and is classified as a
/// waiver — never as a run. (Python and Rust digests agree: each is found by its own digest.)
#[test]
fn every_pinned_waiver_fits_its_slot_and_is_never_a_run() {
    assert!(!PALW_ARMING_WAIVERS_V1.is_empty());
    for w in PALW_ARMING_WAIVERS_V1 {
        let text = requirement_text(w.fence, w.requirement);
        let class = palw_arming_slot_class_for_v1(w.fence, w.requirement, text);
        assert_eq!(w.form(class), Ok(()), "{} slot {}: {:?} in {class:?}", w.fence, w.requirement, w.kind);
        let got = palw_arming_evidence_kind_v1(
            w.fence,
            w.genesis,
            w.activation_daa,
            w.requirement,
            text,
            &w.digest(),
            PALW_ARMING_WAIVERS_V1,
        );
        assert!(matches!(got, Ok(PalwArmingEvidenceV1::Waived(_))), "{} slot {}: {got:?}", w.fence, w.requirement);
    }
}

/// **Every pinned waiver is bound to the flag day (18,400)**: the same waiver at any other height is refused, never a run.
#[test]
fn waiver_height_binds_flag_day() {
    use kaspa_consensus_core::palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1;
    let at = PALW_T12_G14_17000_DAA_V1;
    assert_eq!(at, 18_400);
    for w in PALW_ARMING_WAIVERS_V1 {
        assert_eq!(w.activation_daa, at, "{} slot {} is pinned at another height", w.fence, w.requirement);
        let text = requirement_text(w.fence, w.requirement);
        let d = w.digest();
        for other in [16_600, 17_000, 17_400, at - 1, at + 1] {
            let there = palw_arming_evidence_kind_v1(w.fence, w.genesis, other, w.requirement, text, &d, PALW_ARMING_WAIVERS_V1);
            assert!(there.is_err(), "{} slot {} at {other} must be refused, got {there:?}", w.fence, w.requirement);
        }
    }
    assert!(PALW_G14_ARMING_RECORDS_V1.iter().all(|r| r.activation_daa == at));
}

fn waiver(kind: PalwArmingWaiverKindV1, value: &'static str) -> PalwArmingWaiverV1 {
    PalwArmingWaiverV1 {
        fence: "palw_court_capacity_v1",
        genesis: palw_t12_shipped_params().genesis.hash,
        activation_daa: 18_400,
        requirement: 1,
        kind,
        value,
        reason: "test",
        decided_by: "user 2026-10-27",
    }
}

/// F1: a measured-value slot with an estimate waiver that carries no value is refused.
#[test]
fn f1_a_measured_slot_waiver_without_a_value_is_refused() {
    let class = palw_arming_slot_class_v1(1, requirement_text("palw_court_capacity_v1", 1));
    assert_eq!(class, PalwArmingSlotClassV1::Measured);
    assert!(waiver(PalwArmingWaiverKindV1::ProvisionalEstimate, "").form(class).is_err());
    assert!(waiver(PalwArmingWaiverKindV1::ProvisionalEstimate, "cap 42").form(class).is_ok());
}

/// F1: the review slot with a DrillSkipped waiver is refused (only AgentReview with a report fits it).
#[test]
fn f1_a_review_slot_with_drill_skipped_is_refused() {
    let class = palw_arming_slot_class_v1(PALW_ARMING_WAIVER_REVIEW_V1, "");
    assert_eq!(class, PalwArmingSlotClassV1::Review);
    assert!(waiver(PalwArmingWaiverKindV1::DrillSkipped, "").form(class).is_err());
    assert!(waiver(PalwArmingWaiverKindV1::AgentReview, "").form(class).is_err(), "a review waiver names its report");
    assert!(waiver(PalwArmingWaiverKindV1::AgentReview, "report.md").form(class).is_ok());
}

/// F1: a kind in a slot of another class is refused, and a suite / regression slot takes no waiver at all.
#[test]
fn x_d_drill_skipped_and_e2e_waivers_are_refused_outside_the_override_table() {
    use PalwArmingWaiverKindV1 as K;
    use kaspa_consensus_core::palw_g14_gates_v1::PalwG14GatedFenceV1;
    // Every G14 gated fence's every slot: a DrillSkipped / E2ENotRun waiver fits ONLY where the slot is a drill / node-E2E slot by its
    // text, or is pinned in the Lead's override table (2026-10-27). A V1 suite slot never takes one.
    for f in PalwG14GatedFenceV1::ALL {
        for (i, text) in f.requirements().iter().enumerate() {
            let class = palw_arming_slot_class_for_v1(f.name(), i as u16, text);
            let listed = PALW_ARMING_SLOT_CLASS_OVERRIDES_V1.iter().any(|(n, s, _)| *n == f.name() && *s == i as u16);
            let by_text = palw_arming_slot_class_v1(i as u16, text);
            let mut w = waiver(K::E2ENotRun, "");
            w.fence = f.name();
            w.requirement = i as u16;
            let e2e_ok = w.form(class).is_ok();
            assert_eq!(e2e_ok, by_text == PalwArmingSlotClassV1::NodeE2E || (listed && class == PalwArmingSlotClassV1::NodeE2E));
            w.kind = K::DrillSkipped;
            assert_eq!(w.form(class).is_ok(), class == PalwArmingSlotClassV1::Drill, "{} slot {i}", f.name());
        }
    }
    // The staged court's V1 suite slot is in no table entry: neither kind fits.
    let class = palw_arming_slot_class_for_v1("palw_staged_public_court_v1", 0, requirement_text("palw_staged_public_court_v1", 0));
    assert_eq!(class, PalwArmingSlotClassV1::Run);
    assert!(waiver(K::DrillSkipped, "").form(class).is_err() && waiver(K::E2ENotRun, "").form(class).is_err());
}

#[test]
fn f1_a_mismatched_kind_is_refused() {
    use PalwArmingWaiverKindV1 as K;
    let drill = palw_arming_slot_class_v1(3, requirement_text("palw_court_capacity_v1", 3));
    let suite = palw_arming_slot_class_v1(0, requirement_text("palw_court_capacity_v1", 0));
    assert_eq!((drill, suite), (PalwArmingSlotClassV1::Drill, PalwArmingSlotClassV1::Run));
    assert!(waiver(K::E2ENotRun, "").form(drill).is_err());
    assert!(waiver(K::ProvisionalEstimate, "x").form(drill).is_err());
    assert!(waiver(K::DrillSkipped, "").form(drill).is_ok());
    for kind in [K::DrillSkipped, K::E2ENotRun, K::ProvisionalEstimate, K::AgentReview, K::UserDecision] {
        assert!(waiver(kind, "x").form(suite).is_err(), "{kind:?} filled a suite slot");
    }
}

/// **A waiver never satisfies a requirement that must hold**: ordering, placeholder-value and open-blocker refusals stand with every
/// record present; the ONE exception is court capacity's user-approved V11 estimate (its measured slot is a pinned
/// `ProvisionalEstimate` for exactly that policy). Records here are TEST records (non-waiver digests) for the other slots.
#[test]
fn waiver_cannot_satisfy_requirement_passed() {
    use kaspa_consensus_core::config::params::{PALW_T12_FULL_COURT_CAPACITY_ENTRY, PALW_T12_FULL_SETTLEMENT_ONCE_ENTRY, Params};
    use kaspa_consensus_core::palw_court_capacity_v1::PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1;
    use kaspa_consensus_core::palw_g14_gates_v1::{PALW_G14_ARMING_RECORD_VERSION_V1, PalwG14ArmingRecordV1, PalwG14GatedFenceV1};
    let at = PALW_T12_G14_17000_DAA_V1;
    let base = kaspa_consensus_core::config::params::palw_t12_release_v7_params();
    let g = base.genesis.hash;
    let record = |fence: PalwG14GatedFenceV1, seed: u8| {
        let mut ev: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [seed.wrapping_add(i as u8); 32]).collect();
        if fence == PalwG14GatedFenceV1::CourtCapacityV1 {
            let w = PALW_ARMING_WAIVERS_V1
                .iter()
                .find(|w| w.fence == "palw_court_capacity_v1" && w.requirement == PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1 as u16)
                .expect("the pinned V11 estimate waiver");
            ev[PALW_COURT_MEASUREMENT_EVIDENCE_INDEX_V1] = w.digest();
        }
        PalwG14ArmingRecordV1 {
            version: PALW_G14_ARMING_RECORD_VERSION_V1,
            fence,
            genesis: g,
            activation_daa: at,
            source_commit: [7; 20],
            evidence: Box::leak(ev.into_boxed_slice()),
            review: [seed.wrapping_add(100); 32],
        }
    };
    let records = [record(PalwG14GatedFenceV1::LegacyPublicFilerV1, 10), record(PalwG14GatedFenceV1::CourtCapacityV1, 40)];
    let named = |p: &mut Params, name: &str| {
        let f = PALW_T12_G14_17000_FENCES_V1.iter().find(|f| f.name == name).expect("on the list");
        (f.set)(p, Some(ForkActivation::new(at)));
    };
    // Ordering: held DA v2 without the filer.
    let mut p = base.clone();
    named(&mut p, "palw_legacy_held_da_v2");
    assert!(format!("{:?}", p.validate_palw_v2_with_arming_records_v1(&records)).contains("needs palw_legacy_public_filer_v1"));
    // Placeholder value: court capacity's placeholder policy (not the approved estimate), with its record.
    let mut p = base.clone();
    named(&mut p, "palw_legacy_public_filer_v1");
    named(&mut p, "palw_court_outsider_lane_v1");
    (PALW_T12_FULL_COURT_CAPACITY_ENTRY.set)(&mut p, Some(ForkActivation::new(at)));
    let e = format!("{:?}", p.validate_palw_v2_with_arming_records_v1(&records));
    assert!(e.contains("palw_court_capacity_v1") && !e.contains("no arm-readiness record"), "{e}");
    // Open blocker: settlement once (with its prerequisites) is refused by RFC08-2 whatever records exist.
    let mut p = base.clone();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_KERNEL_SLICE_FENCES_V1, Some(at));
    (PALW_T12_FULL_SETTLEMENT_ONCE_ENTRY.set)(&mut p, Some(ForkActivation::new(at)));
    // Asked of the fence's own check (the record gates of the slice refuse earlier in the whole validation while no record is pinned).
    let e = format!("{:?}", p.validate_palw_settlement_once_v1());
    assert!(e.contains("open blocker") && e.contains("RFC08-2"), "{e}");
    // The one exception: the approved V11 estimate (the list's own court-capacity entry) with its ProvisionalEstimate waiver.
    let mut p = base.clone();
    for name in ["palw_legacy_public_filer_v1", "palw_court_outsider_lane_v1", "palw_court_capacity_v1"] {
        named(&mut p, name);
    }
    assert_eq!(p.validate_palw_v2_with_arming_records_v1(&records), Ok(()), "the approved estimate is accepted");
}

/// The old unpublished height must not fire this list. The activation is one
/// exact boundary; int-13's already-running entries keep their own history.
#[test]
fn unpublished_g14_moves_only_its_boundary_to_18400() {
    use kaspa_consensus_core::config::params::{PALW_T12_G14_18400_DAA_V1, palw_t12_fence_held_by_int13_v1};
    let p = palw_t12_shipped_params();
    assert_eq!(PALW_T12_G14_18400_DAA_V1, 18_400);
    assert_eq!(PALW_T12_G14_17000_DAA_V1, PALW_T12_G14_18400_DAA_V1);
    let fences = p.palw_fences_v1();
    for entry in PALW_T12_G14_17000_FENCES_V1 {
        if entry.name == "palw_tir_frame_cost_v1" {
            // This list also installs an approved value, not an independent
            // activation fence. Its readiness record binds the actual public
            // conviction fence whose boundary is checked by the loop below.
            let cost = kaspa_consensus_core::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1;
            assert_eq!(p.palw_tir_frame_cost_v1, Some(cost));
            assert!(
                kaspa_consensus_core::palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORDS_V1
                    .iter()
                    .any(|r| r.genesis == p.genesis.hash && r.activation_daa == 18_400 && r.cost == cost)
            );
            continue;
        }
        let at = fences
            .iter()
            .find(|(name, _)| *name == entry.name)
            .and_then(|(_, at)| *at)
            .unwrap_or_else(|| panic!("{} must have an actual activation", entry.name));
        if palw_t12_fence_held_by_int13_v1(entry) {
            assert_eq!(at.daa_score(), 15_500);
            continue;
        }
        assert_eq!(at.daa_score(), 18_400, "{}", entry.name);
        assert!(!at.is_active(17_400), "{} fired at unpublished predecessor", entry.name);
        assert!(!at.is_active(18_399), "{} fired early", entry.name);
        assert!(at.is_active(18_400) && at.is_active(18_401), "{} misses its actual boundary", entry.name);
    }
    assert!(!p.fence_schedule_v1().contains(&17_400));
    p.validate_palw_v2().expect("all existing startup guards remain enabled");
}

#[test]
fn moving_existing_g14_does_not_pretend_unfinished_additional_fences_are_enabled() {
    let p = palw_t12_shipped_params();
    assert_eq!(kaspa_consensus_core::config::params::PALW_T12_FULL_ACTIVATION_DAA, None);
    assert!(
        p.palw_fences_v1().iter().all(|(name, at)| *name != "palw_court_family_supplier_v1" || at.is_none()),
        "the isolated release adds no RFC42 fence or implementation"
    );
    assert!(p.palw_claim_local_settlement_v1.is_none());
    assert!(p.palw_unresolved_bound_v1.is_none());
    assert!(p.palw_native_execution_v39.is_none() && p.palw_native_mining_v39.is_none());
    assert!(p.palw_selected_source_bootstrap_v1.is_none() && p.palw_paid_bootstrap_source_v6.is_none());
    assert!(p.palw_settlement_once_v1.is_none());
    assert!(p.palw_reporter_income_v2.is_none() && p.palw_material_income_v3.is_none());
    assert!(p.palw_fp_answer_accounting_v1.is_none());
}
