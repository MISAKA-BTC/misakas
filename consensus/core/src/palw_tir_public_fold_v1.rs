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

fn paired_claim(
    object: &PalwConsensusObjectV2,
) -> Result<(Hash64, &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3), PalwStateV2Error> {
    let PalwConsensusObjectV2::FreePromptCommitted { claim, public_job: Some(original), eval: None, .. } = object else {
        return Err(refused("TFP1 must be paired with the same ordinary public FP object, not evaluation or missing job"));
    };
    Ok((*claim, original))
}

pub(super) fn check_paired_original_v1(b: &TransitionBuilder<'_>, object: &PalwConsensusObjectV2) -> Result<(), PalwStateV2Error> {
    rules(b)?;
    b.extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    let (claim, original) = paired_claim(object)?;
    let envelope =
        b.extras.tir_public_originals.get(&claim).ok_or_else(|| refused("TFP1 has no transaction-local original pairing"))?;
    envelope.matches_object_v1(object).map_err(refused)?;
    if envelope.original_fp_v1().map_err(refused)?.commitment != *original {
        return Err(refused("TFP1 original bytes differ from the actual accepted public FP commitment"));
    }
    Ok(())
}

pub(super) fn prepare_paired_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
    public_job: Option<&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
) -> Result<Option<Frame>, PalwStateV2Error> {
    let PalwConsensusObjectV2::FreePromptCommitted { claim, .. } = object else { return Ok(None) };
    let Some(envelope) = b.extras.tir_public_originals.get(claim) else { return Ok(None) };
    check_paired_original_v1(b, object)?;
    let original = envelope.original_fp_v1().map_err(refused)?.commitment;
    if public_job != Some(&original) {
        return Err(refused("TFP1 cannot install a frame without this object's exact public job"));
    }
    let frame = prepare_original_v1(b, ctx, object, &original, &envelope.header, &envelope.pledge)?;
    if envelope.declared_rent_sompi != frame.plan_v1().terms.plan.rent_sompi {
        return Err(refused("TFP1 declared burn differs from the exact original native full-SUM rent"));
    }
    Ok(Some(frame))
}

/// Native price derivation on the actual pre-object state. The object must be
/// the ordinary extractor's projection of these SAME original bytes; callers
/// must not duplicate its version/consumed-prefix-state conversion. This reads
/// state only: no reservation, payment, receipt credit or admission marker is
/// produced, and all ordinary FP validation is still mandatory at acceptance.
/// The native caller supplies block context/extras, not a wire-advertised cap.
pub fn palw_tir_public_preclaim_quote_v1(
    base: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    ctx: &PalwBlockContextV2,
    extras: &PalwTransitionExtrasV1,
    object: &PalwConsensusObjectV2,
    envelope: &crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1,
) -> Result<crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1, PalwStateV2Error> {
    let builder = TransitionBuilder::try_new(base, params, false, false, false, false, extras)?;
    rules(&builder)?;
    extras.tir_public_network_domain_v1.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    let (_, original) = paired_claim(object)?;
    envelope.matches_object_v1(object).map_err(refused)?;
    let payload = envelope.original_fp_v1().map_err(refused)?;
    if payload.commitment != *original {
        return Err(refused("TFP1 quote changes the actual original public job"));
    }
    let frame = prepare_original_v1(&builder, ctx, object, original, &envelope.header, &envelope.pledge)?;
    if envelope.declared_rent_sompi != frame.plan_v1().terms.plan.rent_sompi {
        return Err(refused("TFP1 quote declaration differs from exact native full-SUM rent"));
    }
    Ok(frame.plan_v1().clone())
}

/// **The exact native plan an original would be admitted under at `now_daa`** — shared by admission
/// ([`prepare_original_v1`]) and the producer's pre-signing quote ([`palw_tir_public_original_quote_v1`]) so the two cannot drift.
/// No pledge and no narrowing: callers decide what they admit or sign. `resource_plan_id` and `source_id` do not depend on
/// `now_daa`; `allocation_id` and the absolute clocks do (they are fixed again at the actual accepting block).
#[allow(clippy::type_complexity)]
fn original_plan_v1(
    b: &TransitionBuilder<'_>,
    now_daa: u64,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
) -> Result<
    (
        crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1,
        crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationRulesV1,
        u64,
        crate::palw_staged_preclaim_v1::PalwStagedClaimGeometryV1,
    ),
    PalwStateV2Error,
> {
    use crate::palw_tir_public_allocation_v1::{PalwTirPublicAllocationQuoteV1, derive_tir_public_allocation_v1};
    let mut native = rules(b)?;
    native.maximum_leaves = b.state.class_step_ladder_v1(&original.job.class_id, native.maximum_leaves);
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(original);
    if b.state.claim(&claim).is_some() || b.state.staged_court_row_v1(&Key::TirClaim(claim)).is_some() {
        return Err(refused("TIR original allocation cannot be attached to an existing claim"));
    }
    let upper = b.state.staged_maximum_custodians_v1(original.job.class_id, now_daa, b.params, b.extras)?;
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
        accepted_daa: now_daa,
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
    Ok((plan, native, upper, geometry))
}

