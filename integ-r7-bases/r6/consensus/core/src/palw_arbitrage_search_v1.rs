//! **ADR-0146 — the bound is a gate, and the program that measures it is this module.**
//!
//! A coefficient table is legitimate only when a committed, re-runnable search reports the worst
//! arbitrage it permits, and that number sits below the efficiency spread the protocol wants to
//! reward (Rules R2, R3, R7). This file is that search. It invents no coefficient: the arithmetic
//! dimensions stay MAC-equivalents, the traffic dimensions stay bytes, and
//! [`PalwCanonicalWorkVectorV1::provisional_scalar_v1`] still sums arithmetic only.
//!
//! **What it found (and what ADR-0146 §9 already recorded).** Pricing on the derivation — not on
//! declared leaves — collapses fork-choice weight per executed MAC-equivalent to **1.000000×**
//! across the four shipped classes and across the admissible declaration space of one class. Pay
//! per giga-MAC-equivalent is one protocol constant. There is therefore **no table to write and
//! no scalar to arm.** The residual ADR-0146 §6 names (resident vs paged) is a cost asymmetry
//! between classes, not a lever a registrant can pull in price.
//!
//! No fence. No `Params` field. `cargo test -p kaspa-consensus-core palw_arbitrage_search` is
//! the command Rule R7 asked for.

use crate::palw_canonical_work_v1::{
    PalwCanonicalClassDescriptorV1, PalwCanonicalExecutionFactsV1, palw_canonical_work_v1, palw_real_weight_block_v1,
};
use crate::palw_economic_compute_v1::{PALW_ECONOMIC_COST_TABLE_V1, palw_attempt_economic_compute_v1};
use crate::palw_step::PalwShapeProfileV3;
use crate::palw_v2::PalwJobContextV2;

/// ADR-0132's one rate, sompi per giga-MAC-equivalent. A second number here would be a per-model
/// multiplier, which the operator's standing rule forbids.
pub const PALW_ARBITRAGE_RATE_SOMPI_PER_GIGA_V1: u64 = 900_000_000;

/// The 72 % worker carve of one block's escrow. Pay is `min(rate × compute, this)`.
pub const PALW_ARBITRAGE_ESCROW_SOMPI_V1: u64 = 320_084_640_000;

/// Past `palw_prefill_draw` an attempt executes one decode token whatever the class declared.
const PREFILL_DRAW: bool = true;

/// Scale for integer ratios. `SCALE / SCALE` is 1.000000×.
pub const PALW_ARBITRAGE_RATIO_SCALE_V1: u128 = 1_000_000;

/// **The bound the search reports for the derivation.** One, because weight IS the arithmetic.
pub const PALW_ARBITRAGE_BOUND_ARITHMETIC_V1: u128 = PALW_ARBITRAGE_RATIO_SCALE_V1;

/// One configuration the search measured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArbitrageRowV1 {
    pub name: &'static str,
    pub executed_mac_eq: u128,
    pub derived_weight: u128,
    pub pay_sompi: u64,
    pub weight_traffic_bytes: u128,
}

/// The search's report: the worst ratio of reward-per-executed-MAC between any two rows, and
/// whether the collapse into a scalar included traffic (it must not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwArbitrageBoundV1 {
    pub rows: Vec<PalwArbitrageRowV1>,
    /// `max(weight/MAC) / min(weight/MAC)`, scaled by [`PALW_ARBITRAGE_RATIO_SCALE_V1`].
    pub weight_per_mac_spread: u128,
    /// `max(pay/MAC) / min(pay/MAC)`, same scale. Identical pay-per-MAC is 1.000000×.
    pub pay_per_mac_spread: u128,
    /// True iff every row's provisional scalar equals its arithmetic and not its traffic.
    pub scalar_is_arithmetic_only: bool,
}

fn job(profile: &PalwShapeProfileV3, prefill: u32, decode: u32) -> PalwJobContextV2 {
    crate::palw_base0_profile::rc_job_context(profile, prefill, decode)
}

