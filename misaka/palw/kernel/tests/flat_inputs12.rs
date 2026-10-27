use misaka_palw_kernel::flat_inputs12::bounded14::*;
use misaka_palw_kernel::{flat_inputs12::*, flat_lengths7::*, merkle3::tensor_commitment_v3, trace::ParamCommitmentsV1};
use misaka_palw_tir::{
    DType, MapParams, ParamSource, Ref, Tensor, TensorType,
    builder::ProgramBuilder,
    interp_v2::{InputProvider, InterpreterV2},
    program::HISTORY_BOUND_V1_SMALL,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
use std::cell::Cell;

fn fixture() -> (FlatInputTemplateV12, MapParams, ParamCommitmentsV1) {
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let patches = b.param("patches", DType::I32, &[4], false);
    let audio = b.param("audio", DType::I32, &[6], false);
    let offset = b.param("offset", DType::I8, &[1], false);
    let state = b.fixed_state("previous", DType::I32, &[1], 0, 100, false);
    let mut pre = b.block("pre", vec![]);
    let p = pre.reduce_sum(patches, 0, DType::I64);
    let a = pre.reduce_sum(audio, 0, DType::I64);
    let y = pre.add(p, a, DType::I64);
    let y = pre.add(y, offset, DType::I64);
    let y = pre.add(y, Ref::State(state), DType::I64);
    let y = pre.clamp(y, 0, 100, DType::I32);
    let y = pre.state_write(state, y);
    let pre = pre.finish(&[y]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let y = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(y);
    let post = post.finish(&[]);
    let old = b.finish(pre, vec![], post, 0);
    let program = TirProgramV2::from_v1_lifting_params(
        &old,
        &[(0, InputSource::External { lo: 0, hi: 20 }), (1, InputSource::External { lo: 0, hi: 20 })],
        OutputDecl::Rows { node: 0 },
    )
    .unwrap();
    let source = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![1], vec![5]).unwrap())].into_iter().collect() };
    let pc =
        ParamCommitmentsV1 { by_instance: [((0, None), tensor_commitment_v3(&source.tensors[&(0, None)]))].into_iter().collect() };
    let lengths = FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7 { token: 0, patch: 4, audio: 6 },
        max_program_bytes: program.v1_view().encode(),
        bindings: vec![
            FlatShapeBindingV7 { slot: FlatShapeSlotV7::Parameter { parameter: 1, axis: 0 }, length: FlatLengthAxisV7::Patch },
            FlatShapeBindingV7 { slot: FlatShapeSlotV7::Parameter { parameter: 2, axis: 0 }, length: FlatLengthAxisV7::Audio },
        ],
    };
    (FlatInputTemplateV12 { program, lengths, max_positions: 3, source_parameters: pc.root() }, source, pc)
}
struct Inputs {
    shape: [Vec<u32>; 2],
    raw: [Vec<u8>; 2],
    calls: Cell<u32>,
    dtype: DType,
}
impl Inputs {
    fn new() -> Self {
        Self {
            shape: [vec![3], vec![5]],
            raw: [
                vec![1i32, 2, 3].into_iter().flat_map(i32::to_le_bytes).collect(),
                vec![1i32; 5].into_iter().flat_map(i32::to_le_bytes).collect(),
            ],
            calls: Cell::new(0),
            dtype: DType::I32,
        }
    }
}
impl RawExternalInputsV12 for Inputs {
    fn input(&self, i: u16) -> Option<RawExternalInputV12<'_>> {
        self.calls.set(self.calls.get() + 1);
        Some(RawExternalInputV12 { dtype: self.dtype, shape: self.shape.get(i as usize)?, bytes: self.raw.get(i as usize)? })
    }
}
fn declaration(a: &FlatInputAdmissionV12) -> FlatInputCommitmentsV12 {
    FlatInputCommitmentsV12 {
        version: 12,
        admission: a.root(),
        job: [9; 64],
        input_roots: [Tensor::new(DType::I32, vec![3], vec![1, 2, 3]).unwrap(), Tensor::new(DType::I32, vec![5], vec![1; 5]).unwrap()]
            .iter()
            .map(tensor_commitment_v3)
            .collect(),
    }
}
fn full_budget() -> FlatInputBudgetV12 {
    FlatInputBudgetV12 { raw_bytes: 1000, reference_bytes: 1000, attempts: 2 }
}

fn stage_job(template: &FlatInputTemplateV12) -> FlatStageJobV14 {
    let actual = FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 };
    let a = template.specialize(actual, 3).unwrap();
    FlatStageJobV14 {
        version: 14,
        template: template.root().unwrap(),
        requester: [1; 64],
        producer: [2; 64],
        nonce: [3; 64],
        actual,
        positions: 3,
        input_roots: declaration(&a.admission).input_roots,
    }
}

#[test]
fn bounded_stage14_binds_maximum_and_actual_resources_to_nonce_inputs_and_source() {
    use misaka_palw_kernel::flat_resources10::ResourceAxisV10;
    let (template, source, pc) = fixture();
    let job = stage_job(&template);
    let checked = derive_stage_job_v14(&template, &job, &pc).unwrap();
    let a = checked.admission();
    assert_ne!(a.max_stage_program, a.actual_stage_program);
    assert_ne!(a.max_scope, a.actual_scope);
    assert_ne!(a.max_resources, a.actual_resources);
    assert_eq!(a.actual_scope, checked.scope().root());
    assert_eq!(a.actual_resources, checked.resources().root());
    assert_eq!(a.actual_replay, checked.replay().root());
    assert!(
        checked.resources().quantities.get(ResourceAxisV10::RawParameterStorage)
            < checked.maximum_resources().quantities.get(ResourceAxisV10::RawParameterStorage)
    );
    let inputs = checked.acquire(&template, &pc, &Inputs::new(), &mut full_budget()).unwrap();
    let out = InterpreterV2::new(checked.program()).unwrap().run_positions(&source, &inputs, job.positions).unwrap();
    assert_eq!(out.iter().map(|s| s.output.data[0]).collect::<Vec<_>>(), vec![16, 32, 48]);
    check_stage_job_v14(&template, &job, &pc, a).unwrap();
    // Every identity and independently derived maximum/actual bound is checked.
    for field in 0..17 {
        let mut bad = a.clone();
        match field {
            0 => bad.version += 1,
            1 => bad.template[0] ^= 1,
            2 => bad.job[0] ^= 1,
            3 => bad.source_parameters[0] ^= 1,
            4 => bad.maxima.patch += 1,
            5 => bad.max_positions += 1,
            6 => bad.max_stage_program[0] ^= 1,
            7 => bad.max_scope[0] ^= 1,
            8 => bad.max_replay[0] ^= 1,
            9 => bad.max_resources[0] ^= 1,
            10 => bad.actual_input_admission[0] ^= 1,
            11 => bad.actual_stage_program[0] ^= 1,
            12 => bad.actual_scope[0] ^= 1,
            13 => bad.actual_replay[0] ^= 1,
            14 => bad.actual_resources[0] ^= 1,
            15 => bad.combined_parameters[0] ^= 1,
            _ => bad.maxima.audio += 1,
        }
        assert!(check_stage_job_v14(&template, &job, &pc, &bad).is_err(), "rebound admission field {field}");
    }
    for field in 0..8 {
        let mut bad = job.clone();
        match field {
            0 => bad.requester[0] ^= 1,
            1 => bad.producer[0] ^= 1,
            2 => bad.nonce[0] ^= 1,
            3 => bad.input_roots[0][0] ^= 1,
            4 => bad.actual.patch = 4,
            5 => bad.positions = 2,
            6 => bad.actual.audio = 7,
            _ => bad.positions = 4,
        }
        assert!(check_stage_job_v14(&template, &bad, &pc, a).is_err(), "rebound job field {field}");
    }
    let mut bad_pc = pc.clone();
    let root = bad_pc.by_instance.remove(&(0, None)).unwrap();
    bad_pc.by_instance.insert((0, Some(0)), root);
    assert!(derive_stage_job_v14(&template, &job, &bad_pc).is_err());
}

