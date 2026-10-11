//! Artifact identity courts through signed 104/105 objects, real carriers, fold,
//! shared block budget and independent replay. Activation is test-armed; artifact
//! attestation and OPV eligibility are derived, with no hook for these roots.
use super::*;
use kaspa_consensus_core::palw_onboarding_v1::{
    ArtifactMismatchProofV1, ArtifactTileOpeningV3, PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1 as RESERVED,
};
use misaka_palw_kernel::merkle::AXIS_ROW;

async fn send_with_cap(net: &mut Net, items: Vec<(usize, Obj)>, cap: usize) {
    let mut carriers = Vec::new();
    for (card, object) in items {
        match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&object, cap).unwrap() {
            Some(chunks) => carriers.extend(chunks.into_iter().map(|chunk| (card, chunk))),
            None => carriers.push((card, object)),
        }
    }
    net.send(carriers).await;
}

fn tile_proof(truth: &OnbFixture, wrong: &OnbFixture) -> ArtifactMismatchProofV1 {
    use super::super::g14_registration_e2e::Tensors;
    use kaspa_consensus_core::palw_tir_artifact_v1::{palw_tir_leaf_index_v1, palw_tir_open_leaf_v1};
    let source = Tensors(truth.params.tensors.iter().map(|(k, t)| (*k, t.to_le_bytes())).collect());
    let (&(param, layer), t) = wrong.params.tensors.iter().next().unwrap();
    ArtifactMismatchProofV1::TileV3 {
        commitments: ParamCommitmentsV1::of_v3(&wrong.params),
        param,
        layer,
        kernel_tile: ArtifactTileOpeningV3::new(misaka_palw_kernel::merkle3::LeafOpeningV3::of(t, AXIS_ROW, 0, 0).unwrap()).unwrap(),
        v2_opening: palw_tir_open_leaf_v1(&truth.program, &source, palw_tir_leaf_index_v1(&truth.program, param, layer, 0).unwrap())
            .unwrap(),
    }
}

#[tokio::test]
async fn artifact_binding_tile_invalid_flood_spends_shared_budget_and_cannot_slash_honest_binder() {
    kaspa_core::log::try_init_logger("warn");
    let truth = onb_fixture(93);
    let honest_pc = ParamCommitmentsV1::of_v3(&truth.params);
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let (class, root) = registered_and_bound(&mut net, &truth, &honest_pc).await;
    let honest = tile_proof(&truth, &truth);
    let binder_before = net.collateral(1);
    let cards = [0usize, 2, 3, 4, 5, 6];
    let before: Vec<u64> = cards.iter().map(|c| net.slashed(*c)).collect();
    let work = borsh::to_vec(&honest).unwrap().len() as u64
        + truth.program.encode().len() as u64
        + match &honest {
            ArtifactMismatchProofV1::TileV3 { kernel_tile, .. } => kernel_tile.leaf().values.len() as u64 * 16,
            _ => unreachable!(),
        };
    let policy = net.ledger().policy;
    assert_eq!(policy.max_adjudications_per_block, 4);
    let fee = policy.dismissal_fee_v1(work);
    let items = cards.iter().map(|c| (*c, net.artifact_challenged(*c, class, root, honest.clone()))).collect();
    send_with_cap(&mut net, items, 32_000).await;
    let charged: u64 = cards.iter().zip(before).map(|(c, old)| net.slashed(*c) - old).sum();
    assert_eq!(charged, 4 * fee, "four judged filings pay; the over-budget filings run no court");
    let (_, runs, spent): (u64, u32, u64) =
        net.api().unwrap().aux_row(kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]).unwrap();
    assert_eq!((runs, spent), (4, 4 * work), "all proof variants share the same block budget");
    assert_eq!(net.collateral(1), binder_before, "no invalid accusation slashes the honest binder");
    let row = net.api().unwrap().artifact_binding_v1(&class, &root).unwrap();
    assert!(!row.refuted && row.reserved == RESERVED);
    // A fresh block can adjudicate again; junk does not reserve future capacity.
    let slash = net.slashed(7);
    let o = net.artifact_challenged(7, class, root, honest);
    send_with_cap(&mut net, vec![(7, o)], 32_000).await;
    assert_eq!(net.slashed(7) - slash, fee);
    let replay = net.replay().await;
    net.assert_same(&replay, "tile flood fees, reservation and shared budget replay");
}

