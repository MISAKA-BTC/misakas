//! **testnet-12's short-window row is released at licence and judged by the room; only the 2M row
//! is held to Final, at its static cap of 1** — ADR-0152 T-2(a)/(b) as approved: the 8k slot is
//! released at licence, and only the 2M lane (C7) is held to Final with c = `max_inflight_claims`.
//!
//! f8c91f19 keyed the hold on `class_is_held_v1`, ADR-0119's held regime: a class that recorded its
//! own step ladder at registration. On testnet-12 BOTH genesis model rows record one, so the
//! short-window row (the review's "8k" row: window 3, `max_inflight_claims` 5) was held too. After
//! each of five licences it read (owed, op-186 room) (1,4) (2,3) (3,2) (4,1) (5,0), and a sixth
//! attempt met `ClassInflightCapped { inflight: 5, cap: 5 }`: about a fifth of the row's
//! throughput (the 2026-09-24 re-review of f8c91f19, probe
//! `review2_held_the_8k_class_is_capped_and_held_to_final`). The hold is now ADR-0152's C7 by the
//! window rule (`palw_panel_held_to_final_v1`: a verification window of at least 1,000 spans). On
//! testnet-12's genesis that selects the 2M row alone.
//!
//! Everything through the real fold on testnet-12's genesis:
//! * the short row admitting on eight ready seats (capacity 5): five claims accepted, bound and
//!   licensed one after another, then a sixth attempt and more, up to the room;
//! * beside it, the 2M row still held to Final at its cap of 1;
//! * the short row's empty-panel room past eight ready seats, which is its capacity, not its cap.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_short_class_is_released_at_licence

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwClaimPhaseV2, PalwStateV2Error};
use kaspa_consensus_core::palw_work_target_v1::{
    PALW_RCORE_C7_WINDOW_SPANS_V1, palw_panel_capacity_by_rate_v1, palw_panel_held_to_final_v1,
};

const PRODUCERS: u64 = 12;

fn live_on(s: &PalwChainStateV2, class: Hash64) -> usize {
    s.claims_iter().filter(|(_, c)| c.class_id == class && !c.phase.is_terminal()).count()
}

#[test]
fn the_genesis_c7_set_is_the_2m_row_alone() {
    let p = t12();
    let g = genesis_state(&p);
    let (short, id2m) = model_classes(&p);
    let held: Vec<_> =
        genesis_classes(&p).iter().map(|c| c.0).filter(|id| g.model_lifecycle(id).is_some_and(palw_panel_held_to_final_v1)).collect();
    assert_eq!(held, vec![id2m], "the panel room holds the 2M row alone to Final");
    assert!(g.class_is_held_v1(&short) && g.class_is_held_v1(&id2m), "both genesis model rows record ADR-0119's held ladder");
    let profile = |id| g.model_lifecycle(&id).expect("a model row").profile;
    assert_eq!(
        (profile(short).verification_window_spans, profile(id2m).verification_window_spans),
        (3, 2_799),
        "windows: short 3 spans, under C7's {PALW_RCORE_C7_WINDOW_SPANS_V1}; 2M 2,799, over it"
    );
    assert_eq!(
        (profile(short).max_inflight_claims, profile(id2m).max_inflight_claims),
        (5, 1),
        "static caps: short 5 (not read past the fence), 2M 1 (c_2M = 1)"
    );
}

