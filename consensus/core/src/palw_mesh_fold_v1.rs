//! **RFC-0007 Parts II and IV in the fold: the witness profile, the audit mesh with its traps, and capped onboarding** — each dormant
//! behind its own fence ([`crate::palw_mesh_v1`]). A child module of `palw_state_v2`, as the vertex fold is, so it reads the builder
//! and the state's tables directly and writes them only through their one writers. The objects, their hashing and the stateless
//! checks are [`crate::palw_mesh_v1`]'s; spec 18 is normative.
//!
//! # The audit draw
//!
//! A claim of an IR class, at acceptance past `palw_audit_mesh_v1`, draws its auditors ([`on_claim_accepted_v1`]): every Active bond
//! registered before the claim, other than the producer's, with free collateral for the penalty, weighted by stake, one seat per
//! operator, [`PALW_AUDITS_PER_CLAIM_V1`] of them. The draw's row carries each auditor's leaf ticket and its reservation (the trap
//! penalty, held in `reserved_exposure` so every free-collateral reader and the withdrawal rule see it). **The hook cannot fail**: it
//! runs after the claim is written, where a refusal would strand the write (`apply_attempt`'s own-attempt skip), so what it cannot do
//! it leaves undone — an unaudited claim is a sensor that missed, never a refused block.
//!
//! # Audited leaves, pay, traps
//!
//! An `Audited` leaf of a vertex ([`count_audited_leaf_v1`]) counts when its seat is a drawn auditor of the claim, names the drawn
//! ticket, was signed inside the audit window, and is the seat's first answer: the outcome is recorded and the audit paid out of the
//! panel reserve. Silence is never a match. [`apply_trap_committed_v1`] / [`apply_trap_revealed_v1`] settle the verifier's dilemma.

use super::*;
use crate::palw_mesh_v1::{
    PALW_AUDIT_MAX_OPEN_V1, PALW_AUDIT_WINDOW_DAA_V1, PALW_AUDITS_PER_CLAIM_V1, PALW_MESH_SWEEP_PER_BLOCK_V1, PALW_TRAP_DEPOSIT_SOMPI_V1,
    PALW_TRAP_MAX_OPEN_V1, PALW_TRAP_MAX_TILES_V1, PALW_TRAP_REVEAL_WINDOW_DAA_V1, PalwAuditAssignmentV1, PalwAuditCandidateV1,
    PalwAuditOutcomeV1, PalwAuditRowV1, PalwMeshErrorV1, PalwTrapCommittedV1, PalwTrapRevealedV1, PalwTrapRowV1,
    palw_mesh_audit_draw_v1, palw_mesh_audit_pay_v1, palw_mesh_audit_seed_v1, palw_mesh_trap_bounty_v1, palw_mesh_trap_penalty_v1,
    palw_trap_slot_drawn_v1, palw_trap_ticket_hits_v1,
};

fn refused(why: impl ToString) -> PalwStateV2Error {
    PalwStateV2Error::MeshRefused(why.to_string())
}

use crate::palw_audit_1004_v1::PALW_AUDIT_1004_MIN_TRAP_TILES_V1;

/// An unrevealed trap forfeits its deposit this long after its commitment: the audit window, the reveal window, and the same again
/// for the claim to have been produced at all.
pub const PALW_TRAP_COMMIT_TTL_DAA_V1: u64 = 2 * (PALW_AUDIT_WINDOW_DAA_V1 + PALW_TRAP_REVEAL_WINDOW_DAA_V1);

/// A bond's free collateral for a new reservation: the collateral net of everything the ledger already holds on it.
fn free_collateral_v1(state: &PalwChainStateV2, bond: &PalwBondKeyV2, record: &PalwBondStateV2) -> u128 {
    (record.collateral as u128).saturating_sub(state.reserved_exposure(bond)).saturating_sub(state.registration_exposure(bond))
}

impl TransitionBuilder<'_> {
    /// Add `amount` to a bond's `reserved_exposure` (the audit mesh's reservations).
    fn mesh_reserve(&mut self, bond: PalwBondKeyV2, amount: u128) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let next = self
            .state
            .reserved_exposure(&bond)
            .checked_add(amount)
            .ok_or(PalwStateV2Error::Overflow("mesh reservation"))?;
        self.write_exposure(bond, Some(next));
        Ok(())
    }

    /// Return `amount` of a bond's reservation.
    fn mesh_release(&mut self, bond: PalwBondKeyV2, amount: u128) -> Result<(), PalwStateV2Error> {
        if amount == 0 {
            return Ok(());
        }
        let next = self
            .state
            .reserved_exposure(&bond)
            .checked_sub(amount)
            .ok_or(PalwStateV2Error::Overflow("mesh reservation underflow"))?;
        self.write_exposure(bond, if next == 0 { None } else { Some(next) });
        Ok(())
    }

    /// Pay `amount` (clamped to the panel reserve) to `bond`'s payout payload; returns what was paid.
    fn mesh_pay_from_reserve(&mut self, bond: &PalwBondKeyV2, amount: u64) -> u64 {
        let paid = amount.min(self.state.panel_reserve_sompi());
        if paid == 0 {
            return 0;
        }
        let Some(payload) = self.state.bonds.get(bond).map(|record| record.payout_payload) else { return 0 };
        if self.add_panel_payout(payload, paid).is_err() {
            return 0;
        }
        let reserve = self.state.panel_reserve_sompi() - paid;
        self.write_panel_reserve(reserve);
        paid
    }
}

