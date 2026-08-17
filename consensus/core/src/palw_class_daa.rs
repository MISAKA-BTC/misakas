//! ADR-0038 Decision D: per-class difficulty domains — prices are discovered, never
//! maintained.
//!
//! Every hand-set cross-class PWU coefficient is a standing arbitrage: the mispriced-cheap
//! class absorbs every miner and the multi-class resilience ADR-0038 buys dies as a
//! monoculture. So the static, canonical PWU score prices work *inside* a class only, and
//! *between* classes each Active class runs its own retarget against its own share of block
//! cadence — the multi-algo-chain construction. This module is that arithmetic, pure and
//! consensus-inert:
//!
//! * [`PalwDifficultyDomainSetV1`] — the Active classes and their cadence shares, plus the
//!   anti-stall floor's share. The invariant `Σ class shares + floor = 1000‰` holds at every
//!   mutation, and freezing a class redistributes its share over the survivors
//!   proportionally, deterministically (largest-remainder, class-id order).
//!   With zero survivors the floor takes everything: the chain limps on spam-hash blocks,
//!   visibly, and never halts (W6).
//! * [`adjust_class_target_v1`] — one class's clamped integer retarget: observed vs expected
//!   blocks over a window, easier when starving, harder when flooding, never by more than
//!   `max_factor` per adjustment. GPU wall-clock never enters — an RTX 5090 second and a
//!   2019-GPU second are different amounts of nothing.
//!
//! The caller (Stage-1 wiring) owns windows, per-class DAA state storage and how targets
//! feed the lottery; this module owns the arithmetic being deterministic, clamped and
//! share-conserving on every node.

use kaspa_hashes::Hash64;
use std::collections::BTreeMap;
use thiserror::Error;

/// Shares are permille; the set's conservation invariant is against this denominator.
pub const PALW_CLASS_SHARE_DENOMINATOR: u16 = 1000;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum PalwClassDaaError {
    #[error("shares sum to {got}‰ with floor {floor}‰ — must sum to exactly {denominator}‰")]
    SharesDoNotConserve { got: u32, floor: u16, denominator: u16 },
    #[error("anti-stall floor is {got}‰ — must be in 1..={max}‰ (a zero floor halts with the last class)")]
    FloorOutOfRange { got: u16, max: u16 },
    #[error("class {class_id} is not in the domain set")]
    UnknownClass { class_id: Hash64 },
    #[error("class {class_id} carries a zero share — a zero-share Active class is a frozen class wearing the wrong status")]
    ZeroShare { class_id: Hash64 },
    #[error("max_factor must be ≥ 2 (1 would freeze the retarget)")]
    MaxFactorTooSmall,
    #[error("expected_blocks must be nonzero")]
    ZeroExpectedBlocks,
}

/// The Active difficulty domains: class → cadence share (permille), plus the anti-stall
/// floor's share. `Σ shares + floor = 1000` always ([`Self::validate`] is checked on every
/// constructor and mutation, so an inconsistent set is unrepresentable through this API).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct PalwDifficultyDomainSetV1 {
    /// The spam-hash backbone's cadence share — deliberately small and deliberately nonzero
    /// (ADR-0038 Decision E: an incident, not an extinction).
    pub anti_stall_floor_permille: u16,
    /// Active class → share (permille). BTreeMap: every iteration below is id-ordered, so
    /// every node redistributes identically.
    pub class_shares_permille: BTreeMap<Hash64, u16>,
}

impl PalwDifficultyDomainSetV1 {
    /// Build a validated set.
    pub fn new(anti_stall_floor_permille: u16, class_shares_permille: BTreeMap<Hash64, u16>) -> Result<Self, PalwClassDaaError> {
        let set = Self { anti_stall_floor_permille, class_shares_permille };
        set.validate()?;
        Ok(set)
    }

    /// The conservation invariant and the floor/zero-share rules.
    pub fn validate(&self) -> Result<(), PalwClassDaaError> {
        if self.anti_stall_floor_permille == 0 || self.anti_stall_floor_permille > PALW_CLASS_SHARE_DENOMINATOR {
            return Err(PalwClassDaaError::FloorOutOfRange {
                got: self.anti_stall_floor_permille,
                max: PALW_CLASS_SHARE_DENOMINATOR,
            });
        }
        if let Some((class_id, _)) = self.class_shares_permille.iter().find(|(_, share)| **share == 0) {
            return Err(PalwClassDaaError::ZeroShare { class_id: *class_id });
        }
        let classes: u32 = self.class_shares_permille.values().map(|s| *s as u32).sum();
        if classes + self.anti_stall_floor_permille as u32 != PALW_CLASS_SHARE_DENOMINATOR as u32 {
            return Err(PalwClassDaaError::SharesDoNotConserve {
                got: classes,
                floor: self.anti_stall_floor_permille,
                denominator: PALW_CLASS_SHARE_DENOMINATOR,
            });
        }
        Ok(())
    }