/// **G14 R1/R2: the producer's pre-signing quote of an ORIGINAL frame** on the accepted snapshot: the exact native plan whose
/// `resource_plan_id`/`source_id`/custodian upper the producer's pledge signs, and whose terms tell it the clocks and the rent it
/// must declare and burn. `Ok(None)` for an original the frame route does not admit (a registered semantic stage, a non-greedy
/// selection rule or a job tail — those still take the ClaimBundle route). Reads state only; buys nothing.
pub fn palw_tir_public_original_quote_v1(
    base: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    now_daa: u64,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
) -> Result<Option<crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1>, PalwStateV2Error> {
    if original.job.decode.is_some() || original.job.tail.is_some() {
        return Ok(None);
    }
    let builder = TransitionBuilder::try_new(base, params, false, false, false, false, extras)?;
    let (plan, _, _, _) = original_plan_v1(&builder, now_daa, original, header)?;
    Ok(plan.semantic.is_none().then_some(plan))
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
    let (plan, native, upper, geometry) = original_plan_v1(b, ctx.daa_score, original, header)?;
    let claim = crate::palw_freeprompt_v3::fp_claim_id_v3(original);
    let record = b.state.tir_class_v1(&geometry.class_id).ok_or_else(|| refused("TIR original class is not registered"))?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(&b.state, geometry.class_id)
        .ok_or_else(|| refused("TIR original layout is not registered"))?;
    let semantics = b.state.class_semantics.get(&geometry.class_id);
    let inventory = semantics.and_then(|s| b.state.staged_tokenizer_inventory_v1(s.tokenizer_id));
    // **G14 R4 (narrowing, not completion): every violation class an admitted original covers must have a native conviction
    // door.** The ORIGINAL frame's door judges arithmetic/logits, greedy selection, output identity and DA; the registered
    // semantic stages (constraint, tokenizer, prefix state) still convict only through the old ClaimBundle's staged courts, which a
    // frame never has. A semantic-bearing original is therefore refused rather than admitted with an unprosecutable layer.
    if plan.semantic.is_some() || original.job.decode.is_some() || original.job.tail.is_some() {
        return Err(refused("TIR original carries a semantic stage or non-greedy selection no frame court judges yet"));
    }
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
    // **G14 R1/R6: the frame's absolute clocks are fixed with the claim's own (d0).** The claim reaches `Final` no earlier than
    // the frame's collection door closes (`palw_claim_final_floor_v1`), so a claim whose X3 hard deadline is fixed before that door
    // would have two incompatible d0 contracts; it is refused here rather than extended later (X3: no extension past d0).
    if let Some(hard) = crate::palw_ruling_fences_v1::palw_claim_hard_deadline_of_v1(b.params, c) {
        if hard < p.terms.collect_until_daa {
            return Err(refused("TIR original collection door outlives the claim's fixed X3 hard deadline"));
        }
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
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(Box::new(prep).into())))?;
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())))?;
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
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(prep)))?;
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())))?;
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
    b.write_staged_row_v1(key, Some(Row::TirAcquisition(Box::new(row).into())))?;
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
    let saved = b.try_checkpoint()?;
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
    let frame = native_frame(b, control.claim, control.allocation_id, control.source_id)?;
    control.validate_native_signature_v1(network_domain, &b.state).map_err(refused)?;
    apply_verified_control_v1(b, ctx, frame, control)
}

