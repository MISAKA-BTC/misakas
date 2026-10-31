//! **RFC-0008: one combined admission/settlement predicate over a claim's liability terms** (dormant — it belongs to
//! `palw_settlement_once_v1`, which owns RFC-0008 settlement and is `None` on every preset; nothing calls it below that fence).
//! The fold wires it into every settlement leg and every budgeted admission (`palw_state_v2::palw_settlement_liability_fold_v1`).
//!
//! A reward leg may be admitted or paid only when ALL of these hold at once, each against its OWN quantity:
//!
//! 1. **pre-draw cap** (ADR-0176): this leg's draw plus what the claim already drew fits the R the claim reserved at acceptance
//!    (a release of a reward drawn at Final draws nothing more);
//! 2. **physical collectibility**: the claim's liability `K` plus every OTHER live liability of the bond whose horizon overlaps it
//!    fits the bond's posted collateral — counted in full, never netted against another claim's unused slack;
//! 3. **collectible for the whole horizon**: the bond's collateral stays locked (no withdrawal/exit) through the liability horizon,
//!    not merely now;
//! 4. **signed horizon**: the liability horizon (including any consented extension) never passes the claim's signed hard deadline
//!    (X3);
//! 5. **signed material TTL**: the material TTL covers the whole horizon (the court can still fetch it);
//! 6. **post-Final court/default**: while the horizon is open (`now <= liability_until`) what leaves the chain's books at once
//!    (`paid_now`, plus what the claim already exported) stays within the PESG §6 export cap of `K`, and the rest VESTS past the
//!    horizon — a post-Final conviction or default can still void it;
//! 7. **retire**: the claim's state cannot retire before its horizon.
//!
//! **No slack crosses dimensions**: surplus collateral never relaxes the lock horizon, a long material TTL never relaxes the hard
//! deadline, a long vest never excuses a collectibility shortfall, unused R of another claim never funds this draw. Each check is
//! a separate refusal ([`PalwSettlementLiabilityRefusalV1`]) evaluated in a fixed order, so a fixture can pin which term refused.
//!
//! **Never default to admit.** The fold hands the predicate [`PalwSettlementTermsSourcedV1`]: each term either carries the
//! claim's or its bond's authenticated value or names why it has none; one unsourced term refuses the leg
//! ([`PalwSettlementLiabilityRefusalV1::Unsourced`]) before any check runs.
//!
//! Horizon conventions: `liability_until` is the LAST DAA a conviction or default can still land; `bond_locked_until` the last DAA
//! the collateral is still held; `vest_until` the FIRST DAA a vested leg may move.
//!
//! Pure and allocation-free. It does not grant credit or weight (RFC-0018 Gate C stays the only credit predicate) and proves no
//! computation (PRINCIPLES §8).

/// The terms of one reward leg of one claim (every field is the claim's or its bond's actual, already authenticated value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSettlementLiabilityTermsV1 {
    /// R the claim reserved at acceptance (the pre-draw cap).
    pub reserved_reward: u64,
    /// R the claim's earlier legs already drew.
    pub consumed_reward: u64,
    /// This leg's draw on the claim's R (0 for a release of a reward already drawn at Final).
    pub draw: u64,
    /// The value this leg moves to its payee (the draw itself, or a held reward released).
    pub payment: u64,
    /// Of `payment`, what leaves the chain's books at once (a payout-queue write or a market reserve); the rest vests.
    pub paid_now: u64,
    /// What the claim's earlier legs already exported while its liability held.
    pub exported_before: u64,
    /// The PESG §6 export cap of `K` (`⌊K · export_cap_permille / 1000⌋`).
    pub export_cap: u64,
    /// The claim's liability collateral `K`.
    pub liability_sompi: u128,
    /// The bond's posted collateral.
    pub bond_collateral: u128,
    /// `K` of the bond's OTHER live claims whose horizons overlap this one.
    pub other_live_liability: u128,
    /// The last DAA the bond's collateral is still held (no withdrawal or exit before it ends).
    pub bond_locked_until: u64,
    /// The last DAA a post-Final conviction or default can land (after any consented extension).
    pub liability_until: u64,
    /// The claim's signed hard deadline (X3): no extension passes it.
    pub signed_hard_deadline: u64,
    /// The last DAA the claim's material is owed.
    pub material_ttl_until: u64,
    /// The first DAA the vested part may move (`None`: nothing vests).
    pub vest_until: Option<u64>,
    /// The earliest DAA the claim's state may retire.
    pub retire_not_before: u64,
    pub now: u64,
}

