//! Synthetic funded-state economics only. Native original debit, source court,
//! protected trace and close rows are trusted boundary fixtures, not live-node
//! or checkpoint/eligibility/transport/Final evidence.
use misaka_palw_kernel::{
    descriptor::k2_tir_v4_descriptor,
    flat_execution11::*,
    flat_lengths7::FlatJobLengthsV7,
    flat_quota11::{QuotaPolicyV11, QuotaQuantitiesV11, TrustedEpochV11, quota_ruleset_id_v11},
    ledger::flat_candidate9::{FlatCandidateRecordV9, derive_flat_candidate_v9},
    trace::ParamCommitmentsV1,
};
use misaka_palw_tir::{DType, MapParams, Ref, Tensor, TensorType, builder::ProgramBuilder, program::HISTORY_BOUND_V1_SMALL};

fn fixture() -> (CheckedExecutionClassV11, QuotaPolicyV11, ExecutionPolicyV11, NativeSourceBindingV11, FlatCandidateRecordV9) {
    let mut b = ProgramBuilder::new(2, HISTORY_BOUND_V1_SMALL);
    let weight = b.param("offset", DType::I8, &[2], false);
    let previous = b.fixed_state("previous", DType::I32, &[2], -100, 100, false);
    let mut pre = b.block("pre", vec![]);
    let t = pre.cast(Ref::Input(0), DType::I32);
    let t = pre.broadcast(t, &[misaka_palw_tir::Dim::Fixed(2)]);
    let t = pre.add(t, weight, DType::I64);
    let t = pre.add(t, Ref::State(previous), DType::I64);
    let t = pre.clamp(t, -100, 100, DType::I32);
    let t = pre.state_write(previous, t);
    let pre = pre.finish(&[t]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let t = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(t);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let params = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![2], vec![3, 5]).unwrap())].into() };
    let pc = ParamCommitmentsV1::of_v3(&params);
    let view = derive_flat_candidate_v9(&k2_tir_v4_descriptor().digest(), &p.encode(), &pc, 4).unwrap();
    let source = NativeSourceBindingV11 {
        descriptor: view.plan.descriptor,
        program_root: view.plan.program_root,
        parameter_root: pc.root(),
        inventory_root: [4; 64],
        bound_daa: 0,
        matures_daa: 3,
        refuted: false,
    };
    let candidate = FlatCandidateRecordV9 {
        binding: source.id(),
        plan: view.plan,
        program_bytes: p.encode(),
        param_commitments: pc,
        scope_bytes: borsh::to_vec(&view.scope).unwrap(),
        replay_bytes: borsh::to_vec(&view.replay).unwrap(),
    };
    let cap = QuotaQuantitiesV11([u128::MAX / 4; 21]);
    let quota = QuotaPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: [2; 64],
        ruleset_digest: quota_ruleset_id_v11(),
        interval_daa: 1000,
        plan: cap.clone(),
        block: cap.clone(),
        actor: cap.clone(),
        network: cap,
        max_subjects: 8,
        max_actors: 8,
        future_attempts: [2, 2, 2, 2, 2, 1],
    };
    let policy = ExecutionPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: quota.network_domain,
        quota_policy_root: quota.root(),
        horizons: ExecutionHorizonsV11 {
            commit_window: 5,
            challenge_window: 4,
            seal_maturity: 3,
            seal_ttl: 100,
            da_answer: 3,
            proof_grace: 3,
            retention: 2,
        },
        reward_cap: 100,
        external_gain_cap: 20,
        default_obligation: 10,
        required_producer_reserve: 130,
        finder_permille: 490,
    };
    let class = derive_execution_class_v11(&candidate, &source, &policy, &quota, 10).unwrap();
    (class, quota, policy, source, candidate)
}
fn funded(class: &CheckedExecutionClassV11, quota: &QuotaPolicyV11) -> PreparedExecutionJobV11 {
    let epoch = TrustedEpochV11::from_chain(quota, [3; 64], 10).unwrap();
    let input = ExecutionJobInputV11 {
        version: 11,
        prompt_root: [5; 64],
        prompt_len: 2,
        generated_count: 2,
        lengths: FlatJobLengthsV7 { token: 3, patch: 0, audio: 0 },
        positions: 3,
    };
    prepare_execution_job_v11(class, quota, &epoch, input, [6; 64], [7; 64], [8; 64], 100, 100, [9; 64]).unwrap()
}
fn trace(class: &CheckedExecutionClassV11, job: &ExecutionJobRecordV11) -> NativeProtectedTraceV11 {
    let c = class.record();
    NativeProtectedTraceV11 {
        vector: [10; 64],
        candidate9_id: c.candidate9_id,
        binding_id: c.binding_id,
        class_scope9_root: c.class_scope9_root,
        replay9_root: c.replay9_root,
        class_resource10_root: c.resource10_root,
        prompt_root: job.input.prompt_root,
        prompt_len: job.input.prompt_len,
        positions: job.input.positions,
        generated_count: job.input.generated_count,
        trace_root: [11; 64],
        output_root: [12; 64],
        final_state_root: [13; 64],
    }
}
fn committed(class: &CheckedExecutionClassV11, quota: &QuotaPolicyV11) -> ExecutionJobStateV11 {
    let job = funded(class, quota).job;
    let t = trace(class, &job);
    let mut state = ExecutionJobStateV11::from_native_funded_job(job).unwrap();
    state.commit(class.record(), [7; 64], &t, 12, 130, [14; 64]).unwrap();
    state
}
fn ready(state: &ExecutionJobStateV11) -> NativeCloseSnapshotV11 {
    let retention = state.claim().unwrap().retention_until;
    NativeCloseSnapshotV11 {
        current_daa: retention + 1,
        pending_own_proofs: 0,
        open_da: 0,
        latest_answer_proof_until: 0,
        required_retained_until: retention,
    }
}
fn judgment(
    class: &CheckedExecutionClassV11,
    state: &ExecutionJobStateV11,
    kind: NativeAdjudicationKindV11,
) -> NativeExecutionJudgmentV11 {
    let c = state.claim().unwrap();
    NativeExecutionJudgmentV11 {
        job_id: state.job().id(),
        claim_id: c.id(),
        vector: c.vector,
        class_id: class.record().id(),
        inventory_root: class.record().inventory_root,
        parameter_root: class.record().parameter_root,
        current_daa: c.retention_until,
        established_daa: c.retention_until,
        proof_id: [15; 64],
        finder: [16; 64],
        kind,
    }
}
#[test]
fn execution_class_requires_exact_mature_source_policy_and_full_candidate() {
    let (_, quota, policy, source, candidate) = fixture();
    for case in 0..14 {
        let mut s = source.clone();
        let mut p = policy.clone();
        let mut q = quota.clone();
        let mut c = candidate.clone();
        let mut now = 10;
        match case {
            0 => s.refuted = true,
            1 => now = 2,
            2 => s.inventory_root = [99; 64],
            3 => p.enabled = false,
            4 => p.required_producer_reserve = 129,
            5 => p.quota_policy_root = [99; 64],
            6 => c.plan.resource_root = [99; 64],
            7 => c.scope_bytes.push(0),
            _ => {
                q.future_attempts[case - 8] = 0;
                p.quota_policy_root = q.root();
            }
        }
        assert!(derive_execution_class_v11(&c, &s, &p, &q, now).is_err());
    }
}

