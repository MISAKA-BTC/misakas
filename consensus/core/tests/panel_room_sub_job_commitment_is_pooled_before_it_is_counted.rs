//! **A free-prompt commitment is pooled with its class's claims in flight BEFORE the gate counts
//! it** — 2026-09-24 audit #4 review item 4 (the room is exact), as its verification of f8c91f19
//! found it for sub-job commitments.
//!
//! The rate rule counts each class's free-prompt claims in whole canonical jobs of their POOLED
//! quanta (`palw_inflight_claims_counted_v1`, DoS audit #11). At f8c91f19 the gate still added the
//! commitment asked about as whole jobs on top of that pooled count — `⌈quanta / per_job⌉`, at
//! least one — so a one-quantum commitment that fills its class's last part-job, and raises no
//! class's term, was refused as if it took a whole claim more: on the block's own attempt's behalf
//! when it was of another class (the verification's probe: `PanelRoomExhausted` for 2M with the 2M
//! op-186 room still 1 after it), and by its own class's room when that was 0. Past the fence the
//! rate room now asks what the class would owe with the commitment pooled in. The static cap of a
//! class held to Final (ADR-0152's C7, `palw_panel_held_to_final_v1`: testnet-12's 2M row) still
//! counts the claim asked about whole: each claim draws its own panel, and c_2M = 1 is one claim.
//!
//! On testnet-12's genesis through the real fold and the acceptance rehearsal, free prompts priced
//! in leaves (`canonical_work_daa = None`, as the cross-class test does); a canonical job is 8
//! quanta. The short-window row is the row as shipped: it records ADR-0119's held ladder, and its
//! 3-span window is not C7's, so the rate room alone judges it.
//!
//! Run: cargo test -p kaspa-consensus-core --test panel_room_sub_job_commitment_is_pooled_before_it_is_counted

#[path = "panel_room_common.rs"]
mod room;
use room::*;

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::config::params::Params;
use kaspa_consensus_core::palw_freeprompt_v3::PalwFpPrefixStateV1;
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwBlockWorkV3, PalwChainStateV2, PalwClaimSourceV2, PalwConsensusObjectV2, PalwStateParamsV2,
    PalwStateV2Error, PalwTransitionExtrasV1, palw_v2_apply_one_object_v1,
};
use kaspa_consensus_core::palw_work_target_v1::{palw_panel_capacity_by_rate_v1, palw_panel_held_to_final_v1};

const PRODUCER: u64 = 41;
const COMMITTER: u64 = 42;

fn fp_extras(p: &Params, daa: u64) -> PalwTransitionExtrasV1 {
    let mut e = room_extras(p, daa);
    e.canonical_work_daa = None;
    e
}

