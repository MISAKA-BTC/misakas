//! **ADR-0160 lane escrow — the escrow is the reward itself; the bond holds only `m_c`**
//! (`Params::palw_capacity_escrow_at_licence`, F-E; testnet-12 only, post-launch, dormant).
//!
//! Today a claim's escrow `E` (the withheld worker carve, 3,200.85 MSK at testnet-12's subsidy) is
//! funded twice before its licence: as the withheld reward itself (unminted, burned on void, paid
//! through a vesting row at `Final` that matures on the lock's clocks) and as option A's reservation
//! on the producer's bond (`w + E`, SR-1). Past F-E, for a claim ACCEPTED past it:
//!
//! * **E-1.** The bond never reserves `E`. The escrow slot of the one ledger
//!   ([`crate::palw_state_v2::PalwStateParamsV2::claim_escrow_term_v2`]) holds `m_c`
//!   ([`palw_escrow_term_v2`], priced by [`palw_monetary_prelicense_risk_v2`]) instead, so the
//!   commitment, SR-1's release at a counted licence, the seat duty at bind (capped by the commitment),
//!   the admission ceiling, the producer's headroom, the forfeit and the load re-derivation all follow
//!   from that one function.
//! * **E-2.** The reward stays withheld and is the escrow: nothing is minted before `Final` plus the
//!   vesting row's maturity (V-4). No new field or object — the ledger simply stops counting `E` twice.
//! * **E-3.** `m_c` is released at a counted licence under SR-1's conditions, unchanged.
//! * **E-4.** A claim voided past the fence keeps its commitment for `h_obl = window_receipt` past the
//!   void ([`palw_claim_obligation_hold_v1`]) — the free-prompt abandon hold generalized to attempt
//!   claims — so a void, a withheld bind or a starved panel never frees the bond's capacity at once.
//! * **E-5.** An unattributed failure (S0′) costs exactly the stage commitment `m_c + reserved`: the
//!   forfeit reads the same escrow slot, and `slash_bond` saturates (never debt).
//!
//! **What is NOT here.** The credit (`ρ`, `q_credit`) is `palw_capacity_aggregate_liability`'s value
//! (lane liab, F-L); the per-bond weight cap that makes `reserved` small is F-W (lane weight). This
//! module reads the step through ONE accessor,
//! [`crate::palw_state_v2::PalwStateParamsV2::capacity_escrow_credit_at_v1`] — ADR-0160 §7.2's named
//! cross-lane line, wired to liab's mirror on `rcore/cap-int` (with the three lines listed below).
//! Without a step the term is `E` (no credit, `q = 0`), so F-E alone changes only E-4.
//!
//! **Two guards the verification of this lane added** (a testnet-12 claim whose slot is `E` is priced
//! and bound exactly as option A):
//!
//! * **The seat side (finding 1).** A credit lowers the commitment `w + m_c`, and L-4 caps the seat
//!   duty at `commitment / seats`, so past a credit the duty falls under `lock_2` while the lock does
//!   not (lane liab's AS-2 divides it by `ρ` only at `q_credit ≥ q_seat`). A seat then binds more
//!   panels than it can back at licence, and under load an honest claim is voided S0′ at RT#2. So the
//!   credit applies only at `q_credit ≥ q_seat` ([`palw_escrow_credit_applies_v1`], where AS-2 re-prices
//!   the lock by the same `ρ` on `rcore/cap-int`), and a seat bound to an attributable attempt past
//!   F-E reserves the eligibility `max(duty_bind, lock_2)` rather than `duty_bind` — the duty itself
//!   wherever no credit cut it ([`palw_escrow_bind_reserves_the_lock_v1`]): a drawn seat can always
//!   back its `Valid`.
//! * **The conviction side (finding 2).** `L = 3E` holds only where every conviction route forfeits the
//!   whole bond and burns its unmatured rewards (the freeze that makes one claim the binding case).
//!   Where a route charges the claim's own forfeit plus at most a capped tier and burns nothing else,
//!   each claim prices alone and the bond's free half bounds what the tiers of a campaign collect, so
//!   the term is priced by [`palw_escrow_m_star_v2`] on the floor the params name
//!   ([`crate::palw_state_v2::PalwStateParamsV2::capacity_escrow_conviction_floor_at_v1`]).
//!
//! **What `rcore/cap-int` rewires, and nothing else** (every line here reads the claim's
//! `accepted_daa`, so the load re-derivation stays exact):
//!
//! 1. [`crate::palw_state_v2::PalwStateParamsV2::capacity_escrow_credit_at_v1`] → liab's
//!    `capacity_step_at`; the stand-in `capacity_escrow_credits` and its setter go.
//! 2. [`palw_escrow_credit_applies_v1`] → liab's `palw_seat_credit_applies_v1(step)`, so the escrow
//!    credit and AS-2's lock cut read one threshold.
//! 3. [`crate::palw_state_v2::PalwStateParamsV2::capacity_escrow_conviction_floor_at_v1`] → the floor
//!    F-L gives the claim: `Tier(`[`palw_escrow_tier_at_min_bond_v1`]`)` while any producer route stays
//!    tier-capped (D-5 not taken), [`PalwEscrowConvictionFloorV1::WholeBond`] only once every producer
//!    route is in F-L's intent class.
//! 4. [`palw_escrow_bind_reserves_the_lock_v1`] → also true where F-L's step is in force at the claim's
//!    `accepted_daa` (AS-1 divides the duty by `ρ` at every step while AS-2 keeps the lock below
//!    `q_seat`), unless `validate_palw_v2` holds F-L and F-E to one height: a claim accepted between
//!    the two would otherwise bind under `lock_2` exactly as finding 1 did.

use crate::palw_state_v2::{
    PalwChainStateV2, PalwClaimPhaseV2, PalwClaimSourceV2, PalwClaimStateV2, PalwStateParamsV2, PalwStateV2Error, PalwVoidReasonV2,
};
use kaspa_hashes::Hash64;

/// **`P*`, the licence probability of one fraudulent claim the deterrent is sized for** (ADR-0152's
/// per-claim bar, redraws included), in permille. A constant: changing it is a new fence.
pub const PALW_ESCROW_P_STAR_PERMILLE_V1: u16 = 500;

/// **`L = 3·G`: what one conviction definitely collects, as a multiple of the claim's gain** (ADR-0160
/// §4.5, the action tier's cap). The fold reads `G` as `E` — the smallest `G` a claim can have
/// (`G = g_res + E ≥ E`) — so the `L` it prices with is never above the normative `3G`, and an
/// under-stated `L` only raises `m*`. A constant: changing it is a new fence.
pub const PALW_ESCROW_CONVICTION_MULTIPLE_V1: u128 = 3;

/// **One ramp step's credit, as the escrow term reads it**: the ramp factor `ρ` (≥ 1) and the credited
/// attribution rate `q_credit` in permille — the two numbers of F-L's `PalwCapacityStepV1` this lane
/// reads (its `from_daa` is how the step is FOUND, not what it prices).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEscrowCreditV1 {
    pub rho: u32,
    pub q_credit_permille: u16,
}

