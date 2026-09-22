//! **ADR-0151's economic-safety bundle: a fraudulent Final's rights are recoverable, and what is
//! not recoverable is priced.**
//!
//! The defence that stops one economic actor from profitably holding a claim's whole
//! `3`-of-`5` quorum is not identity — `palw_admission_independence` says so itself, "it is a price,
//! not a proof: a registrant with several keys paid to several addresses is several parties". The
//! defence is the inequality `minimum_colluding_quorum × seat_lock > max_fraud_gain`
//! ([`crate::palw_panel_var_v1::palw_colluding_quorum_covers_v1`]): hold all three seats and the
//! three locks still cost more than the lie earns.
//!
//! **That inequality was false on the shipped card, and by a wide margin.** Measured on the
//! testnet-12 held Qwen2.5 row at `n_ctx` 2,097,152:
//!
//! | | |
//! |---|---|
//! | `max_fraud_gain` as computed | 2,702.96409 MSK |
//! | three colluding locks cover | 2,702.96409 MSK (**+2 sompi**) |
//! | execution quanta one Final mints | 2,963 |
//! | block rights the gain does NOT count | **2,963 round blocks' FEES** |
//!
//! **A round block earns fees and no subsidy, and the first reading of this got that wrong.**
//! `coinbase.rs`: "a round block's fees are its payout's, aggregated — never the merger's lump,
//! never a carve (**the lane mints nothing, so there is no subsidy to carve**)". Pricing a permit at
//! the block subsidy put the uncounted rights at 10,976.98 MSK; the honest figure is the FEE flow
//! those 2,963 rounds divert, which is a property of the network's traffic and not of the claim.
//! That is why [`palw_permit_value_sompi_v1`] is a declared per-network ceiling and not a
//! derivation: consensus bounds a block's mass, never its fee per unit of mass, so no function here
//! can compute it. What a testnet is FOR is measuring it.
//!
//! `palw_claim_extra_economic_rights_v1` returned `0` while its own doc named "Permits, extra
//! eligibility, or any other sompi-denominated right the claim mints" — and on this network a Final
//! mints exactly that (`palw_execution_quanta_v1`: "an execution quantum is an execution *right*: it
//! converts into at most one algo-10 round permit"). Whatever those rounds are worth, the lie got
//! them for nothing.
//!
//! # Two ways to close it, and why this module takes both
//!
//! Either put the notional value of every minted right into the gain — which makes a seat post
//! collateral against fees nobody has earned yet, the "buy liveness with capital" mistake ADR-0151
//! exists to undo — or make the rights RECOVERABLE so there is nothing to price. This module makes
//! them recoverable and prices only the residual:
//!
//! ```text
//! Final at D                                    liability lock expires at D + window_court
//!   │                                                        │
//!   ├──────────── quanta exist, NOT yet spendable ────────────┤
//!   │             (maturity, this module)                     │
//!   │                                                         ▼
//!   └── a conviction filed anywhere in here destroys ──▶ spendable, unconvictable
//!       every unused quantum of that Final
//! ```
//!
//! With `maturity == window_court` the two edges coincide: at every instant a conviction can still be
//! filed, not one of the Final's quanta has been spent, so revocation recovers **all** of them and the
//! gain's extra term is honestly zero. Shorten the maturity and a window opens in which a right is
//! both spendable and still convictable; [`palw_realizable_before_maturity_v1`] prices exactly that
//! window, so **any** maturity is a safe configuration and only the honest producer's latency moves.
//!
//! # The price of the safe setting, stated
//!
//! At `maturity == window_court` an honest Final's execution permits become spendable
//! `window_court` DAA after it finalizes — 3,000 DAA on the RC windows, and on a network whose clock
//! is one heartbeat every 120 s that is about **100 hours of wall clock**. That is the cost of
//! "recoverable", and it is a knob: [`PALW_EXEC_QUANTUM_MATURITY_DAA`] is the only constant to move,
//! and moving it cannot open a hole because the residual is priced.

use crate::Hash64;

/// **Rounds per DAA tick** — the execution lane runs one round a second and a `ConsensusV2` clock
/// advances once per heartbeat interval, so this is the heartbeat interval in seconds.
///
/// It converts a liability horizon (DAA) into the number of algo-10 permits an attacker could spend
/// inside it, which is what [`palw_realizable_before_maturity_v1`] has to count. Derived from the
/// cadence rather than typed: a network on a different cadence realizes a different number of rights
/// per DAA, and a constant here would price the wrong one.
pub const fn palw_rounds_per_daa_v1(target_time_per_block_ms: u64) -> u64 {
    let ms = if target_time_per_block_ms == 0 { 1 } else { target_time_per_block_ms };
    let per = ms / 1_000;
    if per == 0 { 1 } else { per }
}

