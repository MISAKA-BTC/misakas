//! **Lane LG14-A (RFC-0014 §7 on the legacy V2 route): the dispute reservation on testnet-12's own fold** — the fence test-armed on
//! the bundle's mirror (`palw_legacy_public_filer_v1` is refused by `validate_palw_v2` on every real height; these fixtures fold
//! without validating, as every R-core+ suite does).
//!
//! Every block goes through the real fold and is checked three ways by [`Chain::step_at`]: its delta re-applies to the child and
//! reverts to the parent, and the child's carriage reloads under its committed root (`into_state`: R-core+'s load invariants, the
//! dispute records against their indexes and caps, DL-1's deadlines exactly). The fence-off twin is testnet-12 as it runs.
//!
//! Run: cargo test -p kaspa-consensus-core --test lg14a_legacy_dispute_fold
#[path = "rcore_common.rs"]
mod common;
use common::*;
use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_da_rcore_v1::PalwDaClaimV1;
use kaspa_consensus_core::palw_legacy_public_filer_v1::{
    PALW_DISPUTE_RESERVATION_VERSION_V1, PalwDisputeReacquireV1, PalwDisputeReservationV1, palw_dispute_hard_deadline_v1,
};
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_EXECUTOR_REFUTED_VERSION_V1, PalwExecutorRefutedEvidenceV1, PalwIdentityRulesV1, PalwSessionRuleV1,
    palw_check_executor_refuted_at_v1,
};
use kaspa_consensus_core::palw_offence_v1::{PalwOffenceVerifyError, PalwPanelContradictionV1};
use kaspa_consensus_core::palw_state_v2::{
    PalwStateV2Error, PalwVoidReasonV2, palw_accuser_exposure_v1, palw_da_disclose_window_daa_v1,
};

/// testnet-12 with the public filer armed from genesis on the mirror the fold reads.
fn armed() -> Params {
    let mut p = t12();
    p.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
    p.sync_palw_legacy_public_filer_v1();
    assert!(p.validate_palw_legacy_public_filer_v1().is_err(), "the real validation still refuses the fence");
    p
}

/// The next block folded with `objects`, without committing it — for refusals.
fn try_step(c: &Chain, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let daa = c.daa + 1;
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    c.try_fold(&c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default()).map(|(child, _, _)| child)
}

/// A reservation of `claim` by `reserver` (the fold never reads the signature: the acceptance layer's).
fn reserve(c: &Chain, claim: Hash64, reserver: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let record = c.claim(&claim);
    PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(PalwDisputeReservationV1 {
            version: PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim,
            execution_root: record.execution_root,
            trace_root: record.trace_root,
            reserver,
        }),
        signature: Vec::new(),
    }
}

fn release(claim: Hash64, reserver: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::DisputeReleasedV1 { claim, reserver, signature: Vec::new() }
}

/// A floor claim licensed through the coverage door by the five genesis seats after its producer.
fn licensed_floor_claim(c: &mut Chain, seed: u64) -> (Hash64, Vec<(PalwBondKeyV2, Hash64)>, u64) {
    let id = c.floor_claim(seed);
    let seats = c.floor_seats();
    let bound = c.bind(id, &seats);
    let receipts = covered(id, c.anchor(&id), &seats, &[0, 1, 2, 3, 4], bound);
    c.step(&[PalwConsensusObjectV2::ReceiptLicensedV2 { claim: id, receipts }]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "the coverage licence folds");
    (id, seats, bound)
}

/// The two genesis bonds that sit on no floor panel: the bystanders.
fn bystanders(c: &Chain) -> (PalwBondKeyV2, PalwBondKeyV2) {
    let bonds = genesis_bonds(&c.p);
    (bonds[6].0, bonds[7].0)
}

fn collateral(c: &Chain, bond: &PalwBondKeyV2) -> u64 {
    c.s.bond(bond).expect("a bond").collateral
}

