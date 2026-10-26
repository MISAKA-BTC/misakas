//! **G14 lane F03 (P0-2, T05 slice): an UNLINKED colluding Sybil takes the target claim's outside reservation first.**
//!
//! The r5-f03 fold E2E (`r5_f03_colluders_refill_every_slot_and_one_outsider_reaches_a_verdict_in_time`) lets the coalition use only
//! the producer/seat bond, which the side rule classifies as inside. A coalition bond that shares neither operator id nor payout
//! payload with the claim's side is classified OUTSIDE and can open the claim's reserved first outside session. These tests pin, on
//! the real fold (`apply_palw_transition_v2` + both consistency checkers), what the current rule guarantees and what it does not:
//!
//! * while the Sybil's session runs, the honest outsider is an outside follow-on and is refused (residual: the slot is held);
//! * the Sybil cannot hold it for free: at the opening rung the colluding producer owes the move, and a silent coalition loses its own
//!   lie by the sweep's default (observed on the fold; a first version of this file assumed a challenger-side close and failed);
//! * positive control: with no Sybil the outsider convicts while the coalition holds every non-reserved slot.
//!
//! NOT covered (residual, reported): a coalition whose producer answers every rung while the Sybil goes silent at ITS turn, then a
//! fresh Sybil retakes the freed reservation before the outsider. The rule bounds that refill only by cost (fee + challenger-side
//! loss per cycle, at most `hard window / session window` cycles); it does not order outside filers fairly.
//!
//! TEST-ONLY values: ingress fee `FEE = 5` sompi (injected, not the V4 schedule), rung window 8 (cap 8, R = 4), collaterals 1,000.
//! Fixture values, not adopted parameters. Distinct bond keys here are NOT evidence of independence or of a lottery approval.
use super::*;
use crate::palw_court_paid_ingress_v1::{PalwCourtIngressScheduleV1, set_palw_court_ingress_test_schedule_v1};

const FEE: u64 = 5;

struct Schedule;
impl Schedule {
    fn set() -> Self {
        set_palw_court_ingress_test_schedule_v1(Some(PalwCourtIngressScheduleV1 { fee_sompi_per_session: FEE }));
        Self
    }
}
impl Drop for Schedule {
    fn drop(&mut self) {
        set_palw_court_ingress_test_schedule_v1(None);
    }
}

fn unlinked_bond(n: u64) -> PalwConsensusObjectV2 {
    let mut object = bond_registration_with_collateral(n, n as u8, 40 + n, 1_000);
    if let PalwConsensusObjectV2::BondRegistered { payout_payload, .. } = &mut object {
        *payout_payload = kaspa_hashes::Hash64::from_u64_word(0x9B00 + n);
    }
    object
}

struct World {
    s: PalwChainStateV2,
    p: PalwStateParamsV2,
    daa: u64,
    target: Hash64,
    coalition: Vec<PalwConsensusObjectV2>,
}

/// Bond 1 = producer and only seat of four claims (target + three decoys); bond 2 = the honest outsider; bonds 3 and 4 = unlinked
/// coalition Sybils. The coalition fills the non-reserved room (`cap − R`) with its inside bond, as the r5-f03 E2E does.
fn world() -> World {
    let p = params().with_turn_deadline_daa(8).unwrap().with_court_outsider_lane_from_daa(Some(0));
    assert_eq!(palw_max_concurrent_court_sessions_v1(p.turn_deadline_daa()), 8, "the fixture's cap");
    let mut objects = register_class_and_bond();
    for n in 2..=4 {
        objects.push(unlinked_bond(n));
    }
    let (mut s, _) = apply(&PalwChainStateV2::genesis(), &p, &ctx(1, 100, 1), &objects, None);
    let mut claims = Vec::new();
    let mut daa = 101u64;
    for nonce in 1..=4u64 {
        let env = attempt(40, nonce);
        claims.push(attempt_id_v2(&env.attempt));
        (s, _) = apply(&s, &p, &ctx(daa, daa, daa), &[], Some(&env));
        daa += 1;
    }
    let seats = vec![PalwPanelSeatV2 { bond: bond_key(1), operator_id: op_id(21) }];
    let binds: Vec<_> = claims
        .iter()
        .enumerate()
        .map(|(i, c)| PalwConsensusObjectV2::PanelBound { claim: *c, anchor: h64(77 + i as u64), seats: seats.clone() })
        .collect();
    (s, _) = apply(&s, &p, &ctx(daa, daa, daa), &binds, None);
    daa += 1;
    let licences: Vec<_> =
        claims.iter().map(|c| PalwConsensusObjectV2::ReceiptLicensed { claim: *c, receipts: seat_says(true) }).collect();
    (s, _) = apply(&s, &p, &ctx(daa, daa, daa), &licences, None);
    daa += 1;
    let target = claims[0];
    let mut coalition = Vec::new();
    for d in &claims[1..] {
        coalition.push(court_open(*d, h64(31), bond_key(1), bond_key(1)));
        coalition.push(second_court_open(*d, h64(31), bond_key(1), bond_key(1)));
    }
    // No self-accusation on the target: a silent inside responder on its own court would void the target by default, which is not
    // the attack (the coalition wants the lie to stand). The decoys' six opens already exceed the non-reserved room cap − R = 4.
    let mut w = World { s, p, daa, target, coalition };
    w.refill();
    w
}

