//! ADR-0125 semantic amendment: real mixed mergesets keep the existing consensus clocks and raw colouring.
use super::{TestConsensus, TestContext, adr0125_config_funded_for, adr0125_harness_bond, adr0125_round_block};
use crate::model::stores::headers::HeaderStoreReader;
use crate::processes::window::WindowManager;
use kaspa_consensus_core::api::ConsensusApi;
use kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk;

#[tokio::test]
async fn adr0125_semantic_mixed_mergeset_rounds_do_not_change_consensus_accounting() {
    let (config, bundle) = adr0125_config_funded_for(32);
    assert_eq!(config.params.ghostdag_k(), 1);
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..4 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    // Three siblings force both a genuine red and a merged blue at k=1. Build all from the
    // same tip before inserting any; their coinbases are valid for their own identical parents.
    let siblings: Vec<_> = (11..14)
        .map(|nonce| ctx.build_block_template(nonce, ctx.simulated_time + config.params.target_time_per_block()).block.to_immutable())
        .collect();
    let mut parents: Vec<_> = siblings.iter().map(|block| block.hash()).collect();
    for sibling in siblings {
        ctx.consensus.validate_and_insert_block(sibling).virtual_state_task.await.unwrap();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let baseline = vp.ghostdag_manager.ghostdag(&parents);
    assert!(!baseline.mergeset_reds.is_empty(), "fixture must include a genuine GHOSTDAG red");
    let baseline_daa = vp.window_manager.block_daa_window(&baseline).unwrap().daa_score;
    let pruning_point = config.params.genesis.hash;
    let baseline_root = vp.depth_manager.calc_merge_depth_root(&baseline, pruning_point);
    assert!(vp.depth_manager.merge_breaking_reds(&baseline, baseline_root).is_empty());
    let first = (vp.headers_store.get_timestamp(ctx.consensus.get_sink()).unwrap() - config.params.genesis.timestamp) / 1_000 + 2;
    let first = first + first % 2;
    let (_, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().expect("the tip loads");
    let payout = p2pkh_mldsa87_spk(state.bond(&adr0125_harness_bond()).unwrap().payout_payload.as_byte_slice());
    let mut rounds = Vec::new();
    for index in 0..3 {
        let round = adr0125_round_block(&ctx, &config, first + index * 2, 0, payout.clone(), index);
        let hash = round.header.hash;
        ctx.consensus.validate_and_insert_block(round.to_immutable()).virtual_state_task.await.unwrap();
        rounds.push(hash);
        // Only the newest round tip is a direct parent; earlier rounds enter through its past.
        parents.truncate(3);
        parents.push(hash);
        let mixed = vp.ghostdag_manager.ghostdag(&parents);
        assert_eq!(mixed.selected_parent, baseline.selected_parent, "a round never becomes the selected parent");
        assert_eq!(mixed.blue_score, baseline.blue_score);
        assert_eq!(mixed.blue_work, baseline.blue_work);
        assert_eq!(mixed.mergeset_blues, baseline.mergeset_blues);
        assert_eq!(mixed.blues_anticone_sizes, baseline.blues_anticone_sizes);
        assert_eq!(vp.window_manager.block_daa_window(&mixed).unwrap().daa_score, baseline_daa);
        let root = vp.depth_manager.calc_merge_depth_root(&mixed, pruning_point);
        assert_eq!(root, baseline_root);
        assert!(vp.depth_manager.merge_breaking_reds(&mixed, root).is_empty());
        let classified = mixed.classify_palw_mergeset_v1(|member| vp.ghostdag_manager.is_round_block(member));
        assert_eq!(classified.genuine_reds(), baseline.mergeset_reds.as_slice());
        assert_eq!(classified.rounds().len(), rounds.len());
        for round in &rounds {
            assert!(mixed.mergeset_reds.contains(round), "raw representation stays always-red");
            assert!(classified.rounds().contains(round));
            assert!(!classified.genuine_reds().contains(round));
        }
    }
}
