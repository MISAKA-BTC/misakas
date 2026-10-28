//! **Lane F2-lock (post-launch, 2026-09-27): the `F + 1,000` seat-lock life of lane V02, applied
//! retroactively at `Params::palw_final_lock_life_retro` — on testnet-12's own fold.**
//!
//! Lane V02's lock-life fence (`palw_final_lock_life`, DAA 750 on the fleet) dates a `Valid` seat's
//! lock at its dating block and re-dates it at `Final` extend-only, so every lock a `Final` below
//! that fence stamped `F + window_court` — and every lock a licence below it stamped
//! `L + window_court` — keeps its long life. Past this fence (height `H`):
//!
//! 1. the crossing block (the first chain block with DAA ≥ `H`) re-dates every seat lock of an honest
//!    `Final` to `min(expiry, max(F + 1,000, H))`, `F` read from the claim's liability record;
//! 2. a `Final` at `F ≥ H` dates its seat locks to exactly `F + 1,000`.
//!
//! The chains here run the lane V02 fences (`palw_final_lock_life` and `palw_final_lock_full_collateral`,
//! both at [`V02`]) the way the fleet runs them at DAA 750, moved above this harness's first block so
//! the chain has long locks to re-date, and every block goes through `apply_palw_transition_v7` with
//! the extras the processor resolves and is checked by the harness (the delta re-applies and reverts,
//! the carriage reloads under its committed root). The dormant twin is the same ruleset with the
//! retro fence `None` — the DAA-750 release as the fleet runs it.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_f2_lock_redate -- --nocapture

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_FINAL_LOCK_LIFE_DAA_V1, PalwDeltaEntryV2, PalwRcoreGateV1, palw_accuser_exposure_v1, palw_bond_accuser_reserve_v1,
    palw_bond_committed_raw_v1, palw_false_valid_lock_slashable_v1, palw_final_lock_life_retro_crossing_v1,
    palw_final_lock_retro_expiry_v1, palw_lock_final_daa_retro_v1, palw_lock_is_committed_v1, palw_rcore_gate_room_v1,
    palw_second_clock_depth_v1,
};

/// Lane V02's two fences, moved here from the fleet's DAA 750 so this harness (first block at 1,001)
/// has locks stamped below them.
const V02: u64 = 2_000;

/// testnet-12's launch ruleset with lane V02's two lock fences at [`V02`] and lane F2-lock at `retro`,
/// re-mirrored and validated.
fn rules(retro: Option<u64>) -> Params {
    let mut p = t12();
    assert_eq!(p.palw_final_lock_life_retro, None, "the launch ruleset leaves the fence dormant");
    p.palw_final_lock_life = Some(ForkActivation::new(V02));
    p.palw_final_lock_full_collateral = Some(ForkActivation::new(V02));
    p.palw_final_lock_life_retro = retro.map(ForkActivation::new);
    p.sync_palw_rcore_plus();
    p.validate_palw_v2().expect("a runnable ruleset");
    p
}

/// The five floor seats' licensing `Valid`s on `id`, in the next block.
fn licence(t: &mut Tape, id: Hash64, bound: u64) {
    let receipts: Vec<_> = t.c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    t.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
}

/// A floor claim accepted, bound and licensed in three blocks; returns it and its licence DAA.
fn licensed_claim(t: &mut Tape, seed: u64) -> (Hash64, u64) {
    let id = t.attempt(None, seed);
    let bound = t.bind(id);
    licence(t, id, bound);
    (id, t.c.daa)
}

/// One empty block past every one of `ids`' `Final` deadlines; returns the `Final` DAA.
fn finalize_all(t: &mut Tape, ids: &[Hash64]) -> u64 {
    let last = ids.iter().map(|id| t.c.s.deadline_of(id).expect("a licensed claim owes its Final")).max().unwrap();
    t.at(last + 1, vec![]);
    for id in ids {
        assert!(matches!(t.c.s.claim(id).unwrap().phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == last + 1), "Final");
    }
    last + 1
}

fn lock_of(s: &PalwChainStateV2, seat: PalwBondKeyV2, claim: Hash64) -> PalwSlashableLockV1 {
    *s.slashable_lock(seat, claim).expect("a Valid seat holds a lock")
}

fn seats(t: &Tape) -> Vec<PalwBondKeyV2> {
    t.c.floor_seats().iter().map(|(k, _)| *k).collect()
}

/// The escaped second-clock depth the fold reads at `daa` on `s`.
fn escaped(c: &Chain, s: &PalwChainStateV2, daa: u64) -> Option<u64> {
    palw_second_clock_depth_v1(c.extras_at(daa).settled_anchor_depth, s.recent_anchor_daas(), daa, c.sp.window_court())
}

/// `dos_g2`'s false-`Valid` evidence on the V1 route (the harness folds with `palw_offence_attribution`
/// held dormant): an `ExecutorEquivocation` by the claim's executor, which convicts the execution
/// every `Valid` signer vouched for. `rcore_v02_lock_life`'s.
fn false_valid(p: &Params, claim_id: Hash64, accused: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let (executor, executor_pubkey, _) = floor_producer(p);
    let profile =
        kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
            .expect("the floor profile");
    let attestation = |root: u64| kaspa_consensus_core::palw_slash::PalwExecutionAttestationV1 {
        version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
        executor_id: h(0x1),
        job_context_hash: h(0x2),
        full_logits_trace_root: h(root),
        committed_root: h(root),
        bond_outpoint: executor.0,
        signature: Vec::new(),
    };
    let equivocation = kaspa_consensus_core::palw_carriage::PalwEquivocationCarriageV1 {
        version: kaspa_consensus_core::palw_carriage::PALW_CARRIAGE_VERSION_V1,
        accused_bond_outpoint: executor.0,
        certificate: kaspa_consensus_core::palw_slash::PalwClassContradictionCertificateV1 {
            version: kaspa_consensus_core::palw_slash::PALW_S_OBJECT_VERSION_V3,
            job_context: kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 512, 256),
            attestation_a: attestation(0xAA),
            attestation_b: attestation(0xBB),
        },
    };
    let payload = PalwPanelFalseValidEvidenceV1 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V1,
        claim_id,
        network_domain: h(NET),
        accused_seat: accused.0,
        valid_receipt: PalwSeatReceiptV2 {
            claim: claim_id,
            verdict: PalwReceiptVerdictV2::Valid,
            seat_bond: accused,
            signed_daa: 0,
            signature: Vec::new(),
        },
        executor_pubkey,
        contradiction: PalwPanelContradictionV1::ExecutorEquivocation(equivocation),
    };
    let evidence = borsh::to_vec(&payload).unwrap();
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::PanelFalseValid,
        accused,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}

