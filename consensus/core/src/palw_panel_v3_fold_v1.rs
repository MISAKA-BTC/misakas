//! **RFC-0010: the production fold of the permissionless Panel (V3).** A child module of `palw_state_v2`, as the other fold lanes
//! are, so it reads the builder and writes the tables only through their one writers. Dormant: nothing here runs unless
//! `Params::palw_permissionless_panel_v1` is armed (mirrored as `PalwStateParamsV2::panel_v3`), and `validate_palw_v2` refuses
//! that at every real height.
//!
//! ```text
//!   block N, in the order the chain's own transition runs it
//!   ─────────────────────────────────────────────────────────────────────────────────────────────────────────
//!   2f  advance_v1       engine.advance   releases, seals against the PARENT checkpoint, readiness, due draws
//!                         ├─ binding      → the V2 panel record (anchor = V3 seed), duty row, reserved exposure, PanelBound
//!                         └─ non-fraud end → V2 void (SealUnavailable / BeaconUnavailable / NoCapablePanel / PanelUnavailable)
//!   3   objects          a certified epoch output (tag 120) is queued
//!   4b″ take_block_v1    engine.accept_beacon per queued output (one that does not verify is dropped — the block stands)
//!                         engine.admit        the claims the block created under the V3 rule, in V2 acceptance order
//!                         the engine's journal (delta 170–173) is pushed
//! ```
//!
//! **One exposure ledger.** A V3 binding writes the V2 duty row and the V2 `reserved_exposure` of each seat through the very
//! writer V2's `PanelBound` uses ([`TransitionBuilder::reserve_seat_duties_with`]); the engine's own reservation map is a
//! mirror of those rows, never a second ledger. The headroom the engine draws against is the V2 gate room
//! (`gate_room(.., Work)`, the same question `require_panel_lock_eligible` asks at a V2 bind) with the engine's own live
//! duties added back, so a seat is never counted twice and a legacy lane-A claim and a V3 claim cannot both spend one sompi.
//!
//! **The receipt/court handoff.** The binding becomes an ordinary `PalwPanelStateV2` whose `anchor` is the V3 seed, so every
//! V2 door — receipts, the coverage and S2 licences, DA accusations (a bound panel is what `DaClaimNotAccusable` asks for), the
//! court — works for a V3 claim exactly as for a V2 one, by any ordinary public bond. Sharded classes are NOT bound by V3 (a
//! flat panel cannot license by parts): a V3-rule claim of a class with a shard plan ends at admission, non-fraud
//! (`PermissionlessNoCapablePanel`); the per-shard V3 draw is RFC-0006's DESIGN_GAP.

use super::*;
use crate::palw_panel_beacon_v1::{self as beacon, PanelBeaconHistoryV1};
use crate::palw_panel_v2::PalwPanelDrawPolicyV1;
use crate::palw_permissionless_panel_v1::{PalwPanelV3ParamsV1, panel_admitted_claim_v1, panel_bond_key_v1, panel_snapshot_candidates_v1};
use misaka_palw_challenge::hash::Digest;
use misaka_palw_challenge::{FinalPathV1, PostCommitChallengePolicyV1, WorkFinalEventV1, WorkSourceKindV1};
use misaka_palw_panel as eng;
use misaka_palw_panel::{
    AdmittedClaimV1, BeaconProofV1, BeaconRequestV1, BondIdV1, ClaimPhaseV3, ConsensusViewV1, NonFraudReasonV1, PanelCursorV1,
    PanelErrorV1, PanelFoldEventsV1, PermissionlessPanelStateV1, ReceiptClockV1, SeatCandidateV1, SelectedChainStepV1,
};

/// Where the beacon's source events come from.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum PalwPanelV3BeaconSourceV1 {
    /// **The branch's own settlements** — the only value any processor resolves. Every Final the V2 lattice can write passed
    /// through a Panel licence, so none is a Panel-independent source, and the chain's G14-complete profile set is empty.
    #[default]
    Chain,
    /// A fixed history, for reference replays and tests of the verification path. Never resolved from a running node.
    Reference { events: Vec<WorkFinalEventV1>, eligible_profiles: BTreeSet<Digest> },
}

/// **What the permissionless Panel's draw reads that the pure transition cannot see**, resolved by the host at the PRE-ENTROPY
/// CHECKPOINT (the block's selected parent).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PalwPanelV3InputsV1 {
    /// The public draw policy at the checkpoint: readiness, panel economy, independence. The population is structural.
    pub draw: PalwPanelDrawPolicyV1,
    /// `Params::palw_capability_bound` at the checkpoint.
    pub capability_proof: bool,
    /// The class the outsider seat is drawn from (the network's BASE-0 floor class).
    pub floor_class: Hash64,
    /// The beacon schemes this release approves. **Empty in every release so far** ([`beacon::approved_panel_beacon_policies_v1`]).
    pub approved_beacons: Vec<PostCommitChallengePolicyV1>,
    pub beacon_source: PalwPanelV3BeaconSourceV1,
}

