//! **ADR-0132 Protocol Upgrade C — a claim is paid for the compute it ran, at one rate.**
//!
//! Behind `Params::palw_economic_payout` (dormant on every shipped preset) a `Final` claim of a
//! model class is paid `min(escrow, attempted_ccu × rate)` — proposals **C** (a global rate instead
//! of a unit), **A** (the network draw credited: `attempted = class draws × network draws × one
//! draw's job`) and **B** (the claim snapshots its economics when it is ACCEPTED, so a later retarget,
//! registration or rate change moves no claim already in flight) of ADR-0132 §4, together — and its
//! panel is paid a share derived from the verification compute against the producer's
//! (`PanelShare = clamp(α·C_V / (C_P + α·C_V), S_min, S_max)`, the operator's design), in place of
//! the fixed fifth. What the price leaves is never named: never minted, exactly as a voided escrow
//! is (ADR-0124 Decision 6's rule, kept).
//!
//! Nothing here is a class's number: the rate, `α`, the two share bounds and the cap-utilization
//! ceiling are consensus parameters of the fence, the same for every class (ADR-0135 SA-5). A class
//! heavier than `escrow / rate` is paid its escrow whole and reads as cap-saturated; the registry
//! refuses to step such a class to `ACTIVE` (ADR-0133 Fence 3: "a class above 80 % cap utilization
//! is not activatable"), which is the rule the CLI used to print as a warning.
//!
//! The liveness floor (the base class) is not a model and stays on the schedule: no row is written
//! for it and its `Final` is folded exactly as before the fence.

use borsh::{BorshDeserialize, BorshSerialize};

use crate::palw_economic_compute_v1::{
    PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1, palw_attempted_compute_q32_per_claim_v1, palw_network_expected_attempts_q32_v1,
};
use crate::palw_economics_ledger_v1::{PALW_LEDGER_RATE_SCALE_V1, palw_rate_priced_reward_v1};
use crate::palw_model_registry_v1::PalwModelWorkV1;

/// The layout version of [`PalwClaimEconomicsV1`] (a state object: bump it with the fields).
pub const PALW_ECONOMIC_PAYOUT_VERSION_V1: u16 = 1;

/// **The devnet's numbers** — what `--palw-economic-payout-devnet` and the fork-id arm-by-name
/// schedule: 9 MSK per 10⁹ MAC-eq (testnet-11's rate — see the field), `α = 1`, the panel share between a tenth and three tenths,
/// the cap ceiling at 80 %. The activation is the caller's. A testnet or mainnet card states its
/// own calibrated rate (ADR-0132 §4.3: so the heaviest live class sits under the ceiling).
pub const PALW_ECONOMIC_PAYOUT_DEVNET_V1: crate::config::params::PalwEconomicPayoutV1 = crate::config::params::PalwEconomicPayoutV1 {
    activation: crate::config::params::ForkActivation::never(),
    // testnet-11's rate (9 MSK a G MAC-eq), not a devnet-sized one: ADR-0137's work floor is
    // `W₀ = escrow × 10⁹ / rate`, and a devnet block's escrow is a testnet block's (~3,200 MSK of a
    // 4,445.62 MSK subsidy), so at 0.01 MSK a G the floor was 320 T MAC-eq and the dense A16 class
    // (84 G a draw) drew at p ≈ 2.6 × 10⁻⁴ — one block a day — which the 2026-09-18 drill found at
    // its second phase. At 9 MSK a G the floor is ~356 G and the class draws at p ≈ 0.24, as it
    // will on the testnet.
    rate_sompi_per_giga: 900_000_000,
    panel_share_alpha_permille: 1_000,
    panel_share_min_permille: 100,
    panel_share_max_permille: 300,
    cap_utilization_max_permille: 800,
};

