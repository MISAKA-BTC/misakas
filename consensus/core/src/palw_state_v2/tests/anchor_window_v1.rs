//! **ADR-0170 (`palw_anchor_window_v1`) through the fold: the execution lane's seed anchor is a window.**
//!
//! Three pieces, each pinned here on the schedule rotation (the admission jury's half is `adr0135::admission_independence::anchor_window_v1`):
//!
//! * **M1** — an admitted attempt a chain block MERGES records the anchor beside the block's own (the last admitted one wins; a
//!   refused one records nothing); below the fence a merged attempt is never an anchor;
//! * **M2** — the anchor is not cleared at a span boundary past the fence (below it, it is the span before's and cleared);
//! * **M3** — a due snapshot is seeded by the latest anchor of the window `S − 24 … S − 1`, but only one recorded at or after the span the
//!   snapshot was taken in (participants first, randomness after); an anchor older than the window seeds nothing.
//!
//! And the rule that makes the fence safe to ship: **before its height every fold is byte-identical** — the same chain under a fence at a
//! height it never reaches has the same state root at every block as the chain with no fence at all.

use super::*;
use crate::palw_execution_lane_v1::{PalwExecPermitUseV1, PalwExecSeedAnchorV1, palw_execution_span_seed_v1};

const SPAN: u64 = 100;

fn on() -> PalwStateParamsV2 {
    params().with_worker_carve_permille(620).unwrap().with_anchor_window_from_daa(Some(0))
}

fn off() -> PalwStateParamsV2 {
    params().with_worker_carve_permille(620).unwrap()
}

/// The fence's height is far past every block of these chains: the dormant twin the byte-identity test compares with `off()`.
fn far() -> PalwStateParamsV2 {
    params().with_worker_carve_permille(620).unwrap().with_anchor_window_from_daa(Some(1_000_000_000))
}

/// ADR-0151's economic-safety bundle with a one-span maturity (the challenge window is 20 DAA at a 100-DAA span): the schedule
/// rotation's branch the fence widens.
fn safety(p: &PalwStateParamsV2, open_round: u64) -> PalwTransitionExtrasV1 {
    PalwTransitionExtrasV1 {
        economic_safety: Some(PalwEconomicSafetyFoldV1 {
            target_time_per_block_ms: 1_000,
            permit_value_sompi: 1,
            maturity_daa: p.window_challenge(),
        }),
        ..round_extras_quanta(SPAN, 10, open_round, Vec::<PalwExecPermitUseV1>::new())
    }
}

/// One block with the lane armed: an optional own attempt (with the execution key the processor derives from its header) and the
/// attempts it merges — `(envelope, the merged block, its execution key)` — checked for internal consistency, the delta's round trip and
/// the carriage's, as `fold_round` is. Returns the state, the delta and the skips (the merged works the fold refused).
fn fold_with_merged(
    parent: &PalwChainStateV2,
    p: &PalwStateParamsV2,
    c: &PalwBlockContextV2,
    own: Option<(&PalwAttemptEnvelopeV2, Hash64)>,
    merged: &[(&PalwAttemptEnvelopeV2, BlockHash, Hash64)],
    extras: &PalwTransitionExtrasV1,
) -> (PalwChainStateV2, PalwStateDeltaV2, Vec<(BlockHash, String)>) {
    let admission = crate::palw_admission_v2::PalwAdmissionParamsV2::new(500).unwrap();
    let works: Vec<PalwMergedWorkV1<'_>> = merged
        .iter()
        .map(|(envelope, carrying_block, execution_key)| PalwMergedWorkV1 {
            carrying_block: *carrying_block,
            work: PalwBlockWorkV3::Attempt(envelope),
            execution_key: *execution_key,
            subsidy: 0,
            escrow_carve: None,
            bits: 0,
            job_anchor: Hash64::default(),
        })
        .collect();
    let (work, key) = match own {
        Some((envelope, key)) => (PalwBlockWorkV3::Attempt(envelope), key),
        None => (PalwBlockWorkV3::None, Hash64::default()),
    };
    let (state, delta, skips) =
        apply_palw_transition_v7(parent, p, Some(&admission), c, &[], work, &works, key, false, false, false, false, extras)
            .expect("the block stands");
    state.assert_internal_consistency(p).expect("internal consistency after apply");
    assert_eq!(apply_delta_v2(parent, &delta, p).unwrap().state_root(), state.state_root(), "the delta reproduces the transition");
    assert_eq!(revert_delta_v2(&state, &delta, p).unwrap().state_root(), parent.state_root(), "and reverts to the parent");
    let back: PalwStateCarriageV2 = borsh::from_slice(&borsh::to_vec(&PalwStateCarriageV2::from_state(&state)).unwrap()).unwrap();
    assert_eq!(back.into_state(p, Some(state.state_root())).unwrap().state_root(), state.state_root(), "the carriage round-trips");
    (state, delta, skips)
}

