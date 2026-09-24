//! **The panel room holds a class to Final by its verification window, not by ADR-0119's held
//! ladder** — ADR-0152's C7 by the window rule (`palw_panel_held_to_final_v1`: a window of at least
//! `PALW_RCORE_C7_WINDOW_SPANS_V1` = 1,000 spans), the fix for the 2026-09-24 re-review of
//! f8c91f19.
//!
//! The two predicates answer different questions. `class_is_held_v1` asks whether a class recorded
//! its own step ladder at registration, which decides its court and its data availability. The
//! room asks whether the panel can be taken to have replayed a claim by its licence. On
//! testnet-12's genesis both model rows record a held ladder, while only the 2M row's window is
//! C7's. Both directions, through the real fold:
//!
//! * a class WITH a held ladder and a window under 1,000 spans is NOT held: the short-window row as
//!   shipped. Its licence releases its claim, and moving only its window across the threshold
//!   through the carriage moves op 186's verdict both ways;
//! * a class with a window of 1,000 spans or more IS held, with its ladder or without it (the
//!   re-review's HELD-2, first case): the 2M row as shipped, and with its ladder taken out through
//!   the carriage. A licence does not free its slot: the room stays 0 from Provisional through the
//!   licence-to-Final window, a second claim is `ClassInflightCapped { 1/1 }` while the first is
//!   Provisional and while it is licensed, and it folds once the first is Final;
//! * the threshold itself through the fold (HELD-2, second case): the short row, ladder kept, with
//!   its registered verification compute set so the fold's own span step derives 999 spans, and
//!   1,000. At 999 a second claim folds beside the first, Provisional or licensed; at 1,000 it
//!   meets the derived cap.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_held_to_final_is_the_window_rule

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2, PalwStateDeltaV2, PalwStateParamsV2, PalwStateV2Error,
};
use kaspa_consensus_core::palw_work_target_v1::{
    PALW_RCORE_C7_WINDOW_SPANS_V1, palw_panel_capacity_by_rate_v1, palw_panel_held_to_final_v1,
};

const PRODUCER: u64 = 9_501;
const SECOND: u64 = 9_502;

/// `s` with `class`'s window set to `spans` (read-only: the span step re-derives the window at the
/// next boundary, and nothing folds on this state).
fn with_window(sp: &PalwStateParamsV2, s: &PalwChainStateV2, class: Hash64, spans: u32) -> PalwChainStateV2 {
    edited(sp, s, |c| c.model_lifecycles.get_mut(&class).expect("a model row").profile.verification_window_spans = spans)
}

