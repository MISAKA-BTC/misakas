use super::VirtualStateProcessor;
use crate::{
    errors::{
        BlockProcessResult,
        RuleError::{
            BadAcceptedIDMerkleRoot, BadCoinbaseTransaction, BadOverlayCommitment, BadUTXOCommitment, IneligibleAttestationInBlock,
            InvalidTransactionsInUtxoContext, NonReleasableBondSpendInBlock, UnauthorizedUnbondRequestInBlock,
            UnverifiableSlashingEvidenceInBlock,
            WrongHeaderPruningPoint,
        },
    },
    model::stores::{
        block_transactions::BlockTransactionsStoreReader,
        daa::DaaStoreReader,
        ghostdag::{CompactGhostdagData, GhostdagData},
        headers::HeaderStoreReader,
        rewarded_epochs::RewardedEpochKeys,
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
        ATTESTATION_MLDSA87_CONTEXT, ActiveBondView, BlockEpochContribution, BondMutation, BondStatus, DnsParams, FeeSplitParams,
        OverlaySnapshot, RewardedEpochSet, SlashingSideEffect, attestations_from_accepted_txs, bond_mutations_from_accepted_txs,
        bond_release_daa_score, effective_bond_status,
        epoch_meets_quality_floor, epochs_finalized_at, recompute_epoch_tallies, resolve_slashing_side_effects,
        UNBOND_REQUEST_CONTEXT, slashing_evidence_from_accepted_txs, split_validator_pool, stake_attestation_message,
        unbond_request_message, unbond_requests_from_accepted_txs, validator_id_from_pubkey,
        validator_participation_reward_outputs, validator_quality_bonus_outputs, victim_compensation_outputs,
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
use kaspa_txscript::{script_class::parse_evm_deposit_lock, verify_mldsa87_with_context};
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
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): this block's validator quality
    /// sub-pool (`split_validator_pool(.).1`), persisted by `commit_utxo_state` as
    /// the per-epoch accumulator's recompute input. `0` (never persisted) below
    /// `pos_v2_activation_daa_score` — i.e. on the devnet/simnet preset
    /// (`GENESIS_ACTIVE_DNS_PARAMS`, fenced at `u64::MAX`); on mainnet/testnet
    /// (`PRODUCTION_DNS_PARAMS`, fence = 0) it is populated from block 1.
    pub validator_quality_subpool: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's security-reserve **accrual** — the
    /// `Σ security_reserve_sompi` of its slashing side-effects (set in `apply_slashing_side_effects`).
    /// Feeds the per-block reserve-balance recurrence. `0` below the v2 fence.
    pub reserve_accrual: u64,
    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's **cumulative reserve balance**
    /// (`balance_after(selected_parent) + reserve_accrual − drip`), persisted by `commit_utxo_state`
    /// when non-zero. The finalizing coinbase reads the selected parent's value for the drip. `0`
    /// (never persisted) below the v2 fence.
    pub reserve_balance_after: u64,
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
            validator_quality_subpool: 0,
            reserve_accrual: 0,
            reserve_balance_after: 0,
        }
    }

    pub fn selected_parent(&self) -> BlockHash {
        self.ghostdag_data.selected_parent
    }
}

/// kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): the per-tx bond-spend SKIP filter
/// threaded into mergeset acceptance validation. When `Some`, a transaction that spends a **known
/// non-releasable** bond's locked output-0 (resolved in `bond_view` at `daa_score`) fails UTXO
/// validation — so the acceptance loop SKIPS it (treats it like an invalid tx), keeping the carrying
/// block valid and the bond UTXO locked. `None` ⇒ the check is inert (every spend allowed by it), so
/// below the activation fence behavior is byte-identical to the legacy own-body-only gate.
///
/// `bond_view` is the **post-acceptance** view (selected-parent bonds + every bond freshly inserted
/// anywhere in this block's mergeset), a deterministic function of the shared inputs, so the rule is
/// construction == validation. Cheap to `Copy` (a shared ref + a u64); `Sync` for the parallel walk.
#[derive(Clone, Copy)]
pub(crate) struct BondSpendFilter<'a> {
    bond_view: &'a ActiveBondView,
    daa_score: u64,
}

