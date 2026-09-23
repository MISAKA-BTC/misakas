//! **AUDIT LANE A4 (part 2) — the seat lock a t12 class demands against the collateral posted.**
//!
//! `panel_valid_lock_required` (palw_state_v2.rs:9334) is checked on the `PanelBound` edge
//! (palw_state_v2.rs:16142, `require_panel_lock_eligible`) and again on every Valid receipt
//! (`lock_valid_receipts`, :11186/:16205/:16609/:16635). Its weight term is
//! `palw_fork_weight_sompi_v1(claim.pwu, slash_value_per_pwu)` — `claim.pwu` in DERIVED MAC-eq
//! past `palw_canonical_work`, against a constant calibrated in DECLARED leaves.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a4_seat_locks -- --nocapture

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_economic_safety_v1::{
    PALW_T12_PERMIT_FEE_CEILING_SOMPI, palw_realizable_before_maturity_v1, palw_seat_lock_required_v2,
};
use kaspa_consensus_core::palw_execution_quanta_v1::{PALW_EXECUTION_QUANTUM_V1, palw_execution_quantum_count_v1};
use kaspa_consensus_core::palw_mode_v2::PalwConsensusMode;
use kaspa_consensus_core::palw_model_registry_v1::palw_genesis_model_works_v1;
use kaspa_consensus_core::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};
use kaspa_consensus_core::palw_pwu::{palw_expected_attempts_v1, palw_pwu_v1};
use kaspa_consensus_core::palw_state_v2::{PalwConsensusObjectV2, palw_max_exposure_pwu_of_rule_v1};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const POSTED_COLLATERAL_SOMPI: u128 = 51_642_979_663_480;

fn t12() -> Params {
    Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12))
}

#[test]
fn the_seat_lock_each_t12_class_demands_against_the_collateral_that_is_posted() {
    let params = t12();
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("ConsensusV2") };
    let works = palw_genesis_model_works_v1(&bundle.genesis_objects);
    let base = bundle.base_class_id;

    // The floor's basis, from the build's own canonical floor profile.
    let fp = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .unwrap();
    let (p, d) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let fj = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, p, d);
    let fw = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &fj).unwrap();
    let base_canonical = fw.economic_ccu_per_claim.min(u64::MAX as u128) as u64;

    let carve = params.palw_overlay_carve.expect("t12 arms the carve").worker_carve_permille as u64;
    let escrow = T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve;

    println!("\n=== Q3: is the ADR-0033 credit lane alive on t12? ===");
    println!("  params.palw_credit = {:?}  <- decide_credit_v1 / panel_seats_at_anchor_v3 are DEAD here", params.palw_credit.is_some());
    assert!(params.palw_credit.is_none(), "t12 carries no PalwCreditParamsV1");

    println!("\n=== posted collateral per genesis bond: {POSTED_COLLATERAL_SOMPI} sompi = {:.2} MSK ===", POSTED_COLLATERAL_SOMPI as f64 / 1e8);
    println!("\n  {:<22} {:>20} {:>18} {:>18} {:>12} {:>10}", "class", "max_fraud_gain(MSK)", "seat lock (MSK)", "lock/posted", "claims/seat", "verdict");

    let mut base_declared = 0u64;
    for object in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, .. } = object
            && *class_id == base
        {
            base_declared = palw_max_exposure_pwu_of_rule_v1(pwu_rule);
        }
    }

    for object in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, initial_target, slash_value_per_pwu, .. } = object else {
            continue;
        };
        let declared = palw_max_exposure_pwu_of_rule_v1(pwu_rule);
        let u2 = works.get(class_id).map(|w| w.economic_ccu_per_claim.min(u64::MAX as u128) as u64).unwrap_or(base_canonical);
        let u3 = ((u2 as u128) * (base_declared as u128) / (base_canonical as u128)).min(u64::MAX as u128) as u64;
        let claim_pwu = palw_pwu_v1(*initial_target, u2);
        let quanta = palw_execution_quantum_count_v1(
            u128::from(u2),
            u128::from(PALW_EXECUTION_QUANTUM_V1),
            Hash64::default(),
            Hash64::default(),
        );
        let realizable = palw_realizable_before_maturity_v1(
            quanta.saturating_add(1),
            bundle.state.window_challenge(),
            bundle.state.window_court(),
            params.target_time_per_block_history().after(),
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        // Exactly the runtime's facts: claim.pwu in MAC-eq, claim.reserved in normalised U3.
        let facts = PalwClaimFraudFactsV1 {
            reserved: (u3 as u128) * (*slash_value_per_pwu as u128),
            escrowed_reward: escrow,
            exposure_pwu: claim_pwu,
            slash_value_per_pwu: *slash_value_per_pwu,
            extra_economic_rights_sompi: realizable,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let lock = palw_seat_lock_required_v2(gain, 3);
        let ratio = lock as f64 / POSTED_COLLATERAL_SOMPI as f64;
        let concurrent = POSTED_COLLATERAL_SOMPI / lock.max(1);
        let name = if *class_id == base {
            "BASE-0 floor"
        } else if declared > 1_000_000_000 {
            "Qwen2.5 dense @2M"
        } else {
            "Qwen3.6 hybrid @512"
        };
        println!(
            "  {name:<22} {:>20.2} {:>18.2} {:>17.2}x {:>12} {:>10}",
            gain as f64 / 1e8,
            lock as f64 / 1e8,
            ratio,
            concurrent,
            if lock as u128 <= POSTED_COLLATERAL_SOMPI { "bindable" } else { "REFUSED" }
        );

        // Counterfactual: the same gain with the weight term priced in the unit the reservation
        // and the genesis carve both use (U3), instead of raw MAC-eq.
        let facts_u3 = PalwClaimFraudFactsV1 { exposure_pwu: u3, ..facts };
        let gain_u3 = palw_max_fraud_gain_v1(&facts_u3);
        let lock_u3 = palw_seat_lock_required_v2(gain_u3, 3);
        println!(
            "        counterfactual (pwu in U3): gain {:.2} MSK, lock {:.2} MSK, {:>6.2}x posted -> {}",
            gain_u3 as f64 / 1e8,
            lock_u3 as f64 / 1e8,
            lock_u3 as f64 / POSTED_COLLATERAL_SOMPI as f64,
            if lock_u3 as u128 <= POSTED_COLLATERAL_SOMPI { "bindable" } else { "REFUSED" }
        );
        println!(
            "        terms: cash {:.2} MSK + weight {:.2} MSK + rights {:.2} MSK   (reserved, not added: {:.2} MSK)",
            escrow as f64 / 1e8,
            (claim_pwu as u128 * *slash_value_per_pwu as u128) as f64 / 1e8,
            realizable as f64 / 1e8,
            facts.reserved as f64 / 1e8
        );
    }

    println!("\n  window_court = {} DAA; a lock lives that long (palw_panel_liability_expiry_v1)", bundle.state.window_court());
    println!("  target_time_per_block = {} ms -> a lock holds for {:.1} hours", params.target_time_per_block_history().after(), bundle.state.window_court() as f64 * params.target_time_per_block_history().after() as f64 / 3_600_000.0);
}