/// **The fence's numbers, as the fold reads them at one block** — resolved by the processor from
/// `Params::palw_economic_payout` at the block's DAA, plus the block's own compact `bits` (the network
/// lottery this block's own attempt faced; a merged attempt carries its own block's).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwEconomicPayoutFoldV1 {
    /// Sompi paid per 10⁹ MAC-equivalents of attempted compute.
    pub rate_sompi_per_giga: u64,
    /// `α` in permille: how a unit of verification compute is valued against a unit of the
    /// producer's (1,000 = at par).
    pub panel_share_alpha_permille: u32,
    /// `S_min` and `S_max` of the panel share, in permille of the priced reward.
    pub panel_share_min_permille: u16,
    pub panel_share_max_permille: u16,
    /// A class whose uncapped economic reward exceeds this fraction of the escrow at a span boundary
    /// is not activatable (ADR-0133 Fence 3's 80 %).
    pub cap_utilization_max_permille: u16,
    /// The compact `bits` of the block being folded; `0` where no header was at hand, which prices
    /// the network draw at exactly one (never a refusal: the snapshot is priced, not gated).
    pub block_bits: u32,
}

/// **What a claim snapshots when it is accepted** (proposal B): the facts its `Final` is priced
/// on, fixed at the accepting block so nothing that happens later — a retarget, a registration, a
/// changed rate — moves what a claim in flight is owed. Dropped with the claim's terminal write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct PalwClaimEconomicsV1 {
    /// The economic compute of one draw of the class (its prefill draw job), from the registry's
    /// work for the class.
    pub draw_ccu: u128,
    /// The economic compute one seat spends replaying the claim's job to verify it.
    pub verification_ccu: u128,
    /// The seats a panel of the class has (the registry's `seat_count`): `C_V = seats × verification`.
    pub seat_count: u16,
    /// The class draws a claim costs in expectation at the class target when accepted, Q32.
    pub expected_attempts_q32: u128,
    /// The network draws a class win costs at the carrying block's `bits`, Q32.
    pub network_expected_attempts_q32: u128,
    /// The rate in force when accepted.
    pub rate_sompi_per_giga: u64,
    /// The panel's share of the priced reward, derived once here, in permille.
    pub panel_share_permille: u16,
}

impl kaspa_utils::mem_size::MemSizeEstimator for PalwClaimEconomicsV1 {}

impl PalwClaimEconomicsV1 {
    /// `C_P`: the compute the producer ran for this claim in expectation — class draws × network
    /// draws × one draw (ADR-0131's attempted basis with ADR-0132's network factor).
    pub fn attempted_ccu(&self) -> u128 {
        palw_attempted_ccu_v1(self.expected_attempts_q32, self.network_expected_attempts_q32, self.draw_ccu)
    }

    /// `C_V`: the compute the whole panel spends verifying it.
    pub fn verification_total_ccu(&self) -> u128 {
        self.verification_ccu.saturating_mul(self.seat_count as u128)
    }

    /// The claim's priced reward out of `escrow`: `min(escrow, C_P × rate)`.
    pub fn priced_reward(&self, escrow_sompi: u64) -> u64 {
        palw_rate_priced_reward_v1(escrow_sompi, self.attempted_ccu(), self.rate_sompi_per_giga as u128)
    }
}

/// `class_q32 × network_q32 × draw`, each Q32 factor never below one draw; saturating.
pub fn palw_attempted_ccu_v1(expected_attempts_q32: u128, network_expected_attempts_q32: u128, draw_ccu: u128) -> u128 {
    palw_attempted_compute_q32_per_claim_v1(
        network_expected_attempts_q32,
        palw_attempted_compute_q32_per_claim_v1(expected_attempts_q32, draw_ccu),
    )
}

/// The network draws a win costs at `bits`, Q32 — exactly one where no `bits` were at hand (`0`).
pub fn palw_network_draws_q32_from_bits_v1(bits: u32) -> u128 {
    if bits == 0 { PALW_EXPECTED_ATTEMPTS_Q32_ONE_V1 } else { palw_network_expected_attempts_q32_v1(bits) }
}

