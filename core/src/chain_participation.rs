//! One answer to "may this node act on the chain it is holding?", for every service that acts.
//!
//! Mining, the in-process validator, the external validator's `is_synced` poll, and compute each
//! used to decide this for themselves. They did not agree. `MiningRuleEngine::should_mine` mixes in
//! sync-rate and peer-connectivity rules that are about *mining* throughput; `getServerInfo` and
//! `getSyncStatus` computed `is_synced` differently from each other; and the in-process validator
//! consulted none of them — it read consensus directly and attested. So a node could be mid-IBD,
//! having just replaced its active consensus with a branch nobody compared, and still sign an
//! attestation for that branch. That is the exact step that turns a transient fork into a
//! branch-local DNS anchor the other side can never overcome.
//!
//! This gate exists so that decision is made once, in one place, and every participation path asks
//! it. Adding a new signer means calling [`ChainParticipationGate::allows_participation`]; the
//! failure mode of forgetting is a service that participates when the node has said it should not,
//! which is precisely the bug this replaces.
//!
//! It deliberately says nothing about *which* chain is right. It only tracks whether this node has
//! finished deciding.

use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

use crate::time::unix_now;

/// Where the node is in adopting a chain.
///
/// Only [`ChainParticipation::Ready`] permits mining, attesting, or reporting `is_synced`.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ChainParticipation {
    /// The node is acting on a chain it has settled on.
    Ready,
    /// An IBD holds the latch. The chain may be replaced at any moment — `staging.commit()` happens
    /// partway through, so "IBD is running" already includes "active consensus may have changed".
    IbdRunning,
    /// An IBD finished and the chain it produced has not been weighed against the alternatives.
    /// Participation stays closed at least until `until_ms`.
    CandidateReview,
    /// The node is running a chain it cannot vouch for and cannot fix by itself: `staging.commit()`
    /// swapped in a new active consensus and the IBD then failed. Never clears on its own.
    Quarantined,
}

impl ChainParticipation {
    /// Short stable slug for logs and RPC. Not localized; treated as an identifier by clients.
    pub fn as_str(&self) -> &'static str {
        match self {
            ChainParticipation::Ready => "ready",
            ChainParticipation::IbdRunning => "ibd-running",
            ChainParticipation::CandidateReview => "candidate-review",
            ChainParticipation::Quarantined => "quarantined",
        }
    }
}

/// Where the gate writes its state so a restart cannot clear it.
///
/// Defined as a trait rather than a store because this crate sits below the database layer. The
/// implementation lives next to the node's meta DB; the gate only needs somewhere to put the fact.
pub trait ChainParticipationPersistence: Send + Sync + std::fmt::Debug {
    /// Called on every transition. Implementations must be non-panicking: a node that cannot write
    /// this should keep running with an in-memory gate, not die.
    fn persist(&self, state: ChainParticipation, review_until_ms: u64);

    /// Record how many times this node has abandoned a sync for a verified-better chain.
    ///
    /// Kept with the participation state because it bounds the same thing: a switch cap that a
    /// restart resets is not a cap, and restarting is exactly what an operator does when a node
    /// looks stuck flipping between branches.
    fn persist_switches(&self, switches: u32);

    /// The switch count from the previous run, or 0 on a fresh node.
    fn restore_switches(&self) -> u32;
}

const READY: u8 = 0;
const IBD_RUNNING: u8 = 1;
const CANDIDATE_REVIEW: u8 = 2;
const QUARANTINED: u8 = 3;

/// The shared gate. Cheap to read — every heartbeat and every `getBlockTemplate` hits it.
#[derive(Debug)]
pub struct ChainParticipationGate {
    state: AtomicU8,
    /// Unix-ms floor for [`ChainParticipation::CandidateReview`]. Only meaningful in that state.
    review_until_ms: AtomicU64,
    /// Whether the gate constrains anything on this network. Peerless devnet/simnet nodes have no
    /// competing branch to overlook and no peers to wait for, so holding them back only stalls
    /// tests; this mirrors the carve-out `has_sufficient_peer_connectivity` already makes.
    enabled: bool,
    /// Where transitions are recorded so they survive a restart. `None` keeps the gate purely
    /// in-memory, which is what tests and disabled networks use.
    persistence: Option<Arc<dyn ChainParticipationPersistence>>,
}

