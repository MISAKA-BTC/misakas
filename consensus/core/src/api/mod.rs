use futures_util::future::BoxFuture;
use kaspa_muhash::MuHash;
use std::sync::Arc;

use crate::{
    BlockHashSet, BlueWorkType, ChainPath, Hash64,
    acceptance_data::{AcceptanceData, MergesetBlockAcceptanceData},
    api::args::{TransactionValidationArgs, TransactionValidationBatchArgs},
    block::{
        Block, BlockTemplate, TemplateBuildMode, TemplateTransactionSelector, TemplateTransactionSelectorFactory, VirtualStateApproxId,
    },
    blockstatus::BlockStatus,
    coinbase::MinerData,
    daa_score_timestamp::DaaScoreTimestamp,
    dns_finality::{
        ActiveValidatorSet, AttestationQualityDeficit, ComputeStatusView, DnsConfirmation, MandatoryAttestationDeficit,
        PendingComputeVerdict, PrecommitDuty, StakeBondPage, StakeBondQuery, StakeBondRecord, ValidatorAttestationTarget,
        VltStatusView,
    },
    errors::{
        block::{BlockProcessResult, RuleError},
        coinbase::CoinbaseResult,
        consensus::{ConsensusError, ConsensusResult},
        pruning::PruningImportResult,
        tx::TxResult,
    },
    header::Header,
    mass::{ContextualMasses, NonContextualMasses},
    pruning::{PruningPointProof, PruningPointTrustedData, PruningPointsList, PruningProofMetadata},
    trusted::{ExternalGhostdagData, TrustedBlock},
    tx::{
        MutableTransaction, Transaction, TransactionId, TransactionIndexType, TransactionOutpoint, TransactionQueryResult,
        TransactionType, UtxoEntry,
    },
};

// PR-9.5e: ConsensusApi block-identifier methods use the 64-byte BlockHash.
use crate::BlockHash;

pub use self::stats::{BlockCount, ConsensusStats};

pub mod args;
pub mod counters;
pub mod stats;

pub type BlockValidationFuture = BoxFuture<'static, BlockProcessResult<BlockStatus>>;

/// A struct returned by consensus for block validation processing calls
pub struct BlockValidationFutures {
    /// A future triggered when block processing is completed (header and body processing)
    pub block_task: BlockValidationFuture,

    /// A future triggered when DAG state which included this block has been processed by the virtual processor
    /// (exceptions are header-only blocks and trusted blocks which have the future completed before virtual
    /// processing along with the `block_task`)
    pub virtual_state_task: BlockValidationFuture,
}

/// Abstracts the consensus external API
#[allow(unused_variables)]
pub trait ConsensusApi: Send + Sync {
    fn build_block_template(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        unimplemented!()
    }

    /// kaspa-pq EVM Lane v0.4 (§15 step 6): [`Self::build_block_template`] with
    /// the node's own EVM payload candidates (raw EIP-2718 bytes from the EVM
    /// mempool, pre-admitted + fee-ordered + nonce-ascending). The default
    /// ignores the candidates — correct for every mock and for pre-activation
    /// templates, where the own payload is empty anyway.
    fn build_block_template_with_evm(
        &self,
        miner_data: MinerData,
        tx_selector: Box<dyn TemplateTransactionSelector>,
        build_mode: TemplateBuildMode,
        evm_template_data: crate::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        let _ = evm_template_data;
        self.build_block_template(miner_data, tx_selector, build_mode)
    }

