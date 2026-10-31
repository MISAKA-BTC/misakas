//! A4 integration against the real A1 checker and separately authenticated carrier.
//! The tiny fixture is adapted from A1; expected logits/tokens are scalar constants.
//! Evidence bytes use A0's Panel-author signature helper; duty authority is an explicit test fixture.
#![cfg(feature = "node")]
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
    identity::*,
    messages::*,
    tir::{ExactPart, greedy_selector_id, provenance_id},
    wire::*,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1, TirEvidenceV1, TirHeldInventoryV1, TirParamsSourceV1, positive_check::CheckLimits,
        positive_contract::PreparedCheck, positive_court_adapter::TirCourtCheckerV2,
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
    fixture_for_job(None)
}
fn fixture_for_job(job_id: Option<Hash64>) -> Fixture {
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
    if let Some(job_id) = job_id {
        job.job_id = job_id;
    }
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

use kaspa_consensus_core::palw_positive_court_v2::coordinator::contract::{ContractCourtV2, CourtLimitsV2, TerminalKindV2};

use kaspa_consensus_core::palw_positive_court_v2::coordinator::carrier::{
    PanelFrameAuthorityV2, PanelPacketSignatureV2, PanelSignatureVerifierV2,
};

// An explicit rooted assignment fixture, not a native duty/credit authority.
// Cryptographic byte authentication itself uses the production A0/A4 adapter.
struct FrameDuty {
    scope: ChainScope,
    case: CaseId,
    result: ResultId,
    author: PrincipalId,
    max_signed_bytes: u64,
}
impl PanelFrameAuthorityV2 for FrameDuty {
    type Error = &'static str;
    fn authorize(&self, result: &MinerResultCommit, witness: &PanelWitnessCommit) -> Result<u64, Self::Error> {
        if result.scope != self.scope
            || result.case_id() != self.case
            || result.id() != self.result
            || witness.scope != self.scope
            || witness.case != self.case
            || witness.result != self.result
            || witness.author != self.author
        {
            return Err("unassigned frame duty");
        }
        Ok(self.max_signed_bytes)
    }
}
type ActualVerifier = PanelSignatureVerifierV2<FrameDuty>;
fn verifier_for(result: &MinerResultCommit) -> ActualVerifier {
    verifier_with_limit(result, court_limits().max_part_bytes + SIGNATURE_BYTES as u64)
}
fn verifier_with_limit(result: &MinerResultCommit, max_signed_bytes: u64) -> ActualVerifier {
    PanelSignatureVerifierV2::new(FrameDuty {
        scope: result.scope,
        case: result.case_id(),
        result: result.id(),
        author: principal(&panel_key()),
        max_signed_bytes,
    })
}
fn packet_signature(signature: [u8; SIGNATURE_BYTES]) -> PanelPacketSignatureV2 {
    PanelPacketSignatureV2 { key: panel_key(), signature }
}
fn carrier_signature(frame: &EvidenceFrame<'_>) -> PanelPacketSignatureV2 {
    let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([92; 32]);
    packet_signature(
        *libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key, &frame.id().0.0, EVIDENCE_SIGN_CONTEXT, [4; 32]).unwrap().as_ref(),
    )
}
fn signed_bytes(encoded_frame: &[u8]) -> u64 {
    encoded_frame.len() as u64 + SIGNATURE_BYTES as u64
}
fn service(result: &MinerResultCommit) -> MinerServiceContract {
    MinerServiceContract {
        version: 2,
        revision: 1,
        case: result.case_id(),
        source_catalog: d(50),
        provider_delegations: d(51),
        budget: AggregateBudget {
            rounds: 8,
            unique_source_bytes: 1 << 20,
            transmitted_bytes: 1 << 32,
            additional_work: 1 << 44,
            retained_bytes: 1 << 24,
            retention_until: 120,
            fee: 100,
            witness_attempts: 2,
        },
        funding_consent: d(52),
        deadline: 100,
    }
}
fn court_limits() -> CourtLimitsV2 {
    CourtLimitsV2 {
        max_attempts: 2,
        max_total_wire_bytes: 1 << 32,
        max_part_bytes: 1 << 20,
        max_receipt_bytes: 1 << 20,
        max_reserved_elements: 1 << 44,
        max_reserved_terms: 1 << 44,
        deadline: 100,
    }
}
fn court_result(f: &Fixture, mismatch: bool) -> (MinerResultCommit, MinerServiceContract) {
    let mut result = if mismatch { f.result(d(99)) } else { f.correct_result() };
    let service = service(&result);
    result.service_contract = service.id();
    (result, service)
}
fn court(result: MinerResultCommit, service: MinerServiceContract) -> ContractCourtV2<TirCourtCheckerV2, ActualVerifier> {
    let verifier = verifier_for(&result);
    court_with_verifier(result, service, verifier)
}
fn court_with_verifier(
    result: MinerResultCommit,
    service: MinerServiceContract,
    verifier: ActualVerifier,
) -> ContractCourtV2<TirCourtCheckerV2, ActualVerifier> {
    let signature = sign_result(&result);
    ContractCourtV2::new(TirCourtCheckerV2, verifier, &rooted(&result), result, &miner_key(), &signature, service, court_limits(), 1)
        .unwrap()
}
fn open(
    court: &mut ContractCourtV2<TirCourtCheckerV2, ActualVerifier>,
    f: &Fixture,
    result: &MinerResultCommit,
    attempt: u32,
) -> (PanelWitnessCommit, CoverageDescriptor, Vec<Vec<u8>>) {
    open_at(court, f, result, attempt, 2 + 2 * attempt as u64)
}
fn open_at(
    court: &mut ContractCourtV2<TirCourtCheckerV2, ActualVerifier>,
    f: &Fixture,
    result: &MinerResultCommit,
    attempt: u32,
    now: u64,
) -> (PanelWitnessCommit, CoverageDescriptor, Vec<Vec<u8>>) {
    let prepared = f.prepare(result, limits());
    let descriptor = prepared.descriptor();
    let mut witness = f.witness(result, descriptor);
    witness.attempt = attempt;
    let signature = sign_witness(&witness);
    let context =
        prepared.authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &signature).unwrap();
    court.open_candidate(witness.clone(), &panel_key(), &signature, context, now).unwrap();
    let frames = f.frames(&witness, descriptor);
    (witness, descriptor, frames)
}
fn send(court: &mut ContractCourtV2<TirCourtCheckerV2, ActualVerifier>, bytes: &[u8], cap: u64, now: u64) {
    let frame = EvidenceFrame::decode(bytes, cap).unwrap();
    let authorization = carrier_signature(&frame);
    court.submit_frame(frame, &authorization, now).unwrap();
}