/// A free-prompt commitment of `quanta` quanta on `class` by `n`'s bond.
fn commitment(class_id: Hash64, quantum_leaves: u64, quanta: u64, n: u64, seed: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::FreePromptCommitted {
        job_pin: kaspa_hashes::Hash64::default(),
        claim: h(0xF0_0000 + seed),
        class_id,
        bond: bond_key(n),
        executor_pubkey: pubkey_of(n),
        work_leaves: quantum_leaves * quanta,
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

/// The quanta the fold recorded for `o`'s claim on `s`.
fn quanta_of(s: &PalwChainStateV2, o: &PalwConsensusObjectV2) -> u32 {
    let PalwConsensusObjectV2::FreePromptCommitted { claim, .. } = o else { return 0 };
    match &s.claim(claim).expect("the commitment folded").source {
        PalwClaimSourceV2::FreePrompt { quanta, .. } => *quanta,
        PalwClaimSourceV2::Attempt => 0,
    }
}

/// The acceptance rehearsal of one object on `base`, with `own` as the block's own attempt's class.
fn rehearse(
    p: &Params,
    sp: &PalwStateParamsV2,
    block: &PalwBlockContextV2,
    base: &PalwChainStateV2,
    o: &PalwConsensusObjectV2,
    own: Option<Hash64>,
) -> Result<PalwChainStateV2, PalwStateV2Error> {
    let f = flags(p, block.daa_score);
    let mut e = fp_extras(p, block.daa_score);
    e.own_attempt_class = own;
    palw_v2_apply_one_object_v1(
        base,
        sp,
        block,
        o,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &e,
    )
}

/// The short row's canonical job in leaves over the quanta a job holds: one quantum's leaves.
fn short_quantum_leaves(p: &Params, sp: &PalwStateParamsV2, short: Hash64) -> u64 {
    let job = genesis_classes(p).iter().find(|c| c.0 == short).expect("the short row").1;
    let per_job = sp.fp_quanta_per_canonical_job() as u64;
    assert_eq!(per_job, 8, "testnet-12's canonical job is eight quanta");
    assert_eq!(job % per_job, 0, "the short row's job divides into whole quanta");
    job / per_job
}

#[test]
fn a_one_quantum_commitment_in_the_last_part_job_is_kept_beside_the_2m_attempt() {
    let p = t12();
    let b = bundle(&p);
    let sp = b.state.clone();
    let (short, id2m) = model_classes(&p);
    let honest = honest(&p);
    let now = 1_000u64;
    let q = short_quantum_leaves(&p, &sp, short);

    let mut s = activated(&sp, &activated(&sp, &genesis_state(&p), short), id2m);
    s = readied(&sp, &s, &honest, short, now);
    s = readied(&sp, &s, &honest[..7], id2m, now);
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4400_0000, now, now, 0),
        &[bond_obj(PRODUCER, RICH), bond_obj(COMMITTER, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, now),
    )
    .expect("bonds")
    .0;
    s = readied(&sp, &s, &honest, short, now);
    s = readied(&sp, &s, &honest[..7], id2m, now);
    let next = now + 1;
    let block = ctx(0x4400_0001, next, next, T12_BLOCK_SUBSIDY_SOMPI);

    // A whole job and one quantum more: 9 quanta, pooled to 2 jobs, the 2M attempt still fits.
    let whole = commitment(short, q, 8, COMMITTER, 0xB1);
    let one_q = commitment(short, q, 1, COMMITTER, 0xB2);
    let s1 = rehearse(&p, &sp, &block, &s, &whole, Some(id2m)).expect("a whole job leaves the 2M attempt its room");
    let s2 = rehearse(&p, &sp, &block, &s1, &one_q, Some(id2m)).expect("9 quanta leave it its room");
    assert_eq!((quanta_of(&s1, &whole), quanta_of(&s2, &one_q)), (8, 1));
    assert_eq!(owed(&p, &s2, short), 2, "9 quanta pool to 2 jobs");
    assert_eq!(op186(&p, &sp, &s2, id2m, next).panel_room, 1);

    // A second one-quantum commitment raises no class's term (10 quanta, 2 jobs): kept with the 2M
    // attempt reserved. f8c91f19 counted it as a whole claim more and refused it for 2M.
    let one_q2 = commitment(short, q, 1, COMMITTER, 0xB3);
    let alone = rehearse(&p, &sp, &block, &s2, &one_q2, None).expect("the commitment fits its own class");
    assert_eq!(owed(&p, &alone, short), 2, "10 quanta are still 2 jobs");
    assert_eq!(op186(&p, &sp, &alone, id2m, next).panel_room, 1, "and the 2M attempt still has its room after it");
    let s3 = rehearse(&p, &sp, &block, &s2, &one_q2, Some(id2m));
    println!("second one-quantum commitment, 2M attempt reserved: {:?}", s3.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}")));
    let s3 = s3.expect("pooled, it takes no room from the 2M attempt and is kept");

    // The boundary is the pooled job: 6 quanta more (16, 2 jobs) are kept; 7 more (17, 3 jobs)
    // leave the 2M attempt no room and are refused on its behalf.
    let six = commitment(short, q, 6, COMMITTER, 0xB4);
    let seven = commitment(short, q, 7, COMMITTER, 0xB5);
    let with_six = rehearse(&p, &sp, &block, &s3, &six, Some(id2m)).expect("16 quanta are 2 jobs: kept");
    assert_eq!(owed(&p, &with_six, short), 2);
    let with_seven = rehearse(&p, &sp, &block, &s3, &seven, Some(id2m));
    println!("seven quanta more, 2M attempt reserved: {:?}", with_seven.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}")));
    assert!(
        matches!(with_seven, Err(PalwStateV2Error::PanelRoomExhausted { class, .. }) if class == id2m),
        "17 quanta are 3 jobs, which leave the 2M attempt none"
    );
    let seven_alone = rehearse(&p, &sp, &block, &s3, &seven, None).expect("seven quanta fit their own class");
    assert_eq!(owed(&p, &seven_alone, short), 3);

    // The block the acceptance layer builds from those: four commitments and the 2M attempt, which
    // is admitted.
    let pwu = class_pwu(&p, &s, id2m, next);
    let (env, key, own) =
        junk_attempt(id2m, bond_key(PRODUCER), pubkey_of(PRODUCER), &operator_pubkey_of(PRODUCER), pwu, 0x44_01, 0x44_0000_01);
    let objects = [whole, one_q, one_q2, six];
    let folded = fold_with(&p, &sp, &s, &block, &objects, PalwBlockWorkV3::Attempt(&env), key, &fp_extras(&p, next));
    let (after, _, skips) = folded.unwrap_or_else(|e| panic!("four commitments and the 2M attempt fold: {e}"));
    assert!(skips.is_empty(), "{skips:?}");
    assert!(after.claim(&own).is_some(), "the 2M attempt is admitted");
    assert!(objects.iter().all(|o| quanta_of(&after, o) > 0), "every commitment folded");
    assert_eq!(owed(&p, &after, short), 2);
}

/// The short row at capacity — five jobs owed, op-186 room 0 — with its last job one quantum short.
fn short_row_at_capacity() -> (Params, PalwStateParamsV2, Hash64, PalwChainStateV2, u64) {
    let p = t12();
    let sp = bundle(&p).state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let now = 1_000u64;
    let q = short_quantum_leaves(&p, &sp, short);
    let mut s = activated(&sp, &genesis_state(&p), short);
    assert!(
        s.class_is_held_v1(&short) && !palw_panel_held_to_final_v1(s.model_lifecycle(&short).unwrap()),
        "the premise: the short row records a held ladder and is judged by the room alone"
    );
    s = readied(&sp, &s, &honest, short, now);
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4500_0000, now, now, 0),
        &[bond_obj(COMMITTER, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, now),
    )
    .expect("the bond")
    .0;
    let next = now + 1;
    s = readied(&sp, &s, &honest, short, now);
    assert_eq!(op186(&p, &sp, &s, short, next).panel_room, 5, "eight ready seats hold five short-row jobs");
    // Four whole jobs and one quantum: 33 quanta, 5 jobs.
    let mut objects: Vec<_> = (0..4).map(|i| commitment(short, q, 8, COMMITTER, 0xC0 + i)).collect();
    objects.push(commitment(short, q, 1, COMMITTER, 0xC4));
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4500_0001, next, next, 0),
        &objects,
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, next),
    )
    .expect("33 quanta fold")
    .0;
    let next = next + 1;
    s = readied(&sp, &s, &honest, short, next - 1);
    assert_eq!(owed(&p, &s, short), 5, "33 quanta are 5 jobs");
    assert_eq!(op186(&p, &sp, &s, short, next).panel_room, 0, "and the room is 0");
    (p, sp, short, s, next)
}