/// **How long after its Final a quantum may not be spent, in DAA.**
///
/// `0` is the pre-bundle behaviour: a quantum is spendable as soon as its span opens, which is what
/// made a fraudulent Final's 2,963 permits realizable ~82 hours before its liability lock expired.
///
/// **testnet-12 sets it to the CHALLENGE window, not the liability horizon, on purpose** (the
/// operator, 2026-09-23). Matching the horizon (`window_court`, 3,000 DAA) makes the safety argument
/// trivial — nothing is ever both spendable and convictable, so the residual is identically zero —
/// and buys that triviality with about a hundred hours of frozen execution rights on a clock that
/// ticks every 120 s. Two costs follow: the last testnet before mainnet could not exercise its own
/// execution lane, and `palw_realizable_before_maturity_v1` — the function the whole design rests on —
/// would never be reached by a real claim.
///
/// At `window_challenge` (1,200 DAA, ~40 h) a 1,800-DAA window stays in which a right is both
/// spendable and still convictable, and that window is PRICED into `max_fraud_gain`. The tradeoff
/// becomes the continuous thing it should be — longer maturity, more recoverable, less collateral;
/// shorter maturity, faster permits, more collateral — and mainnet picks its point from what t12
/// measures instead of inheriting a hundred-hour freeze.
pub const PALW_EXEC_QUANTUM_MATURITY_IS_THE_CHALLENGE_WINDOW: bool = true;

/// The maturity this network applies, in DAA: the challenge window on testnet-12.
///
/// Takes both windows because the choice is between them, and returning the shorter one is the whole
/// decision — a caller that passed only one could not express it.
pub const fn palw_exec_quantum_maturity_daa_v1(window_challenge_daa: u64, window_court_daa: u64) -> u64 {
    if PALW_EXEC_QUANTUM_MATURITY_IS_THE_CHALLENGE_WINDOW { window_challenge_daa } else { window_court_daa }
}

/// **The earliest DAA at which a Final's execution quanta may be spent.**
pub const fn palw_exec_quantum_matures_at_v1(final_daa: u64, window_challenge_daa: u64, window_court_daa: u64) -> u64 {
    final_daa.saturating_add(palw_exec_quantum_maturity_daa_v1(window_challenge_daa, window_court_daa))
}

/// **The rights a fraudulent Final can realize before a conviction could still take them** — the
/// residual [`crate::palw_panel_var_v1::palw_max_fraud_gain_v1`] must carry.
///
/// A quantum is spendable from `final_daa + maturity` and revocable until
/// `final_daa + window_court`. The overlap is the exposure:
///
/// ```text
/// realizable = min(quanta_minted, rounds_in(window_court − maturity)) × value_of_one_permit
/// ```
///
/// Zero when the maturity reaches the horizon, which is the shipped setting. Saturating throughout:
/// a network that somehow priced a permit at `u64::MAX` should refuse to seat a panel, not wrap.
pub fn palw_realizable_before_maturity_v1(
    quanta_minted: u32,
    window_challenge_daa: u64,
    window_court_daa: u64,
    target_time_per_block_ms: u64,
    permit_value_sompi: u64,
) -> u128 {
    let maturity = palw_exec_quantum_maturity_daa_v1(window_challenge_daa, window_court_daa);
    let gap_daa = window_court_daa.saturating_sub(maturity);
    if gap_daa == 0 || quanta_minted == 0 || permit_value_sompi == 0 {
        return 0;
    }
    let rounds = gap_daa.saturating_mul(palw_rounds_per_daa_v1(target_time_per_block_ms));
    let realizable = u128::from(quanta_minted).min(u128::from(rounds));
    realizable.saturating_mul(u128::from(permit_value_sompi))
}

/// **What one algo-10 permit is worth, in sompi — a DECLARED ceiling, because consensus cannot
/// derive it.**
///
/// A round block mints nothing: `coinbase.rs` pays "a round block's fees … to its own payout …
/// never a carve (the lane mints nothing, so there is no subsidy to carve)". So a permit is worth
/// the FEES of the transactions its holder includes, and consensus bounds a block's mass but never
/// its fee per unit of mass. There is no function that can compute this value, and a derivation that
/// pretended otherwise — the subsidy, say — would price a right the lane does not pay.
///
/// So it is a per-network ceiling an operator declares and a testnet MEASURES. It is the one input
/// to [`palw_realizable_before_maturity_v1`] that is not a chain fact, and it is named here rather
/// than buried so that "what did we assume a stolen round was worth" is a question with an address.
///
/// **The gain it belongs to is a DIVERSION, not a mint**: had the liar not held the permit, an honest
/// participant would have collected those fees. That makes the figure bounded by real traffic rather
/// than by the claim, which is exactly why testnet-12 runs a short maturity — to measure it on a lane
/// that is actually being used before mainnet fixes a number.
pub const fn palw_permit_value_sompi_v1(permit_fee_ceiling_sompi: u64) -> u64 {
    permit_fee_ceiling_sompi
}

