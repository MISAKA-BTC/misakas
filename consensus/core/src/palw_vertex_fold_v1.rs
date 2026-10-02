//! **RFC-0007 Part I in the fold: the verification vertex, licence by tally, equivocation, `Held` leaves** — dormant behind
//! `Params::palw_verification_vertex_v1`. A child module of `palw_state_v2`, as the held leaf challenge's fold is, so it reads the
//! builder and the state's tables directly and writes them only through their one writers. The objects, their hashing and the
//! stateless checks are [`crate::palw_vertex_v1`]'s; spec 18 is normative.
//!
//! # Licence by tally
//!
//! A `Verdict` leaf counts when [`palw_vertex_leaf_fate_v1`] says so (the conditions `validate_receipt_coverage_v2` applies to a
//! receipt, plus the path rule: a claim whose panel bound before the fence licenses on the receipt path alone). A counted leaf joins
//! its claim's tally; when the counted `Valid`s reach the quorum the tally is expanded into the receipts a `ReceiptLicensedV2` of the
//! same seats would carry and **fed to the same arm** ([`apply_receipt_licensed_v2`]) — no licensing rule is restated, and the licence
//! the arm writes is the one a carried licence writes. A tally the arm declines (the set is not backed, the door does not hold) stays,
//! and the next counted leaf tries again. The arm's own refusals that a carried licence meets at acceptance — the outsider's `Valid`
//! — are asked here first, so a tally never makes a block invalid.
//!
//! A claim already licensed takes a seat's `Valid` or `Sampled` leaf as the supplementary receipt it is.

use super::*;
use crate::palw_vertex_v1::{
    PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1, PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1, PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1,
    PALW_VERTEX_HELD_MAX_PER_CLAIM_V1, PALW_VERTEX_ROUND_DAA_V1, PALW_VERTEX_SWEEP_PER_BLOCK_V1, PalwClaimRefV1, PalwVertexCountedV1,
    PalwVertexEquivocationV1, PalwVertexErrorV1, PalwVertexHeldRowV1, PalwVertexLeafFateV1, PalwVertexLeafV1, PalwVertexRoundRowV1,
    PalwVertexTallyV1, PalwVerificationVertexV1, palw_vertex_admissible_v1, palw_vertex_equivocation_admissible_v1,
    palw_vertex_leaf_fate_v1, palw_vertex_permille_of_v1, palw_vertex_receipts_of_v1, palw_vertex_resolve_claim_v1,
};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::VertexRefused(why.into())
}

/// The panel quorum the tally waits for before it asks the licensing arm: the colluding-quorum constant the arm's own door uses.
fn quorum() -> usize {
    crate::palw_offence_v1::PALW_PANEL_COLLUDING_QUORUM_V1 as usize
}

/// **The claims a list of leaves names, resolved once**: a `Full` reference is the id; a `Compact` one is looked up in an index of
/// the panels bound at the DAAs the list names, built by one walk of the panels (and only when a compact leaf is present).
struct ResolvedLeavesV1 {
    by_bound: BTreeMap<u64, Vec<Hash64>>,
}

impl ResolvedLeavesV1 {
    fn of(state: &PalwChainStateV2, leaves: &[PalwVertexLeafV1]) -> Self {
        let wanted: BTreeSet<u64> = leaves
            .iter()
            .filter_map(|leaf| match leaf.claim_ref() {
                PalwClaimRefV1::Compact { bound_daa, .. } => Some(u64::from(*bound_daa)),
                PalwClaimRefV1::Full(_) => None,
            })
            .collect();
        let mut by_bound: BTreeMap<u64, Vec<Hash64>> = BTreeMap::new();
        if !wanted.is_empty() {
            for (claim, panel) in &state.panels {
                if wanted.contains(&panel.bound_daa) {
                    by_bound.entry(panel.bound_daa).or_default().push(*claim);
                }
            }
        }
        Self { by_bound }
    }

    fn resolve(&self, state: &PalwChainStateV2, claim: &PalwClaimRefV1) -> Option<Hash64> {
        palw_vertex_resolve_claim_v1(state, claim, |at| self.by_bound.get(&at).cloned().unwrap_or_default())
    }
}

