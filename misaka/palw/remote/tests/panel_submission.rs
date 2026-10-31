//! A3 transport integration: a replacement relay retains the original witness
//! author and follows the receiving Node's cursor rather than a local send log.
#[path = "../../tir/exec/tests/node_common/mod.rs"]
mod node_common;

use kaspa_consensus_core::{
    Hash64,
    palw_attempt_rules_v1::palw_attempt_output_root_v1,
    palw_positive_court_v2::{
        coordinator::*,
        gate::ClaimFormV2,
        service::{CHUNK_BYTES, ServicePolicyV2},
        *,
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_court_v1::PalwTirCourtRulesV1,
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use misaka_palw_remote::{
    bundle::DetachedSigner,
    pursuit::positive::{PositivePursuitStepV2, submission::*},
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, demand::DemandLimits};
use misaka_palw_tir_exec::{
    TirParams, TirPlan,
    node::{
        TirClassRunnerV1,
        inventory::{TirHeldInventoryV1, TirParamsSourceV1},
        positive_court::{PanelCaptureV2, RoleLimitsV2},
    },
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

fn policy() -> ExactPolicyV2 {
    ExactPolicyV2 {
        max_leaves: 1 << 12,
        max_part_bytes: 1 << 20,
        max_part_heap: 1 << 24,
        max_total_bytes: 1 << 26,
        max_elements_per_leaf: 1 << 16,
        max_terms_per_leaf: 1 << 18,
        max_total_elements: 1 << 32,
        max_total_terms: 1 << 36,
        max_commit_bytes: 1 << 20,
        deadline: 100,
    }
}
fn rules() -> PalwTirCourtRulesV1 {
    let p = policy();
    PalwTirCourtRulesV1 {
        max_step_leaf_count: p.max_leaves,
        prompt_form: PalwPromptIdsFormV1::Flat,
        limits: DemandLimits { max_elements: p.max_elements_per_leaf, max_terms: p.max_terms_per_leaf },
    }
}
fn scope(result: &MinerResultV2) -> AdmissionScopeV2 {
    AdmissionScopeV2 {
        network: result.network,
        branch: result.branch,
        ruleset: result.ruleset,
        registered_class_id: result.job.shape_profile_id,
        required_form: ClaimFormV2::MinerResultV2,
    }
}

fn with_fixture(body: impl FnOnce(&MinerResultV2, &PanelWitnessV2, &[ExactPartV2], &PalwTirClassV1, ServicePolicyV2)) {
    let mut builder = ProgramBuilder::new(4, 1 << 18);
    let embedding = builder.param("embedding", DType::I8, &[4, 4], false);
    let mut pre = builder.block("pre", vec![]);
    let x = pre.gather(embedding, Ref::Input(0), 0, 0);
    let pre = pre.finish(&[x]);
    let mut post = builder.block("post", vec![TensorType::fixed(DType::I8, &[4])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    let Ref::Node(logits) = post.commit(y) else { unreachable!() };
    let post = post.finish(&[]);
    let program = node_common::tiled(builder.finish(pre, vec![], post, logits));
    let mut map = MapParams::default();
    map.tensors
        .insert((0, None), Tensor::new(DType::I8, vec![4, 4], vec![1, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2]).unwrap());
    let plan = TirPlan::compile(&program).unwrap();
    let params = TirParams::from_map(&plan, &map).unwrap();
    let source = TirParamsSourceV1(&params);
    let inventory = TirHeldInventoryV1::build(&program, &source).unwrap();
    let artifact = inventory.tree.root();
    let class = PalwTirClassV1 {
        version: 1,
        program: program.encode(),
        layout: node_common::layout(&program, 3, 2, 2, 16),
        tokenizer_id: Hash64::from_u64_word(18),
    };
    let mut job = node_common::job(&program, 1, 3, class.class_id(&artifact), &[0], PalwPromptIdsFormV1::Flat);
    job.tokenizer_id = class.tokenizer_id;
    let service = ServicePolicyV2 {
        version: 2,
        case: Hash64::from_u64_word(4),
        source: Hash64::from_u64_word(8),
        chunks: 1,
        max_rounds: 1,
        max_unique_bytes: CHUNK_BYTES as u64,
        max_transmitted_bytes: 1 << 16,
        work_per_chunk: 1,
        max_work: 4,
        retention_until: 100,
        deadline: 100,
        price_per_chunk: 1,
        hold: 1,
    };
    let result = MinerResultV2 {
        version: 2,
        network: Hash64::from_u64_word(1),
        branch: Hash64::from_u64_word(2),
        ruleset: Hash64::from_u64_word(3),
        claim: service.case,
        output: palw_attempt_output_root_v1(&job, &[1, 0, 1]),
        job,
        artifact,
        sampling: kaspa_consensus_core::palw_decode_select_v2::PalwDecodeSamplingV2::GREEDY,
        service_policy: service.id(),
        dependencies: Hash64::from_u64_word(6),
    };
    let space = PalwTirStepSpaceV1::new(&class).unwrap();
    let runner = TirClassRunnerV1::new(&space, &plan, &params, result.job.shape_profile_id).unwrap();
    let panel = PanelCaptureV2::replay(
        &runner,
        &class,
        artifact,
        &result.job,
        &[0],
        PalwPromptIdsFormV1::Flat,
        RoleLimitsV2 { max_prefill: 4, max_decode: 4, max_leaves: 1 << 12, max_capture_bytes: 1 << 26 },
    )
    .unwrap();
    assert_eq!(panel.generated(), &[1, 0, 1]);
    let witness = panel.witness(&result, policy(), 0).unwrap();
    let mut parts = Vec::new();
    panel
        .stream_parts(&runner, &inventory, &rules(), policy().max_part_bytes, &mut |_, p| {
            parts.push(p);
            Ok(())
        })
        .unwrap();
    body(&result, &witness, &parts, &class, service);
}

fn coordinator(result: &MinerResultV2, class: &PalwTirClassV1, service: ServicePolicyV2, panel: &Key) -> PositiveCoordinatorV2 {
    let miner = Key::new(81);
    let p = policy();
    PositiveCoordinatorV2::admit(
        scope(result),
        result.clone(),
        SignatureV2(miner.sign_with_context(result.id().as_byte_slice(), RESULT_CONTEXT).unwrap()),
        miner.public_key(),
        class.clone(),
        p,
        PalwPromptIdsFormV1::Flat,
        PanelRosterV2::new(vec![panel.public_key()], 1 << 20).unwrap(),
        service,
        CoordinatorLimitsV2 {
            max_attempts: 2,
            max_reserved_elements: 1 << 40,
            max_reserved_terms: 1 << 44,
            max_reserved_witness_bytes: p.max_total_bytes * 2,
            max_history_bytes: 1 << 20,
            max_batch_parts: 16,
            max_batch_bytes: 1 << 24,
        },
        1,
    )
    .unwrap()
}

fn send_limits() -> PanelSendLimitsV2 {
    PanelSendLimitsV2 { max_parts: 16, max_bytes: 1 << 24 }
}

#[test]
fn signed_candidate_takeover_preserves_original_author_and_node_cursor() {
    with_fixture(|result, witness, parts, class, service| {
        let author = Key::new(82);
        let stranger = Key::new(83);
        let candidate = PanelCandidateV2::sign(result, scope(result), policy(), witness.clone(), &author).unwrap();
        let takeover = PanelCandidateV2::from_signed(
            result,
            scope(result),
            policy(),
            witness.clone(),
            candidate.signature().clone(),
            &author.public_key(),
        )
        .unwrap();
        assert_eq!(takeover.author(), candidate.author());
        assert_eq!(takeover.signature(), candidate.signature());
        assert!(matches!(
            PanelCandidateV2::from_signed(
                result,
                scope(result),
                policy(),
                witness.clone(),
                candidate.signature().clone(),
                &stranger.public_key()
            ),
            Err(ErrorV2::Authentication)
        ));
        let mut node = coordinator(result, class, service, &author);
        assert_eq!(candidate.open(&mut node, 2), Ok(CandidateEffectV2::Opened));
        assert_eq!(takeover.open(&mut node, 2), Ok(CandidateEffectV2::AlreadyActive));
        let mut batch = candidate.batch(0, 1, send_limits()).unwrap();
        batch.push(0, &parts[0]).unwrap();
        let batch = batch.finish().unwrap();
        assert_eq!(batch.submit(&mut node, 3), Ok(1));
        let charged = node.attempts();
        assert_eq!(batch.submit(&mut node, 3), Err(ErrorV2::Order));
        assert_eq!(node.attempts(), charged);
        assert_eq!(
            takeover.next_step(node.current(), result.branch, 16, 3),
            PositivePursuitStepV2::SubmitParts { witness: witness.id(), first: 1, count: parts.len() as u64 - 1 }
        );
        let mut suffix = takeover.batch(1, parts.len() as u64 - 1, send_limits()).unwrap();
        for (index, part) in parts.iter().enumerate().skip(1) {
            suffix.push(index as u64, part).unwrap();
        }
        assert_eq!(suffix.finish().unwrap().submit(&mut node, 4), Ok(parts.len() as u64 - 1));
        assert_eq!(takeover.next_step(node.current(), result.branch, 16, 4), PositivePursuitStepV2::FinishNodeCheck);
        assert_eq!(node.finish(5), Ok(ClaimStatusV2::Passed));
        assert_eq!(
            takeover.next_step(node.current(), result.branch, 0, 1000),
            PositivePursuitStepV2::NodeCompleted { outcome: ClaimStatusV2::Passed }
        );
        assert_eq!(takeover.next_step(node.current(), Hash64::from_u64_word(999), 16, 5), PositivePursuitStepV2::RevalidateBranch);
        node.revoke();
        assert_eq!(takeover.next_step(node.current(), result.branch, 16, 5), PositivePursuitStepV2::RevalidateBranch);
    });
}

#[test]
fn batch_order_byte_limits_and_incomplete_submission_fail_closed() {
    with_fixture(|result, witness, parts, _, _| {
        let author = Key::new(82);
        let candidate = PanelCandidateV2::sign(result, scope(result), policy(), witness.clone(), &author).unwrap();
        assert!(matches!(candidate.batch(0, 0, send_limits()), Err(ErrorV2::Budget)));
        assert!(matches!(candidate.batch(u64::MAX, 2, send_limits()), Err(ErrorV2::Budget)));
        assert!(matches!(candidate.batch(0, 17, send_limits()), Err(ErrorV2::Budget)));
        let mut batch = candidate.batch(0, 2, send_limits()).unwrap();
        assert_eq!(batch.push(1, &parts[1]), Err(ErrorV2::Order));
        batch.push(0, &parts[0]).unwrap();
        assert_eq!(batch.push(0, &parts[0]), Err(ErrorV2::Order));
        assert!(matches!(batch.finish(), Err(ErrorV2::Incomplete)));
        let mut tiny = candidate.batch(0, 1, PanelSendLimitsV2 { max_parts: 1, max_bytes: 128 }).unwrap();
        assert_eq!(tiny.push(0, &parts[0]), Err(ErrorV2::Budget));
        assert!(matches!(tiny.finish(), Err(ErrorV2::Incomplete)));
    });
}

#[test]
fn result_policy_and_witness_substitution_cannot_be_signed_or_taken_over() {
    with_fixture(|result, witness, _, _, _| {
        let author = Key::new(82);
        let candidate = PanelCandidateV2::sign(result, scope(result), policy(), witness.clone(), &author).unwrap();
        let mut changed = result.clone();
        changed.output = Hash64::from_u64_word(999);
        assert!(PanelCandidateV2::sign(&changed, scope(&changed), policy(), witness.clone(), &author).is_err());
        let mut p = policy();
        p.deadline += 1;
        assert!(matches!(
            PanelCandidateV2::from_signed(
                result,
                scope(result),
                p,
                witness.clone(),
                candidate.signature().clone(),
                &author.public_key()
            ),
            Err(ErrorV2::Binding)
        ));
        let mut changed_witness = witness.clone();
        changed_witness.attempt += 1;
        assert!(matches!(
            PanelCandidateV2::from_signed(
                result,
                scope(result),
                policy(),
                changed_witness,
                candidate.signature().clone(),
                &author.public_key()
            ),
            Err(ErrorV2::Authentication)
        ));
    });
}

#[test]
fn rejected_panel_candidate_can_be_replaced_with_next_authenticated_attempt() {
    with_fixture(|result, witness, parts, class, service| {
        let author = Key::new(82);
        let first = PanelCandidateV2::sign(result, scope(result), policy(), witness.clone(), &author).unwrap();
        let mut node = coordinator(result, class, service, &author);
        first.open(&mut node, 2).unwrap();
        let mut corrupted = parts[0].clone();
        corrupted.cone.output_preimage.values_le[0] ^= 1;
        let mut bad_batch = first.batch(0, 1, send_limits()).unwrap();
        bad_batch.push(0, &corrupted).unwrap();
        assert!(bad_batch.finish().unwrap().submit(&mut node, 3).is_err());
        assert_eq!(node.current().unwrap().statuses(), (ClaimStatusV2::Checking, WitnessStatusV2::Rejected));
        assert_eq!(first.next_step(node.current(), result.branch, 16, 3), PositivePursuitStepV2::WitnessRejected);
        let mut second_witness = witness.clone();
        second_witness.attempt = 1;
        let second = PanelCandidateV2::sign(result, scope(result), policy(), second_witness.clone(), &author).unwrap();
        assert_eq!(
            second.next_step(node.current(), result.branch, 16, 3),
            PositivePursuitStepV2::CommitWitness { witness: second_witness.id() }
        );
        assert_eq!(second.open(&mut node, 4), Ok(CandidateEffectV2::Opened));
        assert_eq!(node.attempts().attempts, 2);
        assert_eq!(node.history().len(), 1);
        assert_eq!(node.history()[0].status, WitnessStatusV2::Rejected);
        let mut full = second.batch(0, parts.len() as u64, send_limits()).unwrap();
        for (index, part) in parts.iter().enumerate() {
            full.push(index as u64, part).unwrap();
        }
        assert_eq!(full.finish().unwrap().submit(&mut node, 5), Ok(parts.len() as u64));
        assert_eq!(node.finish(6), Ok(ClaimStatusV2::Passed));
    });
}

// The subprocess receives commitments, role public keys/signatures and public
// evidence only. No model weights, executor, Miner trace API or capture is sent.
type ChildBundle = (MinerResultV2, PanelWitnessV2, PalwTirClassV1, ExactPolicyV2, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, Vec<u8>, u8);
const CHILD_BUNDLE_ENV: &str = "MISAKA_A3_TEST_PUBLIC_BUNDLE";

#[test]
fn child_cold_node_replays_panel_journal() {
    use std::io::Read;
    let Some(path) = std::env::var_os(CHILD_BUNDLE_ENV) else {
        return;
    };
    let mut bytes = Vec::new();
    std::fs::File::open(path).unwrap().take((1 << 24) + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= 1 << 24);
    let (result, witness, class, p, miner_key, miner_sig, panel_key, panel_sig, journal_bytes, expected): ChildBundle =
        borsh::from_slice(&bytes).unwrap();
    let mut node = ExactCaseV2::admit(
        result.clone(),
        &SignatureV2(miner_sig),
        &miner_key,
        witness,
        &SignatureV2(panel_sig),
        &panel_key,
        &class,
        p,
        PalwPromptIdsFormV1::Flat,
        1,
        scope(&result),
    )
    .unwrap();
    let status = kaspa_consensus_core::palw_positive_court_v2::journal::replay(&mut node, &mut journal_bytes.as_slice(), 3).unwrap();
    assert_eq!(status, if expected == 0 { ClaimStatusV2::Passed } else { ClaimStatusV2::Refuted });
}

#[test]
fn actual_cold_node_process_checks_panel_evidence_and_terminal_result() {
    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = TempDir(std::env::temp_dir().join(format!("misaka-a3-public-{}-{nonce}", std::process::id())));
    std::fs::create_dir(&dir.0).unwrap();
    with_fixture(|honest_result, original_witness, parts, class, _| {
        for expected in [0u8, 1] {
            let mut result = honest_result.clone();
            if expected == 1 {
                result.output = palw_attempt_output_root_v1(&result.job, &[1, 1, 1]);
            }
            let mut witness = original_witness.clone();
            witness.result = result.id();
            let miner = Key::new(81);
            let panel = Key::new(82);
            let miner_sig = miner.sign_with_context(result.id().as_byte_slice(), RESULT_CONTEXT).unwrap();
            let panel_sig = panel.sign_with_context(witness.id().as_byte_slice(), WITNESS_CONTEXT).unwrap();
            let node = ExactCaseV2::admit(
                result.clone(),
                &SignatureV2(miner_sig.clone()),
                &miner.public_key(),
                witness.clone(),
                &SignatureV2(panel_sig.clone()),
                &panel.public_key(),
                class,
                policy(),
                PalwPromptIdsFormV1::Flat,
                1,
                scope(&result),
            )
            .unwrap();
            let mut journal_bytes = Vec::new();
            kaspa_consensus_core::palw_positive_court_v2::journal::write_header(&node, &mut journal_bytes).unwrap();
            for part in parts {
                let cone = borsh::to_vec(&part.cone).unwrap();
                let trace = part.trace.as_ref().map(|t| borsh::to_vec(t).unwrap());
                kaspa_consensus_core::palw_positive_court_v2::journal::write_part(
                    &mut journal_bytes,
                    &cone,
                    trace.as_deref(),
                    policy().max_part_bytes,
                )
                .unwrap();
            }
            kaspa_consensus_core::palw_positive_court_v2::journal::write_finish(&mut journal_bytes).unwrap();
            let bundle: ChildBundle = (
                result,
                witness,
                class.clone(),
                policy(),
                miner.public_key(),
                miner_sig,
                panel.public_key(),
                panel_sig,
                journal_bytes,
                expected,
            );
            let path = dir.0.join(format!("public-evidence-{expected}.borsh"));
            std::fs::write(&path, borsh::to_vec(&bundle).unwrap()).unwrap();
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "child_cold_node_replays_panel_journal", "--nocapture"])
                .env(CHILD_BUNDLE_ENV, &path)
                .output()
                .unwrap();
            assert!(
                child.status.success(),
                "cold Node process failed: {}\n{}",
                String::from_utf8_lossy(&child.stdout),
                String::from_utf8_lossy(&child.stderr)
            );
            assert!(String::from_utf8_lossy(&child.stdout).contains("1 passed"), "the child checker test must actually execute");
        }
    });
}

// Authenticated source acquisition shares the same canonical coordinator as
// witness submission; caches and Panel changes cannot create fresh quotas.
struct SourceProvider {
    response: Option<misaka_palw_remote::pursuit::positive::sources::SourceChunkV2>,
    calls: usize,
}
impl misaka_palw_remote::pursuit::positive::sources::PanelSourceProviderV2 for SourceProvider {
    fn fetch_source(
        &mut self,
        _index: u64,
        _max_opening_siblings: usize,
    ) -> Result<Option<misaka_palw_remote::pursuit::positive::sources::SourceChunkV2>, String> {
        self.calls += 1;
        Ok(self.response.take())
    }
}

fn source_fixture(
    result: &MinerResultV2,
    mut service: ServicePolicyV2,
    max_responses: u64,
) -> (MinerResultV2, ServicePolicyV2, misaka_palw_remote::pursuit::positive::sources::SourceChunkV2) {
    use kaspa_consensus_core::{
        palw_positive_court_v2::service::chunk_hash_v2,
        palw_step_leg::{step_merkle_root_v1, step_opening_v1},
    };
    let bytes = Box::new([17; CHUNK_BYTES]);
    let leaf = chunk_hash_v2(&result.claim, 0, bytes.as_slice());
    service.source = step_merkle_root_v1(&[leaf]).unwrap();
    service.max_transmitted_bytes = max_responses * (CHUNK_BYTES as u64 + 76);
    service.max_work = max_responses;
    let mut result = result.clone();
    result.service_policy = service.id();
    let chunk = misaka_palw_remote::pursuit::positive::sources::SourceChunkV2 { bytes, opening: step_opening_v1(&[leaf], 0).unwrap() };
    (result, service, chunk)
}

fn open_source_request(
    cache: &misaka_palw_remote::pursuit::positive::sources::PanelSourceCacheV2,
    node: &mut PositiveCoordinatorV2,
    author: &Key,
) -> kaspa_consensus_core::palw_positive_court_v2::service::RequestEffectV2 {
    use kaspa_consensus_core::{mldsa87_primitives::mldsa87_key_id, palw_positive_court_v2::service::WorkRequestV2};
    let request = WorkRequestV2::Source { first: 0, count: 1 };
    let signature =
        SignatureV2(author.sign_with_context(node.request_id(request).unwrap().as_byte_slice(), SOURCE_REQUEST_CONTEXT).unwrap());
    cache.request(node, mldsa87_key_id(&author.public_key()), request, &signature, 2).unwrap()
}

#[test]
fn provider_bad_bytes_and_overlong_opening_fall_back_to_miner_without_false_completion() {
    use kaspa_consensus_core::palw_positive_court_v2::service::RequestEffectV2;
    use misaka_palw_remote::pursuit::positive::sources::*;
    with_fixture(|result, _, _, class, service| {
        let (result, service, chunk) = source_fixture(result, service, 2);
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let mut cache =
            PanelSourceCacheV2::new(&result, service, SourceCacheLimitsV2 { max_retained_bytes: 1 << 16, max_provider_attempts: 3 })
                .unwrap();
        assert_eq!(open_source_request(&cache, &mut node, &author), RequestEffectV2::Opened);
        let mut overlong = chunk.clone();
        overlong.opening.siblings.push(Hash64::from_u64_word(999));
        let mut bad = chunk.clone();
        bad.bytes[0] ^= 1;
        let mut oversized_provider = SourceProvider { response: Some(overlong), calls: 0 };
        let mut bad_provider = SourceProvider { response: Some(bad), calls: 0 };
        let mut miner = SourceProvider { response: Some(chunk.clone()), calls: 0 };
        assert_eq!(
            cache.acquire_one(&mut node, 0, &mut [&mut oversized_provider, &mut bad_provider, &mut miner], &mut || 3,),
            Ok(SourceAcquisitionV2::Delivered { provider: 2 })
        );
        assert_eq!((oversized_provider.calls, bad_provider.calls, miner.calls), (1, 1, 1));
        assert_eq!(cache.get(0), Some(chunk.bytes.as_ref()));
        assert_eq!(node.service_meter().transmitted_bytes, 2 * (CHUNK_BYTES as u64 + 76));
        assert_eq!(node.service_meter().work, 2);
        assert_eq!(node.service_meter().paid, 1);
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Complete);
        assert_eq!(node.completion().witness, None);
        assert_eq!(node.completion().claim, ClaimStatusV2::Checking);
        // A local hit causes no second delivery, charge or provider call.
        let meter = node.service_meter();
        assert_eq!(cache.acquire_one(&mut node, 0, &mut [&mut miner], &mut || 4), Ok(SourceAcquisitionV2::Cached));
        assert_eq!(node.service_meter(), meter);
        assert_eq!(miner.calls, 1);
    });
}

#[test]
fn source_takeover_coalesces_requests_and_shared_exhaustion_cannot_reset() {
    use kaspa_consensus_core::palw_positive_court_v2::service::RequestEffectV2;
    use misaka_palw_remote::pursuit::positive::sources::*;
    with_fixture(|result, _, _, class, service| {
        let (result, service, chunk) = source_fixture(result, service, 1);
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let limits = SourceCacheLimitsV2 { max_retained_bytes: 1 << 16, max_provider_attempts: 2 };
        let mut first_panel = PanelSourceCacheV2::new(&result, service.clone(), limits).unwrap();
        assert_eq!(open_source_request(&first_panel, &mut node, &author), RequestEffectV2::Opened);
        let mut invalid = chunk.clone();
        invalid.bytes[0] ^= 1;
        let mut provider = SourceProvider { response: Some(invalid), calls: 0 };
        assert_eq!(first_panel.acquire_one(&mut node, 0, &mut [&mut provider], &mut || 3), Ok(SourceAcquisitionV2::Unavailable));
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Pending);
        let charged = node.service_meter();
        let mut replacement = PanelSourceCacheV2::new(&result, service.clone(), limits).unwrap();
        assert_eq!(open_source_request(&replacement, &mut node, &author), RequestEffectV2::Coalesced);
        assert_eq!(node.service_meter(), charged);
        let mut miner = SourceProvider { response: Some(chunk), calls: 0 };
        assert_eq!(replacement.acquire_one(&mut node, 0, &mut [&mut miner], &mut || 4), Err(ErrorV2::Budget));
        assert_eq!(node.service_meter(), charged);
        assert_eq!(replacement.cached_chunks(), 0);
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Pending);
        assert_eq!(node.completion().claim, ClaimStatusV2::Checking);
        assert_eq!(node.service_meter().paid, 0);
    });
}

