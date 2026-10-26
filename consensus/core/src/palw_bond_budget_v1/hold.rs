//! **ADR-0176 — the mode-aware common hold (ユーザー方針 2026-10-15; DORMANT, no height).**
//!
//! The user's binding guidance of 2026-10-15 for ADR-0176:
//!
//! * **Hold periods are the design baseline**: a claim verified with a Panel (Panel=1, and the existing V2 Panel route) holds its
//!   issuance capacity for a **1-day-equivalent** DAA span, a Panel-free claim (Panel=0 / OPV / native funded work) for a
//!   **7-day-equivalent** span.
//! * **Per-day maximum issuance is equalized**: for the same bond the per-day base caps `q, b, r, f` give Panel=1 the period caps
//!   `(q, b, r, f)` over 1 day and Panel=0 the period caps `(7q, 7b, 7r, 7f)` over 7 days — never the same counts per period with a
//!   7× faster recovery.
//! * **Switching modes never re-acquires capacity**: the same capital's consumption history, holds and liabilities carry across
//!   modes; an accepted claim's period, reservation and liability are fixed at acceptance and a later mode never shortens them.
//! * **Capacity return is not collateral release**: 1 d / 7 d is the earliest normal reuse, not a guaranteed refund time; the
//!   liability clock (`H_L`, `liability_until`) keeps collateral while a bounded procedure is open. A colluding early Valid never
//!   shortens Panel=1's day.
//! * **A weekly total alone allows spending everything on day 1**: short-period burst caps are ADDITIONAL limits over the SAME
//!   ledger — never a second bucket that issues rights of its own, never an early return of consumed long-period capacity.
//!
//! **How this is enforced** (one ledger, three views of it):
//!
//! ```text
//! long view   every reservation, released at d + W_long (W_long = panel_zero_days · daa_per_day)   — the engine's own window
//!             (the policy's window_daa IS W_long for a V3 policy), checked against the 7-day caps 7·daily(C) for every claim
//! day view    shadow rows (origin HoldDay) of every claim reservation, released at d + W_day (panel_one_days · daa_per_day),
//!             checked against the 1-day caps daily(C) for every Panel=1 claim
//! burst view  shadow rows (origin HoldBurst), released at d + W_burst, checked against ⌈daily(C) · W_burst · β / (daa_per_day·1000)⌉
//!             for every claim — an additional limit only
//! ```
//!
//! *Why this is exactly the user's rule.* A Panel=1 claim must satisfy "any 1-day window ≤ daily" (day view) — which implies "any
//! 7-day window ≤ 7·daily" — so the long view adds nothing for a Panel=1-only bond and its capacity recovers every day. A Panel=0
//! claim must satisfy "any 7-day window ≤ 7·daily" (long view). Because every reservation of every mode sits in the long view for
//! 7 days and in the day view for 1 day, a bond that consumed under Panel=0 cannot start Panel=1 with a fresh day (its Panel=0
//! reservations are in the long view, which a Panel=1 claim also checks), and a bond that consumed under Panel=1 cannot start
//! Panel=0 with a fresh week (its Panel=1 reservations stay in the long view for 7 days). The window is the bond's withdrawal hold
//! (`window_holds`), so exit and re-registration cannot clear the long view either.
//!
//! *Why shadow rows.* The day and burst views reuse the engine's own journaled machinery — a claim row in the `claims` table, its
//! release in the `releases` table, its aggregate in the `bonds` table under a **synthetic bond key** that no outpoint can equal
//! ([`palw_bond_hold_shadow_bond_v1`]: a domain-separated hash with output index `u32::MAX − kind`) — so they are rooted, carried,
//! reverted and replayed (delta 190) exactly as every other row, and no state encoding, table id or version changes. A shadow row is
//! never open, never live, never consumed and is dropped by `release_due` at its own release.
//!
//! **Every number here is POLICY and UNAPPROVED.** The 1 / 7 day spans are the user's design baseline; the per-day rates, the burst
//! window and factor and the concurrent-liability fraction come from the measurement procedure in
//! `docs/design/palw/adr-0176-cap-derivation-and-measurement-plan-2026-10-15.md` — never from current code values or past examples.

