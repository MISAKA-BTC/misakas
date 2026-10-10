//! Actual public-bond service -> PQ mempool -> mining template -> consensus DA terminal.
//! Harness-only keys/premine, inert EVM, skipped PoW and a test-armed dormant filer fence.
//! Court windows, collateral and registration rules retain their preset values.
//! The shipping activation validator still rejects the fixture fence; this is no network activation.
use super::*;
use kaspa_consensus::consensus::test_consensus::{TestConsensus, TestConsensusFactory};
use kaspa_consensus_core::block::TemplateBuildMode;
use kaspa_consensus_core::block::TemplateTransactionSelector;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::config::params::{PALW_T12_GENESIS_BONDS, Params};
use kaspa_consensus_core::config::premine::{
    MAIN_PREMINE_INDEX, PALW_RC_BOND_FEE_FLOAT_SOMPI, PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI, genesis_premine_utxos_for,
    premine_outpoint_for,
};
use kaspa_consensus_core::muhash::MuHashExtensions;
use kaspa_consensus_core::network::{NetworkId, NetworkType};
use kaspa_consensus_core::palw_mode_v2::{PalwConsensusMode, PalwConsensusParamsV2};
use kaspa_consensus_core::palw_state_v2::{
    PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT, PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT, PalwClaimPhaseV2, PalwLegacyDisputeViewV1,
    PalwVoidReasonV2, palw_bond_registration_message_v2, palw_operator_possession_message_v1,
};
use kaspa_consensus_core::tx::ScriptPublicKey;
use kaspa_consensus_core::{
    api::ConsensusApi,
    block::{Block, MutableBlock},
    coinbase::MinerData,
};
use kaspa_core::task::tick::TickService;
use kaspa_database::{create_temp_db, prelude::ConnBuilder};
use kaspa_mining::AttestationMempoolPolicy;
use kaspa_mining::manager::{MiningManager, MiningManagerProxy};
use kaspa_mining::model::tx_query::TransactionQuery;
use kaspa_muhash::MuHash;
use kaspa_p2p_mining::rule_engine::MiningRuleEngine;
use std::{collections::BTreeSet, sync::Arc, thread::JoinHandle, time::Duration};

struct OnetimeTxSelector {
    txs: Option<Vec<Transaction>>,
    rejected: bool,
}

impl OnetimeTxSelector {
    fn new(txs: Vec<Transaction>) -> Self {
        Self { txs: Some(txs), rejected: false }
    }
}

impl TemplateTransactionSelector for OnetimeTxSelector {
    fn select_transactions(&mut self) -> Vec<Transaction> {
        // First call returns the fixed set; subsequent calls (the builder's
        // rejection re-selection loop) return empty so the loop terminates
        // instead of unwrapping `None`.
        self.txs.take().unwrap_or_default()
    }

    fn reject_selection(&mut self, _tx_id: kaspa_consensus_core::tx::TransactionId) {
        // Record the rejection so `is_successful` reports failure and
        // `build_block_template` surfaces the per-tx `RuleError` (instead of
        // panicking or silently dropping the tx).
        self.rejected = true;
    }

    fn is_successful(&self) -> bool {
        !self.rejected
    }
}

