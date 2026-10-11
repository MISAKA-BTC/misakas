use super::*;
use crate::flat_execution11::{self as ex, CheckedExecutionClassV11, ExecutionJobStateV11, NativeProtectedTraceV11};

fn fixture() -> (
    CheckedExecutionClassV11,
    CheckedJobQuotaPlanV11,
    QuotaPolicyV11,
    TrustedEpochV11,
    ExecutionJobStateV11,
    NativePaidSealAnchorV11,
    QuotaStateV11,
) {
    fixture_with_config(257, None, None)
}
fn fixture_with_config(
    max_actors: u32,
    cap: Option<QuotaQuantitiesV11>,
    under: Option<(u8, ResourceAxisV10, u128)>,
) -> (
    CheckedExecutionClassV11,
    CheckedJobQuotaPlanV11,
    QuotaPolicyV11,
    TrustedEpochV11,
    ExecutionJobStateV11,
    NativePaidSealAnchorV11,
    QuotaStateV11,
) {
    let (old_class, _, _, mut policy) = super::tests::checked_class_fixture();
    use misaka_palw_tir::{DType, builder::ProgramBuilder, program::Ref};
    let mut pb = ProgramBuilder::new(4, misaka_palw_tir::program::HISTORY_BOUND_V1_SMALL);
    let a = pb.param("protected.a", DType::I16, &[1, 128], false);
    let b = pb.param("protected.b", DType::I16, &[128, 129], false);
    let mut pre = pb.block("pre", vec![]);
    let a = pre.clamp(a, 0, 1, DType::I16);
    let b = pre.clamp(b, 0, 1, DType::I16);
    let target = pre.matmul(a, b, DType::I32);
    let ty = pre.ty(target);
    let pre = pre.finish(&[target]);
    let mut post = pb.block("post", vec![ty]);
    let out = post.reshape_fixed(Ref::CarryIn(0), &[129]);
    post.commit(out);
    let post = post.finish(&[]);
    let program = pb.finish(pre, vec![], post, 0);
    let mut params = misaka_palw_tir::MapParams::default();
    for (i, d) in program.params.iter().enumerate() {
        let shape: Vec<_> = d.shape.iter().map(|n| *n as usize).collect();
        let len = shape.iter().product();
        params.tensors.insert((i as u16, None), misaka_palw_tir::Tensor::new(d.dtype, shape, vec![1; len]).unwrap());
    }
    let pc = ParamCommitmentsV1::of_v3(&params);
    // An explicit isolated network policy fixture, not a native scalar-budget
    // override. Every enforcement below still uses the actual fixed quantities.
    policy.plan = QuotaQuantitiesV11([u64::MAX as u128; QUOTA_AXES_V11]);
    policy.block = policy.plan.clone();
    policy.actor = policy.plan.clone();
    policy.network = policy.plan.clone();
    policy.max_actors = max_actors;
    policy.max_subjects = 257;
    if let Some(cap) = cap {
        policy.block = cap.clone();
        policy.actor = cap.clone();
        policy.network = cap;
    }
    if let Some((scope, axis, cap)) = under {
        match scope {
            0 => policy.plan.0[axis as usize] = cap,
            1 => policy.block.0[axis as usize] = cap,
            2 => policy.actor.0[axis as usize] = cap,
            _ => policy.network.0[axis as usize] = cap,
        }
    }
    let c = old_class.record();
    let view = crate::ledger::flat_candidate9::derive_flat_candidate_v9(&c.descriptor, &program.encode(), &pc, 3).unwrap();
    let source = ex::NativeSourceBindingV11 {
        descriptor: c.descriptor,
        program_root: view.plan.program_root,
        parameter_root: pc.root(),
        inventory_root: c.inventory_root,
        bound_daa: 1,
        matures_daa: 2,
        refuted: false,
    };
    let candidate = crate::ledger::flat_candidate9::FlatCandidateRecordV9 {
        binding: source.id(),
        plan: view.plan,
        program_bytes: program.encode(),
        param_commitments: pc,
        scope_bytes: borsh::to_vec(&view.scope).unwrap(),
        replay_bytes: borsh::to_vec(&view.replay).unwrap(),
    };
    let execution = ex::ExecutionPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: policy.network_domain,
        quota_policy_root: policy.root(),
        horizons: c.horizons.clone(),
        reward_cap: c.reward_cap,
        external_gain_cap: 0,
        default_obligation: 0,
        required_producer_reserve: c.required_producer_reserve,
        finder_permille: c.finder_permille,
    };
    let class = ex::derive_execution_class_v11(&candidate, &source, &execution, &policy, 10).unwrap();
    let epoch = TrustedEpochV11::from_chain(&policy, [201; 64], 10).unwrap();
    let prepared = super::tests::prepared_native_job(&class, &policy, &epoch, 0);
    let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
    let admission = check_native_quota_admission_v11(&class, &book, &super::tests::native_admission(&book)).unwrap();
    let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
    state.admit_baseline(&policy, &epoch, prepared.job.producer, &admission, &prepared.quota_plan).unwrap();
    let r = class.record();
    let trace = NativeProtectedTraceV11 {
        vector: [202; 64],
        candidate9_id: r.candidate9_id,
        binding_id: r.binding_id,
        class_scope9_root: r.class_scope9_root,
        replay9_root: r.replay9_root,
        class_resource10_root: r.resource10_root,
        prompt_root: prepared.job.input.prompt_root,
        prompt_len: 1,
        positions: 1,
        generated_count: 1,
        trace_root: [203; 64],
        output_root: [204; 64],
        final_state_root: [205; 64],
    };
    book.commit(r, prepared.job.producer, &trace, 10, r.required_producer_reserve, [206; 64]).unwrap();
    let mut native = NativePaidSealAnchorV11 {
        job_id: book.job().id(),
        funded_job_root: book.job().root(),
        book_root: book.root(),
        claim_id: book.claim().unwrap().id(),
        vector_id: trace.vector,
        seal_actor: [207; 64],
        seal_operator_id: [208; 64],
        producer_operator_id: [209; 64],
        proof_root: [210; 64],
        posted_daa: 10,
        matures_daa: 13,
        horizon_daa: 113,
        original_paid_seal_root: [211; 64],
        original_debit: 10,
        original_debit_receipt: [212; 64],
        shared_live_seals: 1,
        shared_actor_live_seals: 1,
    };
    native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
    (class, prepared.quota_plan, policy, epoch, book, native, state)
}
fn admit(
    f: &mut (
        CheckedExecutionClassV11,
        CheckedJobQuotaPlanV11,
        QuotaPolicyV11,
        TrustedEpochV11,
        ExecutionJobStateV11,
        NativePaidSealAnchorV11,
        QuotaStateV11,
    ),
) -> Digest {
    let checked = check_native_paid_seal_v11(&f.0, &f.4, &f.3, &f.5).unwrap();
    let key = checked.key().root();
    f.6.admit_protected_seal(&f.2, &f.3, f.5.seal_actor, &checked, &f.1).unwrap();
    key
}
fn mature(policy: &QuotaPolicyV11) -> TrustedEpochV11 {
    TrustedEpochV11::from_chain(policy, [213; 64], 13).unwrap()
}

