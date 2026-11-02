//! Node-local serialization of partition observations, expiry and native-resync ownership.
//! The watchdog still supplies every verdict; polling adds no alternative release policy.
use super::partition_watch::{BlockPlace, PartitionVerdict, PartitionWatch, PeerView, ResyncVerdict};
use crate::flow_context::{ResyncReport, ResyncReporter};
use kaspa_consensus_core::{BlockHash, api::PalwPartitionRefusalV1};
use kaspa_consensusmanager::ConsensusSessionOwned;
use kaspa_core::chain_participation::ChainParticipationGate;
use kaspa_p2p_lib::PeerKey;
use std::{sync::Arc, time::Instant};

#[derive(Default)]
pub(crate) struct PartitionParticipation {
    pub(crate) watch: PartitionWatch,
    pending: Option<ResyncReporter>,
}

pub(crate) enum PartitionUpdate {
    None,
    Ordinary { verdict: PartitionVerdict, changed: bool },
    Confirmed { reporter: ResyncReporter, report: ResyncReport },
}

pub(crate) struct PartitionObservation<'a> {
    pub expected: &'a Option<ResyncReporter>,
    pub block: BlockHash,
    pub peer: PeerKey,
    pub place: BlockPlace,
    pub refusal: Option<PalwPartitionRefusalV1>,
    pub peers: &'a [PeerView],
    pub now: Instant,
    pub report: Option<ResyncReport>,
}

/// Async ancestry results belong to a particular active instance, sink and refusal snapshot.
/// This is a bounded memory read; a pruning-session guard alone does not pin manager adoption.
pub(crate) fn snapshot_matches(
    observed: &ConsensusSessionOwned,
    current: &ConsensusSessionOwned,
    sink: BlockHash,
    refusal: Option<PalwPartitionRefusalV1>,
) -> bool {
    observed.with_consensus_api_v1(|observed| {
        current.with_consensus_api_v1(|active| {
            std::ptr::eq(observed, active) && active.get_sink() == sink && active.palw_partition_refusal_v1() == refusal
        })
    })
}

impl PartitionParticipation {
    pub(crate) fn begin(&mut self, reporter: ResyncReporter, gate: &ChainParticipationGate) {
        self.pending = Some(reporter);
        gate.set_partition_hold(true);
    }

    pub(crate) fn pending(&self) -> Option<ResyncReporter> {
        self.pending.clone()
    }

    /// Called without a new announcement as well as after peer removal. Native confirmation is
    /// deliberately unavailable here: silence/expiry cannot confirm an operator's new database.
    pub(crate) fn refresh(
        &self,
        refusal: Option<PalwPartitionRefusalV1>,
        peers: &[PeerView],
        now: Instant,
        gate: &ChainParticipationGate,
    ) -> PartitionUpdate {
        if self.pending.is_some() {
            return PartitionUpdate::None;
        }
        let verdict = self.watch.verdict(refusal, peers, now);
        let changed = gate.set_partition_hold(matches!(verdict, PartitionVerdict::Hold { .. }));
        PartitionUpdate::Ordinary { verdict, changed }
    }

    /// Caller holds the single state mutex through this method and supplies a current, coherent
    /// consensus snapshot. A completion from before begin/rearm must not affect its new owner.
    pub(crate) fn observe(&mut self, observation: PartitionObservation<'_>, gate: &ChainParticipationGate) -> PartitionUpdate {
        let PartitionObservation { expected, block, peer, place, refusal, peers, now, report } = observation;
        let same_owner = match (&self.pending, expected) {
            (None, None) => true,
            (Some(current), Some(expected)) => Arc::ptr_eq(current, expected),
            _ => false,
        };
        if !same_owner {
            return PartitionUpdate::None;
        }
        self.watch.note_placed(block, peer, place, now);
        if self.pending.is_none() {
            return self.refresh(refusal, peers, now, gate);
        }
        if !gate.chain_settled() {
            return PartitionUpdate::None;
        }
        if let (Some(mut report), ResyncVerdict::Confirmed { on_ours, eligible }) =
            (report, self.watch.resync_verdict(refusal, peers, now))
        {
            report.confirming_outbound_peers = on_ours;
            report.eligible_outbound_peers = eligible;
            let reporter = self.pending.take().expect("owner checked under the partition state lock");
            gate.set_partition_hold(false);
            return PartitionUpdate::Confirmed { reporter, report };
        }
        PartitionUpdate::None
    }
}

#[cfg(test)]
mod tests {
    use super::super::partition_watch::{PARTITION_OBSERVATION_WINDOW, RelayedRelation};
    use super::*;
    use kaspa_consensus_core::api::ConsensusApi;
    use kaspa_utils::networking::{IpAddress, PeerId};
    use parking_lot::Mutex;
    use std::{
        net::IpAddr,
        sync::{
            Barrier,
            atomic::{AtomicUsize, Ordering},
        },
        time::Duration,
    };
    use uuid::Uuid;