/// **A step of the credit schedule, keyed on the claim's `accepted_daa`** — the shape of F-L's
/// `PalwCapacityStepV1 { from_daa, rho, q_credit_permille }`, field for field, so `rcore/cap-int`
/// replaces it with that type without a conversion. On `rcore/cap-escrow` alone it is the stand-in
/// mirror's element ([`PalwStateParamsV2::with_capacity_escrow_credits_v1`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct PalwEscrowCreditStepV1 {
    pub from_daa: u64,
    pub rho: u32,
    pub q_credit_permille: u16,
}

impl PalwEscrowCreditStepV1 {
    pub fn credit(&self) -> PalwEscrowCreditV1 {
        PalwEscrowCreditV1 { rho: self.rho, q_credit_permille: self.q_credit_permille }
    }
}

/// **The step in force at `daa`**: the last of `steps` (sorted by `from_daa`) whose `from_daa ≤ daa`;
/// `None` before the first step. The ramp only ever APPENDS a step (one flag day per `ρ`, ADR-0160
/// D-9), and a claim is priced by the step of its own `accepted_daa` for its whole life.
pub fn palw_escrow_credit_in_force_v1(steps: &[PalwEscrowCreditStepV1], daa: u64) -> Option<PalwEscrowCreditV1> {
    steps.iter().rev().find(|step| step.from_daa <= daa).map(PalwEscrowCreditStepV1::credit)
}

/// **`m*(q)`: the smallest pre-licence commitment that makes one fraudulent claim unprofitable**
/// (ADR-0160 §4.5), in integer permille arithmetic, rounded UP:
///
/// ```text
/// m*(q) = max(0, ⌈(P*·E − q·L/(1−q)) / (1−P*)⌉)
///       = max(0, ⌈(p·E·(1000−q) − 1000·q·L) / ((1000−q)·(1000−p))⌉)   (p = P*‰, q in ‰)
/// ```
///
/// At `P* = 500‰` this is `max(0, E − 2·q·L/(1−q))`. `q = 1000‰` (every fraud convicted) needs
/// nothing; `P*` is clamped below 1000‰ (at 1 the bar is vacuous and the division undefined).
/// Saturating: an input past `u128` answers `E`'s side, never less.
pub fn palw_escrow_m_star_v1(e_sompi: u128, q_permille: u16, l_sompi: u128, p_star_permille: u16) -> u128 {
    let q = u128::from(q_permille.min(1000));
    if q == 1000 {
        return 0;
    }
    let p = u128::from(p_star_permille.min(999));
    let gain = p.saturating_mul(e_sompi).saturating_mul(1000 - q);
    let loss = 1000u128.saturating_mul(q).saturating_mul(l_sompi);
    if loss >= gain {
        return 0;
    }
    (gain - loss).div_ceil((1000 - q) * (1000 - p))
}

/// **The `q` a ramp step needs so that `⌈E/ρ⌉` binds** (`m*(q) ≤ ⌈E/ρ⌉`), in permille, rounded UP to
/// the next permille that satisfies it: ADR-0160 §4.5's `q/(1−q) ≥ (E − E/ρ)/(2L)` at `P* = 500‰`.
/// `1000` if no credit below certainty makes the floor bind. The shadow's alarm and the ramp gate G2
/// compare the measured rate with twice this (the credit rule `q_credit ≤ ½ × measured`).
pub fn palw_escrow_q_needed_permille_v1(e_sompi: u128, rho: u32, l_sompi: u128) -> u16 {
    let floor = e_sompi.div_ceil(u128::from(rho.max(1)));
    (0..=1000u16).find(|q| palw_escrow_m_star_v1(e_sompi, *q, l_sompi, PALW_ESCROW_P_STAR_PERMILLE_V1) <= floor).unwrap_or(1000)
}

/// **`m_c`: what one claim's escrow slot commits on its producer's bond past F-E** (ADR-0160 §4.5):
///
/// ```text
/// m_c = 0                               if E = 0 (the free-prompt lane, an unescrowed merged attempt)
///     = E                               if the class is not attributable (C7) or there is no credit
///     = min(E, max(m*(q_credit), ⌈E/ρ⌉)) otherwise, with L = 3·E
/// ```
///
/// X-I5: C7 → `E`. X-I6: `m_c ≥ ⌈E/ρ⌉ ≥ 1` sompi for `E > 0`, and `m_c = E` at `q_credit = 0`. Never
/// above `E`: the escrow slot never holds more than option A did.
///
/// **`L = 3E` with one claim binding is sound only where every producer conviction route forfeits the
/// whole bond and burns its unmatured rewards** (ADR-0160 AG-2/AG-3 with D-5,
/// [`PalwEscrowConvictionFloorV1::WholeBond`]). The fold prices through
/// [`palw_monetary_prelicense_risk_v2`] on the floor the params name; on a tier route this formula
/// leaves a full bond's campaign profitable (the lane's verification, finding 2).
pub fn palw_monetary_prelicense_risk_v1(e_sompi: u64, credit: Option<PalwEscrowCreditV1>, attributable: bool) -> u128 {
    let e = u128::from(e_sompi);
    if e == 0 {
        return 0;
    }
    let Some(credit) = credit.filter(|_| attributable) else { return e };
    let floor = e.div_ceil(u128::from(credit.rho.max(1)));
    let l = PALW_ESCROW_CONVICTION_MULTIPLE_V1.saturating_mul(e);
    palw_escrow_m_star_v1(e, credit.q_credit_permille, l, PALW_ESCROW_P_STAR_PERMILLE_V1).max(floor).min(e)
}

/// **`q_seat`: the credited attribution below which the escrow credit does not apply**, in permille —
/// lane liab's `PALW_CAPACITY_Q_SEAT_PERMILLE_V1` (ADR-0160 AS-2, `E/(E + 3G) ≤ 1/4`), the rate at
/// which AS-2 divides the seat lock by the step's `ρ`. The escrow credit lowers the commitment L-4
/// caps the duty by; below this rate the lock is not lowered with it, so the duty would fall under
/// `lock_2` and a seat could bind panels it cannot back (finding 1). `rcore/cap-int` replaces the
/// comparison in [`palw_escrow_credit_applies_v1`] with liab's `palw_seat_credit_applies_v1(step)`, so
/// the two sides can never read two thresholds. A constant: changing it is a new fence.
pub const PALW_ESCROW_Q_SEAT_PERMILLE_V1: u16 = 250;

/// **Does a ramp step's credit lower the escrow slot at all?** Only at `q_credit ≥ q_seat`
/// ([`PALW_ESCROW_Q_SEAT_PERMILLE_V1`]): the escrow and the seat lock are divided by the same `ρ` or
/// neither is. Below it the slot stays `E`, which is option A's slot and option A's seat backing.
pub fn palw_escrow_credit_applies_v1(credit: &PalwEscrowCreditV1) -> bool {
    credit.q_credit_permille >= PALW_ESCROW_Q_SEAT_PERMILLE_V1
}