/// **The fence-off twin refuses the objects (the second lock), and the unarmed fold is byte-identical** with the fence `None`,
/// `Some(never())` and armed at a height the script never reaches: the same root and the same delta at every block.
#[test]
fn lg14a_unarmed_the_fold_refuses_the_objects_and_is_byte_identical() {
    let mut never = t12();
    never.palw_legacy_public_filer_v1 = Some(ForkActivation::never());
    never.sync_palw_legacy_public_filer_v1();
    let mut future = t12();
    future.palw_legacy_public_filer_v1 = Some(ForkActivation::new(9_000_000));
    future.sync_palw_legacy_public_filer_v1();
    let script = |p: Params| {
        let mut c = Chain::new(p);
        let mut roots = Vec::new();
        let (id, _, _) = licensed_floor_claim(&mut c, 0x41);
        roots.push(c.s.state_root());
        let (b6, b7) = bystanders(&c);
        // The objects are refused by the fold below the fence.
        for object in [
            reserve(&c, id, b7),
            release(id, b7),
            PalwConsensusObjectV2::DisputeReacquiredV1 {
                request: Box::new(PalwDisputeReacquireV1 {
                    version: 999,
                    claim: id,
                    reserver: b7,
                    reserved_daa: 0,
                    session_number: 0,
                    valid_until_daa: 0,
                    unit: kaspa_consensus_core::palw_da_rcore_v1::PalwDaUnitV1::Event { row: 0, tile: 0 },
                    binding: None,
                }),
                signature: Vec::new(),
            },
        ] {
            match try_step(&c, std::slice::from_ref(&object)) {
                Err(PalwStateV2Error::LegacyDisputeRefused(why)) => assert!(why.contains("not in force"), "{why}"),
                other => panic!("the second lock refuses tags 154–156 below the fence: {other:?}"),
            }
        }
        // A bystander's DA session (the int-12 non-seat route) and the claim's Final.
        c.step(&[da_accuse(id, b6, 0)]);
        roots.push(c.s.state_root());
        assert!(c.s.legacy_dispute_v1(&id).is_none());
        c.finalize(id);
        roots.push(c.s.state_root());
        roots
    };
    let base = script(t12());
    assert_eq!(script(never), base, "Some(never()) folds byte for byte");
    assert_eq!(script(future), base, "an unreached height folds byte for byte");
}

/// **RFC-0014 §7.3: a reservation holds a licensed claim past its Final floor, to the claim's own hard deadline, then lapses.** The
/// deposit is the DA-6 exposure at the claim's stage, on the reserver's free half; the lapse holds it in `dismissed_held`; the claim
/// reaches Final in the next block; at retirement the held deposit is burned — the only collateral that moves.
#[test]
fn lg14a_a_reservation_holds_final_to_the_hard_deadline_then_lapses_and_burns_at_retirement() {
    let mut c = Chain::new(armed());
    let (id, _, _) = licensed_floor_claim(&mut c, 0x42);
    let (_, reserver) = bystanders(&c);
    let floor_deadline = c.s.deadline_of(&id).expect("a licensed claim owes its Final floor");
    let before = collateral(&c, &reserver);
    c.step(&[reserve(&c, id, reserver)]);
    let record = c.s.legacy_dispute_v1(&id).expect("the record").clone();
    let claim = c.claim(&id);
    let hard = palw_dispute_hard_deadline_v1(&claim, palw_da_disclose_window_daa_v1(&c.sp));
    assert_eq!(record.hard_deadline_daa, hard, "the claim's own hard deadline: trace_retention − W_disclose");
    assert!(hard > floor_deadline, "the hold reaches past the Final floor");
    let deposit = record.live.get(&reserver).expect("the live row").deposit;
    assert!(deposit > 0);
    assert_eq!(palw_accuser_exposure_v1(&c.s, &reserver), deposit, "the deposit sits on the free half (A-6)");
    assert_eq!(c.s.deadline_of(&id), None, "the hold: DL-1 gives the claim no deadline");
    assert!(c.s.palw_accusation_pending_v1(&id), "a live reservation is a pending accusation");
    // Past the Final floor and up to the hard deadline: the claim stays licensed.
    c.step_at(floor_deadline + 5, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    c.step_at(hard, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "held to the hard deadline");
    // The lapse.
    c.step_at(hard + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let record = c.s.legacy_dispute_v1(&id).expect("the record holds the deposit").clone();
    assert!(record.live.is_empty() && record.closed.contains(&reserver));
    assert_eq!(record.dismissed_held, vec![(reserver, deposit)]);
    assert_eq!(palw_accuser_exposure_v1(&c.s, &reserver), deposit, "held, still on the free half");
    let resumed = c.s.deadline_of(&id).expect("DL-1 re-derives the deadline");
    assert!(resumed <= hard + 1, "no time credited: the passed floor fires at once");
    c.step(&[]);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::Final { .. }), "Final in the next block");
    assert_eq!(collateral(&c, &reserver), before, "nothing charged before retirement");
    // Retirement burns the held deposit, capped at the floor.
    let retire = c.s.deadline_of(&id).expect("the retirement is armed");
    let slashed_before = c.s.bond(&reserver).unwrap().slashed;
    c.step_at(retire + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.claim(&id).is_none() && c.s.legacy_dispute_v1(&id).is_none(), "the record retires with the claim");
    let burned = deposit.min(c.sp.min_collateral_sompi() as u128) as u64;
    assert_eq!(collateral(&c, &reserver), before - burned, "the burn is the held deposit");
    assert_eq!(c.s.bond(&reserver).unwrap().slashed, slashed_before + burned, "and it is recorded as slashed, paid to nobody");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &reserver), 0);
}

