//! kaspa-pq EVM Lane v0.4 (§15/§16): the EVM transaction mempool.
//!
//! A pool of pending raw EIP-2718 transactions, SEPARATE from the UTXO mempool
//! (§14.1 budget isolation): independent size caps, its own fee ordering, and
//! delayed-acceptance-aware retention. Selection fills the node's OWN template
//! payload (design §15 step 6 — inclusion only, never execution: the txs are
//! executed by whichever chain block later ACCEPTS the payload block).
//!
//! Retention follows the §15 skip-rescue rule: inclusion in a payload does NOT
//! remove a tx (inclusion ≠ acceptance under mergeset delayed acceptance), and
//! class-2/5 skipped txs stay re-includable. An already-executed tx that gets
//! re-included is a deterministic class-3 duplicate skip — harmless to
//! consensus, so phase-1 cleanup is TTL-based (state-nonce pruning can refine
//! this when the receipt index lands).
//!
//! The data structure is feature-free; only raw-bytes admission needs the
//! `evm` cargo feature (kaspa-evm's decoder), mirroring the consensus seam.
//! Admission applies EXACTLY the body-validation class-1 rule, so a
//! mempool-admitted tx can never make the node's own template
//! payload-block-invalid.

use kaspa_consensus_core::evm::{EvmAddress, EvmExecutionPayload, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK};
use kaspa_hashes::EvmH256;
use std::collections::{BTreeMap, BinaryHeap, HashMap};

/// Maximum pending txs in the pool.
pub const EVM_MEMPOOL_MAX_TXS: usize = 4_096;
/// Maximum total raw bytes in the pool (independent of the UTXO mempool RAM budget, §14.1).
pub const EVM_MEMPOOL_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Seconds a pending tx is retained before TTL expiry.
pub const EVM_MEMPOOL_TX_TTL_SECS: u64 = 3_600;
/// Replacement (same sender + nonce) requires `max_fee_per_gas` to grow by
/// at least this percentage — the standard anti-churn fee-bump rule.
pub const EVM_MEMPOOL_REPLACEMENT_BUMP_PCT: u128 = 10;

/// A pending EVM transaction with the metadata selection needs. Field values
/// come from admission ([`kaspa_evm::tx::admit_tx_info`] under the `evm`
/// feature); the struct itself is feature-free so the pool is always testable.
#[derive(Debug, Clone)]
pub struct PendingEvmTx {
    pub hash: EvmH256,
    pub sender: EvmAddress,
    pub nonce: u64,
    pub max_fee_per_gas: u128,
    /// Raw EIP-2718 bytes (what the payload carries).
    pub raw: Vec<u8>,
    /// Unix seconds at insertion (TTL anchor).
    pub added_at: u64,
}

/// Why an insertion was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvmMempoolError {
    /// Failed the class-1 admission rule (decode / signer / chain-id / gas band).
    Inadmissible(String),
    /// Identical tx hash already pending.
    Duplicate(EvmH256),
    /// Same (sender, nonce) pending and the fee bump is below the threshold.
    ReplacementUnderpriced { pending_fee: u128, required_fee: u128 },
    /// The tx alone can never fit a payload (exceeds the per-block byte cap).
    TooLarge(usize),
    /// Pool is full and the fee does not beat the cheapest pending tx.
    Full,
}

impl std::fmt::Display for EvmMempoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvmMempoolError::Inadmissible(e) => write!(f, "inadmissible evm tx: {e}"),
            EvmMempoolError::Duplicate(h) => write!(f, "evm tx {h} already pending"),
            EvmMempoolError::ReplacementUnderpriced { pending_fee, required_fee } => {
                write!(f, "replacement underpriced: pending max_fee {pending_fee}, required ≥ {required_fee}")
            }
            EvmMempoolError::TooLarge(s) => write!(f, "evm tx of {s} bytes can never fit a payload"),
            EvmMempoolError::Full => write!(f, "evm mempool full and fee below the eviction floor"),
        }
    }
}

#[derive(Default)]
pub struct EvmMempool {
    /// tx hash → pending tx.
    txs: HashMap<EvmH256, PendingEvmTx>,
    /// (sender, nonce) → tx hash (the replacement key; BTreeMap gives each
    /// sender's txs in ascending-nonce order for selection).
    by_sender_nonce: BTreeMap<(EvmAddress, u64), EvmH256>,
    /// Sum of raw byte lengths (pool budget accounting).
    total_bytes: usize,
}

