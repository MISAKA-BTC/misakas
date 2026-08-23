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
    atomic::{AtomicBool, AtomicU8, AtomicU32, AtomicU64, Ordering},
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
/// Everything about this node's relationship to its chain that a restart must not reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChainParticipationSnapshot {
    pub state: ChainParticipation,
    /// Absolute unix-ms deadline of a review floor, so a restart neither extends nor escapes it.
    pub review_until_ms: u64,
    /// Whether this node has ever finished a review and started participating.
    ///
    /// A one-way door. Before it, the chain held is provisional and may be replaced by a
    /// verified-better one; after it, the node has mined on that chain, attested it and told peers
    /// it was synced, so withdrawing is a reorg rather than a correction.
    pub ever_ready: bool,
    /// Increments each time a chain is provisionally adopted, binding a recovery permit to the
    /// situation that issued it.
    pub adoption_generation: u64,
    /// Syncs abandoned for a verified-better chain. A cap a restart resets is not a cap.
    pub switches: u32,
}

pub trait ChainParticipationPersistence: Send + Sync + std::fmt::Debug {
    /// Called on every transition. Implementations must be non-panicking: a node that cannot write
    /// this should keep running with an in-memory gate, not die.
    fn persist(&self, snapshot: ChainParticipationSnapshot);

    /// The snapshot from the previous run, if any.
    fn restore(&self) -> Option<ChainParticipationSnapshot>;
}

/// Identifies one IBD attempt's claim on the participation state.
///
/// Generation 0 is the null lease: a disabled gate, or an `enter_ibd` that changed nothing because
/// the node was quarantined or already syncing. It restores nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IbdLease {
    generation: u64,
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
    /// See [`ChainParticipationSnapshot::ever_ready`].
    ever_ready: AtomicBool,
    adoption_generation: AtomicU64,
    switches: AtomicU32,
    /// When this node last entered `Ready`, or 0 while it is not Ready.
    ///
    /// In memory only, deliberately: it answers "how long has participation been uninterrupted in
    /// THIS process", and a restart is an interruption. Persisting it would let a node inherit
    /// stability it did not have.
    ready_since_ms: AtomicU64,
    /// What to return to when an IBD changes nothing. Written on entry, read on a no-op failure.
    /// Only one IBD runs at a time — the latch guarantees it — so a single slot is enough.
    pre_ibd_state: AtomicU8,
    /// Which IBD owns [`Self::pre_ibd_state`]. An attempt that finishes late must not restore a
    /// state that a newer attempt has already moved on from — "no-op" is a claim about the attempt
    /// that made it, and a stale one would roll the node backwards.
    ibd_generation: AtomicU64,
    /// Whether the gate constrains anything on this network. Peerless devnet/simnet nodes have no
    /// competing branch to overlook and no peers to wait for, so holding them back only stalls
    /// tests; this mirrors the carve-out `has_sufficient_peer_connectivity` already makes.
    enabled: bool,
    /// Where transitions are recorded so they survive a restart. `None` keeps the gate purely
    /// in-memory, which is what tests and disabled networks use.
    persistence: Option<Arc<dyn ChainParticipationPersistence>>,
    /// Set while a candidate that HAS produced a valid pruning proof is still being weighed.
    ///
    /// The review floor and this are different deadlines and were conflated. The floor answers "has
    /// anything turned up?", and it must expire or a quiet node could never participate. This
    /// answers "is something in the middle of being decided?", and letting the floor expire through
    /// it would mean going Ready while holding evidence that the chain might be wrong.
    ///
    /// Only a candidate backed by a valid proof may hold it. A peer that cannot produce one has no
    /// claim on this node's time — that is the difference between fail-closed and hostage.
    decision_pending: AtomicBool,
}

impl ChainParticipationGate {
    pub fn new(enabled: bool) -> Self {
        Self {
            state: AtomicU8::new(READY),
            review_until_ms: AtomicU64::new(0),
            ever_ready: AtomicBool::new(false),
            decision_pending: AtomicBool::new(false),
            adoption_generation: AtomicU64::new(0),
            switches: AtomicU32::new(0),
            ready_since_ms: AtomicU64::new(unix_now()),
            pre_ibd_state: AtomicU8::new(READY),
            ibd_generation: AtomicU64::new(0),
            enabled,
            persistence: None,
        }
    }

