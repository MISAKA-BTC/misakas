//! **Lane LG14-A: the legacy route's dispute reservations in the fold** — tags 154–155, the lapse at the claim's hard deadline, the
//! release inside every conviction / void / default (`da_release_all_v1`), the burn at the claim's retirement, and the reserved DA
//! budget. A child module of `palw_state_v2`, so it reads the builder and the tables and writes `legacy_disputes` only through its one
//! journaled writer (delta 200). See [`crate::palw_legacy_public_filer_v1`] for the design.
//!
//! A structural refusal is an `Err`: the acceptance walk's rehearsal drops the object and the block stands. The sweep never fails a
//! block on a reservation row: what it cannot do (a reserver bond that is gone) it skips.

use super::*;
use crate::palw_legacy_public_filer_v1::*;

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::LegacyDisputeRefused(why.into())
}

/// **The two derived indexes, from the rooted map alone** — the one derivation the load, the writer and the consistency check
/// share: `(hard_deadline, claim)` for every record that holds its claim (the lapse sweep's queue), and `(bond, claim)` for every
/// bond with a live or held deposit on the claim (the A-6 exposure ledger's).
#[allow(clippy::type_complexity)]
pub(super) fn palw_legacy_dispute_indexes_of_v1(
    disputes: &BTreeMap<Hash64, PalwDisputeClaimV1>,
) -> (BTreeSet<(u64, Hash64)>, BTreeSet<(PalwBondKeyV2, Hash64)>) {
    let mut deadlines = BTreeSet::new();
    let mut by_reserver = BTreeSet::new();
    for (claim_id, record) in disputes {
        if record.holds() {
            deadlines.insert((record.hard_deadline_daa, *claim_id));
        }
        for bond in record.live.keys().chain(record.dismissed_held.iter().map(|(bond, _)| bond)) {
            by_reserver.insert((*bond, *claim_id));
        }
    }
    (deadlines, by_reserver)
}

