//! The actual A0 NodeChecker port with separately signed Miner/Panel records.
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
    tir::{EXACT_PART_HEADER_BYTES, ExactPart, greedy_selector_id, provenance_id},
    wire::*,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1,
        positive_check::{CheckError, CheckLimits, CheckOutcome},
        positive_contract::{AdapterError, CheckContext, ContractProgress, PreparedCheck, TirNodeChecker},
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
    fn context(&self, result: &MinerResultCommit, limits: CheckLimits) -> (CheckContext, PanelWitnessCommit) {
        let prepared = self.prepare(result, limits);
        let witness = self.witness(result, prepared.descriptor());
        let context = prepared
            .authenticate_witness(witness.clone(), self.binding.clone(), self.generated.clone(), &panel_key(), &sign_witness(&witness))
            .unwrap();
        (context, witness)
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
fn submit(context: &CheckContext, progress: &mut ContractProgress, bytes: &[u8]) -> Result<(), AdapterError> {
    let descriptor = TirNodeChecker.derive_plan(context)?;
    TirNodeChecker.check_part(context, progress, EvidenceFrame::decode(bytes, descriptor.max_part_bytes)?)
}

#[test]
fn signed_role_commits_and_framed_parts_reach_only_a_checked_terminal_fact() {
    let f = fixture();
    for mismatch in [false, true] {
        let result = if mismatch { f.result(d(99)) } else { f.correct_result() };
        let prepared = f.prepare(&result, limits());
        assert_eq!(prepared.reserved_wire_bytes(), prepared.descriptor().mandatory_relations * limits().max_part_bytes);
        let (context, witness) = f.context(&result, limits());
        let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
        let frames = f.frames(&witness, descriptor);
        let mut progress = context.start().unwrap();
        for frame in &frames {
            submit(&context, &mut progress, frame).unwrap();
        }
        assert_eq!(progress.charged_wire_bytes(), frames.iter().map(|p| p.len() as u64).sum::<u64>());
        let terminal = TirNodeChecker.finish(&context, progress).unwrap();
        assert_eq!(terminal.outcome(), if mismatch { CheckOutcome::OutputMismatch } else { CheckOutcome::Matched });
        assert_eq!(terminal.result(), result.id());
        assert_eq!(terminal.witness(), witness.id());
        assert_eq!(terminal.descriptor(), descriptor);
    }
}

#[test]
fn bad_actor_signatures_and_signed_false_witness_metadata_are_rejected() {
    let f = fixture();
    let result = f.correct_result();
    let mut bad = sign_result(&result);
    bad[0] ^= 1;
    assert!(matches!(
        PreparedCheck::authenticate(
            &rooted(&result),
            &f.binding.class,
            f.binding.artifact_root,
            &f.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            limits(),
            result.clone(),
            &miner_key(),
            &bad
        ),
        Err(AdapterError::Contract(ContractError::Signature))
    ));
    for variant in 0..4 {
        let prepared = f.prepare(&result, limits());
        let mut witness = f.witness(&result, prepared.descriptor());
        if variant == 1 {
            witness.execution_record = d(99);
        }
        if variant == 2 {
            witness.candidate_output = d(99);
        }
        if variant == 3 {
            witness.provenance = d(99);
        }
        let mut signature = sign_witness(&witness);
        if variant == 0 {
            signature[0] ^= 1;
        }
        let error = prepared.authenticate_witness(witness, f.binding.clone(), f.generated.clone(), &panel_key(), &signature);
        assert!(if variant == 0 {
            matches!(error, Err(AdapterError::Contract(ContractError::Signature)))
        } else {
            matches!(error, Err(AdapterError::Binding))
        });
    }
}

#[test]
fn signed_false_generated_ids_cannot_become_a_miner_output_mismatch() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let mut witness = f.witness(&result, prepared.descriptor());
    let false_ids = vec![3, 3];
    witness.candidate_output = native(palw_attempt_output_root_v1(&f.binding.job_context, &false_ids));
    let context =
        prepared.authenticate_witness(witness.clone(), f.binding.clone(), false_ids, &panel_key(), &sign_witness(&witness)).unwrap();
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let mut progress = context.start().unwrap();
    let error = f.frames(&witness, descriptor).iter().find_map(|frame| submit(&context, &mut progress, frame).err());
    assert!(matches!(error, Some(AdapterError::Check(CheckError::Binding | CheckError::PanelEvidence))));
    assert!(matches!(TirNodeChecker.finish(&context, progress), Err(AdapterError::Closed)));
}

