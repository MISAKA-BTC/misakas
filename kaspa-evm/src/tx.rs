//! EIP-2718 typed-transaction decoding → revm `TxEnv`, with secp/k256 sender
//! recovery (design §3.1: `evm_payload.transactions` are EIP-2718 bytes).
//!
//! A tx whose bytes fail to decode or whose signature fails to recover is simply
//! not includable — the producer must not put it in a block. The block stays
//! valid (only producer commitment/diff faults invalidate a block, design §6.3);
//! a syntactic encoding pre-check lives in body validation (P3). User execution
//! failures (revert / OOG / bad nonce) are receipts with `status = 0`, never
//! block-invalid (§8.2).

use alloy_consensus::{Transaction as _, TxEnvelope};
use alloy_eips::eip2718::Decodable2718;
use revm::primitives::{TxEnv, U256};

/// Why a raw EVM transaction could not be turned into an executable `TxEnv`.
#[derive(Debug, Clone)]
pub enum TxDecodeError {
    /// EIP-2718 / RLP decoding failed.
    Decode(String),
    /// ECDSA signer recovery failed.
    Recover(String),
}

impl std::fmt::Display for TxDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TxDecodeError::Decode(e) => write!(f, "evm tx decode: {e}"),
            TxDecodeError::Recover(e) => write!(f, "evm tx signer recovery: {e}"),
        }
    }
}

/// Decode one EIP-2718 typed-transaction byte string and map it to a revm
/// `TxEnv` (recovering the sender). Deterministic: the same bytes always yield
/// the same caller + env.
pub fn decode_tx_to_env(raw: &[u8]) -> Result<TxEnv, TxDecodeError> {
    let envelope = TxEnvelope::decode_2718(&mut &raw[..]).map_err(|e| TxDecodeError::Decode(e.to_string()))?;
    let caller = envelope.recover_signer().map_err(|e| TxDecodeError::Recover(e.to_string()))?;

    let mut tx = TxEnv::default();
    tx.caller = caller;
    tx.gas_limit = envelope.gas_limit();
    // For EIP-1559, `gas_price` carries the max fee and `gas_priority_fee` the
    // priority tip; for legacy/2930, `max_fee_per_gas` returns the gas price and
    // the priority fee is `None`.
    tx.gas_price = U256::from(envelope.max_fee_per_gas());
    tx.gas_priority_fee = envelope.max_priority_fee_per_gas().map(U256::from);
    tx.transact_to = envelope.kind();
    tx.value = envelope.value();
    tx.data = envelope.input().clone();
    tx.nonce = Some(envelope.nonce());
    tx.chain_id = envelope.chain_id();
    // TODO(P5): carry the EIP-2930/1559 access list so `gas_used` is exact for
    // access-list txs. Pre-activation refinement; current test/standard transfers
    // and calls carry none.
    Ok(tx)
}