impl World {
    /// The coalition retries every inside open, one per block. A refused retry (capacity, or a session id already used) writes
    /// nothing; the count of admitted retries is returned.
    fn refill(&mut self) -> u64 {
        let mut admitted = 0;
        for open in self.coalition.clone() {
            if let Ok((next, _)) =
                apply_palw_transition_v2(&self.s, &self.p, &ctx(self.daa, self.daa, self.daa), std::slice::from_ref(&open), None)
            {
                next.assert_internal_consistency(&self.p).expect("consistent");
                self.s = next;
                self.daa += 1;
                admitted += 1;
            }
        }
        admitted
    }

    fn try_open(&self, opener: u64) -> Result<(PalwChainStateV2, PalwStateDeltaV2), PalwStateV2Error> {
        let open = court_open(self.target, h64(31), bond_key(1), bond_key(opener));
        apply_palw_transition_v2(&self.s, &self.p, &ctx(self.daa, self.daa, self.daa), &[open], None)
    }

    fn hard(&self) -> u64 {
        crate::palw_court_clock_v2::palw_claim_court_reserve_hard_v2(
            &self.p,
            self.s.claim(&self.target).unwrap(),
            palw_da_disclose_window_daa_v1(&self.p),
        )
    }

    /// One empty block past the Sybil's session deadline: the sweep ends the silent session.
    fn sweep_until_gone(&mut self, sid: Hash64) {
        let deadline = self.s.court_session(&sid).expect("running").deadline_daa;
        self.daa = self.daa.max(deadline + 1);
        (self.s, _) = apply(&self.s, &self.p, &ctx(self.daa, self.daa, self.daa), &[], None);
        self.daa += 1;
        assert!(self.s.court_session(&sid).is_none(), "the silent Sybil's session is swept");
    }
}

/// The unlinked Sybil takes the reservation (pays once); while it runs the outsider is refused. A silent coalition then loses its
/// own lie: the sweep's opening-rung default voids the target through the Sybil's court, before the hard deadline.
#[test]
fn f03_t05_unlinked_sybil_on_the_reservation_cannot_stall_without_voiding_the_lie() {
    let _schedule = Schedule::set();
    let mut w = world();
    assert_eq!(w.s.court_sessions_iter().count(), 4, "the coalition holds exactly the non-reserved room cap - R");
    let sybil_before = w.s.bond(&bond_key(3)).unwrap().collateral;
    (w.s, _) = w.try_open(3).expect("the unlinked Sybil is classified outside and opens on the target's reservation");
    w.daa += 1;
    assert_eq!(sybil_before - w.s.bond(&bond_key(3)).unwrap().collateral, FEE, "the Sybil pays the ingress fee once");
    assert!(
        matches!(w.try_open(2), Err(PalwStateV2Error::CourtSessionsAtCapacity { .. })),
        "residual: the outsider is an outside follow-on while the Sybil holds the reservation"
    );
    let hard = w.hard();
    w.sweep_until_gone(court_session_of(w.target, h64(31), bond_key(1), bond_key(3)));
    assert!(w.daa < hard, "the default lands before the hard deadline");
    assert!(
        matches!(w.s.claim(&w.target).unwrap().phase, PalwClaimPhaseV2::Voided { .. }),
        "a silent colluding responder loses: the lie is voided through the Sybil's own court"
    );
}

/// Positive control (no Sybil): the outsider opens on the reservation, pays once, and its ExecutorGuilty verdict voids the target
/// before the hard deadline while the coalition holds every non-reserved slot. Negative control: inside retries stay refused.
#[test]
fn f03_t05_outsider_convicts_with_every_non_reserved_slot_held() {
    let _schedule = Schedule::set();
    let mut w = world();
    assert_eq!(w.refill(), 0, "negative control: no inside refill fits once cap - R is held");
    let before = w.s.bond(&bond_key(2)).unwrap().collateral;
    let hard = w.hard();
    (w.s, _) = w.try_open(2).expect("the reservation admits the outsider");
    let opened = w.daa;
    w.daa += 1;
    assert_eq!(before - w.s.bond(&bond_key(2)).unwrap().collateral, FEE, "the outsider pays once, at admission");
    assert_eq!(w.refill(), 0, "the coalition cannot take the room back");
    let close = PalwConsensusObjectV2::CourtClosed {
        session_id: court_session_of(w.target, h64(31), bond_key(1), bond_key(2)),
        verdict: PalwCourtVerdictV2::ExecutorGuilty,
        proof: crate::palw_court_v2::PalwCourtVerdictProofV2::Arithmetic {
            refutation: crate::palw_step_refute::tests::skeleton_refutation(),
            operand_openings: Vec::new(),
        },
    };
    assert!(w.daa < hard && w.daa < opened + w.p.turn_deadline_daa());
    (w.s, _) = apply(&w.s, &w.p, &ctx(w.daa, w.daa, w.daa), &[close], None);
    assert!(matches!(w.s.claim(&w.target).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "the lying claim is voided");
    assert_eq!(before - w.s.bond(&bond_key(2)).unwrap().collateral, FEE, "no second ingress charge on the close");
}