// ---------------------------------------------------------------------------------------------
// The beacon's history, derived from the branch
// ---------------------------------------------------------------------------------------------

/// **The branch's settlement events, as the challenge contract reads them.** Every `Final` claim with a canonical work
/// identity; its path is `PanelLicensed` — **by construction of the V2 lattice**: a claim reaches `Final` only from
/// `ReceiptLicensed`, whose licence is a quorum of seat receipts (or a tally of seat vertices). There is no `Final` without a
/// Panel on this chain, so no event here can seed a Panel assignment. BASE-0's floor class is reported as the fallback it is.
pub fn palw_panel_v3_final_events_v1(state: &PalwChainStateV2, base_class: Hash64) -> Vec<WorkFinalEventV1> {
    state
        .claims
        .iter()
        .filter_map(|(id, claim)| {
            let PalwClaimPhaseV2::Final { final_daa } = claim.phase else { return None };
            // A work with no canonical identity has nothing a re-attachment cannot rewrap (a claim id is not an identity).
            let work = claim.work_id.filter(|work| *work != Hash64::default())?;
            let panel = state.panels.get(id);
            Some(WorkFinalEventV1 {
                kind: if claim.class_id == base_class { WorkSourceKindV1::Base0Fallback } else { WorkSourceKindV1::RealUsefulWork },
                source_profile_id: claim.class_id.as_bytes(),
                canonical_work_id: work.as_bytes(),
                execution_commitment: claim.execution_root.as_bytes(),
                accepted_position: claim.accepted_daa,
                settlement_position: final_daa,
                occurrence_index: 0,
                claim_final: true,
                da_satisfied: true,
                validity_independent: true,
                depends_on_profiles: Vec::new(),
                final_path: FinalPathV1::PanelLicensed {
                    panel_seed_id: panel.map(|panel| panel.anchor.as_bytes()).unwrap_or([0u8; 64]),
                    panel_epoch: panel.map(|panel| panel.bound_daa).unwrap_or(0),
                },
            })
        })
        .collect()
}

/// The history a proof is judged against: the branch up to a block's selected parent.
pub struct ChainPanelBeaconHistoryV1<'a> {
    state: &'a PalwChainStateV2,
    base_class: Hash64,
    engine: Option<&'a PermissionlessPanelStateV1>,
    source: &'a PalwPanelV3BeaconSourceV1,
    tip_position: u64,
}

impl<'a> ChainPanelBeaconHistoryV1<'a> {
    pub fn new(
        state: &'a PalwChainStateV2,
        base_class: Hash64,
        engine: Option<&'a PermissionlessPanelStateV1>,
        source: &'a PalwPanelV3BeaconSourceV1,
        tip_position: u64,
    ) -> Self {
        Self { state, base_class, engine, source, tip_position }
    }
}

impl PanelBeaconHistoryV1 for ChainPanelBeaconHistoryV1<'_> {
    fn final_events(&self) -> Vec<WorkFinalEventV1> {
        match self.source {
            PalwPanelV3BeaconSourceV1::Chain => palw_panel_v3_final_events_v1(self.state, self.base_class),
            PalwPanelV3BeaconSourceV1::Reference { events, .. } => events.clone(),
        }
    }

    fn tip_position(&self) -> u64 {
        self.tip_position
    }

    /// **Empty on the chain**: a profile is eligible only when Active AND G14-complete in the epoch's commitment state, and the
    /// code-derived `PUBLIC_PROSECUTION_COMPLETE` gate is not linked into consensus — no class is G14-complete here.
    fn eligible_profiles(&self) -> BTreeSet<Digest> {
        match self.source {
            PalwPanelV3BeaconSourceV1::Chain => BTreeSet::new(),
            PalwPanelV3BeaconSourceV1::Reference { eligible_profiles, .. } => eligible_profiles.clone(),
        }
    }

    fn pending_work_of_epoch(&self, epoch: u64) -> BTreeSet<Digest> {
        self.engine
            .map(|engine| {
                engine
                    .claim_rows()
                    .values()
                    .filter(|record| !record.phase.terminal() && record.seal.as_ref().is_some_and(|seal| seal.beacon_epoch == epoch))
                    .map(|record| record.claim.work_id.as_bytes())
                    .collect()
            })
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------------------------
// The engine's view of the chain
// ---------------------------------------------------------------------------------------------

fn inconsistent(what: &str, why: impl std::fmt::Display) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(format!("permissionless Panel engine: {what}: {why}"))
}