/// **What one conviction of a claim can be counted on to collect, beyond the claim's own forfeit**
/// (finding 2) — the conviction routes' floor the escrow credit is priced against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwEscrowConvictionFloorV1 {
    /// **Some producer route collects the claim's forfeit `w + m_c` plus at most `tier` sompi, and burns
    /// no other reward of the bond** — the freeze does not reach the bond's other claims, so a campaign
    /// is priced claim by claim, and the tiers of many convictions are paid from the bond's free half
    /// (see [`palw_escrow_m_star_v2`]). `tier = 0` where a route charges the forfeit alone.
    Tier(u128),
    /// **Every producer route forfeits the whole bond and burns every unmatured reward** (ADR-0160 AG-2 /
    /// AG-3 with D-5): one claim is the binding case (Bernoulli), `L = 3E`
    /// ([`palw_monetary_prelicense_risk_v1`]).
    WholeBond,
}

/// **`m*` against a route that burns nothing beyond the convicted claim** (finding 2): the smallest `m`
/// with `EV(1) = (1−q)·[P*·E − (1−P*)·m] − q·(m + x(m)) ≤ 0`, in integer permille arithmetic, rounded
/// UP, where `x(m) = min(tier, f·m)` is the action tier one conviction is sure to collect and
/// `f = (1000 − κ)/κ` with `κ` the work gate's ceiling in permille (500‰ on testnet-12, `f = 1`).
///
/// **Why `f·m` caps the tier.** Without a whole-bond burn each claim of a campaign stands alone, so
/// `EV(K) = K·EV(1)` only while every conviction really collects its `L`. A bond of collateral `C`
/// holding `N` claims has `N·(w + m) ≤ κ·C` (the ceiling), and whatever the realized convictions `X ≤ N`
/// and the charges of the claims that failed, the tiers are paid from at least `C − N·(w + m) ≥
/// (1 − κ)·C`; so the tiers collected are at least `X·min(tier, (1 − κ)·C / N) ≥ X·min(tier, f·(w + m))`.
/// The forfeit `w + m` is reserved under the ceiling and always collected; `w` is left out of both
/// terms (it only adds to `L`), so the bound is conservative. At testnet-12's 13,000 MSK floor bond the
/// tier `min(100‰·C, 3G)` is 1,300 MSK — and ten convictions of 13k pieces would ask for the whole
/// bond, which is why the naive `L = w + m + 1,300` does not hold at the concurrency a credit enables.
///
/// ```text
/// case x = f·m:   m ≥ (1−q)·P·E / [(1−q)(1−P) + q·(1+f)]
/// case x = tier:  m ≥ [(1−q)·P·E − q·tier] / [(1−q)(1−P) + q]
/// ```
///
/// `q = 1000‰` needs nothing. `P*` is clamped below 1000‰ and `κ` into `1..=1000‰` (`κ = 1000‰` leaves
/// no free half: `x = 0`). Saturating.
pub fn palw_escrow_m_star_v2(e_sompi: u128, q_permille: u16, tier_sompi: u128, p_star_permille: u16, ceiling_permille: u32) -> u128 {
    let q = u128::from(q_permille.min(1000));
    if q == 1000 {
        return 0;
    }
    let p = u128::from(p_star_permille.min(999));
    let k = u128::from(ceiling_permille.clamp(1, 1000));
    // Scaled by 10⁶·κ: (1−q)·P·E → (1000−q)·P·E·κ; (1−q)(1−P) + q·(1+f) = (1−q)(1−P) + q/κ.
    let gain = (1000 - q).saturating_mul(p).saturating_mul(e_sompi);
    let m_free_half =
        gain.saturating_mul(k).div_ceil((1000 - q).saturating_mul(1000 - p).saturating_mul(k).saturating_add(q.saturating_mul(1_000_000)));
    if (1000 - k).saturating_mul(m_free_half) <= k.saturating_mul(tier_sompi) {
        return m_free_half;
    }
    // Past the tier: each conviction collects `m + tier` (scaled by 10⁶).
    let loss = q.saturating_mul(1000).saturating_mul(tier_sompi);
    if loss >= gain {
        return 0;
    }
    (gain - loss).div_ceil((1000 - q).saturating_mul(1000 - p).saturating_add(q.saturating_mul(1000)))
}

/// **The `q` a ramp step needs so that `⌈E/ρ⌉` binds against `floor`**, in permille, rounded UP to the
/// first permille that satisfies it; `1000` if only certainty does. [`palw_escrow_q_needed_permille_v1`]
/// for [`PalwEscrowConvictionFloorV1::WholeBond`] (at `L = 3E`), [`palw_escrow_m_star_v2`] otherwise.
/// Never below `q_seat`: under it the credit does not apply at all.
pub fn palw_escrow_q_needed_permille_v2(e_sompi: u128, rho: u32, floor: PalwEscrowConvictionFloorV1, ceiling_permille: u32) -> u16 {
    let target = e_sompi.div_ceil(u128::from(rho.max(1)));
    let needed = match floor {
        PalwEscrowConvictionFloorV1::WholeBond => {
            palw_escrow_q_needed_permille_v1(e_sompi, rho, PALW_ESCROW_CONVICTION_MULTIPLE_V1.saturating_mul(e_sompi))
        }
        PalwEscrowConvictionFloorV1::Tier(tier) => (0..=1000u16)
            .find(|q| palw_escrow_m_star_v2(e_sompi, *q, tier, PALW_ESCROW_P_STAR_PERMILLE_V1, ceiling_permille) <= target)
            .unwrap_or(1000),
    };
    needed.max(PALW_ESCROW_Q_SEAT_PERMILLE_V1)
}

/// **`m_c` against the conviction floor the params name** (finding 2): `0` for `E = 0`; `E` for C7 or
/// without a credit; otherwise `min(E, max(m*, ⌈E/ρ⌉))` with `m*` from
/// [`palw_monetary_prelicense_risk_v1`]'s `L = 3E` on [`PalwEscrowConvictionFloorV1::WholeBond`] and
/// from [`palw_escrow_m_star_v2`] on a [`PalwEscrowConvictionFloorV1::Tier`]. The `q_seat` gate is the
/// fold's ([`palw_escrow_term_v2`]), not this formula's. X-I5, X-I6 and `≤ E` as in `_v1`.
pub fn palw_monetary_prelicense_risk_v2(
    e_sompi: u64,
    credit: Option<PalwEscrowCreditV1>,
    attributable: bool,
    floor: PalwEscrowConvictionFloorV1,
    ceiling_permille: u32,
) -> u128 {
    let e = u128::from(e_sompi);
    match floor {
        PalwEscrowConvictionFloorV1::WholeBond => palw_monetary_prelicense_risk_v1(e_sompi, credit, attributable),
        PalwEscrowConvictionFloorV1::Tier(tier) => {
            if e == 0 {
                return 0;
            }
            let Some(credit) = credit.filter(|_| attributable) else { return e };
            let ramp = e.div_ceil(u128::from(credit.rho.max(1)));
            palw_escrow_m_star_v2(e, credit.q_credit_permille, tier, PALW_ESCROW_P_STAR_PERMILLE_V1, ceiling_permille).max(ramp).min(e)
        }
    }
}

