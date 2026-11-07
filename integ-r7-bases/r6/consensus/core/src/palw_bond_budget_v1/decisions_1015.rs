//! **ADR-0176 / ADR-0177 — the user's rulings of 2026-10-15 (C1–C5) on the budget engine** (DORMANT; no height; every value a
//! calibration starting point, never a production value). Binding text: `docs/history/decisions/2026-10/user-decisions-2026-10-15-adr-rfc16-rfc17.md`
//! §1.
//!
//! ```text
//! C1  newly issued miner-facing coinbase reward never bypasses the per-bond R cap on any path: past a V3 budget the WHOLE
//!     subsidy-derived worker base of an attempt-lane carrier is withheld; the claim's escrow (its R reservation) is the only subsidy
//!     ever paid for it, the rest is never minted, and a budget-refused (skipped) attempt is paid no subsidy at all. Transaction fees
//!     are user-paid, so their worker shares stay outside R (separate accounting).
//! C2  the exit (withdrawal) hold stays 7-day-equivalent DAA for every bond, a Panel=1-only bond included; Panel=1's ISSUANCE
//!     capacity recovers after 1 day, but exit, re-registration or a mode switch never resets a long liability or capacity: the long
//!     view (every reservation, 7 days), the liability view (terminal + H_L) and the liability hold only ever extend
//! C3  Round rights and the monetary income rows get rate / burst caps BESIDE the 7-day total: they write the bond's day and burst
//!     shadows like claims (one short-period ledger per bond across every path, so one principal is never used twice) and are admitted
//!     against the day view at a named per-day rate and against the burst view. The rates are calibration starting points.
//! C4b the coinbase-derived per-claim ceiling `r_c` never binds user-paid escrow (`palw_bond_budget_escrow_reservation_v1`); the
//!     producer's common `R` cap and the required collateral still do, and an insufficient claim is refused BEFORE acceptance, never
//!     reduced after
//! C5  verifier capital counts toward a model's allocation `S_m` only where its verification liability, hold period and slashable
//!     scope are defined: a Panel seat's exposure locked behind a LIVE claim (liability: the false-Valid conviction; hold: the seat
//!     lock until the claim resolves; scope: that exposure). Capital merely bonded for Panel=0 / OPV counts 0 until OPV's
//!     verifier-liability accounting exists
//! V10 (RFC-0016, user decision 2026-10-15) a subsidy the verification-attestation gate withholds is never redistributed — not to
//!     another model, not to an uncertified claim — it is unminted for its epoch and never re-issued later: the epoch's accrual
//!     restarts at 0 at every roll and a withheld claim's model reservation is never released to the others
//! ```
//!
//! Nothing in this module is built or run by the lane that wrote it (a separate validation agent compiles). No line is a PASS.

use super::*;

/// **C1 (user decision 2026-10-15): does `R` cover the attempt-lane worker base at `paying_daa`?** `true` exactly where the bond
/// budget is in force with a V3 (mode-aware hold) policy — the only armable form outside the isolated calibration's mechanics probe,
/// which keeps the old coinbase (that probe budgets mechanics, never payment). `false` on every shipped network (no mirror).
pub fn palw_r_cap_covers_worker_base_v1(mirror: Option<&PalwBondBudgetMirrorV1>, paying_daa: u64) -> bool {
    mirror.is_some_and(|m| m.active_at(paying_daa) && m.policy.hold.is_some())
}

/// **C1: the amount withheld from an attempt-lane carrier's worker output** where [`palw_r_cap_covers_worker_base_v1`] holds:
/// `max(recorded, worker_base)` — `recorded` is what the claim escrowed (its carve, inside the claim's `R` reservation; 0 for a
/// skipped / budget-refused attempt) and `worker_base` the carrier's whole subsidy-derived worker base under the paying block's split.
/// The withheld amount never exceeds the subsidy-derived share, so a user-paid fee share is never touched. Elsewhere `recorded`.
pub fn palw_r_cap_withheld_base_v1(covers: bool, recorded: u64, worker_base: u64) -> u64 {
    if covers { recorded.max(worker_base) } else { recorded }
}

