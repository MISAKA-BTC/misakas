//! **ADR-0152 v3.1 T-2(a), the per-bond share (S-6), on testnet-12's genesis through the real
//! fold** — no bond holds more than `⌈c_class / 2⌉` unlicensed claims of a non-base class, where
//! `c_class` is the class's `max_inflight_claims` for a C7 class and otherwise its capacity by rate
//! with no other class beside it (`palw_panel_capacity_by_rate_v1(per_span, 0, window, cost)`).
//!
//! * T20's S half: at eight ready seats the short-window row's capacity is 5, so its share is 3; the
//!   2M row is C7 (by the window rule and by `palw_rcore_conservative_classes`) with c_2M = 1, so
//!   its share is 1; the floor is ungated; below `palw_rcore_plus` there is no share at all.
//! * T21's share half: one bond's fourth unlicensed short-row claim is SKIPPED at step 4 (the block
//!   stands, the skip is listed, no claim is written) while the class still has room, which another
//!   bond then takes; a licence of one of the bond's claims gives the slot back.
//! * The fence-off twin (testnet-12 with `palw_rcore_plus` unset): the same bond takes the whole
//!   room, and only the room refuses it.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_s6_per_bond_share

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwClaimPhaseV2, PalwStateV2Error, palw_bond_class_share_admits_v1,
    palw_bond_class_share_read_v1, palw_class_admits_claim_v1,
};
use kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1;

const PRODUCER: u64 = 6_001;
const OTHER: u64 = 6_002;

/// testnet-12 with R-core+ unset (the fence-off twin): the fence and C7 cleared and the bundle's
/// mirrors re-synced, as the processor twins do.
fn t12_fence_off() -> Params {
    let mut p = t12();
    p.palw_rcore_plus = None;
    p.palw_rcore_conservative_classes = &[];
    p.sync_palw_rcore_plus();
    p
}

/// `(share, unlicensed)` of `bond` on `class` at `daa`, as the fold reads it.
fn share(p: &Params, s: &PalwChainStateV2, bond: u64, class: Hash64, daa: u64) -> Option<(u64, u32)> {
    palw_bond_class_share_read_v1(s, &bundle(p).state, &room_extras(p, daa), &bond_key(bond), &class, daa)
}

