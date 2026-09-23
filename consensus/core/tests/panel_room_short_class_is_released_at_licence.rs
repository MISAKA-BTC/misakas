//! **testnet-12's short-window row is released at licence and judged by the room, not held to
//! Final at its static cap** — ADR-0152 T-2(a), as the 2026-09-24 audit #4 review item 1 left it:
//! the licence releases "8k (and future model classes)"; only 2M is held to Final at c_2M = 1
//! (T-2(b)).
//!
//! f8c91f19 implements the 2M hold with `class_is_held_v1` — "a class under the held regime", one
//! that recorded its own step ladder at registration (ADR-0119: a held profile, recorded by
//! `ClassRegistered`). On testnet-12 BOTH genesis model rows did, so the short-window row — the
//! review's "8k" row, window 3, `max_inflight_claims` 5 — is held too: a licence releases
//! none of its claims, and past the work target it is capped at 5 live claims until Final. That is
//! the drain the licence release exists to remove (Final comes a whole challenge window after the
//! licence), and ADR-0137 D5 put the network's replay budget in place of that per-class cap.
//!
//! On testnet-12's genesis through the real fold, the short row admitting on eight ready seats
//! (capacity 5): five claims accepted, bound and licensed one after another, then a sixth attempt.
//! Ignored while it records the defect; it passes once the hold reaches 2M alone.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_short_class_is_released_at_licence -- --ignored

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2};

const PRODUCERS: u64 = 12;

#[test]
#[ignore = "REAL DEFECT in f8c91f19: `class_is_held_v1` is true for testnet-12's short-window row too, so it is held to Final \
            and capped at 5 (ADR-0152 T-2(a) releases it at licence)"]
fn the_short_row_is_released_at_licence_and_a_sixth_claim_is_admitted_beside_five_licensed() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let seat_count = b.panel.seat_count() as usize;
    let row = genesis_state(&p).model_lifecycle(&short).unwrap().profile.clone();
    println!(
        "short row: held regime {}, window {}, max_inflight_claims {}",
        genesis_state(&p).class_is_held_v1(&short),
        row.verification_window_spans,
        row.max_inflight_claims
    );

    let mut daa = 1_000u64;
    let mut s = readied(&sp, &activated(&sp, &genesis_state(&p), short), &honest, short, daa);
    let bonds: Vec<_> = (1..=PRODUCERS).map(|n| bond_obj(n, RICH)).collect();
    s = go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, 0), &bonds, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    let empty_room = op186(&p, &sp, &s, short, daa).panel_room;
    assert_eq!(empty_room, 5, "eight ready seats hold five short-row claims");

    // Five claims, each accepted, bound and licensed before the next.
    let mut rooms_after_licence = Vec::new();
    for i in 0..row.max_inflight_claims as u64 {
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
        rooms_after_licence.push((owed(&p, &s, short), op186(&p, &sp, &s, short, daa).panel_room));
    }
    let live = s.claims_iter().filter(|(_, c)| c.class_id == short && !c.phase.is_terminal()).count();

    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    let (env, key, sixth) = junk_attempt(short, bond_key(6), pubkey_of(6), &operator_pubkey_of(6), 1_000, 0x56, 0x5_0006);
    let sixth_folds =
        go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key);
    println!("(owed, op186 room) after each licence: {rooms_after_licence:?}; live licensed short claims {live}");
    println!(
        "a sixth short-row attempt: {:?}",
        sixth_folds.as_ref().map(|(st, _, _)| st.claim(&sixth).is_some()).map_err(|e| format!("{e:?}"))
    );

    assert_eq!(live, 5, "five licensed short-row claims wait out their challenge window");
    assert_eq!(
        rooms_after_licence,
        vec![(0, 5); 5],
        "each licence releases its claim's replay (T-2(a)): nothing owed, the room whole"
    );
    let (after, _, _) =
        sixth_folds.expect("a sixth attempt beside five licensed claims is admitted: the budget, not a cap, judges it");
    assert!(after.claim(&sixth).is_some());
}
