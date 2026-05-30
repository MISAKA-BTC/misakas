use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{
            BadAcceptedIDMerkleRoot, BadCoinbaseTransaction, BadUTXOCommitment, IneligibleAttestationInBlock,
            InvalidTransactionsInUtxoContext, NonReleasableBondSpendInBlock, UnverifiableSlashingEvidenceInBlock,
            WrongHeaderPruningPoint,
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
        ATTESTATION_MLDSA65_CONTEXT, ActiveBondView, BondStatus, FeeSplitParams, RewardedEpochSet, SlashingSideEffect,
        attestations_from_accepted_txs, bond_release_daa_score, effective_bond_status, resolve_slashing_side_effects,
        slashing_evidence_from_accepted_txs, split_validator_pool, stake_attestation_message,
        validator_participation_reward_outputs,
    },
    hashing,
    header::Header,
    muhash::MuHashExtensions,
    tx::{
        MutableTransaction, PopulatedTransaction, Transaction, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry,
        ValidatedTransaction, VerifiableTransaction,
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
    ///
    /// kaspa-pq Phase 10/11 (ADR-0016 §D.4): `selected_parent_bond_view` is the
    /// bond set as-of this block's selected parent — the same view the overlay
    /// block-validity rules in `verify_expected_utxo_state` read. After the
    /// mergeset is applied, [`Self::apply_slashing_side_effects`] consumes it to
    /// remove each slashed bond's locked output-0 from `ctx.mergeset_diff` +
    /// `ctx.multiset_hash` (and so the `utxo_commitment`) and mint the reporter
    /// reward at `(slashing_tx_id, 0)`. Both paths into this function (block
    /// validation and virtual recompute) pass the same view + `pov_daa_score`,
    /// so the side-effect is byte-identical across construction and validation.
    /// Gated on `dns_activation_daa_score` (`u64::MAX` on every current network),
    /// so it is a no-op everywhere today.
    pub(super) fn calculate_utxo_state<V: UtxoView + Sync>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        selected_parent_bond_view: &ActiveBondView,
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

        // kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): apply the
        // slashing side-effect over the fully-applied mergeset. Gated/inert on
        // every current network.
        self.apply_slashing_side_effects(ctx, selected_parent_utxo_view, selected_parent_bond_view, pov_daa_score);
    }

    /// kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): the atomic
    /// consensus side-effect of slashing. For each genuine equivocation evidence
    /// accepted in this block's mergeset whose bond still holds a locked output-0
    /// (resolved `Active`/`Unbonding` against the selected-parent bond view),
    /// remove that output-0 UTXO (`S` leaves the supply) and mint the reporter
    /// reward `R` at `(slashing_tx_id, 0)` — the slashing tx declares no outputs
    /// (isolation rule), so index 0 is always free. Net supply change is `R − S`;
    /// the remainder `S − R` is implicitly burned. Both add/remove are mirrored
    /// into `ctx.multiset_hash`, so the `utxo_commitment` reflects the side-effect.
    ///
    /// Resolution runs over the mergeset's *accepted* txs (the same set the
    /// acceptance data records) using the block's selected-parent bond view, so
    /// block validation and virtual recompute — which call
    /// [`Self::calculate_utxo_state`] with identical inputs — produce byte-for-
    /// byte identical side-effects, keeping construction == validation and the
    /// operation reorg-safe.
    ///
    /// Activation gating lives here; the resolved effects are applied by
    /// [`apply_slashing_effects_to_state`], whose per-effect `composed.get`
    /// lookup yields the exact stored UTXO entry (so its `block_daa_score`
    /// matches the multiset element being removed) and doubles as a release-race
    /// guard. Gated on `dns_activation_daa_score` (`u64::MAX` on every current
    /// network), so this returns immediately everywhere today.
    fn apply_slashing_side_effects<V: UtxoView>(
        &self,
        ctx: &mut UtxoProcessingContext,
        selected_parent_utxo_view: &V,
        selected_parent_bond_view: &ActiveBondView,
        pov_daa_score: u64,
    ) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return;
        };
        if pov_daa_score < dns_params.dns_activation_daa_score {
            return;
        }
        let accepted_txs = self.accepted_txs_from_acceptance_data(&ctx.mergeset_acceptance_data);
        let effects = resolve_slashing_side_effects(
            &accepted_txs,
            selected_parent_bond_view,
            pov_daa_score,
            dns_params.reward_params.slashing_reporter_reward_bps,
        );
        apply_slashing_effects_to_state(&effects, selected_parent_utxo_view, &mut ctx.mergeset_diff, &mut ctx.multiset_hash, pov_daa_score);
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

        // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload"): reject a
        // block whose slashing evidence is not genuine, so a forged evidence
        // can never mutate a bond to `Slashed`. Inert below activation.
        self.check_slashing_evidence_genuine(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate. Reject
        // a block whose transactions spend a known bond outpoint whose bond is
        // not releasable, so a bond's staked output-0 is locked while the bond
        // is `Pending`/`Active`/mid-unbonding/`Slashed`. Inert below activation.
        self.check_bond_spend_gate(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq Phase 10/11 + Phase 13 (ADR-0009 Addendum B §B.5 / ADR-0018
        // §F+§E): the validator reward outputs the coinbase must carry. The §F
        // carve (`carve`) splits each source block's reward Worker/Validator/
        // Service; the Validator total (`validator_pool`) funds the §E
        // participation distribution computed by `validator_reward_outputs_for_block`.
        // Both are no-ops on every current network (overlay dormant). The rewarded
        // `(bond, epoch)` keys are stashed for `commit_utxo_state` (§B.3(c)).
        let mergeset_non_daa = self.daa_excluded_store.get_mergeset_non_daa(header.hash).unwrap();
        let carve =
            self.dns_params.as_ref().filter(|p| header.daa_score >= p.dns_activation_daa_score).map(|p| &p.reward_params.fee_split);
        let validator_pool = carve.map_or(0, |fs| {
            self.coinbase_manager.coinbase_validator_pool(&ctx.ghostdag_data, &ctx.mergeset_rewards, &mergeset_non_daa, fs)
        });
        let (validator_reward_outputs, rewarded_keys) = self.validator_reward_outputs_for_block(
            &txs,
            selected_parent_bond_view,
            header.daa_score,
            ctx.selected_parent(),
            validator_pool,
        );
        ctx.validator_rewarded_keys = rewarded_keys;

        // Verify coinbase transaction (incl. the §F carve + §E reward fan-out).
        self.verify_coinbase_transaction(
            &txs[0],
            header.daa_score,
            &ctx.ghostdag_data,
            &ctx.mergeset_rewards,
            &mergeset_non_daa,
            &validator_reward_outputs,
            carve,
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
        // kaspa-pq Phase 13 (ADR-0018 §F): the per-source-block reward carve,
        // threaded to `expected_coinbase_transaction`. `None` on every current
        // network (matches the construction path).
        carve: Option<&FeeSplitParams>,
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
                carve,
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
    /// network `RewardParams`. ADR-0018 §E: each included validator earns a
    /// stake-proportional share of the §F validator pool's participation
    /// sub-pool against the epoch's expected (total active) stake, with
    /// within-block + cross-block `(bond, epoch)` dedup and a whole-output pool
    /// cap (see [`validator_participation_reward_outputs`]).
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
        // kaspa-pq Phase 13 (ADR-0018 §F/§E): the validator-side coinbase pool
        // (`CoinbaseManager::coinbase_validator_pool`) this block's §E
        // participation rewards are distributed from. 0 on every current network
        // (the caller computes it only past `dns_activation_daa_score`).
        validator_pool: u64,
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
                // ADR-0018 §E: carry the bond's stake — the proportional weight in the
                // participation distribution (against the expected-stake denominator).
                attestations.push((att.bond_outpoint, att.epoch, bond.owner_reward_spk_payload, bond.amount));
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

        // ADR-0018 §E: distribute the participation sub-pool (the 70% split of the
        // §F validator pool; the 30% quality-bonus sub-pool is a later slice and is
        // burned for now) proportionally by stake against the epoch's expected
        // (total active) stake — the anti-capture denominator — with the same
        // within-block + cross-block (§B.3(c)) `(bond, epoch)` uniqueness and a
        // whole-output pool cap (Σ ≤ pool; the unspent remainder is not minted).
        let expected_stake = bond_view.total_active_stake_at(daa_score) as u128;
        let (participation_pool, _quality_bonus_pool) =
            split_validator_pool(validator_pool as u128, dns_params.reward_params.validator_participation_bps);
        validator_participation_reward_outputs(participation_pool, expected_stake, &attestations, &already_rewarded)
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

    /// kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload"): the stateful
    /// slashing-evidence genuineness rule. Rejects a block carrying a
    /// `SlashingEvidence` whose referenced bond is unknown in the block's
    /// selected-parent bond view, or one of whose two equivocating attestations
    /// does not ML-DSA-verify against that bond's `validator_pubkey` — so a
    /// forged-but-well-formed evidence (the §A.2 tx-level check is structural
    /// only) cannot mutate a bond to `Slashed`. Inert unless the overlay is
    /// configured **and** past `dns_activation_daa_score` (`u64::MAX`
    /// everywhere today).
    fn check_slashing_evidence_genuine(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        slashing_evidence_genuine(txs, selected_parent_bond_view, self.genesis.hash, activated)
            .map_err(UnverifiableSlashingEvidenceInBlock)
    }

    /// kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate. Rejects a
    /// block that includes a transaction spending a **known** bond outpoint
    /// (present in the block's selected-parent bond view) whose bond is **not
    /// releasable** at the block's DAA score — releasable meaning the bond is
    /// `Unbonding` and `daa_score >= unbond_request_daa_score +
    /// unbonding_period_blocks`. A `Pending`/`Active` bond, an `Unbonding` bond
    /// before its release height, or a `Slashed` bond therefore cannot have its
    /// staked output-0 spent, which is what makes the declared `amount` real
    /// locked capital (D.1 pins `value == amount` to that output at acceptance).
    ///
    /// Like the sibling overlay checks this reads the same selected-parent
    /// [`ActiveBondView`], so it is per-block-deterministic and reorg-safe. Inert
    /// unless the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (`u64::MAX` on every current network, so this
    /// returns `Ok(())` immediately everywhere today).
    fn check_bond_spend_gate(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        bond_spend_gate(txs, selected_parent_bond_view, daa_score, activated)
            .map_err(|(spending_tx, bond_outpoint)| NonReleasableBondSpendInBlock(spending_tx, bond_outpoint))
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

/// Pure core of the ADR-0009 §"SlashingEvidencePayload" stateful genuineness
/// rule (testable without a processor). `activated` folds the
/// `dns_params.is_some() && daa_score >= dns_activation_daa_score` gate; when
/// `false` the rule is a no-op. For each `SlashingEvidence` among `txs` (the
/// structural triple + incompatibility are already enforced by the §A.2
/// stateless tx check), requires that the referenced bond resolves in
/// `bond_view` and that **both** equivocating attestations ML-DSA-verify
/// against that bond's `validator_pubkey` over their canonical
/// [`stake_attestation_message`] digests. On the first failure returns
/// `Err(bond_tx_id)`; the caller maps it to
/// [`UnverifiableSlashingEvidenceInBlock`].
fn slashing_evidence_genuine(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: BlockHash,
    activated: bool,
) -> Result<(), TransactionId> {
    if !activated {
        return Ok(());
    }
    for ev in slashing_evidence_from_accepted_txs(txs) {
        // The bond must exist so we can verify against its validator key.
        let Some(bond) = bond_view.get(&ev.bond_outpoint) else {
            return Err(ev.bond_outpoint.transaction_id);
        };
        for att in [&ev.attestation_a, &ev.attestation_b] {
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
                return Err(ev.bond_outpoint.transaction_id);
            }
        }
    }
    Ok(())
}

/// Pure core of the ADR-0016 §D.2 bond-UTXO spend-gate (testable without a
/// processor). `activated` folds the `dns_params.is_some() && daa_score >=
/// dns_activation_daa_score` gate; when `false` the rule is a no-op (every
/// current network). Scans every input of every transaction (the coinbase has
/// no inputs, so it contributes nothing); if an input's `previous_outpoint` is
/// a **known** bond outpoint in `bond_view` whose bond is **not releasable** at
/// `daa_score`, returns `Err((spending tx id, bond outpoint))` for the caller
/// to map to [`NonReleasableBondSpendInBlock`]. "Releasable" = the bond is
/// `Unbonding` (per [`effective_bond_status`]) **and** `daa_score >=
/// bond_release_daa_score` (`unbond_request_daa_score +
/// unbonding_period_blocks`). Non-bond outpoints are ignored, so ordinary
/// transactions are unaffected.
fn bond_spend_gate(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    daa_score: u64,
    activated: bool,
) -> Result<(), (TransactionId, TransactionOutpoint)> {
    if !activated {
        return Ok(());
    }
    for tx in txs {
        for input in tx.inputs.iter() {
            if let Some(bond) = bond_view.get(&input.previous_outpoint) {
                let releasable = effective_bond_status(bond, daa_score) == BondStatus::Unbonding
                    && bond_release_daa_score(bond).is_some_and(|release| daa_score >= release);
                if !releasable {
                    return Err((tx.id(), input.previous_outpoint));
                }
            }
        }
    }
    Ok(())
}

/// Pure core of the ADR-0013 Addendum C / ADR-0016 §D.4 slashing side-effect,
/// split out of [`VirtualStateProcessor::apply_slashing_side_effects`] so the
/// remove-stake + mint-reporter UTXO/multiset mutation can be unit-tested
/// without a full processor. The caller has already gated on activation and
/// resolved `effects` (canonical block order) against the selected-parent bond
/// view; this applies them.
///
/// For each effect the bond's locked output-0 is looked up in
/// `selected_parent_utxo_view` composed with the running `diff`. If present it
/// is removed — `S` leaves the supply — from both `diff` and `multiset`, and
/// then, when the reward is non-zero, the reporter UTXO is minted at
/// `(slashing_tx_id, 0)` into both (the slashing tx declares no outputs, so
/// index 0 is free). Net supply change is `R − S`; the remainder is implicitly
/// burned. The per-effect recompose lets a later effect observe an earlier
/// one's mutations, and the lookup doubles as a release-race guard: a bond
/// whose output-0 is already gone from the composed view is skipped rather than
/// double-removed. `mint_daa_score` (the block's DAA score) is stamped as the
/// minted entry's `block_daa_score`.
fn apply_slashing_effects_to_state<V: UtxoView>(
    effects: &[SlashingSideEffect],
    selected_parent_utxo_view: &V,
    diff: &mut UtxoDiff,
    multiset: &mut MuHash,
    mint_daa_score: u64,
) {
    for effect in effects {
        // The exact stored entry for the bond's locked output-0 (matches the
        // multiset element); `None` ⇒ already spent in this mergeset ⇒ skip.
        let Some(entry) = ({
            let composed = selected_parent_utxo_view.compose(&*diff);
            composed.get(&effect.bond_outpoint)
        }) else {
            continue;
        };
        // Remove S (the locked stake) from the diff and the multiset.
        diff.remove_utxo(&effect.bond_outpoint, &entry).expect("composed view reported the bond output-0 present");
        multiset.remove_utxo(&effect.bond_outpoint, &entry);

        // Mint the reporter reward R at (slashing_tx_id, 0), if non-zero.
        if let Some(out) = &effect.reporter_output {
            let mint_outpoint = TransactionOutpoint::new(effect.slashing_tx_id, 0);
            let mint_entry = UtxoEntry::new(out.value, out.script_public_key.clone(), mint_daa_score, false);
            diff.add_utxo(mint_outpoint, mint_entry.clone())
                .expect("slashing tx declares no outputs, so (slashing_tx_id, 0) is free");
            multiset.add_utxo(&mint_outpoint, &mint_entry);
        }
    }
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

    // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload" / item 2): the
    // stateful slashing-evidence genuineness rule's pure core. Covers the gate +
    // both reject branches (bond absent / signature invalid). The
    // accept-with-valid-signatures path needs ML-DSA-65 signing (libcrux) and is
    // covered by the dedicated reward-bearing e2e rather than here.
    mod slashing_evidence_genuine {
        use super::super::slashing_evidence_genuine as genuine;
        use kaspa_consensus_core::{
            BlockHash,
            constants::TX_VERSION,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                SlashingEvidencePayload, StakeAttestation, StakeBondRecord,
            },
            subnets::SUBNETWORK_ID_SLASHING_EVIDENCE,
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn attestation(bond_outpoint: TransactionOutpoint, target: u8) -> StakeAttestation {
            StakeAttestation {
                version: DNS_PAYLOAD_VERSION_V1,
                validator_id: Hash64::from_bytes([0xa1; 64]),
                bond_outpoint,
                epoch: 1,
                target_hash: Hash64::from_bytes([target; 64]),
                target_daa_score: 10_000,
                validator_set_commitment: Hash64::from_bytes([0x66; 64]),
                signature: vec![0u8; STAKE_ATTESTATION_SIG_LEN], // garbage — never verifies
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
                activation_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 32],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        // Two incompatible attestations for the same bond (equivocation).
        fn evidence_tx(op: TransactionOutpoint) -> Transaction {
            let ev = SlashingEvidencePayload {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                attestation_a: attestation(op, 0x55),
                attestation_b: attestation(op, 0x99),
                reporter_reward_spk_payload: [0xee; 32],
            };
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_SLASHING_EVIDENCE, 0, borsh::to_vec(&ev).unwrap())
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        #[test]
        fn noop_when_not_activated() {
            // Forged evidence passes when the gate is closed (every current net).
            assert_eq!(genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), false), Ok(()));
        }

        #[test]
        fn rejects_evidence_with_unknown_bond() {
            // Activated + empty bond view ⇒ bond unknown ⇒ reject.
            assert_eq!(genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), true), Err(Hash64::from_bytes([1; 64])));
        }

        #[test]
        fn rejects_evidence_with_invalid_signatures() {
            // Activated + bond present, but the (garbage) attestation signatures
            // fail verification ⇒ a forged evidence cannot slash the bond.
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn ok_when_no_slashing_evidence() {
            assert_eq!(genuine(&[], &ActiveBondView::new(), NET(), true), Ok(()));
        }
    }

    // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the bond-UTXO spend-gate's pure
    // core. Covers the gate plus each releasability branch: Active/Pending/
    // mid-unbonding/Slashed bonds are locked (reject), a released bond and a
    // non-bond input are spendable (accept).
    mod bond_spend_gate {
        use super::super::bond_spend_gate as gate;
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            dns_finality::{ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_VALIDATOR_PUBKEY_LEN, StakeBondRecord},
            subnets::SUBNETWORK_ID_NATIVE,
            tx::{Transaction, TransactionInput, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        // A normal (non-overlay) tx with a single input spending `op`.
        fn spending_tx(op: TransactionOutpoint) -> Transaction {
            let input = TransactionInput::new(op, vec![], 0, 0);
            Transaction::new(TX_VERSION, vec![input], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![])
        }

        // A bond record with all DAA-stamped fields cleared (so its effective
        // status is derived purely from `activation_daa_score`). The caller
        // tweaks the fields to select Pending/Active/Unbonding/Slashed.
        fn bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                unbonding_period_blocks: 5_000,
                owner_reward_spk_payload: [0xdd; 32],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        const DAA: u64 = 10_000;

        #[test]
        fn noop_when_not_activated() {
            // Spending an Active bond is fine while the gate is closed (every
            // current network: dns_activation = u64::MAX).
            let op = outpoint(1);
            let view = ActiveBondView::from_records([(op, bond(op))]);
            assert_eq!(gate(&[spending_tx(op)], &view, DAA, false), Ok(()));
        }

        #[test]
        fn rejects_spend_of_active_bond() {
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, bond(op))]); // activation 0 ⇒ Active at DAA.
            let tx = spending_tx(op);
            assert_eq!(gate(&[tx.clone()], &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn rejects_spend_of_pending_bond() {
            let op = outpoint(3);
            let mut b = bond(op);
            b.activation_daa_score = DAA + 1; // not yet active ⇒ Pending.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(&[tx.clone()], &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn rejects_spend_of_unbonding_before_release() {
            let op = outpoint(4);
            let mut b = bond(op);
            b.unbond_request_daa_score = Some(DAA - 1); // Unbonding, but release = DAA-1+5000 > DAA.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(&[tx.clone()], &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn allows_spend_of_releasable_bond() {
            let op = outpoint(5);
            let mut b = bond(op);
            b.unbond_request_daa_score = Some(1_000); // release = 1_000 + 5_000 = 6_000 ≤ DAA.
            let view = ActiveBondView::from_records([(op, b)]);
            assert_eq!(gate(&[spending_tx(op)], &view, DAA, true), Ok(()));
        }

        #[test]
        fn rejects_spend_of_slashed_bond() {
            let op = outpoint(6);
            let mut b = bond(op);
            b.slashed_at_daa_score = Some(5_000); // Slashed ⇒ terminal, never releasable.
            let view = ActiveBondView::from_records([(op, b)]);
            let tx = spending_tx(op);
            assert_eq!(gate(&[tx.clone()], &view, DAA, true), Err((tx.id(), op)));
        }

        #[test]
        fn ignores_non_bond_inputs() {
            // An input that is not a known bond outpoint is unaffected, even
            // when the gate is active.
            assert_eq!(gate(&[spending_tx(outpoint(7))], &ActiveBondView::new(), DAA, true), Ok(()));
        }

        #[test]
        fn ok_when_no_inputs() {
            let tx = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_NATIVE, 0, vec![]);
            assert_eq!(gate(&[tx], &ActiveBondView::new(), DAA, true), Ok(()));
        }
    }

    // kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): the slashing
    // side-effect *application* core. Given already-resolved effects, asserts
    // the remove-stake + mint-reporter mutation of the UTXO diff and the
    // multiset (and so the utxo_commitment): the stake leaves the supply, the
    // reporter UTXO is minted at (slashing_tx_id, 0), a zero reward mints
    // nothing (whole stake burns), and a missing output-0 is skipped whole
    // (release-race guard) so a reporter is never minted without the matching
    // stake removal. The expected commitment is rebuilt independently from the
    // final UTXO set, proving the add/remove history nets to the right state.
    mod slashing_side_effect_application {
        use super::super::apply_slashing_effects_to_state as apply;
        use kaspa_consensus_core::{
            dns_finality::SlashingSideEffect,
            muhash::MuHashExtensions,
            tx::{ScriptPublicKey, TransactionId, TransactionOutpoint, TransactionOutput, UtxoEntry},
            utxo::{utxo_collection::UtxoCollection, utxo_diff::UtxoDiff},
        };
        use kaspa_hashes::Hash64;
        use kaspa_muhash::MuHash;
        use std::collections::HashMap;

        const BOND_DAA: u64 = 1_000; // DAA at which the bond's output-0 was created.
        const MINT_DAA: u64 = 2_000; // DAA of the slashing block (stamped on the mint).

        fn spk(b: u8) -> ScriptPublicKey {
            ScriptPublicKey::from_vec(0, vec![b; 32])
        }

        fn bond_outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn slashing_tx_id(b: u8) -> TransactionId {
            Hash64::from_bytes([b; 64])
        }

        // The locked output-0 UTXO of a bond worth `amount`, as it sits in the
        // selected-parent UTXO set (the base view + the seeded multiset).
        fn bond_entry(amount: u64) -> UtxoEntry {
            UtxoEntry::new(amount, spk(0xb0), BOND_DAA, false)
        }

        // An effect slashing `amount`, paying a reporter `reward` (≤ amount) to
        // spk(0xee) minted at (tx, 0); `reward == 0` ⇒ no reporter output.
        fn effect(bond: TransactionOutpoint, amount: u64, reward: u64, tx: TransactionId) -> SlashingSideEffect {
            SlashingSideEffect {
                bond_outpoint: bond,
                slashed_amount_sompi: amount,
                reporter_output: (reward > 0).then(|| TransactionOutput::new(reward, spk(0xee))),
                burned_sompi: amount - reward,
                slashing_tx_id: tx,
            }
        }

        // Independent reconstruction of a multiset over an explicit UTXO set —
        // the apply path must reach the same commitment regardless of the
        // add/remove history that produced it.
        fn multiset_of(utxos: &[(TransactionOutpoint, UtxoEntry)]) -> MuHash {
            let mut mh = MuHash::new();
            for (op, e) in utxos {
                mh.add_utxo(op, e);
            }
            mh
        }

        #[test]
        fn removes_stake_and_mints_reporter() {
            let bond_op = bond_outpoint(0x01);
            let tx = slashing_tx_id(0x0a);
            let (amount, reward) = (1_000u64, 250u64);
            let entry = bond_entry(amount);

            // Base view holds the bond's locked output-0; empty diff; multiset
            // already contains the bond UTXO (it is in the committed set).
            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            apply(&[effect(bond_op, amount, reward, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            let mint_op = TransactionOutpoint::new(tx, 0);
            let mint_entry = UtxoEntry::new(reward, spk(0xee), MINT_DAA, false);

            // Diff: stake removed, reporter minted, nothing else touched.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert_eq!(diff.remove.len(), 1);
            assert_eq!(diff.add.get(&mint_op), Some(&mint_entry));
            assert_eq!(diff.add.len(), 1);

            // Commitment now equals a set that only ever held the reporter mint:
            // the removal cancelled the bond and the net set is exactly R.
            assert_eq!(multiset.finalize(), multiset_of(&[(mint_op, mint_entry)]).finalize());
        }

        #[test]
        fn zero_reward_burns_whole_stake() {
            let bond_op = bond_outpoint(0x02);
            let tx = slashing_tx_id(0x0b);
            let entry = bond_entry(1_000);

            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            apply(&[effect(bond_op, 1_000, 0, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            // Stake removed, nothing minted; commitment back to the empty set.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert!(diff.add.is_empty());
            assert_eq!(multiset.finalize(), MuHash::new().finalize());
        }

        #[test]
        fn skips_effect_when_output0_already_absent() {
            // Release-race guard: the bond's output-0 is not in the composed
            // view (already spent in this mergeset). The whole effect — removal
            // AND reporter mint — is skipped, so a reporter is never minted
            // without the matching stake removal.
            let bond_op = bond_outpoint(0x03);
            let tx = slashing_tx_id(0x0c);
            let base: UtxoCollection = HashMap::new(); // output-0 already gone.
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = MuHash::new();

            apply(&[effect(bond_op, 1_000, 250, tx)], &base, &mut diff, &mut multiset, MINT_DAA);

            assert!(diff.add.is_empty());
            assert!(diff.remove.is_empty());
            assert_eq!(multiset.finalize(), MuHash::new().finalize());
        }

        #[test]
        fn applies_each_of_several_distinct_bonds() {
            let (op_a, op_b) = (bond_outpoint(0x04), bond_outpoint(0x05));
            let (tx_a, tx_b) = (slashing_tx_id(0x0d), slashing_tx_id(0x0e));
            // Distinct amounts ⇒ distinct multiset elements; bond b's reward is 0
            // (burns entirely), bond a's reward is non-zero (mints a reporter).
            let (amt_a, amt_b, rew_a) = (1_000u64, 4_000u64, 100u64);
            let (e_a, e_b) = (bond_entry(amt_a), bond_entry(amt_b));

            let base: UtxoCollection = HashMap::from([(op_a, e_a.clone()), (op_b, e_b.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(op_a, e_a.clone()), (op_b, e_b.clone())]);

            apply(
                &[effect(op_a, amt_a, rew_a, tx_a), effect(op_b, amt_b, 0, tx_b)],
                &base,
                &mut diff,
                &mut multiset,
                MINT_DAA,
            );

            let mint_a = TransactionOutpoint::new(tx_a, 0);
            let mint_a_entry = UtxoEntry::new(rew_a, spk(0xee), MINT_DAA, false);

            // Both stakes removed; only a's reporter minted (b's reward is 0).
            assert_eq!(diff.remove.len(), 2);
            assert!(diff.remove.contains_key(&op_a) && diff.remove.contains_key(&op_b));
            assert_eq!(diff.add.len(), 1);
            assert_eq!(diff.add.get(&mint_a), Some(&mint_a_entry));

            // Net committed set = a's reporter mint only.
            assert_eq!(multiset.finalize(), multiset_of(&[(mint_a, mint_a_entry)]).finalize());
        }
    }
}
