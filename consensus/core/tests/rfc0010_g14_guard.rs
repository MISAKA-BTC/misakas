//! **RFC-0010 × G14: no non-fraud end pre-empts an accusation** (agent SHARD, `docs/design/palw/shard-rfc6-10.md` §2), through
//! testnet-12's own fold with the permissionless Panel armed on a copy of the params (`rfc0010_production_fold.rs`'s fixture; these
//! tests bypass `validate_palw_v2`, and say so — the fence is refused at every real height).
//!
//! One predicate (`PalwChainStateV2::palw_accusation_pending_v1`: a DA session, a court session, `DefaultDisputed`) and one writer
//! of every V3 non-fraud end (the fold's `end_claim`): while an accusation is pending,
//!
//! * **a pre-bind engine end** (`BeaconUnavailable` here) is DEFERRED — the engine keeps its decision on its own clock, the V2
//!   claim stays where it is and owes no V2 deadline — and applied at the first stage nothing is pending (the court cleared), or
//!   never, because the court convicted first;
//! * **a V3 S2 licence** owes no deadline (DL-1's G14 row): its gate (an uncharged `PanelUnavailable` expiry that would close every
//!   session neutrally) waits, a court cleared while a DA session is still open re-arms nothing, and the default wins;
//! * **at the receipt window's edge**: an accusation inside the window holds it (no redraw); one carried by the very block whose
//!   pre-object stage redraws comes after that decision — a late accusation, which from then on holds the redrawn Panel.
//!
//! Every block is checked as `rfc0010_production_fold.rs` checks one: the delta re-applies and reverts, the carriage reloads under
//! its root, and the internal and DL-1 deadline consistency hold (an engine end deferred under a pending accusation included).
//!
//! Run: `cargo test -p kaspa-consensus-core --test rfc0010_g14_guard`

#[path = "rcore_common.rs"]
mod common;
use common::*;

use std::collections::BTreeSet;

use kaspa_consensus_core::config::params::ForkActivation;
use kaspa_consensus_core::palw_panel_beacon_v1::{panel_beacon_context_v1, panel_beacon_scheme_of_v1};
use kaspa_consensus_core::palw_permissionless_panel_v1::{
    BeaconProofV1, BeaconRequestV1, ClaimPhaseV3, NonFraudReasonV1, PalwPermissionlessPanelV1, PanelPolicyV1,
};
use kaspa_consensus_core::palw_state_v2::{
    PalwBlockContextV2, PalwPanelV3BeaconSourceV1, PalwPanelV3InputsV1, PalwStateDeltaV2, PalwVoidReasonV2,
    palw_rcore_licence_awaits_replay_v1,
};
use misaka_palw_challenge::{FinalPathV1, PostCommitChallengePolicyV1, WorkBeaconStateV1, WorkFinalEventV1, WorkSourceKindV1};
use misaka_palw_challenge::{collect_work_beacon_v1, policy::reference_policy_v1};

const FENCE: u64 = 1_000;

fn challenge_policy() -> PostCommitChallengePolicyV1 {
    reference_policy_v1(1, 1, 10, 1, 1)
}

fn engine_policy() -> PanelPolicyV1 {
    PanelPolicyV1 {
        seal_depth_blocks: 1,
        seal_wait_daa: 50,
        bond_maturity_daa: 1,
        beacon_period_daa: 100,
        beacon_wait_daa: 8,
        assignment_delay_daa: 1,
        receipt_window_daa: 3,
        seat_count: 5,
        outsider_seats: 0,
        max_retries: 1,
        min_collateral: 1,
        max_candidates: 64,
        max_pending: 64,
        max_pending_per_bond: 16,
        max_assignments_per_block: 8,
        max_admissions_per_block: 8,
        max_tracked_claims: 256,
        max_beacons_per_block: 2,
        max_beacon_proof_bytes: 4096,
        beacon_scheme: panel_beacon_scheme_of_v1(&challenge_policy()),
    }
}

fn independent_event(profile: [u8; 64], work: u8, accepted: u64, settled: u64) -> WorkFinalEventV1 {
    WorkFinalEventV1 {
        kind: WorkSourceKindV1::RealUsefulWork,
        source_profile_id: profile,
        canonical_work_id: [work; 64],
        execution_commitment: [work; 64],
        accepted_position: accepted,
        settlement_position: settled,
        occurrence_index: 0,
        claim_final: true,
        da_satisfied: true,
        validity_independent: true,
        depends_on_profiles: Vec::new(),
        final_path: FinalPathV1::PanelIndependent,
    }
}

struct V3 {
    c: Chain,
    inputs: PalwPanelV3InputsV1,
}

