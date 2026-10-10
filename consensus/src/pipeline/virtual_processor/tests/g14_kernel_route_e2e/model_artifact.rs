//! New signed descriptor-scoped statements over real lowered fixture weights. No V2 decoder
//! registration, artificial job parameters, artifact-attestation hook or eligibility hook.
use super::*;
use kaspa_consensus_core::palw_model_artifact_v2::{model_artifact_binding_id_v2, model_artifact_work_v2};
use kaspa_consensus_core::palw_onboarding_v1::{
    ArtifactBindingStateV1, ArtifactMismatchProofV1, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as RESERVED,
};
use misaka_palw_sdk::tir_stream::ModelArtifactCourtBundleV2;

fn material() -> ModelArtifactCourtBundleV2 {
    let b: ModelArtifactCourtBundleV2 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/rfc02-bert-model-artifact-court.borsh"
    )))
    .unwrap();
    assert_eq!(b.version, 2);
    assert_eq!(b.descriptor.digest(), misaka_palw_kernel::descriptor::k2_tir_v5_descriptor().digest());
    assert_eq!(b.params.by_instance.len(), 92);
    b
}
fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (cfg, b, p, f) = kernel_config_onboarding();
    let mut params = cfg.params.clone();
    params.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new()));
    assert!(params.validate_palw_v2().is_err(), "shipping activation remains refused");
    (Config::new(params), b, p, f)
}
fn bind(b: &ModelArtifactCourtBundleV2, pc: &ParamCommitmentsV1) -> (K, Hash64) {
    (
        K::BindModelArtifactV2 {
            descriptor: b.descriptor.digest(),
            program_bytes: b.program_bytes.clone(),
            model_inventory_root: b.model_inventory_root.as_bytes(),
            param_commitments: pc.clone(),
        },
        model_artifact_binding_id_v2(
            b.descriptor.digest(),
            program_root_v1(&b.program_bytes),
            b.model_inventory_root,
            Hash64::from_bytes(pc.root()),
        ),
    )
}
fn candidate(b: &ModelArtifactCourtBundleV2, id: Hash64) -> (K, Hash64) {
    let program = TirProgramV1::decode_canonical(&b.program_bytes).unwrap();
    let plan = plan_for_tir_program_v1(&b.descriptor, &program, program_root_v1(&b.program_bytes), 1).unwrap();
    let class = Hash64::from_bytes(single_class_id_v1(
        b.descriptor.digest(),
        &b.program_bytes,
        &plan,
        &b.params,
        VerificationModeV1::OptimisticPublicVerification,
    ));
    (
        K::RegisterModelConformanceClassV2 {
            binding: id.as_bytes(),
            descriptor: b.descriptor.digest(),
            program_bytes: b.program_bytes.clone(),
            plan,
            param_commitments: b.params.clone(),
        },
        class,
    )
}
fn pc(proof: &ArtifactMismatchProofV1) -> ParamCommitmentsV1 {
    match proof {
        ArtifactMismatchProofV1::Instances { commitments }
        | ArtifactMismatchProofV1::Row { commitments, .. }
        | ArtifactMismatchProofV1::TileV3 { commitments, .. } => commitments.clone(),
    }
}
async fn send(net: &mut Net, card: usize, object: &K) {
    let o = net.route(card, object);
    let carriers = match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&o, 32_000).unwrap() {
        Some(chunks) => chunks.into_iter().map(|chunk| (card, chunk)).collect(),
        None => vec![(card, o)],
    };
    net.send(carriers).await;
}

