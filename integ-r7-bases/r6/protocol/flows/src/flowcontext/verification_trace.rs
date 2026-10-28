//! Why a challenger verification did not happen, recorded cheaply enough not to change the answer.
//!
//! Three diagnoses of one intermittently failing soak round were wrong, all reached the same way:
//! the trace showed nominations rising and proof requests not, and the gap between them had to be
//! guessed. The step that declines was silent.
//!
//! The obvious repair — record it through `recovery_trace` — is the wrong one here. That path does
//! a `format!`, a synchronous `info!`, and a `Vec::remove(0)` on a full buffer, per event, on the
//! path being measured. The failure is intermittent and concurrency-shaped; instrumentation that
//! heavy is how a race becomes unreproducible and gets declared fixed. So this is deliberately
//! separate and deliberately cheap:
//!
//! - a `u64` atomic increment per reason, always;
//! - a bounded ring of recent skips behind a short lock, `push_back`/`pop_front`, no formatting;
//! - **no logging at all** — the ring is dumped only when a round has already failed.
//!
//! Nothing here allocates on the recording path except the ring's fixed backing store.

use std::{
    collections::VecDeque,
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

use kaspa_p2p_lib::PeerKey;

use super::ibd_candidates::CandidateId;

/// Every way `verify_challenger` can decline to send a proof request.
///
/// Distinct variants rather than one "skipped" because they call for different fixes: a peer that
/// is not the designated prover is a routing question, a candidate in the wrong state is an
/// ordering question, and nobody being eligible is a policy question. Collapsing them is how the
/// last three diagnoses went wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SkipReason {
    /// Another source is the designated prover for this candidate.
    NotDesignatedProver,
    /// No source is eligible — every peer offering it has stopped answering.
    NoEligibleProver,
    /// The candidate is gone from the registry, most likely deleted when its last source dropped.
    CandidateNotFound,
    /// The candidate is in a state that does not want a proof (validated, or already refused).
    CandidateStateChanged,
    /// This flow's peer is no longer among the candidate's sources.
    PeerNoLongerSource,
    /// Participation resumed, so nothing here applies any more.
    ParticipationBecameReady,
    /// The proof request could not be sent — the peer's route is gone.
    RequestChannelUnavailable,
    /// The lease had too little left to be worth starting a request inside.
    LeaseTooShortToStart,
    /// A proof arrived, but for an attempt this node had already written off. Applying it would
    /// credit the current attempt with an answer from the peer that was judged too slow.
    StaleProofResponse,
}

impl SkipReason {
    pub const ALL: [SkipReason; 9] = [
        SkipReason::NotDesignatedProver,
        SkipReason::NoEligibleProver,
        SkipReason::CandidateNotFound,
        SkipReason::CandidateStateChanged,
        SkipReason::PeerNoLongerSource,
        SkipReason::ParticipationBecameReady,
        SkipReason::RequestChannelUnavailable,
        SkipReason::LeaseTooShortToStart,
        SkipReason::StaleProofResponse,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SkipReason::NotDesignatedProver => "NotDesignatedProver",
            SkipReason::NoEligibleProver => "NoEligibleProver",
            SkipReason::CandidateNotFound => "CandidateNotFound",
            SkipReason::CandidateStateChanged => "CandidateStateChanged",
            SkipReason::PeerNoLongerSource => "PeerNoLongerSource",
            SkipReason::ParticipationBecameReady => "ParticipationBecameReady",
            SkipReason::RequestChannelUnavailable => "RequestChannelUnavailable",
            SkipReason::LeaseTooShortToStart => "LeaseTooShortToStart",
            SkipReason::StaleProofResponse => "StaleProofResponse",
        }
    }

    fn index(&self) -> usize {
        Self::ALL.iter().position(|r| r == self).expect("every reason is in ALL")
    }
}

/// One declined verification, with enough context to tell a routing bug from an ordering bug.
#[derive(Clone, Debug)]
pub struct VerificationSkip {
    pub reason: SkipReason,
    pub candidate_id: CandidateId,
    /// Which connection this flow belongs to. A `PeerKey` is the same across a reconnect, so
    /// without this a stale flow's decision is indistinguishable from the current one's — and a
    /// peer that reconnects every thirty seconds is exactly the case being diagnosed.
    pub connection_generation: u64,
    pub executing_peer: PeerKey,
    pub designated_peer: Option<PeerKey>,
    /// The candidate's validation state, as a short static name. Static rather than formatted so
    /// recording stays allocation-free.
    pub candidate_state: &'static str,
    pub live_sources: usize,
    pub participation_state: &'static str,
}

/// Ring capacity. Large enough to hold a whole failing round's skips — the observed rate is a few
/// per second — and small enough that the buffer is not itself a memory concern.
const RING: usize = 512;

fn counters() -> &'static [AtomicU64; SkipReason::ALL.len()] {
    static COUNTERS: OnceLock<[AtomicU64; SkipReason::ALL.len()]> = OnceLock::new();
    COUNTERS.get_or_init(|| std::array::from_fn(|_| AtomicU64::new(0)))
}

