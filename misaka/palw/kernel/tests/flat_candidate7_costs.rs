//! Staged metadata tariff, admission reserve and invalid-attempt debits. No
//! source tensor, complete model or execution/reward acceptance is exercised.
mod common;
use common::{ledger_world::policy, opv_world::opv_example};
use misaka_palw_kernel::{
    descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v4_descriptor},
    hash::Digest,
    ledger::{AuthV1, BlockBudgetV1, KernelLedgerV1, flat_candidate7::*},
    route::KernelRouteObjectV1 as K,
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::{
    DType, Ref, TensorType, TirProgramV1,
    builder::ProgramBuilder,
    program::{HISTORY_BOUND_V1_HELD, MAX_LAYERS, MAX_NODES_PER_BLOCK},
};
const OWNER: Digest = [1; 64];
fn repeated(nodes: usize, layers: usize) -> TirProgramV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let mut pre = b.block("pre", vec![]);
    let x = pre.cast(Ref::Input(0), DType::I32);
    let pre = pre.finish(&[x]);
    let mut body = b.block("body", vec![TensorType::fixed(DType::I32, &[])]);
    let mut x = Ref::CarryIn(0);
    for _ in 0..nodes {
        x = body.clamp(x, -10, 10, DType::I32);
    }
    let body = body.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    b.finish(pre, vec![body; layers], post, 0)
}
fn ledger() -> KernelLedgerV1 {
    let d = k2_tir_v4_descriptor();
    let mut p = policy();
    p.max_court_work_per_block = 1 << 30;
    let mut l =
        KernelLedgerV1::genesis(p, KernelScheduleV1::default().with(d.digest(), KernelStatusV1::Active { since_daa: 0 }), vec![d])
            .unwrap()
            .with_opv_policy(opv_example())
            .unwrap();
    l.sync_bond(OWNER, 1_000_000);
    l.attest_artifact(ParamCommitmentsV1::default().root());
    l.begin_block(1).unwrap();
    l
}
fn object(program: &TirProgramV1) -> K {
    let bytes = program.encode();
    let pc = ParamCommitmentsV1::default();
    let v = derive_flat_candidate_v7(&k2_tir_v4_descriptor().digest(), &bytes, &pc, 1).unwrap();
    K::RegisterFlatModelCandidateV7 {
        binding: [9; 64],
        descriptor: v.plan.descriptor,
        program_bytes: bytes,
        param_commitments: pc,
        max_positions: 1,
        scope_root: v.plan.scope_root,
        replay_root: v.plan.replay_root,
    }
}
fn cost(program: &TirProgramV1) -> (u64, u64) {
    (flat_candidate_registration_work_v7(program.encode().len(), 0).unwrap(), flat_candidate_metadata_work_v7(program).unwrap())
}
#[test]
fn staged_cost_is_exact_schedule_count_and_never_allocates_tensor_extent() {
    let p = repeated(2, 1);
    let q = repeated(2, MAX_LAYERS);
    let expected = |layers: usize| (2 + 2 * layers) as u64 * FLAT_CANDIDATE_METADATA_PASSES_V7 * 1024;
    assert_eq!(flat_candidate_metadata_work_v7(&p), Some(expected(1)));
    assert_eq!(flat_candidate_metadata_work_v7(&q), Some(expected(MAX_LAYERS)));
    assert!(cost(&q).1 > cost(&p).1 * 500, "a repeated schedule pays its actual occurrences");
    let max = repeated(MAX_NODES_PER_BLOCK, MAX_LAYERS);
    TirProgramV1::decode_canonical(&max.encode()).unwrap();
    assert_eq!(flat_candidate_metadata_work_v7(&max), Some((2 + 512 * 1024) * FLAT_CANDIDATE_METADATA_PASSES_V7 * 1024));
    assert!(flat_candidate_registration_work_v7(usize::MAX, 1).is_none());
    assert!(flat_candidate_registration_work_v7(1, usize::MAX).is_none());
    let mut bad = p.clone();
    bad.schedule.layers[0] = u8::MAX;
    assert!(flat_candidate_metadata_work_v7(&bad).is_none());
    bad = p.clone();
    bad.schedule.layers.resize(MAX_LAYERS + 1, 1);
    assert!(flat_candidate_metadata_work_v7(&bad).is_none());
    bad = p;
    bad.blocks[1].nodes.clear();
    assert!(flat_candidate_metadata_work_v7(&bad).is_none());
}
#[test]
fn two_debits_are_one_attempt_and_leave_ordinary_proof_reserve() {
    let p = repeated(2, 4);
    let (pre, metadata) = cost(&p);
    let mut l = ledger();
    l.apply_object(&object(&p), &AuthV1 { signer_bond: OWNER }).unwrap();
    assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: 1, court_work: pre + metadata });
    let remaining = l.policy.admission_work_limit_v1() - l.budget_used().court_work;
    let filled = l.budget_used().charged_v1(&l.policy, remaining, false).unwrap();
    assert_eq!(filled.court_work, l.policy.admission_work_limit_v1());
    assert!(filled.charged_v1(&l.policy, 1, false).is_none());
    let proof = filled.charged_v1(&l.policy, l.policy.guaranteed_proof_work_v1(), true).unwrap();
    assert_eq!(proof.court_work, l.policy.max_court_work_per_block);
}
#[test]
fn malformed_canonical_decode_pays_only_first_debit_and_consumes_retry() {
    let p = repeated(2, 4);
    let mut o = object(&p);
    let K::RegisterFlatModelCandidateV7 { program_bytes, .. } = &mut o else { unreachable!() };
    program_bytes.push(0);
    let pre = flat_candidate_registration_work_v7(program_bytes.len(), 0).unwrap();
    let mut l = ledger();
    let root = l.root();
    for attempt in 1..=2 {
        assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
        assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: attempt, court_work: pre * attempt as u64 });
        assert_eq!(l.root(), root);
        assert!(l.flat_conformance_classes_v7.is_empty());
    }
}
#[test]
fn scheduled_work_is_reserved_before_scope_or_range_and_shared_budget_refusals_keep_parse_debit() {
    let p = repeated(2, 4);
    let (pre, metadata) = cost(&p);
    let mut o = object(&p);
    let K::RegisterFlatModelCandidateV7 { scope_root, .. } = &mut o else { unreachable!() };
    scope_root[0] ^= 1;
    let mut l = ledger();
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
    assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: 1, court_work: pre + metadata });
    let mut l = ledger();
    let used = l.policy.admission_work_limit_v1() - pre - metadata + 1;
    l.restore_budget(BlockBudgetV1 { adjudications: 0, court_work: used });
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).unwrap_err().why.contains("metadata"));
    assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: 1, court_work: used + pre });
    assert!(l.flat_conformance_classes_v7.is_empty());
    // This canonical maximum-layer schedule passes the byte tariff but its
    // actual unrolled metadata does not fit; scope/range derivation cannot run.
    let p = repeated(64, MAX_LAYERS);
    let (pre, metadata) = cost(&p);
    let mut l = ledger();
    assert!(pre < l.policy.admission_work_limit_v1());
    assert!(pre + metadata > l.policy.admission_work_limit_v1());
    let o = K::RegisterFlatModelCandidateV7 {
        binding: [9; 64],
        descriptor: k2_tir_v4_descriptor().digest(),
        program_bytes: p.encode(),
        param_commitments: ParamCommitmentsV1::default(),
        max_positions: 0,
        scope_root: [0; 64],
        replay_root: [0; 64],
    };
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).unwrap_err().why.contains("admission"));
    assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: 1, court_work: pre });
}
#[test]
fn invalid_artifact_hash_attempt_is_paid_before_authentication() {
    let p = repeated(2, 4);
    let mut o = object(&p);
    let K::RegisterFlatModelCandidateV7 { param_commitments, .. } = &mut o else { unreachable!() };
    param_commitments.by_instance.insert((0, Some(u16::MAX)), [3; 64]);
    let pre = flat_candidate_registration_work_v7(p.encode().len(), 1).unwrap();
    let mut l = ledger();
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).unwrap_err().why.contains("authenticated"));
    assert_eq!(l.budget_used(), BlockBudgetV1 { adjudications: 1, court_work: pre });
    assert!(l.flat_conformance_classes_v7.is_empty());
}
