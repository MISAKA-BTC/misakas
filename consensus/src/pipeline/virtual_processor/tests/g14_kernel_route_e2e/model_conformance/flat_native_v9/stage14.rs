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

/// **Everything paid or owed to card `card`'s payee**: its unspent coinbase outputs plus EVERY pending payout naming its payload. The
/// stage principal refund rides its own fixed queue (`stage-original-principal-refund/v14`, so a public Close signer cannot redirect
/// it), not the payee's kernel payout key that [`minted_and_owed`] reads; and when a queue is minted between two reads only the split
/// moves. The sum is the money.
fn paid_to(net: &Net, card: usize) -> u64 {
    let (minted, _) = minted_and_owed(net, &net.chain, card);
    let state = net.chain.tip_state().1;
    let payee = state.bond(&net.bond(card)).expect("the bond").payout_payload;
    minted + state.pending_payouts_iter().filter(|(_, p)| p.payload == payee).map(|(_, p)| p.amount).sum::<u64>()
}

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
                .execution_nonce_counter_v11(&net.chain.tip_state().1.bond(&net.bond(2)).unwrap().operator_id.as_bytes())
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
    assert_eq!(paid_to(&net, 2), 20);
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
    let operator = net.chain.tip_state().1.bond(&net.bond(2)).unwrap().operator_id.as_bytes();
    assert_eq!(net.api().unwrap().execution_nonce_counter_v11(&operator).unwrap().next_nonce, 2);
    let replay = net.replay().await;
    net.assert_same(&replay, "typed original18 consent, shared nonce, principal refund once");
    assert_eq!(paid_to(&net, 2), 20);
}

fn stage_claim(f: &StageFixture, class: &CheckedStageClassV14, book: &StageJobBookV14) -> StageTraceV14 {
    let actual = class.check_job(book.job()).unwrap();
    let plan = derive_stage_quota_v14(class, &actual).unwrap();
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
    StageTraceV14 {
        version: 14,
        class: class.root(),
        job_id: book.id(),
        job_input: book.job().root(),
        admission: actual.admission().root(),
        quota: plan.root(),
        segment_roots: commitments.segment_roots(),
        output_openings: (0..3).map(|p| commitments.node_opening(p, post, program.logits).unwrap()).collect(),
    }
}

#[tokio::test]
async fn stage14_native_rows_commit_authenticates_tensor_output_and_closed_final_replays_independently() {
    let f = stage_fixture();
    let mut net = Net::over_cfg(stage_config(), TestConsensus::new);
    let (_, class) = admitted_stage(&mut net, &f).await;
    let id = funded_stage(&mut net, &f, &class).await;
    let row = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, row.header.book_root).unwrap();
    let claim = stage_claim(&f, &class, &book);
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

