//! **testnet-12's 2M row holds one claim at eight ready seats with the short-window row admitting,
//! and both genesis rows' empty-panel rooms are pinned** — 2026-09-24 audit #4 review item 1.
//!
//! At e93be0f2, past the work target (armed at DAA 0 on testnet-12, the registry governing from
//! DAA 30), the class gate read the rate room and never `max_inflight_claims`: eight ready seats'
//! replay holds two 2M claims (1.68e16 a claim over a 2,799-span window at 700 ‰), so two junk
//! 2M claims folded while the short-window row admitted beside them, and only the third met
//! `PanelRoomExhausted`. The parent gave the 2M row a room of 0 there. The 2M row is held (ADR-0152
//! T-2(b), c_2M = 1): the second claim is now refused at the cap.
//!
//! What the review also measured and still holds: the 2M window fits its receipt deadline
//! (`window × span_daa ≤ receipt_window_for_claim_v1`), so past the fence nothing holds the row,
//! and an `Active` 2M row with eight ready seats stays `Active` across span boundaries.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_2m_cap_holds_beside_the_short_class

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_model_registry_v1::PalwModelLifecycleV1;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwStateV2Error};
use kaspa_consensus_core::palw_work_target_v1::{palw_panel_capacity_by_rate_v1, palw_panel_held_to_final_v1};

