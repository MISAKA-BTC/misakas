//! Actual node exit readers: local cold-load capacity does not judge a spend.
use super::t12_round_lane_e2e::{t12_genesis_chain, t12_with_harness_cards};
use crate::pipeline::virtual_processor::processor::PalwLocalTransitionFaultV1;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::{palw_native_resource_v1::*, palw_state_v2::PalwStateV2Error};
use kaspa_database::prelude::CachePolicy;

#[tokio::test]
async fn native_bond_exit_checked_readers_retry_capacity_without_poisoning_or_granting() {
    const CHILD: &str = "MISAKA_NATIVE_BOND_EXIT_CAPACITY_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::native_bond_exit_checked_readers_retry_capacity_without_poisoning_or_granting", module_path!());
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
    let (config, bundle, premine, floats) = t12_with_harness_cards();
    let mut chain = t12_genesis_chain(&config, &bundle, &premine, &floats);
    let (_, state) = chain.tip_state();
    let vp = chain.vp();
    let now = chain.ctx.consensus.get_virtual_daa_score();
    let root = state.state_root();
    let expected = vp.palw_v2_locked_bond_outpoints(&state, now).unwrap();
    assert_eq!(expected.len(), chain.bonds.len());
    assert!(
        state.kernel_route_state_v1().is_none_or(|r| r.aux.keys().all(|(table, _)| *table != 197)),
        "the genuine genesis has no native collateral bindings"
    );
    chain.ctx.consensus.shutdown(std::mem::take(&mut chain.ctx.join_handles));
    let cold = vp.palw_state_v2_store.read().clone_with_new_cache(CachePolicy::Empty);
    *vp.palw_state_v2_store.write() = cold;
    *vp.palw_reader_snapshot.lock() = None;
    *vp.palw_mempool_locked_cache.lock() = None;
    let memory = 1u64 << 30;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: memory, database_bytes: u64::MAX }));
    // Another in-flight local owner has reserved the declared memory. Release
    // that real ticket later to verify retry on the identical stored state.
    let pressure = reserve_native_requests_v1(memory).unwrap().unwrap();
    // The old None197 path needs no speculative allocation and keeps exactly
    // its actual genesis decision even under a locally exhausted checker.
    assert_eq!(vp.palw_v2_locked_bond_outpoints(&state, now).unwrap(), expected);
    assert!(
        matches!(vp.palw_locked_bond_outpoints_checked_v3_impl(), Err(PalwStateV2Error::NativeResource(_))),
        "cold own state is unavailable, not an empty wallet grant"
    );
    let Err(PalwLocalTransitionFaultV1::Resource(why)) = vp.palw_mempool_bond_gate(now) else {
        panic!("own cold-read capacity is not a judged invalid spend")
    };
    let tx = PalwLocalTransitionFaultV1::Resource(why).into_tx_rule_error_v1();
    assert!(tx.is_local_native_retry_v1());
    assert!(vp.palw_mempool_locked_cache.lock().is_none(), "a failed local read must not install a permissive or invalid gate cache");
    assert_eq!(state.state_root(), root);
    drop(pressure);
    assert_eq!(
        vp.palw_locked_bond_outpoints_checked_v3_impl().unwrap().into_iter().collect::<std::collections::HashSet<_>>(),
        expected,
        "the exact same own state succeeds after local capacity recovers"
    );
}