#[test]
fn actual_checker_finishes_signed_result_only_after_all_required_relations() {
    let f = fixture();
    for mismatch in [false, true] {
        let (result, service) = court_result(&f, mismatch);
        let mut court = court(result.clone(), service);
        let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
        for bytes in &frames[..frames.len() - 1] {
            send(&mut court, bytes, descriptor.max_part_bytes, 3);
        }
        assert_eq!(court.status(), ClaimStatus::Pending);
        assert!(court.finish(4).is_err(), "a correct prefix is not a completed Court");
        send(&mut court, frames.last().unwrap(), descriptor.max_part_bytes, 5);
        let fact = court.finish(6).unwrap();
        assert_eq!(fact.result(), result.id());
        assert_eq!(fact.terminal_kind(), if mismatch { TerminalKindV2::ClaimRefuted } else { TerminalKindV2::PositivePassed });
        assert_eq!(court.status(), if mismatch { ClaimStatus::ClaimRefuted } else { ClaimStatus::PositivePassed });
        assert!(court.finish(7).is_err(), "a completed Court cannot produce a second terminal fact");
    }
}

#[test]
fn unauthenticated_self_consistent_bad_material_cannot_poison_or_charge_an_honest_candidate() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (witness, descriptor, frames) = open(&mut court, &f, &result, 0);
    let forged = frame_bytes(&witness, descriptor, 0, &[0]);
    let frame = EvidenceFrame::decode(&forged, descriptor.max_part_bytes).unwrap();
    assert!(court.submit_frame(frame, &packet_signature([0; SIGNATURE_BYTES]), 3).is_err());
    assert_eq!(court.cursor(), 0);
    assert_eq!(court.charged_wire_bytes(), 0);
    assert_eq!(court.attempts(), 1);
    assert_eq!(court.status(), ClaimStatus::Pending);
    // A genuine witness signature is still not a valid carrier signature.
    let original = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
    assert!(court.submit_frame(original, &packet_signature(sign_witness(&witness)), 3).is_err());
    assert_eq!(court.charged_wire_bytes(), 0);
    for frame in frames {
        send(&mut court, &frame, descriptor.max_part_bytes, 4);
    }
    assert_eq!(court.finish(5).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}

#[test]
fn self_signed_unassigned_prover_cannot_spend_attempts_or_replace_the_active_candidate() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let unassigned = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([93; 32]);
    let unassigned_key = *unassigned.verification_key.as_ref();
    let prepared = f.prepare(&result, limits());
    let mut witness = f.witness(&result, prepared.descriptor());
    witness.author = principal(&unassigned_key);
    let signature =
        *libcrux_ml_dsa::ml_dsa_87::sign(&unassigned.signing_key, &witness.id().0.0, WITNESS_SIGN_CONTEXT, [0; 32]).unwrap().as_ref();
    // Both literal signature and actual A1 context are valid. Only the rooted
    // duty fixture excludes this self-nominated author; no Gate C draw is an
    // opening prerequisite for the independently assigned honest prover.
    witness.authenticate(&result, &unassigned_key, &signature).unwrap();
    let context =
        prepared.authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &unassigned_key, &signature).unwrap();
    assert!(court.open_candidate(witness.clone(), &unassigned_key, &signature, context, 2).is_err());
    assert_eq!(court.attempts(), 0);
    assert_eq!(court.last_attempt(), None);
    assert_eq!(court.witness_status(), None);
    assert_eq!(court.cursor(), 0);
    assert_eq!(court.charged_wire_bytes(), 0);
    assert_eq!(court.charged_elements(), 0);
    assert_eq!(court.charged_terms(), 0);
    assert_eq!(court.status(), ClaimStatus::Pending);

    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    send(&mut court, &frames[0], descriptor.max_part_bytes, 3);
    let before = (
        court.last_attempt(),
        court.attempts(),
        court.cursor(),
        court.charged_wire_bytes(),
        court.charged_elements(),
        court.charged_terms(),
    );
    let context = f
        .prepare(&result, limits())
        .authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &unassigned_key, &signature)
        .unwrap();
    assert!(court.open_candidate(witness, &unassigned_key, &signature, context, 4).is_err());
    assert_eq!(
        (
            court.last_attempt(),
            court.attempts(),
            court.cursor(),
            court.charged_wire_bytes(),
            court.charged_elements(),
            court.charged_terms()
        ),
        before
    );
    assert_eq!(court.witness_status(), Some(WitnessStatus::Checking));
    for frame in &frames[1..] {
        send(&mut court, frame, descriptor.max_part_bytes, 5);
    }
    assert_eq!(court.finish(6).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}