fn anchor(span: u64, block: BlockHash, key: u64) -> PalwExecSeedAnchorV1 {
    PalwExecSeedAnchorV1 { span, block, execution_key: h64(key) }
}

#[test]
fn m1_a_merged_admitted_attempt_records_the_anchor_past_the_fence_and_a_refused_one_does_not() {
    let (p, q) = (on(), off());
    let (s5, _) = round_final_in_span_1(&p);
    let (a, b) = (attempt(40, 2), attempt(40, 3));

    // Span 2 opens on a block that carries no attempt of its own but merges one: past the fence it is the anchor, with the span of the
    // block that FOLDED it, the attempt's own block and its own execution key; below the fence a merged attempt anchors nothing.
    let merged = [(&a, block(60), h64(0xE6))];
    let (past, _, skips) = fold_with_merged(&s5, &p, &ctx(6, 200, 6), None, &merged, &safety(&p, 20_000));
    assert!(skips.is_empty(), "the merged attempt is admitted");
    assert_eq!(past.round_seed_anchor(), Some(&anchor(2, block(60), 0xE6)));
    let (below, _, below_skips) = fold_with_merged(&s5, &q, &ctx(6, 200, 6), None, &merged, &safety(&q, 20_000));
    assert!(below_skips.is_empty());
    assert!(below.round_seed_anchor().is_none(), "below the fence a merged attempt is never an anchor");
    assert!(below.claim(&attempt_id_v2(&a.attempt)).is_some() && past.claim(&attempt_id_v2(&a.attempt)).is_some(), "admitted either way: the fence moves the anchor only");

    // The latest admitted attempt of the block wins: the own attempt first, then the mergeset in order.
    let own = (&b, h64(0xE7));
    let (own_then_merged, _, _) = fold_with_merged(&s5, &p, &ctx(6, 200, 6), Some(own), &merged, &safety(&p, 20_000));
    assert_eq!(own_then_merged.round_seed_anchor(), Some(&anchor(2, block(60), 0xE6)), "own first, merged after: the merged one is the last");
    let (own_only, _, _) = fold_with_merged(&s5, &p, &ctx(6, 200, 6), Some(own), &[], &safety(&p, 20_000));
    assert_eq!(own_only.round_seed_anchor(), Some(&anchor(2, block(6), 0xE7)), "an own attempt alone is as it always was");
    let two = [(&a, block(60), h64(0xE6)), (&b, block(61), h64(0xE8))];
    let (last_wins, _, _) = fold_with_merged(&s5, &p, &ctx(6, 200, 6), None, &two, &safety(&p, 20_000));
    assert_eq!(last_wins.round_seed_anchor(), Some(&anchor(2, block(61), 0xE8)), "the last admitted merged attempt");

    // A refused merged attempt records nothing: the same envelope twice is a DuplicateClaim the second time and is skipped, so the anchor
    // stays the first's.
    let twice = [(&a, block(60), h64(0xE6)), (&a, block(62), h64(0xE9))];
    let (kept, _, skips) = fold_with_merged(&s5, &p, &ctx(6, 200, 6), None, &twice, &safety(&p, 20_000));
    assert_eq!(skips.len(), 1, "the duplicate is skipped, not folded");
    assert_eq!(kept.round_seed_anchor(), Some(&anchor(2, block(60), 0xE6)), "and it is no anchor");
}