#[test]
fn bounded_stage14_wire_checks_input_count_before_allocation_and_preserves_v12() {
    let (template, _, _) = fixture();
    let job = stage_job(&template);
    let wire = borsh::to_vec(&job).unwrap();
    assert_eq!(FlatStageJobV14::from_bytes(&wire).unwrap(), job);
    for end in 0..wire.len() {
        assert!(FlatStageJobV14::from_bytes(&wire[..end]).is_err());
    }
    let mut bad = wire.clone();
    bad[274..278].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatStageJobV14::from_bytes(&bad).is_err());
    let mut bad = wire.clone();
    bad[..2].copy_from_slice(&12u16.to_le_bytes());
    assert!(FlatStageJobV14::from_bytes(&bad).is_err());
    let mut bad = wire;
    bad.push(0);
    assert!(FlatStageJobV14::from_bytes(&bad).is_err());
    // V14 leaves the existing component root domain and admission version intact.
    let v12 = template.specialize(job.actual, job.positions).unwrap();
    assert_eq!(v12.admission.version, 12);
    assert_eq!(v12.admission.root(), template.check(&v12.admission).unwrap().admission.root());
}

#[test]
fn stage14_source_class_wires_exclude_virtual_inputs_and_bind_native_policy() {
    use misaka_palw_kernel::flat_inputs12::source14::*;
    let (template, _, pc) = fixture();
    let wire = FlatStageSourceWireV14 {
        version: 14,
        program_bytes: template.program.encode(),
        inventory_root: [7; 64],
        source_parameters: pc,
    };
    let source = wire.check().unwrap();
    source.check_template(&template).unwrap();
    let encoded = borsh::to_vec(&wire).unwrap();
    assert_eq!(FlatStageSourceWireV14::from_bytes(&encoded).unwrap(), wire);
    for end in 0..encoded.len() {
        assert!(FlatStageSourceWireV14::from_bytes(&encoded[..end]).is_err());
    }
    let mut huge = encoded.clone();
    huge[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatStageSourceWireV14::from_bytes(&huge).is_err());
    let mut bad = wire.clone();
    bad.source_parameters.by_instance.insert((1, None), [8; 64]);
    assert!(bad.check().is_err(), "an external input can never become an original source weight");
    let mut other_inventory = wire.clone();
    other_inventory.inventory_root[0] ^= 1;
    assert_ne!(source.id(), other_inventory.check().unwrap().id());
    assert_ne!(source.relation_root(), other_inventory.check().unwrap().relation_root());
    let mut relabeled = template.program.clone();
    relabeled.blocks[0].name = "different-stage-label".into();
    let mut alias = wire.clone();
    alias.program_bytes = relabeled.encode();
    let alias = alias.check().unwrap();
    assert_ne!(source.id(), alias.id());
    assert_eq!(source.relation_root(), alias.relation_root(), "same invalid real R/PC cannot regain admission through program labels");
    let class_wire = FlatStageClassWireV14 {
        version: 14,
        source_binding: source.id(),
        length_template_bytes: template.lengths.to_bytes().unwrap(),
        max_positions: 3,
        policy_root: [9; 64],
    };
    let class = class_wire.check(&wire).unwrap();
    class.check_job(&stage_job(&template)).unwrap();
    let encoded_class = borsh::to_vec(&class_wire).unwrap();
    assert_eq!(FlatStageClassWireV14::from_bytes(&encoded_class).unwrap(), class_wire);
    let mut huge = encoded_class.clone();
    huge[66..70].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(FlatStageClassWireV14::from_bytes(&huge).is_err());
    for end in 0..encoded_class.len() {
        assert!(FlatStageClassWireV14::from_bytes(&encoded_class[..end]).is_err());
    }
    let mut other_policy = class_wire.clone();
    other_policy.policy_root[0] ^= 1;
    assert_ne!(class.root(), other_policy.check(&wire).unwrap().root());
    let mut absent_policy = class_wire.clone();
    absent_policy.policy_root = [0; 64];
    assert!(absent_policy.check(&wire).is_err());
    let mut rebind = class_wire;
    rebind.source_binding[0] ^= 1;
    assert!(rebind.check(&wire).is_err());
}

