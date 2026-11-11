// Protected-original native composition. The public dispatcher must first
// resolve a complete adopted profile; this function is not an activation
// switch and is not exposed as a peer-controlled ingress.

/// Compose the actual material/funding entrance with the existing native
/// installation owner. All inputs are resolved by the native original planner.
/// The service book starts from this exact ordered prefix, while immutable
/// class/tokenizer/HF identities come from the authenticated original parent.
/// Neither a quote's book nor a preselected reservation ID is accepted here.
///
/// No consensus row is written. The caller runs the remaining common FP
/// checks, then consumes the returned owner at install_original_admission_v1
/// inside the same paired checkpoint. Complete profile resolution, late
/// protected requests and Final gating remain prerequisites for activation.
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_protected_original_candidate_v2(
    b: &mut TransitionBuilder<'_>,
    ctx: &PalwBlockContextV2,
    claim: Hash64,
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    header: &crate::palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    policy: &crate::palw_positive_contract_v2::tfp1_original::native_plan::Tfp1NativePlanningPolicyV3<'_>,
    mut price: impl FnMut(
        crate::palw_court_shared_material_intake_v1::batch::admission::BatchDeliveryV1,
        u64,
    ) -> Result<crate::palw_court_service_v1::ServiceCost, String>,
) -> Result<OriginalAdmissionV1, PalwStateV2Error> {
    use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
    use crate::palw_positive_contract_v2::{
        PublicTfp1FrameV1,
        tfp1_original::{
            funding::admission::Tfp1ProtectedFundingAdmissionErrorV1,
            native_plan::NativeTfp1OriginalPlanV3,
            service_booking::{PreparedProtectedServiceBookingV1, ProtectedServiceBookingErrorV1},
        },
    };

    // The object dispatcher supplies its actual claim key. Native planning
    // rederives it from the literal FP before any candidate can be installed.
    if b.paired_original_checkpoint_claim != Some(claim) || b.tfp1_original_prefix.is_some() || b.state.claim(&claim).is_some() {
        return Err(refused("protected original planning requires its fresh paired checkpoint"));
    }
    let parent = b.claim_local_original_parent_v19.ok_or(PalwStateV2Error::NativeContextUnavailable(
        crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
    ))?;
    // Borrow external native inputs independently of the builder. No pairing
    // clone, invented AcceptedOrigin or unbounded literal copy is needed.
    let extras = b.extras;
    let pairing = extras.tir_funded_originals.get(&claim).ok_or_else(|| refused("protected original has no actual funded carrier"))?;
    let accepted = pairing.history_inputs_v1().ok_or_else(|| refused("protected original has no accepted UTXO handoff"))?;
    if parent.last_point().map(|point| point.block) != Some(accepted.parent_v1())
        || b.tfp1_history_journal_parent != Some(accepted.parent_v1())
    {
        // These are native execution-context facts, not a producer offence.
        return Err(PalwStateV2Error::NativeContextUnavailable(
            crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable,
        ));
    }
    if !pairing.envelope().is_protected_v2() || pairing.public_companion_v3().is_none() {
        return Err(refused("protected original literal carrier differs from native acceptance"));
    }
    let prompt = pairing.public_prompt_bytes_v3().ok_or_else(|| refused("protected original has no literal prompt"))?;
    let descriptor = pairing.public_hf_descriptor_bytes_v2().ok_or_else(|| refused("protected original has no HF reference"))?;
    let plan = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        extras,
        b.params,
        ctx.daa_score,
        ctx.blue_score,
        original,
        header,
        prompt_form,
        prompt,
        descriptor,
        policy,
        &mut price,
    )
    .map_err(protected_native_plan_error_v3)?;
    if plan.terms_v3().obligation_v1().facts_v1().native_identity_v1().0 != claim {
        return Err(refused("protected original plan changes the actual object claim"));
    }
    let admission = plan.admission_v3(original, prompt).map_err(protected_native_plan_error_v3)?;
    let reservation = plan.actual_v3(accepted.origin_v1()).map_err(protected_native_plan_error_v3)?;
    let source = plan.source_v3();
    let class = parent
        .tir_class_v1(&original.job.class_id)
        .ok_or_else(|| refused("protected original class is absent from its accepted parent"))?;
    let prior_book =
        &b.state.court_service_state_v1().ok_or_else(|| refused("protected original has no native service namespace"))?.book;
    let mut candidate_book = NativeRetainedV1::try_from_clone_v1(prior_book).map_err(PalwStateV2Error::NativeResource)?;
    let (prepared, booking, funding) = admission
        .prepare_tfp1_material_funding_v3(
            parent,
            extras,
            ctx,
            prompt_form,
            original,
            source,
            pairing,
            &reservation,
            plan.projection_v3(),
            plan.terms_v3(),
            &mut candidate_book,
            policy.maximum_packets,
            &mut price,
            policy.funding,
        )
        .map_err(|error| match error {
            Tfp1ProtectedFundingAdmissionErrorV1::LocalResource(coverage) => PalwStateV2Error::NativeResource(coverage),
            other => refused(format!("protected original material/reservation/funding refused: {other:?}")),
        })?;
    let frame =
        PublicTfp1FrameV1::prepare_with_native_service_v3(&prepared, original, source, class, ctx, policy.maximum_leaves, &booking)
            .map_err(|error| match error {
                NativeDecodeErrorV1::Resource(coverage) => PalwStateV2Error::NativeResource(coverage),
                NativeDecodeErrorV1::Decode(other) => refused(format!("protected original frame refused: {other:?}")),
            })?;
    let service =
        PreparedProtectedServiceBookingV1::prepare_v1(&b.state, &frame, &booking, candidate_book).map_err(|error| match error {
            ProtectedServiceBookingErrorV1::LocalResource(coverage) => PalwStateV2Error::NativeResource(coverage),
            other => refused(format!("protected original service publication refused: {other:?}")),
        })?;
    let installation = PreparedProtectedOriginalInstallationV1::prepare_in_checkpoint_v2(
        b,
        ctx,
        frame,
        service,
        &funding,
        pairing,
        &admission,
        &reservation,
    )?;
    Ok(OriginalAdmissionV1::Protected(installation))
}

fn protected_native_plan_error_v3(error: crate::palw_native_resource_v1::NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    match error {
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(coverage) => PalwStateV2Error::NativeResource(coverage),
        crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why) => refused(why),
    }
}
