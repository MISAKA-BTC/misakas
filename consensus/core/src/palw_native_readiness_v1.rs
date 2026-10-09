//! **RFC-0012 D1 — why `safe` stands where it does.** A pure explanation of the native-settlement certificate, built from the
//! SAME inputs the snapshot is: the executed chain's effects, the facts, the sink's open claims and data-availability sessions.
//!
//! The snapshot says *what* is safe and gives one stop reason. A reader waiting on a transaction needs more: *which* effect holds
//! `safe` back, *every* condition it still lacks (not only the first), how long each time-driven one has to run, which claim or
//! court session is open, and whether the history the certificate needs is present at all. This module answers that and nothing
//! else: it **decides nothing**, no consensus path reads it, and it is never persisted — it is recomputed from the sink on demand
//! (`getPalwSettlement`, the RFC-0012 RPC; no new op).
//!
//! **Honesty rules.**
//! * The first wait that maps to a stop reason is, for the blocking effect, exactly the snapshot's `stop`
//!   ([`SafeWaitV1::stop`]); a test holds it against [`certify_native_prefix_v1`].
//! * `earliest_ready_in_daa` is an estimate *from the facts already on the chain* and is `None` the moment any unmet condition
//!   needs something that has not happened yet (a claim still working through its panel/receipt/court clocks, an open DA session,
//!   a frontier that has not advanced, work that is not there). It never promises that `safe` WILL advance.
//! * Evidence the node saw and did not count is reported as [`SkippedEvidenceV1`], not hidden.

use crate::Hash64;
use crate::palw_native_settlement_v1::{
    EvidenceAccumulatorV1, MatureUsefulWorkV1, NativeEffectV1, NativePrefixV1, PalwSettlementPolicyV1, SettlementStopV1,
    SkippedEvidenceV1,
};
use crate::palw_state_v2::PalwClaimPhaseV2;
use serde::{Deserialize, Serialize};

pub const NATIVE_READINESS_VERSION_V1: u16 = 1;
/// At most this many open claims and sessions are NAMED per effect; the totals are exact.
pub const NATIVE_READINESS_LISTED_V1: usize = 8;

/// Why the chain walk could not give a certificate its history (`MissingHistory`). Names the cause; never a guess.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HistoryGapV1 {
    /// A chain block's header could not be read.
    HeaderUnreadable,
    /// A chain block's selected parent could not be read.
    ParentUnreadable,
    /// A chain block's EVM execution row exists but could not be read.
    ExecutionRowUnreadable,
    /// The stored execution result disagrees with the header's commitment.
    ExecutionDisagreesWithHeader,
    /// Execution has a hole inside a supposedly connected chain.
    ExecutionGap,
    /// A chain block's PALW delta is missing or undecodable: the retention gap. The evidence it carried cannot be read.
    DeltaNotRetained,
    /// The roots chain (each block's delta root against its child's parent-state commitment) does not link.
    RootChainBroken,
    /// This node cannot materialize the PALW state at the sink.
    StateUnavailable,
    /// PALW state parameters are absent on this node.
    ParamsAbsent,
    /// The persisted snapshot is corrupt or from another ruleset: preserved for resync, never silently forgotten.
    PersistedSnapshotIncompatible,
    /// A reachability lookup failed.
    ReachabilityUnreadable,
    /// The walk ran off the start of the DAG without meeting the pruning point or genesis.
    ChainOpenEnded,
}

/// The phase an unresolved claim is in. `Voided` is never open; it exists so the mapping is total.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaimStageV1 {
    Provisional,
    PanelBound,
    ReceiptLicensed,
    /// `Final`, trace retention not yet lapsed.
    Final,
    /// A data-availability accusation is open on the claim (the pre-R-core phase).
    DefaultDisputed,
    Voided,
}