/// **Is the V2 claim still waiting for its Panel's receipts?** (`Provisional` before the draw, `PanelBound` after, and
/// `DefaultDisputed` while a data-availability accusation holds the claim.) Anything else — licensed, final, voided, retired —
/// is an outcome the engine releases on.
fn claim_awaits_its_panel(claim: Option<&PalwClaimStateV2>) -> bool {
    matches!(
        claim.map(|claim| &claim.phase),
        Some(PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. })
    )
}

/// The engine's read of the fold at one block (the parent base for the population, the pre-object base for headroom).
struct FoldView<'a, 'b> {
    builder: &'a TransitionBuilder<'b>,
    parent: &'a PalwChainStateV2,
    engine: &'a PermissionlessPanelStateV1,
    inputs: &'a PalwPanelV3InputsV1,
    ctx: &'a PalwBlockContextV2,
    mirror: &'a PalwPanelV3ParamsV1,
    /// The engine's own live duties per bond, which V2's ledger already holds (see [`Self::available_collateral`]).
    duties: BTreeMap<BondIdV1, u128>,
    tip_position: u64,
}

impl<'a, 'b> FoldView<'a, 'b> {
    fn new(
        builder: &'a TransitionBuilder<'b>,
        parent: &'a PalwChainStateV2,
        engine: &'a PermissionlessPanelStateV1,
        inputs: &'a PalwPanelV3InputsV1,
        ctx: &'a PalwBlockContextV2,
        mirror: &'a PalwPanelV3ParamsV1,
    ) -> Self {
        let mut view = Self {
            builder,
            parent,
            engine,
            inputs,
            ctx,
            mirror,
            duties: BTreeMap::new(),
            tip_position: parent.last_point.map(|point| point.daa_score).unwrap_or(0),
        };
        // The duties the engine holds on claims still awaiting their Panel: the V2 ledger carries each as a duty row written at
        // the bind. A claim already licensed or ended is not the engine's to count (the engine releases it first thing), and
        // V2 has released or kept its duty on V2's own terms.
        let mut duties = BTreeMap::new();
        for (id, record) in engine.claim_rows() {
            if let ClaimPhaseV3::Bound(binding) = &record.phase
                && !view.terminal_claim(id)
            {
                for seat in &binding.seats {
                    *duties.entry(*seat).or_insert(0u128) += binding.exposure as u128;
                }
            }
        }
        view.duties = duties;
        view
    }
}

impl ConsensusViewV1 for FoldView<'_, '_> {
    /// The public population frozen at the PRE-ENTROPY CHECKPOINT: the parent state, never this block's registrations, top-ups,
    /// readiness proofs or objects. Sanitised so that the engine's structural checks cannot fail the block: an exclusion,
    /// an immature or under-collateralised bond is simply not a candidate, and a population above the cap keeps its highest
    /// collateral (ties by bond id) — weight is proportional to collateral, so the dropped tail carries the least of it.
    fn candidates(&self, claim: &AdmittedClaimV1) -> Result<Vec<SeatCandidateV1>, PanelErrorV1> {
        let policy = self.mirror.policy;
        let checkpoint_daa = self.engine.daa();
        let mut rows = panel_snapshot_candidates_v1(
            self.parent,
            claim.claim_id,
            self.inputs.floor_class,
            checkpoint_daa,
            policy,
            self.inputs.draw,
            self.inputs.capability_proof,
        )
        // A claim the checkpoint does not hold seals an empty population (and ends non-fraud), never a failed block.
        .unwrap_or_default();
        rows.retain(|seat| {
            seat.collateral >= policy.min_collateral
                && seat.registered_daa.checked_add(policy.bond_maturity_daa).is_some_and(|mature| mature <= checkpoint_daa)
                && seat.bond != claim.producer
                && seat.operator != claim.producer_operator
                && seat.key != claim.producer_key
                && seat.roles != 0
                && seat.roles & !3 == 0
                && seat.capability_root != Hash64::default()
                && seat.readiness_root != Hash64::default()
        });
        if rows.len() > policy.max_candidates as usize {
            rows.sort_by(|a, b| b.collateral.cmp(&a.collateral).then(a.bond.cmp(&b.bond)));
            rows.truncate(policy.max_candidates as usize);
        }
        Ok(rows)
    }