fn executed_mac_eq(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> u128 {
    palw_attempt_economic_compute_v1(profile, &job(profile, canonical.0, canonical.1), PREFILL_DRAW, &PALW_ECONOMIC_COST_TABLE_V1)
        .expect("the search's fixture walks")
}

fn derived_weight(profile: &PalwShapeProfileV3, canonical: (u32, u32)) -> (u128, u128) {
    let descriptor = PalwCanonicalClassDescriptorV1::of(profile, crate::Hash64::default()).expect("one weight format");
    let vector = palw_canonical_work_v1(
        &descriptor,
        &PalwCanonicalExecutionFactsV1::of_attempt(&job(profile, canonical.0, canonical.1), PREFILL_DRAW),
    )
    .expect("the search's fixture derives");
    // The lottery factor is not a coefficient: P4 is the derivation of one draw. Claim weight
    // is `expected_attempts × this`; the attempts cancel against the claim's executed MAC-eq
    // (`the_better_model_is_paid_less_is_neutralised`).
    (vector.provisional_scalar_v1(), vector.weight_traffic_bytes)
}

fn pay_sompi(executed: u128) -> u64 {
    ((executed * PALW_ARBITRAGE_RATE_SOMPI_PER_GIGA_V1 as u128) / 1_000_000_000u128).min(PALW_ARBITRAGE_ESCROW_SOMPI_V1 as u128) as u64
}

fn spread_of(values: &[u128]) -> u128 {
    let min = *values.iter().min().expect("the search measured at least one row");
    let max = *values.iter().max().expect("the search measured at least one row");
    if min == 0 {
        return u128::MAX;
    }
    max.saturating_mul(PALW_ARBITRAGE_RATIO_SCALE_V1) / min
}

/// **The committed search** (ADR-0146 R2/R7): every shipped class, one executed draw of its
/// canonical job, priced on the derivation. Adding a class to this list is how a future table —
/// if one is ever proposed — would have to re-measure.
pub fn palw_arbitrage_search_shipped_classes_v1() -> PalwArbitrageBoundV1 {
    let rows_in: [(&'static str, PalwShapeProfileV3, (u32, u32)); 4] = [
        (
            "BASE-0 floor",
            crate::palw_base0_profile::base0_profile_v1(crate::palw_base0_profile::PALW_RC_BASE0_GEOMETRY).expect("floor"),
            crate::palw_base0_profile::PALW_RC_BASE0_CANONICAL,
        ),
        (
            "Qwen3.6-35B-A3B",
            crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                crate::palw_qwen36_profile::QWEN36_35B_A3B,
            ))
            .expect("hybrid"),
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        ),
        (
            "Qwen2.5-A16 @512",
            crate::palw_context_ladder::palw_a16_context_row_profile_v5(crate::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX)
                .expect("dense"),
            crate::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1(),
        ),
        (
            "Qwen3.8-27B",
            crate::palw_qwen36_profile::qwen36_profile_v2(crate::palw_qwen36_profile::qwen36_geometry_artifact_eps(
                crate::palw_qwen36_profile::QWEN38_27B,
            ))
            .expect("27B"),
            crate::palw_qwen36_profile::QWEN36_RC_CANONICAL,
        ),
    ];
    let mut rows = Vec::new();
    let mut scalar_is_arithmetic_only = true;
    for (name, profile, canonical) in rows_in {
        let executed = executed_mac_eq(&profile, canonical);
        let (weight, traffic) = derived_weight(&profile, canonical);
        let descriptor = PalwCanonicalClassDescriptorV1::of(&profile, crate::Hash64::default()).expect("one weight format");
        let vector = palw_canonical_work_v1(
            &descriptor,
            &PalwCanonicalExecutionFactsV1::of_attempt(&job(&profile, canonical.0, canonical.1), PREFILL_DRAW),
        )
        .expect("derived");
        // Traffic leaking into the scalar is a coefficient of 1 byte = 1 MAC, which R4 forbids.
        if vector.provisional_scalar_v1() != vector.arithmetic_mac_eq() {
            scalar_is_arithmetic_only = false;
        }
        rows.push(PalwArbitrageRowV1 {
            name,
            executed_mac_eq: executed,
            derived_weight: weight,
            pay_sompi: pay_sompi(executed),
            weight_traffic_bytes: traffic,
        });
    }
    let weight_ratios: Vec<u128> =
        rows.iter().map(|r| r.derived_weight.saturating_mul(PALW_ARBITRAGE_RATIO_SCALE_V1) / r.executed_mac_eq.max(1)).collect();
    let pay_ratios: Vec<u128> =
        rows.iter().map(|r| (r.pay_sompi as u128).saturating_mul(1_000_000_000u128) / r.executed_mac_eq.max(1)).collect();
    PalwArbitrageBoundV1 {
        rows,
        weight_per_mac_spread: spread_of(&weight_ratios),
        pay_per_mac_spread: spread_of(&pay_ratios),
        scalar_is_arithmetic_only,
    }
}

