//! **G14 item 6 (ADR-0200 C1 / RFC-0033 §2, lane r5-d2-c1): the D2 readiness registry's one writer and the disclosure court fold.**
//! DORMANT.
//!
//! Every write goes through [`TransitionBuilder::write_d2_readiness_v1`], which journals `D2Readiness { key, old, new }` (delta 230):
//! a revert swaps `old`/`new` (`apply_delta_entry`), so a reorg restores the parent's roots and cases exactly, root for root.
//!
//! Callers: the tag-230 arm (record), tags 231/232 (demand/answer), and the block's deadline sweep after every object. All of them
//! are reached only past `palw_court_model_scope_v1` (the acceptance gate routes the objects; the sweep checks the mirror), and the
//! record/demand refuse while the draw cap ([`rr::PALW_D2_READINESS_DRAW_CAP_V1`]) and the fee schedule are unadopted — so on every
//! network nothing is ever written and the registry stays empty (no root block, no tail).
//!
//! The ML-DSA-87 signatures (demand: the challenger's bond; answer: the root's seat bond) are checked by the validator
//! (`palw_validate/court.rs`) before the fold, the same split tag 230 uses.
use super::*;
use crate::palw_court_model_scope_routes_v1::readiness_disclosure as dc;
use crate::palw_court_model_scope_routes_v1::readiness_root as rr;
use std::num::NonZeroU32;

fn refused(why: dc::PalwD2DisclosureRefusalV1) -> PalwStateV2Error {
    PalwStateV2Error::ReadinessRootRefused(why.as_str())
}