use super::*;
use crate::tx::TransactionOutpoint;

/// The budget policy version that carries [`PalwBondHoldPolicyV1`] (ユーザー方針 2026-10-15). Its per-unit rates are **per day**.
pub const PALW_BOND_BUDGET_POLICY_VERSION_V3: u16 = 3;

/// One day in milliseconds.
pub const PALW_BOND_HOLD_DAY_MS_V1: u64 = 86_400_000;

/// **DAA per day-equivalent**: the consensus clock moves once per clock slot (ADR-0142's recovery interval, 120 s), so a day is
/// `86 400 s / 120 s = 720` DAA. A *nominal* conversion: when the measured cadence is slower (e.g. ~150 s/DAA on testnet-12), 720 DAA
/// is longer than a wall-clock day, never shorter — the hold is never cut by a slow clock. A network on another slot needs another
/// constant (and validation refuses a policy that disagrees with this one).
pub const PALW_BOND_HOLD_DAA_PER_DAY_V1: u64 = PALW_BOND_HOLD_DAY_MS_V1 / crate::palw_heartbeat_v1::HEARTBEAT_RECOVERY_INTERVAL_MS;

/// **User decision (2026-10-14 / 2026-10-15): Panel=1's common hold, in days** (1-day-equivalent DAA).
pub const PALW_BOND_HOLD_PANEL_ONE_DAYS_V1: u32 = 1;
/// **User decision (2026-10-14 / 2026-10-15): Panel=0's common hold, in days** (7-day-equivalent DAA; RFC-0015 §8.5.5 `W`).
pub const PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1: u32 = 7;

/// **INTERIM, UNAPPROVED: the burst window**, DAA — one tenth of a day-equivalent (72 DAA, 2.4 h at the nominal slot). POLICY to be
/// fixed from the measured submission pattern (plan §Stage 1); listed in the decision doc.
pub const PALW_BOND_HOLD_BURST_WINDOW_INTERIM_DAA_V1: u64 = PALW_BOND_HOLD_DAA_PER_DAY_V1 / 10;

/// **INTERIM, UNAPPROVED: the burst factor**, permille of the average per-day rate over the burst window. Derived as the DAG
/// parallelism the attempt lane can carry in one slot (at most [`PALW_BOND_HOLD_BURST_PARALLELISM_INTERIM_V1`] accepted attempts of
/// one bond per slot) — a floor of 1000 (an honest steady miner always fits) and this interim ×4. POLICY.
pub const PALW_BOND_HOLD_BURST_FACTOR_INTERIM_PERMILLE_V1: u32 = 4_000;
/// **INTERIM: the DAG parallelism the burst factor is derived from** (accepted attempts of one bond in one slot, measured p95 TBD).
pub const PALW_BOND_HOLD_BURST_PARALLELISM_INTERIM_V1: u32 = 4;

/// **INTERIM, UNAPPROVED: the concurrent-liability cap**, permille of the bond's capital that its open claims may reserve as liability
/// collateral at once (`Σ K ≤ C · φ`). 1000 is today's PESG §6 bound (`Σ K ≤ C`); the derivation `λ · a · H` (+ margin) is plan
/// §Stage 2.
pub const PALW_BOND_HOLD_CONCURRENT_LIABILITY_INTERIM_PERMILLE_V1: u16 = 1_000;

/// Domains of the shadow views.
const PALW_BOND_HOLD_SHADOW_BOND_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/hold-shadow-bond/v1";
const PALW_BOND_HOLD_SHADOW_CLAIM_DOMAIN_V1: &[u8] = b"misaka-palw/bond-budget/hold-shadow-claim/v1";

/// **The verification mode a claim was accepted under** — fixed at acceptance (its period never moves).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
#[repr(u8)]
pub enum PalwHoldModeV1 {
    /// Panel=0: OPV kernel claims and native funded (Panel-free) work — the 7-day-equivalent hold.
    PanelZero = 0,
    /// Panel=1 and the existing V2 Panel route (Attempt, rider, Free Prompt, Panel-licensed kernel claims) — the 1-day hold.
    PanelOne = 1,
}

