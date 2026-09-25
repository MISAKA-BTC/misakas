//! **Lane V02 on testnet-12's own fold: a resolved claim's lock leaves the 500‰ work ceiling past
//! `Params::palw_final_lock_full_collateral`, and stays slashable against the whole collateral.**
//!
//! The 2026-09-25 sweep's V02 (HIGH): after `Final` an honest `Valid` seat's lock is re-dated to
//! `F + window_court` by `persist_panel_liability` and `palw_bond_committed_v1` keeps counting it,
//! so the bind gate (`gate_room(Work)`: `500‰·C − committed`) closes on every seat once enough
//! Finals have landed, and no panel can bind. The user's option (a): past the fence those locks are
//! counted against the whole posted collateral only.
//!
//! Every block goes through `apply_palw_transition_v7` with the extras the processor resolves and is
//! checked by [`Chain::step`] (delta re-applies and reverts, the carriage reloads). Locks that a real
//! chain would accumulate over thousands of Finals are written through the carriage as one retired
//! claim's lock per seat (the load path: a lock whose claim has retired is exactly what a Final leaves
//! once its claim row is gone), beside the real locks of real Finals.
//!
//! Run: cargo test -p kaspa-consensus-core --test rcore_v02_final_lock_budget -- --nocapture

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
    PalwRcoreGateV1, PalwStateV2Error, palw_accuser_exposure_v1, palw_bond_committed_raw_v1, palw_bond_off_ceiling_raw_v1,
    palw_bond_resolved_locks_v1, palw_rcore_bind_prices_v1, palw_rcore_gate_room_of_v1, palw_rcore_gate_room_split_of_v1,
    palw_rcore_gate_room_v1, palw_second_clock_depth_v1,
};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;

const MSK: u128 = 100_000_000;

fn msk(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

/// testnet-12 with lane V02's fence armed at `at`, re-mirrored, validated.
fn armed(at: u64) -> Params {
    let mut p = t12();
    assert_eq!(p.palw_final_lock_full_collateral, None, "testnet-12 ships the fence dormant");
    p.palw_final_lock_full_collateral = Some(ForkActivation::new(at));
    p.sync_palw_final_lock_full_collateral();
    p.validate_palw_v2().expect("the armed copy is a runnable ruleset");
    p
}

/// A chain on `p` over `s` at `daa` (the same state under another ruleset).
fn chain_on(p: Params, s: PalwChainStateV2, daa: u64) -> Chain {
    let sp = bundle(&p).state.clone();
    Chain { p, sp, s, daa, room: false, attribution: false }
}

fn raw_depth(c: &Chain, daa: u64) -> Option<u64> {
    c.extras_at(daa).settled_anchor_depth
}

fn committed(c: &Chain, bond: &PalwBondKeyV2, daa: u64) -> u128 {
    palw_bond_committed_raw_v1(&c.s, &c.sp, bond, daa, raw_depth(c, daa))
}

fn work_room(c: &Chain, bond: &PalwBondKeyV2, daa: u64) -> u128 {
    palw_rcore_gate_room_v1(&c.s, &c.sp, bond, daa, raw_depth(c, daa), PalwRcoreGateV1::Work)
}

fn accuser_room(c: &Chain, bond: &PalwBondKeyV2, daa: u64) -> u128 {
    palw_rcore_gate_room_v1(&c.s, &c.sp, bond, daa, raw_depth(c, daa), PalwRcoreGateV1::Accuser)
}

fn collateral(c: &Chain, bond: &PalwBondKeyV2) -> u128 {
    c.s.bond(bond).expect("a bond").collateral as u128
}

fn ceiling(c: &Chain, bond: &PalwBondKeyV2) -> u128 {
    collateral(c, bond) * c.sp.fp_max_exposure_ratio_permille() as u128 / 1000
}

/// What a seat must have room for to bind `claim` at `daa` (the bind's `max(duty_bind, lock_2)`).
fn eligibility(c: &Chain, claim: &Hash64, daa: u64) -> u128 {
    let record = c.claim(claim);
    palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(daa), claim, &record, c.floor_seats().len(), daa).eligibility
}