#[test]
fn original_paid_seal_checks_exact_subject_funding_operator_clock_and_shared_slots() {
    let (class, plan, policy, epoch, book, native, mut state) = fixture();
    for field in 0..20 {
        let mut bad = native.clone();
        match field {
            0 => bad.job_id[0] ^= 1,
            1 => bad.funded_job_root[0] ^= 1,
            2 => bad.book_root[0] ^= 1,
            3 => bad.claim_id[0] ^= 1,
            4 => bad.vector_id[0] ^= 1,
            5 => bad.seal_actor = book.job().producer,
            6 => bad.seal_operator_id = bad.producer_operator_id,
            7 => bad.posted_daa += 1,
            8 => bad.matures_daa += 1,
            9 => bad.horizon_daa += 1,
            10 => bad.original_paid_seal_root = [0; 64],
            11 => bad.original_debit = 0,
            12 => bad.original_debit_receipt = [0; 64],
            13 => bad.shared_live_seals = 0,
            14 => bad.shared_live_seals = 1025,
            15 => bad.shared_actor_live_seals = 0,
            16 => bad.shared_actor_live_seals = 5,
            17 => bad.horizon_daa = u64::MAX,
            18 => bad.posted_daa = u64::MAX,
            _ => bad.matures_daa = 0,
        }
        assert!(check_native_paid_seal_v11(&class, &book, &epoch, &bad).is_err(), "field {field}");
    }
    let checked = check_native_paid_seal_v11(&class, &book, &epoch, &native).unwrap();
    let root = state.root();
    assert!(state.admit_protected_seal(&policy, &epoch, book.job().producer, &checked, &plan).is_err());
    assert_eq!(state.root(), root);
    state.admit_protected_seal(&policy, &epoch, native.seal_actor, &checked, &plan).unwrap();
    let root = state.root();
    assert!(state.admit_protected_seal(&policy, &epoch, native.seal_actor, &checked, &plan).is_err());
    assert_eq!(state.root(), root);
    state.check_restored(&policy).unwrap();
}
#[test]
fn exact_reserved_vector_includes_two_courts_one_finish_two_da_and_two_attempts_per_part() {
    let mut f = fixture();
    let key = admit(&mut f);
    let p = derive_protected_seal_plan_v11(&f.1).unwrap();
    let expected = p
        .court
        .checked_mul(2)
        .unwrap()
        .checked_add(&p.whole_chunk)
        .unwrap()
        .checked_add(&p.da_request.checked_mul(2).unwrap())
        .unwrap()
        .checked_add(&p.fragment.checked_mul(2 * p.fragment_count_cap).unwrap())
        .unwrap();
    assert_eq!(p.guarantee, expected);
    assert_eq!(p.guarantee.get(ResourceAxisV10::Macs), 0);
    assert_eq!(p.guarantee.get(ResourceAxisV10::ParameterCalls), 0);
    assert_eq!(f.6.protected_seal(&key).unwrap().committed, p.admission.checked_add(&expected).unwrap());
    let producer = f.6.subject(&f.5.job_id).unwrap();
    assert_eq!(f.6.network.reserved, producer.guarantee_remaining.checked_add(&expected).unwrap());
    assert_eq!(borsh::to_vec(f.6.protected_seal(&key).unwrap()).unwrap().len() as u64 + 64, PROTECTED_SEAL_ROW_BYTES_V11);
    assert_eq!(QuotaStateV11::from_rooted_bytes(&borsh::to_vec(&f.6).unwrap(), &f.2, &f.6.root()).unwrap(), f.6);
}
#[test]
fn immature_wrong_owner_wrong_plan_and_failed_court_attempts_never_borrow_a_guarantee() {
    let mut f = fixture();
    let key = admit(&mut f);
    let before = f.6.root();
    assert!(f.6.debit_protected(&f.2, &f.3, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).is_err());
    let epoch = mature(&f.2);
    assert!(f.6.debit_protected(&f.2, &epoch, f.4.job().producer, &key, &f.1, ProtectedActionKindV11::Court, &[]).is_err());
    let mut wrong = f.1.record();
    wrong.native_restore_work -= 1;
    let other = CheckedJobQuotaPlanV11::restore(&wrong, &wrong.root()).unwrap();
    assert!(f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &other, ProtectedActionKindV11::Court, &[]).is_err());
    assert_eq!(f.6.root(), before);
    let producer = f.6.subject(&f.5.job_id).unwrap().clone();
    let total = f.6.network.total().unwrap();
    for _ in 0..2 {
        f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap();
    }
    let after = f.6.root();
    assert!(f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).is_err());
    assert_eq!(f.6.root(), after);
    assert_eq!(f.6.network.total().unwrap(), total);
    assert_eq!(f.6.subject(&f.5.job_id).unwrap(), &producer);
    f.6.check_restored(&f.2).unwrap();
}
#[test]
fn all_raw_fragment_boundaries_refuse_before_debit_and_successful_duplicates_use_no_retry() {
    let mut f = fixture();
    let key = admit(&mut f);
    let epoch = mature(&f.2);
    let p = derive_protected_seal_plan_v11(&f.1).unwrap();
    for (index, count, bytes, outer) in [
        (0, 0, 1, 1),
        (0, p.fragment_count_cap + 1, 1, 1),
        (0, 65, 1, 1),
        (1, 1, 1, 1),
        (u32::MAX, 1, 1, 1),
        (0, 1, 0, 1),
        (0, 1, 32769, 1),
        (0, 1, 1, 1025),
    ] {
        let root = f.6.root();
        assert!(f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, index, count, bytes, outer).is_err());
        assert_eq!(f.6.root(), root);
    }
    let bytes = p.assembly_cap.min(32768);
    let paid = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, bytes, 1024).unwrap();
    f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &paid).unwrap();
    let root = f.6.root();
    assert!(f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &paid).is_err());
    assert!(f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, bytes, 1024).is_err());
    assert_eq!(f.6.root(), root);
    assert_eq!(f.6.protected_seal(&key).unwrap().fragments.attempts_by_index[0], 1);
    assert!(f.6.protected_seal(&key).unwrap().fragments.complete());
    f.6.check_restored(&f.2).unwrap();
}
#[test]
fn failed_fragments_retain_two_prepaid_attempts_and_cannot_reuse_the_first_private_token() {
    let mut f = fixture();
    let key = admit(&mut f);
    let epoch = mature(&f.2);
    let first = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, 1, 1).unwrap();
    let spent = f.6.network.consumed.clone();
    let second = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, 1, 1).unwrap();
    assert_ne!(f.6.network.consumed, spent);
    let root = f.6.root();
    assert!(f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &first).is_err());
    assert!(f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, 1, 1).is_err());
    assert_eq!(f.6.root(), root);
    f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &second).unwrap();
    assert_eq!(f.6.protected_seal(&key).unwrap().fragments.attempts_by_index[0], 2);
    f.6.check_restored(&f.2).unwrap();
}
#[test]
fn out_of_order_fragments_bind_count_cap_sum_and_pay_whole_court_finish_once() {
    let mut f = fixture();
    let key = admit(&mut f);
    let epoch = mature(&f.2);
    let p = derive_protected_seal_plan_v11(&f.1).unwrap();
    let count = p.fragment_count_cap;
    assert!(count >= 2, "ordinary court fixture should require multiple transport parts");
    let before = f.6.root();
    assert!(f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::WholeChunk, &[]).is_err());
    assert_eq!(f.6.root(), before);
    let last = p.assembly_cap - (count as u64 - 1) * 32768;
    let paid = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, count - 1, count, last, 1).unwrap();
    f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &paid).unwrap();
    let root = f.6.root();
    assert!(f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, count - 1, 1, 1).is_err());
    assert_eq!(f.6.root(), root);
    for index in 0..count - 1 {
        let paid = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, index, count, 32768, 1).unwrap();
        f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &paid).unwrap();
    }
    assert_eq!(f.6.protected_seal(&key).unwrap().fragments.accepted_bytes, p.assembly_cap);
    f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::WholeChunk, &[]).unwrap();
    let root = f.6.root();
    assert!(f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::WholeChunk, &[]).is_err());
    assert_eq!(f.6.root(), root);
    f.6.check_restored(&f.2).unwrap();
}

