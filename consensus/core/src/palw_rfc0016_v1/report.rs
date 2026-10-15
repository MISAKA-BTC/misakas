//! **The per-model report and its outcome rule** (RFC-0016 §11, user ruling R6 of 2026-10-15).
//!
//! The report keeps TARGETS (`P_m`, issued rights) apart from OUTCOMES (physical blocks, rights consumed, Finals, actual payout,
//! per-miner revenue, refusals, queue wait, REAL gap / heartbeat-dependent time). [`palw_report_outcome_v1`] is the one place a
//! comparison of two reports becomes a claim: an improvement is claimed only from a physical-block or payout increase, never from a
//! target alone ([`PalwReportClaimV1::TargetOnly`]). `None` fields are UNKNOWN (not consensus-derivable here) and never count as a
//! change in either direction.

use crate::Hash64;
use crate::palw_state_v2::PalwBondKeyV2;

/// One model's row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwModelReportRowV1 {
    pub model: Hash64,
    pub epoch_index: u64,
    // ---- targets (NOT outcomes) ----
    /// `P_m` in parts per million of the allocation (`A_m / ΣA`); `None` with no allocation epoch.
    pub target_p_ppm: Option<u64>,
    /// V+ rights issued this epoch (`Rt_m`, entrant grant included).
    pub rights_issued: u64,
    // ---- outcomes ----
    /// Physical model blocks this epoch (accepted model-class Attempts; the state's epoch counter).
    pub physical_blocks: u64,
    /// Rights consumed (under V+ every accepted model block consumes exactly one).
    pub rights_consumed: u64,
    /// Rights issued and never consumed by the epoch's end (`unused_rights_m`; expire unminted).
    pub rights_unused: u64,
    /// Claims of the model in `Final` (all epochs), consensus-derived.
    pub finals: Option<u64>,
    /// Coinbase-derived reward actually consumed by the model's claims (the budget engine's ledger); `None` without the engine.
    pub payout_coinbase_sompi: Option<u128>,
    /// User-paid escrow paid to the model's kernel claims this epoch (consensus-derived, aux 135; kept apart from coinbase, never
    /// summed with it); `None` below the fences.
    pub payout_escrow_sompi: Option<u128>,
    /// **NODE-LOCAL observations, not consensus** (`palw_rfc0016_v1::metrics`): refusals this process saw for capacity / for rights.
    pub refused_for_capacity: Option<u64>,
    pub refused_for_rights: Option<u64>,
    /// Pending-queue wait of the claims that left the queue this epoch: mean and max DAA (consensus-derived, aux 135).
    pub queue_wait_mean_daa: Option<u64>,
    pub queue_wait_max_daa: Option<u64>,
    /// Claims waiting in the model's queues now (consensus-derived).
    pub queue_len: u64,
    /// DAA since the last BLUE REAL block while REAL work flows (the floor state's `Normal { last_blue }`); `None` when the floor is
    /// Idle (no REAL work flowing) or below the fences.
    pub real_gap_daa: Option<u64>,
    /// DAA this epoch during which the floor was Idle — the clock carried by the floor and the heartbeat (consensus-derived, aux 135).
    pub heartbeat_dependent_daa: Option<u64>,
    /// **V9 (user decision 2026-10-15), measured SEPARATELY:** of the model's claims that ended this epoch, the share a verifier
    /// actually CHECKED (a verdict, a court or a proven fault — not a timeout)…
    pub checked_share_permille: Option<u32>,
    /// …and the share that COMPLETED verification within its deadline (`Final`). A checked claim voided by a court is checked but not
    /// completed; a timeout is neither.
    pub completed_within_deadline_permille: Option<u32>,
    /// Elastic capacity per span and the shortfall, where the capacity fence reads them.
    pub capacity_per_span: Option<u128>,
    pub capacity_shortfall_per_span: Option<u128>,
}

/// One existing miner's revenue from one model (the budget engine's consumed reward of that bond's claims).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwMinerRevenueRowV1 {
    pub bond: PalwBondKeyV2,
    pub model: Hash64,
    pub payout_coinbase_sompi: u128,
    pub claims: u64,
}

