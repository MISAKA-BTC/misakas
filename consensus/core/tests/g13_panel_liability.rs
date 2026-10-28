//! **Lane g14-court-capacity, part 2 (2026-10-16): Panel collateral — Active and Resolved liability (audit G-13's capacity)** — the
//! fold half of the user's direction (`docs/design/palw/t12-daa14000-release-plan-2026-10-15.md`, "court の全体容量と Panel 担保" §2,
//! §3). The Valid lock is NOT released unconditionally at `F + 1,000` again: past LG14-A it lives to the claim's liability horizon
//! (`F + window_court`, G-13), and lane V02's existing split (`palw_final_lock_full_collateral`) keeps it as Resolved liability — off
//! the work ceiling, inside the total collateral, slashable (`kaspa_consensus_core::palw_panel_liability_v1` names the split).
//!
//! * **Late conviction**: a false-`Valid` conviction after `F + 1,000` but inside the horizon slashes every `Valid` signer by its full
//!   lock; under the old rule (V02's `F + 1,000`) the same conviction collects nothing.
//! * **No double pledge**: `L_active + L_resolved + accuser ≤ C` at every block, every room the fold grants is at most `R_new`, and
//!   several frauds proven in one block each collect their full lock.
//! * **Capital efficiency** against the old rule: collateral × lock time, claims processed, and Panel participation — measured on the
//!   fold (printed `[G-13 capital]`).
//! * **Restart and reorg**: the scenario on a recording tape reverts and re-applies block by block, restarts from the carriage at
//!   several tips and replays as an IBD to the same states, locks, slashes and releases.
//! * **Validation**: LG14-A with lane V02's short lock life needs `palw_final_lock_full_collateral` in force where the life is.
//!
//! The rulesets are testnet-12's launch ruleset with lane V02 (its lock life and its split, testnet-12's rule since DAA 750) at
//! genesis and, for the new rule, LG14-A at genesis — each validated with a test record (no mirror-only bypass, audit G-19). The one
//! counterfactual ruleset (the extension WITHOUT the split) is refused by that validation and is folded here only to measure what the
//! split saves; it is labelled so wherever it appears.
//!
//! Run: cargo test -p kaspa-consensus-core --test g13_panel_liability -j 4 -- --test-threads=2 --nocapture

#![allow(warnings)]
#[path = "rcore_common.rs"]
mod common;
use common::*;

use kaspa_consensus_core::config::params::{ForkActivation, palw_t12_launch_params_v1};
use kaspa_consensus_core::palw_g14_gates_v1::*;
use kaspa_consensus_core::palw_offence_v1::{
    PALW_PANEL_FALSE_VALID_VERSION_V1, PalwOffenceKindV1, PalwPanelContradictionV1, PalwPanelFalseValidEvidenceV1,
    palw_offence_evidence_digest_v1,
};
use kaspa_consensus_core::palw_panel_liability_v1::*;
use kaspa_consensus_core::palw_state_v2::{PalwRcoreGateV1, palw_rcore_bind_prices_v1, palw_rcore_gate_room_v1};

const MSK: u128 = 100_000_000;

fn msk(sompi: u128) -> f64 {
    sompi as f64 / MSK as f64
}

