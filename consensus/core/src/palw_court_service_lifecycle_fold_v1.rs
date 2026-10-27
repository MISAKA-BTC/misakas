//! **G14 lane court-f02 (2026-10-22): the native fold side of RFC-0026 F02, RFC-0028 and RFC-0029** — written, not compiled.
//! A child of `palw_state_v2`. Everything here is DORMANT: it acts only past `palw_tir_public_conviction_v1` (the extras flag the
//! fold already reads), F02's service pieces additionally only once a reviewed service profile is pinned
//! ([`crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1`], typed-missing), and the shared lifecycle additionally
//! only past `palw_court_outsider_lane_v1`.
//!
//! * [`install_court_service_binding_v1`] — RFC-0028: ordinary original admission installs the claim's service binding (the
//!   `CourtDutyClockV1` prerequisite: `CourtDutyClockV1::check_context` needs the binding).
//! * [`observe_court_service_block_v1`] — RFC-0026 §4 / ADR-0192 A: each accepting block's ACTUAL reserved supply is recorded on
//!   the rooted book; an unsupplied slot is moved by the canonical recovery sweep (hold/recover, never a default).
//! * [`apply_shared_court_op_v1`] — RFC-0029: the shared TIR fold's lifecycle dispatcher; a newly stored bottom with a native fault
//!   applies its economic effect through the existing conviction funnel ([`apply_shared_bottom_conviction_v1`]).
use super::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}

/// F02's service pieces are in force: past the TIR public conviction fence, with a pinned service profile.
pub(super) fn court_service_f02_active_v1(b: &TransitionBuilder<'_>) -> bool {
    b.extras.tir_public_conviction_active && crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1.is_some()
}

/// The shared (owner-independent) court lifecycle is in force at `daa`: past the TIR public conviction fence AND the court-owner
/// outsider lane (`palw_court_outsider_lane_v1`, which itself needs `palw_legacy_public_filer_v1` at or below it).
pub(super) fn shared_court_lifecycle_active_v1(b: &TransitionBuilder<'_>, daa: u64) -> bool {
    b.extras.tir_public_conviction_active && b.params.court_outsider_lane_active_v1(daa)
}

/// **RFC-0028: install the original's service binding at ordinary admission.** Called after the original frame row is written
/// (its context check reads the frame). `Ok(false)`: dormant (fence off or no pinned profile) — the old behaviour, no row.
pub(super) fn install_court_service_binding_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
) -> Result<bool, PalwStateV2Error> {
    if !court_service_f02_active_v1(b) {
        return Ok(false);
    }
    let Some(profile) = crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1 else { return Ok(false) };
    install_court_service_binding_with_profile_v1(b, ctx, claim, &profile).map(|()| true)
}

/// [`install_court_service_binding_v1`] under an explicit profile (fixtures; a future pinned profile).
pub(super) fn install_court_service_binding_with_profile_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    profile: &crate::palw_court_service_binding_v1::CourtServiceProfileV1,
) -> Result<(), PalwStateV2Error> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim)
        .ok_or_else(|| refused("service binding install has no original frame"))?;
    let plan = frame.plan_v1();
    if plan.terms.accepted_daa != ctx.daa_score || plan.original_claim != claim {
        return Err(refused("service binding installs only at the original's own accepting point"));
    }
    let signers = frame.custody_v1().liable_signers_v1();
    let next = crate::palw_court_service_binding_v1::court_service_binding_candidate_v1(
        b.state.court_service_state_v1(),
        claim,
        plan.allocation_id,
        plan.source_id,
        plan.terms.accepted_daa,
        &signers,
        profile,
    )
    .map_err(|e| refused(format!("court service binding refused: {e:?}")))?;
    // The native writer re-runs `check_retains_unresolved` and `check_context` (signed custody horizon) and journals the write.
    // G14 lane r6-d1-release: written in the row variant the book already has (a released book stays released).
    b.write_court_service_state_v1(next)
}

