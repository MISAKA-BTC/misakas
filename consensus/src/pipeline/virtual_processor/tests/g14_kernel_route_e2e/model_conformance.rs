//! Direct TIR on the node: authenticated complete-domain proof, ordinary execution
//! registration, fresh public verification and Final; no model/compiler or eligibility hook.
use super::*;
use kaspa_consensus_core::palw_artifact::{PalwArtifactOperandV1, artifact_leaf_v1, artifact_root_v1};
use kaspa_consensus_core::palw_model_artifact_v2::model_artifact_binding_id_v2;
use kaspa_consensus_core::palw_model_conformance_v2::{ModelConformanceDomainV2, ModelConformancePostV2, model_conformance_charge_v2};
use kaspa_consensus_core::palw_tir_artifact_v1::PalwTirModelInventoryV2;
use misaka_palw_kernel::descriptor::{KernelDescriptorV1, k2_tir_v4_descriptor, k2_tir_v5_descriptor};
use misaka_palw_kernel::element::{SegFaultV1, SegFindingV1, SegMaterialV1, prove_element_v1};
use misaka_palw_kernel::seg::{
    PromptTileOpeningV1, SegmentedCommitmentsV1, TiledJobV1, build_segmented_evidence_v1, prompt_root_of_ids_v1,
    seg_commitments_of_trace_v1,
};
use misaka_palw_kernel::trace::TraceV1;
use misaka_palw_tir::builder::ProgramBuilder;
use misaka_palw_tir::program::{HISTORY_BOUND_V1_SMALL, Ref};
use misaka_palw_tir::{DType, TensorType};

struct Fixture {
    d: KernelDescriptorV1,
    p: TirProgramV1,
    params: MapParams,
    plan: misaka_palw_kernel::VerificationPlanV1,
    pc: ParamCommitmentsV1,
    r: Hash64,
    class: Digest,
    post: ModelConformancePostV2,
}
fn fixture(encoder: bool) -> Fixture {
    let mut b = ProgramBuilder::new(2, HISTORY_BOUND_V1_SMALL);
    let table = b.param("third.party.weights", DType::I8, &[2, 2], false);
    let (ids, count) = if encoder {
        (b.param("input.ids", DType::Idx, &[3], false), Some(b.param("input.count", DType::Idx, &[], false)))
    } else {
        (Ref::Input(0), None)
    };
    let state = if encoder { None } else { Some(b.fixed_state("memory", DType::I8, &[2], -100, 100, false)) };
    let shape = if encoder { vec![6] } else { vec![2] };
    let pre = {
        let mut block = b.block("pre", vec![]);
        let x = block.gather(table, ids, 0, 0);
        let mut x = block.cast(x, DType::I32);
        if encoder {
            x = block.reshape_fixed(x, &shape);
        }
        if let Some(c) = count {
            let c = block.cast(c, DType::I32);
            x = block.add(x, c, DType::I32);
        }
        if let Some(s) = state {
            let y = block.add(x, Ref::State(s), DType::I32);
            let y = block.clamp(y, -100, 100, DType::I8);
            let y = block.state_write(s, y);
            x = block.cast(y, DType::I32);
        }
        block.finish(&[x])
    };
    let post = {
        let mut block = b.block("post", vec![TensorType::fixed(DType::I32, &shape)]);
        let x = block.clamp(Ref::CarryIn(0), i32::MIN as i64, i32::MAX as i64, DType::I32);
        block.commit(x);
        block.finish(&[])
    };
    let mut p = b.finish(pre, vec![], post, 0);
    p.logits_scheme_id = kaspa_consensus_core::palw_step_refute::tiled_logits_scheme_id_v1().as_bytes();
    let d = if encoder { k2_tir_v5_descriptor() } else { k2_tir_v4_descriptor() };
    let plan = plan_for_tir_program_v1(&d, &p, program_root_v1(&p.encode()), if encoder { 1 } else { 3 }).unwrap();
    let params = MapParams { tensors: [((0, None), Tensor::new(DType::I8, vec![2, 2], vec![1, 2, 3, 4]).unwrap())].into() };
    let pc = ParamCommitmentsV1::of_v3(&params);
    let scope = PalwTirModelInventoryV2::new(&d, &p).unwrap();
    let mut operands = Vec::new();
    scope
        .visit_rows(&mut |row| {
            let raw = params.tensors[&(row.param, row.layer)].to_le_bytes();
            operands.push(PalwArtifactOperandV1 {
                tensor_name: p.params[row.param as usize].name.clone(),
                layer: row.layer,
                row_start: row.row_start,
                bytes: raw[row.row_start as usize..(row.row_start + row.len) as usize].to_vec(),
            });
        })
        .unwrap();
    let r = artifact_root_v1(&operands.iter().map(artifact_leaf_v1).collect::<Vec<_>>()).unwrap();
    let reference = ModelConformanceDomainV2::new(&d, &p, &plan).unwrap().reference(&operands).unwrap();
    assert_eq!(reference.pc_root, pc.root());
    let post = ModelConformancePostV2 { version: 2, operands, implementation_roots: [reference.trace_root; 3] };
    let class = single_class_id_v1(d.digest(), &p.encode(), &plan, &pc, VerificationModeV1::OptimisticPublicVerification);
    Fixture { d, p, params, plan, pc, r, class, post }
}
fn config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (cfg, b, p, f) = kernel_config_onboarding();
    let mut params = cfg.params.clone();
    params.palw_panel_free_v1 = Some(PalwPanelFreeFenceV1::interim_v1(ForkActivation::new(1), Vec::new()));
    params.palw_provider_court_v1 = Some(ForkActivation::new(1));
    assert!(params.validate_palw_v2().is_err(), "shipping gates unchanged");
    (Config::new(params), b, p, f)
}
async fn send(net: &mut Net, card: usize, o: &K) {
    let object = net.route(card, o);
    net.send(vec![(card, object)]).await;
}
fn binding(f: &Fixture, r: Hash64) -> (K, Hash64) {
    (
        K::BindModelArtifactV2 {
            descriptor: f.d.digest(),
            program_bytes: f.p.encode(),
            model_inventory_root: r.as_bytes(),
            param_commitments: f.pc.clone(),
        },
        model_artifact_binding_id_v2(f.d.digest(), program_root_v1(&f.p.encode()), r, Hash64::from_bytes(f.pc.root())),
    )
}
fn candidate(f: &Fixture, id: Hash64) -> K {
    K::RegisterModelConformanceClassV2 {
        binding: id.as_bytes(),
        descriptor: f.d.digest(),
        program_bytes: f.p.encode(),
        plan: f.plan.clone(),
        param_commitments: f.pc.clone(),
    }
}
fn register(f: &Fixture) -> K {
    K::RegisterClassV2 {
        mode: VerificationModeV1::OptimisticPublicVerification,
        descriptor: f.d.digest(),
        program_bytes: f.p.encode(),
        plan: f.plan.clone(),
        param_commitments: f.pc.clone(),
    }
}
fn post(f: &Fixture, id: Hash64, p: &ModelConformancePostV2) -> K {
    K::CompleteModelConformanceV2 { class: f.class, binding: id.as_bytes(), proof: borsh::to_vec(p).unwrap() }
}
async fn prepare(net: &mut Net, f: &Fixture) -> Hash64 {
    net.beat_to(1).await;
    let (object, id) = binding(f, f.r);
    send(net, 1, &object).await;
    let daa = net.api().unwrap().model_artifact_binding_v2(&id).unwrap().matures_daa;
    net.beat_to(daa).await;
    send(net, 0, &candidate(f, id)).await;
    assert!(net.ledger().conformance_classes.contains_key(&f.class));
    id
}
struct Material {
    values: Option<Vec<Vec<Tensor>>>,
    c: SegmentedCommitmentsV1,
}
impl SegMaterialV1 for Material {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        if p == 0 { self.values.clone() } else { None }
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (p == 0 && self.values.is_some()).then(|| self.c.position_path(p).1)
    }
    fn commitments(&self, p: u32) -> Option<Vec<Vec<Digest>>> {
        (p == 0 && self.values.is_some()).then(|| self.c.commitments[0].clone())
    }
}
async fn encoder_claim(net: &mut Net, f: &Fixture, nonce: u8, lie: bool) -> (Digest, Material, Vec<u32>) {
    let prompt = vec![0, 1];
    let job = TiledJobV1 {
        class_binding_id: f.class,
        prompt_len: 2,
        prompt_root: prompt_root_of_ids_v1(&prompt),
        max_new_tokens: 0,
        decode: DecodeRuleV1::Greedy,
        nonce: [nonce; 64],
    };
    send(net, 2, &K::PostTiledJob { job: job.clone() }).await;
    assert!(net.ledger().tiled_jobs.contains_key(&job.id()));
    send(net, 3, &K::PostPromptTile { job: job.id(), tile: PromptTileOpeningV1::of(&prompt, 0).unwrap() }).await;
    let binding = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&f.p).unwrap();
    let mut trace = misaka_palw_kernel::seg_encoder::trace_encoder_v1(&f.p, &f.params, &binding, &prompt).unwrap();
    if lie {
        trace.values[0][0][0].data[0] += 1;
    }
    let c = seg_commitments_of_trace_v1(&TraceV1 { values: trace.values.clone(), inputs: Vec::new() });
    let ledger = net.ledger();
    let row = &ledger.classes[&f.class];
    let evidence = build_segmented_evidence_v1(row.header(f.class), &row.descriptor, prompt.len() as u32, &job.prompt_root, &[], &c);
    let claim = KernelClaimV1 { job_id: job.id(), producer_bond: net.kid(4), generated: Vec::new(), evidence_root: evidence.root() };
    let id = claim.id();
    let salt = [nonce; 64];
    send(net, 4, &K::SealClaim { producer: net.kid(4), job: job.id(), seal: claim_seal_v2(&id, &salt) }).await;
    send(
        net,
        4,
        &K::CommitClaimSalted { salt, commit: SaltedCommitV1::Segmented { claim, evidence, segment_roots: c.segment_roots() } },
    )
    .await;
    assert!(net.ledger().claims.contains_key(&id));
    (id, Material { values: Some(trace.values[0].clone()), c }, prompt)
}
fn fresh(net: &Net, id: &Digest) -> misaka_palw_kernel::seg_ledger::SegClaimViewV1 {
    let api = net.api().unwrap();
    let read = api.claim_read_v1(id).unwrap().unwrap();
    assert_eq!(read.ledger_root, api.ledger_root());
    let header: misaka_palw_kernel::evidence::EvidenceHeaderV1 = borsh::from_slice(&read.record_header).unwrap();
    misaka_palw_kernel::seg_ledger::SegmentedClaimRecordV1::from_bytes(&read.public_record)
        .unwrap()
        .view_for_claim(&header, id, &read.producer_bond)
        .unwrap()
}

#[tokio::test]
async fn scoped_model_complete_check_registers_encoder_and_stateful_decoder_without_identity_hooks() {
    kaspa_core::log::try_init_logger("warn");
    for encoder in [true, false] {
        let f = fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let id = prepare(&mut net, &f).await;
        send(&mut net, 2, &register(&f)).await;
        assert!(!net.ledger().classes.contains_key(&f.class), "candidate is not execution permission");
        let fee_before = net.slashed(7);
        send(&mut net, 7, &post(&f, id, &f.post)).await;
        let row = net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).unwrap();
        assert_eq!((row.binding, row.cases), (id, 14));
        assert!(net.slashed(7) > fee_before);
        {
            use kaspa_consensus_core::palw_opv_bootstrap_v1::{OpvClassFactsV1, OpvEligibilityViewV1};
            let route = net.api().unwrap();
            let ledger = route.ledger().unwrap();
            let policy = route.header.opv.unwrap();
            let view = OpvEligibilityViewV1 {
                policy: &policy,
                denied: &[],
                min_effective_bits: 128,
                sampled_gates_reward: false,
                test_eligible: &[],
            };
            let facts = OpvClassFactsV1::of_registration(f.d.digest(), &f.p.encode(), &f.plan, &f.pc);
            assert_eq!(route.model_conformance_eligibility_v2(&ledger, &facts, net.daa(), &view), Ok(id));
            assert!(
                !route.opv_eligible_set_v1(&ledger, net.daa(), &view).contains(&Hash64::from_bytes(f.class)),
                "candidate alone is not a beacon source"
            );
            let denied = [Hash64::from_bytes(f.class)];
            let denied_view = OpvEligibilityViewV1 { denied: &denied, ..view };
            assert_eq!(
                route.model_conformance_eligibility_v2(&ledger, &facts, net.daa(), &denied_view),
                Err(kaspa_consensus_core::palw_opv_bootstrap_v1::OpvIneligibleV1::Denied)
            );
            let mut another = facts;
            another.plan_root = [8; 64];
            assert_eq!(
                route.model_conformance_eligibility_v2(&ledger, &another, net.daa(), &view),
                Err(kaspa_consensus_core::palw_opv_bootstrap_v1::OpvIneligibleV1::ConformanceOfAnotherStatement)
            );
        }
        send(&mut net, 2, &register(&f)).await;
        assert!(net.ledger().classes.contains_key(&f.class));
        assert!(!net.ledger().conformance_classes.contains_key(&f.class));
        assert!(!net.ledger().attested_artifacts.contains(&f.pc.root()));
        let extras = net.chain.tip_state().1;
        assert!(extras.class(&Hash64::from_bytes(f.class)).is_none(), "no V2 decoder identity was manufactured");
        let replay = net.replay().await;
        net.assert_same(&replay, "scoped conformance and ordinary registration replay");
        if encoder {
            let (honest, material, tokens) = encoder_claim(&mut net, &f, 11, false).await;
            let view = fresh(&net, &honest);
            let params = f.params.clone();
            let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
            let empty = Material { values: None, c: material.c.clone() };
            assert_eq!(
                misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &empty, &art, &tokens).finding,
                SegFindingV1::Clean
            );
            let (false_id, bad, tokens) = encoder_claim(&mut net, &f, 12, true).await;
            let view = fresh(&net, &false_id);
            let fault = prove_element_v1(&view.context(), &bad, &art, &tokens, (0, 0, 0), 0).unwrap();
            let accuser = net.kid(7);
            send(
                &mut net,
                7,
                &K::FileProof { accuser, claim: false_id, proof: ProsecutionV1::Segmented(SegFaultV1::Element(fault).to_bytes()) },
            )
            .await;
            assert!(net.ledger().claims[&false_id].convicted);
            let at = net.daa() + 80;
            net.beat_to(at).await;
            assert!(matches!(net.claim_state(&honest), ClaimStateV1::Final { .. }));
            let replay = net.replay().await;
            net.assert_same(&replay, "fresh outsider conviction and independent honest Final replay");
        }
    }
}

