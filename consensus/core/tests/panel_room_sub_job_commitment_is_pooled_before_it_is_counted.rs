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
//! rate room now asks what the class would owe with the commitment pooled in. A held class's static
//! cap still counts the claim asked about whole: each claim draws its own panel, and c_2M = 1 is
//! one claim.
//!
//! On testnet-12's genesis through the real fold and the acceptance rehearsal, free prompts priced
//! in leaves (`canonical_work_daa = None`, as the cross-class test does); the short-window row's
//! canonical job is 8 quanta.
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
fn short_row_at_capacity(held: bool) -> (Params, PalwStateParamsV2, Hash64, PalwChainStateV2, u64) {
    let p = t12();
    let sp = bundle(&p).state.clone();
    let (short, _) = model_classes(&p);
    let honest = honest(&p);
    let now = 1_000u64;
    let q = short_quantum_leaves(&p, &sp, short);
    let mut s = activated(&sp, &genesis_state(&p), short);
    if !held {
        s = edited(&sp, &s, |c| {
            c.class_step_ladders.remove(&short);
        });
    }
    assert_eq!(s.class_is_held_v1(&short), held);
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

/// Not held, the short row's room pools the commitment: at room 0 a commitment that fills the last
/// part-job is kept and one that starts a job is refused. Held, its static cap of 5 refuses every
/// new claim beside five jobs, part-job or not.
#[test]
fn at_zero_room_a_commitment_that_fills_the_last_part_job_is_kept_and_a_whole_job_is_refused() {
    for held in [false, true] {
        let (p, sp, short, s, daa) = short_row_at_capacity(held);
        let q = short_quantum_leaves(&p, &sp, short);
        let block = ctx(0x4500_0002, daa, daa, 0);
        let attempt_gate = gate(&p, &sp, &s, short, daa);
        let one_q = rehearse(&p, &sp, &block, &s, &commitment(short, q, 1, COMMITTER, 0xC5), None);
        let seven_q = rehearse(&p, &sp, &block, &s, &commitment(short, q, 7, COMMITTER, 0xC6), None);
        let whole = rehearse(&p, &sp, &block, &s, &commitment(short, q, 8, COMMITTER, 0xC7), None);
        let show = |r: &Result<PalwChainStateV2, PalwStateV2Error>| r.as_ref().map(|_| "kept").map_err(|e| format!("{e:?}"));
        println!(
            "held {held}: attempt gate {attempt_gate:?}; 1 quantum {:?}; 7 quanta {:?}; a whole job {:?}",
            show(&one_q),
            show(&seven_q),
            show(&whole)
        );
        assert!(attempt_gate.is_err(), "an attempt meets a room of 0 (held {held})");
        if held {
            let capped = |r: &Result<PalwChainStateV2, PalwStateV2Error>| match r {
                Err(PalwStateV2Error::ClassInflightCapped { class, inflight: 5, cap: 5 }) => *class == short,
                _ => false,
            };
            assert!(capped(&one_q) && capped(&seven_q) && capped(&whole), "the held row's cap of 5 refuses a sixth claim of any size");
        } else {
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
    }
}