fn ring() -> &'static Mutex<VecDeque<VerificationSkip>> {
    static RING_BUF: OnceLock<Mutex<VecDeque<VerificationSkip>>> = OnceLock::new();
    RING_BUF.get_or_init(|| Mutex::new(VecDeque::with_capacity(RING)))
}

/// Record a declined verification. Cheap by construction; never logs, never panics, never waits.
///
/// The counter is the fact and is always recorded. The ring is detail, and it is recorded only if
/// the lock happens to be free: `try_lock`, dropping the event on contention.
///
/// That ordering is deliberate. A diagnostic that blocks the path it measures changes the
/// interleaving it exists to observe — and the bug being observed here is a race that reproduces in
/// about one round in three. A missing detail line is a cost; instrumentation that makes the race
/// disappear and get declared fixed is a much larger one.
pub fn record_skip(skip: VerificationSkip) {
    counters()[skip.reason.index()].fetch_add(1, Ordering::Relaxed);
    if let Ok(mut ring) = ring().try_lock() {
        if ring.len() == RING {
            ring.pop_front();
        }
        ring.push_back(skip);
    }
}

/// A new connection's generation. Monotonic across the process, so no two flows share one.
pub fn next_connection_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub fn clear() {
    for c in counters() {
        c.store(0, Ordering::Relaxed);
    }
    if let Ok(mut ring) = ring().lock() {
        ring.clear();
    }
}

pub fn counts() -> Vec<(SkipReason, u64)> {
    SkipReason::ALL.iter().map(|r| (*r, counters()[r.index()].load(Ordering::Relaxed))).collect()
}

/// Everything worth reading after a round has already failed. Formatting happens here, not on the
/// path being measured.
pub fn dump() -> String {
    let mut out = String::from("verification skips by reason:\n");
    for (reason, n) in counts() {
        if n > 0 {
            out.push_str(&format!("  {:<28} {}\n", reason.as_str(), n));
        }
    }
    let recent: Vec<_> = ring().lock().map(|r| r.iter().cloned().collect()).unwrap_or_default();
    if recent.is_empty() {
        out.push_str("  (none recorded)\n");
        return out;
    }
    out.push_str(&format!("last {} skips:\n", recent.len().min(20)));
    for s in recent.iter().rev().take(20) {
        out.push_str(&format!(
            "  {:<24} cand={} conn={} by={} designated={:?} state={} sources={} participation={}\n",
            s.reason.as_str(),
            s.candidate_id.virtual_selected_parent,
            s.connection_generation,
            s.executing_peer,
            s.designated_peer,
            s.candidate_state,
            s.live_sources,
            s.participation_state,
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::BlockHash;
    use kaspa_utils::networking::{IpAddress, PeerId};
    use std::net::IpAddr;
    use uuid::Uuid;

    /// The ring and counters are process-global, and the test runner is parallel, so any two
    /// tests that `clear()` and then assert on contents race each other — one's entry is the
    /// other's eviction. Serialized here rather than fixed in the trace itself: production only
    /// appends, and a lock on the recording path would be paying for a problem only tests have.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: Mutex<()> = Mutex::new(());
        LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn peer(n: u8) -> PeerKey {
        PeerKey::new(PeerId::new(Uuid::from_u128(n as u128)), IpAddress::new(IpAddr::from([10, 0, 0, n])))
    }

    fn skip(reason: SkipReason) -> VerificationSkip {
        VerificationSkip {
            reason,
            candidate_id: CandidateId {
                pruning_point: BlockHash::from_u64_word(1),
                virtual_selected_parent: BlockHash::from_u64_word(2),
            },
            connection_generation: 7,
            executing_peer: peer(1),
            designated_peer: Some(peer(2)),
            candidate_state: "SummaryReceived",
            live_sources: 1,
            participation_state: "candidate-review",
        }
    }

    #[test]
    fn the_ring_is_bounded_and_keeps_the_most_recent() {
        let _serial = serial();
        clear();
        for _ in 0..RING + 50 {
            record_skip(skip(SkipReason::NotDesignatedProver));
        }
        assert_eq!(ring().lock().unwrap().len(), RING, "a diagnostic must not grow without bound");
        // Counters are not bounded by the ring: the count is the fact, the ring is the detail.
        assert_eq!(counts().iter().find(|(r, _)| *r == SkipReason::NotDesignatedProver).unwrap().1, (RING + 50) as u64);
    }

    #[test]
    fn connection_generations_never_repeat() {
        let a = next_connection_generation();
        let b = next_connection_generation();
        assert_ne!(a, b, "two flows sharing a generation is the ambiguity this exists to remove");
    }

    #[test]
    fn a_dump_names_the_reason_and_the_connection() {
        let _serial = serial();
        clear();
        record_skip(skip(SkipReason::NoEligibleProver));
        let out = dump();
        assert!(out.contains("NoEligibleProver"), "{out}");
        assert!(out.contains("conn=7"), "{out}");
    }
}