#[tokio::test]
async fn model_artifact_binding_matures_only_for_matching_encoder_candidate_without_global_attestation() {
    kaspa_core::log::try_init_logger("warn");
    let b = material();
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    let (o, id) = bind(&b, &b.params);
    send(&mut net, 1, &o).await;
    let api = net.api().unwrap();
    let row = api.model_artifact_binding_v2(&id).expect("signed generic model statement");
    assert_eq!(row.program_bytes, b.program_bytes);
    assert_eq!(row.pc_instances, 92);
    assert_eq!(api.model_artifact_reserved_at_v2(&net.bond(1), net.daa()), RESERVED as u128);
    let state = net.chain.tip_state().1;
    assert!(kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v1(&state, &net.bond(1), net.daa(), None));
    let (registration, class) = candidate(&b, id);
    send(&mut net, 7, &registration).await;
    assert!(net.api().unwrap().kernel_class_record_v1(&class).is_none(), "pending is not candidate permission");
    let honest = K::RefuteModelArtifactV2 { binding: id.as_bytes(), proof: borsh::to_vec(&b.honest).unwrap() };
    let before = net.collateral(1);
    let slash = net.slashed(7);
    send(&mut net, 7, &honest).await;
    assert_eq!(net.collateral(1), before);
    assert!(net.slashed(7) > slash, "honest no-fault filing pays its dismissal");
    assert!(!net.api().unwrap().model_artifact_binding_v2(&id).unwrap().refuted);
    net.beat_to(row.matures_daa).await;
    let mut wrong = registration.clone();
    if let K::RegisterModelConformanceClassV2 { descriptor, .. } = &mut wrong {
        *descriptor = misaka_palw_kernel::descriptor::k2_tir_v4_descriptor().digest();
    }
    send(&mut net, 7, &wrong).await;
    assert!(net.api().unwrap().kernel_class_record_v1(&class).is_none(), "other descriptor cannot borrow model-only scope");
    send(&mut net, 7, &registration).await;
    let api = net.api().unwrap();
    let ledger = api.ledger().unwrap();
    assert!(ledger.conformance_classes.contains_key(&class.as_bytes()), "ordinary v5 metadata admission stands");
    assert_eq!(api.model_artifact_candidate_binding_v2(&class), Some(id));
    assert!(!ledger.attested_artifacts.contains(&b.params.root()), "temporary scope never enters global attestation rows");
    assert!(!api.onboarding_attested_roots_v1(net.daa()).contains(&Hash64::from_bytes(b.params.root())));
    assert!(ledger.classes.is_empty() && ledger.jobs.is_empty() && ledger.claims.is_empty(), "no execution or reward rights");
    // Ordinary candidate registration still cannot borrow that PC root globally.
    let K::RegisterModelConformanceClassV2 { descriptor, program_bytes, plan, param_commitments, .. } = registration else {
        unreachable!()
    };
    let other = K::RegisterConformanceClass { descriptor, program_bytes, plan, param_commitments };
    send(&mut net, 0, &other).await;
    assert_eq!(net.ledger().conformance_classes.len(), 1);
    net.beat_to(row.final_daa).await;
    assert_eq!(net.api().unwrap().model_artifact_binding_v2(&id).unwrap().state_at(net.daa()), ArtifactBindingStateV1::Final);
    assert_eq!(
        net.api().unwrap().model_artifact_reserved_at_v2(&net.bond(1), net.daa()),
        0,
        "reservation ends by its stored horizon without a global tick scan"
    );
    let replay = net.replay().await;
    net.assert_same(&replay, "scoped encoder maturity, no-fault fee, candidate and liability replay");
}

#[tokio::test]
async fn model_artifact_binding_false_encoder_weight_convicts_once_and_never_matures() {
    kaspa_core::log::try_init_logger("warn");
    let b = material();
    let false_pc = pc(&b.false_binding);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    let (o, id) = bind(&b, &false_pc);
    send(&mut net, 1, &o).await;
    let horizon = net.api().unwrap().model_artifact_binding_v2(&id).unwrap().matures_daa;
    let before = net.collateral(1);
    let o = K::RefuteModelArtifactV2 { binding: id.as_bytes(), proof: borsh::to_vec(&b.false_binding).unwrap() };
    send(&mut net, 7, &o).await;
    assert_eq!(net.collateral(1), before - RESERVED);
    assert_eq!(net.owed(7), RESERVED * kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 / 1000);
    assert!(net.api().unwrap().model_artifact_binding_v2(&id).unwrap().refuted);
    let slashed = net.slashed(1);
    send(&mut net, 7, &o).await;
    assert_eq!(net.slashed(1), slashed);
    net.beat_to(horizon).await;
    assert_eq!(net.api().unwrap().model_artifact_binding_v2(&id).unwrap().state_at(net.daa()), ArtifactBindingStateV1::Refuted);
    assert!(!net.ledger().attested_artifacts.contains(&false_pc.root()));
    let replay = net.replay().await;
    net.assert_same(&replay, "bounded encoder model-weight conviction and duplicate replay");
}

