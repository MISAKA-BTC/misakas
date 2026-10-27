//! **`palw_panel`'s `q7_sampled_and_collector_tests` module**, moved out of `palw_panel.rs` (2026-10-12, a behaviour-preserving split: the module path and every item in it are unchanged; only the file moved).

//! **ADR-0152 Q-7 on kaspad (F4 part 2), pinned where it lives.** `Sampled` is the answer of a
//! partial seat whose replay or resume is not coming, past SEAT-R, R-core+ and S3, in the last
//! sixty DAA of its window, on a capture SEAT-S3 verified first; it is filed as a V3 receipt over
//! the seat's assigned mask and never as a `Valid` or a V2. The V3 supplementary collector runs
//! after every licence, carries a release before an upgrade before pay, soonest first, and asks
//! the fold (the consensus crate's tests hold the set it builds to what the door credits:
//! `palw_rcore_q5_gate`, `t12_rcore_sr10_door_gate`).
use super::{
    COURT_MOVE_REPLAN_DAA, PALW_SEAT_SAMPLED_LEAD_DAA_V1, PalwSeatRRoleV1, PalwSeatReceiptFormsV1, PalwSeatS3SampleV1,
    palw_receipt_pools_sweep_v1, palw_seat_receipt_forms_v1, palw_seat_s3_sample_v1, palw_seat_sampled_due_v1,
    palw_supplementary_candidates_fingerprint_v1, palw_supplementary_idle_v1, palw_supplementary_offer_order_v1, palw_v1_offer_v1,
};
use crate::palw_receipt_pool::{PalwReceiptPoolV1, ReceiptSweepV1};
use kaspa_consensus_core::palw_backend::{
    PalwCaptureShapeV1, PalwClaimRootsV1, PalwExecutionBackendV1, PalwExecutionOutcomeV1, PalwMaterialVerdictV1,
};
use kaspa_consensus_core::palw_economic_safety_v1::PalwLicenceDoorTagV1;
use kaspa_consensus_core::palw_panel_v2::{PalwReceiptVerdictV2, PalwSeatReceiptV2, PalwSeatReceiptV3, palw_licence_offer_order_v1};
use kaspa_consensus_core::palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2, PalwSupplementaryEffectV1, PalwSupplementaryOfferV1};
use kaspa_consensus_core::palw_verification_v2::PalwSegmentMaskV2;
use kaspa_consensus_core::tx::TransactionOutpoint;
use kaspa_hashes::Hash64;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

fn h(n: u64) -> Hash64 {
    Hash64::from_u64_word(n)
}

/// A family that verifies exactly one capture against exactly one execution root, and answers
/// every site the same way until `fault_at` (0-based) — counting the sites it was asked.
struct S3Family {
    shape: bool,
    fault_at: Option<usize>,
    cannot: bool,
    asked: AtomicUsize,
}

const CAPTURE: &[u8] = b"the claim's own capture";

impl S3Family {
    fn new() -> Self {
        Self { shape: true, fault_at: None, cannot: false, asked: AtomicUsize::new(0) }
    }
}

impl PalwExecutionBackendV1 for S3Family {
    fn model_id(&self) -> &str {
        "s3-family"
    }
    fn job_for_anchor(&self, _anchor: Hash64) -> Result<(kaspa_consensus_core::palw_v2::PalwJobContextV2, Vec<usize>), String> {
        Err("not asked".into())
    }
    fn execute(
        &self,
        _job: &kaspa_consensus_core::palw_v2::PalwJobContextV2,
        _prompt: &[usize],
    ) -> Result<PalwExecutionOutcomeV1, String> {
        Err("not asked".into())
    }
    fn verify_material(&self, material: &[u8], claim: PalwClaimRootsV1) -> PalwMaterialVerdictV1 {
        if material == CAPTURE && claim.execution_root == h(0xE1) {
            PalwMaterialVerdictV1::Matches
        } else {
            PalwMaterialVerdictV1::Mismatch
        }
    }
    fn capture_shape(&self, _material: &[u8]) -> Option<PalwCaptureShapeV1> {
        let profile = kaspa_consensus_core::palw_base0_profile::base0_profile_v1(
            kaspa_consensus_core::palw_base0_profile::PALW_RC_BASE0_GEOMETRY,
        )
        .expect("the floor's profile");
        self.shape.then(|| PalwCaptureShapeV1 {
            job_context: kaspa_consensus_core::palw_base0_profile::rc_job_context(&profile, 8, 4),
            step_leaf_count: 100,
            layer_count: 4,
        })
    }
    fn replay_layer_site_v3(
        &self,
        _capture: &[u8],
        _site: kaspa_consensus_core::palw_layer_sample_v3::PalwLayerSiteV3,
        _seat_count: u16,
    ) -> Result<bool, String> {
        let n = self.asked.fetch_add(1, Ordering::SeqCst);
        if self.cannot {
            return Err("no opening".into());
        }
        Ok(self.fault_at != Some(n))
    }
}

