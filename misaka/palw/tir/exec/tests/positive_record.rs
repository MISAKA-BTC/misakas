//! Borrowed primitive-only execution-record delivery into the actual A1 checker.
#![cfg(feature = "positive-contract")]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1},
    palw_tir_court_v1::{PalwTirCourtRulesV1, tir_logits_event_disclosure_v1},
    palw_tir_step_v1::{PALW_TIR_STEP_BINDING_VERSION_V1, PalwTirLeafKindV1, PalwTirStepBindingV1, PalwTirStepSpaceV1},
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::*,
    messages::*,
    ports::NodeChecker,
    record::{ExecutionRecord, RECORD_OVERHEAD_BYTES, RecordHeader, encode_record},
    tir::{ExactPart, greedy_selector_id, provenance_id},
    wire::*,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_check::{CheckLimits, CheckOutcome},
        positive_contract::{
            AdapterError, CheckContext, ContractProgress, PreparedCheck, TirNodeChecker,
            record::{RecordDeliveryError, RecordLimits},
        },
    },
};

fn d(n: u8) -> Digest {
    Digest([n; 64])
}
fn native(hash: Hash64) -> Digest {
    Digest(hash.as_bytes())
}
fn limits() -> CheckLimits {
    CheckLimits {
        max_leaves: 1 << 20,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 40,
        max_elements_per_leaf: 1 << 20,
        max_terms_per_leaf: 1 << 24,
        max_total_elements: 1 << 40,
        max_total_terms: 1 << 44,
    }
}
fn miner_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]).verification_key.as_ref()
}
fn panel_key() -> [u8; PUBKEY_BYTES] {
    *libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]).verification_key.as_ref()
}
fn sign_result(result: &MinerResultCommit) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &result.id().0.0, RESULT_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}
fn sign_witness(witness: &PanelWitnessCommit) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}
struct Fixture {
    binding: PalwTirStepBindingV1,
    generated: Vec<u32>,
    parts: Vec<(Vec<u8>, Option<Vec<u8>>)>,
}
fn fixture() -> Fixture {
    // Independent tiny arithmetic: logits = [7,13,2,13] + scalar_embed[token].
    // The prompt's final token and all generated inputs are 1, whose scalar is
    // 1, so both checked rows are [8,14,3,14] and lowest-index greedy is 1.
    let mut pb = ProgramBuilder::new(4, 1 << 18);
    let embed = pb.param("scalar_embed", DType::I8, &[4, 1], false);
    let mut pre = pb.block("pre", vec![]);
    let x = pre.gather(embed, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let bias = pb.konst(DType::I32, &[4], &[7, 13, 2, 13]);
    let mut post = pb.block("post", vec![TensorType::fixed(DType::I8, &[1])]);
    let scalar = post.cast(Ref::CarryIn(0), DType::I32);
    let logits = post.add(bias, scalar, DType::I32);
    let Ref::Node(logits_node) = logits else { unreachable!() };
    post.commit(logits);
    let post = post.finish(&[]);
    let mut program = node_common::tiled(pb.finish(pre, vec![], post, logits_node));
    for block in &mut program.blocks {
        for node in &mut block.nodes {
            node.commit = true;
        }
    }
    let mut map = MapParams::default();
    map.tensors.insert((0, None), Tensor::new(DType::I8, vec![4, 1], vec![0, 1, 2, 3]).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: PALW_TIR_CLASS_VERSION_V1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 2, 2, 64),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let class_id = class.class_id(&artifact);
    let prompt = vec![0, 1];
    let mut job = node_common::job(&program, 2, 2, class_id, &prompt, PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, class_id).unwrap();
    let mut leaves = Vec::new();
    let run = runner.run(&job, &prompt, limits().max_leaves, true, &mut |l| leaves.push(l.preimage.clone())).unwrap();
    assert_eq!(run.generated, vec![1, 1]);
    assert_eq!(run.logits_rows, vec![vec![8, 14, 3, 14], vec![8, 14, 3, 14]]);
    let binding = PalwTirStepBindingV1 {
        version: PALW_TIR_STEP_BINDING_VERSION_V1,
        job_context: job,
        class,
        artifact_root: artifact,
        full_logits_trace_root: run.trace_root,
        step_leaf_count: run.leaf_count,
        step_merkle_root: run.step_merkle_root,
        committed_execution_root: run.execution_root,
    };
    let rules = PalwTirCourtRulesV1 {
        max_step_leaf_count: limits().max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: limits().max_elements_per_leaf, max_terms: limits().max_terms_per_leaf },
    };
    let store = TirEvidenceV1::dense(
        &runner,
        &binding,
        &prompt,
        &run.logits_rows,
        &run.generated,
        &leaves,
        &inventory,
        PalwPromptIdsFormV1::Flat,
        limits().max_leaves,
    )
    .unwrap();
    let parts = (0..binding.step_leaf_count)
        .map(|i| {
            let cone = store.cone_refutation(i, &rules).unwrap();
            let leaf = space.leaf_at(&binding.job_context, i).unwrap();
            let trace = match leaf.kind {
                PalwTirLeafKindV1::Commit { occurrence, node, .. }
                    if occurrence as usize == space.occurrences().len() - 1 && node == program.logits =>
                {
                    Some(
                        tir_logits_event_disclosure_v1(
                            &binding,
                            &run.logits_rows,
                            &run.generated,
                            leaf.position + 1 - binding.job_context.declared_prefill_tokens,
                            0,
                        )
                        .unwrap(),
                    )
                }
                _ => None,
            };
            (borsh::to_vec(&cone).unwrap(), trace.map(|t| borsh::to_vec(&t).unwrap()))
        })
        .collect();
    Fixture { binding, generated: run.generated, parts }
}
impl Fixture {
    fn result(&self, output: Digest) -> MinerResultCommit {
        let job = &self.binding.job_context;
        MinerResultCommit {
            version: 2,
            revision: 1,
            scope: ChainScope { network: d(1), ruleset: d(2), branch: d(3) },
            job: native(job.job_id),
            execution: native(job.context_hash()),
            miner: principal(&miner_key()),
            registered_class: native(self.binding.class.class_id(&self.binding.artifact_root)),
            artifact: native(self.binding.artifact_root),
            program: native(self.binding.class.graph_ir_root()),
            input: native(job.prompt_token_ids_hash),
            tokenizer: native(job.tokenizer_id),
            execution_rules: native(job.cu_ruleset_id),
            selector: greedy_selector_id(),
            generation_seed: job.execution_seed,
            temperature_q24: 0,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output,
            dependencies: d(21),
            service_contract: d(22),
        }
    }
    fn correct_result(&self) -> MinerResultCommit {
        self.result(native(palw_attempt_output_root_v1(&self.binding.job_context, &self.generated)))
    }
    fn prepare(&self, result: &MinerResultCommit, limits: CheckLimits) -> PreparedCheck {
        PreparedCheck::authenticate(
            &rooted(result),
            &self.binding.class,
            self.binding.artifact_root,
            &self.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            limits,
            result.clone(),
            &miner_key(),
            &sign_result(result),
        )
        .unwrap()
    }
    fn witness(&self, result: &MinerResultCommit, plan: CoverageDescriptor) -> PanelWitnessCommit {
        PanelWitnessCommit {
            version: 2,
            revision: 1,
            scope: result.scope,
            case: result.case_id(),
            result: result.id(),
            author: principal(&panel_key()),
            attempt: 0,
            profile: CheckProfile::ExactStreamV1,
            plan: plan.plan,
            execution_record: native(self.binding.committed_execution_root),
            provenance: provenance_id(result),
            candidate_output: native(palw_attempt_output_root_v1(&self.binding.job_context, &self.generated)),
        }
    }
    fn frames(&self, witness: &PanelWitnessCommit, descriptor: CoverageDescriptor) -> Vec<Vec<u8>> {
        self.parts
            .iter()
            .enumerate()
            .map(|(index, (cone, trace))| {
                let payload =
                    ExactPart::new(cone, trace.as_deref(), descriptor.max_part_bytes - EVIDENCE_HEADER_BYTES as u64).unwrap().encode();
                frame_bytes(witness, descriptor, index as u64, &payload)
            })
            .collect()
    }
}
fn rooted(result: &MinerResultCommit) -> AdmissionContext {
    AdmissionContext {
        scope: result.scope,
        expected_job: result.job,
        expected_execution: result.execution,
        expected_miner: result.miner,
        expected_input: result.input,
        registered_class: result.registered_class,
        artifact: result.artifact,
        program: result.program,
        tokenizer: result.tokenizer,
        execution_rules: result.execution_rules,
        selector: result.selector,
        required_form: ClaimForm::PositiveResultV2,
    }
}
fn frame_bytes(witness: &PanelWitnessCommit, descriptor: CoverageDescriptor, index: u64, payload: &[u8]) -> Vec<u8> {
    EvidenceFrame::new(
        EvidenceHeader {
            case: witness.case,
            result: witness.result,
            witness: witness.id(),
            plan: descriptor.plan,
            first_relation: index,
            relation_count: 1,
            payload_bytes: payload.len() as u32,
        },
        payload,
        descriptor.max_part_bytes,
    )
    .unwrap()
    .encode(descriptor.max_part_bytes)
    .unwrap()
}
fn record_header(f: &Fixture, result: &MinerResultCommit) -> RecordHeader {
    RecordHeader {
        version: 2,
        revision: 1,
        result: result.id(),
        artifact: native(f.binding.artifact_root),
        full_logits_trace_root: native(f.binding.full_logits_trace_root),
        step_leaf_count: f.binding.step_leaf_count,
        step_merkle_root: native(f.binding.step_merkle_root),
        committed_execution_root: native(f.binding.committed_execution_root),
    }
}
fn record_bytes(f: &Fixture, result: &MinerResultCommit) -> Vec<u8> {
    encode_record(record_header(f, result), &f.generated, result.decode, 1 << 20).unwrap()
}
fn record_limits(bytes: &[u8], decode: u32) -> RecordLimits {
    RecordLimits { max_record_bytes: bytes.len() as u64, max_generated_heap_bytes: u64::from(decode) * 4 }
}
fn deliver(
    prepared: &PreparedCheck,
    witness: &PanelWitnessCommit,
    bytes: &[u8],
    limits: impl Into<RecordLimits>,
) -> Result<CheckContext, RecordDeliveryError> {
    prepared.authenticate_record(witness.clone(), bytes, limits, &panel_key(), &sign_witness(witness))
}
fn submit_signed(context: &CheckContext, progress: &mut ContractProgress, bytes: &[u8]) -> Result<(), AdapterError> {
    let descriptor = TirNodeChecker.derive_plan(context)?;
    let frame = EvidenceFrame::decode(bytes, descriptor.max_part_bytes)?;
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    let signature: [u8; SIGNATURE_BYTES] =
        *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    TirNodeChecker.check_signed_part(context, progress, frame, &panel_key(), &signature)
}
fn finish_signed(f: &Fixture, context: &CheckContext, witness: &PanelWitnessCommit) {
    let mut progress = context.start().unwrap();
    for frame in f.frames(witness, TirNodeChecker.derive_plan(context).unwrap()) {
        submit_signed(context, &mut progress, &frame).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(context, progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn primitive_only_record_reconstructs_trusted_binding_then_signed_parts_are_checked() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let witness = f.witness(&result, prepared.descriptor());
    let bytes = record_bytes(&f, &result);
    assert_eq!(bytes.len(), RECORD_OVERHEAD_BYTES + f.generated.len() * 4);
    let borrowed = ExecutionRecord::decode(&bytes, bytes.len() as u64, result.decode).unwrap();
    assert_eq!(borrowed.header(), record_header(&f, &result));
    assert_eq!(borrowed.tokens().collect::<Vec<_>>(), vec![1, 1]);
    borrowed.validate_binding(&result, &witness).unwrap();
    let context = deliver(&prepared, &witness, &bytes, record_limits(&bytes, result.decode)).unwrap();
    finish_signed(&f, &context, &witness);
    // A3's scalar-cap convenience route must retain the same byte identities.
    let scalar_cap_context = deliver(&prepared, &witness, &bytes, bytes.len() as u64).unwrap();
    assert_eq!(TirNodeChecker.derive_plan(&context).unwrap(), TirNodeChecker.derive_plan(&scalar_cap_context).unwrap());
}

#[test]
fn corrupt_record_delivery_does_not_consume_the_prepared_witness_or_touch_prior_progress() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let witness = f.witness(&result, prepared.descriptor());
    let valid = record_bytes(&f, &result);
    let cap = record_limits(&valid, result.decode);
    let prior_context = deliver(&prepared, &witness, &valid, cap).unwrap();
    let mut prior_progress = prior_context.start().unwrap();
    let frames = f.frames(&witness, TirNodeChecker.derive_plan(&prior_context).unwrap());
    submit_signed(&prior_context, &mut prior_progress, &frames[0]).unwrap();
    let prior_snapshot = prior_progress.snapshot();
    let prior_wire = prior_progress.charged_wire_bytes();
    let prior_signature = prior_progress.charged_signature_bytes();
    let mut corrupt = Vec::new();
    for field in 0..6 {
        let mut header = record_header(&f, &result);
        match field {
            0 => header.artifact = d(99),
            1 => header.result = ResultId(d(99)),
            2 => header.full_logits_trace_root = d(99),
            3 => header.step_merkle_root = d(99),
            4 => header.committed_execution_root = d(99),
            _ => header.step_leaf_count += 1,
        }
        corrupt.push(encode_record(header, &f.generated, result.decode, 1 << 20).unwrap());
    }
    let mut bad_tokens = valid.clone();
    bad_tokens[RECORD_OVERHEAD_BYTES..RECORD_OVERHEAD_BYTES + 4].copy_from_slice(&3u32.to_le_bytes());
    corrupt.push(bad_tokens);
    let mut bad_count = valid.clone();
    bad_count[RECORD_OVERHEAD_BYTES - 4..RECORD_OVERHEAD_BYTES].copy_from_slice(&u32::MAX.to_le_bytes());
    corrupt.push(bad_count);
    let mut truncated = valid.clone();
    truncated.pop();
    corrupt.push(truncated);
    let mut trailing = valid.clone();
    trailing.push(0);
    corrupt.push(trailing);
    let mut bad_magic = valid.clone();
    bad_magic[0] ^= 1;
    corrupt.push(bad_magic);
    for bytes in corrupt {
        // A wider delivery cap lets the codec inspect trailing data too. It
        // cannot turn unauthenticated transport damage into Panel/Miner guilt.
        let delivery_cap = RecordLimits { max_record_bytes: valid.len() as u64 + 1, ..cap };
        assert!(deliver(&prepared, &witness, &bytes, delivery_cap).is_err());
        assert_eq!(prior_progress.snapshot(), prior_snapshot);
        assert_eq!(prior_progress.charged_wire_bytes(), prior_wire);
        assert_eq!(prior_progress.charged_signature_bytes(), prior_signature);
    }
    // Both a fresh delivery and the previously started checker remain usable.
    let retried = deliver(&prepared, &witness, &valid, cap).unwrap();
    finish_signed(&f, &retried, &witness);
    for frame in &frames[1..] {
        submit_signed(&prior_context, &mut prior_progress, frame).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(&prior_context, prior_progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn generated_storage_and_encoded_record_caps_are_checked_without_consuming_prepared_state() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let witness = f.witness(&result, prepared.descriptor());
    let valid = record_bytes(&f, &result);
    let cap = record_limits(&valid, result.decode);
    let too_small = RecordLimits { max_generated_heap_bytes: u64::from(result.decode) * 4 - 1, ..cap };
    assert!(matches!(deliver(&prepared, &witness, &valid, too_small), Err(RecordDeliveryError::Budget)));
    let short_wire = RecordLimits { max_record_bytes: valid.len() as u64 - 1, ..cap };
    assert!(matches!(
        deliver(&prepared, &witness, &valid, short_wire),
        Err(RecordDeliveryError::Budget | RecordDeliveryError::Contract(ContractError::Budget))
    ));
    let retried = deliver(&prepared, &witness, &valid, cap).unwrap();
    finish_signed(&f, &retried, &witness);
}

#[test]
fn failed_panel_authentication_during_delivery_does_not_poison_a_valid_retry() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let witness = f.witness(&result, prepared.descriptor());
    let valid = record_bytes(&f, &result);
    let cap = record_limits(&valid, result.decode);
    let mut bad_signature = sign_witness(&witness);
    bad_signature[0] ^= 1;
    assert!(prepared.authenticate_record(witness.clone(), &valid, cap, &panel_key(), &bad_signature).is_err());
    assert!(prepared.authenticate_record(witness.clone(), &valid, cap, &miner_key(), &sign_witness(&witness)).is_err());
    let mut false_witness = witness.clone();
    false_witness.candidate_output = d(99);
    assert!(prepared.authenticate_record(false_witness.clone(), &valid, cap, &panel_key(), &sign_witness(&false_witness)).is_err());
    let retried = deliver(&prepared, &witness, &valid, cap).unwrap();
    finish_signed(&f, &retried, &witness);
}