#[test]
fn the_short_row_is_released_at_licence_and_a_sixth_claim_is_admitted_beside_five_licensed() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let seat_count = b.panel.seat_count() as usize;
    let row = genesis_state(&p).model_lifecycle(&short).unwrap().clone();
    assert!(!palw_panel_held_to_final_v1(&row), "the premise: a 3-span window is not C7, though the row records a held ladder");
    let cap = row.profile.max_inflight_claims as u64;

    let mut daa = 1_000u64;
    let mut s = readied(&sp, &activated(&sp, &genesis_state(&p), short), &honest, short, daa);
    let bonds: Vec<_> = (1..=PRODUCERS).map(|n| bond_obj(n, RICH)).collect();
    s = go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, 0), &bonds, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    assert_eq!(op186(&p, &sp, &s, short, daa).panel_room, 5, "eight ready seats hold five short-row claims");

    // Five claims, each accepted, bound and licensed before the next.
    let mut after_licence = Vec::new();
    for i in 0..cap {
        daa += 1;
        s = readied(&sp, &s, &honest, short, daa - 1);
        let producer = 1 + i % PRODUCERS;
        let (env, key, claim) =
            junk_attempt(short, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, 0x50 + i, 0x5_0000 + i);
        s = go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap_or_else(|e| panic!("claim {i} at {daa}: {e}"))
            .0;
        let seats: Vec<_> = genesis_bonds(&p).iter().cycle().skip(i as usize).take(seat_count).map(|(k, o, _)| (*k, *o)).collect();
        daa += 1;
        s = readied(&sp, &s, &honest, short, daa - 1);
        s = bound(&p, &sp, &s, claim, &seats, daa);
        daa += 1;
        s = readied(&sp, &s, &honest, short, daa - 1);
        s = licensed(&p, &sp, &s, claim, &seats, daa);
        assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        after_licence.push((owed(&p, &s, short), op186(&p, &sp, &s, short, daa).panel_room));
    }
    let licensed_live = live_on(&s, short);

    // Past the five licensed claims: more attempts, one a block, until the room refuses one.
    let mut admitted = Vec::new();
    let refused = loop {
        daa += 1;
        s = readied(&sp, &s, &honest, short, daa - 1);
        let n = cap + 1 + admitted.len() as u64;
        let producer = 1 + (n - 1) % PRODUCERS;
        let (env, key, claim) =
            junk_attempt(short, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, 0x50 + n, 0x5_0000 + n);
        match go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key) {
            Ok((next, _, _)) => {
                assert!(next.claim(&claim).is_some(), "claim {n} is live");
                s = next;
                admitted.push((owed(&p, &s, short), op186(&p, &sp, &s, short, daa).panel_room));
            }
            Err(e) => break e,
        }
        assert!(admitted.len() <= 16, "the room bounds the unlicensed claims");
    };
    println!("(owed, op186 room) after each licence: {after_licence:?}; live licensed short claims {licensed_live}");
    println!("(owed, op186 room) after each further attempt: {admitted:?}; then {refused:?}");

    assert_eq!(licensed_live, 5, "five licensed short-row claims wait out their challenge window");
    assert_eq!(after_licence, vec![(0, 5); 5], "each licence releases its claim's replay (T-2(a)): nothing owed, the room whole");
    assert_eq!(
        admitted,
        vec![(1, 4), (2, 3), (3, 2), (4, 1), (5, 0)],
        "a sixth attempt is admitted beside five licensed claims, and so are four more: the room judges them, not the cap of 5"
    );
    assert_eq!(live_on(&s, short), 10, "ten live claims on a row whose static cap is 5");
    assert!(
        matches!(refused, PalwStateV2Error::PanelRoomExhausted { class, .. } if class == short),
        "the eleventh meets the room, not the cap: {refused:?}"
    );
}

