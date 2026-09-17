//! ADR-0124 — the panel is paid out of the claim's own reward, a seat holds exposure while it
//! judges, and a claim is paid for the compute it certifies.
//!
//! Three pure rules, each a function of numbers the fold already holds, so the transition, the
//! acceptance layer and a reader recomputing a payout from the chain cannot disagree:
//!
//! * **The split.** A `Final` claim's reward `R` (the escrow after ADR-0091's buyback) is
//!   `producer + panel_pool`, with the pool [`PALW_PANEL_POOL_PERMILLE_V1`] of `R`. The pool is
//!   divided by the seats the panel was DRAWN with, never by the seats that answered — a seat's
//!   pay does not rise when a neighbour is silent — and only a seat whose `Valid` receipt the
//!   chain credited before the receipt deadline is paid. What the pool does not pay goes to the
//!   panel reserve (`PalwChainStateV2::panel_reserve_sompi`), never back to the producer: a
//!   producer that could keep an omitted seat's share would have a reason to omit it.
//!   [`palw_panel_split_v1`] is the whole arithmetic and it sums exactly.
//!
//! * **The exposure.** A drawn seat reserves [`PALW_SEAT_EXPOSURE_MULTIPLE_V1`] × the claim's own
//!   `reserved` on its bond for the claim's whole life, and a seat convicted of contradicting its
//!   panel's quorum loses exactly that. What is reserved is what is slashable; a floor that is
//!   merely a balance the seat must hold is not at risk on any one claim. A bond is drawn only
//!   while its free collateral covers the reservation ([`palw_seat_has_headroom_v1`]), and the
//!   collateral a bond must hold to be drawn at all is [`PALW_PANEL_COLLATERAL_MULTIPLE_V1`]
//!   producer floors ([`palw_panel_collateral_floor_v1`]) — on a mainnet card whose producer floor
//!   is 10,000 MSK that is the operator's 100,000 MSK.
//!
//! * **The price of work.** The escrow a block withheld is the schedule's full carve; at `Final`
//!   the claim is paid the fraction of it that its class's canonical inference is of the heaviest
//!   weight-bearing model class's ([`palw_work_priced_reward_v1`]), and the rest is never named
//!   as a payout — never minted, exactly as a voided claim's escrow is. The pwu a claim is paid
//!   on is the pwu its exposure is priced on (`palw_exposure_pwu_v1`): a claim is paid on the
//!   same number it can be slashed on. The liveness floor is not a model and is not priced.
//!
//! * **The exposure floor (ADR-0130).** Past `Params::palw_panel_exposure_floor` a seat reserves
//!   the larger of the claim-priced stake above and `λ` × the most the seat can be paid on the claim
//!   ([`palw_panel_seat_exposure_v1`]), so a seat's reward can never outgrow what it stands to lose
//!   by a factor the operator did not choose. The reservation is written into the duty row when the
//!   panel binds and every release and every dissent slash reads the row, never a recomputation.
//!
//! Nothing here reads a fence. The fold decides, at the block's own DAA, whether each rule is in
//! force (`PalwTransitionExtrasV1::panel_economy_active`, `::work_priced_reward_active`,
//! `::panel_reward_multiple_permille`), and a network that never arms them folds byte-identically to
//! one built before this module existed.
//!
//! **Every function here is pure and callable from a state snapshot**, which is what a node-local
//! shadow read (what a `λ` WOULD require, never enforced) is built from: the seat exposure
//! ([`palw_panel_seat_exposure_v1`]), the headroom predicate ([`palw_seat_has_headroom_v1`]), the
//! stored and the hypothetical seat ledgers ([`palw_seat_exposure_ledger_v1`],
//! [`palw_shadow_seat_exposure_ledger_v1`]) and what a bond would back under a hypothetical `λ`
//! ([`palw_shadow_backed_v1`]).

use crate::palw_state_v2::{PalwBondKeyV2, PalwChainStateV2};
use std::collections::BTreeMap;

/// The panel's share of a `Final` claim's reward, in permille — ADR-0124 Decision 1: 80 % to
/// the producer, 20 % to the panel pool. A starting value the operator asked to measure against
/// (raise it if seats are scarce, lower it if producers are), not a derived one.
pub const PALW_PANEL_POOL_PERMILLE_V1: u64 = 200;