impl EvmMempool {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn len(&self) -> usize {
        self.txs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.txs.is_empty()
    }

    pub fn total_bytes(&self) -> usize {
        self.total_bytes
    }

    pub fn contains(&self, hash: &EvmH256) -> bool {
        self.txs.contains_key(hash)
    }

    /// Raw EIP-2718 bytes of a pending tx (§14.2: served to requesting peers).
    pub fn get_raw(&self, hash: &EvmH256) -> Option<Vec<u8>> {
        self.txs.get(hash).map(|t| t.raw.clone())
    }

    /// Insert a pre-admitted pending tx (admission itself happens in
    /// [`crate::manager::MiningManager::submit_evm_transaction`], which is the
    /// only production caller; tests construct `PendingEvmTx` directly).
    pub fn insert(&mut self, tx: PendingEvmTx) -> Result<EvmH256, EvmMempoolError> {
        // A tx that can never fit a payload is not poolable. The payload borsh
        // overhead is the empty-payload base + a 4-byte length per tx.
        let base = EvmExecutionPayload::default().payload_bytes().len();
        if base + 4 + tx.raw.len() > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
            return Err(EvmMempoolError::TooLarge(tx.raw.len()));
        }
        if self.txs.contains_key(&tx.hash) {
            return Err(EvmMempoolError::Duplicate(tx.hash));
        }

        // Same (sender, nonce): replacement requires the standard fee bump.
        if let Some(existing_hash) = self.by_sender_nonce.get(&(tx.sender, tx.nonce)).copied() {
            let existing = &self.txs[&existing_hash];
            // Saturate the BUMP, then the add — `required >= existing` always.
            // (`existing * 110 / 100` reverse-overflows near u128::MAX: the mul
            // saturates and the division then yields LESS than `existing`,
            // letting a cheaper replacement through. Audit L2.)
            let required = existing
                .max_fee_per_gas
                .saturating_add(existing.max_fee_per_gas.saturating_mul(EVM_MEMPOOL_REPLACEMENT_BUMP_PCT) / 100);
            if tx.max_fee_per_gas < required {
                return Err(EvmMempoolError::ReplacementUnderpriced { pending_fee: existing.max_fee_per_gas, required_fee: required });
            }
            self.remove(&existing_hash);
        }

        // Pool budget: evict the cheapest pending tx while full, but only for a
        // strictly better-paying newcomer (no fee-neutral churn).
        while self.txs.len() >= EVM_MEMPOOL_MAX_TXS || self.total_bytes + tx.raw.len() > EVM_MEMPOOL_MAX_TOTAL_BYTES {
            let Some((cheapest_hash, cheapest_fee)) =
                self.txs.values().map(|t| (t.hash, t.max_fee_per_gas)).min_by_key(|(_, fee)| *fee)
            else {
                // Pool is empty yet the budget still does not fit: unreachable
                // given the TooLarge gate above, but fail closed.
                return Err(EvmMempoolError::Full);
            };
            if tx.max_fee_per_gas <= cheapest_fee {
                return Err(EvmMempoolError::Full);
            }
            self.remove(&cheapest_hash);
        }