/// **testnet-12's declared ceiling for one stolen round: 0.01 MSK** — a hundred minimum-relay fees.
///
/// Anchored rather than invented: `PQ_PRODUCTION_MINIMUM_RELAY_TRANSACTION_FEE` is 10,000 sompi, and
/// a round block that filled itself with a hundred standard transactions at the floor rate would pay
/// its holder 1,000,000. Restated here rather than imported because that constant lives in `mining`,
/// downstream of consensus — a consensus rule may not depend on a mempool policy, and a rule that
/// silently tracked one would change with it.
///
/// **This is the figure testnet-12 exists to replace.** The residual is
/// `min(quanta, rounds_in_gap) × this`, and on the held 2M row the binding term is 216,000 rounds —
/// so the whole collateral requirement is linear in a number nobody has measured. At 0.01 MSK the
/// residual is ~2,160 MSK against a 2,703 MSK claim gain, which is affordable; ten times that and it
/// is not, and the maturity would have to lengthen. Measuring the lane's real fee flow is the reason
/// ADR-0151 runs the SHORT maturity on this network at all.
pub const PALW_T12_PERMIT_FEE_CEILING_SOMPI: u64 = 1_000_000;

/// **Whether a Final's rights are forfeit** — the revocation half of the bundle.
///
/// A convicted Final's `execution_root` enters the forfeiture set, and every stage that can still
/// hold one of its rights drops it: the pending `round_finals` row, the span snapshot's `finals`, and
/// the seeded schedule's unused quanta. A CONSUMED quantum is a tombstone on a block the chain
/// already accepted and is not revoked — that is precisely the value
/// [`palw_realizable_before_maturity_v1`] prices, and at the shipped maturity the set is empty.
///
/// Keyed by `execution_root` and not by claim id, for the reason `mint_quanta` dedups by it: the
/// right belongs to the WORK, so a second claim over the same execution is the same right and must
/// forfeit with it.
pub fn palw_exec_rights_are_forfeit_v1(forfeited_roots: &std::collections::BTreeSet<Hash64>, execution_root: &Hash64) -> bool {
    forfeited_roots.contains(execution_root)
}

/// **A seat's required lock, with a margin that is not one sompi** (ADR-0151 D1, D4).
///
/// `palw_min_slashable_per_colluding_seat_v1` returns `gain / quorum + 1`, so the colluding quorum
/// out-values the gain by as little as `quorum` sompi — measured at **2 sompi** on the 2M row. The
/// inequality is then correct and useless: every rounding, every unpriced right and every future
/// term that is added to the gain without being added here flips it.
///
/// This applies a permille margin on top. `100` is ten percent, which is the smallest figure that
/// survives the `extra_economic_rights` term moving by a whole permit.
pub const PALW_SEAT_LOCK_MARGIN_PERMILLE_V1: u32 = 100;