#[test]
fn stage14_funding_shared_nonce_original_debit_quota_and_authenticated_output_cannot_rebind() {
    use misaka_palw_kernel::{
        flat_court5::flat_profile_commitment_v5,
        flat_execution11::{native_requester_nonce_counter_root_v11, *},
        flat_inputs12::source14::*,
        flat_quota11::{stage14::*, *},
        flat_resources10::ResourceAxisV10,
        seg::SegmentedCommitmentsV1,
        trace::trace_v1,
    };
    let (template, source, pc) = fixture();
    let source_wire = FlatStageSourceWireV14 {
        version: 14,
        program_bytes: template.program.encode(),
        inventory_root: [7; 64],
        source_parameters: pc.clone(),
    };
    let class_wire = FlatStageClassWireV14 {
        version: 14,
        source_binding: source_wire.check().unwrap().id(),
        length_template_bytes: template.lengths.to_bytes().unwrap(),
        max_positions: 3,
        policy_root: [9; 64],
    };
    let class = class_wire.check(&source_wire).unwrap();
    let cap = QuotaQuantitiesV11([1 << 56; 21]);
    let policy = QuotaPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: [4; 64],
        ruleset_digest: quota_ruleset_id_v11(),
        interval_daa: 1000,
        plan: cap.clone(),
        block: cap.clone(),
        actor: cap.clone(),
        network: cap,
        max_subjects: 16,
        max_actors: 16,
        future_attempts: [2, 2, 64, 2, 2, 1],
    };
    let execution = ExecutionPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: policy.network_domain,
        quota_policy_root: policy.root(),
        horizons: ExecutionHorizonsV11 {
            commit_window: 10,
            challenge_window: 10,
            seal_maturity: 3,
            seal_ttl: 100,
            da_answer: 10,
            proof_grace: 10,
            retention: 10,
        },
        reward_cap: 100,
        external_gain_cap: 10,
        default_obligation: 20,
        required_producer_reserve: 130,
        finder_permille: 500,
    };
    let mut job = stage_job(&template);
    job.nonce = nonce_bytes_v11(0);
    let actual = class.check_job(&job).unwrap();
    let plan = derive_stage_quota_v14(&class, &actual).unwrap();
    let discovery = discovery13::DiscoveryPolicyV13 {
        version: 13,
        quota_policy_root: policy.root(),
        requests_per_session: 2,
        ordinary_lifetime_requests: 32,
    };
    let source_plan = derive_stage_source_plan_v14(&class, &plan, &policy, &discovery).unwrap();
    let trace_plan = derive_stage_trace_plan_v14(&class, &plan, &policy).unwrap();
    assert_eq!(plan.plan().record().source_court_bound, native_source_court_bound_v11(source_wire.program_bytes.len(), &pc).unwrap());
    assert_eq!(plan.plan().record().class_scope_root, class.maximum_scope().root());
    assert_eq!(plan.plan().record().actual_scope_root, actual.scope().root());
    let native = NativeStageFundingV14 {
        class: class.root(),
        source: class.source().id(),
        policy: class.policy_root(),
        acceptance: [6; 64],
        consent_version: 18,
        source_plan: source_plan.root(),
        trace_plan: trace_plan.root(),
        fee_plan: [22; 64],
        availability_terms: [23; 64],
        source_matures_daa: 5,
        source_refuted: false,
        requester: job.requester,
        requester_operator: [11; 64],
        producer: job.producer,
        producer_operator: [12; 64],
        block: [8; 64],
        daa: 10,
        reward: 20,
        original_debit: 20,
        next_nonce: 0,
        nonce_counter_root: native_requester_nonce_counter_root_v11(&[11; 64], 0),
    };
    let book = StageJobBookV14::fund(&class, &actual, &plan, &execution, &policy, &native).unwrap();
    let raw = borsh::to_vec(&book).unwrap();
    StageJobBookV14::from_rooted_bytes(&raw, book.root()).unwrap().check_restored(&class, &plan).unwrap();
    let mut inflated = raw.clone();
    inflated[468..472].copy_from_slice(&u32::MAX.to_le_bytes());
    assert!(StageJobBookV14::from_rooted_bytes(&inflated, book.root()).is_err());
    let mut native = native;
    native.next_nonce = 1;
    native.nonce_counter_root = native_requester_nonce_counter_root_v11(&native.requester_operator, 1);
    assert!(
        StageJobBookV14::fund(&class, &actual, &plan, &execution, &policy, &native).is_err(),
        "text11 or earlier stage nonce cannot re-enter another domain"
    );
    native.next_nonce = 0;
    native.nonce_counter_root = native_requester_nonce_counter_root_v11(&native.requester_operator, 0);
    native.original_debit = 19;
    assert!(StageJobBookV14::fund(&class, &actual, &plan, &execution, &policy, &native).is_err());
    let anchor = NativeStageQuotaAnchorV14 {
        class: class.root(),
        source: class.source().id(),
        policy: class.policy_root(),
        acceptance: [6; 64],
        source_plan: source_plan.root(),
        trace_plan: trace_plan.root(),
        fee_plan: [22; 64],
        availability_terms: [23; 64],
        funded_book_root: book.root(),
        job_id: book.id(),
        original_debit_receipt: book.debit_receipt(),
        original_debit: 20,
        nonce: NativeRequesterNonceWitnessV11 {
            requester: job.requester,
            requester_operator_id: [11; 64],
            next_nonce: 1,
            counter_root: native_requester_nonce_counter_root_v11(&[11; 64], 1),
        },
        block: [8; 64],
        daa: 10,
        source_refuted: false,
        source_matures_daa: 5,
    };
    let grant = check_stage_quota_admission_v14(&class, &book, &plan, &policy, &anchor).unwrap();
    let epoch = TrustedEpochV11::from_chain(&policy, [8; 64], 10).unwrap();
    let mut state = QuotaStateV11::new(&policy, epoch.clone()).unwrap();
    state.admit_baseline(&policy, &epoch, job.producer, &grant, plan.plan()).unwrap();
    assert!(state.admit_baseline(&policy, &epoch, job.producer, &grant, plan.plan()).is_err());
    assert!(source_plan.requests() >= 44);
    let source_admission =
        check_stage_source_admission_v14(&class, &book, &plan, &policy, &discovery, &epoch, &source_plan, &anchor).unwrap();
    let global = state.global_row();
    let producer = state.actor_row(&job.producer).unwrap();
    let subject = state.subject(&book.id()).unwrap();
    let mut source_shard = source16::SourceShardV16::from_native_rows(
        &policy,
        &source_plan,
        &global,
        &global.root(),
        (&producer, &producer.root()),
        (subject, &subject.row_root()),
        None,
    )
    .unwrap();
    source_shard.admit(&policy, &epoch, &source_plan, &source_admission).unwrap();
    let trace_grant = check_stage_trace_admission_v14(&class, &book, &plan, &policy, &epoch, &trace_plan, &anchor).unwrap();
    let mut trace_shard = trace17::TraceShardV17::from_native_rows(
        &policy,
        &trace_plan,
        source_shard.global_row(),
        &source_shard.global_row().root(),
        job.producer,
        Some((source_shard.producer_row(), &source_shard.producer_row().root())),
        (subject, &subject.row_root()),
        None,
    )
    .unwrap();
    trace_shard.admit(&policy, &epoch, &trace_plan, &trace_grant).unwrap();
    assert_eq!(trace_shard.trace_row().unwrap().original_paid_root, book.acceptance());
    assert!(trace_shard.admit(&policy, &epoch, &trace_plan, &trace_grant).is_err());
    let source_row = source_shard.source_row().unwrap().clone();
    assert_eq!(source_row.original_consent_root, book.acceptance());
    assert_eq!(source_row.parameter, pc.root());
    assert_ne!(source_row.parameter, actual.combined_parameters().root());
    assert_eq!(source_row.deadline_daa, book.deadline_daa());
    assert!(source_shard.admit(&policy, &epoch, &source_plan, &source_admission).is_err());
    let later = TrustedEpochV11::from_chain(&policy, [99; 64], 11).unwrap();
    assert!(check_stage_source_admission_v14(&class, &book, &plan, &policy, &discovery, &later, &source_plan, &anchor).is_err());
    let mut changed_policy = policy.clone();
    changed_policy.plan.0[0] -= 1;
    assert!(book.quota_subject(&plan, &changed_policy).is_err());
    assert!(plan.plan().record().action_envelopes[3].get(ResourceAxisV10::CourtWork) > 0);
    let inputs = actual.acquire(&template, &pc, &Inputs::new(), &mut full_budget()).unwrap();
    struct Combined<'a>(&'a MapParams, &'a AuthenticatedInputsV12);
    impl ParamSource for Combined<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            if j == 0 { self.0.param(j, l) } else { self.1.view_input(j, l) }
        }
    }
    let program = actual.program().v1_view();
    let trace = trace_v1(&program, &Combined(&source, &inputs), &[0; 3]).unwrap();
    let commitments = SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
            .collect(),
    );
    let post = (program.occurrences().len() - 1) as u16;
    let claim = StageTraceV14 {
        version: 14,
        class: class.root(),
        job_id: book.id(),
        job_input: job.root(),
        admission: actual.admission().root(),
        quota: plan.root(),
        segment_roots: commitments.segment_roots(),
        output_openings: (0..3).map(|p| commitments.node_opening(p, post, program.logits).unwrap()).collect(),
    };
    assert_eq!(StageTraceV14::from_bytes(&borsh::to_vec(&claim).unwrap(), 3).unwrap(), claim);
    let mut wrong = claim.clone();
    wrong.output_openings[2].commitment[0] ^= 1;
    assert!(book.clone().commit(&class, &plan, &execution, job.producer, 130, &wrong, 11).is_err());
    assert!(book.clone().commit(&class, &plan, &execution, job.requester, 130, &claim, 11).is_err());
    assert!(book.clone().commit(&class, &plan, &execution, job.producer, 129, &claim, 11).is_err());
    let mut claimed = book.clone();
    assert_eq!(claimed.commit(&class, &plan, &execution, job.producer, 130, &claim, 11).unwrap(), 130);
    assert!(check_stage_quota_admission_v14(&class, &claimed, &plan, &policy, &anchor).is_err());
    assert!(claimed.refund_unclaimed(book.deadline_daa() + 1).is_err());
    let caller_source =
        source26::derive_stage_source_caller_plan_v26(&class, &claimed, &plan, &policy, &execution, &discovery).unwrap();
    assert_eq!(caller_source.requests(), (source_plan.requests() - discovery.ordinary_lifetime_requests) / 4);
    assert_eq!(caller_source.attempts(), 2 * caller_source.requests());
    assert_eq!(
        caller_source.required_horizon(),
        caller_source.requests() as u64 * execution.horizons.da_answer + execution.horizons.proof_grace
    );
    for (held, unit) in caller_source.guarantee().0.iter().zip(&caller_source.request_cost().0) {
        assert_eq!(*held, *unit * caller_source.attempts() as u128);
    }
    assert!(
        caller_source.request_cost().get(misaka_palw_kernel::flat_resources10::ResourceAxisV10::StoredEvidenceBytes)
            >= misaka_palw_kernel::flat_quota11::source16::SOURCE_ANSWER_MAX_BYTES_V16 as u128
    );
    assert!(
        source26::derive_stage_source_caller_plan_v26(&class, &book, &plan, &policy, &execution, &discovery).is_err(),
        "an uncommitted book cannot manufacture source ownership"
    );
    let proof_epoch = TrustedEpochV11::from_chain(&policy, [42; 64], 12).unwrap();
    let mut proof_anchor = NativeStageProofAnchorV14 {
        job: book.id(),
        original_book: book.root(),
        current_book: claimed.root(),
        claim: claimed.claim_root().unwrap(),
        class: class.root(),
        source: class.source().id(),
        source_parameters: class.source().parameters().root(),
        inventory: class.source().inventory_root(),
        admission: actual.admission().root(),
        combined_parameters: actual.combined_parameters().root(),
        maximum_scope: class.maximum_resources().court_scope,
        policy: class.policy_root(),
        quota: plan.root(),
        consent18: book.acceptance(),
        proof_root: [43; 64],
        actor: job.requester,
        actor_operator: [11; 64],
        producer_operator: [12; 64],
        posted_daa: 12,
        matures_daa: 12 + execution.horizons.seal_maturity,
        horizon_daa: 12 + execution.horizons.seal_ttl + execution.horizons.proof_grace,
        original_paid_root: [0; 64],
        original_debit: 7,
        original_debit_receipt: [44; 64],
        shared_live: 1,
        actor_live: 1,
        job_live: 1,
    };
    proof_anchor.original_paid_root = stage_paid_seal_record_root_v14(&proof_anchor);
    proof_anchor.source_parameters = actual.combined_parameters().root();
    proof_anchor.original_paid_root = stage_paid_seal_record_root_v14(&proof_anchor);
    assert!(
        check_stage_paid_seal_v14(&class, &claimed, &plan, &policy, &execution, &proof_epoch, &proof_anchor).is_err(),
        "actual input PC cannot replace original source PC even with a self-consistent paid-record hash"
    );
    proof_anchor.source_parameters = class.source().parameters().root();
    proof_anchor.maximum_scope[0] ^= 1;
    proof_anchor.original_paid_root = stage_paid_seal_record_root_v14(&proof_anchor);
    assert!(check_stage_paid_seal_v14(&class, &claimed, &plan, &policy, &execution, &proof_epoch, &proof_anchor).is_err());
    proof_anchor.maximum_scope[0] ^= 1;
    proof_anchor.original_paid_root = stage_paid_seal_record_root_v14(&proof_anchor);
    let protected = check_stage_paid_seal_v14(&class, &claimed, &plan, &policy, &execution, &proof_epoch, &proof_anchor).unwrap();
    assert_ne!(protected.key().proof_root, proof_anchor.proof_root, "stage14 proof namespace remains explicit");
    let mut protected_state = state.clone();
    protected_state.admit_protected_seal(&policy, &proof_epoch, job.requester, &protected, plan.plan()).unwrap();
    assert!(protected_state.admit_protected_seal(&policy, &proof_epoch, job.requester, &protected, plan.plan()).is_err());
    proof_anchor.actor_operator = [12; 64];
    proof_anchor.original_paid_root = stage_paid_seal_record_root_v14(&proof_anchor);
    assert!(
        check_stage_paid_seal_v14(&class, &claimed, &plan, &policy, &execution, &proof_epoch, &proof_anchor).is_err(),
        "another bond on the original producer operator is not independent"
    );
    // A producer-bound PC default is tied to this exact original claim, source,
    // consent and actual collected liability; a global source flag is no grant.
    let mut judgment = NativeStageJudgmentV14 {
        job: book.id(),
        original_book: book.root(),
        book: claimed.root(),
        class: class.root(),
        claim: claimed.claim_root().unwrap(),
        source: class.source().id(),
        source_parameters: class.source().parameters().root(),
        inventory: class.source().inventory_root(),
        consent18: book.acceptance(),
        proof: [29; 64],
        finder: job.requester,
        finder_operator: [11; 64],
        established_daa: 12,
        current_daa: 12,
        kind: StageAdjudicationKindV14::BoundSourceUnavailable,
    };
    judgment.source_parameters[0] ^= 1;
    assert!(claimed.clone().adjudicate(&class, &plan, &execution, &judgment, 130).is_err());
    judgment.source_parameters[0] ^= 1;
    assert!(claimed.clone().adjudicate(&class, &plan, &execution, &judgment, 129).is_err());
    let verdict_epoch = TrustedEpochV11::from_chain(&policy, [28; 64], 12).unwrap();
    let mut verdict_state = state.clone();
    let old_guarantee = verdict_state.subject(&book.id()).unwrap().guarantee_remaining.clone();
    let stale_verdict =
        verdict_state.debit_stage_verdict_v14(&policy, &verdict_epoch, job.requester, &class, &claimed, &plan).unwrap();
    let paid_verdict = verdict_state.debit_stage_verdict_v14(&policy, &verdict_epoch, job.requester, &class, &claimed, &plan).unwrap();
    let outcome = claimed.adjudicate(&class, &plan, &execution, &judgment, 130).unwrap();
    assert_eq!(claimed.original_funded_root(), book.root());
    assert_eq!(outcome.finder_credit + outcome.burned, 130);
    let native_verdict = NativeStageTerminalAnchorV14 {
        job_id: book.id(),
        original_funded_book_root: book.root(),
        terminal_book_root: claimed.root(),
        acceptance18: book.acceptance(),
        nonce: NativeRequesterNonceWitnessV11 {
            requester: job.requester,
            requester_operator_id: [11; 64],
            next_nonce: 1,
            counter_root: native_requester_nonce_counter_root_v11(&[11; 64], 1),
        },
        established_daa: 12,
        principal_refund: 20,
        principal_queue: [30; 64],
    };
    assert!(check_stage_conviction_terminal_v14(&class, &claimed, &plan, &policy, &native_verdict, &outcome, 129).is_err());
    let convicted = check_stage_conviction_terminal_v14(&class, &claimed, &plan, &policy, &native_verdict, &outcome, 130).unwrap();
    assert!(verdict_state.mark_stage_verdict_closed_v14(&policy, &verdict_epoch, &convicted, &stale_verdict).is_err());
    verdict_state.mark_stage_verdict_closed_v14(&policy, &verdict_epoch, &convicted, &paid_verdict).unwrap();
    assert!(verdict_state.subject(&book.id()).unwrap().closed);
    assert_eq!(verdict_state.subject(&book.id()).unwrap().guarantee_remaining, old_guarantee);
    assert!(verdict_state.mark_stage_verdict_closed_v14(&policy, &verdict_epoch, &convicted, &paid_verdict).is_err());
    assert!(claimed.adjudicate(&class, &plan, &execution, &judgment, 130).is_err());
    let mut unclaimed = book;
    let original_root = unclaimed.root();
    let binding = unclaimed.quota_subject(&plan, &policy).unwrap();
    let late = TrustedEpochV11::from_chain(&policy, [77; 64], unclaimed.deadline_daa() + 1).unwrap();
    let mut close_state = state.clone();
    let remaining = close_state.subject(&unclaimed.id()).unwrap().guarantee_remaining.clone();
    let stale = close_state.debit_expired_terminal(&policy, &late, job.requester, &binding, plan.plan()).unwrap();
    assert!(close_state.mark_expired_stage_terminal_closed_v14(&policy, &late, &binding, &class, &plan, &unclaimed, &stale).is_err());
    assert_eq!(unclaimed.refund_unclaimed(unclaimed.deadline_daa() + 1).unwrap(), 20);
    assert_eq!(unclaimed.original_funded_root(), original_root);
    let paid = close_state.debit_expired_terminal(&policy, &late, job.requester, &binding, plan.plan()).unwrap();
    assert!(close_state.mark_expired_stage_terminal_closed_v14(&policy, &late, &binding, &class, &plan, &unclaimed, &stale).is_err());
    close_state.mark_expired_stage_terminal_closed_v14(&policy, &late, &binding, &class, &plan, &unclaimed, &paid).unwrap();
    assert!(close_state.subject(&unclaimed.id()).unwrap().closed);
    assert_eq!(close_state.subject(&unclaimed.id()).unwrap().guarantee_remaining, remaining);
    let terminal = check_stage_quota_terminal_v14(
        &class,
        &unclaimed,
        &plan,
        &policy,
        &NativeStageTerminalAnchorV14 {
            job_id: unclaimed.id(),
            original_funded_book_root: original_root,
            terminal_book_root: unclaimed.root(),
            acceptance18: unclaimed.acceptance(),
            nonce: NativeRequesterNonceWitnessV11 {
                requester: job.requester,
                requester_operator_id: [11; 64],
                next_nonce: 1,
                counter_root: native_requester_nonce_counter_root_v11(&[11; 64], 1),
            },
            established_daa: late.block_daa,
            principal_refund: 20,
            principal_queue: [8; 64],
        },
    )
    .unwrap();
    let g = close_state.global_row();
    let actor = close_state.actor_row(&job.requester).unwrap();
    let subject = close_state.subject(&unclaimed.id()).unwrap();
    let mut collection = CheckedQuotaShardV11::from_native_rows(
        &policy,
        &g,
        &g.root(),
        job.requester,
        unclaimed.id(),
        None,
        Some((&actor, &actor.root())),
        Some((subject, &subject.row_root())),
        None,
    )
    .unwrap();
    let next_interval = TrustedEpochV11::from_chain(&policy, [55; 64], 1001).unwrap();
    let retired = source16::retire_expired_source_v16(
        &mut collection,
        &policy,
        &next_interval,
        &source_plan,
        &source_row,
        &source_row.root(),
        &terminal,
    )
    .unwrap();
    assert!(retired.retired);
    assert_eq!(retired.guarantee_remaining, QuotaQuantitiesV11::default());
    assert_eq!(retired.consumed, retired.committed);
    let trace_original = trace_shard.trace_row().unwrap().clone();
    let retired_trace = trace17::retire_expired_trace_v17(
        &mut collection,
        &policy,
        &next_interval,
        &trace_plan,
        &trace_original,
        &trace_original.root(),
        &terminal,
    )
    .unwrap();
    assert!(retired_trace.retired);
    assert_eq!(retired_trace.remaining, QuotaQuantitiesV11::default());
    assert_eq!(retired_trace.consumed, retired_trace.committed);
    assert!(
        trace17::retire_expired_trace_v17(
            &mut collection,
            &policy,
            &next_interval,
            &trace_plan,
            &retired_trace,
            &retired_trace.root(),
            &terminal
        )
        .is_err()
    );
    collection.collect_closed_expired(&policy, &next_interval, &terminal).unwrap();
    assert!(collection.subject_row().is_none());
    assert_ne!(collection.actor_row().interval.consumed, source_row.committed);
    assert!(
        source16::retire_expired_source_v16(
            &mut collection,
            &policy,
            &next_interval,
            &source_plan,
            &retired,
            &retired.root(),
            &terminal
        )
        .is_err()
    );
    assert!(close_state.mark_expired_stage_terminal_closed_v14(&policy, &late, &binding, &class, &plan, &unclaimed, &paid).is_err());
    assert!(unclaimed.refund_unclaimed(unclaimed.deadline_daa() + 1).is_err());
}

