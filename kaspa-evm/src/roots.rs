//! Commitment roots for the `EvmExecutionHeader` (design §3.2/§3.3):
//! - MISAKA keyed-BLAKE2b-256 roots over the ordered system-ops / withdrawals /
//!   applied-deposit-claims lists (domain-separated, §3.3).
//! - Ethereum keccak256 ordered tries for the transactions and receipts.
//! - The 2048-bit Ethereum logs bloom.

use kaspa_consensus_core::evm::{
    DepositClaim, EvmReceipt, EvmSystemOp, WithdrawOp, EVM_BLOOM_SIZE, MISAKA_EVM_DEPOSIT_CLAIM_CONTEXT,
    MISAKA_EVM_SYSTEM_OPS_CONTEXT, MISAKA_EVM_WITHDRAWAL_CONTEXT,
};
use kaspa_hashes::{blake2b_256_keyed, EvmH256};
use revm::primitives::keccak256;

/// keyed-BLAKE2b-256 over the borsh encoding of an ordered list under `domain`.
/// An empty list hashes the borsh length prefix → a fixed, domain-separated root.
fn keyed_list_root<T: borsh::BorshSerialize>(domain: &[u8], items: &[T]) -> EvmH256 {
    let bytes = borsh::to_vec(items).expect("borsh of a slice is infallible");
    EvmH256::from_bytes(blake2b_256_keyed(domain, &bytes))
}

/// MISAKA `system_ops_root` over the ordered system ops (payload order, §7.3).
pub fn system_ops_root(ops: &[EvmSystemOp]) -> EvmH256 {
    keyed_list_root(MISAKA_EVM_SYSTEM_OPS_CONTEXT, ops)
}

/// MISAKA `withdrawals_root` over the ordered withdrawal ops (§8.4).
pub fn withdrawals_root(ws: &[WithdrawOp]) -> EvmH256 {
    keyed_list_root(MISAKA_EVM_WITHDRAWAL_CONTEXT, ws)
}

/// MISAKA `deposit_claim_queue_root` over the applied deposit claims.
pub fn deposit_claim_root(cs: &[DepositClaim]) -> EvmH256 {
    keyed_list_root(MISAKA_EVM_DEPOSIT_CLAIM_CONTEXT, cs)
}

/// Ethereum ordered (index-keyed) keccak256 MPT root over the raw EIP-2718
/// transaction bytes (eth-faithful `transactions_root`).
pub fn transactions_root(raw_txs: &[Vec<u8>]) -> EvmH256 {
    let root = alloy_trie::root::ordered_trie_root_with_encoder(raw_txs, |tx, buf| buf.extend_from_slice(tx));
    EvmH256::from_bytes(root.0)
}

/// Receipts root. P2 commits a deterministic borsh encoding of each receipt
/// (self-consistent producer↔verifier); exact Ethereum typed-receipt RLP is a
/// P6 (eth-RPC) refinement.
pub fn receipts_root(receipts: &[EvmReceipt]) -> EvmH256 {
    let root = alloy_trie::root::ordered_trie_root_with_encoder(receipts, |r, buf| {
        buf.extend_from_slice(&borsh::to_vec(r).expect("borsh of a receipt is infallible"))
    });
    EvmH256::from_bytes(root.0)
}

/// Accrue one item (a 20-byte address or a 32-byte topic) into a 2048-bit
/// Ethereum logs bloom: 3 bits, taken from `keccak256(item)` (Yellow Paper §4.3.1).
fn bloom_accrue(bloom: &mut [u8; EVM_BLOOM_SIZE], data: &[u8]) {
    let h = keccak256(data);
    for i in [0usize, 2, 4] {
        let bit = (((h[i] as usize) << 8) | (h[i + 1] as usize)) & 0x7FF;
        bloom[EVM_BLOOM_SIZE - 1 - (bit >> 3)] |= 1u8 << (bit & 7);
    }
}

/// The aggregate logs bloom over every receipt's logs (each log's address + topics).
pub fn logs_bloom(receipts: &[EvmReceipt]) -> [u8; EVM_BLOOM_SIZE] {
    let mut bloom = [0u8; EVM_BLOOM_SIZE];
    for r in receipts {
        accrue_receipt(&mut bloom, r);
    }
    bloom
}

/// The logs bloom for a SINGLE receipt (the standard per-tx `logsBloom` the
/// eth-rpc adapter renders; audit H-05 — was a zero constant).
pub fn receipt_logs_bloom(receipt: &EvmReceipt) -> [u8; EVM_BLOOM_SIZE] {
    let mut bloom = [0u8; EVM_BLOOM_SIZE];
    accrue_receipt(&mut bloom, receipt);
    bloom
}

fn accrue_receipt(bloom: &mut [u8; EVM_BLOOM_SIZE], r: &EvmReceipt) {
    for log in &r.logs {
        bloom_accrue(bloom, log.address.as_ref());
        for topic in &log.topics {
            bloom_accrue(bloom, topic.as_bytes().as_slice());
        }
    }
}