/// What a drawn seat reserves, as a multiple of the claim's own `reserved` — ADR-0124 Decision 3.
/// Three seats of a five-seat panel at 3× each put 9× the claim's exposure behind a corrupt quorum,
/// 10× with the producer's own, which is the "ten times the claim" the operator asked for
/// without pricing any one seat at ten.
pub const PALW_SEAT_EXPOSURE_MULTIPLE_V1: u128 = 3;

/// The collateral a bond must hold to be drawn as a seat, as a multiple of the network's producer
/// floor — ADR-0124 Decision 4. One rule, and the operator's mainnet numbers (a 10,000 MSK
/// producer, a 100,000 MSK panel operator) fall out of it.
pub const PALW_PANEL_COLLATERAL_MULTIPLE_V1: u64 = 10;

/// The collateral a bond must hold to be drawn as a seat: ten producer floors, saturating.
pub fn palw_panel_collateral_floor_v1(min_collateral_sompi: u64) -> u64 {
    min_collateral_sompi.saturating_mul(PALW_PANEL_COLLATERAL_MULTIPLE_V1)
}

/// What one seat reserves on one claim below ADR-0130's floor, saturating.
pub fn palw_seat_exposure_v1(claim_reserved: u128) -> u128 {
    claim_reserved.saturating_mul(PALW_SEAT_EXPOSURE_MULTIPLE_V1)
}

/// **ADR-0130: the largest reward multiple a network may state**, in permille — 100×. A floor past
/// it would price a seat out of every panel on any real collateral distribution; `validate_palw_v2`
/// refuses it before a peer is dialed.
pub const PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1: u32 = 100_000;

/// **ADR-0130: what one seat reserves on one claim** —
/// `max(3 × claim.reserved, ⌊λ‰ × max_seat_reward / 1000⌋)`, saturating.
///
/// `max_seat_reward` is [`palw_panel_split_v1`]`(escrowed_reward, seat_count, _).per_seat`: the pool
/// share of the escrow the claim holds, divided by the seats the panel is drawn with. It is the MOST
/// a seat can be paid on this claim — the work price and the buyback only lower the reward the pool
/// is carved from — so a floor on it bounds every payout the seat can receive.
///
/// `reward_multiple_permille == 0` is "no floor" and is [`palw_seat_exposure_v1`] exactly, which is
/// what every network that has not armed `Params::palw_panel_exposure_floor` reserves. Pure: the
/// draw's headroom, the fold's reservation and a shadow reader asking what a hypothetical `λ` would
/// require all call this one function with the numbers they hold.
pub fn palw_panel_seat_exposure_v1(
    claim_reserved: u128,
    escrowed_reward: u64,
    seat_count: usize,
    reward_multiple_permille: u32,
) -> u128 {
    let stake = palw_seat_exposure_v1(claim_reserved);
    if reward_multiple_permille == 0 {
        return stake;
    }
    let max_seat_reward = palw_panel_split_v1(escrowed_reward, seat_count, 0).per_seat;
    let floor = (max_seat_reward as u128).saturating_mul(reward_multiple_permille as u128) / 1000;
    stake.max(floor)
}

/// **A seat may be drawn only while its free collateral covers what the seat would reserve.**
///
/// `backed` is everything the bond already stands behind — its live claims, its registrations,
/// its open accusations and courts, and the seats it already holds — and the ceiling is the same
/// `fp_max_exposure_ratio_permille` of the collateral that every other reservation on the bond
/// lives under (`reserve_accuser_exposure_v2`). One ceiling, so a bond cannot double-use the
/// collateral behind a claim it produces as the collateral behind a claim it judges.
pub fn palw_seat_has_headroom_v1(collateral: u64, backed: u128, seat_exposure: u128, max_exposure_ratio_permille: u32) -> bool {
    let ceiling = (collateral as u128).saturating_mul(max_exposure_ratio_permille as u128) / 1000;
    backed.saturating_add(seat_exposure) <= ceiling
}