impl ClaimStageV1 {
    pub fn of(phase: &PalwClaimPhaseV2) -> Self {
        match phase {
            PalwClaimPhaseV2::Provisional => Self::Provisional,
            PalwClaimPhaseV2::PanelBound { .. } => Self::PanelBound,
            PalwClaimPhaseV2::ReceiptLicensed { .. } => Self::ReceiptLicensed,
            PalwClaimPhaseV2::Final { .. } => Self::Final,
            PalwClaimPhaseV2::Voided { .. } => Self::Voided,
            PalwClaimPhaseV2::DefaultDisputed { .. } => Self::DefaultDisputed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ConcentrationV1 {
    Operator,
    Class,
}

/// One thing `safe` is waiting for (or one fact about why it cannot advance).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum SafeWaitV1 {
    /// The committed settlement policy is unusable.
    InvalidPolicy,
    /// The certificate's history is incomplete. Nothing is certified around a gap.
    #[serde(rename_all = "camelCase")]
    MissingHistory {
        gap: HistoryGapV1,
        block: Option<Hash64>,
    },
    /// No executed, root-verified block on the selected chain yet.
    Unexecuted,
    /// A below-finalized conflict was seen: sticky until a validated import.
    FinalizedConflict,
    /// The PALW safe frontier has not reached the effect (or is not on the sink's branch).
    #[serde(rename_all = "camelCase")]
    FrontierBehind {
        frontier_blue: u64,
        effect_blue: u64,
        frontier_on_branch: bool,
    },
    /// A claim accepted at or before the effect is unresolved. `wait_daa` is set only for a `Final` claim, whose retention lapse
    /// is a clock; for any other stage the claim's own panel / receipt / challenge / court clocks decide, and no end is promised.
    #[serde(rename_all = "camelCase")]
    OpenClaim {
        claim: Hash64,
        stage: ClaimStageV1,
        accepted_blue: u64,
        /// The DAA through which the producer owes the trace; a `Final` claim is open until it lapses.
        retention_daa: u64,
        /// The next deadline the sweep holds for the claim, if any.
        next_deadline_daa: Option<u64>,
        wait_daa: Option<u64>,
    },
    /// A data-availability court session names a claim at or before the effect (`claim_known` false: the claim is not in the
    /// state, so the dispute cannot be located and counts against every effect).
    #[serde(rename_all = "camelCase")]
    OpenDaSession {
        claim: Hash64,
        claim_known: bool,
        deadline_daa: u64,
        wait_daa: u64,
    },
    /// Facts that would qualify exist, but they are not mature yet. `ready_daa` is the first DAA at which the facts already on the
    /// chain meet the policy (`None`: even all of them would not).
    #[serde(rename_all = "camelCase")]
    WaitingMaturity {
        facts: u64,
        work: String,
        earliest_matured_daa: u64,
        wait_daa: u64,
        ready_daa: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    InsufficientDepth {
        have: u64,
        need: u64,
    },
    #[serde(rename_all = "camelCase")]
    InsufficientWork {
        have: String,
        need: String,
    },
    #[serde(rename_all = "camelCase")]
    ConcentratedWork {
        dimension: ConcentrationV1,
        top_permille: u32,
        cap_permille: u16,
    },
    /// One work identity appears twice: permanent while both facts exist.
    DuplicateWork,
    ArithmeticOverflow,
}

impl SafeWaitV1 {
    /// The stop reason this wait is, if it is one. `WaitingMaturity` explains a depth/work shortfall and is not a stop of its own.
    pub fn stop(&self) -> Option<SettlementStopV1> {
        use SettlementStopV1 as S;
        Some(match self {
            Self::InvalidPolicy => S::InvalidPolicy,
            Self::MissingHistory { .. } => S::MissingHistory,
            Self::Unexecuted => S::Unexecuted,
            Self::FinalizedConflict => S::FinalizedConflict,
            Self::FrontierBehind { .. } => S::FrontierNotCovered,
            Self::OpenClaim { .. } | Self::OpenDaSession { .. } => S::OpenLifecycle,
            Self::WaitingMaturity { .. } => return None,
            Self::InsufficientDepth { .. } => S::InsufficientDepth,
            Self::InsufficientWork { .. } => S::InsufficientWork,
            Self::ConcentratedWork { .. } => S::ConcentratedWork,
            Self::DuplicateWork => S::DuplicateWork,
            Self::ArithmeticOverflow => S::ArithmeticOverflow,
        })
    }
}

/// The policy numbers the certificate is held against.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessPolicyV1 {
    pub settled_anchor_depth: u64,
    /// Decimal string (consensus u128).
    pub unique_mature_work: String,
    pub max_operator_permille: u16,
    pub max_class_permille: u16,
}

/// The maturity rule in force. Named so a reader knows what "mature" means on this network; v1 is the only rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadinessMaturityV1 {
    /// `v1`: a fact is mature at `max(trace_retention_daa, final_daa + claim_retirement_daa)` (a free-prompt slice also not before
    /// its spend plus `quantum_maturity_daa`).
    pub rule: String,
    pub claim_retirement_daa: u64,
    pub quantum_maturity_daa: u64,
}

/// What has been counted toward one effect, from matured facts only.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvidenceTallyV1 {
    pub anchors: u64,
    pub work: String,
    pub matured_facts: u64,
    pub pending_facts: u64,
    pub pending_work: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EffectReadinessV1 {
    pub block: Hash64,
    pub daa: u64,
    pub blue: u64,
    /// This effect and every older one certify: it is at or below `safe`.
    pub in_safe_prefix: bool,
    /// Every unmet condition of THIS effect, in the certificate's order. Empty: it certifies on its own.
    pub waits: Vec<SafeWaitV1>,
    pub open_claims_total: u64,
    pub open_sessions_total: u64,
    /// DAA from the sink until this effect would certify, counting only clocks that are already running and facts already on the
    /// chain. `None`: some unmet condition needs an event that has not happened.
    pub earliest_ready_in_daa: Option<u64>,
    pub evidence: EvidenceTallyV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum FinalizedWaitV1 {
    /// There is no certified prefix for the pruning point to sit under.
    NoSafePrefix,
    /// The pruning point carries no EVM result on this node.
    PruningPointNotExecuted,
    /// The pruning point is not an ancestor of `safe` (it is above it, or on another branch).
    #[serde(rename_all = "camelCase")]
    PruningPointNotUnderSafe { safe_blue: u64 },
    /// A below-finalized conflict is recorded.
    Conflict,
}

/// `finalized` is the validated pruning point under a certified safe prefix: it advances with the pruning point, not with any
/// maturity number.
///
/// **A published `finalized` may be withdrawn, and never silently** (RFC-0012 C11, decided in the policy proposal §12): the label is
/// recomputed from the evidence at every virtual change, so when the certified prefix retreats below the pruning point the label goes
/// to `null` — fail-closed, never to a different block — while the block itself stays canonical (a reorg that abandons it is the
/// separate, sticky `finalizedConflict`). `withdrawn_from` names the head this node published and withdrew, from the withdrawing virtual
/// change until a `finalized` is published again. In memory: a restart forgets it (the log line does not).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FinalizedReadinessV1 {
    pub finalized: Option<Hash64>,
    pub pruning_point: Hash64,
    pub pruning_blue: Option<u64>,
    pub wait: Option<FinalizedWaitV1>,
    #[serde(default)]
    pub withdrawn_from: Option<Hash64>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeSafeReadinessV1 {
    pub version: u16,
    /// The sink the explanation stands at (the snapshot's `generation`).
    pub generation: Hash64,
    pub sink_daa: u64,
    pub sink_blue: u64,
    pub policy: ReadinessPolicyV1,
    pub maturity: ReadinessMaturityV1,
    /// Executed effects on the walked chain (pruning point to sink).
    pub executed_effects: u64,
    pub safe: Option<Hash64>,
    /// How far `safe` trails the newest executed effect.
    pub safe_lag_daa: Option<u64>,
    pub safe_lag_blue: Option<u64>,
    pub stop: Option<SettlementStopV1>,
    /// Weighing never happened: the history, execution or a recorded conflict stopped it first.
    pub stopped_early: Option<SafeWaitV1>,
    /// The first effect that does not certify (the one right above `safe`).
    pub blocking: Option<EffectReadinessV1>,
    /// The newest executed effect (`latest`), when it is not the blocking one.
    pub tip: Option<EffectReadinessV1>,
    pub finalized: FinalizedReadinessV1,
    pub skipped: SkippedEvidenceV1,
}

/// An unresolved claim at the sink, as the lifecycle rule sees it. Every claim the rule counts as open, nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenClaimV1 {
    pub claim: Hash64,
    pub stage: ClaimStageV1,
    pub accepted_blue: u64,
    pub retention_daa: u64,
    pub next_deadline_daa: Option<u64>,
}

/// An open data-availability session at the sink.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenSessionV1 {
    pub claim: Hash64,
    /// `None`: the claim is not in the state.
    pub claim_accepted_blue: Option<u64>,
    pub deadline_daa: u64,
}

