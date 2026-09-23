//! **A licensed 2M claim stays owed, and the 2M room stays 0, until the claim is Final** —
//! 2026-09-24 audit #4 review item 1 (ADR-0152 T-2(b): c_2M = 1, its slot held to Final).
//!
//! At e93be0f2 the licence released every class's replay: a bound 2M claim was charged, and its
//! `ReceiptLicensed` took the charge back while the claim was still live, so the 2M room came back
//! a whole challenge window before Final. This folds one 2M attempt on testnet-12's own genesis
//! through the real fold — accepted, bound, licensed, swept to `Final` — with the 2M row admitting
//! on exactly its required seven ready seats, and reads at each step what the rate rule owes for
//! the class (`palw_panel_demand_read_v1`, the read op 186 and the gate share), op 186's room, and
//! the gate a producer asks.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_2m_licence_keeps_the_claim_owed_until_final

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2, PalwStateV2Error};

const PRODUCER: u64 = 9_101;

#[test]
fn a_licensed_2m_claim_keeps_the_2m_room_at_zero_until_final() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (_, id2m) = model_classes(&p);
    let required = genesis_state(&p).model_lifecycle(&id2m).expect("the 2M row").profile.required_ready_seats as usize;
    assert_eq!(required, 7, "testnet-12's 2M row needs seven ready seats");
    let ready: Vec<_> = honest(&p).into_iter().take(required).collect();
    let seats = honest_seats(&p, b.panel.seat_count() as usize);

    let mut daa = 1_000u64;
    let mut s = readied(&sp, &activated(&sp, &genesis_state(&p), id2m), &ready, id2m, daa);
    s = go(&p, &sp, &s, &ctx(0x9101_0000, daa, daa, 0), &[bond_obj(PRODUCER, RICH)], PalwBlockWorkV3::None, Hash64::default())
        .expect("the producer bonds")
        .0;
    assert_eq!(owed(&p, &s, id2m), 0);
    assert_eq!(op186(&p, &sp, &s, id2m, daa).panel_room, 1, "seven ready seats hold one 2M claim");

    daa += 1;
    s = readied(&sp, &s, &ready, id2m, daa - 1);
    let pwu = class_pwu(&p, &s, id2m, daa);
    let (env, key, claim) =
        junk_attempt(id2m, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), pwu, 0x7D01, 0x7D01_0000);
    let (next, _, skips) =
        go(&p, &sp, &s, &ctx(0x9101_0001, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
            .expect("the 2M attempt");
    assert!(skips.is_empty(), "{skips:?}");
    s = next;
    let provisional = (owed(&p, &s, id2m), op186(&p, &sp, &s, id2m, daa).panel_room);

    daa += 1;
    s = readied(&sp, &s, &ready, id2m, daa - 1);
    s = bound(&p, &sp, &s, claim, &seats, daa);
    let bound_now = (owed(&p, &s, id2m), op186(&p, &sp, &s, id2m, daa).panel_room);

    daa += 1;
    s = readied(&sp, &s, &ready, id2m, daa - 1);
    s = licensed(&p, &sp, &s, claim, &seats, daa);
    let licensed_daa = daa;
    let phase = s.claim(&claim).expect("the claim").phase.clone();
    assert!(matches!(phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "licensed: {phase:?}");
    assert!(!phase.is_terminal(), "a licensed claim is still live");
    let licensed_now = (owed(&p, &s, id2m), op186(&p, &sp, &s, id2m, daa).panel_room);
    let gate_licensed = gate(&p, &sp, &s, id2m, daa + 1);

    // One block past the challenge window: the claim is Final and owes nothing.
    daa = licensed_daa + sp.window_challenge_at(licensed_daa) + 1;
    s = readied(&sp, &s, &ready, id2m, daa - 1);
    s = go(&p, &sp, &s, &ctx(0x9101_0004, daa, daa, 0), &[], PalwBlockWorkV3::None, Hash64::default()).expect("the sweep").0;
    let final_phase = s.claim(&claim).expect("the claim").phase.clone();
    let final_now = (owed(&p, &s, id2m), op186(&p, &sp, &s, id2m, daa).panel_room);
    let gate_final = gate(&p, &sp, &s, id2m, daa + 1);
    println!(
        "2M (owed, op186 room): provisional {provisional:?}, bound {bound_now:?}, licensed {licensed_now:?}, Final {final_now:?}"
    );
    println!("gate licensed {gate_licensed:?}; gate Final {gate_final:?}; phase at DAA {daa}: {final_phase:?}");

    assert_eq!(provisional, (1, 0), "an accepted 2M claim owes one claim and fills the room");
    assert_eq!(bound_now, (1, 0), "bound, it still does");
    assert_eq!(licensed_now, (1, 0), "licensed but not Final, it STILL does: the 2M class is held to Final");
    assert!(
        matches!(
            gate_licensed,
            Err(PalwStateV2Error::ClassInflightCapped { cap: 1, .. } | PalwStateV2Error::PanelRoomExhausted { .. })
        ),
        "the gate refuses a second 2M claim while the first is licensed: {gate_licensed:?}"
    );
    assert!(matches!(final_phase, PalwClaimPhaseV2::Final { .. }), "swept to Final: {final_phase:?}");
    assert_eq!(final_now, (0, 1), "Final releases the slot");
    assert!(gate_final.is_ok(), "and the gate admits the next 2M claim: {gate_final:?}");
}