// ---------------------------------------------------------------------------------------------
// Part II: the witness profile, at a class's registration
// ---------------------------------------------------------------------------------------------

/// **A class has just been registered** past `palw_tir_v1`: past `palw_witness_manifest_v1` it pins its witness profile
/// ([`crate::palw_mesh_v1::palw_witness_profile_v1`]). Infallible: a class whose program serves nothing records none and keeps the
/// v1 manifest.
pub(super) fn record_witness_profile_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    class_id: &Hash64,
    program: &misaka_palw_tir::TirProgramV1,
    max_context: u32,
) {
    if !builder.params.witness_manifest_active_at(ctx.daa_score) {
        return;
    }
    if let Some(profile) = crate::palw_mesh_v1::palw_witness_profile_v1(program, max_context) {
        builder.write_mesh_witness(*class_id, Some(profile));
    }
}

// ---------------------------------------------------------------------------------------------
// Part IV.2: capped onboarding
// ---------------------------------------------------------------------------------------------

/// **A class-level `Held` attestation** (module doc of [`crate::palw_mesh_v1`], "Staged onboarding"): recorded under the class's id
/// while it is `Prefetching` and capped onboarding is armed, from an Active bond, once per `(seat, object, range)`, up to the
/// per-claim cap. Anything else is ignored.
pub(super) fn record_class_held_v1(
    builder: &mut TransitionBuilder<'_>,
    vertex: &crate::palw_vertex_v1::PalwVerificationVertexV1,
    class_id: &Hash64,
    object: u8,
    first: u32,
    last: u32,
    digest: Hash64,
) {
    if !builder.params.capped_active_at(vertex.signed_daa) {
        return;
    }
    let prefetching = builder
        .state
        .model_lifecycles
        .get(class_id)
        .is_some_and(|row| matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Prefetching));
    let active = builder.state.bonds.get(&vertex.seat_bond).is_some_and(|record| matches!(record.status, PalwBondStatusV2::Active));
    if !prefetching || !active {
        return;
    }
    let mut rows = builder.state.vertex.held.get(class_id).cloned().unwrap_or_default();
    if rows.len() >= crate::palw_vertex_v1::PALW_VERTEX_HELD_MAX_PER_CLAIM_V1
        || rows.iter().any(|row| row.seat == vertex.seat_bond && row.object == object && row.first == first && row.last == last)
    {
        return;
    }
    rows.push(crate::palw_vertex_v1::PalwVertexHeldRowV1 {
        seat: vertex.seat_bond,
        object,
        first,
        last,
        digest,
        signed_daa: vertex.signed_daa,
        charged: false,
    });
    builder.write_vertex_held(*class_id, Some(rows));
}

/// **Does a DA certificate stand for this class?** `q` distinct seats' equal `Held` leaves (the capture of its probe job).
pub fn palw_class_da_certificate_v1(state: &PalwChainStateV2, class_id: &Hash64) -> bool {
    let Some(rows) = state.vertex.held.get(class_id) else { return false };
    rows.iter().any(|anchor| {
        anchor.object == crate::palw_vertex_v1::PALW_VERTEX_HELD_OBJECT_CAPTURE_V1 && {
            let mut seats: Vec<PalwBondKeyV2> = rows
                .iter()
                .filter(|row| row.object == anchor.object && row.first == anchor.first && row.last == anchor.last && row.digest == anchor.digest)
                .map(|row| row.seat)
                .collect();
            seats.sort();
            seats.dedup();
            seats.len() >= usize::from(crate::palw_vertex_v1::PALW_VERTEX_DA_QUORUM_V1)
        }
    })
}

/// **May this `Prefetching` class step to `Capped`?** (the observation's `capped_entry`): capped onboarding in force, the class's ready
/// seats short of a panel, a DA certificate, and the capped classes' shares together with this class's within `w_cap`.
pub(super) fn capped_entry_v1(
    builder: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    class_id: &Hash64,
    row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1,
    ready_seats: u32,
    seat_count: u32,
) -> bool {
    if !builder.params.capped_active_at(ctx.daa_score)
        || !matches!(row.state, crate::palw_model_registry_v1::PalwModelLifecycleV1::Prefetching)
        || ready_seats >= seat_count
        || !palw_class_da_certificate_v1(&builder.state, class_id)
    {
        return false;
    }
    let capped_total: u32 = builder
        .state
        .model_lifecycles
        .iter()
        .filter(|(_, other)| crate::palw_mesh_v1::palw_lifecycle_is_capped_v1(&other.state))
        .map(|(id, _)| u32::from(builder.state.class_shares.get(id).copied().unwrap_or(0)))
        .sum();
    crate::palw_mesh_v1::palw_capped_weight_admits_v1(capped_total, u32::from(builder.state.class_shares.get(class_id).copied().unwrap_or(0)))
}