/// **V5 (user decision 2026-10-15): the epoch clock the report carries** — an epoch is a DAA count, and its wall-clock span is
/// measured, never assumed: the report gives the epoch's DAA terms, the nominal slot and the sink block's own timestamp, so a reader
/// (the measurement harness) reports DAA versus wall-clock drift. No field or label says "24 h".
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PalwRfc0016EpochClockV1 {
    pub epoch_daa: u64,
    pub epoch_index: u64,
    pub epoch_start_daa: u64,
    /// The protocol's nominal clock slot (120 s); `epoch_daa · nominal_ms_per_daa` is the NOMINAL span, never the measured one.
    pub nominal_ms_per_daa: u64,
    /// The sink block's header timestamp (ms), read by the node serving the report; `None` where unavailable.
    pub sink_timestamp_ms: Option<u64>,
}

impl PalwRfc0016EpochClockV1 {
    pub fn at(epoch_daa: u64, now_daa: u64, sink_timestamp_ms: Option<u64>) -> Self {
        let epoch_daa = epoch_daa.max(1);
        let epoch_index = now_daa / epoch_daa;
        Self {
            epoch_daa,
            epoch_index,
            epoch_start_daa: epoch_index.saturating_mul(epoch_daa),
            nominal_ms_per_daa: crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS,
            sink_timestamp_ms,
        }
    }

    /// The label a reader shows: a DAA count, with the wall-clock span stated as measured.
    pub fn label(&self) -> String {
        format!("epoch = {} DAA (wall-clock span measured from DAA vs time; never assumed to be 24 h)", self.epoch_daa)
    }

    pub fn json(&self) -> String {
        format!(
            concat!(
                "{{\"epochDaa\":\"{}\",\"epochIndex\":\"{}\",\"epochStartDaa\":\"{}\",\"nominalMsPerDaa\":\"{}\",",
                "\"sinkTimestampMs\":{},\"label\":\"{}\"}}"
            ),
            self.epoch_daa,
            self.epoch_index,
            self.epoch_start_daa,
            self.nominal_ms_per_daa,
            json_opt(self.sink_timestamp_ms),
            self.label()
        )
    }
}

/// [`palw_rfc0016_report_json_v1`] with the V5 epoch clock appended (`"epochClock"`).
pub fn palw_rfc0016_report_json_with_clock_v1(
    now_daa: u64,
    armed: bool,
    rows: &[PalwModelReportRowV1],
    miners: &[PalwMinerRevenueRowV1],
    quote: Option<&super::capacity::PalwAdmissionQuoteV1>,
    clock: &PalwRfc0016EpochClockV1,
) -> String {
    let body = palw_rfc0016_report_json_v1(now_daa, armed, rows, miners, quote);
    match body.strip_suffix('}') {
        Some(open) => format!("{open},\"epochClock\":{}}}", clock.json()),
        None => body,
    }
}

/// **V9: how an ended claim counts** toward `checked_share` and the within-deadline completion rate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVerificationOutcomeV1 {
    /// Checked and completed within its deadline (`Final`).
    CheckedCompleted,
    /// Checked but not completed (a court, a proven fault, a forfeiture).
    CheckedNotCompleted,
    /// Never checked: its deadline passed without a verdict (a timeout, no capable panel, unavailable verifiers, …).
    NotChecked,
}

/// **V9: the outcome of a claim phase** — `None` while the claim is live. Unknown future void reasons count as `NotChecked` (the
/// conservative side: a reason not known to carry a verdict never raises `checked_share`).
pub fn palw_rfc0016_verification_outcome_v1(phase: &crate::palw_state_v2::PalwClaimPhaseV2) -> Option<PalwVerificationOutcomeV1> {
    use crate::palw_state_v2::{PalwClaimPhaseV2, PalwVoidReasonV2};
    match phase {
        PalwClaimPhaseV2::Final { .. } => Some(PalwVerificationOutcomeV1::CheckedCompleted),
        PalwClaimPhaseV2::Voided { reason, .. } => Some(match reason {
            PalwVoidReasonV2::CourtFraud
            | PalwVoidReasonV2::CourtDefault
            | PalwVoidReasonV2::CourtHeldVerdict
            | PalwVoidReasonV2::ProducerWithholding
            | PalwVoidReasonV2::AggregateForfeit
            | PalwVoidReasonV2::WorkSliceProvenFalse
            | PalwVoidReasonV2::WorkSliceDefaulted => PalwVerificationOutcomeV1::CheckedNotCompleted,
            _ => PalwVerificationOutcomeV1::NotChecked,
        }),
        _ => None,
    }
}

