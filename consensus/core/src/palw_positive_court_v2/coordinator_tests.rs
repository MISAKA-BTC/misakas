use super::*;
use crate::palw_positive_court_v2::{
    gate::ClaimFormV2,
    service::{CHUNK_BYTES, chunk_hash_v2},
    tests::{Signed, parts, policy, result},
};
use crate::palw_step_leg::{step_merkle_root_v1, step_opening_v1};
use crate::palw_tir_court_v1::test_support::{TinyExecution, tiny_execution};

fn panel(seed: u8) -> (Hash64, Vec<u8>) {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    let bytes = key.verification_key.as_ref().to_vec();
    (mldsa87_key_id(&bytes), bytes)
}
fn sign(seed: u8, id: Hash64, context: &[u8]) -> SignatureV2 {
    let key = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed; 32]);
    SignatureV2(
        libcrux_ml_dsa::ml_dsa_87::sign(&key.signing_key, id.as_byte_slice(), context, [seed.wrapping_add(1); 32])
            .unwrap()
            .as_ref()
            .to_vec(),
    )
}
fn setup(x: &TinyExecution) -> (Signed, ServicePolicyV2, Vec<u8>, CoordinatorLimitsV2) {
    let bytes = vec![7; CHUNK_BYTES];
    let mut result = result(x);
    let service = ServicePolicyV2 {
        version: 2,
        case: result.claim,
        source: step_merkle_root_v1(&[chunk_hash_v2(&result.claim, 0, &bytes)]).unwrap(),
        chunks: 1,
        max_rounds: 1,
        max_unique_bytes: CHUNK_BYTES as u64,
        max_transmitted_bytes: CHUNK_BYTES as u64 * 4,
        work_per_chunk: 1,
        max_work: 4,
        retention_until: 120,
        deadline: 100,
        price_per_chunk: 10,
        hold: 20,
    };
    result.service_policy = service.id();
    let signed = Signed::new(x, result, policy());
    let limits = CoordinatorLimitsV2 {
        max_attempts: 2,
        max_reserved_elements: 1 << 45,
        max_reserved_terms: 1 << 45,
        max_reserved_witness_bytes: 128 << 20,
        max_history_bytes: 1 << 20,
        max_batch_parts: 1 << 20,
        max_batch_bytes: 64 << 20,
    };
    (signed, service, bytes, limits)
}
fn coordinator(x: &TinyExecution, signed: &Signed, service: ServicePolicyV2, limits: CoordinatorLimitsV2) -> PositiveCoordinatorV2 {
    PositiveCoordinatorV2::admit(
        AdmissionScopeV2 {
            network: signed.result.network,
            branch: signed.result.branch,
            ruleset: signed.result.ruleset,
            registered_class_id: x.binding.job_context.shape_profile_id,
            required_form: ClaimFormV2::MinerResultV2,
        },
        signed.result.clone(),
        signed.miner_signature.clone(),
        signed.miner_key.clone(),
        x.binding.class.clone(),
        signed.policy,
        PalwPromptIdsFormV1::Flat,
        PanelRosterV2::new(vec![panel(32).1, panel(42).1], 2 * MLDSA87_PUBKEY_LEN as u64).unwrap(),
        service,
        limits,
        1,
    )
    .unwrap()
}
fn open(c: &mut PositiveCoordinatorV2, s: &Signed) {
    assert_eq!(c.open_candidate(panel(32).0, s.witness.clone(), &s.panel_signature, 2), Ok(CandidateEffectV2::Opened));
}
fn request(c: &mut PositiveCoordinatorV2, seed: u8) -> Result<RequestEffectV2, ErrorV2> {
    let request = WorkRequestV2::Source { first: 0, count: 1 };
    let signature = sign(seed, c.request_id(request).unwrap(), SOURCE_REQUEST_CONTEXT);
    c.request_source(panel(seed).0, request, &signature, 2)
}
fn response(c: &mut PositiveCoordinatorV2, bytes: &[u8]) {
    let leaf = chunk_hash_v2(&c.result().claim, 0, bytes);
    c.respond_source(0, bytes, &step_opening_v1(&[leaf], 0).unwrap(), 3).unwrap();
}
fn submit(c: &mut PositiveCoordinatorV2, x: &TinyExecution) {
    let parts = parts(x, policy());
    let encoded: Vec<_> = parts.iter().map(|(cone, trace)| EncodedPartV2 { cone, trace: trace.as_deref() }).collect();
    c.submit_batch(c.current().unwrap().witness().id(), 0, &encoded, 4).unwrap();
}

