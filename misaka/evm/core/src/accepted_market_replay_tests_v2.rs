use kaspa_consensus_core::evm::{EvmCandidateOutcome, EvmStateSnapshot};
use crate::model_market::writer_address;
// Included in executor::tests: real signed execution against a hypothetical
// market window, followed by the actual retained accepted-trace engine. These
// are EVM execution controls, not native model admission/settlement evidence.
fn accepted_market_replay_fixture_v2(
    count: usize,
    settle: bool,
) -> (
    EvmStateSnapshot,
    kaspa_consensus_core::evm::EvmTraceReplayBodyV1,
    EvmExecutionResult,
    kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2,
) {
    use kaspa_consensus_core::evm::{EvmReplayEnv, EvmReplayTx};
    use kaspa_consensus_core::{Hash64, palw_native_resource_v1::NativeRetainedV1};
    let line = Hash64::from_u64_word(0x12A0);
    let view = line_view(line);
    let mut raws = Vec::new();
    let mut sender = Address::ZERO;
    for nonce in 0..count {
        let (from, raw) = signed_call(
            0x21,
            nonce as u64,
            writer_address(),
            0,
            100_000,
            EVM_INITIAL_BASE_FEE as u128 * 4,
            crate::model_market::send_action_sell_calldata(&line, 1, 0),
        );
        sender = from;
        raws.push(raw);
    }
    let snapshot = crate::snapshot::snapshot_from_cachedb(&funded_seed(sender, 10u128.pow(22)));
    let settlements = if settle {
        vec![PalwEvmSettlementV1 {
            seq: 0,
            account: EvmAddress::from_bytes(sender.into_array()),
            line_id: line,
            action: PALW_EVM_ACTION_SELL,
            escrow_sompi: 0,
            outcome: PalwEvmSettlementOutcomeV1::Refused { reason: 1 },
        }]
    } else {
        vec![]
    };
    let payload = EvmExecutionPayload {
        system_ops: settlements.iter().copied().map(EvmSystemOp::MarketSettle).collect(),
        evm_coinbase: EvmAddress::from_bytes([0xFE; 20]),
        ..Default::default()
    };
    let accepted: Vec<_> = raws.iter().cloned().map(|raw| cand(raw, 0xAA)).collect();
    let input = EvmBlockInput { market: market_input(&view, true, &settlements), ..input_v2(&payload, &accepted) };
    let (result, _) = execute_block_evm(crate::snapshot::seed_cachedb(&snapshot).unwrap(), &input).unwrap();
    assert!(result.candidate_outcomes.iter().all(|o| matches!(o, EvmCandidateOutcome::Accepted { .. })));
    let body = kaspa_consensus_core::evm::EvmTraceReplayBodyV1 {
        selected_parent: Hash64::from_bytes(input.selected_parent_hash),
        env: EvmReplayEnv {
            header_timestamp_ms: input.header_timestamp_ms,
            blue_work_be: input.blue_work_be.clone(),
            daa_score: input.daa_score,
            coinbase: payload.evm_coinbase,
        },
        system_ops: payload.system_ops.clone(),
        txs: raws
            .into_iter()
            .enumerate()
            .map(|(i, raw)| EvmReplayTx {
                tx_hash: crate::tx::tx_hash(&raw),
                raw,
                payload_coinbase: accepted[i].payload_coinbase,
                originating_payload_block: Hash64::from_u64_word(0x12A1),
                outcome: result.candidate_outcomes[i],
            })
            .collect(),
    };
    // Explicit retention of already-existing typed compatibility inputs. Earlier
    // raw fixture construction is not claimed to have been preadmitted.
    let owned_view = std::sync::Arc::new(NativeRetainedV1::retain_existing_v1((*view).clone()).unwrap());
    let context = kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2 {
        accepting: Hash64::from_u64_word(0x12A2),
        selected_parent: body.selected_parent,
        accepting_daa: body.env.daa_score,
        view: owned_view,
        fences: input.market.fences,
        settlements: NativeRetainedV1::retain_existing_v1(settlements).unwrap(),
    };
    (snapshot, body, result, context)
}
fn accepted_market_replay_run_v2(
    snapshot: &EvmStateSnapshot,
    body: &kaspa_consensus_core::evm::EvmTraceReplayBodyV1,
    result: &EvmExecutionResult,
    context: Option<&kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2>,
    candidate: usize,
) -> Result<crate::trace::TracedTx, crate::trace::TraceError> {
    let EvmCandidateOutcome::Accepted { receipt_index } = result.candidate_outcomes[candidate] else { panic!("actual accepted tx") };
    crate::trace::trace_accepted_tx_with_market_owned_v2(
        snapshot,
        None,
        body,
        receipt_index,
        &result.receipts[receipt_index as usize],
        0,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        false,
        crate::trace::TraceLimits::default(),
        context,
    )
}
#[test]
fn accepted_market_replay_actual_prefix_and_system_settlement_match_committed_receipt() {
    for settle in [false, true] {
        let (snapshot, body, result, context) = accepted_market_replay_fixture_v2(2, settle);
        assert_eq!(result.market_actions.len(), 2);
        assert!(result.receipts.last().unwrap().succeeded);
        let traced = accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 1).unwrap();
        assert_eq!(traced.frame.to, Some(writer_address()));
        assert!(traced.frame.error.is_none());
        assert!(
            accepted_market_replay_run_v2(&snapshot, &body, &result, None, 1).is_err(),
            "inert market must not pass the recorded market receipt"
        );
    }
}
#[test]
fn accepted_market_replay_prefix_per_account_counter_is_retained_at_limit() {
    let (snapshot, body, result, context) = accepted_market_replay_fixture_v2(17, false);
    assert_eq!(result.market_actions.len(), 16);
    assert!(!result.receipts[16].succeeded, "actual seventeenth account move reverts");
    let traced = accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 16).unwrap();
    assert!(traced.frame.error.is_some());
    assert!(accepted_market_replay_run_v2(&snapshot, &body, &result, None, 16).is_err());
}
#[test]
fn accepted_market_replay_wrong_parent_or_daa_refuses_before_interpretation() {
    let (snapshot, mut body, result, context) = accepted_market_replay_fixture_v2(2, false);
    body.selected_parent = kaspa_hashes::Hash64::default();
    assert!(matches!(
        accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 1),
        Err(crate::trace::TraceError::Internal(_))
    ));
    body.selected_parent = context.selected_parent;
    body.env.daa_score += 1;
    assert!(matches!(
        accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 1),
        Err(crate::trace::TraceError::Internal(_))
    ));
}
#[test]
fn accepted_market_replay_actual_handler_quota_refuses_then_retries() {
    const CHILD: &str = "MISAKA_ACCEPTED_MARKET_REPLAY_QUOTA";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::accepted_market_replay_actual_handler_quota_refuses_then_retries", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", full.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    use kaspa_consensus_core::palw_native_resource_v1::*;
    let (snapshot, body, result, context) = accepted_market_replay_fixture_v2(2, false);
    const LIMIT: u64 = 32 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: LIMIT, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(LIMIT).unwrap();
    assert!(matches!(
        accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 1),
        Err(crate::trace::TraceError::Exec(EvmExecError::NativeResource(_)))
    ));
    drop(blocker);
    accepted_market_replay_run_v2(&snapshot, &body, &result, Some(&context), 1).unwrap();
    drop(reserve_native_requests_v1(LIMIT).unwrap());
}