#[test]
fn stage14_full_graph_replay_localizes_media_state_fault_without_decoding_tensor_output() {
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatHeaderV5, FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
        flat_court9::FlatContextV9,
        flat_detect5::{FlatPublicReadsV5, FlatReadV5},
        flat_detect9::{FlatFindingV9, FlatHeaderReadV9, FlatHeaderValueV9, FlatPublicReadsV9, FlatValueReadV9, FlatValueV9},
        flat_replay6::FlatReplayLimitsV6,
        flat_replay9::{FlatReplayLimitsV9, FlatReplayResultV9, FlatReplaySessionV9},
        job::DecodeRuleV1,
        merkle::AXIS_ROW,
        merkle3::{LeafOpeningV3, TreesV3},
        merkle5::FlatLeafHashV5,
        seg::{NodeOpeningV1, SegmentedCommitmentsV1, prompt_root_of_ids_v1},
        trace::{TraceV1, trace_v1},
    };
    struct Public<'a> {
        trace: &'a TraceV1,
        c: &'a SegmentedCommitmentsV1,
        calls: Cell<u32>,
        missing: bool,
    }
    impl FlatPublicReadsV5 for Public<'_> {
        fn node(&self, u: FlatReadV5) -> Option<NodeOpeningV1> {
            self.calls.set(self.calls.get() + 1);
            if self.missing {
                return None;
            }
            self.c.node_opening(u.position, u.occurrence, u.node)
        }
        fn hash(&self, _: FlatReadV5) -> Option<FlatLeafHashV5> {
            None
        }
    }
    impl FlatPublicReadsV9 for Public<'_> {
        fn header(&self, r: FlatHeaderReadV9) -> Option<Vec<u8>> {
            let u = r.node;
            let t = &self.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
            let trees = TreesV3::of(t);
            FlatHeaderValueV9 {
                version: 9,
                scope: r.scope,
                header: FlatHeaderV5::Legacy {
                    dtype: t.dtype.tag(),
                    shape: t.shape.iter().map(|d| *d as u64).collect(),
                    row_root: trees.row_root(),
                    col_root: trees.col_root(),
                },
            }
            .to_bytes()
            .ok()
        }
        fn value(&self, r: FlatValueReadV9) -> Option<Vec<u8>> {
            let u = r.node;
            let t = &self.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
            FlatValueV9 {
                version: 9,
                scope: r.scope,
                value: FlatOperandV5 {
                    node: self.c.node_opening(u.position, u.occurrence, u.node),
                    flat: vec![],
                    legacy: vec![LeafOpeningV3::of_trees(t, &TreesV3::of(t), AXIS_ROW, 0, 0)?],
                },
            }
            .to_bytes()
            .ok()
        }
    }
    let (template, source, pc) = fixture();
    let checked = derive_stage_job_v14(&template, &stage_job(&template), &pc).unwrap();
    let inputs = checked.acquire(&template, &pc, &Inputs::new(), &mut full_budget()).unwrap();
    let artifact = |j, layer| {
        if j == 0 { source.param(j, layer) } else { inputs.view_input(j, layer) }
    };
    struct Source<'a>(&'a dyn Fn(u16, Option<u16>) -> Option<Tensor>);
    impl ParamSource for Source<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            (self.0)(j, l)
        }
    }
    let program = checked.program().v1_view();
    let prompt = [0; 3];
    let honest = trace_v1(&program, &Source(&artifact), &prompt).unwrap();
    let e = checked.replay();
    let r = &e.reference;
    let limits = FlatReplayLimitsV9 {
        reference: FlatReplayLimitsV6 {
            position_tensor_bytes: r.position_tensor_bytes,
            operand_tensor_bytes: r.largest_operand_tensor_bytes,
            history_tensor_bytes: r.history_rows_tensor_bytes,
            parameter_hash_bytes: r.parameter_hash_bytes_per_attempt,
            trace_hash_bytes: r.trace_hash_bytes_per_attempt,
            node_reads: r.node_reads_per_attempt,
            hash_reads: r.hash_reads_per_attempt,
            reply_bytes: r.reply_bytes_per_attempt,
            attempts: 1,
        },
        topk_scan_work: e.topk_scan_work_per_attempt,
        topk_auxiliary_bytes: e.topk_auxiliary_bytes,
        selected_value_reads: e.selected_value_reads_per_attempt,
        header_reads: e.header_reads_per_attempt,
        scope_checker_work: e.scope_checker_work_per_attempt,
    };
    for mode in 0..4 {
        let mut trace = honest.clone();
        if mode == 1 {
            trace.values[2][0][4].data[0] += 1;
        }
        let commitments = SegmentedCommitmentsV1::new(
            trace
                .values
                .iter()
                .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                .collect(),
        );
        let roots = commitments.segment_roots();
        let mut context = FlatContextV9 {
            scope: checked.scope(),
            base: FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &program,
                    params: inputs.combined_pc(),
                    segment_roots: &roots,
                    positions: 3,
                    prompt_len: 3,
                    prompt_root: prompt_root_of_ids_v1(&prompt),
                    inline_prompt: Some(&prompt),
                    generated: &[0],
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            },
        };
        if mode == 3 {
            context.base.claim.positions += 1;
        }
        let public = Public { trace: &trace, c: &commitments, calls: Cell::new(0), missing: mode == 2 };
        let mut session = FlatReplaySessionV9::new(limits.clone());
        let finding = checked.check_replay(&context, &mut session, &public, &artifact);
        match mode {
            0 => assert_eq!(finding, FlatReplayResultV9::Finding(FlatFindingV9::Clean)),
            1 => {
                let FlatReplayResultV9::Finding(FlatFindingV9::Fault(f)) = finding else { panic!("{finding:?}") };
                checked.verify_fault(&context, &f).unwrap();
            }
            2 => assert!(matches!(finding, FlatReplayResultV9::Finding(FlatFindingV9::Demand(_)))),
            _ => {
                assert!(matches!(finding, FlatReplayResultV9::Inconsistent(_)));
                assert_eq!(public.calls.get(), 0);
                assert_eq!(session.attempts, 0);
            }
        }
    }
}