#[test]
fn an_omitted_terminal_part_remains_incomplete_through_the_shared_port() {
    let f = fixture();
    let (context, witness) = f.context(&f.correct_result(), limits());
    let frames = f.frames(&witness, TirNodeChecker.derive_plan(&context).unwrap());
    let mut progress = context.start().unwrap();
    for frame in &frames[..frames.len() - 1] {
        submit(&context, &mut progress, frame).unwrap();
    }
    assert!(matches!(TirNodeChecker.finish(&context, progress), Err(AdapterError::Check(CheckError::Incomplete))));
}

#[test]
fn foreign_frame_identity_does_not_poison_the_target_candidates_progress() {
    let f = fixture();
    let (context, witness) = f.context(&f.correct_result(), limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let frames = f.frames(&witness, descriptor);
    let original = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
    let mut header = original.header();
    header.case = CaseId(d(99));
    let foreign =
        EvidenceFrame::new(header, original.payload(), descriptor.max_part_bytes).unwrap().encode(descriptor.max_part_bytes).unwrap();
    let mut progress = context.start().unwrap();
    assert_eq!(submit(&context, &mut progress, &foreign), Err(AdapterError::Contract(ContractError::Identity)));
    assert_eq!(progress.snapshot().next_leaf, 0);
    assert_eq!(progress.charged_wire_bytes(), 0);
    for frame in &frames {
        submit(&context, &mut progress, frame).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn identical_public_ids_do_not_allow_progress_to_switch_local_resource_limits() {
    let f = fixture();
    let result = f.correct_result();
    let (context, witness) = f.context(&result, limits());
    let mut other_limits = limits();
    other_limits.max_part_heap += 1;
    let (other_context, other_witness) = f.context(&result, other_limits);
    assert_eq!(witness.id(), other_witness.id());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    assert_eq!(descriptor, TirNodeChecker.derive_plan(&other_context).unwrap());
    let frames = f.frames(&witness, descriptor);
    let mut progress = context.start().unwrap();
    assert_eq!(submit(&other_context, &mut progress, &frames[0]), Err(AdapterError::Contract(ContractError::Identity)));
    assert_eq!(progress.charged_wire_bytes(), 0);
    for frame in &frames {
        submit(&context, &mut progress, frame).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn bounded_outer_frames_do_not_authorize_malformed_inner_length_prefixes() {
    let f = fixture();
    let (context, witness) = f.context(&f.correct_result(), limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let (cone, trace) = &f.parts[0];
    let valid = ExactPart::new(cone, trace.as_deref(), descriptor.max_part_bytes - EVIDENCE_HEADER_BYTES as u64).unwrap().encode();
    let mut wrong_magic = valid.clone();
    wrong_magic[0] ^= 1;
    let mut oversized_cone = valid.clone();
    oversized_cone[8..12].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut extra = valid.clone();
    extra.push(0);
    let mut truncated = valid.clone();
    truncated.pop();
    for payload in [wrong_magic, oversized_cone, vec![0, 0, 1], extra, truncated] {
        let frame = frame_bytes(&witness, descriptor, 0, &payload);
        let mut progress = context.start().unwrap();
        assert!(matches!(
            submit(&context, &mut progress, &frame),
            Err(AdapterError::Contract(ContractError::Version | ContractError::NonCanonical | ContractError::Budget))
        ));
        assert_eq!(progress.charged_wire_bytes(), frame.len() as u64);
        assert_eq!(progress.snapshot().next_leaf, 0);
        assert_eq!(progress.snapshot().outcome, CheckOutcome::WitnessRejected);
        assert!(matches!(TirNodeChecker.finish(&context, progress), Err(AdapterError::Closed)));
    }
}

#[test]
fn admission_reserves_every_outer_header_and_both_payload_length_prefixes() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let descriptor = prepared.descriptor();
    let witness = f.witness(&result, descriptor);
    // Measure actual private framing on one byte, independently of the
    // reservation implementation: 284 outer bytes plus the 16-byte G14 annex.
    let payload = ExactPart::new(&[0x17], None, descriptor.max_part_bytes - EVIDENCE_HEADER_BYTES as u64).unwrap().encode();
    let encoded = frame_bytes(&witness, descriptor, 0, &payload);
    let measured_overhead = encoded.len() as u64 - 1;
    assert_eq!(EXACT_PART_HEADER_BYTES, 16);
    assert_eq!(measured_overhead, 300);
    let required = descriptor.mandatory_relations * limits().max_part_bytes;
    assert_eq!(prepared.reserved_wire_bytes(), required);
    let mut short = limits();
    short.max_total_bytes = required - 1;
    let payload_only = descriptor.mandatory_relations * (limits().max_part_bytes - measured_overhead);
    assert!(payload_only <= short.max_total_bytes, "the inner payload budget alone would fit");
    let authenticate = |cap| {
        PreparedCheck::authenticate(
            &rooted(&result),
            &f.binding.class,
            f.binding.artifact_root,
            &f.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            cap,
            result.clone(),
            &miner_key(),
            &sign_result(&result),
        )
    };
    assert!(matches!(
        authenticate(short),
        Err(AdapterError::Check(CheckError::Budget) | AdapterError::Contract(ContractError::Budget))
    ));
    for outer_part_bytes in [measured_overhead - 1, measured_overhead] {
        let mut cap = limits();
        cap.max_part_bytes = outer_part_bytes;
        assert!(matches!(
            authenticate(cap),
            Err(AdapterError::Check(CheckError::Budget) | AdapterError::Contract(ContractError::Budget))
        ));
    }
}

#[test]
fn a_signed_alternate_selector_cannot_replace_the_rooted_greedy_rule() {
    let f = fixture();
    let mut result = f.correct_result();
    result.selector = d(99);
    let mut rooted_greedy = rooted(&result);
    rooted_greedy.selector = greedy_selector_id();
    let authenticate = |admission: &AdmissionContext| {
        PreparedCheck::authenticate(
            admission,
            &f.binding.class,
            f.binding.artifact_root,
            &f.binding.job_context,
            PalwPromptIdsFormV1::Flat,
            limits(),
            result.clone(),
            &miner_key(),
            &sign_result(&result),
        )
    };
    // A valid Miner signature does not change the selector fixed by admission.
    assert!(matches!(authenticate(&rooted_greedy), Err(AdapterError::Contract(ContractError::Scope))));
    // Even an otherwise self-consistent non-greedy context is outside this exact
    // adapter's explicit semantics, rather than grounds to refute the Miner.
    assert!(matches!(authenticate(&rooted(&result)), Err(AdapterError::Check(CheckError::UnsupportedProfile))));
}

fn sign_frame(frame: EvidenceFrame<'_>) -> [u8; SIGNATURE_BYTES] {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    *libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32]).unwrap().as_ref()
}

#[test]
fn unauthenticated_or_foreign_packets_cannot_poison_an_honest_panel_candidate() {
    let f = fixture();
    let (context, witness) = f.context(&f.correct_result(), limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let frames = f.frames(&witness, descriptor);
    let first = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
    let original_signature = sign_frame(first);
    let mut progress = context.start().unwrap();
    // All three attacks arrive under the honest candidate's exact frame IDs.
    let mut bad_signature = original_signature;
    bad_signature[0] ^= 1;
    assert_eq!(
        TirNodeChecker.check_signed_part(&context, &mut progress, first, &panel_key(), &bad_signature),
        Err(AdapterError::Contract(ContractError::Signature))
    );
    let mut altered = frames[0].clone();
    *altered.last_mut().unwrap() ^= 1;
    let altered = EvidenceFrame::decode(&altered, descriptor.max_part_bytes).unwrap();
    assert_eq!(
        TirNodeChecker.check_signed_part(&context, &mut progress, altered, &panel_key(), &original_signature),
        Err(AdapterError::Contract(ContractError::Signature))
    );
    let foreign_key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93; 32]);
    let foreign_signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&foreign_key.signing_key, &first.id().0.0, EVIDENCE_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    assert_eq!(
        TirNodeChecker.check_signed_part(&context, &mut progress, first, foreign_key.verification_key.as_ref(), &foreign_signature),
        Err(AdapterError::Contract(ContractError::Identity))
    );
    assert_eq!(progress.snapshot().next_leaf, 0);
    assert_eq!(progress.snapshot().outcome, CheckOutcome::Checking);
    assert_eq!(progress.charged_wire_bytes(), 0);
    assert_eq!(progress.charged_signature_bytes(), 0);
    assert_eq!(progress.charged_transport_bytes().unwrap(), 0);
    // The legitimate author can still complete the exact same candidate.
    for bytes in &frames {
        let frame = EvidenceFrame::decode(bytes, descriptor.max_part_bytes).unwrap();
        TirNodeChecker.check_signed_part(&context, &mut progress, frame, &panel_key(), &sign_frame(frame)).unwrap();
    }
    assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn author_authenticated_frames_can_be_relayed_and_charge_all_packet_signatures() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let prepared_quote = prepared.signed_transport_quote().unwrap();
    let (context, witness) = f.context(&result, limits());
    let quote = context.signed_transport_quote().unwrap();
    assert_eq!(quote.max_packet_bytes, prepared_quote.max_packet_bytes);
    assert_eq!(quote.total_packet_bytes, prepared_quote.total_packet_bytes);
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let frames = f.frames(&witness, descriptor);
    let mut progress = context.start().unwrap();
    for bytes in &frames {
        let frame = EvidenceFrame::decode(bytes, descriptor.max_part_bytes).unwrap();
        // A relay carries an already authenticated author's packet. It cannot
        // replace the payload or acquire arithmetic authorship from this token.
        let authenticated = frame.authenticate(&witness, &panel_key(), &sign_frame(frame), quote.max_packet_bytes).unwrap();
        TirNodeChecker.check_authenticated_part(&context, &mut progress, authenticated).unwrap();
    }
    let raw_bytes = frames.iter().map(|f| f.len() as u64).sum::<u64>();
    let signature_bytes = frames.len() as u64 * SIGNATURE_BYTES as u64;
    assert_eq!(progress.charged_wire_bytes(), raw_bytes);
    assert_eq!(progress.charged_signature_bytes(), signature_bytes);
    assert_eq!(progress.charged_transport_bytes().unwrap(), raw_bytes + signature_bytes);
    assert!(raw_bytes + signature_bytes <= quote.total_packet_bytes);
    assert_eq!(TirNodeChecker.finish(&context, progress).unwrap().outcome(), CheckOutcome::Matched);
}

#[test]
fn author_signed_malformed_material_charges_the_panel_packet_and_rejects_only_its_witness() {
    let f = fixture();
    let (context, witness) = f.context(&f.correct_result(), limits());
    let descriptor = TirNodeChecker.derive_plan(&context).unwrap();
    let frame_bytes = frame_bytes(&witness, descriptor, 0, &[0; EXACT_PART_HEADER_BYTES]);
    let frame = EvidenceFrame::decode(&frame_bytes, descriptor.max_part_bytes).unwrap();
    let mut progress = context.start().unwrap();
    assert_eq!(
        TirNodeChecker.check_signed_part(&context, &mut progress, frame, &panel_key(), &sign_frame(frame)),
        Err(AdapterError::Contract(ContractError::Version))
    );
    assert_eq!(progress.snapshot().next_leaf, 0);
    assert_eq!(progress.snapshot().outcome, CheckOutcome::WitnessRejected);
    assert_eq!(progress.charged_wire_bytes(), frame_bytes.len() as u64);
    assert_eq!(progress.charged_signature_bytes(), SIGNATURE_BYTES as u64);
    assert_eq!(progress.charged_transport_bytes().unwrap(), frame.signed_wire_bytes());
    assert!(matches!(TirNodeChecker.finish(&context, progress), Err(AdapterError::Closed)));
}

#[test]
fn signed_transport_reservation_includes_every_signature_and_refuses_aggregate_overflow() {
    let f = fixture();
    let result = f.correct_result();
    let prepared = f.prepare(&result, limits());
    let relations = prepared.descriptor().mandatory_relations;
    let quote = prepared.signed_transport_quote().unwrap();
    assert_eq!(quote.max_packet_bytes, limits().max_part_bytes + SIGNATURE_BYTES as u64);
    assert_eq!(quote.unsigned_frame_bytes, relations * limits().max_part_bytes);
    assert_eq!(quote.signature_bytes, relations * SIGNATURE_BYTES as u64);
    assert_eq!(quote.total_packet_bytes, relations * (limits().max_part_bytes + SIGNATURE_BYTES as u64));
    let mut near_overflow = limits();
    near_overflow.max_part_bytes = u64::MAX / relations;
    near_overflow.max_total_bytes = u64::MAX;
    // The raw G14 codec is capped at u32 payloads, so preparation itself remains
    // finite. The larger admitted packet allowance still must reserve its
    // signatures without wrapping its aggregate transport quotation.
    let prepared = f.prepare(&result, near_overflow);
    assert!(matches!(
        prepared.signed_transport_quote(),
        Err(AdapterError::Check(CheckError::Budget) | AdapterError::Contract(ContractError::Budget))
    ));
}