fn record(fence: PalwG14GatedFenceV1, genesis: Hash64, at: u64) -> PalwG14ArmingRecordV1 {
    let evidence: Vec<[u8; 32]> = (0..fence.requirements().len()).map(|i| [0x50 + i as u8; 32]).collect();
    PalwG14ArmingRecordV1 {
        version: PALW_G14_ARMING_RECORD_VERSION_V1,
        fence,
        genesis,
        activation_daa: at,
        source_commit: [0x13; 20],
        evidence: Box::leak(evidence.into_boxed_slice()),
        review: [0xEE; 32],
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Rule {
    /// Lane V02 as testnet-12 runs it: the Valid lock released at `F + 1,000`, resolved locks off the work ceiling.
    Old,
    /// The user's direction: LG14-A's G-13 lock to `F + window_court`, kept as Resolved liability by V02's split.
    New,
    /// COUNTERFACTUAL (refused by validation): G-13's extension with no split — every lock on the work ceiling.
    NoSplit,
}

/// The ruleset of `rule` (see [`Rule`]); `Old` and `New` pass the whole validation (with LG14-A's record for `New`); `NoSplit` is
/// asserted to be refused by name.
fn ruleset(rule: Rule) -> Params {
    let mut p = palw_t12_launch_params_v1();
    let at = Some(ForkActivation::new(0));
    p.palw_final_lock_life = at;
    p.sync_palw_final_lock_life();
    if rule != Rule::NoSplit {
        p.palw_final_lock_full_collateral = at;
        p.sync_palw_final_lock_full_collateral();
    }
    if rule == Rule::Old {
        p.validate_palw_v2().expect("lane V02 at genesis validates");
        return p;
    }
    p.palw_legacy_public_filer_v1 = at;
    p.sync_palw_legacy_public_filer_v1();
    p.sync_palw_g14_gates_v1();
    let records = [record(PalwG14GatedFenceV1::LegacyPublicFilerV1, p.genesis.hash, 0)];
    match rule {
        Rule::New => p.validate_palw_v2_with_arming_records_v1(&records).unwrap_or_else(|e| panic!("the new rule validates: {e:?}")),
        _ => {
            let why = p.validate_palw_v2_with_arming_records_v1(&records).expect_err("the extension without the split");
            assert!(format!("{why:?}").contains("palw_final_lock_full_collateral"), "{why:?}");
        }
    }
    p
}

fn raw_depth(c: &Chain, daa: u64) -> Option<u64> {
    c.extras_at(daa).settled_anchor_depth
}

fn liability(c: &Chain, bond: &PalwBondKeyV2, daa: u64) -> PalwPanelLiabilityV1 {
    palw_panel_liability_v1(&c.s, &c.sp, bond, daa, raw_depth(c, daa)).expect("a bond")
}

fn work_room(c: &Chain, bond: &PalwBondKeyV2, daa: u64) -> u128 {
    palw_rcore_gate_room_v1(&c.s, &c.sp, bond, daa, raw_depth(c, daa), PalwRcoreGateV1::Work)
}

fn collateral(c: &Chain, bond: &PalwBondKeyV2) -> u128 {
    u128::from(c.s.bond(bond).expect("a bond").collateral)
}

fn eligibility(c: &Chain, claim: &Hash64, daa: u64) -> u128 {
    let record = c.claim(claim);
    palw_rcore_bind_prices_v1(&c.s, &c.sp, &c.extras_at(daa), claim, &record, c.floor_seats().len(), daa).eligibility
}

/// **The split is the ledger, and nothing is pledged twice** (asserted for every bond in `bonds` at `daa`): `L_active + L_resolved +
/// accuser ≤ C`; the fold's work room is at most `R_new` with `R_other = accuser` (so a new commitment never takes collateral already
/// pledged) and, under the split, at least `R_new` with `R_other = accuser + reserve` (lane V02's relief); below the split nothing is
/// Resolved.
fn assert_split(c: &Chain, bonds: &[PalwBondKeyV2], daa: u64, at: &str) {
    for bond in bonds {
        let l = liability(c, bond, daa);
        let room = work_room(c, bond, daa);
        assert!(!l.over_pledged(), "{at}: {bond:?} pledged {} of {}", l.pledged(), l.collateral);
        assert!(room <= l.r_new(l.accuser), "{at}: {bond:?} room {room} > R_new {}", l.r_new(l.accuser));
        assert!(l.pledged() + room <= l.collateral, "{at}: {bond:?}: a new commitment never takes pledged collateral");
        if c.sp.final_lock_full_collateral_active_at(daa) {
            assert!(room >= l.r_new(l.accuser + l.accuser_reserve), "{at}: {bond:?}: the split's room");
        } else {
            assert_eq!(l.resolved, 0, "{at}: without the split every lock is Active");
        }
    }
}

/// `dos_g2`'s false-`Valid` evidence on the V1 route (the harness folds with `palw_offence_attribution` held dormant): an
/// `ExecutorEquivocation` by the claim's executor, under the key it registered, which convicts the execution every `Valid` signer
/// vouched for (`rcore_v02_final_lock_budget`'s).
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

/// A floor claim bound to the floor seats and licensed by every seat's `Valid`; returns (claim, licensed DAA).
fn licensed_floor(c: &mut Chain, seed: u64) -> (Hash64, u64) {
    let id = c.floor_claim(seed);
    let bound = c.bind(id, &c.floor_seats());
    let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    (id, c.daa)
}

/// **A live network's second clock** — `n` further floor claims run to `Final` (each one a settled anchor). A lock past its DAA expiry
/// stays live until testnet-12's anchor depth (30) has settled since its `Final`, for at most `2 × window_court`; on the live network
/// anchors settle every few DAA, but this harness's chain is quiet, so without them every lock would outlive its DAA clock by up to
/// `2 × window_court` under either rule. Returns the claims.
fn settle_anchors(c: &mut Chain, n: u64, seed: u64) -> Vec<Hash64> {
    let ids: Vec<Hash64> = (0..n).map(|i| licensed_floor(c, seed + i).0).collect();
    let last = ids.iter().map(|id| c.s.deadline_of(id).expect("a licensed claim owes its Final")).max().expect("n > 0");
    c.step_at(last + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(ids.iter().all(|id| matches!(c.claim(id).phase, PalwClaimPhaseV2::Final { .. })), "every anchor settled");
    ids
}

fn final_daa(c: &Chain, id: &Hash64) -> u64 {
    match c.claim(id).phase {
        PalwClaimPhaseV2::Final { final_daa } => final_daa,
        ref other => panic!("Final expected: {other:?}"),
    }
}

// ---- validation -------------------------------------------------------------------------------------------------------------------

/// **LG14-A beside lane V02's short lock life needs the split.** G-13 extends a `Valid` lock past `F + 1,000` only where V02's life
/// would have released it, so wherever that life and LG14-A are both in force `palw_final_lock_full_collateral` must be too (at or
/// below the later of the two): the extension is then Resolved liability, never the work ceiling. Without the life (testnet-12 as
/// launched) a lock already lives to `F + window_court` and nothing is required.
#[test]
fn lg14a_with_v02_lock_life_needs_the_full_collateral_split() {
    let _ = ruleset(Rule::New);
    let _ = ruleset(Rule::NoSplit);
    // The split ABOVE the later of the two: refused; at it: validates.
    let records = |p: &Params| [record(PalwG14GatedFenceV1::LegacyPublicFilerV1, p.genesis.hash, 0)];
    let mut late = ruleset(Rule::New);
    late.palw_final_lock_full_collateral = Some(ForkActivation::new(1));
    late.sync_palw_final_lock_full_collateral();
    assert!(late.validate_palw_v2_with_arming_records_v1(&records(&late)).is_err(), "the split after both");
    let mut life_later = ruleset(Rule::New);
    life_later.palw_final_lock_life = Some(ForkActivation::new(750));
    life_later.sync_palw_final_lock_life();
    life_later.palw_final_lock_full_collateral = Some(ForkActivation::new(750));
    life_later.sync_palw_final_lock_full_collateral();
    life_later.validate_palw_v2_with_arming_records_v1(&records(&life_later)).expect("the split with the life (testnet-12's 750)");
    // LG14-A without V02's life (testnet-12 as launched): no requirement.
    let mut launch = palw_t12_launch_params_v1();
    launch.palw_legacy_public_filer_v1 = Some(ForkActivation::new(0));
    launch.sync_palw_legacy_public_filer_v1();
    launch.sync_palw_g14_gates_v1();
    launch.validate_palw_v2_with_arming_records_v1(&records(&launch)).expect("no short life, nothing to split");
}

// ---- late conviction --------------------------------------------------------------------------------------------------------------

/// **A conviction after `F + 1,000` but inside the liability horizon slashes every `Valid` signer by its full lock.** The producer's
/// equivocation proves the execution false at `F + 2,000`; every floor seat signed `Valid`. Under the new rule each seat's lock is
/// alive (Resolved), and each conviction takes at least its lock and consumes it. Under the old rule (V02's `F + 1,000`) the locks are
/// gone by then and the same convictions collect nothing — the gap G-13 closes.
#[test]
fn late_conviction_after_f_plus_1000_slashes_every_valid_signer_in_full() {
    let mut results = Vec::new();
    for rule in [Rule::New, Rule::Old] {
        let p = ruleset(rule);
        let mut c = Chain::new(p.clone());
        let seats: Vec<PalwBondKeyV2> = c.floor_seats().iter().map(|(k, _)| *k).collect();
        let (id, _) = licensed_floor(&mut c, 0x1301);
        c.finalize(id);
        let f = final_daa(&c, &id);
        let wc = c.sp.window_court();
        let short = c.sp.final_lock_life_at(f);
        assert!(short < wc, "V02's life ({short}) is shorter than the horizon ({wc})");
        let locks: Vec<u128> = seats.iter().map(|k| c.s.slashable_lock(*k, id).map_or(0, |l| l.amount)).collect();
        assert!(locks.iter().all(|l| *l > 0), "every Valid seat locked at Final");
        let late = f + short + (wc - short) / 2;
        // The network keeps settling anchors after the Final (testnet-12's depth), so each lock is governed by its DAA clock.
        let _ = settle_anchors(&mut c, kaspa_consensus_core::config::params::PALW_T12_SETTLED_ANCHOR_DEPTH, 0x13A0);
        assert!(c.daa < f + short, "the anchors settled before the old rule's release");
        c.step_at(late - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert_split(&c, &seats, late - 1, "before the late conviction");
        let before: Vec<u128> = seats.iter().map(|k| collateral(&c, k)).collect();
        let convictions: Vec<PalwConsensusObjectV2> = seats.iter().map(|k| false_valid(&p, id, *k)).collect();
        let x = ctx(0xCA_0000 + late, late, late, 0);
        let taken: Vec<u128> = match c.try_fold(&c.s, &x, &convictions, PalwBlockWorkV3::None, Hash64::default()) {
            Ok((after, _, _)) => seats.iter().zip(&before).map(|(k, b)| b - u128::from(after.bond(k).unwrap().collateral)).collect(),
            Err(e) => {
                assert_eq!(rule, Rule::Old, "the new rule convicts: {e}");
                eprintln!("[G-13 late] old rule: the late conviction is refused ({e})");
                vec![0; seats.len()]
            }
        };
        if rule == Rule::New {
            c.step_at(late, &convictions, PalwBlockWorkV3::None, Hash64::default(), 0);
            for (i, k) in seats.iter().enumerate() {
                assert!(taken[i] >= locks[i], "seat {i}: the late conviction takes the full lock ({} ≥ {})", taken[i], locks[i]);
                assert!(c.s.slashable_lock(*k, id).is_none(), "seat {i}: the lock is consumed");
            }
            assert_split(&c, &seats, late, "after the late conviction");
        } else {
            assert!(taken.iter().all(|t| *t == 0), "the old rule collects nothing at F + {}: {taken:?}", late - f);
        }
        eprintln!(
            "[G-13 late] {rule:?}: F {f}, conviction at F + {}, locks {:?} MSK, taken {:?} MSK",
            late - f,
            locks.iter().map(|l| msk(*l)).collect::<Vec<_>>(),
            taken.iter().map(|t| msk(*t)).collect::<Vec<_>>()
        );
        results.push((rule, taken.iter().sum::<u128>()));
    }
    assert!(results[0].1 > 0 && results[1].1 == 0);
}

// ---- no double pledge -------------------------------------------------------------------------------------------------------------

/// **Active + Resolved never exceeds the collateral, and several frauds proven at once each collect their full lock.** Four floor
/// claims run to `Final` and a fifth stays licensed (Active) while every seat's split is checked at every stage; then all four Finals
/// are proven fraudulent in ONE block, every `Valid` signer convicted on each: each seat loses at least the sum of its four locks,
/// every one of those locks is consumed, the Active claim's lock stands untouched, and the split still holds. No aggregate-liability
/// coefficient is involved: each lock is backed one-for-one by posted collateral.
#[test]
fn active_plus_resolved_never_exceeds_collateral_and_simultaneous_frauds_each_collect() {
    let p = ruleset(Rule::New);
    let mut c = Chain::new(p.clone());
    let seats: Vec<PalwBondKeyV2> = c.floor_seats().iter().map(|(k, _)| *k).collect();
    let mut finals = Vec::new();
    for i in 0..4u64 {
        let (id, _) = licensed_floor(&mut c, 0x1310 + i);
        assert_split(&c, &seats, c.daa, "licensed");
        finals.push(id);
    }
    for id in &finals {
        c.finalize(*id);
        assert_split(&c, &seats, c.daa, "after a Final");
    }
    let (active, _) = licensed_floor(&mut c, 0x1320);
    let daa = c.daa;
    assert_split(&c, &seats, daa, "four Resolved, one Active");
    let l = liability(&c, &seats[0], daa);
    let resolved_locks: u128 = finals.iter().map(|id| c.s.slashable_lock(seats[0], *id).unwrap().amount).sum();
    assert_eq!(l.resolved, resolved_locks, "the four Finals' locks are exactly the Resolved term");
    assert!(l.active >= c.s.slashable_lock(seats[0], active).unwrap().amount, "the licensed claim's lock is Active");
    eprintln!(
        "[G-13 split] seat 0 at {daa}: C {:.0} MSK, C_work {:.0}, L_active {:.2}, L_resolved {:.2}, accuser {:.2}, reserve {:.0}, R_new {:.2}, fold room {:.2}",
        msk(l.collateral),
        msk(l.work_ceiling),
        msk(l.active),
        msk(l.resolved),
        msk(l.accuser),
        msk(l.accuser_reserve),
        msk(l.r_new(l.accuser + l.accuser_reserve)),
        msk(work_room(&c, &seats[0], daa))
    );
    // All four proven at once, F + 1,500 after the last Final (inside every horizon).
    let last_f = finals.iter().map(|id| final_daa(&c, id)).max().unwrap();
    let first_f = finals.iter().map(|id| final_daa(&c, id)).min().unwrap();
    let at = last_f + 1_500;
    assert!(at < first_f + c.sp.window_court(), "inside every claim's horizon");
    c.step_at(at - 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let before: Vec<u128> = seats.iter().map(|k| collateral(&c, k)).collect();
    let owed: Vec<u128> =
        seats.iter().map(|k| finals.iter().map(|id| c.s.slashable_lock(*k, *id).expect("alive").amount).sum()).collect();
    let active_locks: Vec<u128> = seats.iter().map(|k| c.s.slashable_lock(*k, active).map_or(0, |l| l.amount)).collect();
    let all: Vec<PalwConsensusObjectV2> =
        finals.iter().flat_map(|id| seats.iter().map(|k| false_valid(&p, *id, *k)).collect::<Vec<_>>()).collect();
    c.step_at(at, &all, PalwBlockWorkV3::None, Hash64::default(), 0);
    for (i, k) in seats.iter().enumerate() {
        let taken = before[i] - collateral(&c, k);
        assert!(taken >= owed[i], "seat {i}: four frauds at once collect every lock ({} ≥ {})", taken, owed[i]);
        assert!(finals.iter().all(|id| c.s.slashable_lock(*k, *id).is_none()), "seat {i}: every convicted lock consumed");
        let standing = c.s.slashable_lock(*k, active).map_or(0, |l| l.amount);
        assert_eq!(standing, active_locks[i], "seat {i}: the Active claim's lock is untouched");
    }
    assert_split(&c, &seats, at, "after four simultaneous convictions");
    eprintln!(
        "[G-13 split] four frauds in one block at {at}: owed {:?} MSK, collateral after {:?} MSK",
        owed.iter().map(|o| msk(*o)).collect::<Vec<_>>(),
        seats.iter().map(|k| msk(collateral(&c, k))).collect::<Vec<_>>()
    );
}

/// **A live reservation on a `Final` claim keeps its locks ACTIVE** (the user's table: Active liability includes "while an LG14-A
/// reservation holds Final" — and a reservation on a `Final` claim's open post-Final stage). An outsider reserves a `Final` floor claim
/// whose vesting row is still open: every seat's lock on it moves from Resolved to Active (on the work ceiling) with nothing else
/// changed, the split still holds, and when the outsider releases its reservation the lock is Resolved again.
#[test]
fn a_live_reservation_keeps_a_final_claims_locks_active() {
    let p = ruleset(Rule::New);
    let mut c = Chain::new(p.clone());
    let seats: Vec<PalwBondKeyV2> = c.floor_seats().iter().map(|(k, _)| *k).collect();
    let (id, _) = licensed_floor(&mut c, 0x1360);
    c.finalize(id);
    let outsider = bond_key(0x1361_0001);
    c.step(&[bond_obj(0x1361_0001, RICH)]);
    let before: Vec<PalwPanelLiabilityV1> = seats.iter().map(|k| liability(&c, k, c.daa + 1)).collect();
    assert!(before.iter().all(|l| l.resolved > 0), "a Final claim's locks are Resolved");
    let record = c.claim(&id);
    let reservation = PalwConsensusObjectV2::DisputeReservedV1 {
        reservation: Box::new(kaspa_consensus_core::palw_legacy_public_filer_v1::PalwDisputeReservationV1 {
            version: kaspa_consensus_core::palw_legacy_public_filer_v1::PALW_DISPUTE_RESERVATION_VERSION_V1,
            claim: id,
            execution_root: record.execution_root,
            trace_root: record.trace_root,
            reserver: outsider,
        }),
        signature: Vec::new(),
    };
    c.step(&[reservation]);
    assert!(c.s.legacy_dispute_v1(&id).is_some_and(|r| r.holds()), "the outsider's reservation is live on the Final claim");
    assert!(kaspa_consensus_core::palw_state_v2::palw_claim_under_dispute_v1(&c.s, &id));
    let held: Vec<PalwPanelLiabilityV1> = seats.iter().map(|k| liability(&c, k, c.daa)).collect();
    for (i, (b, h)) in before.iter().zip(&held).enumerate() {
        assert_eq!(h.resolved, 0, "seat {i}: under review, nothing is Resolved");
        assert_eq!(h.active, b.active + b.resolved, "seat {i}: the lock moved to Active, the ledger unchanged");
    }
    assert_split(&c, &seats, c.daa, "a reserved Final claim");
    c.step(&[PalwConsensusObjectV2::DisputeReleasedV1 { claim: id, reserver: outsider, signature: Vec::new() }]);
    assert!(!kaspa_consensus_core::palw_state_v2::palw_claim_under_dispute_v1(&c.s, &id), "released");
    let after: Vec<PalwPanelLiabilityV1> = seats.iter().map(|k| liability(&c, k, c.daa)).collect();
    for (i, (b, a)) in before.iter().zip(&after).enumerate() {
        assert_eq!((a.active, a.resolved), (b.active, b.resolved), "seat {i}: Resolved again once nobody holds it");
    }
    eprintln!(
        "[G-13 review] seat 0: Final {:.2} MSK Resolved → reserved {:.2} MSK Active, 0 Resolved → released {:.2} MSK Resolved",
        msk(before[0].resolved),
        msk(held[0].active),
        msk(after[0].resolved)
    );
}

// ---- capital efficiency -----------------------------------------------------------------------------------------------------------

/// What one run of the stream measured.
#[derive(Debug, Default)]
struct StreamV1 {
    /// Floor claims offered (one every `cadence` DAA).
    offered: u64,
    /// Offers the full Panel took (`work room ≥ eligibility` on every seat): bound and licensed.
    bound: u64,
    /// ∫ L_active dt and ∫ L_resolved dt over the window for one seat (sompi · DAA).
    active_time: u128,
    resolved_time: u128,
    /// The smallest work room seen on any seat (sompi).
    min_room: u128,
}

/// **One run of the stream**: every `cadence` DAA (at least 3: attempt, bind, licence are one block each) for `window` DAA a floor claim
/// is offered; if every floor seat has room for its eligibility `e` the Panel takes it — the attempt, its bind and its licence — and
/// otherwise the offer is not made (no attempt). Claims finalize on their own deadlines and their locks resolve and expire under `rule`;
/// the stream's own Finals are the anchors the second clock counts. The floor seats' collateral is set to `seat_collateral` (a carriage
/// edit; `None` keeps testnet-12's genesis collateral). Every point checks the split ([`assert_split`]).
fn stream(rule: Rule, seat_collateral: Option<u64>, e: u128, cadence: u64, window: u64) -> StreamV1 {
    assert!(cadence >= 3, "attempt, bind and licence take a block each");
    let p = ruleset(rule);
    let mut c = Chain::new(p);
    let seats: Vec<PalwBondKeyV2> = c.floor_seats().iter().map(|(k, _)| *k).collect();
    if let Some(x) = seat_collateral {
        c.s = edited(&c.sp, &c.s, |carriage| {
            for k in &seats {
                carriage.bonds.get_mut(k).expect("a floor seat").collateral = x;
            }
        });
    }
    let start = c.daa;
    let mut out = StreamV1 { min_room: u128::MAX, ..Default::default() };
    let mut seed = 0x1340u64;
    while c.daa + cadence <= start + window {
        let at = c.daa + 1;
        let rooms: Vec<u128> = seats.iter().map(|k| work_room(&c, k, at)).collect();
        out.min_room = out.min_room.min(*rooms.iter().min().unwrap());
        out.offered += 1;
        let l = liability(&c, &seats[0], at);
        out.active_time += l.active * u128::from(cadence);
        out.resolved_time += l.resolved * u128::from(cadence);
        // The Panel floor at `at` (user decision 2026-10-17: raised to 240,000 MSK where G-13's lock rule is in force): a seat below it
        // is not drawn, so no Panel binds.
        let floor = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_at_v1(
            c.sp.min_collateral_sompi(),
            c.sp.legacy_public_filer_active_at(at),
        );
        let drawable = seats.iter().all(|k| c.s.bond(k).is_some_and(|b| b.collateral >= floor));
        if drawable && rooms.iter().all(|r| *r >= e) {
            let id = c.floor_claim(seed);
            seed += 1;
            c.step(&[PalwConsensusObjectV2::PanelBound {
                claim: id,
                anchor: h(0xAC_0000 + c.daa),
                seats: seats_of(&c.floor_seats()),
            }]);
            assert!(matches!(c.claim(&id).phase, PalwClaimPhaseV2::PanelBound { .. }), "room ≥ eligibility: the Panel binds");
            let bind_daa = c.daa;
            let receipts: Vec<_> = c.floor_seats().iter().map(|(k, _)| valid(id, *k, bind_daa)).collect();
            c.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
            out.bound += 1;
        }
        assert_split(&c, &seats, c.daa, "the stream");
        let next = at - 1 + cadence;
        if next > c.daa {
            c.step_at(next, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        }
    }
    out
}

/// **Capital efficiency, measured on the fold**: the old rule (V02's `F + 1,000`), the new rule (G-13's horizon kept as Resolved
/// liability) and the refused counterfactual (G-13 with no split).
///
/// * Per claim (measured on the fold): the floor claim's eligibility `e`, its per-seat lock, licence → Final, the lock life after Final,
///   and collateral × lock time.
/// * Steady state (derived from those measured values and the fold's own room, `max(launch, split)`): the claim rate one seat sustains,
///   against testnet-12's expected per-seat rate, healthy and at the 2026-10-01 peak; the smallest seat collateral that sustains it; and
///   how many of testnet-12's genesis bonds (and a Panel-floor bond) meet that — the Panels able to participate.
/// * The stream (the fold itself): one floor claim offered every 3 DAA (testnet-12's healthy per-seat rate) for 6,000 DAA, on the floor
///   seats at genesis collateral and at the Panel floor (`10 × min_collateral`, the smallest bond the draw seats).
#[test]
fn capital_efficiency_against_the_old_rule() {
    // The per-claim measurements.
    let mut probe = Chain::new(ruleset(Rule::New));
    let seat = probe.floor_seats()[0].0;
    let id = probe.floor_claim(0x1330);
    let e = eligibility(&probe, &id, probe.daa + 1);
    let bound = probe.bind(id, &probe.floor_seats());
    let receipts: Vec<_> = probe.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
    probe.step(&[PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
    let licensed = probe.daa;
    let lock = probe.s.slashable_lock(seat, id).expect("locked").amount;
    probe.finalize(id);
    let f = final_daa(&probe, &id);
    let l = liability(&probe, &seat, f);
    let wc = probe.sp.window_court();
    let short = probe.sp.final_lock_life_at(f);
    let panel_floor = kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_v1(probe.sp.min_collateral_sompi());
    println!(
        "[G-13 capital] floor claim: eligibility {:.2} MSK, lock {:.2} MSK per Valid seat, bind→Final {} DAA, life after Final: old {short}, new {wc}; \
         genesis seat C {:.0} MSK, C_work {:.0}, V02 reserve {:.0}; Panel floor {:.0} MSK",
        msk(e),
        msk(lock),
        f - bound,
        msk(l.collateral),
        msk(l.work_ceiling),
        msk(l.accuser_reserve),
        msk(u128::from(panel_floor)),
    );
    println!(
        "[G-13 capital] collateral × lock time per claim per seat: old {:.0} MSK·DAA, new {:.0} MSK·DAA (×{:.2})",
        msk(lock) * (f - licensed + short) as f64,
        msk(lock) * (f - licensed + wc) as f64,
        (f - licensed + wc) as f64 / (f - licensed + short) as f64
    );
    // **Steady state** (derived from the measured per-claim values above): a seat backing λ claims per DAA holds λ·e·T_active of
    // Active liability (bind → Final at the bind's eligibility e) and λ·ℓ·life of Resolved. The fold's room is max(launch, split):
    // launch keeps the whole ledger under the work ceiling (λ ≤ C_work / (e·T_active + ℓ·life)); the split keeps the Active part under
    // the ceiling and the whole under the collateral less the V02 reserve (λ ≤ min(C_work / (e·T_active), (C − reserve) / (e·T_active
    // + ℓ·life))). The counterfactual has launch only.
    let t_active = (f - bound) as f64;
    let (reserve_f, e_f, lock_f) = (l.accuser_reserve as f64, e as f64, lock as f64);
    let ratio = l.work_ceiling as f64 / l.collateral as f64;
    let sustained = |c: f64, life: f64, split: bool| -> f64 {
        let launch = c * ratio / (e_f * t_active + lock_f * life);
        if !split {
            return launch;
        }
        launch.max((c * ratio / (e_f * t_active)).min((c - reserve_f) / (e_f * t_active + lock_f * life)))
    };
    // The smallest seat collateral that sustains λ (never below the Panel floor).
    let c_min = |lambda: f64, life: f64, split: bool| -> f64 {
        let ledger = e_f * t_active + lock_f * life;
        let launch = lambda * ledger / ratio;
        let need = if split { launch.min((lambda * e_f * t_active / ratio).max(reserve_f + lambda * ledger)) } else { launch };
        need.max(panel_floor as f64)
    };
    let rules = [
        ("old (V02: F + 1,000, split)", short as f64, true),
        ("new (G-13: F + window_court, split)", wc as f64, true),
        ("counterfactual (F + window_court, no split)", wc as f64, false),
    ];
    let genesis_collaterals: Vec<u64> = genesis_bonds(&probe.p).iter().map(|(_, _, c)| *c).collect();
    // The expected rate: testnet-12's observed issuance, 0.6 claims/DAA healthy and 5.3 at the 2026-10-01 peak, each claim engaging 5
    // of 9 seats (`docs/design/palw/t12-panel-backlog-1001.md` §2.3, §3) — per seat 5/9 of it.
    let seats_per_claim = probe.floor_seats().len() as f64;
    for (network_rate, label) in [(0.6, "healthy"), (5.3, "2026-10-01 peak")] {
        let per_seat = network_rate * seats_per_claim / 9.0;
        for (rule, life, split) in &rules {
            let at_genesis = sustained(l.collateral as f64, *life, *split);
            let at_floor = sustained(panel_floor as f64, *life, *split);
            let need = c_min(per_seat, *life, *split);
            let able = genesis_collaterals.iter().filter(|c| **c as f64 >= need).count();
            println!(
                "[G-13 capital] {label} {network_rate} claims/DAA → {per_seat:.3} per seat per DAA; {rule}: a genesis seat sustains {at_genesis:.3} \
                 ({:.2}×), a Panel-floor seat {at_floor:.3} ({:.2}×); smallest seat that sustains it {:.0} MSK — {able} of {} genesis bonds{}",
                at_genesis / per_seat,
                at_floor / per_seat,
                need / 1e8,
                genesis_collaterals.len(),
                if at_genesis >= per_seat { "" } else { "  — a genesis seat is BELOW the expected rate" }
            );
        }
    }
    // The stream: one claim offered every 3 DAA (the healthy per-seat rate) for 6,000 DAA, taken whenever the full Panel has room.
    let cadence = 3u64;
    let window = 6_000u64;
    let levels = [None, Some(panel_floor)];
    let mut rows = Vec::new();
    for seat_collateral in levels {
        for rule in [Rule::Old, Rule::New, Rule::NoSplit] {
            let run = stream(rule, seat_collateral, e, cadence, window);
            println!(
                "[G-13 capital] {:>7} seat C {:>14}: {}/{} offered claims taken in {window} DAA; seat 0 ∫L_active {:.0} + ∫L_resolved {:.0} MSK·DAA; \
                 min work room {:.0} MSK{}",
                format!("{rule:?}"),
                seat_collateral.map_or("genesis".to_string(), |x| format!("{:.0} MSK", msk(u128::from(x)))),
                run.bound,
                run.offered,
                msk(run.active_time),
                msk(run.resolved_time),
                msk(run.min_room),
                if rule == Rule::NoSplit { "  (COUNTERFACTUAL: refused by validation)" } else { "" }
            );
            rows.push((seat_collateral, rule, run));
        }
    }
    // What the split buys: at every collateral the new rule takes at least what the counterfactual takes, and at genesis collateral
    // the new rule takes every offered claim at the healthy rate, as the old rule does.
    for level in levels {
        let of = |rule: Rule| rows.iter().find(|(c, r, _)| *c == level && *r == rule).map(|(_, _, run)| run.bound).unwrap();
        assert!(of(Rule::New) >= of(Rule::NoSplit), "{level:?}: the split never takes fewer claims");
        assert!(of(Rule::Old) >= of(Rule::New), "{level:?}: the longer horizon never takes more claims");
    }
    let genesis =
        |rule: Rule| rows.iter().find(|(c, r, _)| c.is_none() && *r == rule).map(|(_, _, run)| (run.bound, run.offered)).unwrap();
    assert_eq!(genesis(Rule::New).0, genesis(Rule::New).1, "at genesis collateral every offered claim is taken");
    // **User decision 2026-10-17: the Panel floor under G-13's lock rule is 240,000 MSK** (`PALW_PANEL_FLOOR_UNDER_G13_SOMPI_V1`). It is
    // re-derived here from the measured per-claim values: the smallest seat that sustains the healthy per-seat rate under the new rule
    // must not exceed it, and a seat AT it sustains the healthy rate (coverage ≥ 1); the peak is reported.
    let new_floor = kaspa_consensus_core::palw_panel_economy_v1::PALW_PANEL_FLOOR_UNDER_G13_SOMPI_V1;
    let healthy = 0.6 * seats_per_claim / 9.0;
    let peak = 5.3 * seats_per_claim / 9.0;
    let (_, wc_life, _) = rules[1];
    let need_healthy = c_min(healthy, wc_life, true);
    let need_peak = c_min(peak, wc_life, true);
    let at_new_floor = sustained(new_floor as f64, wc_life, true);
    println!(
        "[G-13 floor] new Panel floor {:.0} MSK: sustains {at_new_floor:.3} claims/DAA per seat — healthy {healthy:.3} ({:.2}×), peak {peak:.3} \
         ({:.2}×); re-derived minimum: healthy {:.0} MSK, peak {:.0} MSK",
        msk(u128::from(new_floor)),
        at_new_floor / healthy,
        at_new_floor / peak,
        need_healthy / 1e8,
        need_peak / 1e8
    );
    assert!(need_healthy <= new_floor as f64, "the floor covers the re-derived healthy minimum ({:.0} MSK)", need_healthy / 1e8);
    assert!(at_new_floor >= healthy, "a seat at the new floor sustains the healthy rate");
    let run = stream(Rule::New, Some(new_floor), e, cadence, window);
    println!("[G-13 floor] stream at the new floor (new rule): {}/{} offered claims taken in {window} DAA", run.bound, run.offered);
    assert_eq!(run.bound, run.offered, "at the new floor the stream takes every offered claim at the healthy rate");
    // The floor is the draw's: past LG14-A (G-13's lock rule) the panel floor is raised; below it, ten producer floors.
    let min = probe.sp.min_collateral_sompi();
    assert_eq!(kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_at_v1(min, false), panel_floor);
    assert_eq!(kaspa_consensus_core::palw_panel_economy_v1::palw_panel_collateral_floor_at_v1(min, true), new_floor.max(panel_floor));
}

// ---- restart and reorg ------------------------------------------------------------------------------------------------------------

/// **Restart and reorg give identical liabilities, slashes and releases.** The late-conviction scenario plus a second claim left to
/// run past its horizon (its Resolved lock released by expiry), on a recording tape: every block's delta reverts and re-applies down
/// to the base and back; a node restarted from the carriage at the licence, at the Final, past `F + 1,000` and just before the
/// conviction folds every later block to the same state; and an IBD from the base replays every block to the same deltas and roots.
/// At each restart point the split read on the loaded state is the uninterrupted run's.
#[test]
fn restart_and_reorg_give_identical_liabilities_and_slashes() {
    let p = ruleset(Rule::New);
    let mut tape = Tape::new(Chain::new(p.clone()));
    let seats: Vec<PalwBondKeyV2> = tape.c.floor_seats().iter().map(|(k, _)| *k).collect();
    let mut marks = Vec::new();
    let run = |tape: &mut Tape, seed: u64| -> Hash64 {
        let id = tape.attempt(None, seed);
        let bound = tape.bind(id);
        let receipts: Vec<_> = tape.c.floor_seats().iter().map(|(k, _)| valid(id, *k, bound)).collect();
        tape.step(vec![PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts }]);
        id
    };
    let fraud = run(&mut tape, 0x1350);
    let honest = run(&mut tape, 0x1351);
    marks.push(tape.len());
    let deadline = tape.c.s.deadline_of(&honest).unwrap().max(tape.c.s.deadline_of(&fraud).unwrap());
    tape.at(deadline + 1, vec![]);
    marks.push(tape.len());
    let f = [fraud, honest].iter().map(|id| final_daa(&tape.c, id)).max().unwrap();
    let wc = tape.c.sp.window_court();
    tape.at(f + 1_001, vec![]);
    marks.push(tape.len());
    tape.at(f + 1_999, vec![]);
    marks.push(tape.len());
    let convictions: Vec<PalwConsensusObjectV2> = seats.iter().map(|k| false_valid(&p, fraud, *k)).collect();
    tape.at(f + 2_000, convictions);
    assert!(seats.iter().all(|k| tape.c.s.slashable_lock(*k, fraud).is_none()), "the fraud's locks are taken");
    assert!(seats.iter().all(|k| tape.c.s.slashable_lock(*k, honest).is_some()), "the honest claim's Resolved locks stand");
    // Past the horizon (`F + window_court`) and the second clock's bound (`2 × window_court` more: this tape settles no further
    // anchors), the honest claim's Resolved liability is released by expiry.
    let gone = f + 3 * wc + 2;
    tape.at(gone, vec![]);
    let released: Vec<bool> = seats
        .iter()
        .map(|k| {
            let l = palw_panel_liability_v1(&tape.c.s, &tape.c.sp, k, gone, tape.c.extras_at(gone).settled_anchor_depth).unwrap();
            l.resolved == 0
        })
        .collect();
    assert!(released.iter().all(|r| *r), "past the horizon the honest claim's Resolved liability is released");
    tape.revert_to_base_and_reapply();
    for j in marks {
        let loaded = tape.restart_at(j);
        let daa = tape.daa_at(j);
        let depth = tape.c.extras_at(daa).settled_anchor_depth;
        for k in &seats {
            assert_eq!(
                palw_panel_liability_v1(&loaded, &tape.c.sp, k, daa, depth),
                palw_panel_liability_v1(tape.state_at(j), &tape.c.sp, k, daa, depth),
                "tip {j}: the restarted node's split"
            );
        }
    }
    tape.ibd_from(tape.base.clone());
    eprintln!("[G-13 restart] {} blocks recorded; reverted, re-applied, restarted at 4 tips and replayed", tape.len());
}
