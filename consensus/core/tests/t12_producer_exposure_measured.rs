//! **Why every t12 seat logs `holding: the bond's exposure ceiling leaves no room for another
//! claim` with `produced=0`, measured against the numbers the live fleet printed.**
//!
//! `palw_v2_collateral_for_class_set_v1` sizes the genesis carve from each class's OWN declared
//! leaves (`palw_pwu_v1(target, pwu_per_inference)`). The runtime reserves
//! `palw_exposure_pwu_v3(class, pwu, canonical_draw, exposure_basis) x slash`, which is this
//! class's DERIVED MAC-eq draw renormalised by the FLOOR class's leaves-per-MAC-eq. Those are two
//! different units, and the 2M dense row is where they diverge most.
//!
//! The fleet's own figures, 2026-09-23 (`/root/t12/seat2.log`, all four seats identical):
//!
//! ```text
//! exposure=0/3004409203800  per_claim=5974294206820
//! ```
//!
//! This test derives both numbers from the shipped params so the discrepancy is a measurement and
//! not a reading of a log line.

use kaspa_consensus_core::palw_canonical_work_v1::{PalwCanonicalClassDescriptorV1, palw_canonical_draw_work_v1};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2, palw_max_exposure_pwu_of_rule_v1};

const SLASH_VALUE_PER_PWU: u64 = 5;
const MAX_EXPOSURE_RATIO_PERMILLE: u128 = 500;

/// What the live seats printed, so a change to either side of the comparison fails here.
const LIVE_CEILING_SOMPI: u128 = 3_004_409_203_800;
const LIVE_PER_CLAIM_SOMPI: u128 = 5_974_294_206_820;
/// The collateral that SHIPPED FIRST and wedged the fleet. Kept as a literal, not read from the
/// constant, because this test's whole job is to show why that figure was wrong — reading the
/// (now corrected) constant would make the test pass for the wrong reason.
const T12_WEDGED_COLLATERAL_SOMPI: u128 = 6_008_818_407_600;

fn declared_leaves(object: &PalwConsensusObjectV2) -> u64 {
    match object {
        PalwConsensusObjectV2::ClassRegistered { pwu_rule, .. } => palw_max_exposure_pwu_of_rule_v1(pwu_rule),
        other => panic!("not a registration: {other:?}"),
    }
}

fn pwu_rule_is_derived(object: &PalwConsensusObjectV2) -> bool {
    matches!(object, PalwConsensusObjectV2::ClassRegistered { pwu_rule: PalwPwuRuleV2::DerivedV1 { .. }, .. })
}