#[test]
fn typed_patch_audio_counts_roots_and_stateful_v2_view_match_independent_arithmetic() {
    let (template, source, pc) = fixture();
    let actual = FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 };
    let admission = template.specialize(actual, 3).unwrap();
    assert_eq!((admission.admission.raw_bytes, admission.admission.reference_bytes), (32, 128));
    let input = Inputs::new();
    let declared = declaration(&admission.admission);
    let got = acquire_authenticated_inputs_v12(&template, &admission.admission, [9; 64], &declared, &pc, &input, &mut full_budget())
        .unwrap();
    assert_eq!(input.calls.get(), 2);
    assert!(got.input(0, 3).is_none());
    assert!(got.view_input(1, Some(0)).is_none());
    let out = InterpreterV2::new(&admission.program).unwrap().run_positions(&source, &got, 3).unwrap();
    assert_eq!(out.iter().map(|s| s.output.data[0]).collect::<Vec<_>>(), vec![16, 32, 48]);
    struct Combined<'a>(&'a MapParams, &'a AuthenticatedInputsV12);
    impl ParamSource for Combined<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            if j == 0 { self.0.param(j, l) } else { self.1.view_input(j, l) }
        }
    }
    let view = admission.program.v1_view();
    let full = misaka_palw_kernel::trace::trace_v1(&view, &Combined(&source, &got), &[0; 3]).unwrap();
    for p in 0..3 {
        assert_eq!(full.values[p].last().unwrap()[0].data, vec![16 * (p as i128 + 1)]);
    }
    assert_eq!(got.commitments(), &declared);
    assert_eq!(got.combined_pc().by_instance.len(), 3);
    assert_eq!(got.combined_pc().by_instance[&(1, None)], declared.input_roots[0]);
}

