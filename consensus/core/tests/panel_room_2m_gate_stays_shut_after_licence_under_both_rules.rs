//! **The 2M gate stays shut after the first 2M claim's licence, past the fence as below it, so no
//! second 2M claim is live before the first is Final** — 2026-09-24 audit #4 review item 1.
//!
//! At e93be0f2 the gate (`palw_class_admits_claim_v1`, the question the producer's pre-check and
//! the fold both ask) refused a second 2M claim while the first was bound and ADMITTED it once
//! the first was licensed, past `palw_audit_2026_09_23` only — below the fence it stayed refused —
//! and the second attempt folded, leaving two 2M claims live. The 2M row admitting on exactly its
//! seven required ready seats (room 1 on an empty panel), on testnet-12's genesis, real fold.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_2m_gate_stays_shut_after_licence_under_both_rules

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwClaimPhaseV2, PalwStateV2Error, palw_class_admits_claim_v1};
use kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1;

const NOW: u64 = 1_000;
const A1: u64 = 8_801;
const A2: u64 = 8_802;

#[test]
fn the_2m_gate_refuses_a_second_claim_after_the_first_is_licensed_past_and_below_the_fence() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (_, id2m) = model_classes(&p);
    let g = genesis_state(&p);
    let row = g.model_lifecycle(&id2m).unwrap().profile.clone();
    let required = row.required_ready_seats as usize;
    assert!(palw_panel_held_to_final_v1(g.model_lifecycle(&id2m).unwrap()), "the 2M row is held to Final (ADR-0152's C7)");
    assert_eq!(row.max_inflight_claims, 1, "c_2M = 1");
    let ready: Vec<_> = honest(&p).into_iter().take(required).collect();
    let s0 = readied(&sp, &activated(&sp, &g, id2m), &ready, id2m, NOW);
    let rate = room_extras(&p, NOW);
    assert!(rate.audit_2026_09_23_active && rate.work_target_active && rate.model_registry.as_ref().unwrap().governs_at(NOW));

    let pwu = class_pwu(&p, &g, id2m, NOW);
    let (s1, _, _) = go(
        &p,
        &sp,
        &s0,
        &ctx(0x8800_0001, NOW, NOW, 0),
        &[bond_obj(A1, RICH), bond_obj(A2, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
    )
    .expect("bonds");
    let gate_empty = palw_class_admits_claim_v1(&s1, &sp, &rate, &id2m, NOW);

    let (env1, key1, id1) = junk_attempt(id2m, bond_key(A1), pubkey_of(A1), &operator_pubkey_of(A1), pwu, 0x88_01, 0x88_0000_01);
    let (s2, _, skips) =
        go(&p, &sp, &s1, &ctx(0x8800_0002, NOW + 1, NOW + 1, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env1), key1)
            .expect("2M #1");
    assert!(skips.is_empty(), "{skips:?}");

    let seats = honest_seats(&p, b.panel.seat_count() as usize);
    let s3 = bound(&p, &sp, &s2, id1, &seats, NOW + 2);
    let gate_bound = gate(&p, &sp, &s3, id2m, NOW + 3);

    let s4 = licensed(&p, &sp, &s3, id1, &seats, NOW + 3);
    let ph = s4.claim(&id1).unwrap().phase.clone();
    assert!(matches!(ph, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{ph:?}");
    let gate_licensed = gate(&p, &sp, &s4, id2m, NOW + 4);
    let gate_licensed_below = palw_class_admits_claim_v1(&s4, &sp, &dormant_extras(&p, NOW + 4), &id2m, NOW + 4);

    let (env2, key2, id2) = junk_attempt(id2m, bond_key(A2), pubkey_of(A2), &operator_pubkey_of(A2), pwu, 0x88_02, 0x88_0000_02);
    let second =
        go(&p, &sp, &s4, &ctx(0x8800_0006, NOW + 4, NOW + 4, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env2), key2);
    let live_2m = s4.claims_iter().filter(|(_, c)| c.class_id == id2m && !c.phase.is_terminal()).count();
    println!("gate: empty {gate_empty:?}; #1 bound {gate_bound:?}");
    println!("#1 licensed (live): past the fence {gate_licensed:?}; below the fence {gate_licensed_below:?}");
    println!(
        "#2 folded while #1 licensed: {:?}; live 2M claims {live_2m}",
        second.as_ref().map(|(st, _, _)| st.claim(&id2).map(|c| c.phase.clone()))
    );

    assert!(gate_empty.is_ok(), "an empty panel admits the first 2M claim");
    assert!(gate_bound.is_err(), "with #1 bound, the 2M gate is shut");
    assert!(
        matches!(gate_licensed, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "with #1 licensed and live, the 2M gate is STILL shut past the fence: {gate_licensed:?}"
    );
    assert!(gate_licensed_below.is_err(), "below the fence it is shut as it always was (parity)");
    assert!(
        matches!(second, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "the block carrying the second 2M attempt is refused at the 2M cap: {:?}",
        second.as_ref().map(|_| ())
    );
    assert_eq!(live_2m, 1, "one 2M claim live before Final");
}
