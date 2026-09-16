use crate::{
    Policy,
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    mempool::{
        attestation::{AttestationIndex, AttestationQuarantine, QuarantineReason, extract_attestation_meta},
        config::Config,
        errors::{RuleError, RuleResult},
        model::{
            map::MempoolTransactionCollection,
            pool::{Pool, TransactionsEdges},
            tx::{DoubleSpend, MempoolTransaction},
            utxo_set::MempoolUtxoSet,
        },
        tx::Priority,
    },
    model::{TransactionIdSet, topological_index::TopologicalIndex},
};
use kaspa_consensus_core::{
    block::TemplateTransactionSelector,
    tx::{MutableTransaction, TransactionId, TransactionOutpoint},
};
use kaspa_core::{debug, time::unix_now, trace};
use std::{
    collections::{HashSet, hash_map::Keys, hash_set::Iter},
    iter::once,
    sync::Arc,
};

use super::frontier::Frontier;

/// Pool of transactions to be included in a block template
///
/// ### Rust rewrite notes
///
/// The main design decision is to have [MempoolTransaction]s owned by [all_transactions]
/// without any other external reference so no smart pointer is needed.
///
/// This has following consequences:
///
/// - highPriorityTransactions is dropped in favour of an in-place filtered iterator.
/// - MempoolTransaction.parentTransactionsInPool is moved here and replaced by a map from
///   an id to a set of parent transaction ids introducing an indirection stage when
///   a matching object is required.
/// - chainedTransactionsByParentID maps an id instead of a transaction reference
///   introducing a indirection stage when the matching object is required.
/// - Hash sets are used by parent_transaction_ids_in_pool and chained_transaction_ids_by_parent_id
///   instead of vectors to prevent duplicates.
/// - transactionsOrderedByFeeRate is dropped and replaced by an in-place vector
///   of low-priority transactions sorted by fee rates. This design might eventually
///   prove to be sub-optimal, in which case an index should be implemented, probably
///   requiring smart pointers eventually or an indirection stage too.
pub(crate) struct TransactionsPool {
    /// Mempool config
    config: Arc<Config>,

    /// Store of transactions.
    /// Any mutable access to this map should be carefully reviewed for consistency with all other collections
    /// and fields of this struct. In particular, `estimated_size` must reflect the exact sum of estimated size
    /// for all current transactions in this collection.
    all_transactions: MempoolTransactionCollection,

    /// Transactions dependencies formed by inputs present in pool - ancestor relations.
    parent_transactions: TransactionsEdges,

    /// Transactions dependencies formed by outputs present in pool - successor relations.
    chained_transactions: TransactionsEdges,

    /// Transactions with no parents in the mempool -- ready to be inserted into a block template
    ready_transactions: Frontier,

    last_expire_scan_daa_score: u64,

    /// last expire scan time in milliseconds
    last_expire_scan_time: u64,

    /// Sum of estimated size for all transactions currently held in `all_transactions`
    estimated_size: usize,

    /// Store of UTXOs
    utxo_set: MempoolUtxoSet,

    /// kaspa-pq DNS-finality: index of `StakeAttestationShard` txs currently in the pool, kept
    /// consistent with `all_transactions` (insert on add, remove on every removal path). Empty /
    /// inert whenever the attestation policy is disabled (no shard tx is ever indexed since none is
    /// admitted on a net without `dns_params`, and the maintenance code is also guarded).
    attestation_index: AttestationIndex,

    /// kaspa-pq audit v26 (H-4): short-term quarantine for attestation shards the template
    /// classifier dropped transiently. Quarantined shards are held out of BOTH the priority
    /// and inner selector lanes until their hold lapses, breaking the "drop -> re-select ->
    /// drop" template loop without hard-evicting a recoverable bond. Empty / inert when the
    /// attestation overlay is off.
    attestation_quarantine: AttestationQuarantine,
}

