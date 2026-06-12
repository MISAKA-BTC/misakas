use async_channel::Sender;
use kaspa_consensus_core::BlockHash;
use kaspa_consensus_core::coinbase::MinerData;
use kaspa_consensus_core::dns_finality::p2pkh_mldsa87_spk;
use kaspa_consensus_core::mining_rules::MiningRules;
use kaspa_consensus_core::{
    api::ConsensusApi, block::MutableBlock, blockstatus::BlockStatus, header::Header, merkle::calc_hash_merkle_root,
    subnets::SUBNETWORK_ID_COINBASE, tx::Transaction,
};
use kaspa_consensus_notify::{notification::Notification, root::ConsensusNotificationRoot};
use kaspa_consensusmanager::{ConsensusFactory, ConsensusInstance, DynConsensusCtl};
use kaspa_core::{core::Core, service::Service};
use kaspa_database::utils::DbLifetime;
use kaspa_notify::subscription::context::SubscriptionContext;
use parking_lot::RwLock;

use super::Consensus;
use super::services::{DbDagTraversalManager, DbGhostdagManager, DbWindowManager};
use crate::pipeline::virtual_processor::test_block_builder::TestBlockBuilder;
use crate::processes::window::WindowManager;
use crate::{
    config::Config,
    constants::TX_VERSION,
    errors::BlockProcessResult,
    model::{
        services::reachability::MTReachabilityService,
        stores::{
            DB, ghostdag::DbGhostdagStore, headers::HeaderStoreReader, reachability::DbReachabilityStore, virtual_state::VirtualStores,
        },
    },
    params::Params,
    pipeline::{ProcessingCounters, body_processor::BlockBodyProcessor, virtual_processor::VirtualStateProcessor},
    test_helpers::header_from_precomputed_hash,
};
use kaspa_database::create_temp_db;
use kaspa_database::prelude::ConnBuilder;
use std::future::Future;
use std::{sync::Arc, thread::JoinHandle};

pub struct TestConsensus {
    params: Params,
    consensus: Arc<Consensus>,
    block_builder: TestBlockBuilder,
    _db_lifetime: DbLifetime,
}

impl TestConsensus {
    /// Creates a test consensus instance based on `config` with the provided `db` and `notification_sender`
    pub fn with_db(db: Arc<DB>, config: &Config, notification_sender: Sender<Notification>) -> Self {
        let notification_root = Arc::new(ConsensusNotificationRoot::new(notification_sender));
        let counters = Default::default();
        let tx_script_cache_counters = Default::default();
        let consensus = Arc::new(Consensus::new(
            db,
            Arc::new(config.clone()),
            Default::default(),
            notification_root,
            counters,
            tx_script_cache_counters,
            0,
            Arc::new(MiningRules::default()),
        ));
        let block_builder = TestBlockBuilder::new(consensus.virtual_processor.clone());

        Self { params: config.params.clone(), consensus, block_builder, _db_lifetime: Default::default() }
    }