/// Everything the explanation reads. The `chain` and its flags are the ones the snapshot was certified from.
pub struct NativeReadinessInputV1<'a> {
    pub generation: Hash64,
    pub sink_daa: u64,
    pub sink_blue: u64,
    pub policy: PalwSettlementPolicyV1,
    pub claim_retirement_daa: u64,
    pub quantum_maturity_daa: u64,
    /// Executed effects, oldest first, with their blocks.
    pub chain: &'a [(Hash64, NativeEffectV1)],
    /// The certified prefix of exactly this chain (`certify_native_prefix_v1`).
    pub prefix: &'a NativePrefixV1,
    pub facts: &'a [MatureUsefulWorkV1],
    pub frontier_blue: u64,
    pub frontier_on_branch: bool,
    pub open_claims: &'a [OpenClaimV1],
    pub open_sessions: &'a [OpenSessionV1],
    pub skipped: SkippedEvidenceV1,
    pub finalized: FinalizedReadinessV1,
}

fn policy_of(p: PalwSettlementPolicyV1) -> ReadinessPolicyV1 {
    ReadinessPolicyV1 {
        settled_anchor_depth: p.settled_anchor_depth,
        unique_mature_work: p.unique_mature_work.to_string(),
        max_operator_permille: p.max_operator_permille,
        max_class_permille: p.max_class_permille,
    }
}

pub fn native_maturity_report_v1(claim_retirement_daa: u64, quantum_maturity_daa: u64) -> ReadinessMaturityV1 {
    ReadinessMaturityV1 { rule: "v1".into(), claim_retirement_daa, quantum_maturity_daa }
}

/// The report for a weighing that never happened: the snapshot carried `stop`, and nothing is certified.
pub fn native_stopped_readiness_v1(
    generation: Hash64,
    sink: (u64, u64),
    policy: PalwSettlementPolicyV1,
    maturity: ReadinessMaturityV1,
    wait: SafeWaitV1,
    finalized: FinalizedReadinessV1,
    skipped: SkippedEvidenceV1,
) -> NativeSafeReadinessV1 {
    NativeSafeReadinessV1 {
        version: NATIVE_READINESS_VERSION_V1,
        generation,
        sink_daa: sink.0,
        sink_blue: sink.1,
        policy: policy_of(policy),
        maturity,
        executed_effects: 0,
        safe: None,
        safe_lag_daa: None,
        safe_lag_blue: None,
        stop: wait.stop(),
        stopped_early: Some(wait),
        blocking: None,
        tip: None,
        finalized,
        skipped,
    }
}

/// The first DAA at or after `not_before` at which the facts that already qualify by position meet the policy, counting each fact from
/// its own maturity. `None` if even all of them together would not (or a duplicate / overflow makes the sum permanently unusable).
/// `not_before` matters because evidence is NOT monotone in time: a fact that matures later can break a concentration cap that held
/// earlier, so the answer for "when is everything satisfied at once" is a scan from the last clock, not the maximum of two scans.
fn evidence_ready_daa(
    policy: PalwSettlementPolicyV1,
    effect: &NativeEffectV1,
    qualifying: &[&MatureUsefulWorkV1],
    not_before: u64,
) -> Option<u64> {
    let mut by_maturity: Vec<&&MatureUsefulWorkV1> = qualifying.iter().collect();
    by_maturity.sort_by_key(|f| (f.matured_daa, f.identity));
    let mut acc = EvidenceAccumulatorV1::default();
    let mut anchors = std::collections::BTreeSet::new();
    let mut idx = 0;
    // The state at `not_before`, then at each later maturity.
    let mut at = not_before;
    loop {
        while idx < by_maturity.len() && by_maturity[idx].matured_daa <= at {
            let f = by_maturity[idx];
            acc.add_fact(f);
            if f.anchor_daa >= effect.daa && f.anchor_blue >= effect.blue {
                anchors.insert(f.anchor);
            }
            idx += 1;
        }
        acc.anchors = anchors.len() as u64;
        if acc.evidence(policy).is_ok() {
            return Some(at);
        }
        if acc.duplicate || acc.overflow {
            return None;
        }
        at = by_maturity.get(idx)?.matured_daa;
    }
}

