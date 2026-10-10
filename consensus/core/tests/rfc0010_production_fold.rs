//! **RFC-0010's production fold, through testnet-12's own transition** (`apply_palw_transition_v7` with the extras the processor
//! resolves, [`common::Chain`]'s fixture): the engine as a Some-only sub-state of `PalwChainStateV2`, the V2 receipt/court
//! handoff, the one exposure ledger and the non-fraud terminations.
//!
//! **These tests bypass `validate_palw_v2`, and say so.** `palw_permissionless_panel_v1` cannot be armed on any network
//! (`rfc0010_permissionless_panel.rs` pins that refusal): the beacon is `BEACON_UNAVAILABLE` and its bias review is external.
//! Here the fence is set on a copy of the launch params and mirrored (`sync_palw_permissionless_panel_v1`) exactly as a future,
//! reviewed release would, and the beacon is a REFERENCE history ([`PalwPanelV3BeaconSourceV1::Reference`]) carrying one
//! `PanelIndependent` source — the only way to exercise the verification path, since no such Final exists on a real chain.
//! Nothing here is evidence that the permissionless Panel is complete.
//!
//! Every block is checked as `Chain::step_at` checks one: its delta re-applies to the parent and reverts to it, and the child's
//! carriage reloads under its committed root (`into_state`, which runs the engine's own consistency check).

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
    PalwBlockContextV2, PalwDeltaEntryV2, PalwPanelV3BeaconSourceV1, PalwPanelV3InputsV1, PalwStateDeltaV2, PalwStateV2Error,
    PalwVoidReasonV2,
};
use misaka_palw_challenge::{FinalPathV1, PostCommitChallengePolicyV1, WorkBeaconStateV1, WorkFinalEventV1, WorkSourceKindV1};
use misaka_palw_challenge::{collect_work_beacon_v1, policy::reference_policy_v1};

const FENCE: u64 = 1_000;

fn challenge_policy() -> PostCommitChallengePolicyV1 {
    // k = 1 independent useful work, accepted one DAA after the epoch starts, settled inside ten, locked one deeper.
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

/// A chain on testnet-12 with the permissionless Panel armed at [`FENCE`] on a copy of the params (see the module doc).
struct V3 {
    c: Chain,
    inputs: PalwPanelV3InputsV1,
}

impl V3 {
    fn new() -> Self {
        Self::with(engine_policy(), FENCE)
    }

    fn with(policy: PanelPolicyV1, fence: u64) -> Self {
        let mut p = t12();
        p.palw_permissionless_panel_v1 = Some(PalwPermissionlessPanelV1 { activation: ForkActivation::new(fence), policy });
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

    fn fold(
        &self,
        parent: &PalwChainStateV2,
        x: &PalwBlockContextV2,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
    ) -> Result<(PalwChainStateV2, PalwStateDeltaV2, Vec<(Hash64, String)>), PalwStateV2Error> {
        fold_with(&self.c.p, &self.c.sp, parent, x, objects, work, key, &self.extras(x.daa_score))
    }

    /// One block at `daa`, folded and checked three ways (the delta re-applies and reverts, the carriage reloads).
    fn step_at(
        &mut self,
        daa: u64,
        objects: &[PalwConsensusObjectV2],
        work: PalwBlockWorkV3<'_>,
        key: Hash64,
        subsidy: u64,
    ) -> PalwStateDeltaV2 {
        let x = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = self.c.s.clone();
        let (child, delta, skips) =
            self.fold(&parent, &x, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
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

    fn step(&mut self) {
        self.step_at(self.c.daa + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    fn step_with(&mut self, objects: &[PalwConsensusObjectV2]) {
        self.step_at(self.c.daa + 1, objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    }

    /// A floor attempt by the first genesis bond, accepted in its own block.
    fn floor_claim(&mut self, seed: u64) -> Hash64 {
        let (floor, _, _, _) = genesis_classes(&self.c.p)[0];
        let (bond, pubkey, operator) = floor_producer(&self.c.p);
        let pwu = self.c.floor_pwu(self.c.daa + 1);
        let (env, key, id) = junk_attempt(floor, bond, pubkey, &operator, pwu, seed, 0x10C0 + seed);
        self.step_at(self.c.daa + 1, &[], PalwBlockWorkV3::Attempt(&env), key, T12_BLOCK_SUBSIDY_SOMPI);
        assert!(self.c.s.claim(&id).is_some(), "the floor attempt is accepted");
        id
    }

    fn engine(&self) -> &kaspa_consensus_core::palw_permissionless_panel_v1::PermissionlessPanelStateV1 {
        self.c.s.panel_v3().expect("the engine exists past the fence")
    }

    fn record(&self, id: &Hash64) -> kaspa_consensus_core::palw_permissionless_panel_v1::ClaimRecordV3 {
        self.engine().claim_rows().get(id).expect("the engine tracks the claim").clone()
    }

    fn phase_v2(&self, id: &Hash64) -> PalwClaimPhaseV2 {
        self.c.s.claim(id).expect("the claim is live").phase.clone()
    }

    /// The reference history's one independent source for the epoch of `id`'s seal, and the proof it locks.
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
        // The branch's tip when the carrying block is folded: its parent, two DAA before it.
        let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &[event], release + 5).expect("a valid policy")
        else {
            panic!("the reference source locks a beacon");
        };
        BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(beacon.beacon()).unwrap() }
    }

    /// Drive a floor claim to `Bound`: accepted, sealed, certified, drawn. Returns the claim and the DAA it bound at.
    fn run_to_bound(&mut self, seed: u64) -> (Hash64, u64) {
        let id = self.floor_claim(seed);
        self.step(); // the checkpoint block
        self.step(); // seals against it
        assert_eq!(self.record(&id).phase, ClaimPhaseV3::Sealed);
        let release = self.record(&id).seal.unwrap().anchor_slot;
        let proof = self.proof_for(&id);
        self.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0); // the carrying block's parent
        self.step_at(
            release + 5,
            &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
            PalwBlockWorkV3::None,
            Hash64::default(),
            0,
        );
        assert!(self.engine().beacon_rows().contains_key(&self.record(&id).seal.unwrap().beacon_epoch), "the output is retained");
        // The contribution window closes at release + 8; the draw is due one DAA after it.
        self.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
        assert!(matches!(self.record(&id).phase, ClaimPhaseV3::Bound(_)), "{:?}", self.record(&id).phase);
        (id, release + 10)
    }
}

fn quorum(id: Hash64, seats: &[PalwBondKeyV2], daa: u64) -> PalwConsensusObjectV2 {
    PalwConsensusObjectV2::ReceiptLicensed { claim: id, receipts: seats.iter().map(|seat| valid(id, *seat, daa)).collect() }
}

fn seats_of_panel(s: &PalwChainStateV2, id: &Hash64) -> Vec<PalwBondKeyV2> {
    s.panel(id).expect("a bound panel").seats.iter().map(|seat| seat.bond).collect()
}

// ---------------------------------------------------------------------------------------------------------------------------
// Milestone 1: the fold, the one ledger, the receipt/court handoff
// ---------------------------------------------------------------------------------------------------------------------------

/// **Dormant is dormant.** The same chain with the fence unconfigured, configured at a height it never reaches, and configured at
/// a height it crosses before any claim: below the fence the engine does not exist and the state root is the unconfigured chain's
/// at every block; at the crossing the engine appears (root block, journal) and a claim accepted below the fence is not in it.
#[test]
fn below_the_fence_nothing_exists_and_the_roots_are_unchanged() {
    let mut plain = Chain::new(t12());
    let mut never = V3::with(engine_policy(), 10_000_000);
    let mut roots = Vec::new();
    for _ in 0..3 {
        plain.step(&[]);
        never.step();
        roots.push((plain.s.state_root(), never.c.s.state_root()));
    }
    for (a, b) in roots {
        assert_eq!(a, b, "a fence that is never reached leaves every root alone");
    }
    assert!(never.c.s.panel_v3().is_none());
    // Crossing it: the first block at or past the fence creates the engine; the journal is cursor-only (no claim yet).
    let mut crossing = V3::new();
    let before = crossing.c.s.state_root();
    let delta = crossing.step_at(FENCE, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(crossing.c.s.panel_v3().is_some());
    assert_ne!(crossing.c.s.state_root(), before);
    assert!(delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::PanelV3Cursor { old: None, new: Some(_) })));
    assert!(!delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::PanelV3Claim { .. })));
}

