//! **A step-3 object that does not ask the class gate cannot refuse the block's own attempt at
//! step 4** — 2026-09-24 audit #4 review item 2.
//!
//! The producer asks the fold's gate (`palw_class_admits_claim_v1`, and op 186's `panel_room`) on
//! the PARENT state; the fold admits the block's own attempt at step 4, after step 3 folded the
//! block's objects. Some of those objects raise the demand without asking the gate: a
//! `DefaultAccused` takes a licensed claim out of `ReceiptLicensed` (its replay, released at
//! licence, is owed again), a court opened on a licensed claim puts it back through
//! `open_courts_by_claim`, and a retirement takes a ready seat out of the class's budget. At
//! e93be0f2 the step-4 gate then refused the attempt the pre-check admitted, and a refused own
//! attempt disqualifies the whole block — so one accusation from any bonded party killed an honest
//! attempt block. Past the fence the attempt's room is read once, before step 3's first object,
//! and the re-charges take effect for the next block.
//!
//! Each scenario, on testnet-12's genesis through the real fold, with the short-window row
//! admitting on its eight ready genesis seats: one claim accepted, bound and licensed; the room
//! filled with attempts to exactly one; then, in ONE block, the object and an attempt the
//! pre-check admits.
//!
//! **Which row.** testnet-12's short-window row is under the held regime, and past the review a
//! held class owes every claim until Final, so on it a licence releases nothing and an accusation
//! or a court re-charges nothing. The accusation and the court are therefore also folded on that
//! row with its held-regime ladder taken out through the carriage — a class a licence releases
//! (ADR-0152 T-2(a)) — where the object DOES take the room from one to zero (asserted, so the test
//! cannot pass for want of a re-charge). The court is `CourtOpened` with the held-context ladder
//! off (this fixture's extras leave it `None`): a held dissection (`ShardCourtAccused` →
//! `open_held_dissection_v1`) reaches the demand only through the same `write_court` /
//! `open_courts_by_claim` entry, and its accusation cannot be built outside the crate. The
//! retirements need no such change: they move the budget, not the owed claims.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_step3_objects_cannot_refuse_the_own_attempt

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_bisect::PalwBisectSpaceV1;
use kaspa_consensus_core::palw_court_v2::court_session_id_v2;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2, PalwConsensusObjectV2, PalwStateDeltaV2, PalwStateV2Error,
    palw_v2_apply_one_object_v1,
};

const PRODUCERS: u64 = 12;
const ACCUSER: u64 = 77;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step3 {
    Accusation,
    Court,
    TwoRetirements,
}

type Folded = Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error>;

struct Outcome {
    /// Op 186's room for the class on the parent, and after the object alone.
    room_parent: u64,
    room_after_object: u64,
    /// The producer's pre-check on the parent, past the fence and below it.
    precheck: Result<(), PalwStateV2Error>,
    precheck_below_the_fence: Result<(), PalwStateV2Error>,
    /// The acceptance rehearsal of the object with the own attempt reserved.
    rehearsed: Result<(), String>,
    attempt_alone: Folded,
    object_alone: Folded,
    both: Folded,
    /// The same block under the rule below the fence.
    both_below_the_fence: Folded,
    first: Hash64,
    own: Hash64,
    class: Hash64,
    next_daa: u64,
}

