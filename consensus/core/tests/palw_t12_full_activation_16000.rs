//! **testnet-12's full-activation list** (lane g14p/r7-fence-16000, 2026-10-22): `PALW_T12_FULL_ACTIVATION_FENCES_V1` at
//! `PALW_T12_FULL_ACTIVATION_DAA` — `None`, target 16,000.
//!
//! * **dormant** — with the height `None` every shipped id is the int-12 release's, byte for byte (the int-13 twin, extended);
//! * **ordered** — every "needs X at or below it" of validation names an EARLIER entry (or a rule outside the list, named below);
//! * **16,000 is free** — no other list, no shipped schedule uses it (the fork id names heights, not fences);
//! * **records** — armed without its arming records the list is refused by validation, and the drill flag refuses untouched.
//!
//! Run: `cargo test -p kaspa-consensus-core --test palw_t12_full_activation_16000`

use kaspa_consensus_core::config::drill::{
    PALW_DRILL_SALT_LEN_V1, PalwDrillSaltV1, palw_drill_full_activation_at_v1, palw_drill_int11_at_v1,
    palw_drill_post_launch_fences_at_v1, palw_drill_post_launch_fences_v2_at_v1, palw_drill_post_launch_fences_v3_at_v1,
    palw_drill_tir_fence_at_v1, palw_drill_tir_fence2_at_v1,
};
use kaspa_consensus_core::config::params::{
    PALW_T12_FULL_ACTIVATION_DAA, PALW_T12_FULL_ACTIVATION_FENCES_V1, PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1,
    PALW_T12_INT11_FLAG_DAY_DAA, PALW_T12_INT11_RHO100_DAA, PALW_T12_INT11_RHO250_DAA, PALW_T12_INT11_RHO1000_DAA, PALW_T12_INT13_DAA,
    PALW_T12_INT13_FENCES_V1, Params, palw_t12_arm_full_activation_at_v1, palw_t12_drill_params_v1, palw_t12_release_v6_params,
    palw_t12_release_v7_params, palw_t12_shipped_params,
};

/// The list's names, in the order its prerequisites need. Adding an entry is one line here, one line in the list, and the notes.
const LIST: [&str; 37] = [
    "palw_audit_1004_v1",
    "palw_gen_range_twin_v1",
    "palw_model_court_window",
    "palw_receipt_spend_v4",
    "palw_probabilistic_constraints_v1",
    "palw_kernel_route_panel_one_v1",
    "palw_signed_registration_v1",
    "palw_provider_court_v1",
    "palw_legacy_public_filer_v1",
    "palw_legacy_held_da_v2",
    "palw_staged_public_court_v1",
    "palw_staged_tir_route_v1",
    "palw_staged_float_constraint_v1",
    "palw_tir_container_identity_v1",
    "palw_tir_public_conviction_v1",
    "palw_tir_designated_answers_v1",
    "palw_tir_bounded_door_v1",
    "palw_court_outsider_lane_v1",
    "palw_court_clock_v2",
    "palw_court_capacity_v1",
    "palw_court_completion_attempt_v1",
    "palw_court_model_scope_v1",
    "palw_public_evidence_reward_v1",
    // Compute-claim audit CX-01/CX-02 (ADR-0205 draft): its prerequisites are genesis fences of testnet-12.
    "palw_fp_answer_accounting_v1",
    "palw_claim_hard_deadline_v1",
    "palw_settlement_once_v1",
    "palw_native_liability_horizon_v1",
    "palw_reward_liability_horizon_v1",
    "palw_unresolved_bound_v1",
    "palw_independently_work_certified_v1",
    "palw_dag_sibling_attribution_v1",
    "palw_panel_anchor_permissionless_v1",
    "palw_model_tier_v1",
    "palw_memory_routing_court_v2",
    "palw_monetary_material_v1",
    "palw_elastic_final_capacity_v1",
    "palw_g14_route_required_v1",
];

