//! Signed tokenizer registration through the ordinary node pipeline on an
//! isolated test genesis. The class, its container and EOF enter by carrier.
use super::*;
use crate::model::stores::virtual_state::VirtualStateStoreReader;
use kaspa_consensus_core::{
    coinbase::MinerData,
    config::params::TokenizerRegistrationFenceV1,
    dns_finality::split_normal_tx_fees,
    mldsa87_primitives::MLDSA87_SIGNATURE_LEN,
    palw_class_semantics_v1::{
        PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1, PalwClassSemanticsDeclarationV1, palw_class_semantics_message_v1,
    },
    palw_constraint_stream_carriage_v1::{
        PALW_CONSTRAINT_STREAM_CONTEXT_V1, PalwConstraintStreamActionV1, PalwConstraintStreamMoveV1,
    },
    palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1,
    palw_court_service_v1::ServiceCost,
    palw_fp_constraint_job_v1::PalwTokenTableV1,
    palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    palw_state_v2::PalwStateCarriageV2,
    palw_tir_container_v1::*,
    palw_tokenizer_artifact_inventory_v1::*,
    palw_tokenizer_registration_carriage_v1::encode_tokenizer_registration_payload_v1,
    palw_tokenizer_registration_state_v1::TokenizerRegistrationStatusV1,
    palw_tokenizer_registration_v1::*,
    palw_tokenizer_rendering_v1::{PalwTokenizerRenderingRuleV1, palw_tokenizer_key_events_v1, palw_tokenizer_raw_commitment_v1},
    palw_tokenizer_rendering_v2::{PalwTfp1TokenizerIntakePinsV2, PalwTokenizerRenderingStatementV2},
};
use kaspa_database::{create_temp_db, prelude::ConnBuilder};

const START: u64 = 46;
const FEE: u64 = 2_000_000;
const REGISTRANT: usize = 2;
const EOF_OWNER: usize = 3;
const OPENER: usize = 4;
const PART_PAYER: usize = 5;
const FINISH_PAYER: usize = 6;
const RETRY_PAYER: usize = 7;

fn isolated_config() -> (Config, PalwConsensusParamsV2, Premine, Premine, TokenizerRegistrationPolicyV1) {
    let (mut config, _, premine, floats) = t12_release_compressed_over(Some(44));
    let policy = configure_tokenizer_registration(&mut config.params);
    let PalwConsensusMode::ConsensusV2(bundle) = &config.params.palw_consensus_mode else { unreachable!() };
    (config.clone(), bundle.clone(), premine, floats, policy)
}

