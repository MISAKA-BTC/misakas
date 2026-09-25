use std::sync::atomic::Ordering;

use crate::mempool::{
    Mempool,
    errors::{RuleError, RuleResult},
    model::{
        pool::Pool,
        tx::{MempoolTransaction, TransactionPostValidation, TransactionPreValidation, TxRemovalReason},
    },
    tx::{Orphan, Priority, RbfPolicy},
};
use kaspa_consensus_core::{
    api::ConsensusApi,
    constants::UNACCEPTED_DAA_SCORE,
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, UtxoEntry},
};
use kaspa_core::{debug, info, warn};

impl Mempool {
    pub(crate) fn pre_validate_and_populate_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        mut transaction: MutableTransaction,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<TransactionPreValidation> {
        self.validate_transaction_unacceptance(&transaction)?;
        // Populate mass and estimated_size in the beginning, it will be used in multiple places throughout the validation and insertion.
        transaction.calculated_non_contextual_masses = Some(consensus.calculate_transaction_non_contextual_masses(&transaction.tx));
        self.validate_transaction_in_isolation(&transaction)?;
        let feerate_threshold = self.get_replace_by_fee_constraint(&transaction, rbf_policy)?;
        self.populate_mempool_entries(&mut transaction);
        Ok(TransactionPreValidation { transaction, feerate_threshold })
    }