/// **C1: the newly issued, miner-facing subsidy an attempt-lane carrier is ever paid** under the rule: its escrow (released only at
/// `Final`, inside the producer's `R` cap) — never the uncarved remainder, and nothing for a budget-refused attempt (`escrowed = 0`).
/// What is withheld beyond the escrow is never minted.
pub fn palw_r_cap_paid_subsidy_v1(covers: bool, escrowed: u64, worker_base: u64) -> (u64, u64) {
    if !covers {
        // The old rule: the escrow at Final plus the uncarved remainder at acceptance (the C1 gap this rule closes).
        return (escrowed.saturating_add(worker_base.saturating_sub(escrowed)), 0);
    }
    let withheld = palw_r_cap_withheld_base_v1(true, escrowed, worker_base);
    (escrowed.min(worker_base), withheld.saturating_sub(escrowed))
}

/// **C2 (user decision 2026-10-15, hold revised 2026-10-16): the exit hold, in days** — the common hold for EVERY bond, now one day for
/// both modes (2026-10-16), so exit → re-registration → another mode regains no rate. A claim's own liability locks (G-13: Valid
/// locks to the liability horizon) still keep its collateral past this hold. It is the long
/// view's span ([`PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1`]); the withdrawal gate is [`PalwBondBudgetStateV1::withdrawal_holds`].
pub const PALW_BOND_EXIT_HOLD_DAYS_V1: u32 = PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1;

/// **C2: the exit hold in DAA under `hold`** — `panel_zero_days · daa_per_day` whatever modes the bond used (never the day view).
pub fn palw_bond_exit_hold_daa_v1(hold: &PalwBondHoldPolicyV1) -> u64 {
    hold.window_daa(PalwHoldModeV1::PanelZero)
}

/// **C3 (user decision 2026-10-15): the per-day rate of Round rights**, permille of the bond's per-day Round-rights cap
/// (`daily(C).round_rights`). CALIBRATION STARTING POINT, UNAPPROVED (the measured rate replaces it; RFC-0016 §3.6 Stage 1).
pub const PALW_BOND_HOLD_ROUND_RIGHTS_DAY_RATE_CALIBRATION_PERMILLE_V1: u32 = 1_000;
/// **C3: the per-day rate of the monetary income rows** (market / Round fees, source fees, native income), permille of the bond's
/// per-day `R` (`daily(C).reward_sompi`) — the SAME day view the bond's claims fill. CALIBRATION STARTING POINT, UNAPPROVED.
pub const PALW_BOND_HOLD_INCOME_DAY_RATE_CALIBRATION_PERMILLE_V1: u32 = 1_000;

/// **C3: the origins the rate / burst caps govern beside the long view** — Round rights and the monetary income rows.
pub fn palw_bond_hold_rate_origin_v1(origin: PalwBudgetOriginV1) -> bool {
    matches!(
        origin,
        PalwBudgetOriginV1::RoundRights
            | PalwBudgetOriginV1::MarketFee
            | PalwBudgetOriginV1::RoundFee
            | PalwBudgetOriginV1::SourceFee
            | PalwBudgetOriginV1::NativeIncome
    )
}

fn permille_of_u64(x: u64, permille: u32) -> u64 {
    u64::try_from((x as u128).saturating_mul(permille as u128) / 1_000).unwrap_or(u64::MAX)
}

/// **C3: the day-view caps an income row or a Round-rights draw is admitted against** — the bond's per-day caps with `R` and the Round
/// rights at their named rates (every other dimension as the day view's own). Linear with floors, so sub-additive over a split.
pub fn palw_bond_hold_rate_caps_v1(policy: &PalwBondBudgetPolicyV1, capital: u64) -> PalwBudgetVectorV1 {
    let daily = palw_bond_hold_daily_caps_v1(policy, capital);
    PalwBudgetVectorV1 {
        reward_sompi: permille_of_u64(daily.reward_sompi, PALW_BOND_HOLD_INCOME_DAY_RATE_CALIBRATION_PERMILLE_V1),
        round_rights: permille_of_u64(daily.round_rights, PALW_BOND_HOLD_ROUND_RIGHTS_DAY_RATE_CALIBRATION_PERMILLE_V1),
        ..daily
    }
}

/// The derived shadow id of one growth of a native income hold (a hold grows in place; each growth's short-period use is its own row).
const PALW_BOND_HOLD_RATE_GROWTH_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/hold-rate-growth/v1";

