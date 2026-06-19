use crate::{
    BlockHash, BlueWorkType,
    coinbase::MinerData,
    evm::EvmExecutionPayload,
    header::Header,
    tx::{Transaction, TransactionId, TransactionOutpoint},
};
// PR-9.5e: all block-hash surfaces here (`Block::hash`, selected
// parent, sink, precomputed-hash test ctor) use `BlockHash` (= `Hash64`).
use kaspa_utils::mem_size::MemSizeEstimator;
use std::sync::Arc;

/// A mutable block structure where header and transactions within can still be mutated.
#[derive(Debug, Clone)]
pub struct MutableBlock {
    pub header: Header,
    pub transactions: Vec<Transaction>,
    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): EVM execution payload.
    /// Empty/default for pre-EVM (v0/v1) templates.
    pub evm_payload: EvmExecutionPayload,
}

impl MutableBlock {
    pub fn new(header: Header, txs: Vec<Transaction>) -> Self {
        Self { header, transactions: txs, evm_payload: EvmExecutionPayload::default() }
    }

    pub fn from_header(header: Header) -> Self {
        Self::new(header, vec![])
    }

    pub fn to_immutable(self) -> Block {
        Block::new(self.header, self.transactions).with_evm_payload(Arc::new(self.evm_payload))
    }
}

/// A block structure where the inner header and transactions are wrapped by Arcs for
/// cheap cloning and for cross-thread safety and immutability. Note: no need to wrap
/// this struct with an additional Arc.
#[derive(Debug, Clone)]
pub struct Block {
    pub header: Arc<Header>,
    pub transactions: Arc<Vec<Transaction>>,
    /// kaspa-pq Selected-Parent EVM Lane (ADR-0020): EVM execution payload,
    /// carried separately from the UTXO `transactions` because EVM txs become
    /// canonical only when their block enters the selected-parent chain. Empty
    /// (`EvmExecutionPayload::default()`) for pre-EVM (v0/v1) blocks; required
    /// non-trivial only past activation. Committed via the header's `evm_*_root`
    /// fields, NOT via `hash_merkle_root` (which covers `transactions` only).
    pub evm_payload: Arc<EvmExecutionPayload>,
}

impl Block {
    pub fn new(header: Header, txs: Vec<Transaction>) -> Self {
        Self {
            header: Arc::new(header),
            transactions: Arc::new(txs),
            evm_payload: Arc::new(EvmExecutionPayload::default()),
        }
    }

    pub fn from_arcs(header: Arc<Header>, transactions: Arc<Vec<Transaction>>) -> Self {
        Self { header, transactions, evm_payload: Arc::new(EvmExecutionPayload::default()) }
    }

    pub fn from_header_arc(header: Arc<Header>) -> Self {
        Self { header, transactions: Arc::new(Vec::new()), evm_payload: Arc::new(EvmExecutionPayload::default()) }
    }

    pub fn from_header(header: Header) -> Self {
        Self {
            header: Arc::new(header),
            transactions: Arc::new(Vec::new()),
            evm_payload: Arc::new(EvmExecutionPayload::default()),
        }
    }

    /// kaspa-pq ADR-0020: attach an EVM execution payload (consuming builder).
    /// Used by the block-decode paths (P2P / RPC) and the EVM mining template so
    /// a non-empty payload round-trips; all base constructors default to empty.
    pub fn with_evm_payload(mut self, evm_payload: Arc<EvmExecutionPayload>) -> Self {
        self.evm_payload = evm_payload;
        self
    }

    pub fn is_header_only(&self) -> bool {
        self.transactions.is_empty()
    }

    pub fn hash(&self) -> BlockHash {
        self.header.hash
    }

    /// WARNING: To be used for test purposes only
    pub fn from_precomputed_hash(hash: BlockHash, parents: Vec<BlockHash>) -> Block {
        Block::from_header(Header::from_precomputed_hash(hash, parents))
    }

    /// Check if the block in-memory size is too large to be cached as a pending-validation orphan block.
    /// Returns None if the block is too large
    pub fn asses_for_cache(&self) -> Option<()> {
        (self.estimate_mem_bytes() < 1_000_000).then_some(())
    }
}

impl MemSizeEstimator for Block {
    fn estimate_mem_bytes(&self) -> usize {
        // Calculates mem bytes of the block (for cache tracking purposes)
        size_of::<Self>()
            + self.header.estimate_mem_bytes()
            + size_of::<Vec<Transaction>>()
            + self.transactions.iter().map(Transaction::estimate_mem_bytes).sum::<usize>()
            // ADR-0020: account for the EVM payload (empty for pre-activation blocks).
            + self.evm_payload.estimate_mem_bytes()
    }
}

