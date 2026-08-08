//! Where the chain-recovery sequence actually got to.
//!
//! The two-history scenario fails without reaching a permit request, and grepping log strings could
//! not say which link broke — a stage that never runs prints nothing, and so does a stage that runs
//! and is filtered out. Absence of a log line is not evidence of anything.
//!
//! So the sequence records itself. Every step from "a peer mentioned a chain" to "that chain was
//! committed" emits an event, and a failing test can report the last stage reached next to the one
//! it expected. One run then names the break instead of narrowing it.
//!
//! This is diagnosis, not policy: nothing here decides anything. It is kept in the product rather
//! than bolted onto the test because the same question — how far did recovery get, and why did it
//! stop — is the first one an operator will ask of a node stuck between branches.

use std::sync::{
    Mutex, OnceLock,
    atomic::{AtomicU64, Ordering},
};

use kaspa_consensus_core::BlueWorkType;
use kaspa_core::{debug, info};

use super::ibd_candidates::CandidateId;

/// Bounded so a long-running node cannot accumulate these without limit. Oldest are dropped: a
/// stuck node's *recent* attempts are what explain its current state.
const MAX_TRACE_EVENTS: usize = 4096;

/// Ties the events of one recovery attempt together, so concurrent attempts against different peers
/// can be told apart after the fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecoveryAttemptId(pub u64);

impl RecoveryAttemptId {
    pub fn next() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// The sequence, in the order it has to happen. Declared in order so a test can say "reached X,
/// expected Y" by comparing positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RecoveryStage {
    CandidateObserved,
    SummaryRequested,
    SummaryReceived,
    CandidateNominated,
    ProofRequestSent,
    ProofReceived,
    ProofValidated,
    CandidateCompared,
    PreferredCandidateReserved,
    HandoffReceived,
    IbdStartedForPreferredCandidate,
    FinalityConflictDetected,
    RecoveryPermitRequested,
    RecoveryPermitGranted,
    CandidateCommitted,
    ParticipationReady,
    /// A stage refused. Carries why in `detail`, which is the field a diagnosis usually turns on.
    Rejected,
}

impl RecoveryStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CandidateObserved => "CandidateObserved",
            Self::SummaryRequested => "SummaryRequested",
            Self::SummaryReceived => "SummaryReceived",
            Self::CandidateNominated => "CandidateNominated",
            Self::ProofRequestSent => "ProofRequestSent",
            Self::ProofReceived => "ProofReceived",
            Self::ProofValidated => "ProofValidated",
            Self::CandidateCompared => "CandidateCompared",
            Self::PreferredCandidateReserved => "PreferredCandidateReserved",
            Self::HandoffReceived => "HandoffReceived",
            Self::IbdStartedForPreferredCandidate => "IbdStartedForPreferredCandidate",
            Self::FinalityConflictDetected => "FinalityConflictDetected",
            Self::RecoveryPermitRequested => "RecoveryPermitRequested",
            Self::RecoveryPermitGranted => "RecoveryPermitGranted",
            Self::CandidateCommitted => "CandidateCommitted",
            Self::ParticipationReady => "ParticipationReady",
            Self::Rejected => "Rejected",
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecoveryTraceEvent {
    pub attempt_id: Option<RecoveryAttemptId>,
    pub stage: RecoveryStage,
    pub candidate_id: Option<CandidateId>,
    pub peer: Option<String>,
    pub participation_state: &'static str,
    /// Free-form: comparison results, reject reasons, work figures — whatever makes the line
    /// answer "why" rather than only "what".
    pub detail: String,
}

fn trace() -> &'static Mutex<Vec<RecoveryTraceEvent>> {
    static TRACE: OnceLock<Mutex<Vec<RecoveryTraceEvent>>> = OnceLock::new();
    TRACE.get_or_init(|| Mutex::new(Vec::new()))
}

/// Record a step. Never panics and never blocks on anything but its own short lock: a diagnostic
/// that can take the node down is worse than no diagnostic.
pub fn record(event: RecoveryTraceEvent) {
    // Visible in the node's own log too, since an operator reading logs is the other consumer.
    // `Rejected` is the interesting one, so it is louder.
    let line = format!(
        "recovery-trace stage={} attempt={:?} candidate={:?} peer={:?} participation={} {}",
        event.stage.as_str(),
        event.attempt_id.map(|a| a.0),
        event.candidate_id.map(|c| c.virtual_selected_parent),
        event.peer,
        event.participation_state,
        event.detail
    );
    if event.stage == RecoveryStage::Rejected {
        info!("{line}");
    } else {
        debug!("{line}");
    }

    if let Ok(mut events) = trace().lock() {
        if events.len() >= MAX_TRACE_EVENTS {
            events.remove(0);
        }
        events.push(event);
    }
}