/// **C3: the shadow id of a native income hold's growth** at `accepted_daa` from `prior` held (unique per growth event).
pub fn palw_bond_hold_rate_growth_id_v1(income_id: &Hash64, accepted_daa: u64, prior: u64) -> Hash64 {
    keyed64(PALW_BOND_HOLD_RATE_GROWTH_DOMAIN_V1, &[income_id.as_byte_slice(), &accepted_daa.to_le_bytes(), &prior.to_le_bytes()])
}

impl PalwBondBudgetStateV1 {
    /// **C3: the rate / burst admission of an income row or a Round-rights draw** (beside the long view the caller checks): the bond's
    /// day view — which its claims fill too — within [`palw_bond_hold_rate_caps_v1`], and its burst view within the burst caps. A
    /// no-op for a V2 policy. Refuses before any write.
    pub fn hold_rate_admit_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: &PalwBondKeyV2,
        reservation: &PalwBudgetVectorV1,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let Some(hold) = policy.hold.as_ref() else { return Ok(()) };
        let rate = palw_bond_hold_rate_caps_v1(policy, capital);
        if let Some(dim) = palw_bond_hold_first_excess_v1(&self.hold_view_v1(PalwHoldViewV1::Day, bond), reservation, &rate) {
            return Err(PalwBudgetRefusalV1::Exhausted { dim });
        }
        let burst = palw_bond_hold_burst_caps_v1(policy, hold, capital);
        if let Some(dim) = palw_bond_hold_first_excess_v1(&self.hold_view_v1(PalwHoldViewV1::Burst, bond), reservation, &burst) {
            return Err(PalwBudgetRefusalV1::Burst { dim });
        }
        Ok(())
    }

    /// **C3: a bond's remaining Round rights just before a draw** — the long view's [`palw_round_rights_remaining_v1`], further bounded
    /// by the day view's rate and the burst view (V3 policy). The draw's candidate set reads this, so its reservation always fits.
    pub fn round_rights_remaining_rated_v1(&self, policy: &PalwBondBudgetPolicyV1, capital: u64, bond: &PalwBondKeyV2) -> u64 {
        let long = palw_round_rights_remaining_v1(policy, capital, self.bonds.get(bond));
        let Some(hold) = policy.hold.as_ref() else { return long };
        let rate = palw_bond_hold_rate_caps_v1(policy, capital);
        let burst = palw_bond_hold_burst_caps_v1(policy, hold, capital);
        let day = self.hold_view_v1(PalwHoldViewV1::Day, bond);
        let short = self.hold_view_v1(PalwHoldViewV1::Burst, bond);
        let mut n =
            long.min(rate.round_rights.saturating_sub(day.round_rights)).min(burst.round_rights.saturating_sub(short.round_rights));
        if matches!(policy.round_rights, PalwRoundRightsPolicyV1::CountAgainstBlocks) {
            n = n
                .min(rate.block_units.saturating_sub(day.block_units) / PALW_BUDGET_BLOCK_UNIT_V1)
                .min(burst.block_units.saturating_sub(short.block_units) / PALW_BUDGET_BLOCK_UNIT_V1);
        }
        n
    }
}

/// **C5 (user decision 2026-10-15): is OPV (Panel=0) verifier-liability accounting defined?** `false`: no verification liability,
/// hold or slashable scope exists yet for capital merely bonded for Panel=0 / OPV checks, so such capital counts 0 in `S_m` (the
/// user: required before activation). Flipping this is a reviewed change with the accounting it names, never a value.
pub const PALW_OPV_VERIFIER_LIABILITY_ACCOUNTING_DEFINED_V1: bool = false;

/// **C5: a candidate source of verifier capital for a model's `S_m`.**
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwVerifierCapitalSourceV1 {
    /// A Panel seat's exposure locked behind a claim (`PalwPanelDutyRowV1::seat_exposure`): liability = the false-Valid conviction of
    /// the seat, hold = the seat lock until the claim resolves, slashable scope = that exposure. Counts while the claim is live.
    PanelSeatExposure { exposure: u128, claim_live: bool },
    /// Capital merely bonded for Panel=0 / OPV checking (no seat, no per-check liability or hold yet).
    OpvBondedOnly { capital: u128 },
}