/// **A claim accepted below the fence keeps lane A; one accepted at it enters the engine.** The legacy claim stays `Provisional`
/// with its V2 bind deadline and is not the engine's; the V3 claim owns no V2 deadline (the engine is its clock), and the
/// binder's candidate list (`palw_claims_provisional_past_their_anchor_slot_v1`) does not offer it.
#[test]
fn the_rule_is_chosen_by_acceptance_and_the_v2_binder_never_sees_a_v3_claim() {
    let mut w = V3::new();
    w.c.daa = FENCE - 10;
    let legacy = w.floor_claim(1); // DAA FENCE - 9: below the fence
    assert!(w.c.s.panel_v3().is_none());
    assert!(w.c.s.deadline_of(&legacy).is_some(), "lane A: the V2 bind window is armed");
    w.c.daa = FENCE;
    let v3b = w.floor_claim(3); // DAA FENCE + 1: at the fence
    assert!(w.engine().claim_rows().contains_key(&v3b), "accepted past the fence: the engine's");
    assert!(!w.engine().claim_rows().contains_key(&legacy), "accepted below it: lane A's, for life");
    assert_eq!(w.c.s.deadline_of(&v3b), None, "the engine is the clock: no V2 bind deadline");
    assert!(w.c.s.deadline_of(&legacy).is_some());
    let offered = kaspa_consensus_core::palw_state_v2::palw_claims_provisional_past_their_anchor_slot_v1(&w.c.s, u64::MAX, 1);
    assert!(offered.contains(&legacy) && !offered.contains(&v3b), "the lane-A binder offers only the legacy claim");
    assert_eq!(w.phase_v2(&v3b), PalwClaimPhaseV2::Provisional);
    assert_eq!(w.record(&v3b).phase, ClaimPhaseV3::PendingSeal);
}

/// **The whole life of a V3 claim through testnet-12's fold**, every block delta-checked and carriage-reloaded: accepted,
/// sealed against the parent checkpoint, certified (a reference independent source), drawn from the public population, bound as
/// an ordinary V2 panel (anchor = the V3 seed), licensed by an ordinary V2 quorum, made `Final`, and released.
#[test]
fn a_v3_claim_is_bound_as_an_ordinary_v2_panel_then_licensed_and_finalised_by_v2() {
    let mut w = V3::new();
    let (id, bound_at) = w.run_to_bound(10);
    let ClaimPhaseV3::Bound(binding) = w.record(&id).phase else { unreachable!() };
    // The handoff: a V2 panel record anchored at the V3 seed, the seats of the binding in the binding's order.
    let panel = w.c.s.panel(&id).expect("a V2 panel record").clone();
    assert_eq!(panel.anchor, binding.panel_seed_v3);
    assert_eq!(panel.bound_daa, bound_at);
    let seats = seats_of_panel(&w.c.s, &id);
    assert_eq!(seats.len(), 5);
    assert_eq!(seats.len(), binding.seats.len());
    assert_eq!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { bound_daa: bound_at });
    // Population: public, never the producer, five distinct operators.
    let (producer, _, _) = floor_producer(&w.c.p);
    assert!(!seats.contains(&producer));
    let operators: BTreeSet<_> = panel.seats.iter().map(|seat| seat.operator_id).collect();
    assert_eq!(operators.len(), 5);
    // One ledger: the V2 duty row and each seat's reserved exposure are the binding's.
    let row = w.c.s.panel_duties_of(&id).expect("a duty row").clone();
    assert_eq!(row.len(), 5);
    for seat in &seats {
        assert_eq!(w.c.s.reserved_exposure(seat), binding.exposure as u128, "the seat reserved the engine's exposure, once");
        assert!(row.contains_key(seat));
    }
    assert_eq!(
        w.engine().reserved(&kaspa_consensus_core::palw_permissionless_panel_v1::BondIdV1::from(seats[0])),
        binding.exposure as u128
    );
    // V2's own deadline machinery does not clock a V3 claim.
    assert_eq!(w.c.s.deadline_of(&id), None);

    // The receipt door is V2's: an ordinary quorum licenses the claim; the seats lock like V2 seats.
    w.step_with(&[quorum(id, &seats, bound_at + 1)]);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::ReceiptLicensed { .. }), "{:?}", w.phase_v2(&id));
    for seat in &seats {
        assert!(w.c.s.slashable_lock(*seat, id).is_some(), "a V3 seat's Valid is a slashable lock, exactly as a V2 seat's");
    }
    // The engine releases on the next block (it sees the claim left its Panel); the V2 duty stays until Final.
    w.step();
    assert!(matches!(w.record(&id).phase, ClaimPhaseV3::Released { .. }));
    assert!(w.c.s.panel_duties_of(&id).is_some(), "V2 holds the duty until Final");
    assert_eq!(w.engine().reserved(&kaspa_consensus_core::palw_permissionless_panel_v1::BondIdV1::from(seats[0])), 0);

    // Final, by V2's sweep. The duty and its exposure go with it.
    let deadline = w.c.s.deadline_of(&id).expect("a licensed claim owes its Final deadline");
    w.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::Final { .. }));
    assert!(w.c.s.panel_duties_of(&id).is_none());
    for seat in &seats {
        assert_eq!(w.c.s.reserved_exposure(seat), 0);
    }
}

/// **A V3-bound claim is accusable and defaultable by an ordinary public bond outside the Panel** — the G14 property the handoff
/// exists for. Before the bind the claim has no panel and is `DaClaimNotAccusable` (as a V2 claim is before its bind); after it, a
/// bond that is neither the producer nor a seat opens a data-availability session, nobody answers, and the V2 sweep convicts the
/// producer (`ProducerWithholding`): the claim voids, the producer is slashed, no seat is charged. The engine releases the claim.
/// **The engine's receipt clock never runs against a pending DA demand (G14).** A non-seat session pauses nothing in V2 (V3S-08), but
/// here the policy's 3-DAA receipt window would have redrawn the Panel and then expired the claim `PanelUnavailable` (uncharged,
/// the session closed neutrally) long before the default: the clock is paused while ANY session is open, so the outcome is the DA
/// default's, `ProducerWithholding`.
#[test]
fn a_non_seat_public_bond_accuses_and_convicts_a_v3_bound_claim_exactly_as_it_would_a_v2_claim() {
    let mut w = V3::new();
    let id = w.floor_claim(20);
    let (producer, _, _) = floor_producer(&w.c.p);
    let accuser = genesis_bonds(&w.c.p).iter().map(|(k, _, _)| *k).find(|k| *k != producer).unwrap();
    // Unbound: no panel record — the accusation is refused, as V2 refuses one before the bind.
    let refused = {
        let parent = w.c.s.clone();
        let x = ctx(0xCA_0000 + w.c.daa + 1, w.c.daa + 1, w.c.daa + 1, 0);
        w.fold(&parent, &x, &[da_accuse(id, accuser, 3)], PalwBlockWorkV3::None, Hash64::default())
    };
    assert!(matches!(refused, Err(PalwStateV2Error::DaClaimNotAccusable(_))), "{refused:?}");

    let mut w = V3::new();
    let (id, bound_at) = w.run_to_bound(20);
    let seats = seats_of_panel(&w.c.s, &id);
    let outsider = genesis_bonds(&w.c.p)
        .iter()
        .map(|(k, _, _)| *k)
        .find(|k| *k != producer && !seats.contains(k))
        .expect("a bond outside the Panel");
    let collateral_before = w.c.s.bond(&producer).unwrap().collateral;
    w.step_with(&[da_accuse(id, outsider, 3)]);
    assert!(w.c.s.da_session(&id, &outsider).is_some(), "the public bond's session is open");
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { .. }), "a non-seat session pauses nothing in V2 (V3S-08)");
    // The V3 window (3 DAA, one redraw) would end the claim at bound + 9: the open session holds the clock past it.
    let deadline = w.c.s.da_session(&id, &outsider).unwrap().deadline_daa;
    assert!(deadline > bound_at + 100, "the disclose window outlasts the engine's whole receipt budget");
    w.step_at(bound_at + 20, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 1, "no redraw while a session is open");
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { .. }), "and no expiry");
    w.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 1, "no redraw while the session runs");
    w.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(w.phase_v2(&id), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }),
        "{:?}",
        w.phase_v2(&id)
    );
    assert!(w.c.s.bond(&producer).unwrap().collateral < collateral_before, "the producer is slashed");
    for seat in &seats {
        assert_eq!(w.c.s.bond(seat).unwrap().slashed, 0, "no seat signed anything, none is charged");
        assert_eq!(w.c.s.reserved_exposure(seat), 0, "the duty is released with the void");
    }
    assert!(w.c.s.panel_duties_of(&id).is_none());
    w.step();
    assert!(matches!(w.record(&id).phase, ClaimPhaseV3::Released { .. }), "the engine sees the claim ended");
}

