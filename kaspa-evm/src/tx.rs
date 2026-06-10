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

/// Metadata of an admitted EVM transaction (the fields a mempool needs to key,
/// order, replace, and select it). Produced by [`admit_tx_info`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedEvmTx {
    /// keccak256 over the raw EIP-2718 bytes — the Ethereum tx hash.
    pub hash: kaspa_hashes::EvmH256,
    /// Recovered ECDSA signer.
    pub sender: kaspa_consensus_core::evm::EvmAddress,
    pub nonce: u64,
    pub gas_limit: u64,
    /// EIP-1559 `max_fee_per_gas` (legacy/2930: the gas price) — the mempool's
    /// fee-ordering key.
    pub max_fee_per_gas: u128,
}

/// v0.4 §6.1 class-1 payload admission (syntactic, per tx): EIP-2718 decode +
/// ECDSA signer recovery + chain-id binding + a declared gas-limit sanity band
/// (≥ the 21k intrinsic floor, +32k for creates; ≤ the per-chain-block accepted
/// gas cap, since a never-acceptable tx is not includable data). Runs at body
/// validation, where a violation invalidates the PAYLOAD block itself — the
/// producer chose its own payload (design v0.4 §6.2). Deterministic and
/// context-free (no state, no basefee: those are class-2 acceptance skips).
pub fn admit_tx(raw: &[u8]) -> Result<(), String> {
    admit_tx_info(raw).map(|_| ())
}

/// [`admit_tx`] returning the admitted tx's metadata (§16 EVM mempool: the
/// SAME rule the body-validation class-1 check applies, so a mempool-admitted
/// tx can never make the node's own template payload-block-invalid).
pub fn admit_tx_info(raw: &[u8]) -> Result<AdmittedEvmTx, String> {
    use kaspa_consensus_core::evm::{EvmAddress, EVM_CHAIN_ID, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK};

    let envelope = TxEnvelope::decode_2718(&mut &raw[..]).map_err(|e| format!("decode: {e}"))?;
    let sender = envelope.recover_signer().map_err(|e| format!("signer recovery: {e}"))?;
    match envelope.chain_id() {
        Some(EVM_CHAIN_ID) => {}
        other => return Err(format!("chain_id {other:?} != EVM_CHAIN_ID {EVM_CHAIN_ID}")),
    }
    let intrinsic_floor = if envelope.kind().is_create() { 53_000 } else { 21_000 };
    if envelope.gas_limit() < intrinsic_floor {
        return Err(format!("gas_limit {} below the intrinsic floor {intrinsic_floor}", envelope.gas_limit()));
    }
    if envelope.gas_limit() > MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK {
        return Err(format!(
            "gas_limit {} exceeds MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK {MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK}",
            envelope.gas_limit()
        ));
    }
    Ok(AdmittedEvmTx {
        hash: kaspa_hashes::EvmH256::from_bytes(revm::primitives::keccak256(raw).0),
        sender: EvmAddress::from_bytes(sender.into_array()),
        nonce: envelope.nonce(),
        gas_limit: envelope.gas_limit(),
        max_fee_per_gas: envelope.max_fee_per_gas(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_raw(nonce: u64) -> Vec<u8> {
        use alloy_consensus::{SignableTransaction, TxEip1559};
        use alloy_eips::eip2718::Encodable2718;
        use alloy_signer::SignerSync;
        use alloy_signer_local::PrivateKeySigner;
        use kaspa_consensus_core::evm::{EVM_CHAIN_ID, EVM_INITIAL_BASE_FEE};
        use revm::primitives::{Address, TxKind, B256, U256};
        let signer = PrivateKeySigner::from_bytes(&B256::from([0x11u8; 32])).unwrap();
        let tx = TxEip1559 {
            chain_id: EVM_CHAIN_ID,
            nonce,
            gas_limit: 21_000,
            max_fee_per_gas: EVM_INITIAL_BASE_FEE as u128,
            max_priority_fee_per_gas: 0,
            to: TxKind::Call(Address::with_last_byte(0x22)),
            value: U256::from(500u64),
            access_list: Default::default(),
            input: Default::default(),
        };
        let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        TxEnvelope::from(tx.into_signed(sig)).encoded_2718()
    }

    #[test]
    fn admit_tx_info_extracts_the_mempool_metadata() {
        let raw = fixture_raw(7);
        let info = admit_tx_info(&raw).unwrap();
        assert_eq!(info.nonce, 7);
        assert_eq!(info.gas_limit, 21_000);
        assert_eq!(info.max_fee_per_gas, kaspa_consensus_core::evm::EVM_INITIAL_BASE_FEE as u128);
        assert_eq!(info.hash.as_bytes(), revm::primitives::keccak256(&raw).0, "Ethereum tx hash = keccak256(raw 2718 bytes)");
        // admit_tx and admit_tx_info enforce the identical rule.
        assert!(admit_tx(&raw).is_ok());
        // A truncated tx is inadmissible, not a panic.
        assert!(admit_tx_info(&raw[..raw.len() - 5]).is_err());
    }

    /// Prints the canonical signed-tx fixture used by the consensus §16 e2e
    /// test (consensus has no signing deps, so it embeds these bytes as hex).
    /// Regenerate with:
    ///   cargo test -p kaspa-evm fixture_generator -- --ignored --nocapture
    #[test]
    #[ignore = "fixture generator, run with --ignored --nocapture"]
    fn fixture_generator() {
        for nonce in [0u64, 1] {
            let raw = fixture_raw(nonce);
            println!("nonce {nonce}: {}", alloy_primitives::hex::encode(&raw));
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