/// Everything a control does once its literal signer's ML-DSA signature verified under the native network domain: the frame is
/// the CURRENT rooted original (`native_frame`). Split out so the fold-level E2E tests drive the real native path with synthetic
/// keys; production reaches it only through [`apply_control_inner_v1`].
fn apply_verified_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: Frame,
    control: &crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_tir_public_carriage_v1::{PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T};
    let maximum_leaves = frame.descriptor_v1().maximum_leaves;
    let transition = match (&control.target, &control.action) {
        (T::OwnerOpen { owner }, A::Open) => {
            if control.declared_rent_sompi != owner_terms_v1(&frame)?.plan.rent_sompi {
                return Err(refused("TIR Open declared burn differs from this separately paid native owner rent"));
            }
            return open_v1(b, ctx, &frame, *owner);
        }
        (T::Preparation { .. }, A::Preparation(action)) => {
            if control.declared_rent_sompi != 0 {
                return Err(refused("TIR preparation continuation cannot invent a second upfront rent"));
            }
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
            if control.declared_rent_sompi != 0 {
                return Err(refused("TIR acquisition continuation cannot change its prepaid rent"));
            }
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
        (T::Conviction, A::Convict(evidence)) => {
            if control.declared_rent_sompi != frame.conviction_rent_v1() || control.expected_turn != 0 {
                return Err(refused("TIR public conviction must burn exactly the frame's native terminal rent and carry no turn"));
            }
            return apply_conviction_v1(b, ctx, &frame, control.signer, evidence);
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
        // G14 R5: every literal liable defaulter must still hold its original exit hold, so the conviction opens with its
        // collateral LOCKED and `conviction_collected_v1` counts a real debit. A lapsed hold is refused here — the helper's
        // skip of an unlocked bond is never recorded as a successful collection.
        for bond in unpaid.iter() {
            if !b.state.staged_bond_exit_held_v1(bond, ctx.daa_score) {
                return Err(refused("TIR default names a liable bond whose original exit hold lapsed; no fictitious collection"));
            }
        }
    }
    for change in transition.changes_v1() {
        if b.state.staged_court_row_v1(&change.key) != change.old.as_ref() {
            return Err(refused("TIR transition changes a noncurrent native row"));
        }
    }
    for change in transition.changes_v1() {
        b.write_staged_row_v1(change.key, change.new.clone())?;
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

/// **G14 R4: who may file a public conviction against an ORIGINAL frame** — any Active bond at or above the floor that is not
/// liable on this claim: not its producer and not a seat holding a credited exact Valid custody grant. A drawn Panel seat that
/// signed no Valid and a bond outside the Panel are the SAME filer; neither a vote nor a co-signature is read.
fn conviction_filer_v1(b: &TransitionBuilder<'_>, frame: &Frame, filer: PalwBondKeyV2) -> Result<(), PalwStateV2Error> {
    let record = b.state.bond(&filer).ok_or(PalwStateV2Error::MissingBond(filer))?;
    if filer == frame.custody_v1().pledge_v1().producer
        || frame.original_custody_v1().credit_v1(&filer).is_some()
        || frame.original_custody_v1().grant_v1(&filer).is_some()
        || !matches!(record.status, PalwBondStatusV2::Active)
        || record.collateral < b.params.min_collateral_sompi()
    {
        return Err(refused("TIR public conviction filer is liable on this claim, inactive or below the floor"));
    }
    Ok(())
}

/// The complete original ID sequence, re-chunked natively into the canonical 1024-ID requests and EOF-authenticated against the
/// claim's committed logits trace root. Partial sequences authenticate nothing.
fn authenticated_original_ids_v1(
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    rows_root: Hash64,
    ids: &[u32],
) -> Result<crate::palw_tir_public_source_v1::PalwTirPublicAuthenticatedIdsV1, PalwStateV2Error> {
    use crate::palw_tir_public_source_v1::{
        PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1, PalwTirPublicIdsAssemblerV1, PalwTirPublicRequestV1, PalwTirPublicResponseV1,
    };
    if ids.len() as u64 != u64::from(source.decode_count_v1()) {
        return Err(refused("TIR public output conviction does not carry the complete original ID sequence"));
    }
    let mut assembler = PalwTirPublicIdsAssemblerV1::default();
    let mut offset = 0usize;
    for chunk in ids.chunks(PALW_TIR_PUBLIC_IDS_PER_CHUNK_V1 as usize) {
        let request = PalwTirPublicRequestV1::Ids { offset: u32::try_from(offset).map_err(|_| refused("TIR ID offset overflow"))? };
        let response = PalwTirPublicResponseV1::Ids { rows_root, ids: chunk.to_vec() };
        offset += chunk.len();
        if let Some(done) = assembler.push_v1(source, &request, &response).map_err(refused)? {
            if offset != ids.len() {
                return Err(refused("TIR authenticated IDs reached EOF before the carried sequence ended"));
            }
            return Ok(done);
        }
    }
    Err(refused("TIR public output conviction never reached the original authenticated EOF"))
}

/// **G14 R4/R6: one public bonded verifier convicts an ORIGINAL frame from authenticated public material alone.**
///
/// 1. The filer is any non-liable Active above-floor bond ([`conviction_filer_v1`]); the window is the frame's own original
///    collection door `[accepted, collect_until)` — fixed at d0, never extended — inside which the claim cannot reach `Final`
///    (`palw_claim_final_floor_v1` floors it at `collect_until`), so no Final weight, receipt spend, escrow, Gate C credit or
///    coinbase leg can escape before this door closes.
/// 2. The evidence is adjudicated natively ([`crate::palw_tir_objective_v1`]): the registered program filled from state, the
///    original class/artifact/execution/trace/job pinned from the claim, the court rules and the step ladder the native ones,
///    the proof body bounded by the producer-prepaid terminal witness ceiling. `PredicateHolds` / `OutputHolds` refuse the object
///    (nothing changes; it is neither guilt nor a certificate of honesty). Errors (`Need`, missing material, oversize, EOF short)
///    refuse it too — none of them is read as a finding.
/// 3. The sanction reuses the established economic funnels, never a new tariff: every credited Valid custodian's S4 (its native
///    lock plus the S3/S4 action tier, keyed `palw_false_valid_offence_id_v2(seat, claim)`, so once only across this route, the
///    kind-3 route and the DA route), then the producer through `convict_by_court_verdict_v1` (void a live claim `CourtFraud`,
///    or reverse a `Final` one — which revokes its RFC-0018 Gate C weight and frontier/settlement credit by the one predicate —
///    and record `CourtConviction` once, the filer's reporter reward, by-claim round-right forfeiture and ADR-0160's intent-class
///    funnel). An execution-proving fault also forfeits every minted right by ROOT, so a sibling claim sharing the convicted
///    execution (and an EXEC root's dependent slices, through `void_claim`) loses its standing too.
/// 4. Every liable bond must still hold its original exit hold (the frame's `staged_holds`), so its collateral is locked when the
///    conviction opens and the debit is actually collectible (`conviction_collected_v1`); a liable bond without it refuses the
///    filing instead of recording a fictitious collection.
fn apply_conviction_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
    filer: PalwBondKeyV2,
    evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_tir_objective_v1::{PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out};
    use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;
    let claim_id = frame.plan_v1().original_claim;
    let terms = frame.plan_v1().terms;
    if ctx.daa_score < terms.accepted_daa || ctx.daa_score >= terms.collect_until_daa {
        return Err(refused("TIR public conviction is outside the frame's original collection door"));
    }
    conviction_filer_v1(b, frame, filer)?;
    let claim = b.state.claim(&claim_id).cloned().ok_or(PalwStateV2Error::MissingClaim(claim_id))?;
    let producer = frame.custody_v1().pledge_v1().producer;
    if claim.bond != producer || claim.execution_root != frame.original_v1().execution_root {
        return Err(refused("TIR public conviction target changes the original claim's producer or execution"));
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
        // Once: the record (past R-core+, which this route's fence requires) or, defensively, the conviction's own void.
        return Err(refused("TIR original claim is already convicted; a conviction is recorded once"));
    }
    let native = rules(b)?;
    // `Some(root)`: execution-proving (forfeit by root); `None`: claim-proving only (by claim, in the funnel's record).
    let by_root = match evidence {
        E::Arithmetic { proof } => {
            match crate::palw_tir_objective_v1::adjudicate_original_v1(
                &b.state,
                claim_id,
                proof,
                &native.court,
                frame.descriptor_v1().maximum_leaves,
                native.prompt_form,
                frame.plan_v1().terminal.maximum_witness_bytes,
            )
            .map_err(refused)?
            {
                Arith::Convicts(finding) => {
                    if finding.target_v1().claim_id != claim_id || finding.target_v1().execution_root != claim.execution_root {
                        return Err(refused("TIR arithmetic finding names another claim or execution"));
                    }
                    Some(claim.execution_root)
                }
                Arith::PredicateHolds { .. } => {
                    return Err(refused("TIR nominated arithmetic primitive holds: no conviction, and no finding of claim honesty"));
                }
            }
        }
        E::DecodeToken { proof } => {
            let job = &frame.original_v1().job;
            if job.decode.is_some() || job.tail.is_some() {
                return Err(refused("TIR original job's selection rule is not plain greedy; no native selection court judges it yet"));
            }
            match crate::palw_tir_objective_v1::adjudicate_original_greedy_decode_v1(
                &b.state,
                claim_id,
                proof,
                &native.court,
                frame.descriptor_v1().maximum_leaves,
                native.prompt_form,
                frame.plan_v1().terminal.maximum_witness_bytes,
            )
            .map_err(refused)?
            {
                Arith::Convicts(finding) => {
                    if finding.target_v1().claim_id != claim_id || finding.target_v1().execution_root != claim.execution_root {
                        return Err(refused("TIR selection finding names another claim or execution"));
                    }
                    // The committed trace contradicts its own rows: the execution itself is false (forfeit by root).
                    Some(claim.execution_root)
                }
                Arith::PredicateHolds { .. } => {
                    return Err(refused("TIR nominated greedy selection holds: no conviction, and no finding of claim honesty"));
                }
            }
        }
        E::Output { rows_root, ids } => {
            let original_source = source(b, frame)?;
            let authenticated = authenticated_original_ids_v1(&original_source, *rows_root, ids)?;
            match crate::palw_tir_objective_v1::adjudicate_original_output_v1(
                &b.state,
                claim_id,
                frame.original_v1().output_root,
                &original_source,
                &authenticated,
            )
            .map_err(refused)?
            {
                Out::Fault(found) => match found.fault_v1() {
                    OutFault::OutputRootMismatch => None,
                    OutFault::IdOutsideVocabulary { .. } => Some(claim.execution_root),
                },
                Out::OutputHolds { .. } => {
                    return Err(refused("TIR original output identity holds: no conviction, and no finding of claim honesty"));
                }
            }
        }
    };
    // Liable = the producer and every exact credited Valid custodian (a drawn uncredited seat is not liable and is never charged).
    let credited: Vec<PalwBondKeyV2> = frame.original_custody_v1().credits_v1().map(|(seat, _)| *seat).collect();
    for bond in std::iter::once(&producer).chain(credited.iter()) {
        if !b.state.staged_bond_exit_held_v1(bond, ctx.daa_score) {
            return Err(refused("TIR liable bond lost its original exit hold; refusing a fictitious collection"));
        }
    }
    // 1. Each credited Valid custodian: S4, lock first (consumed keys skip — once across every route).
    if !credited.is_empty() {
        let conv = b.open_conviction_v1(&credited, ctx.daa_score);
        let g = b.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        b.charge_covering_signers_v1(&conv, claim_id, &credited, g)?;
    }
    // 2. The producer, through the one funnel for a proven verdict (void / Final reversal, Gate C revocation, CourtConviction).
    b.convict_by_court_verdict_v1(ctx, claim_id, &claim, filer, None)?;
    // 3. An execution-proving fault forfeits by ROOT (siblings sharing the convicted execution).
    if let Some(root) = by_root {
        b.forfeit_minted_round_rights(&root)?;
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
    let saved = b.try_checkpoint()?;
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
    b.write_slashable_lock(key, Some(lock))?;
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
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(prep)))?;
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())))?;
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