impl ChainParticipationGate {
    pub fn new(enabled: bool) -> Self {
        Self { state: AtomicU8::new(READY), review_until_ms: AtomicU64::new(0), enabled, persistence: None }
    }

    /// Attach durable storage and adopt whatever was last written.
    ///
    /// Restoring matters most for `Quarantined`: it is entered because the node cannot vouch for
    /// the chain it is running, and nothing about restarting the process makes that untrue. A
    /// `CandidateReview` floor is restored as an absolute deadline, so a restart neither extends
    /// nor escapes it.
    pub fn with_persistence(
        mut self,
        persistence: Arc<dyn ChainParticipationPersistence>,
        restored: Option<(ChainParticipation, u64)>,
    ) -> Self {
        if let Some((state, review_until_ms)) = restored {
            self.state.store(
                match state {
                    ChainParticipation::Ready => READY,
                    // An IBD that was running when the process died did not finish, and whatever it
                    // was doing to the active consensus is now in an unknown state. That is the
                    // quarantine case, not a reason to start clean.
                    ChainParticipation::IbdRunning | ChainParticipation::Quarantined => QUARANTINED,
                    ChainParticipation::CandidateReview => CANDIDATE_REVIEW,
                },
                Ordering::SeqCst,
            );
            self.review_until_ms.store(review_until_ms, Ordering::SeqCst);
        }
        self.persistence = Some(persistence);
        self
    }

    fn persist(&self) {
        if let Some(p) = self.persistence.as_ref() {
            p.persist(self.peek(), self.review_until_ms.load(Ordering::SeqCst));
        }
    }

    /// Record a chain switch durably. Bounded by the same reasoning as the gate itself: a cap a
    /// restart resets is not a cap.
    pub fn record_switch(&self, switches: u32) {
        if let Some(p) = self.persistence.as_ref() {
            p.persist_switches(switches);
        }
    }

    /// Switches carried over from previous runs.
    pub fn restored_switches(&self) -> u32 {
        self.persistence.as_ref().map(|p| p.restore_switches()).unwrap_or(0)
    }

    /// State without the elapsed-review promotion, for persisting and for internal checks.
    fn peek(&self) -> ChainParticipation {
        match self.state.load(Ordering::SeqCst) {
            IBD_RUNNING => ChainParticipation::IbdRunning,
            CANDIDATE_REVIEW => ChainParticipation::CandidateReview,
            QUARANTINED => ChainParticipation::Quarantined,
            _ => ChainParticipation::Ready,
        }
    }

    /// A gate that never holds anything back, for tests and single-node networks.
    pub fn disabled() -> Self {
        Self::new(false)
    }

    /// Close the gate for the duration of an IBD. Called when the IBD latch is taken, **not** when
    /// the IBD finishes: `staging.commit()` runs partway through, so by the time an IBD reports
    /// success the node has already been running on the new chain for minutes.
    ///
    /// Does not disturb [`ChainParticipation::Quarantined`] — an unresolved problem outranks a new
    /// attempt to sync, and a node must not be able to launder a quarantine by starting an IBD.
    pub fn enter_ibd(&self) {
        if !self.enabled {
            return;
        }
        let _ = self.state.compare_exchange(READY, IBD_RUNNING, Ordering::SeqCst, Ordering::SeqCst);
        let _ = self.state.compare_exchange(CANDIDATE_REVIEW, IBD_RUNNING, Ordering::SeqCst, Ordering::SeqCst);
        self.persist();
    }