fn panel_bound(c: &Chain, claim: Hash64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::PanelBound { claim, anchor: h(0xAC_0000 + c.daa), seats: seats_of(&c.floor_seats()) }
}

/// One block at `daa` on `c`'s tip, unchecked (the probe beside `step_at`).
fn probe(c: &Chain, daa: u64, objects: &[PalwConsensusObjectV2]) -> Result<PalwChainStateV2, PalwStateV2Error> {
    c.try_fold(&c.s, &ctx(0xCA_0000 + daa, daa, daa, 0), objects, PalwBlockWorkV3::None, Hash64::default()).map(|(s, _, _)| s)
}

/// **Does a `PanelBound` for `claim` bind in a block at `daa` on `c`'s tip?** Past
/// `palw_audit_2026_09_23` (testnet-12 from genesis) a panel whose seats cannot post the lock is INERT
/// (audit C-3): the block folds and the claim stays `Provisional`.
fn binds(c: &Chain, daa: u64, claim: Hash64) -> bool {
    let folded = probe(c, daa, &[panel_bound(c, claim)]).expect("an unbacked panel is inert, never the block's error");
    match folded.claim(&claim).expect("the claim").phase {
        PalwClaimPhaseV2::PanelBound { bound_daa } => {
            assert_eq!(bound_daa, daa);
            true
        }
        PalwClaimPhaseV2::Provisional => false,
        ref other => panic!("unexpected phase {other:?}"),
    }
}

/// Licence a bound claim with the five seats' `Valid`s and run it to `Final`.
fn licence_and_finalize(c: &mut Chain, id: Hash64, bound: u64) {
    let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    c.finalize(id);
}

/// `s` with one RETIRED claim's lock of `amount[i]` on each `seats[i]` (the accumulated post-Final
/// locks of many claims, written through the carriage as a Final leaves them once its row is gone):
/// its expiry `expiry_daa`, its second clock at the state's settled count.
fn with_retired_locks(sp: &PalwStateParamsV2, s: &PalwChainStateV2, claim: Hash64, locks: &[(PalwBondKeyV2, u128)], expiry_daa: u64) -> PalwChainStateV2 {
    assert!(s.claim(&claim).is_none(), "a retired claim");
    let settled = s.settled_attempt_finals();
    edited(sp, s, |carriage| {
        for (seat, amount) in locks {
            carriage.slashable_locks.insert(
                (*seat, claim),
                PalwSlashableLockV1 {
                    claim,
                    amount: *amount,
                    expiry_daa,
                    settled_at_final: settled,
                    attested: PalwSegmentMaskV2::NONE,
                    segments: 0,
                },
            );
        }
    })
}

/// `dos_g2`'s false-`Valid` evidence on the V1 route (the harness folds with `palw_offence_attribution`
/// held dormant, `dos_l5`'s extras): an `ExecutorEquivocation` by the claim's executor, under the key
/// it registered, which convicts the execution every `Valid` signer vouched for.
fn false_valid(p: &Params, claim_id: Hash64, accused: PalwBondKeyV2) -> PalwConsensusObjectV2 {
    let (executor, executor_pubkey, _) = floor_producer(p);
    let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY)
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