#[tokio::test]
async fn scoped_model_complete_check_cannot_be_poisoned_by_an_earlier_false_source_root() {
    let f = fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    let (bad, bad_id) = binding(&f, Hash64::from_bytes([42; 64]));
    send(&mut net, 0, &bad).await;
    let (good, good_id) = binding(&f, f.r);
    send(&mut net, 1, &good).await;
    let at = net.api().unwrap().model_artifact_binding_v2(&good_id).unwrap().matures_daa;
    net.beat_to(at).await;
    send(&mut net, 0, &candidate(&f, bad_id)).await;
    assert_eq!(net.api().unwrap().model_artifact_candidate_binding_v2(&Hash64::from_bytes(f.class)), Some(bad_id));
    send(&mut net, 7, &post(&f, bad_id, &f.post)).await;
    assert!(net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).is_none());
    send(&mut net, 7, &post(&f, good_id, &f.post)).await;
    assert_eq!(net.api().unwrap().model_artifact_candidate_binding_v2(&Hash64::from_bytes(f.class)), Some(good_id));
    send(&mut net, 2, &register(&f)).await;
    assert!(net.ledger().classes.contains_key(&f.class));
    let replay = net.replay().await;
    net.assert_same(&replay, "genesis/registration order cannot poison a matching verified model");
}

#[tokio::test]
async fn scoped_model_complete_check_wrong_roles_and_floods_spend_only_attackers_budget() {
    let f = fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let id = prepare(&mut net, &f).await;
    let before = net.collateral(1);
    let mut wrong = f.post.clone();
    wrong.implementation_roots[2] = [9; 64];
    send(&mut net, 7, &post(&f, id, &wrong)).await;
    assert!(net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).is_none());
    assert_eq!(net.collateral(1), before);
    let junk = vec![2, 0, 255, 255, 255, 255];
    let work = model_conformance_charge_v2(f.p.encode().len(), junk.len()).unwrap();
    let policy = net.ledger().policy;
    let cards = [2usize, 3, 4, 5, 6, 7];
    let old: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    let mut objects = Vec::new();
    for card in cards {
        let object = net.route(card, &K::CompleteModelConformanceV2 { class: f.class, binding: id.as_bytes(), proof: junk.clone() });
        objects.push((card, object));
    }
    net.send(objects).await;
    let paid: u64 = cards.iter().zip(old).map(|(c, old)| net.slashed(*c) - old).sum();
    assert_eq!(paid, 2 * policy.dismissal_fee_v1(work));
    assert_eq!(net.collateral(1), before);
    assert!(net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).is_none());
    send(&mut net, 7, &post(&f, id, &f.post)).await;
    assert!(net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).is_some());
    let fee = net.slashed(7);
    send(&mut net, 7, &post(&f, id, &f.post)).await;
    assert_eq!(net.slashed(7), fee, "duplicate is not replayed or charged");
    let replay = net.replay().await;
    net.assert_same(&replay, "bounded invalid conformance flood and honest recovery replay");
}

// These vocabularies/source inventories exceed finite-domain enumeration. The on-chain
// court receives one bounded dependency line, never all weights or all input words.
fn vector_fixture(encoder: bool) -> Fixture {
    let mut f = fixture(encoder);
    let tokens = 2048;
    f.p.token_bound = tokens;
    let width = if encoder { 2 } else { tokens };
    f.p.params[0].shape = vec![tokens, width];
    if !encoder {
        f.p.states[0].shape = vec![width];
        for b in &mut f.p.blocks {
            for n in &mut b.nodes {
                n.out = TensorType::fixed(n.out.dtype, &[width]);
            }
            for t in &mut b.carry_in {
                *t = TensorType::fixed(t.dtype, &[width]);
            }
        }
    }
    f.params.tensors.insert(
        (0, None),
        Tensor::new(
            DType::I8,
            vec![tokens as usize, width as usize],
            (0..tokens as usize * width as usize).map(|n| (n % 4 + 1) as i128).collect(),
        )
        .unwrap(),
    );
    f.pc = ParamCommitmentsV1::of_v3(&f.params);
    f.plan = plan_for_tir_program_v1(&f.d, &f.p, program_root_v1(&f.p.encode()), if encoder { 1 } else { 3 }).unwrap();
    f.class = single_class_id_v1(f.d.digest(), &f.p.encode(), &f.plan, &f.pc, VerificationModeV1::OptimisticPublicVerification);
    let scope = PalwTirModelInventoryV2::new(&f.d, &f.p).unwrap();
    let raw = f.params.tensors[&(0, None)].to_le_bytes();
    let mut leaves = Vec::new();
    scope
        .visit_rows(&mut |r| {
            leaves.push(artifact_leaf_v1(&PalwArtifactOperandV1 {
                tensor_name: f.p.params[r.param as usize].name.clone(),
                layer: r.layer,
                row_start: r.row_start,
                bytes: raw[r.row_start as usize..(r.row_start + r.len) as usize].to_vec(),
            }))
        })
        .unwrap();
    f.r = artifact_root_v1(&leaves).unwrap();
    assert!(ModelConformanceDomainV2::new(&f.d, &f.p, &f.plan).unwrap_err().contains("input cases"));
    f
}
fn vector_post(
    net: &Net,
    f: &Fixture,
    binding: Hash64,
    prompt: Vec<u32>,
    lie: bool,
) -> (kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2, Material) {
    use kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2;
    let encoder = f.d.digest() == k2_tir_v5_descriptor().digest();
    let mut trace = if encoder {
        let b = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&f.p).unwrap();
        misaka_palw_kernel::seg_encoder::trace_encoder_v1(&f.p, &f.params, &b, &prompt).unwrap()
    } else {
        misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &prompt).unwrap()
    };
    if lie {
        trace.values[0][0][0].data[0] += 1;
    }
    let generated = if encoder { Vec::new() } else { vec![3] };
    let c = seg_commitments_of_trace_v1(&TraceV1 { values: trace.values.clone(), inputs: Vec::new() });
    let ledger = net.ledger();
    let row = &ledger.conformance_classes[&f.class].1;
    let evidence = build_segmented_evidence_v1(
        row.header(f.class),
        &row.descriptor,
        prompt.len() as u32,
        &prompt_root_of_ids_v1(&prompt),
        if generated.is_empty() { &[] } else { &generated[..generated.len() - 1] },
        &c,
    );
    let mut post = ModelVectorPostV2 {
        version: 2,
        prompt,
        max_new_tokens: generated.len() as u32,
        generated,
        evidence,
        segment_roots: c.segment_roots(),
        implementation_roots: [[0; 64]; 3],
    };
    post.implementation_roots = [post.trace_root(f.class, binding.as_bytes()); 3];
    (post, Material { values: Some(trace.values[0].clone()), c })
}
async fn send_vector(
    net: &mut Net,
    f: &Fixture,
    binding: Hash64,
    card: usize,
    post: &kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2,
) -> Hash64 {
    let id = post.id(f.class, binding.as_bytes());
    send(net, card, &K::PostModelVectorV2 { class: f.class, binding: binding.as_bytes(), post: borsh::to_vec(post).unwrap() }).await;
    id
}

#[tokio::test]
async fn model_vector_new_large_vocab_candidates_have_public_courts_without_a_final_or_execution_rights() {
    for encoder in [true, false] {
        let f = vector_fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let binding = prepare(&mut net, &f).await;
        let source_reserved = net.chain.tip_state().1.model_artifact_reserved_at_v2(&net.bond(1), net.daa());
        let (honest, material) = vector_post(&net, &f, binding, vec![0, 1], false);
        let honest_id = send_vector(&mut net, &f, binding, 4, &honest).await;
        let route = net.api().unwrap();
        let view = route.model_vector_view_v2(&honest_id).unwrap();
        let params = f.params.clone();
        let art = move |j: u16, l: Option<u16>| params.tensors.get(&(j, l)).cloned();
        let empty = Material { values: None, c: material.c.clone() };
        assert_eq!(
            misaka_palw_kernel::seg_detect::reexecute_claim_v1(&view.context(), &empty, &art, &honest.prompt).finding,
            SegFindingV1::Clean,
            "fresh verifier uses its own model and the public statement"
        );
        let (bad, bad_material) = vector_post(&net, &f, binding, vec![0, 1], true);
        let bad_id = send_vector(&mut net, &f, binding, 4, &bad).await;
        let route = net.api().unwrap();
        let view = route.model_vector_view_v2(&bad_id).unwrap();
        let fault = prove_element_v1(&view.context(), &bad_material, &art, &bad.prompt, (0, 0, 0), 0).unwrap();
        let proof = SegFaultV1::Element(fault).to_bytes();
        assert!(proof.len() < 32 << 10, "large source is prosecuted with a bounded dependency line");
        assert_eq!(kaspa_consensus_core::palw_model_vector_v2::decode_model_vector_fault_v2(&proof).unwrap().to_bytes(), proof);
        let before = net.collateral(4);
        let fee = net.slashed(7);
        // A valid proof cannot be borrowed by the honest vector. A self-operator bounty is refused.
        send(&mut net, 7, &K::RefuteModelVectorV2 { vector: honest_id.as_bytes(), proof: proof.clone() }).await;
        assert!(net.slashed(7) > fee);
        assert!(!net.api().unwrap().model_vector_header_v2(&honest_id).unwrap().refuted);
        send(&mut net, 4, &K::RefuteModelVectorV2 { vector: bad_id.as_bytes(), proof: proof.clone() }).await;
        assert_eq!(net.collateral(4), before);
        send(&mut net, 7, &K::RefuteModelVectorV2 { vector: bad_id.as_bytes(), proof: proof.clone() }).await;
        assert!(net.api().unwrap().model_vector_header_v2(&bad_id).unwrap().refuted);
        let reward = kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1
            * kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_CHALLENGER_REWARD_PERMILLE_V1
            / 1000;
        assert_eq!(net.owed(7), reward, "only 49% of the collected vector slash is payable");
        assert_eq!(before - net.collateral(4), kaspa_consensus_core::palw_onboarding_v1::PALW_ONBOARDING_BINDING_RESERVATION_SOMPI_V1);
        let after = net.collateral(4);
        send(&mut net, 6, &K::RefuteModelVectorV2 { vector: bad_id.as_bytes(), proof }).await;
        assert_eq!(net.collateral(4), after, "one conviction/collected slash");
        assert_eq!(net.chain.tip_state().1.model_artifact_reserved_at_v2(&net.bond(1), net.daa()), source_reserved);
        assert!(net.api().unwrap().model_conformance_v2(&Hash64::from_bytes(f.class)).is_none());
        send(&mut net, 2, &register(&f)).await;
        let ledger = net.ledger();
        assert!(!ledger.classes.contains_key(&f.class));
        assert!(ledger.jobs.is_empty() && ledger.tiled_jobs.is_empty() && ledger.claims.is_empty());
        assert!(ledger.opv.classes.is_empty() && ledger.opv.claims.is_empty());
        assert_eq!(minted_and_owed(&net, &net.chain, 7), (reward, 0), "later coinbase redeems the bounty exactly once");
        let replay = net.replay().await;
        net.assert_same(&replay, "large-domain candidate vector court without existing jobs, claims or Finals");
        assert_eq!(minted_and_owed(&net, &replay, 7), (reward, 0), "independent node reproduces the unspent redemption");
    }
}

#[tokio::test]
async fn model_vector_catalog_shares_capital_and_keeps_its_bound_after_clocked_release() {
    use kaspa_consensus_core::palw_state_v2::{palw_bond_backs_live_duty_v1, palw_bond_backs_live_duty_v2};
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let mut last = None;
    for n in 0..8 {
        let (p, _) = vector_post(&net, &f, binding, vec![n, n + 1], false);
        let id = send_vector(&mut net, &f, binding, 4, &p).await;
        assert!(net.api().unwrap().model_vector_header_v2(&id).is_some());
        last = Some(id);
    }
    let (p, _) = vector_post(&net, &f, binding, vec![8, 9], false);
    let no = send_vector(&mut net, &f, binding, 4, &p).await;
    assert!(net.api().unwrap().model_vector_header_v2(&no).is_none());
    let state = net.chain.tip_state().1;
    assert_eq!(state.model_vector_reserved_at_v2(&net.bond(4), net.daa()), mega(800) as u128);
    assert!(
        kaspa_consensus_core::palw_state_v2::palw_bond_committed_v1(&state, &net.bond(4), net.daa(), None, 10) >= mega(800) as u128,
        "ordinary free-capital checks read the vector reservations"
    );
    assert!(palw_bond_backs_live_duty_v1(&state, &net.bond(4), net.daa(), None));
    assert!(palw_bond_backs_live_duty_v2(&state, &net.bond(4), net.daa(), None, 10));
    let until = net.api().unwrap().model_vector_header_v2(&last.unwrap()).unwrap().liability_until;
    net.beat_to(until).await;
    assert_eq!(net.chain.tip_state().1.model_vector_reserved_at_v2(&net.bond(4), net.daa()), 0);
    assert_eq!(net.api().unwrap().model_vectors_of_v2(&net.bond(4)).len(), 8, "expired records retain the bounded catalog");
    let replay = net.replay().await;
    net.assert_same(&replay, "vector capital horizon and catalog replay");
}

