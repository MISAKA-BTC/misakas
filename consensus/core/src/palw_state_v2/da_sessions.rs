//! **DA sessions: opening, answering, and the dissection opened at a named leaf** — moved out of `palw_state_v2.rs` (2026-10-12, a behaviour-preserving split: the items are the same; members the parent calls are `pub(super)`).

use super::*;

/// **Put a refuted claim's own clock back** (ADR-0062 SA-4's other half).
///
/// The deadline is RE-DERIVED from the resumed phase rather than remembered, so the index stays
/// exactly what `assert_deadline_consistency` recomputes from the claims alone — the property that
/// lets the index be dropped from the state root and rebuilt on load.
///
/// The restored deadline is derived from the anchor `resume_claim_after_da_session_v2` has already
/// advanced by the session's length, so it is never in this block's past for a claim that answered
/// on time. What it may NOT do is RESTART, which is what D4's "return to `ReceiptLicensed
/// { licensed_daa: the disclosure's DAA }`" would have done — see the phase's own note on why
/// serial accusations would then hold a claim open forever. Advancing by exactly the elapsed
/// session is the difference between the two: a producer gets back the time it had left and not one
/// DAA more.
/// **ADR-0062 SA-1's accusation, as one function both accusation kinds open** (ADR-0103 Decision 4).
/// `event_index_is_the_unit` is `true` for an event accusation, whose index is range-checked
/// against the claim's trace here, and `false` for a held one, whose unit was bounded against the
/// claim's binding before this is called and whose phase carries
/// `PALW_HELD_DA_EVENT_INDEX_SENTINEL_V1`. Every other gate, the clock, the reservation and the
/// phase are the event court's, spelled once.
pub(super) fn open_da_session_v2(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    missing_event_index: u32,
    accuser: &PalwBondKeyV2,
    event_index_is_the_unit: bool,
) -> Result<(), PalwStateV2Error> {
    if !builder.da_court {
        return Err(PalwStateV2Error::DaCourtDormant);
    }
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
    if claim.phase.is_terminal() {
        return Err(PalwStateV2Error::WrongPhase { claim: claim_id, edge: "DefaultAccused" });
    }
    // Singular per claim, and the phase IS the singularity: a claim holds one phase, so a
    // second accusation on a disputed claim has nowhere to go. No index to keep in step.
    if matches!(claim.phase, PalwClaimPhaseV2::DefaultDisputed { .. }) {
        return Err(PalwStateV2Error::DaAccusationAlreadyOpen(claim_id));
    }
    // **A phase this fold could not put back is a phase it may not take away** (SA-7).
    // `resume_claim_after_da_session_v2` returns a `Provisional` claim's paused time on
    // `rebound_daa`, because `accepted_daa` anchors the retention obligation and may not
    // move. A `Provisional` claim that has never been rebound has no such anchor — and it
    // also has no panel, so the check below already refuses it. Asked here anyway: the
    // alternative is an invariant that lives only in the argument that the panel gate
    // implies it, and the arm that would discover it broken is the DISCLOSURE, where
    // refusing means an honest producer's answer is rejected.
    if matches!(claim.phase, PalwClaimPhaseV2::Provisional) && claim.rebound_daa.is_none() {
        return Err(PalwStateV2Error::DaClaimNotAccusable(claim_id));
    }
    // **An accusation names ONE piece of the obligation, and the obligation's size is a
    // chain fact.** `trace_chunk_count` is pinned by ADR-0072 Decision 8, so the range this
    // refuses against cannot be restated by the producer OR the accuser.
    let (accused_row, _accused_tile) = palw_da_event_index_parts_v1(missing_event_index);
    let accusable_rows = palw_da_max_accusable_rows_v1(claim.trace_chunk_count);
    // ADR-0103 Decision 4: a held accusation's unit was bounded against the claim's binding
    // by the caller, and its phase carries the sentinel, which is no event at all.
    if event_index_is_the_unit && accused_row >= accusable_rows {
        return Err(PalwStateV2Error::DaIndexOutOfRange { claim: claim_id, index: missing_event_index, count: accusable_rows });
    }
    // **The whole disclose window must fit inside the retention obligation** (SA-1, SA-6).
    // Accusing at the last DAA of retention would otherwise be a conviction dressed as a
    // question: the producer was already allowed to delete what it is being asked to open.
    let disclose = palw_da_disclose_window_daa_v1(builder.params);
    let deadline_daa = ctx.daa_score.checked_add(disclose).ok_or(PalwStateV2Error::Overflow("da disclose deadline"))?;
    builder.state.require_paid_consumer_dispute_v1(&claim_id, ctx.daa_score, deadline_daa)?;
    palw_exec_v2_rewards_v1::require_native_session_window(&builder.state, builder.params, &claim_id, deadline_daa)?;
    if ctx.daa_score < claim.accepted_daa || deadline_daa > claim.trace_retention_daa {
        return Err(PalwStateV2Error::DaOutsideRetention {
            claim: claim_id,
            at: ctx.daa_score,
            window: disclose,
            retention_daa: claim.trace_retention_daa,
        });
    }
    // **A panel must be bound.** Before that nobody has been in a position to ask for the
    // material, so there is nothing to have been withheld — and the panel is where
    // `seat_count` comes from, which is what an accusation is priced against.
    let panel = builder.state.panels.get(&claim_id).ok_or(PalwStateV2Error::DaClaimNotAccusable(claim_id))?.clone();
    if panel.seats.is_empty() {
        return Err(PalwStateV2Error::EmptyPanel);
    }
    if *accuser == claim.bond {
        return Err(PalwStateV2Error::DaAccuserIsTheProducer(*accuser));
    }
    let accuser_record = builder.state.bonds.get(accuser).ok_or(PalwStateV2Error::MissingBond(*accuser))?.clone();
    // Accusing is a bonded act, and this asks it the way the court asks it: Active, at or
    // above the registry floor. A retiring bond may not put an honest claim under session
    // with collateral that is already leaving.
    if !matches!(accuser_record.status, PalwBondStatusV2::Active) {
        return Err(PalwStateV2Error::BondNotActive(*accuser));
    }
    if accuser_record.collateral < builder.params.min_collateral_sompi() {
        return Err(PalwStateV2Error::BondBelowFloor {
            bond: *accuser,
            collateral: accuser_record.collateral,
            floor: builder.params.min_collateral_sompi(),
        });
    }
    // **"A seat of that claim's panel or a bonded challenger" (SA-1) is the pair of checks
    // above, and the seat case is the subset.** A seat IS a bond the draw already found
    // Active and funded, so requiring Active-and-funded admits every seat and admits a
    // challenger on the same terms — which is the intent: the accusation is priced, not
    // privileged. What is excluded is the producer itself (above) and any bond that is
    // leaving or under the floor.
    let exposure = palw_da_accusation_exposure_v2(claim.reserved, panel.seats.len(), builder.params.min_collateral_sompi());
    // **Admission item 8, on this lane** (SA-7, closing the DA half of P0-10): what this
    // bond already backs — its live claims, its own registrations and every accusation it
    // has open — plus what this accusation would reserve, against collateral × ratio.
    //
    // Being Active and above the floor is a check on the accuser's STATE, and reserving
    // does not change it: `write_exposure` moves the exposure ledger and leaves
    // `collateral` alone, which the lane's own `da3` test asserts ("reserving is not
    // charging"). So without a ceiling one bond holding exactly `min_collateral_sompi`
    // passed the same two checks once per live claim on the network and froze all of them
    // in a single block. The exposure ceiling the `CourtOpened` arm says "does the
    // counting" is applied only in `check_palw_attempt_admission_v2`, i.e. only to a bond
    // that wants to PRODUCE — no bound at all on a bond that only accuses.
    //
    // With the reservation capped at the same figure `slash_seat` can take, the ceiling is
    // a bound on money and not merely on count: `K × min(fraction, floor) ≤ ceiling`, so
    // the K-th refutation is funded exactly like the first and `slash_bond`'s clamp at
    // collateral stops being the thing that makes accusations K..K free.
    //
    // The expression is the free-prompt lane's (`FreePromptCommitted` above), for the
    // reason that one gives: a commitment meets no admission list, so the ceiling is the
    // transition's. `palw_mode_v2` proves `fp_max_exposure_ratio_permille` equals
    // `admission.max_exposure_ratio_permille`, so both lanes ride one number.
    //
    // The check and the write are `reserve_accuser_exposure_v2`, which the `CourtOpened`
    // arm calls too: one ceiling, one ledger, both arms — spelled once so they cannot
    // drift apart again, which is exactly how this arm came to have a bound and its
    // sibling did not.
    builder.reserve_accuser_exposure_v2(
        *accuser,
        accuser_record.collateral,
        exposure,
        "data-availability accusation",
        true,
        ctx.daa_score,
    )?;
    // The claim's own clock is paused while the session runs, exactly as an open court
    // pauses the path to `Final`; `resumed` carries the phase back.
    let mut disputed = claim.clone();
    disputed.phase = PalwClaimPhaseV2::DefaultDisputed {
        accused_daa: ctx.daa_score,
        missing_event_index,
        accuser: *accuser,
        accuser_exposure: exposure,
        resumed: Box::new(claim.phase.clone()),
    };
    builder.write_claim(claim_id, Some(disputed))?;
    builder.disarm_deadline(claim_id);
    builder.arm_deadline(deadline_daa, claim_id)?;
    Ok(())
}