#[test]
fn native_nonce_encoding_and_operator_counter_have_no_padding_or_rotation_alias() {
    for n in [0, 1, 255, 256, u64::MAX] {
        assert_eq!(canonical_nonce_v11(&nonce_bytes_v11(n)).unwrap(), n);
    }
    let nonce = nonce_bytes_v11(7);
    for i in 8..64 {
        let mut bad = nonce;
        bad[i] = 1;
        assert!(canonical_nonce_v11(&bad).is_err());
    }
    assert_ne!(native_requester_nonce_counter_root_v11(&[1; 64], 7), native_requester_nonce_counter_root_v11(&[1; 64], 8));
    assert_ne!(native_requester_nonce_counter_root_v11(&[1; 64], 7), native_requester_nonce_counter_root_v11(&[2; 64], 7));
    assert_eq!(
        native_requester_nonce_counter_root_v11(&[1; 64], 7),
        misaka_palw_kernel::hash::object_id(b"misaka-palw/kernel/native-requester-nonce-counter/v11", &([1u8; 64], 7u64))
    );
}

#[test]
fn rooted_resource_frame_bounds_counts_before_allocation_and_rejects_foreign_roots() {
    use misaka_palw_kernel::flat_resources10::*;
    let (class, _, _, _, _) = fixture();
    let resource = &class.view().resources;
    let bytes = borsh::to_vec(resource).unwrap();
    let count = resource.primitives.len();
    assert_eq!(decode_rooted_flat_resources_v10(&bytes, resource.root(), count).unwrap(), *resource);
    let maximum = flat_resource_vector_max_bytes_v10(count).unwrap();
    assert_eq!(maximum, 545 + 117 * count);
    assert_eq!(bytes.len() + 6, maximum);
    assert_eq!(flat_resource_vector_restore_work_v10(count).unwrap(), maximum as u64 * 64 + count as u64 * 512 + 65536);
    assert!(flat_resource_vector_max_bytes_v10(usize::MAX).is_none());
    assert!(flat_resource_vector_restore_work_v10(usize::MAX).is_none());
    assert!(decode_rooted_flat_resources_v10(&bytes, [99; 64], count).is_err());
    assert!(decode_rooted_flat_resources_v10(&bytes, resource.root(), count + 1).is_err());
    for case in 0..6 {
        let mut bad = bytes.clone();
        match case {
            0 => bad[199..203].copy_from_slice(&u32::MAX.to_le_bytes()),
            1 => bad[198] = 2,
            2 => bad[0] = 0,
            3 => bad[194..198].fill(0),
            4 => bad.push(0),
            _ => {
                bad.pop();
            }
        }
        // Even if a corrupt local anchor matched these bytes, the fixed frame
        // grammar still cannot allocate a declared huge vector or accept tails.
        let bad_root = misaka_palw_kernel::hash::id(b"misaka-palw/kernel/reference-resource-vector/v10", &bad);
        assert!(decode_rooted_flat_resources_v10(&bad, bad_root, count).is_err(), "case {case}");
    }
}

#[test]
fn fast_native_class_restore_preserves_original_funding_after_source_revocation() {
    let (class, quota, policy, mut source, candidate) = fixture();
    let original = class.record().clone();
    let resources = borsh::to_vec(&class.view().resources).unwrap();
    let job = funded(&class, &quota).job;
    source.refuted = true;
    let restored = restore_native_execution_class_v11(&candidate, &source, &original, &policy, &quota, &resources, 12).unwrap();
    assert!(!restored.can_admit());
    assert_eq!(restored.record(), &original);
    assert_eq!(restored.view().resources, class.view().resources);
    let old = ExecutionJobStateV11::from_native_funded_job(job).unwrap();
    let mut book = ExecutionJobStateV11::from_bytes(&borsh::to_vec(&old).unwrap(), &restored, old.job().root(), old.root()).unwrap();
    assert_eq!(book.terminal(), ExecutionTerminalV11::Funded);
    assert_eq!(book.cancel_unclaimed(restored.record(), 2001).unwrap().requester_refund, 100);
    assert!(book.cancel_unclaimed(restored.record(), 2002).is_err());
}