/// **One exposure ledger, both directions.** Collateral sized so that exactly one panel's duty fits a seat's ceiling: a V3 claim
/// bound first leaves no seat room for a legacy (lane-A) binding of the same bonds, and a legacy duty held first leaves no room
/// for the V3 draw — which ends non-fraud (`PermissionlessNoCapablePanel`), never over-reserving. The engine's own reservations
/// are the V2 duty rows, not a second ledger.
#[test]
fn a_legacy_duty_and_a_v3_duty_cannot_spend_the_same_collateral() {
    // Probe the per-seat price on the full-collateral world.
    let mut probe = V3::new();
    let (id, _) = probe.run_to_bound(30);
    let ClaimPhaseV3::Bound(binding) = probe.record(&id).phase else { unreachable!() };
    let eligibility = binding.exposure as u128;
    assert!(eligibility > 0);

    // The world: the work ceiling is 1 permille of collateral, and every non-producer bond holds exactly `1000 · eligibility`, so its
    // ceiling holds exactly ONE duty of this price. (The producer is made rich: its claims reserve their own weight.)
    let (producer, _, _) = floor_producer(&probe.c.p);
    let rich = |w: &mut V3| {
        w.c.sp = w.c.sp.clone().with_fp_exposure_ceiling(1).expect("a one-permille ceiling");
        let collateral = (eligibility * 1000) as u64;
        w.c.s = edited(&w.c.sp, &w.c.s, |carriage| {
            for (key, bond) in carriage.bonds.iter_mut() {
                bond.collateral = if *key == producer { RICH } else { collateral };
            }
        });
    };

    // (a) V3 first: the claim binds (room for exactly one duty), then a second V3 claim finds only the two bonds that hold none.
    let mut w = V3::new();
    rich(&mut w);
    let first = w.floor_claim(31);
    let second = w.floor_claim(32);
    w.step();
    w.step(); // both seal
    let release = w.record(&first).seal.unwrap().anchor_slot;
    assert_eq!(w.record(&second).seal.unwrap().anchor_slot, release);
    let seats_hold = |w: &V3, id: &Hash64| w.c.s.panel_duties_of(id).map(|row| row.len()).unwrap_or(0);
    assert_eq!(seats_hold(&w, &first), 0);
    // Certify the epoch, then let the due draws run in acceptance order.
    let proof = w.proof_for(&first);
    w.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.step_at(
        release + 5,
        &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
    );
    w.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(
        matches!(w.record(&first).phase, ClaimPhaseV3::Bound(_)),
        "the first in acceptance order binds: {:?} / {:?}",
        w.record(&first).phase,
        w.record(&first).claim.required_exposure
    );
    assert_eq!(
        w.record(&second).phase,
        ClaimPhaseV3::Voided { daa: release + 10, reason: NonFraudReasonV1::NoCapablePanel },
        "the second finds seven bonds, five of them on duty at their ceiling: no capable panel"
    );
    assert!(matches!(w.phase_v2(&second), PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::PermissionlessNoCapablePanel, .. }));
    for seat in seats_of_panel(&w.c.s, &first) {
        assert_eq!(w.c.s.reserved_exposure(&seat), eligibility, "no seat is over-reserved");
    }
    // The non-fraud end holds nothing: the producer is not slashed, and its voided claim reserves nothing.
    assert_eq!(w.c.s.bond(&producer).unwrap().slashed, 0);
    assert!(w.c.s.panel_duties_of(&second).is_none());

    // (b) The lane-A drain first: a legacy claim (accepted below the fence) bound by V2's own PanelBound holds five seats at
    // their ceiling; the V3 claim, drawn afterwards from the very same bonds, finds the same two and ends non-fraud.
    let mut w = V3::new();
    rich(&mut w);
    w.c.daa = FENCE - 20;
    let legacy = w.floor_claim(33); // DAA FENCE - 19
    let legacy_seats = w.c.floor_seats();
    w.step_with(&[PalwConsensusObjectV2::PanelBound { claim: legacy, anchor: h(0xAC_0000), seats: seats_of(&legacy_seats) }]);
    assert!(matches!(w.phase_v2(&legacy), PalwClaimPhaseV2::PanelBound { .. }), "lane A binds the legacy claim");
    let held = legacy_seats.iter().map(|(k, _)| w.c.s.reserved_exposure(k)).collect::<Vec<_>>();
    assert!(held.iter().all(|h| *h > 0 && *h <= eligibility), "the legacy duty is on the ledger: {held:?}");
    w.c.daa = FENCE;
    let v3 = w.floor_claim(34);
    w.step();
    w.step();
    let release = w.record(&v3).seal.unwrap().anchor_slot;
    let proof = w.proof_for(&v3);
    w.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.step_at(
        release + 5,
        &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
    );
    w.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(
        w.record(&v3).phase,
        ClaimPhaseV3::Voided { daa: release + 10, reason: NonFraudReasonV1::NoCapablePanel },
        "five bonds hold the legacy duty: the V3 draw cannot reuse their collateral"
    );
    for ((key, _), before) in legacy_seats.iter().zip(&held) {
        assert_eq!(w.c.s.reserved_exposure(key), *before, "the V3 draw left the legacy duty untouched");
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// Task 5: the non-fraud terminations hold nothing and charge nothing
// ---------------------------------------------------------------------------------------------------------------------------

/// What every non-fraud end shares: the V2 claim is `Voided` under the versioned reason, the engine says why, the producer is not
/// slashed or struck, its collateral is whole, no seat holds a duty or an exposure, and the engine releases nothing it still counts.
fn assert_non_fraud_end(w: &V3, id: &Hash64, v2: PalwVoidReasonV2, v3: NonFraudReasonV1, seats: &[PalwBondKeyV2]) {
    let (producer, _, _) = floor_producer(&w.c.p);
    assert!(matches!(w.phase_v2(id), PalwClaimPhaseV2::Voided { reason, .. } if reason == v2), "{:?}", w.phase_v2(id));
    assert!(matches!(w.record(id).phase, ClaimPhaseV3::Voided { reason, .. } if reason == v3), "{:?}", w.record(id).phase);
    let bond = w.c.s.bond(&producer).unwrap();
    assert_eq!(bond.slashed, 0, "nobody is slashed for a seal, a beacon or a population the chain could not provide");
    assert!(w.c.s.withholding_strikes(&producer).is_none(), "no strike");
    assert_eq!(w.c.s.reserved_exposure(&producer), 0, "the producer's reservation returned at once (no E-4 hold)");
    assert!(w.c.s.panel_duties_of(id).is_none());
    for seat in seats {
        assert_eq!(w.c.s.reserved_exposure(seat), 0, "no seat holds an exposure");
        assert!(w.c.s.slashable_lock(*seat, *id).is_none());
    }
    assert!(w.engine().live_claims().all(|(live, _)| live != id));
}

#[test]
fn seal_unavailable_ends_a_claim_that_never_reached_its_checkpoint_depth() {
    let mut policy = engine_policy();
    policy.seal_depth_blocks = 1_000; // never reached inside seal_wait_daa
    let mut w = V3::with(policy, FENCE);
    let id = w.floor_claim(41);
    w.step_at(FENCE + 51, &[], PalwBlockWorkV3::None, Hash64::default(), 0); // = accepted + seal_wait: still waiting
    assert_eq!(w.record(&id).phase, ClaimPhaseV3::PendingSeal);
    w.step_at(FENCE + 52, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_non_fraud_end(&w, &id, PalwVoidReasonV2::SealUnavailable, NonFraudReasonV1::SealUnavailable, &[]);
}

#[test]
fn beacon_unavailable_ends_a_sealed_claim_when_no_proof_arrived_and_no_block_hash_is_a_fallback() {
    let mut w = V3::new();
    let id = w.floor_claim(42);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    // The window runs to release + 8 inclusive; nothing is carried; a heartbeat-style empty block at the deadline decides nothing.
    w.step_at(release + 8, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).phase, ClaimPhaseV3::Sealed, "the contribution window is still open at its last DAA");
    w.step_at(release + 9, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_non_fraud_end(&w, &id, PalwVoidReasonV2::BeaconUnavailable, NonFraudReasonV1::BeaconUnavailable, &[]);
    assert!(w.engine().beacon_rows().is_empty());
}

#[test]
fn no_capable_panel_ends_a_claim_the_public_population_cannot_seat() {
    let mut policy = engine_policy();
    policy.seat_count = 8; // seven non-producer genesis bonds exist
    let mut w = V3::with(policy, FENCE);
    let id = w.floor_claim(43);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    let proof = w.proof_for(&id);
    w.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.step_at(
        release + 5,
        &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
    );
    w.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_non_fraud_end(&w, &id, PalwVoidReasonV2::PermissionlessNoCapablePanel, NonFraudReasonV1::NoCapablePanel, &[]);
}

/// A Panel that says nothing is redrawn from the original seed (disjoint, public seats, the previous round's duties released,
/// the claim marked as V2 marks a redrawn one), and when the retries are spent the claim EXPIRES — V2's lane-PL `PanelUnavailable`,
/// uncharged — rather than burning the producer for verifiers that did not show up.
#[test]
fn panel_unavailable_redraws_from_the_original_seed_then_expires_uncharged() {
    let mut policy = engine_policy();
    policy.seat_count = 3; // seven public bonds: a redraw needs fresh operators, so three then three of the remaining four
    let mut w = V3::with(policy, FENCE);
    let (id, bound_at) = w.run_to_bound(44);
    let first = seats_of_panel(&w.c.s, &id);
    let ClaimPhaseV3::Bound(b0) = w.record(&id).phase else { unreachable!() };
    // The window is bound + 3: at bound + 3 nothing is due, one DAA later the engine redraws.
    w.step_at(bound_at + 3, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 1);
    w.step_at(bound_at + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let ClaimPhaseV3::Bound(b1) = w.record(&id).phase else { panic!("redrawn: {:?}", w.record(&id).phase) };
    assert_eq!((b1.retry_index, b1.panel_seed_v3), (1, b0.panel_seed_v3), "the original seed, the next round");
    let second = seats_of_panel(&w.c.s, &id);
    assert!(second.iter().all(|seat| !first.contains(seat)), "alternates are never reused");
    assert_eq!(second.len(), 3);
    // The V2 side moved with it: a fresh panel record at the redraw, one duty row, the old round's exposure gone.
    assert_eq!(w.c.s.panel(&id).unwrap().bound_daa, bound_at + 4);
    assert_eq!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { bound_daa: bound_at + 4 });
    assert_eq!(w.c.s.claim(&id).unwrap().rebound_daa, Some(bound_at + 4), "marked as a redrawn claim");
    for seat in &first {
        if !second.contains(seat) {
            assert_eq!(w.c.s.reserved_exposure(seat), 0, "the previous round's seat left duty");
        }
    }
    for seat in &second {
        assert_eq!(w.c.s.reserved_exposure(seat), b1.exposure as u128);
    }
    // Spent: the next window's end is the expiry.
    w.step_at(bound_at + 7, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(w.record(&id).phase, ClaimPhaseV3::Bound(_)));
    w.step_at(bound_at + 8, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let all: Vec<_> = first.iter().chain(second.iter()).copied().collect();
    assert_non_fraud_end(&w, &id, PalwVoidReasonV2::PanelUnavailable, NonFraudReasonV1::PanelUnavailable, &all);
}

// ---------------------------------------------------------------------------------------------------------------------------
// Task 6 at the fold: the carried proof is evidence, never a command
// ---------------------------------------------------------------------------------------------------------------------------

/// A bad carried proof — forged output, another epoch's, garbage bytes, a scheme nobody approved — is DROPPED and the block stands:
/// the block folds, the engine retains nothing, and the claim ends `BeaconUnavailable` as if nothing had been carried.
#[test]
fn a_bad_carried_proof_is_dropped_and_the_block_stands() {
    let mut w = V3::new();
    let id = w.floor_claim(45);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    let good = w.proof_for(&id);
    let mut forged_output = good.clone();
    forged_output.output = Hash64::from_u64_word(7);
    let mut other_epoch = good.clone();
    other_epoch.epoch += 1;
    let garbage = BeaconProofV1 { proof: vec![1, 2, 3], ..good.clone() };
    w.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    // One block carries all three (the policy admits two per block; the third is not even queued) and stands.
    let objects: Vec<_> = [forged_output, other_epoch, garbage]
        .into_iter()
        .map(|p| PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(p) })
        .collect();
    let delta = w.step_at(release + 5, &objects, PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(w.engine().beacon_rows().is_empty(), "nothing verified, nothing retained");
    assert!(!delta.entries.iter().any(|e| matches!(e, PalwDeltaEntryV2::PanelV3Beacon { .. })));
    // A scheme the release does not approve: the same good bytes, no approved policy.
    let mut none = V3::new();
    none.inputs.approved_beacons.clear();
    let id2 = none.floor_claim(45);
    none.step();
    none.step();
    let release2 = none.record(&id2).seal.unwrap().anchor_slot;
    let good2 = none.proof_for(&id2);
    none.step_at(release2 + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    none.step_at(
        release2 + 5,
        &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(good2) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
    );
    assert!(none.engine().beacon_rows().is_empty());
    none.step_at(release2 + 9, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_non_fraud_end(&none, &id2, PalwVoidReasonV2::BeaconUnavailable, NonFraudReasonV1::BeaconUnavailable, &[]);
    // …and the same good bytes, carried on the honest chain, verify (the control).
    let mut control = V3::new();
    let (cid, _) = control.run_to_bound(45);
    assert!(matches!(control.record(&cid).phase, ClaimPhaseV3::Bound(_)));
}

/// The second lock: below the fence the object is a payload an older build cannot decode (the acceptance walk drops it first); the
/// fold refuses it.
#[test]
fn the_beacon_object_is_refused_by_name_below_the_fence() {
    let mut w = V3::with(engine_policy(), 10_000_000);
    let proof = BeaconProofV1 { epoch: 1, output: Hash64::from_u64_word(1), proof: vec![] };
    let object = PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) };
    assert!(kaspa_consensus_core::palw_state_v2::palw_object_is_panel_v3_v1(&object));
    let x = ctx(0xCA_0000 + w.c.daa + 1, w.c.daa + 1, w.c.daa + 1, 0);
    let parent = w.c.s.clone();
    let refused = w.fold(&parent, &x, &[object], PalwBlockWorkV3::None, Hash64::default());
    assert!(matches!(refused, Err(PalwStateV2Error::CarriageInconsistent(_))), "{refused:?}");
    w.step();
}

/// **Today's chain yields `BEACON_UNAVAILABLE`.** Every Final the V2 lattice writes is Panel-licensed (a claim reaches `Final` only
/// through a Panel's licence), so the events the chain derives are all `PanelLicensed` and none may seed a Panel assignment — even
/// with a scheme approved and the claim's class forced G14-eligible. The fold's own consequence: with the chain as the source and
/// no proof possible, the claim ends `BeaconUnavailable`, non-fraud.
#[test]
fn on_todays_chain_no_final_is_panel_independent_so_the_beacon_is_unavailable() {
    use kaspa_consensus_core::palw_panel_beacon_v1::{PanelBeaconHistoryV1, PanelBeaconStateV1, panel_beacon_state_v1};
    use kaspa_consensus_core::palw_state_v2::{ChainPanelBeaconHistoryV1, palw_panel_v3_final_events_v1};
    let mut w = V3::new();
    let (id, bound_at) = w.run_to_bound(46);
    let seats = seats_of_panel(&w.c.s, &id);
    w.step_with(&[quorum(id, &seats, bound_at + 1)]);
    let deadline = w.c.s.deadline_of(&id).unwrap();
    w.step_at(deadline + 1, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::Final { .. }));
    let floor = w.inputs.floor_class;
    for base in [floor, Hash64::default()] {
        let events = palw_panel_v3_final_events_v1(&w.c.s, base);
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0].final_path, FinalPathV1::PanelLicensed { .. }), "a Final is Panel-licensed on this lattice");
        assert_eq!(events[0].kind, if base == floor { WorkSourceKindV1::Base0Fallback } else { WorkSourceKindV1::RealUsefulWork });
    }
    // The chain's own history: no eligible profile, and with the class forced eligible the Panel-licensed path still refuses it.
    let challenge = challenge_policy();
    let mirror = *w.c.sp.panel_v3().unwrap();
    let req = BeaconRequestV1 {
        network: mirror.network,
        ruleset: mirror.ruleset,
        scheme: mirror.policy.beacon_scheme,
        epoch: 11,
        release_daa: bound_at + 1,
        deadline_daa: bound_at + 9,
    };
    let source = PalwPanelV3BeaconSourceV1::Chain;
    let history = ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), w.c.s.panel_v3(), &source, bound_at + 2);
    assert!(history.eligible_profiles(0).is_empty(), "no class is G14-complete in consensus");
    let unavailable = panel_beacon_state_v1(std::slice::from_ref(&challenge), &history, &req, bound_at + 100).unwrap();
    assert!(matches!(unavailable, PanelBeaconStateV1::Accumulator(WorkBeaconStateV1::Unavailable { have: 0, need: 1 })));
    let class = w.c.s.claim(&id).unwrap().class_id.as_bytes();
    let forced = PalwPanelV3BeaconSourceV1::Reference {
        events: palw_panel_v3_final_events_v1(&w.c.s, Hash64::default()),
        eligible_profiles: BTreeSet::from([class]),
        works: Vec::new(),
        sealed: Vec::new(),
    };
    let history = ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), w.c.s.panel_v3(), &forced, bound_at + 2);
    let still = panel_beacon_state_v1(std::slice::from_ref(&challenge), &history, &req, bound_at + 100).unwrap();
    assert!(matches!(still, PanelBeaconStateV1::Accumulator(WorkBeaconStateV1::Unavailable { have: 0, need: 1 })), "{still:?}");

    // The fold's consequence, with the chain as the source: no proof can exist, the claim ends BeaconUnavailable.
    let mut chain_source = V3::new();
    chain_source.inputs.beacon_source = PalwPanelV3BeaconSourceV1::Chain;
    let cid = chain_source.floor_claim(47);
    chain_source.step();
    chain_source.step();
    let release = chain_source.record(&cid).seal.unwrap().anchor_slot;
    chain_source.step_at(release + 9, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_non_fraud_end(&chain_source, &cid, PalwVoidReasonV2::BeaconUnavailable, NonFraudReasonV1::BeaconUnavailable, &[]);
}