#[tokio::test]
async fn stage14_original_real_pc_delivery_then_default_collects_original_coins_once_and_replays() {
    use kaspa_consensus_core::palw_flat_execution_v11::{
        EXECUTION_DEMAND_SLOT_TABLE_V11, EXECUTION_QUOTA_TABLE_V11, NativeSourceRequestV16,
    };
    use misaka_palw_kernel::flat_quota11::source16::opening::{AnswerV16, HeaderV16};
    let f = stage_fixture();
    let mut net = Net::over_cfg(stage_config(), TestConsensus::new);
    let (_, class) = admitted_stage(&mut net, &f).await;
    let id = funded_stage(&mut net, &f, &class).await;
    let row = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    let original = StageJobBookV14::from_rooted_bytes(&row.book_bytes, row.header.book_root).unwrap();
    let claim = stage_claim(&f, &class, &original);
    action(&mut net, 0, STAGE_ACTION_COMMIT_V14, borsh::to_vec(&claim).unwrap()).await;
    let mut request = NativeSourceRequestV16 {
        job: id,
        param: 1,
        layer_code: 0,
        kind: 0,
        axis: 0,
        level: 0,
        index: 0,
        lane: 0,
        entitlement: [0; 64],
        slot: 0,
    };
    action(&mut net, 3, 7, borsh::to_vec(&request).unwrap()).await;
    let slot = borsh::to_vec(&(16u8, net.bond(3), 0u8)).unwrap();
    assert!(
        net.api().unwrap().aux_row::<Digest>(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot).is_none(),
        "external input PC cannot be acquired as original source weight"
    );
    request.param = 0;
    action(&mut net, 3, 7, borsh::to_vec(&request).unwrap()).await;
    let pending: Digest = net.api().unwrap().aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot).unwrap();
    assert_eq!(net.api().unwrap().stage_source_answer_owner_v14(&pending, net.daa()), Some(net.bond(0)));
    assert_eq!(
        net.api().unwrap().execution_source_answer_owner_v16(&pending, net.daa()),
        None,
        "typed source demand cannot enter text settlement namespace"
    );
    let weight = &f.params.tensors[&(0, None)];
    let trees = misaka_palw_kernel::merkle3::TreesV3::of(weight);
    let payload = borsh::to_vec(&AnswerV16::Header(HeaderV16::of(weight, &trees).unwrap())).unwrap();
    let mut answer = pending.to_vec();
    answer.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    answer.extend_from_slice(&payload);
    action(&mut net, 0, 8, answer).await;
    assert!(net.api().unwrap().execution_source_demand_v16(&pending).unwrap().settled);
    let fees: NativeStageFeesV20 = net.api().unwrap().aux_row(STAGE_TABLE_V14, &stage_key_v14(12, &id)).unwrap();
    assert_eq!(fees.earned, fees.plan.source_unit_fee.min(fees.plan.maximum));
    assert!(20 + fees.earned <= fees.plan.reward + fees.plan.maximum);
    request.kind = 2;
    request.axis = misaka_palw_kernel::merkle::AXIS_ROW;
    action(&mut net, 3, 7, borsh::to_vec(&request).unwrap()).await;
    let pending: Digest = net.api().unwrap().aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &slot).unwrap();
    let demand = net.api().unwrap().execution_source_demand_v16(&pending).unwrap();
    let before = net.chain.tip_state().1.bond(&net.bond(0)).unwrap().collateral;
    net.beat_to(demand.deadline + 1).await;
    action(&mut net, 1, 9, pending.to_vec()).await;
    let settled = net.api().unwrap().stage_job_header_v14(&id).unwrap();
    assert_eq!(settled.terminal, StageTerminalV14::Convicted);
    assert_eq!(net.chain.tip_state().1.bond(&net.bond(0)).unwrap().collateral, before - 130);
    assert_eq!(paid_to(&net, 2), 20, "the principal refund, once");
    let outcome: StageSettlementV14 = net.api().unwrap().aux_row(STAGE_TABLE_V14, &stage_key_v14(13, &id)).unwrap();
    assert_eq!(paid_to(&net, 3), outcome.finder_credit);
    let (_, subject): (Digest, misaka_palw_kernel::flat_quota11::QuotaSubjectSummaryV11) =
        net.api().unwrap().aux_row(EXECUTION_QUOTA_TABLE_V11, &borsh::to_vec(&(2u8, id)).unwrap()).unwrap();
    assert!(subject.closed);
    action(&mut net, 1, 9, pending.to_vec()).await;
    assert_eq!(net.chain.tip_state().1.bond(&net.bond(0)).unwrap().collateral, before - 130);
    net.beat_to(1001).await;
    action(&mut net, 1, STAGE_ACTION_COLLECT_V14, id.to_vec()).await;
    assert!(net.api().unwrap().stage_job_header_v14(&id).is_none());
    let replay = net.replay().await;
    net.assert_same(&replay, "typed original bound-PC fee/default actual130slash20refund once, genuine quota close and publicGC");
}

