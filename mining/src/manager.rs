use crate::{
    MempoolCountersSnapshot, MiningCounters, P2pTxCountSample,
    block_template::{builder::BlockTemplateBuilder, errors::BuilderError},
    cache::BlockTemplateCache,
    errors::MiningManagerResult,
    feerate::{FeeEstimateVerbose, FeerateEstimations, FeerateEstimatorArgs},
    mempool::{
        Mempool,
        config::Config,
        model::tx::{MempoolTransaction, TransactionPostValidation, TransactionPreValidation, TxRemovalReason},
        populate_entries_and_try_validate::{
            populate_mempool_transactions_in_parallel, validate_mempool_transaction, validate_mempool_transactions_in_parallel,
        },
        tx::{Orphan, Priority, RbfPolicy},
    },
    model::{
        owner_txs::{GroupedOwnerTransactions, ScriptPublicKeySet},
        topological_sort::IntoIterTopologically,
        tx_insert::TransactionInsertion,
        tx_query::TransactionQuery,
    },
};
use itertools::Itertools;
use kaspa_consensus_core::{
    api::{
        ConsensusApi,
        args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    },
    block::{BlockTemplate, EvmClaimStaleKind, TemplateBuildMode, TemplateTransactionSelector},
    coinbase::MinerData,
    errors::{block::RuleError as BlockRuleError, tx::TxRuleError},
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutput},
};
use kaspa_consensusmanager::{ConsensusProxy, spawn_blocking};
use kaspa_core::{debug, error, info, time::{unix_now, Stopwatch}, warn};
use kaspa_mining_errors::{manager::MiningManagerError, mempool::RuleError};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub struct MiningManager {
    config: Arc<Config>,
    block_template_cache: BlockTemplateCache,
    mempool: RwLock<Mempool>,
    // kaspa-pq EVM Lane v0.4 (§15/§16): the EVM tx pool, SEPARATE from the UTXO
    // mempool (§14.1 budget isolation). Fills the node's own template payload.
    evm_mempool: RwLock<crate::evm_mempool::EvmMempool>,
    // §8.2 / §16: the miner's declared EVM coinbase (`--evm-fee-recipient`) —
    // claims the priority fees of this node's own payload txs on acceptance.
    // Zero (None) burns nothing but credits the zero address; set it on miners.
    evm_fee_recipient: Option<kaspa_consensus_core::evm::EvmAddress>,
    counters: Arc<MiningCounters>,
}

impl MiningManager {
    pub fn new(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
    ) -> Self {
        let config = Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass);
        Self::with_config(config, cache_lifetime, counters, None)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_extended_config(
        target_time_per_block: u64,
        relay_non_std_transactions: bool,
        max_block_mass: u64,
        ram_scale: f64,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
        // kaspa-pq PQ-only relay (audit Finding C): require ML-DSA-87 P2PKH for every mempool
        // output AND spent input, matching the PQ-only consensus rule. The production daemon passes
        // `true`; the `MiningManager::new` test path leaves it `false` (base-config default) so the
        // non-ML-DSA mempool unit tests keep exercising the upstream class table.
        pq_only: bool,
        // kaspa-pq EVM Lane v0.4 (§16): the miner's EVM coinbase (None = zero).
        evm_fee_recipient: Option<kaspa_consensus_core::evm::EvmAddress>,
    ) -> Self {
        let mut config =
            Config::build_default(target_time_per_block, relay_non_std_transactions, max_block_mass).apply_ram_scale(ram_scale);
        config.pq_only = pq_only;
        // kaspa-pq: the production node charges ≈100× a Kaspa transaction's fee (the ×10 relay rate
        // on top of the ~10× ML-DSA compute mass) to reconcile the ~72×-larger post-quantum
        // signature. The `MiningManager::new` test path keeps the upstream base rate so the mempool
        // unit fixtures stay calibrated.
        config.minimum_relay_transaction_fee = crate::mempool::config::PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE;
        Self::with_config(config, cache_lifetime, counters, evm_fee_recipient)
    }

    pub(crate) fn with_config(
        config: Config,
        cache_lifetime: Option<u64>,
        counters: Arc<MiningCounters>,
        evm_fee_recipient: Option<kaspa_consensus_core::evm::EvmAddress>,
    ) -> Self {
        let config = Arc::new(config);
        let mempool = RwLock::new(Mempool::new(config.clone(), counters.clone()));
        let block_template_cache = BlockTemplateCache::new(cache_lifetime);
        let evm_mempool = RwLock::new(crate::evm_mempool::EvmMempool::new());
        Self { config, block_template_cache, mempool, evm_mempool, evm_fee_recipient, counters }
    }

    /// kaspa-pq EVM Lane v0.4 (§16): admit a raw EIP-2718 EVM transaction into
    /// the EVM mempool. Admission applies EXACTLY the body-validation class-1
    /// rule (kaspa-evm `admit_tx_info`), so a pooled tx can never make the
    /// node's own template payload-block-invalid. Returns the Ethereum tx hash
    /// (keccak256 of the raw bytes).
    #[cfg(feature = "evm")]
    pub fn submit_evm_transaction(&self, raw: Vec<u8>) -> Result<kaspa_hashes::EvmH256, crate::evm_mempool::EvmMempoolError> {
        let info = kaspa_evm::tx::admit_tx_info(&raw).map_err(crate::evm_mempool::EvmMempoolError::Inadmissible)?;
        let now_secs = unix_now() / 1000;
        let result = {
            let mut pool = self.evm_mempool.write();
            pool.expire(now_secs);
            pool.insert(crate::evm_mempool::PendingEvmTx {
                hash: info.hash,
                sender: info.sender,
                nonce: info.nonce,
                gas_limit: info.gas_limit,
                max_fee_per_gas: info.max_fee_per_gas,
                max_priority_fee_per_gas: info.max_priority_fee_per_gas,
                raw,
                added_at: now_secs,
            })
        };
        // A newly admitted EVM tx changes the next template's payload even when the
        // virtual state (the template-cache key) is unchanged. Drop the cached
        // template so the next get_block_template rebuilds and includes it, instead
        // of serving a stale (payload-less) template for up to the cache lifetime —
        // which would delay first inclusion of a freshly submitted tx / burst.
        if result.is_ok() {
            self.block_template_cache.clear();
        }
        result
    }

    /// Non-`evm` builds cannot decode/admit EVM transactions (the lane is
    /// `u64::MAX`-inert on every default network; an evm-active net requires an
    /// `--features evm` node — the same refusal as the consensus seam).
    #[cfg(not(feature = "evm"))]
    pub fn submit_evm_transaction(&self, _raw: Vec<u8>) -> Result<kaspa_hashes::EvmH256, crate::evm_mempool::EvmMempoolError> {
        Err(crate::evm_mempool::EvmMempoolError::Inadmissible(
            "this kaspad was built without the `evm` feature — cannot admit EVM transactions".to_string(),
        ))
    }

    /// Snapshot of the pending EVM tx count (RPC/diagnostics).
    pub fn evm_mempool_len(&self) -> usize {
        self.evm_mempool.read().len()
    }

    /// The next nonce for `sender` accounting for this node's pending EVM txs
    /// (audit M-08, `eth_getTransactionCount(…,"pending")`). `state_nonce` is the
    /// chain (accepted) nonce; the result is `state_nonce` plus the contiguous
    /// pending run, so a wallet's back-to-back sends do not collide.
    pub fn evm_next_pending_nonce(&self, sender: kaspa_consensus_core::evm::EvmAddress, state_nonce: u64) -> u64 {
        self.evm_mempool.read().next_pending_nonce(&sender, state_nonce)
    }

