//! Synthetic metadata admission. Large dimensions do not execute a checkpoint
//! or allocate a max-length trace, and candidates carry no execution rights.
mod common;
use common::{ledger_world::policy, opv_world::opv_example};
use misaka_palw_kernel::{
    descriptor::{KernelScheduleV1, KernelStatusV1, k2_tir_v4_descriptor},
    hash::Digest,
    job::{DecodeRuleV1, KernelJobV1},
    ledger::{AuthV1, KernelLedgerV1, LedgerEventV1, flat_candidate9::*},
    route::KernelRouteObjectV1 as K,
    rows::{TABLE_FLAT_CONFORMANCE_CLASSES_V9, diff_rows},
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
    let v = derive_flat_candidate_v9(&k2_tir_v4_descriptor().digest(), &p.encode(), &params(), max).unwrap();
    K::RegisterFlatModelCandidateV9 {
        binding: [9; 64],
        descriptor: v.plan.descriptor,
        program_bytes: p.encode(),
        param_commitments: params(),
        max_positions: max,
        scope_root: v.plan.scope_root,
        replay_root: v.plan.replay_root,
        resource_root: v.plan.resource_root,
    }
}
fn apply(l: &mut KernelLedgerV1, o: &K) -> Digest {
    let e = l.apply_object(o, &AuthV1 { signer_bond: OWNER }).unwrap();
    let [LedgerEventV1::FlatConformanceClassRegisteredV9 { class }] = e.as_slice() else { panic!("{e:?}") };
    *class
}

