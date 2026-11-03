use super::*;
use crate::model::stores::{ghostdag::GhostdagStoreReader, pruning::PruningStoreReader};
use kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2;
use std::sync::atomic::Ordering;

#[tokio::test]
async fn round_observer_cache_preserves_nonempty_status_permits_used_finals_and_recent_rows() {
    use kaspa_consensus_core::palw_execution_lane_v1::PalwExecFinalV1;
    let (config, bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, original) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let bond = original.bond(&adr0125_harness_bond()).unwrap();
    let final_row = PalwExecFinalV1 {
        domain: bundle.base_class_id,
        bond: adr0125_harness_bond(),
        operator_id: bond.operator_id,
        claim_id: kaspa_hashes::Hash64::from_u64_word(0xC1A1),
        execution_root: kaspa_hashes::Hash64::from_u64_word(0xE0),
        credit: 1,
        accepted_blue_score: 0,
    };
    let schedule = adr0125_schedule_of(0, &[final_row]);
    let round = 42 + schedule.domains[0].parity as u64;
    let mut carriage = PalwStateCarriageV2::from_state(&original);
    carriage.round_span = 0;
    carriage.round_finals.insert(final_row.claim_id, final_row);
    carriage.round_schedules.insert(0, schedule);
    carriage.round_permits_used.insert((0, round), 1);
    let scheduled = carriage.into_state(&bundle.state, None).unwrap();
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &scheduled).unwrap();
    let old = vp.palw_round_lane_status_uncached_for_tests_v1(round).unwrap();
    assert!(!old.view.permits.is_empty(), "fixture must exercise real permits, not only empty equality");
    assert_eq!(old.view.used, vec![0]);
    assert_eq!((old.accepted_in_span, old.finals_span, old.finals), (1, 0, 1));
    let before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    for _ in 0..3 {
        assert_eq!(vp.palw_round_lane_status_v1(round), Some(old.clone()));
        assert_eq!(vp.palw_round_view_v1(round), Some(old.view.clone()));
        assert_eq!(vp.palw_recent_executions_v1(10), vp.palw_recent_executions_uncached_for_tests_v1(10));
    }
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), before + 1);
}

#[tokio::test]
async fn round_observer_cache_walks_once_per_exact_target_and_releases_global_guards() {
    let (config, bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let parent_state = {
        let store = vp.palw_state_v2_store.read();
        let (_, delta) = store.delta_of(sink).unwrap();
        kaspa_consensus_core::palw_state_v2::revert_delta_v2(&state, &delta, &bundle.state).unwrap()
    };
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    let writer = vp.clone();
    *vp.template_parent_snapshot_v1.load_probe.lock() = Some(Box::new(move || {
        let thread = std::thread::spawn(move || {
            assert!(writer.palw_state_v2_store.try_write().is_some(), "cold observer cannot hold global PALW guard");
            assert!(writer.virtual_stores.try_write().is_some(), "captured virtual guard must end before the cold walk");
        });
        thread.join().unwrap();
    }));
    let before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    let old = vp.palw_round_lane_status_uncached_for_tests_v1(42);
    assert_eq!(vp.palw_round_lane_status_v1(42), old);
    let (at, owner) = vp.palw_observer_state_at_cached_v1(sink).unwrap();
    assert_eq!((at, owner.state_root()), (sink, state.state_root()));
    vp.palw_state_v2_store.write().set_tip_for_tests(sink, &state).unwrap();
    assert_eq!(vp.palw_round_lane_status_v1(43), vp.palw_round_lane_status_uncached_for_tests_v1(43));
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    assert_eq!(vp.palw_recent_executions_v1(10), vp.palw_recent_executions_uncached_for_tests_v1(10));
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), before + 1);
}