#[test]
fn the_ceiling_and_the_per_claim_reservation_are_in_different_units() {
    // The shipped t12 bundle, so every fence position is the one the fleet is running.
    let params = kaspa_consensus_core::config::params::Params::from(
        kaspa_consensus_core::network::NetworkId::with_suffix(kaspa_consensus_core::network::NetworkType::Testnet, 12),
    );
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        panic!("testnet-12 is a ConsensusV2 network")
    };

    println!("\n=== t12 genesis registry ===");
    let mut rows = Vec::new();
    for object in bundle.genesis_objects.iter().cloned() {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, .. } = &object {
            let leaves = declared_leaves(&object);
            println!("  class {:.16}…  declared_leaves_per_draw={leaves}  derived_rule={}", class_id.to_string(), pwu_rule_is_derived(&object));
            rows.push((*class_id, leaves));
        }
    }
    assert!(rows.len() >= 2, "t12 registers the floor plus at least one model row");

    println!("\n=== ceiling ===");
    let ceiling = T12_WEDGED_COLLATERAL_SOMPI * MAX_EXPOSURE_RATIO_PERMILLE / 1000;
    println!("  collateral      {T12_WEDGED_COLLATERAL_SOMPI} sompi = {:.8} MSK", T12_WEDGED_COLLATERAL_SOMPI as f64 / 1e8);
    println!("  ceiling (500‰)  {ceiling} sompi = {:.8} MSK", ceiling as f64 / 1e8);
    assert_eq!(ceiling, LIVE_CEILING_SOMPI, "the derived ceiling is the one the fleet printed");

    println!("\n=== the 2M dense row, both units ===");
    let n_ctx = kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX;
    let canonical = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(n_ctx);
    let (profile, _, _object_2m) = kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_registration_v1(
        kaspa_consensus_core::config::params::PALW_T12_GENESIS_QWEN25_A16_2M_ARTIFACT_ROOT,
        n_ctx,
        1,
        SLASH_VALUE_PER_PWU,
        u128::MAX / 2,
        bundle,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::MerkleV1,
        match bundle.genesis_objects.iter().find_map(|o| match o {
            PalwConsensusObjectV2::BondRegistered { bond, .. } => Some(*bond),
            _ => None,
        }) { Some(k) => k, None => panic!("t12 registers bonds at genesis") },
    )
    .expect("the 2M held registration derives");
    let descriptor = PalwCanonicalClassDescriptorV1::of(&profile, kaspa_consensus_core::Hash64::default()).expect("one weight format");
    let job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, canonical.0, canonical.1);
    let derived_per_draw = palw_canonical_draw_work_v1(&descriptor, &job, true).expect("derives").provisional_scalar_v1();
    println!("  n_ctx                     {n_ctx}");
    println!("  canonical (prefill,decode) {canonical:?}");
    println!("  derived MAC-eq per draw    {derived_per_draw}");

    println!("\n=== the floor row, both units (the basis) ===");
    let floor_profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .expect("the floor profile derives");
    let floor_canonical = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let floor_descriptor =
        PalwCanonicalClassDescriptorV1::of(&floor_profile, kaspa_consensus_core::Hash64::default()).expect("one weight format");
    let floor_job = kaspa_consensus_core::palw_base0_profile::rc_job_context(&floor_profile, floor_canonical.0, floor_canonical.1);
    let floor_derived = palw_canonical_draw_work_v1(&floor_descriptor, &floor_job, true).expect("derives").provisional_scalar_v1();
    let floor_declared = rows[0].1;
    println!("  declared leaves per draw  {floor_declared}");
    println!("  derived MAC-eq per draw   {floor_derived}");

    println!("\n=== the two units, side by side, for the 2M row ===");
    let declared_2m = 27_002_967_184u128;
    let scaled = derived_per_draw * (floor_declared as u128) / floor_derived;
    println!("  A. its OWN declared leaves         {declared_2m} -> {} sompi = {:.2} MSK", declared_2m * 5, (declared_2m * 5) as f64 / 1e8);
    println!("  B. renormalised by the floor       {scaled} -> {} sompi = {:.2} MSK", scaled * 5, (scaled * 5) as f64 / 1e8);
    println!("  ratio B/A                          {:.2}x", scaled as f64 / declared_2m as f64);

    let per_claim = scaled * 5;
    assert_eq!(
        per_claim, LIVE_PER_CLAIM_SOMPI,
        "the renormalised reservation is the per_claim the live seats printed"
    );
    assert!(
        per_claim > ceiling,
        "this is the wedge: one claim needs {per_claim} against a ceiling of {ceiling}"
    );

    println!("\n=== what the fleet needs ===");
    let needed_one = per_claim * 1000 / MAX_EXPOSURE_RATIO_PERMILLE;
    println!("  collateral for ONE in-flight 2M claim  {needed_one} sompi = {:.2} MSK", needed_one as f64 / 1e8);
    println!("  shipped                                {T12_WEDGED_COLLATERAL_SOMPI} sompi = {:.2} MSK", T12_WEDGED_COLLATERAL_SOMPI as f64 / 1e8);
    println!("  short by                               {:.2}x", needed_one as f64 / T12_WEDGED_COLLATERAL_SOMPI as f64);

    // **And the shipped card now clears it.** This is the assertion that says the fleet can mine;
    // everything above it says why it could not.
    let shipped = kaspa_consensus_core::config::premine::PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI as u128;
    let shipped_ceiling = shipped * MAX_EXPOSURE_RATIO_PERMILLE / 1000;
    println!("\n=== the corrected card ===");
    println!("  collateral       {shipped} sompi = {:.2} MSK", shipped as f64 / 1e8);
    println!("  ceiling (500‰)   {shipped_ceiling} sompi = {:.2} MSK", shipped_ceiling as f64 / 1e8);
    println!("  concurrent 2M claims it clears: {}", shipped_ceiling / per_claim);
    assert!(shipped_ceiling > per_claim, "the corrected card admits at least one 2M claim");
    assert_eq!(shipped_ceiling / per_claim, 4, "four, which is PALW_MODEL_CLAIM_CONCURRENCY_V1");
}