#[test]
fn typed_input_wrong_job_source_scope_or_budget_is_refused_before_provider() {
    let (template, _, pc) = fixture();
    let a = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap().admission;
    let input = Inputs::new();
    let expected = declaration(&a);
    for kind in 0..7 {
        let mut budget = full_budget();
        let mut bad_a = a.clone();
        let mut bad_e = expected.clone();
        let mut bad_pc = pc.clone();
        let mut job = [9; 64];
        match kind {
            0 => job = [8; 64],
            1 => bad_a.scope = [7; 64],
            2 => bad_e.input_roots.pop().map(|_| ()).unwrap(),
            3 => {
                bad_pc.by_instance.insert((0, None), [8; 64]);
            }
            4 => budget.raw_bytes = 31,
            5 => budget.reference_bytes = 127,
            _ => budget.attempts = 0,
        }
        let before = (budget.raw_bytes, budget.reference_bytes, budget.attempts);
        assert!(acquire_authenticated_inputs_v12(&template, &bad_a, job, &bad_e, &bad_pc, &input, &mut budget).is_err());
        assert_eq!(input.calls.get(), 0);
        assert_eq!(before, (budget.raw_bytes, budget.reference_bytes, budget.attempts));
    }
}

#[test]
fn typed_input_bad_image_interval_or_commitment_retains_full_attempt() {
    let (template, _, pc) = fixture();
    let a = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap().admission;
    for kind in 0..5 {
        let mut input = Inputs::new();
        let mut e = declaration(&a);
        let mut budget = full_budget();
        match kind {
            0 => input.dtype = DType::I16,
            1 => input.shape[0] = vec![4],
            2 => {
                input.raw[0].push(0);
            }
            3 => input.raw[0][..4].copy_from_slice(&21i32.to_le_bytes()),
            _ => e.input_roots[0] = [8; 64],
        }
        assert!(acquire_authenticated_inputs_v12(&template, &a, [9; 64], &e, &pc, &input, &mut budget).is_err());
        assert_eq!(input.calls.get(), 1);
        assert_eq!((budget.raw_bytes, budget.reference_bytes, budget.attempts), (968, 872, 1));
    }
}

