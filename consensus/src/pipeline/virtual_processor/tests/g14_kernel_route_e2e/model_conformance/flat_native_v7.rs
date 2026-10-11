//! Ordinary node fold, public op-211 replies and independent source replay7.
//! Synthetic sparse graph evidence is not real checkpoint fidelity or full RFC.
use super::*;
use kaspa_consensus_core::palw_flat_model_vector_da_v3::FlatVectorAnswerV3;
use kaspa_consensus_core::palw_flat_model_vector_da_v7::*;
use kaspa_consensus_core::palw_flat_model_vector_v7::*;
use misaka_palw_kernel::flat_court5::FlatOperandV5;
use misaka_palw_kernel::flat_court7::{FlatFaultV7, verify_flat_fault_v7};
use misaka_palw_kernel::flat_detect7::{FlatFindingV7, FlatValueV7};
use misaka_palw_kernel::flat_replay6::FlatReplayLimitsV6;
use misaka_palw_kernel::flat_replay7::*;
use misaka_palw_kernel::merkle3::{LeafOpeningV3, TreesV3};
use misaka_palw_kernel::merkle5::{FlatLeafValueV5, FlatTreeV5};
use misaka_palw_sdk::flat_snapshot_v7::*;
use misaka_palw_sdk::flat_vector_v7::*;