/// **The panel's share of a priced reward**, in permille:
/// `clamp(α·C_V / (C_P + α·C_V), S_min, S_max)` with `α` in permille. A zero denominator (no
/// compute on either side) is the floor; the bounds are applied as given, `min` capped by `max`.
pub fn palw_panel_share_permille_v1(c_p: u128, c_v: u128, alpha_permille: u32, min_permille: u16, max_permille: u16) -> u16 {
    let max = max_permille.min(1_000);
    let min = min_permille.min(max);
    // α·C_V in thousandths against C_P in thousandths, so the ratio needs no fraction.
    let weighted = c_v.saturating_mul(alpha_permille as u128);
    let denominator = c_p.saturating_mul(1_000).saturating_add(weighted);
    if denominator == 0 {
        return min;
    }
    let share = weighted.saturating_mul(1_000) / denominator;
    (share.min(1_000) as u16).clamp(min, max)
}

/// **Cap utilization** in permille: the uncapped economic reward (`attempted × rate`) over the
/// escrow a claim of the class holds. Above 1,000 the price is capped at the escrow and the class
/// reads as saturated; `u32::MAX` where there is no escrow to hold it (nothing to price against).
pub fn palw_cap_utilization_permille_v1(attempted_ccu: u128, rate_sompi_per_giga: u64, escrow_sompi: u64) -> u32 {
    if escrow_sompi == 0 {
        return u32::MAX;
    }
    let uncapped = attempted_ccu.saturating_mul(rate_sompi_per_giga as u128) / PALW_LEDGER_RATE_SCALE_V1;
    (uncapped.saturating_mul(1_000) / escrow_sompi as u128).min(u32::MAX as u128) as u32
}

