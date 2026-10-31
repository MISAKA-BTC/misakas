//! Real carriers from captured public class/material data. No captured chain
//! state, phase, approval or completion is installed into the node.
use super::*;
use kaspa_consensus_core::palw_class_semantics_v1::*;
use kaspa_consensus_core::palw_constraint_stream_carriage_v1::*;
use kaspa_consensus_core::palw_constraint_stream_court_v1::PalwConstraintStreamPhaseV1;
use kaspa_consensus_core::palw_state_v2::{PalwClassStatusV2, PalwStateCarriageV2};
use kaspa_consensus_core::palw_tir_funded_original_envelope_v1::PalwTirFundedOriginalEnvelopeV1;
use kaspa_consensus_core::palw_tir_funded_original_envelope_v2::PalwTirFundedOriginalEnvelopeV2;
use kaspa_consensus_core::palw_tir_step_v1::PalwTirStepBindingV1;
use kaspa_consensus_core::palw_tokenizer_artifact_inventory_v1::*;
use misaka_palw_sdk::onboarding_chain::{EnvelopeSigner, SignedRegistrationRequestV1};
use std::path::Path;

struct FamilyCardSignerV2(usize);
impl EnvelopeSigner for FamilyCardSignerV2 {
    fn public_key(&self) -> Vec<u8> {
        TestConsensus::palw_v2_registry_keypair(self.0 as u64).verification_key.as_ref().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        libcrux_ml_dsa::ml_dsa_87::sign(
            &TestConsensus::palw_v2_registry_keypair(self.0 as u64).signing_key,
            message,
            context,
            [0xa4; 32],
        )
        .map(|s| s.as_ref().to_vec())
        .map_err(|e| format!("{e:?}"))
    }
}
pub(super) struct FamilyCaptureV2 {
    pub original: PalwTirFundedOriginalEnvelopeV2,
    pub binding: PalwTirStepBindingV1,
    pub semantics: PalwClassSemanticsV1,
    pub raw: Vec<u8>,
    pub object: Obj,
    pub generated: Vec<u32>,
    pub card: usize,
    pub preimages: Vec<kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1>,
    pub hashes: Vec<Hash64>,
    pub operands: Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1>,
    pub prompt: Vec<u32>,
    pub rows: Vec<Vec<i32>>,
}
impl FamilyCaptureV2 {
    pub(super) fn read(dir: &Path, net: &Net) -> Self {
        let read = |name: &str| std::fs::read(dir.join(name)).unwrap_or_else(|e| panic!("{}: {e}", dir.join(name).display()));
        let original = PalwTirFundedOriginalEnvelopeV2::decode_v2(&read("original.tfo2")).unwrap();
        let fp = PalwTirFundedOriginalEnvelopeV1::decode_v1(&original.funded_original)
            .unwrap()
            .original_v1()
            .unwrap()
            .original_fp_v1()
            .unwrap();
        assert_eq!(fp.commitment.job.network_domain, net.domain, "execution must use this actual funded genesis");
        let card = (0..net.funding.len())
            .find(|card| {
                fp.commitment.job.executor_pubkey == card_pubkey(*card) && fp.commitment.job.executor_bond == net.bond(*card).0
            })
            .expect("execution names an actual registered funded card/key pair");
        let (binding, preimages, hashes, operands, prompt, rows, generated): (
            PalwTirStepBindingV1,
            Vec<kaspa_consensus_core::palw_step_leg::PalwStepTileLeafV1>,
            Vec<Hash64>,
            Vec<kaspa_consensus_core::palw_artifact::PalwArtifactOperandV1>,
            Vec<u32>,
            Vec<Vec<i32>>,
            Vec<u32>,
        ) = borsh::from_slice(&read("producer-execution.borsh")).unwrap();
        assert_eq!(binding.committed_execution_root, fp.commitment.execution_root);
        // This file supplies immutable public declarations only; it is never
        // loaded into any consensus state store or used to supply an approval.
        let source: PalwStateCarriageV2 = borsh::from_slice(&read("state.borsh")).unwrap();
        let semantics = source.class_semantics[&fp.commitment.job.class_id].clone();
        let raw = read("raw-tokenizer.json");
        assert_eq!(kaspa_consensus_core::palw_tokenizer_rendering_v1::palw_tokenizer_raw_commitment_v1(&raw), semantics.tokenizer_id);
        let mut object: Obj = borsh::from_slice(&read("original.object.borsh")).unwrap();
        // These native projection fields are deliberately absent from the
        // legacy object wire. Re-derive from the actual signed FP literal for
        // the read-only quote; real node admission extracts them independently.
        if let Obj::FreePromptCommitted { claim, job_pin, public_job, .. } = &mut object {
            assert_eq!(*claim, fp.claim_id());
            *job_pin = kaspa_consensus_core::palw_fp_execution_v3::palw_fp_job_pin_v1(&fp.commitment);
            *public_job = Some(Box::new(fp.commitment.clone()));
        } else {
            panic!("capture must contain the signed FP original projection");
        }
        Self { original, binding, semantics, raw, object, generated, card, preimages, hashes, operands, prompt, rows }
    }
}

