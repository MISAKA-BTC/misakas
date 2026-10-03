//! **ADR-0160 lane S — the issuance slot, its lifetime, and the DoS caps** (testnet-12 only, behind the
//! dormant `Params::palw_capacity_issuance_slots`, F-S; rcore/cap-s1 stage 2).
//!
//! **What it bounds: queues, never safety** (ADR-0160 v3 §6). A bond's claims are admitted only while
//! it holds fewer than `N_out` issuance slots and its token bucket holds a token; both scale with ρ, so
//! at ρ = 100 a 13,000 MSK bond holds at most 200 outstanding claims and issues 10 a DAA (the user's
//! example: 200 outstanding, 10 a DAA; its burst is 10, inside the user's 8–16). Nothing in the liability, the weight cap or the audit door reads these numbers:
//! J-1 bounds weight at any claim count, the audit door refuses every credited fraud individually, and
//! each claim's liability is its own withheld reward plus the pool (§6.3, test S-T6 runs the attack
//! battery with the caps at ∞). What they bound is the length of every queue a claim joins — the
//! panels, the carriage, the anchors, the audits, the state.
//!
//! ```text
//! u_b          = ⌊C_b / 6,500 MSK⌋                           (2 at 13k, 15 at 100k, 153 at 1M)
//! N_out(b, t)  = u_b · ρ(t)                                   outstanding (slot-holding) claims
//! r_b(t)       = u_b · ρ(t) / 20                              refill, claims a DAA (milli-claims: u·ρ·50)
//! B_b(t)       = max(1, r_b(t))                               bucket depth (burst): one DAA's refill
//! tokens_b(t)  = min(1,000·B, tokens(t0) + r_milli · (t − t0)) refilled at use; a claim costs 1,000
//! admit        ⟺ outstanding_b < N_out(b, t)  ∧  tokens_b(t) ≥ 1,000
//! ```
//!
//! **The depth is one DAA's refill (stage 4, rcore/cap-s1), not ADR-0160 v3 §6.1's `max(4, ⌈u·ρ/25⌉)`**,
//! for two reasons found in stages 2 and 4. (1) The fold refills at DAA granularity, so a bond spends at
//! most `B` a DAA: with `B < r` (every row from `u·ρ > 100`) the rate `r` was never reached — 13k at ρ 100
//! issued 8 a DAA, not the example's 10. (2) The user's stage-4 property, "13k × 10 bonds never beats
//! 130k × 1": the floor of 4 let ten 13k bonds burst 40 in a DAA where their 130k whole bursts 8 (ρ 10).
//! `B = r` is linear in `u` (milli-claims), so pieces hold together no more depth and no more refill
//! than their whole (`Σ⌊C_i/6,500⌋ ≤ ⌊ΣC_i/6,500⌋`); its one-claim floor binds only where `u·ρ < 20` —
//! a 13k bond below ρ 10, no capacity step — where it keeps a small bond able to issue at all.
//!
//! **The slot** (S.1, S.5): a bond's new-rule ATTEMPT claims accepted past F-S hold one slot each from
//! acceptance until a COUNTED licence (`basis_k ≥ 2`; an S2 licence keeps it to `Final`), `Final`, a
//! conviction void, or the end of E-4's hold for an unconvicted void (`voided_daa + h_obl`). Never by a
//! producer action. The count is derived from the claims (never hashed) — [`palw_issuance_holds_slot_v1`]
//! is the one predicate.
//!
//! **The bucket** (S.3) is rooted: `issuance_buckets: bond → PalwIssuanceBucketV1 { tokens_milli,
//! last_daa }`, written at each admission past F-S (refilled to the block's DAA, one token spent), hashed
//! only when non-empty. A bond with no bucket row holds a full bucket.

use crate::palw_state_v2::{PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2, PalwVoidReasonV2};

/// Collateral per issuance unit — lane weight's FCW unit (6,500 MSK), so `u_b` is `W_cap`'s count.
pub const PALW_ISSUANCE_COLLATERAL_PER_UNIT_SOMPI_V1: u64 = crate::palw_weight_cap_v1::PALW_CAPACITY_COLLATERAL_PER_FCW_SOMPI_V1;
/// The least bucket depth, in milli-claims (B's floor: one claim).
pub const PALW_ISSUANCE_BURST_FLOOR_MILLI_V1: u64 = 1_000;
/// `r = u·ρ / 20` claims a DAA.
pub const PALW_ISSUANCE_RATE_DIVISOR_V1: u64 = 20;
/// One claim's cost in the bucket's fixed point (milli-claims).
pub const PALW_ISSUANCE_TOKEN_MILLI_V1: u64 = 1_000;