type Funding = (TransactionOutpoint, UtxoEntry);
const OUTSIDER: usize = 8;
fn key(card: usize) -> &'static libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair {
    static KEYS: std::sync::OnceLock<Vec<libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair>> = std::sync::OnceLock::new();
    &KEYS.get_or_init(|| {
        (0..9)
            .map(|i| {
                let mut seed = [0xB0; 32];
                seed[0] += i;
                libcrux_ml_dsa::ml_dsa_87::generate_key_pair(seed)
            })
            .collect()
    })[card]
}
fn card_pubkey(card: usize) -> Vec<u8> {
    key(card).verification_key.as_ref().to_vec()
}
fn card_payout_payload(card: usize) -> Hash64 {
    Hash64::from_bytes(kaspa_hashes::blake2b_512_address_payload(&card_pubkey(card)).as_bytes())
}
fn card_payout_spk(card: usize) -> ScriptPublicKey {
    signable_script(&card_pubkey(card))
}
fn signed(card: usize, message: &[u8], context: &[u8]) -> Vec<u8> {
    libcrux_ml_dsa::ml_dsa_87::sign(&key(card).signing_key, message, context, [0x5A; 32]).unwrap().as_ref().to_vec()
}
fn fixture_config() -> (Config, PalwConsensusParamsV2, Vec<Funding>, Vec<Funding>) {
    let mut params = Params::from(NetworkId::with_suffix(NetworkType::Testnet, 12));
    let PalwConsensusMode::ConsensusV2(bundle) = &mut params.palw_consensus_mode else { unreachable!() };
    let mut row = 0;
    for object in &mut bundle.genesis_objects {
        if let PalwConsensusObjectV2::BondRegistered { pubkey, payout_payload, .. } = object {
            *pubkey = card_pubkey(row);
            *payout_payload = card_payout_payload(row);
            row += 1;
        }
    }
    assert_eq!(row, 8);
    let mut premine: Vec<_> = genesis_premine_utxos_for(params.net).into_iter().collect();
    let mut floats: Vec<Option<Funding>> = vec![None; 9];
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    for (outpoint, entry) in &mut premine {
        for (i, card) in PALW_T12_GENESIS_BONDS.iter().enumerate() {
            if entry.amount == PALW_RC_BOND_FEE_FLOAT_SOMPI
                && entry.script_public_key == kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&card.payout_payload)
            {
                entry.script_public_key = card_payout_spk(i);
                floats[i] = Some((*outpoint, entry.clone()));
            }
        }
        if *outpoint == main {
            entry.script_public_key = card_payout_spk(OUTSIDER);
            floats[OUTSIDER] = Some((*outpoint, entry.clone()));
        }
    }
    let mut hash = MuHash::new();
    for (o, e) in &premine {
        hash.add_utxo(o, e);
    }
    params.genesis.utxo_commitment = hash.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    params.skip_proof_of_work = true;
    params.evm_activation_daa_score = u64::MAX;
    params.palw_model_evm = None;
    params.validate_palw_v2().expect("fixture keys alone preserve the validated preset");
    params.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
    params.sync_palw_legacy_public_filer_v1();
    assert!(params.validate_palw_v2().is_err(), "the shipping activation guard remains closed");
    let PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else { unreachable!() };
    let bundle = bundle.clone();
    (Config::new(params), bundle, premine, floats.into_iter().map(Option::unwrap).collect())
}
struct Chain {
    tc: Arc<TestConsensus>,
    handles: Vec<JoinHandle<()>>,
    config: Config,
    bundle: PalwConsensusParamsV2,
    bonds: Vec<PalwBondKeyV2>,
    network_domain: Hash64,
    time: u64,
    nonce: u64,
    history: Vec<Block>,
}
impl Drop for Chain {
    fn drop(&mut self) {
        self.tc.shutdown(std::mem::take(&mut self.handles));
    }
}
impl Chain {
    fn new(config: Config, bundle: PalwConsensusParamsV2, premine: &[Funding]) -> Self {
        let tc = Arc::new(TestConsensus::new(&config));
        let mut multiset = MuHash::new();
        tc.append_imported_pruning_point_utxos(premine, &mut multiset);
        tc.import_pruning_point_utxo_set(config.params.genesis.hash, multiset).unwrap();
        let handles = tc.init();
        let bonds = bundle
            .genesis_objects
            .iter()
            .filter_map(|o| if let PalwConsensusObjectV2::BondRegistered { bond, .. } = o { Some(*bond) } else { None })
            .collect();
        let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
            config.params.net.to_string().as_bytes(),
            Some(config.params.genesis.hash),
        );
        let time = config.params.genesis.timestamp;
        Self { tc, handles, config, bundle, bonds, network_domain, time, nonce: 0, history: vec![] }
    }
    fn daa(&self) -> u64 {
        self.tc.get_virtual_daa_score()
    }
    fn view(&self, claim: Hash64) -> PalwLegacyDisputeViewV1 {
        self.tc.palw_legacy_dispute_v1(claim).unwrap()
    }
    async fn insert(&mut self, block: MutableBlock) -> Block {
        let block = block.to_immutable();
        self.tc.validate_and_insert_block(block.clone()).virtual_state_task.await.expect("real pipeline accepts block");
        self.history.push(block.clone());
        block
    }
    fn stamp(&mut self, block: &mut MutableBlock) {
        self.time += self.config.params.target_time_per_block();
        self.nonce += 1;
        block.header.timestamp = self.time;
        block.header.nonce = self.nonce;
        block.header.finalize();
    }
    async fn heartbeat(&mut self, txs: Vec<Transaction>) -> Block {
        let mut template = self
            .tc
            .build_block_template(
                MinerData::new(card_payout_spk(0), vec![]),
                Box::new(OnetimeTxSelector::new(txs)),
                TemplateBuildMode::Standard,
            )
            .unwrap();
        self.stamp(&mut template.block);
        let (template, _) = self.tc.heartbeat_adapt_block_template(template).unwrap();
        self.time = self.time.max(template.block.header.timestamp);
        self.insert(template.block).await
    }
    async fn advance(&mut self, to: u64) {
        let start = self.daa();
        let cap = to.saturating_sub(start).saturating_mul(4).saturating_add(400);
        for n in 0..cap {
            if self.daa() >= to {
                return;
            }
            self.heartbeat(vec![]).await;
            if n % 250 == 249 {
                eprintln!("[service-public] heartbeat advance {} -> {to}", self.daa());
            }
        }
        panic!("heartbeat did not reach {to}, DAA {}", self.daa());
    }
    async fn send(
        &mut self,
        card: usize,
        object: &PalwConsensusObjectV2,
        funding: &mut Funding,
        outputs: &[TransactionOutput],
    ) -> Transaction {
        let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
            &self.config,
            key(card),
            object,
            funding.0,
            &funding.1,
            outputs,
            None,
            self.daa(),
        )
        .unwrap();
        self.tc.validate_mempool_transaction(&mut MutableTransaction::from_tx(tx.clone()), &Default::default()).unwrap();
        self.heartbeat(vec![tx.clone()]).await;
        self.heartbeat(vec![]).await;
        let index = tx.outputs.len() - 1;
        let output = &tx.outputs[index];
        *funding = (
            TransactionOutpoint::new(tx.id(), index as u32),
            UtxoEntry::new(output.value, output.script_public_key.clone(), self.daa(), false),
        );
        tx
    }
    async fn import_fresh(&mut self, premine: &[Funding]) {
        let mut fresh = Chain::new(self.config.clone(), self.bundle.clone(), premine);
        for block in &self.history {
            fresh
                .tc
                .validate_and_insert_block(block.clone())
                .virtual_state_task
                .await
                .expect("fresh DB validates public block history");
        }
        assert_eq!(fresh.daa(), self.daa());
        eprintln!("[service-public] fresh database validated {} public blocks at DAA {}", self.history.len(), fresh.daa());
        fresh.time = self.time;
        fresh.nonce = self.nonce;
        fresh.history = std::mem::take(&mut self.history);
        std::mem::swap(self, &mut fresh); // The old database and processors are shut down here.
    }
    fn build_attempt_drawn(
        &mut self,
        card: usize,
        step_ms: u64,
        txs: Vec<Transaction>,
        keep: &dyn Fn(Hash64) -> bool,
        win: bool,
    ) -> (MutableBlock, Hash64) {
        use kaspa_consensus_core::palw_attempt_v2::{
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT, PALW_ATTEMPT_V2_TRACE_CHUNKS, PALW_ATTEMPT_V2_VERSION, PalwAttemptEnvelopeV2,
            PalwAttemptUnsignedV2, attempt_id_v2, attempt_trace_manifest_root_v1, challenge_v2, class_ticket_v3, execution_anchor_v3,
            execution_commitment_v3,
        };
        self.time += step_ms;
        self.nonce += 1;

        let bond = self.bonds[card];
        let mut t = self
            .tc
            .build_block_template(
                MinerData::new(card_payout_spk(card), vec![]),
                Box::new(OnetimeTxSelector::new(txs)),
                TemplateBuildMode::Standard,
            )
            .expect("a template");
        assert!(
            kaspa_consensus_core::pow_layer0::is_palw_attempt_algo_id(t.block.header.pow_algo_id),
            "a ConsensusV2 template declares the attempt lane"
        );
        t.block.header.timestamp = self.time;
        t.block.header.nonce = self.nonce;
        let facts = self.tc.palw_producer_facts_v2(self.bundle.base_class_id, Some(bond.0)).expect("testnet-12 answers for its floor");
        let bond_facts = facts.bond.as_ref().expect("a genesis card is a registered bond").clone();
        assert_eq!(bond_facts.registered_pubkey, card_pubkey(card), "the chain registered card {card}'s harness key");
        facts.ready_to_produce(&card_pubkey(card)).unwrap_or_else(|why| panic!("card {card} is not ready to produce: {why}"));
        let header = &t.block.header;
        let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(header);
        let execution = Hash64::from_u64_word(0xE7EC_0000_0000_0000 | self.nonce);
        let mut attempt = PalwAttemptUnsignedV2 {
            version: PALW_ATTEMPT_V2_VERSION,
            network_domain: self.network_domain,
            challenge: challenge_v2(self.network_domain, pre_pow, header.timestamp, header.nonce, facts.class_id, &bond.0),
            class_id: facts.class_id,
            executor_bond: bond.0,
            executor_pubkey: card_pubkey(card),
            operator_id: bond_facts.operator_id,
            artifact_root: facts.artifact_root,
            trace_root: Hash64::default(),
            output_root: Hash64::from_u64_word(0x0070_0000_0000_0000 | self.nonce),
            execution_root: execution,
            pwu: facts.pwu,
            trace_manifest_root: Hash64::default(),
            trace_chunk_count: PALW_ATTEMPT_V2_TRACE_CHUNKS,
            trace_retention_daa: header.daa_score.saturating_add(facts.min_trace_retention_daa),
        };
        let anchor = execution_anchor_v3(self.network_domain, pre_pow, facts.class_id, &bond.0, header.nonce);
        let mut won = false;
        for draw in 0u64..4_000_000 {
            attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x7A00_0000_0000_0000);
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            let wins = class_ticket_v3(&attempt, anchor) <= facts.class_target;
            if wins == win && (!win || keep(execution_commitment_v3(&attempt, anchor))) {
                won = true;
                break;
            }
        }
        assert!(won, "the floor's class lottery is winnable (with the draw kept) — or losable, when a loss is asked for");
        let claim_id = attempt_id_v2(&attempt);
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(
            &key(card).signing_key,
            claim_id.as_byte_slice(),
            PALW_ATTEMPT_V2_MLDSA87_CONTEXT,
            [0x5Au8; 32],
        )
        .expect("ML-DSA-87 sign over a 64-byte attempt id")
        .as_ref()
        .to_vec();
        t.block.header.palw_commitment = PalwAttemptEnvelopeV2 { attempt, signature }.encode_wire();
        t.block.header.finalize();
        (t.block, claim_id)
    }
}