#[test]
fn ordinary_floods_leave_both_producer_and_paid_seal_da_court_guarantees_available() {
    let mut trial = fixture();
    let _ = admit(&mut trial);
    let exact = trial.6.network.total().unwrap();
    let mut f = fixture_with_config(257, Some(exact), None);
    let key = admit(&mut f);
    let total = f.6.network.total().unwrap();
    let epoch = mature(&f.2);
    let ordinary = DerivedOrdinaryMetadataV11 { quantities: action_envelope(1, 1, None).unwrap() };
    for _ in 0..32 {
        let root = f.6.root();
        assert!(f.6.debit_ordinary(&f.2, &epoch, [231; 64], &ordinary).is_err());
        assert_eq!(f.6.root(), root);
    }
    let producer = f.6.subject(&f.5.job_id).unwrap().clone();
    for _ in 0..2 {
        f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::DARequest, &[]).unwrap();
    }
    let root = f.6.root();
    assert!(f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::DARequest, &[]).is_err());
    assert_eq!(f.6.root(), root);
    assert_eq!(f.6.subject(&f.5.job_id).unwrap(), &producer);
    let binding = producer.binding;
    let answer = f.1.action(QuotaActionKindV11::DAAnswer, &[]).unwrap();
    f.6.debit(&f.2, &epoch, binding.producer, &binding, &f.1, &answer).unwrap();
    f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap();
    assert_eq!(f.6.network.total().unwrap(), total);
    f.6.check_restored(&f.2).unwrap();
    // This is resource-counter saturation, not a claim that the native32-slot
    // demand catalog has already consumed this pure API; native tests own that.
}
#[test]
fn every_positive_protected_axis_enforces_plan_block_actor_network_atomically() {
    let f = fixture();
    let p = derive_protected_seal_plan_v11(&f.1).unwrap();
    let need = p.admission.checked_add(&p.guarantee).unwrap();
    let baseline = f.6.network.total().unwrap();
    let combined = baseline.checked_add(&need).unwrap();
    let mut exercised = 0;
    for axis in ResourceAxisV10::ALL {
        if need.get(axis) == 0 {
            continue;
        }
        for scope in 0..4 {
            let cap = if scope == 1 || scope == 3 { combined.get(axis) - 1 } else { need.get(axis) - 1 };
            if (scope == 0 || scope == 2) && baseline.get(axis) > cap {
                continue;
            }
            let (class, plan, policy, epoch, book, native, mut state) = fixture_with_config(257, None, Some((scope, axis, cap)));
            let checked = check_native_paid_seal_v11(&class, &book, &epoch, &native).unwrap();
            let root = state.root();
            assert!(state.admit_protected_seal(&policy, &epoch, native.seal_actor, &checked, &plan).is_err(), "{axis:?} scope{scope}");
            assert_eq!(state.root(), root);
            state.check_restored(&policy).unwrap();
            exercised += 1;
        }
    }
    assert!(exercised >= 40, "must test independently metered positive axes across all cap scopes");
}
#[test]
fn closed_job_cannot_gc_live_seal_and_expired_one_row_gc_never_refunds_or_recredits() {
    let mut f = fixture();
    let key = admit(&mut f);
    let summary = f.6.protected_seal(&key).unwrap().clone();
    let binding = f.4.job().quota_subject(f.0.record()).unwrap();
    let late = TrustedEpochV11::from_chain(&f.2, [232; 64], f.4.job().deadline_daa + 1).unwrap();
    let paid = f.6.debit_expired_terminal(&f.2, &late, [233; 64], &binding, &f.1).unwrap();
    let retention = f.4.claim().unwrap().retention_until;
    let close = ex::NativeCloseSnapshotV11 {
        current_daa: late.block_daa,
        pending_own_proofs: 0,
        open_da: 0,
        latest_answer_proof_until: retention,
        required_retained_until: retention,
    };
    f.4.finalize(f.0.record(), &close).unwrap();
    f.6.mark_expired_terminal_closed(&f.2, &late, &binding, &f.0, &f.4, &paid).unwrap();
    let terminal = check_native_quota_terminal_v11(&f.0, &f.4, &super::tests::native_terminal(&f.4)).unwrap();
    let root = f.6.root();
    assert!(f.6.collect_expired_protected_seal(&f.2, &late, [233; 64], &key, &terminal).is_err());
    assert_eq!(f.6.root(), root);
    assert_eq!(f.6.protected_seal(&key).unwrap(), &summary);
    let next = TrustedEpochV11::from_chain(&f.2, [234; 64], 1100).unwrap();
    assert!(f.6.collect_closed_expired(&f.2, &next, [233; 64], &terminal).is_err());
    assert_eq!(f.6.root(), root);
    let receipt = f.6.collect_expired_protected_seal(&f.2, &next, [233; 64], &key, &terminal).unwrap();
    assert_eq!(f.6.network.consumed, receipt.consumed);
    assert_eq!(f.6.network.reserved, QuotaQuantitiesV11::default());
    assert!(f.6.protected_seal(&key).is_none());
    let spent = f.6.network.consumed.clone();
    let root = f.6.root();
    assert!(f.6.collect_expired_protected_seal(&f.2, &next, [233; 64], &key, &terminal).is_err());
    assert_eq!(f.6.root(), root);
    f.6.collect_closed_expired(&f.2, &next, [233; 64], &terminal).unwrap();
    assert!(f.6.network.consumed.get(ResourceAxisV10::MetadataWork) > spent.get(ResourceAxisV10::MetadataWork));
    assert!(f.6.subject(&binding.job_id).is_none());
    assert!(check_native_paid_seal_v11(&f.0, &f.4, &next, &f.5).is_err());
    f.6.check_restored(&f.2).unwrap();
}
#[test]
fn producer_and_seal_guarantees_survive_equal_daa_blocks_and_ancestor_replay_together() {
    let mut f = fixture();
    let key = admit(&mut f);
    let epoch = mature(&f.2);
    f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::DARequest, &[]).unwrap();
    let ancestor = f.6.clone();
    let equal = TrustedEpochV11::from_chain(&f.2, [235; 64], 13).unwrap();
    let receipt = f.6.debit_protected(&f.2, &equal, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap();
    let after = f.6.clone();
    assert_eq!(f.6.network.total().unwrap(), ancestor.network.total().unwrap());
    assert_eq!(f.6.block.reserved, f.6.network.reserved);
    f.6 = ancestor;
    assert_eq!(f.6.debit_protected(&f.2, &equal, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap(), receipt);
    assert_eq!(f.6, after);
    f.6.check_restored(&f.2).unwrap();
    for invalid in [
        TrustedEpochV11::from_chain(&f.2, equal.block_hash, 14).unwrap(),
        TrustedEpochV11::from_chain(&f.2, [236; 64], 12).unwrap(),
        TrustedEpochV11::from_chain(&f.2, [237; 64], 1100).unwrap(),
    ] {
        let root = f.6.root();
        assert!(f.6.debit_protected(&f.2, &invalid, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).is_err());
        assert_eq!(f.6.root(), root);
    }
}
#[test]
fn protected_rooted_restore_rejects_conserved_rewind_counts_truncation_duplicate_and_trailing_bytes() {
    let mut f = fixture();
    let key = admit(&mut f);
    let epoch = mature(&f.2);
    f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap();
    let expected = f.6.root();
    let bytes = borsh::to_vec(&f.6).unwrap();
    let mut rewound = f.6.clone();
    let axis = ResourceAxisV10::MetadataWork as usize;
    let seal = rewound.protected_seals.get_mut(&key).unwrap();
    seal.consumed.0[axis] -= 1;
    seal.guarantee_remaining.0[axis] += 1;
    for s in [&mut rewound.block, &mut rewound.network, rewound.actors.get_mut(&f.5.seal_actor).unwrap()] {
        s.consumed.0[axis] -= 1;
        s.reserved.0[axis] += 1;
    }
    let forged = borsh::to_vec(&rewound).unwrap();
    assert!(QuotaStateV11::from_bytes(&f.2, &forged).is_ok());
    assert!(QuotaStateV11::from_rooted_bytes(&forged, &f.2, &expected).is_err());
    let mut none = f.6.clone();
    none.protected_seals.clear();
    let count_offset = borsh::to_vec(&none).unwrap().len() - 4;
    let mut flooded = bytes.clone();
    flooded[count_offset..count_offset + 4].copy_from_slice(&1025u32.to_le_bytes());
    assert!(QuotaStateV11::from_bytes(&f.2, &flooded).is_err());
    for size in [0, count_offset, count_offset + 4, bytes.len() - 1] {
        assert!(QuotaStateV11::from_bytes(&f.2, &bytes[..size]).is_err());
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(QuotaStateV11::from_bytes(&f.2, &trailing).is_err());
    let mut missing = f.6.clone();
    missing.protected_seals.clear();
    assert!(missing.check_restored(&f.2).is_err());
    let mut wrong_owner = f.6.clone();
    wrong_owner.protected_seals.get_mut(&key).unwrap().seal_operator_id = f.5.producer_operator_id;
    assert!(wrong_owner.check_restored(&f.2).is_err());
    // Two valid keys allow an explicit duplicate-row codec test.
    let posted = TrustedEpochV11::from_chain(&f.2, [238; 64], 13).unwrap();
    let mut native = f.5.clone();
    native.proof_root[0] ^= 1;
    native.posted_daa = 13;
    native.matures_daa = 16;
    native.horizon_daa = 116;
    native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
    let checked = check_native_paid_seal_v11(&f.0, &f.4, &posted, &native).unwrap();
    f.6.admit_protected_seal(&f.2, &posted, native.seal_actor, &checked, &f.1).unwrap();
    let mut duplicate = borsh::to_vec(&f.6).unwrap();
    let row = count_offset + 4;
    let width = PROTECTED_SEAL_ROW_BYTES_V11 as usize;
    let first = duplicate[row..row + 64].to_vec();
    duplicate[row + width..row + width + 64].copy_from_slice(&first);
    assert!(QuotaStateV11::from_bytes(&f.2, &duplicate).is_err());
}
#[test]
fn fixed_retained_actor_four_and_global_1024_limits_are_enforced_before_allocation() {
    let mut f = fixture();
    for group in 0..256u32 {
        if group != 0 {
            let prepared = super::tests::prepared_native_job(&f.0, &f.2, &f.3, group as u64);
            let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
            let admission = check_native_quota_admission_v11(&f.0, &book, &super::tests::native_admission(&book)).unwrap();
            f.6.admit_baseline(&f.2, &f.3, prepared.job.producer, &admission, &prepared.quota_plan).unwrap();
            let r = f.0.record();
            let mut vector = [202; 64];
            vector[..4].copy_from_slice(&group.to_le_bytes());
            let trace = NativeProtectedTraceV11 {
                vector,
                candidate9_id: r.candidate9_id,
                binding_id: r.binding_id,
                class_scope9_root: r.class_scope9_root,
                replay9_root: r.replay9_root,
                class_resource10_root: r.resource10_root,
                prompt_root: prepared.job.input.prompt_root,
                prompt_len: 1,
                positions: 1,
                generated_count: 1,
                trace_root: [203; 64],
                output_root: [204; 64],
                final_state_root: [205; 64],
            };
            book.commit(r, prepared.job.producer, &trace, 10, r.required_producer_reserve, [206; 64]).unwrap();
            f.1 = prepared.quota_plan;
            f.4 = book;
            f.5.job_id = f.4.job().id();
            f.5.funded_job_root = f.4.job().root();
            f.5.book_root = f.4.root();
            f.5.claim_id = f.4.claim().unwrap().id();
            f.5.vector_id = vector;
        }
        for part in 0..4u32 {
            let i = group * 4 + part;
            let mut native = f.5.clone();
            native.seal_actor = [240; 64];
            native.seal_actor[..4].copy_from_slice(&group.to_le_bytes());
            native.seal_operator_id = native.seal_actor;
            native.proof_root[..4].copy_from_slice(&i.to_le_bytes());
            native.shared_live_seals = i + 1;
            native.shared_actor_live_seals = part + 1;
            native.original_paid_seal_root = native_paid_seal_record_root_v11(&native);
            let checked = check_native_paid_seal_v11(&f.0, &f.4, &f.3, &native).unwrap();
            f.6.admit_protected_seal(&f.2, &f.3, native.seal_actor, &checked, &f.1).unwrap();
        }
        assert_eq!(f.6.subject(&f.4.job().id()).unwrap().retained_protected_seals, 4);
        let mut fifth = f.5.clone();
        // A new actor still cannot bypass the rooted PER JOB four limit.
        fifth.seal_actor = [241; 64];
        fifth.proof_root = [242; 64];
        fifth.shared_live_seals = (group * 4 + 4).min(1024);
        fifth.shared_actor_live_seals = 1;
        fifth.original_paid_seal_root = native_paid_seal_record_root_v11(&fifth);
        let checked = check_native_paid_seal_v11(&f.0, &f.4, &f.3, &fifth).unwrap();
        let root = f.6.root();
        assert!(f.6.admit_protected_seal(&f.2, &f.3, fifth.seal_actor, &checked, &f.1).is_err());
        assert_eq!(f.6.root(), root);
    }
    assert_eq!(f.6.protected_seal_count(), 1024);
    f.6.check_restored(&f.2).unwrap();
    let bytes = borsh::to_vec(&f.6).unwrap();
    assert_eq!(
        bytes.len() as u64,
        QUOTA_STATE_FIXED_BYTES_V11
            + 257 * QUOTA_ACTOR_ROW_BYTES_V11
            + 256 * QUOTA_SUBJECT_ROW_BYTES_V11
            + 1024 * PROTECTED_SEAL_ROW_BYTES_V11
    );
    assert_eq!(QuotaStateV11::from_rooted_bytes(&bytes, &f.2, &f.6.root()).unwrap(), f.6);
    // An independently funded 257th job with an empty per-job seal catalog
    // still cannot pass the shared global 1024 retained bound.
    let prepared = super::tests::prepared_native_job(&f.0, &f.2, &f.3, 256);
    let mut book = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
    let admission = check_native_quota_admission_v11(&f.0, &book, &super::tests::native_admission(&book)).unwrap();
    f.6.admit_baseline(&f.2, &f.3, prepared.job.producer, &admission, &prepared.quota_plan).unwrap();
    let r = f.0.record();
    let trace = NativeProtectedTraceV11 {
        vector: [243; 64],
        candidate9_id: r.candidate9_id,
        binding_id: r.binding_id,
        class_scope9_root: r.class_scope9_root,
        replay9_root: r.replay9_root,
        class_resource10_root: r.resource10_root,
        prompt_root: prepared.job.input.prompt_root,
        prompt_len: 1,
        positions: 1,
        generated_count: 1,
        trace_root: [203; 64],
        output_root: [204; 64],
        final_state_root: [205; 64],
    };
    book.commit(r, prepared.job.producer, &trace, 10, r.required_producer_reserve, [206; 64]).unwrap();
    let mut extra = f.5.clone();
    extra.job_id = book.job().id();
    extra.funded_job_root = book.job().root();
    extra.book_root = book.root();
    extra.claim_id = book.claim().unwrap().id();
    extra.vector_id = trace.vector;
    extra.seal_actor = [244; 64];
    extra.proof_root = [245; 64];
    extra.shared_live_seals = 1024;
    extra.shared_actor_live_seals = 1;
    extra.original_paid_seal_root = native_paid_seal_record_root_v11(&extra);
    let checked = check_native_paid_seal_v11(&f.0, &book, &f.3, &extra).unwrap();
    let root = f.6.root();
    assert!(f.6.admit_protected_seal(&f.2, &f.3, extra.seal_actor, &checked, &prepared.quota_plan).is_err());
    assert_eq!(f.6.root(), root);
}
#[test]
fn canonical_original_operator_highwater_witness_refuses_nonce_rewind_rotation_and_max_nonce() {
    let (class, _, policy, epoch, book, _, _) = fixture();
    // Original unclaimed funded book, not the already committed fixture book.
    let original = ExecutionJobStateV11::from_native_funded_job(book.job().clone()).unwrap();
    let native = super::tests::native_admission(&original);
    check_native_quota_admission_v11(&class, &original, &native).unwrap();
    for field in 0..4 {
        let mut wrong = super::tests::native_admission(&original);
        match field {
            0 => wrong.nonce_witness.next_nonce = 0,
            1 => wrong.nonce_witness.requester = [243; 64],
            2 => wrong.nonce_witness.requester_operator_id = [244; 64],
            _ => wrong.nonce_witness.counter_root[0] ^= 1,
        }
        assert!(check_native_quota_admission_v11(&class, &original, &wrong).is_err());
    }
    let prepared = super::tests::prepared_native_job(&class, &policy, &epoch, 1);
    let mut invalid = prepared.job.clone();
    invalid.nonce[63] = 1;
    let invalid_book = ExecutionJobStateV11::from_native_funded_job(invalid).unwrap();
    let mut wrong = super::tests::native_admission(&original);
    wrong.job_id = invalid_book.job().id();
    wrong.funded_job_root = invalid_book.job().root();
    wrong.book_root = invalid_book.root();
    wrong.request_key = invalid_book.job().request_key();
    assert!(check_native_quota_admission_v11(&class, &invalid_book, &wrong).is_err());
    let mut maximum = prepared.job;
    maximum.nonce = ex::nonce_bytes_v11(u64::MAX);
    let maximum_book = ExecutionJobStateV11::from_native_funded_job(maximum).unwrap();
    let mut max_anchor = super::tests::native_admission(&original);
    max_anchor.job_id = maximum_book.job().id();
    max_anchor.funded_job_root = maximum_book.job().root();
    max_anchor.book_root = maximum_book.root();
    max_anchor.request_key = maximum_book.job().request_key();
    max_anchor.nonce_witness.next_nonce = u64::MAX;
    max_anchor.nonce_witness.counter_root =
        ex::native_requester_nonce_counter_root_v11(&max_anchor.nonce_witness.requester_operator_id, u64::MAX);
    assert!(check_native_quota_admission_v11(&class, &maximum_book, &max_anchor).is_err());
}
#[test]
fn fixed_selected_native_rows_match_full_book_reservation_court_and_fragment_algebra() {
    fn load(state: &QuotaStateV11, policy: &QuotaPolicyV11, actor: Digest, job: Digest, seal: Option<Digest>) -> CheckedQuotaShardV11 {
        let global = state.global_row();
        let ar = state.actor_row(&actor);
        let subject = state.subject(&job);
        let selected_seal = seal;
        let seal = seal.and_then(|key| state.protected_seal(&key));
        let ar_root = ar.as_ref().map(|r| r.root());
        let subject_root = subject.map(|r| r.row_root());
        let seal_root = seal.map(|r| r.row_root());
        CheckedQuotaShardV11::from_native_rows(
            policy,
            &global,
            &global.root(),
            actor,
            job,
            selected_seal,
            ar.as_ref().zip(ar_root.as_ref()),
            subject.zip(subject_root.as_ref()),
            seal.zip(seal_root.as_ref()),
        )
        .unwrap()
    }
    let mut f = fixture();
    let checked = check_native_paid_seal_v11(&f.0, &f.4, &f.3, &f.5).unwrap();
    let key = checked.key().root();
    let mut shard = load(&f.6, &f.2, f.5.seal_actor, f.5.job_id, Some(key));
    let receipt = shard.admit_protected_seal(&f.2, &f.3, &checked, &f.1).unwrap();
    assert_eq!(f.6.admit_protected_seal(&f.2, &f.3, f.5.seal_actor, &checked, &f.1).unwrap(), receipt);
    let epoch = mature(&f.2);
    assert_eq!(
        shard.debit_protected(&f.2, &epoch, &f.1, ProtectedActionKindV11::Court, &[]).unwrap(),
        f.6.debit_protected(&f.2, &epoch, f.5.seal_actor, &key, &f.1, ProtectedActionKindV11::Court, &[]).unwrap()
    );
    let paid = shard.debit_protected_fragment(&f.2, &epoch, &f.1, 0, 1, 1, 1).unwrap();
    let full_paid = f.6.debit_protected_fragment(&f.2, &epoch, f.5.seal_actor, &key, &f.1, 0, 1, 1, 1).unwrap();
    assert_eq!(paid.receipt(), full_paid.receipt());
    shard.accept_protected_fragment(&f.2, &epoch, &f.1, &paid).unwrap();
    f.6.accept_protected_fragment(&f.2, &epoch, f.5.seal_actor, &f.1, &full_paid).unwrap();
    assert_eq!(shard.global_row(), f.6.global_row());
    assert_eq!(shard.actor_row(), f.6.actor_row(&f.5.seal_actor).unwrap());
    assert_eq!(shard.subject_row(), f.6.subject(&f.5.job_id));
    assert_eq!(shard.seal_row(), f.6.protected_seal(&key));
    f.6.check_restored(&f.2).unwrap();
    // Reconstruct from the actual reference book's authenticated selected rows.
    let restored = load(&f.6, &f.2, f.5.seal_actor, f.5.job_id, Some(key));
    assert_eq!(restored.global_row(), shard.global_row());
}
#[test]
fn selected_rooted_codecs_bound_exact_widths_and_authenticate_all_four_native_records() {
    let mut f = fixture();
    let key = admit(&mut f);
    let global = f.6.global_row();
    let actor = f.6.actor_row(&f.5.seal_actor).unwrap();
    let subject = f.6.subject(&f.5.job_id).unwrap();
    let seal = f.6.protected_seal(&key).unwrap();
    let gb = borsh::to_vec(&global).unwrap();
    let ab = borsh::to_vec(&actor).unwrap();
    let sb = borsh::to_vec(subject).unwrap();
    let pb = borsh::to_vec(seal).unwrap();
    assert_eq!(gb.len() as u64, QUOTA_GLOBAL_ROW_BYTES_V11);
    assert_eq!(ab.len() as u64, QUOTA_SELECTED_ACTOR_ROW_BYTES_V11);
    assert_eq!(sb.len() as u64, QUOTA_SELECTED_SUBJECT_ROW_BYTES_V11);
    assert_eq!(pb.len() as u64, QUOTA_SELECTED_SEAL_ROW_BYTES_V11);
    assert_eq!(QuotaGlobalRowV11::from_rooted_bytes(&gb, &global.root()).unwrap(), global);
    assert_eq!(QuotaActorRowV11::from_rooted_bytes(&ab, &actor.root()).unwrap(), actor);
    assert_eq!(QuotaSubjectSummaryV11::from_rooted_row_bytes(&sb, &subject.row_root()).unwrap(), *subject);
    assert_eq!(ProtectedSealSummaryV11::from_rooted_row_bytes(&pb, &seal.row_root()).unwrap(), *seal);
    for body in [&gb, &ab, &sb, &pb] {
        assert!(body.len() < 2048);
    }
    for truncate in [true, false] {
        let change = |bytes: &Vec<u8>| {
            let mut b = bytes.clone();
            if truncate {
                b.pop();
            } else {
                b.push(0);
            }
            b
        };
        assert!(QuotaGlobalRowV11::from_rooted_bytes(&change(&gb), &global.root()).is_err());
        assert!(QuotaActorRowV11::from_rooted_bytes(&change(&ab), &actor.root()).is_err());
        assert!(QuotaSubjectSummaryV11::from_rooted_row_bytes(&change(&sb), &subject.row_root()).is_err());
        assert!(ProtectedSealSummaryV11::from_rooted_row_bytes(&change(&pb), &seal.row_root()).is_err());
    }
    let global_root = global.root();
    let actor_root = actor.root();
    let subject_root = subject.row_root();
    let seal_root = seal.row_root();
    for field in 0..4 {
        let mut wrong = [0; 64];
        wrong[0] = 1;
        assert!(
            CheckedQuotaShardV11::from_native_rows(
                &f.2,
                &global,
                if field == 0 { &wrong } else { &global_root },
                f.5.seal_actor,
                f.5.job_id,
                Some(key),
                Some((&actor, if field == 1 { &wrong } else { &actor_root })),
                Some((subject, if field == 2 { &wrong } else { &subject_root })),
                Some((seal, if field == 3 { &wrong } else { &seal_root }))
            )
            .is_err()
        );
    }
}
#[test]
fn selected_shard_baseline_is_once_only_and_preserves_other_subject_aggregate_guarantees() {
    let (class, plan, policy, epoch, book, _, existing) = fixture();
    let prepared = super::tests::prepared_native_job(&class, &policy, &epoch, 1);
    let newbook = ExecutionJobStateV11::from_native_funded_job(prepared.job.clone()).unwrap();
    let checked = check_native_quota_admission_v11(&class, &newbook, &super::tests::native_admission(&newbook)).unwrap();
    let global = existing.global_row();
    let actor = existing.actor_row(&book.job().producer).unwrap();
    let mut shard = CheckedQuotaShardV11::from_native_rows(
        &policy,
        &global,
        &global.root(),
        book.job().producer,
        prepared.job.id(),
        None,
        Some((&actor, &actor.root())),
        None,
        None,
    )
    .unwrap();
    let mut full = existing.clone();
    assert_eq!(
        shard.admit_baseline(&policy, &epoch, &checked, &prepared.quota_plan).unwrap(),
        full.admit_baseline(&policy, &epoch, book.job().producer, &checked, &prepared.quota_plan).unwrap()
    );
    assert_eq!(shard.global_row(), full.global_row());
    assert_eq!(shard.actor_row(), full.actor_row(&book.job().producer).unwrap());
    assert_eq!(shard.subject_row(), full.subject(&prepared.job.id()));
    assert_eq!(full.subject(&book.job().id()), existing.subject(&book.job().id()));
    let before = shard.global_row();
    assert!(shard.admit_baseline(&policy, &epoch, &checked, &plan).is_err());
    assert_eq!(shard.global_row(), before);
    full.check_restored(&policy).unwrap();
}
#[test]
fn exact_source_pc_filing_and_fixed_native_factory_match_public_factory_without_underreservation() {
    let (class, program, pc, policy) = super::tests::checked_class_fixture();
    let r = class.record();
    let source = ex::native_source_court_bound_v11(program.encode().len(), &pc).unwrap();
    assert_eq!(source.max_proof_bytes, 4 + 67 * pc.by_instance.len() as u64 + (128 << 10));
    let candidate = crate::ledger::flat_candidate9::FlatCandidateRecordV9 {
        binding: r.binding_id,
        plan: class.view().plan.clone(),
        program_bytes: program.encode(),
        param_commitments: pc.clone(),
        scope_bytes: borsh::to_vec(&class.view().scope).unwrap(),
        replay_bytes: borsh::to_vec(&class.view().replay).unwrap(),
    };
    let binding = ex::NativeSourceBindingV11 {
        descriptor: r.descriptor,
        program_root: r.program_root,
        parameter_root: r.parameter_root,
        inventory_root: r.inventory_root,
        bound_daa: 1,
        matures_daa: 2,
        refuted: false,
    };
    let execution = ex::ExecutionPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: policy.network_domain,
        quota_policy_root: policy.root(),
        horizons: r.horizons.clone(),
        reward_cap: r.reward_cap,
        external_gain_cap: 0,
        default_obligation: 0,
        required_producer_reserve: r.required_producer_reserve,
        finder_permille: r.finder_permille,
    };
    let native = ex::restore_native_execution_class_header_v11(&candidate, &binding, Some(r), &execution, &policy, 10).unwrap();
    assert!(!native.can_admit());
    for p in 1..=3 {
        let q = derive_job_quota_plan_from_checked_native_class_v11(&native, p).unwrap();
        let public = derive_job_quota_plan_v11(&program, &pc, 3, p, None).unwrap();
        assert_eq!(q.record(), public.record());
        assert_eq!(q.source_court_bound(), &source);
        let computed = complete_scope_api_bounds_v9(&class.view().scope).unwrap();
        assert!(source.max_proof_bytes > computed.filing_bytes);
        assert_eq!(q.action_carrier_cap(QuotaActionKindV11::Chunk).unwrap(), source.max_proof_bytes + 7 + ACTION_OUTER_ALLOWANCE_V11);
        let protected = derive_protected_seal_plan_v11(&q).unwrap();
        assert_eq!(protected.fragment_count_cap, (source.max_proof_bytes + 7 + ACTION_OUTER_ALLOWANCE_V11).div_ceil(32768) as u32);
        assert!(q.action(QuotaActionKindV11::Court, &[]).unwrap().quantities().get(ResourceAxisV10::CourtWork) >= source.work as u128);
    }
}

fn load_selected(
    state: &QuotaStateV11,
    policy: &QuotaPolicyV11,
    actor: Digest,
    job: Digest,
    key: Option<Digest>,
) -> CheckedQuotaShardV11 {
    let global = state.global_row();
    let ar = state.actor_row(&actor);
    let subject = state.subject(&job);
    let seal = key.and_then(|key| state.protected_seal(&key));
    let ar_root = ar.as_ref().map(|r| r.root());
    let subject_root = subject.map(|r| r.row_root());
    let seal_root = seal.map(|r| r.row_root());
    CheckedQuotaShardV11::from_native_rows(
        policy,
        &global,
        &global.root(),
        actor,
        job,
        key,
        ar.as_ref().zip(ar_root.as_ref()),
        subject.zip(subject_root.as_ref()),
        seal.zip(seal_root.as_ref()),
    )
    .unwrap()
}

#[test]
fn selected_pair_sum_original_job_counts_attempts_and_lookup_keys_refuse_before_mutation() {
    let mut f = fixture();
    let key = admit(&mut f);
    let global = f.6.global_row();
    let actor = f.6.actor_row(&f.5.seal_actor).unwrap();
    let subject = f.6.subject(&f.5.job_id).unwrap();
    let seal = f.6.protected_seal(&key).unwrap();
    let load = |g: &QuotaGlobalRowV11, s: &QuotaSubjectSummaryV11, job, seal_key| {
        CheckedQuotaShardV11::from_native_rows(
            &f.2,
            g,
            &g.root(),
            f.5.seal_actor,
            job,
            seal_key,
            Some((&actor, &actor.root())),
            Some((s, &s.row_root())),
            Some((seal, &seal.row_root())),
        )
    };
    let mut smaller = global.clone();
    for axis in 0..QUOTA_AXES_V11 {
        smaller.network.reserved.0[axis] = subject.guarantee_remaining.0[axis].max(seal.guarantee_remaining.0[axis]);
    }
    smaller.block.reserved = smaller.network.reserved.clone();
    subject.guarantee_remaining.within(&smaller.network.reserved).unwrap();
    seal.guarantee_remaining.within(&smaller.network.reserved).unwrap();
    assert!(load(&smaller, subject, f.5.job_id, Some(key)).is_err());
    assert!(load(&global, subject, [1; 64], Some(key)).is_err());
    assert!(load(&global, subject, f.5.job_id, Some([1; 64])).is_err());
    for field in 0..4 {
        let mut s = subject.clone();
        match field {
            0 => s.retained_protected_seals = 0,
            1 => s.retained_protected_seals = 5,
            2 => s.future_attempts_used[0] = f.2.future_attempts[0] + 1,
            _ => s.binding.deadline_daa = f.2.interval_daa,
        }
        assert!(load(&global, &s, f.5.job_id, Some(key)).is_err());
    }
    // Full codec cross-checks the rooted per-job counter against actual rows.
    for count in [0, 2, 5] {
        let mut wrong = f.6.clone();
        wrong.subjects.get_mut(&f.5.job_id).unwrap().retained_protected_seals = count;
        let bytes = borsh::to_vec(&wrong).unwrap();
        assert!(QuotaStateV11::from_bytes(&f.2, &bytes).is_err());
        assert!(QuotaStateV11::from_rooted_bytes(&bytes, &f.2, &f.6.root()).is_err());
    }
}

#[test]
fn selected_producer_public_ordinary_and_late_terminal_debits_match_full_book_with_failed_attempts_retained() {
    let mut f = fixture();
    let key = admit(&mut f);
    let binding = f.4.job().quota_subject(f.0.record()).unwrap();
    let epoch = mature(&f.2);
    let mut producer = load_selected(&f.6, &f.2, binding.producer, binding.job_id, None);
    let answer = f.1.action(QuotaActionKindV11::DAAnswer, &[0; 64]).unwrap();
    assert_eq!(
        producer.debit_reserved(&f.2, &epoch, &binding, &f.1, &answer).unwrap(),
        f.6.debit(&f.2, &epoch, binding.producer, &binding, &f.1, &answer).unwrap()
    );
    assert_eq!(producer.global_row(), f.6.global_row());
    assert_eq!(producer.subject_row(), f.6.subject(&binding.job_id));
    let mut finder = load_selected(&f.6, &f.2, f.5.seal_actor, binding.job_id, Some(key));
    let before = finder.global_row();
    assert!(finder.debit_reserved(&f.2, &epoch, &binding, &f.1, &answer).is_err());
    assert_eq!(finder.global_row(), before);
    let ordinary = derive_ordinary_metadata_v11(&f.0.view().program, f.0.parameters()).unwrap();
    finder.debit_ordinary(&f.2, &epoch, &ordinary).unwrap();
    f.6.debit_ordinary(&f.2, &epoch, f.5.seal_actor, &ordinary).unwrap();
    let public = f.1.action(QuotaActionKindV11::Court, &[]).unwrap();
    finder.debit_public_action(&f.2, &epoch, &binding, &f.1, &public).unwrap();
    f.6.debit_public_action(&f.2, &epoch, f.5.seal_actor, &binding, &f.1, &public).unwrap();
    assert_eq!(finder.global_row(), f.6.global_row());
    assert_eq!(finder.actor_row(), f.6.actor_row(&f.5.seal_actor).unwrap());
    assert_eq!(finder.seal_row(), f.6.protected_seal(&key));
    assert_eq!(finder.subject_row(), f.6.subject(&binding.job_id));
    // A different authenticated public fee payer may close an old genuine job.
    let payer = [249; 64];
    let late = TrustedEpochV11::from_chain(&f.2, [250; 64], 1100).unwrap();
    let mut terminal_shard = load_selected(&f.6, &f.2, payer, binding.job_id, None);
    let paid = terminal_shard.debit_expired_terminal(&f.2, &late, &binding, &f.1).unwrap();
    let full_paid = f.6.debit_expired_terminal(&f.2, &late, payer, &binding, &f.1).unwrap();
    assert_eq!(paid.receipt(), full_paid.receipt());
    assert_eq!(terminal_shard.global_row(), f.6.global_row());
    assert!(!terminal_shard.subject_row().unwrap().closed);
    assert!(terminal_shard.mark_expired_terminal_closed(&f.2, &late, &binding, &f.0, &f.4, &paid).is_err());
    assert!(!terminal_shard.subject_row().unwrap().closed);
    let retention = f.4.claim().unwrap().retention_until;
    let close = ex::NativeCloseSnapshotV11 {
        current_daa: late.block_daa,
        pending_own_proofs: 0,
        open_da: 0,
        latest_answer_proof_until: retention,
        required_retained_until: retention,
    };
    f.4.finalize(f.0.record(), &close).unwrap();
    terminal_shard.mark_expired_terminal_closed(&f.2, &late, &binding, &f.0, &f.4, &paid).unwrap();
    f.6.mark_expired_terminal_closed(&f.2, &late, &binding, &f.0, &f.4, &full_paid).unwrap();
    assert_eq!(terminal_shard.global_row(), f.6.global_row());
    assert_eq!(terminal_shard.actor_row(), f.6.actor_row(&payer).unwrap());
    assert_eq!(terminal_shard.subject_row(), f.6.subject(&binding.job_id));
    assert_eq!(f.6.protected_seal(&key).unwrap().guarantee_remaining, finder.seal_row().unwrap().guarantee_remaining);
    assert!(terminal_shard.mark_expired_terminal_closed(&f.2, &late, &binding, &f.0, &f.4, &paid).is_err());
    f.6.check_restored(&f.2).unwrap();
}

#[test]
fn source_parameter_hash_payload_tree_axes_and_native_answer_capacity_are_independently_bounded() {
    let f = fixture();
    let pc = f.0.parameters();
    let pc_body = 4 + pc.by_instance.keys().map(|(_, l)| 67 + u128::from(l.is_some()) * 2).sum::<u128>();
    let source_hash = pc_body + 4 + 26 + 4096 * 16 + 64 * 128 + 1 + 4 + 4 * 8 + 128 + 16 + 128 + 32768 + 32 * 128;
    for kind in [QuotaActionKindV11::Court, QuotaActionKindV11::Chunk] {
        let q = f.1.action(kind, &[]).unwrap();
        assert_eq!(q.quantities().get(ResourceAxisV10::ParameterHashBytes), source_hash);
        assert_eq!(q.quantities().get(ResourceAxisV10::RawParameterStorage), pc_body + 4096 * 16 + 32768);
        assert_eq!(q.quantities().get(ResourceAxisV10::ParameterTreeScratch), 4096 * 16 + 4 * 8 + 96 * 64 + 8192);
        assert!(q.quantities().get(ResourceAxisV10::CopiedReferenceBytes) > 3 * f.1.action_carrier_cap(kind).unwrap() as u128);
    }
    for axis in [ResourceAxisV10::ParameterHashBytes, ResourceAxisV10::RawParameterStorage, ResourceAxisV10::ParameterTreeScratch] {
        let mut policy = f.2.clone();
        let q = f.1.action(QuotaActionKindV11::Court, &[]).unwrap();
        policy.plan.0[axis as usize] = q.quantities().get(axis) - 1;
        let before = f.6.root();
        assert!(q.quantities().within(&policy.plan).is_err());
        assert_eq!(f.6.root(), before);
    }
    assert!(f.2.check_native_protected_capacity(32).is_err());
    let mut policy = f.2.clone();
    policy.future_attempts[2] = 39;
    assert!(policy.check_native_protected_capacity(32).is_err());
    policy.future_attempts[2] = 40;
    policy.check_native_protected_capacity(32).unwrap();
    policy.future_attempts[2] = 48;
    policy.check_native_protected_capacity(32).unwrap();
    assert_eq!(PROTECTED_DA_REQUESTS_PER_JOB_MAX_V11, 8);
    assert!(policy.check_native_protected_capacity(56).is_err());
    policy.future_attempts[2] = 64;
    policy.check_native_protected_capacity(56).unwrap();
    assert!(policy.check_native_protected_capacity(u32::MAX).is_err());
}