#[test]
fn authenticated_bad_panel_material_does_not_refute_miner_or_reset_attempt_and_wire_budgets() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (witness, descriptor, frames) = open(&mut court, &f, &result, 0);
    send(&mut court, &frames[0], descriptor.max_part_bytes, 3);
    // A distinct signed packet is not made free merely by sharing the case,
    // witness and next relation with an accepted history.
    let bad = frame_bytes(&witness, descriptor, 1, &[0]);
    let frame = EvidenceFrame::decode(&bad, descriptor.max_part_bytes).unwrap();
    let signature = carrier_signature(&frame);
    assert!(court.submit_frame(frame, &signature, 3).is_err());
    assert_eq!(court.status(), ClaimStatus::Pending);
    assert_eq!(court.witness_status(), Some(WitnessStatus::Rejected));
    assert_eq!(court.cursor(), 1);
    let charged = court.charged_wire_bytes();
    assert_eq!(charged, signed_bytes(&frames[0]) + signed_bytes(&bad));
    let (_, next, frames) = open(&mut court, &f, &result, 1);
    assert_eq!(court.attempts(), 2);
    assert_eq!(court.charged_wire_bytes(), charged);
    for frame in frames {
        send(&mut court, &frame, next.max_part_bytes, 4);
    }
    assert_eq!(court.finish(5).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
    assert!(court.charged_wire_bytes() > charged);
}

#[test]
fn candidate_authority_requires_a_finite_usable_signed_packet_limit() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let minimum = EVIDENCE_HEADER_BYTES as u64 + SIGNATURE_BYTES as u64;
    for unusable in [0, u64::MAX, minimum - 1] {
        let mut court = court_with_verifier(result.clone(), service.clone(), verifier_with_limit(&result, unusable));
        let prepared = f.prepare(&result, limits());
        let witness = f.witness(&result, prepared.descriptor());
        let signature = sign_witness(&witness);
        let context =
            prepared.authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &signature).unwrap();
        assert!(court.open_candidate(witness, &panel_key(), &signature, context, 2).is_err(), "limit {unusable}");
        assert_eq!(court.attempts(), 0);
        assert_eq!(court.last_attempt(), None);
        assert_eq!(court.witness_status(), None);
        assert_eq!(court.reserved_receipt_bytes(), 0);
        assert_eq!(court.cursor(), 0);
        assert_eq!(court.charged_wire_bytes(), 0);
        assert_eq!(court.charged_elements(), 0);
        assert_eq!(court.charged_terms(), 0);
        assert_eq!(court.status(), ClaimStatus::Pending);
    }
}

#[test]
fn authenticated_noncanonical_relation_count_is_refused_before_case_charging() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    let canonical = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
    let mut header = canonical.header();
    header.relation_count = 2;
    let malformed = EvidenceFrame::new(header, canonical.payload(), descriptor.max_part_bytes).unwrap();
    let before = (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms());
    let signature = carrier_signature(&malformed);
    assert!(court.submit_frame(malformed, &signature, 3).is_err());
    assert_eq!(
        (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms()),
        before
    );
    assert_eq!(court.cursor(), 0);
    assert_eq!(court.attempts(), 1);
    assert_eq!(court.status(), ClaimStatus::Pending);
    // The syntactic delivery refusal neither proves a Miner fault nor poisons
    // the actual checker. Canonical material can still complete this candidate.
    for bytes in &frames {
        send(&mut court, bytes, descriptor.max_part_bytes, 4);
    }
    assert_eq!(court.charged_wire_bytes(), frames.iter().map(|p| signed_bytes(p)).sum::<u64>());
    assert_eq!(court.finish(5).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}

#[test]
fn repeated_authenticated_future_relation_cannot_exhaust_the_case_and_is_usable_later() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    assert!(frames.len() > 2);
    let future = EvidenceFrame::decode(&frames[1], descriptor.max_part_bytes).unwrap();
    let signature = carrier_signature(&future);
    let before = (
        court.last_attempt(),
        court.attempts(),
        court.cursor(),
        court.charged_wire_bytes(),
        court.charged_elements(),
        court.charged_terms(),
        court.reserved_receipt_bytes(),
    );
    for now in 3..=7 {
        let future = EvidenceFrame::decode(&frames[1], descriptor.max_part_bytes).unwrap();
        assert!(court.submit_frame(future, &signature, now).is_err());
        assert_eq!(
            (
                court.last_attempt(),
                court.attempts(),
                court.cursor(),
                court.charged_wire_bytes(),
                court.charged_elements(),
                court.charged_terms(),
                court.reserved_receipt_bytes(),
            ),
            before
        );
        assert_eq!(court.status(), ClaimStatus::Pending);
        assert_eq!(court.witness_status(), Some(WitnessStatus::Checking));
    }
    // Rejected deliveries cannot commit their supplied clock either. The last
    // committed native time remains the opening at two, so an explicit trusted
    // sweep at that same time must still succeed after those refused packets.
    court.expire(2).unwrap();
    // Refusing delivery did not accept, consume or permanently blacklist this
    // exact signed packet. Once relation zero is checked, the same bytes and
    // same signature can supply the next required relation exactly once.
    send(&mut court, &frames[0], descriptor.max_part_bytes, 8);
    let future = EvidenceFrame::decode(&frames[1], descriptor.max_part_bytes).unwrap();
    court.submit_frame(future, &signature, 9).unwrap();
    assert_eq!(court.cursor(), 2);
    assert_eq!(court.charged_wire_bytes(), signed_bytes(&frames[0]) + signed_bytes(&frames[1]));
    assert_eq!(court.attempts(), 1);
    for bytes in &frames[2..] {
        send(&mut court, bytes, descriptor.max_part_bytes, 10);
    }
    assert_eq!(court.charged_wire_bytes(), frames.iter().map(|p| signed_bytes(p)).sum::<u64>());
    assert_eq!(court.finish(11).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}

#[test]
fn exclusive_deadline_rejects_final_frame_before_any_success_fact() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    for frame in &frames[..frames.len() - 1] {
        send(&mut court, frame, descriptor.max_part_bytes, 99);
    }
    let before = court.charged_wire_bytes();
    let last = EvidenceFrame::decode(frames.last().unwrap(), descriptor.max_part_bytes).unwrap();
    let signature = carrier_signature(&last);
    assert!(court.submit_frame(last, &signature, 100).is_err());
    assert_eq!(court.charged_wire_bytes(), before);
    assert_eq!(court.status(), ClaimStatus::Pending, "a refused packet does not supply the trusted expiry clock");
    court.expire(100).unwrap();
    assert_eq!(court.status(), ClaimStatus::EndedUnverified);
    assert!(court.finish(100).is_err());
}

