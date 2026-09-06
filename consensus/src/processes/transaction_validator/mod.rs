pub mod errors;
pub mod tx_validation_in_header_context;
pub mod tx_validation_in_isolation;
pub mod tx_validation_in_utxo_context;
use std::sync::Arc;

use kaspa_txscript::{
    SigCacheKey,
    caches::{Cache, TxScriptCacheCounters},
};

use kaspa_consensus_core::{config::params::PqEnforcementMode, mass::MassCalculator};
use kaspa_txscript::ScriptPolicy;

#[derive(Clone)]
pub struct TransactionValidator {
    max_tx_inputs: usize,
    max_tx_outputs: usize,
    max_signature_script_len: usize,
    max_script_public_key_len: usize,
    coinbase_payload_script_public_key_max_len: u8,
    coinbase_maturity: u64,
    /// **The mergeset's own bound, because the coinbase carries one output per entitled RED**
    /// (ADR-0058; mainnet audit 2026-09-05). `ghostdag_k` bounds the blues; nothing bounds the
    /// reds but this, and the isolation guard was sized as though the reds shared one aggregate
    /// output — the shape they had before ADR-0058 paid each of them its own.
    mergeset_size_limit: u64,
    sig_cache: Cache<SigCacheKey, bool>,
    pub(crate) mass_calculator: MassCalculator,
    /// kaspa-pq PQ-only enforcement mode for this network (ADR-0019).
    pq_enforcement: PqEnforcementMode,
    /// DAA score at/after which `PqEnforcementMode::Consensus` takes effect.
    pq_activation_daa_score: u64,
    /// **`Params::palw_panel_da_admissible()`** — whether this ruleset carries ADR-0077 Decision
    /// 16's mode-2 commitment SHAPE at all. The isolation door is context-free by contract and
    /// holds no DAA score, so it asks the height-free question; the extraction walk asks the
    /// height-indexed one (`palw_panel_da_at`), which is strictly stronger.
    palw_panel_da_admissible: bool,
    /// **`Params::palw_prompt_ids_form_v1()`** — which commitment a commitment's carried prompt ids
    /// must hash to (ADR-0081 Decision 3). Genesis-only by `validate_palw_v2`, so height-free here
    /// is exact, not an approximation.
    palw_prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    /// ADR-0087 Decision 3: whether a model market's sink output (`OP_RETURN "MSKMDL01" <line id>`)
    /// is a consensus-legal output class here — `true` iff the network declares `palw_model_market`
    /// at all; a dormant (`None`) network keeps the PQ-only rule byte-for-byte.
    model_sink_outputs_allowed: bool,
}

impl TransactionValidator {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        mergeset_size_limit: u64,
        counters: Arc<TxScriptCacheCounters>,
        mass_calculator: MassCalculator,
        pq_enforcement: PqEnforcementMode,
        pq_activation_daa_score: u64,
        palw_panel_da_admissible: bool,
        palw_prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1,
        model_sink_outputs_allowed: bool,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            mergeset_size_limit,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator,
            pq_enforcement,
            pq_activation_daa_score,
            palw_panel_da_admissible,
            palw_prompt_ids_form,
            model_sink_outputs_allowed,
        }
    }

    pub fn new_for_tests(
        max_tx_inputs: usize,
        max_tx_outputs: usize,
        max_signature_script_len: usize,
        max_script_public_key_len: usize,
        coinbase_payload_script_public_key_max_len: u8,
        coinbase_maturity: u64,
        mergeset_size_limit: u64,
        counters: Arc<TxScriptCacheCounters>,
    ) -> Self {
        Self {
            max_tx_inputs,
            max_tx_outputs,
            max_signature_script_len,
            max_script_public_key_len,
            coinbase_payload_script_public_key_max_len,
            coinbase_maturity,
            mergeset_size_limit,
            sig_cache: Cache::with_counters(10_000, counters),
            mass_calculator: MassCalculator::new(0, 0, 0, 0),
            // Tests run upstream-compatible (no PQ restriction) unless a test
            // explicitly exercises PQ-only via the script engine directly.
            pq_enforcement: PqEnforcementMode::Disabled,
            pq_activation_daa_score: 0,
            // Every shipped preset's door: no mode-2 shape, flat prompt digests.
            palw_panel_da_admissible: false,
            palw_prompt_ids_form: kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            model_sink_outputs_allowed: false,
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