/// **ADR-0192 A/A (lane F03b): install the claim-common shared frontier at the original's own accepting point.** Runs after the
/// claim's service binding is written (F02 plain admission or the RFC-0035 funded all-branch book). `Ok(false)` when the shared
/// lifecycle is not in force at this DAA or the claim has no binding (dormant: no row, the old behaviour), or when the identical
/// context is already installed (the funded path calls this twice). A different retained context is refused: the frontier is fixed
/// at acceptance and no owner, filer or Sybil can replace it. Fails closed: with the lifecycle in force and a binding present, an
/// underivable frontier refuses the admission instead of accepting a claim no outsider can pursue.
pub(super) fn install_shared_frontier_context_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
) -> Result<bool, PalwStateV2Error> {
    if !shared_court_lifecycle_active_v1(b, ctx.daa_score) {
        return Ok(false);
    }
    if !b.state.court_service_state_v1().is_some_and(|s| s.claims.contains_key(&claim)) {
        return Ok(false);
    }
    let derived = crate::palw_court_shared_tir_state_v1::derive_shared_tir_context_v1(&b.state, claim)
        .map_err(|e| refused(format!("shared frontier install refused: {e}")))?;
    // F03b2 (provisional): Q (in packets, every coordinate at its maximum size) <= c * W, so the schedule serves every coordinate
    // before hard. Over capacity is a production error at original acceptance: the admission is refused (checkpoint restores).
    let universe_packets = crate::palw_court_shared_tir_state_v1::shared_frontier_universe_packets_v1(&b.state, &derived)
        .map_err(|e| refused(format!("shared frontier packets: {e}")))?;
    if !crate::palw_court_shared_frontier_budget_v1::shared_frontier_capacity_admits_v1(
        universe_packets,
        crate::palw_g14_values_v1::PALW_G14_SERVICE_BUDGET_PACKETS_PER_DAA_V1,
        derived.hard_daa.saturating_sub(derived.original_accepted_daa),
    ) {
        return Err(refused("shared frontier universe exceeds the reserved service capacity of the hard window"));
    }
    if derived.original_accepted_daa != ctx.daa_score {
        return Err(refused("shared frontier installs only at the original's own accepting point"));
    }
    let key = Key::CourtSharedContext(claim);
    match b.state.staged_court_row_v1(&key) {
        Some(Row::CourtSharedContext(old)) if **old == derived => return Ok(false),
        Some(_) => return Err(refused("shared frontier context is immutable once installed")),
        None => {}
    }
    b.write_staged_row_v1(key, Some(Row::CourtSharedContext(Box::new(derived))))?;
    Ok(true)
}

/// **ADR-0192 A: record this accepting block's actual reserved service supply.** Called once per block after every object. The
/// unreserved court cost is the block's deterministic court-capacity spend (`charge_court_op_v1`; zero below
/// `palw_court_capacity_v1`). When unreserved court operations spent reserved capacity, the slot records NO supply (a later
/// assessment is `MissingSupply` → no-slash hold); the block itself stands. The canonical recovery sweep then runs on the
/// current clock.
pub(super) fn observe_court_service_block_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> Result<(), PalwStateV2Error> {
    // Lane r5: a funded original's all-branch binding (RFC-0035 A, installed under the funded schedule's profile) needs its
    // supply recorded too, or its packets stay `Pending` and its liability never closes. Bindings exist only past the conviction
    // fence (F02 with a pinned profile, or a funded admission), and an empty book returns below, so gating on the fence alone
    // changes nothing where no binding exists.
    // Lane F02b: a kernel-route admission schedule in force records supply for the bindings it installed.
    if !(court_service_f02_active_v1(b)
        || b.extras.kernel_court_admission_v1.is_some()
        || (b.extras.tir_public_conviction_active && !b.state.funded_escrows_v1().is_empty()))
    {
        return Ok(());
    }
    let Some(old) = b.state.court_service_state_v1() else { return Ok(()) };
    if old.claims.is_empty() {
        return Ok(());
    }
    let spent = b.court_cost_spent;
    let unreserved = crate::palw_court_service_v1::ServiceCost { bytes: spent.bytes, work: spent.work, io: 0, controls: 0 };
    let block: [u8; 64] = *ctx.block.as_byte_slice();
    let mut next = old.clone();
    match next.book.observe_block(ctx.daa_score, block, unreserved) {
        Ok(crate::palw_court_service_v1::ServiceObservation::FirstBlock) => {}
        // A later block at the same DAA cannot repair or duplicate the first opportunity.
        Ok(crate::palw_court_service_v1::ServiceObservation::AdditionalBlockAtSameDaa) => return Ok(()),
        // Reserved capacity was spent: no supply at this DAA (the clock is not advanced by this block).
        Err(crate::palw_court_service_v1::ServiceRefusal::ReservedCapacitySpent) => return Ok(()),
        Err(e) => return Err(refused(format!("court service observation refused: {e:?}"))),
    }
    if &next != old {
        b.write_court_service_state_v1(next)?;
    }
    b.recover_court_service_supply_v1(ctx)?;
    // G14 lane r6-d1-release (user decision D1): release every binding that is fully decided (bounded per block).
    release_resolved_court_service_bindings_v1(b, ctx)?;
    Ok(())
}