#[test]
fn signed_result_cannot_replace_rooted_input_even_with_its_own_valid_signature() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let expected = rooted(&result);
    let mut foreign = result.clone();
    foreign.input = d(87);
    let signature = sign_result(&foreign);
    assert!(
        ContractCourtV2::new(
            TirCourtCheckerV2,
            verifier_for(&foreign),
            &expected,
            foreign,
            &miner_key(),
            &signature,
            service,
            court_limits(),
            1,
        )
        .is_err()
    );
}

#[test]
fn accepted_packet_replays_are_receipt_bound_and_never_spend_budget_or_mint_coverage() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
    send(&mut court, &frames[0], descriptor.max_part_bytes, 3);
    let before = (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms());
    for now in 4..=6 {
        send(&mut court, &frames[0], descriptor.max_part_bytes, now);
        assert_eq!(court.cursor(), 1);
        assert_eq!(court.charged_wire_bytes(), signed_bytes(&frames[0]));
        assert_eq!(
            (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms()),
            before
        );
        assert_eq!(court.status(), ClaimStatus::Pending);
    }
    // Receipt lookup does not turn an unauthenticated delivery into a valid
    // replay just because its unsigned bytes match an earlier accepted frame.
    let first = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
    assert!(court.submit_frame(first, &packet_signature([0; SIGNATURE_BYTES]), 6).is_err());
    assert_eq!(
        (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms()),
        before
    );
    assert!(court.finish(7).is_err());

    // A receipt must cover the whole accepted prefix, not only the last ACK.
    assert!(frames.len() > 2);
    send(&mut court, &frames[1], descriptor.max_part_bytes, 8);
    let after_second = (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms());
    send(&mut court, &frames[0], descriptor.max_part_bytes, 9);
    assert_eq!(
        (court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms()),
        after_second
    );
    for bytes in &frames[2..] {
        send(&mut court, bytes, descriptor.max_part_bytes, 10);
    }
    assert_eq!(court.charged_wire_bytes(), frames.iter().map(|p| signed_bytes(p)).sum::<u64>());
    assert_eq!(court.finish(11).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
}

#[test]
fn accepted_packet_receipts_require_the_full_plan_capacity_before_spending_an_attempt() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let descriptor = f.prepare(&result, limits()).descriptor();
    let required_receipts = descriptor.mandatory_relations.checked_mul(64).unwrap();
    for enough in [false, true] {
        let mut bounded = court_limits();
        bounded.max_receipt_bytes = required_receipts - u64::from(!enough);
        let mut court = ContractCourtV2::new(
            TirCourtCheckerV2,
            verifier_for(&result),
            &rooted(&result),
            result.clone(),
            &miner_key(),
            &sign_result(&result),
            service.clone(),
            bounded,
            1,
        )
        .unwrap();
        let witness = f.witness(&result, descriptor);
        let signature = sign_witness(&witness);
        let context = f
            .prepare(&result, limits())
            .authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &signature)
            .unwrap();
        assert_eq!(court.open_candidate(witness, &panel_key(), &signature, context, 2).is_ok(), enough);
        assert_eq!(court.attempts(), u32::from(enough));
        assert_eq!(court.charged_wire_bytes(), 0);
        assert_eq!(court.cursor(), 0);
        if !enough {
            assert_eq!(court.last_attempt(), None);
            assert_eq!(court.charged_elements(), 0);
            assert_eq!(court.charged_terms(), 0);
        }
    }
}

#[test]
fn a_context_for_another_signed_witness_cannot_be_opened_under_the_same_result_and_plan() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let prepared = f.prepare(&result, limits());
    let outer = f.witness(&result, prepared.descriptor());
    let mut inner = outer.clone();
    inner.attempt = 1;
    assert_eq!(outer.result, inner.result);
    assert_eq!(outer.plan, inner.plan);
    assert_ne!(outer.id(), inner.id());
    let context = prepared
        .authenticate_witness(inner.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &sign_witness(&inner))
        .unwrap();
    assert!(court.open_candidate(outer.clone(), &panel_key(), &sign_witness(&outer), context, 2).is_err());
    assert_eq!(court.status(), ClaimStatus::Pending);
    assert_eq!(court.attempts(), 0);
    assert_eq!(court.last_attempt(), None);
    assert_eq!(court.witness_status(), None);
    assert_eq!(court.cursor(), 0);
    assert_eq!(court.charged_wire_bytes(), 0);
    assert!(court.finish(3).is_err());
    // A malformed caller context cannot consume the honest Panel's ordinal.
    let (_, descriptor, frames) = open_at(&mut court, &f, &result, 0, 4);
    assert_eq!(court.attempts(), 1);
    send(&mut court, &frames[0], descriptor.max_part_bytes, 5);
    let before = (
        court.last_attempt(),
        court.attempts(),
        court.cursor(),
        court.charged_wire_bytes(),
        court.charged_elements(),
        court.charged_terms(),
    );
    let context = f
        .prepare(&result, limits())
        .authenticate_witness(inner.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &sign_witness(&inner))
        .unwrap();
    assert!(court.open_candidate(outer.clone(), &panel_key(), &sign_witness(&outer), context, 6).is_err());
    assert_eq!(
        (
            court.last_attempt(),
            court.attempts(),
            court.cursor(),
            court.charged_wire_bytes(),
            court.charged_elements(),
            court.charged_terms()
        ),
        before
    );
    // The signed witness and coverage descriptor alone do not identify the
    // active checker: inner heap/evaluation allowances also belong to context.
    let mut changed_limits = limits();
    changed_limits.max_part_heap /= 2;
    let changed = f.prepare(&result, changed_limits);
    assert_eq!(changed.descriptor(), descriptor);
    let changed_context = changed
        .authenticate_witness(outer.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &sign_witness(&outer))
        .unwrap();
    assert!(court.open_candidate(outer.clone(), &panel_key(), &sign_witness(&outer), changed_context, 7).is_err());
    assert_eq!(
        (
            court.last_attempt(),
            court.attempts(),
            court.cursor(),
            court.charged_wire_bytes(),
            court.charged_elements(),
            court.charged_terms()
        ),
        before
    );
}

