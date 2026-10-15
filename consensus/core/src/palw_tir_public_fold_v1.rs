//! Prospective original TIR admission and native sidecar transitions. This is a
//! child of `palw_state_v2`: only its native transaction dispatcher may call it.
//! Quotes, local Ready files and signature validation alone never buy standing.
use super::*;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_public_claim_v1::{PalwTirPublicClaimV1 as Frame, palw_tir_public_claim_v1};
use crate::palw_tir_public_owned_v1::{
    PalwTirPublicContractV1, PalwTirPublicOwnedActionV1 as Action, PalwTirPublicOwnedEffectV1 as Effect, PalwTirPublicOwnedV1,
    PalwTirPublicPreparationContractV1, PalwTirPublicPreparationV1,
};
use crate::palw_tir_public_state_v1::{PalwTirPublicAcquisitionRowV1, PalwTirPublicPreparationRowV1};

fn refused(why: impl Into<String>) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}
fn rules(b: &TransitionBuilder<'_>) -> Result<crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1, PalwStateV2Error> {
    b.extras.tir_public_allocation_rules_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)
}
fn source(
    b: &TransitionBuilder<'_>,
    frame: &Frame,
) -> Result<crate::palw_tir_public_source_v1::PalwTirPublicSourceV1, PalwStateV2Error> {
    let record = b.state.tir_class_v1(&frame.original_v1().job.class_id).ok_or_else(|| refused("TIR original class is absent"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, record.facts.class_id)
        .ok_or_else(|| refused("TIR original registered layout is absent"))?;
    frame.source_v1(record, layout, frame.descriptor_v1().maximum_leaves).map_err(refused)
}
fn native_frame(b: &TransitionBuilder<'_>, claim: Hash64, allocation: Hash64, source_id: Hash64) -> Result<Frame, PalwStateV2Error> {
    rules(b)?;
    let frame = palw_tir_public_claim_v1(&b.state, claim).ok_or_else(|| refused("TIR control has no paid original frame"))?;
    if frame.plan_v1().allocation_id != allocation || frame.plan_v1().source_id != source_id {
        return Err(refused("TIR control substitutes original allocation or source"));
    }
    frame.check_structure_v1(&frame.key_v1()).map_err(refused)?;
    Ok(frame.clone())
}
fn independent(b: &TransitionBuilder<'_>, frame: &Frame, actor: PalwBondKeyV2) -> Result<(), PalwStateV2Error> {
    let record = b.state.bond(&actor).ok_or(PalwStateV2Error::MissingBond(actor))?;
    if actor == frame.custody_v1().pledge_v1().producer
        || frame.custody_v1().drawn_panel_v1().contains(&actor)
        || !matches!(record.status, PalwBondStatusV2::Active)
        || record.collateral < b.params.min_collateral_sompi()
    {
        return Err(refused("TIR owner/collector is not an independent active above-floor bond"));
    }
    Ok(())
}

/// All inputs are transaction-local original material or resolved native state.
/// The ordinary FP signature/price/admission path must also accept that SAME
/// original object. No row is written by this preparation step.
pub(super) fn prepare_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    pledge: &crate::palw_tir_public_custody_v1::PalwTirPublicCustodyPledgeV1,
) -> Result<Frame, PalwStateV2Error> {
    use crate::palw_tir_public_allocation_v1::{PalwTirPublicAllocationQuoteV1, derive_tir_public_allocation_v1};
    let mut native = rules(b)?;
    native.maximum_leaves = b.state.class_step_ladder_v1(&original.job.class_id, native.maximum_leaves);
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(original);
    if b.state.claim(&claim).is_some() || b.state.staged_court_row_v1(&Key::TirClaim(claim)).is_some() {
        return Err(refused("TIR original allocation cannot be attached to an existing claim"));
    }
    let upper = b.state.staged_maximum_custodians_v1(original.job.class_id, ctx.daa_score, b.params, b.extras)?;
    if native.maximum_custodians != upper {
        return Err(refused("TIR original quote does not use the actual native custodian upper"));
    }
    // Upper is rederived here even though extras were resolved by native policy.
    native.maximum_custodians = upper;
    let geometry = crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1 {
        class_id: original.job.class_id,
        producer: PalwBondKeyV2(original.job.executor_bond),
        execution_root: original.execution_root,
        job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original),
        accepted_daa: ctx.daa_score,
    };
    let record = b.state.tir_class_v1(&geometry.class_id).ok_or_else(|| refused("TIR original class is not registered"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, geometry.class_id)
        .ok_or_else(|| refused("TIR original layout is not registered"))?;
    let semantics = b.state.class_semantics.get(&geometry.class_id);
    let inventory = semantics.and_then(|s| b.state.staged_tokenizer_inventory_v1(s.tokenizer_id));
    let plan = match derive_tir_public_allocation_v1(&geometry, original, header, record, layout, semantics, inventory, native)
        .map_err(refused)?
    {
        PalwTirPublicAllocationQuoteV1::Complete(plan) => *plan,
        PalwTirPublicAllocationQuoteV1::Incomplete { missing, .. } => {
            return Err(refused(format!("TIR original full allocation remains incomplete: {missing:?}")));
        }
    };
    let original_source = crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_v1(
        record,
        layout,
        header,
        original.execution_root,
        original.trace_root,
        native.maximum_leaves,
    )
    .map_err(refused)?;
    let producer = b.state.bond(&geometry.producer).ok_or(PalwStateV2Error::MissingBond(geometry.producer))?;
    let panel: BTreeSet<_> = b.state.panel(&claim).map(|p| p.seats.iter().map(|s| s.bond).collect()).unwrap_or_default();
    let custody = crate::palw_tir_public_custody_v1::PalwTirPublicCustodyContractV1::new_v1(
        pledge.clone(),
        plan.terms,
        panel.clone(),
        original_source.source_id_v1(),
        plan.resource_plan_id,
        upper,
        &producer.pubkey,
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    )
    .map_err(refused)?;
    Frame::prepare_native_v1(
        original.clone(),
        header.clone(),
        plan,
        custody,
        &geometry,
        record,
        layout,
        semantics,
        inventory,
        native,
        &panel,
        object,
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    )
    .map_err(refused)
}