fn unmet_evidence(acc: &EvidenceAccumulatorV1, policy: PalwSettlementPolicyV1) -> Vec<SafeWaitV1> {
    let mut out = Vec::new();
    if acc.duplicate {
        out.push(SafeWaitV1::DuplicateWork);
    }
    if acc.overflow {
        out.push(SafeWaitV1::ArithmeticOverflow);
        return out;
    }
    if acc.anchors < policy.settled_anchor_depth {
        out.push(SafeWaitV1::InsufficientDepth { have: acc.anchors, need: policy.settled_anchor_depth });
    }
    if acc.work < policy.unique_mature_work {
        out.push(SafeWaitV1::InsufficientWork { have: acc.work.to_string(), need: policy.unique_mature_work.to_string() });
    }
    for (max, limit, dimension) in [
        (acc.max_operator, policy.max_operator_permille, ConcentrationV1::Operator),
        (acc.max_class, policy.max_class_permille, ConcentrationV1::Class),
    ] {
        let Some(scaled) = max.checked_mul(1000) else {
            out.push(SafeWaitV1::ArithmeticOverflow);
            return out;
        };
        let Some(bound) = acc.work.checked_mul(limit as u128) else {
            out.push(SafeWaitV1::ArithmeticOverflow);
            return out;
        };
        if scaled > bound {
            let top = if acc.work == 0 { 1000 } else { scaled.div_ceil(acc.work).min(u32::MAX as u128) as u32 };
            out.push(SafeWaitV1::ConcentratedWork { dimension, top_permille: top, cap_permille: limit });
        }
    }
    out
}

fn diagnose(i: &NativeReadinessInputV1<'_>, idx: usize) -> EffectReadinessV1 {
    let (block, e) = i.chain[idx];
    let mut waits: Vec<SafeWaitV1> = Vec::new();
    // Does everything unmet clear by the passage of time alone, on facts already on the chain?
    let mut time_driven = true;
    // The longest lifecycle clock already running (a `Final` claim's trace retention), and the moment everything is met at once.
    let mut lifecycle_clock: u64 = 0;
    let mut promised_clock: u64 = 0;

    if !i.policy.valid() {
        waits.push(SafeWaitV1::InvalidPolicy);
        time_driven = false;
    }
    if !e.frontier_covers {
        waits.push(SafeWaitV1::FrontierBehind {
            frontier_blue: i.frontier_blue,
            effect_blue: e.blue,
            frontier_on_branch: i.frontier_on_branch,
        });
        time_driven = false;
    }

    // Lifecycle: every claim accepted at or before the effect, and every session that names one.
    let mut claims: Vec<&OpenClaimV1> = i.open_claims.iter().filter(|c| c.accepted_blue <= e.blue).collect();
    claims.sort_by_key(|c| (c.accepted_blue, c.claim));
    let mut sessions: Vec<&OpenSessionV1> = i.open_sessions.iter().filter(|s| s.claim_accepted_blue.unwrap_or(0) <= e.blue).collect();
    sessions.sort_by_key(|s| (s.deadline_daa, s.claim));
    if !e.lifecycle_closed {
        for c in &claims {
            if c.stage == ClaimStageV1::Final {
                lifecycle_clock = lifecycle_clock.max(c.retention_daa.saturating_sub(i.sink_daa));
            } else {
                time_driven = false;
            }
        }
        if !sessions.is_empty() {
            time_driven = false;
        }
        for c in claims.iter().take(NATIVE_READINESS_LISTED_V1) {
            waits.push(SafeWaitV1::OpenClaim {
                claim: c.claim,
                stage: c.stage,
                accepted_blue: c.accepted_blue,
                retention_daa: c.retention_daa,
                next_deadline_daa: c.next_deadline_daa,
                wait_daa: (c.stage == ClaimStageV1::Final).then(|| c.retention_daa.saturating_sub(i.sink_daa)),
            });
        }
        for s in sessions.iter().take(NATIVE_READINESS_LISTED_V1) {
            waits.push(SafeWaitV1::OpenDaSession {
                claim: s.claim,
                claim_known: s.claim_accepted_blue.is_some(),
                deadline_daa: s.deadline_daa,
                wait_daa: s.deadline_daa.saturating_sub(i.sink_daa),
            });
        }
        if claims.is_empty() && sessions.is_empty() {
            // The flag says open and nothing explains it: never presented as a clock.
            time_driven = false;
        }
    }

    // Evidence: matured facts are counted, the rest are the maturity wait.
    let qualifying: Vec<&MatureUsefulWorkV1> =
        i.facts.iter().filter(|f| f.accepted_daa >= e.daa && f.accepted_blue >= e.blue && f.work > 0).collect();
    let mut acc = EvidenceAccumulatorV1::default();
    let mut anchors = std::collections::BTreeSet::new();
    let (mut matured, mut pending, mut pending_work) = (0u64, 0u64, 0u128);
    let mut earliest_pending: Option<u64> = None;
    for f in &qualifying {
        if f.matured_daa <= i.sink_daa {
            acc.add_fact(f);
            if f.anchor_daa >= e.daa && f.anchor_blue >= e.blue {
                anchors.insert(f.anchor);
            }
            matured += 1;
        } else {
            pending += 1;
            pending_work = pending_work.saturating_add(f.work);
            earliest_pending = Some(earliest_pending.map_or(f.matured_daa, |m| m.min(f.matured_daa)));
        }
    }
    acc.anchors = anchors.len() as u64;
    let tally = EvidenceTallyV1 {
        anchors: acc.anchors,
        work: acc.work.to_string(),
        matured_facts: matured,
        pending_facts: pending,
        pending_work: pending_work.to_string(),
    };
    if i.policy.valid() {
        let unmet = unmet_evidence(&acc, i.policy);
        let evidence_met = unmet.is_empty();
        if !evidence_met {
            waits.extend(unmet);
            match earliest_pending {
                Some(earliest) => {
                    // The evidence alone: the first DAA at which the facts already on the chain meet the policy.
                    let ready = evidence_ready_daa(i.policy, &e, &qualifying, i.sink_daa);
                    waits.push(SafeWaitV1::WaitingMaturity {
                        facts: pending,
                        work: pending_work.to_string(),
                        earliest_matured_daa: earliest,
                        wait_daa: earliest.saturating_sub(i.sink_daa),
                        ready_daa: ready,
                    });
                    if ready.is_none() {
                        time_driven = false;
                    }
                }
                None => time_driven = false,
            }
        }
        // Everything at once. Evidence is not monotone in time (a fact that matures later can break a cap), so when a lifecycle clock
        // is running the evidence is re-read at the moment the last clock stops, not assumed from its own earlier answer.
        if time_driven && (!evidence_met || lifecycle_clock > 0) {
            match evidence_ready_daa(i.policy, &e, &qualifying, i.sink_daa.saturating_add(lifecycle_clock)) {
                Some(at) => promised_clock = at.saturating_sub(i.sink_daa),
                None => time_driven = false,
            }
        }
    }

    let in_prefix = i.prefix.safe.is_some_and(|s| idx <= s);
    let certified_alone = waits.is_empty();
    EffectReadinessV1 {
        block,
        daa: e.daa,
        blue: e.blue,
        in_safe_prefix: in_prefix,
        earliest_ready_in_daa: (!certified_alone && time_driven).then_some(promised_clock),
        waits,
        open_claims_total: claims.len() as u64,
        open_sessions_total: sessions.len() as u64,
        evidence: tally,
    }
}