/// **A verification vertex** (see the module doc): admitted by [`palw_vertex_admissible_v1`] (the second lock behind the acceptance
/// walk's drop), its `(round, seat)` row written, then every leaf in order.
pub(super) fn apply_vertex_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    vertex: &PalwVerificationVertexV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.vertex_active_at(daa) {
        return Err(refused(PalwVertexErrorV1::Dormant.to_string()));
    }
    // Every refusal comes before the first write.
    palw_vertex_admissible_v1(&builder.state, vertex, daa).map_err(|e| refused(e.to_string()))?;
    builder.write_vertex_round(
        (vertex.round, vertex.seat_bond),
        Some(PalwVertexRoundRowV1 { leaves_root: vertex.leaves_root, accepted_daa: daa, convicted: false }),
    );
    let resolved = ResolvedLeavesV1::of(&builder.state, &vertex.leaves);
    for leaf in &vertex.leaves {
        match leaf {
            PalwVertexLeafV1::Verdict { claim, verdict } => {
                let Some(claim_id) = resolved.resolve(&builder.state, claim) else { continue };
                count_verdict_v1(builder, ctx, vertex, &claim_id, verdict)?;
            }
            PalwVertexLeafV1::Held { claim, object, first, last, digest } => {
                let Some(claim_id) = resolved.resolve(&builder.state, claim) else { continue };
                record_held_v1(builder, vertex, &claim_id, *object, *first, *last, *digest);
            }
            // Refused by the shape check above; named, never `unreachable!`.
            PalwVertexLeafV1::Audited { .. } => return Err(refused("an audit leaf: the audit mesh is not armed by this fence")),
        }
    }
    Ok(())
}

fn count_verdict_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    vertex: &PalwVerificationVertexV1,
    claim_id: &Hash64,
    verdict: &crate::palw_panel_v2::PalwReceiptVerdictV2,
) -> Result<(), PalwStateV2Error> {
    let seat = vertex.seat_bond;
    match palw_vertex_leaf_fate_v1(&builder.state, builder.params, ctx.daa_score, &seat, vertex.signed_daa, claim_id, verdict) {
        PalwVertexLeafFateV1::Ignored(_) => Ok(()),
        PalwVertexLeafFateV1::Counted => {
            let mut tally = builder.state.vertex.tallies.get(claim_id).cloned().unwrap_or_default();
            tally.counted.push(PalwVertexCountedV1 { seat, verdict: *verdict, signed_daa: vertex.signed_daa });
            let reached = tally.valid() >= quorum();
            builder.write_vertex_tally(*claim_id, Some(tally.clone()));
            if reached {
                license_from_tally_v1(builder, ctx, claim_id, &tally)?;
            }
            Ok(())
        }
        PalwVertexLeafFateV1::Supplementary => {
            let counted = [PalwVertexCountedV1 { seat, verdict: *verdict, signed_daa: vertex.signed_daa }];
            let Some(receipts) = palw_vertex_receipts_of_v1(&builder.state, claim_id, &counted) else { return Ok(()) };
            apply_receipt_licensed_v2(builder, ctx, claim_id, &receipts)
        }
    }
}

/// **The licence by tally**: the counted leaves as the receipts they stand for, through the licensing arm — after the one refusal a
/// carried licence meets at acceptance and the arm would otherwise raise as a fold error (an outsider-judged claim whose outsider has
/// not answered `Valid`).
fn license_from_tally_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: &Hash64,
    tally: &PalwVertexTallyV1,
) -> Result<(), PalwStateV2Error> {
    let Some(claim) = builder.state.claims.get(claim_id).cloned() else { return Ok(()) };
    let Some(receipts) = palw_vertex_receipts_of_v1(&builder.state, claim_id, &tally.counted) else { return Ok(()) };
    let inner: Vec<crate::palw_panel_v2::PalwSeatReceiptV2> = receipts.iter().map(|r| r.receipt.clone()).collect();
    let verdicts = palw_seat_verdicts_of_v2(&inner);
    if palw_licence_names_its_outsider_v1(&builder.state, claim_id, &claim, &verdicts, builder.extras.admission_independence_daa).is_err() {
        return Ok(());
    }
    apply_receipt_licensed_v2(builder, ctx, claim_id, &receipts)
}