/// The short-window row admitting on eight ready seats, the 2M row beside it, and two rich bonds.
fn setup(p: &Params, daa: u64) -> (PalwChainStateV2, Hash64, Hash64) {
    let sp = bundle(p).state;
    let (short, id2m) = model_classes(p);
    let honest = honest(p);
    let mut s = genesis_state(p);
    for id in [short, id2m] {
        s = activated(&sp, &s, id);
        s = readied(&sp, &s, &honest, id, daa);
    }
    let bonds = [bond_obj(PRODUCER, RICH), bond_obj(OTHER, RICH)];
    s = go(p, &sp, &s, &ctx(0x6000_0000 + daa, daa, daa, 0), &bonds, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    (s, short, id2m)
}

/// One attempt by `producer` on `class` at `daa` through the real fold (the block's own work).
fn attempt(
    p: &Params,
    s: &PalwChainStateV2,
    class: Hash64,
    producer: u64,
    seed: u64,
    daa: u64,
) -> Result<(PalwChainStateV2, Vec<(Hash64, String)>, Hash64), PalwStateV2Error> {
    let sp = bundle(p).state;
    let s = readied(&sp, s, &honest(p), class, daa - 1);
    let (env, key, claim) =
        junk_attempt(class, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, seed, seed << 16);
    go(p, &sp, &s, &ctx(0x6100_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key)
        .map(|(next, _, skips)| (next, skips, claim))
}

/// **T20 (S half): the share is ⌈c_class / 2⌉ — 3 on the short row at eight ready seats, 1 on the
/// 2M row (c_2M = 1), none on the floor; and no share below `palw_rcore_plus`.**
#[test]
fn t20_the_share_is_half_the_class_capacity_and_the_2m_share_is_one() {
    let p = t12();
    let daa = 1_000;
    let (s, short, id2m) = setup(&p, daa);
    let sp = bundle(&p).state;
    assert_eq!(op186(&p, &sp, &s, short, daa).panel_room, 5, "the premise: eight ready seats hold five short-row claims");
    assert!(palw_panel_held_to_final_v1(s.model_lifecycle(&id2m).unwrap()), "the 2M row is C7 by the window rule");
    assert!(sp.rcore_conservative_classes().contains(&id2m), "and by testnet-12's C7 list");
    assert_eq!(s.model_lifecycle(&id2m).unwrap().profile.max_inflight_claims, 1, "c_2M = 1 (T20)");
    assert_eq!(share(&p, &s, PRODUCER, short, daa), Some((3, 0)), "⌈5 / 2⌉");
    assert_eq!(share(&p, &s, PRODUCER, id2m, daa), Some((1, 0)), "⌈1 / 2⌉: a C7 class shares its static cap");
    assert_eq!(share(&p, &s, PRODUCER, bundle(&p).base_class_id, daa), None, "the floor is ungated");

    let off = t12_fence_off();
    let (s_off, _, _) = setup(&off, daa);
    assert!(!bundle(&off).state.rcore_plus_active_at(daa), "the twin's mirror is off");
    for class in [short, id2m] {
        assert_eq!(share(&off, &s_off, PRODUCER, class, daa), None, "no share below the fence");
    }
}

/// **T21 (the share): a bond's fourth unlicensed short-row claim is skipped, never fatal; the room
/// it leaves is another bond's; a licence gives the slot back.**
#[test]
fn t21_a_bond_past_its_share_is_skipped_and_the_room_stays_for_others() {
    let p = t12();
    let sp = bundle(&p).state;
    let mut daa = 1_000;
    let (mut s, short, _) = setup(&p, daa);
    let seat_count = bundle(&p).panel.seat_count() as usize;
    let seats: Vec<_> = genesis_bonds(&p).iter().take(seat_count).map(|(k, o, _)| (*k, *o)).collect();

    let mut mine = Vec::new();
    for i in 0..3u64 {
        daa += 1;
        let (next, skips, claim) = attempt(&p, &s, short, PRODUCER, 0x6100 + i, daa).unwrap_or_else(|e| panic!("claim {i}: {e}"));
        assert!(skips.is_empty(), "claim {i} is inside the share: {skips:?}");
        assert!(next.claim(&claim).is_some());
        s = next;
        mine.push(claim);
    }
    assert_eq!(share(&p, &s, PRODUCER, short, daa), Some((3, 3)), "the bond holds its share");
    assert_eq!(op186(&p, &sp, &s, short, daa).panel_room, 2, "the class still has room");
    // So the class gate alone admits the fourth — which is why the producer's pre-check
    // (`palw_producer_facts_v2`'s `class_admission_refusal`) asks the share too (the S-6 review's
    // finding 1; pinned at the processor by `t12_rcore_s6_producer_share`).
    palw_class_admits_claim_v1(&s, &sp, &room_extras(&p, daa + 1), &short, daa + 1).expect("the class gate alone says: produce");
    palw_bond_class_share_admits_v1(&s, &sp, &room_extras(&p, daa + 1), &bond_key(PRODUCER), &short, daa + 1)
        .expect_err("the producer's pre-check sees the share");
    palw_bond_class_share_admits_v1(&s, &sp, &room_extras(&p, daa + 1), &bond_key(OTHER), &short, daa + 1)
        .expect("another bond's share is its own");

    // The fourth: the block stands, the attempt is skipped by name, no claim is written.
    daa += 1;
    let (next, skips, fourth) = attempt(&p, &s, short, PRODUCER, 0x6103, daa).expect("a share refusal never disqualifies the block");
    assert_eq!(skips.len(), 1, "the own attempt is listed as skipped: {skips:?}");
    assert!(skips[0].1.contains("its share of the class is 3"), "{}", skips[0].1);
    assert!(next.claim(&fourth).is_none(), "the skipped attempt leaves no claim");
    assert_eq!(op186(&p, &sp, &next, short, daa).panel_room, 2, "and takes no room");
    s = next;

    // The room it left is another bond's.
    daa += 1;
    let (next, skips, theirs) = attempt(&p, &s, short, OTHER, 0x6200, daa).expect("another bond");
    assert!(skips.is_empty() && next.claim(&theirs).is_some(), "{skips:?}");
    s = next;

    // A licence of one of the bond's claims gives its slot back.
    daa += 1;
    s = readied(&sp, &s, &honest(&p), short, daa - 1);
    s = bound(&p, &sp, &s, mine[0], &seats, daa);
    daa += 1;
    s = readied(&sp, &s, &honest(&p), short, daa - 1);
    s = licensed(&p, &sp, &s, mine[0], &seats, daa);
    assert!(matches!(s.claim(&mine[0]).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    assert_eq!(share(&p, &s, PRODUCER, short, daa), Some((3, 2)), "a licensed claim is not in the share");
    daa += 1;
    let (next, skips, again) = attempt(&p, &s, short, PRODUCER, 0x6104, daa).expect("inside the share again");
    assert!(skips.is_empty() && next.claim(&again).is_some(), "{skips:?}");
    assert_eq!(share(&p, &next, PRODUCER, short, daa), Some((3, 3)));
}

/// **The fence-off twin: no share — the same bond takes the whole room, and the room alone refuses
/// its sixth claim** (the rule testnet-12 folds with `palw_rcore_plus` unset, and every other network).
#[test]
fn t21_fence_off_twin_the_room_alone_bounds_one_bond() {
    let p = t12_fence_off();
    let mut daa = 1_000;
    let (mut s, short, _) = setup(&p, daa);
    for i in 0..5u64 {
        daa += 1;
        let (next, skips, claim) = attempt(&p, &s, short, PRODUCER, 0x6300 + i, daa).unwrap_or_else(|e| panic!("claim {i}: {e}"));
        assert!(skips.is_empty() && next.claim(&claim).is_some(), "claim {i}: {skips:?}");
        s = next;
    }
    daa += 1;
    let refused = attempt(&p, &s, short, PRODUCER, 0x6305, daa).map(|_| ()).expect_err("the room refuses the sixth");
    assert!(matches!(refused, PalwStateV2Error::PanelRoomExhausted { class, .. } if class == short), "{refused:?}");
}