impl PalwHoldModeV1 {
    /// The default mode of an origin (a path that knows better passes its mode): the V2 Panel routes are Panel=1; a kernel-route
    /// reservation defaults to the longer, stricter Panel=0 (the kernel route passes `PanelOne` for a Panel-licensed claim).
    pub fn for_origin(origin: PalwBudgetOriginV1) -> Self {
        match origin {
            PalwBudgetOriginV1::KernelRoute => Self::PanelZero,
            _ => Self::PanelOne,
        }
    }

    /// The mode's hold, in days.
    pub fn days(self, hold: &PalwBondHoldPolicyV1) -> u64 {
        match self {
            Self::PanelZero => hold.panel_zero_days as u64,
            Self::PanelOne => hold.panel_one_days as u64,
        }
    }
}

/// **The hold policy** — every value POLICY; the spans are the user's baseline and validation pins them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwBondHoldPolicyV1 {
    /// DAA per day-equivalent ([`PALW_BOND_HOLD_DAA_PER_DAY_V1`]).
    pub daa_per_day: u64,
    /// Panel=1's hold in days (1).
    pub panel_one_days: u32,
    /// Panel=0's hold in days (7); the policy's `window_daa` is `panel_zero_days · daa_per_day`.
    pub panel_zero_days: u32,
    /// The burst view's window, DAA (`1..=panel_one_days · daa_per_day`).
    pub burst_window_daa: u64,
    /// The burst factor, permille of the per-day rate prorated over the burst window (`≥ 1000`).
    pub burst_factor_permille: u32,
    /// The concurrent-liability cap, permille of capital (`1..=1000`).
    pub concurrent_liability_permille: u16,
}

impl PalwBondHoldPolicyV1 {
    /// **NOT A PROPOSAL**: the user's spans with the interim, unapproved burst and liability values.
    pub const fn baseline_unapproved_v1() -> Self {
        Self {
            daa_per_day: PALW_BOND_HOLD_DAA_PER_DAY_V1,
            panel_one_days: PALW_BOND_HOLD_PANEL_ONE_DAYS_V1,
            panel_zero_days: PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1,
            burst_window_daa: PALW_BOND_HOLD_BURST_WINDOW_INTERIM_DAA_V1,
            burst_factor_permille: PALW_BOND_HOLD_BURST_FACTOR_INTERIM_PERMILLE_V1,
            concurrent_liability_permille: PALW_BOND_HOLD_CONCURRENT_LIABILITY_INTERIM_PERMILLE_V1,
        }
    }

    /// The mode's window, DAA (`days · daa_per_day`).
    pub fn window_daa(&self, mode: PalwHoldModeV1) -> u64 {
        mode.days(self).saturating_mul(self.daa_per_day)
    }

    /// The value's own refusals (the spans pinned to the user's decision; `window_daa` is checked by the budget policy).
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.daa_per_day != PALW_BOND_HOLD_DAA_PER_DAY_V1 {
            return Err("the hold's DAA per day must be the nominal slot conversion (86 400 s / 120 s = 720)");
        }
        if self.panel_one_days != PALW_BOND_HOLD_PANEL_ONE_DAYS_V1 || self.panel_zero_days != PALW_BOND_HOLD_PANEL_ZERO_DAYS_V1 {
            return Err("the common hold is 1-day-equivalent DAA for Panel=1 and 7-day-equivalent DAA for Panel=0 (user decision)");
        }
        if self.burst_window_daa == 0 || self.burst_window_daa > self.window_daa(PalwHoldModeV1::PanelOne) {
            return Err("the burst window must be positive and no longer than Panel=1's day");
        }
        if self.burst_factor_permille < 1_000 {
            return Err("the burst factor must be at least 1000 permille (an honest steady miner must fit the burst view)");
        }
        if self.concurrent_liability_permille == 0 || self.concurrent_liability_permille > 1_000 {
            return Err("the concurrent-liability cap is a positive permille of capital, at most 1000");
        }
        Ok(())
    }
}