/// Whether a registry row is a `Capped` class's.
pub(super) fn palw_lifecycle_row_is_capped_v1(row: &crate::palw_model_registry_v1::PalwModelLifecycleRowV1) -> bool {
    crate::palw_mesh_v1::palw_lifecycle_is_capped_v1(&row.state)
}

/// **The admission gate of a `Capped` class** (past the lifecycle's own refusal): the capped classes' open weight may not exceed
/// `w_cap` of the immature weight — a claim is refused once they hold more than the cap allows (so the cap is exceeded by at most
/// the one claim that crossed it) — and a capped class's open claims are bounded by [`PALW_CAPPED_MAX_CLAIMS_V1`].
pub(super) fn capped_class_admits_v1(state: &PalwChainStateV2) -> Result<(), PalwStateV2Error> {
    if state.vertex.mesh.capped.len() >= crate::palw_mesh_v1::PALW_CAPPED_MAX_CLAIMS_V1 {
        return Err(refused("the capped claim table is full"));
    }
    let capped_open: u128 = state
        .vertex
        .mesh
        .capped
        .keys()
        .filter_map(|claim| state.claims.get(claim))
        .map(|claim| claim.immature_contribution)
        .fold(0u128, u128::saturating_add);
    let total = state.bounded_immature;
    if capped_open.saturating_mul(1_000) > total.saturating_mul(u128::from(crate::palw_mesh_v1::PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1)) && capped_open > 0 {
        return Err(refused(format!(
            "the capped classes hold {capped_open} of {total} immature weight, past w_cap of {} ‰",
            crate::palw_mesh_v1::PALW_CAPPED_WEIGHT_CAP_PERMILLE_V1
        )));
    }
    Ok(())
}

/// **A class has left `Capped`** (holders seated): every capped claim of the class still waiting gets its re-verification window —
/// the deadline [`crate::palw_state_v2::palw_provisional_bind_deadline_v1`] reads — and its deadline is re-armed. A claim that
/// binds a panel inside the window is the holders' to verify through the ordinary receipt path; one that does not is voided at the
/// window's end like any claim that never bound, its escrow never minted.
pub(super) fn capped_exit_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, class_id: &Hash64) {
    let window_end = ctx.daa_score.saturating_add(crate::palw_mesh_v1::PALW_CAPPED_REVERIFY_WINDOW_DAA_V1);
    let waiting: Vec<(Hash64, crate::palw_mesh_v1::PalwCappedClaimRowV1)> = builder
        .state
        .vertex
        .mesh
        .capped
        .iter()
        .filter(|(_, row)| row.class_id == *class_id && row.window_end_daa.is_none())
        .map(|(claim, row)| (*claim, row.clone()))
        .collect();
    for (claim, mut row) in waiting {
        row.window_end_daa = Some(window_end);
        builder.write_mesh_capped(claim, Some(row));
        let _ = builder.rearm_claim_deadline_dl1_v1(claim, ctx.daa_score);
    }
}

// ---------------------------------------------------------------------------------------------
// The draw, at a claim's acceptance
// ---------------------------------------------------------------------------------------------