fn roots(execution_root: Hash64) -> PalwClaimRootsV1 {
    PalwClaimRootsV1 {
        execution_root,
        trace_root: h(0x71),
        anchor: h(0xA1),
        attempt_draw: Some(false),
        output_root: Some(h(0x01)),
        job_pin: None,
    }
}

fn sample(family: &S3Family, capture: &[u8], execution_root: Hash64) -> PalwSeatS3SampleV1 {
    palw_seat_s3_sample_v1(family, capture, roots(execution_root), h(0x77), h(0xC1), 2, 5)
}

/// **SEAT-S3 before S3**: a capture that is not the claim's — other bytes, or the claim's bytes
/// under another execution root — is never sampled (not one site asked); the claim's own capture
/// with every site agreeing is `Sampled`; one site that disagrees is `Faulted` (the court's, not a
/// receipt); a site the family cannot recompute, or a shape it cannot read, is `Unchecked`.
#[test]
fn s3_samples_only_a_capture_seat_s3_verified() {
    for (bytes, root) in [(b"somebody else's".as_slice(), h(0xE1)), (CAPTURE, h(0xBAD))] {
        let family = S3Family::new();
        assert_eq!(sample(&family, bytes, root), PalwSeatS3SampleV1::Unchecked);
        assert_eq!(family.asked.load(Ordering::SeqCst), 0, "an unverified capture is never sampled");
    }
    let family = S3Family::new();
    assert_eq!(sample(&family, CAPTURE, h(0xE1)), PalwSeatS3SampleV1::Sampled);
    assert_eq!(
        family.asked.load(Ordering::SeqCst),
        kaspa_consensus_core::palw_layer_sample_v3::PALW_LAYER_SAMPLE_V3_SITES as usize,
        "every drawn site recomputed"
    );
    let family = S3Family { fault_at: Some(2), ..S3Family::new() };
    assert_eq!(sample(&family, CAPTURE, h(0xE1)), PalwSeatS3SampleV1::Faulted);
    assert_eq!(family.asked.load(Ordering::SeqCst), 3, "it stops at the first lie");
    let family = S3Family { cannot: true, ..S3Family::new() };
    assert_eq!(sample(&family, CAPTURE, h(0xE1)), PalwSeatS3SampleV1::Unchecked);
    let family = S3Family { shape: false, ..S3Family::new() };
    assert_eq!(sample(&family, CAPTURE, h(0xE1)), PalwSeatS3SampleV1::Unchecked);
    assert_eq!(family.asked.load(Ordering::SeqCst), 0);
}

/// **When a seat answers `Sampled`**: a partial seat that replays or resumes, past SEAT-R, R-core+
/// and S3, within the last `PALW_SEAT_SAMPLED_LEAD_DAA_V1` DAA of its window and not after it —
/// never the full seat, never an abstaining one, never with any fence dormant.
#[test]
fn sampled_is_due_only_for_a_partial_seat_at_the_end_of_its_window_past_every_fence() {
    let deadline = 1_000;
    let at = deadline - PALW_SEAT_SAMPLED_LEAD_DAA_V1;
    for role in [PalwSeatRRoleV1::PartialReplays, PalwSeatRRoleV1::PartialResumes] {
        assert!(palw_seat_sampled_due_v1(true, true, true, role, deadline, at), "{role:?} at the lead");
        assert!(palw_seat_sampled_due_v1(true, true, true, role, deadline, deadline), "{role:?} at the deadline");
        assert!(!palw_seat_sampled_due_v1(true, true, true, role, deadline, at - 1), "{role:?}: its Valid may still come");
        assert!(!palw_seat_sampled_due_v1(true, true, true, role, deadline, deadline + 1), "{role:?}: past the window");
        for (seat_r, rcore, s3) in [(false, true, true), (true, false, true), (true, true, false)] {
            assert!(!palw_seat_sampled_due_v1(seat_r, rcore, s3, role, deadline, at), "{role:?} {seat_r} {rcore} {s3}");
        }
    }
    for role in [PalwSeatRRoleV1::FullSeat, PalwSeatRRoleV1::PartialAbstains] {
        assert!(!palw_seat_sampled_due_v1(true, true, true, role, deadline, at), "{role:?} never samples");
    }
}