/// **G14 lane r6-d1-release: the resolved-binding release step** (user decision D1). Runs once per accepting block, after the
/// block's supply was recorded and the recovery sweep ran, only past `palw_tir_public_conviction_v1` (the caller's gate) and only on
/// the FIRST block at a DAA (the book's clock equals this block's DAA and the observation advanced it). A binding is released when
/// [`PalwChainStateV2::court_service_release_eligible_v1`] holds (every packet decided at/after hard, no open court, every duty clock
/// past its response deadline) AND no dependent row of the claim remains (one scan of the staged rows, only when a candidate
/// exists; dependents are no longer held once eligible, so they leave at their own TTL). At most
/// [`crate::palw_court_service_release_v1::COURT_SERVICE_RELEASES_PER_BLOCK_V1`] per block, in claim order. The book is rewritten in
/// its released form through the journalled writer (reorg undoes it with the rest of the block); each released claim's DL-1 deadline
/// is re-armed (its retirement, now that the hold is gone).
pub(super) fn release_resolved_court_service_bindings_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
) -> Result<(), PalwStateV2Error> {
    let Some(service) = b.state.court_service_state_v1() else { return Ok(()) };
    if service.book.last_observed_daa != Some(ctx.daa_score) {
        return Ok(());
    }
    // G14-FIX S2 (audit, 2026-10-27): below the gate the cap is applied BEFORE the dependent-row filter (the original order, kept
    // byte-identical for live testnet-12), so N dependent-held claims at the front of the book starve every releasable claim behind
    // them forever. At/after the gate the dependent filter runs first and the cap is applied to the releasable claims.
    let filter_first = release_filter_first_v1(b.params, ctx.daa_score);
    let state = &b.state;
    let candidates = court_service_release_candidates_v1(
        service.claims.keys().filter(|claim| state.court_service_release_eligible_v1(claim)).copied(),
        || state.staged_courts.keys().filter_map(|key| state.court_service_dependent_claim_v1(key)),
        crate::palw_court_service_release_v1::COURT_SERVICE_RELEASES_PER_BLOCK_V1,
        filter_first,
    );
    if candidates.is_empty() {
        return Ok(());
    }
    let mut next = b.state.court_service_as_v3_v1().ok_or_else(|| refused("service book vanished before its release"))?;
    next.release_v1(&candidates, ctx.daa_score).map_err(refused)?;
    b.write_staged_row_v1(Key::CourtService, Some(Row::CourtServiceV3(Box::new(next).into())))?;
    for claim in candidates {
        b.rearm_claim_deadline_dl1_v1(claim, ctx.daa_score)?;
    }
    Ok(())
}