/// Call only after the same ordinary FP object has passed native validation and
/// its normal reservation has been made. Its normative transaction minimum fee
/// MUST include `frame.plan.terms.plan.rent_sompi` before this fold can run.
/// This function checks the additional full deposit on the ONE native ledger;
/// the frame's separate producer exposure/exit index retains it thereafter.
pub(super) fn install_original_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: Frame,
) -> Result<(), PalwStateV2Error> {
    rules(b)?;
    frame.check_structure_v1(&frame.key_v1()).map_err(refused)?;
    let p = frame.plan_v1();
    let c = b.state.claim(&p.original_claim).ok_or(PalwStateV2Error::MissingClaim(p.original_claim))?;
    let producer = frame.custody_v1().pledge_v1().producer;
    if c.accepted_daa != ctx.daa_score
        || p.terms.accepted_daa != ctx.daa_score
        || c.bond != producer
        || c.execution_root != frame.original_v1().execution_root
        || b.state.staged_court_row_v1(&frame.key_v1()).is_some()
        || b.state.staged_court_row_v1(&frame.preparation_key_v1()).is_some()
    {
        return Err(refused("TIR original install lacks same-block native FP acceptance or repeats an allocation"));
    }
    let room = b.gate_room(&producer, ctx.daa_score, PalwRcoreGateV1::Work);
    if u128::from(p.terms.plan.deposit_sompi) > room {
        return Err(refused("TIR original full deposit exceeds producer native work room"));
    }
    let original_source = source(b, &frame)?;
    let preparation = PalwTirPublicPreparationV1::new_v1(
        PalwTirPublicPreparationContractV1 {
            claim: p.original_claim,
            allocation_id: p.allocation_id,
            source_id: p.source_id,
            original_accepted_daa: p.terms.accepted_daa,
            preparation_until_daa: p.preparation_until_daa,
            collect_until_daa: p.terms.collect_until_daa,
            material_until_daa: p.terms.material_until_daa,
            producer,
            custodians: frame.custody_v1().liable_signers_v1(),
            drawn_panel: frame.custody_v1().drawn_panel_v1().clone(),
            plan: p.preparation.clone(),
        },
        &original_source,
        p.preparation_work.clone(),
        &p.acquisition,
    )
    .map_err(refused)?;
    let prep = PalwTirPublicPreparationRowV1 {
        descriptor: frame.descriptor_v1().clone(),
        allocation_id: p.allocation_id,
        preparation: Box::new(preparation).into(),
    };
    prep.check_structure_v1(&frame.preparation_key_v1()).map_err(refused)?;
    // The native dispatcher atomically owns the ordinary FP writes and these two
    // rows. Neither a separately submitted quote nor a historical FP can enter.
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(Box::new(prep).into())));
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())));
    Ok(())
}