#[tokio::test]
async fn stage14_original_paid_header_court_and_public_proof_retirement_replay_once() {
    use kaspa_consensus_core::palw_flat_execution_v11::NativeSourceHeaderFaultV16;
    use misaka_palw_kernel::flat_quota11::source16::opening::HeaderV16;
    let mut f = stage_fixture();
    // Producer signs an opaque PC for [2], while original V2/R declares [1].
    // Header authenticity, not unavailable external R, establishes the fault.
    let false_weight = Tensor::new(DType::I8, vec![2], vec![5, 6]).unwrap();
    let false_params = MapParams { tensors: [((0, None), false_weight.clone())].into() };
    f.source.source_parameters = ParamCommitmentsV1::of_v3(&false_params);
    f.template.source_parameters = f.source.source_parameters.root();
    let mut net = Net::over_cfg(stage_config(), TestConsensus::new);
    let (_, class) = admitted_stage(&mut net, &f).await;
    let id = funded_stage(&mut net, &f, &class).await;
    let row = net.api().unwrap().stage_job_row_v14(&id).unwrap();
    let book = StageJobBookV14::from_rooted_bytes(&row.book_bytes, row.header.book_root).unwrap();
    let claim = stage_claim(&f, &class, &book);
    action(&mut net, 0, STAGE_ACTION_COMMIT_V14, borsh::to_vec(&claim).unwrap()).await;
    let fault = NativeSourceHeaderFaultV16 {
        param: 0,
        layer_code: 0,
        header: HeaderV16::of(&false_weight, &misaka_palw_kernel::merkle3::TreesV3::of(&false_weight)).unwrap(),
    };
    let payload = borsh::to_vec(&fault).unwrap();
    let mut filing = 14u16.to_le_bytes().to_vec();
    filing.push(2);
    filing.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    filing.extend_from_slice(&payload);
    let proof = stage_proof_root_v14(&filing);
    let salt = [51; 64];
    let binding = stage_proof_seal_v14(proof, salt, net.kid(3), id, claim.root());
    let mut body = id.to_vec();
    body.extend_from_slice(&proof);
    body.extend_from_slice(&binding);
    action(&mut net, 3, 16, body).await;
    let pursuit: NativeStagePursuitV14 = net.api().unwrap().aux_row(STAGE_TABLE_V14, &stage_key_v14(16, &id)).unwrap();
    let seal = *pursuit.seal_keys.iter().find(|k| **k != [0; 64]).unwrap();
    let own = net.api().unwrap().stage_seal_v14(&seal).unwrap();
    assert!(own.original_debit > 0);
    net.beat_to(own.matures_daa).await;
    let producer = net.collateral(0);
    let mut reveal = seal.to_vec();
    reveal.extend_from_slice(&salt);
    reveal.extend_from_slice(&(filing.len() as u32).to_le_bytes());
    reveal.extend_from_slice(&filing);
    action(&mut net, 3, 17, reveal.clone()).await;
    assert_eq!(net.api().unwrap().stage_job_header_v14(&id).unwrap().terminal, StageTerminalV14::Convicted);
    assert_eq!(net.collateral(0), producer - 130);
    assert!(net.api().unwrap().stage_source_refuted_relation_v14(&class.source().relation_root()));
    let expected = paid_to(&net, 3);
    action(&mut net, 3, 17, reveal).await;
    assert_eq!(paid_to(&net, 3), expected, "a replayed reveal pays nothing more (only the minted/owed split may move)");
    net.beat_to(1001).await;
    action(&mut net, 1, 18, seal.to_vec()).await;
    assert!(net.api().unwrap().stage_seal_v14(&seal).is_none());
    action(&mut net, 1, STAGE_ACTION_COLLECT_V14, id.to_vec()).await;
    assert!(net.api().unwrap().stage_job_header_v14(&id).is_none());
    let replay = net.replay().await;
    net.assert_same(&replay, "typed originalpaid realPCheader court actual130slash20refund once and public proof/job retirement");
}