#[tokio::test]
async fn model_vector_wrong_roots_scope_and_invalid_refutations_cost_only_the_actor() {
    use kaspa_consensus_core::palw_model_artifact_v2::model_artifact_work_v2;
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (good, _) = vector_post(&net, &f, binding, vec![0, 1], false);
    let mut wrong = good.clone();
    wrong.implementation_roots[2][0] ^= 1;
    let before = net.collateral(1);
    let fee = net.slashed(4);
    let id = send_vector(&mut net, &f, binding, 4, &wrong).await;
    assert!(net.api().unwrap().model_vector_header_v2(&id).is_none());
    assert!(net.slashed(4) > fee);
    assert_eq!(net.collateral(1), before);
    wrong = good.clone();
    wrong.evidence.header.plan_root[0] ^= 1;
    wrong.implementation_roots = [wrong.trace_root(f.class, binding.as_bytes()); 3];
    let id = send_vector(&mut net, &f, binding, 4, &wrong).await;
    assert!(net.api().unwrap().model_vector_header_v2(&id).is_none());
    let id = send_vector(&mut net, &f, binding, 4, &good).await;
    let before_duplicate = net.slashed(4);
    let held = net.chain.tip_state().1.model_vector_reserved_at_v2(&net.bond(4), net.daa());
    send_vector(&mut net, &f, binding, 4, &good).await;
    assert!(net.slashed(4) > before_duplicate, "a judged duplicate cannot consume free admission CPU");
    assert_eq!(net.chain.tip_state().1.model_vector_reserved_at_v2(&net.bond(4), net.daa()), held);
    let h = net.api().unwrap().model_vector_header_v2(&id).unwrap();
    let junk = vec![0, 255, 255, 255, 255];
    let work =
        model_artifact_work_v2(h.program_bytes_len as usize, junk.len() + h.post_bytes_len as usize).unwrap() + h.max_court_work;
    let expected = net.ledger().policy.dismissal_fee_v1(work);
    let mut objects = Vec::new();
    let cards = [2, 3, 5, 6, 7];
    let fees: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    for card in cards {
        objects.push((card, net.route(card, &K::RefuteModelVectorV2 { vector: id.as_bytes(), proof: junk.clone() })));
    }
    net.send(objects).await;
    let deltas: Vec<_> = cards.into_iter().zip(fees).map(|(card, fee)| net.slashed(card) - fee).collect();
    assert!(deltas.iter().all(|fee| *fee == 0 || *fee == expected));
    assert_eq!(
        deltas.iter().filter(|fee| **fee == expected).count(),
        net.ledger().policy.max_adjudications_per_block as usize,
        "only judged filings consume fees; the shared block cap dismisses the rest"
    );
    assert!(!net.api().unwrap().model_vector_header_v2(&id).unwrap().refuted);
    assert_eq!(net.collateral(1), before);
    let replay = net.replay().await;
    net.assert_same(&replay, "invalid vector court flood and scope refusal replay");
}

fn node_unit(node: u16) -> kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorUnitV2 {
    kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorUnitV2 { position: 0, occurrence: 0, node, axis: 2, line: 0, tile: 0 }
}
async fn ask_vector(
    net: &mut Net,
    vector: Hash64,
    card: usize,
    slot: u8,
    u: &kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorUnitV2,
) -> Option<Hash64> {
    send(
        net,
        card,
        &K::DemandModelVectorV2 {
            vector: vector.as_bytes(),
            slot,
            position: u.position,
            occurrence: u.occurrence,
            node: u.node,
            axis: u.axis,
            line: u.line,
            tile: u.tile,
        },
    )
    .await;
    net.api().unwrap().model_vector_demand_slot_v2(&net.bond(card), slot)
}
fn vector_answer(
    material: &Material,
    u: &kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorUnitV2,
) -> kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorAnswerV2 {
    let opening = misaka_palw_kernel::seg::position_node_opening_v1(
        u.position,
        &material.c.commitments[u.position as usize],
        material.c.position_path(u.position).1,
        u.occurrence,
        u.node,
    )
    .unwrap();
    let tile = if u.axis == 2 {
        None
    } else {
        let values = material.values.as_ref().unwrap();
        Some(
            kaspa_consensus_core::palw_onboarding_v1::ArtifactTileOpeningV3::new(
                misaka_palw_kernel::merkle3::LeafOpeningV3::of(
                    &values[u.occurrence as usize][u.node as usize],
                    u.axis,
                    u.line,
                    u.tile,
                )
                .unwrap(),
            )
            .unwrap(),
        )
    };
    kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorAnswerV2 { opening, tile }
}

/// Read the answer's carrier back from the selected chain, and authenticate it to
/// the node's public receipt. The verifier never receives the sender's local buffer.
fn vector_answer_from_blocks(
    net: &Net,
    vector: Hash64,
    unit: &kaspa_consensus_core::palw_model_vector_da_v2::ModelVectorUnitV2,
    demand: Hash64,
) -> Vec<u8> {
    use kaspa_consensus_core::palw_lifecycle_objects_v2::PalwLifecycleTxPayloadV2;
    let receipt = net.api().unwrap().model_vector_served_v2(&vector, unit).unwrap();
    for block in chain_blocks(&net.chain, net.chain.sink()) {
        for tx in block.transactions.iter() {
            if tx.subnetwork_id != kaspa_consensus_core::subnets::SUBNETWORK_ID_PALW_LIFECYCLE {
                continue;
            }
            let Ok(payload) = borsh::from_slice::<PalwLifecycleTxPayloadV2>(&tx.payload) else { continue };
            let Obj::KernelRouteV1 { bytes, .. } = payload.object else { continue };
            let Ok(K::AnswerModelVectorV2 { demand: id, answer }) = K::decode(&bytes) else { continue };
            if id == demand.as_bytes()
                && misaka_palw_kernel::hash::object_id(b"misaka-palw/model-vector/answer/v2", &answer.as_slice())
                    == receipt.answer_root
            {
                return answer;
            }
        }
    }
    panic!("the public served receipt has no matching answer in the selected chain's carrier bodies")
}

/// This source holds decoded public replies, verifier-own re-execution, and placeholders for withheld values.
/// It has no access to the producer's trace or the producer's commitment collection.
struct VectorReplies {
    values: Vec<Vec<Tensor>>,
    commitments: Vec<Vec<Digest>>,
    siblings: Vec<Digest>,
}
impl SegMaterialV1 for VectorReplies {
    fn position(&self, p: u32) -> Option<Vec<Vec<Tensor>>> {
        (p == 0).then(|| self.values.clone())
    }
    fn position_siblings(&self, p: u32) -> Option<Vec<Digest>> {
        (p == 0).then(|| self.siblings.clone())
    }
    fn commitments(&self, p: u32) -> Option<Vec<Vec<Digest>>> {
        (p == 0).then(|| self.commitments.clone())
    }
}

#[tokio::test]
async fn model_vector_da_public_replies_enable_a_fresh_whole_value_conviction_without_model_disclosure() {
    use kaspa_consensus_core::palw_model_vector_da_v2::*;
    for (encoder, gather_fault) in [(true, false), (false, false), (false, true)] {
        let f = vector_fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let binding = prepare(&mut net, &f).await;
        let (mut post, mut producer) = vector_post(&net, &f, binding, vec![0, 1], false);
        let mask = misaka_palw_kernel::seg_scope::model_vector_withheld_mask_v2(&f.p);
        let w = misaka_palw_kernel::trace::WiringV1::new(&f.p).unwrap();
        let target = if gather_fault {
            assert!(!misaka_palw_kernel::seg_scope::seg_withheld_mask_v1(&f.p)[0][0]);
            assert!(mask[0][0], "4 MiB source Gather has a bounded dependency court");
            (0, 0)
        } else {
            w.occurrences
                .iter()
                .enumerate()
                .find_map(|(s, (b, _))| {
                    f.p.blocks[*b as usize].nodes.iter().enumerate().find_map(|(n, node)| {
                        (mask[s][n] && node.inputs.iter().any(|r| matches!(r, Ref::Node(k) if mask[s][*k as usize]))).then_some((s, n))
                    })
                })
                .expect("fixture keeps a whole-value relation with a withheld committed operand")
        };
        // Exercise both an earlier bounded dependency relation and a later
        // relation with an authenticated whole committed operand.
        producer.values.as_mut().unwrap()[target.0][target.1].data[0] += 1;
        let mut commitments = producer.c.commitments.clone();
        commitments[0] = producer
            .values
            .as_ref()
            .unwrap()
            .iter()
            .map(|o| o.iter().map(misaka_palw_kernel::merkle3::tensor_commitment_v3).collect())
            .collect();
        producer.c = SegmentedCommitmentsV1::new(commitments);
        post.segment_roots = producer.c.segment_roots();
        post.evidence = build_segmented_evidence_v1(
            post.evidence.header.clone(),
            &f.d,
            post.prompt.len() as u32,
            &prompt_root_of_ids_v1(&post.prompt),
            if post.generated.is_empty() { &[] } else { &post.generated[..post.generated.len() - 1] },
            &producer.c,
        );
        post.implementation_roots = [post.trace_root(f.class, binding.as_bytes()); 3];
        let vector = send_vector(&mut net, &f, binding, 4, &post).await;
        let mut forbidden = ModelVectorUnitV2 { occurrence: target.0 as u16, node: target.1 as u16, ..node_unit(0) };
        forbidden.axis = 0;
        let before = net.slashed(7);
        assert!(ask_vector(&mut net, vector, 7, 0, &forbidden).await.is_none());
        let raw_model =
            misaka_palw_kernel::scope::court_model_bytes_mask_v1(&f.p, misaka_palw_kernel::seg_scope::seg_model_params_v1(&f.p));
        assert!(raw_model[0][0], "Gather's copy of model rows is always model bytes, even beyond the WholeValue price cap");
        let mut raw_request = node_unit(0);
        raw_request.axis = 0;
        assert!(ask_vector(&mut net, vector, 7, 0, &raw_request).await.is_none());
        assert!(net.slashed(7) > before, "demanding model-derived bytes is rejected at the actor's cost");
        let own_trace = if encoder {
            let b = misaka_palw_kernel::seg_encoder::encoder_binding_v1(&f.p).unwrap();
            misaka_palw_kernel::seg_encoder::trace_encoder_v1(&f.p, &f.params, &b, &post.prompt).unwrap()
        } else {
            misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &post.prompt).unwrap()
        };
        let model_dependent =
            misaka_palw_kernel::scope::court_model_dependent_mask_v1(&f.p, misaka_palw_kernel::seg_scope::seg_model_params_v1(&f.p));
        let mut public = VectorReplies { values: Vec::new(), commitments: Vec::new(), siblings: Vec::new() };
        for (s, (block, _)) in w.occurrences.iter().enumerate() {
            let mut values = Vec::new();
            let mut commitments = Vec::new();
            for (n, node) in f.p.blocks[*block as usize].nodes.iter().enumerate() {
                let u = ModelVectorUnitV2 { occurrence: s as u16, node: n as u16, ..node_unit(0) };
                let demand = ask_vector(&mut net, vector, 7, 0, &u).await.unwrap();
                let wire = borsh::to_vec(&vector_answer(&producer, &u)).unwrap();
                send(&mut net, 4, &K::AnswerModelVectorV2 { demand: demand.as_bytes(), answer: wire.clone() }).await;
                assert!(net.api().unwrap().model_vector_demand_v2(&demand).unwrap().settled);
                let wire = vector_answer_from_blocks(&net, vector, &u, demand);
                let a = decode_model_vector_answer_v2(&wire).unwrap();
                assert!(a.tile.is_none());
                commitments.push(a.opening.commitment);
                public.siblings = a.opening.position_siblings;
                let shape = node.out.resolve(w.h(s as u16, 0));
                let mut value = Tensor::zeros(node.out.dtype, &shape);
                if !mask[s][n] && model_dependent[s][n] {
                    // Matching values can be supplied by the verifier's own model;
                    // no model-derived tensor bytes are acquired from the poster here.
                    value = own_trace.values[0][s][n].clone();
                } else if !mask[s][n] {
                    let u = ModelVectorUnitV2 { axis: 0, ..u };
                    assert!(misaka_palw_kernel::merkle3::LayoutV3::of(&shape).len <= 4096);
                    let demand = ask_vector(&mut net, vector, 7, 0, &u).await.unwrap();
                    let wire = borsh::to_vec(&vector_answer(&producer, &u)).unwrap();
                    send(&mut net, 4, &K::AnswerModelVectorV2 { demand: demand.as_bytes(), answer: wire.clone() }).await;
                    let wire = vector_answer_from_blocks(&net, vector, &u, demand);
                    let a = decode_model_vector_answer_v2(&wire).unwrap();
                    for (i, x) in a.tile.unwrap().leaf().elements().unwrap() {
                        value.data[i as usize] = x;
                    }
                }
                values.push(value);
            }
            public.values.push(values);
            public.commitments.push(commitments);
        }
        assert_eq!(net.chain.tip_state().1.model_vector_demand_reserved_v2(&net.bond(7)), 0);
        assert_eq!(net.api().unwrap().model_vector_open_demands_v2(&vector), 0);
        let route = net.api().unwrap();
        let view = route.model_vector_view_v2(&vector).unwrap();
        let params = f.params.clone();
        let own_model = move |j, l| params.tensors.get(&(j, l)).cloned();
        let fault = match misaka_palw_kernel::element::check_model_vector_positions_v2(
            &view.context(),
            &public,
            &own_model,
            &post.prompt,
            &[0],
        ) {
            SegFindingV1::Fault(f) => *f,
            other => panic!("public replies plus the verifier's own model must construct a court: {other:?}"),
        };
        assert!(matches!(&fault, SegFaultV1::WholeValue(f) if (f.occurrence as usize, f.node as usize) == target));
        if !gather_fault {
            assert!(matches!(&fault, SegFaultV1::WholeValue(f) if f.inputs.iter().any(|i| i.whole.is_some())));
        } else {
            let SegFaultV1::WholeValue(ref whole) = fault else { unreachable!() };
            assert!(whole.inputs[0].whole.is_none());
            assert_eq!(whole.inputs[0].leaves.len(), 1, "court opens one selected weight row, not 2048 rows");
            assert!(
                matches!(misaka_palw_kernel::element::verify_seg_fault_v1(&view.context(), &fault), Err(_)),
                "the frozen claim court does not acquire candidate scope"
            );
            assert!(misaka_palw_kernel::element::verify_model_vector_fault_v2(&view.context(), &fault).is_ok());
            let mut tampered = whole.clone();
            tampered.inputs[0].leaves[0].values[0] += 1;
            let invalid = SegFaultV1::WholeValue(tampered).to_bytes();
            let before = net.collateral(4);
            let fee_before = net.slashed(6);
            send(&mut net, 6, &K::RefuteModelVectorV2 { vector: vector.as_bytes(), proof: invalid }).await;
            assert!(!net.api().unwrap().model_vector_header_v2(&vector).unwrap().refuted);
            assert_eq!(net.collateral(4), before);
            assert!(net.slashed(6) > fee_before, "unbound weight bytes spend the challenger's fee");
            let count = w.occurrences.iter().map(|(b, _)| f.p.blocks[*b as usize].nodes.len() as u64).sum();
            let (opening_bound, work_bound) = misaka_palw_kernel::seg_scope::model_vector_whole_court_cost_v2(&f.p, 0, 0, count);
            assert!(net.api().unwrap().model_vector_header_v2(&vector).unwrap().max_court_work >= work_bound);
            let mut oversized = whole.clone();
            let leaf = oversized.inputs[0].leaves[0].clone();
            while SegFaultV1::WholeValue(oversized.clone()).to_bytes().len() as u64 <= opening_bound {
                oversized.inputs[0].leaves.push(leaf.clone());
            }
            let oversized = SegFaultV1::WholeValue(oversized);
            assert!(misaka_palw_kernel::element::verify_model_vector_fault_v2(&view.context(), &oversized).is_err());
            let before = net.collateral(4);
            let fee_before = net.slashed(6);
            send(&mut net, 6, &K::RefuteModelVectorV2 { vector: vector.as_bytes(), proof: oversized.to_bytes() }).await;
            assert_eq!(net.collateral(4), before);
            assert!(net.slashed(6) > fee_before);
            let mut no_wire = whole.clone();
            no_wire.inputs[0].leaves[0].dtype = 255;
            assert!(
                misaka_palw_kernel::element::verify_model_vector_fault_v2(&view.context(), &SegFaultV1::WholeValue(no_wire)).is_err()
            );

            // An honest control acquires its node opening from a separate
            // statement's authenticated public answer. Its parameter witness
            // comes from the same verifier-owned model, not from the producer.
            let (honest_post, honest_material) = vector_post(&net, &f, binding, post.prompt.clone(), false);
            let honest = send_vector(&mut net, &f, binding, 4, &honest_post).await;
            let unit = node_unit(0);
            let demand = ask_vector(&mut net, honest, 7, 0, &unit).await.unwrap();
            send(
                &mut net,
                4,
                &K::AnswerModelVectorV2 {
                    demand: demand.as_bytes(),
                    answer: borsh::to_vec(&vector_answer(&honest_material, &unit)).unwrap(),
                },
            )
            .await;
            let wire = vector_answer_from_blocks(&net, honest, &unit, demand);
            let answer = decode_model_vector_answer_v2(&wire).unwrap();
            assert!(answer.tile.is_none());
            let mut honest_proof = whole.clone();
            honest_proof.output = answer.opening;
            let honest_proof = SegFaultV1::WholeValue(honest_proof);
            let honest_view = net.api().unwrap().model_vector_view_v2(&honest).unwrap();
            assert_eq!(
                misaka_palw_kernel::element::verify_model_vector_fault_v2(&honest_view.context(), &honest_proof),
                Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
            );
            let before = net.collateral(4);
            let fee_before = net.slashed(6);
            send(&mut net, 6, &K::RefuteModelVectorV2 { vector: honest.as_bytes(), proof: honest_proof.to_bytes() }).await;
            assert_eq!(net.collateral(4), before);
            assert!(!net.api().unwrap().model_vector_header_v2(&honest).unwrap().refuted);
            assert!(net.slashed(6) > fee_before);
            println!(
                "candidate Gather: source_bytes={} opening_bound={} court_work={} proof_bytes={} selected_parameter_leaves={}",
                f.params.tensors[&(0, None)].to_le_bytes().len(),
                opening_bound,
                work_bound,
                fault.to_bytes().len(),
                whole.inputs[0].leaves.len()
            );
        }
        let proof = fault.to_bytes();
        assert!(proof.len() < 32 << 10);
        assert_eq!(kaspa_consensus_core::palw_model_vector_v2::decode_model_vector_fault_v2(&proof).unwrap(), fault);
        let before = net.collateral(4);
        send(&mut net, 7, &K::RefuteModelVectorV2 { vector: vector.as_bytes(), proof }).await;
        assert!(net.api().unwrap().model_vector_header_v2(&vector).unwrap().refuted);
        assert_eq!(before - net.collateral(4), mega(100));
        assert_eq!(net.owed(7), mega(49));
        assert!(net.ledger().claims.is_empty() && net.ledger().classes.is_empty());
        let replay = net.replay().await;
        net.assert_same(&replay, "public candidate DA, verifier-own-model whole court and once-only slash replay");
    }
}