impl TransactionsPool {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        let target_time_per_block = 1.0 / (config.network_blocks_per_second as f64);
        Self {
            config,
            all_transactions: MempoolTransactionCollection::default(),
            parent_transactions: TransactionsEdges::default(),
            chained_transactions: TransactionsEdges::default(),
            ready_transactions: Frontier::new(target_time_per_block),
            last_expire_scan_daa_score: 0,
            last_expire_scan_time: unix_now(),
            utxo_set: MempoolUtxoSet::new(),
            estimated_size: 0,
            attestation_index: AttestationIndex::default(),
            attestation_quarantine: AttestationQuarantine::default(),
        }
    }

    /// Add a mutable transaction to the pool
    pub(crate) fn add_transaction(
        &mut self,
        transaction: MutableTransaction,
        virtual_daa_score: u64,
        priority: Priority,
        transaction_size: usize,
    ) -> RuleResult<&MempoolTransaction> {
        let transaction = MempoolTransaction::new(transaction, priority, virtual_daa_score);
        let id = transaction.id();
        self.add_mempool_transaction(transaction, transaction_size)?;
        Ok(self.get(&id).unwrap())
    }

    /// Add a mempool transaction to the pool
    pub(crate) fn add_mempool_transaction(&mut self, transaction: MempoolTransaction, transaction_size: usize) -> RuleResult<()> {
        let id = transaction.id();

        assert!(!self.all_transactions.contains_key(&id), "transaction {id} to be added already exists in the transactions pool");
        assert!(transaction.mtx.is_fully_populated(), "transaction {id} to be added in the transactions pool is not fully populated");

        // Create the bijective parent/chained relations.
        // This concerns only the parents of the added transaction.
        // The transactions chained to the added transaction cannot be stored
        // here yet since, by definition, they would have been orphans.
        let parents = self.get_parent_transaction_ids_in_pool(&transaction.mtx);
        self.parent_transactions.insert(id, parents.clone());
        if parents.is_empty() {
            self.ready_transactions.insert((&transaction).into());
        }
        for parent_id in parents {
            let entry = self.chained_transactions.entry(parent_id).or_default();
            entry.insert(id);
        }

        self.utxo_set.add_transaction(&transaction.mtx);
        self.estimated_size += transaction_size;

        // kaspa-pq DNS-finality: index attestation-shard txs so they can be expired/deduped and
        // preferred during template selection. Guarded on the policy so it is a no-op when the
        // overlay is off. A decode failure here is only logged (the tx was already accepted by
        // consensus-equivalent validation; the index is best-effort policy state).
        if self.config.attestation_policy.enabled
            && let Some(result) = extract_attestation_meta(&transaction.mtx, transaction.added_at_daa_score, transaction.priority)
        {
            match result {
                Ok(meta) => self.attestation_index.insert(meta),
                Err(err) => trace!("Skipping attestation index insert for {}: {}", id, err),
            }
        }

        self.all_transactions.insert(id, transaction);
        trace!("Added transaction {}", id);
        Ok(())
    }

    /// Fully removes the transaction from all relational sets, as well as from the UTXO set
    pub(crate) fn remove_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        // Remove all bijective parent/chained relations
        if let Some(parents) = self.parent_transactions.get(transaction_id) {
            for parent in parents.iter() {
                if let Some(chains) = self.chained_transactions.get_mut(parent) {
                    chains.remove(transaction_id);
                }
            }
        }
        if let Some(chains) = self.chained_transactions.get(transaction_id) {
            for chain in chains.iter() {
                if let Some(parents) = self.parent_transactions.get_mut(chain) {
                    parents.remove(transaction_id);
                    if parents.is_empty() {
                        let tx = self.all_transactions.get(chain).unwrap();
                        self.ready_transactions.insert(tx.into());
                    }
                }
            }
        }
        self.parent_transactions.remove(transaction_id);
        self.chained_transactions.remove(transaction_id);

        // Remove the transaction itself
        let removed_tx = self.all_transactions.remove(transaction_id).ok_or(RuleError::RejectMissingTransaction(*transaction_id))?;

        self.ready_transactions.remove(&(&removed_tx).into());

        // kaspa-pq DNS-finality: keep the attestation index from leaking. This is the single
        // internal removal site (all mempool removal paths funnel through it), so deregistering
        // here covers every case. No-op for non-attestation txs / when the overlay is off.
        if self.config.attestation_policy.enabled {
            self.attestation_index.remove(transaction_id);
            // kaspa-pq audit v26 (H-4): keep the quarantine consistent with the index — a
            // removed (evicted/accepted) shard must not linger as a quarantine entry.
            self.attestation_quarantine.remove(transaction_id);
        }

        // TODO: consider using `self.parent_transactions.get(transaction_id)`
        // The tradeoff to consider is whether it might be possible that a parent tx exists in the pool
        // however its relation as parent is not registered. This can supposedly happen in rare cases where
        // the parent was removed w/o redeemers and then re-added
        let parent_ids = self.get_parent_transaction_ids_in_pool(&removed_tx.mtx);

        // Remove the transaction from the mempool UTXO set
        self.utxo_set.remove_transaction(&removed_tx.mtx, &parent_ids);
        self.estimated_size -= removed_tx.mtx.mempool_estimated_bytes();

        if self.all_transactions.is_empty() {
            assert_eq!(0, self.estimated_size, "Sanity test -- if tx pool is empty, estimated byte size should be zero");
        }

        Ok(removed_tx)
    }

    pub(crate) fn update_revalidated_transaction(&mut self, transaction: MutableTransaction) -> bool {
        if let Some(tx) = self.all_transactions.get_mut(&transaction.id()) {
            // Make sure to update the overall estimated size since the updated transaction might have a different size
            self.estimated_size -= tx.mtx.mempool_estimated_bytes();
            tx.mtx = transaction;
            self.estimated_size += tx.mtx.mempool_estimated_bytes();
            true
        } else {
            false
        }
    }

    pub(crate) fn ready_transaction_count(&self) -> usize {
        self.ready_transactions.len()
    }

    pub(crate) fn ready_transaction_total_mass(&self) -> u64 {
        self.ready_transactions.total_mass()
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier.
    ///
    /// When the attestation overlay is enabled and an epoch is ready, the priority attestation
    /// shards are yielded before normal txs, oldest ready epoch first. Optional shard tx/mass
    /// budgets are honored only when configured; the default
    /// overlay policy leaves them unlimited at this selector layer and relies on block mass. When
    /// the overlay is off (or no epoch is ready) this is byte-identical to the upstream path.
    pub(crate) fn build_selector(&self, latest_ready_epoch: Option<u64>) -> Box<dyn TemplateTransactionSelector> {
        let policy = &self.config.attestation_policy;
        let base_policy = Policy::new(self.config.maximum_mass_per_block)
            .with_max_attestation_shard_txs(policy.max_attestation_shard_txs_per_block)
            .with_max_attestation_shard_mass(policy.max_attestation_shard_mass_per_block);

        // Fast path: overlay off, no ready epoch, or no attestation shards in the pool.
        let Some(latest_ready_epoch) = latest_ready_epoch else {
            return self.ready_transactions.build_selector(&base_policy);
        };
        if !policy.enabled || self.attestation_index.is_empty() {
            return self.ready_transactions.build_selector(&base_policy);
        }

        let priority = self.build_attestation_priority_set(latest_ready_epoch);

        // kaspa-pq audit v26 (M-1 + H-4): compute the quarantine + future-epoch exclude set FIRST,
        // because it must apply to BOTH the empty-priority fallback below AND the compose path.
        //  - M-1: a future-within-grace shard (admission allows `shard_epoch` up to
        //    `latest_ready_epoch + grace`) is intentionally absent from the priority lane (which
        //    skips `epoch > latest_ready_epoch`); the INNER lane has no such filter and would
        //    otherwise leak it into the template ahead of its epoch.
        //  - H-4: a currently-quarantined (transiently-dropped) shard must not leak back in via the
        //    non-priority lane, re-triggering the drop→re-select→drop loop the quarantine breaks.
        // Computing this BEFORE the `priority.is_empty()` fallback closes the single-eligible-shard /
        // DNS-overlay-wedge case (only shard is quarantined/future ⇒ empty priority set), where the
        // old fallback returned an UNFILTERED selector and leaked the shard.
        let mut exclude: std::collections::HashSet<TransactionId> = std::collections::HashSet::new();
        for (tx_id, meta) in self.attestation_index.by_txid.iter() {
            if meta.shard_epoch > latest_ready_epoch || self.attestation_quarantine.is_active(tx_id, latest_ready_epoch) {
                exclude.insert(*tx_id);
            }
        }
        if priority.is_empty() {
            // No priority shards, but any quarantined/future shards must STILL be kept out of the
            // fallback selector. When there is nothing to exclude this is byte-identical to the
            // prior fast path.
            return if exclude.is_empty() {
                self.ready_transactions.build_selector(&base_policy)
            } else {
                self.ready_transactions.build_selector_excluding(&base_policy, &exclude)
            };
        }

        // Compose: priority attestation shards first, then ordinary txs under one shared dynamic
        // block-mass/shard budget. This is deliberately not a fixed "remaining mass" selector: if a
        // priority shard is later dropped by the template classifier, its mass and shard-cap unit
        // must become available to the ordinary lane immediately.
        exclude.extend(priority.iter().map(|t| t.tx.id()));
        let remainder = self.ready_transactions.sequence_excluding(&exclude);
        Box::new(crate::mempool::model::frontier::selectors::AttestationPrioritySelector::new(priority, remainder, base_policy))
    }

    /// kaspa-pq DNS-finality (P1): pick the priority attestation-shard set from the ready frontier.
    ///
    /// Deterministic order: stake-score-window shards first, then by epoch ascending, then feerate descending, then txid. Bounded by
    /// `max_attestation_shard_txs_per_block`, `max_attestation_shard_mass_per_block`, and the block
    /// mass. "Recent" means `epoch in [latest_ready_epoch - required_stake_depth_epochs + 1,
    /// latest_ready_epoch]`; reward-fresh means
    /// `epoch <= latest_ready_epoch && latest_ready_epoch - epoch <= reward_uniqueness_window_blocks
    /// / epoch_len` (a coarse, conservative recency check using epoch units).
    ///
    /// kaspa-pq audit v24 (H-1): FUTURE-epoch shards (`epoch > latest_ready_epoch`) are NEVER
    /// candidates. The old `latest_ready_epoch.saturating_sub(epoch) <= reward_window_epochs` test
    /// underflow-saturated to `0` for future shards, classifying them as "reward-fresh" and letting
    /// them into the priority lane ahead of genuinely-rewardable current shards. Both freshness
    /// predicates now require `epoch <= latest_ready_epoch`.
    ///
    /// kaspa-pq attestation liveness: optional hard-inclusion deployments clear deficient ready
    /// epochs oldest-first. The selector therefore feeds old stake-score-window shards before newer
    /// ones; otherwise hard-inclusion miners can repeatedly build templates full of recent
    /// attestations while consensus rejects them for missing an older deficient epoch.
    fn build_attestation_priority_set(
        &self,
        latest_ready_epoch: u64,
    ) -> Vec<crate::mempool::model::frontier::selectors::SequenceSelectorTransaction> {
        use crate::mempool::model::frontier::selectors::SequenceSelectorTransaction;

        let policy = &self.config.attestation_policy;
        let max_txs = policy.max_attestation_shard_txs_per_block;
        let max_mass = policy.max_attestation_shard_mass_per_block;
        let block_mass = self.config.maximum_mass_per_block;
        let mut selected = Vec::new();
        let mut selected_mass: u64 = 0;
        let mut selected_ids: HashSet<TransactionId> = HashSet::new();

        let try_select = |tx: Arc<kaspa_consensus_core::tx::Transaction>,
                          mass: u64,
                          selected: &mut Vec<SequenceSelectorTransaction>,
                          selected_mass: &mut u64,
                          selected_ids: &mut HashSet<TransactionId>|
         -> bool {
            if selected_ids.contains(&tx.id()) {
                return false;
            }
            if max_txs > 0 && selected.len() as u64 >= max_txs {
                return false;
            }
            let next_mass = selected_mass.saturating_add(mass);
            if next_mass > block_mass {
                return false;
            }
            if max_mass > 0 && next_mass > max_mass {
                return false;
            }
            *selected_mass = next_mass;
            selected_ids.insert(tx.id());
            selected.push(SequenceSelectorTransaction::new(tx, mass));
            true
        };

        let epoch_len = policy.epoch_len_blue_score.max(1);
        let depth = policy.required_stake_depth_epochs;
        let recent_window_start = latest_ready_epoch.saturating_sub(depth.saturating_sub(1));
        let score_window_epochs = policy.stake_score_window_blue_score.div_ceil(epoch_len);
        let hard_retention_epochs = policy.hard_retention_epochs();
        // reward-uniqueness window expressed in epoch units (coarse, conservative).
        let reward_window_epochs = policy.reward_uniqueness_window_blocks / epoch_len;

        // Collect candidate attestation shards that are READY (present in the frontier).
        struct Cand {
            tx: Arc<kaspa_consensus_core::tx::Transaction>,
            mass: u64,
            epoch: u64,
            feerate: f64,
            in_score_window: bool,
            in_recent_window: bool,
        }
        let mut candidates: Vec<Cand> = Vec::new();
        for key in self.ready_transactions.keys_ascending_iter() {
            let tx_id = key.tx.id();
            if selected_ids.contains(&tx_id) {
                continue;
            }
            if let Some(meta) = self.attestation_index.get(&tx_id) {
                // kaspa-pq audit v26 (H-4): a quarantined shard is held out of the priority
                // lane until its hold lapses (a transient template drop should not be
                // re-selected on the very next build).
                if self.attestation_quarantine.is_active(&tx_id, latest_ready_epoch) {
                    continue;
                }
                let epoch = meta.shard_epoch;
                // kaspa-pq audit v24 (H-1): a future-epoch shard is never canonical/rewardable
                // for the current ready epoch and must not enter the priority lane.
                if epoch > latest_ready_epoch {
                    continue;
                }
                let age = latest_ready_epoch - epoch;
                let in_recent_window = epoch >= recent_window_start; // epoch <= latest_ready_epoch already holds.
                let in_score_window = age <= score_window_epochs;
                // kaspa-pq audit v24 (H-1): subtraction is now underflow-safe (epoch <= latest).
                let reward_fresh = age <= reward_window_epochs;
                let in_priority_horizon = age <= hard_retention_epochs;
                if in_score_window || in_recent_window || reward_fresh || in_priority_horizon {
                    candidates.push(Cand {
                        tx: key.tx.clone(),
                        mass: key.mass,
                        epoch,
                        feerate: key.feerate(),
                        in_score_window,
                        in_recent_window,
                    });
                }
            }
        }

        // Deterministic order: score-window first, then oldest epoch first,
        // then recent-window, feerate desc, txid asc.
        candidates.sort_by(|a, b| {
            b.in_score_window
                .cmp(&a.in_score_window)
                .then(a.epoch.cmp(&b.epoch))
                .then(b.in_recent_window.cmp(&a.in_recent_window))
                .then(b.feerate.partial_cmp(&a.feerate).unwrap_or(std::cmp::Ordering::Equal))
                .then(a.tx.id().cmp(&b.tx.id()))
        });

        for cand in candidates {
            let _ = try_select(cand.tx, cand.mass, &mut selected, &mut selected_mass, &mut selected_ids);
        }
        selected
    }

    /// Builds a feerate estimator based on internal state of the ready transactions frontier
    pub(crate) fn build_feerate_estimator(&self, args: FeerateEstimatorArgs) -> FeerateEstimator {
        self.ready_transactions.build_feerate_estimator(args)
    }

    /// Returns the exceeding low-priority transactions having the lowest fee rates in order
    /// to make room for `transaction`. The returned transactions
    /// are guaranteed to be unchained (no successor in mempool) and to not be parent of
    /// `transaction`.
    ///
    /// An error is returned if the mempool is filled with high priority transactions, or
    /// there are not enough lower feerate transactions that can be removed to accommodate `transaction`
    pub(crate) fn limit_transaction_count(
        &self,
        transaction: &MutableTransaction,
        transaction_size: usize,
    ) -> RuleResult<Vec<TransactionId>> {
        // No eviction needed -- return
        if self.len() < self.config.maximum_transaction_count
            && self.estimated_size + transaction_size <= self.config.mempool_size_limit
        {
            return Ok(Default::default());
        }

        // Returns a vector of transactions to be removed (the caller has to actually remove)
        let feerate_threshold = transaction.calculated_feerate().unwrap();
        let mut txs_to_remove = Vec::with_capacity(1); // Normally we expect a single removal
        let mut selection_overall_size = 0;
        for tx in self
            .ready_transactions
            .ascending_iter()
            .map(|tx| self.all_transactions.get(&tx.id()).unwrap())
            .filter(|mtx| mtx.priority == Priority::Low)
        {
            // TODO (optimization): inline the `has_parent_in_set` check within the redeemer traversal and exit early if possible
            let redeemers = self.get_redeemer_ids_in_pool(&tx.id()).into_iter().chain(once(tx.id())).collect::<TransactionIdSet>();
            if transaction.has_parent_in_set(&redeemers) {
                continue;
            }

            // We are iterating ready txs by ascending feerate so the pending tx has lower feerate than all remaining txs
            if tx.feerate() > feerate_threshold {
                let err = RuleError::RejectMempoolIsFull;
                debug!("Transaction {} with feerate {} has been rejected: {}", transaction.id(), feerate_threshold, err);
                return Err(err);
            }

            txs_to_remove.push(tx.id());
            selection_overall_size += tx.mtx.mempool_estimated_bytes();

            if self.len() + 1 - txs_to_remove.len() <= self.config.maximum_transaction_count
                && self.estimated_size + transaction_size - selection_overall_size <= self.config.mempool_size_limit
            {
                return Ok(txs_to_remove);
            }
        }

        // We could not find sufficient space for the pending transaction
        debug!(
            "Mempool is filled with high-priority/ancestor txs (count: {}, bytes: {}). Transaction {} with feerate {} and size {} has been rejected: {}",
            self.len(),
            self.estimated_size,
            transaction.id(),
            feerate_threshold,
            transaction_size,
            RuleError::RejectMempoolIsFull
        );
        Err(RuleError::RejectMempoolIsFull)
    }

    pub(crate) fn get_estimated_size(&self) -> usize {
        self.estimated_size
    }

    pub(crate) fn all_transaction_ids_with_priority(&self, priority: Priority) -> Vec<TransactionId> {
        self.all().values().filter_map(|x| if x.priority == priority { Some(x.id()) } else { None }).collect()
    }

    pub(crate) fn get_outpoint_owner_id(&self, outpoint: &TransactionOutpoint) -> Option<&TransactionId> {
        self.utxo_set.get_outpoint_owner_id(outpoint)
    }

    /// Make sure no other transaction in the mempool is already spending an output which one of this transaction inputs spends
    pub(crate) fn check_double_spends(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        self.utxo_set.check_double_spends(transaction)
    }

    /// Returns the first double spend of every transaction in the mempool double spending on `transaction`
    pub(crate) fn get_double_spend_transaction_ids(&self, transaction: &MutableTransaction) -> Vec<DoubleSpend> {
        self.utxo_set.get_double_spend_transaction_ids(transaction)
    }

    pub(crate) fn get_double_spend_owner<'a>(&'a self, double_spend: &DoubleSpend) -> RuleResult<&'a MempoolTransaction> {
        match self.get(&double_spend.owner_id) {
            Some(transaction) => Ok(transaction),
            None => {
                // This case should never arise in the first place.
                // Anyway, in case it does, if a double spent transaction id is found but the matching
                // transaction cannot be located in the mempool a replacement is no longer possible
                // so a double spend error is returned.
                Err(double_spend.into())
            }
        }
    }

    pub(crate) fn collect_expired_low_priority_transactions(&mut self, virtual_daa_score: u64) -> Vec<TransactionId> {
        let now = unix_now();
        if virtual_daa_score < self.last_expire_scan_daa_score + self.config.transaction_expire_scan_interval_daa_score
            || now < self.last_expire_scan_time + self.config.transaction_expire_scan_interval_milliseconds
        {
            return vec![];
        }

        self.last_expire_scan_daa_score = virtual_daa_score;
        self.last_expire_scan_time = now;

        // Never expire high priority transactions
        // Remove all transactions whose added_at_daa_score is older then transaction_expire_interval_daa_score
        self.all_transactions
            .values()
            .filter_map(|x| {
                if (x.priority == Priority::Low)
                    && virtual_daa_score > x.added_at_daa_score + self.config.transaction_expire_interval_daa_score
                {
                    Some(x.id())
                } else {
                    None
                }
            })
            .collect()
    }

    /// kaspa-pq DNS-finality: collect attestation-shard txs whose `shard_epoch` is older than the
    /// hard-retention horizon relative to `latest_ready_epoch`. Unlike the low-priority sweep above,
    /// these expire **even if high priority** — that is the whole point of the fix (the validator's
    /// RPC-submitted shards are high priority and would otherwise never expire).
    ///
    /// Returns empty when the policy is disabled or no epoch is ready yet (`latest_ready_epoch`
    /// is `None`).
    pub(crate) fn collect_expired_attestation_shards(&mut self, latest_ready_epoch: Option<u64>) -> Vec<TransactionId> {
        if !self.config.attestation_policy.enabled {
            return vec![];
        }
        let Some(latest_ready_epoch) = latest_ready_epoch else {
            return vec![];
        };
        self.attestation_index.collect_hard_expired(latest_ready_epoch, self.config.attestation_policy.hard_retention_epochs())
    }

    /// kaspa-pq DNS-finality: number of attestation-shard txs currently indexed in the pool.
    pub(crate) fn attestation_tx_count(&self) -> usize {
        self.attestation_index.len()
    }

    /// kaspa-pq DNS-finality: read access to the attestation index (dedup / replacement decisions).
    pub(crate) fn attestation_index(&self) -> &AttestationIndex {
        &self.attestation_index
    }

    /// kaspa-pq audit v26 (H-4): quarantine a template-transient-dropped shard until
    /// `until_epoch` (exclusive), so it is held out of both selector lanes instead of being
    /// re-selected into every subsequent template. No-op when the overlay is off (the manager
    /// only calls this on enabled nets, but guard defensively).
    pub(crate) fn quarantine_attestation_shard(&mut self, tx_id: TransactionId, until_epoch: u64) {
        if !self.config.attestation_policy.enabled {
            return;
        }
        self.attestation_quarantine.insert(tx_id, QuarantineReason::TemplateTransient, until_epoch);
    }

    /// kaspa-pq audit v26 (H-4): drop quarantine entries whose hold has lapsed at
    /// `current_epoch`, so recovered bonds become re-selectable. Called on new-block / TTL
    /// sweep. No-op when the overlay is off.
    pub(crate) fn retain_active_attestation_quarantine(&mut self, current_epoch: u64) {
        if !self.config.attestation_policy.enabled {
            return;
        }
        self.attestation_quarantine.retain_active(current_epoch);
    }

    /// kaspa-pq audit v26 (H-4): number of shards currently in quarantine (for the gauge).
    pub(crate) fn attestation_quarantine_len(&self) -> usize {
        self.attestation_quarantine.len()
    }
}