/// Which term refused (fixed evaluation order: the first failing term).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PalwSettlementLiabilityRefusalV1 {
    /// A term has no authenticated source (the name says which and why); nothing was evaluated.
    Unsourced(&'static str),
    PreDrawCap,
    Uncollectible,
    CollateralUnlocksBeforeHorizon,
    HorizonPastHardDeadline,
    MaterialExpiresBeforeHorizon,
    /// Inside the horizon, the at-once part plus earlier exports exceed the export cap of `K`.
    ExportBeyondCap,
    /// Inside the horizon, a vested part does not vest past it.
    PaidBeforeHorizon,
    RetiresBeforeHorizon,
    /// `paid_now > payment` or an arithmetic overflow.
    Malformed,
}

impl PalwSettlementLiabilityRefusalV1 {
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Unsourced(why) => why,
            Self::PreDrawCap => "the draw exceeds the claim's reserved R",
            Self::Uncollectible => "the claim's liability plus the bond's other live liabilities exceed its collateral",
            Self::CollateralUnlocksBeforeHorizon => "the bond's collateral unlocks before the liability horizon",
            Self::HorizonPastHardDeadline => "the liability horizon passes the signed hard deadline (X3)",
            Self::MaterialExpiresBeforeHorizon => "the material TTL ends before the liability horizon",
            Self::ExportBeyondCap => "inside the horizon the at-once payment exceeds the PESG export cap",
            Self::PaidBeforeHorizon => "inside the horizon a vested part does not vest past it",
            Self::RetiresBeforeHorizon => "the claim may retire before its liability horizon",
            Self::Malformed => "malformed settlement terms",
        }
    }
}

/// **The combined predicate** (module doc). `Ok(())` only when every term holds against its own quantity.
pub fn palw_settlement_liability_admits_v1(t: &PalwSettlementLiabilityTermsV1) -> Result<(), PalwSettlementLiabilityRefusalV1> {
    use PalwSettlementLiabilityRefusalV1 as R;
    if t.paid_now > t.payment {
        return Err(R::Malformed);
    }
    let drawn = t.consumed_reward.checked_add(t.draw).ok_or(R::Malformed)?;
    if drawn > t.reserved_reward {
        return Err(R::PreDrawCap);
    }
    let owed = t.liability_sompi.checked_add(t.other_live_liability).ok_or(R::Malformed)?;
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
    if t.payment > 0 && t.now <= t.liability_until {
        let exported = t.exported_before.checked_add(t.paid_now).ok_or(R::Malformed)?;
        if t.paid_now > 0 && exported > t.export_cap {
            return Err(R::ExportBeyondCap);
        }
        if t.payment > t.paid_now && t.vest_until.is_none_or(|vest| vest <= t.liability_until) {
            return Err(R::PaidBeforeHorizon);
        }
    }
    if t.retire_not_before < t.liability_until {
        return Err(R::RetiresBeforeHorizon);
    }
    Ok(())
}

/// One term as the fold read it: the authenticated value, or the reason it has none.
pub type PalwSettlementTermV1<T> = Result<T, &'static str>;

