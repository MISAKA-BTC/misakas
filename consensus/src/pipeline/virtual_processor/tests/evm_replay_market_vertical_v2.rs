//! Real accepted BASE parents and child fences; market trade rows are not planted.
use super::*;
#[tokio::test]
async fn replay_market_native_parent_window_differs_from_accepting_post_window() {
    let mut ctx = context(true);
    let mut parent = mine(&mut ctx, 0xA00).await;
    let mut selected = None;
    for nonce in 0xA01..0xA20 {
        let accepting = mine(&mut ctx, nonce).await;
        if ctx.consensus.get_header(accepting).unwrap().daa_score != ctx.consensus.get_header(parent).unwrap().daa_score {
            selected = Some((parent, accepting));
            break;
        }
        parent = accepting;
    }
    let (parent, accepting) = selected.expect("actual adjacent DAA-distinct accepted pair");
    let context = ctx.consensus.palw_evm_replay_market_owned_v2(accepting).unwrap().unwrap();
    assert_eq!(context.accepting, accepting);
    assert_eq!(context.selected_parent, parent);
    assert_eq!(context.accepting_daa, ctx.consensus.get_header(accepting).unwrap().daa_score);
    assert_eq!(context.view.chain_daa, ctx.consensus.get_header(parent).unwrap().daa_score);
    assert_eq!(context.fences, ctx.consensus.params().palw_evm_market_fences_at(context.accepting_daa));
    let (post, _) = ctx.consensus.palw_evm_view_at_owned_v2(accepting).unwrap().unwrap();
    assert_ne!(context.view.chain_daa, post.chain_daa);
    assert!(context.settlements.is_empty());
}
#[tokio::test]
async fn replay_market_native_import_change_cannot_return_projected_owners() {
    let mut ctx = context(true);
    mine(&mut ctx, 0xA30).await;
    let accepting = mine(&mut ctx, 0xA31).await;
    assert!(ctx.consensus.palw_evm_replay_market_owned_v2(accepting).unwrap().is_some());
    ctx.consensus.observe_evm_view_projection_v2(Some(|vp| {
        let point = vp.pruning_point_store.read().pruning_point().unwrap();
        vp.begin_pruning_point_import(point).unwrap();
    }));
    assert!(
        matches!(ctx.consensus.palw_evm_replay_market_owned_v2(accepting), Err(NativeEvmViewFaultV2::Context(ref reason)) if reason.contains("during replay market projection"))
    );
}
#[tokio::test]
async fn replay_market_native_active_capacity_and_dormant_empty_work() {
    const CHILD: &str = "MISAKA_REPLAY_MARKET_NATIVE_QUOTA";
    if std::env::var_os(CHILD).is_none() {
        let full = format!("{}::replay_market_native_active_capacity_and_dormant_empty_work", module_path!());
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
    let mut active = context(true);
    mine(&mut active, 0xA40).await;
    let accepting = mine(&mut active, 0xA41).await;
    let mut dormant = context(false);
    let inert = mine(&mut dormant, 0xA42).await;
    active.consensus.shutdown(std::mem::take(&mut active.join_handles));
    dormant.consensus.shutdown(std::mem::take(&mut dormant.join_handles));
    use kaspa_consensus_core::palw_native_resource_v1::*;
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(matches!(active.consensus.palw_evm_replay_market_owned_v2(accepting), Err(NativeEvmViewFaultV2::Resource(_))));
    assert!(dormant.consensus.palw_evm_replay_market_owned_v2(inert).unwrap().is_none());
}
