//! **Node-only reads of the execution lane — what an explorer asks of a node (lane SCAN).**
//!
//! No consensus rule lives here and nothing here is hashed: this is what a node SAW of the round
//! lane and what its sink state HOLDS for it, shaped for `getBlock`'s verbose data and
//! `getPalwRoundLane`.
//!
//! * [`PalwRoundLaneTelemetryV1`] — a bounded, in-memory ledger of the round blocks this node has
//!   judged (accepted with their lineage, or refused with a reason), plus the refusal counters and
//!   the lane's last accepted round. It is filled where the verdicts and the header stage already
//!   decide; it is lost on restart, and a block the ledger no longer holds is reported as `ROUND`
//!   (a round block whose verdict this node cannot recall), never guessed as `EXEC` or `RED`.
//! * [`palw_recent_executions_v1`] — one row per claim whose Final reached the execution lane, read
//!   from the sink's PALW state: the tickets it minted (the rounds it is permitted) and the ones
//!   spent (the rounds accepted).

use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2};
use crate::BlockHash;
use kaspa_hashes::Hash64;
use std::collections::{BTreeMap, HashMap, VecDeque};
use std::sync::{Mutex, OnceLock};

/// How many judged round blocks the ledger keeps. A round is a second; this is over an hour of a
/// full lane and a few hundred kilobytes.
pub const PALW_ROUND_LEDGER_CAPACITY_V1: usize = 4_096;

/// The lane reports STALE when it has accepted no round block for this long (wall clock against the
/// last accepted block's timestamp). A ticket lane with nothing minted is quiet by design; the flag
/// says "look", the refusal reasons beside it say why.
pub const PALW_ROUND_LANE_STALE_AFTER_MS_V1: u64 = 600_000;

/// Why a round block was not granted its permit — the names the explorer prints and the counters key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwRoundRefusalV1 {
    /// The header stage refused the block (envelope, signature, anchor or mergeset rule, PoW).
    HeaderEnvelope,
    HeaderSignature,
    HeaderMergesetRule,
    HeaderAnchorOffChain,
    HeaderOther,
    /// The merging block's verdict (ADR-0125 §7.2, in the order the verdict checks them).
    EnvelopeUndecodable,
    SpanOutsideWindow,
    PermitEquivocated,
    NoSchedule,
    PermitNotGranted,
    BondNotActiveOrKeyMismatch,
    PayoutMismatch,
    PermitAlreadyUsed,
}

impl PalwRoundRefusalV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::HeaderEnvelope => "header_envelope_invalid",
            Self::HeaderSignature => "header_signature_invalid",
            Self::HeaderMergesetRule => "header_mergeset_rule",
            Self::HeaderAnchorOffChain => "header_anchor_off_chain",
            Self::HeaderOther => "header_other",
            Self::EnvelopeUndecodable => "envelope_undecodable",
            Self::SpanOutsideWindow => "span_outside_window",
            Self::PermitEquivocated => "permit_equivocated",
            Self::NoSchedule => "no_schedule_for_span",
            Self::PermitNotGranted => "permit_not_granted",
            Self::BondNotActiveOrKeyMismatch => "bond_inactive_or_key_mismatch",
            Self::PayoutMismatch => "coinbase_payout_mismatch",
            Self::PermitAlreadyUsed => "permit_already_used",
        }
    }
}

/// A granted permit's lineage: the ticket it spent and, through it, the claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRoundLineageV1 {
    /// Zero for a permit the domain lottery handed out (no Final stands behind it).
    pub quantum_id: Hash64,
    pub claim_id: Option<Hash64>,
    pub quantum_index: Option<u32>,
    /// The claim's class, where the state still keeps the claim.
    pub class_id: Option<Hash64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwRoundOutcomeV1 {
    Exec(PalwRoundLineageV1),
    Refused(PalwRoundRefusalV1),
}

