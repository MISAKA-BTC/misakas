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
            beacon_source: PalwPanelV3BeaconSourceV1::Reference { events: Vec::new(), eligible_profiles: BTreeSet::new() },
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
    fn step_at(&mut self, daa: u64, objects: &[PalwConsensusObjectV2], work: PalwBlockWorkV3<'_>, key: Hash64, subsidy: u64) -> PalwStateDeltaV2 {
        let x = ctx(0xCA_0000 + daa, daa, daa, subsidy);
        let parent = self.c.s.clone();
        let (child, delta, skips) = self.fold(&parent, &x, objects, work, key).unwrap_or_else(|e| panic!("the block at DAA {daa} folds: {e}"));
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
        self.inputs.beacon_source = PalwPanelV3BeaconSourceV1::Reference { events: vec![event.clone()], eligible_profiles: BTreeSet::from([profile]) };
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
        let WorkBeaconStateV1::Locked(beacon) = collect_work_beacon_v1(&context, &[event], release + 5).expect("a valid policy") else {
            panic!("the reference source locks a beacon");
        };
        BeaconProofV1 { epoch, output: Hash64::from_bytes(beacon.output), proof: borsh::to_vec(&beacon).unwrap() }
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
    assert_eq!(w.engine().reserved(&kaspa_consensus_core::palw_permissionless_panel_v1::BondIdV1::from(seats[0])), binding.exposure as u128);
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
/// A non-seat session pauses nothing (V3S-08), so the policy's receipt window is the V2 one's order of magnitude: a window shorter
/// than the disclose window would let the engine's redraws expire the claim, uncharged, before the default could convict it.
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

    let mut policy = engine_policy();
    policy.receipt_window_daa = 100_000;
    let mut w = V3::with(policy, FENCE);
    let (id, _bound_at) = w.run_to_bound(20);
    let seats = seats_of_panel(&w.c.s, &id);
    let outsider = genesis_bonds(&w.c.p).iter().map(|(k, _, _)| *k).find(|k| *k != producer && !seats.contains(k)).expect("a bond outside the Panel");
    let collateral_before = w.c.s.bond(&producer).unwrap().collateral;
    w.step_with(&[da_accuse(id, outsider, 3)]);
    assert!(w.c.s.da_session(&id, &outsider).is_some(), "the public bond's session is open");
    assert!(matches!(w.phase_v2(&id), PalwClaimPhaseV2::PanelBound { .. }), "a non-seat session pauses nothing (V3S-08)");
    // Nobody answers: the session's deadline, then the default one block later.
    let deadline = w.c.s.da_session(&id, &outsider).unwrap().deadline_daa;
    w.step_at(deadline, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert_eq!(w.record(&id).binding_history.len(), 1, "no redraw while the engine's window runs");
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
    w.step_at(release + 5, &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }], PalwBlockWorkV3::None, Hash64::default(), 0);
    w.step_at(release + 10, &[], PalwBlockWorkV3::None, Hash64::default(), 0);
    assert!(matches!(w.record(&first).phase, ClaimPhaseV3::Bound(_)), "the first in acceptance order binds: {:?} / {:?}", w.record(&first).phase, w.record(&first).claim.required_exposure);
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
    w.step_at(release + 5, &[PalwConsensusObjectV2::PanelBeaconProofV3 { proof: Box::new(proof) }], PalwBlockWorkV3::None, Hash64::default(), 0);
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