/// The window chain: span 1's Final is snapshotted at span 2 for span 4 (a one-span maturity); `anchor_in` is the span an attempt-carrying
/// block records the anchor in (`None`: no span carries one); `open_at` the DAA of the block that opens the span the snapshot is due at or
/// later. Returns the states of the blocks that open span 2, an in-between span and the due span.
fn chain_to(
    p: &PalwStateParamsV2,
    anchor_in: Option<u64>,
    open_at: u64,
) -> (PalwChainStateV2, PalwChainStateV2, PalwChainStateV2) {
    let (s5, _) = round_final_in_span_1(p);
    let attempt_block = |parent: &PalwChainStateV2, daa: u64, word: u64, nonce: u64, open_round: u64| {
        fold_with_merged(parent, p, &ctx(word, daa, word), Some((&attempt(40, nonce), h64(0xE0 + word))), &[], &safety(p, open_round)).0
    };
    let plain = |parent: &PalwChainStateV2, daa: u64, word: u64, open_round: u64| {
        fold_with_merged(parent, p, &ctx(word, daa, word), None, &[], &safety(p, open_round)).0
    };
    let carries = |span: u64| anchor_in == Some(span);
    // Span 1 holds the Final; an anchor recorded there precedes the snapshot span 2 takes. (Block words and blue scores only rise.)
    let s5 = if carries(1) { attempt_block(&s5, 150, 6, 10, 10_000) } else { s5 };
    // Span 2 opens (the snapshot is taken here, for span 4).
    let s6 = if carries(2) { attempt_block(&s5, 200, 7, 11, 20_000) } else { plain(&s5, 200, 7, 20_000) };
    // Span 3, and a gap to the DAA the due span opens at.
    let s7 = if carries(3) { attempt_block(&s6, 300, 8, 12, 30_000) } else { plain(&s6, 300, 8, 30_000) };
    let s8 = plain(&s7, open_at, 9, 40_000);
    (s6, s7, s8)
}

#[test]
fn m2_m3_the_anchor_survives_the_span_boundary_and_seeds_the_snapshot_a_span_the_rule_would_have_waited_for() {
    let (p, q) = (on(), off());
    // An attempt carried in span 2 (the block that opens it, so it anchors the snapshot that block takes).
    let (s6, s7, s8) = chain_to(&p, Some(2), 400);
    let pending = s6.round_pending_snapshot().expect("the Final is a snapshot").clone();
    assert_eq!(pending.target_span, 4, "the maturity delays the snapshot by one span");
    let recorded = *s6.round_seed_anchor().expect("the opening block anchors");
    assert_eq!(recorded.span, 2);
    assert_eq!(s7.round_seed_anchor(), Some(&recorded), "M2: span 3 opened and the anchor is still the span-2 one");
    // M3: span 4 opens with no anchor of span 3, and the due snapshot is seeded all the same, by the anchor of span 2.
    let schedule = s8.round_schedule(4).expect("seeded by the window's anchor").clone();
    let (score, frontier) = s7.safe_frontier();
    assert_eq!(schedule.seed, palw_execution_span_seed_v1(&recorded, 4, score, frontier), "the seed is the anchor's execution, the span and the frontier");
    assert!(s8.round_pending_snapshot().is_none(), "the snapshot is spent");
    assert_eq!(s8.round_seed_anchor(), Some(&recorded), "and the anchor stays for the next reader");

    // The same chain below the fence: the anchor is cleared at span 3's opening and span 4 has nothing to seed from — the snapshot waits.
    let (_, l7, l8) = chain_to(&q, Some(2), 400);
    assert!(l7.round_seed_anchor().is_none(), "below the fence the anchor is the span before's, and cleared");
    assert!(l8.round_schedule(4).is_none(), "no anchor of span 3: no schedule");
    assert_eq!(l8.round_pending_snapshot().map(|s| s.target_span), Some(4), "the due snapshot waits, as it always did");
}