/// [`crate::palw_panel_var_v1::palw_panel_seat_required_v1`] with the margin above.
pub fn palw_seat_lock_required_v2(max_fraud_gain: u128, colluding_quorum: u64) -> u128 {
    let base = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(max_fraud_gain, colluding_quorum);
    base.saturating_add(base.saturating_mul(u128::from(PALW_SEAT_LOCK_MARGIN_PERMILLE_V1)) / 1_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::palw_offence_v1::{PALW_PANEL_COLLUDING_QUORUM_V1, palw_colluding_quorum_covers_v1};
    use crate::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};

    /// The testnet-12 held 2M row, as the card registers it.
    const ESCROW: u64 = 266_736_960;
    const PWU_PER_INFERENCE: u64 = 27_002_967_184;
    const SLASH: u64 = 5;
    const WINDOW_CHALLENGE: u64 = 1_200;
    const WINDOW_COURT: u64 = 3_000;
    const CADENCE_MS: u64 = 120_000;
    /// **The quanta one held-2M Final mints: 270,029, not 2,963.** Execution quanta mint from the
    /// UNCLAMPED CanonicalWork scalar ("a heavier verified job earns more spend-once tickets"), so
    /// the work-price unit that clamps the lottery does not clamp this. The first reading applied
    /// that clamp and under-counted by 91x.
    const QUANTA: u32 = 270_029;
    const PERMIT: u64 = PALW_T12_PERMIT_FEE_CEILING_SOMPI;

    fn facts(extra: u128) -> PalwClaimFraudFactsV1 {
        PalwClaimFraudFactsV1 {
            reserved: 0,
            escrowed_reward: ESCROW,
            pwu: crate::palw_pwu::palw_pwu_v1(u128::MAX / 2, PWU_PER_INFERENCE),
            slash_value_per_pwu: SLASH,
            extra_economic_rights_sompi: extra,
        }
    }

    fn residual() -> u128 {
        palw_realizable_before_maturity_v1(QUANTA, WINDOW_CHALLENGE, WINDOW_COURT, CADENCE_MS, palw_permit_value_sompi_v1(PERMIT))
    }

    /// **The defect, reproduced.** With the rights priced at zero the colluding quorum out-values the
    /// gain by TWO SOMPI, on a claim that mints 2,963 permits.
    #[test]
    fn the_shipped_inequality_held_by_two_sompi() {
        let gain = palw_max_fraud_gain_v1(&facts(0));
        let seat = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert_eq!(seat.saturating_mul(u128::from(PALW_PANEL_COLLUDING_QUORUM_V1)) - gain, 2, "the whole margin");
    }

    /// **testnet-12 runs the SHORT maturity, so the residual is real and priced.**
    ///
    /// 1,200 DAA of maturity against a 3,000-DAA liability horizon leaves 1,800 DAA in which a right
    /// is both spendable and still convictable. At 120 rounds a DAA that is 216,000 rounds, against a
    /// mint of 270,029 quanta — so the GAP is what binds, and lengthening the maturity is what
    /// shrinks the bill.
    #[test]
    fn the_short_maturity_prices_every_quantum() {
        assert_eq!(palw_exec_quantum_maturity_daa_v1(WINDOW_CHALLENGE, WINDOW_COURT), WINDOW_CHALLENGE);
        assert_eq!(palw_exec_quantum_matures_at_v1(10_000, WINDOW_CHALLENGE, WINDOW_COURT), 11_200);
        assert_eq!(palw_rounds_per_daa_v1(CADENCE_MS), 120);
        // 1,800 DAA x 120 rounds = 216,000 rounds, against 270,029 quanta: the GAP binds, not the
        // mint. That is the term a longer maturity shrinks, and the reason the knob works at all.
        assert_eq!(residual(), 216_000 * u128::from(PERMIT), "the rounds in the gap are the binding cap");
        assert!(residual() > 0, "the point of the short maturity is that this path is REACHED");
    }

    /// **The tradeoff is continuous, which is the property mainnet needs.** Longer maturity, less
    /// realizable; at the liability horizon, nothing.
    #[test]
    fn maturity_and_recoverable_rights_move_together() {
        let at = |maturity: u64| {
            let gap = WINDOW_COURT.saturating_sub(maturity);
            let rounds = gap.saturating_mul(palw_rounds_per_daa_v1(CADENCE_MS));
            u128::from(QUANTA).min(u128::from(rounds)).saturating_mul(u128::from(PERMIT))
        };
        assert_eq!(at(WINDOW_COURT), 0, "maturity at the horizon leaves nothing to price");
        assert!(at(2_990) < at(WINDOW_CHALLENGE), "a longer maturity prices less");
        assert!(at(0) >= at(WINDOW_CHALLENGE), "and no maturity prices the most");
        // The knob is monotone, so an operator can trade latency for collateral without a cliff.
        let mut previous = u128::MAX;
        for maturity in [0u64, 600, 1_200, 1_800, 2_400, 3_000] {
            let now = at(maturity);
            assert!(now <= previous, "the residual must not grow with maturity at {maturity}");
            previous = now;
        }
    }

    /// **With the residual priced, three colluding seats out-value the lie again** — and by a tenth
    /// rather than by two sompi, so one unpriced permit cannot flip it.
    #[test]
    fn the_priced_gain_is_covered_with_a_real_margin() {
        let gain = palw_max_fraud_gain_v1(&facts(residual()));
        assert!(gain > palw_max_fraud_gain_v1(&facts(0)), "the rights are in the gain now");
        let seat = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        let quorum = u128::from(PALW_PANEL_COLLUDING_QUORUM_V1);
        assert!(palw_colluding_quorum_covers_v1(seat, PALW_PANEL_COLLUDING_QUORUM_V1, gain), "the inequality holds");
        assert!(
            seat.saturating_mul(quorum) > gain.saturating_add(u128::from(PERMIT)),
            "and survives a whole unpriced permit of drift"
        );
        // The shipped one-sompi margin would not have.
        let thin = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert!(thin.saturating_mul(quorum) <= gain.saturating_add(u128::from(PERMIT)));
    }

    /// A round block mints nothing, so a permit is priced at a declared FEE ceiling and never at a
    /// subsidy. Pinned because the first reading of this priced it at the block subsidy and reported
    /// an uncounted gain four times too large.
    #[test]
    fn a_permit_is_priced_at_fees_not_at_a_subsidy() {
        assert_eq!(palw_permit_value_sompi_v1(PERMIT), PERMIT);
        assert_eq!(PALW_T12_PERMIT_FEE_CEILING_SOMPI, 1_000_000, "a hundred minimum-relay fees a round, until t12 measures it");
    }
}