impl V3 {
    fn new() -> Self {
        Self::with(engine_policy())
    }

    fn with(policy: PanelPolicyV1) -> Self {
        let mut p = t12();
        p.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(FENCE), policy });
        p.sync_palw_permissionless_panel_v1();
        let c = Chain::new(p);
        let floor = genesis_classes(&c.p)[0].0;
        let inputs = PalwPanelV3InputsV1 {
            draw: Default::default(),
            capability_proof: false,
            floor_class: floor,
            approved_beacons: vec![challenge_policy()],
            beacon_source: PalwPanelV3BeaconSourceV1::Reference {
                events: Vec::new(),
                eligible_profiles: BTreeSet::new(),
                works: Vec::new(),
                sealed: Vec::new(),
            },
        };
        Self { c, inputs }
    }

    fn extras(&self, daa: u64) -> PalwTransitionExtrasV1 {
        let mut e = self.c.extras_at(daa);
        e.panel_v3 = Some(self.inputs.clone());
        e
    }

    fn step_at(
        &mut self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        subsidy: u64,
    ) -> PalwStateDeltaV2 {
        let x: PalwBlockContextV2 = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = self.c.s.clone();
        let (child, delta, skips) = fold_with(&self.c.p, &self.c.sp, &parent, &x, objects, work, key, &self.extras(daa))
            .unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
        assert!(skips.is_empty(), "nothing skipped at {daa}: {skips:?}");
        assert_eq!(apply_delta_v2(&parent, &delta, &self.c.sp).expect("re-applies"), child, "DAA {daa}: the delta is the transition");
        assert_eq!(revert_delta_v2(&child, &delta, &self.c.sp).expect("reverts"), parent, "DAA {daa}: the delta reverts");
        let reloaded = PalwStateCarriageV2::from_state(&child)
            .into_state(&self.c.sp, Some(child.state_root()))
            .unwrap_or_else(|e| panic!("DAA {daa}: the carriage reloads: {e}"));
        assert_eq!(reloaded, child, "DAA {daa}: reload is the state");
        child.assert_internal_consistency(&self.c.sp).unwrap_or_else(|e| panic!("DAA {daa}: internal consistency: {e}"));
        child.assert_deadline_consistency(&self.c.sp).unwrap_or_else(|e| panic!("DAA {daa}: deadline consistency: {e}"));
        self.c.s = child;
        self.c.daa = daa;
        delta
    }

    fn at(&mut self, daa: u64) {
        self.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    fn step(&mut self) {
        self.at(self.c.daa + 1);
    }

    fn step_with(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.step_at(self.c.daa + 1, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    fn floor_claim(&mut self, seed: u64) -> Hash64 {
        let (floor, _, _, _) = genesis_classes(&self.c.p)[0];
        let (bond, pubkey, operator) = floor_producer(&self.c.p);
        let pwu = self.c.floor_pwu(self.c.daa + 1);
        let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
        self.step_at(self.c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        assert!(self.c.s.claim(&id).is_some(), "the floor attempt is accepted");
        id
    }

    fn record(&self, id: &Hash64) -> kaspa_consensus_core::palw_permissionless_panel_v1::ClaimRecordV3 {
        self.c.s.panel_v3().expect("the engine").claim_rows().get(id).expect("the engine tracks the claim").clone()
    }

    fn phase_v2(&self, id: &Hash64) -> PalwClaimPhaseV2 {
        self.c.s.claim(id).expect("the claim is live").phase.clone()
    }

    fn proof_for(&mut self, id: &Hash64) -> BeaconProofV1 {
        let seal = self.record(id).seal.expect("sealed");
        let (release, epoch) = (seal.anchor_slot, seal.beacon_epoch);
        let mirror = *self.c.sp.panel_v3().expect("the mirror");
        let profile = [0x77u8; 64];
        let event = independent_event(profile, 0x42, release + 1, release + 2);
        self.inputs.beacon_source = PalwPanelV3BeaconSourceV1::Reference {
            events: vec![event.clone()],
            eligible_profiles: BTreeSet::from([profile]),
            works: Vec::new(),
            sealed: Vec::new(),
        };
        let request = BeaconRequestV1 {
            network: mirror.network,
            ruleset: mirror.ruleset,
            scheme: mirror.policy.beacon_scheme,
            epoch,
            release_daa: release,
            deadline_daa: release + mirror.policy.beacon_wait_daa,
        };
        let context = panel_beacon_context_v1(&request, &challenge_policy(), BTreeSet::from([profile]));
        let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &[event], release + 5).expect("a valid policy")
        else {
            panic!("the reference source locks a beacon");
        };
        BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() }
    }

    fn run_to_bound(&mut self, seed: u64) -> (Hash64, u64) {
        let id = self.floor_claim(seed);
        self.step();
        self.step();
        assert_eq!(self.record(&id).phase, ClaimPhaseV3::Sealed);
        let release = self.record(&id).seal.unwrap().anchor_slot;
        let proof = self.proof_for(&id);
        self.at(release + 4);
        self.step_at(
            release + 5,
            &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        self.at(release + 10);
        assert!(matches!(self.record(&id).phase, ClaimPhaseV3::Bound(_)), "{:?}", self.record(&id).phase);
        (id, release + 10)
    }

    /// A genesis bond that is neither the producer nor in `seats`.
    fn outsider(&self, seats: &[PalwBondKeyV2]) -> PalwBondKeyV2 {
        let (producer, _, _) = floor_producer(&self.c.p);
        genesis_bonds(&self.c.p).iter().map(|(k, _, _)| *k).find(|k| *k != producer && !seats.contains(k)).expect("a bond outside")
    }

    fn panel_seats(&self, id: &Hash64) -> Vec<(PalwBondKeyV2, Hash64)> {
        self.c.s.panel(id).expect("a bound panel").seats.iter().map(|seat| (seat.bond, seat.operator_id)).collect()
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// A pre-bind end under an open court
// ---------------------------------------------------------------------------------------------------------------------------

/// **`BeaconUnavailable` is deferred while a court is open on the claim, and applied in the block the court is cleared** — never a
/// neutral close of the court. The engine decides on its own clock (its record is `Voided` at the window's end, nothing re-rolled);
/// the V2 claim stays `Provisional`, owes no V2 deadline, is never offered to the lane-A binder; the court's clear releases it.
#[test]
fn a_beacon_unavailable_end_waits_for_an_open_court_and_lands_when_the_court_is_cleared() {
    let mut w = V3::new();
    let id = w.floor_claim(70);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    let challenger = w.outsider(&[]);
    let session = court_session_of(&w.c.s, id, challenger);
    assert!(w.c.sp.turn_deadline_daa() >= 2, "the premise: the court's first rung outlives the next two blocks");
    // The court opens on the last DAA of the contribution window, before the engine's decision.
    w.step_at(release + 8, &[court_opened(&w.c.s, id, challenger)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(w.c.s.open_courts_of(&id) > 0 && w.c.s.palw_accusation_pending_v1(&id), "a court is pending on the unbound claim");
    // The contribution window closes with nothing carried: the engine ends the claim — and the V2 void waits.
    w.at(release + 9);
    assert!(
        matches!(w.record(&id).phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::BeaconUnavailable, .. }),
        "the engine decided on its own clock: {:?}",
        w.record(&id).phase
    );
    assert_eq!(w.phase_v2(&id), PalwClaimPhaseV2::Provisional, "deferred: the court is not closed by a non-fraud void");
    assert!(w.c.s.panel_v3_clocks_claim_v1(&id), "still the engine's");
    assert_eq!(w.c.s.deadline_of(&id), None, "no V2 deadline while deferred");
    let offered = kaspa_consensus_core::palw_state_v2::palw_claims_provisional_past_their_anchor_slot_v1(&w.c.s, u64::MAX, 1);
    assert!(!offered.contains(&id), "never offered to the lane-A binder");
    assert!(w.c.s.court_session(&session).is_some(), "the court runs on");
    // The court clears (the challenger defeated): nothing is pending, the deferred end lands in that block, uncharged.
    let (producer, _, _) = floor_producer(&w.c.p);
    w.step_with(&[court_cleared(session)]);
    assert!(!w.c.s.palw_accusation_pending_v1(&id));
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BeaconUnavailable, .. }),
        "the engine's decision, applied once nothing is pending: {:?}",
        w.phase_v2(&id)
    );
    assert_eq!(w.c.s.bond(&producer).unwrap().slashed, 0, "a non-fraud end charges nobody");
    assert!(!w.c.s.panel_v3_clocks_claim_v1(&id));
}

/// **A deferred end is never applied while the accusation runs, and a conviction ends the claim first.** The court's responder
/// never moves: walking the court to its own end, at every stop the claim is either still deferred (`Provisional`, the court open)
/// or ended by the court — never `BeaconUnavailable` while a session was open.
#[test]
fn a_deferred_end_never_closes_a_running_court_and_the_court_decides_first() {
    let mut w = V3::new();
    let id = w.floor_claim(71);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    let challenger = w.outsider(&[]);
    let session = court_session_of(&w.c.s, id, challenger);
    w.step_at(release + 8, &[court_opened(&w.c.s, id, challenger)], PalwBlockWorkV3::None, Hash64::default(), 0);
    let court_deadline = w.c.s.court_session(&session).expect("open").deadline_daa;
    w.at(release + 9);
    assert!(matches!(w.record(&id).phase, ClaimPhaseV3::Voided { reason: NonFraudReasonV1::BeaconUnavailable, .. }));
    let mut stops: Vec<u64> = (1..=8).map(|k| release + 9 + k * (court_deadline + 4 - release - 9) / 8).collect();
    stops.dedup();
    for daa in stops {
        if daa <= w.c.daa {
            continue;
        }
        let open_before = w.c.s.palw_accusation_pending_v1(&id);
        w.at(daa);
        let phase = w.phase_v2(&id);
        match phase {
            PalwClaimPhaseV2::Provisional => assert!(w.c.s.palw_accusation_pending_v1(&id), "DAA {daa}: deferred only while pending"),
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::BeaconUnavailable, .. } => {
                assert!(!w.c.s.palw_accusation_pending_v1(&id), "DAA {daa}: applied only once nothing is pending");
                assert!(!open_before || w.c.s.court_session(&session).is_none(), "DAA {daa}: the court had ended");
            }
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtDefault, .. } => {}
            other => panic!("DAA {daa}: {other:?}"),
        }
        if phase.is_terminal() {
            eprintln!("[g14-guard] the claim ended at DAA {daa}: {phase:?}");
            return;
        }
    }
    panic!("the court did not end by its own deadline + 4");
}