impl PalwBondBudgetPolicyV1 {
    /// **NOT A PROPOSAL**: a V3 (mode-aware hold) policy with the user's spans and the interim unapproved burst/liability values,
    /// every per-day rate one unit — what a fixture or a probe of the V3 form reads. No network can be armed with it.
    pub fn hold_baseline_unapproved_v1() -> Self {
        let hold = PalwBondHoldPolicyV1::baseline_unapproved_v1();
        let mut p = Self::unapproved_probe_v1();
        p.version = PALW_BOND_BUDGET_POLICY_VERSION_V3;
        p.window_daa = hold.window_daa(PalwHoldModeV1::PanelZero);
        p.hold = Some(hold);
        p
    }
}

fn floor_mul_div_u64(a: u128, b: u128, d: u128) -> u64 {
    u64::try_from(a.checked_mul(b).map(|p| p / d.max(1)).unwrap_or(u128::MAX)).unwrap_or(u64::MAX)
}

/// **A bond's per-day caps** `daily(C)` under a V3 policy: `⌊C·rate/u⌋` per dimension (the policy's per-unit rates are per day;
/// `ρ` reaches `Q` only). Linear with a floor, hence sub-additive over any split of `C`.
pub fn palw_bond_hold_daily_caps_v1(policy: &PalwBondBudgetPolicyV1, capital: u64) -> PalwBudgetVectorV1 {
    let c = capital as u128;
    let u = policy.capital_unit_sompi as u128;
    let blocks = floor_mul_div_u64(c, policy.block_units_per_unit as u128, u);
    PalwBudgetVectorV1 {
        claims: floor_mul_div_u64(c, policy.claims_per_unit_at_rho() as u128, u),
        block_units: blocks,
        reward_sompi: floor_mul_div_u64(c, policy.reward_per_unit_sompi as u128, u),
        final_weight: c.checked_mul(policy.final_weight_per_unit as u128).map(|p| p / u.max(1)).unwrap_or(u128::MAX),
        round_rights: match policy.round_rights {
            PalwRoundRightsPolicyV1::CountAgainstBlocks => blocks / PALW_BUDGET_BLOCK_UNIT_V1,
            PalwRoundRightsPolicyV1::ExecutionCap { rights_per_unit } => floor_mul_div_u64(c, rights_per_unit as u128, u),
        },
    }
}

fn scale(v: PalwBudgetVectorV1, k: u64) -> PalwBudgetVectorV1 {
    PalwBudgetVectorV1 {
        claims: v.claims.saturating_mul(k),
        block_units: v.block_units.saturating_mul(k),
        reward_sompi: v.reward_sompi.saturating_mul(k),
        final_weight: v.final_weight.saturating_mul(k as u128),
        round_rights: v.round_rights.saturating_mul(k),
    }
}

/// **A mode's period caps**: `daily(C) · days(mode)` — the per-day maximum issuance is the same for both modes (Panel=1: `daily`
/// over 1 day; Panel=0: `7·daily` over 7 days).
pub fn palw_bond_hold_period_caps_v1(
    policy: &PalwBondBudgetPolicyV1,
    hold: &PalwBondHoldPolicyV1,
    capital: u64,
    mode: PalwHoldModeV1,
) -> PalwBudgetVectorV1 {
    scale(palw_bond_hold_daily_caps_v1(policy, capital), mode.days(hold))
}

fn ceil_mul_div(a: u128, b: u128, d: u128) -> u128 {
    let d = d.max(1);
    a.checked_mul(b).map(|p| p.div_ceil(d)).unwrap_or(u128::MAX)
}

/// **The burst caps** (an additional limit): `⌈daily(C) · W_burst · β / (daa_per_day · 1000)⌉` per dimension. The ceiling keeps a
/// bond whose daily rate is below one unit per burst window able to use it at all; the long and day views still bound the totals, so
/// the ceiling never raises a period total (splitting capital can only add burst headroom, never period capacity).
pub fn palw_bond_hold_burst_caps_v1(policy: &PalwBondBudgetPolicyV1, hold: &PalwBondHoldPolicyV1, capital: u64) -> PalwBudgetVectorV1 {
    let daily = palw_bond_hold_daily_caps_v1(policy, capital);
    let num = (hold.burst_window_daa as u128).saturating_mul(hold.burst_factor_permille as u128);
    let den = (hold.daa_per_day as u128).saturating_mul(1_000);
    let f = |x: u128| ceil_mul_div(x, num, den);
    let sat = |x: u128| u64::try_from(x).unwrap_or(u64::MAX);
    PalwBudgetVectorV1 {
        claims: sat(f(daily.claims as u128)),
        block_units: sat(f(daily.block_units as u128)),
        reward_sompi: sat(f(daily.reward_sompi as u128)),
        final_weight: f(daily.final_weight),
        round_rights: sat(f(daily.round_rights as u128)),
    }
}

