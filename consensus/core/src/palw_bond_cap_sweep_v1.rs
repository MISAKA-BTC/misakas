//! **ADR-0176 cap derivation — the Stage 2 / Stage 3 harness** (ユーザー方針 2026-10-15 item 8; ユーザー判断 2026-10-15 item 7).
//!
//! A deterministic simulator that runs the real budget engine (`PalwBondBudgetStateV1`, a V3 policy: the 1-day / 7-day holds, the
//! burst view, the concurrent-liability cap) under a **measured honest profile** (Stage 1, `scripts/drills/adr0176_stage1_measure.py`)
//! and a **candidate cap setting**, and classifies the candidate:
//!
//! | region | meaning |
//! |---|---|
//! | `TooLow` | the honest miner runs out of cap: REAL stops only for lack of cap while compute, demand and verification exist |
//! | `CollateralBound` | the per-claim reservation × concurrent claims exceeds `φ·C`: collateral, not the cap, stops the honest miner |
//! | `UnderCollectable` | the per-claim reservation is below the collectable loss its rights require |
//! | `VerificationShort` | the profile's Final capacity is below the accepted rate: a Final backlog grows |
//! | `TooHigh` | the honest miner uses less than the utilization floor of its cap: only a faster (forging) producer could use the rest |
//! | `Feasible` | none of the above |
//!
//! It also runs the **same bond as a fast forger** (one submission every DAA) and reports its extra rights — which must be 0 beyond
//! the honest miner's own unused cap (the caps are equal; forging never buys more).
//!
//! **This module decides nothing.** It is the instrument the plan's Stage 2 sweep runs in the validation phase; production values
//! come from measured profiles and adversarial tests, never from the fixture profiles in its tests. If no candidate is `Feasible`,
//! the report names the constraints that conflict instead of a number.

use std::collections::BTreeMap;

use crate::Hash64;
use crate::palw_bond_budget_v1::*;
use crate::palw_state_v2::PalwBondKeyV2;

/// **A measured honest profile** (Stage 1): one reference miner on one bond.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwHonestProfileV1 {
    /// The verification mode the profile was measured under.
    pub mode: PalwHoldModeV1,
    /// The minimum producer bond of the target ruleset, base units.
    pub capital_sompi: u64,
    /// The DAA between two accepted claims the reference hardware sustains (compute + lottery + submission), as measured: the
    /// simulator cycles through it (a measured sequence, not a distribution guess).
    pub claim_intervals_daa: Vec<u64>,
    /// DAA from acceptance to Final (verification wait), cycled likewise.
    pub final_delays_daa: Vec<u64>,
    /// Finals per day the verification side sustained (0: unbounded / not measured).
    pub finals_per_day: u64,
    /// The reward and Final weight one claim asks.
    pub reward_per_claim_sompi: u64,
    pub final_weight_per_claim: u128,
}

/// **One candidate setting** — the tuning targets of guidance item 1 (per-period Q/B/R/F, per-claim reservation, concurrent-liability
/// cap, burst), per capital unit per DAY.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapCandidateV1 {
    pub capital_unit_sompi: u64,
    pub claims_per_unit_day: u64,
    pub block_units_per_unit_day: u64,
    pub reward_per_unit_day_sompi: u64,
    pub final_weight_per_unit_day: u64,
    pub reservation_per_claim_sompi: u64,
    pub collectable_loss_per_claim_sompi: u64,
    pub concurrent_liability_permille: u16,
    pub burst_factor_permille: u32,
}

/// The classification of a candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum PalwCapRegionV1 {
    TooLow,
    CollateralBound,
    UnderCollectable,
    VerificationShort,
    TooHigh,
    Feasible,
}