#[tokio::test]
async fn artifact_binding_tile_false_bytes_convict_once_through_signed_carriers() {
    kaspa_core::log::try_init_logger("warn");
    let (truth, wrong) = (onb_fixture(94), onb_fixture(95));
    let pc = ParamCommitmentsV1::of_v3(&wrong.params);
    let proof = tile_proof(&truth, &wrong);
    assert_eq!(
        kaspa_consensus_core::palw_onboarding_v1::verify_artifact_mismatch_v1(
            &truth.program,
            truth.artifact_root,
            Hash64::from_bytes(pc.root()),
            &proof,
        ),
        Ok(())
    );
    let mut net = Net::over_cfg(kernel_config_onboarding(), TestConsensus::new);
    net.beat_to(1).await;
    let (class, root) = registered_and_bound(&mut net, &truth, &pc).await;
    let before = net.collateral(1);
    let o = net.artifact_challenged(7, class, root, proof.clone());
    send_with_cap(&mut net, vec![(7, o)], 6 << 10).await;
    assert!(net.api().unwrap().artifact_binding_v1(&class, &root).unwrap().refuted);
    assert_eq!(net.collateral(1), before - RESERVED);
    assert_eq!(net.owed(7), RESERVED * kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1 / 1000);
    let slash = net.slashed(1);
    let o = net.artifact_challenged(7, class, root, proof);
    send_with_cap(&mut net, vec![(7, o)], 6 << 10).await;
    assert_eq!(net.slashed(1), slash);
    assert!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()).is_empty());
    let replay = net.replay().await;
    net.assert_same(&replay, "v3 artifact identity conviction and duplicate replay");
}

