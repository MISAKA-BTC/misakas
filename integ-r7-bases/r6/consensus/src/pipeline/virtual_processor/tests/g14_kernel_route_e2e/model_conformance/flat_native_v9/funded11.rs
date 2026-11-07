//! Signed native original escrow/nonce/quota mechanics on independently replayed
//! nodes. Small synthetic fixtures are not checkpoint/model coverage evidence.
use super::*;
mod chunks;
mod public_filing;
mod text29;
mod owning39;
mod real_checkpoint;
mod source16;
use kaspa_consensus_core::palw_flat_execution_v11::*;
use kaspa_consensus_core::palw_funded_execution_v11::PalwFundedExecutionFenceV11;
use misaka_palw_kernel::flat_execution11::{ExecutionTerminalV11, nonce_bytes_v11};
fn native_config() -> (Config, PalwConsensusParamsV2, Premine, Premine) {
    let (mut cfg, bundle, premine, floats) = config();
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        cfg.params.net.to_string().as_bytes(),
        Some(cfg.params.genesis.hash),
    );
    cfg.params.palw_funded_execution_v11 =
        Some(PalwFundedExecutionFenceV11::unapproved_probe_at(ForkActivation::new(0), domain.as_bytes()));
    (cfg, bundle, premine, floats)
}
async fn action(net: &mut Net, actor: usize, kind: u8, body: Vec<u8>) {
    send(net, actor, &K::FundedExecutionV11 { action: kind, body }).await;
}
/// The node's own mempool refuses this signed object (the processor's fixed preflight
/// rejects it before any fold). The card's funding chain is restored afterwards.
fn mempool_refuses(net: &mut Net, actor: usize, kind: u8, body: Vec<u8>) -> bool {
    let saved = net.funding[actor].clone();
    let object = net.route(actor, &K::FundedExecutionV11 { action: kind, body });
    let tx = net.carrier(actor, &object);
    let refused = net.mempool(&tx).is_err();
    net.funding[actor] = saved;
    refused
}
/// Minted outputs plus EVERY pending queue owed to the card's payee — the v13 design keeps
/// original principal refunds in a per-job queue, not the pooled kernel payout key.
fn payee_minted_and_owed_all_queues(net: &Net, card: usize) -> u64 {
    let (minted, _) = minted_and_owed(net, &net.chain, card);
    let state = net.chain.tip_state().1;
    let payload = state.bond(&net.bond(card)).expect("the bond").payout_payload;
    minted + state.pending_payouts_iter().filter(|(_, p)| p.payload == payload).map(|(_, p)| p.amount).sum::<u64>()
}
async fn funded(
    net: &mut Net,
    f: &Sparse9,
) -> (Digest, Digest, Hash64, misaka_palw_kernel::ledger::flat_candidate9::FlatCandidateRecordV9) {
    let (bind, candidate, record) = admitted(net, f).await;
    action(net, 0, EXECUTION_ACTION_REGISTER_V11, borsh::to_vec(&candidate).unwrap()).await;
    let class = net
        .api()
        .unwrap()
        .aux
        .iter()
        .filter(|((table, _), _)| *table == EXECUTION_CLASS_TABLE_V11)
        .find_map(|(_, bytes)| native_execution_class_header_v11(bytes).ok().map(|(h, _)| h.record.id()))
        .expect("native class admitted");
    let mut request = FlatExecutionFundV11 {
        version: 11,
        execution_class: class,
        preparation: [0; 64],
        producer: net.kid(0),
        nonce: nonce_bytes_v11(0),
        reward: 20,
        prompt: vec![0, 1],
        generated_count: 1,
    };
    action(net, 2, EXECUTION_ACTION_PREPARE_V11, borsh::to_vec(&request).unwrap()).await;
    request.preparation = net
        .api()
        .unwrap()
        .aux
        .iter()
        .filter(|((table, _), _)| *table == EXECUTION_PREPARATION_TABLE_V11)
        .find_map(|(_, raw)| borsh::from_slice::<NativeExecutionPreparationV11>(raw).ok().map(|p| p.id()))
        .expect("native exact preparation");
    action(net, 0, EXECUTION_ACTION_ACCEPT_V18, borsh::to_vec(&request.preparation).unwrap()).await;
    action(net, 2, EXECUTION_ACTION_FUND_V11, borsh::to_vec(&request).unwrap()).await;
    let ids: Vec<Digest> = net
        .api()
        .unwrap()
        .aux_row(EXECUTION_JOB_OWNER_TABLE_V11, &borsh::to_vec(&net.bond(2)).unwrap())
        .expect("original funded native requester catalog");
    assert_eq!(ids.len(), 1);
    (ids[0], class, bind, record)
}
#[tokio::test]
async fn execution11_native_original_escrow_monotone_nonce_unclaimed_refund_once_and_independent_ibd() {
    let f = sparse9(false);
    let mut net = Net::over_cfg(native_config(), TestConsensus::new);
    let (id, _, _, _) = funded(&mut net, &f).await;
    let original = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    assert_eq!(original.header.terminal, ExecutionTerminalV11::Funded);
    let op = net.chain.tip_state().1.bond(&net.bond(2)).unwrap().operator_id.as_bytes();
    assert_eq!(net.api().unwrap().execution_nonce_counter_v11(&op).unwrap().next_nonce, 1);
    let class = net.api().unwrap().execution_class_header_v11(&original.header.class_id).unwrap().checked_fixed_header().unwrap();
    let book = original.restore_book(&class).unwrap();
    assert_eq!(book.job().escrow_original_debit, 20);
    assert_eq!(book.escrow_remaining(), 20);
    action(&mut net, 2, EXECUTION_ACTION_CLOSE_V11, borsh::to_vec(&id).unwrap()).await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal, ExecutionTerminalV11::Funded);
    net.beat_to(book.job().commit_deadline + 1).await;
    action(&mut net, 2, EXECUTION_ACTION_CLOSE_V11, borsh::to_vec(&id).unwrap()).await;
    let terminal = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    assert_eq!(terminal.header.terminal, ExecutionTerminalV11::Cancelled);
    assert_eq!(terminal.restore_book(&class).unwrap().escrow_remaining(), 0);
    let credit: NativeExecutionCreditV11 =
        net.api().unwrap().aux_row(EXECUTION_CREDIT_TABLE_V11, &borsh::to_vec(&id).unwrap()).unwrap();
    assert_eq!(credit.requester_refund, 20);
    assert_eq!(credit.producer_credit, 0);
    assert_eq!(payee_minted_and_owed_all_queues(&net, 2), 20);
    action(&mut net, 2, EXECUTION_ACTION_CLOSE_V11, borsh::to_vec(&id).unwrap()).await;
    let replay = net.replay().await;
    net.assert_same(&replay, "native original funded nonce, escrow and once-only unclaimed refund");
    assert_eq!(payee_minted_and_owed_all_queues(&net, 2), 20, "the refund is paid exactly once");
}
#[tokio::test]
async fn execution11_native_commit_binds_original_job_input_and_reserves_producer_but_incomplete_final_cannot_pay() {
    let f = sparse9(false);
    let mut net = Net::over_cfg(native_config(), TestConsensus::new);
    let (id, class, bind, record) = funded(&mut net, &f).await;
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    let fixed = net.api().unwrap().execution_class_header_v11(&class).unwrap().checked_fixed_header().unwrap();
    let book = row.restore_book(&fixed).unwrap();
    let (body, _) = statement(&net, &f, bind, fixed.record().candidate9_id, &record, None);
    let post = FlatExecutionPostV11 {
        version: 11,
        execution_class: class,
        job_id: id,
        requester_nonce: book.job().nonce,
        job_input_root: book.job().input.root(),
        actual_scope9_root: book.job().actual_scope9_root,
        actual_resource10_root: book.job().actual_resource10_root,
        quota_plan_root: book.job().quota_plan_root,
        body,
    };
    action(&mut net, 0, EXECUTION_ACTION_COMMIT_V11, borsh::to_vec(&post).unwrap()).await;
    let row = net.api().unwrap().execution_job_row_v11(&id).unwrap();
    assert_eq!(row.header.terminal, ExecutionTerminalV11::Challengeable);
    assert_eq!(row.header.producer_reserved, fixed.record().required_producer_reserve);
    assert_eq!(net.chain.tip_state().1.native_execution_reserved_v11(&net.bond(0)), row.header.producer_reserved as u128);
    let book = row.restore_book(&fixed).unwrap();
    net.beat_to(book.claim().unwrap().retention_until + 1).await;
    action(&mut net, 0, EXECUTION_ACTION_CLOSE_V11, borsh::to_vec(&id).unwrap()).await;
    assert_eq!(net.api().unwrap().execution_job_header_v11(&id).unwrap().terminal, ExecutionTerminalV11::Challengeable);
    assert!(net.api().unwrap().aux.get(&(EXECUTION_CREDIT_TABLE_V11, borsh::to_vec(&id).unwrap())).is_none());
    let replay = net.replay().await;
    net.assert_same(&replay, "native exact committed job and closed incomplete Final");
}

