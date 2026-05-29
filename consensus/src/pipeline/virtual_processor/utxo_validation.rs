use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{
            BadAcceptedIDMerkleRoot, BadCoinbaseTransaction, BadUTXOCommitment, IneligibleAttestationInBlock,
            InvalidTransactionsInUtxoContext, WrongHeaderPruningPoint,
        },
    },
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        daa::DaaStoreReader,
        ghostdag::{CompactGhostdagData, GhostdagData},
        headers::HeaderStoreReader,
        rewarded_epochs::{RewardedEpochKeys, RewardedEpochsStoreReader},
    },
    processes::{
        pruning::PruningPointReply,
        transaction_validator::{
            errors::{TxResult, TxRuleError},
            tx_validation_in_utxo_context::TxValidationFlags,
        },
    },
};
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::{
    BlockHashMap, BlockHashSet, HashMapCustomHasher,
    acceptance_data::{AcceptedTxEntry, MergesetBlockAcceptanceData},
    api::args::TransactionValidationArgs,
    coinbase::*,
    dns_finality::{
        ATTESTATION_MLDSA65_CONTEXT, ActiveBondView, RewardedEpochSet, attestations_from_accepted_txs, stake_attestation_message,
        validator_reward_outputs_from_attestations,
    },
    hashing,
    header::Header,
    muhash::MuHashExtensions,
    tx::{
        MutableTransaction, PopulatedTransaction, Transaction, TransactionId, TransactionOutput, ValidatedTransaction,
        VerifiableTransaction,
    },
    utxo::{
        utxo_diff::UtxoDiff,
        utxo_view::{UtxoView, UtxoViewComposition},
    },
};
use kaspa_core::{info, trace};
use kaspa_muhash::MuHash;
use kaspa_txscript::verify_mldsa65_with_context;
use kaspa_utils::refs::Refs;

use rayon::prelude::*;
use smallvec::{SmallVec, smallvec};
use std::{iter::once, ops::Deref};

pub(crate) mod crescendo {
    use kaspa_core::{info, log::CRESCENDO_KEYWORD};
    use std::sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    };

    #[derive(Clone)]
    pub(crate) struct _CrescendoLogger {
        steps: Arc<AtomicU8>,
    }

    impl _CrescendoLogger {
        pub fn _new() -> Self {
            Self { steps: Arc::new(AtomicU8::new(Self::_ACTIVATE)) }
        }

        const _ACTIVATE: u8 = 0;

        pub fn _report_activation(&self) -> bool {
            if self.steps.compare_exchange(Self::_ACTIVATE, Self::_ACTIVATE + 1, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                info!(target: CRESCENDO_KEYWORD, "[Crescendo] [--------- Crescendo activated for UTXO state processing rules ---------]");
                true
            } else {
                false
            }
        }
    }
}

/// A context for processing the UTXO state of a block with respect to its selected parent.
/// Note this can also be the virtual block.
pub(super) struct UtxoProcessingContext<'a> {
    pub ghostdag_data: Refs<'a, GhostdagData>,
    pub multiset_hash: MuHash,
    pub mergeset_diff: UtxoDiff,
    pub accepted_tx_ids: Vec<TransactionId>,
    pub mergeset_acceptance_data: Vec<MergesetBlockAcceptanceData>,
    pub mergeset_rewards: BlockHashMap<BlockRewardData>,
    pub pruning_sample_from_pov: Option<BlockHash>,
    /// kaspa-pq (ADR-0009 Addendum B §B.3(c)): the `(bond, epoch)` pairs this
    /// block's coinbase rewarded, computed during `verify_expected_utxo_state`
    /// and persisted by `commit_utxo_state` for descendant uniqueness checks.
    pub validator_rewarded_keys: RewardedEpochKeys,
}