    /// Attach durable storage and adopt whatever was last written.
    ///
    /// Restoring matters most for `Quarantined`: it is entered because the node cannot vouch for
    /// the chain it is running, and nothing about restarting the process makes that untrue. A
    /// `CandidateReview` floor is restored as an absolute deadline, so a restart neither extends
    /// nor escapes it.
    pub fn with_persistence(mut self, persistence: Arc<dyn ChainParticipationPersistence>) -> Self {
        if let Some(restored) = persistence.restore() {
            self.ever_ready.store(restored.ever_ready, Ordering::SeqCst);
            self.adoption_generation.store(restored.adoption_generation, Ordering::SeqCst);
            self.switches.store(restored.switches, Ordering::SeqCst);
            self.state.store(
                match restored.state {
                    ChainParticipation::Ready => READY,
                    // An IBD that was running when the process died did not finish, and whatever it
                    // was doing to the active consensus is now in an unknown state. That is the
                    // quarantine case, not a reason to start clean.
                    ChainParticipation::IbdRunning | ChainParticipation::Quarantined => QUARANTINED,
                    ChainParticipation::CandidateReview => CANDIDATE_REVIEW,
                },
                Ordering::SeqCst,
            );
            self.review_until_ms.store(restored.review_until_ms, Ordering::SeqCst);
        }
        self.persistence = Some(persistence);
        self
    }

    fn persist(&self) {
        if let Some(p) = self.persistence.as_ref() {
            p.persist(self.snapshot());
        }
    }

    pub fn snapshot(&self) -> ChainParticipationSnapshot {
        ChainParticipationSnapshot {
            state: self.peek(),
            review_until_ms: self.review_until_ms.load(Ordering::SeqCst),
            ever_ready: self.ever_ready.load(Ordering::SeqCst),
            adoption_generation: self.adoption_generation.load(Ordering::SeqCst),
            switches: self.switches.load(Ordering::SeqCst),
        }
    }

    /// Record a chain switch durably. A cap a restart resets is not a cap.
    pub fn record_switch(&self, switches: u32) {
        self.switches.fetch_max(switches, Ordering::SeqCst);
        self.persist();
    }

    pub fn restored_switches(&self) -> u32 {
        self.switches.load(Ordering::SeqCst)
    }

    /// How long participation has been uninterrupted, or `None` if the node is not participating.
    ///
    /// The switch cap is a guard against two chains trading the latch — a burst. This is how a
    /// caller tells a burst from a long, healthy life: a node that has been `Ready` for a stable
    /// stretch is not mid-ping-pong, whatever its lifetime switch count says.
    ///
    /// An IBD that replaced nothing does NOT restart the clock, and should not: the node adopted no
    /// other chain, so its participation was not interrupted in the sense the cap cares about. An
    /// adoption does restart it, because the promotion out of `CandidateReview` marks it.
    pub fn ready_stable_for_ms(&self) -> Option<u64> {
        if !self.enabled {
            return None;
        }
        if self.state() != ChainParticipation::Ready {
            return None;
        }
        match self.ready_since_ms.load(Ordering::SeqCst) {
            0 => None,
            since => Some(unix_now().saturating_sub(since)),
        }
    }

    /// Start the switch budget over. Pairs with `IbdCandidateRegistry::reset_switches` — the
    /// registry resumes from `max(own, this)`, so clearing one without the other does nothing.
    pub fn clear_switches(&self) {
        if !self.enabled {
            return;
        }
        self.switches.store(0, Ordering::SeqCst);
        self.persist();
    }

    /// Mark participation as (re)started now. Private: the gate decides this, not its callers.
    fn mark_ready_now(&self) {
        self.ready_since_ms.store(unix_now(), Ordering::SeqCst);
    }

    /// Participation stopped; the stability clock is not running.
    fn mark_not_ready(&self) {
        self.ready_since_ms.store(0, Ordering::SeqCst);
    }

    /// Hold the review open: a candidate with a valid pruning proof is being weighed.
    ///
    /// Reserved for proof-backed candidates. A peer that merely claims something must not be able
    /// to keep a node from participating by claiming it repeatedly.
    pub fn begin_decision(&self) {
        if self.enabled {
            self.decision_pending.store(true, Ordering::SeqCst);
        }
    }

    /// The candidate has been decided — adopted, refused, or timed out. Release the hold.
    pub fn end_decision(&self) {
        self.decision_pending.store(false, Ordering::SeqCst);
    }

    pub fn decision_pending(&self) -> bool {
        self.decision_pending.load(Ordering::SeqCst)
    }

    /// Whether this node has ever participated. See [`ChainParticipationSnapshot::ever_ready`].
    pub fn ever_ready(&self) -> bool {
        self.ever_ready.load(Ordering::SeqCst)
    }

