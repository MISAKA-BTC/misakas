//! **RFC-0008: one combined admission/settlement predicate over a claim's liability terms** (dormant — it belongs to
//! `palw_settlement_once_v1`, which owns RFC-0008 settlement and is `None` on every preset; nothing calls it below that fence).
//!
//! A reward leg may be admitted or paid only when ALL of these hold at once, each against its OWN quantity:
//!
//! 1. **pre-draw cap** (ADR-0176): this leg's draw plus what the claim already drew fits the R the claim reserved at acceptance;
//! 2. **physical collectibility**: the claim's liability `K` plus every OTHER live liability of the bond whose horizon overlaps it
//!    fits the bond's posted collateral — counted in full, never netted against another claim's unused slack;
//! 3. **collectible for the whole horizon**: the bond's collateral stays locked (no withdrawal/exit) through the liability horizon,
//!    not merely now;
//! 4. **signed horizon**: the liability horizon (including any consented extension) never passes the claim's signed hard deadline
//!    (X3);
//! 5. **signed material TTL**: the providers' signed material TTL covers the whole horizon (the court can still fetch it);
//! 6. **post-Final court/default**: while the horizon is open (`now <= liability_until`) a paid leg must VEST past the horizon —
//!    a post-Final conviction or default can still void it;
//! 7. **retire**: the claim's state cannot retire before its horizon.
//!
//! **No slack crosses dimensions**: surplus collateral never relaxes the lock horizon, a long material TTL never relaxes the hard
//! deadline, a long vest never excuses a collectibility shortfall, unused R of another claim never funds this draw. Each check is
//! a separate refusal ([`PalwSettlementLiabilityRefusalV1`]) evaluated in a fixed order, so a fixture can pin which term refused.
//!
//! Pure and allocation-free. It does not grant credit or weight (RFC-0018 Gate C stays the only credit predicate).

/// The terms of one reward leg of one claim (every field is the claim's or its bond's actual, already authenticated value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSettlementLiabilityTermsV1 {
    /// R the claim reserved at acceptance (the pre-draw cap).
    pub reserved_reward: u64,
    /// R the claim's earlier legs already drew.
    pub consumed_reward: u64,
    /// This leg's draw.
    pub draw: u64,
    /// The claim's liability collateral `K`.
    pub liability_sompi: u128,
    /// The bond's posted collateral.
    pub bond_collateral: u128,
    /// `K` of the bond's OTHER live claims whose horizons overlap this one.
    pub other_live_liability: u128,
    /// The bond cannot withdraw or exit before this DAA.
    pub bond_locked_until: u64,
    /// The claim's liability horizon (post-Final court/default window end, after any consented extension).
    pub liability_until: u64,
    /// The claim's signed hard deadline (X3): no extension passes it.
    pub signed_hard_deadline: u64,
    /// The providers' signed material TTL.
    pub material_ttl_until: u64,
    /// When this leg vests (`None`: written straight to the payout queue).
    pub vest_until: Option<u64>,
    /// The earliest DAA the claim's state may retire.
    pub retire_not_before: u64,
    pub now: u64,
}

/// Which term refused (fixed evaluation order: the first failing term).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSettlementLiabilityRefusalV1 {
    PreDrawCap,
    Uncollectible,
    CollateralUnlocksBeforeHorizon,
    HorizonPastHardDeadline,
    MaterialExpiresBeforeHorizon,
    PaidBeforeHorizon,
    RetiresBeforeHorizon,
    Overflow,
}