#[test]
fn authenticated_source_transfer_reuses_cache_without_new_node_charge() {
    use misaka_palw_remote::pursuit::positive::sources::*;
    with_fixture(|result, _, _, class, service| {
        let (result, service, chunk) = source_fixture(result, service, 2);
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let limits = SourceCacheLimitsV2 { max_retained_bytes: 1 << 16, max_provider_attempts: 2 };
        let mut old = PanelSourceCacheV2::new(&result, service.clone(), limits).unwrap();
        open_source_request(&old, &mut node, &author);
        let mut miner = SourceProvider { response: Some(chunk), calls: 0 };
        assert_eq!(old.acquire_one(&mut node, 0, &mut [&mut miner], &mut || 3), Ok(SourceAcquisitionV2::Delivered { provider: 0 }));
        let charged = node.service_meter();
        let mut replacement = PanelSourceCacheV2::new(&result, service.clone(), limits).unwrap();
        for entry in old.authenticated_entries() {
            replacement.import_authenticated(entry.clone()).unwrap();
        }
        assert_eq!(replacement.acquire_one(&mut node, 0, &mut [&mut miner], &mut || 4), Ok(SourceAcquisitionV2::Cached));
        assert_eq!(node.service_meter(), charged);
        assert_eq!(miner.calls, 1);
        assert!(replacement.next_missing_request(0, 1, 1).unwrap().is_none());
        let mut wrong_result = result.clone();
        wrong_result.branch = Hash64::from_u64_word(999);
        let mut wrong_cache = PanelSourceCacheV2::new(&wrong_result, service, limits).unwrap();
        assert_eq!(wrong_cache.acquire_one(&mut node, 0, &mut [&mut miner], &mut || 4), Err(ErrorV2::Binding));
        assert_eq!(node.service_meter(), charged);
    });
}