/// **Where `Sampled` is filed** (Q-1, SR-10): one V3 receipt over the seat's assigned mask, never
/// the whole-job V2 (the V2 doors take `Valid` alone), in every role that can reach it — so it
/// rides a licence set only as a rider (counted in nothing) or SR-10's V3 door, where it is
/// credited for pay and latches `unserved_seen`.
#[test]
fn sampled_is_filed_as_the_seats_v3_over_its_assigned_mask() {
    let mask = PalwSegmentMaskV2(0b0100);
    for role in [PalwSeatRRoleV1::PartialReplays, PalwSeatRRoleV1::PartialResumes] {
        assert_eq!(
            palw_seat_receipt_forms_v1(true, true, false, role, mask),
            PalwSeatReceiptFormsV1 { segments: Some(mask), whole: false },
            "{role:?}"
        );
    }
}

fn offer(claim: u64, releases: bool, upgrades: bool, lands_by: u64) -> (Hash64, PalwSupplementaryOfferV1) {
    (
        h(claim),
        PalwSupplementaryOfferV1 {
            object: PalwConsensusObjectV2::ReceiptLicensedV2 { claim: h(claim), receipts: Vec::new() },
            effect: PalwSupplementaryEffectV1 {
                credited: Vec::new(),
                recounted: Vec::new(),
                basis_k_before: 1,
                basis_k_after: if upgrades { 2 } else { 1 },
                upgrades,
                releases_escrow: releases,
                lands_by_daa: lands_by,
            },
        },
    )
}

/// **The collector's order**: a set that releases the escrow before one that lifts the S2 gate
/// before one that only pays, each soonest-first by the DAA it must land by, then by claim id.
#[test]
fn the_collector_carries_a_release_then_an_upgrade_then_pay_soonest_first() {
    let mut offers = vec![
        offer(1, false, false, 10),
        offer(2, false, true, 500),
        offer(3, true, true, 300),
        offer(4, false, true, 400),
        offer(5, true, false, 200),
        offer(6, false, false, 10),
    ];
    palw_supplementary_offer_order_v1(&mut offers);
    let order: Vec<Hash64> = offers.iter().map(|(claim, _)| *claim).collect();
    assert_eq!(order, vec![h(5), h(3), h(4), h(2), h(1), h(6)]);
}