/// An abstraction for a recallable transaction selector with persistent state
pub trait TemplateTransactionSelector {
    /// Expected to return a batch of transactions which were not previously selected.
    /// The batch will typically contain sufficient transactions to fill the block
    /// mass (along with the previously unrejected txs), or will drain the selector    
    fn select_transactions(&mut self) -> Vec<Transaction>;

    /// Should be used to report invalid transactions obtained from the *most recent*
    /// `select_transactions` call. Implementors should use this call to internally
    /// track the selection state and discard the rejected tx from internal occupation calculations
    fn reject_selection(&mut self, tx_id: TransactionId);

    /// Determine whether this was an overall successful selection episode
    fn is_successful(&self) -> bool;
}

/// Block template build mode
#[derive(Clone, Copy, Debug)]
pub enum TemplateBuildMode {
    /// Block template build can possibly fail if `TemplateTransactionSelector::is_successful` deems the operation unsuccessful.
    ///
    /// In such a case, the build fails with `BlockRuleError::InvalidTransactionsInNewBlock`.
    Standard,

    /// Block template build always succeeds. The built block contains only the validated transactions.
    Infallible,
}

/// kaspa-pq EVM Lane (§9.2): why the template path could not include a queued
/// deposit claim when it re-validated the claim against the live selected-parent
/// claim view. Drives the mining manager's claim-queue reconciliation so that a
/// merely-not-yet-visible lock is retried instead of being evicted on sight.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvmClaimStaleKind {
    /// The lock outpoint is PRESENT in the live view but the claim can never
    /// execute against it (the refund window has opened, or a lock field no
    /// longer matches the claim). Terminal — the queue entry is evicted at once.
    Invalid,
    /// The lock outpoint is ABSENT from the live view. Usually transient: the
    /// deposit-lock's block is not yet on this node's selected chain (a lagging
    /// miner or a forky DAG), or the lock was just consumed by an accepted block.
    /// The queue entry is RETAINED and retried, and evicted only after it stays
    /// absent for many consecutive templates (so a consumed / never-confirmed
    /// lock is still reaped, but a transient lag resolves long before).
    Absent,
}

/// A block template for miners.
#[derive(Debug, Clone)]
pub struct BlockTemplate {
    pub block: MutableBlock,
    pub miner_data: MinerData,
    pub coinbase_has_red_reward: bool,
    /// Coinbase outputs whose scripts are bound to `miner_data.script_public_key`.
    pub coinbase_miner_script_output_indices: Vec<usize>,
    pub selected_parent_timestamp: u64,
    pub selected_parent_daa_score: u64,
    pub selected_parent_hash: BlockHash,
    /// Expected length is one less than txs length due to lack of coinbase transaction
    pub calculated_fees: Vec<u64>,
    /// kaspa-pq EVM Lane (§9.2): deposit claims the template path could not
    /// include when re-validated against the live claim view, each tagged with
    /// [`EvmClaimStaleKind`] so the mining manager reconciles its claim queue
    /// correctly — `Invalid` claims are evicted at once, while `Absent` (lock not
    /// yet on this node's selected chain) ones are retained and retried.
    pub stale_evm_claims: Vec<(TransactionOutpoint, EvmClaimStaleKind)>,
}

impl BlockTemplate {
    pub fn new(
        block: MutableBlock,
        miner_data: MinerData,
        coinbase_has_red_reward: bool,
        coinbase_miner_script_output_indices: Vec<usize>,
        selected_parent_timestamp: u64,
        selected_parent_daa_score: u64,
        selected_parent_hash: BlockHash,
        calculated_fees: Vec<u64>,
        stale_evm_claims: Vec<(TransactionOutpoint, EvmClaimStaleKind)>,
    ) -> Self {
        Self {
            block,
            miner_data,
            coinbase_has_red_reward,
            coinbase_miner_script_output_indices,
            selected_parent_timestamp,
            selected_parent_daa_score,
            selected_parent_hash,
            calculated_fees,
            stale_evm_claims,
        }
    }

    pub fn to_virtual_state_approx_id(&self) -> VirtualStateApproxId {
        VirtualStateApproxId::new(self.block.header.daa_score, self.block.header.blue_work, self.selected_parent_hash)
    }
}

/// An opaque data structure representing a unique approximate identifier for virtual state. Note that it is
/// approximate in the sense that in rare cases a slightly different virtual state might produce the same identifier,
/// hence it should be used for cache-like heuristics only
#[derive(PartialEq)]
pub struct VirtualStateApproxId {
    daa_score: u64,
    blue_work: BlueWorkType,
    sink: BlockHash,
}

impl VirtualStateApproxId {
    pub fn new(daa_score: u64, blue_work: BlueWorkType, sink: BlockHash) -> Self {
        Self { daa_score, blue_work, sink }
    }
}
