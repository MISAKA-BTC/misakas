//! ADR-0038 / external audit P0-10: **a bond may not back more immature PALW work than it can pay
//! for.**
//!
//! Admission cannot tell an honest trace root from a fabricated one — the design says so plainly,
//! and routes the answer through sampling, the court and bond economics instead. That leaves one
//! obligation this module carries: while a block is `Provisional` or `ReceiptLicensed` it already
//! contributes live weight, and nothing yet has proven its root. If a single bond can stand behind
//! an unbounded number of such blocks, an attacker grinds random roots and accumulates many times
//! its collateral in live weight before the first slash lands.
//!
//! Today that attack is expensive for an unrelated reason: every full node re-runs the LLM, which
//! is itself a W1 violation. **Removing the re-run without this cap is what turns the attack
//! cheap** — the two must not be separated, and this module exists so that they cannot be.
//!
//! ## The rule
//!
//! ```text
//! Σ immature_pwu backed by a bond  ≤  slashable_collateral / penalty_per_pwu
//! ```
//!
//! `Final` and `Voided` work is not immature and does not count: the first has matured under the
//! challenge window and the second has already been punished. Exposure is therefore released by
//! the same events that end a block's provisional life, with no separate bookkeeping to drift.

use crate::tx::TransactionOutpoint;
use std::collections::BTreeMap;

/// How much collateral one pwu of unproven work must have behind it, in sompi.
///
/// A parameter rather than a constant because it prices a slash against a unit of work, and both
/// sides are network facts. `0` is refused rather than treated as "no limit": a caller that means
/// "unbounded" has to say so by not calling this at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwExposureParamsV1 {
    pub penalty_sompi_per_pwu: u64,
}

#[derive(thiserror::Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwExposureError {
    #[error("penalty_sompi_per_pwu is zero — an exposure cap that divides by zero is not a cap")]
    ZeroPenalty,
}

impl PalwExposureParamsV1 {
    pub fn validate(&self) -> Result<(), PalwExposureError> {
        if self.penalty_sompi_per_pwu == 0 {
            return Err(PalwExposureError::ZeroPenalty);
        }
        Ok(())
    }

    /// The most immature pwu `collateral_sompi` may stand behind.
    pub fn cap_pwu(&self, collateral_sompi: u64) -> Result<u64, PalwExposureError> {
        self.validate()?;
        Ok(collateral_sompi / self.penalty_sompi_per_pwu)
    }
}

/// One immature block, as the exposure walk sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwImmatureWorkV1 {
    /// The bond that would be slashed if this block's root proves fabricated.
    pub bond_outpoint: TransactionOutpoint,
    /// That bond's slashable collateral at this chain point.
    pub collateral_sompi: u64,
    /// The work this block claims.
    pub pwu: u64,
}