#[tokio::test]
async fn model_artifact_binding_invalid_flood_spends_the_shared_budget_before_decode() {
    kaspa_core::log::try_init_logger("warn");
    let b = material();
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    let (o, id) = bind(&b, &b.params);
    send(&mut net, 1, &o).await;
    let junk = vec![2, 255, 255, 255, 255];
    let work = model_artifact_work_v2(b.program_bytes.len(), junk.len()).unwrap();
    let policy = net.ledger().policy;
    assert_eq!(policy.max_adjudications_per_block, 4);
    let paid_runs = 4.min(policy.max_court_work_per_block / work) as usize;
    assert!(paid_runs > 0);
    let cards = [0usize, 2, 3, 4, 5, 6];
    let before: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    let before_binder = net.collateral(1);
    let mut objects = Vec::new();
    for card in cards {
        let o = net.route(card, &K::RefuteModelArtifactV2 { binding: id.as_bytes(), proof: junk.clone() });
        objects.push((card, o));
    }
    net.send(objects).await;
    let charged: u64 = cards.iter().zip(before).map(|(c, old)| net.slashed(*c) - old).sum();
    assert_eq!(charged, paid_runs as u64 * policy.dismissal_fee_v1(work));
    let (_, runs, spent): (u64, u32, u64) =
        net.api().unwrap().aux_row(kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]).unwrap();
    assert_eq!((runs, spent), (paid_runs as u32, paid_runs as u64 * work));
    assert_eq!(net.collateral(1), before_binder);
    let replay = net.replay().await;
    net.assert_same(&replay, "model proof flood budget and dismissal replay");
}

#[tokio::test]
async fn model_artifact_binding_catalog_is_bounded_and_cannot_reuse_reserved_capital() {
    kaspa_core::log::try_init_logger("warn");
    let mut b = material();
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    let limit = kaspa_consensus_core::palw_model_artifact_v2::PALW_MODEL_ARTIFACT_BINDINGS_PER_BOND_V2;
    assert!(net.collateral(1) >= RESERVED * limit as u64);
    let mut refused_id = Hash64::default();
    for i in 0..=limit {
        // Distinct signed declarations, not conformance/availability assertions for these roots.
        b.model_inventory_root = Hash64::from_bytes([100 + i as u8; 64]);
        let (o, id) = bind(&b, &b.params);
        send(&mut net, 1, &o).await;
        if i < limit {
            assert!(net.api().unwrap().model_artifact_binding_v2(&id).is_some());
        } else {
            refused_id = id;
        }
    }
    let api = net.api().unwrap();
    assert!(api.model_artifact_binding_v2(&refused_id).is_none());
    assert_eq!(api.model_artifact_bindings_of_v2(&net.bond(1)).len(), limit);
    assert_eq!(api.model_artifact_reserved_at_v2(&net.bond(1), net.daa()), RESERVED as u128 * limit as u128);
    let state = net.chain.tip_state().1;
    assert!(
        kaspa_consensus_core::palw_state_v2::palw_bond_committed_v1(&state, &net.bond(1), net.daa(), None, 0)
            >= RESERVED as u128 * limit as u128
    );
    assert!(net.ledger().classes.is_empty() && net.ledger().conformance_classes.is_empty());
    let replay = net.replay().await;
    net.assert_same(&replay, "bounded model statement catalog and shared collateral replay");
}