/// **The snapshot a class's claim takes at acceptance**, from the fence, the registry's work for
/// the class, the panel's seat count, the class target and the carrying block's `bits`.
pub fn palw_claim_economics_snapshot_v1(
    fold: &PalwEconomicPayoutFoldV1,
    work: &PalwModelWorkV1,
    seat_count: u16,
    class_target: u128,
    carrying_bits: u32,
) -> PalwClaimEconomicsV1 {
    let expected_attempts_q32 = crate::palw_economic_compute_v1::palw_expected_attempts_q32_v1(class_target);
    let network_expected_attempts_q32 = palw_network_draws_q32_from_bits_v1(carrying_bits);
    let c_p = palw_attempted_ccu_v1(expected_attempts_q32, network_expected_attempts_q32, work.economic_ccu_per_claim);
    let c_v = work.verification_ccu.saturating_mul(seat_count as u128);
    PalwClaimEconomicsV1 {
        draw_ccu: work.economic_ccu_per_claim,
        verification_ccu: work.verification_ccu,
        seat_count,
        expected_attempts_q32,
        network_expected_attempts_q32,
        rate_sompi_per_giga: fold.rate_sompi_per_giga,
        panel_share_permille: palw_panel_share_permille_v1(
            c_p,
            c_v,
            fold.panel_share_alpha_permille,
            fold.panel_share_min_permille,
            fold.panel_share_max_permille,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold() -> PalwEconomicPayoutFoldV1 {
        PalwEconomicPayoutFoldV1 {
            rate_sompi_per_giga: 1_000,
            panel_share_alpha_permille: 1_000,
            panel_share_min_permille: 100,
            panel_share_max_permille: 300,
            cap_utilization_max_permille: 800,
            block_bits: 0,
        }
    }

    #[test]
    fn adr0132_the_price_is_attempted_compute_at_the_rate_capped_by_the_escrow() {
        // 1 draw × 1 network draw × 5·10⁹ MAC-eq at 1,000 sompi per 10⁹ = 5,000 sompi.
        let work = PalwModelWorkV1 { verification_ccu: 5_000_000_000, economic_ccu_per_claim: 5_000_000_000, ..Default::default() };
        let snap = palw_claim_economics_snapshot_v1(&fold(), &work, 5, u128::MAX, 0);
        assert_eq!(snap.attempted_ccu(), 5_000_000_000);
        assert_eq!(snap.priced_reward(1_000_000), 5_000, "under the cap the price is the compute at the rate");
        assert_eq!(snap.priced_reward(4_000), 4_000, "a heavier claim than the escrow is paid the escrow whole");
        assert_eq!(snap.priced_reward(0), 0);
    }

    #[test]
    fn adr0132_the_network_draw_is_credited_and_the_floor_bits_double_the_attempted_compute() {
        let work = PalwModelWorkV1 { verification_ccu: 1, economic_ccu_per_claim: 1_000, ..Default::default() };
        let one = palw_claim_economics_snapshot_v1(&fold(), &work, 5, u128::MAX, 0);
        let floor = palw_claim_economics_snapshot_v1(&fold(), &work, 5, u128::MAX, 0x207f_ffff);
        assert_eq!(one.attempted_ccu(), 1_000, "no bits at hand: one network draw");
        assert_eq!(floor.attempted_ccu(), 2_000, "the difficulty floor is a coin flip: two network draws a win");
        // A tighter class target draws more, and the snapshot keeps it.
        let half = palw_claim_economics_snapshot_v1(&fold(), &work, 5, u128::MAX / 2, 0);
        assert_eq!(half.attempted_ccu(), 2_000, "a class at half of MAX draws two forwards a claim");
    }

    #[test]
    fn adr0132_the_panel_share_follows_verification_against_producer_compute_inside_its_bounds() {
        // Verification as heavy as production at α = 1: a half, clamped to S_max.
        assert_eq!(palw_panel_share_permille_v1(1_000, 1_000, 1_000, 100, 300), 300);
        // A tenth of the producer's compute: 1/11 ≈ 91 ‰, lifted to S_min.
        assert_eq!(palw_panel_share_permille_v1(10_000, 1_000, 1_000, 100, 300), 100);
        // A fifth: 1/6 ≈ 166 ‰, inside the bounds.
        assert_eq!(palw_panel_share_permille_v1(5_000, 1_000, 1_000, 100, 300), 166);
        // α halves the weight of verification: 1/11 → the floor again.
        assert_eq!(palw_panel_share_permille_v1(5_000, 1_000, 500, 100, 300), 100);
        // Nothing on either side is the floor; a min above max is capped by max.
        assert_eq!(palw_panel_share_permille_v1(0, 0, 1_000, 100, 300), 100);
        assert_eq!(palw_panel_share_permille_v1(0, 1, 1_000, 900, 300), 300);
        // The seven-seat full replay of a class: C_V = 7 × job, C_P = 1.5 × 2 × job → 7/10 → S_max.
        let work = PalwModelWorkV1 { verification_ccu: 1_000, economic_ccu_per_claim: 1_000, ..Default::default() };
        let snap = palw_claim_economics_snapshot_v1(&fold(), &work, 7, u128::MAX, 0x207f_ffff);
        assert_eq!(snap.verification_total_ccu(), 7_000);
        assert_eq!(snap.panel_share_permille, 300);
    }

    #[test]
    fn adr0132_cap_utilization_reads_the_uncapped_reward_against_the_escrow() {
        assert_eq!(palw_cap_utilization_permille_v1(5_000_000_000, 1_000, 10_000), 500);
        assert_eq!(palw_cap_utilization_permille_v1(5_000_000_000, 1_000, 5_000), 1_000);
        assert_eq!(palw_cap_utilization_permille_v1(50_000_000_000, 1_000, 5_000), 10_000, "ten times over: saturated");
        assert_eq!(palw_cap_utilization_permille_v1(1, 1_000, 0), u32::MAX, "no escrow: nothing to price against");
        assert_eq!(palw_cap_utilization_permille_v1(u128::MAX, u64::MAX, 1), u32::MAX, "saturates, never wraps");
    }

    #[test]
    fn adr0132_the_snapshot_is_a_borsh_object_with_a_fixed_layout() {
        let snap = PalwClaimEconomicsV1 {
            draw_ccu: 1,
            verification_ccu: 2,
            seat_count: 3,
            expected_attempts_q32: 4,
            network_expected_attempts_q32: 5,
            rate_sompi_per_giga: 6,
            panel_share_permille: 7,
        };
        let bytes = borsh::to_vec(&snap).unwrap();
        assert_eq!(bytes.len(), 16 + 16 + 2 + 16 + 16 + 8 + 2, "u128, u128, u16, u128, u128, u64, u16");
        assert_eq!(bytes[0], 1);
        assert_eq!(bytes[16], 2);
        assert_eq!(bytes[32], 3);
        assert_eq!(PalwClaimEconomicsV1::try_from_slice(&bytes).unwrap(), snap);
    }
}
