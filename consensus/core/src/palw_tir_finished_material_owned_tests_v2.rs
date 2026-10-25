// Included in the real native public-fold test module: actual registered MLDSA
// keys sign Capture/Ready/Open/Demand/Begin/Parts/Finish. Original FP funding is
// the existing accepted-original fixture, not a mined public-chain campaign.
fn finished_owned_world_v2(
    ids: bool,
) -> (
    SourceThreeFixture,
    PalwChainStateV2,
    crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1,
    crate::palw_tir_public_owned_v1::PalwTirPublicAcceptedBodyRefV1,
) {
    use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
    use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q, PalwTirPublicResponseV1 as R};
    let f = source_three_fixture();
    let frame = &f.fx.frame;
    let network = f.extras.tir_public_network_domain_v1.unwrap();
    let mut demand = continuation_source_cases_v2(&f).pop().unwrap();
    let request = if ids { Q::Ids { offset: 0 } } else { Q::StepLeaf { index: 0 } };
    demand.control.action = A::Acquisition(Owned::Demand { request });
    demand.control.signature = source_three_sign(
        &f.owner_key,
        demand.control.signing_message_v1(network).unwrap(),
        crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1,
    );
    let mut b = TransitionBuilder::try_new(&demand.state, &f.params, false, false, false, false, &f.extras).unwrap();
    apply_control_v1(&mut b, &at(frame, demand.offset), network, &demand.control).unwrap();
    let mut state = b.state.clone();
    drop(b);
    let target = demand.control.target;
    let T::Acquisition { owner, reserved_daa } = target else { unreachable!() };
    let key = owner_key(frame, owner, reserved_daa).unwrap();
    let (demand_id, _, turn, _) = owner_pending(frame, &state, target);
    let source = frame.source_v1(&f.fx.record, &f.fx.layout, f.fx.rules.maximum_leaves).unwrap();
    let body = if ids {
        let rows =
            crate::palw_step_refute::tiled_logits_rows_root_v1(&f.fx.execution.binding.job_context, &f.fx.execution.rows).unwrap();
        crate::palw_tir_public_owned_v1::palw_tir_public_ids_body_v1(&source, rows, &f.fx.execution.generated).unwrap()
    } else {
        borsh::to_vec(&R::StepLeaf {
            preimage: f.fx.execution.preimages[0].clone(),
            opening: crate::palw_step_leg::step_opening_v1(&f.fx.execution.hashes, 0).unwrap(),
        })
        .unwrap()
    };
    let producer = frame.custody_v1().pledge_v1().producer;
    let mut actions = vec![Owned::Begin {
        demand_id,
        answerer: producer,
        total_bytes: body.len() as u64,
        body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),
    }];
    for (offset, chunk) in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
        actions.push(Owned::Part {
            demand_id,
            answerer: producer,
            offset: (offset * crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,
            bytes: chunk.to_vec(),
        });
    }
    actions.push(Owned::Finish { demand_id, answerer: producer });
    let mut offset = demand.offset + 1;
    for action in actions {
        let mut control = source_three_control(frame, producer, target, A::Acquisition(action), 0, &f.producer_key, network);
        control.expected_turn = turn;
        control.signature = source_three_sign(
            &f.producer_key,
            control.signing_message_v1(network).unwrap(),
            crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1,
        );
        let mut b = TransitionBuilder::try_new(&state, &f.params, false, false, false, false, &f.extras).unwrap();
        apply_control_v1(&mut b, &at(frame, offset), network, &control).expect("real signed response stage");
        state = b.state.clone();
        drop(b);
        offset += 1;
    }
    state.last_point = Some(at(frame, offset - 1));
    let reference = owner_pending(frame, &state, target).3.unwrap();
    (f, state, key, reference)
}
#[test]
fn finished_material_owned_signed_unit_and_ids_match_raw_and_current_scope() {
    use crate::palw_tir_public_owned_v1::PalwTirPublicFinishedV1 as L;
    use crate::palw_tir_public_state_v1::{PalwTirPublicFinishedOwnedV2 as O, palw_tir_public_material_context_v2};
    for ids in [false, true] {
        let (f, state, key, reference) = finished_owned_world_v2(ids);
        let context = palw_tir_public_material_context_v2(&state, key, f.fx.rules.maximum_leaves).unwrap();
        let owned = context.finished_material_owned_v2(&reference).unwrap().unwrap();
        let legacy =
            crate::palw_tir_public_state_v1::palw_tir_public_finished_material_v1(&state, &reference, f.fx.rules.maximum_leaves)
                .unwrap()
                .unwrap();
        match (owned, legacy) {
            (O::Unit(a), L::Unit(b)) => assert_eq!(*a, b),
            (O::Ids(a), L::Ids(b)) => assert_eq!(*a, b),
            _ => panic!("same authenticated material kind"),
        }
        let mut missing = reference.clone();
        missing.body_hash = h(0xF1);
        assert!(context.finished_material_owned_v2(&missing).unwrap().is_none());
        let mut pruned = state.clone();
        pruned.staged_courts.remove(&crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirRawChunk(reference.chunk_head));
        let c = palw_tir_public_material_context_v2(&pruned, key, f.fx.rules.maximum_leaves).unwrap();
        assert!(matches!(
            c.finished_material_owned_v2(&reference),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))
        ));
        let mut expired = state.clone();
        expired.last_point.as_mut().unwrap().daa_score = f.fx.frame.plan_v1().terms.material_until_daa;
        let c = palw_tir_public_material_context_v2(&expired, key, f.fx.rules.maximum_leaves).unwrap();
        assert!(matches!(
            c.finished_material_owned_v2(&reference),
            Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_))
        ));
    }
}
#[test]
fn finished_material_owned_actual_body_stage_quota_retry_and_output_release() {
    const CHILD: &str = "MISAKA_FINISHED_MATERIAL_BODY_QUOTA";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::finished_material_owned_actual_body_stage_quota_retry_and_output_release", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    use crate::palw_native_resource_v1::*;
    use crate::palw_tir_public_state_v1::{finished_material_owned_v2 as M, palw_tir_public_material_context_v2};
    let (f, state, key, reference) = finished_owned_world_v2(false);
    let baseline = palw_tir_public_material_context_v2(&state, key, f.fx.rules.maximum_leaves).unwrap();
    let result = baseline.finished_material_owned_v2(&reference).unwrap().unwrap();
    let (_, _, identityq, bodyq) = M::material_stages_v2();
    drop(result);
    drop(baseline);
    const LIMIT: u64 = 1 << 30;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let sourceq = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::native_construction_requests_v1(
        &f.fx.record,
        &f.fx.layout,
        f.fx.frame.header_v1(),
    )
    .unwrap();

    let context = palw_tir_public_material_context_v2(&state, key, f.fx.rules.maximum_leaves).unwrap();
    assert!(sourceq + identityq + bodyq < LIMIT);
    let block = reserve_native_requests_v1(LIMIT - sourceq - identityq - bodyq + 1).unwrap();
    M::reset_material_stages_v2();
    assert!(matches!(context.finished_material_owned_v2(&reference), Err(NativeDecodeErrorV1::Resource(_))));
    let (attempts, admitted, _, _) = M::material_stages_v2();
    assert_eq!((attempts, admitted), (1, 0));
    drop(block);
    let exact = reserve_native_requests_v1(LIMIT - sourceq - identityq - bodyq).unwrap();
    M::reset_material_stages_v2();
    assert!(matches!(context.finished_material_owned_v2(&reference), Err(NativeDecodeErrorV1::Resource(_))));
    let (attempts, admitted, _, _) = M::material_stages_v2();
    assert_eq!((attempts, admitted), (1, 1), "exact body Q admits its real Vec, then the separately quoted decoder is deferred");
    drop(exact);
    M::reset_material_stages_v2();
    let output = context.finished_material_owned_v2(&reference).unwrap().unwrap();
    assert_eq!(M::material_stages_v2().1, 1);
    drop(context);
    assert!(reserve_native_requests_v1(LIMIT).is_err(), "returned authenticated output retains its own ticket");
    drop(output);
    assert!(reserve_native_requests_v1(LIMIT).is_ok());
}