pub(super) fn bind_panel_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, claim: Hash64) -> Result<(), PalwStateV2Error> {
    let Some(mut frame) = palw_tir_public_claim_v1(&b.state, claim).cloned() else { return Ok(()) };
    frame.bind_original_panel_native_v1(&b.state, ctx.daa_score).map_err(refused)?;
    let Some(Row::TirPreparation(old)) = b.state.staged_court_row_v1(&frame.preparation_key_v1()) else {
        return Err(refused("TIR actual Panel binding lost original preparation"));
    };
    let mut prep = old.clone();
    prep.preparation.bind_native_panel_v1(frame.custody_v1().drawn_panel_v1()).map_err(refused)?;
    prep.check_structure_v1(&frame.preparation_key_v1()).map_err(refused)?;
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(prep)));
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())));
    Ok(())
}

/// Each outside owner pays this exact native component afresh. The immutable
/// absolute doors stay those of the ORIGINAL claim; no Open restarts its clock.
pub(super) fn owner_terms_v1(
    frame: &Frame,
) -> Result<crate::palw_rendering_resources_v1::PalwRenderingClaimTermsV1, PalwStateV2Error> {
    frame.plan_v1().owner_terms_v1().map_err(refused)
}
fn owner_contract(frame: &Frame, owner: PalwBondKeyV2, reserved: u64) -> Result<PalwTirPublicContractV1, PalwStateV2Error> {
    Ok(PalwTirPublicContractV1 {
        claim: frame.plan_v1().original_claim,
        owner,
        reserved_daa: reserved,
        bundle_id: frame.plan_v1().allocation_id,
        source_id: frame.plan_v1().source_id,
        terms: owner_terms_v1(frame)?,
        plan: frame.plan_v1().acquisition.clone(),
        producer: frame.custody_v1().pledge_v1().producer,
        custodians: frame.custody_v1().liable_signers_v1(),
    })
}
fn owner_key(frame: &Frame, owner: PalwBondKeyV2, reserved: u64) -> Result<Key, PalwStateV2Error> {
    let contract = owner_contract(frame, owner, reserved)?;
    Ok(Key::TirAcquisition(contract.claim, owner, frame.descriptor_v1().allocation_id_v1(&contract).map_err(refused)?))
}

fn open_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
    owner: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    independent(b, frame, owner)?;
    let c = owner_contract(frame, owner, ctx.daa_score)?;
    let key = owner_key(frame, owner, ctx.daa_score)?;
    let allowance = rules(b)?.config.maximum_owned_rows_per_reserver;
    if b.state.staged_court_row_v1(&key).is_some()
        || b.state.staged_owner_allowance_used_v1(owner, usize::try_from(allowance).unwrap_or(usize::MAX)) >= allowance as usize
    {
        return Err(refused("TIR outside owner already opened this row or exhausts its own native row allowance"));
    }
    b.check_accuser_room(owner, u128::from(c.terms.plan.deposit_sompi), "TIR public owner", ctx.daa_score)?;
    let funded =
        c.terms.plan.rent_sompi.checked_add(c.terms.plan.deposit_sompi).ok_or(PalwStateV2Error::Overflow("TIR owner funding"))?;
    let original_source = source(b, frame)?;
    let Some(Row::TirPreparation(prep)) = b.state.staged_court_row_v1(&frame.preparation_key_v1()) else {
        return Err(refused("TIR Open lost original shared preparation"));
    };
    let mut owned = PalwTirPublicOwnedV1::admit_native_v1(
        c,
        &original_source,
        frame.custody_v1().drawn_panel_v1(),
        frame.descriptor_v1().native_config,
        frame.descriptor_v1().native_custodian_upper,
        ctx.daa_score,
        funded,
    )
    .map_err(refused)?;
    owned.install_work_v1(&original_source, frame.plan_v1().owner_work.clone()).map_err(refused)?;
    owned.bind_preparation_v1(&original_source, &prep.preparation).map_err(refused)?;
    if owned.response_custodians_v1().is_empty() {
        return Err(refused("TIR Open has no original Ready source; collect preparation defaults"));
    }
    let row = PalwTirPublicAcquisitionRowV1 { descriptor: frame.descriptor_v1().clone(), owned: Box::new(owned).into() };
    row.check_structure_v1(&key).map_err(refused)?;
    // Ordinary carrier fee validation must burn owner_terms.rent, independently
    // of the producer's original full payment. The indexed row occupies only
    // this owner's additional deposit; it cannot spend the original prep pool.
    b.write_staged_row_v1(key, Some(Row::TirAcquisition(Box::new(row).into())));
    Ok(())
}