/// **An open court session on a V3-bound claim holds the engine's receipt clock too** (G14): an accusation of the executor must
/// reach its verdict or its own backstop before a non-fraud expiry can close the claim — and the session with it, neutrally.
#[test]
fn an_open_court_session_holds_the_receipt_clock() {
    let mut w = V3::new();
    let (id, bound_at) = w.run_to_bound(48);
    let (producer, _, _) = floor_producer(&w.c.p);
    let seats = seats_of_panel(&w.c.s, &id);
    let challenger = genesis_bonds(&w.c.p)
        .iter()
        .map(|(k, _, _)| *k)
        .find(|k| *k != producer && !seats.contains(k))
        .expect("a bond outside the Panel");
    w.step_with(&[court_opened(&w.c.s, id, challenger)]);
    assert!(w.c.s.open_courts_of(&id) > 0, "the court is open");
    // The window (3 DAA, one redraw) is long past at bound + 50; the session holds it.
    w.step_at(bound_at + 50, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 1, "no redraw while a court session is open");
    // The court is the accusation's own clock: with its responder silent it ends the claim by CONVICTION, not by a non-fraud expiry.
    let phase = w.phase_v2(&id);
    assert!(
        matches!(
            phase,
            PalwClaimPhaseV2::PanelBound { .. }
                | PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtDefault, .. }
        ),
        "the court decides, never the V3 expiry: {phase:?}"
    );
    eprintln!("[court-pause] phase at bound + 50: {phase:?}");
}