#[test]
fn typed_input_maxima_bind_actual_axes_and_cannot_resize_weights_or_hide_media() {
    let (template, _, _) = fixture();
    assert!(template.specialize(FlatJobLengthsV7 { token: 0, patch: 5, audio: 5 }, 1).is_err());
    assert!(template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 0 }, 1).is_err());
    assert!(template.specialize(FlatJobLengthsV7 { token: 1, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.lengths.bindings.remove(0);
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.lengths.bindings[0].slot = FlatShapeSlotV7::Parameter { parameter: 0, axis: 0 };
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
    let (mut bad, _, _) = fixture();
    bad.max_positions = 0;
    assert!(bad.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 1).is_err());
}

#[test]
fn typed_media_input_and_prior_state_openings_bind_complete_court() {
    use misaka_palw_kernel::{
        element::SegClaimContextV1,
        flat_court5::{FlatOperandV5, FlatSegContextV5, flat_profile_commitment_v5, flat_trace_profile_id_v5},
        flat_court9::{FlatContextV9, FlatFaultV9, verify_flat_fault_v9},
        flat_point9::{FlatPointFaultV9, PointContextV9, PointOperandKindV9, ordinary_point_court_id_v9, point_dependencies_v9},
        job::DecodeRuleV1,
        merkle::AXIS_ROW,
        merkle3::{LeafOpeningV3, TreesV3},
        seg::{SegmentedCommitmentsV1, prompt_root_of_ids_v1},
        trace::{SourceV1, WiringV1},
        verify::DismissalV1,
    };
    let (template, source, pc) = fixture();
    let job14 = stage_job(&template);
    let checked14 = derive_stage_job_v14(&template, &job14, &pc).unwrap();
    let specialized = template.specialize(FlatJobLengthsV7 { token: 0, patch: 3, audio: 5 }, 3).unwrap();
    let declared = declaration(&specialized.admission);
    let inputs = acquire_authenticated_inputs_v12(
        &template,
        &specialized.admission,
        [9; 64],
        &declared,
        &pc,
        &Inputs::new(),
        &mut full_budget(),
    )
    .unwrap();
    struct Combined<'a>(&'a MapParams, &'a AuthenticatedInputsV12);
    impl ParamSource for Combined<'_> {
        fn param(&self, j: u16, l: Option<u16>) -> Option<Tensor> {
            if j == 0 { self.0.param(j, l) } else { self.1.view_input(j, l) }
        }
    }
    let params = Combined(&source, &inputs);
    let program = specialized.program.v1_view();
    let prompt = [0; 3];
    let honest = misaka_palw_kernel::trace::trace_v1(&program, &params, &prompt).unwrap();
    let wiring = WiringV1::new(&program).unwrap();
    // Patch/audio reductions, the explicit previous-state read and its write
    // exercise the input PC and state boundary within the same scope.
    for node in [0u16, 1, 4, (program.blocks[0].nodes.len() - 1) as u16] {
        for lie in [false, true] {
            let mut trace = honest.clone();
            if lie {
                trace.values[2][0][node as usize].data[0] += 1;
            }
            let commitments = SegmentedCommitmentsV1::new(
                trace
                    .values
                    .iter()
                    .map(|p| p.iter().map(|s| s.iter().map(|t| flat_profile_commitment_v5(t).unwrap()).collect()).collect())
                    .collect(),
            );
            let roots = commitments.segment_roots();
            let base = || FlatSegContextV5 {
                profile: flat_trace_profile_id_v5(),
                claim: SegClaimContextV1 {
                    program: &program,
                    params: inputs.combined_pc(),
                    segment_roots: &roots,
                    positions: 3,
                    prompt_len: 3,
                    prompt_root: prompt_root_of_ids_v1(&prompt),
                    inline_prompt: Some(&prompt),
                    generated: &[0],
                    decode: DecodeRuleV1::Greedy,
                    encoder: None,
                },
            };
            let point = PointContextV9 { court_id: ordinary_point_court_id_v9(), base: base() };
            let context = FlatContextV9 { scope: &specialized.scope, base: base() };
            let relation = wiring.node(0, node);
            let sources = (0..relation.inputs.len()).map(|i| wiring.input_source(&prompt, 2, 0, node, i).unwrap()).collect::<Vec<_>>();
            let own = sources
                .iter()
                .map(|s| match s {
                    SourceV1::Param { index, layer } => params.param(*index, *layer).unwrap(),
                    SourceV1::Node { position, occurrence, node } => {
                        honest.values[*position as usize][*occurrence as usize][*node as usize].clone()
                    }
                    _ => panic!("unexpected media/state source"),
                })
                .collect::<Vec<_>>();
            let dependencies = point_dependencies_v9(&point, 2, 0, node, 0, &own).unwrap();
            let operands = own
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    assert_eq!(dependencies.kinds[i], PointOperandKindV9::Row);
                    FlatOperandV5 {
                        node: match sources[i] {
                            SourceV1::Node { position, occurrence, node } => commitments.node_opening(position, occurrence, node),
                            _ => None,
                        },
                        flat: vec![],
                        legacy: dependencies.leaves[i]
                            .iter()
                            .map(|(row, tile)| LeafOpeningV3::of_trees(t, &TreesV3::of(t), AXIS_ROW, *row, *tile).unwrap())
                            .collect(),
                    }
                })
                .collect();
            let output = &trace.values[2][0][node as usize];
            let proof = FlatFaultV9::Ordinary(FlatPointFaultV9 {
                version: 9,
                element: 0,
                output: FlatOperandV5 {
                    node: commitments.node_opening(2, 0, node),
                    flat: vec![],
                    legacy: vec![LeafOpeningV3::of_trees(output, &TreesV3::of(output), AXIS_ROW, 0, 0).unwrap()],
                },
                inputs: operands,
                token: None,
            });
            let proof = FlatFaultV9::from_bytes(&proof.to_bytes().unwrap()).unwrap();
            assert_eq!(verify_flat_fault_v9(&context, &proof).map(|_| ()), if lie { Ok(()) } else { Err(DismissalV1::NoFault) });
            assert_eq!(checked14.verify_fault(&context, &proof).map(|_| ()), if lie { Ok(()) } else { Err(DismissalV1::NoFault) });
            let mut enlarged = FlatContextV9 { scope: context.scope, base: base() };
            enlarged.base.claim.positions += 1;
            assert!(matches!(checked14.verify_fault(&enlarged, &proof), Err(DismissalV1::NotAuthentic(_))));
            // Another tensor of the same declared shape cannot substitute the
            // exact job input or its authenticated predecessor state value.
            let mut borrowed = proof;
            let FlatFaultV9::Ordinary(ref mut f) = borrowed else { panic!() };
            f.inputs[0].legacy[0].values[0] ^= 1;
            assert!(matches!(verify_flat_fault_v9(&context, &borrowed), Err(DismissalV1::NotAuthentic(_))));
        }
    }
}