fn scenario(p: &Params, object: Step3, released_at_licence: bool) -> Outcome {
    let b = bundle(p);
    let sp = b.state.clone();
    let seat_count = b.panel.seat_count() as usize;
    let (class, _) = model_classes(p);
    let honest = honest(p);
    let mut s = activated(&sp, &genesis_state(p), class);
    if released_at_licence {
        s = edited(&sp, &s, |c| {
            c.class_step_ladders.remove(&class);
        });
    }
    assert_eq!(s.class_is_held_v1(&class), !released_at_licence);

    let mut daa = 1_000u64;
    let mut objs: Vec<PalwConsensusObjectV2> = (1..=PRODUCERS).map(|n| bond_obj(n, RICH)).collect();
    objs.push(bond_obj(ACCUSER, RICH));
    s = readied(&sp, &s, &honest, class, daa);
    s = go(p, &sp, &s, &ctx(0x3000_0000 + daa, daa, daa, 0), &objs, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    assert_eq!(op186(p, &sp, &s, class, daa).ready_seats_now, 8);

    // The first claim: accepted, bound, licensed.
    daa += 1;
    s = readied(&sp, &s, &honest, class, daa - 1);
    let (env, key, first) = junk_attempt(class, bond_key(1), pubkey_of(1), &operator_pubkey_of(1), 1_000, 0x1, 0x1_0001);
    s = go(p, &sp, &s, &ctx(0x3000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the first attempt")
        .0;
    daa += 1;
    s = readied(&sp, &s, &honest, class, daa - 1);
    let seats = honest_seats(p, seat_count);
    s = bound(p, &sp, &s, first, &seats, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, class, daa - 1);
    s = licensed(p, &sp, &s, first, &seats, daa);
    assert!(
        matches!(s.claim(&first).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "{:?}",
        s.claim(&first).unwrap().phase
    );

    // Fill the class's room to exactly one; op 186 and the gate agree on every parent.
    let mut seed = 0x100u64;
    loop {
        let room = op186(p, &sp, &s, class, daa).panel_room;
        let verdict = gate(p, &sp, &s, class, daa);
        assert_eq!(room > 0, verdict.is_ok(), "op186 room {room} vs gate {verdict:?} at {daa}");
        if room <= 1 {
            break;
        }
        daa += 1;
        seed += 1;
        s = readied(&sp, &s, &honest, class, daa - 1);
        let producer = 1 + seed % PRODUCERS;
        let (env, key, _) =
            junk_attempt(class, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, seed, 0x2_0000 + seed);
        s = go(p, &sp, &s, &ctx(0x3000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap_or_else(|e| panic!("fill attempt at {daa}: {e}"))
            .0;
    }
    let room_parent = op186(p, &sp, &s, class, daa).panel_room;

    // The next block; its readiness rows are the parent's (the producer sees no more).
    let next = daa + 1;
    s = readied(&sp, &s, &honest, class, daa);
    let precheck = gate(p, &sp, &s, class, next);
    let precheck_below_the_fence =
        kaspa_consensus_core::palw_state_v2::palw_class_admits_claim_v1(&s, &sp, &dormant_extras(p, next), &class, next);
    let objects: Vec<PalwConsensusObjectV2> = match object {
        Step3::Accusation => {
            vec![PalwConsensusObjectV2::DefaultAccused {
                claim: first,
                missing_event_index: 0,
                accuser: bond_key(ACCUSER),
                signature: Vec::new(),
            }]
        }
        Step3::Court => {
            let c = s.claim(&first).unwrap();
            let (space, space_size) = (PalwBisectSpaceV1::TraceEvents, 16);
            let session_id = court_session_id_v2(&first, &c.trace_root, &c.bond, &bond_key(ACCUSER), space, space_size);
            vec![PalwConsensusObjectV2::CourtOpened {
                session_id,
                claim: first,
                challenger_bond: bond_key(ACCUSER),
                space,
                space_size,
                signature: Vec::new(),
            }]
        }
        // Two ready seats that sit on no panel and hold no lock.
        Step3::TwoRetirements => [honest[honest.len() - 1], honest[honest.len() - 2]]
            .into_iter()
            .map(|bond| PalwConsensusObjectV2::BondRetireRequested { bond, signature: vec![1] })
            .collect(),
    };
    seed += 1;
    let producer = 1 + seed % PRODUCERS;
    let (env, key, own) =
        junk_attempt(class, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, seed, 0x2_0000 + seed);
    let block = ctx(0x3000_0000 + next, next, next, T12_BLOCK_SUBSIDY_SOMPI);

    // The acceptance rehearsal (step 3 with the own attempt reserved) keeps the object.
    let mut rehearsal = room_extras(p, next);
    rehearsal.own_attempt_class = Some(class);
    let f = flags(p, next);
    let mut rehearsed = Ok(());
    let mut at = s.clone();
    for o in &objects {
        match palw_v2_apply_one_object_v1(
            &at,
            &sp,
            &block,
            o,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &rehearsal,
        ) {
            Ok(after) => at = after,
            Err(e) => {
                rehearsed = Err(e.to_string());
                break;
            }
        }
    }

    let attempt_alone = go(p, &sp, &s, &block, &[], PalwBlockWorkV3::Attempt(&env), key);
    let object_alone = go(p, &sp, &s, &block, &objects, PalwBlockWorkV3::None, Hash64::default());
    let both = go(p, &sp, &s, &block, &objects, PalwBlockWorkV3::Attempt(&env), key);
    let both_below_the_fence = fold_with(p, &sp, &s, &block, &objects, PalwBlockWorkV3::Attempt(&env), key, &dormant_extras(p, next));
    let room_after_object =
        object_alone.as_ref().map(|(after, _, _)| op186(p, &sp, after, class, next).panel_room).unwrap_or(u64::MAX);
    Outcome {
        room_parent,
        room_after_object,
        precheck,
        precheck_below_the_fence,
        rehearsed,
        attempt_alone,
        object_alone,
        both,
        both_below_the_fence,
        first,
        own,
        class,
        next_daa: next,
    }
}

fn show(r: &Folded) -> String {
    match r {
        Ok(_) => "folds".to_string(),
        Err(e) => format!("refused: {e}"),
    }
}

/// What every scenario asserts: the pre-check, the object and the attempt alone are what the
/// review measured; together they fold, the attempt is admitted, and the charge the object made
/// stands for the next block.
fn assert_the_attempt_keeps_its_room(p: &Params, o: &Outcome, object: Step3, recharges: bool) {
    let sp = bundle(p).state;
    println!(
        "{object:?}: room on the parent {}, after the object alone {}; pre-check {:?}; rehearsal {:?}",
        o.room_parent, o.room_after_object, o.precheck, o.rehearsed
    );
    println!(
        "{object:?}: attempt alone {}; object alone {}; both {}; both below the fence {}",
        show(&o.attempt_alone),
        show(&o.object_alone),
        show(&o.both),
        show(&o.both_below_the_fence)
    );
    assert_eq!(o.room_parent, 1, "the class has room for exactly one more claim");
    assert!(o.precheck.is_ok(), "the pre-check admits the attempt: {:?}", o.precheck);
    assert!(o.rehearsed.is_ok(), "the rehearsal keeps the object: {:?}", o.rehearsed);
    assert!(o.object_alone.is_ok(), "control: the object alone folds");
    let (after_attempt, _, _) = o.attempt_alone.as_ref().expect("control: the attempt alone is admitted");
    assert_eq!(op186(p, &sp, after_attempt, o.class, o.next_daa).panel_room, 0, "control: the attempt alone takes the last room");
    if recharges {
        assert_eq!(o.room_after_object, 0, "the premise: the object alone takes the last room, without asking the gate");
    }

    let (after, _, _) = o.both.as_ref().unwrap_or_else(|e| panic!("the object and the attempt fold in one block ({object:?}): {e}"));
    assert!(after.claim(&o.own).is_some(), "the block's own attempt is admitted");
    if object == Step3::Accusation {
        assert!(matches!(after.claim(&o.first).unwrap().phase, PalwClaimPhaseV2::DefaultDisputed { .. }), "and the accusation stands");
    }
    assert!(gate(p, &sp, after, o.class, o.next_daa + 1).is_err(), "the charge the object made takes effect for the next block");
}

#[test]
fn a_default_accusation_on_a_licensed_claim_and_the_own_attempt_fold_in_one_block() {
    let p = t12();
    let o = scenario(&p, Step3::Accusation, true);
    assert_the_attempt_keeps_its_room(&p, &o, Step3::Accusation, true);
}

#[test]
fn a_court_opened_on_a_licensed_claim_and_the_own_attempt_fold_in_one_block() {
    let p = t12();
    let o = scenario(&p, Step3::Court, true);
    assert_the_attempt_keeps_its_room(&p, &o, Step3::Court, true);
}

#[test]
fn a_default_accusation_and_the_own_attempt_fold_in_one_block_on_testnet12s_short_row() {
    let p = t12();
    let o = scenario(&p, Step3::Accusation, false);
    assert_the_attempt_keeps_its_room(&p, &o, Step3::Accusation, false);
}

#[test]
fn two_retirements_of_ready_seats_and_the_own_attempt_fold_in_one_block() {
    let p = t12();
    let o = scenario(&p, Step3::TwoRetirements, false);
    assert_the_attempt_keeps_its_room(&p, &o, Step3::TwoRetirements, true);
    // Below the fence nothing moved: the readiness path predates the rate rule, and the block the
    // old rule's pre-check admitted is still refused there (testnet-11's behaviour, unchanged).
    assert!(o.precheck_below_the_fence.is_ok(), "the old rule's pre-check admits too: {:?}", o.precheck_below_the_fence);
    assert!(
        matches!(o.both_below_the_fence, Err(PalwStateV2Error::PanelRoomExhausted { .. })),
        "below the fence the two retirements still refuse the attempt: {}",
        show(&o.both_below_the_fence)
    );
}