// ---- reads ------------------------------------------------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// A claim's dispute record, if any reservation was ever accepted on it (and it still holds something).
    pub fn legacy_dispute_v1(&self, claim_id: &Hash64) -> Option<&PalwDisputeClaimV1> {
        self.legacy_disputes.get(claim_id)
    }

    /// Every dispute record, in claim order — a read for the RPC and the tests, never a consensus path.
    pub fn legacy_disputes_iter_v1(&self) -> impl Iterator<Item = (&Hash64, &PalwDisputeClaimV1)> + '_ {
        self.legacy_disputes.iter()
    }

    /// **Does a live reservation hold `claim_id`'s ends?** `false` on every chain below `palw_legacy_public_filer_v1` (the table is
    /// empty there), which is what keeps every reader of it byte-identical below the fence.
    pub fn palw_legacy_dispute_holds_v1(&self, claim_id: &Hash64) -> bool {
        self.legacy_disputes.get(claim_id).is_some_and(|record| record.holds())
    }

    /// `bond`'s live reservation on `claim_id`.
    pub fn legacy_dispute_reservation_v1(&self, claim_id: &Hash64, bond: &PalwBondKeyV2) -> Option<&PalwDisputeReservationRowV1> {
        self.legacy_disputes.get(claim_id).and_then(|record| record.live.get(bond))
    }

    /// The claims on which `bond` holds a live or held deposit, in claim order.
    pub fn legacy_disputes_of_v1(&self, bond: &PalwBondKeyV2) -> impl Iterator<Item = Hash64> + '_ {
        let bond = *bond;
        self.legacy_disputes_by_reserver
            .range((bond, ZERO_HASH64)..)
            .take_while(move |(held, _)| *held == bond)
            .map(|(_, claim)| *claim)
    }

    /// **A-6: what `bond` holds through reservations** — its live deposits and its dismissed ones not yet refunded or burned. Read by
    /// [`palw_accuser_exposure_v1`], so a deposit sits on the reserver's free half exactly as a DA session's exposure does.
    pub(crate) fn legacy_dispute_exposure_v1(&self, bond: &PalwBondKeyV2) -> u128 {
        self.legacy_disputes_of_v1(bond)
            .filter_map(|claim| self.legacy_disputes.get(&claim))
            .map(|record| record.exposure_of(bond))
            .fold(0u128, u128::saturating_add)
    }

    /// Live reservations `bond` holds, across claims.
    pub fn legacy_live_reservations_of_v1(&self, bond: &PalwBondKeyV2) -> usize {
        self.legacy_disputes_of_v1(bond).filter(|claim| self.legacy_dispute_reservation_v1(claim, bond).is_some()).count()
    }

    /// **The records agree with each other, with the claims and with their indexes** — what a carriage somebody else wrote must
    /// satisfy before it is believed. Below `palw_rcore_plus` the table is empty (no writer runs there).
    pub(crate) fn assert_legacy_disputes_consistency_v1(&self, params: &PalwStateParamsV2) -> Result<(), PalwStateV2Error> {
        let bad = |why: String| Err(PalwStateV2Error::CarriageInconsistent(format!("legacy dispute: {why}")));
        if params.rcore_plus_from_daa().is_none() && !self.legacy_disputes.is_empty() {
            return bad("records on a network where palw_rcore_plus is dormant".into());
        }
        if palw_legacy_dispute_indexes_of_v1(&self.legacy_disputes)
            != (self.legacy_dispute_deadlines.clone(), self.legacy_disputes_by_reserver.clone())
        {
            return bad("the indexes differ from the records".into());
        }
        let mut live_per_bond: BTreeMap<PalwBondKeyV2, usize> = BTreeMap::new();
        for (claim_id, record) in &self.legacy_disputes {
            let Some(claim) = self.claims.get(claim_id) else {
                return bad(format!("a record names claim {claim_id}, which the state does not hold"));
            };
            if record.live.is_empty() && record.dismissed_held.is_empty() {
                return bad(format!("claim {claim_id}'s record holds nothing and was not deleted"));
            }
            if record.holds() && matches!(claim.phase, PalwClaimPhaseV2::Voided { .. } | PalwClaimPhaseV2::DefaultDisputed { .. }) {
                return bad(format!("claim {claim_id} is voided with a live reservation (every void releases them)"));
            }
            if record.live.len() > PALW_DISPUTE_LIVE_RESERVATIONS_PER_CLAIM_V1
                || record.reservers_total() > PALW_DISPUTE_RESERVERS_PER_CLAIM_TOTAL_V1
                || record.live.keys().any(|bond| record.closed.contains(bond))
                || record.live.values().any(|row| row.sessions_opened > PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1 || row.deposit == 0)
                || record.dismissed_held.iter().any(|(bond, amount)| *amount == 0 || !record.closed.contains(bond))
            {
                return bad(format!("claim {claim_id}'s record exceeds its caps or contradicts itself"));
            }
            if record.live.keys().any(|bond| *bond == claim.bond) {
                return bad(format!("claim {claim_id}'s producer holds a reservation on it"));
            }
            for bond in record.live.keys() {
                *live_per_bond.entry(*bond).or_default() += 1;
            }
        }
        if live_per_bond.values().any(|live| *live > PALW_DISPUTE_LIVE_RESERVATIONS_PER_BOND_V1) {
            return bad("a bond holds more live reservations than its cap".into());
        }
        Ok(())
    }
}

// ---- the writer and the outcomes ----------------------------------------------------------------------------------------------