/// **A reservation holds a Provisional claim's bind deadline and a bound claim's receipt deadline** — the bystander's pursuit is
/// never outrun by a neutral timeout (the V3S-08 gap: past `palw_rcore_plus` only a seat's session paused a V2 claim).
#[test]
fn lg14a_a_reservation_holds_the_bind_and_receipt_timeouts() {
    let mut c = Chain::new(armed());
    let id = c.floor_claim(0x43);
    let (_, reserver) = bystanders(&c);
    let bind_deadline = c.s.deadline_of(&id).expect("a Provisional claim owes its bind deadline");
    c.step(&[reserve(&c, id, reserver)]);
    c.step_at(bind_deadline + 3, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(c.claim(&id).phase, PalwClaimPhaseV2::Provisional, "no BindTimeout while held");
    let seats = c.floor_seats();
    c.bind(id, &seats);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }), "the panel still binds");
    assert_eq!(c.s.deadline_of(&id), None, "and the bound claim owes no receipt deadline while held");
    let hard = c.s.legacy_dispute_v1(&id).unwrap().hard_deadline_daa;
    c.step_at(hard, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }), "no receipt timeout while held");
    c.step_at(hard + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(c.s.deadline_of(&id).is_some(), "the lapse re-arms DL-1's deadline");
}

/// **RFC-0014 §7.4: a reserved session rides its reservation's budget, never DA-8's shared non-seat budget** — with that budget
/// exhausted (as a producer's Sybils would exhaust it), a non-reserved bystander is refused and the reserver is admitted; its
/// session pauses the claim like a seat's and moves neither DA-8 count.
#[test]
fn lg14a_a_reserved_session_rides_its_own_budget() {
    let mut c = Chain::new(armed());
    let (id, _, _) = licensed_floor_claim(&mut c, 0x44);
    let (outsider, reserver) = bystanders(&c);
    c.s = edited(&c.sp, &c.s, |carriage| {
        carriage.da_claims.insert(id, PalwDaClaimV1 { opened_non_seat_total: 16, ..Default::default() });
    });
    match try_step(&c, &[da_accuse(id, outsider, 0)]) {
        Err(PalwStateV2Error::DaSessionBudgetExhausted { .. }) => {}
        other => panic!("DA-8's shared budget is exhausted for a non-reserved bystander: {other:?}"),
    }
    c.step(&[reserve(&c, id, reserver)]);
    c.step(&[da_accuse(id, reserver, 0)]);
    let session = c.s.da_session(&id, &reserver).expect("the reserved session is open").clone();
    assert!(session.accuser_is_seat, "seat-like: it pauses the claim (DA-5)");
    let record = c.s.da_claim(&id).expect("the DA record");
    assert_eq!((record.opened_non_seat_total, record.open_seat_sessions, record.open_other_sessions), (16, 1, 0));
    assert!(record.paused_since.is_some());
    assert_eq!(c.s.legacy_dispute_reservation_v1(&id, &reserver).unwrap().sessions_opened, 1, "counted on the reservation");
}

/// **A conviction (here a DA default of the reserved session) refunds every deposit** — the live one and a released one held in
/// `dismissed_held` — and deletes the record; the reservers' collateral never moves.
#[test]
fn lg14a_a_default_refunds_every_deposit() {
    let mut c = Chain::new(armed());
    let (id, _, _) = licensed_floor_claim(&mut c, 0x45);
    let (early, reserver) = bystanders(&c);
    let (before_early, before_reserver) = (collateral(&c, &early), collateral(&c, &reserver));
    c.step(&[reserve(&c, id, early)]);
    c.step(&[release(id, early)]);
    assert_eq!(c.s.legacy_dispute_v1(&id).unwrap().dismissed_held.len(), 1, "the released deposit is held");
    assert!(c.s.legacy_dispute_v1(&id).unwrap().live.is_empty() && c.s.deadline_of(&id).is_some(), "the hold ended");
    c.step(&[reserve(&c, id, reserver)]);
    c.step(&[da_accuse(id, reserver, 0)]);
    let deadline = c.s.da_session(&id, &reserver).unwrap().deadline_daa;
    c.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&id).phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "the unanswered reserved session defaults: {:?}",
        c.claim(&id).phase
    );
    assert!(c.s.legacy_dispute_v1(&id).is_none(), "every deposit refunded, the record gone");
    assert_eq!(palw_accuser_exposure_v1(&c.s, &early), 0);
    assert_eq!(palw_accuser_exposure_v1(&c.s, &reserver), 0);
    assert_eq!((collateral(&c, &early), collateral(&c, &reserver)), (before_early, before_reserver), "no reserver is charged");
}