#[test]
fn source_fetch_that_crosses_deadline_uses_fresh_clock_and_cannot_complete_delivery() {
    use misaka_palw_remote::pursuit::positive::sources::*;
    with_fixture(|result, _, _, class, service| {
        let (result, service, chunk) = source_fixture(result, service, 2);
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let mut cache =
            PanelSourceCacheV2::new(&result, service, SourceCacheLimitsV2 { max_retained_bytes: 1 << 16, max_provider_attempts: 2 })
                .unwrap();
        open_source_request(&cache, &mut node, &author);
        let charged = node.service_meter();
        let mut miner = SourceProvider { response: Some(chunk), calls: 0 };
        // Entry and pre-fetch are before the deadline; the provider returns at
        // the exclusive deadline, and the post-fetch clock must reject it.
        let mut times = [3, 3, 100].into_iter();
        let mut clock = || times.next().unwrap();
        assert_eq!(cache.acquire_one(&mut node, 0, &mut [&mut miner], &mut clock), Err(ErrorV2::Deadline));
        assert_eq!(miner.calls, 1);
        assert_eq!(node.service_meter(), charged);
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Pending);
        assert_eq!(node.completion().claim, ClaimStatusV2::Checking);
        assert_eq!(cache.cached_chunks(), 0);
    });
}