/// The numbers the draw needs past the fence, resolved by the processor at the claim's ANCHOR
/// (where every other rule that decides a panel is resolved) and carried to the draw and to the
/// acceptance layer as one value, so both recompute one panel.
///
/// A caller may also build one by hand — a shadow reader asking whether a claim's panel WOULD be
/// drawable under a hypothetical `reward_multiple_permille` passes it to
/// `palw_panel_v2::derive_panel_v2_with_policy` against a state snapshot, and nothing is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSeatEconomyV1 {
    /// [`palw_panel_collateral_floor_v1`] of the network's producer floor.
    pub panel_floor_sompi: u64,
    /// The state params' `fp_max_exposure_ratio_permille`, the ceiling every reservation shares.
    pub max_exposure_ratio_permille: u32,
    /// ADR-0130: `Params::palw_panel_exposure_floor`'s `λ` where it is active at the anchor, `0`
    /// (no floor) where it is not — the fourth argument of [`palw_panel_seat_exposure_v1`].
    pub reward_multiple_permille: u32,
}

impl PalwSeatEconomyV1 {
    /// What a seat drawn on a claim holding `claim_reserved` and `escrowed_reward` would reserve on a
    /// panel of `seat_count` seats under this economy.
    pub fn seat_exposure(&self, claim_reserved: u128, escrowed_reward: u64, seat_count: usize) -> u128 {
        palw_panel_seat_exposure_v1(claim_reserved, escrowed_reward, seat_count, self.reward_multiple_permille)
    }
}

/// **The seat exposure every bond holds on the live duty rows, as the rows STORE it** — the seat
/// half of `reserved_exposure`, per bond. What the fold reserved when each panel bound and what it
/// will release; `PalwChainStateV2::assert_internal_consistency` rebuilds the same sum.
pub fn palw_seat_exposure_ledger_v1(state: &PalwChainStateV2) -> BTreeMap<PalwBondKeyV2, u128> {
    let mut ledger: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
    for (_, row) in state.panel_duty_rows_iter() {
        for seat in row.seats.keys() {
            let held = ledger.entry(*seat).or_insert(0);
            *held = held.saturating_add(row.seat_exposure);
        }
    }
    ledger
}

/// **The shadow ledger: the seat exposure every bond WOULD hold on the live duty rows had each
/// panel reserved under `reward_multiple_permille`** — [`palw_panel_seat_exposure_v1`] of each
/// row's claim, over the row's own seat count, summed per seat bond. Pure and snapshot-only: no
/// incremental bookkeeping, nothing written, and `0` reproduces the stored ledger of a network that
/// never armed a floor.
pub fn palw_shadow_seat_exposure_ledger_v1(state: &PalwChainStateV2, reward_multiple_permille: u32) -> BTreeMap<PalwBondKeyV2, u128> {
    let mut ledger: BTreeMap<PalwBondKeyV2, u128> = BTreeMap::new();
    for (claim_id, row) in state.panel_duty_rows_iter() {
        // A row names a live claim (the consistency check refuses a state where it does not), so
        // the lookup only misses on a state no fold wrote; such a row contributes nothing.
        let Some(claim) = state.claim(claim_id) else { continue };
        let per_seat = palw_panel_seat_exposure_v1(claim.reserved, claim.escrowed_reward, row.seats.len(), reward_multiple_permille);
        for seat in row.seats.keys() {
            let held = ledger.entry(*seat).or_insert(0);
            *held = held.saturating_add(per_seat);
        }
    }
    ledger
}

/// **What `bond` would back under `reward_multiple_permille`** — the `backed` argument of
/// [`palw_seat_has_headroom_v1`] as the draw computes it (`reserved_exposure + registration_exposure`),
/// with the seat half replaced by the shadow ledger's. A shadow reader's capacity is then
/// `ceiling − palw_shadow_backed_v1(..)`, with `ceiling = collateral × fp_max_exposure_ratio_permille / 1000`.
pub fn palw_shadow_backed_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2, reward_multiple_permille: u32) -> u128 {
    let stored = palw_seat_exposure_ledger_v1(state).get(bond).copied().unwrap_or(0);
    let shadow = palw_shadow_seat_exposure_ledger_v1(state, reward_multiple_permille).get(bond).copied().unwrap_or(0);
    state.reserved_exposure(bond).saturating_sub(stored).saturating_add(shadow).saturating_add(state.registration_exposure(bond))
}

