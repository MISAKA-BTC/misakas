//! **ADR-0152 SW-9 (M4), T91 through the fold, op 186 and the fold's gate, on testnet-12's rows.**
//!
//! Past `palw_rcore_plus` the rate room of a class OUTSIDE C7 counts effective ready operators
//! (`ready_eff = min(ready operators, max(seat_count, ⌊ΣW / W_max⌋))` over SW-2's capped weights)
//! instead of ready bonds: under the stake-weighted draw the replay lands on the heaviest operators,
//! so that is the capacity the class actually has. It feeds `panel_rate_v1` / `panel_room_v1` (the
//! fold's gate) and op 186's `panel_room` alike, through one decision
//! (`palw_panel_room_ready_eff_terms_v1`) and one count (`palw_panel_room_ready_count_v1`).
//!
//! The fixture is `panel_room_common`'s: testnet-12 itself, its genesis fold, its short-window row
//! (the review's "8k" row, window 3, room-governed since post-edit 5) and its 2M row (C7: window
//! ≥ 1,000 spans, held to its static cap of one). Every reading is taken twice — on the bundle as
//! shipped (the mirror armed) and on its fence-off twin (`with_rcore_plus_mirrors(None, …)`), which
//! must count bonds exactly as before.
//!
//! Run: cargo test -p kaspa-consensus-core --test adr0152_sw9_ready_eff_room

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_panel_v2::PalwPanelStakeDrawV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwChainStateV2, PalwStateParamsV2, PalwStateV2Error, palw_panel_room_ready_count_v1,
    palw_panel_room_ready_eff_terms_v1,
};
use kaspa_consensus_core::palw_work_target_v1::{palw_panel_capacity_by_rate_v1, palw_panel_held_to_final_v1};

const MSK: u64 = 100_000_000;
const NOW: u64 = 1_000;

/// Forty floor-sized operators (130,000 MSK), one 20M operator, and twelve RICH producers that are
/// never readied (they only carry the attempts that fill the room).
const SMALL: std::ops::Range<u64> = 9_800..9_840;
const WHALE: u64 = 9_900;
const PRODUCERS: std::ops::Range<u64> = 9_700..9_712;

struct Fixture {
    p: kaspa_consensus_core::config::params::Params,
    armed: PalwStateParamsV2,
    off: PalwStateParamsV2,
    short: Hash64,
    id2m: Hash64,
    /// Every bond registered, nobody ready yet (the registration block's lifecycle step then holds
    /// both model rows; [`Fixture::ready`] makes them `Active` again).
    registered: PalwChainStateV2,
}

fn fixture() -> Fixture {
    let p = t12();
    let b = bundle(&p);
    let armed = b.state.clone();
    assert!(armed.rcore_plus_active_at(NOW), "testnet-12's bundle mirrors palw_rcore_plus");
    let off = armed.clone().with_rcore_plus_mirrors(None, 0, Vec::new());
    assert!(!off.rcore_plus_active_at(NOW));
    let (short, id2m) = model_classes(&p);
    let mut objs: Vec<_> = SMALL.map(|n| bond_obj(n, 130_000 * MSK)).collect();
    objs.push(bond_obj(WHALE, 20_000_000 * MSK));
    objs.extend(PRODUCERS.map(|n| bond_obj(n, RICH)));
    let s = activated(&armed, &activated(&armed, &genesis_state(&p), short), id2m);
    let registered = go(&p, &armed, &s, &ctx(0x9100_0000, NOW, NOW, 0), &objs, PalwBlockWorkV3::None, Hash64::default())
        .expect("the bonds register")
        .0;
    Fixture { p, armed, off, short, id2m, registered }
}

impl Fixture {
    /// `keys` proved ready for both model rows at `NOW`, and both rows `Active` again (the
    /// registration block's lifecycle step saw no ready seat and held them).
    fn ready(&self, keys: &[PalwBondKeyV2]) -> PalwChainStateV2 {
        let s = readied(&self.armed, &self.registered, keys, self.short, NOW);
        let s = readied(&self.armed, &s, keys, self.id2m, NOW);
        activated(&self.armed, &activated(&self.armed, &s, self.short), self.id2m)
    }

