use super::*;
use misaka_palw_kernel::flat_quota11::source16::{
    SourceObligationV16,
    opening::{AnswerV16, HeaderV16},
};

async fn committed(net: &mut Net, f: &Sparse9) -> Digest {
    let (id, class, bind, record) = funded(net, f).await;
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    let fixed = net.api().unwrap().execution_class_header_v11(&class).unwrap().checked_fixed_header().unwrap();
    let book = row.restore_book(&fixed).unwrap();
    let (body, _) = statement(net, f, bind, fixed.record().candidate9_id, &record, None);
    action(
        net,
        0,
        EXECUTION_ACTION_COMMIT_V11,
        borsh::to_vec(&FlatExecutionPostV11 {
            version: 11,
            execution_class: class,
            job_id: id,
            requester_nonce: book.job().nonce,
            job_input_root: book.job().input.root(),
            actual_scope9_root: book.job().actual_scope9_root,
            actual_resource10_root: book.job().actual_resource10_root,
            quota_plan_root: book.job().quota_plan_root,
            body,
        })
        .unwrap(),
    )
    .await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal, ExecutionTerminalV11::Challengeable);
    id
}
async fn ask_header(net: &mut Net, job: Digest, param: u16, slot: u8) -> Digest {
    let request = NativeSourceRequestV16 {
        job,
        lane: 0,
        entitlement: [0; 64],
        slot,
        param,
        layer_code: 0,
        kind: 0,
        axis: 0,
        level: 0,
        index: 0,
    };
    action(net, 7, EXECUTION_ACTION_SOURCE_REQUEST_V16, borsh::to_vec(&request).unwrap()).await;
    net.api()
        .unwrap()
        .aux_row(EXECUTION_DEMAND_SLOT_TABLE_V11, &borsh::to_vec(&(16u8, net.bond(7), slot)).unwrap())
        .expect("original own source pending unit admitted")
}
fn source_row(net: &Net, job: Digest) -> SourceObligationV16 {
    let (root, row): (Digest, SourceObligationV16) =
        net.api().unwrap().aux_row(EXECUTION_QUOTA_TABLE_V11, &borsh::to_vec(&(16u8, job)).unwrap()).unwrap();
    assert_eq!(root, row.root());
    row
}
fn config_source() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut cfg, b, p, f) = native_config();
    // This test checks paid native malformed callbacks, not complete pursuit or
    // Final. A finite answer window accommodates the independent node carriers.
    cfg.params.palw_funded_execution_v11.as_mut().unwrap().policies.execution.horizons.da_answer = 20;
    (cfg, b, p, f)
}
#[tokio::test]
async fn execution11_native_source15_signed_nested_failures_spend_original_attempts_without_fee_and_replay() {
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_source(), TestConsensus::new);
    let job = committed(&mut net, &f).await;
    let pending = ask_header(&mut net, job, 0, 0).await;
    let demand = net.api().unwrap().execution_source_demand_v16(&pending).unwrap();
    let before = source_row(&net, job);
    let mut hostile = pending.to_vec();
    hostile.extend_from_slice(&u32::MAX.to_le_bytes());
    // Wrong owner is rejected by fixed preflight before source reservation.
    action(&mut net, 6, EXECUTION_ACTION_SOURCE_ANSWER_V16, hostile.clone()).await;
    assert_eq!(source_row(&net, job), before);
    let requester_before = net.collateral(7);
    let producer_before = minted_and_owed(&net, &net.chain, 0);
    for expected in [1, 2] {
        action(&mut net, 0, EXECUTION_ACTION_SOURCE_ANSWER_V16, hostile.clone()).await;
        let current = source_row(&net, job);
        assert_eq!(current.requests_admitted, before.requests_admitted);
        assert_eq!(current.answers_used, before.answers_used + expected);
        assert_ne!(current.guarantee_remaining, before.guarantee_remaining);
        assert_eq!(net.api().unwrap().execution_source_demand_v16(&pending).unwrap().attempts, expected as u8);
        assert!(!net.api().unwrap().execution_source_demand_v16(&pending).unwrap().settled);
    }
    let exhausted = source_row(&net, job);
    action(&mut net, 0, EXECUTION_ACTION_SOURCE_ANSWER_V16, hostile).await;
    assert_eq!(source_row(&net, job), exhausted, "third invalid packet gets no fresh original attempt");
    assert_eq!(net.collateral(7), requester_before, "malformed producer callbacks do not collect the requester's delivery bond");
    assert_eq!(minted_and_owed(&net, &net.chain, 0), producer_before, "undelivered PC bytes earn no producer DA fee");
    assert!(net.api().unwrap().execution_source_answer_owner_v16(&pending, net.daa()).is_none());
    assert_eq!(demand.pc, f.pc.by_instance[&(0, None)]);
    let replay = net.replay().await;
    net.assert_same(&replay, "signed original Source15 hostile nested lengths retain paid attempts and no fee");
}
#[tokio::test]
async fn execution11_native_original_pc_header_delivery_capped_income_once_and_independent_replay() {
    let f = sparse9(false);
    let mut net = Net::over_cfg(config_source(), TestConsensus::new);
    let job = committed(&mut net, &f).await;
    let pending = ask_header(&mut net, job, 0, 0).await;
    let d = net.api().unwrap().execution_source_demand_v16(&pending).unwrap();
    let t = &f.params.tensors[&(0, None)];
    let payload = borsh::to_vec(&AnswerV16::Header(HeaderV16::of(t, &TreesV3::of(t)).unwrap())).unwrap();
    let body = borsh::to_vec(&(pending, payload)).unwrap();
    let old = source_row(&net, job);
    let requester = net.collateral(7);
    let producer = minted_and_owed(&net, &net.chain, 0);
    action(&mut net, 0, EXECUTION_ACTION_SOURCE_ANSWER_V16, body.clone()).await;
    let done = net.api().unwrap().execution_source_demand_v16(&pending).unwrap();
    assert!(done.settled);
    assert_eq!(done.attempts, 1);
    assert_eq!(source_row(&net, job).answers_used, old.answers_used + 1);
    assert_eq!(requester - net.collateral(7), d.bond);
    let fees: NativeExecutionFeesV19 =
        net.api().unwrap().aux_row(EXECUTION_CREDIT_TABLE_V11, &borsh::to_vec(&(19u8, job)).unwrap()).unwrap();
    assert_eq!(fees.earned, d.bond.min(fees.plan.maximum));
    assert!(fees.earned + fees.plan.reward <= 100);
    let now = minted_and_owed(&net, &net.chain, 0);
    assert_eq!(now.0 + now.1 - producer.0 - producer.1, fees.earned);
    action(&mut net, 0, EXECUTION_ACTION_SOURCE_ANSWER_V16, body).await;
    assert_eq!(source_row(&net, job).answers_used, old.answers_used + 1);
    assert_eq!(minted_and_owed(&net, &net.chain, 0), now);
    let replay = net.replay().await;
    net.assert_same(&replay, "exact original PC header, original requester fee debit, once-only capped income");
}