/// **The traffic term is derived from the format's block layout** (ADR-0146 §2 / Rule R1).
/// `Q4_K` is `144/256` of an 8-bit stream, `I8` is 1. No coefficient.
pub fn palw_arbitrage_q4k_traffic_ratio_v1() -> (u64, u64) {
    palw_real_weight_block_v1(12)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command ADR-0146 Rule R7 asked for. One assertion: the derivation's worst
    /// reward-per-MAC across the four shipped classes is 1.000000×, the scalar did not swallow
    /// traffic, and there is therefore no coefficient table to arm.
    #[test]
    fn the_search_finds_no_coefficient_table_to_write() {
        let bound = palw_arbitrage_search_shipped_classes_v1();
        assert!(bound.scalar_is_arithmetic_only, "provisional_scalar_v1 invented a coefficient");
        assert_eq!(
            bound.weight_per_mac_spread,
            PALW_ARBITRAGE_BOUND_ARITHMETIC_V1,
            "weight per executed MAC-equivalent spread across shipped classes: {:?}",
            bound.rows.iter().map(|r| (r.name, r.derived_weight, r.executed_mac_eq)).collect::<Vec<_>>()
        );
        assert_eq!(
            bound.pay_per_mac_spread, PALW_ARBITRAGE_BOUND_ARITHMETIC_V1,
            "pay per executed MAC-equivalent spread across shipped classes"
        );
        for row in &bound.rows {
            assert!(row.executed_mac_eq > 0, "{} executes nothing", row.name);
            assert_eq!(row.derived_weight, row.executed_mac_eq, "{}: weight is the arithmetic, not a guess", row.name);
        }
        let (q4_bytes, q4_weights) = palw_arbitrage_q4k_traffic_ratio_v1();
        assert_eq!((q4_bytes, q4_weights), (144, 256), "Q4_K layout is a derivation, not a preference");
        assert_eq!(palw_real_weight_block_v1(24), (1, 1), "I8 is one byte per weight");
    }

    /// Representation search on one class: every admissible declaration of the dense row, one
    /// executed job `(63, 1)`. Weight-per-MAC must not move. This is R2 over the declaration
    /// space ADR-0146 §3 tabled at 427× on the leaf basis.
    #[test]
    fn no_admissible_declaration_of_one_class_beats_another_on_the_derivation() {
        let mut profile =
            crate::palw_context_ladder::palw_a16_context_row_profile_v5(crate::palw_qwen25_profile::QWEN25_A16_GRAPH_V5_N_CTX)
                .expect("dense");
        let canonical = crate::palw_qwen25_profile::qwen25_a16_graph_v5_canonical_v1();
        let baseline = derived_weight(&profile, canonical).0;
        let executed = executed_mac_eq(&profile, canonical);
        assert!(executed > 0);
        let mut ratios = Vec::new();
        for decode in [1u32, 2, 8, 64, 256, 370] {
            let (weight, _) = derived_weight(&profile, (canonical.0, decode));
            ratios
                .push(weight.saturating_mul(PALW_ARBITRAGE_RATIO_SCALE_V1) / executed_mac_eq(&profile, (canonical.0, decode)).max(1));
        }
        for tile in [24u32, 32, 64, 256, 4096] {
            for table in [&mut profile.pre_nodes, &mut profile.gdn_nodes, &mut profile.attn_nodes, &mut profile.post_nodes] {
                for node in table.iter_mut() {
                    node.tile_len = tile;
                }
            }
            let (weight, _) = derived_weight(&profile, canonical);
            ratios.push(weight.saturating_mul(PALW_ARBITRAGE_RATIO_SCALE_V1) / executed_mac_eq(&profile, canonical).max(1));
            assert_eq!(weight, baseline, "tile_len {tile} moved derived weight of the same execution");
        }
        assert_eq!(spread_of(&ratios), PALW_ARBITRAGE_BOUND_ARITHMETIC_V1, "a declaration beat another on the derivation");
    }

    /// **ADR-0144 P4 / the 0147–0149 arming gate.** Representation-neutral accounting is the
    /// derivation at 1.000000×. The economic bundle may arm without contradicting P4; it stays
    /// `None` on every shipped preset until an operator chooses a height (0144 item 0).
    #[test]
    fn p4_arithmetic_is_one_so_the_bundle_may_arm() {
        let bound = palw_arbitrage_search_shipped_classes_v1();
        assert!(bound.scalar_is_arithmetic_only);
        assert_eq!(bound.weight_per_mac_spread, PALW_ARBITRAGE_BOUND_ARITHMETIC_V1);
        assert_eq!(bound.pay_per_mac_spread, PALW_ARBITRAGE_BOUND_ARITHMETIC_V1);
        assert_eq!(
            PALW_ARBITRAGE_BOUND_ARITHMETIC_V1, PALW_ARBITRAGE_RATIO_SCALE_V1,
            "the bound that would justify a coefficient table is 1.000000× — there is no table to write"
        );
    }
}
