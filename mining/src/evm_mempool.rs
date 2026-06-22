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

use kaspa_consensus_core::evm::{
    DepositClaim, EvmAddress, EvmExecutionPayload, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK,
};
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::EvmH256;
use std::collections::{BTreeMap, BinaryHeap, HashMap};

/// Maximum pending txs in the pool.
pub const EVM_MEMPOOL_MAX_TXS: usize = 4_096;
/// Maximum total raw bytes in the pool (independent of the UTXO mempool RAM budget, §14.1).
pub const EVM_MEMPOOL_MAX_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Seconds a pending tx is retained before TTL expiry.
pub const EVM_MEMPOOL_TX_TTL_SECS: u64 = 3_600;
/// Replacement (same sender + nonce) requires BOTH `max_fee_per_gas` and
/// `max_priority_fee_per_gas` to grow by at least this percentage — the standard
/// anti-churn fee-bump rule (priority too, so a tip-less churn can't replace).
pub const EVM_MEMPOOL_REPLACEMENT_BUMP_PCT: u128 = 10;
/// Per-sender pending-tx cap (one-sender DoS bound — the global cap alone lets one
/// sender monopolize the whole pool). A legitimate sender rarely needs a deep queue.
pub const EVM_MEMPOOL_MAX_TXS_PER_SENDER: usize = 256;
/// Per-sender declared-gas cap (a few blocks' worth, so one sender can never reserve
/// more template gas than several blocks can accept).
pub const EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER: u64 = 4 * MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK;
/// Maximum pending deposit claims queued for own-payload `system_ops` (§9.2).
/// Several blocks' worth of the per-block cap so a backlog can drain.
pub const EVM_MEMPOOL_MAX_CLAIMS: usize = 4_096;
/// §9.2: evict a queued deposit claim once its lock has been ABSENT from the live
/// claim view for this many CONSECUTIVE templates. Templates rebuild per new
/// virtual state (≈ per block), so this is a chain-progress budget: a deposit-lock
/// that is genuinely being buried on the selected chain reappears within a handful
/// of blocks, so this is set generously (a slow miner / forky DAG has ample room),
/// yet it still reaps a consumed or never-confirmed lock instead of retrying forever.
pub const MAX_CLAIM_ABSENT_STRIKES: u32 = 600;

/// A pending EVM transaction with the metadata selection needs. Field values
/// come from admission ([`kaspa_evm::tx::admit_tx_info`] under the `evm`
/// feature); the struct itself is feature-free so the pool is always testable.
#[derive(Debug, Clone)]
pub struct PendingEvmTx {
    pub hash: EvmH256,
    pub sender: EvmAddress,
    pub nonce: u64,
    /// Declared gas limit — needed to gas-cap the template (a payload may not declare
    /// more gas than a chain block can accept) and to bound per-sender reservations.
    pub gas_limit: u64,
    pub max_fee_per_gas: u128,
    /// EIP-1559 priority tip (legacy/2930: 0). Drives the EFFECTIVE-tip ordering.
    pub max_priority_fee_per_gas: u128,
    /// Raw EIP-2718 bytes (what the payload carries).
    pub raw: Vec<u8>,
    /// Unix seconds at insertion (TTL anchor).
    pub added_at: u64,
}

impl PendingEvmTx {
    /// The miner's effective per-gas tip at `base_fee` (EIP-1559): `min(priority,
    /// max_fee − base_fee)`. A high-`max_fee` zero-tip tx scores 0 here, so it can
    /// never outrank a paying tx in template selection.
    pub fn effective_tip(&self, base_fee: u128) -> u128 {
        self.max_priority_fee_per_gas.min(self.max_fee_per_gas.saturating_sub(base_fee))
    }
}

/// Why an insertion was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EvmMempoolError {
    /// Failed the class-1 admission rule (decode / signer / chain-id / gas band).
    Inadmissible(String),
    /// Identical tx hash already pending.
    Duplicate(EvmH256),
    /// Same (sender, nonce) pending and the fee bump is below the threshold.
    /// Carries the admitted tx hash (audit #8: lets the relay flow verify the
    /// peer's bytes hash to the requested id even on this benign rejection).
    ReplacementUnderpriced { pending_fee: u128, required_fee: u128, hash: EvmH256 },
    /// The tx alone can never fit a payload (exceeds the per-block byte cap).
    TooLarge { size: usize, hash: EvmH256 },
    /// Pool is full and the fee does not beat the cheapest pending tx.
    Full { hash: EvmH256 },
    /// The sender already has [`EVM_MEMPOOL_MAX_TXS_PER_SENDER`] pending txs (one-sender DoS bound).
    SenderTxLimit { sender: EvmAddress, limit: usize, hash: EvmH256 },
    /// Admitting this tx would push the sender's pending DECLARED gas over
    /// [`EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER`].
    SenderGasLimit { sender: EvmAddress, limit: u64, hash: EvmH256 },
}

impl std::fmt::Display for EvmMempoolError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EvmMempoolError::Inadmissible(e) => write!(f, "inadmissible evm tx: {e}"),
            EvmMempoolError::Duplicate(h) => write!(f, "evm tx {h} already pending"),
            EvmMempoolError::ReplacementUnderpriced { pending_fee, required_fee, .. } => {
                write!(f, "replacement underpriced: pending max_fee {pending_fee}, required ≥ {required_fee}")
            }
            EvmMempoolError::TooLarge { size, .. } => write!(f, "evm tx of {size} bytes can never fit a payload"),
            EvmMempoolError::Full { .. } => write!(f, "evm mempool full and fee below the eviction floor"),
            EvmMempoolError::SenderTxLimit { sender, limit, .. } => {
                write!(f, "sender {sender} already has the maximum {limit} pending evm txs")
            }
            EvmMempoolError::SenderGasLimit { sender, limit, .. } => {
                write!(f, "sender {sender} would exceed the {limit} pending declared-gas budget")
            }
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
    /// §9.2 producer-side deposit claims, keyed by the claimed lock outpoint
    /// (the natural dedup key — one claim per lock). The claim values come
    /// pre-resolved + pre-validated from the RPC layer (which has the UTXO
    /// view); the VSP template path re-validates against the live
    /// selected-parent view and drops any that went stale, so a queued claim
    /// can never make the node's own block invalid. (`TransactionOutpoint` is
    /// not `Ord`, so this is a `HashMap`; selection sorts for determinism.)
    claims: HashMap<TransactionOutpoint, DepositClaim>,
    /// §9.2: per-lock count of CONSECUTIVE templates in which the claim's lock was
    /// ABSENT from the live claim view. Incremented by [`Self::note_claim_absent`]
    /// (≈ once per block, see [`MAX_CLAIM_ABSENT_STRIKES`]) and reset by
    /// [`Self::note_claim_present`] when the lock reappears, so a deposit-lock that
    /// flickers in and out of a forky selected chain is not evicted prematurely.
    /// Only holds entries for currently-absent claims; cleared on removal/insert.
    stale_strikes: HashMap<TransactionOutpoint, u32>,
    /// Last-known head base fee, used to score evictions by the SAME effective
    /// tip the template selection uses (audit H-07). Updated by the template path
    /// via [`Self::set_base_fee`]; defaults to 0 (⇒ effective tip == priority tip),
    /// which is already a safe lower bound — never `max_fee`. Pure local policy:
    /// a stale value only mis-ranks an eviction, never affects block validity.
    base_fee: u128,
}