struct Sparse7 {
    tie: bool,
    d: KernelDescriptorV1,
    p: TirProgramV1,
    params: MapParams,
    pc: ParamCommitmentsV1,
    r: Hash64,
}
fn sparse7(tie: bool) -> Sparse7 {
    let mut b = ProgramBuilder::new(4, HISTORY_BOUND_V1_SMALL);
    let scores = b.param("scores", DType::I32, &[65], false);
    let experts = b.param("experts", DType::I8, &[65, 4], false);
    let state = b.fixed_state("selected.memory", DType::I32, &[4], -100000, 100000, false);
    let pre = {
        let mut k = b.block("pre", vec![]);
        let selected = k.topk(scores, 0, 2);
        let x = k.gather(experts, selected, 0, 0);
        let x = k.reduce_sum(x, 0, DType::I64);
        let x = k.reshape_fixed(x, &[4]);
        let x = k.add(x, Ref::State(state), DType::I64);
        let x = k.clamp(x, -100000, 100000, DType::I32);
        let x = k.state_write(state, x);
        k.finish(&[x])
    };
    let post = {
        let mut k = b.block("post", vec![TensorType::fixed(DType::I32, &[4])]);
        let x = k.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        k.commit(x);
        k.finish(&[])
    };
    let mut p = b.finish(pre, vec![], post, 0);
    p.logits_scheme_id = kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes();
    let d = k2_tir_v4_descriptor();
    let params = MapParams {
        tensors: [
            ((0, None), Tensor::new(DType::I32, vec![65], (0..65).map(|x| if tie { 14 } else { x as i128 }).collect()).unwrap()),
            ((1, None), Tensor::new(DType::I8, vec![65, 4], (0..65 * 4).map(|x| (x % 4 + 1) as i128).collect()).unwrap()),
        ]
        .into(),
    };
    let pc = ParamCommitmentsV1::of_v3(&params);
    let inv = PalwTirModelInventoryV2::new(&d, &p).unwrap();
    let mut leaves = vec![];
    inv.visit_rows(&mut |r| {
        let t = &params.tensors[&(r.param, None)];
        let raw = t.to_le_bytes();
        leaves.push(artifact_leaf_v1(&PalwArtifactOperandV1 {
            tensor_name: p.params[r.param as usize].name.clone(),
            layer: r.layer,
            row_start: r.row_start,
            bytes: raw[r.row_start as usize..(r.row_start + r.len) as usize].to_vec(),
        }));
    })
    .unwrap();
    Sparse7 { tie, d, p, params, pc, r: artifact_root_v1(&leaves).unwrap() }
}
async fn admitted(net: &mut Net, f: &Sparse7) -> (Hash64, Digest, misaka_palw_kernel::ledger::flat_candidate7::FlatCandidateRecordV7) {
    admitted_bound(net, f, 3).await
}
async fn admitted_bound(
    net: &mut Net,
    f: &Sparse7,
    max_positions: u32,
) -> (Hash64, Digest, misaka_palw_kernel::ledger::flat_candidate7::FlatCandidateRecordV7) {
    net.beat_to(1).await;
    let object = K::BindModelArtifactV2 {
        descriptor: f.d.digest(),
        program_bytes: f.p.encode(),
        model_inventory_root: f.r.as_bytes(),
        param_commitments: f.pc.clone(),
    };
    let bind = model_artifact_binding_id_v2(f.d.digest(), program_root_v1(&f.p.encode()), f.r, Hash64::from_bytes(f.pc.root()));
    send(net, 1, &object).await;
    net.beat_to(net.api().unwrap().model_artifact_binding_v2(&bind).unwrap().matures_daa).await;
    let p = misaka_palw_sdk::flat_candidate_v7::prepare_flat_candidate_v7(
        bind.as_bytes(),
        f.d.digest(),
        &f.p.encode(),
        &f.pc,
        max_positions,
    )
    .unwrap();
    send(net, 0, &p.object).await;
    let r = net.ledger().flat_conformance_classes_v7[&p.class].1.clone();
    (bind, p.class, r)
}
/// Producer side only. The finder receives this helper's on-chain answer bytes.
struct Server7 {
    trace: TraceV1,
    c: SegmentedCommitmentsV1,
}
impl Server7 {
    fn answer(&self, u: &FlatVectorUnitV7) -> FlatVectorAnswerV7 {
        let node = self.c.node_opening(u.position, u.occurrence, u.node).unwrap();
        let t = &self.trace.values[u.position as usize][u.occurrence as usize][u.node as usize];
        match u.kind {
            FlatVectorUnitKindV7::Node => FlatVectorAnswerV7::Hash(FlatVectorAnswerV3 { version: 3, opening: node, leaf: None }),
            FlatVectorUnitKindV7::Hash(index) => FlatVectorAnswerV7::Hash(FlatVectorAnswerV3 {
                version: 3,
                opening: node,
                leaf: Some(FlatTreeV5::of(t).unwrap().opening(index).unwrap()),
            }),
            FlatVectorUnitKindV7::Value(index) => {
                let (flat, legacy) = if t.data.len() >= 64 {
                    let tree = FlatTreeV5::of(t).unwrap();
                    let leaf = tree.layout().leaf_of(index).unwrap();
                    (vec![FlatLeafValueV5::of(t, &tree, leaf).unwrap()], vec![])
                } else {
                    let layout = misaka_palw_kernel::merkle3::LayoutV3::of(&t.shape);
                    let (line, tile, _) = layout.row_leaf_of(index).unwrap();
                    (vec![], vec![LeafOpeningV3::of_trees(t, &TreesV3::of(t), 0, line, tile).unwrap()])
                };
                FlatVectorAnswerV7::Value(FlatValueV7 { version: 7, value: FlatOperandV5 { node: Some(node), flat, legacy } })
            }
        }
    }
}
fn statement(
    net: &Net,
    f: &Sparse7,
    bind: Hash64,
    class: Digest,
    record: &misaka_palw_kernel::ledger::flat_candidate7::FlatCandidateRecordV7,
    corrupt: Option<u8>,
) -> (FlatModelVectorPostV7, Server7) {
    let prompt = vec![0, 1];
    let mut trace = misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &prompt).unwrap();
    if let Some(case) = corrupt {
        match case {
            0 => trace.values[1][0][0].data = vec![64, 65],
            1 => trace.values[1][0][0].data = vec![63, 63],
            2 => trace.values[1][0][0].data = vec![64, 63],
            3 => trace.values[1][0][0].data = vec![0, 1],
            4 => trace.values[1][0][6].data[0] += 1,
            _ => trace.values[1][0][0].data = vec![1, 2],
        }
    }
    let c = SegmentedCommitmentsV1::new(
        trace
            .values
            .iter()
            .map(|p| {
                p.iter()
                    .map(|s| s.iter().map(|t| misaka_palw_kernel::flat_court5::flat_profile_commitment_v5(t).unwrap()).collect())
                    .collect()
            })
            .collect(),
    );
    let mut header = misaka_palw_kernel::evidence::EvidenceHeaderV1 {
        network_domain: net.domain.as_bytes(),
        ruleset_digest: net.ledger().policy.ruleset_digest,
        class_binding_id: class,
        program_root: record.plan.program_root,
        artifact_root: record.param_commitments.root(),
        plan_root: record.plan.root(),
    };
    header.network_domain = net.ledger().policy.network_domain;
    let evidence = build_segmented_evidence_v1(header, &f.d, prompt.len() as u32, &prompt_root_of_ids_v1(&prompt), &[], &c);
    let mut post = FlatModelVectorPostV7 {
        version: 7,
        profile: record.plan.profile,
        court_set: record.plan.court_set,
        scope_root: record.plan.scope_root,
        replay_root: record.plan.replay_root,
        lengths: None,
        trace: kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2 {
            version: 2,
            prompt,
            max_new_tokens: 1,
            generated: vec![3],
            evidence,
            segment_roots: c.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    post.trace.implementation_roots = [post.trace_root(class, bind.as_bytes()); 3];
    (post, Server7 { trace, c })
}
fn snapshot(net: &Net) -> FlatPublicSnapshotV7 {
    use kaspa_rpc_core::GetPalwKernelRowsRequest;
    use kaspa_rpc_core::convert::palw_kernel::{palw_kernel_rows_request_v1, palw_kernel_rows_response_v1};
    let a = net.api().unwrap();
    let anchor = FlatSnapshotAnchorV7 {
        tip_daa: net.chain.ctx.consensus.get_virtual_daa_score(),
        network_domain: Hash64::from_bytes(a.header.policy.network_domain),
        ruleset_digest: Hash64::from_bytes(a.header.policy.ruleset_digest),
        ledger_root: a.ledger_root(),
        aux_root: a.aux_root(),
    };
    let node = net.chain.ctx.consensus.consensus_clone();
    let mut snapshot = misaka_palw_sdk::onboarding_snapshot::KernelRowsSnapshotV1::default();
    let mut request = GetPalwKernelRowsRequest { max_bytes: 16 << 10, ..Default::default() };
    loop {
        let wire: GetPalwKernelRowsRequest = serde_json::from_slice(&serde_json::to_vec(&request).unwrap()).unwrap();
        let (after, max) = palw_kernel_rows_request_v1(&wire).unwrap();
        let bytes = serde_json::to_vec(&palw_kernel_rows_response_v1(node.as_ref(), after, max).unwrap()).unwrap();
        match snapshot.push(misaka_palw_sdk::flat_candidate_v6::snapshot::flat_rpc_page_v6(&bytes).unwrap()).unwrap() {
            None => return FlatPublicSnapshotV7::from_snapshot(snapshot, anchor).unwrap(),
            Some((t, k)) => {
                request = GetPalwKernelRowsRequest {
                    has_cursor: true,
                    after_table: t as u32,
                    after_key: faster_hex::hex_string(&k),
                    max_bytes: 16 << 10,
                }
            }
        }
    }
}
fn limits(e: &FlatReplayEnvelopeV7, attempts: u32) -> FlatReplayLimitsV7 {
    let r = &e.reference;
    FlatReplayLimitsV7 {
        reference: FlatReplayLimitsV6 {
            position_tensor_bytes: r.position_tensor_bytes,
            operand_tensor_bytes: r.largest_operand_tensor_bytes,
            history_tensor_bytes: r.history_rows_tensor_bytes,
            parameter_hash_bytes: r.parameter_hash_bytes_per_attempt * attempts as u128,
            trace_hash_bytes: r.trace_hash_bytes_per_attempt * attempts as u128,
            node_reads: r.node_reads_per_attempt * attempts as u128,
            hash_reads: r.hash_reads_per_attempt * attempts as u128,
            reply_bytes: r.reply_bytes_per_attempt * attempts as u128,
            attempts,
        },
        topk_scan_work: e.topk_scan_work_per_attempt * attempts as u128,
        topk_auxiliary_bytes: e.topk_auxiliary_bytes,
        selected_value_reads: e.selected_value_reads_per_attempt * attempts as u128,
    }
}
async fn ask(net: &mut Net, vector: Hash64, unit: FlatVectorUnitV7) -> Hash64 {
    ask_actor(net, 7, vector, unit).await
}
async fn ask_actor(net: &mut Net, actor: usize, vector: Hash64, unit: FlatVectorUnitV7) -> Hash64 {
    send(
        net,
        actor,
        &K::DemandFlatModelUnitV7 {
            vector: vector.as_bytes(),
            slot: 0,
            position: unit.position,
            occurrence: unit.occurrence,
            node: unit.node,
            kind: match unit.kind {
                FlatVectorUnitKindV7::Node => 0,
                FlatVectorUnitKindV7::Hash(_) => 1,
                FlatVectorUnitKindV7::Value(_) => 2,
            },
            index: match unit.kind {
                FlatVectorUnitKindV7::Node => 0,
                FlatVectorUnitKindV7::Hash(i) | FlatVectorUnitKindV7::Value(i) => i,
            },
        },
    )
    .await;
    net.api().unwrap().flat_vector_slot_v7(&net.bond(actor), 0).unwrap()
}
async fn independent(net: &mut Net, f: &Sparse7, vector: Hash64, class: Digest, bind: Hash64, server: &Server7) -> FlatFindingV7 {
    independent_actor(net, 7, f, vector, class, bind, server).await
}
async fn independent_actor(
    net: &mut Net,
    actor: usize,
    f: &Sparse7,
    vector: Hash64,
    class: Digest,
    bind: Hash64,
    server: &Server7,
) -> FlatFindingV7 {
    let own = sparse7(f.tie).params; // New independent source copy, no producer trace or hooks.
    assert_eq!(own.tensors, f.params.tensors);
    let initial = snapshot(net);
    let (_, v) = initial.candidate(vector, Hash64::from_bytes(class), bind).unwrap();
    let mut session = FlatReplaySessionV7::new(limits(&v.replay, 128));
    for _ in 0..128 {
        let snap = snapshot(net);
        let (_, v) = snap.candidate(vector, Hash64::from_bytes(class), bind).unwrap();
        let result = session.check(
            &v.context(),
            &v.replay,
            &snap.reads(vector),
            &|i, l| own.tensors.get(&(i, l)).cloned(),
            v.trace.job.inline_prompt.as_ref().unwrap(),
        );
        let finding = match result {
            FlatReplayResultV7::Finding(f) => f,
            x => panic!("independent bounded checker: {x:?}"),
        };
        let unit = match finding {
            FlatFindingV7::Demand(u) => FlatVectorUnitV7 {
                position: u.position,
                occurrence: u.occurrence,
                node: u.node,
                kind: u.leaf.map(FlatVectorUnitKindV7::Hash).unwrap_or(FlatVectorUnitKindV7::Node),
            },
            FlatFindingV7::ValueDemand(u) => FlatVectorUnitV7 {
                position: u.node.position,
                occurrence: u.node.occurrence,
                node: u.node.node,
                kind: FlatVectorUnitKindV7::Value(u.element),
            },
            f => return f,
        };
        let d = ask_actor(net, actor, vector, unit.clone()).await;
        let answer = server.answer(&unit).to_bytes().unwrap();
        send(net, 4, &K::AnswerFlatModelUnitV7 { demand: d.as_bytes(), answer }).await;
        assert!(net.api().unwrap().flat_vector_demand_v7(&d).unwrap().settled);
    }
    panic!("bounded acquisition did not terminate")
}
#[tokio::test]
async fn flat_v7_native_independent_sparse_state_checker_public_values_own_seal_once_capital_and_redemption() {
    for corrupt in [None, Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)] {
        let f = sparse7(corrupt == Some(5));
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let (bind, class, record) = admitted(&mut net, &f).await;
        let (post, server) = statement(&net, &f, bind, class, &record, corrupt);
        let (id, o) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
        send(&mut net, 4, &o).await;
        assert!(net.api().unwrap().flat_model_vector_protected_v7(&id));
        assert!(!net.api().unwrap().flat_model_vector_protected_v6(&id));
        let source = net.collateral(1);
        let before = net.collateral(4);
        let result = independent(&mut net, &f, id, class, bind, &server).await;
        if corrupt.is_none() {
            assert_eq!(result, FlatFindingV7::Clean);
            assert!(!net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
        } else {
            let FlatFindingV7::Fault(fault) = result else { panic!("no fault: {result:?}") };
            let v = net.api().unwrap().flat_model_vector_view_v7(&id).unwrap();
            verify_flat_fault_v7(&v.context(), &fault).unwrap();
            if corrupt.unwrap() != 4 {
                assert!(matches!(fault, FlatFaultV7::TopK(_)));
            } else {
                assert!(matches!(fault, FlatFaultV7::V5(_)));
            }
            let proof = fault.to_bytes().unwrap();
            send(&mut net, 6, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: proof.clone() }).await;
            send(&mut net, 6, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt: [42; 64], proof: proof.clone() }).await;
            assert_eq!(net.owed(6), 0);
            assert_eq!(net.collateral(4), before);
            let secret = FlatProofSecretV7::new(net.domain, id, net.bond(7), [42; 64], proof.clone()).unwrap();
            let seal = secret.seal_object(net.domain, &net.bond(7)).unwrap();
            send(&mut net, 7, &seal).await;
            let row = net.api().unwrap().flat_model_seal_v7(&id, &net.kid(7)).unwrap();
            assert!(net.chain.tip_state().1.flat_model_seal_reserved_v7(&net.bond(7)) > 0);
            let K::SealFlatModelProofV7 { seal, .. } = seal else { unreachable!() };
            send(&mut net, 6, &K::SealFlatModelProofV7 { vector: id.as_bytes(), seal }).await;
            net.beat_to(row.seal.mature_daa).await;
            send(&mut net, 6, &K::RevealFlatModelProofV7 { vector: id.as_bytes(), salt: [42; 64], proof: proof.clone() }).await;
            assert_eq!(net.owed(6), 0);
            let snap = snapshot(&net);
            let row = snap.reveal_row(id, Hash64::from_bytes(class), bind, &net.bond(7)).unwrap();
            let reveal = secret.reveal_object(net.domain, &net.bond(7), &row, net.daa()).unwrap();
            send(&mut net, 7, &reveal).await;
            assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
            assert_eq!(before - net.collateral(4), mega(100));
            assert_eq!(minted_and_owed(&net, &net.chain, 7).0 + minted_and_owed(&net, &net.chain, 7).1, mega(49));
            assert_eq!(net.chain.tip_state().1.flat_model_seal_reserved_v7(&net.bond(7)), 0);
            send(&mut net, 7, &reveal).await;
            assert_eq!(before - net.collateral(4), mega(100));
            let copyist = net.kid(6);
            send(&mut net, 3, &K::ExpireFlatModelSealV7 { vector: id.as_bytes(), actor: copyist }).await;
        }
        assert_eq!(net.collateral(1), source);
        assert!(net.ledger().classes.is_empty() && net.ledger().claims.is_empty());
        let replay = net.replay().await;
        net.assert_same(&replay, "public mixed scope7 state/sparse checker, own court and original capital settlement");
    }
}