#[test]
fn fresh_common_court_replays_actual_authenticated_history_and_reconstructs_all_counters() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let replay = || {
        let mut court = court(result.clone(), service);
        let (_, descriptor, frames) = open(&mut court, &f, &result, 0);
        let first = EvidenceFrame::decode(&frames[0], descriptor.max_part_bytes).unwrap();
        assert!(court.submit_frame(first, &packet_signature([0; SIGNATURE_BYTES]), 3).is_err());
        assert_eq!(court.charged_wire_bytes(), 0);
        send(&mut court, &frames[0], descriptor.max_part_bytes, 4);
        send(&mut court, &frames[0], descriptor.max_part_bytes, 4);
        for bytes in &frames[1..] {
            send(&mut court, bytes, descriptor.max_part_bytes, 5);
        }
        let bytes = frames.iter().map(|p| signed_bytes(p)).sum::<u64>();
        assert_eq!(court.charged_wire_bytes(), bytes);
        assert_eq!(court.cursor(), descriptor.mandatory_relations);
        assert_eq!(court.finish(6).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
        (court.status(), court.attempts(), court.cursor(), court.charged_wire_bytes(), court.charged_elements(), court.charged_terms())
    };
    let original = replay();
    let restored_by_rechecking_history = replay();
    assert_eq!(restored_by_rechecking_history, original);
    assert_eq!(original.0, ClaimStatus::PositivePassed);
    assert_eq!(original.1, 1);
}

#[test]
fn replay_on_another_branch_or_after_the_current_cutoff_cannot_reuse_the_old_success() {
    let f = fixture();
    let (old_result, old_service) = court_result(&f, false);
    let mut old_court = court(old_result.clone(), old_service);
    let (_, old_descriptor, old_frames) = open(&mut old_court, &f, &old_result, 0);
    for frame in &old_frames {
        send(&mut old_court, frame, old_descriptor.max_part_bytes, 3);
    }
    assert_eq!(old_court.finish(4).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);
    old_court.revoke();
    assert_eq!(old_court.status(), ClaimStatus::Revoked);

    let mut new_result = old_result.clone();
    new_result.scope.branch = d(88);
    let new_service = service(&new_result);
    new_result.service_contract = new_service.id();
    assert_ne!(new_result.case_id(), old_result.case_id());
    let mut new_court = court(new_result.clone(), new_service);
    let (_, descriptor, new_frames) = open(&mut new_court, &f, &new_result, 0);
    let old_frame = EvidenceFrame::decode(&old_frames[0], old_descriptor.max_part_bytes).unwrap();
    let old_authorization = carrier_signature(&old_frame);
    assert!(new_court.submit_frame(old_frame, &old_authorization, 3).is_err());
    assert_eq!(new_court.cursor(), 0);
    assert_eq!(new_court.charged_wire_bytes(), 0);
    assert_eq!(new_court.status(), ClaimStatus::Pending);
    for frame in &new_frames {
        send(&mut new_court, frame, descriptor.max_part_bytes, 4);
    }
    assert_eq!(new_court.finish(5).unwrap().terminal_kind(), TerminalKindV2::PositivePassed);

    // A recovery driver supplies the CURRENT canonical clock, not historical
    // event timestamps to postpone an already reached exclusive cutoff.
    let mut expired = court(old_result.clone(), old_service);
    let (_, descriptor, _) = open(&mut expired, &f, &old_result, 0);
    let old_frame = EvidenceFrame::decode(&old_frames[0], descriptor.max_part_bytes).unwrap();
    assert!(expired.submit_frame(old_frame, &old_authorization, 100).is_err());
    assert_eq!(expired.cursor(), 0);
    assert_eq!(expired.charged_wire_bytes(), 0);
    assert_eq!(expired.status(), ClaimStatus::Pending);
    expired.expire(100).unwrap();
    assert_eq!(expired.status(), ClaimStatus::EndedUnverified);
    assert!(expired.finish(100).is_err());
}

mod admitted_fact_bridge {
    use super::*;
    use kaspa_consensus_core::palw_positive_court_v2::coordinator::{admission::*, contract::CheckedCourtFactV2};
    use misaka_palw_positive_accounting::fees::{
        AccountingAuthority, AccountingError, AccountingLimits, PrivateAccountingStore, ResolvedAdmission,
    };
    use misaka_palw_positive_contract::ports::AdmissionIntent;
    use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