pub(super) fn open_da_session_rcore_mode_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    accuser: PalwBondKeyV2,
    named: crate::palw_da_rcore_v1::PalwDaUnitV1,
    binding: Option<&crate::palw_step_leg::PalwStepBindingV2>,
    reacquire: bool,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_da_rcore_v1::{PalwDaDrawSpaceV1, PalwDaSessionV1, PalwDaUnitV1, palw_da_draw_seed_v1, palw_da_draw_units_v1};
    use crate::palw_held_da_v1::PalwHeldMissingV1;
    if !builder.da_court {
        return Err(PalwStateV2Error::DaCourtDormant);
    }
    let admission = builder.read().da_admission_v1(&claim_id, &accuser, ctx.daa_score)?;
    let claim = builder.state.claims.get(&claim_id).ok_or(PalwStateV2Error::MissingClaim(claim_id))?.clone();
    // **D2 (REQ-T6-3, restricted; `palw_court_model_scope_v1`)**: a held `StepLeaf` demand compels the producer's weight rows
    // (`artifact_openings`), so on a claim accepted past the fence it is not admissible — LG14-B's legacy held unit, whose answer owes
    // no model rows, is the route. A claim accepted below the fence keeps the demand (the OLD rule).
    if matches!(named, PalwDaUnitV1::Held(PalwHeldMissingV1::StepLeaf { .. })) {
        crate::palw_ruling_fences_v1::palw_court_model_scope_admits_v1(
            builder.params,
            &claim,
            crate::palw_court_scope_v1::CourtUnitKindV1::HeldStepLeaf,
        )
        .map_err(|refusal| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: refusal.why().to_string() })?;
    }
    // **M3 review F3: no session opens on a unit the chain already holds the answer to.** Such a
    // session could never be ended — every answer to it is `DaUnitAlreadyAnswered` — so a seat's
    // would pause the claim for the whole disclose window, and it would close "refuted" at its
    // deadline with the accuser's exposure held and burned at retirement. The accusation's signature
    // binds only (domain, claim, index, accuser), so without this a third party could REPLAY an
    // answered accusation to burn its accuser's stake and spend its seat's session budget: a refuted
    // session's units are all answered, so this refuses every replay of one.
    if !reacquire
        && builder.state.da_claims.get(&claim_id).is_some_and(|record| {
            crate::palw_da_rcore_v1::palw_da_unit_answered_v1(
                record,
                &named,
                crate::palw_da_rcore_v1::palw_da_in_run_rows_v1(&claim, builder.extras.fp_da_pins_active),
            )
        })
    {
        return Err(PalwStateV2Error::DaUnitAlreadyAnswered(claim_id));
    }
    let seed = palw_da_draw_seed_v1(&ctx.block, &claim_id, &accuser);
    let drawn = match (named, binding) {
        (PalwDaUnitV1::Event { row, tile }, None) => {
            let accusable = palw_da_max_accusable_rows_v1(claim.trace_chunk_count);
            if row >= accusable {
                return Err(PalwStateV2Error::DaIndexOutOfRange {
                    claim: claim_id,
                    index: palw_da_event_index_v1(row, tile),
                    count: accusable,
                });
            }
            let space = if matches!(claim.source, PalwClaimSourceV2::Attempt) && builder.state.class_is_held_v1(&claim.class_id) {
                PalwDaDrawSpaceV1::Nothing
            } else {
                PalwDaDrawSpaceV1::Events {
                    rows: crate::palw_da_rcore_v1::palw_da_in_run_rows_v1(&claim, builder.extras.fp_da_pins_active),
                    // RFC-0002 Phase F: an IR class's tiles come from its record (it has no fp
                    // profile); the same clamp as the legacy rule.
                    tiles: match builder.state.tir_classes.get(&claim.class_id) {
                        Some(record) => record.logits_tiles().clamp(1, 256),
                        None => crate::palw_da_rcore_v1::palw_da_row_tiles_v1(
                            builder.state.fp_work_profiles.get(&claim.class_id).map(|profile| profile.as_ref()),
                        ),
                    },
                }
            };
            palw_da_draw_units_v1(&seed, &named, &space)
        }
        (PalwDaUnitV1::Held(missing), Some(binding)) => {
            let form = builder.extras.prompt_ids_form_v1();
            if let PalwHeldMissingV1::StepLeaf { leaf } = missing
                && binding.committed_execution_root == claim.execution_root
                && crate::palw_da_rcore_v1::palw_da_step_leaf_is_fused_v1(binding, leaf)
            {
                return Err(PalwStateV2Error::DaUnitNeedsDissection { claim: claim_id, leaf });
            }
            crate::palw_held_da_v1::palw_held_da_check_accusation_v1(&claim.execution_root, &missing, binding, form)
                .map_err(|e| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, &claim_id, binding) {
                return Err(PalwStateV2Error::DaBindingIsIdentityFault { claim: claim_id, why });
            }
            let chunks_of = |checkpoint: u32| -> Option<u64> {
                let covered = crate::palw_context_ladder::palw_checkpoint_covered_at_index_v1(
                    &binding.shape_profile,
                    checkpoint,
                    binding.checkpoint_profile.checkpoint_interval,
                )?;
                let positions =
                    crate::palw_context_ladder::palw_checkpoint_positions_at_v1(&binding.shape_profile, &binding.job_context, covered);
                crate::palw_state_chunk_map::palw_state_chunk_count_at_v1(&binding.shape_profile, positions)
            };
            let space = match missing {
                PalwHeldMissingV1::PromptIdsTile { .. } => PalwDaDrawSpaceV1::PromptTiles {
                    tiles: crate::palw_prompt_ids_v1::prompt_ids_tile_count_v1(u64::from(binding.job_context.declared_prefill_tokens))
                        .unwrap_or(0),
                },
                PalwHeldMissingV1::StateChunk { .. } => {
                    PalwDaDrawSpaceV1::StateChunks { checkpoints: binding.checkpoint_count, chunks_of: &chunks_of }
                }
                PalwHeldMissingV1::StepRange { .. } | PalwHeldMissingV1::StepLeaf { .. } => {
                    PalwDaDrawSpaceV1::StepRanges { leaves: binding.step_leaf_count }
                }
            };
            // Every drawn unit is bounded exactly as the named one was; one that would not be (none
            // by construction) is dropped rather than demanded.
            palw_da_draw_units_v1(&seed, &named, &space)
                .into_iter()
                .filter(|unit| match unit {
                    PalwDaUnitV1::Held(drawn) => {
                        crate::palw_held_da_v1::palw_held_da_check_accusation_v1(&claim.execution_root, drawn, binding, form).is_ok()
                    }
                    PalwDaUnitV1::Event { .. }
                    | PalwDaUnitV1::TirStepLeaf { .. }
                    | PalwDaUnitV1::TirStepNode { .. }
                    | PalwDaUnitV1::TirRowNode { .. }
                    | PalwDaUnitV1::TirStepRun { .. }
                    | PalwDaUnitV1::PipelineStepLeaf { .. }
                    | PalwDaUnitV1::PipelineStepNode { .. }
                    | PalwDaUnitV1::LegacyHeldV2(_) => false,
                })
                .collect()
        }
        // The second IR fence: an IR step unit, named only — no draws (`open_da_session_tir_step_v1`
        // bounded it by the widest execution; the claim's own bound is the accused's to prove).
        (
            PalwDaUnitV1::TirStepLeaf { .. }
            | PalwDaUnitV1::TirStepNode { .. }
            | PalwDaUnitV1::TirRowNode { .. }
            | PalwDaUnitV1::TirStepRun { .. },
            None,
        ) => Vec::new(),
        // Pipeline-claim data availability: a pipeline step unit, named only — no draws.
        (PalwDaUnitV1::PipelineStepLeaf { .. } | PalwDaUnitV1::PipelineStepNode { .. }, None) => Vec::new(),
        // LG14-B (tag 157): a legacy held unit, named only — no draws. Its bound is the binding tag 157's arm checked
        // (`open_da_session_legacy_held_v2`), and the drawn units' purpose (an answer the producer cannot pick) is the descent's own:
        // the verifier names the next unit from the last answer.
        (PalwDaUnitV1::LegacyHeldV2(_), None) => Vec::new(),
        _ => {
            return Err(PalwStateV2Error::DaAnswerMalformed {
                claim: claim_id,
                why: "an accusation's unit and binding disagree in kind",
            });
        }
    };
    let mut units = Vec::with_capacity(1 + drawn.len());
    units.push(named);
    if !reacquire {
        units.extend(drawn);
    }
    // ADR-0152 §4-ter.3 step 6 (the second review's MEDIUM): the challenger of a live held forfeit on
    // this claim pauses it as a seat does, once a record — counted with the seats' sessions (DL-1 and
    // DA-5 read nothing else), after DA-8 admitted it on the budget its standing gives it.
    // Lane LG14-A: a reserved session (a non-seat whose dispute reservation on the claim is live) is seat-like too, counted on its
    // reservation's budget. Evaluated before the held-forfeit pause so a reserved session never spends that record's one pause; below
    // `palw_legacy_public_filer_v1` no reservation exists and the expression is the one it was.
    let reserved = !admission.accuser_is_seat && builder.state.legacy_dispute_reservation_v1(&claim_id, &accuser).is_some();
    let seat_like =
        admission.accuser_is_seat || reserved || (!claim.phase.is_terminal() && builder.held_forfeit_pauses_v1(claim_id, accuser)?);
    let session = PalwDaSessionV1 {
        opened_daa: ctx.daa_score,
        deadline_daa: admission.deadline_daa,
        accuser_is_seat: seat_like,
        exposure: admission.exposure,
        units,
        stage: admission.stage,
    };
    builder.write_da_session((claim_id, accuser), Some(session))?;
    let mut record = builder.state.da_claims.get(&claim_id).cloned().unwrap_or_default();
    let pauses = if seat_like {
        record.open_seat_sessions = record.open_seat_sessions.saturating_add(1);
        if admission.accuser_is_seat {
            let opened = record.opened_by_seat.entry(accuser).or_insert(0);
            *opened = opened.saturating_add(1);
        } else if reserved {
            // Lane LG14-A: on the reservation's own budget; neither DA-8 count moves.
            builder.legacy_dispute_count_session_v1(claim_id, accuser)?;
        } else {
            // **The fourth review's HIGH: a record-holder's pause is not a seat's session.** It is
            // counted with the seats' OPEN sessions (`open_seat_sessions`, `paused_since`: all DA-5 and
            // DL-1 read), but DA-8's per-seat budget (`opened_by_seat`, at most four) is a seat's
            // alone — admission asks it of a seat and never of anyone else, and the loader refuses a
            // count past four. Counted there, a non-seat that lost five held dissections on one claim
            // drove the fold to a tip its own loader refused on every node: a chain halt for five
            // forfeits. The session was admitted on the non-seat budget, so it is counted on it —
            // the lifetime total (`opened_non_seat_total`, sixteen), exactly as the conversion of a
            // demand that predates its record leaves it (counted there at its admission).
            record.opened_non_seat_total = record.opened_non_seat_total.saturating_add(1);
        }
        record.open_seat_sessions == 1
    } else {
        record.open_other_sessions = record.open_other_sessions.saturating_add(1);
        record.opened_non_seat_total = record.opened_non_seat_total.saturating_add(1);
        false
    };
    if pauses {
        record.paused_since = Some(ctx.daa_score);
    }
    builder.write_da_claim(claim_id, Some(record))?;
    // DA-5 / DL-1: a seat's first open session pauses a live claim, and any session defers a
    // terminal claim's retirement — either way DL-1 gives it no deadline while the session is open.
    // X3 (REQ-X3-5): on a licensed claim accepted past `palw_claim_hard_deadline_v1` any open session holds its Final (DL-1's X3 row).
    if claim.phase.is_terminal()
        || pauses
        || (matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. })
            && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(builder.params, &claim))
    {
        builder.disarm_deadline(claim_id);
    }
    // RFC-0010 × G14 (SHARD): any session on a V3 S2 licence holds it — DL-1 gives it no deadline while one is open (the close of
    // the last re-derives it, `da_close_session_v1`). Dormant: no engine exists on any shipped chain.
    if builder.state.palw_v3_s2_licence_held_v1(&claim_id, &claim)
        || builder.state.palw_v2_expiry_held_v1(builder.params, &claim_id, &claim)
    {
        builder.disarm_deadline(claim_id);
    }
    let until = admission.deadline_daa.saturating_add(builder.params.window_challenge_at(ctx.daa_score));
    builder.da_rekey_v1(claim_id, until, ctx.daa_score)?;
    Ok(())
}

