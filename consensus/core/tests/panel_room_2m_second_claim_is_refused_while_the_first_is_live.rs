//! **A second 2M attempt is refused while the first 2M claim is live — bound or licensed, at
//! seven ready seats and at eight** — 2026-09-24 audit #4 review item 1 (ADR-0152 T-2(b)).
//!
//! At e93be0f2 the fold's gate admitted the second 2M claim twice over: at seven ready seats once
//! the first was licensed (the licence released its replay), and at eight ready seats while the
//! first was only bound (past the work target the rate room replaced `max_inflight_claims`, and
//! eight seats' replay holds two 2M claims). Both through the real fold on testnet-12's genesis,
//! the 2M row admitting; the block carrying the second attempt is refused whole.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_2m_second_claim_is_refused_while_the_first_is_live

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_economy_v1::{palw_panel_seat_exposure_v1, palw_seat_has_headroom_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBlockWorkV3, PalwBondKeyV2, PalwClaimPhaseV2, PalwStateV2Error};

const A1: u64 = 9_201;
const A2: u64 = 9_202;

#[test]
fn at_seven_ready_seats_the_second_2m_claim_is_refused_bound_and_licensed() {
    scenario(7);
}

#[test]
fn at_eight_ready_seats_the_second_2m_claim_is_refused_bound_and_licensed() {
    scenario(8);
}

fn scenario(n_ready: usize) {
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
    let b = bundle(&p);
    let sp = b.state.clone();
    let economy = p.palw_seat_economy_at(0).expect("t12 arms the panel economy");
    let seat_count = b.panel.seat_count() as usize;
    let fold0 = registry_fold(&p, 2_000).expect("t12 arms the registry");
    let d0 = fold0.grace_until_daa.max(1_000) + 10;
    let (_, id2m) = model_classes(&p);
    let ready: Vec<PalwBondKeyV2> = honest(&p).into_iter().take(n_ready).collect();

    let g = readied(&sp, &activated(&sp, &genesis_state(&p), id2m), &ready, id2m, d0);
    let pwu = class_pwu(&p, &g, id2m, d0);
    let per = admitted_per_attempt_on(&p, &g, id2m, pwu, T12_BLOCK_SUBSIDY_SOMPI);
    let seat_2m = palw_panel_seat_exposure_v1(per.reserved, per.escrow as u64, seat_count, economy.reward_multiple_permille);
    let coll = at_least_the_floor(&p, per.collateral);
    let mut s =
        go(&p, &sp, &g, &ctx(1, d0, d0, 0), &[bond_obj(A1, coll), bond_obj(A2, coll)], PalwBlockWorkV3::None, Hash64::default())
            .expect("the producers bond")
            .0;
    let empty_room = op186(&p, &sp, &s, id2m, d0).panel_room;

    // The first 2M attempt, admitted.
    let (env1, key1, id1) = junk_attempt(id2m, bond_key(A1), pubkey_of(A1), &operator_pubkey_of(A1), pwu, 0x8D01, 0x8D01_0000);
    s = readied(&sp, &s, &ready, id2m, d0);
    let (next, _, skips) =
        go(&p, &sp, &s, &ctx(0x8D_0001, d0 + 1, d0 + 1, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env1), key1)
            .expect("the first 2M attempt is admitted");
    assert!(skips.is_empty(), "{skips:?}");
    s = next;

    // Bound on honest seats with 2M headroom.
    let mut order: Vec<(PalwBondKeyV2, Hash64, u64)> = genesis_bonds(&p)
        .into_iter()
        .filter(|(k, _, _)| {
            palw_seat_has_headroom_v1(
                s.bond(k).unwrap().collateral,
                s.reserved_exposure(k) + s.registration_exposure(k),
                seat_2m,
                economy.max_exposure_ratio_permille,
            )
        })
        .collect();
    order.sort_by_key(|(k, _, _)| s.reserved_exposure(k));
    let seats: Vec<(PalwBondKeyV2, Hash64)> = order.iter().take(seat_count).map(|(k, o, _)| (*k, *o)).collect();
    assert_eq!(seats.len(), seat_count);
    s = readied(&sp, &s, &ready, id2m, d0 + 1);
    s = bound(&p, &sp, &s, id1, &seats, d0 + 2);

    // The second 2M attempt while the first is bound.
    let (env2, key2, id2) = junk_attempt(id2m, bond_key(A2), pubkey_of(A2), &operator_pubkey_of(A2), pwu, 0x8D02, 0x8D02_0000);
    s = readied(&sp, &s, &ready, id2m, d0 + 2);
    let while_bound =
        go(&p, &sp, &s, &ctx(0x8D_0010, d0 + 3, d0 + 3, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env2), key2);
    let room_bound = op186(&p, &sp, &s, id2m, d0 + 3).panel_room;

    // Licence the first; it is live, not Final.
    s = licensed(&p, &sp, &s, id1, &seats, d0 + 3);
    let phase1 = s.claim(&id1).unwrap().phase.clone();
    assert!(matches!(phase1, PalwClaimPhaseV2::ReceiptLicensed { .. }), "{phase1:?}");

    // The second 2M attempt while the first is licensed.
    s = readied(&sp, &s, &ready, id2m, d0 + 3);
    let while_licensed =
        go(&p, &sp, &s, &ctx(0x8D_0004, d0 + 4, d0 + 4, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env2), key2);
    let room_licensed = op186(&p, &sp, &s, id2m, d0 + 4).panel_room;
    let summary = |r: &Result<_, PalwStateV2Error>| match r {
        Ok((st, _, _)) => {
            format!("admitted (claim present {})", kaspa_consensus_core::palw_state_v2::PalwChainStateV2::claim(st, &id2).is_some())
        }
        Err(e) => format!("refused: {e:?}"),
    };
    println!(
        "[{n_ready} ready] empty room {empty_room}; while #1 bound: room {room_bound}, #2 {}; while #1 licensed: room {room_licensed}, #2 {}",
        summary(&while_bound),
        summary(&while_licensed)
    );

    assert_eq!(empty_room, 1, "{n_ready} ready seats: the 2M room on an empty panel is its cap, 1");
    for (when, result, room) in [("bound", &while_bound, room_bound), ("licensed", &while_licensed, room_licensed)] {
        assert_eq!(room, 0, "{n_ready} ready seats, #1 {when}: op 186 shows no 2M room");
        assert!(
            matches!(result, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
            "{n_ready} ready seats, #1 {when}: the block carrying a second 2M attempt is refused at the 2M cap: {}",
            summary(result)
        );
    }
}