    // The authority and balances are explicit test fixtures. The implementation
    // under test is A5's real private store, not a native funds/journal adapter.
    struct Facts {
        root: CourtAdmissionRootV2,
        resolved: ResolvedCourtAdmissionV2,
        accounting: ResolvedAdmission,
        terminal: Option<ClaimStatus>,
        case_present: bool,
        court_deadline: u64,
    }
    #[derive(Clone)]
    struct View(Rc<RefCell<Facts>>);
    impl CourtAdmissionViewV2 for View {
        type Error = &'static str;
        fn root(&self) -> CourtAdmissionRootV2 {
            self.0.borrow().root
        }
        fn resolve(&self, _: &MinerResultCommit) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
            Ok(self.0.borrow().resolved)
        }
        fn resolve_committed(
            &self,
            intent: &AdmissionIntent,
            result: &MinerResultCommit,
        ) -> Result<ResolvedCourtAdmissionV2, Self::Error> {
            let facts = self.0.borrow();
            if !facts.case_present
                || intent.case != facts.resolved.intent.case
                || intent.result != result.id()
                || intent.scope != facts.root.scope
            {
                return Err("current canonical Case row/anchor missing");
            }
            if facts.root.now >= facts.court_deadline && facts.terminal.is_none() {
                return Err("pending Court deadline elapsed");
            }
            Ok(facts.resolved)
        }
    }
    impl AccountingAuthority for View {
        fn now(&self) -> u64 {
            self.0.borrow().root.now
        }
        fn admission(&self, intent: &AdmissionIntent) -> Result<ResolvedAdmission, AccountingError> {
            let facts = self.0.borrow();
            if facts.accounting.intent != *intent {
                return Err(AccountingError::MissingConsentOrFunding);
            }
            Ok(facts.accounting.clone())
        }
        fn closed_case(&self, intent: &AdmissionIntent) -> Result<ClaimStatus, AccountingError> {
            let facts = self.0.borrow();
            if facts.accounting.intent != *intent {
                return Err(AccountingError::MissingCourtClosure);
            }
            facts.terminal.ok_or(AccountingError::MissingCourtClosure)
        }
    }
    type Admission = AtomicAdmissionCoordinatorV2<PrivateAccountingStore<View>, View>;
    fn admission(result: &MinerResultCommit, service: MinerServiceContract) -> (Admission, View) {
        let intent = AdmissionIntent {
            scope: result.scope,
            case: result.case_id(),
            result: result.id(),
            resource_quote: d(61),
            funding_consent: service.funding_consent,
            obligations: d(62),
            expiry: 90,
        };
        let view = View(Rc::new(RefCell::new(Facts {
            root: CourtAdmissionRootV2 { scope: result.scope, generation: 1, now: 1 },
            resolved: ResolvedCourtAdmissionV2 { context: rooted(result), service, intent },
            accounting: ResolvedAdmission {
                intent,
                payer: result.miner,
                fee_cap: 100,
                hold: 100,
                contractual_burn: 0,
                resource_units: 4,
                services: BTreeMap::new(),
            },
            terminal: None,
            case_present: true,
            court_deadline: 100,
        })));
        let store = PrivateAccountingStore::new(
            view.clone(),
            BTreeMap::from([(result.miner, 1000)]),
            AccountingLimits { max_accounts: 4, max_cases: 4, max_services_per_case: 4, resource_capacity: 10 },
        )
        .unwrap();
        (AtomicAdmissionCoordinatorV2::new(store, view.clone()), view)
    }
    fn completed(
        f: &Fixture,
        result: &MinerResultCommit,
        service: MinerServiceContract,
    ) -> CheckedCourtFactV2<misaka_palw_tir_exec::node::positive_contract::CheckedTerminal> {
        completed_at(f, result, service, 3)
    }
    fn completed_at(
        f: &Fixture,
        result: &MinerResultCommit,
        service: MinerServiceContract,
        frame_daa: u64,
    ) -> CheckedCourtFactV2<misaka_palw_tir_exec::node::positive_contract::CheckedTerminal> {
        let mut court = court(result.clone(), service);
        let (_, descriptor, frames) = open(&mut court, f, result, 0);
        for frame in frames {
            send(&mut court, &frame, descriptor.max_part_bytes, frame_daa);
        }
        court.finish(frame_daa + 1).unwrap()
    }

    #[test]
    fn real_checked_terminal_binds_real_private_admission_and_closes_its_hold_once() {
        let f = fixture();
        let (result, service) = court_result(&f, false);
        let (mut c, view) = admission(&result, service);
        let prepared = c.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap();
        let receipt = c.commit(prepared).unwrap();
        assert_eq!(c.store().balance(result.miner), 900);
        assert_eq!(c.store().reserved_resources(), 4);
        let fact = completed(&f, &result, service);
        view.0.borrow_mut().root.now = 10;
        let bound = c.bind_checked_fact(&receipt, fact).unwrap();
        assert_eq!(bound.checked_fact().terminal_kind(), TerminalKindV2::PositivePassed);
        assert_eq!(bound.admitted_case().intent(), receipt.intent());
        assert_eq!(bound.root(), view.root());
        // Closing is authorized from this checked fact in the explicit test
        // view; the public A4/A5 API receives no `complete=true` parameter.
        view.0.borrow_mut().terminal = Some(match bound.checked_fact().terminal_kind() {
            TerminalKindV2::PositivePassed => ClaimStatus::PositivePassed,
            other => panic!("unexpected checked terminal: {other:?}"),
        });
        let close = c.store().prepare_close(bound.admitted_case().store_case()).unwrap();
        let undo = c.apply_bound_transition(&bound, close).unwrap();
        assert_eq!(c.store().balance(result.miner), 1000);
        assert_eq!(c.store().reserved_resources(), 0);
        assert!(c.store().conserved());
        let close_again = c.store().prepare_close(bound.admitted_case().store_case()).unwrap();
        c.apply_bound_transition(&bound, close_again).unwrap();
        assert_eq!(c.store().balance(result.miner), 1000);
        assert_eq!(c.store().reserved_resources(), 0);
        assert!(c.store().conserved());
        // The duplicate close installed no new book, so the original genuine
        // undo still restores the pre-close hold/resource reservation together.
        c.rollback(undo).unwrap();
        assert_eq!(c.store().balance(result.miner), 900);
        assert_eq!(c.store().reserved_resources(), 4);
        assert!(c.store().conserved());
    }

    #[test]
    fn foreign_receipt_case_result_branch_reverse_generation_or_missing_case_cannot_bind_fact() {
        for mode in 0..9 {
            let f = fixture();
            let (result, service) = court_result(&f, false);
            let (mut c, view) = admission(&result, service);
            let prepared = c.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap();
            let receipt = c.commit(prepared).unwrap();
            let outcome = match mode {
                0 => {
                    let (foreign, _) = admission(&result, service);
                    foreign.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
                1 => {
                    let mut changed = result.clone();
                    changed.output = d(99);
                    assert_eq!(changed.case_id(), result.case_id());
                    assert_ne!(changed.id(), result.id());
                    c.bind_checked_fact(&receipt, completed(&f, &changed, service)).err()
                }
                2 => {
                    let another = fixture_for_job(Some(Hash64::from_bytes([76; 64])));
                    let (other_result, other_service) = court_result(&another, false);
                    assert_eq!(other_result.scope, result.scope);
                    assert_ne!(other_result.case_id(), result.case_id());
                    c.bind_checked_fact(&receipt, completed(&another, &other_result, other_service)).err()
                }
                3 => {
                    let mut changed = result.clone();
                    changed.scope.branch = d(77);
                    let changed_service = super::service(&changed);
                    changed.service_contract = changed_service.id();
                    c.bind_checked_fact(&receipt, completed(&f, &changed, changed_service)).err()
                }
                4 => {
                    view.0.borrow_mut().root.generation = 0;
                    c.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
                5 => {
                    view.0.borrow_mut().root.scope.branch = d(78);
                    c.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
                6 => {
                    view.0.borrow_mut().resolved.intent.resource_quote = d(79);
                    c.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
                7 => {
                    view.0.borrow_mut().case_present = false;
                    c.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
                _ => {
                    view.0.borrow_mut().root.now = 0;
                    c.bind_checked_fact(&receipt, completed(&f, &result, service)).err()
                }
            };
            assert!(outcome.is_some(), "binding mode {mode} must fail");
            assert_eq!(c.store().balance(result.miner), 900);
            assert_eq!(c.store().reserved_resources(), 4);
            assert!(c.store().conserved());
        }
    }

    #[test]
    fn timely_bound_fact_can_close_after_normal_generation_advance_and_service_expiry() {
        let f = fixture();
        let (result, service) = court_result(&f, false);
        let (mut c, view) = admission(&result, service);
        let prepared = c.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap();
        let receipt = c.commit(prepared).unwrap();
        view.0.borrow_mut().root.now = 10;
        let bound = c.bind_checked_fact(&receipt, completed(&f, &result, service)).unwrap();
        assert_eq!(bound.checked_fact().terminal_kind(), TerminalKindV2::PositivePassed);
        {
            let mut facts = view.0.borrow_mut();
            facts.terminal = Some(match bound.checked_fact().terminal_kind() {
                TerminalKindV2::PositivePassed => ClaimStatus::PositivePassed,
                other => panic!("unexpected checked terminal: {other:?}"),
            });
            facts.root.generation += 1;
            facts.root.now = 110; // after both admission expiry and service cutoff
        }
        let close = c.store().prepare_close(bound.admitted_case().store_case()).unwrap();
        c.apply_bound_transition(&bound, close).unwrap();
        assert_eq!(c.store().balance(result.miner), 1000);
        assert_eq!(c.store().reserved_resources(), 0);
        assert!(c.store().conserved());
        // The funding cutoff forbids a NEW admission, but does not erase the
        // independently completed canonical Case history used to bind/rebind.
        assert!(c.prepare(result.clone(), &miner_key(), &sign_result(&result)).is_err());
        assert!(c.bind_checked_fact(&receipt, completed(&f, &result, service)).is_ok());
    }

    #[test]
    fn bound_transition_checks_current_scope_resolution_and_store_epoch_after_undo() {
        for mode in 0..5 {
            let f = fixture();
            let (result, service) = court_result(&f, false);
            let (mut c, view) = admission(&result, service);
            let prepared = c.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap();
            let receipt = c.commit(prepared).unwrap();
            view.0.borrow_mut().root.now = 10;
            let bound = c.bind_checked_fact(&receipt, completed(&f, &result, service)).unwrap();
            view.0.borrow_mut().terminal = Some(match bound.checked_fact().terminal_kind() {
                TerminalKindV2::PositivePassed => ClaimStatus::PositivePassed,
                other => panic!("unexpected checked terminal: {other:?}"),
            });
            let stale = c.store().prepare_close(bound.admitted_case().store_case()).unwrap();
            if mode == 3 {
                let current = c.store().prepare_close(bound.admitted_case().store_case()).unwrap();
                let undo = c.apply_bound_transition(&bound, current).unwrap();
                c.rollback(undo).unwrap();
            }
            {
                let mut facts = view.0.borrow_mut();
                facts.root.now = 20;
                match mode {
                    0 => facts.root.scope.branch = d(79),
                    1 => facts.root.generation = 0,
                    2 => facts.resolved.intent.obligations = d(80),
                    3 => facts.root.generation += 1,
                    _ => facts.root.now = 9,
                }
            }
            let error = c.apply_bound_transition(&bound, stale).err().unwrap();
            if mode == 3 {
                assert_eq!(error, CourtAdmissionErrorV2::Store(AccountingError::StalePreparation));
            } else {
                assert_eq!(error, CourtAdmissionErrorV2::StaleRoot);
            }
            assert_eq!(c.store().balance(result.miner), 900);
            assert_eq!(c.store().reserved_resources(), 4);
            assert!(c.store().conserved());
        }
    }

    #[test]
    fn first_binding_after_funding_cutoff_uses_independent_current_court_deadline() {
        for now in [95, 100] {
            let f = fixture();
            let (result, service) = court_result(&f, false);
            let (mut c, view) = admission(&result, service);
            let prepared = c.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap();
            let receipt = c.commit(prepared).unwrap();
            let fact = completed_at(&f, &result, service, 94);
            {
                let mut facts = view.0.borrow_mut();
                facts.root.generation += 1; // normal block progression
                facts.root.now = now;
            }
            assert!(receipt.intent().expiry < now);
            assert_eq!(view.0.borrow().court_deadline, 100);
            let bound = c.bind_checked_fact(&receipt, fact);
            if now < 100 {
                assert_eq!(bound.unwrap().checked_fact().terminal_kind(), TerminalKindV2::PositivePassed);
            } else {
                assert!(matches!(bound, Err(CourtAdmissionErrorV2::View("pending Court deadline elapsed"))));
            }
            assert_eq!(c.store().balance(result.miner), 900);
            assert_eq!(c.store().reserved_resources(), 4);
            // NEW admissions still obey the payer's earlier funding cutoff.
            assert!(c.prepare(result.clone(), &miner_key(), &sign_result(&result)).is_err());
        }
    }

    use misaka_palw_positive_contract::ports::AtomicCourtStore;
    struct RestartBook(PrivateAccountingStore<View>);
    impl AtomicCourtStore for RestartBook {
        type PreparedAdmission = <PrivateAccountingStore<View> as AtomicCourtStore>::PreparedAdmission;
        type AdmittedCase = <PrivateAccountingStore<View> as AtomicCourtStore>::AdmittedCase;
        type CheckedTransition = <PrivateAccountingStore<View> as AtomicCourtStore>::CheckedTransition;
        type UndoRecord = <PrivateAccountingStore<View> as AtomicCourtStore>::UndoRecord;
        type Error = AccountingError;
        fn prepare_admission(&self, intent: AdmissionIntent) -> Result<Self::PreparedAdmission, Self::Error> {
            self.0.prepare_admission(intent)
        }
        fn commit_admission(&mut self, prepared: Self::PreparedAdmission) -> Result<Self::AdmittedCase, Self::Error> {
            self.0.commit_admission(prepared)
        }
        fn apply_transition(
            &mut self,
            case: &Self::AdmittedCase,
            transition: Self::CheckedTransition,
        ) -> Result<Self::UndoRecord, Self::Error> {
            self.0.apply_transition(case, transition)
        }
        fn rollback(&mut self, undo: Self::UndoRecord) -> Result<(), Self::Error> {
            self.0.rollback(undo)
        }
    }
    impl CommittedCourtRecoveryStoreV2 for RestartBook {
        fn recover_committed_case(&self, intent: AdmissionIntent, _: CourtAdmissionRootV2) -> Result<Self::AdmittedCase, Self::Error> {
            // Explicit PRIVATE adapter. The independently resolved committed
            // view above is still mandatory; this does not restore native funds.
            self.0.recover_committed_case(intent)
        }
    }
    #[test]
    fn real_a1_fact_binds_recovered_actual_book_after_restart_and_funding_cutoff() {
        let f = fixture();
        let (result, service) = court_result(&f, false);
        let (mut old, view) = admission(&result, service);
        let old_receipt = old.commit(old.prepare(result.clone(), &miner_key(), &sign_result(&result)).unwrap()).unwrap();
        let fact = completed(&f, &result, service);
        let foreign_fact = completed(&f, &result, service);
        view.0.borrow_mut().terminal = Some(match fact.terminal_kind() {
            TerminalKindV2::PositivePassed => ClaimStatus::PositivePassed,
            other => panic!("unexpected actual A1 terminal {other:?}"),
        });
        let before_restart_close = old.store().prepare_close(old_receipt.store_case()).unwrap();
        let (book, _) = old.into_parts();
        let mut current = AtomicAdmissionCoordinatorV2::new(RestartBook(book), view.clone());
        {
            let mut facts = view.0.borrow_mut();
            facts.root.now = 110;
            facts.root.generation = 8;
        }
        let before = (
            current.store().0.balance(result.miner),
            current.store().0.reserved_resources(),
            current.store().0.totals(result.case_id()),
        );
        let recovered =
            current.recover_committed_case(result.clone(), &miner_key(), &sign_result(&result), old_receipt.intent()).unwrap();
        assert_eq!(
            before,
            (
                current.store().0.balance(result.miner),
                current.store().0.reserved_resources(),
                current.store().0.totals(result.case_id())
            )
        );
        assert_eq!(recovered.root(), view.root());
        assert!(current.prepare(result.clone(), &miner_key(), &sign_result(&result)).is_err());
        assert!(matches!(current.bind_checked_fact(&old_receipt, foreign_fact), Err(CourtAdmissionErrorV2::ForeignCapability)));
        let bound = current.bind_checked_fact(&recovered, fact).unwrap();
        assert_eq!(bound.checked_fact().terminal_kind(), TerminalKindV2::PositivePassed);
        // Actual A5 transition prepared BEFORE restart remains valid: recovery
        // performed no epoch update, hold, reservation or journal operation.
        current.apply_bound_transition(&bound, before_restart_close).unwrap();
        assert_eq!(current.store().0.balance(result.miner), 1000);
        assert_eq!(current.store().0.reserved_resources(), 0);
        let again = current.store().0.prepare_close(recovered.store_case()).unwrap();
        current.apply_bound_transition(&bound, again).unwrap();
        assert_eq!(current.store().0.balance(result.miner), 1000);
        assert_eq!(current.store().0.reserved_resources(), 0);
        assert!(current.store().0.conserved());
    }
}

#[test]
fn honest_material_with_insufficient_inner_evaluation_allowance_stays_unverified() {
    let f = fixture();
    let (result, service) = court_result(&f, false);
    let mut court = court(result.clone(), service);
    let mut small = limits();
    small.max_elements_per_leaf = 1;
    small.max_terms_per_leaf = 1;
    let prepared = f.prepare(&result, small);
    let descriptor = prepared.descriptor();
    let witness = f.witness(&result, descriptor);
    let signature = sign_witness(&witness);
    let context =
        prepared.authenticate_witness(witness.clone(), f.binding.clone(), f.generated.clone(), &panel_key(), &signature).unwrap();
    court.open_candidate(witness.clone(), &panel_key(), &signature, context, 2).unwrap();
    let frames = f.frames(&witness, descriptor);
    let mut charged = 0;
    let mut failed = false;
    for bytes in frames {
        let frame = EvidenceFrame::decode(&bytes, descriptor.max_part_bytes).unwrap();
        let authorization = carrier_signature(&frame);
        charged += signed_bytes(&bytes);
        if court.submit_frame(frame, &authorization, 3).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed, "the honest vector relation exceeds the admitted one-element allowance");
    assert_eq!(court.witness_status(), Some(WitnessStatus::Incomplete));
    assert_eq!(court.status(), ClaimStatus::Pending);
    assert_eq!(court.attempts(), 1);
    assert_eq!(court.charged_wire_bytes(), charged);
    assert!(court.charged_elements() > 0);
    assert!(court.charged_terms() > 0);
    assert!(court.finish(4).is_err());
}