/// Full actual parameter map and canonical program; only the authenticated bounded
/// witness enters this node. No artifact-attestation or eligibility hook is called.
/// The stateful release conformance gate remains closed after candidate preparation.
#[tokio::test]
async fn artifact_binding_tile_actual_weights_register_refute_and_prepare_without_identity_hooks() {
    use kaspa_consensus_core::palw_onboarding_v1::{ArtifactBindingStateV1, PalwOnboardingGateV1};
    use misaka_palw_kernel::descriptor::k2_tir_v4_descriptor;
    kaspa_core::log::try_init_logger("warn");
    let bundle: misaka_palw_sdk::tir_stream::ArtifactTileCourtBundleV3 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-artifact-tile-court.borsh"
    )))
    .unwrap();
    assert_eq!(bundle.version, 1);
    assert_eq!(bundle.params.by_instance.len(), 1636);
    assert_eq!(
        bundle.program_bytes.as_slice(),
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/design/palw/tir/evidence/qwen25-real-declared-program.tir"))
    );
    let prepared: ParamCommitmentsV1 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-params-v3-commitments.borsh"
    )))
    .unwrap();
    assert_eq!(bundle.params, prepared, "declaration changes no weight commitment");
    assert_eq!(
        bundle.inventory_root.to_string(),
        "5c02fc51ad06f17066c64dca3b79c4bbf986fdd399b7e583a6384ff8d00580d13aeefb2f42c3cbec3a1499f639b6463e792ac40394ba590bc902e1b65fbeed9a"
    );

    let program = TirProgramV1::decode_canonical(&bundle.program_bytes).unwrap();
    let descriptor = k2_tir_v4_descriptor();
    let plan = plan_for_tir_program_v1(&descriptor, &program, program_root_v1(&bundle.program_bytes), 32).unwrap();
    let class = bundle.class.clone();
    assert_eq!(class.program, bundle.program_bytes);
    assert_eq!(class.layout.max_context, 32);
    let mut f = OnbFixture {
        program,
        params: MapParams::default(),
        plan,
        pc: bundle.params.clone(),
        class,
        artifact_root: bundle.inventory_root,
    };
    let (config, bundle_params, premine, floats) = kernel_config_onboarding();
    let mut params = config.params.clone();
    let mut fence = PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new());
    fence.min_effective_bits = 128;
    fence.sampled_conformance_gates_reward = false;
    params.palw_panel_free_v1 = Some(fence);
    assert!(params.validate_palw_v2().is_err(), "only activation is test-armed; shipping validation stays closed");
    let mut net = Net::over_cfg((Config::new(params), bundle_params, premine, floats), TestConsensus::new);
    net.beat_to(1).await;
    let (v2, good_root) = registered_and_bound(&mut net, &f, &bundle.params).await;
    assert!(net.api().unwrap().onboarding_attested_roots_v1(net.daa()).is_empty());
    let binder_before = net.collateral(1);
    let outsider_before = net.slashed(7);
    let o = net.artifact_challenged(7, v2, good_root, bundle.honest.clone());
    send_with_cap(&mut net, vec![(7, o)], 32_000).await;
    assert_eq!(net.collateral(1), binder_before);
    assert!(net.slashed(7) > outsider_before, "a bounded but false accusation pays the dismissal tariff");
    assert!(!net.api().unwrap().artifact_binding_v1(&v2, &good_root).unwrap().refuted);
    // A different V2 statement falsely binds one changed parameter tensor. Its
    // reservation is slashed once by an outsider from just the two root openings.
    f.class.tokenizer_id = Hash64::from_bytes([0x77; 64]);
    let ArtifactMismatchProofV1::TileV3 { commitments: wrong_pc, .. } = &bundle.false_binding else { unreachable!() };
    let (bad_v2, bad_root) = registered_and_bound(&mut net, &f, wrong_pc).await;
    let before = net.collateral(1);
    let o = net.artifact_challenged(7, bad_v2, bad_root, bundle.false_binding.clone());
    send_with_cap(&mut net, vec![(7, o)], 32_000).await;
    assert!(net.api().unwrap().artifact_binding_v1(&bad_v2, &bad_root).unwrap().refuted);
    assert_eq!(net.collateral(1), before - RESERVED);
    let slashed = net.slashed(1);
    let o = net.artifact_challenged(7, bad_v2, bad_root, bundle.false_binding);
    send_with_cap(&mut net, vec![(7, o)], 32_000).await;
    assert_eq!(net.slashed(1), slashed, "the actual false binding slashes once");
    let live = net.api().unwrap().artifact_binding_v1(&v2, &good_root).unwrap();
    net.beat_to(live.matures_daa + 1).await;
    let route = net.api().unwrap();
    assert_eq!(route.artifact_binding_v1(&v2, &good_root).unwrap().state_at(net.daa()), ArtifactBindingStateV1::Matured);
    assert!(route.onboarding_attested_roots_v1(net.daa()).contains(&good_root));
    assert!(!route.onboarding_attested_roots_v1(net.daa()).contains(&bad_root));
    let candidate = Hash64::from_bytes(single_class_id_v1(
        descriptor.digest(),
        &bundle.program_bytes,
        &f.plan,
        &bundle.params,
        VerificationModeV1::OptimisticPublicVerification,
    ));
    let o = net.route(
        1,
        &K::RegisterConformanceClass {
            descriptor: descriptor.digest(),
            program_bytes: bundle.program_bytes,
            plan: f.plan,
            param_commitments: bundle.params,
        },
    );
    send_with_cap(&mut net, vec![(1, o)], 32_000).await;
    assert!(
        net.ledger().conformance_classes.contains_key(&candidate.as_bytes()),
        "the real class is prepared without an attestation hook"
    );
    assert!(net.ledger().classes.is_empty() && net.ledger().jobs.is_empty() && net.ledger().claims.is_empty());
    let o = net.kernel_bound(1, v2, candidate);
    net.send(vec![(1, o)]).await;
    assert!(net.api().unwrap().kernel_binding_v1(&v2).is_some(), "binding reads the candidate metadata");
    assert!(!matches!(net.api().unwrap().onboarding_gate_v1(&v2, &f.artifact_root, net.daa()), PalwOnboardingGateV1::Ready));
    assert!(kaspa_consensus_core::palw_opv_bootstrap_v1::palw_complete_check_domain_v1(&f.program, 32).is_err());
    let replay = net.replay().await;
    net.assert_same(&replay, "actual V2 registration, bounded identity court and candidate without identity hooks");
}