/// Structural registration, prospective immutable metadata and raw EOF intake
/// use real signed carriers before any original inference claim. No model
/// quality/correctness screening or promotion is introduced.
pub(super) async fn register_family_capture_class_v2(net: &mut Net, capture: &FamilyCaptureV2) {
    use kaspa_consensus_core::palw_tir_attempt_v1::{PalwTirJobFactsV1, palw_tir_attempt_canonical_v1, palw_tir_job_context_v1};
    use kaspa_consensus_core::palw_tir_class_v1::*;
    let card = capture.card;
    let signer = FamilyCardSignerV2(card);
    let class = capture.semantics.class_id;
    assert!(net.chain.tip_state().1.class(&class).is_none(), "registration precedes the first original");
    let current = net.chain.tip_state().1;
    let base = net.bundle.base_class_id;
    let target = current.class_target(&base).unwrap().target;
    let slash = current.class(&base).unwrap().slash_value_per_pwu;
    let facts = PalwTirJobFactsV1::of_class(&capture.binding.class, class).unwrap();
    let canonical = palw_tir_job_context_v1(&facts, palw_tir_attempt_canonical_v1(&capture.binding.class).unwrap());
    let activation = net.daa() + 30;
    let mut registration = kaspa_consensus_core::palw_tir_admission_v1::palw_tir_post_genesis_registration_v1(
        capture.binding.class.clone(),
        canonical,
        capture.binding.artifact_root,
        0,
        target,
        slash,
        activation,
        net.bond(card),
        Vec::new(),
        net.bundle.court.max_step_leaf_count(),
    )
    .expect("native registration counts the program's canonical work");
    let Obj::ClassRegisteredTirV1 {
        class_id,
        artifact_root,
        slash_value_per_pwu,
        pwu_rule,
        initial_target,
        share_permille,
        activation_daa,
        admission,
    } = &mut registration
    else {
        unreachable!()
    };
    let message = palw_tir_class_registration_message_v1(
        net.domain,
        *class_id,
        *share_permille,
        *activation_daa,
        &admission.registrant_bond,
        *artifact_root,
        *slash_value_per_pwu,
        *initial_target,
        pwu_rule,
        &admission.canonical,
        &admission.class,
    );
    admission.signature = signer.sign_with_context(message.as_byte_slice(), PALW_TIR_CLASS_REGISTRATION_MLDSA87_CONTEXT_V1).unwrap();
    let request =
        SignedRegistrationRequestV1::for_network(registration, net.bond(card), net.daa(), net.daa() + 200, &net.config.params)
            .unwrap();
    net.send(vec![(card, request.sign(&signer).unwrap())]).await;
    assert!(net.chain.tip_state().1.class(&class).is_some(), "actual signed registration must be accepted");
    net.send(vec![(card, Obj::TirClassLayoutPublishedV1 { class_id: class, layout: Box::new(capture.binding.class.layout.clone()) })])
        .await;
    let sem = &capture.semantics;
    let declaration = PalwClassSemanticsDeclarationV1 {
        version: 1,
        class_id: class,
        artifact_root: sem.artifact_root,
        model_profile_id: sem.model_profile_id,
        runtime_manifest_hash: sem.runtime_manifest_hash,
        runtime_class_id: sem.runtime_class_id,
        tokenizer_id: sem.tokenizer_id,
        cu_ruleset_id: sem.cu_ruleset_id,
        token_table_root: sem.token_table_root.unwrap(),
        rendering_rule: sem.rendering_rule.unwrap(),
        raw_tokenizer_bytes: sem.raw_tokenizer_bytes.unwrap(),
        tokenizer_key_events: sem.tokenizer_key_events.unwrap(),
        prefix_state_relation: sem.prefix_state_relation,
    };
    let message = palw_class_semantics_message_v1(net.domain, &declaration);
    let signature = signer.sign_with_context(message.as_byte_slice(), PALW_CLASS_SEMANTICS_SIGNATURE_CONTEXT_V1).unwrap();
    net.send(vec![(card, Obj::ClassSemanticsDeclaredV1 { declaration: Box::new(declaration), signature })]).await;
    assert_eq!(net.chain.tip_state().1.class_semantics_v1(&class).unwrap().token_table_root, sem.token_table_root);
    let mut offer = PalwTokenizerArtifactInventoryOfferV1 {
        version: 1,
        artifact_hash: sem.tokenizer_id,
        opener: net.bond(card),
        maximum_bytes: capture.raw.len() as u64,
        accept_until_daa: net.daa() + 100,
        signature: Vec::new(),
    };
    offer.signature = signer.sign_with_context(offer.id_v1().as_byte_slice(), PALW_ARTIFACT_INVENTORY_CONTEXT_V1).unwrap();
    let id = offer.id_v1();
    let movement = |reserved_daa, action| {
        let mut movement = PalwConstraintStreamMoveV1 {
            version: 1,
            claim: sem.tokenizer_id,
            reserver: offer.opener,
            reserved_daa,
            seed_id: id,
            phase: PalwConstraintStreamPhaseV1::Tokenizer,
            action,
            signature: Vec::new(),
        };
        movement.signature = signer.sign_with_context(&movement.signing_message(), PALW_CONSTRAINT_STREAM_CONTEXT_V1).unwrap();
        Obj::PublicCourtStreamMoveV1 { movement: Box::new(movement) }
    };
    net.send(vec![(card, movement(0, PalwConstraintStreamActionV1::ArtifactInventoryOpen { class_id: class, offer: offer.clone() }))])
        .await;
    let opened = net.chain.tip_state().1.staged_tokenizer_inventory_open_v1(sem.tokenizer_id, offer.opener).unwrap().opened_daa;
    let mut offset = 0;
    for bytes in capture.raw.chunks(PALW_ARTIFACT_INVENTORY_PART_BYTES_V1) {
        net.send(vec![(
            card,
            movement(
                opened,
                PalwConstraintStreamActionV1::ArtifactInventoryMove(PalwTokenizerArtifactInventoryActionV1::Part {
                    offset,
                    bytes: bytes.to_vec(),
                }),
            ),
        )])
        .await;
        offset += bytes.len() as u64;
    }
    net.send(vec![(
        card,
        movement(opened, PalwConstraintStreamActionV1::ArtifactInventoryMove(PalwTokenizerArtifactInventoryActionV1::Finish)),
    )])
    .await;
    assert_eq!(
        net.chain.tip_state().1.staged_tokenizer_inventory_v1(sem.tokenizer_id).unwrap().actual_bytes,
        capture.raw.len() as u64
    );
    net.beat_to(activation).await;
    assert!(
        matches!(net.chain.tip_state().1.class(&class).unwrap().status, PalwClassStatusV2::Active),
        "actual post-registration activation, never a supplied Active flag"
    );
}