/// Which immature blocks a bond can actually stand behind, **in the order given**.
///
/// Returns one verdict per input, `true` where the block is inside its bond's cap.
///
/// **Prefix-mandatory, like the credit budget it sits beside.** The caller supplies blocks in
/// canonical chain order and the walk admits them until the bond's cap is reached, then refuses the
/// rest — it never skips an over-large block to fit a later smaller one. Skipping would make the
/// admitted set depend on the sizes present, so two nodes replaying the same chain in the same
/// order could still disagree if one of them had a block the other lacked.
///
/// **A refusal is "carries no live weight", never "invalid block".** The block stays in the DAG and
/// keeps its spam-hash backbone; what it loses is the ramped pwu, which is the thing the bond was
/// supposed to be standing behind. Rejecting the block instead would let anyone grief a producer by
/// racing cheap commitments onto its bond.
pub fn admit_within_exposure_v1(
    blocks: &[PalwImmatureWorkV1],
    params: &PalwExposureParamsV1,
) -> Result<Vec<bool>, PalwExposureError> {
    params.validate()?;
    // Per bond: how much it has committed, and whether its prefix has already ended. The second is
    // what makes this prefix-mandatory rather than a knapsack — without it the walk would skip an
    // over-large block and admit a later smaller one, and the admitted set would depend on which
    // sizes happened to be present.
    let mut spent: BTreeMap<(crate::Hash64, u32), (u64, bool)> = BTreeMap::new();
    let mut out = Vec::with_capacity(blocks.len());
    for block in blocks {
        let key = (block.bond_outpoint.transaction_id, block.bond_outpoint.index);
        let cap = params.cap_pwu(block.collateral_sompi)?;
        let entry = spent.entry(key).or_insert((0, false));
        if entry.1 {
            out.push(false); // this bond's prefix ended earlier; nothing after it counts
            continue;
        }
        // `checked_add`, not saturating: a total that overflows is not a large total to compare, it
        // is a total this arithmetic cannot represent, and saturating it to `u64::MAX` would then
        // COMPARE as under a `u64::MAX` cap. Refuse instead.
        match entry.0.checked_add(block.pwu).filter(|total| *total <= cap) {
            Some(total) => {
                entry.0 = total;
                out.push(true);
            }
            None => {
                entry.1 = true;
                out.push(false);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hash64;

    const P: PalwExposureParamsV1 = PalwExposureParamsV1 { penalty_sompi_per_pwu: 10 };

    fn op(seed: u8) -> TransactionOutpoint {
        TransactionOutpoint::new(Hash64::from_bytes([seed; 64]), 0)
    }
    fn work(bond: u8, collateral: u64, pwu: u64) -> PalwImmatureWorkV1 {
        PalwImmatureWorkV1 { bond_outpoint: op(bond), collateral_sompi: collateral, pwu }
    }

    /// The attack the cap exists for: one bond, many unproven blocks.
    ///
    /// 1 000 sompi at 10 sompi/pwu backs 100 pwu. Four 30-pwu blocks are 120, so the fourth is
    /// outside — and the fifth stays outside no matter how small, because the cap is on the SUM.
    #[test]
    fn one_bond_cannot_back_more_immature_work_than_it_can_pay_for() {
        let blocks = [work(1, 1_000, 30), work(1, 1_000, 30), work(1, 1_000, 30), work(1, 1_000, 30), work(1, 1_000, 1)];
        assert_eq!(admit_within_exposure_v1(&blocks, &P).unwrap(), vec![true, true, true, false, false]);
    }

    /// Exposure is per bond: a second bond brings its own collateral, and neither borrows the
    /// other's headroom.
    #[test]
    fn exposure_is_per_bond_and_never_pooled() {
        let blocks = [work(1, 100, 10), work(2, 100, 10), work(1, 100, 1), work(2, 100, 1)];
        assert_eq!(admit_within_exposure_v1(&blocks, &P).unwrap(), vec![true, true, false, false]);
    }

    /// Prefix-mandatory: the walk stops, it does not shop for a block that fits.
    ///
    /// Without this the admitted set depends on which sizes happen to be present, so two nodes
    /// replaying the same chain could differ whenever one held a block the other lacked.
    #[test]
    fn the_walk_stops_rather_than_skipping_to_a_smaller_block() {
        // Cap is 10 pwu. The first block wants 11 — over on its own — and the second wants 1,
        // which WOULD fit. It is refused anyway: the prefix ended at the first.
        let blocks = [work(1, 100, 11), work(1, 100, 1)];
        assert_eq!(
            admit_within_exposure_v1(&blocks, &P).unwrap(),
            vec![false, false],
            "once a bond's prefix ends nothing after it counts, however small"
        );

        // Reordered, the same two blocks give a different admitted set — order decides, which is
        // exactly why the caller owes canonical chain order and why this is not a knapsack.
        let reordered = [work(1, 100, 1), work(1, 100, 11)];
        assert_eq!(admit_within_exposure_v1(&reordered, &P).unwrap(), vec![true, false]);
    }

    /// A zero penalty is refused rather than read as "no limit".
    #[test]
    fn a_cap_that_divides_by_zero_is_not_a_cap() {
        let zero = PalwExposureParamsV1 { penalty_sompi_per_pwu: 0 };
        assert_eq!(zero.validate(), Err(PalwExposureError::ZeroPenalty));
        assert_eq!(admit_within_exposure_v1(&[work(1, 100, 1)], &zero), Err(PalwExposureError::ZeroPenalty));
    }

    /// A total that cannot be represented is refused, not saturated.
    ///
    /// At one sompi per pwu the cap IS the collateral, so `u64::MAX` collateral admits a
    /// `u64::MAX/2 + 1` block twice over — and the second sum overflows. Saturating it to
    /// `u64::MAX` would then compare as *under* a `u64::MAX` cap and admit unbounded exposure as
    /// though it were exactly at the limit.
    #[test]
    fn a_total_that_cannot_be_represented_is_refused_not_saturated() {
        let one = PalwExposureParamsV1 { penalty_sompi_per_pwu: 1 };
        let half = u64::MAX / 2 + 1;
        let blocks = [work(1, u64::MAX, half), work(1, u64::MAX, half), work(1, u64::MAX, 1)];
        assert_eq!(admit_within_exposure_v1(&blocks, &one).unwrap(), vec![true, false, false]);
    }
}
