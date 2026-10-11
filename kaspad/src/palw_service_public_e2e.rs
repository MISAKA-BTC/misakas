//! Actual public-bond service -> PQ mempool -> mining template -> consensus terminal.
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
use kaspa_consensus_core::palw_backend::PalwExecutionBackendV1;
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
fn fixture_config(
    fp_v4: bool,
    fp_input_bootstrap: bool,
    before_job_retention: bool,
    constraint_v2: bool,
) -> (Config, PalwConsensusParamsV2, Vec<Funding>, Vec<Funding>) {
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
    if fp_v4 || constraint_v2 {
        // Exercise the admitted V4 family under the normal deployable decode-rules fence.
        // No public-prosecution eligibility/admission callback is substituted.
        params.palw_fp_decode_rules = Some(ForkActivation::new(0));
        params.sync_palw_fp_decode_rules();
    }
    if constraint_v2 {
        params.palw_fp_decode_constraint = Some(ForkActivation::new(0));
        params.sync_palw_fp_decode_constraint_v1();
        params.palw_fp_constraint_v2 = Some(ForkActivation::new(0));
    }
    params.validate_palw_v2().expect("fixture keys and the ordinary decode-rules fence validate");
    params.palw_legacy_public_filer_v1 = Some(ForkActivation::new(if before_job_retention { 1060 } else { 0 }));
    params.sync_palw_legacy_public_filer_v1();
    if fp_input_bootstrap {
        // Reacquiring omitted/pruned IDs uses LG14-B's binding/PromptIds bootstrap.
        // Both dormant halves of the public route are test-armed; production validation stays closed.
        params.palw_legacy_held_da_v2 = Some(ForkActivation::new(0));
    }
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
        let mut validated = MutableTransaction::from_tx(tx);
        self.tc.validate_mempool_transaction(&mut validated, &Default::default()).unwrap();
        let tx = validated.tx.as_ref().clone();
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
        self.build_attempt_roots(card, step_ms, txs, keep, win, None)
    }
    fn build_attempt_roots(
        &mut self,
        card: usize,
        step_ms: u64,
        txs: Vec<Transaction>,
        keep: &dyn Fn(Hash64) -> bool,
        win: bool,
        roots: Option<&kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1>,
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
                Box::new(OnetimeTxSelector::new(txs.clone())),
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
        let candidates = if roots.is_some() { 4096 } else { 4_000_000 };
        for draw in 0u64..candidates {
            let draw_anchor = if let Some(roots) = roots {
                // A foreign job's real roots are fixed. Draw by changing the public template,
                // never by changing the authenticated material or pretending nonces buy tickets.
                // Timestamp and nonce are blanked in pre-PoW; coinbase extra data changes the
                // actual transaction root and thus the template's public execution anchor.
                t = self
                    .tc
                    .build_block_template(
                        MinerData::new(card_payout_spk(card), draw.to_le_bytes().to_vec()),
                        Box::new(OnetimeTxSelector::new(txs.clone())),
                        TemplateBuildMode::Standard,
                    )
                    .unwrap();
                t.block.header.timestamp = self.time;
                t.block.header.nonce = self.nonce;
                let pre_pow = kaspa_consensus_core::hashing::header::pre_pow_hash_64(&t.block.header);
                attempt.challenge = challenge_v2(
                    self.network_domain,
                    pre_pow,
                    t.block.header.timestamp,
                    t.block.header.nonce,
                    facts.class_id,
                    &bond.0,
                );
                attempt.execution_root = roots.execution_root;
                attempt.output_root = roots.output_root;
                attempt.trace_root = roots.trace_root;
                execution_anchor_v3(self.network_domain, pre_pow, facts.class_id, &bond.0, t.block.header.nonce)
            } else {
                attempt.trace_root = Hash64::from_u64_word((self.nonce << 32) ^ draw ^ 0x7A00_0000_0000_0000);
                anchor
            };
            attempt.trace_manifest_root = attempt_trace_manifest_root_v1(attempt.trace_root, attempt.trace_chunk_count);
            let wins = class_ticket_v3(&attempt, draw_anchor) <= facts.class_target;
            if wins == win && (!win || keep(execution_commitment_v3(&attempt, draw_anchor))) {
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
    fresh_public_service(PublicCase::Withholding).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_convicts_a_public_foreign_job_and_reveals() {
    fresh_public_service(PublicCase::ForeignJob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_convicts_an_ordinarily_admitted_fp_v4_foreign_job() {
    fresh_public_service(PublicCase::ForeignFpJob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_convicts_an_ordinarily_admitted_fp_v3_foreign_job() {
    fresh_public_service(PublicCase::ForeignFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_convicts_fp_v3_after_original_commitment_history_is_pruned() {
    fresh_public_service(PublicCase::PrunedForeignFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_reacquires_pruned_fp_input_replays_and_releases_an_honest_job() {
    fresh_public_service(PublicCase::PrunedHonestFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_defaults_when_pruned_fp_input_is_withheld() {
    fresh_public_service(PublicCase::PrunedWithheldFpV3Input).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_replays_public_fp_input_without_opening_a_dispute() {
    fresh_public_service(PublicCase::HonestFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_convicts_an_unretained_fp_job_after_its_history_is_pruned() {
    fresh_public_service(PublicCase::UnretainedForeignFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_defaults_on_a_withheld_binding_without_fp_job_metadata() {
    fresh_public_service(PublicCase::UnretainedWithheldFpV3Binding).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_reacquires_an_unretained_job_and_input_then_replays_and_releases() {
    fresh_public_service(PublicCase::UnretainedHonestFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_reconstructs_a_multipart_unretained_job_then_replays_and_releases() {
    fresh_public_service(PublicCase::UnretainedMultipartHonestFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_replays_an_admitted_large_constraint_job_from_public_multipart_history() {
    fresh_public_service(PublicCase::UnretainedLargeConstraintJob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_defaults_on_a_withheld_large_constraint_job() {
    fresh_public_service(PublicCase::UnretainedWithheldLargeConstraintJob).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fresh_public_bond_service_defaults_on_a_withheld_job_preimage_after_a_correct_binding() {
    fresh_public_service(PublicCase::UnretainedWithheldFpV3Job).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_public_bond_service_convicts_a_constraint_forbidden_token_without_job_metadata() {
    fresh_public_service(PublicCase::UnretainedConstraintForbidden).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_public_bond_service_convicts_a_constraint_premature_stop_without_job_metadata() {
    fresh_public_service(PublicCase::UnretainedConstraintStop).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_public_bond_service_convicts_a_constraint_beaten_token_without_job_metadata() {
    fresh_public_service(PublicCase::UnretainedConstraintBeaten).await;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicCase {
    Withholding,
    ForeignJob,
    ForeignFpJob,
    ForeignFpV3Job,
    PrunedForeignFpV3Job,
    PrunedHonestFpV3Job,
    PrunedWithheldFpV3Input,
    HonestFpV3Job,
    UnretainedForeignFpV3Job,
    UnretainedWithheldFpV3Binding,
    UnretainedHonestFpV3Job,
    UnretainedMultipartHonestFpV3Job,
    UnretainedLargeConstraintJob,
    UnretainedWithheldLargeConstraintJob,
    UnretainedWithheldFpV3Job,
    UnretainedConstraintForbidden,
    UnretainedConstraintStop,
    UnretainedConstraintBeaten,
}

impl PublicCase {
    fn is_fp(self) -> bool {
        !matches!(self, Self::Withholding | Self::ForeignJob)
    }
    fn pruned_fp(self) -> bool {
        matches!(self, Self::PrunedForeignFpV3Job | Self::PrunedHonestFpV3Job | Self::PrunedWithheldFpV3Input) || self.unretained_fp()
    }
    fn unretained_fp(self) -> bool {
        matches!(
            self,
            Self::UnretainedForeignFpV3Job
                | Self::UnretainedWithheldFpV3Binding
                | Self::UnretainedHonestFpV3Job
                | Self::UnretainedMultipartHonestFpV3Job
                | Self::UnretainedLargeConstraintJob
                | Self::UnretainedWithheldLargeConstraintJob
                | Self::UnretainedWithheldFpV3Job
                | Self::UnretainedConstraintForbidden
                | Self::UnretainedConstraintStop
                | Self::UnretainedConstraintBeaten
        )
    }
    fn identity_fault(self) -> bool {
        matches!(
            self,
            Self::ForeignJob | Self::ForeignFpJob | Self::ForeignFpV3Job | Self::PrunedForeignFpV3Job | Self::UnretainedForeignFpV3Job
        )
    }
    fn da_default(self) -> bool {
        matches!(
            self,
            Self::Withholding
                | Self::PrunedWithheldFpV3Input
                | Self::UnretainedWithheldFpV3Binding
                | Self::UnretainedWithheldFpV3Job
                | Self::UnretainedWithheldLargeConstraintJob
        )
    }
    fn reacquire_honest(self) -> bool {
        matches!(
            self,
            Self::PrunedHonestFpV3Job
                | Self::UnretainedHonestFpV3Job
                | Self::UnretainedMultipartHonestFpV3Job
                | Self::UnretainedLargeConstraintJob
        )
    }
    fn constraint_fault(self) -> Option<u8> {
        match self {
            Self::UnretainedConstraintForbidden => Some(1),
            Self::UnretainedConstraintStop => Some(2),
            Self::UnretainedConstraintBeaten => Some(3),
            _ => None,
        }
    }
    fn constraint(self) -> bool {
        self.large_constraint() || self.constraint_fault().is_some()
    }
    fn exact_fault(self) -> bool {
        self.identity_fault() || self.constraint_fault().is_some()
    }
    fn answers_input(self) -> bool {
        self.reacquire_honest() || self.constraint_fault().is_some()
    }
    fn large_constraint(self) -> bool {
        matches!(self, Self::UnretainedLargeConstraintJob | Self::UnretainedWithheldLargeConstraintJob)
    }
}

/// A public table and a well-formed second-form DFA large enough that a signed answer cannot
/// fit a single lifecycle carrier. Its ordinary job transaction must still pass the real mass
/// and admission checks below. This fixture is not a claim about the maximum of every profile.
fn public_constraint_table(tokenizer_id: Hash64) -> kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1 {
    let vocab = misaka_palw_base0::rc::palw_rc_base0_shape_v1().vocab;
    kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1 {
        entries: vec![Some(b"a".to_vec()); vocab],
        eog_token_ids: vec![(vocab - 1) as u32],
        tokenizer_id,
    }
}

fn selection_constraint_table(tokenizer_id: Hash64) -> kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1 {
    let mut table = public_constraint_table(tokenizer_id);
    for entry in &mut table.entries {
        *entry = Some(b"b".to_vec());
    }
    table.entries[0] = Some(b"a".to_vec());
    table.entries[1] = Some(b"a".to_vec());
    table
}

fn selection_constraint_job(
    job: &mut kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Arc<kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1> {
    use kaspa_consensus_core::palw_decode_constraint_v1::*;
    use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
    let table = Arc::new(selection_constraint_table(job.tokenizer_id));
    let constraint = PalwDecodeConstraintV1 {
        version: 1,
        compiler_id: Hash64::from_u64_word(0xC015_0100),
        start_frame: 0,
        frames: vec![PalwConstraintFrameV1 {
            start: 0,
            nodes: vec![PalwConstraintNodeV1 {
                accepting: true,
                edges: vec![PalwConstraintEdgeV1 { lo: b'a', hi: b'a', action: PalwConstraintActionV1::Goto(0) }],
            }],
        }],
    };
    job.version = PALW_FP_CONSTRAINT_VERSION;
    job.decode = None;
    job.tail = Some(kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 {
        constraint: constraint.to_bytes(),
        table_root: table.root(),
    }));
    table
}

fn large_public_constraint_job(
    job: &mut kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptJobV3,
) -> Arc<kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableV1> {
    use kaspa_consensus_core::palw_decode_constraint_v1::*;
    use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
    use kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1;
    // The producer holds its own copy. It does not populate the verifier's public registry.
    let table = Arc::new(public_constraint_table(job.tokenizer_id));
    let root = table.root();
    let edges = (0..128).map(|byte| PalwConstraintEdgeV1 { lo: byte, hi: byte, action: PalwConstraintActionV1::Goto(0) }).collect();
    let constraint = PalwDecodeConstraintV1 {
        version: 2,
        compiler_id: Hash64::from_u64_word(0xC014_0200),
        start_frame: 0,
        frames: vec![PalwConstraintFrameV1 { start: 0, nodes: vec![PalwConstraintNodeV1 { accepting: true, edges }; 169] }],
    }
    .to_bytes();
    assert!(constraint.len() > 100_000);
    kaspa_consensus_core::palw_fp_constraint_v2::palw_constraint_admitted_v1(&constraint, true).unwrap();
    job.version = PALW_FP_CONSTRAINT_VERSION;
    job.decode = None;
    job.tail = Some(PalwFpJobTailV1::Constraint(PalwFpConstraintTailV1 { constraint, table_root: root }));
    table
}

/// The CLI's 48 KiB payload cap does not bound ordinary consensus admission. Construct a
/// genuine PQ-signed transaction for the large fixture and submit it to the ordinary validators.
fn large_constraint_transaction(
    commitment: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    prompt_token_ids: Vec<u32>,
    funding: &Funding,
    fee: u64,
) -> Transaction {
    use kaspa_consensus_core::hashing::sighash::{Mldsa87SigHashReusedValuesUnsync, calc_mldsa87_signature_hash};
    use kaspa_consensus_core::hashing::sighash_type::SIG_HASH_ALL;
    use kaspa_consensus_core::palw_freeprompt_v3::*;
    use kaspa_consensus_core::tx::{TransactionInput, TransactionOutput};
    use kaspa_txscript::{MLDSA87_TX_CONTEXT, script_builder::ScriptBuilder};
    let signature = signed(0, fp_claim_id_v3(&commitment).as_byte_slice(), PALW_FP_V3_MLDSA87_COMMITMENT_CONTEXT);
    let payload = PalwFpCommitmentTxPayloadV3 { version: PALW_FP_V3_VERSION, commitment, prompt_token_ids, signature };
    let tx = Transaction::new(
        kaspa_consensus_core::constants::TX_VERSION,
        vec![TransactionInput::new(funding.0, vec![], u64::MAX, 1)],
        vec![TransactionOutput::new(funding.1.amount - fee, funding.1.script_public_key.clone())],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
        0,
        borsh::to_vec(&payload).unwrap(),
    );
    let mut populated = MutableTransaction::with_entries(tx, vec![funding.1.clone()]);
    let sighash = calc_mldsa87_signature_hash(&populated.as_verifiable(), 0, SIG_HASH_ALL, &Mldsa87SigHashReusedValuesUnsync::new());
    let mut signature = signed(0, sighash.as_byte_slice(), MLDSA87_TX_CONTEXT);
    signature.push(SIG_HASH_ALL.to_u8());
    populated.tx.inputs[0].signature_script =
        ScriptBuilder::new().add_data(&signature).unwrap().add_data(&card_pubkey(0)).unwrap().drain();
    populated.tx
}

/// The producer executes a real registered model under FP job B, then claims the roots
/// for user-prompt job A when identity_fault is requested. Otherwise the commitment is honest.
/// Only requested signed public Event/input answers are shared with the outsider.
fn public_fp_job(
    chain: &Chain,
    identity_fault: bool,
    large_constraint: bool,
    constraint_fault: Option<u8>,
) -> (
    kaspa_consensus_core::palw_backend::PalwExecutionOutcomeV1,
    kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1,
    Option<(kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3, Vec<u32>)>,
) {
    use kaspa_consensus_core::palw_freeprompt_v3::{
        PALW_FP_PRIVACY_PUBLIC_DA, PALW_FP_PROMPT_MODE_USER, PALW_FP_V3_VERSION, PalwFreePromptJobV3, fp_canonical_anchor_v1,
    };
    let backend = misaka_palw_base0::backend::Base0Backend::new(
        misaka_palw_base0::classes::resolve_class_v1(
            &chain.bundle.court,
            chain.bundle.base_class_id,
            misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().unwrap(),
            &[],
        )
        .unwrap(),
    )
    .with_step_ladder_cap(chain.bundle.court.max_step_leaf_count())
    .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1());
    let facts = chain.tc.palw_producer_facts_v2(chain.bundle.base_class_id, Some(chain.bonds[0].0)).unwrap();
    let mut job = PalwFreePromptJobV3 {
        version: PALW_FP_V3_VERSION,
        network_domain: chain.network_domain,
        class_id: chain.bundle.base_class_id,
        executor_bond: chain.bonds[0].0,
        executor_pubkey: card_pubkey(0),
        operator_id: facts.bond.as_ref().unwrap().operator_id,
        anchor_block: facts.chain_point,
        anchor_daa: facts.daa_score,
        job_nonce: [0x47; 32],
        tokenizer_id: Hash64::default(),
        prompt_token_ids_hash: Hash64::default(),
        prompt_tokens: 0,
        decode_token_limit: 0,
        max_context_tokens: 0,
        privacy_mode: PALW_FP_PRIVACY_PUBLIC_DA,
        prompt_mode: PALW_FP_PROMPT_MODE_USER,
        sampling_seed: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_SEED_GREEDY,
        temperature_q: kaspa_consensus_core::palw_decode_select_v2::PALW_DECODE_TEMPERATURE_GREEDY,
        decode: None,
        tail: None,
    };
    let producer_table = if large_constraint {
        Some(large_public_constraint_job(&mut job))
    } else if constraint_fault.is_some() {
        Some(selection_constraint_job(&mut job))
    } else if chain.config.params.palw_fp_decode_rules_active_at(chain.daa()) {
        job = job.into_v4(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
        None
    } else {
        None
    };
    let derive = |job: &mut PalwFreePromptJobV3| {
        let (ctx, prompt) = backend.job_for_anchor(fp_canonical_anchor_v1(job)).unwrap();
        job.prompt_token_ids_hash = kaspa_consensus_core::palw_prompt_ids_v1::prompt_token_ids_commitment_v1(
            backend.prompt_ids_form(),
            &prompt.iter().map(|t| *t as u32).collect::<Vec<_>>(),
        )
        .unwrap();
        job.prompt_tokens = prompt.len() as u32;
        job.decode_token_limit = ctx.exact_decode_tokens;
        job.max_context_tokens = ctx.max_context_tokens;
        prompt
    };
    let prompt = derive(&mut job);
    let mask = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_constraint_mask_for_host_v1(
        &job,
        producer_table.or_else(|| backend.token_table_for_job_v1(&job)),
    )
    .unwrap();
    let run = kaspa_consensus_core::palw_fp_constraint_job_v1::palw_fp_with_constraint_scope_v1(mask, || {
        backend.execute_free_prompt(&job, &prompt)
    })
    .unwrap();
    let shape = backend.capture_shape(&run.outcome.material).unwrap();
    let mut commitment = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(
        &job,
        &shape.job_context,
        &run,
        chain.daa() + facts.min_trace_retention_daa,
    )
    .unwrap();
    let mut event = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap();
    let mut outcome = run.outcome;
    if let Some(fault) = constraint_fault {
        use kaspa_consensus_core::palw_step_refute::{PalwTraceEventDisclosureV1, base0_logits_trace_root_v1};
        let PalwTraceEventDisclosureV1::Flat { binding, pin } = &mut event else { panic!("registered floor is flat") };
        assert!(pin.generated_token_ids.len() > 2);
        assert!(pin.generated_token_ids[..3].iter().all(|id| *id <= 1));
        pin.generated_token_ids[2] = match fault {
            1 => 2,
            2 => binding.shape_profile.vocab_size - 1,
            3 => 1 - pin.generated_token_ids[2],
            _ => unreachable!(),
        };
        binding.full_logits_trace_root = base0_logits_trace_root_v1(&binding.job_context, &pin.logits_rows, &pin.generated_token_ids);
        binding.committed_execution_root = kaspa_consensus_core::palw_step_leg::binding_commitment_root_v1(binding);
        kaspa_consensus_core::palw_step_leg::verify_binding_v1(binding).unwrap();
        commitment.execution_root = binding.committed_execution_root;
        commitment.trace_root = binding.full_logits_trace_root;
        commitment.output_root =
            kaspa_consensus_core::palw_attempt_rules_v1::palw_attempt_output_root_v1(&binding.job_context, &pin.generated_token_ids);
        outcome.execution_root = commitment.execution_root;
        outcome.trace_root = commitment.trace_root;
        outcome.output_root = commitment.output_root;
        // Retain only the public event and input/job descriptor. No producer capture is
        // available to the outsider or to a test-side proof builder.
        outcome.material.clear();
    }
    // The claimed job A has a fully valid user prompt and schedule; its authenticated
    // execution material belongs to B. Admission must not require trusting Panel replay.
    if identity_fault {
        job.job_nonce[0] ^= 1;
    }
    let ctx = backend.fp_job_context_for_executed_v1(&job, commitment.decode_tokens_executed).unwrap();
    commitment.job = job;
    commitment.schedule_root = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(
        &ctx.context_hash(),
        commitment.job.prompt_tokens,
        commitment.decode_tokens_executed,
    )
    .0;
    (outcome, event, Some((commitment, prompt.into_iter().map(|t| t as u32).collect())))
}

async fn fresh_public_service(case: PublicCase) {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let (config, bundle, premine, mut floats) = fixture_config(
        case == PublicCase::ForeignFpJob,
        case.pruned_fp() || case == PublicCase::HonestFpV3Job,
        case.unretained_fp(),
        case.constraint(),
    );
    let mut chain = Chain::new(config, bundle, &premine);
    chain.advance(2).await;
    if case.is_fp() {
        // Derived-work pricing requires the registered graph to be publicly carried. The
        // ordinary ClassLaneCertified arm authenticates it by the existing class id and
        // certified family; do not inject the profile into state or waive the price gate.
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .unwrap();
        assert_eq!(profile.shape_profile_id(), chain.bundle.base_class_id);
        let publish = PalwConsensusObjectV2::ClassLaneCertified {
            class_id: chain.bundle.base_class_id,
            lane: kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt,
            profile: Box::new(profile.clone()),
        };
        let reachable = kaspa_consensus_core::palw_class_admission_v2::reachable_kernels_v1(&profile);
        let covered = chain.tc.palw_certified_families_v1().iter().any(|(lane, _, row)| {
            *lane == kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt
                && reachable.is_subset(&row.family.kernel_ids)
        });
        if !covered {
            // Carry the normal public drill evidence; the chain itself grades every fault.
            // A build-local certificate or test callback does not certify the on-chain lane.
            let evidence =
                misaka_palw_base0::e2e_drill::rc_free_prompt_evidence_v1(misaka_palw_base0::e2e_drill::PalwRcFamilyV1::Base0).unwrap();
            let certificate = PalwConsensusObjectV2::FamilyCertified {
                evidence: Box::new(kaspa_consensus_core::palw_state_v2::PalwCertificationEvidenceV1::FreePrompt(evidence)),
            };
            let carriers =
                kaspa_consensus_core::palw_state_v2::palw_object_chunks_v1(&certificate).unwrap().unwrap_or_else(|| vec![certificate]);
            let count = carriers.len();
            for carrier in carriers {
                chain.send(0, &carrier, &mut floats[0], &[]).await;
            }
            assert!(
                chain
                    .tc
                    .palw_certified_families_v1()
                    .iter()
                    .any(|(lane, _, row)| *lane == kaspa_consensus_core::palw_state_v2::PalwCertifiedLaneV1::FreePrompt
                        && reachable.is_subset(&row.family.kernel_ids))
            );
            eprintln!("[service-public] normal FP family certification accepted from {count} public drill carriers");
        }
        chain.send(0, &publish, &mut floats[0], &[]).await;
        eprintln!("[service-public] registered FP graph publicly carried by ordinary ClassLaneCertified");
    }
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
    let foreign = (case != PublicCase::Withholding).then(|| {
        if case.is_fp() {
            return public_fp_job(&chain, case.identity_fault(), case.large_constraint(), case.constraint_fault());
        }
        use misaka_palw_base0::classes::resolve_class_v1;
        let backend = misaka_palw_base0::backend::Base0Backend::new(
            resolve_class_v1(
                &chain.bundle.court,
                chain.bundle.base_class_id,
                misaka_palw_base0::rc::palw_rc_base0_artifact_root_v1().unwrap(),
                &[],
            )
            .unwrap(),
        )
        .with_step_ladder_cap(chain.bundle.court.max_step_leaf_count())
        .with_prompt_ids_form(chain.config.params.palw_prompt_ids_form_v1());
        let (job, prompt) = backend.job_for_anchor(Hash64::from_u64_word(0xF0_12_E1_6E)).unwrap();
        let job = kaspa_consensus_core::palw_attempt_v2::palw_attempt_job_v1(
            job,
            chain.config.params.palw_prefill_draw_active_at(chain.daa()),
        );
        let outcome = backend.execute(&job, &prompt).unwrap();
        let event = backend.disclose_trace_event(&outcome.material, 0, 0).unwrap();
        (outcome, event, None)
    });
    let claim = if let Some((_, _, Some((commitment, prompt)))) = &foreign {
        let carried = kaspa_consensus_core::palw_freeprompt_v3::palw_fp_carried_prompt_ids_v1(&commitment.job, prompt);
        assert_eq!(&carried, prompt, "ordinary PublicDA FP carries the hash-bound user prompt");
        let quote = chain
            .tc
            .palw_fp_commitment_price_v1(
                commitment.job.class_id,
                carried.clone(),
                commitment.job.prompt_tokens,
                commitment.decode_tokens_executed,
                commitment.work_leaves,
                Some(chain.bonds[0].0),
            )
            .unwrap()
            .price
            .expect("the ordinary registry admits and prices this FP job");
        let signer = kaspa_pq_validator_core::ValidatorKey::from_seed([0xB0; 32]);
        let build = |fee| {
            if case.large_constraint() {
                return large_constraint_transaction(commitment.clone(), carried.clone(), &floats[0], fee);
            }
            signer
                .build_fp_commitment_tx(
                    chain.network_domain,
                    chain.config.params.palw_prompt_ids_form_v1(),
                    commitment.clone(),
                    carried.clone(),
                    kaspa_pq_validator_core::FpCommitmentPriceV1::Chain { quanta: quote.quanta, pwu: quote.pwu },
                    floats[0].0,
                    &floats[0].1,
                    fee,
                )
                .unwrap()
        };
        let probe = build(250_000);
        let params = &chain.config.params;
        let mass = kaspa_consensus_core::mass::MassCalculator::new(
            params.mass_per_tx_byte,
            params.mass_per_script_pub_key_byte,
            params.mass_per_sig_op,
            params.storage_mass_parameter,
        );
        if case.large_constraint() {
            let masses = mass.calc_non_contextual_masses(&probe);
            let job_bytes = borsh::object_length(&commitment.job).unwrap();
            assert!(
                job_bytes > 100_000
                    && job_bytes as u64 <= kaspa_consensus_core::palw_legacy_held_da_v2::PALW_LEGACY_FP_JOB_MAX_BYTES_V2
            );
            assert!(masses.transient_mass <= params.max_block_mass);
            assert!(masses.transient_mass > params.max_block_mass * 99 / 100, "exercise the real ordinary transaction mass boundary");
            eprintln!(
                "[service-public] large constraint job bytes {job_bytes}, ordinary transaction transient mass {} / {}",
                masses.transient_mass, params.max_block_mass
            );
        }
        let fee = relay_fee_for_compute_mass(mass.calc_non_contextual_masses(&probe).compute_mass).max(250_000);
        let mut validated = MutableTransaction::from_tx(build(fee));
        chain.tc.validate_mempool_transaction(&mut validated, &Default::default()).unwrap();
        let tx = validated.tx.as_ref().clone();
        let claim = kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(commitment);
        let block = chain.heartbeat(vec![tx.clone()]).await;
        assert!(block.transactions.iter().any(|t| t.id() == tx.id()));
        chain.heartbeat(vec![]).await;
        let output = tx.outputs.last().unwrap();
        floats[0] = (
            TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32),
            UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
        );
        let view = chain.view(claim);
        assert_eq!(
            view.job_identity,
            kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(commitment),
            "the normal block fold pins the accepted FP job"
        );
        eprintln!(
            "[service-public] ordinary FP Job V{} admission: claim {claim}, {} quanta, {} pwu, fee {fee}",
            commitment.job.version, quote.quanta, quote.pwu
        );
        claim
    } else {
        let (attempt, claim) = chain.build_attempt_roots(
            0,
            chain.config.params.target_time_per_block(),
            vec![],
            &|_| true,
            true,
            foreign.as_ref().map(|(outcome, _, _)| outcome),
        );
        chain.insert(attempt).await;
        claim
    };
    let slot = chain.view(claim).accepted_daa + chain.bundle.panel.anchor_delay();
    chain.advance(slot).await;
    let (anchor, _) = chain.build_attempt_drawn(7, chain.config.params.target_time_per_block(), vec![], &|_| true, true);
    chain.insert(anchor).await;
    let duties = chain.tc.palw_seat_duties_v2(chain.bonds.clone()).into_iter().filter(|d| d.claim_id == claim).collect::<Vec<_>>();
    assert!(!duties.is_empty());
    assert!(chain.tc.palw_seat_duties_v2(vec![bond]).is_empty());
    let all_seats = duties.iter().map(|d| d.seat_bond).collect::<BTreeSet<_>>();
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
    if case.unretained_fp() {
        assert!(chain.daa() < 1060, "the job was admitted before retention activation");
        assert!(chain.view(claim).public_fp_job.is_none(), "no metadata is injected or removed");
        chain.advance(1060).await;
        assert!(chain.config.params.palw_legacy_public_filer_active_at(chain.daa()));
    }
    assert!(chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == claim && !c.seat));
    if case.is_fp() {
        let candidate = chain.tc.palw_fraud_filer_candidates_v1(bond).into_iter().find(|c| c.claim_id == claim).unwrap();
        assert!(candidate.job.free_prompt, "public discovery identifies the ordinarily admitted FP lane");
        let stored = chain
            .view(claim)
            .public_fp_job
            .map(|wire| borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3>(&wire).unwrap());
        if case.unretained_fp() {
            assert!(stored.is_none());
            assert!(candidate.accepted_daa < 1060);
            assert_ne!(chain.view(claim).job_identity, Hash64::default());
        } else {
            assert_eq!(
                kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(
                    stored.as_ref().expect("the ordinary accepted extractor retained authenticated job metadata")
                ),
                claim
            );
        }
        if case.pruned_fp() {
            use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
            use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
            let commitment_block = chain
                .history
                .iter()
                .find(|b| {
                    b.transactions.iter().any(|t| {
                        t.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                            && borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>(&t.payload)
                                .is_ok_and(|p| p.claim_id() == claim)
                    })
                })
                .unwrap()
                .hash();
            // Delete real DB rows and caches, rather than substituting a read API. This tests lost history,
            // independently of the still-open network pruned-import/recovery requirement.
            chain.tc.block_transactions_store.delete(commitment_block).unwrap();
            chain.tc.acceptance_data_store.delete(candidate.job.accepted_block).unwrap();
            assert!(chain.tc.get_block(commitment_block).is_err());
            assert!(chain.tc.get_block_acceptance_data(candidate.job.accepted_block).is_err());
            let reacquired =
                super::palw_fraud_filer::palw_fraud_filer_fp_payload_v1(chain.tc.as_ref().as_ref(), &candidate, chain.network_domain);
            if case.unretained_fp() {
                assert!(reacquired.is_err(), "neither accepted history nor rooted metadata can supply the old FP body");
                assert!(chain.view(claim).public_fp_job.is_none());
            } else {
                let reacquired = reacquired.expect("public rooted metadata survives missing original history");
                assert_eq!(reacquired.commitment, stored.unwrap());
                assert!(reacquired.prompt_token_ids.is_empty());
                assert!(reacquired.signature.is_empty());
            }
            assert!(
                chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == claim && !c.seat),
                "ordinary candidate discovery survives missing commitment history"
            );
            assert!(
                chain.config.params.palw_legacy_held_da_v2_active_at(chain.daa()),
                "the input bootstrap's own dormant fence is active in this fixture"
            );
            eprintln!(
                "[service-public] original FP body/acceptance rows pruned; rooted job metadata present = {}",
                !case.unretained_fp()
            );
        }
    }
    let producer_capital = chain.tc.palw_operator_da_standing_v1(chain.view(claim).producer).unwrap().collateral;
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
    if case.constraint() {
        use kaspa_consensus_core::palw_fp_constraint_job_v1::*;
        // Independently rebuild and load the fixture's PUBLIC tokenizer file for the outside
        // verifier. The producer's table Arc, capture and pending state are never handed over.
        let table = if case.constraint_fault().is_some() {
            selection_constraint_table(Hash64::default())
        } else {
            public_constraint_table(Hash64::default())
        };
        let public_file = palw_token_table_file_encode_v1(&table);
        palw_token_tables_v1().register(palw_token_table_file_decode_v1(&public_file).unwrap());
    }
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
    let mut answer_submitted = None;
    let mut input_chunks = BTreeSet::new();
    let mut job_preimage_submitted = false;
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
        if case == PublicCase::HonestFpV3Job {
            assert!(!view.executor_refuted && !view.court_convicted && !view.da_defaulted);
            assert!(
                !view.record.as_ref().is_some_and(|record| record.knows(&bond)),
                "an already public complete input must not consume this verifier's reservation/deposit budget"
            );
            if service.fraud_filer_honest_replays.lock().unwrap().contains(&claim) {
                assert!(
                    !chain.history[service_start..]
                        .iter()
                        .flat_map(|b| b.transactions.iter())
                        .filter_map(|tx| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).ok())
                        .any(|payload| matches!(payload.object, PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. }
                        if reservation.claim == claim && reservation.reserver == bond))
                );
                eprintln!(
                    "[service-public] complete public FP input independently replayed Honest without any target reservation at DAA {}",
                    chain.daa()
                );
                break;
            }
        }
        reserved |= view.record.as_ref().is_some_and(|r| r.live.contains_key(&bond));
        demanded |= view.sessions.iter().any(|(accuser, _)| *accuser == bond);
        if demanded
            && case != PublicCase::UnretainedWithheldFpV3Binding
            && answer_submitted.is_none()
            && let Some((_, event, _)) = &foreign
        {
            use kaspa_consensus_core::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1, palw_da_answer_object_v1};
            let object = palw_da_answer_object_v1(
                &chain.network_domain,
                claim,
                PalwDaUnitV1::Event { row: 0, tile: 0 },
                PalwDaAnswerV1::Event(event.clone()),
                chain.bonds[0],
                chain.bundle.court.max_close_bytes(),
                |message, context| Some(signed(0, message, context)),
            )
            .unwrap();
            let mut unsigned = object.clone();
            if let PalwConsensusObjectV2::MaterialDisclosedV2 { signature, .. } = &mut unsigned {
                signature[0] ^= 1;
            }
            let mut bad_event = event.clone();
            use kaspa_consensus_core::palw_step_refute::PalwTraceEventDisclosureV1;
            match &mut bad_event {
                PalwTraceEventDisclosureV1::Flat { binding, .. }
                | PalwTraceEventDisclosureV1::Tiled { binding, .. }
                | PalwTraceEventDisclosureV1::OutOfRange { binding } => {
                    binding.job_context.job_id = Hash64::from_u64_word(0xBAD);
                }
            }
            let forged = palw_da_answer_object_v1(
                &chain.network_domain,
                claim,
                PalwDaUnitV1::Event { row: 0, tile: 0 },
                PalwDaAnswerV1::Event(bad_event),
                chain.bonds[0],
                chain.bundle.court.max_close_bytes(),
                |message, context| Some(signed(0, message, context)),
            )
            .unwrap();
            let unasked = palw_da_answer_object_v1(
                &chain.network_domain,
                claim,
                PalwDaUnitV1::Event { row: 0, tile: 1 },
                PalwDaAnswerV1::Event(event.clone()),
                chain.bonds[0],
                chain.bundle.court.max_close_bytes(),
                |message, context| Some(signed(0, message, context)),
            )
            .unwrap();
            let stranger = palw_da_answer_object_v1(
                &chain.network_domain,
                claim,
                PalwDaUnitV1::Event { row: 0, tile: 0 },
                PalwDaAnswerV1::Event(event.clone()),
                bond,
                chain.bundle.court.max_close_bytes(),
                |message, context| Some(signed(OUTSIDER, message, context)),
            )
            .unwrap();
            let session = manager.consensus().unguarded_session();
            for (name, refused) in [("signature", unsigned), ("binding", forged), ("unasked unit", unasked), ("discloser", stranger)] {
                let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                    &chain.config,
                    key(0),
                    &refused,
                    floats[0].0,
                    &floats[0].1,
                    &[],
                    None,
                    chain.daa(),
                )
                .unwrap();
                let refusal = retained_flow
                    .submit_rpc_transaction(&session, tx, kaspa_mining::mempool::tx::Orphan::Forbidden)
                    .await
                    .expect_err("public evidence carriage must not relay unauthenticated or unauthorized answers");
                assert!(format!("{refusal:?}").contains("PalwH1CarrierRefused"), "{name}: {refusal:?}");
            }
            eprintln!("[service-public] H1 refuses bad signature/binding, unasked unit and unliable discloser");
            let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                &chain.config,
                key(0),
                &object,
                floats[0].0,
                &floats[0].1,
                &[],
                None,
                chain.daa(),
            )
            .unwrap();
            retained_flow
                .submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden)
                .await
                .expect("the authenticated producer answer passes the real PQ mempool");
            answer_submitted = Some(tx.id());
            eprintln!("[service-public] foreign-job authenticated event submitted through the real PQ mempool");
            let mut included = false;
            for _ in 0..8 {
                let block = pool_heartbeat(&mut chain, &manager, &pool).await;
                if block.transactions.iter().any(|t| t.id() == tx.id()) {
                    included = true;
                    break;
                }
            }
            assert!(included, "the production template carries the authenticated fault disclosure");
            pool_heartbeat(&mut chain, &manager, &pool).await;
            let disclosed = chain.view(claim);
            assert_eq!(disclosed.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 0, tile: 0 }), !case.identity_fault());
            assert!(!disclosed.executor_refuted && !disclosed.da_defaulted, "carriage itself is neither a DA answer nor conviction");
            let output = tx.outputs.last().unwrap();
            floats[0] = (
                TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32),
                UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
            );
            eprintln!("[service-public] authenticated binding carried; answered = {}", !case.identity_fault());
        }
        if (matches!(
            case,
            PublicCase::UnretainedHonestFpV3Job
                | PublicCase::UnretainedMultipartHonestFpV3Job
                | PublicCase::UnretainedLargeConstraintJob
        ) || case.constraint_fault().is_some())
            && !job_preimage_submitted
            && chain.view(claim).sessions.iter().any(|(who, s)| {
                *who == bond
                    && s.units.contains(&kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::LegacyHeldV2(
                        kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::FpJob,
                    ))
            })
        {
            use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
            use kaspa_consensus_core::palw_legacy_held_da_v2::{
                PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_VERSION_V2, PalwLegacyHeldAnswerCarriageV2,
                PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2, palw_legacy_held_answer_message_v2,
            };
            let (_, event, Some((commitment, _))) = foreign.as_ref().unwrap() else { unreachable!() };
            let correct_job = commitment.job.clone();
            for corrupt in [true, false] {
                let mut job = correct_job.clone();
                if corrupt {
                    job.job_nonce[0] ^= 1;
                    if case.large_constraint() {
                        use kaspa_consensus_core::palw_freeprompt_v3::PalwFpJobTailV1;
                        let Some(PalwFpJobTailV1::Constraint(tail)) = job.tail.as_mut() else { unreachable!() };
                        let mut constraint =
                            kaspa_consensus_core::palw_fp_constraint_job_v1::palw_constraint_of_bytes_v1(&tail.constraint).unwrap();
                        constraint.frames[0].nodes.truncate(1);
                        tail.constraint = constraint.to_bytes();
                        // A wrong direct preimage does not occupy the session's immutable signed
                        // manifest. A wrong multipart manifest instead owes its existing default;
                        // it cannot be replaced merely because the last part was refused.
                    }
                }
                let mut answer = PalwLegacyHeldAnswerCarriageV2 {
                    version: PALW_LEGACY_HELD_VERSION_V2,
                    claim,
                    unit: PalwLegacyHeldUnitV2::FpJob,
                    binding: event.binding().clone(),
                    answer: PalwLegacyHeldAnswerV2::FpJob(Box::new(job)),
                    discloser: chain.bonds[0],
                    signature: vec![],
                };
                let message = palw_legacy_held_answer_message_v2(chain.network_domain.as_byte_slice(), &answer);
                answer.signature = signed(0, message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
                let objects = if !corrupt
                    && matches!(case, PublicCase::UnretainedMultipartHonestFpV3Job | PublicCase::UnretainedLargeConstraintJob)
                {
                    use kaspa_consensus_core::palw_legacy_answer_chunks_v2::*;
                    // Both fixtures use real authenticated history. The old small job forces two
                    // parts; the large constraint genuinely exceeds one carrier. Neither fixture
                    // exercises the producer's automatic splitting worker or every maximum profile.
                    let answer_bytes = borsh::object_length(&answer).unwrap();
                    let part_bytes = if case == PublicCase::UnretainedLargeConstraintJob {
                        assert!(answer_bytes + 1 > kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES);
                        PALW_LEGACY_ANSWER_CHUNK_BYTES_V2
                    } else {
                        answer_bytes.div_ceil(2) as u32
                    };
                    let (declaration, chunks) =
                        palw_legacy_answer_split_v2(&answer, part_bytes, chain.bundle.court.max_close_bytes(), |d| {
                            let message = palw_legacy_answer_declaration_message_v2(chain.network_domain.as_byte_slice(), d);
                            Ok(signed(0, message.as_byte_slice(), PALW_LEGACY_ANSWER_DECLARE_CONTEXT_V2))
                        })
                        .unwrap();
                    assert_eq!(chunks.len(), 2);
                    let mut objects = vec![PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration: Box::new(declaration) }];
                    objects.extend(
                        chunks.into_iter().map(|chunk| PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk: Box::new(chunk) }),
                    );
                    objects
                } else {
                    vec![PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer: Box::new(answer) }]
                };
                let carriers = objects.len();
                for (index, object) in objects.into_iter().enumerate() {
                    let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                        &chain.config,
                        key(0),
                        &object,
                        floats[0].0,
                        &floats[0].1,
                        &[],
                        None,
                        chain.daa(),
                    )
                    .unwrap();
                    let session = manager.consensus().unguarded_session();
                    retained_flow
                        .submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden)
                        .await
                        .expect("the signed descriptor carrier passes ordinary PQ admission");
                    let mut included = false;
                    for _ in 0..8 {
                        let block = pool_heartbeat(&mut chain, &manager, &pool).await;
                        if block.transactions.iter().any(|t| t.id() == tx.id()) {
                            included = true;
                            break;
                        }
                    }
                    assert!(included, "the job descriptor was actually carried");
                    pool_heartbeat(&mut chain, &manager, &pool).await;
                    let output = tx.outputs.last().unwrap();
                    floats[0] = (
                        TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32),
                        UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
                    );
                    assert_eq!(
                        chain.view(claim).unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob)),
                        !corrupt && index + 1 == carriers,
                        "partial parts never answer the DA obligation"
                    );
                }
                let view = chain.view(claim);
                assert_eq!(view.unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::FpJob)), !corrupt);
                assert!(view.public_fp_job.is_none(), "court reacquisition never fabricates original commitment metadata");
                assert!(!view.executor_refuted && !view.court_convicted && !view.da_defaulted);
            }
            job_preimage_submitted = true;
            eprintln!("[service-public] wrong job refused, authenticated job preimage acquired through signed public carriers");
        }
        if case.answers_input() {
            use kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1;
            use kaspa_consensus_core::palw_legacy_held_da_v2::{
                PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2, PALW_LEGACY_HELD_VERSION_V2, PalwLegacyHeldAnswerCarriageV2,
                PalwLegacyHeldAnswerV2, PalwLegacyHeldUnitV2, palw_legacy_held_answer_message_v2, palw_legacy_prompt_answer_v2,
            };
            let requested: Vec<u32> = chain
                .view(claim)
                .sessions
                .iter()
                .filter(|(who, _)| *who == bond)
                .flat_map(|(_, session)| session.units.iter())
                .filter_map(|unit| match unit {
                    PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk }) => Some(*chunk),
                    _ => None,
                })
                .collect();
            for chunk in requested {
                if input_chunks.contains(&chunk) {
                    continue;
                }
                let (_, event, Some((_, ids))) = foreign.as_ref().unwrap() else { unreachable!() };
                let binding = event.binding().clone();
                let answer =
                    palw_legacy_prompt_answer_v2(&binding, ids, chunk, chain.config.params.palw_prompt_ids_form_v1()).unwrap();
                let mut carriage = PalwLegacyHeldAnswerCarriageV2 {
                    version: PALW_LEGACY_HELD_VERSION_V2,
                    claim,
                    unit: PalwLegacyHeldUnitV2::PromptIds { chunk },
                    binding,
                    answer,
                    discloser: chain.bonds[0],
                    signature: vec![],
                };
                // A correctly signed but hash-inconsistent input cannot poison the public accumulator.
                let mut corrupt = carriage.clone();
                let PalwLegacyHeldAnswerV2::PromptIds { ids, .. } = &mut corrupt.answer else { unreachable!() };
                ids[0] ^= 1;
                let message = palw_legacy_held_answer_message_v2(chain.network_domain.as_byte_slice(), &corrupt);
                corrupt.signature = signed(0, message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
                let session = manager.consensus().unguarded_session();
                let object = PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer: Box::new(corrupt) };
                let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                    &chain.config,
                    key(0),
                    &object,
                    floats[0].0,
                    &floats[0].1,
                    &[],
                    None,
                    chain.daa(),
                )
                .unwrap();
                // Carriage and an authenticated answer are separate: a signed invalid input may
                // be carried, but the fold must not settle its unit and the reader must not learn it.
                retained_flow
                    .submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden)
                    .await
                    .expect("the signed invalid input carrier can be published without becoming an answer");
                let mut included = false;
                for _ in 0..8 {
                    let block = pool_heartbeat(&mut chain, &manager, &pool).await;
                    if block.transactions.iter().any(|t| t.id() == tx.id()) {
                        included = true;
                        break;
                    }
                }
                assert!(included, "the malformed signed input was actually carried");
                pool_heartbeat(&mut chain, &manager, &pool).await;
                let still_waiting = chain.view(claim);
                assert!(
                    !still_waiting.unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk })),
                    "an invalid input chunk cannot satisfy the public obligation"
                );
                assert!(still_waiting.sessions.iter().any(|(who, session)| *who == bond
                    && session.units.contains(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk }))));
                assert!(!still_waiting.executor_refuted && !still_waiting.court_convicted && !still_waiting.da_defaulted);
                let output = tx.outputs.last().unwrap();
                floats[0] = (
                    TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32),
                    UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
                );
                let message = palw_legacy_held_answer_message_v2(chain.network_domain.as_byte_slice(), &carriage);
                carriage.signature = signed(0, message.as_byte_slice(), PALW_LEGACY_HELD_ANSWER_MLDSA87_CONTEXT_V2);
                let object = PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer: Box::new(carriage) };
                let tx = palw_carrier_fees::build_lifecycle_carrier_v2(
                    &chain.config,
                    key(0),
                    &object,
                    floats[0].0,
                    &floats[0].1,
                    &[],
                    None,
                    chain.daa(),
                )
                .unwrap();
                retained_flow
                    .submit_rpc_transaction(&session, tx.clone(), kaspa_mining::mempool::tx::Orphan::Forbidden)
                    .await
                    .expect("the authenticated public input is admitted by the ordinary PQ mempool");
                let mut included = false;
                for _ in 0..8 {
                    let block = pool_heartbeat(&mut chain, &manager, &pool).await;
                    if block.transactions.iter().any(|t| t.id() == tx.id()) {
                        included = true;
                        break;
                    }
                }
                assert!(included, "ordinary mining template includes the requested public input");
                pool_heartbeat(&mut chain, &manager, &pool).await;
                assert!(
                    chain.view(claim).unit_was_answered_v1(&PalwDaUnitV1::LegacyHeldV2(PalwLegacyHeldUnitV2::PromptIds { chunk }))
                );
                let output = tx.outputs.last().unwrap();
                floats[0] = (
                    TransactionOutpoint::new(tx.id(), (tx.outputs.len() - 1) as u32),
                    UtxoEntry::new(output.value, output.script_public_key.clone(), chain.daa(), false),
                );
                input_chunks.insert(chunk);
                eprintln!(
                    "[service-public] authenticated public FP input chunk {chunk} mined; the verifier receives no private capture"
                );
            }
            let honest = chain.view(claim);
            assert!(!honest.executor_refuted && !honest.court_convicted && !honest.da_defaulted);
            if honest.record.as_ref().is_some_and(|record| record.closed.contains(&bond)) {
                assert!(!input_chunks.is_empty(), "the full input was reacquired through real public carriers");
                if case.unretained_fp() {
                    assert!(job_preimage_submitted, "the job was reacquired before the input or replay");
                }
                assert!(
                    service.fraud_filer_honest_replays.lock().unwrap().contains(&claim),
                    "release alone is insufficient: the real replay worker must have judged the three roots Honest"
                );
                assert!(honest.record.as_ref().unwrap().live.get(&bond).is_none());
                let carriers: Vec<_> = chain.history[service_start..]
                    .iter()
                    .flat_map(|b| b.transactions.iter())
                    .filter_map(|tx| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload).ok())
                    .map(|p| p.object)
                    .collect();
                if matches!(case, PublicCase::UnretainedMultipartHonestFpV3Job | PublicCase::UnretainedLargeConstraintJob) {
                    use kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2;
                    assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::LegacyHeldAnswerDeclaredV2 { declaration }
                        if declaration.claim == claim && declaration.unit == PalwLegacyHeldUnitV2::FpJob)));
                    assert_eq!(
                        carriers
                            .iter()
                            .filter(|o| matches!(o, PalwConsensusObjectV2::LegacyHeldAnswerChunkV2 { chunk }
                        if chunk.claim == claim && chunk.unit == PalwLegacyHeldUnitV2::FpJob))
                            .count(),
                        2
                    );
                    assert!(
                        !carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::LegacyHeldAnsweredV2 { answer }
                        if answer.claim == claim && answer.unit == PalwLegacyHeldUnitV2::FpJob
                            && kaspa_consensus_core::palw_legacy_held_da_v2::palw_legacy_held_check_answer_v3(
                                &honest.execution_root, &answer.unit, &answer.binding, &answer.answer,
                                answer.binding.step_leaf_count, chain.config.params.palw_prompt_ids_form_at(chain.daa()),
                            ).is_ok())),
                        "the correct job was never carried as a direct answer"
                    );
                }
                assert!(
                    carriers.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::DisputeReleasedV1 { claim: c, reserver, .. } if *c == claim && *reserver == bond)),
                    "independent successful replay releases the input-bootstrap reservation through the actual service"
                );
                assert!(!carriers.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::ObjectiveOffence { evidence, .. }
                        if borsh::from_slice::<kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1>(evidence)
                            .is_ok_and(|e| e.claim_id == claim))));
                eprintln!(
                    "[service-public] honest pruned FP input reacquired, independently replayed and reservation released at DAA {}",
                    chain.daa()
                );
                break;
            }
        }
        if case.exact_fault() {
            assert!(!view.da_defaulted, "disclosed job fraud must reach exact conviction, not DA default");
            let offence =
                kaspa_consensus_core::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&view.producer.0, &claim);
            let filing = chain.tc.palw_reporter_filing_read_v1(offence, Hash64::default(), bond, None).unwrap();
            if let Some(consumed) = &filing.consumed {
                assert_eq!(consumed.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::ExecutorRefuted);
                assert_eq!(consumed.accused, view.producer.0);
                assert!(matches!(view.phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }));
                {
                    // A kind-3 identity conviction also debits the producer. When it lands first,
                    // kind 4 records the same fault without charging or rewarding it twice.
                    let producer_after = chain.tc.palw_operator_da_standing_v1(view.producer).unwrap().collateral;
                    assert!(producer_capital > 0);
                    assert!(producer_after < producer_capital, "the proven fraud collects the producer's forfeit and S2 action");
                    let producer_reward = filing.pending.and_then(|p| p.best);
                    let mut panels = Vec::new();
                    for seat in &all_seats {
                        let key = kaspa_consensus_core::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim);
                        let panel = chain.tc.palw_reporter_filing_read_v1(key, Hash64::default(), bond, None).unwrap();
                        if let (Some(consumed), Some(winner)) = (panel.consumed, panel.pending.and_then(|p| p.best)) {
                            assert_eq!(consumed.kind, kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2);
                            assert_eq!(consumed.accused, seat.0);
                            assert!(consumed.amount > 0);
                            assert!(consumed.collected > 0, "each relied-on Valid signer actually loses collateral");
                            assert_eq!(winner.reporter, bond);
                            assert!(winner.committed_daa < consumed.accepted_daa);
                            panels.push((*seat, key, winner));
                        }
                    }
                    if panels.len() != all_seats.len() {
                        for _ in 0..10 {
                            pool_heartbeat(&mut chain, &manager, &pool).await;
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        continue;
                    }
                    if let Some(best) = &producer_reward {
                        assert_eq!(best.reporter, bond);
                        assert!(best.committed_daa < consumed.accepted_daa);
                    } else {
                        assert_eq!(consumed.amount, 0, "only a duplicate producer leg has no reporter reward");
                    }
                    let carriers = chain.history[service_start..]
                        .iter()
                        .flat_map(|b| b.transactions.iter())
                        .filter_map(|t| borsh::from_slice::<PalwLifecycleTxPayloadV2>(&t.payload).ok())
                        .map(|p| p.object)
                        .collect::<Vec<_>>();
                    assert!(
                        chain.history[service_start..]
                            .iter()
                            .flat_map(|b| b.transactions.iter())
                            .any(|t| Some(t.id()) == answer_submitted)
                    );
                    if let Some(best) = &producer_reward {
                        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterCommitted { commitment, reporter, .. } if *commitment == best.commitment && *reporter == bond)));
                        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterRevealed { offence_key, reporter, .. } if *offence_key == offence && *reporter == bond)));
                    }
                    let evidence = carriers
                        .iter()
                        .find_map(|o| match o {
                            PalwConsensusObjectV2::ObjectiveOffence { kind, accused, evidence, .. }
                                if *kind == consumed.kind && *accused == view.producer =>
                            {
                                borsh::from_slice::<kaspa_consensus_core::palw_offence_attribution_v1::PalwExecutorRefutedEvidenceV1>(
                                    evidence,
                                )
                                .ok()
                            }
                            _ => None,
                        })
                        .expect("the verifier's exact proof is included in an actual block");
                    assert_eq!(evidence.claim_id, claim);
                    if case.constraint_fault().is_some() {
                        let kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::ConstrainedDecodeFlat {
                            accusation, ..
                        } = evidence.contradiction
                        else {
                            panic!("exact constraint token proof, not identity/output/default")
                        };
                        assert_eq!(accusation.position, 2);
                        assert!(job_preimage_submitted, "the outsider reacquired the original job before proof generation");
                        match case.constraint_fault().unwrap() {
                            1 => assert_eq!(accusation.pin.generated_token_ids[2], 2),
                            2 => assert!(accusation.witness.is_some()),
                            3 => assert!(accusation.beat_lane.is_some()),
                            _ => unreachable!(),
                        }
                    } else {
                        assert!(matches!(
                            evidence.contradiction,
                            kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::IdentityMismatch { .. }
                        ));
                    }
                    for (seat, key, winner) in &panels {
                        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::ObjectiveOffence { kind, accused, .. } if *kind == kaspa_consensus_core::palw_offence_v1::PalwOffenceKindV1::PanelFalseValidV2 && *accused == *seat)));
                        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterCommitted { commitment, reporter, .. } if *commitment == winner.commitment && *reporter == bond)));
                        assert!(carriers.iter().any(|o| matches!(o, PalwConsensusObjectV2::ReporterRevealed { offence_key, reporter, .. } if *offence_key == *key && *reporter == bond)));
                    }
                    assert!(reserved && demanded);
                    assert!(chain.tc.palw_fraud_filer_status_v1(bond).unwrap().live.iter().all(|r| r.claim_id != claim));
                    eprintln!(
                        "[service-public] exact {case:?} conviction plus all {} Valid signer sanctions/reveals at DAA {}, producer collateral {} -> {}, kind4 consumed {} additional amount {}",
                        panels.len(),
                        chain.daa(),
                        producer_capital,
                        producer_after,
                        consumed.accepted_daa,
                        consumed.amount
                    );
                    break;
                }
            }
        }
        if view.da_defaulted {
            assert!(case.da_default(), "an honest answered input must never default");
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
            if case == PublicCase::PrunedWithheldFpV3Input {
                assert!(answer_submitted.is_some(), "the producer disclosed the correct binding first");
                assert!(carriers.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand }
                        if demand.claim == claim && demand.accuser == bond
                            && matches!(demand.unit, kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::PromptIds { .. }))),
                    "the default follows an actual authenticated-input obligation, not a missing binding");
            }
            if matches!(case, PublicCase::UnretainedWithheldFpV3Job | PublicCase::UnretainedWithheldLargeConstraintJob) {
                assert!(answer_submitted.is_some(), "the producer disclosed the correct binding first");
                assert!(
                    carriers.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand }
                        if demand.claim == claim && demand.accuser == bond
                            && demand.unit == kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::FpJob)),
                    "the default must follow the publicly authenticated missing-job obligation"
                );
                assert!(
                    !carriers.iter().any(|object| matches!(object,
                    PalwConsensusObjectV2::LegacyHeldDemandedV2 { demand }
                        if demand.claim == claim && matches!(demand.unit,
                            kaspa_consensus_core::palw_legacy_held_da_v2::PalwLegacyHeldUnitV2::PromptIds { .. }))),
                    "input acquisition cannot start before an authenticated job exists"
                );
            }
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
                "[service-public] objective DA default at {} after {carried} mined service batches, required producer material withheld",
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
        } else {
            // Public history/input retries are paced in DAA. Keep the ordinary chain
            // producing before the target reserves, including while another claim is pursued.
            pool_heartbeat(&mut chain, &manager, &pool).await;
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