    /// Freeze/remove a class: its share redistributes over the survivors proportionally to
    /// their existing shares, deterministically (integer largest-remainder; remainder order
    /// is by descending fractional part then ascending class id, so ties break identically
    /// on every node). With zero survivors the floor absorbs everything — the anti-stall
    /// degradation of ADR-0038 Decision D, W6.
    pub fn remove_class(&mut self, class_id: &Hash64) -> Result<(), PalwClassDaaError> {
        let Some(removed_share) = self.class_shares_permille.remove(class_id) else {
            return Err(PalwClassDaaError::UnknownClass { class_id: *class_id });
        };
        if self.class_shares_permille.is_empty() {
            self.anti_stall_floor_permille += removed_share;
            debug_assert!(self.validate().is_ok());
            return Ok(());
        }
        let survivor_total: u64 = self.class_shares_permille.values().map(|s| *s as u64).sum();
        // Proportional integer split of `removed_share` with largest-remainder assignment.
        let mut assigned_total: u16 = 0;
        let mut remainders: Vec<(u64, Hash64)> = Vec::with_capacity(self.class_shares_permille.len());
        let mut additions: BTreeMap<Hash64, u16> = BTreeMap::new();
        for (id, share) in self.class_shares_permille.iter() {
            let numerator = removed_share as u64 * *share as u64;
            let addition = (numerator / survivor_total) as u16;
            let remainder = numerator % survivor_total;
            assigned_total += addition;
            additions.insert(*id, addition);
            remainders.push((remainder, *id));
        }
        // Distribute the residue: one permille each to the largest remainders, id ascending on ties.
        remainders.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        let mut residue = removed_share - assigned_total;
        for (_, id) in remainders {
            if residue == 0 {
                break;
            }
            *additions.get_mut(&id).expect("id from the same map") += 1;
            residue -= 1;
        }
        for (id, addition) in additions {
            *self.class_shares_permille.get_mut(&id).expect("id from the same map") += addition;
        }
        debug_assert!(self.validate().is_ok());
        Ok(())
    }

    /// The floor's effective share right now (1000‰ exactly when no class survives).
    pub fn floor_share_permille(&self) -> u16 {
        self.anti_stall_floor_permille
    }
}

/// One class's clamped retarget over its own window. `current_target` is the class lottery
/// target (bigger = easier); a class that produced fewer blocks than its share expected gets
/// easier, one that flooded gets harder — by the exact observed ratio, clamped to
/// `[current/max_factor, current×max_factor]` per adjustment so one weird window cannot
/// cliff a class (the same reason Bitcoin clamps ×4). `observed_blocks = 0` (a dead-quiet
/// window) is the full easing clamp, not a division.
pub fn adjust_class_target_v1(
    current_target: u128,
    observed_blocks: u64,
    expected_blocks: u64,
    max_factor: u32,
) -> Result<u128, PalwClassDaaError> {
    if max_factor < 2 {
        return Err(PalwClassDaaError::MaxFactorTooSmall);
    }
    if expected_blocks == 0 {
        return Err(PalwClassDaaError::ZeroExpectedBlocks);
    }
    let floor = current_target / max_factor as u128;
    let ceiling = current_target.saturating_mul(max_factor as u128);
    if observed_blocks == 0 {
        return Ok(ceiling);
    }
    // new = current × expected / observed, in u256-free arithmetic: split current into
    // high/low halves so the multiply cannot overflow u128.
    let scaled = mul_div_u128(current_target, expected_blocks as u128, observed_blocks as u128);
    Ok(scaled.clamp(floor, ceiling))
}