/// **The resolved-binding release selection (pure).** `eligible` in claim order; `dependents` yields the claim of every dependent
/// staged row (scanned only when a candidate exists). `filter_first = false` is the original order (take `limit`, then drop the
/// dependent-held ones); `true` (G14-FIX S2) drops the dependent-held ones first, then takes `limit` in claim order.
pub(super) fn court_service_release_candidates_v1<D: Iterator<Item = Hash64>>(
    eligible: impl Iterator<Item = Hash64>,
    dependents: impl FnOnce() -> D,
    limit: usize,
    filter_first: bool,
) -> std::collections::BTreeSet<Hash64> {
    let mut candidates: std::collections::BTreeSet<Hash64> = eligible.take(if filter_first { usize::MAX } else { limit }).collect();
    if candidates.is_empty() {
        return candidates;
    }
    for claim in dependents() {
        candidates.remove(&claim);
        if candidates.is_empty() {
            return candidates;
        }
    }
    if filter_first {
        candidates = candidates.into_iter().take(limit).collect();
    }
    candidates
}

/// **G14-FIX S2: the release step filters dependents BEFORE the per-block cap** at/after `palw_court_clock_v2` (a fence of the G14
/// list at testnet-12 DAA 17,400, mirrored as `court_clock_v2_from_daa`). Below it, or unarmed: the original take-then-filter order.
pub(super) fn release_filter_first_v1(params: &PalwStateParamsV2, daa: u64) -> bool {
    params.court_clock_v2_from_daa().is_some_and(|from| daa >= from)
}

/// **G14 lane r5-f02-wire: the wire entry (tag 178, `PalwConsensusObjectV2::SharedCourtOpV1`)** — maps the carried operation onto
/// [`apply_shared_court_op_v1`] (borrowed; no copy of bodies or parts).
pub(super) fn apply_shared_court_wire_op_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    op: &crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1,
) -> Result<SharedCourtOpEffectV1, PalwStateV2Error> {
    use crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1 as W;
    op.validate_shape_v1().map_err(refused)?;
    // G14 lane F03b2: no size refusal here. A request is charged ⌈max bytes / 34,000⌉ packets in the request fold.
    let op = match op {
        W::Body { claim, ordinal, bytes } => SharedCourtOpV1::Body { claim: *claim, ordinal: *ordinal, bytes },
        W::Ids { claim, rows_root, ids } => SharedCourtOpV1::Ids { claim: *claim, rows_root: *rows_root, ids },
        W::Request { claim, intent } => SharedCourtOpV1::Request { claim: *claim, intent },
        W::Numerical { manifest, parts } => SharedCourtOpV1::Numerical { manifest, parts },
        // RFC-0026 F02 × RFC08 hold: its own gates (hold fence, F02, slash amount); not the shared TIR lifecycle's.
        W::NativeHeldDefault { family, job } => {
            super::palw_native_post_final_default_fold_v1::apply_native_post_final_default_v1(b, ctx, *family, job)?;
            return Ok(SharedCourtOpEffectV1::NativeDefaulted);
        }
    };
    apply_shared_court_op_v1(b, ctx, op)
}

/// One shared-court operation of the owner-independent lifecycle (RFC-0029). Carried on the wire by tag 178
/// ([`apply_shared_court_wire_op_v1`]); this is the single entry every carriage reaches.
pub(super) enum SharedCourtOpV1<'a> {
    Request {
        claim: Hash64,
        intent: &'a crate::palw_court_shared_tir_frontier_v1::SignedSharedRequestV1,
    },
    Body {
        claim: Hash64,
        ordinal: u64,
        bytes: &'a [u8],
    },
    Ids {
        claim: Hash64,
        rows_root: Hash64,
        ids: &'a [u32],
    },
    Numerical {
        manifest: &'a crate::palw_court_numerical_transport_v1::NumericalManifestV1,
        parts: &'a [crate::palw_court_numerical_transport_v1::NumericalPartV1],
    },
    /// RFC-0024 A (lane r6-p2-shared): a single-position shared bottom, judged against the position round (writer row) on
    /// chain. Dormant below `palw_tir_bounded_door_v1` (refused by its fold). No wire carriage yet: the tag-178 numerical
    /// transport has no position-bottom kind (see the lane notes).
    PositionBottom {
        control: &'a crate::palw_court_shared_tir_bottom_v1::SignedSharedTirPositionBottomV2,
    },
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum SharedCourtOpEffectV1 {
    Request(crate::palw_court_shared_tir_frontier_v1::SharedRequestEffectV1),
    Stored(bool),
    Numerical(super::palw_court_shared_tir_fold_v1::NumericalTransportEffectV1),
    /// A newly stored bottom found a native fault and the conviction funnel applied it.
    Convicted,
    /// A held text29 / funded v11 Final defaulted after Final (refund + slash).
    NativeDefaulted,
}