    pub(crate) fn post_validate_and_insert_transaction(
        &mut self,
        consensus: &dyn ConsensusApi,
        validation_result: RuleResult<()>,
        transaction: MutableTransaction,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> RuleResult<TransactionPostValidation> {
        let transaction_id = transaction.id();

        // First check if the transaction was not already added to the mempool.
        // The case may arise since the execution of the manager public functions is no
        // longer atomic and different code paths may lead to inserting the same transaction
        // concurrently.
        if self.transaction_pool.has(&transaction_id) {
            debug!("Transaction {0} is not post validated since already in the mempool", transaction_id);
            return Err(RuleError::RejectDuplicate(transaction_id));
        }

        self.validate_transaction_unacceptance(&transaction)?;

        match validation_result {
            Ok(_) => {}
            Err(RuleError::RejectMissingOutpoint) => {
                if orphan == Orphan::Forbidden {
                    return Err(RuleError::RejectDisallowedOrphan(transaction_id));
                }
                let _ = self.get_replace_by_fee_constraint(&transaction, rbf_policy)?;
                self.orphan_pool.try_add_orphan(consensus.get_virtual_daa_score(), transaction, priority)?;
                return Ok(TransactionPostValidation::default());
            }
            Err(err) => {
                return Err(err);
            }
        }

        // Perform mempool in-context validations prior to possible RBF replacements
        self.validate_transaction_in_context(&transaction)?;

        // kaspa-pq DNS-finality (audit v24 H-1): reject far-future attestation shards at admission
        // so they never linger in the mempool (and never enter the priority lane). No-op for
        // non-shard txs / when the overlay is off.
        self.validate_attestation_future_epoch(consensus, &transaction, priority)?;

        // kaspa-pq DNS-finality: same-key dedup / replacement for attestation shards. Must run
        // before we accept the tx (and before RBF, which is UTXO-based and irrelevant to the
        // inputs-less / fee-funded shard's attestation identity). Returns the id of an older shard
        // to replace, if any; rejects duplicates that don't meet the replacement bump.
        let replaced_attestation = self.resolve_attestation_dedup(&transaction, priority)?;
        self.validate_attestation_pool_capacity(&transaction, priority, replaced_attestation.is_some())?;

        // Check double spends and try to remove them if the RBF policy requires it
        let removed_transaction = self.execute_replace_by_fee(&transaction, rbf_policy)?;

        // The superseded same-key attestation shard (if any) is removed AFTER the replacement is
        // safely added to the pool (see below), so a failed add never leaves the validator with no
        // shard for that key.

        //
        // Note: there exists a case below where `limit_transaction_count` returns an error signaling that
        //       this tx should be rejected due to mempool size limits (rather than evicting others). However,
        //       if this tx happened to be an RBF tx, it might have already caused an eviction in the line
        //       above. We choose to ignore this rare case for now, as it essentially means that even the increased
        //       feerate of the replacement tx is very low relative to the mempool overall.
        //

        // Before adding the transaction, check if there is room in the pool. M1: a possession proof
        // whose row is lapsing at the tip may take a reserved place and lead the lane (asked once, here).
        let transaction_size = transaction.mempool_estimated_bytes();
        let palw_readiness = self.palw_readiness_admission(consensus, &transaction);
        let txs_to_remove = self.transaction_pool.limit_transaction_count_with(&transaction, transaction_size, palw_readiness)?;
        if !txs_to_remove.is_empty() {
            let transaction_pool_len_before = self.transaction_pool.len();
            for x in txs_to_remove.iter() {
                self.remove_transaction(x, true, TxRemovalReason::MakingRoom, format!(" for {}", transaction_id).as_str())?;
                // self.transaction_pool.limit_transaction_count(&transaction) returns the
                // smallest prefix of `ready_transactions` (sorted by ascending fee-rate)
                // that makes enough room for `transaction`, but since each call to `self.remove_transaction`
                // also removes all transactions dependant on `x` we might already have sufficient space, so
                // we constantly check the break condition.
                //
                // Note that self.transaction_pool.len() < self.config.maximum_transaction_count means we have
                // at least one available slot in terms of the count limit
                if self.transaction_pool.len() < self.config.maximum_transaction_count
                    && self.transaction_pool.get_estimated_size() + transaction_size <= self.config.mempool_size_limit
                {
                    break;
                }
            }
            self.counters
                .tx_evicted_counts
                .fetch_add(transaction_pool_len_before.saturating_sub(self.transaction_pool.len()) as u64, Ordering::Relaxed);
        }

        assert!(
            self.transaction_pool.len() < self.config.maximum_transaction_count
                && self.transaction_pool.get_estimated_size() + transaction_size <= self.config.mempool_size_limit,
            "Transactions in mempool: {}, max: {}, mempool bytes size: {}, max: {}",
            self.transaction_pool.len() + 1,
            self.config.maximum_transaction_count,
            self.transaction_pool.get_estimated_size() + transaction_size,
            self.config.mempool_size_limit,
        );

        // Add the transaction to the mempool as a MempoolTransaction and return a clone of the embedded Arc<Transaction>
        let accepted_transaction = self
            .transaction_pool
            .add_transaction_with(transaction, consensus.get_virtual_daa_score(), priority, transaction_size, palw_readiness)?
            .mtx
            .tx
            .clone();

        // Now that the replacement shard is safely in the pool, drop the superseded same-key shard.
        // Tolerate it already being gone (e.g. evicted above as "making room"); never fail the
        // accept because of the cleanup.
        if let Some(old_tx_id) = replaced_attestation
            && self.transaction_pool.has(&old_tx_id)
            && let Err(err) = self.remove_transaction(
                &old_tx_id,
                false,
                TxRemovalReason::AttestationReplaced,
                format!(" by {}", transaction_id).as_str(),
            )
        {
            warn!("Failed to remove superseded attestation shard {old_tx_id}: {err}");
        }
        Ok(TransactionPostValidation { removed: removed_transaction, accepted: Some(accepted_transaction) })
    }

    /// **M1 (the 2026-09-25 model-registry review): what the tip says of a possession proof as it
    /// enters** — the proof, and how urgently its row needs it (`ConsensusApi::palw_readiness_urgency_v1`).
    /// `None` for every other transaction and where `Config::palw_h1_carrier_priority` is off
    /// (every network but testnet-12), so nothing else is decoded or asked.
    pub(crate) fn palw_readiness_admission(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: &MutableTransaction,
    ) -> Option<crate::mempool::model::palw_carriers::PalwReadinessAdmissionV1> {
        if !self.config.palw_h1_carrier_priority {
            return None;
        }
        let carrier = kaspa_consensus_core::palw_readiness_escalation_v1::PalwReadinessCarrierV1::of_tx(&transaction.tx)?;
        let urgency = consensus.palw_readiness_urgency_v1(&[carrier]).first().copied().flatten();
        Some(crate::mempool::model::palw_carriers::PalwReadinessAdmissionV1 { carrier, urgency })
    }

    /// **M1: re-ask the tip about every possession proof the pool holds** — at each new block, when
    /// the DAA moves and rows renew. A proof whose row now escalates may represent it (a head
    /// candidate, and a reserved place while the row is lapsing); one whose row was renewed — by it,
    /// or by a copy another block carried — goes back to being an ordinary transaction. One tip read
    /// for the whole pool, the proofs taken in lane order (feerate, then arrival). A no-op where the
    /// flag is off or the pool holds no proof.
    pub(crate) fn refresh_palw_readiness(&mut self, consensus: &dyn ConsensusApi) {
        if !self.config.palw_h1_carrier_priority {
            return;
        }
        let proofs = self.transaction_pool.palw_readiness_entries();
        if proofs.is_empty() {
            return;
        }
        let carriers: Vec<_> = proofs.iter().map(|(_, carrier)| *carrier).collect();
        let urgencies = consensus.palw_readiness_urgency_v1(&carriers);
        for ((id, _), urgency) in proofs.iter().zip(urgencies.into_iter().chain(std::iter::repeat(None))) {
            self.transaction_pool.set_palw_readiness_urgency(id, urgency);
        }
    }

    /// Validates that the transaction wasn't already accepted into the DAG
    fn validate_transaction_unacceptance(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        // Reject if the transaction is registered as an accepted transaction
        let transaction_id = transaction.id();
        match self.accepted_transactions.has(&transaction_id) {
            true => Err(RuleError::RejectAlreadyAccepted(transaction_id)),
            false => Ok(()),
        }
    }

    fn validate_transaction_in_isolation(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        let transaction_id = transaction.id();
        if self.transaction_pool.has(&transaction_id) {
            return Err(RuleError::RejectDuplicate(transaction_id));
        }

        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_isolation(transaction)?;
        }
        Ok(())
    }