/// `⌊a × b / d⌋` where `b`, `d` fit u64 widened to u128. Exact by the identity
/// `a·b = (a/d)·d·b + (a%d)·b`, so `⌊a·b/d⌋ = (a/d)·b + ⌊(a%d)·b/d⌋` — the second term's
/// product fits u128 because both factors are < 2^64. The first term saturates only when the
/// TRUE quotient exceeds u128, and every saturated value still lands above the caller's
/// ceiling clamp, so saturation never changes the clamped result.
fn mul_div_u128(a: u128, b: u128, d: u128) -> u128 {
    (a / d).saturating_mul(b).saturating_add((a % d) * b / d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(seed: u64) -> Hash64 {
        Hash64::from_u64_word(seed)
    }

    fn set(floor: u16, shares: &[(u64, u16)]) -> PalwDifficultyDomainSetV1 {
        PalwDifficultyDomainSetV1::new(floor, shares.iter().map(|(s, p)| (id(*s), *p)).collect()).unwrap()
    }

    /// The conservation invariant is checked at construction: over/under 1000‰, a zero
    /// floor, and a zero-share class are all unrepresentable.
    #[test]
    fn construction_enforces_conservation() {
        assert!(PalwDifficultyDomainSetV1::new(10, [(id(1), 500), (id(2), 490)].into()).is_ok());
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(10, [(id(1), 500), (id(2), 500)].into()),
            Err(PalwClassDaaError::SharesDoNotConserve { got: 1000, floor: 10, .. })
        ));
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(0, [(id(1), 1000)].into()),
            Err(PalwClassDaaError::FloorOutOfRange { got: 0, .. })
        ));
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(10, [(id(1), 990), (id(2), 0)].into()),
            Err(PalwClassDaaError::ZeroShare { .. })
        ));
    }

    /// Freezing a class conserves shares exactly, splits proportionally, and is
    /// deterministic (whole-permille arithmetic, largest-remainder for the residue).
    #[test]
    fn removal_redistributes_proportionally_and_conserves() {
        let mut s = set(10, &[(1, 600), (2, 300), (3, 90)]);
        s.remove_class(&id(1)).unwrap();
        // 600 splits over (300, 90): 600×300/390 = 461.53… → 461, 600×90/390 = 138.46… → 138,
        // residue 1 goes to the larger remainder (class 2: .53 vs class 3: .46).
        assert_eq!(s.class_shares_permille[&id(2)], 300 + 462);
        assert_eq!(s.class_shares_permille[&id(3)], 90 + 138);
        assert!(s.validate().is_ok());
    }

    /// The last class's death hands everything to the anti-stall floor — degraded, visible,
    /// alive (W6). Removing an unknown class is an error, not a no-op (I7's spirit).
    #[test]
    fn last_removal_is_the_anti_stall_mode() {
        let mut s = set(10, &[(1, 990)]);
        s.remove_class(&id(1)).unwrap();
        assert_eq!(s.floor_share_permille(), 1000);
        assert!(s.class_shares_permille.is_empty());
        assert!(s.validate().is_ok());
        assert_eq!(s.remove_class(&id(1)), Err(PalwClassDaaError::UnknownClass { class_id: id(1) }));
    }

    /// Sequential removals conserve at every step regardless of order, and both orders end
    /// with the survivor holding everything but the floor.
    #[test]
    fn removal_order_conserves_and_converges() {
        for order in [[1u64, 2], [2, 1]] {
            let mut s = set(10, &[(1, 500), (2, 300), (3, 190)]);
            s.remove_class(&id(order[0])).unwrap();
            assert!(s.validate().is_ok());
            s.remove_class(&id(order[1])).unwrap();
            assert!(s.validate().is_ok());
            assert_eq!(s.class_shares_permille[&id(3)], 990);
        }
    }

    /// The retarget: starving classes ease by the observed ratio, flooding classes harden,
    /// both clamped at ×max_factor per window, and a silent window is the full easing clamp.
    #[test]
    fn retarget_follows_ratio_and_clamps() {
        // Starving: expected 100, observed 50 ⇒ ×2 easier.
        assert_eq!(adjust_class_target_v1(1_000_000, 50, 100, 4).unwrap(), 2_000_000);
        // Flooding: observed 400 ⇒ ÷4 — exactly at the clamp.
        assert_eq!(adjust_class_target_v1(1_000_000, 400, 100, 4).unwrap(), 250_000);
        // Beyond the clamp in both directions.
        assert_eq!(adjust_class_target_v1(1_000_000, 1_000, 100, 4).unwrap(), 250_000);
        assert_eq!(adjust_class_target_v1(1_000_000, 1, 100, 4).unwrap(), 4_000_000);
        // Dead-quiet window: full easing clamp, no division by zero.
        assert_eq!(adjust_class_target_v1(1_000_000, 0, 100, 4).unwrap(), 4_000_000);
        // Balanced window: unchanged.
        assert_eq!(adjust_class_target_v1(1_000_000, 100, 100, 4).unwrap(), 1_000_000);
        // Guard rails.
        assert_eq!(adjust_class_target_v1(1, 1, 1, 1), Err(PalwClassDaaError::MaxFactorTooSmall));
        assert_eq!(adjust_class_target_v1(1, 1, 0, 4), Err(PalwClassDaaError::ZeroExpectedBlocks));
    }

    /// The wide-target corner: a near-max u128 target retargets without overflow and lands
    /// inside the clamp (the mul_div split is exercised above the 64-bit boundary).
    #[test]
    fn retarget_is_overflow_free_at_wide_targets() {
        let wide = u128::MAX / 8;
        let eased = adjust_class_target_v1(wide, 50, 100, 4).unwrap();
        assert_eq!(eased, wide * 2);
        let hardened = adjust_class_target_v1(wide, 200, 100, 4).unwrap();
        assert_eq!(hardened, wide / 2);
    }

    /// Borsh roundtrip of the domain set.
    #[test]
    fn domain_set_roundtrips_borsh() {
        let s = set(10, &[(1, 500), (2, 490)]);
        assert_eq!(s, borsh::from_slice::<PalwDifficultyDomainSetV1>(&borsh::to_vec(&s).unwrap()).unwrap());
    }
}