/// **A claim has just been accepted**: past `palw_audit_mesh_v1`, an IR class's claim draws its audit row. Infallible by design (module
/// doc): whatever cannot be done is left undone.
pub(super) fn on_claim_accepted_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, claim_id: &Hash64) {
    if !builder.params.audit_mesh_active_at(ctx.daa_score) {
        return;
    }
    let Some(claim) = builder.state.claims.get(claim_id).cloned() else { return };
    // **RFC-0007 Part IV.2**: a claim of a `Capped` class is a capped claim — its row holds it out of the bind window's backstop
    // until the class's holders are seated (and then its re-verification window). The deadline is re-armed from the one function.
    if builder.params.capped_active_at(ctx.daa_score)
        && builder
            .state
            .model_lifecycles
            .get(&claim.class_id)
            .is_some_and(|row| crate::palw_mesh_v1::palw_lifecycle_is_capped_v1(&row.state))
        && builder.state.vertex.mesh.capped.len() < crate::palw_mesh_v1::PALW_CAPPED_MAX_CLAIMS_V1
    {
        builder.write_mesh_capped(
            *claim_id,
            Some(crate::palw_mesh_v1::PalwCappedClaimRowV1 {
                class_id: claim.class_id,
                accepted_daa: claim.accepted_daa,
                window_end_daa: None,
            }),
        );
        let _ = builder.rearm_claim_deadline_dl1_v1(*claim_id, ctx.daa_score);
    }
    if !matches!(claim.source, PalwClaimSourceV2::Attempt)
        || !builder.state.tir_classes.contains_key(&claim.class_id)
        || builder.state.vertex.mesh.audits.contains_key(claim_id)
        || builder.state.vertex.mesh.audits.len() >= PALW_AUDIT_MAX_OPEN_V1
    {
        return;
    }
    let pay = palw_mesh_audit_pay_v1(claim.escrowed_reward);
    if pay == 0 {
        return;
    }
    let penalty = palw_mesh_trap_penalty_v1(pay);
    let candidates: Vec<PalwAuditCandidateV1> = builder
        .state
        .bonds
        .iter()
        .filter(|(key, record)| {
            **key != claim.bond
                && matches!(record.status, PalwBondStatusV2::Active)
                && record.registered_daa < claim.accepted_daa
                && free_collateral_v1(&builder.state, key, record) >= penalty
        })
        .map(|(key, record)| PalwAuditCandidateV1 {
            bond: *key,
            operator_id: record.operator_id,
            weight_msk: crate::palw_panel_v2::palw_draw_operator_weight_msk_v1(
                u128::from(record.collateral) / u128::from(crate::constants::SOMPI_PER_KASPA),
                crate::palw_panel_v2::PALW_DRAW_WEIGHT_CAP_MSK_V1,
            ),
        })
        .collect();
    // **Lane PA, P-F5**: past the fence the seed reads the beacon, not the carrying block's own (grindable) hash.
    let seed = match builder.extras.audit_1004_draw_seed_source.filter(|_| builder.params.audit_1004_active_at(ctx.daa_score)) {
        Some(beacon) => crate::palw_mesh_v1::palw_mesh_audit_seed_beacon_v1(claim_id, &beacon, &claim.execution_root, ctx.daa_score),
        None => palw_mesh_audit_seed_v1(claim_id, &claim.accepted_block, &claim.execution_root, ctx.daa_score),
    };
    let drawn = palw_mesh_audit_draw_v1(&seed, &candidates, PALW_AUDITS_PER_CLAIM_V1);
    if drawn.is_empty() {
        return;
    }
    let mut assignments = Vec::with_capacity(drawn.len());
    for (auditor, ticket) in &drawn {
        if builder.mesh_reserve(*auditor, penalty).is_err() {
            // Undo what this draw reserved: no row, no reservation.
            for done in &assignments {
                let done: &PalwAuditAssignmentV1 = done;
                let _ = builder.mesh_release(done.auditor, done.reserved);
            }
            return;
        }
        assignments.push(PalwAuditAssignmentV1 { auditor: *auditor, ticket: *ticket, reserved: penalty, outcome: None });
    }
    let audit_end_daa = ctx.daa_score.saturating_add(PALW_AUDIT_WINDOW_DAA_V1);
    builder.write_mesh_audit(
        *claim_id,
        Some(PalwAuditRowV1 {
            drawn_daa: ctx.daa_score,
            audit_end_daa,
            row_end_daa: audit_end_daa.saturating_add(PALW_TRAP_REVEAL_WINDOW_DAA_V1),
            pay,
            penalty,
            assignments,
            trap_settled: false,
        }),
    );
}

// ---------------------------------------------------------------------------------------------
// Audited leaves
// ---------------------------------------------------------------------------------------------

/// **The audit rows a list of leaves names, resolved once.** A `Full` reference is the claim id when the claim has an audit row; a
/// `Compact` one names the DAA the audit was *drawn* at (the claim's acceptance) and a 16-byte prefix — the one audited claim
/// drawn at that DAA with that prefix, `None` when none or more than one is. An ambiguous or unknown reference is ignored.
pub(super) struct AuditRefIndexV1 {
    by_drawn: BTreeMap<u64, Vec<Hash64>>,
}

impl AuditRefIndexV1 {
    pub(super) fn of(state: &PalwChainStateV2, leaves: &[crate::palw_vertex_v1::PalwVertexLeafV1]) -> Self {
        let wanted: BTreeSet<u64> = leaves
            .iter()
            .filter_map(|leaf| match leaf {
                crate::palw_vertex_v1::PalwVertexLeafV1::Audited { claim: crate::palw_vertex_v1::PalwClaimRefV1::Compact { bound_daa, .. }, .. } => {
                    Some(u64::from(*bound_daa))
                }
                _ => None,
            })
            .collect();
        let mut by_drawn: BTreeMap<u64, Vec<Hash64>> = BTreeMap::new();
        if !wanted.is_empty() {
            for (claim, row) in &state.vertex.mesh.audits {
                if wanted.contains(&row.drawn_daa) {
                    by_drawn.entry(row.drawn_daa).or_default().push(*claim);
                }
            }
        }
        Self { by_drawn }
    }