    /// **The one ledger's room for one more duty, excluding the engine's own** (the engine subtracts its live reservations
    /// itself): a bond that may take work at the panel floor and is not frozen has the V2 gate room
    /// (`gate_room(.., Work)`, the question `require_panel_lock_eligible` asks at a V2 bind), on the pre-object base, plus the
    /// duties of V3-bound claims V2's ledger already holds for it.
    fn available_collateral(&self, bond: &BondIdV1) -> u128 {
        let key = panel_bond_key_v1(*bond);
        let state = &self.builder.state;
        let floor = crate::palw_panel_economy_v1::palw_panel_collateral_floor_v1(self.builder.params.min_collateral_sompi());
        let may_work = state.bonds.get(&key).is_some_and(|record| palw_bond_may_take_work_v2(record, floor))
            && !crate::palw_aggregate_liability_v1::palw_bond_is_frozen_v1(state, &key);
        if !may_work {
            return 0;
        }
        self.builder
            .gate_room(&key, self.ctx.daa_score, PalwRcoreGateV1::Work)
            .saturating_add(self.duties.get(bond).copied().unwrap_or(0))
    }

    fn verify_beacon(&self, request: &BeaconRequestV1, proof: &BeaconProofV1) -> Result<(), PanelErrorV1> {
        let history = ChainPanelBeaconHistoryV1::new(
            self.parent,
            self.builder.params.base_class_id(),
            Some(self.engine),
            &self.inputs.beacon_source,
            self.tip_position,
        );
        beacon::verify_panel_beacon_for_engine_v1(&self.inputs.approved_beacons, &history, request, proof)
    }

    /// The V2 claim has left its Panel's receipts (licensed, final, voided, retired): the engine releases its reservation.
    fn terminal_claim(&self, claim: &Hash64) -> bool {
        !claim_awaits_its_panel(self.builder.state.claims.get(claim))
    }