    fn validate_transaction_in_context(&self, transaction: &MutableTransaction) -> RuleResult<()> {
        if !self.config.accept_non_standard {
            self.check_transaction_standard_in_context(transaction)?;
        }
        Ok(())
    }

    /// kaspa-pq DNS-finality: same-key dedup / replacement for attestation shards.
    ///
    /// Returns:
    /// - `Ok(None)` when the tx is not an attestation shard, the overlay is off, or it overlaps no
    ///   existing shard (accept as-is).
    /// - `Ok(Some(old_tx_id))` when the new shard overlaps exactly one existing shard with the SAME
    ///   anchor tuple AND meets the replacement bump (feerate bump OR fee + min-relay-fee) — caller
    ///   removes `old_tx_id` and accepts the new tx.
    /// - `Err(RejectDuplicateAttestation)` when the overlap is a same-anchor duplicate not meeting
    ///   the bump, or (MVP conservative rule) when the new shard overlaps more than one existing
    ///   shard.
    ///
    /// A differing anchor tuple on an overlapping key is potential equivocation — we log a warning
    /// and keep one shard for templating (accept the new one without removing the old; the slashing
    /// path, not the mempool, handles equivocation).
    fn resolve_attestation_dedup(&self, transaction: &MutableTransaction, priority: Priority) -> RuleResult<Option<TransactionId>> {
        use crate::mempool::attestation::extract_attestation_meta;

        let policy = &self.config.attestation_policy;
        if !policy.enabled {
            return Ok(None);
        }

        // Decode the incoming shard. `None` => not a shard tx; `Some(Err)` => malformed payload
        // (let it through here — isolation/standardness rules judge malformed txs, not this policy).
        let new_meta = match extract_attestation_meta(transaction, 0, priority) {
            Some(Ok(meta)) => meta,
            Some(Err(err)) => {
                debug!("Attestation dedup: skipping malformed shard {}: {}", transaction.id(), err);
                return Ok(None);
            }
            None => return Ok(None),
        };

        let index = self.transaction_pool.attestation_index();

        // Find every distinct existing shard tx overlapping any of the new shard's keys.
        let mut overlapping: Vec<TransactionId> = Vec::new();
        for key in new_meta.keys.iter() {
            if let Some(owner) = index.owner_of_key(key)
                && owner != transaction.id()
                && !overlapping.contains(&owner)
            {
                overlapping.push(owner);
            }
        }

        match overlapping.len() {
            0 => Ok(None),
            1 => {
                let old_tx_id = overlapping[0];
                let Some(old_meta) = index.get(&old_tx_id) else {
                    // Index inconsistency (should not happen given by_key ⊆ by_txid): degrade to
                    // "no overlap" and accept the new shard rather than panicking on the RPC path.
                    return Ok(None);
                };

                if old_meta.anchor_tuple() != new_meta.anchor_tuple() {
                    // kaspa-pq audit v24 (H-4): potential equivocation — same (bond, validator,
                    // epoch) key but a DIFFERENT anchor tuple. The previous behavior warned-but-
                    // accepted both, which overwrote `by_key` while leaving the old tx in
                    // `by_txid`/`by_epoch` — unbounded accumulation of conflicting shards. REJECT
                    // the new shard instead: the mempool keeps exactly one shard per key and never
                    // churns; equivocation slashing is a consensus concern, evidenced from mined
                    // blocks, not from mempool retention.
                    warn!(
                        "Rejecting conflicting attestation shard {}: shares a (bond, validator, epoch) key with existing shard {} but declares a different anchor tuple (possible equivocation; slashing handled by consensus from mined blocks)",
                        transaction.id(),
                        old_tx_id
                    );
                    self.counters.attestation_conflict_rejected_counts.fetch_add(1, Ordering::Relaxed);
                    return Err(RuleError::RejectConflictingAttestation(transaction.id()));
                }

                // Same anchor tuple => genuine duplicate. Replace only on a sufficient bump.
                let bump_feerate = old_meta.feerate * (1.0 + policy.replacement_bump_pct as f64 / 100.0);
                let new_feerate = transaction.calculated_feerate().unwrap_or(0.0);
                let new_fee = transaction.calculated_fee.unwrap_or(0);
                let fee_bump_ok = new_fee >= old_meta.fee.saturating_add(self.config.minimum_relay_transaction_fee);

                if new_feerate >= bump_feerate || fee_bump_ok {
                    // kaspa-pq audit v24 (H-6): replacement removes the old tx with
                    // `remove_redeemers = false`, so it must not orphan a child that chained off the
                    // old tx's change, nor remove the new tx's own in-pool parent. Reject the
                    // replacement unless the funding is disjoint (the old tx has no in-pool
                    // descendants AND the new tx does not descend from the old tx). For the common
                    // fee-funded shard (no chained funding) both checks are trivially satisfied.
                    if let Err(err) = self.check_attestation_replacement_safe(&old_tx_id, transaction) {
                        self.counters.attestation_dedup_rejected_counts.fetch_add(1, Ordering::Relaxed);
                        return Err(err);
                    }
                    debug!(
                        "Attestation replacement: shard {} replaces {} (new feerate {:.4} vs bump threshold {:.4}, new fee {} vs old {})",
                        transaction.id(),
                        old_tx_id,
                        new_feerate,
                        bump_feerate,
                        new_fee,
                        old_meta.fee
                    );
                    self.counters.attestation_replaced_counts.fetch_add(1, Ordering::Relaxed);
                    Ok(Some(old_tx_id))
                } else {
                    debug!(
                        "Rejecting duplicate attestation shard {}: does not beat existing {} (feerate {:.4} < {:.4} and fee {} < {} + {})",
                        transaction.id(),
                        old_tx_id,
                        new_feerate,
                        bump_feerate,
                        new_fee,
                        old_meta.fee,
                        self.config.minimum_relay_transaction_fee
                    );
                    self.counters.attestation_dedup_rejected_counts.fetch_add(1, Ordering::Relaxed);
                    Err(RuleError::RejectDuplicateAttestation(transaction.id()))
                }
            }
            // MVP: a shard overlapping more than one existing shard is rejected (we won't try to
            // multi-replace atomically). Rare in practice (one validator => one key per shard).
            _ => {
                debug!(
                    "Rejecting attestation shard {}: overlaps {} existing shards (multi-overlap replacement not supported)",
                    transaction.id(),
                    overlapping.len()
                );
                self.counters.attestation_dedup_rejected_counts.fetch_add(1, Ordering::Relaxed);
                Err(RuleError::RejectDuplicateAttestation(transaction.id()))
            }
        }
    }