/// The short row's room pools the commitment: at room 0 a commitment that fills the last part-job
/// is kept and one that starts a job is refused.
#[test]
fn at_zero_room_a_commitment_that_fills_the_last_part_job_is_kept_and_a_whole_job_is_refused() {
    let (p, sp, short, s, daa) = short_row_at_capacity();
    let q = short_quantum_leaves(&p, &sp, short);
    let block = ctx(0x4500_0002, daa, daa, 0);
    let attempt_gate = gate(&p, &sp, &s, short, daa);
    let one_q = rehearse(&p, &sp, &block, &s, &commitment(short, q, 1, COMMITTER, 0xC5), None);
    let seven_q = rehearse(&p, &sp, &block, &s, &commitment(short, q, 7, COMMITTER, 0xC6), None);
    let whole = rehearse(&p, &sp, &block, &s, &commitment(short, q, 8, COMMITTER, 0xC7), None);
    let show = |r: &Result<PalwChainStateV2, PalwStateV2Error>| r.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}"));
    println!(
        "attempt gate {attempt_gate:?}; 1 quantum {:?}; 7 quanta {:?}; a whole job {:?}",
        show(&one_q),
        show(&seven_q),
        show(&whole)
    );
    assert!(attempt_gate.is_err(), "an attempt meets a room of 0");
    let one_q = one_q.unwrap_or_else(|e| panic!("34 quanta are still 5 jobs: kept: {e}"));
    assert_eq!(owed(&p, &one_q, short), 5);
    let seven_q = seven_q.unwrap_or_else(|e| panic!("40 quanta are still 5 jobs: kept: {e}"));
    assert_eq!(owed(&p, &seven_q, short), 5);
    assert!(
        matches!(whole, Err(PalwStateV2Error::PanelRoomExhausted { class, .. }) if class == short),
        "41 quanta are 6 jobs, past the capacity of 5: {:?}",
        show(&whole)
    );
}