    /// Creates a test consensus instance based on `config` with a temp DB and the provided `notification_sender`
    pub fn with_notifier(config: &Config, notification_sender: Sender<Notification>, context: SubscriptionContext) -> Self {
        let (db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let notification_root = Arc::new(ConsensusNotificationRoot::with_context(notification_sender, context));
        let counters = Default::default();
        let tx_script_cache_counters = Default::default();
        let consensus = Arc::new(Consensus::new(
            db,
            Arc::new(config.clone()),
            Default::default(),
            notification_root,
            counters,
            tx_script_cache_counters,
            0,
            Arc::new(MiningRules::default()),
        ));
        let block_builder = TestBlockBuilder::new(consensus.virtual_processor.clone());

        Self { consensus, block_builder, params: config.params.clone(), _db_lifetime: db_lifetime }
    }

    /// Creates a test consensus instance based on `config` with a temp DB and no notifier
    pub fn new(config: &Config) -> Self {
        let (db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
        let (dummy_notification_sender, _) = async_channel::unbounded();
        let notification_root = Arc::new(ConsensusNotificationRoot::new(dummy_notification_sender));
        let counters = Default::default();
        let tx_script_cache_counters = Default::default();
        let consensus = Arc::new(Consensus::new(
            db,
            Arc::new(config.clone()),
            Default::default(),
            notification_root,
            counters,
            tx_script_cache_counters,
            0,
            Arc::new(MiningRules::default()),
        ));
        let block_builder = TestBlockBuilder::new(consensus.virtual_processor.clone());

        Self { consensus, block_builder, params: config.params.clone(), _db_lifetime: db_lifetime }
    }

    /// Clone the inner consensus Arc. For general usage of the underlying consensus simply deref
    pub fn consensus_clone(&self) -> Arc<Consensus> {
        self.consensus.clone()
    }

    pub fn params(&self) -> &Params {
        &self.params
    }

    pub fn build_header_with_parents(&self, hash: BlockHash, parents: Vec<BlockHash>) -> Header {
        let mut header = header_from_precomputed_hash(hash, parents.clone());
        let parents_by_level = self.consensus.services.parents_manager.calc_block_parents(self.pruning_point(), &parents);
        header.parents_by_level = parents_by_level;
        let ghostdag_data = self.consensus.services.ghostdag_manager.ghostdag(header.direct_parents());
        let daa_window = self.consensus.services.window_manager.block_daa_window(&ghostdag_data).unwrap();
        header.bits = self.consensus.services.window_manager.calculate_difficulty_bits(&ghostdag_data, &daa_window);
        header.daa_score = daa_window.daa_score;
        // kaspa-pq ADR-0007 Phase 3: declare the algo the network mandates at
        // this DAA score (`header_from_precomputed_hash` defaults to the
        // Phase-1 kHeavyHash id, which `check_pow_algo_id` rejects on the
        // BLAKE2b-SHA3-active mainnet/testnet params).
        header.pow_algo_id = kaspa_consensus_core::pow_layer0::required_algo_id(
            self.params.pow_blake2b_sha3_activation.is_active(daa_window.daa_score),
        );
        header.timestamp = self.consensus.services.window_manager.calc_past_median_time(&ghostdag_data).unwrap().0 + 1;
        header.blue_score = ghostdag_data.blue_score;
        header.blue_work = ghostdag_data.blue_work;

        header
    }

    pub fn add_header_only_block_with_parents(
        &self,
        hash: BlockHash,
        parents: Vec<BlockHash>,
    ) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.validate_and_insert_block(self.build_header_only_block_with_parents(hash, parents).to_immutable()).virtual_state_task
    }

    /// Adds a valid block with the given transactions and parents to the consensus.
    ///
    /// # Panics
    ///
    /// Panics if block builder validation rules are violated.
    /// See `kaspa_consensus_core::errors::block::RuleError` for the complete list of possible validation rules.
    pub fn add_utxo_valid_block_with_parents(
        &self,
        hash: BlockHash,
        parents: Vec<BlockHash>,
        txs: Vec<Transaction>,
    ) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        // kaspa-pq PQ-only: coinbase outputs (and any reward derived from this block)
        // must be the standard ML-DSA-87 P2PKH class — see check_transaction_pq_output_classes.
        let miner_data = MinerData::new(p2pkh_mldsa87_spk(&[0u8; 64]), vec![]);
        self.validate_and_insert_block(self.build_utxo_valid_block_with_parents(hash, parents, miner_data, txs).to_immutable())
            .virtual_state_task
    }

    pub fn add_empty_utxo_valid_block_with_parents(
        &self,
        hash: BlockHash,
        parents: Vec<BlockHash>,
    ) -> impl Future<Output = BlockProcessResult<BlockStatus>> {
        self.add_utxo_valid_block_with_parents(hash, parents, vec![])
    }

    /// Builds a valid block with the given transactions, parents, and miner data.
    ///
    /// # Panics
    ///
    /// Panics if block builder validation rules are violated.
    /// See `kaspa_consensus_core::errors::block::RuleError` for the complete list of possible validation rules.
    pub fn build_utxo_valid_block_with_parents(
        &self,
        hash: BlockHash,
        parents: Vec<BlockHash>,
        miner_data: MinerData,
        txs: Vec<Transaction>,
    ) -> MutableBlock {
        let mut template = self.block_builder.build_block_template_with_parents(parents, miner_data, txs).unwrap();
        template.block.header.hash = hash;
        template.block
    }