#[tokio::test]
async fn flat_v7_native_own_chunk_lane_refuses_unsealed_and_borrowed_actor_survives_generic_saturation() {
    use kaspa_consensus_core::palw_kernel_route_v1::*;
    use kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_GROUPS;
    let f = sparse7(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let (bind, class, record) = admitted(&mut net, &f).await;
    let (post, server) = statement(&net, &f, bind, class, &record, Some(0));
    let (id, o) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
    send(&mut net, 4, &o).await;
    let FlatFindingV7::Fault(fault) = independent(&mut net, &f, id, class, bind, &server).await else { panic!() };
    let proof = fault.to_bytes().unwrap();
    let actor = net.kid(7);
    let salt = [71; 64];
    let inner = net.route(7, &K::RevealFlatModelProofV7 { vector: id.as_bytes(), salt, proof: proof.clone() });
    let target = PalwKernelChunkTargetV1::FlatProofV7 { vector: id, actor };
    let parts = palw_kernel_chunks_v1(&inner, net.bond(7), target, 1024).unwrap();
    assert!(parts.len() > 1);
    let unsealed = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, unsealed)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 0);
    let seal = misaka_palw_kernel::flat_seal7::flat_proof_seal_v7(&id.as_bytes(), &actor, &salt, &proof).unwrap();
    send(&mut net, 7, &K::SealFlatModelProofV7 { vector: id.as_bytes(), seal }).await;
    net.beat_to(net.api().unwrap().flat_model_seal_v7(&id, &actor).unwrap().seal.mature_daa).await;
    let mut borrowed = parts[0].clone();
    borrowed.opener = net.bond(6);
    let borrowed = signed_flat_chunk(&mut net, 6, borrowed);
    net.send(vec![(6, borrowed)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(6)), 0);
    net.send(
        (0..PALW_OBJECT_CHUNK_MAX_GROUPS)
            .map(|i| {
                (
                    1,
                    Obj::ObjectChunk {
                        group: Hash64::from_u64_word(9900 + i as u64),
                        index: 0,
                        count: 16,
                        bytes: vec![i as u8; 1024],
                    },
                )
            })
            .collect(),
    )
    .await;
    for i in 0..PALW_OBJECT_CHUNK_MAX_GROUPS {
        assert!(net.chain.tip_state().1.pending_chunk_group(&Hash64::from_u64_word(9900 + i as u64)).is_some());
    }
    let before = net.collateral(4);
    let first = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, first)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 1);
    assert_eq!(
        net.api().unwrap().chunk_deposits_of_v1(&net.bond(7)),
        parts.len() as u64 * PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1
    );
    for p in parts.iter().skip(1) {
        let p = signed_flat_chunk(&mut net, 7, p.clone());
        net.send(vec![(7, p)]).await;
    }
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(net.api().unwrap().chunk_deposits_of_v1(&net.bond(7)), 0);
    assert_eq!(before - net.collateral(4), mega(100));
    let again = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, again)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 0);
    assert_eq!(before - net.collateral(4), mega(100));
    let replay = net.replay().await;
    net.assert_same(&replay, "own mature chunk7 uses independent lane and settles original once");
}