/// The first dimension where `ledger + reservation` passes `caps`, among the dimensions the reservation actually uses (a claim that
/// takes nothing of a dimension is not refused by it — round rights drawn today never refuse a claim with `T = 0`).
pub fn palw_bond_hold_first_excess_v1(
    ledger: &PalwBudgetVectorV1,
    reservation: &PalwBudgetVectorV1,
    caps: &PalwBudgetVectorV1,
) -> Option<PalwBudgetDimV1> {
    [
        PalwBudgetDimV1::Claims,
        PalwBudgetDimV1::BlockUnits,
        PalwBudgetDimV1::Reward,
        PalwBudgetDimV1::FinalWeight,
        PalwBudgetDimV1::RoundRights,
    ]
    .into_iter()
    .find(|dim| {
        let r = reservation.get(*dim);
        r > 0 && ledger.get(*dim).saturating_add(r) > caps.get(*dim)
    })
}

/// The shadow view a row belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PalwHoldViewV1 {
    Day = 1,
    Burst = 2,
    /// ユーザー判断 2026-10-15 (`palw_bond_formula_v2`): terminal claims' liability until it ends.
    Liability = 3,
}

impl PalwHoldViewV1 {
    fn origin(self) -> PalwBudgetOriginV1 {
        match self {
            Self::Day => PalwBudgetOriginV1::HoldDay,
            Self::Burst => PalwBudgetOriginV1::HoldBurst,
            Self::Liability => PalwBudgetOriginV1::HoldLiability,
        }
    }

    fn window_daa(self, hold: &PalwBondHoldPolicyV1) -> u64 {
        match self {
            Self::Day => hold.window_daa(PalwHoldModeV1::PanelOne),
            Self::Burst => hold.burst_window_daa,
            // Event-driven (terminal + H_L); never written through the window path.
            Self::Liability => 0,
        }
    }
}

/// **The synthetic bond key of a bond's shadow view**: a domain-separated hash of the bond as the transaction id and the output index
/// `u32::MAX − view`. No registration outpoint can equal it (no transaction has 2³²−2 outputs, and the id is a keyed hash), so the
/// view's aggregate row never meets a real bond's row.
pub fn palw_bond_hold_shadow_bond_v1(view: PalwHoldViewV1, bond: &PalwBondKeyV2) -> PalwBondKeyV2 {
    let digest = keyed64(PALW_BOND_HOLD_SHADOW_BOND_DOMAIN_V1, &[&[view as u8], &enc(bond)]);
    PalwBondKeyV2(TransactionOutpoint::new(digest, u32::MAX - view as u32))
}

/// The shadow row id of a claim in a view.
pub fn palw_bond_hold_shadow_claim_v1(view: PalwHoldViewV1, claim: &Hash64) -> Hash64 {
    keyed64(PALW_BOND_HOLD_SHADOW_CLAIM_DOMAIN_V1, &[&[view as u8], claim.as_byte_slice()])
}

/// Whether an origin writes shadow rows: claims (and old claims seeded at the fence). Monetary income rows (fees, native income,
/// reporter debt) and Round rights are bounded by the long view alone (decision doc: per-day Round-rights / income rates).
pub fn palw_bond_hold_shadows_origin_v1(origin: PalwBudgetOriginV1) -> bool {
    matches!(
        origin,
        PalwBudgetOriginV1::Attempt
            | PalwBudgetOriginV1::Rider
            | PalwBudgetOriginV1::FreePrompt
            | PalwBudgetOriginV1::Legacy
            | PalwBudgetOriginV1::KernelRoute
    )
}