    /// §9.2: queue a pre-resolved + pre-validated deposit claim for inclusion in
    /// this node's own template `system_ops`. The RPC layer (which holds the
    /// UTXO view) resolves the lock outpoint into the `DepositClaim` and checks
    /// it is currently claimable; the VSP template path re-validates against the
    /// live selected-parent view before committing. Returns `false` only when
    /// the claim queue is full. Always available (DepositClaim is a plain
    /// consensus type — no revm needed to QUEUE a claim).
    pub fn submit_evm_deposit_claim(&self, claim: kaspa_consensus_core::evm::DepositClaim) -> bool {
        self.evm_mempool.write().insert_claim(claim)
    }

    /// Whether a deposit claim for this lock outpoint is already queued.
    pub fn has_pending_evm_deposit_claim(&self, outpoint: &kaspa_consensus_core::tx::TransactionOutpoint) -> bool {
        self.evm_mempool.read().contains_claim(outpoint)
    }

    /// §14.2 relay: of `outpoints`, the ones NOT already queued (the request
    /// filter for incoming deposit-claim invs). Always available — a claim is a
    /// plain consensus type (no revm), so a non-`evm` build can still queue/serve.
    pub fn evm_unknown_deposit_claims(
        &self,
        outpoints: Vec<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        let pool = self.evm_mempool.read();
        outpoints.into_iter().filter(|o| !pool.contains_claim(o)).collect()
    }

    /// §14.2 relay: serve a queued deposit claim (typed) to a requesting peer.
    pub fn get_evm_deposit_claim(
        &self,
        outpoint: &kaspa_consensus_core::tx::TransactionOutpoint,
    ) -> Option<kaspa_consensus_core::evm::DepositClaim> {
        self.evm_mempool.read().get_claim(outpoint)
    }

    /// §14.2 relay: whether this build can run the class-1 admission precheck.
    /// A non-`evm` build must never REQUEST pending EVM txs (it could neither
    /// admit them nor fairly judge the sending peer), so the relay flow checks
    /// this before acting on an inv.
    pub fn supports_evm_admission(&self) -> bool {
        cfg!(feature = "evm")
    }

    /// §14.2 relay: of `tx_hashes`, the ones NOT already pending in the EVM
    /// mempool (the request filter for incoming EVM invs).
    pub fn evm_unknown_transactions(&self, tx_hashes: Vec<kaspa_hashes::EvmH256>) -> Vec<kaspa_hashes::EvmH256> {
        let pool = self.evm_mempool.read();
        tx_hashes.into_iter().filter(|h| !pool.contains(h)).collect()
    }

    /// §14.2 relay: serve a pending EVM tx's raw bytes to a requesting peer.
    pub fn get_evm_transaction_raw(&self, tx_hash: &kaspa_hashes::EvmH256) -> Option<Vec<u8>> {
        self.evm_mempool.read().get_raw(tx_hash)
    }

    /// §18.1 / §16: whether the given EVM tx is currently pending in this
    /// node's EVM mempool (the "pending" tier of the inclusion-status ladder).
    pub fn has_pending_evm_transaction(&self, tx_hash: &kaspa_hashes::EvmH256) -> bool {
        self.evm_mempool.read().contains(tx_hash)
    }

    /// Build this template's own EVM payload candidates: expire by TTL, prune
    /// already-accepted txs (nonce below the sender's canonical state nonce),
    /// then select per-sender CONTIGUOUS, byte- and declared-gas-capped,
    /// effective-tip-ordered runs (inclusion only — acceptance is a later block).
    ///
    /// On a hard consensus-read failure for the canonical state nonces, this
    /// emits an EMPTY EVM payload this template (+WARN) rather than selecting
    /// against a zeroed view. A zeroed view (the previous `unwrap_or_default()`)
    /// makes the selector hunt for nonce 0 and silently DROP every sender whose
    /// real nonce is higher — stranding every pending tx at
    /// `included_in=[] / last_skip_class=0` (payload starvation) until the read
    /// recovers, instead of harmlessly skipping the EVM payload for one block.
    /// The account-nonce + base-fee reads are skipped entirely while the pool is
    /// empty (the common case, and always so in a non-evm build).
    fn build_evm_template_data(&self, consensus: &dyn ConsensusApi) -> kaspa_consensus_core::evm::EvmTemplateData {
        use kaspa_consensus_core::evm::{
            EVM_INITIAL_BASE_FEE, MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK, MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK,
            MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK,
        };
        let mut pool = self.evm_mempool.write();
        pool.expire(unix_now() / 1000);
        let transactions = if pool.is_empty() {
            Vec::new()
        } else {
            let senders = pool.pending_senders();
            // Audit H-10: fetch the committed (nonce, balance) view in one snapshot read.
            // Balances let `select_candidates` skip senders that cannot pay a tx's
            // up-front gas reservation (a guaranteed class-2 skip — wasted payload slot).
            match consensus.get_evm_account_states(&senders) {
                Ok(states) => {
                    let state_nonces: HashMap<_, _> = states.iter().map(|(a, (n, _))| (*a, *n)).collect();
                    let state_balances: HashMap<_, _> = states.iter().map(|(a, (_, b))| (*a, *b)).collect();
                    pool.prune_below_state_nonce(&state_nonces);
                    // base fee from the SAME EVM head; absent head (early chain) or a
                    // read error => the genesis initial base fee (effective-tip ordering
                    // only — never a silent zero-by-error that would mis-rank tips).
                    let base_fee = match consensus.get_evm_head_header() {
                        Ok(Some(h)) => h.base_fee_per_gas.try_to_u128().unwrap_or(EVM_INITIAL_BASE_FEE as u128),
                        _ => EVM_INITIAL_BASE_FEE as u128,
                    };
                    // Keep the pool's eviction score aligned with this base fee (audit H-07).
                    pool.set_base_fee(base_fee);
                    pool.select_candidates(
                        MAX_EVM_PAYLOAD_BYTES_PER_DAG_BLOCK,
                        // A payload may not declare more gas than a chain block can accept.
                        MAX_EVM_ACCEPTED_GAS_PER_CHAIN_BLOCK,
                        base_fee,
                        &state_nonces,
                        Some(&state_balances),
                    )
                }
                Err(e) => {
                    warn!(
                        "EVM template: canonical state-nonce view unavailable ({e}); emitting an empty EVM payload this template (pending senders={}). \
                         Selecting against a zeroed nonce view would strand every higher-nonce sender at included_in=[]/last_skip_class=0.",
                        senders.len()
                    );
                    Vec::new()
                }
            }
        };
        kaspa_consensus_core::evm::EvmTemplateData {
            evm_coinbase: self.evm_fee_recipient.unwrap_or_default(),
            transactions,
            // §9.2: queued deposit claims for the own-payload system ops. The VSP
            // re-validates each against the live selected-parent view and drops
            // stale ones, so this over-approximates (cap = per-block consensus limit).
            system_ops: pool.select_claims(MAX_DEPOSIT_CLAIMS_PER_EVM_BLOCK),
        }
    }