    fn key(n: u16) -> PeerKey {
        PeerKey::new(PeerId::new(Uuid::from_u128(n as u128)), IpAddress::new(IpAddr::from([10, 2, 0, n as u8])))
    }

    fn peers() -> Vec<PeerView> {
        (1..=2).map(|n| PeerView { key: key(n), outbound: true, connected_for: Duration::from_secs(600) }).collect()
    }

    fn refusal() -> Option<PalwPartitionRefusalV1> {
        Some(PalwPartitionRefusalV1 {
            sink: BlockHash::from_u64_word(1),
            refused: BlockHash::from_u64_word(2),
            own_daa: 100,
            refused_daa: 110,
            first_refused_daa: 100,
            resolves: 4,
        })
    }

    fn report() -> ResyncReport {
        ResyncReport {
            confirmed_unix_ms: 1,
            sink: BlockHash::from_u64_word(1),
            sink_daa: 100,
            confirming_outbound_peers: 0,
            eligible_outbound_peers: 0,
        }
    }

    fn observation<'a>(expected: &'a Option<ResyncReporter>, peers: &'a [PeerView], now: Instant) -> PartitionObservation<'a> {
        PartitionObservation {
            expected,
            block: BlockHash::from_u64_word(3),
            peer: key(1),
            place: BlockPlace { on_our_chain: true, extends_refused: false },
            refusal: None,
            peers,
            now,
            report: Some(report()),
        }
    }

    fn note(state: &mut PartitionParticipation, relation: RelayedRelation, now: Instant) {
        for peer in peers() {
            state.watch.note_relay(peer.key, relation, now);
        }
    }