/// **The action tier every producer conviction of a claim is sure to add at the smallest bond that can
/// hold it** — S1/S2's `min(100‰ · C_min, 3E)` (`palw_rcore_s1s2_action_v1` at `C₀ = C_min`, with `G`
/// read as `E`, its lower bound). 1,300 MSK on testnet-12. What [`PalwEscrowConvictionFloorV1::Tier`]
/// carries on `rcore/cap-int` where F-L puts S1 in the intent class and D-5 has not yet put kinds 5
/// and 12 there; on this branch alone the floor is `Tier(0)` (see
/// [`crate::palw_state_v2::PalwStateParamsV2::capacity_escrow_conviction_floor_at_v1`]).
pub fn palw_escrow_tier_at_min_bond_v1(min_collateral_sompi: u64, e_sompi: u128) -> u128 {
    let share = u128::from(min_collateral_sompi).saturating_mul(u128::from(crate::palw_state_v2::PALW_RCORE_S1S2_ACTION_PERMILLE_V1)) / 1000;
    share.min(e_sompi.saturating_mul(PALW_ESCROW_CONVICTION_MULTIPLE_V1))
}

/// **Does class `class_id` have a conviction route the credit may price?** (ADR-0160 §4.5.) `false`
/// for C7's list (`Params::palw_rcore_conservative_classes`, the mirror — testnet-12's 2M row: an
/// attention lie past `n_ctx` 8,192 is not attributable, so its `q` is 0 until ADR-0153).
///
/// **The list alone, deliberately, and not C7's window rule beside it.** `m_c` is part of a claim's
/// commitment, which the load re-derivation recomputes from the claim RECORD and the params
/// (`assert_internal_consistency_v3`, X-I7) — never from the class's mutable lifecycle row, whose
/// profile is re-derived when its artifact bytes are committed. SR-1's release reads its own record
/// for the same reason (`palw_rcore_release_record_holds_v1`). The window rule's half of C7 is
/// enforced where state IS at hand, at acceptance ([`palw_escrow_class_gate_v1`]): past the fence a
/// class C7 by its window but absent from the list takes no attempt, so every claim priced here as
/// attributable is outside C7 by both halves. On testnet-12 both halves select exactly the 2M row.
pub fn palw_claim_class_attributable_v1(params: &PalwStateParamsV2, class_id: &Hash64) -> bool {
    !params.rcore_conservative_classes().contains(class_id)
}

/// **The escrow slot of a claim accepted past F-E** — `m_c` at the credit of the claim's own
/// `accepted_daa`, and never above option A's term (so a ruleset that armed F-E without the
/// escrow-backed exposure could not ADD a reservation — `validate_palw_v2` refuses that ruleset too).
/// Called through [`PalwStateParamsV2::claim_escrow_term_v2`], which dispatches here past the fence.
///
/// The credit counts only at `q_credit ≥ q_seat` ([`palw_escrow_credit_applies_v1`], finding 1), and
/// is priced against the conviction floor of the claim's `accepted_daa`
/// ([`PalwStateParamsV2::capacity_escrow_conviction_floor_at_v1`], finding 2) under the work gate's
/// ceiling. Every input is the record's or the params', so the load re-derivation returns the same slot.
pub fn palw_escrow_term_v2(params: &PalwStateParamsV2, accepted_daa: u64, escrowed_reward: u64, class_id: &Hash64) -> u128 {
    let option_a = params.claim_escrow_reservation_v1(accepted_daa, escrowed_reward);
    palw_monetary_prelicense_risk_v2(
        escrowed_reward,
        params.capacity_escrow_credit_at_v1(accepted_daa).filter(palw_escrow_credit_applies_v1),
        palw_claim_class_attributable_v1(params, class_id),
        params.capacity_escrow_conviction_floor_at_v1(accepted_daa, u128::from(escrowed_reward)),
        params.fp_max_exposure_ratio_permille(),
    )
    .min(option_a)
}

/// **Does a seat bound to `claim` reserve its eligibility `max(duty_bind, lock_2)` rather than
/// `duty_bind`?** (finding 1.) For every ATTEMPT claim accepted past F-E whose class is attributable
/// (not C7) and whose escrow is not 0 — the claims a credit can price below option A.
///
/// L-4 caps the duty at `commitment / seats` so that a withholder never pins more of its panel than it
/// forfeits (F14), and L-4b draws only a seat with room for `max(duty_bind, lock_2)`; while `lock_2 ≤
/// duty` (floor and 8k at option A's `w + E`) a counted `Valid` is backed by construction. A credit
/// cuts the commitment by up to `ρ` and the duty with it, but not the lock: `duty_bind` then falls
/// under `lock_2`, the draw's eligibility is no longer what the bind reserved, and a seat can sit on
/// more panels than its room can back at licence — under load an honest claim binds, cannot license,
/// and is voided S0′ at RT#2 (the verifier's saturation probe: 320 honest claims charged at ρ = 10).
/// Reserving the eligibility at bind makes the draw's check and the reservation one amount, so a seat
/// that was drawn can always lock its `Valid` (A-1 counts `max(duty, lock)` and the lock is at most
/// `lock_2`) and a saturated seat is refused at the draw — a `BindTimeout` / `NoCapablePanel` void,
/// uncharged — never at the licence.
///
/// * **Without a credit it is `duty_bind`, byte for byte, on every testnet-12 class it reaches**: the
///   floor's and the 8k row's `lock_2` is at most their duty at `w + E` (the floor's: 240.13 against
///   640.17 MSK), so the maximum is the duty (`v_t5_saturated_seats_never_charge_an_honest_producer`'s F-E-armed,
///   uncredited run binds and reserves exactly as option A).
/// * **Keyed on the claim, not on the credit**, so it also covers a duty cut by anything else that
///   leaves the lock alone — on `rcore/cap-int`, lane liab's AS-1, which divides `λ` and `lock_2` by
///   `ρ` in the duty at every step while AS-2 divides the lock only from `q_seat`. There the
///   commitment is still `w + E` (this lane credits nothing below `q_seat`), so the lock reserved is
///   within F14. From `q_seat`, AS-2 lowers the lock by the same `ρ` and `lock_2 ≤ duty_bind` again on
///   floor and 8k (`5 · ⌈lock_2/ρ⌉ ≤ ⌈E/ρ⌉`): the two amounts coincide and F14 holds as AS-1 states it.
/// * **Where they still differ** (this branch alone with a credit, where no AS-2 exists; or a class
///   whose `seats · lock_2` exceeds its commitment) backing is chosen over F14: the panel pins at most
///   `seats · lock_2` against a forfeit of `w + m_c` (3.75× at ρ = 10 here), so the residual falls on
///   seat capital and never on an honest producer.
/// * **C7 and the free-prompt lane keep ADR-0152 L-4b's accepted residual** (their lock exceeds the
///   duty by design — 2M's `lock_2` would pin 2.65× its forfeit — and neither is ever credited).
pub fn palw_escrow_bind_reserves_the_lock_v1(params: &PalwStateParamsV2, claim: &PalwClaimStateV2) -> bool {
    params.capacity_escrow_active_at(claim.accepted_daa)
        && matches!(claim.source, PalwClaimSourceV2::Attempt)
        && claim.escrowed_reward > 0
        && palw_claim_class_attributable_v1(params, &claim.class_id)
}