/// The seat's collateral after a probe block at `daa` on `s` carrying `objects` (unchecked, not
/// committed), and whether its lock on `claim` survived it.
fn probe(
    c: &Chain,
    s: &PalwChainStateV2,
    daa: u64,
    objects: &[PalwConsensusObjectV2],
    seat: PalwBondKeyV2,
    claim: Hash64,
) -> (u64, bool) {
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let (child, _, skips) = c.try_fold(s, &x, objects, PalwBlockWorkV3::None, Hash64::default()).expect("the probe folds");
    assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
    (child.bond(&seat).expect("the seat's bond").collateral, child.slashable_lock(seat, claim).is_some())
}

/// The `SlashableLock` rewrites a block's delta carries: `(key, old expiry, new expiry)`.
fn lock_rewrites(delta: &kaspa_consensus_core::palw_state_v2::PalwStateDeltaV2) -> Vec<((PalwBondKeyV2, Hash64), u64, u64)> {
    delta
        .entries
        .iter()
        .filter_map(|e| match e {
            PalwDeltaEntryV2::SlashableLock { key, old: Some(old), new: Some(new) } => Some((*key, old.expiry_daa, new.expiry_daa)),
            _ => None,
        })
        .collect()
}

/// **The first scenario, on one ruleset**: the chain up to (not including) the crossing block at `h`.
///
/// * `a` — licensed and `Final` at ~1,124, far below lane V02: its locks `F + window_court`;
/// * `x` — the same, then convicted (V1 route) at `h − 2` on seat 0: the `Final` reversed, its record
///   marked, the other seats' locks left;
/// * thirty claims licensed and `Final` below lane V02 (so `a`'s second clock is released by `h`);
/// * `b` — licensed at 1,893 (below lane V02: `L + window_court`), `Final` at 2,014 (past it, extend-only
///   keeps the licence's long lock);
/// * `c` — licensed at ~2,103 and `Final` at ~2,224, both past lane V02: `F + 1,000` already;
/// * an empty block at `h − 1`, the crossing block's parent.
struct Scenario {
    t: Tape,
    a: (Hash64, u64),
    x: Hash64,
    settled: Vec<(Hash64, u64)>,
    b: (Hash64, u64, u64),
    c: (Hash64, u64),
}

fn scenario(p: Params, h: u64) -> Scenario {
    let mut t = Tape::new(Chain::new(p));
    let (a, _) = licensed_claim(&mut t, 0xA1);
    let (x, _) = licensed_claim(&mut t, 0xA2);
    let fa = finalize_all(&mut t, &[a, x]);
    let mut settled_ids = Vec::new();
    for i in 0..30 {
        settled_ids.push(licensed_claim(&mut t, 0xB00 + i).0);
    }
    let fs = finalize_all(&mut t, &settled_ids);
    t.at(1_890, vec![]);
    let (b, lb) = licensed_claim(&mut t, 0xB1);
    assert!(lb < V02, "b is licensed below lane V02");
    // The licence dates the lock from `max(L, H(c))` (§4-quater V5) with the long life.
    let b_long = lock_of(&t.c.s, seats(&t)[0], b).expiry_daa;
    assert!(b_long >= lb + t.c.sp.window_court(), "b's licence lock is long");
    let fb = finalize_all(&mut t, &[b]);
    assert!(fb >= V02, "b is Final past lane V02");
    t.at(2_100, vec![]);
    let (c, _) = licensed_claim(&mut t, 0xC1);
    let fc = finalize_all(&mut t, &[c]);
    assert!(fc < h - 2, "every Final of the scenario lands below the crossing");
    let seat0 = seats(&t)[0];
    t.at(h - 2, vec![false_valid(&t.c.p.clone(), x, seat0)]);
    // The V1 route (this harness folds with `palw_offence_attribution` dormant) reverses the `Final`
    // and leaves the record unmarked; the claim row, `Voided`, is what refuses its re-date. (On
    // testnet-12's own route, S-4's funnel, the record is marked too — `mark_liability_convicted`.)
    assert!(matches!(t.c.s.claim(&x).unwrap().phase, PalwClaimPhaseV2::Voided { .. }), "x's Final is reversed");
    assert_eq!(palw_lock_final_daa_retro_v1(&t.c.s, &t.c.sp, &x), None, "a reversed Final has no F to re-date from");
    t.at(h - 1, vec![]);
    Scenario { t, a: (a, fa), x, settled: settled_ids.into_iter().map(|id| (id, fs)).collect(), b: (b, b_long, fb), c: (c, fc) }
}