/// Project rule (new kinds ride unjudged below their own fence): with the funded fence active but
/// Final27 ABSENT, every funded action 25..=40 — well-signed or with a corrupted signature, with a
/// garbage body — is carried in a VALID block and has ZERO state effect. Unverified source checkpoint
/// (2026-10-15): written during the code-first phase; it runs in the consolidated validation phase.
#[tokio::test]
async fn native_final27_actions_below_their_fence_are_neither_rejected_nor_applied() {
    let mut net = Net::over_cfg(native_config(), TestConsensus::new);
    // The route's state may not exist before its first folded object: absent = no native rows.
    let native = |net: &Net| -> Vec<((u8, Vec<u8>), Vec<u8>)> {
        net.api()
            .map(|r| r.aux.iter().filter(|((table, _), _)| (87..=112).contains(table)).map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default()
    };
    let before = native(&net);
    let bonds_before = net.chain.tip_state().1.bond(&net.bond(0)).map(|b| b.collateral);
    // 25..=40 are Final27-gated; 45/46 are the typed window class register/attach (§II.14.60),
    // gated by their own fence too.
    for action in (25u8..=40).chain([45u8, 46]) {
        let mut object = net.route(0, &K::FundedExecutionV11 { action, body: vec![action; 300] });
        if action % 2 == 1 {
            let Obj::KernelRouteV1 { signature, .. } = &mut object else { unreachable!() };
            let last = signature.len() - 1;
            signature[last] ^= 0x01; // an unsigned body must still never be applied
        }
        // `send` asserts the node's mempool and template carry it, and the block it folds in is valid.
        net.send(vec![(0, object)]).await;
    }
    // Stage28/Master32 and owning39 actions have their own execution + Final27
    // fence. Even an invalid signature and oversized nested frame are opaque
    // before it; neither the paid parser nor an original reservation may run.
    for action in 19u8..=70 {
        let mut object = net.route(0, &K::StageExecutionV14 { action, body: vec![action; 300] });
        if action % 2 == 0 {
            let Obj::KernelRouteV1 { signature, .. } = &mut object else { unreachable!() };
            let last = signature.len() - 1;
            signature[last] ^= 0x01;
        }
        net.send(vec![(0, object)]).await;
    }
    assert_eq!(native(&net), before, "below Final27 the opaque actions change no native row");
    assert_eq!(net.chain.tip_state().1.bond(&net.bond(0)).map(|b| b.collateral), bonds_before, "no fee or predebit");
}
