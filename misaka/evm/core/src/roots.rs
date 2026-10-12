//! Commitment roots for the `EvmExecutionHeader` (design §3.2/§3.3):
//! - MISAKA keyed-BLAKE2b-256 roots over the ordered system-ops / withdrawals /
//!   applied-deposit-claims lists (domain-separated, §3.3).
//! - Ethereum keccak256 ordered tries for the transactions and receipts.
//! - The 2048-bit Ethereum logs bloom.

use kaspa_consensus_core::evm::{
    DepositClaim, EVM_BLOOM_SIZE, EvmReceipt, EvmSystemOp, MISAKA_EVM_DEPOSIT_CLAIM_CONTEXT, MISAKA_EVM_SYSTEM_OPS_CONTEXT,
    MISAKA_EVM_WITHDRAWAL_CONTEXT, WithdrawOp,
};
use kaspa_hashes::{EvmH256, blake2b_256_keyed};
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

/// Receipts root **v1** (pre-`evm_typed_receipt_root` fence). Commits a
/// deterministic borsh encoding of each receipt (self-consistent producer↔
/// verifier) — NOT Ethereum's typed-receipt encoding. Frozen: every block below
/// the fence MUST reproduce these exact bytes (the §12 Phase-7 fork switches to
/// [`receipts_root_v2`] at/above the fence only).
pub fn receipts_root(receipts: &[EvmReceipt]) -> EvmH256 {
    let root = alloy_trie::root::ordered_trie_root_with_encoder(receipts, |r, buf| {
        buf.extend_from_slice(&borsh::to_vec(r).expect("borsh of a receipt is infallible"))
    });
    EvmH256::from_bytes(root.0)
}

/// The EIP-2718 transaction type of a canonical 2718-encoded raw tx: the leading
/// byte when it is a type identifier (`<= 0x7f`, i.e. 1 = EIP-2930, 2 = EIP-1559),
/// else `0` (legacy — a raw tx whose first byte is an RLP list header `>= 0xc0`).
/// Only types 0/1/2 are admissible on this Shanghai chain; any other value maps
/// to legacy defensively.
#[inline]
pub fn eip2718_tx_type(raw: &[u8]) -> u8 {
    match raw.first().copied() {
        Some(b) if b <= 0x7f => b,
        _ => 0,
    }
}