/// **Does a void for `reason` keep the claim's obligation for `h_obl`?** (E-4.) Every reason the
/// producer is NOT convicted under — the ones a producer can reach by withholding, abandoning or
/// starving its own claim after observing its panel (the withdrawal principle's T1–T8), and the
/// class-capacity void:
///
/// * `BindTimeout`, `NoCapablePanel` — no bind in the window (free today: nothing is charged);
/// * `ReceiptTimeout` (the second), `UnavailableQuorum`, `NotReplayBacked` — S0′, charged the stage
///   commitment (E-5) and held beside it.
///
/// **A conviction's void is charged, not held**: `CourtFraud`, `ProducerWithholding`, `CourtDefault` and
/// `CourtHeldVerdict` take the forfeit and the action tier at the void (and past F-L the aggregate
/// funnel), so the obligation is realized there. They are also the reasons a conviction writes on a
/// `Final` claim it reverses (`reverse_convicted_final`), whose commitment was already released at
/// `Final`; a record-pure hold cannot tell that record from a live claim's void, and holding it would
/// re-count a commitment the ledger no longer carries. Exhaustive: a reason added later (liab's
/// `AggregateForfeit`) must be placed here by name.
pub fn palw_void_reason_keeps_obligation_v1(reason: PalwVoidReasonV2) -> bool {
    match reason {
        PalwVoidReasonV2::BindTimeout
        | PalwVoidReasonV2::NoCapablePanel
        | PalwVoidReasonV2::ReceiptTimeout
        | PalwVoidReasonV2::UnavailableQuorum
        | PalwVoidReasonV2::NotReplayBacked => true,
        PalwVoidReasonV2::CourtFraud
        | PalwVoidReasonV2::ProducerWithholding
        | PalwVoidReasonV2::CourtDefault
        | PalwVoidReasonV2::CourtHeldVerdict => false,
    }
}

/// **E-4: the DAA through which a voided claim's commitment stays on its bond** — `voided_daa + h_obl`
/// (`h_obl = window_receipt`, 600 DAA on testnet-12), for an ATTEMPT claim accepted past F-E and
/// voided for a reason that keeps the obligation ([`palw_void_reason_keeps_obligation_v1`]); `None`
/// for every other record (the free-prompt lane keeps its own abandon hold, E-6). Inclusive, as the
/// sweep is (`palw_claim_is_on_abandon_hold_v2`'s boundary): the hold is released in the first block
/// whose DAA exceeds it. Saturating: an overflowing hold never elapses, the safe direction. A pure
/// function of the record and the params, so the deadline index and the ledger re-derive it alike.
pub fn palw_claim_obligation_release_at_v1(claim: &PalwClaimStateV2, params: &PalwStateParamsV2) -> Option<u64> {
    if !params.capacity_escrow_active_at(claim.accepted_daa) || !matches!(claim.source, PalwClaimSourceV2::Attempt) {
        return None;
    }
    let PalwClaimPhaseV2::Voided { voided_daa, reason } = claim.phase else { return None };
    let h_obl = params.window_receipt();
    (h_obl > 0 && palw_void_reason_keeps_obligation_v1(reason)).then(|| voided_daa.saturating_add(h_obl))
}

/// **E-4: is `claim` on its obligation hold at `now_daa`?** — what `palw_claim_commitment_v1`'s
/// `Voided` arm ORs beside the free-prompt abandon hold.
pub fn palw_claim_obligation_hold_v1(claim: &PalwClaimStateV2, params: &PalwStateParamsV2, now_daa: u64) -> bool {
    palw_claim_obligation_release_at_v1(claim, params).is_some_and(|release_at| now_daa <= release_at)
}