/// **The crossing block re-dates every long `Final` lock, once, to `max(F + 1,000, H)`** — expired at
/// the crossing itself where `F + 1,000 ≤ H`, `F + 1,000` beyond it — and nothing else: the lock of a
/// `Final` a conviction reversed, and one already dated past lane V02, keep their expiry. Below `H` the
/// armed chain is the dormant one to the root; the twin never re-dates. The rewrite is each lock's own
/// delta entry in the crossing block and in no other.
#[test]
fn the_crossing_block_redates_every_long_final_lock_to_max_f_plus_1000_h() {
    const H: u64 = 2_500;
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;
    let armed = scenario(rules(Some(H)), H);
    let dormant = scenario(rules(None), H);
    let wc = armed.t.c.sp.window_court();
    assert_eq!(armed.t.len(), dormant.t.len(), "same blocks");
    for j in 1..=armed.t.len() {
        assert_eq!(armed.t.daa_at(j), dormant.t.daa_at(j));
        assert_eq!(
            armed.t.state_at(j).state_root(),
            dormant.t.state_at(j).state_root(),
            "below H (DAA {}) the armed fold is the dormant one, to the root",
            armed.t.daa_at(j)
        );
    }
    let seats = seats(&armed.t);
    let (a, fa) = armed.a;
    let (b, b_long, fb) = armed.b;
    let (c, fc) = armed.c;
    let before = armed.t.c.s.clone();
    for seat in &seats {
        assert_eq!(lock_of(&before, *seat, a).expiry_daa, fa + wc, "a: F + window_court below the crossing");
        assert_eq!(lock_of(&before, *seat, b).expiry_daa, b_long, "b: the licence's long lock, kept at its Final");
        assert_eq!(lock_of(&before, *seat, c).expiry_daa, fc + life, "c: F + 1,000, dated past lane V02");
    }
    assert!(fa + life <= H && fb + life > H, "the premise: a's F + 1,000 is at or below H, b's beyond it");
    assert_eq!(palw_lock_final_daa_retro_v1(&before, &armed.t.c.sp, &a), Some(fa), "F is read from a's record");
    assert_eq!(palw_lock_final_daa_retro_v1(&before, &armed.t.c.sp, &b), Some(fb));
    assert_eq!(palw_lock_final_daa_retro_v1(&before, &armed.t.c.sp, &armed.x), None, "a reversed Final is not re-dated");

    let (mut armed_t, mut dormant_t) = (armed.t, dormant.t);
    assert!(palw_final_lock_life_retro_crossing_v1(&armed_t.c.sp, Some(H - 1), H), "DAA H over H - 1 is the crossing");
    armed_t.at(H, vec![]);
    dormant_t.at(H, vec![]);
    let after = armed_t.c.s.clone();
    let twin = dormant_t.c.s.clone();
    for seat in &seats {
        assert_eq!(lock_of(&after, *seat, a).expiry_daa, H, "a: max(F + 1,000, H) = H — expired at the crossing block");
        assert!(!lock_of(&after, *seat, a).is_live(H), "a's lock is dead on the DAA clock at H");
        assert_eq!(lock_of(&after, *seat, b).expiry_daa, fb + life, "b: F + 1,000 (was L + window_court)");
        assert_eq!(lock_of(&after, *seat, c).expiry_daa, fc + life, "c: untouched (already F + 1,000)");
        for (id, fs) in &armed.settled {
            assert_eq!(lock_of(&after, *seat, *id).expiry_daa, (fs + life).max(H), "a settled claim: max(F + 1,000, H)");
        }
        for (id, _) in armed.settled.iter().chain([(a, fa)].iter()) {
            let (old, new) = (lock_of(&before, *seat, *id), lock_of(&after, *seat, *id));
            assert_eq!(PalwSlashableLockV1 { expiry_daa: old.expiry_daa, ..new }, old, "only the expiry moves");
        }
        assert_eq!(lock_of(&twin, *seat, a).expiry_daa, fa + wc, "the dormant twin keeps F + window_court");
        assert_eq!(lock_of(&twin, *seat, b).expiry_daa, b_long, "and the licence's long life");
        if seat != &seats[0] {
            assert_eq!(
                lock_of(&after, *seat, armed.x).expiry_daa,
                lock_of(&before, *seat, armed.x).expiry_daa,
                "x (a Final a conviction reversed): its co-signers' locks keep their expiry"
            );
        }
    }
    // The block's delta is exactly the re-date: one entry per re-dated lock, the old lock in it.
    let crossing = armed_t.blocks.last().unwrap();
    let rewrites = lock_rewrites(&crossing.delta);
    assert_eq!(rewrites.len(), seats.len() * (1 + armed.settled.len() + 1), "a, the thirty and b — five seats each");
    for (key, old, new) in &rewrites {
        assert!(new < old, "a rewrite only shortens");
        assert_eq!(*old, lock_of(&before, key.0, key.1).expiry_daa);
    }
    assert!(lock_rewrites(&dormant_t.blocks.last().unwrap().delta).is_empty(), "the twin's block re-dates nothing");
    // Once: the next block rewrites no lock.
    armed_t.step(vec![]);
    assert!(!palw_final_lock_life_retro_crossing_v1(&armed_t.c.sp, Some(H), H + 1), "H + 1 over H is not a crossing");
    assert!(lock_rewrites(&armed_t.blocks.last().unwrap().delta).is_empty(), "the next block re-dates nothing");
    println!(
        "[f2lock] crossing at {H}: {} locks re-dated (a F={fa}: {} -> {H}; b F={fb}: {} -> {}; c untouched at {}); x (reversed) untouched",
        rewrites.len(),
        fa + wc,
        b_long,
        fb + life,
        fc + life
    );
}