/// **C5: the verifier capital a source contributes to `S_m`** — its amount only where liability, hold and slashable scope are all
/// defined; `0` otherwise (bonded-only Panel=0 capital, a seat whose claim has resolved, a zero exposure).
pub fn palw_verifier_capital_counted_v1(source: PalwVerifierCapitalSourceV1) -> u128 {
    match source {
        PalwVerifierCapitalSourceV1::PanelSeatExposure { exposure, claim_live } => {
            if claim_live {
                exposure
            } else {
                0
            }
        }
        PalwVerifierCapitalSourceV1::OpvBondedOnly { capital } => {
            if PALW_OPV_VERIFIER_LIABILITY_ACCOUNTING_DEFINED_V1 {
                capital
            } else {
                0
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mirror(policy: PalwBondBudgetPolicyV1) -> PalwBondBudgetMirrorV1 {
        PalwBondBudgetMirrorV1 { from_daa: 100, policy, allocation: None }
    }

    const KAS: u64 = 100_000_000;

    fn bond(n: u8) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(crate::Hash64::from_bytes([n; 64]), 0))
    }

    fn h(n: u64) -> Hash64 {
        let mut b = [0u8; 64];
        b[..8].copy_from_slice(&n.to_le_bytes());
        Hash64::from_bytes(b)
    }

    /// TEST policy (V3): per 1,000 BILI per DAY — 3 claims, 3 blocks, 30 BILI of R, 3,000 weight, 1,000 Round rights.
    fn v3() -> PalwBondBudgetPolicyV1 {
        let mut p = PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1();
        p.capital_unit_sompi = 1_000 * KAS;
        p.claims_per_unit = 3;
        p.block_units_per_unit = 3 * PALW_BUDGET_BLOCK_UNIT_V1;
        p.reward_per_unit_sompi = 30 * KAS;
        p.final_weight_per_unit = 3_000;
        p.max_open_claims_per_bond = 1_000_000;
        p.round_rights = PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit: 1_000 };
        p.validate().expect("a well-formed V3 test policy");
        p
    }

    const CAPITAL: u64 = 1_000 * KAS;

    #[test]
    fn c3_income_rows_take_a_day_rate_and_a_burst_cap_beside_the_week() {
        // daily R = 30 BILI; burst R = ⌈30 · 72 · 4000 / (720 · 1000)⌉ = 12 BILI per 72 DAA (TEST values).
        let p = v3();
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        let fee = |s: &mut PalwBondBudgetStateV1, id: u64, now: u64, amount: u64, j: &mut Vec<PalwBudgetWriteV1>| {
            s.release_due(now, j);
            s.reserve_income_v1(&p, CAPITAL, h(id), bond(1), now, amount, amount, PalwBudgetOriginV1::MarketFee, j)
        };
        fee(&mut s, 1, 10, 10 * KAS, &mut j).expect("10 BILI fits the burst");
        assert_eq!(fee(&mut s, 2, 11, 10 * KAS, &mut j), Err(PalwBudgetRefusalV1::Burst { dim: PalwBudgetDimV1::Reward }));
        let w_burst = p.hold.unwrap().burst_window_daa;
        fee(&mut s, 3, 10 + w_burst, 10 * KAS, &mut j).expect("the burst window passed");
        fee(&mut s, 4, 10 + 2 * w_burst, 10 * KAS, &mut j).expect("30 BILI in the day");
        assert_eq!(
            fee(&mut s, 5, 10 + 3 * w_burst, KAS, &mut j),
            Err(PalwBudgetRefusalV1::Exhausted { dim: PalwBudgetDimV1::Reward }),
            "the day's rate is used although the 7-day total (210 BILI) has room"
        );
        let day = PALW_BOND_HOLD_DAA_PER_DAY_V1;
        fee(&mut s, 6, 10 + day, 10 * KAS, &mut j).expect("the next day");
        s.check_consistency().expect("the shadows are ordinary rows");
    }

    #[test]
    fn c3_claims_and_income_share_one_principal_in_the_short_views() {
        // A Panel=1 claim's 10 BILI of R fills the same day view an income row reads: the principal is never used twice.
        let p = v3();
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.release_due(10, &mut j);
        let ask =
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 10 * KAS, final_weight: 0, liability_sompi: 0 };
        s.reserve_claim_moded_v1(
            &p,
            CAPITAL,
            h(100),
            bond(1),
            None,
            10,
            ask,
            PalwBudgetOriginV1::Attempt,
            PalwHoldModeV1::PanelOne,
            &mut j,
        )
        .unwrap();
        assert_eq!(
            s.reserve_income_v1(&p, CAPITAL, h(101), bond(1), 10, 3 * KAS, 3 * KAS, PalwBudgetOriginV1::RoundFee, &mut j),
            Err(PalwBudgetRefusalV1::Burst { dim: PalwBudgetDimV1::Reward }),
            "10 + 3 > 12: the claim's R is in the burst view the income reads"
        );
        s.reserve_income_v1(&p, CAPITAL, h(102), bond(1), 10, 2 * KAS, 2 * KAS, PalwBudgetOriginV1::RoundFee, &mut j)
            .expect("10 + 2 ≤ 12");
        // And the other way round: the income's R is in the day view a Panel=1 claim checks.
        assert_eq!(s.hold_view_v1(PalwHoldViewV1::Day, &bond(1)).reward_sompi, 12 * KAS);
    }

    #[test]
    fn c3_round_rights_are_rated_and_the_draw_reads_the_same_bound() {
        // daily Round rights 1,000; burst ⌈1000 · 72 · 4000 / 720 000⌉ = 400; the hold total 1,000 — the hold is one day for both
        // modes since 2026-10-16 (TEST values).
        let p = v3();
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.release_due(10, &mut j);
        assert_eq!(s.round_rights_remaining_rated_v1(&p, CAPITAL, &bond(1)), 400);
        assert_eq!(palw_round_rights_remaining_v1(&p, CAPITAL, s.bond_row(&bond(1))), 1_000, "the 1-day hold alone would allow 1,000");
        assert_eq!(
            s.reserve_round_rights_v1(&p, CAPITAL, 1, bond(1), 10, 401, &mut j),
            Err(PalwBudgetRefusalV1::Burst { dim: PalwBudgetDimV1::RoundRights })
        );
        s.reserve_round_rights_v1(&p, CAPITAL, 1, bond(1), 10, 400, &mut j).unwrap();
        assert_eq!(s.round_rights_remaining_rated_v1(&p, CAPITAL, &bond(1)), 0);
        let w_burst = p.hold.unwrap().burst_window_daa;
        s.release_due(10 + w_burst, &mut j);
        assert_eq!(s.round_rights_remaining_rated_v1(&p, CAPITAL, &bond(1)), 400);
        s.reserve_round_rights_v1(&p, CAPITAL, 2, bond(1), 10 + w_burst, 400, &mut j).unwrap();
        s.release_due(10 + 2 * w_burst, &mut j);
        assert_eq!(s.round_rights_remaining_rated_v1(&p, CAPITAL, &bond(1)), 200, "the day's rate: 1,000 − 800");
    }

    #[test]
    fn c3_a_native_income_growth_is_rated_and_written_once() {
        let p = v3();
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.release_due(10, &mut j);
        s.hold_native_income_v1(&p, CAPITAL, h(7), bond(1), 10, 500, 10 * KAS, &[], true, &mut j).unwrap();
        assert_eq!(s.hold_view_v1(PalwHoldViewV1::Burst, &bond(1)).reward_sompi, 10 * KAS);
        // Growing the same hold by 10 more within the burst window: 20 > 12.
        assert_eq!(
            s.hold_native_income_v1(&p, CAPITAL, h(7), bond(1), 11, 500, 20 * KAS, &[], true, &mut j),
            Err(PalwBudgetRefusalV1::Burst { dim: PalwBudgetDimV1::Reward })
        );
        assert!(palw_bond_hold_rate_origin_v1(PalwBudgetOriginV1::NativeIncome));
        assert!(!palw_bond_hold_rate_origin_v1(PalwBudgetOriginV1::DeferredReporterIncome), "reporter debt stays outside the window");
        // A V2 policy: no short views at all (the long view alone, unchanged).
        let mut v2 = PalwBondBudgetPolicyV1::opv_mechanics_probe_v1();
        v2.max_open_claims_per_bond = 1_000;
        let s2 = PalwBondBudgetStateV1::new(0, &v2);
        assert_eq!(
            s2.hold_rate_admit_v1(&v2, CAPITAL, &bond(1), &PalwBudgetVectorV1 { reward_sompi: u64::MAX, ..PalwBudgetVectorV1::ZERO }),
            Ok(())
        );
    }

    #[test]
    fn c4b_user_escrow_is_never_bound_by_the_coinbase_per_claim_ceiling() {
        let mut p = v3();
        p.slice_rights_by_rho = true;
        p.rho = 3;
        let (r_c, _) = palw_bond_budget_claim_ceilings_v1(&p).expect("slicing policy");
        let ask = PalwBudgetAskV1 { block_units: 0, reward_sompi: r_c * 3, final_weight: 0, liability_sompi: KAS as u128 };
        assert_eq!(palw_bond_budget_reservation_v1(&p, ask).reward_sompi, r_c, "a coinbase-derived leg is sliced");
        assert_eq!(palw_bond_budget_escrow_reservation_v1(&p, ask).reward_sompi, r_c * 3, "a user's escrow is not");
        let s = PalwBondBudgetStateV1::new(0, &p);
        let r = s
            .preview_claim_for_origin_moded_v1(
                &p,
                CAPITAL,
                bond(1),
                None,
                ask,
                PalwBudgetOriginV1::KernelRoute,
                PalwHoldModeV1::PanelZero,
            )
            .expect("inside the common R cap");
        assert_eq!(r.reward_sompi, r_c * 3);
        // The required collateral still binds: a liability above the concurrent-collateral cap is refused before acceptance.
        let heavy = PalwBudgetAskV1 { liability_sompi: (CAPITAL as u128) * 2, ..ask };
        assert_eq!(
            s.preview_claim_for_origin_moded_v1(
                &p,
                CAPITAL,
                bond(1),
                None,
                heavy,
                PalwBudgetOriginV1::KernelRoute,
                PalwHoldModeV1::PanelZero
            ),
            Err(PalwBudgetRefusalV1::LiabilityExceedsCapital)
        );
    }

    #[test]
    fn c5_only_liability_backed_verifier_capital_counts() {
        use PalwVerifierCapitalSourceV1::*;
        assert_eq!(palw_verifier_capital_counted_v1(PanelSeatExposure { exposure: 500, claim_live: true }), 500);
        assert_eq!(palw_verifier_capital_counted_v1(PanelSeatExposure { exposure: 500, claim_live: false }), 0, "resolved: no lock");
        assert_eq!(palw_verifier_capital_counted_v1(PanelSeatExposure { exposure: 0, claim_live: true }), 0);
        assert!(!PALW_OPV_VERIFIER_LIABILITY_ACCOUNTING_DEFINED_V1);
        assert_eq!(palw_verifier_capital_counted_v1(OpvBondedOnly { capital: 1_000_000 }), 0, "bonded-only Panel=0 capital counts 0");
    }

    /// RFC-0016 V10 (user decision 2026-10-15): the gate withholds an `Unattested` model-epoch's subsidy whole; the other model's
    /// budget does not grow from it in that epoch, and the next epoch's accrual starts from zero — never re-issued.
    #[test]
    fn v10_withheld_subsidy_is_never_redistributed_nor_reissued() {
        use crate::palw_econ_contracts_v1::{PalwModelVerificationOutcomeV1, palw_model_verification_subsidy_v1};
        let gated = palw_model_verification_subsidy_v1(PalwModelVerificationOutcomeV1::Unattested, 5 * KAS);
        assert_eq!((gated.payable, gated.deferred, gated.withheld), (0, 0, 5 * KAS), "withheld whole, nothing deferred");
        let p = v3();
        let ap = PalwModelAllocationPolicyV1 {
            version: PALW_MODEL_ALLOCATION_POLICY_VERSION_V2,
            epoch_daa: 10,
            seasoning_epochs: 1,
            curve: PalwAllocationCurveV1 { alpha_halves: 3 },
            max_models_per_bond: 4,
            average: None,
        };
        let mut s = PalwBondBudgetStateV1::new(0, &p);
        let mut j = Vec::new();
        s.assign(&ap, bond(1), &[(h(1), 300 * KAS)], 1, CAPITAL, 0, |_| true, &mut j).unwrap();
        s.assign(&ap, bond(2), &[(h(2), 300 * KAS)], 1, CAPITAL, 0, |_| true, &mut j).unwrap();
        s.roll_epoch(&ap, 2, 20, |_| (CAPITAL, true), &mut j);
        s.accrue(10 * KAS, &mut j);
        assert_eq!((s.model_available(&h(1)), s.model_available(&h(2))), (5 * KAS, 5 * KAS));
        let ask =
            PalwBudgetAskV1 { block_units: PALW_BUDGET_BLOCK_UNIT_V1, reward_sompi: 5 * KAS, final_weight: 0, liability_sompi: 0 };
        s.release_due(21, &mut j);
        s.reserve_claim_v1(&p, CAPITAL, h(50), bond(1), Some(h(1)), 21, ask, PalwBudgetOriginV1::Attempt, &mut j).unwrap();
        // Model 1's claim is withheld at its Final (Unattested): its reservation stays charged — nothing flows to model 2.
        assert_eq!(s.model_available(&h(1)), 0);
        assert_eq!(s.model_available(&h(2)), 5 * KAS, "model 2 never receives model 1's withheld subsidy");
        // The next epoch: the accrual restarts at zero; the withheld 5 is never re-issued.
        s.roll_epoch(&ap, 3, 30, |_| (CAPITAL, true), &mut j);
        assert_eq!(s.header.allocation.map(|e| e.accrued_sompi), Some(0));
        assert_eq!((s.model_available(&h(1)), s.model_available(&h(2))), (0, 0));
    }

    #[test]
    fn c2_the_exit_hold_is_one_day_for_every_mode() {
        // User decision 2026-10-16: the hold is fixed at one day for Panel=0 and Panel=1 alike, so the exit hold — the long view's
        // span — is one day and equals both modes' views (a mode switch cannot regain rate: both views are the same day).
        let hold = PalwBondHoldPolicyV1::baseline_unapproved_v1();
        assert_eq!(PALW_BOND_EXIT_HOLD_DAYS_V1, 1);
        assert_eq!(palw_bond_exit_hold_daa_v1(&hold), PALW_BOND_HOLD_DAA_PER_DAY_V1);
        assert_eq!(palw_bond_exit_hold_daa_v1(&hold), hold.window_daa(PalwHoldModeV1::PanelZero));
        assert_eq!(palw_bond_exit_hold_daa_v1(&hold), hold.window_daa(PalwHoldModeV1::PanelOne), "both modes hold one day");
        // The V3 policy's long window IS the exit hold.
        assert_eq!(PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1().window_daa, palw_bond_exit_hold_daa_v1(&hold));
    }

    #[test]
    fn c1_the_rule_is_off_without_a_v3_budget() {
        assert!(!palw_r_cap_covers_worker_base_v1(None, 1_000), "no mirror: every shipped network");
        let probe = mirror(PalwBondBudgetPolicyV1::opv_mechanics_probe_v1());
        assert!(!palw_r_cap_covers_worker_base_v1(Some(&probe), 1_000), "the calibration mechanics probe keeps the old coinbase");
        let v3 = mirror(PalwBondBudgetPolicyV1::hold_baseline_unapproved_v1());
        assert!(!palw_r_cap_covers_worker_base_v1(Some(&v3), 99), "below the budget's height");
        assert!(palw_r_cap_covers_worker_base_v1(Some(&v3), 100));
    }

    #[test]
    fn c1_an_admitted_attempt_is_paid_only_its_escrow_and_a_refused_one_nothing() {
        // worker base 720 of a 1000 subsidy, carve (escrow) 500: the uncarved 220 is not paid outside R.
        assert_eq!(palw_r_cap_withheld_base_v1(true, 500, 720), 720);
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 500, 720), (500, 220), "escrow paid at Final, 220 never minted");
        // A budget-refused (skipped) attempt: no escrow, the whole base withheld, nothing paid.
        assert_eq!(palw_r_cap_withheld_base_v1(true, 0, 720), 720);
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 0, 720), (0, 720));
        // The old rule paid the uncarved remainder at acceptance.
        assert_eq!(palw_r_cap_withheld_base_v1(false, 500, 720), 500);
        assert_eq!(palw_r_cap_paid_subsidy_v1(false, 500, 720), (720, 0));
        // t12's 720 ‰ carve of a 72 % base: carve == base, nothing changes for an admitted attempt.
        assert_eq!(palw_r_cap_paid_subsidy_v1(true, 720, 720), (720, 0));
    }

    #[test]
    fn c1_the_withheld_amount_never_exceeds_the_subsidy_share() {
        // The coinbase subtracts the withheld amount from `worker_base + fee_worker_shares`; the fee shares (user-paid) survive.
        for (escrow, base) in [(0u64, 0u64), (0, 1), (1, 1), (10, 100), (100, 100)] {
            let withheld = palw_r_cap_withheld_base_v1(true, escrow, base);
            assert!(withheld <= base.max(escrow));
            let fee_share = 37u64;
            assert_eq!((base + fee_share).saturating_sub(withheld), base.saturating_sub(withheld) + fee_share);
        }
    }
}