impl TransitionBuilder<'_> {
    /// The one writer: journals the write.
    pub(super) fn write_d2_readiness_v1(
        &mut self,
        key: dc::PalwD2ReadinessKeyV1,
        new: Option<dc::PalwD2ReadinessEntryV1>,
    ) -> Result<(), PalwStateV2Error> {
        let old = self.state.d2_readiness.get(&key).cloned();
        match &new {
            Some(entry) => self.state.d2_readiness.insert(key, entry.clone()),
            None => self.state.d2_readiness.remove(&key),
        };
        self.entries.push(PalwDeltaEntryV2::D2Readiness { key, old, new });
        Ok(())
    }

    fn d2_court_active_v1(&self, daa_score: u64) -> bool {
        self.params.court_model_scope_from_daa().is_some_and(|from| daa_score >= from)
    }

    /// **Tag 230**: admit against `cap` (the shipped constant is `None` → refusal) and record once per (beacon, class, seat, span).
    pub(super) fn record_d2_readiness_root_v1(
        &mut self,
        root: &rr::SeatReadinessRootV1,
        cap: Option<NonZeroU32>,
        daa_score: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.d2_court_active_v1(daa_score) {
            return Err(PalwStateV2Error::ReadinessRootRefused("the seat readiness root is dormant (palw_court_model_scope_v1)"));
        }
        if !self.state.bonds.contains_key(&root.seat) {
            return Err(PalwStateV2Error::ReadinessRootRefused("a readiness root names a seat bond this chain does not have"));
        }
        let (key, entry) = dc::palw_d2_readiness_record_v1(&self.state.d2_readiness, root, cap, daa_score).map_err(refused)?;
        self.write_d2_readiness_v1(key, Some(entry))
    }

    /// **Tag 231**: open a disclosure case. The challenger must be a bond of this chain whose free collateral covers the fee.
    pub(super) fn open_d2_disclosure_v1(
        &mut self,
        demand: &dc::PalwD2DisclosureDemandV1,
        cap: Option<NonZeroU32>,
        fees: Option<dc::PalwD2DisclosureFeeScheduleV1>,
        daa_score: u64,
    ) -> Result<(), PalwStateV2Error> {
        if !self.d2_court_active_v1(daa_score) {
            return Err(PalwStateV2Error::ReadinessRootRefused("the readiness disclosure court is dormant (palw_court_model_scope_v1)"));
        }
        let (key, entry) = dc::palw_d2_disclosure_open_v1(&self.state.d2_readiness, demand, cap, fees, daa_score).map_err(refused)?;
        let bond = self
            .state
            .bonds
            .get(&demand.challenger)
            .ok_or(PalwStateV2Error::ReadinessRootRefused("the demand names a challenger bond this chain does not have"))?;
        if bond.collateral.saturating_sub(bond.slashed) < demand.fee_sompi {
            return Err(PalwStateV2Error::ReadinessRootRefused("the challenger's free collateral does not cover the demand fee"));
        }
        self.write_d2_readiness_v1(key, Some(entry))
    }

    /// **Tag 232**: the seat's answer — discloses, or convicts the root on a path its own root refutes.
    pub(super) fn answer_d2_disclosure_v1(&mut self, answer: &dc::PalwD2DisclosureAnswerV1, daa_score: u64) -> Result<(), PalwStateV2Error> {
        if !self.d2_court_active_v1(daa_score) {
            return Err(PalwStateV2Error::ReadinessRootRefused("the readiness disclosure court is dormant (palw_court_model_scope_v1)"));
        }
        for (key, entry) in dc::palw_d2_disclosure_answer_v1(&self.state.d2_readiness, answer, daa_score).map_err(refused)? {
            self.write_d2_readiness_v1(key, Some(entry))?;
        }
        Ok(())
    }

    /// **The block's deadline sweep** (after every object): unanswered cases past their inclusive deadline default under the F02
    /// supply premise, else hold. A no-op below the fence or on an empty registry (every network).
    pub(super) fn sweep_d2_disclosures_v1(&mut self, daa_score: u64, supply_premise: bool) -> Result<(), PalwStateV2Error> {
        if !self.d2_court_active_v1(daa_score) || self.state.d2_readiness.is_empty() {
            return Ok(());
        }
        // audit B-08 (fix-b): the premise (a pinned service profile) AND a court service opportunity the accepting chain RECORDED
        // inside this case's own window (`ServiceReservations::supply`, one entry per first block at a DAA that left the reserved
        // capacity unspent). A case whose window recorded none is a no-supply hold, never a default.
        let writes = {
            let book = self.state.court_service_state_v1().map(|s| &s.book);
            let supplied = |case: &dc::PalwD2DisclosureCaseV1| {
                supply_premise
                    && book.is_some_and(|book| book.supply.range(case.opened_daa..=case.deadline_daa).next().is_some())
            };
            dc::palw_d2_disclosure_sweep_with_supply_v1(&self.state.d2_readiness, daa_score, &supplied)
        };
        for (key, entry) in writes {
            self.write_d2_readiness_v1(key, Some(entry))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    const FENCE: u64 = 1_000;
    fn params(armed: bool) -> PalwStateParamsV2 {
        PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 0)
            .unwrap()
            .with_court_model_scope_from_daa(armed.then_some(FENCE))
    }
    fn bond_key(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n), 0))
    }
    fn delta(daa: u64, entries: Vec<PalwDeltaEntryV2>) -> PalwStateDeltaV2 {
        PalwStateDeltaV2 { point: PalwBlockContextV2 { block: BlockHash::from_u64_word(daa), daa_score: daa, blue_score: 1, subsidy: 0 }, entries }
    }
    /// A genesis state with the seat (3) and a challenger (9), each a bond with free collateral.
    fn parent() -> PalwChainStateV2 {
        let mut s = PalwChainStateV2::genesis();
        for n in [3, 9] {
            let b = PalwBondStateV2 {
                pubkey: vec![n as u8; 4],
                operator_id: h(100 + n),
                collateral: 1_000,
                slashed: 0,
                status: PalwBondStatusV2::Active,
                registered_daa: 0,
                payout_payload: h(200 + n),
                capable_classes: Default::default(),
            };
            s.bonds.insert(bond_key(n), b);
        }
        s
    }
    fn fixture(count: u32) -> (rr::SeatReadinessRootV1, Vec<(u32, Hash64)>) {
        let ctx = rr::ReadinessContextV1 { beacon: h(11), class_id: h(12), seat: bond_key(3), span: 4 };
        let ds: Vec<_> = (0..count).map(|i| (i * 2, h(500 + u64::from(i)))).collect();
        (rr::SeatReadinessRootV1::from_digests(ctx, &ds, NonZeroU32::new(count)).unwrap(), ds)
    }
    const FEES: Option<dc::PalwD2DisclosureFeeScheduleV1> = Some(dc::PalwD2DisclosureFeeScheduleV1 { demand_fee_sompi: 7 });

    /// Dormant below the fence and with the shipped (unadopted) cap/fee: nothing is written, the root is the parent's.
    #[test]
    fn d2c1_registry_is_dormant_below_the_fence_and_refuses_unadopted_values() {
        let extras = PalwTransitionExtrasV1::default();
        let parent = parent();
        let (root, _) = fixture(3);
        for armed in [false, true] {
            let p = params(armed);
            let mut b = TransitionBuilder::new(&parent, &p, false, false, false, false, &extras);
            assert!(b.record_d2_readiness_root_v1(&root, NonZeroU32::new(3), FENCE - 1).is_err(), "below the fence");
            assert!(b.record_d2_readiness_root_v1(&root, rr::PALW_D2_READINESS_DRAW_CAP_V1, FENCE).is_err(), "unadopted cap");
            b.sweep_d2_disclosures_v1(FENCE + 10_000, true).unwrap();
            assert!(b.entries.is_empty());
            assert!(b.state.d2_readiness_v1().is_empty());
            assert_eq!(b.state.state_root(), parent.state_root());
        }
    }

    /// **Reorg/undo**: record → demand → wrong-path answer (conviction), each block's delta reverted in turn restores the previous
    /// registry and state root exactly; the carriage round-trips the registry and refuses an inconsistent row.
    #[test]
    fn d2c1_record_demand_convict_reverts_block_by_block_and_round_trips_the_carriage() {
        let p = params(true);
        let extras = PalwTransitionExtrasV1::default();
        let s0 = parent();
        let (root, ds) = fixture(4);
        let cap = NonZeroU32::new(4);
        let rk = rr::palw_d2_readiness_root_key_v1(&root.context());

        let mut b = TransitionBuilder::new(&s0, &p, false, false, false, false, &extras);
        b.record_d2_readiness_root_v1(&root, cap, FENCE).unwrap();
        assert!(b.record_d2_readiness_root_v1(&root, cap, FENCE).is_err(), "once per seat and draw");
        let (s1, d1) = (b.state.clone(), delta(FENCE, b.entries.clone()));

        let demand = dc::PalwD2DisclosureDemandV1 { root_key: rk, ordinal: 2, challenger: bond_key(9), fee_sompi: 7, valid_until_daa: FENCE + 5 };
        let mut b = TransitionBuilder::new(&s1, &p, false, false, false, false, &extras);
        assert!(b.open_d2_disclosure_v1(&demand, cap, None, FENCE + 1).is_err(), "unadopted fee schedule");
        let mut unfunded = demand;
        unfunded.challenger = bond_key(77);
        assert!(b.open_d2_disclosure_v1(&unfunded, cap, FEES, FENCE + 1).is_err(), "unknown challenger bond");
        b.open_d2_disclosure_v1(&demand, cap, FEES, FENCE + 1).unwrap();
        let (s2, d2) = (b.state.clone(), delta(FENCE + 1, b.entries.clone()));

        let mut path = rr::palw_d2_readiness_path_v1(&root.context(), &ds, 2);
        path.claimed_digest = h(999);
        let answer = dc::PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: 2, path };
        let mut b = TransitionBuilder::new(&s2, &p, false, false, false, false, &extras);
        b.answer_d2_disclosure_v1(&answer, FENCE + 2).unwrap();
        let (s3, d3) = (b.state.clone(), delta(FENCE + 2, b.entries.clone()));
        assert!(matches!(
            s3.d2_readiness_v1().get(&dc::PalwD2ReadinessKeyV1::Root(rk)),
            Some(dc::PalwD2ReadinessEntryV1::Root(r)) if matches!(r.standing, dc::PalwD2ReadinessStandingV1::PathConvicted { ordinal: 2, .. })
        ));

        for state in [&s1, &s2, &s3] {
            let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(state)).unwrap();
            let back: PalwStateCarriageV2 = borsh::from_slice(&bytes).unwrap();
            assert_eq!(&back.d2_readiness, state.d2_readiness_v1());
        }
        let mut bad = s2.clone();
        if let Some(dc::PalwD2ReadinessEntryV1::Case(c)) = bad.d2_readiness.get_mut(&dc::PalwD2ReadinessKeyV1::Case(rk, 2)) {
            c.demand.ordinal = 3; // a case under another coordinate's key
        }
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&bad)).unwrap();
        assert!(borsh::from_slice::<PalwStateCarriageV2>(&bytes).is_err());

        let u2 = revert_delta_v2(&s3, &d3, &p).unwrap();
        assert_eq!(u2.d2_readiness_v1(), s2.d2_readiness_v1());
        assert_eq!(u2.state_root(), s2.state_root());
        let u1 = revert_delta_v2(&u2, &d2, &p).unwrap();
        assert_eq!(u1.state_root(), s1.state_root());
        let u0 = revert_delta_v2(&u1, &d1, &p).unwrap();
        assert!(u0.d2_readiness_v1().is_empty());
        assert_eq!(u0.state_root(), s0.state_root(), "an empty registry roots exactly as before");
    }

    /// **Reorg of the deadline sweep**: an unanswered case closes past its inclusive deadline (hold without the premise, default
    /// with it); reverting the sweep's delta reopens the case, so the other branch's in-time answer can still land.
    #[test]
    fn d2c1_deadline_sweep_reverts_and_the_reopened_case_takes_an_in_time_answer() {
        let p = params(true);
        let extras = PalwTransitionExtrasV1::default();
        let (root, ds) = fixture(2);
        let cap = NonZeroU32::new(2);
        let rk = rr::palw_d2_readiness_root_key_v1(&root.context());
        let mut b = TransitionBuilder::new(&parent(), &p, false, false, false, false, &extras);
        b.record_d2_readiness_root_v1(&root, cap, FENCE).unwrap();
        let demand = dc::PalwD2DisclosureDemandV1 { root_key: rk, ordinal: 0, challenger: bond_key(9), fee_sompi: 7, valid_until_daa: FENCE };
        b.open_d2_disclosure_v1(&demand, cap, FEES, FENCE).unwrap();
        let plain = b.state.clone();
        let deadline = FENCE + dc::PALW_D2_DISCLOSURE_TURN_DAA_V1;
        // audit B-08 (fix-b): the default needs a court service opportunity RECORDED inside the case's own window.
        let supplied_at = |daa: u64| {
            use crate::palw_court_service_v1::{ServiceCost, ServiceReservations};
            let mut book = ServiceReservations::new(ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 }).unwrap();
            book.observe_block(daa, [1; 64], Default::default()).unwrap();
            let h = Hash64::from_u64_word(0xD2);
            let svc = crate::palw_court_service_state_v2::CourtServiceStateV2 { version: 2, source_profile_id: h,
                profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(h), book, claims: Default::default() };
            let mut s = plain.clone();
            s.staged_courts.insert(crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::CourtService,
                crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::CourtService(Box::new(svc).into()));
            s
        };
        let inside = supplied_at(FENCE + 1);
        let outside = supplied_at(deadline + 1);

        for (open, premise, closes) in [(&plain, false, 1), (&inside, true, 2), (&plain, true, 1), (&outside, true, 1)] {
            let open = open.clone();
            let mut b = TransitionBuilder::new(&open, &p, false, false, false, false, &extras);
            b.sweep_d2_disclosures_v1(deadline, premise).unwrap();
            assert!(b.entries.is_empty(), "the deadline DAA itself is in time");
            b.sweep_d2_disclosures_v1(deadline + 1, premise).unwrap();
            assert_eq!(b.entries.len(), closes, "premise {premise}: default only with supply recorded in the case window");
            let swept = b.state.clone();
            let undone = revert_delta_v2(&swept, &delta(deadline + 1, b.entries.clone()), &p).unwrap();
            assert_eq!(undone.state_root(), open.state_root());
            let mut b = TransitionBuilder::new(&undone, &p, false, false, false, false, &extras);
            let answer = dc::PalwD2DisclosureAnswerV1 { root_key: rk, ordinal: 0, path: rr::palw_d2_readiness_path_v1(&root.context(), &ds, 0) };
            b.answer_d2_disclosure_v1(&answer, deadline).unwrap();
            assert!(matches!(
                b.state.d2_readiness_v1().get(&dc::PalwD2ReadinessKeyV1::Case(rk, 0)),
                Some(dc::PalwD2ReadinessEntryV1::Case(c)) if matches!(c.outcome, dc::PalwD2DisclosureOutcomeV1::Disclosed { .. })
            ));
        }
    }
}