// ---------------------------------------------------------------------------------------------------------------------------
// Observation: what a client reads of a V3 claim (RPC op 220 serves exactly these records)
// ---------------------------------------------------------------------------------------------------------------------------

use kaspa_consensus_core::palw_permissionless_panel_v1::{
    PANEL_V3_OBSERVATION_VERSION_V1, panel_v3_claim_status_v1, panel_v3_overview_v1,
};

fn status(w: &V3, id: &Hash64) -> kaspa_consensus_core::palw_permissionless_panel_v1::PanelV3ClaimStatusV1 {
    panel_v3_claim_status_v1(&w.c.s, &w.c.sp, id).expect("the state holds the claim")
}

/// **The read follows one claim from acceptance to its end**: the rule, the V2 phase, the engine phase, the seal, the frozen snapshot,
/// the epoch's certified-output state, the assignment (seed, seats, exposure, witness block), the retries and the terminal reason —
/// each only once the chain has fixed it. It is a pure read of the state: reading changes nothing.
#[test]
fn the_observation_follows_a_claim_from_acceptance_to_release() {
    let mut w = V3::new();
    let before = w.c.s.state_root();
    assert!(panel_v3_claim_status_v1(&w.c.s, &w.c.sp, &Hash64::from_bytes([9; 64])).is_none(), "an unknown claim is not a status");
    assert!(!panel_v3_overview_v1(&w.c.s).active, "no engine below the fence");
    let id = w.floor_claim(60);
    let s = status(&w, &id);
    assert_eq!(
        (s.version, s.rule, s.v2_phase.as_str(), s.engine_phase),
        (PANEL_V3_OBSERVATION_VERSION_V1, "permissionlessV3", "provisional", Some("pendingSeal"))
    );
    assert!(s.seal.is_none() && s.snapshot.is_none() && s.beacon.is_none() && s.assignment.is_none() && s.terminal.is_none());
    assert_eq!(s.acceptance_order, Some(w.record(&id).acceptance_order));

    w.step();
    w.step();
    let s = status(&w, &id);
    assert_eq!(s.engine_phase, Some("sealed"));
    let seal = s.seal.clone().expect("sealed");
    let snapshot = s.snapshot.clone().expect("the frozen snapshot");
    assert_eq!(snapshot.root, w.record(&id).snapshot.unwrap().root);
    assert!(snapshot.candidates > 0);
    let beacon = s.beacon.clone().expect("the epoch the seal named");
    assert_eq!(
        (beacon.epoch, beacon.release_daa, beacon.state, beacon.output),
        (seal.beacon_epoch, seal.anchor_slot, "collecting", None)
    );
    assert_eq!(beacon.deadline_daa, seal.anchor_slot + engine_policy().beacon_wait_daa);

    let release = seal.anchor_slot;
    let proof = w.proof_for(&id);
    let output = proof.output;
    w.step_at(release + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.step_at(
        release + 5,
        &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }],
        PalwBlockWorkV3::None,
        Hash64::default(),
        0,
    );
    let beacon = status(&w, &id).beacon.expect("beacon");
    assert_eq!((beacon.state, beacon.output), ("certified", Some(output)), "the retained certified output is shown");
    w.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let s = status(&w, &id);
    assert_eq!((s.engine_phase, s.v2_phase.as_str(), s.retries), (Some("bound"), "panelBound", 0));
    let assignment = s.assignment.clone().expect("the assignment");
    let ClaimPhaseV3::Bound(binding) = w.record(&id).phase else { unreachable!() };
    assert_eq!(assignment.seed, binding.panel_seed_v3);
    assert_eq!(assignment.beacon_id, binding.beacon_id);
    assert_eq!(assignment.seats.len(), 5);
    assert_eq!(assignment.exposure, binding.exposure.to_string());
    assert_eq!(assignment.bound_daa, release + 10);
    assert!(s.terminal.is_none());
    let overview = panel_v3_overview_v1(&w.c.s);
    assert!(overview.active);
    assert_eq!((overview.tracked_claims, overview.bound, overview.pending_seal), (1, 1, 0));
    assert_eq!(overview.certified_epochs, vec![seal.beacon_epoch]);

    let seats = seats_of_panel(&w.c.s, &id);
    w.step_with(&[quorum(id, &seats, release + 11)]);
    w.step();
    let s = status(&w, &id);
    assert_eq!((s.engine_phase, s.v2_phase.as_str()), (Some("released"), "receiptLicensed"));
    let terminal = s.terminal.expect("the end");
    assert_eq!((terminal.reason, terminal.fraud), ("RELEASED", false));
    assert!(s.assignment.is_some(), "the assignment stays readable after the release");

    assert_ne!(w.c.s.state_root(), before);
    let again = w.c.s.state_root();
    let _ = (status(&w, &id), panel_v3_overview_v1(&w.c.s));
    assert_eq!(w.c.s.state_root(), again, "reading a status changes nothing");
}

