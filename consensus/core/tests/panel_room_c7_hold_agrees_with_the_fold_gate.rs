//! **The C7 hold, through the fold gate rather than op 186 alone** (adopted from the review of the
//! C7 restoration, 2026-09-24).
//!
//! 1. The short row at TWELVE ready seats: op 186 reads a room of 8 (its rate capacity, past the
//!    static cap of 5). The fold's own gate admits exactly 8 unlicensed attempts, refuses the 9th
//!    with `PanelRoomExhausted`, and agrees with op 186 (`room > 0` iff the gate admits) on every
//!    parent.
//! 2. The 2M row (C7, c = 1) with the block's own 2M attempt reserved: a one-quantum 2M commitment
//!    in step 3 must meet the cap (the reservation counts), so the hold cannot be bypassed by a
//!    same-block commitment beside the own attempt.

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwBondKeyV2, PalwConsensusObjectV2, PalwStateV2Error, palw_v2_apply_one_object_v1,
};
use kaspa_consensus_core::palw_work_target_v1::palw_panel_held_to_final_v1;

#[test]
fn the_short_row_at_twelve_ready_seats_the_gate_admits_eight() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, _) = model_classes(&p);
    let mut daa = 1_000u64;
    let extra: Vec<PalwBondKeyV2> = (0..4u64).map(|i| bond_key(9_950 + i)).collect();
    let mut objs: Vec<_> = (0..4u64).map(|i| bond_obj(9_950 + i, RICH)).collect();
    objs.extend((1..=12u64).map(|n| bond_obj(n, RICH)));
    let mut keys = honest(&p);
    keys.extend(extra);
    assert_eq!(keys.len(), 12);
    let mut s = readied(&sp, &activated(&sp, &genesis_state(&p), short), &honest(&p), short, daa);
    s = go(&p, &sp, &s, &ctx(0x6100_0000 + daa, daa, daa, 0), &objs, PalwBlockWorkV3::None, Hash64::default()).expect("bonds").0;
    assert!(s.model_lifecycle(&short).unwrap().state.admits_claims(), "{:?}", s.model_lifecycle(&short).unwrap().state);
    s = readied(&sp, &s, &keys, short, daa);
    assert_eq!(op186(&p, &sp, &s, short, daa).ready_seats_now, 12);
    let first_room = op186(&p, &sp, &s, short, daa).panel_room;

    let mut admitted = 0u64;
    let mut trail = Vec::new();
    let refused = loop {
        daa += 1;
        s = readied(&sp, &s, &keys, short, daa - 1);
        let room = op186(&p, &sp, &s, short, daa).panel_room;
        let verdict = gate(&p, &sp, &s, short, daa);
        assert_eq!(room > 0, verdict.is_ok(), "op186 room {room} vs gate {verdict:?} after {admitted}");
        let n = admitted + 1;
        let producer = 1 + (n - 1) % 12;
        let (env, key, claim) =
            junk_attempt(short, bond_key(producer), pubkey_of(producer), &operator_pubkey_of(producer), 1_000, 0x61 + n, 0x6_1000 + n);
        match go(&p, &sp, &s, &ctx(0x6100_0000 + daa, daa, daa, T12_BLOCK_SUBSIDY_SOMPI), &[], PalwBlockWorkV3::Attempt(&env), key) {
            Ok((next, _, _)) => {
                assert!(next.claim(&claim).is_some());
                s = next;
                admitted += 1;
                trail.push((owed(&p, &s, short), op186(&p, &sp, &s, short, daa).panel_room));
            }
            Err(e) => break e,
        }
        assert!(admitted <= 20);
    };
    println!("12 ready seats: first room {first_room}; (owed, room) {trail:?}; then {refused:?}");
    assert_eq!(first_room, 8);
    assert_eq!(admitted, 8, "the fold gate admits the op-186 room of 8, past the static cap of 5");
    assert!(matches!(refused, PalwStateV2Error::PanelRoomExhausted { class, .. } if class == short), "{refused:?}");
}

#[test]
fn the_c7_own_attempt_reservation_counts_against_a_same_class_commitment() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (_, id2m) = model_classes(&p);
    let honest = honest(&p);
    let now = 1_000u64;
    let per_job = sp.fp_quanta_per_canonical_job() as u64;
    let job = genesis_classes(&p).iter().find(|c| c.0 == id2m).expect("the 2M row").1;
    let q = (job / per_job).max(1);
    let mut s = activated(&sp, &genesis_state(&p), id2m);
    assert!(palw_panel_held_to_final_v1(s.model_lifecycle(&id2m).unwrap()));
    s = readied(&sp, &s, &honest, id2m, now);
    let mut e = room_extras(&p, now);
    e.canonical_work_daa = None;
    s = fold_with(&p, &sp, &s, &ctx(0x6200_0000, now, now, 0), &[bond_obj(42, RICH)], PalwBlockWorkV3::None, Hash64::default(), &e)
        .expect("the bond")
        .0;
    let next = now + 1;
    s = readied(&sp, &s, &honest, id2m, now);
    assert_eq!(op186(&p, &sp, &s, id2m, next).panel_room, 1);
    let commit = PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF6_0001),
        class_id: id2m,
        bond: bond_key(42),
        executor_pubkey: pubkey_of(42),
        work_leaves: q,
        prompt_token_ids_hash: h(0x7E_6001),
        prompt_tokens: 0,
        prompt_token_ids: Vec::new(),
        decode_tokens_executed: 1,
        trace_root: h(0x1F60_0001),
        output_root: h(0x2F60_0001),
        execution_root: h(0x3F60_0001),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: PalwFpPrefixStateV1::genesis(id2m),
    };
    let block = ctx(0x6200_0001, next, next, 0);
    let f = flags(&p, next);
    let run = |own: Option<Hash64>| {
        let mut e = room_extras(&p, next);
        e.canonical_work_daa = None;
        e.own_attempt_class = own;
        palw_v2_apply_one_object_v1(&s, &sp, &block, &commit, f.unavailable_abstains, f.capability_bound, f.uncertified_weightless, f.da_court, &e)
    };
    let alone = run(None);
    let beside_own = run(Some(id2m));
    println!("2M one-quantum commitment alone: {:?}; beside the own 2M attempt: {:?}", alone.as_ref().map(|_| "kept"), beside_own.as_ref().map(|_| "kept"));
    assert!(alone.is_ok(), "control: alone it takes the one slot: {:?}", alone.err());
    assert!(
        matches!(beside_own, Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "beside the own 2M attempt the cap of one is already taken: {:?}",
        beside_own.as_ref().map(|_| "kept")
    );
}