pub(super) struct AdmittedFamilyOriginalV2 {
    pub claim: Hash64,
    pub carrier: Transaction,
    pub folding: Block,
    pub origin: kaspa_consensus_core::palw_state_v2::PalwBlockContextV2,
    pub inventories: Vec<(Hash64, Vec<(kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1, u64)>)>,
}

/// Reprice at the node's actual next admission point, then sign each independent
/// duty. The carrier passes the real mempool, native UTXO signatures, template
/// construction, selected-chain insertion and the following block's fold.
pub(super) async fn admit_family_original_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    mut provenance: Option<kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>,
) -> AdmittedFamilyOriginalV2 {
    use kaspa_consensus_core::palw_court_family_supplier_v1::*;
    use kaspa_consensus_core::palw_court_original_escrow_v1::{COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1, FundingConsentV1};
    use kaspa_consensus_core::palw_history_custody_v1::*;
    use kaspa_consensus_core::palw_tir_public_custody_v1::*;
    let card = capture.card;
    let signer = FamilyCardSignerV2(card);
    let producer = net.bond(card);
    let base = PalwTirFundedOriginalEnvelopeV1::decode_v1(&capture.original.funded_original).unwrap();
    let mut envelope = base.original_v1().unwrap();
    let fp = envelope.original_fp_v1().unwrap();
    let claim = fp.claim_id();
    let old = net.chain.tip_state().1;
    let point = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: Hash64::from_u64_word(0xfa42),
        daa_score: net.daa() + 2,
        blue_score: old.last_point().unwrap().blue_score + 2,
        subsidy: 0,
    };
    let table = capture.original.semantic_table.as_deref();
    let allocation = net
        .chain
        .vp()
        .palw_family_original_allocation_quote_v2_impl(point, fp.commitment.clone(), envelope.header.clone(), table)
        .expect("native producer allocation from this actual registered snapshot");
    envelope.pledge.resource_plan_id = allocation.resource_plan_id;
    envelope.pledge.source_id = allocation.source_id;
    envelope.pledge.maximum_custodians = allocation.acquisition.maximum_custodians;
    envelope.pledge.accept_until_daa = point.daa_score + 1;
    envelope.pledge.material_until_daa = allocation.terms.material_until_daa;
    envelope.pledge.signature =
        signer.sign_with_context(envelope.pledge.id_v1().as_byte_slice(), PALW_TIR_SOURCE_PLEDGE_CONTEXT_V1).unwrap();
    envelope.declared_rent_sompi = allocation.terms.plan.rent_sompi;
    let hard = point.daa_score + kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA;
    let material = allocation.terms.material_until_daa.max(hard + allocation.acquisition.config.reorg_retention_daa);
    if let Some(semantic) = &allocation.semantic {
        let mut history = PalwHistoryCustodyPledgeV1 {
            version: 1,
            claim,
            producer,
            execution_root: fp.commitment.execution_root,
            job_identity: envelope.pledge.job_identity,
            cu_ruleset_id: capture.semantics.cu_ruleset_id,
            resource_plan_id: allocation.semantic_resource_plan_id.unwrap(),
            maximum_records: allocation.maximum_semantic_history_records,
            maximum_custodians: allocation.acquisition.maximum_custodians,
            accept_until_daa: point.daa_score + 1,
            material_until_daa: material.max(semantic.terms.material_until_daa),
            signature: Vec::new(),
        };
        history.signature = signer.sign_with_context(history.pledge_id_v1().as_byte_slice(), PALW_HISTORY_PLEDGE_CONTEXT_V1).unwrap();
        envelope.history_pledge = Some(history);
    }
    // Exact producer projection, checked by the quote against the signed FP.
    let object = capture.object.clone();
    let native = net
        .chain
        .vp()
        .palw_family_original_native_quote_v2_impl(point, object, envelope.clone(), table)
        .expect("native full original Source/graph/four-role inventory quote");
    assert_eq!(native.accepted_daa_v2(), point.daa_score);
    let schedule = kaspa_consensus_core::palw_g14_values_v1::palw_g14_funded_original_schedule_estimate_v1();
    let payer_key = signer.public_key();
    let refund = kaspa_hashes::blake2b_512_address_payload(&payer_key);
    let mut funding = base.funding.terms.clone();
    funding.network_domain = net.domain;
    funding.relative_graph_id = native.relative_graph_v2();
    funding.source_id = native.original_plan_v2().source_id;
    funding.fee_profile_id = schedule.fee_profile_id();
    funding.capital_profile_id = schedule.capital_profile_id();
    funding.retention_profile_id = schedule.retention_profile_id();
    funding.settlement_policy_id = schedule.settlement_policy_id();
    funding.accept_from_daa = point.daa_score;
    funding.accept_until_daa = point.daa_score;
    funding.material_until_daa = native.material_until_v2();
    funding.funding_output_index = 1;
    funding.funding_amount_sompi = native.required_original_funding_v2();
    funding.payer_key_id = kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&payer_key);
    funding.refund_payload = refund;
    let funding_signature =
        signer.sign_with_context(funding.id_v1().as_byte_slice(), COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1).unwrap();
    let tfo1 = PalwTirFundedOriginalEnvelopeV1::new_v1(
        envelope.encode_v1().unwrap(),
        FundingConsentV1 { terms: funding, signature: funding_signature },
        payer_key,
    )
    .unwrap();
    let route = old.kernel_route().unwrap();
    let ruleset = Hash64::from_bytes(route.header.policy.ruleset_digest);
    let mut suppliers = Vec::new();
    for inventory in native.inventories_v2() {
        let mut terms = capture.original.suppliers.iter().find(|s| s.terms.role == inventory.role_v1()).unwrap().terms.clone();
        terms.network = net.domain;
        terms.ruleset = ruleset;
        terms.supplier = producer;
        terms.sponsor = producer;
        terms.accept_from_daa = point.daa_score;
        terms.accept_until_daa = point.daa_score;
        terms.respond_until_daa = native.hard_until_v2();
        terms.material_until_daa = native.material_until_v2();
        let terms = kaspa_consensus_core::palw_state_v2::palw_family_supplier_terms_quote_v1(inventory, terms).unwrap();
        suppliers.push(
            sign_family_supplier_v1(terms, |who, message, context| {
                assert_eq!(*who, producer);
                Some(signer.sign_with_context(message, context).unwrap())
            })
            .unwrap(),
        );
    }
    let inventories = native
        .inventories_v2()
        .iter()
        .zip(&suppliers)
        .map(|(inventory, consent)| (consent.id_v1().unwrap(), inventory.targets_v1().to_vec()))
        .collect();
    let funded = PalwTirFundedOriginalEnvelopeV2 {
        version: 2,
        funded_original: tfo1.encode_v1().unwrap(),
        suppliers,
        semantic_table: capture.original.semantic_table.clone(),
    };
    let mut payload = funded.encode_v2().unwrap();
    payload =
        kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_retain_encode_v1(&payload, &capture.generated)
            .unwrap()
            .to_vec();
    if let Some(provenance) = &mut provenance {
        provenance.inner = payload;
        payload = provenance.encode_v1().unwrap();
    }
    let (input, entry) = net.funding[card].clone();
    let fee = envelope.declared_rent_sompi.checked_add(FEE).unwrap();
    let change = entry
        .amount
        .checked_sub(native.required_original_funding_v2())
        .and_then(|n| n.checked_sub(fee))
        .expect("actual finite genesis float must fund original escrow and native carrier rent");
    let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes());
    let mut carrier = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(input, Vec::new(), 0, 1)],
        vec![TransactionOutput::new(change, spk.clone()), TransactionOutput::new(native.required_original_funding_v2(), spk.clone())],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
        0,
        payload,
    );
    crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
        &mut carrier,
        entry,
        card,
        net.config.params.storage_mass_parameter,
    );
    net.mempool(&carrier).expect("actual signed native inputs and TFO2 pass this node's mempool");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()));
    let folding = net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let state = net.chain.tip_state().1;
    let admitted = state.claim(&claim).expect("actual accepted carrier must install its original, not an ordinary fallback");
    assert_eq!(admitted.accepted_daa, point.daa_score, "singleton admission is never silently shifted");
    assert_eq!(
        state.family_supplier_rows_v1().unwrap().iter().filter(|(_, row)| row.consent.terms.claim == claim).count(),
        native.inventories_v2().len()
    );
    let escrow = state
        .funded_escrows_v1()
        .values()
        .find(|row| row.funding_outpoint == TransactionOutpoint::new(carrier.id(), 1))
        .expect("actual own output is the locked escrow");
    assert_eq!(escrow.amount_sompi, native.required_original_funding_v2());
    assert_eq!(
        net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 1)).unwrap().amount,
        escrow.amount_sompi
    );
    net.funding[card] = (TransactionOutpoint::new(carrier.id(), 0), UtxoEntry::new(change, spk, 0, false));
    let origin = kaspa_consensus_core::palw_state_v2::PalwBlockContextV2 {
        block: admitted.accepted_block,
        daa_score: admitted.accepted_daa,
        blue_score: admitted.accepted_blue_score,
        subsidy: 0,
    };
    let locked = TransactionOutpoint::new(carrier.id(), 1);
    let locked_entry = net.chain.ctx.consensus.get_virtual_utxo_entry(locked).unwrap();
    let mut unauthorized = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(locked, Vec::new(), 0, 1)],
        vec![TransactionOutput::new(locked_entry.amount.checked_sub(FEE).unwrap(), locked_entry.script_public_key.clone())],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
        &mut unauthorized,
        locked_entry.clone(),
        card,
        net.config.params.storage_mass_parameter,
    );
    assert!(net.mempool(&unauthorized).is_err(), "a genuinely signed payer spend cannot bypass accepted original escrow");
    let rejected = net.chain.heartbeat(net.ttpb(), vec![unauthorized.clone()]).await;
    assert!(!rejected.transactions.iter().any(|tx| tx.id() == unauthorized.id()), "real template excludes the unauthorized spend");
    assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(locked).unwrap().amount, locked_entry.amount);
    AdmittedFamilyOriginalV2 { claim, carrier, folding, origin, inventories }
}