#[test]
fn fast_native_restore_rejects_rooted_but_wrong_resource_namespace_and_primitives() {
    let (class, quota, policy, source, candidate) = fixture();
    for case in 0..8 {
        let mut resource = class.view().resources.clone();
        match case {
            0 => resource.program = [99; 64],
            1 => resource.parameters = [99; 64],
            2 => resource.court_scope = [99; 64],
            3 => resource.positions += 1,
            4 => resource.encoder = Some((0, 1)),
            5 => resource.primitives[0].occurrence += 1,
            6 => resource.primitives[0].node += 1,
            _ => resource.primitives[0].prim_tag ^= 1,
        }
        // Consistent forged root declarations are not enough: the decoded
        // namespace must also match the independently rechecked full scope.
        let mut c = candidate.clone();
        c.plan.resource_root = resource.root();
        let mut original = class.record().clone();
        original.resource10_root = resource.root();
        original.candidate9_id = c.id();
        assert!(
            restore_native_execution_class_v11(&c, &source, &original, &policy, &quota, &borsh::to_vec(&resource).unwrap(), 12)
                .is_err(),
            "case {case}"
        );
    }
    let raw = borsh::to_vec(&class.view().resources).unwrap();
    let mut original = class.record().clone();
    original.resource10_root = [99; 64];
    assert!(restore_native_execution_class_v11(&candidate, &source, &original, &policy, &quota, &raw, 12).is_err());
    let mut c = candidate.clone();
    c.scope_bytes.push(0);
    assert!(restore_native_execution_class_v11(&c, &source, class.record(), &policy, &quota, &raw, 12).is_err());
}