/// The outer native dispatcher supplies the network's real signature domain.
/// No network/domain or unsigned source metadata is accepted from the action.
pub(super) fn apply_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    control: &crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1,
) -> Result<(), PalwStateV2Error> {
    let saved = b.checkpoint();
    let result = apply_control_inner_v1(b, ctx, network_domain, control);
    if result.is_err() {
        b.restore(saved);
    }
    result
}
fn apply_control_inner_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    control: &crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T};
    let frame = native_frame(b, control.claim, control.allocation_id, control.source_id)?;
    control.validate_native_signature_v1(network_domain, &b.state).map_err(refused)?;
    let maximum_leaves = frame.descriptor_v1().maximum_leaves;
    let transition = match (&control.target, &control.action) {
        (T::OwnerOpen { owner }, A::Open) => return open_v1(b, ctx, &frame, *owner),
        (T::Preparation { .. }, A::Preparation(action)) => {
            let Some(Row::TirPreparation(prep)) = b.state.staged_court_row_v1(&frame.preparation_key_v1()) else {
                return Err(refused("TIR preparation control has no original funded row"));
            };
            control.check_preparation_scope_v1(&prep.preparation).map_err(refused)?;
            if matches!(action, Action::PreparationCollect { .. }) {
                independent(b, &frame, control.signer)?;
            }
            // A nonproducer may only acquire Ready atomically with its own
            // ordinary Valid credit and separate RawValidGrant, never here.
            if matches!(action, Action::PreparationReady { .. }) && control.signer != frame.custody_v1().pledge_v1().producer {
                return Err(refused("TIR seat Ready requires atomic ordinary Valid+raw grant"));
            }
            crate::palw_tir_public_state_v1::palw_tir_public_preparation_transition_v1(
                &b.state,
                frame.preparation_key_v1(),
                control.source_id,
                control.expected_turn,
                control.signer,
                ctx.daa_score,
                action,
                maximum_leaves,
            )
            .map_err(refused)?
        }
        (T::Acquisition { owner, reserved_daa }, A::Acquisition(action)) => {
            let key = owner_key(&frame, *owner, *reserved_daa)?;
            let Some(Row::TirAcquisition(row)) = b.state.staged_court_row_v1(&key) else {
                return Err(refused("TIR control has no separately paid owner acquisition"));
            };
            control.check_owned_scope_v1(&row.owned).map_err(refused)?;
            if matches!(action, Action::Demand { .. } | Action::Reacquire { .. } | Action::Collect { .. }) {
                independent(b, &frame, control.signer)?;
            }
            crate::palw_tir_public_state_v1::palw_tir_public_transition_v1(
                &b.state,
                key,
                control.source_id,
                control.expected_turn,
                control.signer,
                ctx.daa_score,
                action,
                maximum_leaves,
                None,
            )
            .map_err(refused)?
        }
        _ => return Err(refused("TIR control substitutes preparation/open/owner namespaces")),
    };
    // Pure effects are not sanctions. Only the native literal default after its
    // original clock reaches the established economic policy below.
    let unpaid = match &transition.effect {
        Effect::Default(default) => Some(default.missing_custodians.clone()),
        Effect::PreparationDefault(default) => Some(std::iter::once(default.actor).collect()),
        _ => None,
    };
    if let Some(unpaid) = &unpaid {
        let liable = frame.custody_v1().liable_signers_v1();
        if unpaid.is_empty() || !unpaid.is_subset(&liable) {
            return Err(refused("TIR default names no actual original signed custody liability"));
        }
        // A qualified seat must still have its real original S4 lock. The
        // economic helper intentionally skips a consumed signer; an absent
        // unconsumed lock is NOT successful collection of a fictitious debt.
        for seat in unpaid.iter().filter(|seat| **seat != frame.custody_v1().pledge_v1().producer) {
            let offence = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &control.claim);
            if !b.state.consumed_offences.contains_key(&offence) && !b.state.slashable_locks.contains_key(&(*seat, control.claim)) {
                return Err(refused("TIR qualified default lost its unconsumed native S4 lock"));
            }
        }
    }
    for change in transition.changes_v1() {
        if b.state.staged_court_row_v1(&change.key) != change.old.as_ref() {
            return Err(refused("TIR transition changes a noncurrent native row"));
        }
    }
    for change in transition.changes_v1() {
        b.write_staged_row_v1(change.key, change.new.clone());
    }
    if let Some(unpaid) = unpaid {
        let claim = b.state.claim(&control.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(control.claim))?;
        b.staged_selected_default_charge_v1(
            ctx,
            control.claim,
            &claim,
            frame.custody_v1().pledge_v1().producer,
            &unpaid,
            control.signer,
        )?;
    }
    Ok(())
}