/// **RFC-0029: the shared TIR fold's lifecycle dispatcher.** Dormant (`ObjectiveOffenceDormant`) unless the shared lifecycle is in
/// force. Atomic: a refused economic effect rolls the stored bottom back with it.
pub(super) fn apply_shared_court_op_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    op: SharedCourtOpV1<'_>,
) -> Result<SharedCourtOpEffectV1, PalwStateV2Error> {
    use super::palw_court_shared_tir_fold_v1 as fold;
    if !shared_court_lifecycle_active_v1(b, ctx.daa_score) {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let saved = b.try_checkpoint()?;
    let result = (|| match op {
        SharedCourtOpV1::Request { claim, intent } => fold::request(b, ctx, claim, intent).map(SharedCourtOpEffectV1::Request),
        SharedCourtOpV1::Body { claim, ordinal, bytes } => {
            fold::share_body(b, ctx, claim, ordinal, bytes).map(SharedCourtOpEffectV1::Stored)
        }
        SharedCourtOpV1::Ids { claim, rows_root, ids } => {
            fold::share_ids(b, ctx, claim, rows_root, ids).map(SharedCourtOpEffectV1::Stored)
        }
        SharedCourtOpV1::Numerical { manifest, parts } => {
            let effect = fold::numerical_transport(b, ctx, manifest, parts)?;
            if let fold::NumericalTransportEffectV1::Bottom(
                true,
                crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1::NativeFault(_),
            ) = &effect
            {
                // The row the bottom just stored names its claim and filer (signed, verified by `fold::bottom`).
                let (claim, filer) = match b.entries.last() {
                    Some(PalwDeltaEntryV2::StagedCourtSeed {
                        key: Key::CourtSharedBottom(_),
                        new: Some(Row::CourtSharedBottom(r)),
                        ..
                    }) => (r.control.claim, r.control.filer),
                    _ => return Err(refused("shared bottom effect lost its stored row")),
                };
                // G14 lane r5-f02-wire (item 5): the court end archive this conviction may build records the ACTUAL bottom
                // bundle as its native evidence (the accepting point + the tag-178 object carrying this exact manifest and these
                // parts), never `Uncaptured`. Through `apply_object` it is already this object; a direct dispatcher call captures it
                // here, and the builder's previous evidence is restored afterwards.
                let previous = capture_shared_bottom_evidence_v1(b, ctx, manifest, parts)?;
                let applied = apply_shared_bottom_conviction_v1(b, ctx, claim, filer);
                b.court_end_evidence = previous;
                applied?;
                return Ok(SharedCourtOpEffectV1::Convicted);
            }
            Ok(SharedCourtOpEffectV1::Numerical(effect))
        }
        SharedCourtOpV1::PositionBottom { control } => {
            let (opened, outcome) = fold::position_bottom(b, ctx, control)?;
            if opened && matches!(outcome, crate::palw_court_shared_tir_bottom_v1::SharedTirBottomOutcomeV1::NativeFault(_)) {
                // The same conviction funnel as a newly stored whole-tile bottom (claim and filer are the signed control's).
                apply_shared_bottom_conviction_v1(b, ctx, control.control.claim, control.control.filer)?;
                return Ok(SharedCourtOpEffectV1::Convicted);
            }
            Ok(SharedCourtOpEffectV1::Numerical(fold::NumericalTransportEffectV1::Bottom(opened, outcome)))
        }
    })();
    if result.is_err() {
        b.restore(saved);
    }
    result
}

/// **G14 lane r5-f02-wire (item 5): capture the shared bottom bundle as the court end evidence** — `Native { point, object }` with
/// the tag-178 object carrying exactly this manifest and these parts (already so when it arrived through `apply_object`). Returns
/// the builder's previous evidence, which the caller restores after the conviction.
pub(super) fn capture_shared_bottom_evidence_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    manifest: &crate::palw_court_numerical_transport_v1::NumericalManifestV1,
    parts: &[crate::palw_court_numerical_transport_v1::NumericalPartV1],
) -> Result<
    Option<crate::palw_staged_court_state_v1::PalwSharedCourtV1<crate::palw_court_end_archive_v1::CourtEndEvidenceV1>>,
    PalwStateV2Error,