    /// kaspa-pq DNS-finality: enforce the attestation-shard pool count cap at admission.
    /// Same-key replacements are allowed at the cap because they do not increase the indexed
    /// shard count once the superseded tx is removed.
    fn validate_attestation_pool_capacity(
        &self,
        transaction: &MutableTransaction,
        priority: Priority,
        is_replacement: bool,
    ) -> RuleResult<()> {
        use crate::mempool::attestation::extract_attestation_meta;

        let policy = &self.config.attestation_policy;
        if !policy.enabled || policy.max_attestation_mempool_txs == 0 || is_replacement {
            return Ok(());
        }

        match extract_attestation_meta(transaction, 0, priority) {
            Some(Ok(_)) if self.transaction_pool.attestation_tx_count() >= policy.max_attestation_mempool_txs => {
                debug!(
                    "Rejecting attestation shard {}: attestation pool count {} reached cap {}",
                    transaction.id(),
                    self.transaction_pool.attestation_tx_count(),
                    policy.max_attestation_mempool_txs
                );
                Err(RuleError::RejectMempoolIsFull)
            }
            _ => Ok(()),
        }
    }

    /// kaspa-pq DNS-finality (audit v24 H-6): guard a same-key attestation-shard replacement against
    /// orphaning the change-chain.
    ///
    /// The replacement path removes the old (superseded) shard with `remove_redeemers = false` so the
    /// rest of the pool is preserved. That is only sound when the old tx has no in-pool descendants
    /// (otherwise they'd reference a now-missing outpoint) AND the new tx does not itself descend from
    /// the old tx (otherwise we'd remove the new tx's own parent). The overwhelmingly common shard is
    /// fee-funded with no in-pool funding chain, so both checks pass trivially. When a chain IS
    /// present we conservatively REJECT the replacement (keeping the old shard intact) rather than
    /// cascade-removing descendants — correct over clever, and reorg-safe.
    fn check_attestation_replacement_safe(&self, old_tx_id: &TransactionId, new_tx: &MutableTransaction) -> RuleResult<()> {
        use crate::mempool::model::pool::Pool;

        // Descendants of the old shard currently in the pool.
        let old_descendants = self.transaction_pool.get_redeemer_ids_in_pool(old_tx_id);
        if !old_descendants.is_empty() {
            debug!(
                "Rejecting attestation replacement of {}: it has {} in-pool descendant(s) that would be orphaned",
                old_tx_id,
                old_descendants.len()
            );
            return Err(RuleError::RejectDuplicateAttestation(new_tx.id()));
        }

        // The new tx must not descend from the old tx (directly or transitively): if it did, removing
        // the old tx would remove the new tx's own funding parent.
        let mut old_closure: std::collections::HashSet<TransactionId> = old_descendants.into_iter().collect();
        old_closure.insert(*old_tx_id);
        if new_tx.has_parent_in_set(&old_closure) {
            debug!(
                "Rejecting attestation replacement: new shard {} descends from the shard {} it would replace",
                new_tx.id(),
                old_tx_id
            );
            return Err(RuleError::RejectDuplicateAttestation(new_tx.id()));
        }

        Ok(())
    }