/// **The price of work: the escrow times the fraction the claim's inference is of the unit's,
/// never above the escrow.**
///
/// `exposure_pwu` is the claim's `palw_exposure_pwu_v1` (one canonical inference of its class,
/// or the claimed pwu under `MaxPerAttempt`); `unit_pwu` is the heaviest such value among the
/// weight-bearing model classes at the paying block. A claim at or above the unit is paid the
/// whole escrow; a lighter one proportionally less, floored at zero; a unit of zero (no model
/// class bears weight) prices nothing and pays the escrow whole. Integer, exact, monotone in the
/// pwu, and never more than `escrow` — so the schedule the accepting block withheld against is
/// never exceeded by construction.
pub fn palw_work_priced_reward_v1(escrow: u64, exposure_pwu: u64, unit_pwu: u64) -> u64 {
    if unit_pwu == 0 || exposure_pwu >= unit_pwu {
        return escrow;
    }
    ((escrow as u128).saturating_mul(exposure_pwu as u128) / unit_pwu as u128) as u64
}

/// One `Final` reward, split. `producer + per_seat × credited + reserve == reward`, always — the
/// type exists so no caller re-derives one side and mints a sompi the schedule does not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwPanelSplitV1 {
    /// The producer's exact rest once the pool is carved: a function of the reward alone, so no
    /// seat's silence and no division's dust ever moves a sompi toward the producer.
    pub producer: u64,
    /// The pool divided by the seats the panel was drawn with; zero when the panel had no seats.
    pub per_seat: u64,
    /// How many seats were credited, capped at the seat count.
    pub credited: u64,
    /// `per_seat × credited` — what the seats are actually named.
    pub paid: u64,
    /// The pool less what was paid: uncredited seats' shares and the division's dust. Never the
    /// producer's, by construction — a producer that could keep an omitted seat's share would
    /// have a reason to omit it.
    pub reserve: u64,
}

