//! Synthetic metadata admission. Large dimensions do not execute a checkpoint
//! or allocate a max-length trace, and candidates carry no execution rights.
mod common;
use common::{ledger_world::policy, opv_world::opv_example};
use misaka_palw_kernel::{
    descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v4_descriptor},
    hash::Digest,
    job::{DecodeRuleV1, KernelJobV1},
    ledger::{AuthV1, BlockBudgetV1, KernelLedgerV1, LedgerEventV1, flat_candidate7::*},
    plan::plan_for_tir_program_v1,
    public::program_root_v1,
    route::KernelRouteObjectV1 as K,
    rows::{TABLE_FLAT_CONFORMANCE_CLASSES_V7, diff_rows},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::{DType, TensorType, TirProgramV1, builder::ProgramBuilder, program::HISTORY_BOUND_V1_HELD};
const OWNER: Digest = [1; 64];
fn program(n: u32) -> TirProgramV1 {
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let scores = b.param("independent.scores", DType::I32, &[n], false);
    let mut pre = b.block("topk", vec![]);
    let x = pre.topk(scores, 0, 2);
    let x = pre.cast(x, DType::I32);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let y = post.cast(misaka_palw_tir::Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    b.finish(pre, vec![], post, 0)
}
fn params() -> ParamCommitmentsV1 {
    // Metadata-only source commitment, not authenticated tensor execution.
    ParamCommitmentsV1 { by_instance: [((0, None), [8; 64])].into() }
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
    l.attest_artifact(params().root());
    l.begin_block(1).unwrap();
    l
}
fn object(p: &TirProgramV1, max: u32) -> K {
    let v = derive_flat_candidate_v7(&k2_tir_v4_descriptor().digest(), &p.encode(), &params(), max).unwrap();
    K::RegisterFlatModelCandidateV7 {
        binding: [9; 64],
        descriptor: v.plan.descriptor,
        program_bytes: p.encode(),
        param_commitments: params(),
        max_positions: max,
        scope_root: v.plan.scope_root,
        replay_root: v.plan.replay_root,
    }
}
fn apply(l: &mut KernelLedgerV1, o: &K) -> Digest {
    let e = l.apply_object(o, &AuthV1 { signer_bond: OWNER }).unwrap();
    let [LedgerEventV1::FlatConformanceClassRegisteredV7 { class }] = e.as_slice() else { panic!("{e:?}") };
    *class
}
#[test]
fn flat_candidate7_identity_rows_rollback_and_no_execution_rights() {
    let mut l = ledger();
    let p = program(65);
    let o = object(&p, 128);
    assert_eq!(&o.encode()[..2], &[1, 40]);
    assert_eq!(K::decode(&o.encode()).unwrap(), o);
    let before = l.to_rows();
    let root = l.root();
    let bonds = l.bonds.clone();
    let class = apply(&mut l, &o);
    let rows = l.to_rows();
    assert_ne!(root, l.root());
    assert_eq!(l.bonds, bonds);
    assert!(l.classes.is_empty() && l.conformance_classes.is_empty() && l.opv.classes.is_empty() && l.opv.admitted.is_empty());
    assert!(l.claims.is_empty() && l.jobs.is_empty());
    let (_, record) = &l.flat_conformance_classes_v7[&class];
    assert_eq!(record.id(), class);
    record.checked().unwrap();
    assert!(l.budget_used().court_work > 0);
    let restored = KernelLedgerV1::from_rows(&l, l.scalars(), &rows).unwrap();
    assert_eq!(restored.to_rows(), rows);
    assert_eq!(restored.root(), l.root());
    let mut no_new = l.clone();
    no_new.flat_conformance_classes_v7.clear();
    assert_eq!(no_new.root(), root, "absent v7 rows preserve the historical root");
    let mut rolled = rows.clone();
    for (key, old, _) in diff_rows(&before, &rows) {
        if let Some(v) = old {
            rolled.insert(key, v);
        } else {
            rolled.remove(&key);
        }
    }
    assert_eq!(rolled, before);
    let job =
        KernelJobV1 { class_binding_id: class, prompt: vec![0], max_new_tokens: 1, decode: DecodeRuleV1::Greedy, nonce: [1; 64] };
    assert!(l.apply_object(&K::PostJob { job }, &AuthV1 { signer_bond: OWNER }).is_err());
    let mut withdrawing = restored;
    withdrawing.apply_object(&K::RequestExit { bond: OWNER }, &AuthV1 { signer_bond: OWNER }).unwrap();
    withdrawing.begin_block(100).unwrap();
    withdrawing.apply_object(&K::Withdraw { bond: OWNER }, &AuthV1 { signer_bond: OWNER }).unwrap();
    assert!(withdrawing.flat_conformance_classes_v7.is_empty());
    KernelLedgerV1::from_rows(&withdrawing, withdrawing.scalars(), &withdrawing.to_rows()).unwrap();
}
#[test]
fn flat_candidate7_long_topk_metadata_admission_is_separate_from_legacy_plan() {
    let p = program(10_485_760);
    let o = object(&p, HISTORY_BOUND_V1_HELD);
    let mut l = ledger();
    let class = apply(&mut l, &o);
    let v = l.flat_conformance_classes_v7[&class].1.checked().unwrap();
    assert_eq!(v.plan.version, 7);
    assert!(v.scope.worst_court_bytes < 1 << 20);
    assert_eq!(v.replay.topk_auxiliary_bytes, 128);
    let d = k2_tir_v4_descriptor();
    let old = K::RegisterConformanceClass {
        descriptor: d.digest(),
        program_bytes: p.encode(),
        param_commitments: params(),
        plan: plan_for_tir_program_v1(&d, &p, program_root_v1(&p.encode()), HISTORY_BOUND_V1_HELD).unwrap(),
    };
    l.begin_block(2).unwrap();
    assert!(l.apply_object(&old, &AuthV1 { signer_bond: OWNER }).is_err());
    assert!(l.conformance_classes.is_empty());
    eprintln!(
        "FLAT_CANDIDATE7_METADATA N=10485760 max_positions={} program_bytes={} predecode_work={} scheduled_metadata_work={} total_registration_work={} court_bytes={} court_work={} auxiliary_bytes={} no_weights_or_job_executed=true",
        HISTORY_BOUND_V1_HELD,
        p.encode().len(),
        flat_candidate_registration_work_v7(p.encode().len(), 1).unwrap(),
        flat_candidate_metadata_work_v7(&p).unwrap(),
        flat_candidate_registration_work_v7(p.encode().len(), 1).unwrap() + flat_candidate_metadata_work_v7(&p).unwrap(),
        v.scope.worst_court_bytes,
        v.scope.worst_court_work,
        v.replay.topk_auxiliary_bytes
    );
}
#[test]
fn flat_candidate7_shared_old_new_catalog_and_block_budget_cannot_be_bypassed() {
    let p = program(65);
    let d = k2_tir_v4_descriptor();
    let mut l = ledger();
    for i in 1..=8 {
        l.begin_block(i).unwrap();
        if i % 2 == 0 {
            apply(&mut l, &object(&p, i as u32));
        } else {
            let o = K::RegisterConformanceClass {
                descriptor: d.digest(),
                program_bytes: p.encode(),
                param_commitments: params(),
                plan: plan_for_tir_program_v1(&d, &p, program_root_v1(&p.encode()), i as u32).unwrap(),
            };
            l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).unwrap();
        }
    }
    assert_eq!(l.conformance_classes.len(), 4);
    assert_eq!(l.flat_conformance_classes_v7.len(), 4);
    l = KernelLedgerV1::from_rows(&l, l.scalars(), &l.to_rows()).unwrap();
    l.begin_block(9).unwrap();
    let root = l.root();
    assert!(l.apply_object(&object(&p, 9), &AuthV1 { signer_bond: OWNER }).unwrap_err().why.contains("catalog"));
    assert_eq!(l.root(), root);
    let o = K::RegisterConformanceClass {
        descriptor: d.digest(),
        program_bytes: p.encode(),
        param_commitments: params(),
        plan: plan_for_tir_program_v1(&d, &p, program_root_v1(&p.encode()), 9).unwrap(),
    };
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
    let mut l = ledger();
    l.restore_budget(BlockBudgetV1 { adjudications: 0, court_work: l.policy.admission_work_limit_v1() });
    let before = l.budget_used();
    assert!(l.apply_object(&object(&p, 9), &AuthV1 { signer_bond: OWNER }).is_err());
    assert_eq!(l.budget_used(), before);
    assert!(l.flat_conformance_classes_v7.is_empty());
}
#[test]
fn flat_candidate7_declared_and_stored_scope_mutations_are_refused() {
    let p = program(65);
    for kind in 0..5 {
        let mut l = ledger();
        let mut o = object(&p, 9);
        let K::RegisterFlatModelCandidateV7 { scope_root, replay_root, max_positions, program_bytes, param_commitments, .. } = &mut o
        else {
            unreachable!()
        };
        match kind {
            0 => scope_root[0] ^= 1,
            1 => replay_root[0] ^= 1,
            2 => *max_positions = 10,
            3 => program_bytes.push(0),
            _ => param_commitments.by_instance.clear(),
        }
        let root = l.root();
        assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
        assert_eq!(l.root(), root);
        assert!(l.flat_conformance_classes_v7.is_empty());
        if kind < 4 {
            assert!(l.budget_used().court_work > 0, "failed canonical/scope derivation consumes its attempt");
        }
    }
    let mut l = ledger();
    let class = apply(&mut l, &object(&p, 9));
    let (_, record) = l.flat_conformance_classes_v7[&class].clone();
    for kind in 0..6 {
        let mut bad = record.clone();
        match kind {
            0 => bad.plan.version = 6,
            1 => bad.plan.court_set[0] ^= 1,
            2 => bad.plan.profile[0] ^= 1,
            3 => bad.plan.scope_root[0] ^= 1,
            4 => bad.plan.replay_root[0] ^= 1,
            _ => bad.plan.max_positions += 1,
        }
        let mut rows = l.to_rows();
        rows.remove(&(TABLE_FLAT_CONFORMANCE_CLASSES_V7, borsh::to_vec(&class).unwrap()));
        rows.insert((TABLE_FLAT_CONFORMANCE_CLASSES_V7, borsh::to_vec(&bad.id()).unwrap()), borsh::to_vec(&(OWNER, bad)).unwrap());
        assert!(KernelLedgerV1::from_rows(&l, l.scalars(), &rows).is_err(), "rekeying cannot authenticate forged stored metadata");
    }
    let mut narrowed = l.clone();
    narrowed.policy.prosecution.max_verifier_ram = 1;
    assert!(KernelLedgerV1::from_rows(&narrowed, narrowed.scalars(), &l.to_rows()).is_err());
}
#[test]
fn flat_candidate7_codec_counts_exact_ranges_and_complete_parameter_instances() {
    let p = program(65);
    let bytes = object(&p, 1).encode();
    for n in [0, 1, 129, bytes.len() - 1] {
        assert!(K::decode(&bytes[..n]).is_err());
    }
    let mut b = bytes.clone();
    b[130..134].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(K::decode(&b).is_err());
    let at = 134 + p.encode().len();
    let mut b = bytes.clone();
    b[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(K::decode(&b).is_err());
    let mut b = bytes;
    b.push(0);
    assert!(K::decode(&b).is_err());
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let a = b.param("scores.a", DType::I32, &[65], false);
    let c = b.param("scores.b", DType::I32, &[65], false);
    let mut pre = b.block("pre", vec![]);
    let a = pre.topk(a, 0, 2);
    let a = pre.cast(a, DType::I32);
    let c = pre.topk(c, 0, 2);
    let c = pre.cast(c, DType::I32);
    let x = pre.add(a, c, DType::I32);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let y = post.cast(misaka_palw_tir::Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    assert!(derive_flat_candidate_v7(&k2_tir_v4_descriptor().digest(), &p.encode(), &params(), 1).is_err());
    let mut pc = params();
    pc.by_instance.insert((1, None), [9; 64]);
    derive_flat_candidate_v7(&k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 1).unwrap();
    // Cast a full I32 parameter into I8 without a clamp: canonical typing can
    // be legal while exact arithmetic range admission must still reject it.
    let mut b = ProgramBuilder::new(256, HISTORY_BOUND_V1_HELD);
    let x = b.param("unsafe", DType::I32, &[2], false);
    let mut pre = b.block("pre", vec![]);
    let x = pre.cast(x, DType::I8);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I8, &[2])]);
    let y = post.cast(misaka_palw_tir::Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    assert!(derive_flat_candidate_v7(&k2_tir_v4_descriptor().digest(), &p.encode(), &params(), 1).err().unwrap().contains("range"));
}