    /// **The receipt clock never runs against a pending accusation — structural, G14.** A Panel's receipt window
    /// (and the redraw and `PanelUnavailable` expiry it ends in) must not pre-empt a DA default a public bond has already
    /// demanded: a colluding producer and Panel could otherwise let the window lapse, void the claim uncharged and close the
    /// session neutrally (a void closes every session without conviction). So the engine's clock is PAUSED while ANY session
    /// on the claim is open — seat or non-seat; V2 itself pauses only for seat sessions (V3S-08), which is the narrower rule
    /// this strengthens — and so is it while a court session is open; a session's close re-bases the window to start no
    /// earlier than that close. The pause is bounded:
    /// a claim takes at most three non-seat sessions open at once and sixteen over its life, each at most the disclose window.
    /// V2's own phase anchor (which a seat session's pause credit moves) and the old court's `DefaultDisputed` are read too.
    fn receipt_clock(&self, claim: &Hash64) -> Option<ReceiptClockV1> {
        let record = self.builder.state.da_claims.get(claim);
        if record.is_some_and(|record| record.open_sessions() > 0) {
            return Some(ReceiptClockV1::Paused);
        }
        // …and while a court session is open on it: an accusation of the executor must reach its verdict (or its own backstop)
        // before a non-fraud expiry can close the claim and the session with it. Bounded by the court's session capacity.
        if self.builder.state.open_courts_by_claim.get(claim).is_some_and(|open| *open > 0) {
            return Some(ReceiptClockV1::Paused);
        }
        match self.builder.state.claims.get(claim).map(|claim| &claim.phase) {
            Some(PalwClaimPhaseV2::PanelBound { bound_daa }) => Some(ReceiptClockV1::Running {
                bound_daa: (*bound_daa).max(record.and_then(|record| record.last_closed_daa).unwrap_or(0)),
            }),
            Some(PalwClaimPhaseV2::DefaultDisputed { .. }) => Some(ReceiptClockV1::Paused),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The fold
// ---------------------------------------------------------------------------------------------

/// The mirror, iff the V3 rule governs claims at this block (the fence in force, R-core+ in force).
fn active_mirror(builder: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Option<PalwPanelV3ParamsV1> {
    builder.params.panel_v3().copied().filter(|mirror| ctx.daa_score >= mirror.from_daa && builder.params.rcore_plus_active_at(ctx.daa_score))
}

fn create_engine(parent: &PalwChainStateV2, mirror: &PalwPanelV3ParamsV1) -> Result<PermissionlessPanelStateV1, PalwStateV2Error> {
    PermissionlessPanelStateV1::from_cursor(PanelCursorV1 {
        version: eng::WIRE_VERSION_V1,
        network: mirror.network,
        ruleset: mirror.ruleset,
        policy: mirror.policy,
        tip: parent.last_point.map(|point| point.block).unwrap_or_default(),
        height: 0,
        daa: parent.last_point.map(|point| point.daa_score).unwrap_or(0),
        next_order: 0,
    })
    .map_err(|e| inconsistent("creation", e))
}

fn v2_reason_of(reason: NonFraudReasonV1) -> PalwVoidReasonV2 {
    match reason {
        NonFraudReasonV1::SealUnavailable => PalwVoidReasonV2::SealUnavailable,
        NonFraudReasonV1::BeaconUnavailable => PalwVoidReasonV2::BeaconUnavailable,
        NonFraudReasonV1::NoCapablePanel => PalwVoidReasonV2::PermissionlessNoCapablePanel,
        NonFraudReasonV1::PanelUnavailable => PalwVoidReasonV2::PanelUnavailable,
    }
}

/// **Step 2f: the engine's pre-object stage** and its V2 effects.
pub(super) fn advance_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(mirror) = active_mirror(builder, ctx) else { return Ok(()) };
    let inputs = builder.extras.panel_v3.clone().unwrap_or_default();
    let engine = match builder.state.panel_v3.take() {
        Some(engine) => engine,
        None => create_engine(parent, &mirror)?,
    };
    let step = SelectedChainStepV1 {
        block: ctx.block,
        parent: engine.tip(),
        height: engine.height().saturating_add(1),
        daa: ctx.daa_score,
        admissions: Vec::new(),
        beacons: Vec::new(),
    };
    let advanced = {
        let view = FoldView::new(builder, parent, &engine, &inputs, ctx, &mirror);
        engine.advance(&step, &view)
    };
    let (next, events) = advanced.map_err(|e| inconsistent("advance", e))?;
    // The engine is in place BEFORE its V2 effects are written: the deadline derivations a writer consults read it.
    builder.state.panel_v3 = Some(next);
    apply_events(builder, ctx, &events)
}

fn apply_events(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, events: &PanelFoldEventsV1) -> Result<(), PalwStateV2Error> {
    for (id, reason) in &events.non_fraud_voids {
        end_claim(builder, ctx, id, v2_reason_of(*reason))?;
    }
    for (id, binding) in &events.bindings {
        bind_v2(builder, ctx, id, binding)?;
    }
    Ok(())
}

/// A non-fraud termination: the V2 claim ends (the reservation released, nothing slashed, no strike, no hold).
fn end_claim(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, id: &Hash64, reason: PalwVoidReasonV2) -> Result<(), PalwStateV2Error> {
    let Some(claim) = builder.state.claims.get(id).cloned() else { return Ok(()) };
    if claim.phase.is_terminal() {
        return Ok(());
    }
    builder.void_claim(*id, &claim, ctx.daa_score, reason)
}

/// **A binding becomes the V2 bind**: the panel record (anchor = the V3 seed), the duty row and each seat's reserved exposure
/// through V2's one writer, and `PanelBound`. A retry releases the previous round's duties first and re-binds in place.
fn bind_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    binding: &eng::PanelBoundV3,
) -> Result<(), PalwStateV2Error> {
    let claim = builder.state.claims.get(claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(*claim_id))?;
    let retry = binding.retry_index > 0;
    match (&claim.phase, retry) {
        (PalwClaimPhaseV2::Provisional, false) | (PalwClaimPhaseV2::PanelBound { .. }, true) => {}
        _ => return Err(PalwStateV2Error::WrongPhase { claim: *claim_id, edge: "PanelBoundV3" }),
    }
    let mut seats = Vec::with_capacity(binding.seats.len());
    for seat in &binding.seats {
        let bond = panel_bond_key_v1(*seat);
        let operator_id = builder.state.bonds.get(&bond).map(|record| record.operator_id).ok_or(PalwStateV2Error::MissingBond(bond))?;
        seats.push(PalwPanelSeatV2 { bond, operator_id });
    }
    if retry {
        // The previous round's seats leave duty with their exposure; the redraw deals different ones.
        builder.release_seat_duties(claim_id)?;
    }
    builder.write_panel(*claim_id, Some(PalwPanelStateV2 { anchor: binding.panel_seed_v3, seats: seats.clone(), bound_daa: ctx.daa_score }));
    builder.reserve_seat_duties_with(*claim_id, &seats, binding.exposure as u128, ctx.daa_score)?;
    let mut bound = claim;
    bound.phase = PalwClaimPhaseV2::PanelBound { bound_daa: ctx.daa_score };
    if retry {
        // A redrawn claim is marked as V2 marks one, so no consumer reads it as a first panel.
        bound.rebound_daa = Some(ctx.daa_score);
    }
    builder.write_claim(*claim_id, Some(bound));
    if !retry {
        // ADR-0164 F-K (K4's denominator): a claim that reached its Panel.
        builder.note_breaker_v1(ctx.daa_score, crate::palw_capacity_s567_v1::PalwBreakerEventV1::Bound);
    }
    Ok(())
}

/// **Step 3: a certified epoch output** is queued, bounded by the policy.
pub(super) fn queue_beacon_object_v1(builder: &mut TransitionBuilder<'_>, proof: &BeaconProofV1) {
    let Some(mirror) = builder.params.panel_v3() else { return };
    if builder.panel_v3_beacons.len() < mirror.policy.max_beacons_per_block as usize {
        builder.panel_v3_beacons.push(proof.clone());
    }
}

/// The claims this block created, in the order the chain's transition created them (V2 acceptance order).
fn claims_created_this_block(builder: &TransitionBuilder<'_>) -> Vec<Hash64> {
    let mut seen = BTreeSet::new();
    let mut created = Vec::new();
    for entry in &builder.entries {
        if let PalwDeltaEntryV2::Claim { key, old: None, new: Some(_) } = entry
            && seen.insert(*key)
        {
            created.push(*key);
        }
    }
    created
}

/// Why a V3-rule claim cannot enter the engine (it ends non-fraud, in this block).
fn admission_of(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    mirror: &PalwPanelV3ParamsV1,
    claim_id: &Hash64,
    claim: &PalwClaimStateV2,
) -> Result<AdmittedClaimV1, &'static str> {
    if claim.work_id.is_none_or(|work| work == Hash64::default()) {
        return Err("a claim with no canonical work identity cannot be drawn a Panel (P0-10)");
    }
    // A class with a layer-shard plan is drawn per shard; the V3 draw is flat (RFC-0006's per-shard V3 draw is a DESIGN_GAP).
    if builder.params.tir_shard_active_at(ctx.daa_score) && builder.state.tir_shard_plans.contains_key(&claim.class_id) {
        return Err("a sharded class is drawn per shard; the permissionless draw is flat");
    }
    // The outsider seat is part of the licence (`palw_licence_names_its_outsider_v1`): the policy must draw one exactly when V2
    // would require one, or a bound claim could never license.
    let outsider = palw_claim_is_outsider_judged_v1(&builder.state, claim, builder.extras.admission_independence_daa);
    if outsider != (mirror.policy.outsider_seats == 1) {
        return Err("the policy's outsider seat disagrees with the claim's independence rule");
    }
    let prices = builder.read().rcore_bind_prices(claim_id, claim, mirror.policy.seat_count as usize, ctx.daa_score);
    let exposure = u64::try_from(prices.eligibility).map_err(|_| "the per-seat exposure does not fit")?;
    panel_admitted_claim_v1(&builder.state, *claim_id, exposure).map_err(|_| "the claim has no admissible immutable fields")
}

/// **Step 4b″: the certified outputs, the admissions and the journal.**
pub(super) fn take_block_v1(
    builder: &mut TransitionBuilder<'_>,
    parent: &PalwChainStateV2,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let proofs = std::mem::take(&mut builder.panel_v3_beacons);
    let Some(mirror) = active_mirror(builder, ctx) else { return Ok(()) };
    let inputs = builder.extras.panel_v3.clone().unwrap_or_default();
    let mut engine = builder.state.panel_v3.take().ok_or_else(|| inconsistent("take", "the engine is absent past the fence"))?;
    // Certified outputs: one at a time, and a proof that does not verify is dropped — the block stands.
    for proof in &proofs {
        let attempt = {
            let view = FoldView::new(builder, parent, &engine, &inputs, ctx, &mirror);
            engine.accept_beacon(proof, &view)
        };
        if let Ok(next) = attempt {
            engine = next;
        }
    }
    // Admissions: the claims this block created under the V3 rule, in acceptance order.
    let mut admitted = Vec::new();
    let mut refused = Vec::new();
    for claim_id in claims_created_this_block(builder) {
        let Some(claim) = builder.state.claims.get(&claim_id) else { continue };
        if claim.phase.is_terminal() || !builder.params.panel_v3_rule_at(claim.accepted_daa) {
            continue;
        }
        match admission_of(builder, ctx, &mirror, &claim_id, claim) {
            Ok(admission) => admitted.push(admission),
            Err(_) => refused.push(claim_id),
        }
    }
    let (next, rejected) = engine.admit(&admitted).map_err(|e| inconsistent("admit", e))?;
    builder.state.panel_v3 = Some(next);
    // A claim the engine could not take ends non-fraud at once, holding nothing.
    for claim_id in refused.into_iter().chain(rejected.into_iter().map(|(id, _)| id)) {
        end_claim(builder, ctx, &claim_id, PalwVoidReasonV2::PermissionlessNoCapablePanel)?;
    }
    push_journal(builder, parent);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The journal: the engine's keyed decomposition
// ---------------------------------------------------------------------------------------------

/// The delta entries (170–173) of the engine's change from the parent state to this one.
fn journal(old: Option<&PermissionlessPanelStateV1>, new: Option<&PermissionlessPanelStateV1>) -> Vec<PalwDeltaEntryV2> {
    let mut entries = Vec::new();
    let Some(new) = new else { return entries };
    let old_cursor = old.map(|engine| engine.cursor());
    if old_cursor.as_ref() != Some(&new.cursor()) {
        entries.push(PalwDeltaEntryV2::PanelV3Cursor { old: old_cursor, new: Some(new.cursor()) });
    }
    let empty_claims = BTreeMap::new();
    let old_claims = old.map(|engine| engine.claim_rows()).unwrap_or(&empty_claims);
    for key in old_claims.keys().chain(new.claim_rows().keys()).collect::<BTreeSet<_>>() {
        let (before, after) = (old_claims.get(key), new.claim_rows().get(key));
        if before != after {
            entries.push(PalwDeltaEntryV2::PanelV3Claim { key: *key, old: before.cloned(), new: after.cloned() });
        }
    }
    let empty_work = BTreeSet::new();
    let old_work = old.map(|engine| engine.work_id_rows()).unwrap_or(&empty_work);
    for key in old_work.symmetric_difference(new.work_id_rows()) {
        entries.push(PalwDeltaEntryV2::PanelV3WorkId { key: *key, old: old_work.contains(key), new: new.work_id_rows().contains(key) });
    }
    let empty_beacons = BTreeMap::new();
    let old_beacons = old.map(|engine| engine.beacon_rows()).unwrap_or(&empty_beacons);
    for key in old_beacons.keys().chain(new.beacon_rows().keys()).collect::<BTreeSet<_>>() {
        let (before, after) = (old_beacons.get(key).copied(), new.beacon_rows().get(key).copied());
        if before != after {
            entries.push(PalwDeltaEntryV2::PanelV3Beacon { key: *key, old: before, new: after });
        }
    }
    entries
}

fn push_journal(builder: &mut TransitionBuilder<'_>, parent: &PalwChainStateV2) {
    let entries = journal(parent.panel_v3.as_ref(), builder.state.panel_v3.as_ref());
    builder.entries.extend(entries);
}

fn mismatch(what: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::DeltaMismatch(what)
}

pub(super) fn apply_cursor_entry_v1(
    state: &mut PalwChainStateV2,
    old: &Option<PanelCursorV1>,
    new: &Option<PanelCursorV1>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    if state.panel_v3.as_ref().map(|engine| engine.cursor()).as_ref() != expected.as_ref() {
        return Err(mismatch("the Panel V3 cursor does not match the delta's expectation"));
    }
    match (install, state.panel_v3.as_mut()) {
        (Some(cursor), Some(engine)) => engine.set_cursor(cursor.clone()),
        (Some(cursor), None) => {
            state.panel_v3 =
                Some(PermissionlessPanelStateV1::from_cursor(cursor.clone()).map_err(|_| mismatch("the Panel V3 cursor does not build an engine"))?);
        }
        (None, Some(engine)) => {
            if !engine.claim_rows().is_empty() || !engine.work_id_rows().is_empty() || !engine.beacon_rows().is_empty() {
                return Err(mismatch("the Panel V3 engine is removed while it still holds rows"));
            }
            state.panel_v3 = None;
        }
        (None, None) => {}
    }
    Ok(())
}

pub(super) fn apply_claim_entry_v1(
    state: &mut PalwChainStateV2,
    key: &Hash64,
    old: &Option<eng::ClaimRecordV3>,
    new: &Option<eng::ClaimRecordV3>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    let engine = state.panel_v3.as_mut().ok_or_else(|| mismatch("a Panel V3 claim row without an engine"))?;
    if engine.claim_rows().get(key) != expected.as_ref() {
        return Err(mismatch("a Panel V3 claim row does not match the delta's expectation"));
    }
    engine.put_claim_row(*key, install.clone());
    Ok(())
}

pub(super) fn apply_work_id_entry_v1(
    state: &mut PalwChainStateV2,
    key: &Hash64,
    old: bool,
    new: bool,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    let engine = state.panel_v3.as_mut().ok_or_else(|| mismatch("a Panel V3 work identity without an engine"))?;
    if engine.work_id_rows().contains(key) != expected {
        return Err(mismatch("a Panel V3 work identity does not match the delta's expectation"));
    }
    engine.put_work_id_row(*key, install);
    Ok(())
}

pub(super) fn apply_beacon_entry_v1(
    state: &mut PalwChainStateV2,
    key: u64,
    old: &Option<Hash64>,
    new: &Option<Hash64>,
    revert: bool,
) -> Result<(), PalwStateV2Error> {
    let (expected, install) = if revert { (new, old) } else { (old, new) };
    let engine = state.panel_v3.as_mut().ok_or_else(|| mismatch("a Panel V3 certified output without an engine"))?;
    if engine.beacon_rows().get(&key) != expected.as_ref() {
        return Err(mismatch("a Panel V3 certified output does not match the delta's expectation"));
    }
    engine.put_beacon_row(key, *install);
    Ok(())
}

/// After a delta's rows are applied or reverted: re-derive the engine's reservations (its one derived field).
pub(super) fn refresh_derived_v1(state: &mut PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    if let Some(engine) = state.panel_v3.as_mut() {
        engine.refresh_derived().map_err(|e| inconsistent("derived reservations", e))?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Consistency
// ---------------------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// **The engine agrees with the V2 tables.** Its own invariants hold and its identity is the fence's; every claim it still
    /// clocks is a V2 claim awaiting its Panel, bound exactly as the engine says (the panel record's anchor and seats, the duty
    /// row's seats and exposure); a claim the engine released is not waiting for its Panel; every V2 claim waiting for a Panel
    /// that was accepted under the V3 rule is the engine's, and no other is.
    pub(super) fn assert_panel_v3_consistency_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let bad = |why: String| PalwStateV2Error::CarriageInconsistent(format!("permissionless Panel: {why}"));
        let Some(engine) = &self.panel_v3 else {
            if let Some((id, _)) = self.claims.iter().find(|(_, claim)| {
                matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. }) && params.panel_v3_rule_at(claim.accepted_daa)
            }) {
                return Err(bad(format!("claim {id} was accepted under the rule but no engine exists")));
            }
            return Ok(());
        };
        engine.check_consistency().map_err(|e| bad(format!("the engine: {e}")))?;
        let mirror = params.panel_v3().ok_or_else(|| bad("an engine exists but the fence is not configured".into()))?;
        let cursor = engine.cursor();
        if cursor.network != mirror.network || cursor.ruleset != mirror.ruleset || cursor.policy != mirror.policy {
            return Err(bad("the engine's identity or policy differs from the fence's".into()));
        }
        for (id, record) in engine.claim_rows() {
            let claim = self.claims.get(id);
            // A claim V2 licensed, ended or retired in this block's objects leaves its Panel: the engine releases it at the
            // NEXT block's advance, so a live record whose claim no longer awaits its Panel is not an inconsistency.
            let left_its_panel = !record.phase.terminal() && !claim_awaits_its_panel(claim);
            match &record.phase {
                _ if left_its_panel => {}
                ClaimPhaseV3::PendingSeal | ClaimPhaseV3::Sealed | ClaimPhaseV3::EntropyReady { .. } => {
                    if !claim.is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::Provisional)) {
                        return Err(bad(format!("claim {id} awaits its draw but is not Provisional")));
                    }
                    if self.panel_duties.contains_key(id) {
                        return Err(bad(format!("claim {id} awaits its draw but holds a duty row")));
                    }
                }
                ClaimPhaseV3::Bound(binding) => {
                    let Some(claim) = claim else { return Err(bad(format!("claim {id} is bound but not in the claim table"))) };
                    if !matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. } | PalwClaimPhaseV2::DefaultDisputed { .. }) {
                        return Err(bad(format!("claim {id} is bound in the engine but its phase is {:?}", claim.phase)));
                    }
                    let panel = self.panels.get(id).ok_or_else(|| bad(format!("claim {id} is bound but holds no panel record")))?;
                    let seats: Vec<PalwBondKeyV2> = binding.seats.iter().map(|seat| panel_bond_key_v1(*seat)).collect();
                    if panel.anchor != binding.panel_seed_v3 || panel.seats.iter().map(|seat| seat.bond).collect::<Vec<_>>() != seats {
                        return Err(bad(format!("claim {id}'s panel record is not its binding")));
                    }
                    let row = self.panel_duties.get(id).ok_or_else(|| bad(format!("claim {id} is bound but holds no duty row")))?;
                    if row.seat_exposure != binding.exposure as u128 || row.seats.keys().copied().collect::<BTreeSet<_>>() != seats.iter().copied().collect() {
                        return Err(bad(format!("claim {id}'s duty row is not its binding")));
                    }
                }
                ClaimPhaseV3::Released { .. } => {
                    if claim_awaits_its_panel(claim) {
                        return Err(bad(format!("claim {id} was released but still awaits its Panel")));
                    }
                }
                ClaimPhaseV3::Voided { .. } => {
                    if claim.is_some_and(|claim| matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. })) {
                        return Err(bad(format!("claim {id} ended in the engine but is still waiting in V2")));
                    }
                }
            }
        }
        for (id, claim) in &self.claims {
            if !matches!(claim.phase, PalwClaimPhaseV2::Provisional | PalwClaimPhaseV2::PanelBound { .. }) {
                continue;
            }
            let tracked = engine.claim_rows().contains_key(id);
            if params.panel_v3_rule_at(claim.accepted_daa) != tracked {
                return Err(bad(format!("claim {id}: accepted under the V3 rule = {}, tracked by the engine = {tracked}", !tracked)));
            }
        }
        Ok(())
    }
}