/// **The split room, as arithmetic** — for every `(committed, off_ceiling ≤ committed, accuser)` on a
/// grid: the work room never lets `committed + accuser` pass the collateral, is never less than the
/// unsplit room, equals it at `off_ceiling = 0`, and bounds the work in flight (`committed −
/// off_ceiling`) by the ceiling; the accuser room is the unsplit one.
#[test]
fn v02_the_split_room_keeps_the_one_invariant_and_only_widens_work() {
    use PalwRcoreGateV1::{Accuser, Work};
    let c = 1_000u64;
    for committed in (0..=1_100u128).step_by(25) {
        for off in (0..=committed).step_by(25) {
            for accuser in (0..=600u128).step_by(50) {
                let split = palw_rcore_gate_room_split_of_v1(c, 500, committed, off, accuser, Work);
                let plain = palw_rcore_gate_room_of_v1(c, 500, committed, accuser, Work);
                assert!(split >= plain, "({committed}, {off}, {accuser}): the split never narrows work");
                if split > 0 {
                    assert!(committed + accuser + split <= c as u128, "({committed}, {off}, {accuser}): the invariant");
                    assert!(committed - off + split <= 500, "({committed}, {off}, {accuser}): the ceiling bounds work in flight");
                }
                assert_eq!(
                    palw_rcore_gate_room_split_of_v1(c, 500, committed, off, accuser, Accuser),
                    palw_rcore_gate_room_of_v1(c, 500, committed, accuser, Accuser),
                    "the accuser gate does not move"
                );
                assert_eq!(palw_rcore_gate_room_split_of_v1(c, 500, committed, 0, accuser, Work), plain, "off = 0 is the shipped room");
            }
        }
    }
    // The V02 shape: locks alone past the ceiling, nothing in flight.
    assert_eq!(palw_rcore_gate_room_of_v1(1_000, 500, 600, 0, Work), 0, "below the fence: closed");
    assert_eq!(palw_rcore_gate_room_split_of_v1(1_000, 500, 600, 600, 0, Work), 400, "past it: the collateral's remainder");
    assert_eq!(palw_rcore_gate_room_split_of_v1(1_000, 500, 600, 500, 0, Work), 400, "…never more than C − committed");
    assert_eq!(palw_rcore_gate_room_split_of_v1(1_000, 500, 1_000, 900, 0, Work), 0, "a bond full of locks takes no work");
}

/// **The partition on a real Final, and its fence**: before `Final` a `Valid` seat's lock rides a live
/// claim (work: `max(duty, lock)`, nothing resolved); after it the duty is gone and the whole lock is
/// the resolved term — every lock term of `committed`, to the sompi. Below the fence the off-ceiling
/// part is 0; from the height it is the resolved term; and a lock written BELOW the height is read past
/// it exactly as one written past it.
#[test]
fn v02_a_finals_lock_is_the_resolved_term_and_the_fence_reads_it_at_the_gates_daa() {
    let first = 1_001u64;
    let fence = 1_200u64;
    let mut c = Chain::new(armed(fence));
    let id = c.floor_claim(0x0201);
    let bound = c.bind(id, &c.floor_seats());
    let seat = c.floor_seats()[0].0;
    let duty_daa = c.daa;
    let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let licensed = c.daa;
    let lock = c.s.slashable_lock(seat, id).expect("the Valid seat locked at the licence").amount;
    let duty_before = c.reserved(&seat);
    let depth = |c: &Chain, daa| palw_second_clock_depth_v1(raw_depth(c, daa), c.s.recent_anchor_daas(), daa, c.sp.window_court());
    assert_eq!(palw_bond_resolved_locks_v1(&c.s, &seat, licensed, depth(&c, licensed), c.sp.window_court()), 0, "a live claim's lock is work");
    assert_eq!(committed(&c, &seat, licensed), duty_before.max(lock), "max(duty, lock) while the claim lives");
    c.finalize(id);
    let final_daa = c.daa;
    assert!(final_daa < fence, "the premise: the lock was written below the height");
    assert_eq!(c.reserved(&seat), 0, "the duty left at Final");
    let resolved = palw_bond_resolved_locks_v1(&c.s, &seat, final_daa, depth(&c, final_daa), c.sp.window_court());
    assert_eq!(resolved, lock, "after Final the whole lock is the resolved term");
    assert_eq!(committed(&c, &seat, final_daa), lock, "and it is every term of committed");
    for daa in [final_daa, fence - 1] {
        assert_eq!(palw_bond_off_ceiling_raw_v1(&c.s, &c.sp, &seat, daa, raw_depth(&c, daa)), 0, "below the height: 0 at {daa}");
    }
    for daa in [fence, fence + 1_000] {
        assert_eq!(palw_bond_off_ceiling_raw_v1(&c.s, &c.sp, &seat, daa, raw_depth(&c, daa)), lock, "from the height: the lock at {daa}");
    }
    let twin = chain_on(t12(), c.s.clone(), c.daa);
    assert_eq!(palw_bond_off_ceiling_raw_v1(&twin.s, &twin.sp, &seat, fence, raw_depth(&twin, fence)), 0, "the launch build: never");
    println!(
        "[v02] t12 floor claim: accepted {first}, bound {bound} (duty from {duty_daa}), licensed {licensed}, Final {final_daa} \
         (licence→Final {} DAA); seat duty {:.2} MSK, lock {:.2} MSK (5 Valid, V1 door), collateral {:.2}, ceiling {:.2}",
        final_daa - licensed,
        msk(duty_before),
        msk(lock),
        msk(collateral(&c, &seat)),
        msk(ceiling(&c, &seat)),
    );
}