impl PalwBondBudgetStateV1 {
    /// **The day / burst view's ledger of a bond** (the sum of its shadow rows still inside the view).
    pub fn hold_view_v1(&self, view: PalwHoldViewV1, bond: &PalwBondKeyV2) -> PalwBudgetVectorV1 {
        self.bonds.get(&palw_bond_hold_shadow_bond_v1(view, bond)).map(|row| row.window).unwrap_or_default()
    }

    /// **The hold admission** (ユーザー方針 2026-10-15), run beside [`palw_bond_budget_admit_v1`] (which checks the long view against the
    /// 7-day caps): the concurrent-liability cap, the day view for a Panel=1 claim, the burst view for every claim. A no-op for a V2
    /// policy.
    pub fn hold_admit_v1(
        &self,
        policy: &PalwBondBudgetPolicyV1,
        capital: u64,
        bond: &PalwBondKeyV2,
        reservation: &PalwBudgetVectorV1,
        liability_sompi: u128,
        mode: PalwHoldModeV1,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let Some(hold) = policy.hold.as_ref() else { return Ok(()) };
        let row = self.bonds.get(bond).copied().unwrap_or_default();
        // The terminal claims whose liability has not ended (the formula's liability view; empty below `palw_bond_formula_v2`).
        let kept =
            self.bonds.get(&palw_bond_hold_shadow_bond_v1(PalwHoldViewV1::Liability, bond)).map_or(0, |r| r.open_liability_sompi);
        let open = row
            .open_liability_sompi
            .checked_add(kept)
            .and_then(|v| v.checked_add(liability_sompi))
            .ok_or(PalwBudgetRefusalV1::Overflow)?;
        if open > (capital as u128).saturating_mul(hold.concurrent_liability_permille as u128) / 1_000 {
            return Err(PalwBudgetRefusalV1::LiabilityExceedsCapital);
        }
        if mode == PalwHoldModeV1::PanelOne {
            let caps = palw_bond_hold_period_caps_v1(policy, hold, capital, PalwHoldModeV1::PanelOne);
            if let Some(dim) = palw_bond_hold_first_excess_v1(&self.hold_view_v1(PalwHoldViewV1::Day, bond), reservation, &caps) {
                return Err(PalwBudgetRefusalV1::Exhausted { dim });
            }
        }
        let burst = palw_bond_hold_burst_caps_v1(policy, hold, capital);
        if let Some(dim) = palw_bond_hold_first_excess_v1(&self.hold_view_v1(PalwHoldViewV1::Burst, bond), reservation, &burst) {
            return Err(PalwBudgetRefusalV1::Burst { dim });
        }
        Ok(())
    }

    /// **Write a claim's shadow rows** (both views) at its acceptance — a no-op for a V2 policy or an origin without shadows. The rows
    /// are fixed at acceptance: their release is `accepted_daa + view window`, and nothing but `release_due` removes them.
    pub(super) fn write_hold_shadows_v1(
        &mut self,
        policy: &PalwBondBudgetPolicyV1,
        claim_id: &Hash64,
        bond: &PalwBondKeyV2,
        accepted_daa: u64,
        reservation: &PalwBudgetVectorV1,
        origin: PalwBudgetOriginV1,
        j: &mut Vec<PalwBudgetWriteV1>,
    ) -> Result<(), PalwBudgetRefusalV1> {
        let Some(hold) = policy.hold.as_ref() else { return Ok(()) };
        if !palw_bond_hold_shadows_origin_v1(origin) || reservation.is_zero() {
            return Ok(());
        }
        for view in [PalwHoldViewV1::Day, PalwHoldViewV1::Burst] {
            let id = palw_bond_hold_shadow_claim_v1(view, claim_id);
            if self.claims.contains_key(&id) {
                return Err(PalwBudgetRefusalV1::Duplicate);
            }
            let key = palw_bond_hold_shadow_bond_v1(view, bond);
            let release = accepted_daa.checked_add(view.window_daa(hold)).ok_or(PalwBudgetRefusalV1::Overflow)?;
            let mut row = self.bonds.get(&key).copied().unwrap_or_default();
            row.window = row.window.checked_add(*reservation).ok_or(PalwBudgetRefusalV1::Overflow)?;
            row.latest_release_daa = row.latest_release_daa.max(release);
            self.put_bond(&key, row, j);
            self.put_claim(
                &id,
                Some(PalwBudgetClaimRowV1 {
                    bond: key,
                    model: None,
                    accepted_daa,
                    reuse_not_before: release,
                    reserved: *reservation,
                    consumed: PalwBudgetVectorV1::ZERO,
                    open: false,
                    in_window: true,
                    live: false,
                    origin: view.origin(),
                    liability_sompi: 0,
                }),
                j,
            );
            self.put_release((release, id), true, j);
        }
        Ok(())
    }