#[tokio::test]
async fn model_vector_da_deadlines_hold_capital_and_responses_survive_a_full_court_budget() {
    use kaspa_consensus_core::palw_state_v2::{palw_bond_backs_live_duty_v1, palw_bond_backs_live_duty_v2};
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (post, material) = vector_post(&net, &f, binding, vec![0, 1], false);
    let vector = send_vector(&mut net, &f, binding, 4, &post).await;
    let source = net.collateral(1);
    assert!(ask_vector(&mut net, vector, 4, 0, &node_unit(0)).await.is_none());
    let mut demands = Vec::new();
    for slot in 0..4 {
        demands.push(ask_vector(&mut net, vector, 7, slot, &node_unit(slot as u16)).await.unwrap());
    }
    assert!(ask_vector(&mut net, vector, 7, 4, &node_unit(0)).await.is_none());
    let other = ask_vector(&mut net, vector, 6, 0, &node_unit(0)).await.unwrap();
    assert_eq!(net.api().unwrap().model_vector_open_demands_v2(&vector), 5);
    let deposit = net.api().unwrap().model_vector_demand_v2(&demands[0]).unwrap().bond;
    let state = net.chain.tip_state().1;
    assert_eq!(state.model_vector_demand_reserved_v2(&net.bond(7)), deposit as u128 * 4);
    assert!(palw_bond_backs_live_duty_v1(&state, &net.bond(7), net.daa(), None));
    assert!(palw_bond_backs_live_duty_v2(&state, &net.bond(7), net.daa(), None, 10));
    let valid = borsh::to_vec(&vector_answer(&material, &node_unit(0))).unwrap();
    let mut wrong = valid.clone();
    wrong[8] ^= 1;
    let fee = net.slashed(2);
    send(&mut net, 2, &K::AnswerModelVectorV2 { demand: demands[0].as_bytes(), answer: wrong }).await;
    assert!(net.slashed(2) > fee);
    assert!(!net.api().unwrap().model_vector_demand_v2(&demands[0]).unwrap().settled);
    let before = net.collateral(4);
    let hostile_cards = [2, 3, 5, 6];
    let hostile_fees: Vec<_> = hostile_cards.iter().map(|c| net.slashed(*c)).collect();
    let mut objects = Vec::new();
    for card in hostile_cards {
        objects.push((card, net.route(card, &K::RefuteModelVectorV2 { vector: vector.as_bytes(), proof: vec![0] })));
    }
    objects.push((4, net.route(4, &K::AnswerModelVectorV2 { demand: demands[0].as_bytes(), answer: valid.clone() })));
    net.send(objects).await;
    assert_eq!(
        hostile_cards.into_iter().zip(hostile_fees).filter(|(card, fee)| net.slashed(*card) > *fee).count(),
        net.ledger().policy.max_adjudications_per_block as usize,
        "all shared court runs were actually consumed before accepting the bounded answer",
    );
    assert!(
        net.api().unwrap().model_vector_demand_v2(&demands[0]).unwrap().settled,
        "answer has no shared adjudication-run dependency"
    );
    assert_eq!(net.collateral(4), before);
    assert_eq!(net.owed(4), deposit, "serving is funded by the requester's collected deposit");
    assert_eq!(net.api().unwrap().model_vector_open_demands_v2(&vector), 4);
    // The second requester closes from the public receipt, without reposting bytes.
    send(&mut net, 3, &K::SettleModelVectorDemandV2 { demand: other.as_bytes() }).await;
    assert_eq!(net.api().unwrap().model_vector_open_demands_v2(&vector), 3);
    let until = net.api().unwrap().model_vector_header_v2(&vector).unwrap().liability_until;
    net.beat_to(until + 1).await;
    let state = net.chain.tip_state().1;
    assert_eq!(
        state.model_vector_reserved_at_v2(&net.bond(4), net.daa()),
        mega(100) as u128,
        "unresolved default cannot escape by waiting out the liability horizon"
    );
    assert!(palw_bond_backs_live_duty_v1(&state, &net.bond(4), net.daa(), None));
    let before = net.collateral(4);
    let fee = net.slashed(2);
    let late = borsh::to_vec(&vector_answer(&material, &node_unit(1))).unwrap();
    send(&mut net, 2, &K::AnswerModelVectorV2 { demand: demands[1].as_bytes(), answer: late }).await;
    assert_eq!(net.slashed(2), fee, "late answer is refused before classification");
    send(&mut net, 3, &K::SettleModelVectorDemandV2 { demand: demands[1].as_bytes() }).await;
    let h = net.api().unwrap().model_vector_header_v2(&vector).unwrap();
    assert!(h.unavailable && !h.refuted, "availability failure is not an integer conviction");
    assert_eq!(before - net.collateral(4), mega(100));
    assert_eq!(net.owed(7), mega(49));
    assert_eq!(net.owed(3), 0, "copying the explicit close cannot steal its requester's bounty");
    assert_eq!(net.chain.tip_state().1.model_vector_demand_reserved_v2(&net.bond(7)), 0);
    assert_eq!(net.chain.tip_state().1.model_vector_reserved_at_v2(&net.bond(4), net.daa()), 0);
    for d in &demands {
        send(&mut net, 3, &K::SettleModelVectorDemandV2 { demand: d.as_bytes() }).await;
    }
    assert_eq!(net.api().unwrap().model_vector_open_demands_v2(&vector), 0);
    assert_eq!(net.collateral(1), source);
    let (post, _) = vector_post(&net, &f, binding, vec![1, 2], false);
    let vector2 = send_vector(&mut net, &f, binding, 4, &post).await;
    let new = ask_vector(&mut net, vector2, 7, 0, &node_unit(0)).await.unwrap();
    assert_ne!(new, demands[0]);
    assert!(net.api().unwrap().model_vector_demand_v2(&demands[0]).is_none(), "settled slot replacement removes its old row");
    let fee = net.slashed(2);
    send(&mut net, 2, &K::AnswerModelVectorV2 { demand: new.as_bytes(), answer: valid }).await;
    assert!(net.slashed(2) > fee, "another prompt's opening cannot answer the new vector's unit");
    let early = net.api().unwrap().model_vector_demand_v2(&new).unwrap();
    net.beat_to(early.filed_daa + 8).await;
    let later = ask_vector(&mut net, vector2, 6, 0, &node_unit(0)).await.unwrap();
    let (_, material2) = vector_post(&net, &f, binding, vec![1, 2], false);
    net.beat_to(early.deadline_daa + 1).await;
    let response = borsh::to_vec(&vector_answer(&material2, &node_unit(0))).unwrap();
    send(&mut net, 4, &K::AnswerModelVectorV2 { demand: later.as_bytes(), answer: response }).await;
    let served = net.api().unwrap().model_vector_served_v2(&vector2, &node_unit(0)).unwrap();
    assert!(served.answered_daa > early.deadline_daa);
    assert!(net.api().unwrap().model_vector_demand_v2(&later).unwrap().settled);
    send(&mut net, 3, &K::SettleModelVectorDemandV2 { demand: new.as_bytes() }).await;
    assert!(
        net.api().unwrap().model_vector_header_v2(&vector2).unwrap().unavailable,
        "later service of the shared unit cannot retroactively erase an earlier request's default"
    );
    let replay = net.replay().await;
    net.assert_same(&replay, "DA slots, hostile proofs, late answer, held horizon, explicit default and new generation replay");
}