impl TransitionBuilder<'_> {
    /// **The one writer of `legacy_disputes`** (delta 200): the record, its two indexes, journaled.
    pub(super) fn write_legacy_dispute(&mut self, claim_id: Hash64, new: Option<PalwDisputeClaimV1>) {
        let old = match &new {
            Some(record) => self.state.legacy_disputes.insert(claim_id, record.clone()),
            None => self.state.legacy_disputes.remove(&claim_id),
        };
        if old == new {
            return;
        }
        if let Some(previous) = &old {
            self.state.legacy_dispute_deadlines.remove(&(previous.hard_deadline_daa, claim_id));
            for bond in previous.live.keys().chain(previous.dismissed_held.iter().map(|(bond, _)| bond)) {
                self.state.legacy_disputes_by_reserver.remove(&(*bond, claim_id));
            }
        }
        if let Some(record) = &new {
            if record.holds() {
                self.state.legacy_dispute_deadlines.insert((record.hard_deadline_daa, claim_id));
            }
            for bond in record.live.keys().chain(record.dismissed_held.iter().map(|(bond, _)| bond)) {
                self.state.legacy_disputes_by_reserver.insert((*bond, claim_id));
            }
        }
        self.entries.push(PalwDeltaEntryV2::LegacyDispute { key: claim_id, old, new });
    }

    /// **The deposit a reservation of `claim_id` holds at `now`**: DA-6's session exposure at the claim's stage —
    /// `min(⌈r · S_P(stage)⌉, min_collateral)` with INTF's `r` (1,000 bps below `palw_reporter_share_v2`, 4,900 past it), the same
    /// arithmetic `da_admission_v1` prices a session with.
    fn legacy_dispute_deposit_v1(&self, claim_id: &Hash64, claim: &PalwClaimStateV2, now_daa: u64) -> Result<u128, PalwStateV2Error> {
        use crate::palw_da_rcore_v1::{PalwDaStageV1, palw_da_session_exposure_at_bps_v1, palw_da_stage_reward_base_v1};
        let stage = match claim.phase {
            PalwClaimPhaseV2::Final { .. } => PalwDaStageV1::FinalRow,
            PalwClaimPhaseV2::ReceiptLicensed { .. } => PalwDaStageV1::Licensed,
            _ => PalwDaStageV1::Live,
        };
        let full = palw_claim_bond_reservation_v1(self.params, claim).ok_or(PalwStateV2Error::Overflow("dispute deposit"))?;
        let producer_collateral = self.state.bonds.get(&claim.bond).map(|bond| bond.collateral).unwrap_or(0);
        let g = palw_claim_g_v1(&self.state, &crate::palw_weight_cap_v1::PalwCapacityGainScaleV1::of(self.params), claim_id)
            .map(|gains| gains.g())
            .unwrap_or(0);
        Ok(palw_da_session_exposure_at_bps_v1(
            palw_da_stage_reward_base_v1(stage, full, producer_collateral, g),
            self.params.min_collateral_sompi(),
            self.params.reporter_reward_bps_at(now_daa),
        ))
    }

    /// **A reserved DA session opened** (`open_da_session_rcore_v1`): counted on its reservation's own budget.
    pub(super) fn legacy_dispute_count_session_v1(&mut self, claim_id: Hash64, bond: PalwBondKeyV2) {
        let Some(mut record) = self.state.legacy_disputes.get(&claim_id).cloned() else { return };
        let Some(row) = record.live.get_mut(&bond) else { return };
        row.sessions_opened = row.sessions_opened.saturating_add(1);
        self.write_legacy_dispute(claim_id, Some(record));
    }

    /// **The hold ended** (the last live reservation closed): DL-1 re-derives the claim's deadline from its own anchors — no time
    /// is credited (the hold is the challenger's), and a deadline that passed during it is swept in the next block.
    fn legacy_dispute_hold_ended_v1(&mut self, claim_id: Hash64, now_daa: u64) -> Result<(), PalwStateV2Error> {
        if self.state.claims.contains_key(&claim_id) && !self.state.palw_legacy_dispute_holds_v1(&claim_id) {
            self.rearm_claim_deadline_dl1_v1(claim_id, now_daa)?;
        }
        Ok(())
    }

    /// **`bonds`' reservations on `claim_id` end without an objective outcome** (a release, the lapse): each deposit is held in
    /// `dismissed_held` until the claim resolves — refunded by a later conviction, burned at retirement.
    fn legacy_dispute_dismiss_v1(&mut self, claim_id: Hash64, bonds: &[PalwBondKeyV2], now_daa: u64) -> Result<(), PalwStateV2Error> {
        let Some(mut record) = self.state.legacy_disputes.get(&claim_id).cloned() else { return Ok(()) };
        let held = record.holds();
        for bond in bonds {
            if let Some(row) = record.live.remove(bond) {
                record.closed.insert(*bond);
                record.dismissed_held.push((*bond, row.deposit));
            }
        }
        self.write_legacy_dispute(claim_id, Some(record));
        if held {
            self.legacy_dispute_hold_ended_v1(claim_id, now_daa)?;
        }
        Ok(())
    }

    /// **Every reservation on `claim_id` closes with the claim's outcome** — called inside `da_release_all_v1`, which every void,
    /// conviction, `Final` reversal and DA default already calls. Live deposits are refunded (removing the row releases the A-6
    /// exposure); with `convicted`, every held deposit is refunded too. A record left holding nothing is deleted.
    pub(super) fn legacy_dispute_release_all_v1(
        &mut self,
        claim_id: Hash64,
        convicted: bool,
        now_daa: u64,
    ) -> Result<(), PalwStateV2Error> {
        let Some(mut record) = self.state.legacy_disputes.get(&claim_id).cloned() else { return Ok(()) };
        let held = record.holds();
        let live: Vec<PalwBondKeyV2> = record.live.keys().copied().collect();
        record.live.clear();
        record.closed.extend(live);
        if convicted {
            record.dismissed_held.clear();
        }
        let keep = !record.dismissed_held.is_empty();
        self.write_legacy_dispute(claim_id, keep.then_some(record));
        if held {
            self.legacy_dispute_hold_ended_v1(claim_id, now_daa)?;
        }
        Ok(())
    }

    /// **The claim retires: its record goes with it, burning every held deposit no conviction refunded** (`slash_seat`, capped at
    /// the floor, earning nobody anything — DA-6's rule for `refuted_held`). Retirement never runs while a reservation holds the claim
    /// (DL-1 gives it no deadline). A reserver bond that is gone is skipped.
    pub(super) fn legacy_dispute_retire_v1(&mut self, claim_id: Hash64) -> Result<(), PalwStateV2Error> {
        let Some(record) = self.state.legacy_disputes.get(&claim_id).cloned() else { return Ok(()) };
        if record.holds() {
            return Err(PalwStateV2Error::CarriageInconsistent(format!(
                "claim {claim_id} retired with a live reservation (DL-1 defers retirement while one holds it)"
            )));
        }
        for (bond, amount) in &record.dismissed_held {
            if self.state.bonds.contains_key(bond) {
                self.slash_seat(*bond, *amount, self.params.min_collateral_sompi())?;
            }
        }
        self.write_legacy_dispute(claim_id, None);
        Ok(())
    }
}

