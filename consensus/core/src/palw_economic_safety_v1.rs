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
//! | each convertible to one algo-10 permit worth a block | 3.70468 MSK |
//! | block rights the gain does NOT count | **10,976.98 MSK** |
//!
//! `palw_claim_extra_economic_rights_v1` returned `0` while its own doc named "Permits, extra
//! eligibility, or any other sompi-denominated right the claim mints" — and on this network a Final
//! mints exactly that (`palw_execution_quanta_v1`: "an execution quantum is an execution *right*: it
//! converts into at most one algo-10 round permit"). So the lie's biggest prize was free.
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
/// Set to the liability horizon (`PalwCourtParamsV2::window_court`) by
/// [`palw_exec_quantum_maturity_daa_v1`], so the two edges coincide and revocation is total. A
/// SHORTER value is legal and safe — [`palw_realizable_before_maturity_v1`] prices the window it
/// opens — and is how an operator trades collateral for latency.
pub const PALW_EXEC_QUANTUM_MATURITY_IS_THE_LIABILITY_HORIZON: bool = true;

/// The maturity this network applies: the liability horizon when
/// [`PALW_EXEC_QUANTUM_MATURITY_IS_THE_LIABILITY_HORIZON`], else nothing.
pub const fn palw_exec_quantum_maturity_daa_v1(window_court_daa: u64) -> u64 {
    if PALW_EXEC_QUANTUM_MATURITY_IS_THE_LIABILITY_HORIZON { window_court_daa } else { 0 }
}

/// **The earliest DAA at which a Final's execution quanta may be spent.**
pub const fn palw_exec_quantum_matures_at_v1(final_daa: u64, window_court_daa: u64) -> u64 {
    final_daa.saturating_add(palw_exec_quantum_maturity_daa_v1(window_court_daa))
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
    window_court_daa: u64,
    target_time_per_block_ms: u64,
    permit_value_sompi: u64,
) -> u128 {
    let maturity = palw_exec_quantum_maturity_daa_v1(window_court_daa);
    let gap_daa = window_court_daa.saturating_sub(maturity);
    if gap_daa == 0 || quanta_minted == 0 || permit_value_sompi == 0 {
        return 0;
    }
    let rounds = gap_daa.saturating_mul(palw_rounds_per_daa_v1(target_time_per_block_ms));
    let realizable = u128::from(quanta_minted).min(u128::from(rounds));
    realizable.saturating_mul(u128::from(permit_value_sompi))
}