    pub fn get_block_template(&self, consensus: &dyn ConsensusApi, miner_data: &MinerData) -> MiningManagerResult<BlockTemplate> {
        let virtual_state_approx_id = consensus.get_virtual_state_approx_id();
        let mut cache_lock = self.block_template_cache.lock(virtual_state_approx_id);
        let immutable_template = cache_lock.get_immutable_cached_template();

        // We first try and use a cached template if not expired
        if let Some(immutable_template) = immutable_template {
            drop(cache_lock);
            if immutable_template.miner_data == *miner_data {
                return Ok(immutable_template.as_ref().clone());
            }
            // Miner data is new -- make the minimum changes required
            // Note the call returns a modified clone of the cached block template
            let block_template = BlockTemplateBuilder::modify_block_template(consensus, miner_data, &immutable_template)?;

            // No point in updating cache since we have no reason to believe this coinbase will be used more
            // than the previous one, and we want to maintain the original template caching time
            return Ok(block_template);
        }

        // Rust rewrite:
        // We avoid passing a mempool ref to blockTemplateBuilder by calling
        // mempool.BlockCandidateTransactions and mempool.RemoveTransactions here.
        // We remove recursion seen in blockTemplateBuilder.BuildBlockTemplate here.
        debug!("Building a new block template...");
        let _swo = Stopwatch::<22>::with_threshold("build_block_template full loop");
        let mut attempts: u64 = 0;
        loop {
            attempts += 1;
            // kaspa-pq EVM Lane v0.4 (§15 step 6): the node's own EVM payload
            // candidates, RE-selected per build attempt (P0-2) — a retry after the
            // virtual state advanced re-reads the live canonical nonces instead of
            // reusing stale candidates. Inclusion only (acceptance is a later block).
            let evm_template_data = self.build_evm_template_data(consensus);

            let selector = self.build_selector();
            let block_template_builder = BlockTemplateBuilder::new();
            let build_mode = if attempts < self.config.maximum_build_block_template_attempts {
                TemplateBuildMode::Standard
            } else {
                TemplateBuildMode::Infallible
            };
            match block_template_builder.build_block_template(consensus, miner_data, selector, build_mode, evm_template_data.clone()) {
                Ok(block_template) => {
                    // §9.2: reconcile the claim queue with what the template path found
                    // when it re-validated each SELECTED claim against the LIVE claim view.
                    //  - Invalid (lock present but unclaimable: refund-aged / field
                    //    mismatch) → terminal, evict the queue entry at once.
                    //  - Absent (lock not yet on this node's selected chain — a lagging
                    //    miner or a forky DAG — or just consumed) → usually transient, so
                    //    KEEP + retry; evict only after it stays absent for
                    //    MAX_CLAIM_ABSENT_STRIKES consecutive templates (≈ blocks), which
                    //    reaps a consumed / never-confirmed lock without dropping a claim
                    //    whose deposit-lock is merely still being buried.
                    //  - Claims that DID make it into the template have their lock present
                    //    again → reset their consecutive-absent run.
                    if !evm_template_data.system_ops.is_empty() {
                        let stale: std::collections::HashSet<_> =
                            block_template.stale_evm_claims.iter().map(|(op, _)| *op).collect();
                        let mut pool = self.evm_mempool.write();
                        for claim in &evm_template_data.system_ops {
                            if !stale.contains(&claim.deposit_outpoint) {
                                pool.note_claim_present(&claim.deposit_outpoint);
                            }
                        }
                        for (outpoint, kind) in &block_template.stale_evm_claims {
                            match kind {
                                EvmClaimStaleKind::Invalid => {
                                    pool.remove_claim(outpoint);
                                }
                                EvmClaimStaleKind::Absent => {
                                    if pool.note_claim_absent(outpoint) {
                                        warn!(
                                            "EVM: evicting deposit claim {outpoint} — its lock stayed absent from the selected chain for {} consecutive templates; re-submit submitEvmDepositClaim once the deposit-lock tx is confirmed on the selected chain",
                                            crate::evm_mempool::MAX_CLAIM_ABSENT_STRIKES
                                        );
                                        pool.remove_claim(outpoint);
                                    }
                                }
                            }
                        }
                    }
                    let block_template = cache_lock.set_immutable_cached_template(block_template);
                    match attempts {
                        1 => {
                            debug!(
                                "Built a new block template with {} transactions in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        2 => {
                            debug!(
                                "Built a new block template with {} transactions at second attempt in {:#?}",
                                block_template.block.transactions.len(),
                                _swo.elapsed()
                            );
                        }
                        n => {
                            debug!(
                                "Built a new block template with {} transactions in {} attempts totaling {:#?}",
                                block_template.block.transactions.len(),
                                n,
                                _swo.elapsed()
                            );
                        }
                    }
                    return Ok(block_template.as_ref().clone());
                }
                Err(BuilderError::ConsensusError(BlockRuleError::InvalidTransactionsInNewBlock(invalid_transactions))) => {
                    let mut missing_outpoint: usize = 0;
                    let mut invalid: usize = 0;

                    let mut mempool_write = self.mempool.write();
                    invalid_transactions.iter().for_each(|(x, err)| {
                        // On missing outpoints, the most likely is that the tx was already in a block accepted by
                        // the consensus but not yet processed by handle_new_block_transactions(). Another possibility
                        // is a double spend. In both cases, we simply remove the transaction but keep its redeemers.
                        // Those will either be valid in a next block template or invalidated if it's a double spend.
                        //
                        // If the redeemers of a transaction accepted in consensus but not yet handled in mempool were
                        // removed, it would lead to having subsequently submitted children transactions of the removed
                        // redeemers being unexpectedly either orphaned or rejected in case orphans are disallowed.
                        //
                        // For all other errors, we do remove the redeemers.

                        let removal_result = if *err == TxRuleError::MissingTxOutpoints {
                            missing_outpoint += 1;
                            mempool_write.remove_transaction(x, false, TxRemovalReason::Muted, "")
                        } else {
                            invalid += 1;
                            warn!("Remove per BBT invalid transaction and descendants");
                            mempool_write.remove_transaction(
                                x,
                                true,
                                TxRemovalReason::InvalidInBlockTemplate,
                                format!(" error: {}", err).as_str(),
                            )
                        };
                        if let Err(err) = removal_result {
                            // Original golang comment:
                            // mempool.remove_transactions might return errors in situations that are perfectly fine in this context.
                            // TODO: Once the mempool invariants are clear, this might return an error:
                            // https://github.com/kaspanet/kaspad/issues/1553
                            // NOTE: unlike golang, here we continue removing also if an error was found
                            error!("Error from mempool.remove_transactions: {:?}", err);
                        }
                    });
                    drop(mempool_write);

                    debug!(
                        "Building a new block template failed for {} txs missing outpoint and {} invalid txs",
                        missing_outpoint, invalid
                    );
                }
                Err(err) => {
                    warn!("Building a new block template failed: {}", err);
                    return Err(err)?;
                }
            }
        }
    }

    /// Dynamically builds a transaction selector based on the specific state of the ready transactions frontier
    pub(crate) fn build_selector(&self) -> Box<dyn TemplateTransactionSelector> {
        self.mempool.read().build_selector()
    }

    /// Returns realtime feerate estimations based on internal mempool state
    pub(crate) fn get_realtime_feerate_estimations(&self) -> FeerateEstimations {
        let args = FeerateEstimatorArgs::new(self.config.network_blocks_per_second, self.config.maximum_mass_per_block);
        let estimator = self.mempool.read().build_feerate_estimator(args);
        estimator.calc_estimations(self.config.minimum_feerate())
    }

    /// Returns realtime feerate estimations based on internal mempool state with additional verbose data
    pub(crate) fn get_realtime_feerate_estimations_verbose(
        &self,
        consensus: &dyn ConsensusApi,
        prefix: kaspa_addresses::Prefix,
    ) -> MiningManagerResult<FeeEstimateVerbose> {
        let args = FeerateEstimatorArgs::new(self.config.network_blocks_per_second, self.config.maximum_mass_per_block);
        let network_mass_per_second = args.network_mass_per_second();
        let mempool_read = self.mempool.read();
        let estimator = mempool_read.build_feerate_estimator(args);
        let ready_transactions_count = mempool_read.ready_transaction_count();
        let ready_transaction_total_mass = mempool_read.ready_transaction_total_mass();
        drop(mempool_read);
        let mut resp = FeeEstimateVerbose {
            estimations: estimator.calc_estimations(self.config.minimum_feerate()),
            network_mass_per_second,
            mempool_ready_transactions_count: ready_transactions_count as u64,
            mempool_ready_transactions_total_mass: ready_transaction_total_mass,

            next_block_template_feerate_min: -1.0,
            next_block_template_feerate_median: -1.0,
            next_block_template_feerate_max: -1.0,
        };
        // calculate next_block_template_feerate_xxx
        {
            // kaspa-pq PQ-only: use an ML-DSA-87 P2PKH placeholder (the only standard class) so
            // this fee-estimate template's coinbase miner script matches the consensus PQ rule —
            // a legacy `PubKey` placeholder would build a non-PQ coinbase payload script that the
            // PQ-only invariant (incl. the coinbase-payload check) rejects.
            let script_public_key = kaspa_txscript::pay_to_address_script(&kaspa_addresses::Address::new(
                prefix,
                kaspa_addresses::Version::PubKeyHashMlDsa87,
                &[0u8; 64],
            ));
            let miner_data: MinerData = MinerData::new(script_public_key, vec![]);

            let BlockTemplate { block: kaspa_consensus_core::block::MutableBlock { transactions, .. }, calculated_fees, .. } =
                self.get_block_template(consensus, &miner_data)?;

            let Some(Stats { max, median, min }) = feerate_stats(transactions, calculated_fees) else {
                return Ok(resp);
            };

            resp.next_block_template_feerate_max = max;
            resp.next_block_template_feerate_min = min;
            resp.next_block_template_feerate_median = median;
        }
        Ok(resp)
    }

    /// Clears the block template cache, forcing the next call to get_block_template to build a new block template.
    #[cfg(test)]
    pub(crate) fn clear_block_template(&self) {
        self.block_template_cache.clear();
    }

    #[cfg(test)]
    pub(crate) fn block_template_builder(&self) -> BlockTemplateBuilder {
        BlockTemplateBuilder::new()
    }

    /// validate_and_insert_transaction validates the given transaction, and
    /// adds it to the set of known transactions that have not yet been
    /// added to any block.
    ///
    /// The validation is constrained by a Replace by fee policy applied
    /// to double spends in the mempool. For more information, see [`RbfPolicy`].
    ///
    /// On success, returns transactions that where unorphaned following the insertion
    /// of the provided transaction.
    ///
    /// The returned transactions are references of objects owned by the mempool.
    pub fn validate_and_insert_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: Transaction,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> MiningManagerResult<TransactionInsertion> {
        self.validate_and_insert_mutable_transaction(consensus, MutableTransaction::from_tx(transaction), priority, orphan, rbf_policy)
    }

    /// Exposed for tests only
    ///
    /// See `validate_and_insert_transaction`
    pub(crate) fn validate_and_insert_mutable_transaction(
        &self,
        consensus: &dyn ConsensusApi,
        transaction: MutableTransaction,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> MiningManagerResult<TransactionInsertion> {
        // read lock on mempool
        let TransactionPreValidation { mut transaction, feerate_threshold } =
            self.mempool.read().pre_validate_and_populate_transaction(consensus, transaction, rbf_policy)?;
        let args = TransactionValidationArgs::new(feerate_threshold);
        // no lock on mempool
        let validation_result = validate_mempool_transaction(consensus, &mut transaction, &args);
        // write lock on mempool
        let mut mempool = self.mempool.write();
        match mempool.post_validate_and_insert_transaction(consensus, validation_result, transaction, priority, orphan, rbf_policy)? {
            TransactionPostValidation { removed, accepted: Some(accepted_transaction) } => {
                let unorphaned_transactions = mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction);
                drop(mempool);

                // The capacity used here may be exceeded since accepted unorphaned transaction may themselves unorphan other transactions.
                let mut accepted_transactions = Vec::with_capacity(unorphaned_transactions.len() + 1);
                // We include the original accepted transaction as well
                accepted_transactions.push(accepted_transaction);
                accepted_transactions.extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions));
                self.counters.increase_tx_counts(1, priority);

                Ok(TransactionInsertion::new(removed, accepted_transactions))
            }
            TransactionPostValidation { removed, accepted: None } => Ok(TransactionInsertion::new(removed, vec![])),
        }
    }

    fn validate_and_insert_unorphaned_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        mut incoming_transactions: Vec<MempoolTransaction>,
    ) -> Vec<Arc<Transaction>> {
        // The capacity used here may be exceeded (see next comment).
        let mut accepted_transactions = Vec::with_capacity(incoming_transactions.len());
        // The validation args map is immutably empty since unorphaned transactions do not require pre processing so there
        // are no feerate thresholds to use. Instead, we rely on this being checked during post processing.
        let args = TransactionValidationBatchArgs::new();
        // We loop as long as incoming unorphaned transactions do unorphan other transactions when they
        // get validated and inserted into the mempool.
        while !incoming_transactions.is_empty() {
            // Since the consensus validation requires a slice of MutableTransaction, we destructure the vector of
            // MempoolTransaction into 2 distinct vectors holding respectively the needed MutableTransaction and Priority.
            let (mut transactions, priorities): (Vec<MutableTransaction>, Vec<Priority>) =
                incoming_transactions.into_iter().map(|x| (x.mtx, x.priority)).unzip();

            // no lock on mempool
            // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
            let mut lower_bound: usize = 0;
            let mut validation_results = Vec::with_capacity(transactions.len());
            while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
                assert!(lower_bound < upper_bound, "the chunk is never empty");
                validation_results.extend(validate_mempool_transactions_in_parallel(
                    consensus,
                    &mut transactions[lower_bound..upper_bound],
                    &args,
                ));
                lower_bound = upper_bound;
            }
            assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

            // write lock on mempool
            let mut mempool = self.mempool.write();
            incoming_transactions = transactions
                .into_iter()
                .zip(priorities)
                .zip(validation_results)
                .flat_map(|((transaction, priority), validation_result)| {
                    let orphan_id = transaction.id();
                    let rbf_policy = Mempool::get_orphan_transaction_rbf_policy(priority);
                    match mempool.post_validate_and_insert_transaction(
                        consensus,
                        validation_result,
                        transaction,
                        priority,
                        Orphan::Forbidden,
                        rbf_policy,
                    ) {
                        Ok(TransactionPostValidation { removed: _, accepted: Some(accepted_transaction) }) => {
                            accepted_transactions.push(accepted_transaction.clone());
                            self.counters.increase_tx_counts(1, priority);
                            mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction)
                        }
                        Ok(TransactionPostValidation { removed: _, accepted: None }) => vec![],
                        Err(err) => {
                            debug!("Failed to unorphan transaction {0} due to rule error: {1}", orphan_id, err);
                            vec![]
                        }
                    }
                })
                .collect::<Vec<_>>();
            drop(mempool);
        }
        accepted_transactions
    }

    /// Validates a batch of transactions, handling iteratively only the independent ones, and
    /// adds those to the set of known transactions that have not yet been added to any block.
    ///
    /// The validation is constrained by a Replace by fee policy applied
    /// to double spends in the mempool. For more information, see [`RbfPolicy`].
    ///
    /// Returns transactions that where unorphaned following the insertion of the provided
    /// transactions. The returned transactions are references of objects owned by the mempool.
    pub fn validate_and_insert_transaction_batch(
        &self,
        consensus: &dyn ConsensusApi,
        transactions: Vec<Transaction>,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> Vec<MiningManagerResult<Arc<Transaction>>> {
        const TRANSACTION_CHUNK_SIZE: usize = 250;

        // The capacity used here may be exceeded since accepted transactions may unorphan other transactions.
        let mut insert_results: Vec<MiningManagerResult<Arc<Transaction>>> = Vec::with_capacity(transactions.len());
        let mut unorphaned_transactions = vec![];
        let _swo = Stopwatch::<80>::with_threshold("validate_and_insert_transaction_batch topological_sort op");
        let sorted_transactions = transactions.into_iter().map(MutableTransaction::from_tx).topological_into_iter();
        drop(_swo);

        // read lock on mempool
        // Here, we simply log and drop all erroneous transactions since the caller doesn't care about those anyway
        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        let mut args = TransactionValidationBatchArgs::new();
        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            let txs = chunk.filter_map(|tx| {
                let transaction_id = tx.id();
                match mempool.pre_validate_and_populate_transaction(consensus, tx, rbf_policy) {
                    Ok(TransactionPreValidation { transaction, feerate_threshold }) => {
                        if let Some(threshold) = feerate_threshold {
                            args.set_feerate_threshold(transaction.id(), threshold);
                        }
                        Some(transaction)
                    }
                    Err(RuleError::RejectAlreadyAccepted(transaction_id)) => {
                        debug!("Ignoring already accepted transaction {}", transaction_id);
                        None
                    }
                    Err(RuleError::RejectDuplicate(transaction_id)) => {
                        debug!("Ignoring transaction already in the mempool {}", transaction_id);
                        None
                    }
                    Err(RuleError::RejectDuplicateOrphan(transaction_id)) => {
                        debug!("Ignoring transaction already in the orphan pool {}", transaction_id);
                        None
                    }
                    Err(err) => {
                        debug!("Failed to pre validate transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                        None
                    }
                }
            });
            transactions.extend(txs);
        }

        // no lock on mempool
        // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            validation_results.extend(validate_mempool_transactions_in_parallel(
                consensus,
                &mut transactions[lower_bound..upper_bound],
                &args,
            ));
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        // write lock on mempool
        // Here again, transactions failing post validation are logged and dropped
        for chunk in &transactions.into_iter().zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut mempool = self.mempool.write();
            let txs = chunk.flat_map(|(transaction, validation_result)| {
                let transaction_id = transaction.id();
                match mempool.post_validate_and_insert_transaction(
                    consensus,
                    validation_result,
                    transaction,
                    priority,
                    orphan,
                    rbf_policy,
                ) {
                    Ok(TransactionPostValidation { removed: _, accepted: Some(accepted_transaction) }) => {
                        insert_results.push(Ok(accepted_transaction.clone()));
                        self.counters.increase_tx_counts(1, priority);
                        mempool.get_unorphaned_transactions_after_accepted_transaction(&accepted_transaction)
                    }
                    Ok(TransactionPostValidation { removed: _, accepted: None }) | Err(RuleError::RejectDuplicate(_)) => {
                        // Either orphaned or already existing in the mempool
                        vec![]
                    }
                    Err(err) => {
                        debug!("Failed to post validate transaction {0} due to rule error: {1}", transaction_id, err);
                        insert_results.push(Err(MiningManagerError::MempoolError(err)));
                        vec![]
                    }
                }
            });
            unorphaned_transactions.extend(txs);
        }

        insert_results
            .extend(self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions).into_iter().map(Ok));
        insert_results
    }

    fn next_transaction_chunk_upper_bound(&self, transactions: &[MutableTransaction], lower_bound: usize) -> Option<usize> {
        if lower_bound >= transactions.len() {
            return None;
        }
        let mut mass = 0;
        transactions[lower_bound..]
            .iter()
            .position(|tx| {
                mass += tx.calculated_non_contextual_masses.unwrap().max();
                mass >= self.config.maximum_mass_per_block
            })
            // Make sure the upper bound is greater than the lower bound, allowing to handle a very unlikely,
            // (if not impossible) case where the mass of a single transaction is greater than the maximum
            // chunk mass.
            .map(|relative_index| relative_index.max(1) + lower_bound)
            .or(Some(transactions.len()))
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> Option<MutableTransaction> {
        self.mempool.read().get_transaction(transaction_id, query)
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub fn has_transaction(&self, transaction_id: &TransactionId, query: TransactionQuery) -> bool {
        self.mempool.read().has_transaction(transaction_id, query)
    }

    pub fn get_all_transactions(&self, query: TransactionQuery) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;
        // read lock on mempool by transaction chunks
        let transactions = if query.include_transaction_pool() {
            let transaction_ids = self.mempool.read().get_all_transaction_ids(TransactionQuery::TransactionsOnly).0;
            let mut transactions = Vec::with_capacity(self.mempool.read().transaction_count(TransactionQuery::TransactionsOnly));
            for chunks in transaction_ids.chunks(TRANSACTION_CHUNK_SIZE) {
                let mempool = self.mempool.read();
                transactions.extend(chunks.iter().filter_map(|x| mempool.get_transaction(x, TransactionQuery::TransactionsOnly)));
            }
            transactions
        } else {
            vec![]
        };
        // read lock on mempool
        let orphans = if query.include_orphan_pool() {
            self.mempool.read().get_all_transactions(TransactionQuery::OrphansOnly).1
        } else {
            vec![]
        };
        (transactions, orphans)
    }

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub fn get_transactions_by_addresses(
        &self,
        script_public_keys: &ScriptPublicKeySet,
        query: TransactionQuery,
    ) -> GroupedOwnerTransactions {
        // TODO: break the monolithic lock
        self.mempool.read().get_transactions_by_addresses(script_public_keys, query)
    }

    pub fn transaction_count(&self, query: TransactionQuery) -> usize {
        self.mempool.read().transaction_count(query)
    }

    pub fn handle_new_block_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        block_daa_score: u64,
        block_transactions: &[Transaction],
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        // TODO: should use tx acceptance data to verify that new block txs are actually accepted into virtual state.
        // TODO: avoid returning a result from this function (and the underlying function). Any possible error is a
        // problem of the internal implementation and unrelated to the caller

        // write lock on mempool
        let unorphaned_transactions = self.mempool.write().handle_new_block_transactions(block_daa_score, block_transactions)?;

        // alternate no & write lock on mempool
        let accepted_transactions = self.validate_and_insert_unorphaned_transactions(consensus, unorphaned_transactions);

        Ok(accepted_transactions)
    }

    pub fn expire_low_priority_transactions(&self, consensus: &dyn ConsensusApi) {
        // very fine-grained write locks on mempool
        debug!("<> Expiring low priority transactions...");

        // orphan pool
        if let Err(err) = self.mempool.write().expire_orphan_low_priority_transactions(consensus) {
            warn!("Failed to expire transactions from orphan pool: {}", err);
        }

        // accepted transaction cache
        self.mempool.write().expire_accepted_transactions(consensus);

        // mempool
        let expired_low_priority_transactions = self.mempool.write().collect_expired_low_priority_transactions(consensus);
        for chunk in &expired_low_priority_transactions.iter().chunks(24) {
            let mut mempool = self.mempool.write();
            chunk.into_iter().for_each(|tx| {
                if let Err(err) = mempool.remove_transaction(tx, true, TxRemovalReason::Muted, "") {
                    warn!("Failed to remove transaction {} from mempool: {}", tx, err);
                }
            });
        }
        match expired_low_priority_transactions.len() {
            0 => {}
            1 => debug!("Removed transaction ({}) {}", TxRemovalReason::Expired, expired_low_priority_transactions[0]),
            n => debug!("Removed {} transactions ({}): {}...", n, TxRemovalReason::Expired, expired_low_priority_transactions[0]),
        }
    }

    pub fn revalidate_high_priority_transactions(
        &self,
        consensus: &dyn ConsensusApi,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        const TRANSACTION_CHUNK_SIZE: usize = 1000;

        // read lock on mempool
        // Prepare a vector with clones of high priority transactions found in the mempool
        let mempool = self.mempool.read();
        let transaction_ids = mempool.all_transaction_ids_with_priority(Priority::High);
        if transaction_ids.is_empty() {
            debug!("<> Revalidating high priority transactions found no transactions");
            return;
        } else {
            debug!("<> Revalidating {} high priority transactions...", transaction_ids.len());
        }
        drop(mempool);
        // read lock on mempool by transaction chunks
        let mut transactions = Vec::with_capacity(transaction_ids.len());
        for chunk in &transaction_ids.iter().chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            transactions.extend(chunk.filter_map(|x| mempool.get_transaction(x, TransactionQuery::TransactionsOnly)));
        }

        let mut valid: usize = 0;
        let mut accepted: usize = 0;
        let mut other: usize = 0;
        let mut missing_outpoint: usize = 0;
        let mut invalid: usize = 0;

        // We process the transactions by level of dependency inside the batch.
        // Doing so allows to remove all chained dependencies of rejected transactions.
        let _swo = Stopwatch::<800>::with_threshold("revalidate topological_sort op");
        let sorted_transactions = transactions.topological_into_iter();
        drop(_swo);

        // read lock on mempool by transaction chunks
        // As the revalidation process is no longer atomic, we filter the transactions ready for revalidation,
        // keeping only the ones actually present in the mempool (see comment above).
        let _swo = Stopwatch::<900>::with_threshold("revalidate populate_mempool_entries op");
        let mut transactions = Vec::with_capacity(sorted_transactions.len());
        for chunk in &sorted_transactions.chunks(TRANSACTION_CHUNK_SIZE) {
            let mempool = self.mempool.read();
            let txs = chunk.filter_map(|mut x| {
                let transaction_id = x.id();
                if mempool.has_accepted_transaction(&transaction_id) {
                    accepted += 1;
                    None
                } else if mempool.has_transaction(&transaction_id, TransactionQuery::TransactionsOnly) {
                    x.clear_entries();
                    mempool.populate_mempool_entries(&mut x);
                    match x.is_fully_populated() {
                        false => Some(x),
                        true => {
                            // If all entries are populated with mempool UTXOs, we already know the transaction is valid
                            valid += 1;
                            None
                        }
                    }
                } else {
                    other += 1;
                    None
                }
            });
            transactions.extend(txs);
        }
        drop(_swo);

        // no lock on mempool
        // We process the transactions by chunks of max block mass to prevent locking the virtual processor for too long.
        let mut lower_bound: usize = 0;
        let mut validation_results = Vec::with_capacity(transactions.len());
        while let Some(upper_bound) = self.next_transaction_chunk_upper_bound(&transactions, lower_bound) {
            assert!(lower_bound < upper_bound, "the chunk is never empty");
            let _swo = Stopwatch::<60>::with_threshold("revalidate validate_mempool_transactions_in_parallel op");
            validation_results
                .extend(populate_mempool_transactions_in_parallel(consensus, &mut transactions[lower_bound..upper_bound]));
            drop(_swo);
            lower_bound = upper_bound;
        }
        assert_eq!(transactions.len(), validation_results.len(), "every transaction should have a matching validation result");

        // write lock on mempool
        // Depending on the validation result, transactions are either accepted or removed
        for chunk in &transactions.into_iter().zip(validation_results).chunks(TRANSACTION_CHUNK_SIZE) {
            let mut valid_ids = Vec::with_capacity(TRANSACTION_CHUNK_SIZE);
            let mut mempool = self.mempool.write();
            let _swo = Stopwatch::<60>::with_threshold("revalidate update_revalidated_transaction op");
            for (transaction, validation_result) in chunk {
                let transaction_id = transaction.id();
                match validation_result {
                    Ok(()) => {
                        // Only consider transactions still being in the mempool since during the validation some might have been removed.
                        if mempool.update_revalidated_transaction(transaction) {
                            // A following transaction should not remove this one from the pool since we process in a topological order.
                            // Still, considering the (very unlikely) scenario of two high priority txs sandwiching a low one, where
                            // in this case topological order is not guaranteed since we only considered chained dependencies of
                            // high-priority transactions, we might wrongfully return as valid the id of a removed transaction.
                            // However, as only consequence, said transaction would then be advertised to registered peers and not be
                            // provided upon request.
                            valid_ids.push(transaction_id);
                            valid += 1;
                        } else {
                            other += 1;
                        }
                    }
                    Err(RuleError::RejectMissingOutpoint) => {
                        let missing_txs = transaction
                            .entries
                            .iter()
                            .zip(transaction.tx.inputs.iter())
                            .filter_map(|(entry, input)| entry.is_none().then_some(input.previous_outpoint.transaction_id))
                            .collect::<Vec<_>>();

                        // A transaction may have missing outpoints for legitimate reasons related to concurrency, like a race condition between
                        // an accepted block having not started yet or unfinished call to handle_new_block_transactions but already processed by
                        // the consensus and this ongoing call to revalidate.
                        //
                        // So we only remove the transaction and keep its redeemers in the mempool because we cannot be sure they are invalid, in
                        // fact in the race condition case they are valid regarding outpoints.
                        let extra_info = match missing_txs.len() {
                            0 => " but no missing tx!".to_string(), // this is never supposed to happen
                            1 => format!(" missing tx {}", missing_txs[0]),
                            n => format!(" with {} missing txs {}..{}", n, missing_txs[0], missing_txs.last().unwrap()),
                        };

                        // This call cleanly removes the invalid transaction.
                        _ = mempool
                            .remove_transaction(
                                &transaction_id,
                                false,
                                TxRemovalReason::RevalidationWithMissingOutpoints,
                                extra_info.as_str(),
                            )
                            .inspect_err(|err| warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err));
                        missing_outpoint += 1;
                    }
                    Err(err) => {
                        // Rust rewrite note:
                        // The behavior changes here compared to the golang version.
                        // The failed revalidation is simply logged and the process continues.
                        warn!(
                            "Removing high priority transaction {0} and its redeemers, it failed revalidation with {1}",
                            transaction_id, err
                        );
                        // This call cleanly removes the invalid transaction and its redeemers.
                        _ = mempool
                            .remove_transaction(&transaction_id, true, TxRemovalReason::Muted, "")
                            .inspect_err(|err| warn!("Failed to remove transaction {} from mempool: {}", transaction_id, err));
                        invalid += 1;
                    }
                }
            }
            if !valid_ids.is_empty() {
                let _ = transaction_ids_sender.send(valid_ids);
            }
            drop(_swo);
            drop(mempool);
        }
        match accepted + missing_outpoint + invalid {
            0 => {
                info!("Revalidated {} high priority transactions", valid);
            }
            _ => {
                info!(
                    "Revalidated {} and removed {} high priority transactions (removals: {} accepted, {} missing outpoint, {} invalid)",
                    valid,
                    accepted + missing_outpoint + invalid,
                    accepted,
                    missing_outpoint,
                    invalid,
                );
                if other > 0 {
                    debug!(
                        "During revalidation of high priority transactions {} txs were removed from the mempool by concurrent flows",
                        other
                    )
                }
            }
        }
    }

    /// is_transaction_output_dust returns whether or not the passed transaction output
    /// amount is considered dust or not based on the configured minimum transaction
    /// relay fee.
    ///
    /// Dust is defined in terms of the minimum transaction relay fee. In particular,
    /// if the cost to the network to spend coins is more than 1/3 of the minimum
    /// transaction relay fee, it is considered dust.
    pub fn is_transaction_output_dust(&self, transaction_output: &TransactionOutput) -> bool {
        self.mempool.read().is_transaction_output_dust(transaction_output)
    }

    pub fn has_accepted_transaction(&self, transaction_id: &TransactionId) -> bool {
        self.mempool.read().has_accepted_transaction(transaction_id)
    }

    pub fn unaccepted_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unaccepted_transactions(transactions)
    }

    pub fn unknown_transactions(&self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        self.mempool.read().unknown_transactions(transactions)
    }

    #[cfg(test)]
    pub(crate) fn get_estimated_size(&self) -> usize {
        self.mempool.read().get_estimated_size()
    }
}