/// **V9: `(checked_share, completed_within_deadline)` in permille** over `outcomes` (`None` with no ended claim: UNKNOWN, never 0 or
/// 1000).
pub fn palw_rfc0016_verification_shares_v1(outcomes: &[PalwVerificationOutcomeV1]) -> (Option<u32>, Option<u32>) {
    if outcomes.is_empty() {
        return (None, None);
    }
    let n = outcomes.len() as u64;
    let checked = outcomes.iter().filter(|o| !matches!(o, PalwVerificationOutcomeV1::NotChecked)).count() as u64;
    let completed = outcomes.iter().filter(|o| matches!(o, PalwVerificationOutcomeV1::CheckedCompleted)).count() as u64;
    (Some((checked * 1_000 / n) as u32), Some((completed * 1_000 / n) as u32))
}

/// What a before/after comparison may claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwReportClaimV1 {
    /// Physical blocks or payout rose (and neither fell).
    Improved { physical_delta: i128, payout_delta: i128 },
    /// An outcome fell.
    Worsened { physical_delta: i128, payout_delta: i128 },
    /// Outcomes unchanged (or UNKNOWN) while the target moved — reported as a target change, NEVER as an improvement.
    TargetOnly { target_delta_ppm: i128 },
    /// Nothing measurable changed.
    Unchanged,
}

fn delta(before: Option<u128>, after: Option<u128>) -> Option<i128> {
    match (before, after) {
        (Some(b), Some(a)) => Some(a as i128 - b as i128),
        _ => None,
    }
}

/// **The outcome rule.** Physical blocks and coinbase payout are the outcomes; the target is reported only as a target.
pub fn palw_report_outcome_v1(before: &PalwModelReportRowV1, after: &PalwModelReportRowV1) -> PalwReportClaimV1 {
    let physical = after.physical_blocks as i128 - before.physical_blocks as i128;
    let payout = delta(before.payout_coinbase_sompi, after.payout_coinbase_sompi).unwrap_or(0);
    if physical < 0 || payout < 0 {
        return PalwReportClaimV1::Worsened { physical_delta: physical, payout_delta: payout };
    }
    if physical > 0 || payout > 0 {
        return PalwReportClaimV1::Improved { physical_delta: physical, payout_delta: payout };
    }
    let target = delta(before.target_p_ppm.map(u128::from), after.target_p_ppm.map(u128::from)).unwrap_or(0);
    if target != 0 { PalwReportClaimV1::TargetOnly { target_delta_ppm: target } } else { PalwReportClaimV1::Unchanged }
}

fn json_opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map(|x| format!("\"{x}\"")).unwrap_or_else(|| "null".to_string())
}