/// **The combined predicate** (module doc). `Ok(())` only when every term holds against its own quantity.
pub fn palw_settlement_liability_admits_v1(t: &PalwSettlementLiabilityTermsV1) -> Result<(), PalwSettlementLiabilityRefusalV1> {
    use PalwSettlementLiabilityRefusalV1 as R;
    let drawn = t.consumed_reward.checked_add(t.draw).ok_or(R::Overflow)?;
    if drawn > t.reserved_reward {
        return Err(R::PreDrawCap);
    }
    let owed = t.liability_sompi.checked_add(t.other_live_liability).ok_or(R::Overflow)?;
    if owed > t.bond_collateral {
        return Err(R::Uncollectible);
    }
    if t.bond_locked_until < t.liability_until {
        return Err(R::CollateralUnlocksBeforeHorizon);
    }
    if t.liability_until > t.signed_hard_deadline {
        return Err(R::HorizonPastHardDeadline);
    }
    if t.material_ttl_until < t.liability_until {
        return Err(R::MaterialExpiresBeforeHorizon);
    }
    if t.draw > 0 && t.now <= t.liability_until && t.vest_until.is_none_or(|vest| vest <= t.liability_until) {
        return Err(R::PaidBeforeHorizon);
    }
    if t.retire_not_before < t.liability_until {
        return Err(R::RetiresBeforeHorizon);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! SOURCE ONLY (not compiled or executed in this lane).
    use super::*;
    use PalwSettlementLiabilityRefusalV1 as R;

    fn good() -> PalwSettlementLiabilityTermsV1 {
        PalwSettlementLiabilityTermsV1 {
            reserved_reward: 100,
            consumed_reward: 40,
            draw: 60,
            liability_sompi: 500,
            bond_collateral: 1_000,
            other_live_liability: 500,
            bond_locked_until: 2_000,
            liability_until: 2_000,
            signed_hard_deadline: 2_500,
            material_ttl_until: 2_000,
            vest_until: Some(2_001),
            retire_not_before: 2_000,
            now: 1_500,
        }
    }

    #[test]
    fn every_term_at_its_exact_boundary_admits() {
        palw_settlement_liability_admits_v1(&good()).unwrap();
        // Past the horizon a leg may be paid straight to the queue.
        palw_settlement_liability_admits_v1(&PalwSettlementLiabilityTermsV1 { now: 2_001, vest_until: None, ..good() }).unwrap();
    }

    #[test]
    fn each_term_refuses_on_its_own_and_names_itself() {
        let cases = [
            (PalwSettlementLiabilityTermsV1 { draw: 61, ..good() }, R::PreDrawCap),
            (PalwSettlementLiabilityTermsV1 { other_live_liability: 501, ..good() }, R::Uncollectible),
            (PalwSettlementLiabilityTermsV1 { bond_locked_until: 1_999, ..good() }, R::CollateralUnlocksBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { signed_hard_deadline: 1_999, ..good() }, R::HorizonPastHardDeadline),
            (PalwSettlementLiabilityTermsV1 { material_ttl_until: 1_999, ..good() }, R::MaterialExpiresBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { vest_until: Some(2_000), ..good() }, R::PaidBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { vest_until: None, ..good() }, R::PaidBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { retire_not_before: 1_999, ..good() }, R::RetiresBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { consumed_reward: u64::MAX, ..good() }, R::Overflow),
        ];
        for (terms, refusal) in cases {
            assert_eq!(palw_settlement_liability_admits_v1(&terms), Err(refusal), "{terms:?}");
        }
    }

    #[test]
    fn no_dimension_is_satisfied_by_another_dimensions_slack() {
        // Abundant collateral does not excuse a lock that ends before the horizon.
        let rich = PalwSettlementLiabilityTermsV1 { bond_collateral: u128::MAX / 2, bond_locked_until: 1_999, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&rich), Err(R::CollateralUnlocksBeforeHorizon));
        // A long material TTL does not move the signed hard deadline.
        let ttl = PalwSettlementLiabilityTermsV1 { material_ttl_until: u64::MAX, signed_hard_deadline: 1_999, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&ttl), Err(R::HorizonPastHardDeadline));
        // A long vest does not excuse a collectibility shortfall.
        let vest = PalwSettlementLiabilityTermsV1 { vest_until: Some(u64::MAX), other_live_liability: 501, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&vest), Err(R::Uncollectible));
        // Unused R elsewhere never funds this claim's draw: only its own reservation counts.
        let draw = PalwSettlementLiabilityTermsV1 { reserved_reward: 100, consumed_reward: 100, draw: 1, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&draw), Err(R::PreDrawCap));
        // A late retirement does not excuse paying before the horizon.
        let retire = PalwSettlementLiabilityTermsV1 { retire_not_before: u64::MAX, vest_until: None, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&retire), Err(R::PaidBeforeHorizon));
    }
}
