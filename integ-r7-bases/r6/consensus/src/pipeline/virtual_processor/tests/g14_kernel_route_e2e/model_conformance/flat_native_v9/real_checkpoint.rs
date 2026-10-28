//! Actual Qwen32 reproduction through the ordinary native9 conformance lane.
//! The independently executed local fixture contains public directories and
//! bounded R/PC and point openings. This test verifies their native identities,
//! public acquisition, own proof settlement and replay; it does not itself run
//! the original model, search an unknown node, promote execution11 or prove an
//! honest job Final. No artifact-attestation or eligibility test hook is used.
use super::*;
use kaspa_consensus_core::palw_onboarding_v1::{ArtifactMismatchProofV1, verify_artifact_mismatch_v2};
use misaka_palw_kernel::flat_detect5::{FlatPublicReadsV5, FlatReadV5};
use misaka_palw_kernel::flat_detect9::{FlatHeaderReadV9, FlatHeaderValueV9, FlatPublicReadsV9, FlatValueReadV9};
use misaka_palw_sdk::flat_checkpoint_fixture9::{FlatCheckpointFixtureV9, FlatFixtureReplyV9};

fn actual_fixture() -> FlatCheckpointFixtureV9 {
    let bytes =
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../docs/design/palw/tir/evidence/qwen25-real-generic9-fixture.borsh"));
    assert_eq!(bytes.len(), 1_360_317);
    let f: FlatCheckpointFixtureV9 = borsh::from_slice(bytes).unwrap();
    assert_eq!(f.version, 9);
    assert_eq!(f.descriptor, k2_tir_v4_descriptor());
    assert_eq!(f.program_bytes.len(), 14_011);
    assert_eq!(
        Hash64::from_bytes(program_root_v1(&f.program_bytes)),
        "a1360976cdff92e473dff36769bf468140e4fb5a03150ed3be67d7af3ca72d0a5e21f4d1bf70aa4e39678aa5d6d82bdb499556f3b01999be362adb9e7a560f3d"
            .parse::<Hash64>()
            .unwrap()
    );
    assert_eq!(
        Hash64::from_bytes(f.params.root()),
        "1c07cbbadb1ec7166361a6ffdda43c7c5740183dbb8ecd9659e7c4c5c906cf5495ac5df4fd25617829628b3cec7969e43eeee06bed08c0beeaa51d76b023f729"
            .parse::<Hash64>()
            .unwrap()
    );
    assert_eq!(f.params.by_instance.len(), 1_636);
    assert_eq!(f.tokens, vec![4498]);
    assert_eq!(f.honest_generated.len(), 1);
    assert_eq!(f.requested_public_replies.len(), 9);
    assert_eq!(f.ordinary_fault_bytes.len(), 17_816);
    let p = TirProgramV1::decode_canonical(&f.program_bytes).unwrap();
    assert_eq!(p.occurrences().len(), 30);
    assert_eq!(p.params.len(), 70);
    assert_eq!(p.occurrences().iter().map(|(b, _)| p.blocks[*b as usize].nodes.len()).sum::<usize>(), 7_658);
    assert_ne!(f.honest_commitments, f.wrong_commitments);
    f
}

/// Actual large metadata rides ordinary bounded carriers with the unchanged
/// per-block work budget. This avoids assuming one large carrier is admitted.
async fn send_actual(net: &mut Net, card: usize, object: &K) {
    let route = net.route(card, object);
    let carriers = match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&route, 32_000).unwrap() {
        Some(parts) => parts.into_iter().map(|p| (card, p)).collect(),
        None => vec![(card, route)],
    };
    net.send(carriers).await;
}