type IterTxId<'a> = Iter<'a, TransactionId>;
type KeysTxId<'a> = Keys<'a, TransactionId, MempoolTransaction>;

impl<'a> TopologicalIndex<'a, KeysTxId<'a>, IterTxId<'a>, TransactionId> for TransactionsPool {
    fn topology_nodes(&'a self) -> KeysTxId<'a> {
        self.all_transactions.keys()
    }

    fn topology_node_edges(&'a self, key: &TransactionId) -> Option<IterTxId<'a>> {
        self.chained_transactions.get(key).map(|x| x.iter())
    }
}

impl Pool for TransactionsPool {
    #[inline]
    fn all(&self) -> &MempoolTransactionCollection {
        &self.all_transactions
    }

    #[inline]
    fn chained(&self) -> &TransactionsEdges {
        &self.chained_transactions
    }
}

#[cfg(test)]
mod attestation_priority_tests {
    use super::*;
    use crate::mempool::{attestation::AttestationMempoolPolicy, config::Config};
    use kaspa_consensus_core::{
        constants::TX_VERSION,
        dns_finality::{StakeAttestation, StakeAttestationShardPayload},
        mass::NonContextualMasses,
        subnets::SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
        tx::{Transaction, TransactionOutpoint},
    };
    use kaspa_hashes::Hash64;

    fn hash64(b: u8) -> Hash64 {
        Hash64::from_bytes([b; 64])
    }

    /// A ready (input-less, fee-funded) attestation-shard tx for `epoch` with a unique key.
    fn shard_mtx(epoch: u64, validator: u8) -> MutableTransaction {
        shard_mtx_with_mass_and_fee(epoch, validator, 1000, 10_000)
    }

    fn shard_mtx_with_mass_and_fee(epoch: u64, validator: u8, mass: u64, fee: u64) -> MutableTransaction {
        let att = StakeAttestation {
            version: 1,
            validator_id: hash64(validator),
            bond_outpoint: TransactionOutpoint::new(hash64(0xaa), validator as u32),
            epoch,
            target_hash: hash64(0xbb),
            target_daa_score: 1234,
            validator_set_commitment: Hash64::default(),
            signature: vec![],
        };
        let payload = StakeAttestationShardPayload {
            version: 1,
            epoch,
            target_hash: hash64(0xbb),
            target_daa_score: 1234,
            validator_set_commitment: Hash64::default(),
            attestations: vec![att],
        };
        let payload = borsh::to_vec(&payload).unwrap();
        let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_ATTESTATION_SHARD, 0, payload);
        let mut mtx = MutableTransaction::from_tx(tx);
        mtx.calculated_fee = Some(fee);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(mass, mass));
        mtx
    }