        let hash = tx.hash;
        self.total_bytes += tx.raw.len();
        self.by_sender_nonce.insert((tx.sender, tx.nonce), hash);
        self.txs.insert(hash, tx);
        Ok(hash)
    }

    /// Remove one pending tx (no-op if absent).
    pub fn remove(&mut self, hash: &EvmH256) -> Option<PendingEvmTx> {
        let tx = self.txs.remove(hash)?;
        self.by_sender_nonce.remove(&(tx.sender, tx.nonce));
        self.total_bytes -= tx.raw.len();
        Some(tx)
    }

    /// TTL expiry (phase-1 retention bound; see the module note on class-3).
    pub fn expire(&mut self, now_secs: u64) {
        let expired: Vec<EvmH256> =
            self.txs.values().filter(|t| now_secs.saturating_sub(t.added_at) > EVM_MEMPOOL_TX_TTL_SECS).map(|t| t.hash).collect();
        for h in expired {
            self.remove(&h);
        }
    }

    /// Select the node's own template payload txs (design §15 step 6):
    /// per-sender strictly ascending nonces (acceptance executes payload txs in
    /// order — an out-of-order nonce is a guaranteed class-2 skip), globally
    /// highest-`max_fee_per_gas`-first across the current head of each sender's
    /// run, greedily byte-capped so the assembled payload stays within
    /// `max_payload_bytes` (the §4.1 borsh size the body rule enforces).
    pub fn select_candidates(&self, max_payload_bytes: usize) -> Vec<Vec<u8>> {
        // Per-sender ascending-nonce runs (BTreeMap iteration order).
        let mut runs: HashMap<EvmAddress, Vec<&PendingEvmTx>> = HashMap::new();
        for ((sender, _nonce), hash) in self.by_sender_nonce.iter() {
            runs.entry(*sender).or_default().push(&self.txs[hash]);
        }

        // Greedy head-of-run max-heap by fee (deterministic tie-break by hash).
        #[derive(PartialEq, Eq)]
        struct Head {
            fee: u128,
            hash: EvmH256,
            sender: EvmAddress,
        }
        impl Ord for Head {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.fee.cmp(&other.fee).then_with(|| self.hash.cmp(&other.hash))
            }
        }
        impl PartialOrd for Head {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut heap: BinaryHeap<Head> = runs
            .iter()
            .map(|(sender, run)| Head { fee: run[0].max_fee_per_gas, hash: run[0].hash, sender: *sender })
            .collect();
        let mut next_idx: HashMap<EvmAddress, usize> = runs.keys().map(|s| (*s, 0)).collect();

        let base = EvmExecutionPayload::default().payload_bytes().len();
        let mut budget = max_payload_bytes.saturating_sub(base);
        let mut selected = Vec::new();
        while let Some(head) = heap.pop() {
            let run = &runs[&head.sender];
            let idx = next_idx[&head.sender];
            let tx = run[idx];
            let cost = 4 + tx.raw.len();
            if cost <= budget {
                budget -= cost;
                selected.push(tx.raw.clone());
                // Advance this sender's run; a higher nonce must never precede a
                // lower one, so the run's NEXT tx only enters the heap now.
                if idx + 1 < run.len() {
                    next_idx.insert(head.sender, idx + 1);
                    let nxt = run[idx + 1];
                    heap.push(Head { fee: nxt.max_fee_per_gas, hash: nxt.hash, sender: head.sender });
                }
            }
            // A head that does not fit is dropped together with the rest of its
            // run (its successors must not jump the nonce order).
        }
        selected
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tx(sender_byte: u8, nonce: u64, fee: u128, size: usize, tag: u8) -> PendingEvmTx {
        let mut hash = [0u8; 32];
        hash[0] = sender_byte;
        hash[1] = nonce as u8;
        hash[2] = tag;
        PendingEvmTx {
            hash: EvmH256::from_bytes(hash),
            sender: EvmAddress::from_bytes([sender_byte; 20]),
            nonce,
            max_fee_per_gas: fee,
            raw: vec![tag; size],
            added_at: 1_000,
        }
    }

    #[test]
    fn insert_duplicate_replace_and_evict() {
        let mut pool = EvmMempool::new();
        let a0 = tx(0xA, 0, 100, 10, 1);
        let a0_hash = pool.insert(a0.clone()).unwrap();
        assert_eq!(pool.insert(a0.clone()), Err(EvmMempoolError::Duplicate(a0_hash)));

        // Same (sender, nonce), +5% fee: underpriced. +10%: replaces.
        let cheap_bump = tx(0xA, 0, 105, 10, 2);
        assert!(matches!(pool.insert(cheap_bump), Err(EvmMempoolError::ReplacementUnderpriced { .. })));
        let good_bump = tx(0xA, 0, 110, 10, 3);
        let new_hash = pool.insert(good_bump).unwrap();
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&new_hash));
        assert!(!pool.contains(&a0_hash));

        // A tx that can never fit a payload is rejected outright.
        assert!(matches!(
            pool.insert(tx(0xB, 0, 999, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, 4)),
            Err(EvmMempoolError::TooLarge(_))
        ));
    }

    /// Audit L2: near u128::MAX the old `existing * 110 / 100` reverse-overflowed
    /// (saturating mul, then division) into a threshold BELOW the pending fee —
    /// a strictly cheaper replacement was admitted. The bump must saturate so
    /// `required >= existing` always holds.
    #[test]
    fn replacement_bump_is_monotone_at_u128_max() {
        let mut pool = EvmMempool::new();
        let near_max = u128::MAX - 5;
        pool.insert(tx(0xA, 0, near_max, 10, 1)).unwrap();
        // A CHEAPER tx must never replace, no matter how the threshold math saturates.
        assert!(matches!(pool.insert(tx(0xA, 0, near_max - 1, 10, 2)), Err(EvmMempoolError::ReplacementUnderpriced { .. })));
        // Equal fee is also under the (saturated) required threshold.
        assert!(matches!(pool.insert(tx(0xA, 0, near_max, 10, 3)), Err(EvmMempoolError::Duplicate(_) | EvmMempoolError::ReplacementUnderpriced { .. })));
    }

    #[test]
    fn selection_is_fee_ordered_and_nonce_ascending_per_sender() {
        let mut pool = EvmMempool::new();
        // Sender A: nonce 0 (fee 50), nonce 1 (fee 500 — must NOT precede nonce 0).
        pool.insert(tx(0xA, 0, 50, 10, 1)).unwrap();
        pool.insert(tx(0xA, 1, 500, 10, 2)).unwrap();
        // Sender B: nonce 0 (fee 100).
        pool.insert(tx(0xB, 0, 100, 10, 3)).unwrap();

        let selected = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK);
        assert_eq!(selected.len(), 3);
        // B0 (100) first; A0 (50) before A1 despite A1's higher fee; once A0 is
        // in, A1 (500) outbids nothing remaining.
        assert_eq!(selected[0], vec![3u8; 10]); // B0
        assert_eq!(selected[1], vec![1u8; 10]); // A0
        assert_eq!(selected[2], vec![2u8; 10]); // A1
        let a_first = selected.iter().position(|r| r == &vec![1u8; 10]).unwrap();
        let a_second = selected.iter().position(|r| r == &vec![2u8; 10]).unwrap();
        assert!(a_first < a_second, "per-sender nonce order holds");
    }

    #[test]
    fn selection_respects_the_byte_cap() {
        let mut pool = EvmMempool::new();
        let base = EvmExecutionPayload::default().payload_bytes().len();
        // Two txs of 100 raw bytes each (104 with the per-tx length prefix).
        pool.insert(tx(0xA, 0, 100, 100, 1)).unwrap();
        pool.insert(tx(0xB, 0, 90, 100, 2)).unwrap();
        // Budget for exactly one tx.
        let selected = pool.select_candidates(base + 104);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0], vec![1u8; 100], "the higher-fee tx wins the slot");
        // Assembled payload actually fits the cap it was selected under.
        let payload = EvmExecutionPayload { transactions: selected, ..Default::default() };
        assert!(payload.payload_bytes().len() <= base + 104);
    }

    #[test]
    fn ttl_expiry_and_removal_keep_accounting_consistent() {
        let mut pool = EvmMempool::new();
        let h = pool.insert(tx(0xA, 0, 100, 10, 1)).unwrap();
        pool.insert(tx(0xB, 0, 100, 20, 2)).unwrap();
        assert_eq!(pool.total_bytes(), 30);
        // §14.2 relay serving: pending raw bytes by hash, None when absent.
        assert_eq!(pool.get_raw(&h), Some(vec![1u8; 10]));
        assert_eq!(pool.get_raw(&EvmH256::from_bytes([0xFF; 32])), None);
        pool.remove(&h);
        assert_eq!(pool.total_bytes(), 20);
        // Within TTL: nothing expires. Past TTL: everything goes.
        pool.expire(1_000 + EVM_MEMPOOL_TX_TTL_SECS);
        assert_eq!(pool.len(), 1);
        pool.expire(1_001 + EVM_MEMPOOL_TX_TTL_SECS);
        assert!(pool.is_empty());
        assert_eq!(pool.total_bytes(), 0);
    }
}