pub(in crate::pipeline::virtual_processor::tests) fn configure_tokenizer_registration(
    p: &mut kaspa_consensus_core::config::params::Params,
) -> TokenizerRegistrationPolicyV1 {
    p.genesis =
        kaspa_consensus_core::palw_g14_gates_v1::palw_isolated_g14_genesis_v1(&p.genesis, Hash64::from_u64_word(0x544b5231)).unwrap();
    let at = ForkActivation::new(START);
    if p.palw_tir_v1.as_ref().is_none_or(|f| !f.activation.is_active(START)) {
        p.palw_tir_v1 = Some(PalwTirFenceV1::testnet12_v1(at));
        p.sync_palw_tir_v1();
    }
    p.palw_audit_2026_09_11 = Some(ForkActivation::always());
    p.palw_class_semantics_v1 = Some(at);
    p.palw_legacy_public_filer_v1 = Some(at);
    p.palw_staged_public_court_v1 = Some(at);
    p.palw_staged_tir_route_v1 = Some(at);
    p.palw_tir_container_identity_v1 = Some(at);
    p.palw_tir_public_conviction_v1 = Some(at);
    p.palw_tir_frame_cost_v1 = Some(kaspa_consensus_core::palw_g14_values_v1::PALW_G14_TIR_FRAME_COST_ESTIMATE_V1);
    if p.palw_rcore_plus_fence().is_none_or(|f| !f.is_active(START)) {
        p.palw_rcore_plus = Some(at);
    }
    if p.palw_objective_offence.is_none_or(|f| !f.is_active(START)) {
        p.palw_objective_offence = Some(at);
    }
    if p.palw_offence_attribution.is_none_or(|f| !f.is_active(START)) {
        p.palw_offence_attribution = Some(at);
    }
    if p.palw_capacity_aggregate_liability.as_ref().is_none_or(|f| !f.activation.is_active(START)) {
        p.palw_capacity_aggregate_liability =
            Some(kaspa_consensus_core::palw_aggregate_liability_v1::PalwCapacityLiabilityV1::t12_at_v1(at));
    }
    p.sync_palw_legacy_public_filer_v1();
    p.sync_palw_capacity_liability();
    p.sync_palw_rcore_plus();
    p.sync_palw_2026_10_12_ruling_fences_v1();
    p.sync_palw_g14_gates_v1();
    let sources = p.tokenizer_registration_source_blocks_bound_v1().unwrap();
    let mut policy = TokenizerRegistrationPolicyV1 {
        version: TOKENIZER_REGISTRATION_VERSION_V1,
        max_raw_bytes: 65_536,
        max_vocab: 262_144,
        max_key_events: 4_096,
        max_eog_ids: 4,
        max_rendering_bytes: 65_536,
        max_parser_scratch_bytes: 16_000_000,
        max_carrier_bytes: 80_000,
        max_retained_bytes: 1_000_000,
        max_retained_total_bytes: 4_000_000,
        maximum_source_blocks: sources,
        service_budget: ServiceCost {
            bytes: 80_000 * sources,
            work: 50_000_000 * sources,
            io: 1_000_000 * sources,
            controls: sources,
        },
        max_work: 50_000_000,
        max_pending_per_opener: 2,
        max_pending_total: 4,
        max_session_daa: 100,
        retention_daa: 20,
        carrier_overhead_bytes: 4,
        retention_copies: 2,
        metadata_bytes: 32_000,
        work_rates: [1; 7],
        bookkeeping_work_per_step: 1,
        rent_per_carrier_byte: 1,
        rent_per_retained_byte_daa: 1,
        rent_per_work: 1,
        max_registration_charge: 100_000_000,
    };
    policy.service_budget.work += policy.bookkeeping_work_v1().unwrap().sweep;
    p.palw_tokenizer_registration_v1 = Some(TokenizerRegistrationFenceV1 { activation: at, open_cutoff: None, policy });
    assert!(p.has_isolated_g14_genesis_v1());
    p.validate_palw_v2().expect("all node parameter validation on the isolated genesis");
    policy
}

fn signed_bytes(card: usize, message: &[u8], purpose: &[u8]) -> [u8; MLDSA87_SIGNATURE_LEN] {
    *libcrux_ml_dsa::ml_dsa_87::sign(&TestConsensus::palw_v2_registry_keypair(card as u64).signing_key, message, purpose, [0x5b; 32])
        .unwrap()
        .as_ref()
}

fn native_funding(env: &Env, tx: &Transaction) -> (TransactionOutpoint, UtxoEntry) {
    let outpoint = TransactionOutpoint::new(tx.id(), 0);
    let entry = env.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).expect("the funding output actually exists");
    (outpoint, entry)
}

async fn publish(
    env: &mut Env,
    object: Obj,
    card: usize,
    funding: (TransactionOutpoint, UtxoEntry),
) -> (TransactionOutpoint, UtxoEntry) {
    let carrier = carrier_from(env, &object, card, funding, FEE);
    mine_carrier(&mut env.chain, &carrier).await;
    native_funding(env, &carrier)
}