/// **Validation's "X needs Y at or below it" pairs among the list** (fence, prerequisite) — read from the refusals in
/// `config/params/validation.rs`, `palw_g14_gates_v1.rs`, `palw_settlement_once_v1.rs`, `palw_rfc0018_v1.rs`, `palw_model_tier_v1.rs`,
/// `palw_monetary_material_v1.rs`, `palw_rfc0016_v1/values.rs` and the resolvers in `params.rs` (`*_fence()`).
const PREREQUISITES: &[(&str, &str)] = &[
    ("palw_kernel_route_panel_one_v1", "palw_probabilistic_constraints_v1"),
    ("palw_staged_public_court_v1", "palw_legacy_public_filer_v1"),
    ("palw_staged_tir_route_v1", "palw_staged_public_court_v1"),
    ("palw_staged_float_constraint_v1", "palw_staged_public_court_v1"),
    ("palw_tir_public_conviction_v1", "palw_staged_tir_route_v1"),
    ("palw_tir_public_conviction_v1", "palw_staged_public_court_v1"),
    ("palw_tir_designated_answers_v1", "palw_tir_public_conviction_v1"),
    ("palw_tir_bounded_door_v1", "palw_tir_public_conviction_v1"),
    ("palw_court_outsider_lane_v1", "palw_legacy_public_filer_v1"),
    ("palw_court_clock_v2", "palw_legacy_public_filer_v1"),
    ("palw_court_capacity_v1", "palw_legacy_public_filer_v1"),
    ("palw_court_model_scope_v1", "palw_legacy_held_da_v2"),
    ("palw_settlement_once_v1", "palw_claim_hard_deadline_v1"),
    ("palw_settlement_once_v1", "palw_probabilistic_constraints_v1"),
    ("palw_native_liability_horizon_v1", "palw_settlement_once_v1"),
    ("palw_independently_work_certified_v1", "palw_probabilistic_constraints_v1"),
    ("palw_model_tier_v1", "palw_independently_work_certified_v1"),
    ("palw_monetary_material_v1", "palw_provider_court_v1"),
    ("palw_elastic_final_capacity_v1", "palw_probabilistic_constraints_v1"),
    // The route mirror reads the resolved heights of the route fences: it goes last.
    ("palw_g14_route_required_v1", "palw_legacy_public_filer_v1"),
    ("palw_g14_route_required_v1", "palw_legacy_held_da_v2"),
    ("palw_g14_route_required_v1", "palw_staged_public_court_v1"),
    ("palw_g14_route_required_v1", "palw_tir_public_conviction_v1"),
    ("palw_g14_route_required_v1", "palw_court_capacity_v1"),
];

/// Prerequisites validation names that are NOT on the list and NOT in force on testnet-12 — the notes' "missing" section. Armed without
/// them the list is refused by name until a reviewed change adds them (they are policy-carrying fences the user has not approved).
const MISSING_PREREQUISITES: &[&str] = &["palw_bond_budget_v1", "palw_model_bond_allocation_v1", "palw_material_income_v3"];

/// Every other list's height on testnet-12 (shipped, or once scheduled and kept as a pinned history).
const OTHER_HEIGHTS: [u64; 11] = [750, 1_000, 1_300, 1_700, 2_000, 3_600, 5_300, 5_395, 5_490, 5_585, 9_000];

fn ids(p: &Params) -> (String, String, String) {
    (p.consensus_params_id().to_string(), p.consensus_identity_id().to_string(), p.consensus_schedule_id().to_string())
}

fn fence_at(p: &Params, name: &str) -> Option<u64> {
    p.palw_fences_v1().iter().find(|(n, _)| *n == name).and_then(|(_, f)| *f).map(|f| f.daa_score())
}

fn salt() -> PalwDrillSaltV1 {
    PalwDrillSaltV1::from_bytes([0x5d; PALW_DRILL_SALT_LEN_V1]).expect("a legal salt")
}

/// int-12's params / identity / schedule ids (the fleet's `5ee7fd8e…`), split so the re-pin tool does not read them as a pin.
fn int12_ids() -> (String, String, String) {
    (
        concat!("5ee7fd8ee019968cf52929b844cf9ddf", "b1aad500842a89cf04bced8ba4edefb6").to_owned(),
        concat!("5de80e64b63572de0cbf1a09679034e3", "a1765166e8249d3a88f8e29891215bb5").to_owned(),
        concat!("1678e07359f6727e96224041450a3b1d", "2aadcf8acd6bb6db0c277ff4d401c9b9").to_owned(),
    )
}