/// **G14 E2E at the native fold (source only — written in the no-build pass, NOT run; never labelled PASS):** the producer and
/// EVERY credited Panel seat collude on a claim whose step leaf 7 is forged; one bond outside the Panel (and, identically, one drawn
/// Panel seat that signed no Valid) convicts it from public material alone. What these tests do NOT cover, stated so nothing reads
/// them as more: real ML-DSA (the frame's pledge/grant/receipt signatures are synthetic and the signed carrier is bypassed by
/// `apply_verified_control_v1`), the outer transaction fee/burn, the processor's block/template walk, the node filer, and the
/// capacity-liability freeze (its admission refusal is ADM's existing `ProducerFrozen` test).
#[cfg(test)]
mod conviction_e2e_tests {
    use super::*;
    use crate::palw_tir_public_carriage_v1::{
        PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T, PalwTirPublicControlV1 as Control,
        PalwTirPublicConvictionEvidenceV1 as Evidence,
    };
    use crate::palw_tir_public_claim_v1::{PalwTirPublicFrameFixtureV1, palw_tir_public_frame_fixture_v1};
    use crate::palw_tir_public_custody_v1::{PalwTirPublicCustodyGrantV1, PalwTirPublicOriginalCreditV1, PalwTirPublicOriginalPanelV1};