/// **The report as JSON** (RPC op 255's payload): `targets` and `outcomes` in separate objects per model, the node-local observations
/// under `node_local` (never outcomes), integers as decimal strings (u128-safe). `quote`, where asked, is the pre-lottery quote's
/// Debug rendering (advisory).
pub fn palw_rfc0016_report_json_v1(
    now_daa: u64,
    armed: bool,
    rows: &[PalwModelReportRowV1],
    miners: &[PalwMinerRevenueRowV1],
    quote: Option<&super::capacity::PalwAdmissionQuoteV1>,
) -> String {
    let models: Vec<String> = rows
        .iter()
        .map(|r| {
            format!(
                concat!(
                    "{{\"model\":\"{}\",\"epoch\":\"{}\",",
                    "\"targets\":{{\"pPpm\":{},\"rightsIssued\":\"{}\"}},",
                    "\"outcomes\":{{\"physicalBlocks\":\"{}\",\"rightsConsumed\":\"{}\",\"rightsUnused\":\"{}\",\"finals\":{},",
                    "\"payoutCoinbaseSompi\":{},\"payoutEscrowSompi\":{},\"queueLen\":\"{}\",\"queueWaitMeanDaa\":{},",
                    "\"queueWaitMaxDaa\":{},\"realGapDaa\":{},\"heartbeatDependentDaa\":{},",
                    "\"checkedSharePermille\":{},\"completedWithinDeadlinePermille\":{}}},",
                    "\"nodeLocal\":{{\"refusedForCapacity\":{},\"refusedForRights\":{}}}}}"
                ),
                r.model,
                r.epoch_index,
                json_opt(r.target_p_ppm),
                r.rights_issued,
                r.physical_blocks,
                r.rights_consumed,
                r.rights_unused,
                json_opt(r.finals),
                json_opt(r.payout_coinbase_sompi),
                json_opt(r.payout_escrow_sompi),
                r.queue_len,
                json_opt(r.queue_wait_mean_daa),
                json_opt(r.queue_wait_max_daa),
                json_opt(r.real_gap_daa),
                json_opt(r.heartbeat_dependent_daa),
                json_opt(r.checked_share_permille),
                json_opt(r.completed_within_deadline_permille),
                json_opt(r.refused_for_capacity),
                json_opt(r.refused_for_rights),
            )
        })
        .collect();
    let miners: Vec<String> = miners
        .iter()
        .map(|m| {
            format!(
                "{{\"bond\":\"{}:{}\",\"model\":\"{}\",\"payoutCoinbaseSompi\":\"{}\",\"claims\":\"{}\"}}",
                m.bond.0.transaction_id, m.bond.0.index, m.model, m.payout_coinbase_sompi, m.claims
            )
        })
        .collect();
    let quote = quote
        .map(|q| format!("{q:?}").replace('\\', "\\\\").replace('"', "\\\""))
        .map(|q| format!("\"{q}\""))
        .unwrap_or_else(|| "null".into());
    format!(
        "{{\"nowDaa\":\"{now_daa}\",\"armed\":{armed},\"models\":[{}],\"existingMiners\":[{}],\"quote\":{quote}}}",
        models.join(","),
        miners.join(",")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(target: u64, physical: u64, payout: u128) -> PalwModelReportRowV1 {
        PalwModelReportRowV1 {
            target_p_ppm: Some(target),
            physical_blocks: physical,
            payout_coinbase_sompi: Some(payout),
            ..Default::default()
        }
    }

    /// V9: `checked_share` and the within-deadline completion are measured apart (a court void is checked, not completed).
    #[test]
    fn v9_checked_share_and_completion_are_separate() {
        use PalwVerificationOutcomeV1::*;
        assert_eq!(palw_rfc0016_verification_shares_v1(&[]), (None, None), "UNKNOWN, never a number");
        let (checked, completed) =
            palw_rfc0016_verification_shares_v1(&[CheckedCompleted, CheckedNotCompleted, NotChecked, CheckedCompleted]);
        assert_eq!((checked, completed), (Some(750), Some(500)));
        let final_phase = crate::palw_state_v2::PalwClaimPhaseV2::Final { final_daa: 1 };
        assert_eq!(palw_rfc0016_verification_outcome_v1(&final_phase), Some(CheckedCompleted));
        let timeout = crate::palw_state_v2::PalwClaimPhaseV2::Voided {
            voided_daa: 1,
            reason: crate::palw_state_v2::PalwVoidReasonV2::BindTimeout,
        };
        assert_eq!(palw_rfc0016_verification_outcome_v1(&timeout), Some(NotChecked));
        assert_eq!(palw_rfc0016_verification_outcome_v1(&crate::palw_state_v2::PalwClaimPhaseV2::Provisional), None);
    }

    /// V5: the epoch clock is a DAA count with the measured wall-clock span; nothing asserts 24 h.
    #[test]
    fn v5_the_epoch_clock_never_says_24_hours() {
        let clock = PalwRfc0016EpochClockV1::at(720, 1_500, Some(1_700_000_000_000));
        assert_eq!((clock.epoch_index, clock.epoch_start_daa), (2, 1_440));
        assert!(clock.label().contains("never assumed to be 24 h"));
        let json = palw_rfc0016_report_json_with_clock_v1(1_500, false, &[], &[], None, &clock);
        assert!(json.ends_with("}}") && json.contains("\"epochClock\":{\"epochDaa\":\"720\""));
        assert!(!json.contains("\"epochHours\""), "no wall-clock length is asserted");
    }

    /// R6: a target increase alone is never reported as an improvement (T2 without compute).
    #[test]
    fn a_target_increase_alone_is_not_an_improvement() {
        assert_eq!(
            palw_report_outcome_v1(&row(100, 5, 50), &row(300, 5, 50)),
            PalwReportClaimV1::TargetOnly { target_delta_ppm: 200 }
        );
        assert_eq!(
            palw_report_outcome_v1(&row(100, 5, 50), &row(300, 8, 50)),
            PalwReportClaimV1::Improved { physical_delta: 3, payout_delta: 0 }
        );
        assert_eq!(
            palw_report_outcome_v1(&row(100, 5, 50), &row(300, 4, 80)),
            PalwReportClaimV1::Worsened { physical_delta: -1, payout_delta: 30 }
        );
        let mut unknown = row(300, 5, 0);
        unknown.payout_coinbase_sompi = None;
        assert_eq!(palw_report_outcome_v1(&row(100, 5, 50), &unknown), PalwReportClaimV1::TargetOnly { target_delta_ppm: 200 });
    }
}