/// One round block this node judged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwRoundBlockRecordV1 {
    pub hash: BlockHash,
    pub daa_score: u64,
    pub timestamp_ms: u64,
    pub round: u64,
    pub permit_index: u16,
    pub bond: Option<PalwBondKeyV2>,
    pub outcome: PalwRoundOutcomeV1,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwRoundLaneHealthV1 {
    pub accepted_total: u64,
    pub refused_total: u64,
    /// Refusals counted since the last accepted round block (all of them while none was accepted).
    pub refused_since_last_accepted: u64,
    pub last_accepted: Option<PalwRoundBlockRecordV1>,
    /// The newest round block judged, whatever its outcome.
    pub latest: Option<PalwRoundBlockRecordV1>,
    /// Refusal reasons, most frequent first (ties by name), at most `top`.
    pub top_refusals: Vec<(&'static str, u64)>,
    pub stale: bool,
    pub stale_after_ms: u64,
}

#[derive(Default)]
struct LedgerInner {
    order: VecDeque<BlockHash>,
    by_hash: HashMap<BlockHash, PalwRoundBlockRecordV1>,
    refusals: BTreeMap<&'static str, u64>,
    accepted_total: u64,
    refused_total: u64,
    refused_since_accept: u64,
    last_accepted: Option<PalwRoundBlockRecordV1>,
    latest: Option<PalwRoundBlockRecordV1>,
}

/// The ledger. Interior-mutable and cheap: a mutex held for a map insert.
#[derive(Default)]
pub struct PalwRoundLaneTelemetryV1 {
    inner: Mutex<LedgerInner>,
    /// When the ledger started counting (the node's clock, ms); 0 for a ledger built by a test.
    started_ms: u64,
}

impl PalwRoundLaneTelemetryV1 {
    fn lock(&self) -> std::sync::MutexGuard<'_, LedgerInner> {
        // A poisoned mutex only means a panic elsewhere while holding it; the data is counters.
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Record a verdict on one round block. A block already recorded is overwritten by its newer
    /// verdict without being counted twice (a block can be judged again after a reorg).
    pub fn record(&self, record: PalwRoundBlockRecordV1) {
        let mut g = self.lock();
        let replaced = g.by_hash.insert(record.hash, record);
        if let Some(old) = replaced {
            // Undo the old verdict's count so a re-judgement is one verdict.
            match old.outcome {
                PalwRoundOutcomeV1::Exec(_) => g.accepted_total = g.accepted_total.saturating_sub(1),
                PalwRoundOutcomeV1::Refused(r) => {
                    g.refused_total = g.refused_total.saturating_sub(1);
                    if let Some(n) = g.refusals.get_mut(r.name()) {
                        *n = n.saturating_sub(1);
                    }
                }
            }
        } else {
            g.order.push_back(record.hash);
            while g.order.len() > PALW_ROUND_LEDGER_CAPACITY_V1 {
                if let Some(evicted) = g.order.pop_front() {
                    g.by_hash.remove(&evicted);
                }
            }
        }
        match record.outcome {
            PalwRoundOutcomeV1::Exec(_) => {
                g.accepted_total += 1;
                g.refused_since_accept = 0;
                if g.last_accepted.is_none_or(|prev| (record.timestamp_ms, record.daa_score) >= (prev.timestamp_ms, prev.daa_score)) {
                    g.last_accepted = Some(record);
                }
            }
            PalwRoundOutcomeV1::Refused(r) => {
                g.refused_total += 1;
                g.refused_since_accept += 1;
                *g.refusals.entry(r.name()).or_insert(0) += 1;
            }
        }
        if g.latest.is_none_or(|prev| (record.timestamp_ms, record.daa_score) >= (prev.timestamp_ms, prev.daa_score)) {
            g.latest = Some(record);
        }
    }

    /// A refusal at the header stage: the block never reached the verdict and may never have a
    /// stored header, so only the hash and the reason are known.
    pub fn record_header_refusal(&self, hash: BlockHash, timestamp_ms: u64, daa_score: u64, reason: PalwRoundRefusalV1) {
        self.record(PalwRoundBlockRecordV1 {
            hash,
            daa_score,
            timestamp_ms,
            round: 0,
            permit_index: 0,
            bond: None,
            outcome: PalwRoundOutcomeV1::Refused(reason),
        });
    }

    /// When this ledger started counting — a restart empties it, so every count is "since then".
    pub fn started_ms(&self) -> u64 {
        self.started_ms
    }

    pub fn lookup(&self, hash: &BlockHash) -> Option<PalwRoundBlockRecordV1> {
        self.lock().by_hash.get(hash).copied()
    }

    pub fn health(&self, now_ms: u64, top: usize) -> PalwRoundLaneHealthV1 {
        let g = self.lock();
        let mut top_refusals: Vec<(&'static str, u64)> = g.refusals.iter().filter(|(_, n)| **n > 0).map(|(k, n)| (*k, *n)).collect();
        top_refusals.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
        top_refusals.truncate(top);
        let stale = match g.last_accepted {
            Some(last) => now_ms.saturating_sub(last.timestamp_ms) > PALW_ROUND_LANE_STALE_AFTER_MS_V1,
            None => false,
        };
        PalwRoundLaneHealthV1 {
            accepted_total: g.accepted_total,
            refused_total: g.refused_total,
            refused_since_last_accepted: g.refused_since_accept,
            last_accepted: g.last_accepted,
            latest: g.latest,
            top_refusals,
            stale,
            stale_after_ms: PALW_ROUND_LANE_STALE_AFTER_MS_V1,
        }
    }
}

/// The node's one ledger. A process runs one consensus; tests build their own
/// [`PalwRoundLaneTelemetryV1`] and never read this.
pub fn palw_round_lane_telemetry_v1() -> &'static PalwRoundLaneTelemetryV1 {
    static LEDGER: OnceLock<PalwRoundLaneTelemetryV1> = OnceLock::new();
    LEDGER.get_or_init(|| PalwRoundLaneTelemetryV1 { started_ms: kaspa_core::time::unix_now(), ..Default::default() })
}

/// A block's class as an explorer shows it: `BLUE` (a chain block or merged blue), `EXEC` (a round
/// block that held its permit), `RED` (an ordinary GHOSTDAG red), `ROUND` (a refused round or a
/// round block whose verdict this node cannot recall) or unclassified
/// (not merged yet, or outside what the node keeps).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwBlockClassV1 {
    Blue,
    Exec,
    Red,
    Round,
    Unmerged,
}