#[test]
fn native_eligibility_is_exact_ephemeral_and_cannot_be_reused_in_another_block() {
    let (class, quota, policy, source, candidate) = fixture();
    let raw = borsh::to_vec(&class.view().resources).unwrap();
    let restore = || restore_native_execution_class_v11(&candidate, &source, class.record(), &policy, &quota, &raw, 12).unwrap();
    let witness = NativeClassEligibilityV11 {
        class_id: class.record().id(),
        active_descriptor: class.record().descriptor,
        active_binding: class.record().binding_id,
        execution_policy_root: policy.root(),
        quota_policy_root: quota.root(),
        block_hash: [3; 64],
        current_daa: 12,
    };
    for case in 0..12 {
        let mut s = source.clone();
        let mut w = witness.clone();
        match case {
            0 => s.refuted = true,
            1 => s.matures_daa = 13,
            2 => s.inventory_root = [99; 64],
            3 => s.program_root = [99; 64],
            4 => s.parameter_root = [99; 64],
            5 => s.descriptor = [99; 64],
            6 => w.active_binding = [99; 64],
            7 => w.execution_policy_root = [99; 64],
            8 => w.quota_policy_root = [99; 64],
            9 => w.class_id = [99; 64],
            10 => w.active_descriptor = [99; 64],
            _ => s.bound_daa = s.matures_daa + 1,
        }
        assert!(authorize_native_execution_class_v11(restore(), &s, &w).is_err(), "case {case}");
    }
    let authorized = authorize_native_execution_class_v11(restore(), &source, &witness).unwrap();
    assert!(authorized.can_admit());
    let input = funded(&class, &quota).job.input;
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 12).unwrap();
    let prepare = |epoch: &TrustedEpochV11, nonce| {
        prepare_execution_job_v11(&authorized, &quota, epoch, input.clone(), [6; 64], [7; 64], nonce, 100, 100, [9; 64])
    };
    assert!(prepare(&epoch, nonce_bytes_v11(0)).is_ok());
    assert!(prepare(&epoch, [8; 64]).is_err());
    assert!(prepare(&TrustedEpochV11::from_chain(&quota, [4; 64], 12).unwrap(), nonce_bytes_v11(0)).is_err());
    assert!(prepare(&TrustedEpochV11::from_chain(&quota, [3; 64], 13).unwrap(), nonce_bytes_v11(0)).is_err());
}
#[test]
fn revoked_original_class_restores_existing_escrow_but_cannot_admit_another_job() {
    let (class, quota, policy, mut source, candidate) = fixture();
    let funded_job = funded(&class, &quota).job;
    let old_claim = committed(&class, &quota);
    let original_class = class.record().clone();
    source.refuted = true;
    assert!(derive_execution_class_v11(&candidate, &source, &policy, &quota, 12).is_err());
    let restored = restore_execution_class_v11(&candidate, &source, &original_class, &policy, &quota, 12).unwrap();
    assert!(!restored.can_admit());
    assert_eq!(restored.record(), &original_class);
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 12).unwrap();
    assert!(prepare_execution_job_v11(
        &restored, &quota, &epoch, funded_job.input.clone(), [6; 64], [7; 64], [18; 64], 100, 100, [19; 64],
    ).is_err());

    let mut claim =
        ExecutionJobStateV11::from_bytes(&borsh::to_vec(&old_claim).unwrap(), &restored, old_claim.job().root(), old_claim.root())
            .unwrap();
    assert_eq!(claim.terminal(), ExecutionTerminalV11::Challengeable);
    assert_eq!(claim.escrow_remaining(), 100);
    // Revocation itself generated no fault. Only a separately verified native
    // R/PC judgment over this exact old claim can settle its original reserve.
    let proof = judgment(&restored, &claim, NativeAdjudicationKindV11::SourceMismatch);
    let settlement = claim.adjudicate(restored.record(), &proof, 77).unwrap();
    assert_eq!(settlement.requester_refund, 100);
    assert_eq!(settlement.producer_credit, 0);
    assert_eq!(settlement.collected_original_slash, 77);
    assert!(claim.adjudicate(restored.record(), &proof, 77).is_err());

    let mut unclaimed = ExecutionJobStateV11::from_native_funded_job(funded_job).unwrap();
    assert_eq!(unclaimed.cancel_unclaimed(restored.record(), 2001).unwrap().requester_refund, 100);
    assert!(unclaimed.cancel_unclaimed(restored.record(), 2002).is_err());
}
#[test]
fn original_class_restore_rechecks_exact_source_candidate_policy_and_cannot_grant_admission() {
    let (class, quota, policy, source, candidate) = fixture();
    let restored = restore_execution_class_v11(&candidate, &source, class.record(), &policy, &quota, 10).unwrap();
    assert!(class.can_admit());
    assert!(!restored.can_admit());
    // Restoration remains settlement-only even when the source is eligible.
    let job = funded(&class, &quota).job;
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
    assert!(
        prepare_execution_job_v11(
            &restored,
            &quota,
            &epoch,
            job.input.clone(),
            job.requester,
            job.producer,
            [18; 64],
            100,
            100,
            [19; 64],
        )
        .is_err()
    );
    for case in 0..10 {
        let mut s = source.clone();
        let mut p = policy.clone();
        let mut q = quota.clone();
        let mut c = candidate.clone();
        let mut record = class.record().clone();
        let mut now = 10;
        match case {
            0 => s.inventory_root = [99; 64],
            1 => s.parameter_root = [99; 64],
            2 => s.program_root = [99; 64],
            3 => s.bound_daa = 4,
            4 => now = 2,
            5 => p.finder_permille = 491,
            6 => q.max_subjects = 7,
            7 => c.plan.resource_root = [99; 64],
            8 => c.scope_bytes.push(0),
            _ => record.reward_cap = 101,
        }
        assert!(restore_execution_class_v11(&c, &s, &record, &p, &q, now).is_err(), "case {case}");
    }
}
#[test]
fn funded_actual_job_binds_all_text_lengths_resource_and_single_interval() {
    let (class, quota, _, _, _) = fixture();
    let prepared = funded(&class, &quota);
    let record = prepared.quota_plan.record();
    assert_eq!(prepared.job.actual_resource10_root, record.resource_root);
    assert_eq!(prepared.job.actual_scope9_root, record.actual_scope_root);
    assert_ne!(prepared.job.actual_scope9_root, class.record().class_scope9_root);
    assert_eq!(prepared.job.quota_subject(class.record()).unwrap().quota_plan_root, record.root());
    for case in 0..8 {
        let mut input = prepared.job.input.clone();
        let mut reward = 100;
        let mut debit = 100;
        let mut daa = 10;
        match case {
            0 => input.lengths.token = 2,
            1 => input.lengths.patch = 1,
            2 => input.lengths.audio = 1,
            3 => input.generated_count = 1,
            4 => reward = 101,
            5 => debit = 99,
            6 => daa = 900,
            _ => daa = u64::MAX,
        }
        let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], daa).unwrap();
        assert!(prepare_execution_job_v11(&class, &quota, &epoch, input, [6; 64], [7; 64], [8; 64], reward, debit, [9; 64]).is_err());
    }
}
#[test]
fn one_claim_final_credit_is_original_escrow_and_cannot_repeat_after_restore() {
    let (class, quota, _, _, _) = fixture();
    let mut state = committed(&class, &quota);
    let job_root = state.job().root();
    let close = ready(&state);
    let paid = state.finalize(class.record(), &close).unwrap();
    assert_eq!((paid.escrow_debit, paid.producer_credit, paid.requester_refund, paid.collected_original_slash), (100, 100, 0, 0));
    assert_eq!(paid.release_producer_reserve, 130);
    assert!(state.claim().unwrap().credit_paid);
    let bytes = borsh::to_vec(&state).unwrap();
    let mut restored = ExecutionJobStateV11::from_bytes(&bytes, &class, job_root, state.root()).unwrap();
    let before = restored.root();
    assert!(restored.finalize(class.record(), &close).is_err());
    assert!(
        restored.adjudicate(class.record(), &judgment(&class, &restored, NativeAdjudicationKindV11::SourceMismatch), 130).is_err()
    );
    assert_eq!(before, restored.root());
}
#[test]
fn claim_wrong_actor_identity_clock_or_original_reserve_leaves_funding_unchanged() {
    let (class, quota, _, _, _) = fixture();
    let job = funded(&class, &quota).job;
    for case in 0..8 {
        let mut state = ExecutionJobStateV11::from_native_funded_job(job.clone()).unwrap();
        let before = state.root();
        let mut t = trace(&class, &job);
        let mut actor = job.producer;
        let mut daa = 12;
        let mut reserve = 130;
        match case {
            0 => actor = [99; 64],
            1 => reserve = 129,
            2 => daa = 9,
            3 => daa = 16,
            4 => t.prompt_root = [99; 64],
            5 => t.class_resource10_root = [99; 64],
            6 => t.generated_count = 1,
            _ => t.positions = 4,
        }
        assert!(state.commit(class.record(), actor, &t, daa, reserve, [14; 64]).is_err());
        assert_eq!(before, state.root());
    }
    let mut state = committed(&class, &quota);
    let before = state.root();
    assert!(state.commit(class.record(), job.producer, &trace(&class, &job), 13, 130, [17; 64]).is_err());
    assert_eq!(state.root(), before);
}
#[test]
fn pending_own_proofs_da_answer_and_retention_grace_block_final_atomically() {
    let (class, quota, _, _, _) = fixture();
    for case in 0..6 {
        let mut state = committed(&class, &quota);
        let before = state.root();
        let mut c = ready(&state);
        match case {
            0 => c.current_daa -= 1,
            1 => c.pending_own_proofs = 1,
            2 => c.open_da = 1,
            3 => c.latest_answer_proof_until = c.current_daa,
            4 => c.required_retained_until = c.current_daa,
            _ => c.required_retained_until -= 1,
        }
        assert!(state.finalize(class.record(), &c).is_err());
        assert_eq!(before, state.root());
    }
}
#[test]
fn all_native_faults_refund_one_escrow_and_pay_only_collected_original_slash() {
    let (class, quota, _, _, _) = fixture();
    for kind in [
        NativeAdjudicationKindV11::SourceMismatch,
        NativeAdjudicationKindV11::ComputationFault,
        NativeAdjudicationKindV11::DataUnavailable,
    ] {
        for collected in [0, 1, 77, 130] {
            let mut state = committed(&class, &quota);
            let j = judgment(&class, &state, kind);
            let settled = state.adjudicate(class.record(), &j, collected).unwrap();
            assert_eq!((settled.requester_refund, settled.producer_credit), (100, 0));
            assert_eq!(settled.finder_credit + settled.burned, collected);
            assert_eq!(settled.finder_credit, collected * 490 / 1000);
            assert!(!state.claim().unwrap().credit_paid);
            let before = state.root();
            assert!(state.adjudicate(class.record(), &j, collected).is_err());
            assert_eq!(before, state.root());
            ExecutionJobStateV11::from_bytes(&borsh::to_vec(&state).unwrap(), &class, state.job().root(), state.root()).unwrap();
        }
    }
}
#[test]
fn wrong_native_source_subject_late_judgment_and_excess_slash_are_atomic_refusals() {
    let (class, quota, _, _, _) = fixture();
    for case in 0..10 {
        let mut state = committed(&class, &quota);
        let before = state.root();
        let mut j = judgment(&class, &state, NativeAdjudicationKindV11::SourceMismatch);
        let mut collected = 130;
        match case {
            0 => j.job_id = [99; 64],
            1 => j.claim_id = [99; 64],
            2 => j.vector = [99; 64],
            3 => j.class_id = [99; 64],
            4 => j.inventory_root = [99; 64],
            5 => j.parameter_root = [99; 64],
            6 => j.current_daa += 1,
            7 => j.finder = state.job().producer,
            8 => collected = 131,
            _ => j.established_daa += 1,
        }
        assert!(state.adjudicate(class.record(), &j, collected).is_err());
        assert_eq!(before, state.root());
    }
}
#[test]
fn job_identity_cannot_change_with_funding_carrier_and_unclaimed_refunds_once() {
    let (class, quota, _, _, _) = fixture();
    let job = funded(&class, &quota).job;
    let mut alias = job.clone();
    alias.escrow_debit_receipt = [99; 64];
    alias.admitted_daa += 1;
    assert_eq!(job.id(), alias.id());
    assert_eq!(job.request_key(), alias.request_key());
    assert_ne!(job.root(), alias.root());
    alias.producer = [98; 64];
    alias.class_id = [97; 64];
    assert_eq!(job.request_key(), alias.request_key());
    let mut state = ExecutionJobStateV11::from_native_funded_job(job).unwrap();
    let before = state.root();
    assert!(state.cancel_unclaimed(class.record(), 15).is_err());
    assert_eq!(before, state.root());
    let refund = state.cancel_unclaimed(class.record(), 16).unwrap();
    assert_eq!(refund.requester_refund, 100);
    let bytes = borsh::to_vec(&state).unwrap();
    ExecutionJobStateV11::from_bytes(&bytes, &class, state.job().root(), state.root()).unwrap();
    assert!(ExecutionJobStateV11::from_bytes(&bytes, &class, [99; 64], state.root()).is_err());
    assert!(ExecutionJobStateV11::from_bytes(&bytes, &class, state.job().root(), [99; 64]).is_err());
    assert!(
        ExecutionJobStateV11::from_bytes(&vec![0; EXECUTION_STATE_MAX_BYTES_V11 + 1], &class, state.job().root(), state.root())
            .is_err()
    );
    let before = state.root();
    assert!(state.cancel_unclaimed(class.record(), 17).is_err());
    assert_eq!(before, state.root());
}