/// **The crossing (the V02 scenario)**: every floor seat already backs post-`Final` locks past its
/// 500‰ ceiling — one real Final's lock plus the accumulated locks of retired claims — so below the
/// fence (and on the launch build at any height) a new floor claim's bind is refused
/// `SeatValidLockRefused`, while from the height the same bind folds, the claim licenses and Finals,
/// and the next claim binds too: the seats' ledgers now stand well past 500‰ of their collateral,
/// never past 100%. The accuser room is the launch build's. A lock-full bond (resolved locks up to
/// `C − eligibility/2`) is refused past the fence too — the whole collateral still bounds it. And a
/// false-`Valid` conviction past the fence still slashes a resolved lock, exactly as the launch build
/// slashes it.
#[test]
fn v02_binds_continue_past_the_fence_once_locks_fill_the_ceiling_and_a_conviction_still_slashes() {
    let fence = 1_200u64;
    let p = armed(fence);
    let mut c = Chain::new(p.clone());
    let seats: Vec<PalwBondKeyV2> = c.floor_seats().iter().map(|(k, _)| *k).collect();

    // One real Final: every seat of the floor's panel holds its real lock on claim 1.
    let claim1 = c.floor_claim(0x0301);
    let bound1 = c.bind(claim1, &c.floor_seats());
    licence_and_finalize(&mut c, claim1, bound1);
    let real_lock = c.s.slashable_lock(seats[0], claim1).expect("a real lock").amount;

    // Claim 2, and the accumulated locks: each seat's committed set to its ceiling + 100,000 MSK (locks alone).
    let claim2 = c.floor_claim(0x0302);
    let now = c.daa;
    let e2 = eligibility(&c, &claim2, now);
    let expiry = now + c.sp.window_court();
    let fill: Vec<(PalwBondKeyV2, u128)> =
        seats.iter().map(|k| (*k, (ceiling(&c, k) + 100_000 * MSK).saturating_sub(committed(&c, k, now)))).collect();
    c.s = with_retired_locks(&c.sp, &c.s, h(0x0E_7123), &fill, expiry);
    for k in &seats {
        let committed_k = committed(&c, k, now);
        assert!(committed_k > ceiling(&c, k), "the premise: post-Final locks alone pass the 500‰ ceiling");
        assert_eq!(work_room(&c, k, now), 0, "below the height: no work room");
        assert!(accuser_room(&c, k, now) > 0, "the free half's accuser room is the remainder");
    }

    // Below the height: the bind is refused, on both builds.
    assert!(!binds(&c, now + 1, claim2), "below the fence the bind is refused (the panel is inert)");
    let twin = chain_on(t12(), c.s.clone(), c.daa);
    assert!(!binds(&twin, now + 1, claim2), "…as on the launch build");

    // From the height: the launch build still refuses; the armed build binds.
    c.step_at(fence - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let twin = chain_on(t12(), c.s.clone(), c.daa);
    assert!(!binds(&twin, fence, claim2), "the launch build refuses at the height and past it");
    assert!(binds(&c, fence, claim2), "the armed build binds from the height");
    for k in &seats {
        let room = work_room(&c, k, fence);
        assert!(room >= e2, "from the height a lock-heavy seat has room for the bind ({} ≥ {})", msk(room), msk(e2));
        assert_eq!(accuser_room(&c, k, fence), accuser_room(&twin, k, fence), "the accuser room does not move");
        assert!(committed(&c, k, fence) + palw_accuser_exposure_v1(&c.s, k) + room <= collateral(&c, k), "the one invariant");
    }
    let bound2 = c.bind(claim2, &c.floor_seats());
    assert_eq!(bound2, fence, "the bind folds at the height");
    licence_and_finalize(&mut c, claim2, bound2);
    // And the next one: binds keep happening while the locks stand past the ceiling.
    let claim3 = c.floor_claim(0x0303);
    let bound3 = c.bind(claim3, &c.floor_seats());
    licence_and_finalize(&mut c, claim3, bound3);
    let daa = c.daa;
    for k in &seats {
        let committed_k = committed(&c, k, daa);
        assert!(committed_k > ceiling(&c, k), "the ledger stands past 500‰");
        assert!(committed_k + palw_accuser_exposure_v1(&c.s, k) <= collateral(&c, k), "and never past 100%");
        println!(
            "[v02] seat {k:?}: committed {:.2} MSK = {:.1}‰ of {:.2}; work room {:.2}; the three claims' locks {:.2} + {:.2} + {:.2}",
            msk(committed_k),
            committed_k as f64 * 1000.0 / collateral(&c, k) as f64,
            msk(collateral(&c, k)),
            msk(work_room(&c, k, daa)),
            msk(c.s.slashable_lock(*k, claim1).map(|l| l.amount).unwrap_or(0)),
            msk(c.s.slashable_lock(*k, claim2).map(|l| l.amount).unwrap_or(0)),
            msk(c.s.slashable_lock(*k, claim3).map(|l| l.amount).unwrap_or(0)),
        );
    }

    // A false-`Valid` conviction past the height slashes the resolved lock — the launch build's slash.
    let accused = seats[0];
    let lock = c.s.slashable_lock(accused, claim1).expect("the real lock is still standing").amount;
    assert_eq!(lock, real_lock, "untouched by the crossing");
    let before = collateral(&c, &accused);
    let conviction = false_valid(&p, claim1, accused);
    let twin = chain_on(t12(), c.s.clone(), c.daa);
    let at = c.daa + 1;
    let twin_after = probe(&twin, at, std::slice::from_ref(&conviction)).expect("the launch build convicts");
    c.step_at(at, &[conviction], PalwBlockWorkV3::None, Hash64::default(), 0);
    let taken = before - collateral(&c, &accused);
    assert!(taken >= lock, "the conviction takes at least the lock ({} ≥ {})", msk(taken), msk(lock));
    assert!(c.s.slashable_lock(accused, claim1).is_none(), "the lock is consumed");
    assert_eq!(twin_after.bond(&accused).unwrap().collateral, c.s.bond(&accused).unwrap().collateral, "the same slash as the launch build");
    println!("[v02] conviction past the fence: seat {accused:?} lock {:.2} MSK, collateral taken {:.2} MSK", msk(lock), msk(taken));

    // The whole collateral still bounds a lock-full seat: resolved locks up to C − eligibility/2 refuse the bind.
    let claim4 = c.floor_claim(0x0304);
    let now = c.daa;
    let e4 = eligibility(&c, &claim4, now);
    let top: Vec<(PalwBondKeyV2, u128)> =
        seats[1..].iter().map(|k| (*k, (collateral(&c, k) - e4 / 2).saturating_sub(committed(&c, k, now)))).collect();
    let full = with_retired_locks(&c.sp, &c.s, h(0x0E_7124), &top, now + c.sp.window_court());
    let full = chain_on(p.clone(), full, c.daa);
    for (k, _) in &top {
        assert!(work_room(&full, k, now + 1) < e4, "a seat whose locks fill the collateral has no room even past the fence");
    }
    assert!(!binds(&full, now + 1, claim4), "100% still bounds the bind past the fence");
    assert!(binds(&c, now + 1, claim4), "…and the same seats without those locks bind it");
}