/// Actual A/C admission phase of the joint campaign. Explicit paths are
/// mandatory: no skipped branch, seeded admission state or BASE replacement.
/// Selected approval, material pursuit and conviction are subsequent phases;
/// this return value is not itself computation-credit authority.
pub(super) async fn admit_source_and_prefix_consumer_v2(
    net: &mut Net,
    source_dir: &Path,
    consumer_dir: &Path,
) -> (FamilyCaptureV2, AdmittedFamilyOriginalV2, FamilyCaptureV2, AdmittedFamilyOriginalV2) {
    use kaspa_consensus_core::palw_claim_local_v1::prefix_v1::*;
    let source = FamilyCaptureV2::read(source_dir, net);
    let consumer = FamilyCaptureV2::read(consumer_dir, net);
    assert_eq!((source.card, consumer.card), (1, 2), "false A, honest C and independent B0 use distinct actual funded actors");
    assert_eq!(source.semantics, consumer.semantics, "the authenticated prefix belongs to the same immutable class");
    let source_fp = PalwTirFundedOriginalEnvelopeV1::decode_v1(&source.original.funded_original)
        .unwrap()
        .original_v1()
        .unwrap()
        .original_fp_v1()
        .unwrap();
    let consumer_fp = PalwTirFundedOriginalEnvelopeV1::decode_v1(&consumer.original.funded_original)
        .unwrap()
        .original_v1()
        .unwrap()
        .original_fp_v1()
        .unwrap();
    let prefix = consumer_fp.consumed_prefix_state_v1();
    assert!(!prefix.is_genesis());
    assert!(source_fp.consumed_prefix_state_v1().is_genesis());
    register_family_capture_class_v2(net, &source).await;
    let retention = source_fp.commitment.trace_retention_daa;
    assert!(
        retention > net.daa() + 2 * kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
        "exports sign sufficient finite original custody before admission"
    );
    let export = PalwPrefixExportIntentV1 {
        version: 1,
        network: net.domain,
        source_namespace: 1,
        source_nonce: Hash64::from_u64_word(0xfa4201),
        base_commitment: source_fp.claim_id(),
        producer_key_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&card_pubkey(source.card)),
        ordinal: 0,
        prefix,
        liability_until: retention,
        material_until: retention,
    };
    let signed = PalwSignedPrefixExportV1 {
        signature: sign(source.card, export.id_v1().as_byte_slice(), PREFIX_EXPORT_CONTEXT_V1),
        intent: export,
    };
    let a = admit_family_original_v2(
        net,
        &source,
        Some(PalwPrefixDependencyEnvelopeV1 { inner: vec![1], exports: vec![signed], consumed: None }),
    )
    .await;
    let state = net.chain.tip_state().1;
    let context = state.claim_local_admission_context_v1(&net.bundle.state, &a.origin, a.claim).unwrap().unwrap();
    let (actual_export, identity) = context.prefix_export_v1(0).unwrap().unwrap();
    assert_eq!(identity.accepted_block, a.origin.block);
    assert_eq!(identity.accepted_daa, a.origin.daa_score);
    assert_eq!(identity.accepted_blue_score, a.origin.blue_score);
    // The use signs the actual authenticated birth returned by executed state,
    // never a predicted containing block or a producer-supplied origin.
    let usage = PalwPrefixUseIntentV1 {
        version: 1,
        network: net.domain,
        consumer_nonce: Hash64::from_u64_word(0xfa4202),
        base_commitment: consumer_fp.claim_id(),
        consumer_key_id: kaspa_consensus_core::mldsa87_primitives::mldsa87_key_id(&card_pubkey(consumer.card)),
        source: identity,
        prefix,
        hard_deadline: net.daa() + 2 + kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
        material_until: consumer_fp.commitment.trace_retention_daa,
    };
    usage
        .check_source_v1(
            &actual_export.intent,
            &identity,
            net.domain,
            consumer_fp.claim_id(),
            net.daa() + 2,
            state.last_point().unwrap().blue_score + 2,
        )
        .unwrap();
    let consumed =
        PalwSignedPrefixUseV1 { signature: sign(consumer.card, usage.id_v1().as_byte_slice(), PREFIX_USE_CONTEXT_V1), intent: usage };
    let c = admit_family_original_v2(
        net,
        &consumer,
        Some(PalwPrefixDependencyEnvelopeV1 { inner: vec![1], exports: vec![], consumed: Some(consumed) }),
    )
    .await;
    let state = net.chain.tip_state().1;
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    assert_eq!(
        state.state_root(),
        restored.state_root(),
        "actual signed A/C accepted carriers restore through full native validation"
    );
    assert_ne!(state.claim(&a.claim).unwrap().bond, state.claim(&c.claim).unwrap().bond);
    (source, a, consumer, c)
}