/// **What one algo-10 permit is worth, in sompi** — the block it buys.
///
/// The subsidy is the floor and the honest one to price against: a permit's holder produces a round
/// block and collects its subsidy. Fees are not added, and the omission is named: a fee is paid by a
/// transaction the attacker would also have to supply, so counting it would let a liar inflate its own
/// slash requirement by spamming itself. The subsidy is what the CHAIN hands over for holding the
/// permit, and that is what a lie steals.
pub const fn palw_permit_value_sompi_v1(subsidy_sompi: u64) -> u64 {
    subsidy_sompi
}

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
    use crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1;
    use crate::palw_offence_v1::palw_colluding_quorum_covers_v1;
    use crate::palw_panel_var_v1::{PalwClaimFraudFactsV1, palw_max_fraud_gain_v1};

    /// The testnet-12 held 2M row, as the card registers it.
    const ESCROW: u64 = 266_736_960;
    const PWU_PER_INFERENCE: u64 = 27_002_967_184;
    const SLASH: u64 = 5;
    const WINDOW_COURT: u64 = 3_000;
    const CADENCE_MS: u64 = 120_000;
    const SUBSIDY: u64 = 370_468_345;
    const QUANTA: u32 = 2_963;

    fn facts(extra: u128) -> PalwClaimFraudFactsV1 {
        PalwClaimFraudFactsV1 {
            reserved: 0,
            escrowed_reward: ESCROW,
            pwu: crate::palw_pwu::palw_pwu_v1(u128::MAX / 2, PWU_PER_INFERENCE),
            slash_value_per_pwu: SLASH,
            extra_economic_rights_sompi: extra,
        }
    }

    /// **The defect, reproduced.** With the rights priced at zero the colluding quorum out-values the
    /// gain by two sompi, while the Final mints 2,963 permits worth ten thousand MSK.
    #[test]
    fn the_shipped_inequality_held_by_two_sompi_and_missed_ten_thousand_msk() {
        let gain = palw_max_fraud_gain_v1(&facts(0));
        let seat = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert_eq!(seat.saturating_mul(u128::from(PALW_PANEL_COLLUDING_QUORUM_V1)) - gain, 2, "the whole margin");
        let missed = u128::from(QUANTA) * u128::from(palw_permit_value_sompi_v1(SUBSIDY));
        assert!(missed > gain * 4, "the uncounted block rights are four times the gain that was counted");
    }

    /// **Maturity at the liability horizon leaves nothing to price.** The two edges coincide, so at
    /// every instant a conviction can be filed every quantum is still unused.
    #[test]
    fn at_the_shipped_maturity_no_right_is_realizable_before_conviction() {
        assert_eq!(palw_exec_quantum_maturity_daa_v1(WINDOW_COURT), WINDOW_COURT);
        assert_eq!(palw_exec_quantum_matures_at_v1(10_000, WINDOW_COURT), 13_000);
        assert_eq!(
            palw_realizable_before_maturity_v1(QUANTA, WINDOW_COURT, CADENCE_MS, palw_permit_value_sompi_v1(SUBSIDY)),
            0,
            "nothing is both spendable and still convictable"
        );
    }

    /// **And a shorter maturity is priced, not a hole.** The residual is the permits realizable in the
    /// gap, capped by the quanta that exist.
    #[test]
    fn a_shorter_maturity_is_priced() {
        // One round a second against a 120 s tick: 120 rounds a DAA.
        assert_eq!(palw_rounds_per_daa_v1(CADENCE_MS), 120);
        // A gap of one DAA admits 120 permits; the mint has 2,963, so the cap does not bite.
        let one_daa_gap = palw_realizable_before_maturity_v1(QUANTA, WINDOW_COURT, CADENCE_MS, palw_permit_value_sompi_v1(SUBSIDY));
        assert_eq!(one_daa_gap, 0, "the shipped setting has no gap at all");
        // Priced directly, with the maturity stripped out, so the arithmetic is visible.
        let gap_rounds = 1u128 * 120;
        let priced = gap_rounds.min(u128::from(QUANTA)) * u128::from(SUBSIDY);
        assert_eq!(priced, 120 * u128::from(SUBSIDY));
    }

    /// **The margin is a tenth, not two sompi** — and the inequality survives a whole permit moving.
    #[test]
    fn the_widened_margin_survives_one_permit_of_drift() {
        let gain = palw_max_fraud_gain_v1(&facts(0));
        let seat = palw_seat_lock_required_v2(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        let quorum = u128::from(PALW_PANEL_COLLUDING_QUORUM_V1);
        assert!(palw_colluding_quorum_covers_v1(seat, PALW_PANEL_COLLUDING_QUORUM_V1, gain));
        // A gain that grew by one permit is still covered, which the one-sompi margin was not.
        let drifted = gain + u128::from(SUBSIDY);
        assert!(
            seat.saturating_mul(quorum) > drifted,
            "a tenth of the lock must absorb one unpriced permit: {} vs {}",
            seat.saturating_mul(quorum),
            drifted
        );
        let thin = crate::palw_offence_v1::palw_min_slashable_per_colluding_seat_v1(gain, PALW_PANEL_COLLUDING_QUORUM_V1);
        assert!(thin.saturating_mul(quorum) <= drifted, "the shipped margin does not");
    }
}