/// Explain the certificate: which effect holds `safe` back and everything it still lacks, plus the same for the newest effect.
pub fn native_safe_readiness_v1(i: &NativeReadinessInputV1<'_>) -> NativeSafeReadinessV1 {
    let n = i.chain.len();
    let blocking_idx = match i.prefix.safe {
        Some(s) if s + 1 < n => Some(s + 1),
        Some(_) => None,
        None if n > 0 => Some(0),
        None => None,
    };
    let blocking = blocking_idx.map(|k| diagnose(i, k));
    let tip_idx = n.checked_sub(1).filter(|t| Some(*t) != blocking_idx);
    let tip = tip_idx.map(|t| diagnose(i, t));
    let (safe_lag_daa, safe_lag_blue) = match (i.prefix.safe, i.chain.last()) {
        (Some(s), Some((_, newest))) => {
            (Some(newest.daa.saturating_sub(i.chain[s].1.daa)), Some(newest.blue.saturating_sub(i.chain[s].1.blue)))
        }
        _ => (None, None),
    };
    NativeSafeReadinessV1 {
        version: NATIVE_READINESS_VERSION_V1,
        generation: i.generation,
        sink_daa: i.sink_daa,
        sink_blue: i.sink_blue,
        policy: policy_of(i.policy),
        maturity: native_maturity_report_v1(i.claim_retirement_daa, i.quantum_maturity_daa),
        executed_effects: n as u64,
        safe: i.prefix.safe.map(|s| i.chain[s].0),
        safe_lag_daa,
        safe_lag_blue,
        stop: i.prefix.stop,
        stopped_early: None,
        blocking,
        tip,
        finalized: i.finalized.clone(),
        skipped: i.skipped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_native_settlement_v1::{certify_native_effect_v1, certify_native_prefix_v1, native_open_from_v1};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }

    fn policy() -> PalwSettlementPolicyV1 {
        PalwSettlementPolicyV1 { settled_anchor_depth: 2, unique_mature_work: 20, max_operator_permille: 600, max_class_permille: 600 }
    }

    fn fact(v: u64, blue: u64, matured: u64, work: u128) -> MatureUsefulWorkV1 {
        MatureUsefulWorkV1 {
            identity: h(v),
            anchor: h(v),
            operator: h(v),
            class: h(v),
            anchor_blue: blue,
            accepted_blue: blue,
            anchor_daa: blue,
            accepted_daa: blue,
            matured_daa: matured,
            work,
        }
    }

    fn finalized() -> FinalizedReadinessV1 {
        FinalizedReadinessV1 {
            finalized: None,
            pruning_point: h(0),
            pruning_blue: Some(0),
            wait: Some(FinalizedWaitV1::NoSafePrefix),
            withdrawn_from: None,
        }
    }

    /// Run the explanation on a chain of `n` effects at blues 10, 20, … (daa = blue).
    #[allow(clippy::too_many_arguments)]
    fn explain(
        n: usize,
        covers_until: usize,
        facts: &[MatureUsefulWorkV1],
        claims: &[OpenClaimV1],
        sessions: &[OpenSessionV1],
        sink_daa: u64,
        policy: PalwSettlementPolicyV1,
    ) -> (NativeSafeReadinessV1, NativePrefixV1) {
        let open_from = native_open_from_v1(
            claims.iter().map(|c| (c.accepted_blue, PalwClaimPhaseV2::Provisional, c.retention_daa)),
            sessions.iter().map(|s| s.claim_accepted_blue),
            sink_daa,
        );
        // `native_open_from_v1` treats a Final claim as closed once its retention lapses: the fixtures only list unlapsed ones.
        let chain: Vec<(Hash64, NativeEffectV1)> = (1..=n)
            .map(|k| {
                let blue = 10 * k as u64;
                (
                    h(0xB000 + k as u64),
                    NativeEffectV1 { daa: blue, blue, frontier_covers: k <= covers_until, lifecycle_closed: blue < open_from },
                )
            })
            .collect();
        let effects: Vec<NativeEffectV1> = chain.iter().map(|c| c.1).collect();
        let prefix = certify_native_prefix_v1(policy, &effects, sink_daa, facts);
        let input = NativeReadinessInputV1 {
            generation: h(0x51),
            sink_daa,
            sink_blue: 10 * n as u64,
            policy,
            claim_retirement_daa: 3_000,
            quantum_maturity_daa: 120,
            chain: &chain,
            prefix: &prefix,
            facts,
            frontier_blue: 10 * covers_until as u64,
            frontier_on_branch: true,
            open_claims: claims,
            open_sessions: sessions,
            skipped: SkippedEvidenceV1::default(),
            finalized: finalized(),
        };
        (native_safe_readiness_v1(&input), prefix)
    }

    fn stop_of(e: &EffectReadinessV1) -> Option<SettlementStopV1> {
        e.waits.iter().find_map(|w| w.stop())
    }

    #[test]
    fn rfc0012_readiness_first_stop_is_the_snapshots_stop_and_a_certified_chain_has_no_blocker() {
        // Four effects, facts at blue 30 and 40 (two anchors, 20 work), matured.
        let facts = [fact(1, 30, 0, 10), fact(2, 40, 0, 10)];
        let (r, prefix) = explain(4, 4, &facts, &[], &[], 100, policy());
        assert_eq!(prefix.stop, Some(SettlementStopV1::InsufficientDepth), "the newest effects have no later anchors");
        let b = r.blocking.as_ref().expect("a blocker");
        assert_eq!(stop_of(b), prefix.stop, "the explanation's first stop is the certificate's");
        assert_eq!(r.safe, Some(h(0xB000 + 3)), "effects 1..=3 certify, 4 does not");
        assert_eq!(b.block, h(0xB000 + 4));
        assert_eq!(r.tip, None, "the tip is the blocker: not reported twice");
        // Everything certifies.
        let facts = [fact(1, 30, 0, 10), fact(2, 40, 0, 10)];
        let (r, prefix) = explain(2, 2, &facts, &[], &[], 100, policy());
        assert_eq!((prefix.stop, r.blocking.is_none()), (None, true));
        assert!(r.tip.as_ref().is_some_and(|t| t.in_safe_prefix && t.waits.is_empty()));
    }

    #[test]
    fn rfc0012_readiness_names_every_unmet_condition_not_only_the_first() {
        // Frontier behind, an open Provisional claim, and no evidence at all.
        let claims = [OpenClaimV1 {
            claim: h(0xC1),
            stage: ClaimStageV1::Provisional,
            accepted_blue: 10,
            retention_daa: 5_400,
            next_deadline_daa: Some(700),
        }];
        let (r, prefix) = explain(3, 0, &[], &claims, &[], 100, policy());
        let b = r.blocking.expect("a blocker");
        assert_eq!(prefix.stop, Some(SettlementStopV1::FrontierNotCovered));
        let kinds: Vec<_> = b.waits.iter().map(|w| w.stop()).collect();
        assert_eq!(
            kinds,
            vec![
                Some(SettlementStopV1::FrontierNotCovered),
                Some(SettlementStopV1::OpenLifecycle),
                Some(SettlementStopV1::InsufficientDepth),
                Some(SettlementStopV1::InsufficientWork),
            ]
        );
        assert_eq!(b.open_claims_total, 1);
        assert_eq!(b.earliest_ready_in_daa, None, "a Provisional claim and a frontier that has not moved are events, not clocks");
        match &b.waits[1] {
            SafeWaitV1::OpenClaim { claim, stage, wait_daa, next_deadline_daa, .. } => {
                assert_eq!((*claim, *stage, *wait_daa, *next_deadline_daa), (h(0xC1), ClaimStageV1::Provisional, None, Some(700)));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn rfc0012_readiness_maturity_wait_is_exact_and_tight() {
        // Facts at blue 30 / 40 mature at DAA 500 and 800; sink at 400 -> nothing matured, depth and work short.
        let facts = [fact(1, 30, 500, 10), fact(2, 40, 800, 10)];
        let (r, prefix) = explain(1, 1, &facts, &[], &[], 400, policy());
        let b = r.blocking.expect("a blocker");
        assert_eq!(prefix.stop, Some(SettlementStopV1::InsufficientDepth));
        assert_eq!(b.evidence.pending_facts, 2);
        let wait =
            b.waits.iter().find_map(|w| if let SafeWaitV1::WaitingMaturity { .. } = w { Some(w.clone()) } else { None }).unwrap();
        assert_eq!(
            wait,
            SafeWaitV1::WaitingMaturity {
                facts: 2,
                work: "20".into(),
                earliest_matured_daa: 500,
                wait_daa: 100,
                ready_daa: Some(800)
            }
        );
        assert_eq!(b.earliest_ready_in_daa, Some(400), "800 - 400: both facts must mature");
        // Tight: the certificate flips exactly there.
        let at = |t: u64| certify_native_effect_v1(policy(), (10, 10), t, true, true, true, true, &facts).is_ok();
        assert!(!at(799) && at(800));
    }

    #[test]
    fn rfc0012_readiness_a_final_claims_retention_lapse_is_a_clock_and_sets_the_estimate() {
        // One Final claim at blue 10 holds the lifecycle until DAA 600; facts are mature already.
        let facts = [fact(1, 30, 0, 10), fact(2, 40, 0, 10)];
        let claims = [OpenClaimV1 {
            claim: h(0xC2),
            stage: ClaimStageV1::Final,
            accepted_blue: 10,
            retention_daa: 600,
            next_deadline_daa: None,
        }];
        let (r, prefix) = explain(3, 3, &facts, &claims, &[], 100, policy());
        let b = r.blocking.expect("a blocker");
        assert_eq!(prefix.stop, Some(SettlementStopV1::OpenLifecycle));
        assert_eq!(b.earliest_ready_in_daa, Some(500));
        match &b.waits[0] {
            SafeWaitV1::OpenClaim { stage: ClaimStageV1::Final, wait_daa: Some(500), .. } => {}
            other => panic!("{other:?}"),
        }
        // At the estimate the lifecycle is closed (the lapse is `retention <= sink`), one DAA earlier it is not.
        let closed = |t: u64| 10 < native_open_from_v1([(10, PalwClaimPhaseV2::Final { final_daa: 0 }, 600)], [], t);
        assert!(!closed(599) && closed(600));
    }

    #[test]
    fn rfc0012_readiness_an_open_session_and_a_missing_claim_are_named_and_never_a_clock() {
        let facts = [fact(1, 30, 0, 10), fact(2, 40, 0, 10)];
        let sessions = [
            OpenSessionV1 { claim: h(0xD1), claim_accepted_blue: Some(20), deadline_daa: 900 },
            OpenSessionV1 { claim: h(0xD2), claim_accepted_blue: None, deadline_daa: 700 },
        ];
        let (r, prefix) = explain(3, 3, &facts, &[], &sessions, 100, policy());
        let b = r.blocking.expect("a blocker");
        assert_eq!(prefix.stop, Some(SettlementStopV1::OpenLifecycle));
        // The oldest effect (blue 10) is held only by the session whose claim is gone: it counts against every effect.
        assert_eq!(b.open_sessions_total, 1);
        assert_eq!(b.earliest_ready_in_daa, None);
        // The newest (blue 30) is held by both, ordered by deadline.
        let tip = r.tip.expect("the tip");
        assert_eq!(tip.open_sessions_total, 2);
        let listed: Vec<_> = tip
            .waits
            .iter()
            .filter_map(|w| match w {
                SafeWaitV1::OpenDaSession { claim, claim_known, deadline_daa, wait_daa } => {
                    Some((*claim, *claim_known, *deadline_daa, *wait_daa))
                }
                _ => None,
            })
            .collect();
        assert_eq!(listed, vec![(h(0xD2), false, 700, 600), (h(0xD1), true, 900, 800)], "ordered by deadline");
    }

    #[test]
    fn rfc0012_readiness_lists_are_capped_and_totals_are_exact() {
        let claims: Vec<OpenClaimV1> = (0..20)
            .map(|k| OpenClaimV1 {
                claim: h(0xE000 + k),
                stage: ClaimStageV1::ReceiptLicensed,
                accepted_blue: 10,
                retention_daa: 5_400,
                next_deadline_daa: None,
            })
            .collect();
        let (r, _) = explain(2, 2, &[], &claims, &[], 100, policy());
        let b = r.blocking.expect("a blocker");
        assert_eq!(b.open_claims_total, 20);
        assert_eq!(b.waits.iter().filter(|w| matches!(w, SafeWaitV1::OpenClaim { .. })).count(), NATIVE_READINESS_LISTED_V1);
    }

    #[test]
    fn rfc0012_readiness_concentration_duplicate_and_overflow_are_reported_as_what_they_are() {
        // One operator supplies all the work: 1000 permille against a 600 cap.
        let mut a = fact(1, 30, 0, 15);
        let mut b = fact(2, 40, 0, 15);
        a.operator = h(0x77);
        b.operator = h(0x77);
        let (r, prefix) = explain(1, 1, &[a, b], &[], &[], 100, policy());
        assert_eq!(prefix.stop, Some(SettlementStopV1::ConcentratedWork));
        let blk = r.blocking.expect("a blocker");
        assert!(blk.waits.contains(&SafeWaitV1::ConcentratedWork {
            dimension: ConcentrationV1::Operator,
            top_permille: 1000,
            cap_permille: 600
        }));
        assert_eq!(blk.earliest_ready_in_daa, None, "more work from other operators is an event, not a clock");
        // The same identity twice.
        let c = fact(1, 40, 0, 10);
        let (r, prefix) = explain(1, 1, &[fact(1, 30, 0, 10), c], &[], &[], 100, policy());
        assert_eq!(prefix.stop, Some(SettlementStopV1::DuplicateWork));
        assert_eq!(stop_of(&r.blocking.unwrap()), Some(SettlementStopV1::DuplicateWork));
        // Overflow.
        let (r, prefix) = explain(1, 1, &[fact(1, 30, 0, u128::MAX), fact(2, 40, 0, 1)], &[], &[], 100, policy());
        assert_eq!(prefix.stop, Some(SettlementStopV1::ArithmeticOverflow));
        assert_eq!(stop_of(&r.blocking.unwrap()), Some(SettlementStopV1::ArithmeticOverflow));
    }

    /// The explanation is held against the certificate on many shapes: for the blocking effect its first stop equals the
    /// prefix's stop; and when it promises a time, the per-effect reference certifies exactly then and not a DAA before.
    #[test]
    fn rfc0012_readiness_agrees_with_the_certificate_on_generated_chains() {
        let mut seed = 0x9E37_79B9_7F4A_7C15u64;
        let mut next = move |m: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % m.max(1)
        };
        let mut promised = 0;
        for case in 0..400u64 {
            let n = 1 + next(6) as usize;
            let covers = next(n as u64 + 1) as usize;
            let sink_daa = 100 + next(200);
            let facts: Vec<MatureUsefulWorkV1> = (0..next(7))
                .map(|k| {
                    let mut f = fact(1 + k + 100 * case, 10 * (1 + next(n as u64 + 1)), next(500), 1 + next(12) as u128);
                    f.operator = h(next(3));
                    f.class = h(next(2));
                    f.anchor = h(1 + next(4) + 100 * case);
                    f
                })
                .collect();
            // Only Final claims, so every wait is a clock.
            let claims: Vec<OpenClaimV1> = (0..next(3))
                .map(|k| OpenClaimV1 {
                    claim: h(0xF000 + k),
                    stage: ClaimStageV1::Final,
                    accepted_blue: 10 * (1 + next(n as u64)),
                    retention_daa: sink_daa + 1 + next(300),
                    next_deadline_daa: None,
                })
                .collect();
            let p = PalwSettlementPolicyV1 {
                settled_anchor_depth: 1 + next(3),
                unique_mature_work: 1 + next(25) as u128,
                max_operator_permille: 500 + next(501) as u16,
                max_class_permille: 500 + next(501) as u16,
            };
            let (r, prefix) = explain(n, covers, &facts, &claims, &[], sink_daa, p);
            if let Some(b) = &r.blocking {
                assert_eq!(stop_of(b), prefix.stop, "case {case}: the first stop is the certificate's");
                if let Some(d) = b.earliest_ready_in_daa {
                    promised += 1;
                    let idx = (b.blue / 10 - 1) as usize;
                    let e = |t: u64| {
                        let open_from = native_open_from_v1(
                            claims.iter().map(|c| (c.accepted_blue, PalwClaimPhaseV2::Final { final_daa: 0 }, c.retention_daa)),
                            [],
                            t,
                        );
                        certify_native_effect_v1(p, (b.daa, b.blue), t, true, idx < covers, b.blue < open_from, true, &facts).is_ok()
                    };
                    assert!(e(sink_daa + d), "case {case}: certifies at the promised time (+{d})");
                    if d > 0 {
                        assert!(!e(sink_daa + d - 1), "case {case}: and not a DAA before");
                    }
                }
            } else {
                assert!(prefix.stop.is_none());
            }
        }
        assert!(promised > 30, "the generator promised a time often enough to mean something ({promised})");
    }

    /// Evidence is not monotone in time: a fact that matures later can break a concentration cap that held earlier. When a lifecycle
    /// clock is running, the promise is read at the moment that clock stops, and withheld if the evidence is broken by then.
    #[test]
    fn rfc0012_readiness_withholds_a_promise_that_a_later_maturity_would_break() {
        let (mut a, mut c, mut b) = (fact(1, 30, 0, 12), fact(2, 40, 0, 8), fact(3, 40, 300, 20));
        (a.operator, c.operator, b.operator) = (h(0x71), h(0x72), h(0x71));
        let facts = [a, c, b];
        let claims = [OpenClaimV1 {
            claim: h(0xC3),
            stage: ClaimStageV1::Final,
            accepted_blue: 10,
            retention_daa: 600,
            next_deadline_daa: None,
        }];
        let (r, prefix) = explain(1, 1, &facts, &claims, &[], 100, policy());
        let blk = r.blocking.expect("held by the lifecycle");
        assert_eq!(prefix.stop, Some(SettlementStopV1::OpenLifecycle));
        assert_eq!(blk.earliest_ready_in_daa, None, "by DAA 600 the third fact has matured and operator 0x71 holds 80%");
        let at = |t: u64| certify_native_effect_v1(policy(), (10, 10), t, true, true, true, true, &facts);
        assert!(at(100).is_ok(), "the evidence alone is met now");
        assert_eq!(at(600), Err(SettlementStopV1::ConcentratedWork), "and broken when the clock stops");
    }

    #[test]
    fn rfc0012_readiness_wire_is_camel_case_and_round_trips() {
        let claims = [OpenClaimV1 {
            claim: h(0xC9),
            stage: ClaimStageV1::DefaultDisputed,
            accepted_blue: 10,
            retention_daa: 5_400,
            next_deadline_daa: Some(1_200),
        }];
        let (r, _) = explain(2, 1, &[fact(1, 20, 900, 5)], &claims, &[], 100, policy());
        let json = serde_json::to_string(&r).unwrap();
        for key in [
            "\"generation\"",
            "\"sinkDaa\"",
            "\"executedEffects\"",
            "\"safeLagDaa\"",
            "\"stoppedEarly\"",
            "\"blocking\"",
            "\"earliestReadyInDaa\"",
            "\"kind\":\"openClaim\"",
            "\"stage\":\"defaultDisputed\"",
            "\"nextDeadlineDaa\":1200",
            "\"kind\":\"waitingMaturity\"",
        ] {
            assert!(json.contains(key), "{key} in {json}");
        }
        assert_eq!(serde_json::from_str::<NativeSafeReadinessV1>(&json).unwrap(), r);
    }

    #[test]
    fn rfc0012_readiness_stopped_early_carries_the_cause() {
        let r = native_stopped_readiness_v1(
            h(0x51),
            (100, 90),
            policy(),
            native_maturity_report_v1(3_000, 120),
            SafeWaitV1::MissingHistory { gap: HistoryGapV1::DeltaNotRetained, block: Some(h(0xB1)) },
            finalized(),
            SkippedEvidenceV1::default(),
        );
        assert_eq!(r.stop, Some(SettlementStopV1::MissingHistory));
        assert!(r.blocking.is_none() && r.safe.is_none());
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"gap\":\"deltaNotRetained\""), "{json}");
        assert_eq!(serde_json::from_str::<NativeSafeReadinessV1>(&json).unwrap(), r);
    }
}