    /// The short row's empty-panel room at `ready` counted seats, by the rate rule's own arithmetic.
    fn capacity(&self, s: &PalwChainStateV2, class: Hash64, ready: u128) -> u64 {
        let b = bundle(&self.p);
        let globals = registry_fold(&self.p, NOW).expect("t12 arms the registry").globals;
        let row = s.model_lifecycle(&class).unwrap();
        let per_span = ready * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
        palw_panel_capacity_by_rate_v1(
            per_span,
            0,
            row.profile.verification_window_spans as u64,
            row.work.economic_ccu_per_claim * b.panel.seat_count() as u128,
        )
    }
}

fn genesis_keys(p: &kaspa_consensus_core::config::params::Params) -> Vec<PalwBondKeyV2> {
    honest(p)
}

/// **T91: which rows count `ready_eff`, and what op 186 reads.** The decision: the short row past
/// the fence, never the 2M row (C7) and never below it. The count, from the bonds op 186 calls
/// ready: 8 at genesis (the 8k row reads 8, as §3.14 says: nothing moves at genesis); 13 of 48 with
/// forty 130k operators beside the genesis seats; still 13 with a ready 20M operator more (the cap).
/// Op 186's `panel_room` is the rate capacity at that count on the armed bundle and at the bond count
/// on the fence-off twin; the 2M row reads the same room under both.
#[test]
fn t91_the_room_counts_effective_ready_operators_outside_c7_past_the_fence() {
    let f = fixture();
    let p = &f.p;
    let small: Vec<PalwBondKeyV2> = SMALL.map(bond_key).collect();
    let genesis = genesis_keys(p);
    let wide: Vec<PalwBondKeyV2> = genesis.iter().copied().chain(small.iter().copied()).collect();
    let whale: Vec<PalwBondKeyV2> = wide.iter().copied().chain([bond_key(WHALE)]).collect();

    // The decision, per row and per fence.
    let short_row = f.registered.model_lifecycle(&f.short).unwrap().clone();
    let row_2m = f.registered.model_lifecycle(&f.id2m).unwrap().clone();
    assert!(!palw_panel_held_to_final_v1(&short_row) && palw_panel_held_to_final_v1(&row_2m));
    assert_eq!(palw_panel_room_ready_eff_terms_v1(&f.armed, &short_row, NOW), Some(PalwPanelStakeDrawV1::V1));
    assert_eq!(palw_panel_room_ready_eff_terms_v1(&f.armed, &row_2m, NOW), None, "C7 is held by its static cap");
    assert_eq!(palw_panel_room_ready_eff_terms_v1(&f.off, &short_row, NOW), None, "the fence-off twin counts bonds");

    let mut readings = Vec::new();
    for (label, keys, bonds, eff) in [("genesis", &genesis, 8u32, 8u32), ("wide", &wide, 48, 13), ("whale", &whale, 49, 13)] {
        let s = f.ready(keys);
        let armed = op186(p, &f.armed, &s, f.short, NOW);
        let off = op186(p, &f.off, &s, f.short, NOW);
        assert_eq!(
            (armed.ready_seats_now, off.ready_seats_now),
            (bonds, bonds),
            "{label}: op 186's ready_seats_now stays the bond count"
        );
        let counted = palw_panel_room_ready_count_v1(&f.armed, &short_row, NOW, 5, keys.iter().map(|k| s.bond(k).unwrap()));
        assert_eq!(counted, eff, "{label}: ready_eff");
        let bond_count = palw_panel_room_ready_count_v1(&f.off, &short_row, NOW, 5, keys.iter().map(|k| s.bond(k).unwrap()));
        assert_eq!(bond_count, bonds, "{label}: the twin's count");
        assert_eq!(armed.panel_room, f.capacity(&s, f.short, eff as u128), "{label}: op 186 armed reads the capacity at ready_eff");
        assert_eq!(off.panel_room, f.capacity(&s, f.short, bonds as u128), "{label}: op 186 off reads it at the bond count");
        let room_2m = (op186(p, &f.armed, &s, f.id2m, NOW).panel_room, op186(p, &f.off, &s, f.id2m, NOW).panel_room);
        assert_eq!(room_2m.0, room_2m.1, "{label}: the 2M row (C7) reads one room on and off");
        readings.push((label, bonds, eff, armed.panel_room, off.panel_room, room_2m.0));
    }
    println!("(state, ready bonds, ready_eff, short room armed, short room off, 2M room): {readings:?}");
    assert_eq!(readings[0].3, readings[0].4, "at genesis nothing moves: the 8k row counts 8 either way");
    assert!(readings[1].3 < readings[1].4, "beside forty 130k operators the armed room is the smaller");
    assert_eq!(readings[1].3, readings[2].3, "a ready 20M operator moves nothing: its weight is capped");
}