    fn enabled_policy() -> AttestationMempoolPolicy {
        AttestationMempoolPolicy {
            enabled: true,
            epoch_len_blue_score: 100,
            attestation_lag_blue_score: 0,
            stake_score_window_blue_score: 300,
            reward_uniqueness_window_blocks: 200, // reward_window_epochs = 200/100 = 2
            required_stake_depth_epochs: 2,       // recent window = [latest-1, latest]
            hard_retention_grace_epochs: 2,
            replacement_bump_pct: 10,
            max_attestation_mempool_txs: 100_000,
            max_attestation_txs_per_key: 1,
            max_attestation_shard_txs_per_block: 16,
            max_attestation_shard_mass_per_block: 0,
            quarantine_epochs: 1,
        }
    }

    fn pool_with_policy(policy: AttestationMempoolPolicy) -> TransactionsPool {
        pool_with_policy_and_block_mass(policy, 500_000)
    }

    fn pool_with_policy_and_block_mass(policy: AttestationMempoolPolicy, max_block_mass: u64) -> TransactionsPool {
        let mut config = Config::build_default(1000, false, max_block_mass);
        config.attestation_policy = policy;
        TransactionsPool::new(Arc::new(config))
    }

    fn add_shard(pool: &mut TransactionsPool, mtx: MutableTransaction) -> TransactionId {
        let size = mtx.mempool_estimated_bytes();
        let id = mtx.id();
        pool.add_transaction(mtx, 0, Priority::High, size).unwrap();
        id
    }