async fn prerequisites(m: &mut Mined, raw: &[u8], table: &PalwTokenTableV1) -> PalwTfp1TokenizerIntakePinsV2 {
    let Obj::ClassRegisteredTirV1 { artifact_root, admission, .. } = &m.object else { unreachable!() };
    let class_id = m.class_id;
    let artifact_root = *artifact_root;
    let canonical = &admission.canonical;
    let layout = admission.class.layout.clone();
    let declaration = PalwClassSemanticsDeclarationV1 {
        version: 1,
        class_id,
        artifact_root,
        model_profile_id: canonical.model_profile_id,
        runtime_manifest_hash: canonical.runtime_manifest_hash,
        runtime_class_id: canonical.runtime_class_id,
        tokenizer_id: table.tokenizer_id,
        cu_ruleset_id: canonical.cu_ruleset_id,
        token_table_root: table.root(),
        rendering_rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        raw_tokenizer_bytes: raw.len() as u64,
        tokenizer_key_events: palw_tokenizer_key_events_v1(raw).unwrap(),
        prefix_state_relation: None,
    };
    let mut funding = native_funding(&m.env, &m.carrier);
    funding = publish(&mut m.env, Obj::TirClassLayoutPublishedV1 { class_id, layout: Box::new(layout) }, REGISTRANT, funding).await;
    let message = palw_class_semantics_message_v1(m.env.domain, &declaration);
    let signature = signed_bytes(REGISTRANT, message.as_byte_slice(), PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1).to_vec();
    funding =
        publish(&mut m.env, Obj::ClassSemanticsDeclaredV1 { declaration: Box::new(declaration), signature }, REGISTRANT, funding)
            .await;

    let program = m.record.program.as_ref().clone();
    let opening = PalwArtRootOpeningV1 {
        version: PALWART_VERSION_V3,
        manifest_digest: PalwArtRootOpeningV1::manifest_digest_of_v1(b"isolated TKR1 fixture"),
        program_kind: PALWART_PROGRAM_TIR_V1_V3,
        kernel_id: kaspa_consensus_core::palw_tir_artifact_v1::palw_tir_graph_ir_root_v1(&program),
        program,
        kernel_version: 0,
        tokenizer_id: table.tokenizer_id,
        sections: vec![PalwArtSectionOpeningV1 {
            kind: PALWART_SECTION_TIR_INVENTORY_V3,
            family: 1,
            name: "tir-inventory".into(),
            inner_root_scheme: PALWART_INNER_TIR_INVENTORY_ROOT_V3,
            inner_root: artifact_root,
            inner_leaf_count: 3,
            entries_root: Hash64::from_u64_word(8),
        }],
    };
    let binding = PalwTirContainerBindingV1 {
        version: PALW_TIR_CONTAINER_BINDING_VERSION_V1,
        class_id,
        container_root: opening.root_v1(),
        opening,
    };
    check_tir_container_binding_v1(&binding, &class_id, &artifact_root, &m.record, m.env.chain.daa_of(m.env.chain.sink())).unwrap();
    let message = palw_tir_container_binding_message_v1(m.env.domain, &binding);
    let signature = signed_bytes(REGISTRANT, message.as_byte_slice(), PALW_TIR_CONTAINER_BINDING_SIGNATURE_CONTEXT_V1).to_vec();
    let container_root = binding.container_root;
    publish(&mut m.env, Obj::TirClassContainerBoundV1 { binding: Box::new(binding), signature }, REGISTRANT, funding).await;

    let mut offer = PalwTokenizerArtifactInventoryOfferV1 {
        version: 1,
        artifact_hash: table.tokenizer_id,
        opener: m.env.chain.bonds[EOF_OWNER],
        maximum_bytes: raw.len() as u64,
        accept_until_daa: m.env.chain.daa_of(m.env.chain.sink()) + 30,
        signature: Vec::new(),
    };
    offer.signature = signed_bytes(EOF_OWNER, offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).to_vec();
    let movement = |reserved_daa, action| {
        let mut movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: table.tokenizer_id,
            reserver: offer.opener,
            reserved_daa,
            seed_id: offer.id_v1(),
            phase: PalwConstraintStreamPhaseV1::PrefixCourt,
            action,
            signature: Vec::new(),
        };
        movement.signature = signed_bytes(EOF_OWNER, &movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).to_vec();
        Obj::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
    };
    let funding = m.env.floats[EOF_OWNER].clone();
    let funding = publish(
        &mut m.env,
        movement(0, PalwConstraintStreamActionV1::ArtifactInventoryOpen { class_id, offer: offer.clone() }),
        EOF_OWNER,
        funding,
    )
    .await;
    let opened = m.env.chain.tip_state().1.staged_tokenizer_inventory_open_v1(table.tokenizer_id, offer.opener).unwrap().opened_daa;
    let funding = publish(
        &mut m.env,
        movement(
            opened,
            PalwConstraintStreamActionV1::ArtifactInventoryMove(PalwTokenizerArtifactInventoryActionV1::Part {
                offset: 0,
                bytes: raw.to_vec(),
            }),
        ),
        EOF_OWNER,
        funding,
    )
    .await;
    publish(
        &mut m.env,
        movement(opened, PalwConstraintStreamActionV1::ArtifactInventoryMove(PalwTokenizerArtifactInventoryActionV1::Finish)),
        EOF_OWNER,
        funding,
    )
    .await;
    warm_to(&mut m.env, m.activation).await;
    let state = m.env.chain.tip_state().1;
    let eof = state.staged_tokenizer_inventory_v1(table.tokenizer_id).expect("the legacy EOF was accepted");
    assert_eq!(eof.actual_bytes, raw.len() as u64);
    assert_eq!(palw_tir_bound_container_v1(&state, class_id).unwrap().container_root, container_root);
    let semantics_id = state.class_semantics_v1(&class_id).expect("the signed declaration was accepted").commitment();
    PalwTfp1TokenizerIntakePinsV2 { class_id, container_root, semantics_id, inventory_id: eof.inventory_id }
}