impl PalwBlockClassV1 {
    pub fn name(self) -> &'static str {
        match self {
            Self::Blue => "BLUE",
            Self::Exec => "EXEC",
            Self::Red => "RED",
            Self::Round => "ROUND",
            Self::Unmerged => "",
        }
    }
}

/// What the node says about one block's lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwBlockLaneV1 {
    pub class: PalwBlockClassV1,
    /// The round block's envelope, where the block is one and it decodes: `(round, permit index, bond)`.
    pub envelope: Option<(u64, u16, PalwBondKeyV2)>,
    /// The ledger's verdict, while the node still holds it.
    pub record: Option<PalwRoundBlockRecordV1>,
}

/// The class of a ROUND block from the ledger alone: a granted permit is `EXEC`, a refusal `ROUND`, and
/// no record `ROUND`. One function for the node and its tests.
pub fn palw_round_block_class_v1(record: Option<&PalwRoundBlockRecordV1>) -> PalwBlockClassV1 {
    match record.map(|r| r.outcome) {
        Some(PalwRoundOutcomeV1::Exec(_)) => PalwBlockClassV1::Exec,
        Some(PalwRoundOutcomeV1::Refused(_)) => PalwBlockClassV1::Round,
        None => PalwBlockClassV1::Round,
    }
}