/// Convenience for the common shape.
pub fn record_stage(
    stage: RecoveryStage,
    attempt_id: Option<RecoveryAttemptId>,
    candidate_id: Option<CandidateId>,
    peer: Option<String>,
    participation_state: &'static str,
    detail: impl Into<String>,
) {
    record(RecoveryTraceEvent { attempt_id, stage, candidate_id, peer, participation_state, detail: detail.into() });
}

/// Everything recorded so far.
pub fn snapshot() -> Vec<RecoveryTraceEvent> {
    trace().lock().map(|e| e.clone()).unwrap_or_default()
}

pub fn clear() {
    if let Ok(mut events) = trace().lock() {
        events.clear();
    }
}

/// The furthest stage reached, ignoring rejections.
///
/// "Furthest" rather than "last" because attempts interleave: a later attempt failing early must
/// not make it look as though the sequence regressed.
pub fn furthest_stage() -> Option<RecoveryStage> {
    snapshot().iter().map(|e| e.stage).filter(|s| *s != RecoveryStage::Rejected).max()
}

/// Whether a stage was ever reached.
pub fn reached(stage: RecoveryStage) -> bool {
    snapshot().iter().any(|e| e.stage == stage)
}

pub fn count(stage: RecoveryStage) -> usize {
    snapshot().iter().filter(|e| e.stage == stage).count()
}

/// A report for a failing test: how far the sequence got, what should have come next, and every
/// refusal along the way. This is the output that turns "it did not work" into one hypothesis.
pub fn diagnosis(expected: RecoveryStage) -> String {
    let events = snapshot();
    let furthest = furthest_stage();
    let mut out = format!("recovery sequence reached {:?}, expected to reach {}\n", furthest.map(|s| s.as_str()), expected.as_str());
    out.push_str("stage counts:\n");
    for stage in [
        RecoveryStage::CandidateObserved,
        RecoveryStage::SummaryRequested,
        RecoveryStage::SummaryReceived,
        RecoveryStage::CandidateNominated,
        RecoveryStage::ProofRequestSent,
        RecoveryStage::ProofReceived,
        RecoveryStage::ProofValidated,
        RecoveryStage::CandidateCompared,
        RecoveryStage::PreferredCandidateReserved,
        RecoveryStage::HandoffReceived,
        RecoveryStage::IbdStartedForPreferredCandidate,
        RecoveryStage::FinalityConflictDetected,
        RecoveryStage::RecoveryPermitRequested,
        RecoveryStage::RecoveryPermitGranted,
        RecoveryStage::CandidateCommitted,
        RecoveryStage::ParticipationReady,
    ] {
        out.push_str(&format!("  {:<32} {}\n", stage.as_str(), count(stage)));
    }
    out.push_str("rejections:\n");
    for e in events.iter().filter(|e| e.stage == RecoveryStage::Rejected) {
        out.push_str(&format!("  candidate={:?} peer={:?} {}\n", e.candidate_id.map(|c| c.virtual_selected_parent), e.peer, e.detail));
    }
    out
}

/// Formats a work comparison the way a diagnosis needs to read it.
pub fn describe_comparison(challenger: BlueWorkType, defender: BlueWorkType) -> String {
    let verdict = if challenger > defender {
        "ChallengerStrictlySuperior"
    } else if challenger == defender {
        "Equal"
    } else {
        "DefenderFavored"
    };
    format!("challenger_work={challenger} defender_work={defender} comparison={verdict}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stages_are_ordered_so_progress_can_be_compared() {
        // `furthest_stage` relies on this ordering; if the enum is reordered, a test would silently
        // start reporting the wrong break point.
        assert!(RecoveryStage::CandidateObserved < RecoveryStage::SummaryReceived);
        assert!(RecoveryStage::SummaryReceived < RecoveryStage::ProofValidated);
        assert!(RecoveryStage::ProofValidated < RecoveryStage::PreferredCandidateReserved);
        assert!(RecoveryStage::PreferredCandidateReserved < RecoveryStage::RecoveryPermitRequested);
        assert!(RecoveryStage::RecoveryPermitRequested < RecoveryStage::CandidateCommitted);
    }

    #[test]
    fn attempt_ids_are_distinct() {
        assert_ne!(RecoveryAttemptId::next(), RecoveryAttemptId::next());
    }
}
