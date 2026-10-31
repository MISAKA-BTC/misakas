#![cfg(feature = "positive-panel")]
#[path = "../../tir/exec/tests/positive_panel_r1_common/mod.rs"]
mod common;
use common::*;
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::*,
    ports::{NodeChecker, PanelBuilder},
    tir,
    wire::EvidenceFrame,
};
use misaka_palw_remote::{
    bundle::DetachedSigner,
    positive_panel::{PanelTransportError, SignedPanelPartR1, SignedPanelWitnessR1},
};
use misaka_palw_tir_exec::node::{
    positive_check::CheckOutcome,
    positive_contract::{PreparedCheck, TirNodeChecker},
    positive_panel::TirPanelBuilder,
};

struct Key(libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair);
impl Key {
    fn new(seed: u8) -> Self {
        Self(libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]))
    }
}
impl DetachedSigner for Key {
    fn public_key(&self) -> Vec<u8> {
        self.0.verification_key.as_ref().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        libcrux_ml_dsa::ml_dsa_87::sign(&self.0.signing_key, message, context, [0; 32])
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| format!("{e:?}"))
    }
}
const SIGNED_FRAME_CAP: u64 = FRAME_CAP + SIGNATURE_BYTES as u64;
fn sign_packet(witness: &SignedPanelWitnessR1, bytes: &[u8]) -> SignedPanelPartR1 {
    let frame = EvidenceFrame::decode(bytes, FRAME_CAP).unwrap();
    witness.sign_part(&frame, &Key::new(62)).unwrap()
}
fn prepare_node(f: &Fixture) -> PreparedCheck {
    let mut full_frame_limits = check_limits();
    full_frame_limits.max_part_bytes = FRAME_CAP;
    PreparedCheck::authenticate(
        &f.rooted,
        &f.class,
        f.artifact,
        &f.job,
        kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
        full_frame_limits,
        f.result.clone(),
        &f.key,
        &f.signature,
    )
    .unwrap()
}

#[test]
fn original_author_signature_survives_takeover_and_rejects_other_keys_or_content() {
    with_fixture(false, |f, runner, inventory| {
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
        let plan = capture.admission().plan();
        let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
        let authenticated = f.result.authenticate(&f.key, &f.signature).unwrap();
        let signer = Key::new(62);
        let signed = SignedPanelWitnessR1::sign(&authenticated, plan, witness, &signer).unwrap();
        let relay = SignedPanelWitnessR1::from_signed(
            &authenticated,
            plan,
            signed.witness().clone(),
            *signed.author_key(),
            *signed.signature(),
        )
        .unwrap();
        assert_eq!(relay.witness(), signed.witness());
        assert_eq!(relay.signature(), signed.signature());
        let other = Key::new(63);
        assert!(matches!(
            SignedPanelWitnessR1::from_signed(
                &authenticated,
                plan,
                signed.witness().clone(),
                *other.0.verification_key.as_ref(),
                *signed.signature()
            ),
            Err(ContractError::Identity)
        ));
        let mut changed = signed.witness().clone();
        changed.attempt += 1;
        assert!(matches!(
            SignedPanelWitnessR1::from_signed(&authenticated, plan, changed, *signed.author_key(), *signed.signature()),
            Err(ContractError::Signature)
        ));
        let mut changed = signed.witness().clone();
        changed.provenance = d(99);
        assert!(SignedPanelWitnessR1::sign(&authenticated, plan, changed, &signer).is_err());
        let cursor = CoverageCursor { plan: plan.plan, next_relation: 1 };
        assert_eq!(relay.next_range(f.rooted.scope, cursor, 2), Ok((1, 2)));
        let mut wrong_view = f.rooted.scope;
        wrong_view.branch = d(99);
        assert!(relay.next_range(wrong_view, cursor, 2).is_err());
        assert!(relay.next_range(f.rooted.scope, CoverageCursor { plan: PlanId(d(98)), next_relation: 1 }, 2).is_err());
    });
}

