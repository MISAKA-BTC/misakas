//! Consensus and Session management structures.
//!
//! We use newtypes in order to simplify changing the underlying lock in the future

use kaspa_consensus_core::{
    BlockHash, BlockHashSet, BlueWorkType, ChainPath,
    acceptance_data::{AcceptanceData, MergesetBlockAcceptanceData},
    api::{BlockCount, BlockValidationFutures, ConsensusApi, ConsensusStats, DynConsensus},
    block::Block,
    blockstatus::BlockStatus,
    daa_score_timestamp::DaaScoreTimestamp,
    dns_finality::{
        ActiveValidatorSet, AttestationQualityDeficit, DnsConfirmation, StakeBondPage, StakeBondQuery, StakeBondRecord,
        ValidatorAttestationTarget,
    },
    errors::consensus::ConsensusResult,
    header::Header,
    mass::{ContextualMasses, NonContextualMasses},
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{MutableTransaction, Transaction, TransactionId, TransactionOutpoint, TransactionQueryResult, TransactionType, UtxoEntry},
};
use kaspa_utils::sync::rwlock::*;
use std::{ops::Deref, sync::Arc};

pub use tokio::task::spawn_blocking;

use crate::BlockProcessingBatch;

#[allow(dead_code)]
#[derive(Clone)]
pub struct SessionOwnedReadGuard(Arc<RfRwLockOwnedReadGuard>);

#[allow(dead_code)]
pub struct SessionReadGuard<'a>(RfRwLockReadGuard<'a>);

pub struct SessionWriteGuard<'a>(RfRwLockWriteGuard<'a>);

impl SessionWriteGuard<'_> {
    /// Releases and recaptures the write lock. Makes sure that other pending readers/writers get a
    /// chance to capture the lock before this thread does so.
    pub fn blocking_yield(&mut self) {
        self.0.blocking_yield();
    }
}

#[derive(Clone)]
pub struct SessionLock(Arc<RfRwLock>);

impl Default for SessionLock {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionLock {
    pub fn new() -> SessionLock {
        SessionLock(Arc::new(RfRwLock::new()))
    }

    pub async fn read_owned(&self) -> SessionOwnedReadGuard {
        SessionOwnedReadGuard(Arc::new(self.0.clone().read_owned().await))
    }

    pub async fn read(&self) -> SessionReadGuard<'_> {
        SessionReadGuard(self.0.read().await)
    }

    pub fn blocking_read(&self) -> SessionReadGuard<'_> {
        SessionReadGuard(self.0.blocking_read())
    }

    pub fn blocking_write(&self) -> SessionWriteGuard<'_> {
        SessionWriteGuard(self.0.blocking_write())
    }
}

#[derive(Clone)]
pub struct ConsensusInstance {
    session_lock: SessionLock,
    consensus: DynConsensus,
}

impl ConsensusInstance {
    pub fn new(session_lock: SessionLock, consensus: DynConsensus) -> Self {
        Self { session_lock, consensus }
    }

    /// Returns a blocking session to be used in **non async** environments.
    /// Users would usually need to call something like `futures::executor::block_on` in order
    /// to acquire the session, but we prefer leaving this decision to the caller
    pub async fn session_blocking(&self) -> ConsensusSessionBlocking<'_> {
        let g = self.session_lock.read().await;
        ConsensusSessionBlocking::new(g, self.consensus.clone())
    }

    /// Returns an unguarded *blocking* consensus session. There's no guarantee that data will not be pruned between
    /// two sequential consensus calls. This session doesn't hold the consensus pruning lock, so it should
    /// be preferred upon [`session_blocking()`](Self::session_blocking) when data consistency is not important.
    pub fn unguarded_session_blocking(&self) -> ConsensusSessionBlocking<'static> {
        ConsensusSessionBlocking::new_without_session_guard(self.consensus.clone())
    }

    /// Returns a consensus session for accessing consensus operations in a bulk. The user can safely assume
    /// that consensus state is consistent between operations, that is, no pruning was performed between the calls.
    /// The returned object is an *owned* consensus session type which can be cloned and shared across threads.
    /// The sharing ability is useful for spawning blocking operations on a different thread using the same
    /// session object, see [`ConsensusSessionOwned::spawn_blocking()`](ConsensusSessionOwned::spawn_blocking). The caller is responsible to make sure
    /// that the overall lifetime of this session is not too long (~2 seconds max)
    pub async fn session(&self) -> ConsensusSessionOwned {
        let g = self.session_lock.read_owned().await;
        ConsensusSessionOwned::new(g, self.consensus.clone())
    }

    /// Returns an unguarded consensus session. There's no guarantee that data will not be pruned between
    /// two sequential consensus calls. This session doesn't hold the consensus pruning lock, so it should
    /// be preferred upon [`session()`](Self::session) when data consistency is not important.
    pub fn unguarded_session(&self) -> ConsensusSessionOwned {
        ConsensusSessionOwned::new_without_session_guard(self.consensus.clone())
    }
}