// ---- the arms -----------------------------------------------------------------------------------------------------------------

/// **Tag 154: a reservation** (RFC-0014 §7.2). Past the fence; the claim has a pursuit to hold; the roots are the claim's; the
/// reserver is an Active bond at or above the floor and not the producer, has never reserved this claim, and is inside the caps; the
/// claim's hard deadline has not passed; the deposit fits the reserver's free half. The signature is the processor's.
pub(super) fn apply_dispute_reserved_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reservation: &PalwDisputeReservationV1,
) -> Result<(), PalwStateV2Error> {
    let now = ctx.daa_score;
    if !builder.params.legacy_public_filer_active_at(now) {
        return Err(refused("palw_legacy_public_filer_v1 is not in force at this block"));
    }
    if reservation.version != PALW_DISPUTE_RESERVATION_VERSION_V1 {
        return Err(refused("a reservation of an unknown version"));
    }
    let claim_id = reservation.claim;
    let claim = builder.state.claims.get(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    let final_row_open = matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
        && builder.state.vesting.get(&claim_id).is_some_and(|row| row.matured_at.is_none());
    if !palw_dispute_phase_reservable_v1(&claim.phase, final_row_open) {
        return Err(refused("the claim has no pursuit a reservation may hold (voided, or Final past its post-Final stage)"));
    }
    if reservation.execution_root != claim.execution_root || reservation.trace_root != claim.trace_root {
        return Err(refused("the reservation names roots that are not the claim's"));
    }
    let reserver = reservation.reserver;
    if reserver == claim.bond {
        return Err(refused("a producer cannot reserve a pursuit of its own claim"));
    }
    let bond = builder.state.bonds.get(&reserver).cloned().ok_or(PalwStateV2Error::MissingBond(reserver))?;
    if !matches!(bond.status, PalwBondStatusV2::Active) {
        return Err(PalwStateV2Error::BondNotActive(reserver));
    }
    let floor = builder.params.min_collateral_sompi();
    if bond.collateral < floor {
        return Err(PalwStateV2Error::BondBelowFloor { bond: reserver, collateral: bond.collateral, floor });
    }
    let w_disclose = palw_da_disclose_window_daa_v1(builder.params);
    let mut record = builder.state.legacy_disputes.get(&claim_id).cloned().unwrap_or_else(|| PalwDisputeClaimV1 {
        opened_daa: now,
        hard_deadline_daa: palw_dispute_hard_deadline_v1(&claim, w_disclose),
        ..Default::default()
    });
    if now > record.hard_deadline_daa {
        return Err(refused("the claim's hard deadline has passed: no data-availability session could still open on it"));
    }
    if record.knows(&reserver) {
        return Err(refused("this bond has reserved the claim before: one reservation per bond per claim over the claim's life"));
    }
    if record.live.len() >= PALW_DISPUTE_LIVE_RESERVATIONS_PER_CLAIM_V1 {
        return Err(refused("the claim holds as many live reservations as it may"));
    }
    if record.reservers_total() >= PALW_DISPUTE_RESERVERS_PER_CLAIM_TOTAL_V1 {
        return Err(refused("the claim has admitted as many reservers as it may over its life"));
    }
    if builder.state.legacy_live_reservations_of_v1(&reserver) >= PALW_DISPUTE_LIVE_RESERVATIONS_PER_BOND_V1 {
        return Err(refused("the bond holds as many live reservations as it may"));
    }
    let deposit = builder.legacy_dispute_deposit_v1(&claim_id, &claim, now)?;
    if deposit == 0 {
        return Err(refused("the claim prices no deposit (it reserves nothing)"));
    }
    builder.check_accuser_room(reserver, deposit, "dispute reservation", now)?;
    let held_before = record.holds();
    let hard = record.hard_deadline_daa;
    record.live.insert(reserver, PalwDisputeReservationRowV1 { reserved_daa: now, deposit, sessions_opened: 0 });
    builder.write_legacy_dispute(claim_id, Some(record));
    // The hold: DL-1 gives the claim no deadline while the record holds it (`palw_rcore_deadline_v1`, `arm_deadline`).
    if !held_before {
        builder.disarm_deadline(claim_id);
    }
    // The vesting row and the signers' `Valid` locks follow the hold, as they follow a DA session (DA-5 / L-3): a conviction landing
    // inside it finds them unmatured and live.
    let until = hard.saturating_add(builder.params.window_challenge_at(now));
    builder.da_rekey_v1(claim_id, until, now);
    Ok(())
}

/// **Tag 155: the reserver ends its own reservation.** Past the fence; a live reservation of this bond on this claim. Its deposit is
/// held until the claim resolves. The signature is the processor's.
pub(super) fn apply_dispute_released_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    reserver: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.legacy_public_filer_active_at(ctx.daa_score) {
        return Err(refused("palw_legacy_public_filer_v1 is not in force at this block"));
    }
    if builder.state.legacy_dispute_reservation_v1(&claim_id, &reserver).is_none() {
        return Err(refused("no live reservation of this bond on this claim"));
    }
    builder.legacy_dispute_dismiss_v1(claim_id, &[reserver], ctx.daa_score)
}