#[test]
fn formal_r1_batches_reach_actual_a1_node_port_and_resume_at_live_cursor() {
    for wrong_result in [false, true] {
        with_fixture(wrong_result, |f, runner, inventory| {
            let builder = TirPanelBuilder::new(runner, inventory);
            let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
            let plan = capture.admission().plan();
            let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
            let authenticated = f.result.authenticate(&f.key, &f.signature).unwrap();
            let signed = SignedPanelWitnessR1::sign(&authenticated, plan, witness, &Key::new(62)).unwrap();
            let prepared_node = prepare_node(f);
            assert_eq!(prepared_node.descriptor(), plan);
            let context = prepared_node
                .authenticate_witness(
                    signed.witness().clone(),
                    capture.binding().clone(),
                    capture.generated().to_vec(),
                    signed.author_key(),
                    signed.signature(),
                )
                .unwrap();
            let mut progress = context.start().unwrap();
            let checker = TirNodeChecker;
            assert_eq!(checker.derive_plan(&context).unwrap(), plan);
            let parts = builder.prepare_parts(&capture).unwrap();
            let first = parts.build_part(CoverageCursor { plan: plan.plan, next_relation: 0 }).unwrap();
            let bytes = first.frame(signed.witness(), &plan).unwrap().encode(FRAME_CAP).unwrap();
            let packet = sign_packet(&signed, &bytes);
            let relay = SignedPanelWitnessR1::from_signed(
                &authenticated,
                plan,
                signed.witness().clone(),
                *signed.author_key(),
                *signed.signature(),
            )
            .unwrap();
            let transferred = SignedPanelPartR1::from_signed(packet.bytes().to_vec(), *packet.signature(), FRAME_CAP).unwrap();
            let mut prefix = relay.batch(0, 1, 16, SIGNED_FRAME_CAP).unwrap();
            prefix.push(&transferred).unwrap();
            assert_eq!(prefix.bytes(), bytes.len() as u64 + SIGNATURE_BYTES as u64);
            assert_eq!(prefix.check_with(&checker, &context, &mut progress).unwrap(), 1);
            assert_eq!(progress.snapshot().next_leaf, 1);
            // A stale local send log cannot advance or rewind Node progress.
            assert!(matches!(prefix.check_with(&checker, &context, &mut progress), Err(PanelTransportError::Node(_))));
            // Reconstruct progress by rechecking the accepted prefix rather
            // than deserializing a saved cursor/outcome.
            let mut resumed = context.start().unwrap();
            prefix.check_with(&checker, &context, &mut resumed).unwrap();
            let cursor = CoverageCursor { plan: plan.plan, next_relation: resumed.snapshot().next_leaf };
            let (first, count) = signed.next_range(f.rooted.scope, cursor, plan.mandatory_relations).unwrap();
            let mut suffix = relay.batch(first, count, 16, SIGNED_FRAME_CAP * 16).unwrap();
            for index in first..first + count {
                let part = parts.build_part(CoverageCursor { plan: plan.plan, next_relation: index }).unwrap();
                suffix.push(&signed.sign_part(&part.frame(signed.witness(), &plan).unwrap(), &Key::new(62)).unwrap()).unwrap();
            }
            suffix.check_with(&checker, &context, &mut resumed).unwrap();
            assert_eq!(resumed.snapshot().next_leaf, plan.mandatory_relations);
            let terminal = checker.finish(&context, resumed).unwrap();
            assert_eq!(terminal.outcome(), if wrong_result { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched });
        });
    }
}