pub struct ConsensusSessionBlocking<'a> {
    _session_guard: Option<SessionReadGuard<'a>>,
    consensus: DynConsensus,
}

impl<'a> ConsensusSessionBlocking<'a> {
    pub fn new(session_guard: SessionReadGuard<'a>, consensus: DynConsensus) -> Self {
        Self { _session_guard: Some(session_guard), consensus }
    }

    pub fn new_without_session_guard(consensus: DynConsensus) -> Self {
        Self { _session_guard: None, consensus }
    }
}

impl Deref for ConsensusSessionBlocking<'_> {
    type Target = dyn ConsensusApi; // We avoid exposing the Arc itself by ref since it can be easily cloned and misused

    fn deref(&self) -> &Self::Target {
        self.consensus.as_ref()
    }
}

/// An *owned* consensus session type which can be cloned and shared across threads.
/// See method `spawn_blocking` within for context on the usefulness of this type.
/// Please note - you must use [`ConsensusProxy`] type alias instead of this struct.
#[derive(Clone)]
pub struct ConsensusSessionOwned {
    _session_guard: Option<SessionOwnedReadGuard>,
    consensus: DynConsensus,
}

impl ConsensusSessionOwned {
    pub fn new(session_guard: SessionOwnedReadGuard, consensus: DynConsensus) -> Self {
        Self { _session_guard: Some(session_guard), consensus }
    }

    pub fn new_without_session_guard(consensus: DynConsensus) -> Self {
        Self { _session_guard: None, consensus }
    }

    /// Uses [`tokio::task::spawn_blocking`] to run the provided consensus closure on a thread where blocking is acceptable.
    /// Note that this function is only available on the *owned* session, and requires cloning the session. In fact this
    /// function is the main motivation for a separate session type.
    pub async fn spawn_blocking<F, R>(self, f: F) -> R
    where
        F: FnOnce(&dyn ConsensusApi) -> R + Send + 'static,
        R: Send + 'static,
    {
        spawn_blocking(move || f(self.consensus.as_ref())).await.unwrap()
    }
}

