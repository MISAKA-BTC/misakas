//! **A held class's licensed claim stays on EVERY class's budget until Final, not only on its own
//! cap** — 2026-09-24 audit #4 review item 1 (ADR-0152 T-2(b)), the consequence its verification of
//! f8c91f19 asked to have recorded as a decision.
//!
//! `palw_panel_owed_v1` makes a held class owe its whole tally, and each class's owed row is also
//! what every OTHER class's capacity is read beside. So a licensed 2M claim keeps charging the
//! short-window row's room through its licence-to-Final window, where T-2(a)'s licence release
//! would have given the short row its room back at the licence. That is the hold's premise, kept
//! deliberately: the panel is not known to have finished a held claim's replay at its licence, and
//! the seats that owe it are the seats every class's budget is counted on.
//!
//! On testnet-12's genesis through the real fold: both model rows admitting on all eight ready
//! seats, one 2M attempt accepted, bound, licensed and swept to Final, and at each step
//! `(short room, 2M room)` as op 186 prints them; beside it the licensed state with only the 2M
//! row's ladder removed (not held), where the licence does release.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_held_claim_is_charged_to_every_class_until_final

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2};

const PRODUCER: u64 = 9_301;

#[test]
fn a_licensed_2m_claim_keeps_charging_the_short_rows_room_until_final() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, id2m) = model_classes(&p);
    let honest = honest(&p);
    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let rd = |s: &PalwChainStateV2, daa: u64| {
        let mut s = s.clone();
        for id in [short, id2m] {
            s = readied(&sp, &s, &honest, id, daa);
        }
        s
    };
    let rooms = |s: &PalwChainStateV2, daa: u64| (op186(&p, &sp, s, short, daa).panel_room, op186(&p, &sp, s, id2m, daa).panel_room);

    let mut daa = 1_000u64;
    let mut s = genesis_state(&p);
    for id in [short, id2m] {
        s = activated(&sp, &s, id);
    }
    s = rd(&s, daa);
    s = go(&p, &sp, &s, &ctx(0xA200_0000, daa, daa, 0), &[bond_obj(PRODUCER, RICH)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the producer bonds")
        .0;
    let empty = rooms(&s, daa);

    daa += 1;
    s = rd(&s, daa - 1);
    let pwu = class_pwu(&p, &s, id2m, daa);
    let (env, key, claim) =
        junk_attempt(id2m, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), pwu, 0xA201, 0xA201_0000);
    s = go(&p, &sp, &s, &ctx(0xA200_0001, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .expect("the 2M attempt")
        .0;
    let provisional = rooms(&s, daa);
    daa += 1;
    s = rd(&s, daa - 1);
    s = bound(&p, &sp, &s, claim, &seats, daa);
    daa += 1;
    s = rd(&s, daa - 1);
    s = licensed(&p, &sp, &s, claim, &seats, daa);
    let licensed_daa = daa;
    assert!(matches!(s.claim(&claim).expect("the claim").phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let licensed_rooms = rooms(&s, daa);
    // The same state with the 2M row not held (only its ladder removed): the licence frees its replay.
    let released = edited(&sp, &s, |c| {
        c.class_step_ladders.remove(&id2m);
    });
    let released_rooms = rooms(&released, daa);

    // One block past the challenge window: the claim is Final and charges no class.
    let challenge = sp.window_challenge_at(licensed_daa);
    daa = licensed_daa + challenge + 1;
    s = rd(&s, daa - 1);
    s = go(&p, &sp, &s, &ctx(0xA200_0004, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("the sweep").0;
    assert!(matches!(s.claim(&claim).expect("the claim").phase, PalwClaimPhaseV2::Final { .. }));
    let final_rooms = rooms(&s, daa);
    println!(
        "(short room, 2M room): empty {empty:?}; 2M provisional {provisional:?}; 2M licensed {licensed_rooms:?}; \
         licensed with the 2M row not held {released_rooms:?}; Final {final_rooms:?}; licence-to-Final window {challenge} DAA"
    );

    assert_eq!(empty, (5, 1), "eight ready seats: five short-row claims, and the 2M row's cap of one");
    assert_eq!(provisional, (3, 0), "an accepted 2M claim takes two short-row claims' room and fills its own");
    assert_eq!(licensed_rooms, provisional, "licensed, it charges both rows exactly as before: held to Final");
    assert_eq!(released_rooms, (5, 2), "a class not held would have released both at the licence");
    assert_eq!(final_rooms, empty, "Final releases both");
}
