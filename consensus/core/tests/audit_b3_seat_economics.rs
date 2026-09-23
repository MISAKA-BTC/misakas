//! AUDIT-ONLY (lane B3). What one Valid signature must lock, at the colluding quorum the code
//! assumes (3) and at the quorum the S2 door actually admits (1). Read-only; delete freely.

use kaspa_consensus_core::config::params::palw_t12_shipped_params;
use kaspa_consensus_core::palw_admission_v2::palw_attempt_derived_pwu_v1;
use kaspa_consensus_core::palw_economic_safety_v1::palw_seat_lock_required_v2;
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
use kaspa_consensus_core::palw_pwu::palw_expected_attempts_v1;
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, PalwPwuRuleV2};

const MSK: f64 = 100_000_000.0;

#[test]
fn audit_b3_seat_lock_at_quorum_three_versus_one() {
    let p = palw_t12_shipped_params();
    let PalwConsensusMode::ConsensusV2(bundle) = &p.palw_consensus_mode else { panic!() };
    println!("PALW_PANEL_COLLUDING_QUORUM_V1 = {PALW_PANEL_COLLUDING_QUORUM_V1}");
    println!("palw_verification_s2 = {:?} (a licence on the full seat's Valid alone)", p.palw_verification_s2);
    println!("worker carve permille (fence) = {:?}", p.palw_overlay_carve.map(|c| c.worker_carve_permille));

    // the escrow a t12 claim carries, from the runtime subsidy the recon measured
    let subsidy: u128 = 444_562_014_000;
    let escrow: u128 = subsidy * 720 / 1000;
    println!("escrow per claim = {escrow} sompi = {:.2} MSK", escrow as f64 / MSK);

    for object in bundle.genesis_objects.iter() {
        let PalwConsensusObjectV2::ClassRegistered {
            class_id, admission, pwu_rule, initial_target, slash_value_per_pwu, ..
        } = object
        else {
            continue;
        };
        let PalwPwuRuleV2::DerivedV1 { pwu_per_inference } = pwu_rule else { continue };
        let per_draw: u128 = match admission.as_ref() {
            Some(c) => {
                // the derived work of one draw, exactly as the registry row computes it
                kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&c.profile, &c.canonical)
                    .map(|w| w.economic_ccu_per_claim)
                    .unwrap_or(0)
            }
            None => u128::from(*pwu_per_inference),
        };
        let attempts = palw_expected_attempts_v1(*initial_target);
        let pwu = palw_attempt_derived_pwu_v1(*initial_target, per_draw);
        let fork_weight_sompi = u128::from(pwu) * u128::from(*slash_value_per_pwu);
        let gain = fork_weight_sompi + escrow; // weight half + cash half; extra rights excluded
        let at3 = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        let at1 = palw_seat_lock_required_v2(gain, 1);
        let posted: u128 = 51_642_979_663_480;
        println!(
            "\nclass {}\n  declared leaves {pwu_per_inference}\n  derived per draw {per_draw} MAC-eq\n  \
             initial_target {initial_target}\n  expected_attempts {attempts}  claim.pwu {pwu}\n  \
             fork weight {fork_weight_sompi} sompi = {:.2} MSK\n  max_fraud_gain(no extra rights) {gain} sompi = {:.2} MSK\n  \
             seat lock @quorum 3 = {at3} sompi = {:.2} MSK  ({:.2}x posted)\n  \
             seat lock @quorum 1 = {at1} sompi = {:.2} MSK  ({:.2}x posted)\n  \
             posted collateral per genesis bond = {posted} sompi = {:.2} MSK",
            &class_id.to_string()[..16],
            fork_weight_sompi as f64 / MSK,
            gain as f64 / MSK,
            at3 as f64 / MSK,
            at3 as f64 / posted as f64,
            at1 as f64 / MSK,
            at1 as f64 / posted as f64,
            posted as f64 / MSK,
        );
    }
}