> {
    let carried = PalwConsensusObjectV2::SharedCourtOpV1 {
        op: Box::new(crate::palw_court_shared_op_carriage_v1::PalwSharedCourtOpV1::Numerical {
            manifest: manifest.clone(),
            parts: parts.to_vec(),
        }),
    };
    let previous = b.court_end_evidence.clone();
    let captured = matches!(previous.as_deref(),
        Some(crate::palw_court_end_archive_v1::CourtEndEvidenceV1::Native { point, object })
            if point == ctx && object.as_ref() == &carried);
    if !captured {
        b.prepare_native_court_end_cause_v1(ctx, &carried)?;
    }
    Ok(previous)
}

/// **RFC-0029: a shared bottom's native fault → claim void / slash / reward refusal**, through the SAME funnel a public TIR
/// conviction uses (`apply_conviction_v1`): every credited Valid custodian charged on its own S4 lock, the producer convicted by
/// `convict_by_court_verdict_as_v1` (void / Final reversal, Gate C revocation, the `CourtConviction` record, the reporter reward and
/// the refusal of the producer's own reward). The reason is `CourtHeldVerdict` — a dissection's bottom — and nothing is forfeited by
/// execution root: a bottom fault may concern a signed numerical statement rather than the whole execution.
pub(super) fn apply_shared_bottom_conviction_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    filer: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_v1(&b.state, claim_id)
        .ok_or_else(|| refused("shared bottom conviction has no original frame"))?
        .clone();
    let producer = frame.custody_v1().pledge_v1().producer;
    // The filer is any Active bond at the floor that is not liable on this claim (G14 R4's rule).
    let record = b.state.bond(&filer).ok_or(PalwStateV2Error::MissingBond(filer))?;
    if filer == producer
        || frame.original_custody_v1().credit_v1(&filer).is_some()
        || frame.original_custody_v1().grant_v1(&filer).is_some()
        || !matches!(record.status, PalwBondStatusV2::Active)
        || record.collateral < b.params.min_collateral_sompi()
    {
        return Err(refused("shared bottom filer is liable on this claim, inactive or below the floor"));
    }
    let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {
        return Err(refused("shared bottom conviction target changes the original claim's producer or execution"));
    }
    if b.state.consumed_offences.contains_key(&palw_court_conviction_offence_id_v1(&producer.0, &claim_id))
        || matches!(
            claim.phase,
            PalwClaimPhaseV2::Voided {
                reason: PalwVoidReasonV2::CourtFraud | PalwVoidReasonV2::CourtHeldVerdict | PalwVoidReasonV2::CourtDefault,
                ..
            }
        )
    {
        return Err(refused("original claim is already convicted; a conviction is recorded once"));
    }
    let credited: Vec<PalwBondKeyV2> = frame.original_custody_v1().credits_v1().map(|(seat, _)| *seat).collect();
    for bond in std::iter::once(&producer).chain(credited.iter()) {
        if !b.state.staged_bond_exit_held_v1(bond, ctx.daa_score) {
            return Err(refused("shared bottom liable bond lost its original exit hold; refusing a fictitious collection"));
        }
    }
    for seat in &credited {
        let offence = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id);
        if b.state.consumed_offences.contains_key(&offence) {
            continue;
        }
        if !b.state.slashable_locks.get(&(*seat, claim_id)).is_some_and(|lock| lock.amount > 0 && lock.expiry_daa > ctx.daa_score) {
            return Err(refused("shared bottom credited Valid seat lost its unconsumed native S4 lock; no silent skip"));
        }
    }
    if !credited.is_empty() {
        let conv = b.open_conviction_v1(&credited, ctx.daa_score);
        let g = b.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        b.charge_covering_signers_v1(&conv, claim_id, &credited, g)?;
    }
    b.convict_by_court_verdict_as_v1(ctx, claim_id, &claim, filer, None, PalwVoidReasonV2::CourtHeldVerdict)
}