#[test]
fn a_class_with_a_held_ladder_and_a_window_under_1000_spans_is_not_held() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let g = genesis_state(&p);
    let row = g.model_lifecycle(&short).expect("the short row").clone();
    assert!(g.class_is_held_v1(&short), "the premise: the short row records ADR-0119's held ladder");
    assert!(row.profile.verification_window_spans < PALW_RCORE_C7_WINDOW_SPANS_V1, "and its window is under C7's");
    assert!(!palw_panel_held_to_final_v1(&row));

    let mut daa = 1_000u64;
    let mut s = readied(&sp, &activated(&sp, &g, short), &honest, short, daa);
    s = go(&p, &sp, &s, &ctx(0x5500_0000 + daa, daa, daa, 0), &[bond_obj(PRODUCER, RICH)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the producer bonds")
        .0;
    let read = |s: &PalwChainStateV2, daa: u64| (owed(&p, s, short), op186(&p, &sp, s, short, daa).panel_room);
    let empty = read(&s, daa);

    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    let (env, key, claim) =
        junk_attempt(short, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), 1_000, 0x55_01, 0x55_0100);
    s = go(&p, &sp, &s, &ctx(0x5500_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the attempt")
        .0;
    let provisional = read(&s, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    s = bound(&p, &sp, &s, claim, &seats, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    s = licensed(&p, &sp, &s, claim, &seats, daa);
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let licensed_now = read(&s, daa);

    // The same licensed state with only the window moved, the ladder kept: one span under C7's
    // threshold the licence still releases the claim; at the threshold it is owed to Final and the
    // room is the static cap's.
    let per_span = {
        let globals = registry_fold(&p, daa).expect("the registry").globals;
        8 * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000
    };
    let claim_replay = s.model_lifecycle(&short).unwrap().work.economic_ccu_per_claim * b.panel.seat_count() as u128;
    let capacity = |spans: u32| palw_panel_capacity_by_rate_v1(per_span, 0, spans as u64, claim_replay);
    let under = with_window(&sp, &s, short, PALW_RCORE_C7_WINDOW_SPANS_V1 - 1);
    let at = with_window(&sp, &s, short, PALW_RCORE_C7_WINDOW_SPANS_V1);
    assert!(under.class_is_held_v1(&short) && at.class_is_held_v1(&short), "the ladder is kept in both");
    let (under_read, at_read) = (read(&under, daa), read(&at, daa));
    println!(
        "short row (owed, op186 room): empty {empty:?}, provisional {provisional:?}, licensed {licensed_now:?}; licensed with a \
         {}-span window {under_read:?} (capacity {}), with a {}-span window {at_read:?} (capacity {})",
        PALW_RCORE_C7_WINDOW_SPANS_V1 - 1,
        capacity(PALW_RCORE_C7_WINDOW_SPANS_V1 - 1),
        PALW_RCORE_C7_WINDOW_SPANS_V1,
        capacity(PALW_RCORE_C7_WINDOW_SPANS_V1)
    );

    assert_eq!((empty, provisional), ((0, 5), (1, 4)), "eight ready seats hold five; an accepted claim is owed");
    assert_eq!(licensed_now, (0, 5), "licensed, it is released: nothing owed, the room whole (T-2(a))");
    assert_eq!(
        under_read,
        (0, capacity(PALW_RCORE_C7_WINDOW_SPANS_V1 - 1)),
        "999 spans: still released, and the room is the capacity — the cap of 5 is not read"
    );
    assert!(under_read.1 > row.profile.max_inflight_claims as u64);
    assert_eq!(at_read, (1, row.profile.max_inflight_claims as u64 - 1), "1,000 spans: owed to Final, and held to the cap");
    assert!(capacity(PALW_RCORE_C7_WINDOW_SPANS_V1) - 1 > at_read.1, "the cap, not the capacity, is what binds there");
}

/// **HELD-2, the first case: a licensed claim of a class held to Final does not free its slot.**
/// One 2M claim accepted, bound, licensed and swept to Final on eight ready seats. A second 2M
/// attempt is folded beside it while it is Provisional, while it is ReceiptLicensed, and after it is
/// Final. The room is read at every step, and across the licence-to-Final window at L+10, L+40, L+70
/// and L+100 (the checkpoints the re-review read). `ladder` keeps the row's ADR-0119 held ladder
/// (testnet-12 as shipped) or takes it out through the carriage; the outcome is the same, because
/// the hold is the window's.
fn held_2m_walk(ladder: bool) {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (_, id2m) = model_classes(&p);
    let honest = honest(&p);
    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let mut g = genesis_state(&p);
    if !ladder {
        g = edited(&sp, &g, |c| {
            c.class_step_ladders.remove(&id2m);
        });
    }
    let row = g.model_lifecycle(&id2m).expect("the 2M row").clone();
    assert_eq!(g.class_is_held_v1(&id2m), ladder, "the premise: the 2M row's ADR-0119 ladder is kept ({ladder}) or taken out");
    assert!(row.profile.verification_window_spans >= PALW_RCORE_C7_WINDOW_SPANS_V1, "and its window is C7's");
    assert!(palw_panel_held_to_final_v1(&row));
    assert_eq!(row.profile.max_inflight_claims, 1, "c_2M = 1");

    let mut daa = 1_000u64;
    let mut s = readied(&sp, &activated(&sp, &g, id2m), &honest, id2m, daa);
    s = go(
        &p,
        &sp,
        &s,
        &ctx(0x5600_0000 + daa, daa, daa, 0),
        &[bond_obj(PRODUCER, RICH), bond_obj(SECOND, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the producers bond")
    .0;
    let read = |s: &PalwChainStateV2, daa: u64| (owed(&p, s, id2m), op186(&p, &sp, s, id2m, daa).panel_room);
    let capped = |r: &Folded| matches!(r, Err(PalwStateV2Error::ClassInflightCapped { class, inflight: 1, cap: 1 }) if *class == id2m);
    let empty = read(&s, daa);

    daa += 1;
    s = readied(&sp, &s, &honest, id2m, daa - 1);
    let pwu = class_pwu(&p, &s, id2m, daa);
    let (env, key, claim) =
        junk_attempt(id2m, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), pwu, 0x56_01, 0x56_0100);
    let (env2, key2, second) =
        junk_attempt(id2m, bond_key(SECOND), pubkey_of(SECOND), &operator_pubkey_of(SECOND), pwu, 0x56_02, 0x56_0200);
    let attempt_second = |s: &PalwChainStateV2, daa: u64| {
        go(&p, &sp, s, &ctx(0x5600_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env2), key2)
    };
    s = go(&p, &sp, &s, &ctx(0x5600_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the 2M attempt")
        .0;
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional), "{:?}", s.claim(&claim).unwrap().phase);
    let provisional = read(&s, daa);
    let while_provisional = attempt_second(&s, daa + 1);

    daa += 1;
    s = readied(&sp, &s, &honest, id2m, daa - 1);
    s = bound(&p, &sp, &s, claim, &seats, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, id2m, daa - 1);
    s = licensed(&p, &sp, &s, claim, &seats, daa);
    let licensed_daa = daa;
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let licensed_now = read(&s, daa);
    let while_licensed = attempt_second(&s, daa + 1);

    // Across the licence-to-Final window: an empty block at each checkpoint, the claim still live.
    let challenge = sp.window_challenge_at(licensed_daa);
    let mut through_the_window = Vec::new();
    for after in [10u64, 40, 70, 100] {
        assert!(after < challenge, "L+{after} is inside the {challenge}-DAA challenge window");
        daa = licensed_daa + after;
        s = readied(&sp, &s, &honest, id2m, daa - 1);
        s = go(&p, &sp, &s, &ctx(0x5600_0000 + daa, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default())
            .unwrap_or_else(|e| panic!("the block at L+{after}: {e}"))
            .0;
        assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "L+{after}: still licensed");
        through_the_window.push((after, read(&s, daa), capped(&attempt_second(&s, daa + 1))));
    }

    // One block past the challenge window: Final, and the slot is free.
    daa = licensed_daa + challenge + 1;
    s = readied(&sp, &s, &honest, id2m, daa - 1);
    s = go(&p, &sp, &s, &ctx(0x5600_0000 + daa, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("the sweep").0;
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Final { .. }));
    let final_now = read(&s, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, id2m, daa - 1);
    let after_final = attempt_second(&s, daa);
    let show = |r: &Folded| r.as_ref().map(|(st, _, _)| st.claim(&second).is_some()).map_err(|e| format!("{e:?}"));
    println!(
        "2M, ADR-0119 ladder {ladder} (owed, op186 room): empty {empty:?}, provisional {provisional:?}, licensed {licensed_now:?}, \
         (L+n, (owed, room), second capped) {through_the_window:?}, Final {final_now:?}; second claim while provisional {:?}, \
         while licensed {:?}, after Final {:?}",
        show(&while_provisional),
        show(&while_licensed),
        show(&after_final)
    );

    assert_eq!(empty, (0, 1), "eight ready seats: the 2M room is its cap of one (ladder {ladder})");
    assert_eq!(provisional, (1, 0), "an accepted 2M claim owes one claim and fills the room (ladder {ladder})");
    assert!(capped(&while_provisional), "a second claim beside the Provisional one meets c_2M = 1 (ladder {ladder})");
    assert_eq!(licensed_now, (1, 0), "a licence releases nothing: owed to Final (ladder {ladder})");
    assert!(capped(&while_licensed), "a second claim beside the licensed one meets c_2M = 1 (ladder {ladder})");
    assert_eq!(
        through_the_window,
        vec![(10, (1, 0), true), (40, (1, 0), true), (70, (1, 0), true), (100, (1, 0), true)],
        "the room stays 0 and the cap refuses a second claim across the whole licence-to-Final window (ladder {ladder})"
    );
    assert_eq!(final_now, (0, 1), "Final releases the slot (ladder {ladder})");
    assert!(
        after_final.is_ok_and(|(st, _, _)| st.claim(&second).is_some()),
        "and the second claim folds once the first is Final (ladder {ladder})"
    );
}

type Folded = Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error>;

#[test]
fn the_2m_row_as_shipped_is_held_to_final_provisional_licensed_and_across_the_challenge_window() {
    held_2m_walk(true);
}

#[test]
fn a_class_with_a_window_of_1000_spans_or_more_and_no_held_ladder_is_held_to_final() {
    held_2m_walk(false);
}

/// What [`window_walk`] reads for one window.
#[derive(Debug)]
struct WindowWalk {
    /// The window and the static cap the fold's own span step derived from the edited work.
    window: u32,
    cap: u32,
    /// The rate capacity on the empty panel at this window (the other rows hold nothing).
    capacity: u64,
    /// `(owed, op-186 room)` with claim #1 Provisional, then ReceiptLicensed.
    provisional: (u128, u64),
    licensed: (u128, u64),
    /// Whether a second attempt folds beside #1 Provisional, then beside #1 licensed (`Ok(true)`),
    /// or the error that refused it.
    second_while_provisional: Result<bool, PalwStateV2Error>,
    second_while_licensed: Result<bool, PalwStateV2Error>,
}

/// The short row with its ADR-0119 ladder kept and its REGISTERED verification compute set so that
/// the registry's own span step derives a window of `spans` from it (the step re-derives the
/// profile at every boundary from the row's work, so an edited profile alone would not survive a
/// fold). One claim accepted, bound and licensed on eight ready seats, and a second attempt folded
/// beside it at each phase.
///
/// The edit writes the window into the row's profile as well, as a registration would have: the
/// step judges `window_fits_receipt` by the per-class receipt deadline read off the row it is
/// stepping, and a row whose stored window disagrees with its work would be `Held` for the one
/// boundary until the two agree.
fn window_walk(spans: u32) -> WindowWalk {
    use kaspa_consensus_core::palw_model_registry_v1::{PalwModelWorkV1, palw_verification_window_spans_v1};
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let mut daa = 1_000u64;
    let globals = registry_fold(&p, daa).expect("the registry").globals;
    let g = genesis_state(&p);
    let work = g.model_lifecycle(&short).expect("the short row").work;
    // The least verification compute whose window is `spans` (the window is monotone in it).
    let window_of = |ccu: u128| palw_verification_window_spans_v1(&PalwModelWorkV1 { verification_ccu: ccu, ..work }, &globals);
    assert!(window_of(0) < spans);
    let mut hi = work.verification_ccu.max(1);
    while window_of(hi) < spans {
        hi = hi.saturating_mul(2);
    }
    let mut lo = 0u128;
    while hi - lo > 1 {
        let mid = lo + (hi - lo) / 2;
        if window_of(mid) >= spans { hi = mid } else { lo = mid }
    }
    assert_eq!(window_of(hi), spans, "a verification compute derives exactly {spans} spans");

    let mut s = edited(&sp, &activated(&sp, &g, short), |c| {
        let row = c.model_lifecycles.get_mut(&short).expect("the short row");
        row.work.verification_ccu = hi;
        row.profile.verification_window_spans = spans;
    });
    s = readied(&sp, &s, &honest, short, daa);
    // The first block crosses a span boundary: the fold's own step re-derives the row's profile.
    s = go(
        &p,
        &sp,
        &s,
        &ctx(0x5700_0000 + daa, daa, daa, 0),
        &[bond_obj(PRODUCER, RICH), bond_obj(SECOND, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("the producers bond")
    .0;
    let row = s.model_lifecycle(&short).expect("the short row").clone();
    assert!(row.state.admits_claims(), "the short row still admits at a {spans}-span window: {:?}", row.state);
    assert!(s.class_is_held_v1(&short), "the ladder is kept");
    let read = |s: &PalwChainStateV2, daa: u64| (owed(&p, s, short), op186(&p, &sp, s, short, daa).panel_room);
    let ready = op186(&p, &sp, &s, short, daa).ready_seats_now as u128;
    let per_span = ready * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
    let capacity = palw_panel_capacity_by_rate_v1(
        per_span,
        0,
        row.profile.verification_window_spans as u64,
        row.work.economic_ccu_per_claim * b.panel.seat_count() as u128,
    );

    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    let (env, key, claim) =
        junk_attempt(short, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), 1_000, 0x57_01, 0x57_0100);
    let (env2, key2, second) =
        junk_attempt(short, bond_key(SECOND), pubkey_of(SECOND), &operator_pubkey_of(SECOND), 1_000, 0x57_02, 0x57_0200);
    let attempt_second = |s: &PalwChainStateV2, daa: u64| {
        go(&p, &sp, s, &ctx(0x5700_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env2), key2)
            .map(|(st, _, _)| st.claim(&second).is_some())
    };
    s = go(&p, &sp, &s, &ctx(0x5700_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the first attempt")
        .0;
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::Provisional));
    let provisional = read(&s, daa);
    let second_while_provisional = attempt_second(&s, daa + 1);
    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    s = bound(&p, &sp, &s, claim, &seats, daa);
    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    s = licensed(&p, &sp, &s, claim, &seats, daa);
    assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(s.model_lifecycle(&short).unwrap().profile.verification_window_spans, spans, "the window held across the walk");
    let licensed = read(&s, daa);
    let second_while_licensed = attempt_second(&s, daa + 1);
    WindowWalk {
        window: row.profile.verification_window_spans,
        cap: row.profile.max_inflight_claims,
        capacity,
        provisional,
        licensed,
        second_while_provisional,
        second_while_licensed,
    }
}

/// **HELD-2, the second case: the window rule's boundary through the fold, not only the unit
/// test.** The same row, its ADR-0119 ladder kept in both, at a window the fold derived as 999
/// spans and as 1,000. At 999 it is an ordinary class: its static cap is not read, a second claim
/// folds beside the first while that one is Provisional (the re-review's N4 probe), and the licence
/// releases it. At 1,000 it is C7: owed to Final, and a second claim meets the static cap the same
/// span step derived — one claim — though the rate capacity has room for nearly two thousand.
#[test]
fn the_window_threshold_through_the_fold_999_spans_is_released_and_1000_is_held() {
    let under = window_walk(PALW_RCORE_C7_WINDOW_SPANS_V1 - 1);
    let at = window_walk(PALW_RCORE_C7_WINDOW_SPANS_V1);
    println!("999 spans: {under:?}");
    println!("1,000 spans: {at:?}");

    assert_eq!((under.window, at.window), (PALW_RCORE_C7_WINDOW_SPANS_V1 - 1, PALW_RCORE_C7_WINDOW_SPANS_V1));
    assert_eq!((under.cap, at.cap), (1, 1), "the span step derives a static cap of one at both windows");
    assert_eq!(under.provisional, (1, under.capacity - 1), "999 spans: the room is the capacity less the claim, the cap unread");
    assert!(under.provisional.1 > under.cap as u64, "far past the static cap of {}", under.cap);
    assert!(
        matches!(under.second_while_provisional, Ok(true)),
        "999 spans: a second claim folds beside the Provisional one: {:?}",
        under.second_while_provisional
    );
    assert_eq!(under.licensed, (0, under.capacity), "999 spans: the licence releases the claim (T-2(a))");
    assert!(matches!(under.second_while_licensed, Ok(true)), "{:?}", under.second_while_licensed);

    assert!(at.capacity > at.cap as u64 + 1, "1,000 spans: the rate capacity ({}) is not what binds", at.capacity);
    assert_eq!(at.provisional, (1, 0), "1,000 spans: the room is the cap's");
    assert!(
        matches!(at.second_while_provisional, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "1,000 spans: a second claim beside the Provisional one meets the cap: {:?}",
        at.second_while_provisional
    );
    assert_eq!(at.licensed, (1, 0), "1,000 spans: the licence releases nothing, owed to Final");
    assert!(
        matches!(at.second_while_licensed, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "1,000 spans: a second claim beside the licensed one meets the cap: {:?}",
        at.second_while_licensed
    );
}
