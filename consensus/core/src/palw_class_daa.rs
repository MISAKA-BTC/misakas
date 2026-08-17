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
//! * [`PalwDifficultyDomainSetV1`] — the Active classes and their cadence shares. The invariant
//!   `Σ class shares = 1000‰` holds at every mutation, and freezing a class redistributes its
//!   share over the survivors proportionally, deterministically (largest-remainder, class-id
//!   order).
//!
//!   **The residual survivor is the BASE class, not a hash lane** (ADR-0039 W6′, superseding
//!   ADR-0038 W6). An earlier shape carried an `anti_stall_floor_permille` that `validate`
//!   refused to let reach zero, and that took the whole 1000‰ when the last class was removed —
//!   i.e. it structurally guaranteed a spam-hash lane could always produce blocks. ADR-0039
//!   removes that lane: block production is PALW work, and the liveness floor is a portable
//!   integer-only class (`PALW-BASE-0`) held permanently Active, whose share may never be zero
//!   and which may never be removed. With no class able to produce certified work the network
//!   HALTS LOUDLY by design — a visible stop, not a silent fork onto cheap hashes.
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
    #[error("shares sum to {got}‰ — must sum to exactly {denominator}‰")]
    SharesDoNotConserve { got: u32, denominator: u16 },
    #[error("the base class {class_id} is not in the domain set — the liveness floor may never be absent")]
    BaseClassAbsent { class_id: Hash64 },
    #[error("the base class {class_id} may not be removed — it is the liveness floor (ADR-0039 W6′)")]
    BaseClassNotRemovable { class_id: Hash64 },
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
    /// The liveness floor: a class that is always Active, always holds a nonzero share, and can
    /// never be removed (ADR-0039 W6′). `PALW-BASE-0` in practice — the portable integer-only
    /// class whose catalog closes, so it can be audited and convicted on any CPU.
    ///
    /// This replaced an `anti_stall_floor_permille` that guaranteed a spam-hash lane. The floor is
    /// PALW work now; there is no hash lane to fall back to, and that is the point.
    pub base_class_id: Hash64,
    /// Active class → share (permille). BTreeMap: every iteration below is id-ordered, so
    /// every node redistributes identically.
    pub class_shares_permille: BTreeMap<Hash64, u16>,
}

impl PalwDifficultyDomainSetV1 {
    /// Build a validated set. `base_class_id` is the liveness floor and must be one of the shares.
    pub fn new(base_class_id: Hash64, class_shares_permille: BTreeMap<Hash64, u16>) -> Result<Self, PalwClassDaaError> {
        let set = Self { base_class_id, class_shares_permille };
        set.validate()?;
        Ok(set)
    }