/// **The non-fraud ends read as such**: `BEACON_UNAVAILABLE` carries `fraud: false`, the beacon block says `unavailable` with no
/// output, and the V2 phase names the void. A redrawn Panel shows its retry count and keeps the original seed's seats' history.
#[test]
fn the_observation_names_a_non_fraud_end_and_counts_a_retry() {
    let mut w = V3::new();
    let id = w.floor_claim(61);
    w.step();
    w.step();
    let release = w.record(&id).seal.unwrap().anchor_slot;
    w.step_at(release + 9, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let s = status(&w, &id);
    assert_eq!(s.engine_phase, Some("voided"));
    assert!(s.v2_phase.starts_with("voided:"), "{}", s.v2_phase);
    let terminal = s.terminal.expect("terminal");
    assert_eq!((terminal.reason, terminal.fraud), ("BEACON_UNAVAILABLE", false));
    let beacon = s.beacon.expect("beacon");
    assert_eq!((beacon.state, beacon.output), ("unavailable", None));
    assert!(s.assignment.is_none());

    // A retry: the Panel never answers, the engine redraws once (from the original seed), and the count shows it.
    let mut policy = engine_policy();
    policy.seat_count = 3;
    let mut r = V3::with(policy, FENCE);
    let (rid, bound_at) = r.run_to_bound(62);
    assert_eq!(status(&r, &rid).retries, 0);
    r.step_at(bound_at + 4, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    let s = status(&r, &rid);
    assert_eq!(s.retries, 1, "one redraw so far");
    assert_eq!(s.assignment.expect("the new Panel").retry_index, 1);
}

/// **The read is stable**: version 1's JSON keys are pinned (a reader of version 1 ignores keys it does not know; keys are only
/// appended), and a legacy claim reads as `historicalLaneA` with no engine phase.
#[test]
fn the_observation_json_is_pinned_and_a_legacy_claim_reads_as_lane_a() {
    let mut w = V3::new();
    w.c.daa = FENCE - 10;
    let legacy = w.floor_claim(63);
    w.c.daa = FENCE;
    let v3 = w.floor_claim(64);
    let l = status(&w, &legacy);
    assert_eq!((l.rule, l.engine_phase, l.acceptance_order), ("historicalLaneA", None, None));
    assert!(l.seal.is_none() && l.assignment.is_none() && l.terminal.is_none());
    let json = serde_json::to_value(status(&w, &v3)).unwrap();
    let mut keys: Vec<&str> = json.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "acceptanceOrder",
            "acceptedDaa",
            "assignment",
            "beacon",
            "claimId",
            "enginePhase",
            "retries",
            "rule",
            "seal",
            "snapshot",
            "terminal",
            "v2Phase",
            "version"
        ]
    );
    assert_eq!(json["version"], 1);
    assert_eq!(json["rule"], "permissionlessV3");
    assert_eq!(json["enginePhase"], "pendingSeal");
    let overview = serde_json::to_value(panel_v3_overview_v1(&w.c.s)).unwrap();
    let mut keys: Vec<&str> = overview.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort();
    assert_eq!(
        keys,
        [
            "active",
            "bound",
            "certifiedEpochs",
            "daa",
            "entropyReady",
            "height",
            "pendingSeal",
            "policyId",
            "released",
            "retainedWorkIds",
            "sealed",
            "tip",
            "trackedClaims",
            "version",
            "voided"
        ]
    );
}

// ---------------------------------------------------------------------------------------------------------------------------
// Task 8 (fold level): a participant who is NOT in genesis
// ---------------------------------------------------------------------------------------------------------------------------