    /// kaspa-pq audit v24 (H-1): a FUTURE-epoch shard (epoch > latest_ready_epoch) must NEVER
    /// enter the attestation priority lane. Before the fix the underflow-saturating freshness
    /// test mis-classified future shards as "reward-fresh".
    #[test]
    fn future_epoch_shard_excluded_from_priority_lane() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;

        let current_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 1)); // rewardable
        let future_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch + 5, 2)); // far future

        let priority = pool.build_attestation_priority_set(latest_ready_epoch);
        let ids: Vec<_> = priority.iter().map(|t| t.tx.id()).collect();
        assert!(ids.contains(&current_id), "the current-epoch shard must be in the priority lane");
        assert!(!ids.contains(&future_id), "a future-epoch shard must NOT enter the priority lane (H-1)");
    }

    /// kaspa-pq attestation liveness: within the stake-score window, older ready epochs are selected
    /// before newer ones so optional hard-inclusion deployments can clear deficiencies oldest-first.
    #[test]
    fn oldest_score_window_shards_precede_newer_shards() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;

        // stake-score window = 3 epochs, so both epoch 8 and 10 are mandatory-capable; epoch 8 wins.
        let older = add_shard(&mut pool, shard_mtx(8, 1));
        let newer = add_shard(&mut pool, shard_mtx(10, 2));

        let priority = pool.build_attestation_priority_set(latest_ready_epoch);
        let ids: Vec<_> = priority.iter().map(|t| t.tx.id()).collect();
        assert!(ids.contains(&older) && ids.contains(&newer));
        let pos_older = ids.iter().position(|id| *id == older).unwrap();
        let pos_newer = ids.iter().position(|id| *id == newer).unwrap();
        assert!(pos_older < pos_newer, "oldest ready shard must precede newer shards in the priority lane");
    }

    /// kaspa-pq audit v24 (H-2): the priority lane consumes part of the per-block shard budget;
    /// the inner selector must receive only the REMAINING budget. With a per-block shard cap of 1
    /// and one rewardable shard, the priority lane takes it and the inner lane gets 0 remaining,
    /// so the total block never carries more than 1 shard.
    #[test]
    fn priority_lane_consumes_budget_no_double_count() {
        let mut policy = enabled_policy();
        policy.max_attestation_shard_txs_per_block = 1;
        let mut pool = pool_with_policy(policy);
        let latest_ready_epoch = 10u64;

        // Two rewardable shards; cap = 1.
        add_shard(&mut pool, shard_mtx(10, 1));
        add_shard(&mut pool, shard_mtx(10, 2));

        let mut selector = pool.build_selector(Some(latest_ready_epoch));
        let selected = selector.select_transactions();
        let shards = selected.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD).count();
        assert_eq!(shards, 1, "with a per-block cap of 1, priority+inner together must not exceed 1 shard (H-2)");
    }

    /// kaspa-pq audit v26 (M-1): a future-within-grace shard (admission allows `epoch` up to
    /// `latest_ready_epoch + grace`) must NOT leak into the template via the inner selector lane,
    /// even when the per-block shard budget is not exhausted. With a current-epoch shard and a
    /// future-within-grace shard both ready and ample budget, only the current-epoch shard is
    /// selected. (Before the fix the priority lane skipped the future shard but the inner lane
    /// pulled it in.)
    #[test]
    fn future_within_grace_shard_excluded_from_inner_lane() {
        let mut policy = enabled_policy();
        policy.max_attestation_shard_txs_per_block = 16; // ample budget (not exhausted)
        let mut pool = pool_with_policy(policy);
        let latest_ready_epoch = 10u64;

        let current_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 1)); // current epoch
        let future_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch + 1, 2)); // future within grace

        let mut selector = pool.build_selector(Some(latest_ready_epoch));
        let selected = selector.select_transactions();
        let ids: Vec<_> = selected.iter().map(|tx| tx.id()).collect();
        assert!(ids.contains(&current_id), "the current-epoch shard must be selected");
        assert!(!ids.contains(&future_id), "a future-within-grace shard must NOT leak into either lane (M-1)");
        let shards = selected.iter().filter(|tx| tx.subnetwork_id == SUBNETWORK_ID_STAKE_ATTESTATION_SHARD).count();
        assert_eq!(shards, 1, "exactly one (current-epoch) shard is selected");
    }

    /// kaspa-pq audit v26 (H-4): a quarantined current-epoch shard is omitted from the priority
    /// lane while held, and becomes re-selectable after the hold lapses (`retain_active`).
    #[test]
    fn quarantined_shard_omitted_then_reselectable_after_retain() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;
        let shard_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 1));

        // Without quarantine it is in the priority lane.
        let before = pool.build_attestation_priority_set(latest_ready_epoch);
        assert!(before.iter().any(|t| t.tx.id() == shard_id), "shard is selectable before quarantine");

        // Quarantine until epoch 12.
        pool.quarantine_attestation_shard(shard_id, 12);
        let held = pool.build_attestation_priority_set(latest_ready_epoch);
        assert!(!held.iter().any(|t| t.tx.id() == shard_id), "quarantined shard must be omitted from the priority lane (H-4)");

        // Still held at epoch 11.
        let held2 = pool.build_attestation_priority_set(11);
        assert!(!held2.iter().any(|t| t.tx.id() == shard_id), "still held at epoch 11 (until 12 exclusive)");

        // Reap at epoch 12 -> hold lapsed -> re-selectable.
        pool.retain_active_attestation_quarantine(12);
        let released = pool.build_attestation_priority_set(12);
        assert!(released.iter().any(|t| t.tx.id() == shard_id), "shard must be re-selectable after the hold lapses (H-4)");
        assert_eq!(pool.attestation_quarantine_len(), 0, "lapsed entry reaped by retain_active");
    }

    /// kaspa-pq audit v26 (H-4): a quarantined shard is also excluded from the INNER selector lane,
    /// so it cannot leak via the non-priority path while held. With a quarantined current-epoch
    /// shard and one un-quarantined current-epoch shard, only the latter is selected.
    #[test]
    fn quarantined_shard_excluded_from_inner_lane() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;
        let quarantined_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 1));
        let free_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 2));

        // Quarantine the first; the second drives the priority lane so build_selector takes the
        // priority+inner path.
        pool.quarantine_attestation_shard(quarantined_id, 12);

        let mut selector = pool.build_selector(Some(latest_ready_epoch));
        let selected = selector.select_transactions();
        let ids: Vec<_> = selected.iter().map(|tx| tx.id()).collect();
        assert!(ids.contains(&free_id), "the un-quarantined shard must be selected");
        assert!(!ids.contains(&quarantined_id), "the quarantined shard must not leak via the inner lane (H-4)");
    }

    /// kaspa-pq audit v26 (H-4 empty-priority fallback): when the ONLY shard is quarantined the
    /// priority set is EMPTY and build_selector takes the fallback path. That path must STILL
    /// exclude the quarantined shard — otherwise it leaks back into the template and re-triggers the
    /// drop→re-select→drop loop quarantine exists to break (the single-validator / DNS-overlay-wedge
    /// scenario). Regression for the fallback bypass; the other H-4 test keeps a second shard so it
    /// never exercises the empty-priority branch.
    #[test]
    fn quarantined_sole_shard_not_leaked_via_empty_priority_fallback() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;
        let shard_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch, 1));
        pool.quarantine_attestation_shard(shard_id, 12);

        // Sole shard quarantined ⇒ empty priority set ⇒ fallback path.
        assert!(
            pool.build_attestation_priority_set(latest_ready_epoch).is_empty(),
            "precondition: priority set is empty (sole shard quarantined)"
        );

        let mut selector = pool.build_selector(Some(latest_ready_epoch));
        let selected = selector.select_transactions();
        assert!(
            !selected.iter().any(|tx| tx.id() == shard_id),
            "a quarantined sole shard must NOT leak via the empty-priority fallback (H-4)"
        );
    }

    /// kaspa-pq audit v26 (M-1 empty-priority fallback): when the ONLY shard is future-within-grace
    /// the priority set is EMPTY; the fallback path must still exclude it so it cannot be mined ahead
    /// of its epoch. Regression for the fallback bypass.
    #[test]
    fn future_sole_shard_not_leaked_via_empty_priority_fallback() {
        let mut pool = pool_with_policy(enabled_policy());
        let latest_ready_epoch = 10u64;
        let future_id = add_shard(&mut pool, shard_mtx(latest_ready_epoch + 1, 1)); // future within grace

        assert!(
            pool.build_attestation_priority_set(latest_ready_epoch).is_empty(),
            "precondition: priority set is empty (sole shard is future)"
        );

        let mut selector = pool.build_selector(Some(latest_ready_epoch));
        let selected = selector.select_transactions();
        assert!(
            !selected.iter().any(|tx| tx.id() == future_id),
            "a future-within-grace sole shard must NOT leak via the empty-priority fallback (M-1)"
        );
    }

    /// kaspa-pq audit v24 (H-6): the descendant-detection mechanism the replacement-safety guard
    /// relies on. A child chained off a shard's output must be reported as an in-pool redeemer, so
    /// `check_attestation_replacement_safe` would reject a replacement that removes that shard
    /// (which would otherwise orphan the child). This pins the mechanism without a full consensus
    /// harness.
    #[test]
    fn replacement_safety_detects_descendant_chain() {
        use crate::mempool::model::pool::Pool;
        use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput, UtxoEntry};
        use kaspa_txscript::pay_to_address_script;

        let mut pool = pool_with_policy(enabled_policy());

        // A "parent" shard tx with one spendable output.
        let spk = pay_to_address_script(&kaspa_addresses::Address::new(
            kaspa_addresses::Prefix::Testnet,
            kaspa_addresses::Version::PubKey,
            &[0u8; 32],
        ));
        let parent_tx = Transaction::new(
            TX_VERSION,
            vec![],
            vec![TransactionOutput::new(5_000, spk.clone())],
            0,
            SUBNETWORK_ID_STAKE_ATTESTATION_SHARD,
            0,
            borsh::to_vec(&StakeAttestationShardPayload {
                version: 1,
                epoch: 10,
                target_hash: hash64(0xbb),
                target_daa_score: 1234,
                validator_set_commitment: hash64(0xcc),
                attestations: vec![StakeAttestation {
                    version: 1,
                    validator_id: hash64(1),
                    bond_outpoint: TransactionOutpoint::new(hash64(0xaa), 0),
                    epoch: 10,
                    target_hash: hash64(0xbb),
                    target_daa_score: 1234,
                    validator_set_commitment: hash64(0xcc),
                    signature: vec![],
                }],
            })
            .unwrap(),
        );
        let mut parent_mtx = MutableTransaction::from_tx(parent_tx);
        parent_mtx.calculated_fee = Some(1_000);
        parent_mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1000, 1000));
        let parent_id = add_shard(&mut pool, parent_mtx);

        // A child tx spending the parent's output-0 (chained in the pool).
        let child_input = TransactionInput::new(TransactionOutpoint::new(parent_id, 0), vec![], 0, 0);
        let child_tx = Transaction::new(
            TX_VERSION,
            vec![child_input],
            vec![TransactionOutput::new(4_000, spk)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
            0,
            vec![],
        );
        let mut child_mtx = MutableTransaction::from_tx(child_tx);
        child_mtx.entries[0] = Some(UtxoEntry::new(
            5_000,
            pay_to_address_script(&kaspa_addresses::Address::new(
                kaspa_addresses::Prefix::Testnet,
                kaspa_addresses::Version::PubKey,
                &[0u8; 32],
            )),
            0,
            false,
        ));
        child_mtx.calculated_fee = Some(1_000);
        child_mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(1000, 1000));
        let child_size = child_mtx.mempool_estimated_bytes();
        let child_id = child_mtx.id();
        pool.add_transaction(child_mtx, 0, Priority::High, child_size).unwrap();

        // The parent shard has an in-pool descendant ⇒ the H-6 guard must see it.
        let redeemers = pool.get_redeemer_ids_in_pool(&parent_id);
        assert!(redeemers.contains(&child_id), "the child must be detected as an in-pool redeemer of the parent shard (H-6)");
    }
}