    /// The conservation invariant, the zero-share rule, and the base class's presence.
    pub fn validate(&self) -> Result<(), PalwClassDaaError> {
        // The floor must BE a class in the set. There is no lane outside the classes any more, so
        // an absent base class is a set with no guaranteed producer at all (ADR-0039 W6′).
        if !self.class_shares_permille.contains_key(&self.base_class_id) {
            return Err(PalwClassDaaError::BaseClassAbsent { class_id: self.base_class_id });
        }
        if let Some((class_id, _)) = self.class_shares_permille.iter().find(|(_, share)| **share == 0) {
            return Err(PalwClassDaaError::ZeroShare { class_id: *class_id });
        }
        let classes: u32 = self.class_shares_permille.values().map(|s| *s as u32).sum();
        if classes != PALW_CLASS_SHARE_DENOMINATOR as u32 {
            return Err(PalwClassDaaError::SharesDoNotConserve {
                got: classes,
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
        // The floor is not removable. Freezing every other class leaves the base class holding the
        // whole cadence, which is the designed degraded mode; removing the base class would leave a
        // set with no producer, and there is no hash lane behind it to catch that (ADR-0039 W6′).
        if *class_id == self.base_class_id {
            return Err(PalwClassDaaError::BaseClassNotRemovable { class_id: *class_id });
        }
        let Some(removed_share) = self.class_shares_permille.remove(class_id) else {
            return Err(PalwClassDaaError::UnknownClass { class_id: *class_id });
        };
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

    /// The base class's effective share right now — 1000‰ exactly when every other class has been
    /// frozen out, which is the designed degraded mode: the whole cadence on the portable
    /// integer-only floor, still PALW work, still auditable and convictable.
    pub fn base_share_permille(&self) -> u16 {
        *self.class_shares_permille.get(&self.base_class_id).expect("validate() guarantees the base class is present")
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

    /// `id(0)` is the base class in every fixture below — the liveness floor.
    fn base() -> Hash64 {
        id(0)
    }

    fn set(base_share: u16, shares: &[(u64, u16)]) -> PalwDifficultyDomainSetV1 {
        let mut map: BTreeMap<Hash64, u16> = shares.iter().map(|(s, p)| (id(*s), *p)).collect();
        map.insert(base(), base_share);
        PalwDifficultyDomainSetV1::new(base(), map).unwrap()
    }

    /// Construction enforces conservation to exactly 1000‰, refuses a zero-share class, and
    /// refuses a set whose base class is absent — the floor must BE a class now, because there is
    /// no hash lane outside the classes to fall back on (ADR-0039 W6′).
    #[test]
    fn construction_enforces_conservation_and_the_base_class() {
        assert!(PalwDifficultyDomainSetV1::new(base(), [(base(), 10), (id(1), 500), (id(2), 490)].into()).is_ok());
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(base(), 10), (id(1), 500), (id(2), 500)].into()),
            Err(PalwClassDaaError::SharesDoNotConserve { got: 1010, .. })
        ));
        // The base class must be present...
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(id(1), 1000)].into()),
            Err(PalwClassDaaError::BaseClassAbsent { .. })
        ));
        // ...and, like every class, must hold a nonzero share.
        assert!(matches!(
            PalwDifficultyDomainSetV1::new(base(), [(base(), 0), (id(1), 1000)].into()),
            Err(PalwClassDaaError::ZeroShare { .. })
        ));
    }

    /// Freezing a class conserves shares exactly, splits proportionally, and is
    /// deterministic (whole-permille arithmetic, largest-remainder for the residue).
    #[test]
    fn removal_redistributes_proportionally_and_conserves() {
        let mut s = set(10, &[(1, 600), (2, 300), (3, 90)]);
        s.remove_class(&id(1)).unwrap();
        // 600 splits over the survivors, which now INCLUDE the base class (10): total 400, so
        // 600×10/400 = 15, 600×300/400 = 450, 600×90/400 = 135 — exact, no residue.
        assert_eq!(s.base_share_permille(), 10 + 15);
        assert_eq!(s.class_shares_permille[&id(2)], 300 + 450);
        assert_eq!(s.class_shares_permille[&id(3)], 90 + 135);
        assert!(s.validate().is_ok());
    }

    /// Freezing every other class leaves the BASE class holding the whole cadence — degraded,
    /// visible, and still PALW work (ADR-0039 W6′). The base class itself is not removable, so the
    /// set can never reach "no producer"; with nothing able to certify work the network halts
    /// loudly rather than falling back to a hash lane, which is the trade ADR-0039 makes
    /// deliberately.
    #[test]
    fn the_base_class_is_the_degraded_mode_and_cannot_be_removed() {
        let mut s = set(10, &[(1, 990)]);
        s.remove_class(&id(1)).unwrap();
        assert_eq!(s.base_share_permille(), 1000, "the floor takes the whole cadence");
        assert_eq!(s.class_shares_permille.len(), 1, "and it is still a CLASS, not a lane outside them");
        assert!(s.validate().is_ok());
        // The floor cannot be removed, and an unknown class is an error rather than a no-op.
        assert_eq!(s.remove_class(&base()), Err(PalwClassDaaError::BaseClassNotRemovable { class_id: base() }));
        assert_eq!(s.remove_class(&id(1)), Err(PalwClassDaaError::UnknownClass { class_id: id(1) }));
    }

    /// Sequential removals conserve at every step regardless of order, and both orders end
    /// with the survivor holding everything but the floor.
    #[test]
    fn removal_order_conserves_and_is_a_function_of_the_order() {
        let mut outcomes = Vec::new();
        for order in [[1u64, 2], [2, 1]] {
            let mut s = set(10, &[(1, 500), (2, 300), (3, 190)]);
            s.remove_class(&id(order[0])).unwrap();
            assert!(s.validate().is_ok(), "conservation holds at every step");
            s.remove_class(&id(order[1])).unwrap();
            assert!(s.validate().is_ok());
            outcomes.push((s.base_share_permille(), s.class_shares_permille[&id(3)]));
        }
        // Conservation is absolute in both orders.
        for (base, c3) in &outcomes {
            assert_eq!(*base as u32 + *c3 as u32, PALW_CLASS_SHARE_DENOMINATOR as u32);
        }
        // But the split is NOT order-independent, and this is a real property, not a rounding
        // nicety: largest-remainder redistribution is path-dependent, and putting the liveness floor
        // INSIDE the class map (ADR-0039 W6′) made it observable — the base class now takes a share
        // of every redistribution. MEASURED: removing 1 then 2 lands (50, 950); 2 then 1 lands
        // (49, 951).
        assert_eq!(outcomes[0], (50, 950));
        assert_eq!(outcomes[1], (49, 951));
        assert_ne!(outcomes[0], outcomes[1], "if these ever agree, the note above is stale");

        // The consequence is a REQUIREMENT ON THE CALLER, which is why it is pinned here: the
        // removal order must be a function of the chain (the order classes were frozen in), never
        // of a node's iteration order over a pending set. Two nodes applying the same freezes in
        // different orders would carry shares differing by the residue and then retarget apart.
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

