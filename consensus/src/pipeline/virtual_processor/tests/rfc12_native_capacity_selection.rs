//! Local admission exhaustion must not become a weaker fork-choice verdict.
//! Run only in the consolidated verification phase. The child isolates global
//! admission settings from every other consensus fixture in this test binary.
use super::*;
use crate::model::stores::{pruning::PruningStoreReader, virtual_state::VirtualStateStoreReader};
use kaspa_consensus_core::{
    config::params::ForkActivation,
    palw_mode_v2::PalwConsensusMode,
    palw_native_resource_v1::{NativeSnapshotResourceLimitsV1, arm_native_logical_limits_v1},
};

#[test]
fn local_capacity_does_not_choose_an_ancestor_or_disqualify_valid_blocks() {
    let worker = format!("{}::capacity_selection_worker", module_path!().split_once("::").unwrap().1);
    let status = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &worker, "--nocapture", "--test-threads=1"])
        .env("MISAKA_RFC12_CAPACITY_SELECTION_CHILD", "1")
        .status()
        .unwrap();
    assert!(status.success(), "isolated native capacity counterexample failed: {status}");
}

#[tokio::test]
async fn capacity_selection_worker() {
    if std::env::var("MISAKA_RFC12_CAPACITY_SELECTION_CHILD").as_deref() != Ok("1") {
        return;
    }
    let catalog = palw_v2_test_catalog();
    let bundle = palw_v2_test_bundle(&catalog);
    let config = ConfigBuilder::new(MAINNET_PARAMS)
        .skip_proof_of_work()
        .edit_consensus_params(|p| {
            p.palw_consensus_mode = PalwConsensusMode::ConsensusV2(bundle.clone());
            p.palw_frontier_provenance = Some(ForkActivation::always());
            *p = p.clone().with_palw_v2_cadence();
        })
        .build();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor();
    let sink = ctx.consensus.get_sink();
    let ancestor = vp.headers_store.get_header(sink).unwrap().direct_parents()[0];
    let before_status = (ctx.consensus.block_status(sink), ctx.consensus.block_status(ancestor));
    let before_root = vp.palw_state_v2_store.read().state_root_of(sink).unwrap();
    let daa = vp.headers_store.get_daa_score(sink).unwrap();
    let mut bonds = vp.initial_active_bond_view();
    assert!(vp.palw_candidate_order_v2_checked(sink).unwrap().is_some());
    assert!(vp.dns_reorg_outcome_checked(ancestor, sink, &bonds).is_ok());
    assert!(vp.palw_rule_e_gate_v1(ancestor, sink, daa).is_ok());
    assert!(vp.palw_frontier_provenance_outcome_checked(sink, ancestor).is_ok());

    // Arm only after constructing a real valid chain. Neither local quota nor
    // its refusal belongs to the consensus rule or the candidate's identity.
    arm_native_logical_limits_v1(Some(NativeSnapshotResourceLimitsV1 { memory_bytes: 0, database_bytes: 0 }));
    assert!(vp.palw_candidate_order_v2_checked(sink).is_err());
    assert!(ctx.consensus.get_palw_candidate_order_v2_checked().is_err());
    assert!(ctx.consensus.get_palw_rule_e_weighing_v1_checked().is_err());
    assert!(vp.palw_weighing_point_v2_checked(blockhash::NONE).is_err(), "sinkless staging must preserve capacity too");
    assert!(vp.dns_reorg_outcome_checked(ancestor, sink, &bonds).is_err());
    assert!(vp.palw_rule_e_gate_v1(ancestor, sink, daa).is_err());
    assert!(vp.palw_frontier_provenance_outcome_checked(sink, ancestor).is_err());
    let mut states = super::super::palw_rule_e::PalwRuleEStatesV1::default();
    assert!(vp.palw_rule_e_prefers_v1(&mut states, ancestor, sink).is_err());

    let pruning_point = vp.pruning_point_store.read().pruning_point().unwrap();
    let virtual_read = vp.virtual_stores.upgradable_read();
    let previous = virtual_read.state.get().unwrap();
    let finality = vp.virtual_finality_point(&previous.ghostdag_data, pruning_point);
    let mut diff = previous.utxo_diff.clone().to_reversed();
    assert!(vp.sink_search_algorithm(&virtual_read, &mut diff, &mut bonds, sink, vec![ancestor], finality, pruning_point).is_err());
    drop(virtual_read);
    assert_eq!(ctx.consensus.get_sink(), sink, "capacity cannot commit a lower ancestor as the new sink");
    assert_eq!((ctx.consensus.block_status(sink), ctx.consensus.block_status(ancestor)), before_status);
    assert_eq!(vp.palw_state_v2_store.read().state_root_of(sink).unwrap(), before_root);
}
