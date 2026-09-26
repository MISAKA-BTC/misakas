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
//!   ([`palw_monetary_prelicense_risk_v1`]) instead, so the commitment, SR-1's release at a counted
//!   licence, the seat duty at bind (capped by the commitment), the admission ceiling, the producer's
//!   headroom, the forfeit and the load re-derivation all follow from that one function.
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
//! cross-lane line, wired to liab's mirror on `rcore/cap-int`. Without a step the term is `E` (no
//! credit, `q = 0`), so F-E alone changes only E-4.

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
pub fn palw_escrow_term_v2(params: &PalwStateParamsV2, accepted_daa: u64, escrowed_reward: u64, class_id: &Hash64) -> u128 {
    let option_a = params.claim_escrow_reservation_v1(accepted_daa, escrowed_reward);
    palw_monetary_prelicense_risk_v1(
        escrowed_reward,
        params.capacity_escrow_credit_at_v1(accepted_daa),
        palw_claim_class_attributable_v1(params, class_id),
    )
    .min(option_a)
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