#[test]
fn the_list_is_the_int13_list_then_the_g14_fences_in_prerequisite_order() {
    let names: Vec<&str> = PALW_T12_FULL_ACTIVATION_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(names, LIST);
    let int13: Vec<&str> = PALW_T12_INT13_FENCES_V1.iter().map(|f| f.name).collect();
    assert_eq!(&names[..int13.len()], &int13[..], "the int-13 list, unchanged and first");
    let mut seen = std::collections::BTreeSet::new();
    for name in &names {
        assert!(seen.insert(*name), "{name} twice");
    }
    // Every entry is a fence of the ruleset (`palw_fences_v1` names it, so the fork id schedules it).
    let shipped = palw_t12_shipped_params();
    for name in &names {
        assert!(shipped.palw_fences_v1().iter().any(|(n, _)| n == name), "{name} is not on palw_fences_v1");
    }
    // Every prerequisite on the list comes earlier.
    let index = |n: &str| names.iter().position(|x| *x == n);
    for (fence, prerequisite) in PREREQUISITES {
        let f = index(fence).unwrap_or_else(|| panic!("{fence} on the list"));
        let p = index(prerequisite).unwrap_or_else(|| panic!("{prerequisite} on the list"));
        assert!(p < f, "{prerequisite} must come before {fence}");
    }
    for missing in MISSING_PREREQUISITES {
        assert!(index(missing).is_none(), "{missing} is a documented missing prerequisite, not an entry");
        assert_eq!(fence_at(&shipped, missing), None, "{missing} is not in force on testnet-12 either");
    }
    // `palw_shard_court` is a genesis rule of the held regime on testnet-12: not on the list (moving it would move a genesis rule).
    assert!(index("palw_shard_court").is_none());
    assert_eq!(fence_at(&shipped, "palw_shard_court"), Some(0), "the held regime's genesis court");
}

#[test]
fn with_the_height_none_every_shipped_id_is_the_int12_release() {
    assert_eq!(PALW_T12_FULL_ACTIVATION_DAA, None, "nothing armed until the records are pinned and the user approves");
    assert_eq!(PALW_T12_INT13_DAA, None);
    let shipped = palw_t12_shipped_params();
    for name in LIST {
        assert_eq!(fence_at(&shipped, name), None, "{name} is dormant on the shipped ruleset");
    }
    assert_eq!(ids(&shipped), int12_ids(), "the shipped ruleset is int-12's, byte for byte");
    assert_eq!(ids(&palw_t12_release_v7_params()), int12_ids(), "the v7 baseline too");
    assert_eq!(ids(&palw_t12_release_v6_params()), int12_ids(), "and v6");
    // Dormant twice is dormant: the reverse-order clear of an already-dormant list moves nothing.
    let mut again = shipped.clone();
    palw_t12_arm_full_activation_at_v1(&mut again, None);
    assert_eq!(ids(&again), ids(&shipped));
}

#[test]
fn sixteen_thousand_is_used_by_no_other_list_or_schedule() {
    let target = PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1;
    assert_eq!(target, 16_000);
    assert!(!OTHER_HEIGHTS.contains(&target));
    for other in [
        PALW_T12_INT11_FLAG_DAY_DAA,
        PALW_T12_INT11_RHO100_DAA,
        PALW_T12_INT11_RHO250_DAA,
        PALW_T12_INT11_RHO1000_DAA,
        PALW_T12_INT13_DAA,
    ] {
        assert_ne!(other, Some(target), "another list's height");
    }
    for p in [palw_t12_shipped_params(), palw_t12_drill_params_v1(&salt())] {
        assert!(!p.fence_schedule_v1().contains(&target), "{:?}", p.fence_schedule_v1());
        assert!(p.palw_fences_v1().iter().all(|(_, f)| f.map(|f| f.daa_score()) != Some(target)));
    }
}

#[test]
fn armed_at_a_test_height_it_moves_the_params_and_schedule_ids_only_and_comes_back_byte_for_byte() {
    let shipped = palw_t12_shipped_params();
    let mut armed = shipped.clone();
    palw_t12_arm_full_activation_at_v1(&mut armed, Some(PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1));
    for name in LIST {
        assert_eq!(fence_at(&armed, name), Some(PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1), "{name}");
    }
    for ((name, was), (_, now)) in shipped.palw_fences_v1().iter().zip(armed.palw_fences_v1().iter()) {
        if !LIST.contains(name) {
            assert_eq!(was, now, "{name} did not move");
        }
    }
    let (b, a) = (ids(&shipped), ids(&armed));
    assert_ne!(a.0, b.0, "the params id moves");
    assert_eq!(a.1, b.1, "a future height: the identity is unchanged");
    assert_ne!(a.2, b.2, "the schedule names 16,000");
    assert!(armed.fence_schedule_v1().contains(&PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1));
    palw_t12_arm_full_activation_at_v1(&mut armed, None);
    assert_eq!(ids(&armed), ids(&shipped), "dormant in reverse order is the shipped ruleset again");
}