/// Receipts root **v2** — the exact Ethereum EIP-2718 TYPED receipt root (the
/// §12 Phase-7 consensus fork, active at/above `evm_typed_receipt_root_activation`).
///
/// Each receipt is encoded as Ethereum does: a legacy (type-0) receipt is the
/// bare `rlp([status, cumulativeGasUsed, logsBloom, logs])`; a typed (type-1/2)
/// receipt is `tx_type_byte || rlp(...)`. The trie is keyed by `rlp(index)` and
/// the root is the keccak secure-MPT — i.e. `eth_getBlockReceipts`-grade
/// `receiptsRoot`. `executed_raws` is parallel to `receipts` (the accepted txs in
/// order); its leading byte yields each receipt's tx type.
///
/// Built entirely on alloy's canonical encoder (`alloy_consensus::proofs::
/// calculate_receipt_root` over `ReceiptEnvelope`, the same code reth uses for
/// real Ethereum blocks): `status` = `Eip658Value::Eip658(succeeded)`, the
/// per-receipt `logsBloom` is alloy's standard 2048-bit bloom (identical to
/// [`receipt_logs_bloom`], guarded by a test), and `logs` are the receipt's
/// effective logs. Minimal MISAKA-side logic ⇒ minimal byte-divergence surface.
pub fn receipts_root_v2(receipts: &[EvmReceipt], executed_raws: &[Vec<u8>]) -> EvmH256 {
    use alloy_consensus::{Eip658Value, Receipt, ReceiptEnvelope};
    use alloy_primitives::{Address, B256, Bytes, Log, LogData};

    let envelopes: Vec<ReceiptEnvelope> = receipts
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let logs: Vec<Log> = r
                .logs
                .iter()
                .map(|l| {
                    let topics: Vec<B256> = l.topics.iter().map(|t| B256::from(t.as_bytes())).collect();
                    Log {
                        address: Address::from(l.address.as_bytes()),
                        data: LogData::new_unchecked(topics, Bytes::copy_from_slice(&l.data)),
                    }
                })
                .collect();
            // alloy computes the standard Ethereum per-receipt bloom from these logs.
            let with_bloom =
                Receipt { status: Eip658Value::Eip658(r.succeeded), cumulative_gas_used: r.cumulative_gas_used as u128, logs }
                    .with_bloom();
            // `executed_raws` is parallel to `receipts`; a defensive missing entry is legacy.
            match executed_raws.get(i).map(|raw| eip2718_tx_type(raw)).unwrap_or(0) {
                1 => ReceiptEnvelope::Eip2930(with_bloom),
                2 => ReceiptEnvelope::Eip1559(with_bloom),
                _ => ReceiptEnvelope::Legacy(with_bloom),
            }
        })
        .collect();

    EvmH256::from_bytes(alloy_consensus::proofs::calculate_receipt_root(&envelopes).0)
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

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::evm::{EvmAddress, EvmLog};

    fn topic(b: u8) -> EvmH256 {
        EvmH256::from_bytes([b; 32])
    }

    fn receipt(succeeded: bool, cum_gas: u64, logs: Vec<EvmLog>) -> EvmReceipt {
        EvmReceipt { succeeded, cumulative_gas_used: cum_gas, gas_used: cum_gas, logs }
    }

    fn log(addr: u8, topics: &[u8], data: &[u8]) -> EvmLog {
        EvmLog { address: EvmAddress::from_bytes([addr; 20]), topics: topics.iter().map(|t| topic(*t)).collect(), data: data.to_vec() }
    }

    // A minimal valid-shaped raw tx whose LEADING byte selects the EIP-2718 type:
    // 0xc0 → legacy (RLP list header), 0x01 → EIP-2930, 0x02 → EIP-1559.
    fn raw_of_type(ty: u8) -> Vec<u8> {
        match ty {
            1 => vec![0x01, 0xde, 0xad],
            2 => vec![0x02, 0xde, 0xad],
            _ => vec![0xc0],
        }
    }

    #[test]
    fn tx_type_classification() {
        assert_eq!(eip2718_tx_type(&[0xc0]), 0, "RLP list header ⇒ legacy");
        assert_eq!(eip2718_tx_type(&[0xf8, 0x6c]), 0, "long RLP list ⇒ legacy");
        assert_eq!(eip2718_tx_type(&[0x01, 0xaa]), 1, "EIP-2930");
        assert_eq!(eip2718_tx_type(&[0x02, 0xbb]), 2, "EIP-1559");
        assert_eq!(eip2718_tx_type(&[]), 0, "empty ⇒ legacy (defensive)");
    }

    /// External anchor: the v2 root of ZERO receipts is Ethereum's empty-trie root
    /// keccak256(rlp("")) — pinning the trie machinery to Ethereum's constant.
    #[test]
    fn v2_empty_is_ethereum_empty_root() {
        let root = receipts_root_v2(&[], &[]);
        assert_eq!(root.as_bytes(), alloy_trie::EMPTY_ROOT_HASH.0);
        // 0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421
        assert_eq!(faster_hex::hex_string(&root.as_bytes()), "56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421");
        // v1 ALSO yields Ethereum's empty-trie root for empty receipts — so the
        // executor's empty-block fast path (which may emit either, depending on the
        // fence) is byte-identical across the fence. This pins that equivalence so a
        // future alloy_trie / encoder change cannot silently fork empty blocks.
        assert_eq!(receipts_root(&[]).as_bytes(), alloy_trie::EMPTY_ROOT_HASH.0);
        assert_eq!(receipts_root(&[]), receipts_root_v2(&[], &[]), "v1(empty) == v2(empty)");
    }

    /// The per-receipt bloom MISAKA computes (receipt_logs_bloom, the eth-rpc
    /// logsBloom) is byte-identical to alloy's standard Ethereum bloom that v2
    /// embeds in the typed-receipt encoding — so the RPC bloom and the committed
    /// root agree.
    #[test]
    fn misaka_bloom_matches_alloy_bloom() {
        use alloy_consensus::{Eip658Value, Receipt};
        use alloy_primitives::{Address, B256, Bytes, Log, LogData};
        let r = receipt(true, 21_000, vec![log(0xAB, &[0x11, 0x22], &[0xde, 0xad, 0xbe, 0xef]), log(0xCD, &[0x33], &[])]);
        let mine = receipt_logs_bloom(&r);
        let logs: Vec<Log> = r
            .logs
            .iter()
            .map(|l| Log {
                address: Address::from(l.address.as_bytes()),
                data: LogData::new_unchecked(
                    l.topics.iter().map(|t| B256::from(t.as_bytes())).collect(),
                    Bytes::copy_from_slice(&l.data),
                ),
            })
            .collect();
        let alloy_bloom = Receipt { status: Eip658Value::Eip658(true), cumulative_gas_used: 21_000u128, logs }.with_bloom().logs_bloom;
        assert_eq!(mine, alloy_bloom.0, "MISAKA receipt bloom must equal alloy's Ethereum bloom");
    }

    /// v2 differs from v1 (the fork actually changes the root) AND v2 is
    /// deterministic + sensitive to tx type, status, gas and logs.
    #[test]
    fn v2_is_deterministic_and_distinct_from_v1() {
        let r = receipt(true, 21_000, vec![log(0xAB, &[0x11], &[0x01])]);
        let raws = vec![raw_of_type(2)];
        let v2 = receipts_root_v2(std::slice::from_ref(&r), &raws);
        // deterministic
        assert_eq!(v2, receipts_root_v2(std::slice::from_ref(&r), &raws));
        // distinct from the borsh v1 root (the fork changes bytes)
        assert_ne!(v2, receipts_root(std::slice::from_ref(&r)));
        // tx-type sensitive: same receipt as legacy vs 1559 ⇒ different root
        assert_ne!(
            receipts_root_v2(std::slice::from_ref(&r), &[raw_of_type(0)]),
            receipts_root_v2(std::slice::from_ref(&r), &[raw_of_type(2)])
        );
        // status sensitive
        let mut r_fail = r.clone();
        r_fail.succeeded = false;
        assert_ne!(receipts_root_v2(&[r], &raws), receipts_root_v2(&[r_fail], &raws));
    }

    /// Pinned regression vectors (lock the exact bytes; an alloy bump or adapter
    /// change that moves them fails loudly — the §22 byte-stability discipline).
    #[test]
    fn v2_pinned_vectors() {
        // (a) one legacy receipt, success, no logs.
        let a = receipts_root_v2(&[receipt(true, 21_000, vec![])], &[raw_of_type(0)]);
        // (b) one EIP-1559 receipt, success, two logs.
        let b = receipts_root_v2(
            &[receipt(true, 50_000, vec![log(0x11, &[0xaa, 0xbb], &[0x01, 0x02]), log(0x22, &[0xcc], &[])])],
            &[raw_of_type(2)],
        );
        // (c) mixed: legacy then 1559.
        let c = receipts_root_v2(
            &[receipt(false, 21_000, vec![]), receipt(true, 71_000, vec![log(0x33, &[0xdd], &[0xff])])],
            &[raw_of_type(0), raw_of_type(2)],
        );
        let hexes: Vec<String> = [a, b, c].iter().map(|h| faster_hex::hex_string(&h.as_bytes())).collect();
        assert_eq!(
            hexes,
            vec![
                "056b23fbba480696b65fe5a59b8f2148a1299103c4f57df839233af2cf4ca2d2".to_string(),
                "ee2da4c63be486077db914a86d828398597427813828e72664063db681d0ca79".to_string(),
                "0e2ad821ad76362ff62a4ef6f867ca170d9ff8c085a210c344559e0603efaa61".to_string(),
            ],
            "v2 receipt-root vectors changed — if alloy was bumped, re-pin intentionally"
        );
    }
}