/// **The lapse** (step 2, right after the DA sweep, in the fold and the pre-object base): every claim whose hard deadline is behind
/// this block loses its live reservations, in `(hard_deadline, claim)` order.
pub(super) fn sweep_legacy_disputes(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    while let Some(&(deadline, claim_id)) = builder.state.legacy_dispute_deadlines.iter().next() {
        if deadline >= ctx.daa_score {
            break;
        }
        let live: Vec<PalwBondKeyV2> =
            builder.state.legacy_disputes.get(&claim_id).map(|record| record.live.keys().copied().collect()).unwrap_or_default();
        if live.is_empty() {
            // Unreachable: the index holds exactly the records that hold. Dropped rather than looped on.
            builder.state.legacy_dispute_deadlines.remove(&(deadline, claim_id));
            continue;
        }
        builder.legacy_dispute_dismiss_v1(claim_id, &live, ctx.daa_score)?;
    }
    Ok(())
}

// ---- the public read (RFC-0014 §6.3; RPC 204) and the engine's admission check ------------------------------------------------

impl PalwChainStateV2 {
    /// **One claim's dispute view** — what any node serves to any verifier (no operator allowlist, no secret): the claim's identity,
    /// roots and retention, DL-1's deadline and the hard deadline a reservation would hold it to, the dispute record, the open DA
    /// sessions with their units, the answered units, the open courts, and which of its convictions are recorded. `None` for a claim
    /// the state does not hold.
    pub fn palw_legacy_dispute_view_v1(&self, params: &PalwStateParamsV2, claim_id: &Hash64) -> Option<PalwLegacyDisputeViewV1> {
        let claim = self.claims.get(claim_id)?;
        let producer = claim.bond.0;
        let recorded = |key: Hash64| self.consumed_offences.contains_key(&key);
        Some(PalwLegacyDisputeViewV1 {
            claim_id: *claim_id,
            class_id: claim.class_id,
            producer: claim.bond,
            phase: claim.phase.clone(),
            execution_root: claim.execution_root,
            trace_root: claim.trace_root,
            accepted_daa: claim.accepted_daa,
            trace_retention_daa: claim.trace_retention_daa,
            job_identity: claim.job_identity,
            work_leaves: claim.work_leaves,
            deadline_daa: self.deadline_of(claim_id),
            hard_deadline_daa: palw_dispute_hard_deadline_v1(claim, palw_da_disclose_window_daa_v1(params)),
            record: self.legacy_disputes.get(claim_id).cloned(),
            sessions: self.da_sessions_of(claim_id).map(|(accuser, session)| (*accuser, session.clone())).collect(),
            answered: self.da_claims.get(claim_id).map(|record| record.answered.iter().copied().collect()).unwrap_or_default(),
            open_courts: self.open_courts_of(claim_id),
            executor_refuted: recorded(crate::palw_offence_attribution_v1::palw_executor_refuted_offence_id_v1(&producer, claim_id)),
            court_convicted: recorded(palw_court_conviction_offence_id_v1(&producer, claim_id)),
            da_defaulted: recorded(crate::palw_da_rcore_v1::palw_da_offence_id_v1(&producer, claim_id)),
        })
    }

