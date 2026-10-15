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
    /// User-paid escrow paid to the model's kernel claims; UNKNOWN in this reader (kept apart from coinbase, never summed with it).
    pub payout_escrow_sompi: Option<u128>,
    /// Attempts refused for capacity / rights (node counters, not consensus state): UNKNOWN here.
    pub refused_for_capacity: Option<u64>,
    pub refused_for_rights: Option<u64>,
    /// Pending-queue wait (DAA) median and p95; UNKNOWN until the queue is wired.
    pub queue_wait_p50_daa: Option<u64>,
    pub queue_wait_p95_daa: Option<u64>,
    /// DAA since the network's last REAL (model) block, and DAA the clock depended on the heartbeat/floor; UNKNOWN in this reader.
    pub real_gap_daa: Option<u64>,
    pub heartbeat_dependent_daa: Option<u64>,
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