/// **`u_b`** — a bond's issuance units, `⌊C / 6,500 MSK⌋`.
pub fn palw_issuance_units_v1(collateral_sompi: u64) -> u64 {
    collateral_sompi / PALW_ISSUANCE_COLLATERAL_PER_UNIT_SOMPI_V1
}

/// **`N_out = u · ρ`** — the most slot-holding claims a bond may hold (S-I1).
pub fn palw_issuance_outstanding_cap_v1(units: u64, rho: u32) -> u64 {
    units.saturating_mul(u64::from(rho.max(1)))
}

/// **`r` in milli-claims a DAA** — `u · ρ · 1,000 / 20`.
pub fn palw_issuance_rate_milli_v1(units: u64, rho: u32) -> u64 {
    palw_issuance_outstanding_cap_v1(units, rho).saturating_mul(PALW_ISSUANCE_TOKEN_MILLI_V1) / PALW_ISSUANCE_RATE_DIVISOR_V1
}

/// **`B = max(1, r)`** — the bucket's depth in milli-claims: one DAA's refill, at least one claim.
pub fn palw_issuance_burst_milli_v1(units: u64, rho: u32) -> u64 {
    palw_issuance_rate_milli_v1(units, rho).max(PALW_ISSUANCE_BURST_FLOOR_MILLI_V1)
}

/// **A bond's token bucket** (S.3), rooted in `PalwChainStateV2::issuance_buckets`: the tokens (in
/// milli-claims) it held at `last_daa`, the DAA of its last admission.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwIssuanceBucketV1 {
    pub tokens_milli: u64,
    pub last_daa: u64,
}

/// **The bucket's tokens at `now_daa`** (S-I2: a pure function of the row and the params): a bond with
/// no row holds a full bucket; otherwise `min(B, tokens + r_milli · (now − last))` (milli-claims).
pub fn palw_issuance_tokens_at_v1(bucket: Option<&PalwIssuanceBucketV1>, burst_milli: u64, rate_milli: u64, now_daa: u64) -> u64 {
    let full = burst_milli;
    match bucket {
        None => full,
        Some(row) => {
            let elapsed = now_daa.saturating_sub(row.last_daa);
            row.tokens_milli.saturating_add(rate_milli.saturating_mul(elapsed)).min(full)
        }
    }
}

/// Why lane S refuses an admission (S.2: `IssuanceCapped`, non-fatal for the block's own attempt).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwIssuanceRefusalV1 {
    /// The bond holds `outstanding` slots of its `cap` (`N_out`).
    Outstanding { outstanding: u64, cap: u64 },
    /// The bucket holds `tokens_milli` < 1,000 at this DAA.
    Rate { tokens_milli: u64 },
}

/// **One bond's issuance reading at a DAA** — what admission, the fold and the producer's facts ask
/// (S.4: one function at the three sites).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwIssuanceReadV1 {
    pub units: u64,
    pub rho: u32,
    pub outstanding: u64,
    pub cap: u64,
    pub burst_milli: u64,
    pub rate_milli: u64,
    pub tokens_milli: u64,
}

impl PalwIssuanceReadV1 {
    pub fn of_v1(collateral_sompi: u64, rho: u32, outstanding: u64, bucket: Option<&PalwIssuanceBucketV1>, now_daa: u64) -> Self {
        let units = palw_issuance_units_v1(collateral_sompi);
        let (cap, burst_milli, rate_milli) =
            (palw_issuance_outstanding_cap_v1(units, rho), palw_issuance_burst_milli_v1(units, rho), palw_issuance_rate_milli_v1(units, rho));
        Self {
            units,
            rho,
            outstanding,
            cap,
            burst_milli,
            rate_milli,
            tokens_milli: palw_issuance_tokens_at_v1(bucket, burst_milli, rate_milli, now_daa),
        }
    }

