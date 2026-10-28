//! Source regression only; executed with the consolidated validation run.
#![allow(warnings)]
use misaka_palw_tir::{
    DType, MapParams, Ref, Tensor,
    builder::ProgramBuilder,
    program::HISTORY_BOUND_V1_SMALL,
    program_v2::{InputSource, OutputDecl, TirProgramV2},
};
fn recurrent() -> TirProgramV2 {
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let input = b.param("step", DType::I32, &[1], false);
    let state = b.fixed_state("global", DType::I32, &[1], -100, 100, false);
    let pre = {
        let mut q = b.block("pre", vec![]);
        let x = q.cast(input, DType::I32);
        q.finish(&[x])
    };
    let ty = b.blocks[pre as usize].nodes[0].out.clone();
    // Read global state inside a layer occurrence. Frozen layer-local lookup
    // would incorrectly substitute zero on every step.
    // The layer adds in I64 and narrows back: a layer block carries out exactly its carry-in signature (I32).
    let layer = {
        let mut q = b.block("layer", vec![ty.clone()]);
        let a = q.cast(Ref::CarryIn(0), DType::I64);
        let z = q.cast(Ref::State(state), DType::I64);
        let x = q.add(a, z, DType::I64);
        let y = q.cast(x, DType::I32);
        q.finish(&[y])
    };
    let carry = b.blocks[layer as usize].nodes.last().unwrap().out.clone();
    let (post, out) = {
        let mut q = b.block("post", vec![carry]);
        let x = q.state_write(state, Ref::CarryIn(0));
        q.commit(x);
        let Ref::Node(out) = x else { unreachable!() };
        (q.finish(&[]), out)
    };
    TirProgramV2::from_v1_lifting_params(
        &b.finish(pre, vec![layer], post, out),
        &[(0, InputSource::External { lo: -10, hi: 10 })],
        OutputDecl::Rows { node: out },
    )
    .unwrap()
}
#[test]
fn original_v2_stream_binds_each_position_and_global_post_writer() {
    let p = recurrent();
    let mut got = vec![];
    misaka_palw_kernel::trace::trace_streaming_stage_v32(
        &p,
        &MapParams::default(),
        &[0, 0, 0],
        &|k, q| {
            assert_eq!(k, 0);
            Some(Tensor::new(DType::I32, vec![1], vec![q as i128 + 1]).unwrap())
        },
        &mut |_, values| {
            got.push(values.last().unwrap().last().unwrap().data[0]);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(got, [1, 3, 6]);
    assert!(
        misaka_palw_kernel::trace::trace_streaming_stage_v32(
            &p,
            &MapParams::default(),
            &[0, 0, 0],
            &|_, q| if q == 1 { None } else { Some(Tensor::new(DType::I32, vec![1], vec![1]).unwrap()) },
            &mut |_, _| Ok(())
        )
        .is_err()
    );
}

#[test]
fn genuine_late_final_round_trip_preserves_original_closure_without_extending_conviction() {
    use misaka_palw_kernel::{
        flat_inputs12::{
            bounded14::FlatStageJobV14,
            bounded28::{FlatStageClassWireV28, stage_template_root_v28},
            source28::FlatStageSourceWireV28,
        },
        flat_lengths7::{FlatJobLengthsV7, FlatLengthTemplateV7},
        flat_quota11::{
            NativeRequesterNonceWitnessV11, QuotaPolicyV11, QuotaQuantitiesV11, quota_ruleset_id_v11,
            stage14::{StageTraceV14, bounded28::*},
        },
        trace::ParamCommitmentsV1,
    };
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let input = b.param("x", DType::I32, &[1], false);
    let pre = {
        let mut q = b.block("pre", vec![]);
        let x = q.cast(input, DType::I32);
        q.finish(&[x])
    };
    let ty = b.blocks[pre as usize].nodes[0].out.clone();
    let (post, node) = {
        let mut q = b.block("post", vec![ty]);
        let x = q.cast(Ref::CarryIn(0), DType::I32);
        q.commit(x);
        let Ref::Node(node) = x else { unreachable!() };
        (q.finish(&[]), node)
    };
    let p = TirProgramV2::from_v1_lifting_params(
        &b.finish(pre, vec![], post, node),
        &[(0, InputSource::External { lo: -1, hi: 1 })],
        OutputDecl::Rows { node },
    )
    .unwrap();
    let source = FlatStageSourceWireV28 {
        version: 28,
        program_bytes: p.encode(),
        inventory_root: [1; 64],
        source_parameters: ParamCommitmentsV1::default(),
    }
    .check()
    .unwrap();
    let lengths = FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7::default(),
        max_program_bytes: p.v1_view().encode(),
        bindings: vec![],
    };
    let class = FlatStageClassWireV28 {
        version: 28,
        source: source.id(),
        lengths: borsh::to_vec(&lengths).unwrap(),
        max_positions: 1,
        policy: [2; 64],
        component_manifest: [3; 64],
        complete_source: [4; 64],
        stage: 0,
    }
    .check(&source)
    .unwrap();
    let one = Tensor::new(DType::I32, vec![1], vec![1]).unwrap();
    let job = FlatStageJobV14 {
        version: 14,
        template: stage_template_root_v28(class.template(), &source).unwrap(),
        requester: [5; 64],
        producer: [6; 64],
        nonce: [0; 64],
        actual: FlatJobLengthsV7::default(),
        positions: 1,
        input_roots: vec![misaka_palw_kernel::merkle3::tensor_commitment_v3(&one)],
    };
    let actual = class.check_job(&job).unwrap();
    let quota = derive_stage_quota_v28(&class, &actual).unwrap();
    let caps = QuotaQuantitiesV11([1u128 << 60; 21]);
    let policy = QuotaPolicyV11 {
        version: 11,
        enabled: true,
        network_domain: [7; 64],
        ruleset_digest: quota_ruleset_id_v11(),
        interval_daa: 1000,
        plan: caps.clone(),
        block: caps.clone(),
        actor: caps.clone(),
        network: caps,
        max_subjects: 8,
        max_actors: 8,
        future_attempts: [2, 32, 64, 2, 2, 1],
    };
    let request_op = [8; 64];
    let producer_op = [9; 64];
    let mut book = StageJobBookV28::fund(
        &class,
        &actual,
        &quota,
        &policy,
        &NativeStageFundingV28 {
            class: class.root(),
            source: source.id(),
            policy: class.wire().policy,
            consent: [10; 64],
            contract: [11; 64],
            temporal: [12; 64],
            source_plan: [13; 64],
            caller_source: [14; 64],
            paged_plan: [15; 64],
            final_policy: [16; 64],
            final_basis: [17; 64],
            physical: [18; 64],
            requester_operator: request_op,
            producer_operator: producer_op,
            reward: 10,
            actual_debit: 10,
            debit_receipt: [19; 64],
            reserve: 100,
            finder: 40,
            nonce: NativeRequesterNonceWitnessV11 {
                requester: job.requester,
                requester_operator_id: request_op,
                counter_root: misaka_palw_kernel::flat_execution11::native_requester_nonce_counter_root_v11(&request_op, 1),
                next_nonce: 1,
            },
            admitted: 10,
            commit_until: 20,
            challenge_window: 10,
            final_wait: 10,
            deadline: 100,
        },
    )
    .unwrap();
    let mut commits = vec![];
    misaka_palw_kernel::trace::trace_streaming_stage_v32(
        &p,
        &MapParams::default(),
        &[0],
        &|_, _| Some(one.clone()),
        &mut |_, values| {
            commits.push(
                values
                    .iter()
                    .map(|v| v.iter().map(|t| misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(t).unwrap()).collect())
                    .collect(),
            );
            Ok(())
        },
    )
    .unwrap();
    let directory = misaka_palw_kernel::seg::SegmentedCommitmentsV1::new(commits);
    let occ = p.occurrences().len() as u16 - 1;
    let trace = StageTraceV14 {
        version: 14,
        class: class.root(),
        job_id: book.id(),
        job_input: job.root(),
        admission: actual.admission().root(),
        quota: quota.root(),
        segment_roots: directory.segment_roots(),
        output_openings: vec![directory.node_opening(0, occ, node).unwrap()],
    };
    book.commit(&class, &actual, &quota, job.producer, 11, &trace, 100).unwrap();
    let mut late_fault = book.clone();
    assert!(
        late_fault
            .convict(&NativeStageConvictionV28 {
                job: book.id(),
                original: book.original_root(),
                claim: book.claim().unwrap(),
                producer_operator: producer_op,
                finder: [50; 64],
                finder_operator: [51; 64],
                proof: [52; 64],
                actual_collected: 100,
                established: 120
            })
            .is_err()
    );
    book.finalize(&NativeStageFinalV28 {
        job: book.id(),
        original: book.original_root(),
        claim: book.claim().unwrap(),
        consent: book.consent(),
        contract: book.contract(),
        final_policy: book.final_policy_root(),
        final_basis: book.final_basis_root(),
        physical: book.physical_root(),
        producer_operator: producer_op,
        established: 120,
        pending_units: 0,
        retained_owners: 0,
        source_original: book.source_plan_root(),
        original_source_closed: true,
        original_source_equivalent: true,
        original_trace_closed: true,
        actual_escrow: 10,
        actual_producer_credit: 10,
        producer_queue: [20; 64],
        original_native_conditions: [21; 64],
    })
    .unwrap();
    let stored = borsh::to_vec(&book).unwrap();
    let restored: StageJobBookV28 = borsh::from_slice(&stored).unwrap();
    restored.check(&class, &actual, &quota).unwrap();
    assert_eq!(restored.settlement().unwrap().producer_credit, 10);
    assert_eq!(restored.deadline(), 100);
    assert_eq!(restored.settlement().unwrap().established, 120);
}
