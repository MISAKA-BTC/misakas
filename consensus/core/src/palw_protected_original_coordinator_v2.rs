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
    original: &crate::palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    source: &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    admission: &crate::palw_positive_contract_v2::PublicOriginalAdmissionV1<'_>,
    reservation: &crate::palw_positive_contract_v2::PublicOriginalReservationV1<'_>,
    prompt_form: crate::palw_prompt_ids_v1::PalwPromptIdsFormV1,
    projection: &crate::palw_court_shared_material_intake_v1::batch::plain_output::projection::PlainBatchOriginalProjectionV2,
    terms: &crate::palw_positive_contract_v2::tfp1_original::preclaim::ProtectedBatchPreclaimTermsV1,
    policy: crate::palw_positive_contract_v2::tfp1_original::funding::Tfp1FundingPolicyV1,
    maximum_leaves: u64,
    maximum_packets: usize,
    price: impl FnMut(
        crate::palw_court_shared_material_intake_v1::batch::admission::BatchDeliveryV1,
        u64,
    ) -> Result<crate::palw_court_service_v1::ServiceCost, String>,
) -> Result<OriginalAdmissionV1, PalwStateV2Error> {
    use crate::palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1};
    use crate::palw_positive_contract_v2::{
        PublicTfp1FrameV1,
        tfp1_original::{
            funding::admission::Tfp1ProtectedFundingAdmissionErrorV1,
            service_booking::{PreparedProtectedServiceBookingV1, ProtectedServiceBookingErrorV1},
        },
    };

    // This is a native-derived identity, rechecked against the literal FP by
    // the material/funding entrance below. Avoid an unleased FP re-encoding.
    let claim = terms.obligation_v1().facts_v1().native_identity_v1().0;
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
    if accepted.origin_v1() != reservation.origin || pairing.public_companion_v3().is_none() {
        return Err(refused("protected original literal carrier differs from native acceptance"));
    }
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
            reservation,
            projection,
            terms,
            &mut candidate_book,
            maximum_packets,
            price,
            policy,
        )
        .map_err(|error| match error {
            Tfp1ProtectedFundingAdmissionErrorV1::LocalResource(coverage) => PalwStateV2Error::NativeResource(coverage),
            other => refused(format!("protected original material/reservation/funding refused: {other:?}")),
        })?;
    let frame = PublicTfp1FrameV1::prepare_with_native_service_v3(&prepared, original, source, class, ctx, maximum_leaves, &booking)
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
        admission,
        reservation,
    )?;
    Ok(OriginalAdmissionV1::Protected(installation))
}