/// **A bond registered after genesis, by anyone, is a public candidate and is seated with no operator named.** The registration is
/// the ordinary `BondRegistered` (the harness folds it without the acceptance layer's ML-DSA check, like every test here); once it
/// has matured by the policy's `bond_maturity_daa` it is in the sealed snapshot, and with the seat count equal to the population
/// the draw MUST seat it. Nothing here names an operator: the population is the chain's bonds.
#[test]
fn a_bond_registered_after_genesis_is_in_the_snapshot_and_is_seated_with_no_operator_named() {
    let genesis_bonds: Vec<_> = V3::with(engine_policy(), FENCE).c.s.bonds_iter().map(|(key, _)| *key).collect();
    assert!(genesis_bonds.len() >= 6, "testnet-12 starts with its genesis bonds: {}", genesis_bonds.len());
    let mut policy = engine_policy();
    policy.seat_count = genesis_bonds.len() as u16; // the producer is excluded, the newcomer added: every candidate is seated
    let mut w = V3::with(policy, FENCE);
    let template = w.c.s.bonds_iter().next().map(|(_, record)| record.clone()).expect("a genesis bond");
    let newcomer = PalwBondKeyV2(kaspa_consensus_core::tx::TransactionOutpoint {
        transaction_id: kaspa_consensus_core::tx::TransactionId::from_u64_word(0xB0_0001),
        index: 0,
    });
    let register = PalwConsensusObjectV2::BondRegistered {
        bond: newcomer,
        pubkey: vec![0x5A; template.pubkey.len().max(32)],
        operator_pubkey: vec![0x6B; 32],
        collateral: template.collateral,
        payout_payload: Hash64::from_u64_word(0x9A77),
        capable_classes: template.capable_classes.clone(),
        signature: Vec::new(), // the acceptance layer's check; the harness folds the transition only
    };
    w.step_at(5, &[register], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(w.c.s.bond(&newcomer).is_some(), "registered");
    w.c.daa = FENCE;
    let (id, _) = w.run_to_bound(70);
    let snapshot = w.record(&id).snapshot.expect("the sealed snapshot");
    assert!(
        snapshot.candidates.iter().any(|c| c.bond == kaspa_consensus_core::palw_permissionless_panel_v1::BondIdV1::from(newcomer)),
        "the newcomer is in the frozen population"
    );
    let seats = seats_of_panel(&w.c.s, &id);
    assert_eq!(seats.len(), genesis_bonds.len());
    assert!(seats.contains(&newcomer), "seated: the population is the chain's bonds, not a named operator list");
    let panel = w.c.s.panel(&id).unwrap();
    let operators: BTreeSet<_> = panel.seats.iter().map(|seat| seat.operator_id).collect();
    assert_eq!(operators.len(), seats.len(), "distinct operators");
}

/// **A wrong snapshot is refused wherever it could enter.** In the fold the snapshot is host-derived — the producer carries none —
/// so the only door left is a stored/imported engine (a carriage, a checkpoint import): a frozen population whose candidate list, root,
/// policy, class or exclusion was touched does not reload under the committed state root, and the engine's own check names it.
#[test]
fn a_tampered_snapshot_does_not_reload_and_the_engine_refuses_it() {
    let mut w = V3::new();
    let id = w.floor_claim(71);
    w.step();
    w.step();
    assert_eq!(w.record(&id).phase, ClaimPhaseV3::Sealed);
    let committed = w.c.s.state_root();
    let intact = PalwStateCarriageV2::from_state(&w.c.s).into_state(&w.c.sp, Some(committed)).expect("the untouched carriage reloads");
    assert_eq!(intact, w.c.s);

    let tamper = |edit: &dyn Fn(&mut kaspa_consensus_core::palw_permissionless_panel_v1::ClaimRecordV3)| {
        let mut carriage = PalwStateCarriageV2::from_state(&w.c.s);
        let engine = carriage.panel_v3.as_mut().expect("the engine rides the carriage");
        let mut record = engine.claim_rows().get(&id).unwrap().clone();
        edit(&mut record);
        engine.put_claim_row(id, Some(record));
        // The engine's own consistency check refuses it…
        let own = carriage.panel_v3.as_ref().unwrap().check_consistency();
        // …and the carriage does not reload under the committed root.
        let reloaded = carriage.into_state(&w.c.sp, Some(committed));
        (own, reloaded)
    };
    let cases: Vec<(&str, Box<dyn Fn(&mut kaspa_consensus_core::palw_permissionless_panel_v1::ClaimRecordV3)>)> = vec![
        (
            "a candidate removed",
            Box::new(|r| {
                r.snapshot.as_mut().unwrap().candidates.pop();
            }),
        ),
        (
            "a candidate's collateral raised",
            Box::new(|r| {
                r.snapshot.as_mut().unwrap().candidates[0].collateral += 1;
            }),
        ),
        (
            "the root replaced",
            Box::new(|r| {
                r.snapshot.as_mut().unwrap().root = Hash64::from_u64_word(5);
            }),
        ),
        (
            "the policy id replaced",
            Box::new(|r| {
                r.snapshot.as_mut().unwrap().policy_id = Hash64::from_u64_word(6);
            }),
        ),
        (
            "the excluded producer replaced",
            Box::new(|r| {
                r.snapshot.as_mut().unwrap().excluded_operator = Hash64::from_u64_word(7);
            }),
        ),
    ];
    for (name, edit) in &cases {
        let (own, reloaded) = tamper(edit.as_ref());
        assert!(own.is_err(), "{name}: the engine's own check accepted it");
        assert!(reloaded.is_err(), "{name}: the carriage reloaded");
    }
}

// ---------------------------------------------------------------------------------------------------------------------------
// RFC-0010 residual: the beacon's eligible source set, frozen at the epoch's commitment position
// ---------------------------------------------------------------------------------------------------------------------------
//
// What these drive: the engine's freeze stage (the first block whose DAA reaches the epoch's `release_daa` freezes the set the
// host derives from that block's PARENT), its journal (delta 174), its prune, its carriage, and the chain history's read of it.
// The derivation input is the reference source's set (`palw_panel_v3_epoch_sources_v1` under `Reference`): this harness has no
// kernel route with an OPV-eligible class, so the `Chain` derivation (`opv_eligible_set_v1` on the parent's route) is the same
// call it was before, now made once at the freeze instead of at every verification. The READ under test is the `Chain` one.

use kaspa_consensus_core::palw_panel_beacon_v1::{PanelBeaconHistoryV1, verify_panel_beacon_v1};
use kaspa_consensus_core::palw_permissionless_panel_v1::EpochSourceSetV1;
use kaspa_consensus_core::palw_state_v2::ChainPanelBeaconHistoryV1;

const A: [u8; 64] = [0xA1; 64];
const B: [u8; 64] = [0xB2; 64];
const C: [u8; 64] = [0xC3; 64];

/// A reference source whose eligibility is `profiles` (and no events: the fold verifies no proof in these tests).
fn eligible(profiles: &[[u8; 64]]) -> PalwPanelV3BeaconSourceV1 {
    PalwPanelV3BeaconSourceV1::Reference {
        events: Vec::new(),
        eligible_profiles: profiles.iter().copied().collect(),
        works: Vec::new(),
        sealed: Vec::new(),
    }
}

fn frozen_of(profiles: &[[u8; 64]], frozen_daa: u64) -> EpochSourceSetV1 {
    let mut ids: Vec<Hash64> = profiles.iter().map(|p| Hash64::from_bytes(*p)).collect();
    ids.sort();
    EpochSourceSetV1 { frozen_daa, profiles: ids }
}

fn epoch_sources_entries(delta: &PalwStateDeltaV2) -> Vec<&PalwDeltaEntryV2> {
    delta.entries.iter().filter(|e| matches!(e, PalwDeltaEntryV2::PanelV3EpochSources { .. })).collect()
}

/// The chain's frozen eligibility with a fixed list of Panel-independent settlements (the V2 lattice derives none: every Final
/// it writes is Panel-licensed), so a beacon can be verified at the lock against exactly what the chain froze.
struct FrozenWithEvents<'a> {
    chain: ChainPanelBeaconHistoryV1<'a>,
    events: Vec<WorkFinalEventV1>,
}

impl PanelBeaconHistoryV1 for FrozenWithEvents<'_> {
    fn final_events(&self) -> Vec<WorkFinalEventV1> {
        self.events.clone()
    }
    fn attributed_works(&self) -> Vec<misaka_palw_challenge::AttributedWorkV1> {
        Vec::new()
    }
    fn sealed_sources(&self) -> Vec<misaka_palw_challenge::SealedSourceV3> {
        Vec::new()
    }
    fn tip_position(&self) -> u64 {
        self.chain.tip_position()
    }
    fn eligible_profiles(&self, commitment_position: u64) -> BTreeSet<misaka_palw_challenge::hash::Digest> {
        self.chain.eligible_profiles(commitment_position)
    }
    fn pending_work_of_epoch(&self, epoch: u64) -> BTreeSet<misaka_palw_challenge::hash::Digest> {
        self.chain.pending_work_of_epoch(epoch)
    }
}

impl V3 {
    /// A floor claim sealed for its epoch: `(claim, release_daa, epoch)`.
    fn sealed_claim(&mut self, seed: u64) -> (Hash64, u64, u64) {
        let id = self.floor_claim(seed);
        self.step();
        self.step();
        let seal = self.record(&id).seal.expect("sealed");
        (id, seal.anchor_slot, seal.beacon_epoch)
    }

    fn request(&self, epoch: u64, release: u64) -> BeaconRequestV1 {
        let mirror = *self.c.sp.panel_v3().expect("the mirror");
        BeaconRequestV1 {
            network: mirror.network,
            ruleset: mirror.ruleset,
            scheme: mirror.policy.beacon_scheme,
            epoch,
            release_daa: release,
            deadline_daa: release + mirror.policy.beacon_wait_daa,
        }
    }

    fn at(&mut self, daa: u64) -> PalwStateDeltaV2 {
        self.step_at(daa, &[], PalwBlockWorkV3::None, Hash64::default(), 0)
    }
}

/// A proof of one independent work by `profile`, built against `built_against` (what a producer believes eligible).
fn beacon_of(
    req: &BeaconRequestV1,
    profile: [u8; 64],
    release: u64,
    built_against: BTreeSet<[u8; 64]>,
) -> (WorkFinalEventV1, Option<BeaconProofV1>) {
    let event = independent_event(profile, 0x50, release + 1, release + 2);
    let context = panel_beacon_context_v1(req, &challenge_policy(), built_against);
    let proof = match collect_work_beacon_v1(&context, std::slice::from_ref(&event), release + 5).expect("a valid policy") {
        WorkBeaconStateV1::Locked(beacon) => Some(BeaconProofV1 {
            epoch: req.epoch,
            output: Hash64::from_bytes(beacon.output),
            proof: borsh::to_vec(beacon.beacon()).unwrap(),
        }),
        _ => None,
    };
    (event, proof)
}

