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
fn fixture_config(fp_v4: bool, fp_input_bootstrap: bool) -> (Config, PalwConsensusParamsV2, Vec<Funding>, Vec<Funding>) {
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
    if fp_v4 {
        // Exercise the admitted V4 family under the normal deployable decode-rules fence.
        // No public-prosecution eligibility/admission callback is substituted.
        params.palw_fp_decode_rules = Some(ForkActivation::new(0));
        params.sync_palw_fp_decode_rules();
    }
    params.validate_palw_v2().expect("fixture keys and the ordinary decode-rules fence validate");
    params.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PublicCase {
    Withholding,
    ForeignJob,
    ForeignFpJob,
    ForeignFpV3Job,
    PrunedForeignFpV3Job,
}

impl PublicCase {
    fn is_fp(self) -> bool {
        matches!(self, Self::ForeignFpJob | Self::ForeignFpV3Job | Self::PrunedForeignFpV3Job)
    }
}

/// The producer executes a real registered model under FP job B, then claims the roots
/// for user-prompt job A. Only its signed Event is later shared with the outsider.
fn foreign_fp_job(
    chain: &Chain,
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
    if chain.config.params.palw_fp_decode_rules_active_at(chain.daa()) {
        job = job.into_v4(kaspa_consensus_core::palw_decode_pipeline_v4::DecodeConfigV4::NOOP);
    }
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
    let run = backend.execute_free_prompt(&job, &prompt).unwrap();
    let shape = backend.capture_shape(&run.outcome.material).unwrap();
    let mut commitment = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_commitment_from_context_v3(
        &job,
        &shape.job_context,
        &run,
        chain.daa() + facts.min_trace_retention_daa,
    )
    .unwrap();
    let event = backend.disclose_trace_event(&run.outcome.material, 0, 0).unwrap();
    // The claimed job A has a fully valid user prompt and schedule; its authenticated
    // execution material belongs to B. Admission must not require trusting Panel replay.
    job.job_nonce[0] ^= 1;
    let ctx = backend.fp_job_context_for_executed_v1(&job, commitment.decode_tokens_executed).unwrap();
    commitment.job = job;
    commitment.schedule_root = kaspa_consensus_core::palw_v2::expected_schedule_commitment_v2(
        &ctx.context_hash(),
        commitment.job.prompt_tokens,
        commitment.decode_tokens_executed,
    )
    .0;
    (run.outcome, event, Some((commitment, prompt.into_iter().map(|t| t as u32).collect())))
}

async fn fresh_public_service(case: PublicCase) {
    kaspa_core::log::try_init_logger("warn,kaspad_lib::palw_panel=info");
    let (config, bundle, premine, mut floats) =
        fixture_config(case == PublicCase::ForeignFpJob, case == PublicCase::PrunedForeignFpV3Job);
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
            return foreign_fp_job(&chain);
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
    assert!(chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == claim && !c.seat));
    if case.is_fp() {
        let candidate = chain.tc.palw_fraud_filer_candidates_v1(bond).into_iter().find(|c| c.claim_id == claim).unwrap();
        assert!(candidate.job.free_prompt, "public discovery identifies the ordinarily admitted FP lane");
        let public_job = chain.view(claim).public_fp_job.expect("the ordinary accepted extractor retained authenticated job metadata");
        let stored: kaspa_consensus_core::palw_freeprompt_v3::PalwFreePromptCommitmentV3 = borsh::from_slice(&public_job).unwrap();
        assert_eq!(kaspa_consensus_core::palw_freeprompt_v3::fp_claim_id_v3(&stored), claim);
        if case == PublicCase::PrunedForeignFpV3Job {
            use kaspa_consensus::model::stores::acceptance_data::AcceptanceDataStore;
            use kaspa_consensus::model::stores::block_transactions::BlockTransactionsStore;
            let commitment_block = chain.history.iter().find(|b| b.transactions.iter().any(|t| {
                t.subnetwork_id == kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT
                    && borsh::from_slice::<kaspa_consensus_core::palw_freeprompt_v3::PalwFpCommitmentTxPayloadV3>(&t.payload)
                        .is_ok_and(|p| p.claim_id() == claim)
            })).unwrap().hash();
            // Delete real DB rows and caches, rather than substituting a read API. This tests lost history,
            // independently of the still-open network pruned-import/recovery requirement.
            chain.tc.block_transactions_store.delete(commitment_block).unwrap();
            chain.tc.acceptance_data_store.delete(candidate.job.accepted_block).unwrap();
            assert!(chain.tc.get_block(commitment_block).is_err());
            assert!(chain.tc.get_block_acceptance_data(candidate.job.accepted_block).is_err());
            let reacquired = super::palw_fraud_filer::palw_fraud_filer_fp_payload_v1(chain.tc.as_ref().as_ref(), &candidate, chain.network_domain)
                .expect("public rooted metadata starts prosecution despite missing original accepted block history");
            assert_eq!(reacquired.commitment, stored);
            assert!(reacquired.prompt_token_ids.is_empty());
            assert!(reacquired.signature.is_empty());
            assert!(chain.tc.palw_fraud_filer_candidates_v1(bond).iter().any(|c| c.claim_id == claim && !c.seat),
                "ordinary candidate discovery survives missing commitment history");
            assert!(chain.config.params.palw_legacy_held_da_v2_active_at(chain.daa()),
                "the input bootstrap's own dormant fence is active in this fixture");
            eprintln!("[service-public] original FP commitment body and acceptance rows pruned; only rooted public job metadata remains");
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
        if demanded
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
            assert!(!disclosed.unit_was_answered_v1(&PalwDaUnitV1::Event { row: 0, tile: 0 }));
            assert!(!disclosed.executor_refuted && !disclosed.da_defaulted, "carriage itself is neither a DA answer nor conviction");
            eprintln!("[service-public] authenticated J1 binding carried; DA answer remains refused until the verifier files");
        }
        if case != PublicCase::Withholding {
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
                    assert!(matches!(
                        evidence.contradiction,
                        kaspa_consensus_core::palw_offence_v1::PalwPanelContradictionV1::IdentityMismatch { .. }
                    ));
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