/// **The terms as gathered**, each with its provenance. [`Self::resolve_v1`] refuses on the FIRST unsourced term, in field order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PalwSettlementTermsSourcedV1 {
    pub reserved_reward: PalwSettlementTermV1<u64>,
    pub consumed_reward: PalwSettlementTermV1<u64>,
    pub draw: u64,
    pub payment: u64,
    pub paid_now: u64,
    pub exported_before: PalwSettlementTermV1<u64>,
    pub export_cap: PalwSettlementTermV1<u64>,
    pub liability_sompi: PalwSettlementTermV1<u128>,
    pub bond_collateral: PalwSettlementTermV1<u128>,
    pub other_live_liability: PalwSettlementTermV1<u128>,
    pub bond_locked_until: PalwSettlementTermV1<u64>,
    pub liability_until: PalwSettlementTermV1<u64>,
    pub signed_hard_deadline: PalwSettlementTermV1<u64>,
    pub material_ttl_until: PalwSettlementTermV1<u64>,
    pub vest_until: Option<u64>,
    pub retire_not_before: PalwSettlementTermV1<u64>,
    pub now: u64,
}

impl PalwSettlementTermsSourcedV1 {
    /// Every term sourced, or the first missing one named (never a default).
    pub fn resolve_v1(&self) -> Result<PalwSettlementLiabilityTermsV1, PalwSettlementLiabilityRefusalV1> {
        use PalwSettlementLiabilityRefusalV1::Unsourced as U;
        Ok(PalwSettlementLiabilityTermsV1 {
            reserved_reward: self.reserved_reward.map_err(U)?,
            consumed_reward: self.consumed_reward.map_err(U)?,
            draw: self.draw,
            payment: self.payment,
            paid_now: self.paid_now,
            exported_before: self.exported_before.map_err(U)?,
            export_cap: self.export_cap.map_err(U)?,
            liability_sompi: self.liability_sompi.map_err(U)?,
            bond_collateral: self.bond_collateral.map_err(U)?,
            other_live_liability: self.other_live_liability.map_err(U)?,
            bond_locked_until: self.bond_locked_until.map_err(U)?,
            liability_until: self.liability_until.map_err(U)?,
            signed_hard_deadline: self.signed_hard_deadline.map_err(U)?,
            material_ttl_until: self.material_ttl_until.map_err(U)?,
            vest_until: self.vest_until,
            retire_not_before: self.retire_not_before.map_err(U)?,
            now: self.now,
        })
    }
}

