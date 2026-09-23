//! **AUDIT LANE A4 (part 3) — the steady state, once `palw_work_target` has stepped W to its floor.**
//!
//! `palw_work_target_step_v1` never returns below `floor = palw_work_floor_v1(escrow, rate)`, and
//! `palw_work_ticket_target_v1(ccu, W) = MAX · min(1, CCU/W)`. So W = W₀ is the LOOSEST the targets
//! ever get, and therefore the largest `expected_attempts`-independent permit yield per unit of real
//! compute. This file measures the steady state, not the genesis targets.
//!
//! Run: cargo test -p kaspa-consensus-core --test audit_a4_worktarget -- --nocapture

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
use kaspa_consensus_core::palw_work_target_v1::{palw_work_floor_v1, palw_work_ticket_target_v1};

const T12_BLOCK_SUBSIDY_SOMPI: u64 = 444_562_014_000;
const POSTED_COLLATERAL_SOMPI: u128 = 51_642_979_663_480;

#[test]
fn the_steady_state_permit_yield_and_seat_lock_per_t12_class() {
    let params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { panic!("ConsensusV2") };
    let works = palw_genesis_model_works_v1(&bundle.genesis_objects);
    let base = bundle.base_class_id;

    let fp = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
        kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
    )
    .unwrap();
    let (p, d) = kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_CANONICAL;
    let fj = kaspa_consensus_core::palw_base0_profile::rc_job_context(&fp, p, d);
    let fw = kaspa_consensus_core::palw_model_registry_v1::palw_model_work_from_carriage_v1(&fp, &fj).unwrap();
    let base_canonical = fw.economic_ccu_per_claim.min(u64::MAX as u128) as u64;

    let carve = params.palw_overlay_carve.expect("carve").worker_carve_permille as u64;
    let escrow = T12_BLOCK_SUBSIDY_SOMPI / 1_000 * carve;
    let rate = params.palw_economic_payout.expect("payout").rate_sompi_per_giga;
    let w0 = palw_work_floor_v1(escrow, rate);

    println!("\n=== W0 = escrow x 10^9 / rate ===");
    println!("  escrow {escrow} sompi, rate {rate} sompi/G MAC-eq");
    println!("  W0     {w0} MAC-eq (CCU) = {:.2} G", w0 as f64 / 1e9);

    let mut base_declared = 0u64;
    for o in &bundle.genesis_objects {
        if let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, .. } = o
            && *class_id == base
        {
            base_declared = palw_max_exposure_pwu_of_rule_v1(pwu_rule);
        }
    }

    println!("\n=== steady state (W = W0), per class ===");
    for o in &bundle.genesis_objects {
        let PalwConsensusObjectV2::ClassRegistered { class_id, pwu_rule, slash_value_per_pwu, .. } = o else { continue };
        let declared = palw_max_exposure_pwu_of_rule_v1(pwu_rule);
        let ccu = works.get(class_id).map(|w| w.economic_ccu_per_claim).unwrap_or(u128::from(base_canonical));
        let u2 = ccu.min(u64::MAX as u128) as u64;
        let u3 = (ccu * (base_declared as u128) / (base_canonical as u128)).min(u64::MAX as u128) as u64;
        let target = palw_work_ticket_target_v1(ccu, w0);
        let attempts = palw_expected_attempts_v1(target);
        let claim_pwu = palw_pwu_v1(target, u2);
        let real_work = (attempts as u128).saturating_mul(ccu); // MAC-eq actually executed
        let quanta = palw_execution_quantum_count_v1(ccu, u128::from(PALW_EXECUTION_QUANTUM_V1), Hash64::default(), Hash64::default());
        let realizable = palw_realizable_before_maturity_v1(
            quanta.saturating_add(1),
            bundle.state.window_challenge(),
            bundle.state.window_court(),
            params.target_time_per_block_history().after(),
            PALW_T12_PERMIT_FEE_CEILING_SOMPI,
        );
        let facts = PalwClaimFraudFactsV1 {
            reserved: (u3 as u128) * (*slash_value_per_pwu as u128),
            escrowed_reward: escrow,
            exposure_pwu: claim_pwu,
            slash_value_per_pwu: *slash_value_per_pwu,
            extra_economic_rights_sompi: realizable,
        };
        let gain = palw_max_fraud_gain_v1(&facts);
        let lock = palw_seat_lock_required_v2(gain, 3);
        // Nominal permit value at the declared ceiling, against the escrow the same claim releases.
        let permit_value = u128::from(quanta) * u128::from(PALW_T12_PERMIT_FEE_CEILING_SOMPI);
        let name = if *class_id == base {
            "BASE-0 floor"
        } else if declared > 1_000_000_000 {
            "Qwen2.5 dense @2M"
        } else {
            "Qwen3.6 hybrid @512"
        };
        println!("\n  --- {name} ---");
        println!("    CCU per draw (U2)        {ccu}");
        println!("    ticket target            {target}  (= {:.6} x MAX)", target as f64 / u128::MAX as f64);
        println!("    expected_attempts        {attempts}");
        println!("    real compute per claim   {real_work} MAC-eq = {:.3} G", real_work as f64 / 1e9);
        println!("    claim.pwu (weight)       {claim_pwu}");
        println!("    [cash]   escrow          {escrow} sompi = {:.2} MSK (HARD CAP)", escrow as f64 / 1e8);
        println!("    [permit] quanta minted   {quanta}");
        println!("             nominal value   {permit_value} sompi = {:.2} MSK  ({:.2}x the escrow, UNCAPPED)", permit_value as f64 / 1e8, permit_value as f64 / escrow as f64);
        println!("    [weight] fork weight     {} sompi = {:.2} MSK", claim_pwu as u128 * *slash_value_per_pwu as u128, (claim_pwu as u128 * *slash_value_per_pwu as u128) as f64 / 1e8);
        println!("    seat lock required       {lock} sompi = {:.2} MSK  ({:.2}x posted) -> {}", lock as f64 / 1e8, lock as f64 / POSTED_COLLATERAL_SOMPI as f64, if lock <= POSTED_COLLATERAL_SOMPI { "bindable" } else { "REFUSED" });
        println!("    --- per unit of REAL work ---");
        println!("      permits per G MAC-eq   {:.6}", quanta as f64 / (real_work as f64 / 1e9));
        println!("      cash MSK per G MAC-eq  {:.6}", (escrow as f64 / 1e8) / (real_work as f64 / 1e9));
        println!("      permit MSK per G MAC-eq {:.6}", (permit_value as f64 / 1e8) / (real_work as f64 / 1e9));
        println!("      TOTAL MSK per G MAC-eq  {:.6}", ((escrow as u128 + permit_value) as f64 / 1e8) / (real_work as f64 / 1e9));
    }
}
