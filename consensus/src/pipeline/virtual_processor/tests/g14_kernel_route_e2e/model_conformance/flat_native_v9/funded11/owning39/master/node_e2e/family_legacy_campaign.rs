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

mod adjudication;
mod refund;
pub(super) use adjudication::{adjudicate_forged_family_claim_v2, complete_honest_family_court_v2};
pub(super) use refund::{refund_family_escrow_reorg_v3, refund_family_escrow_v2};

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
    /// Stored carrying provenance is not an authenticated full native point.
    pub stored_origin: (Hash64, u64, u64),
    /// Present only when actual source admission authenticates the full point.
    pub actual_origin: Option<kaspa_consensus_core::palw_state_v2::PalwBlockContextV2>,
    pub inventories: Vec<(Hash64, Vec<(kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1, u64)>)>,
    /// Confirmed ordinary outputs of the original carrier, separate from escrow.
    pub material_funding:
        Vec<(Hash64, kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1, TransactionOutpoint)>,
    pub target_ids: Vec<(Hash64, kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1, Hash64)>,
}

/// Compare against the independently retained accepted outer transaction, not
/// an inner TFO2/FPA1 id reconstructed from the winning row itself.
pub(super) fn assert_family_winning_carrier_v2(state: &PalwChainStateV2, admitted: &AdmittedFamilyOriginalV2) {
    let expected = borsh::to_vec(&admitted.carrier.id()).unwrap();
    let route = state.kernel_route_state_v1().expect("accepted original has its native route");
    assert_eq!(
        route.aux.get(&(
            kaspa_consensus_core::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1,
            admitted.claim.as_byte_slice().to_vec()
        )),
        Some(&expected),
        "winning carrier is exactly the accepted signed outer transaction, including FPR2/FPA1 wrappers"
    );
}