#[test]
fn execution_checked_money_overflow_and_corrupt_state_cannot_spend_or_rewind() {
    let (_, quota, mut policy, source, candidate) = fixture();
    policy.reward_cap = u64::MAX;
    policy.external_gain_cap = 0;
    policy.default_obligation = 0;
    policy.required_producer_reserve = u64::MAX;
    let class = derive_execution_class_v11(&candidate, &source, &policy, &quota, 10).unwrap();
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
    let input = funded(&class, &quota).job.input;
    let job =
        prepare_execution_job_v11(&class, &quota, &epoch, input, [6; 64], [7; 64], [8; 64], u64::MAX, u64::MAX, [9; 64]).unwrap().job;
    let t = trace(&class, &job);
    let mut state = ExecutionJobStateV11::from_native_funded_job(job).unwrap();
    state.commit(class.record(), [7; 64], &t, 12, u64::MAX, [14; 64]).unwrap();
    let before = state.root();
    let j = judgment(&class, &state, NativeAdjudicationKindV11::SourceMismatch);
    assert!(state.adjudicate(class.record(), &j, 1).is_err());
    assert_eq!(before, state.root());
    let bytes = borsh::to_vec(&state).unwrap();
    let mut corrupt = bytes.clone();
    // The final option is the settlement: a truncated/corrupt fixed grammar
    // cannot become a paid/rewound state even with the original job root.
    *corrupt.last_mut().unwrap() = 2;
    assert!(ExecutionJobStateV11::from_bytes(&corrupt, &class, state.job().root(), before).is_err());
    let close = ready(&state);
    state.finalize(class.record(), &close).unwrap();
    assert!(ExecutionJobStateV11::from_bytes(&bytes, &class, state.job().root(), state.root()).is_err());
}

