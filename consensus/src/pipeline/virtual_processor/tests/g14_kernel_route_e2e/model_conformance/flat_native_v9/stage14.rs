//! Signed typed stage admission and independent original escrow replay. These
//! small fixtures establish native mechanics, not checkpoint/A6/A8 coverage.
use super::*;
use kaspa_consensus_core::palw_flat_stage_v14::*;
use misaka_palw_kernel::{
    flat_inputs12::{bounded14::*, source14::*},
    flat_lengths7::*,
    flat_quota11::stage14::*,
};
use misaka_palw_tir::program_v2::{InputSource, OutputDecl, TirProgramV2};

struct StageFixture {
    source: FlatStageSourceWireV14,
    template: misaka_palw_kernel::flat_inputs12::FlatInputTemplateV12,
    params: MapParams,
    input: Tensor,
}
fn stage_fixture() -> StageFixture {
    let mut b = ProgramBuilder::new(1, HISTORY_BOUND_V1_SMALL);
    let patches = b.param("public.patches", DType::I32, &[4], false);
    let offset = b.param("original.weight", DType::I8, &[1], false);
    let state = b.fixed_state("previous", DType::I32, &[1], 0, 100, false);
    let mut pre = b.block("pre", vec![]);
    let x = pre.reduce_sum(patches, 0, DType::I64);
    let x = pre.add(x, offset, DType::I64);
    let x = pre.add(x, Ref::State(state), DType::I64);
    let x = pre.clamp(x, 0, 100, DType::I32);
    let x = pre.state_write(state, x);
    let pre = pre.finish(&[x]);
    let mut post = b.block("post", vec![TensorType::fixed(DType::I32, &[1])]);
    let x = post.cast(Ref::CarryIn(0), DType::I32);
    post.commit(x);
    let post = post.finish(&[]);
    let old = b.finish(pre, vec![], post, 0);
    let program =
        TirProgramV2::from_v1_lifting_params(&old, &[(0, InputSource::External { lo: 0, hi: 20 })], OutputDecl::Rows { node: 0 })
            .unwrap();
    let weight = Tensor::new(DType::I8, vec![1], vec![5]).unwrap();
    let params = MapParams { tensors: [((0, None), weight.clone())].into() };
    let pc = ParamCommitmentsV1::of_v3(&params);
    let leaf = artifact_leaf_v1(&PalwArtifactOperandV1 {
        tensor_name: "original.weight".into(),
        layer: None,
        row_start: 0,
        bytes: weight.to_le_bytes(),
    });
    let source = FlatStageSourceWireV14 {
        version: 14,
        program_bytes: program.encode(),
        inventory_root: artifact_root_v1(&[leaf]).unwrap().as_bytes(),
        source_parameters: pc.clone(),
    };
    let lengths = FlatLengthTemplateV7 {
        version: 7,
        maxima: FlatJobLengthsV7 { token: 0, patch: 4, audio: 0 },
        max_program_bytes: program.v1_view().encode(),
        bindings: vec![FlatShapeBindingV7 {
            slot: FlatShapeSlotV7::Parameter { parameter: 1, axis: 0 },
            length: FlatLengthAxisV7::Patch,
        }],
    };
    let template =
        misaka_palw_kernel::flat_inputs12::FlatInputTemplateV12 { program, lengths, max_positions: 3, source_parameters: pc.root() };
    StageFixture { source, template, params, input: Tensor::new(DType::I32, vec![3], vec![1, 2, 3]).unwrap() }
}
fn stage_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut cfg, bundle, premine, floats) = config();
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        cfg.params.net.to_string().as_bytes(),
        Some(cfg.params.genesis.hash),
    );
    cfg.params.palw_stage_execution_v14 =
        Some(PalwStageExecutionFenceV14::unapproved_probe_at(ForkActivation::new(0), domain.as_bytes()));
    (cfg, bundle, premine, floats)
}
async fn action(net: &mut Net, actor: usize, kind: u8, body: Vec<u8>) {
    send(net, actor, &K::StageExecutionV14 { action: kind, body }).await;
}
async fn admitted_stage(net: &mut Net, f: &StageFixture) -> (Digest, CheckedStageClassV14) {
    net.beat_to(1).await;
    action(net, 1, STAGE_ACTION_BIND_V14, borsh::to_vec(&f.source).unwrap()).await;
    let source = f.source.check().unwrap().id();
    let sh = net.api().unwrap().stage_source_header_v14(&source).expect("native original real-PC source");
    net.beat_to(sh.matures_daa).await;
    let policies = net.config.params.palw_stage_execution_v14.as_ref().unwrap().policies.clone();
    let wire = FlatStageClassWireV14 {
        version: 14,
        source_binding: source,
        length_template_bytes: f.template.lengths.to_bytes().unwrap(),
        max_positions: 3,
        policy_root: policies.root(),
    };
    let class = wire.check(&f.source).unwrap();
    action(net, 0, STAGE_ACTION_REGISTER_V14, borsh::to_vec(&wire).unwrap()).await;
    assert!(net.api().unwrap().stage_class_header_v14(&class.root()).is_some());
    (class.root(), class)
}
fn preparation(net: &Net, f: &StageFixture, class: &CheckedStageClassV14) -> (StageAcceptRequestV14, StageFundRequestV14) {
    let job = FlatStageJobV14 {
        version: 14,
        template: f.template.root().unwrap(),
        requester: net.kid(2),
        producer: net.kid(0),
        nonce: misaka_palw_kernel::flat_execution11::nonce_bytes_v11(
            net.api()
                .unwrap()
                .execution_nonce_counter_v11(&net.chain.tip_state().1.bonds[&net.bond(2)].operator_id.as_bytes())
                .unwrap()
                .next_nonce,
        ),
        actual: FlatJobLengthsV7 { token: 0, patch: 3, audio: 0 },
        positions: 3,
        input_roots: vec![misaka_palw_kernel::merkle3::tensor_commitment_v3(&f.input)],
    };
    let actual = class.check_job(&job).unwrap();
    let plan = derive_stage_quota_v14(class, &actual).unwrap();
    let p = net.config.params.palw_stage_execution_v14.as_ref().unwrap().policies.clone();
    let accept = StageAcceptRequestV14 {
        version: 14,
        class: class.root(),
        requester: net.bond(2),
        reward: 20,
        expires_daa: net.daa() + p.execution.execution.horizons.commit_window,
        source: class.source().id(),
        policy: p.root(),
        admission: actual.admission().root(),
        quota: plan.root(),
        job_bytes: borsh::to_vec(&job).unwrap(),
    };
    let fund = StageFundRequestV14 {
        version: 14,
        class: class.root(),
        producer: net.bond(0),
        reward: 20,
        acceptance: [0; 64],
        job_bytes: borsh::to_vec(&job).unwrap(),
        input_images: vec![f.input.to_le_bytes()],
    };
    (accept, fund)
}
async fn funded_stage(net: &mut Net, f: &StageFixture, class: &CheckedStageClassV14) -> Digest {
    let (old, mut fund) = preparation(net, f, class);
    let actual = class.check_job(&FlatStageJobV14::from_bytes(&fund.job_bytes).unwrap()).unwrap();
    let plan = derive_stage_quota_v14(class, &actual).unwrap();
    let p = net.config.params.palw_stage_execution_v14.as_ref().unwrap().policies.clone();
    let source_plan = derive_stage_source_plan_v14(class, &plan, &p.execution.quota, &p.execution.discovery).unwrap();
    let trace_plan = derive_stage_trace_plan_v14(class, &plan, &p.execution.quota).unwrap();
    let fee_plan = stage_delivery_fee_plan_v20(&net.ledger().policy, &p, &old, source_plan.requests()).unwrap();
    let request = StageAcceptRequestV18 {
        version: 18,
        preparation_bytes: borsh::to_vec(&old).unwrap(),
        source_plan: source_plan.root(),
        trace_plan: trace_plan.root(),
        fee_plan: fee_plan.root(),
        availability_terms: stage_availability_terms_v18(&p, &old, source_plan.root(), trace_plan.root(), fee_plan.root()),
    };
    action(net, 0, STAGE_ACTION_ACCEPT_V18, borsh::to_vec(&request).unwrap()).await;
    let ids = net.api().unwrap().stage_owner_ids_v14(8, &net.bond(0)).unwrap();
    fund.acceptance =
        ids.into_iter().find(|id| net.api().unwrap().stage_acceptance_v18(id).is_some()).expect("explicit producer consent18");
    action(net, 2, STAGE_ACTION_FUND_V14, borsh::to_vec(&fund).unwrap()).await;
    let jobs = net.api().unwrap().stage_owner_ids_v14(6, &net.bond(2)).unwrap();
    assert_eq!(jobs.len(), 1);
    let id = jobs[0];
    assert!(net.api().unwrap().stage_acceptance_v18(&fund.acceptance).is_none());
    action(net, 2, STAGE_ACTION_FUND_V14, borsh::to_vec(&fund).unwrap()).await;
    assert_eq!(net.api().unwrap().stage_owner_ids_v14(6, &net.bond(2)).unwrap(), jobs);
    id
}
#[tokio::test]
async fn stage14_native_consent18_prevents_victim_reservation_and_original_refund_replays_once() {
    let f = stage_fixture();
    let mut net = Net::over_cfg(stage_config(), TestConsensus::new);
    let (_, class) = admitted_stage(&mut net, &f).await;
    let (old, mut fund) = preparation(&net, &f, &class);
    action(&mut net, 2, STAGE_ACTION_ACCEPT_V14, borsh::to_vec(&old).unwrap()).await;
    assert!(net.api().unwrap().stage_owner_ids_v14(8, &net.bond(0)).unwrap().is_empty());
    action(&mut net, 2, STAGE_ACTION_FUND_V14, borsh::to_vec(&fund).unwrap()).await;
    assert!(net.api().unwrap().stage_owner_ids_v14(7, &net.bond(0)).unwrap().is_empty());
    let (old, _) = preparation(&net, &f, &class);
    action(&mut net, 0, STAGE_ACTION_ACCEPT_V14, borsh::to_vec(&old).unwrap()).await;
    fund.acceptance = net.api().unwrap().stage_owner_ids_v14(8, &net.bond(0)).unwrap()[0];
    action(&mut net, 2, STAGE_ACTION_FUND_V14, borsh::to_vec(&fund).unwrap()).await;
    assert!(net.api().unwrap().stage_owner_ids_v14(7, &net.bond(0)).unwrap().is_empty(), "13 metadata cannot reserve16/17 guarantees");
    let id = funded_stage(&mut net, &f, &class).await;
    let row = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, row.header.book_root).unwrap();
    assert_eq!(book.escrow_remaining(), 20);
    assert_eq!(book.original_funded_root(), row.header.original_book_root);
    action(&mut net, 2, STAGE_ACTION_CLOSE_V14, id.to_vec()).await;
    assert_eq!(net.api().unwrap().stage_job_header_v14(&id).unwrap().terminal, StageTerminalV14::Funded);
    net.beat_to(book.deadline_daa() + 1).await;
    action(&mut net, 3, STAGE_ACTION_CLOSE_V14, id.to_vec()).await;
    assert_eq!(net.api().unwrap().stage_job_header_v14(&id).unwrap().terminal, StageTerminalV14::Refunded);
    let paid = minted_and_owed(&net, &net.chain, 2);
    assert_eq!(paid.0 + paid.1, 20);
    action(&mut net, 3, STAGE_ACTION_CLOSE_V14, id.to_vec()).await;
    action(&mut net, 3, STAGE_ACTION_COLLECT_V14, id.to_vec()).await;
    assert!(net.api().unwrap().stage_job_header_v14(&id).is_some(), "old guarantee cannot be collected within its original interval");
    net.beat_to(1001).await;
    action(&mut net, 3, STAGE_ACTION_COLLECT_V14, id.to_vec()).await;
    assert!(net.api().unwrap().stage_job_header_v14(&id).is_none());
    let tombstone: NativeStageCollectedV14 = net.api().unwrap().aux_row(STAGE_TABLE_V14, &stage_key_v14(17, &id)).unwrap();
    assert_eq!(tombstone.original_nonce, book.job().nonce);
    assert_eq!(tombstone.consent18, book.acceptance());
    assert!(net.api().unwrap().stage_owner_ids_v14(6, &net.bond(2)).unwrap().is_empty());
    assert!(net.api().unwrap().stage_owner_ids_v14(7, &net.bond(0)).unwrap().is_empty());
    let second = funded_stage(&mut net, &f, &class).await;
    assert_ne!(second, id);
    let operator = net.chain.tip_state().1.bonds[&net.bond(2)].operator_id.as_bytes();
    assert_eq!(net.api().unwrap().execution_nonce_counter_v11(&operator).unwrap().next_nonce, 2);
    let replay = net.replay().await;
    net.assert_same(&replay, "typed original18 consent, shared nonce, principal refund once");
    let paid = minted_and_owed(&net, &net.chain, 2);
    assert_eq!(paid.0 + paid.1, 20);
}