/// **The room rises by what the crossing releases**, and a conviction for a fault on a re-dated lock
/// after its new expiry takes nothing, while before it (and on the dormant twin) it takes the lock.
///
/// At the crossing `a`'s locks end at `H` and its second clock is released (thirty-two licences since
/// its `Final`), so each seat's `committed` falls by exactly `a`'s lock and the 100% term of the work
/// gate (`C − R − committed − accuser`) rises by the same; nothing else of the seat's moves (`b`'s lock
/// is live to `F + 1,000`, the thirty settled locks are held by their second clock). A V1 `PanelFalseValid`
/// on `a`: below `H` it takes the seat's lock; folded IN the crossing block and after it, nothing (the
/// lock is released, lane V02's rule leaves it alone), while the twin takes it. On `b`, before its new
/// expiry, the conviction still takes the lock.
#[test]
fn the_room_rises_by_the_released_locks_and_a_conviction_past_the_new_expiry_takes_nothing() {
    const H: u64 = 2_500;
    let armed = scenario(rules(Some(H)), H);
    let dormant = scenario(rules(None), H);
    let (a, _) = armed.a;
    let (b, _, fb) = armed.b;
    let seat = seats(&armed.t)[1];
    let p = armed.t.c.p.clone();
    let c = &armed.t.c;
    let before = c.s.clone();

    // Below H (the tip at H - 1, a probe block at H - 1 is not possible, so on the state one block
    // earlier): the conviction takes a's long lock.
    let earlier = armed.t.state_at(armed.t.len() - 1).clone();
    let earlier_daa = armed.t.daa_at(armed.t.len() - 1);
    assert!(earlier_daa < H - 1);
    let collateral = before.bond(&seat).unwrap().collateral;
    let (taken_below, kept_below) = probe(c, &earlier, H - 1, &[false_valid(&p, a, seat)], seat, a);
    assert!(!kept_below && (collateral - taken_below) as u128 >= lock_of(&before, seat, a).amount, "below H: a's lock is taken");

    let mut armed_t = armed.t;
    let mut dormant_t = dormant.t;
    // Folded IN the crossing block: the re-date runs before the objects, so the conviction meets a
    // released lock.
    let (in_crossing, kept_in_crossing) = probe(&armed_t.c, &before, H, &[false_valid(&p, a, seat)], seat, a);
    assert_eq!(in_crossing, collateral, "in the crossing block: nothing is taken from a's released lock");
    assert!(kept_in_crossing, "and its row is left to prune with its record");

    armed_t.at(H, vec![]);
    dormant_t.at(H, vec![]);
    let after = armed_t.c.s.clone();
    let raw = armed_t.c.extras_at(H).settled_anchor_depth;
    let sp = armed_t.c.sp.clone();
    let lock_a = lock_of(&after, seat, a);
    for (claim, lock) in after.slashable_locks_of(&seat) {
        let was = palw_lock_is_committed_v1(
            &before,
            &claim.1,
            &lock_of(&before, seat, claim.1),
            H,
            escaped(&armed_t.c, &before, H),
            sp.window_court(),
        );
        let is = palw_lock_is_committed_v1(&after, &claim.1, lock, H, escaped(&armed_t.c, &after, H), sp.window_court());
        assert_eq!((was, is), (true, claim.1 != a), "only a's lock stops being committed at the crossing ({:?})", claim.1);
    }
    let committed_before = palw_bond_committed_raw_v1(&before, &sp, &seat, H, raw);
    let committed_after = palw_bond_committed_raw_v1(&after, &sp, &seat, H, raw);
    assert_eq!(committed_before - committed_after, lock_a.amount, "committed falls by exactly a's lock");
    let posted = after.bond(&seat).unwrap().collateral as u128;
    let term2 = |s: &PalwChainStateV2, committed: u128| {
        posted
            .saturating_sub(palw_bond_accuser_reserve_v1(&sp, H))
            .saturating_sub(committed)
            .saturating_sub(palw_accuser_exposure_v1(s, &seat))
    };
    assert_eq!(term2(&after, committed_after) - term2(&before, committed_before), lock_a.amount, "the 100% term rises by it");
    let room_before = palw_rcore_gate_room_v1(&before, &sp, &seat, H, raw, PalwRcoreGateV1::Work);
    let room_after = palw_rcore_gate_room_v1(&after, &sp, &seat, H, raw, PalwRcoreGateV1::Work);
    assert!(
        room_after >= room_before && room_after - room_before <= lock_a.amount,
        "the work room never falls, and rises by at most it"
    );
    let twin_committed = palw_bond_committed_raw_v1(&dormant_t.c.s, &dormant_t.c.sp, &seat, H, raw);
    assert_eq!(twin_committed, committed_before, "the twin keeps it committed");

    // Past the crossing: a's released lock is not taken on the armed chain, and is on the twin.
    let probe_daa = H + 1;
    assert!(!palw_false_valid_lock_slashable_v1(&after, &sp, &a, &lock_a, probe_daa, escaped(&armed_t.c, &after, probe_daa)));
    let (kept_collateral, kept) = probe(&armed_t.c, &after, probe_daa, &[false_valid(&p, a, seat)], seat, a);
    assert_eq!(kept_collateral, collateral, "past the new expiry: nothing is taken");
    assert!(kept, "and the released lock row is left alone");
    let (twin_after, twin_kept) = probe(&dormant_t.c, &dormant_t.c.s, probe_daa, &[false_valid(&p, a, seat)], seat, a);
    assert!(!twin_kept && (collateral - twin_after) as u128 >= lock_a.amount, "the dormant twin still takes the long lock");
    // b's lock, re-dated to F + 1,000 and still live: a conviction takes it.
    let lock_b = lock_of(&after, seat, b);
    assert_eq!(lock_b.expiry_daa, fb + PALW_FINAL_LOCK_LIFE_DAA_V1);
    let (b_after, b_kept) = probe(&armed_t.c, &after, lock_b.expiry_daa - 1, &[false_valid(&p, b, seat)], seat, b);
    assert!(!b_kept && (collateral - b_after) as u128 >= lock_b.amount, "before its new expiry b's lock is taken");
    println!(
        "[f2lock] seat room at the crossing: committed {:.2} -> {:.2} MSK (released a's {:.2}); work room {:.2} -> {:.2}",
        msk(committed_before),
        msk(committed_after),
        msk(lock_a.amount),
        msk(room_before),
        msk(room_after)
    );
}