/// One execution: a Final whose credit reached the execution lane, and where its rounds stand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwExecutionRowV1 {
    pub claim_id: Hash64,
    /// Where the state still keeps the claim; `None` once it retired.
    pub class_id: Option<Hash64>,
    pub executor_bond: PalwBondKeyV2,
    pub domain: Hash64,
    /// `credited`, `maturing` or `scheduled` (the lane's own stage), see [`PalwExecutionRowV1::status`].
    pub stage: &'static str,
    pub credit: u64,
    pub span: u64,
    /// Rounds the Final is permitted (tickets minted) and rounds already accepted (tickets spent).
    pub tickets: u32,
    pub tickets_spent: u32,
    pub first_round: Option<u64>,
    pub last_round: Option<u64>,
    pub accepted_daa: Option<u64>,
    pub accepted_block: Option<BlockHash>,
    /// `provisional`, `panel_bound`, ... as `getPalwClaims` spells them; empty when retired.
    pub phase: &'static str,
    pub final_daa: Option<u64>,
}

impl PalwExecutionRowV1 {
    /// `voided` (a convicted or defaulted claim), `complete` (every ticket spent), `running`
    /// (scheduled, some left), `no_tickets` (scheduled, the mint issued none), `maturing`
    /// (waiting for its span), `credited` (gathered, waiting for the next span).
    pub fn status(&self) -> &'static str {
        if self.phase == "voided" {
            return "voided";
        }
        match self.stage {
            "scheduled" if self.tickets == 0 => "no_tickets",
            "scheduled" if self.tickets_spent >= self.tickets => "complete",
            "scheduled" => "running",
            other => other,
        }
    }
}

fn phase_name(phase: &PalwClaimPhaseV2) -> &'static str {
    match phase {
        PalwClaimPhaseV2::Provisional => "provisional",
        PalwClaimPhaseV2::PanelBound { .. } => "panel_bound",
        PalwClaimPhaseV2::ReceiptLicensed { .. } => "receipt_licensed",
        PalwClaimPhaseV2::Final { .. } => "final",
        PalwClaimPhaseV2::Voided { .. } => "voided",
        _ => "default_disputed",
    }
}

/// **The executions the sink's state holds, newest first**: every Final of the open span, of the
/// pending snapshots and of the kept schedules, with the rounds each is permitted and has spent.
/// Returns at most `limit` rows (0 = all) and the number of executions the state holds.
pub fn palw_recent_executions_v1(state: &PalwChainStateV2, limit: usize) -> (Vec<PalwExecutionRowV1>, usize) {
    let mut finals: BTreeMap<Hash64, (Hash64, PalwBondKeyV2)> = BTreeMap::new();
    let (_, open) = state.round_finals();
    for (id, f) in open {
        finals.insert(*id, (f.domain, f.bond));
    }
    for snapshot in state.round_pending_snapshots().values() {
        for f in &snapshot.finals {
            finals.insert(f.claim_id, (f.domain, f.bond));
        }
    }
    for schedule in state.round_schedules().values() {
        for f in &schedule.finals {
            finals.insert(f.claim_id, (f.domain, f.bond));
        }
    }
    let mut rows: Vec<PalwExecutionRowV1> = finals
        .into_iter()
        .filter_map(|(claim_id, (domain, bond))| {
            let lane = crate::palw_producer_v2::palw_claim_exec_lane_v1(state, &claim_id)?;
            let claim = state.claim(&claim_id);
            Some(PalwExecutionRowV1 {
                claim_id,
                class_id: claim.map(|c| c.class_id),
                executor_bond: claim.map(|c| c.bond).unwrap_or(bond),
                domain,
                stage: lane.stage,
                credit: lane.credit,
                span: lane.span,
                tickets: lane.tickets,
                tickets_spent: lane.tickets_spent,
                first_round: lane.first_round,
                last_round: lane.last_round,
                accepted_daa: claim.map(|c| c.accepted_daa),
                accepted_block: claim.map(|c| c.accepted_block),
                phase: claim.map(|c| phase_name(&c.phase)).unwrap_or(""),
                final_daa: claim.and_then(|c| match c.phase {
                    PalwClaimPhaseV2::Final { final_daa } => Some(final_daa),
                    _ => None,
                }),
            })
        })
        .collect();
    // Newest first: by Final DAA where the claim is still kept, else by span; the claim id breaks ties.
    rows.sort_by(|a, b| {
        b.final_daa
            .unwrap_or(0)
            .cmp(&a.final_daa.unwrap_or(0))
            .then(b.span.cmp(&a.span))
            .then(a.claim_id.cmp(&b.claim_id))
    });
    let total = rows.len();
    if limit > 0 {
        rows.truncate(limit);
    }
    (rows, total)
}

