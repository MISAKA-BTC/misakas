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
//! Run: cargo test -p kaspa-consensus-core --test rcore_v02_lock_life

#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_var_v1::PalwSlashableLockV1;
use kaspa_consensus_core::palw_state_v2::PALW_FINAL_LOCK_LIFE_DAA_V1;
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
    println!("[v02life] conviction at F+{}: seat lock {:.2} MSK, collateral taken {:.2} MSK", PALW_FINAL_LOCK_LIFE_DAA_V1 / 2, msk(lock.amount), msk(taken));
}