/// Async proxy for the mining manager
#[derive(Clone)]
pub struct MiningManagerProxy {
    inner: Arc<MiningManager>,
}

impl MiningManagerProxy {
    pub fn new(inner: Arc<MiningManager>) -> Self {
        Self { inner }
    }

    pub async fn get_block_template(self, consensus: &ConsensusProxy, miner_data: MinerData) -> MiningManagerResult<BlockTemplate> {
        consensus.clone().spawn_blocking(move |c| self.inner.get_block_template(c, &miner_data)).await
    }

    /// Returns realtime feerate estimations based on internal mempool state
    /// kaspa-pq EVM Lane v0.4 (§16): admit a raw EIP-2718 EVM tx into the EVM
    /// mempool (sync + cheap: decode + k256 recovery + pool insert).
    pub fn submit_evm_transaction(&self, raw: Vec<u8>) -> Result<kaspa_hashes::EvmH256, crate::evm_mempool::EvmMempoolError> {
        self.inner.submit_evm_transaction(raw)
    }

    /// §14.2 relay: whether this build can run the class-1 admission precheck.
    pub fn supports_evm_admission(&self) -> bool {
        self.inner.supports_evm_admission()
    }

    /// §14.2 relay: filter out EVM tx hashes already pending in the EVM mempool.
    pub fn evm_unknown_transactions(&self, tx_hashes: Vec<kaspa_hashes::EvmH256>) -> Vec<kaspa_hashes::EvmH256> {
        self.inner.evm_unknown_transactions(tx_hashes)
    }