/// **The simulator's report for one candidate** — the measurements the plan asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwCapSimReportV1 {
    pub days: u64,
    pub honest_accepted: u64,
    /// Submissions the honest miner had ready that the CAP refused (Q/B/R/F/day/burst views).
    pub honest_refused_for_cap: u64,
    /// Submissions refused for collateral (the concurrent-liability cap).
    pub honest_refused_for_collateral: u64,
    /// DAA during which REAL production stood still only for lack of cap (compute + demand + verification present).
    pub cap_stall_daa: u64,
    /// The longest gap between two accepted honest claims, DAA.
    pub max_real_gap_daa: u64,
    /// Honest accepted claims over the period cap's claims (‰).
    pub utilization_permille: u64,
    /// The peak open liability (≈ λ·a·H in steady state).
    pub peak_open_liability_sompi: u128,
    /// The same bond forging (one submission per DAA): its accepted claims, and how many more than the honest miner.
    pub forger_accepted: u64,
    pub forger_extra_over_cap: u64,
    pub region: PalwCapRegionV1,
}

/// The utilization floor below which a cap is `TooHigh` (‰) — a measurement threshold, POLICY.
pub const PALW_CAP_SWEEP_UTILIZATION_FLOOR_PERMILLE_V1: u64 = 800;

fn bond_key(n: u8) -> PalwBondKeyV2 {
    PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
}

fn claim_id(lane: u8, n: u64) -> Hash64 {
    let mut b = [0u8; 64];
    b[0] = lane;
    b[8..16].copy_from_slice(&n.to_le_bytes());
    Hash64::from_bytes(b)
}

/// The V3 budget policy a candidate stands for.
pub fn palw_cap_candidate_policy_v1(c: &PalwCapCandidateV1) -> PalwBondBudgetPolicyV1 {
    let mut p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
    p.capital_unit_sompi = c.capital_unit_sompi.max(1);
    p.claims_per_unit = c.claims_per_unit_day.max(1);
    p.block_units_per_unit = c.block_units_per_unit_day.max(1);
    p.reward_per_unit_sompi = c.reward_per_unit_day_sompi.max(1);
    p.final_weight_per_unit = c.final_weight_per_unit_day.max(1);
    p.max_open_claims_per_bond = u32::MAX;
    p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1 };
    if let Some(hold) = p.hold.as_mut() {
        hold.concurrent_liability_permille = c.concurrent_liability_permille.clamp(1, 1_000);
        hold.burst_factor_permille = c.burst_factor_permille.max(1_000);
    }
    p
}