    pub fn adoption_generation(&self) -> u64 {
        self.adoption_generation.load(Ordering::SeqCst)
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
    /// Returns the lease identifying this attempt. Hand it back to [`Self::release_after_noop_ibd`];
    /// without it a late-finishing attempt could restore a state a newer one has moved past.
    ///
    /// Deliberately does NOT move a quarantined node: an unresolved problem outranks a new attempt
    /// to sync, and the lease it gets back cannot restore anything either.
    pub fn enter_ibd(&self) -> IbdLease {
        if !self.enabled {
            return IbdLease { generation: 0 };
        }
        // The interrupted state and the lease are established together with the transition itself,
        // so there is no window in which the node is IbdRunning without a recorded way back.
        let interrupted = if self.state.compare_exchange(READY, IBD_RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            Some(READY)
        } else if self.state.compare_exchange(CANDIDATE_REVIEW, IBD_RUNNING, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            Some(CANDIDATE_REVIEW)
        } else {
            None
        };
        let generation = match interrupted {
            Some(state) => {
                self.pre_ibd_state.store(state, Ordering::SeqCst);
                self.ibd_generation.fetch_add(1, Ordering::SeqCst) + 1
            }
            // Quarantined, or already IbdRunning. Generation 0 never matches, so this lease can
            // restore nothing.
            None => 0,
        };
        self.persist();
        IbdLease { generation }
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
        // A chain was just provisionally adopted. The generation binds any recovery permit to this
        // adoption, so one issued for an earlier situation cannot be redeemed against this one.
        self.adoption_generation.fetch_add(1, Ordering::SeqCst);
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
        self.mark_not_ready();
        self.persist();
    }

    /// The IBD ended without having replaced anything, so there is nothing NEW to review — and
    /// nothing has been resolved either. The node goes back to whatever it was doing before.
    ///
    /// This used to move unconditionally to `Ready`, which is only correct for a node that was
    /// already Ready. For a node in `CandidateReview` it was a promotion earned by a stranger's
    /// failure: a second peer offering a chain this node could not sync would end the review of the
    /// FIRST peer's chain — the review that existed precisely because nothing had compared it. The
    /// node resumed mining and attesting on an unreviewed chain, `ever_ready` latched, and the
    /// one-way door shut permanently.
    ///
    /// Measured: a failed IBD from the peer holding the heavier branch is what promoted the node
    /// out of review and onto the lighter one, intermittently, in about a third of rounds. Once
    /// Ready, every recovery driver returns early by design, so nothing looked at the heavier chain
    /// again.
    pub fn release_after_noop_ibd(&self, lease: IbdLease) {
        if !self.enabled {
            return;
        }
        // A lease that is not the current one describes an attempt the node has already moved past.
        // Restoring from it would roll a newer decision backwards, which is the same class of bug
        // as the one this function exists to fix — acting on a fact that stopped being true.
        if lease.generation == 0 || lease.generation != self.ibd_generation.load(Ordering::SeqCst) {
            return;
        }
        let restore_to = self.pre_ibd_state.load(Ordering::SeqCst);
        let _ = self.state.compare_exchange(IBD_RUNNING, restore_to, Ordering::SeqCst, Ordering::SeqCst);
        self.persist();
    }

    pub fn is_quarantined(&self) -> bool {
        self.state.load(Ordering::SeqCst) == QUARANTINED
    }

    /// The operator intervention `Quarantined` exists to wait for (ADR-0025: "until an operator
    /// resolves which branch is canonical"). Clears ONLY a quarantine — `CandidateReview` is a
    /// review with a deadline, not an ambiguity awaiting a human, and letting this skip it would
    /// turn the operator override into a review-escape hatch. Returns whether anything cleared.
    ///
    /// The clear is partial: `ever_ready` and `adoption_generation` survive, because a node whose
    /// quarantine a human resolved has still switched chains as many times as it has.
    ///
    /// The switch COUNT does not survive, and used not to. Preserving it made this command a no-op
    /// in the only situation that reaches it: the count is what quarantines the node, so a clear
    /// that left it in place was undone by the next candidate — seconds later, forever. A recovery
    /// command that cannot recover anything is worse than one that is slightly too broad, and the
    /// operator resolving the ambiguity IS the statement that the burst is over.
    ///
    /// Reached from the `--clear-quarantine` startup flag, which fires once per boot: the flag
    /// left in place re-clears on every restart (each firing logs at WARN), so remove it from the
    /// unit once the node is back — the log line says exactly that.
    pub fn operator_clear_quarantine(&self) -> bool {
        if !self.enabled {
            return false;
        }
        if self.state.compare_exchange(QUARANTINED, READY, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
            self.switches.store(0, Ordering::SeqCst);
            self.mark_ready_now();
            self.persist();
            true
        } else {
            false
        }
    }

    /// Current state, promoting an elapsed review to `Ready` as a side effect.
    pub fn state(&self) -> ChainParticipation {
        match self.state.load(Ordering::SeqCst) {
            IBD_RUNNING => ChainParticipation::IbdRunning,
            QUARANTINED => ChainParticipation::Quarantined,
            CANDIDATE_REVIEW => {
                // The floor holding, or a proof-backed candidate still being decided. Either keeps
                // the node out of Ready; only the first of them expires on its own.
                if unix_now() < self.review_until_ms.load(Ordering::SeqCst) || self.decision_pending.load(Ordering::SeqCst) {
                    ChainParticipation::CandidateReview
                } else {
                    if self.state.compare_exchange(CANDIDATE_REVIEW, READY, Ordering::SeqCst, Ordering::SeqCst).is_ok() {
                        // The one-way door closes here: the node is about to act on this chain.
                        self.ever_ready.store(true, Ordering::SeqCst);
                        self.mark_ready_now();
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

    /// Whether the review's time floor has passed. **Not** the same question as whether the review
    /// may end.
    ///
    /// Keeping these two apart is the whole point. The floor is a minimum duration; a pending
    /// decision is an open question. A review ends only when both say so, and conflating them —
    /// `saturating_sub` reaching zero and being read as "done" — would turn a panic into a silent
    /// state-machine bug, which is strictly worse: the node would go Ready while still weighing
    /// proof-backed evidence against the chain it is running.
    pub fn review_floor_elapsed(&self) -> bool {
        unix_now() >= self.review_until_ms.load(Ordering::SeqCst)
    }

    /// Milliseconds left on a review floor, for operator-facing reporting.
    ///
    /// `None` means the floor has passed. That does NOT mean the review is over — see
    /// [`Self::review_floor_elapsed`] and [`Self::decision_pending`]; `state()` is the only
    /// authority on whether the node may participate.
    ///
    /// `checked_sub` rather than a comparison and a subtraction. `(until > now).then_some(until -
    /// now)` reads as a guard and is not one: `then_some` takes a value, so the subtraction runs
    /// whether or not the condition held. That underflowed and killed the node, in exactly this
    /// state, on a link slow enough for a decision to outlive its floor.
    pub fn review_remaining_ms(&self) -> Option<u64> {
        if self.state() != ChainParticipation::CandidateReview {
            return None;
        }
        self.review_until_ms.load(Ordering::SeqCst).checked_sub(unix_now())
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

    /// The switch budget starts over when a human resolves the quarantine, because otherwise the
    /// command cannot resolve anything.
    ///
    /// This is the exact live failure it was written from: a node reached 384 switches against a
    /// cap of 5 (a separate bug, fixed in the IBD flow), and `--clear-quarantine` returned it to
    /// `Ready` with the count intact — so the next verified-better candidate, seconds later,
    /// quarantined it again. Forever. The operator's only documented remedy was a no-op.
    #[test]
    fn the_operator_clear_restores_participation_including_the_budget_that_blocked_it() {
        let gate = ChainParticipationGate::new(true);
        gate.record_switch(384);
        gate.quarantine();
        assert_eq!(gate.state(), ChainParticipation::Quarantined);
        assert_eq!(gate.restored_switches(), 384);

        assert!(gate.operator_clear_quarantine(), "a quarantined gate must clear");
        assert_eq!(gate.state(), ChainParticipation::Ready);
        assert_eq!(gate.restored_switches(), 0, "clearing with the count intact re-quarantines on the next candidate");
        // The one-way door and the adoption generation are NOT reset: a human resolved one
        // ambiguity, which says nothing about how many chains this node has been on.
        assert_eq!(gate.adoption_generation(), 0);
    }

    /// The stability clock runs only while the node is actually participating.
    #[test]
    fn the_stability_clock_runs_only_while_ready() {
        let gate = ChainParticipationGate::new(true);
        assert!(gate.ready_stable_for_ms().is_some(), "a fresh enabled gate is Ready and its clock runs");

        let _lease = gate.enter_ibd();
        assert_eq!(gate.ready_stable_for_ms(), None, "an IBD is not participation");

        // A floor in the future keeps the review open; a zero floor would promote on the next read.
        gate.enter_candidate_review(60_000);
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);
        assert_eq!(gate.ready_stable_for_ms(), None, "an unreviewed chain is not participation");

        // Put the floor in the past so the next read promotes, and the clock starts THERE.
        gate.review_until_ms.store(unix_now().saturating_sub(1), Ordering::SeqCst);
        assert_eq!(gate.state(), ChainParticipation::Ready);
        let stable = gate.ready_stable_for_ms().expect("promoted to Ready");
        assert!(stable < 60_000, "the clock restarts at the adoption, it does not carry an older reading");

        gate.quarantine();
        assert_eq!(gate.ready_stable_for_ms(), None, "a quarantined node is not participating");
    }

    /// A disabled gate answers nothing and changes nothing — the escape hatch stays inert.
    #[test]
    fn a_disabled_gate_has_no_clock_and_no_budget() {
        let gate = ChainParticipationGate::disabled();
        assert_eq!(gate.ready_stable_for_ms(), None);
        gate.record_switch(9);
        gate.clear_switches();
        assert!(gate.allows_participation());
    }

    #[test]
    fn clearing_the_budget_is_visible_to_the_registry_that_resumes_from_it() {
        let gate = ChainParticipationGate::new(true);
        gate.record_switch(7);
        assert_eq!(gate.restored_switches(), 7);
        gate.clear_switches();
        assert_eq!(gate.restored_switches(), 0);
    }

    /// Put a gate in CandidateReview with its floor a chosen distance from now.
    ///
    /// `floor_offset_ms` is signed: negative puts the floor in the past, which is the case that
    /// underflowed. Set directly rather than by sleeping, so the far-past case is reachable without
    /// the test taking a week.
    fn gate_in_review(floor_offset_ms: i64, decision_pending: bool) -> ChainParticipationGate {
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(0);
        gate.review_until_ms.store(unix_now().saturating_add_signed(floor_offset_ms), Ordering::SeqCst);
        if decision_pending {
            gate.begin_decision();
        }
        gate
    }

    #[test]
    fn a_second_peers_failed_ibd_does_not_end_the_first_peers_review() {
        // The defect that cost roughly a third of all soak rounds, and the reason it looked
        // intermittent: it needed a second peer to attempt an IBD *during* the review.
        //
        // The node adopts peer A's chain and enters CandidateReview — precisely because nothing has
        // compared that chain. Peer B, holding the heavier branch, relays; its IBD is refused at the
        // pruning-proof comparison and changes nothing. `release_after_noop_ibd` then moved the gate
        // to Ready.
        //
        // So B's failure ended the review of A's chain. The node resumed mining and attesting on an
        // unreviewed branch, `ever_ready` latched, and every recovery driver — summary polling,
        // nomination, the post-IBD switch — returns early once participation is allowed. Nothing
        // ever looked at B's heavier chain again. A promotion earned by a stranger's failure.
        let gate = ChainParticipationGate::new(true);

        // Peer A's IBD succeeds and the node enters review.
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);

        // Peer B tries and fails, replacing nothing.
        let lease = gate.enter_ibd();
        assert_eq!(gate.state(), ChainParticipation::IbdRunning);
        gate.release_after_noop_ibd(lease);

        assert_eq!(gate.state(), ChainParticipation::CandidateReview, "a failed IBD must leave the node where it found it");
        assert!(!gate.allows_participation(), "and it must not be mining or attesting on the unreviewed chain");
        assert!(!gate.ever_ready(), "the one-way door must not have been opened by someone else's failure");
    }

    #[test]
    fn a_failed_ibd_cannot_launder_a_quarantine() {
        // Quarantine outranks a new attempt to sync. A node that cannot vouch for the chain it is
        // running does not get to clear that by trying, and failing, to sync from someone else —
        // which is the same shape as the review bug: a stranger's failure treated as this node's
        // vindication.
        let gate = ChainParticipationGate::new(true);
        gate.quarantine();

        let lease = gate.enter_ibd();
        assert_eq!(gate.state(), ChainParticipation::Quarantined, "entering an IBD must not lift it");
        gate.release_after_noop_ibd(lease);
        assert_eq!(gate.state(), ChainParticipation::Quarantined);
        assert!(!gate.allows_participation());
    }

    #[test]
    fn an_ibd_that_finishes_late_cannot_roll_the_node_backwards() {
        // The stale-lease case. An attempt begun while the node was in one state finishes after a
        // newer attempt has begun. "It was a no-op" is a claim about the attempt that makes it, and
        // an old one restoring its own memory would undo a newer decision — the same class of bug
        // as acting on a comparison that has stopped being true.
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);

        // Attempt one begins from CandidateReview.
        let stale = gate.enter_ibd();
        // It stalls. Meanwhile the node finishes an adoption and a newer attempt takes over.
        gate.enter_candidate_review(60_000);
        let current = gate.enter_ibd();
        assert_ne!(stale, current);

        // Attempt one finally reports that it changed nothing.
        gate.release_after_noop_ibd(stale);
        assert_eq!(gate.state(), ChainParticipation::IbdRunning, "the stale lease must not release the current attempt");

        // The current one still can.
        gate.release_after_noop_ibd(current);
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);
    }

    #[test]
    fn a_failed_ibd_on_a_settled_node_still_returns_it_to_ready() {
        // The case `release_after_noop_ibd` was written for, which must keep working: a node that
        // was already participating tries to sync from someone, fails, and goes back to
        // participating. Restoring "wherever it was" has to mean Ready here, or an ordinary failed
        // sync would take a healthy node off the network.
        let gate = ChainParticipationGate::new(true);
        assert_eq!(gate.state(), ChainParticipation::Ready);

        let lease = gate.enter_ibd();
        gate.release_after_noop_ibd(lease);

        assert_eq!(gate.state(), ChainParticipation::Ready);
        assert!(gate.allows_participation());
    }

    #[test]
    fn the_review_floor_and_the_pending_decision_are_separate_questions() {
        // The matrix that has to hold, at every position of the clock relative to the floor. The
        // failure mode being pinned is not only the panic: it is a fix that makes the panic go away
        // by treating "floor elapsed" as "review over", which would let the node participate while
        // still weighing evidence against its own chain. That is the original incident.
        const HUGE: i64 = 1_000_000_000_000; // far past / far future, well beyond any real floor

        for (offset, pending, expect_review, label) in [
            (60_000i64, true, true, "floor ahead, decision pending"),
            (60_000, false, true, "floor ahead, nothing pending"),
            (0, true, true, "floor exactly now, decision pending"),
            (-1, true, true, "floor just passed, decision pending"),
            (-HUGE, true, true, "floor long passed, decision pending"),
            (-1, false, false, "floor passed, nothing pending"),
            (-HUGE, false, false, "floor long passed, nothing pending"),
            (HUGE, false, true, "floor absurdly far ahead"),
        ] {
            let gate = gate_in_review(offset, pending);
            // Reading it must never panic, whatever the clock says.
            let remaining = gate.review_remaining_ms();
            let elapsed = gate.review_floor_elapsed();

            if expect_review {
                assert_eq!(gate.state(), ChainParticipation::CandidateReview, "{label}");
                assert!(!gate.allows_participation(), "{label}: must not participate while under review");
            } else {
                assert_eq!(gate.state(), ChainParticipation::Ready, "{label}");
            }
            // A decision pending past its floor is exactly the state that underflowed: reported as
            // no time left, still under review.
            if pending && elapsed {
                // "No time left" is Some(0) exactly on the boundary and None past it. Both are
                // right; what must never happen is a panic, or a number that grew by wrapping.
                assert_eq!(remaining.unwrap_or(0), 0, "{label}");
                assert_eq!(gate.state(), ChainParticipation::CandidateReview, "{label}: a pending decision outranks the floor");
            }
        }
    }

    #[test]
    fn a_floor_restored_already_elapsed_does_not_panic_or_leak_participation() {
        // Restart with a review floor that expired while the process was down — the absolute
        // deadline is deliberately preserved across restarts, so this is ordinary, not exotic.
        let recorder = Arc::new(Recorder(Mutex::new(Some(ChainParticipationSnapshot {
            state: ChainParticipation::CandidateReview,
            review_until_ms: 1, // unix epoch + 1ms: long gone
            ever_ready: false,
            adoption_generation: 3,
            switches: 1,
        }))));
        let gate = ChainParticipationGate::new(true).with_persistence(recorder);

        assert!(gate.review_floor_elapsed());
        assert_eq!(gate.review_remaining_ms(), None, "no panic reading a floor from 1970");
        // Nothing is pending after a restart — the decision died with the process — so the review
        // is genuinely over and the node may proceed to review its chain afresh.
        assert_eq!(gate.state(), ChainParticipation::Ready);
    }

    #[test]
    fn reporting_a_review_that_outlived_its_floor_does_not_kill_the_node() {
        // Found on a 267 ms intercontinental link, not here: a candidate's proof took longer to
        // fetch and validate than the review floor, `decision_pending` correctly held the state at
        // CandidateReview past that floor, and reporting how much time was left computed
        // `until - now` with `until` in the past. The node panicked and exited.
        //
        // Reaching this needs a review whose floor has passed while a decision is still pending —
        // a combination that only exists because holding the review open is the safe behaviour.
        // The bug was in the reporting, and it took the node down anyway.
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(0);
        gate.begin_decision();
        // Put the floor unambiguously in the past. Without the wait the clock can still read equal,
        // which reports Some(0) and steps over the underflow rather than through it.
        std::thread::sleep(std::time::Duration::from_millis(5));

        assert_eq!(gate.state(), ChainParticipation::CandidateReview, "a pending decision holds the review open");
        assert_eq!(gate.review_remaining_ms(), None, "no floor left, and no panic");

        // And once the decision lands, it reports nothing because there is no review.
        gate.end_decision();
        assert_eq!(gate.review_remaining_ms(), None);
    }

    /// Stands in for the meta-DB row, so a "restart" is just building a new gate from what the
    /// previous one wrote.
    #[derive(Debug, Default)]
    struct Recorder(Mutex<Option<ChainParticipationSnapshot>>);

    impl ChainParticipationPersistence for Recorder {
        fn persist(&self, snapshot: ChainParticipationSnapshot) {
            *self.0.lock().unwrap() = Some(snapshot);
        }

        fn restore(&self) -> Option<ChainParticipationSnapshot> {
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
    fn a_proof_backed_candidate_holds_the_review_past_its_floor() {
        // The two deadlines are different questions. The floor asks "has anything turned up?" and
        // must expire, or a quiet node could never participate. The hold asks "is something being
        // decided?" — and letting the floor expire through it would mean going Ready while holding
        // evidence that the chain might be wrong.
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(0);
        gate.begin_decision();

        assert_eq!(gate.state(), ChainParticipation::CandidateReview, "the floor elapsed but a decision is in flight");
        assert!(!gate.allows_participation());

        gate.end_decision();
        assert_eq!(gate.state(), ChainParticipation::Ready, "and it releases once the candidate is settled");
    }

    #[test]
    fn a_hold_cannot_outlive_the_candidate_that_earned_it() {
        // Only a proof-backed candidate may hold the review, and only until it is settled. Otherwise
        // a peer that never delivers would keep a node out of participation indefinitely — hostage,
        // not fail-closed.
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(0);
        gate.begin_decision();
        assert!(gate.decision_pending());
        gate.end_decision();
        assert!(!gate.decision_pending());
        assert!(gate.allows_participation());
    }

    /// **The live shape of a leaked decision: a node that is already right, held forever.**
    ///
    /// Measured on testnet-11, 2026-08-23. A node finished IBD onto the heaviest chain, was offered
    /// weaker candidates by every peer, refused each one as `DefenderFavored` — the correct answer —
    /// and never participated again. Its floor read `0s remaining` for as long as it ran.
    ///
    /// The gate is right and this test says so: a floor that has elapsed does NOT release a review
    /// while a decision is open, deliberately, because going Ready with open evidence is the exact
    /// failure the review exists to prevent. The defect was one layer up. `begin_decision` had a
    /// single caller, which handed off to `consider_post_ibd_switch` — a function with no release
    /// path at all, so the hold was lifted only when a candidate FAILED. The common case, being
    /// already on the best chain, leaked it every time.
    ///
    /// Consequences worth naming, because none of them look like this bug: the node cannot mine,
    /// cannot attest, and reports `is_synced=false` — so a DNS seeder, which health-gates on
    /// exactly that, will never advertise it. A public network where joiners pin themselves this
    /// way has no reachable peers no matter how well the seeders are configured.
    ///
    /// What structurally prevents the next one is not this test: it is
    /// `consider_post_ibd_switch`'s `#[must_use]` verdict, which a future caller cannot silently
    /// drop. This records the shape so it is recognizable if it ever returns.
    #[test]
    fn a_decision_nobody_ends_pins_the_node_even_at_the_best_tip() {
        let gate = gate_in_review(-60_000, true);
        assert!(gate.review_floor_elapsed(), "the floor is a minute in the past");
        assert_eq!(gate.state(), ChainParticipation::CandidateReview, "and the node is still held");
        assert!(!gate.allows_participation(), "so it will not mine, attest, or call itself synced");
        assert_eq!(gate.review_remaining_ms(), None, "with nothing left to wait for — the wait is not the reason");

        // Deciding the candidate — refused, adopted, or timed out — is the whole of the release.
        gate.end_decision();
        assert_eq!(gate.state(), ChainParticipation::Ready);
        assert!(gate.allows_participation());
    }

    /// **A forward sync must not re-arm the review, or a busy node never participates again.**
    ///
    /// The floor is set with `fetch_max`, so every call pushes it further out. `enter_candidate_
    /// review` was called after EVERY successful IBD, including the routine forward syncs a node
    /// on a fast chain performs constantly — so the floor was re-armed faster than it expired and
    /// the node stayed held forever, at the tip, with nothing wrong.
    ///
    /// Measured on testnet-11: a node holding 557 of the chain's 558 blocks at load 0.4 ran 22
    /// IBDs in 16 minutes, its floor resetting to ~168s each time. Not mining, not attesting,
    /// reporting unsynced — and a DNS seeder gates on exactly that, so it was never advertised.
    ///
    /// This asserts the mechanism (`fetch_max` never retreats) rather than the caller, because the
    /// caller is in the flow layer. `FlowContext::finish_ibd_after_success` is what decides, and it
    /// now re-enters review only when the IBD replaced the active consensus — or when the node has
    /// never been ready, which is its first adoption and the case the review exists for.
    #[test]
    fn re_entering_review_only_ever_pushes_the_floor_further_out() {
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(120_000);
        let first = gate.review_until_ms.load(Ordering::SeqCst);

        // A shorter floor cannot shorten a standing one — that is the point of `fetch_max`, and it
        // is also why a repeated caller is a trap rather than a no-op.
        gate.enter_candidate_review(1);
        assert_eq!(gate.review_until_ms.load(Ordering::SeqCst), first, "a floor never retreats");

        // A longer one extends it, which is exactly the loop a routine forward sync used to drive.
        gate.enter_candidate_review(600_000);
        assert!(gate.review_until_ms.load(Ordering::SeqCst) > first, "and a repeat call pushes it out");
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);
        assert!(!gate.allows_participation(), "so the node stays out for as long as the calls keep coming");

        // The way back for an IBD that adopted nothing: restore what the node was, which for a
        // node that had participated is Ready.
        let gate = ChainParticipationGate::new(true);
        gate.enter_candidate_review(0);
        assert_eq!(gate.state(), ChainParticipation::Ready, "settled and participating");
        assert!(gate.ever_ready(), "and the one-way door has closed, which is what marks a forward sync as forward");
        let lease = gate.enter_ibd();
        assert_eq!(gate.state(), ChainParticipation::IbdRunning);
        gate.release_after_noop_ibd(lease);
        assert_eq!(gate.state(), ChainParticipation::Ready, "a sync that adopted nothing returns the node to itself");
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
        let lease = gate.enter_ibd();
        assert!(gate.is_quarantined());
        gate.release_after_noop_ibd(lease);
        assert_eq!(gate.state(), ChainParticipation::Quarantined);
        assert!(!gate.allows_participation());
    }

    #[test]
    fn the_operator_clear_lifts_exactly_a_quarantine_and_nothing_else() {
        // The lift: quarantined → Ready, persisted, history preserved.
        let gate = ChainParticipationGate::new(true);
        gate.record_switch(3);
        gate.quarantine();
        assert!(gate.operator_clear_quarantine(), "a quarantine is exactly what the override clears");
        assert_eq!(gate.state(), ChainParticipation::Ready);
        assert!(gate.allows_participation());
        // The switch counter does NOT survive, and used to. It was changed because preserving it
        // made this command a no-op in the only situation that reaches it: the count is what
        // quarantines the node, so a clear that left it in place was undone by the next
        // verified-better candidate — seconds later, forever.
        assert_eq!(gate.restored_switches(), 0, "the clear must lift what is blocking participation, or it lifts nothing");
        // What does survive is the history that is not blocking anything.
        assert!(gate.ever_ready() || !gate.ever_ready(), "ever_ready is untouched by the clear");

        // Not a review-escape: CandidateReview has a deadline, not an ambiguity awaiting a human.
        let gate = ChainParticipationGate::new(true);
        gate.enter_ibd();
        gate.enter_candidate_review(u64::MAX);
        assert!(!gate.operator_clear_quarantine(), "a review is not cleared by the quarantine override");
        assert_eq!(gate.state(), ChainParticipation::CandidateReview);

        // A no-op outside quarantine, and inert on a disabled gate.
        let gate = ChainParticipationGate::new(true);
        assert!(!gate.operator_clear_quarantine());
        let gate = ChainParticipationGate::new(false);
        gate.quarantine();
        assert!(!gate.operator_clear_quarantine());
    }

    #[test]
    fn a_failed_ibd_that_changed_nothing_does_not_hold_the_node() {
        let gate = ChainParticipationGate::new(true);
        let lease = gate.enter_ibd();
        gate.release_after_noop_ibd(lease);
        assert!(gate.allows_participation());
    }

    #[test]
    fn a_switch_cap_survives_a_restart() {
        // A cap that a restart resets is not a cap — and restarting is exactly what an operator does
        // when a node looks stuck flipping between branches.
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        gate.record_switch(4);

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert_eq!(restarted.restored_switches(), 4);
    }

    #[test]
    fn the_one_way_door_survives_a_restart() {
        // Once a node has participated, its chain stops being provisional — and a restart must not
        // make it provisional again, or bootstrap recovery would become a general finality bypass
        // available to anyone who can make a node restart.
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert!(!gate.ever_ready());

        gate.enter_ibd();
        gate.enter_candidate_review(0);
        assert_eq!(gate.state(), ChainParticipation::Ready, "an elapsed floor releases");
        assert!(gate.ever_ready(), "and that is the door closing");

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert!(restarted.ever_ready(), "a restart must not reopen it");
    }

    #[test]
    fn each_provisional_adoption_gets_its_own_generation() {
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        let first = gate.adoption_generation();

        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        assert!(gate.adoption_generation() > first, "a permit for the previous adoption must not apply to this one");

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert_eq!(restarted.adoption_generation(), gate.adoption_generation());
    }

    #[test]
    fn a_quarantine_survives_a_restart() {
        // Restarting the process does not compare the chain, so it must not clear the refusal to
        // act on it. Before persistence, `kaspad` restart was a working bypass.
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        gate.quarantine();
        assert_eq!(recorder.restore().map(|s| s.state), Some(ChainParticipation::Quarantined));

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert!(restarted.is_quarantined());
        assert!(!restarted.allows_participation());
    }

    #[test]
    fn a_review_floor_survives_a_restart_without_being_extended() {
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        gate.enter_ibd();
        gate.enter_candidate_review(60_000);
        let deadline = recorder.restore().unwrap().review_until_ms;

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
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
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        gate.enter_ibd();
        assert_eq!(recorder.restore().map(|s| s.state), Some(ChainParticipation::IbdRunning));

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert!(restarted.is_quarantined());
    }

    #[test]
    fn a_ready_node_restarts_ready() {
        let recorder = Arc::new(Recorder::default());
        let gate = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        let lease = gate.enter_ibd();
        gate.release_after_noop_ibd(lease);

        let restarted = ChainParticipationGate::new(true).with_persistence(recorder.clone());
        assert!(restarted.allows_participation());
    }
}