    /// The claims a live reservation holds, in claim order (those of `reserver` when given) — RPC 205's list.
    pub fn palw_legacy_disputes_live_v1(&self, reserver: Option<&PalwBondKeyV2>, limit: usize) -> Vec<Hash64> {
        match reserver {
            Some(bond) => self
                .legacy_disputes_of_v1(bond)
                .filter(|claim| self.legacy_dispute_reservation_v1(claim, bond).is_some())
                .take(limit)
                .collect(),
            None => self.legacy_disputes.iter().filter(|(_, record)| record.holds()).map(|(claim, _)| *claim).take(limit).collect(),
        }
    }
}

/// **The view RPC 204 serves** ([`PalwChainStateV2::palw_legacy_dispute_view_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwLegacyDisputeViewV1 {
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub phase: PalwClaimPhaseV2,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub accepted_daa: u64,
    pub trace_retention_daa: u64,
    /// The job the claim recorded (an attempt's carrying-header anchor; a free prompt's pin): what a verifier replays.
    pub job_identity: Hash64,
    pub work_leaves: u64,
    /// DL-1's deadline (`None` while a reservation, a seat-like session or a court holds the claim).
    pub deadline_daa: Option<u64>,
    /// `trace_retention_daa − W_disclose`: the last DAA a reservation or a DA session may open.
    pub hard_deadline_daa: u64,
    pub record: Option<PalwDisputeClaimV1>,
    pub sessions: Vec<(PalwBondKeyV2, crate::palw_da_rcore_v1::PalwDaSessionV1)>,
    pub answered: Vec<crate::palw_da_rcore_v1::PalwDaUnitV1>,
    pub open_courts: u32,
    pub executor_refuted: bool,
    pub court_convicted: bool,
    pub da_defaulted: bool,
}

/// **Would the fold take this reservation on `state` at `now_daa`?** The fold's own tag-154 arm on a scratch builder — what a filer
/// asks before it pays a carrier, so the reservation it sends is one the chain admits. `Ok(deposit)` or the fold's refusal.
pub fn palw_legacy_dispute_reservation_check_v1(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    reservation: &PalwDisputeReservationV1,
    now_daa: u64,
) -> Result<u128, PalwStateV2Error> {
    let mut builder = TransitionBuilder::new(state, params, false, false, false, true, extras);
    let ctx = PalwBlockContextV2 { block: Hash64::default(), daa_score: now_daa, blue_score: 0, subsidy: 0 };
    apply_dispute_reserved_v1(&mut builder, &ctx, reservation)?;
    Ok(builder.state.legacy_dispute_reservation_v1(&reservation.claim, &reservation.reserver).map(|row| row.deposit).unwrap_or(0))
}

// ---- the common filer's candidates (RFC-0014 §6.1's Discover) and the RPC observations (ops 204, 206) ----------------------------

/// **One claim the common filer may pursue** — the node's read of the tip ([`PalwChainStateV2::palw_fraud_filer_candidates_v1`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PalwFraudFilerCandidateV1 {
    pub claim_id: Hash64,
    pub producer: PalwBondKeyV2,
    pub accepted_daa: u64,
    /// The filer's bond is a seat of the claim's current panel: its sessions are the seat's duties' (P2-6, P2-8), never this filer's.
    pub seat: bool,
    /// What the node replays (lane B's job facts: the claim's block, class, artifact root and committed roots).
    pub job: crate::palw_operator_da_v1::PalwOperatorDaJobV1,
}

