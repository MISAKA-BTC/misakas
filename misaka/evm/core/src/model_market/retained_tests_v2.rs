//! Retained setup and real simulation controls. This is not an accepted EVM campaign.
use super::*;
use kaspa_consensus_core::{evm::EvmStateSnapshot, palw_native_resource_v1::*, palw_state_v2::*};
fn retained_view() -> NativeEvmViewOwnedV2 {
    let source = NativeRetainedV1::retain_existing_v1(PalwChainStateV2::genesis()).unwrap();
    source.evm_view_owned_v2(1, Hash64::default(), Default::default()).unwrap()
}
fn fences() -> PalwEvmMarketFencesV1 {
    PalwEvmMarketFencesV1 { market_active: true, lines_active: true, evm_active: true, ..Default::default() }
}
#[test]
fn historical_market_retained_real_sim_matches_legacy_and_rejects_ambiguous_env() {
    use crate::sim::*;
    let view = retained_view();
    let raw = Arc::new((**view).clone());
    let env = EthCallEnv {
        chain_id: 1,
        gas_limit: 30_000_000,
        palw_view_owned_v2: Some(view),
        market_fences: fences(),
        ..Default::default()
    };
    let legacy = EthCallEnv { palw_view: Some(raw), palw_view_owned_v2: None, ..env.clone() };
    let snapshot = EvmStateSnapshot::default();
    for signature in ["chainDaa()", "classCount()", "lineCount()"] {
        let call = EthCall { to: Some(MISAKA_MODEL_REGISTRY_PRECOMPILE), data: calldata(signature, &[]), ..Default::default() };
        let actual = simulate_call(&snapshot, &env, &call).unwrap();
        let old = simulate_call(&snapshot, &legacy, &call).unwrap();
        assert!(actual.success && old.success);
        assert_eq!(actual.output, old.output);
        assert_eq!(actual.gas_used, old.gas_used);
        assert_eq!(estimate_gas(&snapshot, &env, &call).unwrap(), estimate_gas(&snapshot, &legacy, &call).unwrap());
    }
    let ambiguous = EthCallEnv { palw_view: legacy.palw_view.clone(), ..env };
    assert!(matches!(simulate_call(&snapshot, &ambiguous, &EthCall::default()), Err(crate::EvmExecError::InvariantViolation(_))));
    let dormant = EthCallEnv { market_fences: Default::default(), ..ambiguous };
    assert!(matches!(simulate_call(&snapshot, &dormant, &EthCall::default()), Err(crate::EvmExecError::InvariantViolation(_))));
}
#[test]
fn historical_market_retained_exact_counter_bound_matches_legacy_reset() {
    let view = retained_view();
    let retained = MarketHandlers::new_retained_v2(view.clone(), fences(), 1).unwrap();
    let legacy = MarketHandlers::new(Arc::new((**view).clone()), fences(), 1);
    let capacity = retained.retained.as_ref().unwrap().accounts.lock().unwrap().capacity();
    // The actual intercept uses this same pre-record counter check; no separate
    // account-row policy is introduced. Reverted subcalls remain counted until
    // the executor's explicit committed-log reset, identically to legacy.
    for i in 0..MAX_MARKET_ACTIONS_PER_EVM_BLOCK {
        assert_eq!(retained.queued(), legacy.queued());
        assert!(retained.queued() < MAX_MARKET_ACTIONS_PER_EVM_BLOCK);
        let mut address = [0u8; 20];
        address[18..].copy_from_slice(&(i as u16).to_be_bytes());
        let address = Address::from(address);
        retained.record_v2(address);
        legacy.record_v2(address);
        assert_eq!(retained.queued_for_account(&address), legacy.queued_for_account(&address));
    }
    assert_eq!(retained.queued(), MAX_MARKET_ACTIONS_PER_EVM_BLOCK);
    assert_eq!(retained.retained.as_ref().unwrap().accounts.lock().unwrap().capacity(), capacity);
    assert_eq!(retained.retained.as_ref().unwrap().accounts.lock().unwrap().len(), 128);
    let actions = [PalwEvmMarketActionV1 {
        seq: 0,
        account: kaspa_consensus_core::evm::EvmAddress::default(),
        line_id: Hash64::default(),
        kind: kaspa_consensus_core::evm::model_market::PalwEvmMarketActionKindV1::Seed,
        gross_sompi: 0,
    }; 3];
    retained.set_committed(&actions);
    legacy.set_committed(&actions);
    assert_eq!(retained.queued(), legacy.queued());
    assert_eq!(retained.queued_for_account(&Address::ZERO), 3);
    retained.set_committed(&[]);
    legacy.set_committed(&[]);
    assert_eq!(retained.queued(), 0);
    assert_eq!(retained.queued_for_account(&Address::ZERO), 0);
}
#[test]
fn historical_market_retained_collision_order_is_legacy_last_wins() {
    // Test the production sort/dedup on artificial collisions; no assumption
    // about finding distinct cryptographic facade collisions is required.
    let address = Address::from([1u8; 20]);
    let first = Hash64::from_u64_word(1);
    let last = Hash64::from_u64_word(2);
    let other = Address::from([2u8; 20]);
    let input = [(address, first, 0), (other, first, 1), (address, last, 2)];
    let old: HashMap<_, _> = input.iter().map(|(a, id, _)| (*a, *id)).collect();
    let mut actual = input.to_vec();
    RetainedSetupV2::sort_facades(&mut actual);
    assert_eq!(actual.len(), old.len());
    for (address, id, _) in actual {
        assert_eq!(Some(&id), old.get(&address));
    }
}
#[test]
fn historical_market_retained_setup_quota_and_callback_clone_keep_both_owners() {
    const CHILD: &str = "MISAKA_EVM_RETAINED_SETUP_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let name = format!("{}::historical_market_retained_setup_quota_and_callback_clone_keep_both_owners", module_path!());
        assert!(
            std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", name.split_once("::").unwrap().1, "--nocapture", "--test-threads=1"])
                .env(CHILD, "1")
                .status()
                .unwrap()
                .success()
        );
        return;
    }
    const TOTAL: u64 = 32 * 1024 * 1024;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: TOTAL, database_bytes: 0 }));
    let state = PalwChainStateV2::genesis();
    let view_bytes = state.evm_view_quote_v2().unwrap().persistent_bytes;
    let source = NativeRetainedV1::retain_existing_v1(state).unwrap();
    let view = source.evm_view_owned_v2(1, Hash64::default(), Default::default()).unwrap();
    drop(source);
    let setup = RetainedSetupV2::quote(view.lines.len()).unwrap();
    let blocker = reserve_native_requests_v1(TOTAL - view_bytes - setup + 1).unwrap();
    assert!(MarketHandlers::new_retained_v2(view.clone(), fences(), 1).is_err());
    drop(blocker);
    let blocker = reserve_native_requests_v1(TOTAL - view_bytes - setup).unwrap();
    let handler = MarketHandlers::new_retained_v2(view.clone(), fences(), 1).unwrap();
    let callback_clone = handler.clone();
    let weak = Arc::downgrade(&view);
    drop(view);
    assert!(reserve_native_requests_v1(1).is_err());
    drop(handler);
    assert!(weak.upgrade().is_some());
    assert!(reserve_native_requests_v1(1).is_err());
    drop(callback_clone);
    assert!(weak.upgrade().is_none());
    drop(reserve_native_requests_v1(1).unwrap());
    drop(blocker);
    drop(reserve_native_requests_v1(TOTAL).unwrap());
}