#[test]
fn imported_source_is_local_data_until_delivered_to_canonical_node() {
    use misaka_palw_remote::pursuit::positive::sources::*;
    with_fixture(|result, _, _, class, service| {
        let (result, service, chunk) = source_fixture(result, service, 2);
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let mut cache =
            PanelSourceCacheV2::new(&result, service, SourceCacheLimitsV2 { max_retained_bytes: 1 << 16, max_provider_attempts: 2 })
                .unwrap();
        cache.import_authenticated(chunk).unwrap();
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::NotRequested);
        open_source_request(&cache, &mut node, &author);
        let charged = node.service_meter();
        assert_eq!(cache.acquire_one(&mut node, 0, &mut [], &mut || 3), Ok(SourceAcquisitionV2::Cached));
        assert_eq!(node.service_meter(), charged);
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Pending);
        cache.deliver_cached(&mut node, 0, 3).unwrap();
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Complete);
        assert_eq!(node.service_meter().paid, 1);
        assert_eq!(node.completion().claim, ClaimStatusV2::Checking);
        assert_eq!(node.completion().witness, None);
    });
}

#[test]
fn cold_panel_acquires_existing_container_then_replays_and_submits_complete_witness() {
    use kaspa_consensus_core::{
        mldsa87_primitives::mldsa87_key_id,
        palw_positive_court_v2::service::{WorkRequestV2, chunk_hash_v2},
        palw_step_leg::{step_merkle_root_v1, step_opening_v1},
    };
    use misaka_palw_remote::pursuit::positive::sources::*;
    use misaka_palw_tir_exec::node::positive_court::open_panel_artifact_v2;
    struct TempDir(std::path::PathBuf);
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    struct ContainerProvider(std::collections::BTreeMap<u64, SourceChunkV2>);
    impl PanelSourceProviderV2 for ContainerProvider {
        fn fetch_source(&mut self, index: u64, _max_siblings: usize) -> Result<Option<SourceChunkV2>, String> {
            Ok(self.0.remove(&index))
        }
    }
    let nonce = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = TempDir(std::env::temp_dir().join(format!("misaka-a3-cold-panel-{}-{nonce}", std::process::id())));
    std::fs::create_dir(&dir.0).unwrap();
    with_fixture(|original_result, _, _, class, mut service| {
        let program = class.decode_program().unwrap();
        let source_path = dir.0.join("provider.palwtir");
        let raw_weights: Vec<u8> = [1i8, 9, 3, 2, 5, 5, 1, -2, -3, -2, -1, 0, 0, 7, 10, 2].into_iter().map(|n| n as u8).collect();
        misaka_palw_tir_artifact::write_container_v1(
            &source_path,
            &program,
            borsh::to_vec(&class.layout).unwrap(),
            class.tokenizer_id.as_bytes(),
            String::new(),
            &mut |param, layer| {
                assert_eq!((param, layer), (0, None));
                Ok(raw_weights.clone())
            },
        )
        .unwrap();
        let mut source_bytes = std::fs::read(&source_path).unwrap();
        let count = source_bytes.len().div_ceil(CHUNK_BYTES) as u64;
        source_bytes.resize(count as usize * CHUNK_BYTES, 0);
        let hashes: Vec<_> = source_bytes
            .chunks_exact(CHUNK_BYTES)
            .enumerate()
            .map(|(index, bytes)| chunk_hash_v2(&original_result.claim, index as u64, bytes))
            .collect();
        service.source = step_merkle_root_v1(&hashes).unwrap();
        service.chunks = count;
        service.max_unique_bytes = count * CHUNK_BYTES as u64;
        service.max_rounds = 1;
        service.max_transmitted_bytes = count * (CHUNK_BYTES as u64 + 76 + 64 * count);
        service.max_work = count;
        service.hold = count;
        let mut result = original_result.clone();
        result.service_policy = service.id();
        let author = Key::new(82);
        let mut node = coordinator(&result, class, service.clone(), &author);
        let mut cache =
            PanelSourceCacheV2::new(&result, service, SourceCacheLimitsV2 { max_retained_bytes: 1 << 20, max_provider_attempts: 1 })
                .unwrap();
        let mut unpublished = Vec::new();
        assert_eq!(cache.write_complete_source(&mut unpublished).unwrap_err().kind(), std::io::ErrorKind::UnexpectedEof);
        assert!(unpublished.is_empty());
        let request = WorkRequestV2::Source { first: 0, count };
        let signature =
            SignatureV2(author.sign_with_context(node.request_id(request).unwrap().as_byte_slice(), SOURCE_REQUEST_CONTEXT).unwrap());
        cache.request(&mut node, mldsa87_key_id(&author.public_key()), request, &signature, 2).unwrap();
        let mut provider = ContainerProvider(
            source_bytes
                .chunks_exact(CHUNK_BYTES)
                .enumerate()
                .map(|(index, bytes)| {
                    let bytes: Box<[u8; CHUNK_BYTES]> = Box::new(bytes.try_into().unwrap());
                    (index as u64, SourceChunkV2 { bytes, opening: step_opening_v1(&hashes, index as u64).unwrap() })
                })
                .collect(),
        );
        for index in 0..count {
            assert_eq!(
                cache.acquire_one(&mut node, index, &mut [&mut provider], &mut || 3),
                Ok(SourceAcquisitionV2::Delivered { provider: 0 })
            );
        }
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Complete);
        assert_eq!(node.completion().witness, None);
        let panel_path = dir.0.join("panel-acquired.palwtir");
        let mut file = std::fs::File::create(&panel_path).unwrap();
        assert_eq!(cache.write_complete_source(&mut file).unwrap(), source_bytes.len() as u64);
        std::io::Write::flush(&mut file).unwrap();
        drop(file);
        std::fs::remove_file(&source_path).unwrap();
        let file_cap = source_bytes.len() as u64;
        assert!(open_panel_artifact_v2(&panel_path, &result, class, file_cap - 1, 1 << 20).is_err());
        assert!(open_panel_artifact_v2(&panel_path, &result, class, file_cap, 1).is_err());
        let mut wrong_result = result.clone();
        wrong_result.artifact = Hash64::from_u64_word(999);
        assert!(open_panel_artifact_v2(&panel_path, &wrong_result, class, file_cap, 1 << 20).is_err());
        let mut wrong_class = class.clone();
        wrong_class.layout.checkpoint_interval += 1;
        assert!(open_panel_artifact_v2(&panel_path, &result, &wrong_class, file_cap, 1 << 20).is_err());
        // The signed transport root alone is insufficient: actual parameter
        // bytes must also open the registered model artifact.
        let forged_path = dir.0.join("forged-weights.palwtir");
        misaka_palw_tir_artifact::write_container_v1(
            &forged_path,
            &program,
            borsh::to_vec(&class.layout).unwrap(),
            class.tokenizer_id.as_bytes(),
            String::new(),
            &mut |_, _| Ok(vec![11; 16]),
        )
        .unwrap();
        assert!(open_panel_artifact_v2(&forged_path, &result, class, 1 << 20, 1 << 20).is_err());
        let artifact = open_panel_artifact_v2(&panel_path, &result, class, file_cap, 1 << 20).unwrap();
        let space = PalwTirStepSpaceV1::new(class).unwrap();
        let runner = TirClassRunnerV1::new(&space, artifact.plan(), artifact.params(), result.job.shape_profile_id).unwrap();
        let panel = PanelCaptureV2::replay(
            &runner,
            class,
            result.artifact,
            &result.job,
            &[0],
            PalwPromptIdsFormV1::Flat,
            RoleLimitsV2 { max_prefill: 4, max_decode: 4, max_leaves: 1 << 12, max_capture_bytes: 1 << 26 },
        )
        .unwrap();
        assert_eq!(panel.generated(), &[1, 0, 1]);
        let witness = panel.witness(&result, policy(), 0).unwrap();
        let candidate = PanelCandidateV2::sign(&result, scope(&result), policy(), witness.clone(), &author).unwrap();
        candidate.open(&mut node, 4).unwrap();
        let mut batch = candidate.batch(0, witness.binding.step_leaf_count, send_limits()).unwrap();
        panel
            .stream_parts(&runner, &artifact, &rules(), policy().max_part_bytes, &mut |index, part| {
                batch.push(index, &part).map_err(|e| format!("{e:?}"))
            })
            .unwrap();
        batch.finish().unwrap().submit(&mut node, 5).unwrap();
        assert_eq!(node.finish(6), Ok(ClaimStatusV2::Passed));
        assert_eq!(node.completion().miner_data, DataResponseStatusV2::Complete);
    });
}