/// **Where the pieces live in the loop.** The licence collector offers coverage, then V1, then S2
/// past the fence (X22, `palw_licence_offer_order_v1` handed the fence); the supplementary
/// collector runs after it (so it never takes a licence's slot), over both pools, with the
/// V2 full-replay upgrade chosen ahead of any pay set inside `palw_select_supplementary_offer_v1`,
/// and before a seat carries its own V2 receipt, only past `palw_rcore_plus`, skipping a pool that
/// came to nothing; `Sampled` is decided before the memory check (its case is a host that fits a
/// segment, not the whole replay), answers the verdict block first, and a sampled site that
/// disagrees is noted as the seat's fault (the court's), never filed.
#[test]
fn the_collector_follows_the_licences_and_sampled_precedes_the_memory_check() {
    use crate::palw_panel::readiness_memory_and_stuck_carrier_tests::CodeNeedleV1 as _;
    let source = crate::palw_panel::source_support::former_parent_production_source_v1();
    let licences =
        source.find_code("kaspa_consensus_core::palw_panel_v2::palw_licence_offer_order_v1(").expect("the licence collector");
    let licence_call = &source[licences..licences + 1_200];
    let (coverage, quorum, optimistic) = (
        licence_call.find_code(".palw_v2_receipt_coverage_assemble(claim, v3.clone())").expect("coverage"),
        licence_call.find_code("session.palw_v2_receipt_quorum_assemble(claim, set)").expect("V1"),
        licence_call.find_code(".palw_v2_optimistic_assemble(claim, v3.clone())").expect("S2"),
    );
    assert!(coverage < quorum && quorum < optimistic, "the doors handed over as coverage, V1, S2");
    assert!(
        licence_call.contains_code("self.consensus_config.params.palw_rcore_plus_active_at(current_daa),"),
        "X22's order is the fence's"
    );
    let collector = source.find_code("session.palw_v2_supplementary_assemble(claim, v3, v2)").expect("the supplementary collector");
    let own = source.find_code("session.palw_v2_supplementary_receipt_assemble(claim, vec![receipt])").expect("the own V2 receipt");
    assert!(licences < collector && collector < own, "licences, then the collector, then the seat's own receipt");
    let gated = &source[..collector];
    assert!(
        gated.rfind("if self.consensus_config.params.palw_rcore_plus_active_at(current_daa)").is_some_and(|at| at > licences),
        "the collector runs only past the fence"
    );
    let asked = &source[gated.rfind("for claim in claims {").expect("the collector's claims")..collector];
    assert!(asked.contains_code("palw_supplementary_idle_v1(supplementary_idle.get(&claim), fingerprint, current_daa)"));
    let claims = &source[gated.rfind("let mut claims = receipt_pool_v3.claim_ids();").expect("the V3 pool's claims")..collector];
    assert!(claims.contains_code("claims.extend(receipt_pool_v2.claim_ids());"), "and the V2 pool's");
    assert!(
        source.contains_code("Ok(PalwSeatS3SampleV1::Faulted) => self.note_seat_fault_v1(duty.claim_id, 0, 0),"),
        "a disagreeing site is the court's"
    );
    let sweep = source.find_code("let pools_sweep = palw_receipt_pools_sweep_v1(").expect("one sweep rule");
    assert_eq!(
        source[sweep..].find_code(
            "let pools_sweep = palw_receipt_pools_sweep_v1(self.consensus_config.params.palw_rcore_plus_active_at(current_daa),"
        ),
        Some(0),
        "the fence is passed to this sweep call"
    );
    let after = &source[sweep..sweep + 400];
    assert!(
        after.contains_code("receipt_pool_v2.sweep(&pools_sweep);") && after.contains_code("receipt_pool_v3.sweep(&pools_sweep);")
    );
    let duty_loop = &source[source.find_code("for index in order {").expect("the duty loop")..];
    let due = duty_loop.find_code("palw_seat_sampled_due_v1(").expect("the Sampled decision");
    let memory = duty_loop.find_code("self.replay_memory_budget_v1(").expect("the memory check");
    let verdict = duty_loop.find_code("let verdict = 'verdict: {").expect("the verdict block");
    let answers = duty_loop.find_code("break 'verdict Some(PalwReceiptVerdictV2::Sampled);").expect("the Sampled answer");
    assert!(due < memory && memory < verdict && verdict < answers, "decided before the memory check, answered first");
    assert!(
        answers < duty_loop.find_code("if self.resolve_backend(&session, duty.class_id, duty.artifact_root).is_err() {").unwrap(),
        "ahead of every arm"
    );
}