    /// **S-I1 and the bucket**: `outstanding < N_out` and a whole token.
    pub fn admits_v1(&self) -> Result<(), PalwIssuanceRefusalV1> {
        if self.outstanding >= self.cap {
            return Err(PalwIssuanceRefusalV1::Outstanding { outstanding: self.outstanding, cap: self.cap });
        }
        if self.tokens_milli < PALW_ISSUANCE_TOKEN_MILLI_V1 {
            return Err(PalwIssuanceRefusalV1::Rate { tokens_milli: self.tokens_milli });
        }
        Ok(())
    }

    /// The row the fold writes when it admits: refilled to `now_daa`, one token spent.
    pub fn spent_v1(&self, now_daa: u64) -> PalwIssuanceBucketV1 {
        PalwIssuanceBucketV1 { tokens_milli: self.tokens_milli.saturating_sub(PALW_ISSUANCE_TOKEN_MILLI_V1), last_daa: now_daa }
    }
}

/// **Does `claim` hold an issuance slot at `now_daa`?** (S.1, S.5.) An ATTEMPT claim accepted at or past
/// F-S's height, from acceptance until: a counted licence (`basis_k ≥ 2`, the room's counted-licensed
/// predicate — an S2 licence keeps its slot), `Final`, a conviction void (`CourtFraud`,
/// `AggregateForfeit`), or the end of E-4's hold for any other void (`voided_daa + h_obl`, the
/// escrow lane's predicate). A pure function of the record, the params and the DAA.
pub fn palw_issuance_holds_slot_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2, now_daa: u64) -> bool {
    if !matches!(claim.source, PalwClaimSourceV2::Attempt) || !params.capacity_slots_applies_at(claim.accepted_daa) {
        return false;
    }
    match &claim.phase {
        PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. } => true,
        PalwClaimPhaseV2::ReceiptLicensed { .. } => !crate::palw_state_v2::palw_rcore_counts_licensed_v1(claim),
        PalwClaimPhaseV2::Final { .. } => false,
        PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::AggregateForfeit, .. } => false,
        PalwClaimPhaseV2::Voided { voided_daa, .. } => now_daa < voided_daa.saturating_add(params.window_receipt()),
    }
}