    pub(super) fn resolve(&self, state: &PalwChainStateV2, claim: &crate::palw_vertex_v1::PalwClaimRefV1) -> Option<Hash64> {
        match claim {
            crate::palw_vertex_v1::PalwClaimRefV1::Full(id) => state.vertex.mesh.audits.contains_key(id).then_some(*id),
            crate::palw_vertex_v1::PalwClaimRefV1::Compact { bound_daa, .. } => {
                let mut named = self.by_drawn.get(&u64::from(*bound_daa)).into_iter().flatten().filter(|id| claim.names(id, u64::from(*bound_daa)));
                let first = named.next()?;
                named.next().is_none().then_some(*first)
            }
        }
    }
}

/// **An `Audited` leaf** of an accepted vertex: recorded and paid when its seat is a drawn auditor of the claim, names the drawn
/// ticket, was signed inside the audit window, and is the seat's first answer. Anything else is ignored: a leaf that does not count
/// is never a refusal.
pub(super) fn count_audited_leaf_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    seat: &PalwBondKeyV2,
    signed_daa: u64,
    claim_id: &Hash64,
    ticket: u64,
    result: u8,
) -> Result<(), PalwStateV2Error> {
    let Some(mut row) = builder.state.vertex.mesh.audits.get(claim_id).cloned() else { return Ok(()) };
    if signed_daa < row.drawn_daa || signed_daa > row.audit_end_daa || ctx.daa_score > row.audit_end_daa {
        return Ok(());
    }
    let Some(assignment) = row.assignments.iter_mut().find(|a| a.auditor == *seat) else { return Ok(()) };
    if assignment.outcome.is_some() || assignment.ticket != ticket {
        return Ok(());
    }
    let matched = match result {
        0 => true,
        1 => false,
        other => return Err(refused(PalwMeshErrorV1::AuditResultUnknown(other))),
    };
    // **Lane PA, C-F4 (`palw_audit_1004_v1`): an answer is paid only once it has stood** — at the row's sweep, unless a trap
    // revealed on the claim caught it (below), so a lazy "match" on a trap is never paid. A row a trap already settled takes no answer.
    let deferred = builder.params.audit_1004_active_at(ctx.daa_score);
    if deferred && row.trap_settled {
        return Ok(());
    }
    assignment.outcome = Some(PalwAuditOutcomeV1 { matched, signed_daa });
    let pay = row.pay;
    builder.write_mesh_audit(*claim_id, Some(row));
    if !deferred {
        builder.mesh_pay_from_reserve(seat, pay);
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Traps
// ---------------------------------------------------------------------------------------------

impl PalwChainStateV2 {
    /// **The audits `seat` still owes at `daa_score`**: drawn on a live claim of a registered class, not yet answered, inside the
    /// window. Read-only, for the node's duty loop.
    pub fn mesh_audit_duties_v1(&self, seat: &PalwBondKeyV2, daa_score: u64) -> Vec<crate::palw_mesh_v1::PalwMeshAuditDutyV1> {
        self.vertex
            .mesh
            .audits
            .iter()
            .filter_map(|(claim_id, row)| {
                let assignment = row.assignment_of(seat)?;
                if assignment.outcome.is_some() || daa_score > row.audit_end_daa {
                    return None;
                }
                let claim = self.claims.get(claim_id)?;
                let artifact_root = self.classes.get(&claim.class_id)?.artifact_root;
                Some(crate::palw_mesh_v1::PalwMeshAuditDutyV1 {
                    claim_id: *claim_id,
                    class_id: claim.class_id,
                    artifact_root,
                    executor_bond: claim.bond,
                    accepted_block: claim.accepted_block,
                    trace_root: claim.trace_root,
                    execution_root: claim.execution_root,
                    drawn_daa: row.drawn_daa,
                    audit_end_daa: row.audit_end_daa,
                    ticket: assignment.ticket,
                })
            })
            .collect()
    }

    /// The `(audit_end, row_end)` window of a claim's audit row, if it still stands (a trap setter reads it to time its reveal).
    pub fn mesh_audit_window_v1(&self, claim: &Hash64) -> Option<(u64, u64)> {
        self.vertex.mesh.audits.get(claim).map(|row| (row.audit_end_daa, row.row_end_daa))
    }
}

impl PalwChainStateV2 {
    /// **The stateful half of a `TrapCommitted`'s admission**, shared by the acceptance walk and the fold: the setter is registered and
    /// Active, drawn by the slot lottery, has no trap open, has the deposit free, the commitment is new and the table has room. Pure
    /// over the state. (The ML-DSA signature is the walk's.)
    pub fn mesh_trap_committed_admissible_v1(&self, trap: &PalwTrapCommittedV1, daa_score: u64) -> Result<(), PalwMeshErrorV1> {
        let setter = trap.setter_bond;
        let record = self.bonds.get(&setter).ok_or(PalwMeshErrorV1::UnknownBond(setter))?;
        if !matches!(record.status, PalwBondStatusV2::Active) {
            return Err(PalwMeshErrorV1::BondNotActive(setter));
        }
        if self.vertex.mesh.traps.contains_key(&trap.commitment) {
            return Err(PalwMeshErrorV1::TrapCommitmentKnown);
        }
        if self.vertex.mesh.traps.values().any(|row| row.setter == setter) {
            return Err(PalwMeshErrorV1::TrapAlreadyOpen(setter));
        }
        if self.vertex.mesh.traps.len() >= PALW_TRAP_MAX_OPEN_V1 {
            return Err(PalwMeshErrorV1::TrapTableFull);
        }
        if !palw_trap_slot_drawn_v1(&setter, daa_score) {
            return Err(PalwMeshErrorV1::TrapSlotNotDrawn(setter));
        }
        let free = free_collateral_v1(self, &setter, record);
        if free < PALW_TRAP_DEPOSIT_SOMPI_V1 {
            return Err(PalwMeshErrorV1::TrapDepositUnaffordable { bond: setter, free });
        }
        Ok(())
    }
}

/// **`TrapCommitted`**: admitted ([`PalwChainStateV2::mesh_trap_committed_admissible_v1`]); the deposit is reserved on the bond.
pub(super) fn apply_trap_committed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    trap: &PalwTrapCommittedV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.audit_mesh_active_at(ctx.daa_score) {
        return Err(refused(PalwMeshErrorV1::Dormant("palw_audit_mesh_v1")));
    }
    builder.state.mesh_trap_committed_admissible_v1(trap, ctx.daa_score).map_err(refused)?;
    let setter = trap.setter_bond;
    builder.mesh_reserve(setter, PALW_TRAP_DEPOSIT_SOMPI_V1)?;
    builder.write_mesh_trap(
        trap.commitment,
        Some(PalwTrapRowV1 { setter, committed_daa: ctx.daa_score, deposit: PALW_TRAP_DEPOSIT_SOMPI_V1, revealed_claim: None }),
    );
    Ok(())
}

/// **The stateful half of a `TrapRevealed`'s admission**, shared by the acceptance walk and the fold: the reveal opens a commitment
/// of this setter, over a tile count and tile the commitment bound, about the setter's own audited claim, after the audit window
/// and inside the reveal window. Pure over the state.
impl PalwChainStateV2 {
    pub fn mesh_trap_revealed_admissible_v1(&self, reveal: &PalwTrapRevealedV1, daa_score: u64) -> Result<(), PalwMeshErrorV1> {
        self.mesh_trap_revealed_admissible_v2(reveal, daa_score, false)
    }

    /// [`Self::mesh_trap_revealed_admissible_v1`] with lane PA's rule (`palw_audit_1004_v1`, **P-F4**) when `audit_1004`: a single-tile
    /// trap is refused (every ticket lands on the planted tile) and the commitment must precede the audit draw it is revealed
    /// against — a trap committed after the draw can be sized to the draw, slashing honest auditors.
    pub fn mesh_trap_revealed_admissible_v2(
        &self,
        reveal: &PalwTrapRevealedV1,
        daa_score: u64,
        audit_1004: bool,
    ) -> Result<(), PalwMeshErrorV1> {
        let state = self;
        if audit_1004 && reveal.tiles < PALW_AUDIT_1004_MIN_TRAP_TILES_V1 {
            return Err(PalwMeshErrorV1::TrapTilesInvalid { tiles: reveal.tiles, fault_leaf: reveal.fault_leaf });
        }
    if reveal.tiles == 0 || reveal.tiles > PALW_TRAP_MAX_TILES_V1 || reveal.fault_leaf >= reveal.tiles {
        return Err(PalwMeshErrorV1::TrapTilesInvalid { tiles: reveal.tiles, fault_leaf: reveal.fault_leaf });
    }
    let row = state.vertex.mesh.traps.get(&reveal.commitment()).ok_or(PalwMeshErrorV1::TrapNotCommitted)?;
    if row.setter != reveal.setter_bond {
        return Err(PalwMeshErrorV1::TrapSetterDiffers { got: reveal.setter_bond, committed: row.setter });
    }
    if row.revealed_claim.is_some() {
        return Err(PalwMeshErrorV1::TrapAlreadyRevealed);
    }
    let audit = state.vertex.mesh.audits.get(&reveal.claim).ok_or(PalwMeshErrorV1::TrapClaimNotAudited(reveal.claim))?;
    if audit_1004 && row.committed_daa >= audit.drawn_daa {
        return Err(PalwMeshErrorV1::TrapCommittedAfterTheDraw { committed: row.committed_daa, drawn: audit.drawn_daa });
    }
    let claim = state.claims.get(&reveal.claim).ok_or(PalwMeshErrorV1::TrapClaimNotAudited(reveal.claim))?;
    if claim.bond != reveal.setter_bond {
        return Err(PalwMeshErrorV1::TrapClaimNotTheSetters(reveal.claim));
    }
    if daa_score < audit.audit_end_daa {
        return Err(PalwMeshErrorV1::TrapRevealedEarly { at: daa_score, audit_end: audit.audit_end_daa });
    }
    if daa_score > audit.row_end_daa {
        return Err(PalwMeshErrorV1::TrapRevealedLate { at: daa_score, row_end: audit.row_end_daa });
    }
    Ok(())
}
}

/// **`TrapRevealed`** (module doc): the deposit is released and the row dropped; every auditor whose ticket landed on the planted tile
/// and who attested a match is slashed the penalty, one who attested the mismatch is paid the bounty; the trap claim is voided
/// **without slashing its setter** (a claim already terminal stays as it is); the audit row is marked settled so a second reveal of
/// the same claim changes nothing.
pub(super) fn apply_trap_revealed_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    reveal: &PalwTrapRevealedV1,
) -> Result<(), PalwStateV2Error> {
    if !builder.params.audit_mesh_active_at(ctx.daa_score) {
        return Err(refused(PalwMeshErrorV1::Dormant("palw_audit_mesh_v1")));
    }
    builder.state.mesh_trap_revealed_admissible_v2(reveal, ctx.daa_score, builder.params.audit_1004_active_at(ctx.daa_score)).map_err(refused)?;
    let commitment = reveal.commitment();
    let trap = builder.state.vertex.mesh.traps.get(&commitment).cloned().ok_or_else(|| refused(PalwMeshErrorV1::TrapNotCommitted))?;
    let mut audit = builder.state.vertex.mesh.audits.get(&reveal.claim).cloned().ok_or_else(|| refused(PalwMeshErrorV1::TrapClaimNotAudited(reveal.claim)))?;

    // 1. The deposit comes back, and the commitment is spent.
    builder.mesh_release(trap.setter, trap.deposit)?;
    builder.write_mesh_trap(commitment, None);

    // 2. The auditors the planted tile caught (once per claim).
    if !audit.trap_settled {
        let bounty = palw_mesh_trap_bounty_v1(audit.pay);
        let deferred = builder.params.audit_1004_active_at(ctx.daa_score);
        for assignment in audit.assignments.iter_mut() {
            let (Some(outcome), true) =
                (assignment.outcome, palw_trap_ticket_hits_v1(assignment.ticket, reveal.fault_leaf, reveal.tiles))
            else {
                continue;
            };
            if outcome.matched {
                builder.slash_bond(assignment.auditor, audit.penalty)?;
                // C-F4: the slashed answer was wrong — it earns no audit pay at the sweep.
                if deferred {
                    assignment.outcome = None;
                }
            } else {
                builder.mesh_pay_from_reserve(&assignment.auditor, bounty);
            }
        }
        audit.trap_settled = true;
        builder.write_mesh_audit(reveal.claim, Some(audit));
    }

    // 3. The trap claim: voided, its setter not slashed. **Lane PA, P-F1 (`palw_audit_1004_v1`): never a claim with a dispute open** — a
    // court session or a DA session on it is its challenger's to win (a void closes the court NEUTRALLY, so a fraudulent executor
    // could reveal a trap on its own claim after a challenger opened a court and walk away unconvicted).
    let disputed = builder.params.audit_1004_active_at(ctx.daa_score)
        && (builder.state.open_courts_of(&reveal.claim) > 0 || builder.state.da_sessions_of(&reveal.claim).next().is_some());
    if !disputed
        && let Some(claim) = builder.state.claims.get(&reveal.claim).cloned()
        && !claim.phase.is_terminal()
    {
        builder.void_claim(reveal.claim, &claim, ctx.daa_score, PalwVoidReasonV2::ReceiptTimeout)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// The sweep
// ---------------------------------------------------------------------------------------------

/// **The mesh's sweep** (before a block's objects, mirrored in `palw_v2_pre_object_base_v1`): audit rows past their reveal window
/// release their reservations and leave; a trap not revealed within [`PALW_TRAP_COMMIT_TTL_DAA_V1`] forfeits its deposit. At most
/// [`PALW_MESH_SWEEP_PER_BLOCK_V1`] rows of each table a block, in key order. A no-op on a chain with no row.
pub(super) fn sweep_mesh_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    if builder.state.vertex.mesh.audits.is_empty() && builder.state.vertex.mesh.traps.is_empty() {
        return;
    }
    let deferred = builder.params.audit_1004_active_at(ctx.daa_score);
    let dead_audits: Vec<Hash64> = builder
        .state
        .vertex
        .mesh
        .audits
        .iter()
        .filter(|(_, row)| row.row_end_daa < ctx.daa_score)
        .map(|(claim, _)| *claim)
        .take(PALW_MESH_SWEEP_PER_BLOCK_V1)
        .collect();
    for claim in dead_audits {
        if let Some(row) = builder.state.vertex.mesh.audits.get(&claim).cloned() {
            for assignment in &row.assignments {
                // **C-F4 (`palw_audit_1004_v1`): the audit pay of an answer that stood**, paid once, from the reserve (clamped).
                if deferred && assignment.outcome.is_some() {
                    builder.mesh_pay_from_reserve(&assignment.auditor, row.pay);
                }
                let _ = builder.mesh_release(assignment.auditor, assignment.reserved);
            }
        }
        builder.write_mesh_audit(claim, None);
    }
    let dead_traps: Vec<Hash64> = builder
        .state
        .vertex
        .mesh
        .traps
        .iter()
        .filter(|(_, row)| row.committed_daa.saturating_add(PALW_TRAP_COMMIT_TTL_DAA_V1) < ctx.daa_score)
        .map(|(commitment, _)| *commitment)
        .take(PALW_MESH_SWEEP_PER_BLOCK_V1)
        .collect();
    for commitment in dead_traps {
        if let Some(row) = builder.state.vertex.mesh.traps.get(&commitment).cloned() {
            // A trap never revealed forfeits its deposit: slashed, then the reservation returned.
            let _ = builder.slash_bond(row.setter, row.deposit);
            let _ = builder.mesh_release(row.setter, row.deposit);
        }
        builder.write_mesh_trap(commitment, None);
    }
}

impl PalwChainStateV2 {
    /// **The mesh tables' own consistency**: a witness profile belongs to a registered class; an audit row holds at most the drawn
    /// number of distinct auditors of registered bonds, each reserving at most the row's penalty, in windows the constants derive;
    /// a trap belongs to a registered setter and holds at most the deposit.
    pub(crate) fn assert_mesh_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        let bad = |why: String| Err(PalwStateV2Error::CarriageInconsistent(format!("mesh tables: {why}")));
        for class_id in self.vertex.mesh.witness.keys() {
            if !self.classes.contains_key(class_id) {
                return bad(format!("a witness profile of class {class_id}, which is not registered"));
            }
        }
        for (claim_id, row) in &self.vertex.mesh.audits {
            if row.assignments.is_empty() || row.assignments.len() > PALW_AUDITS_PER_CLAIM_V1 {
                return bad(format!("the audit row of claim {claim_id} holds {} assignments", row.assignments.len()));
            }
            if row.audit_end_daa != row.drawn_daa.saturating_add(PALW_AUDIT_WINDOW_DAA_V1)
                || row.row_end_daa != row.audit_end_daa.saturating_add(PALW_TRAP_REVEAL_WINDOW_DAA_V1)
            {
                return bad(format!("the audit row of claim {claim_id} has windows its draw would not have written"));
            }
            for (i, assignment) in row.assignments.iter().enumerate() {
                if !self.bonds.contains_key(&assignment.auditor) {
                    return bad(format!("an auditor of claim {claim_id} is not a registered bond"));
                }
                if assignment.reserved > row.penalty {
                    return bad(format!("an auditor of claim {claim_id} reserves more than the penalty"));
                }
                if row.assignments[..i].iter().any(|other| other.auditor == assignment.auditor) {
                    return bad(format!("claim {claim_id} audits one seat twice"));
                }
            }
        }
        for (claim_id, row) in &self.vertex.mesh.capped {
            let Some(claim) = self.claims.get(claim_id) else { return bad(format!("a capped row of claim {claim_id}, which does not exist")) };
            if !matches!(claim.phase, PalwClaimPhaseV2::Provisional) {
                return bad(format!("a capped row of claim {claim_id}, which is no longer Provisional"));
            }
            if !self.model_lifecycles.contains_key(&row.class_id) || claim.class_id != row.class_id {
                return bad(format!("a capped row of claim {claim_id} names a class that is not the claim's, or has no registry row"));
            }
        }
        for (commitment, row) in &self.vertex.mesh.traps {
            if !self.bonds.contains_key(&row.setter) {
                return bad(format!("trap {commitment} belongs to a setter that is not a registered bond"));
            }
            if row.deposit > PALW_TRAP_DEPOSIT_SOMPI_V1 {
                return bad(format!("trap {commitment} holds more than the deposit"));
            }
        }
        Ok(())
    }
}

/// **Lane PA, C-F4 (`palw_audit_1004_v1`): re-snapshot a lead claim's audit pay at a riders split.** The lead's escrow is split `1 + n`
/// ways and each rider draws its own audit at its share; the lead's row still carried 4 ‰ of the whole, so the batch funded
/// `pay(E) + Σ pay(E/(1+n))` — more than one claim's worth. The row's `pay` follows the escrow the lead keeps (its reservation, the
/// penalty, stays: auditors reserved it at the draw). A lead with no row changes nothing.
pub(super) fn resnapshot_audit_pay_v1(builder: &mut TransitionBuilder<'_>, lead_id: &Hash64, kept_escrow: u64) {
    let Some(mut row) = builder.state.vertex.mesh.audits.get(lead_id).cloned() else { return };
    let pay = palw_mesh_audit_pay_v1(kept_escrow);
    if pay == row.pay {
        return;
    }
    row.pay = pay;
    builder.write_mesh_audit(*lead_id, Some(row));
}