    /// kaspa-pq DNS-finality (audit v24 H-1): reject a `StakeAttestationShard` whose shard epoch is
    /// far beyond the latest ready attestation epoch.
    ///
    /// A future-epoch shard can never be canonical/rewardable for the current ready epoch (the
    /// priority lane already excludes it — H-1), and if admitted it would otherwise sit in the
    /// mempool consuming a slot until the TTL sweep eventually reaches it. A small grace
    /// (`hard_retention_grace_epochs`) tolerates a node lagging the tip by a few epochs / benign
    /// clock skew so a just-too-early shard is not rejected. No-op for non-shard txs and when the
    /// overlay is off.
    fn validate_attestation_future_epoch(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: &MutableTransaction,
        priority: Priority,
    ) -> RuleResult<()> {
        use crate::mempool::attestation::extract_attestation_meta;

        let policy = &self.config.attestation_policy;
        if !policy.enabled {
            return Ok(());
        }
        let meta = match extract_attestation_meta(transaction, 0, priority) {
            Some(Ok(meta)) => meta,
            // Malformed payloads are judged by isolation/standardness rules, not this policy.
            Some(Err(_)) | None => return Ok(()),
        };
        // Latest ready epoch from the current tip; `None` means no epoch is ready yet, in which case
        // we cannot meaningfully bound "future" — accept (TTL/priority still govern selection).
        let Some(latest_ready_epoch) = kaspa_consensus_core::dns_finality::ready_epoch_from_tip_blue_score(
            consensus.get_sink_blue_score(),
            policy.epoch_len_blue_score,
            policy.attestation_lag_blue_score,
        ) else {
            return Ok(());
        };
        let grace = policy.hard_retention_grace_epochs;
        if meta.shard_epoch > latest_ready_epoch.saturating_add(grace) {
            debug!(
                "Rejecting future attestation shard {}: epoch {} > latest ready epoch {} + grace {}",
                transaction.id(),
                meta.shard_epoch,
                latest_ready_epoch,
                grace
            );
            self.counters.attestation_future_rejected_counts.fetch_add(1, Ordering::Relaxed);
            return Err(RuleError::RejectFutureAttestation(transaction.id(), meta.shard_epoch, latest_ready_epoch));
        }
        Ok(())
    }