/// Ordinary Valid validation, backing/credit/S4, new grant and Ready all commit
/// in ONE native object. Any failure rolls every intermediate write back, not
/// just the new frame. No other object may pre-credit this raw promise.
pub(super) fn apply_grant_ready_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    movement: &crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1,
) -> Result<(), PalwStateV2Error> {
    let saved = b.checkpoint();
    let result = apply_grant_ready_inner_v1(b, ctx, network_domain, movement);
    if result.is_err() {
        b.restore(saved);
    }
    result
}
fn apply_grant_ready_inner_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    network_domain: Hash64,
    movement: &crate::palw_tir_public_carriage_v1::PalwTirPublicGrantReadyV1,
) -> Result<(), PalwStateV2Error> {
    let mut frame = native_frame(b, movement.claim, movement.allocation_id, movement.source_id)?;
    movement.validate_native_signature_v1(network_domain, &b.state).map_err(refused)?;
    if b.state.panel_duties_of(&movement.claim).and_then(|d| d.get(&movement.signer)).copied() != Some(0) {
        return Err(refused("TIR raw grant must carry the literal newly credited Valid in this same atomic object"));
    }
    crate::palw_panel_v2::validate_public_custody_receipt_v1(
        &b.state,
        b.params,
        ctx,
        network_domain,
        &movement.receipt,
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1,
    )
    .map_err(|e| refused(format!("TIR ordinary Valid receipt refused: {e:?}")))?;
    let original_source = source(b, &frame)?;
    let Some(Row::TirPreparation(old)) = b.state.staged_court_row_v1(&frame.preparation_key_v1()) else {
        return Err(refused("TIR atomic grant lost original preparation"));
    };
    let mut prep = old.clone();
    prep.preparation.qualify_ready_v1(&original_source, movement.signer, ctx.daa_score, movement.descriptor).map_err(refused)?;
    let claim = b.state.claim(&movement.claim).cloned().ok_or(PalwStateV2Error::MissingClaim(movement.claim))?;
    // Existing monetary formula, native duty/window checks and counted-seat lock
    // funnel. On PanelBound this acknowledgement does NOT license the claim.
    b.credit_supplementary_receipts(movement.claim, &claim, std::slice::from_ref(&movement.receipt), ctx)?;
    let key = (movement.signer, movement.claim);
    let mut lock =
        b.state.slashable_locks.get(&key).copied().ok_or_else(|| refused("TIR ordinary Valid created no native S4 lock"))?;
    if lock.amount == 0 {
        return Err(refused("TIR ordinary Valid has no slashable native collateral"));
    }
    // Q4: never reprice the lock. Its original exact raw promise retains the
    // native amount through the original material/collection horizon.
    lock.expiry_daa = lock.expiry_daa.max(frame.plan_v1().terms.bond_exit_after_daa);
    b.write_slashable_lock(key, Some(lock));
    frame
        .qualify_original_native_v1(
            &b.state,
            &original_source,
            &prep.preparation,
            &movement.receipt,
            &movement.grant,
            ctx.daa_score,
            network_domain,
        )
        .map_err(refused)?;
    prep.check_structure_v1(&frame.preparation_key_v1()).map_err(refused)?;
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(prep)));
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())));
    Ok(())
}

