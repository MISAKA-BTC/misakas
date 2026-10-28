//! Actual checkpoint discovery by a fresh IBD node and new registered actor.
//! The finder obtains only op-211 candidate/job/replies and independently held
//! mapped weights. Producer mutation coordinates never enter finder replay.
//! Requires the local pinned artifact; this test is run explicitly for evidence.
//! Ordinary-only scope: no TopK/history/decode completeness, execution11 Final,
//! source floating fidelity or checkpoint-population acceptance is claimed.
use super::*;
use misaka_palw_kernel::job::DecodeRuleV1;
use misaka_palw_kernel::{
    flat_detect9::FlatHeaderValueV9,
    flat_resources10::{ResourceAxisV10, ResourceQuantitiesV10},
};
use misaka_palw_sdk::{flat_checkpoint_fixture9::FlatCheckpointFixtureV9, flat_streamed_locator_v9::*};
use misaka_palw_tir_exec::{NodeValue, StepSink, TirExecutor, node::TirArtifactV1};
use std::{collections::BTreeMap, path::Path};

/// Only this producer sees the injected location and privately owns its values.
struct Producer {
    map: BTreeMap<(u8, Option<u16>), usize>,
    values: Vec<Vec<Option<Tensor>>>,
    bytes: u128,
}
impl StepSink for Producer {
    fn every_node(&self) -> bool {
        true
    }
    fn node(&mut self, v: &NodeValue<'_>) {
        let s = self.map[&(v.block, v.layer)];
        self.bytes += v.data.len() as u128 * 16;
        assert!(self.bytes <= 256 << 20);
        let mut t = v.to_tensor();
        if (s, v.node) == (1, 27) {
            t.data[1500] += 1;
        }
        assert!(self.values[s][v.node as usize].replace(t).is_none());
    }
}
struct ActualServer {
    values: Vec<Vec<Tensor>>,
    directory: SegmentedCommitmentsV1,
    scope: Digest,
}
impl ActualServer {
    fn execute(path: &Path, p: &TirProgramV1, token: u32) -> Self {
        let artifact = TirArtifactV1::open(path).unwrap();
        assert_eq!(&artifact.plan().program, p);
        let mut producer = Producer {
            map: p.occurrences().into_iter().enumerate().map(|(s, k)| (k, s)).collect(),
            values: p.occurrences().iter().map(|(b, _)| (0..p.blocks[*b as usize].nodes.len()).map(|_| None).collect()).collect(),
            bytes: 0,
        };
        TirExecutor::new(artifact.plan(), artifact.params()).unwrap().step(token, &mut producer).unwrap();
        let values = producer.values.into_iter().map(|s| s.into_iter().map(Option::unwrap).collect::<Vec<_>>()).collect::<Vec<_>>();
        let directory = SegmentedCommitmentsV1::new(vec![
            values
                .iter()
                .map(|s| s.iter().map(|t| misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(t).unwrap()).collect())
                .collect(),
        ]);
        Self { values, directory, scope: [0; 64] }
    }
    fn answer(&self, u: &FlatVectorUnitV9) -> FlatVectorAnswerV9 {
        assert_eq!(u.position, 0);
        let t = &self.values[u.occurrence as usize][u.node as usize];
        let node = self.directory.node_opening(u.position, u.occurrence, u.node).unwrap();
        let hash = FlatVectorAnswerV3 { version: 3, opening: node.clone(), leaf: None };
        match u.kind {
            FlatVectorUnitKindV9::Node => FlatVectorAnswerV9::Hash(hash),
            FlatVectorUnitKindV9::Hash(index) => {
                FlatVectorAnswerV9::Hash(FlatVectorAnswerV3 { leaf: Some(FlatTreeV5::of(t).unwrap().opening(index).unwrap()), ..hash })
            }
            FlatVectorUnitKindV9::Header => {
                let shape = t.shape.iter().map(|d| *d as u64).collect();
                let header = if t.len() >= 64 {
                    misaka_palw_kernel::flat_court5::FlatHeaderV5::Flat {
                        dtype: t.dtype.tag(),
                        shape,
                        root: FlatTreeV5::of(t).unwrap().root(),
                    }
                } else {
                    let trees = TreesV3::of(t);
                    misaka_palw_kernel::flat_court5::FlatHeaderV5::Legacy {
                        dtype: t.dtype.tag(),
                        shape,
                        row_root: trees.row_root(),
                        col_root: trees.col_root(),
                    }
                };
                FlatVectorAnswerV9::Header { node: hash, value: FlatHeaderValueV9 { version: 9, scope: self.scope, header } }
            }
            FlatVectorUnitKindV9::Value(index) => {
                let (flat, legacy) = if t.len() >= 64 {
                    let tree = FlatTreeV5::of(t).unwrap();
                    (vec![FlatLeafValueV5::of(t, &tree, tree.layout().leaf_of(index).unwrap()).unwrap()], vec![])
                } else {
                    let l = misaka_palw_kernel::merkle3::LayoutV3::of(&t.shape);
                    let (row, tile, _) = l.row_leaf_of(index).unwrap();
                    (vec![], vec![LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, row, tile).unwrap()])
                };
                FlatVectorAnswerV9::Value(FlatValueV9 {
                    version: 9,
                    scope: self.scope,
                    value: FlatOperandV5 { node: Some(node), flat, legacy },
                })
            }
        }
    }
}
async fn send_large(net: &mut Net, card: usize, object: &K) {
    let route = net.route(card, object);
    let carriers = match kaspa_consensus_core::palw_state_v2::palw_object_chunks_with_cap_v1(&route, 32_000).unwrap() {
        Some(p) => p.into_iter().map(|p| (card, p)).collect(),
        None => vec![(card, route)],
    };
    net.send(carriers).await;
}
fn local_limits() -> MappedFlatReplayLimitsV9 {
    let mut reference = ResourceQuantitiesV10([1 << 36; 21]);
    reference.set(ResourceAxisV10::Macs, 1 << 35);
    // Explicit v2 local protocol: index steps are operation units. The v1
    // uniform 2^36 ceiling refused the genuine P1 geometry before source open;
    // retain that refusal, and keep every native admission/court cap unchanged.
    reference.set(ResourceAxisV10::IndexSteps, 1 << 38);
    reference.set(ResourceAxisV10::RawParameterStorage, 2 << 30);
    reference.set(ResourceAxisV10::ParameterHashBytes, 1 << 40);
    reference.set(ResourceAxisV10::TraceHashBytes, 1 << 40);
    MappedFlatReplayLimitsV9 {
        reference,
        retained_tensor_bytes: 256 << 20,
        raw_artifact_bytes: 2 << 30,
        parameter_workspace_bytes: 32 << 20,
        selected_parameter_bytes: 64 << 20,
        selected_parameter_total_bytes: 256 << 20,
        attempts: 64,
        public_reads: 512,
        public_reply_bytes: 4 << 20,
        scope_checker_work: 1 << 40,
        parameter_hash_bytes: 1 << 47,
        trace_hash_bytes: 1 << 43,
        artifact_hash_bytes: 4 << 30,
        inventory: MappedInventoryLimitsV9 {
            leaves: 2_000_000,
            raw_bytes: 2 << 30,
            hash_bytes: 4 << 30,
            work: 1 << 40,
            workspace_bytes: 512 << 10,
        },
    }
}
#[tokio::test]
#[ignore = "requires RFC02_ACTUAL_QWEN_FILE pointing to the independently acquired pinned1.8GB integer artifact"]
async fn flat_v9_actual_qwen_fresh_node_unknown_ordinary_public_discovery_own_proof_redemption_and_ibd() {
    use super::super::super::canonical::{agree, ibd_node, know_newcomer, newcomer_funding, register_newcomer, sync};
    kaspa_core::log::try_init_logger("warn");
    let path = std::env::var_os("RFC02_ACTUAL_QWEN_FILE").expect("explicit local artifact required");
    let path = Path::new(&path);
    let source: FlatCheckpointFixtureV9 = borsh::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/design/palw/tir/evidence/qwen25-real-generic9-fixture.borsh"
    )))
    .unwrap();
    // This archival object supplies only public registration metadata. Its
    // fault bytes, mutation location, old replies and both directories are
    // never passed to the fresh finder. Producer executes its own full model.
    let p = TirProgramV1::decode_canonical(&source.program_bytes).unwrap();
    let mut server = ActualServer::execute(path, &p, 4498);
    let generated = DecodeRuleV1::Greedy.select(&server.values[29][p.logits as usize]).unwrap();
    let mut main = Net::over_cfg(fresh_config(), TestConsensus::new);
    let funding = newcomer_funding(&main);
    main.beat_to(1).await;
    let binding = model_artifact_binding_id_v2(
        source.descriptor.digest(),
        program_root_v1(&source.program_bytes),
        source.model_inventory_root,
        Hash64::from_bytes(source.params.root()),
    );
    send_large(
        &mut main,
        1,
        &K::BindModelArtifactV2 {
            descriptor: source.descriptor.digest(),
            program_bytes: source.program_bytes.clone(),
            model_inventory_root: source.model_inventory_root.as_bytes(),
            param_commitments: source.params.clone(),
        },
    )
    .await;
    main.beat_to(main.api().unwrap().model_artifact_binding_header_v2(&binding).unwrap().matures_daa).await;
    let candidate = misaka_palw_sdk::flat_candidate_v9::prepare_flat_candidate_v9(
        binding.as_bytes(),
        source.descriptor.digest(),
        &source.program_bytes,
        &source.params,
        32,
    )
    .unwrap();
    send_large(&mut main, 0, &candidate.object).await;
    let record = main.ledger().flat_conformance_classes_v9[&candidate.class].1.clone();
    server.scope = record.plan.scope_root;
    let header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: main.ledger().policy.network_domain,
        ruleset_digest: main.ledger().policy.ruleset_digest,
        class_binding_id: candidate.class,
        program_root: record.plan.program_root,
        artifact_root: record.param_commitments.root(),
        plan_root: record.plan.root(),
    };
    let evidence = build_segmented_evidence_v1(header, &source.descriptor, 1, &prompt_root_of_ids_v1(&[4498]), &[], &server.directory);
    let mut post = FlatModelVectorPostV9 {
        version: 9,
        profile: record.plan.profile,
        court_set: record.plan.court_set,
        scope_root: record.plan.scope_root,
        replay_root: record.plan.replay_root,
        resource_root: record.plan.resource_root,
        trace: kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2 {
            version: 2,
            prompt: vec![4498],
            max_new_tokens: 1,
            generated: vec![generated],
            evidence,
            segment_roots: server.directory.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    post.trace.implementation_roots = [post.trace_root(candidate.class, binding.as_bytes()); 3];
    let (vector, object) = prepare_flat_model_post_v9(Hash64::from_bytes(candidate.class), binding, &record, post).unwrap();
    send(&mut main, 4, &object).await;
    let producer_before = main.collateral(4);
    let binder_before = main.collateral(1);
    let mut node = ibd_node(&main).await;
    let own_bond = register_newcomer(&mut node, funding).await;
    sync(&node, &mut main).await;
    know_newcomer(&mut main, own_bond);
    let snap = snapshot(&node);
    let (_, view) = snap.candidate(vector, Hash64::from_bytes(candidate.class), binding).unwrap();
    let started = std::time::Instant::now();
    let public_binding = snap.model_binding(binding).unwrap();
    let mut own = MappedFlatReplayV9::prepare(&view, &public_binding, binding, path, local_limits()).unwrap();
    assert_eq!(own.usage().model_inventory.leaves, 1_445_797);
    assert_eq!(own.usage().model_inventory.raw_bytes, 1_835_516_100);
    assert_eq!(own.usage().authenticated_model_inventory_root, Some(public_binding.model_inventory_root));
    assert_eq!(own.artifact_sha256, "435ed8fcddf68a4fc7dd22792dfe912fdbc3a9b4ab1a6803a711fcb1ff834a64");
    assert_eq!(own.usage().authenticated_parameter_instances, 1636);
    assert_eq!(own.usage().independently_executed_nodes, 7658);
    assert_eq!(own.usage().retained_tensor_bytes, 224_231_504);
    let mut demands = 0;
    let mut unknown_leaf_selected = false;
    let fault = loop {
        let snap = snapshot(&node);
        let (_, view) = snap.candidate(vector, Hash64::from_bytes(candidate.class), binding).unwrap();
        let result = own.check(&view, &snap.reads(vector));
        let unit = match result {
            MappedFlatReplayResultV9::Finding(FlatFindingV9::Fault(f)) => break f,
            MappedFlatReplayResultV9::Finding(FlatFindingV9::Demand(u)) => FlatVectorUnitV9 {
                position: u.position,
                occurrence: u.occurrence,
                node: u.node,
                kind: u.leaf.map(FlatVectorUnitKindV9::Hash).unwrap_or(FlatVectorUnitKindV9::Node),
            },
            MappedFlatReplayResultV9::Finding(FlatFindingV9::HeaderDemand(u)) => FlatVectorUnitV9 {
                position: u.node.position,
                occurrence: u.node.occurrence,
                node: u.node.node,
                kind: FlatVectorUnitKindV9::Header,
            },
            MappedFlatReplayResultV9::Finding(FlatFindingV9::ValueDemand(u)) => {
                unknown_leaf_selected |= u.element > 127;
                FlatVectorUnitV9 {
                    position: u.node.position,
                    occurrence: u.node.occurrence,
                    node: u.node.node,
                    kind: FlatVectorUnitKindV9::Value(u.element),
                }
            }
            other => panic!("actual ordinary outsider did not finish: {other:?}"),
        };
        assert!(demands < 64);
        let demand = ask_actor(&mut node, 8, vector, unit.clone()).await;
        demands += 1;
        // Server receives only the exact on-chain demanded unit, with no finder
        // hint. Every answer becomes immutable public material before retry.
        let answer = server.answer(&unit);
        let row = node.api().unwrap().flat_vector_demand_v9(&demand).unwrap();
        assert!(answer.authenticates(&row));
        send(&mut node, 4, &K::AnswerFlatModelUnitV9 { demand: demand.as_bytes(), answer: answer.to_bytes().unwrap() }).await;
        assert!(node.api().unwrap().flat_vector_demand_v9(&demand).unwrap().settled);
    };
    assert!(unknown_leaf_selected);
    assert_eq!(own.authenticated_matching_prefix_nodes(), 35);
    let FlatFaultV9::Ordinary(point) = &fault else { panic!("ordinary court expected") };
    assert_eq!(
        (point.output.node.as_ref().unwrap().occurrence, point.output.node.as_ref().unwrap().node, point.element),
        (1, 27, 1500)
    );
    assert_eq!(point.output.flat[0].opening.index, 23);
    let snap = snapshot(&node);
    let (_, view) = snap.candidate(vector, Hash64::from_bytes(candidate.class), binding).unwrap();
    verify_flat_fault_v9(&view.context(), &fault).unwrap();
    let proof = fault.to_bytes().unwrap();
    assert_eq!(proof.len(), 17816);
    let secret = FlatProofSecretV9::new(node.domain, vector, own_bond, [123; 64], proof).unwrap();
    let seal = secret.seal_object(node.domain, &own_bond).unwrap();
    send(&mut node, 8, &seal).await;
    let row = node.api().unwrap().flat_model_seal_v9(&vector, &node.kid(8)).unwrap();
    node.beat_to(row.seal.mature_daa).await;
    let row = snapshot(&node).reveal_row(vector, Hash64::from_bytes(candidate.class), binding, &own_bond).unwrap();
    let reveal = secret.reveal_object(node.domain, &own_bond, &row, node.daa()).unwrap();
    send(&mut node, 8, &reveal).await;
    assert!(node.api().unwrap().flat_model_vector_header_v3(&vector).unwrap().refuted);
    assert_eq!(producer_before - node.collateral(4), mega(100));
    assert_eq!(binder_before, node.collateral(1));
    let ttpb = node.ttpb();
    node.chain.heartbeat(ttpb, Vec::new()).await;
    let (minted, owed) = minted_and_owed(&node, &node.chain, 8);
    assert_eq!(minted + owed, mega(49));
    assert!(minted > 0);
    send(&mut node, 8, &reveal).await;
    assert_eq!(producer_before - node.collateral(4), mega(100));
    sync(&node, &mut main).await;
    agree(&main, &node, "actual Qwen fresh node public unknown ordinary proof");
    let replay = node.replay().await;
    node.assert_same(&replay, "actual mapped source own proof, original principal and UTXO IBD");
    assert_eq!(minted_and_owed(&node, &replay, 8), (minted, owed));
    assert!(node.ledger().classes.is_empty() && node.ledger().claims.is_empty());
    eprintln!(
        "ACTUAL_QWEN_FRESH_ORDINARY9 demands={demands} own_usage={:?} elapsed_ms={} scope={} no_native_Final=true",
        own.usage(),
        started.elapsed().as_millis(),
        Hash64::from_bytes(view.scope.root())
    );
}