    /// §14.2 relay: serve a pending EVM tx's raw bytes to a requesting peer.
    pub fn get_evm_transaction_raw(&self, tx_hash: &kaspa_hashes::EvmH256) -> Option<Vec<u8>> {
        self.inner.get_evm_transaction_raw(tx_hash)
    }

    /// Audit M-08: the next nonce for `eth_getTransactionCount(…,"pending")` —
    /// chain nonce + this node's contiguous pending EVM txs for the account.
    pub fn evm_next_pending_nonce(&self, sender: kaspa_consensus_core::evm::EvmAddress, state_nonce: u64) -> u64 {
        self.inner.evm_next_pending_nonce(sender, state_nonce)
    }

    /// §18.1: whether the given EVM tx is pending in this node's EVM mempool.
    pub fn has_pending_evm_transaction(&self, tx_hash: &kaspa_hashes::EvmH256) -> bool {
        self.inner.has_pending_evm_transaction(tx_hash)
    }

    /// §9.2: queue a pre-validated deposit claim for the own-payload system ops.
    pub fn submit_evm_deposit_claim(&self, claim: kaspa_consensus_core::evm::DepositClaim) -> bool {
        self.inner.submit_evm_deposit_claim(claim)
    }

    /// §9.2: whether a deposit claim for this lock outpoint is already queued.
    pub fn has_pending_evm_deposit_claim(&self, outpoint: &kaspa_consensus_core::tx::TransactionOutpoint) -> bool {
        self.inner.has_pending_evm_deposit_claim(outpoint)
    }