/// **The acceptance half of C7 for the escrow term**: past F-E (at the block's DAA, which is the new
/// claim's `accepted_daa`), an ATTEMPT of a class that is C7 by its verification window
/// (`palw_rcore_class_is_c7_v1`) but absent from the conservative list is refused
/// (`ClassNotAdmitting`, the class gate's refusal), so no claim is ever priced as attributable while
/// the window rule calls its class C7 (X-I5). Unreachable on testnet-12 as configured (the list IS the
/// window rule's one class); it binds only a ruleset that opens a long-window class without listing
/// it. The free-prompt lane escrows nothing (`E = 0`) and is not asked.
pub fn palw_escrow_class_gate_v1(
    params: &PalwStateParamsV2,
    state: &PalwChainStateV2,
    class_id: &Hash64,
    now_daa: u64,
    attempt: bool,
) -> Result<(), PalwStateV2Error> {
    if !attempt || !params.capacity_escrow_active_at(now_daa) {
        return Ok(());
    }
    if palw_claim_class_attributable_v1(params, class_id) && crate::palw_state_v2::palw_rcore_class_is_c7_v1(params, state, class_id) {
        return Err(PalwStateV2Error::ClassNotAdmitting {
            class: *class_id,
            state: "C7 by its verification window but absent from palw_rcore_conservative_classes: past \
                    palw_capacity_escrow_at_licence its escrow term would be priced as attributable (ADR-0160 X-I5)"
                .into(),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// testnet-12's `E` (720‰ of the 4,445.62014 MSK subsidy): 320,084,650,080 sompi.
    const E: u128 = 320_084_650_080;
    const MSK: u128 = 100_000_000;
    /// ADR-0160 §4.5's normative `L = 3G` on the floor: `3 × (w + E)` with `w_floor = 10,752,660`.
    const L_3G_FLOOR: u128 = 3 * (E + 10_752_660);
    const L_CMIN: u128 = 13_000 * MSK;

    fn msk(sompi: u128) -> f64 {
        sompi as f64 / 1e8
    }

    /// **E-T2: the golden `m*(q)` table of ADR-0160 §4.5, both `L` values** (to 0.1 MSK, the table's
    /// precision), and `m* = E` at `q = 0`.
    #[test]
    fn m_star_is_the_adr_table_at_both_l_values() {
        let rows: [(u16, f64, f64); 6] = [
            (0, 3_200.85, 3_200.85),
            (50, 2_190.0, 1_832.4),
            (80, 1_530.8, 940.0),
            (100, 1_066.9, 312.0),
            (110, 827.1, 0.0),
            (143, 0.0, 0.0),
        ];
        for (q, at_3g, at_cmin) in rows {
            let a = palw_escrow_m_star_v1(E, q, L_3G_FLOOR, PALW_ESCROW_P_STAR_PERMILLE_V1);
            let b = palw_escrow_m_star_v1(E, q, L_CMIN, PALW_ESCROW_P_STAR_PERMILLE_V1);
            println!("q = {q}‰: m*(3G) = {:.2} MSK, m*(C_min) = {:.2} MSK", msk(a), msk(b));
            assert!((msk(a) - at_3g).abs() < 0.1, "q = {q}‰ at L = 3G: {} vs {at_3g}", msk(a));
            assert!((msk(b) - at_cmin).abs() < 0.1, "q = {q}‰ at L = C_min: {} vs {at_cmin}", msk(b));
        }
        assert_eq!(palw_escrow_m_star_v1(E, 0, L_3G_FLOOR, 500), E, "no attribution: the whole escrow");
        assert_eq!(palw_escrow_m_star_v1(E, 1000, L_3G_FLOOR, 500), 0, "certain attribution: nothing");
    }

    /// **E-T2: the `q` each ramp step needs** (ADR-0160 §4.5: 0.130 / 0.138 / 0.142 / 0.143 at `L = 3G`),
    /// rounded up to the permille, and the fold's own `L = 3E` needs the same permille at every step.
    #[test]
    fn q_needed_is_the_adr_list_and_barely_moves_with_rho() {
        for (rho, want) in [(10u32, 131u16), (25, 138), (100, 142), (1000, 143)] {
            let at_3g = palw_escrow_q_needed_permille_v1(E, rho, L_3G_FLOOR);
            let at_3e = palw_escrow_q_needed_permille_v1(E, rho, 3 * E);
            let at_cmin = palw_escrow_q_needed_permille_v1(E, rho, L_CMIN);
            println!("rho = {rho}: q_needed = {at_3g}‰ at 3G, {at_3e}‰ at 3E, {at_cmin}‰ at C_min");
            assert_eq!(at_3g, want, "rho = {rho} at 3G");
            assert_eq!(at_3e, want, "rho = {rho}: pricing G as E moves no permille");
            assert!(at_cmin <= at_3g, "a larger L needs no more attribution");
        }
    }

    /// **X-I5 / X-I6 and the ceiling**: C7 and no-credit are `E`; `q = 0` is `E` at any `ρ`; past the
    /// needed `q` the term is exactly `⌈E/ρ⌉ ≥ 1`; the term never exceeds `E`; `E = 0` is `0`.
    #[test]
    fn m_c_is_e_for_c7_and_no_credit_and_the_floor_past_the_needed_q() {
        let credit = |rho, q| Some(PalwEscrowCreditV1 { rho, q_credit_permille: q });
        let e = E as u64;
        assert_eq!(palw_monetary_prelicense_risk_v1(e, credit(1000, 1000), false), E, "X-I5: C7 keeps E");
        assert_eq!(palw_monetary_prelicense_risk_v1(e, None, true), E, "no step, no credit");
        for rho in [1u32, 10, 25, 50, 100, 1000] {
            assert_eq!(palw_monetary_prelicense_risk_v1(e, credit(rho, 0), true), E, "X-I6: q = 0 is E at rho {rho}");
            let m = palw_monetary_prelicense_risk_v1(e, credit(rho, 143), true);
            assert_eq!(m, E.div_ceil(u128::from(rho)), "rho {rho}: past q_needed the ramp floor binds");
            assert!(m >= 1);
        }
        assert_eq!(palw_monetary_prelicense_risk_v1(1, credit(1000, 999), true), 1, "X-I6: at least one sompi");
        assert_eq!(palw_monetary_prelicense_risk_v1(0, credit(10, 143), true), 0, "no escrow, no term (the FP lane)");
        assert_eq!(palw_monetary_prelicense_risk_v1(e, credit(0, 143), true), E, "rho 0 is read as 1");
        // Between: at q = 100‰ (below q_needed) m* binds above E/rho.
        let m = palw_monetary_prelicense_risk_v1(e, credit(10, 100), true);
        assert!(m > E.div_ceil(10) && m < E, "q below the need: m* binds ({})", msk(m));
    }

    /// **`m_c` is monotone** — never larger for a larger credit (more `q`, more `ρ`), never above `E`.
    #[test]
    fn m_c_never_rises_with_the_credit() {
        let e = E as u64;
        let mut last_by_rho = u128::MAX;
        for rho in [1u32, 2, 5, 10, 25, 50, 100, 1000, 100_000] {
            let mut last = u128::MAX;
            for q in (0..=1000u16).step_by(7) {
                let m = palw_monetary_prelicense_risk_v1(e, Some(PalwEscrowCreditV1 { rho, q_credit_permille: q }), true);
                assert!(m <= E && m <= last, "rho {rho}, q {q}");
                last = m;
            }
            let at = palw_monetary_prelicense_risk_v1(e, Some(PalwEscrowCreditV1 { rho, q_credit_permille: 500 }), true);
            assert!(at <= last_by_rho, "rho {rho}");
            last_by_rho = at;
        }
    }

    /// **The deterrent the term buys: EV(1) ≤ 0 at `P ≤ P*` for every credited `q`** (ADR-0160 §4.5 —
    /// freeze-on-first-conviction makes one claim the binding case). `EV(1) = (1−q)·[P·E − (1−P)·m] − q·L`
    /// with the fold's `m_c` and `L = 3E`, over a grid of `q`, `ρ` and `P`.
    #[test]
    fn one_fraudulent_claim_is_unprofitable_at_the_bar() {
        let e = E as f64;
        for rho in [1u32, 10, 25, 100, 1000] {
            for q in (0..=990u16).step_by(11) {
                let m =
                    palw_monetary_prelicense_risk_v1(E as u64, Some(PalwEscrowCreditV1 { rho, q_credit_permille: q }), true) as f64;
                let qf = f64::from(q) / 1000.0;
                for p in [0.0, 0.1, 0.25, 0.5] {
                    let ev = (1.0 - qf) * (p * e - (1.0 - p) * m) - qf * 3.0 * e;
                    assert!(ev <= 1e-3 * e, "rho {rho}, q {q}‰, P {p}: EV(1) = {ev}");
                }
            }
        }
        // ADR-0160 Appendix A's EV(K) at q = 0.11, P = 0.5, m = 32 MSK, L = 13,000 MSK: K = 1 binds.
        let ev = |k: i32| {
            let (q, p, m, l, e) = (0.11f64, 0.5f64, 32.0f64, 13_000.0f64, 3_200.846_500_8f64);
            (1.0 - q).powi(k) * f64::from(k) * (p * e - (1.0 - p) * m) - (1.0 - (1.0 - q).powi(k)) * l
        };
        for (k, want) in [(1, -20.0), (2, -193.0), (5, -1_317.0), (10, -4_006.0), (100, -12_999.0), (1000, -13_000.0)] {
            assert!((ev(k) - want).abs() < 1.0, "EV({k}) = {} vs {want}", ev(k));
        }
    }

    /// testnet-12's floor `w` (0.1075266 MSK).
    const W_FLOOR: u128 = 10_752_660;
    /// The tier S1/S2 adds at a 13,000 MSK bond: `min(100‰ · 13,000, 3E)` = 1,300 MSK.
    const TIER_13K: u128 = 1_300 * MSK;
    /// testnet-12's work-gate ceiling.
    const KAPPA: u32 = 500;

    /// The fold's slot for a claim of escrow `E` at `(ρ, q)` on `floor` — the q_seat gate included.
    fn m_c(rho: u32, q: u16, floor: PalwEscrowConvictionFloorV1) -> u128 {
        let credit = Some(PalwEscrowCreditV1 { rho, q_credit_permille: q }).filter(palw_escrow_credit_applies_v1);
        palw_monetary_prelicense_risk_v2(E as u64, credit, true, floor, KAPPA)
    }

    /// `E[f(X)]` for `X ~ Binomial(n, q)`, in log space (so `n` in the thousands neither underflows nor
    /// overflows).
    fn binomial_expectation(n: u64, q: f64, f: impl Fn(u64) -> f64) -> f64 {
        if q <= 0.0 {
            return f(0);
        }
        if q >= 1.0 {
            return f(n);
        }
        let (lq, lp) = (q.ln(), (1.0 - q).ln());
        let mut log_choose = 0.0f64;
        let mut sum = 0.0f64;
        for x in 0..=n {
            if x > 0 {
                log_choose += ((n - x + 1) as f64).ln() - (x as f64).ln();
            }
            sum += (log_choose + x as f64 * lq + (n - x) as f64 * lp).exp() * f(x);
        }
        sum
    }

    /// **A full bond's campaign on a route that burns nothing beyond the convicted claim** (finding 2's
    /// setting), in MSK. `n` fraudulent claims of slot `m` on a bond of `c`: each is convicted with
    /// probability `q` (forfeit `w + m`, plus its tier while the bond's free part lasts), else licenses
    /// with probability `p` (gains `E`) or fails and is charged `w + m` (S0′). The tiers are paid from
    /// what is left after EVERY claim's forfeit (`c − n·(w + m)`, the least the free part can be), so the
    /// defender's collection is never overstated.
    fn campaign_ev(c: f64, n: u64, m: f64, w: f64, e: f64, q: f64, p: f64, tier: f64) -> f64 {
        let free = (c - n as f64 * (w + m)).max(0.0);
        binomial_expectation(n, q, |x| {
            let rest = (n - x) as f64;
            rest * (p * e - (1.0 - p) * (w + m)) - x as f64 * (w + m) - (x as f64 * tier).min(free)
        })
    }

    /// **Finding 2's closed forms**: on `Tier(0)` (this branch: S1's first strikes charge the forfeit
    /// alone) `m* = (1−q)E/(1+q)`; on `Tier(1,300)` below the tier `m* = (1−q)E/(1+3q)`; `q = 1000‰` is
    /// 0; and the `q` each ramp step needs on each floor — never below `q_seat`.
    #[test]
    fn m_star_v2_is_the_closed_form_and_the_q_each_step_needs() {
        let e = E as f64;
        for q in [0u16, 100, 250, 500, 800, 950] {
            let qf = f64::from(q) / 1000.0;
            let at_0 = palw_escrow_m_star_v2(E, q, 0, PALW_ESCROW_P_STAR_PERMILLE_V1, KAPPA);
            assert!((msk(at_0) - (1.0 - qf) * e / (1.0 + qf) / 1e8).abs() < 1e-6, "Tier(0), q {q}: {}", msk(at_0));
            let at_t = palw_escrow_m_star_v2(E, q, TIER_13K, PALW_ESCROW_P_STAR_PERMILLE_V1, KAPPA);
            let below = (1.0 - qf) * e / (1.0 + 3.0 * qf);
            let above = ((1.0 - qf) * 0.5 * e - qf * TIER_13K as f64) / ((1.0 - qf) * 0.5 + qf);
            let want = if below <= TIER_13K as f64 { below } else { above.max(0.0) };
            assert!((msk(at_t) - want / 1e8).abs() < 1e-6, "Tier(1,300), q {q}: {} vs {}", msk(at_t), want / 1e8);
            println!("q = {q}‰: m*(Tier 0) = {:.2} MSK, m*(Tier 1,300) = {:.2} MSK", msk(at_0), msk(at_t));
        }
        assert_eq!(palw_escrow_m_star_v2(E, 1000, 0, 500, KAPPA), 0, "certain attribution: nothing");
        assert_eq!(palw_escrow_m_star_v2(E, 0, TIER_13K, 500, KAPPA), E, "no attribution: the whole escrow");
        // κ = 1000‰ leaves no free half: the tier is never counted.
        assert_eq!(palw_escrow_m_star_v2(E, 500, TIER_13K, 500, 1000), palw_escrow_m_star_v2(E, 500, 0, 500, KAPPA));
        let mut rows = Vec::new();
        for rho in [10u32, 25, 50, 100, 1000] {
            let t0 = palw_escrow_q_needed_permille_v2(E, rho, PalwEscrowConvictionFloorV1::Tier(0), KAPPA);
            let t1 = palw_escrow_q_needed_permille_v2(E, rho, PalwEscrowConvictionFloorV1::Tier(TIER_13K), KAPPA);
            let wb = palw_escrow_q_needed_permille_v2(E, rho, PalwEscrowConvictionFloorV1::WholeBond, KAPPA);
            println!("rho = {rho}: q_needed = {t0}‰ (Tier 0), {t1}‰ (Tier 1,300), {wb}‰ (whole bond, q_seat-gated)");
            assert!(t0 >= t1 && t1 >= wb && wb == PALW_ESCROW_Q_SEAT_PERMILLE_V1, "rho {rho}");
            rows.push((rho, t0, t1));
        }
        assert_eq!(rows, vec![(10, 819, 693), (25, 924, 858), (50, 961, 925), (100, 981, 962), (1000, 999, 997)]);
    }

    /// **Finding 1's gate**: no credit below `q_seat`, whatever the floor — the slot is `E`, option A's,
    /// so the seat side is option A's too; at and past it the credit counts. Monotone in `q` and `ρ`.
    #[test]
    fn the_credit_counts_only_from_q_seat_and_never_rises() {
        for floor in [PalwEscrowConvictionFloorV1::Tier(0), PalwEscrowConvictionFloorV1::Tier(TIER_13K), PalwEscrowConvictionFloorV1::WholeBond] {
            for rho in [1u32, 10, 100, 1000] {
                assert_eq!(m_c(rho, PALW_ESCROW_Q_SEAT_PERMILLE_V1 - 1, floor), E, "{floor:?} rho {rho}: below q_seat");
                let mut last = u128::MAX;
                for q in (0..=1000u16).step_by(9).chain([1000]) {
                    let m = m_c(rho, q, floor);
                    assert!(m <= E && m <= last && m >= E.div_ceil(u128::from(rho)), "{floor:?} rho {rho} q {q}");
                    last = m;
                }
                assert_eq!(m_c(rho, 1000, floor), E.div_ceil(u128::from(rho)), "certainty: the ramp floor");
            }
        }
        assert!(m_c(10, 250, PalwEscrowConvictionFloorV1::WholeBond) == E.div_ceil(10), "whole bond at q_seat: E/ρ binds");
        assert!(m_c(10, 250, PalwEscrowConvictionFloorV1::Tier(0)) > E / 2, "Tier(0) at q_seat: m* = 0.6E");
    }

    /// **Finding 2, the fix: on a tier route the priced `m_c` keeps EV ≤ 0 for one claim AND for a full
    /// bond's campaign** — 13,000 and 100,000 MSK bonds, every `N` up to the ceiling, `P ∈ {0, ¼, ½}`,
    /// every ramp step `ρ` and a grid of `q` (below `q_seat` too, where the slot is `E`), on `Tier(0)`
    /// (this branch) and `Tier(1,300)` (`rcore/cap-int` before D-5; the floor tier at `C_min`, which a
    /// larger bond's own tier only exceeds), with the tiers paid from the bond's free part only
    /// (`campaign_ev`).
    #[test]
    fn a_tier_route_campaign_is_unprofitable_at_the_priced_slot() {
        let (e, w) = (E as f64 / 1e8, W_FLOOR as f64 / 1e8);
        for c in [13_000.0f64, 100_000.0] {
            for (tier, floor) in [(0.0f64, PalwEscrowConvictionFloorV1::Tier(0)), (1_300.0f64, PalwEscrowConvictionFloorV1::Tier(TIER_13K))] {
                for rho in [10u32, 25, 50, 100, 1000] {
                    for q in (0..=1000u16).step_by(25) {
                        let m = msk(m_c(rho, q, floor));
                        let qf = f64::from(q) / 1000.0;
                        let n_max = ((c * f64::from(KAPPA) / 1000.0) / (w + m)).floor() as u64;
                        for p in [0.0, 0.25, 0.5] {
                            let one = (1.0 - qf) * (p * e - (1.0 - p) * m) - qf * (m + tier.min(m));
                            assert!(one <= 1e-6, "{c} {floor:?} rho {rho} q {q} P {p}: EV(1) = {one}");
                            for n in [1, n_max / 2, n_max].into_iter().filter(|n| *n >= 1) {
                                let ev = campaign_ev(c, n, m, w, e, qf, p, tier);
                                assert!(ev <= 1e-6, "{c} {floor:?} rho {rho} q {q} P {p} N {n}: EV = {ev} MSK");
                            }
                        }
                    }
                }
            }
        }
    }

    /// **Finding 2, the defect it closes**: on a tier route (1,300 MSK at 13k) the Bernoulli price
    /// `L = 3E` (the lane's first `m_c`: `⌈E/10⌉` from 131‰) and the naive `L = w + m + 1,300` (whose
    /// ρ = 10 need is ≈ 470‰) both leave a full 13k bond's campaign profitable: twenty claims ask their
    /// convictions for more tiers than the bond's free half holds.
    #[test]
    fn the_bernoulli_and_the_naive_tier_price_are_profitable_on_a_tier_route() {
        let (e, w, c) = (E as f64 / 1e8, W_FLOOR as f64 / 1e8, 13_000.0f64);
        let bernoulli = msk(palw_monetary_prelicense_risk_v1(E as u64, Some(PalwEscrowCreditV1 { rho: 10, q_credit_permille: 150 }), true));
        assert!((bernoulli - 320.08).abs() < 0.01);
        let n = ((c / 2.0) / (w + bernoulli)).floor() as u64;
        assert_eq!(n, 20);
        let at_150 = campaign_ev(c, n, bernoulli, w, e, 0.15, 0.5, 1_300.0);
        let at_470 = campaign_ev(c, n, bernoulli, w, e, 0.47, 0.5, 1_300.0);
        println!("20 claims at m = {bernoulli:.2} MSK on a 13k bond: EV = {at_150:.0} MSK at q = 0.15, {at_470:.0} MSK at q = 0.47");
        assert!(at_150 > 10_000.0 && at_470 > 3_000.0, "the defect: fraud pays");
        // The one-claim test the lane shipped cannot see it: EV(1) at L = 3E is negative.
        assert!(0.85 * (0.5 * e - 0.5 * bernoulli) - 0.15 * 3.0 * e < 0.0);
        // At the same q the fix prices the slot out of it.
        for q in [150u16, 470] {
            let m = msk(m_c(10, q, PalwEscrowConvictionFloorV1::Tier(TIER_13K)));
            let n = ((c / 2.0) / (w + m)).floor() as u64;
            assert!(campaign_ev(c, n, m, w, e, f64::from(q) / 1000.0, 0.5, 1_300.0) <= 0.0, "q {q}: m {m:.2}, N {n}");
        }
    }

    /// The tier at the smallest bond: 1,300 MSK at testnet-12's 13,000 MSK floor; `3E` past 96k.
    #[test]
    fn the_tier_at_the_minimum_bond_is_ten_percent_capped_at_3e() {
        assert_eq!(palw_escrow_tier_at_min_bond_v1((13_000 * MSK) as u64, E), TIER_13K);
        assert_eq!(palw_escrow_tier_at_min_bond_v1((1_000_000 * MSK) as u64, E), 3 * E);
        assert_eq!(palw_escrow_tier_at_min_bond_v1(0, E), 0);
    }

    /// The stand-in schedule is found by `from_daa`, last step wins, nothing before the first.
    #[test]
    fn the_step_in_force_is_the_last_one_at_or_below_the_daa() {
        let steps = [
            PalwEscrowCreditStepV1 { from_daa: 100, rho: 10, q_credit_permille: 150 },
            PalwEscrowCreditStepV1 { from_daa: 500, rho: 25, q_credit_permille: 150 },
        ];
        assert_eq!(palw_escrow_credit_in_force_v1(&steps, 99), None);
        assert_eq!(palw_escrow_credit_in_force_v1(&steps, 100).map(|c| c.rho), Some(10));
        assert_eq!(palw_escrow_credit_in_force_v1(&steps, 499).map(|c| c.rho), Some(10));
        assert_eq!(palw_escrow_credit_in_force_v1(&steps, 500).map(|c| c.rho), Some(25));
        assert_eq!(palw_escrow_credit_in_force_v1(&[], 500), None);
    }

    /// Every reason is placed: the unconvicted ones hold, the convictions charge.
    #[test]
    fn the_unconvicted_reasons_hold_and_the_convictions_charge() {
        use PalwVoidReasonV2::*;
        for r in [BindTimeout, NoCapablePanel, ReceiptTimeout, UnavailableQuorum, NotReplayBacked] {
            assert!(palw_void_reason_keeps_obligation_v1(r), "{r:?}");
        }
        for r in [CourtFraud, ProducerWithholding, CourtDefault, CourtHeldVerdict] {
            assert!(!palw_void_reason_keeps_obligation_v1(r), "{r:?}");
        }
    }
}
