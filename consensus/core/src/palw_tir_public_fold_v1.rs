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

include!("palw_tir_prepared_original_geometry_v1.rs");
include!("palw_funded_original_coordinator_fold_v1.rs");

/// Diagnostics for a funded carrier on the native accepted UTXO path. There is
/// deliberately no Admitted/Complete variant and no state or spend capability.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PalwFundedOriginalCandidateV1 {
    Invalid(&'static str),
    Missing(&'static str),
    /// Local refusal/out-of-coverage is never an invalid claim or conviction.
    Resource(crate::palw_native_resource_v1::NativeCoverageV1),
}

/// Read-only candidate inspection. `validated` alone does not attest provenance:
/// the consensus caller must obtain it from its composed-view UTXO validator.
/// No caller-provided profile, graph or consent hash becomes native policy.
/// Ordinary fallback outputs survive every result of this function.
/// Native params/court/ladder must come from the consensus processor's current
/// config resolvers; these references are NOT a caller-adoptable ingress DTO.
/// Source construction reserves its native ticket. Remaining traversal work
/// without calibrated coverage stays Missing; Resource never implies fraud.
pub fn palw_funded_original_candidate_v1(
    state: &PalwChainStateV2,
    network: Hash64,
    daa: u64,
    validated: &crate::tx::ValidatedTransaction<'_>,
    native_params: Option<&PalwStateParamsV2>,
    native_court: Option<(&crate::palw_mode_v2::PalwCourtParamsV2,u64)>,
) -> PalwFundedOriginalCandidateV1 {
    use PalwFundedOriginalCandidateV1::{Invalid, Missing};
    use crate::palw_tir_funded_original_envelope_v1::{is_funded_original_carrier_v1, PalwTirFundedOriginalEnvelopeV1};
    let tx = validated.tx;
    if !is_funded_original_carrier_v1(tx) || !tx.id_is_current() {
        return Invalid("carrier framing or stale actual transaction id");
    }
    if tx.inputs.len() != validated.entries.len() {
        return Invalid("actual input entry count");
    }
    let mut seen = std::collections::HashSet::new();
    let mut input = 0u64;
    for (i, e) in tx.inputs.iter().zip(&validated.entries) {
        if !seen.insert(i.previous_outpoint) { return Invalid("duplicate actual input"); }
        let Some(next) = input.checked_add(e.amount) else { return Invalid("actual input amount overflow"); };
        input = next;
    }
    let Some(output) = tx.outputs.iter().try_fold(0u64, |n,o| n.checked_add(o.value)) else {
        return Invalid("actual output amount overflow");
    };
    if input.checked_sub(output) != Some(validated.calculated_fee) {
        return Invalid("actual input/output fee differs from native validation");
    }
    let Ok(envelope) = PalwTirFundedOriginalEnvelopeV1::decode_v1(&tx.payload) else {
        return Invalid("funded original bounded codec");
    };
    let terms = &envelope.funding.terms;
    if terms.network_domain != network || daa < terms.accept_from_daa || daa > terms.accept_until_daa {
        return Invalid("funding network or actual acceptance interval");
    }
    let Ok(actual) = crate::palw_court_original_escrow_v1::inspect_transaction_v1(tx, terms.funding_output_index) else {
        return Invalid("actual funding output");
    };
    if envelope.funding.check_actual_output_v1(&actual, &envelope.payer_pubkey).is_err()
        || envelope.funding.verify_signature_v1(&envelope.payer_pubkey).is_err() {
        return Invalid("actual funding output or payer consent signature");
    }
    let Ok(original) = envelope.original_v1() else { return Invalid("literal original envelope"); };
    let Ok(fp) = original.original_fp_v1() else { return Invalid("literal original commitment"); };
    if fp.validate_signature_v3(crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).is_err() {
        return Invalid("original commitment signature");
    }
    let Some(producer) = state.bond(&PalwBondKeyV2(fp.commitment.job.executor_bond)) else {
        return Invalid("current producer absent");
    };
    if producer.pubkey != fp.commitment.job.executor_pubkey || !matches!(producer.status, PalwBondStatusV2::Active) {
        return Invalid("current producer authority or status");
    }
    // Codec already binds pledge identity to these exact original bytes. Only
    // the actual current producer key can authenticate this separate consent;
    // payer signature or a nonzero resource/profile hash cannot replace it.
    if original.pledge.validate_signature_v1(&producer.pubkey,
        crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1).is_err() {
        return Invalid("current producer source pledge signature");
    }
    if state.claim(&fp.claim_id()).is_some() {
        return Invalid("original claim already exists");
    }
    let Some(record) = state.tir_class_v1(&fp.commitment.job.class_id) else {
        return Invalid("registered original TIR class absent");
    };
    let Some(layout) = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(state, record.facts.class_id) else {
        return Invalid("registered original TIR layout absent");
    };
    let Some(class) = state.class(&fp.commitment.job.class_id) else {
        return Invalid("native registered class state absent");
    };
    if class.registered_daa > daa || class.artifact_root != original.header.artifact_root {
        return Invalid("native registered class time or artifact");
    }
    // Producer consent has its own actual acceptance clock. This does not
    // manufacture any future service/retention horizon or extend that clock.
    if daa > original.pledge.accept_until_daa {
        return Invalid("producer source pledge acceptance clock");
    }
    let Some(params) = native_params else {
        return Missing("actual native state policy unavailable");
    };
    let demand = crate::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(params, class.registered_daa);
    let Some((court, network_ladder)) = native_court else {
        return Missing("actual current native court or ladder unavailable");
    };
    // Same native class uplift as original_plan_v1; no caller-declared class
    // ladder or signed profile ID is accepted in place of the rooted record.
    let maximum_leaves = state.class_step_ladder_v1(&fp.commitment.job.class_id, network_ladder);
    if maximum_leaves == 0 || !crate::palw_attn_dissect::palw_attn_arity_is_legal_v1(court.dissection_arity()) {
        return Missing("actual current native court geometry unsupported");
    }
    let source = match crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
        record, layout, &original.header, fp.commitment.execution_root, fp.commitment.trace_root, maximum_leaves,
    ) {
        Ok(source) => source,
        Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(coverage)) => {
            return PalwFundedOriginalCandidateV1::Resource(coverage);
        }
        Err(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(_)) => {
            return Invalid("native original source geometry or job roots");
        }
    };
    if source.source_id_v1() != original.pledge.source_id {
        return Invalid("producer pledge differs from actual native source");
    }
    // Source's exact old constructor has run with actual court/class ladder.
    // The registration-time demand is resolved from actual native state policy,
    // NOT the accepting DAA or a producer/profile declaration. Do not enumerate
    // numerical/semantic paths under an uncalibrated traversal budget, or build
    // an invented paid frame when measuredFrameCost is absent. These diagnostics
    // intentionally retain Missing and grant no funded/Complete/state authority.
    match demand {
        crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::Release2000 =>
            Missing("native source checked; registered Release2000 demand; numerical/semantic grammar calibration, profiles and ordered coordinator missing"),
        crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7 =>
            Missing("native source checked; registered H7 demand; numerical/semantic grammar calibration, profiles and ordered coordinator missing"),
    }

}

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
/// The CURRENT rooted original frame of `claim`, shared (an `Arc` clone, not a deep copy of every credit).
///
/// **Audit §10 (the frame pricing fix): the full structure check runs only where the fold re-reads the whole frame** — an outside
/// owner's Open and a conviction filing (`full`), whose prices carry one measured full-frame pass each. Every other control checks
/// the frame's immutable identity (claim, allocation, source) here and its own row in its transition: the rooted frame is written
/// only by its install and its custody writes, each of which checks the whole frame it writes, and the state root authenticates it
/// in between. Measured before the fix: every Demand / Begin / Part / Finish re-checked it (2.3 ms at a 2.7 MB frame, 0.12 ms at
/// 63 KB) and its price assumed 32 such passes.
fn native_frame(
    b: &TransitionBuilder<'_>,
    claim: Hash64,
    allocation: Hash64,
    source_id: Hash64,
    full: bool,
) -> Result<crate::palw_staged_court_state_v1::PalwSharedCourtV1<Frame>, PalwStateV2Error> {
    rules(b)?;
    let Some(Row::TirClaim(frame)) = b.state.staged_court_row_v1(&Key::TirClaim(claim)) else {
        return Err(refused("TIR control has no paid original frame"));
    };
    if frame.plan_v1().allocation_id != allocation || frame.plan_v1().source_id != source_id || frame.key_v1() != Key::TirClaim(claim) {
        return Err(refused("TIR control substitutes original allocation or source"));
    }
    if full {
        frame.check_structure_v1(&frame.key_v1()).map_err(refused)?;
    }
    Ok(frame.clone())
}
/// **Who may own (Open / Demand / Collect) on an ORIGINAL frame** — any Active above-floor bond that is not liable on the claim:
/// not its producer and not a seat holding an exact credited Valid custody grant. **G14 G-T4:** a drawn Panel seat that signed no
/// Valid is as independent as a bond outside the Panel (the same rule as [`conviction_filer_v1`]), so an honest drawn seat that
/// found a fault pursues it through the same public acquisition and filing as an outsider; it can no longer grant custody on the
/// claim once it owns there ([`apply_grant_ready_v1`] refuses an owner).
fn independent(b: &TransitionBuilder<'_>, frame: &Frame, actor: PalwBondKeyV2) -> Result<(), PalwStateV2Error> {
    let record = b.state.bond(&actor).ok_or(PalwStateV2Error::MissingBond(actor))?;
    if actor == frame.custody_v1().pledge_v1().producer
        || frame.original_custody_v1().credit_v1(&actor).is_some()
        || frame.original_custody_v1().grant_v1(&actor).is_some()
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

/// The refusal code of [`require_original_frame_v1`] (`ClassNotRewardable`), stable for operators and tests.
pub const PALW_G03_FRAMELESS_TIR_ORIGINAL_CODE_V1: &str = "G03_FRAMELESS_TIR_ORIGINAL";

/// **G14 G-03 (the frame is mandatory): past `palw_tir_public_conviction_v1`, a Free Prompt commitment on a registered TIR class is
/// admitted only WITH its transaction-paired `TFP1` ORIGINAL frame.**
///
/// Chosen over "admit it frameless and withhold its Final, credit and reward" because it is one predicate at the one door every
/// TIR original passes (the commitment's acceptance), before anything is priced, reserved or written, and so cannot be bypassed:
/// - not by omitting `TFP1` (refused here);
/// - not by a job shape the frame does not judge — a job tail, a constrained job with no semantic contract, a flat logits scheme,
///   a `PanelDa` prompt (no public ids), a semantic stage whose staged court cannot open (see [`prepare_original_v1`]): such an
///   original cannot be framed, so with this rule it is not taken at all;
/// - not by a carrier pairing another claim's envelope (`check_paired_original_v1` binds the envelope to this exact object).
///
/// The alternative would have to withhold the claim's standing at every consumer (Final, spend weight, receipt spends, escrow,
/// coinbase legs, Gate C, model allocation), and any consumer missed would be a bypass. Refused here, the claim never exists.
/// The fence is read from the params (`tir_public_conviction_active`), not from whether the route's rules resolved at this block:
/// a block where they do not resolve refuses every TIR original (fail-closed), framed or not. `Ok` below the fence and for every
/// non-TIR class (A-2: nothing is judged there).
pub(super) fn require_original_frame_v1(b: &TransitionBuilder<'_>, claim: Hash64, class_id: Hash64) -> Result<(), PalwStateV2Error> {
    if !b.extras.tir_public_conviction_active || b.state.tir_class_v1(&class_id).is_none() || b.extras.tir_public_originals.contains_key(&claim)
    {
        return Ok(());
    }
    Err(PalwStateV2Error::ClassNotRewardable {
        class: class_id,
        code: PALW_G03_FRAMELESS_TIR_ORIGINAL_CODE_V1,
        why: "past palw_tir_public_conviction_v1 a TIR Free Prompt original must carry its paired TFP1 frame".into(),
    })
}

pub(super) fn prepare_paired_original_v1(
    b: &TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    object: &PalwConsensusObjectV2,
    public_job: Option<&crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3>,
) -> Result<Option<PreparedOriginalFrameV1>, PalwStateV2Error> {
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
    // Capture native facts without new Source reconstruction/resource scope.
    // The ordinary old preparation and price/quote paths remain unchanged.
    let mut native=rules(b)?;
    native.maximum_leaves=b.state.class_step_ladder_v1(&original.job.class_id,native.maximum_leaves);
    let registered=b.state.class(&original.job.class_id).map(|c|(c.registered_daa,crate::palw_tir_fence2_v1::palw_tir_demand_rules_at_v1(b.params,c.registered_daa)));
    Ok(Some(PreparedOriginalFrameV1{frame,at:*ctx,native,registered,network:b.extras.tir_public_network_domain_v1,semantic_snapshot:b.state.class_semantics_v1(&original.job.class_id).cloned()}))
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
    // G14 audit §11 (d): a frame's custodians are the flat Panel's credited seats and the producer (no typed licence reaches it).
    if b.state.tir_class_v1(&original.job.class_id).is_none() {
        return Err(refused("TIR original class is not registered"));
    }
    let upper = crate::palw_tir_frame_cost_v1::palw_tir_frame_custodian_upper_v1(now_daa, b.params, b.extras)?;
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
/// must declare and burn. `Ok(None)` for an original the frame route does not admit (a job tail or a flat-scheme class — those
/// still take the ClaimBundle route). A plan with `semantic` set also needs the producer's history pledge over that component
/// (`TFP1`'s `history_pledge`: `semantic_resource_plan_id`, `maximum_semantic_history_records`, the acquisition custodian upper,
/// the pledge's own acceptance and material horizon). Reads state only; buys nothing.
pub fn palw_tir_public_original_quote_v1(
    base: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    now_daa: u64,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
) -> Result<Option<crate::palw_tir_public_allocation_v1::PalwTirPublicAllocationPlanV1>, PalwStateV2Error> {
    if original.job.tail.is_some() {
        return Ok(None);
    }
    let builder = TransitionBuilder::try_new(base, params, false, false, false, false, extras)?;
    let (plan, _, _, geometry) = original_plan_v1(&builder, now_daa, original, header)?;
    let tiled = builder.state.tir_class_v1(&geometry.class_id).is_some_and(|record| record.facts.tiled);
    let judged = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&original.job).is_some() || plan.semantic.is_some();
    // Mirrors `prepare_original_v1`: a semantic stage is framed only where its staged court can open (G-18).
    let openable = plan.semantic.is_none() || params.legacy_public_filer_active_at(now_daa);
    Ok((tiled && judged && openable).then_some(plan))
}

/// **G14 R4 (semantic stages): the staged-court contract an ORIGINAL frame with a registered semantic stage carries beside
/// itself** — `Ok(None)` for a frame without one. The frame's full allocation already prices and schedules the semantic component
/// (`plan.semantic`, rebased onto the frame's own clocks, and its `semantic_resource_plan_id`); this builds the SAME ClaimBundle
/// contract the staged semantic courts (constraint, tokenizer, prefix state) hang off, from that component and the producer's
/// transaction-local history pledge (`TFP1`'s `history_pledge`, authenticated against the original object by the carrier walk).
/// Every pledge field is compared with the recomputed plan and the producer's bond signature is re-verified here, exactly as the
/// ClaimBundle route's `prepare_required_bundle_v1` does. Nothing is paid twice: the frame's rent and deposit already include the
/// semantic terms. A Valid custodian of a claim a semantic court convicts is charged through kind 3 on the `CourtFraud` void, as
/// on the ClaimBundle route; its frame exit hold keeps the collateral collectible.
fn frame_semantic_bundle_v1(b: &TransitionBuilder<'_>, frame: &Frame, now_daa: u64) -> Result<Option<Row>, PalwStateV2Error> {
    let plan = frame.plan_v1();
    let Some(semantic) = plan.semantic.as_ref() else { return Ok(None) };
    let config = b.extras.staged_public_court.ok_or(PalwStateV2Error::ObjectiveOffenceDormant)?;
    let claim = plan.original_claim;
    let original = frame.original_v1();
    let resource_plan_id =
        plan.semantic_resource_plan_id.ok_or_else(|| refused("TIR original semantic component has no resource identity"))?;
    let semantics = b
        .state
        .class_semantics
        .get(&original.job.class_id)
        .ok_or_else(|| refused("TIR original semantic component lost its class's immutable semantics"))?;
    let pledge = b.extras.history_custody_pledges.get(&claim).cloned().ok_or_else(|| {
        refused("TIR original with a semantic stage lacks its producer's signed history pledge (TFP1 history_pledge)")
    })?;
    pledge.check_v1().map_err(refused)?;
    let producer = frame.custody_v1().pledge_v1().producer;
    let record = b.state.bond(&producer).ok_or(PalwStateV2Error::MissingBond(producer))?;
    pledge
        .validate_signature_v1(&record.pubkey, crate::palw_constraint_stream_carriage_v1::palw_staged_verify_mldsa87_v1)
        .map_err(refused)?;
    let job_identity = crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original);
    if (
        pledge.claim,
        pledge.producer,
        pledge.execution_root,
        pledge.job_identity,
        pledge.cu_ruleset_id,
        pledge.resource_plan_id,
        pledge.maximum_records,
        pledge.maximum_custodians,
    ) != (
        claim,
        producer,
        original.execution_root,
        job_identity,
        semantics.cu_ruleset_id,
        resource_plan_id,
        plan.maximum_semantic_history_records,
        plan.acquisition.maximum_custodians,
    ) || now_daa >= pledge.accept_until_daa
        || semantic.terms.material_until_daa > pledge.material_until_daa
    {
        return Err(refused("TIR original history pledge changes the frame's semantic identity, capacity or paid horizon"));
    }
    let terms = semantic.terms;
    let custody = crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1::new(
        original.execution_root,
        job_identity,
        semantics.cu_ruleset_id,
        &terms,
    );
    Ok(Some(Row::ClaimBundle(Box::new(crate::palw_staged_bundle_v1::PalwStagedBundleContractV1 {
        claim: crate::palw_staged_court_state_v1::PalwStagedClaimContractV1 {
            terms,
            producer,
            panel: std::collections::BTreeSet::new(),
            prefix_custody: Some(custody.clone()),
        },
        plan: semantic.clone(),
        config,
        original_binding: crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(frame.header_v1().clone()),
        producer_custodian: None,
        history_custody: crate::palw_history_custody_v1::PalwHistoryCustodyContractV1 {
            custody,
            producer,
            maximum_records: plan.maximum_semantic_history_records,
            maximum_custodians: plan.acquisition.maximum_custodians,
            pledge,
        },
    }))))
}

/// **G14 R4: would the fold accept this public conviction filing at `next_daa`?** A dry run of the SAME native path
/// ([`apply_conviction_v1`]: window, filer, once-only, the native adjudication of the evidence, the liable bonds' exit holds and
/// every funnel) on a throw-away builder over the accepted snapshot — nothing is committed. The node's filer scheduler asks it
/// before it signs, so it never pays a conviction rent the fold would refuse for a reason the accepted state already shows. It is
/// never a verdict: a later block can still change the answer, and only the accepted fold convicts.
pub fn palw_tir_public_conviction_preflight_v1(
    base: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    next_daa: u64,
    filer: PalwBondKeyV2,
    claim: Hash64,
    evidence: &crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1,
) -> Result<(), PalwStateV2Error> {
    evidence.check_v1().map_err(refused)?;
    let mut builder = TransitionBuilder::try_new(base, params, false, false, false, false, extras)?;
    rules(&builder)?;
    let frame =
        palw_tir_public_claim_v1(base, claim).cloned().ok_or_else(|| refused("TIR conviction preflight: no ORIGINAL frame"))?;
    frame.check_structure_v1(&frame.key_v1()).map_err(refused)?;
    let ctx = PalwBlockContextV2 { block: Default::default(), daa_score: next_daa, blue_score: 0, subsidy: 0 };
    apply_conviction_v1(&mut builder, &ctx, &frame, filer, evidence)
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
    // **G14 R4: every violation class an admitted original covers must have a native conviction door.** The ORIGINAL frame's
    // door judges arithmetic/logits, output identity, DA, and selection and stop under the job's OWN rule (greedy, seeded
    // Decision 11 sampling, or the V4 pipeline: `PalwTirSelectionRuleV1`, `original_stop_rule_fault_v1`); a registered semantic
    // stage (constraint, tokenizer, prefix state) is judged by the staged semantic courts on the ClaimBundle contract the frame
    // installs beside itself (`frame_semantic_bundle_v1`, checked below and written by `install_original_v1`). Still refused
    // rather than admitted with an unprosecutable layer: a job tail (evaluation/generation) and a FLAT-scheme class (its
    // decode-token pin carries whole rows, which no bounded control holds without split witness carriage, R7).
    if original.job.tail.is_some() || !record.facts.tiled {
        return Err(refused("TIR original carries a job tail or a flat logits scheme no bounded frame door judges yet"));
    }
    // A constrained job's selection is its constraint's mask: the staged Constraint court judges it, so the frame must carry
    // the semantic contract (its plan always has the Constraint family when the job is constrained).
    if crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&original.job).is_none() && plan.semantic.is_none() {
        return Err(refused("TIR constrained original has no staged semantic contract to judge its selection"));
    }
    // **G14 from DAA 0 (no legacy dependency): a semantic stage is admitted only where its court can actually open.** The staged
    // semantic courts (prefix state, constraint, raw tokenizer) open a seed or a bundle only through the opener's own live legacy
    // reservation (tag 154, `palw_legacy_public_filer_v1`; audit G-18). Where that fence is not in force the stage would be admitted
    // with no conviction door, so the original is refused instead (and, with G-03, not taken at all). The frame's own doors —
    // arithmetic, selection, output/stop and DA — need no reservation and convict at any fence height, DAA 0 included.
    if plan.semantic.is_some() && !b.params.legacy_public_filer_active_at(ctx.daa_score) {
        return Err(refused(
            "TIR original's semantic stage has no openable court here (staged courts need palw_legacy_public_filer_v1, G-18)",
        ));
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
    let frame = Frame::prepare_native_v1(
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
    .map_err(refused)?;
    // The semantic stages' staged contract must build (pledge, identity, capacity, horizon) before anything is written.
    frame_semantic_bundle_v1(b, &frame, ctx.daa_score)?;
    Ok(frame)
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
    prompt: &PalwFpClaimPromptV1,
) -> Result<(), PalwStateV2Error> {
    let native = rules(b)?;
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
    // G14 r5 (item 2): clock v2 takes precedence over X3 when it governs (`palw_claim_route_hard_v2`: a + 10,000), else X3's hard.
    if let Some(hard) = crate::palw_court_clock_v2::palw_claim_route_hard_v2(b.params, c) {
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
    // G14 R4: a registered semantic stage installs its staged-court contract beside the frame, at the same d0.
    if let Some(bundle) = frame_semantic_bundle_v1(b, &frame, ctx.daa_score)? {
        let key = Key::Claim(p.original_claim);
        if b.state.staged_court_row_v1(&key).is_some() {
            return Err(refused("TIR original semantic contract would overwrite an existing staged claim contract"));
        }
        b.write_staged_row_v1(key, Some(bundle))?;
    }
    keep_original_prompt_ids_v1(b, &frame, native.prompt_form, prompt)?;
    // The native dispatcher atomically owns the ordinary FP writes and these two
    // rows. Neither a separately submitted quote nor a historical FP can enter.
    let original_claim = p.original_claim;
    b.write_staged_row_v1(frame.preparation_key_v1(), Some(Row::TirPreparation(Box::new(prep).into())))?;
    b.write_staged_row_v1(frame.key_v1(), Some(Row::TirClaim(Box::new(frame).into())))?;
    // G14 lane court-f02 (RFC-0028): ordinary admission installs the original's court service binding once its frame row exists.
    // Dormant (no row) below `palw_tir_public_conviction_v1` or without a pinned service profile.
    super::palw_court_service_lifecycle_fold_v1::install_court_service_binding_v1(b, ctx, original_claim)?;
    Ok(())
}

/// **G14 G-T3: the ORIGINAL frame keeps its own public replay input** — the prompt ids the claim was accepted with, under the
/// claim's ADR-0145 prompt row `(class, producer, claim)`, kept at least until the frame's material horizon (epoch-aligned upward,
/// exactly as the boundary sweep removes rows). An outside verifier replays the original job from these ids alone; before this the
/// row existed only where `palw_fp_derived_work` priced the claim, so a frame on a network without that fence (or whose row expired
/// first) could not be replayed by anyone outside the Panel. The ids must be the job's own (count and hash, under the native prompt
/// form): a `PanelDa` original carries none and is refused, as it has no public input to replay.
///
/// The row means what ADR-0145 says it means — "this class has been paid for this prompt" — which is true of a framed original
/// whether or not derived work priced it, so a later derived-work price reading it is reading a fact. A row the derived-work branch
/// already wrote is only ever extended to the horizon, never shortened or rewritten otherwise.
fn keep_original_prompt_ids_v1(
    b: &mut TransitionBuilder<'_>,
    frame: &Frame,
    form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    prompt: &PalwFpClaimPromptV1,
) -> Result<(), PalwStateV2Error> {
    let original = frame.original_v1();
    // A canonical job's prompt is a pure function of its own public job (`fp_canonical_anchor_v1` → the class's `job_for_anchor`):
    // every holder of the registered class derives it, and the carrier carries none. Nothing to keep.
    if original.job.prompt_mode == crate::palw_freeprompt_v3::PALW_FP_PROMPT_MODE_CANONICAL && prompt.prompt_token_ids.is_empty() {
        return Ok(());
    }
    crate::palw_freeprompt_v3::palw_fp_prompt_ids_admit_v1(&original.job, &prompt.prompt_token_ids, form)
        .map_err(|e| refused(format!("TIR original frame has no public prompt ids to keep for an outside replay: {e:?}")))?;
    let epoch = b.params.epoch_length.max(1);
    let horizon = frame
        .plan_v1()
        .terms
        .material_until_daa
        .div_ceil(epoch)
        .checked_mul(epoch)
        .ok_or(PalwStateV2Error::Overflow("TIR original prompt horizon"))?;
    let key = (original.job.class_id, frame.custody_v1().pledge_v1().producer, frame.plan_v1().original_claim);
    let row = match b.state.fp_claim_prompts.get(&key) {
        Some(kept) if kept.prompt_token_ids != prompt.prompt_token_ids => {
            return Err(refused("TIR original's kept prompt row names other ids than the original commitment"));
        }
        Some(kept) if kept.expires_daa >= horizon => return Ok(()),
        Some(kept) => PalwFpClaimPromptV1 { expires_daa: horizon, ..kept.clone() },
        None => PalwFpClaimPromptV1 { expires_daa: prompt.expires_daa.max(horizon), ..prompt.clone() },
    };
    b.write_fp_claim_prompt(key, Some(row))
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

/// **G14 R1 / G-T1: reclaim an idle owner's slot for the Open that needs it** — the idle row is retired in place (its open demand
/// closed, its turns spent, so it never demands again; its accepted answers stay rooted to the horizon) and its owner's deposit is
/// forfeited: burned through `slash_bond` (debited and recorded in the bond's `slashed`), paid to nobody — not to the Open that
/// reclaims it, so reclaiming is never a revenue. Refused (nothing changes) unless the row is idle at this block.
fn reclaim_owner_slot_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    key: Key,
    owner: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    let Some(Row::TirAcquisition(old)) = b.state.staged_court_row_v1(&key) else {
        return Err(refused("TIR idle-slot reclamation lost the idle owner's row"));
    };
    let mut row = old.as_ref().clone();
    let deposit = row.owned.contract_v1().terms.plan.deposit_sompi;
    let mut owned = row.owned.as_ref().clone();
    if tir_owner_fair_start_v1(b, ctx) {
        // Round 2: an admitted row's idleness counts from its admission (`staged_tir_idle_admitted_owners_v1` chose it), and the
        // reclamation is stamped as its release so the next waiting row is admitted from now.
        owned.retire_admitted_idle_v1().map_err(refused)?;
        owned.stamp_release_v1(ctx.daa_score);
    } else {
        owned.retire_idle_v1(ctx.daa_score).map_err(refused)?;
    }
    row.owned = Box::new(owned).into();
    row.check_structure_v1(&key).map_err(refused)?;
    b.write_staged_row_v1(key, Some(Row::TirAcquisition(Box::new(row).into())))?;
    b.slash_bond(owner, u128::from(deposit))?;
    Ok(())
}

fn tir_owner_tenure_v1(frame: &Frame, grace: u64) -> u64 {
    crate::palw_tir_public_claim_v1::palw_tir_owner_tenure_v1(frame, grace)
}

/// **Lane g14-court-capacity-r2: rotate the owner queue at `now`** — every admitted row past its tenure while a row waits yields its
/// slot (no forfeit), and every idle admitted row is reclaimed (its deposit burned) when some row waits. Called by an Open and by a
/// waiting row's Demand, before admission is read.
fn rotate_owner_queue_v1(b: &mut TransitionBuilder<'_>, ctx: &PalwBlockContextV2, frame: &Frame) -> Result<(), PalwStateV2Error> {
    let claim = frame.plan_v1().original_claim;
    let now = ctx.daa_score;
    let waiting = b.state.staged_tir_owner_queue_v1(&claim).iter().any(|(_, _, admitted, _)| !admitted);
    if !waiting {
        return Ok(());
    }
    for (idle_key, idle_owner) in b.state.staged_tir_idle_admitted_owners_v1(&claim, now) {
        reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;
    }
    let grace = b
        .state
        .staged_tir_owner_queue_v1(&claim)
        .first()
        .and_then(|(key, ..)| match b.state.staged_court_row_v1(key) {
            Some(Row::TirAcquisition(row)) => Some(row.owned.owner_idle_grace_daa_v1()),
            _ => None,
        })
        .unwrap_or(1);
    for (key, _) in b.state.staged_tir_overstayed_owners_v1(&claim, now, tir_owner_tenure_v1(frame, grace)) {
        let Some(Row::TirAcquisition(old)) = b.state.staged_court_row_v1(&key) else { continue };
        let mut row = old.as_ref().clone();
        let mut owned = row.owned.as_ref().clone();
        owned.retire_admitted_idle_v1().map_err(refused)?;
        owned.stamp_release_v1(now);
        row.owned = Box::new(owned).into();
        row.check_structure_v1(&key).map_err(refused)?;
        b.write_staged_row_v1(key, Some(Row::TirAcquisition(Box::new(row).into())))?;
    }
    Ok(())
}

/// **Lane g14-court-capacity-r2: the owner fair start is in force** — past `palw_court_capacity_v1`, or (G14 F03 ④, lane
/// g14p/court-owner) past `palw_court_outsider_lane_v1`, which needs no court-capacity policy: the owner queue, its tenure rotation
/// (an admitted row past `palw_tir_owner_tenure_v1` yields while a row waits — deadline-based preemption of an owner not finishing)
/// and idle reclamation (deposit burned) are pure state rules. K_owner = 8 is unchanged: the concurrent custodian load stays 8, and an
/// outsider's Open is never refused for the cap — it waits, in arrival order, for a slot that a bounded tenure guarantees. Dormant;
/// `false` on every shipped network, where `open_v1` refuses an Open at K_owner exactly as before.
fn tir_owner_fair_start_v1(b: &TransitionBuilder<'_>, ctx: &PalwBlockContextV2) -> bool {
    b.params.court_capacity_at_v1(ctx.daa_score).is_some() || b.params.court_outsider_lane_active_v1(ctx.daa_score)
}

fn open_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
    owner: PalwBondKeyV2,
) -> Result<(), PalwStateV2Error> {
    independent(b, frame, owner)?;
    // **Lane g14-court-capacity-r2 (past `palw_court_capacity_v1`): the owner fair start.** An Open is never refused for the K_owner
    // cap: past the cap it WAITS in the claim's owner queue (arrival order; `staged_tir_owner_queue_v1`), imposes no custodian load,
    // and is admitted — may demand — as the rows ahead release their slots (turns spent, or an idle admitted row reclaimed: its
    // deposit burned, as below). The concurrent custodian load stays K_owner; an Open filed later never starts before this one.
    let claim = frame.plan_v1().original_claim;
    let fair_start = tir_owner_fair_start_v1(b, ctx);
    if fair_start && b.state.staged_tir_owner_count_v1(&claim) >= Frame::MAXIMUM_OWNERS_V1 {
        // Every admitted idle row is reclaimed (deposit burned) when the new row would wait.
        for (idle_key, idle_owner) in b.state.staged_tir_idle_admitted_owners_v1(&claim, ctx.daa_score) {
            reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;
        }
    }
    if fair_start {
        rotate_owner_queue_v1(b, ctx, frame)?;
    }
    // **K_owner (Lead default, pending user review): refuse an excess owner before ANY custodian duty is created.** The frame's
    // liable custodians answer at most `custodian_load_bound_v1` across the owners holding slots at once; each owner below pays
    // only its own rent and deposit (`owner_terms_v1`), never the producer's prepaid preparation pool.
    //
    // **G-T1 (Lead default R1, idle-slot reclamation; the cap stays K_owner = 8): an idle owner never locks an honest verifier
    // out.** At the cap, this Open reclaims the slot of the holder idle longest (no move for one native acquisition span since its
    // last progress, `owner_idle_at_v1`): that row is retired (its open demand closed, its turns spent — it can never demand again),
    // and its deposit is forfeited (burned through `slash_bond`, credited to nobody, so reclaiming pays no one). Only when every
    // holder is still making progress is the Open refused. Atomic with the Open (the caller's checkpoint).
    if !fair_start && b.state.staged_tir_owner_count_v1(&claim) >= Frame::MAXIMUM_OWNERS_V1 {
        let Some((idle_key, idle_owner)) = b.state.staged_tir_idle_owner_v1(&claim, ctx.daa_score) else {
            return Err(refused("TIR original frame already carries its K_owner outside owners, all still active; custodian load is bounded"));
        };
        reclaim_owner_slot_v1(b, ctx, idle_key, idle_owner)?;
    }
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
    // G-T4: a drawn seat holding no custody grant owns exactly as an outsider does (`independent`); the owned row's own admission
    // keeps refusing an owner inside the panel it is handed, so it is handed the panel less this owner.
    let mut others = frame.custody_v1().drawn_panel_v1().clone();
    others.remove(&owner);
    let mut owned = PalwTirPublicOwnedV1::admit_native_v1(
        c,
        &original_source,
        &others,
        frame.descriptor_v1().native_config,
        frame.descriptor_v1().native_clock,
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
    use crate::palw_tir_public_carriage_v1::PalwTirPublicControlActionV1 as A;
    let full = matches!(control.action, A::Open | A::Convict(_));
    let frame = native_frame(b, control.claim, control.allocation_id, control.source_id, full)?;
    control.validate_native_signature_v1(network_domain, &b.state).map_err(refused)?;
    apply_verified_control_v1(b, ctx, &frame, control)
}

/// Everything a control does once its literal signer's ML-DSA signature verified under the native network domain: the frame is
/// the CURRENT rooted original (`native_frame`). Split out so the fold-level E2E tests drive the real native path with synthetic
/// keys; production reaches it only through [`apply_control_inner_v1`].
fn apply_verified_control_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
    control: &crate::palw_tir_public_carriage_v1::PalwTirPublicControlV1,
) -> Result<(), PalwStateV2Error> {
    // The service candidate adds several rooted writes to one prerequisite
    // transition. Keep the verified-half boundary atomic too (not just its
    // signature-checking caller), including marker/index/quota refusal. This
    // extra candidate checkpoint is not priced or adopted by a real profile.
    if b.state.court_service_holds_claim_v1(&control.claim) {
        let saved = b.try_checkpoint()?;
        let result = apply_verified_control_inner_v1(b, ctx, frame, control);
        if result.is_err() { b.restore(saved); }
        result
    } else {
        apply_verified_control_inner_v1(b, ctx, frame, control)
    }
}
fn apply_verified_control_inner_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
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
            // Lane g14-court-capacity-r2: a WAITING owner row (beyond the first K_owner of the owner queue) demands nothing until the
            // rows ahead release their slots — the queue is rotated first (overstayed rows yield, idle ones are reclaimed).
            if matches!(action, Action::Demand { .. } | Action::Reacquire { .. }) && tir_owner_fair_start_v1(b, ctx) {
                let claim = frame.plan_v1().original_claim;
                if !b.state.staged_tir_owner_admitted_v1(&claim, &key) {
                    rotate_owner_queue_v1(b, ctx, &frame)?;
                }
                if !b.state.staged_tir_owner_admitted_v1(&claim, &key) {
                    return Err(refused(
                        "TIR owner row is waiting in the owner queue: it demands once the rows ahead release their slots",
                    ));
                }
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
        (T::ConvictionWitness, A::WitnessPart(part)) => {
            let rent = if part.offset == 0 { frame.conviction_rent_v1() } else { 0 };
            if control.declared_rent_sompi != rent || control.expected_turn != 0 {
                return Err(refused("TIR staged witness burns the conviction rent at its first part only, and carries no turn"));
            }
            return apply_witness_part_v1(b, ctx, &frame, control.signer, part);
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
    if unpaid.is_some() {
        if let Some(Row::CourtService(service)) = b.state.staged_court_row_v1(&Key::CourtService) {
            if service.claims.contains_key(&frame.plan_v1().original_claim) {
                if let T::Acquisition { owner, reserved_daa } = control.target {
                    let key = owner_key(frame, owner, reserved_daa)?;
                    for actor in unpaid.as_ref().expect("default effect was matched") {
                        let duty = crate::palw_court_tir_requests_v1::tir_native_response_duty_v1(
                            &b.state, ctx, key, *actor, service.profile_id).map_err(refused)?
                            .ok_or_else(|| refused("service default has no outstanding native response duty"))?;
                        if !duty.response_fits_original_hard {
                            return Err(refused("native response exceeds original service hard bound; no party default"));
                        }
                    }
                }
            }
            // G14 lane court-f02 (RFC-0026 F02 / ADR-0192 A): with the explicit inclusion premise in force (the conviction fence
            // and a pinned service profile), attribute from the RECORDED reserved supply and the actors' native duty clocks:
            // supplied-and-silent defaults through the existing path below; unsupplied is a no-slash hold. Otherwise the legacy
            // deny-only guard is unchanged.
            if super::palw_court_service_lifecycle_fold_v1::court_service_f02_active_v1(b) {
                let unpaid = unpaid.as_ref().expect("default effect was matched");
                match service.default_attribution_v1(&b.state, &frame, unpaid, true).map_err(refused)? {
                    crate::palw_court_service_binding_v1::CourtServiceAttributionV1::PartyDefault => {}
                    crate::palw_court_service_binding_v1::CourtServiceAttributionV1::NoSlashHold(why) => return Err(refused(why)),
                }
            } else {
                service.default_hold(&frame).map_err(refused)?;
            }
        }
    }
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
    // Candidate history is recorded only for real prerequisite transitions of
    // an already bound claim. A later binding/read/WorkDebit never backfills it.
    let duty_clock_scope = match (&control.target, &control.action) {
        (T::Acquisition { owner, reserved_daa }, A::Acquisition(action @ (
            Action::Demand { .. } | Action::Reacquire { .. } | Action::Begin { .. } | Action::Part { .. }))) => {
            if let Some(service) = b.state.court_service_state_v1().filter(|s| s.claims.contains_key(&control.claim)) {
                let key = owner_key(frame, *owner, *reserved_daa)?;
                let Some(Row::TirAcquisition(row)) = b.state.staged_court_row_v1(&key) else {
                    return Err(refused("duty clock lost current acquisition"));
                };
                match action {
                    Action::Begin { answerer, .. } | Action::Part { answerer, .. } => {
                        // Only the literal answerer's cursor can change. An
                        // owner's voluntary answer creates no Ready debt.
                        if row.owned.response_custodians_v1().contains(answerer) {
                            let previous = crate::palw_court_tir_requests_v1::tir_native_response_duty_v1(
                                &b.state, ctx, key, *answerer, service.profile_id).map_err(refused)?.map(|d| d.id);
                            Some((key, service.profile_id, row.clone(), Some((*answerer, previous))))
                        } else { None }
                    },
                    _ => {
                        // Successful Demand/Reacquire cannot erase any owed
                        // response (native new_demand enforces this). Their new
                        // demand/turn introduces Begin for every Ready actor.
                        Some((key, service.profile_id, row.clone(), None))
                    },
                }
            } else { None }
        },
        _ => None,
    };
    for change in transition.changes_v1() {
        if b.state.staged_court_row_v1(&change.key) != change.old.as_ref() {
            return Err(refused("TIR transition changes a noncurrent native row"));
        }
    }
    for change in transition.changes_v1() {
        b.write_staged_row_v1(change.key, change.new.clone())?;
    }
    if let Some((key, profile, prior, selected)) = duty_clock_scope {
        // Arc-clone the already admitted immutable owner row, rather than
        // allocate an unreserved vector of all actors/old duty IDs.
        for actor in prior.owned.response_custodians_v1().iter().copied() {
            if selected.is_some_and(|(selected_actor, _)| selected_actor != actor) { continue; }
            let previous_id = selected.and_then(|(_, id)| id);
            if let Some(duty) = crate::palw_court_tir_requests_v1::tir_native_response_duty_v1(
                &b.state, ctx, key, actor, profile).map_err(refused)? {
                if previous_id == Some(duty.id) { continue; }
                let clock = crate::palw_court_duty_clock_v1::CourtDutyClockV1::from_native(
                    &duty, ctx, profile, frame.plan_v1().terms.material_until_daa).map_err(refused)?;
                b.write_staged_row_v1(Key::CourtDutyClock(duty.id), Some(Row::CourtDutyClock(Box::new(clock))))?;
            }
        }
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
    use crate::palw_tir_objective_v1::{
        PalwTirObjectiveOutcomeV1 as Arith, PalwTirOutputFaultV1 as OutFault, PalwTirOutputOutcomeV1 as Out,
    };
    use crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1 as E;
    let claim_id = frame.plan_v1().original_claim;
    let terms = frame.plan_v1().terms;
    // G14 r5 (item 1): the door is the claim's clock-v2 door when it governs (never shorter than the signed one).
    if !crate::palw_court_clock_v2::palw_route_door_open_v2(
        b.params,
        b.state.legacy_disputes.get(&claim_id),
        b.state.claim(&claim_id),
        palw_da_disclose_window_daa_v1(b.params),
        terms.accepted_daa,
        terms.collect_until_daa,
        ctx.daa_score,
    ) {
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
            // The ORIGINAL job's own selection rule — greedy, seeded Decision 11 sampling, or the V4 pipeline — never the filer's.
            let rule = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&frame.original_v1().job).ok_or_else(|| {
                refused("TIR original job's selection is a constraint mask or a tail; the frame's selection door does not judge it")
            })?;
            match crate::palw_tir_objective_v1::adjudicate_original_decode_v1(
                &b.state,
                claim_id,
                proof,
                &native.court,
                frame.descriptor_v1().maximum_leaves,
                native.prompt_form,
                frame.plan_v1().terminal.maximum_witness_bytes,
                &rule,
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
                    return Err(refused(
                        "TIR nominated selection holds under the job's rule: no conviction, and no finding of claim honesty",
                    ));
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
                // G14 R4 (the stop rule): the authenticated answer matches the claim, but it does not END where the job's own
                // decoder ends it (past a completed stop sequence, short of the budget with no stop, or a banned id). The committed
                // trace itself carries the over-long or short answer, so the execution is false (forfeit by root).
                Out::OutputHolds { .. } => {
                    match crate::palw_tir_objective_v1::original_stop_rule_fault_v1(
                        &frame.original_v1().job,
                        original_source.vocab_v1(),
                        authenticated.ids_v1(),
                    ) {
                        Some(_) => Some(claim.execution_root),
                        None => {
                            return Err(refused(
                                "TIR original output identity and stop rule hold: no conviction, and no finding of claim honesty",
                            ));
                        }
                    }
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
    // **G14 G-T7 / G-13 (TIR side): every credited Valid seat is charged on its own native S4 lock, never skipped silently.** The
    // shared funnel (`charge_covering_signers_v1`) skips a signer whose lock is absent; here the conviction asserts first that each
    // credited seat whose false-Valid offence is not yet consumed still holds its lock on this claim, unexpired at this block (the
    // grant dated it to `bond_exit_after_daa` ≥ the door's close, and the paths that could drop it before the door closes keep a
    // frame-credited lock: `palw_tir_public_frame_keeps_lock_v1`). Same assert as the DA side (`apply_verified_control_v1`).
    for seat in &credited {
        let offence = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id);
        if b.state.consumed_offences.contains_key(&offence) {
            continue;
        }
        if !b.state.slashable_locks.get(&(*seat, claim_id)).is_some_and(|lock| lock.amount > 0 && lock.expiry_daa > ctx.daa_score) {
            return Err(refused("TIR credited Valid seat lost its unconsumed native S4 lock before the door closed; no silent skip"));
        }
    }
    // 1. Each credited Valid custodian: S4, lock first (consumed keys skip — once across every route).
    if !credited.is_empty() {
        let conv = b.open_conviction_v1(&credited, ctx.daa_score);
        let g = b.claim_g_v1(&claim_id).map(|gains| gains.g()).unwrap_or(0);
        b.charge_covering_signers_v1(&conv, claim_id, &credited, g)?;
    }
    // 2. The producer, through the one funnel for a proven verdict (void / Final reversal, Gate C revocation, CourtConviction).
    if b.state.court_service_holds_claim_v1(&claim_id) {
        // The assembled evidence is retained, not just the last fragment or a digest.
        let previous = b.court_end_evidence.clone();
        b.prepare_tir_court_end_cause_v1(ctx, claim_id, filer, evidence)?;
        let result = b.convict_by_court_verdict_v1(ctx, claim_id, &claim, filer, None);
        b.court_end_evidence = previous;
        result?;
    } else {
        b.convict_by_court_verdict_v1(ctx, claim_id, &claim, filer, None)?;
    }
    // 3. An execution-proving fault forfeits by ROOT (siblings sharing the convicted execution).
    if let Some(root) = by_root {
        b.forfeit_minted_round_rights(&root)?;
    }
    Ok(())
}

/// **G14 R7: one in-order part of a staged conviction witness** — the split carriage of a witness larger than one control, never
/// a truncation. Same window and filer rule as a direct filing; the declared whole is bounded by the frame's own prepaid witness
/// ceiling (`terminal.maximum_witness_bytes`, the envelope the conviction rent already prices in 16 KiB parts). Parts append in
/// order to the (claim, filer) row; the part that completes the declared bytes authenticates the assembly against its digest,
/// decodes it ([`crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1::decode_staged_v1`]) and adjudicates it
/// through [`apply_conviction_v1`] exactly as a direct filing — atomically: a holding or refused witness rolls the whole part
/// back (the incomplete row then waits, rent already burned, until the door's close sweeps it).
fn apply_witness_part_v1(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    frame: &Frame,
    filer: PalwBondKeyV2,
    part: &crate::palw_tir_public_carriage_v1::PalwTirPublicWitnessPartV1,
) -> Result<(), PalwStateV2Error> {
    use crate::palw_tir_public_carriage_v1::{PalwTirPublicWitnessRowV1 as Witness, palw_tir_public_witness_digest_v1};
    part.check_v1().map_err(refused)?;
    let claim_id = frame.plan_v1().original_claim;
    let terms = frame.plan_v1().terms;
    if !crate::palw_court_clock_v2::palw_route_door_open_v2(
        b.params,
        b.state.legacy_disputes.get(&claim_id),
        b.state.claim(&claim_id),
        palw_da_disclose_window_daa_v1(b.params),
        terms.accepted_daa,
        terms.collect_until_daa,
        ctx.daa_score,
    ) {
        return Err(refused("TIR staged witness is outside the frame's original collection door"));
    }
    conviction_filer_v1(b, frame, filer)?;
    if part.total_bytes > frame.plan_v1().terminal.maximum_witness_bytes {
        return Err(refused("TIR staged witness exceeds the frame's prepaid witness ceiling"));
    }
    let key = Key::TirWitness(claim_id, filer);
    let existing = match b.state.staged_court_row_v1(&key) {
        Some(Row::TirWitness(row)) => Some(row.as_ref().clone()),
        Some(_) => return Err(refused("TIR staged witness key holds another row")),
        None => None,
    };
    let mut row = match (existing.clone(), part.offset) {
        (None, 0) => Witness {
            claim: claim_id,
            filer,
            allocation_id: frame.plan_v1().allocation_id,
            total_bytes: part.total_bytes,
            digest: part.digest,
            until_daa: terms.collect_until_daa,
            received: Vec::new(),
        },
        (Some(row), offset) if offset != 0 => row,
        _ => return Err(refused("TIR staged witness part is out of order (a witness opens once, at offset 0)")),
    };
    if (row.total_bytes, row.digest, row.allocation_id, row.received.len() as u64)
        != (part.total_bytes, part.digest, frame.plan_v1().allocation_id, part.offset)
    {
        return Err(refused("TIR staged witness part changes its declared whole or skips accepted bytes"));
    }
    row.received.extend_from_slice(&part.bytes);
    if (row.received.len() as u64) < row.total_bytes {
        row.check_structure_v1(claim_id, filer).map_err(refused)?;
        b.write_staged_row_v1(key, Some(Row::TirWitness(Box::new(row).into())))?;
        return Ok(());
    }
    // Complete: the assembly must be exactly the declared witness, then it is ONE conviction filing.
    if palw_tir_public_witness_digest_v1(&row.received) != row.digest {
        return Err(refused("TIR staged witness does not assemble to its declared digest"));
    }
    let evidence =
        crate::palw_tir_public_carriage_v1::PalwTirPublicConvictionEvidenceV1::decode_staged_v1(&row.received).map_err(refused)?;
    if existing.is_some() && !b.state.court_service_holds_material_v1(&key) {
        b.write_staged_row_v1(key, None)?;
    }
    // A recovery hold retains the existing partial row, without changing the
    // V1 codec's ban on completed rows. It must not prevent the assembled,
    // verified evidence from reaching adjudication. Terminal release/archive
    // of that retained prefix still needs the adopted recovery contract.
    apply_conviction_v1(b, ctx, frame, filer, &evidence)
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
    // A custody write: the whole frame is checked (here and again before it is written back).
    let shared = native_frame(b, movement.claim, movement.allocation_id, movement.source_id, true)?;
    let mut frame: Frame = (*shared).clone();
    movement.validate_native_signature_v1(network_domain, &b.state).map_err(refused)?;
    if b.state.panel_duties_of(&movement.claim).and_then(|d| d.get(&movement.signer)).copied() != Some(0) {
        return Err(refused("TIR raw grant must carry the literal newly credited Valid in this same atomic object"));
    }
    // G-T4: owner and custodian stay disjoint — a drawn seat that opened an owner acquisition on this claim cannot also grant.
    if b.state.staged_tir_acquisitions_of_v1(&movement.claim).iter().any(|(owner, _)| *owner == movement.signer) {
        return Err(refused("TIR seat owns an acquisition on this claim; an owner cannot also grant its custody"));
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
    include!("palw_court_shared_output_native_tests.rs");
    include!("palw_court_shared_selection_native_tests.rs");
    include!("palw_court_shared_numerical_material_native_tests.rs");
    include!("palw_prefix_material_native_tests.rs");
    include!("palw_court_shared_prefix_family_native_tests.rs");
    include!("palw_court_original_aggregate_native_tests.rs");
    include!("palw_root_input_material_native_tests.rs");
    include!("palw_court_shared_constraint_native_tests.rs");
    include!("palw_court_shared_tokenizer_native_tests.rs");
    include!("palw_court_shared_material_intake_native_tests.rs");
    use super::*;
    use crate::palw_tir_public_carriage_v1::{
        PalwTirPublicControlActionV1 as A, PalwTirPublicControlTargetV1 as T, PalwTirPublicControlV1 as Control,
        PalwTirPublicConvictionEvidenceV1 as Evidence,
    };
    use crate::palw_tir_public_claim_v1::{PalwTirPublicFrameFixtureV1, palw_tir_public_frame_fixture_v1};
    use crate::palw_tir_public_custody_v1::{
        PalwTirPublicCustodyGrantV1, PalwTirPublicOriginalCreditV1, PalwTirPublicOriginalPanelV1,
    };

    const FORGED_LEAF: usize = 7;
    const COLLATERAL: u64 = 1_000_000_000_000;
    const RESERVED: u128 = 7_000;
    const LOCK: u128 = 5_000;
    const SIG: usize = crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN;

    fn h(n: u64) -> Hash64 {
        Hash64::from_u64_word(n)
    }
    fn numerical_parts(control:crate::palw_court_numerical_transport_v1::NumericalControlRefV1<'_>,scope:Hash64,
        rules:&crate::palw_court_close_receipt_v1::NativeCourtRulesV1)
        ->(crate::palw_court_numerical_transport_v1::NumericalManifestV1,Vec<crate::palw_court_numerical_transport_v1::NumericalPartV1>) {
        use crate::palw_court_numerical_transport_v1::*;
        let m=describe_numerical_v1(&control,scope,rules).unwrap();
        assert_eq!(borsh::from_slice::<NumericalManifestV1>(&borsh::to_vec(&m).unwrap()).unwrap(),m);
        let mut parts=Vec::new();
        write_numerical_parts_v1(&control,&m,rules,|part|{
            let bytes=borsh::to_vec(&part)?;
            assert!(bytes.len()<=crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1 as usize);
            parts.push(borsh::from_slice::<NumericalPartV1>(&bytes)?);Ok(())
        }).unwrap();
        // Joined bytes exist only in this independent codec oracle, never in
        // the sender/receiver implementation or the native fold adapter.
        let joined:Vec<_>=parts.iter().flat_map(|p|p.bytes.iter().copied()).collect();
        assert_eq!(joined,borsh::to_vec(&control).unwrap());
        (m,parts)
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
        credited_frame_with(fx, &colluders(), &drawn())
    }
    /// [`credited_frame`] over `credited` seats (each with an exact credited Valid custody grant + Ready) of the drawn Panel `drawn`.
    fn credited_frame_with(fx: &PalwTirPublicFrameFixtureV1, credited: &[PalwBondKeyV2], drawn: &[PalwBondKeyV2]) -> Frame {
        let frame = fx.frame.clone();
        let plan = frame.plan_v1().clone();
        let header = frame.header_v1().clone();
        let original = frame.original_v1().clone();
        let pledge = frame.custody_v1().pledge_v1().clone();
        let bound = plan.terms.accepted_daa;
        let panel = PalwTirPublicOriginalPanelV1 { anchor: h(0xA0), bound_daa: bound, seats: drawn.iter().copied().collect() };
        let credited = credited
            .iter()
            .copied()
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
                let ready = crate::palw_tir_public_owned_v1::PalwTirPublicReadyV1 {
                    descriptor,
                    accepted_daa: bound + 2,
                    work_receipt: h(0xE1),
                };
                (bound + 2, grant, PalwTirPublicOriginalCreditV1 { receipt, ready })
            })
            .collect();
        let custody = frame.original_custody_v1().clone().with_credits_for_tests_v1(panel, credited).unwrap();
        frame.with_custody_for_tests_v1(custody).unwrap()
    }

    /// The accepted world the colluders built: the licensed claim, its drawn Panel, the colluders' native S4 locks, the registered
    /// class and the rooted frame (its indices — producer exposure and the liable signers' exit holds — rebuilt from the rows).
    fn world(fx: &PalwTirPublicFrameFixtureV1, frame: &Frame) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
        world_with(fx, frame, &drawn())
    }
    /// [`world`] with the drawn Panel `drawn`.
    fn world_with(
        fx: &PalwTirPublicFrameFixtureV1,
        frame: &Frame,
        drawn: &[PalwBondKeyV2],
    ) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1) {
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
        for (n, key) in std::iter::once(producer).chain(drawn.iter().copied()).chain(std::iter::once(outsider())).enumerate() {
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
        // The only Active class holds the whole share table (ADR-0045: a populated table sums to 1000‰) — the carriage's invariant.
        state.class_shares.insert(class_id, 1000);
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
            // A licensed claim past R-core+ records its licence door (the carriage's load invariant): the V1 quorum, counted.
            rcore: PalwClaimRcoreV1 {
                licence_door: Some(crate::palw_economic_safety_v1::PalwLicenceDoorTagV1::Quorum),
                basis_k: PALW_RCORE_FINAL_BASIS_K_V1,
                ..PalwClaimRcoreV1::default()
            },
        };
        let held = palw_claim_commitment_v1(&params, &claim, plan.terms.accepted_daa + 3).unwrap();
        state.reserved_exposure.insert(producer, held);
        let panel = PalwPanelStateV2 {
            anchor: h(0xA0),
            seats: drawn.iter().map(|seat| PalwPanelSeatV2 { bond: *seat, operator_id: h(0x77) }).collect(),
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
        state
            .staged_courts
            .insert(frame.key_v1(), crate::palw_staged_court_state_v1::PalwStagedCourtRowV1::TirClaim(Box::new(frame.clone()).into()));
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let extras = PalwTransitionExtrasV1 {
            tir_public_allocation_rules_v1: Some(fx.rules),
            tir_public_network_domain_v1: Some(h(0x4E7)),
            ..Default::default()
        };
        (state, params, extras)
    }

    include!("palw_funded_original_native_fixture_tests.rs");

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
            .chain(
                drawn().into_iter().map(|seat| crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&seat.0, &claim_id)),
            )
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
            assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, liable, cone(&fx))).is_err());
            assert_eq!(b.state.state_root(), root_before);
        }

        // One bond OUTSIDE the Panel convicts, with no Panel signature, vote or majority.
        let mut outside = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut outside, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).unwrap();
        let (phase, collateral, consumed) = outcome(&outside.state, &frame);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert!(collateral[0] < COLLATERAL, "the producer is actually debited (its reservation at least)");
        assert!(collateral[1] <= COLLATERAL - LOCK as u64 && collateral[2] <= COLLATERAL - LOCK as u64, "each colluding Valid: S4");
        assert_eq!(collateral[3], COLLATERAL, "the honest drawn seat that signed no Valid is never charged");
        assert_eq!(
            consumed,
            vec![true, true, true, false],
            "CourtConviction once; each colluder's false-Valid once; never the honest seat"
        );
        for seat in colluders() {
            assert!(!outside.state.slashable_locks.contains_key(&(seat, claim_id)), "the lock was taken, not left to expire");
        }
        // Standing refused: a voided claim is never Final, so no receipt spend (block opportunity), Final weight, escrow, model
        // allocation or Gate C credit can rest on it; its execution root's minted rights are forfeit by root.
        assert!(!matches!(outside.state.claims.get(&claim_id).unwrap().phase, PalwClaimPhaseV2::Final { .. }));

        // Once only: the same or another filer, the same or another valid proof, finds the conviction recorded.
        let again = apply_verified_control_v1(&mut outside, &at(&frame, 6), &frame, &filing(&frame, honest_seat(), cone(&fx)));
        assert!(again.is_err());

        // The honest drawn seat files the SAME object and reaches the SAME verdict (the common filer).
        let mut seat = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut seat, &at(&frame, 5), &frame, &filing(&frame, honest_seat(), cone(&fx))).unwrap();
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
        assert!(
            apply_verified_control_v1(&mut late, &at(&frame, door), &frame, &filing(&frame, outsider(), cone(&fx))).is_err()
        );
    }

    #[test]
    fn g14_honest_twin_is_not_convicted_and_a_holding_predicate_certifies_nothing() {
        // The same leaf of an HONEST execution: the primitive holds — the filing is refused and changes nothing.
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = credited_frame(&fx);
        let (state, params, extras) = world(&fx, &frame);
        let root = state.state_root();
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).is_err());
        assert_eq!(b.state.state_root(), root);
        assert!(matches!(
            b.state.claims.get(&frame.plan_v1().original_claim).unwrap().phase,
            PalwClaimPhaseV2::ReceiptLicensed { .. }
        ));
        // A wrong burn is refused before adjudication (the frame's native terminal rent is exact).
        let mut cheap = filing(&frame, outsider(), cone(&fx));
        cheap.declared_rent_sompi = frame.conviction_rent_v1() - 1;
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &cheap).is_err());
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
            apply_verified_control_v1(&mut b, &at(&honest, 5), &honest, &filing(&honest, outsider(), output(ids.clone())))
                .is_err()
        );
        // A truncated sequence authenticates nothing.
        let mut short = ids.clone();
        short.pop();
        if !short.is_empty() {
            assert!(
                apply_verified_control_v1(&mut b, &at(&honest, 5), &honest, &filing(&honest, outsider(), output(short)))
                    .is_err()
            );
        }
        assert_eq!(b.state.state_root(), root);
        // The lying original: convicted, CourtFraud, colluding Valid custodians charged, honest seat untouched.
        let lying_fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(4, h(0xBAD));
        let lying = credited_frame(&lying_fx);
        let (state, params, extras) = world(&lying_fx, &lying);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &filing(&lying, outsider(), output(ids.clone()))).unwrap();
        let (phase, collateral, consumed) = outcome(&b.state, &lying);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert_eq!(collateral[3], COLLATERAL);
        assert_eq!(consumed, vec![true, true, true, false]);
        // The claim RECORD retaining a different output root than the frame's literal original is refused, never overridden.
        let (mut state, params, extras) = world(&lying_fx, &lying);
        state.claims.get_mut(&lying.plan_v1().original_claim).unwrap().output_root = h(0xBAE);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &filing(&lying, outsider(), output(ids))).is_err());
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
        install_original_v1(&mut b, &accept, frame.clone(), &prompt_row(&fx, &frame)).unwrap();
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
                action: A::Preparation(crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1::PreparationCollect {
                    actor: producer,
                }),
                signature: vec![5; SIG],
            };
            control.check_v1().unwrap();
            let result = apply_verified_control_v1(&mut b, &at(&frame, offset), &frame, &control);
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

    /// **G14 R7: a witness carried in parts is assembled in order and adjudicated once, by the part that completes it** — the
    /// first part burns the conviction rent, nothing is judged before the declared whole lands, an out-of-order or rent-free first
    /// part is refused, and the completed witness convicts exactly as the same evidence filed directly (the row is consumed).
    #[test]
    fn g14_staged_witness_assembles_in_order_and_convicts_once() {
        staged_witness_with_service_hold(false);
    }
    #[test]
    fn rooted_service_hold_does_not_block_streamed_fraud_conviction() {
        staged_witness_with_service_hold(true);
    }
    fn staged_witness_with_service_hold(service_hold: bool) {
        use crate::palw_tir_public_carriage_v1::{PalwTirPublicWitnessPartV1 as Part, palw_tir_public_witness_digest_v1};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let ids = fx.execution.generated.clone();
        let rows_root =
            crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
        let lying_fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(4, h(0xBAD));
        let lying = credited_frame(&lying_fx);
        let (state, params, extras) = world(&lying_fx, &lying);
        let encoded = borsh::to_vec(&Evidence::Output { rows_root, ids }).unwrap();
        assert!(encoded.len() as u64 <= lying.plan_v1().terminal.maximum_witness_bytes);
        let digest = palw_tir_public_witness_digest_v1(&encoded);
        let split = encoded.len() / 2;
        let part = |offset: usize, end: usize| {
            let plan = lying.plan_v1();
            Control {
                version: 1,
                claim: plan.original_claim,
                allocation_id: plan.allocation_id,
                source_id: plan.source_id,
                target: T::ConvictionWitness,
                expected_turn: 0,
                declared_rent_sompi: if offset == 0 { lying.conviction_rent_v1() } else { 0 },
                signer: outsider(),
                action: A::WitnessPart(Part {
                    total_bytes: encoded.len() as u64,
                    digest,
                    offset: offset as u64,
                    bytes: encoded[offset..end].to_vec(),
                }),
                signature: vec![5; SIG],
            }
        };
        let claim_id = lying.plan_v1().original_claim;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        if service_hold {
            use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
            use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
            let plan = lying.plan_v1();
            let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
            let mut book = ServiceReservations::new(cost).unwrap();
            book.reserve(&[ServicePacket { id: [31; 64], release_daa: plan.terms.accepted_daa + 1,
                service_before: plan.terms.accepted_daa + 2, cost }], plan.terms.accepted_daa + 10_000).unwrap();
            book.observe_block(plan.terms.accepted_daa + 2, [32; 64], Default::default()).unwrap();
            let service = CourtServiceStateV2 { version: 2, source_profile_id: h(33), profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(h(33)), book,
                claims: std::collections::BTreeMap::from([(claim_id, ServiceClaimBinding { claim: claim_id,
                    allocation_id: plan.allocation_id, source_id: plan.source_id, original_accepted_daa: plan.terms.accepted_daa,
                    hard_deadline_daa: plan.terms.accepted_daa + 10_000,
                    packet_ids: std::collections::BTreeSet::from([[31; 64]]) })]) };
            b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(service).into()))).unwrap();
        }
        assert!(apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &part(split, encoded.len())).is_err());
        let mut free = part(0, split);
        free.declared_rent_sompi = 0;
        assert!(apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &free).is_err());
        apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &part(0, split)).unwrap();
        assert!(b.state.staged_court_row_v1(&Key::TirWitness(claim_id, outsider())).is_some());
        assert!(matches!(b.state.claims.get(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        apply_verified_control_v1(&mut b, &at(&lying, 6), &lying, &part(split, encoded.len())).unwrap();
        let (phase, _, consumed) = outcome(&b.state, &lying);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert_eq!(consumed, vec![true, true, true, false]);
        let saved = b.state.staged_court_row_v1(&Key::TirWitness(claim_id, outsider()));
        if service_hold {
            let Some(Row::TirWitness(row)) = saved else { panic!("recovery retains the existing partial witness") };
            assert_eq!(row.received, encoded[..split]);
            row.check_structure_v1(claim_id, outsider()).unwrap();
        } else {
            assert!(saved.is_none(), "legacy consumes the witness as before");
        }
    }

    /// **G14 R8: a fresh node restored from the pruning-point carriage continues the ORIGINAL frame identically** — not only an
    /// index rebuild: the colluders' accepted world with a witness half-carried before the pruning point is imported through the
    /// IBD wire (`PalwStateCarriageV2` bytes → `into_state` with the archival root), every TIR index equals the archival node's,
    /// the frame, its d0 door and the in-flight witness read the same, and the completing part reaches the same conviction, the
    /// same slashes and the same state root on both nodes.
    #[test]
    fn g14_fresh_node_import_continues_the_original_frame_identically() {
        use crate::palw_tir_public_carriage_v1::{PalwTirPublicWitnessPartV1 as Part, palw_tir_public_witness_digest_v1};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let ids = fx.execution.generated.clone();
        let rows_root =
            crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context, &fx.execution.rows).unwrap();
        let lying_fx = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(4, h(0xBAD));
        let lying = credited_frame(&lying_fx);
        let (state, params, extras) = world(&lying_fx, &lying);
        let claim_id = lying.plan_v1().original_claim;
        let encoded = borsh::to_vec(&Evidence::Output { rows_root, ids }).unwrap();
        let digest = palw_tir_public_witness_digest_v1(&encoded);
        let split = encoded.len() / 2;
        let part = |offset: usize, end: usize| {
            let plan = lying.plan_v1();
            Control {
                version: 1,
                claim: plan.original_claim,
                allocation_id: plan.allocation_id,
                source_id: plan.source_id,
                target: T::ConvictionWitness,
                expected_turn: 0,
                declared_rent_sompi: if offset == 0 { lying.conviction_rent_v1() } else { 0 },
                signer: outsider(),
                action: A::WitnessPart(Part {
                    total_bytes: encoded.len() as u64,
                    digest,
                    offset: offset as u64,
                    bytes: encoded[offset..end].to_vec(),
                }),
                signature: vec![5; SIG],
            }
        };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut b, &at(&lying, 5), &lying, &part(0, split)).unwrap();
        let archival = b.state.clone();
        let bytes = borsh::to_vec(&crate::palw_state_v2::PalwStateCarriageV2::from_state(&archival)).unwrap();
        let fresh = borsh::from_slice::<crate::palw_state_v2::PalwStateCarriageV2>(&bytes)
            .unwrap()
            .into_state(&params, Some(archival.state_root()))
            .expect("a fresh node imports the accepted frame world from the pruning-point carriage");
        assert_eq!(fresh.state_root(), archival.state_root());
        fresh.assert_tir_acquisition_indices_v1().unwrap();
        assert_eq!(fresh.staged_due, archival.staged_due);
        assert_eq!(fresh.staged_holds, archival.staged_holds);
        assert_eq!(fresh.staged_tir_claims_by_producer, archival.staged_tir_claims_by_producer);
        let now = lying.plan_v1().terms.accepted_daa + 6;
        assert_eq!(
            fresh.palw_tir_public_prosecution_open_v1(&claim_id, now),
            archival.palw_tir_public_prosecution_open_v1(&claim_id, now)
        );
        assert_eq!(palw_tir_public_claim_v1(&fresh, claim_id), palw_tir_public_claim_v1(&archival, claim_id));
        assert_eq!(
            fresh.staged_court_row_v1(&Key::TirWitness(claim_id, outsider())),
            archival.staged_court_row_v1(&Key::TirWitness(claim_id, outsider()))
        );
        let roots: Vec<Hash64> = [&archival, &fresh]
            .into_iter()
            .map(|node| {
                let mut b = TransitionBuilder::new(node, &params, false, false, false, false, &extras);
                apply_verified_control_v1(&mut b, &at(&lying, 6), &lying, &part(split, encoded.len())).unwrap();
                let (phase, _, consumed) = outcome(&b.state, &lying);
                assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
                assert_eq!(consumed, vec![true, true, true, false]);
                b.state.state_root()
            })
            .collect();
        assert_eq!(roots[0], roots[1], "the restored node reaches the archival node's exact post-conviction root");
    }

    /// The producer's honest preparation on the claim-wide lane, after the original acceptance installed it: one paid capture
    /// debit, then its own `PreparationReady` over a descriptor of the original roots.
    fn producer_ready(
        frame: &Frame,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
    ) -> PalwChainStateV2 {
        use crate::palw_tir_public_owned_v1::{
            PalwTirPublicOwnedActionV1 as Owned, PalwTirPublicWorkDebitV1, PalwTirPublicWorkPhaseV1, PalwTirPublicWorkScopeV1,
            PalwTirPublicWorkV1,
        };
        let plan = frame.plan_v1();
        let producer = frame.custody_v1().pledge_v1().producer;
        let header = frame.header_v1();
        let original = frame.original_v1();
        let scope = PalwTirPublicWorkScopeV1 {
            claim: plan.original_claim,
            owner: producer,
            reserved_daa: plan.terms.accepted_daa,
            bundle_id: plan.allocation_id,
            source_id: plan.source_id,
            turn: 0,
            request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 },
            actor: producer,
            phase: PalwTirPublicWorkPhaseV1::Capture,
            input_head: plan.source_id,
            input_offset: 0,
            slice: 0,
            cursor_id: h(9),
        };
        let debit = PalwTirPublicWorkDebitV1 {
            expected_nonce: 0,
            previous_head: Hash64::default(),
            scope,
            work: PalwTirPublicWorkV1 { attempts: 1, parser_steps: 1, ..Default::default() },
            restart: false,
            history: None,
        };
        let prep = |action: Owned| Control {
            version: 1,
            claim: plan.original_claim,
            allocation_id: plan.allocation_id,
            source_id: plan.source_id,
            target: T::Preparation { original_accepted_daa: plan.terms.accepted_daa },
            expected_turn: 0,
            declared_rent_sompi: 0,
            signer: producer,
            action: A::Preparation(action),
            signature: vec![5; SIG],
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
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        apply_verified_control_v1(&mut b, &at(frame, 1), &frame, &prep(Owned::WorkDebit(debit))).unwrap();
        apply_verified_control_v1(&mut b, &at(frame, 2), &frame, &prep(Owned::PreparationReady { descriptor })).unwrap();
        b.state.clone()
    }

    /// The frame installed at its own acceptance (claim-wide preparation beside it), its producer Ready, and an outside owner's
    /// accepted Open and Demand of one original step leaf. Returns the state after the Demand, the owner's acquisition target
    /// and the step offset (from the original acceptance) the custodian answers from.
    fn demanded_world(
        fx: &PalwTirPublicFrameFixtureV1,
        frame: &Frame,
        leaf: u64,
    ) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, T, u64) {
        demanded_world_inner(fx, frame, leaf, false)
    }
    fn demanded_world_inner(
        fx: &PalwTirPublicFrameFixtureV1, frame: &Frame, leaf: u64, bind_before_demand: bool,
    ) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, T, u64) {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let (mut state, params, extras) = world(fx, frame);
        state.staged_courts.remove(&frame.key_v1());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        install_original_v1(&mut b, &at(frame, 0), frame.clone(), &prompt_row(fx, frame)).unwrap();
        let installed = b.state.clone();
        let ready = producer_ready(frame, &installed, &params, &extras);
        let plan = frame.plan_v1();
        let open_at = plan.preparation_until_daa - plan.terms.accepted_daa;
        let rent = owner_terms_v1(frame).unwrap().plan.rent_sompi;
        let opened =
            control_step(frame, &ready, &params, &extras, open_at, outsider(), T::OwnerOpen { owner: outsider() }, A::Open, rent, 0)
                .expect("an outside bond opens its own acquisition after the preparation door");
        let opened = if bind_before_demand { bind_clock_service(&opened, frame, &params, &extras) } else { opened };
        let acquisition = T::Acquisition { owner: outsider(), reserved_daa: plan.terms.accepted_daa + open_at };
        let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: leaf };
        let demanded = control_step(
            frame,
            &opened,
            &params,
            &extras,
            open_at + 1,
            outsider(),
            acquisition,
            A::Acquisition(Owned::Demand { request }),
            0,
            0,
        )
        .expect("the owner demands one original step leaf from the frame's Ready custodians");
        (demanded, params, extras, acquisition, open_at + 2)
    }

    /// One verified control at `offset` DAA past the original acceptance; the new state on success.
    #[allow(clippy::too_many_arguments)]
    fn control_step(
        frame: &Frame,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        offset: u64,
        signer: PalwBondKeyV2,
        target: T,
        action: A,
        rent: u64,
        turn: u64,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let plan = frame.plan_v1();
        let control = Control {
            version: 1,
            claim: plan.original_claim,
            allocation_id: plan.allocation_id,
            source_id: plan.source_id,
            target,
            expected_turn: turn,
            declared_rent_sompi: rent,
            signer,
            action,
            signature: vec![5; SIG],
        };
        let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
        apply_verified_control_v1(&mut b, &at(frame, offset), &frame, &control).map(|()| b.state.clone())
    }

    /// The owner's open demand on `frame`: `(demand, respond_until, turn, producer's finished body)`.
    fn owner_pending(
        frame: &Frame,
        state: &PalwChainStateV2,
        acquisition: T,
    ) -> (Hash64, u64, u64, Option<crate::palw_tir_public_owned_v1::PalwTirPublicAcceptedBodyRefV1>) {
        let T::Acquisition { owner, reserved_daa } = acquisition else { unreachable!() };
        let key = owner_key(frame, owner, reserved_daa).unwrap();
        let producer = frame.custody_v1().pledge_v1().producer;
        let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else { panic!("the owner's acquisition row") };
        let p = row.owned.active_v1().expect("an open demand");
        (p.demand_id, p.respond_until_daa, p.turn, p.slots.get(&producer).and_then(|slot| slot.finished.clone()))
    }

    #[test]
    fn transport_budget_counts_signed_native_parts_and_preserves_old_claims() {
        use crate::palw_court_capacity_v1::{PalwCourtCapacityMirrorV1, palw_court_capacity_placeholder_policy_v1};
        use crate::palw_court_transport_v1::{PalwCourtTransportFenceV1, PalwCourtTransportMirrorV1, palw_court_transport_cost_v1};
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let producer = frame.custody_v1().pledge_v1().producer;
        let (state, params, extras, acquisition, offset) = demanded_world(&fx, &frame, 0);
        let (demand_id, _, turn, _) = owner_pending(&frame, &state, acquisition);
        let body = borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage: fx.execution.preimages[0].clone(),
            opening: crate::palw_step_leg::step_opening_v1(&fx.execution.hashes, 0).unwrap(),
        }).unwrap();
        assert!(body.len() <= crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1);
        // Trusted fixture setup only; the incoming Part/Finish below use real signatures and native dispatch.
        let mut ready = control_step(&frame, &state, &params, &extras, offset, producer, acquisition,
            A::Acquisition(Owned::Begin { demand_id, answerer: producer, total_bytes: body.len() as u64,
                body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) }), 0, turn).unwrap();
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x42; 32]);
        ready.bonds.get_mut(&producer).unwrap().pubkey = kp.verification_key.as_ref().to_vec();
        let signed = |action| {
            let mut control = Control { version: 1, claim: frame.plan_v1().original_claim,
                allocation_id: frame.plan_v1().allocation_id, source_id: frame.plan_v1().source_id,
                target: acquisition, expected_turn: turn, declared_rent_sompi: 0, signer: producer,
                action: A::Acquisition(action), signature: vec![0; SIG] };
            let digest = control.signing_message_v1(extras.tir_public_network_domain_v1.unwrap()).unwrap();
            control.signature = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, digest.as_byte_slice(),
                crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, [0x43; 32]).unwrap().as_ref().to_vec();
            PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control) }
        };
        let part = signed(Owned::Part { demand_id, answerer: producer, offset: 0, bytes: body });
        let finish = signed(Owned::Finish { demand_id, answerer: producer });
        let ctx = at(&frame, offset + 1);
        let mirror = PalwCourtTransportMirrorV1 { from_daa: frame.plan_v1().terms.accepted_daa,
            policy: PalwCourtTransportFenceV1::unapproved_probe_at(crate::config::params::ForkActivation::never()).policy };
        let cost = palw_court_transport_cost_v1(&ready, Some(mirror), ctx.daa_score, &part).unwrap().unwrap();
        let mut policy = palw_court_capacity_placeholder_policy_v1();
        policy.block_budget = cost;
        policy.one_move_budget = Default::default();
        let active = params.clone().with_court_capacity_v1(Some(PalwCourtCapacityMirrorV1 { from_daa: 0, policy }))
            .with_court_transport_v1(Some(mirror));
        let mut b = TransitionBuilder::new(&ready, &active, false, false, false, false, &extras);
        super::super::apply_object(&mut b, &ctx, &part).expect("native signed Part fits exactly");
        assert_eq!(b.court_cost_spent, cost);
        assert_eq!(b.court_one_move_spent, Default::default());
        let before = b.state.clone();
        let error = super::super::apply_object(&mut b, &ctx, &finish).unwrap_err();
        assert!(format!("{error:?}").contains("CourtCapacityRefused"));
        assert_eq!(b.state, before);
        assert!(owner_pending(&frame, &b.state, acquisition).3.is_none());
        // A dormant fence or a claim originally accepted before it retains the old accounting.
        for old in [None, Some(PalwCourtTransportMirrorV1 { from_daa: mirror.from_daa + 1, ..mirror })] {
            let old_params = active.clone().with_court_transport_v1(old);
            let mut b = TransitionBuilder::new(&ready, &old_params, false, false, false, false, &extras);
            super::super::apply_object(&mut b, &ctx, &part).unwrap();
            super::super::apply_object(&mut b, &ctx, &finish).unwrap();
            assert_eq!(b.court_cost_spent, Default::default());
            assert!(owner_pending(&frame, &b.state, acquisition).3.is_some());
        }
    }

    /// The producer answers the open demand from its committed material — Begin, the canonical Part(s), Finish — one accepted
    /// move per DAA from `offset`. Returns the state and the next free offset.
    #[allow(clippy::too_many_arguments)]
    fn producer_answers(
        fx: &PalwTirPublicFrameFixtureV1,
        frame: &Frame,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        acquisition: T,
        leaf: usize,
        offset: u64,
    ) -> (PalwChainStateV2, u64) {
        use crate::palw_court_tir_requests_v1::TirNativeDemandSetV1;
        let producer = frame.custody_v1().pledge_v1().producer;
        let (demand, _, turn, _) = owner_pending(frame, state, acquisition);
        let body = borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage: fx.execution.preimages[leaf].clone(),
            opening: crate::palw_step_leg::step_opening_v1(&fx.execution.hashes, leaf as u64).unwrap(),
        })
        .unwrap();
        let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        // A synthetic namespace for this shape/dispatcher fixture, never an adopted service profile.
        let demands = TirNativeDemandSetV1::new(frame, &source, Hash64::from_u64_word(0x73)).unwrap();
        let graph = demands.response_for(crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index:leaf as u64 }).unwrap();
        let branch = graph.branch(body.len() as u64).unwrap();
        let T::Acquisition { owner, reserved_daa } = acquisition else { unreachable!() };
        let key = owner_key(frame,owner,reserved_daa).unwrap();
        let mut answered = state.clone();
        let mut previous_duty = None;
        for ordinal in 0..branch.len() {
            let point = at(frame,offset+ordinal);
            let duty = crate::palw_court_tir_requests_v1::tir_native_response_duty_v1(
                &answered,&point,key,producer,Hash64::from_u64_word(0x73)).unwrap().unwrap();
            assert_eq!((duty.logical_id,duty.demand_id,duty.turn),(graph.logical_id,demand,turn));
            assert_eq!(duty.clock,crate::palw_court_tir_requests_v1::TirNativeDutyClockV1::ResponseWindow);
            assert!(duty.remaining_response_stages_upper>=branch.len()-ordinal);
            assert_ne!(previous_duty,Some(duty.id)); previous_duty=Some(duty.id);
            let action = branch.action_at(ordinal,demand,producer,&body).unwrap();
            use crate::palw_court_tir_requests_v1::TirNativeDutyStageV1 as Stage;
            use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
            assert!(matches!((&duty.stage,&action),
                (Stage::Begin { .. },Owned::Begin { .. }) |
                (Stage::Part { .. },Owned::Part { .. }) |
                (Stage::Finish { .. },Owned::Finish { .. })));
            answered = control_step(frame,&answered,params,extras,offset+ordinal,producer,acquisition,
                A::Acquisition(action),0,turn).expect("the derived response stage passes the existing native dispatcher");
        }
        assert!(crate::palw_court_tir_requests_v1::tir_native_response_duty_v1(
            &answered,&at(frame,offset+branch.len()),key,producer,Hash64::from_u64_word(0x73)).unwrap().is_none());
        (answered, offset + branch.len())
    }

    #[test]
    fn court_relay_presigned_custody_response_uses_literal_signer_not_carrier_inputs() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        use crate::palw_lifecycle_objects_v2::{PalwLifecycleTxPayloadV2, PALW_LIFECYCLE_TX_VERSION_V2,
            palw_lifecycle_objects_from_accepted_txs_v2};
        use crate::tx::{Transaction, TransactionInput, TransactionOutpoint};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = &fx.frame;
        let (mut state, params, extras, target, offset) = demanded_world(&fx, frame, 0);
        let plan = frame.plan_v1();
        let producer = frame.custody_v1().pledge_v1().producer;
        let network = frame.original_v1().job.network_domain;
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x71; 32]);
        // The original fixture is verified-half. Only these controls use the
        // real registered key and real native signature verification.
        state.bonds.get_mut(&producer).unwrap().pubkey = kp.verification_key.as_ref().to_vec();
        let (demand_id, _, turn, _) = owner_pending(frame, &state, target);
        let bytes = borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage: fx.execution.preimages[0].clone(),
            opening: crate::palw_step_leg::step_opening_v1(&fx.execution.hashes, 0).unwrap(),
        }).unwrap();
        assert!(bytes.len() <= crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1);
        let actions = [Owned::Begin { demand_id, answerer: producer, total_bytes: bytes.len() as u64,
            body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&bytes) },
            Owned::Part { demand_id, answerer: producer, offset: 0, bytes },
            Owned::Finish { demand_id, answerer: producer }];
        let controls: Vec<_> = actions.into_iter().map(|action| {
            let mut c = Control { version: 1, claim: plan.original_claim, allocation_id: plan.allocation_id,
                source_id: plan.source_id, target, expected_turn: turn, declared_rent_sompi: 0,
                signer: producer, action: A::Acquisition(action), signature: vec![0; SIG] };
            c.signature = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,
                c.signing_message_v1(network).unwrap().as_byte_slice(),
                crate::palw_tir_public_carriage_v1::PALW_TIR_PUBLIC_CONTROL_CONTEXT_V1, [0; 32]).unwrap().as_ref().to_vec();
            c
        }).collect();
        drop(kp); // No online signer is used for any later relay operation.
        for (index, control) in controls.iter().enumerate() {
            let payload = borsh::to_vec(&PalwLifecycleTxPayloadV2 { version: PALW_LIFECYCLE_TX_VERSION_V2,
                object: PalwConsensusObjectV2::TirPublicControlV1 { control: Box::new(control.clone()) } }).unwrap();
            let carriers: Vec<_> = [0x81, 0x82].into_iter().map(|payer| Transaction::new(0,
                vec![TransactionInput::new(TransactionOutpoint::new(Hash64::from_u64_word(payer), 0), vec![], 0, 0)],
                vec![], 0, crate::subnets::SUBNETWORK_ID_PALW_LIFECYCLE.clone(), 0, payload.clone())).collect();
            // Codec/native relay check only: these illustrative carrier inputs
            // are not funded or validated as UTXO transactions in this fixture.
            assert_ne!(carriers[0].id(), carriers[1].id());
            let mut roots = Vec::new();
            let mut next = None;
            for tx in carriers {
                assert_ne!(tx.inputs[0].previous_outpoint, producer.0);
                let extracted = palw_lifecycle_objects_from_accepted_txs_v2(&[tx]);
                assert!(extracted.skipped.is_empty());
                let PalwConsensusObjectV2::TirPublicControlV1 { control: carried } = &extracted.objects[0].object else { panic!("control") };
                assert_eq!(carried.as_ref(), control);
                let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
                let before = b.state.state_root();
                assert!(apply_control_v1(&mut b, &at(frame, offset + index as u64), Hash64::default(), carried).is_err());
                assert_eq!(b.state.state_root(), before, "wrong-domain relay cannot mutate state");
                apply_control_v1(&mut b, &at(frame, offset + index as u64), network, carried).unwrap();
                roots.push(b.state.state_root());
                next = Some(b.state.clone());
            }
            assert_eq!(roots[0], roots[1], "carrier identity cannot change the signed court response");
            state = next.unwrap();
        }
        assert!(owner_pending(frame, &state, target).3.is_some(), "relayed Finish authenticates the original body");
    }

    #[test]
    fn original_wide_numerical_inventory_covers_every_native_eligible_leaf() {
        use crate::palw_tir_dissect_v1 as d;
        use crate::palw_tir_fence2_v1::PalwTirDemandRulesV1 as Demand;
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let source = fx.frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(&fx.frame, &source, h(251)).unwrap();
        let binding = &fx.execution.binding;
        let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&binding.class).unwrap();
        let intervals = misaka_palw_tir::interval::analyze_ranges(&space.program).unwrap();
        // Independent reference: every actual leaf at its original coordinate,
        // then every actual native child range, with no grouped occurrence or
        // history arithmetic from the implementation under test.
        let sites: Vec<_> = (0..binding.step_leaf_count).filter_map(|i| {
            let leaf = space.leaf_at(&binding.job_context, i).unwrap();
            d::palw_tir_dissect_site_v1(&space, &intervals, &leaf).map(|s| (leaf, s))
        }).collect();
        assert!(!sites.is_empty());
        assert!(sites.iter().map(|(l, _)| l.position).collect::<std::collections::BTreeSet<_>>().len() > 1);
        let mut counts = Vec::new();
        for arity in [2, 4, 8, 16, 32, 64] {
            let mut reference = d::tree_inventory_v1::NumericalTreeInventoryV1::default();
            let mut signed_reference = 0u64;
            let mut maximum_signed_reference = 0u64;
            for (leaf, site) in &sites {
                let tiles = u64::from(site.history_positions.div_ceil(site.tile_positions));
                let crate::palw_tir_step_v1::PalwTirLeafKindV1::Commit {block, node, ..} = leaf.kind else {panic!("site")};
                let values = d::palw_tir_dissect_value_bound_v2(&space.program, &space.program.blocks[block as usize], node,
                    space.commit_tile_len(block, node).unwrap(), Demand::H7) as usize;
                let mut todo = vec![(0, tiles, 0u32)];
                while let Some((first, n, depth)) = todo.pop() {
                    reference.max_depth = reference.max_depth.max(depth);
                    let children = crate::palw_attn_dissect::palw_attn_child_ranges_v1(first, n, arity).unwrap();
                    if children.is_empty() {
                        reference.terminals += 1;
                        reference.terminal_path_bytes += u64::from(depth);
                        continue;
                    }
                    reference.disclosures += 1;
                    reference.child_claims += children.len() as u64;
                    reference.disclosure_path_bytes += u64::from(depth);
                    // Unsent width exemplars: actual DTO serialization with
                    // native count bounds, not purported valid math/signatures.
                    let row = crate::palw_court_shared_tir_round_v1::SignedSharedTirRoundV1 {
                        version:1, claim:fx.frame.plan_v1().original_claim, leaf:leaf.index,
                        root_id:source.source_id_v1(), path:vec![0; depth as usize], parent_id:source.source_id_v1(),
                        signer:fx.frame.custody_v1().pledge_v1().producer,
                        round:Box::new(d::PalwTirDissectRoundV1 {version:1, children:children.iter().map(|_| {
                            let mut partials = vec![Vec::new(); site.reductions.len()];
                            partials[0] = vec![0; values];
                            d::PalwTirRangeClaimV1 {partials}
                        }).collect()}),
                        signature:vec![0; crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN],
                    };
                    let bytes = borsh::to_vec(&row).unwrap().len() as u64;
                    signed_reference += bytes;
                    maximum_signed_reference = maximum_signed_reference.max(bytes);
                    todo.extend(children.into_iter().map(|(f, c)| (f, c, depth + 1)));
                }
            }
            let court = fx.rules.court.with_dissection_arity(arity).unwrap();
            for demand in [Demand::Release2000, Demand::H7] {
                let evidence = set.numerical_evidence_transport_upper(&fx.record, &fx.layout, &court, demand).unwrap();
                assert_eq!(evidence.signed_bytes_upper, evidence.roots_signed_bytes_upper
                    + evidence.bottoms_signed_bytes_upper + evidence.disclosures.signed_bytes_upper);
                assert_eq!(evidence.signed_controls, sites.len() as u64 + reference.disclosures + reference.terminals);
                let transport = evidence.disclosures;
                assert_eq!(transport.scope_id, set.scope_id().unwrap());
                if demand == Demand::H7 {
                    assert_eq!(transport.signed_bytes_upper, signed_reference);
                    assert!(transport.maximum_signed_bytes_upper >= maximum_signed_reference);
                }
                let actual = transport.numerical;
                assert_eq!(actual.source_id, source.source_id_v1());
                assert_eq!(actual.original_leaves, binding.step_leaf_count);
                assert_eq!(actual.eligible_roots, sites.len() as u64);
                assert_eq!(actual.arity, arity);
                assert_eq!(actual.demand, demand);
                assert_eq!(actual.trees.disclosures, reference.disclosures);
                assert_eq!(actual.trees.terminals, reference.terminals);
                assert_eq!(actual.trees.child_claims, reference.child_claims);
                assert_eq!(actual.trees.disclosure_path_bytes, reference.disclosure_path_bytes);
                assert_eq!(actual.trees.terminal_path_bytes, reference.terminal_path_bytes);
                assert_eq!(actual.trees.max_depth, reference.max_depth);
                assert_eq!(actual.trees.child_claims + actual.eligible_roots,
                    actual.trees.disclosures + actual.trees.terminals);
                assert!(actual.trees.round_body_bytes_upper > 0);
            }
            counts.push((arity, reference.disclosures, reference.terminals));
        }
        eprintln!("original numerical inventory: original_leaves={} eligible_roots={} arity_disclosures_terminals={counts:?}",
            binding.step_leaf_count, sites.len());
        let mut wrong_layout = fx.layout.clone();
        wrong_layout.h_tile *= 2;
        assert!(set.numerical_tree_upper(&fx.record, &wrong_layout, &fx.rules.court, Demand::H7).is_err());
        let mut wrong_record = fx.record.clone();
        wrong_record.program = std::sync::Arc::new(vec![]);
        assert!(set.numerical_tree_upper(&wrong_record, &fx.layout, &fx.rules.court, Demand::H7).is_err());
    }

    #[test]
    fn shared_material_two_tile_row_pins_match_native_without_full_rows() {
        use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1,PalwTirTraceEventDisclosureV1,tir_logits_event_disclosure_v1};
        use crate::palw_court_shared_tir_builder_store_v1::SharedTirBuilderStoreV1 as MaterialStore;
        use crate::palw_court_shared_tir_state_v1 as shared;
        use crate::palw_tir_public_source_v1::{PalwTirPublicRequestV1 as Q,PalwTirPublicResponseV1 as R};
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();
        let frame=&fx.frame;let claim=frame.plan_v1().original_claim;let p=frame.plan_v1();
        let (state,params,extras)=world(&fx,frame);let state=bind_clock_service(&state,frame,&params,&extras);
        let source=frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        assert!(source.vocab_v1()>4096,"additional 4097-word fixture exercises two different tiles");
        let profile=shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();let scope=set.scope_id().unwrap();
        let c=shared::SharedTirContextV1 {version:1,claim,allocation_id:p.allocation_id,source_id:p.source_id,rules_id:p.rules_id,
            profile_id:profile,scope_id:scope,queries:set.len(),original_accepted_daa:p.terms.accepted_daa,
            hard_daa:p.terms.accepted_daa+10_000,material_until_daa:p.terms.material_until_daa};
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(c)))).unwrap();
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();
        let ids=source.retain_complete_ids_v1(rows,&fx.execution.generated).unwrap();
        let mut units=Vec::new();
        for row in 0..source.decode_count_v1() {for tile in 0..source.vocab_v1().div_ceil(4096) {
            let PalwTirTraceEventDisclosureV1::Tiled {row_root,row_opening,tile_lanes,tile_opening,..}=
                tir_logits_event_disclosure_v1(&fx.execution.binding,&fx.execution.rows,&fx.execution.generated,row,u8::try_from(tile).unwrap()).unwrap() else {panic!("tiled fixture")};
            units.push(source.authenticate_v1(&Q::TraceTile {row,tile},&R::TraceTile {row_root,row_opening,lanes:tile_lanes,opening:tile_opening},Some(&ids)).unwrap());
        }}
        let store=MaterialStore::new(&b.state,claim,scope,&units,&[],Some(&ids)).unwrap();
        for row in 0..source.decode_count_v1() {for lane in [0,4095,4096] {
            let expected=fx.execution.row_pin(row,lane).unwrap();let actual=store.row_pin(row,lane).unwrap();
            assert_eq!(borsh::to_vec(&actual).unwrap(),borsh::to_vec(&expected).unwrap());
        }}
        assert_eq!(store.decode_pin(),fx.execution.decode_pin());
        assert!(store.row_pin(source.decode_count_v1(),0).is_none());assert!(store.row_pin(0,source.vocab_v1()).is_none());
        let no_ids=MaterialStore::new(&b.state,claim,scope,&units,&[],None).unwrap();assert!(no_ids.row_pin(0,0).is_none());
        let other_tile=if fx.execution.generated[0]/4096==0 {1}else{0};
        let partial:Vec<_>=units.iter().filter(|u|u.request_v1()!=Q::TraceTile {row:0,tile:other_tile}).cloned().collect();
        let missing=MaterialStore::new(&b.state,claim,scope,&partial,&[],Some(&ids)).unwrap();
        assert!(missing.row_pin(0,other_tile*4096).is_none());
    }

    #[test]
    fn shared_history_root_binds_public_original_policy_and_literal_producer_signature() {
        use crate::palw_court_shared_tir_root_v1::*;
        use crate::palw_court_shared_tir_state_v1 as shared;
        use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
        use crate::palw_tir_court_v1::{PalwTirCourtRulesV1, build_tir_root_claim_v1};
        let fx = crate::palw_tir_public_claim_v1::palw_tir_public_history_frame_fixture_v1();
        let frame = &fx.frame;
        let claim = frame.plan_v1().original_claim;
        let (mut state, params, extras) = world(&fx, frame);
        let params = params.with_tir_from_daa(Some(0));
        let producer = frame.custody_v1().pledge_v1().producer;
        let keys = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([91; 32]);
        // Existing original/registration/payment are verified-half fixtures.
        // Key replacement is fixture setup, never a native rotation transaction.
        state.bonds.get_mut(&producer).unwrap().pubkey = keys.verification_key.as_ref().to_vec();
        state = bind_clock_service(&state, frame, &params, &extras);
        let profile = shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let source = frame.source_v1(&fx.record, &fx.layout, fx.rules.maximum_leaves).unwrap();
        let set = crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame, &source, profile).unwrap();
        let p = frame.plan_v1();
        let c = shared::SharedTirContextV1 { version: 1, claim, allocation_id: p.allocation_id,
            source_id: p.source_id, rules_id: p.rules_id, profile_id: profile, scope_id: set.scope_id().unwrap(), queries: set.len(),
            original_accepted_daa: p.terms.accepted_daa, hard_daa: p.terms.accepted_daa + 10_000,
            material_until_daa: p.terms.material_until_daa };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_staged_row_v1(Key::CourtSharedContext(claim), Some(Row::CourtSharedContext(Box::new(c)))).unwrap();
        let state = b.state.clone();
        let rules = NativeCourtRulesV1 { network_domain: extras.tir_public_network_domain_v1.unwrap(), at: at(frame, 1),
            court: fx.rules.court, step_ladder: fx.rules.maximum_leaves, prompt_ids_merkle: false,
            held_regime: false, decode_close_convicts_only: false, fp_decode_from_daa: None,
            cell_closure_from_daa: None, memory_routing_from_daa: None, tir_active: true, gen_active: false,
            task_heads_active: false, improvement_active: false, constraint_active: false };
        {
            use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
            let c=shared::context(&state,claim).unwrap();
            let opening=fx.execution.param_opening(0).unwrap();
            let before=state.state_root();
            c.check_parameter_opening(&state,c.scope_id,&opening).unwrap();
            let wire=borsh::to_vec(&opening).unwrap();
            assert_eq!(*c.decode_parameter_opening(&state,c.scope_id,0,&wire).unwrap(),opening);
            assert!(c.decode_parameter_opening(&state,Hash64::default(),0,&wire).is_err());
            assert!(c.decode_parameter_opening(&state,c.scope_id,1,&wire).is_err());
            for cut in [0,wire.len()/2,wire.len()-1] {assert!(c.decode_parameter_opening(&state,c.scope_id,0,&wire[..cut]).is_err());}
            let mut hostile=wire.clone();hostile[..4].copy_from_slice(&u32::MAX.to_le_bytes());
            assert!(c.decode_parameter_opening(&state,c.scope_id,0,&hostile).is_err());
            assert!(c.check_parameter_opening(&state,Hash64::default(),&opening).is_err());
            let mut bad=opening.clone();bad.operand.bytes[0]^=1;
            assert!(c.check_parameter_opening(&state,c.scope_id,&bad).is_err());
            let mut other=c.clone();other.scope_id=Hash64::default();
            assert!(other.check_parameter_opening(&state,other.scope_id,&opening).is_err());
            let mut missing=state.clone();missing.tir_classes.remove(&frame.descriptor_v1().class_id);
            assert!(c.check_parameter_opening(&missing,c.scope_id,&opening).is_err());
            let mut corrupt=state.clone();corrupt.tir_classes.get_mut(&frame.descriptor_v1().class_id).unwrap().program=std::sync::Arc::new(vec![]);
            assert!(c.check_parameter_opening(&corrupt,c.scope_id,&opening).is_err());
            assert_eq!(state.state_root(),before);
        }
        let binding = &fx.execution.binding;
        let space = crate::palw_tir_step_v1::PalwTirStepSpaceV1::new(&binding.class).unwrap();
        let intervals = misaka_palw_tir::interval::analyze_ranges(&space.program).unwrap();
        let (leaf, _) = (0..binding.step_leaf_count).filter_map(|i| {
            let l = space.leaf_at(&binding.job_context, i).unwrap();
            crate::palw_tir_dissect_v1::palw_tir_dissect_site_v1(&space, &intervals, &l).map(|site| (i, site))
        }).max_by_key(|(i, site)| (site.history_positions, site.reductions.len(), std::cmp::Reverse(*i))).unwrap();
        let court = PalwTirCourtRulesV1 { max_step_leaf_count: rules.step_ladder, prompt_form: fx.rules.prompt_form,
            limits: crate::palw_court_v2::palw_tir_court_limits_v1(&rules.court) };
        let mut root = build_tir_root_claim_v1(binding, leaf, &fx.execution, &court).unwrap();
        crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut root.finalize.binding);
        let mut control = SignedSharedTirRootV1 { version: 1, claim, scope_id: set.scope_id().unwrap(),
            policy_id: shared_tir_root_policy_id_v1(&rules).unwrap(), leaf, arity: rules.court.dissection_arity(),
            signer: producer, root: Box::new(root), signature: vec![] };
        let sign = |c: &mut SignedSharedTirRootV1| {
            c.signature = libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
                c.signing_message(rules.network_domain).unwrap().as_byte_slice(), SHARED_TIR_ROOT_CONTEXT_V1, [92; 32])
                .unwrap().as_ref().to_vec();
        };
        sign(&mut control);
        let legacy = crate::palw_state_v2::PalwConsensusObjectV2::CourtTirRootClaimed {
            session_id:claim, root:control.root.clone(), arity:control.arity, signature:control.signature.clone()};
        assert_eq!(shared_root_carried_bytes_v1(&control.root).unwrap(), borsh::to_vec(&legacy).unwrap().len() as u64
            + crate::palw_tir_close_size_v1::MOVE_CARRIER_EXTRA_BYTES);
        let transport = set.numerical_evidence_transport_upper(&fx.record, &fx.layout, &rules.court,
            crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7).unwrap();
        let root_fixed = borsh::to_vec(&control).unwrap().len() as u64 - borsh::object_length(control.root.as_ref()).unwrap() as u64;
        assert_eq!(transport.maximum_root_signed_bytes_upper, shared_root_body_ceiling_v1().unwrap() + root_fixed);
        assert!(transport.maximum_root_signed_bytes_upper > crate::palw_tir_admission_v1::PALW_TIR_DISSECT_CARRIER_BYTES_V1,
            "new framing cannot inherit a single-carrier guarantee from the old payload ceiling");
        assert!(borsh::to_vec(&control).unwrap().len() as u64 <= transport.maximum_root_signed_bytes_upper);
        // Isolate the byte predicate only: padding here is not a valid root
        // or a new network-ID allowance and is never signed or ingested.
        let mut boundary = control.root.as_ref().clone();
        let remaining = shared_root_body_ceiling_v1().unwrap() as usize - borsh::object_length(&boundary).unwrap();
        let old = boundary.finalize.binding.job_context.network_id.len();
        boundary.finalize.binding.job_context.network_id.resize(old + remaining, 0);
        check_shared_root_cost_v1(&boundary).unwrap();
        boundary.finalize.binding.job_context.network_id.push(0);
        assert!(check_shared_root_cost_v1(&boundary).is_err());
        let mut oversized = control.clone(); oversized.root = Box::new(boundary);
        assert!(check_shared_tir_root_v1(&state, &params, &rules, &oversized).err().unwrap().contains("root carrier allowance"));
        let before = state.state_root();
        assert_eq!(state.court_sessions_len(), 0);
        let checked = check_shared_tir_root_v1(&state, &params, &rules, &control).unwrap();
        assert_eq!(checked.site().history_positions, 24);
        assert_eq!(checked.control(), &control);
        // Relays/requesters contribute nothing to the identity. Rechecking by a
        // second caller does not allocate a session or choose a numerical child.
        check_shared_tir_root_v1(&state, &params, &rules, &control).unwrap();
        let mut later = rules; later.at = at(frame, 2);
        assert_eq!(shared_tir_root_policy_id_v1(&later).unwrap(), control.policy_id);
        check_shared_tir_root_v1(&state, &params, &later, &control).unwrap();
        for mutate in 0..11 {
            let mut bad = control.clone();
            match mutate {
                0 => bad.claim = h(0xFA01), 1 => bad.scope_id = h(0xFA02), 2 => bad.policy_id = h(0xFA03),
                3 => bad.leaf = binding.step_leaf_count, 4 => bad.arity = 3, 5 => bad.signer = outsider(),
                6 => bad.root.finalize.binding.committed_execution_root = h(0xFA04),
                7 => bad.root.elements.clear(), 8 => bad.root.finalize.binding.class.program = binding.class.program.clone(),
                9 => bad.version = 2, 10 => bad.signature[0] ^= 1, _ => unreachable!(),
            }
            // Re-sign semantic/source substitutions, so refusal is not merely
            // an old signature failing to match the edited body.
            if mutate < 9 { sign(&mut bad); }
            assert!(check_shared_tir_root_v1(&state, &params, &rules, &bad).is_err(), "mutation {mutate}");
        }
        let mut old = control.clone();
        old.signature = libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
            &crate::palw_tir_dissect_v1::palw_tir_root_claim_message_v1(&h(0x515), &old.root),
            crate::palw_court_v2::PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT, [93;32]).unwrap().as_ref().to_vec();
        assert!(check_shared_tir_root_v1(&state, &params, &rules, &old).is_err());
        let mut other_network = rules; other_network.network_domain = h(0xFA05);
        let mut wrong_network = control.clone();
        wrong_network.policy_id = shared_tir_root_policy_id_v1(&other_network).unwrap();
        assert!(check_shared_tir_root_v1(&state, &params, &other_network, &wrong_network).is_err());
        let mut other_rules = rules; other_rules.held_regime = !rules.held_regime;
        assert!(check_shared_tir_root_v1(&state, &params, &other_rules, &control).is_err());
        let mut hard = rules; hard.at = at(frame, 10_000);
        assert!(check_shared_tir_root_v1(&state, &params, &hard, &control).is_err());
        let mut missing = state.clone(); missing.staged_courts.remove(&frame.key_v1());
        assert!(check_shared_tir_root_v1(&missing, &params, &rules, &control).is_err(), "no native-claim fallback");
        let mut missing = state.clone(); missing.staged_courts.remove(&Key::CourtSharedContext(claim));
        assert!(check_shared_tir_root_v1(&missing, &params, &rules, &control).is_err());
        let mut substituted = state.clone();
        let Some(Row::CourtSharedContext(row)) = substituted.staged_courts.get_mut(&Key::CourtSharedContext(claim)) else { unreachable!() };
        row.profile_id = h(0xFA06);
        assert!(check_shared_tir_root_v1(&substituted, &params, &rules, &control).is_err());
        let mut inactive = rules; inactive.tir_active = false;
        assert!(check_shared_tir_root_v1(&state, &params, &inactive, &control).is_err());
        let mut exited = state.clone(); exited.bonds.get_mut(&producer).unwrap().collateral = 0;
        assert!(check_shared_tir_root_v1(&exited, &params, &rules, &control).is_err());
        assert_eq!(state.state_root(), before);
        assert_eq!(state.court_sessions_len(), 0);

        use super::super::palw_court_shared_tir_fold_v1 as fold;
        use crate::palw_native_resource_v1::{NativeHeapV1, preflight_native_value_v1, preflight_native_carriage_v1};
        let mut native = rules; native.at = at(frame, 5);
        let mut root_extras = extras.clone(); root_extras.native_court_rules_v1 = Some(native);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &root_extras);
        assert!(fold::root(&mut b, &native.at, &oversized).is_err());
        assert_eq!(b.state.state_root(), before); assert!(b.entries.is_empty());
        use crate::palw_court_numerical_transport_v1::NumericalControlRefV1 as N;
        let (manifest,parts)=numerical_parts(N::Root(&control),set.scope_id().unwrap(),&native);
        for kind in 0..9 {
            let mut m=manifest.clone();let mut p=parts.clone();
            match kind {0=>m.network=h(0xCF01),1=>m.scope=h(0xCF02),2=>m.leaf+=1,3=>m.total_bytes+=1,
                4=>p[0].bytes[0]^=1,5=>p[0].offset+=1,6=>p[0].stream_id=h(0xCF03),7=>{p.pop();},_=>m.body_hash=h(0xCF04)}
            assert!(fold::numerical_transport(&mut b,&native.at,&m,&p).is_err(),"transport mutation {kind}");
            assert_eq!(b.state.state_root(),before);assert!(b.entries.is_empty());
        }
        let mut bad_signature=control.clone();bad_signature.signature[0]^=1;
        let (m,p)=numerical_parts(N::Root(&bad_signature),set.scope_id().unwrap(),&native);
        assert!(fold::numerical_transport(&mut b,&native.at,&m,&p).is_err());
        assert_eq!(b.state.state_root(),before);assert!(b.entries.is_empty());
        let mut direct=TransitionBuilder::new(&state,&params,false,false,false,false,&root_extras);
        assert!(fold::root(&mut direct,&native.at,&control).unwrap());
        assert_eq!(fold::numerical_transport(&mut b,&native.at,&manifest,&parts).unwrap(),fold::NumericalTransportEffectV1::Root(true));
        assert_eq!(b.state.state_root(),direct.state.state_root());assert_eq!(b.entries.len(),direct.entries.len());
        eprintln!("numerical root transport: bytes={} parts={}",manifest.total_bytes,parts.len());
        let key = Key::CourtSharedRoot(claim, leaf);
        let Some(Row::CourtSharedRoot(retained)) = b.state.staged_court_row_v1(&key).cloned() else { panic!() };
        assert_eq!(retained.rules, native);
        let frontier = retained.numerical_frontier(&b.state).unwrap();
        assert_eq!(frontier.terminal_tiles(), 12);
        assert_eq!(frontier.maximum_disclosures(), 11);
        let canonical = b.state.state_root(); let entries = b.entries.len();
        let mut retry = control.clone();
        retry.signature = libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
            retry.signing_message(native.network_domain).unwrap().as_byte_slice(), SHARED_TIR_ROOT_CONTEXT_V1, [94;32])
            .unwrap().as_ref().to_vec();
        assert_ne!(retry.signature, control.signature);
        assert!(!fold::root(&mut b, &native.at, &retry).unwrap());
        assert_eq!(b.state.state_root(), canonical); assert_eq!(b.entries.len(), entries);
        assert_eq!(b.state.staged_court_row_v1(&key), Some(&Row::CourtSharedRoot(retained.clone())));
        assert!(b.write_staged_row_v1(key, None).is_err());
        assert!(b.write_staged_row_v1(Key::CourtSharedRoot(claim, leaf + 1), Some(Row::CourtSharedRoot(retained.clone()))).is_err());
        let mut changed = retained.clone(); changed.material_until_daa += 1;
        assert!(b.write_staged_row_v1(key, Some(Row::CourtSharedRoot(changed))).is_err());
        assert_eq!(b.state.state_root(), canonical); assert_eq!(b.entries.len(), entries);
        // A genuinely valid signature under a different actual policy still
        // cannot evade the single (claim,leaf) slot by choosing another policy ID.
        let mut alternative = native; alternative.held_regime = !native.held_regime;
        let mut alt = control.clone(); alt.policy_id = shared_tir_root_policy_id_v1(&alternative).unwrap(); sign(&mut alt);
        check_shared_tir_root_v1(&b.state, &params, &alternative, &alt).unwrap();
        let mut alt_extras = root_extras.clone(); alt_extras.native_court_rules_v1 = Some(alternative);
        let mut other = TransitionBuilder::new(&b.state, &params, false, false, false, false, &alt_extras);
        assert!(fold::root(&mut other, &alternative.at, &alt).is_err());
        assert_eq!(other.state.state_root(), canonical); assert!(other.entries.is_empty());
        let row = Row::CourtSharedRoot(retained.clone());
        let wire = borsh::to_vec(&row).unwrap();
        assert_eq!(borsh::to_vec(&key).unwrap()[0], 22); assert_eq!(wire[0], 28);
        let shape = preflight_native_value_v1::<Row>(wire.as_slice(), wire.len() as u64).unwrap();
        assert!(shape.logical_requests_v1() >= row.native_heap_v1("root row").bytes().unwrap() + std::mem::size_of::<Row>() as u64);
        assert_eq!(borsh::from_slice::<Row>(&wire).unwrap(), row);
        for cut in [0, 1, wire.len()/2, wire.len()-1] {
            assert!(preflight_native_value_v1::<Row>(&wire[..cut], cut as u64).is_err());
        }
        let mut trailing = wire.clone(); trailing.push(0);
        assert!(preflight_native_value_v1::<Row>(trailing.as_slice(), trailing.len() as u64).is_err());
        let signature_at = 1 + 2 + borsh::to_vec(&control).unwrap().len() - SIG - 4;
        let mut oversized = wire.clone(); oversized[signature_at..signature_at+4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(preflight_native_value_v1::<Row>(oversized.as_slice(), oversized.len() as u64).is_err());
        assert!(borsh::from_slice::<Row>(&oversized).is_err());
        let mut undone = b.state.clone();
        for entry in b.entries.iter().rev() { apply_delta_entry(&mut undone, entry, true).unwrap(); }
        assert_eq!(undone.state_root(), state.state_root());
        for entry in &b.entries { apply_delta_entry(&mut undone, entry, false).unwrap(); }
        assert_eq!(undone.state_root(), canonical);
        let mut accepted = b.state.clone(); accepted.set_last_point_for_tests(Some(native.at));
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&accepted)).unwrap();
        preflight_native_carriage_v1(bytes.as_slice(), bytes.len() as u64).unwrap();
        let fresh = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params, Some(accepted.state_root())).unwrap();
        assert_eq!(fresh.state_root(), accepted.state_root());
        let Some(Row::CourtSharedRoot(loaded)) = fresh.staged_court_row_v1(&key) else { panic!() };
        assert_eq!(loaded.numerical_frontier(&fresh).unwrap(), frontier);
        let mut rotated = fresh.clone(); rotated.bonds.get_mut(&producer).unwrap().pubkey = vec![0; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN];
        rotated.bonds.get_mut(&producer).unwrap().collateral = 0;
        loaded.check_loaded_context(&rotated).unwrap();
        assert!(check_shared_tir_root_v1(&rotated, &params, &native, &control).is_err());
        let mut future = (*loaded).clone(); future.rules.at = at(frame, 6);
        assert!(future.check_loaded_context(&fresh).is_err());
        let mut swept = TransitionBuilder::new(&fresh, &params, false, false, false, false, &root_extras);
        let past = at(frame, frame.plan_v1().terms.material_until_daa - frame.plan_v1().terms.accepted_daa + 1);
        super::super::palw_staged_court_fold_v1::sweep(&mut swept, &past).unwrap();
        assert!(swept.state.staged_court_row_v1(&key).is_some());
        assert!(swept.write_staged_row_v1(key, None).is_err());
        assert_eq!(fresh.court_sessions_len(), 0);

        use crate::palw_court_shared_tir_round_v1 as rounds;
        use crate::palw_court_shared_tir_bottom_v1 as bottoms;
        // Canonical potential requirements, derived before the path loop; not the
        // oracle-observed one-execution read subset below. Fixture bytes are
        // actual artifact openings decoded through retained native context.
        let parameter_source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &fx.record,&fx.layout,frame.header_v1(),frame.original_v1().execution_root,
            frame.original_v1().trace_root,fx.rules.maximum_leaves).unwrap();
        let parameter_cover=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(
            &parameter_source,&fx.record,&fx.layout,
            crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
                max_original_leaves:1_000_000,max_total_edges:10_000_000,
                max_total_coordinates:1_000_000,max_root_memberships:1_000_000}).unwrap();
        let (material_join_graph,material_join_numerical,material_join_profile,material_join_visits)=include!("palw_numerical_join_terminal_setup.rs");
        let material_join=crate::palw_court_shared_tir_builder_store_v1::numerical_material_join::NumericalMaterialJoinV1::derive_native_v1(frame,&parameter_source,&material_join_graph,&material_join_numerical,&parameter_cover,material_join_profile,&native.court,crate::palw_tir_fence2_v1::PalwTirDemandRulesV1::H7,material_join_visits).unwrap();
        // A genuine different original's opaque cover; no cover DTO/roots are
        // forged or edited. Even an empty numerical set cannot be substituted.
        let foreign_parameter_fixture=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_two_tile_v1();
        let foreign_parameter_original=foreign_parameter_fixture.frame.original_v1();
        let foreign_parameter_source=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::new_native_v1(
            &foreign_parameter_fixture.record,&foreign_parameter_fixture.layout,foreign_parameter_fixture.frame.header_v1(),
            foreign_parameter_original.execution_root,foreign_parameter_original.trace_root,
            foreign_parameter_fixture.rules.maximum_leaves).unwrap();
        let foreign_parameter_cover=crate::palw_tir_public_source_v1::PalwTirPublicSourceV1::root_input_cover_native_v1(
            &foreign_parameter_source,&foreign_parameter_fixture.record,&foreign_parameter_fixture.layout,
            crate::palw_tir_public_source_v1::parameter_cover_v1::OriginalParameterCoverBudgetV1 {
                max_original_leaves:1_000_000,max_total_edges:10_000_000,
                max_total_coordinates:1_000_000,max_root_memberships:1_000_000}).unwrap();
        assert_ne!(foreign_parameter_cover.parameters_v1().source_id,parameter_cover.parameters_v1().source_id);
        let covered_root=parameter_cover.parameters_v1().roots.iter().find(|r|r.leaf==leaf).unwrap();
        assert!(!covered_root.parameters.is_empty(),"actual root must require parameters");
        let mut owner_requests=crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        owner_requests.vec_v1::<crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_artifact::PalwArtifactOpeningV1>>(
            covered_root.parameters.len() as u64).unwrap();
        let _owner_array=crate::palw_native_resource_v1::reserve_native_requests_v1(owner_requests.bytes_v1()).unwrap();
        let mut covered_parameters=Vec::new();
        covered_parameters.try_reserve_exact(covered_root.parameters.len()).unwrap();
        for required in &covered_root.parameters {
            use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
            // Original fixture opening allocation is test provisioning; it is not
            // a production acquisition/retention result. Serializer scratch is
            // separately pre-reserved before building the carrier byte Vec.
            let opening=fx.execution.param_opening(*required).unwrap();
            let encoded=borsh::object_length(&opening).unwrap() as u64;
            let raw_requests=encoded.max(1024).checked_mul(2).unwrap();
            let _raw_owner=crate::palw_native_resource_v1::reserve_native_requests_v1(raw_requests).unwrap();
            let wire=borsh::to_vec(&opening).unwrap();
            let context=shared::context(&fresh,claim).unwrap();
            covered_parameters.push(context.decode_parameter_opening(&fresh,context.scope_id,*required,&wire).unwrap());
        }
        // Authenticated source material only; the builder view never sees fx.execution.
        let material_units: Vec<_> = (0..binding.step_leaf_count).map(|index| {
            use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
            let q=crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf {index};
            let r=crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
                preimage:fx.execution.step_leaf(index).unwrap(),opening:fx.execution.step_opening(index).unwrap(),
            };
            source.authenticate_v1(&q,&r,None).unwrap()
        }).collect();
        {
            use crate::palw_tir_court_v1::{PalwTirEvidenceStoreV1,build_tir_cone_refutation_v1};
            use crate::palw_court_shared_tir_builder_store_v1::{SharedTirBuilderStoreV1 as MaterialStore,PromptMaterialV1};
            let (early_leaf,early)=(0..binding.step_leaf_count).find_map(|i|
                build_tir_cone_refutation_v1(binding,i,&fx.execution,&court).ok()
                    .filter(|p|!p.prompt_token_ids.is_empty()).map(|p|(i,p))).expect("fixture has a prompt-reading cone");
            let params:Vec<_>=early.params.single().map(|p|p.opened.as_slice()).unwrap_or(&[]).iter()
                .map(|(i,_)|fx.execution.param_opening(*i).unwrap()).collect();
            let scope=set.scope_id().unwrap();
            let store=MaterialStore::new_with_prompt(&fresh,claim,scope,&material_units,&params,None,
                PromptMaterialV1::Flat(&fx.execution.prompt)).unwrap();
            let replay=build_tir_cone_refutation_v1(binding,early_leaf,&store,&court).unwrap();
            assert_eq!(borsh::to_vec(&replay).unwrap(),borsh::to_vec(&early).unwrap());
            let missing=MaterialStore::new(&fresh,claim,scope,&material_units,&params,None).unwrap();
            assert!(build_tir_cone_refutation_v1(binding,early_leaf,&missing,&court).is_err());
            let mut bad=fx.execution.prompt.clone();bad[0]^=1;
            assert!(MaterialStore::new_with_prompt(&fresh,claim,scope,&material_units,&params,None,PromptMaterialV1::Flat(&bad)).is_err());
            assert!(MaterialStore::new_with_prompt(&fresh,claim,scope,&material_units,&params,None,PromptMaterialV1::Merkle(&[])).is_err());
        }
        for false_partition in [false,true] {
        let mut round_state=fresh.clone();
        let filer_keys=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([101;32]);
        // Fixture identity setup, not a native registration/rotation or funding test.
        round_state.bonds.get_mut(&outsider()).unwrap().pubkey=filer_keys.verification_key.as_ref().to_vec();
        let (mut event_offset,mut local_faults)=(10,0);

        let mut paths=std::collections::VecDeque::from([Vec::<u8>::new()]);
        let (mut disclosures,mut terminals)=(0,0);
        let mut retained_controls=Vec::new();
        while let Some(path)=paths.pop_front(){
            let prepared=rounds::prepare_shared_tir_round_v1(&round_state,claim,leaf,&path).unwrap();
            if prepared.phase.terminal_range().is_some(){
                let mut evidence=crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&fx.execution,&court).unwrap();
                // Observe actual dependencies without changing the fixture, builder or native rules.
                let observed=crate::palw_tir_court_v1::ObservedEvidenceStoreV1 {
                    inner:&fx.execution,calls:Default::default(),deny:None,
                };
                let rebuilt=crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&observed,&court).unwrap();
                assert_eq!(borsh::to_vec(&rebuilt).unwrap(),borsh::to_vec(&evidence).unwrap());
                let calls=observed.calls.into_inner();
                {
                    use crate::palw_tir_court_v1::PalwTirEvidenceStoreV1;
                    use crate::palw_court_shared_tir_builder_store_v1::SharedTirBuilderStoreV1 as MaterialStore;
                    // Oracle-derived read subset is fixture provisioning, not a proof of
                    // original-wide prefetch/reservation. All inputs are authenticated anew.
                    let openings:Vec<_>=evidence.params.single().unwrap().opened.iter()
                        .map(|(i,_)|fx.execution.param_opening(*i).unwrap()).collect();
                    let scope=set.scope_id().unwrap();
                    let store=MaterialStore::new(&round_state,claim,scope,&material_units,&openings,None).unwrap();
                    let replay=crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&store,&court).unwrap();
                    assert_eq!(borsh::to_vec(&replay).unwrap(),borsh::to_vec(&evidence).unwrap());
                    use crate::palw_court_shared_tir_builder_store_v1::{SignedSharedTerminalIntentV1 as Intent,
                        SHARED_TERMINAL_INTENT_CONTEXT_V1 as INTENT_CONTEXT};
                    let mut intent_rules=native;intent_rules.at=at(frame,event_offset);
                    let mut intent=Intent {version:1,claim,scope_id:scope,policy_id:control.policy_id,leaf,
                        root_id:prepared.root_id,path:path.clone(),parent_id:prepared.parent_id,filer:outsider(),signature:vec![]};
                    let sign_intent=|i:&mut Intent| {
                        i.signature=libcrux_ml_dsa::ml_dsa_87::sign(&filer_keys.signing_key,
                            i.signing_message(native.network_domain).unwrap().as_byte_slice(),INTENT_CONTEXT,[109;32])
                            .unwrap().as_ref().to_vec();
                    };
                    sign_intent(&mut intent);
                    // Existing node construction policy, used only as a fixture work cap.
                    // This is not a new consensus limit, CPU calibration or funded profile.
                    let limit=crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1 {evaluation:court.limits,
                        max_target_requests:65_536,max_source_reads:court.limits.max_elements
                            .checked_add(court.limits.max_terms).unwrap().checked_mul(8).unwrap().checked_add(65_536).unwrap()};
                    let before_intent=round_state.state_root();
                    let (built,meter)=store.build_terminal(&round_state,&params,&intent_rules,&intent,limit).unwrap();
                    let covered_store=MaterialStore::new_with_retained_parameters(&round_state,claim,scope,
                        &material_units,&covered_parameters,None,
                        crate::palw_court_shared_tir_builder_store_v1::PromptMaterialV1::Missing).unwrap();
                    let (covered,covered_meter)=covered_store.build_terminal_with_parameter_cover(
                        &round_state,&params,&intent_rules,&intent,limit,&parameter_cover).unwrap();
                    assert_eq!(borsh::to_vec(&covered).unwrap(),borsh::to_vec(&built).unwrap());
                    assert_eq!(covered_meter,meter);
                    include!("palw_numerical_join_terminal_selected.rs");
                    // Same valid signed intent and retained terminal phase as
                    // positive path: rejection must occur at source affinity,
                    // not an invalid signature/root/phase shortcut.
                    if terminals==0 {
                        let foreign=covered_store.build_terminal_with_parameter_cover(
                            &round_state,&params,&intent_rules,&intent,limit,&foreign_parameter_cover).unwrap_err();
                        assert_eq!(foreign,"builder parameter cover from another original source");
                    }

                    let missing_store=MaterialStore::new_with_retained_parameters(&round_state,claim,scope,
                        &material_units,&covered_parameters[1..],None,
                        crate::palw_court_shared_tir_builder_store_v1::PromptMaterialV1::Missing).unwrap();
                    // Correctly authenticated remaining inputs are not enough:
                    // missing canonical requirement is caught before finalize.
                    let refused=missing_store.build_terminal_with_parameter_cover(
                        &round_state,&params,&intent_rules,&intent,limit,&parameter_cover).unwrap_err();
                    assert!(refused.contains("lacks authenticated parameter leaf"),"{refused}");

                    let mut expected=replay.clone();
                    crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut expected.binding);
                    assert_eq!(borsh::to_vec(&built).unwrap(),borsh::to_vec(&expected).unwrap());
                    assert!(meter.target_requests>0 && meter.source_reads>0);
                    let exact=crate::palw_tir_court_v1::PalwTirBuilderWorkLimitsV1 {evaluation:misaka_palw_tir::demand::DemandLimits {max_elements:meter.work.elements,max_terms:meter.work.terms},
                        max_target_requests:meter.target_requests,max_source_reads:meter.source_reads};
                    let (again,again_meter)=store.build_terminal(&round_state,&params,&intent_rules,&intent,exact).unwrap();
                    assert_eq!(borsh::to_vec(&again).unwrap(),borsh::to_vec(&built).unwrap());assert_eq!(again_meter,meter);
                    let mut short=exact;short.max_source_reads-=1;
                    assert!(store.build_terminal(&round_state,&params,&intent_rules,&intent,short).is_err());
                    let mut second=intent.clone();second.filer=producer;
                    second.signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
                        second.signing_message(native.network_domain).unwrap().as_byte_slice(),INTENT_CONTEXT,[110;32])
                        .unwrap().as_ref().to_vec();
                    assert_eq!(borsh::to_vec(&store.build_terminal(&round_state,&params,&intent_rules,&second,exact).unwrap().0).unwrap(),
                        borsh::to_vec(&built).unwrap(),"a second authenticated filer has no exclusive body owner to wait for");
                    if terminals==0 {
                        for kind in 0..7 {
                            let mut bad=intent.clone();
                            match kind {0=>bad.scope_id=h(0xDD01),1=>bad.policy_id=h(0xDD02),2=>bad.root_id=h(0xDD03),
                                3=>bad.parent_id=h(0xDD04),4=>bad.leaf+=1,5=>{bad.path.pop();},_=>{bad.signature[0]^=1;}};
                            if kind!=6 {sign_intent(&mut bad);}
                            assert!(store.build_terminal(&round_state,&params,&intent_rules,&bad,limit).is_err(),"intent mutation {kind}");
                        }
                        let mut future=intent_rules;future.at=at(frame,10_000);
                        assert!(store.build_terminal(&round_state,&params,&future,&intent,limit).is_err());
                        let mut before=intent_rules;before.at=at(frame,0);
                        assert!(store.build_terminal(&round_state,&params,&before,&intent,limit).is_err());
                        let mut network=intent_rules;network.network_domain=h(0xDD05);
                        assert!(store.build_terminal(&round_state,&params,&network,&intent,limit).is_err());
                        let mut wrong_context=intent.clone();
                        wrong_context.signature=libcrux_ml_dsa::ml_dsa_87::sign(&filer_keys.signing_key,
                            wrong_context.signing_message(native.network_domain).unwrap().as_byte_slice(),
                            bottoms::SHARED_TIR_BOTTOM_CONTEXT_V1,[111;32]).unwrap().as_ref().to_vec();
                        assert!(store.build_terminal(&round_state,&params,&intent_rules,&wrong_context,limit).is_err());
                    }
                    assert_eq!(round_state.state_root(),before_intent,"intent construction cannot mutate or reserve a slot");
                    let no_params=MaterialStore::new(&round_state,claim,scope,&material_units,&[],None).unwrap();
                    assert!(crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&no_params,&court).is_err());
                    assert!(no_params.build_terminal(&round_state,&params,&intent_rules,&intent,limit).is_err());
                    let no_steps=MaterialStore::new(&round_state,claim,scope,&[],&openings,None).unwrap();
                    assert!(crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&no_steps,&court).is_err());
                    assert!(no_steps.build_terminal(&round_state,&params,&intent_rules,&intent,limit).is_err());
                    assert!(MaterialStore::new(&round_state,claim,Hash64::default(),&material_units,&openings,None).is_err());
                    let duplicate=vec![openings[0].clone(),openings[0].clone()];
                    assert!(MaterialStore::new(&round_state,claim,scope,&material_units,&duplicate,None).is_err());
                    let mut bad=openings.clone();bad[0].operand.bytes[0]^=1;
                    assert!(MaterialStore::new(&round_state,claim,scope,&material_units,&bad,None).is_err());
                    assert!(store.prompt_token_ids().is_none());assert!(store.decode_pin().is_none());
                }
                let before_root=round_state.state_root();
                for family in ["param_opening","param_multiproof","param_carriage","prompt_token_ids","prompt_ids_opening","decode_pin"] {
                    if calls.iter().any(|c|c.starts_with(&format!("{family}("))) {
                        let missing=crate::palw_tir_court_v1::ObservedEvidenceStoreV1 {
                            inner:&fx.execution,calls:Default::default(),deny:Some(family),
                        };
                        assert!(crate::palw_tir_court_v1::build_tir_dissect_bottom_v1(binding,&prepared.phase,&missing,&court).is_err(),
                            "missing {family} unexpectedly built terminal {path:?}");
                        assert_eq!(round_state.state_root(),before_root);
                    }
                }
                eprintln!("terminal dependencies false_partition={false_partition} path={path:?} calls={calls:?}");
                crate::palw_tir_admission_v1::palw_tir_binding_strip_program_v1(&mut evidence.binding);
                let native_proof=crate::palw_court_v2::PalwCourtVerdictProofV2::TirDissection {bottom:Box::new(evidence.clone())};
                let bytes=bottoms::shared_bottom_close_bytes_v1(&evidence).unwrap();
                assert_eq!(bytes,borsh::to_vec(&native_proof).unwrap().len() as u64);
                let small=crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(native.court.max_step_leaf_count(),native.court.turn_deadline_daa(),
                    native.court.terminal_rounds(),bytes-1,native.court.max_terminal_macs(),native.court.max_operand_count()).unwrap();
                assert!(bottoms::check_shared_bottom_cost_v1(&evidence,&small).is_err());
                let mut c=bottoms::SignedSharedTirBottomV1 {version:1,claim,leaf,root_id:prepared.root_id,path:path.clone(),parent_id:prepared.parent_id,
                    filer:outsider(),bottom:Box::new(evidence),signature:vec![]};
                let sign_bottom=|c:&mut bottoms::SignedSharedTirBottomV1|{
                    c.signature=libcrux_ml_dsa::ml_dsa_87::sign(&filer_keys.signing_key,
                        c.signing_message(native.network_domain).unwrap().as_byte_slice(),bottoms::SHARED_TIR_BOTTOM_CONTEXT_V1,[102;32]).unwrap().as_ref().to_vec();
                };
                sign_bottom(&mut c);
                let body = borsh::object_length(c.bottom.as_ref()).unwrap() as u64;
                let actual_overhead = borsh::to_vec(&c).unwrap().len() as u64 - body - path.len() as u64;
                let predicted_one = transport.maximum_bottom_signed_bytes_upper
                    - u64::from(transport.disclosures.numerical.trees.max_depth);
                assert_eq!(predicted_one, native.court.max_close_bytes() - 1 + actual_overhead);
                assert!(borsh::to_vec(&c).unwrap().len() as u64 <= transport.maximum_bottom_signed_bytes_upper);
                let mut at_rules=native;at_rules.at=at(frame,event_offset);event_offset+=1;
                let mut at_extras=root_extras.clone();at_extras.native_court_rules_v1=Some(at_rules);
                let mut b=TransitionBuilder::new(&round_state,&params,false,false,false,false,&at_extras);
                let before=b.state.state_root();
                let original_claim=b.state.claim(&claim).unwrap().clone();
                let original_producer=b.state.bond(&producer).unwrap().clone();let original_filer=b.state.bond(&outsider()).unwrap().clone();
                for kind in 0..4 {
                    let mut bad=c.clone();
                    match kind {0=>bad.parent_id=h(0xBC01),1=>bad.bottom.output_opening.leaf_index=leaf+1,
                        2=>bad.bottom.binding.committed_execution_root=h(0xBC02),3=>bad.bottom.output_opening.siblings.clear(),_=>unreachable!()}
                    sign_bottom(&mut bad);
                    assert!(fold::bottom(&mut b,&at_rules.at,&bad).is_err(),"bad bottom {kind}");
                    assert_eq!(b.state.state_root(),before);assert!(b.entries.is_empty());
                }
                let (m,p)=numerical_parts(N::Bottom(&c),set.scope_id().unwrap(),&at_rules);
                let fold::NumericalTransportEffectV1::Bottom(opened,outcome)=fold::numerical_transport(&mut b,&at_rules.at,&m,&p).unwrap()
                    else {panic!("bottom transport kind")};assert!(opened);
                eprintln!("numerical bottom transport: bytes={} parts={}",m.total_bytes,p.len());
                match &outcome {bottoms::SharedTirBottomOutcomeV1::NoMismatchFound=>{},bottoms::SharedTirBottomOutcomeV1::NativeFault(_)=>{assert!(false_partition);local_faults+=1;}}
                assert_eq!(b.state.claim(&claim),Some(&original_claim));
                assert_eq!(b.state.bond(&producer),Some(&original_producer));assert_eq!(b.state.bond(&outsider()),Some(&original_filer));
                let key=Key::CourtSharedBottom(c.id().unwrap());let row=b.state.staged_court_row_v1(&key).unwrap().clone();
                let wire=borsh::to_vec(&row).unwrap();assert_eq!(borsh::to_vec(&key).unwrap()[0],24);assert_eq!(wire[0],30);
                let shape=preflight_native_value_v1::<Row>(wire.as_slice(),wire.len() as u64).unwrap();
                assert!(shape.logical_requests_v1()>=row.native_heap_v1("bottom row").bytes().unwrap()+std::mem::size_of::<Row>() as u64);
                assert_eq!(borsh::from_slice::<Row>(&wire).unwrap(),row);
                let canonical=b.state.state_root();let n=b.entries.len();
                let mut other=c.clone();other.filer=producer;
                other.signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,other.signing_message(native.network_domain).unwrap().as_byte_slice(),
                    bottoms::SHARED_TIR_BOTTOM_CONTEXT_V1,[103;32]).unwrap().as_ref().to_vec();
                let (opened,again)=fold::bottom(&mut b,&at_rules.at,&other).unwrap();assert!(!opened);assert_eq!(again,outcome);
                assert_eq!(b.state.state_root(),canonical);assert_eq!(b.entries.len(),n);
                assert!(b.write_staged_row_v1(key,None).is_err());
                if let Row::CourtSharedBottom(mut changed)=row.clone(){changed.outcome_digest=h(0xBC03);assert!(changed.check_context(&b.state).is_err());
                    assert!(b.write_staged_row_v1(key,Some(Row::CourtSharedBottom(changed))).is_err());}
                let mut undone=b.state.clone();for e in b.entries.iter().rev(){apply_delta_entry(&mut undone,e,true).unwrap();}
                assert_eq!(undone.state_root(),round_state.state_root());for e in &b.entries{apply_delta_entry(&mut undone,e,false).unwrap();}
                assert_eq!(undone.state_root(),canonical);
                round_state=b.state.clone();round_state.set_last_point_for_tests(Some(at_rules.at));
                terminals+=1;continue;
            }
            let children=prepared.phase.child_ranges().len();
            let sum_index=checked.site().folds.iter().enumerate().find_map(|(i,f)|
                (matches!(f,crate::palw_tir_dissect_v1::PalwTirFoldV1::Sum) && !control.root.elements[i].is_empty()).then_some(i))
                .expect("the corpus has a nonempty sum reduction");
            let mut msg=crate::palw_tir_court_v1::build_tir_dissect_round_v1(binding,&prepared.phase,fx.layout.h_tile,&fx.execution,&court).unwrap();
            if false_partition {
                let bounds=checked.site().bounds[sum_index];
                let truth: i128=msg.children.iter().map(|c|c.partials[sum_index][0]).sum();
                let delta=prepared.phase.claim().partials[sum_index][0]-truth;
                if path.is_empty(){
                    let delta=[1i128,-1].into_iter().find(|d|{
                        let a=msg.children[0].partials[sum_index][0]+d;let b=msg.children[1].partials[sum_index][0]-d;
                        a>=bounds.lo && a<=bounds.hi && b>=bounds.lo && b<=bounds.hi
                    }).expect("the corpus has room for opposite signed partition errors");
                    msg.children[0].partials[sum_index][0]+=delta;msg.children[1].partials[sum_index][0]-=delta;
                }else if delta!=0 {
                    let child=msg.children.iter_mut().find(|c|{
                        let v=c.partials[sum_index][0]+delta;v>=bounds.lo && v<=bounds.hi
                    }).expect("the same lie can be pushed into an allowed child");
                    child.partials[sum_index][0]+=delta;
                }
            }
            let mut c=rounds::SignedSharedTirRoundV1 {version:1,claim,leaf,root_id:prepared.root_id,path:path.clone(),
                parent_id:prepared.parent_id,signer:producer,round:Box::new(msg),signature:vec![]};
            let round_sign=|c:&mut rounds::SignedSharedTirRoundV1,nonce:u8|{
                c.signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
                    c.signing_message(native.network_domain).unwrap().as_byte_slice(),rounds::SHARED_TIR_ROUND_CONTEXT_V1,[nonce;32])
                    .unwrap().as_ref().to_vec();
            };
            round_sign(&mut c,95);
            let mut at_rules=native;at_rules.at=at(frame,event_offset);event_offset+=1;
            let mut at_extras=root_extras.clone();at_extras.native_court_rules_v1=Some(at_rules);
            let mut b=TransitionBuilder::new(&round_state,&params,false,false,false,false,&at_extras);
            let unchanged=b.state.state_root();
            for kind in 0..4 {
                let mut bad=c.clone();
                match kind {0=>bad.parent_id=h(0xBAD1),1=>bad.root_id=h(0xBAD2),2=>bad.signer=outsider(),
                    3=>bad.round.children[0].partials[sum_index][0]+=1,_=>unreachable!()}
                round_sign(&mut bad,96);
                assert!(fold::round(&mut b,&at_rules.at,&bad).is_err());
                assert_eq!(b.state.state_root(),unchanged);assert!(b.entries.is_empty());
            }
            let mut legacy=c.clone();
            legacy.signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,
                &crate::palw_tir_dissect_v1::palw_tir_round_message_v1(&h(0x515),prepared.phase.round(),&c.round),
                crate::palw_court_v2::PALW_COURT_V2_MLDSA87_ATTN_RESPONDER_CONTEXT,[97;32]).unwrap().as_ref().to_vec();
            assert!(fold::round(&mut b,&at_rules.at,&legacy).is_err());
            let (m,p)=numerical_parts(N::Round(&c),set.scope_id().unwrap(),&at_rules);
            assert_eq!(fold::numerical_transport(&mut b,&at_rules.at,&m,&p).unwrap(),fold::NumericalTransportEffectV1::Round(true));
            let row_key=Key::CourtSharedRound(c.id().unwrap());
            assert_eq!(borsh::to_vec(&row_key).unwrap()[0],23);
            let row=b.state.staged_court_row_v1(&row_key).unwrap().clone();let row_wire=borsh::to_vec(&row).unwrap();
            assert_eq!(row_wire[0],29);
            let shape=preflight_native_value_v1::<Row>(row_wire.as_slice(),row_wire.len() as u64).unwrap();
            assert!(shape.logical_requests_v1()>=row.native_heap_v1("round").bytes().unwrap()+std::mem::size_of::<Row>() as u64);
            assert_eq!(borsh::from_slice::<Row>(&row_wire).unwrap(),row);
            assert!(preflight_native_value_v1::<Row>(&row_wire[..row_wire.len()-1],row_wire.len() as u64-1).is_err());
            if path.is_empty(){
                let path_at=1+2+2+64+8+64;
                let mut bad=row_wire[..path_at].to_vec();bad.extend_from_slice(&u32::MAX.to_le_bytes());
                assert!(borsh::from_slice::<Row>(&bad).is_err());
            }
            let canonical=b.state.state_root();let n=b.entries.len();
            let mut retry=c.clone();round_sign(&mut retry,98);
            assert!(!fold::round(&mut b,&at_rules.at,&retry).unwrap());
            assert_eq!(b.state.state_root(),canonical);assert_eq!(b.entries.len(),n);
            assert!(b.write_staged_row_v1(row_key,None).is_err());
            let mut undone=b.state.clone();
            for e in b.entries.iter().rev(){apply_delta_entry(&mut undone,e,true).unwrap();}
            assert_eq!(undone.state_root(),round_state.state_root());
            for e in &b.entries{apply_delta_entry(&mut undone,e,false).unwrap();}
            assert_eq!(undone.state_root(),canonical);
            round_state=b.state.clone();round_state.set_last_point_for_tests(Some(at_rules.at));
            retained_controls.push(c);disclosures+=1;
            // Visit the other sibling first; first arrival never closes a path.
            for child in (0..children).rev(){let mut next=path.clone();next.push(child as u8);paths.push_back(next);}
        }
        assert_eq!(disclosures,11);assert_eq!(terminals,12);
        if false_partition {assert!(local_faults>0);} else {assert_eq!(local_faults,0);}
        let bytes=borsh::to_vec(&PalwStateCarriageV2::from_state(&round_state)).unwrap();
        preflight_native_carriage_v1(bytes.as_slice(),bytes.len() as u64).unwrap();
        let round_fresh=borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap().into_state(&params,Some(round_state.state_root())).unwrap();
        assert_eq!(round_fresh.state_root(),round_state.state_root());
        for c in &retained_controls {
            let key=Key::CourtSharedRound(c.id().unwrap());
            let Some(Row::CourtSharedRound(row))=round_fresh.staged_court_row_v1(&key) else{panic!()};
            row.check_loaded_context(&round_fresh).unwrap();
            assert!(rounds::prepare_shared_tir_round_v1(&round_fresh,claim,leaf,&c.path).is_ok());
        }
        let child=retained_controls.iter().find(|c|!c.path.is_empty()).unwrap();
        let mut orphan=round_fresh.clone();orphan.staged_courts.remove(&Key::CourtSharedRound(retained_controls[0].id().unwrap()));
        let Some(Row::CourtSharedRound(row))=round_fresh.staged_court_row_v1(&Key::CourtSharedRound(child.id().unwrap())) else{panic!()};
        assert!(row.check_loaded_context(&orphan).is_err());
        let mut too_long=child.clone();too_long.path=vec![0;rounds::SHARED_TIR_MAX_PATH_V1+1];
        assert!(too_long.signing_message(native.network_domain).is_err());
        let mut round_swept=TransitionBuilder::new(&round_fresh,&params,false,false,false,false,&root_extras);
        super::super::palw_staged_court_fold_v1::sweep(&mut round_swept,&past).unwrap();
        assert!(round_swept.state.staged_court_row_v1(&Key::CourtSharedRound(child.id().unwrap())).is_some());
        assert_eq!(round_fresh.court_sessions_len(),0);
        for (_,row) in &round_fresh.staged_courts {
            if let Row::CourtSharedBottom(row)=row {row.check_loaded_context(&round_fresh).unwrap();}
        }
        }
    }

    #[test]
    fn rooted_shared_query_and_signed_streams_work_with_all_native_owner_slots_occupied_and_reload() {
        exercise_rooted_shared_streams(palw_tir_public_frame_fixture_v1(Some((7,1)),4),7,false);
    }
    #[test]
    fn rooted_shared_wide_leaf_stream_crosses_part_boundary_and_reloads() {
        let fx=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_wide_v1(Some((7,1)),4);
        let leaf=fx.execution.preimages.iter().enumerate().max_by_key(|(_,p)|p.values_le.len()).unwrap().0;
        exercise_rooted_shared_streams(fx,leaf,true);
    }
    fn exercise_rooted_shared_streams(fx:PalwTirPublicFrameFixtureV1,leaf_index:usize,multipart:bool) {
        use crate::palw_court_shared_tir_state_v1::{self as shared,RootedSharedDataV1};
        use crate::palw_court_shared_tir_frontier_v1::{SignedSharedRequestV1,SharedRequestEffectV1,SHARED_REQUEST_CONTEXT_V1};
        use super::super::palw_court_shared_tir_fold_v1 as fold;
        let frame=&fx.frame;let claim=frame.plan_v1().original_claim;
        let colluders=squatters(Frame::MAXIMUM_OWNERS_V1);
        let (mut state,params,extras,offset)=open_world(&fx,frame,&colluders);
        for owner in &colluders {state=open(frame,&state,&params,&extras,offset,*owner).unwrap();}
        assert_eq!(state.staged_tir_owner_count_v1(&claim),Frame::MAXIMUM_OWNERS_V1);
        assert!(open(frame,&state,&params,&extras,offset+1,outsider()).is_err());
        let honest=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([11;32]);
        let colluder=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([12;32]);
        state.bonds.get_mut(&outsider()).unwrap().pubkey=honest.verification_key.as_ref().to_vec();
        state.bonds.get_mut(&colluders[0]).unwrap().pubkey=colluder.verification_key.as_ref().to_vec();
        let producer=frame.custody_v1().pledge_v1().producer;
        let publisher=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([31;32]);
        // Existing Ready/original installation is a verified-half fixture, not
        // a key-rotation or real registration transaction. New controls use real signatures.
        state.bonds.get_mut(&producer).unwrap().pubkey=publisher.verification_key.as_ref().to_vec();
        state=bind_clock_service(&state,frame,&params,&extras);
        let profile=shared::shared_native_profile_id_v1(state.court_service_state_v1().unwrap().profile_id);
        let source=frame.source_v1(&fx.record,&fx.layout,fx.rules.maximum_leaves).unwrap();
        let set=crate::palw_court_tir_requests_v1::TirSharedRequestSetV1::new(frame,&source,profile).unwrap();
        let p=frame.plan_v1();let context=shared::SharedTirContextV1 {version:1,claim,allocation_id:p.allocation_id,
            source_id:p.source_id,rules_id:p.rules_id,profile_id:profile,scope_id:set.scope_id().unwrap(),queries:set.len(),
            original_accepted_daa:p.terms.accepted_daa,hard_daa:p.terms.accepted_daa+10_000,material_until_daa:p.terms.material_until_daa};
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        // Trusted internal context fixture, NOT a complete paid original or a
        // timely initial court notice; the synthetic unit-cost book is unchanged.
        b.write_staged_row_v1(Key::CourtSharedContext(claim),Some(Row::CourtSharedContext(Box::new(context)))).unwrap();
        let installed=b.state.clone();
        let find=|query| (0..set.len()).find(|&i|set.request_at(i).unwrap()==query).unwrap();
        let first=find(crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf {index:leaf_index as u64});
        let next=find(crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepRun {first:0,count:1});
        let ids_ordinal=find(crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::Ids {offset:0});
        let make=|ordinal,actor,seed:u8| {
            let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed;32]);
            let mut intent=SignedSharedRequestV1 {version:1,scope:set.scope_id().unwrap(),ordinal,
                request_id:set.request_id(ordinal).unwrap(),signer:actor,signature:vec![]};
            intent.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,
                intent.signing_message(extras.tir_public_network_domain_v1.unwrap()).unwrap().as_byte_slice(),
                SHARED_REQUEST_CONTEXT_V1,[13;32]).unwrap().as_ref().to_vec();intent
        };
        let ctx=at(frame,offset+1);
        assert_eq!(fold::request(&mut b,&ctx,claim,&make(first,colluders[0],12)).unwrap(),SharedRequestEffectV1::Opened);
        let after_first=b.state.state_root();let entries=b.entries.len();
        assert_eq!(fold::request(&mut b,&ctx,claim,&make(first,outsider(),11)).unwrap(),SharedRequestEffectV1::Coalesced);
        assert_eq!(b.state.state_root(),after_first);assert_eq!(b.entries.len(),entries);
        assert_eq!(fold::request(&mut b,&ctx,claim,&make(next,outsider(),11)).unwrap(),SharedRequestEffectV1::Opened);
        assert_eq!(b.state.staged_tir_owner_count_v1(&claim),Frame::MAXIMUM_OWNERS_V1,"shared query did not claim/evict an owner slot");
        let body=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:fx.execution.preimages[leaf_index].clone(),opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,leaf_index as u64).unwrap()}).unwrap();
        if multipart {assert!(body.len()>crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1,"wide original must actually require multiple parts");}
        use crate::palw_court_shared_tir_stream_v1::{SignedSharedStreamV1,SharedStreamTargetV1 as StreamTarget,SharedStreamActionV1 as StreamAction,SHARED_STREAM_CONTEXT_V1};
        let sign_stream=|target:StreamTarget,actor,seed:u8,action:StreamAction| {
            let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed;32]);
            let mut control=SignedSharedStreamV1 {version:1,claim,scope:set.scope_id().unwrap(),target,signer:actor,action,signature:vec![]};
            control.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,control.signing_message(extras.tir_public_network_domain_v1.unwrap()).unwrap().as_byte_slice(),
                SHARED_STREAM_CONTEXT_V1,[32;32]).unwrap().as_ref().to_vec();control
        };
        use crate::palw_court_shared_tir_response_v1::{self as response,SignedSharedDataResponseV1,SHARED_DATA_RESPONSE_CONTEXT_V1};
        let request_id=set.request_id(first).unwrap();
        let make_response=|node:&PalwChainStateV2,actor,seed:u8,digest:Hash64| {
            let q=response::query(node,request_id).unwrap();let kp=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([seed;32]);
            let mut r=SignedSharedDataResponseV1 {version:1,claim,scope_id:q.scope_id,request_id,signer:actor,
                query_block:q.opened_block,query_daa:q.opened_daa,data_digest:digest,signature:vec![]};
            r.signature=libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key,r.signing_message(extras.tir_public_network_domain_v1.unwrap()).unwrap().as_byte_slice(),
                SHARED_DATA_RESPONSE_CONTEXT_V1,[33;32]).unwrap().as_ref().to_vec();r
        };
        let stage=response::shared_data_response_stage_v1(&b.state,request_id,producer).unwrap();
        assert!(stage.data.is_none() && stage.response.is_none() && stage.resource_first_eligible_daa.is_none());
        let before=b.state.state_root();let missing_data=make_response(&b.state,producer,31,Hash64::default());
        assert!(fold::data_response(&mut b,&at(frame,offset+2),&missing_data).is_err());assert_eq!(b.state.state_root(),before);
        let stream_target=StreamTarget::Unit {ordinal:first,request_id:set.request_id(first).unwrap()};
        let begin=StreamAction::Begin {total_bytes:body.len() as u64,body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body)};
        let control=sign_stream(stream_target.clone(),producer,31,begin.clone());let slot=control.slot_id().unwrap();
        let stream_id=control.declared_stream_id(body.len() as u64,crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body)).unwrap();
        let before=b.state.state_root();let n=b.entries.len();
        assert!(fold::stream_control(&mut b,&ctx,&sign_stream(stream_target.clone(),outsider(),11,begin)).is_err(),"active signer without original Ready cannot upload");
        let mut forged=control.clone();forged.signature[0]^=1;
        assert!(fold::stream_control(&mut b,&ctx,&forged).is_err());assert_eq!(b.state.state_root(),before);assert_eq!(b.entries.len(),n);
        assert!(fold::stream_control(&mut b,&ctx,&control).unwrap());
        let begun=b.state.state_root();assert!(!fold::stream_control(&mut b,&at(frame,offset+2),&control).unwrap());assert_eq!(b.state.state_root(),begun,"Begin retry cannot redate");
        let replacement=sign_stream(stream_target.clone(),producer,31,StreamAction::Begin {total_bytes:1,body_hash:Hash64::default()});
        assert!(fold::stream_control(&mut b,&ctx,&replacement).is_err());assert_eq!(b.state.state_root(),begun);
        let finish=sign_stream(stream_target.clone(),producer,31,StreamAction::Finish {stream_id});
        assert!(fold::stream_control(&mut b,&ctx,&finish).is_err());assert_eq!(b.state.state_root(),begun,"incomplete EOF rolls back");
        let mut cursor=0u64;
        for part in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) {
            let part_control=sign_stream(stream_target.clone(),producer,31,StreamAction::Part {stream_id,offset:cursor,bytes:part.to_vec()});
            let mut wrong=part_control.clone();wrong.action=StreamAction::Part {stream_id,offset:cursor+1,bytes:part.to_vec()};
            wrong.signature=libcrux_ml_dsa::ml_dsa_87::sign(&publisher.signing_key,wrong.signing_message(extras.tir_public_network_domain_v1.unwrap()).unwrap().as_byte_slice(),SHARED_STREAM_CONTEXT_V1,[32;32]).unwrap().as_ref().to_vec();
            let before=b.state.state_root();assert!(fold::stream_control(&mut b,&ctx,&wrong).is_err());assert_eq!(b.state.state_root(),before);
            assert!(fold::stream_control(&mut b,&ctx,&part_control).unwrap());let prefix=b.state.state_root();
            assert!(!fold::stream_control(&mut b,&ctx,&part_control).unwrap());assert_eq!(b.state.state_root(),prefix);
            cursor+=part.len() as u64;
        }
        // Full-state checkpoints must share all retained chunk payloads.
        // Box storage would copy the growing prefix at every operation.
        let checkpoint=b.try_checkpoint().unwrap();
        let Some(Row::CourtSharedChunk(live))=b.state.staged_court_row_v1(&Key::CourtSharedChunk(slot,0)) else {panic!()};
        let Some(Row::CourtSharedChunk(saved))=checkpoint.0.staged_court_row_v1(&Key::CourtSharedChunk(slot,0)) else {panic!()};
        assert_eq!(live.bytes.as_ptr(),saved.bytes.as_ptr(),"checkpoint must not deep-copy retained prefix bytes");
        drop(checkpoint);
        assert!(shared::read_shared_data_v1(&b.state,claim,first).unwrap().is_none(),"prefix is not authenticated public data");
        let claim_before_stream=b.state.claim(&claim).cloned();assert!(fold::stream_control(&mut b,&ctx,&finish).unwrap());
        assert_eq!(b.state.claim(&claim),claim_before_stream.as_ref(),"data EOF never certifies computation");
        let finished=b.state.state_root();assert!(!fold::stream_control(&mut b,&ctx,&finish).unwrap());assert_eq!(b.state.state_root(),finished);
        let data_stage=response::shared_data_response_stage_v1(&b.state,request_id,producer).unwrap();
        assert!(data_stage.data.is_some() && data_stage.response.is_none(),"signed upload EOF is not a fresh query response");
        assert_eq!(data_stage.resource_first_eligible_daa,Some(ctx.daa_score+1));

        // The wide fixture exercises full-size + final-ragged parts with a source-bounded but
        // invalid run body. EOF failure retains exactly the accepted prefix.
        let run_count=source.leaf_count_v1().min(crate::palw_tir_public_source_v1::PALW_TIR_PUBLIC_RUN_MAX_LEAVES_V1 as u64) as u32;
        let run_query=crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepRun {first:0,count:run_count};
        let run_ordinal=find(run_query);fold::request(&mut b,&ctx,claim,&make(run_ordinal,outsider(),11)).unwrap();
        let run_target=StreamTarget::Unit {ordinal:run_ordinal,request_id:set.request_id(run_ordinal).unwrap()};
        let invalid_len=source.unit_bound_v1(&run_query).unwrap().maximum_response_bytes.min((crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1+1) as u64) as usize;
        let invalid_body=vec![0;invalid_len];
        if multipart {assert!(invalid_len>crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1); }
        let bad_begin=sign_stream(run_target.clone(),producer,31,StreamAction::Begin {total_bytes:invalid_body.len() as u64,body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&invalid_body)});
        let bad_stream=bad_begin.declared_stream_id(invalid_body.len() as u64,crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&invalid_body)).unwrap();
        assert!(fold::stream_control(&mut b,&ctx,&bad_begin).unwrap());
        for (i,part) in invalid_body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            assert!(fold::stream_control(&mut b,&ctx,&sign_stream(run_target.clone(),producer,31,StreamAction::Part {stream_id:bad_stream,offset:(i*crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,bytes:part.to_vec()})).unwrap());
        }
        let invalid_prefix=b.state.state_root();let invalid_entries=b.entries.len();
        assert!(fold::stream_control(&mut b,&ctx,&sign_stream(run_target,producer,31,StreamAction::Finish {stream_id:bad_stream})).is_err());
        assert_eq!(b.state.state_root(),invalid_prefix);assert_eq!(b.entries.len(),invalid_entries);
        assert!(shared::read_shared_data_v1(&b.state,claim,run_ordinal).unwrap().is_none());
        let before=b.state.state_root();let n=b.entries.len();
        assert!(fold::share_body(&mut b,&ctx,claim,first,&body[..body.len()-1]).is_err());
        assert_eq!(b.state.state_root(),before);assert_eq!(b.entries.len(),n);
        let claim_before=b.state.claim(&claim).cloned();
        // The narrow case shares the forged leaf; the wide case shares the
        // widest leaf of an execution containing a committed lie. Public
        // source authentication must retain it for semantic refutation, never
        // certify computation or advance credit merely because it is available.
        assert!(!fold::share_body(&mut b,&ctx,claim,first,&body).unwrap(),"signed EOF already installed this data");
        assert_eq!(b.state.claim(&claim),claim_before.as_ref());
        assert!(!fold::share_body(&mut b,&ctx,claim,first,&body).unwrap());
        assert!(matches!(shared::read_shared_data_v1(&b.state,claim,first).unwrap(),Some(RootedSharedDataV1::Unit(unit))
            if unit.source_id_v1()==p.source_id && borsh::to_vec(unit.response_v1()).unwrap()==body));
        let trace_ordinal=find(crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::TraceTile {row:0,tile:0});
        fold::request(&mut b,&ctx,claim,&make(trace_ordinal,outsider(),11)).unwrap();
        let event=crate::palw_tir_court_v1::tir_logits_event_disclosure_v1(&fx.execution.binding,&fx.execution.rows,&fx.execution.generated,0,0).unwrap();
        let crate::palw_tir_court_v1::PalwTirTraceEventDisclosureV1::Tiled {row_root,row_opening,tile_lanes,tile_opening,..}=event else {panic!("tiled fixture")};
        let trace_body=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::TraceTile {
            row_root,row_opening,lanes:tile_lanes,opening:tile_opening}).unwrap();
        let before=b.state.state_root();let journal=b.entries.len();
        assert!(fold::share_body(&mut b,&ctx,claim,trace_ordinal,&trace_body).is_err(),"trace still needs authenticated IDs");
        assert_eq!(b.state.state_root(),before);assert_eq!(b.entries.len(),journal);
        fold::request(&mut b,&ctx,claim,&make(ids_ordinal,outsider(),11)).unwrap();
        let rows=crate::palw_step_refute::tiled_logits_rows_root_v1(&fx.execution.binding.job_context,&fx.execution.rows).unwrap();
        let mut bad_ids=fx.execution.generated.clone();bad_ids[0]^=1;
        let before=b.state.state_root();assert!(fold::share_ids(&mut b,&ctx,claim,rows,&bad_ids).is_err());assert_eq!(b.state.state_root(),before);
        let ids_body=crate::palw_tir_public_owned_v1::palw_tir_public_ids_body_v1(&source,rows,&fx.execution.generated).unwrap();
        let ids_hash=crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&ids_body);
        let ids_begin=sign_stream(StreamTarget::FullIds,producer,31,StreamAction::Begin {total_bytes:ids_body.len() as u64,body_hash:ids_hash});
        let ids_stream=ids_begin.declared_stream_id(ids_body.len() as u64,ids_hash).unwrap();
        assert!(fold::stream_control(&mut b,&ctx,&ids_begin).unwrap());
        for (i,part) in ids_body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
            assert!(fold::stream_control(&mut b,&ctx,&sign_stream(StreamTarget::FullIds,producer,31,StreamAction::Part {stream_id:ids_stream,offset:(i*crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,bytes:part.to_vec()})).unwrap());
        }
        assert!(shared::read_shared_data_v1(&b.state,claim,ids_ordinal).unwrap().is_none());
        assert!(fold::stream_control(&mut b,&ctx,&sign_stream(StreamTarget::FullIds,producer,31,StreamAction::Finish {stream_id:ids_stream})).unwrap());
        assert!(!fold::share_ids(&mut b,&ctx,claim,rows,&fx.execution.generated).unwrap());
        assert!(matches!(shared::read_shared_data_v1(&b.state,claim,ids_ordinal).unwrap(),Some(RootedSharedDataV1::Ids {rows_root,ids})
            if rows_root==rows && ids==fx.execution.generated.as_slice()));
        assert!(fold::share_body(&mut b,&ctx,claim,trace_ordinal,&trace_body).unwrap());
        assert!(matches!(shared::read_shared_data_v1(&b.state,claim,trace_ordinal).unwrap(),Some(RootedSharedDataV1::Unit(unit))
            if borsh::to_vec(unit.response_v1()).unwrap()==trace_body));
        let data=response::shared_data_fingerprint_v1(&b.state,request_id).unwrap().unwrap();
        let signed_response=make_response(&b.state,producer,31,data);let reply_ctx=at(frame,offset+2);
        let before=b.state.state_root();let entries=b.entries.len();
        assert!(fold::data_response(&mut b,&ctx,&signed_response).is_err(),"response binds an already accepted query block");
        let non_ready=make_response(&b.state,outsider(),11,data);assert!(fold::data_response(&mut b,&reply_ctx,&non_ready).is_err());
        let mut wrong=signed_response.clone();wrong.signature[0]^=1;
        assert!(fold::data_response(&mut b,&reply_ctx,&wrong).is_err());
        let wrong_data=make_response(&b.state,producer,31,Hash64::default());assert!(fold::data_response(&mut b,&reply_ctx,&wrong_data).is_err());
        assert_eq!(b.state.state_root(),before);assert_eq!(b.entries.len(),entries);
        let claim_before_reply=b.state.claim(&claim).cloned();assert!(fold::data_response(&mut b,&reply_ctx,&signed_response).unwrap());
        assert_eq!(b.state.claim(&claim),claim_before_reply.as_ref());
        let replied=b.state.state_root();let n=b.entries.len();
        assert!(!fold::data_response(&mut b,&at(frame,offset+3),&signed_response).unwrap());assert_eq!(b.state.state_root(),replied);assert_eq!(b.entries.len(),n);
        let stage=response::shared_data_response_stage_v1(&b.state,request_id,producer).unwrap();
        assert_eq!(stage.response.as_ref().unwrap().accepted_block,reply_ctx.block);
        assert!(b.state.court_service_state_v1().unwrap().default_hold(frame).is_err(),"data receipt never authorizes default");
        // A reaccepted query on an alternate branch has the same shared query
        // ID/data but a different accepting block. Old response signatures fail.
        let mut alt=TransitionBuilder::new(&installed,&params,false,false,false,false,&extras);
        let alt_open=at(frame,offset+4);fold::request(&mut alt,&alt_open,claim,&make(first,outsider(),11)).unwrap();
        fold::share_body(&mut alt,&alt_open,claim,first,&body).unwrap();
        let alt_root=alt.state.state_root();assert!(fold::data_response(&mut alt,&at(frame,offset+5),&signed_response).is_err());assert_eq!(alt.state.state_root(),alt_root);
        let replacement=make_response(&alt.state,producer,31,response::shared_data_fingerprint_v1(&alt.state,request_id).unwrap().unwrap());
        assert!(fold::data_response(&mut alt,&at(frame,offset+5),&replacement).unwrap(),"data uploaded by a helper still needs the designated signer's own fresh response");
        assert!(fold::data_response(&mut b,&at(frame,10_000),&signed_response).is_err());
        assert!(b.write_staged_row_v1(Key::CourtSharedResponse(request_id,producer),None).is_err());
        let accepted=b.state.clone();let mut undone=accepted.clone();
        for entry in b.entries.iter().rev() {apply_delta_entry(&mut undone,entry,true).unwrap();}
        assert_eq!(undone.state_root(),state.state_root());
        for entry in &b.entries {apply_delta_entry(&mut undone,entry,false).unwrap();}
        assert_eq!(undone.state_root(),accepted.state_root());
        let reload=|node:&PalwChainStateV2| {
            borsh::from_slice::<PalwStateCarriageV2>(&borsh::to_vec(&PalwStateCarriageV2::from_state(node)).unwrap()).unwrap()
                .into_state(&params,Some(node.state_root()))
        };
        let fresh=reload(&accepted).unwrap();
        let mut bad_prefix=accepted.clone();
        let chunk_key=Key::CourtSharedChunk(slot,0);
        let Some(Row::CourtSharedChunk(chunk))=bad_prefix.staged_courts.get_mut(&chunk_key) else {panic!()};chunk.bytes[0]^=1;
        assert!(reload(&bad_prefix).is_err(),"raw prefix must reproduce retained head and original EOF");
        let mut orphan=accepted.clone();orphan.staged_courts.remove(&chunk_key);assert!(reload(&orphan).is_err());
        let before=b.state.state_root();
        assert!(fold::stream_control(&mut b,&at(frame,10_000),&finish).is_err());assert_eq!(b.state.state_root(),before);
        assert!(b.write_staged_row_v1(Key::CourtSharedStream(slot),None).is_err());
        assert!(b.write_staged_row_v1(chunk_key,None).is_err());

        assert_eq!(fresh.state_root(),accepted.state_root());
        let mut wrong_reply=fresh.clone();let response_key=Key::CourtSharedResponse(request_id,producer);
        let Some(Row::CourtSharedResponse(row))=wrong_reply.staged_courts.get_mut(&response_key) else {panic!()};row.data_digest=Hash64::default();
        assert!(reload(&wrong_reply).is_err(),"response metadata must reauthenticate actual public data");
        let mut wrong_opening=fresh.clone();let Some(Row::CourtSharedResponse(row))=wrong_opening.staged_courts.get_mut(&response_key) else {panic!()};
        row.query_block=Hash64::from_u64_word(999);assert!(reload(&wrong_opening).is_err(),"receipt cannot move to another accepted query context");

        assert!(shared::read_shared_data_v1(&fresh,claim,first).unwrap().is_some());
        let query_key=Key::CourtSharedQuery(set.request_id(first).unwrap());
        let mut poisoned=fresh.clone();let Some(Row::CourtSharedQuery(row))=poisoned.staged_courts.get_mut(&query_key) else {panic!()};
        let body=row.body.as_mut().unwrap();let mut leaf=fx.execution.preimages[leaf_index].clone();leaf.values_le[0]^=1;
        body.bytes=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:leaf,opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,leaf_index as u64).unwrap()}).unwrap();
        assert!(source.decode_response_v1(&crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf {index:leaf_index as u64},&body.bytes).is_ok());
        body.digest=shared::shared_body_digest_v1(&body.bytes);
        row.check(&query_key).unwrap();assert!(reload(&poisoned).is_err(),"matching stored digest cannot replace original-root authentication");
        let mut wrong_ids=fresh.clone();let Some(Row::CourtSharedIds(ids))=wrong_ids.staged_courts.get_mut(&Key::CourtSharedIds(claim)) else {panic!()};
        ids.ids[0]^=1;assert!(reload(&wrong_ids).is_err(),"raw persisted IDs are reauthenticated, not trusted as a sealed flag");
        // Direct invalid-state counterexample, not an accepted native mutation.
        // Step-root access is independent; trace access and whole-state loading
        // must still reject the corrupt outer-root dependency.
        assert!(shared::read_shared_data_v1(&wrong_ids,claim,first).unwrap().is_some());
        assert!(shared::read_shared_data_v1(&wrong_ids,claim,trace_ordinal).is_err());
        let mut missing_ids=fresh.clone();missing_ids.staged_courts.remove(&Key::CourtSharedIds(claim));
        assert!(shared::read_shared_data_v1(&missing_ids,claim,first).unwrap().is_some());
        assert!(shared::read_shared_data_v1(&missing_ids,claim,trace_ordinal).is_err());assert!(reload(&missing_ids).is_err());
        let mut no_requester=fresh.clone();no_requester.bonds.remove(&outsider());
        let Some(Row::CourtSharedQuery(q))=no_requester.staged_court_row_v1(&Key::CourtSharedQuery(set.request_id(next).unwrap())) else {panic!()};
        q.check_context(&no_requester).unwrap();
        let mut changed=TransitionBuilder::new(&fresh,&params,false,false,false,false,&extras);
        assert!(changed.write_staged_row_v1(Key::CourtSharedContext(claim),None).is_err());
        assert!(changed.write_staged_row_v1(Key::CourtService,None).is_err());
        let past=at(frame,p.terms.material_until_daa-p.terms.accepted_daa+1);
        super::super::palw_staged_court_fold_v1::sweep(&mut changed,&past).unwrap();
        assert!(changed.state.staged_court_row_v1(&query_key).is_some());
        let hard=at(frame,10_000);let before=changed.state.state_root();
        assert!(fold::request(&mut changed,&hard,claim,&make(first,outsider(),11)).is_err());assert_eq!(changed.state.state_root(),before);
        // Source/signature/native journal fixture only; no actual lifecycle,
        // original all-branch funding, refilling/late adversary campaign or node T05.
        assert!(installed.staged_tir_owner_count_v1(&claim)==Frame::MAXIMUM_OWNERS_V1);
    }

    fn bind_clock_service(state: &PalwChainStateV2, frame: &Frame, params: &PalwStateParamsV2, extras: &PalwTransitionExtrasV1) -> PalwChainStateV2 {
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding, court_service_profile_id_v2};
        use crate::palw_court_service_v1::{ServiceCost,ServicePacket,ServiceReservations};
        let p=frame.plan_v1();let cost=ServiceCost { bytes:1,work:1,io:1,controls:1 };
        let mut book=ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket { id:[71;64],release_daa:p.terms.accepted_daa+1,
            service_before:p.terms.accepted_daa+2,cost }],p.terms.accepted_daa+10_000).unwrap();
        let source_profile_id=Hash64::from_u64_word(72);
        let service=CourtServiceStateV2 { version:2,source_profile_id,profile_id:court_service_profile_id_v2(source_profile_id),book,
            claims:std::collections::BTreeMap::from([(p.original_claim,ServiceClaimBinding {
                claim:p.original_claim,allocation_id:p.allocation_id,source_id:p.source_id,
                original_accepted_daa:p.terms.accepted_daa,hard_deadline_daa:p.terms.accepted_daa+10_000,
                packet_ids:std::collections::BTreeSet::from([[71;64]]) })]) };
        let mut b=TransitionBuilder::new(state,params,false,false,false,false,extras);
        b.write_staged_row_v1(Key::CourtService,Some(Row::CourtService(Box::new(service).into()))).unwrap();
        b.state.clone()
    }
    #[test]
    fn native_duty_clock_records_actual_stages_and_survives_native_restore() {
        use crate::palw_court_tir_requests_v1::tir_native_response_duty_v1 as duty;
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=&fx.frame;
        let (old,params,extras,target,offset)=demanded_world(&fx,frame,0);
        let state=bind_clock_service(&old,frame,&params,&extras);
        let T::Acquisition { owner,reserved_daa }=target else { unreachable!() };
        let key=owner_key(frame,owner,reserved_daa).unwrap();let actor=frame.custody_v1().pledge_v1().producer;
        let profile=state.court_service_state_v1().unwrap().profile_id;let ctx=at(frame,offset);
        let initial=duty(&state,&ctx,key,actor,profile).unwrap().unwrap();
        assert!(state.staged_court_row_v1(&Key::CourtDutyClock(initial.id)).is_none(),"binding cannot backfill an old demand");
        let body=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:fx.execution.preimages[0].clone(),opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,0).unwrap() }).unwrap();
        let begin=Owned::Begin { demand_id:initial.demand_id,answerer:actor,total_bytes:body.len() as u64,
            body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) };
        let make=|action| Control { version:1,claim:initial.claim,allocation_id:initial.allocation_id,source_id:initial.source_id,
            target,expected_turn:initial.turn,declared_rent_sompi:0,signer:actor,action:A::Acquisition(action),signature:vec![5;SIG] };
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        apply_verified_control_v1(&mut b,&ctx,frame,&make(begin.clone())).unwrap();
        let part=duty(&b.state,&ctx,key,actor,profile).unwrap().unwrap();let part_key=Key::CourtDutyClock(part.id);
        let Some(Row::CourtDutyClock(first))=b.state.staged_court_row_v1(&part_key) else { panic!("Begin did not record Part prerequisite") };
        assert_eq!((first.predecessor_block,first.predecessor_daa),(ctx.block,ctx.daa_score));
        assert!(!first.stage_started_before_unit(ctx.daa_score));assert!(first.stage_started_before_unit(ctx.daa_score+1));
        // Later acceptance of the same native stage must retain different history.
        let late=control_step(frame,&state,&params,&extras,offset+3,actor,target,A::Acquisition(begin),0,initial.turn).unwrap();
        let Some(Row::CourtDutyClock(later))=late.staged_court_row_v1(&part_key) else { panic!() };
        assert_ne!(first.predecessor_daa,later.predecessor_daa);
        assert!(!later.stage_started_before_unit(ctx.daa_score+1));
        // Deliberately inject contradictory historical provenance as a writer
        // failure fixture, NOT a claimed valid node history. The real verified
        // boundary must undo preceding stage writes when the marker conflicts.
        let retained=first.clone();
        let mut collision=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        collision.write_staged_row_v1(part_key,Some(Row::CourtDutyClock(later.clone()))).unwrap();
        let collision_root=collision.state.state_root();let entries=collision.entries.len();
        let err=apply_verified_control_v1(&mut collision,&ctx,frame,&make(Owned::Begin {
            demand_id:initial.demand_id,answerer:actor,total_bytes:body.len() as u64,
            body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) })).unwrap_err();
        assert!(format!("{err:?}").contains("immutable"));
        assert_eq!(collision.state.state_root(),collision_root);assert_eq!(collision.entries.len(),entries);
        assert!(b.write_staged_row_v1(Key::CourtService,None).is_err());
        let mut wrong_profile=b.state.court_service_state_v1().unwrap().clone();
        wrong_profile.source_profile_id=Hash64::from_u64_word(73);
        wrong_profile.profile_id=crate::palw_court_service_state_v2::court_service_profile_id_v2(wrong_profile.source_profile_id);
        assert!(b.write_staged_row_v1(Key::CourtService,Some(Row::CourtService(Box::new(wrong_profile).into()))).is_err());

        // Existing native controls may advance at the same block and DAA. The
        // accounting successor must not impose an extra native response delay.
        apply_verified_control_v1(&mut b,&ctx,frame,&make(Owned::Part { demand_id:initial.demand_id,answerer:actor,offset:0,bytes:body })).unwrap();
        let finish=duty(&b.state,&ctx,key,actor,profile).unwrap().unwrap();let finish_key=Key::CourtDutyClock(finish.id);
        assert!(b.state.staged_court_row_v1(&finish_key).is_some());
        apply_verified_control_v1(&mut b,&ctx,frame,&make(Owned::Finish { demand_id:initial.demand_id,answerer:actor })).unwrap();
        assert!(duty(&b.state,&ctx,key,actor,profile).unwrap().is_none());
        assert_eq!(b.state.staged_court_row_v1(&part_key),Some(&Row::CourtDutyClock(retained)));
        let accepted=b.state.clone();let mut undone=accepted.clone();
        for entry in b.entries.iter().rev() { apply_delta_entry(&mut undone,entry,true).unwrap(); }
        assert_eq!(undone.state_root(),state.state_root());
        for entry in &b.entries { apply_delta_entry(&mut undone,entry,false).unwrap(); }
        assert_eq!(undone.state_root(),accepted.state_root());
        let bytes=borsh::to_vec(&crate::palw_state_v2::PalwStateCarriageV2::from_state(&accepted)).unwrap();
        let fresh=borsh::from_slice::<crate::palw_state_v2::PalwStateCarriageV2>(&bytes).unwrap()
            .into_state(&params,Some(accepted.state_root())).unwrap();
        assert_eq!(fresh.staged_court_row_v1(&finish_key),accepted.staged_court_row_v1(&finish_key));
        let mut swept=TransitionBuilder::new(&fresh,&params,false,false,false,false,&extras);
        let past=at(frame,frame.plan_v1().terms.material_until_daa-frame.plan_v1().terms.accepted_daa+1);
        super::super::palw_staged_court_fold_v1::sweep(&mut swept,&past).unwrap();
        assert!(swept.state.staged_court_row_v1(&finish_key).is_some(),"unresolved original retains prerequisite evidence beyond TTL");
        assert!(swept.write_staged_row_v1(finish_key,None).is_err());
        let mut substituted=fresh.clone();substituted.staged_courts.remove(&Key::CourtService);
        let Some(Row::CourtDutyClock(clock))=fresh.staged_court_row_v1(&finish_key) else { panic!() };
        assert!(clock.check_context(&substituted).is_err());
    }

    #[test]
    fn native_duty_clock_demand_and_latest_parts_keep_the_original_deadline() {
        use crate::palw_court_tir_requests_v1::tir_native_response_duty_v1 as duty;
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=&fx.frame;
        let (state,params,extras,target,offset)=demanded_world_inner(&fx,frame,0,true);
        let T::Acquisition { owner,reserved_daa }=target else { unreachable!() };
        let key=owner_key(frame,owner,reserved_daa).unwrap();let actor=frame.custody_v1().pledge_v1().producer;
        let profile=state.court_service_state_v1().unwrap().profile_id;
        let original=duty(&state,&at(frame,offset),key,actor,profile).unwrap().unwrap();
        let Some(Row::CourtDutyClock(opened))=state.staged_court_row_v1(&Key::CourtDutyClock(original.id)) else { panic!("actual Demand must record Begin provenance") };
        let demand_context=at(frame,offset-1);
        assert_eq!((opened.predecessor_block,opened.predecessor_daa),(demand_context.block,demand_context.daa_score));
        assert_eq!(opened.first_eligible_daa,original.opened_daa+1);
        let end=original.respond_until_daa-frame.plan_v1().terms.accepted_daa;
        let body=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:fx.execution.preimages[0].clone(),opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,0).unwrap() }).unwrap();
        let begun=control_step(frame,&state,&params,&extras,end-2,actor,target,A::Acquisition(Owned::Begin {
            demand_id:original.demand_id,answerer:actor,total_bytes:body.len() as u64,
            body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) }),0,original.turn).unwrap();
        let partial=control_step(frame,&begun,&params,&extras,end-1,actor,target,A::Acquisition(Owned::Part {
            demand_id:original.demand_id,answerer:actor,offset:0,bytes:body }),0,original.turn).unwrap();
        let finish=duty(&partial,&at(frame,end),key,actor,profile).unwrap().unwrap();
        assert_eq!(finish.respond_until_daa,original.respond_until_daa);
        let Some(Row::CourtDutyClock(clock))=partial.staged_court_row_v1(&Key::CourtDutyClock(finish.id)) else { panic!() };
        assert_eq!(clock.first_eligible_daa,original.respond_until_daa);
        assert!(!clock.stage_started_before_unit(original.respond_until_daa),"legal latest Part creates no invented future Finish opportunity");
        assert!(control_step(frame,&partial,&params,&extras,end,actor,target,A::Acquisition(Owned::Finish {
            demand_id:original.demand_id,answerer:actor }),0,original.turn).is_err(),"clock recording never extends native response authority");
        let mut b=TransitionBuilder::new(&partial,&params,false,false,false,false,&extras);
        let control=Control { version:1,claim:original.claim,allocation_id:original.allocation_id,source_id:original.source_id,
            target,expected_turn:original.turn,declared_rent_sompi:0,signer:owner,
            action:A::Acquisition(Owned::Collect { demand_id:original.demand_id }),signature:vec![5;SIG] };
        let before=b.state.state_root();let entries=b.entries.len();
        assert!(apply_verified_control_v1(&mut b,&at(frame,end),frame,&control).is_err(),"timing marker never bypasses the service default denial");
        assert_eq!(b.state.state_root(),before);assert_eq!(b.entries.len(),entries);
    }

    #[test]
    fn service_stage_eligibility_is_not_recoverable_from_current_tir_cursor_alone() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        use crate::palw_court_tir_requests_v1::tir_native_response_duty_v1 as duty;
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=&fx.frame;
        let (state,params,extras,target,offset)=demanded_world(&fx,frame,0);
        let T::Acquisition { owner,reserved_daa }=target else { unreachable!() };
        let key=owner_key(frame,owner,reserved_daa).unwrap();
        let actor=frame.custody_v1().pledge_v1().producer;
        let (demand_id,until,turn,_)=owner_pending(frame,&state,target);
        let body=borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
            preimage:fx.execution.preimages[0].clone(),
            opening:crate::palw_step_leg::step_opening_v1(&fx.execution.hashes,0).unwrap() }).unwrap();
        assert!(body.len()<=crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1);
        let begin=Owned::Begin { demand_id,answerer:actor,total_bytes:body.len() as u64,
            body_hash:crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body) };
        let part=Owned::Part { demand_id,answerer:actor,offset:0,bytes:body };
        let run=|begin_at,part_at| {
            let begun=control_step(frame,&state,&params,&extras,begin_at,actor,target,A::Acquisition(begin.clone()),0,turn).unwrap();
            control_step(frame,&begun,&params,&extras,part_at,actor,target,A::Acquisition(part.clone()),0,turn).unwrap()
        };
        let early_part_daa=frame.plan_v1().terms.accepted_daa+offset+1;
        let late_part_daa=frame.plan_v1().terms.accepted_daa+offset+4;
        assert!(late_part_daa<until);
        let early=run(offset,offset+1);let late=run(offset+3,offset+4);
        assert_eq!(early.staged_court_row_v1(&key),late.staged_court_row_v1(&key),
            "the retained acquisition cursor contains no predecessor accepting DAA");
        let profile=Hash64::from_u64_word(0x73);let common=at(frame,offset+5);
        let early_duty=duty(&early,&common,key,actor,profile).unwrap().unwrap();
        let late_duty=duty(&late,&common,key,actor,profile).unwrap().unwrap();
        assert_eq!(early_duty,late_duty,"the current duty view cannot recover the erased prerequisite timing");
        assert!(matches!(early_duty.stage,crate::palw_court_tir_requests_v1::TirNativeDutyStageV1::Finish { .. }));
        let assigned_daa=early_part_daa+1;
        assert!(early_part_daa<assigned_daa && late_part_daa>=assigned_daa,
            "one identical current cursor was eligible at that unit's start; the other was not");
        // Actual native controls above use different accepting contexts. This is
        // a retained-TIR-metadata counterexample, not a pair of identical node
        // block histories or a throughput measurement. Full journals could
        // distinguish the histories, but their lookup/retention is not a field
        // of the current book or duty view.
    }

    #[test]
    fn native_response_duty_reads_original_state_and_never_erases_expired_obligations() {
        use crate::palw_court_tir_requests_v1::{tir_native_response_duty_v1 as duty,TirNativeDutyClockV1 as Clock};
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=&fx.frame;
        let (state,_,_,target,offset)=demanded_world(&fx,frame,0);
        let T::Acquisition { owner,reserved_daa }=target else { unreachable!() };
        let key=owner_key(frame,owner,reserved_daa).unwrap();let actor=frame.custody_v1().pledge_v1().producer;
        let profile=Hash64::from_u64_word(0x73);
        let first=duty(&state,&at(frame,offset),key,actor,profile).unwrap().unwrap();
        assert_eq!(first.clock,Clock::ResponseWindow);assert!(first.response_fits_original_hard);
        let end=first.respond_until_daa-frame.plan_v1().terms.accepted_daa;
        let before=duty(&state,&at(frame,end-1),key,actor,profile).unwrap().unwrap();
        let expired=duty(&state,&at(frame,end),key,actor,profile).unwrap().unwrap();
        assert_eq!(before.clock,Clock::ResponseWindow);assert_eq!(expired.clock,Clock::ResponseExpired);
        assert_eq!(first.id,expired.id,"elapsed time cannot replace the owed response identity");
        assert_eq!(first.stage,expired.stage,"expiration is not fulfillment or permission to slash");
        let collection_end=match state.staged_court_row_v1(&key).unwrap() { Row::TirAcquisition(r)=>r.owned.contract_v1().terms.collect_until_daa,_=>unreachable!() };
        assert_eq!(duty(&state,&at(frame,collection_end-frame.plan_v1().terms.accepted_daa),key,actor,profile)
            .unwrap().unwrap().clock,Clock::CollectionClosed);
        assert!(duty(&state,&at(frame,offset),key,owner,profile).is_err(),"a voluntary owner response is not a literal custody debt");
        assert_ne!(first.id,duty(&state,&at(frame,offset),key,actor,Hash64::from_u64_word(0x74)).unwrap().unwrap().id);
        let mut missing=state.clone();missing.staged_courts.remove(&frame.preparation_key_v1());
        assert!(duty(&missing,&at(frame,offset),key,actor,profile).is_err());
        let mut missing=state.clone();missing.staged_courts.remove(&frame.key_v1());
        assert!(duty(&missing,&at(frame,offset),key,actor,profile).is_err(),"cannot silently fall back to an old bundle");
        assert!(duty(&state,&at(frame,offset),frame.preparation_key_v1(),actor,profile).is_err());
    }

    #[test]
    fn rooted_service_guard_denies_default_and_native_undo_restores_legacy() {
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let (state, params, extras, acquisition, _) = demanded_world(&fx, &frame, 0);
        let (demand_id, until, turn, _) = owner_pending(&frame, &state, acquisition);
        let base = frame.plan_v1().terms.accepted_daa;
        let ctx = at(&frame, until - base);
        let control = Control { version: 1, claim: frame.plan_v1().original_claim,
            allocation_id: frame.plan_v1().allocation_id, source_id: frame.plan_v1().source_id,
            target: acquisition, expected_turn: turn, declared_rent_sompi: 0, signer: outsider(),
            action: A::Acquisition(Owned::Collect { demand_id }), signature: vec![5; SIG] };
        // Test the verified half of the native control dispatcher. Fixture setup
        // and carrier signatures here are synthetic, not node-ingress evidence.
        let mut legacy = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut legacy, &ctx, &frame, &control).expect("legacy silence still defaults");
        assert_ne!(legacy.state.state_root(), state.state_root());
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        for supplied in [false, true] {
            let mut book = ServiceReservations::new(cost).unwrap();
            book.reserve(&[ServicePacket { id: [1; 64], release_daa: base + 1, service_before: base + 2, cost }], base + 10_000).unwrap();
            if supplied { book.observe_block(base + 1, [11; 64], Default::default()).unwrap(); }
            book.observe_block(base + 2, [12; 64], Default::default()).unwrap();
            let service = CourtServiceStateV2 { version: 2, source_profile_id: Hash64::from_u64_word(42), profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(Hash64::from_u64_word(42)), book,
                claims: std::collections::BTreeMap::from([(control.claim, ServiceClaimBinding {
                    claim: control.claim, allocation_id: control.allocation_id, source_id: control.source_id,
                    original_accepted_daa: base, hard_deadline_daa: base + 10_000,
                    packet_ids: std::collections::BTreeSet::from([[1; 64]]) })]) };
            let row = Row::CourtService(Box::new(service).into());
            row.check(&Key::CourtService).unwrap();
            let encoded = borsh::to_vec(&row).unwrap();
            let decoded: Row = borsh::from_slice(&encoded).unwrap();
            assert_eq!(decoded, row, "snapshot row preserves the entire book");
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            b.write_staged_row_v1(Key::CourtService, Some(decoded)).unwrap();
            // Before any shared context/duty clock is installed, the service
            // writer itself must retain obligations. A shape-valid empty book
            // used to remove this claim's holds without a terminal decision.
            let accepted = b.state.court_service_state_v1().unwrap().clone();
            let protected = b.state.clone();
            let journal = b.entries.len();
            assert!(b.write_staged_row_v1(Key::CourtService, None).is_err());
            let retyped = b.state.staged_court_row_v1(&frame.key_v1()).unwrap().clone();
            assert!(b.write_staged_row_v1(Key::CourtService, Some(retyped)).is_err());
            let mut candidates = Vec::new();
            let mut empty = accepted.clone();
            empty.claims.clear(); empty.book = ServiceReservations::new(cost).unwrap();
            candidates.push(empty);
            let mut profile = accepted.clone();
            profile.source_profile_id = Hash64::from_u64_word(43);
            profile.profile_id = crate::palw_court_service_state_v2::court_service_profile_id_v2(profile.source_profile_id);
            candidates.push(profile);
            let mut promise = accepted.clone();
            promise.book.packets.get_mut(&[1;64]).unwrap().service_before += 1;
            candidates.push(promise);
            let mut capacity = accepted.clone();
            capacity.book.budget.bytes += 1;
            for left in capacity.book.remaining.values_mut() { left.bytes += 1; }
            candidates.push(capacity);
            let mut clock = accepted.clone();
            clock.book.supply.clear(); clock.book.last_observed_daa = None;
            candidates.push(clock);
            if supplied {
                let mut erased = accepted.clone(); erased.book.supply.clear(); candidates.push(erased);
                let mut rewritten = accepted.clone();
                rewritten.book.supply.get_mut(&(base+1)).unwrap().accepting_block = [99;64];
                candidates.push(rewritten);
            } else {
                let mut invented = accepted.clone();
                invented.book.supply.insert(base+1, crate::palw_court_service_v1::ServiceSupply {
                    accepting_block:[99;64], reserved:cost,
                });
                candidates.push(invented);
            }
            for changed in candidates {
                changed.check_context(&b.state).expect("counterexample is independently shape/context-valid");
                assert!(b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(changed).into()))).is_err());
                assert_eq!(b.state, protected);
                assert_eq!(b.entries.len(), journal, "refusal precedes canonical state/journal writes");
            }
            assert_eq!(b.state, protected);
            // A monotone accounting observation still works. This fixture
            // update is not an authenticated production block-supply adapter.
            let mut advanced = accepted.clone();
            advanced.book.observe_block(base+3, [13;64], Default::default()).unwrap();
            b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(advanced).into()))).unwrap();
            let protected = b.state.clone();
            let err = apply_verified_control_v1(&mut b, &ctx, &frame, &control).unwrap_err();
            let message = format!("{err:?}");
            assert!(message.contains(if supplied { "inclusion assumptions" } else { "supply is missing" }), "{message}");
            assert_eq!(b.state, protected, "a denied default cannot debit, close, advance or replace a row");
            // This is the real native StagedCourtSeed journal (delta tag 200),
            // not a clone-only service-book rollback.
            let mut undone = protected;
            for entry in b.entries.iter().rev() { apply_delta_entry(&mut undone, entry, true).unwrap(); }
            assert_eq!(undone.state_root(), state.state_root());
            let mut replayed = undone;
            for entry in &b.entries { apply_delta_entry(&mut replayed, entry, false).unwrap(); }
            assert_eq!(replayed.state_root(), b.state.state_root());
        }
    }

    #[test]
    fn rooted_service_semantic_custody_guard_rejects_missing_short_and_substituted_contracts() {
        use crate::palw_court_service_state_v2::require_tir_service_custody_window_v1 as check;
        use crate::palw_staged_bundle_v1::{PalwStagedBundlePlanV1, PalwStagedFamilyV1, PalwStagedBundleContractV1};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let (mut state, params, extras, _, _) = demanded_world(&fx, &fx.frame, 0);
        let terms = fx.frame.plan_v1().terms;
        let semantic = PalwStagedBundlePlanV1::derive_v1(terms.accepted_daa,
            vec![(PalwStagedFamilyV1::Constraint, terms.plan)], fx.rules.config).unwrap();
        let resource = Hash64::from_u64_word(0x53e);
        // Explicit guard-only synthetic injection, NOT native semantic admission.
        let frame = fx.frame.clone().with_service_semantic_guard_fixture_v1(semantic.clone(), resource);
        let claim = frame.plan_v1().original_claim;
        let until = terms.accepted_daa + 10_000;
        assert!(check(&state, &frame, until).unwrap_err().contains("missing"));
        let producer = frame.custody_v1().pledge_v1().producer;
        let custody = crate::palw_prefix_custody_v1::PalwPrefixCustodyContractV1::new(
            frame.original_v1().execution_root, crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
            Hash64::from_u64_word(1), &semantic.terms);
        let pledge = crate::palw_history_custody_v1::PalwHistoryCustodyPledgeV1 {
            version: 1, claim, producer, execution_root: frame.original_v1().execution_root,
            job_identity: crate::palw_fp_execution_v3::palw_fp_job_pin_v1(frame.original_v1()),
            cu_ruleset_id: Hash64::from_u64_word(1), resource_plan_id: resource, maximum_records: 1,
            maximum_custodians: 4, accept_until_daa: terms.accepted_daa + 1, material_until_daa: until,
            signature: vec![2; SIG],
        };
        let bundle = PalwStagedBundleContractV1 {
            claim: crate::palw_staged_court_state_v1::PalwStagedClaimContractV1 {
                terms: semantic.terms, producer, panel: Default::default(), prefix_custody: Some(custody.clone()) },
            plan: semantic, config: fx.rules.config,
            history_custody: crate::palw_history_custody_v1::PalwHistoryCustodyContractV1 {
                pledge, producer, maximum_records: 1, maximum_custodians: 4, custody },
            original_binding: crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(frame.header_v1().clone()),
            producer_custodian: None,
        };
        for delta in [-1_i64, 0, 1] {
            let mut row = bundle.clone();
            row.history_custody.pledge.material_until_daa = (until as i64 + delta) as u64;
            state.staged_courts.insert(Key::Claim(claim), Row::ClaimBundle(Box::new(row).into()));
            assert_eq!(check(&state, &frame, until).is_ok(), delta >= 0);
        }
        // A pledge reaching hard alone used to pass service-context validation.
        // The real original's material horizon and native hard+reorg floor must
        // both be covered. These remain guard-only synthetic semantic rows, not
        // a claim that the native semantic admission/signatures were exercised.
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding,
            tir_service_required_material_until_v1};
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
        let required = tir_service_required_material_until_v1(&frame, until).unwrap();
        assert!(required > until, "fixture exposes the missing post-hard material obligation");
        assert!(frame.custody_v1().pledge_v1().material_until_daa >= required,
            "test uses the existing TIR promise without extending it");
        assert!(tir_service_required_material_until_v1(&frame, until - 1).is_err());
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket { id: [0x53; 64], release_daa: terms.accepted_daa + 1,
            service_before: terms.accepted_daa + 2, cost }], until).unwrap();
        let profile = h(0x53f);
        let service = CourtServiceStateV2 { version: 2, source_profile_id: profile,
            profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(profile), book,
            claims: BTreeMap::from([(claim, ServiceClaimBinding { claim, allocation_id: frame.plan_v1().allocation_id,
                source_id: frame.plan_v1().source_id, original_accepted_daa: terms.accepted_daa,
                hard_deadline_daa: until, packet_ids: BTreeSet::from([[0x53; 64]]) })]) };
        state.staged_courts.insert(Key::TirClaim(claim), Row::TirClaim(Box::new(frame.clone()).into()));
        for horizon in [until, required - 1, required, required + 1] {
            let mut row = bundle.clone();
            row.history_custody.pledge.material_until_daa = horizon;
            state.staged_courts.insert(Key::Claim(claim), Row::ClaimBundle(Box::new(row).into()));
            assert!(check(&state, &frame, until).is_ok(), "old hard-only constraint accepts this promise");
            assert_eq!(service.check_context(&state).is_ok(), horizon >= required,
                "service must cover actual retained original material: {horizon} vs {required}");
        }
        // A shape-valid semantic contract still uses the guard-only original
        // fixture above (not native original admission or a signature test).
        let mut retained = bundle.clone();
        let cu = frame.header_v1().job_context.cu_ruleset_id;
        retained.history_custody.pledge.cu_ruleset_id = cu;
        retained.history_custody.custody.cu_ruleset_id = cu;
        retained.claim.prefix_custody.as_mut().unwrap().cu_ruleset_id = cu;
        retained.history_custody.pledge.material_until_daa = required.max(retained.claim.terms.material_until_daa);
        let dependency = Key::Claim(claim);
        let row = Row::ClaimBundle(Box::new(retained.clone()));
        row.check(&dependency).unwrap();
        state.staged_courts.insert(dependency, row.clone());
        let owner_key = Key::Owner(claim, outsider());
        let owner_row = Row::OwnerBundle(Box::new(crate::palw_staged_bundle_v1::PalwOwnedStagedBundleV1 {
            claim, owner: outsider(), reserved_daa: terms.accepted_daa + 1,
            execution_root: frame.original_v1().execution_root, plan: retained.plan.clone(), inventory: None,
            selected: None, selected_context: None, selected_native: None, flat: None, flat_context: None,
            float: None, float_context: None, history_recovery: Default::default(), active: None, opened: Vec::new(),
        }).into());
        owner_row.check(&owner_key).unwrap();
        state.staged_courts.insert(owner_key, owner_row.clone());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let ttl = retained.claim.terms.bond_exit_after_daa.max(terms.material_until_daa) + 1;
        let point = at(&frame, ttl - terms.accepted_daa);
        let mut old = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        super::super::palw_staged_court_fold_v1::sweep(&mut old, &point).unwrap();
        assert!(old.state.staged_court_row_v1(&dependency).is_none(), "old claim keeps finite TTL");
        assert!(old.state.staged_court_row_v1(&owner_key).is_none(), "old owner keeps finite TTL");
        let mut held = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        held.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(service.clone()).into()))).unwrap();
        let root = held.state.state_root(); let entries = held.entries.len();
        assert!(held.write_staged_row_v1(dependency, None).is_err());
        let mut short = retained.clone(); short.history_custody.pledge.material_until_daa = required - 1;
        assert!(held.write_staged_row_v1(dependency, Some(Row::ClaimBundle(Box::new(short)))).is_err());
        assert!(held.write_staged_row_v1(dependency, Some(Row::Claim(retained.claim.clone()))).is_err());
        let mut longer = retained.clone(); longer.history_custody.pledge.material_until_daa += 1;
        assert!(held.write_staged_row_v1(dependency, Some(Row::ClaimBundle(Box::new(longer)))).is_err(),
            "even a longer replacement promise is not the accepted signed pledge");
        let mut rebound = retained.clone();
        let crate::palw_public_binding_v1::PalwPublicExecutionBindingHeaderV1::Tir(header) = &mut rebound.original_binding else { unreachable!() };
        header.step_merkle_root = Hash64::default();
        assert!(held.write_staged_row_v1(dependency, Some(Row::ClaimBundle(Box::new(rebound)))).is_err());
        let mut foreign = retained.clone(); foreign.history_custody.pledge.resource_plan_id = Hash64::default();
        assert!(held.write_staged_row_v1(dependency, Some(Row::ClaimBundle(Box::new(foreign)))).is_err());
        assert_eq!(held.state.state_root(), root); assert_eq!(held.entries.len(), entries);
        super::super::palw_staged_court_fold_v1::sweep(&mut held, &point).unwrap();
        assert_eq!(held.state.staged_court_row_v1(&dependency), Some(&row));
        assert_eq!(held.state.staged_court_row_v1(&owner_key), Some(&owner_row));
        held.state.court_service_state_v1().unwrap().check_context(&held.state).unwrap();
        // The actual native panel callback may still update retained membership;
        // blanket immutability of the dependency would break this legitimate path.
        let panel = held.state.panel(&claim).unwrap().clone();
        held.record_staged_panel_v1(claim, &panel).unwrap();
        assert_eq!(held.state.staged_bundle_contract_v1(claim).unwrap().claim.panel,
            panel.seats.iter().map(|s| s.bond).collect());
        held.state.staged_court_row_v1(&dependency).unwrap().check(&dependency).unwrap();
        assert!(held.state.court_service_holds_material_v1(&owner_key));
        // Owner material is retained, not a blanket ban on its normal updates.
        held.write_staged_row_v1(owner_key, Some(owner_row.clone())).unwrap();
        let live = held.state.claim(&claim).unwrap().clone();
        let silent = panel.seats[0].bond;
        for unpaid in [BTreeSet::from([producer, silent]), BTreeSet::from([silent])] {
            let root = held.state.state_root(); let entries = held.entries.len();
            assert!(held.staged_selected_default_charge_v1(&point, claim, &live, producer, &unpaid, outsider()).is_err());
            assert_eq!(held.state.state_root(), root); assert_eq!(held.entries.len(), entries);
        }
        held.state.court_service_state_v1().unwrap().check_context(&held.state).unwrap();
        for field in 0..5 {
            let mut row = bundle.clone();
            let p = &mut row.history_custody.pledge;
            match field {
                0 => p.claim = Hash64::default(),
                1 => p.resource_plan_id = Hash64::default(),
                2 => p.execution_root = Hash64::default(),
                3 => p.job_identity = Hash64::default(),
                _ => p.producer = PalwBondKeyV2(crate::tx::TransactionOutpoint::new(Hash64::default(), 99)),
            }
            state.staged_courts.insert(Key::Claim(claim), Row::ClaimBundle(Box::new(row).into()));
            assert!(check(&state, &frame, until).unwrap_err().contains("scope"));
        }
    }

    /// Synthetic verified-half fixture: native sweep/index/journal behavior,
    /// not signed court admission, supplied block capacity or default authority.
    // Verified-half archive fixtures: original admission, carrier signatures,
    // service costs and declaration funding remain synthetic, not node E02.
    fn archive_service(b: &mut TransitionBuilder<'_>, frame: &Frame) {
        use crate::palw_court_service_v1::{ServiceCost,ServicePacket,ServiceReservations};
        use crate::palw_court_service_state_v2::{CourtServiceStateV2,ServiceClaimBinding,court_service_profile_id_v2};
        let p=frame.plan_v1();let cost=ServiceCost {bytes:1,work:1,io:1,controls:1};
        let mut book=ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket {id:[61;64],release_daa:p.terms.accepted_daa+1,
            service_before:p.terms.accepted_daa+2,cost}],p.terms.accepted_daa+10_000).unwrap();
        let service=CourtServiceStateV2 {version:2,source_profile_id:h(62),profile_id:court_service_profile_id_v2(h(62)),book,
            claims:BTreeMap::from([(p.original_claim,ServiceClaimBinding {claim:p.original_claim,allocation_id:p.allocation_id,
                source_id:p.source_id,original_accepted_daa:p.terms.accepted_daa,hard_deadline_daa:p.terms.accepted_daa+10_000,
                packet_ids:std::collections::BTreeSet::from([[61;64]])})])};
        b.write_staged_row_v1(Key::CourtService,Some(Row::CourtService(Box::new(service).into()))).unwrap();
    }
    fn archive_session(b: &mut TransitionBuilder<'_>, frame: &Frame) -> (Hash64,PalwCourtSessionStateV2) {
        use crate::palw_bisect::{PalwBisectLadderV1,PalwBisectSpaceV1};
        let p=frame.plan_v1();let claim=b.state.claim(&p.original_claim).unwrap();let opened=p.terms.accepted_daa+3;
        let ladder=PalwBisectLadderV1::open_at_named_leaf(&p.original_claim,&claim.trace_root,
            &crate::palw_court_v2::court_party_id_v2(&outsider()),&crate::palw_court_v2::court_party_id_v2(&claim.bond),
            PalwBisectSpaceV1::StepLeaves,frame.header_v1().step_leaf_count,FORGED_LEAF as u64,opened,opened+1).unwrap();
        let id=ladder.session_id();let session=PalwCourtSessionStateV2 {claim:p.original_claim,challenger_bond:outsider(),
            opened_daa:opened,deadline_daa:opened+100,ladder,dissection:None};
        b.write_court(id,Some(session.clone())).unwrap();
        for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {
            let declarer=if side==PalwCourtSideV1::Executor {frame.custody_v1().pledge_v1().producer}else{outsider()};
            let first=vec![7;16];let last=vec![8;16];let mut all=first.clone();all.extend_from_slice(&last);
            let declared=opened+1;
            let group=PalwCourtCloseGroupV2 {declarer,count:2,chunk_digests:vec![palw_court_close_chunk_digest_v1(&first),palw_court_close_chunk_digest_v1(&last)],
                close_digest:palw_court_close_chunk_digest_v1(&all),verdict:PalwCourtVerdictV2::ExecutorGuilty,declared_daa:declared,
                assembly_deadline_daa:declared+palw_close_assembly_daa_v1(2),present:1,deposit_outpoint:declarer.0,
                deposit:palw_close_assembly_deposit_v1(2),chunks:BTreeMap::from([(0,first)])};
            b.write_court_close_group((id,side),Some(group)).unwrap();
        }
        (id,session)
    }
    fn archive_roundtrip(b: &mut TransitionBuilder<'_>, params: &PalwStateParamsV2, frame: &Frame, id: Hash64, point: PalwBlockContextV2, parent: Hash64) {
        use crate::palw_native_resource_v1::{NativeHeapV1,NativeWireScanV1,NativeWireReaderV1};
        let old=b.state.last_point;b.state.last_point=Some(point);
        b.entries.push(PalwDeltaEntryV2::LastPoint {old,new:Some(point)});
        let archive=b.state.court_end_archive_v1(id).unwrap().clone();archive.check_loaded_context(&b.state).unwrap();
        let row=b.state.staged_court_row_v1(&Key::CourtEndArchive(id)).unwrap();let bytes=borsh::to_vec(row).unwrap();
        assert_eq!(bytes[0],26);assert_eq!(borsh::to_vec(&Key::CourtEndArchive(id)).unwrap()[0],20);
        let mut reader=NativeWireReaderV1::new(bytes.as_slice(),bytes.len() as u64);
        let shape=Row::scan_native_wire_v1(&mut reader).unwrap();assert_eq!(reader.remaining(),0);
        assert!(shape.heap>=row.native_heap_v1("archive test").bytes().unwrap());
        let restored:Row=borsh::from_slice(&bytes).unwrap();assert_eq!(&restored,row);
        let mut short=NativeWireReaderV1::new(&bytes[..bytes.len()-1],(bytes.len()-1) as u64);
        assert!(Row::scan_native_wire_v1(&mut short).is_err());
        let before=b.state.clone();let journal=b.entries.len();
        assert!(b.write_staged_row_v1(Key::CourtEndArchive(id),None).is_err());
        let mut changed=archive.clone();changed.groups.clear();
        assert!(b.write_staged_row_v1(Key::CourtEndArchive(id),Some(Row::CourtEndArchive(Box::new(changed).into()))).is_err());
        assert_eq!(b.state,before);assert_eq!(b.entries.len(),journal);
        let past=at(frame,archive.material_until_daa-frame.plan_v1().terms.accepted_daa+1);
        super::super::palw_staged_court_fold_v1::sweep(b,&past).unwrap();
        assert!(b.state.court_end_archive_v1(id).is_some());
        let old=b.state.last_point;b.state.last_point=Some(past);
        b.entries.push(PalwDeltaEntryV2::LastPoint {old,new:Some(past)});
        let saved=b.state.clone();let wire=borsh::to_vec(&PalwStateCarriageV2::from_state(&saved)).unwrap();
        let fresh=borsh::from_slice::<PalwStateCarriageV2>(&wire).unwrap().into_state(params,Some(saved.state_root())).unwrap();
        assert_eq!(fresh.state_root(),saved.state_root());assert_eq!(fresh.court_end_archive_v1(id),saved.court_end_archive_v1(id));
        let mut backwards=saved.clone();for e in b.entries.iter().rev(){apply_delta_entry(&mut backwards,e,true).unwrap();}
        assert!(backwards.court_end_archive_v1(id).is_none());
        assert_eq!(backwards.state_root(),parent,"undo returns to the actual native parent root");
        for e in &b.entries{apply_delta_entry(&mut backwards,e,false).unwrap();}
        assert_eq!(backwards.state_root(),saved.state_root());
    }
    #[test]
    fn rooted_court_end_archive_native_verdict_preserves_other_side() {
        let fx=palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF,1)),4);let frame=fx.frame.clone();
        let (state,params,extras)=world(&fx,&frame);let params=params.with_tir_from_daa(Some(0));
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        archive_service(&mut b,&frame);let (id,session)=archive_session(&mut b,&frame);
        let Evidence::Arithmetic {proof}=cone(&fx) else {unreachable!()};
        let court=crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap();
        assert_eq!(crate::palw_court_v2::adjudicate_court_close_v2(&b.state,&id,&proof,&court,frame.header_v1().step_leaf_count,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).unwrap(),PalwCourtVerdictV2::ExecutorGuilty);
        let object=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict:PalwCourtVerdictV2::ExecutorGuilty,proof:*proof};
        let before=b.state.bond(&outsider()).unwrap().collateral;let point=at(&frame,5);
        super::super::objects::apply_object(&mut b,&point,&object).unwrap();
        assert!(b.state.court_session(&id).is_none());let a=b.state.court_end_archive_v1(id).unwrap();
        assert_eq!(a.session,session);assert_eq!(a.groups.len(),2);assert!(a.applied_deposit_debits.is_empty());
        assert!(matches!(a.evidence.as_ref(),crate::palw_court_end_archive_v1::CourtEndEvidenceV1::Native {object:o,point:p} if **o==object && *p==point));
        assert_eq!(b.state.bond(&outsider()).unwrap().collateral,before);
        assert_eq!(b.state.court_archive_pending_exposure_v1(&outsider()),u128::from(palw_close_assembly_deposit_v1(2)));
        assert_eq!(palw_accuser_exposure_v1(&b.state,&outsider()),u128::from(palw_close_assembly_deposit_v1(2)));
        assert_eq!(a.adjudication(),crate::palw_court_end_archive_v1::CourtEndDispositionV1::NativeCourtVerdict(PalwCourtVerdictV2::ExecutorGuilty));
        assert!(b.state.staged_bond_exit_held_v1(&outsider(),frame.plan_v1().terms.material_until_daa+1));
        let saved=b.state.clone();assert!(b.write_court(id,Some(session)).is_err());assert_eq!(b.state,saved);
        archive_roundtrip(&mut b,&params,&frame,id,point,state.state_root());
    }
    #[test]
    fn rooted_court_end_archive_complete_malformed_side_is_settled_once() {
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=fx.frame.clone();
        let (state,params,extras)=world(&fx,&frame);let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        archive_service(&mut b,&frame);let (id,_)=archive_session(&mut b,&frame);
        let producer=frame.custody_v1().pledge_v1().producer;let collateral=b.state.bond(&producer).unwrap().collateral;
        let own_before=b.state.bond(&outsider()).unwrap().collateral;let side=PalwCourtSideV1::Challenger;
        let point=at(&frame,5);let deposit=palw_close_assembly_deposit_v1(2);
        let object=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:vec![8;16]};
        super::super::objects::apply_object(&mut b,&point,&object).unwrap();
        let a=b.state.court_end_archive_v1(id).unwrap();assert_eq!(a.applied_deposit_debits.get(&side),Some(&deposit.min(own_before)));
        assert!(a.groups[&side].is_complete());assert_eq!(a.pending_nominal(&outsider()),0);
        assert_eq!(a.pending_nominal(&producer),u128::from(deposit));
        assert_eq!(b.state.bond(&producer).unwrap().collateral,collateral);
        let saved=b.state.clone();assert!(super::super::objects::apply_object(&mut b,&point,&object).is_err());assert_eq!(b.state,saved);
        archive_roundtrip(&mut b,&params,&frame,id,point,state.state_root());
    }
    #[test]
    fn rooted_court_end_archive_fulfilled_close_keeps_raw_material_without_pending_charge() {
        let fx=palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF,1)),4);let frame=fx.frame.clone();
        let (state,params,extras)=world(&fx,&frame);let params=params.with_tir_from_daa(Some(0));
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        archive_service(&mut b,&frame);let (id,_)=archive_session(&mut b,&frame);
        let Evidence::Arithmetic {proof}=cone(&fx) else {unreachable!()};
        let court=crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap();
        let verdict=crate::palw_court_v2::adjudicate_court_close_v2(&b.state,&id,&proof,&court,frame.header_v1().step_leaf_count,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat).unwrap();
        let close=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict,proof:*proof};let bytes=borsh::to_vec(&close).unwrap();
        assert_eq!(palw_court_close_object_digest_v1(&close).unwrap(),palw_court_close_chunk_digest_v1(&bytes));
        let split=bytes.len()/2;assert!(split>0 && bytes.len()-split<=PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
        let side=PalwCourtSideV1::Challenger;
        let mut group=b.state.court_close_group(&id,side).unwrap().clone();
        group.chunk_digests=vec![palw_court_close_chunk_digest_v1(&bytes[..split]),palw_court_close_chunk_digest_v1(&bytes[split..])];
        group.close_digest=palw_court_close_chunk_digest_v1(&bytes);group.chunks=BTreeMap::from([(0,bytes[..split].to_vec())]);
        b.write_court_close_group((id,side),Some(group)).unwrap();
        let point=at(&frame,5);let object=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:bytes[split..].to_vec()};
        super::super::objects::apply_object(&mut b,&point,&object).unwrap();
        let a=b.state.court_end_archive_v1(id).unwrap();assert!(a.fulfilled_groups.contains(&side));
        assert!(a.applied_deposit_debits.is_empty());assert_eq!(a.pending_nominal(&outsider()),0);
        assert_eq!(a.groups[&side].chunks.values().flatten().copied().collect::<Vec<_>>(),bytes);
        assert!(matches!(a.evidence.as_ref(),crate::palw_court_end_archive_v1::CourtEndEvidenceV1::Native {object,..} if **object==close));
        archive_roundtrip(&mut b,&params,&frame,id,point,state.state_root());
    }
    #[test]
    fn rooted_court_end_archive_uncaptured_removal_retains_nominal_court_stake() {
        let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=fx.frame.clone();let (state,params,extras)=world(&fx,&frame);
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
        archive_service(&mut b,&frame);let (id,_)=archive_session(&mut b,&frame);
        let stake=b.state.claim(&frame.plan_v1().original_claim).unwrap().reserved;let before=b.state.bond(&outsider()).unwrap().collateral;
        b.write_court(id,None).unwrap();let a=b.state.court_end_archive_v1(id).unwrap();
        assert_eq!(a.adjudication(),crate::palw_court_end_archive_v1::CourtEndDispositionV1::Uncaptured);
        assert_eq!(a.court_stake_nominal,stake);
        // A cause receipt alone cannot resolve THIS court's stake. This is a
        // read-only projection counterexample, not an accepted native object.
        let mut related=a.clone();
        related.evidence=Box::new(crate::palw_court_end_archive_v1::CourtEndEvidenceV1::Native {
            point:at(&frame,5),object:Box::new(PalwConsensusObjectV2::CourtCloseChunk {
                session_id:id,side:PalwCourtSideV1::Challenger,index:0,bytes:vec![7;16],
            }),
        }).into();
        assert_eq!(related.adjudication(),crate::palw_court_end_archive_v1::CourtEndDispositionV1::RelatedNativeTransition);
        assert_eq!(related.pending_nominal(&outsider()),a.pending_nominal(&outsider()));
        assert_eq!(palw_accuser_exposure_v1(&b.state,&outsider()),stake+u128::from(palw_close_assembly_deposit_v1(2)));
        assert_eq!(b.state.bond(&outsider()).unwrap().collateral,before);
        archive_roundtrip(&mut b,&params,&frame,id,at(&frame,5),state.state_root());
    }
    /// Real declaration signatures and native fold, still synthetic original
    /// funding/service inputs. No v5 proof verdict or node ingress is claimed.
    #[test]
    fn rooted_court_close_receipt_preserves_signed_native_inputs() {
        use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
        use crate::palw_court_v2::{palw_court_close_declaration_message_v1 as message,
            PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT as DOMAIN};
        use crate::palw_native_resource_v1::{NativeHeapV1,NativeWireScanV1,NativeWireReaderV1};
        for retain_rules in [false,true] {
            let fx=palw_tir_public_frame_fixture_v1(None,4);let frame=fx.frame.clone();
            let (mut state,params,mut extras)=world(&fx,&frame);
            let producer=frame.custody_v1().pledge_v1().producer;
            let keys=[libcrux_ml_dsa::ml_dsa_87::generate_key_pair([71;32]),libcrux_ml_dsa::ml_dsa_87::generate_key_pair([72;32])];
            for (who,key) in [producer,outsider()].into_iter().zip(&keys) {state.bonds.get_mut(&who).unwrap().pubkey=key.verification_key.as_ref().to_vec();}
            let point=at(&frame,5);
            if retain_rules {extras.native_court_rules_v1=Some(NativeCourtRulesV1 {
                network_domain:h(0x771),at:point,court:crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap(),
                step_ladder:frame.header_v1().step_leaf_count,prompt_ids_merkle:false,held_regime:false,decode_close_convicts_only:false,
                fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,
                tir_active:false,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false,
            });}
            let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
            archive_service(&mut b,&frame);let (id,_)=archive_session(&mut b,&frame);
            // Remove only synthetic fixture groups, then declare through the arm.
            for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {b.write_court_close_group((id,side),None).unwrap();}
            let bytes=vec![0x99;16];let digest=palw_court_close_chunk_digest_v1(&bytes);
            for (i,side) in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger].into_iter().enumerate() {
                let verdict=PalwCourtVerdictV2::ExecutorGuilty;
                let sig=libcrux_ml_dsa::ml_dsa_87::sign(&keys[i].signing_key,&message(&id,side,1,&[digest],&digest,verdict),DOMAIN,[73;32]).unwrap().as_ref().to_vec();
                let object=PalwConsensusObjectV2::CourtCloseDeclared {session_id:id,side,count:1,chunk_digests:vec![digest],close_digest:digest,verdict,signature:sig};
                crate::palw_court_v2::check_close_declared_chunk_count_v2(&id,1,&crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap()).unwrap();
                if let PalwConsensusObjectV2::CourtCloseDeclared {signature,..}=&object {
                    crate::palw_court_v2::check_court_close_declaration_acceptance_v2(&b.state,&id,side,1,&[digest],&digest,verdict,signature,
                        crate::palw_attempt_material_v3::verify_ready_signature_v3).unwrap();
                }
                let mut bad=object.clone();if let PalwConsensusObjectV2::CourtCloseDeclared {signature,..}=&mut bad {signature[0]^=1;}
                let root=b.state.state_root();let n=b.entries.len();
                assert!(super::super::objects::apply_object(&mut b,&point,&bad).is_err());
                assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),n);
                super::super::objects::apply_object(&mut b,&point,&object).unwrap();
                let key=Key::CourtCloseReceipt(id,side);let row=b.state.staged_court_row_v1(&key).unwrap().clone();
                let Row::CourtCloseReceipt(receipt)=&row else {panic!()};
                assert_eq!(receipt.declaration.as_ref(),&object);assert_eq!(receipt.rules,extras.native_court_rules_v1);
                assert_eq!(receipt.declarer_pubkey,keys[i].verification_key.as_ref());receipt.check_context(&b.state).unwrap();
                let wire=borsh::to_vec(&row).unwrap();assert_eq!(wire[0],27);assert_eq!(borsh::to_vec(&key).unwrap()[0],21);
                let mut reader=NativeWireReaderV1::new(std::io::Cursor::new(&wire),wire.len() as u64);
                let shape=Row::scan_native_wire_v1(&mut reader).unwrap();assert_eq!(reader.remaining(),0);
                assert!(shape.heap>=row.native_heap_v1("receipt").bytes().unwrap());
                assert_eq!(borsh::from_slice::<Row>(&wire).unwrap(),row);
                assert!(b.write_staged_row_v1(key,None).is_err());
                let mut modified=receipt.as_ref().clone();modified.rules=None;
                if retain_rules {assert!(b.write_staged_row_v1(key,Some(Row::CourtCloseReceipt(Box::new(modified).into()))).is_err());}
                // Even a genuine old signed declaration is not a fresh event.
                let mut absent=b.state.clone();absent.staged_courts.remove(&key);
                let mut forged=TransitionBuilder::new(&absent,&params,false,false,false,false,&extras);
                let root=forged.state.state_root();assert!(forged.write_staged_row_v1(key,Some(row)).is_err());assert_eq!(forged.state.state_root(),root);
            }
            super::super::objects::apply_object(&mut b,&at(&frame,6),&PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side:PalwCourtSideV1::Executor,index:0,bytes}).unwrap();
            for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {
                let Row::CourtCloseReceipt(r)=b.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)).unwrap() else {panic!()};r.check_context(&b.state).unwrap();
            }
            archive_roundtrip(&mut b,&params,&frame,id,at(&frame,6),state.state_root());
            for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {assert!(b.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)).is_some());}
        }
    }

    /// Actual v5 results for a tiny honest/forged cone and real declaration
    /// signatures. Original/service/capital remain verified-half fixtures.
    #[test]
    fn rooted_archive_v5_replays_signed_bytes_without_state_or_money_changes() {
        // Exercise real wire CPU/arithmetic errors, not a caller permission flag.
        use crate::palw_native_resource_v1::{NativeWireReaderV1,NativeWireShapeV1,NativeWireScanV1};
        use super::super::palw_court_archive_verify_v1::inspection_io_error;
        let overflow=NativeWireShapeV1::default().allocation(u64::MAX,16).unwrap_err();
        assert!(matches!(inspection_io_error("actual wire overflow",overflow),PalwStateV2Error::NativeResource(_)));
        let mut reader=NativeWireReaderV1::new(&[][..],0);
        let timeout=loop {if let Err(e)=reader.step(){break e;}};
        assert!(matches!(inspection_io_error("actual wire CPU bound",timeout),PalwStateV2Error::NativeResource(_)));
        let mut reader=NativeWireReaderV1::new(&[][..],0);
        let truncated=PalwConsensusObjectV2::scan_native_wire_v1(&mut reader).unwrap_err();
        assert!(matches!(inspection_io_error("truncated proof",truncated),PalwStateV2Error::CarriageInconsistent(_)));
        use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
        use crate::palw_court_v2::{palw_court_close_declaration_message_v1 as message,
            PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT as DOMAIN};
        for forged in [false,true] {for held in [false,true] {for wrong_verdict in [false,true] {
            let fx=palw_tir_public_frame_fixture_v1(if forged {Some((FORGED_LEAF,1))}else{None},4);let frame=fx.frame.clone();
            let (mut state,params,mut extras)=world(&fx,&frame);let params=params.with_tir_from_daa(Some(0));
            let keys=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([81;32]);state.bonds.get_mut(&outsider()).unwrap().pubkey=keys.verification_key.as_ref().to_vec();
            let rules=NativeCourtRulesV1 {network_domain:h(0x881),at:at(&frame,5),
                court:crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap(),
                step_ladder:frame.header_v1().step_leaf_count,prompt_ids_merkle:false,held_regime:held,decode_close_convicts_only:true,
                fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,
                tir_active:true,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false};
            extras.native_court_rules_v1=Some(rules);extras.offence_attribution_active=true;
            let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);
            archive_service(&mut b,&frame);let (id,_)=archive_session(&mut b,&frame);
            for side in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {b.write_court_close_group((id,side),None).unwrap();}
            let Evidence::Arithmetic {proof}=cone(&fx) else {unreachable!()};
            let actual=crate::palw_court_v2::adjudicate_court_close_v5(&b.state,&id,&proof,&rules.court,rules.step_ladder,
                crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,held,true,None,None,None).unwrap();
            assert_eq!(actual,if forged {PalwCourtVerdictV2::ExecutorGuilty}else{PalwCourtVerdictV2::ChallengerDefeated});
            let session=b.state.court_session(&id).unwrap();
            let judge_borrowed=|sid:&Hash64,court:&crate::palw_mode_v2::PalwCourtParamsV2| {
                crate::palw_court_v2::adjudicate_court_close_borrowed_v5(&b.state,sid,session,b.state.tir_dissection_v1(&id),
                    &proof,court,rules.step_ladder,crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,held,true,None,None,None)
            };
            assert_eq!(judge_borrowed(&id,&rules.court).unwrap(),actual);
            let other=h(0x882);assert_ne!(other,id);
            assert!(matches!(judge_borrowed(&other,&rules.court),Err(crate::palw_court_v2::PalwCourtV2Error::BindingInvalid(_))));
            let small=crate::palw_mode_v2::PalwCourtParamsV2::with_cost_ceilings(rules.court.max_step_leaf_count(),
                rules.court.turn_deadline_daa(),rules.court.terminal_rounds(),1,rules.court.max_terminal_macs(),rules.court.max_operand_count()).unwrap();
            assert!(matches!(judge_borrowed(&other,&small),Err(crate::palw_court_v2::PalwCourtV2Error::CloseTooLarge {..})),
                "cost refusal precedes even borrowed scope/state inspection");
            let stated=if wrong_verdict {if forged {PalwCourtVerdictV2::ChallengerDefeated}else{PalwCourtVerdictV2::ExecutorGuilty}}else{actual};
            let correct=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict:actual,proof:*proof.clone()};
            let committed=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict:stated,proof:*proof};
            let raw=borsh::to_vec(&committed).unwrap();let split=raw.len()/2;assert!(split>0 && raw.len()-split<=PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
            let digests=vec![palw_court_close_chunk_digest_v1(&raw[..split]),palw_court_close_chunk_digest_v1(&raw[split..])];let digest=palw_court_close_chunk_digest_v1(&raw);
            let side=PalwCourtSideV1::Challenger;
            crate::palw_court_v2::check_close_declared_chunk_count_v2(&id,2,&rules.court).unwrap();
            let signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,&message(&id,side,2,&digests,&digest,stated),DOMAIN,[82;32]).unwrap().as_ref().to_vec();
            let declared=PalwConsensusObjectV2::CourtCloseDeclared {session_id:id,side,count:2,chunk_digests:digests,close_digest:digest,verdict:stated,signature};
            super::super::objects::apply_object(&mut b,&at(&frame,5),&declared).unwrap();
            super::super::objects::apply_object(&mut b,&at(&frame,6),&PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:0,bytes:raw[..split].to_vec()}).unwrap();
            super::super::objects::apply_object(&mut b,&at(&frame,7),&correct).unwrap();
            assert!(b.state.court_session(&id).is_none());
            // Inspect an internal native event pair, without opening public
            // archive ingress or declaring this material fulfilled. The actual
            // chunk dispatcher still refuses a terminated active session.
            {
                let root=b.state.state_root();let entries=b.entries.len();
                assert!(b.inspect_archived_native_chunk_v1().is_err(),"no current native event");
                let group=&b.state.court_end_archive_v1(id).unwrap().groups[&side];
                let deadline=group.assembly_deadline_daa;
                let chunk=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:raw[split..].to_vec()};
                for (daa,expected) in [(deadline-1,true),(deadline,true),(deadline+1,false)] {
                    let point=at(&frame,daa-frame.plan_v1().terms.accepted_daa);
                    assert!(super::super::objects::apply_object(&mut b,&point,&chunk).is_err(),"public archive intake stays closed");
                    b.prepare_native_court_end_cause_v1(&point,&chunk).unwrap();
                    let result=b.inspect_archived_native_chunk_v1();
                    assert_eq!(result.is_ok(),expected);
                    if let Ok(token)=result {
                        assert_eq!(token.point(),point);assert_eq!(token.assembly_deadline_daa(),deadline);
                        assert!(token.matches_state(&b.state));assert!(token.matches_chunk(id,side,1,&raw[split..]));
                        assert!(!token.matches_chunk(id,side,0,&raw[split..]));
                        let mut changed=b.state.clone();changed.last_point=Some(point);assert!(!token.matches_state(&changed));
                    }
                    b.court_end_evidence=None;
                    assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),entries);
                }
                b.prepare_native_court_end_cause_v1(&at(&frame,7),&correct).unwrap();
                assert!(b.inspect_archived_native_chunk_v1().is_err(),"another native object is no chunk receipt");
                b.court_end_evidence=None;
                assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),entries);
            }
            let mut candidate=b.state.court_end_archive_v1(id).unwrap().groups[&side].clone();
            candidate.chunks.insert(1,raw[split..].to_vec());candidate.present=PalwCourtCloseGroupV2::full_mask(2);
            let root=b.state.state_root();let journal=b.entries.len();
            let result=b.verify_archived_close_v1(id,side,&candidate);
            if wrong_verdict {assert!(result.unwrap_err().to_string().contains("v5 result differs"));}
            else {
                let token=result.unwrap();assert_eq!(token.verdict(),actual);assert_eq!(token.session_id(),id);assert_eq!(token.side(),side);
                assert!(token.matches_state(&b.state));assert_eq!(token.group(),&candidate);assert_eq!(token.close(),&committed);
                let mut changed=b.state.clone();changed.last_point=Some(at(&frame,8));assert!(!token.matches_state(&changed));
            }
            assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),journal);
            // RFC-0034 S1: actual native event + original active-prefix
            // provenance + the retained v5 verifier can discharge this one
            // declaration. These are private native fixtures, not public fee
            // admission or a delivery/throughput measurement.
            for edge in [0u64,1] {
                let mut trial=TransitionBuilder::new(&b.state,&params,false,false,false,false,&extras);
                let before=trial.state.clone();let before_root=before.state_root();
                let origin=before.court_archive_progress_v1(id,side).unwrap();
                assert_eq!(origin.initial_native_present,1);assert!(origin.native_chunks.is_empty());assert!(origin.completed.is_none());
                let deadline=before.court_end_archive_v1(id).unwrap().groups[&side].assembly_deadline_daa;
                let point=at(&frame,deadline-edge-frame.plan_v1().terms.accepted_daa);
                let chunk=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:raw[split..].to_vec()};
                assert!(super::super::objects::apply_object(&mut trial,&point,&chunk).is_err(),"public intake remains closed");
                trial.prepare_native_court_end_cause_v1(&point,&chunk).unwrap();
                let result=trial.apply_native_archived_chunk_v1();
                if wrong_verdict {
                    assert!(result.is_err());assert_eq!(trial.state.state_root(),before_root);assert!(trial.entries.is_empty());
                }else{
                    assert_eq!(result.unwrap(),true);
                    let progress=trial.state.court_archive_progress_v1(id,side).unwrap().clone();
                    assert_eq!(progress.native_chunks.get(&1),Some(&point));assert!(progress.completed.is_some());
                    assert_eq!(trial.state.court_archive_pending_exposure_v1(&outsider()),0);
                    assert_eq!(trial.state.bond(&outsider()),before.bond(&outsider()));
                    assert_eq!(trial.state.claim(&frame.plan_v1().original_claim),before.claim(&frame.plan_v1().original_claim));
                    let a=trial.state.court_end_archive_v1(id).unwrap();let old=before.court_end_archive_v1(id).unwrap();
                    assert_eq!(a.evidence,old.evidence);assert_eq!(a.fulfilled_groups,old.fulfilled_groups);
                    assert_eq!(a.applied_deposit_debits,old.applied_deposit_debits);assert_eq!(a.court_stake_nominal,old.court_stake_nominal);
                    assert!(!trial.state.court_archive_holds_bond_v1(&outsider()),"only this declaration's nominal hold is discharged");
                    let changed=trial.state.state_root();let entries=trial.entries.len();
                    assert!(trial.apply_native_archived_chunk_v1().is_err());assert_eq!(trial.state.state_root(),changed);assert_eq!(trial.entries.len(),entries);
                    assert!(trial.write_staged_row_v1(Key::CourtArchiveProgress(id,side),None).is_err());
                    let mut reverted=progress.clone();reverted.completed=None;
                    assert!(trial.write_staged_row_v1(Key::CourtArchiveProgress(id,side),Some(Row::CourtArchiveProgress(Box::new(reverted).into()))).is_err());
                    assert_eq!(trial.state.state_root(),changed);assert_eq!(trial.entries.len(),entries);
                    let old=trial.state.last_point;trial.state.last_point=Some(point);
                    trial.entries.push(PalwDeltaEntryV2::LastPoint {old,new:Some(point)});
                    progress.check_loaded_context(&trial.state).unwrap();
                    let mut malformed=progress.clone();
                    malformed.completed.as_mut().unwrap().close=Box::new(chunk.clone());
                    assert!(matches!(malformed.check_loaded_context(&trial.state),Err(PalwStateV2Error::CarriageInconsistent(_))),
                        "loaded semantic mismatch remains distinct from local construction capacity");
                    let saved=trial.state.clone();
                    let wire=borsh::to_vec(&PalwStateCarriageV2::from_state(&saved)).unwrap();
                    let fresh=borsh::from_slice::<PalwStateCarriageV2>(&wire).unwrap().into_state(&params,Some(saved.state_root())).unwrap();
                    assert_eq!(fresh.state_root(),saved.state_root());assert_eq!(fresh.court_archive_pending_exposure_v1(&outsider()),0);
                    let mut backwards=saved.clone();for e in trial.entries.iter().rev(){apply_delta_entry(&mut backwards,e,true).unwrap();}
                    assert_eq!(backwards.state_root(),before_root);assert_eq!(backwards.court_archive_pending_exposure_v1(&outsider()),before.court_archive_pending_exposure_v1(&outsider()));
                    for e in &trial.entries{apply_delta_entry(&mut backwards,e,false).unwrap();}
                    assert_eq!(backwards.state_root(),saved.state_root());
                }
                // A peer/caller timestamp never seeds missing provenance.
                let mut old=before.clone();old.staged_courts.remove(&Key::CourtArchiveProgress(id,side));
                let mut unproven=TransitionBuilder::new(&old,&params,false,false,false,false,&extras);
                let root=unproven.state.state_root();unproven.prepare_native_court_end_cause_v1(&point,&chunk).unwrap();
                assert!(unproven.apply_native_archived_chunk_v1().is_err());assert_eq!(unproven.state.state_root(),root);assert!(unproven.entries.is_empty());
                assert!(unproven.capture_archive_native_origins_v1(id).is_err(),"no retroactive native prefix from an existing archive");
                assert_eq!(unproven.state.state_root(),root);assert!(unproven.entries.is_empty());
            }
            for mutation in 0..4 {
                let mut bad=candidate.clone();
                match mutation {0=>{bad.chunks.get_mut(&0).unwrap()[0]^=1;},1=>{bad.deposit+=1;},2=>{bad.present^=1;},_=>{bad.chunks.remove(&1);}}
                assert!(b.verify_archived_close_v1(id,side,&bad).is_err());assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),journal);
            }
            // Explicit denial counterexamples, not accepted native mutations:
            // current extras are valid, but retained policy is missing/dormant.
            for missing in [false,true] {
                let mut bad_state=b.state.clone();let key=Key::CourtCloseReceipt(id,side);
                let Row::CourtCloseReceipt(row)=bad_state.staged_court_row_v1(&key).unwrap() else {panic!()};let mut row=row.as_ref().clone();
                if missing {row.rules=None;}else{row.rules.as_mut().unwrap().tir_active=false;}
                bad_state.staged_courts.insert(key,Row::CourtCloseReceipt(Box::new(row).into()));
                let mut refused=TransitionBuilder::new(&bad_state,&params,false,false,false,false,&extras);
                let before=refused.state.state_root();let err=refused.verify_archived_close_v1(id,side,&candidate).unwrap_err().to_string();
                assert!(err.contains(if missing {"inputs missing"}else{"family was dormant"}),"{err}");
                assert_eq!(refused.state.state_root(),before);assert!(refused.entries.is_empty());
            }
            archive_roundtrip(&mut b,&params,&frame,id,at(&frame,7),state.state_root());
            let root=b.state.state_root();let journal=b.entries.len();
            assert_eq!(b.verify_archived_close_v1(id,side,&candidate).is_ok(),!wrong_verdict);
            assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),journal);
            // After TTL, relay the signed missing material without changing any
            // original deadline, adjudication, balance or nominal responsibility.
            let before=b.state.court_end_archive_v1(id).unwrap().clone();
            let exposure=b.state.court_archive_pending_exposure_v1(&outsider());
            let bond=b.state.bond(&outsider()).unwrap().clone();
            let claim=b.state.claim(&frame.plan_v1().original_claim).unwrap().clone();
            for (index,bytes) in [(0,raw[..split].to_vec()),(2,raw[split..].to_vec()),(1,vec![]),(1,vec![0xAB])]{
                assert!(b.append_archived_close_chunk_v1(id,side,index,&bytes).is_err());
                assert_eq!(b.state.state_root(),root);assert_eq!(b.entries.len(),journal);
            }
            b.append_archived_close_chunk_v1(id,side,1,&raw[split..]).unwrap();
            let after=b.state.court_end_archive_v1(id).unwrap();
            assert_eq!(after.groups[&side],candidate);after.check_material_extension(&before).unwrap();
            assert!(!after.fulfilled_groups.contains(&side));assert!(!after.applied_deposit_debits.contains_key(&side));
            assert_eq!(b.state.court_archive_pending_exposure_v1(&outsider()),exposure);
            assert_eq!(b.state.bond(&outsider()),Some(&bond));assert_eq!(b.state.claim(&frame.plan_v1().original_claim),Some(&claim));
            let appended=b.state.state_root();let entries=b.entries.len();
            assert!(b.append_archived_close_chunk_v1(id,side,1,&raw[split..]).is_err());
            for mutate in 0..5 {
                let mut changed=b.state.court_end_archive_v1(id).unwrap().clone();
                match mutate {0=>changed.material_until_daa+=1,1=>{changed.fulfilled_groups.insert(side);},
                    2=>{changed.groups.get_mut(&side).unwrap().chunks.remove(&1);changed.groups.get_mut(&side).unwrap().present^=2;},
                    3=>changed.groups.get_mut(&side).unwrap().deposit+=1,_=>changed.court_stake_nominal+=1}
                assert!(b.write_staged_row_v1(Key::CourtEndArchive(id),Some(Row::CourtEndArchive(Box::new(changed).into()))).is_err());
                assert_eq!(b.state.state_root(),appended);assert_eq!(b.entries.len(),entries);
            }
            assert_eq!(b.verify_archived_close_v1(id,side,&candidate).is_ok(),!wrong_verdict);
            let tip=*b.state.last_point().unwrap();
            archive_roundtrip(&mut b,&params,&frame,id,tip,state.state_root());
        }}}
    }

    #[test]
    fn archive_s1_does_not_credit_an_unclocked_middle_chunk_or_release_the_other_side() {
        use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
        use crate::palw_court_v2::{palw_court_close_declaration_message_v1 as message,PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT as DOMAIN};
        let fx=palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF,1)),4);let frame=fx.frame.clone();
        let (mut state,params,mut extras)=world(&fx,&frame);let params=params.with_tir_from_daa(Some(0));
        let keys=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([83;32]);state.bonds.get_mut(&outsider()).unwrap().pubkey=keys.verification_key.as_ref().to_vec();
        extras.native_court_rules_v1=Some(NativeCourtRulesV1 {network_domain:h(0x881),at:at(&frame,5),
            court:crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap(),
            step_ladder:frame.header_v1().step_leaf_count,prompt_ids_merkle:false,held_regime:false,decode_close_convicts_only:true,
            fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,
            tir_active:true,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false});
        extras.offence_attribution_active=true;
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);archive_service(&mut b,&frame);
        let (id,_)=archive_session(&mut b,&frame);let side=PalwCourtSideV1::Challenger;
        b.write_court_close_group((id,side),None).unwrap();
        let Evidence::Arithmetic {proof}=cone(&fx) else {unreachable!()};
        let close=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict:PalwCourtVerdictV2::ExecutorGuilty,proof:*proof};
        let raw=borsh::to_vec(&close).unwrap();let cut=raw.len()/3;assert!(cut>0);
        let chunks=[&raw[..cut],&raw[cut..2*cut],&raw[2*cut..]];
        let digests=chunks.iter().map(|b|palw_court_close_chunk_digest_v1(b)).collect::<Vec<_>>();let digest=palw_court_close_chunk_digest_v1(&raw);
        let signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,&message(&id,side,3,&digests,&digest,PalwCourtVerdictV2::ExecutorGuilty),DOMAIN,[84;32]).unwrap().as_ref().to_vec();
        let declaration=PalwConsensusObjectV2::CourtCloseDeclared {session_id:id,side,count:3,chunk_digests:digests,close_digest:digest,verdict:PalwCourtVerdictV2::ExecutorGuilty,signature};
        super::super::objects::apply_object(&mut b,&at(&frame,5),&declaration).unwrap();
        super::super::objects::apply_object(&mut b,&at(&frame,6),&PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:0,bytes:chunks[0].to_vec()}).unwrap();
        super::super::objects::apply_object(&mut b,&at(&frame,7),&close).unwrap();
        let native_parent=b.state.clone();
        for middle_clocked in [false,true] {
            let mut trial=TransitionBuilder::new(&native_parent,&params,false,false,false,false,&extras);
            let producer=frame.custody_v1().pledge_v1().producer;
            let other=trial.state.court_end_archive_v1(id).unwrap().groups[&PalwCourtSideV1::Executor].clone();
            let other_exposure=trial.state.court_archive_pending_exposure_v1(&producer);
            let own=trial.state.court_archive_pending_exposure_v1(&outsider());
            let money=trial.state.bond(&outsider()).unwrap().clone();
            if middle_clocked {
                let object=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:chunks[1].to_vec()};
                trial.prepare_native_court_end_cause_v1(&at(&frame,8),&object).unwrap();
                assert_eq!(trial.apply_native_archived_chunk_v1().unwrap(),false);
            }else{trial.append_archived_close_chunk_v1(id,side,1,chunks[1]).unwrap();}
            let deadline=trial.state.court_end_archive_v1(id).unwrap().groups[&side].assembly_deadline_daa;
            let point=at(&frame,deadline-frame.plan_v1().terms.accepted_daa);
            let final_chunk=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:2,bytes:chunks[2].to_vec()};
            let saved=trial.state.clone();let entries=trial.entries.len();
            trial.prepare_native_court_end_cause_v1(&at(&frame,deadline+1-frame.plan_v1().terms.accepted_daa),&final_chunk).unwrap();
            assert!(trial.apply_native_archived_chunk_v1().is_err());assert_eq!(trial.state,saved);assert_eq!(trial.entries.len(),entries);
            trial.prepare_native_court_end_cause_v1(&point,&final_chunk).unwrap();
            assert_eq!(trial.apply_native_archived_chunk_v1().unwrap(),middle_clocked);
            assert_eq!(trial.state.court_archive_pending_exposure_v1(&outsider()),if middle_clocked {0}else{own});
            assert_eq!(trial.state.court_archive_pending_exposure_v1(&producer),other_exposure);
            assert_eq!(trial.state.court_end_archive_v1(id).unwrap().groups[&PalwCourtSideV1::Executor],other);
            assert_eq!(trial.state.bond(&outsider()),Some(&money));
            assert!(trial.state.court_archive_holds_bond_v1(&producer));
            if !middle_clocked {assert!(trial.state.court_archive_holds_bond_v1(&outsider()));}
            assert!(trial.state.court_end_archive_v1(id).unwrap().groups[&side].is_complete());
            if !middle_clocked {
                // Retention alone supplied no time proof. A new genuine native
                // event can now deliver that SAME middle chunk before/equal to
                // the old deadline; the current point is not backdated.
                let archive=trial.state.court_end_archive_v1(id).unwrap().clone();
                let before=trial.state.state_root();let entries=trial.entries.len();
                let reobserve=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:chunks[1].to_vec()};
                trial.prepare_native_court_end_cause_v1(&at(&frame,deadline+1-frame.plan_v1().terms.accepted_daa),&reobserve).unwrap();
                assert!(trial.apply_native_archived_chunk_v1().is_err());
                let forged=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:vec![0xAA]};
                trial.prepare_native_court_end_cause_v1(&point,&forged).unwrap();assert!(trial.apply_native_archived_chunk_v1().is_err());
                trial.prepare_native_court_end_cause_v1(&point,&final_chunk).unwrap();assert!(trial.apply_native_archived_chunk_v1().is_err(),"already qualified final chunk is not another delivery");
                assert_eq!(trial.state.state_root(),before);assert_eq!(trial.entries.len(),entries);
                trial.prepare_native_court_end_cause_v1(&point,&reobserve).unwrap();assert!(trial.apply_native_archived_chunk_v1().unwrap());
                let progress=trial.state.court_archive_progress_v1(id,side).unwrap();
                assert_eq!(progress.native_chunks.get(&1),Some(&point));assert!(progress.completed.is_some());
                assert_eq!(trial.state.court_end_archive_v1(id),Some(&archive),"same bytes and original cause stay exactly intact");
                assert_eq!(trial.state.court_archive_pending_exposure_v1(&outsider()),0);
                assert_eq!(trial.state.court_archive_pending_exposure_v1(&producer),other_exposure);
                assert_eq!(trial.state.bond(&outsider()),Some(&money));
                let after=trial.state.state_root();let entries=trial.entries.len();
                assert!(trial.apply_native_archived_chunk_v1().is_err());assert_eq!(trial.state.state_root(),after);assert_eq!(trial.entries.len(),entries);
            }
        }
    }

    /// G14 lane court-archive-terminal: RFC-0034 S1 discharge followed by
    /// pruning. Timely actual completion discharges exactly that side and
    /// makes the archive prunable; late material never discharges and the
    /// archive is never pruned; pruning is journaled so undo restores it.
    #[test]
    fn archive_s1_timely_fulfillment_discharges_and_prunes_only_when_all_resolved() {
        use crate::palw_court_close_receipt_v1::NativeCourtRulesV1;
        use crate::palw_court_v2::{palw_court_close_declaration_message_v1 as message,PALW_COURT_V2_MLDSA87_CLOSE_DECLARATION_CONTEXT as DOMAIN};
        let fx=palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF,1)),4);let frame=fx.frame.clone();
        let (mut state,params,mut extras)=world(&fx,&frame);let params=params.with_tir_from_daa(Some(0));
        let keys=libcrux_ml_dsa::ml_dsa_87::generate_key_pair([85;32]);state.bonds.get_mut(&outsider()).unwrap().pubkey=keys.verification_key.as_ref().to_vec();
        extras.native_court_rules_v1=Some(NativeCourtRulesV1 {network_domain:h(0x881),at:at(&frame,5),
            court:crate::palw_mode_v2::PalwCourtParamsV2::new(frame.header_v1().step_leaf_count,params.turn_deadline_daa(),2).unwrap(),
            step_ladder:frame.header_v1().step_leaf_count,prompt_ids_merkle:false,held_regime:false,decode_close_convicts_only:true,
            fp_decode_from_daa:None,cell_closure_from_daa:None,memory_routing_from_daa:None,
            tir_active:true,gen_active:false,task_heads_active:false,improvement_active:false,constraint_active:false});
        extras.offence_attribution_active=true;
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);archive_service(&mut b,&frame);
        let (id,_)=archive_session(&mut b,&frame);let side=PalwCourtSideV1::Challenger;
        for s in [PalwCourtSideV1::Executor,PalwCourtSideV1::Challenger] {b.write_court_close_group((id,s),None).unwrap();}
        let Evidence::Arithmetic {proof}=cone(&fx) else {unreachable!()};
        let close=PalwConsensusObjectV2::CourtClosed {session_id:id,verdict:PalwCourtVerdictV2::ExecutorGuilty,proof:*proof};
        let raw=borsh::to_vec(&close).unwrap();let split=raw.len()/2;assert!(split>0 && raw.len()-split<=PALW_COURT_CLOSE_CHUNK_MAX_BYTES);
        let digests=vec![palw_court_close_chunk_digest_v1(&raw[..split]),palw_court_close_chunk_digest_v1(&raw[split..])];
        let digest=palw_court_close_chunk_digest_v1(&raw);
        let signature=libcrux_ml_dsa::ml_dsa_87::sign(&keys.signing_key,&message(&id,side,2,&digests,&digest,PalwCourtVerdictV2::ExecutorGuilty),DOMAIN,[86;32]).unwrap().as_ref().to_vec();
        let declaration=PalwConsensusObjectV2::CourtCloseDeclared {session_id:id,side,count:2,chunk_digests:digests,close_digest:digest,verdict:PalwCourtVerdictV2::ExecutorGuilty,signature};
        super::super::objects::apply_object(&mut b,&at(&frame,5),&declaration).unwrap();
        super::super::objects::apply_object(&mut b,&at(&frame,6),&PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:0,bytes:raw[..split].to_vec()}).unwrap();
        super::super::objects::apply_object(&mut b,&at(&frame,7),&close).unwrap();
        assert!(b.state.court_session(&id).is_none());
        let parent=b.state.clone();
        assert!(!parent.court_archive_fully_resolved_v1(id),"pending declaration deposit is an unresolved liability");
        {
            let mut early=TransitionBuilder::new(&parent,&params,false,false,false,false,&extras);
            let before=early.state.clone();
            assert!(early.prune_resolved_court_archive_v1(id).is_err());
            assert_eq!(early.state,before);assert!(early.entries.is_empty());
        }
        let deadline=parent.court_end_archive_v1(id).unwrap().groups[&side].assembly_deadline_daa;
        let final_chunk=PalwConsensusObjectV2::CourtCloseChunk {session_id:id,side,index:1,bytes:raw[split..].to_vec()};
        // Late material: never discharges, never prunable (even if retained by a private append).
        {
            let mut late=TransitionBuilder::new(&parent,&params,false,false,false,false,&extras);let before=late.state.clone();
            late.prepare_native_court_end_cause_v1(&at(&frame,deadline+1-frame.plan_v1().terms.accepted_daa),&final_chunk).unwrap();
            assert!(late.apply_native_archived_chunk_v1().is_err());late.court_end_evidence=None;
            assert_eq!(late.state,before);
            late.append_archived_close_chunk_v1(id,side,1,&raw[split..]).unwrap();
            assert!(late.state.court_end_archive_v1(id).unwrap().groups[&side].is_complete());
            assert!(!late.state.court_archive_fully_resolved_v1(id));
            assert!(late.state.court_archive_holds_bond_v1(&outsider()));
            let root=late.state.state_root();let entries=late.entries.len();
            assert!(late.prune_resolved_court_archive_v1(id).is_err());
            assert_eq!(late.state.state_root(),root);assert_eq!(late.entries.len(),entries);
        }
        // Timely actual completion (at the original deadline).
        let mut t=TransitionBuilder::new(&parent,&params,false,false,false,false,&extras);
        let money=t.state.bond(&outsider()).unwrap().clone();
        t.prepare_native_court_end_cause_v1(&at(&frame,deadline-frame.plan_v1().terms.accepted_daa),&final_chunk).unwrap();
        assert!(t.apply_native_archived_chunk_v1().unwrap());t.court_end_evidence=None;
        assert_eq!(t.state.court_archive_pending_exposure_v1(&outsider()),0);
        assert!(!t.state.court_archive_holds_bond_v1(&outsider()),"exit hold released for the discharged side");
        assert_eq!(t.state.bond(&outsider()),Some(&money),"nominal deposit is never escrow: no money moves");
        assert!(t.state.court_archive_fully_resolved_v1(id));
        // Only the pruner may remove these rows.
        let discharged=t.state.clone();let entries=t.entries.len();
        assert!(t.write_staged_row_v1(Key::CourtEndArchive(id),None).is_err());
        assert!(t.write_staged_row_v1(Key::CourtCloseReceipt(id,side),None).is_err());
        assert!(t.write_staged_row_v1(Key::CourtArchiveProgress(id,side),None).is_err());
        assert_eq!(t.state,discharged);assert_eq!(t.entries.len(),entries);
        // A different session id is not pruned.
        assert!(t.prune_resolved_court_archive_v1(h(0x993)).is_err());assert_eq!(t.state,discharged);
        t.prune_resolved_court_archive_v1(id).unwrap();
        assert!(t.court_archive_prune.is_none());
        assert!(t.state.court_end_archive_v1(id).is_none());
        assert!(t.state.court_archive_progress_v1(id,side).is_none());
        assert!(t.state.staged_court_row_v1(&Key::CourtCloseReceipt(id,side)).is_none());
        assert_eq!(t.state.bond(&outsider()),Some(&money));
        assert_eq!(t.state.court_archive_pending_exposure_v1(&outsider()),0);
        assert!(t.prune_resolved_court_archive_v1(id).is_err(),"prune is once-only");
        // Reorg: undo restores the parent exactly, redo re-prunes.
        let pruned=t.state.state_root();
        let mut backwards=t.state.clone();for e in t.entries.iter().rev(){apply_delta_entry(&mut backwards,e,true).unwrap();}
        assert_eq!(backwards.state_root(),parent.state_root());
        assert_eq!(backwards.court_end_archive_v1(id),parent.court_end_archive_v1(id));
        assert_eq!(backwards.court_archive_pending_exposure_v1(&outsider()),parent.court_archive_pending_exposure_v1(&outsider()));
        for e in &t.entries{apply_delta_entry(&mut backwards,e,false).unwrap();}
        assert_eq!(backwards.state_root(),pruned);
    }

    #[test]
    fn rooted_court_end_archive_streamed_fraud_keeps_complete_evidence() {
        use crate::palw_tir_public_carriage_v1::{PalwTirPublicWitnessPartV1 as Part,palw_tir_public_witness_digest_v1};
        let honest=palw_tir_public_frame_fixture_v1(None,4);let fx=crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_lying_output_v1(4,h(0xBAD));
        let frame=fx.frame.clone();let (state,params,mut extras)=world(&fx,&frame);extras.offence_attribution_active=true;
        let evidence=Evidence::Output {rows_root:crate::palw_step_refute::tiled_logits_rows_root_v1(&honest.execution.binding.job_context,&honest.execution.rows).unwrap(),ids:honest.execution.generated.clone()};
        let encoded=borsh::to_vec(&evidence).unwrap();let split=encoded.len()/2;let digest=palw_tir_public_witness_digest_v1(&encoded);
        let mut b=TransitionBuilder::new(&state,&params,false,false,false,false,&extras);archive_service(&mut b,&frame);
        let (id,_)=archive_session(&mut b,&frame);let before=b.state.bond(&outsider()).unwrap().collateral;
        let p=frame.plan_v1();
        for (offset,end,at_offset) in [(0,split,5),(split,encoded.len(),6)] {
            let control=Control {version:1,claim:p.original_claim,allocation_id:p.allocation_id,source_id:p.source_id,
                target:T::ConvictionWitness,expected_turn:0,declared_rent_sompi:if offset==0 {frame.conviction_rent_v1()}else{0},
                signer:outsider(),action:A::WitnessPart(Part {total_bytes:encoded.len() as u64,digest,offset:offset as u64,bytes:encoded[offset..end].to_vec()}),signature:vec![5;SIG]};
            apply_verified_control_v1(&mut b,&at(&frame,at_offset),&frame,&control).unwrap();
        }
        assert!(matches!(b.state.claim(&p.original_claim).unwrap().phase,PalwClaimPhaseV2::Voided {reason:PalwVoidReasonV2::CourtFraud,..}));
        assert_eq!(b.state.bond(&outsider()).unwrap().collateral,before);
        let a=b.state.court_end_archive_v1(id).unwrap();assert_eq!(a.groups.len(),2);
        assert!(matches!(a.evidence.as_ref(),crate::palw_court_end_archive_v1::CourtEndEvidenceV1::TirConviction {evidence:e,point,..} if **e==evidence && *point==at(&frame,6)));
        archive_roundtrip(&mut b,&params,&frame,id,at(&frame,6),state.state_root());
    }

    #[test]
    fn rooted_service_court_sweeps_skip_held_without_starving_cleanup() {
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
        use crate::palw_bisect::{PalwBisectLadderV1, PalwBisectSpaceV1, PalwBisectTurnV1};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let (state, params, extras) = world(&fx, &frame);
        let plan = frame.plan_v1();
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket { id: [41; 64], release_daa: plan.terms.accepted_daa + 1,
            service_before: plan.terms.accepted_daa + 2, cost }], plan.terms.accepted_daa + 10_000).unwrap();
        book.observe_block(plan.terms.accepted_daa + 2, [42; 64], Default::default()).unwrap();
        let service = CourtServiceStateV2 { version: 2, source_profile_id: Hash64::from_u64_word(43), profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(Hash64::from_u64_word(43)), book,
            claims: std::collections::BTreeMap::from([(plan.original_claim, ServiceClaimBinding {
                claim: plan.original_claim, allocation_id: plan.allocation_id, source_id: plan.source_id,
                original_accepted_daa: plan.terms.accepted_daa, hard_deadline_daa: plan.terms.accepted_daa + 10_000,
                packet_ids: std::collections::BTreeSet::from([[41; 64]]) })]) };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(service).into()))).unwrap();
        let service_state = b.state.clone();
        let opened = plan.terms.accepted_daa + 3;
        let ctx = at(&frame, 20_000);
        // Responder, challenger and terminal/backstop all lack permission to
        // default. These are ladder-machine states, not signed court admission.
        for turn in [PalwBisectTurnV1::AwaitDisclosure, PalwBisectTurnV1::AwaitVerdict, PalwBisectTurnV1::Terminal] {
            let mut ladder = PalwBisectLadderV1::open(
                &frame.header_v1().job_context.context_hash(), &frame.original_v1().execution_root,
                &h(0xC1), &h(0xC2), PalwBisectSpaceV1::StepLeaves,
                frame.header_v1().step_leaf_count, opened, opened + 1,
            ).unwrap();
            if turn == PalwBisectTurnV1::AwaitVerdict {
                ladder.apply_disclosure(&crate::palw_bisect::PalwBisectDisclosureV1 {
                    version: 1, session_id: ladder.session_id(), round: 0,
                    midpoint: ladder.expected_midpoint().unwrap(), mid_state: h(0xD1),
                }, opened + 1, 1).unwrap();
            } else if turn == PalwBisectTurnV1::Terminal {
                ladder = PalwBisectLadderV1::open_at_named_leaf(
                    &frame.header_v1().job_context.context_hash(), &frame.original_v1().execution_root,
                    &h(0xC1), &h(0xC2), PalwBisectSpaceV1::StepLeaves,
                    frame.header_v1().step_leaf_count, 0, opened, opened + 1,
                ).unwrap();
            }
            assert_eq!(ladder.turn(), turn);
            let session = PalwCourtSessionStateV2 {
                claim: plan.original_claim, challenger_bond: outsider(), opened_daa: opened,
                deadline_daa: opened + 10, ladder, dissection: None,
            };
            let held_id = h(0xC3);
            let orphan_id = h(0xC4);
            let mut held = TransitionBuilder::new(&service_state, &params, false, false, false, false, &extras);
            held.write_court(held_id, Some(session.clone())).unwrap();
            let mut orphan = session.clone();
            orphan.claim = h(0xC5);
            orphan.deadline_daa += 1;
            // Both index keys are deliberately ordered: held first, orphan next.
            held.write_court(orphan_id, Some(orphan)).unwrap();
            let before = held.state.clone();
            let journal = held.entries.len();
            super::super::sweep_court_deadlines(&mut held, &ctx).unwrap();
            assert_eq!(held.state.court_sessions.get(&held_id), before.court_sessions.get(&held_id));
            assert!(!held.state.court_sessions.contains_key(&orphan_id));
            assert_eq!(held.state.claims, before.claims);
            assert_eq!(held.state.bonds, before.bonds);
            assert_eq!(held.state.court_deadlines.len(), 1);
            assert_eq!(held.state.court_deadlines.iter().next(), before.court_deadlines.iter().next());
            assert!(held.entries.len() > journal, "orphan cleanup must progress past the held head");
            let settled = held.state.clone();
            let journal = held.entries.len();
            super::super::sweep_court_deadlines(&mut held, &ctx).unwrap();
            assert_eq!(held.state, settled);
            assert_eq!(held.entries.len(), journal, "repeated sweep does not refresh a held clock");
            // The independent assembly sweep must also skip a held head,
            // preserving its charged deposit while cleaning up a later orphan.
            if turn == PalwBisectTurnV1::Terminal {
                let group = PalwCourtCloseGroupV2 {
                    declarer: frame.custody_v1().pledge_v1().producer, count: 1,
                    chunk_digests: vec![h(0xD2)], close_digest: h(0xD3),
                    verdict: PalwCourtVerdictV2::ExecutorGuilty, declared_daa: opened,
                    assembly_deadline_daa: opened + 2, present: 0,
                    deposit_outpoint: TransactionOutpoint::new(h(0xD4), 0),
                    deposit: 7, chunks: Default::default(),
                };
                let key = (held_id, PalwCourtSideV1::Executor);
                held.write_court_close_group(key, Some(group.clone())).unwrap();
                let orphan_key = (orphan_id, PalwCourtSideV1::Executor);
                let mut orphan_group = group.clone();
                orphan_group.assembly_deadline_daa += 1;
                orphan_group.deposit = 0;
                held.write_court_close_group(orphan_key, Some(orphan_group)).unwrap();
                let before = held.state.clone();
                super::super::sweep_court_close_deadlines(&mut held, &ctx).unwrap();
                assert_eq!(held.state.court_close_groups.get(&key), Some(&group));
                assert!(!held.state.court_close_groups.contains_key(&orphan_key));
                assert_eq!(held.state.bonds, before.bonds);
                assert_eq!(held.state.claims, before.claims);
                assert_eq!(held.state.court_sessions, before.court_sessions);
                assert_eq!(held.state.court_close_deadlines.len(), 1);
                let settled = held.state.clone();
                let journal = held.entries.len();
                super::super::sweep_court_close_deadlines(&mut held, &ctx).unwrap();
                assert_eq!(held.state, settled);
                assert_eq!(held.entries.len(), journal);
                let mut ordinary = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
                ordinary.write_court(held_id, Some(session.clone())).unwrap();
                ordinary.write_court_close_group(key, Some(group)).unwrap();
                super::super::sweep_court_close_deadlines(&mut ordinary, &ctx).unwrap();
                assert!(!ordinary.state.court_sessions.contains_key(&held_id));
                assert!(!ordinary.state.court_close_groups.contains_key(&key));
            }
            // Same expired session without service follows the existing ending.
            let mut ordinary = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            ordinary.write_court(held_id, Some(session)).unwrap();
            super::super::sweep_court_deadlines(&mut ordinary, &ctx).unwrap();
            assert!(!ordinary.state.court_sessions.contains_key(&held_id));
        }
    }

    #[test]
    fn rooted_service_hold_retains_final_collateral_and_material_past_hard() {
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let (demanded, params, extras, acquisition, offset) = demanded_world(&fx, &frame, 0);
        let (state, _) = producer_answers(&fx, &frame, &demanded, &params, &extras, acquisition, 0, offset);
        let plan = frame.plan_v1();
        let claim = state.claim(&plan.original_claim).unwrap().clone();
        // The candidate may retain state, but cannot manufacture a longer
        // signed service promise. Boundary is on the actual accepted pledge.
        let signed_until = frame.custody_v1().pledge_v1().material_until_daa;
        for until in [signed_until - 1, signed_until] {
            crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(&state, &frame, until).unwrap();
        }
        assert!(crate::palw_court_service_state_v2::require_tir_service_custody_window_v1(
            &state, &frame, signed_until.checked_add(1).unwrap()).unwrap_err().contains("signed TIR custody"));
        let now = plan.terms.material_until_daa.max(plan.terms.bond_exit_after_daa).max(plan.terms.accepted_daa + 10_000) + 1;
        let ctx = at(&frame, now - plan.terms.accepted_daa);
        // The old path retains its finite TTL; no old signed deadline is rewritten.
        let mut old = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        super::super::palw_staged_court_fold_v1::sweep(&mut old, &ctx).unwrap();
        assert!(palw_tir_public_claim_v1(&old.state, plan.original_claim).is_none());
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve(&[ServicePacket { id: [41; 64], release_daa: plan.terms.accepted_daa + 1,
            service_before: plan.terms.accepted_daa + 2, cost }], plan.terms.accepted_daa + 10_000).unwrap();
        book.observe_block(plan.terms.accepted_daa + 2, [42; 64], Default::default()).unwrap();
        let service = CourtServiceStateV2 { version: 2, source_profile_id: Hash64::from_u64_word(43), profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(Hash64::from_u64_word(43)), book,
            claims: std::collections::BTreeMap::from([(plan.original_claim, ServiceClaimBinding {
                claim: plan.original_claim, allocation_id: plan.allocation_id, source_id: plan.source_id,
                original_accepted_daa: plan.terms.accepted_daa, hard_deadline_daa: plan.terms.accepted_daa + 10_000,
                packet_ids: std::collections::BTreeSet::from([[41; 64]]) })]) };
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(service).into()))).unwrap();
        // Advance the accepted point through the native journal shape, so the
        // snapshot really loads AFTER hard/retention rather than querying a late
        // clock against an earlier snapshot.
        let old_point = b.state.last_point;
        b.state.last_point = Some(ctx);
        b.entries.push(PalwDeltaEntryV2::LastPoint { old: old_point, new: Some(ctx) });
        let protected = b.state.clone();
        assert!(palw_rcore_deadline_v1(&b.state, &params, &plan.original_claim, &claim, Some(now)).unwrap().is_none());
        assert!(!b.state.deadlines.iter().any(|(_, id)| *id == plan.original_claim));
        b.finalize_claim(plan.original_claim, &claim, now).unwrap();
        b.retire_claim(plan.original_claim, &claim).unwrap();
        super::super::palw_staged_court_fold_v1::sweep(&mut b, &ctx).unwrap();
        assert_eq!(b.state, protected, "hard/TTL passage cannot Final, pay, retire, refund or prune");
        for liable in frame.custody_v1().liable_signers_v1() {
            assert!(b.state.staged_bond_exit_held_v1(&liable, now));
        }
        assert!(!b.state.staged_bond_exit_held_v1(&honest_seat(), now), "a seat with no credited Valid is not liable");
        assert!(!b.state.staged_bond_exit_held_v1(&outsider(), now), "the requesting owner is not a custodian");
        assert!(owner_pending(&frame, &b.state, acquisition).3.is_some(), "authenticated response remains available");
        assert!(b.write_staged_row_v1(frame.key_v1(), None).is_err(), "direct evidence erasure is refused too");
        assert_eq!(b.state, protected);
        // A stale derived deadline cannot bypass the hold; no replacement hard deadline is manufactured.
        b.state.deadlines.insert((now - 1, plan.original_claim));
        super::super::sweep_deadlines(&mut b, &ctx).unwrap();
        assert_eq!(b.state, protected);
        let mut restored = b.state.clone();
        for entry in b.entries.iter().rev() { apply_delta_entry(&mut restored, entry, true).unwrap(); }
        super::super::rebuild_deadline_index_v2(&mut restored, &params).unwrap();
        assert_eq!(restored.state_root(), state.state_root());
        assert!(!restored.court_service_holds_claim_v1(&plan.original_claim));
        for entry in &b.entries { apply_delta_entry(&mut restored, entry, false).unwrap(); }
        super::super::rebuild_deadline_index_v2(&mut restored, &params).unwrap();
        assert_eq!(restored.state_root(), protected.state_root());
        assert_eq!(restored.deadlines, protected.deadlines);
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&protected)).unwrap();
        let fresh = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap()
            .into_state(&params, Some(protected.state_root())).expect("fresh snapshot reconstructs unresolved service holds");
        assert_eq!(fresh.state_root(), protected.state_root());
        assert_eq!(fresh.deadlines, protected.deadlines);
        assert!(fresh.staged_bond_exit_held_v1(&frame.custody_v1().pledge_v1().producer, now));
        assert_eq!(palw_tir_public_claim_v1(&restored, plan.original_claim).unwrap().plan_v1().terms, plan.terms);
    }

    #[test]
    fn rooted_service_repair_journals_without_releasing_holds() {
        use crate::palw_court_service_v1::{ServiceCost, ServicePacket, ServiceReservations};
        use crate::palw_court_service_state_v2::{CourtServiceStateV2, ServiceClaimBinding};
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let (state, params, extras, _, owner_offset) = demanded_world(&fx, &frame, 0);
        let plan = frame.plan_v1();
        let hard = plan.terms.accepted_daa + 10_000;
        let cost = ServiceCost { bytes: 1, work: 1, io: 1, controls: 1 };
        // X3's de-stacked clock differs from the old main prototype: the
        // fixture's owner interval fits the original hard bound. Exercise the
        // rooted adapter there, without claiming calibrated service capacity.
        let owner_start = plan.terms.accepted_daa + owner_offset + 1;
        let mut geometry = ServiceReservations::new(cost).unwrap();
        geometry.reserve(&[ServicePacket { id:[50;64], release_daa:owner_start,
            service_before:owner_start+10, cost }],hard).unwrap();
        assert!(owner_start + 10 <= hard);
        eprintln!("X3 owner fixture geometry (not runtime/network measurement): accepted={}, preparation_until={}, owner_interval={}..{}, hard={}",
            plan.terms.accepted_daa,plan.preparation_until_daa,owner_start,owner_start+10,hard);
        let unchanged = geometry.clone();
        assert_eq!(geometry.reserve(&[ServicePacket { id:[49;64], release_daa:hard,
            service_before:hard+1, cost }],hard),
            Err(crate::palw_court_service_v1::ServiceRefusal::InvalidInterval));
        assert_eq!(geometry,unchanged,"a real out-of-bound interval still refuses atomically");
        let start = owner_start;
        let now = start + 1;
        let first = ServicePacket { id: [51;64], release_daa:start, service_before:start+10, cost };
        let other = ServicePacket { id: [52;64], release_daa:start+2, service_before:start+3, cost };
        let mut book = ServiceReservations::new(cost).unwrap();
        book.reserve(&[first.clone(), other.clone()], plan.terms.accepted_daa+10_000).unwrap();
        book.observe_block(now, [53;64], Default::default()).unwrap();
        let service = CourtServiceStateV2 { version:2, source_profile_id: Hash64::from_u64_word(54), profile_id: crate::palw_court_service_state_v2::court_service_profile_id_v2(Hash64::from_u64_word(54)), book,
            claims: std::collections::BTreeMap::from([(plan.original_claim, ServiceClaimBinding {
                claim:plan.original_claim, allocation_id:plan.allocation_id, source_id:plan.source_id,
                original_accepted_daa:plan.terms.accepted_daa, hard_deadline_daa:plan.terms.accepted_daa+10_000,
                packet_ids:std::collections::BTreeSet::from([first.id,other.id]) })]) };
        let point = at(&frame, now-plan.terms.accepted_daa);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(b.recover_court_service_supply_v1(&point).unwrap().is_empty());
        b.write_staged_row_v1(Key::CourtService, Some(Row::CourtService(Box::new(service).into()))).unwrap();
        let old_point = b.state.last_point;
        b.state.last_point = Some(point);
        b.entries.push(PalwDeltaEntryV2::LastPoint { old:old_point, new:Some(point) });
        let before = b.state.clone();
        let stale = at(&frame, now-plan.terms.accepted_daa+1);
        assert!(b.recover_court_service_supply_v1(&stale).is_err());
        assert_eq!(b.state,before,"a stale supply clock must not move reservations");
        let journal_start = b.entries.len();
        let report = b.recover_court_service_supply_v1(&point).unwrap();
        assert_eq!(report.len(),1);
        assert_eq!(report[0].claim,plan.original_claim);
        assert_eq!(report[0].result,Ok(1));
        let recovered = b.state.court_service_state_v1().unwrap();
        assert_eq!(recovered.book.assigned_daa(&first.id),Some(start+3));
        assert_eq!(recovered.book.assigned_daa(&other.id),Some(start+2));
        assert_eq!(recovered.book.packet(&first.id),Some(&first));
        assert_eq!(recovered.book.packet(&other.id),Some(&other));
        assert_eq!(recovered.claims, before.court_service_state_v1().unwrap().claims);
        assert!(b.state.court_service_holds_claim_v1(&plan.original_claim));
        assert!(b.state.staged_bond_exit_held_v1(&frame.custody_v1().pledge_v1().producer,now));
        assert!(recovered.default_hold(&frame).is_err(),"repair cannot authorize default");
        assert_eq!(b.state.claim(&plan.original_claim), before.claim(&plan.original_claim));
        assert_eq!(b.state.deadlines,before.deadlines);
        let after = b.state.clone();
        let entries = b.entries[journal_start..].to_vec();
        assert_eq!(entries.len(),1,"one native delta-200 row commits the atomic repair");
        assert!(b.recover_court_service_supply_v1(&point).unwrap().is_empty());
        assert_eq!(b.entries.len(),journal_start+1,"repeat does not emit another write");
        let mut reverted = after.clone();
        for entry in entries.iter().rev() { apply_delta_entry(&mut reverted,entry,true).unwrap(); }
        assert_eq!(reverted.state_root(),before.state_root());
        for entry in &entries { apply_delta_entry(&mut reverted,entry,false).unwrap(); }
        assert_eq!(reverted.state_root(),after.state_root());
        let bytes = borsh::to_vec(&PalwStateCarriageV2::from_state(&after)).unwrap();
        let fresh = borsh::from_slice::<PalwStateCarriageV2>(&bytes).unwrap()
            .into_state(&params,Some(after.state_root())).unwrap();
        assert_eq!(fresh.state_root(),after.state_root());
        assert_eq!(fresh.court_service_state_v1(),after.court_service_state_v1());
        assert!(fresh.court_service_holds_claim_v1(&plan.original_claim));
    }

    /// **G14 R5: an honest custodian that answers an outside owner's demand inside its clock is never defaulted or charged** —
    /// the owner's Collect past the clock finds nobody missing and is refused; the producer's collateral and claim are untouched.
    /// The charge is live, only unearned: the same producer silent on the owner's next demand IS collected once its clock passes.
    #[test]
    fn g14_an_honest_custodian_answers_in_time_and_is_not_slashed() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let base = frame.plan_v1().terms.accepted_daa;
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let (demanded, params, extras, acquisition, offset) = demanded_world(&fx, &frame, 0);
        let before = demanded.bonds.get(&producer).unwrap().collateral;
        let (demand, respond_until, turn, _) = owner_pending(&frame, &demanded, acquisition);
        let (answered, next) = producer_answers(&fx, &frame, &demanded, &params, &extras, acquisition, 0, offset);
        assert!(base + next <= respond_until, "the whole answer landed inside its own response clock");
        assert!(owner_pending(&frame, &answered, acquisition).3.is_some(), "the accepted slot holds the finished body");
        let late = respond_until - base;
        let collect = A::Acquisition(Owned::Collect { demand_id: demand });
        assert!(
            control_step(&frame, &answered, &params, &extras, late, outsider(), acquisition, collect, 0, turn).is_err(),
            "nobody is missing: there is nothing to collect"
        );
        let da = crate::palw_da_rcore_v1::palw_da_offence_id_v1(&producer.0, &claim_id);
        assert_eq!(answered.bonds.get(&producer).unwrap().collateral, before);
        assert!(!answered.consumed_offences.contains_key(&da));
        assert!(matches!(answered.claims.get(&claim_id).unwrap().phase, PalwClaimPhaseV2::ReceiptLicensed { .. }));
        // Contrast: a demand the producer leaves unanswered is collected past its clock.
        let next_demand =
            A::Acquisition(Owned::Demand { request: crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 1 } });
        let second = control_step(&frame, &answered, &params, &extras, late, outsider(), acquisition, next_demand, 0, turn + 1)
            .expect("an answered turn lets the owner demand again");
        let (demand2, until2, turn2, _) = owner_pending(&frame, &second, acquisition);
        let collect2 = A::Acquisition(Owned::Collect { demand_id: demand2 });
        let charged = control_step(&frame, &second, &params, &extras, until2 - base, outsider(), acquisition, collect2, 0, turn2)
            .expect("a silent custodian is collected past its own clock");
        assert!(charged.bonds.get(&producer).unwrap().collateral < before);
        assert!(charged.consumed_offences.contains_key(&da));
    }

    /// **G14 R4 end to end: an outsider detects a fault through its OWN acquisition, and its finding is filed, convicted and
    /// slashed.** The outside owner demands the leaf its independent replay disagrees with; the producer's accepted answer is read
    /// back from the rooted chunks only and re-authenticated against the original roots
    /// (`palw_tir_public_finished_material_v1`); the cone primitive over that same committed leaf passes the filer's dry run of
    /// the fold (`palw_tir_public_conviction_preflight_v1`) and the filing convicts: `CourtFraud`, the producer debited, the
    /// `CourtConviction` recorded once.
    #[test]
    fn g14_an_outsider_detects_a_fault_and_its_filed_finding_convicts_and_slashes() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicFinishedV1;
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = fx.frame.clone();
        let base = frame.plan_v1().terms.accepted_daa;
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let (demanded, params, extras, acquisition, offset) = demanded_world(&fx, &frame, FORGED_LEAF as u64);
        let before = demanded.bonds.get(&producer).unwrap().collateral;
        let (mut answered, next) = producer_answers(&fx, &frame, &demanded, &params, &extras, acquisition, FORGED_LEAF, offset);
        // The reader answers from an ACCEPTED snapshot (a node's tip): the block that accepted the Finish.
        answered.last_point = Some(at(&frame, next - 1));
        // Detection: the accepted answer, re-read from rooted chunks and re-authenticated, against the outsider's own replay.
        let reference = owner_pending(&frame, &answered, acquisition).3.expect("the producer's finished body");
        let Some(PalwTirPublicFinishedV1::Unit(unit)) = crate::palw_tir_public_state_v1::palw_tir_public_finished_material_v1(
            &answered,
            &reference,
            frame.descriptor_v1().maximum_leaves,
        )
        .unwrap() else {
            panic!("an authenticated step unit")
        };
        let crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf { opening, .. } = unit.response_v1() else {
            panic!("a step leaf")
        };
        let context = fx.execution.binding.job_context.clone();
        let replay = crate::palw_tir_court_v1::test_support::tiny_execution_in(|_, _, _| context.clone(), None);
        assert_eq!(opening.leaf_hash, fx.execution.hashes[FORGED_LEAF], "the accepted answer is the committed leaf");
        assert_ne!(replay.hashes[FORGED_LEAF], opening.leaf_hash, "the outsider's own replay disagrees exactly there");
        // The finding: the cone primitive over that same committed leaf.
        let evidence = cone(&fx);
        let Evidence::Arithmetic { proof } = &evidence else { unreachable!() };
        let crate::palw_court_v2::PalwCourtVerdictProofV2::TirCone { refutation } = proof.as_ref() else { unreachable!() };
        assert_eq!(refutation.output_opening.leaf_hash, opening.leaf_hash);
        // Filed: the fold's own dry run accepts it, and the filing convicts and slashes.
        palw_tir_public_conviction_preflight_v1(&answered, &params, &extras, base + next, outsider(), claim_id, &evidence)
            .expect("the filer's preflight accepts the finding");
        let convicted = control_step(
            &frame,
            &answered,
            &params,
            &extras,
            next,
            outsider(),
            T::Conviction,
            A::Convict(evidence),
            frame.conviction_rent_v1(),
            0,
        )
        .expect("the filing convicts");
        assert!(matches!(
            convicted.claims.get(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }
        ));
        assert!(convicted.bonds.get(&producer).unwrap().collateral < before);
        assert!(convicted.consumed_offences.contains_key(&palw_court_conviction_offence_id_v1(&producer.0, &claim_id)));
    }

    // ---------------------------------------------------------------------------------------------------------------------------
    // G14 close-out (2026-10-16, lane TIR): T-N2..T-N7 and the fence-at-genesis world. Fold level: the real native transitions
    // (`apply_object`, `install_original_v1`, `apply_verified_control_v1` — the signed carrier's ML-DSA check is the caller's), with
    // synthetic signatures exactly as the tests above. Node-level coverage is T-N1 (kaspad), not these.
    // ---------------------------------------------------------------------------------------------------------------------------

    /// The ADR-0145 prompt row the ordinary FP arm hands `install_original_v1` (the claim's own public prompt ids).
    fn prompt_row(fx: &PalwTirPublicFrameFixtureV1, frame: &Frame) -> PalwFpClaimPromptV1 {
        let ids = fx.execution.prompt.clone();
        PalwFpClaimPromptV1 {
            prompt_tokens: ids.len() as u32,
            prefix_root: crate::palw_freeprompt_v3::fp_prompt_prefix_root_v1(&ids),
            prompt_token_ids: ids,
            expires_daa: palw_fp_prompt_row_expiry_v1(frame.plan_v1().terms.accepted_daa, 1000),
            consumed_state_root: Hash64::default(),
            consumed_prefix_tokens: 0,
        }
    }

    fn court_rules() -> crate::palw_tir_court_v1::PalwTirCourtRulesV1 {
        crate::palw_tir_court_v1::PalwTirCourtRulesV1 {
            max_step_leaf_count: 1 << 26,
            prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1::Flat,
            limits: misaka_palw_tir::demand::DemandLimits::UNLIMITED,
        }
    }

    /// The trace tile `(row, tile 0)` of an execution, exactly as a custodian's `TraceTile` answer carries it (the tiny class has
    /// one tile per row).
    fn trace_tile(
        x: &crate::palw_tir_court_v1::test_support::TinyExecution,
        row: u32,
    ) -> (crate::palw_tir_public_source_v1::PalwTirPublicRequestV1, crate::palw_tir_public_source_v1::PalwTirPublicResponseV1) {
        let pin = crate::palw_step_refute::tiled_decode_pin_v1(&x.binding.job_context, &x.rows, &x.generated, row, 0).unwrap();
        (
            crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::TraceTile { row, tile: 0 },
            crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::TraceTile {
                row_root: pin.row_root,
                row_opening: pin.row_opening,
                lanes: pin.committed_tile_lanes,
                opening: pin.committed_opening,
            },
        )
    }

    /// **T-N2 `g14_tir_frame_decode_token_and_stop_rule_filing_convicts`** — one case each: greedy, seeded Decision 11 sampling,
    /// the V4 pipeline with penalties, and the V4 stop rule. In each, the producer's committed token (or answer length) violates
    /// the ORIGINAL job's own rule over honest arithmetic; every credited Panel seat colludes. One outside bond localizes the
    /// selection fault from public material alone with the node's helper (`palw_tir_public_selection_probe_v1`: the complete
    /// committed IDs, an independent replay's IDs, the acquired trace tile), files it, and the fold convicts — `CourtFraud`, both
    /// colluding Valid seats charged once, the honest drawn seat untouched. The honest twin of every rule probes `Agrees`.
    #[test]
    fn g14_tir_frame_decode_token_and_stop_rule_filing_convicts() {
        use crate::palw_decode_pipeline_v4::DecodeConfigV4;
        use crate::palw_tir_objective_v1::{PalwTirSelectionProbeV1 as Probe, palw_tir_public_selection_probe_v1};
        let off = DecodeConfigV4 {
            repeat_penalty_q: 65_536,
            penalty_window: 0,
            frequency_penalty_q: 0,
            presence_penalty_q: 0,
            logit_bias: Vec::new(),
            stop_sequences: Vec::new(),
        };
        let penalties = DecodeConfigV4 {
            repeat_penalty_q: 4 * 65_536,
            penalty_window: 4,
            frequency_penalty_q: 2 << 24,
            presence_penalty_q: 2 << 24,
            ..off.clone()
        };
        type Edit = Box<dyn Fn(&mut crate::palw_freeprompt_v3::PalwFreePromptJobV3)>;
        let greedy: Edit = Box::new(|_: &mut crate::palw_freeprompt_v3::PalwFreePromptJobV3| {});
        let seeded: Edit = Box::new(|job: &mut crate::palw_freeprompt_v3::PalwFreePromptJobV3| {
            job.temperature_q = 1 << 24;
            job.sampling_seed = [7; 32];
        });
        let v4 = |config: DecodeConfigV4| -> Edit {
            Box::new(move |job: &mut crate::palw_freeprompt_v3::PalwFreePromptJobV3| {
                job.version = crate::palw_freeprompt_v3::PALW_FP_V4_VERSION;
                job.decode = Some(config.clone());
            })
        };
        // Selection cases: (name, job edit, the decode position whose committed token violates the job's rule).
        let selection: Vec<(&str, Edit, u32)> =
            vec![("greedy", greedy, 0), ("seeded", seeded, 0), ("v4-penalty", v4(penalties.clone()), 1)];
        for (name, edit, wrong) in selection {
            let honest = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(4, &edit, None);
            let lying = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(4, &edit, Some(wrong));
            let rule = crate::palw_tir_court_v1::PalwTirSelectionRuleV1::of_job_v1(&lying.frame.original_v1().job).unwrap();
            let vocab = lying.execution.rows[0].len() as u32;
            let replay = honest.execution.generated.clone();
            assert_ne!(lying.execution.generated, replay, "{name}: the lie commits another token than the rule");
            assert_eq!(lying.execution.generated[..wrong as usize], replay[..wrong as usize], "{name}: the prefix agrees");
            // The honest twin: the probe finds nothing to localize.
            assert_eq!(
                palw_tir_public_selection_probe_v1(
                    &honest.execution.binding,
                    vocab,
                    &rule,
                    &court_rules(),
                    &honest.execution.generated,
                    &replay,
                    &[]
                )
                .unwrap(),
                Probe::Agrees,
                "{name}"
            );
            // Localization: the first disagreement names its row's tile; with the custodian's answer the pin is built.
            let need = palw_tir_public_selection_probe_v1(
                &lying.execution.binding,
                vocab,
                &rule,
                &court_rules(),
                &lying.execution.generated,
                &replay,
                &[],
            )
            .unwrap();
            assert_eq!(
                need,
                Probe::Need(vec![crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::TraceTile { row: wrong, tile: 0 }]),
                "{name}"
            );
            let Probe::Ready { position, evidence } = palw_tir_public_selection_probe_v1(
                &lying.execution.binding,
                vocab,
                &rule,
                &court_rules(),
                &lying.execution.generated,
                &replay,
                &[trace_tile(&lying.execution, wrong)],
            )
            .unwrap() else {
                panic!("{name}: the selection pin is built and refutes the committed token")
            };
            assert_eq!(position, wrong, "{name}");
            // Filed by one outside bond: convicted under the ORIGINAL job's own rule.
            let frame = credited_frame(&lying);
            let (state, params, extras) = world(&lying, &frame);
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), evidence))
                .unwrap_or_else(|e| panic!("{name}: the selection filing convicts: {e:?}"));
            let (phase, collateral, consumed) = outcome(&b.state, &frame);
            assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{name}: {phase:?}");
            assert_eq!(collateral[3], COLLATERAL, "{name}: the honest drawn seat is untouched");
            assert_eq!(consumed, vec![true, true, true, false], "{name}");
            // The same evidence against the honest twin's frame refutes nothing: refused, nothing moves.
            let honest_frame = credited_frame(&honest);
            let (state, params, extras) = world(&honest, &honest_frame);
            let root = state.state_root();
            let Probe::Ready { evidence, .. } = palw_tir_public_selection_probe_v1(
                &lying.execution.binding,
                vocab,
                &rule,
                &court_rules(),
                &lying.execution.generated,
                &replay,
                &[trace_tile(&lying.execution, wrong)],
            )
            .unwrap() else {
                unreachable!()
            };
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            assert!(
                apply_verified_control_v1(&mut b, &at(&honest_frame, 5), &honest_frame, &filing(&honest_frame, outsider(), evidence))
                    .is_err(),
                "{name}: another claim's pin does not authenticate against this claim"
            );
            assert_eq!(b.state.state_root(), root, "{name}");
        }
        // The stop rule: the honest decoder of a V4 job whose stop sequence is the first token stops after it, but the claim commits
        // the whole budget. Output door, from the complete authenticated IDs only.
        let first = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(4, |_| {}, None).execution.generated[0];
        let stop = DecodeConfigV4 { stop_sequences: vec![vec![first]], ..off };
        let lying = crate::palw_tir_public_claim_v1::palw_tir_public_frame_fixture_ruled_v1(4, v4(stop), None);
        let ids = lying.execution.generated.clone();
        assert_eq!(ids[0], first);
        let vocab = lying.execution.rows[0].len() as u32;
        assert!(
            crate::palw_tir_objective_v1::original_stop_rule_fault_v1(&lying.frame.original_v1().job, vocab, &ids).is_some(),
            "the answer continues past its completed stop sequence"
        );
        let rows_root = crate::palw_step_refute::tiled_logits_rows_root_v1(&lying.execution.binding.job_context, &lying.execution.rows)
            .unwrap();
        let frame = credited_frame(&lying);
        let (state, params, extras) = world(&lying, &frame);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), Evidence::Output { rows_root, ids }))
            .expect("the stop-rule filing convicts");
        let (phase, collateral, consumed) = outcome(&b.state, &frame);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert_eq!(collateral[3], COLLATERAL);
        assert_eq!(consumed, vec![true, true, true, false]);
    }

    /// **T-N3 `g14_tir_frame_without_tfp1_earns_nothing_past_the_fence`** (G-03) — through the ordinary Free Prompt acceptance arm
    /// (`apply_object`): past `palw_tir_public_conviction_v1` a TIR original WITHOUT its paired `TFP1` is refused by name
    /// (`G03_FRAMELESS_TIR_ORIGINAL`) before anything is priced or written — no claim, so no Final, weight, receipt spend, escrow,
    /// coinbase leg or Gate C credit can ever rest on it. Below the fence the same object is not judged by this rule (A-2), and
    /// past it an object WITH its paired envelope passes this rule (to the frame's own admission).
    #[test]
    fn g14_tir_frame_without_tfp1_earns_nothing_past_the_fence() {
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = fx.frame.clone();
        let (mut state, params, mut extras) = world(&fx, &frame);
        let claim_id = frame.plan_v1().original_claim;
        // A fresh original: no claim row yet; the producer's registered key is the commitment's.
        state.claims.remove(&claim_id);
        state.staged_courts.remove(&frame.key_v1());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let PalwConsensusObjectV2::FreePromptCommitted { executor_pubkey, .. } = &fx.object else { unreachable!() };
        state.bonds.get_mut(&frame.custody_v1().pledge_v1().producer).unwrap().pubkey = executor_pubkey.clone();
        let root = state.state_root();
        let g03 = |result: &Result<(), PalwStateV2Error>| {
            matches!(result, Err(PalwStateV2Error::ClassNotRewardable { code, .. }) if *code == PALW_G03_FRAMELESS_TIR_ORIGINAL_CODE_V1)
        };
        // Past the fence, frameless: refused by name, nothing written.
        extras.tir_public_conviction_active = true;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let refused = super::super::objects::apply_object(&mut b, &at(&frame, 0), &fx.object);
        assert!(g03(&refused), "{refused:?}");
        assert!(!b.state.claims.contains_key(&claim_id), "no claim: nothing can earn");
        assert_eq!(b.state.state_root(), root);
        // Below the fence: this rule does not judge (A-2) — whatever else the arm says, it is not G-03.
        extras.tir_public_conviction_active = false;
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(!g03(&super::super::objects::apply_object(&mut b, &at(&frame, 0), &fx.object)));
        // A non-TIR class is never asked.
        let mut other = state.clone();
        other.tir_classes.remove(&frame.descriptor_v1().class_id);
        extras.tir_public_conviction_active = true;
        let mut b = TransitionBuilder::new(&other, &params, false, false, false, false, &extras);
        assert!(!g03(&super::super::objects::apply_object(&mut b, &at(&frame, 0), &fx.object)));
        // The rule itself, by its three inputs.
        let b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        assert!(g03(&require_original_frame_v1(&b, claim_id, frame.descriptor_v1().class_id)));
        let mut paired = extras.clone();
        let envelope = crate::palw_tir_public_fp_envelope_v1::PalwTirPublicFpEnvelopeV1 {
            version: 1,
            declared_rent_sompi: frame.plan_v1().terms.plan.rent_sompi,
            original_payload: Vec::new(),
            header: frame.header_v1().clone(),
            pledge: frame.custody_v1().pledge_v1().clone(),
            history_pledge: None,
        };
        paired.tir_public_originals.insert(claim_id, envelope);
        let b = TransitionBuilder::new(&state, &params, false, false, false, false, &paired);
        assert!(require_original_frame_v1(&b, claim_id, frame.descriptor_v1().class_id).is_ok(), "a paired original reaches its frame");
    }

    /// The `n` extra outside bonds of the slot tests, registered Active above the floor.
    fn squatters(n: u64) -> Vec<PalwBondKeyV2> {
        (0..n).map(|i| bond(400 + i)).collect()
    }

    /// The installed frame (producer Ready) with the `squatters` bonds registered, at the first DAA an owner may Open.
    fn open_world(
        fx: &PalwTirPublicFrameFixtureV1,
        frame: &Frame,
        extra: &[PalwBondKeyV2],
    ) -> (PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1, u64) {
        let (mut state, params, extras) = world(fx, frame);
        for (i, key) in extra.iter().enumerate() {
            state.bonds.insert(*key, bond_state(100 + i as u64));
        }
        state.staged_courts.remove(&frame.key_v1());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        install_original_v1(&mut b, &at(frame, 0), frame.clone(), &prompt_row(fx, frame)).unwrap();
        let installed = b.state.clone();
        let ready = producer_ready(frame, &installed, &params, &extras);
        let plan = frame.plan_v1();
        (ready, params, extras, plan.preparation_until_daa - plan.terms.accepted_daa)
    }

    fn open(
        frame: &Frame,
        state: &PalwChainStateV2,
        params: &PalwStateParamsV2,
        extras: &PalwTransitionExtrasV1,
        offset: u64,
        owner: PalwBondKeyV2,
    ) -> Result<PalwChainStateV2, PalwStateV2Error> {
        let rent = owner_terms_v1(frame).unwrap().plan.rent_sompi;
        control_step(frame, state, params, extras, offset, owner, T::OwnerOpen { owner }, A::Open, rent, 0)
    }

    fn owner_row(state: &PalwChainStateV2, frame: &Frame, owner: PalwBondKeyV2, reserved: u64) -> PalwTirPublicOwnedV1 {
        let key = owner_key(frame, owner, reserved).unwrap();
        let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else { panic!("{owner:?}'s acquisition row") };
        row.owned.as_ref().clone()
    }

    /// **T-N4 `g14_tir_owner_slots_cannot_be_preempted_by_idle_owners`** (G-T1; Lead default R1, the cap stays K_owner = 8).
    /// Eight colluding bonds take every owner slot the moment the frame opens and then do nothing. While they are fresh an honest
    /// outsider's Open is refused (the custodian-load cap holds); once they have made no progress for the native grace, the
    /// outsider's Open reclaims the longest-idle slot: that row is retired in place (it can never demand again), its owner's deposit
    /// is burned (paid to nobody), the frame still carries eight slot holders, and the outsider proceeds to Demand. An owner with an
    /// unpaid demand inside its clock is never reclaimed, and one answered late keeps the whole grace from the answer.
    #[test]
    fn g14_tir_owner_slots_cannot_be_preempted_by_idle_owners() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let colluders = squatters(Frame::MAXIMUM_OWNERS_V1);
        let (mut state, params, extras, open_at) = open_world(&fx, &frame, &colluders);
        let claim_id = frame.plan_v1().original_claim;
        let base = frame.plan_v1().terms.accepted_daa;
        for owner in &colluders {
            state = open(&frame, &state, &params, &extras, open_at, *owner).expect("a colluder takes a slot");
        }
        assert_eq!(state.staged_tir_owner_count_v1(&claim_id), Frame::MAXIMUM_OWNERS_V1);
        let grace = owner_row(&state, &frame, colluders[0], base + open_at).owner_idle_grace_daa_v1();
        let open_until = frame.plan_v1().terms.open_until_daa - base;
        assert!(open_at + grace + 1 < open_until, "the grace fits inside the frame's open window");
        // Fresh holders: the outsider is refused, nothing moves.
        assert!(!state.staged_tir_owner_slot_open_v1(&claim_id, base + open_at + 1));
        assert!(open(&frame, &state, &params, &extras, open_at + 1, outsider()).is_err());
        // One colluder keeps a live unpaid demand (its custodian stays silent): it is never the slot reclaimed.
        let busy = colluders[0];
        let busy_target = T::Acquisition { owner: busy, reserved_daa: base + open_at };
        let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: 0 };
        state = control_step(&frame, &state, &params, &extras, open_at + 1, busy, busy_target, A::Acquisition(Owned::Demand { request }), 0, 0)
            .expect("the busy colluder demands");
        // Past the grace: every other holder is idle; the outsider's Open reclaims exactly one slot.
        let late = open_at + 1 + grace;
        assert!(state.staged_tir_owner_slot_open_v1(&claim_id, base + late));
        let (idle_key, idle_owner) = state.staged_tir_idle_owner_v1(&claim_id, base + late).unwrap();
        assert_ne!(idle_owner, busy, "a holder with an unpaid demand inside its clock is never reclaimed");
        let before = state.bonds.get(&idle_owner).unwrap().clone();
        let deposit = owner_terms_v1(&frame).unwrap().plan.deposit_sompi;
        let opened = open(&frame, &state, &params, &extras, late, outsider()).expect("an idle slot never locks the outsider out");
        assert_eq!(opened.staged_tir_owner_count_v1(&claim_id), Frame::MAXIMUM_OWNERS_V1, "the cap stays K_owner");
        let after = opened.bonds.get(&idle_owner).unwrap();
        assert_eq!(after.collateral, before.collateral - deposit, "the idle owner's deposit is forfeited");
        assert_eq!(after.slashed, before.slashed + deposit, "burned (recorded as slashed), paid to nobody");
        assert_eq!(opened.bonds.get(&outsider()).unwrap().collateral, COLLATERAL, "reclaiming pays the outsider nothing");
        let Some(Row::TirAcquisition(retired)) = opened.staged_court_row_v1(&idle_key) else { panic!("the retired row stays to its horizon") };
        assert!(!retired.owned.holds_owner_slot_v1() && retired.owned.active_v1().is_none());
        let crate::palw_staged_court_state_v1::PalwStagedCourtKeyV1::TirAcquisition(_, _, _) = idle_key else { unreachable!() };
        let retired_target = T::Acquisition { owner: idle_owner, reserved_daa: base + open_at };
        assert!(
            control_step(&frame, &opened, &params, &extras, late + 1, idle_owner, retired_target, A::Acquisition(Owned::Demand { request }), 0, 0)
                .is_err(),
            "a retired owner never demands again (the custodian-load bound holds)"
        );
        // The outsider proceeds.
        let mine = T::Acquisition { owner: outsider(), reserved_daa: base + late };
        let demanded = control_step(&frame, &opened, &params, &extras, late + 1, outsider(), mine, A::Acquisition(Owned::Demand { request }), 0, 0)
            .expect("the outsider demands from its reclaimed slot");
        demanded.assert_tir_acquisition_indices_v1().unwrap();
        // A second outsider at the same DAA reclaims the next idle slot (each Open reclaims one).
        let second = bond(390);
        let mut with_second = demanded.clone();
        with_second.bonds.insert(second, bond_state(390));
        assert!(open(&frame, &with_second, &params, &extras, late + 1, second).is_ok());
        // Reorg: the reclaiming block's journal restores the eight fresh holders and the forfeited deposit exactly.
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        let rent = owner_terms_v1(&frame).unwrap().plan.rent_sompi;
        let control = Control {
            version: 1,
            claim: claim_id,
            allocation_id: frame.plan_v1().allocation_id,
            source_id: frame.plan_v1().source_id,
            target: T::OwnerOpen { owner: outsider() },
            expected_turn: 0,
            declared_rent_sompi: rent,
            signer: outsider(),
            action: A::Open,
            signature: vec![5; SIG],
        };
        apply_verified_control_v1(&mut b, &at(&frame, late), &frame, &control).unwrap();
        let mut reverted = b.state.clone();
        for entry in b.entries.iter().rev() {
            apply_delta_entry(&mut reverted, entry, true).unwrap();
        }
        assert_eq!(reverted.state_root(), state.state_root());
    }

    /// **Lane g14-court-capacity-r2 (audit 2026-10-17 §6.2 / §7 P0(2)): eight colluding ACTIVE owners cannot lock an honest newcomer
    /// out, and the newcomer convicts in time.** Past `palw_court_capacity_v1` (the owner fair start): eight colluding bonds take every
    /// owner slot the moment the frame opens and keep them ACTIVE (each demand answered inside its clock by the colluding producer, the
    /// next demand at once — never idle, so idle reclamation never fires). An honest outsider's Open is NOT refused: it waits in the
    /// claim's owner queue, imposes no custodian load (its Demand is refused while it waits), and eight more coalition Opens filed after
    /// it queue BEHIND it (the refill). When the first colluder has spent its prepaid turns its slot passes to the outsider — never to a
    /// later coalition row — and the outsider demands the leaf its replay disagrees with, reads the producer's accepted answer and files
    /// the conviction: `CourtFraud`, the producer debited, inside the frame's collection door. Custodian load never exceeds K_owner.
    /// Below the fence the same Open at K_owner is refused (T-N4's world).
    #[test]
    fn g14r2_eight_active_colluding_owners_cannot_lock_out_a_newcomer_who_convicts_in_time() {
        eight_active_colluders_newcomer_convicts_v1(|params| {
            params.with_court_capacity_v1(Some(crate::palw_court_capacity_v1::PalwCourtCapacityMirrorV1 {
                from_daa: 0,
                policy: crate::palw_court_capacity_v1::palw_court_capacity_placeholder_policy_v1(),
            }))
        });
    }

    /// **G14 F03 ④ (lane g14p/court-owner): the same all-colluder occupancy with continuous refill, past `palw_court_outsider_lane_v1`
    /// alone** (no court-capacity policy, whose RFC-0025 parameters are not adopted): the owner queue's tenure rotation and idle
    /// reclamation give the outsider a slot in time; K_owner stays 8 (asserted inside); below the fence the Open is refused as before.
    #[test]
    fn g14_f03_outsider_lane_eight_active_colluders_cannot_lock_out_a_newcomer() {
        eight_active_colluders_newcomer_convicts_v1(|params| params.with_court_outsider_lane_from_daa(Some(0)));
    }

    fn eight_active_colluders_newcomer_convicts_v1(arm: impl Fn(PalwStateParamsV2) -> PalwStateParamsV2) {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = fx.frame.clone();
        let colluders = squatters(Frame::MAXIMUM_OWNERS_V1);
        let late_coalition: Vec<PalwBondKeyV2> = (0..Frame::MAXIMUM_OWNERS_V1).map(|i| bond(500 + i)).collect();
        let mut extra = colluders.clone();
        extra.extend(late_coalition.iter().copied());
        let (mut state, params, extras, open_at) = open_world(&fx, &frame, &extra);
        // Below the fence (T-N4's world) the outsider is refused at K_owner; past it, the fair start.
        let below = params.clone();
        let params = arm(params);
        let claim_id = frame.plan_v1().original_claim;
        let base = frame.plan_v1().terms.accepted_daa;
        let producer = frame.custody_v1().pledge_v1().producer;
        let terms = frame.plan_v1().terms;
        for owner in &colluders {
            state = open(&frame, &state, &params, &extras, open_at, *owner).expect("a colluder takes a slot");
        }
        let max_turns = owner_row(&state, &frame, colluders[0], base + open_at).contract_v1().plan.maximum_turns;
        eprintln!(
            "[G14 r2 TIR] open_at {open_at}, open_until {}, collect_until {}, maximum_turns {max_turns}",
            terms.open_until_daa - base,
            terms.collect_until_daa - base
        );
        assert_eq!(state.staged_tir_owner_count_v1(&claim_id), Frame::MAXIMUM_OWNERS_V1);
        assert!(open(&frame, &state, &below, &extras, open_at + 1, outsider()).is_err(), "below the fence: refused at K_owner");
        // The colluders go active: each demands at once (the producer answers inside the clock below).
        let mut offset = open_at + 1;
        let leaf_of = |_: usize| FORGED_LEAF as u64;
        for (i, owner) in colluders.iter().enumerate() {
            let target = T::Acquisition { owner: *owner, reserved_daa: base + open_at };
            let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: leaf_of(i) };
            state = control_step(&frame, &state, &params, &extras, offset, *owner, target, A::Acquisition(Owned::Demand { request }), 0, 0)
                .expect("a colluder demands");
        }
        // The honest newcomer: not refused — it waits; its Demand is refused while it waits.
        offset += 1;
        let arrived = offset;
        state = open(&frame, &state, &params, &extras, offset, outsider()).expect("never refused for the K_owner cap past the fence");
        let mine = T::Acquisition { owner: outsider(), reserved_daa: base + arrived };
        let mine_key = owner_key(&frame, outsider(), base + arrived).unwrap();
        assert!(!state.staged_tir_owner_admitted_v1(&claim_id, &mine_key), "it waits behind the eight");
        let forged = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: FORGED_LEAF as u64 };
        let refused =
            control_step(&frame, &state, &params, &extras, offset + 1, outsider(), mine, A::Acquisition(Owned::Demand { request: forged }), 0, 0)
                .expect_err("a waiting row demands nothing");
        assert!(format!("{refused}").contains("waiting in the owner queue"), "{refused}");
        // The refill: eight more coalition Opens AFTER the newcomer queue behind it.
        offset += 1;
        for owner in &late_coalition {
            state = open(&frame, &state, &params, &extras, offset, *owner).expect("the coalition queues too");
        }
        // Keep the colluders active (every open demand answered inside its clock, the next demand at once); every block the newcomer
        // tries its Demand, which rotates the queue: no colluder is ever idle, but an admitted row past its tenure yields to it.
        // Custodian load (demands open at once) never exceeds K_owner.
        offset += 1;
        let mut turns = vec![1u64; colluders.len()];
        let mut admitted_at = None;
        while admitted_at.is_none() {
            assert!(base + offset < terms.open_until_daa, "the newcomer must get its slot inside the open window");
            for (i, owner) in colluders.iter().enumerate() {
                let target = T::Acquisition { owner: *owner, reserved_daa: base + open_at };
                let key = owner_key(&frame, *owner, base + open_at).unwrap();
                let Some(Row::TirAcquisition(row)) = state.staged_court_row_v1(&key) else { panic!("colluder row") };
                if !row.owned.holds_owner_slot_v1() {
                    continue;
                }
                if row.owned.active_v1().is_some_and(|p| p.slots.get(&producer).and_then(|s| s.finished.as_ref()).is_none()) {
                    let (answered, next) = producer_answers(&fx, &frame, &state, &params, &extras, target, leaf_of(i) as usize, offset);
                    state = answered;
                    offset = offset.max(next);
                }
                if turns[i] < max_turns {
                    let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: leaf_of(i) };
                    state = control_step(&frame, &state, &params, &extras, offset, *owner, target, A::Acquisition(Owned::Demand { request }), 0, turns[i])
                        .expect("the colluder stays active");
                    turns[i] += 1;
                }
            }
            assert!(state.staged_tir_idle_admitted_owners_v1(&claim_id, base + offset).is_empty(), "the colluders are never idle");
            offset += 1;
            // The node's owner worker asks this before it signs a Demand; it must agree with the fold, block by block.
            let grace = owner_row(&state, &frame, outsider(), base + arrived).owner_idle_grace_daa_v1();
            let tenure = crate::palw_tir_public_claim_v1::palw_tir_owner_tenure_v1(&frame, grace);
            let predicted = state.staged_tir_owner_demand_admits_v1(&claim_id, &mine_key, base + offset, tenure);
            match control_step(&frame, &state, &params, &extras, offset, outsider(), mine, A::Acquisition(Owned::Demand { request: forged }), 0, 0) {
                Ok(next) => {
                    assert!(predicted, "the worker's predicate admits exactly when the fold does");
                    state = next;
                    admitted_at = Some(offset);
                }
                Err(why) => {
                    assert!(!predicted, "the worker would have wasted a Demand: {why}");
                    assert!(format!("{why}").contains("waiting in the owner queue"), "{why}");
                }
            }
            let open_demands = state
                .staged_tir_owner_queue_v1(&claim_id)
                .iter()
                .filter(|(key, ..)| matches!(state.staged_court_row_v1(key), Some(Row::TirAcquisition(r)) if r.owned.active_v1().is_some()))
                .count() as u64;
            assert!(open_demands <= Frame::MAXIMUM_OWNERS_V1, "custodian load bounded by K_owner: {open_demands}");
        }
        let admitted_at = admitted_at.unwrap();
        // Every colluder past its tenure yielded, with no forfeit (they were active); the slots went in arrival order — the newcomer
        // first — and the last of the later coalition Opens still waits.
        let yielded: Vec<_> = colluders
            .iter()
            .filter(|o| !owner_row(&state, &frame, **o, base + open_at).holds_owner_slot_v1())
            .collect();
        assert!(!yielded.is_empty(), "slots rotated");
        assert!(yielded.iter().all(|o| state.bonds.get(*o).unwrap().slashed == 0), "an active owner yields without forfeit");
        let queue = state.staged_tir_owner_queue_v1(&claim_id);
        let position = queue.iter().position(|(k, ..)| *k == mine_key).expect("the newcomer is queued");
        assert!(queue[..position].iter().all(|(_, owner, ..)| colluders.contains(owner)), "nobody filed later is ahead of it");
        eprintln!("[G14 r2 TIR] {} colluders yielded at the tenure; queue position {position} of {}", yielded.len(), queue.len());
        // The newcomer's demand of the forged leaf is in; the producer answers, and the newcomer files the conviction.
        let (mut answered, next) = producer_answers(&fx, &frame, &state, &params, &extras, mine, FORGED_LEAF, admitted_at + 1);
        answered.last_point = Some(at(&frame, next - 1));
        let reference = owner_pending(&frame, &answered, mine).3.expect("the producer's finished body");
        assert!(
            crate::palw_tir_public_state_v1::palw_tir_public_finished_material_v1(&answered, &reference, frame.descriptor_v1().maximum_leaves)
                .unwrap()
                .is_some(),
            "the newcomer's own authenticated material"
        );
        let before = answered.bonds.get(&producer).unwrap().collateral;
        assert!(base + next < terms.collect_until_daa, "inside the collection door");
        let convicted = control_step(&frame, &answered, &params, &extras, next, outsider(), T::Conviction, A::Convict(cone(&fx)), frame.conviction_rent_v1(), 0)
            .expect("the newcomer's filing convicts");
        eprintln!(
            "[G14 r2 TIR] newcomer arrived at +{arrived}, admitted at +{admitted_at}, convicted at +{next} (open_until +{}, collect_until +{})",
            terms.open_until_daa - base,
            terms.collect_until_daa - base
        );
        assert!(matches!(
            convicted.claims.get(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }
        ));
        assert!(convicted.bonds.get(&producer).unwrap().collateral < before, "the producer is slashed");
    }

    /// **T-N5 `g14_tir_frame_replays_without_derived_work_prompt_ids`** (G-T3). On a ruleset where `palw_fp_derived_work` priced
    /// nothing (no prompt row exists), installing the ORIGINAL frame keeps the claim's public prompt ids under its ADR-0145 row to
    /// at least the frame's material horizon (epoch-aligned); an outside verifier reads them back
    /// (`fp_claim_prompt_ids_v1`) and its independent replay of the original job from them reproduces the frame's committed step
    /// and trace roots. Ids that are not the job's own (a `PanelDa` original carries none) refuse the frame.
    #[test]
    fn g14_tir_frame_replays_without_derived_work_prompt_ids() {
        let fx = palw_tir_public_frame_fixture_v1(None, 4);
        let frame = fx.frame.clone();
        let (mut state, params, extras) = world(&fx, &frame);
        state.staged_courts.remove(&frame.key_v1());
        super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
        let class_id = frame.descriptor_v1().class_id;
        let producer = frame.custody_v1().pledge_v1().producer;
        let claim_id = frame.plan_v1().original_claim;
        assert!(state.fp_claim_prompts.is_empty(), "derived work priced nothing: no prompt row before the frame");
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        install_original_v1(&mut b, &at(&frame, 0), frame.clone(), &prompt_row(&fx, &frame)).unwrap();
        let kept = b.state.fp_claim_prompts.get(&(class_id, producer, claim_id)).expect("the frame keeps its prompt row").clone();
        let horizon = frame.plan_v1().terms.material_until_daa;
        assert!(kept.expires_daa >= horizon && kept.expires_daa % params.epoch_length == 0, "kept to the horizon, epoch-aligned");
        let ids = b.state.fp_claim_prompt_ids_v1(&class_id, &producer, &claim_id).expect("an outsider reads the public input");
        assert_eq!(ids, fx.execution.prompt);
        // The outsider's own replay from those ids reproduces the committed roots.
        let context = fx.execution.binding.job_context.clone();
        let replay = crate::palw_tir_court_v1::test_support::tiny_execution_in(|_, _, _| context.clone(), None);
        assert_eq!(replay.prompt, ids);
        assert_eq!(replay.binding.step_merkle_root, frame.header_v1().step_merkle_root);
        assert_eq!(replay.binding.full_logits_trace_root, frame.header_v1().full_logits_trace_root);
        // A row derived work already wrote with a shorter life is extended, never shortened or rewritten.
        let mut short = prompt_row(&fx, &frame);
        short.expires_daa = params.epoch_length;
        let mut seeded = state.clone();
        seeded.fp_claim_prompts.insert((class_id, producer, claim_id), short.clone());
        let mut b = TransitionBuilder::new(&seeded, &params, false, false, false, false, &extras);
        install_original_v1(&mut b, &at(&frame, 0), frame.clone(), &short).unwrap();
        let extended = b.state.fp_claim_prompts.get(&(class_id, producer, claim_id)).unwrap();
        assert!(extended.expires_daa >= horizon);
        assert_eq!(extended.prompt_token_ids, short.prompt_token_ids);
        // No public ids (or not the job's own): the frame is refused, nothing written.
        for wrong in [Vec::new(), vec![1u32, 6, 4]] {
            let mut row = prompt_row(&fx, &frame);
            row.prompt_tokens = wrong.len() as u32;
            row.prompt_token_ids = wrong;
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            assert!(install_original_v1(&mut b, &at(&frame, 0), frame.clone(), &row).is_err());
        }
    }

    /// **T-N6 (fold half) `g14_tir_seat_finding_reaches_the_filer_queue`** (G-T4). An honest DRAWN seat that signed no Valid owns an
    /// acquisition exactly as an outsider does (Open, Demand), reads the producer's accepted answer back from the rooted chunks, and
    /// its filing of the forged leaf convicts; a credited (liable) seat still cannot own. The node half — the seat's finding entering
    /// the filer book — is `kaspad::palw_tir_public_owner::tests`.
    #[test]
    fn g14_tir_seat_finding_reaches_the_filer_queue() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = credited_frame(&fx);
        let (state, params, extras, open_at) = open_world(&fx, &frame, &[]);
        let base = frame.plan_v1().terms.accepted_daa;
        let claim_id = frame.plan_v1().original_claim;
        let seat = honest_seat();
        assert!(frame.custody_v1().drawn_panel_v1().contains(&seat) && frame.original_custody_v1().credit_v1(&seat).is_none());
        // A colluding (credited) seat is liable: it cannot own.
        assert!(open(&frame, &state, &params, &extras, open_at, colluders()[0]).is_err());
        let opened = open(&frame, &state, &params, &extras, open_at, seat).expect("an uncredited drawn seat owns as an outsider does");
        let target = T::Acquisition { owner: seat, reserved_daa: base + open_at };
        let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: FORGED_LEAF as u64 };
        let demanded = control_step(&frame, &opened, &params, &extras, open_at + 1, seat, target, A::Acquisition(Owned::Demand { request }), 0, 0)
            .expect("the seat demands the leaf its replay disagrees with");
        let (answered, next) = producer_answers(&fx, &frame, &demanded, &params, &extras, target, FORGED_LEAF, open_at + 2);
        assert!(owner_pending(&frame, &answered, target).3.is_some());
        let evidence = cone(&fx);
        palw_tir_public_conviction_preflight_v1(&answered, &params, &extras, base + next, seat, claim_id, &evidence)
            .expect("the seat's finding passes the filer's dry run");
        let convicted = control_step(&frame, &answered, &params, &extras, next, seat, T::Conviction, A::Convict(evidence), frame.conviction_rent_v1(), 0)
            .expect("the seat's filing convicts");
        let (phase, collateral, consumed) = outcome(&convicted, &frame);
        assert!(matches!(phase, PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }), "{phase:?}");
        assert_eq!(collateral[3], COLLATERAL, "the honest filing seat is never debited");
        assert_eq!(consumed, vec![true, true, true, false], "the filing seat is never charged as a signer");
        assert_eq!(convicted.bonds.get(&seat).unwrap().slashed, 0, "the honest seat is never slashed");
    }

    /// **T-N7 `g14_tir_conviction_asserts_each_valid_seat_lock`** (G-T7, G-13 on the TIR side). Every credited Valid seat is
    /// charged on its own native S4 lock: a conviction that would skip a credited seat because its unconsumed lock is gone (or
    /// expired) is refused whole and changes nothing — never a silent skip recorded as a collection. With every lock in place each
    /// is taken (lock first) and each seat debited at least its lock. While the frame's door is open the lock is reserved for the
    /// frame's own conviction (`palw_tir_public_frame_keeps_lock_v1`), so the other lock-dropping paths leave it; after the door,
    /// or for a seat without a credit, it is not reserved.
    #[test]
    fn g14_tir_conviction_asserts_each_valid_seat_lock() {
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = credited_frame(&fx);
        let (state, params, extras) = world(&fx, &frame);
        let claim_id = frame.plan_v1().original_claim;
        let terms = frame.plan_v1().terms;
        let now = terms.accepted_daa + 5;
        for seat in colluders() {
            let lock = state.slashable_locks.get(&(seat, claim_id)).unwrap();
            assert!(lock.expiry_daa >= terms.collect_until_daa, "the lock lasts to the claim's liability horizon (the door)");
            assert!(crate::palw_tir_public_claim_v1::palw_tir_public_frame_keeps_lock_v1(&state, &seat, claim_id, now));
            assert!(!crate::palw_tir_public_claim_v1::palw_tir_public_frame_keeps_lock_v1(&state, &seat, claim_id, terms.collect_until_daa));
        }
        assert!(!crate::palw_tir_public_claim_v1::palw_tir_public_frame_keeps_lock_v1(&state, &honest_seat(), claim_id, now));
        // A credited seat's unconsumed lock is gone: refused, nothing moves.
        let mut lost = state.clone();
        lost.slashable_locks.remove(&(colluders()[1], claim_id));
        let root = lost.state_root();
        let mut b = TransitionBuilder::new(&lost, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).is_err());
        assert_eq!(b.state.state_root(), root);
        // Expired at this block: refused likewise.
        let mut expired = state.clone();
        expired.slashable_locks.get_mut(&(colluders()[0], claim_id)).unwrap().expiry_daa = now;
        let mut b = TransitionBuilder::new(&expired, &params, false, false, false, false, &extras);
        assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).is_err());
        // A seat whose false-Valid offence another route already consumed is not asked again (once across every route).
        let mut consumed = lost.clone();
        let offence = crate::palw_offence_attribution_v1::palw_false_valid_offence_id_v2(&colluders()[1].0, &claim_id);
        let any = state.consumed_offences.values().next().cloned();
        if let Some(record) = any {
            consumed.consumed_offences.insert(offence, record);
            let mut b = TransitionBuilder::new(&consumed, &params, false, false, false, false, &extras);
            assert!(apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).is_ok());
        }
        // Every lock in place: each is taken and each seat debited at least its lock.
        let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
        apply_verified_control_v1(&mut b, &at(&frame, 5), &frame, &filing(&frame, outsider(), cone(&fx))).unwrap();
        for seat in colluders() {
            assert!(!b.state.slashable_locks.contains_key(&(seat, claim_id)));
            assert!(b.state.bonds.get(&seat).unwrap().collateral <= COLLATERAL - LOCK as u64);
        }
    }

    /// **G14 from genesis: the ORIGINAL frame convicts with nothing of the legacy route in the world** — no legacy filer fence
    /// (tag 154 reservations), no legacy dispute, no reservation of any kind, no pre-fence claim: a fresh ruleset whose only PALW
    /// facts are the registered TIR class, the bonds and the frame accepted at its own block. One outside bond opens, demands the
    /// forged leaf, reads the producer's accepted answer back, and its filing convicts and slashes; the producer's attempts are then
    /// refused (`ProducerFrozen` is the funnel's; here: the conviction's record and the void). The frame's doors need none of the
    /// legacy route's state.
    #[test]
    fn g14_tir_frame_convicts_with_the_fence_at_genesis_and_no_legacy_history() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), 4);
        let frame = fx.frame.clone();
        let (state, params, extras, open_at) = open_world(&fx, &frame, &[]);
        for daa in [0, frame.plan_v1().terms.accepted_daa, frame.plan_v1().terms.collect_until_daa] {
            assert!(!params.legacy_public_filer_active_at(daa), "no legacy filer fence at any height");
        }
        assert!(state.legacy_disputes.is_empty(), "no legacy dispute or reservation exists");
        assert!(state.claims.len() == 1, "the frame's own claim is the only claim (no pre-fence history)");
        let base = frame.plan_v1().terms.accepted_daa;
        let claim_id = frame.plan_v1().original_claim;
        let producer = frame.custody_v1().pledge_v1().producer;
        let opened = open(&frame, &state, &params, &extras, open_at, outsider()).unwrap();
        let target = T::Acquisition { owner: outsider(), reserved_daa: base + open_at };
        let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: FORGED_LEAF as u64 };
        let demanded = control_step(&frame, &opened, &params, &extras, open_at + 1, outsider(), target, A::Acquisition(Owned::Demand { request }), 0, 0)
            .unwrap();
        let (answered, next) = producer_answers(&fx, &frame, &demanded, &params, &extras, target, FORGED_LEAF, open_at + 2);
        let before = answered.bonds.get(&producer).unwrap().collateral;
        let convicted = control_step(&frame, &answered, &params, &extras, next, outsider(), T::Conviction, A::Convict(cone(&fx)), frame.conviction_rent_v1(), 0)
            .expect("convicted with no legacy state");
        assert!(matches!(
            convicted.claims.get(&claim_id).unwrap().phase,
            PalwClaimPhaseV2::Voided { reason: PalwVoidReasonV2::CourtFraud, .. }
        ));
        assert!(convicted.bonds.get(&producer).unwrap().collateral < before);
        assert!(convicted.consumed_offences.contains_key(&palw_court_conviction_offence_id_v1(&producer.0, &claim_id)));
        assert!(convicted.legacy_disputes.is_empty(), "the conviction created no legacy-route state either");
    }

    /// **What each accepting frame action really does, measured** (the frame pricing proposal, 2026-10-16). For custodian uppers
    /// 6, 33 and 257 — every seat of the drawn Panel holding an exact credited Valid custody grant, so the rooted frame is at its
    /// largest — the outside owner's Open, Demand and the producer's Begin / Part / Finish answer, the producer's preparation debit
    /// and Ready, and the conviction filing are applied through the verified native fold (`apply_verified_control_v1`), and each is
    /// timed. Beside them: one full-frame pass (`check_structure_v1`, a deep clone, the Borsh encode) at the same size. If an
    /// action's time grows with the frame, it re-reads the whole frame; if it stays flat, it touches only its own data.
    ///
    /// Timings are wall-clock on THIS host and build (recorded, never pinned). Set `MISAKA_TIR_FOLD_MEASURE_OUT=<file>` to write
    /// the canonical JSON record. Counts and byte sizes are deterministic.
    #[test]
    fn tir_frame_accepting_actions_are_measured_across_custodian_uppers() {
        use crate::palw_tir_public_owned_v1::PalwTirPublicOwnedActionV1 as Owned;
        use std::time::Instant;
        const ROUNDS: usize = 5;
        // The median of ROUNDS applications of `control` to `state` (each from the same pre-state).
        let timed = |frame: &Frame,
                     state: &PalwChainStateV2,
                     params: &PalwStateParamsV2,
                     extras: &PalwTransitionExtrasV1,
                     offset: u64,
                     control: &Control|
         -> (u64, PalwChainStateV2) {
            let mut samples = Vec::new();
            let mut last = None;
            for _ in 0..ROUNDS {
                let mut b = TransitionBuilder::new(state, params, false, false, false, false, extras);
                let start = Instant::now();
                apply_verified_control_v1(&mut b, &at(frame, offset), &frame, control).expect("the measured action applies");
                samples.push(start.elapsed().as_nanos() as u64);
                last = Some(b.state.clone());
            }
            samples.sort_unstable();
            (samples[ROUNDS / 2], last.unwrap())
        };
        fn median(f: &mut dyn FnMut() -> u64) -> u64 {
            let mut samples: Vec<u64> = (0..ROUNDS).map(|_| f()).collect();
            samples.sort_unstable();
            samples[ROUNDS / 2]
        }
        let control_of = |frame: &Frame, target: T, action: A, signer: PalwBondKeyV2, rent: u64, turn: u64| {
            let plan = frame.plan_v1();
            Control {
                version: 1,
                claim: plan.original_claim,
                allocation_id: plan.allocation_id,
                source_id: plan.source_id,
                target,
                expected_turn: turn,
                declared_rent_sompi: rent,
                signer,
                action,
                signature: vec![5; SIG],
            }
        };
        let mut rows = Vec::new();
        for upper in [6u64, 33, 257] {
            let fx = palw_tir_public_frame_fixture_v1(Some((FORGED_LEAF, 1)), upper);
            let seats: Vec<PalwBondKeyV2> = (0..upper - 1).map(|i| bond(10_000 + i)).collect();
            let frame = credited_frame_with(&fx, &seats, &seats);
            let frame_bytes = borsh::to_vec(&frame).unwrap().len() as u64;
            // One full-frame pass, by part.
            let check_ns = median(&mut || {
                let start = Instant::now();
                frame.check_structure_v1(&frame.key_v1()).unwrap();
                start.elapsed().as_nanos() as u64
            });
            let clone_ns = median(&mut || {
                let start = Instant::now();
                std::hint::black_box(frame.clone());
                start.elapsed().as_nanos() as u64
            });
            let encode_ns = median(&mut || {
                let start = Instant::now();
                std::hint::black_box(borsh::to_vec(&frame).unwrap());
                start.elapsed().as_nanos() as u64
            });
            // The accepted world: the credited frame installed at its acceptance, the producer prepared and Ready.
            let (mut state, params, extras) = world_with(&fx, &frame, &seats);
            state.staged_courts.remove(&frame.key_v1());
            super::super::palw_staged_court_fold_v1::rebuild_indices(&mut state);
            let mut b = TransitionBuilder::new(&state, &params, false, false, false, false, &extras);
            install_original_v1(&mut b, &at(&frame, 0), frame.clone(), &prompt_row(&fx, &frame)).unwrap();
            let installed = b.state.clone();
            let ready = producer_ready(&frame, &installed, &params, &extras);
            let plan = frame.plan_v1();
            let base = plan.terms.accepted_daa;
            let producer = frame.custody_v1().pledge_v1().producer;
            let open_at = plan.preparation_until_daa - base;
            let rent = owner_terms_v1(&frame).unwrap().plan.rent_sompi;
            let (open_ns, opened) = timed(
                &frame,
                &ready,
                &params,
                &extras,
                open_at,
                &control_of(&frame, T::OwnerOpen { owner: outsider() }, A::Open, outsider(), rent, 0),
            );
            let target = T::Acquisition { owner: outsider(), reserved_daa: base + open_at };
            let request = crate::palw_tir_public_source_v1::PalwTirPublicRequestV1::StepLeaf { index: FORGED_LEAF as u64 };
            let (demand_ns, demanded) = timed(
                &frame,
                &opened,
                &params,
                &extras,
                open_at + 1,
                &control_of(&frame, target, A::Acquisition(Owned::Demand { request }), outsider(), 0, 0),
            );
            let (demand_id, _, turn, _) = owner_pending(&frame, &demanded, target);
            let body = borsh::to_vec(&crate::palw_tir_public_source_v1::PalwTirPublicResponseV1::StepLeaf {
                preimage: fx.execution.preimages[FORGED_LEAF].clone(),
                opening: crate::palw_step_leg::step_opening_v1(&fx.execution.hashes, FORGED_LEAF as u64).unwrap(),
            })
            .unwrap();
            let begin = Owned::Begin {
                demand_id,
                answerer: producer,
                total_bytes: body.len() as u64,
                body_hash: crate::palw_tir_public_owned_v1::palw_tir_public_body_hash_v1(&body),
            };
            let (begin_ns, mut answered) = timed(
                &frame,
                &demanded,
                &params,
                &extras,
                open_at + 2,
                &control_of(&frame, target, A::Acquisition(begin), producer, 0, turn),
            );
            let mut part_ns = Vec::new();
            let mut offset = open_at + 3;
            for (i, bytes) in body.chunks(crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1).enumerate() {
                let part = Owned::Part {
                    demand_id,
                    answerer: producer,
                    offset: (i * crate::palw_tir_public_owned_v1::PALW_TIR_PUBLIC_OWNED_PART_BYTES_V1) as u64,
                    bytes: bytes.to_vec(),
                };
                let (took, next) =
                    timed(&frame, &answered, &params, &extras, offset, &control_of(&frame, target, A::Acquisition(part), producer, 0, turn));
                part_ns.push(took);
                answered = next;
                offset += 1;
            }
            let finish = Owned::Finish { demand_id, answerer: producer };
            let (finish_ns, answered) =
                timed(&frame, &answered, &params, &extras, offset, &control_of(&frame, target, A::Acquisition(finish), producer, 0, turn));
            let filing_control = filing(&frame, outsider(), cone(&fx));
            let filing_bytes = borsh::to_vec(&filing_control).unwrap().len() as u64;
            let (filing_ns, _) = timed(&frame, &answered, &params, &extras, offset + 1, &filing_control);
            // The producer's preparation debit and Ready on the installed world, timed the same way.
            let prep_ns = {
                let mut samples = Vec::new();
                for _ in 0..ROUNDS {
                    let start = Instant::now();
                    std::hint::black_box(producer_ready(&frame, &installed, &params, &extras));
                    samples.push(start.elapsed().as_nanos() as u64);
                }
                samples.sort_unstable();
                samples[ROUNDS / 2]
            };
            let row = serde_json::json!({
                "upper": upper,
                "credited_seats": seats.len(),
                "frame_bytes": frame_bytes,
                "charged_parser_steps_per_owner_action": frame_bytes.saturating_mul(32 * 32),
                "full_frame_pass_ns": { "check_structure": check_ns, "deep_clone": clone_ns, "borsh_encode": encode_ns },
                "action_ns": {
                    "open": open_ns, "demand": demand_ns, "begin": begin_ns, "part": part_ns, "finish": finish_ns,
                    "filing": filing_ns, "preparation_debit_and_ready_incl_state_clone": prep_ns,
                },
                "answer_body_bytes": body.len(),
                "filing_control_bytes": filing_bytes,
            });
            eprintln!("[tir-fold-measure] {row}");
            rows.push(row);
        }
        // One ML-DSA-87 verification (every control's native signature check), timed the same way.
        let kp = libcrux_ml_dsa::ml_dsa_87::generate_key_pair([0x3A; 32]);
        let message = [0x5Au8; 64];
        let signature = libcrux_ml_dsa::ml_dsa_87::sign(&kp.signing_key, &message, b"misaka-measure", [0x11; 32]).unwrap();
        let verify_ns = median(&mut || {
            let start = Instant::now();
            libcrux_ml_dsa::ml_dsa_87::verify(&kp.verification_key, &message, b"misaka-measure", &signature).unwrap();
            start.elapsed().as_nanos() as u64
        });
        // **The cost units these measurements give** (audit §10), at the largest measured frame: the parse rate of one full-frame pass
        // (`check_structure`, at the existing tariff of 32 parser steps a byte), the slowest read action plus one signature check in
        // those steps, and the full-frame passes an Open or a filing costs. Recorded beside the raw times; never pinned here.
        let (smallest, largest) = (rows.first().unwrap(), rows.last().unwrap());
        let frame_bytes = largest["frame_bytes"].as_u64().unwrap();
        let pass_ns = largest["full_frame_pass_ns"]["check_structure"].as_u64().unwrap().max(1);
        let steps_per_ns = (32.0 * frame_bytes as f64) / pass_ns as f64;
        let read = |row: &serde_json::Value| {
            let ns = |k: &str| row["action_ns"][k].as_u64().unwrap();
            ["demand", "begin", "finish"]
                .into_iter()
                .map(ns)
                .chain(row["action_ns"]["part"].as_array().unwrap().iter().map(|v| v.as_u64().unwrap()))
                .max()
                .unwrap()
        };
        let (upper_lo, upper_hi) = (smallest["upper"].as_u64().unwrap(), largest["upper"].as_u64().unwrap());
        let (read_lo, read_hi) = (read(smallest), read(largest));
        // A read action's time fitted as `a + b × upper` through the smallest and largest uppers (b rounded up), plus one signature.
        let per_custodian_ns = (read_hi.saturating_sub(read_lo) as f64 / (upper_hi - upper_lo) as f64).max(0.0);
        let intercept_ns = (read_hi as f64 - per_custodian_ns * upper_hi as f64).max(read_lo as f64 - per_custodian_ns * upper_lo as f64);
        let action_parser_steps = ((intercept_ns + verify_ns as f64) * steps_per_ns).ceil() as u64;
        let action_parser_steps_per_custodian = (per_custodian_ns * steps_per_ns).ceil() as u64;
        let read_ns = read_hi;
        let ns = |k: &str| largest["action_ns"][k].as_u64().unwrap();
        let frame_passes_per_check = ns("open").max(ns("filing")).div_ceil(pass_ns).max(1);
        let record = serde_json::json!({
            "record": "misaka-palw/tir-fold-measure/v1",
            "profile": if cfg!(debug_assertions) { "debug" } else { "release" },
            "host": { "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                      "threads": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
                      "label": std::env::var("MISAKA_MEASURE_HOST").unwrap_or_default() },
            "status": "not arming evidence: wall-clock of this host and build; the tiny court fixture's class",
            "uppers": rows,
            "mldsa87_verify_ns": verify_ns,
            "derived": {
                "parser_steps_per_ns": steps_per_ns,
                "slowest_read_action_ns": read_ns,
                "action_parser_steps": action_parser_steps,
                "action_parser_steps_per_custodian": action_parser_steps_per_custodian,
                "frame_passes_per_check": frame_passes_per_check,
            },
        });
        eprintln!("[tir-fold-measure] derived {}", record["derived"]);
        if let Ok(out) = std::env::var("MISAKA_TIR_FOLD_MEASURE_OUT") {
            std::fs::write(out, serde_json::to_string(&record).unwrap()).unwrap();
        }
    }
}