    /// §14.2 relay: of `outpoints`, the ones NOT already queued (claim-inv request filter).
    pub fn evm_unknown_deposit_claims(
        &self,
        outpoints: Vec<kaspa_consensus_core::tx::TransactionOutpoint>,
    ) -> Vec<kaspa_consensus_core::tx::TransactionOutpoint> {
        self.inner.evm_unknown_deposit_claims(outpoints)
    }

    /// §14.2 relay: serve a queued deposit claim (typed) to a requesting peer.
    pub fn get_evm_deposit_claim(
        &self,
        outpoint: &kaspa_consensus_core::tx::TransactionOutpoint,
    ) -> Option<kaspa_consensus_core::evm::DepositClaim> {
        self.inner.get_evm_deposit_claim(outpoint)
    }

    pub async fn get_realtime_feerate_estimations(self) -> FeerateEstimations {
        spawn_blocking(move || self.inner.get_realtime_feerate_estimations()).await.unwrap()
    }

    /// Returns realtime feerate estimations based on internal mempool state with additional verbose data
    pub async fn get_realtime_feerate_estimations_verbose(
        self,
        consensus: &ConsensusProxy,
        prefix: kaspa_addresses::Prefix,
    ) -> MiningManagerResult<FeeEstimateVerbose> {
        consensus.clone().spawn_blocking(move |c| self.inner.get_realtime_feerate_estimations_verbose(c, prefix)).await
    }