impl<'a> UtxoProcessingContext<'a> {
    pub fn new(ghostdag_data: Refs<'a, GhostdagData>, selected_parent_multiset_hash: MuHash) -> Self {
        let mergeset_size = ghostdag_data.mergeset_size();
        Self {
            ghostdag_data,
            multiset_hash: selected_parent_multiset_hash,
            mergeset_diff: UtxoDiff::default(),
            accepted_tx_ids: Vec::with_capacity(1), // We expect at least the selected parent coinbase tx
            mergeset_rewards: BlockHashMap::with_capacity(mergeset_size),
            mergeset_acceptance_data: Vec::with_capacity(mergeset_size),
            pruning_sample_from_pov: Default::default(),
            validator_rewarded_keys: Vec::new(),
        }
    }

    pub fn selected_parent(&self) -> BlockHash {
        self.ghostdag_data.selected_parent
    }
}

impl VirtualStateProcessor {
    /// Calculates UTXO state and transaction acceptance data relative to the selected parent state
    pub(super) fn calculate_utxo_state<V: UtxoView + Sync>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        pov_daa_score: u64,
    ) {
        let selected_parent_transactions = self.block_transactions_store.get(ctx.selected_parent()).unwrap();
        let validated_coinbase = ValidatedTransaction::new_coinbase(&selected_parent_transactions[0]);

        ctx.mergeset_diff.add_transaction(&validated_coinbase, pov_daa_score).unwrap();
        ctx.multiset_hash.add_transaction(&validated_coinbase, pov_daa_score);
        let validated_coinbase_id = validated_coinbase.id();
        ctx.accepted_tx_ids.push(validated_coinbase_id);

        for (i, (merged_block, txs)) in once((ctx.selected_parent(), selected_parent_transactions))
            .chain(
                ctx.ghostdag_data
                    .consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref())
                    .map(|b| (b, self.block_transactions_store.get(b).unwrap())),
            )
            .enumerate()
        {
            // Create a composed UTXO view from the selected parent UTXO view + the mergeset UTXO diff
            let composed_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);

            // The first block in the mergeset is always the selected parent
            let is_selected_parent = i == 0;

            // No need to fully validate selected parent transactions since selected parent txs were already validated
            // as part of selected parent UTXO state verification with the exact same UTXO context.
            let validation_flags = if is_selected_parent { TxValidationFlags::SkipScriptChecks } else { TxValidationFlags::Full };
            let (validated_transactions, inner_multiset) =
                self.validate_transactions_with_muhash_in_parallel(&txs, &composed_view, pov_daa_score, validation_flags);

            ctx.multiset_hash.combine(&inner_multiset);

            let mut block_fee = 0u64;
            for (validated_tx, _) in validated_transactions.iter() {
                ctx.mergeset_diff.add_transaction(validated_tx, pov_daa_score).unwrap();
                ctx.accepted_tx_ids.push(validated_tx.id());
                block_fee += validated_tx.calculated_fee;
            }

            ctx.mergeset_acceptance_data.push(MergesetBlockAcceptanceData {
                block_hash: merged_block,
                // For the selected parent, we prepend the coinbase tx
                accepted_transactions: is_selected_parent
                    .then_some(AcceptedTxEntry { transaction_id: validated_coinbase_id, index_within_block: 0 })
                    .into_iter()
                    .chain(
                        validated_transactions
                            .into_iter()
                            .map(|(tx, tx_idx)| AcceptedTxEntry { transaction_id: tx.id(), index_within_block: tx_idx }),
                    )
                    .collect(),
            });

            let coinbase_data = self.coinbase_manager.deserialize_coinbase_payload(&txs[0].payload).unwrap();
            ctx.mergeset_rewards.insert(
                merged_block,
                BlockRewardData::new(coinbase_data.subsidy, block_fee, coinbase_data.miner_data.script_public_key),
            );
        }
    }

    /// Verify that the current block fully respects its own UTXO view. We define a block as
    /// UTXO valid if all the following conditions hold:
    ///     1. The block header includes the expected `utxo_commitment`.
    ///     2. The block header includes the expected `accepted_id_merkle_root`.
    ///     3. The block header includes the expected `pruning_point`.
    ///     4. The block coinbase transaction rewards the mergeset blocks correctly.
    ///     5. All non-coinbase block transactions are valid against its own UTXO view.
    pub(super) fn verify_expected_utxo_state<V: UtxoView + Sync>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B): the bond set as-of this
        // block's selected parent. Consumed by the Model-B reward-eligibility
        // rule (PR-10.5′-b2b); the coinbase reward fan-out reader lands in b3.
        selected_parent_bond_view: &ActiveBondView,
        header: &Header,
    ) -> BlockProcessResult<()> {
        // Verify header UTXO commitment
        let expected_commitment = ctx.multiset_hash.finalize();
        if expected_commitment != header.utxo_commitment {
            return Err(BadUTXOCommitment(header.hash, header.utxo_commitment, expected_commitment));
        }
        trace!("correct commitment: {}, {}", header.hash, expected_commitment);

        // Verify header accepted_id_merkle_root
        let expected_accepted_id_merkle_root =
            self.calc_accepted_id_merkle_root(ctx.accepted_tx_ids.iter().copied(), ctx.selected_parent());

        if expected_accepted_id_merkle_root != header.accepted_id_merkle_root {
            return Err(BadAcceptedIDMerkleRoot(header.hash, header.accepted_id_merkle_root, expected_accepted_id_merkle_root));
        }

        let txs = self.block_transactions_store.get(header.hash).unwrap();

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): Model-B
        // reward-eligibility block-validity rule, run BEFORE the coinbase
        // check so the fan-out below can assume every included attestation is
        // eligible (its bond resolves to Active with a valid signature).
        // Inert below activation.
        self.check_attestation_reward_eligibility(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.5): the validator reward
        // outputs the coinbase must carry, derived from the block's included
        // attestations resolved against its selected-parent bond view. Empty
        // (no-op) on every current network. The rewarded `(bond, epoch)` keys
        // are stashed for `commit_utxo_state` to persist (§B.3(c)).
        let (validator_reward_outputs, rewarded_keys) =
            self.validator_reward_outputs_for_block(&txs, selected_parent_bond_view, header.daa_score, ctx.selected_parent());
        ctx.validator_rewarded_keys = rewarded_keys;

        // Verify coinbase transaction (incl. the validator reward fan-out).
        self.verify_coinbase_transaction(
            &txs[0],
            header.daa_score,
            &ctx.ghostdag_data,
            &ctx.mergeset_rewards,
            &self.daa_excluded_store.get_mergeset_non_daa(header.hash).unwrap(),
            &validator_reward_outputs,
        )?;

        // Verify the header pruning point
        let reply = self.verify_header_pruning_point(header, ctx.ghostdag_data.to_compact())?;
        ctx.pruning_sample_from_pov = Some(reply.pruning_sample);

        // Verify all transactions are valid in context
        let current_utxo_view = selected_parent_utxo_view.compose(&ctx.mergeset_diff);
        let validated_transactions =
            self.validate_transactions_in_parallel(&txs, &current_utxo_view, header.daa_score, TxValidationFlags::Full);
        if validated_transactions.len() < txs.len() - 1 {
            // Some non-coinbase transactions are invalid
            return Err(InvalidTransactionsInUtxoContext(txs.len() - 1 - validated_transactions.len(), txs.len() - 1));
        }

        Ok(())
    }

    fn verify_header_pruning_point(
        &self,
        header: &Header,
        ghostdag_data: CompactGhostdagData,
    ) -> BlockProcessResult<PruningPointReply> {
        let reply = self.pruning_point_manager.expected_header_pruning_point(ghostdag_data);
        if reply.pruning_point != header.pruning_point {
            return Err(WrongHeaderPruningPoint(reply.pruning_point, header.pruning_point));
        }
        Ok(reply)
    }

    fn verify_coinbase_transaction(
        &self,
        coinbase: &Transaction,
        daa_score: u64,
        ghostdag_data: &GhostdagData,
        mergeset_rewards: &BlockHashMap<BlockRewardData>,
        mergeset_non_daa: &BlockHashSet,
        validator_reward_outputs: &[TransactionOutput],
    ) -> BlockProcessResult<()> {
        // Extract only miner data from the provided coinbase
        let miner_data = self.coinbase_manager.deserialize_coinbase_payload(&coinbase.payload).unwrap().miner_data;
        let expected_coinbase = self
            .coinbase_manager
            .expected_coinbase_transaction(
                daa_score,
                miner_data,
                ghostdag_data,
                mergeset_rewards,
                mergeset_non_daa,
                validator_reward_outputs,
            )
            .unwrap()
            .tx;
        if hashing::tx::hash(coinbase) != hashing::tx::hash(&expected_coinbase) { Err(BadCoinbaseTransaction) } else { Ok(()) }
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.5 / ADR-0013): the
    /// validator reward outputs a block's coinbase must carry. Derived
    /// deterministically from the block's included attestations
    /// (`attestations_from_accepted_txs`, canonical order) resolved against
    /// `bond_view` (the bond set as-of the block's selected parent) and the
    /// network `RewardParams`, with within-block `(bond, epoch)` dedup + the
    /// per-block cap (see [`validator_reward_outputs_from_attestations`]).
    ///
    /// Run identically by the coinbase **construction** (block-template) and
    /// **validation** paths, so they agree byte-for-byte. Returns no outputs
    /// unless the overlay is configured AND `daa_score` has reached
    /// `dns_activation_daa_score` (`u64::MAX` everywhere today) — so the
    /// coinbase is unchanged on every current network. Callers run the §B.4
    /// eligibility rule first, so every attestation here resolves to an
    /// `Active` bond; the `if let Some` is a defensive skip.
    ///
    /// `selected_parent` is the block's selected parent — the chain tip the
    /// `(bond, epoch)` cross-block uniqueness walk starts from (§B.3(c)). The
    /// walk (this block + its selected-chain ancestors within
    /// `reward_uniqueness_window_blocks` DAA) reads the per-block
    /// `rewarded_epochs_store` to build the already-rewarded prefix set; the
    /// matching recency bound drops attestations whose target is older than the
    /// window, so the bounded walk is guaranteed to see any prior reward of the
    /// same pair. The walk reads nothing on every current network (no rows
    /// while the overlay is dormant), so this stays inert.
    pub(super) fn validator_reward_outputs_for_block(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        daa_score: u64,
        selected_parent: BlockHash,
    ) -> (Vec<TransactionOutput>, RewardedEpochKeys) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return (Vec::new(), Vec::new());
        };
        if daa_score < dns_params.dns_activation_daa_score {
            return (Vec::new(), Vec::new());
        }
        let window = dns_params.reward_uniqueness_window_blocks;

        // Resolve eligible, recent attestations (canonical order). Recency
        // (§B.3(c)): an attestation whose target is older than the window earns
        // nothing — keeps the uniqueness walk below bounded.
        let mut attestations = Vec::new();
        for att in attestations_from_accepted_txs(txs) {
            if daa_score.saturating_sub(att.target_daa_score) > window {
                continue;
            }
            if let Some(bond) = bond_view.active_bond_at(&att.bond_outpoint, att.target_daa_score) {
                attestations.push((att.bond_outpoint, att.epoch, bond.owner_reward_spk_payload));
            }
        }

        // Build the already-rewarded prefix set: walk the selected parent and
        // its selected-chain ancestors within `window` DAA, unioning each
        // block's persisted rewarded `(bond, epoch)` keys (§B.3(c)).
        let mut already_rewarded = RewardedEpochSet::new();
        for ancestor in once(selected_parent).chain(self.reachability_service.default_backward_chain_iterator(selected_parent)) {
            let ancestor_daa = self.headers_store.get_daa_score(ancestor).unwrap();
            if daa_score.saturating_sub(ancestor_daa) > window {
                break;
            }
            if let Ok(keys) = self.rewarded_epochs_store.get(ancestor) {
                for (bond_outpoint, epoch) in keys.iter() {
                    already_rewarded.insert(*bond_outpoint, *epoch);
                }
            }
        }

        validator_reward_outputs_from_attestations(
            dns_params.reward_params.per_attestation_reward_sompi,
            dns_params.reward_params.max_validator_inflation_per_block_sompi,
            &attestations,
            &already_rewarded,
        )
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): block-template
    /// pre-filter. Drops any `StakeAttestationShard` tx carrying an attestation
    /// that is not §B.4-eligible (its bond does not resolve to `Active` in
    /// `bond_view` with a valid signature) so that a block mined from the
    /// template passes the eligibility rule rather than self-disqualifying.
    /// Non-shard txs are always retained. Inert unless the overlay is
    /// configured **and** past `dns_activation_daa_score` (`u64::MAX`
    /// everywhere today), so on every current network this is a no-op and the
    /// template is unchanged. Recency is *not* filtered here: a stale-but-
    /// eligible shard is valid (§B.4 ignores recency) and simply earns no
    /// reward, so it may remain.
    pub(super) fn retain_reward_eligible_attestation_shards(
        &self,
        txs: &mut Vec<Transaction>,
        bond_view: &ActiveBondView,
        daa_score: u64,
    ) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        if daa_score < dns_params.dns_activation_daa_score {
            return;
        }
        let net_id = self.genesis.hash;
        // A non-shard tx yields no attestations → `attestation_reward_eligibility`
        // returns Ok, so it is retained. A shard tx is retained iff *all* its
        // attestations are eligible.
        txs.retain(|tx| attestation_reward_eligibility(std::slice::from_ref(tx), bond_view, net_id, true).is_ok());
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): the Model-B
    /// reward-eligibility **block-validity** rule. Rejects a block that
    /// includes a `StakeAttestationShard` whose attestation is not
    /// structurally reward-eligible against this block's own selected-parent
    /// bond view — its bond must resolve to `Active` (at the attestation's
    /// `target_daa_score`) **and** its ML-DSA-65 signature must verify. This
    /// makes "included ⇒ rewardable" a consensus invariant, so the coinbase
    /// reward fan-out (PR-10.5′-b3) needs no skip set. Reward **uniqueness**
    /// (Addendum B §B.3(c)) is a reward-emission concern, not a validity one
    /// (a duplicate `(bond, epoch)` is simply unrewarded), and is not checked
    /// here.
    ///
    /// Inert unless the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (`u64::MAX` on every current network, so
    /// this returns `Ok(())` immediately everywhere today). The canonical
    /// digest + signature verification mirror the StakeScore aggregation pass
    /// (`processor.rs`) byte-for-byte and the validator-service signer.
    fn check_attestation_reward_eligibility(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        // Fold the gate: configured overlay AND past activation.
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        // ADR-0009 Addendum A.3: the network_id discriminator is the genesis hash.
        attestation_reward_eligibility(txs, selected_parent_bond_view, self.genesis.hash, activated)
            .map_err(|(bond_tx, epoch)| IneligibleAttestationInBlock(bond_tx, epoch))
    }

    /// Validates transactions against the provided `utxo_view` and returns a vector with all transactions
    /// which passed the validation along with their original index within the containing block
    pub(crate) fn validate_transactions_in_parallel<'a, V: UtxoView + Sync>(
        &self,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
        flags: TxValidationFlags,
    ) -> Vec<(ValidatedTransaction<'a>, u32)> {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .enumerate()
                .skip(1) // Skip the coinbase tx.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags).ok().map(|vtx| (vtx, i as u32)))
                .collect()
        })
    }

    /// Same as validate_transactions_in_parallel except during the iteration this will also
    /// calculate the muhash in parallel for valid transactions
    pub(crate) fn validate_transactions_with_muhash_in_parallel<'a, V: UtxoView + Sync>(
        &self,
        txs: &'a Vec<Transaction>,
        utxo_view: &V,
        pov_daa_score: u64,
        flags: TxValidationFlags,
    ) -> (SmallVec<[(ValidatedTransaction<'a>, u32); 2]>, MuHash) {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .enumerate()
                .skip(1) // Skip the coinbase tx.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags).ok().map(|vtx| {
                    let mh = MuHash::from_transaction(&vtx, pov_daa_score);
                    (smallvec![(vtx, i as u32)], mh)
                }
                ))
                .reduce(
                    || (smallvec![], MuHash::new()),
                    |mut a, mut b| {
                        a.0.append(&mut b.0);
                        a.1.combine(&b.1);
                        a
                    },
                )
        })
    }

    /// Attempts to populate the transaction with UTXO entries and performs all utxo-related tx validations
    pub(super) fn validate_transaction_in_utxo_context<'a>(
        &self,
        transaction: &'a Transaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
        flags: TxValidationFlags,
    ) -> TxResult<ValidatedTransaction<'a>> {
        let mut entries = Vec::with_capacity(transaction.inputs.len());
        for input in transaction.inputs.iter() {
            if let Some(entry) = utxo_view.get(&input.previous_outpoint) {
                entries.push(entry);
            } else {
                // Missing at least one input. For perf considerations, we report once a single miss is detected and avoid collecting all possible misses.
                return Err(TxRuleError::MissingTxOutpoints);
            }
        }
        let populated_tx = PopulatedTransaction::new(transaction, entries);
        let res = self.transaction_validator.validate_populated_transaction_and_get_fee(&populated_tx, pov_daa_score, flags, None);
        match res {
            Ok(calculated_fee) => Ok(ValidatedTransaction::new(populated_tx, calculated_fee)),
            Err(tx_rule_error) => {
                // TODO (relaxed): aggregate by error types and log through the monitor (in order to not flood the logs)
                info!("Rejecting transaction {} due to transaction rule error: {}", transaction.id(), tx_rule_error);
                Err(tx_rule_error)
            }
        }
    }

    /// Populates the mempool transaction with maximally found UTXO entry data
    pub(crate) fn populate_mempool_transaction_in_utxo_context(
        &self,
        mutable_tx: &mut MutableTransaction,
        utxo_view: &impl UtxoView,
    ) -> TxResult<()> {
        let mut has_missing_outpoints = false;
        for i in 0..mutable_tx.tx.inputs.len() {
            if mutable_tx.entries[i].is_some() {
                // We prefer a previously populated entry if such exists
                continue;
            }
            if let Some(entry) = utxo_view.get(&mutable_tx.tx.inputs[i].previous_outpoint) {
                mutable_tx.entries[i] = Some(entry);
            } else {
                // We attempt to fill as much as possible UTXO entries, hence we do not break in this case but rather continue looping
                has_missing_outpoints = true;
            }
        }
        if has_missing_outpoints {
            return Err(TxRuleError::MissingTxOutpoints);
        }
        Ok(())
    }

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to validation if all found
    pub(super) fn validate_mempool_transaction_in_utxo_context(
        &self,
        mutable_tx: &mut MutableTransaction,
        utxo_view: &impl UtxoView,
        pov_daa_score: u64,
        args: &TransactionValidationArgs,
    ) -> TxResult<()> {
        self.populate_mempool_transaction_in_utxo_context(mutable_tx, utxo_view)?;

        // Calc the contextual storage mass
        let contextual_mass = self
            .transaction_validator
            .mass_calculator
            .calc_contextual_masses(&mutable_tx.as_verifiable())
            .ok_or(TxRuleError::MassIncomputable)?;

        // Set the inner mass field
        mutable_tx.tx.set_mass(contextual_mass.storage_mass);

        // At this point we know all UTXO entries are populated, so we can safely pass the tx as verifiable
        let mass_and_feerate_threshold = args
            .feerate_threshold
            .map(|threshold| (contextual_mass.max(mutable_tx.calculated_non_contextual_masses.unwrap()), threshold));
        let calculated_fee = self.transaction_validator.validate_populated_transaction_and_get_fee(
            &mutable_tx.as_verifiable(),
            pov_daa_score,
            TxValidationFlags::SkipMassCheck, // we can skip the mass check since we just set it
            mass_and_feerate_threshold,
        )?;
        mutable_tx.calculated_fee = Some(calculated_fee);
        Ok(())
    }

    /// Calculates the accepted_id_merkle_root based on the current DAA score and the accepted tx ids
    /// refer KIP-15 for more details
    ///
    /// PR-9.5c: `accepted_tx_ids` widened to `TransactionId`
    /// (= `Hash64`); return type widened to `AcceptedIdMerkleRoot`
    /// (= `Hash64`). The branch combination uses the keyed
    /// BLAKE2b-512 `AcceptedIdMerkleBranchHash64` hasher (same
    /// domain as `merkle::calc_accepted_id_merkle_root_pre_crescendo`)
    /// so the post-Crescendo path and the pre-Crescendo path
    /// produce values from the same hash family.
    pub(super) fn calc_accepted_id_merkle_root(
        &self,
        accepted_tx_ids: impl ExactSizeIterator<Item = kaspa_consensus_core::TransactionId>,
        selected_parent: kaspa_consensus_core::BlockHash,
    ) -> kaspa_consensus_core::AcceptedIdMerkleRoot {
        use kaspa_hashes::{AcceptedIdMerkleBranchHash64, HasherBase};
        let parent_root = self.headers_store.get_header(selected_parent).unwrap().accepted_id_merkle_root;
        let leaves_root = kaspa_consensus_core::merkle::calc_accepted_id_merkle_root_pre_crescendo(accepted_tx_ids.collect());
        let mut hasher = AcceptedIdMerkleBranchHash64::new();
        hasher.update(parent_root.as_byte_slice()).update(leaves_root.as_byte_slice());
        hasher.finalize()
    }
}