#[test]
fn incomplete_ordered_and_over_budget_batches_cannot_skip_mandatory_relations() {
    with_fixture(false, |f, runner, inventory| {
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
        let plan = capture.admission().plan();
        let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
        let authenticated = f.result.authenticate(&f.key, &f.signature).unwrap();
        let signed = SignedPanelWitnessR1::sign(&authenticated, plan, witness, &Key::new(62)).unwrap();
        assert!(signed.batch(0, 0, 16, FRAME_CAP).is_err());
        assert!(signed.batch(u64::MAX, 2, 16, FRAME_CAP).is_err());
        let parts = builder.prepare_parts(&capture).unwrap();
        let zero = parts
            .build_part(CoverageCursor { plan: plan.plan, next_relation: 0 })
            .unwrap()
            .frame(signed.witness(), &plan)
            .unwrap()
            .encode(FRAME_CAP)
            .unwrap();
        let one = parts
            .build_part(CoverageCursor { plan: plan.plan, next_relation: 1 })
            .unwrap()
            .frame(signed.witness(), &plan)
            .unwrap()
            .encode(FRAME_CAP)
            .unwrap();
        let zero_packet = sign_packet(&signed, &zero);
        let one_packet = sign_packet(&signed, &one);
        let mut batch = signed.batch(0, 2, 16, SIGNED_FRAME_CAP * 2).unwrap();
        assert!(batch.push(&one_packet).is_err());
        assert_eq!(batch.bytes(), 0);
        batch.push(&zero_packet).unwrap();
        let charged = batch.bytes();
        assert!(batch.push(&zero_packet).is_err());
        assert_eq!(batch.bytes(), charged);
        let prepared_node = prepare_node(f);
        let context = prepared_node
            .authenticate_witness(
                signed.witness().clone(),
                capture.binding().clone(),
                capture.generated().to_vec(),
                signed.author_key(),
                signed.signature(),
            )
            .unwrap();
        let mut progress = context.start().unwrap();
        assert!(matches!(batch.check_with(&TirNodeChecker, &context, &mut progress), Err(PanelTransportError::Contract(_))));
        assert_eq!(progress.snapshot().next_leaf, 0);
        let mut tiny = signed.batch(0, 1, 1, SIGNATURE_BYTES as u64 + 128).unwrap();
        assert!(matches!(tiny.push(&zero_packet), Err(ContractError::Budget)));
        assert_eq!(tiny.bytes(), 0);
        // Corrupt only arithmetic bytes within an otherwise valid formal frame.
        let frame = EvidenceFrame::decode(&one, FRAME_CAP).unwrap();
        let raw = tir::decode_frame(&frame, FRAME_CAP).unwrap();
        let mut cone: kaspa_consensus_core::palw_tir_court_v1::PalwTirConeRefutationV1 = borsh::from_slice(raw.cone()).unwrap();
        cone.output_preimage.values_le[0] ^= 1;
        let cone = borsh::to_vec(&cone).unwrap();
        let payload =
            tir::ExactPart::new(&cone, raw.trace(), FRAME_CAP - misaka_palw_positive_contract::wire::EVIDENCE_HEADER_BYTES as u64)
                .unwrap()
                .encode();
        let mut header = frame.header();
        header.payload_bytes = payload.len() as u32;
        let bad = EvidenceFrame::new(header, &payload, FRAME_CAP).unwrap().encode(FRAME_CAP).unwrap();
        // A deliberately malicious ORIGINAL author signs this invalid arithmetic.
        // This differs from an unauthenticated relay modifying another author's packet.
        let bad_packet = sign_packet(&signed, &bad);
        let mut partial = signed.batch(0, 2, 16, SIGNED_FRAME_CAP * 2).unwrap();
        partial.push(&zero_packet).unwrap();
        partial.push(&bad_packet).unwrap();
        assert!(matches!(partial.check_with(&TirNodeChecker, &context, &mut progress), Err(PanelTransportError::Node(_))));
        assert_eq!(progress.snapshot().next_leaf, 1);
        assert!(TirNodeChecker.finish(&context, progress).is_err());
    });
}

