//! **A gated object of another class that would take the block's own attempt's room is refused,
//! and the attempt is admitted** — 2026-09-24 audit #4 review item 2 (the cross-class path).
//!
//! The rate rule's budget a span is each class's OWN ready seats' replay, while the demand is every
//! class's. So a free-prompt commitment can fit its own class's room and still leave another
//! class — the one the block's own attempt is on — with none. At e93be0f2 the step-3 reservation
//! for the own attempt only counted it inside its own class's room: commitments on the short-window
//! row (eight ready seats) each passed their own gate, and after the third one the 2M attempt's
//! room at step 4 was 0, which refuses the whole block (worked in arithmetic, `cross.py`, and
//! masked at the parent because the 2M room was always 0). Past the fence a gated object of
//! another class is refused where the own attempt fitted before it and would not after it.
//!
//! On testnet-12's genesis, through the real fold and the acceptance rehearsal: the 2M row admitting
//! on seven ready seats (room 1), the short-window row on eight, and one-job free-prompt
//! commitments on the short row beside a 2M attempt. Free prompts on testnet-12 are priced in
//! compute, which needs a lane-certified class work profile; this folds them priced in leaves
//! (`canonical_work_daa = None`, as the review's item-3 fixture does). The class gate runs before
//! the price, and a whole-job commitment counts one claim either way.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_cross_class_commitment_cannot_take_the_own_attempts_room

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockWorkV3, PalwChainStateV2, PalwConsensusObjectV2, PalwStateV2Error, PalwTransitionExtrasV1, palw_v2_apply_one_object_v1,
};

const PRODUCER: u64 = 41;
const COMMITTER: u64 = 42;

fn fp_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = room_extras(p, daa);
    e.canonical_work_daa = None;
    e
}

/// A whole-job free-prompt commitment on `class` by `n`'s bond.
fn commitment(class_id: Hash64, leaves: u64, n: u64, seed: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF0_0000 + seed),
        class_id,
        bond: bond_key(n),
        executor_pubkey: pubkey_of(n),
        work_leaves: leaves,
        prompt_token_ids_hash: h(0x7E_0000 + seed),
        prompt_tokens: 0,
        prompt_token_ids: Vec::new(),
        decode_tokens_executed: 1,
        trace_root: h(0x1F00_0000 + seed),
        output_root: h(0x2F00_0000 + seed),
        execution_root: h(0x3F00_0000 + seed),
        trace_chunk_count: 1,
        trace_retention_daa: 9_999_999,
        consumed_prefix_state: PalwFpPrefixStateV1::genesis(class_id),
    }
}