/// **(a) + (b): a profile denied or lapsed after the commitment does not change the beacon at the lock, and one that becomes
/// eligible after it is not added.** The set is frozen in the first block reaching `release` (from its parent), journaled once,
/// never rewritten; the chain history reads it — at `release` only — whatever the source answers later.
#[test]
fn the_epochs_source_set_is_frozen_at_its_commitment_and_later_eligibility_changes_do_not_move_the_beacon() {
    let mut w = V3::new();
    let (_, release, epoch) = w.sealed_claim(60);
    w.inputs.beacon_source = eligible(&[A, B]);
    w.at(release - 1);
    assert!(w.engine().epoch_source_rows().is_empty(), "nothing is frozen below the commitment position");
    let crossing = w.at(release + 1);
    let frozen = frozen_of(&[A, B], release + 1);
    assert_eq!(w.engine().epoch_source_rows().get(&epoch), Some(&frozen), "frozen by the first block reaching release");
    assert!(matches!(
        epoch_sources_entries(&crossing)[..],
        [PalwDeltaEntryV2::PanelV3EpochSources { key, old: None, new: Some(_) }] if *key == epoch
    ));

    // After the commitment: B is denied (or its DA lapsed), C becomes eligible.
    w.inputs.beacon_source = eligible(&[A, C]);
    let later = w.at(release + 2);
    assert!(epoch_sources_entries(&later).is_empty(), "the frozen set is never rewritten");
    assert_eq!(w.engine().epoch_source_rows().get(&epoch), Some(&frozen));

    let chain = PalwPanelV3BeaconSourceV1::Chain;
    let history = ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), w.c.s.panel_v3(), &chain, release + 2);
    assert_eq!(history.eligible_profiles(release), BTreeSet::from([A, B]), "the chain reads the frozen set, not today's answer");
    assert!(history.eligible_profiles(release + 1).is_empty(), "only the epoch's own commitment position has a set");
    let no_engine = ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), None, &chain, release + 2);
    assert!(no_engine.eligible_profiles(release).is_empty(), "no engine, no frozen set: nothing is re-derived");

    // At the lock (tip release + 5): B's work locks the beacon against the frozen set; C's — eligible only after the commitment —
    // does not, even with a proof its producer built against the later set.
    let req = w.request(epoch, release);
    let (b_event, b_proof) = beacon_of(&req, B, release, BTreeSet::from([A, B]));
    let b_proof = b_proof.expect("B's work locks against the frozen set");
    let at_lock = |events: Vec<WorkFinalEventV1>| FrozenWithEvents {
        chain: ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), w.c.s.panel_v3(), &chain, release + 5),
        events,
    };
    assert_eq!(verify_panel_beacon_v1(&[challenge_policy()], &at_lock(vec![b_event.clone()]), &req, &b_proof), Ok(()));
    let (c_event, c_proof) = beacon_of(&req, C, release, BTreeSet::from([A, C]));
    let c_proof = c_proof.expect("against the later set C's work would lock");
    assert!(verify_panel_beacon_v1(&[challenge_policy()], &at_lock(vec![c_event]), &req, &c_proof).is_err());
    // The steering this closes: re-derived at the lock, B's denial would have unlocked the epoch and C's admission re-seeded it.
    assert!(beacon_of(&req, B, release, BTreeSet::from([A, C])).1.is_none(), "a re-derived set would drop B's contribution");
    assert_ne!(b_proof.output, c_proof.output);
}

/// **(c): a reorg across the freeze block reverts the freeze and the other branch re-freezes from ITS commitment block's parent.**
/// The walk a real reorg executes: each block's delta reverted to the fork point (the freeze goes with the block that made it, a
/// later block's revert leaves it), then the other branch folded; re-applying the first branch's delta reproduces its freeze.
#[test]
fn a_reorg_across_the_freeze_block_reverts_it_and_the_other_branch_refreezes() {
    let mut w = V3::new();
    let (_, release, epoch) = w.sealed_claim(61);
    w.inputs.beacon_source = eligible(&[A, B]);
    w.at(release - 1);
    let (fork, fork_daa) = (w.c.s.clone(), w.c.daa);
    let d1 = w.at(release + 1);
    let branch1 = w.c.s.clone();
    let d2 = w.at(release + 2);
    assert_eq!(w.engine().epoch_source_rows().get(&epoch), Some(&frozen_of(&[A, B], release + 1)));

    let back1 = revert_delta_v2(&w.c.s, &d2, &w.c.sp).expect("reverts");
    assert_eq!(back1, branch1);
    assert_eq!(back1.panel_v3().unwrap().epoch_source_rows().get(&epoch), Some(&frozen_of(&[A, B], release + 1)));
    let back0 = revert_delta_v2(&back1, &d1, &w.c.sp).expect("reverts");
    assert_eq!(back0, fork);
    assert!(back0.panel_v3().unwrap().epoch_source_rows().is_empty(), "the freeze goes with its block");

    // The other branch: its own commitment block (at release + 3) sees another eligibility at its parent.
    w.c.s = back0;
    w.c.daa = fork_daa;
    w.inputs.beacon_source = eligible(&[A, C]);
    w.at(release + 3);
    assert_eq!(w.engine().epoch_source_rows().get(&epoch), Some(&frozen_of(&[A, C], release + 3)));
    let chain = PalwPanelV3BeaconSourceV1::Chain;
    let history = ChainPanelBeaconHistoryV1::new(&w.c.s, Hash64::default(), w.c.s.panel_v3(), &chain, release + 3);
    assert_eq!(history.eligible_profiles(release), BTreeSet::from([A, C]));
    // And back again: branch 1's delta re-applied to the fork is branch 1, its freeze included.
    assert_eq!(apply_delta_v2(&fork, &d1, &w.c.sp).expect("re-applies"), branch1);
}

/// **(d): a restart (the carriage encoded, decoded and loaded under the committed root) and a replay from genesis reproduce the
/// frozen set; a tampered one does not load.** The set is committed by the engine's root inside the `panel_v3/v1` block.
#[test]
fn a_restart_and_a_replay_reproduce_the_frozen_set_and_a_tampered_one_does_not_load() {
    let drive = |w: &mut V3| -> (u64, u64) {
        let (_, release, epoch) = w.sealed_claim(62);
        w.inputs.beacon_source = eligible(&[B, A]);
        w.at(release + 1);
        w.inputs.beacon_source = eligible(&[C]);
        w.at(release + 2);
        (release, epoch)
    };
    let mut w = V3::new();
    let (release, epoch) = drive(&mut w);
    let committed = w.c.s.state_root();
    let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&w.c.s)).expect("encodes");
    let carriage: PalwStateCarriageV2 = borsh::from_slice(&bytes).expect("decodes");
    let restarted = carriage.into_state(&w.c.sp, Some(committed)).expect("loads under its root");
    assert_eq!(restarted, w.c.s);
    assert_eq!(restarted.panel_v3().unwrap().epoch_source_rows().get(&epoch), Some(&frozen_of(&[A, B], release + 1)));

    // A restarted node that replays the same blocks from genesis ends on the same root and set.
    let mut replay = V3::new();
    drive(&mut replay);
    assert_eq!(replay.c.s.state_root(), committed);
    assert_eq!(replay.engine().epoch_source_rows(), w.engine().epoch_source_rows());

    // A frozen set swapped in an imported carriage does not load under the committed root.
    let mut tampered = PalwStateCarriageV2::from_state(&w.c.s);
    tampered.panel_v3.as_mut().unwrap().put_epoch_source_row(epoch, Some(frozen_of(&[A, C], release + 1)));
    assert!(tampered.panel_v3.as_ref().unwrap().check_consistency().is_ok(), "well formed, only not the chain's");
    assert!(tampered.into_state(&w.c.sp, Some(committed)).is_err(), "the root commits the frozen set");
}

/// **Bounded: one set per epoch, dropped once the epoch's contribution window closes** (`release + beacon_wait_daa`), and none for
/// a derivation above the cap (never a truncated set).
#[test]
fn a_frozen_set_is_pruned_after_its_window_and_an_over_cap_derivation_freezes_nothing() {
    let mut w = V3::new();
    let (_, release, epoch) = w.sealed_claim(63);
    w.inputs.beacon_source = eligible(&[A]);
    w.at(release + 1);
    assert!(w.engine().epoch_source_rows().contains_key(&epoch));
    let wait = w.c.sp.panel_v3().unwrap().policy.beacon_wait_daa;
    w.at(release + wait);
    assert!(w.engine().epoch_source_rows().contains_key(&epoch), "kept through the window's last DAA");
    let pruned = w.at(release + wait + 1);
    assert!(w.engine().epoch_source_rows().is_empty(), "dropped once the window closed");
    assert!(matches!(
        epoch_sources_entries(&pruned)[..],
        [PalwDeltaEntryV2::PanelV3EpochSources { key, old: Some(_), new: None }] if *key == epoch
    ));

    let mut over = V3::new();
    let (_, release, _) = over.sealed_claim(64);
    let many: Vec<[u8; 64]> = (0..=kaspa_consensus_core::palw_permissionless_panel_v1::MAX_EPOCH_SOURCES_V1 as u64)
        .map(|i| {
            let mut id = [0u8; 64];
            id[..8].copy_from_slice(&i.to_le_bytes());
            id
        })
        .collect();
    over.inputs.beacon_source = eligible(&many);
    over.at(release + 1);
    assert!(over.engine().epoch_source_rows().is_empty(), "over the cap: nothing frozen, the epoch has no eligible source");
}