/// Split a `Final` claim's reward between the producer, the credited seats and the reserve.
///
/// The pool is `⌊reward × PALW_PANEL_POOL_PERMILLE_V1 / 1000⌋`; the producer is named the exact
/// rest. The pool is divided by `seat_count` — the DRAWN panel — so a seat's share is fixed at
/// binding and does not rise when a neighbour stays silent (Decision 2). `credited` above
/// `seat_count` is clamped: the fold credits a seat once and only a drawn seat, so this is a
/// belt over braces, never a path.
pub fn palw_panel_split_v1(reward: u64, seat_count: usize, credited: usize) -> PalwPanelSplitV1 {
    let pool = ((reward as u128) * (PALW_PANEL_POOL_PERMILLE_V1 as u128) / 1000) as u64;
    let producer = reward - pool;
    let per_seat = if seat_count == 0 { 0 } else { pool / seat_count as u64 };
    let credited = credited.min(seat_count) as u64;
    let paid = per_seat * credited;
    PalwPanelSplitV1 { producer, per_seat, credited, paid, reserve: pool - paid }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// testnet-11's attempt escrow: 62 % of the 4,445.62 MSK block (`YEAR1_PER_BLOCK_TWO_MINUTE`),
    /// the whole worker share — the 2,756.28 MSK every attempt block withholds.
    const T11_ESCROW: u64 = 275_628_448_680;
    /// `pwu_per_inference` of the shipped testnet-11 classes (`palw_class_daa.rs`'s table).
    const T11_FLOOR_PWU: u64 = 7_708;
    const T11_A16_PWU: u64 = 1_589_424;
    const T11_QWEN36_PWU: u64 = 2_685_360;

    #[test]
    fn the_split_sums_exactly_and_the_producer_never_takes_a_seats_share() {
        for reward in [0u64, 1, 4, 5, 999, 1_000, T11_ESCROW, u64::MAX / 2] {
            for seats in [0usize, 1, 3, 5, 8] {
                for credited in 0..=seats + 1 {
                    let s = palw_panel_split_v1(reward, seats, credited);
                    assert_eq!(
                        s.producer as u128 + s.paid as u128 + s.reserve as u128,
                        reward as u128,
                        "reward {reward} seats {seats} credited {credited}: the split is exact"
                    );
                    assert_eq!(s.paid, s.per_seat * s.credited);
                    assert!(s.credited as usize <= seats, "a credit above the draw is clamped");
                    // The producer is a function of the reward alone: the seats' answers move
                    // nothing between the producer and the panel.
                    assert_eq!(s.producer, reward - ((reward as u128) * 200 / 1000) as u64);
                }
            }
        }
    }

    #[test]
    fn five_seats_three_credited_on_the_testnet_11_block() {
        // 80 % of the escrow to the producer; the pool of 20 % is five equal seats, three of which
        // answered; the two silent shares and the dust go to the reserve.
        let s = palw_panel_split_v1(T11_ESCROW, 5, 3);
        assert_eq!(s.producer, 220_502_758_944);
        assert_eq!(s.per_seat, 11_025_137_947);
        assert_eq!(s.paid, 33_075_413_841);
        assert_eq!(s.reserve, 22_050_275_895);
        assert_eq!(s.producer + s.paid + s.reserve, T11_ESCROW);
        // All five credited: nothing left for the reserve but the dust of `pool / 5`.
        let all = palw_panel_split_v1(T11_ESCROW, 5, 5);
        assert_eq!(all.reserve, T11_ESCROW / 5 - all.per_seat * 5);
        assert!(all.reserve < 5);
        // Nobody credited: the whole pool is reserve, and the producer is exactly what it was.
        let none = palw_panel_split_v1(T11_ESCROW, 5, 0);
        assert_eq!((none.paid, none.reserve), (0, 55_125_689_736));
        assert_eq!(none.producer, s.producer);
    }

    #[test]
    fn the_work_price_pays_the_unit_whole_and_the_lighter_class_its_fraction() {
        // The heaviest weight-bearing class is the unit: paid the escrow whole.
        assert_eq!(palw_work_priced_reward_v1(T11_ESCROW, T11_QWEN36_PWU, T11_QWEN36_PWU), T11_ESCROW);
        // A16 is 1,589,424 / 2,685,360 of the unit: 59.19 % of the escrow.
        let a16 = palw_work_priced_reward_v1(T11_ESCROW, T11_A16_PWU, T11_QWEN36_PWU);
        assert_eq!(a16, 163_140_313_185);
        assert_eq!(a16 as u128, (T11_ESCROW as u128) * (T11_A16_PWU as u128) / (T11_QWEN36_PWU as u128));
        // The floor's inference is 0.29 % of the unit's; the rule would price it there, which is
        // why the fold does not price the floor at all (Decision 6).
        assert_eq!(palw_work_priced_reward_v1(T11_ESCROW, T11_FLOOR_PWU, T11_QWEN36_PWU), 791_158_013);
        // Above the unit is capped at the escrow: the schedule is never exceeded.
        assert_eq!(palw_work_priced_reward_v1(T11_ESCROW, T11_QWEN36_PWU * 100, T11_QWEN36_PWU), T11_ESCROW);
        // No unit (no model class bears weight) prices nothing.
        assert_eq!(palw_work_priced_reward_v1(T11_ESCROW, 1, 0), T11_ESCROW);
        // Zero work is paid nothing.
        assert_eq!(palw_work_priced_reward_v1(T11_ESCROW, 0, T11_QWEN36_PWU), 0);
        // Monotone in the pwu, never above the escrow.
        let mut last = 0;
        for pwu in (0..=T11_QWEN36_PWU).step_by(97_531) {
            let paid = palw_work_priced_reward_v1(T11_ESCROW, pwu, T11_QWEN36_PWU);
            assert!(paid >= last && paid <= T11_ESCROW);
            last = paid;
        }
    }

    #[test]
    fn the_exposure_and_the_floor_are_multiples_and_saturate() {
        assert_eq!(palw_seat_exposure_v1(100), 300);
        assert_eq!(palw_seat_exposure_v1(u128::MAX), u128::MAX);
        assert_eq!(palw_panel_collateral_floor_v1(400_000), 4_000_000);
        // The operator's mainnet numbers: a 10,000 MSK producer floor is a 100,000 MSK panel floor.
        assert_eq!(
            palw_panel_collateral_floor_v1(10_000 * crate::constants::SOMPI_PER_KASPA),
            100_000 * crate::constants::SOMPI_PER_KASPA
        );
        assert_eq!(palw_panel_collateral_floor_v1(u64::MAX), u64::MAX);
    }

    /// **ADR-0130 on testnet-11's numbers.** Past DAA 6,001 an attempt escrows 72 % of the 4,445.62 MSK
    /// block — 3,200.85 MSK — and a QWEN36 claim reserves 5 sompi/pwu × 2,685,360 pwu = 0.134 MSK, so a
    /// seat of a five-seat panel is paid up to 128.03 MSK while it risks 3 × 0.134 = 0.40 MSK. At
    /// `λ = 2` the seat reserves twice its most, 256.07 MSK, and the claim-priced stake no longer binds.
    ///
    /// The brief that asked for this quoted "per seat 64,016,930,016 sompi" and "λ = 2000 ‰ →
    /// 128,033,860,032 sompi": those are the POOL (20 % of the escrow) and twice the pool, i.e. the
    /// share of a ONE-seat panel. Both readings are pinned — the formula is the rule, and the one-seat
    /// row is where the brief's figures come from — and the five-seat row is testnet-11's.
    #[test]
    fn adr0130_the_seat_exposure_floor_on_testnet_11s_numbers() {
        const T11_ESCROW_6001: u64 = 320_084_650_080;
        const T11_QWEN36_RESERVED: u128 = 13_426_800;
        assert_eq!(T11_QWEN36_RESERVED, 5 * T11_QWEN36_PWU as u128, "5 sompi a pwu × one QWEN36 inference");
        assert_eq!(palw_seat_exposure_v1(T11_QWEN36_RESERVED), 40_280_400, "3 × 13,426,800: the 0.40 MSK a seat risks today");

        // The brief's figures: the pool, and λ = 2 of it — a one-seat panel.
        assert_eq!(palw_panel_split_v1(T11_ESCROW_6001, 1, 0).per_seat, 64_016_930_016);
        assert_eq!(palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 1, 2_000), 128_033_860_032);

        // testnet-11's five-seat panel: 128.03 MSK a seat, 256.07 MSK reserved at λ = 2.
        assert_eq!(palw_panel_split_v1(T11_ESCROW_6001, 5, 0).per_seat, 12_803_386_003);
        let t11 = palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 5, 2_000);
        assert_eq!(t11, 25_606_772_006);
        assert!(t11 > palw_seat_exposure_v1(T11_QWEN36_RESERVED), "the floor binds: 256.07 MSK against 0.40 MSK");
        // The mainnet range the operator is considering: 5× and 10× the seat's most.
        assert_eq!(palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 5, 5_000), 64_016_930_015);
        assert_eq!(palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 5, 10_000), 128_033_860_030);

        // No floor is today's stake exactly, whatever the escrow.
        for escrow in [0, 1, T11_ESCROW, T11_ESCROW_6001, u64::MAX] {
            for seats in [0usize, 1, 5] {
                assert_eq!(palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, escrow, seats, 0), 40_280_400);
            }
        }
        // A claim-priced stake above the floor is kept: the floor only raises a reservation.
        assert_eq!(palw_panel_seat_exposure_v1(10_000_000_000_000, T11_ESCROW_6001, 5, 2_000), 30_000_000_000_000);
        // A panel of no seats has no seat reward to floor.
        assert_eq!(palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 0, 2_000), 40_280_400);
        // Monotone in λ, and saturating rather than wrapping.
        let mut last = 0;
        for lambda in (0..=PALW_PANEL_REWARD_MULTIPLE_MAX_PERMILLE_V1).step_by(2_500) {
            let e = palw_panel_seat_exposure_v1(T11_QWEN36_RESERVED, T11_ESCROW_6001, 5, lambda);
            assert!(e >= last);
            last = e;
        }
        assert_eq!(palw_panel_seat_exposure_v1(u128::MAX, u64::MAX, 1, u32::MAX), u128::MAX);
        // The economy value carries λ to the same function.
        let economy = PalwSeatEconomyV1 { panel_floor_sompi: 0, max_exposure_ratio_permille: 500, reward_multiple_permille: 2_000 };
        assert_eq!(economy.seat_exposure(T11_QWEN36_RESERVED, T11_ESCROW_6001, 5), t11);
    }

    #[test]
    fn headroom_is_the_shared_ceiling_and_counts_what_the_bond_already_backs() {
        // 1,000 sompi of collateral at a 500‰ ceiling backs 500.
        assert!(palw_seat_has_headroom_v1(1_000, 0, 500, 500));
        assert!(!palw_seat_has_headroom_v1(1_000, 0, 501, 500));
        // A bond producing (300 backed) can hold one seat at 200 but not at 201.
        assert!(palw_seat_has_headroom_v1(1_000, 300, 200, 500));
        assert!(!palw_seat_has_headroom_v1(1_000, 300, 201, 500));
        // Saturating, never wrapping into a false yes.
        assert!(!palw_seat_has_headroom_v1(1_000, u128::MAX, 1, 500));
        assert!(!palw_seat_has_headroom_v1(u64::MAX, u128::MAX - 1, u128::MAX, 1000));
    }
}