/// Old licensing paths must not bypass the atomic new custody door. A Panel
/// signature alone is never a raw-source grant. Historic claims have no frame
/// and keep their previous behavior byte-for-byte.
pub(super) fn require_qualified_receipts_v1(
    state: &PalwChainStateV2,
    claim: Hash64,
    receipts: &[crate::palw_panel_v2::PalwSeatReceiptV2],
) -> Result<(), PalwStateV2Error> {
    let Some(frame) = palw_tir_public_claim_v1(state, claim) else { return Ok(()) };
    for receipt in receipts.iter().filter(|r| r.verdict == crate::palw_panel_v2::PalwReceiptVerdictV2::Valid) {
        if frame.original_custody_v1().credit_v1(&receipt.seat_bond).is_none_or(|c| c.receipt != *receipt) {
            return Err(refused("TIR licensing Valid lacks this exact native atomic RawValidGrant+Ready credit"));
        }
    }
    Ok(())
}
pub(super) fn require_qualified_typed_receipts_v1(state: &PalwChainStateV2, claim: Hash64) -> Result<(), PalwStateV2Error> {
    if palw_tir_public_claim_v1(state, claim).is_some() {
        // Mandatory remaining typed V3/V4 integration, not a conversion of
        // segment/shard signatures into a whole-job V2 grant or completion.
        return Err(refused("TIR typed segmented/shard raw grants are not yet wired; no V2 signature reinterpretation"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::from_u64_word(n), 0))
    }
    #[test]
    fn owner_fee_and_deposit_are_a_separate_component_without_new_absolute_clocks() {
        // This synthetic-signature candidate is geometry only, NOT paid native
        // admission, receipt credit or evidence of a successful network run.
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_fixture_v1();
        let terms = owner_terms_v1(&frame).unwrap();
        assert_eq!(terms, frame.plan_v1().owner_terms_v1().unwrap());
        let mut absolute = terms;
        absolute.plan = frame.plan_v1().terms.plan;
        assert_eq!(absolute, frame.plan_v1().terms);
        assert!(terms.plan.covers_v1(frame.plan_v1().acquisition.envelope));
        assert!(terms.plan.covers_v1(frame.plan_v1().owner_work.envelope));
        assert!(terms.plan.rent_sompi > 0 && terms.plan.deposit_sompi > 0);
        let owner = bond(900);
        let at = frame.plan_v1().preparation_until_daa;
        assert_ne!(owner_key(&frame, owner, at).unwrap(), owner_key(&frame, owner, at + 1).unwrap());
        assert_ne!(owner_key(&frame, owner, at).unwrap(), owner_key(&frame, bond(901), at).unwrap());
    }
    #[test]
    fn new_original_frame_cannot_license_an_unqualified_valid_or_reinterpret_typed_receipt() {
        let frame = crate::palw_tir_public_claim_v1::palw_tir_public_claim_fixture_v1();
        let claim = frame.plan_v1().original_claim;
        let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
            claim,
            verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
            seat_bond: bond(900),
            signed_daa: frame.plan_v1().terms.accepted_daa,
            signature: Vec::new(),
        };
        let mut state = PalwChainStateV2::genesis();
        assert!(
            require_qualified_receipts_v1(&state, claim, std::slice::from_ref(&receipt)).is_ok(),
            "old path has no additional rule"
        );
        state.staged_courts.insert(frame.key_v1(), Row::TirClaim(Box::new(frame).into()));
        assert!(require_qualified_receipts_v1(&state, claim, &[receipt]).is_err());
        assert!(require_qualified_typed_receipts_v1(&state, claim).is_err());
    }
}
