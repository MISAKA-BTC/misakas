//! Isolated logical-quota counterexamples, not whole-node RSS measurements.
use super::*;
use kaspa_consensus_core::{
    block::{BlockTemplate, MutableBlock},
    config::params::MAINNET_PARAMS,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
    tx::scriptvec,
};

#[test]
fn owning_outputs_and_cache_copies_reserve_before_allocating() {
    let worker = format!("{}::capacity_worker", module_path!().split_once("::").unwrap().1);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
        .env("MISAKA_COINBASE_BUILD_CAPACITY_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success(), "coinbase capacity worker failed: {status}");
}

#[test]
fn capacity_worker() {
    if std::env::var("MISAKA_COINBASE_BUILD_CAPACITY_CHILD").as_deref() != Ok("1") {
        return;
    }
    let script = ScriptPublicKey::new(0, scriptvec![0x51, 0x52]);
    let requests = 4 * std::mem::size_of::<TransactionOutput>() as u64 + 2 * script.script().len() as u64;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: requests, database_bytes: 0 }));
    let mut render = CoinbaseOutputsV2::Build { outputs: Vec::new(), allocation: Default::default() };
    render.push(17, &script).unwrap();
    let capacity = match &render {
        CoinbaseOutputsV2::Build { outputs, .. } => outputs.capacity(),
        _ => unreachable!(),
    };
    let p = &MAINNET_PARAMS;
    let manager = CoinbaseManager::new(
        p.coinbase_payload_script_public_key_max_len,
        p.max_coinbase_payload_len,
        p.deflationary_phase_daa_score,
        p.pre_deflationary_phase_base_subsidy,
        p.target_time_per_block_history(),
    );
    let ghostdag = GhostdagData::new(
        0,
        0.into(),
        p.genesis.hash,
        kaspa_consensus_core::blockhash::BlockHashes::new(vec![]),
        kaspa_consensus_core::blockhash::BlockHashes::new(vec![]),
        Default::default(),
    );
    let result = manager.expected_coinbase_transaction(
        0,
        0,
        MinerData::new(script.clone(), vec![]),
        &ghostdag,
        &Default::default(),
        &Default::default(),
        &[],
        None,
        (0, 0),
        0,
        &Default::default(),
        false,
        &Default::default(),
        &Default::default(),
        &Default::default(),
    );
    assert!(
        matches!(result, Err(CoinbaseError::LocalNativeCapacityUnavailable(_))),
        "the real renderer propagates local capacity without panic or semantic-invalid verdict"
    );
    assert!(matches!(render.push(19, &script), Err(CoinbaseError::LocalNativeCapacityUnavailable(_))));
    assert_eq!(render.len(), 1, "refusal must precede the owning push");
    let CoinbaseOutputsV2::Build { outputs, allocation } = render else { unreachable!() };
    assert_eq!(outputs.capacity(), capacity, "refusal must not grow destination capacity");
    let tx = Transaction::new(constants::TX_VERSION, vec![], outputs, 0, subnets::SUBNETWORK_ID_COINBASE, 0, vec![]);
    let mut comparison = CoinbaseOutputsV2::Compare { actual: &tx, count: 0, first_difference: None };
    comparison.push(17, &script).unwrap();
    drop(comparison);
    let template = BlockTemplate::new(
        MutableBlock::new((&MAINNET_PARAMS.genesis).into(), vec![tx]),
        MinerData::new(script.clone(), vec![]),
        false,
        vec![],
        0,
        0,
        MAINNET_PARAMS.genesis.hash,
        vec![],
        vec![],
        vec![],
    )
    .with_coinbase_allocation_v2(allocation);
    let shared = std::sync::Arc::new(template);
    let cached = std::sync::Arc::clone(&shared);
    drop(shared);
    assert!(cached.try_clone_v2().is_err(), "sharing a cache entry keeps its original ticket live");
    assert!(reserve_native_requests_v1(1).is_err());
    drop(cached);
    let returned = reserve_native_requests_v1(requests).unwrap();
    drop(returned);
    let mut replacement = CoinbaseOutputsV2::Build { outputs: Vec::new(), allocation: Default::default() };
    replacement.push(23, &script).unwrap();
}
