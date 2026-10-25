//! Actual accepted BASE blocks -> exact historical public reader -> revm reads.
//! No nonBASE canonical-head maturity or native39 settlement mapping is asserted.
use super::*;
use crate::model::stores::pruning::PruningStoreReader;
use kaspa_consensus_core::{
    config::params::ForkActivation,
    evm::{EVM_CHAIN_ID, model_market::MISAKA_MODEL_REGISTRY_PRECOMPILE, view_owned_v2::*},
    palw_mode_v2::PalwConsensusMode,
};
use std::sync::Arc;
fn context(active: bool) -> TestContext {
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle_funded_for(&catalog, 8);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            *p = p.clone().with_palw_v2_cadence();
            if active {
                p.evm_activation_daa_score = 0;
                p.palw_model_market = Some(ForkActivation::new(0));
                p.palw_model_evm = Some(ForkActivation::new(0));
            }
        })
        .build();
    config.params.validate_palw_v2().expect("explicit private fixture must satisfy the full native configuration contract");
    TestContext::new(TestConsensus::new(&config))
}
async fn mine(ctx: &mut TestContext, nonce: u64) -> BlockHash {
    let template = ctx.build_block_template_keeping_time(nonce);
    let hash = template.block.header.hash;
    let status = ctx.consensus.validate_and_insert_block(template.into_immutable_v3().unwrap()).virtual_state_task.await.unwrap();
    assert_eq!(status, BlockStatus::StatusUTXOValid);
    assert_eq!(ctx.consensus.get_sink(), hash);
    hash
}
#[tokio::test]
async fn historical_market_native_api_post_target_reaches_real_sim_and_legacy_parity() {
    let mut ctx = context(true);
    let p = mine(&mut ctx, 1).await;
    let p_daa = ctx.consensus.get_header(p).unwrap().daa_score;
    let mut a = p;
    for nonce in 2..18 {
        a = mine(&mut ctx, nonce).await;
        if ctx.consensus.get_header(a).unwrap().daa_score != p_daa {
            break;
        }
    }
    assert_ne!(ctx.consensus.get_header(a).unwrap().daa_score, p_daa, "actual DAA must distinguish the two read contexts");
    let read = |target| {
        let (view, fences) = ctx.consensus.palw_evm_view_at_owned_v2(target).unwrap().expect("explicit active window");
        let header = ctx.consensus.get_header(target).unwrap();
        assert_eq!(view.chain_daa, header.daa_score);
        let vp = ctx.consensus.virtual_processor();
        let state = vp.palw_evm_post_state_owned_v2(target).unwrap().unwrap();
        let raw = vp.palw_evm_view_with_market_gate_v1(&state, vp.palw_state_params_v2.as_ref().unwrap(), target, header.daa_score);
        assert_eq!(&**view, &raw, "actual legacy resolver and raw projection parity");
        let snapshot = ctx.consensus.get_evm_state_snapshot_of(target).unwrap().expect("actual accepted EVM snapshot");
        let env = kaspa_evm::sim::EthCallEnv {
            chain_id: EVM_CHAIN_ID,
            gas_limit: 30_000_000,
            palw_view_owned_v2: Some(view.clone()),
            market_fences: fences,
            ..Default::default()
        };
        let call = kaspa_evm::sim::EthCall {
            to: Some(MISAKA_MODEL_REGISTRY_PRECOMPILE),
            data: kaspa_evm::model_market::calldata("chainDaa()", &[]),
            ..Default::default()
        };
        let actual = kaspa_evm::sim::simulate_call(&snapshot, &env, &call).unwrap();
        let legacy = kaspa_evm::sim::EthCallEnv { palw_view: Some(Arc::new((**view).clone())), palw_view_owned_v2: None, ..env };
        let old = kaspa_evm::sim::simulate_call(&snapshot, &legacy, &call).unwrap();
        assert!(actual.success && old.success);
        assert_eq!(actual.output, old.output);
        assert_eq!(actual.gas_used, old.gas_used);
        assert_eq!(kaspa_evm::model_market::abi::read_u64(&actual.output, 0), Some(header.daa_score));
        actual.output
    };
    assert_ne!(read(p), read(a), "historical API and handlers must not silently show the latest block for both selectors");
}
#[tokio::test]
async fn historical_market_native_api_projection_import_change_never_returns_owner() {
    let mut ctx = context(true);
    let target = mine(&mut ctx, 5).await;
    assert!(ctx.consensus.palw_evm_view_at_owned_v2(target).unwrap().is_some());
    ctx.consensus.observe_evm_view_projection_v2(Some(|vp| {
        let point = vp.pruning_point_store.read().pruning_point().unwrap();
        vp.begin_pruning_point_import(point).unwrap();
    }));
    let actual = ctx.consensus.palw_evm_view_at_owned_v2(target);
    assert!(
        matches!(actual, Err(NativeEvmViewFaultV2::Context(ref reason)) if reason.contains("during historical view projection")),
        "{actual:?}"
    );
    assert!(
        matches!(ctx.consensus.palw_evm_view_at_owned_v2(target), Err(NativeEvmViewFaultV2::Context(ref reason)) if reason=="import incomplete")
    );
}
#[tokio::test]
async fn historical_market_native_api_real_missing_and_corrupt_header_are_distinct() {
    use kaspa_database::{prelude::DbKey, registry::DatabaseStorePrefixes};
    let mut ctx = context(true);
    mine(&mut ctx, 9).await;
    ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
    let missing = BlockHash::from_bytes([0xa1; 64]);
    assert!(matches!(ctx.consensus.palw_evm_view_at_owned_v2(missing), Err(NativeEvmViewFaultV2::HistoryUnavailable)));
    let corrupt = BlockHash::from_bytes([0xa2; 64]);
    ctx.consensus
        .virtual_processor()
        .db
        .put(DbKey::new(&Vec::<u8>::from(DatabaseStorePrefixes::CompressedHeaders), corrupt), [0xff])
        .unwrap();
    let actual = ctx.consensus.palw_evm_view_at_owned_v2(corrupt);
    assert!(matches!(actual, Err(NativeEvmViewFaultV2::Store(_))), "{actual:?}");
}
#[tokio::test]
async fn historical_market_native_api_active_resource_and_dormant_zero_state_work() {
    const CHILD: &str = "MISAKA_EVM_VIEW_API_QUOTA_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let name = format!("{}::historical_market_native_api_active_resource_and_dormant_zero_state_work", module_path!());
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
    let mut active = context(true);
    let target = mine(&mut active, 10).await;
    let mut dormant = context(false);
    let dormant_target = mine(&mut dormant, 11).await;
    active.consensus.shutdown(std::mem::take(&mut active.join_handles));
    dormant.consensus.shutdown(std::mem::take(&mut dormant.join_handles));
    use kaspa_consensus_core::palw_native_resource_v1::*;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(active.consensus.palw_evm_view_at_owned_v2(target), Err(NativeEvmViewFaultV2::Resource(_))));
    assert!(dormant.consensus.palw_evm_view_at_owned_v2(dormant_target).unwrap().is_none());
}