    /// Validates a transaction and adds it to the set of known transactions that have not yet been
    /// added to any block.
    ///
    /// The validation is constrained by a Replace by fee policy applied
    /// to double spends in the mempool. For more information, see [`RbfPolicy`].
    ///
    /// The returned transactions are references of objects owned by the mempool.
    pub async fn validate_and_insert_transaction(
        self,
        consensus: &ConsensusProxy,
        transaction: Transaction,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> MiningManagerResult<TransactionInsertion> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.validate_and_insert_transaction(c, transaction, priority, orphan, rbf_policy))
            .await
    }

    /// Validates a batch of transactions, handling iteratively only the independent ones, and
    /// adds those to the set of known transactions that have not yet been added to any block.
    ///
    /// The validation is constrained by a Replace by fee policy applied
    /// to double spends in the mempool. For more information, see [`RbfPolicy`].
    ///
    /// Returns transactions that where unorphaned following the insertion of the provided
    /// transactions. The returned transactions are references of objects owned by the mempool.
    pub async fn validate_and_insert_transaction_batch(
        self,
        consensus: &ConsensusProxy,
        transactions: Vec<Transaction>,
        priority: Priority,
        orphan: Orphan,
        rbf_policy: RbfPolicy,
    ) -> Vec<MiningManagerResult<Arc<Transaction>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.validate_and_insert_transaction_batch(c, transactions, priority, orphan, rbf_policy))
            .await
    }

    pub async fn handle_new_block_transactions(
        self,
        consensus: &ConsensusProxy,
        block_daa_score: u64,
        block_transactions: Arc<Vec<Transaction>>,
    ) -> MiningManagerResult<Vec<Arc<Transaction>>> {
        consensus
            .clone()
            .spawn_blocking(move |c| self.inner.handle_new_block_transactions(c, block_daa_score, &block_transactions))
            .await
    }

    pub async fn expire_low_priority_transactions(self, consensus: &ConsensusProxy) {
        consensus.clone().spawn_blocking(move |c| self.inner.expire_low_priority_transactions(c)).await;
    }

    pub async fn revalidate_high_priority_transactions(
        self,
        consensus: &ConsensusProxy,
        transaction_ids_sender: UnboundedSender<Vec<TransactionId>>,
    ) {
        consensus.clone().spawn_blocking(move |c| self.inner.revalidate_high_priority_transactions(c, transaction_ids_sender)).await;
    }

    /// Try to return a mempool transaction by its id.
    ///
    /// Note: the transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transaction(self, transaction_id: TransactionId, query: TransactionQuery) -> Option<MutableTransaction> {
        spawn_blocking(move || self.inner.get_transaction(&transaction_id, query)).await.unwrap()
    }

    /// Returns whether the mempool holds this transaction in any form.
    pub async fn has_transaction(self, transaction_id: TransactionId, query: TransactionQuery) -> bool {
        spawn_blocking(move || self.inner.has_transaction(&transaction_id, query)).await.unwrap()
    }

    pub async fn transaction_count(self, query: TransactionQuery) -> usize {
        spawn_blocking(move || self.inner.transaction_count(query)).await.unwrap()
    }

    pub async fn get_all_transactions(self, query: TransactionQuery) -> (Vec<MutableTransaction>, Vec<MutableTransaction>) {
        spawn_blocking(move || self.inner.get_all_transactions(query)).await.unwrap()
    }

    /// get_transactions_by_addresses returns the sending and receiving transactions for
    /// a set of addresses.
    ///
    /// Note: a transaction is an orphan if tx.is_fully_populated() returns false.
    pub async fn get_transactions_by_addresses(
        self,
        script_public_keys: ScriptPublicKeySet,
        query: TransactionQuery,
    ) -> GroupedOwnerTransactions {
        spawn_blocking(move || self.inner.get_transactions_by_addresses(&script_public_keys, query)).await.unwrap()
    }

    /// Returns whether a transaction id was registered as accepted in the mempool, meaning
    /// that the consensus accepted a block containing it and said block was handled by the
    /// mempool.
    ///
    /// Registered transaction ids expire after a delay and are unregistered from the mempool.
    /// So a returned value of true means with certitude that the transaction was accepted and
    /// a false means either the transaction was never accepted or it was but beyond the expiration
    /// delay.
    pub async fn has_accepted_transaction(self, transaction_id: TransactionId) -> bool {
        spawn_blocking(move || self.inner.has_accepted_transaction(&transaction_id)).await.unwrap()
    }

    /// Returns a vector of unaccepted transactions.
    /// For more details, see [`Self::has_accepted_transaction()`].
    pub async fn unaccepted_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unaccepted_transactions(transactions)).await.unwrap()
    }

    /// Returns a vector with all transaction ids that are neither in the mempool, nor in the orphan pool
    /// nor accepted.
    pub async fn unknown_transactions(self, transactions: Vec<TransactionId>) -> Vec<TransactionId> {
        spawn_blocking(move || self.inner.unknown_transactions(transactions)).await.unwrap()
    }

    pub fn snapshot(&self) -> MempoolCountersSnapshot {
        self.inner.counters.snapshot()
    }

    pub fn p2p_tx_count_sample(&self) -> P2pTxCountSample {
        self.inner.counters.p2p_tx_count_sample()
    }

    /// Returns a recent sample of transaction count which is not necessarily accurate
    /// but is updated enough for being used as a stats/metric
    pub fn transaction_count_sample(&self, query: TransactionQuery) -> u64 {
        let mut count = 0;
        if query.include_transaction_pool() {
            count += self.inner.counters.txs_sample.load(std::sync::atomic::Ordering::Relaxed)
        }
        if query.include_orphan_pool() {
            count += self.inner.counters.orphans_sample.load(std::sync::atomic::Ordering::Relaxed)
        }
        count
    }
}