/// A `Held` leaf: recorded once per `(seat, object, range)` on a live claim whose panel the seat sits on, up to the per-claim cap. A
/// second attestation of the same range with another digest is ignored: the first stands.
fn record_held_v1(
    builder: &mut TransitionBuilder<'_>,
    vertex: &PalwVerificationVertexV1,
    claim_id: &Hash64,
    object: u8,
    first: u32,
    last: u32,
    digest: Hash64,
) {
    let (Some(claim), Some(panel)) = (builder.state.claims.get(claim_id), builder.state.panels.get(claim_id)) else { return };
    if claim.phase.is_terminal() || !panel.seats.iter().any(|seat| seat.bond == vertex.seat_bond) {
        return;
    }
    let mut rows = builder.state.vertex.held.get(claim_id).cloned().unwrap_or_default();
    if rows.len() >= PALW_VERTEX_HELD_MAX_PER_CLAIM_V1
        || rows.iter().any(|row| row.seat == vertex.seat_bond && row.object == object && row.first == first && row.last == last)
    {
        return;
    }
    rows.push(PalwVertexHeldRowV1 { seat: vertex.seat_bond, object, first, last, digest, signed_daa: vertex.signed_daa, charged: false });
    builder.write_vertex_held(*claim_id, Some(rows));
}

/// **An equivocation** (RFC-0007 §I.6): the bond slashed [`PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1`] ‰, every lock it holds on a
/// claim either vertex names forfeited, and the bond ejected (a forced retirement: it backs its existing claims to resolution and
/// takes no new ones). The `(round, seat)` row records the conviction, so the same pair is convicted once.
pub(super) fn apply_vertex_equivocation_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    evidence: &PalwVertexEquivocationV1,
) -> Result<(), PalwStateV2Error> {
    let daa = ctx.daa_score;
    if !builder.params.vertex_active_at(daa) {
        return Err(refused(PalwVertexErrorV1::Dormant.to_string()));
    }
    palw_vertex_equivocation_admissible_v1(&builder.state, evidence, daa).map_err(|e| refused(e.to_string()))?;
    let (seat, round) = (evidence.a.seat_bond, evidence.a.round);
    let record = builder.state.bonds.get(&seat).ok_or(PalwStateV2Error::MissingBond(seat))?.clone();

    // The claims either side names, resolved on the state as it stands.
    let all: Vec<PalwVertexLeafV1> = evidence.a_leaves.iter().chain(evidence.b_leaves.iter()).copied().collect();
    let resolved = ResolvedLeavesV1::of(&builder.state, &all);
    let claims: BTreeSet<Hash64> = all.iter().filter_map(|leaf| resolved.resolve(&builder.state, leaf.claim_ref())).collect();

    // 1. The locks, each forfeited into the slash.
    let mut total = palw_vertex_permille_of_v1(u128::from(record.collateral), PALW_VERTEX_EQUIVOCATION_PENALTY_PERMILLE_V1)
        .ok_or(PalwStateV2Error::Overflow("equivocation penalty"))?;
    for claim in claims {
        if let Some(lock) = builder.state.slashable_locks.get(&(seat, claim)).copied() {
            total = total.checked_add(lock.amount).ok_or(PalwStateV2Error::Overflow("equivocation forfeited locks"))?;
            builder.write_slashable_lock((seat, claim), None);
        }
    }
    // 2. The slash (clamped at the bond's collateral, as every slash is).
    builder.slash_bond(seat, total)?;
    // 3. The ejection.
    let record = builder.state.bonds.get(&seat).ok_or(PalwStateV2Error::MissingBond(seat))?.clone();
    if matches!(record.status, PalwBondStatusV2::Active) {
        let status = PalwBondStatusV2::Retiring { since_daa: daa, settled_at_since: builder.state.settled_attempt_finals };
        builder.write_bond(seat, Some(PalwBondStateV2 { status, ..record }));
    }
    // 4. The conviction: one per `(seat, round)`.
    let (accepted_daa, leaves_root) = match builder.state.vertex.rounds.get(&(round, seat)) {
        Some(row) => (row.accepted_daa, row.leaves_root),
        None => (daa, evidence.a.leaves_root.min(evidence.b.leaves_root)),
    };
    builder.write_vertex_round((round, seat), Some(PalwVertexRoundRowV1 { leaves_root, accepted_daa, convicted: true }));
    Ok(())
}

/// **The sweep: the `(round, seat)` rows past their evidence window**, the oldest rounds first, at most
/// [`PALW_VERTEX_SWEEP_PER_BLOCK_V1`] a block. A row is needed while a vertex of its round could still land
/// ([`crate::palw_vertex_v1::PALW_VERTEX_MAX_CARRY_DAA_V1`]) and while an equivocation of it is still admissible
/// ([`PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1`]); the second is the longer. A no-op on a chain with no row.
pub(super) fn sweep_vertex_rounds_v1(builder: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) {
    if builder.state.vertex.rounds.is_empty() {
        return;
    }
    let dead = |round: u64| {
        round.saturating_add(1).saturating_mul(PALW_VERTEX_ROUND_DAA_V1).saturating_add(PALW_VERTEX_EVIDENCE_WINDOW_DAA_V1) < ctx.daa_score
    };
    let doomed: Vec<(u64, PalwBondKeyV2)> =
        builder.state.vertex.rounds.keys().take_while(|(round, _)| dead(*round)).take(PALW_VERTEX_SWEEP_PER_BLOCK_V1).copied().collect();
    for key in doomed {
        builder.write_vertex_round(key, None);
    }
}