async fn register_actual(
    net: &mut Net,
    f: &FlatCheckpointFixtureV9,
) -> (Hash64, Digest, misaka_palw_kernel::ledger::flat_candidate9::FlatCandidateRecordV9) {
    net.beat_to(1).await;
    let bind = model_artifact_binding_id_v2(
        f.descriptor.digest(),
        program_root_v1(&f.program_bytes),
        f.model_inventory_root,
        Hash64::from_bytes(f.params.root()),
    );
    send_actual(
        net,
        1,
        &K::BindModelArtifactV2 {
            descriptor: f.descriptor.digest(),
            program_bytes: f.program_bytes.clone(),
            model_inventory_root: f.model_inventory_root.as_bytes(),
            param_commitments: f.params.clone(),
        },
    )
    .await;
    let row = net.api().unwrap().model_artifact_binding_header_v2(&bind).unwrap();
    assert!(!row.refuted);
    assert_eq!(row.pc_instances, 1_636);
    net.beat_to(row.matures_daa).await;
    let prepared = misaka_palw_sdk::flat_candidate_v9::prepare_flat_candidate_v9(
        bind.as_bytes(),
        f.descriptor.digest(),
        &f.program_bytes,
        &f.params,
        32,
    )
    .unwrap();
    send_actual(net, 0, &prepared.object).await;
    let record = net.ledger().flat_conformance_classes_v9.get(&prepared.class).unwrap().1.clone();
    assert_eq!(record.plan.max_positions, 32);
    assert_eq!(record.binding, bind.as_bytes());
    assert_eq!(record.program_bytes, f.program_bytes);
    assert_eq!(record.param_commitments, f.params);
    (bind, prepared.class, record)
}

