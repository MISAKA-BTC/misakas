//! EVM block-environment derivation (design §4). Every input is ancestor-derived
//! — the `selected_parent`'s committed `EvmExecutionHeader` plus this block's L1
//! header — so the env (and therefore the committed EVM result) is a pure
//! function of the block. The current L1/EVM block hashes are intentionally NOT
//! inputs (they already commit to the EVM result — design §4.2 circularity rule).

use kaspa_consensus_core::evm::{
    EvmExecutionHeader, EVM_BASE_FEE_MAX_CHANGE_DENOMINATOR, EVM_CHAIN_ID, EVM_ELASTICITY_MULTIPLIER, EVM_GAS_LIMIT,
    EVM_INITIAL_BASE_FEE, MISAKA_EVM_PREVRANDAO_CONTEXT,
};
use kaspa_hashes::blake2b_256_keyed;
use revm::primitives::{Address, B256};

/// The deterministic EVM block environment for one block (design §4.1).
#[derive(Clone, Debug)]
pub struct EvmDerivedEnv {
    pub evm_number: u64,
    pub evm_timestamp_sec: u64,
    pub base_fee_per_gas: u128,
    pub prev_randao: B256,
    pub coinbase: Address,
    pub gas_limit: u64,
    pub chain_id: u64,
}

/// Derive the env for a block from its `selected_parent`'s EVM header (`None`
/// for the first EVM block, whose parent is the EVM genesis at number 0) and
/// this block's L1 header context.
pub fn derive_env(
    parent: Option<&EvmExecutionHeader>,
    header_timestamp_ms: u64,
    selected_parent_hash: &[u8; 64],
    blue_work_be: &[u8],
    daa_score: u64,
    coinbase: Address,
) -> EvmDerivedEnv {
    let (parent_number, parent_ts, base_fee_per_gas) = match parent {
        Some(p) => (p.evm_number, p.evm_timestamp_sec, next_base_fee(evmu256_to_u128(p.base_fee_per_gas), p.gas_used)),
        None => (0, 0, EVM_INITIAL_BASE_FEE as u128),
    };

    // Strict-monotone EVM logical time (design §4.1 / §15.3): never < parent+1.
    let header_sec = header_timestamp_ms / 1000;
    let evm_timestamp_sec = header_sec.max(parent_ts + 1);

    // prevrandao = keyed-BLAKE2b-256(domain, selected_parent_hash ‖ blue_work ‖ daa_score)
    // (design §4.3). FROZEN byte order. Grindable, not secure randomness.
    let mut preimage = Vec::with_capacity(64 + blue_work_be.len() + 8);
    preimage.extend_from_slice(selected_parent_hash);
    preimage.extend_from_slice(blue_work_be);
    preimage.extend_from_slice(&daa_score.to_le_bytes());
    let prev_randao = B256::from(blake2b_256_keyed(MISAKA_EVM_PREVRANDAO_CONTEXT, &preimage));

    EvmDerivedEnv {
        evm_number: parent_number + 1,
        evm_timestamp_sec,
        base_fee_per_gas,
        prev_randao,
        coinbase,
        gas_limit: EVM_GAS_LIMIT,
        chain_id: EVM_CHAIN_ID,
    }
}

/// EIP-1559 base-fee update from the parent block (design §5.3, P2 fixed-limit
/// form). Integer math, deterministic.
pub fn next_base_fee(parent_base_fee: u128, parent_gas_used: u64) -> u128 {
    let gas_target = (EVM_GAS_LIMIT / EVM_ELASTICITY_MULTIPLIER) as u128;
    let denom = EVM_BASE_FEE_MAX_CHANGE_DENOMINATOR as u128;
    let used = parent_gas_used as u128;
    if used == gas_target {
        parent_base_fee
    } else if used > gas_target {
        // Increase, by at least 1 wei.
        let delta = (parent_base_fee.saturating_mul(used - gas_target) / gas_target / denom).max(1);
        parent_base_fee.saturating_add(delta)
    } else {
        // Decrease.
        let delta = parent_base_fee.saturating_mul(gas_target - used) / gas_target / denom;
        parent_base_fee.saturating_sub(delta)
    }
}

/// The committed base fee fits a `u128` (1 gwei seed × bounded EIP-1559 drift);
/// a (spec-impossible) overflow saturates rather than panicking the verifier.
fn evmu256_to_u128(v: kaspa_consensus_core::evm::EvmU256) -> u128 {
    v.try_to_u128().unwrap_or(u128::MAX)
}