    /// **ユーザー判断 2026-10-15 (`palw_bond_formula_v2`): close a claim and keep its liability until it ends.** The claim closes as
    /// [`Self::close_at`] does (its issuance capacity still leaves only at its own `d + W`); its liability collateral moves to the bond's
    /// liability view until `liability_until` (terminal + `H_L`, at least the court/DA/retention span), where the concurrent-collateral
    /// check keeps counting it — a reservation is released only when the liability ends, never at a licence or a Final.
    pub fn close_keeping_liability_v1(&mut self, claim_id: &Hash64, liability_until: u64, j: &mut Vec<PalwBudgetWriteV1>) {
        let Some(claim) = self.claims.get(claim_id).copied().filter(|c| c.open) else { return };
        self.close_at(claim_id, Some(liability_until), j);
        if claim.liability_sompi == 0 {
            return;
        }
        let id = palw_bond_hold_shadow_claim_v1(PalwHoldViewV1::Liability, claim_id);
        if self.claims.contains_key(&id) {
            return;
        }
        let key = palw_bond_hold_shadow_bond_v1(PalwHoldViewV1::Liability, &claim.bond);
        let mut row = self.bonds.get(&key).copied().unwrap_or_default();
        row.open_claims = row.open_claims.saturating_add(1);
        row.open_liability_sompi = row.open_liability_sompi.saturating_add(claim.liability_sompi);
        row.latest_release_daa = row.latest_release_daa.max(liability_until);
        self.put_bond(&key, row, j);
        self.put_claim(
            &id,
            Some(PalwBudgetClaimRowV1 {
                bond: key,
                model: None,
                accepted_daa: claim.accepted_daa,
                reuse_not_before: liability_until,
                reserved: PalwBudgetVectorV1::ZERO,
                consumed: PalwBudgetVectorV1::ZERO,
                open: true,
                in_window: true,
                live: false,
                origin: PalwHoldViewV1::Liability.origin(),
                liability_sompi: claim.liability_sompi,
            }),
            j,
        );
        self.put_release((liability_until, id), true, j);
    }

    /// **The liability view of a bond**: the collateral its terminal claims still hold (`palw_bond_formula_v2`; 0 below it).
    pub fn kept_liability_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        self.bonds.get(&palw_bond_hold_shadow_bond_v1(PalwHoldViewV1::Liability, bond)).map_or(0, |r| r.open_liability_sompi)
    }

    /// **Follow a same-block shrink or a block-unit transfer into the shadows** (riders re-attribute their lead's block and escrow in
    /// the acceptance block; the riders' own reservations write their own shadows). Only ever lowers a shadow, never below zero.
    pub(super) fn shrink_hold_shadows_v1(&mut self, claim_id: &Hash64, freed: &PalwBudgetVectorV1, j: &mut Vec<PalwBudgetWriteV1>) {
        if freed.is_zero() {
            return;
        }
        for view in [PalwHoldViewV1::Day, PalwHoldViewV1::Burst] {
            let id = palw_bond_hold_shadow_claim_v1(view, claim_id);
            let Some(mut row) = self.claims.get(&id).copied().filter(|row| row.in_window) else { continue };
            let take = (*freed).min(row.reserved);
            row.reserved = row.reserved.checked_sub(take).unwrap_or(PalwBudgetVectorV1::ZERO);
            let mut agg = self.bonds.get(&row.bond).copied().unwrap_or_default();
            agg.window = agg.window.checked_sub(take).unwrap_or(PalwBudgetVectorV1::ZERO);
            self.put_bond(&row.bond.clone(), agg, j);
            self.put_claim(&id, Some(row), j);
        }
    }
}