    #[test]
    fn partition_refresh_expires_ordinary_hold_without_another_announcement() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        note(&mut state, RelayedRelation::Other, now);
        assert!(matches!(state.refresh(refusal(), &peers(), now, &gate), PartitionUpdate::Ordinary { changed: true, .. }));
        assert!(!gate.allows_participation());
        state.refresh(refusal(), &peers(), now + PARTITION_OBSERVATION_WINDOW, &gate);
        assert!(!gate.allows_participation(), "the existing inclusive 30-minute boundary is preserved");
        // No note_relay/note_placed/announcement occurs between these participation polls.
        assert!(matches!(
            state.refresh(refusal(), &peers(), now + PARTITION_OBSERVATION_WINDOW + Duration::from_nanos(1), &gate),
            PartitionUpdate::Ordinary { changed: true, verdict: PartitionVerdict::Clear(_) }
        ));
        assert!(gate.allows_participation());
    }

    #[test]
    fn partition_refresh_keeps_real_peer_thresholds_and_fresh_refusals() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        note(&mut state, RelayedRelation::Other, now);
        state.refresh(refusal(), &peers(), now, &gate);
        assert!(gate.partition_hold());
        for invalid in [
            vec![peers()[0]],
            peers().into_iter().map(|p| PeerView { outbound: false, ..p }).collect(),
            peers().into_iter().map(|p| PeerView { connected_for: Duration::from_secs(599), ..p }).collect(),
            [peers(), vec![PeerView { key: key(3), ..peers()[0] }, PeerView { key: key(4), ..peers()[0] }]].concat(),
        ] {
            state.refresh(refusal(), &invalid, now, &gate);
            assert!(!gate.partition_hold());
            state.refresh(refusal(), &peers(), now, &gate);
            assert!(gate.partition_hold());
        }
    }

    #[test]
    fn partition_refresh_forget_rechecks_ordinary_but_never_confirms_native() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        note(&mut state, RelayedRelation::Other, now);
        state.refresh(refusal(), &peers(), now, &gate);
        state.watch.forget_peer(&key(2));
        state.refresh(refusal(), &peers()[..1], now, &gate);
        assert!(gate.allows_participation());
        state.begin(Arc::new(|_| panic!("ordinary refresh cannot confirm a native hold")), &gate);
        for peer in peers() {
            state.watch.forget_peer(&peer.key);
        }
        for at in [now, now + PARTITION_OBSERVATION_WINDOW + Duration::from_secs(1)] {
            assert!(matches!(state.refresh(None, &[], at, &gate), PartitionUpdate::None));
            assert!(state.pending().is_some());
            assert!(!gate.allows_participation());
        }
    }

    #[test]
    fn partition_refresh_native_confirmation_keeps_all_existing_requirements() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        note(&mut state, RelayedRelation::ExtendsOurs, now);
        state.begin(Arc::new(|_| {}), &gate);
        let expected = state.pending();
        for invalid in [
            vec![peers()[0]],
            peers().into_iter().map(|p| PeerView { outbound: false, ..p }).collect(),
            peers().into_iter().map(|p| PeerView { connected_for: Duration::from_secs(599), ..p }).collect(),
            [peers(), vec![PeerView { key: key(3), ..peers()[0] }, PeerView { key: key(4), ..peers()[0] }]].concat(),
        ] {
            assert!(matches!(state.observe(observation(&expected, &invalid, now), &gate), PartitionUpdate::None));
            assert!(state.pending().is_some() && gate.partition_hold());
        }
        let valid = peers();
        let mut refusing = observation(&expected, &valid, now);
        refusing.refusal = refusal();
        assert!(matches!(state.observe(refusing, &gate), PartitionUpdate::None));
        assert!(gate.partition_hold());
        assert!(matches!(
            state.observe(observation(&expected, &valid, now), &gate),
            PartitionUpdate::Confirmed { report: ResyncReport { confirming_outbound_peers: 2, eligible_outbound_peers: 2, .. }, .. }
        ));
        assert!(state.pending().is_none() && gate.allows_participation());
    }

    #[test]
    fn partition_refresh_stale_completions_cannot_release_a_new_native_owner() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        let valid = peers();
        note(&mut state, RelayedRelation::ExtendsOurs, now);
        let ordinary = state.pending();
        state.begin(Arc::new(|_| {}), &gate);
        let old_native = state.pending();
        assert!(matches!(state.observe(observation(&ordinary, &valid, now), &gate), PartitionUpdate::None));
        state.begin(Arc::new(|_| panic!("new owner must not be confirmed by an old completion")), &gate);
        assert!(matches!(state.observe(observation(&old_native, &valid, now), &gate), PartitionUpdate::None));
        assert!(state.pending().is_some() && gate.partition_hold());
    }

    #[test]
    fn partition_refresh_concurrent_confirmation_reports_once_outside_the_lock() {
        let state = Arc::new(Mutex::new(PartitionParticipation::default()));
        let gate = Arc::new(ChainParticipationGate::new(true));
        let calls = Arc::new(AtomicUsize::new(0));
        let now = Instant::now();
        let callback: ResyncReporter = {
            let state = state.clone();
            let calls = calls.clone();
            Arc::new(move |_| {
                assert!(state.try_lock().expect("reporter runs outside the state lock").pending().is_none());
                calls.fetch_add(1, Ordering::SeqCst);
            })
        };
        note(&mut state.lock(), RelayedRelation::ExtendsOurs, now);
        state.lock().begin(callback, &gate);
        let expected = state.lock().pending();
        let barrier = Arc::new(Barrier::new(2));
        std::thread::scope(|scope| {
            for _ in 0..2 {
                let state = state.clone();
                let gate = gate.clone();
                let expected = expected.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    barrier.wait();
                    let update = { state.lock().observe(observation(&expected, &peers(), now), &gate) };
                    // Both competing decisions must release their locks before the reentrant
                    // callback probe, so another legitimate holder cannot make try_lock flaky.
                    barrier.wait();
                    if let PartitionUpdate::Confirmed { reporter, report } = update {
                        reporter(&report);
                    }
                });
            }
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(gate.allows_participation());
    }

    #[test]
    fn partition_refresh_cannot_release_base_quarantine_or_confirm_an_unsettled_chain() {
        let gate = ChainParticipationGate::new(true);
        let mut state = PartitionParticipation::default();
        let now = Instant::now();
        note(&mut state, RelayedRelation::Other, now);
        state.refresh(refusal(), &peers(), now, &gate);
        gate.quarantine();
        state.refresh(None, &peers(), now, &gate);
        assert!(!gate.partition_hold() && !gate.allows_participation());
        state.begin(Arc::new(|_| {}), &gate);
        note(&mut state, RelayedRelation::ExtendsOurs, now);
        let expected = state.pending();
        assert!(matches!(state.observe(observation(&expected, &peers(), now), &gate), PartitionUpdate::None));
        assert!(state.pending().is_some() && gate.partition_hold());
    }

    #[test]
    fn partition_refresh_async_snapshot_rejects_replaced_instance_sink_or_refusal() {
        struct Snapshot(Mutex<(BlockHash, Option<PalwPartitionRefusalV1>)>);
        impl ConsensusApi for Snapshot {
            fn get_sink(&self) -> BlockHash {
                self.0.lock().0
            }
            fn palw_partition_refusal_v1(&self) -> Option<PalwPartitionRefusalV1> {
                self.0.lock().1
            }
        }
        let sink = BlockHash::from_u64_word(1);
        let original = Arc::new(Snapshot(Mutex::new((sink, refusal()))));
        let observed = ConsensusSessionOwned::new_without_session_guard(original.clone());
        let current = ConsensusSessionOwned::new_without_session_guard(original.clone());
        assert!(snapshot_matches(&observed, &current, sink, refusal()));
        let replacement = ConsensusSessionOwned::new_without_session_guard(Arc::new(Snapshot(Mutex::new((sink, refusal())))));
        assert!(!snapshot_matches(&observed, &replacement, sink, refusal()), "equal metadata is not the same active instance");
        original.0.lock().0 = BlockHash::from_u64_word(2);
        assert!(!snapshot_matches(&observed, &current, sink, refusal()));
        *original.0.lock() = (sink, None);
        assert!(!snapshot_matches(&observed, &current, sink, refusal()));
    }
}