impl BondSpendFilter<'_> {
    /// `true` iff `outpoint` is a known bond that is NOT releasable at `self.daa_score` (i.e. its
    /// locked output-0 must not be spent). Mirrors the legacy [`bond_spend_gate`] releasable test.
    fn locks(&self, outpoint: &TransactionOutpoint) -> bool {
        self.bond_view.get(outpoint).is_some_and(|bond| {
            let releasable = effective_bond_status(bond, self.daa_score) == BondStatus::Unbonding
                && bond_release_daa_score(bond).is_some_and(|release| self.daa_score >= release);
            !releasable
        })
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
    /// Gated on `dns_activation_daa_score` (= 0 on every current network), so it
    /// runs from genesis today. (The gate is retained for any net that sets it > 0.)
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

        // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): above the fence, build the
        // POST-ACCEPTANCE bond view the per-tx spend-skip is evaluated against = the selected-parent
        // bonds PLUS every bond freshly DECLARED by a StakeBond tx anywhere in this mergeset. Only
        // `Insert` mutations are applied (a fresh bond is always Pending/Active ⇒ non-releasable);
        // `Slash`/`Unbond` are deliberately omitted so an unaccepted or within-mergeset unbond can
        // never make a bond look releasable here (within-mergeset unbonds can't reach release — the
        // window is days). Including a bond declared by a StakeBond tx that turns out UTXO-invalid is
        // a harmless SAFE SUPERSET: its output-0 does not exist, so nothing can spend it. A
        // deterministic function of the shared (selected_parent_bond_view, mergeset) inputs, so it is
        // construction == validation. `None` (inert) below the fence ⇒ the legacy own-body gate is the
        // sole protection and acceptance is byte-identical to today.
        let bond_gate_view: Option<ActiveBondView> = self
            .dns_params
            .as_ref()
            // Active only at/above the mergeset fence AND at/above dns_activation (matching the legacy
            // gate's `dns_activation_daa_score` semantics). The dns_activation conjunct is defensive:
            // every sane config sets the mergeset fence ≥ dns_activation (and below dns_activation the
            // bond set is empty anyway), but pinning it makes the invariant explicit rather than implied.
            .filter(|p| {
                pov_daa_score >= p.bond_spend_gate_mergeset_activation_daa_score && pov_daa_score >= p.dns_activation_daa_score
            })
            .map(|_| {
                let mut view = selected_parent_bond_view.clone();
                let (min_bond, unbonding_floor) = self.dns_bond_floors();
                // The SAME raw block-tx set the acceptance loop below iterates (incl. each block's
                // coinbase, which is inert here: `bond_mutations_from_accepted_txs` only emits Inserts
                // for DNS-subnetwork StakeBond txs). This is a deliberately distinct, never-persisted
                // view used ONLY for the gate decision; the authoritative committed bond mutations are
                // derived from ACCEPTED txs (`dns_bond_mutations_from_acceptance`, processor.rs). The
                // two must stay superset-consistent (this raw view ⊇ the accepted-tx bond set).
                let mergeset_txs: Vec<Transaction> = once(ctx.selected_parent())
                    .chain(ctx.ghostdag_data.consensus_ordered_mergeset_without_selected_parent(self.ghostdag_store.deref()))
                    .flat_map(|b| (*self.block_transactions_store.get(b).unwrap()).clone())
                    .collect();
                let inserts: Vec<BondMutation> = bond_mutations_from_accepted_txs(&mergeset_txs, pov_daa_score, min_bond, unbonding_floor)
                    .into_iter()
                    .filter(|m| matches!(m, BondMutation::Insert(..)))
                    .collect();
                view.apply(&inserts);
                view
            });

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
            // kaspa-pq bond spend-gate (mergeset hardening): gate every accepted mergeset tx (incl.
            // the selected parent's body, also accepted here) against the post-acceptance view, so a
            // merge-blue spend of a non-releasable bond's output-0 is skipped. `None` (inert) below
            // the fence. The spend-skip is independent of `SkipScriptChecks`.
            let bond_filter = bond_gate_view.as_ref().map(|view| BondSpendFilter { bond_view: view, daa_score: pov_daa_score });
            let (validated_transactions, inner_multiset) =
                self.validate_transactions_with_muhash_in_parallel(&txs, &composed_view, pov_daa_score, validation_flags, bond_filter);

            ctx.multiset_hash.combine(&inner_multiset);

            // kaspa-pq ADR-0018 §F bridge wiring: classify each accepted tx's fee. A tx creating
            // ≥1 EVM_DEPOSIT_LOCK output (recognised by the SAME `parse_evm_deposit_lock` the
            // claim path uses at processes/evm — so the lock-shape definition can never diverge)
            // is a bridge tx: its whole fee is finality-class, split at the validator-primary §F
            // finality ratios instead of the 90/10 normal ratios. DOUBLY gated at THIS
            // accumulation site — shared by coinbase construction and validation (both call
            // `calculate_utxo_state`), so c==v holds structurally:
            //   1. `finality_fee_activation_daa_score` — the §F wiring fence;
            //   2. `evm_activation_daa_score` — lock OUTPUTS are consensus-legal on every net
            //      (the output-class exemption is unconditional), but the BRIDGE only exists on
            //      an EVM-active net; without this gate a miner on an EVM-inert net (mainnet
            //      today) could self-include a never-claimable lock tx and reroute its fee
            //      75/25 to the §E pool. Both scores are consensus-fixed per net and
            //      `pov_daa_score` is path-identical, so the conjunction preserves c==v.
            // Below either fence `finality_fee` stays 0 ⇒ byte-identical splits.
            let finality_fee_active = pov_daa_score >= self.evm_activation_daa_score
                && self.dns_params.as_ref().is_some_and(|p| pov_daa_score >= p.finality_fee_activation_daa_score);
            let mut block_fee = 0u64;
            let mut finality_fee = 0u64;
            for (validated_tx, _) in validated_transactions.iter() {
                ctx.mergeset_diff.add_transaction(validated_tx, pov_daa_score).unwrap();
                ctx.accepted_tx_ids.push(validated_tx.id());
                block_fee += validated_tx.calculated_fee;
                if finality_fee_active
                    && validated_tx.tx.outputs.iter().any(|o| parse_evm_deposit_lock(&o.script_public_key).is_some())
                {
                    finality_fee += validated_tx.calculated_fee;
                }
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
                BlockRewardData::new(coinbase_data.subsidy, block_fee, finality_fee, coinbase_data.miner_data.script_public_key),
            );
        }

        // kaspa-pq Phase 11 (ADR-0013 Addendum C / ADR-0016 §D.4): apply the
        // slashing side-effect over the fully-applied mergeset. Gated on
        // `dns_activation_daa_score` (= 0 on every current network), so it runs
        // from genesis everywhere (the 4-way reserve/victim split is the part
        // fenced behind `pos_v2_activation_daa_score` — see below).
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
    /// guard. Gated on `dns_activation_daa_score` (= 0 on every current
    /// network), so this runs from genesis today.
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
        // ADR-0018 "本格版" (PoS-v2) §slashing: the reserve + victim shares are fenced — `0` below
        // `pos_v2_activation_daa_score`, so `compute_slashing_distribution` degenerates to the pre-v2
        // 2-way (reporter + burn) on the devnet/simnet preset (fence = `u64::MAX`). On mainnet/testnet
        // (`PRODUCTION_DNS_PARAMS`, fence = 0) the full 4-way split runs from block 1.
        let (security_reserve_bps, victim_epoch_pool_bps) = if pov_daa_score >= dns_params.pos_v2_activation_daa_score {
            (dns_params.reward_params.security_reserve_bps, dns_params.reward_params.victim_epoch_pool_bps)
        } else {
            (0, 0)
        };
        let mut effects = resolve_slashing_side_effects(
            &accepted_txs,
            selected_parent_bond_view,
            pov_daa_score,
            dns_params.reward_params.slashing_reporter_reward_bps,
            security_reserve_bps,
            victim_epoch_pool_bps,
        );
        // ADR-0018 "本格版" (PoS-v2) victim compensation: for each slashed bond with a victim pool,
        // recompute the slashed validator's epoch's honest (non-slashed) included set from the
        // selected-parent window and build the victim outputs. Inert when fenced (pool = 0 ⇒ skip) —
        // i.e. on the devnet/simnet preset; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) the
        // victim pool is non-zero and these outputs are built from block 1. The recompute reads the same
        // selected-parent view in both the block-validation and virtual-recompute paths ⇒ construction == validation.
        for effect in effects.iter_mut() {
            if effect.victim_epoch_pool_sompi == 0 {
                continue;
            }
            let slashed_payload = selected_parent_bond_view.get(&effect.bond_outpoint).map(|b| b.owner_reward_spk_payload);
            effect.victim_outputs = self.slashed_epoch_victim_outputs(
                dns_params,
                ctx.selected_parent(),
                selected_parent_bond_view,
                effect.slashed_epoch,
                slashed_payload,
                effect.victim_epoch_pool_sompi,
            );
        }
        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): this block's security-reserve accrual = Σ of
        // the slashed bonds' reserve shares (0 when fenced). Feeds the reserve-balance recurrence
        // (`commit_utxo_state` persists `parent_balance + reserve_accrual − drip`). The reserve share
        // is NOT minted (it leaves the supply with the bond removal until it later drips back out).
        ctx.reserve_accrual = effects.iter().fold(0u64, |acc, e| acc.saturating_add(e.security_reserve_sompi));
        apply_slashing_effects_to_state(&effects, selected_parent_utxo_view, &mut ctx.mergeset_diff, &mut ctx.multiset_hash, pov_daa_score);
    }

    /// kaspa-pq ADR-0022: the selected-parent window's per-block epoch contributions (oldest →
    /// newest by DAA), drawn from [`Self::selected_chain_overlay_window`] so a pruned-IBD node's
    /// below-pruning-point history is supplied by the imported pruning-point snapshot (the raw
    /// selected-chain walk cannot traverse below the pruning point). The single seam every coinbase
    /// reward recompute (victim compensation / reserve drip / deferred quality bonus) reads, so they
    /// match a from-genesis node's accumulator after a prune. Byte-equivalent to the former raw chain
    /// walk wherever the walk never reaches the pruning point — `selected_chain_overlay_window` skips
    /// empty-contribution blocks, which are tally-neutral in `recompute_epoch_tallies` — so this is a
    /// no-op on a non-pruned node (the only path on every net while the pruning point is genesis).
    fn selected_chain_epoch_contributions(&self, selected_parent: BlockHash, parent_daa: u64, walk_bound: u64) -> Vec<BlockEpochContribution> {
        let mut v: Vec<BlockEpochContribution> = self
            .selected_chain_overlay_window(selected_parent, parent_daa, walk_bound)
            .into_iter()
            .map(|c| BlockEpochContribution { block_daa_score: c.block_daa_score, rewarded_keys: c.rewarded_keys, quality_subpool: c.quality_subpool })
            .collect();
        v.sort_by_key(|c| c.block_daa_score);
        v
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2) §slashing — the victim-compensation outputs for one
    /// slashed bond: the `victim_pool` distributed stake-proportionally among the **honest**
    /// validators of the slashed validator's epoch. Recomputes `slashed_epoch`'s accumulator
    /// `included` set from the selected-parent window (the same bounded walk + pure
    /// `recompute_epoch_tallies` Phase 1/2 use), drops the slashed validator (matched by its
    /// `owner_reward_spk_payload`), and pays the rest via [`victim_compensation_outputs`]. Resolves
    /// bonds against `bond_view` (as-of the selected parent) so the block-validation and
    /// virtual-recompute paths build byte-identical outputs (construction == validation, reorg-safe
    /// — a finalized/buried epoch's blocks are immutable). Empty while the v2 fence is closed (no
    /// accumulator rows in the window) — i.e. on the devnet/simnet preset; on mainnet/testnet
    /// (`PRODUCTION_DNS_PARAMS`, fence = 0) it runs from block 1.
    fn slashed_epoch_victim_outputs(
        &self,
        dns_params: &DnsParams,
        selected_parent: BlockHash,
        bond_view: &ActiveBondView,
        slashed_epoch: u64,
        slashed_payload: Option<[u8; 64]>,
        victim_pool: u64,
    ) -> Vec<TransactionOutput> {
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // Gather the selected-parent window's per-block contributions (sink-parent first, then
        // ancestors), stopping at the window edge or the v2 fence.
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);

        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(parent_daa, epoch_len, finalization_depth, &contributions, &bonds);
        let included = tallies.into_iter().find(|(epoch, _)| *epoch == slashed_epoch).map(|(_, tally)| tally.included).unwrap_or_default();
        // Drop the slashed (equivocating) validator — it earns no victim compensation in its own
        // misbehavior epoch. Matched by the owner reward payload carried in the accumulator.
        let honest: Vec<_> =
            included.into_iter().filter(|(payload, _)| slashed_payload.is_none_or(|sp| payload.as_bytes() != sp)).collect();
        let total_honest_stake: u128 = honest.iter().map(|(_, stake)| *stake as u128).sum();
        victim_compensation_outputs(victim_pool, &honest, total_honest_stake)
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4) §reserve drip — the security-reserve **drip**
    /// coinbase outputs THIS block emits for the epoch(s) it finalizes, plus the total dripped (for
    /// the reserve-balance recurrence). For each epoch the block finalizes (the same crossing as the
    /// quality bonus), drips `min(remaining_balance, reserve_drip_per_epoch_cap_sompi)` distributed
    /// stake-proportionally to that epoch's included validators (reusing the bonus distributor). The
    /// reserve decreases by exactly the **minted** amount (≤ budget), so it is value-conserving and
    /// the unspent tail rolls over. `parent_balance` is the selected parent's committed cumulative
    /// reserve balance (read by the caller from `reserve_balance_store`), so construction (template)
    /// and validation read the identical as-of-selected-parent balance ⇒ byte-identical, reorg-safe.
    /// Returns no outputs below the v2 fence / when the parent balance or the per-epoch cap is 0 /
    /// when no epoch crosses — so it is inert on the devnet/simnet preset (fence = `u64::MAX`); on
    /// mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) the drip runs from block 1.
    pub(super) fn reserve_drip_outputs(
        &self,
        dns_params: &DnsParams,
        daa_score: u64,
        selected_parent: BlockHash,
        bond_view: &ActiveBondView,
        parent_balance: u64,
    ) -> (Vec<TransactionOutput>, u64) {
        let cap = dns_params.reward_params.reserve_drip_per_epoch_cap_sompi;
        if daa_score < dns_params.pos_v2_activation_daa_score || parent_balance == 0 || cap == 0 {
            return (Vec::new(), 0);
        }
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();
        let Some((e_min, e_max)) = epochs_finalized_at(parent_daa, daa_score, epoch_len, finalization_depth) else {
            return (Vec::new(), 0);
        };

        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);
        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(daa_score, epoch_len, finalization_depth, &contributions, &bonds);

        let mut outputs = Vec::new();
        let mut remaining = parent_balance;
        let mut total_drip = 0u64;
        for (epoch, tally) in &tallies {
            if *epoch < e_min || *epoch > e_max || remaining == 0 {
                continue;
            }
            let budget = remaining.min(cap);
            if budget == 0 {
                continue;
            }
            // Stake-proportional distribution to the epoch's included validators (meets=true ⇒ pays).
            let drip = validator_quality_bonus_outputs(budget as u128, &tally.included, tally.expected_stake, true);
            let minted: u64 = drip.iter().fold(0u64, |acc, o| acc.saturating_add(o.value));
            outputs.extend(drip);
            remaining = remaining.saturating_sub(minted);
            total_drip = total_drip.saturating_add(minted);
        }
        (outputs, total_drip)
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

        // kaspa-pq ADR-0022: verify the DNS/PoS-v2 overlay-state commitment (as-of the
        // selected parent). The block-template builder committed the identical snapshot
        // (construction == validation); a pruned-IBD node imports the overlay stores at
        // the pruning point and this same check on the first post-pruning block verifies
        // them against its header. Gated on the overlay being active (`dns_params`).
        if self.dns_params.is_some() {
            let snap = self.compute_overlay_snapshot(ctx.selected_parent(), selected_parent_bond_view);
            let expected_overlay = snap.commitment_root();
            if expected_overlay != header.overlay_commitment_root {
                kaspa_core::warn!(
                    "[overlay-diag] block {} sp={} sp_daa={} bonds={} reserve={} window={} empty_root={} header_root={} computed_root={} window_detail={:?}",
                    header.hash,
                    ctx.selected_parent(),
                    self.headers_store.get_daa_score(ctx.selected_parent()).unwrap_or(u64::MAX),
                    snap.bonds.len(),
                    snap.reserve_balance,
                    snap.window.len(),
                    OverlaySnapshot::default().commitment_root(),
                    header.overlay_commitment_root,
                    expected_overlay,
                    snap.window
                        .iter()
                        .map(|c| (c.block_hash, c.block_daa_score, c.rewarded_keys.len(), c.quality_subpool))
                        .collect::<Vec<_>>()
                );
                return Err(BadOverlayCommitment(header.hash, header.overlay_commitment_root, expected_overlay));
            }
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

        // kaspa-pq Phase 10/11 (ADR-0016 §D.2): the legacy bond-UTXO spend-gate. Rejects a block
        // whose OWN BODY spends a known non-releasable bond outpoint, against the selected-parent bond
        // view. Inert below `dns_activation_daa_score`.
        //
        // kaspa-pq bond spend-gate mergeset hardening: this own-body REJECT gate misses a spend that
        // rides in a MERGE-BLUE block of this chain block's mergeset (those txs are accepted by
        // `calculate_utxo_state`, never presented here). At/above the
        // `bond_spend_gate_mergeset_activation_daa_score` fence, protection moves to the acceptance-
        // time SKIP in `calculate_utxo_state` (which covers BOTH the mergeset and — when this block is
        // later merged — its own body), so this legacy gate is disabled to avoid an honest miner
        // self-rejecting an own-body bond-spend the skip would simply not accept. The fence is
        // `u64::MAX` on every current preset, so this gate runs unchanged (byte-identical) today.
        let mergeset_bond_gate_active =
            self.dns_params.as_ref().is_some_and(|p| header.daa_score >= p.bond_spend_gate_mergeset_activation_daa_score);
        if !mergeset_bond_gate_active {
            self.check_bond_spend_gate(&txs, selected_parent_bond_view, header.daa_score)?;
        }

        // kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): reject a block whose
        // StakeUnbondRequest is not owner-authorized (unknown/ineligible bond, or a
        // bad owner key / signature), so an attacker cannot force honest bonds into
        // Unbonding to grief them out of the active set. Genesis-active.
        self.check_unbond_request_authorized(&txs, selected_parent_bond_view, header.daa_score)?;

        // kaspa-pq Phase 10/11 + Phase 13 (ADR-0009 Addendum B §B.5 / ADR-0018
        // §F+§E): the validator reward outputs the coinbase must carry. The §F
        // carve (`carve`) splits each source block's reward Worker/Validator/
        // Service; the Validator total (`validator_pool`) funds the §E
        // participation distribution computed by `validator_reward_outputs_for_block`.
        // Both are gated on `dns_activation_daa_score` (= 0 on every current network)
        // with the §F carve in Stage 3 (`full_reward_split_daa_score` = 0), so the
        // overlay is active and the fan-out runs from genesis everywhere. The rewarded
        // `(bond, epoch)` keys are stashed for `commit_utxo_state` (§B.3(c)).
        let mergeset_non_daa = self.daa_excluded_store.get_mergeset_non_daa(header.hash).unwrap();
        // ADR-0018 §F staged rollout: None (Stage 1) / bootstrap (Stage 2) / full
        // (Stage 3) selected by DAA, identically to the construction path.
        let carve = self.dns_params.as_ref().and_then(|p| p.reward_fee_split(header.daa_score));
        let validator_pool = carve.map_or(0, |fs| {
            self.coinbase_manager.coinbase_validator_pool(&ctx.ghostdag_data, &ctx.mergeset_rewards, &mergeset_non_daa, fs)
        });
        let (validator_reward_outputs, rewarded_keys, newly_included_stake, expected_stake) = self.validator_reward_outputs_for_block(
            &txs,
            selected_parent_bond_view,
            header.daa_score,
            ctx.selected_parent(),
            validator_pool,
        );
        ctx.validator_rewarded_keys = rewarded_keys;

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 1): stash this block's validator
        // quality sub-pool (the §E split's bonus share) for the per-epoch
        // accumulator. Gated by the v2 fence (`pos_v2_activation_daa_score`): on the
        // devnet/simnet preset (fence = `u64::MAX`) it stays 0 and `commit_utxo_state`
        // writes no row — inert there; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`,
        // fence = 0) it is populated from block 1. (Below `dns_activation` the pool is
        // already 0, since `validator_pool` is.) Does NOT affect the coinbase, so
        // construction == validation is untouched.
        ctx.validator_quality_subpool = self
            .dns_params
            .as_ref()
            .filter(|p| header.daa_score >= p.pos_v2_activation_daa_score)
            .map_or(0, |p| {
                split_validator_pool(validator_pool as u128, p.reward_params.validator_participation_bps).1.min(u64::MAX as u128) as u64
            });

        // kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 4): append the security-reserve **drip** outputs
        // (for the epoch[s] this block finalizes) after the participation + quality-bonus outputs, and
        // advance the per-block reserve-balance recurrence `balance_after = parent_balance +
        // reserve_accrual − drip`. The drip reads the selected parent's COMMITTED balance (so the
        // construction (template) and validation paths agree byte-for-byte). Inert below the v2 fence.
        let mut validator_reward_outputs = validator_reward_outputs;
        if let Some(dns_params) = self.dns_params.as_ref() {
            let parent_balance = self.reserve_balance_store.get(ctx.selected_parent()).unwrap_or(0);
            let (drip_outputs, drip_total) =
                self.reserve_drip_outputs(dns_params, header.daa_score, ctx.selected_parent(), selected_parent_bond_view, parent_balance);
            validator_reward_outputs.extend(drip_outputs);
            ctx.reserve_balance_after = parent_balance.saturating_add(ctx.reserve_accrual).saturating_sub(drip_total);
        }

        // Verify coinbase transaction (incl. the §F carve + §E fan-out + §D bounty).
        self.verify_coinbase_transaction(
            &txs[0],
            header.daa_score,
            &ctx.ghostdag_data,
            &ctx.mergeset_rewards,
            &mergeset_non_daa,
            &validator_reward_outputs,
            carve,
            (newly_included_stake, expected_stake),
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
        // kaspa-pq Phase 13 (ADR-0018 §D): `(newly_included_stake, expected_stake)`,
        // threaded to `expected_coinbase_transaction` for the inclusion bounty.
        inclusion: (u128, u128),
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
                inclusion,
            )
            .unwrap()
            .tx;
        if hashing::tx::hash(coinbase) != hashing::tx::hash(&expected_coinbase) {
            // kaspa-pq diagnostic (coinbase mismatch): dump the mismatch SHAPE so the
            // cache-retarget class (equal output count, payload already retargeted, a
            // single SCRIPT-only diff at a miner-script output index — old miner's spk
            // left behind) is distinguishable in-field from a reward-generation skew
            // (differing output count, or an AMOUNT diff, or a validator/bond-owner
            // script diff). Emitted only on the already-failing branch → consensus-neutral.
            let (n_act, n_exp) = (coinbase.outputs.len(), expected_coinbase.outputs.len());
            let first_diff = (0..n_act.min(n_exp)).find(|&i| coinbase.outputs[i] != expected_coinbase.outputs[i]);
            match first_diff {
                Some(i) => {
                    let (a, e) = (&coinbase.outputs[i], &expected_coinbase.outputs[i]);
                    kaspa_core::warn!(
                        "[coinbase-mismatch] outputs act={n_act} exp={n_exp}; first diff @{i}: value_eq={} script_eq={} payload_eq={} (act_value={} exp_value={})",
                        a.value == e.value,
                        a.script_public_key == e.script_public_key,
                        coinbase.payload == expected_coinbase.payload,
                        a.value,
                        e.value,
                    );
                }
                None => {
                    kaspa_core::warn!(
                        "[coinbase-mismatch] outputs act={n_act} exp={n_exp}; no in-range output diff (count or payload mismatch) payload_eq={}",
                        coinbase.payload == expected_coinbase.payload,
                    );
                }
            }
            Err(BadCoinbaseTransaction)
        } else {
            Ok(())
        }
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
    /// `dns_activation_daa_score` (= 0 everywhere today) — so it is active
    /// from genesis on every current network. Callers run the §B.4
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
    /// same pair. The overlay is genesis-active on every current network
    /// (`dns_activation_daa_score` = 0), so the walk reads the rows written as
    /// validators are rewarded.
    pub(super) fn validator_reward_outputs_for_block(
        &self,
        txs: &[Transaction],
        bond_view: &ActiveBondView,
        daa_score: u64,
        selected_parent: BlockHash,
        // kaspa-pq Phase 13 (ADR-0018 §F/§E): the validator-side coinbase pool
        // (`CoinbaseManager::coinbase_validator_pool`) this block's §E
        // participation rewards are distributed from. The caller computes it past
        // `dns_activation_daa_score` (= 0 on every current network), so it is
        // funded from genesis everywhere.
        validator_pool: u64,
        // kaspa-pq Phase 13 (ADR-0018 §D): also returns `(newly_included_stake,
        // expected_stake)` so the coinbase can pay the §D worker inclusion bounty.
    ) -> (Vec<TransactionOutput>, RewardedEpochKeys, u128, u128) {
        let Some(dns_params) = self.dns_params.as_ref() else {
            return (Vec::new(), Vec::new(), 0, 0);
        };
        if daa_score < dns_params.dns_activation_daa_score {
            return (Vec::new(), Vec::new(), 0, 0);
        }
        let window = dns_params.reward_uniqueness_window_blocks;

        // kaspa-pq DNS v3: canonical anchors as-of the selected parent (the deterministic
        // as-of-block view, like `bond_view` / the deferred-bonus path). Only an attestation
        // naming THIS chain's canonical lagged anchor for a READY, NON-DUPLICATE epoch earns a
        // reward — exactly the GoodAttestation v3 rule the PR4 StakeScore verifier applies, so
        // reward and StakeScore agree on what counts. A non-canonical or duplicate target earns
        // nothing (it is simply absent from `creditable`), and so never enters `rewarded_keys`
        // (hence never the §D bounty, §E pool, cross-block dedup, or the deferred bonus).
        let creditable = self.canonical_anchors_in_window(selected_parent, dns_params);

        // Resolve eligible, recent, CANONICAL attestations (canonical order). Recency
        // (§B.3(c)): an attestation whose target is older than the window earns
        // nothing — keeps the uniqueness walk below bounded.
        let mut attestations = Vec::new();
        for att in attestations_from_accepted_txs(txs) {
            if daa_score.saturating_sub(att.target_daa_score) > window {
                continue;
            }
            // v3 canonical gate: the epoch must be creditable (ready, non-duplicate) and the
            // attestation must name its canonical anchor exactly.
            let Some(anchor) = creditable.get(&att.epoch) else {
                continue;
            };
            if att.target_hash != anchor.anchor_hash || att.target_daa_score != anchor.anchor_daa_score {
                continue;
            }
            if let Some(bond) = bond_view.active_bond_at(&att.bond_outpoint, att.target_daa_score) {
                // ADR-0018 §E: carry the bond's stake — the proportional weight in the
                // participation distribution (against the expected-stake denominator).
                attestations.push((att.bond_outpoint, att.epoch, bond.owner_reward_spk_payload, bond.amount));
            }
        }

        // Build the already-rewarded prefix set: the selected parent and its
        // selected-chain ancestors within `window` DAA, unioning each block's
        // rewarded `(bond, epoch)` keys (§B.3(c)). ADR-0022: routed through
        // `selected_chain_overlay_window`, which merges the persisted below-pruning-
        // point window — so a pruned-IBD node dedups post-pruning blocks against the
        // pre-pruning rewards too (its walk cannot reach below the pruning point).
        // Inert merge on a from-genesis node.
        let mut already_rewarded = RewardedEpochSet::new();
        for c in self.selected_chain_overlay_window(selected_parent, daa_score, window) {
            for (bond_outpoint, epoch) in c.rewarded_keys.iter() {
                already_rewarded.insert(*bond_outpoint, *epoch);
            }
        }

        // ADR-0018 §E: distribute the participation sub-pool proportionally by stake against the
        // epoch's expected (total active) stake — the anti-capture denominator — with the same
        // within-block + cross-block (§B.3(c)) `(bond, epoch)` uniqueness and a whole-output pool
        // cap (Σ ≤ pool; the unspent remainder is not minted).
        //
        // M-04 (denominator definition): the per-block reward intentionally uses the INCLUSION-time
        // active set (`total_active_stake_at(daa_score)`, below) as the expected-stake denominator,
        // whereas the StakeScore security signal uses the epoch-ANCHOR-time set. Both are
        // per-block-deterministic (read from the same selected-parent bond view), so neither splits;
        // they differ only in reference point, which is correct — the reward pays inclusion in THIS
        // block against the stake live at THIS block, while StakeScore measures buried-epoch security.
        //
        // ADR-0018 "本格版" (PoS-v2): the participation/quality split is **fenced**. Below
        // `pos_v2_activation_daa_score` the FULL pool funds participation (effective bps = 10_000),
        // byte-identical to the pre-v2 behavior regardless of the configured `validator_participation_bps`
        // — so on the devnet/simnet preset (fence = `u64::MAX`) raising the quality share in the presets
        // stays inert. At/above the fence — i.e. on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0)
        // from block 1 — the configured split carves the quality-bonus sub-pool, which the per-epoch
        // accumulator accrues (Phase 1) and `deferred_quality_bonus_outputs` (below) pays at finalization.
        let expected_stake = bond_view.total_active_stake_at(daa_score) as u128;
        let participation_bps = if daa_score >= dns_params.pos_v2_activation_daa_score {
            dns_params.reward_params.validator_participation_bps
        } else {
            10_000
        };
        let (participation_pool, _quality_bonus_pool) = split_validator_pool(validator_pool as u128, participation_bps);

        // ADR-0018 §D base inclusion bounty: the stake this block NEWLY includes — the
        // same recency-filtered attestations under the same within-block + cross-block
        // (§B.3(c)) `(bond, epoch)` dedup as §E, but summed pre-pool-cap (the miner
        // included them regardless of what §E could pay). The coinbase pays the includer
        // a proportional share of the §D pool against `expected_stake`.
        let mut seen_in_block = RewardedEpochSet::new();
        let mut newly_included_stake: u128 = 0;
        for (bond_outpoint, epoch, _payload, stake) in &attestations {
            if already_rewarded.contains(bond_outpoint, *epoch) || !seen_in_block.insert(*bond_outpoint, *epoch) {
                continue;
            }
            newly_included_stake += *stake as u128;
        }

        let (mut outputs, rewarded_keys) =
            validator_participation_reward_outputs(participation_pool, expected_stake, &attestations, &already_rewarded);

        // ADR-0018 "本格版" (PoS-v2) §E deferred quality bonus: append the bonus outputs for any
        // epoch THIS block first buries beyond the finalization window (φS-gated), recomputed from
        // the selected-parent window. Inert below the v2 fence. The finalized epochs are old
        // (buried by `reward_window + max_reorg_horizon`) and disjoint from the participation
        // epochs (recent, within `reward_window`), so the two output sets never double-pay.
        outputs.extend(self.deferred_quality_bonus_outputs(dns_params, daa_score, selected_parent, bond_view));

        (outputs, rewarded_keys, newly_included_stake, expected_stake)
    }

    /// kaspa-pq ADR-0018 "本格版" (PoS-v2, Phase 2) §E — the **deferred quality-bonus** coinbase
    /// outputs THIS block emits for every epoch it newly finalizes. An epoch `E` is finalized-by-
    /// this-block iff its finalization threshold `(E+1)·L + finalization_depth` falls in
    /// `(selected_parent.daa_score, daa_score]` — a pure DAA crossing, so on any chain exactly one
    /// block pays `E` (the once-per-epoch guard; no extra store). For each crossed `E` the tally is
    /// recomputed from the selected-parent window via [`recompute_epoch_tallies`] (the same pure
    /// core the Phase-1 accumulator store uses); because `E` is buried beyond `reward_window +
    /// max_reorg_horizon` its contributing blocks are reorg-immutable, so the coinbase
    /// **construction** and **validation** paths — and any competing chain — build byte-identical
    /// outputs. Each crossed epoch that met φS pays its accrued quality pool to its included
    /// validators ([`validator_quality_bonus_outputs`]); one that missed φS pays nothing (rollover).
    ///
    /// Returns no outputs below the v2 fence (`pos_v2_activation_daa_score`), or when no epoch
    /// crosses this block — so it is inert on the devnet/simnet preset (fence = `u64::MAX`); on
    /// mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) it pays from block 1, and is
    /// O(1) amortized (the deep window walk runs only on the ~1-in-`L` crossing blocks).
    fn deferred_quality_bonus_outputs(
        &self,
        dns_params: &DnsParams,
        daa_score: u64,
        selected_parent: BlockHash,
        // The bond set as-of the block's selected parent (the deterministic, as-of-block view the
        // participation path uses). Resolving the finalized epochs against THIS — not the live
        // `stake_bonds_store` (as-of the current sink) — is what keeps construction == validation
        // when a non-tip block is validated. Buried bonds are immutable, so this is also reorg-safe.
        bond_view: &ActiveBondView,
    ) -> Vec<TransactionOutput> {
        if daa_score < dns_params.pos_v2_activation_daa_score {
            return Vec::new();
        }
        let epoch_len = dns_params.epoch_length_blocks.max(1);
        let finalization_depth = dns_params.reward_uniqueness_window_blocks.saturating_add(dns_params.max_reorg_horizon_blocks);
        let parent_daa = self.headers_store.get_daa_score(selected_parent).unwrap();

        // The inclusive epoch range this block newly finalizes (thresholds crossed in
        // `(parent_daa, daa_score]`); `None` ⇒ this block finalizes nothing (the common case).
        let Some((e_min, e_max)) = epochs_finalized_at(parent_daa, daa_score, epoch_len, finalization_depth) else {
            return Vec::new();
        };

        // Recompute the finalized epochs' tallies from the selected-parent window (the same bounded
        // walk + pure core the Phase-1 accumulator uses), anchored at the selected parent so
        // construction and validation read the identical buried history.
        let walk_bound = finalization_depth.saturating_add(epoch_len.saturating_mul(2));
        let contributions = self.selected_chain_epoch_contributions(selected_parent, parent_daa, walk_bound);
        let bonds = bond_view.records();
        let tallies = recompute_epoch_tallies(daa_score, epoch_len, finalization_depth, &contributions, &bonds);

        // Pay each crossed epoch in `[e_min, e_max]` that met φS.
        let mut outputs = Vec::new();
        for (epoch, tally) in &tallies {
            if *epoch < e_min || *epoch > e_max {
                continue;
            }
            let included_sum: u128 = tally.included.iter().map(|(_, s)| *s as u128).sum();
            let meets = epoch_meets_quality_floor(included_sum, tally.expected_stake, dns_params.stake_event_quality_floor_bps);
            outputs.extend(validator_quality_bonus_outputs(tally.quality_pool_accrued, &tally.included, tally.expected_stake, meets));
        }
        outputs
    }

    /// kaspa-pq Phase 10/11 (ADR-0009 Addendum B §B.4): block-template
    /// pre-filter. Drops any `StakeAttestationShard` tx carrying an attestation
    /// that is not §B.4-eligible (its bond does not resolve to `Active` in
    /// `bond_view` with a valid signature) so that a block mined from the
    /// template passes the eligibility rule rather than self-disqualifying.
    /// Non-shard txs are always retained. Active when the overlay is
    /// configured **and** past `dns_activation_daa_score` (= 0
    /// everywhere today), so on every current network this runs and the
    /// template reflects the overlay. Recency is *not* filtered here: a stale-but-
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
    /// `target_daa_score`) **and** its ML-DSA-87 signature must verify. This
    /// makes "included ⇒ rewardable" a consensus invariant, so the coinbase
    /// reward fan-out (PR-10.5′-b3) needs no skip set. Reward **uniqueness**
    /// (Addendum B §B.3(c)) is a reward-emission concern, not a validity one
    /// (a duplicate `(bond, epoch)` is simply unrewarded), and is not checked
    /// here.
    ///
    /// Active when the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (= 0 on every current network, so
    /// this runs from genesis today). The canonical
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
    /// only) cannot mutate a bond to `Slashed`. Active when the overlay is
    /// configured **and** past `dns_activation_daa_score` (= 0
    /// everywhere today).
    fn check_slashing_evidence_genuine(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let Some(params) = self.dns_params.as_ref() else { return Ok(()) };
        let activated = daa_score >= params.dns_activation_daa_score;
        slashing_evidence_genuine(
            txs,
            selected_parent_bond_view,
            self.genesis.hash,
            daa_score,
            params.evidence_window_blocks,
            activated,
        )
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
    /// [`ActiveBondView`], so it is per-block-deterministic and reorg-safe. Active
    /// when the overlay is configured **and** `daa_score` has reached
    /// `dns_activation_daa_score` (= 0 on every current network, so this
    /// runs from genesis today).
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

    /// kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): the stake-unbond owner-
    /// authorization rule. Rejects a block carrying a `StakeUnbondRequest` that
    /// is not authorized by the bond owner (see [`unbond_request_authorized`]).
    /// Inert below activation.
    fn check_unbond_request_authorized(
        &self,
        txs: &[Transaction],
        selected_parent_bond_view: &ActiveBondView,
        daa_score: u64,
    ) -> BlockProcessResult<()> {
        let activated = self.dns_params.as_ref().is_some_and(|p| daa_score >= p.dns_activation_daa_score);
        unbond_request_authorized(txs, selected_parent_bond_view, self.genesis.hash.as_byte_slice(), daa_score, activated)
            .map_err(|(tx_id, bond_outpoint)| UnauthorizedUnbondRequestInBlock(tx_id, bond_outpoint))
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
                // `None`: the own-body / template path is not mergeset acceptance; the legacy
                // own-body bond gate (below the fence) covers it, see `verify_expected_utxo_state`.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags, None).ok().map(|vtx| (vtx, i as u32)))
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
        // kaspa-pq bond spend-gate (mergeset hardening): forwarded to the per-tx check so a mergeset
        // tx spending a non-releasable bond is SKIPPED (not accepted, not muhashed). `None` ⇒ inert.
        bond_filter: Option<BondSpendFilter>,
    ) -> (SmallVec<[(ValidatedTransaction<'a>, u32); 2]>, MuHash) {
        self.thread_pool.install(|| {
            txs
                .par_iter() // We can do this in parallel without complications since block body validation already ensured
                            // that all txs within each block are independent
                .enumerate()
                .skip(1) // Skip the coinbase tx.
                .filter_map(|(i, tx)| self.validate_transaction_in_utxo_context(tx, &utxo_view, pov_daa_score, flags, bond_filter).ok().map(|vtx| {
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
        // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): when `Some`, a tx that spends
        // a known non-releasable bond's locked output-0 fails validation, so the caller's `filter_map`
        // SKIPS it (it is not accepted, its muhash/diff contribution is never produced, the bond stays
        // locked). `None` on every path that is not mergeset-acceptance, and on every net below the
        // fence ⇒ byte-identical to the legacy own-body-only gate.
        bond_filter: Option<BondSpendFilter>,
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
        // kaspa-pq bond spend-gate (mergeset hardening): reject — so the caller skips, NOT rejecting
        // the whole block — any tx whose input spends a known non-releasable bond's locked output-0.
        // Inert when `bond_filter` is `None` (every path except fence-active mergeset acceptance).
        if let Some(filter) = bond_filter {
            if let Some(input) = transaction.inputs.iter().find(|input| filter.locks(&input.previous_outpoint)) {
                return Err(TxRuleError::SpendsNonReleasableBond(input.previous_outpoint));
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
/// `false` the rule is a no-op. On every current network the gate is `true`
/// from genesis (`dns_activation_daa_score` = 0), so the rule is active. On the
/// first ineligible attestation returns `Err((bond tx id, epoch))`; the caller maps
/// it to [`IneligibleAttestationInBlock`]. An attestation is eligible iff its
/// bond resolves to `Active` in `bond_view` at the attestation's
/// `target_daa_score` **and** its ML-DSA-87 signature verifies over the
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
        // kaspa-pq DNS v2 (P-1A): the self-declared validator_id must match the bond's
        // validator_pubkey_hash (validator_id is not in the signed digest); reward eligibility
        // shares the StakeScore binding so no reward can be earned by a non-canonical validator_id.
        if att.validator_id != bond.validator_pubkey_hash {
            return Err((att.bond_outpoint.transaction_id, att.epoch));
        }
        // (b) ML-DSA-87 signature verifies over the canonical digest.
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
            verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
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
    including_daa: u64,
    evidence_window_blocks: u64,
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
        // audit #2: freshness — the evidence must be included within `evidence_window_blocks` of the
        // newer equivocating attestation's target. This bounds how far back slashing can reach and
        // keeps it inside the bond's still-locked window (the params invariant `unbonding_period >=
        // max_reorg_horizon + evidence_window` guarantees a fresh evidence still finds the staked
        // output-0 present for the slashing side-effect to remove). A stale equivocation dredged up
        // long after the fact to grief a since-honest validator is rejected.
        let newest_target = ev.attestation_a.target_daa_score.max(ev.attestation_b.target_daa_score);
        if including_daa.saturating_sub(newest_target) > evidence_window_blocks {
            return Err(ev.bond_outpoint.transaction_id);
        }
        for att in [&ev.attestation_a, &ev.attestation_b] {
            // kaspa-pq DNS v2 (P-1A): both equivocating attestations must be bound to the bond's
            // validator_pubkey_hash (validator_id is not in the signed digest), so slashing can't be
            // spoofed against a bond via a mismatched validator_id.
            if att.validator_id != bond.validator_pubkey_hash {
                return Err(ev.bond_outpoint.transaction_id);
            }
            // audit #2: the bond must have had slashable locked stake at the attestation's target —
            // Active or Unbonding, the same set `resolve_slashing_side_effects` can slash. An
            // equivocation by a bond that was still Pending (never activated) or already Slashed at
            // that target had no stake at risk, so it is not slashable.
            if !matches!(effective_bond_status(bond, att.target_daa_score), BondStatus::Active | BondStatus::Unbonding) {
                return Err(ev.bond_outpoint.transaction_id);
            }
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
                verify_mldsa87_with_context(&bond.validator_pubkey, &digest, &att.signature, ATTESTATION_MLDSA87_CONTEXT),
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

/// kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): pure core of the stake-unbond
/// owner-authorization block-validity rule (testable without a processor).
/// `activated` folds the `dns_params.is_some() && daa_score >=
/// dns_activation_daa_score` gate; when `false` the rule is a no-op. For each
/// `StakeUnbondRequest` among `txs`, requires: (a) the referenced bond resolves
/// in `bond_view` and is `Pending`/`Active` at `daa_score` (not already
/// `Unbonding`/`Slashed`, so at most one unbond mutation applies per bond per
/// chain — keeping `ActiveBondView` apply/revert clean); (b) the payload's
/// `owner_pubkey` hashes to the bond's `owner_pubkey_hash`; and (c) its
/// ML-DSA-87 signature verifies over the canonical [`unbond_request_message`]
/// digest under [`UNBOND_REQUEST_CONTEXT`]. On the first failure returns
/// `Err((unbond tx id, bond outpoint))`, mapped to
/// [`UnauthorizedUnbondRequestInBlock`]. This is what stops an attacker forcing
/// honest bonds into `Unbonding` to grief them out of the active set.
fn unbond_request_authorized(
    txs: &[Transaction],
    bond_view: &ActiveBondView,
    net_id: &[u8],
    daa_score: u64,
    activated: bool,
) -> Result<(), (TransactionId, TransactionOutpoint)> {
    if !activated {
        return Ok(());
    }
    for (tx_id, req) in unbond_requests_from_accepted_txs(txs) {
        // (a) the bond must exist and still be locked-but-not-yet-exiting.
        let Some(bond) = bond_view.get(&req.bond_outpoint) else {
            return Err((tx_id, req.bond_outpoint));
        };
        if !matches!(effective_bond_status(bond, daa_score), BondStatus::Pending | BondStatus::Active) {
            return Err((tx_id, req.bond_outpoint));
        }
        // (b) the signing key must be THIS bond's owner.
        if validator_id_from_pubkey(&req.owner_pubkey) != bond.owner_pubkey_hash {
            return Err((tx_id, req.bond_outpoint));
        }
        // (c) the owner's signature over the network- and bond-bound digest must verify (audit M-04).
        let digest = unbond_request_message(net_id, req.bond_outpoint).as_bytes();
        if !matches!(
            verify_mldsa87_with_context(&req.owner_pubkey, &digest, &req.signature, UNBOND_REQUEST_CONTEXT),
            Ok(true)
        ) {
            return Err((tx_id, req.bond_outpoint));
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

        // kaspa-pq ADR-0018 "本格版" (PoS-v2): mint the victim-compensation outputs at
        // `(slashing_tx_id, 2..)` (index 1 is reserved/kept free). Empty while the v2 fence is closed —
        // i.e. on the devnet/simnet preset (fence = `u64::MAX`) ⇒ no extra mints ⇒ byte-identical to the
        // pre-v2 2-way slashing; on mainnet/testnet (`PRODUCTION_DNS_PARAMS`, fence = 0) these mint from
        // block 1. The security-reserve share
        // (`effect.security_reserve_sompi`) is NOT minted here: it leaves the supply with the bond
        // removal (≡ burn) until Phase 4 accrues it to the reserve pool. Σ(reporter + victim) ≤ S, so
        // the slash stays value-conserving.
        for (i, out) in effect.victim_outputs.iter().enumerate() {
            let mint_outpoint = TransactionOutpoint::new(effect.slashing_tx_id, 2 + i as u32);
            let mint_entry = UtxoEntry::new(out.value, out.script_public_key.clone(), mint_daa_score, false);
            diff.add_utxo(mint_outpoint, mint_entry.clone())
                .expect("slashing tx declares no outputs, so (slashing_tx_id, 2+i) is free");
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
    // ML-DSA-87 signing (libcrux) and is covered by the PR-10.5′-b3 end-to-end
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
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        #[test]
        fn noop_when_not_activated() {
            // Even an attestation referencing an unknown bond passes when the
            // gate is closed (the `false` arg below). Current nets run with the
            // gate open (dns_activation = 0); this covers the pre-activation path.
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

    // kaspa-pq H-05 (audit / ADR-0010 "Unbonding"): the stake-unbond owner-
    // authorization rule. Unlike the mods above, this exercises the full ACCEPT
    // path with a real ML-DSA-87 owner signature (libcrux) — owner-authorization
    // is THE security property (it blocks the active-set grief attack).
    mod unbond_request_authorized {
        use super::super::unbond_request_authorized as authz;
        use kaspa_consensus_core::{
            constants::TX_VERSION,
            dns_finality::{
                ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_ATTESTATION_SIG_LEN, STAKE_VALIDATOR_PUBKEY_LEN,
                StakeBondRecord, StakeUnbondRequestPayload, UNBOND_REQUEST_CONTEXT, unbond_request_message,
                validator_id_from_pubkey,
            },
            subnets::SUBNETWORK_ID_STAKE_UNBOND,
            tx::{Transaction, TransactionOutpoint},
        };
        use kaspa_hashes::Hash64;
        use libcrux_ml_dsa::ml_dsa_87 as mldsa;

        // audit M-04: the signer and verifier must agree on the network id bound into the digest.
        const NET_ID: &[u8] = b"audit-m04-unbond-test-net";

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn owner_kp(seed: u8) -> mldsa::MLDSA87KeyPair {
            mldsa::generate_key_pair([seed; 32])
        }

        // A bond whose `owner_pubkey_hash` binds to `kp`, Active from genesis.
        fn bond_owned_by(op: TransactionOutpoint, kp: &mldsa::MLDSA87KeyPair) -> StakeBondRecord {
            let owner_pubkey = kp.verification_key.as_ref().to_vec();
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: validator_id_from_pubkey(&owner_pubkey),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        fn unbond_tx(op: TransactionOutpoint, owner_pubkey: Vec<u8>, signature: Vec<u8>) -> Transaction {
            let payload =
                borsh::to_vec(&StakeUnbondRequestPayload { version: DNS_PAYLOAD_VERSION_V1, bond_outpoint: op, owner_pubkey, signature })
                    .unwrap();
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_STAKE_UNBOND, 0, payload)
        }

        fn signed_unbond_tx(op: TransactionOutpoint, kp: &mldsa::MLDSA87KeyPair) -> Transaction {
            let digest = unbond_request_message(NET_ID, op).as_bytes();
            let sig = mldsa::sign(&kp.signing_key, &digest, UNBOND_REQUEST_CONTEXT, [0x99u8; 32]).expect("sign");
            unbond_tx(op, kp.verification_key.as_ref().to_vec(), sig.as_ref().to_vec())
        }

        #[test]
        fn noop_when_not_activated() {
            let op = outpoint(1);
            let tx = unbond_tx(op, vec![0u8; STAKE_VALIDATOR_PUBKEY_LEN], vec![0u8; STAKE_ATTESTATION_SIG_LEN]);
            assert_eq!(authz(&[tx], &ActiveBondView::new(), NET_ID, 10_000, false), Ok(()));
        }

        #[test]
        fn accepts_owner_authorized_request() {
            let op = outpoint(2);
            let kp = owner_kp(2);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &kp))]);
            assert_eq!(authz(&[signed_unbond_tx(op, &kp)], &view, NET_ID, 10_000, true), Ok(()));
        }

        #[test]
        fn rejects_unknown_bond() {
            let op = outpoint(3);
            let kp = owner_kp(3);
            assert!(authz(&[signed_unbond_tx(op, &kp)], &ActiveBondView::new(), NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_request_signed_by_non_owner() {
            // The grief attack: a bond owned by `owner`, request signed by `attacker` → blocked.
            let op = outpoint(4);
            let owner = owner_kp(4);
            let attacker = owner_kp(40);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &owner))]);
            assert!(authz(&[signed_unbond_tx(op, &attacker)], &view, NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_bad_signature() {
            let op = outpoint(5);
            let kp = owner_kp(5);
            let view = ActiveBondView::from_records([(op, bond_owned_by(op, &kp))]);
            let tx = unbond_tx(op, kp.verification_key.as_ref().to_vec(), vec![0u8; STAKE_ATTESTATION_SIG_LEN]);
            assert!(authz(&[tx], &view, NET_ID, 10_000, true).is_err());
        }

        #[test]
        fn rejects_already_unbonding_bond() {
            // at-most-once: a bond already Unbonding cannot be unbonded again (clean revert).
            let op = outpoint(6);
            let kp = owner_kp(6);
            let mut rec = bond_owned_by(op, &kp);
            rec.unbond_request_daa_score = Some(1);
            let view = ActiveBondView::from_records([(op, rec)]);
            assert!(authz(&[signed_unbond_tx(op, &kp)], &view, NET_ID, 10_000, true).is_err());
        }
    }

    // kaspa-pq Phase 10/11 (ADR-0009 §"SlashingEvidencePayload" / item 2): the
    // stateful slashing-evidence genuineness rule's pure core. Covers the gate +
    // both reject branches (bond absent / signature invalid). The
    // accept-with-valid-signatures path needs ML-DSA-87 signing (libcrux) and is
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
                created_daa_score: 0,
                unbonding_period_blocks: 100,
                owner_reward_spk_payload: [0xdd; 64],
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
                reporter_reward_spk_payload: [0xee; 64],
            };
            Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_SLASHING_EVIDENCE, 0, borsh::to_vec(&ev).unwrap())
        }

        const NET: fn() -> BlockHash = || Hash64::from_bytes([0x07; 64]);

        // The fixture attestations target DAA 10_000; a block at FRESH_DAA with a WINDOW-block
        // evidence window is well within the freshness bound.
        const FRESH_DAA: u64 = 10_000;
        const WINDOW: u64 = 200_000;

        #[test]
        fn noop_when_not_activated() {
            // Forged evidence passes when the gate is closed (every current net).
            assert_eq!(genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, false), Ok(()));
        }

        #[test]
        fn rejects_evidence_with_unknown_bond() {
            // Activated + empty bond view ⇒ bond unknown ⇒ reject.
            assert_eq!(
                genuine(&[evidence_tx(outpoint(1))], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, true),
                Err(Hash64::from_bytes([1; 64]))
            );
        }

        #[test]
        fn rejects_evidence_with_invalid_signatures() {
            // Activated + bond present + fresh, but the (garbage) attestation signatures fail
            // verification ⇒ a forged evidence cannot slash the bond.
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), FRESH_DAA, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn rejects_stale_evidence_outside_the_window() {
            // audit #2: bond present, but the including block is more than `evidence_window_blocks`
            // past the equivocating attestations' target ⇒ stale ⇒ reject.
            let op = outpoint(2);
            let view = ActiveBondView::from_records([(op, active_bond(op))]);
            let stale_daa = 10_000 + WINDOW + 1; // target=10_000; diff = WINDOW+1 > WINDOW.
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), stale_daa, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn rejects_evidence_when_bond_not_slashable_at_target() {
            // audit #2: bond present + fresh + validator_id matches, but the bond was still Pending
            // (activation after the target) ⇒ no stake at risk at that target ⇒ not slashable.
            let op = outpoint(2);
            let mut bond = active_bond(op);
            bond.validator_pubkey_hash = Hash64::from_bytes([0xa1; 64]); // match the attestation's validator_id
            bond.activation_daa_score = 50_000; // Pending at target 10_000
            let view = ActiveBondView::from_records([(op, bond)]);
            assert_eq!(genuine(&[evidence_tx(op)], &view, NET(), FRESH_DAA, WINDOW, true), Err(Hash64::from_bytes([2; 64])));
        }

        #[test]
        fn ok_when_no_slashing_evidence() {
            assert_eq!(genuine(&[], &ActiveBondView::new(), NET(), FRESH_DAA, WINDOW, true), Ok(()));
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
                created_daa_score: 0,
                unbonding_period_blocks: 5_000,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        const DAA: u64 = 10_000;

        #[test]
        fn noop_when_not_activated() {
            // Spending an Active bond is fine while the gate is closed (the
            // gate-closed path; current nets run with it open, dns_activation = 0).
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

    // kaspa-pq (ADR-0016 §D.2, bond spend-gate mergeset hardening): the `BondSpendFilter::locks`
    // predicate that drives the acceptance-time SKIP (the merge-blue-aware complement to the legacy
    // own-body `bond_spend_gate`). Same releasability semantics, exercised per-outpoint.
    mod bond_spend_filter {
        use super::super::BondSpendFilter;
        use kaspa_consensus_core::{
            dns_finality::{ActiveBondView, BondStatus, DNS_PAYLOAD_VERSION_V1, STAKE_VALIDATOR_PUBKEY_LEN, StakeBondRecord},
            tx::TransactionOutpoint,
        };
        use kaspa_hashes::Hash64;

        const DAA: u64 = 10_000;

        fn outpoint(b: u8) -> TransactionOutpoint {
            TransactionOutpoint::new(Hash64::from_bytes([b; 64]), 0)
        }

        fn bond(op: TransactionOutpoint) -> StakeBondRecord {
            StakeBondRecord {
                version: DNS_PAYLOAD_VERSION_V1,
                bond_outpoint: op,
                owner_pubkey_hash: Hash64::from_bytes([0xaa; 64]),
                validator_pubkey_hash: Hash64::from_bytes([0xbb; 64]),
                validator_pubkey: vec![0xcc; STAKE_VALIDATOR_PUBKEY_LEN],
                amount: 1_000,
                activation_daa_score: 0,
                created_daa_score: 0,
                unbonding_period_blocks: 5_000,
                owner_reward_spk_payload: [0xdd; 64],
                unbond_request_daa_score: None,
                slashed_at_daa_score: None,
                status: BondStatus::Active,
            }
        }

        #[test]
        fn locks_active_and_pending_unlocks_releasable_and_non_bond() {
            let active = outpoint(1);
            let pending_op = outpoint(2);
            let mut pending = bond(pending_op);
            pending.activation_daa_score = DAA + 1; // Pending (not yet active).
            let releasable_op = outpoint(3);
            let mut releasable = bond(releasable_op);
            releasable.unbond_request_daa_score = Some(1_000); // release = 1_000 + 5_000 = 6_000 ≤ DAA.

            let view = ActiveBondView::from_records([
                (active, bond(active)),
                (pending_op, pending),
                (releasable_op, releasable),
            ]);
            let filter = BondSpendFilter { bond_view: &view, daa_score: DAA };

            assert!(filter.locks(&active), "Active bond's output-0 must be locked (skip the spend)");
            assert!(filter.locks(&pending_op), "Pending bond's output-0 must be locked");
            assert!(!filter.locks(&releasable_op), "a releasable (Unbonding past release) bond is spendable");
            assert!(!filter.locks(&outpoint(9)), "a non-bond outpoint is never locked");
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
                // PoS-v2 4-way fields: inert (2-way) in these apply-path tests.
                security_reserve_sompi: 0,
                victim_epoch_pool_sompi: 0,
                slashed_epoch: 0,
                victim_outputs: vec![],
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
        fn mints_victim_outputs_at_index_two_onward() {
            // PoS-v2 4-way: reporter minted at (tx,0), victim compensations at (tx,2),(tx,3) — index
            // 1 stays free — and the security-reserve share is NOT minted (it burns until Phase 4).
            let bond_op = bond_outpoint(0x06);
            let tx = slashing_tx_id(0x0f);
            let amount = 1_000u64;
            let entry = bond_entry(amount);

            let base: UtxoCollection = HashMap::from([(bond_op, entry.clone())]);
            let mut diff = UtxoDiff::new(HashMap::new(), HashMap::new());
            let mut multiset = multiset_of(&[(bond_op, entry.clone())]);

            // reporter 100, reserve 200 (unminted), victim pool 700 → two victim outputs 300 + 400.
            let mut eff = effect(bond_op, amount, 100, tx);
            eff.security_reserve_sompi = 200;
            eff.victim_epoch_pool_sompi = 700;
            eff.victim_outputs = vec![TransactionOutput::new(300, spk(0xc1)), TransactionOutput::new(400, spk(0xc2))];

            apply(&[eff], &base, &mut diff, &mut multiset, MINT_DAA);

            let r = (TransactionOutpoint::new(tx, 0), UtxoEntry::new(100, spk(0xee), MINT_DAA, false));
            let v1 = (TransactionOutpoint::new(tx, 2), UtxoEntry::new(300, spk(0xc1), MINT_DAA, false));
            let v2 = (TransactionOutpoint::new(tx, 3), UtxoEntry::new(400, spk(0xc2), MINT_DAA, false));

            // Bond removed; reporter + two victims minted; index 1 unused; reserve (200) NOT minted.
            assert_eq!(diff.remove.get(&bond_op), Some(&entry));
            assert_eq!(diff.add.len(), 3);
            assert_eq!(diff.add.get(&r.0), Some(&r.1));
            assert_eq!(diff.add.get(&v1.0), Some(&v1.1));
            assert_eq!(diff.add.get(&v2.0), Some(&v2.1));
            assert!(diff.add.get(&TransactionOutpoint::new(tx, 1)).is_none());
            // Commitment equals a set holding only reporter + victim mints (bond cancelled, reserve burned).
            assert_eq!(multiset.finalize(), multiset_of(&[r, v1, v2]).finalize());
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