/// **X22: a licence at `basis_k ≥ 2` before S2, past the fence** (the M4 review's HIGH). With a V1
/// set and an S2 set both on hand, the node offers V1 — S2 would land the claim on Q-5's gate at
/// basis 1 — and asks S2 not at all; coverage still comes first; S2 only when neither is on hand.
/// Below the fence the order is what it was (coverage, S2, V1). The processor half —
/// `t12_x22_three_v2_valids_license_v1_not_s2` — holds the doors to what they build.
#[test]
fn x22_offers_a_replay_backed_licence_before_s2_and_asks_lazily() {
    use std::cell::Cell;
    let asked = Cell::new(Vec::<&str>::new());
    let door = |name: &'static str, on_hand: bool| {
        let asked = &asked;
        move || {
            let mut seen = asked.take();
            seen.push(name);
            asked.set(seen);
            on_hand.then_some(name)
        }
    };
    let offer = |x22: bool, coverage: bool, quorum: bool, optimistic: bool| {
        asked.set(Vec::new());
        let got = palw_licence_offer_order_v1(x22, door("coverage", coverage), door("quorum", quorum), door("optimistic", optimistic));
        (got, asked.take())
    };
    assert_eq!(
        offer(true, false, true, true),
        (Some(("quorum", PalwLicenceDoorTagV1::Quorum)), vec!["coverage", "quorum"]),
        "V1 on hand: offered, and S2 never built"
    );
    assert_eq!(offer(true, true, true, true), (Some(("coverage", PalwLicenceDoorTagV1::Coverage)), vec!["coverage"]));
    assert_eq!(
        offer(true, false, false, true),
        (Some(("optimistic", PalwLicenceDoorTagV1::Optimistic)), vec!["coverage", "quorum", "optimistic"]),
        "S2 only when nothing else is on hand"
    );
    assert_eq!(offer(true, false, false, false).0, None);
    assert_eq!(
        offer(false, false, true, true),
        (Some(("optimistic", PalwLicenceDoorTagV1::Optimistic)), vec!["coverage", "optimistic"]),
        "below the fence: coverage, S2, then V1, as before"
    );
    assert_eq!(offer(false, false, true, false).0, Some(("quorum", PalwLicenceDoorTagV1::Quorum)));
}

/// **The collector's composition under P2-5's contract, ASSUMED (ADR-0152 X22 with SR-6): a backed
/// V1 before S2, an unbacked one passed over.** Not T45's guard — the assembler here is a stand-in
/// that states the contract (the backed subset of the set, if it is a quorum), and P2-5 changed
/// neither `palw_v1_offer_v1` nor `palw_licence_offer_order_v1`, so this passes with or without
/// P2-5. What pins P2-5 is the processor's
/// `t12_t45_the_collector_prefers_a_backed_v1_over_s2_and_never_waits_on_an_unbacked_one`, which
/// drives these same two functions (both consensus-core's) over the REAL assemblers on a 2M claim
/// with an unbacked seat, and goes red without P2-5. What this test adds is the order's arithmetic
/// on a contract-shaped assembler: with an unbacked seat first in the pool the three-seat prefix is
/// passed over and the four-seat one offers its three backed seats, through V1, S2 never built;
/// rotated by `sent` the unbacked seat moves and the quorum prefix is backed at once; with two
/// backed seats in the pool no prefix licenses and S2 is offered.
#[test]
fn the_collector_composes_a_backed_v1_before_s2_under_the_assumed_p2_5_contract() {
    use std::cell::Cell;
    const UNBACKED: u64 = 9;
    const QUORUM: usize = 3;
    // The P2-5 assembler's contract: the backed subset of the set, if it is a quorum.
    let assemble = |set: Vec<u64>| {
        let backed: Vec<u64> = set.into_iter().filter(|seat| *seat != UNBACKED).collect();
        (backed.len() >= QUORUM).then_some(backed)
    };
    let s2_built = Cell::new(false);
    let collect = |pool: &[u64], sent: u32| {
        s2_built.set(false);
        palw_licence_offer_order_v1(
            true,
            || None,
            || palw_v1_offer_v1(pool, sent, assemble),
            || {
                s2_built.set(true);
                Some(vec![0])
            },
        )
    };
    assert_eq!(collect(&[UNBACKED, 1, 2, 3, 4], 0), Some((vec![1, 2, 3], PalwLicenceDoorTagV1::Quorum)), "the backed three, via V1");
    assert!(!s2_built.get(), "S2 never built while a backed V1 is on hand");
    assert_eq!(collect(&[UNBACKED, 1, 2, 3, 4], 1), Some((vec![1, 2, 3], PalwLicenceDoorTagV1::Quorum)), "rotated: backed at once");
    assert_eq!(collect(&[UNBACKED, 1, 2], 0), Some((vec![0], PalwLicenceDoorTagV1::Optimistic)), "no backed quorum: S2");
    assert!(s2_built.get());
}