/// **A long lock on a claim not yet `Final` is untouched by the crossing, and dated exactly `F + 1,000`
/// at its `Final` past `H`** — where lane V02 alone (the twin) keeps the licence's `L + window_court`.
/// A claim `Final` below `H` with `F + 1,000 > H` is re-dated to `F + 1,000` at the crossing. A crossing
/// block whose DAA jumps past `H` re-dates to `max(F + 1,000, H)`, never to its own DAA.
#[test]
fn a_claim_not_final_at_the_crossing_is_untouched_then_dated_exactly_at_its_final() {
    const H: u64 = 2_010;
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;
    let run = |retro: Option<u64>| {
        let mut t = Tape::new(Chain::new(rules(retro)));
        let (a, _) = licensed_claim(&mut t, 0xD1);
        let fa = finalize_all(&mut t, &[a]);
        t.at(1_985, vec![]);
        let (d, ld) = licensed_claim(&mut t, 0xD2);
        assert!(ld < V02 && t.c.s.deadline_of(&d).unwrap() > H + 7, "d is licensed below lane V02 and owes its Final past H");
        t.at(H - 1, vec![]);
        (t, a, fa, d, ld)
    };
    let (mut armed, a, fa, d, ld) = run(Some(H));
    let (mut dormant, ..) = run(None);
    let wc = armed.c.sp.window_court();
    assert!(fa + life > H, "the premise: a's F + 1,000 lies beyond H");
    // d's licence lock: dated from `max(L, H(c))` (§4-quater V5) with the long life, below lane V02.
    let d_long = lock_of(&armed.c.s, seats(&armed)[0], d).expiry_daa;
    assert!(d_long >= ld + wc);
    let seats = seats(&armed);
    // The crossing block jumps past H.
    armed.at(H + 7, vec![]);
    dormant.at(H + 7, vec![]);
    for seat in &seats {
        assert_eq!(lock_of(&armed.c.s, *seat, a).expiry_daa, fa + life, "a: F + 1,000 — not the crossing block's DAA");
        assert_eq!(lock_of(&armed.c.s, *seat, d).expiry_daa, d_long, "d (licensed, not Final): untouched");
    }
    assert!(matches!(armed.c.s.claim(&d).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
    let fd = finalize_all(&mut armed, &[d]);
    assert_eq!(finalize_all(&mut dormant, &[d]), fd, "same progression");
    assert!(fd >= H);
    for seat in &seats {
        let lock = lock_of(&armed.c.s, *seat, d);
        assert_eq!(lock.expiry_daa, fd + life, "d's Final past H dates it exactly F + 1,000");
        assert_eq!(lock.settled_at_final, armed.c.s.settled_attempt_finals(), "and restarts its second clock at the Final");
        assert_eq!(lock_of(&dormant.c.s, *seat, d).expiry_daa, d_long, "lane V02 alone keeps the licence's long lock");
    }
    // A claim licensed past H: extend-only and exact agree, on both chains.
    let (e, _) = licensed_claim(&mut armed, 0xD3);
    let fe = finalize_all(&mut armed, &[e]);
    let (e2, _) = licensed_claim(&mut dormant, 0xD3);
    assert_eq!(e, e2);
    assert_eq!(finalize_all(&mut dormant, &[e]), fe);
    for seat in &seats {
        assert_eq!(lock_of(&armed.c.s, *seat, e).expiry_daa, fe + life);
        assert_eq!(lock_of(&dormant.c.s, *seat, e).expiry_daa, fe + life);
    }
    println!(
        "[f2lock] H={H}, crossing at {}: a F={fa} -> {}; d licensed {ld} (lock {d_long}) untouched, Final {fd} -> {} (twin keeps {d_long})",
        H + 7,
        fa + life,
        fd + life,
    );
}

/// **A claim that goes `Final` IN the crossing block** (the block jumps past its deadline): the sweep
/// finalizes it at `F = the crossing block's DAA ≥ H` and dates its long licence lock exactly `F + 1,000`
/// (part 2), and the re-date that follows the sweeps leaves it there, while it re-dates the older `Final`
/// beside it — one block, both halves, the same answer on the twin-free path the pre-object base takes.
#[test]
fn a_claim_final_in_the_crossing_block_is_dated_once_exactly() {
    const H: u64 = 2_010;
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;
    let mut t = Tape::new(Chain::new(rules(Some(H))));
    let (a, _) = licensed_claim(&mut t, 0xE1);
    let fa = finalize_all(&mut t, &[a]);
    t.at(1_985, vec![]);
    let (d, _) = licensed_claim(&mut t, 0xE2);
    let seats = seats(&t);
    let d_long = lock_of(&t.c.s, seats[0], d).expiry_daa;
    let deadline = t.c.s.deadline_of(&d).expect("d owes its Final");
    assert!(deadline >= H, "d's Final lands past H");
    t.at(H - 1, vec![]);
    let parent = t.c.s.clone();
    // The crossing block jumps past d's deadline: the sweep finalizes d in it.
    let crossing = deadline + 1;
    t.at(crossing, vec![]);
    assert!(matches!(t.c.s.claim(&d).unwrap().phase, PalwClaimPhaseV2::Final { final_daa } if final_daa == crossing));
    for seat in &seats {
        assert_eq!(lock_of(&parent, *seat, d).expiry_daa, d_long, "d's licence lock is long below the crossing");
        assert_eq!(lock_of(&t.c.s, *seat, d).expiry_daa, crossing + life, "d: Final in the crossing block, F + 1,000 exactly");
        assert_eq!(lock_of(&t.c.s, *seat, a).expiry_daa, (fa + life).max(H), "a: re-dated by the same block");
    }
    // The pre-object base the processor draws the crossing block's panels on carries the same locks.
    let x = ctx(0xCA_0000 + crossing, crossing, crossing, 0);
    let f = flags(&t.c.p, crossing);
    let base = kaspa_consensus_core::palw_state_v2::palw_v2_pre_object_base_v1(
        &parent,
        &t.c.sp,
        &x,
        f.unavailable_abstains,
        f.capability_bound,
        f.uncertified_weightless,
        f.da_court,
        &t.c.extras_at(crossing),
    )
    .expect("the pre-object base");
    for seat in &seats {
        assert_eq!(lock_of(&base, *seat, d), lock_of(&t.c.s, *seat, d), "the draw's base and the fold agree on d");
        assert_eq!(lock_of(&base, *seat, a), lock_of(&t.c.s, *seat, a), "and on a");
    }
    t.revert_to_base_and_reapply();
}

/// **A reorg across `H` restores the old expiries; a restart and an IBD reproduce the crossing to the
/// delta and the root.** The recorded run is reverted block by block to its base (every tip on the way
/// the recorded state, and loadable), re-applied, folded again from its base (IBD) and restarted from a
/// loaded carriage below, at and past the crossing. A fork below `H` that never crosses holds the long
/// locks; a fork that crosses at another DAA re-dates them to the same expiries — `H`-keyed, not
/// block-keyed — and the reorg between the two chains goes through the recorded states both ways.
#[test]
fn a_reorg_across_h_restores_the_old_expiries_and_restart_and_ibd_reproduce_the_crossing() {
    const H: u64 = 2_500;
    let s = scenario(rules(Some(H)), H);
    let (a, fa) = s.a;
    let (b, _, fb) = s.b;
    let mut t = s.t;
    let wc = t.c.sp.window_court();
    let below = t.len();
    t.at(H, vec![]);
    t.step(vec![]);
    let seat = seats(&t)[2];
    assert_eq!(lock_of(&t.c.s, seat, a).expiry_daa, H);
    t.revert_to_base_and_reapply();
    t.ibd_from(t.base.clone());
    for j in [below - 1, below, below + 1] {
        t.restart_at(j);
    }
    // Reverting the crossing block restores the long locks exactly.
    let reverted = revert_delta_v2(&t.state_at(below + 1).clone(), &t.blocks[below].delta, &t.c.sp).expect("the crossing reverts");
    assert_eq!(&reverted, t.state_at(below), "the crossing reverts to its parent");
    assert_eq!(lock_of(&reverted, seat, a).expiry_daa, fa + wc, "a's long lock is back");
    // A fork below H that never crosses (it skips x's conviction): the long locks stay.
    let mut stay = t.fork(below - 2);
    stay.at(H - 1, vec![]);
    assert_eq!(lock_of(&stay.c.s, seat, a).expiry_daa, fa + wc, "a chain that has not crossed holds F + window_court");
    t.reorg_to(below - 2, &stay);
    // A fork that crosses at another DAA: the same expiries.
    let mut other = t.fork(below);
    other.at(H + 11, vec![]);
    assert_eq!(lock_of(&other.c.s, seat, a).expiry_daa, H, "H-keyed: the crossing block's own DAA does not matter");
    assert_eq!(lock_of(&other.c.s, seat, b).expiry_daa, fb + PALW_FINAL_LOCK_LIFE_DAA_V1);
    assert_eq!(lock_rewrites(&other.blocks[0].delta), lock_rewrites(&t.blocks[below].delta), "the same re-date, lock for lock");
    t.reorg_to(below, &other);
    other.revert_to_base_and_reapply();
    println!("[f2lock] reorg/restart/IBD across H={H}: crossing block {} of {}, checked", below + 1, t.len());
}

/// **Pure: the per-lock rule.** `min(expiry, max(F + life, H))` for an honest `Final` with no DA history
/// and a lock at or below its record; the lock's own expiry for a lock a DA re-key raised past its
/// record, and for every non-`Final` claim; never later than the lock.
#[test]
fn the_per_lock_rule_only_ever_shortens_and_skips_raised_locks() {
    const H: u64 = 2_500;
    let s = scenario(rules(Some(H)), H);
    let (a, fa) = s.a;
    let t = s.t;
    let sp = &t.c.sp;
    let wc = sp.window_court();
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;
    let seat = seats(&t)[3];
    let lock = lock_of(&t.c.s, seat, a);
    assert_eq!(palw_final_lock_retro_expiry_v1(&t.c.s, sp, &a, &lock, H, life), H);
    assert_eq!(palw_final_lock_retro_expiry_v1(&t.c.s, sp, &a, &lock, 1_000, life), fa + life, "H below F + 1,000");
    let raised = PalwSlashableLockV1 { expiry_daa: fa + wc + 1, ..lock };
    assert_eq!(palw_final_lock_retro_expiry_v1(&t.c.s, sp, &a, &raised, H, life), fa + wc + 1, "a DA-raised lock is kept");
    let short = PalwSlashableLockV1 { expiry_daa: H - 1, ..lock };
    assert_eq!(palw_final_lock_retro_expiry_v1(&t.c.s, sp, &a, &short, H, life), H - 1, "never later than the lock");
    let unknown = h(0xDEAD);
    assert_eq!(palw_lock_final_daa_retro_v1(&t.c.s, sp, &unknown), None, "no record, no F");
    assert_eq!(palw_final_lock_retro_expiry_v1(&t.c.s, sp, &unknown, &lock, H, life), lock.expiry_daa);
    // The crossing predicate.
    assert!(!palw_final_lock_life_retro_crossing_v1(&bundle(&rules(None)).state, Some(0), u64::MAX), "dormant: never");
    let armed = bundle(&rules(Some(H))).state;
    assert!(palw_final_lock_life_retro_crossing_v1(&armed, None, H), "over the genesis point");
    assert!(palw_final_lock_life_retro_crossing_v1(&armed, Some(0), H + 50));
    assert!(!palw_final_lock_life_retro_crossing_v1(&armed, Some(H - 1), H - 1), "below H");
    assert!(!palw_final_lock_life_retro_crossing_v1(&armed, Some(H), H), "a block at H over a parent at H is not the crossing");
}

/// **A claim with data-availability history keeps its locks — at the crossing and at a `Final` past
/// `H`.** DA-5's re-keys (`da_rekey_v1`) hold every lock of a claim behind `deadline + window_challenge`
/// of each session opened on it, raising only, and a closed session's end is not in the rooted state —
/// so the rule reads the claim's DA record (DA-2: from its first session until the claim row retires)
/// and leaves such a claim's locks as they are: `b` (a `Final` whose non-seat session closed at 2,300)
/// keeps its long lock through the crossing while `a` beside it is re-dated; `d` (licensed below lane
/// V02, a session closed before its `Final`) keeps the extend-only date at its `Final` past `H`. The DA
/// record is written through the carriage as a closed session leaves it (no open session, the counts
/// and the last close).
#[test]
fn a_claim_with_da_history_keeps_its_locks_at_the_crossing_and_at_its_final() {
    use kaspa_consensus_core::palw_da_rcore_v1::PalwDaClaimV1;
    let da_record = |closed: u64| PalwDaClaimV1 { opened_non_seat_total: 1, last_closed_daa: Some(closed), ..Default::default() };
    // At the crossing.
    const H: u64 = 2_500;
    let s = scenario(rules(Some(H)), H);
    let (a, _) = s.a;
    let (b, b_long, _) = s.b;
    let seats = seats(&s.t);
    let with_da = edited(&s.t.c.sp, &s.t.c.s, |carriage| {
        carriage.da_claims.insert(b, da_record(2_300));
    });
    assert_eq!(palw_lock_final_daa_retro_v1(&with_da, &s.t.c.sp, &b), None, "a claim with DA history has no F to re-date from");
    let mut c = s.t.chain_on(with_da, s.t.c.daa);
    c.step_at(H, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    for seat in &seats {
        assert_eq!(lock_of(&c.s, *seat, b).expiry_daa, b_long, "b (DA history): untouched by the crossing");
        assert_eq!(lock_of(&c.s, *seat, a).expiry_daa, H, "a beside it is re-dated");
    }
    // At a Final past H.
    const H2: u64 = 2_010;
    let mut t = Tape::new(Chain::new(rules(Some(H2))));
    t.at(1_985, vec![]);
    let (d, _) = licensed_claim(&mut t, 0xD2);
    let d_long = lock_of(&t.c.s, seats[0], d).expiry_daa;
    t.at(1_995, vec![]);
    let with_da = edited(&t.c.sp, &t.c.s, |carriage| {
        carriage.da_claims.insert(d, da_record(1_995));
    });
    let mut c = t.chain_on(with_da, t.c.daa);
    let deadline = c.s.deadline_of(&d).expect("d owes its Final");
    assert!(deadline >= H2);
    c.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(c.claim(&d).phase, PalwClaimPhaseV2::Final { .. }));
    for seat in &seats {
        assert_eq!(lock_of(&c.s, *seat, d).expiry_daa, d_long, "d (DA history): lane V02's extend-only date at its Final");
    }
}

// ---------------------------------------------------------------------------------------------
// The crossing block on a state of testnet-12's size (the auditor's DAA-816 counts carried to ~1,300)
// ---------------------------------------------------------------------------------------------

/// The fleet's ruleset — the DAA-750 release (lane V02's two lock fences at 750) — with lane F2-lock at
/// `retro`.
fn fleet(retro: Option<u64>) -> Params {
    let mut p = kaspa_consensus_core::config::params::palw_t12_shipped_params();
    p.palw_final_lock_life_retro = retro.map(ForkActivation::new);
    p.sync_palw_final_lock_life_retro();
    p.validate_palw_v2().expect("a runnable ruleset");
    p
}

/// One lock's amount on the live chain (~212 MSK a seat per floor panel, the auditor's figure).
const LOCK_SOMPI: u128 = 212 * 100_000_000;

/// **testnet-12's genesis state carrying the post-`Final` obligations of `pre` claims `Final` below
/// DAA 750 (their locks `F + window_court`) and `post` claims `Final` in [750, 1,290] (their locks
/// `F + 1,000`)**, written through the carriage as a `Final` leaves them once its claim row has retired
/// (the record and one lock per signer), five signers a claim — the heaviest seat on every claim, four
/// others in rotation — and the second clock long released (6,000 anchors settled, the last at 1,290).
/// Returns the state, the heaviest seat and each claim's `(id, F, pre)`.
fn fleet_state(p: &Params, pre: u64, post: u64) -> (PalwChainStateV2, PalwBondKeyV2, Vec<(Hash64, u64, bool)>) {
    use kaspa_consensus_core::palw_panel_var_v1::PalwPanelLiabilityRecordV1;
    use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
    let sp = bundle(p).state.clone();
    let wc = sp.window_court();
    let producer = floor_producer(p).0;
    let pool: Vec<PalwBondKeyV2> = genesis_bonds(p).into_iter().map(|(k, _, _)| k).filter(|k| *k != producer).collect();
    assert_eq!(pool.len(), 7, "testnet-12's eight genesis bonds, the producer aside");
    let (floor, _, _, _) = genesis_classes(p)[0];
    let claims: Vec<(Hash64, u64, bool)> = (0..pre)
        .map(|i| (h(0xF2_0000_0000 + i), 200 + i * 550 / pre, true))
        .chain((0..post).map(|i| (h(0xF2_1000_0000 + i), 750 + i * 540 / post, false)))
        .collect();
    let s = edited(&sp, &genesis_state(p), |carriage| {
        carriage.settled_attempt_finals = 6_000;
        carriage.recent_anchor_daas = vec![1_280, 1_285, 1_290];
        for (i, (claim, f, is_pre)) in claims.iter().enumerate() {
            let signers: Vec<PalwBondKeyV2> = std::iter::once(pool[0]).chain((0..4).map(|k| pool[1 + (i + k) % 6])).collect();
            let expiry = if *is_pre { f + wc } else { f + PALW_FINAL_LOCK_LIFE_DAA_V1 };
            for seat in &signers {
                carriage.slashable_locks.insert(
                    (*seat, *claim),
                    PalwSlashableLockV1 {
                        claim: *claim,
                        amount: LOCK_SOMPI,
                        expiry_daa: expiry,
                        settled_at_final: 2 * f,
                        attested: PalwSegmentMaskV2::NONE,
                        segments: 0,
                    },
                );
            }
            carriage.panel_liabilities.insert(
                *claim,
                PalwPanelLiabilityRecordV1 {
                    claim_id: *claim,
                    work_id: *claim,
                    class_id: floor,
                    execution_root: h(0xE0_0000_0000 + i as u64),
                    output_root: h(0xE1_0000_0000 + i as u64),
                    executor_bond: producer,
                    voided_daa: None,
                    void_reason: None,
                    valid_signers: signers.iter().map(|seat| (seat.0, *claim)).collect(),
                    locked_sompi: LOCK_SOMPI * signers.len() as u128,
                    expiry_daa: f + wc,
                    settled_at_final: 2 * f,
                    job_identity: Hash64::default(),
                    free_prompt: false,
                    trace_root: Hash64::default(),
                    segment_count: 0,
                    licence_door: None,
                    basis_k: 0,
                    g_res_sompi: 0,
                    escrowed_reward: 0,
                },
            );
        }
    });
    (s, pool[0], claims)
}

/// **The crossing block on a state of testnet-12's size: every long lock re-dated, the heaviest seat's
/// room rising by exactly what it releases, and the block's cost.** 2,400 claims `Final` below DAA 750
/// and 1,100 in [750, 1,290] (17,500 locks, 3,500 of them on the heaviest seat — ~742k MSK, past the
/// point where the 100% term `C − R − committed` binds its room, as on the fleet near DAA ~1,350); the
/// fence at 1,300. The crossing re-dates the 12,000 long locks: those with `F + 1,000 ≤ 1,300` end at
/// the crossing (the seat's room rises by exactly their amount), the rest at `F + 1,000` (released as
/// the DAA passes it, by 1,749 — not 3,749). The twin (the fleet's release) moves nothing. Timed: the
/// crossing block's fold and its pre-object base, armed and dormant, and the delta's size.
#[test]
fn the_crossing_block_on_a_fleet_sized_state_redates_thousands_and_stays_cheap() {
    const H: u64 = 1_300;
    let (pre, post) = (2_400u64, 1_100u64);
    let armed_p = fleet(Some(H));
    let twin_p = fleet(None);
    let (s0, heavy, claims) = fleet_state(&armed_p, pre, post);
    let mut c = Chain::new(armed_p.clone());
    c.s = s0;
    c.step_at(H - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let parent = c.s.clone();
    assert_eq!(parent.slashable_locks_of(&heavy).count() as u64, pre + post);
    let twin_sp = bundle(&twin_p).state.clone();
    let twin = Chain { p: twin_p.clone(), sp: twin_sp.clone(), s: parent.clone(), daa: H - 1, room: false, attribution: false };

    let x = ctx(0xCA_0000 + H, H, H, 0);
    let time = |chain: &Chain| {
        let mut best = std::time::Duration::MAX;
        let mut out = None;
        for _ in 0..3 {
            let t0 = std::time::Instant::now();
            let folded = chain.try_fold(&parent, &x, &[], PalwBlockWorkV3::None, Hash64::default()).expect("the crossing block folds");
            best = best.min(t0.elapsed());
            out = Some(folded);
        }
        (best, out.unwrap())
    };
    let base_time = |chain: &Chain| {
        let f = flags(&chain.p, H);
        let e = chain.extras_at(H);
        let mut best = std::time::Duration::MAX;
        for _ in 0..3 {
            let t0 = std::time::Instant::now();
            kaspa_consensus_core::palw_state_v2::palw_v2_pre_object_base_v1(
                &parent,
                &chain.sp,
                &x,
                f.unavailable_abstains,
                f.capability_bound,
                f.uncertified_weightless,
                f.da_court,
                &e,
            )
            .expect("the pre-object base");
            best = best.min(t0.elapsed());
        }
        best
    };
    let (armed_fold, (child, delta, skips)) = time(&c);
    let (twin_fold, (twin_child, twin_delta, _)) = time(&twin);
    let (armed_base, twin_base) = (base_time(&c), base_time(&twin));
    assert!(skips.is_empty());
    let rewrites = lock_rewrites(&delta);
    assert_eq!(rewrites.len() as u64, 5 * pre, "every long lock, and only those, is re-dated");
    assert!(lock_rewrites(&twin_delta).is_empty(), "the fleet's release re-dates nothing");
    assert_eq!(twin_child.slashable_locks_of(&heavy).count(), child.slashable_locks_of(&heavy).count(), "no lock is removed");
    let wc = c.sp.window_court();
    let life = PALW_FINAL_LOCK_LIFE_DAA_V1;
    let mut released = 0u128;
    for (claim, f, is_pre) in &claims {
        let lock = lock_of(&child, heavy, *claim);
        let want = if *is_pre { (f + life).max(H) } else { f + life };
        assert_eq!(lock.expiry_daa, want, "claim F={f} (pre {is_pre})");
        assert_eq!(lock_of(&twin_child, heavy, *claim).expiry_daa, if *is_pre { f + wc } else { f + life });
        if *is_pre && f + life <= H {
            released += LOCK_SOMPI;
        }
    }
    // The heaviest seat's room: bound by the 100% term, which rises by exactly what the crossing released.
    let raw = c.extras_at(H).settled_anchor_depth;
    let room = |s: &PalwChainStateV2, sp: &PalwStateParamsV2| palw_rcore_gate_room_v1(s, sp, &heavy, H, raw, PalwRcoreGateV1::Work);
    let posted = parent.bond(&heavy).unwrap().collateral as u128;
    let term2 = |s: &PalwChainStateV2, sp: &PalwStateParamsV2| {
        posted
            .saturating_sub(palw_bond_accuser_reserve_v1(sp, H))
            .saturating_sub(palw_bond_committed_raw_v1(s, sp, &heavy, H, raw))
            .saturating_sub(palw_accuser_exposure_v1(s, &heavy))
    };
    let (room_before, room_after, room_twin) = (room(&parent, &c.sp), room(&child, &c.sp), room(&twin_child, &twin_sp));
    assert_eq!(room_before, term2(&parent, &c.sp), "before the crossing the 100% term binds the heavy seat");
    assert_eq!(room_after, term2(&child, &c.sp), "and after it");
    assert!(released > 0);
    assert_eq!(room_after - room_before, released, "the room rises by exactly the released locks");
    assert_eq!(room_twin, room_before, "the fleet's release frees nothing at 1,300");
    let t0 = std::time::Instant::now();
    let delta_bytes = borsh::to_vec(&delta).expect("the delta encodes").len();
    let delta_encode = t0.elapsed();
    let twin_delta_bytes = borsh::to_vec(&twin_delta).expect("the delta encodes").len();
    let t0 = std::time::Instant::now();
    let root = child.state_root();
    let root_time = t0.elapsed();
    assert_ne!(root, twin_child.state_root(), "the re-date is in the committed root");
    let t0 = std::time::Instant::now();
    assert_eq!(revert_delta_v2(&child, &delta, &c.sp).expect("the crossing reverts"), parent, "a reorg across H restores it");
    let revert_time = t0.elapsed();
    println!(
        "[f2lock/fleet] {} locks ({} records); crossing at {H} re-dates {} (heavy seat: {} locks, room {:.0} -> {:.0} MSK, +{:.0} released now)",
        5 * (pre + post),
        claims.len(),
        rewrites.len(),
        pre + post,
        msk(room_before),
        msk(room_after),
        msk(released)
    );
    println!(
        "[f2lock/fleet] crossing block fold: armed {:?} vs dormant {:?}; pre-object base: armed {:?} vs dormant {:?}; delta {} B vs {} B \
         (encoded in {:?}); state root {:?}; revert {:?}",
        armed_fold, twin_fold, armed_base, twin_base, delta_bytes, twin_delta_bytes, delta_encode, root_time, revert_time
    );
    assert!(armed_fold < std::time::Duration::from_secs(5), "the crossing block's fold stays cheap even unoptimised");
}