/// Represents statistical information about fee rates of transactions.
struct Stats {
    /// The maximum fee rate observed.
    max: f64,
    /// The median fee rate observed.
    median: f64,
    /// The minimum fee rate observed.
    min: f64,
}
/// Calculates the maximum, median, and minimum fee rates (fee per unit mass)
/// for a set of transactions, excluding the first transaction which is assumed
/// to be the coinbase transaction.
///
/// # Arguments
///
/// * `transactions` - A vector of `Transaction` objects. The first transaction
///   is assumed to be the coinbase transaction and is excluded from fee rate
///   calculations.
/// * `calculated_fees` - A vector of fees associated with the transactions.
///   This vector should have one less element than the `transactions` vector
///   since the first transaction (coinbase) does not have a fee.
///
/// # Returns
///
/// Returns an `Option<Stats>` containing the maximum, median, and minimum fee
/// rates if the input vectors are valid. Returns `None` if the vectors are
/// empty or if the lengths are inconsistent.
fn feerate_stats(transactions: Vec<Transaction>, calculated_fees: Vec<u64>) -> Option<Stats> {
    if calculated_fees.is_empty() {
        return None;
    }
    if transactions.len() != calculated_fees.len() + 1 {
        error!(
            "[feerate_stats] block template transactions length ({}) is expected to be one more than `calculated_fees` length ({})",
            transactions.len(),
            calculated_fees.len()
        );
        return None;
    }
    debug_assert!(transactions[0].is_coinbase());
    let mut feerates = calculated_fees
        .into_iter()
        .zip(transactions
            .iter()
            // skip coinbase tx
            .skip(1)
            .map(Transaction::mass))
        .map(|(fee, mass)| fee as f64 / mass as f64)
        .collect_vec();
    feerates.sort_unstable_by(f64::total_cmp);

    let max = feerates[feerates.len() - 1];
    let min = feerates[0];
    let median = feerates[feerates.len() / 2];

    Some(Stats { max, median, min })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::subnets;
    use std::iter::repeat_n;

    fn transactions(length: usize) -> Vec<Transaction> {
        let tx = || {
            let tx = Transaction::new(0, vec![], vec![], 0, Default::default(), 0, vec![]);
            tx.set_mass(2);
            tx
        };
        let mut txs = repeat_n(tx(), length).collect_vec();
        txs[0].subnetwork_id = subnets::SUBNETWORK_ID_COINBASE;
        txs
    }

    #[test]
    fn feerate_stats_test() {
        let calculated_fees = vec![100u64, 200, 300, 400];
        let txs = transactions(calculated_fees.len() + 1);
        let Stats { max, median, min } = feerate_stats(txs, calculated_fees).unwrap();
        assert_eq!(max, 200.0);
        assert_eq!(median, 150.0);
        assert_eq!(min, 50.0);
    }

    #[test]
    fn feerate_stats_empty_test() {
        let calculated_fees = vec![];
        let txs = transactions(calculated_fees.len() + 1);
        assert!(feerate_stats(txs, calculated_fees).is_none());
    }

    #[test]
    fn feerate_stats_inconsistent_test() {
        let calculated_fees = vec![100u64, 200, 300, 400];
        let txs = transactions(calculated_fees.len());
        assert!(feerate_stats(txs, calculated_fees).is_none());
    }
}