    /// Returns a list with all successfully unorphaned transactions after some
    /// transaction has been accepted.
    pub(crate) fn get_unorphaned_transactions_after_accepted_transaction(
        &mut self,
        transaction: &Transaction,
    ) -> Vec<MempoolTransaction> {
        let mut unorphaned_transactions = Vec::new();
        let transaction_id = transaction.id();
        let mut outpoint = TransactionOutpoint::new(transaction_id, 0);
        for (i, output) in transaction.outputs.iter().enumerate() {
            outpoint.index = i as u32;
            let mut orphan_id = None;
            if let Some(orphan) = self.orphan_pool.outpoint_orphan_mut(&outpoint) {
                for (i, input) in orphan.mtx.tx.inputs.iter().enumerate() {
                    if input.previous_outpoint == outpoint {
                        if orphan.mtx.entries[i].is_none() {
                            let entry = UtxoEntry::new(output.value, output.script_public_key.clone(), UNACCEPTED_DAA_SCORE, false);
                            orphan.mtx.entries[i] = Some(entry);
                            if orphan.mtx.is_verifiable() {
                                orphan_id = Some(orphan.id());
                            }
                        }
                        break;
                    }
                }
            } else {
                continue;
            }
            if let Some(orphan_id) = orphan_id {
                match self.unorphan_transaction(&orphan_id) {
                    Ok(unorphaned_tx) => {
                        unorphaned_transactions.push(unorphaned_tx);
                        debug!("Transaction {0} unorphaned", transaction_id);
                    }
                    Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                        debug!("Ignoring already accepted transaction {}", transaction_id);
                    }
                    Err(err) => {
                        // In case of validation error, we log the problem and drop the
                        // erroneous transaction.
                        info!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err.to_string());
                    }
                }
            }
        }

        unorphaned_transactions
    }

    fn unorphan_transaction(&mut self, transaction_id: &TransactionId) -> RuleResult<MempoolTransaction> {
        // Rust rewrite:
        // - Instead of adding the validated transaction to mempool transaction pool,
        //   we return it.
        // - The function is relocated from OrphanPool into Mempool.
        // - The function no longer validates the transaction in mempool (signatures) nor in context.
        //   This job is delegated to a fn called later in the process (Manager::validate_and_insert_unorphaned_transactions).

        // Remove the transaction identified by transaction_id from the orphan pool.
        let mut transactions = self.orphan_pool.remove_orphan(transaction_id, false, TxRemovalReason::Unorphaned, "")?;

        // At this point, `transactions` contains exactly one transaction.
        // The one we just removed from the orphan pool.
        assert_eq!(transactions.len(), 1, "the list returned by remove_orphan is expected to contain exactly one transaction");
        let transaction = transactions.pop().unwrap();
        let rbf_policy = Self::get_orphan_transaction_rbf_policy(transaction.priority);

        self.validate_transaction_unacceptance(&transaction.mtx)?;
        let _ = self.get_replace_by_fee_constraint(&transaction.mtx, rbf_policy)?;
        Ok(transaction)
    }

    /// Returns the RBF policy to apply to an orphan/unorphaned transaction by inferring it from the transaction priority.
    pub(crate) fn get_orphan_transaction_rbf_policy(priority: Priority) -> RbfPolicy {
        // The RBF policy applied to an orphaned transaction is not recorded in the orphan pool
        // but we can infer it from the priority:
        //
        //  - high means a submitted tx via RPC which forbids RBF
        //  - low means a tx arrived via P2P which allows RBF
        //
        // Note that the RPC submit transaction replacement case, implying a mandatory RBF, forbids orphans
        // so is excluded here.
        match priority {
            Priority::High => RbfPolicy::Forbidden,
            Priority::Low => RbfPolicy::Allowed,
        }
    }
}