/// Pure core of the ADR-0009 Addendum B §B.4 reward-eligibility rule, split
/// out from [`VirtualStateProcessor::check_attestation_reward_eligibility`] so
/// it can be unit-tested without a full processor. `activated` folds the
/// `dns_params.is_some() && daa_score >= dns_activation_daa_score` gate; when
/// `false` the rule is a no-op (every current network). On the first
/// ineligible attestation returns `Err((bond tx id, epoch))`; the caller maps
/// it to [`IneligibleAttestationInBlock`]. An attestation is eligible iff its
/// bond resolves to `Active` in `bond_view` at the attestation's
/// `target_daa_score` **and** its ML-DSA-65 signature verifies over the
/// canonical [`stake_attestation_message`] digest (Addendum A.3 layout).
fn attestation_reward_eligibility(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: BlockHash,
    activated: bool,
) -> Result<(), (TransactionId, u64)> {
    if !activated {
        return Ok(());
    }
    for att in attestations_from_accepted_txs(txs) {
        // (a) bond resolves to Active at the attestation's anchor.
        let Some(bond) = bond_view.active_bond_at(&att.bond_outpoint, att.target_daa_score) else {
            return Err((att.bond_outpoint.transaction_id, att.epoch));
        };
        // (b) ML-DSA-65 signature verifies over the canonical digest.
        let digest = stake_attestation_message(
            net_id.as_byte_slice(),
            att.epoch,
            att.target_hash,
            att.target_daa_score,
            att.validator_set_commitment,
            att.bond_outpoint,
        )
        .as_bytes();
        if !matches!(
            verify_mldsa65_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA65_CONTEXT),
            Ok(true)
        ) {
            return Err((att.bond_outpoint.transaction_id, att.epoch));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use itertools::Itertools;

    use super::*;

    #[test]
    fn test_rayon_reduce_retains_order() {
        // this is an independent test to replicate the behavior of
        // validate_txs_in_parallel and validate_txs_with_muhash_in_parallel
        // and assert that the order of data is retained when doing par_iter
        let data: Vec<u16> = (1..=1000).collect();

        let collected: Vec<u16> = data
            .par_iter()
            .filter_map(|a| {
                let chance: f64 = rand::random();
                if chance < 0.05 {
                    return None;
                }
                Some(*a)
            })
            .collect();

        println!("collected len: {}", collected.len());

        collected.iter().tuple_windows().for_each(|(prev, curr)| {
            // Data was originally sorted, so we check if they remain sorted after filtering
            assert!(prev < curr, "expected {} < {} if original sort was preserved", prev, curr);
        });

        let reduced: SmallVec<[u16; 2]> = data
            .par_iter()
            .filter_map(|a: &u16| {
                let chance: f64 = rand::random();
                if chance < 0.05 {
                    return None;
                }
                Some(smallvec![*a])
            })
            .reduce(
                || smallvec![],
                |mut arr, mut curr_data| {
                    arr.append(&mut curr_data);
                    arr
                },
            );

        println!("reduced len: {}", reduced.len());

        reduced.iter().tuple_windows().for_each(|(prev, curr)| {
            // Data was originally sorted, so we check if they remain sorted after filtering
            assert!(prev < curr, "expected {} < {} if original sort was preserved", prev, curr);
        });
    }

    // kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): the reward-eligibility
    // rule's pure core. Covers the gate + both reject branches (bond absent /
    // signature invalid). The accept-with-valid-signature path requires
    // ML-DSA-65 signing (libcrux) and is covered by the PR-10.5′-b3 end-to-end
    // integration test rather than here.
    mod attestation_reward_eligibility {
        use super::super::attestation_reward_eligibility as eligibility;
        use kaspa_consensus_core::{
            BlockHash,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                StakeAttestation, StakeBondRecord, single_attestation_shard, stake_attestation_shard_tx,
            },
            tx::TransactionOutpoint,
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn attestation(bond_outpoint: TransactionOutpoint) -> StakeAttestation {
            StakeAttestation {
                version: DNS_PAYLOAD_VERSION_V1,
                validator_id: Hash64::from_bytes([0xa1; 64]),
                bond_outpoint,
                epoch: 1,
                target_hash: Hash64::from_bytes([0x55; 64]),
                target_daa_score: 10_000,
                validator_set_commitment: Hash64::from_bytes([0x66; 64]),
                // Garbage signature — never verifies. The accept path is tested
                // end-to-end in b3 (a real validator-signed attestation).
                signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN],
            }
        }

        fn active_bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0, // Active from genesis.
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 32],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        #[test]
        fn noop_when_not_activated() {
            // Even an attestation referencing an unknown bond passes when the
            // gate is closed (every current network: dns_activation = u64::MAX).
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(outpoint(1))));
            assert_eq!(eligibility(&[tx], &ActiveBondView::new(), NET(), false), Ok(()));
        }

        #[test]
        fn rejects_attestation_with_unknown_bond() {
            // Activated + empty bond view ⇒ the bond does not resolve ⇒ reject.
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(outpoint(1))));
            assert_eq!(eligibility(&[tx], &ActiveBondView::new(), NET(), true), Err((Hash64::from_bytes([1; 64]), 1)));
        }

        #[test]
        fn rejects_attestation_with_invalid_signature() {
            // Activated + bond present & Active, but the (garbage) signature
            // fails verification ⇒ reject at branch (b).
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            let tx = stake_attestation_shard_tx(&single_attestation_shard(attestation(op)));
            assert_eq!(eligibility(&[tx], &view, NET(), true), Err((Hash64::from_bytes([2; 64]), 1)));
        }

        #[test]
        fn ok_when_no_attestation_shards() {
            // Activated but no shard txs ⇒ nothing to check ⇒ Ok.
            assert_eq!(eligibility(&[], &ActiveBondView::new(), NET(), true), Ok(()));
        }
    }
}