#[test]
fn unauthenticated_relay_mutations_never_touch_node_progress_or_charge_arithmetic() {
    with_fixture(false, |f, runner, inventory| {
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
        let plan = capture.admission().plan();
        let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
        let authenticated = f.result.authenticate(&f.key, &f.signature).unwrap();
        let signed = SignedPanelWitnessR1::sign(&authenticated, plan, witness, &Key::new(62)).unwrap();
        let part = builder.build_part(&capture, CoverageCursor { plan: plan.plan, next_relation: 0 }).unwrap();
        let frame = part.frame(signed.witness(), &plan).unwrap();
        let original = signed.sign_part(&frame, &Key::new(62)).unwrap();
        assert!(signed.sign_part(&frame, &Key::new(63)).is_err());
        let raw = tir::decode_frame(&frame, FRAME_CAP).unwrap();
        let cone: kaspa_consensus_core::palw_tir_court_v1::PalwTirConeRefutationV1 = borsh::from_slice(raw.cone()).unwrap();
        let encode_changed_cone = |cone: &kaspa_consensus_core::palw_tir_court_v1::PalwTirConeRefutationV1| {
            let bytes = borsh::to_vec(cone).unwrap();
            let payload = tir::ExactPart::new(
                &bytes,
                raw.trace(),
                FRAME_CAP - misaka_palw_positive_contract::wire::EVIDENCE_HEADER_BYTES as u64,
            )
            .unwrap()
            .encode();
            let mut header = frame.header();
            header.payload_bytes = payload.len() as u32;
            EvidenceFrame::new(header, &payload, FRAME_CAP).unwrap().encode(FRAME_CAP).unwrap()
        };
        let mut arithmetic = cone.clone();
        arithmetic.output_preimage.values_le[0] ^= 1;
        let mut missing_opening = cone.clone();
        assert!(!missing_opening.output_opening.siblings.is_empty());
        missing_opening.output_opening.siblings.clear();
        let mut header = frame.header();
        header.case = CaseId(d(99));
        let changed_header = EvidenceFrame::new(header, frame.payload(), FRAME_CAP).unwrap().encode(FRAME_CAP).unwrap();
        let mut signature = *original.signature();
        signature[0] ^= 1;
        let packets = vec![
            SignedPanelPartR1::from_signed(encode_changed_cone(&arithmetic), *original.signature(), FRAME_CAP).unwrap(),
            SignedPanelPartR1::from_signed(encode_changed_cone(&missing_opening), *original.signature(), FRAME_CAP).unwrap(),
            SignedPanelPartR1::from_signed(changed_header, *original.signature(), FRAME_CAP).unwrap(),
            SignedPanelPartR1::from_signed(original.bytes().to_vec(), signature, FRAME_CAP).unwrap(),
            SignedPanelPartR1::from_signed(original.bytes().to_vec(), [0; SIGNATURE_BYTES], FRAME_CAP).unwrap(),
        ];
        let prepared = prepare_node(f);
        let context = prepared
            .authenticate_witness(
                signed.witness().clone(),
                capture.binding().clone(),
                capture.generated().to_vec(),
                signed.author_key(),
                signed.signature(),
            )
            .unwrap();
        let mut progress = context.start().unwrap();
        let before = progress.snapshot();
        for packet in packets {
            let mut batch = signed.batch(0, 1, 1, SIGNED_FRAME_CAP).unwrap();
            assert!(batch.push(&packet).is_err());
            assert_eq!(batch.bytes(), 0);
            assert!(matches!(batch.check_with(&TirNodeChecker, &context, &mut progress), Err(PanelTransportError::Contract(_))));
            assert_eq!(progress.snapshot(), before);
        }
        // Exact original author-signed bytes remain transferable by a new relay.
        let transferred = SignedPanelPartR1::from_signed(original.bytes().to_vec(), *original.signature(), FRAME_CAP).unwrap();
        let relay = SignedPanelWitnessR1::from_signed(
            &authenticated,
            plan,
            signed.witness().clone(),
            *signed.author_key(),
            *signed.signature(),
        )
        .unwrap();
        let mut batch = relay.batch(0, 1, 1, SIGNED_FRAME_CAP).unwrap();
        batch.push(&transferred).unwrap();
        assert_eq!(batch.check_with(&TirNodeChecker, &context, &mut progress).unwrap(), 1);
        assert_eq!(progress.snapshot().next_leaf, 1);
    });
}