// ---------------------------------------------------------------------------------------------------------------------------
// The V3 S2 licence (DL-1's G14 row)
// ---------------------------------------------------------------------------------------------------------------------------

/// A V3 claim bound and S2-licensed by its full seat and one partial seat (`OptimisticLicensed`, `basis_k` 1). Returns the claim,
/// its bound DAA and the gate (`bound + window_receipt + 1`).
fn s2_licensed(w: &mut V3, seed: u64) -> (Hash64, u64, u64) {
    let (id, bound) = w.run_to_bound(seed);
    let seats = w.panel_seats(&id);
    let anchor = w.c.s.panel(&id).unwrap().anchor;
    let full = palw_segment_assignment_v2(anchor, id, seats.len() as u16).full_seat as usize;
    let partial = (full + 1) % seats.len();
    w.step_with(&[PalwConsensusObjectV2::OptimisticLicensed {
        claim: id,
        receipts: covered(id, anchor, &seats, &[full, partial], bound + 1),
    }]);
    let claim = w.c.s.claim(&id).unwrap().clone();
    assert!(matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }), "S2 licenses: {:?}", claim.phase);
    assert!(palw_rcore_licence_awaits_replay_v1(&claim), "basis_k 1: the gate is armed");
    let gate = w.c.s.deadline_of(&id).expect("DL-1's Q-5 row");
    (id, bound, gate)
}

