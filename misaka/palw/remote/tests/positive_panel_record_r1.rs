#![cfg(feature = "positive-panel")]
//! Primitive-record delivery is checked before creating arithmetic progress.
//! A corrupt relay delivery cannot consume the original signed Panel candidate.
#[path = "../../tir/exec/tests/positive_panel_r1_common/mod.rs"]
mod common;
use common::*;
use kaspa_consensus_core::palw_prompt_ids_v1::PalwPromptIdsFormV1;
use misaka_palw_positive_contract::{
    CONTRACT_REVISION, CONTRACT_VERSION, ContractError,
    identity::*,
    messages::*,
    ports::{NodeChecker, PanelBuilder},
    record::{ExecutionRecord, RECORD_HEADER_BYTES, RECORD_MAGIC, RECORD_OVERHEAD_BYTES, RecordHeader, encode_record},
};
use misaka_palw_remote::{bundle::DetachedSigner, positive_panel::SignedPanelWitnessR1};
use misaka_palw_tir_exec::node::{
    positive_check::CheckOutcome,
    positive_contract::{CheckContext, PreparedCheck, TirNodeChecker, record::RecordDeliveryError},
    positive_panel::TirPanelBuilder,
};

const RECORD_CAP: u64 = 1 << 20;
const SIGNED_FRAME_CAP: u64 = FRAME_CAP + SIGNATURE_BYTES as u64;
struct PanelKey(libcrux_ml_dsa::ml_dsa_87::MLDSA87KeyPair);
impl PanelKey {
    fn new() -> Self {
        Self(libcrux_ml_dsa::ml_dsa_87::generate_key_pair([62; 32]))
    }
}
impl DetachedSigner for PanelKey {
    fn public_key(&self) -> Vec<u8> {
        self.0.verification_key.as_ref().to_vec()
    }
    fn sign_with_context(&self, message: &[u8], context: &[u8]) -> Result<Vec<u8>, String> {
        libcrux_ml_dsa::ml_dsa_87::sign(&self.0.signing_key, message, context, [0; 32])
            .map(|s| s.as_ref().to_vec())
            .map_err(|e| format!("{e:?}"))
    }
}
fn prepare(f: &Fixture) -> PreparedCheck {
    let mut limits = check_limits();
    limits.max_part_bytes = FRAME_CAP;
    PreparedCheck::authenticate(
        &f.rooted,
        &f.class,
        f.artifact,
        &f.job,
        PalwPromptIdsFormV1::Flat,
        limits,
        f.result.clone(),
        &f.key,
        &f.signature,
    )
    .unwrap()
}
fn receive_record(
    prepared: &PreparedCheck,
    signed: &SignedPanelWitnessR1,
    bytes: &[u8],
    cap: u64,
) -> Result<CheckContext, RecordDeliveryError> {
    prepared.authenticate_record(signed.witness().clone(), bytes, cap, signed.author_key(), signed.signature())
}

#[test]
fn primitive_execution_record_roundtrips_literal_roots_and_ids_with_exact_extent() {
    with_fixture(false, |f, runner, inventory| {
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
        let encoded = capture.encode_execution_record(RECORD_CAP).unwrap();
        assert_eq!(RECORD_OVERHEAD_BYTES, 344);
        assert_eq!(encoded.len(), 344 + 4 * EXPECTED.len());
        assert_eq!(&encoded[..8], RECORD_MAGIC);
        let decoded = ExecutionRecord::decode(&encoded, encoded.len() as u64, f.result.decode).unwrap();
        let binding = capture.binding();
        let expected = RecordHeader {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            result: f.result.id(),
            artifact: digest(binding.artifact_root),
            full_logits_trace_root: digest(binding.full_logits_trace_root),
            step_leaf_count: binding.step_leaf_count,
            step_merkle_root: digest(binding.step_merkle_root),
            committed_execution_root: digest(binding.committed_execution_root),
        };
        assert_eq!(decoded.header(), expected);
        assert_eq!(decoded.tokens().collect::<Vec<_>>(), EXPECTED);
        assert_eq!(decoded.generated_count(), EXPECTED.len() as u32);
        assert_eq!(decoded.token(EXPECTED.len() as u32), None);
        assert_eq!(decoded.wire_bytes(), encoded.len() as u64);
        assert_eq!(decoded.generated_bytes(), EXPECTED.iter().flat_map(|id| id.to_le_bytes()).collect::<Vec<_>>());
        assert_eq!(encode_record(expected, capture.generated(), f.result.decode, encoded.len() as u64).unwrap(), encoded);
        assert_eq!(capture.encode_execution_record(encoded.len() as u64 - 1), Err(ContractError::Budget));
        assert_eq!(ExecutionRecord::decode(&encoded, encoded.len() as u64 - 1, f.result.decode), Err(ContractError::Budget));
        assert!(ExecutionRecord::decode(&encoded, RECORD_CAP, f.result.decode - 1).is_err());
        // The registered program/dtypes and shared derivation are not changed
        // to emit this compact roots-and-token-IDs object.
        assert_eq!(binding.class, f.class);
        assert_eq!(runner.plan.program.encode(), f.class.program);
        let plan = capture.admission().plan();
        assert_eq!(
            plan.plan,
            coverage_plan_id(
                &f.result,
                CheckProfile::ExactStreamV1,
                misaka_palw_positive_contract::tir::exact_derivation_id(),
                binding.step_leaf_count
            )
            .unwrap()
        );
    });
}

