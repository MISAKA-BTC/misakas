//! AUDIT LANE A1 — which pwu-checking rule is actually live on t12.
//! Run: cargo test -p kaspa-consensus-core --test audit_a1_fence_reachability -- --nocapture

use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::network::{NetworkId, NetworkType};

#[test]
fn a1_which_pwu_rule_is_live_on_t12() {
    let p = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    println!("\nt12 fences that decide which pwu rule runs:");
    println!("  palw_canonical_work          {:?}", p.palw_canonical_work);
    println!("  palw_prefill_draw            {:?}", p.palw_prefill_draw);
    println!("  palw_work_target             {:?}", p.palw_work_target);
    println!("  palw_model_registry          {:?}", p.palw_model_registry);
    println!("  palw_economic_payout         {:?}", p.palw_economic_payout);
    println!("  palw_attempt_activation      {:?}", p.palw_attempt_activation);
    println!("\nV1 PALW proof-of-work activations (the ONLY lane that reaches check_pwu_claim_v1):");
    println!("  pow_palw_activation          {:?}", p.pow_palw_activation);
    println!("  pow_palw_ollama_activation   {:?}", p.pow_palw_ollama_activation);
    println!("  pow_blake2b_sha3_activation  {:?}", p.pow_blake2b_sha3_activation);
    println!("\n  pow_palw_activation.is_active(0)               {}", p.pow_palw_activation.is_active(0));
    println!("  pow_palw_activation.is_active(u64::MAX-1)      {}", p.pow_palw_activation.is_active(u64::MAX - 1));
    println!("  pow_palw_ollama_activation.is_active(u64::MAX-1) {}", p.pow_palw_ollama_activation.is_active(u64::MAX - 1));
    println!("\n  palw_canonical_work_daa()    {:?}", p.palw_canonical_work_daa());
    println!("  palw_prefill_draw_active_at(0) {}", p.palw_prefill_draw_active_at(0));
    println!("\n  palw_block_commitment        {:?}  <- gates check_pwu_claim_v1's only non-test caller", p.palw_block_commitment);
}
