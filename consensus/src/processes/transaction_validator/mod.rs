pub mod errors;
pub mod tx_validation_in_header_context;
pub mod tx_validation_in_isolation;
pub mod tx_validation_in_utxo_context;
use std::sync::Arc;

use kaspa_txscript::{
    SigCacheKey,
    caches::{Cache, TxScriptCacheCounters},
};

use kaspa_consensus_core::{KType, config::params::PqEnforcementMode, mass::MassCalculator};
use kaspa_txscript::ScriptPolicy;

#[derive(Clone)]
pub struct TransactionValidator {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
    coinbase_payload_script_public_key_max_len: u8,
    coinbase_maturity: u64,
    ghostdag_k: KType,
    sig_cache: Cache<SigCacheKey, bool>,
    pub(crate) mass_calculator: MassCalculator,
    /// kaspa-pq PQ-only enforcement mode for this network (ADR-0019).
    pq_enforcement: PqEnforcementMode,
    /// DAA score at/after which `PqEnforcementMode::Consensus` takes effect.
    pq_activation_daa_score: u64,
}

impl TransactionValidator {
    pub fn new(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        ghostdag_k: KType,
        counters: Arc<TxScriptCacheCounters>,
        mass_calculator: MassCalculator,
        pq_enforcement: PqEnforcementMode,
        pq_activation_daa_score: u64,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            ghostdag_k,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator,
            pq_enforcement,
            pq_activation_daa_score,
        }
    }

    pub fn new_for_tests(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        ghostdag_k: KType,
        counters: Arc<TxScriptCacheCounters>,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            ghostdag_k,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator: MassCalculator::new(0, 0, 0, 0),
            // Tests run upstream-compatible (no PQ restriction) unless a test
            // explicitly exercises PQ-only via the script engine directly.
            pq_enforcement: PqEnforcementMode::Disabled,
            pq_activation_daa_score: 0,
        }
    }

    /// kaspa-pq: resolve the [`ScriptPolicy`] to apply at `pov_daa_score`.
    /// `PQ_ONLY` once PQ-only enforcement is active (legacy secp256k1 opcodes +
    /// P2SH become hard errors), else `LEGACY` (upstream-identical). See ADR-0019.
    pub(crate) fn resolved_script_policy(&self, pov_daa_score: u64) -> ScriptPolicy {
        if matches!(self.pq_enforcement, PqEnforcementMode::Consensus) && pov_daa_score >= self.pq_activation_daa_score {
            ScriptPolicy::PQ_ONLY
        } else {
            ScriptPolicy::LEGACY
        }
    }
}