#[test]
fn at_eight_ready_seats_with_the_short_class_admitting_a_second_2m_claim_is_refused() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let (short, id2m) = model_classes(&p);
    let now = 1_000u64;
    let span = registry_fold(&p, now).unwrap().span_daa;
    let row = g.model_lifecycle(&id2m).unwrap().clone();
    let w = row.profile.verification_window_spans as u64;
    let receipt_window = sp.receipt_window_for_claim_v1(&g, &id2m, now);
    println!("2M: window {w} spans × span {span} DAA = {} DAA; receipt window {receipt_window}", w * span);
    assert!(p.palw_audit_2026_09_23_active_at(now) && p.palw_work_target_at(0), "t12 arms the fence and the work target");
    assert!(w * span <= receipt_window, "the 2M window fits its receipt deadline, so nothing past the fence holds the row");

    // Both rows admitting, the eight genesis seats ready for both.
    let mut s0 = g.clone();
    for id in [short, id2m] {
        s0 = readied(&sp, &activated(&sp, &s0, id), &honest(&p), id, now);
    }
    let pwu = class_pwu(&p, &g, id2m, now);
    let (s1, _, _) = go(
        &p,
        &sp,
        &s0,
        &ctx(0x9100_0001, now, 1, 0),
        &[bond_obj(9_201, RICH), bond_obj(9_202, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("bonds");
    let rooms_empty = (op186(&p, &sp, &s1, short, now).panel_room, op186(&p, &sp, &s1, id2m, now).panel_room);
    let (e1, k1, id1) = junk_attempt(id2m, bond_key(9_201), pubkey_of(9_201), &operator_pubkey_of(9_201), pwu, 0x91_01, 0x91_0000_01);
    let (s2, _, sk) = go(&p, &sp, &s1, &ctx(0x9100_0002, now + 1, 2, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&e1), k1)
        .expect("2M #1");
    assert!(sk.is_empty(), "{sk:?}");
    assert!(s2.claim(&id1).is_some(), "the first 2M claim is live");
    let (e2, k2, _) = junk_attempt(id2m, bond_key(9_202), pubkey_of(9_202), &operator_pubkey_of(9_202), pwu, 0x91_02, 0x91_0000_02);
    let second = go(&p, &sp, &s2, &ctx(0x9100_0003, now + 2, 3, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&e2), k2);
    let rooms_one = (op186(&p, &sp, &s2, short, now + 1).panel_room, op186(&p, &sp, &s2, id2m, now + 1).panel_room);
    println!("rooms (short, 2M): empty {rooms_empty:?}, one 2M claim live {rooms_one:?}");
    println!("second 2M attempt: {:?}", second.as_ref().map(|_| "admitted").map_err(|e| format!("{e:?}")));

    assert_eq!(rooms_empty.1, 1, "eight ready seats: the 2M room is its cap, 1, where the rate alone would say 2");
    assert_eq!(rooms_one.1, 0, "one 2M claim live: no 2M room");
    assert!(rooms_one.0 > 0, "the short-window row is still admitting beside it");
    assert!(
        matches!(second, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "the second 2M claim is refused at the cap: {:?}",
        second.as_ref().map(|_| ())
    );

    // Walk past the next span boundaries so the lifecycle step runs: the row stays Active.
    let mut s = s2;
    let mut daa = now + 2;
    let boundary = ((now / span) + 3) * span + 1;
    let mut blk = 0x9100_0100u64;
    while daa <= boundary {
        s = go(&p, &sp, &s, &ctx(blk, daa, blk - 0x9100_0000, 0), &[], PalwBlockWorkV3::None, Hash64::default())
            .expect("an empty block")
            .0;
        daa += 1;
        blk += 1;
    }
    let r = s.model_lifecycle(&id2m).unwrap();
    println!("after the boundary at DAA {}: 2M {:?}, inflight {}, ready {}", daa - 1, r.state, r.inflight_claims, r.ready_seats);
    assert_eq!(r.state, PalwModelLifecycleV1::Active, "an Active 2M row with eight ready seats stays Active");
    assert!(gate(&p, &sp, &s, id2m, daa).is_err(), "and its one claim still fills its room");
}

#[test]
fn the_genesis_rows_empty_panel_rooms_are_pinned() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let g = genesis_state(&p);
    let (short, id2m) = model_classes(&p);
    let now = 1_000u64;
    let fold = registry_fold(&p, now).unwrap();
    let globals = fold.globals;
    let seat_count = b.panel.seat_count() as u128;
    for (id, window, cap, held) in [(short, 3u32, 5u32, false), (id2m, 2_799, 1, true)] {
        let row = g.model_lifecycle(&id).unwrap();
        assert_eq!(row.state, PalwModelLifecycleV1::Prefetching, "the genesis rows open Prefetching");
        assert!(g.class_is_held_v1(&id), "both genesis model rows record ADR-0119's held ladder");
        assert_eq!(palw_panel_held_to_final_v1(row), held, "only the 2M row is held to Final (ADR-0152's C7)");
        assert_eq!((row.profile.verification_window_spans, row.profile.max_inflight_claims), (window, cap));
        assert_eq!(row.profile.required_ready_seats, 7);
    }
    // As genesis leaves them: no seat ready, no room.
    assert_eq!((op186(&p, &sp, &g, short, now).panel_room, op186(&p, &sp, &g, id2m, now).panel_room), (0, 0));

    let capacity = |s: &kaspa_consensus_core::palw_state_v2::PalwChainStateV2, id: Hash64, ready: u128| {
        let row = s.model_lifecycle(&id).unwrap();
        let per_span = ready * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
        palw_panel_capacity_by_rate_v1(
            per_span,
            0,
            row.profile.verification_window_spans as u64,
            row.work.economic_ccu_per_claim * seat_count,
        )
    };
    let mut rooms = Vec::new();
    for n in [5usize, 7, 8] {
        let mut s = g.clone();
        for id in [short, id2m] {
            s = readied(&sp, &activated(&sp, &s, id), &honest(&p)[..n], id, now);
        }
        let read = (op186(&p, &sp, &s, short, now), op186(&p, &sp, &s, id2m, now));
        assert_eq!((read.0.ready_seats_now, read.1.ready_seats_now), (n as u32, n as u32));
        rooms.push((n, read.0.panel_room, read.1.panel_room, capacity(&s, short, n as u128), capacity(&s, id2m, n as u128)));
    }
    println!("(ready, short room, 2M room, short capacity, 2M capacity): {rooms:?}");
    assert_eq!(
        rooms,
        vec![(5, 3, 1, 3, 1), (7, 5, 1, 5, 1), (8, 5, 1, 5, 2)],
        "both rows admitting on an empty panel: the short row's room is its capacity (its cap of 5 is not read), the 2M \
         row's is its cap of 1 wherever its capacity is more"
    );
}
