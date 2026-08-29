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
        header.pow_algo_id = kaspa_consensus_core::pow_layer0::required_algo_id_for_mode(
            self.params.palw_consensus_mode.required_algo_id(),
            self.params.pow_palw_ollama_activation.is_active(daa_window.daa_score),
            self.params.pow_palw_activation.is_active(daa_window.daa_score),
            self.params.pow_blake2b_sha3_activation.is_active(daa_window.daa_score),
        );
        header.timestamp = self.consensus.services.window_manager.calc_past_median_time(&ghostdag_data).unwrap().0 + 1;
        header.blue_score = ghostdag_data.blue_score;
        header.blue_work = ghostdag_data.blue_work;
        // ADR-0042 Decision 3a: on a `ConsensusV2` network the carriage is not optional — the
        // algo-6 tag IS `Expand(commitment_root)`, so a header without an envelope has no work to
        // price and `check_palw_commitment_shape` refuses it before GHOSTDAG. `skip_proof_of_work`
        // does not reach that gate (it skips the DIFFICULTY check, not the algorithm-id or shape
        // ones), so without this a V2 harness cannot build a chain at ALL and a test that tries
        // hangs rather than failing — which is exactly how this was found.
        //
        // The envelope is stamped LAST, after every field the challenge binds, because the
        // challenge is a function of the header's own position. Stamping it earlier would produce
        // a `PalwV2ChallengeMismatch` at the finalizer — the same refusal a re-mounted attempt
        // gets, which is the property it exists for.
        if header.pow_algo_id == kaspa_consensus_core::pow_layer0::POW_ALGO_ID_PALW_COMMITTED_V2 {
            header.palw_commitment = self.palw_v2_test_carriage(&header);
        }

        header
    }

    /// A position-bound `PalwAttemptEnvelopeV2` for `header`, as a miner on a V2 network would
    /// carry — the harness's answer to "what does a block of this network look like".
    ///
    /// **The signature is real, and the note that said it need not be was the hole.**
    ///
    /// It read: "the signature is a fixture: the finalizer does not read it (identity is the
    /// UNSIGNED attempt, ADR-0042 Decision 3c) and stateful admission — where the key and the bond
    /// are checked — is Unit C step 3's consumer, not this." Every clause of that was true and the
    /// conclusion did not follow: stateful admission checked the key against the bond and NOBODY
    /// checked the signature, because the pipeline called the entry point that takes no verifier.
    /// A harness carrying `vec![7u8; 32]` as an ML-DSA-87 key and `vec![0x5A; ..]` as its signature
    /// could not have noticed, which is why it did not.
    /// **The harness's own ML-DSA-87 identity, generated once.**
    ///
    /// The carriage calls itself a miner because it computes what the chain will demand rather
    /// than hard-coding it. It carried `executor_pubkey: vec![7u8; 32]` and
    /// `signature: vec![0x5A; ..]` — neither an ML-DSA-87 key nor an ML-DSA-87 signature — and
    /// every V2 pipeline test passed, because the pipeline called the admission entry point that
    /// takes no verifier. A harness that fabricates the one credential the chain is supposed to
    /// check is not measuring the chain.
    ///
    /// Generated once: ML-DSA-87 keygen is not cheap and every block of every V2 test needs the
    /// same identity — the genesis `BondRegistered` registers this key, and admission item 2
    /// compares the carried one against it.
    pub(crate) fn palw_v2_harness_keypair() -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        static KP: std::sync::OnceLock<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair> = std::sync::OnceLock::new();
        KP.get_or_init(|| libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0xB0u8; 32]))
    }

    /// **A real ML-DSA-87 identity per genesis-registry row.**
    ///
    /// The registry's non-executor rows carried four-byte placeholders, which no signature can
    /// verify against — so no fixture could ever sign a panel RECEIPT, which is one reason the
    /// missing `ReceiptLicensed` edge went unnoticed. Row `n` gets a distinct key, deterministically.
    /// Unused today. Kept as a pair with `palw_v2_registry_pubkey` so the V2 registry fixtures
    /// stay whole: a half-removed fixture set is how the next test hand-rolls a keypair.
    #[allow(dead_code)]
    pub(crate) fn palw_v2_registry_keypair(n: u64) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
        static KPS: std::sync::OnceLock<Vec<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>> = std::sync::OnceLock::new();
        let all = KPS.get_or_init(|| {
            (0..16u64)
                .map(|i| {
                    let mut seed = [0xB0u8; 32];
                    seed[0] = 0xB0u8.wrapping_add(i as u8);
                    libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed)
                })
                .collect()
        });
        &all[(n as usize) % all.len()]
    }

    /// The other half of the pair above.
    #[allow(dead_code)]
    pub(crate) fn palw_v2_registry_pubkey(n: u64) -> Vec<u8> {
        Self::palw_v2_registry_keypair(n).verification_key.as_ref().to_vec()
    }

    /// The verification key the genesis bond registers and the carriage carries — one value, so
    /// admission item 2's equality is a fact about the harness rather than a coincidence.
    pub(crate) fn palw_v2_harness_pubkey() -> Vec<u8> {
        Self::palw_v2_harness_keypair().verification_key.as_ref().to_vec()
    }

    /// A receipt-lane (algo-7) carriage for `header`, signed or not.
    ///
    /// `signed: false` is the attacker: a spend whose challenge binds this header correctly and
    /// whose signature is the right LENGTH and nothing else. That is precisely what
    /// `validate_stateless_v3` accepts, and for a while it was all any node checked.
    #[allow(dead_code)]
    pub(crate) fn palw_v3_test_receipt_carriage(&self, header: &Header, signed: bool) -> Vec<u8> {
        use kaspa_consensus_core::palw_freeprompt_v3::{
            PALW_FP_V3_MLDSA87_SPEND_CONTEXT, PALW_FP_V3_VERSION, PalwReceiptSpendEnvelopeV3, PalwReceiptSpendUnsignedV3,
            fp_spend_id_v3, spend_challenge_v3,
        };
        use kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for;
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        let network_id = self.params.net.to_string();
        // The genesis-bound domain (audit M2-18) — the harness must sign under exactly what the
        // header processor and the virtual processor verify under, or it proves nothing.
        let network_domain = palw_network_domain_v2_for(network_id.as_bytes(), Some(self.params.genesis.hash));
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let claim_id = kaspa_hashes::Hash64::from_u64_word(0xFC);
        let quantum_index = 0u32;
        let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0);
        let kp = Self::palw_v2_harness_keypair();
        let spend = PalwReceiptSpendUnsignedV3 {
            version: PALW_FP_V3_VERSION,
            network_domain,
            challenge: spend_challenge_v3(network_domain, pre_pow, header.timestamp, header.nonce, claim_id, quantum_index, &bond),
            claim_id,
            quantum_index,
            beacon_block: kaspa_hashes::Hash64::from_u64_word(0xBEAC),
            producer_bond: bond,
            producer_pubkey: Self::palw_v2_harness_pubkey(),
        };
        let signature = if signed {
            let message = fp_spend_id_v3(&spend);
            libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, message.as_byte_slice(), PALW_FP_V3_MLDSA87_SPEND_CONTEXT, [0u8; 32])
                .expect("the harness signs")
                .as_ref()
                .to_vec()
        } else {
            vec![0x5A; kaspa_consensus_core::dns_finality::STAKE_ATTESTATION_SIG_LEN]
        };
        PalwReceiptSpendEnvelopeV3 { spend, signature }.encode()
    }

    pub(crate) fn palw_v2_test_carriage(&self, header: &Header) -> Vec<u8> {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2, PalwAttemptUnsignedV2, challenge_v2, palw_network_domain_v2_for,
        };
        use kaspa_consensus_core::tx::{TransactionId, TransactionOutpoint};
        let network_id = self.params.net.to_string();
        let network_domain = palw_network_domain_v2_for(network_id.as_bytes(), Some(self.params.genesis.hash));
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let (class_id, class_target, pwu_per_inference) = match &self.params.palw_consensus_mode {
            kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) => {
                // The class's registered seed target and per-inference cost, read from the genesis
                // registration the bundle carries — the same two facts admission will re-derive
                // from chain state. Reading them from the bundle rather than hard-coding is what
                // keeps the harness a MINER: it computes what the chain will demand.
                let mut target = 0u128;
                let mut per_inference = 0u64;
                for object in &bundle.genesis_objects {
                    if let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::ClassRegistered {
                        class_id: id,
                        initial_target,
                        pwu_rule,
                        ..
                    } = object
                        && *id == bundle.base_class_id
                    {
                        target = *initial_target;
                        per_inference = match pwu_rule {
                            kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::DerivedV1 { pwu_per_inference } => *pwu_per_inference,
                            kaspa_consensus_core::palw_state_v2::PalwPwuRuleV2::MaxPerAttempt(cap) => *cap,
                        };
                    }
                }
                (bundle.base_class_id, target, per_inference)
            }
            _ => unreachable!("only called on a ConsensusV2 network"),
        };
        let bond = TransactionOutpoint::new(TransactionId::from_u64_word(0xB0), 0);
        let attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain,
            challenge: challenge_v2(network_domain, pre_pow, header.timestamp, header.nonce, class_id, &bond),
            class_id,
            executor_bond: bond,
            executor_pubkey: Self::palw_v2_harness_pubkey(),
            // DERIVED from the operator key, never a literal: `BondRegistered` mints the id with
            // `palw_operator_id_v2(operator_pubkey)`, and admission item 3 compares the carried id
            // against the registration's. A hand-written value here is an attempt no chain admits,
            // which is what the harness measured before this line existed.
            operator_id: kaspa_consensus_core::palw_state_v2::palw_operator_id_v2(&[21u8; 8]),
            artifact_root: kaspa_hashes::Hash64::from_u64_word(0xA7),
            trace_root: kaspa_hashes::Hash64::from_u64_word(0x7A),
            output_root: kaspa_hashes::Hash64::from_u64_word(0x00),
            execution_root: kaspa_hashes::Hash64::from_u64_word(0x4E),
            // DERIVED, not chosen: ADR-0045 Decision 1 makes admission item 6 an EQUALITY against
            // `palw_pwu_v1(class target at the candidate point, pwu_per_inference)`. Both factors
            // are chain state, so a miner picking a number is a miner refused — which is the whole
            // point of the derivation, and what this harness measured the moment it was wired.
            pwu: kaspa_consensus_core::palw_pwu::palw_pwu_v1(class_target, pwu_per_inference),
            trace_manifest_root: kaspa_hashes::Hash64::from_u64_word(0xD0),
            trace_chunk_count: 8,
            trace_retention_daa: u64::MAX,
        };
        let attempt = self.palw_v2_win_class_ticket(attempt, class_target);
        // Signed AFTER the ticket search, because the search moves fields that are inside
        // `attempt_id_v2` and the signature is over that id. Signing first would authorise an
        // attempt nobody mined.
        let message = kaspa_consensus_core::palw_attempt_v2::attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &Self::palw_v2_harness_keypair().signing_key,
            message.as_byte_slice(),
            kaspa_consensus_core::palw_attempt_v2::PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("ML-DSA-87 sign over a 64-byte attempt id")
        .as_ref()
        .to_vec();
        PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire()
    }

    /// The class LOTTERY, run the way a miner runs it (ADR-0039: "ticket, not hash").
    ///
    /// The network target decides whether a header is a block at all; the class target decides
    /// whether it is a block of THIS class, and `class_ticket_v2` is a function of the whole
    /// unsigned attempt — so a miner varies its `job_nonce` until the ticket lands under the
    /// class's target. The harness does exactly that, and it must: the alternative is a fixture
    /// that only ever produced losing tickets, which is what it did before this existed.
    ///
    /// Bounded, and the bound fails LOUDLY. A silent give-up would hand back a carriage the chain
    /// refuses, and the block would die at admission with no hint that the lottery was the reason.
    fn palw_v2_win_class_ticket(
        &self,
        mut attempt: kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2,
        class_target: u128,
    ) -> kaspa_consensus_core::palw_attempt_v2::PalwAttemptUnsignedV2 {
        // The ticket is a function of the whole unsigned attempt, and what a real executor varies
        // between tries is its EXECUTION — a different run yields a different trace root. The
        // harness stands in for that by varying the trace root, which is the same lever at the
        // same place; it does not touch the challenge (that binds the header position) or the
        // pwu (that is derived and checked for equality).
        for nonce in 0u64..1_000_000 {
            attempt.trace_root = kaspa_hashes::Hash64::from_u64_word(0x7A00_0000_0000_0000u64.wrapping_add(nonce));
            if kaspa_consensus_core::palw_attempt_v2::class_ticket_v2(&attempt) <= class_target {
                return attempt;
            }
        }
        panic!("the harness could not win the class lottery in 1e6 tries — the target is unreachably tight for a test")
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