/// The producer serves literal openings under the previously signed inventory.
/// These captures supply transport data only. An outside filer must obtain
/// authenticated rooted material and independently execute the program.
pub(super) async fn supply_family_materials_v2(net: &mut Net, capture: &FamilyCaptureV2, admitted: &AdmittedFamilyOriginalV2) {
    use kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1 as T;
    use kaspa_consensus_core::palw_court_shared_material_intake_v1::MaterialTargetV1 as M;
    use kaspa_consensus_core::palw_step_leg::*;
    use kaspa_consensus_core::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    let state = net.chain.tip_state().1;
    let context = kaspa_consensus_core::palw_court_shared_tir_state_v1::derive_shared_tir_context_v1(&state, admitted.claim).unwrap();
    let source = context.source(&state).unwrap();
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&state, admitted.claim).unwrap();
    let queries =
        kaspa_consensus_core::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, context.profile_id).unwrap();
    let ctx = capture.binding.job_context.context_hash();
    let row_hashes: Vec<_> = capture
        .rows
        .iter()
        .enumerate()
        .map(|(row, lanes)| kaspa_consensus_core::palw_step_refute::tiled_logits_row_root_v1(&ctx, row as u32, lanes).unwrap())
        .collect();
    let rows_root = step_merkle_root_v1(&row_hashes).unwrap();
    let response = |query: Q| -> R {
        match query {
            Q::StepNode { level, index } => {
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&capture.hashes, level, index).unwrap();
                R::StepNode { frontier, siblings }
            }
            Q::StepLeaf { index } => R::StepLeaf {
                preimage: capture.preimages[index as usize].clone(),
                opening: step_opening_v1(&capture.hashes, index).unwrap(),
            },
            Q::StepRun { first, count } => R::StepRun {
                preimages: capture.preimages[first as usize..first as usize + count as usize].to_vec(),
                range: PalwStepRangeOpeningV1 {
                    first_leaf_index: first,
                    leaf_hashes: capture.hashes[first as usize..first as usize + count as usize].to_vec(),
                    siblings: step_merkle_range_siblings_v1(&capture.hashes, first, u64::from(count)).unwrap(),
                },
            },
            Q::RowsNode { level, index } => {
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&row_hashes, level, index).unwrap();
                R::RowsNode { frontier, siblings }
            }
            Q::TileNode { row, level, index } => {
                let hashes: Vec<_> = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .enumerate()
                    .map(|(tile, lanes)| {
                        kaspa_consensus_core::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx, row, tile as u32, lanes)
                    })
                    .collect();
                let (frontier, siblings) =
                    kaspa_consensus_core::palw_tir_court_v1::palw_tir_step_node_parts_v1(&hashes, level, index).unwrap();
                R::TileNode {
                    row_root: row_hashes[row as usize],
                    row_opening: step_opening_v1(&row_hashes, u64::from(row)).unwrap(),
                    frontier,
                    siblings,
                }
            }
            Q::TraceTile { row, tile } => {
                let lanes = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .nth(tile as usize)
                    .unwrap()
                    .to_vec();
                let hashes: Vec<_> = capture.rows[row as usize]
                    .chunks(kaspa_consensus_core::palw_step_refute::PALW_LOGITS_TILE_LANES)
                    .enumerate()
                    .map(|(tile, lanes)| {
                        kaspa_consensus_core::palw_step_refute::tiled_logits_tile_leaf_v1(&ctx, row, tile as u32, lanes)
                    })
                    .collect();
                R::TraceTile {
                    row_root: row_hashes[row as usize],
                    row_opening: step_opening_v1(&row_hashes, u64::from(row)).unwrap(),
                    lanes,
                    opening: step_opening_v1(&hashes, u64::from(tile)).unwrap(),
                }
            }
            Q::Ids { offset } => R::Ids {
                rows_root,
                ids: capture.generated[offset as usize
                    ..capture.generated.len().min(
                        offset as usize + kaspa_consensus_core::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize,
                    )]
                    .to_vec(),
            },
        }
    };
    let mut lanes = Vec::new();
    for (contract, targets) in &admitted.inventories {
        let mut targets = targets.clone();
        targets.sort_by_key(|(target, _)| !matches!(target, T::Input(M::FullIds)));
        for (target, bound) in targets {
            if target == T::SemanticTable {
                continue;
            } // supplied by actual original carrier
            let body = match target {
                T::TokenizerRaw => borsh::to_vec(&capture.raw).unwrap(),
                T::Input(M::Parameter { leaf }) => {
                    borsh::to_vec(&kaspa_consensus_core::palw_artifact::open_artifact_leaf_v1(&capture.operands, leaf).unwrap())
                        .unwrap()
                }
                T::Input(M::PromptFlat) => borsh::to_vec(&capture.prompt).unwrap(),
                T::Input(M::Unit { ordinal }) => borsh::to_vec(&response(queries.request_at(ordinal).unwrap())).unwrap(),
                T::Input(M::FullIds) => {
                    let mut body = Vec::new();
                    for offset in (0..capture.generated.len() as u32)
                        .step_by(kaspa_consensus_core::palw_tir_public_source_v1::PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize)
                    {
                        body.extend_from_slice(&borsh::to_vec(&response(Q::Ids { offset })).unwrap());
                    }
                    body
                }
                unsupported => panic!("genuine family fixture has unsupported target {unsupported:?}"),
            };
            let controls = misaka_palw_sdk::court_family_supplier_v1::family_material_upload_v1(
                net.domain,
                *contract,
                target,
                net.bond(capture.card),
                bound,
                &body,
                |who, message, context| {
                    assert_eq!(*who, net.bond(capture.card));
                    Some(sign(capture.card, message, context))
                },
            )
            .unwrap();
            let early =
                matches!(target, T::TokenizerRaw | T::Input(M::Parameter { .. }) | T::Input(M::PromptFlat) | T::Input(M::FullIds));
            lanes.push((
                early,
                controls.into_iter().map(misaka_palw_sdk::court_family_supplier_v1::family_material_object_v1).collect(),
            ));
        }
    }
    send_parallel_family_controls_v2(net, capture.card, admitted.claim, lanes).await;
    let state = net.chain.tip_state().1;
    for (contract, targets) in &admitted.inventories {
        assert_eq!(
            state.family_supplier_rows_v1().unwrap().iter().find(|(id, _)| id == contract).unwrap().1.completed.len(),
            targets.len()
        );
    }
    let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
    assert_eq!(
        state.state_root(),
        restored.state_root(),
        "actual node multipart material and financial sidecars survive native cold restore"
    );
}

