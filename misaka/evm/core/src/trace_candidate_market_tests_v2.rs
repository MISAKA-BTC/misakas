//! Candidate diagnosis executes the same read-only market doors as eth_call.
//! These signed EVM calls are simulations, not accepted bridge/market settlements.
use super::{
    tests::{fund, head_env, signed_tx},
    *,
};
use crate::sim::{EthCall, simulate_call};
use kaspa_consensus_core::{
    evm::{EvmAddress, model_market::PalwEvmMarketFencesV1},
    palw_native_resource_v1::*,
    palw_state_v2::PalwChainStateV2,
};
use std::sync::Arc;

fn env() -> EthCallEnv {
    let source = NativeRetainedV1::retain_existing_v1(PalwChainStateV2::genesis()).unwrap();
    let view = source.evm_view_owned_v2(1, kaspa_hashes::Hash64::default(), Default::default()).unwrap();
    EthCallEnv {
        palw_view_owned_v2: Some(view),
        market_fences: PalwEvmMarketFencesV1 { market_active: true, lines_active: true, evm_active: true, ..Default::default() },
        ..head_env()
    }
}
fn signed(signature: &str) -> (Address, Vec<u8>, Vec<u8>) {
    let data = crate::model_market::calldata(signature, &[]);
    let to = kaspa_consensus_core::evm::model_market::MISAKA_MODEL_REGISTRY_PRECOMPILE;
    let (sender, raw) =
        signed_tx(0, revm::primitives::TxKind::Call(Address::from_slice(&to.as_bytes())), 0, 200_000, data.clone().into());
    (sender, raw, data)
}
#[test]
fn candidate_market_retained_trace_matches_simulation_and_raw_view() {
    let env = env();
    let legacy = EthCallEnv {
        palw_view: Some(Arc::new((***env.palw_view_owned_v2.as_ref().unwrap()).clone())),
        palw_view_owned_v2: None,
        ..env.clone()
    };
    for signature in ["chainDaa()", "classCount()", "lineCount()"] {
        let (sender, raw, data) = signed(signature);
        let snapshot = fund(sender);
        let call = EthCall {
            from: EvmAddress::from_bytes(sender.into_array()),
            to: Some(kaspa_consensus_core::evm::model_market::MISAKA_MODEL_REGISTRY_PRECOMPILE),
            data,
            gas_limit: 200_000,
            ..Default::default()
        };
        let sim = simulate_call(&snapshot, &env, &call).unwrap();
        let trace = trace_candidate_tx(&snapshot, &env, &raw, TraceLimits::default()).unwrap();
        let old = trace_candidate_tx(&snapshot, &legacy, &raw, TraceLimits::default()).unwrap();
        assert!(sim.success && trace.executed && trace.succeeded && old.executed && old.succeeded);
        assert_eq!(trace.output.as_ref(), sim.output.as_slice());
        assert_eq!(trace.output.len(), 32, "actual ABI door must run, not an empty precompile account");
        assert_eq!(trace.output, old.output);
        assert_eq!(trace.gas_used, old.gas_used);
        assert_eq!(trace.frame.as_ref().unwrap().output, trace.output);
        let dormant = EthCallEnv { market_fences: Default::default(), ..env.clone() };
        let inert = trace_candidate_tx(&snapshot, &dormant, &raw, TraceLimits::default()).unwrap();
        assert!(inert.executed && inert.succeeded && inert.output.is_empty());
    }
}
#[test]
fn candidate_market_ambiguous_view_refused_even_below_fence_before_decode() {
    let env = env();
    let ambiguous = EthCallEnv { palw_view: Some(Arc::new((***env.palw_view_owned_v2.as_ref().unwrap()).clone())), ..env };
    for env in [ambiguous.clone(), EthCallEnv { market_fences: Default::default(), ..ambiguous }] {
        assert!(matches!(
            trace_candidate_tx(&EvmStateSnapshot::default(), &env, &[], TraceLimits::default()),
            Err(TraceError::Exec(EvmExecError::InvariantViolation(_)))
        ));
    }
}
#[test]
fn candidate_market_capacity_refusal_precedes_decode_and_retries_without_leak() {
    const CHILD: &str = "MISAKA_CANDIDATE_MARKET_CAPACITY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::candidate_market_capacity_refusal_precedes_decode_and_retries_without_leak", module_path!());
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
    let env = env();
    const TOTAL: u64 = 32 << 20;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: TOTAL, database_bytes: 0 }));
    let blocker = reserve_native_requests_v1(TOTAL).unwrap();
    assert!(matches!(
        trace_candidate_tx(&EvmStateSnapshot::default(), &env, &[], TraceLimits::default()),
        Err(TraceError::Exec(EvmExecError::NativeResource(_)))
    ));
    drop(blocker);
    let (sender, raw, _) = signed("chainDaa()");
    let result = trace_candidate_tx(&fund(sender), &env, &raw, TraceLimits::default()).unwrap();
    assert!(result.executed && result.succeeded && result.output.len() == 32);
    drop(reserve_native_requests_v1(TOTAL).unwrap());
}