#[test]
fn acyclic_funding_consent_precedes_final_result_and_actual_panel_node_plan() {
    use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
    use misaka_palw_positive_contract::{
        CONTRACT_REVISION, CONTRACT_VERSION,
        funding::{FundingConsent, FundingPlanBasis, funding_scope},
        native,
    };
    use misaka_palw_tir_exec::node::{positive_check::CoveragePlan, positive_panel::PanelAdmission};
    with_fixture(false, |f, runner, inventory| {
        // These fork/policy/quote/address digests are explicit synthetic inputs
        // from a fixed fixture resolver, not native funding/UTXO capabilities.
        let fired_fork = d(70);
        let policy = d(71);
        let mut rooted = f.rooted;
        rooted.scope.ruleset = native::ruleset_id(fired_fork, policy);
        let coverage = CoveragePlan::derive(&f.class, f.artifact, &f.job, PalwPromptIdsFormV1::Flat, check_limits()).unwrap();
        let relations = coverage.leaf_count();
        let basis = FundingPlanBasis {
            profile: CheckProfile::ExactStreamV1,
            derivation: tir::exact_derivation_id(),
            mandatory_relations: relations,
            source_catalog: d(72),
            resource_quote: d(73),
            policy,
            schedule: d(74),
            required_fee: 3,
            required_capital: 5,
        };
        let mut result_core = f.result.clone();
        result_core.scope = rooted.scope;
        result_core.service_contract = Digest([0; 64]);
        let stale_preconsent_plan = coverage_plan_id(&result_core, basis.profile, basis.derivation, relations).unwrap();
        let mut service = MinerServiceContract {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result_core.case_id(),
            source_catalog: basis.source_catalog,
            provider_delegations: d(75),
            budget: AggregateBudget {
                rounds: 4,
                unique_source_bytes: 1 << 20,
                transmitted_bytes: 1 << 22,
                additional_work: 1000,
                retained_bytes: 1 << 20,
                retention_until: 100,
                fee: 2,
                witness_attempts: 2,
            },
            funding_consent: Digest([0; 64]),
            deadline: 90,
        };
        let payer = Key::new(63);
        let consent = FundingConsent {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            case: result_core.case_id(),
            scope: funding_scope(&result_core, &service, &basis),
            payer: principal(payer.0.verification_key.as_ref()),
            max_fees: 4,
            max_locked: 9,
            funding_output_index: 0,
            refund_payload: d(93), // Synthetic address payload, deliberately not a principal/key ID.
            admission_expiry: 80,
        };
        assert_ne!(consent.refund_payload, consent.payer.0);
        let payer_signature: [u8; SIGNATURE_BYTES] =
            payer.sign_with_context(&consent.id().0.0, FUNDING_SIGN_CONTEXT).unwrap().try_into().unwrap();
        service.funding_consent = consent.id().0;
        let mut final_result = result_core.clone();
        final_result.service_contract = service.id();
        let payer_auth =
            consent.authenticate(&final_result, &service, &basis, 1, payer.0.verification_key.as_ref(), &payer_signature).unwrap();
        assert_eq!(payer_auth.basis().mandatory_relations, relations);
        assert_eq!(consent.scope, funding_scope(&final_result, &service, &basis));
        assert_eq!(final_result.case_id(), result_core.case_id());
        assert_ne!(final_result.id(), result_core.id());
        // Invalid literal terms fail the shared helper even with coherent
        // acyclic links. No actual output qualification is performed here.
        for bad_term in 0..3 {
            let mut invalid = consent.clone();
            match bad_term {
                0 => invalid.max_fees = 2,
                1 => invalid.max_locked = 7,
                _ => invalid.admission_expiry = 1,
            }
            let mut invalid_service = service;
            invalid_service.funding_consent = invalid.id().0;
            let mut invalid_result = final_result.clone();
            invalid_result.service_contract = invalid_service.id();
            assert_eq!(invalid.validate(&invalid_result, &invalid_service, &basis, 1), Err(ContractError::Budget));
        }
        let miner = Key::new(61);
        let miner_signature: [u8; SIGNATURE_BYTES] =
            miner.sign_with_context(&final_result.id().0.0, RESULT_SIGN_CONTEXT).unwrap().try_into().unwrap();
        let authenticated = final_result.authenticate(&f.key, &miner_signature).unwrap();
        let admitted = PanelAdmission::authenticate(
            &final_result,
            rooted,
            &f.class,
            &f.job,
            PROMPT,
            PalwPromptIdsFormV1::Flat,
            &f.key,
            &miner_signature,
            panel_limits(),
        )
        .unwrap();
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&admitted).unwrap();
        let final_plan = admitted.plan();
        assert_eq!(final_plan.mandatory_relations, basis.mandatory_relations);
        assert_eq!(final_plan.plan, coverage_plan_id(&final_result, basis.profile, basis.derivation, coverage.leaf_count()).unwrap());
        assert_ne!(final_plan.plan, stale_preconsent_plan);
        let mut stale = final_plan;
        stale.plan = stale_preconsent_plan;
        assert!(builder.commit(&capture, &stale, witness_author(), 0).is_err());
        let witness = builder.commit(&capture, &final_plan, witness_author(), 0).unwrap();
        let signed = SignedPanelWitnessR1::sign(&authenticated, final_plan, witness, &Key::new(62)).unwrap();
        let mut full_limits = check_limits();
        full_limits.max_part_bytes = FRAME_CAP;
        let prepared = PreparedCheck::authenticate(
            &rooted,
            &f.class,
            f.artifact,
            &f.job,
            PalwPromptIdsFormV1::Flat,
            full_limits,
            final_result.clone(),
            &f.key,
            &miner_signature,
        )
        .unwrap();
        assert_eq!(prepared.descriptor(), final_plan);
        let context = prepared
            .authenticate_witness(
                signed.witness().clone(),
                capture.binding().clone(),
                capture.generated().to_vec(),
                signed.author_key(),
                signed.signature(),
            )
            .unwrap();
        let mut progress = context.start().unwrap();
        let mut batch = signed.batch(0, relations, relations, SIGNED_FRAME_CAP * relations).unwrap();
        let parts = builder.prepare_parts(&capture).unwrap();
        for next_relation in 0..relations {
            let part = parts.build_part(CoverageCursor { plan: final_plan.plan, next_relation }).unwrap();
            let packet = signed.sign_part(&part.frame(signed.witness(), &final_plan).unwrap(), &Key::new(62)).unwrap();
            batch.push(&packet).unwrap();
        }
        assert_eq!(batch.check_with(&TirNodeChecker, &context, &mut progress).unwrap(), relations);
        assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
    });
}