#[test]
fn candidate9_complete_roots_rows_rollback_and_no_execution_eligibility() {
    let mut l = ledger();
    let o = object(&program(65), 32);
    let before = l.to_rows();
    let root = l.root();
    let class = apply(&mut l, &o);
    let rows = l.to_rows();
    assert_eq!(&o.encode()[..2], &[1, 53]); // codex 48, renumbered at rfc02/merge-final
    assert_eq!(K::decode(&o.encode()).unwrap(), o);
    assert_eq!(l.native_candidate_count_v9(), 1);
    let record = l.flat_conformance_classes_v9[&class].1.clone();
    record.checked().unwrap();
    record.checked_native().unwrap();
    let restored = KernelLedgerV1::from_rows(&l, l.scalars(), &rows).unwrap();
    assert_eq!(restored.root(), l.root());
    assert_eq!(restored.to_rows(), rows);
    assert!(l.classes.is_empty() && l.jobs.is_empty() && l.claims.is_empty() && l.opv.admitted.is_empty());
    let job =
        KernelJobV1 { class_binding_id: class, prompt: vec![0], max_new_tokens: 1, decode: DecodeRuleV1::Greedy, nonce: [1; 64] };
    assert!(l.apply_object(&K::PostJob { job }, &AuthV1 { signer_bond: OWNER }).is_err());
    let mut undone = rows;
    for (key, old, _) in diff_rows(&before, &undone.clone()) {
        match old {
            Some(v) => {
                undone.insert(key, v);
            }
            None => {
                undone.remove(&key);
            }
        }
    }
    assert_eq!(undone, before);
    assert_eq!(KernelLedgerV1::from_rows(&l, l.scalars(), &undone).unwrap().root(), root);
    let mut withdrawal = restored;
    withdrawal.apply_object(&K::RequestExit { bond: OWNER }, &AuthV1 { signer_bond: OWNER }).unwrap();
    withdrawal.begin_block(100).unwrap();
    withdrawal.apply_object(&K::Withdraw { bond: OWNER }, &AuthV1 { signer_bond: OWNER }).unwrap();
    assert_eq!(withdrawal.native_candidate_count_v9(), 0);
    assert!(withdrawal.flat_conformance_classes_v9.is_empty());
}
#[test]
fn candidate9_import_forged_scope_replay_resource_and_rooted_tariff_refuse() {
    let mut l = ledger();
    let class = apply(&mut l, &object(&program(65), 32));
    let record = l.flat_conformance_classes_v9[&class].1.clone();
    for case in 0..5 {
        let mut forged = record.clone();
        match case {
            0 => forged.plan.scope_root[0] ^= 1,
            1 => forged.plan.replay_root[0] ^= 1,
            2 => forged.plan.resource_root[0] ^= 1,
            3 => forged.plan.native_restore_work = 0,
            _ => forged.scope_bytes[0] ^= 1,
        }
        assert!(forged.checked().is_err());
        let mut rows = l.to_rows();
        rows.remove(&(TABLE_FLAT_CONFORMANCE_CLASSES_V9, borsh::to_vec(&class).unwrap()));
        rows.insert(
            (TABLE_FLAT_CONFORMANCE_CLASSES_V9, borsh::to_vec(&forged.id()).unwrap()),
            borsh::to_vec(&(OWNER, forged)).unwrap(),
        );
        assert!(KernelLedgerV1::from_rows(&l, l.scalars(), &rows).is_err());
    }
}
#[test]
fn candidate9_bounded_nested_codec_rejects_counts_before_allocation() {
    let mut l = ledger();
    let class = apply(&mut l, &object(&program(65), 32));
    let row = l.to_rows()[&(TABLE_FLAT_CONFORMANCE_CLASSES_V9, borsh::to_vec(&class).unwrap())].clone();
    let (_, _, _, at, n) = candidate_row_header_v9(&row).unwrap();
    let pc_at = at + n as usize;
    assert_eq!(decode_candidate_row_v9(&row).unwrap().1.id(), class);
    let mut wrong = row.clone();
    wrong[pc_at..pc_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_candidate_row_v9(&wrong).is_err());
    let scope_at = pc_at + 4 + 67;
    let mut wrong = row.clone();
    wrong[scope_at..scope_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(decode_candidate_row_v9(&wrong).is_err());
    let mut trailing = row.clone();
    trailing.push(0);
    assert!(decode_candidate_row_v9(&trailing).is_err());
    for cut in [0, 64, at, pc_at, scope_at, row.len() - 1] {
        assert!(decode_candidate_row_v9(&row[..cut]).is_err());
    }
}
#[test]
fn candidate9_global128_catalog_and_raw_import_caps_precede_untrusted_decode() {
    let mut l = ledger();
    let p = program(65);
    for index in 0..128 {
        let owner = [2 + (index / 8) as u8; 64];
        l.sync_bond(owner, 1_000_000);
        l.begin_block(2 + index as u64).unwrap();
        let o = object(&p, index as u32 + 1);
        l.apply_object(&o, &AuthV1 { signer_bond: owner }).unwrap();
    }
    assert_eq!(l.native_candidate_count_v9(), 128);
    l.begin_block(130).unwrap();
    l.sync_bond(OWNER, 1_000_000);
    let root = l.root();
    assert!(l.apply_object(&object(&p, 129), &AuthV1 { signer_bond: OWNER }).unwrap_err().why.contains("registry"));
    assert_eq!(l.root(), root);
    let rows = l.to_rows();
    check_candidate_registry_import_v9(&rows, &l.policy).unwrap();
    let rebuilt = KernelLedgerV1::from_rows(&l, l.scalars(), &rows).unwrap();
    assert_eq!(rebuilt.native_candidate_count_v9(), 128);
    let mut too_many = rows.clone();
    too_many.insert((TABLE_FLAT_CONFORMANCE_CLASSES_V9, vec![255; 64]), vec![]);
    let err = check_candidate_registry_import_v9(&too_many, &l.policy).unwrap_err();
    assert!(err.contains("count/raw"), "{err}");
    let mut oversize = misaka_palw_kernel::rows::LedgerRowsV1::new();
    oversize.insert((TABLE_FLAT_CONFORMANCE_CLASSES_V9, vec![0; 64]), vec![0; FLAT_CANDIDATE_RECORD_MAX_BYTES_V9 + 1]);
    assert!(check_candidate_registry_import_v9(&oversize, &l.policy).unwrap_err().contains("count/raw"));
}
#[test]
fn candidate9_duplicate_refusal_and_projected_shared_owner_catalog() {
    let mut l = ledger();
    let o = object(&program(65), 32);
    let class = apply(&mut l, &o);
    l.begin_block(2).unwrap();
    let root = l.root();
    assert!(l.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
    assert_eq!(l.root(), root);
    let mut projected = ledger();
    projected.note_native_candidate_projection_v9(OWNER, class).unwrap();
    assert!(projected.flat_conformance_classes_v9.is_empty());
    assert!(projected.native_candidate_owned_v9(&OWNER, &class));
    assert_eq!(projected.native_candidate_count_v9(), 1);
    assert!(projected.note_native_candidate_projection_v9(OWNER, class).is_err());
    assert!(!projected.native_candidate_owned_v9(&[2; 64], &class));
    projected.begin_block(3).unwrap();
    assert!(projected.apply_object(&o, &AuthV1 { signer_bond: OWNER }).is_err());
}