/// **The collateral each option would need, in the unit the RUNTIME reserves in.**
///
/// Three units are in play and the shipped carve used the one no runtime path reads:
///
/// | unit | the 2M row | who reads it |
/// |---|---|---|
/// | the class's own declared leaves | 27,002,967,184 | `palw_v2_collateral_for_class_set_v1` (the shipped carve), and the admission equality BELOW the canonical-work fence |
/// | its derived MAC-eq per draw | 3,357,281,757,221,376 | `palw_attempt_derived_pwu_v1` — the pwu an attempt must declare ABOVE the fence, so the fork weight |
/// | that, renormalised by the floor's leaves-per-MAC-eq | 1,194,858,841,364 | `palw_exposure_pwu_v3` — the reservation and the ceiling |
///
/// t12 arms `palw_canonical_work` at genesis, so the third row is live and the first is not.
#[test]
fn the_collateral_each_row_needs_in_the_runtime_unit() {
    const CONCURRENCY: u128 = 4; // PALW_MODEL_CLAIM_CONCURRENCY_V1
    const MAX_CLAIM_EXPOSURE_DAA: u128 = 7_200;

    let floor_declared: u128 = 7_708;
    let floor_derived: u128 = 21_657_728;

    // Each row's derived MAC-eq per draw, measured the way the fold measures it.
    let row = |n_ctx: u32, held_qwen25: bool| -> u128 {
        let (profile, canonical) = if held_qwen25 {
            (
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_artifact_row_profile_v7(
                    kaspa_consensus_core::palw_qwen25_profile::PalwQwen25GeometryV1 {
                        n_ctx,
                        ..kaspa_consensus_core::palw_qwen25_profile::QWEN25_1_5B
                    },
                )
                .expect("2M profile"),
                kaspa_consensus_core::palw_qwen25_profile::qwen25_a16_held_canonical_v1(n_ctx),
            )
        } else {
            (
                kaspa_consensus_core::palw_qwen36_profile::qwen36_profile_v7(
                    kaspa_consensus_core::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                        kaspa_consensus_core::palw_qwen36_profile::PalwQwen36GeometryV1 {
                            n_ctx,
                            ..kaspa_consensus_core::palw_qwen36_profile::QWEN36_35B_A3B
                        },
                    ),
                )
                .expect("qwen36 profile"),
                kaspa_consensus_core::palw_qwen36_profile::qwen36_held_canonical_v1(n_ctx),
            )
        };
        let d = PalwCanonicalClassDescriptorV1::of(&profile, kaspa_consensus_core::Hash64::default()).expect("one weight format");
        let j = kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, canonical.0, canonical.1);
        palw_canonical_draw_work_v1(&d, &j, true).expect("derives").provisional_scalar_v1()
    };

    let renorm = |derived: u128| derived * floor_declared / floor_derived;

    let dense = renorm(row(kaspa_consensus_core::config::params::PALW_T12_DENSE_N_CTX, true));
    let hybrid = renorm(row(kaspa_consensus_core::config::params::PALW_T12_HYBRID_N_CTX, false));
    let floor = floor_declared; // the floor IS the basis: renorm is the identity on it

    println!("\n=== per-claim reservation, in the runtime's unit (x5 sompi) ===");
    for (name, pwu) in [("floor", floor), ("Qwen3.6@512", hybrid), ("Qwen2.5@2M", dense)] {
        println!("  {name:14} pwu={pwu:>16}  per_claim={:>16} sompi = {:>12.2} MSK", pwu * 5, (pwu * 5) as f64 / 1e8);
    }

    // The same shape as `palw_v2_collateral_for_class_set_v1`: the floor is really concurrent
    // (one claim a block for the whole exposure window), each model row is capped at its concurrency.
    let ceiling = floor * 5 * (MAX_CLAIM_EXPOSURE_DAA + 1) + (hybrid * 5 + dense * 5) * CONCURRENCY;
    let collateral = ceiling * 1000 / MAX_EXPOSURE_RATIO_PERMILLE;

    println!("\n=== corrected collateral ===");
    println!("  ceiling needed   {ceiling} sompi = {:.2} MSK", ceiling as f64 / 1e8);
    println!("  collateral       {collateral} sompi = {:.2} MSK", collateral as f64 / 1e8);
    println!("  shipped          {T12_WEDGED_COLLATERAL_SOMPI} sompi = {:.2} MSK", T12_WEDGED_COLLATERAL_SOMPI as f64 / 1e8);
    println!("  factor           {:.2}x", collateral as f64 / T12_WEDGED_COLLATERAL_SOMPI as f64);
    println!("  8 seats          {:.2} MSK  ({:.4}% of the 10B cap)", (collateral * 8) as f64 / 1e8, (collateral * 8) as f64 / 1e8 / 1e10 * 100.0);

    println!("\n=== the minimal alternative: one 2M claim in flight, at 500 permille ===");
    let one = dense * 5 * 1000 / MAX_EXPOSURE_RATIO_PERMILLE;
    println!("  collateral       {one} sompi = {:.2} MSK  ({:.2}x shipped)", one as f64 / 1e8, one as f64 / T12_WEDGED_COLLATERAL_SOMPI as f64);
}