/// **The twin with nothing pending**: the gate expires the V3 S2 licence `PanelUnavailable`, uncharged (RFC-0010's rule, unchanged).
#[test]
fn with_nothing_pending_a_v3_s2_licence_expires_panel_unavailable_at_its_gate() {
    let mut w = V3::new();
    let (id, _, gate) = s2_licensed(&mut w, 80);
    w.at(gate - 1);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::ReceiptLicensed { .. }));
    w.at(gate + 1);
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PanelUnavailable, .. }),
        "{:?}",
        w.phase_v2(&id)
    );
}

/// **A non-seat DA session on a V3 S2 licence holds its gate, and the default wins.** The session disarms the deadline (none while
/// pending), the claim is still licensed past the gate — where its twin above expired neutrally — and at the session's deadline the
/// producer's withholding is confirmed (`ProducerWithholding`): the expiry never closed the session.
#[test]
fn a_pending_da_session_holds_a_v3_s2_licence_past_its_gate_and_the_default_wins() {
    let mut w = V3::new();
    let (id, _, gate) = s2_licensed(&mut w, 81);
    let seats: Vec<PalwBondKeyV2> = w.panel_seats(&id).into_iter().map(|(b, _)| b).collect();
    let accuser = w.outsider(&seats);
    let (producer, _, _) = floor_producer(&w.c.p);
    let collateral = w.c.s.bond(&producer).unwrap().collateral;
    w.step_with(&[da_accuse(id, accuser, 3)]);
    assert!(w.c.s.palw_v3_s2_licence_held_v1(&id, w.c.s.claim(&id).unwrap()), "held by the pending session");
    assert_eq!(w.c.s.deadline_of(&id), None, "DL-1: no deadline while an accusation is pending");
    let session_deadline = w.c.s.da_session(&id, &accuser).expect("open").deadline_daa;
    assert!(session_deadline > gate, "the premise: the disclose window outlasts the S2 gate");
    w.at(gate + 1);
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::ReceiptLicensed { .. }),
        "past the gate, still licensed: {:?}",
        w.phase_v2(&id)
    );
    w.at(session_deadline);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::ReceiptLicensed { .. }));
    w.at(session_deadline + 1);
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "the default wins: {:?}",
        w.phase_v2(&id)
    );
    assert!(w.c.s.bond(&producer).unwrap().collateral < collateral, "the producer is charged");
}