#[test]
fn warm_panel_completes_without_any_miner_data_request() {
    let x = tiny_execution(None);
    let (s, p, _, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    open(&mut c, &s);
    submit(&mut c, &x);
    assert_eq!(c.finish(5), Ok(ClaimStatusV2::Passed));
    assert_eq!(
        c.completion(),
        CompletionV2 {
            miner_data: DataResponseStatusV2::NotRequested,
            witness: Some(WitnessStatusV2::Complete),
            claim: ClaimStatusV2::Passed
        }
    );
    assert_eq!(c.service_meter().paid, 0);
    assert_eq!(c.service_meter().refunded, 20);
}

#[test]
fn miner_data_completion_survives_panel_rejection_and_key_change() {
    let x = tiny_execution(None);
    let (s, p, bytes, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    assert_eq!(request(&mut c, 32), Ok(RequestEffectV2::Opened));
    response(&mut c, &bytes);
    open(&mut c, &s);
    let bad = [EncodedPartV2 { cone: &[], trace: None }];
    assert_eq!(c.submit_batch(s.witness.id(), 0, &bad, 4), Err(ErrorV2::Wire));
    assert_eq!(c.completion().claim, ClaimStatusV2::Checking);
    assert_eq!(c.completion().witness, Some(WitnessStatusV2::Rejected));
    assert_eq!(c.completion().miner_data, DataResponseStatusV2::Complete);
    assert_eq!(request(&mut c, 42), Ok(RequestEffectV2::Coalesced));
    assert_eq!(c.service_meter().unique_bytes, CHUNK_BYTES as u64);
    assert_eq!(c.service_meter().paid, 10);
    let mut retry = s.witness.clone();
    retry.attempt = 1;
    assert_eq!(c.open_candidate(panel(42).0, retry.clone(), &sign(42, retry.id(), WITNESS_CONTEXT), 5), Ok(CandidateEffectV2::Opened));
    assert_eq!(c.history().len(), 1);
    assert_eq!(c.history()[0].status, WitnessStatusV2::Rejected);
    assert_eq!(c.attempts().attempts, 2);
    submit(&mut c, &x);
    assert_eq!(c.finish(6), Ok(ClaimStatusV2::Passed));
    assert_eq!(c.service_meter().paid, 10);
    assert_eq!(c.service_meter().refunded, 10);
}

#[test]
fn no_second_active_candidate_or_stale_ordinal_can_reset_the_case() {
    let x = tiny_execution(None);
    let (s, p, _, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    open(&mut c, &s);
    assert_eq!(c.open_candidate(panel(32).0, s.witness.clone(), &s.panel_signature, 3), Ok(CandidateEffectV2::AlreadyActive));
    let mut second = s.witness.clone();
    second.attempt = 1;
    assert_eq!(c.open_candidate(panel(42).0, second.clone(), &sign(42, second.id(), WITNESS_CONTEXT), 3), Err(ErrorV2::Order));
    assert_eq!(c.attempts().attempts, 1);
    assert_eq!(c.current().unwrap().witness().id(), s.witness.id());
}

#[test]
fn signed_malformed_admission_consumes_attempt_but_does_not_convict_miner() {
    let x = tiny_execution(None);
    let (s, p, _, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    let mut bad = s.witness.clone();
    bad.binding.step_leaf_count += 1;
    assert_eq!(c.open_candidate(panel(32).0, bad.clone(), &sign(32, bad.id(), WITNESS_CONTEXT), 2), Err(ErrorV2::Binding));
    assert_eq!(c.attempts().attempts, 1);
    assert_eq!(c.history()[0].status, WitnessStatusV2::Rejected);
    assert_eq!(c.completion().claim, ClaimStatusV2::Checking);
    assert_eq!(c.open_candidate(panel(32).0, s.witness.clone(), &s.panel_signature, 3), Err(ErrorV2::Order));
    let mut retry = s.witness.clone();
    retry.attempt = 1;
    assert_eq!(c.open_candidate(panel(42).0, retry.clone(), &sign(42, retry.id(), WITNESS_CONTEXT), 4), Ok(CandidateEffectV2::Opened));
    c.submit_batch(retry.id(), 0, &[EncodedPartV2 { cone: &[], trace: None }], 5).unwrap_err();
    let mut excess = s.witness.clone();
    excess.attempt = 2;
    assert_eq!(c.open_candidate(panel(32).0, excess.clone(), &sign(32, excess.id(), WITNESS_CONTEXT), 6), Err(ErrorV2::Budget));
    assert_eq!(c.completion().claim, ClaimStatusV2::Checking);
}

#[test]
fn node_pass_does_not_forgive_pending_source_duties_and_direct_reply_still_works() {
    let x = tiny_execution(None);
    let (s, p, bytes, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    request(&mut c, 32).unwrap();
    open(&mut c, &s);
    submit(&mut c, &x);
    c.finish(5).unwrap();
    assert_eq!(c.completion().miner_data, DataResponseStatusV2::Pending);
    assert_eq!(c.service_meter().remaining, 20);
    assert_eq!(request(&mut c, 42), Err(ErrorV2::Closed));
    let leaf = chunk_hash_v2(&c.result().claim, 0, &bytes);
    c.respond_source(0, &bytes, &step_opening_v1(&[leaf], 0).unwrap(), 6).unwrap();
    assert_eq!(c.completion().miner_data, DataResponseStatusV2::Complete);
    assert_eq!(c.completion().claim, ClaimStatusV2::Passed);
    assert_eq!(c.service_meter().paid, 10);
    assert_eq!(c.service_meter().refunded, 10);
}

#[test]
fn deadline_refunds_pending_source_even_if_computation_already_passed() {
    let x = tiny_execution(None);
    let (s, p, _, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    request(&mut c, 32).unwrap();
    open(&mut c, &s);
    submit(&mut c, &x);
    c.finish(5).unwrap();
    c.expire(100);
    assert_eq!(c.completion().claim, ClaimStatusV2::Passed);
    assert_eq!(c.completion().miner_data, DataResponseStatusV2::Pending);
    assert_eq!(c.service_meter().paid, 0);
    assert_eq!(c.service_meter().refunded, 20);
    c.expire(101);
    assert_eq!(c.service_meter().refunded, 20);
}

#[test]
fn panel_abandonment_after_miner_reply_ends_unverified_without_miner_fraud() {
    let x = tiny_execution(None);
    let (s, p, bytes, limits) = setup(&x);
    let mut c = coordinator(&x, &s, p, limits);
    request(&mut c, 32).unwrap();
    response(&mut c, &bytes);
    open(&mut c, &s);
    c.expire(100);
    assert_eq!(
        c.completion(),
        CompletionV2 {
            miner_data: DataResponseStatusV2::Complete,
            witness: Some(WitnessStatusV2::Incomplete),
            claim: ClaimStatusV2::EndedUnverified
        }
    );
    assert_eq!(c.service_meter().paid, 10);
    assert_eq!(c.service_meter().refunded, 10);
}

#[test]
fn replaying_authenticated_prefix_rebuilds_service_and_attempt_budgets() {
    let x = tiny_execution(None);
    let (s, p, bytes, limits) = setup(&x);
    let replay = |c: &mut PositiveCoordinatorV2| {
        request(c, 32).unwrap();
        response(c, &bytes);
        open(c, &s);
        c.submit_batch(s.witness.id(), 0, &[EncodedPartV2 { cone: &[], trace: None }], 4).unwrap_err();
        let mut retry = s.witness.clone();
        retry.attempt = 1;
        c.open_candidate(panel(42).0, retry.clone(), &sign(42, retry.id(), WITNESS_CONTEXT), 5).unwrap();
    };
    let mut original = coordinator(&x, &s, p.clone(), limits);
    replay(&mut original);
    let mut restored = coordinator(&x, &s, p, limits);
    replay(&mut restored);
    assert_eq!(original.completion(), restored.completion());
    assert_eq!(original.service_meter(), restored.service_meter());
    assert_eq!(original.attempts(), restored.attempts());
    assert_eq!(original.history(), restored.history());
    assert_eq!(request(&mut restored, 42), Ok(RequestEffectV2::Coalesced));
    assert_eq!(restored.service_meter().paid, 10);
    submit(&mut restored, &x);
    assert_eq!(restored.finish(6), Ok(ClaimStatusV2::Passed));
}

#[test]
fn all_attempt_resources_must_be_reserved_before_first_candidate() {
    let x = tiny_execution(None);
    let (s, p, _, mut limits) = setup(&x);
    limits.max_reserved_witness_bytes = 1;
    let result = PositiveCoordinatorV2::admit(
        AdmissionScopeV2 {
            network: s.result.network,
            branch: s.result.branch,
            ruleset: s.result.ruleset,
            registered_class_id: x.binding.job_context.shape_profile_id,
            required_form: ClaimFormV2::MinerResultV2,
        },
        s.result,
        s.miner_signature,
        s.miner_key,
        x.binding.class,
        s.policy,
        PalwPromptIdsFormV1::Flat,
        PanelRosterV2::new(vec![panel(32).1], MLDSA87_PUBKEY_LEN as u64).unwrap(),
        p,
        limits,
        1,
    );
    assert!(matches!(result, Err(ErrorV2::Budget)));
}