/// **One bond's issuance reading on `state` at `now_daa`** — the ONE function the fold's `apply_attempt`,
/// admission (`palw_admission_v2`) and the producer's facts ask (S.4), so the three can never disagree:
/// `None` below F-S; otherwise the bond's slot-holding claims (a scan of the claims, [`palw_issuance_holds_slot_v1`]),
/// its collateral, the step's ρ at `now_daa` and its bucket row.
pub fn palw_issuance_read_at_v1(
    state: &crate::palw_state_v2::PalwChainStateV2,
    params: &PalwStateParamsV2,
    bond: &crate::palw_state_v2::PalwBondKeyV2,
    collateral_sompi: u64,
    now_daa: u64,
) -> Option<PalwIssuanceReadV1> {
    if !params.capacity_slots_active_at(now_daa) {
        return None;
    }
    let outstanding = state.claims_iter().filter(|(_, claim)| claim.bond == *bond && palw_issuance_holds_slot_v1(params, claim, now_daa)).count();
    let mut rho = crate::palw_weight_cap_v1::palw_capacity_rho_at_v1(params, now_daa);
    // **ADR-0164 F-K: the issuance tier is `min(schedule ρ, the breaker's level)`** — the breaker only ever lowers it, and a claim in
    // flight keeps the price it was accepted at (only the caps on NEW claims follow the tier).
    if params.capacity_breaker_active_at(now_daa) {
        rho = crate::palw_capacity_s567_v1::palw_breaker_tier_v1(rho, state.rho_breaker_v1());
        // …and the bond's own row lowers its tier alone (B1, B2).
        if let Some(own) = state.bond_breaker_v1(bond) {
            rho = rho.min(crate::palw_capacity_s567_v1::PALW_RHO_LADDER_V1[usize::from(own.level.min(crate::palw_capacity_s567_v1::PALW_BREAKER_TOP_LEVEL_V1))]);
        }
    }
    Some(PalwIssuanceReadV1::of_v1(collateral_sompi, rho, outstanding as u64, state.issuance_bucket_of_v1(bond), now_daa))
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSK: u64 = 100_000_000;

    /// ADR-0160 v3 §6.2 (T5)'s `N_out` and `r`, the user's example at ρ 100 on a 13,000 MSK bond: 200
    /// outstanding, 10 claims a DAA — and a burst of one DAA's refill (10, inside the user's 8–16).
    #[test]
    fn the_caps_are_the_adrs_table_with_one_daa_of_burst() {
        let row = |msk: u64, rho: u32| {
            let u = palw_issuance_units_v1(msk * MSK);
            (palw_issuance_outstanding_cap_v1(u, rho), palw_issuance_burst_milli_v1(u, rho), palw_issuance_rate_milli_v1(u, rho))
        };
        assert_eq!(row(13_000, 100), (200, 10_000, 10_000), "13k at ρ 100: 200 / a burst of 10 / 10 a DAA");
        assert_eq!(row(13_000, 10), (20, 1_000, 1_000), "13k at ρ 10: 20 / 1 / 1 a DAA");
        assert_eq!(row(13_000, 25), (50, 2_500, 2_500), "13k at ρ 25: 2.5 a DAA, carried in milli-claims");
        assert_eq!(row(13_000, 1_000), (2_000, 100_000, 100_000));
        assert_eq!(row(100_000, 10), (150, 7_500, 7_500));
        assert_eq!(row(100_000, 100), (1_500, 75_000, 75_000));
        assert_eq!(row(1_000_000, 100), (15_300, 765_000, 765_000));
        assert_eq!(row(1_000_000, 1_000), (153_000, 7_650_000, 7_650_000));
        assert_eq!(row(13_000, 1), (2, 1_000, 100), "ρ = 1: two slots, a one-claim floor, one claim per ten DAA");
    }

    /// **Split-neutral (the user's stage-4 property):** ten 13k bonds hold no more slots, depth or refill
    /// than one 130k bond, at every capacity step (the one-claim floor binds only below ρ 10 at 13k).
    #[test]
    fn ten_small_bonds_hold_no_more_slots_depth_or_refill_than_their_whole() {
        for rho in [10u32, 25, 50, 100, 250, 500, 1_000] {
            for (small, whole) in [(13_000u64, 130_000u64), (19_000, 190_000), (100_000, 1_000_000)] {
                let piece = PalwIssuanceReadV1::of_v1(small * MSK, rho, 0, None, 7);
                let one = PalwIssuanceReadV1::of_v1(whole * MSK, rho, 0, None, 7);
                assert!(10 * piece.cap <= one.cap, "N_out at ρ {rho}: {small}");
                assert!(10 * piece.burst_milli <= one.burst_milli, "B at ρ {rho}: {small}");
                assert!(10 * piece.rate_milli <= one.rate_milli, "r at ρ {rho}: {small}");
            }
        }
    }

    /// The bucket: a missing row is full; a spend costs one token; the refill is linear and capped.
    #[test]
    fn the_bucket_refills_linearly_and_caps_at_its_depth() {
        let read = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 0, None, 50);
        assert_eq!(read.tokens_milli, 10_000, "a bond with no row holds a full bucket");
        let mut row = read.spent_v1(50);
        assert_eq!(row, PalwIssuanceBucketV1 { tokens_milli: 9_000, last_daa: 50 });
        for _ in 0..9 {
            let r = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 0, Some(&row), 50);
            r.admits_v1().expect("the burst");
            row = r.spent_v1(50);
        }
        let dry = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 0, Some(&row), 50);
        assert_eq!(dry.admits_v1(), Err(PalwIssuanceRefusalV1::Rate { tokens_milli: 0 }), "the burst is spent within a DAA");
        let later = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 0, Some(&row), 51);
        assert_eq!(later.tokens_milli, 10_000, "a DAA refills 10 claims: the rate is reachable");
        let full = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 0, Some(&row), 10_000);
        assert_eq!(full.tokens_milli, 10_000, "capped at one DAA's refill");
        let capped = PalwIssuanceReadV1::of_v1(13_000 * MSK, 100, 200, None, 51);
        assert_eq!(capped.admits_v1(), Err(PalwIssuanceRefusalV1::Outstanding { outstanding: 200, cap: 200 }), "S-I1");
    }
}