/// Actual claim-level selected approval after the honest consumer's independent
/// replay. This drives the same public eligibility-attestation ingress as the
/// native39 campaign, with the legacy FP certification identity and real draw.
/// No Final, approval bit, draw seed or certification row is inserted by tests.
pub(super) async fn certify_honest_family_claim_v2(net: &mut Net, admitted: &AdmittedFamilyOriginalV2) {
    use kaspa_consensus_core::palw_security_certification_v1::*;
    use misaka_palw_kernel::model_eligibility_v1::*;
    let id = palw_certification_id_v1(&admitted.claim);
    let key = certification_claim_key_v1(&id);
    let read = |net: &Net| {
        net.chain
            .tip_state()
            .1
            .kernel_route_state_v1()
            .and_then(|r| r.aux_row::<PalwClaimCertificationRowV1>(PALW_SECURITY_CERTIFICATION_TABLE_V1, &key))
    };
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&net.chain.tip_state().1, admitted.claim)
        .expect("consumer original is actually retained")
        .plan_v1()
        .clone();
    // The native original has a finite collection door. A caller may not turn
    // missing own Final into a supplied optimistic certification.
    net.beat_to(frame.terms.collect_until_daa + 1).await;
    assert!(
        matches!(
            net.chain.tip_state().1.claim(&admitted.claim).unwrap().phase,
            kaspa_consensus_core::palw_state_v2::PalwClaimPhaseV2::Final { .. }
        ),
        "honest consumer reaches its actual own Final"
    );
    let policy = net.bundle.state.rfc0018_v1().unwrap().certification;
    for round in 0..64 {
        if read(net).is_some_and(|r| r.anchor.is_some()) {
            break;
        }
        net.chain.attempt(5 + (round % 3), net.ttpb(), Vec::new(), &|_| true).await;
        net.beat_to(net.daa() + 8).await;
    }
    let drawn = read(net).expect("real FP Final commits its certification");
    assert!(
        drawn.anchor.is_some() && drawn.draws.len() == policy.m as usize,
        "real later entropy and matured pool draw every selected position"
    );
    for card in 0..8 {
        if !drawn.draws.contains(&net.kid(card)) {
            continue;
        }
        let attestation = EligibilityAttestationV1 {
            version: ELIGIBILITY_ATTESTATION_VERSION_V1,
            network: net.domain.as_bytes(),
            exam: id,
            verifier: net.kid(card),
            verdict: EligibilityVerdictV1::Reproduced,
            transcript: frame.resource_plan_id.as_bytes(),
            claim_mismatches: vec![],
        };
        let object = net.route(card, &K::AttestEligibilityExamV1 { attestation });
        net.send(vec![(card, object)]).await;
    }
    assert!(
        read(net).unwrap().positive_positions() >= u32::from(policy.k),
        "selected verifier signatures reach the actual quorum under the policy's honest-fraction assumption"
    );
    net.beat_to(drawn.final_daa + policy.prosecution_window_daa + 2).await;
    assert!(
        palw_claim_security_certified_v1(&net.chain.tip_state().1, &admitted.claim),
        "claim-level selected approval is actually finalized"
    );
}

