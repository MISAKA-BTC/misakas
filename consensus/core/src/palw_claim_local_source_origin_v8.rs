//! Borrowed historical source metadata, backed by either genuine scoped
//! admission or unchanged legacy F1. Never a projected fullframe/fullstate.
use super::palw_claim_local_source_admission_v8::{self as archive, SourceAdmissionViewV8};
use super::*;
use crate::palw_claim_local_v1::prefix_v1::{
    PalwPrefixDependencyRefusalV1 as R, PalwPrefixExportIdentityV1, PalwSignedPrefixExportV1, PalwSignedPrefixUseV1,
};
use crate::palw_claim_local_v1::{PalwClaimLocalAdmissionContextV1, PalwClaimLocalInputV1};
enum Kind {
    Scoped { id: Hash64, view: SourceAdmissionViewV8 },
    Legacy(PalwClaimLocalAdmissionContextV1),
}
pub(super) struct SourceOriginV8 {
    inner: Kind,
}
impl SourceOriginV8 {
    /// Exact immutable legacy admission only; never replace it with a scoped
    /// Source8 DTO or a current-world projection.
    pub(super) fn native_legacy_at_point_v19(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        id: Hash64,
        point: &PalwBlockContextV2,
    ) -> Result<Option<Self>, PalwStateV2Error> {
        if let Some(v) = s.claim_local_admission_context_v1(p, point, id)? {
            return Ok(Some(Self { inner: Kind::Legacy(v) }));
        }
        let now = s.last_point.ok_or_else(|| bad_legacy("frozen legacy source lacks native tip"))?.daa_score;
        Ok(super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::with_verified_ancestor_descriptor_v19(
            s,
            id,
            point.block,
            point.daa_score,
            point.blue_score,
            now,
            |d| {
                if d.point_v19() != *point {
                    return Err(bad_legacy("leased legacy source exact point differs"));
                }
                d.with_native_legacy_origin_v19(s, p)
            },
        )?
        .flatten())
    }
    pub(super) fn from_native_retained_legacy_frame_v19(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        id: Hash64,
        point: PalwBlockContextV2,
        frame: crate::palw_native_resource_v1::NativeRetainedV1<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>,
    ) -> Result<Option<Self>, PalwStateV2Error> {
        if !p.claim_local_settlement_active_at(point.daa_score)
            || s.last_point.is_none_or(|tip| point.blue_score > tip.blue_score || point.daa_score > tip.daa_score)
        {
            return Err(bad_legacy("retained legacy origin outside actual native fence/tip"));
        }
        Ok(super::palw_claim_local_fold_v1::admission_from_native_retained_frame_v19(frame, &point, id)?
            .map(|v| Self { inner: Kind::Legacy(v) }))
    }
    pub(super) fn native_legacy_frame_key_v19(&self) -> Option<[u8; 65]> {
        match &self.inner {
            Kind::Legacy(v) => Some(crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(v.accepting_point().block)),
            Kind::Scoped { .. } => None,
        }
    }