#[tokio::test]
async fn stage14_native_rows_commit_authenticates_tensor_output_and_closed_final_replays_independently() {
    let f = stage_fixture();
    let mut net = Net::over_cfg(stage_config(), TestConsensus::new);
    let (_, class) = admitted_stage(&mut net, &f).await;
    let id = funded_stage(&mut net, &f, &class).await;
    let row = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, row.header.book_root).unwrap();
    let actual = class.check_job(book.job()).unwrap();
    let plan = derive_stage_quota_v14(&class, &actual).unwrap();
    struct Combined<'a>(&'a MapParams, &'a Tensor);
    impl misaka_palw_tir::ParamSource for Combined<'_> {
        fn param(&self, id: u16, layer: Option<u16>) -> Option<Tensor> {
            if id == 1 && layer.is_none() { Some(self.1.clone()) } else { self.0.tensors.get(&(id, layer)).cloned() }
        }
    }
    let program = actual.program().v1_view();
    let trace = misaka_palw_kernel::trace::trace_v1(&program, &Combined(&f.params, &f.input), &[0; 3]).unwrap();
    let commitments = SegmentedCommitmentsV1::new(
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
    let post = (program.occurrences().len() - 1) as u16;
    let claim = StageTraceV14 {
        version: 14,
        class: class.root(),
        job_id: id,
        job_input: book.job().root(),
        admission: actual.admission().root(),
        quota: plan.root(),
        segment_roots: commitments.segment_roots(),
        output_openings: (0..3).map(|p| commitments.node_opening(p, post, program.logits).unwrap()).collect(),
    };
    action(&mut net, 0, STAGE_ACTION_COMMIT_V14, borsh::to_vec(&claim).unwrap()).await;
    let committed = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    assert_eq!(committed.header.terminal, StageTerminalV14::Challengeable);
    assert_eq!(committed.header.producer_reserved, 130);
    let book = StageJobBookV14::from_rooted_bytes(&committed.book_bytes, committed.header.book_root).unwrap();
    assert_eq!(book.claim_root(), Some(claim.root()));
    assert_eq!(book.escrow_remaining(), 20);
    net.beat_to(book.deadline_daa() + 1).await;
    action(&mut net, 0, STAGE_ACTION_CLOSE_V14, id.to_vec()).await;
    assert_eq!(net.api().unwrap().stage_job_header_v14(&id).unwrap().terminal, StageTerminalV14::Challengeable);
    assert_eq!(minted_and_owed(&net, &net.chain, 0), (0, 0));
    let replay = net.replay().await;
    net.assert_same(&replay, "typed full-graph authenticated Rows output and incomplete Final refusal");
}