// Explicit flat-v5 profile. The producer helper returns only public commitment
// structure and hash openings to direct court component tests, never output values.
// The separate DA tests below obtain material through the real node read API.
struct FlatPublicMaterial {
    c: SegmentedCommitmentsV1,
    hash: Option<misaka_palw_kernel::merkle5::FlatLeafHashV5>,
}
fn flat_vector_post_at(
    net: &Net,
    f: &Fixture,
    binding: Hash64,
    prompt: Vec<u32>,
    lie: Option<usize>,
) -> (kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorPostV3, FlatPublicMaterial) {
    use kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorPostV3;
    let encoder = f.d.digest() == k2_tir_v5_descriptor().digest();
    let e = encoder.then(|| misaka_palw_kernel::seg_encoder::encoder_binding_v1(&f.p).unwrap());
    let mut trace = if let Some(e) = e {
        misaka_palw_kernel::seg_encoder::trace_encoder_v1(&f.p, &f.params, &e, &prompt).unwrap()
    } else {
        misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &prompt).unwrap()
    };
    if let Some(at) = lie {
        trace.values[0][0][0].data[at] += 1;
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
    let hash = (trace.values[0][0][0].data.len() >= 64)
        .then(|| misaka_palw_kernel::merkle5::FlatTreeV5::of(&trace.values[0][0][0]).unwrap().opening(0).unwrap());
    let ledger = net.ledger();
    let row = &ledger.conformance_classes[&f.class].1;
    let scope = misaka_palw_kernel::flat_scope5::flat_resource_scope_v5(&f.p, f.plan.max_positions, e.as_ref()).unwrap();
    let evidence = build_segmented_evidence_v1(
        row.header(f.class),
        &row.descriptor,
        prompt.len() as u32,
        &prompt_root_of_ids_v1(&prompt),
        &[],
        &c,
    );
    let mut post = FlatModelVectorPostV3 {
        version: 3,
        profile: scope.profile,
        scope_root: scope.root(),
        trace: kaspa_consensus_core::palw_model_vector_v2::ModelVectorPostV2 {
            version: 2,
            prompt,
            max_new_tokens: if encoder { 0 } else { 1 },
            generated: if encoder { vec![] } else { vec![3] },
            evidence,
            segment_roots: c.segment_roots(),
            implementation_roots: [[0; 64]; 3],
        },
    };
    post.trace.implementation_roots = [post.trace_root(f.class, binding.as_bytes()); 3];
    (post, FlatPublicMaterial { c, hash })
}
fn flat_vector_post(
    net: &Net,
    f: &Fixture,
    binding: Hash64,
    prompt: Vec<u32>,
    lie: bool,
) -> (kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorPostV3, FlatPublicMaterial) {
    flat_vector_post_at(net, f, binding, prompt, lie.then_some(0))
}
async fn send_flat_vector(
    net: &mut Net,
    f: &Fixture,
    binding: Hash64,
    card: usize,
    post: &kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorPostV3,
) -> Hash64 {
    let id = post.id(f.class, binding.as_bytes());
    send(net, card, &K::PostFlatModelVectorV3 { class: f.class, binding: binding.as_bytes(), post: borsh::to_vec(post).unwrap() })
        .await;
    id
}
fn flat_first_relation_fault(
    view: &kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorViewV3,
    public: &FlatPublicMaterial,
    model: &MapParams,
) -> Vec<u8> {
    flat_first_relation_fault_from_reads(view, public.c.node_opening(0, 0, 0).unwrap(), public.hash.as_ref(), model)
}
fn flat_first_relation_fault_from_reads(
    view: &kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorViewV3,
    output: misaka_palw_kernel::seg::NodeOpeningV1,
    hash: Option<&misaka_palw_kernel::merkle5::FlatLeafHashV5>,
    model: &MapParams,
) -> Vec<u8> {
    use misaka_palw_kernel::flat_court5::*;
    use misaka_palw_kernel::merkle3::{LeafOpeningV3, TreesV3};
    // Only the external model copy and authenticated job input supply operands.
    let table = model.tensors[&(0, None)].clone();
    let token = if let Some(e) = view.trace.encoder {
        e.ids(view.trace.job.inline_prompt.as_ref().unwrap()).unwrap()
    } else {
        Tensor::scalar(DType::Idx, view.trace.job.inline_prompt.as_ref().unwrap()[0] as i128).unwrap()
    };
    let own = vec![table.clone(), token];
    let deps = if hash.is_some() {
        flat_leaf_dependencies_v5(&view.context(), 0, 0, 0, hash.unwrap().index, &own).unwrap()
    } else {
        flat_whole_dependencies_v5(&view.context(), 0, 0, 0, &own).unwrap()
    };
    let tree = TreesV3::of(&table);
    let inputs = vec![
        FlatOperandV5 {
            node: None,
            flat: vec![],
            legacy: deps.leaves[0]
                .iter()
                .map(|(line, tile)| LeafOpeningV3::of_trees(&table, &tree, 0, *line, *tile).unwrap())
                .collect(),
        },
        FlatOperandV5::default(),
    ];
    let fault = if let Some(leaf) = hash {
        FlatFaultV5::Leaf(FlatLeafFaultV5 { version: 5, output, leaf: leaf.clone(), inputs, token: None })
    } else {
        FlatFaultV5::Whole(FlatWholeFaultV5 { output, inputs, token: None })
    };
    fault.to_bytes().unwrap()
}
#[tokio::test]
async fn flat_v3_node_preemption_counterexample_and_staged_v6_seal_rejects_equivalent_carrier() {
    use misaka_palw_kernel::flat_court5::*;
    use misaka_palw_kernel::flat_seal6::*;
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (post, public) = flat_vector_post(&net, &f, binding, vec![0, 1], true);
    let id = send_flat_vector(&mut net, &f, binding, 4, &post).await;
    let view = net.api().unwrap().flat_model_vector_view_v3(&id).unwrap();
    let proof = flat_first_relation_fault(&view, &public, &f.params);
    let mut alternate = FlatFaultV5::from_bytes(&proof).unwrap();
    let tile = misaka_palw_kernel::seg::PromptTileOpeningV1::of(&post.trace.prompt, 0).unwrap();
    match &mut alternate {
        FlatFaultV5::Whole(f) => f.token = Some(tile),
        _ => panic!("small encoder relation"),
    }
    // The exact job prompt is already inline in the stored candidate. Its valid
    // redundant tile changes the carrier without changing the proven relation.
    let conviction = verify_flat_fault_v5(&view.context(), &FlatFaultV5::from_bytes(&proof).unwrap()).unwrap();
    assert_eq!(verify_flat_fault_v5(&view.context(), &alternate).unwrap(), conviction);
    let alternate = alternate.to_bytes().unwrap();
    assert_ne!(alternate, proof);
    assert_ne!(flat_proof_digest_v6(&alternate).unwrap(), flat_proof_digest_v6(&proof).unwrap());
    let (finder, copyist, salt) = (net.kid(7), net.kid(6), [42; 64]);
    let mut seals =
        FlatSealBookV6::new(FlatSealPolicyV6 { maturity_daa: 3, ttl_daa: 20, max_rows: 4, max_rows_per_actor: 2 }).unwrap();
    let now = net.daa();
    let until = net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().liability_until;
    let seal = flat_proof_seal_v6(&id.as_bytes(), &finder, &salt, &proof).unwrap();
    seals.post(id.as_bytes(), finder, seal, now, until).unwrap();
    seals.post(id.as_bytes(), copyist, seal, now, until).unwrap();
    net.beat_to(now + 3).await;
    assert_eq!(
        seals.reveal(&id.as_bytes(), &copyist, &salt, &alternate, net.daa(), &view.context()),
        Err(FlatSealErrorV6::SealMismatch)
    );
    let authorized = seals.reveal(&id.as_bytes(), &finder, &salt, &proof, net.daa(), &view.context()).unwrap();
    assert_eq!((authorized.actor, authorized.conviction), (finder, conviction));
    // Frozen v3 does NOT call the staged seal gate. This is a counterexample,
    // not a successful security test of a protected consensus payout route.
    let collateral = net.collateral(4);
    send(&mut net, 6, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: alternate }).await;
    assert_eq!(collateral - net.collateral(4), mega(100));
    assert_eq!(net.owed(6), mega(49));
    assert_eq!(net.owed(7), 0);
    send(&mut net, 7, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof }).await;
    assert_eq!(net.collateral(4), collateral - mega(100));
    assert_eq!(net.owed(7), 0);
    let replay = net.replay().await;
    net.assert_same(&replay, "frozen unsealed v3 counterexample and separate staged seal contract");
}
#[tokio::test]
async fn flat_model_vector_node_admits_typed_scope_convicts_leaf_and_whole_once_and_replays_redemption() {
    for encoder in [true, false] {
        let f = vector_fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let binding = prepare(&mut net, &f).await;
        let (honest, public) = flat_vector_post(&net, &f, binding, vec![0, 1], false);
        let honest_id = send_flat_vector(&mut net, &f, binding, 4, &honest).await;
        let view = net.api().unwrap().flat_model_vector_view_v3(&honest_id).unwrap();
        let fault = flat_first_relation_fault(&view, &public, &f.params);
        assert_eq!(
            misaka_palw_kernel::flat_court5::verify_flat_fault_v5(
                &view.context(),
                &misaka_palw_kernel::flat_court5::FlatFaultV5::from_bytes(&fault).unwrap()
            ),
            Err(misaka_palw_kernel::verify::DismissalV1::NoFault)
        );
        let (bad, public) = flat_vector_post(&net, &f, binding, vec![0, 1], true);
        let id = send_flat_vector(&mut net, &f, binding, 4, &bad).await;
        let view = net.api().unwrap().flat_model_vector_view_v3(&id).unwrap();
        let fault = flat_first_relation_fault(&view, &public, &f.params);
        let fee = net.slashed(7);
        send(&mut net, 7, &K::RefuteFlatModelVectorV3 { vector: honest_id.as_bytes(), proof: fault.clone() }).await;
        assert!(net.slashed(7) > fee);
        assert!(!net.api().unwrap().flat_model_vector_header_v3(&honest_id).unwrap().refuted);
        let collateral = net.collateral(4);
        send(&mut net, 4, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: fault.clone() }).await;
        assert_eq!(net.collateral(4), collateral, "self-operator bounty refused");
        send(&mut net, 7, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: fault.clone() }).await;
        assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
        assert_eq!(collateral - net.collateral(4), mega(100));
        let reward = mega(49);
        assert_eq!(net.owed(7), reward);
        let after = net.collateral(4);
        send(&mut net, 6, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: fault }).await;
        assert_eq!(net.collateral(4), after);
        send(&mut net, 2, &register(&f)).await;
        assert!(net.ledger().classes.is_empty());
        assert!(net.ledger().jobs.is_empty() && net.ledger().claims.is_empty());
        assert_eq!(minted_and_owed(&net, &net.chain, 7), (reward, 0));
        let replay = net.replay().await;
        net.assert_same(&replay, "explicit flat profile typed scope and one collected conviction");
        assert_eq!(minted_and_owed(&net, &replay, 7), (reward, 0));
    }
}
#[tokio::test]
async fn flat_model_vector_node_scope_identity_and_hostile_filings_pay_shared_budgets_without_poisoning() {
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (post, _) = flat_vector_post(&net, &f, binding, vec![0, 1], false);
    let source = net.collateral(1);
    for field in 0..5 {
        let mut bad = post.clone();
        match field {
            0 => bad.profile[0] ^= 1,
            1 => bad.scope_root[0] ^= 1,
            2 => bad.trace.evidence.header.program_root[0] ^= 1,
            3 => bad.trace.implementation_roots[0][0] ^= 1,
            _ => bad.trace.evidence.header.artifact_root[0] ^= 1,
        }
        if field != 3 {
            bad.trace.implementation_roots = [bad.trace_root(f.class, binding.as_bytes()); 3];
        }
        let fees = net.slashed(4);
        let id = send_flat_vector(&mut net, &f, binding, 4, &bad).await;
        assert!(net.api().unwrap().flat_model_vector_header_v3(&id).is_none());
        assert!(net.slashed(4) > fees);
        assert_eq!(net.collateral(1), source);
    }
    let id = send_flat_vector(&mut net, &f, binding, 4, &post).await;
    let held = net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa());
    let fees = net.slashed(4);
    send_flat_vector(&mut net, &f, binding, 4, &post).await;
    assert!(net.slashed(4) > fees);
    assert_eq!(held, net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa()));
    let h = net.api().unwrap().flat_model_vector_header_v3(&id).unwrap();
    let junk = vec![5, 0, 255];
    let work = kaspa_consensus_core::palw_flat_model_vector_v3::flat_model_vector_work_v3(
        h.program_bytes_len,
        h.pc_instances,
        junk.len() + h.post_bytes_len as usize,
    )
    .unwrap()
        + h.max_court_work;
    let expected = net.ledger().policy.dismissal_fee_v1(work);
    let cards = [2, 3, 5, 6, 7];
    let before: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    let objects = cards
        .iter()
        .map(|c| (*c, net.route(*c, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: junk.clone() })))
        .collect();
    net.send(objects).await;
    let deltas: Vec<_> = cards.iter().zip(before).map(|(c, b)| net.slashed(*c) - b).collect();
    assert!(deltas.iter().all(|d| *d == 0 || *d == expected));
    assert_eq!(deltas.iter().filter(|d| **d == expected).count(), net.ledger().policy.max_adjudications_per_block as usize);
    assert!(!net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(net.collateral(1), source);
    let replay = net.replay().await;
    net.assert_same(&replay, "flat hostile declarations and bounded court flood");
}
#[tokio::test]
async fn flat_model_vector_node_and_v2_catalogs_share_the_same_capital_and_eight_statement_limit() {
    use kaspa_consensus_core::palw_state_v2::{palw_bond_backs_live_duty_v1, palw_bond_backs_live_duty_v2, palw_bond_committed_v1};
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    for n in 0..8 {
        if n % 2 == 0 {
            let (p, _) = flat_vector_post(&net, &f, binding, vec![n, n + 1], false);
            let id = send_flat_vector(&mut net, &f, binding, 4, &p).await;
            assert!(net.api().unwrap().flat_model_vector_header_v3(&id).is_some());
            if n == 0 {
                let state = net.chain.tip_state().1;
                let bond = net.bond(4);
                assert!(palw_bond_committed_v1(&state, &bond, net.daa(), None, 10) >= mega(100) as u128);
                assert!(palw_bond_backs_live_duty_v1(&state, &bond, net.daa(), None));
                assert!(palw_bond_backs_live_duty_v2(&state, &bond, net.daa(), None, 10));
            }
        } else {
            let (p, _) = vector_post(&net, &f, binding, vec![n, n + 1], false);
            let id = send_vector(&mut net, &f, binding, 4, &p).await;
            assert!(net.api().unwrap().model_vector_header_v2(&id).is_some());
        }
    }
    let bond = net.bond(4);
    let state = net.chain.tip_state().1;
    assert_eq!(state.flat_model_vector_reserved_at_v3(&bond, net.daa()), mega(400) as u128);
    assert_eq!(state.model_vector_reserved_at_v2(&bond, net.daa()), mega(400) as u128);
    assert!(palw_bond_committed_v1(&state, &bond, net.daa(), None, 10) >= mega(800) as u128);
    let (p, _) = flat_vector_post(&net, &f, binding, vec![9, 10], false);
    let id = send_flat_vector(&mut net, &f, binding, 4, &p).await;
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).is_none());
    let (p, _) = vector_post(&net, &f, binding, vec![9, 10], false);
    let id = send_vector(&mut net, &f, binding, 4, &p).await;
    assert!(net.api().unwrap().model_vector_header_v2(&id).is_none());
    let flat_ids = net.api().unwrap().flat_model_vectors_of_v3(&bond);
    let until = net.api().unwrap().flat_model_vector_header_v3(&flat_ids[0]).unwrap().liability_until;
    net.beat_to(until + 30).await;
    assert_eq!(net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&bond, net.daa()), 0);
    assert_eq!(net.api().unwrap().flat_model_vectors_of_v3(&bond).len(), 4);
    let replay = net.replay().await;
    net.assert_same(&replay, "mixed typed vector catalog and shared original capital");
}