/// [`palw_settlement_liability_admits_v1`] over gathered terms: an unsourced term refuses first.
pub fn palw_settlement_liability_sourced_v1(t: &PalwSettlementTermsSourcedV1) -> Result<(), PalwSettlementLiabilityRefusalV1> {
    palw_settlement_liability_admits_v1(&t.resolve_v1()?)
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
            payment: 60,
            paid_now: 10,
            exported_before: 0,
            export_cap: 255,
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

    fn sourced(t: PalwSettlementLiabilityTermsV1) -> PalwSettlementTermsSourcedV1 {
        PalwSettlementTermsSourcedV1 {
            reserved_reward: Ok(t.reserved_reward),
            consumed_reward: Ok(t.consumed_reward),
            draw: t.draw,
            payment: t.payment,
            paid_now: t.paid_now,
            exported_before: Ok(t.exported_before),
            export_cap: Ok(t.export_cap),
            liability_sompi: Ok(t.liability_sompi),
            bond_collateral: Ok(t.bond_collateral),
            other_live_liability: Ok(t.other_live_liability),
            bond_locked_until: Ok(t.bond_locked_until),
            liability_until: Ok(t.liability_until),
            signed_hard_deadline: Ok(t.signed_hard_deadline),
            material_ttl_until: Ok(t.material_ttl_until),
            vest_until: t.vest_until,
            retire_not_before: Ok(t.retire_not_before),
            now: t.now,
        }
    }

    #[test]
    fn every_term_at_its_exact_boundary_admits() {
        palw_settlement_liability_admits_v1(&good()).unwrap();
        // Past the horizon a leg may be paid straight to the queue, whole.
        palw_settlement_liability_admits_v1(&PalwSettlementLiabilityTermsV1 { now: 2_001, vest_until: None, paid_now: 60, ..good() })
            .unwrap();
        // Inside it, an at-once leg exactly at the export cap with nothing vesting.
        palw_settlement_liability_admits_v1(&PalwSettlementLiabilityTermsV1 {
            paid_now: 60,
            exported_before: 195,
            export_cap: 255,
            vest_until: None,
            ..good()
        })
        .unwrap();
        palw_settlement_liability_sourced_v1(&sourced(good())).unwrap();
    }

    #[test]
    fn each_term_refuses_on_its_own_and_names_itself() {
        let cases = [
            (PalwSettlementLiabilityTermsV1 { draw: 61, ..good() }, R::PreDrawCap),
            (PalwSettlementLiabilityTermsV1 { other_live_liability: 501, ..good() }, R::Uncollectible),
            (PalwSettlementLiabilityTermsV1 { bond_locked_until: 1_999, ..good() }, R::CollateralUnlocksBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { signed_hard_deadline: 1_999, ..good() }, R::HorizonPastHardDeadline),
            (PalwSettlementLiabilityTermsV1 { material_ttl_until: 1_999, ..good() }, R::MaterialExpiresBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { exported_before: 246, ..good() }, R::ExportBeyondCap),
            (PalwSettlementLiabilityTermsV1 { vest_until: Some(2_000), ..good() }, R::PaidBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { vest_until: None, ..good() }, R::PaidBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { retire_not_before: 1_999, ..good() }, R::RetiresBeforeHorizon),
            (PalwSettlementLiabilityTermsV1 { consumed_reward: u64::MAX, ..good() }, R::Malformed),
            (PalwSettlementLiabilityTermsV1 { paid_now: 61, ..good() }, R::Malformed),
            // A release of a reward drawn at Final draws no R, but its payment is still held to the horizon.
            (PalwSettlementLiabilityTermsV1 { draw: 0, payment: 60, paid_now: 0, vest_until: None, ..good() }, R::PaidBeforeHorizon),
        ];
        for (terms, refusal) in cases {
            assert_eq!(palw_settlement_liability_admits_v1(&terms), Err(refusal), "{terms:?}");
        }
    }

    #[test]
    fn an_unsourced_term_refuses_before_any_check_and_is_never_defaulted() {
        // Every other term would admit; the missing hard deadline alone refuses, by name.
        let mut t = sourced(good());
        t.signed_hard_deadline = Err("X3 does not govern the claim");
        assert_eq!(palw_settlement_liability_sourced_v1(&t), Err(R::Unsourced("X3 does not govern the claim")));
        // The first missing term in field order is the one named.
        t.reserved_reward = Err("no reservation row");
        assert_eq!(palw_settlement_liability_sourced_v1(&t), Err(R::Unsourced("no reservation row")));
        // Even a leg that would trivially pass (zero draw past the horizon) is refused when a term is missing.
        let mut idle = sourced(PalwSettlementLiabilityTermsV1 { draw: 0, payment: 0, paid_now: 0, now: 9_999, ..good() });
        idle.material_ttl_until = Err("no covenant");
        assert_eq!(palw_settlement_liability_sourced_v1(&idle), Err(R::Unsourced("no covenant")));
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
        let draw = PalwSettlementLiabilityTermsV1 { reserved_reward: 100, consumed_reward: 100, draw: 1, paid_now: 0, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&draw), Err(R::PreDrawCap));
        // A late retirement does not excuse paying before the horizon.
        let retire = PalwSettlementLiabilityTermsV1 { retire_not_before: u64::MAX, vest_until: None, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&retire), Err(R::PaidBeforeHorizon));
        // A vest far past the horizon does not lift the export cap on the at-once part.
        let export = PalwSettlementLiabilityTermsV1 { vest_until: Some(u64::MAX), exported_before: 250, paid_now: 10, ..good() };
        assert_eq!(palw_settlement_liability_admits_v1(&export), Err(R::ExportBeyondCap));
    }
}
