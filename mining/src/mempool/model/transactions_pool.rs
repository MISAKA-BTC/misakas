use crate::{
    Policy,
    feerate::{FeerateEstimator, FeerateEstimatorArgs},
    mempool::{
        attestation::{AttestationIndex, AttestationQuarantine, QuarantineReason, extract_attestation_meta},
        config::Config,
        errors::{RuleError, RuleResult},
        model::{
            frontier::{
                feerate_key::FeerateTransactionKey,
                selectors::{PalwCarrierLaneSelector, SequenceSelectorTransaction},
            },
            map::MempoolTransactionCollection,
            palw_carriers::{
                PALW_H1_CARRIER_LANE_MASS_DIVISOR, PALW_H1_CARRIER_LANE_SCAN, PalwCarrierIndexV1, PalwCarrierReserveV1,
                PalwReadinessAdmissionV1,
            },
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

    /// **ADR-0152 v3.1 H-1: the lifecycle carriers in the pool, in lane order** (`palw_carriers`).
    /// Kept consistent with `all_transactions` at the pool's one insertion site and its one removal
    /// site, so a template build walks a sorted index instead of decoding or sorting anything. Empty
    /// and never written unless `Config::palw_h1_carrier_priority` is set.
    palw_carriers: PalwCarrierIndexV1,
}

impl TransactionsPool {
    pub(crate) fn new(config: Arc<Config>) -> Self {
        let target_time_per_block = 1.0 / (config.network_blocks_per_second as f64);
        let palw_carrier_reserve = PalwCarrierReserveV1::of(&config);
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
            palw_carriers: PalwCarrierIndexV1::new(palw_carrier_reserve),
        }
    }

    /// Add a mutable transaction to the pool (the tests' door; the mempool's one insertion site is
    /// [`Self::add_transaction_with`]).
    #[cfg(test)]
    pub(crate) fn add_transaction(
        &mut self,
        transaction: MutableTransaction,
        virtual_daa_score: u64,
        priority: Priority,
        transaction_size: usize,
    ) -> RuleResult<&MempoolTransaction> {
        self.add_transaction_with(transaction, virtual_daa_score, priority, transaction_size, None)
    }

    /// Add a mutable transaction to the pool, with what the tip read said of a possession proof as it
    /// entered (M1; `Mempool::palw_readiness_admission`) — the pool's one insertion site passes it.
    pub(crate) fn add_transaction_with(
        &mut self,
        transaction: MutableTransaction,
        virtual_daa_score: u64,
        priority: Priority,
        transaction_size: usize,
        palw_readiness: Option<PalwReadinessAdmissionV1>,
    ) -> RuleResult<&MempoolTransaction> {
        let transaction = MempoolTransaction::new(transaction, priority, virtual_daa_score);
        let id = transaction.id();
        self.add_mempool_transaction(transaction, transaction_size, palw_readiness)?;
        Ok(self.get(&id).unwrap())
    }

    /// Add a mempool transaction to the pool
    pub(crate) fn add_mempool_transaction(
        &mut self,
        transaction: MempoolTransaction,
        transaction_size: usize,
        palw_readiness: Option<PalwReadinessAdmissionV1>,
    ) -> RuleResult<()> {
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

        // ADR-0152 H-1: index the lifecycle carriers the template's carrier lane takes first. One
        // decode per carrier-subnetwork transaction, here, so no template build decodes anything; the
        // fee and mass are the frontier's own (`FeerateTransactionKey`), so the lane and the frontier
        // weigh a carrier alike.
        if self.config.palw_h1_carrier_priority
            && let Some(object) =
                kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_of_tx_v1(&transaction.mtx.tx)
        {
            let key = FeerateTransactionKey::from(&transaction);
            let lane_key = kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_lane_key_v1(&object);
            self.palw_carriers.insert(id, key.fee, key.mass, transaction_size, lane_key);
        } else if self.config.palw_h1_carrier_priority
            && let Some(admission) = palw_readiness
        {
            // M1: every possession proof is indexed — a carrier while its row escalates, an ordinary
            // transaction otherwise, and promoted or demoted at each new block without a decode.
            let key = FeerateTransactionKey::from(&transaction);
            self.palw_carriers.insert_readiness(id, key.fee, key.mass, transaction_size, admission.carrier, admission.escalated);
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
        // ADR-0152 H-1: the carrier index follows the pool through the same single removal site.
        self.palw_carriers.remove(transaction_id);

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
    /// **ADR-0152 H-1 first:** where the network obliges heartbeats to carry lifecycle objects, the
    /// carriers in [`Self::build_palw_carrier_lane`] are yielded before anything else and the rest
    /// of the template is the composition below, unchanged, within the block mass the lane leaves;
    /// [`PalwCarrierLaneSelector`] drops the lane's own ids from what the rest selects. So the rest
    /// keeps upstream's selectors — the in-place sampler on a large frontier included — and a
    /// carrier costs a template no walk of the frontier (P2-9 review, finding 2); a lane carrier the
    /// rest samples again costs at most its mass in under-fill. With no carrier ready (always, where
    /// the flag is off) this is the composition below, byte for byte.
    ///
    /// When the attestation overlay is enabled and an epoch is ready, the priority attestation
    /// shards are yielded before normal txs, oldest ready epoch first. Optional shard tx/mass
    /// budgets are honored only when configured; the default
    /// overlay policy leaves them unlimited at this selector layer and relies on block mass. When
    /// the overlay is off (or no epoch is ready) this is byte-identical to the upstream path.
    pub(crate) fn build_selector(&self, latest_ready_epoch: Option<u64>) -> Box<dyn TemplateTransactionSelector> {
        let carriers = self.build_palw_carrier_lane();
        if carriers.is_empty() {
            return self.build_selector_within(latest_ready_epoch, self.config.maximum_mass_per_block);
        }
        // The lane is bounded by half the block mass, so the subtraction cannot underflow.
        let lane_mass: u64 = carriers.iter().map(|carrier| carrier.mass).sum();
        let rest = self.build_selector_within(latest_ready_epoch, self.config.maximum_mass_per_block - lane_mass);
        Box::new(PalwCarrierLaneSelector::new(carriers, rest))
    }

    /// **ADR-0152 v3.1 H-1 (P2-9): the lifecycle carriers a template takes before anything else.**
    ///
    /// A carrier pays the minimum relay fee (the panel's funder does), so under a block's worth of
    /// better-paying traffic the feerate-weighted frontier samples it almost never — and during a
    /// licence halt, when heartbeats may be the only blocks, "almost never" is how a conviction
    /// misses the window V-8 gives it. So the READY carriers (no parent still in the pool: a chained
    /// carrier waits for its parent like any transaction, and the panel keeps one in flight) are
    /// taken first, in the index's order — feerate descending, then arrival — and only the FIRST of
    /// each lane key (`PalwH1LaneKeyV1`: a second accusation of one claim is a drop the fold charges
    /// its filer for, not a conviction). Each is taken if it still fits the lane
    /// ([`PALW_H1_CARRIER_LANE_MASS_DIVISOR`]); one that does not, and every duplicate, rides the
    /// ordinary lane on its feerate. Every carrier here passed the fold's gate when it entered the
    /// pool, and the template asks the gate again.
    ///
    /// Every template, not only the heartbeat's: the lane is what H-1 asks of a heartbeat, a bonded
    /// block serving it too only lands a conviction sooner, and one composition keeps the template
    /// cache honest for every caller. Empty where `Config::palw_h1_carrier_priority` is off. Walks at
    /// most [`PALW_H1_CARRIER_LANE_SCAN`] carriers, in order, and stops once the lane is full.
    pub(crate) fn build_palw_carrier_lane(&self) -> Vec<SequenceSelectorTransaction> {
        if !self.config.palw_h1_carrier_priority || self.palw_carriers.is_empty() {
            return Vec::new();
        }
        let budget = self.config.maximum_mass_per_block / PALW_H1_CARRIER_LANE_MASS_DIVISOR;
        let mut used = 0u64;
        let mut keys_taken = HashSet::new();
        let mut lane = Vec::new();
        // **M1: the lane's head is the first escalating possession proof that is ready and fits —
        // one a template.** Behind a DA storm's carriers a proof of tens of KB would never fit what
        // they leave of the lane, and on the fee market's half better-paying traffic takes it; one at
        // the head a template keeps a seat's rows fresh while the storm keeps the rest of the lane.
        let head =
            self.palw_carriers.escalated_readiness_in_lane_order().take(PALW_H1_CARRIER_LANE_SCAN).find_map(|(id, mass, key)| {
                let ready = self.parent_transactions.get(&id).is_some_and(|parents| parents.is_empty());
                let transaction = self.all_transactions.get(&id).filter(|_| ready && mass <= budget)?;
                Some((id, mass, key.cloned(), transaction))
            });
        let head_id = head.as_ref().map(|(id, ..)| *id);
        if let Some((_, mass, key, transaction)) = head {
            used += mass;
            keys_taken.extend(key);
            lane.push(SequenceSelectorTransaction::new(transaction.mtx.tx.clone(), mass));
        }
        for (id, mass, lane_key) in self.palw_carriers.in_lane_order().take(PALW_H1_CARRIER_LANE_SCAN) {
            if used == budget {
                break;
            }
            if head_id == Some(id) {
                continue;
            }
            let ready = self.parent_transactions.get(&id).is_some_and(|parents| parents.is_empty());
            let Some(transaction) = self.all_transactions.get(&id).filter(|_| ready) else { continue };
            if lane_key.is_some_and(|key| keys_taken.contains(key)) || used.saturating_add(mass) > budget {
                continue;
            }
            used += mass;
            if let Some(key) = lane_key {
                keys_taken.insert(key.clone());
            }
            lane.push(SequenceSelectorTransaction::new(transaction.mtx.tx.clone(), mass));
        }
        lane
    }

    /// **M1: the possession proofs the pool holds** — what the mempool asks the tip about at a new
    /// block ([`Self::set_palw_readiness_escalated`]). Empty where the flag is off.
    pub(crate) fn palw_readiness_entries(
        &self,
    ) -> Vec<(TransactionId, kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1)> {
        self.palw_carriers.readiness_entries()
    }

    /// **M1: a proof's row starts or stops escalating at the tip** — it takes or gives up its reserved
    /// place and its lane slot (`PalwCarrierIndexV1::set_readiness_escalated`).
    pub(crate) fn set_palw_readiness_escalated(&mut self, id: &TransactionId, escalated: bool) {
        self.palw_carriers.set_readiness_escalated(id, escalated);
    }

    /// The pre-H-1 composition within `block_mass` (what the carrier lane left; the whole block when
    /// it is empty, and then exactly the selector this pool built before the lane existed).
    fn build_selector_within(&self, latest_ready_epoch: Option<u64>, block_mass: u64) -> Box<dyn TemplateTransactionSelector> {
        let policy = &self.config.attestation_policy;
        let base_policy = Policy::new(block_mass)
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
    ///
    /// **ADR-0152 H-1 (P2-9 review, finding 1): the carrier reserve** ([`PalwCarrierReserveV1`]).
    /// Where `Config::palw_h1_carrier_priority` is set, an incoming carrier the reserve will hold
    /// (`PalwCarrierIndexV1::would_reserve`: room by count and bytes, its lane key not yet held)
    /// takes its room from the cheapest unreserved transactions whatever they pay, and no incoming
    /// transaction takes a reserved carrier's room — nor that of a transaction whose eviction would
    /// take a reserved carrier with it. Everywhere else this is upstream's rule, unchanged.
    ///
    /// **M1:** `palw_readiness` is what the tip read said of a possession proof as it entered: an
    /// escalating proof the reserve will hold takes its room exactly as an H-1 carrier does; any other
    /// proof is an ordinary transaction.
    pub(crate) fn limit_transaction_count_with(
        &self,
        transaction: &MutableTransaction,
        transaction_size: usize,
        palw_readiness: Option<PalwReadinessAdmissionV1>,
    ) -> RuleResult<Vec<TransactionId>> {
        // No eviction needed -- return
        if self.len() < self.config.maximum_transaction_count
            && self.estimated_size + transaction_size <= self.config.mempool_size_limit
        {
            return Ok(Default::default());
        }

        // ADR-0152 H-1: does this admission take a reserved place? Decoded only for a full pool, and
        // asked through the index's own rule, so the carrier let evict for the reserve is the one
        // the insertion then reserves (the evictions below never touch a reserved carrier).
        // M1: an escalating possession proof the reserve will hold takes its room the same way.
        let takes_reserve = self.config.palw_h1_carrier_priority
            && (kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_object_of_tx_v1(&transaction.tx).is_some_and(
                |object| {
                    let lane_key = kaspa_consensus_core::palw_heartbeat_carriers_v1::palw_h1_carrier_lane_key_v1(&object);
                    self.palw_carriers.would_reserve(transaction_size, lane_key.as_ref())
                },
            ) || palw_readiness.is_some_and(|admission| {
                admission.escalated && self.palw_carriers.would_reserve(transaction_size, Some(&admission.carrier.lane_key()))
            }));

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
            // ADR-0152 H-1: a reserved carrier is evicted by nothing — not for an ordinary
            // transaction, not for another carrier — nor is anything whose eviction would take one
            // with it. An unreserved carrier is an ordinary transaction here. (The index is empty
            // where the flag is off.)
            if redeemers.iter().any(|id| self.palw_carriers.is_reserved(id)) {
                continue;
            }

            // We are iterating ready txs by ascending feerate so the pending tx has lower feerate than all remaining txs
            // (a carrier taking the reserve evicts the cheapest ordinary transactions whatever they pay).
            if !takes_reserve && tx.feerate() > feerate_threshold {
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

    /// [`Self::limit_transaction_count_with`] for a transaction the tip read said nothing of (the
    /// tests' door).
    #[cfg(test)]
    pub(crate) fn limit_transaction_count(
        &self,
        transaction: &MutableTransaction,
        transaction_size: usize,
    ) -> RuleResult<Vec<TransactionId>> {
        self.limit_transaction_count_with(transaction, transaction_size, None)
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

/// **ADR-0152 v3.1 H-1 (P2-9, T38's miner half at the pool): the carrier lane.**
#[cfg(test)]
mod palw_carrier_lane_tests {
    use super::*;
    use crate::mempool::config::Config;
    use kaspa_consensus_core::{
        constants::TX_VERSION,
        mass::NonContextualMasses,
        palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2},
        palw_offence_v1::PalwOffenceKindV1,
        palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
        subnets::{SUBNETWORK_ID_NATIVE, SUBNETWORK_ID_PALW_LIFECYCLE, SubnetworkId},
        tx::{ScriptPublicKey, Transaction, TransactionInput, TransactionOutput, UtxoEntry},
    };
    use kaspa_hashes::Hash64;
    use std::collections::HashMap;

    const BLOCK_MASS: u64 = 10_000;

    fn pool(carrier_priority: bool) -> TransactionsPool {
        let mut config = Config::build_default(1000, false, BLOCK_MASS);
        config.palw_h1_carrier_priority = carrier_priority;
        TransactionsPool::new(Arc::new(config))
    }

    fn mtx(subnetwork: SubnetworkId, payload: Vec<u8>, salt: u64, mass: u64, fee: u64) -> MutableTransaction {
        let spk = ScriptPublicKey::from_vec(0, vec![0x51]);
        let tx = Transaction::new(TX_VERSION, vec![], vec![TransactionOutput::new(salt, spk)], 0, subnetwork, 0, payload);
        let mut mtx = MutableTransaction::from_tx(tx);
        mtx.calculated_fee = Some(fee);
        mtx.calculated_non_contextual_masses = Some(NonContextualMasses::new(mass, mass));
        mtx
    }

    fn payload(object: PalwConsensusObjectV2) -> Vec<u8> {
        borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2, object }).unwrap()
    }

    fn accusation(claim: u64) -> PalwConsensusObjectV2 {
        PalwConsensusObjectV2::DefaultAccused {
            claim: Hash64::from_u64_word(claim),
            missing_event_index: 0,
            accuser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB0), 0)),
            signature: vec![1; 8],
        }
    }

    fn carrier(claim: u64, mass: u64, fee: u64) -> MutableTransaction {
        mtx(SUBNETWORK_ID_PALW_LIFECYCLE, payload(accusation(claim)), claim, mass, fee)
    }

    fn add(pool: &mut TransactionsPool, mtx: MutableTransaction) -> TransactionId {
        let (size, id) = (mtx.mempool_estimated_bytes(), mtx.id());
        pool.add_transaction(mtx, 0, Priority::Low, size).unwrap();
        id
    }

    /// Fifty native transactions paying 100 sompi a gram, 1,000 mass each: five blocks' worth.
    fn better_paying_traffic(pool: &mut TransactionsPool) -> HashMap<TransactionId, u64> {
        (0..50u64).map(|i| (add(pool, mtx(SUBNETWORK_ID_NATIVE, vec![], 1_000 + i, 1_000, 100_000)), 1_000)).collect()
    }

    /// **T38: under five blocks of traffic paying a hundred times its feerate, a heartbeat's
    /// template leads with the conviction carrier** — every build, not by luck of the sample — and
    /// the whole selection still fits one block with nothing selected twice. Without the lane the
    /// frontier's feerate-cubed sampling would take the carrier about never.
    #[test]
    fn a_carrier_leads_the_template_under_a_block_of_better_paying_traffic() {
        let mut pool = pool(true);
        let mut masses = better_paying_traffic(&mut pool);
        let accused = add(&mut pool, carrier(1, 2_000, 2_000));
        masses.insert(accused, 2_000);
        for _ in 0..20 {
            let selected = pool.build_selector(None).select_transactions();
            assert_eq!(selected.first().map(|tx| tx.id()), Some(accused), "the carrier leads the template");
            let ids: HashSet<_> = selected.iter().map(|tx| tx.id()).collect();
            assert_eq!(ids.len(), selected.len(), "nothing is selected twice");
            let mass: u64 = selected.iter().map(|tx| masses[&tx.id()]).sum();
            assert!(mass <= BLOCK_MASS, "the lanes together fit one block: {mass}");
            assert!(selected.len() > 1, "the rest of the block is still the fee market's");
        }
    }

    /// **Off the flag nothing changes**: no carrier is indexed and no lane is built, so every
    /// network but testnet-12 selects exactly as before.
    #[test]
    fn without_the_flag_there_is_no_lane() {
        let mut pool = pool(false);
        better_paying_traffic(&mut pool);
        add(&mut pool, carrier(1, 2_000, 2_000));
        assert!(pool.palw_carriers.is_empty(), "nothing is indexed");
        assert!(pool.build_palw_carrier_lane().is_empty(), "no lane");
    }

    /// **The lane is bounded to half a block, and orders by what the filer pays, then by arrival**
    /// (P2-9 review, finding 5). Higher feerate first; at an equal feerate the EARLIER carrier first,
    /// whatever it weighs — lighter-first let light junk out-rank a heavy honest ML-DSA-87 filing —
    /// and a carrier that no longer fits is left to the fee market.
    #[test]
    fn the_lane_takes_half_a_block_best_payer_first_earlier_first_on_a_tie() {
        let mut pool = pool(true);
        let heavy_first = add(&mut pool, carrier(1, 3_000, 3_000)); // feerate 1, filed first
        let light_later = add(&mut pool, carrier(2, 1_000, 1_000)); // feerate 1, lighter, later
        let rich = add(&mut pool, carrier(3, 1_500, 15_000)); // feerate 10
        let too_heavy = add(&mut pool, carrier(4, 6_000, 600_000)); // feerate 100, over half a block
        let lane: Vec<_> = pool.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect();
        // Budget 5,000: rich (1,500) + the earlier tie (3,000) fit; the later one would reach 5,500.
        assert_eq!(lane, vec![rich, heavy_first], "best payer first, then the earlier of the tie");
        assert!(!lane.contains(&light_later) && !lane.contains(&too_heavy));
        let mass: u64 = pool.build_palw_carrier_lane().iter().map(|c| c.mass).sum();
        assert!(mass <= BLOCK_MASS / PALW_H1_CARRIER_LANE_MASS_DIVISOR);
    }

    /// **One carrier per lane key** (review finding 5): a second accusation of the same claim — which
    /// the fold would drop in the block that carries the first — stays in the pool and the frontier
    /// but gets no lane slot; an accusation of another claim does.
    #[test]
    fn the_lane_takes_the_first_carrier_of_each_key() {
        let mut pool = pool(true);
        let first = add(&mut pool, carrier(1, 1_000, 1_000));
        let copy = mtx(
            SUBNETWORK_ID_PALW_LIFECYCLE,
            payload(PalwConsensusObjectV2::DefaultAccused {
                claim: Hash64::from_u64_word(1),
                missing_event_index: 3,
                accuser: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB9), 0)),
                signature: vec![2; 8],
            }),
            101,
            1_000,
            50_000,
        );
        let copy = add(&mut pool, copy); // pays fifty times more, and is still the same session
        let other = add(&mut pool, carrier(2, 1_000, 1_000));
        let lane: Vec<_> = pool.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect();
        assert_eq!(lane, vec![copy, other], "the best-paying accusation of claim 1 takes its key; claim 2 has its own");
        assert!(!lane.contains(&first) && pool.palw_carriers.contains(&first), "the other stays in the pool, unlaned");
    }

    /// Fills a pool to BOTH its limits with `n` ordinary transactions paying 100 sompi a gram.
    fn full_pool(carrier_priority: bool, n: u64) -> TransactionsPool {
        let traffic: Vec<_> = (0..n).map(|i| mtx(SUBNETWORK_ID_NATIVE, vec![], 1_000 + i, 1_000, 100_000)).collect();
        let size = traffic[0].mempool_estimated_bytes();
        let mut config = Config::build_default(1000, false, BLOCK_MASS);
        config.palw_h1_carrier_priority = carrier_priority;
        config.maximum_transaction_count = n as usize;
        config.mempool_size_limit = n as usize * size;
        let mut pool = TransactionsPool::new(Arc::new(config));
        for tx in traffic {
            add(&mut pool, tx);
        }
        assert_eq!((pool.len(), pool.get_estimated_size()), (n as usize, n as usize * size), "full on both limits");
        pool
    }

    /// Admits `mtx` the way the mempool does: the evictions `limit_transaction_count` names, then
    /// the insertion.
    fn admit(pool: &mut TransactionsPool, mtx: MutableTransaction) -> RuleResult<(TransactionId, Vec<TransactionId>)> {
        let size = mtx.mempool_estimated_bytes();
        let evicted = pool.limit_transaction_count(&mtx, size)?;
        for id in &evicted {
            pool.remove_transaction(id).unwrap();
        }
        Ok((add(pool, mtx), evicted))
    }

    /// **T38 under a FULL mempool (P2-9 review, finding 1): a min-fee carrier is admitted into a pool
    /// full on both limits of transactions paying a hundred times more, leads the template, and is
    /// not the one evicted when better-paying traffic keeps coming.** Without the flag the same pool
    /// refuses it, as upstream always did; and the reserve is bounded — past it a carrier competes on
    /// feerate like anything else.
    #[test]
    fn a_full_pool_keeps_room_for_a_carrier_and_the_carrier_leads() {
        const N: u64 = 40; // a reserve of N / 8 = 5 carriers, by count
        let mut refusing = full_pool(false, N);
        assert!(matches!(admit(&mut refusing, carrier(1, 2_000, 2_000)), Err(RuleError::RejectMempoolIsFull)), "upstream refuses it");

        let mut pool = full_pool(true, N);
        let (accused, evicted) = admit(&mut pool, carrier(1, 2_000, 2_000)).expect("the reserve makes room");
        assert!(!evicted.is_empty(), "room was taken from the fee market, whatever it paid");
        let selected = pool.build_selector(None).select_transactions();
        assert_eq!(selected.first().map(|tx| tx.id()), Some(accused), "and the carrier leads the template");

        // Better-paying traffic keeps arriving: it evicts ordinary transactions, never the carrier.
        for i in 0..(2 * N) {
            let rich = mtx(SUBNETWORK_ID_NATIVE, vec![], 50_000 + i, 1_000, 1_000_000);
            let (_, evicted) = admit(&mut pool, rich).expect("a richer transaction still gets in");
            assert!(!evicted.contains(&accused), "the carrier's room is not the fee market's");
        }
        assert!(pool.palw_carriers.contains(&accused));

        // Carriers fill the reserve; the next one competes with the fee market on feerate, and loses
        // — it may not take a reserved carrier's room either.
        let mut claim = 2;
        while pool.palw_carriers.would_reserve(carrier(claim, 2_000, 2_000).mempool_estimated_bytes(), Some(&session(claim))) {
            let (id, _) = admit(&mut pool, carrier(claim, 2_000, 2_000)).expect("inside the reserve a carrier is admitted");
            assert!(pool.palw_carriers.is_reserved(&id));
            claim += 1;
        }
        let held = pool.palw_carriers.len();
        assert!(held >= 2 && pool.palw_carriers.reserved().0 == held, "the reserve holds every carrier so far: {held}");
        assert!(
            matches!(admit(&mut pool, carrier(claim, 2_000, 2_000)), Err(RuleError::RejectMempoolIsFull)),
            "past the reserve a carrier is an ordinary transaction"
        );
        assert_eq!(pool.palw_carriers.len(), held, "and no reserved carrier made room for it");
    }

    fn session(claim: u64) -> kaspa_consensus_core::palw_heartbeat_carriers_v1::PalwH1LaneKeyV1 {
        kaspa_consensus_core::palw_heartbeat_carriers_v1::PalwH1LaneKeyV1::DaSession(Hash64::from_u64_word(claim))
    }

    /// **The reserve is not first-come for one party** (the reserve's own flood): the H-1 gate judges
    /// each carrier alone against the tip, so one bond's reporter can file many commitments that
    /// each pass it. Keyed, the bond holds ONE reserved place — its other commitments are ordinary
    /// min-fee transactions a full pool refuses — and the conviction filed after the flood still
    /// takes a place of its own and leads the template.
    #[test]
    fn one_reporters_flood_holds_one_reserved_place() {
        let mut pool = full_pool(true, 40); // a reserve of 5 places
        let commitment = |n: u64| {
            mtx(
                SUBNETWORK_ID_PALW_LIFECYCLE,
                payload(PalwConsensusObjectV2::ReporterCommitted {
                    commitment: Hash64::from_u64_word(n),
                    reporter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xBAD), 0)),
                    signature: vec![1; 8],
                }),
                5_000 + n,
                2_000,
                2_000,
            )
        };
        let (first, _) = admit(&mut pool, commitment(1)).expect("the bond's first commitment takes the bond's place");
        for n in 2..=20 {
            assert!(
                matches!(admit(&mut pool, commitment(n)), Err(RuleError::RejectMempoolIsFull)),
                "commitment {n}: the bond already holds its place, so it competes on feerate and loses"
            );
        }
        assert_eq!(pool.palw_carriers.reserved().0, 1);
        let (accused, _) = admit(&mut pool, carrier(1, 2_000, 2_000)).expect("the conviction still finds a place");
        assert!(pool.palw_carriers.is_reserved(&first) && pool.palw_carriers.is_reserved(&accused));
        let lane: Vec<_> = pool.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect();
        assert_eq!(lane, vec![first, accused], "both lead the template, in arrival order");
    }

    /// Only H-1's kinds ride the lane: a licence, a quorum of `Unavailable` or an unfileable offence
    /// kind competes for fees like any transaction, and an undecodable 0x4b payload is nothing.
    #[test]
    fn only_h1_kinds_ride_the_lane() {
        let mut pool = pool(true);
        let licence = PalwConsensusObjectV2::ReceiptLicensed { claim: Hash64::from_u64_word(5), receipts: vec![] };
        let fold_only = PalwConsensusObjectV2::ObjectiveOffence {
            kind: PalwOffenceKindV1::DaDefault,
            accused: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB1), 0)),
            evidence_id: Hash64::from_u64_word(6),
            evidence: vec![1],
        };
        add(&mut pool, mtx(SUBNETWORK_ID_PALW_LIFECYCLE, payload(licence), 10, 1_000, 1_000));
        add(&mut pool, mtx(SUBNETWORK_ID_PALW_LIFECYCLE, payload(fold_only), 11, 1_000, 1_000));
        add(&mut pool, mtx(SUBNETWORK_ID_PALW_LIFECYCLE, vec![0xFF; 40], 12, 1_000, 1_000));
        assert!(pool.build_palw_carrier_lane().is_empty());
        let reveal = PalwConsensusObjectV2::ReporterRevealed {
            offence_key: Hash64::from_u64_word(7),
            reporter: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(0xB2), 0)),
            salt: [9; 32],
        };
        let revealed = add(&mut pool, mtx(SUBNETWORK_ID_PALW_LIFECYCLE, payload(reveal), 13, 1_000, 1_000));
        assert_eq!(pool.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect::<Vec<_>>(), vec![revealed]);
    }

    /// A carrier whose parent is still in the pool waits for it like any transaction (it cannot be
    /// mined first), joins the lane once the parent leaves, and leaves the lane with the pool.
    #[test]
    fn the_lane_follows_readiness_and_removal() {
        let mut pool = pool(true);
        let parent = add(&mut pool, mtx(SUBNETWORK_ID_NATIVE, vec![], 1, 1_000, 1_000));
        let spk = ScriptPublicKey::from_vec(0, vec![0x51]);
        let child_tx = Transaction::new(
            TX_VERSION,
            vec![TransactionInput::new(TransactionOutpoint::new(parent, 0), vec![], 0, 0)],
            vec![TransactionOutput::new(1, spk.clone())],
            0,
            SUBNETWORK_ID_PALW_LIFECYCLE,
            0,
            payload(accusation(8)),
        );
        let mut child = MutableTransaction::from_tx(child_tx);
        child.entries[0] = Some(UtxoEntry::new(1, spk, 0, false));
        child.calculated_fee = Some(1_000);
        child.calculated_non_contextual_masses = Some(NonContextualMasses::new(1_000, 1_000));
        let chained = add(&mut pool, child);
        assert!(pool.build_palw_carrier_lane().is_empty(), "a chained carrier waits for its parent");
        pool.remove_transaction(&parent).unwrap();
        assert_eq!(pool.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect::<Vec<_>>(), vec![chained]);
        pool.remove_transaction(&chained).unwrap();
        assert!(pool.palw_carriers.is_empty() && pool.build_palw_carrier_lane().is_empty(), "removal clears the index");
    }

    // ---- M1 (the 2026-09-25 model-registry review): a possession proof whose row is about to lapse ----

    /// A seat's possession proof for `(bond, class)`: `mass` and `fee` as the frontier weighs them.
    fn proof(bond: u64, class: u64, salt: u64, mass: u64, fee: u64) -> MutableTransaction {
        let object = PalwConsensusObjectV2::SeatReadinessProvedV2 {
            bond: PalwBondKeyV2(TransactionOutpoint::new(Hash64::from_u64_word(bond), 0)),
            class_id: Hash64::from_u64_word(class),
            span: salt,
            proof: Box::new(kaspa_consensus_core::palw_artifact::PalwArtifactMultiproofV1 {
                leaf_count: 1,
                opened: vec![(
                    0,
                    kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1 {
                        tensor_name: String::new(),
                        layer: None,
                        row_start: 0,
                        bytes: vec![1],
                    },
                )],
                siblings: vec![],
            }),
            signature: vec![1; 8],
        };
        mtx(SUBNETWORK_ID_PALW_LIFECYCLE, payload(object), 90_000 + salt, mass, fee)
    }

    /// What the tip read says of `mtx` as it enters: the mempool asks it once
    /// (`Mempool::palw_readiness_admission`); here the test says it.
    fn tip_says(mtx: &MutableTransaction, escalated: bool) -> Option<PalwReadinessAdmissionV1> {
        kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1::of_tx(&mtx.tx)
            .map(|carrier| PalwReadinessAdmissionV1 { carrier, escalated })
    }

    /// [`admit`], with the tip read's verdict on a possession proof.
    fn admit_proof(
        pool: &mut TransactionsPool,
        mtx: MutableTransaction,
        escalated: bool,
    ) -> RuleResult<(TransactionId, Vec<TransactionId>)> {
        let (size, id, verdict) = (mtx.mempool_estimated_bytes(), mtx.id(), tip_says(&mtx, escalated));
        let evicted = pool.limit_transaction_count_with(&mtx, size, verdict)?;
        for evicted in &evicted {
            pool.remove_transaction(evicted).unwrap();
        }
        pool.add_transaction_with(mtx, 0, Priority::Low, size, verdict).unwrap();
        Ok((id, evicted))
    }

    /// **M1 under a full mempool: a min-fee proof whose row is about to lapse is admitted into a pool
    /// full of transactions paying a hundred times more, leads the template, and is not evicted by
    /// the better-paying traffic that keeps coming.** A proof whose row is fresh gets no privilege —
    /// the full pool refuses it as upstream always did — and without the flag (every network but
    /// testnet-12) neither is even indexed.
    #[test]
    fn a_full_pool_admits_and_templates_a_proof_whose_row_is_about_to_lapse() {
        const N: u64 = 40;
        let mut full = full_pool(true, N);
        assert!(
            matches!(admit_proof(&mut full, proof(1, 7, 1, 4_000, 4_000), false), Err(RuleError::RejectMempoolIsFull)),
            "a fresh row's proof is an ordinary transaction: a full pool refuses it"
        );
        let (renewal, evicted) = admit_proof(&mut full, proof(1, 7, 2, 4_000, 4_000), true).expect("the reserve makes room");
        assert!(!evicted.is_empty() && full.palw_carriers.is_reserved(&renewal), "room taken from the fee market, and held");
        for _ in 0..10 {
            let selected = full.build_selector(None).select_transactions();
            assert_eq!(selected.first().map(|tx| tx.id()), Some(renewal), "the proof leads the template, every build");
        }
        for i in 0..(2 * N) {
            let rich = mtx(SUBNETWORK_ID_NATIVE, vec![], 70_000 + i, 1_000, 1_000_000);
            let (_, evicted) = admit(&mut full, rich).expect("a richer transaction still gets in");
            assert!(!evicted.contains(&renewal), "the proof's room is not the fee market's");
        }
        // One place per row: a copy of the same (bond, class) proof is an ordinary transaction.
        assert!(
            matches!(admit_proof(&mut full, proof(1, 7, 3, 4_000, 4_000), true), Err(RuleError::RejectMempoolIsFull)),
            "the row already holds its place"
        );
        // The twin: without the flag the escalated proof is refused and nothing is indexed.
        let mut refusing = full_pool(false, N);
        assert!(matches!(admit_proof(&mut refusing, proof(1, 7, 2, 4_000, 4_000), true), Err(RuleError::RejectMempoolIsFull)));
        let mut idle = pool(false);
        admit_proof(&mut idle, proof(1, 7, 2, 4_000, 4_000), true).unwrap();
        assert!(idle.palw_carriers.is_empty() && idle.build_palw_carrier_lane().is_empty(), "no index, no lane off the flag");
    }

    /// **M1 under a DA storm: the lane's head is the proof, and the storm keeps the rest of the
    /// lane.** Ten accusations of ten claims paying five times the proof's feerate, filed first, fill
    /// the half-block lane on their own and would keep a 4,000-mass proof out of it for ever (it never
    /// fits what they leave, and the fee market's half is better-paying traffic); escalated, the
    /// proof leads every template, one head a template, and the storm still rides behind it. Not
    /// escalated, it is not in the lane at all.
    #[test]
    fn a_da_storm_does_not_keep_a_lapsing_proof_out_of_the_template() {
        let storm = |pool: &mut TransactionsPool| -> Vec<TransactionId> {
            better_paying_traffic(pool);
            (100..110).map(|claim| add(pool, carrier(claim, 1_000, 5_000))).collect()
        };
        let mut stormy = pool(true);
        let accusations = storm(&mut stormy);
        let (renewal, _) = admit_proof(&mut stormy, proof(1, 7, 2, 4_000, 4_000), true).unwrap();
        let (second, _) = admit_proof(&mut stormy, proof(2, 7, 2, 4_000, 4_000), true).unwrap();
        let lane: Vec<_> = stormy.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect();
        assert_eq!(lane.first(), Some(&renewal), "the earlier lapsing proof leads");
        assert_eq!(&lane[1..], &accusations[..1], "the storm keeps what the proof leaves of the lane");
        assert!(!lane.contains(&second), "one head a template: the other proof waits its turn behind the storm");
        let selected = stormy.build_selector(None).select_transactions();
        assert_eq!(selected.first().map(|tx| tx.id()), Some(renewal));
        // The head is mined; the next template leads with the other proof.
        stormy.remove_transaction(&renewal).unwrap();
        assert_eq!(stormy.build_palw_carrier_lane().first().map(|c| c.tx.id()), Some(second));

        let mut today = pool(true);
        let accusations = storm(&mut today);
        let (ordinary, _) = admit_proof(&mut today, proof(1, 7, 2, 4_000, 4_000), false).unwrap();
        let lane: Vec<_> = today.build_palw_carrier_lane().iter().map(|c| c.tx.id()).collect();
        assert_eq!(lane, accusations[..5].to_vec(), "a fresh row's proof gets no lane: the storm fills it");
        assert!(!lane.contains(&ordinary) && !today.palw_carriers.is_reserved(&ordinary));
    }

    /// **The tip read moves after admission**: a proof that entered on a fresh row is promoted when
    /// its row nears staleness — a reserved place and the lane's head — and demoted when the row is
    /// renewed (by it or a copy), giving its place to the best carrier that fits.
    #[test]
    fn a_proof_is_promoted_when_its_row_nears_staleness_and_demoted_when_it_is_renewed() {
        let mut roomy = pool(true);
        let mut full = full_pool(true, 40);
        let (early, _) = admit_proof(&mut roomy, proof(1, 7, 1, 4_000, 4_000), false).unwrap();
        assert!(roomy.build_palw_carrier_lane().is_empty() && !roomy.palw_carriers.is_reserved(&early), "an ordinary transaction");
        roomy.set_palw_readiness_escalated(&early, true);
        assert!(roomy.palw_carriers.is_reserved(&early), "its row nears staleness: a place");
        assert_eq!(roomy.build_palw_carrier_lane().first().map(|c| c.tx.id()), Some(early), "and the lane's head");
        roomy.set_palw_readiness_escalated(&early, false);
        assert!(roomy.build_palw_carrier_lane().is_empty() && !roomy.palw_carriers.is_reserved(&early), "renewed: ordinary again");
        assert_eq!(roomy.palw_readiness_entries().len(), 1, "still indexed, so the next block can ask again");

        // In a full pool a demoted proof's place passes on to a carrier that waited for one.
        let (held, _) = admit_proof(&mut full, proof(1, 7, 2, 4_000, 4_000), true).unwrap();
        let mut claim = 1;
        while full.palw_carriers.would_reserve(carrier(claim, 2_000, 2_000).mempool_estimated_bytes(), Some(&session(claim))) {
            admit(&mut full, carrier(claim, 2_000, 2_000)).unwrap();
            claim += 1;
        }
        let waiting = add(&mut full, carrier(claim, 2_000, 2_000_000)); // a rich carrier past the reserve
        assert!(!full.palw_carriers.is_reserved(&waiting));
        full.set_palw_readiness_escalated(&held, false);
        assert!(full.palw_carriers.is_reserved(&waiting) && !full.palw_carriers.is_reserved(&held), "the place passed on");
    }
}