/// **The admission's refusals**: the producer, a second reservation by the same bond (also after a release — the replay rule), roots
/// that are not the claim's, a voided claim, and a reservation past the hard deadline.
#[test]
fn lg14a_the_admission_refuses_by_name() {
    let mut c = Chain::new(armed());
    let (id, _, _) = licensed_floor_claim(&mut c, 0x46);
    let (b6, b7) = bystanders(&c);
    let producer = c.claim(&id).bond;
    let refused = |c: &Chain, object: PalwConsensusObjectV2, want: &str| match try_step(c, &[object]) {
        Err(PalwStateV2Error::LegacyDisputeRefused(why)) => assert!(why.contains(want), "{why} (wanted {want})"),
        other => panic!("refused ({want}): {other:?}"),
    };
    refused(&c, reserve(&c, id, producer), "producer");
    let mut wrong = reserve(&c, id, b7);
    if let PalwConsensusObjectV2::DisputeReservedV1 { reservation, .. } = &mut wrong {
        reservation.trace_root = h(0xBAD);
    }
    refused(&c, wrong, "roots");
    c.step(&[reserve(&c, id, b7)]);
    refused(&c, reserve(&c, id, b7), "before");
    c.step(&[release(id, b7)]);
    refused(&c, reserve(&c, id, b7), "before");
    refused(&c, release(id, b7), "no live reservation");
    let hard = c.s.legacy_dispute_v1(&id).unwrap().hard_deadline_daa;
    c.step_at(hard + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    if c.s.claim(&id).is_some_and(|claim| !claim.phase.is_terminal()) {
        refused(&c, reserve(&c, id, b6), "hard deadline");
    }
}

/// **RFC-0014 §7.4: past the fence another bond's open court no longer refuses a direct proof** — the same adjudicator, the same
/// evidence: `ClaimUnderSession` under int-12's rule, judged on its merits under the fence's.
#[test]
fn lg14a_a_direct_proof_is_judged_over_an_open_court() {
    let mut c = Chain::new(armed());
    let (id, _, _) = licensed_floor_claim(&mut c, 0x47);
    let (challenger, _) = bystanders(&c);
    c.step(&[court_opened(&c.s, id, challenger)]);
    assert_eq!(c.s.open_courts_of(&id), 1, "a court is open on the claim");
    let evidence = borsh::to_vec(&PalwExecutorRefutedEvidenceV1 {
        version: PALW_EXECUTOR_REFUTED_VERSION_V1,
        claim_id: id,
        contradiction: PalwPanelContradictionV1::ProducerWithholding { voided_daa: 0 },
        prompt_ids_opening: None,
        reporter_reveal: Vec::new(),
    })
    .unwrap();
    let rules = PalwIdentityRulesV1 {
        prompt_ids_form: c.p.palw_prompt_ids_form_v1(),
        base_class_id: bundle(&c.p).base_class_id,
        da_signer_liability: false,
    };
    let executor = c.claim(&id).bond;
    let judge = |rule| palw_check_executor_refuted_at_v1(&c.s, &executor, &evidence, false, false, rules, rule);
    assert_eq!(judge(PalwSessionRuleV1::RefusedUnderSession).unwrap_err(), PalwOffenceVerifyError::ClaimUnderSession);
    let past = judge(PalwSessionRuleV1::DirectProofFirst).unwrap_err();
    assert!(matches!(past, PalwOffenceVerifyError::ContradictionNotAdmitted(_)), "judged on its merits past the fence: {past:?}");
    assert_eq!(PalwSessionRuleV1::at(c.sp.legacy_public_filer_active_at(c.daa)), PalwSessionRuleV1::DirectProofFirst);
}