#[tokio::test]
async fn flat_v7_native_selected_value_default_holds_original_horizon_and_cannot_be_slashed_again() {
    let f = sparse7(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let (bind, class, record) = admitted(&mut net, &f).await;
    let (post, server) = statement(&net, &f, bind, class, &record, Some(0));
    let (id, o) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
    send(&mut net, 4, &o).await;
    let unit = FlatVectorUnitV7 { position: 1, occurrence: 0, node: 0, kind: FlatVectorUnitKindV7::Value(1) };
    let d = ask(&mut net, id, unit.clone()).await;
    let mut answer = server.answer(&unit).to_bytes().unwrap();
    answer.push(0);
    let fee = net.slashed(6);
    send(&mut net, 6, &K::AnswerFlatModelUnitV7 { demand: d.as_bytes(), answer }).await;
    assert!(net.slashed(6) > fee);
    assert!(!net.api().unwrap().flat_vector_demand_v7(&d).unwrap().settled);
    let until = net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().liability_until;
    net.beat_to(until + 1).await;
    assert_eq!(net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa()), mega(100) as u128);
    let before = net.collateral(4);
    send(&mut net, 3, &K::SettleFlatModelDemandV7 { demand: d.as_bytes() }).await;
    assert!(net.api().unwrap().flat_vector_unavailable_v7(&id));
    assert_eq!(before - net.collateral(4), mega(100));
    assert_eq!(net.owed(7), mega(49));
    assert_eq!(net.owed(3), 0);
    assert_eq!(net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa()), 0);
    assert_eq!(net.chain.tip_state().1.flat_vector_demand_reserved_v7(&net.bond(7)), 0);
    send(&mut net, 3, &K::SettleFlatModelDemandV7 { demand: d.as_bytes() }).await;
    send(&mut net, 7, &K::RevealFlatModelProofV7 { vector: id.as_bytes(), salt: [1; 64], proof: vec![7, 0, 255] }).await;
    assert_eq!(before - net.collateral(4), mega(100));
    let replay = net.replay().await;
    net.assert_same(&replay, "selected value7 fixed-row default holds original horizon and is once only");
}