async fn ask_flat_vector(
    net: &mut Net,
    vector: Hash64,
    card: usize,
    slot: u8,
    unit: &kaspa_consensus_core::palw_flat_model_vector_da_v3::FlatVectorUnitV3,
) -> Option<Hash64> {
    let before = net.api().unwrap().flat_vector_slot_v3(&net.bond(card), slot);
    send(
        net,
        card,
        &K::DemandFlatModelVectorV3 {
            vector: vector.as_bytes(),
            slot,
            position: unit.position,
            occurrence: unit.occurrence,
            node: unit.node,
            leaf: unit.leaf,
        },
    )
    .await;
    net.api().unwrap().flat_vector_slot_v3(&net.bond(card), slot).filter(|id| Some(*id) != before)
}
fn flat_server_answer(
    server: &FlatPublicMaterial,
    tree: Option<&misaka_palw_kernel::merkle5::FlatTreeV5>,
    unit: &kaspa_consensus_core::palw_flat_model_vector_da_v3::FlatVectorUnitV3,
) -> Vec<u8> {
    use kaspa_consensus_core::palw_flat_model_vector_da_v3::FlatVectorAnswerV3;
    borsh::to_vec(&FlatVectorAnswerV3 {
        version: 3,
        opening: server.c.node_opening(unit.position, unit.occurrence, unit.node).unwrap(),
        leaf: unit.leaf.map(|i| tree.unwrap().opening(i).unwrap()),
    })
    .unwrap()
}
#[tokio::test]
async fn flat_hash_da_node_fresh_public_reads_localize_late_leaf_and_convict_without_output_values() {
    use kaspa_consensus_core::palw_flat_model_vector_da_v3::*;
    use misaka_palw_kernel::merkle5::*;
    let f = vector_fixture(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (post, server) = flat_vector_post_at(&net, &f, binding, vec![0, 1], Some(1900));
    let vector = send_flat_vector(&mut net, &f, binding, 4, &post).await;
    // Producer retains its private tree. It supplies only authenticated wire replies.
    let mut producer = misaka_palw_kernel::trace::trace_v1(&f.p, &f.params, &post.trace.prompt).unwrap();
    producer.values[0][0][0].data[1900] += 1;
    let producer_tree = FlatTreeV5::of(&producer.values[0][0][0]).unwrap();
    drop(producer);
    use misaka_palw_kernel::flat_detect5::*;
    struct PublicChain<'a> {
        read: &'a dyn Fn(FlatReadV5) -> Option<FlatVectorAnswerV3>,
    }
    impl FlatPublicReadsV5 for PublicChain<'_> {
        fn node(&self, u: FlatReadV5) -> Option<misaka_palw_kernel::seg::NodeOpeningV1> {
            (self.read)(u).map(|a| a.opening)
        }
        fn hash(&self, u: FlatReadV5) -> Option<FlatLeafHashV5> {
            (self.read)(u).and_then(|a| a.leaf)
        }
    }
    let view = net.api().unwrap().flat_model_vector_view_v3(&vector).unwrap();
    let job = view.trace.job.inline_prompt.as_ref().unwrap();
    let max = flat_material_bounds_v5(DType::I8, &[2048]).unwrap().localization_replies + 1;
    use misaka_palw_kernel::flat_replay6::*;
    assert_eq!(FLAT_REPLAY_REPLY_BYTES_V6, FLAT_VECTOR_ANSWER_MAX_BYTES_V3 as u128);
    let envelope = net.api().unwrap().flat_model_vector_replay_envelope_v6(&vector).unwrap();
    assert_eq!(envelope.base_scope, view.scope.root());
    let attempts = (max + 1) as u32;
    let limits = FlatReplayLimitsV6 {
        position_tensor_bytes: envelope.position_tensor_bytes,
        operand_tensor_bytes: envelope.largest_operand_tensor_bytes,
        history_tensor_bytes: envelope.history_rows_tensor_bytes,
        parameter_hash_bytes: envelope.parameter_hash_bytes_per_attempt * attempts as u128,
        trace_hash_bytes: envelope.trace_hash_bytes_per_attempt * attempts as u128,
        node_reads: envelope.node_reads_per_attempt * attempts as u128,
        hash_reads: envelope.hash_reads_per_attempt * attempts as u128,
        reply_bytes: envelope.reply_bytes_per_attempt * attempts as u128,
        attempts,
    };
    let mut session = FlatReplaySessionV6::new(envelope.clone(), limits);
    let mut queried = Vec::new();
    let mut terminal = None;
    for _ in 0..=max {
        // Every retry reads native stored bytes. The generic verifier executes
        // its own model; it never receives the producer's tensor or tree.
        let finding = {
            let api = net.api().unwrap();
            let read = |u: FlatReadV5| {
                let unit = FlatVectorUnitV3 { position: u.position, occurrence: u.occurrence, node: u.node, leaf: u.leaf };
                decode_flat_vector_answer_v3(&api.flat_vector_public_answer_v3(&vector, &unit)?).ok()
            };
            session.check(
                &view.context(),
                &view.scope,
                &PublicChain { read: &read },
                &|i, l| f.params.tensors.get(&(i, l)).cloned(),
                job,
            )
        };
        match finding {
            FlatReplayResultV6::Finding(FlatFindingV5::Demand(u)) => {
                assert_eq!((u.position, u.occurrence, u.node), (0, 0, 0));
                let unit = FlatVectorUnitV3 { position: u.position, occurrence: u.occurrence, node: u.node, leaf: u.leaf };
                let demand = ask_flat_vector(&mut net, vector, 7, 0, &unit).await.unwrap();
                let reply = flat_server_answer(&server, u.leaf.map(|_| &producer_tree), &unit);
                send(&mut net, 4, &K::AnswerFlatModelVectorV3 { demand: demand.as_bytes(), answer: reply }).await;
                let api = net.api().unwrap();
                let answer = decode_flat_vector_answer_v3(&api.flat_vector_public_answer_v3(&vector, &unit).unwrap()).unwrap();
                assert!(answer.authenticates(&api.flat_vector_demand_v3(&demand).unwrap()));
                queried.push(unit);
            }
            FlatReplayResultV6::Finding(FlatFindingV5::Fault(fault)) => {
                terminal = Some(fault);
                break;
            }
            other => panic!("bad relation did not yield a filing: {other:?}"),
        }
    }
    assert!(session.attempts() > 1 && session.attempts() <= attempts);
    assert_eq!(
        session.reserved_hash_bytes(),
        (
            envelope.parameter_hash_bytes_per_attempt * session.attempts() as u128,
            envelope.trace_hash_bytes_per_attempt * session.attempts() as u128
        )
    );
    let (nodes, hashes, bytes) = session.public_reads();
    assert!(nodes <= envelope.node_reads_per_attempt * attempts as u128);
    assert!(hashes <= envelope.hash_reads_per_attempt * attempts as u128);
    assert!(bytes <= envelope.reply_bytes_per_attempt * attempts as u128);
    let fault = terminal.expect("generic verifier localized a late leaf within the bound");
    match &fault {
        misaka_palw_kernel::flat_court5::FlatFaultV5::Leaf(f) => assert_eq!(f.leaf.index, 1900 / 64),
        other => panic!("wrong terminal court: {other:?}"),
    }
    assert!(queried.len() > 2, "initial leaf matched; subsequent probes were needed");
    assert!(queried.len() as u64 <= max);
    let fault = fault.to_bytes().unwrap();
    send(&mut net, 7, &K::RefuteFlatModelVectorV3 { vector: vector.as_bytes(), proof: fault }).await;
    assert!(net.api().unwrap().flat_model_vector_header_v3(&vector).unwrap().refuted);
    assert_eq!(net.owed(7), mega(49));
    assert_eq!(net.chain.tip_state().1.flat_vector_demand_reserved_v3(&net.bond(7)), 0);
    net.send(Vec::new()).await;
    let replay = net.replay().await;
    net.assert_same(&replay, "public hash-only localization and independent payout replay");
    for unit in queried {
        let other = replay.ctx.consensus.palw_kernel_route_v1().unwrap();
        assert_eq!(
            other.flat_vector_public_answer_v3(&vector, &unit),
            net.api().unwrap().flat_vector_public_answer_v3(&vector, &unit)
        );
    }
    assert_eq!(minted_and_owed(&net, &replay, 7), (mega(49), 0));
}
#[tokio::test]
async fn flat_hash_da_node_shared_slots_response_priority_withholding_and_horizon_hold() {
    use kaspa_consensus_core::palw_flat_model_vector_da_v3::*;
    use kaspa_consensus_core::palw_state_v2::{palw_bond_backs_live_duty_v1, palw_bond_backs_live_duty_v2};
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (p, server) = flat_vector_post(&net, &f, binding, vec![0, 1], false);
    let vector = send_flat_vector(&mut net, &f, binding, 4, &p).await;
    let unit = FlatVectorUnitV3 { position: 0, occurrence: 0, node: 0, leaf: None };
    let binder = net.collateral(1);
    assert!(ask_flat_vector(&mut net, vector, 4, 0, &unit).await.is_none());
    let fee = net.slashed(7);
    assert!(
        ask_flat_vector(&mut net, vector, 7, 0, &FlatVectorUnitV3 { leaf: Some(0), ..unit.clone() }).await.is_none(),
        "small node never exposes a hash leaf or clear value"
    );
    assert!(net.slashed(7) > fee);
    let demand = ask_flat_vector(&mut net, vector, 7, 0, &unit).await.unwrap();
    let (old, _) = vector_post(&net, &f, binding, vec![0, 1], false);
    let old_vector = send_vector(&mut net, &f, binding, 4, &old).await;
    assert!(ask_vector(&mut net, old_vector, 7, 0, &node_unit(0)).await.is_none(), "new slot blocks old route");
    let old_demand = ask_vector(&mut net, old_vector, 6, 0, &node_unit(0)).await.unwrap();
    assert!(ask_flat_vector(&mut net, vector, 6, 0, &unit).await.is_none(), "old slot blocks new route");
    let other = ask_flat_vector(&mut net, vector, 5, 0, &unit).await.unwrap();
    let valid = flat_server_answer(&server, None, &unit);
    let mut bad = valid.clone();
    bad[10] ^= 1;
    let fee = net.slashed(2);
    send(&mut net, 2, &K::AnswerFlatModelVectorV3 { demand: demand.as_bytes(), answer: bad }).await;
    assert!(net.slashed(2) > fee);
    let deposit = net.api().unwrap().flat_vector_demand_v3(&demand).unwrap().bond;
    let state = net.chain.tip_state().1;
    assert_eq!(state.flat_vector_demand_reserved_v3(&net.bond(7)), deposit as u128);
    assert!(palw_bond_backs_live_duty_v1(&state, &net.bond(7), net.daa(), None));
    assert!(palw_bond_backs_live_duty_v2(&state, &net.bond(7), net.daa(), None, 10));
    let hostile = [2, 3, 6, 7];
    let fees: Vec<_> = hostile.iter().map(|c| net.slashed(*c)).collect();
    let mut objects: Vec<_> = hostile
        .iter()
        .map(|c| (*c, net.route(*c, &K::RefuteFlatModelVectorV3 { vector: vector.as_bytes(), proof: vec![5, 0, 255] })))
        .collect();
    objects.push((4, net.route(4, &K::AnswerFlatModelVectorV3 { demand: demand.as_bytes(), answer: valid.clone() })));
    net.send(objects).await;
    assert_eq!(
        hostile.iter().zip(fees).filter(|(c, b)| net.slashed(**c) > *b).count(),
        net.ledger().policy.max_adjudications_per_block as usize
    );
    assert!(net.api().unwrap().flat_vector_demand_v3(&demand).unwrap().settled, "bounded answer survives a fully spent court count");
    send(&mut net, 3, &K::SettleFlatModelVectorDemandV3 { demand: other.as_bytes() }).await;
    assert_eq!(net.api().unwrap().flat_vector_open_demands_v3(&vector), 0);
    // Slot reuse deletes the old fixed request but keeps published answer bytes.
    let missing = FlatVectorUnitV3 { node: 1, ..unit.clone() };
    let pending = ask_flat_vector(&mut net, vector, 7, 0, &missing).await.unwrap();
    assert!(net.api().unwrap().flat_vector_demand_v3(&demand).is_none());
    assert_eq!(net.api().unwrap().flat_vector_public_answer_v3(&vector, &unit), Some(valid));
    let until = net.api().unwrap().flat_model_vector_header_v3(&vector).unwrap().liability_until;
    net.beat_to(until + 1).await;
    assert_eq!(net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa()), mega(100) as u128);
    let before = net.collateral(4);
    send(&mut net, 3, &K::SettleFlatModelVectorDemandV3 { demand: pending.as_bytes() }).await;
    assert!(net.api().unwrap().flat_vector_unavailable_v3(&vector));
    assert!(!net.api().unwrap().flat_model_vector_header_v3(&vector).unwrap().refuted);
    assert_eq!(before - net.collateral(4), mega(100));
    assert_eq!(net.owed(7), mega(49));
    assert_eq!(net.owed(3), 0);
    assert_eq!(net.chain.tip_state().1.flat_model_vector_reserved_at_v3(&net.bond(4), net.daa()), 0);
    assert_eq!(net.chain.tip_state().1.flat_vector_demand_reserved_v3(&net.bond(7)), 0);
    send(&mut net, 3, &K::SettleFlatModelVectorDemandV3 { demand: old_demand.as_bytes() }).await;
    assert_eq!(net.collateral(1), binder);
    let replay = net.replay().await;
    net.assert_same(&replay, "shared requester slots, held liability and objective flat DA default");
}