    /// Builds a block template by letting consensus first fix the selected-parent /
    /// virtual-state snapshot and then asking `tx_selector_factory` for a selector using
    /// the mandatory-attestation deficits derived from that exact snapshot. The default
    /// implementation preserves compatibility for mocks/non-overlay callers.
    fn build_block_template_with_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
    ) -> Result<BlockTemplate, RuleError> {
        let tx_selector = tx_selector_factory.build_selector(None, &[]);
        self.build_block_template(miner_data, tx_selector, build_mode)
    }

    /// [`Self::build_block_template_with_selector_factory`] with the node's own EVM payload
    /// candidates.
    fn build_block_template_with_evm_selector_factory(
        &self,
        miner_data: MinerData,
        tx_selector_factory: &dyn TemplateTransactionSelectorFactory,
        build_mode: TemplateBuildMode,
        evm_template_data: crate::evm::EvmTemplateData,
    ) -> Result<BlockTemplate, RuleError> {
        let tx_selector = tx_selector_factory.build_selector(None, &[]);
        self.build_block_template_with_evm(miner_data, tx_selector, build_mode, evm_template_data)
    }

    fn validate_and_insert_block(&self, block: Block) -> BlockValidationFutures {
        unimplemented!()
    }

    fn validate_and_insert_trusted_block(&self, tb: TrustedBlock) -> BlockValidationFutures {
        unimplemented!()
    }

    /// Populates the mempool transaction with maximally found UTXO entry data and proceeds to full transaction
    /// validation if all are found. If validation is successful, also `transaction.calculated_fee` is expected to be populated.
    fn validate_mempool_transaction(&self, transaction: &mut MutableTransaction, args: &TransactionValidationArgs) -> TxResult<()> {
        unimplemented!()
    }

    /// Populates the mempool transactions with maximally found UTXO entry data and proceeds to full transactions
    /// validation if all are found. If validation is successful, also `transaction.calculated_fee` is expected to be populated.
    fn validate_mempool_transactions_in_parallel(
        &self,
        transactions: &mut [MutableTransaction],
        args: &TransactionValidationBatchArgs,
    ) -> Vec<TxResult<()>> {
        unimplemented!()
    }

    /// Populates the mempool transaction with maximally found UTXO entry data.
    fn populate_mempool_transaction(&self, transaction: &mut MutableTransaction) -> TxResult<()> {
        unimplemented!()
    }

    /// Populates the mempool transactions with maximally found UTXO entry data.
    fn populate_mempool_transactions_in_parallel(&self, transactions: &mut [MutableTransaction]) -> Vec<TxResult<()>> {
        unimplemented!()
    }

    fn calculate_transaction_non_contextual_masses(&self, transaction: &Transaction) -> NonContextualMasses {
        unimplemented!()
    }

    fn calculate_transaction_contextual_masses(&self, transaction: &MutableTransaction) -> Option<ContextualMasses> {
        unimplemented!()
    }

    /// Returns an aggregation of consensus stats. Designed to be a fast call.
    fn get_stats(&self) -> ConsensusStats {
        unimplemented!()
    }

    fn get_virtual_daa_score(&self) -> u64 {
        unimplemented!()
    }

    /// **What a `ConsensusV2` block producer must read from chain state** — see
    /// [`crate::palw_producer_v2`] for why it is one derived answer rather than the ingredients.
    ///
    /// `None` on every network that is not `ConsensusV2`, and on one that is but does not know the
    /// class — which is the honest answer in both cases and not an error: a producer asking a
    /// hash-only chain for a class target is a producer on the wrong network, and it should be
    /// able to find that out by asking.
    fn palw_producer_facts_v2(
        &self,
        _class_id: kaspa_hashes::Hash64,
        _bond: Option<crate::tx::TransactionOutpoint>,
    ) -> Option<crate::palw_producer_v2::PalwProducerFactsV2> {
        None
    }

    fn get_virtual_bits(&self) -> u32 {
        unimplemented!()
    }

    fn get_virtual_past_median_time(&self) -> u64 {
        unimplemented!()
    }

    fn get_virtual_merge_depth_root(&self) -> Option<BlockHash> {
        unimplemented!()
    }

    /// Returns the `BlueWork` threshold at which blocks with lower or equal blue work are considered
    /// to be un-mergeable by current virtual state.
    /// (Note: in some rare cases when the node is unsynced the function might return zero as the threshold)
    fn get_virtual_merge_depth_blue_work_threshold(&self) -> BlueWorkType {
        unimplemented!()
    }

    fn get_sink(&self) -> BlockHash {
        unimplemented!()
    }

    fn get_sink_timestamp(&self) -> u64 {
        unimplemented!()
    }

    fn get_sink_blue_score(&self) -> u64 {
        unimplemented!()
    }

    /// kaspa-pq Phase 10 (ADR-0009): the current DNS finality confirmation view,
    /// or `None` when the DNS overlay is not configured for this network or no
    /// DnsState has been written yet. Default `None` keeps non-overlay
    /// ConsensusApi impls (mocks/tests) trivially correct.
    fn get_dns_confirmation(&self) -> Option<DnsConfirmation> {
        None
    }

    /// kaspa-pq Phase 11 (ADR-0010): the stake-bond record registered at
    /// `bond_outpoint`, or `None` when the DNS overlay is not configured for this
    /// network or no such bond exists. Used by the in-process validator service to
    /// evaluate its own bond status (active / unbonding / slashed). Default `None`
    /// keeps non-overlay ConsensusApi impls (mocks/tests) trivially correct.
    fn get_stake_bond(&self, _bond_outpoint: TransactionOutpoint) -> Option<StakeBondRecord> {
        None
    }

    /// MISAKA Compute Token Program (design §9.3): one `(asset, owner)` TOK ledger
    /// row. `None` when the token program is not configured for this network;
    /// `Some(default)` (zero balance, zero nonce) for an absent row — an absent row
    /// IS the empty account (design §4.2). Default `None` keeps non-token impls
    /// trivially correct.
    fn get_token_account(&self, _asset_id: u64, _owner: Hash64) -> Option<crate::token::TokenAccount> {
        None
    }

    /// MISAKA Compute Token Program (design §9.3): an asset's supply counters, with
    /// the same `None`/default semantics as [`Self::get_token_account`].
    fn get_token_supply(&self, _asset_id: u64) -> Option<crate::token::TokenSupply> {
        None
    }

    /// MISAKA Compute Token Program (design §9.3): one epoch's emission-settlement
    /// view plus the fold/settlement cursors. `epoch = None` reads the most recently
    /// settled epoch (settlement cursor − 1). `None` when the token program is not
    /// configured.
    fn get_token_emission_info(&self, _epoch: Option<u64>) -> Option<crate::token::TokenEmissionInfo> {
        None
    }

    /// kaspa-pq: a paged, filtered enumeration of the `StakeBonds` overlay store,
    /// backing the `GetStakeBonds` RPC. Lets a bond owner recover the outpoint(s)
    /// of bonds they funded — the key a `StakeUnbondRequest` binds to — since the
    /// store is outpoint-keyed with no owner index (the owner filter is a full
    /// scan). Returns an empty page when the DNS overlay is not configured.
    /// Default keeps non-overlay ConsensusApi impls (mocks/tests) trivially correct.
    fn get_stake_bonds(&self, _query: StakeBondQuery) -> StakeBondPage {
        StakeBondPage::default()
    }

    /// kaspa-pq Phase 11 (ADR-0010/0017): the active validator set for the current epoch
    /// (at the sink), or `None` when the DNS overlay is not configured. Under ADR-0017
    /// every active-bond validator attests (no committee, no sortition); the in-process
    /// validator service checks its own `validator_id` against `members` to decide
    /// attestation eligibility. Default `None`.
    fn get_active_validator_set(&self) -> Option<ActiveValidatorSet> {
        None
    }

    /// kaspa-pq DNS optional hard-inclusion diagnostic: deficient canonical epochs for the current
    /// selected parent, including the anchor tuple, already-credited `(bond, validator, epoch)` keys,
    /// active validators, and stake delta still needed to reach the quality floor. Shipped presets
    /// keep the hard-inclusion fence inert, so this is empty there. This legacy API is not
    /// template-exact because it cannot include candidate accepted txs from a future template
    /// snapshot. Mining must use [`Self::build_block_template_with_selector_factory`] so consensus
    /// can pass the selector deficits derived from the exact template snapshot.
    fn get_mandatory_attestation_deficits(&self) -> Vec<MandatoryAttestationDeficit> {
        Vec::new()
    }

    /// kaspa-pq liveness-first attestation diagnostic: ready canonical epochs whose included
    /// attestation stake is below the network quality floor. Unlike
    /// [`Self::get_mandatory_attestation_deficits`], this remains populated on shipped presets
    /// where missing attestations are not a base-chain validity failure.
    fn get_attestation_quality_deficits(&self) -> Vec<AttestationQualityDeficit> {
        Vec::new()
    }

    /// kaspa-pq Phase 11 (ADR-0010/0017): the ready-to-sign stake-attestation target for
    /// `bond_outpoint` at the current sink (epoch, target, active-validator-set commitment, and
    /// the bound message digest), or `None` when the overlay is not configured. The validator
    /// service signs `message` under `ATTESTATION_MLDSA87_CONTEXT`. Default `None`.
    fn get_validator_attestation_target(&self, _bond_outpoint: TransactionOutpoint) -> Option<ValidatorAttestationTarget> {
        None
    }

    /// kaspa-pq DNS v3 (batch): all READY, creditable (non-duplicate) canonical-anchor
    /// attestation targets for `bond_outpoint` in `[from_epoch, latest_ready]`, ascending, capped
    /// at `limit`, filtering out epochs whose anchor DAA does not see this bond as Active. Lets a
    /// validator that fell behind sign every epoch it missed. Default empty.
    fn get_validator_attestation_targets(
        &self,
        _bond_outpoint: TransactionOutpoint,
        _from_epoch: u64,
        _limit: usize,
    ) -> Vec<ValidatorAttestationTarget> {
        Vec::new()
    }

    /// MISAKA Verified LLM Token-Weighted BFT: the accepted compute certificates `validator_id`
    /// was sortitioned to audit and has not published a verdict for, newest first, capped at
    /// `limit`.
    ///
    /// A verifier learns it was drawn by asking this rather than by being told: the certificate,
    /// its phase-1 commitment (including the job input) and the sortition beacon are all on chain,
    /// so committee membership is derivable, and it is derived by the same code that credits the
    /// certificate. A node that guessed differently from consensus would either publish verdicts
    /// nobody counts or withhold ones a job needs to mint.
    ///
    /// Empty below the VLT fence, which is where every shipped preset sits.
    fn get_pending_compute_verdicts(&self, _validator_id: Hash64, _limit: usize) -> Vec<PendingComputeVerdict> {
        Vec::new()
    }

    /// MISAKA Verified LLM Token-Weighted BFT: this validator's own standing in the compute
    /// overlay — capability expiry, in-class peers, and the commitments it has not certified yet.
    ///
    /// `None` when the DNS overlay is not configured for this network.
    fn get_compute_status(&self, _validator_id: Hash64, _bond_outpoint: TransactionOutpoint) -> Option<ComputeStatusView> {
        None
    }

    /// MISAKA: where this network sits on the two-fence activation path, plus the gauges behind
    /// that decision — `W(E)`, `Q(E)`, the snapshot epoch and its root.
    ///
    /// Read from the last recompute rather than derived on demand: deriving it would re-walk the
    /// credit window, and a metrics scrape must never be able to stall consensus. `None` when the
    /// DNS overlay is not configured.
    fn get_vlt_status(&self) -> Option<VltStatusView> {
        None
    }

    /// MISAKA §5 round 2: the lock this validator is carrying on the selected chain and the epochs
    /// it still owes a precommit for.
    ///
    /// `None` when the DNS overlay is not configured; `round_active: false` below the VLT weight
    /// fence, which is where every shipped preset sits.
    fn get_precommit_duty(&self, _validator_id: Hash64, _bond_outpoint: TransactionOutpoint) -> Option<PrecommitDuty> {
        None
    }

    fn get_sink_daa_score_timestamp(&self) -> DaaScoreTimestamp {
        unimplemented!()
    }

    fn get_current_block_color(&self, hash: BlockHash) -> Option<bool> {
        unimplemented!()
    }

    fn get_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        unimplemented!()
    }

    /// retention period root refers to the earliest block from which the current node has full header & block data
    fn get_retention_period_root(&self) -> BlockHash {
        unimplemented!()
    }

    fn estimate_block_count(&self) -> BlockCount {
        unimplemented!()
    }

    /// Gets the virtual chain paths from `low` to the `sink` hash, or until `chain_path_added_limit` is reached
    ///
    /// Note:
    ///     1) `chain_path_added_limit` will populate removed fully, and then the added chain path, up to `chain_path_added_limit` amount of hashes.
    ///     1.1) use `None to impose no limit with optimized backward chain iteration, for better performance in cases where batching is not required.
    fn get_virtual_chain_from_block(&self, low: BlockHash, chain_path_added_limit: Option<usize>) -> ConsensusResult<ChainPath> {
        unimplemented!()
    }

    fn get_chain_block_samples(&self) -> Vec<DaaScoreTimestamp> {
        unimplemented!()
    }

    /// Returns the fully populated transaction with the given txid which was accepted at the provided accepting_block_daa_score.
    /// The argument `accepting_block_daa_score` is expected to be the DAA score of the accepting chain block of `txid`.
    /// Note: If the transaction vec is None, the function returns all accepted transactions.
    fn get_transactions_by_accepting_daa_score(
        &self,
        accepting_daa_score: u64,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        unimplemented!()
    }

    fn get_transactions_by_block_acceptance_data(
        &self,
        accepting_block: BlockHash,
        block_acceptance_data: MergesetBlockAcceptanceData,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        unimplemented!()
    }

    fn get_transactions_by_accepting_block(
        &self,
        accepting_block: BlockHash,
        tx_ids: Option<Vec<TransactionId>>,
        tx_type: TransactionType,
    ) -> ConsensusResult<TransactionQueryResult> {
        unimplemented!()
    }

    fn get_virtual_parents(&self) -> BlockHashSet {
        unimplemented!()
    }

    fn get_virtual_parents_len(&self) -> usize {
        unimplemented!()
    }

    fn get_virtual_utxos(
        &self,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> Vec<(TransactionOutpoint, UtxoEntry)> {
        unimplemented!()
    }

    /// Point lookup of a single outpoint in the virtual UTXO set (kaspa-pq EVM
    /// Lane §9.2: a depositor submits a lock outpoint; the node resolves the
    /// `EVM_DEPOSIT_LOCK` entry to build + validate a `DepositClaim`). `None` if
    /// the outpoint is unspent-absent (never existed or already spent).
    fn get_virtual_utxo_entry(&self, outpoint: TransactionOutpoint) -> Option<UtxoEntry> {
        unimplemented!()
    }

    fn get_tips(&self) -> Vec<BlockHash> {
        unimplemented!()
    }

    fn get_tips_len(&self) -> usize {
        unimplemented!()
    }

    fn modify_coinbase_payload(&self, payload: Vec<u8>, miner_data: &MinerData) -> CoinbaseResult<Vec<u8>> {
        unimplemented!()
    }

    // PR-9.5c: return type widened to `crate::MerkleRoot`
    // (= `Hash64`) for the ADR-0008 consensus-identity cascade.
    fn calc_transaction_hash_merkle_root(&self, txs: &[Transaction]) -> crate::MerkleRoot {
        unimplemented!()
    }

    /// Validate a proof's own soundness without comparing it to this node's chain.
    ///
    /// For weighing a chain the node has not committed to; see
    /// `PruningProofManager::validate_pruning_point_proof_standalone`.
    fn validate_pruning_proof_standalone(&self, _proof: &PruningPointProof) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn validate_pruning_proof(&self, proof: &PruningPointProof, proof_metadata: &PruningProofMetadata) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn apply_pruning_proof(&self, proof: PruningPointProof, trusted_set: &[TrustedBlock]) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn import_pruning_points(&self, pruning_points: PruningPointsList) -> PruningImportResult<()> {
        unimplemented!()
    }

    fn append_imported_pruning_point_utxos(&self, utxoset_chunk: &[(TransactionOutpoint, UtxoEntry)], current_multiset: &mut MuHash) {
        unimplemented!()
    }

    fn import_pruning_point_utxo_set(&self, new_pruning_point: BlockHash, imported_utxo_multiset: MuHash) -> PruningImportResult<()> {
        unimplemented!()
    }

    // kaspa-pq ADR-0022: pruned-IBD EVM + DNS/PoS-v2 overlay snapshot transfer.
    /// Serve: the pruning point's EVM execution header + state snapshot, or `None` if absent.
    fn pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
    ) -> Option<(crate::evm::EvmExecutionHeader, crate::evm::EvmStateSnapshot)> {
        let _ = pruning_point;
        unimplemented!()
    }

    /// Import: verify + persist the pruning point's EVM execution state.
    fn import_pruning_point_evm_state(
        &self,
        pruning_point: BlockHash,
        evm_header: crate::evm::EvmExecutionHeader,
        snapshot: crate::evm::EvmStateSnapshot,
    ) -> PruningImportResult<()> {
        let _ = (pruning_point, evm_header, snapshot);
        unimplemented!()
    }

    /// Serve: the persisted overlay snapshot as-of the current pruning point, or `None`.
    fn pruning_point_overlay_snapshot(&self) -> Option<crate::dns_finality::PruningPointOverlaySnapshot> {
        unimplemented!()
    }

    /// **Assemble a lifecycle object from a receipt pool** — the largest set of gossip-delivered
    /// receipts the acceptance validator itself accepts for `claim`, as the object a block would
    /// take (`ReceiptLicensed` on a Licensed quorum, `ProducerDefaulted` on an Unavailable one), or
    /// `None` while no quorum is assemblable. Garbage candidates are dropped, never fatal: the
    /// submitter's whole job is to keep working next to a poisoned pool.
    fn palw_v2_receipt_quorum_assemble(
        &self,
        _claim: crate::Hash64,
        _candidates: Vec<crate::palw_panel_v2::PalwSeatReceiptV2>,
    ) -> Option<crate::palw_state_v2::PalwConsensusObjectV2> {
        unimplemented!()
    }

    /// **The seat duties this node holds** (launch blockers §2) — the claims whose panels name a
    /// bond in `mine`, with the roots each seat must decide against.
    fn palw_seat_duties_v2(&self, _mine: Vec<crate::palw_state_v2::PalwBondKeyV2>) -> Vec<crate::palw_producer_v2::PalwSeatDutyV2> {
        Vec::new()
    }

    /// Import: install the pruning point's PALW V2 chain state (launch blockers §1).
    ///
    /// The carriage is REBUILT and its `state_root` demanded back against the pruning point's own
    /// header before anything is written — a peer that forges one byte of bonds, shares or claims
    /// produces a different root and is refused here, not detected after a durable write.
    fn import_pruning_point_palw_state(
        &self,
        pruning_point: BlockHash,
        carriage: crate::palw_state_v2::PalwStateCarriageV2,
    ) -> PruningImportResult<()> {
        let _ = (pruning_point, carriage);
        unimplemented!()
    }

    /// Export: the pruning point's PALW V2 chain state, for a peer syncing from it. `None` on a
    /// network with no V2 ruleset, or when this node holds no state at that point.
    fn pruning_point_palw_state(&self, pruning_point: BlockHash) -> Option<crate::palw_state_v2::PalwStateCarriageV2> {
        let _ = pruning_point;
        None
    }

    /// Import: persist the pruning point's DNS/PoS-v2 overlay snapshot.
    fn import_pruning_point_overlay_snapshot(
        &self,
        pruning_point: BlockHash,
        snapshot: crate::dns_finality::OverlaySnapshot,
    ) -> PruningImportResult<()> {
        let _ = (pruning_point, snapshot);
        unimplemented!()
    }

    fn is_chain_ancestor_of(&self, low: BlockHash, high: BlockHash) -> ConsensusResult<bool> {
        unimplemented!()
    }

    fn get_hashes_between(&self, low: BlockHash, high: BlockHash, max_blocks: usize) -> ConsensusResult<(Vec<BlockHash>, BlockHash)> {
        unimplemented!()
    }

    fn get_header(&self, hash: BlockHash) -> ConsensusResult<Arc<Header>> {
        unimplemented!()
    }

    /// The heaviest header chain by BLUE WORK — a **download-ordering hint, never chain authority**
    /// (ADR-0042 Decision 9, external audit P0-5).
    ///
    /// Renamed from `get_headers_selected_tip`, and the rename is the point. PALW weight is a
    /// function of ACCEPTED TRANSACTIONS, and the header processor is upstream of bodies by
    /// construction — header-first sync exists to order headers whose bodies have not arrived. So
    /// this value cannot be PALW-ordered, and a header-only approximation would be a SECOND
    /// fork-choice rule disagreeing with the first, which is the defect rather than a fix for it.
    ///
    /// A node may therefore sync toward a header chain PALW would not select, and then decline to
    /// accept it: the cost is wasted download, never a divergent chain. What must never happen is a
    /// consensus decision reading this — pruning, finality and acceptance take the virtual sink,
    /// which is ordered by `palw_fork_choice::compare_palw_candidates_v1`.
    ///
    /// The old name read like an authority and was used like a hint. Anything that starts reading
    /// this to decide chain state reopens P0-5, and now has to type "download hint" to do it.
    fn get_header_download_hint(&self) -> BlockHash {
        unimplemented!()
    }

    /// **Unit D, site 2's input: this consensus's own PALW order for its virtual sink.**
    ///
    /// `None` on a network with no `ConsensusV2` bundle (every shipped preset) and on a sink this
    /// node could not weigh. The IBD staging gate compares two of these through
    /// `palw_fork_authority_v2::decide_ibd_commit_v2`, which is the SAME comparator the virtual
    /// tip and the deep-reorg gate use — the unit's whole point is that a node cannot hold two
    /// answers to "which chain is canonical" (P0-5).
    ///
    /// Deliberately the sink's and not the header download hint's: the hint is a download
    /// heuristic that says so in its own name, and a consensus decision reading it reopens P0-5.
    fn get_palw_candidate_order_v2(&self) -> Option<crate::palw_fork_choice::PalwCandidateOrderV1> {
        None
    }

    /// Returns the antipast of block `hash` from the POV of `context`, i.e. `antipast(hash) ∩ past(context)`.
    /// Since this might be an expensive operation for deep blocks, we allow the caller to specify a limit
    /// `max_traversal_allowed` on the maximum amount of blocks to traverse for obtaining the answer
    fn get_antipast_from_pov(
        &self,
        hash: BlockHash,
        context: BlockHash,
        max_traversal_allowed: Option<u64>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        unimplemented!()
    }

    /// Returns the anticone of block `hash` from the POV of `virtual`
    fn get_anticone(&self, hash: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        unimplemented!()
    }

    fn get_pruning_point_proof(&self) -> Arc<PruningPointProof> {
        unimplemented!()
    }

    fn create_virtual_selected_chain_block_locator(
        &self,
        low: Option<BlockHash>,
        high: Option<BlockHash>,
    ) -> ConsensusResult<Vec<BlockHash>> {
        unimplemented!()
    }

    fn create_block_locator_from_pruning_point(&self, high: BlockHash, limit: usize) -> ConsensusResult<Vec<BlockHash>> {
        unimplemented!()
    }

    fn pruning_point_headers(&self) -> Vec<Arc<Header>> {
        unimplemented!()
    }

    fn get_pruning_point_anticone_and_trusted_data(&self) -> ConsensusResult<Arc<PruningPointTrustedData>> {
        unimplemented!()
    }

    fn get_block(&self, hash: BlockHash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_block_transactions(
        &self,
        hash: BlockHash,
        indices: Option<Vec<TransactionIndexType>>,
    ) -> ConsensusResult<Vec<Transaction>> {
        unimplemented!()
    }

    fn get_block_body(&self, hash: BlockHash) -> ConsensusResult<Arc<Vec<Transaction>>> {
        unimplemented!()
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the raw tx-lookup row for an EVM tx hash
    /// (absent = never seen). `accepted_in` entries may include orphaned
    /// branches; pair with [`Self::get_evm_tx_receipt`] for canonical state.
    fn get_evm_tx_locations(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<crate::evm::EvmTxLocations> {
        unimplemented!()
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the canonical-resolved receipt of an EVM
    /// tx — `Some` iff some ACCEPTING block on the CURRENT selected chain
    /// executed it (`eth_getTransactionReceipt` semantics: pending / skipped /
    /// orphaned ⇒ `None`).
    fn get_evm_tx_receipt(&self, tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<Option<crate::evm::EvmTxReceiptView>> {
        unimplemented!()
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the canonical EVM head execution header —
    /// the EVM header committed by the current sink (drives `eth_blockNumber`
    /// and the "latest" block tag). `None` on a non-EVM node / before activation.
    fn get_evm_head_header(&self) -> ConsensusResult<Option<crate::evm::EvmExecutionHeader>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the EVM execution header committed by the
    /// L1 chain block `block`, if any.
    fn get_evm_header_of(&self, _block: BlockHash) -> ConsensusResult<Option<crate::evm::EvmExecutionHeader>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16, audit H-04): the canonical EVM heads —
    /// `latest` / `safe` / `finalized` L1 block hashes — used to resolve the
    /// `safe`/`finalized` block tags and to read account state at a non-reorgable
    /// height (holder-gated access). `None` on a non-EVM node / before activation.
    fn get_evm_canonical_heads(&self) -> ConsensusResult<Option<crate::evm::CanonicalEvmHeads>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the full EVM state snapshot committed after
    /// the L1 chain block `block`, for read-only `eth_*` state queries and
    /// `eth_call` simulation.
    fn get_evm_state_snapshot_of(&self, _block: BlockHash) -> ConsensusResult<Option<crate::evm::EvmStateSnapshot>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane (§11): the per-accepting-block `debug_traceTransaction`
    /// replay plan (store prefix 219). `None` on a non-EVM node, before activation,
    /// after pruning, or for an accepting block with no traceable candidate txs.
    fn get_evm_trace_replay_body(&self, _block: BlockHash) -> ConsensusResult<Option<crate::evm::EvmTraceReplayBodyV1>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane (§12): reconstruct + verify the full EVM state AT a
    /// historical block by seeding from the nearest checkpoint (or genesis) and
    /// replaying the forward diffs along the block's selected-parent chain
    /// (design §12.4). The reconstructed state's keccak-MPT root is checked
    /// against the block's committed `state_root` — a mismatch is a hard error
    /// (data corruption; never an empty-state fallback). `Ok(None)` ⇒ `block` is
    /// not an EVM block (no header). An `Err` ⇒ the block IS an EVM block but its
    /// state history is unavailable on this node (GC'd past `--evm-history-mode`
    /// retention) or corrupt — the RPC layer maps it to a JSON-RPC error, never a
    /// silent empty state. Requires an `evm`-feature node (revm for the root).
    fn reconstruct_evm_state_at(&self, _block: BlockHash) -> ConsensusResult<Option<crate::evm::EvmStateSnapshot>> {
        Ok(None)
    }

    /// kaspa-pq C-01 Stage 1 (S7, audit H-03): an O(1) flat point-lookup of one
    /// account at the CURRENT canonical head. A point query (`eth_getBalance` etc.)
    /// must not materialize the entire EVM state (H-03 = unbounded RPC full-state
    /// reads); when the flat state backend is at the head this answers from a single
    /// keyed row. Returns [`crate::evm::FlatHeadAccount::Stale`] when the flat store
    /// is not at the head (shadow backend disabled / mid-rebase / read error), so the
    /// caller transparently falls back to the authoritative full-snapshot path. Pure
    /// store reads — no revm — so it is available (and simply `Stale`) on a non-evm
    /// node. RPC read-only ⇒ consensus-neutral. Default = always `Stale`.
    fn get_evm_flat_account_at_head(&self, _address: crate::evm::EvmAddress) -> ConsensusResult<crate::evm::FlatHeadAccount> {
        Ok(crate::evm::FlatHeadAccount::Stale)
    }

    /// kaspa-pq EVM Lane (§11): the network's three EVM-execution activation fences
    /// — `(evm_gas_pool_v2, evm_f002_withdraw_cap, evm_f003_mldsa_verify)` activation
    /// DAA scores. The trace replay MUST run the same gas-pool / withdraw-cap / F003
    /// regime the accepting block executed under, so it reads these instead of
    /// assuming inert. Default = all inert (`u64::MAX`).
    fn evm_activation_fences(&self) -> (u64, u64, u64) {
        (u64::MAX, u64::MAX, u64::MAX)
    }

    /// kaspa-pq EVM Lane: the canonical account nonces at the EVM head (the sink's
    /// committed EVM state) for `addresses`. An account that does not exist yet is
    /// omitted; the caller treats absence as nonce 0. Used by the mining template
    /// path to prune already-accepted txs (nonce < state nonce) and to select
    /// contiguous per-sender nonce runs. Pure local template policy — never part of
    /// consensus. Default impl reuses the head state snapshot.
    ///
    /// When the sink has NO committed EVM state snapshot (pre-first-EVM-commit, or a
    /// transient sink view) this returns `Err` — distinct from `Ok(empty)`. The two
    /// must not be conflated: `Ok(`absent account`)` legitimately means nonce 0, but
    /// "no snapshot at all" means there is no canonical nonce view, and resolving
    /// every sender to 0 in that case would start each sender's run at nonce 0, find
    /// nothing for a higher-nonce sender, and strand it at
    /// `included_in=[] / last_skip_class=0` (payload starvation). The mining template
    /// path treats the `Err` as "skip the EVM payload this template" (empty payload +
    /// WARN). On an EVM-active chain past the first commit the sink always has a
    /// snapshot, so the `Err` only fires in the early / no-EVM-state window.
    fn get_evm_account_nonces(
        &self,
        addresses: &[crate::evm::EvmAddress],
    ) -> ConsensusResult<std::collections::HashMap<crate::evm::EvmAddress, u64>> {
        if addresses.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let wanted: std::collections::HashSet<crate::evm::EvmAddress> = addresses.iter().copied().collect();
        let Some(s) = self.get_evm_state_snapshot_of(self.get_sink())? else {
            return Err(ConsensusError::General("no committed EVM state snapshot at the sink"));
        };
        let mut out = std::collections::HashMap::with_capacity(addresses.len());
        for acct in s.accounts {
            if wanted.contains(&acct.address) {
                out.insert(acct.address, acct.nonce);
            }
        }
        Ok(out)
    }

    /// kaspa-pq EVM Lane (audit H-10): the canonical `(nonce, balance)` at the EVM head
    /// for `addresses`. Same source + same `Err` (no snapshot) semantics as
    /// [`Self::get_evm_account_nonces`]; an account absent from the snapshot is omitted
    /// (caller treats it as nonce 0 / balance 0). `balance` is the
    /// [`crate::evm::EvmU256`] wei balance saturated into a `u128` (a balance above
    /// `u128::MAX` saturates UP, so it is never mistaken for "cannot pay"). Used by the
    /// mining template path to skip selecting a sender's transaction when its committed
    /// balance cannot cover the EIP-1559 up-front gas reservation — a guaranteed
    /// class-2 skip at execution that would otherwise waste a payload slot. Pure local
    /// template policy; never part of consensus, and a tx is never evicted by it (only
    /// passed over for THIS template, so a later credit lets a future template pick it).
    fn get_evm_account_states(
        &self,
        addresses: &[crate::evm::EvmAddress],
    ) -> ConsensusResult<std::collections::HashMap<crate::evm::EvmAddress, (u64, u128)>> {
        if addresses.is_empty() {
            return Ok(std::collections::HashMap::new());
        }
        let wanted: std::collections::HashSet<crate::evm::EvmAddress> = addresses.iter().copied().collect();
        let Some(s) = self.get_evm_state_snapshot_of(self.get_sink())? else {
            return Err(ConsensusError::General("no committed EVM state snapshot at the sink"));
        };
        let mut out = std::collections::HashMap::with_capacity(addresses.len());
        for acct in s.accounts {
            if wanted.contains(&acct.address) {
                out.insert(acct.address, (acct.nonce, acct.balance.try_to_u128().unwrap_or(u128::MAX)));
            }
        }
        Ok(out)
    }

    /// kaspa-pq EVM Lane v0.4 (§16): the EVM "block" (header + L1 hash + tx
    /// hashes) of the L1 chain block `l1_hash`, for `eth_getBlockBy*`. `None`
    /// if that block has no EVM header.
    fn get_evm_block_by_l1_hash(&self, _l1_hash: BlockHash) -> ConsensusResult<Option<crate::evm::EvmBlockResponse>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§9, `eth_subscribe("logs")`): ALL logs of the block
    /// at `l1_hash`, in block-global `logIndex` order, read straight from the
    /// immutable receipts store by hash WITHOUT canonical filtering — the WebSocket
    /// reorg pump must emit DETACHED blocks too (which the number map no longer
    /// points to). Empty for a non-EVM / unknown block.
    fn get_evm_block_logs(&self, _l1_hash: BlockHash) -> ConsensusResult<Vec<crate::evm::EvmLogEntry>> {
        Ok(Vec::new())
    }

    /// kaspa-pq EVM Lane v0.4 (§16, audit R-2): the raw EIP-2718 bytes of an EVM
    /// tx by hash (absent = never seen in a stored payload). Resolves
    /// `eth_getTransactionByHash`/receipt without the bounded included_in scan.
    fn get_evm_raw_tx(&self, _tx_hash: kaspa_hashes::EvmH256) -> ConsensusResult<Option<Vec<u8>>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16, `eth_getBlockByNumber`): the canonical EVM
    /// block at `evm_number` (reorg-validated — `None` if no canonical chain
    /// block currently holds that number).
    fn get_evm_block_by_number(&self, _evm_number: u64) -> ConsensusResult<Option<crate::evm::EvmBlockResponse>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16, `eth_getBlockByHash`): the EVM block whose
    /// eth-rpc 32-byte id (first 32 bytes of the 64-byte L1 hash) is `rpc_hash`.
    fn get_evm_block_by_rpc_hash(&self, _rpc_hash: kaspa_hashes::EvmH256) -> ConsensusResult<Option<crate::evm::EvmBlockResponse>> {
        Ok(None)
    }

    /// kaspa-pq EVM Lane v0.4 (§16, `eth_getLogs`): logs over the canonical
    /// `evm_number` range `[from_number, to_number]`, filtered by `addresses`
    /// (empty = any) and per-position `topics` (an empty inner vec = wildcard at
    /// that position). Bounded; only canonical chain blocks contribute.
    fn get_evm_logs(
        &self,
        _from_number: u64,
        _to_number: u64,
        _addresses: Vec<crate::evm::EvmAddress>,
        _topics: Vec<Vec<kaspa_hashes::EvmH256>>,
    ) -> ConsensusResult<Vec<crate::evm::EvmLogEntry>> {
        Ok(Vec::new())
    }

    /// kaspa-pq EVM Lane v0.4 (§3.1): the block's own `EvmExecutionPayload`
    /// (the bytes `evm_payload_hash` commits to). The payload store only holds
    /// rows for non-empty payloads, so absence maps to the empty payload —
    /// total for any block with a body. Used by the body-only IBD server so a
    /// served body can reassemble into a valid v2 block on the requester.
    fn get_block_evm_payload(&self, hash: BlockHash) -> ConsensusResult<crate::evm::EvmExecutionPayload> {
        unimplemented!()
    }

    fn get_block_even_if_header_only(&self, hash: BlockHash) -> ConsensusResult<Block> {
        unimplemented!()
    }

    fn get_ghostdag_data(&self, hash: BlockHash) -> ConsensusResult<ExternalGhostdagData> {
        unimplemented!()
    }

    fn get_block_children(&self, hash: BlockHash) -> Option<Vec<BlockHash>> {
        unimplemented!()
    }

    fn get_block_parents(&self, hash: BlockHash) -> Option<Arc<Vec<BlockHash>>> {
        unimplemented!()
    }

    fn get_block_status(&self, hash: BlockHash) -> Option<BlockStatus> {
        unimplemented!()
    }

    fn get_block_acceptance_data(&self, hash: BlockHash) -> ConsensusResult<Arc<AcceptanceData>> {
        unimplemented!()
    }

    /// Returns acceptance data for a set of blocks belonging to the selected parent chain.
    ///
    /// See `self::get_virtual_chain`
    fn get_blocks_acceptance_data(
        &self,
        hashes: &[BlockHash],
        merged_blocks_limit: Option<usize>,
    ) -> ConsensusResult<Vec<Arc<AcceptanceData>>> {
        unimplemented!()
    }

    fn is_chain_block(&self, hash: BlockHash) -> ConsensusResult<bool> {
        unimplemented!()
    }

    fn get_pruning_point_utxos(
        &self,
        expected_pruning_point: BlockHash,
        from_outpoint: Option<TransactionOutpoint>,
        chunk_size: usize,
        skip_first: bool,
    ) -> ConsensusResult<Vec<(TransactionOutpoint, UtxoEntry)>> {
        unimplemented!()
    }

    fn get_missing_block_body_hashes(&self, high: BlockHash) -> ConsensusResult<Vec<BlockHash>> {
        unimplemented!()
    }
    fn get_body_missing_anticone(&self) -> Vec<BlockHash> {
        unimplemented!()
    }
    fn clear_body_missing_anticone_set(&self) {
        unimplemented!()
    }

    fn pruning_point(&self) -> BlockHash {
        unimplemented!()
    }

    fn estimate_network_hashes_per_second(&self, start_hash: Option<BlockHash>, window_size: usize) -> ConsensusResult<u64> {
        unimplemented!()
    }

    fn validate_pruning_points(&self, syncer_virtual_selected_parent: BlockHash) -> ConsensusResult<()> {
        unimplemented!()
    }

    fn are_pruning_points_violating_finality(&self, pp_list: PruningPointsList) -> bool {
        unimplemented!()
    }

    fn creation_timestamp(&self) -> u64 {
        unimplemented!()
    }

    fn finality_point(&self) -> BlockHash {
        unimplemented!()
    }

    fn clear_pruning_utxo_set(&self) {
        unimplemented!()
    }

    fn set_pruning_utxoset_stable_flag(&self, val: bool) {
        unimplemented!()
    }

    fn is_pruning_utxoset_stable(&self) -> bool {
        unimplemented!()
    }

    fn is_pruning_point_anticone_fully_synced(&self) -> bool {
        unimplemented!()
    }

    fn is_consensus_in_transitional_ibd_state(&self) -> bool {
        unimplemented!()
    }

    fn intrusive_pruning_point_update(&self, new_pruning_point: BlockHash, syncer_sink: BlockHash) -> ConsensusResult<()> {
        unimplemented!()
    }

    /// Returns the n most recent pruning points (including the current pruning point)
    fn get_n_last_pruning_points(&self, n: usize) -> Vec<BlockHash> {
        unimplemented!()
    }
}

pub type DynConsensus = Arc<dyn ConsensusApi>;