/// **A court cleared while a DA session is still open re-arms nothing.** The court's close re-derives the deadline through DL-1,
/// which holds the licence while the session is pending; the session's own clock decides.
#[test]
fn a_court_cleared_under_a_pending_da_session_re_arms_no_gate() {
    let mut w = V3::new();
    let (id, _, gate) = s2_licensed(&mut w, 82);
    let seats: Vec<PalwBondKeyV2> = w.panel_seats(&id).into_iter().map(|(b, _)| b).collect();
    let accuser = w.outsider(&seats);
    let challenger = genesis_bonds(&w.c.p)
        .iter()
        .map(|(k, _, _)| *k)
        .find(|k| *k != floor_producer(&w.c.p).0 && !seats.contains(k) && *k != accuser)
        .expect("a second bond outside");
    let session = court_session_of(&w.c.s, id, challenger);
    w.step_with(&[da_accuse(id, accuser, 3), court_opened(&w.c.s, id, challenger)]);
    assert_eq!(w.c.s.deadline_of(&id), None);
    w.step_with(&[court_cleared(session)]);
    assert!(w.c.s.court_session(&session).is_none(), "the court closed");
    assert_eq!(w.c.s.deadline_of(&id), None, "the DA session still holds the licence: DL-1 re-arms no gate");
    w.at(gate + 2);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", w.phase_v2(&id));
    let session_deadline = w.c.s.da_session(&id, &accuser).expect("open").deadline_daa;
    w.at(session_deadline + 1);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }));
}

// ---------------------------------------------------------------------------------------------------------------------------
// The receipt window's edge
// ---------------------------------------------------------------------------------------------------------------------------

/// **An accusation inside the receipt window holds it; one carried by the block whose pre-object stage redraws is a late one.**
/// The window is `bound + 3`. (a) A session opened at `bound + 3` (its last DAA) pauses the clock: no redraw at `bound + 4` or after.
/// (b) A session carried by the block at `bound + 4` lands AFTER that block's stage 2f redrew the Panel (the decision preceded it):
/// it lands on the redrawn claim — and from then on it holds the new Panel's window too, to its default.
#[test]
fn an_accusation_inside_the_window_holds_it_and_one_in_the_redrawing_block_is_late_but_holds_the_redraw() {
    // (a)
    let mut w = V3::new();
    let (id, bound) = w.run_to_bound(90);
    let seats: Vec<PalwBondKeyV2> = w.panel_seats(&id).into_iter().map(|(b, _)| b).collect();
    let accuser = w.outsider(&seats);
    w.step_at(bound + 3, &[da_accuse(id, accuser, 3)], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.at(bound + 4);
    w.at(bound + 12);
    assert_eq!(w.record(&id).binding_history.len(), 1, "(a) no redraw: the session opened inside the window holds it");
    // (b) Three seats a round, so a redraw finds fresh operators (seven public bonds).
    let mut policy = engine_policy();
    policy.seat_count = 3;
    let mut w = V3::with(policy);
    let (id, bound) = w.run_to_bound(91);
    let first: Vec<PalwBondKeyV2> = w.panel_seats(&id).into_iter().map(|(b, _)| b).collect();
    let accuser = w.outsider(&first);
    // The redrawing block carries the accusation (the redraw may seat the accuser — then its session is a seat's; it holds alike).
    w.step_at(bound + 4, &[da_accuse(id, accuser, 3)], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 2, "(b) the block's stage 2f redrew before its objects");
    assert!(w.c.s.da_session(&id, &accuser).is_some(), "(b) the late accusation lands on the redrawn claim");
    let deadline = w.c.s.da_session(&id, &accuser).unwrap().deadline_daa;
    w.at(bound + 4 + 10);
    assert_eq!(w.record(&id).binding_history.len(), 2, "(b) from then on the session holds the new window: no expiry");
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { .. }));
    w.at(deadline + 1);
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "{:?}",
        w.phase_v2(&id)
    );
}