/// One live reservation, as op 206 reports it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwFraudFilerReservationStatusV1 {
    pub claim_id: Hash64,
    pub reserved_daa: u64,
    /// Sompi, a decimal string.
    pub deposit: String,
    pub sessions_opened: u8,
    pub hard_deadline_daa: u64,
}

/// **What `getPalwFraudFilerStatus` (RPC op 206) answers for one bond**: the fence, the bond's live reservations and the deposits held
/// on it, and the A-6 exposure they put on its free half. A pure read of the tip state.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwFraudFilerStatusV1 {
    pub version: u16,
    /// `palw_legacy_public_filer_v1`'s height, `None` while dormant (every shipped preset).
    pub fence_daa: Option<u64>,
    pub active: bool,
    pub bond: String,
    pub live: Vec<PalwFraudFilerReservationStatusV1>,
    /// `(claim, sompi)` of every deposit held after a release or a lapse (refunded at a conviction, burned at retirement).
    pub held: Vec<(Hash64, String)>,
    /// The bond's reservation exposure on its free half (live and held deposits), sompi.
    pub exposure: String,
    pub live_cap: u32,
    pub sessions_per_reservation: u8,
}

/// The observation version of ops 204 and 206 (fields are only ever appended).
pub const PALW_LEGACY_DISPUTE_OBSERVATION_VERSION_V1: u16 = 1;

/// **What `getPalwLegacyDispute` (RPC op 204) answers**: one claim's dispute view ([`PalwLegacyDisputeViewV1`]) as camelCase JSON —
/// bonds `txid:index`, amounts decimal sompi, phases and units by name.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PalwLegacyDisputeObservationV1 {
    pub version: u16,
    pub claim_id: Hash64,
    pub class_id: Hash64,
    pub producer: String,
    pub phase: String,
    pub execution_root: Hash64,
    pub trace_root: Hash64,
    pub accepted_daa: u64,
    pub trace_retention_daa: u64,
    pub job_identity: Hash64,
    pub work_leaves: u64,
    pub deadline_daa: Option<u64>,
    pub hard_deadline_daa: u64,
    pub held: bool,
    /// `(reserver, reserved_daa, deposit, sessions_opened)`.
    pub reservations: Vec<(String, u64, String, u8)>,
    pub closed: Vec<String>,
    pub dismissed_held: Vec<(String, String)>,
    /// `(accuser, opened_daa, deadline_daa, seat, exposure, units)`.
    pub sessions: Vec<(String, u64, u64, bool, String, Vec<String>)>,
    pub answered: Vec<String>,
    pub open_courts: u32,
    pub executor_refuted: bool,
    pub court_convicted: bool,
    pub da_defaulted: bool,
}

/// `txid:index`, the form the RPC parses bonds in.
pub fn palw_bond_text_v1(bond: &PalwBondKeyV2) -> String {
    format!("{}:{}", bond.0.transaction_id, bond.0.index)
}

impl PalwLegacyDisputeObservationV1 {
    pub fn of(view: &PalwLegacyDisputeViewV1) -> Self {
        let record = view.record.as_ref();
        Self {
            version: PALW_LEGACY_DISPUTE_OBSERVATION_VERSION_V1,
            claim_id: view.claim_id,
            class_id: view.class_id,
            producer: palw_bond_text_v1(&view.producer),
            phase: format!("{:?}", view.phase),
            execution_root: view.execution_root,
            trace_root: view.trace_root,
            accepted_daa: view.accepted_daa,
            trace_retention_daa: view.trace_retention_daa,
            job_identity: view.job_identity,
            work_leaves: view.work_leaves,
            deadline_daa: view.deadline_daa,
            hard_deadline_daa: view.hard_deadline_daa,
            held: record.is_some_and(PalwDisputeClaimV1::holds),
            reservations: record
                .map(|record| {
                    record
                        .live
                        .iter()
                        .map(|(bond, row)| (palw_bond_text_v1(bond), row.reserved_daa, row.deposit.to_string(), row.sessions_opened))
                        .collect()
                })
                .unwrap_or_default(),
            closed: record.map(|record| record.closed.iter().map(palw_bond_text_v1).collect()).unwrap_or_default(),
            dismissed_held: record
                .map(|record| {
                    record.dismissed_held.iter().map(|(bond, amount)| (palw_bond_text_v1(bond), amount.to_string())).collect()
                })
                .unwrap_or_default(),
            sessions: view
                .sessions
                .iter()
                .map(|(accuser, session)| {
                    (
                        palw_bond_text_v1(accuser),
                        session.opened_daa,
                        session.deadline_daa,
                        session.accuser_is_seat,
                        session.exposure.to_string(),
                        session.units.iter().map(|unit| format!("{unit:?}")).collect(),
                    )
                })
                .collect(),
            answered: view.answered.iter().map(|unit| format!("{unit:?}")).collect(),
            open_courts: view.open_courts,
            executor_refuted: view.executor_refuted,
            court_convicted: view.court_convicted,
            da_defaulted: view.da_defaulted,
        }
    }