/// **ADR-0152 DA-4 (M3): `MaterialDisclosedV2` — an answer to every open session that demands the
/// unit.**
///
/// The discloser is the claim's producer or a bond with a live lock on the claim (X7; the acceptance
/// layer verified its signature); the unit is demanded by an open session and not yet answered
/// (which bounds the bytes: each distinct unit is answered once); the answer is the unit's own form
/// and is checked by hash arithmetic against the claim's roots — `check_trace_event_disclosure_v1`
/// for an event, the held checkers for a held unit — plus the identity rule (J-5; `IdentityNotRecorded`
/// falls back to the arithmetic alone). A held `StepLeaf`'s evidence is adjudicated: guilty convicts
/// the claim (`act_on_convicted_claim_v1`: voided `CourtFraud` live, the `Final` reversed after), a
/// fused leaf is `DaUnitNeedsDissection`. An accepted answer joins `answered` (a `Flat` answer sets
/// `flat_answered`, covering every in-run row); every session whose units are then all answered is
/// refuted — its exposure held in `refuted_held` (DA-6) — and closes.
pub(super) fn apply_da_answer_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    unit: crate::palw_da_rcore_v1::PalwDaUnitV1,
    answer: &crate::palw_da_rcore_v1::PalwDaAnswerV1,
    discloser: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_da_rcore_v1::{PalwDaAnswerV1, PalwDaUnitV1};
    let (claim, record, in_run) = da_answer_admitted_v1(builder, ctx, claim_id, &unit, discloser)?;
    let now = ctx.daa_score;
    let malformed = |why| PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why };
    // An IR answer's binding is verified at the claim's class's ladder past the second IR fence (the
    // release's 2^22 below it).
    let tir_ladder = builder.tir_da_ladder_v1(&claim.class_id, ctx.daa_score);
    // The answer's form, by the rule node policy builds with (`palw_da_answer_form_v1`; P2-7): the
    // unit's own kind, and a held carriage of version 1 naming this claim and unit, unsigned.
    crate::palw_da_rcore_v1::palw_da_answer_form_v1(&claim_id, &unit, answer).map_err(malformed)?;
    let flat = match (&unit, answer) {
        (PalwDaUnitV1::Event { row, tile }, PalwDaAnswerV1::Event(disclosure)) => {
            crate::palw_step_refute::check_trace_event_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *row,
                *tile,
                disclosure,
                crate::palw_step_leg::PALW_STEP_LEG_MAX_LEAVES,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, &claim_id, disclosure.binding()) {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            matches!(disclosure, crate::palw_step_refute::PalwTraceEventDisclosureV1::Flat { .. })
        }
        // **RFC-0002 Phase F: an IR claim's event**, checked as a legacy event is — hash arithmetic
        // against the claim's roots, then the identity rule over the IR binding. Below
        // `palw_tir_v1` the acceptance layer drops the object by name (an older build cannot decode
        // it); reaching the fold there is refused, the second lock.
        (PalwDaUnitV1::Event { row, tile }, PalwDaAnswerV1::TirEvent(carried)) => {
            if !builder.params.tir_active_at(ctx.daa_score) {
                return Err(malformed("an IR answer before palw_tir_v1 is in force"));
            }
            // The disclosure carries its binding with the program empty; the claim's class's row
            // holds the program, put back before anything reads the binding.
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let disclosure = carried.with_program_v1(record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_trace_event_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *row,
                *tile,
                &disclosure,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) =
                builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, disclosure.binding(), tir_ladder)
            {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            disclosure.is_flat()
        }
        // **The second IR fence: an IR step leaf**, checked by hash arithmetic against the claim's
        // roots (`check_tir_step_leaf_disclosure_v1`: the leaf, its opening, the ids, the logits
        // tile's row pin), then the identity rule over its binding.
        (PalwDaUnitV1::TirStepLeaf { index }, PalwDaAnswerV1::TirStepLeaf(carried)) => {
            if !builder.params.tir_fence2_active_at(ctx.daa_score) {
                return Err(malformed("an IR step-leaf answer before palw_tir_fence2 is in force"));
            }
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let disclosure = carried.with_program_v1(record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_step_leaf_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *index,
                &disclosure,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &disclosure.binding, tir_ladder)
            {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            false
        }
        // **The second IR fence: an interior node of the claim's step tree**, by hash arithmetic: its
        // frontier folds to the node and its siblings walk it to the claim's step root
        // (`check_tir_step_node_disclosure_v1`), then the identity rule over its binding.
        (PalwDaUnitV1::TirStepNode { level, index }, PalwDaAnswerV1::TirStepNode(carried)) => {
            if !builder.params.tir_fence2_active_at(ctx.daa_score) {
                return Err(malformed("an IR step-node answer before palw_tir_fence2 is in force"));
            }
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let disclosure = carried.with_program_v1(record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_step_node_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *level,
                *index,
                &disclosure,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &disclosure.binding, tir_ladder)
            {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            false
        }
        // **The second IR fence: an IR step unit past the claim's execution**, proven by the claim's
        // own binding (`check_tir_step_out_of_range_v1`): the demand was keyed by the claim alone, so
        // its bound is the accused's to show. The session it ends is refuted like any other.
        // **The second IR fence: a node of the claim's tiled trace's rows tree**, by hash arithmetic:
        // it reaches a rows root that, with the carried ids, reproduces the claim's trace root
        // (`check_tir_row_node_disclosure_v1`), then the identity rule over its binding.
        (PalwDaUnitV1::TirRowNode { level, index }, PalwDaAnswerV1::TirRowNode(carried)) => {
            if !builder.params.tir_fence2_active_at(ctx.daa_score) {
                return Err(malformed("an IR rows-tree answer before palw_tir_fence2 is in force"));
            }
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let disclosure = carried.with_program_v1(record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_row_node_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *level,
                *index,
                &disclosure,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &disclosure.binding, tir_ladder)
            {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            false
        }
        // **RFC-0006: a run of step leaves**, by hash arithmetic against the claim's roots: every preimage hashes to its opened
        // leaf and the range opening walks to the claim's step root (`check_tir_step_run_disclosure_v1`), then the identity
        // rule over its binding.
        (PalwDaUnitV1::TirStepRun { first, count }, PalwDaAnswerV1::TirStepRun(carried)) => {
            if !builder.params.tir_shard_active_at(ctx.daa_score) {
                return Err(malformed("an IR step-run answer before palw_tir_shard_v1 is in force"));
            }
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let disclosure = carried.with_program_v1(record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_step_run_disclosure_v1(
                claim.trace_root,
                claim.execution_root,
                *first,
                *count,
                &disclosure,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &disclosure.binding, tir_ladder)
            {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            false
        }
        (
            PalwDaUnitV1::TirStepLeaf { .. }
            | PalwDaUnitV1::TirStepNode { .. }
            | PalwDaUnitV1::TirRowNode { .. }
            | PalwDaUnitV1::TirStepRun { .. },
            PalwDaAnswerV1::TirStepOutOfRange(carried),
        ) => {
            if !builder.params.tir_fence2_active_at(ctx.daa_score) {
                return Err(malformed("an IR out-of-range answer before palw_tir_fence2 is in force"));
            }
            let record = builder
                .state
                .tir_classes
                .get(&claim.class_id)
                .ok_or(PalwStateV2Error::DaAnswerMalformed { claim: claim_id, why: "the claim's class is not an IR program" })?;
            let binding = crate::palw_tir_admission_v1::palw_tir_binding_with_program_v1(carried, record).map_err(malformed)?;
            crate::palw_tir_court_v1::check_tir_step_out_of_range_v1(
                claim.trace_root,
                claim.execution_root,
                &unit,
                &binding,
                tir_ladder,
            )
            .map_err(|e| PalwStateV2Error::DaOpeningRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_tir_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &binding, tir_ladder) {
                return Err(PalwStateV2Error::DaOpeningRefused { claim: claim_id, why });
            }
            false
        }
        (PalwDaUnitV1::Held(missing), PalwDaAnswerV1::Held(carriage)) => {
            let Some(network_ladder) = builder.extras.held_context_ladder else {
                return Err(PalwStateV2Error::HeldContextDormant);
            };
            let ladder = builder.state.class_step_ladder_v1(&claim.class_id, network_ladder);
            crate::palw_held_da_v1::palw_held_da_check_disclosure_v1(
                &claim.execution_root,
                missing,
                &carriage.binding,
                &carriage.disclosure,
                ladder,
                builder.extras.prompt_ids_form_v1(),
            )
            .map_err(|e| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: e.to_string() })?;
            if let Some(why) = builder.da_binding_answers_another_job_v1(ctx.daa_score, &claim_id, &carriage.binding) {
                return Err(PalwStateV2Error::HeldDaRefused { claim: claim_id, why });
            }
            // **LG14-B × ADR-0177 D2 (DA16b §7.5 item 1): past `palw_legacy_held_da_v2` a `StepLeaf` answer owes its CLAIM part only**
            // — the committed half of the leaf's refutation, checked by membership (`check_execution_step_committed_half_v1`), and NO
            // artifact rows: model bytes are never compelled. It is retrieval, not adjudication; the accuser supplies the rows from its
            // own copy in the one-move filing (`ShardCourtAccused`, or tag 159), so every terminal stays reachable and silence is still
            // the producer's default. Below the fence, int-12's adjudicated answer, byte for byte.
            if builder.extras.legacy_held_da_v2_active
                && let crate::palw_held_da_v1::PalwHeldDisclosureV1::StepLeaf { evidence } = &carriage.disclosure
            {
                if !evidence.artifact_openings.is_empty() {
                    return Err(PalwStateV2Error::HeldDaRefused {
                        claim: claim_id,
                        why:
                            "past palw_legacy_held_da_v2 a StepLeaf answer owes no model rows (ADR-0177 D2): the accuser supplies them"
                                .to_string(),
                    });
                }
                crate::palw_step_refute::check_execution_step_committed_half_v1(
                    &evidence.refutation,
                    evidence.prompt_ids_opening.as_ref(),
                    ladder,
                )
                .map_err(|e| PalwStateV2Error::HeldDaRefused { claim: claim_id, why: format!("the leaf's committed half: {e}") })?;
            } else if let crate::palw_held_da_v1::PalwHeldDisclosureV1::StepLeaf { evidence } = &carriage.disclosure {
                // ADR-0111 Decision 4: a leaf's evidence is adjudicated by the one-move verdict.
                let artifact_root =
                    builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
                // t12 (`palw_audit_2026_09_23`): the verdict `MaterialDisclosedHeld` asks, at the same
                // fence — this arm is its successor past `palw_rcore_plus`, and the bound verdict
                // (8be0f661) must not be skipped by the answer route that is live on t12.
                let bound_to = crate::palw_shard_court_v1::PalwOneMoveClaimV2 {
                    execution_root: claim.execution_root,
                    class_id: claim.class_id,
                    artifact_root,
                };
                match evidence.verdict_at_v2(&bound_to, ladder, builder.extras.audit_2026_09_23_active).map_err(|e| {
                    PalwStateV2Error::HeldDaRefused { claim: claim_id, why: format!("the evidence does not adjudicate: {e}") }
                })? {
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::ExecutorGuilty => {
                        // The executor convicted itself by answering — the one-move verdict, a PROVEN
                        // court conviction (S-4): its sessions end with it, it is recorded
                        // `CourtConviction`, and its reward goes to the accuser whose session
                        // demanded the unit first (the smallest `(opened_daa, accuser)`).
                        let challenger = builder
                            .state
                            .da_sessions_of(&claim_id)
                            .filter(|(_, session)| session.units.contains(&unit))
                            .map(|(accuser, session)| (session.opened_daa, *accuser))
                            .min()
                            .map(|(_, accuser)| accuser)
                            .unwrap_or(discloser);
                        return builder.convict_by_court_verdict_v1(ctx, claim_id, &claim, challenger, None);
                    }
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::FalseAccusation => {}
                    crate::palw_shard_court_v1::PalwShardCourtVerdictV1::NeedsDissection => {
                        return Err(PalwStateV2Error::DaUnitNeedsDissection { claim: claim_id, leaf: evidence.leaf_index() });
                    }
                }
            }
            false
        }
        // **Pipeline-claim data availability** (spec 17 §17.14): a pipeline step unit is answered by its
        // disclosure or an out-of-range proof, by hash arithmetic over the claim's committed roots.
        (
            PalwDaUnitV1::PipelineStepLeaf { .. } | PalwDaUnitV1::PipelineStepNode { .. },
            PalwDaAnswerV1::PipelineStepLeaf(_) | PalwDaAnswerV1::PipelineStepNode(_) | PalwDaAnswerV1::PipelineStepOutOfRange(_),
        ) => {
            palw_pipeline_da_fold_v1::check_pipeline_answer_v1(builder, ctx, claim_id, &claim, &unit, answer)?;
            false
        }
        // Refused by the form check above; spelled again so the match stands on its own.
        _ => return Err(malformed("an event unit is answered by an event disclosure, a held unit by a held carriage")),
    };
    da_answer_recorded_v1(builder, claim_id, record, unit, flat, in_run, now)
}

/// **A session opened at `Terminal` on a named leaf** — the body [`open_held_dissection_v1`] and
/// the IR one-move court's dissected leaf (RFC-0002 F7) share: the court's capacity, the ladder
/// narrowed to the leaf under the id a bisection between the same parties over the same space would
/// have had, the accuser's reservation, the claim's path to `Final` frozen. `opening_turn` is the
/// responder's first rung (the root claim).
#[allow(clippy::too_many_arguments)]
pub(super) fn open_dissection_at_named_leaf_v1(
    builder: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim_id: Hash64,
    claim: &PalwClaimStateV2,
    challenger_bond: PalwBondKeyV2,
    leaf_index: u64,
    ladder: u64,
    opening_turn: u64,
    published: crate::palw_court_model_scope_routes_v1::PalwD2OpeningOperandsV1<'_>,
) -> Result<Option<Hash64>, PalwStateV2Error> {
    let refused = |why: String| PalwStateV2Error::HeldDissectionRefused { claim: claim_id, leaf: leaf_index, why };
    // **D2 (REQ-T6-3 restricted, REQ-T6-4 replaced; `palw_court_model_scope_v1`)**: every dissection opened at a named leaf is
    // answered by the producer's root claim, whose object carries model operands (`AttnRootClaimOperands`: an attention site's
    // quantization rows; `TirGenRootClaimParams`: the parameter leaves an IR / generative tile reads). On a claim accepted past the
    // fence the accused is never compelled to supply them from its own copy: the dissection opens only if the object opening it
    // PUBLISHES them (the challenger's own copy, authenticated against the registered root; for attention, deriving the site), and
    // the accused's root claim — which signs coordinates and claim values only — relays those public bytes
    // (`palw_court_model_scope_routes_v1`). Refused BEFORE any write. A claim accepted below the fence keeps the OLD rule:
    // `published` is not read.
    if crate::palw_ruling_fences_v1::palw_court_model_scope_governs_claim_v1(builder.params, claim) {
        let class_root =
            builder.state.classes.get(&claim.class_id).ok_or(PalwStateV2Error::MissingClass(claim.class_id))?.artifact_root;
        let proven_root = builder.state.claim_roots.get(&claim_id).copied().unwrap_or(class_root);
        let gen_root = builder.state.gen_classes.get(&claim.class_id).map(|row| row.artifact_root);
        let opening_cap = crate::palw_court_v2::palw_attn_opening_cap_v1(
            &builder.state,
            &claim.class_id,
            builder.extras.held_context_ladder.unwrap_or(crate::palw_step::PALW_STEP_MAX_LEAVES),
            builder.extras.held_context_ladder.is_some(),
        );
        crate::palw_court_model_scope_routes_v1::palw_d2_named_leaf_operands_admit_v1(
            claim,
            leaf_index,
            &published,
            proven_root,
            gen_root,
            opening_cap,
        )
        .map_err(|e| refused(e.to_string()))?;
    }
    // **X3 (REQ-X3-4, `palw_claim_hard_deadline_v1`)**: a court pursuit on a claim accepted past the fence starts only at or before
    // the claim's open cutoff and only if `start + B_remaining <= hard`. `Ok` below the fence.
    crate::palw_ruling_fences_v1::palw_claim_pursuit_may_start_v1(builder.params, claim, ctx.daa_score)
        .map_err(|why| refused(why.to_string()))?;
    // Lane g14-court-capacity (audit G-07): past `palw_court_capacity_v1` court admission (the challenger's reservation, the escrow,
    // the deadline-aware queue) replaces the fixed concurrency cap — a queued prosecution opens nothing now (`Ok(None)`); below the
    // fence, the cap as it always was.
    let admission_window =
        builder.state.class_model_court_window_v1(&claim.class_id).unwrap_or_else(|| builder.params.window_court());
    match super::palw_court_capacity_fold_v1::court_admission_v1(builder, ctx, claim_id, challenger_bond, admission_window)? {
        super::palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Queued => return Ok(None),
        super::palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Open => {}
        super::palw_court_capacity_fold_v1::PalwCourtAdmissionV1::Legacy => {
            let open_sessions = builder.state.court_sessions.len() as u64;
            let max_sessions = palw_max_concurrent_court_sessions_v1(builder.params.turn_deadline_daa());
            if open_sessions >= max_sessions {
                return Err(PalwStateV2Error::CourtSessionsAtCapacity {
                    open: open_sessions,
                    max: max_sessions,
                    turn_deadline_daa: builder.params.turn_deadline_daa(),
                    per_block: PALW_COURT_CLOSE_MAX_PER_BLOCK as u64,
                });
            }
        }
    }
    let challenger_collateral =
        builder.state.bonds.get(&challenger_bond).ok_or(PalwStateV2Error::MissingBond(challenger_bond))?.collateral;
    // **The class's own window, if it committed one** (`palw_model_court_window`): only a class registered past
    // that fence has a row, so every other session — all of them below the fence — takes the window and the
    // retention rule (none, at this opening) exactly as the release before this one did.
    let model_window = builder.state.class_model_court_window_v1(&claim.class_id);
    let session_window = model_window.unwrap_or_else(|| builder.params.window_court());
    let deadline_daa = ctx.daa_score.checked_add(session_window).ok_or(PalwStateV2Error::Overflow("court deadline"))?;
    palw_exec_v2_rewards_v1::require_native_session_window(&builder.state, builder.params, &claim_id, deadline_daa)?;
    if model_window.is_some() {
        // A model window may outrun the retention obligation the claim was accepted with (the obligation is
        // sized by the network window): the opening must lie inside retention, and the claim's retention is
        // extended to the session's deadline, so the producer owes the trace for as long as the court runs.
        if ctx.daa_score > claim.trace_retention_daa {
            return Err(PalwStateV2Error::CourtOutsideRetention {
                claim: claim_id,
                at: ctx.daa_score,
                window: session_window,
                retention_daa: claim.trace_retention_daa,
            });
        }
        // X3 (REQ-X3-3): a claim accepted past `palw_claim_hard_deadline_v1` owes its retention as fixed at acceptance — a model
        // window that outruns it is not opened rather than extending it (the extension stays the OLD rule below the fence).
        if deadline_daa > claim.trace_retention_daa && crate::palw_ruling_fences_v1::palw_claim_x3_governs_v1(builder.params, claim) {
            return Err(refused(
                "X3: the model court window outruns the claim's retention fixed at acceptance (palw_claim_hard_deadline_v1)".into(),
            ));
        }
        if deadline_daa > claim.trace_retention_daa {
            let mut extended_claim = claim.clone();
            extended_claim.trace_retention_daa = deadline_daa;
            builder.write_claim(claim_id, Some(extended_claim))?;
        }
    }
    let first_deadline_daa =
        ctx.daa_score.checked_add(opening_turn).ok_or(PalwStateV2Error::Overflow("court opening rung deadline"))?.min(deadline_daa);
    // The space a bisection would have declared: the ruleset's own leaf cap — the ladder the
    // one-move court adjudicated the accusation at — never a count the accuser chose.
    let space_size = ladder;
    let ladder = crate::palw_bisect::PalwBisectLadderV1::open_at_named_leaf(
        &claim_id,
        &claim.trace_root,
        &crate::palw_court_v2::court_party_id_v2(&challenger_bond),
        &crate::palw_court_v2::court_party_id_v2(&claim.bond),
        crate::palw_bisect::PalwBisectSpaceV1::StepLeaves,
        space_size,
        leaf_index,
        ctx.daa_score,
        first_deadline_daa,
    )
    .map_err(|e| refused(e.to_string()))?;
    let session_id = ladder.session_id();
    if builder.state.court_sessions.contains_key(&session_id) {
        return Err(PalwStateV2Error::DuplicateSession(session_id));
    }
    // **ADR-0152 §4-ter C4: the reservation that makes the charge certain.** Past
    // `palw_offence_attribution` with R-core+'s one ledger in force, the opening asks S's one accuser
    // gate (`gate_room(.., Accuser)`, [`palw_rcore_gate_room_of_v1`]: `max(committed, 500‰·C) +
    // accuser + new ≤ C`) for the charge a losing challenger will pay, `min(max(reserved, G),
    // floor)`; the gate's accuser ledger already counts this bond's other open held dissections at
    // their own charge ([`PalwFoldReadV1::accuser_ledger_v1`], the review's F7). Below either fence
    // the reservation is the one it always was.
    if builder.extras.offence_attribution_active && builder.params.rcore_plus_active_at(ctx.daa_score) {
        let floor = u128::from(builder.params.min_collateral_sompi());
        let charge = builder.held_dissection_charge_v1(&claim_id, claim).min(floor);
        let room = builder.gate_room(&challenger_bond, ctx.daa_score, PalwRcoreGateV1::Accuser);
        if charge > room {
            let ceiling = u128::from(challenger_collateral);
            return Err(PalwStateV2Error::AccusationExposureCeiling {
                bond: challenger_bond,
                edge: "held dissection",
                backed: ceiling.saturating_sub(room),
                accusation: charge,
                ceiling,
            });
        }
    } else {
        builder.reserve_challenger_v1(challenger_bond, challenger_collateral, claim.reserved, "held dissection", ctx.daa_score)?;
    }
    let mut opened = PalwCourtSessionStateV2 {
        claim: claim_id,
        challenger_bond,
        opened_daa: ctx.daa_score,
        deadline_daa,
        ladder,
        dissection: None,
    };
    cap_session_rung_deadline_v2(&mut opened, builder.params);
    builder.write_court(session_id, Some(opened))?;
    super::palw_court_capacity_fold_v1::court_opened_v1(builder, ctx, claim_id, challenger_bond, session_id)?;
    if matches!(claim.phase, PalwClaimPhaseV2::ReceiptLicensed { .. }) {
        builder.disarm_deadline(claim_id);
    }
    // The session's id, for a caller that keys something to it (the held leaf challenge's close group); `None` when the prosecution
    // was queued past `palw_court_capacity_v1`.
    Ok(Some(session_id))
}