#[test]
fn trusted_daa_jump_can_close_or_refund_expired_funding_without_reopening_courts() {
    let (class, quota, _, _, _) = fixture();
    let mut final_state = committed(&class, &quota);
    let mut close = ready(&final_state);
    close.current_daa = 2001; // Past original deadline and two quota intervals.
    assert_eq!(final_state.finalize(class.record(), &close).unwrap().producer_credit, 100);
    let mut unclaimed = ExecutionJobStateV11::from_native_funded_job(funded(&class, &quota).job).unwrap();
    assert_eq!(unclaimed.cancel_unclaimed(class.record(), 2001).unwrap().requester_refund, 100);
    let mut default = committed(&class, &quota);
    let mut j = judgment(&class, &default, NativeAdjudicationKindV11::DataUnavailable);
    j.current_daa = 2001; // Established immutable trace-DA default was within liability.
    assert_eq!(default.adjudicate(class.record(), &j, 130).unwrap().requester_refund, 100);
    for kind in [NativeAdjudicationKindV11::SourceMismatch, NativeAdjudicationKindV11::ComputationFault] {
        let mut live = committed(&class, &quota);
        let before = live.root();
        let mut late = judgment(&class, &live, kind);
        late.current_daa = 2001;
        assert!(live.adjudicate(class.record(), &late, 130).is_err());
        assert_eq!(before, live.root());
    }
}

fn light_fixture() -> (CheckedNativeExecutionClassV11, QuotaPolicyV11, NativeSourceBindingV11) {
    let (full, quota, policy, source, candidate) = fixture();
    let light = restore_native_execution_class_header_v11(&candidate, &source, Some(full.record()), &policy, &quota, 10).unwrap();
    assert_eq!(full.record(), light.record());
    assert!(!light.can_admit());
    let witness = NativeClassEligibilityV11 {
        class_id: light.record().id(),
        active_descriptor: source.descriptor,
        active_binding: source.id(),
        execution_policy_root: policy.root(),
        quota_policy_root: quota.root(),
        block_hash: [3; 64],
        current_daa: 10,
    };
    (authorize_native_execution_class_header_v11(light, &source, &witness).unwrap(), quota, source)
}
fn light_preparation(light: &CheckedNativeExecutionClassV11, quota: &QuotaPolicyV11) -> CheckedExecutionPreparationV11 {
    let (full, _, _, _, _) = fixture();
    let actual = funded(&full, quota);
    bind_native_execution_preparation_v11(light, quota, actual.job.input, actual.quota_plan, [6; 64], [7; 64], nonce_bytes_v11(0), 100)
        .unwrap()
}

#[test]
fn source_court_quote_binds_exact_pc_namespace_and_original_class_identity() {
    let (class, quota, policy, source, candidate) = fixture();
    let bound = &class.record().source_court_bound;
    assert_eq!(bound.max_proof_bytes, 4 + 67 + (128 << 10));
    let b = candidate.program_bytes.len() as u64;
    assert_eq!(bound.work, b * b + 64 * b + 8832 + 64 * (bound.max_proof_bytes + 7) + 65536);
    assert_eq!(bound.scratch_bytes, 8 * b + 512 + 8 * (bound.max_proof_bytes + 7) + 65536);
    let mut layered = candidate.param_commitments.clone();
    layered.by_instance.insert((1, Some(0)), [11; 64]);
    let layered_quote = native_source_court_bound_v11(candidate.program_bytes.len(), &layered).unwrap();
    assert_eq!(layered_quote.max_proof_bytes, bound.max_proof_bytes + 69);
    assert_eq!(layered_quote.work, bound.work + 8832 + 64 * 69);
    assert!(native_source_court_bound_v11(usize::MAX, &layered).is_err());
    for field in 0..3 {
        let mut original = class.record().clone();
        match field {
            0 => original.source_court_bound.max_proof_bytes += 1,
            1 => original.source_court_bound.work += 1,
            _ => original.source_court_bound.scratch_bytes += 1,
        }
        assert_ne!(original.id(), class.record().id());
        assert!(restore_native_execution_class_header_v11(&candidate, &source, Some(&original), &policy, &quota, 10).is_err());
    }
}

