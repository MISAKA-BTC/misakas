//! Exact born TIR exporter source. Complete historical carriages are borrowed
//! only inside the authenticated F1 callback, never fabricated as partial worlds.
use super::*;
use crate::palw_claim_local_v1::prefix_v1::{PalwPrefixExportIdentityV1, PalwPrefixExportReservationV1};
use crate::palw_native_resource_v1::NativeDecodeErrorV1;
use crate::palw_staged_court_state_v1::{PalwStagedCourtKeyV1 as Key, PalwStagedCourtRowV1 as Row};
use crate::palw_tir_prefix_export_relation_v2::{self as relation, TirExportTargetV2, TirPrefixExportSpecV2};
fn bad(why: &'static str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(why.into())
}
fn unavailable() -> PalwStateV2Error {
    PalwStateV2Error::NativeContextUnavailable(crate::palw_source_bootstrap_v1::PalwSourceBootstrapContextFaultV1::HistoryUnavailable)
}
fn native_error(e: NativeDecodeErrorV1<String>) -> PalwStateV2Error {
    match e {
        NativeDecodeErrorV1::Resource(e) => PalwStateV2Error::NativeResource(e),
        NativeDecodeErrorV1::Decode(_) => bad("source-export native source identity or geometry differs"),
    }
}
/// Not deserializable; only with_born_export_v2 constructs this view. Complete
/// source/program/header values remain under native leases through the callback.
pub(super) struct BornTirExportViewV2<'a> {
    target: TirExportTargetV2,
    spec: TirPrefixExportSpecV2,
    reservation: &'a PalwPrefixExportReservationV1,
    original: &'a PalwClaimStateV2,
    frame: &'a crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1,
    source: &'a crate::palw_tir_public_source_v1::PalwTirPublicSourceV1,
    space: &'a crate::palw_tir_step_v1::PalwTirStepSpaceV1,
}
impl BornTirExportViewV2<'_> {
    pub(super) fn target(&self) -> TirExportTargetV2 {
        self.target
    }
    pub(super) fn spec(&self) -> TirPrefixExportSpecV2 {
        self.spec
    }
    pub(super) fn reservation(&self) -> &PalwPrefixExportReservationV1 {
        self.reservation
    }
    pub(super) fn original(&self) -> &PalwClaimStateV2 {
        self.original
    }
    pub(super) fn frame(&self) -> &crate::palw_tir_public_claim_v1::PalwTirPublicClaimV1 {
        self.frame
    }
    pub(super) fn source(&self) -> &crate::palw_tir_public_source_v1::PalwTirPublicSourceV1 {
        self.source
    }
    pub(super) fn space(&self) -> &crate::palw_tir_step_v1::PalwTirStepSpaceV1 {
        self.space
    }
}
/// Source view is not a paid-admission capability. The native court owner must
/// additionally match its accepted source-export allocation/fees/material duty.
/// Verified funded native scoped source uses no historical whole-world frame.
/// Legacy cohorts retain their unchanged historical F1 reader below.
/// This callback grants neither paid Open nor source approval/financial status.
pub(super) fn with_born_export_v2<T>(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    identity: &PalwPrefixExportIdentityV1,
    ordinal: u32,
    reservation: &PalwPrefixExportReservationV1,
    read: impl FnOnce(BornTirExportViewV2<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<T, PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(identity.accepted_daa)
        || state.last_point.is_none_or(|tip| identity.accepted_daa > tip.daa_score || identity.accepted_blue_score > tip.blue_score)
    {
        return with_born_export_legacy_v2(state, params, identity, ordinal, reservation, read);
    }
    let read = {
        let mut callback = Some(read);
        // Probe the authenticated native tuple directly. If it was never
        // observed, do not decode any F just to decide to use legacy F below.
        if let Some(view) = super::palw_claim_local_source_admission_v8::for_birth_point_v8(
            state,
            identity.source_claim,
            identity.accepted_block,
            identity.accepted_daa,
            identity.accepted_blue_score,
        )? {
            let point = view.accepting_point(); // actual stored subsidy too
            if state.last_point.is_none_or(|tip| point.daa_score > tip.daa_score || point.blue_score > tip.blue_score) {
                return Err(unavailable());
            }
            // Preserve dormant/pre-fence errors without a new data decoder.
            if !params.claim_local_settlement_active_at(point.daa_score) {
                return with_born_export_legacy_v2(
                    state,
                    params,
                    identity,
                    ordinal,
                    reservation,
                    callback.take().ok_or_else(|| bad("legacy TIR source callback already consumed"))?,
                );
            }
            let funded =
                super::palw_claim_local_born_tir_source_v16::has_verified_original_scope_v17(state, identity.source_claim, point)?;
            #[cfg(test)]
            let scoped = funded
                || super::palw_claim_local_born_tir_source_v16::has_completed_original_scope_v13(state, identity.source_claim, point)?;
            #[cfg(not(test))]
            let scoped = funded;
            if scoped {
                let origin = super::palw_claim_local_source_origin_v8::SourceOriginV8::from_scoped_admission_v13(view);
                let admitted = origin.scoped_admission_v13().ok_or_else(|| bad("scoped TIR source is not native admission"))?;
                let (export, actual) = origin
                    .prefix_export_v1(ordinal)
                    .map_err(|_| bad("scoped TIR source signed export or born key differs"))?
                    .ok_or_else(unavailable)?;
                if actual != *identity {
                    return Err(bad("scoped TIR source changes exact export identity"));
                }
                let result = super::palw_claim_local_born_tir_source_v16::with_born_tir_export_data_v16(
                    state,
                    params,
                    identity,
                    ordinal,
                    reservation,
                    |data| {
                        if data.original() != origin.original_claim()
                            || data.inputs() != admitted.inputs()
                            || data.policy() != admitted.policy()
                            || data.original_params_bytes() != admitted.original_params_bytes()
                            || data.signed_export() != export
                            || data.target().source != actual
                            || data.target().export_ordinal != ordinal
                        {
                            return Err(bad("scoped TIR source archive changes actual admission policy or signed export"));
                        }
                        let reader = callback.take().ok_or_else(|| bad("scoped TIR source callback already consumed"))?;
                        reader(BornTirExportViewV2 {
                            target: data.target(),
                            spec: data.spec(),
                            reservation,
                            original: data.original(),
                            frame: data.frame(),
                            source: data.source(),
                            space: data.space(),
                        })
                    },
                )?;
                return result.ok_or_else(|| bad("selected scoped TIR source became unavailable"));
            }
        }
        callback.take().ok_or_else(|| bad("legacy TIR source callback already consumed"))?
    };
    with_born_export_legacy_v2(state, params, identity, ordinal, reservation, read)
}

fn with_born_export_legacy_v2<T>(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    identity: &PalwPrefixExportIdentityV1,
    ordinal: u32,
    reservation: &PalwPrefixExportReservationV1,
    read: impl FnOnce(BornTirExportViewV2<'_>) -> Result<T, PalwStateV2Error>,
) -> Result<T, PalwStateV2Error> {
    let point = state.claim_local_prefix_source_point_v1(identity)?;
    if reservation.source_claim != identity.source_claim
        || reservation.point != point
        || state.prefix_export_reservation_v1(&point, identity.source_claim)?.as_ref() != Some(reservation)
    {
        return Err(bad("TIR export source changes its actual born reservation"));
    }
    let origin = state.claim_local_admission_context_v1(params, &point, identity.source_claim)?.ok_or_else(unavailable)?;
    let original = origin.original_claim();
    if !matches!(original.source, PalwClaimSourceV2::FreePrompt { .. }) || original.bond != reservation.bond {
        return Err(bad("TIR export source changes its actual producer or namespace"));
    }
    let (export, actual_identity) = origin
        .prefix_export_v1(ordinal)
        .map_err(|_| bad("TIR export signature or accepted source differs"))?
        .ok_or_else(unavailable)?;
    if actual_identity != *identity
        || export.intent.ordinal != ordinal
        || export.intent.liability_until > reservation.liability_until
        || export.intent.material_until > reservation.material_until
    {
        return Err(bad("TIR export source changes its signed ordinal, identity or finite duty"));
    }
    super::palw_claim_local_rewards_v1::frame_read(origin.frame(), |_prior, current| {
        let frame = match current.staged_courts.get(&Key::TirClaim(identity.source_claim)) {
            Some(Row::TirClaim(frame)) => frame.as_ref(),
            _ => return Err(unavailable()),
        };
        frame
            .check_structure_v1(&Key::TirClaim(identity.source_claim))
            .map_err(|_| bad("TIR export historical original frame is inconsistent"))?;
        let job = frame.original_v1();
        if crate::palw_freeprompt_v3::fp_claim_id_v3(job) != identity.source_claim
            || job.job.class_id != original.class_id
            || job.job.executor_bond != original.bond.0
            || job.execution_root != original.execution_root
            || job.trace_root != original.trace_root
            || frame.header_v1().committed_execution_root != original.execution_root
            || frame.plan_v1().original_claim != identity.source_claim
        {
            return Err(bad("TIR export source substitutes its original native job"));
        }
        let record = current.tir_classes.get(&original.class_id).ok_or_else(unavailable)?;
        let layout = match current.staged_courts.get(&Key::TirLayout(original.class_id)) {
            Some(Row::TirLayout(layout)) => layout.as_ref(),
            _ => return Err(unavailable()),
        };
        // The actual original frame freezes the rule; current/new raw-ID rows
        // and a caller's declared leaf ceiling never substitute for it.
        let maximum = frame.descriptor_v1().maximum_leaves;
        let source = frame.source_identity_native_v2(record, layout, maximum).map_err(native_error)?;
        // Its opaque admission/export already authenticates the born job key.
        // A later registered bond-key rotation cannot change that authority.
        let prepared = relation::prepare_export_source_v2(job, &source, record, layout, &job.job.executor_pubkey, maximum)
            .map_err(native_error)?;
        let spec = prepared.prepare_export_v2(export).map_err(native_error)?;
        let target = TirExportTargetV2 {
            version: 2,
            source: actual_identity,
            export_ordinal: ordinal,
            prefix: export.intent.prefix,
            source_execution_root: original.execution_root,
            liability_until: export.intent.liability_until,
            material_until: export.intent.material_until,
        };
        read(BornTirExportViewV2 { target, spec, reservation, original, frame, source: &source, space: prepared.space_v2() })
    })
}

/// The dispatcher passes the row fetched from its NEW born-export native key.
/// A wire body cannot replace this lookup. Shared registration owns that key,
/// all-branch paid Open, collection clocks and native row resource admission.
pub(super) fn check_rooted_refutation_v2(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    now: &PalwBlockContextV2,
    accused: PalwBondKeyV2,
    reference: &crate::palw_tir_prefix_export_owned_v2::TirExportRefutationV2,
    rooted: &crate::palw_tir_prefix_export_owned_v2::OwnedTirExportCourtV2,
) -> Result<(PalwPrefixExportReservationV1, PalwPrefixExportIdentityV1), PalwStateV2Error> {
    if !params.claim_local_settlement_active_at(now.daa_score) || !params.rcore_plus_active_at(now.daa_score) {
        return Err(PalwStateV2Error::ObjectiveOffenceDormant);
    }
    let outcome = reference
        .resolve_rooted(rooted, accused, now.daa_score, now.blue_score)
        .map_err(|_| bad("source-export reference is not its actual rooted finite court"))?
        .ok_or_else(|| bad("source-export complete boundary agrees with the producer declaration"))?;
    if outcome.derived == outcome.target.prefix.state_root {
        return Err(bad("source-export mismatch outcome does not disagree"));
    }
    let identity = outcome.target.source;
    let point = state.claim_local_prefix_source_point_v1(&identity)?;
    let reservation = state.prefix_export_reservation_v1(&point, identity.source_claim)?.ok_or_else(unavailable)?;
    with_born_export_v2(state, params, &identity, outcome.target.export_ordinal, &reservation, |born| {
        if born.target() != outcome.target
            || born.original().bond != accused
            || rooted.seed().export_budget != born.reservation().budget_id
            || rooted.seed().source_allocation != born.frame().plan_v1().allocation_id
        {
            return Err(bad("source-export rooted outcome substitutes original allocation or born subject"));
        }
        Ok(())
    })?;
    Ok((reservation, identity))
}