async fn send_protected_flat_vector(
    net: &mut Net,
    f: &Fixture,
    binding: Hash64,
    card: usize,
    body: &kaspa_consensus_core::palw_flat_model_vector_v3::FlatModelVectorPostV3,
) -> Hash64 {
    let (id, object) =
        misaka_palw_sdk::flat_candidate_v6::prepare_flat_model_post_v6(Hash64::from_bytes(f.class), binding, body.clone()).unwrap();
    send(net, card, &object).await;
    id
}
#[tokio::test]
async fn flat_v6_native_seal_blocks_equivalent_preemption_legacy_bypass_and_pays_finder_once() {
    use kaspa_consensus_core::palw_flat_model_seal_v6::*;
    use misaka_palw_kernel::flat_court5::*;
    use misaka_palw_kernel::flat_seal6::*;
    for encoder in [true, false] {
        let f = vector_fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        let binding = prepare(&mut net, &f).await;
        let (post, public) = flat_vector_post(&net, &f, binding, vec![0, 1], true);
        let id = send_protected_flat_vector(&mut net, &f, binding, 4, &post).await;
        assert!(net.api().unwrap().flat_model_vector_protected_v6(&id));
        assert_ne!(id, post.id(f.class, binding.as_bytes()));
        let view = net.api().unwrap().flat_model_vector_view_v3(&id).unwrap();
        let proof = flat_first_relation_fault(&view, &public, &f.params);
        let mut alternate = FlatFaultV5::from_bytes(&proof).unwrap();
        if encoder {
            match &mut alternate {
                FlatFaultV5::Whole(f) => f.token = Some(PromptTileOpeningV1::of(&post.trace.prompt, 0).unwrap()),
                _ => panic!(),
            }
            assert_eq!(
                verify_flat_fault_v5(&view.context(), &alternate),
                verify_flat_fault_v5(&view.context(), &FlatFaultV5::from_bytes(&proof).unwrap())
            );
        }
        let alternate = alternate.to_bytes().unwrap();
        let (finder, thief, salt) = (net.kid(7), net.kid(6), [42; 64]);
        let private =
            misaka_palw_sdk::flat_candidate_v6::FlatProofSecretV6::new(net.domain, id, net.bond(7), salt, proof.clone()).unwrap();
        let seal = flat_proof_seal_v6(&id.as_bytes(), &finder, &salt, &proof).unwrap();
        let collateral = net.collateral(4);
        // The old unsealed consumer must refuse this new statement identity.
        send(&mut net, 6, &K::RefuteFlatModelVectorV3 { vector: id.as_bytes(), proof: alternate.clone() }).await;
        send(&mut net, 6, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof: alternate.clone() }).await;
        assert_eq!(net.owed(6), 0);
        assert_eq!(net.collateral(4), collateral);
        let seal_object = private.seal_object(net.domain, &net.bond(7)).unwrap();
        assert_eq!(seal_object, K::SealFlatModelProofV6 { vector: id.as_bytes(), seal });
        send(&mut net, 7, &seal_object).await;
        let row = net.api().unwrap().flat_model_seal_v6(&id, &finder).unwrap();
        assert_eq!(row.seal.mature_daa, row.seal.posted_daa + FLAT_NATIVE_SEAL_MATURITY_DAA_V6);
        assert_eq!(net.chain.tip_state().1.flat_model_seal_reserved_v6(&net.bond(7)), row.reserved as u128);
        assert!(row.reserved > 0);
        // Copying the seal cannot bind another authenticated signer to its bytes.
        send(&mut net, 6, &K::SealFlatModelProofV6 { vector: id.as_bytes(), seal }).await;
        net.beat_to(row.seal.mature_daa).await;
        send(&mut net, 6, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof: alternate.clone() }).await;
        assert!(!net.api().unwrap().flat_model_seal_v6(&id, &thief).unwrap().seal.spent);
        // An equivalent proof learned at reveal cannot get a new mature seal first.
        let fresh = flat_proof_seal_v6(&id.as_bytes(), &net.kid(5), &salt, &alternate).unwrap();
        send(&mut net, 5, &K::SealFlatModelProofV6 { vector: id.as_bytes(), seal: fresh }).await;
        send(&mut net, 5, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof: alternate }).await;
        assert_eq!(net.owed(5), 0);
        assert_eq!(net.collateral(4), collateral);
        if encoder {
            let before = net.slashed(7);
            let altered = match FlatFaultV5::from_bytes(&proof).unwrap() {
                FlatFaultV5::Whole(mut f) => {
                    f.token = Some(PromptTileOpeningV1::of(&post.trace.prompt, 0).unwrap());
                    FlatFaultV5::Whole(f).to_bytes().unwrap()
                }
                _ => panic!(),
            };
            send(&mut net, 7, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof: altered }).await;
            assert!(net.slashed(7) > before);
        }
        let reveal_object = private
            .reveal_object(net.domain, &net.bond(7), &net.api().unwrap().flat_model_seal_v6(&id, &finder).unwrap(), net.daa())
            .unwrap();
        send(&mut net, 7, &reveal_object).await;
        assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
        assert!(net.api().unwrap().flat_model_seal_v6(&id, &finder).unwrap().seal.spent);
        assert_eq!(net.chain.tip_state().1.flat_model_seal_reserved_v6(&net.bond(7)), 0);
        assert_eq!(collateral - net.collateral(4), mega(100));
        let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
        assert_eq!(minted + owed, mega(49));
        assert_eq!((net.owed(5), net.owed(6)), (0, 0));
        send(&mut net, 7, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof }).await;
        let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
        assert_eq!(minted + owed, mega(49));
        assert_eq!(net.collateral(4), collateral - mega(100));
        // Closing a decided vector returns the copyist's unused deposit, once.
        send(&mut net, 3, &K::ExpireFlatModelSealV6 { vector: id.as_bytes(), actor: thief }).await;
        assert!(net.api().unwrap().flat_model_seal_v6(&id, &thief).is_none());
        assert_eq!(net.chain.tip_state().1.flat_model_seal_reserved_v6(&net.bond(6)), 0);
        assert_eq!(minted_and_owed(&net, &net.chain, 7), (mega(49), 0));
        let replay = net.replay().await;
        net.assert_same(&replay, "protected seal/reveal conviction, original capital and one finder payout");
        assert_eq!(minted_and_owed(&net, &replay, 7), (mega(49), 0));
    }
}
#[tokio::test]
async fn flat_v6_native_seals_bound_actor_slots_hold_capital_and_forfeit_expired_false_seals() {
    use kaspa_consensus_core::palw_flat_model_seal_v6::*;
    use misaka_palw_kernel::flat_seal6::flat_proof_seal_v6;
    let f = vector_fixture(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let mut ids = Vec::new();
    for n in 0..5 {
        let (body, _) = flat_vector_post(&net, &f, binding, vec![n, n + 1], false);
        let id = send_protected_flat_vector(&mut net, &f, binding, 4, &body).await;
        assert!(net.api().unwrap().flat_model_vector_header_v3(&id).is_some());
        let seal = flat_proof_seal_v6(&id.as_bytes(), &net.kid(7), &[1; 64], b"bad proof").unwrap();
        send(&mut net, 7, &K::SealFlatModelProofV6 { vector: id.as_bytes(), seal }).await;
        ids.push(id);
    }
    assert_eq!(net.api().unwrap().flat_model_seals_of_v6(&net.bond(7)).len(), FLAT_NATIVE_SEALS_PER_BOND_V6);
    assert_eq!(net.api().unwrap().flat_model_seal_count_v6(), FLAT_NATIVE_SEALS_PER_BOND_V6 as u32);
    assert!(net.api().unwrap().flat_model_seal_v6(&ids[4], &net.kid(7)).is_none());
    let row = net.api().unwrap().flat_model_seal_v6(&ids[0], &net.kid(7)).unwrap();
    let held = net.chain.tip_state().1.flat_model_seal_reserved_v6(&net.bond(7));
    let state = net.chain.tip_state().1;
    assert!(kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v1(&state, &net.bond(7), net.daa(), None));
    assert!(kaspa_consensus_core::palw_state_v2::palw_bond_backs_live_duty_v2(&state, &net.bond(7), net.daa(), None, 10));
    assert_eq!(held, row.reserved as u128 * FLAT_NATIVE_SEALS_PER_BOND_V6 as u128);
    let before = net.slashed(7);
    // A mature seal of malformed bytes enters a bounded court and charges its signer.
    net.beat_to(row.seal.mature_daa).await;
    send(&mut net, 7, &K::RevealFlatModelProofV6 { vector: ids[0].as_bytes(), salt: [1; 64], proof: b"bad proof".to_vec() }).await;
    assert!(net.slashed(7) > before);
    assert!(!net.api().unwrap().flat_model_seal_v6(&ids[0], &net.kid(7)).unwrap().seal.spent);
    // Re-sealing does not replace or extend another pending immutable slot.
    send(&mut net, 7, &K::SealFlatModelProofV6 { vector: ids[0].as_bytes(), seal: [0; 64] }).await;
    assert_eq!(net.api().unwrap().flat_model_seal_v6(&ids[0], &net.kid(7)).unwrap(), row);
    let actor = net.kid(7);
    send(&mut net, 3, &K::ExpireFlatModelSealV6 { vector: ids[0].as_bytes(), actor }).await;
    assert!(net.api().unwrap().flat_model_seal_v6(&ids[0], &actor).is_some());
    net.beat_to(row.seal.expires_daa).await;
    assert_eq!(
        net.chain.tip_state().1.flat_model_seal_reserved_v6(&net.bond(7)),
        held,
        "unsettled expiry cannot free the original capital"
    );
    let collateral = net.collateral(7);
    send(&mut net, 3, &K::ExpireFlatModelSealV6 { vector: ids[0].as_bytes(), actor }).await;
    assert_eq!(collateral - net.collateral(7), row.reserved);
    assert!(net.api().unwrap().flat_model_seal_v6(&ids[0], &actor).is_none());
    assert_eq!(net.api().unwrap().flat_model_seal_count_v6(), 3);
    send(&mut net, 3, &K::ExpireFlatModelSealV6 { vector: ids[0].as_bytes(), actor }).await;
    assert_eq!(net.collateral(7), collateral - row.reserved);
    let replay = net.replay().await;
    net.assert_same(&replay, "protected bounded slots, invalid court fees and explicit deposit forfeiture");
}

#[tokio::test]
async fn flat_v6_native_mature_proof_uses_reserved_budget_after_admission_flood() {
    use kaspa_consensus_core::palw_flat_model_seal_v6::*;
    use misaka_palw_kernel::flat_seal6::flat_proof_seal_v6;
    let f = vector_fixture(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (body, public) = flat_vector_post(&net, &f, binding, vec![0, 1], true);
    let mut post = FlatModelVectorPostV6 { version: 6, body: body.clone() };
    post.body.trace.implementation_roots = [post.trace_root(f.class, binding.as_bytes()); 3];
    let id = send_protected_flat_vector(&mut net, &f, binding, 4, &body).await;
    let view = net.api().unwrap().flat_model_vector_view_v3(&id).unwrap();
    let proof = flat_first_relation_fault(&view, &public, &f.params);
    let actor = net.kid(7);
    let salt = [11; 64];
    let seal = flat_proof_seal_v6(&id.as_bytes(), &actor, &salt, &proof).unwrap();
    send(&mut net, 7, &K::SealFlatModelProofV6 { vector: id.as_bytes(), seal }).await;
    let mature = net.api().unwrap().flat_model_seal_v6(&id, &actor).unwrap().seal.mature_daa;
    net.beat_to(mature).await;
    let collateral = net.collateral(4);
    let cards = [2, 3, 5, 6];
    let before: Vec<_> = cards.iter().map(|c| net.slashed(*c)).collect();
    let mut objects = Vec::new();
    for card in cards {
        objects.push((
            card,
            net.route(
                card,
                &K::PostFlatModelVectorV6 { class: f.class, binding: binding.as_bytes(), post: borsh::to_vec(&post).unwrap() },
            ),
        ));
    }
    objects.push((7, net.route(7, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof })));
    net.send(objects).await;
    assert_eq!(
        cards.iter().zip(before).filter(|(c, b)| net.slashed(**c) > *b).count(),
        (net.ledger().policy.max_adjudications_per_block - net.ledger().policy.prosecution_reserved_runs()) as usize
    );
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(net.owed(7), mega(49));
    assert_eq!(collateral - net.collateral(4), mega(100));
    let replay = net.replay().await;
    net.assert_same(&replay, "admission load cannot spend the mature proof's reserved run/work");
}

fn signed_flat_chunk(net: &mut Net, card: usize, chunk: kaspa_consensus_core::palw_kernel_route_v1::PalwKernelChunkV1) -> Obj {
    use kaspa_consensus_core::palw_kernel_route_v1::*;
    let message = palw_kernel_chunk_message_v1(net.domain, &chunk);
    net.rnd = net.rnd.wrapping_add(1);
    let key = TestConsensus::palw_v2_registry_keypair(card as u64);
    let signature = libcrux_ml_dsa::ml_dsa_87::sign(
        &key.signing_key,
        message.as_byte_slice(),
        PALW_KERNEL_CHUNK_MLDSA87_CONTEXT_V1,
        [net.rnd; 32],
    )
    .unwrap()
    .as_ref()
    .to_vec();
    Obj::KernelRouteChunkV1 { chunk: Box::new(chunk), signature }
}

#[tokio::test]
async fn flat_v6_native_own_sealed_chunks_survive_generic_saturation_and_refuse_borrowed_actors() {
    use kaspa_consensus_core::palw_kernel_route_v1::*;
    use kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_GROUPS;
    use misaka_palw_kernel::flat_seal6::*;
    use misaka_palw_sdk::flat_candidate_v6::snapshot::FlatSnapshotAnchorV6;
    let f = vector_fixture(true);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let binding = prepare(&mut net, &f).await;
    let (post, public) = flat_vector_post(&net, &f, binding, vec![0, 1], true);
    let id = send_protected_flat_vector(&mut net, &f, binding, 4, &post).await;
    // Anchor comes from authenticated local consensus, separately from remote op-211 JSON.
    let public_snapshot = |net: &Net| {
        let local = net.api().unwrap();
        canonical::Rpc::of(&net.chain).flat_snapshot(FlatSnapshotAnchorV6 {
            tip_daa: net.daa(),
            network_domain: Hash64::from_bytes(local.header.policy.network_domain),
            ruleset_digest: Hash64::from_bytes(local.header.policy.ruleset_digest),
            ledger_root: local.ledger_root(),
            aux_root: local.aux_root(),
        })
    };
    let snapshot = public_snapshot(&net);
    let (_, view) = snapshot.candidate(id, Hash64::from_bytes(f.class), binding).unwrap();
    let proof = flat_first_relation_fault(&view, &public, &f.params);
    let (actor, salt) = (net.kid(7), [71; 64]);
    let reveal = net.route(7, &K::RevealFlatModelProofV6 { vector: id.as_bytes(), salt, proof: proof.clone() });
    let target = PalwKernelChunkTargetV1::FlatProofV6 { vector: id, actor };
    let parts = palw_kernel_chunks_v1(&reveal, net.bond(7), target, 1024).unwrap();
    assert!(parts.len() > 1);
    let missing = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, missing)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 0);
    let seal = flat_proof_seal_v6(&id.as_bytes(), &actor, &salt, &proof).unwrap();
    send(&mut net, 7, &K::SealFlatModelProofV6 { vector: id.as_bytes(), seal }).await;
    net.beat_to(net.api().unwrap().flat_model_seal_v6(&id, &actor).unwrap().seal.mature_daa).await;
    let snapshot = public_snapshot(&net);
    let checked_row = snapshot.reveal_row(id, Hash64::from_bytes(f.class), binding, &net.bond(7)).unwrap();
    assert_eq!(checked_row, net.api().unwrap().flat_model_seal_v6(&id, &actor).unwrap());
    assert!(snapshot.reveal_row(id, Hash64::from_bytes(f.class), binding, &net.bond(6)).is_err());
    let mut borrowed = parts[0].clone();
    borrowed.opener = net.bond(6);
    let borrowed = signed_flat_chunk(&mut net, 6, borrowed);
    net.send(vec![(6, borrowed)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(6)), 0);
    let garbage: Vec<_> = (0..PALW_OBJECT_CHUNK_MAX_GROUPS)
        .map(|n| {
            (1, Obj::ObjectChunk { group: Hash64::from_u64_word(9100 + n as u64), index: 0, count: 16, bytes: vec![n as u8; 1024] })
        })
        .collect();
    net.send(garbage).await;
    for n in 0..PALW_OBJECT_CHUNK_MAX_GROUPS {
        assert!(net.chain.tip_state().1.pending_chunk_group(&Hash64::from_u64_word(9100 + n as u64)).is_some());
    }
    let collateral = net.collateral(4);
    let first = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, first)]).await;
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 1);
    assert_eq!(
        net.api().unwrap().chunk_deposits_of_v1(&net.bond(7)),
        parts.len() as u64 * PALW_KERNEL_CHUNK_DEPOSIT_PER_PART_SOMPI_V1
    );
    let remaining: Vec<_> = parts[1..].iter().map(|p| (7, signed_flat_chunk(&mut net, 7, p.clone()))).collect();
    net.send(remaining).await;
    assert!(net.api().unwrap().flat_model_vector_header_v3(&id).unwrap().refuted);
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 0);
    assert_eq!(net.api().unwrap().chunk_deposits_of_v1(&net.bond(7)), 0);
    assert_eq!(collateral - net.collateral(4), mega(100));
    let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
    assert_eq!(minted + owed, mega(49));
    let again = signed_flat_chunk(&mut net, 7, parts[0].clone());
    net.send(vec![(7, again)]).await;
    let (minted, owed) = minted_and_owed(&net, &net.chain, 7);
    assert_eq!(minted + owed, mega(49));
    assert_eq!(net.api().unwrap().chunk_groups_of_v1(&net.bond(7)), 0);
    net.chain.heartbeat(net.ttpb(), Vec::new()).await;
    let other = net.replay().await;
    net.assert_same(&other, "sealed actor-owned chunks bypass the saturated generic table, settle once and replay");
}