    const FORGED_LEAF: usize = 7;
    const COLLATERAL: u64 = 1_000_000_000_000;
    const RESERVED: u128 = 7_000;
    const LOCK: u128 = 5_000;
    const SIG: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn bond(n: u64) -> PalwBondKeyV2 {
        PalwBondKeyV2(crate::tx::TransactionOutpoint::new(h(n), 0))
    }
    /// Two colluding seats that sign Valid, one honest drawn seat that signs nothing, one bond outside the Panel.
    fn colluders() -> [PalwBondKeyV2; 2] {
        [bond(201), bond(202)]
    }
    fn honest_seat() -> PalwBondKeyV2 {
        bond(203)
    }
    fn outsider() -> PalwBondKeyV2 {
        bond(300)
    }
    fn drawn() -> Vec<PalwBondKeyV2> {
        let mut seats = colluders().to_vec();
        seats.push(honest_seat());
        seats
    }
    fn bond_state(operator: u64) -> PalwBondStateV2 {
        PalwBondStateV2 {
            pubkey: vec![7; 4],
            operator_id: h(0x0900 + operator),
            collateral: COLLATERAL,
            slashed: 0,
            status: PalwBondStatusV2::Active,
            registered_daa: 0,
            payout_payload: h(0x9A11),
            capable_classes: Default::default(),
        }
    }

    /// The frame with the WHOLE drawn Panel frozen and every colluder holding an exact credited Valid custody grant + Ready.
    fn credited_frame(fx: &PalwTirPublicFrameFixtureV1) -> Frame {
        let frame = fx.frame.clone();
        let plan = frame.plan_v1().clone();
        let header = frame.header_v1().clone();
        let original = frame.original_v1().clone();
        let pledge = frame.custody_v1().pledge_v1().clone();
        let bound = plan.terms.accepted_daa;
        let panel = PalwTirPublicOriginalPanelV1 { anchor: h(0xA0), bound_daa: bound, seats: drawn().into_iter().collect() };
        let credited = colluders()
            .into_iter()
            .map(|seat| {
                let receipt = crate::palw_panel_v2::PalwSeatReceiptV2 {
                    claim: plan.original_claim,
                    verdict: crate::palw_panel_v2::PalwReceiptVerdictV2::Valid,
                    seat_bond: seat,
                    signed_daa: bound + 1,
                    signature: vec![3; SIG],
                };
                let grant = PalwTirPublicCustodyGrantV1 {
                    version: 1,
                    claim: plan.original_claim,
                    seat,
                    signed_daa: bound + 1,
                    pledge_id: pledge.id_v1(),
                    valid_receipt_digest: PalwTirPublicCustodyGrantV1::receipt_digest_v1(&receipt),
                    signature: vec![4; SIG],
                };
                let descriptor = crate::palw_backend::PalwTirPublicPreparedDescriptorV1 {
                    version: 1,
                    source_id: plan.source_id,
                    class_id: frame.descriptor_v1().class_id,
                    artifact_root: header.artifact_root,
                    execution_root: original.execution_root,
                    trace_root: original.trace_root,
                    step_root: header.step_merkle_root,
                    step_leaf_count: header.step_leaf_count,
                    decode_count: header.job_context.exact_decode_tokens,
                    vocab: plan.acquisition.envelope.vocab,
                    material_until_daa: plan.terms.material_until_daa,
                    manifest_digest: h(0xD1),
                    retained_bytes: 1,
                };
                let ready =
                    crate::palw_tir_public_owned_v1::PalwTirPublicReadyV1 { descriptor, accepted_daa: bound + 2, work_receipt: h(0xE1) };
                (bound + 2, grant, PalwTirPublicOriginalCreditV1 { receipt, ready })
            })
            .collect();
        let custody = frame.original_custody_v1().clone().with_credits_for_tests_v1(panel, credited).unwrap();
        frame.with_custody_for_tests_v1(custody).unwrap()
    }