    /// The camelCase JSON document.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an observation is plain data and serializes")
    }
}

impl PalwFraudFilerStatusV1 {
    /// The camelCase JSON document.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("an observation is plain data and serializes")
    }
}

impl PalwChainStateV2 {
    /// **The claims `me`'s filer may pursue at `now_daa`**, oldest acceptance first: past the fence, every claim at a reservable stage
    /// (`palw_dispute_phase_reservable_v1`: unlicensed, licensed, or `Final` with its vesting row unmatured) that is not `me`'s own,
    /// and every claim on which `me` holds a dispute record (so a restarted filer finds its own pursuits). Empty below the fence.
    pub fn palw_fraud_filer_candidates_v1(
        &self,
        params: &PalwStateParamsV2,
        me: &PalwBondKeyV2,
        now_daa: u64,
    ) -> Vec<PalwFraudFilerCandidateV1> {
        if !params.legacy_public_filer_active_at(now_daa) {
            return Vec::new();
        }
        let mut out: Vec<PalwFraudFilerCandidateV1> = self
            .claims
            .iter()
            .filter(|(claim_id, claim)| {
                let known = self.legacy_disputes.get(claim_id).is_some_and(|record| record.knows(me));
                let final_row_open = matches!(claim.phase, PalwClaimPhaseV2::Final { .. })
                    && self.vesting.get(claim_id).is_some_and(|row| row.matured_at.is_none());
                claim.bond != *me && (known || palw_dispute_phase_reservable_v1(&claim.phase, final_row_open))
            })
            .filter_map(|(claim_id, claim)| {
                let facts = crate::palw_operator_da_v1::palw_operator_da_claim_facts_v1(self, claim_id)?;
                Some(PalwFraudFilerCandidateV1 {
                    claim_id: *claim_id,
                    producer: claim.bond,
                    accepted_daa: claim.accepted_daa,
                    seat: facts.seats.contains(me),
                    job: facts.job,
                })
            })
            .collect();
        out.sort_by_key(|candidate| (candidate.accepted_daa, candidate.claim_id));
        out
    }

    /// **Op 206's read: `bond`'s reservations at `now_daa`** — `None` for a bond the state does not hold.
    pub fn palw_fraud_filer_status_v1(
        &self,
        params: &PalwStateParamsV2,
        bond: &PalwBondKeyV2,
        now_daa: u64,
    ) -> Option<PalwFraudFilerStatusV1> {
        self.bond(bond)?;
        let mut live = Vec::new();
        let mut held = Vec::new();
        for claim_id in self.legacy_disputes_of_v1(bond) {
            let Some(record) = self.legacy_disputes.get(&claim_id) else { continue };
            if let Some(row) = record.live.get(bond) {
                live.push(PalwFraudFilerReservationStatusV1 {
                    claim_id,
                    reserved_daa: row.reserved_daa,
                    deposit: row.deposit.to_string(),
                    sessions_opened: row.sessions_opened,
                    hard_deadline_daa: record.hard_deadline_daa,
                });
            }
            for (holder, amount) in &record.dismissed_held {
                if holder == bond {
                    held.push((claim_id, amount.to_string()));
                }
            }
        }
        Some(PalwFraudFilerStatusV1 {
            version: PALW_LEGACY_DISPUTE_OBSERVATION_VERSION_V1,
            fence_daa: params.legacy_public_filer_from_daa(),
            active: params.legacy_public_filer_active_at(now_daa),
            bond: palw_bond_text_v1(bond),
            live,
            held,
            exposure: self.legacy_dispute_exposure_v1(bond).to_string(),
            live_cap: PALW_DISPUTE_LIVE_RESERVATIONS_PER_BOND_V1 as u32,
            sessions_per_reservation: PALW_DISPUTE_SESSIONS_PER_RESERVATION_V1,
        })
    }
}