#[test]
fn primitive_record_reconstructs_trusted_binding_and_checks_all_signed_relations() {
    for wrong_result in [false, true] {
        with_fixture(wrong_result, |f, runner, inventory| {
            let builder = TirPanelBuilder::new(runner, inventory);
            let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
            let plan = capture.admission().plan();
            let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
            let miner = f.result.authenticate(&f.key, &f.signature).unwrap();
            let signer = PanelKey::new();
            let signed = SignedPanelWitnessR1::sign(&miner, plan, witness, &signer).unwrap();
            let encoded = capture.encode_execution_record(RECORD_CAP).unwrap();
            ExecutionRecord::decode(&encoded, RECORD_CAP, f.result.decode)
                .unwrap()
                .validate_binding(&f.result, signed.witness())
                .unwrap();
            // No serialized class, job, program or generated Vec is supplied
            // separately. A1 reconstructs using its independently rooted state.
            let prepared = prepare(f);
            let context = receive_record(&prepared, &signed, &encoded, RECORD_CAP).unwrap();
            let mut progress = context.start().unwrap();
            assert_eq!(progress.snapshot().next_leaf, 0);
            let parts = builder.prepare_parts(&capture).unwrap();
            let mut batch = signed
                .batch(0, plan.mandatory_relations, plan.mandatory_relations, SIGNED_FRAME_CAP * plan.mandatory_relations)
                .unwrap();
            for next_relation in 0..plan.mandatory_relations {
                let part = parts.build_part(CoverageCursor { plan: plan.plan, next_relation }).unwrap();
                batch.push(&signed.sign_part(&part.frame(signed.witness(), &plan).unwrap(), &signer).unwrap()).unwrap();
            }
            assert_eq!(batch.check_with(&TirNodeChecker, &context, &mut progress).unwrap(), plan.mandatory_relations);
            assert_eq!(progress.snapshot().next_leaf, plan.mandatory_relations);
            assert_eq!(
                TirNodeChecker.finish(&context, progress).unwrap().outcome(),
                if wrong_result { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched }
            );
        });
    }
}

#[test]
fn corrupt_record_deliveries_do_not_poison_original_signed_witness_or_retry() {
    with_fixture(false, |f, runner, inventory| {
        let builder = TirPanelBuilder::new(runner, inventory);
        let capture = builder.replay_and_capture(&f.admit().unwrap()).unwrap();
        let plan = capture.admission().plan();
        let witness = builder.commit(&capture, &plan, witness_author(), 0).unwrap();
        let miner = f.result.authenticate(&f.key, &f.signature).unwrap();
        let signer = PanelKey::new();
        let signed = SignedPanelWitnessR1::sign(&miner, plan, witness, &signer).unwrap();
        let valid = capture.encode_execution_record(RECORD_CAP).unwrap();
        let decoded = ExecutionRecord::decode(&valid, RECORD_CAP, f.result.decode).unwrap();
        let mut malformed = Vec::new();
        for field in 0..6 {
            let mut header = decoded.header();
            match field {
                0 => header.result = ResultId(d(91)),
                1 => header.artifact = d(92),
                2 => header.full_logits_trace_root = d(93),
                3 => header.step_leaf_count += 1,
                4 => header.step_merkle_root = d(94),
                _ => header.committed_execution_root = d(95),
            }
            malformed.push(encode_record(header, EXPECTED, f.result.decode, RECORD_CAP).unwrap());
        }
        for field in 0..7 {
            let mut bytes = valid.clone();
            match field {
                0 => bytes[8] ^= 1,  // header version
                1 => bytes[10] ^= 1, // header revision
                2 => bytes[0] ^= 1,  // magic
                3 => bytes[RECORD_MAGIC.len() + RECORD_HEADER_BYTES..RECORD_OVERHEAD_BYTES].copy_from_slice(&u32::MAX.to_le_bytes()),
                4 => bytes[RECORD_OVERHEAD_BYTES..RECORD_OVERHEAD_BYTES + 4].copy_from_slice(&99u32.to_le_bytes()),
                5 => bytes[RECORD_OVERHEAD_BYTES..RECORD_OVERHEAD_BYTES + 4].copy_from_slice(&2u32.to_le_bytes()),
                _ => bytes.push(0),
            }
            malformed.push(bytes);
        }
        malformed.push(valid[..valid.len() - 1].to_vec());
        // PreparedCheck is retained across untrusted record deliveries. No
        // arithmetic progress or native attempt is created by a codec refusal.
        let prepared = prepare(f);
        with_fixture(true, |other, _, _| {
            // An independently prepared, different signed Result has no custody
            // of this witness/record even though its registered model is equal.
            let other_prepared = prepare(other);
            assert!(receive_record(&other_prepared, &signed, &valid, RECORD_CAP).is_err());
        });
        assert!(receive_record(&prepared, &signed, &valid, valid.len() as u64 - 1).is_err());
        for bytes in malformed {
            assert!(receive_record(&prepared, &signed, &bytes, RECORD_CAP).is_err());
            let original = receive_record(&prepared, &signed, &valid, RECORD_CAP).unwrap();
            assert_eq!(original.start().unwrap().snapshot().next_leaf, 0);
            assert_eq!(signed.witness().attempt, 0);
        }
        // The original author-signed candidate remains usable after all of the
        // bad deliveries, with its original attempt and unchanged exact plan.
        let context = receive_record(&prepared, &signed, &valid, RECORD_CAP).unwrap();
        let mut progress = context.start().unwrap();
        let parts = builder.prepare_parts(&capture).unwrap();
        for next_relation in 0..plan.mandatory_relations {
            let part = parts.build_part(CoverageCursor { plan: plan.plan, next_relation }).unwrap();
            let mut batch = signed.batch(next_relation, 1, 1, SIGNED_FRAME_CAP).unwrap();
            batch.push(&signed.sign_part(&part.frame(signed.witness(), &plan).unwrap(), &signer).unwrap()).unwrap();
            batch.check_with(&TirNodeChecker, &context, &mut progress).unwrap();
        }
        assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
    });
}