#[tokio::test]
async fn flat_candidate7_native_mature_binding_independent_plan_paid_refusals_and_replay() {
    use misaka_palw_sdk::flat_candidate_v7::prepare_flat_candidate_v7;
    for encoder in [false, true] {
        let f = fixture(encoder);
        let mut net = Net::over_cfg(config(), TestConsensus::new);
        net.beat_to(1).await;
        let (b, id) = binding(&f, f.r);
        send(&mut net, 1, &b).await;
        let prepared =
            prepare_flat_candidate_v7(id.as_bytes(), f.d.digest(), &f.p.encode(), &f.pc, if encoder { 1 } else { 3 }).unwrap();
        let before = net.slashed(0);
        send(&mut net, 0, &prepared.object).await;
        assert!(net.ledger().flat_conformance_classes_v7.is_empty(), "pending binding grants no preparation");
        assert!(net.slashed(0) > before, "invalid metadata consumes its own fee");
        net.beat_to(net.api().unwrap().model_artifact_binding_v2(&id).unwrap().matures_daa).await;
        let mut old_scope = prepared.object.clone();
        let K::RegisterFlatModelCandidateV7 { scope_root, .. } = &mut old_scope else { unreachable!() };
        let e = if encoder { Some(misaka_palw_kernel::seg_encoder::encoder_binding_v1(&f.p).unwrap()) } else { None };
        *scope_root =
            misaka_palw_kernel::flat_scope5::flat_resource_scope_v5(&f.p, if encoder { 1 } else { 3 }, e.as_ref()).unwrap().root();
        let before = net.slashed(0);
        send(&mut net, 0, &old_scope).await;
        assert!(net.ledger().flat_conformance_classes_v7.is_empty());
        assert!(net.slashed(0) > before);
        send(&mut net, 0, &prepared.object).await;
        let ledger = net.ledger();
        let (owner, record) = &ledger.flat_conformance_classes_v7[&prepared.class];
        assert_eq!(*owner, net.kid(0));
        assert_eq!(record.binding, id.as_bytes());
        assert_eq!(record.plan, prepared.plan);
        record.checked().unwrap();
        assert_eq!(net.api().unwrap().model_artifact_candidate_binding_v2(&Hash64::from_bytes(prepared.class)), Some(id));
        assert!(
            net.api().unwrap().kernel_class_record_v1(&Hash64::from_bytes(prepared.class)).is_none(),
            "old execution/candidate reader never adopts7"
        );
        assert!(!ledger.attested_artifacts.contains(&f.pc.root()), "native scope is temporary");
        assert!(ledger.classes.is_empty() && ledger.opv.classes.is_empty() && ledger.claims.is_empty());
        let job = TiledJobV1 {
            class_binding_id: prepared.class,
            prompt_len: 1,
            prompt_root: prompt_root_of_ids_v1(&[0]),
            max_new_tokens: 1,
            decode: DecodeRuleV1::Greedy,
            nonce: [3; 64],
        };
        send(&mut net, 2, &K::PostTiledJob { job }).await;
        assert!(net.ledger().tiled_jobs.is_empty());
        let mut borrowed = prepared.object.clone();
        let K::RegisterFlatModelCandidateV7 { binding, .. } = &mut borrowed else { unreachable!() };
        *binding = [0; 64];
        let before = net.slashed(2);
        send(&mut net, 2, &borrowed).await;
        assert!(net.slashed(2) > before);
        assert_eq!(net.ledger().flat_conformance_classes_v7.len(), 1);
        let replay = net.replay().await;
        net.assert_same(&replay, "native v7 candidate paid refusal, scoped identity and no execution rights");
    }
}

#[tokio::test]
async fn flat_candidate7_native_long_topk_is_registered_as_metadata_without_legacy_execution_rights() {
    use misaka_palw_sdk::flat_candidate_v7::prepare_flat_candidate_v7;
    let n = 10_485_760;
    let max = misaka_palw_tir::program::HISTORY_BOUND_V1_HELD;
    let mut b = ProgramBuilder::new(256, max);
    let scores = b.param("independent.scores", DType::I32, &[n], false);
    let mut pre = b.block("topk", vec![]);
    let x = pre.topk(scores, 0, 2);
    let x = pre.cast(x, DType::I32);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[2])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let post = post.finish(&[]);
    let p = b.finish(pre, vec![], post, 0);
    let d = k2_tir_v4_descriptor();
    // Deliberately metadata only: no checkpoint or parameter values are read.
    let pc = ParamCommitmentsV1 { by_instance: [((0, None), [8; 64])].into() };
    let r = Hash64::from_bytes([9; 64]);
    let id = model_artifact_binding_id_v2(d.digest(), program_root_v1(&p.encode()), r, Hash64::from_bytes(pc.root()));
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    net.beat_to(1).await;
    send(
        &mut net,
        1,
        &K::BindModelArtifactV2 {
            descriptor: d.digest(),
            program_bytes: p.encode(),
            model_inventory_root: r.as_bytes(),
            param_commitments: pc.clone(),
        },
    )
    .await;
    net.beat_to(net.api().unwrap().model_artifact_binding_header_v2(&id).unwrap().matures_daa).await;
    let prepared = prepare_flat_candidate_v7(id.as_bytes(), d.digest(), &p.encode(), &pc, max).unwrap();
    send(&mut net, 0, &prepared.object).await;
    let ledger = net.ledger();
    let v = ledger.flat_conformance_classes_v7[&prepared.class].1.checked().unwrap();
    assert_eq!(v.scope.max_positions, max);
    assert_eq!(v.replay.topk_auxiliary_bytes, 128);
    assert!(v.scope.worst_court_bytes < 1 << 20);
    assert!(ledger.classes.is_empty() && ledger.claims.is_empty() && ledger.opv.admitted.is_empty());
    eprintln!(
        "FLAT_CANDIDATE7_NATIVE_METADATA N={n} max_positions={max} court_bytes={} court_work={} auxiliary_bytes=128 no_weights_or_job_executed=true",
        v.scope.worst_court_bytes, v.scope.worst_court_work
    );
    let replay = net.replay().await;
    net.assert_same(&replay, "long TopK explicitly registered metadata, not legacy execution");
}

#[tokio::test]
async fn flat_candidate7_native_shares_legacy_catalog_and_cannot_spend_reserved_proof_budget() {
    use kaspa_consensus_core::palw_kernel_route_v1::PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1;
    use misaka_palw_sdk::flat_candidate_v7::prepare_flat_candidate_v7;
    let f = fixture(false);
    let mut net = Net::over_cfg(config(), TestConsensus::new);
    let id = prepare(&mut net, &f).await;
    for max in 1..=7 {
        let p = prepare_flat_candidate_v7(id.as_bytes(), f.d.digest(), &f.p.encode(), &f.pc, max).unwrap();
        send(&mut net, 0, &p.object).await;
    }
    assert_eq!(net.ledger().conformance_classes.len(), 1);
    assert_eq!(net.ledger().flat_conformance_classes_v7.len(), 7);
    let extra = prepare_flat_candidate_v7(id.as_bytes(), f.d.digest(), &f.p.encode(), &f.pc, 8).unwrap();
    let before = net.slashed(0);
    send(&mut net, 0, &extra.object).await;
    assert!(net.slashed(0) > before);
    assert!(!net.ledger().flat_conformance_classes_v7.contains_key(&extra.class));
    let mut bad = extra.object.clone();
    let K::RegisterFlatModelCandidateV7 { scope_root, .. } = &mut bad else { unreachable!() };
    scope_root[0] ^= 1;
    let mut objects = Vec::new();
    for _ in 0..8 {
        objects.push((2, net.route(2, &bad)));
    }
    net.send(objects).await;
    let (_, _, work): (u64, u32, u64) = net.api().unwrap().aux_row(PALW_KERNEL_ROUTE_TABLE_BLOCK_BUDGET_V1, &[]).unwrap();
    assert!(work <= net.ledger().policy.admission_work_limit_v1());
    assert!(!net.ledger().flat_conformance_classes_v7.contains_key(&extra.class));
    assert!(net.ledger().classes.is_empty());
    let replay = net.replay().await;
    net.assert_same(&replay, "old/new shared catalog and registration flood preserves proof reserve");
}

mod flat_native_v7;

mod flat_native_v9;
