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

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

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
}

impl ChainParticipationGate {
    pub fn new(enabled: bool) -> Self {
        Self { state: AtomicU8::new(READY), review_until_ms: AtomicU64::new(0), enabled }
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
    }

    /// The IBD ended without having replaced anything, so there is nothing to review.
    pub fn release_after_noop_ibd(&self) {
        if !self.enabled {
            return;
        }
        let _ = self.state.compare_exchange(IBD_RUNNING, READY, Ordering::SeqCst, Ordering::SeqCst);
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
                    let _ = self.state.compare_exchange(CANDIDATE_REVIEW, READY, Ordering::SeqCst, Ordering::SeqCst);
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
}
