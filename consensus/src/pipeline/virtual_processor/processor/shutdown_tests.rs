use super::*;
use crate::consensus::test_consensus::TestConsensus;
use crate::pipeline::deps_manager::BlockTask;
use kaspa_consensus_core::{
    api::ConsensusApi,
    blockstatus::BlockStatus,
    config::{ConfigBuilder, params::MAINNET_PARAMS},
};
use kaspa_consensus_notify::notification::PruningPointUtxoSetOverrideNotification;

#[test]
fn stopping_virtual_drains_queued_tasks_preserves_db_and_critical_notifications() {
    let config = ConfigBuilder::new(MAINNET_PARAMS).skip_proof_of_work().build();
    let (notify_tx, notify_rx) = async_channel::unbounded();
    let tc = TestConsensus::with_notifier(&config, notify_tx, Default::default());
    let handles = tc.init();
    let vp = tc.virtual_processor().clone();
    let sink = tc.get_sink();
    let block = tc.get_block(sink).unwrap();
    tc.shutdown(handles);
    assert!(vp.is_consensus_exiting.load(Ordering::Acquire));
    assert!(matches!(vp.check_shutdown_boundary(), Err(PalwLocalTransitionFaultV1::Shutdown)));
    assert!(
        matches!(vp.palw_candidate_state_v2_checked(sink), Err(PalwWeighFaultV2::NoOpinion)),
        "ordinary/in-block readers retain their non-PALW behavior even while this instance stops"
    );
    assert!(
        vp.palw_candidate_state_for_selection_v2(sink).is_err(),
        "selection shutdown must never become absent state or GateInactive"
    );
    let before: Vec<_> = vp.db.iterator(rocksdb::IteratorMode::Start).map(Result::unwrap).collect();
    let root = tc.notification_root();
    root.send(Notification::PruningPointUtxoSetOverride(PruningPointUtxoSetOverrideNotification {})).unwrap();
    assert!(!root.is_closed());

    let (tx, rx) = crossbeam_channel::unbounded();
    let mut results = Vec::new();
    for _ in 0..4 {
        let (reply, result) = tokio::sync::oneshot::channel();
        tx.send(VirtualStateProcessingMessage::Process(BlockTask::Ordinary { block: block.clone() }, reply)).unwrap();
        results.push(result);
    }
    tx.send(VirtualStateProcessingMessage::Exit).unwrap();
    vp.worker_with_receiver(&rx);
    for result in results {
        assert!(
            matches!(futures::executor::block_on(result).unwrap(), Err(RuleError::ConsensusExiting)),
            "every queued caller receives a local stop, never channel cancellation or peer invalidity"
        );
    }
    assert!(
        matches!(vp.pruning_receiver.try_recv(), Ok(PruningProcessingMessage::Exit)),
        "the existing downstream Exit path is retained"
    );
    assert_eq!(tc.get_sink(), sink);
    assert_eq!(tc.block_status(sink), BlockStatus::StatusUTXOValid);
    let after: Vec<_> = vp.db.iterator(rocksdb::IteratorMode::Start).map(Result::unwrap).collect();
    assert_eq!(before, after, "abandoned resolution must not partially publish virtual or alter block validity");
    assert!(!root.is_closed(), "a virtual worker must not close the root before all consensus workers join");
    root.close(); // The normal manager/factory closes only after workers join.
    let mut critical = 0;
    while let Ok(n) = notify_rx.try_recv() {
        critical += usize::from(matches!(n, Notification::PruningPointUtxoSetOverride(_)));
    }
    assert_eq!(critical, 1, "already queued critical notifications survive shutdown and drain before EOF");
    assert!(notify_rx.is_closed());

    let (next_tx, _) = async_channel::unbounded();
    let reopened = TestConsensus::with_db(vp.db.clone(), &config, next_tx);
    let next_handles = reopened.init();
    assert!(!reopened.virtual_processor().is_consensus_exiting.load(Ordering::Acquire));
    assert_eq!(reopened.get_sink(), sink);
    assert_eq!(reopened.block_status(sink), BlockStatus::StatusUTXOValid);
    reopened.shutdown(next_handles);
}