/// A class held to Final — the 2M row — counts the claim asked about whole at its static cap: with
/// one one-quantum commitment in flight (one job of c_2M = 1), a commitment of any size is refused
/// at the cap, though pooled it would add no job (one and seven quanta) or fit the rate's capacity of
/// two (a whole job).
#[test]
fn a_class_held_to_final_refuses_a_part_job_commitment_at_its_cap() {
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
    let row = s.model_lifecycle(&id2m).unwrap().clone();
    assert!(palw_panel_held_to_final_v1(&row) && row.profile.max_inflight_claims == 1, "the premise: 2M is C7, c_2M = 1");
    s = readied(&sp, &s, &honest, id2m, now);
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4600_0000, now, now, 0),
        &[bond_obj(COMMITTER, RICH)],
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, now),
    )
    .expect("the bond")
    .0;
    let next = now + 1;
    s = readied(&sp, &s, &honest, id2m, now);
    assert_eq!(op186(&p, &sp, &s, id2m, next).panel_room, 1, "eight ready seats: the 2M room is its cap of one");
    let first = commitment(id2m, q, 1, COMMITTER, 0xD0);
    s = fold_with(
        &p,
        &sp,
        &s,
        &ctx(0x4600_0001, next, next, 0),
        &[first.clone()],
        PalwBlockWorkV3::None,
        Hash64::default(),
        &fp_extras(&p, next),
    )
    .expect("a one-quantum 2M commitment folds")
    .0;
    assert_eq!(quanta_of(&s, &first), 1);
    let next = next + 1;
    s = readied(&sp, &s, &honest, id2m, next - 1);
    let globals = registry_fold(&p, next).expect("the registry").globals;
    let per_span = 8 * globals.reference_work_per_span * (globals.utilization_permille.min(1_000) as u128) / 1_000;
    let capacity = palw_panel_capacity_by_rate_v1(
        per_span,
        0,
        row.profile.verification_window_spans as u64,
        row.work.economic_ccu_per_claim * b.panel.seat_count() as u128,
    );
    assert_eq!((owed(&p, &s, id2m), capacity), (1, 2), "one job owed of a rate capacity of two");
    assert_eq!(op186(&p, &sp, &s, id2m, next).panel_room, 0, "and the room is the cap's: 0");

    let block = ctx(0x4600_0002, next, next, 0);
    let capped = |r: &Result<PalwChainStateV2, PalwStateV2Error>| matches!(r, Err(PalwStateV2Error::ClassInflightCapped { class, inflight: 1, cap: 1 }) if *class == id2m);
    let one_q = rehearse(&p, &sp, &block, &s, &commitment(id2m, q, 1, COMMITTER, 0xD1), None);
    let seven_q = rehearse(&p, &sp, &block, &s, &commitment(id2m, q, 7, COMMITTER, 0xD2), None);
    let whole = rehearse(&p, &sp, &block, &s, &commitment(id2m, q, 8, COMMITTER, 0xD3), None);
    let show = |r: &Result<PalwChainStateV2, PalwStateV2Error>| r.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}"));
    println!("2M at its cap: 1 quantum {:?}; 7 quanta {:?}; a whole job {:?}", show(&one_q), show(&seven_q), show(&whole));
    assert!(capped(&one_q) && capped(&seven_q) && capped(&whole), "the 2M row's cap of 1 refuses a second claim of any size");
    assert!(
        matches!(gate(&p, &sp, &s, id2m, next), Err(PalwStateV2Error::ClassInflightCapped { inflight: 1, cap: 1, .. })),
        "and an attempt"
    );
}