impl EvmMempool {
    pub fn new() -> Self {
        Default::default()
    }

    /// Update the base fee used for eviction scoring (audit H-07). Called by the
    /// template path with the same head base fee it selects against, so a tx that
    /// cannot be SELECTED (zero effective tip) is also the first to be EVICTED.
    pub fn set_base_fee(&mut self, base_fee: u128) {
        self.base_fee = base_fee;
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

    /// Number of queued deposit claims.
    pub fn claims_len(&self) -> usize {
        self.claims.len()
    }

    /// The queued claim for this lock outpoint (§14.2: served to requesting peers).
    /// Returns the typed claim; the relay flow borsh-encodes it (the mining crate
    /// has no borsh dependency).
    pub fn get_claim(&self, outpoint: &TransactionOutpoint) -> Option<DepositClaim> {
        self.claims.get(outpoint).cloned()
    }

    /// Whether a claim for this lock outpoint is already queued.
    pub fn contains_claim(&self, outpoint: &TransactionOutpoint) -> bool {
        self.claims.contains_key(outpoint)
    }

    /// Queue a pre-resolved + pre-validated deposit claim (§9.2). Dedup is by
    /// the claimed lock outpoint (one claim per lock); a re-submit replaces the
    /// queued claim (idempotent — the fields are derived from the same lock).
    /// Audit F6: when the queue is full of OTHER claims, a strictly higher-paying
    /// newcomer DISPLACES the lowest-`claim_tip` queued claim (no churn on ties),
    /// rather than being rejected — so a high-tip claim is never crowded out by
    /// low-tip claims that merely arrived first. Returns `false` only when the
    /// queue is full and the newcomer does not out-pay the cheapest queued claim.
    pub fn insert_claim(&mut self, claim: DepositClaim) -> bool {
        // Idempotent re-submit / dedup by lock outpoint (never counts against the cap).
        // A deliberate re-submit also restarts the absent-strike run (a fresh retry
        // window — the operator may be re-submitting precisely because the lock has
        // now confirmed on the selected chain).
        if self.claims.contains_key(&claim.deposit_outpoint) {
            self.stale_strikes.remove(&claim.deposit_outpoint);
            self.claims.insert(claim.deposit_outpoint, claim);
            return true;
        }
        if self.claims.len() >= EVM_MEMPOOL_MAX_CLAIMS {
            // Find the lowest-priority queued claim (tip asc, then deterministic outpoint).
            let victim = self
                .claims
                .values()
                .min_by(|a, b| {
                    a.claim_tip_sompi
                        .cmp(&b.claim_tip_sompi)
                        .then_with(|| a.deposit_outpoint.transaction_id.cmp(&b.deposit_outpoint.transaction_id))
                        .then_with(|| a.deposit_outpoint.index.cmp(&b.deposit_outpoint.index))
                })
                .map(|c| (c.deposit_outpoint, c.claim_tip_sompi));
            match victim {
                // Only displace for a strictly higher-paying newcomer (avoids ping-pong on ties).
                Some((victim_op, victim_tip)) if claim.claim_tip_sompi > victim_tip => {
                    self.remove_claim(&victim_op);
                }
                _ => return false,
            }
        }
        self.claims.insert(claim.deposit_outpoint, claim);
        true
    }

    /// Drop a queued claim (e.g. once its lock has been consumed). No-op if absent.
    pub fn remove_claim(&mut self, outpoint: &TransactionOutpoint) -> Option<DepositClaim> {
        self.stale_strikes.remove(outpoint);
        self.claims.remove(outpoint)
    }

    /// §9.2: the template path found this claim's lock ABSENT from the live claim
    /// view (usually transient — the deposit-lock's block is not yet on this node's
    /// selected chain, or it was just consumed). Record one consecutive-absent
    /// strike and return `true` iff the claim has now been absent for
    /// [`MAX_CLAIM_ABSENT_STRIKES`] consecutive templates and should be evicted.
    /// No-op returning `false` if the claim is no longer queued.
    pub fn note_claim_absent(&mut self, outpoint: &TransactionOutpoint) -> bool {
        if !self.claims.contains_key(outpoint) {
            return false;
        }
        let strikes = self.stale_strikes.entry(*outpoint).or_insert(0);
        *strikes += 1;
        *strikes >= MAX_CLAIM_ABSENT_STRIKES
    }

    /// §9.2: the claim's lock is present in the live view again (it was selected
    /// into the latest template) — reset its consecutive-absent run so only an
    /// uninterrupted absence counts toward eviction. Cheap no-op if the claim has
    /// no recorded strikes.
    pub fn note_claim_present(&mut self, outpoint: &TransactionOutpoint) {
        self.stale_strikes.remove(outpoint);
    }

    /// Select up to `max` queued claims for the own-payload `system_ops`. Audit F6:
    /// ordered by `claim_tip_sompi` DESCENDING (the inclusion incentive — credited to
    /// the accepting block's `evm_coinbase`), with a deterministic `(txid, index)`
    /// tiebreak so all producers agree. The VSP template path re-validates each against
    /// the live selected-parent claim view and drops any that went stale, so this is an
    /// over-approximation AND pure local template policy — selecting a since-spent lock
    /// is harmless (filtered before the payload is built) and the order can never make
    /// the node's own block invalid; it only maximizes bridge revenue + claim latency.
    pub fn select_claims(&self, max: usize) -> Vec<DepositClaim> {
        let mut claims: Vec<DepositClaim> = self.claims.values().cloned().collect();
        claims.sort_by(|a, b| {
            b.claim_tip_sompi
                .cmp(&a.claim_tip_sompi)
                .then_with(|| a.deposit_outpoint.transaction_id.cmp(&b.deposit_outpoint.transaction_id))
                .then_with(|| a.deposit_outpoint.index.cmp(&b.deposit_outpoint.index))
        });
        claims.truncate(max);
        claims
    }

    /// The distinct senders with pending txs (for the template path to batch-fetch
    /// their canonical state nonces). BTreeMap keys are sorted, so equal senders are
    /// contiguous — one push per sender.
    pub fn pending_senders(&self) -> Vec<EvmAddress> {
        let mut senders: Vec<EvmAddress> = Vec::new();
        let mut last: Option<EvmAddress> = None;
        for (sender, _nonce) in self.by_sender_nonce.keys() {
            if last != Some(*sender) {
                senders.push(*sender);
                last = Some(*sender);
            }
        }
        senders
    }

    /// The next nonce a wallet should use for `sender` given the chain `state_nonce`
    /// (audit M-08, `eth_getTransactionCount(…,"pending")`): walk the CONTIGUOUS run
    /// of pending nonces starting at `state_nonce` and return the first gap. With no
    /// pending txs this is just `state_nonce`, so back-to-back sends increment
    /// correctly instead of colliding on the latest (accepted) nonce.
    pub fn next_pending_nonce(&self, sender: &EvmAddress, state_nonce: u64) -> u64 {
        let mut n = state_nonce;
        while self.by_sender_nonce.contains_key(&(*sender, n)) {
            n += 1;
        }
        n
    }

    /// Count of this sender's pending txs (BTreeMap range over the sender's nonces).
    fn sender_tx_count(&self, sender: &EvmAddress) -> usize {
        self.by_sender_nonce.range((*sender, 0)..=(*sender, u64::MAX)).count()
    }

    /// Sum of this sender's pending DECLARED gas limits (saturating).
    fn sender_declared_gas(&self, sender: &EvmAddress) -> u64 {
        self.by_sender_nonce
            .range((*sender, 0)..=(*sender, u64::MAX))
            .fold(0u64, |acc, (_, h)| acc.saturating_add(self.txs[h].gas_limit))
    }

    /// Remove every pending tx whose nonce is BELOW the sender's canonical state
    /// nonce — i.e. already accepted on the selected chain (or otherwise spent), so
    /// it can only ever be a class-2/3 skip. This is the state-nonce form of the §15
    /// "remove accepted" cleanup (the module note: refine TTL-only retention once the
    /// receipt index lands). Called by the template path each build with the live
    /// account nonces. Senders absent from `state_nonces` (no account yet) are left
    /// untouched (their state nonce is 0 — nothing to prune).
    pub fn prune_below_state_nonce(&mut self, state_nonces: &HashMap<EvmAddress, u64>) {
        let stale: Vec<EvmH256> = self
            .txs
            .values()
            .filter(|t| state_nonces.get(&t.sender).is_some_and(|&n| t.nonce < n))
            .map(|t| t.hash)
            .collect();
        for h in stale {
            self.remove(&h);
        }
    }

    /// Insert a pre-admitted pending tx (admission itself happens in
    /// [`crate::manager::MiningManager::submit_evm_transaction`], which is the
    /// only production caller; tests construct `PendingEvmTx` directly).
    pub fn insert(&mut self, tx: PendingEvmTx) -> Result<EvmH256, EvmMempoolError> {
        // A tx that can never fit a payload is not poolable. The payload borsh
        // overhead is the empty-payload base + a 4-byte length per tx.
        let base = EvmExecutionPayload::default().payload_bytes().len();
        if base + 4 + tx.raw.len() > MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK {
            return Err(EvmMempoolError::TooLarge { size: tx.raw.len(), hash: tx.hash });
        }
        if self.txs.contains_key(&tx.hash) {
            return Err(EvmMempoolError::Duplicate(tx.hash));
        }

        // Same (sender, nonce): replacement requires BOTH the fee cap and the
        // priority tip to bump (geth's rule). Without the tip requirement a churn of
        // zero-tip replacements could thrash the slot at no cost to the miner.
        //
        // Audit M-01: every remaining check is computed against a PROJECTED pool
        // that already discounts the tx being replaced — but NOTHING is mutated
        // until all checks pass, so a rejected replacement can never strand its
        // predecessor (which would open a permanent nonce gap).
        let existing_hash = self.by_sender_nonce.get(&(tx.sender, tx.nonce)).copied();
        if let Some(existing_hash) = existing_hash {
            let existing = &self.txs[&existing_hash];
            // Saturate the BUMP, then the add — `required >= existing` always.
            // (`existing * 110 / 100` reverse-overflows near u128::MAX: the mul
            // saturates and the division then yields LESS than `existing`,
            // letting a cheaper replacement through. Audit L2.)
            let required_fee = existing
                .max_fee_per_gas
                .saturating_add(existing.max_fee_per_gas.saturating_mul(EVM_MEMPOOL_REPLACEMENT_BUMP_PCT) / 100);
            let required_tip = existing
                .max_priority_fee_per_gas
                .saturating_add(existing.max_priority_fee_per_gas.saturating_mul(EVM_MEMPOOL_REPLACEMENT_BUMP_PCT) / 100);
            if tx.max_fee_per_gas < required_fee || tx.max_priority_fee_per_gas < required_tip {
                return Err(EvmMempoolError::ReplacementUnderpriced { pending_fee: existing.max_fee_per_gas, required_fee, hash: tx.hash });
            }
        }
        let is_replacement = existing_hash.is_some();
        let (replaced_gas, replaced_bytes) =
            existing_hash.map(|h| (self.txs[&h].gas_limit, self.txs[&h].raw.len())).unwrap_or((0, 0));

        // Per-sender DoS bounds. A replacement reuses its predecessor's slot, so
        // it is not a NEW tx (count) and its gas discounts the predecessor's.
        if !is_replacement {
            let sender_count = self.sender_tx_count(&tx.sender);
            if sender_count >= EVM_MEMPOOL_MAX_TXS_PER_SENDER {
                return Err(EvmMempoolError::SenderTxLimit { sender: tx.sender, limit: EVM_MEMPOOL_MAX_TXS_PER_SENDER, hash: tx.hash });
            }
        }
        let sender_gas_after = self.sender_declared_gas(&tx.sender).saturating_sub(replaced_gas).saturating_add(tx.gas_limit);
        if sender_gas_after > EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER {
            return Err(EvmMempoolError::SenderGasLimit { sender: tx.sender, limit: EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER, hash: tx.hash });
        }

        // Pool budget: PLAN the evictions (do not mutate) against a pool that
        // already excludes the replaced tx. Evict the lowest EFFECTIVE-tip txs
        // (audit H-07: the SAME score template selection uses, so a high-`max_fee`
        // zero-tip tx cannot squat a slot it could never be selected from), and
        // only for a strictly higher-tip newcomer (no fee-neutral churn).
        let new_tip = tx.effective_tip(self.base_fee);
        let mut proj_len = self.txs.len() - usize::from(is_replacement);
        let mut proj_bytes = self.total_bytes - replaced_bytes;
        let mut planned_evictions: Vec<EvmH256> = Vec::new();
        if proj_len >= EVM_MEMPOOL_MAX_TXS || proj_bytes + tx.raw.len() > EVM_MEMPOOL_MAX_TOTAL_BYTES {
            // Candidate victims by ascending effective tip (deterministic hash
            // tiebreak), excluding the tx being replaced.
            let mut victims: Vec<(EvmH256, u128, usize)> = self
                .txs
                .values()
                .filter(|t| Some(t.hash) != existing_hash)
                .map(|t| (t.hash, t.effective_tip(self.base_fee), t.raw.len()))
                .collect();
            victims.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
            let mut vi = 0;
            while proj_len >= EVM_MEMPOOL_MAX_TXS || proj_bytes + tx.raw.len() > EVM_MEMPOOL_MAX_TOTAL_BYTES {
                let Some(&(vh, vtip, vbytes)) = victims.get(vi) else {
                    // Ran out of victims without fitting: reject, pool untouched.
                    return Err(EvmMempoolError::Full { hash: tx.hash });
                };
                if new_tip <= vtip {
                    return Err(EvmMempoolError::Full { hash: tx.hash });
                }
                planned_evictions.push(vh);
                proj_len -= 1;
                proj_bytes -= vbytes;
                vi += 1;
            }
        }

        // All checks passed — commit. From here nothing can fail, so the
        // replaced tx is only ever removed when its successor will be inserted.
        if let Some(existing_hash) = existing_hash {
            self.remove(&existing_hash);
        }
        for vh in planned_evictions {
            self.remove(&vh);
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

    /// Select the node's own template payload txs (design §15 step 6).
    ///
    /// Per sender we take the CONTIGUOUS run starting at the sender's canonical
    /// `state_nonce` (acceptance executes payload txs in nonce order — a tx after a
    /// missing nonce is a guaranteed class-2 skip, and a tx below the state nonce is
    /// already accepted, so it is parked/pruned). Across senders we greedily pick the
    /// head with the highest EFFECTIVE tip at `base_fee` (so a high-`max_fee` zero-tip
    /// tx cannot crowd out a paying one), capped by BOTH the payload byte budget
    /// (`max_payload_bytes`, the §4.1 borsh size the body rule enforces) AND the
    /// declared-gas budget (`max_declared_gas`, normally the per-chain-block accepted
    /// gas cap — so the assembled payload never declares more gas than a block can
    /// accept, the local complement of the executor's gas accounting).
    ///
    /// `state_nonces` is the live account-nonce view (absent sender ⇒ nonce 0). It is
    /// a pure local template policy: a stale or gapped pick only wastes a slot, never
    /// invalidates the node's own block.
    ///
    /// `state_balances` (audit H-10) is the matching committed-balance view, in wei
    /// saturated to `u128` (absent sender ⇒ balance 0). When `Some`, a sender whose
    /// running balance cannot cover a transaction's EIP-1559 up-front gas reservation
    /// (`gas_limit × max_fee_per_gas`) is passed over for the rest of its run — that tx
    /// would be a guaranteed class-2 skip at execution, so selecting it only wastes a
    /// payload slot (the high-`max_fee` unfundable-Sybil squat). The deduction ignores
    /// value transfers (so the check is LENIENT — it never drops a tx execution would
    /// accept), and nothing is evicted: the tx stays pending and a later template
    /// selects it once the sender's committed balance covers it. `None` disables the
    /// filter (unchanged selection — used where no balance view is threaded).
    pub fn select_candidates(
        &self,
        max_payload_bytes: usize,
        max_declared_gas: u64,
        base_fee: u128,
        state_nonces: &HashMap<EvmAddress, u64>,
        state_balances: Option<&HashMap<EvmAddress, u128>>,
    ) -> Vec<Vec<u8>> {
        // Distinct senders (BTreeMap keys are sorted, so a run of equal senders is contiguous).
        let mut senders: Vec<EvmAddress> = Vec::new();
        let mut last: Option<EvmAddress> = None;
        for (sender, _nonce) in self.by_sender_nonce.keys() {
            if last != Some(*sender) {
                senders.push(*sender);
                last = Some(*sender);
            }
        }

        // Per-sender CONTIGUOUS run from the canonical state nonce (park on a gap).
        let mut runs: HashMap<EvmAddress, Vec<&PendingEvmTx>> = HashMap::new();
        for sender in senders {
            let mut expected = state_nonces.get(&sender).copied().unwrap_or(0);
            let mut run: Vec<&PendingEvmTx> = Vec::new();
            while let Some(hash) = self.by_sender_nonce.get(&(sender, expected)) {
                run.push(&self.txs[hash]);
                expected = match expected.checked_add(1) {
                    Some(n) => n,
                    None => break,
                };
            }
            if !run.is_empty() {
                runs.insert(sender, run);
            }
        }

        // Greedy head-of-run max-heap by EFFECTIVE tip (deterministic tie-break by hash).
        #[derive(PartialEq, Eq)]
        struct Head {
            tip: u128,
            hash: EvmH256,
            sender: EvmAddress,
        }
        impl Ord for Head {
            fn cmp(&self, other: &Self) -> std::cmp::Ordering {
                self.tip.cmp(&other.tip).then_with(|| self.hash.cmp(&other.hash))
            }
        }
        impl PartialOrd for Head {
            fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(other))
            }
        }

        let mut heap: BinaryHeap<Head> = runs
            .iter()
            .map(|(sender, run)| Head { tip: run[0].effective_tip(base_fee), hash: run[0].hash, sender: *sender })
            .collect();
        let mut next_idx: HashMap<EvmAddress, usize> = runs.keys().map(|s| (*s, 0)).collect();

        let base = EvmExecutionPayload::default().payload_bytes().len();
        let mut bytes_left = max_payload_bytes.saturating_sub(base);
        let mut gas_left = max_declared_gas;
        // Audit H-10: per-sender running balance (lazily seeded from the committed view).
        // Only consulted when `state_balances` is Some.
        let mut balance_left: HashMap<EvmAddress, u128> = HashMap::new();
        let mut selected = Vec::new();
        while let Some(head) = heap.pop() {
            let run = &runs[&head.sender];
            let idx = next_idx[&head.sender];
            let tx = run[idx];
            let cost = 4 + tx.raw.len();
            // Audit H-10: the EIP-1559 up-front gas reservation revm deducts before
            // execution. value transfers are ignored (lenient — see the doc comment).
            let gas_reservation = (tx.gas_limit as u128).saturating_mul(tx.max_fee_per_gas);
            let affordable = match state_balances {
                Some(bals) => {
                    let remaining = *balance_left.entry(head.sender).or_insert_with(|| bals.get(&head.sender).copied().unwrap_or(0));
                    remaining >= gas_reservation
                }
                None => true,
            };
            // Must be affordable AND fit BOTH budgets. A head that does not qualify drops
            // the rest of its run (its successors must not jump the nonce order, and a
            // higher nonce needs at least as much balance as this one).
            if affordable && cost <= bytes_left && tx.gas_limit <= gas_left {
                bytes_left -= cost;
                gas_left -= tx.gas_limit;
                if state_balances.is_some() {
                    if let Some(remaining) = balance_left.get_mut(&head.sender) {
                        *remaining = remaining.saturating_sub(gas_reservation);
                    }
                }
                selected.push(tx.raw.clone());
                if idx + 1 < run.len() {
                    next_idx.insert(head.sender, idx + 1);
                    let nxt = run[idx + 1];
                    heap.push(Head { tip: nxt.effective_tip(base_fee), hash: nxt.hash, sender: head.sender });
                }
            }
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
            gas_limit: 21_000,
            max_fee_per_gas: fee,
            // priority == max_fee so effective_tip at base_fee 0 equals `fee` — keeps
            // the existing fee-ordering assertions meaningful under the new selector.
            max_priority_fee_per_gas: fee,
            raw: vec![tag; size],
            added_at: 1_000,
        }
    }

    /// Convenience: the no-gas-cap, base-fee-0, no-state-nonce selection the
    /// pre-pagination tests exercised (all senders start at nonce 0).
    fn select(pool: &EvmMempool, max_payload_bytes: usize) -> Vec<Vec<u8>> {
        pool.select_candidates(max_payload_bytes, u64::MAX, 0, &HashMap::new(), None)
    }

    /// Build a pool DIRECTLY (bypassing the admission path) with one sender's
    /// CONTIGUOUS nonce range — used to exercise select/prune over a range larger
    /// than the per-sender admission caps (`EVM_MEMPOOL_MAX_TXS_PER_SENDER` AND, for
    /// large ranges at 21k gas each, `EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER`).
    /// `select_candidates` has no admission gate, so selecting against such a pool
    /// is valid. Each tx's `raw` encodes its nonce in the first 8 bytes so a
    /// selection can be mapped back to exact nonces (and overlap/dup detected).
    fn pool_with_sender_range(sender_byte: u8, nonces: std::ops::Range<u64>, raw_size: usize) -> EvmMempool {
        let mut pool = EvmMempool::new();
        let sender = EvmAddress::from_bytes([sender_byte; 20]);
        for nonce in nonces {
            let mut hash = [0u8; 32];
            hash[0] = sender_byte;
            hash[1..9].copy_from_slice(&nonce.to_le_bytes());
            let h = EvmH256::from_bytes(hash);
            let mut raw = vec![0u8; raw_size.max(8)];
            raw[..8].copy_from_slice(&nonce.to_le_bytes());
            pool.total_bytes += raw.len();
            pool.by_sender_nonce.insert((sender, nonce), h);
            pool.txs.insert(
                h,
                PendingEvmTx { hash: h, sender, nonce, gas_limit: 21_000, max_fee_per_gas: 1, max_priority_fee_per_gas: 0, raw, added_at: 1_000 },
            );
        }
        pool
    }

    fn nonce_of(raw: &[u8]) -> u64 {
        u64::from_le_bytes(raw[..8].try_into().unwrap())
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
            Err(EvmMempoolError::TooLarge { .. })
        ));
    }

    /// Audit M-08: pending nonce = state nonce + the contiguous pending run.
    #[test]
    fn next_pending_nonce_walks_the_contiguous_run() {
        let mut pool = EvmMempool::new();
        let s = EvmAddress::from_bytes([0xA; 20]);
        // No pending txs ⇒ just the state nonce.
        assert_eq!(pool.next_pending_nonce(&s, 5), 5);
        // Pending 5,6,7 (contiguous from state 5) ⇒ next is 8.
        for n in [5u64, 6, 7] {
            pool.insert(tx(0xA, n, 100, 10, n as u8)).unwrap();
        }
        assert_eq!(pool.next_pending_nonce(&s, 5), 8);
        // A gap (9 present, 8 missing) does not count past the gap.
        pool.insert(tx(0xA, 9, 100, 10, 9)).unwrap();
        assert_eq!(pool.next_pending_nonce(&s, 5), 8);
        // A different sender is unaffected.
        assert_eq!(pool.next_pending_nonce(&EvmAddress::from_bytes([0xB; 20]), 3), 3);
    }

    /// Audit M-01: a replacement that passes the fee/tip bump but fails a later
    /// bound (here the per-sender declared-gas cap) must NOT remove the tx it was
    /// replacing — otherwise that (sender, nonce) slot vanishes and every higher
    /// nonce parks behind a permanent gap.
    #[test]
    fn failed_replacement_preserves_the_old_tx() {
        let mut pool = EvmMempool::new();
        let a0 = tx(0xA, 0, 100, 10, 1);
        let a0_hash = pool.insert(a0).unwrap();

        // Bump passes (fee/tip 200 ≥ 110% of 100), but the replacement declares
        // more gas than the per-sender cap → SenderGasLimit.
        let mut huge = tx(0xA, 0, 200, 10, 2);
        huge.gas_limit = EVM_MEMPOOL_MAX_DECLARED_GAS_PER_SENDER + 1;
        assert!(matches!(pool.insert(huge), Err(EvmMempoolError::SenderGasLimit { .. })));

        // The original tx is intact — no nonce gap.
        assert_eq!(pool.len(), 1);
        assert!(pool.contains(&a0_hash));
        assert_eq!(pool.total_bytes(), 10);
    }

    /// Audit H-07: eviction is scored by the SAME effective tip template
    /// selection uses, so a Sybil flood of high-`max_fee` ZERO-tip txs (strong
    /// under the old `max_fee` score, worthless under the tip score) cannot evict
    /// or crowd out a paying tx. base_fee defaults to 0 ⇒ effective_tip == tip.
    #[test]
    fn zero_tip_flood_cannot_evict_a_paying_tx() {
        let mut pool = EvmMempool::new();
        // Fill to the count cap with high-max_fee, ZERO-tip txs (direct insert to
        // skip the admission path — we are testing eviction, not admission).
        for i in 0..EVM_MEMPOOL_MAX_TXS {
            let mut hb = [0u8; 32];
            hb[..8].copy_from_slice(&(i as u64).to_le_bytes());
            let h = EvmH256::from_bytes(hb);
            let sender = EvmAddress::from_bytes([(i % 251) as u8; 20]);
            let nonce = (i / 251) as u64;
            let t = PendingEvmTx {
                hash: h, sender, nonce, gas_limit: 21_000,
                max_fee_per_gas: 1_000_000, max_priority_fee_per_gas: 0, raw: vec![0u8; 16], added_at: 1_000,
            };
            pool.total_bytes += t.raw.len();
            pool.by_sender_nonce.insert((sender, nonce), h);
            pool.txs.insert(h, t);
        }
        assert_eq!(pool.len(), EVM_MEMPOOL_MAX_TXS);

        // Another zero-tip squatter — even with an astronomically higher max_fee —
        // cannot evict, because it could never be SELECTED (effective tip 0).
        let squatter = PendingEvmTx {
            hash: EvmH256::from_bytes([0xFE; 32]), sender: EvmAddress::from_bytes([0xFE; 20]), nonce: 0,
            gas_limit: 21_000, max_fee_per_gas: u128::MAX, max_priority_fee_per_gas: 0, raw: vec![0u8; 16], added_at: 1_000,
        };
        assert!(matches!(pool.insert(squatter), Err(EvmMempoolError::Full { .. })));

        // A paying tx (positive effective tip) DOES evict a zero-tip slot and is admitted.
        let payer = PendingEvmTx {
            hash: EvmH256::from_bytes([0xAB; 32]), sender: EvmAddress::from_bytes([0xFF; 20]), nonce: 0,
            gas_limit: 21_000, max_fee_per_gas: 1_000_000, max_priority_fee_per_gas: 5, raw: vec![0u8; 16], added_at: 1_000,
        };
        let ph = pool.insert(payer).unwrap();
        assert!(pool.contains(&ph));
        assert_eq!(pool.len(), EVM_MEMPOOL_MAX_TXS, "evicted exactly one zero-tip squatter");
    }

    /// §9.2 producer-side claim queue: dedup by lock outpoint, deterministic
    /// sorted selection, capacity bound, idempotent re-submit.
    #[test]
    fn claim_queue_dedups_orders_and_bounds() {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let mk = |txid_byte: u8, idx: u32, amount: u64| DepositClaim {
            deposit_outpoint: TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([txid_byte; 64]), idx),
            evm_address: EvmAddress::from_bytes([txid_byte; 20]),
            amount_sompi: amount,
            claim_tip_sompi: 0,
        };
        let mut pool = EvmMempool::new();
        assert!(pool.insert_claim(mk(0xC, 1, 100)));
        assert!(pool.insert_claim(mk(0xA, 0, 200)));
        assert!(pool.insert_claim(mk(0xA, 5, 300)));
        assert_eq!(pool.claims_len(), 3);
        // Re-submit of the same outpoint is idempotent (replace, not grow).
        assert!(pool.insert_claim(mk(0xA, 0, 999)));
        assert_eq!(pool.claims_len(), 3);
        assert!(pool.contains_claim(&TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0xA; 64]), 0)));

        // Selection is sorted by (txid, index): A:0, A:5, then C:1.
        let sel = pool.select_claims(10);
        assert_eq!(sel.len(), 3);
        assert_eq!(sel[0].deposit_outpoint.index, 0);
        assert_eq!(sel[0].amount_sompi, 999, "the re-submit replaced the value");
        assert_eq!(sel[1].deposit_outpoint.index, 5);
        assert_eq!(sel[2].deposit_outpoint.transaction_id, kaspa_hashes::Hash64::from_bytes([0xC; 64]));
        // Cap honored.
        assert_eq!(pool.select_claims(2).len(), 2);
        // Removal.
        pool.remove_claim(&TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0xC; 64]), 1));
        assert_eq!(pool.claims_len(), 2);
    }

    /// §9.2 (claim-retry): a queued claim whose lock is ABSENT from the live view
    /// is RETAINED and retried — evicted only after `MAX_CLAIM_ABSENT_STRIKES`
    /// CONSECUTIVE absent templates. A template in which the lock reappears
    /// (`note_claim_present`) resets the run, so a forky DAG that flickers the lock
    /// in/out never evicts a claim whose deposit-lock is still being buried. This is
    /// the fix for "deposit claim dropped on the first stale view and never retried".
    #[test]
    fn claim_absent_strikes_retain_then_evict() {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let op = TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0x7; 64]), 0);
        let claim = DepositClaim {
            deposit_outpoint: op,
            evm_address: EvmAddress::from_bytes([0x7; 20]),
            amount_sompi: 1_000,
            claim_tip_sompi: 0,
        };
        let mut pool = EvmMempool::new();
        assert!(pool.insert_claim(claim.clone()));

        // A strike on a claim that is not queued is a no-op (never panics, never evicts).
        let ghost = TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([0x9; 64]), 0);
        assert!(!pool.note_claim_absent(&ghost));

        // Absent for MAX-1 consecutive templates → still RETAINED (no eviction signal).
        for _ in 0..(MAX_CLAIM_ABSENT_STRIKES - 1) {
            assert!(!pool.note_claim_absent(&op), "claim must be retained while under the strike cap");
        }
        assert!(pool.contains_claim(&op));

        // The lock reappears in a template → reset the consecutive-absent run.
        pool.note_claim_present(&op);

        // A full uninterrupted run of MAX absent templates is now required again: the
        // MAX-th consecutive strike is the one that signals eviction.
        for _ in 0..(MAX_CLAIM_ABSENT_STRIKES - 1) {
            assert!(!pool.note_claim_absent(&op));
        }
        assert!(pool.note_claim_absent(&op), "the MAX-th consecutive absent template signals eviction");
        pool.remove_claim(&op);
        assert!(!pool.contains_claim(&op));

        // Removal cleared the strike state: a re-submit gets a fresh retry window.
        assert!(pool.insert_claim(claim));
        assert!(!pool.note_claim_absent(&op));
    }

    /// §14.2 relay serve/filter primitives: `get_claim` returns the queued claim
    /// (the responder side) and `None` for an unknown outpoint (the request filter
    /// keys on `contains_claim`, which `get_claim` agrees with).
    #[test]
    fn claim_relay_get_and_contains() {
        use kaspa_consensus_core::tx::TransactionOutpoint;
        let op = |b: u8, idx: u32| TransactionOutpoint::new(kaspa_hashes::Hash64::from_bytes([b; 64]), idx);
        let claim = DepositClaim {
            deposit_outpoint: op(0xD, 2),
            evm_address: EvmAddress::from_bytes([0xD; 20]),
            amount_sompi: 4242,
            claim_tip_sompi: 7,
        };
        let mut pool = EvmMempool::new();
        // Unknown before insert.
        assert!(pool.get_claim(&op(0xD, 2)).is_none());
        assert!(!pool.contains_claim(&op(0xD, 2)));
        // Served verbatim after insert.
        assert!(pool.insert_claim(claim.clone()));
        assert_eq!(pool.get_claim(&op(0xD, 2)).as_ref(), Some(&claim));
        assert!(pool.contains_claim(&op(0xD, 2)));
        // A different outpoint is still unknown (the relay request-filter must keep requesting it).
        assert!(pool.get_claim(&op(0xD, 3)).is_none());
        assert!(!pool.contains_claim(&op(0xD, 3)));
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

        let selected = select(&pool, MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK);
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
        let selected = select(&pool, base + 104);
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

    /// P1 (the acute liveness fix): the template prunes already-accepted txs (nonce below the
    /// canonical state nonce) and selects the per-sender CONTIGUOUS run from the state nonce,
    /// parking a sender that has a nonce gap at its state nonce.
    #[test]
    fn select_is_contiguous_from_state_nonce_and_prunes_accepted() {
        let mut pool = EvmMempool::new();
        // Sender A: pool has nonces 0,1,2,3; state nonce 2 ⇒ 0,1 are already accepted.
        for n in 0..4u64 {
            pool.insert(tx(0xA, n, 100, 10, n as u8 + 1)).unwrap();
        }
        // Sender B: pool has 5,6; state nonce 4 ⇒ a GAP at 4 ⇒ the whole sender parks.
        pool.insert(tx(0xB, 5, 200, 10, 50)).unwrap();
        pool.insert(tx(0xB, 6, 200, 10, 51)).unwrap();

        let state: HashMap<EvmAddress, u64> =
            HashMap::from([(EvmAddress::from_bytes([0xA; 20]), 2u64), (EvmAddress::from_bytes([0xB; 20]), 4u64)]);

        pool.prune_below_state_nonce(&state);
        assert_eq!(pool.len(), 4, "A:0 and A:1 (below state nonce 2) are pruned");

        let sel = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &state, None);
        // Only A's contiguous run 2,3 is selectable; B parks on its nonce-4 gap.
        assert_eq!(sel.len(), 2, "A's contiguous run from state nonce 2; B parked on the gap");
        assert_eq!(sel[0], vec![3u8; 10], "A nonce 2 (tag 3) first");
        assert_eq!(sel[1], vec![4u8; 10], "then A nonce 3 (tag 4)");
    }

    /// REGRESSION (user's 2000-tx single-sender burst): when one sender's run
    /// exceeds the 128 KiB payload cap, the FIRST payload takes the contiguous
    /// head; once that head is canonically accepted (state nonce advances) and
    /// pruned, the SECOND payload must carry the TAIL forward — with no gap and
    /// no duplication. On the old `cb136a4` selector (no state-nonce reference,
    /// no prune-on-inclusion) the accepted head re-occupied each payload, so the
    /// tail was stranded at `included_in=[] / last_skip_class=0`. This pins the
    /// fixed behavior.
    #[test]
    fn accepted_prefix_does_not_starve_payload_tail() {
        const START: u64 = 2702;
        const END: u64 = 4702; // exclusive => 2000 txs
        const RAW: usize = 110; // ≈ a 1559 transfer envelope
        let sender = EvmAddress::from_bytes([0x71; 20]);
        let cap = MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK;

        let mut pool = pool_with_sender_range(0x71, START..END, RAW);
        assert_eq!(pool.len(), (END - START) as usize);

        // Payload #1: state nonce at START. The byte cap binds (2000×114 B ≫ 128 KiB),
        // so the selector takes the contiguous head [START, START+n1).
        let state1 = HashMap::from([(sender, START)]);
        let first = pool.select_candidates(cap, u64::MAX, 0, &state1, None);
        let n1 = first.len();
        assert!(n1 > 0 && n1 < (END - START) as usize, "the run is byte-capped into a prefix, not all-or-nothing (got {n1})");
        let first_nonces: Vec<u64> = first.iter().map(|r| nonce_of(r)).collect();
        assert_eq!(first_nonces, (START..START + n1 as u64).collect::<Vec<_>>(), "payload #1 = the contiguous head");

        // Payload #1 is accepted => state nonce advances; prune the accepted prefix.
        let state2 = HashMap::from([(sender, START + n1 as u64)]);
        pool.prune_below_state_nonce(&state2);
        assert_eq!(pool.len(), (END - START) as usize - n1, "accepted prefix pruned from the active pool");

        // Payload #2: the TAIL carries forward (no re-packing of the accepted
        // head, no gap, no duplicate) — exactly what cb136a4 stranded.
        let second = pool.select_candidates(cap, u64::MAX, 0, &state2, None);
        let second_nonces: Vec<u64> = second.iter().map(|r| nonce_of(r)).collect();
        let n2 = second.len();
        assert_eq!(
            second_nonces,
            (START + n1 as u64..START + n1 as u64 + n2 as u64).collect::<Vec<_>>(),
            "payload #2 = the contiguous tail starting exactly where #1 ended"
        );

        // No starvation: the two payload generations cover EVERY tx exactly once.
        assert_eq!(n1 + n2, (END - START) as usize, "all 2000 txs selected across two payload generations, none stranded");
        let mut all: Vec<u64> = first_nonces.into_iter().chain(second_nonces).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), (END - START) as usize, "no duplicate tx across the two payloads");
    }

    /// The declared-gas budget binds before the byte cap: uniform 21k-gas txs with
    /// a gas cap of 10×21k truncate the run at 10, regardless of how many would fit
    /// by bytes. (Every other selector test passes u64::MAX gas, so this is the only
    /// coverage of the `gas_limit <= gas_left` / `gas_left -=` budget branch.)
    #[test]
    fn selection_is_truncated_by_the_declared_gas_cap() {
        let sender = EvmAddress::from_bytes([0x33; 20]);
        let pool = pool_with_sender_range(0x33, 0..100, 50); // 100×54 B ≪ 128 KiB ⇒ bytes don't bind
        let state = HashMap::from([(sender, 0u64)]);
        let sel = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, 10 * 21_000, 0, &state, None);
        assert_eq!(sel.len(), 10, "run truncated by the declared-gas cap (10×21k), not the byte cap");
        let nonces: Vec<u64> = sel.iter().map(|r| nonce_of(r)).collect();
        assert_eq!(nonces, (0..10).collect::<Vec<_>>(), "contiguous head up to the gas budget");
    }

    /// While the canonical state nonce does NOT advance (the head is included in
    /// a DAG payload but not yet accepted), re-selecting must return the SAME
    /// contiguous head — the tx stays re-includable, never dropped on inclusion
    /// alone (inclusion ≠ acceptance).
    #[test]
    fn unaccepted_prefix_is_re_includable_across_templates() {
        const START: u64 = 100;
        let sender = EvmAddress::from_bytes([0x55; 20]);
        let pool = pool_with_sender_range(0x55, START..START + 300, 110);
        let state = HashMap::from([(sender, START)]);
        let a = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &state, None);
        let b = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &state, None);
        assert_eq!(a, b, "the same head re-selects identically while the state nonce is static");
        assert_eq!(a.first().map(|r| nonce_of(r)), Some(START), "head starts at the state nonce");
    }

    /// P1 (one-sender DoS bound): a single sender cannot exceed the per-sender pending-tx cap;
    /// a different sender is unaffected.
    #[test]
    fn per_sender_cap_bounds_one_sender() {
        let mut pool = EvmMempool::new();
        for n in 0..EVM_MEMPOOL_MAX_TXS_PER_SENDER as u64 {
            pool.insert(tx(0xA, n, 100, 10, 0)).unwrap();
        }
        assert_eq!(pool.len(), EVM_MEMPOOL_MAX_TXS_PER_SENDER);
        assert!(
            matches!(pool.insert(tx(0xA, 9_999, 100, 10, 0xFF)), Err(EvmMempoolError::SenderTxLimit { .. })),
            "one sender cannot exceed the per-sender cap"
        );
        assert!(pool.insert(tx(0xB, 0, 100, 10, 0xFE)).is_ok(), "a different sender is unaffected");
    }

    /// P1 (effective-tip ordering): a high-`max_fee` but ZERO-tip 1559 tx must NOT outrank a
    /// lower-`max_fee` tx that actually pays a tip — the miner's revenue is the tip.
    #[test]
    fn selection_orders_by_effective_tip_not_max_fee() {
        let mut pool = EvmMempool::new();
        let zero_tip = PendingEvmTx {
            hash: EvmH256::from_bytes([0xA, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            sender: EvmAddress::from_bytes([0xA; 20]),
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 1_000, // high ceiling …
            max_priority_fee_per_gas: 0, // … but no tip
            raw: vec![0xA1; 10],
            added_at: 1_000,
        };
        let real_tip = PendingEvmTx {
            hash: EvmH256::from_bytes([0xB, 0, 2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            sender: EvmAddress::from_bytes([0xB; 20]),
            nonce: 0,
            gas_limit: 21_000,
            max_fee_per_gas: 500, // lower ceiling …
            max_priority_fee_per_gas: 100, // … but a real tip
            raw: vec![0xB2; 10],
            added_at: 1_000,
        };
        pool.insert(zero_tip).unwrap();
        pool.insert(real_tip).unwrap();
        // base_fee 400: zero_tip effective tip = min(0, 1000-400) = 0; real_tip = min(100, 500-400) = 100.
        let sel = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 400, &HashMap::new(), None);
        assert_eq!(sel.len(), 2);
        assert_eq!(sel[0], vec![0xB2; 10], "the paying tx (real tip) is selected before the zero-tip high-max-fee tx");
    }

    /// Audit H-10: with a balance view, `select_candidates` skips a sender that cannot
    /// cover a tx's up-front gas reservation (gas_limit × max_fee_per_gas). The tx is
    /// not evicted — it is simply passed over for this template. Absent ⇒ balance 0.
    #[test]
    fn selection_skips_unfundable_senders_h10() {
        // gas reservation per tx = 21_000 (tx() gas_limit) × max_fee.
        let mut pool = EvmMempool::new();
        pool.insert(tx(0xA, 0, 100, 10, 0xAA)).unwrap(); // tip 100, reservation 2_100_000
        pool.insert(tx(0xB, 0, 50, 10, 0xBB)).unwrap(); // tip 50,  reservation 1_050_000
        let addr_a = EvmAddress::from_bytes([0xA; 20]);
        let nonces: HashMap<EvmAddress, u64> = HashMap::new();

        // No balance view: both selected (A first — higher effective tip).
        let all = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &nonces, None);
        assert_eq!(all.len(), 2, "without a balance view, selection is unchanged");

        // A funded, B absent (⇒ balance 0): only A is selected.
        let mut bals: HashMap<EvmAddress, u128> = HashMap::new();
        bals.insert(addr_a, 10_000_000);
        let sel = pool.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &nonces, Some(&bals));
        assert_eq!(sel.len(), 1, "the unfunded sender is skipped");
        assert_eq!(sel[0], vec![0xAA; 10], "the funded sender's tx is the one selected");

        // Running balance within ONE sender's run: balance covers 2 of 3 contiguous txs.
        let mut pool2 = EvmMempool::new();
        pool2.insert(tx(0xC, 0, 100, 10, 0xC0)).unwrap();
        pool2.insert(tx(0xC, 1, 100, 10, 0xC1)).unwrap();
        pool2.insert(tx(0xC, 2, 100, 10, 0xC2)).unwrap();
        let addr_c = EvmAddress::from_bytes([0xC; 20]);
        let mut bals2: HashMap<EvmAddress, u128> = HashMap::new();
        bals2.insert(addr_c, 5_000_000); // covers 2 × 2_100_000 = 4_200_000, not the 3rd
        let sel2 = pool2.select_candidates(MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK, u64::MAX, 0, &HashMap::new(), Some(&bals2));
        assert_eq!(sel2.len(), 2, "the running balance funds 2 of the sender's 3 contiguous txs");
    }
}