struct ServiceRun {
    service: Arc<PalwPanelService>,
    task: Option<tokio::task::JoinHandle<()>>,
}
impl ServiceRun {
    async fn finish(&mut self) {
        self.service.clone().signal_exit();
        tokio::time::timeout(Duration::from_secs(10), self.task.take().unwrap())
            .await
            .expect("service observes exit")
            .expect("service joins without panic");
        self.service.clone().stop().await.unwrap();
    }
}
impl Drop for ServiceRun {
    fn drop(&mut self) {
        self.service.shutdown.trigger.trigger();
        if let Some(task) = self.task.as_ref() {
            task.abort();
        }
    }
}
fn panel_config(chain: &Chain, dir: &std::path::Path, bond: PalwBondKeyV2, funding: &Funding) -> PalwPanelConfig {
    let key_path = dir.join("verifier.key");
    let mut seed = [0xB0; 32];
    seed[0] += OUTSIDER as u8;
    std::fs::write(&key_path, faster_hex::hex_string(&seed)).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&key_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    }
    assert_eq!(kaspa_pq_validator_core::load_validator_seed(key_path.to_str().unwrap()).unwrap(), seed);
    PalwPanelConfig {
        key_path: key_path.to_string_lossy().into_owned(),
        bond: format!("{}:{}", bond.0.transaction_id, bond.0.index),
        fee_outpoint: Some(format!("{}:{}", funding.0.transaction_id, funding.0.index)),
        state_dir: dir.join("state"),
        court: chain.bundle.court.clone(),
        prompt_ids_form: chain.config.params.palw_prompt_ids_form_v1(),
        class_artifacts: vec![],
        class_cache_bytes: 0,
        seat_replay_slots: Some(1),
        vertex_full_refs: false,
        vertex_equivocate_at: None,
        sketch_check: false,
        class_residency: misaka_palw_sdk::PalwWeightResidencyV1::PageCache,
        telemetry: Default::default(),
        challenge: false,
        canonical_claims: false,
        canonical_class: None,
        canonical_interval_daa: 100,
        drill_tamper_fp_leaf: None,
        drill_challenge_all: false,
        drill_answer_only: false,
        drill_refuse_leaf_evidence: false,
        improve_evaluate: false,
        tir_shard_hold: vec![],
        tir_shard_declare: None,
        tir_shard_gpu: false,
        tir_shard_shadow: false,
        tir_shard_demand_runs: false,
        tir_shard_watch: false,
        tir_shard_mirror: None,
        tir_shard_fetch: false,
        tir_shard_run_leaves: 0,
        improve_artifact_dir: None,
        root_fetch_cmd: None,
        root_drop_dir: None,
        improve_capture_dir: None,
        improve_tamper: None,
        retention_dir: dir.join("empty-retention"),
        evidence_provider_dirs: vec![],
        register_class: None,
        chain_classes: false,
        register_bond: false,
        bond_collateral: None,
        pay_address: None,
        producer_class: None,
    }
}
async fn pool_heartbeat(chain: &mut Chain, manager: &Arc<ConsensusManager>, pool: &MiningManagerProxy) -> Block {
    let session = manager.consensus().unguarded_session();
    let mut template = pool.clone().get_block_template(&session, MinerData::new(card_payout_spk(0), vec![])).await.unwrap();
    chain.stamp(&mut template.block);
    let (template, _) = chain.tc.heartbeat_adapt_block_template(template).unwrap();
    chain.time = chain.time.max(template.block.header.timestamp);
    let block = chain.insert(template.block).await;
    pool.clone().handle_new_block_transactions(&session, block.header.daa_score, block.transactions.clone()).await.unwrap();
    block
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_carries_reserved_da_to_objective_default() {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let (config, bundle, premine, mut floats) = fixture_config();
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    let signed_bond = PalwBondKeyV2(TransactionOutpoint::new(Default::default(), 0));
    let public_key = card_pubkey(OUTSIDER);
    let payout = card_payout_payload(OUTSIDER);
    let classes = BTreeSet::new();
    let collateral = PALW_T12_GENESIS_BOND_COLLATERAL_SOMPI;
    let message =
        palw_bond_registration_message_v2(chain.network_domain, &signed_bond, &public_key, &public_key, collateral, &payout, &classes);
    let possession = palw_operator_possession_message_v1(chain.network_domain, &signed_bond, &public_key, &public_key);
    let mut signature = signed(OUTSIDER, message.as_byte_slice(), PALW_BOND_REGISTRATION_V2_MLDSA87_CONTEXT);
    signature.extend(signed(OUTSIDER, possession.as_byte_slice(), PALW_OPERATOR_POSSESSION_MLDSA87_CONTEXT));
    let registration = PalwConsensusObjectV2::BondRegistered {
        bond: signed_bond,
        pubkey: public_key.clone(),
        operator_pubkey: public_key.clone(),
        collateral,
        payout_payload: payout,
        capable_classes: classes,
        signature,
    };
    let tx = chain
        .send(OUTSIDER, &registration, &mut floats[OUTSIDER], &[TransactionOutput::new(collateral, card_payout_spk(OUTSIDER))])
        .await;
    let bond = PalwBondKeyV2(TransactionOutpoint::new(tx.id(), 0));
    assert_eq!(chain.tc.palw_bond_of_pubkey_v2(&public_key).unwrap().0, bond);
    assert!(!chain.bonds.contains(&bond));
    let registered = chain
        .tc
        .palw_claim_rows_v1(bond, kaspa_consensus_core::palw_producer_v2::PalwClaimRoleV1::Executor, true, 1)
        .unwrap()
        .bond
        .unwrap();
    assert!(registered.registered_daa > 0, "no genesis maturity exception");
    assert!(registered.capable_classes.is_empty());
    let registered_before = chain.daa();
    let maturity = chain.config.params.palw_bond_maturity.unwrap().window_daa;
    chain.advance(registered_before + maturity + 1).await;
    eprintln!("[service-public] ordinary bond registered before DAA {registered_before}, maturity {maturity}, now {}", chain.daa());
    let (attempt, claim) = chain.build_attempt_drawn(0, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(attempt).await;
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    chain.advance(slot).await;
    let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(anchor).await;
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty());
    assert!(chain.tc.palw_seat_duties_v2(vec![bond]).is_empty());
    let signed_daa = chain.daa();
    let receipts = duties
        .iter()
        .map(|d| {
            let card = chain.bonds.iter().position(|b| *b == d.seat_bond).unwrap();
            let message = palw_receipt_message_v2(chain.network_domain, claim, PalwReceiptVerdictV2::Valid, signed_daa);
            PalwSeatReceiptV2 {
                claim,
                verdict: PalwReceiptVerdictV2::Valid,
                seat_bond: d.seat_bond,
                signed_daa,
                signature: signed(card, message.as_byte_slice(), PALW_RECEIPT_V2_MLDSA87_CONTEXT),
            }
        })
        .collect();
    let object = chain.tc.palw_v2_receipt_quorum_assemble(claim, receipts).expect("all colluding seats license");
    let card = chain.bonds.iter().position(|b| *b == duties[0].seat_bond).unwrap();
    chain.send(card, &object, &mut floats[card], &[]).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    chain.import_fresh(&premine).await;
    assert!(matches!(chain.view(claim).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert!(chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == claim && !c.seat));
    let manager = Arc::new(ConsensusManager::new(Arc::new(TestConsensusFactory::new(chain.tc.clone()))));
    let config = Arc::new(chain.config.clone());
    let tick = Arc::new(TickService::new());
    let (_meta_lifetime, meta_db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (addresses, _) = kaspa_addressmanager::AddressManager::new(config.clone(), meta_db, tick.clone());
    let pool = MiningManagerProxy::new(Arc::new(MiningManager::new_with_extended_config(
        config.target_time_per_block(),
        false,
        config.max_block_mass,
        config.ram_scale,
        config.block_template_cache_lifetime,
        Default::default(),
        true,
        config.palw_model_market.is_some(),
        config.palw_activation_pool_fence().is_some(),
        None,
        AttestationMempoolPolicy::disabled(),
        kaspa_mining::mempool::config::palw_h1_carrier_priority_for(&config.params),
    )));
    let hub = kaspa_p2p_lib::Hub::new();
    let rules = Arc::new(MiningRuleEngine::new(
        manager.clone(),
        config.clone(),
        Default::default(),
        tick.clone(),
        hub.clone(),
        Default::default(),
        Arc::new(kaspa_core::chain_participation::ChainParticipationGate::new(true)),
    ));
    let flow = Arc::new(FlowContext::new(
        manager.clone(),
        addresses,
        config.clone(),
        pool.clone(),
        tick,
        chain.tc.notification_root(),
        hub,
        rules,
    ));
    let retained_flow = flow.clone();
    let dir = tempfile::tempdir().unwrap();
    let service =
        Arc::new(PalwPanelService::new(panel_config(&chain, dir.path(), bond, &floats[OUTSIDER]), manager.clone(), flow, config));
    assert!(service.keypair.is_some());
    assert_eq!(service.bond, Some(bond.0));
    let cloned = service.clone();
    let task = tokio::spawn(async move {
        cloned.start().await.unwrap();
    });
    let mut running = ServiceRun { service: service.clone(), task: Some(task) };
    let deadline = std::time::Instant::now() + Duration::from_secs(360);
    let service_start = chain.history.len();
    let mut next_da_progress = chain.daa() + 100;
    let mut reserved = false;
    let mut demanded = false;
    loop {
        assert!(!running.task.as_ref().unwrap().is_finished(), "production worker unexpectedly stopped");
        assert!(std::time::Instant::now() < deadline, "service made no objective progress: {:?}", chain.view(claim));
        let (pending, _) = pool.clone().get_all_transactions(TransactionQuery::TransactionsOnly).await;
        if !pending.is_empty() {
            let ids = pending.iter().map(|t| t.tx.id()).collect::<Vec<_>>();
            for tx in &pending {
                if let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.tx.payload) {
                    eprintln!(
                        "[service-public] real mempool carrier {:?}, {} bytes",
                        std::mem::discriminant(&payload.object),
                        tx.tx.payload.len()
                    );
                }
            }
            let block = pool_heartbeat(&mut chain, &manager, &pool).await;
            assert!(
                ids.iter().any(|id| block.transactions.iter().any(|tx| tx.id() == *id)),
                "production template includes an actual service carrier"
            );
            pool_heartbeat(&mut chain, &manager, &pool).await;
        }
        let view = chain.view(claim);
        if chain.daa() >= next_da_progress {
            eprintln!(
                "[service-public] DA advance {}, deadlines {:?}",
                chain.daa(),
                view.sessions.iter().map(|(_, s)| s.deadline_daa).collect::<Vec<_>>()
            );
            next_da_progress = chain.daa() + 100;
        }
        reserved |= view.record.as_ref().is_some_and(|r| r.live.contains_key(&bond));
        demanded |= view.sessions.iter().any(|(accuser, _)| *accuser == bond);
        if view.da_defaulted {
            assert!(matches!(view.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
            let carriers = chain.history[service_start..]
                .iter()
                .flat_map(|b| b.transactions.iter())
                .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
                .map(|p| p.object)
                .collect::<Vec<_>>();
            assert!(carriers.iter().any(|o|matches!(o,PalwConsensusObjectV2::DisputeReservedV1 {reservation,..} if reservation.claim==claim && reservation.reserver==bond)),"the actual blocks include this verifier reservation");
            assert!(
                carriers
                    .iter()
                    .any(|o| matches!(o,PalwConsensusObjectV2::DefaultAccused {claim:c,accuser,..} if *c==claim && *accuser==bond)),
                "the actual blocks include this verifier DA demand"
            );
            let carried = carriers.len();
            assert!(reserved && demanded && carried >= 2);
            let offence = kaspa_consensus_core::palw_da_rcore_v1::palw_da_offence_id_v1(&view.producer.0, &claim);
            let filing = chain.tc.palw_reporter_filing_read_v1(offence, Hash64::default(), bond, None).unwrap();
            let consumed = filing.consumed.as_ref().unwrap();
            assert_eq!(consumed.accused, view.producer.0);
            assert_eq!(consumed.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::DaDefault);
            assert!(consumed.amount > 0, "default collected the producer sanction");
            assert!(
                chain.tc.palw_fraud_filer_status_v1(bond).unwrap().live.iter().all(|r| r.claim_id != claim),
                "objective outcome releases the public reservation"
            );
            eprintln!(
                "[service-public] objective DA default at {} after {carried} mined service batches, no producer material",
                chain.daa()
            );
            break;
        }
        if demanded {
            for _ in 0..50 {
                pool_heartbeat(&mut chain, &manager, &pool).await;
            }
        } else if reserved {
            for _ in 0..10 {
                pool_heartbeat(&mut chain, &manager, &pool).await;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    running.finish().await;
    let weak = Arc::downgrade(&service);
    let authorizer = service.opening_authorizer(chain.network_domain);
    drop(running);
    drop(service);
    assert!(weak.upgrade().is_none(), "gossip callbacks must not retain the stopped service");
    let request = kaspa_p2p_flows::palw_gossip::PalwOpeningRequestV1 {
        claim,
        interval_index: None,
        leaf_index: None,
        requester_pubkey: &[],
        requested_daa: chain.daa(),
        signature: &[],
    };
    assert_eq!(authorizer(&request), Err(kaspa_p2p_flows::palw_gossip::PalwServeRefusalV1::NotServing));
    drop(retained_flow);
    eprintln!("[service-public] actual service start/exit joined, callback owner released");
}