    /// Native opaque view only: its claim identity comes from the authenticated
    /// Source8 record, never a supplied raw slot or an invented historical state.
    pub(super) fn from_scoped_admission_v13(view: SourceAdmissionViewV8) -> Self {
        let id = view.archive_quote_claim_id_v12();
        Self { inner: Kind::Scoped { id, view } }
    }
    pub(super) fn scoped_admission_v13(&self) -> Option<&SourceAdmissionViewV8> {
        match &self.inner {
            Kind::Scoped { view, .. } => Some(view),
            Kind::Legacy(_) => None,
        }
    }
    pub(super) fn for_claim(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        id: Hash64,
        c: &PalwClaimStateV2,
    ) -> Result<Option<Self>, PalwStateV2Error> {
        if let Some(view) = archive::for_stored_claim_v8(s, id, c)? {
            return Ok(Some(Self { inner: Kind::Scoped { id, view } }));
        }
        if let Some(v) = super::palw_claim_local_rewards_v1::origin_for(s, p, id, c)? {
            return Ok(Some(Self { inner: Kind::Legacy(v) }));
        }
        let now = s.last_point.ok_or_else(|| bad_legacy("leased source lacks native tip"))?.daa_score;
        Ok(super::palw_tir_public_fold_v1::original_archive_reservation_proposal_v1::with_verified_ancestor_descriptor_v19(
            s,
            id,
            c.accepted_block,
            c.accepted_daa,
            c.accepted_blue_score,
            now,
            |d| d.with_native_legacy_origin_v19(s, p),
        )?
        .flatten())
    }
    pub(super) fn at_point(
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        id: Hash64,
        point: &PalwBlockContextV2,
    ) -> Result<Option<Self>, PalwStateV2Error> {
        if let Some(view) = archive::for_accepting_point_v8(s, id, *point)? {
            return Ok(Some(Self { inner: Kind::Scoped { id, view } }));
        }
        Self::native_legacy_at_point_v19(s, p, id, point)
    }
    pub(super) fn claim_id(&self) -> Hash64 {
        match &self.inner {
            Kind::Scoped { id, .. } => *id,
            Kind::Legacy(v) => v.claim_id(),
        }
    }
    pub(super) fn accepting_point(&self) -> PalwBlockContextV2 {
        match &self.inner {
            Kind::Scoped { view, .. } => view.accepting_point(),
            Kind::Legacy(v) => v.accepting_point(),
        }
    }
    pub(super) fn original_claim(&self) -> &PalwClaimStateV2 {
        match &self.inner {
            Kind::Scoped { view, .. } => view.original_claim(),
            Kind::Legacy(v) => v.original_claim(),
        }
    }
    pub(super) fn inputs(&self) -> &[PalwClaimLocalInputV1] {
        match &self.inner {
            Kind::Scoped { view, .. } => view.inputs(),
            Kind::Legacy(v) => &v.frame().inputs,
        }
    }
    pub(super) fn prefix_use(&self) -> Result<Option<&PalwSignedPrefixUseV1>, PalwStateV2Error> {
        match &self.inner {
            Kind::Scoped { view, .. } => view.prefix_use().map_err(super::palw_claim_local_paid_archive_v2::state_error),
            Kind::Legacy(v) => legacy_prefix_use(v),
        }
    }
    pub(super) fn prefix_export_v1(&self, ordinal: u32) -> Result<Option<(&PalwSignedPrefixExportV1, PalwPrefixExportIdentityV1)>, R> {
        let born = self.original_claim();
        let point = self.accepting_point();
        if !matches!(born.source, crate::palw_state_v2::PalwClaimSourceV2::FreePrompt { .. })
            || born.accepted_block != point.block
            || born.accepted_daa != point.daa_score
            || born.accepted_blue_score != point.blue_score
        {
            return Err(R::Identity);
        }
        let mut envelopes = self.inputs().iter().filter_map(|input| match input {
            PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == self.claim_id() => Some(envelope.as_ref()),
            _ => None,
        });
        let Some(envelope) = envelopes.next() else {
            return Ok(None);
        };
        if envelopes.next().is_some() {
            return Err(R::Identity);
        }
        envelope.check_v1()?;
        let Some(export) = envelope.exports.iter().find(|e| e.intent.ordinal == ordinal) else {
            return Ok(None);
        };
        let mut jobs = self.inputs().iter().filter_map(|input| match input {
            PalwClaimLocalInputV1::FreePromptNative { claim, public_job: Some(job), .. } if *claim == self.claim_id() => {
                Some(job.as_ref())
            }
            _ => None,
        });
        let Some(job) = jobs.next() else {
            return Err(R::Identity);
        };
        if jobs.next().is_some()
            || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != self.claim_id()
            || export.intent.base_commitment != self.claim_id()
            || export.intent.network != job.job.network_domain
            || export.intent.prefix.class_id != self.original_claim().class_id
        {
            return Err(R::Identity);
        }
        if export.intent.material_until > self.original_claim().trace_retention_daa
            || export.intent.liability_until < self.accepting_point().daa_score
        {
            return Err(R::Retention);
        }
        export.intent.verify_signature_v1(&job.job.executor_pubkey, &export.signature)?;
        let point = self.accepting_point();
        Ok(Some((
            export,
            PalwPrefixExportIdentityV1 {
                intent: export.intent.id_v1(),
                source_claim: self.claim_id(),
                accepted_block: point.block,
                accepted_daa: point.daa_score,
                accepted_blue_score: point.blue_score,
            },
        )))
    }
}
fn legacy_prefix_use(origin: &PalwClaimLocalAdmissionContextV1) -> Result<Option<&PalwSignedPrefixUseV1>, PalwStateV2Error> {
    let id = origin.claim_id();
    let mut inputs = origin.frame().inputs.iter().filter_map(|input| match input {
        PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == id => Some(envelope),
        _ => None,
    });
    let Some(envelope) = inputs.next() else {
        return Ok(None);
    };
    if inputs.next().is_some() {
        return Err(bad_legacy("duplicate prefix dependency birth metadata"));
    }
    envelope.check_v1().map_err(|_| bad_legacy("invalid prefix birth envelope"))?;
    let Some(consumed) = &envelope.consumed else {
        return Ok(None);
    };
    // A non-genesis prefix whose producer cannot be authenticated stays held.
    // Missing birth material is not an independent input and not a fraud verdict.
    let Some(descriptor) = origin.frame().accepted_objects.iter().find_map(|object| match object {
        PalwConsensusObjectV2::FreePromptCommitted { claim, consumed_prefix_state, .. } if *claim == id => Some(consumed_prefix_state),
        _ => None,
    }) else {
        return Ok(None);
    };
    // public_job is intentionally skipped by the released object wire codec;
    // its authenticated native birth material has its own retained input tag.
    let Some(job) = origin.frame().inputs.iter().find_map(|input| match input {
        PalwClaimLocalInputV1::FreePromptNative { claim, public_job: Some(job), .. } if *claim == id => Some(job.as_ref()),
        _ => None,
    }) else {
        return Ok(None);
    };
    if consumed.intent.base_commitment != id
        || consumed.intent.network != job.job.network_domain
        || consumed.intent.prefix != *descriptor
        || crate::palw_freeprompt_v3::fp_claim_id_v3(job) != id
        || consumed.intent.verify_signature_v1(&job.job.executor_pubkey, &consumed.signature).is_err()
    {
        return Ok(None);
    }
    Ok(Some(consumed))
}
fn bad_legacy(s: &str) -> PalwStateV2Error {
    PalwStateV2Error::CarriageInconsistent(s.into())
}