/// **Armed without its records, the list is refused.** The records each entry needs (the notes list them per fence): the G14
/// arm-readiness records (`palw_g14_gates_v1::PALW_G14_ARMING_RECORDS_V1`, empty), the TIR frame-cost records
/// (`palw_tir_frame_cost_v1::PALW_TIR_FRAME_COST_RECORDS_V1`), the RFC-0608 measured records (memory/routing court v2, monetary
/// material), and the approved policies (X3, court capacity, model tiers, monetary material) — none is pinned, so public testnet-12
/// at 16,000 does not validate.
#[test]
fn armed_on_public_testnet_12_without_records_it_is_refused() {
    let mut armed = palw_t12_shipped_params();
    palw_t12_arm_full_activation_at_v1(&mut armed, Some(PALW_T12_FULL_ACTIVATION_TARGET_DAA_V1));
    let why = armed.validate_palw_v2().expect_err("no arming record is pinned: refused");
    let why = format!("{why:?}");
    assert!(!why.is_empty());
    // With explicitly empty record lists it is the same refusal: nothing but the records (and the policies) stand in the way.
    assert!(armed.validate_palw_v2_with_arming_and_cost_records_v1(&[], &[]).is_err());
}

fn drill_with_the_earlier_flag_days_low() -> Params {
    let mut p = palw_t12_drill_params_v1(&salt());
    palw_drill_post_launch_fences_at_v1(&mut p, 6).expect("--palw-drill-fence-at=6");
    palw_drill_post_launch_fences_v2_at_v1(&mut p, 10).expect("--palw-drill-fence2-at=10");
    palw_drill_post_launch_fences_v3_at_v1(&mut p, 14).expect("--palw-drill-fence3-at=14");
    palw_drill_tir_fence_at_v1(&mut p, 20).expect("--palw-drill-tir-at=20");
    palw_drill_tir_fence2_at_v1(&mut p, 24).expect("--palw-drill-tir2-at=24");
    palw_drill_int11_at_v1(&mut p, 28).expect("--palw-drill-int11-at=28");
    p
}

/// **The drill flag moves exactly the list or refuses untouched.** On the salted drill genesis (not an isolated G14 test genesis) no
/// record admits the gated entries, so today the move is refused BY VALIDATION and leaves the ruleset as it came; the other refusals
/// (genesis, never, another fence's height, public genesis, another network) are the post-launch mover's.
#[test]
fn the_drill_flag_refuses_untouched_until_the_records_exist() {
    let before = drill_with_the_earlier_flag_days_low();
    for name in LIST {
        assert_eq!(fence_at(&before, name), None, "{name}: dormant on the drill ruleset before the flag");
    }
    let mut p = before.clone();
    match palw_drill_full_activation_at_v1(&mut p, 60) {
        Ok(moves) => {
            // Only once every record exists for the drill genesis: exactly the list, in its order, and the result validates.
            assert_eq!(moves.iter().map(|m| m.name).collect::<Vec<_>>(), LIST);
            p.validate_palw_v2().expect("the moved drill validates");
        }
        Err(why) => {
            assert!(why.contains("does not validate"), "{why}");
            assert_eq!(ids(&p), ids(&before), "refused untouched");
        }
    }
    for (at, what) in [(0, "genesis"), (u64::MAX, "never"), (24, "the second IR fence's height")] {
        let mut q = before.clone();
        assert!(palw_drill_full_activation_at_v1(&mut q, at).is_err(), "{what}");
        assert_eq!(ids(&q), ids(&before), "{what}: untouched");
    }
    let mut public = palw_t12_shipped_params();
    assert!(palw_drill_full_activation_at_v1(&mut public, 60).is_err(), "public testnet-12's genesis");
    let mut mainnet = kaspa_consensus_core::config::params::mainnet_shipped_params();
    assert!(palw_drill_full_activation_at_v1(&mut mainnet, 60).is_err(), "another network");
}