/// **T91: the fold's gate admits exactly op 186's room, on and off.** Forty 130k operators beside
/// the genesis seats, all ready for the short row; RICH producers (never ready) carry unlicensed
/// attempts one a block. On every parent op 186's room is positive iff the fold's own class gate
/// (`panel_rate_v1`) admits; the fold then admits exactly the first room (13's capacity armed, 48's
/// off) and refuses the next with `PanelRoomExhausted`, whose budget is the per-span replay of the
/// counted seats over the row's window — the fold, `panel_rate_v1` and op 186 agree.
#[test]
fn t91_the_fold_gate_admits_the_op186_room_counted_by_ready_eff() {
    let f = fixture();
    let p = &f.p;
    let wide: Vec<PalwBondKeyV2> = genesis_keys(p).into_iter().chain(SMALL.map(bond_key)).collect();
    let globals = registry_fold(p, NOW).expect("t12 arms the registry").globals;
    let mut firsts = Vec::new();
    for (label, sp, counted) in [("armed", &f.armed, 13u128), ("off", &f.off, 48u128)] {
        let mut s = f.ready(&wide);
        let first_room = op186(p, sp, &s, f.short, NOW).panel_room;
        assert_eq!(first_room, f.capacity(&s, f.short, counted), "{label}: the first room");
        let mut daa = NOW;
        let mut admitted = 0u64;
        let refused = loop {
            daa += 1;
            s = readied(sp, &s, &wide, f.short, daa - 1);
            let room = op186(p, sp, &s, f.short, daa).panel_room;
            let verdict = gate(p, sp, &s, f.short, daa);
            assert_eq!(room > 0, verdict.is_ok(), "{label}: op186 room {room} vs gate {verdict:?} after {admitted}");
            let producer = PRODUCERS.start + admitted % (PRODUCERS.end - PRODUCERS.start);
            let (env, key, claim) = junk_attempt(
                f.short,
                bond_key(producer),
                pubkey_of(producer),
                &operator_pubkey_of(producer),
                1_000,
                0x91 + admitted,
                0x9_1000 + admitted,
            );
            match go(p, sp, &s, &ctx(0x9200_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key) {
                Ok((next, _, _)) => {
                    assert!(next.claim(&claim).is_some());
                    s = next;
                    admitted += 1;
                }
                Err(e) => break e,
            }
            assert!(admitted <= 40, "{label}: the room is bounded");
        };
        println!("{label}: first room {first_room}, admitted {admitted}, then {refused:?}");
        assert_eq!(admitted, first_room, "{label}: the gate admits exactly the op-186 room");
        let row = s.model_lifecycle(&f.short).unwrap();
        let window = row.profile.verification_window_spans as u64;
        let per_span = counted * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
        match refused {
            PalwStateV2Error::PanelRoomExhausted { class, budget, horizon_spans, .. } => {
                assert_eq!(class, f.short);
                assert_eq!((budget, horizon_spans), (per_span * window as u128, window), "{label}: panel_rate_v1's budget");
            }
            other => panic!("{label}: expected PanelRoomExhausted, got {other:?}"),
        }
        firsts.push(first_room);
    }
    assert!(firsts[0] < firsts[1], "ready_eff is the smaller capacity: {firsts:?}");
}

/// **T91: the floor is not gated by the room**, on or off: its row is `Active` whatever the model
/// rows' rooms read, and the fold's gate admits a floor claim beside the full short row.
#[test]
fn t91_the_floor_is_not_gated_by_the_room() {
    let f = fixture();
    let p = &f.p;
    let wide: Vec<PalwBondKeyV2> = genesis_keys(p).into_iter().chain(SMALL.map(bond_key)).collect();
    let s = f.ready(&wide);
    let base = bundle(p).base_class_id;
    for sp in [&f.armed, &f.off] {
        assert!(gate(p, sp, &s, base, NOW + 1).is_ok(), "the floor's gate");
        assert_eq!(op186(p, sp, &s, base, NOW).panel_room, 0, "op 186 reads no room for the floor");
    }
}
