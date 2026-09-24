//! **testnet-12's held set is both genesis model rows, and the short-window row is held to Final at
//! its static cap of 5 as the 2M row is at 1** — 2026-09-24 audit #4 review item 1, as ADR-0152 v3
//! settled it (T-2(b), T20: "the held set is exactly {8k, 2M} … both held classes owe every claim
//! until Final and are refused past `max_inflight_claims` (pins the 8k cap value)").
//!
//! f8c91f19 holds a class by `class_is_held_v1` — a class that recorded its own step ladder at
//! registration (ADR-0119). On testnet-12 BOTH genesis model rows did, so the short-window row (the
//! review's "8k" row, window 3) is held too: a licence releases none of its claims, and past the
//! work target it takes no sixth claim until one is Final. 92a9658d recorded that as a defect
//! against the v3 draft's T-2(a); the accepted v3 names it the rule (V3S-09), and C7 — the 2M-only
//! conservative set — is ADR-0152's R-core+ charging set, not a second hold.
//!
//! On testnet-12's genesis through the real fold, the short row admitting on eight ready seats
//! (capacity 5): five claims accepted, bound and licensed one after another, a sixth attempt, and
//! the sweep to Final.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_short_class_is_held_to_final_at_its_cap

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2, PalwStateV2Error};

const PRODUCERS: u64 = 12;

#[test]
fn the_genesis_held_set_is_the_short_row_and_the_2m_row() {
    let p = t12();
    let g = genesis_state(&p);
    let (short, id2m) = model_classes(&p);
    let held: Vec<_> = genesis_classes(&p).iter().map(|c| c.0).filter(|id| g.class_is_held_v1(id)).collect();
    assert_eq!(held.len(), 2, "two held rows at genesis: {held:?}");
    assert!(held.contains(&short) && held.contains(&id2m), "the short row and the 2M row: {held:?}");
    let cap = |id| g.model_lifecycle(&id).expect("a model row").profile.max_inflight_claims;
    assert_eq!((cap(short), cap(id2m)), (5, 1), "their static caps: short 5, 2M 1 (c_2M = 1)");
}

#[test]
fn the_short_row_owes_every_licensed_claim_until_final_and_refuses_a_sixth() {
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
    assert_eq!(op186(&p, &sp, &s, short, daa).panel_room, 5, "eight ready seats hold five short-row claims");

    // Five claims, each accepted, bound and licensed before the next.
    let mut rooms_after_licence = Vec::new();
    let mut last_licence = daa;
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
        last_licence = daa;
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
        vec![(1, 4), (2, 3), (3, 2), (4, 1), (5, 0)],
        "a licence releases nothing of a held class: each claim stays owed and the room shrinks by one"
    );
    assert!(
        matches!(sixth_folds, Err(PalwStateV2Error::ClassInflightCapped { class, inflight: 5, cap: 5 }) if class == short),
        "a sixth attempt beside five licensed claims meets the static cap"
    );

    // One block past the last claim's challenge window: all five are Final, and the room is whole.
    daa = last_licence + sp.window_challenge_at(last_licence) + 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    s = go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("the sweep").0;
    let live = s.claims_iter().filter(|(_, c)| c.class_id == short && !c.phase.is_terminal()).count();
    let final_now = (owed(&p, &s, short), op186(&p, &sp, &s, short, daa).panel_room);
    println!("after the sweep at DAA {daa}: live {live}, (owed, op186 room) {final_now:?}");
    assert_eq!((live, final_now), (0, (0, 5)), "Final releases every slot");
    daa += 1;
    s = readied(&sp, &s, &honest, short, daa - 1);
    let (after, _, _) =
        go(&p, &sp, &s, &ctx(0x5000_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .expect("the sixth attempt is admitted once the five are Final");
    assert!(after.claim(&sixth).is_some());
}