// Narrow compatibility dispatch: only typed facts leave this method. A scoped
// original never masquerades as a fullframe; fallback reads an authentic exact
// legacy consumer admission, or reports missing provenance.
impl SourceOriginV8 {
    pub(super) fn visit_legacy_carrier_facts_v9(
        &self,
        s: &PalwChainStateV2,
        p: &PalwStateParamsV2,
        visit: impl FnMut(
            &super::palw_claim_local_carrier_lineage_v9::CarrierFactV9,
            super::palw_claim_local_carrier_lineage_v9::CarrierSourceViewV9,
        ) -> Result<(), PalwStateV2Error>,
    ) -> Result<bool, PalwStateV2Error> {
        match &self.inner {
            Kind::Legacy(v) => super::palw_claim_local_carrier_lineage_v9::visit_actual_legacy_context_v9(s, p, v, visit),
            Kind::Scoped { .. } => {
                let Some(v) = s.claim_local_admission_context_v1(p, &self.accepting_point(), self.claim_id())? else {
                    return Ok(false);
                };
                if v.original_claim() != self.original_claim() {
                    return Err(bad_legacy("legacy carrier fallback differs from actual scoped consumer"));
                }
                let own = |x: &&PalwClaimLocalInputV1| matches!(x,PalwClaimLocalInputV1::ConsumedCarrier{consumer,..}if *consumer==self.claim_id());
                if !v.frame().inputs.iter().filter(own).eq(self.inputs().iter().filter(own)) {
                    return Err(bad_legacy("legacy carrier inventory differs from actual scoped consumer"));
                }
                super::palw_claim_local_carrier_lineage_v9::visit_actual_legacy_context_v9(s, p, &v, visit)
            }
        }
    }
}

/// Complete retained prefix-consumer origins. Scoped cohorts are visited once
/// from their actual directory, including retired births. Legacy F1 contributes
/// only its opaque ACTUAL admission at that same point and never aliases a live
/// claim with the same raw ID. A scoped + legacy copy contributes one origin.
pub(super) fn visit_retained_prefix_origins_v8(
    state: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    mut visit: impl FnMut(&SourceOriginV8) -> Result<(), PalwStateV2Error>,
) -> Result<(), PalwStateV2Error> {
    archive::visit_retained_admissions_v8(state, |id, view| visit(&SourceOriginV8 { inner: Kind::Scoped { id, view } }))?;
    for (key, raw) in super::palw_claim_local_rewards_v1::frames(state) {
        let frame = super::palw_claim_local_rewards_v1::decoded::<crate::palw_claim_local_v1::PalwClaimLiabilityFrameV1>(raw)?;
        // The global context validator also enforces this. Check locally before
        // canonical opaque lookup or scoped-copy dedup so an unvalidated alias
        // cannot cause the same actual legacy birth to be visited twice.
        if key.as_slice() != crate::palw_claim_local_v1::palw_claim_liability_frame_key_v1(frame.point.block) {
            return Err(bad_legacy("retained prefix frame key differs from its actual accepting block"));
        }
        // An authentic A8 copy permits deduplication, never bypasses checks
        // on a still-retained legacy frame. Preserve both carriage and native
        // journal validation before skipping its duplicated prefix origins.
        if frame.inputs.iter().any(|input| matches!(input, PalwClaimLocalInputV1::PrefixDependency { .. })) {
            super::palw_claim_local_rewards_v1::frame_read(&frame, |_, _| Ok(()))?;
        }
        for input in &frame.inputs {
            let PalwClaimLocalInputV1::PrefixDependency { claim, .. } = input else { continue };
            if archive::retained_accepting_point_present_v8(state, *claim, frame.point)? {
                continue;
            }
            let Some(origin) = state.claim_local_admission_context_v1(params, &frame.point, *claim)? else {
                continue;
            };
            visit(&SourceOriginV8 { inner: Kind::Legacy(origin) })?;
        }
    }
    Ok(())
}