fn tkr_carrier(env: &Env, wire: &TokenizerRegistrationWireV1, card: usize, fee: u64) -> Transaction {
    let (outpoint, entry) = env.floats[card].clone();
    let mut tx = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(outpoint, Vec::new(), 0, 1)],
        vec![TransactionOutput::new(entry.amount.checked_sub(fee).unwrap(), card_payout_spk(card))],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        encode_tokenizer_registration_payload_v1(wire).unwrap(),
    );
    sign_spend(&mut tx, entry, card, env.config.params.storage_mass_parameter);
    tx
}

async fn wait_for_source_daa(env: &mut Env, target: u64, part_slot: u64, finish_slot: u64) {
    let ttpb = env.config.params.target_time_per_block().max(kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS);
    while env.chain.ctx.consensus.get_virtual_daa_score() < target {
        env.chain.heartbeat(ttpb, vec![]).await;
    }
    let (accepted_block, state) = env.chain.tip_state();
    eprintln!(
        "[tkr1] source target={target} sink_daa={} accepted_block={accepted_block} accepted_point={:?} virtual_daa={} part_slot={part_slot} finish_slot={finish_slot}",
        env.chain.daa_of(env.chain.sink()),
        state.last_point(),
        env.chain.ctx.consensus.get_virtual_daa_score(),
    );
    assert_eq!(env.chain.ctx.consensus.get_virtual_daa_score(), target);
    assert_eq!(env.chain.daa_of(env.chain.sink()), target - 1);
}

fn assert_collectible(env: &Env, source: &Block, amount: u64) {
    let vp = env.chain.vp();
    let stores = vp.virtual_stores.read();
    let view = stores.state.get().unwrap();
    assert_eq!(view.mergeset_rewards.get(&source.header.hash).expect("virtual merges the source").total_fees, amount);
}

fn assert_utxo_accepted(env: &Env, folding: &Block, tx: &Transaction) {
    let c = &env.chain.ctx.consensus;
    let rows = c.get_block_acceptance_data(folding.header.hash).unwrap();
    assert!(rows.iter().flat_map(|row| &row.accepted_transactions).any(|entry| entry.transaction_id == tx.id()));
    assert!(c.get_virtual_utxo_entry(tx.inputs[0].previous_outpoint).is_none());
    assert_eq!(c.get_virtual_utxo_entry(TransactionOutpoint::new(tx.id(), 0)).unwrap().amount, tx.outputs[0].value);
}

#[test]
fn signed_tkr1_open_parts_finish_and_duplicate_use_the_actual_node() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(run()))
        .unwrap()
        .join()
        .unwrap();
}

