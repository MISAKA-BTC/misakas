//! Diagnostic refusal tests for the requested all-feature DAA 18,400 candidate.
//! Passing these tests means the current blockers are still enforced, NOT that
//! the candidate is ready to ship. This tracks the implementation-only goal; shipping evidence remains separate.
//! No preset, record or validation gate is changed.
use kaspa_consensus_core::config::params::{
    ForkActivation, PALW_T12_FULL_ACTIVATION_FENCES_V1, PALW_T12_G14_17000_FENCES_V1, Params, palw_t12_arm_after_int13_at_v1,
    palw_t12_shipped_params,
};

fn candidate() -> Params {
    let mut p = palw_t12_shipped_params();
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_G14_17000_FENCES_V1, Some(18_400));
    palw_t12_arm_after_int13_at_v1(&mut p, PALW_T12_FULL_ACTIVATION_FENCES_V1, Some(18_400));
    p
}

#[test]
fn full_list_at_17400_is_not_a_valid_shipping_ruleset_yet() {
    let p = candidate();
    let why = p.validate_palw_v2().expect_err("the full candidate must not silently bypass readiness");
    println!("FULL_18400_FIRST_REFUSAL: {why:?}");
    assert_eq!(p.palw_settlement_once_v1, Some(ForkActivation::new(18_400)));
    assert!(p.palw_claim_local_settlement_v1.is_none(), "the FULL list itself does not cover ALL dormant features");
}

#[test]
fn independent_full_list_blockers_remain_visible_beyond_the_first_refusal() {
    let mut p = candidate();
    let settlement = format!("{:?}", p.validate_palw_settlement_once_v1().unwrap_err());
    println!("SETTLEMENT_ONCE: {settlement}");
    assert!(settlement.contains("open blocker") && settlement.contains("unsourced terms"));

    let monetary = format!("{:?}", p.validate_palw_monetary_material_v1().unwrap_err());
    println!("MONETARY_MATERIAL: {monetary}");
    assert!(monetary.contains("capacity/cost bounds"));

    let unresolved = format!("{:?}", p.validate_palw_unresolved_bound_v1().unwrap_err());
    println!("RFC23_STAGE1: {unresolved}");
    assert!(unresolved.contains("steady-state lag"));

    let memory = format!("{:?}", p.validate_palw_memory_routing_court_v2().unwrap_err());
    println!("MEMORY_ROUTING: {memory}");
    assert!(memory.contains("no pinned measured-value record"));

    // Monetary material additionally needs both income schemas, absent from FULL.
    p.palw_reporter_income_v2 = Some(ForkActivation::new(18_400));
    p.palw_material_income_v3 = Some(ForkActivation::new(18_400));
    p.sync_palw_bond_budget_v1();
    let reporter = format!("{:?}", p.validate_palw_reporter_income_v2().unwrap_err());
    println!("REPORTER_INCOME: {reporter}");
    assert!(reporter.contains("legacy cash migration"));
    let material_income = format!("{:?}", p.validate_palw_material_income_v3().unwrap_err());
    println!("MATERIAL_INCOME: {material_income}");
    assert!(material_income.contains("signed material/TTL"));

    // ALL additionally includes stage 2, which is absent from the FULL list.
    p.palw_claim_local_settlement_v1 = Some(ForkActivation::new(18_400));
    p.sync_palw_claim_local_settlement_v1();
    let local = format!("{:?}", p.validate_palw_claim_local_settlement_v1().unwrap_err());
    println!("RFC23_STAGE2: {local}");
    assert!(local.contains("runtime integration is incomplete"));
}

#[test]
fn assembled_fence_inventory_keeps_existing_activation_history_visible() {
    let p = palw_t12_shipped_params();
    let g14 = PALW_T12_G14_17000_FENCES_V1;
    let full = PALW_T12_FULL_ACTIVATION_FENCES_V1;
    for (name, at) in p.palw_fences_v1() {
        let cohort = if g14.iter().any(|entry| entry.name == name) {
            "G14"
        } else if full.iter().any(|entry| entry.name == name) {
            "FULL"
        } else {
            "outside_named_lists"
        };
        println!("FENCE_INVENTORY {name} current={at:?} list={cohort}");
    }
    assert!(p.palw_fences_v1().iter().any(|(name, at)| *name == "palw_claim_local_settlement_v1" && at.is_none()));
}