#[test]
fn m1_m3_a_merged_anchor_seeds_the_due_snapshot_past_the_fence_and_below_it_the_snapshot_waits() {
    // The regime ADR-0165 creates: no chain block of the whole chain carries an attempt of its own, and the only attempt there is — a REAL
    // one — is MERGED by the block that opens span 2. Past the fence it anchors, the anchor survives span 3's opening, and span 4 seeds the
    // snapshot span 2 took with it; below the fence the same blocks leave no anchor and the due snapshot waits.
    let run = |p: &PalwStateParamsV2| {
        let (s5, _) = round_final_in_span_1(p);
        let a = attempt(40, 2);
        let (s6, _, _) = fold_with_merged(&s5, p, &ctx(6, 200, 6), None, &[(&a, block(60), h64(0xE6))], &safety(p, 20_000));
        let (s7, _, _) = fold_with_merged(&s6, p, &ctx(7, 300, 7), None, &[], &safety(p, 30_000));
        let (s8, _, _) = fold_with_merged(&s7, p, &ctx(8, 400, 8), None, &[], &safety(p, 40_000));
        (s7, s8)
    };
    let (s7, s8) = run(&on());
    let recorded = *s7.round_seed_anchor().expect("the merged attempt's anchor survived span 3's opening");
    assert_eq!((recorded.span, recorded.block), (2, block(60)));
    let schedule = s8.round_schedule(4).expect("span 4 is seeded by a merged attempt's anchor").clone();
    let (score, frontier) = s7.safe_frontier();
    assert_eq!(schedule.seed, palw_execution_span_seed_v1(&recorded, 4, score, frontier));
    assert!(s8.round_pending_snapshot().is_none(), "the snapshot is spent");
    let (l7, l8) = run(&off());
    assert!(l7.round_seed_anchor().is_none() && l8.round_schedule(4).is_none(), "below the fence a merged attempt anchors nothing");
    assert_eq!(l8.round_pending_snapshot().map(|s| s.target_span), Some(4), "and the due snapshot waits");
}

#[test]
fn m3_an_anchor_recorded_before_the_snapshot_was_taken_seeds_nothing_and_one_older_than_the_window_seeds_nothing() {
    let p = on();
    // Recorded in span 1, before span 2 took the snapshot: inside the window of span 4, but participants come first.
    let (_, _, early) = chain_to(&p, Some(1), 400);
    assert!(early.round_seed_anchor().is_some(), "the anchor survives (M2)");
    assert!(early.round_schedule(4).is_none(), "an anchor older than the snapshot seeds nothing");
    assert_eq!(early.round_pending_snapshot().map(|s| s.target_span), Some(4), "the snapshot waits");

    // The window's far edge: W = 24 spans. A block opens span 27 (DAA 2,700).
    let (_, _, span3_at_27) = chain_to(&p, Some(3), 2_700);
    assert!(span3_at_27.round_schedule(27).is_some(), "an anchor of span 3 is 24 spans before span 27: the oldest the window reads");
    assert!(span3_at_27.round_pending_snapshot().is_none());
    let (_, _, span2_at_27) = chain_to(&p, Some(2), 2_700);
    assert!(span2_at_27.round_schedule(27).is_none(), "an anchor of span 2 is 25 spans back: outside the window");
    assert_eq!(span2_at_27.round_pending_snapshot().map(|s| s.target_span), Some(4), "the snapshot waits inside its grace");
}

#[test]
fn before_the_fence_every_fold_is_byte_identical_to_a_chain_with_no_fence_at_all() {
    // The same chains — own anchors, merged attempts, a gap, a due snapshot — under a fence at a height they never reach and under none.
    for anchor_in in [None, Some(1), Some(2), Some(3)] {
        let (a6, a7, a8) = chain_to(&far(), anchor_in, 400);
        let (b6, b7, b8) = chain_to(&off(), anchor_in, 400);
        for (a, b) in [(&a6, &b6), (&a7, &b7), (&a8, &b8)] {
            assert_eq!(a.state_root(), b.state_root(), "anchor in {anchor_in:?}");
            assert_eq!(a.round_seed_anchor(), b.round_seed_anchor());
        }
    }
    let merged_chain = |p: &PalwStateParamsV2| {
        let (s5, _) = round_final_in_span_1(p);
        let a = attempt(40, 2);
        let (s6, _, _) = fold_with_merged(&s5, p, &ctx(6, 200, 6), None, &[(&a, block(60), h64(0xE6))], &safety(p, 20_000));
        let (s7, _, _) = fold_with_merged(&s6, p, &ctx(7, 300, 7), None, &[], &safety(p, 30_000));
        let (s8, _, _) = fold_with_merged(&s7, p, &ctx(8, 400, 8), None, &[], &safety(p, 40_000));
        [s6, s7, s8]
    };
    for (a, b) in merged_chain(&far()).iter().zip(merged_chain(&off()).iter()) {
        assert_eq!(a.state_root(), b.state_root(), "a merged attempt below the fence leaves no fingerprint either");
    }
}