/// Real confirmed UTXO lanes remove the harness's one-float-per-card
/// serialization. All target dependencies remain on their own lane, and IDs
/// finish before row/tile bodies that authenticate against complete IDs.
async fn send_parallel_family_controls_v2(net: &mut Net, card: usize, claim: Hash64, objects: Vec<(bool, Vec<Obj>)>) {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use std::collections::VecDeque;
    if objects.is_empty() {
        return;
    }
    let starting = net.daa();
    let (input, entry) = net.funding[card].clone();
    let script = entry.script_public_key.clone();
    let amounts: Vec<_> = objects.iter().map(|(_, items)| FEE.checked_mul(items.len() as u64 + 1).unwrap()).collect();
    let total = amounts.iter().try_fold(FEE, |sum, n| sum.checked_add(*n)).unwrap();
    let change = entry.amount.checked_sub(total).expect("parallel material controls spend only finite actual producer UTXOs");
    let mut outputs = vec![TransactionOutput::new(change, script.clone())];
    outputs.extend(amounts.iter().map(|n| TransactionOutput::new(*n, script.clone())));
    let mut splitter = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(input, vec![], 0, 1)],
        outputs,
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        vec![],
    );
    crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
        &mut splitter,
        entry,
        card,
        net.config.params.storage_mass_parameter,
    );
    net.mempool(&splitter).expect("actual producer signs and funds independent material control lanes");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![splitter.clone()]).await;
    assert!(carrying.transactions.iter().any(|t| t.id() == splitter.id()));
    net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let main = TransactionOutpoint::new(splitter.id(), 0);
    net.funding[card] = (main, net.chain.ctx.consensus.get_virtual_utxo_entry(main).unwrap());
    let mut lanes: Vec<_> = objects
        .into_iter()
        .enumerate()
        .map(|(index, (early, items))| {
            let outpoint = TransactionOutpoint::new(splitter.id(), index as u32 + 1);
            (early, outpoint, net.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).unwrap(), VecDeque::from(items))
        })
        .collect();
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&net.chain.tip_state().1, claim)
        .unwrap()
        .plan_v1()
        .clone();
    let mut inputs_checked = false;
    let mut batches = 0;
    while lanes.iter().any(|(_, _, _, items)| !items.is_empty()) {
        let early = lanes.iter().any(|(early, _, _, items)| *early && !items.is_empty());
        if !early && !inputs_checked {
            assert!(
                net.daa() + 2 < frame.preparation_until_daa,
                "actual immutable input/ID supply leaves a full confirmed producer Ready before native preparation door: now {} prep {}",
                net.daa(),
                frame.preparation_until_daa
            );
            eprintln!(
                "[family-native-packing-inputs] claim={claim} birth={} inputsReady={} preparation={} open={} collect={}",
                frame.terms.accepted_daa,
                net.daa(),
                frame.preparation_until_daa,
                frame.terms.open_until_daa,
                frame.terms.collect_until_daa
            );
            inputs_checked = true;
        }
        let mut offered = Vec::new();
        let mut indexes = Vec::new();
        for (index, (is_early, input, entry, items)) in lanes.iter().enumerate() {
            if items.is_empty() || (*is_early != early) {
                continue;
            }
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 {
                version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: items.front().unwrap().clone(),
            })
            .unwrap();
            let mut tx = Transaction::new(
                crate::constants::TX_VERSION,
                vec![TransactionInput::new(*input, vec![], 0, 1)],
                vec![TransactionOutput::new(entry.amount.checked_sub(FEE).unwrap(), script.clone())],
                0,
                kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE,
                0,
                payload,
            );
            crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
                &mut tx,
                entry.clone(),
                card,
                net.config.params.storage_mass_parameter,
            );
            net.mempool(&tx).expect("parallel material lane is signed and spends an actually confirmed output");
            offered.push(tx);
            indexes.push(index);
        }
        let carrying = net.chain.heartbeat(net.ttpb(), offered.clone()).await;
        let mut included = 0;
        for (tx, index) in offered.iter().zip(indexes) {
            if !carrying.transactions.iter().any(|t| t.id() == tx.id()) {
                continue;
            }
            let lane = &mut lanes[index];
            lane.1 = TransactionOutpoint::new(tx.id(), 0);
            lane.2 = net.chain.ctx.consensus.get_virtual_utxo_entry(lane.1).unwrap();
            lane.3.pop_front();
            included += 1;
        }
        assert!(included > 0, "actual template must include at least one in-format material control; no silent deadline wait");
        // The next action observes the real folded Begin/Part, rather than
        // borrowing an uncommitted draft from a producer's local queue.
        net.chain.heartbeat(net.ttpb(), Vec::new()).await;
        batches += 1;
        eprintln!(
            "[family-native-packing] card={card} phase={} batch={batches} included={included} elapsedDAA={}",
            if early { "inputs+ids" } else { "source-units" },
            net.daa() - starting
        );
    }
    if !inputs_checked {
        assert!(
            net.daa() + 2 < frame.preparation_until_daa,
            "actual ordinary input supply leaves a confirmed producer Ready before native preparation door: now {} prep {}",
            net.daa(),
            frame.preparation_until_daa
        );
        eprintln!(
            "[family-native-packing-inputs] claim={claim} birth={} inputsReady={} preparation={} open={} collect={}",
            frame.terms.accepted_daa,
            net.daa(),
            frame.preparation_until_daa,
            frame.terms.open_until_daa,
            frame.terms.collect_until_daa
        );
    }
    assert!(
        net.daa() <= frame.terms.accepted_daa + kaspa_consensus_core::palw_court_policy_v1::DISPUTE_WINDOW_DAA,
        "actual full native inventory supply completes within signed original hard horizon"
    );
    // The one-FEE lane changes remain actual unspent outputs in the node.
    // They are excluded from the main float, never replaced by a nominal sum.
    for (_, outpoint, entry, items) in lanes {
        assert!(items.is_empty());
        assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).unwrap().amount, entry.amount);
    }
}

/// First actual-node timing diagnostic. The producer capture is public input,
/// not a supplied prestate. A later self-contained producer constructor replaces
/// this manual diagnostic input; this is not labelled the joint A/C/B campaign.
#[tokio::test]
#[ignore = "explicit public producer capture generated for this exact funded genesis is required"]
async fn family_native39_real_node_original_material_packing_from_producer_capture() {
    let dir = std::env::var("MISAKA_FAMILY_NODE_CAPTURE_DIR").expect("explicit actualgenesis producer capture directory");
    let parts = crate::pipeline::virtual_processor::tests::family_supplier_node_candidate::family_native39_campaign_config_v2();
    let mut net = Net::over_cfg(parts, TestConsensus::new);
    let capture = FamilyCaptureV2::read(Path::new(&dir), &net);
    register_family_capture_class_v2(&mut net, &capture).await;
    let admitted = admit_family_original_v2(&mut net, &capture, None).await;
    supply_family_materials_v2(&mut net, &capture, &admitted).await;
}
