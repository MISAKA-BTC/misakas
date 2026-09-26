//! **Lane V02 (post-launch, 2026-09-26) on testnet-12's own fold: the post-`Final` honest lock life
//! is `window_court` below `Params::palw_final_lock_life` and `PALW_FINAL_LOCK_LIFE_DAA_V1` (1,000)
//! past it.**
//!
//! The 2026-09-25 sweep's V02 (HIGH), the user's decision 2026-09-26 "both": after `Final` an honest
//! `Valid` seat's lock is re-dated by `persist_panel_liability` to `F + window_court` (3,000 on
//! testnet-12), so the seat's capital sits behind the 500‰ work ceiling — and its withdrawal gate,
//! its duty backing and `slashable_available` all read `is_live` — for 3,000 DAA. Past the fence the
//! lock is stamped with 1,000 instead, so the capital returns ~3× sooner. Only the LOCK's `expiry_daa`
//! moves: the liability record, the vesting rows that hold the executor's escrow, the court/DA windows
//! and the pruning slack keep `window_court`, so a conviction still fires through `F + window_court`.
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves and is
//! checked by [`Chain::step`] (the delta re-applies and reverts, the carriage reloads).
//!
//! * a **pure** check of the life arithmetic and its deterministic keying (`final_lock_life_at`, and
//!   `is_live` reading the STORED expiry whichever side of the fence);
//! * a **transition crossing** on a low fence: the same floor claim reaches `Final` with a lock that
//!   expires 2,000 DAA earlier on the armed build than on the launch build, freeing the seat's
//!   capital that much sooner;
//! * a **conviction within `F + 1,000`** still slashes the resolved lock, exactly as the launch build.
//!
//! And the user's 2026-09-26 decision on the review's double-commit (the lock's `is_live` ended at
//! `F + 1,000`, freeing its collateral for new work, while both conviction routes still took the ROW
//! to `F + window_court`): past the fence a seat's lock stops being SLASHABLE when it stops being
//! COMMITTED (`palw_false_valid_lock_slashable_v1` — `palw_bond_committed_v1`'s per-lock term, both
//! clocks at the escaped depth), and the rest of the conviction runs unchanged:
//!
//! * past `F + 1,000` a lock the **second clock still holds** (no licence since the `Final`) stays
//!   reserved and slashable — the rule follows commitment, not the DAA alone;
//! * **free + reserved ≤ posted at every DAA of the gap** (thirty licences after the `Final` release
//!   the second clock): the armed lock leaves the bind's reservation and the conviction's reach at the
//!   same DAA, `F + 1,000`; folded V1 convictions take it at `F + 999` and nothing from `F + 1,000`
//!   (the row left alone, the `Final` still reversed); the launch build reserves and slashes it to
//!   `F + window_court`;
//! * a **real kind-3 conviction in the gap** (testnet-12's route, `palw_offence_attribution` armed,
//!   S-4's funnel) reverses the `Final`, burns the executor's escrow `E` from its vesting row
//!   (`window_court` clock) and charges the producer S3 — while the expired seat lock and the seat's
//!   collateral are left alone; inside the life the same object takes S4.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_v02_lock_life

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_attempt_v2::{attempt_id_v2, execution_commitment_v3};
use kaspa_consensus_core::palw_base0_profile::{PALW_RC_BASE0_GEOMETRY, base0_profile_v1, rc_job_context};
use kaspa_consensus_core::palw_legs::{PALW_LEGS_OBJECT_VERSION_V1, PalwCheckpointProfileV1};
use kaspa_consensus_core::palw_offence_attribution_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V2, PalwFalseValidReceiptV1, PalwPanelFalseValidEvidenceV2, palw_false_valid_offence_id_v2,
};
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_state_chunk_map::integer_kv_state_layout_id_v1;
use kaspa_consensus_core::palw_state_v2::{
    PALW_FINAL_LOCK_LIFE_DAA_V1, PalwVoidReasonV2, palw_false_valid_lock_slashable_v1, palw_second_clock_depth_v1,
};
use kaspa_consensus_core::palw_step_leg::{
    PALW_STEP_LEG_OBJECT_VERSION_V1, PalwStepBindingV2, checkpoint_empty_root_v2, checkpoint_leg_root_v2,
    execution_commitment_root_v2, step_leg_root_v1,
};
use kaspa_consensus_core::palw_step_refute::{base0_decode_token_select_v1, base0_logits_trace_root_v1};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;

const MSK: u128 = 100_000_000;