/// **The lineage of a granted permit**: the quantum it spends, and through it the claim, from the
/// schedule that granted it and the state's claim table. A lottery permit has no quantum and no claim.
pub fn palw_round_lineage_v1(
    state: &PalwChainStateV2,
    schedule: &crate::palw_execution_lane_v1::PalwExecScheduleV1,
    quantum_id: Hash64,
) -> PalwRoundLineageV1 {
    if quantum_id == Hash64::default() {
        return PalwRoundLineageV1 { quantum_id, claim_id: None, quantum_index: None, class_id: None };
    }
    let quantum = schedule.quanta.iter().find(|q| q.quantum_id == quantum_id);
    PalwRoundLineageV1 {
        quantum_id,
        claim_id: quantum.map(|q| q.final_id),
        quantum_index: quantum.map(|q| q.index),
        class_id: quantum.and_then(|q| state.claim(&q.final_id)).map(|c| c.class_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tx::{TransactionId, TransactionOutpoint};

    fn h(v: u64) -> Hash64 {
        Hash64::from_u64_word(v)
    }
    fn bond(v: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(TransactionOutpoint { transaction_id: TransactionId::from_u64_word(v), index: 0 })
    }
    fn rec(hash: u64, ts: u64, outcome: PalwRoundOutcomeV1) -> PalwRoundBlockRecordV1 {
        PalwRoundBlockRecordV1 { hash: h(hash), daa_score: ts / 1000, timestamp_ms: ts, round: ts / 1000, permit_index: 0, bond: Some(bond(1)), outcome }
    }
    fn exec(claim: u64) -> PalwRoundOutcomeV1 {
        PalwRoundOutcomeV1::Exec(PalwRoundLineageV1 { quantum_id: h(9), claim_id: Some(h(claim)), quantum_index: Some(3), class_id: Some(h(77)) })
    }
    fn refused(r: PalwRoundRefusalV1) -> PalwRoundOutcomeV1 {
        PalwRoundOutcomeV1::Refused(r)
    }

    #[test]
    fn an_empty_lane_is_not_stale_and_has_nothing_to_say() {
        let t = PalwRoundLaneTelemetryV1::default();
        let health = t.health(10_000_000, 3);
        assert!(!health.stale, "a lane that has accepted nothing has no last-accepted time to age");
        assert_eq!((health.accepted_total, health.refused_total, health.refused_since_last_accepted), (0, 0, 0));
        assert!(health.last_accepted.is_none() && health.latest.is_none() && health.top_refusals.is_empty());
    }

    #[test]
    fn an_accepted_block_resets_the_since_last_accepted_count_and_a_refusal_raises_it() {
        let t = PalwRoundLaneTelemetryV1::default();
        t.record(rec(1, 1_000, refused(PalwRoundRefusalV1::PermitNotGranted)));
        t.record(rec(2, 2_000, refused(PalwRoundRefusalV1::PermitNotGranted)));
        assert_eq!(t.health(3_000, 5).refused_since_last_accepted, 2);
        t.record(rec(3, 3_000, exec(5)));
        let after = t.health(3_000, 5);
        assert_eq!(after.refused_since_last_accepted, 0);
        assert_eq!((after.accepted_total, after.refused_total), (1, 2));
        t.record(rec(4, 4_000, refused(PalwRoundRefusalV1::PayoutMismatch)));
        assert_eq!(t.health(4_000, 5).refused_since_last_accepted, 1);
    }

    #[test]
    fn the_top_refusals_are_ordered_by_count_then_name_and_cut_to_top() {
        let t = PalwRoundLaneTelemetryV1::default();
        for i in 0..3 {
            t.record(rec(10 + i, 1_000 + i, refused(PalwRoundRefusalV1::PermitNotGranted)));
        }
        for i in 0..3 {
            t.record(rec(20 + i, 1_000 + i, refused(PalwRoundRefusalV1::HeaderMergesetRule)));
        }
        t.record(rec(30, 1_000, refused(PalwRoundRefusalV1::PermitAlreadyUsed)));
        let top = t.health(2_000, 2).top_refusals;
        assert_eq!(top, vec![("header_mergeset_rule", 3), ("permit_not_granted", 3)], "ties break by name, and the cut is `top`");
    }

    #[test]
    fn the_lane_is_stale_only_once_the_last_accepted_block_is_older_than_the_threshold() {
        let t = PalwRoundLaneTelemetryV1::default();
        t.record(rec(1, 1_000_000, exec(1)));
        assert!(!t.health(1_000_000 + PALW_ROUND_LANE_STALE_AFTER_MS_V1, 1).stale, "exactly at the threshold is not past it");
        assert!(t.health(1_000_001 + PALW_ROUND_LANE_STALE_AFTER_MS_V1, 1).stale);
    }

    #[test]
    fn judging_a_block_again_replaces_its_verdict_and_counts_it_once() {
        let t = PalwRoundLaneTelemetryV1::default();
        t.record(rec(1, 1_000, refused(PalwRoundRefusalV1::PermitNotGranted)));
        t.record(rec(1, 1_000, exec(8)));
        let health = t.health(1_000, 5);
        assert_eq!((health.accepted_total, health.refused_total), (1, 0), "the old refusal is un-counted");
        assert!(health.top_refusals.is_empty());
        assert!(matches!(t.lookup(&h(1)).unwrap().outcome, PalwRoundOutcomeV1::Exec(_)));
    }

    #[test]
    fn the_ledger_is_bounded_and_evicts_the_oldest_first() {
        let t = PalwRoundLaneTelemetryV1::default();
        for i in 0..(PALW_ROUND_LEDGER_CAPACITY_V1 as u64 + 10) {
            t.record(rec(i + 1, 1_000 + i, exec(i)));
        }
        assert!(t.lookup(&h(1)).is_none(), "the oldest record is gone");
        assert!(t.lookup(&h(PALW_ROUND_LEDGER_CAPACITY_V1 as u64 + 10)).is_some(), "the newest is kept");
        assert_eq!(t.lock().by_hash.len(), PALW_ROUND_LEDGER_CAPACITY_V1);
    }

    #[test]
    fn a_header_refusal_keeps_only_the_hash_and_the_reason() {
        let t = PalwRoundLaneTelemetryV1::default();
        t.record_header_refusal(h(5), 5_000, 5, PalwRoundRefusalV1::HeaderSignature);
        let r = t.lookup(&h(5)).unwrap();
        assert!(r.bond.is_none());
        assert_eq!(r.outcome, refused(PalwRoundRefusalV1::HeaderSignature));
        assert_eq!(t.health(5_000, 5).top_refusals, vec![("header_signature_invalid", 1)]);
    }

    #[test]
    fn every_refusal_has_a_distinct_name() {
        use PalwRoundRefusalV1::*;
        let all = [
            HeaderEnvelope, HeaderSignature, HeaderMergesetRule, HeaderAnchorOffChain, HeaderOther, EnvelopeUndecodable,
            SpanOutsideWindow, PermitEquivocated, NoSchedule, PermitNotGranted, BondNotActiveOrKeyMismatch, PayoutMismatch,
            PermitAlreadyUsed,
        ];
        let names: std::collections::BTreeSet<_> = all.iter().map(|r| r.name()).collect();
        assert_eq!(names.len(), all.len());
    }

    fn row(stage: &'static str, tickets: u32, spent: u32, phase: &'static str) -> PalwExecutionRowV1 {
        PalwExecutionRowV1 {
            claim_id: h(1),
            class_id: None,
            executor_bond: bond(1),
            domain: h(2),
            stage,
            credit: 1,
            span: 1,
            tickets,
            tickets_spent: spent,
            first_round: None,
            last_round: None,
            accepted_daa: None,
            accepted_block: None,
            phase,
            final_daa: None,
        }
    }

    #[test]
    fn an_execution_status_reads_the_lane_stage_and_the_tickets() {
        assert_eq!(row("scheduled", 120, 0, "final").status(), "running");
        assert_eq!(row("scheduled", 120, 119, "final").status(), "running");
        assert_eq!(row("scheduled", 120, 120, "final").status(), "complete");
        assert_eq!(row("scheduled", 0, 0, "final").status(), "no_tickets");
        assert_eq!(row("maturing", 0, 0, "final").status(), "maturing");
        assert_eq!(row("credited", 0, 0, "final").status(), "credited");
        assert_eq!(row("scheduled", 120, 3, "voided").status(), "voided", "a convicted claim is voided whatever its tickets say");
    }

    #[test]
    fn a_lottery_permit_has_no_claim_and_no_quantum() {
        let state = PalwChainStateV2::genesis();
        let schedule = crate::palw_execution_lane_v1::PalwExecScheduleV1 {
            span_index: 1,
            seed: h(1),
            domains: vec![],
            finals: vec![],
            quanta: vec![],
        };
        let lineage = palw_round_lineage_v1(&state, &schedule, Hash64::default());
        assert_eq!((lineage.claim_id, lineage.quantum_index, lineage.class_id), (None, None, None));
    }

    #[test]
    fn a_ticket_names_its_final_and_its_index() {
        let state = PalwChainStateV2::genesis();
        let quantum = crate::palw_execution_quanta_v1::PalwExecQuantumV1 {
            quantum_id: h(40),
            final_id: h(41),
            index: 7,
            bond: bond(1),
            operator_id: h(3),
            domain: h(4),
            scheduled_round: 100,
        };
        let schedule = crate::palw_execution_lane_v1::PalwExecScheduleV1 {
            span_index: 1,
            seed: h(1),
            domains: vec![],
            finals: vec![],
            quanta: vec![quantum],
        };
        let lineage = palw_round_lineage_v1(&state, &schedule, h(40));
        assert_eq!((lineage.claim_id, lineage.quantum_index), (Some(h(41)), Some(7)));
        assert_eq!(lineage.class_id, None, "the claim is not in this (empty) state");
        let unknown = palw_round_lineage_v1(&state, &schedule, h(99));
        assert_eq!(unknown.claim_id, None, "a ticket the schedule never issued names no claim");
    }

    #[test]
    fn an_empty_state_holds_no_executions() {
        let state = PalwChainStateV2::genesis();
        let (rows, total) = palw_recent_executions_v1(&state, 10);
        assert!(rows.is_empty());
        assert_eq!(total, 0);
    }

    #[test]
    fn a_round_block_is_exec_or_round_and_its_verdict_never_makes_it_a_genuine_red() {
        let granted = rec(1, 1_000, exec(5));
        let refused_rec = rec(2, 2_000, refused(PalwRoundRefusalV1::PermitNotGranted));
        assert_eq!(palw_round_block_class_v1(Some(&granted)), PalwBlockClassV1::Exec);
        assert_eq!(palw_round_block_class_v1(Some(&refused_rec)), PalwBlockClassV1::Round, "a refused round is never a genuine red");
        assert_eq!(palw_round_block_class_v1(None), PalwBlockClassV1::Round, "no verdict is never guessed");
        assert_eq!(
            [PalwBlockClassV1::Blue, PalwBlockClassV1::Exec, PalwBlockClassV1::Red, PalwBlockClassV1::Round, PalwBlockClassV1::Unmerged].map(|c| c.name()),
            ["BLUE", "EXEC", "RED", "ROUND", ""]
        );
    }
}