#[test]
fn native_fixed_header_checks_existing_book_without_granting_new_admission() {
    let (light, quota, _) = light_fixture();
    let original_header = light.header();
    let header = original_header.restore(original_header.root()).unwrap();
    assert!(!header.can_admit());
    assert!(header.native_admission_epoch().is_none());
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
    let prepared =
        prepare_execution_job_from_checked_plan_v11(&light, &quota, &epoch, light_preparation(&light, &quota), 100, [9; 64]).unwrap();
    let mut state = ExecutionJobStateV11::from_native_funded_job(prepared.job).unwrap();
    let bytes = borsh::to_vec(&state).unwrap();
    let mut restored = ExecutionJobStateV11::from_bytes(&bytes, &header, state.job().root(), state.root()).unwrap();
    assert_eq!(restored, state);
    let prep = light_preparation(&light, &quota);
    assert!(prepare_execution_job_from_checked_plan_v11(&header, &quota, &epoch, prep, 100, [9; 64]).is_err());
    state.cancel_unclaimed(header.record(), 16).unwrap();
    restored.cancel_unclaimed(header.record(), 16).unwrap();
    assert_eq!(state, restored);
    let closed = ExecutionJobStateV11::from_bytes(&borsh::to_vec(&state).unwrap(), &header, state.job().root(), state.root()).unwrap();
    assert_eq!(closed.settlement().unwrap().requester_refund, 100);
    assert!(ExecutionJobStateV11::from_bytes(&bytes, &header, state.job().root(), closed.root()).is_err());
}

#[test]
fn native_fixed_header_refuses_foreign_anchor_and_inconsistent_encoder_frame() {
    let (light, _, _) = light_fixture();
    let header = light.header();
    assert!(header.restore([0; 64]).is_err());
    for case in 0..5 {
        let mut corrupt = header.clone();
        match case {
            0 => corrupt.version = 12,
            1 => corrupt.record.version = 12,
            2 => corrupt.record.max_positions = 0,
            3 => corrupt.encoder_input_limit = Some(10),
            _ => {
                corrupt.record.encoder = true;
                corrupt.encoder_input_limit = Some(0);
            }
        }
        assert!(corrupt.restore(header.root()).is_err());
        assert!(corrupt.restore(corrupt.root()).is_err());
    }
}

#[test]
fn staged_native_fund_restores_exact_prompt_actors_nonce_reward_and_actual_plan() {
    let (light, quota, _) = light_fixture();
    let preparation = light_preparation(&light, &quota);
    let record = preparation.record().clone();
    let plan = preparation.quota_plan().record();
    let root = record.root();
    for case in 0..10 {
        let mut changed = record.clone();
        match case {
            0 => changed.version = 12,
            1 => changed.class_id[0] ^= 1,
            2 => changed.input.prompt_root[0] ^= 1,
            3 => changed.input.prompt_len += 1,
            4 => changed.input.generated_count += 1,
            5 => changed.requester[0] ^= 1,
            6 => changed.producer[0] ^= 1,
            7 => changed.nonce = nonce_bytes_v11(1),
            8 => changed.reward -= 1,
            _ => changed.quota_plan_root[0] ^= 1,
        }
        assert!(CheckedExecutionPreparationV11::restore(&changed, &plan, root).is_err());
    }
    let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
    let restored = CheckedExecutionPreparationV11::restore(&record, &plan, root).unwrap();
    let funded = prepare_execution_job_from_checked_plan_v11(&light, &quota, &epoch, restored, 100, [9; 64]).unwrap();
    assert_eq!(funded.job.input, record.input);
    assert_eq!(funded.job.requester, record.requester);
    assert_eq!(funded.job.producer, record.producer);
    assert_eq!(funded.job.nonce, record.nonce);
    assert_eq!(funded.job.reward, record.reward);
    assert_eq!(funded.job.quota_plan_root, record.quota_plan_root);
    assert_eq!(funded.job.actual_resource10_root, plan.resource_root);
    assert_eq!(funded.job.actual_scope9_root, plan.actual_scope_root);
}

#[test]
fn staged_native_fund_rechecks_current_epoch_original_debit_and_plan_namespace() {
    let (light, quota, _) = light_fixture();
    let prep = light_preparation(&light, &quota);
    let record = prep.record().clone();
    let plan = prep.quota_plan().record();
    for case in 0..6 {
        let mut epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
        let mut debit = 100;
        match case {
            0 => epoch.block_hash[0] ^= 1,
            1 => epoch.block_daa += 1,
            2 => epoch.network_interval += 1,
            3 => debit = 99,
            4 => debit = 101,
            _ => epoch.block_daa = 2001,
        }
        let restored = CheckedExecutionPreparationV11::restore(&record, &plan, record.root()).unwrap();
        assert!(prepare_execution_job_from_checked_plan_v11(&light, &quota, &epoch, restored, debit, [9; 64]).is_err());
    }
    // Even a consistently rooted synthetic corrupted native record cannot cross
    // the class/input namespace. Real anchors come from native state only.
    for case in 0..5 {
        let mut changed_plan = plan.clone();
        match case {
            0 => changed_plan.program_root[0] ^= 1,
            1 => changed_plan.parameter_root[0] ^= 1,
            2 => changed_plan.class_scope_root[0] ^= 1,
            3 => changed_plan.positions += 1,
            _ => changed_plan.source_court_bound.work -= 1,
        }
        let mut changed_prep = record.clone();
        changed_prep.quota_plan_root = changed_plan.root();
        let restored = CheckedExecutionPreparationV11::restore(&changed_prep, &changed_plan, changed_prep.root()).unwrap();
        let epoch = TrustedEpochV11::from_chain(&quota, [3; 64], 10).unwrap();
        assert!(prepare_execution_job_from_checked_plan_v11(&light, &quota, &epoch, restored, 100, [9; 64]).is_err());
    }
}

