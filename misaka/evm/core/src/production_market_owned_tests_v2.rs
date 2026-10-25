// Included in executor::tests. Signed interpreter controls use a hypothetical
// market window, not an approved native model or accepted settlement fixture.
fn production_market_input_fixture_v2() -> (
    EvmStateSnapshot,
    kaspa_consensus_core::evm::EvmTraceReplayBodyV1,
    kaspa_consensus_core::evm::view_owned_v2::NativeEvmReplayMarketV2,
) {
    let (snapshot, body, _, context) = accepted_market_replay_fixture_v2(2, true);
    (snapshot, body, context)
}
#[test]
fn production_market_owned_ordinary_executor_matches_legacy_roots_state_and_receipts() {
    let (snapshot, body, context) = production_market_input_fixture_v2();
    let payload = EvmExecutionPayload { system_ops: body.system_ops.clone(), evm_coinbase: body.env.coinbase, ..Default::default() };
    let accepted: Vec<_> =
        body.txs.iter().map(|tx| AcceptedTxCandidate { raw: tx.raw.clone(), payload_coinbase: tx.payload_coinbase }).collect();
    let raw = std::sync::Arc::new((**context.view).clone());
    let old = EvmBlockInput { market: market_input(&raw, true, &context.settlements), ..input_v2(&payload, &accepted) };
    let (old_result, old_state) = crate::snapshot::execute_block_from_snapshot(&snapshot, &old).unwrap();
    let owned = EvmBlockInput {
        market: EvmMarketInput { palw_view: None, palw_view_owned_v2: Some(context.view.clone()), ..old.market.clone() },
        ..old
    };
    let (result, state) = crate::snapshot::execute_block_from_snapshot(&snapshot, &owned).unwrap();
    assert_eq!(result.header, old_result.header);
    assert_eq!(result.receipts, old_result.receipts);
    assert_eq!(result.market_actions, old_result.market_actions);
    assert_eq!(result.candidate_outcomes, old_result.candidate_outcomes);
    assert_eq!(state, old_state);
    assert_eq!(result.market_actions.len(), 2, "real signed writer calls reached the retained handlers");
}
#[test]
fn production_market_owned_dual_input_refuses_full_snapshot_fast_path_and_trace_sibling() {
    let (snapshot, _, context) = production_market_input_fixture_v2();
    let payload = EvmExecutionPayload::default();
    let accepted = [];
    let raw = std::sync::Arc::new((**context.view).clone());
    let input = EvmBlockInput {
        market: EvmMarketInput { palw_view_owned_v2: Some(context.view.clone()), ..market_input(&raw, true, &[]) },
        ..input_v2(&payload, &accepted)
    };
    assert!(matches!(
        execute_block_evm(crate::snapshot::seed_cachedb(&snapshot).unwrap(), &input),
        Err(EvmExecError::InvariantViolation(_))
    ));
    assert!(
        matches!(crate::snapshot::execute_block_from_snapshot(&snapshot, &input), Err(EvmExecError::InvariantViolation(_))),
        "empty acceptance cannot bypass dual-input refusal"
    );
    let owned = EvmBlockInput { market: EvmMarketInput { palw_view: None, ..input.market.clone() }, ..input };
    assert!(
        matches!(
            execute_block_evm_with_retained_market_v2(crate::snapshot::seed_cachedb(&snapshot).unwrap(), &owned, context.view.clone()),
            Err(EvmExecError::InvariantViolation(_))
        ),
        "explicit replay sibling cannot double-supply an owned input"
    );
}
#[test]
fn production_market_owned_handler_resource_is_typed_not_execution_refusal() {
    const CHILD: &str = "MISAKA_PRODUCTION_MARKET_EXECUTOR_QUOTA";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::production_market_owned_handler_resource_is_typed_not_execution_refusal", module_path!());
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
    let (snapshot, body, context) = production_market_input_fixture_v2();
    let payload = EvmExecutionPayload { system_ops: body.system_ops.clone(), evm_coinbase: body.env.coinbase, ..Default::default() };
    let accepted: Vec<_> =
        body.txs.iter().map(|tx| AcceptedTxCandidate { raw: tx.raw.clone(), payload_coinbase: tx.payload_coinbase }).collect();
    let raw = std::sync::Arc::new((**context.view).clone());
    let legacy = EvmBlockInput { market: market_input(&raw, true, &context.settlements), ..input_v2(&payload, &accepted) };
    let owned = EvmBlockInput {
        market: EvmMarketInput { palw_view: None, palw_view_owned_v2: Some(context.view.clone()), ..legacy.market.clone() },
        ..input_v2(&payload, &accepted)
    };
    use kaspa_consensus_core::palw_native_resource_v1::*;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(crate::snapshot::execute_block_from_snapshot(&snapshot, &owned), Err(EvmExecError::NativeResource(_))));
    assert!(
        crate::snapshot::execute_block_from_snapshot(&snapshot, &legacy).is_ok(),
        "ordinary compatibility path is unchanged; earlier raw allocation is not claimed admitted"
    );
}