async fn prepare_class(
    env: Env,
    policy: TokenizerRegistrationPolicyV1,
) -> (Mined, TokenizerRegistrationOpenBodyV1, &'static [u8], kaspa_consensus_core::palw_fp_constraint_job_v1::PalwTokenTableEogOpeningV1)
{
    let raw = br#"{"model":{"vocab":{"A":0},"merges":[]},"added_tokens":[]}"#;
    let mut c = case("dense-gqa-2layer");
    c.class.tokenizer_id = palw_tokenizer_raw_commitment_v1(raw);
    let mut m = mine_registration_on(env, &c, REGISTRANT).await;
    let mut entries = vec![None; m.record.logits_vocab as usize];
    entries[0] = Some(b"A".to_vec());
    let table = PalwTokenTableV1 { entries, eog_token_ids: vec![0], tokenizer_id: c.class.tokenizer_id };
    let pins = prerequisites(&mut m, raw, &table).await;
    let eog = table.eog_opening().unwrap();
    let now = m.env.chain.daa_of(m.env.chain.sink());
    let body = TokenizerRegistrationOpenBodyV1 {
        version: TOKENIZER_REGISTRATION_VERSION_V1,
        network: m.env.domain,
        policy: policy.id_v1().unwrap(),
        pins,
        artifact_root: c.root,
        statement: PalwTokenizerRenderingStatementV2 {
            tokenizer_id: table.tokenizer_id,
            exact_raw_bytes: raw.len() as u64,
            table_root: table.root(),
            vocab: table.vocab(),
            rule: PalwTokenizerRenderingRuleV1::QwenByteLevelJsonV1,
        },
        eog_digest: TokenizerRegistrationFinishV1::eog_digest_v1(&eog).unwrap(),
        opener: m.env.chain.bonds[OPENER],
        nonce: Hash64::from_u64_word(0x544b5231),
        accept_until_daa: now + 20,
        finish_until_daa: now + 40,
        material_until_daa: now + 40 + policy.retention_daa,
        max_registration_charge: policy.max_registration_charge,
    };
    (m, body, raw, eog)
}

pub(in crate::pipeline::virtual_processor::tests) async fn prepare_round_class(
    chain: T12Chain,
    premine: Premine,
    floats: Premine,
) -> (T12Chain, (TokenizerRegistrationOpenBodyV1, Premine)) {
    let config = chain.config.clone();
    let bundle = chain.bundle.clone();
    let policy = config.params.palw_tokenizer_registration_v1.as_ref().unwrap().policy;
    let env = Env::around(chain, (config, bundle, premine, floats));
    let (mined, body, _, _) = prepare_class(env, policy).await;
    (mined.env.chain, (body, mined.env.floats))
}

async fn run() {
    let (config, bundle, premine, floats, policy) = isolated_config();
    let (_db_lifetime, db) = create_temp_db!(ConnBuilder::default().with_files_limit(10));
    let (sender, _receiver) = async_channel::unbounded();
    let node = TestConsensus::with_db(db.clone(), &config, sender);
    let chain = t12_genesis_chain_on(node, &config, &bundle, &premine, &floats);
    let mut env = Env::around(chain, (config, bundle, premine, floats));
    let mut malformed = b"TKR1".to_vec();
    malformed.resize(policy.source_budget_v1().unwrap().bytes as usize + 1, 0xff);
    let over_work = Transaction::new(
        crate::constants::TX_VERSION,
        vec![],
        vec![],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
        0,
        malformed,
    );
    assert!(
        env.chain.ctx.consensus.tokenizer_registration_local_charge_v1(&over_work).is_none(),
        "the dormant wire retains its legacy meaning"
    );
    warm_to(&mut env, START).await;
    let (mut m, body, raw, eog) = prepare_class(env, policy).await;
    // A tick-bearing heartbeat uses the recovery cadence. Target block time
    // alone permits repeated-DAA beats, which cannot deliver an exact slot.
    let ttpb =
        m.env.config.params.target_time_per_block().max(kaspa_consensus_core::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS);
    let open_id = body.id_v1().unwrap();
    let preview = m.env.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap();
    assert_eq!(preview.parent_point.block, m.env.chain.sink());
    assert_eq!(preview.parent_root, m.env.chain.tip_state().1.state_root());
    let quote = preview.value.unwrap();
    assert_eq!(quote.open_id, open_id);
    assert_eq!(quote.quote, policy.quote_v1(&body, preview.quote_daa, quote.declared_key_events).unwrap());
    assert!(m.env.chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.is_none());

    let open = TokenizerRegistrationOpenV1 {
        body,
        signature: signed_bytes(OPENER, open_id.as_byte_slice(), TOKENIZER_REGISTRATION_OPEN_CONTEXT_V1),
    };
    let open_wire = TokenizerRegistrationWireV1::Open(open);
    let gross = quote.quote.charge + FEE;
    let open_tx = tkr_carrier(&m.env, &open_wire, OPENER, gross);
    mempool_verdict(&m.env, &open_tx).expect("a properly funded signed TKR1 Open is standard at the test clock");
    {
        use kaspa_mining::{
            MiningCounters,
            errors::MiningManagerError,
            manager::MiningManager,
            mempool::{
                errors::RuleError,
                tx::{Orphan, Priority, RbfPolicy},
            },
        };
        let manager = MiningManager::new(
            m.env.config.params.target_time_per_block(),
            true,
            m.env.config.params.max_block_mass,
            None,
            std::sync::Arc::new(MiningCounters::default()),
        );
        let consensus = m.env.chain.ctx.consensus.consensus_clone();
        let insert = |tx| {
            manager.validate_and_insert_transaction(consensus.as_ref(), tx, Priority::High, Orphan::Forbidden, RbfPolicy::Forbidden)
        };
        assert!(
            matches!(insert(over_work), Err(MiningManagerError::MempoolError(RuleError::RejectTokenizerRegistrationSourceWork))),
            "local TKR1 work rejects the oversized carrier before ordinary transaction checks"
        );
        assert!(insert(open_tx.clone()).unwrap().accepted.iter().any(|tx| tx.id() == open_tx.id()));
        assert!(
            matches!(insert(open_tx.clone()), Err(MiningManagerError::MempoolError(RuleError::RejectDuplicate(id))) if id == open_tx.id())
        );
    }

    // Observe the accepted Open before constructing or submitting its Part.
    // The first source at the reserved DAA becomes its clock reference. Its
    // immediate child folds that source at the same DAA.
    let payout = card_payout_spk(OPENER);
    let carrying = m.env.chain.heartbeat_paying(MinerData::new(payout.clone(), vec![]), ttpb, vec![open_tx.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == open_tx.id()));
    let preview_after = m.env.chain.ctx.consensus.palw_tokenizer_registration_open_quote_v1(body).unwrap().unwrap();
    let preview_charge = preview_after.value.unwrap().quote.charge;
    let tip = gross - preview_charge;
    assert_collectible(&m.env, &carrying, tip);
    let open_fold = m.env.chain.heartbeat(ttpb, vec![]).await;
    let open_status = m.env.chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
    assert_eq!(open_status.accepted.block, open_fold.header.hash);
    assert_eq!(open_status.quote.charge, preview_charge);
    assert_eq!(open_status.charge_id, body.charge_id_v1().unwrap());
    assert_eq!(open_status.status, TokenizerRegistrationStatusV1::Pending);
    assert_eq!(open_status.parts.len(), 1);
    assert_eq!(open_status.parts[0].accepted, None);
    assert!(open_status.finishes[0].slot_daa > open_status.parts[0].slot_daa);
    assert_utxo_accepted(&m.env, &open_fold, &open_tx);
    let worker_fee =
        m.env.chain.vp().fee_split_at(open_fold.header.daa_score).map_or(tip, |split| split_normal_tx_fees(tip, &split).worker_sompi);
    assert!(open_fold.transactions[0].outputs.iter().any(|out| out.script_public_key == payout && out.value == worker_fee));

    let mut part = TokenizerRegistrationPartV1 {
        body: TokenizerRegistrationPartBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            index: 0,
            offset: 0,
            payload_digest: TokenizerRegistrationPartV1::payload_digest_v1(raw).unwrap(),
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        bytes: raw.to_vec(),
    };
    part.signature = signed_bytes(OPENER, part.signing_message_v1().unwrap().as_byte_slice(), TOKENIZER_REGISTRATION_PART_CONTEXT_V1);
    let part_tx = tkr_carrier(&m.env, &TokenizerRegistrationWireV1::Part(part), PART_PAYER, FEE);
    let part_slot = open_status.parts[0].slot_daa;
    assert!(part_slot > open_status.accepted.daa_score);
    wait_for_source_daa(&mut m.env, part_slot, part_slot, open_status.finishes[0].slot_daa).await;
    let part_source = m.env.chain.heartbeat(ttpb, vec![part_tx.clone()]).await;
    eprintln!(
        "[tkr1] Part source DAA={} virtual_after={} timestamp={}",
        part_source.header.daa_score,
        m.env.chain.ctx.consensus.get_virtual_daa_score(),
        part_source.header.timestamp
    );
    assert_eq!(part_source.header.daa_score, part_slot);
    assert_eq!(m.env.chain.ctx.consensus.get_virtual_daa_score(), part_slot);
    assert!(part_source.transactions.iter().any(|tx| tx.id() == part_tx.id()));
    let part_fold = m.env.chain.heartbeat(ttpb, vec![]).await;
    assert_eq!(part_fold.header.daa_score, part_slot);
    assert_utxo_accepted(&m.env, &part_fold, &part_tx);
    let part_status = m.env.chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
    let part_state = m.env.chain.tip_state().1;
    let received = part_status.parts[0].accepted.expect("the actual Part folded");
    assert_eq!(received.block, part_fold.header.hash);
    assert_eq!(received.daa_score, open_status.parts[0].slot_daa);

    let mut finish = TokenizerRegistrationFinishV1 {
        body: TokenizerRegistrationFinishBodyV1 {
            version: body.version,
            network: body.network,
            policy: body.policy,
            open_id,
            opener: body.opener,
            eog_digest: body.eog_digest,
            finish_until_daa: body.finish_until_daa,
        },
        signature: [0; MLDSA87_SIGNATURE_LEN],
        eog,
    };
    finish.signature =
        signed_bytes(OPENER, finish.signing_message_v1().unwrap().as_byte_slice(), TOKENIZER_REGISTRATION_FINISH_CONTEXT_V1);
    let finish_tx = tkr_carrier(&m.env, &TokenizerRegistrationWireV1::Finish(finish), FINISH_PAYER, FEE);

    let finish_slot = part_status.finishes[0].slot_daa;
    assert!(finish_slot > received.daa_score);
    wait_for_source_daa(&mut m.env, finish_slot, part_slot, finish_slot).await;
    let finish_source = m.env.chain.heartbeat(ttpb, vec![finish_tx.clone()]).await;
    eprintln!(
        "[tkr1] Finish source DAA={} virtual_after={} timestamp={}",
        finish_source.header.daa_score,
        m.env.chain.ctx.consensus.get_virtual_daa_score(),
        finish_source.header.timestamp
    );
    assert_eq!(finish_source.header.daa_score, finish_slot);
    assert_eq!(m.env.chain.ctx.consensus.get_virtual_daa_score(), finish_slot);
    assert!(finish_source.transactions.iter().any(|tx| tx.id() == finish_tx.id()));
    let finish_fold = m.env.chain.heartbeat(ttpb, vec![]).await;
    assert_eq!(finish_fold.header.daa_score, finish_slot);
    assert_utxo_accepted(&m.env, &finish_fold, &finish_tx);
    let complete = m.env.chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
    assert!(matches!(complete.status, TokenizerRegistrationStatusV1::Complete { point, record }
        if point.block == finish_fold.header.hash && point.daa_score == open_status.finishes[0].slot_daa && record != Hash64::default()));
    assert!(complete.finishes[0].attempted && !complete.finishes[1].attempted);

    let retry = tkr_carrier(&m.env, &open_wire, RETRY_PAYER, FEE);
    let retry_source = m.env.chain.heartbeat(ttpb, vec![retry.clone()]).await;
    assert_collectible(&m.env, &retry_source, FEE);
    let retry_fold = m.env.chain.heartbeat(ttpb, vec![]).await;
    assert_utxo_accepted(&m.env, &retry_fold, &retry);
    let unchanged = m.env.chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
    assert_eq!(*unchanged, *complete);

    let (sink, state) = m.env.chain.tip_state();
    let root = state.state_root();
    let selected = chain_blocks(&m.env.chain, sink);
    let fold_roots =
        [open_fold.header.hash, part_fold.header.hash, finish_fold.header.hash].map(|hash| (hash, root_at(&m.env.chain, hash)));
    // Restore the accepted Part point from the wire carriage authenticated
    // by its child's committed parent root. Discard the importer's locally
    // replayed PALW history through the import point before applying it.
    let fork = selected.iter().position(|block| block.header.hash == part_fold.header.hash).unwrap();
    let p = part_fold.header.hash;
    let importer = fresh_node(&m);
    for block in &selected[..=fork] {
        arrive(&importer, block.clone(), "block through the TKR1 pruning point").await;
    }
    arrive(&importer, Block::from_header_arc(selected[fork + 1].header.clone()), "pruning point child's header").await;
    let api = &importer.ctx.consensus;
    let virtual_utxos = || api.get_virtual_utxos(None, 10_000, false).into_iter().collect::<std::collections::HashMap<_, _>>();
    let verified_utxos = virtual_utxos();
    let wire = {
        let donor = m.env.chain.vp();
        donor.capture_pruning_point_palw_state(p).unwrap();
        borsh::to_vec(&donor.pruning_point_palw_state(p).unwrap()).unwrap()
    };
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&wire).unwrap();
    {
        let vp = importer.vp();
        let mut store = vp.palw_state_v2_store.write();
        store.delete_tip_for_tests().unwrap();
        for hash in std::iter::once(importer.config.params.genesis.hash).chain(selected[..=fork].iter().map(|b| b.header.hash)) {
            store.delete_delta_for_tests(hash).unwrap();
        }
    }
    importer.establish_pruning_import_target(p);
    importer.vp().import_pruning_point_palw_state(p, carriage).expect("authenticated TKR1 snapshot imports at the Part point");
    let (at, imported) = importer.tip_state();
    assert_eq!(at, p);
    assert_eq!(imported.state_root(), fold_roots[1].1);
    for key in [
        PalwStagedCourtKeyV1::TokenizerRegistrationBook,
        PalwStagedCourtKeyV1::TokenizerRegistrationSession(open_id),
        PalwStagedCourtKeyV1::TokenizerRegistrationPart(open_id, 0),
    ] {
        let expected = part_state.staged_court_row_v1(&key).expect("donor row at the accepted Part point");
        assert_eq!(imported.staged_court_row_v1(&key), Some(expected), "imported paid state, including original Part bytes");
    }
    // The snapshot import invalidates the stable marker until the UTXO
    // generation completes. This epoch-zero fixture retains its verified
    // UTXOs throughout, so only the marker needs to be restored.
    assert!(!api.is_pruning_utxoset_stable());
    assert!(api.palw_tokenizer_registration_status_v1(open_id).is_err(), "an incomplete import cannot serve an authoritative status");
    assert_eq!(virtual_utxos(), verified_utxos);
    api.set_pruning_utxoset_stable_flag(true).expect("fixture retains the verified pruning-point UTXOs");
    let imported_status = api.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap();
    assert_eq!(imported_status.parent_point.block, p);
    assert_eq!(imported_status.parent_root, fold_roots[1].1);
    assert_eq!(*imported_status.value.unwrap(), *part_status);
    for block in &selected[fork + 1..] {
        arrive(&importer, block.clone(), "block after the TKR1 pruning point").await;
        assert_eq!(importer.sink(), block.header.hash);
        assert_eq!(importer.tip_state().1.state_root(), root_at(&m.env.chain, block.header.hash));
        if block.header.hash == finish_fold.header.hash {
            let status = importer.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
            assert_eq!(*status, *complete, "Finish folds from the authenticated imported Part bytes");
            assert!(
                importer
                    .ctx
                    .consensus
                    .get_block_acceptance_data(block.header.hash)
                    .unwrap()
                    .iter()
                    .flat_map(|entry| &entry.accepted_transactions)
                    .any(|entry| entry.transaction_id == finish_tx.id())
            );
        }
    }
    assert_eq!(importer.tip_state().1.state_root(), root);
    drop(importer);
    let (simulated_time, nonce) = (m.env.chain.ctx.simulated_time, m.env.chain.nonce_for_reopen());
    let Env { chain, config, bundle, premine, floats, .. } = m.env;
    drop(chain);

    let mut resumed = config.clone();
    resumed.process_genesis = false;
    let (sender, _restart_receiver) = async_channel::unbounded();
    let node = TestConsensus::with_db(db.clone(), &resumed, sender);
    let restarted = t12_reopened_chain(node, &resumed, &bundle, simulated_time, nonce);
    assert_eq!(restarted.sink(), sink);
    assert_eq!(restarted.tip_state().1.state_root(), root);
    let status =
        |chain: &T12Chain| chain.ctx.consensus.palw_tokenizer_registration_status_v1(open_id).unwrap().unwrap().value.unwrap();
    assert_eq!(*status(&restarted), *complete);
    for (hash, want) in fold_roots {
        assert_eq!(root_at(&restarted, hash), want);
    }
    for tx in [&open_tx, &part_tx, &finish_tx, &retry] {
        let node = &restarted.ctx.consensus;
        assert!(node.get_virtual_utxo_entry(tx.inputs[0].previous_outpoint).is_none());
        assert_eq!(node.get_virtual_utxo_entry(TransactionOutpoint::new(tx.id(), 0)).unwrap().amount, tx.outputs[0].value);
    }
    drop(restarted);

    let replay = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in &selected {
        arrive(&replay, block.clone(), "selected TKR1 replay block").await;
    }
    assert_eq!(replay.sink(), sink);
    assert_eq!(replay.tip_state().1.state_root(), root);
    assert_eq!(*status(&replay), *complete);
    for (hash, want) in fold_roots {
        assert_eq!(root_at(&replay, hash), want);
    }

    // Fork after Part but before Finish. The competing branch has the same
    // paid Open and accepted Part and never consumes the original Finish slot.
    let mut competing = t12_genesis_chain(&config, &bundle, &premine, &floats);
    for block in &selected[..=fork] {
        arrive(&competing, block.clone(), "common TKR1 block").await;
    }
    competing.ctx.simulated_time = part_fold.header.timestamp;
    for _ in 0..(selected.len() - fork + 3) {
        let miner = MinerData::new(card_payout_spk(RETRY_PAYER), vec![0x54, 0x4b, 0x52]);
        let block = competing.heartbeat_paying(miner, ttpb, vec![]).await;
        arrive(&replay, block, "competing branch without Finish").await;
    }
    assert_eq!(replay.sink(), competing.sink());
    assert_eq!(replay.tip_state().1.state_root(), competing.tip_state().1.state_root());
    let reorged = status(&replay);
    assert_eq!(*reorged, *part_status);
    assert_eq!(reorged.status, TokenizerRegistrationStatusV1::Pending);
    assert!(!reorged.finishes.iter().any(|finish| finish.attempted));
    assert_eq!(root_at(&replay, finish_fold.header.hash), fold_roots[2].1);
}
