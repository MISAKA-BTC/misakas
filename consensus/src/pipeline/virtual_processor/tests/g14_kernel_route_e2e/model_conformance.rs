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
    let work = model_artifact_work_v2(h.program_bytes_len as usize, junk.len()).unwrap() + h.max_court_work;
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