#[tokio::test]
async fn flat_v7_native_fixed_text_length_specialization_is_identity_bound_and_cannot_grow_or_bypass_scope() {
    use misaka_palw_kernel::flat_lengths7::*;
    let f = sparse7(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let (bind, class, record) = admitted_bound(&mut net, &f, 2).await;
    let (mut post, server) = statement(&net, &f, bind, class, &record, None);
    let template = FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7 { token: 3, patch: 0, audio: 0 },
        max_program_bytes: f.p.encode(),
        bindings: vec![],
    };
    let specialized = template.specialize(FlatJobLengthsV7 { token: 2, patch: 0, audio: 0 }, 2, false).unwrap();
    assert_eq!(specialized.program, f.p);
    post.lengths = Some(FlatLengthBindingV7 { template: template.to_bytes().unwrap(), admission: specialized.admission });
    post.trace.implementation_roots = [post.trace_root(class, bind.as_bytes()); 3];
    for field in 0..4 {
        let mut bad = post.clone();
        match field {
            0 => bad.lengths.as_mut().unwrap().admission.program_root[0] ^= 1,
            1 => bad.lengths.as_mut().unwrap().admission.actual.token = 3,
            2 => bad.lengths.as_mut().unwrap().admission.actual.patch = 1,
            _ => bad.lengths.as_mut().unwrap().template[14..18].copy_from_slice(&u32::MAX.to_le_bytes()),
        }
        bad.trace.implementation_roots = [bad.trace_root(class, bind.as_bytes()); 3];
        let id = bad.id(class, bind.as_bytes());
        let fee = net.slashed(4);
        send(&mut net, 4, &K::PostFlatModelVectorV7 { class, binding: bind.as_bytes(), post: borsh::to_vec(&bad).unwrap() }).await;
        assert!(net.api().unwrap().flat_model_vector_header_v3(&id).is_none());
        assert!(net.slashed(4) > fee);
    }
    let (id, o) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
    send(&mut net, 4, &o).await;
    assert!(net.api().unwrap().flat_model_vector_protected_v7(&id));
    assert_eq!(independent(&mut net, &f, id, class, bind, &server).await, FlatFindingV7::Clean);
    let replay = net.replay().await;
    net.assert_same(&replay, "native text length7 frozen at scoped candidate admission and exact job post");
}