/// Reprice at the node's actual next admission point, then sign each independent
/// duty. The carrier passes the real mempool, native UTXO signatures, template
/// construction, selected-chain insertion and the following block's fold.
pub(super) async fn admit_family_original_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    provenance: Option<kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>,
) -> AdmittedFamilyOriginalV2 {
    admit_family_original_archive_mode_v1(net, capture, provenance, None).await
}
pub(super) async fn admit_family_original_archive_mode_v1(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    provenance: Option<kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>,
    archive_budget: Option<kaspa_consensus_core::palw_state_v2::PalwOriginalArchivePayerBudgetV1>,
) -> AdmittedFamilyOriginalV2 {
    admit_family_original_archive_case_v1(net, capture, provenance, archive_budget, ArchiveCarrierCaseV1::Positive)
        .await
        .expect("positive actual archive carrier")
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ArchiveCarrierCaseV1 {
    Positive,
    RentShort,
    CapitalShort,
}
pub(super) async fn admit_family_original_archive_case_v1(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    mut provenance: Option<kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>,
    archive_budget: Option<kaspa_consensus_core::palw_state_v2::PalwOriginalArchivePayerBudgetV1>,
    case: ArchiveCarrierCaseV1,
) -> Option<AdmittedFamilyOriginalV2> {
    admit_family_original_archive_case_kind_v2(net, capture, provenance, archive_budget, case, false).await
}
pub(super) async fn admit_family_original_archive_case_kind_v2(
    net: &mut Net,
    capture: &FamilyCaptureV2,
    mut provenance: Option<kaspa_consensus_core::palw_claim_local_v1::prefix_v1::PalwPrefixDependencyEnvelopeV1>,
    archive_budget: Option<kaspa_consensus_core::palw_state_v2::PalwOriginalArchivePayerBudgetV1>,
    case: ArchiveCarrierCaseV1,
    mixed: bool,
) -> Option<AdmittedFamilyOriginalV2> {
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
        // Quote context uses the real accepted parent; the future birth is
        // recovered only from executed Source8 after the carrier folds.
        block: if archive_budget.is_some() { net.chain.sink() } else { Hash64::from_u64_word(0xfa42) },
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
    let (quoted_input, _) = net.funding[card].clone();
    let candidate_inputs = vec![TransactionInput::new(quoted_input, Vec::new(), 0, 1)];
    let native = net
        .chain
        .vp()
        .palw_family_original_native_quote_v3_impl(point, object, envelope.clone(), table, &candidate_inputs, provenance.as_ref())
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
        let terms = kaspa_consensus_core::palw_state_v2::palw_family_supplier_terms_quote_v3(inventory, terms).unwrap();
        suppliers.push(
            sign_family_supplier_v1(terms, |who, message, context| {
                assert_eq!(*who, producer);
                Some(signer.sign_with_context(message, context).unwrap())
            })
            .unwrap(),
        );
    }
    let target_ids = native
        .inventories_v2()
        .iter()
        .zip(&suppliers)
        .flat_map(|(inventory, consent)| {
            let contract = consent.id_v1().unwrap();
            inventory.targets_v1().iter().map(move |(target, _)| (contract, *target, inventory.target_id_v1(*target).unwrap()))
        })
        .collect();
    let inventories: Vec<_> = native
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
    let funded = funded.with_signature_aliases_v3().unwrap();
    let mut funding_amount = native.required_original_funding_v2();
    let funded = if let Some(budget) = archive_budget {
        let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes());
        let mut draft = Transaction::new(
            crate::constants::TX_VERSION,
            candidate_inputs.clone(),
            vec![TransactionOutput::new(0, spk.clone()), TransactionOutput::new(funding_amount, spk)],
            0,
            kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
            0,
            funded.encode_v2().unwrap(),
        );
        draft.finalize();
        let quote = if mixed {
            net.chain.vp().palw_original_archive_native_quote_v2_impl(
                point,
                &capture.object,
                &draft,
                &draft.payload,
                provenance.as_ref(),
                budget,
            )
        } else {
            net.chain.vp().palw_original_archive_native_quote_v1_impl(
                point,
                &capture.object,
                &draft,
                &draft.payload,
                provenance.as_ref(),
                budget,
            )
        }
        .expect("normal public archive quote from actual accepted node state");
        let expected = if mixed {
            kaspa_consensus_core::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MixedV2
        } else {
            kaspa_consensus_core::palw_court_original_escrow_v1::OriginalArchiveProfileKindV2::MaterialV1
        };
        let mut terms = PalwTirFundedOriginalEnvelopeV1::decode_v1(&funded.funded_original).unwrap().funding.terms;
        quote.bind_existing_funding_terms_v1(&mut terms).unwrap();
        assert_eq!(kaspa_consensus_core::palw_court_original_escrow_v1::original_archive_profile_kind_v2(&terms), Some(expected));
        eprintln!(
            "[archive-node-quote] payer rent ceiling={} capital ceiling={} native input capacity={} final H={} full rent={} funding={}",
            budget.additional_carrier_rent_sompi,
            budget.dedicated_archive_capital_sompi,
            quote.input_archive_capacity_bytes_v1(),
            quote.material_until_daa_v1(),
            quote.declared_rent_sompi_v1(),
            quote.funding_output_amount_sompi_v1()
        );
        funding_amount = quote.funding_output_amount_sompi_v1();
        envelope.declared_rent_sompi = quote.declared_rent_sompi_v1();
        let signature = signer.sign_with_context(terms.id_v1().as_byte_slice(), COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1).unwrap();
        let tfo = PalwTirFundedOriginalEnvelopeV1::new_v1(
            envelope.encode_v1().unwrap(),
            FundingConsentV1 { terms, signature },
            signer.public_key(),
        )
        .unwrap();
        PalwTirFundedOriginalEnvelopeV2 { funded_original: tfo.encode_v1().unwrap(), ..funded }.with_signature_aliases_v3().unwrap()
    } else {
        funded
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
    // Material control capital exists in the same confirmed carrier as the
    // original. A later splitter would consume the first promised opportunity.
    let mut material_outputs = Vec::new();
    for (contract, targets) in &inventories {
        for (target, bound) in targets {
            if *target == kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialTargetV1::SemanticTable {
                continue;
            }
            let controls = bound
                .div_ceil(kaspa_consensus_core::palw_court_family_material_v1::FAMILY_MATERIAL_PART_BYTES_V1 as u64)
                .checked_add(2)
                .unwrap();
            material_outputs.push((*contract, *target, FEE.checked_mul(controls.checked_add(1).unwrap()).unwrap()));
        }
    }
    let lane_total = material_outputs.iter().try_fold(0u64, |sum, (_, _, value)| sum.checked_add(*value)).unwrap();
    let change = entry
        .amount
        .checked_sub(lane_total)
        .and_then(|n| n.checked_sub(funding_amount))
        .and_then(|n| n.checked_sub(fee))
        .expect("actual finite genesis float must fund original escrow and native carrier rent");
    let spk = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&refund.as_bytes());
    let mut outputs = vec![TransactionOutput::new(change, spk.clone()), TransactionOutput::new(funding_amount, spk.clone())];
    outputs.extend(material_outputs.iter().map(|(_, _, amount)| TransactionOutput::new(*amount, spk.clone())));
    let mut carrier = Transaction::new(
        crate::constants::TX_VERSION,
        {
            assert_eq!(input, candidate_inputs[0].previous_outpoint);
            candidate_inputs
        },
        outputs,
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_FP_COMMITMENT,
        0,
        payload,
    );
    crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
        &mut carrier,
        entry.clone(),
        card,
        net.config.params.storage_mass_parameter,
    );
    if case != ArchiveCarrierCaseV1::Positive {
        assert!(archive_budget.is_some());
        if case == ArchiveCarrierCaseV1::RentShort {
            // Same full signed literal/profile; real outer fee is rent-1.
            carrier.outputs[0].value = carrier.outputs[0].value.checked_add(FEE).and_then(|v| v.checked_add(1)).unwrap();
        } else {
            let mut short_funded = funded.clone();
            let mut short_base = PalwTirFundedOriginalEnvelopeV1::decode_v1(&short_funded.funded_original).unwrap();
            short_base.funding.terms.funding_amount_sompi -= 1;
            short_base.funding.signature = signer
                .sign_with_context(short_base.funding.terms.id_v1().as_byte_slice(), COURT_ORIGINAL_FUNDING_SIGNATURE_CONTEXT_V1)
                .unwrap();
            short_funded.funded_original = short_base.encode_v1().unwrap();
            let mut payload = kaspa_consensus_core::palw_fp_answer_accounting_v1::palw_fp_answer_envelope_retain_encode_v1(
                &short_funded.with_signature_aliases_v3().unwrap().encode_v2().unwrap(),
                &capture.generated,
            )
            .unwrap()
            .to_vec();
            if let Some(p) = &provenance {
                let mut p = p.clone();
                p.inner = payload;
                payload = p.encode_v1().unwrap();
            }
            carrier.payload = payload;
            carrier.outputs[1].value -= 1;
            carrier.outputs[0].value += 1;
        }
        carrier.finalize();
        crate::pipeline::virtual_processor::tests::t12_round_lane_e2e::sign_spend(
            &mut carrier,
            entry.clone(),
            card,
            net.config.params.storage_mass_parameter,
        );
        assert!(carrier.id_is_current());
        // This is a PALW allocation refusal, not a UTXO/block rollback. Submit
        // a real block even if local mempool policy rejects the allocation.
        let carrying = net.chain.heartbeat(net.ttpb(), vec![carrier.clone()]).await;
        assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()));
        net.chain.heartbeat(net.ttpb(), Vec::new()).await;
        let state = net.chain.tip_state().1;
        assert!(state.claim(&claim).is_none(), "underpaid native allocation must be dropped");
        assert!(state.funded_escrows_v1().values().all(|row| row.claim != claim), "no Source17-funded allocation/escrow may survive");
        assert!(state.family_supplier_rows_v1().unwrap().iter().all(|(_, row)| row.consent.terms.claim != claim));
        assert_eq!(
            net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 0)).unwrap().amount,
            carrier.outputs[0].value,
            "ordinary signed carrier UTXO change survives"
        );
        assert_eq!(
            net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 1)).unwrap().amount,
            carrier.outputs[1].value,
            "unadmitted own output remains ordinary UTXO"
        );
        assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(input).is_none(), "actual input was spent despite allocation refusal");
        let cold = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
        assert_eq!(cold.state_root(), state.state_root(), "ordinary negative state has no dangling financial/source indices");
        return None;
    }
    net.mempool(&carrier).expect("actual signed native inputs and TFO2 pass this node's mempool");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![carrier.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == carrier.id()));
    let folding = net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let state = net.chain.tip_state().1;
    let admitted = state.claim(&claim).expect("actual accepted carrier must install its original, not an ordinary fallback");
    assert_eq!(admitted.accepted_daa, point.daa_score, "singleton admission is never silently shifted");
    let winning = borsh::to_vec(&carrier.id()).unwrap();
    assert_eq!(
        state.kernel_route_state_v1().unwrap().aux.get(&(
            kaspa_consensus_core::palw_fp_answer_accounting_v1::PALW_FP_WINNING_CARRIER_TABLE_V1,
            claim.as_byte_slice().to_vec(),
        )),
        Some(&winning),
        "actual native acceptance records the literal outer carrier id"
    );
    assert_eq!(
        state.family_supplier_rows_v1().unwrap().iter().filter(|(_, row)| row.consent.terms.claim == claim).count(),
        native.inventories_v2().len()
    );
    let escrow = state
        .funded_escrows_v1()
        .values()
        .find(|row| row.funding_outpoint == TransactionOutpoint::new(carrier.id(), 1))
        .expect("actual own output is the locked escrow");
    assert_eq!(escrow.amount_sompi, funding_amount);
    assert_eq!(
        net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(carrier.id(), 1)).unwrap().amount,
        escrow.amount_sompi
    );
    let change_outpoint = TransactionOutpoint::new(carrier.id(), 0);
    net.funding[card] = (change_outpoint, net.chain.ctx.consensus.get_virtual_utxo_entry(change_outpoint).unwrap());
    let material_funding = material_outputs
        .iter()
        .enumerate()
        .map(|(index, (contract, target, amount))| {
            let outpoint = TransactionOutpoint::new(carrier.id(), u32::try_from(index + 2).unwrap());
            assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).unwrap().amount, *amount);
            (*contract, *target, outpoint)
        })
        .collect();
    let stored_origin = (admitted.accepted_block, admitted.accepted_daa, admitted.accepted_blue_score);
    let actual_origin = if net.bundle.state.claim_local_settlement_active_at(state.last_point().unwrap().daa_score) {
        Some(
            *kaspa_consensus_core::palw_state_v2::palw_actual_fp_accepting_point_from_live_stored_v1(&state, claim, stored_origin)
                .expect("actual source admission must authenticate the complete birth point"),
        )
    } else {
        None
    };
    if archive_budget.is_some() {
        let actual = actual_origin.expect("new profile requires actual Source8 birth, never prospective point block label");
        let status = kaspa_consensus_core::palw_state_v2::palw_original_archive_status_v1(&state, claim, actual)
            .unwrap()
            .expect("normal constructed Source17 receipt");
        assert!(status.constructed_v1() && status.material_ready_v1());
        assert_eq!(status.contract_v1(), escrow.contract_id);
        assert_eq!(status.source_v1(), envelope.pledge.source_id);
        assert!(
            !state.kernel_route_state_v1().unwrap().aux.contains_key(&(
                kaspa_consensus_core::palw_claim_local_v1::PALW_CLAIM_LOCAL_TABLE_V1,
                kaspa_consensus_core::palw_claim_local_v1::palw_claim_liability_frame_key_v1(actual.block).to_vec()
            )),
            "pure original source uses scoped archive rather than birth F1"
        );
        let restored = PalwStateCarriageV2::from_state(&state).into_state(&net.bundle.state, Some(state.state_root())).unwrap();
        assert_eq!(
            kaspa_consensus_core::palw_state_v2::palw_original_archive_status_v1(&restored, claim, actual).unwrap(),
            Some(status)
        );
    }
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
    assert_eq!(net.chain.ctx.consensus.get_virtual_utxo_entry(locked).unwrap().amount, locked_entry.amount);
    Some(AdmittedFamilyOriginalV2 { claim, carrier, folding, stored_origin, actual_origin, inventories, material_funding, target_ids })
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
    // Confirm the source's promised materials before starting a later consumer's
    // clock. The caller may repeat supply: completed targets are checked/skipped.
    supply_family_materials_v2(net, &source, &a).await;
    let state = net.chain.tip_state().1;
    let origin = a.actual_origin.expect("Prefix provenance requires an authenticated actual birth");
    let context = state.claim_local_admission_context_v1(&net.bundle.state, &origin, a.claim).unwrap().unwrap();
    let (actual_export, identity) = context.prefix_export_v1(0).unwrap().unwrap();
    assert_eq!(identity.accepted_block, origin.block);
    assert_eq!(identity.accepted_daa, origin.daa_score);
    assert_eq!(identity.accepted_blue_score, origin.blue_score);
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
    assert_family_winning_carrier_v2(&state, &a);
    assert_family_winning_carrier_v2(&state, &c);
    assert_family_winning_carrier_v2(&restored, &a);
    assert_family_winning_carrier_v2(&restored, &c);
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
                    siblings: step_merkle_range_siblings_v1(
                        &capture.hashes,
                        usize::try_from(first).unwrap(),
                        usize::try_from(count).unwrap(),
                    )
                    .unwrap(),
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
    let supplier_rows = state.family_supplier_rows_v1().unwrap();
    let mut lanes = Vec::new();
    for (contract, targets) in &admitted.inventories {
        let mut targets = targets.clone();
        targets.sort_by_key(|(target, _)| !matches!(target, T::Input(M::FullIds)));
        for (target, bound) in targets {
            let row = &supplier_rows.iter().find(|(id, _)| id == contract).unwrap().1;
            let target_id =
                admitted.target_ids.iter().find(|(id, native_target, _)| id == contract && *native_target == target).unwrap().2;
            if row.completed.contains_key(&target_id) {
                continue;
            }
            assert_ne!(target, T::SemanticTable, "the actual original carrier completes its exact table target");
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
    send_parallel_family_controls_v2(net, capture.card, admitted, lanes).await;
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
    assert_family_winning_carrier_v2(&state, admitted);
    assert_family_winning_carrier_v2(&restored, admitted);
    adjudication::supply_missing_family_numerical_material_v2(net, capture, admitted).await;
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
async fn send_parallel_family_controls_v2(
    net: &mut Net,
    card: usize,
    admitted: &AdmittedFamilyOriginalV2,
    objects: Vec<(bool, Vec<Obj>)>,
) {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::{PALW_LIFECYCLE_TX_VERSION_V2, PalwLifecycleTxPayloadV2};
    use std::collections::VecDeque;
    if objects.is_empty() {
        return;
    }
    let starting = net.daa();
    let claim = admitted.claim;
    let mut lanes: Vec<_> = objects
        .into_iter()
        .map(|(early, items)| {
            let Obj::CourtFamilyMaterialV1 { control } = &items[0] else {
                panic!("material lane must contain only native family controls")
            };
            let outpoint = admitted
                .material_funding
                .iter()
                .find(|(contract, target, _)| *contract == control.contract && *target == control.target)
                .unwrap()
                .2;
            let entry =
                net.chain.ctx.consensus.get_virtual_utxo_entry(outpoint).expect("original-confirmed actual material funding output");
            assert!(entry.amount >= FEE.checked_mul(items.len() as u64 + 1).unwrap());
            let total = items.len();
            (early, outpoint, entry, VecDeque::from(items), total)
        })
        .collect();
    let frame = kaspa_consensus_core::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&net.chain.tip_state().1, claim)
        .unwrap()
        .plan_v1()
        .clone();
    let mut inputs_checked = false;
    let mut batches = 0;
    while lanes.iter().any(|(_, _, _, items, _)| !items.is_empty()) {
        let early = lanes.iter().any(|(early, _, _, items, _)| *early && !items.is_empty());
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
        let state = net.chain.tip_state().1;
        let service = state.court_service_state_v1().unwrap();
        let rows = state.family_supplier_rows_v1().unwrap();
        let mut next_assignment = None;
        for (index, (_is_early, input, entry, items, _total)) in lanes.iter().enumerate() {
            if items.is_empty() {
                continue;
            }
            let Obj::CourtFamilyMaterialV1 { control } = items.front().unwrap() else { unreachable!() };
            let row = &rows.iter().find(|(id, _)| *id == control.contract).unwrap().1;
            let target = admitted
                .target_ids
                .iter()
                .find(|(id, native_target, _)| *id == control.contract && *native_target == control.target)
                .unwrap()
                .2;
            use kaspa_consensus_core::palw_court_family_material_v1::FamilyMaterialActionV1 as A;
            let ordinal = match &control.action {
                A::Begin { .. } => 0,
                A::Part { offset, .. } => {
                    offset / kaspa_consensus_core::palw_court_family_material_v1::FAMILY_MATERIAL_PART_BYTES_V1 as u64 + 1
                }
                A::Finish { .. } => u64::try_from(row.targets.iter().find(|t| t.id == target).unwrap().packets.len() - 1).unwrap(),
                A::BeginReserved { .. } => panic!("this campaign serves only its primary original-paid inventory"),
            };
            let target_row = row.targets.iter().find(|t| t.id == target).unwrap();
            let packet = target_row.packets[usize::try_from(ordinal).unwrap()];
            let assigned = service.book.assigned_daa(&packet).unwrap();
            let accepting = net.daa().checked_add(2).unwrap();
            assert!(
                assigned >= accepting,
                "actual primary assignment must remain reachable after its causal predecessor: assigned={assigned} accepting={accepting}"
            );
            next_assignment = Some(next_assignment.map_or(assigned, |old: u64| old.min(assigned)));
            if assigned != accepting {
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
                vec![TransactionOutput::new(entry.amount.checked_sub(FEE).unwrap(), entry.script_public_key.clone())],
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
        if offered.is_empty() {
            let target = next_assignment.expect("remaining actual material lane has a rooted assignment").checked_sub(2).unwrap();
            assert!(target > net.daa());
            net.beat_to(target).await;
            continue;
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
    for (_, outpoint, entry, items, _) in lanes {
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
    settle_family_escrow_fee_v2(&mut net, &capture, &admitted).await;
}

/// Spend the actual admitted escrow through native script validation, mempool,
/// block packing, UTXO acceptance and the rooted settlement fold. Fee funding is
/// a separate real input; the escrow's scheduled burn is never collectible.
pub(super) async fn settle_family_escrow_fee_v2(net: &mut Net, capture: &FamilyCaptureV2, admitted: &AdmittedFamilyOriginalV2) {
    use kaspa_consensus_core::palw_funded_escrow_v1 as escrow;
    let before = net.chain.tip_state().1;
    let row = *before.funded_escrows_v1().values().find(|r| r.claim == admitted.claim).expect("accepted original escrow");
    assert!(row.liability_open && row.is_fresh_v1());
    let [work, relay, burn] = kaspa_consensus_core::palw_g14_values_v1::palw_g14_escrow_split_v1(row.fee_sompi);
    let remainder = row.amount_sompi.checked_sub(row.fee_sompi).unwrap();
    assert!(work > 0 && relay > 0 && burn > 0 && remainder > 0);
    let locked_entry = net.chain.ctx.consensus.get_virtual_utxo_entry(row.funding_outpoint).unwrap();
    assert_eq!(locked_entry.amount, row.amount_sompi);
    let (fee_outpoint, _) = net.funding[capture.card].clone();
    let fee_entry = net.chain.ctx.consensus.get_virtual_utxo_entry(fee_outpoint).unwrap();
    let fee_change = fee_entry.amount.checked_sub(FEE).expect("actual payer input funds normal carrier fee");
    let pay = |amount, payload: Hash64| {
        TransactionOutput::new(amount, kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&payload.as_bytes()))
    };
    let mut spend = Transaction::new(
        crate::constants::TX_VERSION,
        vec![TransactionInput::new(row.funding_outpoint, Vec::new(), 0, 1), TransactionInput::new(fee_outpoint, Vec::new(), 0, 1)],
        vec![
            pay(work, row.seat_payee),
            pay(relay, row.relay_payee),
            pay(remainder, row.refund_payload),
            TransactionOutput::new(fee_change, fee_entry.script_public_key.clone()),
        ],
        0,
        kaspa_consensus_core::subnets::SUBNETWORK_ID_NATIVE,
        0,
        Vec::new(),
    );
    let signers = vec![(locked_entry.clone(), capture.card), (fee_entry.clone(), capture.card)];
    sign_spend_many(&mut spend, &signers, net.config.params.storage_mass_parameter);
    spend.finalize();
    let schedule = escrow::palw_escrow_fee_schedule_v1(&row).expect("admission recorded real schedule/payees");
    let plan =
        escrow::palw_escrow_qualified_spend_v1(&row, &spend, &[locked_entry.amount, fee_entry.amount], net.daa(), Some(&schedule))
            .unwrap();
    assert_eq!((plan.paid_now, plan.burned_now, plan.continued_now), (work + relay, burn, remainder));
    let actual_fee = locked_entry.amount + fee_entry.amount - spend.outputs.iter().map(|o| o.value).sum::<u64>();
    assert_eq!(actual_fee, FEE + burn, "ordinary fee and scheduled burn draw on distinct actual inputs");
    assert_eq!(
        net.chain.vp().palw_collectible_transaction_fee_v1(&spend, actual_fee, burn, net.daa()),
        FEE,
        "the scheduled destruction cannot enter the accepting miner's collectible fee"
    );
    let mut stolen = spend.clone();
    stolen.outputs[2].value -= 1;
    stolen.outputs[3].value += 1;
    sign_spend_many(&mut stolen, &signers, net.config.params.storage_mass_parameter);
    stolen.finalize();
    let view: kaspa_consensus_core::utxo::utxo_collection::UtxoCollection =
        [(row.funding_outpoint, locked_entry), (fee_outpoint, fee_entry.clone())].into_iter().collect();
    net.chain
        .vp()
        .validate_transaction_in_utxo_context(
            &stolen,
            &view,
            net.daa(),
            crate::processes::transaction_validator::tx_validation_in_utxo_context::TxValidationFlags::Full,
            None,
        )
        .expect("the rejected redistribution has genuine ordinary input signatures and amounts");
    assert!(net.mempool(&stolen).is_err(), "even genuine payer signatures cannot move escrow capital to normal change");
    net.mempool(&spend).expect("exact scheduled spend passes native scripts and escrow lock");
    let carrying = net.chain.heartbeat(net.ttpb(), vec![spend.clone()]).await;
    assert!(carrying.transactions.iter().any(|tx| tx.id() == spend.id()));
    net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let after = net.chain.tip_state().1;
    let continued = after.funded_escrows_v1()[&row.contract_id];
    assert_eq!(continued, plan.new, "the actual accepted input and rooted monetary effects are one settlement");
    assert!(net.chain.ctx.consensus.get_virtual_utxo_entry(row.funding_outpoint).is_none());
    for (index, amount) in [(0, work), (1, relay), (2, remainder), (3, fee_change)] {
        assert_eq!(
            net.chain.ctx.consensus.get_virtual_utxo_entry(TransactionOutpoint::new(spend.id(), index)).unwrap().amount,
            amount
        );
    }
    assert_eq!(
        continued.amount_sompi,
        continued.paid_sompi + continued.burned_sompi + continued.returned_sompi + continued.unreleased_v1().unwrap()
    );
    assert!(net.mempool(&spend).is_err(), "actual escrow input cannot be accepted twice");
    net.funding[capture.card] =
        (TransactionOutpoint::new(spend.id(), 3), UtxoEntry::new(fee_change, fee_entry.script_public_key, 0, false));
}