#[test]
fn revoked_source_allows_native_light_book_refund_but_refuses_fresh_eligibility() {
    let (full, quota, policy, mut source, candidate) = fixture();
    source.refuted = true;
    let light = restore_native_execution_class_header_v11(&candidate, &source, Some(full.record()), &policy, &quota, 10).unwrap();
    let witness = NativeClassEligibilityV11 {
        class_id: light.record().id(),
        active_descriptor: source.descriptor,
        active_binding: source.id(),
        execution_policy_root: policy.root(),
        quota_policy_root: quota.root(),
        block_hash: [3; 64],
        current_daa: 10,
    };
    let header_record = light.header();
    assert!(authorize_native_execution_class_header_v11(light, &source, &witness).is_err());
    let header = header_record.restore(header_record.root()).unwrap();
    let mut state = ExecutionJobStateV11::from_native_funded_job(funded(&full, &quota).job).unwrap();
    state.check_restored(&header, state.job().root()).unwrap();
    assert_eq!(state.cancel_unclaimed(header.record(), 16).unwrap().requester_refund, 100);
}

fn established_court(j: &NativeExecutionJudgmentV11) -> NativeEstablishedExecutionCourtV11 {
    NativeEstablishedExecutionCourtV11 {
        job_id: j.job_id,
        claim_id: j.claim_id,
        vector: j.vector,
        class_id: j.class_id,
        inventory_root: j.inventory_root,
        parameter_root: j.parameter_root,
        established_daa: j.established_daa,
        proof_id: j.proof_id,
        kind: j.kind,
        original_finder: j.finder,
        eligible_finder: Some(j.finder),
    }
}

#[test]
fn original_verified_court_can_settle_late_once_without_granting_a_new_late_court() {
    let (class, quota, _, _, _) = fixture();
    for kind in [NativeAdjudicationKindV11::SourceMismatch, NativeAdjudicationKindV11::ComputationFault] {
        let mut state = committed(&class, &quota);
        let mut j = judgment(&class, &state, kind);
        let original = established_court(&j);
        j.current_daa = 2001;
        let before = state.root();
        assert!(state.adjudicate(class.record(), &j, 130).is_err());
        assert_eq!(state.root(), before);
        let settlement = state.adjudicate_established(class.record(), &j, &original, 130).unwrap();
        assert_eq!(settlement.requester_refund, 100);
        assert_eq!(settlement.finder_credit, 63);
        assert_eq!(settlement.burned, 67);
        let bytes = borsh::to_vec(&state).unwrap();
        let mut restored = ExecutionJobStateV11::from_bytes(&bytes, &class, state.job().root(), state.root()).unwrap();
        let after = restored.root();
        assert!(restored.adjudicate_established(class.record(), &j, &original, 130).is_err());
        assert_eq!(restored.root(), after);
    }
}

#[test]
fn historical_court_rejects_foreign_subject_proof_or_backdated_liability_atomically() {
    let (class, quota, _, _, _) = fixture();
    for case in 0..14 {
        let mut state = committed(&class, &quota);
        let mut j = judgment(&class, &state, NativeAdjudicationKindV11::SourceMismatch);
        j.current_daa = 2001;
        let mut original = established_court(&j);
        match case {
            0 => original.job_id[0] ^= 1,
            1 => original.claim_id[0] ^= 1,
            2 => original.vector[0] ^= 1,
            3 => original.class_id[0] ^= 1,
            4 => original.inventory_root[0] ^= 1,
            5 => original.parameter_root[0] ^= 1,
            6 => original.proof_id[0] ^= 1,
            7 => original.kind = NativeAdjudicationKindV11::ComputationFault,
            8 => original.established_daa += 1,
            9 => {
                j.established_daa = state.claim().unwrap().retention_until + 1;
                original.established_daa = j.established_daa;
            }
            10 => {
                j.established_daa = state.claim().unwrap().committed_daa - 1;
                original.established_daa = j.established_daa;
            }
            11 => j.current_daa = j.established_daa - 1,
            12 => original.original_finder[0] ^= 1,
            _ => original.eligible_finder = Some([99; 64]),
        }
        let before = state.root();
        assert!(state.adjudicate_established(class.record(), &j, &original, 130).is_err());
        assert_eq!(state.root(), before);
    }
    let mut state = committed(&class, &quota);
    let mut j = judgment(&class, &state, NativeAdjudicationKindV11::DataUnavailable);
    let anchor = established_court(&j);
    j.current_daa = 2001;
    assert!(state.adjudicate_established(class.record(), &j, &anchor, 130).is_err());
    assert!(state.adjudicate(class.record(), &j, 130).is_ok());
}

#[test]
fn historical_source_theorem_burns_same_operator_bounty_and_never_pays_a_close_caller() {
    let (class, quota, _, _, _) = fixture();
    for same_bond in [false, true] {
        let mut state = committed(&class, &quota);
        let mut j = judgment(&class, &state, NativeAdjudicationKindV11::SourceMismatch);
        if same_bond {
            j.finder = state.job().producer;
        }
        let mut original = established_court(&j);
        // Native authenticates equal operator IDs, even when their bonds differ.
        original.eligible_finder = None;
        j.current_daa = 2001;
        let result = state.adjudicate_established(class.record(), &j, &original, 130).unwrap();
        assert_eq!(result.requester_refund, 100);
        assert_eq!(result.collected_original_slash, 130);
        assert_eq!(result.finder, None);
        assert_eq!(result.finder_credit, 0);
        assert_eq!(result.burned, 130);
        assert_eq!(result.producer_credit, 0);
        ExecutionJobStateV11::from_bytes(&borsh::to_vec(&state).unwrap(), &class, state.job().root(), state.root()).unwrap();
    }
    let mut state = committed(&class, &quota);
    let mut j = judgment(&class, &state, NativeAdjudicationKindV11::SourceMismatch);
    j.finder = state.job().producer;
    j.current_daa = 2001;
    let original = established_court(&j); // Attempted self entitlement.
    let before = state.root();
    assert!(state.adjudicate_established(class.record(), &j, &original, 130).is_err());
    assert_eq!(state.root(), before);
}
