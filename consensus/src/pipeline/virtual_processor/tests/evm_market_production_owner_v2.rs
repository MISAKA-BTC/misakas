//! Actual native producer state/projection admission and unavailable-state
//! boundary. No queue, approved model, claim or EVM execution facts are planted.
use super::*;
use kaspa_consensus_core::errors::block::RuleError;
use kaspa_consensus_core::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1 as C;
#[tokio::test]
async fn evm_market_production_unreadable_or_missing_parent_stops_actual_template() {
    use kaspa_database::{prelude::DbKey, registry::DatabaseStorePrefixes};
    for missing in [false, true] {
        let mut ctx = context(true);
        let parent = mine(&mut ctx, 0xb10 + missing as u64).await;
        ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
        let vp = ctx.consensus.virtual_processor();
        if missing {
            vp.palw_state_v2_store.write().delete_tip_for_tests().unwrap();
        } else {
            vp.db.put(DbKey::prefix_only(&Vec::<u8>::from(DatabaseStorePrefixes::PalwStateV2Tip)), [0xff]).unwrap();
        }
        let expected = if missing { C::HistoryUnavailable } else { C::StoreUnreadable };
        assert!(
            matches!(vp.evm_template_parent_owned_v2(parent,ctx.consensus.get_header(parent).unwrap().daa_score),Err(crate::pipeline::virtual_processor::processor::PalwLocalTransitionFaultV1::Context(why)) if why==expected)
        );
        let actual = ctx.consensus.build_block_template(
            new_miner_data(),
            Box::new(OnetimeTxSelector::new(vec![])),
            TemplateBuildMode::Standard,
        );
        assert!(matches!(&actual,Err(RuleError::LocalNativeStateContextUnavailable(why)) if why==&expected), "{actual:?}");
        assert_eq!(ctx.consensus.get_sink(), parent);
        assert_eq!(ctx.consensus.block_status(parent), BlockStatus::StatusUTXOValid);
    }
}
#[tokio::test]
async fn evm_market_production_retained_owner_capacity_and_exact_parent() {
    const CHILD: &str = "MISAKA_EVM_PRODUCTION_MARKET_OWNER";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::evm_market_production_retained_owner_capacity_and_exact_parent", module_path!());
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
    let mut ctx = context(true);
    let parent = mine(&mut ctx, 0xb20).await;
    let mut dormant = context(false);
    let inert = mine(&mut dormant, 0xb21).await;
    ctx.consensus.shutdown(std::mem::take(&mut ctx.join_handles));
    dormant.consensus.shutdown(std::mem::take(&mut dormant.join_handles));
    let vp = ctx.consensus.virtual_processor();
    let daa = ctx.consensus.get_header(parent).unwrap().daa_score;
    assert!(
        vp.evm_template_parent_owned_v2(BlockHash::from_bytes([0xee; 64]), daa).is_err(),
        "exact parent cannot be silently replaced by tip"
    );
    use kaspa_consensus_core::palw_native_resource_v1::*;
    let budget = 512 * 1024 * 1024;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: budget, database_bytes: budget }));
    assert!(
        vp.palw_state_v2_store.read().load_tip(vp.palw_state_params_v2.as_ref().unwrap()).is_err(),
        "the previous bare extractor is unavailable under real native admission"
    );
    let state = vp.evm_template_parent_owned_v2(parent, daa).unwrap().unwrap();
    let context = vp.evm_market_context_owned_v2(Some(&state), parent, daa).unwrap().unwrap();
    assert_eq!(context.view.chain_daa, daa);
    assert!(context.settlements.is_empty());
    let retained = context.view.clone();
    drop(context);
    drop(state);
    assert!(reserve_native_requests_v1(budget).is_err(), "callback view clone retains its actual ticket");
    drop(retained);
    drop(reserve_native_requests_v1(budget).unwrap());
    // Exercise the actual acceptance driver with an actual native parent and
    // owned window. This extra payload row is execution input only: it is not
    // an accepted DAG block, deposit, or fabricated on-chain credit.
    let state = vp.evm_template_parent_owned_v2(parent, daa).unwrap().unwrap();
    let projected = vp.evm_market_context_owned_v2(Some(&state), parent, daa).unwrap().unwrap();
    let merged = BlockHash::from_bytes([0xed; 64]);
    let raw = crate::pipeline::virtual_processor::tests::rfc12_zero_dns_matrix::withdrawal().0;
    let mut batch = rocksdb::WriteBatch::default();
    crate::model::stores::evm::EvmPayloadStore::insert_batch(
        &*vp.evm_payload_store,
        &mut batch,
        merged,
        kaspa_consensus_core::evm::EvmExecutionPayload { transactions: vec![raw], ..Default::default() },
    )
    .unwrap();
    vp.db.write(batch).unwrap();
    // OnceLock is armed exactly once. Measure the actual ledger's available
    // demand with immediately released probes, then occupy that room without
    // changing the configured limit or detaching either live owner.
    let (mut low, mut high) = (0, budget);
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if reserve_native_requests_v1(mid).is_ok() {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    let blocker = reserve_native_requests_v1(low).unwrap();
    assert!(reserve_native_requests_v1(1).is_err());
    let header = ctx.consensus.get_header(parent).unwrap();
    let actual = crate::processes::evm::evm_execute_acceptance(
        &vp.evm_header_store,
        &vp.evm_state_store,
        &vp.evm_payload_store,
        parent,
        &[merged],
        &header,
        &Default::default(),
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        u64::MAX,
        kaspa_consensus_core::evm::evm_user_gas_cap_v1(None),
        kaspa_evm::EvmMarketInput {
            palw_view: None,
            palw_view_owned_v2: Some(projected.view.clone()),
            fences: vp.palw_evm_market_fences_at(daa),
            expected_settlements: &projected.settlements,
            chain_id: kaspa_consensus_core::evm::EVM_CHAIN_ID,
        },
    );
    assert!(
        matches!(actual, Err(crate::processes::evm::EvmValidateError::Resource(_))),
        "actual executor capacity survives the driver taxonomy: {actual:?}"
    );
    assert!(matches!(
        vp.evm_template_parent_owned_v2(parent, daa),
        Err(crate::pipeline::virtual_processor::processor::PalwLocalTransitionFaultV1::Resource(_))
    ));
    assert!(
        dormant.consensus.virtual_processor().evm_template_parent_owned_v2(inert, 0).unwrap().is_none(),
        "dormant market adds no state work"
    );
    assert_eq!(ctx.consensus.block_status(parent), BlockStatus::StatusUTXOValid);
    drop(blocker);
    assert!(vp.evm_template_parent_owned_v2(parent, daa).unwrap().is_some(), "same configured limit admits retry after owner release");
}