fn receipt(claim: Hash64, seat: u64) -> PalwSeatReceiptV2 {
    PalwSeatReceiptV2 {
        claim,
        verdict: PalwReceiptVerdictV2::Valid,
        seat_bond: PalwBondKeyV2(TransactionOutpoint::new(h(seat), 0)),
        signed_daa: 105,
        signature: vec![seat as u8; 8],
    }
}

/// **Past the fence a licence this node carried drops none of the claim's receipts** (the M4
/// review's LOW: the sweep change was untested). Both pools hold a seat's V2 and V3 for a claim
/// this node submitted a licence for: past `palw_rcore_plus` both are kept — the V3 door and the
/// V2 door take them on the licensed claim, and the collector reads them — until the retention age,
/// which still bounds them; below the fence both go the moment the licence was carried, as before.
#[test]
fn past_the_fence_the_pools_keep_a_licensed_claims_receipts_until_the_retention_age() {
    let claim = h(0xC1);
    let pools = || {
        let mut v2: PalwReceiptPoolV1<PalwSeatReceiptV2> = PalwReceiptPoolV1::new(h(999));
        let mut v3: PalwReceiptPoolV1<PalwSeatReceiptV3> = PalwReceiptPoolV1::new(h(999));
        v2.insert_own(receipt(claim, 7), 100);
        v3.insert_own(PalwSeatReceiptV3 { receipt: receipt(claim, 7), segments: PalwSegmentMaskV2::single(1) }, 100);
        (v2, v3)
    };
    let live = HashSet::new();
    let submitted: HashMap<Hash64, u64> = [(claim, 110)].into();
    let carried_nothing = HashMap::new();
    let sweep = |current_daa: u64| ReceiptSweepV1 { live: &live, submitted: &submitted, current_daa, retention_daa: 4_000 };
    for (rcore, at, kept) in [(true, 120, true), (false, 120, false), (true, 100 + 4_001, false)] {
        let (mut v2, mut v3) = pools();
        let rule = palw_receipt_pools_sweep_v1(rcore, sweep(at), &carried_nothing);
        v2.sweep(&rule);
        v3.sweep(&rule);
        assert_eq!((v2.contains_claim(&claim), v3.contains_claim(&claim)), (kept, kept), "fence {rcore}, DAA {at}");
    }
}

/// **The collector does not re-ask a pool that came to nothing** (the M4 review's LOW on its
/// per-tick cost) — until a receipt arrives or leaves (the fingerprint moves) or the replan
/// interval passes.
#[test]
fn the_collector_skips_a_pool_that_came_to_nothing_until_it_changes() {
    let claim = h(0xC2);
    let v3 = vec![PalwSeatReceiptV3 { receipt: receipt(claim, 7), segments: PalwSegmentMaskV2::single(1) }];
    let v2 = vec![receipt(claim, 8)];
    let print = palw_supplementary_candidates_fingerprint_v1(&v3, &v2);
    assert_eq!(print, palw_supplementary_candidates_fingerprint_v1(&v3, &v2), "the same pool");
    let more = [v2.clone(), vec![receipt(claim, 9)]].concat();
    assert_ne!(print, palw_supplementary_candidates_fingerprint_v1(&v3, &more), "a receipt arrived");
    assert_ne!(print, palw_supplementary_candidates_fingerprint_v1(&[], &v2), "a receipt left");
    assert_ne!(
        palw_supplementary_candidates_fingerprint_v1(&v3, &[]),
        palw_supplementary_candidates_fingerprint_v1(&[], &[v3[0].receipt.clone()]),
        "a V3 is not its inner V2"
    );
    let none_at = Some(&(print, 500));
    assert!(!palw_supplementary_idle_v1(None, print, 500), "never asked");
    assert!(palw_supplementary_idle_v1(none_at, print, 500 + COURT_MOVE_REPLAN_DAA - 1), "the same pool, inside the interval");
    assert!(!palw_supplementary_idle_v1(none_at, print ^ 1, 501), "a changed pool is asked at once");
    assert!(!palw_supplementary_idle_v1(none_at, print, 500 + COURT_MOVE_REPLAN_DAA), "and the same pool once an interval");
}