#[test]
fn a_commitment_that_would_take_the_2m_attempts_room_is_refused_and_the_attempt_is_admitted() {
    // ADR-0152 §4-quater (U-D1): the 2M row is closed at launch, so a live 2M claim exists only past the flag day
    // that installs its measured row — this test's premise runs there (`t12_2m_open`, measuring the derived
    // 13,995-DAA deadline).
    let p = t12_2m_open();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, id2m) = model_classes(&p);
    let honest = honest(&p);
    let now = 1_000u64;
    let g = genesis_state(&p);
    let short_leaves = genesis_classes(&p).iter().find(|c| c.0 == short).unwrap().1;

    let mut s = activated(&sp, &activated(&sp, &g, short), id2m);
    s = readied(&sp, &s, &honest, short, now);
    s = readied(&sp, &s, &honest[..7], id2m, now);
    let f = flags(&p, now);
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4100_0000, now, now, 0),
        &[bond_obj(PRODUCER, RICH), bond_obj(COMMITTER, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, now),
    )
    .expect("bonds")
    .0;
    let next = now + 1;
    s = readied(&sp, &s, &honest, short, now);
    s = readied(&sp, &s, &honest[..7], id2m, now);
    let rooms = (op186(&p, &sp, &s, short, now).panel_room, op186(&p, &sp, &s, id2m, now).panel_room);
    assert_eq!(rooms, (5, 1), "(short, 2M) rooms on the parent");
    assert!(gate(&p, &sp, &s, id2m, next).is_ok(), "the producer's pre-check admits the 2M attempt");

    let c: Vec<PalwConsensusObjectV2> = (1..=3).map(|i| commitment(short, short_leaves, COMMITTER, 0xC0 + i)).collect();
    let pwu = class_pwu(&p, &s, id2m, next);
    let (env, key, own) =
        junk_attempt(id2m, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), pwu, 0x41_01, 0x41_0000_01);
    let block = ctx(0x4100_0001, next, next, T12_BLOCK_SUBSIDY_SOMPI);

    // The acceptance rehearsal, one object at a time, with and without the own attempt reserved.
    let rehearse = |base: &PalwChainStateV2, o: &PalwConsensusObjectV2, own: Option<Hash64>| {
        let mut e = fp_extras(&p, next);
        e.own_attempt_class = own;
        palw_v2_apply_one_object_v1(
            base,
            &sp,
            &block,
            o,
            f.unavailable_abstains,
            f.capability_bound,
            f.uncertified_weightless,
            f.da_court,
            &e,
        )
    };
    let after_one = rehearse(&s, &c[0], Some(id2m)).expect("the first commitment leaves the 2M attempt its room");
    let after_two = rehearse(&after_one, &c[1], Some(id2m)).expect("so does the second");
    let third_reserved = rehearse(&after_two, &c[2], Some(id2m));
    let third_alone = rehearse(&after_two, &c[2], None);
    let short_room_after_two = op186(&p, &sp, &after_two, short, next).panel_room;
    println!(
        "after two commitments: short room {short_room_after_two}, 2M room {}; third, 2M attempt reserved: {:?}; third, nothing reserved: {:?}",
        op186(&p, &sp, &after_two, id2m, next).panel_room,
        third_reserved.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}")),
        third_alone.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}"))
    );
    let after_three = third_alone.expect("the third commitment fits its own class");
    assert_eq!(op186(&p, &sp, &after_three, id2m, next).panel_room, 0, "and after it the 2M class has no room");
    assert!(
        matches!(third_reserved, Err(PalwStateV2Error::PanelRoomExhausted { class, .. }) if class == id2m),
        "with the 2M attempt reserved, the third commitment is refused on the 2M class's behalf"
    );

    // The block the acceptance layer builds: two commitments and the attempt, which is admitted.
    let two_and_attempt = fold_with(&p, &sp, &s, &block, &c[..2], PalwBlockWorkV3::Attempt(&env), key, &fp_extras(&p, next));
    let (after, _, _) = two_and_attempt.as_ref().unwrap_or_else(|e| panic!("two commitments and the 2M attempt fold: {e}"));
    assert!(after.claim(&own).is_some(), "the 2M attempt is admitted");
    assert!(
        c[..2].iter().all(|o| matches!(o, PalwConsensusObjectV2::FreePromptCommitted { claim, .. } if after.claim(claim).is_some()))
    );

    // A block carrying the third too is refused on the 2M class's behalf (the rehearsal above is
    // what keeps the third out of the block the acceptance layer builds); without the attempt the
    // same three commitments fold.
    let three_and_attempt = fold_with(&p, &sp, &s, &block, &c, PalwBlockWorkV3::Attempt(&env), key, &fp_extras(&p, next));
    println!("three commitments and the attempt: {:?}", three_and_attempt.as_ref().map(|_| ()).map_err(|e| format!("{e:?}")));
    assert!(matches!(three_and_attempt, Err(PalwStateV2Error::PanelRoomExhausted { class, .. }) if class == id2m));
    let three_alone = fold_with(&p, &sp, &s, &block, &c, PalwBlockWorkV3::None, Hash64::default(), &fp_extras(&p, next));
    assert!(three_alone.is_ok(), "the three commitments alone fold: {:?}", three_alone.as_ref().map(|_| ()));
}