    /// An IBD finished and produced a chain that nothing has yet been compared against. Hold
    /// participation for at least `min_review_ms`.
    ///
    /// The floor is a floor, not a verdict: when it expires the node resumes because time passed,
    /// not because anything was checked. That is a placeholder for the pre-commit candidate
    /// comparison, and the only reason it is acceptable meanwhile is that the alternative — never
    /// releasing — is a node that can never mine again.
    pub fn enter_candidate_review(&self, min_review_ms: u64) {
        if !self.enabled || self.is_quarantined() {
            return;
        }
        self.review_until_ms.fetch_max(unix_now().saturating_add(min_review_ms), Ordering::SeqCst);
        let _ = self.state.compare_exchange(IBD_RUNNING, CANDIDATE_REVIEW, Ordering::SeqCst, Ordering::SeqCst);
        let _ = self.state.compare_exchange(READY, CANDIDATE_REVIEW, Ordering::SeqCst, Ordering::SeqCst);
        self.persist();
    }

    /// Stop participating until an operator intervenes. Irreversible within the process.
    ///
    /// Reserved for what this node can state without guessing: the active consensus was replaced
    /// and the IBD then failed, so it is running a chain whose sync never completed. Failing closed
    /// is the point — the alternative is a node that keeps signing while it has no idea what it is
    /// on. Note this is deliberately NOT triggered by unverified peer claims; see
    /// `FlowContext::finish_ibd_after_success` for why that would take honest nodes offline.
    pub fn quarantine(&self) {
        if !self.enabled {
            return;
        }
        self.state.store(QUARANTINED, Ordering::SeqCst);
        self.persist();
    }

    /// The IBD ended without having replaced anything, so there is nothing to review.
    pub fn release_after_noop_ibd(&self) {
        if !self.enabled {
            return;
        }
        let _ = self.state.compare_exchange(IBD_RUNNING, READY, Ordering::SeqCst, Ordering::SeqCst);
        self.persist();
    }

    pub fn is_quarantined(&self) -> bool {
        self.state.load(Ordering::SeqCst) == QUARANTINED
    }

    /// Current state, promoting an elapsed review to `Ready` as a side effect.
    pub fn state(&self) -> ChainParticipation {
        match self.state.load(Ordering::SeqCst) {
            IBD_RUNNING => ChainParticipation::IbdRunning,
            QUARANTINED => ChainParticipation::Quarantined,
            CANDIDATE_REVIEW => {
                if unix_now() < self.review_until_ms.load(Ordering::SeqCst) {
                    ChainParticipation::CandidateReview
                } else {
                    if self.state.compare_exchange(CANDIDATE_REVIEW, READY, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        self.persist();
                    }
                    ChainParticipation::Ready
                }
            }
            _ => ChainParticipation::Ready,
        }
    }

    /// **The** question. May this node mine, attest, sign, or call itself synced?
    ///
    /// Every participation path must go through here rather than reasoning about sync on its own.
    /// It answers only about this node's confidence in its own chain — callers still apply their
    /// own conditions on top (a miner also needs peers, a validator also needs an active bond).
    pub fn allows_participation(&self) -> bool {
        !self.enabled || self.state() == ChainParticipation::Ready
    }

    /// Milliseconds left on a review floor, for operator-facing reporting.
    pub fn review_remaining_ms(&self) -> Option<u64> {
        if self.state() != ChainParticipation::CandidateReview {
            return None;
        }
        let now = unix_now();
        let until = self.review_until_ms.load(Ordering::SeqCst);
        (until > now).then_some(until - now)
    }
}