/// **Why the collateral is sized on the RENORMALISED reservation and not on the raw fork weight.**
///
/// ADR-0151 D1 says collateral covers the fraud a Valid Final authorizes. Above the canonical-work
/// fence an attempt's pwu — and so the fork weight its Final buys — is
/// `palw_pwu_v1(target, derived_per_draw)` on the RAW MAC-eq draw. This test prints that number for
/// the 2M row next to the 10B supply cap, because it is the reason the exposure unit exists: the
/// raw weight of one 2M claim is not a quantity any bond can post.
#[test]
fn the_raw_fork_weight_of_a_2m_claim_is_not_collateralizable() {
    const CAP_SOMPI: u128 = 10_000_000_000u128 * 100_000_000; // 10B MSK
    let derived_per_draw: u128 = 3_357_281_757_221_376;
    let attempt_pwu = kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1(u128::MAX / 2, derived_per_draw);
    let weight = kaspa_consensus_core::palw_panel_var_v1::palw_fork_weight_sompi_v1(attempt_pwu, SLASH_VALUE_PER_PWU);
    println!("\n=== the 2M row's raw weight unit ===");
    println!("  derived MAC-eq per draw   {derived_per_draw}");
    println!("  attempt.pwu (above fence) {attempt_pwu}");
    println!("  fork weight               {weight} sompi = {:.2} MSK", weight as f64 / 1e8);
    println!("  as a share of the 10B cap {:.4}%", weight as f64 / CAP_SOMPI as f64 * 100.0);
    println!("  renormalised reservation  {LIVE_PER_CLAIM_SOMPI} sompi = {:.2} MSK", LIVE_PER_CLAIM_SOMPI as f64 / 1e8);
    println!("  ratio raw/renormalised    {:.0}x", weight as f64 / LIVE_PER_CLAIM_SOMPI as f64);
    assert!(
        weight > CAP_SOMPI / 1000,
        "if the raw weight were postable this test's premise would be wrong: {weight} against a cap of {CAP_SOMPI}"
    );
}