// Re-address ordinary premine coins, never a bond, to the same public participant
// used by the canonical harness. Registration is an actual lifecycle transaction.
fn fresh_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    use kaspa_consensus_core::config::premine::{MAIN_PREMINE_INDEX, premine_outpoint_for};
    use kaspa_consensus_core::muhash::MuHashExtensions;
    let (cfg, bundle, mut premine, floats) = config();
    let mut params = cfg.params.clone();
    let main = premine_outpoint_for(params.net, MAIN_PREMINE_INDEX);
    let (_, entry) = premine.iter_mut().find(|(o, _)| *o == main).unwrap();
    entry.script_public_key = card_payout_spk(8);
    let mut set = kaspa_muhash::MuHash::new();
    for (outpoint, entry) in &premine {
        set.add_utxo(outpoint, entry);
    }
    params.genesis.utxo_commitment = set.finalize();
    params.genesis.hash = kaspa_consensus_core::header::Header::from(&params.genesis).hash;
    assert!(params.validate_palw_v2().is_err(), "shipping validation stays dormant");
    (Config::new(params), bundle, premine, floats)
}

#[tokio::test]
async fn flat_v7_native_fresh_ibd_and_pruned_registered_participant_public_acquisition_conviction_and_redemption() {
    use super::super::canonical::{agree, ibd_node, know_newcomer, newcomer_funding, pruned_import_node, register_newcomer, sync};
    for pruned in [false, true] {
        let f = sparse7(false);
        let mut main = Net::over_cfg(fresh_config(), TestConsensus::new);
        let funding = newcomer_funding(&main);
        let (bind, class, record) = admitted(&mut main, &f).await;
        let (post, server) = statement(&main, &f, bind, class, &record, Some(0));
        let (id, object) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
        send(&mut main, 4, &object).await;
        let before = main.collateral(4);
        let source = main.collateral(1);
        let mut node = if pruned { pruned_import_node(&mut main).await } else { ibd_node(&main).await };
        let bond = register_newcomer(&mut node, funding).await;
        assert_eq!(node.bond(8), bond);
        assert!(node.chain.tip_state().1.bond(&bond).unwrap().registered_daa > 0);
        sync(&node, &mut main).await;
        know_newcomer(&mut main, bond);
        assert_eq!(main.api().unwrap().flat_vector_open_demands_v7(&id), 0);
        // No source acquisition produces no court object and changes no chain verdict.
        let observed = snapshot(&node).candidate(id, Hash64::from_bytes(class), bind).unwrap().0;
        assert!(!observed.refuted);
        let FlatFindingV7::Fault(fault) = independent_actor(&mut node, 8, &f, id, class, bind, &server).await else {
            panic!("fresh public participant must find range fault")
        };
        let view = snapshot(&node);
        let (_, view) = view.candidate(id, Hash64::from_bytes(class), bind).unwrap();
        verify_flat_fault_v7(&view.context(), &fault).unwrap();
        let proof = fault.to_bytes().unwrap();
        let secret = FlatProofSecretV7::new(node.domain, id, bond, [88; 64], proof).unwrap();
        let seal = secret.seal_object(node.domain, &bond).unwrap();
        send(&mut node, 8, &seal).await;
        let row = node.api().unwrap().flat_model_seal_v7(&id, &node.kid(8)).unwrap();
        node.beat_to(row.seal.mature_daa).await;
        let row = snapshot(&node).reveal_row(id, Hash64::from_bytes(class), bind, &bond).unwrap();
        let reveal = secret.reveal_object(node.domain, &bond, &row, node.daa()).unwrap();
        send(&mut node, 8, &reveal).await;
        assert!(node.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
        assert_eq!(before - node.collateral(4), mega(100));
        assert_eq!(source, node.collateral(1));
        let ttpb = node.ttpb();
        node.chain.heartbeat(ttpb, Vec::new()).await;
        let (minted, owed) = minted_and_owed(&node, &node.chain, 8);
        assert_eq!(minted + owed, mega(49), "registered outsider gets exactly its original-principal bounty");
        assert!(minted > 0, "bounty redemption reaches an actual UTXO");
        send(&mut node, 8, &reveal).await;
        assert_eq!(before - node.collateral(4), mega(100));
        sync(&node, &mut main).await;
        agree(&main, &node, "fresh public mixed court7 registered participant");
        assert!(node.ledger().classes.is_empty() && node.ledger().claims.is_empty(), "candidate confers no execution or Final rights");
        let replay = node.replay().await;
        assert_eq!(
            minted_and_owed(&node, &replay, 8),
            (minted, owed),
            "full IBD independently recreates the outsider's UTXO redemption"
        );
        if pruned {
            // Deleted pre-pruning deltas are intentionally absent on this node.
            // Compare all available deltas and the complete tip/route to full IBD.
            let replay_node = node.on_chain(replay);
            agree(&replay_node, &node, "pruned newcomer7 public court and actual redemption");
        } else {
            node.assert_same(&replay, "IBD newcomer7 public acquisition, own court and actual redemption");
        }
    }
}

#[tokio::test]
async fn flat_v7_native_mature_proof_uses_guaranteed_budget_after_admission_flood() {
    let f = sparse7(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let (bind, class, record) = admitted(&mut net, &f).await;
    let (post, server) = statement(&net, &f, bind, class, &record, Some(0));
    let (_, object) = prepare_flat_model_post_v7(Hash64::from_bytes(class), bind, &record, post).unwrap();
    send(&mut net, 4, &object).await;
    let K::PostFlatModelVectorV7 { post, .. } = &object else { unreachable!() };
    let decoded = decode_flat_model_vector_post_v7(post).unwrap();
    let id = decoded.id(class, bind.as_bytes());
    let FlatFindingV7::Fault(fault) = independent(&mut net, &f, id, class, bind, &server).await else { panic!() };
    let secret = FlatProofSecretV7::new(net.domain, id, net.bond(7), [77; 64], fault.to_bytes().unwrap()).unwrap();
    let seal = secret.seal_object(net.domain, &net.bond(7)).unwrap();
    send(&mut net, 7, &seal).await;
    let row = net.api().unwrap().flat_model_seal_v7(&id, &net.kid(7)).unwrap();
    net.beat_to(row.seal.mature_daa).await;
    let reveal = secret.reveal_object(net.domain, &net.bond(7), &row, net.daa()).unwrap();
    let collateral = net.collateral(4);
    let cards = [2, 3, 5, 6];
    let before: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    let mut objects: Vec<_> = cards.iter().map(|c| (*c, net.route(*c, &object))).collect();
    objects.push((7, net.route(7, &reveal)));
    net.send(objects).await;
    assert_eq!(
        cards.iter().zip(before).filter(|(c, b)| net.slashed(**c) > *b).count(),
        (net.ledger().policy.max_adjudications_per_block - net.ledger().policy.prosecution_reserved_runs()) as usize
    );
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(collateral - net.collateral(4), mega(100));
    let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
    assert_eq!(minted + owed, mega(49));
    let replay = net.replay().await;
    net.assert_same(&replay, "mature mixed7 public proof retains shared guaranteed work and run after admissions");
}