impl PalwChainStateV2 {
    /// Normal consumed-source admission needs only actual signed metadata/E,
    /// not a TIR model/source frame. All borrowed values stay under the opaque
    /// scoped/legacy origin leases until this check finishes; nothing is cloned.
    pub(crate) fn check_prefix_consumed_source_v8(
        &self,
        params: &PalwStateParamsV2,
        now: &PalwBlockContextV2,
        consumed: &PalwSignedPrefixUseV1,
        consumer_network: Hash64,
        consumer_claim: Hash64,
    ) -> Result<(), PalwStateV2Error> {
        let bad = |why: R| PalwStateV2Error::CarriageInconsistent(format!("RFC0040: {why}"));
        let source = consumed.intent.source;
        let point = self.claim_local_prefix_source_point_v1(&source)?;
        // Preserve the old opaque legacy admission's source-fence refusal.
        if !params.claim_local_settlement_active_at(point.daa_score) {
            return Err(bad(R::Identity));
        }
        let origin = SourceOriginV8::at_point(self, params, source.source_claim, &point)?.ok_or_else(|| bad(R::Identity))?;
        // Full intent, never matching state root/class or a current raw claim.
        let ordinal = origin
            .inputs()
            .iter()
            .filter_map(|input| match input {
                PalwClaimLocalInputV1::PrefixDependency { claim, envelope } if *claim == source.source_claim => Some(envelope),
                _ => None,
            })
            .flat_map(|envelope| envelope.exports.iter())
            .find(|e| e.intent.id_v1() == source.intent)
            .map(|e| e.intent.ordinal)
            .ok_or_else(|| bad(R::Identity))?;
        let (export, actual) = origin.prefix_export_v1(ordinal).map_err(bad)?.ok_or_else(|| bad(R::Identity))?;
        let reservation = self.prefix_export_reservation_v1(&point, source.source_claim)?.ok_or_else(|| bad(R::Retention))?;
        if reservation.convicted_daa.is_some()
            || reservation.liability_until < export.intent.liability_until
            || reservation.material_until < export.intent.material_until
        {
            return Err(bad(R::Retention));
        }
        consumed
            .intent
            .check_source_v1(&export.intent, &actual, consumer_network, consumer_claim, now.daa_score, now.blue_score)
            .map_err(bad)
    }
}

impl SourceOriginV8 {
    /// Native complete legacy frame only, including a genuine retained Source19
    /// descriptor when the old F key was legally collected. Never a Source8 DTO
    /// or a current-world projection; the old canonical F1 codec stays unchanged.
    pub(super) fn visit_native_legacy_material_v24(
        &self,
        mut visit: impl FnMut(&[u8]) -> Result<(), PalwStateV2Error>,
    ) -> Result<bool, PalwStateV2Error> {
        let Kind::Legacy(origin) = &self.inner else { return Ok(false) };
        let frame = origin.frame();
        let n = borsh::object_length(frame).map_err(|_| bad_legacy("native legacy material length"))?;
        let mut q = crate::palw_native_resource_v1::TirNativeRequestsV1::new();
        q.exact_v1::<u8>(n as u64).map_err(PalwStateV2Error::NativeResource)?;
        let _lease =
            crate::palw_native_resource_v1::reserve_native_requests_v1(q.bytes_v1()).map_err(PalwStateV2Error::NativeResource)?;
        let mut raw = Vec::with_capacity(n);
        borsh::BorshSerialize::serialize(frame, &mut raw).map_err(|_| bad_legacy("native legacy material encoding"))?;
        visit(&raw)?;
        Ok(true)
    }
}