impl ConsensusSessionOwned {
    pub fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        self.consensus.validate_and_insert_block(block)
    }

    pub fn validate_and_insert_block_batch(&self, mut batch: Vec<Block>) -> BlockProcessingBatch {
        // Sort by blue work in order to ensure topological order
        batch.sort_by(|a, b| a.header.blue_work.partial_cmp(&b.header.blue_work).unwrap());
        let (block_tasks, virtual_state_tasks) = batch
            .iter()
            .map(|b| {
                let BlockValidationFutures { block_task, virtual_state_task } = self.consensus.validate_and_insert_block(b.clone());
                (block_task, virtual_state_task)
            })
            .unzip();
        BlockProcessingBatch::new(batch, block_tasks, virtual_state_tasks)
    }

    pub fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        self.consensus.validate_and_insert_trusted_block(tb)
    }

    pub fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> NonContextualMasses {
        // This method performs pure calculations so no need for an async wrapper
        self.consensus.calculate_transaction_non_contextual_masses(transaction)
    }

    pub fn calculate_transaction_contextual_masses(&self, transaction: &MutableTransaction) -> Option<ContextualMasses> {
        // This method performs pure calculations so no need for an async wrapper
        self.consensus.calculate_transaction_contextual_masses(transaction)
    }

    pub fn get_virtual_daa_score(&self) -> u64 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_daa_score()
    }

    pub fn get_virtual_bits(&self) -> u32 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_bits()
    }

    pub fn get_virtual_past_median_time(&self) -> u64 {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_past_median_time()
    }

    pub fn get_virtual_parents(&self) -> BlockHashSet {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_parents()
    }

    pub fn get_virtual_parents_len(&self) -> usize {
        // Accessing cached virtual fields is lock-free and does not require spawn_blocking
        self.consensus.get_virtual_parents_len()
    }

    pub async fn async_get_stats(&self) -> ConsensusStats {
        self.clone().spawn_blocking(|c| c.get_stats()).await
    }

    pub async fn async_get_virtual_merge_depth_root(&self) -> Option<BlockHash> {
        self.clone().spawn_blocking(|c| c.get_virtual_merge_depth_root()).await
    }

    /// Returns the `BlueWork` threshold at which blocks with lower or equal blue work are considered
    /// to be un-mergeable by current virtual state.
    /// (Note: in some rare cases when the node is unsynced the function might return zero as the threshold)
    pub async fn async_get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        self.clone().spawn_blocking(|c| c.get_virtual_merge_depth_blue_work_threshold()).await
    }

    pub async fn async_get_sink(&self) -> BlockHash {
        self.clone().spawn_blocking(|c| c.get_sink()).await
    }

    pub async fn async_get_sink_timestamp(&self) -> u64 {
        self.clone().spawn_blocking(|c| c.get_sink_timestamp()).await
    }

    pub async fn async_get_sink_blue_score(&self) -> u64 {
        self.clone().spawn_blocking(|c| c.get_sink_blue_score()).await
    }

    /// kaspa-pq Phase 10 (ADR-0009): current DNS finality confirmation view
    /// (`None` if the overlay is not configured / no DnsState yet).
    pub async fn async_get_dns_confirmation(&self) -> Option<DnsConfirmation> {
        self.clone().spawn_blocking(|c| c.get_dns_confirmation()).await
    }

    /// kaspa-pq DNS v3: ready epochs below the StakeScore attestation quality floor.
    pub async fn async_get_attestation_quality_deficits(&self) -> Vec<AttestationQualityDeficit> {
        self.clone().spawn_blocking(|c| c.get_attestation_quality_deficits()).await
    }

    /// kaspa-pq Phase 11 (ADR-0010): the stake-bond record at `bond_outpoint`
    /// (`None` if the overlay is not configured / no such bond exists).
    pub async fn async_get_stake_bond(&self, bond_outpoint: TransactionOutpoint) -> Option<StakeBondRecord> {
        self.clone().spawn_blocking(move |c| c.get_stake_bond(bond_outpoint)).await
    }

    /// kaspa-pq: a paged, filtered page of stake bonds (behind the `GetStakeBonds`
    /// RPC). Empty page if the overlay is not configured.
    pub async fn async_get_stake_bonds(&self, query: StakeBondQuery) -> StakeBondPage {
        self.clone().spawn_blocking(move |c| c.get_stake_bonds(query)).await
    }

    /// kaspa-pq Phase 11 (ADR-0010/0012): the validator committee for the current
    /// epoch (`None` if the overlay is not configured / committee not selectable yet).
    pub async fn async_get_active_validator_set(&self) -> Option<ActiveValidatorSet> {
        self.clone().spawn_blocking(|c| c.get_active_validator_set()).await
    }

    /// kaspa-pq Phase 11 (ADR-0010): the ready-to-sign stake-attestation target for
    /// `bond_outpoint` at the current sink (`None` if the overlay is not configured /
    /// no committee selectable yet).
    pub async fn async_get_validator_attestation_target(
        &self,
        bond_outpoint: TransactionOutpoint,
    ) -> Option<ValidatorAttestationTarget> {
        self.clone().spawn_blocking(move |c| c.get_validator_attestation_target(bond_outpoint)).await
    }

    /// kaspa-pq DNS v3 (batch): the READY, creditable canonical-anchor attestation targets
    /// for `bond_outpoint` in `[from_epoch, latest_ready]` (ascending, capped at `limit`),
    /// so a validator that fell behind can sign every missed epoch.
    pub async fn async_get_validator_attestation_targets(
        &self,
        bond_outpoint: TransactionOutpoint,
        from_epoch: u64,
        limit: usize,
    ) -> Vec<ValidatorAttestationTarget> {
        self.clone().spawn_blocking(move |c| c.get_validator_attestation_targets(bond_outpoint, from_epoch, limit)).await
    }

    pub async fn async_get_sink_daa_score_timestamp(&self) -> DaaScoreTimestamp {
        self.clone().spawn_blocking(|c| c.get_sink_daa_score_timestamp()).await
    }

    pub async fn async_get_current_block_color(&self, hash: BlockHash) -> Option<bool> {
        self.clone().spawn_blocking(move |c| c.get_current_block_color(hash)).await
    }

    /// retention period root refers to the earliest block from which the current node has full header & block data
    pub async fn async_get_retention_period_root(&self) -> BlockHash {
        self.clone().spawn_blocking(|c| c.get_retention_period_root()).await
    }

    pub async fn async_estimate_block_count(&self) -> BlockCount {
        self.clone().spawn_blocking(|c| c.estimate_block_count()).await
    }

    pub async fn async_get_virtual_chain_from_block(
        &self,
        low: BlockHash,
        chain_path_added_limit: Option<usize>,
    ) -> ConsensusResult<ChainPath> {
        self.clone().spawn_blocking(move |c| c.get_virtual_chain_from_block(low, chain_path_added_limit)).await
    }

    pub async fn async_get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        self.clone().spawn_blocking(move |c| c.get_virtual_utxos(from_outpoint, chunk_size, skip_first)).await
    }

    /// kaspa-pq EVM Lane §9.2: point lookup of one outpoint in the virtual UTXO
    /// set (resolve a submitted deposit-lock outpoint to its entry).
    pub async fn async_get_virtual_utxo_entry(&self, outpoint: TransactionOutpoint) -> Option<UtxoEntry> {
        self.clone().spawn_blocking(move |c| c.get_virtual_utxo_entry(outpoint)).await
    }

    pub async fn async_get_tips(&self) -> Vec<BlockHash> {
        self.clone().spawn_blocking(|c| c.get_tips()).await
    }

    pub async fn async_get_tips_len(&self) -> usize {
        self.clone().spawn_blocking(|c| c.get_tips_len()).await
    }

    pub async fn async_is_chain_ancestor_of(&self, low: BlockHash, high: BlockHash) -> ConsensusResult<bool> {
        self.clone().spawn_blocking(move |c| c.is_chain_ancestor_of(low, high)).await
    }

    pub async fn async_get_hashes_between(
        &self,
        low: BlockHash,
        high: BlockHash,
        max_blocks: usize,
    ) -> ConsensusResult<(Vec<BlockHash>, BlockHash)> {
        self.clone().spawn_blocking(move |c| c.get_hashes_between(low, high, max_blocks)).await
    }

    pub async fn async_get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        self.clone().spawn_blocking(move |c| c.get_header(hash)).await
    }

    pub async fn async_get_headers_selected_tip(&self) -> BlockHash {
        self.clone().spawn_blocking(|c| c.get_headers_selected_tip()).await
    }

    pub async fn async_get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        self.clone().spawn_blocking(|c| c.get_chain_block_samples()).await
    }

    pub async fn async_get_transactions_by_accepting_daa_score(
        &self,
        accepting_daa_score: u64,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        self.clone().spawn_blocking(move |c| c.get_transactions_by_accepting_daa_score(accepting_daa_score, tx_ids, tx_type)).await
    }

    pub async fn async_get_transactions_by_block_acceptance_data(
        &self,
        accepting_block: BlockHash,
        block_acceptance_data: MergesetBlockAcceptanceData,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        self.clone()
            .spawn_blocking(move |c| {
                c.get_transactions_by_block_acceptance_data(accepting_block, block_acceptance_data, tx_ids, tx_type)
            })
            .await
    }

    /// Returns the antipast of block `hash` from the POV of `context`, i.e. `antipast(hash) ∩ past(context)`.
    /// Since this might be an expensive operation for deep blocks, we allow the caller to specify a limit
    /// `max_traversal_allowed` on the maximum amount of blocks to traverse for obtaining the answer
    pub async fn async_get_antipast_from_pov(
        &self,
        hash: BlockHash,
        context: BlockHash,
        max_traversal_allowed: Option<u64>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.get_antipast_from_pov(hash, context, max_traversal_allowed)).await
    }

    /// Returns the anticone of block `hash` from the POV of `virtual`
    pub async fn async_get_anticone(&self, hash: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.get_anticone(hash)).await
    }

    pub async fn async_get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        self.clone().spawn_blocking(|c| c.get_pruning_point_proof()).await
    }

    pub async fn async_create_virtual_selected_chain_block_locator(
        &self,
        low: Option<BlockHash>,
        high: Option<BlockHash>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.create_virtual_selected_chain_block_locator(low, high)).await
    }

    pub async fn async_create_block_locator_from_pruning_point(
        &self,
        high: BlockHash,
        limit: usize,
    ) -> ConsensusResult<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.create_block_locator_from_pruning_point(high, limit)).await
    }

    pub async fn async_pruning_point_headers(&self) -> Vec<Arc<Header>> {
        self.clone().spawn_blocking(|c| c.pruning_point_headers()).await
    }

    pub async fn async_get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        self.clone().spawn_blocking(|c| c.get_pruning_point_anticone_and_trusted_data()).await
    }

    pub async fn async_get_block(&self, hash: BlockHash) -> ConsensusResult<Block> {
        self.clone().spawn_blocking(move |c| c.get_block(hash)).await
    }

    pub async fn async_get_block_body(&self, hash: BlockHash) -> ConsensusResult<Arc<Vec<Transaction>>> {
        self.clone().spawn_blocking(move |c| c.get_block_body(hash)).await
    }

    /// kaspa-pq EVM Lane v0.4 (§3.1): the block's own EVM payload (absent row =
    /// the empty payload). Served with body-only IBD responses so a v2 block
    /// reassembles with a matching `evm_payload_hash` on the requester.
    /// kaspa-pq EVM Lane v0.4 (§16): raw tx-lookup row (DA visibility + skips).
    pub async fn async_get_evm_tx_locations(
        &self,
        tx_hash: kaspa_consensus_core::EvmH256,
    ) -> ConsensusResult<kaspa_consensus_core::evm::EvmTxLocations> {
        self.clone().spawn_blocking(move |c| c.get_evm_tx_locations(tx_hash)).await
    }

    /// kaspa-pq EVM Lane v0.4 (§16): canonical-resolved receipt (None = not
    /// accepted under the current chain).
    pub async fn async_get_evm_tx_receipt(
        &self,
        tx_hash: kaspa_consensus_core::EvmH256,
    ) -> ConsensusResult<Option<kaspa_consensus_core::evm::EvmTxReceiptView>> {
        self.clone().spawn_blocking(move |c| c.get_evm_tx_receipt(tx_hash)).await
    }

    pub async fn async_get_block_evm_payload(
        &self,
        hash: BlockHash,
    ) -> ConsensusResult<kaspa_consensus_core::evm::EvmExecutionPayload> {
        self.clone().spawn_blocking(move |c| c.get_block_evm_payload(hash)).await
    }

    pub async fn async_get_block_even_if_header_only(&self, hash: BlockHash) -> ConsensusResult<Block> {
        self.clone().spawn_blocking(move |c| c.get_block_even_if_header_only(hash)).await
    }

    pub async fn async_get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
        self.clone().spawn_blocking(move |c| c.get_ghostdag_data(hash)).await
    }

    pub async fn async_get_block_children(&self, hash: BlockHash) -> Option<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.get_block_children(hash)).await
    }

    pub async fn async_get_block_parents(&self, hash: BlockHash) -> Option<Arc<Vec<BlockHash>>> {
        self.clone().spawn_blocking(move |c| c.get_block_parents(hash)).await
    }

    pub async fn async_get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
        self.clone().spawn_blocking(move |c| c.get_block_status(hash)).await
    }

    pub async fn async_get_block_acceptance_data(&self, hash: BlockHash) -> ConsensusResult<Arc<AcceptanceData>> {
        self.clone().spawn_blocking(move |c| c.get_block_acceptance_data(hash)).await
    }

    /// Returns acceptance data for a set of blocks belonging to the selected parent chain.
    ///
    /// See `self::get_virtual_chain`
    pub async fn async_get_blocks_acceptance_data(
        &self,
        hashes: Vec<BlockHash>,
        merged_blocks_limit: Option<usize>,
    ) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        self.clone().spawn_blocking(move |c| c.get_blocks_acceptance_data(&hashes, merged_blocks_limit)).await
    }

    pub async fn async_is_chain_block(&self, hash: BlockHash) -> ConsensusResult<bool> {
        self.clone().spawn_blocking(move |c| c.is_chain_block(hash)).await
    }

    pub async fn async_get_pruning_point_utxos(
        &self,
        expected_pruning_point: BlockHash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        self.clone()
            .spawn_blocking(move |c| c.get_pruning_point_utxos(expected_pruning_point, from_outpoint, chunk_size, skip_first))
            .await
    }

    pub async fn async_get_missing_block_body_hashes(&self, high: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        self.clone().spawn_blocking(move |c| c.get_missing_block_body_hashes(high)).await
    }

    pub async fn async_get_body_missing_anticone(&self) -> Vec<BlockHash> {
        self.clone().spawn_blocking(move |c| c.get_body_missing_anticone()).await
    }

    pub async fn async_clear_body_missing_anticone_set(&self) {
        self.clone().spawn_blocking(move |c| c.clear_body_missing_anticone_set()).await
    }

    pub async fn async_pruning_point(&self) -> BlockHash {
        self.clone().spawn_blocking(|c| c.pruning_point()).await
    }

    pub async fn async_estimate_network_hashes_per_second(
        &self,
        start_hash: Option<BlockHash>,
        window_size: usize,
    ) -> ConsensusResult<u64> {
        self.clone().spawn_blocking(move |c| c.estimate_network_hashes_per_second(start_hash, window_size)).await
    }

    pub async fn async_validate_pruning_points(&self, syncer_virtual_selected_parent: BlockHash) -> ConsensusResult<()> {
        self.clone().spawn_blocking(move |c| c.validate_pruning_points(syncer_virtual_selected_parent)).await
    }

    pub async fn async_are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        self.clone().spawn_blocking(move |c| c.are_pruning_points_violating_finality(pp_list)).await
    }

    pub async fn async_creation_timestamp(&self) -> u64 {
        self.clone().spawn_blocking(move |c| c.creation_timestamp()).await
    }

    pub async fn async_finality_point(&self) -> BlockHash {
        self.clone().spawn_blocking(move |c| c.finality_point()).await
    }
    pub async fn async_clear_pruning_utxo_set(&self) {
        self.clone().spawn_blocking(move |c| c.clear_pruning_utxo_set()).await
    }
    pub async fn async_is_pruning_utxoset_stable(&self) -> bool {
        self.clone().spawn_blocking(move |c| c.is_pruning_utxoset_stable()).await
    }
    pub async fn async_is_pruning_point_anticone_fully_synced(&self) -> bool {
        self.clone().spawn_blocking(move |c| c.is_pruning_point_anticone_fully_synced()).await
    }
    pub async fn async_is_consensus_in_transitional_ibd_state(&self) -> bool {
        self.clone().spawn_blocking(move |c| c.is_consensus_in_transitional_ibd_state()).await
    }
    pub async fn async_set_pruning_utxoset_unstable(&self) {
        self.clone().spawn_blocking(move |c| c.set_pruning_utxoset_stable_flag(false)).await
    }
    pub async fn async_set_pruning_utxoset_stable(&self) {
        self.clone().spawn_blocking(move |c| c.set_pruning_utxoset_stable_flag(true)).await
    }
    pub async fn async_intrusive_pruning_point_update(
        &self,
        new_pruning_point: BlockHash,
        syncer_sink: BlockHash,
    ) -> ConsensusResult<()> {
        self.clone().spawn_blocking(move |c| c.intrusive_pruning_point_update(new_pruning_point, syncer_sink)).await
    }
    pub async fn async_get_n_last_pruning_points(&self, n: usize) -> Vec<BlockHash> {
        self.clone().spawn_blocking(move |c| c.get_n_last_pruning_points(n)).await
    }
}

pub type ConsensusProxy = ConsensusSessionOwned;