/// **The attesters of a claim not yet charged**, read before a data-availability default voids the claim (the void drops the rows).
pub(super) fn held_attesters_to_charge_v1(state: &PalwChainStateV2, claim_id: &Hash64) -> Vec<PalwBondKeyV2> {
    let mut seats: Vec<PalwBondKeyV2> =
        state.vertex.held.get(claim_id).map(|rows| rows.iter().filter(|row| !row.charged).map(|row| row.seat).collect()).unwrap_or_default();
    seats.sort();
    seats.dedup();
    seats
}

/// **The `Held` exposure** (RFC-0007 §I.7): every attester of a claim whose data the chain has concluded was not served loses
/// [`PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1`] ‰ of its collateral. Called by the data-availability default after the producer's own
/// charge, with the attesters read before it; marks the rows charged where the claim stands.
pub(super) fn charge_held_attesters_v1(
    builder: &mut TransitionBuilder<'_>,
    claim_id: &Hash64,
    attesters: &[PalwBondKeyV2],
) -> Result<(), PalwStateV2Error> {
    for seat in attesters {
        let Some(record) = builder.state.bonds.get(seat) else { continue };
        let exposure = palw_vertex_permille_of_v1(u128::from(record.collateral), PALW_VERTEX_HELD_EXPOSURE_PERMILLE_V1)
            .ok_or(PalwStateV2Error::Overflow("held exposure"))?;
        builder.slash_bond(*seat, exposure)?;
    }
    if let Some(mut rows) = builder.state.vertex.held.get(claim_id).cloned() {
        for row in rows.iter_mut().filter(|row| attesters.contains(&row.seat)) {
            row.charged = true;
        }
        builder.write_vertex_held(*claim_id, Some(rows));
    }
    Ok(())
}

impl PalwChainStateV2 {
    /// **The vertex tables' own consistency**: a tally belongs to a `PanelBound` claim with a panel and names only seats of it, each
    /// once; `Held` rows belong to live claims with panels, at most the per-claim cap; a round row's round is not in the future of
    /// the last block.
    pub(crate) fn assert_vertex_consistency_v1(&self) -> Result<(), PalwStateV2Error> {
        let bad = |why: String| Err(PalwStateV2Error::CarriageInconsistent(format!("vertex tables: {why}")));
        for (claim_id, tally) in &self.vertex.tallies {
            let (Some(claim), Some(panel)) = (self.claims.get(claim_id), self.panels.get(claim_id)) else {
                return bad(format!("a tally of claim {claim_id}, which has no claim or panel"));
            };
            if !matches!(claim.phase, PalwClaimPhaseV2::PanelBound { .. }) {
                return bad(format!("a tally of claim {claim_id}, which is not PanelBound"));
            }
            if tally.counted.is_empty() {
                return bad(format!("an empty tally of claim {claim_id}"));
            }
            for (i, row) in tally.counted.iter().enumerate() {
                if !panel.seats.iter().any(|seat| seat.bond == row.seat) {
                    return bad(format!("a counted leaf of claim {claim_id} by a seat the panel does not hold"));
                }
                if tally.counted[..i].iter().any(|other| other.seat == row.seat) {
                    return bad(format!("claim {claim_id} counts a seat twice"));
                }
            }
        }
        for (claim_id, rows) in &self.vertex.held {
            let (Some(claim), true) = (self.claims.get(claim_id), self.panels.contains_key(claim_id)) else {
                return bad(format!("Held rows of claim {claim_id}, which has no claim or panel"));
            };
            if claim.phase.is_terminal() {
                return bad(format!("Held rows of claim {claim_id}, which is terminal"));
            }
            if rows.is_empty() || rows.len() > PALW_VERTEX_HELD_MAX_PER_CLAIM_V1 {
                return bad(format!("Held rows of claim {claim_id} are empty or past the cap"));
            }
        }
        Ok(())
    }
}