impl Default for ChainParticipationGate {
    fn default() -> Self {
        Self::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Stands in for the meta-DB row, so a "restart" is just building a new gate from what the
    /// previous one wrote.
    #[derive(Debug, Default)]
    struct Recorder(Mutex<Option<(ChainParticipation, u64)>>, Mutex<u32>);

    impl ChainParticipationPersistence for Recorder {
        fn persist(&self, state: ChainParticipation, review_until_ms: u64) {
            *self.0.lock().unwrap() = Some((state, review_until_ms));
        }

        fn persist_switches(&self, switches: u32) {
            *self.1.lock().unwrap() = switches;
        }

        fn restore_switches(&self) -> u32 {
            *self.1.lock().unwrap()
        }
    }

    impl Recorder {
        fn last(&self) -> Option<(ChainParticipation, u64)> {
            *self.0.lock().unwrap()
        }
    }

    #[test]
    fn a_disabled_gate_permits_everything() {
        let gate = ChainParticipationGate::disabled();
        gate.enter_ibd();
        assert!(gate.allows_participation());
        gate.quarantine();
        assert!(gate.allows_participation(), "a disabled gate has no opinions, including quarantine");
    }

    #[test]
    fn participation_closes_when_ibd_starts_not_when_it_finishes() {
        // The whole point: `staging.commit()` runs partway through an IBD, so waiting for success
        // means the node signs on the new chain for minutes before the gate ever closes.
        let gate = ChainParticipationGate::new(true);
        assert!(gate.allows_participation());

        gate.enter_ibd();
        assert_eq!(gate.state(), ChainParticipation::IbdRunning);
        assert!(!gate.allows_participation());
    }

    #[test]
    fn review_holds_for_its_floor_then_releases() {
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);
        assert!(!gate.allows_participation());
        assert!(gate.review_remaining_ms().is_some_and(|ms| ms > 55_000));

        gate.enter_candidate_review(0);
        // fetch_max keeps the longer floor, so a zero-length review cannot cut one short.
        assert!(!gate.allows_participation(), "a later shorter review must not release the node early");
    }

    #[test]
    fn an_elapsed_review_releases() {
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(0);
        assert_eq!(gate.state(), ChainParticipation::Ready);
        assert!(gate.allows_participation());
    }

    #[test]
    fn quarantine_never_clears_on_its_own() {
        let gate = ChainParticipationGate::new(true);
        gate.quarantine();

        // Not by time, not by review, and crucially not by starting another IBD — otherwise a node
        // could launder an unresolved quarantine by triggering a resync.
        gate.enter_candidate_review(0);
        assert!(gate.is_quarantined());
        gate.enter_ibd();
        assert!(gate.is_quarantined());
        gate.release_after_noop_ibd();
        assert_eq!(gate.state(), ChainParticipation::Quarantined);
        assert!(!gate.allows_participation());
    }

    #[test]
    fn a_failed_ibd_that_changed_nothing_does_not_hold_the_node() {
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.release_after_noop_ibd();
        assert!(gate.allows_participation());
    }

    #[test]
    fn a_switch_cap_survives_a_restart() {
        // A cap that a restart resets is not a cap — and restarting is exactly what an operator does
        // when a node looks stuck flipping between branches.
        let recorder = Arc::new(Recorder::default());
        recorder.persist_switches(4);
        assert_eq!(recorder.restore_switches(), 4);
    }

    #[test]
    fn a_quarantine_survives_a_restart() {
        // Restarting the process does not compare the chain, so it must not clear the refusal to
        // act on it. Before persistence, `kaspad` restart was a working bypass.
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone(), None);
        gate.quarantine();
        assert_eq!(recorder.last().map(|(s, _)| s), Some(ChainParticipation::Quarantined));

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone(), recorder.last());
        assert!(restarted.is_quarantined());
        assert!(!restarted.allows_participation());
    }

    #[test]
    fn a_review_floor_survives_a_restart_without_being_extended() {
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone(), None);
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        let (_, deadline) = recorder.last().unwrap();

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone(), recorder.last());
        assert_eq!(restarted.state(), ChainParticipation::CandidateReview);
        // Absolute deadline, so restarting neither escapes the floor nor restarts the clock.
        assert_eq!(restarted.review_until_ms.load(Ordering::SeqCst), deadline);
    }

    #[test]
    fn dying_mid_ibd_comes_back_quarantined() {
        // An IBD that was running when the process died did not finish. `staging.commit()` may have
        // already swapped the active consensus, and nothing recorded whether it did — so the node
        // cannot vouch for what it is now running.
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone(), None);
        gate.enter_ibd();
        assert_eq!(recorder.last().map(|(s, _)| s), Some(ChainParticipation::IbdRunning));

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone(), recorder.last());
        assert!(restarted.is_quarantined());
    }

    #[test]
    fn a_ready_node_restarts_ready() {
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone(), None);
        gate.enter_ibd();
        gate.release_after_noop_ibd();

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone(), recorder.last());
        assert!(restarted.allows_participation());
    }
}