#[cfg(test)]
mod g14_fix_s2_tests {
    use super::*;

    fn id(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }

    /// The book: `limit` dependent-held eligible claims first (claim order), then one releasable claim behind them.
    fn book(limit: usize) -> (Vec<Hash64>, Vec<Hash64>, Hash64) {
        let mut all: Vec<Hash64> = (1..=limit as u64 + 1).map(id).collect();
        all.sort();
        let releasable = *all.last().unwrap();
        let dependent = all[..limit].to_vec();
        (all, dependent, releasable)
    }

    /// Below the gate (and unarmed): the original take-then-filter order — the first `limit` are all dependent, so NOTHING is
    /// released this block and the claim behind them waits (the audited starvation, kept byte-identical for live testnet-12).
    #[test]
    fn below_the_gate_the_original_order_is_kept() {
        let limit = crate::palw_court_service_release_v1::COURT_SERVICE_RELEASES_PER_BLOCK_V1;
        let (all, dependent, _) = book(limit);
        let got = court_service_release_candidates_v1(all.iter().copied(), || dependent.iter().copied(), limit, false);
        assert!(got.is_empty(), "old order: the cap is spent on dependent-held claims");
        // The original order verbatim, as a reference implementation.
        let mut reference: std::collections::BTreeSet<Hash64> = all.iter().take(limit).copied().collect();
        for c in &dependent {
            reference.remove(c);
        }
        assert_eq!(got, reference);
        // With no dependents the old order releases exactly the first `limit`.
        let got = court_service_release_candidates_v1(all.iter().copied(), std::iter::empty, limit, false);
        assert_eq!(got, all.iter().take(limit).copied().collect());
    }

    /// At/after the gate: with the first `limit` all dependent, the releasable claim behind them is selected in the same block,
    /// and the cap still bounds the release.
    #[test]
    fn past_the_gate_a_releasable_claim_behind_dependents_is_released() {
        let limit = crate::palw_court_service_release_v1::COURT_SERVICE_RELEASES_PER_BLOCK_V1;
        let (all, dependent, releasable) = book(limit);
        let got = court_service_release_candidates_v1(all.iter().copied(), || dependent.iter().copied(), limit, true);
        assert_eq!(got, std::collections::BTreeSet::from([releasable]));
        // The cap still holds: 2·limit releasable claims → exactly the first `limit` in claim order.
        let mut many: Vec<Hash64> = (1..=2 * limit as u64).map(id).collect();
        many.sort();
        let got = court_service_release_candidates_v1(many.iter().copied(), std::iter::empty, limit, true);
        assert_eq!(got, many.iter().take(limit).copied().collect());
    }

    /// The gate is `palw_court_clock_v2` (G14 list, 17,400 on testnet-12) through its state-params mirror.
    #[test]
    fn the_gate_is_the_court_clock_v2_mirror() {
        let unarmed = PalwStateParamsV2::new(100, 10, 10, 20, 500, 1000, Hash64::from_u64_word(1), 4, 1000, 100, 1000, 0).unwrap();
        let armed = unarmed.clone().with_court_clock_v2_from_daa(Some(17_400));
        assert!(!release_filter_first_v1(&armed, 17_399));
        assert!(release_filter_first_v1(&armed, 17_400));
        assert!(!release_filter_first_v1(&unarmed, u64::MAX));
    }
}