    pub fn build_block_with_parents_and_transactions(
        &self,
        hash: BlockHash,
        parents: Vec<BlockHash>,
        mut txs: Vec<Transaction>,
    ) -> MutableBlock {
        let mut header = self.build_header_with_parents(hash, parents);
        // kaspa-pq PQ-only: encode an ML-DSA-87 P2PKH miner script in the coinbase
        // payload so that if this block is rewarded in a merging block's coinbase, the
        // reward output is the standard class and passes check_transaction_pq_output_classes.
        // (The coinbase itself carries no outputs, so the block is still disqualified at
        // coinbase verification, which is what these tests exercise.)
        let miner_spk = p2pkh_mldsa87_spk(&[0u8; 64]);
        let miner_script = miner_spk.script();
        let cb_payload: Vec<u8> = header.blue_score.to_le_bytes().iter().copied() // Blue score
            .chain(self.consensus.services.coinbase_manager.calc_block_subsidy(header.daa_score).to_le_bytes().iter().copied()) // Subsidy
            .chain((0_u16).to_le_bytes().iter().copied()) // Script public key version
            .chain((miner_script.len() as u8).to_le_bytes().iter().copied()) // Script public key length
            .chain(miner_script.iter().copied()) // Script public key
            .collect();

        let cb = Transaction::new(TX_VERSION, vec![], vec![], 0, SUBNETWORK_ID_COINBASE, 0, cb_payload);
        txs.insert(0, cb);
        header.hash_merkle_root = calc_hash_merkle_root(txs.iter());
        MutableBlock::new(header, txs)
    }

    pub fn build_header_only_block_with_parents(&self, hash: BlockHash, parents: Vec<BlockHash>) -> MutableBlock {
        MutableBlock::from_header(self.build_header_with_parents(hash, parents))
    }

    pub fn init(&self) -> Vec<JoinHandle<()>> {
        self.consensus.run_processors()
    }

    pub fn shutdown(&self, wait_handles: Vec<JoinHandle<()>>) {
        self.consensus.shutdown(wait_handles)
    }

    pub fn window_manager(&self) -> &DbWindowManager {
        &self.consensus.services.window_manager
    }

    pub fn dag_traversal_manager(&self) -> &DbDagTraversalManager {
        &self.consensus.services.dag_traversal_manager
    }

    pub fn ghostdag_store(&self) -> &Arc<DbGhostdagStore> {
        &self.consensus.ghostdag_store
    }

    pub fn reachability_store(&self) -> &Arc<RwLock<DbReachabilityStore>> {
        &self.consensus.reachability_store
    }

    pub fn reachability_service(&self) -> &MTReachabilityService<DbReachabilityStore> {
        &self.consensus.services.reachability_service
    }

    pub fn headers_store(&self) -> Arc<impl HeaderStoreReader> {
        self.consensus.headers_store.clone()
    }

    pub fn virtual_stores(&self) -> Arc<RwLock<VirtualStores>> {
        self.consensus.virtual_stores.clone()
    }

    pub fn processing_counters(&self) -> &Arc<ProcessingCounters> {
        self.consensus.processing_counters()
    }

    pub fn block_body_processor(&self) -> &Arc<BlockBodyProcessor> {
        &self.consensus.body_processor
    }

    pub fn virtual_processor(&self) -> &Arc<VirtualStateProcessor> {
        &self.consensus.virtual_processor
    }

    pub fn ghostdag_manager(&self) -> &DbGhostdagManager {
        &self.consensus.services.ghostdag_manager
    }
}

impl std::ops::Deref for TestConsensus {
    type Target = Arc<Consensus>;

    fn deref(&self) -> &Self::Target {
        &self.consensus
    }
}

impl Service for TestConsensus {
    fn ident(self: Arc<TestConsensus>) -> &'static str {
        "test-consensus"
    }

    fn start(self: Arc<TestConsensus>, _core: Arc<Core>) -> Vec<JoinHandle<()>> {
        self.init()
    }

    fn stop(self: Arc<TestConsensus>) {
        self.consensus.signal_exit()
    }
}

/// A factory which always returns the same consensus instance. Does not support the staging API.
pub struct TestConsensusFactory {
    tc: Arc<TestConsensus>,
}

impl TestConsensusFactory {
    pub fn new(tc: Arc<TestConsensus>) -> Self {
        Self { tc }
    }
}

impl ConsensusFactory for TestConsensusFactory {
    fn new_active_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        let ci = ConsensusInstance::new(self.tc.session_lock(), self.tc.consensus_clone());
        (ci, self.tc.consensus_clone() as DynConsensusCtl)
    }

    fn new_staging_consensus(&self) -> (ConsensusInstance, DynConsensusCtl) {
        unimplemented!()
    }

    fn close(&self) {
        self.tc.notification_root().close();
    }

    fn delete_inactive_consensus_entries(&self) {
        unimplemented!()
    }

    fn delete_staging_entry(&self) {
        unimplemented!()
    }
}