#[test]
fn beside_it_the_2m_row_is_still_held_to_final_at_its_cap_of_one() {
    const P2M: u64 = 9_401;
    const P2M_SECOND: u64 = 9_402;
    const PSHORT: u64 = 9_403;
    const PSHORT_SECOND: u64 = 9_404;
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, id2m) = model_classes(&p);
    let honest = honest(&p);
    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let g = genesis_state(&p);
    assert!(palw_panel_held_to_final_v1(g.model_lifecycle(&id2m).unwrap()), "the premise: the 2M row is C7");
    let rd = |s: &PalwChainStateV2, daa: u64| {
        let mut s = s.clone();
        for id in [short, id2m] {
            s = readied(&sp, &s, &honest, id, daa);
        }
        s
    };
    // ((short owed, short room), (2M owed, 2M room)).
    let read = |s: &PalwChainStateV2, daa: u64| {
        ((owed(&p, s, short), op186(&p, &sp, s, short, daa).panel_room), (owed(&p, s, id2m), op186(&p, &sp, s, id2m, daa).panel_room))
    };

    let mut daa = 1_000u64;
    let mut s = g.clone();
    for id in [short, id2m] {
        s = activated(&sp, &s, id);
    }
    s = rd(&s, daa);
    let bonds: Vec<_> = [P2M, P2M_SECOND, PSHORT, PSHORT_SECOND].into_iter().map(|n| bond_obj(n, RICH)).collect();
    s = go(&p, &sp, &s, &ctx(0x5200_0000 + daa, daa, daa, 0), &bonds, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    let empty = read(&s, daa);

    // One 2M claim, then one short-row claim, each accepted, bound and licensed.
    let pwu_2m = class_pwu(&p, &s, id2m, daa + 1);
    let mut licences = Vec::new();
    for (class, producer, pwu, seed) in [(id2m, P2M, pwu_2m, 0x52_01u64), (short, PSHORT, 1_000, 0x52_02)] {
        daa += 1;
        s = rd(&s, daa - 1);
        let (env, key, claim) =
            junk_attempt(class, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), pwu, seed, seed << 16);
        s = go(&p, &sp, &s, &ctx(0x5200_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .unwrap_or_else(|e| panic!("the attempt on {class}: {e}"))
            .0;
        daa += 1;
        s = rd(&s, daa - 1);
        s = bound(&p, &sp, &s, claim, &seats, daa);
        daa += 1;
        s = rd(&s, daa - 1);
        s = licensed(&p, &sp, &s, claim, &seats, daa);
        assert!(matches!(s.claim(&claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        licences.push(daa);
    }
    let both_licensed = read(&s, daa);

    // A second claim on each row, beside the two licensed ones.
    daa += 1;
    s = rd(&s, daa - 1);
    let block = ctx(0x5200_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI);
    let second = |class: Hash64, producer: u64, pwu: u64, seed: u64| {
        let (env, key, claim) =
            junk_attempt(class, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), pwu, seed, seed << 16);
        (go(&p, &sp, &s, &block, &[], PalwBlockWorkV3::Attempt(&env), key), claim)
    };
    let (second_2m, second_2m_id) = second(id2m, P2M_SECOND, pwu_2m, 0x52_03);
    let (second_short, second_short_id) = second(short, PSHORT_SECOND, 1_000, 0x52_04);

    // One block past the later licence's challenge window: both licensed claims are Final.
    let last = *licences.last().unwrap();
    daa = last + sp.window_challenge_at(last) + 1;
    s = rd(&s, daa - 1);
    s = go(&p, &sp, &s, &ctx(0x5200_0000 + daa, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("the sweep").0;
    let final_now = read(&s, daa);
    daa += 1;
    s = rd(&s, daa - 1);
    let (env, key, after_final) = junk_attempt(
        id2m,
        bond_key(P2M_SECOND),
        pubkey_of(P2M_SECOND),
        &operator_pubkey_of(P2M_SECOND),
        pwu_2m,
        0x52_05,
        0x52_05 << 16,
    );
    let second_2m_after_final =
        go(&p, &sp, &s, &ctx(0x5200_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key);
    let show = |r: &Result<(PalwChainStateV2, _, _), PalwStateV2Error>, id: &Hash64| {
        r.as_ref().map(|(st, _, _)| st.claim(id).is_some()).map_err(|e| format!("{e:?}"))
    };
    println!("((short owed, room), (2M owed, room)): empty {empty:?}; both licensed {both_licensed:?}; both Final {final_now:?}");
    println!(
        "second 2M beside the licensed one: {:?}; second short: {:?}; 2M after Final: {:?}",
        show(&second_2m, &second_2m_id),
        show(&second_short, &second_short_id),
        show(&second_2m_after_final, &after_final)
    );

    assert_eq!(empty, ((0, 5), (0, 1)), "eight ready seats: five short-row claims, and the 2M row's cap of one");
    assert_eq!(
        both_licensed,
        ((0, 3), (1, 0)),
        "licensed, the short claim is released and the 2M claim is not: it fills its own cap and charges the short row's budget"
    );
    assert!(
        matches!(second_2m, Err(PalwStateV2Error::ClassInflightCapped { class, inflight: 1, cap: 1 }) if class == id2m),
        "a second 2M claim beside the licensed one meets c_2M = 1"
    );
    assert!(second_short.is_ok_and(|(st, _, _)| st.claim(&second_short_id).is_some()), "a second short-row claim is admitted");
    assert_eq!(final_now, empty, "Final releases the 2M slot");
    assert!(second_2m_after_final.is_ok_and(|(st, _, _)| st.claim(&after_final).is_some()), "and a second 2M claim is admitted");
}

/// The short row's empty-panel room is its rate capacity past eight ready seats: its static cap of
/// 5 is not read past the fence. (Holding the row to its cap, as f8c91f19 did, read 5/5/5 here.)
#[test]
fn the_short_rows_empty_panel_room_past_eight_ready_seats_is_its_capacity_not_its_cap() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let now = 1_000u64;
    let globals = registry_fold(&p, now).expect("t12 arms the registry").globals;
    let seat_count = b.panel.seat_count() as u128;
    let extra: Vec<PalwBondKeyV2> = (0..8u64).map(|i| bond_key(9_950 + i)).collect();
    let bonds: Vec<_> = (0..8u64).map(|i| bond_obj(9_950 + i, RICH)).collect();
    let s0 = go(
        &p,
        &sp,
        &activated(&sp, &genesis_state(&p), short),
        &ctx(0x5300_0000, now, now, 0),
        &bonds,
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("eight more bonds")
    .0;
    let mut keys = honest(&p);
    keys.extend(extra);
    let mut rooms = Vec::new();
    for n in [8usize, 12, 16] {
        let s = readied(&sp, &s0, &keys[..n], short, now);
        let read = op186(&p, &sp, &s, short, now);
        assert_eq!(read.ready_seats_now, n as u32);
        let row = s.model_lifecycle(&short).unwrap();
        let per_span = n as u128 * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
        let capacity = palw_panel_capacity_by_rate_v1(
            per_span,
            0,
            row.profile.verification_window_spans as u64,
            row.work.economic_ccu_per_claim * seat_count,
        );
        rooms.push((n, read.panel_room, capacity));
    }
    println!("(ready seats, short room, short capacity): {rooms:?}");
    assert_eq!(rooms, vec![(8, 5, 5), (12, 8, 8), (16, 11, 11)], "the room is the capacity, past the cap of 5");
}