    /// The accepted world the colluders built: the licensed claim, its drawn Panel, the colluders' native S4 locks, the registered
    /// class and the rooted frame (its indices — producer exposure and the liable signers' exit holds — rebuilt from the rows).
    fn world(fx: &PalwTirPublicFrameFixtureV1, frame: &Frame) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
        // R-core+ in force from genesis: the route's fence requires it (the conviction tiers, the `CourtConviction` record and the
        // exit-lock lattice a collectible debit needs).
        let params = PalwStateParamsV2::new(100, 10, 40, 20, 500, 1000, h(1), 4, 1000, 100, 1000, 600)
            .unwrap()
            .with_fp_exposure_ceiling(500)
            .unwrap()
            .with_escrow_backed_exposure_from_daa(Some(0))
            .with_rcore_plus_mirrors(Some(0), 12_900, Vec::new());
        let mut state = PalwChainStateV2::genesis();
        let plan = frame.plan_v1();
        let original = frame.original_v1();
        let claim_id = plan.original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let class_id = frame.descriptor_v1().class_id;
        for (n, key) in std::iter::once(producer).chain(drawn()).chain(std::iter::once(outsider())).enumerate() {
            state.bonds.insert(key, bond_state(n as u64));
        }
        state.set_class_for_tests(
            class_id,
            PalwClassStateV2 {
                artifact_root: frame.header_v1().artifact_root,
                slash_value_per_pwu: 1,
                pwu_rule: PalwPwuRuleV2::MaxPerAttempt(10),
                status: PalwClassStatusV2::Active,
                registered_daa: 0,
                registrant_bond: None,
                fused_attention: false,
            },
        );
        state.tir_classes.insert(class_id, fx.record.clone());
        state.staged_courts.insert(
            crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirLayout(class_id),
            crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirLayout(Box::new(fx.layout.clone())),
        );
        let claim = PalwClaimStateV2 {
            source: PalwClaimSourceV2::FreePrompt { quanta: 1, spent: Default::default() },
            class_id,
            bond: producer,
            pwu: 10,
            accepted_daa: plan.terms.accepted_daa,
            rebound_daa: None,
            accepted_blue_score: plan.terms.accepted_daa,
            accepted_block: BlockHash::from_u64_word(1),
            trace_root: original.trace_root,
            output_root: original.output_root,
            execution_root: original.execution_root,
            trace_chunk_count: 1,
            trace_retention_daa: plan.terms.material_until_daa,
            reserved: RESERVED,
            immature_contribution: 0,
            escrowed_reward: 0,
            work_leaves: original.work_leaves,
            work_id: None,
            phase: PalwClaimPhaseV2::ReceiptLicensed { licensed_daa: plan.terms.accepted_daa + 3 },
            rights_reserved: 0,
            job_identity: frame.custody_v1().pledge_v1().job_identity,
            rcore: PalwClaimRcoreV1::default(),
        };
        let held = palw_claim_commitment_v1(&params, &claim, plan.terms.accepted_daa + 3).unwrap();
        state.reserved_exposure.insert(producer, held);
        let panel = PalwPanelStateV2 {
            anchor: h(0xA0),
            seats: drawn().into_iter().map(|seat| PalwPanelSeatV2 { bond: seat, operator_id: h(0x77) }).collect(),
            bound_daa: plan.terms.accepted_daa,
        };
        state.set_false_valid_rows_for_tests(claim_id, Some(claim), Some(panel), None, 0);
        for seat in frame.original_custody_v1().credits_v1().map(|(seat, _)| *seat) {
            state.slashable_locks.insert(
                (seat, claim_id),
                crate::palw_panel_var_v1::PalwSlashableLockV1 {
                    claim: claim_id,
                    amount: LOCK,
                    expiry_daa: plan.terms.bond_exit_after_daa,
                    settled_at_final: 0,
                    attested: crate::palw_verification_v2::PalwSegmentMaskV2::NONE,
                    segments: 0,
                },
            );
        }
        state.staged_courts.insert(frame.key_v1(), crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirClaim(Box::new(frame.clone()).into()));
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let extras = PalwTransitionExtrasV1 {
            tir_public_allocation_rules_v1: Some(fx.rules),
            tir_public_network_domain_v1: Some(h(0x4E7)),
            ..Default::default()
        };
        (state, params, extras)
    }

    fn cone(fx: &PalwTirPublicFrameFixtureV1) -> Evidence {
        let rules = crate::palw_tir_court_v1::PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
        };
        let refutation =
            crate::palw_tir_court_v1::build_tir_cone_refutation_v1(&fx.execution.binding, FORGED_LEAF as u64, &fx.execution, &rules)
                .unwrap();
        let mut proof = crate::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation: Box::new(refutation) };
        proof.tir_strip_program_v1();
        Evidence::Arithmetic { proof: Box::new(proof) }
    }

    fn filing(frame: &Frame, filer: PalwBondKeyV2, evidence: Evidence) -> Control {
        let plan = frame.plan_v1();
        let mut control = Control::conviction_unsigned_v1(
            plan.original_claim,
            plan.allocation_id,
            plan.source_id,
            filer,
            evidence,
            frame.conviction_rent_v1(),
        )
        .unwrap();
        // Synthetic: the native signature check is the caller's (`apply_control_inner_v1`); this test drives the verified half.
        control.signature = vec![5; SIG];
        control
    }

    fn at(frame: &Frame, offset: u64) -> PalwBlockContextV2 {
        let daa = frame.plan_v1().terms.accepted_daa + offset;
        PalwBlockContextV2 { block: BlockHash::from_u64_word(9_000 + daa), daa_score: daa, blue_score: daa, subsidy: 0 }
    }

    /// The verdict a run reaches, by the fields every filer must agree on.
    fn outcome(state: &PalwChainStateV2, frame: &Frame) -> (PalwClaimPhaseV2, Vec<u64>, Vec<bool>) {
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let phase = state.claims.get(&claim_id).unwrap().phase.clone();
        let collateral = std::iter::once(producer).chain(drawn()).map(|b| state.bonds.get(&b).unwrap().collateral).collect();
        let consumed = std::iter::once(palw_court_conviction_offence_id_v1(&producer.0, &claim_id))
            .chain(drawn().into_iter().map(|seat| crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id)))
            .map(|key| state.consumed_offences.contains_key(&key))
            .collect();
        (phase, collateral, consumed)
    }

    #[test]
    fn g14_full_collusion_one_outside_verifier_convicts_slashes_and_refuses_the_claims_standing() {
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = credited_frame(&fx);
        let (state, params, extras) = world(&fx, &frame);
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let terms = frame.plan_v1().terms;
        let root_before = state.state_root();

        // Before any verdict: the claim cannot reach Final and Gate C waits while the frame's d0 door is open.
        let claim = state.claims.get(&claim_id).unwrap().clone();
        let floor = palw_claim_final_floor_v1(&state, &params, &claim_id, &claim, terms.accepted_daa + 3).unwrap();
        assert!(floor >= terms.collect_until_daa, "no Final (weight, receipt spend, escrow, Gate C credit) before the door closes");
        assert!(state.palw_tir_public_prosecution_open_v1(&claim_id, terms.collect_until_daa - 1));
        assert!(!state.palw_tir_public_prosecution_open_v1(&claim_id, terms.collect_until_daa));
        for liable in std::iter::once(producer).chain(colluders()) {
            assert!(state.staged_bond_exit_held_v1(&liable, terms.collect_until_daa - 1), "every liable bond is exit-held");
        }
        assert!(!state.staged_bond_exit_held_v1(&honest_seat(), terms.accepted_daa), "a drawn seat without a Valid is not liable");

        // Liable parties cannot file; nothing moves.
        for liable in std::iter::once(producer).chain(colluders()) {
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), frame.clone(), &filing(&frame, liable, cone(&fx))).is_err());
            assert_eq!(b.state.state_root(), root_before);
        }

        // One bond OUTSIDE the Panel convicts, with no Panel signature, vote or majority.
        let mut outside = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut outside, &at(&frame, 5), frame.clone(), &filing(&frame, outsider(), cone(&fx))).unwrap();
        let (phase, collateral, consumed) = outcome(&outside.state, &frame);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert!(collateral[0] < COLLATERAL, "the producer is actually debited (its reservation at least)");
        assert!(collateral[1] <= COLLATERAL - LOCK as u64 && collateral[2] <= COLLATERAL - LOCK as u64, "each colluding Valid: S4");
        assert_eq!(collateral[3], COLLATERAL, "the honest drawn seat that signed no Valid is never charged");
        assert_eq!(consumed, vec![true, true, true, false], "CourtConviction once; each colluder's false-Valid once; never the honest seat");
        for seat in colluders() {
            assert!(!outside.state.slashable_locks.contains_key(&(seat, claim_id)), "the lock was taken, not left to expire");
        }
        // Standing refused: a voided claim is never Final, so no receipt spend (block opportunity), Final weight, escrow, model
        // allocation or Gate C credit can rest on it; its execution root's minted rights are forfeit by root.
        assert!(!matches!(outside.state.claims.get(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));

        // Once only: the same or another filer, the same or another valid proof, finds the conviction recorded.
        let again = apply_verified_control_v1(&mut outside, &at(&frame, 6), frame.clone(), &filing(&frame, honest_seat(), cone(&fx)));
        assert!(again.is_err());

        // The honest drawn seat files the SAME object and reaches the SAME verdict (the common filer).
        let mut seat = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut seat, &at(&frame, 5), frame.clone(), &filing(&frame, honest_seat(), cone(&fx))).unwrap();
        assert_eq!(outcome(&seat.state, &frame), outcome(&outside.state, &frame));

        // Reorg: reverting the block's journal restores the pre-verdict root exactly.
        let mut reverted = outside.state.clone();
        for entry in outside.entries.iter().rev() {
            apply_delta_entry(&mut reverted, entry, true).unwrap();
        }
        assert_eq!(reverted.state_root(), root_before);

        // Outside the frame's own d0 door nothing is adjudicated.
        let mut late = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let door = terms.collect_until_daa - terms.accepted_daa;
        assert!(apply_verified_control_v1(&mut late, &at(&frame, door), frame.clone(), &filing(&frame, outsider(), cone(&fx))).is_err());
    }

    #[test]
    fn g14_honest_twin_is_not_convicted_and_a_holding_predicate_certifies_nothing() {
        // The same leaf of an HONEST execution: the primitive holds — the filing is refused and changes nothing.
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = credited_frame(&fx);
        let (state, params, extras) = world(&fx, &frame);
        let root = state.state_root();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), frame.clone(), &filing(&frame, outsider(), cone(&fx))).is_err());
        assert_eq!(b.state.state_root(), root);
        assert!(matches!(b.state.claims.get(&frame.plan_v1().original_claim).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        // A wrong burn is refused before adjudication (the frame's native terminal rent is exact).
        let mut cheap = filing(&frame, outsider(), cone(&fx));
        cheap.declared_rent_sompi = frame.conviction_rent_v1() - 1;
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), frame.clone(), &cheap).is_err());
    }

    #[test]
    fn g14_output_identity_convicts_from_the_complete_authenticated_ids_only() {
        // An honest execution whose producer-signed ORIGINAL commitment names a different output root than the IDs its own
        // committed trace authenticates: claim-proving (by claim, never by root), convicted from public IDs alone.
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let ids = fx.execution.generated.clone();
        let rows_root =
            crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
        let output = |ids: Vec<u32>| Evidence::Output { rows_root, ids };
        // Honest original: the identity holds — refused, nothing moves.
        let honest = credited_frame(&fx);
        let (state, params, extras) = world(&fx, &honest);
        let root = state.state_root();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(
            apply_verified_control_v1(&mut b, &at(&honest, 5), honest.clone(), &filing(&honest, outsider(), output(ids.clone())))
                .is_err()
        );
        // A truncated sequence authenticates nothing.
        let mut short = ids.clone();
        short.pop();
        if !short.is_empty() {
            assert!(apply_verified_control_v1(&mut b, &at(&honest, 5), honest.clone(), &filing(&honest, outsider(), output(short))).is_err());
        }
        assert_eq!(b.state.state_root(), root);
        // The lying original: convicted, CourtFraud, colluding Valid custodians charged, honest seat untouched.
        let lying = credited_frame(&fx).with_original_output_root_for_tests_v1(h(0xBAD));
        let (state, params, extras) = world(&fx, &lying);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut b, &at(&lying, 5), lying.clone(), &filing(&lying, outsider(), output(ids.clone()))).unwrap();
        let (phase, collateral, consumed) = outcome(&b.state, &lying);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert_eq!(collateral[3], COLLATERAL);
        assert_eq!(consumed, vec![true, true, true, false]);
        // The claim RECORD retaining a different output root than the frame's literal original is refused, never overridden.
        let (mut state, params, extras) = world(&fx, &lying);
        state.claims.get_mut(&lying.plan_v1().original_claim).unwrap().output_root = h(0xBAE);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&lying, 5), lying.clone(), &filing(&lying, outsider(), output(ids))).is_err());
    }

    #[test]
    fn g14_one_outside_bond_reaches_the_preparation_default_with_no_panel_signature() {
        // DA terminal by ONE public bonded verifier: the producer never publishes its original Ready by the d0 preparation door.
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = fx.frame.clone();
        let (mut state, params, extras) = world(&fx, &frame);
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        // The original acceptance installed the claim-wide preparation beside the frame (`install_original_v1`): rebuild that here
        // through the same native writer at the claim's own accepting block.
        state.staged_courts.remove(&frame.key_v1());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let accept = at(&frame, 0);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        install_original_v1(&mut b, &accept, frame.clone()).unwrap();
        let installed = b.state.clone();
        let before = installed.bonds.get(&producer).unwrap().collateral;
        let prep = frame.plan_v1().preparation_until_daa;
        let collect = |offset: u64| {
            let mut b = TransitionBuilder::new(&installed, &params, false, false, false, false, &extras);
            let control = Control {
                version: 1,
                claim: claim_id,
                allocation_id: frame.plan_v1().allocation_id,
                source_id: frame.plan_v1().source_id,
                target: T::Preparation { original_accepted_daa: frame.plan_v1().terms.accepted_daa },
                expected_turn: 0,
                declared_rent_sompi: 0,
                signer: outsider(),
                action: A::Preparation(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1::PreparationCollect { actor: producer }),
                signature: vec![5; SIG],
            };
            control.check_v1().unwrap();
            let result = apply_verified_control_v1(&mut b, &at(&frame, offset), frame.clone(), &control);
            (result, b)
        };
        // Before the producer's door closes nothing is owed.
        let (early, _) = collect(1);
        assert!(early.is_err());
        // After it: the outsider alone collects. The producer is debited, the claim voided for withholding (not fraud), the DA record
        // written once, and no Panel seat is touched.
        let (late, b) = collect(prep - frame.plan_v1().terms.accepted_daa);
        late.unwrap();
        assert!(matches!(
            b.state.claims.get(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::ProducerWithholding, .. }
        ));
        assert!(b.state.bonds.get(&producer).unwrap().collateral < before);
        assert!(b.state.consumed_offences.contains_key(&crate::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &claim_id)));
        for seat in drawn() {
            assert_eq!(b.state.bonds.get(&seat).unwrap().collateral, COLLATERAL);
        }
    }
}