#[tokio::test]
async fn round_observer_cache_invalidates_on_sink_move_and_import_without_new_refusals() {
    let (config, _bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let old_sink = ctx.consensus.get_sink();
    vp.palw_round_lane_status_v1(42).unwrap();
    ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    let sink = ctx.consensus.get_sink();
    assert_ne!(sink, old_sink);
    let before = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    assert_eq!(vp.palw_round_lane_status_v1(43), vp.palw_round_lane_status_uncached_for_tests_v1(43));
    assert_eq!(vp.palw_observer_state_at_cached_v1(sink).unwrap().0, sink);
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), before + 1);
    let pruning_point = vp.pruning_point_store.read().pruning_point().unwrap();
    vp.begin_pruning_point_import(pruning_point).unwrap();
    let before_import = vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed);
    assert!(vp.palw_template_parent_checked_v1(sink).is_err(), "active import cannot hit a stale memo");
    let old = vp.palw_round_lane_status_uncached_for_tests_v1(43);
    assert!(old.is_some(), "the original observer was available during this sidecar import");
    assert_eq!(vp.palw_round_lane_status_v1(43), old);
    assert_eq!(vp.palw_recent_executions_v1(10), vp.palw_recent_executions_uncached_for_tests_v1(10));
    assert_eq!(vp.template_parent_snapshot_v1.loads.load(Ordering::Relaxed), before_import);
}

#[tokio::test]
async fn round_observer_cache_changed_root_uses_identical_uncached_fallback_result() {
    let (config, bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    vp.palw_round_lane_status_v1(42).unwrap();
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let (_, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    let parent_state = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&state, &delta, &bundle.state).unwrap();
    vp.palw_state_v2_store.write().set_tip_for_tests(parent, &parent_state).unwrap();
    vp.palw_state_v2_store
        .write()
        .set_delta_record_for_tests(
            sink,
            crate::model::stores::palw_state_v2::PalwStateDeltaRecordV2 {
                state_root: kaspa_hashes::Hash64::from_u64_word(0xBAD),
                delta_borsh: borsh::to_vec(&delta).unwrap(),
            },
        )
        .unwrap();
    assert!(vp.palw_template_parent_checked_v1(sink).is_err());
    assert_eq!(vp.palw_round_lane_status_v1(42), vp.palw_round_lane_status_uncached_for_tests_v1(42));
    assert_eq!(vp.palw_recent_executions_v1(10), vp.palw_recent_executions_uncached_for_tests_v1(10));
}

#[tokio::test]
async fn round_observer_cache_preserves_original_pruning_snapshot_fallback() {
    let (config, bundle) = adr0125_config();
    let mut ctx = TestContext::new(TestConsensus::new(&config));
    for _ in 0..3 {
        ctx.build_block_template_row(0..1).validate_and_insert_row().await.assert_valid_utxo_tip();
    }
    let vp = ctx.consensus.virtual_processor().clone();
    let (sink, state) = vp.palw_state_v2_store.read().load_tip(&bundle.state).unwrap().unwrap();
    let parent = vp.ghostdag_store.get_selected_parent(sink).unwrap();
    let (_, delta) = vp.palw_state_v2_store.read().delta_of(sink).unwrap();
    let parent_state = kaspa_consensus_core::palw_state_v2::revert_delta_v2(&state, &delta, &bundle.state).unwrap();
    {
        let mut store = vp.palw_state_v2_store.write();
        store.set_pruning_snapshot_for_tests(sink, &state).unwrap();
        store.set_tip_for_tests(parent, &parent_state).unwrap();
        store.delete_delta_for_tests(sink).unwrap();
    }
    assert!(vp.palw_template_parent_checked_v1(sink).is_err(), "the strict optimization genuinely lacks history");
    let old = vp.palw_round_lane_status_uncached_for_tests_v1(42);
    assert!(old.is_some(), "original pruning snapshot fallback succeeds");
    assert_eq!(vp.palw_round_lane_status_v1(42), old);
    assert_eq!(vp.palw_recent_executions_v1(10), vp.palw_recent_executions_uncached_for_tests_v1(10));
}