fn actual_post(
    net: &Net,
    f: &FlatCheckpointFixtureV9,
    bind: Hash64,
    class: Digest,
    record: &misaka_palw_kernel::ledger::flat_candidate9::FlatCandidateRecordV9,
    wrong: bool,
) -> (Hash64, K, SegmentedCommitmentsV1) {
    let c = SegmentedCommitmentsV1::new(if wrong { f.wrong_commitments.clone() } else { f.honest_commitments.clone() });
    let header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: net.ledger().policy.network_domain,
        ruleset_digest: net.ledger().policy.ruleset_digest,
        class_binding_id: class,
        program_root: record.plan.program_root,
        artifact_root: record.param_commitments.root(),
        plan_root: record.plan.root(),
    };
    let evidence =
        build_segmented_evidence_v1(header, &f.descriptor, f.tokens.len() as u32, &prompt_root_of_ids_v1(&f.tokens), &[], &c);
    let mut post = FlatModelVectorPostV9 {
        version: 9,
        profile: record.plan.profile,
        court_set: record.plan.court_set,
        scope_root: record.plan.scope_root,
        replay_root: record.plan.replay_root,
        resource_root: record.plan.resource_root,
        trace: kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2 {
            version: 2,
            prompt: f.tokens.clone(),
            max_new_tokens: 1,
            generated: f.honest_generated.clone(),
            evidence,
            segment_roots: c.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    // The replicated roots are statement fields, not independent-run receipts.
    post.trace.implementation_roots = [post.trace_root(class, bind.as_bytes()); 3];
    let (id, object) = prepare_flat_model_post_v9(Hash64::from_bytes(class), bind, record, post).unwrap();
    (id, object, c)
}

fn fixture_answer(r: &FlatFixtureReplyV9, directory: &SegmentedCommitmentsV1) -> (FlatVectorUnitV9, FlatVectorAnswerV9) {
    let node = directory.node_opening(r.position, r.occurrence, r.node).unwrap();
    let hash_node = || FlatVectorAnswerV3 { version: 3, opening: node.clone(), leaf: None };
    let (kind, answer) = match r.kind {
        0 => {
            let supplied: misaka_palw_kernel::seg::NodeOpeningV1 = borsh::from_slice(&r.bytes).unwrap();
            assert_eq!(supplied, node);
            (FlatVectorUnitKindV9::Node, FlatVectorAnswerV9::Hash(hash_node()))
        }
        1 => {
            let leaf = misaka_palw_kernel::merkle5::decode_flat_hash_v5(&r.bytes).unwrap();
            (FlatVectorUnitKindV9::Hash(r.index), FlatVectorAnswerV9::Hash(FlatVectorAnswerV3 { leaf: Some(leaf), ..hash_node() }))
        }
        2 => (
            FlatVectorUnitKindV9::Header,
            FlatVectorAnswerV9::Header { node: hash_node(), value: FlatHeaderValueV9::from_bytes(&r.bytes).unwrap() },
        ),
        3 => (FlatVectorUnitKindV9::Value(r.index), FlatVectorAnswerV9::Value(FlatValueV9::from_bytes(&r.bytes).unwrap())),
        _ => panic!("unknown local fixture reply kind"),
    };
    (FlatVectorUnitV9 { position: r.position, occurrence: r.occurrence, node: r.node, kind }, answer)
}

#[tokio::test]
async fn flat_v9_actual_qwen32_public_point1500_own_seal_original_principal_and_native_replay() {
    kaspa_core::log::try_init_logger("warn");
    let f = actual_fixture();
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let (bind, class, record) = register_actual(&mut net, &f).await;
    let source_before = net.collateral(1);
    let p = TirProgramV1::decode_canonical(&f.program_bytes).unwrap();
    let inventory = PalwTirModelInventoryV2::new(&f.descriptor, &p).unwrap();
    assert_eq!(
        verify_artifact_mismatch_v2(&inventory, f.model_inventory_root, Hash64::from_bytes(f.params.root()), &f.honest_binding),
        Err("the two openings agree on every byte they share")
    );
    let fee_before = net.slashed(6);
    send_actual(&mut net, 6, &K::RefuteModelArtifactV2 { binding: bind.as_bytes(), proof: borsh::to_vec(&f.honest_binding).unwrap() })
        .await;
    assert!(net.slashed(6) > fee_before);
    assert_eq!(net.collateral(1), source_before);
    assert!(!net.api().unwrap().model_artifact_binding_header_v2(&bind).unwrap().refuted);

    // A distinct descriptor-scoped statement with one false PC tile is
    // challenged from just its two authenticated openings. It cannot corrupt
    // or slash the honest source statement or borrow its candidate identity.
    let ArtifactMismatchProofV1::TileV3 { commitments: false_pc, .. } = &f.false_binding else { panic!("actual false tile control") };
    let false_bind = model_artifact_binding_id_v2(
        f.descriptor.digest(),
        program_root_v1(&f.program_bytes),
        f.model_inventory_root,
        Hash64::from_bytes(false_pc.root()),
    );
    send_actual(
        &mut net,
        2,
        &K::BindModelArtifactV2 {
            descriptor: f.descriptor.digest(),
            program_bytes: f.program_bytes.clone(),
            model_inventory_root: f.model_inventory_root.as_bytes(),
            param_commitments: false_pc.clone(),
        },
    )
    .await;
    assert!(net.api().unwrap().model_artifact_binding_header_v2(&false_bind).is_some());
    let false_before = net.collateral(2);
    let control = K::RefuteModelArtifactV2 { binding: false_bind.as_bytes(), proof: borsh::to_vec(&f.false_binding).unwrap() };
    send_actual(&mut net, 6, &control).await;
    assert!(net.api().unwrap().model_artifact_binding_header_v2(&false_bind).unwrap().refuted);
    assert_eq!(false_before - net.collateral(2), mega(100));
    send_actual(&mut net, 6, &control).await;
    assert_eq!(false_before - net.collateral(2), mega(100));
    assert_eq!(net.collateral(1), source_before);

    let (honest_id, honest_object, _) = actual_post(&net, &f, bind, class, &record, false);
    send_actual(&mut net, 3, &honest_object).await;
    assert!(!net.api().unwrap().flat_model_vector_header_v3(&honest_id).unwrap().refuted);
    let (id, object, directory) = actual_post(&net, &f, bind, class, &record, true);
    send_actual(&mut net, 4, &object).await;
    assert!(net.api().unwrap().flat_model_vector_protected_v9(&id));
    let producer_before = net.collateral(4);
    for r in &f.requested_public_replies {
        let (unit, answer) = fixture_answer(r, &directory);
        let demand = ask(&mut net, id, unit.clone()).await;
        let row = net.api().unwrap().flat_vector_demand_v9(&demand).unwrap();
        assert!(answer.authenticates(&row), "the exact captured reply authenticates this native job demand");
        send(&mut net, 4, &K::AnswerFlatModelUnitV9 { demand: demand.as_bytes(), answer: answer.to_bytes().unwrap() }).await;
        assert!(net.api().unwrap().flat_vector_demand_v9(&demand).unwrap().settled);
        let snap = snapshot(&net);
        let public = snap.reads(id);
        let read = FlatReadV5 { position: unit.position, occurrence: unit.occurrence, node: unit.node, leaf: None };
        match unit.kind {
            FlatVectorUnitKindV9::Node => {
                assert_eq!(public.node(read), directory.node_opening(read.position, read.occurrence, read.node))
            }
            FlatVectorUnitKindV9::Hash(index) => assert_eq!(
                public.hash(FlatReadV5 { leaf: Some(index), ..read }).unwrap(),
                misaka_palw_kernel::merkle5::decode_flat_hash_v5(&r.bytes).unwrap()
            ),
            FlatVectorUnitKindV9::Header => {
                assert_eq!(public.header(FlatHeaderReadV9 { scope: record.plan.scope_root, node: read }).unwrap(), r.bytes)
            }
            FlatVectorUnitKindV9::Value(element) => {
                assert_eq!(public.value(FlatValueReadV9 { scope: record.plan.scope_root, node: read, element }).unwrap(), r.bytes)
            }
        }
    }
    let snap = snapshot(&net);
    let (_, view) = snap.candidate(id, Hash64::from_bytes(class), bind).unwrap();
    let fault = FlatFaultV9::from_bytes(&f.ordinary_fault_bytes).unwrap();
    let FlatFaultV9::Ordinary(point) = &fault else { panic!("actual fixture must carry an ordinary point fault") };
    assert_eq!(point.element, 1500);
    assert_eq!(point.output.flat[0].opening.index, 23);
    verify_flat_fault_v9(&view.context(), &fault).unwrap();
    let honest = snapshot(&net);
    let (_, honest) = honest.candidate(honest_id, Hash64::from_bytes(class), bind).unwrap();
    assert!(verify_flat_fault_v9(&honest.context(), &fault).is_err(), "wrong output proof cannot prosecute honest directory");
    assert!(net.ledger().classes.is_empty() && net.ledger().claims.is_empty(), "candidate9 statements grant no job/Final rights");
    let secret = FlatProofSecretV9::new(net.domain, id, net.bond(7), [91; 64], f.ordinary_fault_bytes.clone()).unwrap();
    let seal = secret.seal_object(net.domain, &net.bond(7)).unwrap();
    send(&mut net, 7, &seal).await;
    let row = net.api().unwrap().flat_model_seal_v9(&id, &net.kid(7)).unwrap();
    net.beat_to(row.seal.mature_daa).await;
    let row = snapshot(&net).reveal_row(id, Hash64::from_bytes(class), bind, &net.bond(7)).unwrap();
    let reveal = secret.reveal_object(net.domain, &net.bond(7), &row, net.daa()).unwrap();
    send(&mut net, 7, &reveal).await;
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(producer_before - net.collateral(4), mega(100));
    assert_eq!(net.collateral(1), source_before);
    let ttpb = net.ttpb();
    net.chain.heartbeat(ttpb, Vec::new()).await;
    let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
    assert_eq!(minted + owed, mega(49));
    assert!(minted > 0, "actual checkpoint point court bounty reaches an ordinary UTXO");
    send(&mut net, 7, &reveal).await;
    assert_eq!(producer_before - net.collateral(4), mega(100));
    assert!(!net.api().unwrap().flat_model_vector_header_v3(&honest_id).unwrap().refuted);
    let replay = net.replay().await;
    net.assert_same(
        &replay,
        "actual Qwen scope32/job1 bounded ordinary point1500, own actor court, original principal and redeemed UTXO",
    );
    assert_eq!(minted_and_owed(&net, &replay, 7), minted_and_owed(&net, &net.chain, 7));
}
