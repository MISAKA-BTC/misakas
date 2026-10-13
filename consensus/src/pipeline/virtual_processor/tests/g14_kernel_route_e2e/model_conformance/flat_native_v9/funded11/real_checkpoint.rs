//! The actual full-weight Qwen0.5 directory is off-chain test input. Admission,
//! nonce, original coins and producer statements are independently verified by
//! the ordinary node. This scoped fixed job grants no full-context/A6 credit.
use super::*;
use misaka_palw_sdk::real_checkpoint_fixture11::RealCheckpointExecutionFixtureV11;

fn fixed_checkpoint() -> RealCheckpointExecutionFixtureV11 {
    let path = std::env::var("RFC02_NATIVE_CHECKPOINT_FIXTURE")
        .expect("set RFC02_NATIVE_CHECKPOINT_FIXTURE to the pinned actual full-weight execution fixture");
    let raw = std::fs::read(path).unwrap();
    assert_eq!(raw.len(), 11_046_405);
    let f: RealCheckpointExecutionFixtureV11 = borsh::from_slice(&raw).unwrap();
    assert_eq!(f.version, 11);
    assert_eq!(faster_hex::hex_string(&f.artifact_sha256), "9af80e4e459cc2334519295fb100c58291382e3e4b4a5ee3e7098e7d1d8a17b6");
    assert_eq!(f.generated, [19, 17, 151645]);
    assert_eq!(f.prompt.len(), 24);
    assert_eq!(f.commitments.len(), 26);
    assert_eq!(f.params.by_instance.len(), 1404);
    assert_eq!(f.program_bytes.len(), 14_007);
    assert_eq!(f.commitments.iter().map(|p| p.iter().map(Vec::len).sum::<usize>()).sum::<usize>(), 170_820);
    let program = TirProgramV1::decode_canonical(&f.program_bytes).unwrap();
    assert_eq!(program.occurrences().iter().map(|(b, _)| program.blocks[*b as usize].nodes.len()).sum::<usize>(), 6570);
    f
}
async fn bounded(net: &mut Net, actor: usize, object: &K) {
    let signed = net.route(actor, object);
    let parts = kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&signed, 32_000).unwrap();
    let objects = parts.unwrap_or_else(|| vec![signed]);
    net.send(objects.into_iter().map(|o| (actor, o)).collect()).await;
}
#[tokio::test]
#[ignore = "requires pinned actual full-weight fixture; run with RFC02_NATIVE_CHECKPOINT_FIXTURE"]
async fn execution11_actual_qwen05_original64_scope26_fund_commit_and_independent_ibd() {
    kaspa_core::log::try_init_logger("warn");
    let f = fixed_checkpoint();
    let descriptor = k2_tir_v4_descriptor();
    let mut net = Net::over_cfg(native_config(), TestConsensus::new);
    net.beat_to(1).await;
    let bind = model_artifact_binding_id_v2(
        descriptor.digest(),
        program_root_v1(&f.program_bytes),
        f.model_inventory_root,
        Hash64::from_bytes(f.params.root()),
    );
    bounded(
        &mut net,
        1,
        &K::BindModelArtifactV2 {
            descriptor: descriptor.digest(),
            program_bytes: f.program_bytes.clone(),
            model_inventory_root: f.model_inventory_root.as_bytes(),
            param_commitments: f.params.clone(),
        },
    )
    .await;
    let source = net.api().unwrap().model_artifact_binding_header_v2(&bind).expect("actual original PC and R bound");
    assert_eq!(source.pc_instances, 1404);
    assert!(!source.refuted);
    net.beat_to(source.matures_daa).await;
    let candidate = misaka_palw_sdk::flat_candidate_v9::prepare_flat_candidate_v9(
        bind.as_bytes(),
        descriptor.digest(),
        &f.program_bytes,
        &f.params,
        64,
    )
    .unwrap();
    bounded(&mut net, 0, &candidate.object).await;
    let record = net.ledger().flat_conformance_classes_v9[&candidate.class].1.clone();
    assert_eq!(record.program_bytes, f.program_bytes);
    assert_eq!(record.param_commitments, f.params);
    assert_eq!(record.plan.max_positions, 64);
    action(&mut net, 0, EXECUTION_ACTION_REGISTER_V11, borsh::to_vec(&candidate.class).unwrap()).await;
    let class = net
        .api()
        .unwrap()
        .aux
        .iter()
        .filter(|((table, _), _)| *table == EXECUTION_CLASS_TABLE_V11)
        .find_map(|(_, bytes)| native_execution_class_header_v11(bytes).ok().map(|(h, _)| h.record.id()))
        .expect("actual native class");
    let mut request = FlatExecutionFundV11 {
        version: 11,
        execution_class: class,
        preparation: [0; 64],
        producer: net.kid(0),
        nonce: nonce_bytes_v11(0),
        reward: 20,
        prompt: f.prompt.clone(),
        generated_count: 3,
    };
    action(&mut net, 2, EXECUTION_ACTION_PREPARE_V11, borsh::to_vec(&request).unwrap()).await;
    request.preparation = net
        .api()
        .unwrap()
        .aux
        .iter()
        .filter(|((table, _), _)| *table == EXECUTION_PREPARATION_TABLE_V11)
        .find_map(|(_, raw)| borsh::from_slice::<NativeExecutionPreparationV11>(raw).ok().map(|p| p.id()))
        .expect("actual exact job preparation");
    // Unconsented requester packets cannot reserve another producer's future
    // answer resources or reward income. Original authorization precedes Fund.
    action(&mut net, 2, EXECUTION_ACTION_FUND_V11, borsh::to_vec(&request).unwrap()).await;
    assert!(net.api().unwrap().aux_row::<Vec<Digest>>(EXECUTION_JOB_OWNER_TABLE_V11, &borsh::to_vec(&net.bond(2)).unwrap()).is_none());
    action(&mut net, 0, EXECUTION_ACTION_ACCEPT_V18, borsh::to_vec(&request.preparation).unwrap()).await;
    action(&mut net, 2, EXECUTION_ACTION_FUND_V11, borsh::to_vec(&request).unwrap()).await;
    let ids: Vec<Digest> = net.api().unwrap().aux_row(EXECUTION_JOB_OWNER_TABLE_V11, &borsh::to_vec(&net.bond(2)).unwrap()).unwrap();
    assert_eq!(ids.len(), 1);
    let id = ids[0];
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    let checked = net.api().unwrap().execution_class_header_v11(&class).unwrap().checked_fixed_header().unwrap();
    let book = row.restore_book(&checked).unwrap();
    assert_eq!(book.job().input.positions, 26);
    assert_eq!(book.job().escrow_original_debit, 20);
    let directory = SegmentedCommitmentsV1::new(f.commitments.clone());
    let evidence = build_segmented_evidence_v1(
        misaka_palw_kernel::evidence::EvidenceHeaderV1 {
            network_domain: net.ledger().policy.network_domain,
            ruleset_digest: net.ledger().policy.ruleset_digest,
            class_binding_id: candidate.class,
            program_root: record.plan.program_root,
            artifact_root: f.params.root(),
            plan_root: record.plan.root(),
        },
        &descriptor,
        24,
        &prompt_root_of_ids_v1(&f.prompt),
        &[],
        &directory,
    );
    let mut body = FlatModelVectorPostV9 {
        version: 9,
        profile: record.plan.profile,
        court_set: record.plan.court_set,
        scope_root: record.plan.scope_root,
        replay_root: record.plan.replay_root,
        resource_root: record.plan.resource_root,
        trace: kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2 {
            version: 2,
            prompt: f.prompt,
            max_new_tokens: 3,
            generated: f.generated,
            evidence,
            segment_roots: directory.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    body.trace.implementation_roots = [body.trace_root(candidate.class, bind.as_bytes()); 3];
    let post = FlatExecutionPostV11 {
        version: 11,
        execution_class: class,
        job_id: id,
        requester_nonce: book.job().nonce,
        job_input_root: book.job().input.root(),
        actual_scope9_root: book.job().actual_scope9_root,
        actual_resource10_root: book.job().actual_resource10_root,
        quota_plan_root: book.job().quota_plan_root,
        body,
    };
    action(&mut net, 0, EXECUTION_ACTION_COMMIT_V11, borsh::to_vec(&post).unwrap()).await;
    let committed = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    assert_eq!(committed.header.terminal, ExecutionTerminalV11::Challengeable);
    assert_eq!(committed.header.producer_reserved, checked.record().required_producer_reserve);
    let view = net.api().unwrap().execution_view_v11(&id).unwrap();
    assert_eq!(view.trace.generated, [19, 17, 151645]);
    assert_eq!(view.trace.segment_roots, directory.segment_roots());
    let replay = net.replay().await;
    net.assert_same(&replay, "actual full-weight fixed Qwen job, original PC/R/source, escrow/nonce and honest wholegraph commitment");
}