fn msk(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

/// testnet-12 with lane V02's lock-life fence armed at `at`, re-mirrored, validated.
fn armed(at: u64) -> Params {
    let mut p = t12();
    assert_eq!(p.palw_final_lock_life, None, "testnet-12 ships the fence dormant");
    p.palw_final_lock_life = Some(ForkActivation::new(at));
    p.sync_palw_final_lock_life();
    p.validate_palw_v2().expect("the armed copy is a runnable ruleset");
    p
}

/// Licence a bound claim with the five seats' `Valid`s and run it to `Final`.
fn licence_and_finalize(c: &mut Chain, id: Hash64, bound: u64) {
    let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    c.finalize(id);
}

/// A floor claim taken to `Final` on `p`; returns the chain, the claim, the first seat and its lock.
fn floor_to_final(p: Params, seed: u64) -> (Chain, Hash64, PalwBondKeyV2, PalwSlashableLockV1) {
    let mut c = Chain::new(p);
    let id = c.floor_claim(seed);
    let bound = c.bind(id, &c.floor_seats());
    licence_and_finalize(&mut c, id, bound);
    let seat = c.floor_seats()[0].0;
    let lock = c.s.slashable_lock(seat, id).expect("a Valid seat holds a lock past the Final").clone();
    (c, id, seat, lock)
}

/// `dos_g2`'s false-`Valid` evidence on the V1 route (the harness folds with `palw_offence_attribution`
/// held dormant, `dos_l5`'s extras): an `ExecutorEquivocation` by the claim's executor, under the key
/// it registered, which convicts the execution every `Valid` signer vouched for. Copied from
/// `rcore_v02_final_lock_budget`.
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

/// **Pure: the life is `window_court` below the fence and 1,000 (2,000 shorter) from it, and `is_live`
/// reads the STORED expiry whichever side of the fence a lock is evaluated on (deterministic keying).**
#[test]
fn the_lock_life_is_window_court_below_the_fence_and_a_thousand_past_it() {
    let dormant = bundle(&t12()).state.clone();
    let wc = dormant.window_court();
    assert!(wc > PALW_FINAL_LOCK_LIFE_DAA_V1, "the premise: testnet-12's court window is longer than the short life");
    assert_eq!(dormant.final_lock_life_at(0), wc, "dormant: the pre-fence life at every DAA");
    assert_eq!(dormant.final_lock_life_at(u64::MAX), wc);
    assert_eq!(dormant.final_lock_life_from_daa(), None);

    let armed = bundle(&armed(500)).state.clone();
    assert_eq!(armed.final_lock_life_from_daa(), Some(500), "the mirror carries the height");
    assert_eq!(armed.final_lock_life_at(499), wc, "below the height: the pre-fence life, byte for byte");
    assert_eq!(armed.final_lock_life_at(500), PALW_FINAL_LOCK_LIFE_DAA_V1, "at the height: the short life");
    assert_eq!(armed.final_lock_life_at(10_000), PALW_FINAL_LOCK_LIFE_DAA_V1, "and past it");
    assert_eq!(wc - armed.final_lock_life_at(500), 2_000, "a lock dated past the fence expires 2,000 DAA earlier");

    // The read side is the stored `expiry_daa`, not the fence: a lock keeps whatever life it was
    // stamped with, read on either side. A lock stamped below the fence (long) and one stamped past it
    // (short) live out their own stored expiries.
    let long = PalwSlashableLockV1 {
        claim: h(1),
        amount: 1,
        expiry_daa: 400 + wc,
        settled_at_final: 0,
        attested: PalwSegmentMaskV2::NONE,
        segments: 0,
    };
    let short = PalwSlashableLockV1 { expiry_daa: 600 + PALW_FINAL_LOCK_LIFE_DAA_V1, ..long };
    assert!(long.is_live(400 + wc - 1) && !long.is_live(400 + wc), "the pre-fence lock lives its full window_court");
    assert!(short.is_live(600 + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1) && !short.is_live(600 + PALW_FINAL_LOCK_LIFE_DAA_V1));
}

/// **Transition crossing: the same floor claim reaches `Final` with a lock that expires 2,000 DAA
/// earlier past the fence, and the seat's capital frees that much sooner.** Same DAA progression on
/// both builds (the fence touches no deadline), so `F` is identical and the only difference is the
/// life the lock is stamped with.
#[test]
fn the_lock_expires_two_thousand_daa_earlier_past_the_fence_and_binds_resume_sooner() {
    // A low fence, so the claim's licence is past it and the lock takes the short life.
    let (dormant, _, d_seat, d_lock) = floor_to_final(t12(), 0x0101);
    let (armed, _, a_seat, a_lock) = floor_to_final(armed(1_002), 0x0101);
    assert_eq!(dormant.daa, armed.daa, "same progression, same Final DAA");
    let f = armed.daa;
    let wc = armed.sp.window_court();

    assert_eq!(d_lock.expiry_daa, f + wc, "the launch build: F + window_court");
    assert_eq!(a_lock.expiry_daa, f + PALW_FINAL_LOCK_LIFE_DAA_V1, "past the fence: F + 1,000");
    assert_eq!(d_lock.expiry_daa - a_lock.expiry_daa, 2_000, "2,000 DAA earlier past the fence");
    assert_eq!(d_lock.amount, a_lock.amount, "same lock amount — only its life moved");

    // Binds resume sooner: past F + 1,000 the armed lock is `is_live`-dead, so the seat's capital is
    // back under the 500‰ ceiling for new work; on the launch build it is still locked until F + wc.
    let probe_daa = f + PALW_FINAL_LOCK_LIFE_DAA_V1 + 1;
    assert!(a_lock.is_live(f + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1), "the armed lock is live right up to F + 1,000");
    assert!(!a_lock.is_live(probe_daa), "and dead one DAA past it");
    assert!(d_lock.is_live(probe_daa), "the launch lock is still live there");
    let armed_free = armed.s.slashable_available(&a_seat, probe_daa);
    let dormant_free = dormant.s.slashable_available(&d_seat, probe_daa);
    assert!(
        armed_free >= dormant_free + a_lock.amount,
        "the armed seat has its lock's capital back at F + 1,000 ({} ≥ {} + {})",
        msk(armed_free),
        msk(dormant_free),
        msk(a_lock.amount)
    );
    println!(
        "[v02life] F {f}: lock {:.2} MSK, launch expiry {} (F+{wc}), armed expiry {} (F+{}); free at F+1001: launch {:.2}, armed {:.2}",
        msk(a_lock.amount),
        d_lock.expiry_daa,
        a_lock.expiry_daa,
        PALW_FINAL_LOCK_LIFE_DAA_V1,
        msk(dormant_free),
        msk(armed_free),
    );
}

/// **A conviction within `F + 1,000` still slashes the resolved lock**, exactly as the launch build
/// does — the lock is live and present, and its collateral is taken and the lock consumed.
#[test]
fn a_conviction_within_f_plus_one_thousand_still_slashes() {
    let p = armed(1_002);
    let (mut c, claim, seat, lock) = floor_to_final(p.clone(), 0x0201);
    let f = c.daa;
    assert_eq!(lock.expiry_daa, f + PALW_FINAL_LOCK_LIFE_DAA_V1, "the short life is in force");

    let at = f + PALW_FINAL_LOCK_LIFE_DAA_V1 / 2; // squarely inside [F, F + 1,000)
    assert!(lock.is_live(at), "within F + 1,000 the lock is live");
    let before = c.s.bond(&seat).expect("the seat's bond").collateral;
    let conviction = false_valid(&p, claim, seat);
    c.step_at(at, &[conviction], PalwBlockWorkV3::None, Hash64::default(), 0);
    let after = c.s.bond(&seat).expect("the seat's bond").collateral;
    let taken = (before - after) as u128;
    assert!(taken >= lock.amount, "the conviction takes at least the lock ({} ≥ {})", msk(taken), msk(lock.amount));
    assert!(c.s.slashable_lock(seat, claim).is_none(), "the lock is consumed");
    println!(
        "[v02life] conviction at F+{}: seat lock {:.2} MSK, collateral taken {:.2} MSK",
        PALW_FINAL_LOCK_LIFE_DAA_V1 / 2,
        msk(lock.amount),
        msk(taken)
    );
}

// ---------------------------------------------------------------------------------------------
// The user's 2026-09-26 decision on the review's double-commit: past the fence a seat's lock stops
// being SLASHABLE when it stops being COMMITTED (`palw_false_valid_lock_slashable_v1`), so the
// collateral `slashable_available` hands to new work at `F + 1,000` is never taken a second time;
// the rest of a conviction in `[F + 1,000, F + window_court]` — the `Final` reversed, `E` burned
// from its vesting row on the unchanged `window_court` clock — runs as before.
// ---------------------------------------------------------------------------------------------

/// `n` more floor claims licensed after the chain's last `Final` — past `palw_audit_2026_09_23` each
/// licence ticks the second clock once (`license_claim`), so after testnet-12's 30
/// (`PALW_T12_SETTLED_ANCHOR_DEPTH`) a lock re-dated at that `Final` is held by its DAA clock alone —
/// then one block past every one's `Final` deadline, so no claim is left live (a live claim's lock is
/// committed whatever its clocks say). Returns the `Final` DAA of the last.
fn settle_licences_after_final(c: &mut Chain, n: u64, seed: u64) -> u64 {
    let settled_before = c.s.settled_attempt_finals();
    let mut ids = Vec::new();
    for i in 0..n {
        let id = c.floor_claim(seed + i);
        let bound = c.bind(id, &c.floor_seats());
        let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
        c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        ids.push(id);
    }
    assert_eq!(c.s.settled_attempt_finals(), settled_before + n, "each licence ticks the second clock once");
    let last = ids.iter().map(|id| c.s.deadline_of(id).expect("a licensed claim owes its Final")).max().unwrap();
    c.step_at(last + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    for id in &ids {
        assert!(matches!(c.claim(id).phase, PalwClaimPhaseV2::Final { .. }), "every settling claim is Final (terminal)");
    }
    c.daa
}

/// The raw second-clock depth the processor hands the fold at `daa`, and its escape.
fn escaped_depth(c: &Chain, daa: u64) -> Option<u64> {
    palw_second_clock_depth_v1(c.extras_at(daa).settled_anchor_depth, c.s.recent_anchor_daas(), daa, c.sp.window_court())
}

/// **Free + reserved for `seat` at `daa`, on `c`'s state.** `free` is what the bind reads for new
/// work (`palw_slashable_available_v1` at the processor's depth); `reserved` is what a conviction at
/// `daa` could still take from the seat's locks (`palw_false_valid_lock_slashable_v1` on every row).
fn free_and_reserved(c: &Chain, seat: &PalwBondKeyV2, daa: u64) -> (u128, u128) {
    let free = c.s.palw_slashable_available_v1(seat, daa, c.extras_at(daa).settled_anchor_depth, c.sp.window_court());
    let depth = escaped_depth(c, daa);
    let reserved =
        c.s.slashable_locks_of(seat)
            .filter(|((_, claim), lock)| palw_false_valid_lock_slashable_v1(&c.s, &c.sp, claim, lock, daa, depth))
            .map(|(_, lock)| lock.amount)
            .sum();
    (free, reserved)
}

/// The seat's collateral after a probe block at `daa` carrying `objects` (unchecked, not committed),
/// and whether the lock on `claim` survived it.
fn probe(c: &Chain, daa: u64, objects: &[PalwConsensusObjectV2], seat: PalwBondKeyV2, claim: Hash64) -> (u64, bool) {
    let x = ctx(0xCA_0000 + daa, daa, daa, 0);
    let (child, _, skips) = c.try_fold(&c.s, &x, objects, PalwBlockWorkV3::None, Hash64::default()).expect("the probe folds");
    assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
    (child.bond(&seat).expect("the seat's bond").collateral, child.slashable_lock(seat, claim).is_some())
}

/// **Past `F + 1,000` the rule follows COMMITMENT, not the DAA alone: a lock the second clock still
/// holds stays reserved AND slashable.** With no licence since the `Final`, `is_live_v3` keeps the
/// short lock live past its DAA expiry (fewer than 30 anchors settled since it began), so the bind
/// still reserves it — and a conviction in the gap takes it, exactly as the launch build does. No
/// collateral is both free and reserved.
#[test]
fn past_f_plus_one_thousand_a_lock_the_second_clock_still_holds_stays_reserved_and_slashable() {
    let p = armed(1_002);
    let (mut c, claim, seat, lock) = floor_to_final(p.clone(), 0x0301);
    let f = c.daa;
    let wc = c.sp.window_court();
    assert_eq!(lock.expiry_daa, f + PALW_FINAL_LOCK_LIFE_DAA_V1, "the short life is in force");

    let at = f + PALW_FINAL_LOCK_LIFE_DAA_V1 + wc / 2;
    assert!(at > f + PALW_FINAL_LOCK_LIFE_DAA_V1 && at < f + wc, "in (F + 1,000, F + window_court)");
    assert!(!lock.is_live(at), "the DAA clock alone has released it");
    assert!(
        palw_false_valid_lock_slashable_v1(&c.s, &c.sp, &claim, &lock, at, escaped_depth(&c, at)),
        "but the second clock still commits it (no licence since the Final), so it is still slashable"
    );
    let posted = c.s.bond(&seat).expect("bond").collateral as u128;
    let (free, reserved) = free_and_reserved(&c, &seat, at);
    assert!(free + reserved <= posted, "free {:.2} + reserved {:.2} ≤ posted {:.2}", msk(free), msk(reserved), msk(posted));

    let before = c.s.bond(&seat).expect("the seat's bond").collateral;
    c.step_at(at, &[false_valid(&p, claim, seat)], PalwBlockWorkV3::None, Hash64::default(), 0);
    let taken = (before - c.s.bond(&seat).expect("the seat's bond").collateral) as u128;
    assert!(taken >= lock.amount, "a still-committed lock is taken ({} ≥ {})", msk(taken), msk(lock.amount));
    assert!(c.s.slashable_lock(seat, claim).is_none(), "the lock is consumed");
    println!(
        "[v02life] F+{} with the second clock holding: free {:.2} + reserved {:.2} ≤ posted {:.2}; conviction took {:.2} MSK",
        at - f,
        msk(free),
        msk(reserved),
        msk(posted),
        msk(taken)
    );
}

/// **The review's double-commit, closed: free + reserved ≤ posted at EVERY DAA of the gap.** Thirty
/// licences after the `Final` release the second clock, so on the armed build the short lock's
/// collateral really is free for new work from `F + 1,000` — and from that same DAA no conviction
/// takes it (checked at every DAA through `F + window_court` and one epoch past, over all the seat's
/// locks). Folded convictions on the V1 route agree with the predicate at the edges: at `F + 999` the
/// lock is taken; from `F + 1,000` on nothing is taken and the lock row is left alone, while the
/// conviction still records and reverses the `Final` (#8). The launch build holds the lock reserved
/// and slashable to `F + window_court`, so it satisfies the same bound (below the fence the rule is
/// the row, byte for byte).
#[test]
fn free_plus_reserved_never_exceeds_posted_at_any_daa_in_the_gap() {
    let p_armed = armed(1_002);
    let (mut armed_c, a_claim, a_seat, a_lock) = floor_to_final(p_armed.clone(), 0x0401);
    let (mut dormant_c, d_claim, d_seat, d_lock) = floor_to_final(t12(), 0x0401);
    let f = armed_c.daa;
    assert_eq!(dormant_c.daa, f, "same progression, same Final DAA (the fence touches no deadline)");
    let wc = armed_c.sp.window_court();
    let settled_to = settle_licences_after_final(&mut armed_c, 30, 0x0410);
    assert_eq!(settle_licences_after_final(&mut dormant_c, 30, 0x0410), settled_to, "same progression");
    assert!(settled_to < f + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1, "the settling ends before the gap");
    let a_lock_now = *armed_c.s.slashable_lock(a_seat, a_claim).expect("the target lock is untouched by the settling");
    assert_eq!(a_lock_now, a_lock, "the settling does not re-date the target lock");

    let posted = armed_c.s.bond(&a_seat).expect("bond").collateral as u128;
    assert_eq!(posted, dormant_c.s.bond(&d_seat).expect("bond").collateral as u128, "same posted collateral");
    let epoch = armed_c.sp.epoch_length();

    // Every DAA from just inside the life to one epoch past F + window_court.
    let mut armed_freed_at = None;
    for daa in (f + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1)..=(f + wc + epoch) {
        let (free, reserved) = free_and_reserved(&armed_c, &a_seat, daa);
        assert!(
            free + reserved <= posted,
            "armed at F+{}: free {:.2} + reserved {:.2} ≤ posted {:.2}",
            daa - f,
            msk(free),
            msk(reserved),
            msk(posted)
        );
        let slashable =
            palw_false_valid_lock_slashable_v1(&armed_c.s, &armed_c.sp, &a_claim, &a_lock, daa, escaped_depth(&armed_c, daa));
        assert_eq!(slashable, a_lock.is_live(daa), "armed at F+{}: slashable exactly while its DAA clock runs", daa - f);
        if !slashable && armed_freed_at.is_none() {
            armed_freed_at = Some(daa);
        }
    }
    assert_eq!(
        armed_freed_at,
        Some(f + PALW_FINAL_LOCK_LIFE_DAA_V1),
        "released for new work and for convictions together, at F + 1,000"
    );
    // The launch build: the lock is reserved AND slashable through the whole window.
    for daa in (f + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1)..(f + wc) {
        let (free, reserved) = free_and_reserved(&dormant_c, &d_seat, daa);
        assert!(free + reserved <= posted, "launch at F+{}: free {:.2} + reserved {:.2} ≤ posted", daa - f, msk(free), msk(reserved));
        assert!(d_lock.is_live(daa), "launch: the lock lives F + window_court");
    }

    // The fold agrees at the edges (V1 route: `dos_l5`'s extras).
    let a_before = armed_c.s.bond(&a_seat).expect("bond").collateral;
    let (after, kept) =
        probe(&armed_c, f + PALW_FINAL_LOCK_LIFE_DAA_V1 - 1, &[false_valid(&p_armed, a_claim, a_seat)], a_seat, a_claim);
    assert!((a_before - after) as u128 >= a_lock.amount && !kept, "armed at F+999: the live lock is taken");
    for daa in
        [f + PALW_FINAL_LOCK_LIFE_DAA_V1, f + PALW_FINAL_LOCK_LIFE_DAA_V1 + 1, f + (wc + PALW_FINAL_LOCK_LIFE_DAA_V1) / 2, f + wc - 1]
    {
        let (after, kept) = probe(&armed_c, daa, &[false_valid(&p_armed, a_claim, a_seat)], a_seat, a_claim);
        assert_eq!(after, a_before, "armed at F+{}: nothing is taken from the expired lock", daa - f);
        assert!(kept, "armed at F+{}: the expired lock row is left alone (it prunes with its record)", daa - f);
    }
    let d_before = dormant_c.s.bond(&d_seat).expect("bond").collateral;
    for daa in [f + PALW_FINAL_LOCK_LIFE_DAA_V1, f + wc - 1] {
        let (after, kept) = probe(&dormant_c, daa, &[false_valid(&t12(), d_claim, d_seat)], d_seat, d_claim);
        assert!((d_before - after) as u128 >= d_lock.amount && !kept, "launch at F+{}: the lock is taken", daa - f);
    }

    // And the conviction in the gap still runs its claim leg: the Final is reversed (#8), recorded once.
    let gap = f + PALW_FINAL_LOCK_LIFE_DAA_V1 + wc / 2;
    armed_c.step_at(gap, &[false_valid(&p_armed, a_claim, a_seat)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(armed_c.claim(&a_claim).phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == gap),
        "the Final is reversed"
    );
    assert_eq!(armed_c.s.bond(&a_seat).expect("bond").collateral, a_before, "the seat pays nothing for the expired lock");
    assert_eq!(armed_c.s.slashable_lock(a_seat, a_claim).copied(), Some(a_lock), "the expired lock is left alone");
    println!(
        "[v02life] gap F+{}..=F+{}: posted {:.2} MSK, lock {:.2} MSK; armed released for new work AND convictions at F+{}; launch reserves it to F+{wc}",
        PALW_FINAL_LOCK_LIFE_DAA_V1 - 1,
        wc + epoch,
        msk(posted),
        msk(a_lock.amount),
        armed_freed_at.unwrap() - f,
    );
}

// ---- a real kind-3 conviction (the testnet-12 route: `palw_offence_attribution` armed) ----------

fn rebind(b: &mut PalwStepBindingV2) {
    let ctx_hash = b.job_context.context_hash();
    let profile_hash = b.shape_profile.shape_profile_id();
    let decode_calls = b.job_context.exact_decode_tokens.saturating_sub(1);
    let step_root = step_leg_root_v1(&ctx_hash, &profile_hash, b.step_leaf_count, &b.step_merkle_root);
    let ckpt_root = checkpoint_leg_root_v2(
        &ctx_hash,
        &b.checkpoint_profile.profile_hash(),
        &b.state_chunk_map_id,
        decode_calls,
        b.checkpoint_count,
        &b.checkpoint_merkle_root,
    );
    b.committed_execution_root =
        execution_commitment_root_v2(&ctx_hash, &b.full_logits_trace_root, &b.activation_leg_root, &ckpt_root, &step_root);
}

/// A well-formed flat base0 commitment for `job_id` (`palw_offence_attribution_t11_verdicts`'s
/// shape): it reproduces its own root, so a claim that commits that root is ITS execution — and it
/// answers another job or class than the floor claim recorded, a claim-proving `IdentityMismatch`.
fn foreign_binding(job_id: Hash64) -> PalwStepBindingV2 {
    let profile = base0_profile_v1(PALW_RC_BASE0_GEOMETRY).expect("RC profile");
    let mut job = rc_job_context(&profile, 4, 4);
    job.job_id = job_id;
    let vocab = profile.vocab_size as usize;
    let rows: Vec<Vec<i32>> =
        (0..job.exact_decode_tokens).map(|r| (0..vocab).map(|i| ((i * 31 + r as usize * 7) % 2001) as i32 - 1000).collect()).collect();
    let toks: Vec<u32> = rows.iter().map(|r| base0_decode_token_select_v1(r) as u32).collect();
    let ctx_hash = job.context_hash();
    let mut b = PalwStepBindingV2 {
        version: PALW_STEP_LEG_OBJECT_VERSION_V1,
        job_context: job.clone(),
        shape_profile: profile.clone(),
        checkpoint_profile: PalwCheckpointProfileV1 {
            version: PALW_LEGS_OBJECT_VERSION_V1,
            checkpoint_interval: 8,
            state_layout_id: integer_kv_state_layout_id_v1(),
        },
        state_chunk_map_id: profile.state_chunk_map_id,
        full_logits_trace_root: base0_logits_trace_root_v1(&job, &rows, &toks),
        activation_leg_root: h(0x7120_0A00),
        step_leaf_count: 1 << 12,
        step_merkle_root: h(0x7120_0B00),
        checkpoint_count: 0,
        checkpoint_merkle_root: checkpoint_empty_root_v2(&ctx_hash),
        committed_execution_root: Hash64::default(),
    };
    rebind(&mut b);
    b
}

/// A floor attempt by the genesis producer whose committed execution root is [`foreign_binding`]'s,
/// accepted in its own block with the carrying header's execution anchor handed to the fold as the
/// processor hands it (`extras.own_job_anchor`, a [`Tape`] block), so the claim records its
/// `job_identity`; returns the chain, the claim and the binding.
fn floor_claim_committing_a_foreign_execution(c: Chain, seed: u64) -> (Chain, Hash64, PalwStepBindingV2) {
    let (floor, _, _, _) = genesis_classes(&c.p)[0];
    let (bond, pubkey, operator) = floor_producer(&c.p);
    let pwu = c.floor_pwu(c.daa + 1);
    let pre_pow = 0x10C0 + seed;
    let (mut env, _, _) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, pre_pow);
    let anchor = floor_job_anchor(&c.p, bond, pre_pow);
    let binding = foreign_binding(anchor);
    env.attempt.execution_root = binding.committed_execution_root;
    let key = execution_commitment_v3(&env.attempt, anchor);
    let id = attempt_id_v2(&env.attempt);
    let daa = c.daa + 1;
    let mut t = Tape::new(c);
    let skips = t.block(daa, vec![], Some((env, key, anchor)), T12_BLOCK_SUBSIDY_SOMPI).expect("the attempt's block folds");
    assert!(skips.is_empty() && t.c.s.claim(&id).is_some(), "the floor attempt is accepted: {skips:?}");
    assert_eq!(t.c.s.claim(&id).unwrap().job_identity, anchor, "the claim records the job it answers");
    (t.c, id, binding)
}

/// Kind 3 (`PanelFalseValidV2`) against `seat`'s licensing `Valid` on `claim`, proving the claim false.
fn kind3_identity(claim: Hash64, seat: PalwBondKeyV2, signed_daa: u64, binding: &PalwStepBindingV2) -> PalwConsensusObjectV2 {
    let evidence = borsh::to_vec(&PalwPanelFalseValidEvidenceV2 {
        version: PALW_PANEL_FALSE_VALID_VERSION_V2,
        claim_id: claim,
        accused_seat: seat.0,
        receipt: PalwFalseValidReceiptV1::Full(valid(claim, seat, signed_daa)),
        contradiction: PalwPanelContradictionV1::IdentityMismatch { binding: binding.clone() },
        prompt_ids_opening: None,
        reporter_reveal: Vec::new(),
    })
    .unwrap();
    PalwConsensusObjectV2::ObjectiveOffence {
        kind: PalwOffenceKindV1::PanelFalseValidV2,
        accused: seat,
        evidence_id: palw_offence_evidence_digest_v1(&evidence),
        evidence,
    }
}

/// **A conviction in the gap still reverses the `Final` and burns `E` — and leaves the expired seat
/// lock alone.** On testnet-12's own route (`palw_offence_attribution` armed, kind 3 through the
/// S-4 funnel), a false `Valid` proven in `[F + 1,000, F + window_court]` after the second clock has
/// released the seat's short lock: the claim is voided `CourtFraud` at the conviction, its vesting
/// row (the executor's escrow, on the `window_court` clock) is burned whole and the producer pays
/// S3's action, the `(seat, claim)` record is written — and the seat's collateral and its lock row
/// are untouched (no S4, no reporter extraction from it). The same object folded inside the life
/// (`F + 500`) takes the lock and S4's action, as the launch build does throughout the window.
#[test]
fn a_conviction_in_the_gap_reverses_the_final_and_burns_e_and_leaves_the_expired_lock_alone() {
    let mut c = Chain::new(armed(1_002));
    c.attribution = true;
    let (mut c, claim, binding) = floor_claim_committing_a_foreign_execution(c, 0x0501);
    let bound = c.bind(claim, &c.floor_seats());
    licence_and_finalize(&mut c, claim, bound);
    let f = c.daa;
    let wc = c.sp.window_court();
    let seat = c.floor_seats()[0].0;
    let lock = *c.s.slashable_lock(seat, claim).expect("the seat's lock past the Final");
    assert_eq!(lock.expiry_daa, f + PALW_FINAL_LOCK_LIFE_DAA_V1, "the short life is in force");
    let producer = floor_producer(&c.p).0;
    settle_licences_after_final(&mut c, 30, 0x0510);
    let row = c.s.vesting_row(&claim).expect("the claim's vesting row, written at Final").clone();
    assert!(row.total_sompi_u128() > 0, "the row holds the escrow");
    let object = kind3_identity(claim, seat, bound, &binding);

    // Inside the life: the lock and S4's action are taken, and the row is burned.
    let inside = f + PALW_FINAL_LOCK_LIFE_DAA_V1 / 2;
    let seat_before = c.s.bond(&seat).expect("bond").collateral;
    let x = ctx(0xCA_0000 + inside, inside, inside, 0);
    let (child, _, _) =
        c.try_fold(&c.s, &x, std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default()).expect("kind 3 folds");
    let s4 = (seat_before - child.bond(&seat).expect("bond").collateral) as u128;
    assert!(s4 > lock.amount, "inside the life: S4 = the lock + its action ({} > {})", msk(s4), msk(lock.amount));
    assert!(child.slashable_lock(seat, claim).is_none() && child.vesting_row(&claim).is_none(), "the lock taken, the row burned");

    // In the gap: the lock is expired and released; the conviction runs without it.
    let gap = f + PALW_FINAL_LOCK_LIFE_DAA_V1 + 500;
    assert!(gap < f + wc && gap < row.expiry_daa, "in the gap, before the vesting row's window_court clock runs");
    assert!(
        !palw_false_valid_lock_slashable_v1(&c.s, &c.sp, &claim, &lock, gap, escaped_depth(&c, gap)),
        "the lock is no longer committed at the gap"
    );
    let burned_before = c.s.vesting_counters().burned;
    let producer_before = c.s.bond(&producer).expect("bond").collateral;
    c.step_at(gap, std::slice::from_ref(&object), PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(c.claim(&claim).phase, PalwClaimPhaseV2::Voided { voided_daa, reason: PalwVoidReasonV2::CourtFraud } if voided_daa == gap),
        "the Final is reversed"
    );
    assert!(c.s.vesting_row(&claim).is_none(), "E's vesting row is burned");
    assert_eq!(c.s.vesting_counters().burned - burned_before, row.total_sompi_u128(), "burned whole, never minted");
    let s3 = producer_before - c.s.bond(&producer).expect("bond").collateral;
    assert!(s3 > 0, "the producer pays S3's action");
    assert_eq!(c.s.bond(&seat).expect("bond").collateral, seat_before, "the seat pays nothing for the expired lock");
    assert_eq!(c.s.slashable_lock(seat, claim).copied(), Some(lock), "the expired lock row is left alone");
    let record = c.s.consumed_offence(&palw_false_valid_offence_id_v2(&seat.0, &claim)).expect("the (seat, claim) record").clone();
    assert_eq!((record.kind, record.claim_id), (PalwOffenceKindV1::PanelFalseValidV2, claim));
    assert_eq!(u128::from(record.amount), u128::from(s3), "the record's nominal is the producer's leg alone");
    println!(
        "[v02life] kind 3 at F+{}: Final reversed, E row {:.2} MSK burned, producer S3 {:.2} MSK, seat 0 (expired lock {:.2} MSK left); inside the life S4 {:.2} MSK",
        gap - f,
        msk(row.total_sompi_u128()),
        msk(u128::from(s3)),
        msk(lock.amount),
        msk(s4)
    );
}