/// **Simulate one candidate** over `days` day-equivalents.
pub fn palw_bond_cap_simulate_v1(profile: &PalwHonestProfileV1, candidate: &PalwCapCandidateV1, days: u64) -> PalwCapSimReportV1 {
    let policy = palw_cap_candidate_policy_v1(candidate);
    let hold = policy.hold.expect("a V3 policy");
    let end = days.saturating_mul(hold.daa_per_day).max(1);
    let ask = PalwBudgetAskV1 {
        block_units: PALW_BUDGET_BLOCK_UNIT_V1,
        reward_sompi: profile.reward_per_claim_sompi,
        final_weight: profile.final_weight_per_claim,
        liability_sompi: candidate.reservation_per_claim_sompi as u128,
    };
    let origin = match profile.mode {
        PalwHoldModeV1::PanelZero => PalwBudgetOriginV1::KernelRoute,
        PalwHoldModeV1::PanelOne => PalwBudgetOriginV1::Attempt,
    };
    let interval =
        |k: usize| profile.claim_intervals_daa.get(k % profile.claim_intervals_daa.len().max(1)).copied().unwrap_or(1).max(1);
    let delay = |k: usize| profile.final_delays_daa.get(k % profile.final_delays_daa.len().max(1)).copied().unwrap_or(1);

    // The honest miner: a claim is ready at `next_ready`; refused → it retries every DAA (the stall is measured), never discarded.
    let mut honest = PalwBondBudgetStateV1::new(0, &policy);
    let mut finals: BTreeMap<u64, Vec<Hash64>> = BTreeMap::new();
    let (mut accepted, mut refused_cap, mut refused_collateral, mut stall, mut max_gap, mut peak) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u128);
    let mut next_ready = interval(0);
    let mut last_accept = 0u64;
    let mut k = 0usize;
    // The forger: same bond capital, same mode, a submission every DAA, Final at once (a colluding Valid).
    let mut forger = PalwBondBudgetStateV1::new(0, &policy);
    let mut forger_accepted = 0u64;
    for now in 1..=end {
        let mut j = Vec::new();
        honest.release_due(now, &mut j);
        forger.release_due(now, &mut j);
        for id in finals.remove(&now).unwrap_or_default() {
            honest.close_at(&id, Some(now.saturating_add(policy.liability_hold_daa)), &mut j);
        }
        if now >= next_ready {
            let id = claim_id(1, k as u64);
            match honest.reserve_claim_moded_v1(
                &policy,
                profile.capital_sompi,
                id,
                bond_key(1),
                None,
                now,
                ask,
                origin,
                profile.mode,
                &mut j,
            ) {
                Ok(_) => {
                    accepted += 1;
                    max_gap = max_gap.max(now - last_accept);
                    last_accept = now;
                    finals.entry(now.saturating_add(delay(k))).or_default().push(id);
                    k += 1;
                    next_ready = now.saturating_add(interval(k));
                }
                Err(PalwBudgetRefusalV1::LiabilityExceedsCapital) => {
                    refused_collateral += 1;
                }
                Err(_) => {
                    refused_cap += 1;
                    stall += 1;
                }
            }
            let open = honest.bond_row(&bond_key(1)).map_or(0, |r| r.open_liability_sompi);
            peak = peak.max(open);
        }
        let fid = claim_id(2, now);
        if forger
            .reserve_claim_moded_v1(&policy, profile.capital_sompi, fid, bond_key(2), None, now, ask, origin, profile.mode, &mut j)
            .is_ok()
        {
            forger_accepted += 1;
            forger.close_at(&fid, Some(now.saturating_add(policy.liability_hold_daa)), &mut j);
        }
    }
    let period_claims = palw_bond_hold_period_caps_v1(&policy, &hold, profile.capital_sompi, profile.mode).claims;
    let windows = days.div_ceil(profile.mode.days(&hold)).max(1);
    let cap_total = period_claims.saturating_mul(windows).max(1);
    let utilization = accepted.saturating_mul(1_000) / cap_total;
    let accepted_per_day = accepted / days.max(1);
    let region = if refused_cap > 0 {
        PalwCapRegionV1::TooLow
    } else if refused_collateral > 0 {
        PalwCapRegionV1::CollateralBound
    } else if candidate.reservation_per_claim_sompi < candidate.collectable_loss_per_claim_sompi {
        PalwCapRegionV1::UnderCollectable
    } else if profile.finals_per_day > 0 && accepted_per_day > profile.finals_per_day {
        PalwCapRegionV1::VerificationShort
    } else if utilization < PALW_CAP_SWEEP_UTILIZATION_FLOOR_PERMILLE_V1 {
        PalwCapRegionV1::TooHigh
    } else {
        PalwCapRegionV1::Feasible
    };
    PalwCapSimReportV1 {
        days,
        honest_accepted: accepted,
        honest_refused_for_cap: refused_cap,
        honest_refused_for_collateral: refused_collateral,
        cap_stall_daa: stall,
        max_real_gap_daa: max_gap,
        utilization_permille: utilization,
        peak_open_liability_sompi: peak,
        forger_accepted,
        forger_extra_over_cap: forger_accepted.saturating_sub(cap_total),
        region,
    }
}

/// **The Stage 2 sweep**: every candidate, simulated and classified; and, when none is `Feasible`, the regions that conflict.
pub fn palw_bond_cap_sweep_v1(
    profile: &PalwHonestProfileV1,
    candidates: &[PalwCapCandidateV1],
    days: u64,
) -> (Vec<(PalwCapCandidateV1, PalwCapSimReportV1)>, Option<Vec<PalwCapRegionV1>>) {
    let rows: Vec<_> = candidates.iter().map(|c| (c.clone(), palw_bond_cap_simulate_v1(profile, c, days))).collect();
    let feasible = rows.iter().any(|(_, r)| r.region == PalwCapRegionV1::Feasible);
    let conflicts = (!feasible).then(|| {
        let mut regions: Vec<_> = rows.iter().map(|(_, r)| r.region).collect();
        regions.sort();
        regions.dedup();
        regions
    });
    (rows, conflicts)
}
