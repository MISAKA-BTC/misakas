//! Native pre-signing construction order. Source and relative obligations
//! exist before a companion/manifest ID; physical booking comes afterwards.
//! These explicit policy inputs are not a public preset or an activation flag.

use super::{
    Tfp1PublicAdmissionPolicyV1,
    funding::{
        Tfp1FundingPolicyV1,
        admission::{ProtectedFundingQuoteV2, Tfp1ProtectedFundingAdmissionErrorV1},
    },
    initial_supply::{InitialSupplyErrorV1, PublicTfp1InitialSupplyV1, reserve_identity_writers_v3},
    preclaim::{
        ProtectedBatchPreclaimTermsV1,
        initial_request::InitialMaterialRequestPlanV1,
        original_duties::{ProtectedEarlyDutyPolicyV1, ProtectedOriginalDutiesV1},
    },
};
use crate::{
    Hash64,
    palw_batch_court_v1::BatchCourtErrorV1,
    palw_court_service_v1::{ServiceCost, ServiceReservations},
    palw_court_shared_material_intake_v1::{
        MaterialIntakeScheduleV1,
        batch::{
            admission::{
                BatchDeliveryV1, NativeBatchDeliveryCostsV1, plain_selection::PlainSelectionBudgetV3,
                plain_tokenizer::PlainTokenizerBatchBudgetV2,
            },
            plain_output::projection::PlainBatchOriginalProjectionV2,
        },
    },
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_native_resource_v1::{NativeDecodeErrorV1, NativeRetainedV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::{
        PublicOriginalAdmissionV1, PublicOriginalProspectiveReservationV2, PublicOriginalReservationV1, PublicTfp1HfArtifactV1,
    },
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_public_source_v1::{PalwTirPublicSourceV1, parameter_cover_v1::StreamingParameterCoverBudgetV1},
};
use misaka_palw_positive_contract::{
    identity::Digest,
    messages::AggregateBudget,
    public_one_honest::{
        FirstNoticeDemand, HfArtifactDescriptorV1, ManifestEntryV1, ManifestV1, MaterialCaseDemand, MaterialScope, OriginalClock,
        OriginalCommit, ProtectedRescueDemand, VerificationSchedule,
    },
    reservations::RemainingCapacity,
    rows::{AcceptedOrigin, AggregateUsage},
};

type Error = NativeDecodeErrorV1<String>;

#[cfg(test)]
pub(crate) mod tests;

/// Native resolver inputs, deliberately without a Default, wire decoder,
/// active flag or shipped values. A complete public resolver must also own
/// late requests/repair/settlement and derive funding.other_relative_graph.
/// This partial assembly must never advertise that omitted duties are booked.
pub(crate) struct Tfp1NativePlanningPolicyV3<'a> {
    pub admission: Tfp1PublicAdmissionPolicyV1<'a>,
    pub maximum_leaves: u64,
    pub cover: StreamingParameterCoverBudgetV1,
    pub maximum_ranges: usize,
    pub output_verification: ServiceCost,
    pub output_verification_daa: u64,
    pub selection: PlainSelectionBudgetV3,
    pub service_profile: Hash64,
    pub delivery: NativeBatchDeliveryCostsV1,
    pub tokenizer: PlainTokenizerBatchBudgetV2,
    pub storage: MaterialIntakeScheduleV1,
    pub early: ProtectedEarlyDutyPolicyV1,
    pub request_service: ServiceCost,
    pub request_retained_bytes: u64,
    pub notice: FirstNoticeDemand,
    pub case: MaterialCaseDemand,
    pub initial_supply: RemainingCapacity,
    pub rescue: ProtectedRescueDemand,
    pub budget: AggregateBudget,
    pub usage: AggregateUsage,
    pub funding: Tfp1FundingPolicyV1,
    pub maximum_packets: usize,
}

/// Own the one native Source and the class used for material authentication.
/// No signature, AcceptedOrigin, reservation slot or escrow is manufactured.
pub(crate) struct NativeTfp1OriginalPlanV3<'a> {
    pub(super) projection: PlainBatchOriginalProjectionV2,
    class: NativeRetainedV1<PalwTirClassV1>,
    pub(super) terms: ProtectedBatchPreclaimTermsV1,
    policy: &'a Tfp1NativePlanningPolicyV3<'a>,
    clocks: (u64, u64),
    supply: PublicTfp1InitialSupplyV1,
    parent: &'a PalwChainStateV2,
    extras: &'a PalwTransitionExtrasV1,
    prompt_form: PalwPromptIdsFormV1,
}

/// Unsigned native metadata and literal manifest, ready for the shared quote
/// calculation. Content roots originate with the publisher; geometry alone
/// cannot produce trace/output content or demonstrate delivery.
pub(crate) struct NativeTfp1UnsignedMaterialV3 {
    pub companion: OriginalCommit,
    pub manifest: NativeRetainedV1<Vec<u8>>,
}

pub(crate) struct NativeTfp1UnsignedQuoteV3 {
    pub material: NativeTfp1UnsignedMaterialV3,
    pub funding: ProtectedFundingQuoteV2,
}

impl<'a> NativeTfp1OriginalPlanV3<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_v3(
        parent: &'a PalwChainStateV2,
        extras: &'a PalwTransitionExtrasV1,
        params: &PalwStateParamsV2,
        daa: u64,
        blue: u64,
        original: &PalwFreePromptCommitmentV3,
        header: &PalwPublicTirBindingHeaderV1,
        prompt_form: PalwPromptIdsFormV1,
        prompt: &[u8],
        descriptor: &[u8],
        policy: &'a Tfp1NativePlanningPolicyV3<'a>,
        mut price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<Self, Error> {
        use NativeDecodeErrorV1::{Decode, Resource};
        let contract = |error| Decode(format!("native original contract: {error:?}"));
        // Check these fixed inputs before the expensive Source/cover build.
        if policy.admission.scope.network.0 != original.job.network_domain.as_bytes()
            || policy.maximum_packets == 0
            || policy.maximum_ranges == 0
            || prompt.len() > crate::palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1
            || std::str::from_utf8(prompt).is_err()
        {
            return Err(Decode("native original policy domain or finite bounds are absent".into()));
        }
        super::super::tfp1_material::require_cold_tfp1_v1(original, &header.job_context).map_err(contract)?;
        let descriptor = HfArtifactDescriptorV1::decode_v1(descriptor).map_err(contract)?;
        let schedule =
            VerificationSchedule::new(OriginalClock::new(daa).map_err(contract)?, policy.notice, policy.case).map_err(contract)?;
        let request = InitialMaterialRequestPlanV1::new_v1(daa, policy.notice, policy.request_service, policy.request_retained_bytes)
            .map_err(batch_error)?;
        let projection = PlainBatchOriginalProjectionV2::derive_at_clocks_v2(
            parent,
            extras,
            daa,
            blue,
            original,
            header,
            prompt_form,
            policy.maximum_leaves,
            policy.cover,
            policy.maximum_ranges,
            policy.output_verification,
            policy.output_verification_daa,
        )?
        .with_native_selection_at_clocks_v3(parent, extras, params, daa, blue, original, policy.selection)?;
        let record = parent.tir_class_v1(&original.job.class_id).ok_or_else(|| Decode("native original class absent".into()))?;
        let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(parent, original.job.class_id)
            .ok_or_else(|| Decode("native original layout absent".into()))?;
        // Existing class resolution clones the registered program/layout and
        // hashes bounded preimages. Cover it before allocation, conservatively
        // retaining that existing native constructor's peak for this owner.
        let requests = PalwTirPublicSourceV1::native_construction_requests_v1(record, layout, header)?;
        let lease = reserve_native_pending_requests_v1(requests).map_err(Resource)?;
        let binding =
            header.resolve_registered_layout_v1(record, layout, original.execution_root, policy.maximum_leaves).map_err(Decode)?;
        let class = NativeRetainedV1::from_owner_reserved_v1(binding.class, lease);
        let source = projection.source_v1();
        let hf = {
            let _hash_scratch = reserve_identity_writers_v3(original, source, &class).map_err(supply_error)?;
            PublicTfp1HfArtifactV1::resolve_v1(
                parent,
                original,
                source,
                descriptor,
                policy.admission.manifest_scope_limits[MaterialScope::Artifact as usize],
            )
            .map_err(contract)?
        };
        let supply = PublicTfp1InitialSupplyV1::derive_at_clocks_v2(
            parent,
            extras,
            daa,
            blue,
            original,
            source,
            &class,
            prompt_form,
            prompt,
            Some(&hf),
        )
        .map_err(supply_error)?;
        supply
            .check_limits_v1(
                policy.admission.max_manifest_bytes,
                policy.admission.manifest_scope_limits,
                policy.admission.manifest_total_limit,
                policy.initial_supply,
            )
            .map_err(supply_error)?;
        let projected = projection
            .compile_prospective_original_terms_v2(
                policy.service_profile,
                policy.budget.retention_until,
                policy.delivery,
                policy.tokenizer,
                policy.storage,
                &mut price,
            )
            .map_err(batch_error)?;
        let early =
            ProtectedOriginalDutiesV1::derive_v1(projected, &supply, blue, policy.notice, schedule, policy.rescue, policy.early)
                .map_err(batch_error)?;
        let terms =
            ProtectedBatchPreclaimTermsV1::new_with_original_duties_v2(projected, policy.rescue, request, early).map_err(contract)?;
        Ok(Self { projection, class, terms, policy, clocks: (daa, blue), supply, parent, extras, prompt_form })
    }

    pub(crate) fn source_v3(&self) -> &PalwTirPublicSourceV1 {
        self.projection.source_v1()
    }
    pub(crate) fn projection_v3(&self) -> &PlainBatchOriginalProjectionV2 {
        &self.projection
    }
    pub(crate) fn terms_v3(&self) -> &ProtectedBatchPreclaimTermsV1 {
        &self.terms
    }

    pub(crate) fn admission_v3(
        &self,
        original: &PalwFreePromptCommitmentV3,
        prompt: &[u8],
    ) -> Result<PublicOriginalAdmissionV1<'_>, Error> {
        let _hash_scratch = reserve_identity_writers_v3(original, self.source_v3(), &self.class).map_err(supply_error)?;
        PublicOriginalAdmissionV1::from_native_tfp1_v1(
            original,
            self.source_v3(),
            &self.class,
            self.policy.admission,
            prompt,
            self.terms.obligation_v1(),
            &self.policy.budget,
        )
        .map_err(|error| Error::Decode(format!("native original admission: {error:?}")))
    }

    /// Relative obligations and the fee ceiling are fixed before manifest
    /// binding. Only then is OriginalId available for physical slot booking.
    /// Publisher entries must match native subjects, sizes and known roots;
    /// unknown content roots are never replaced with placeholder hashes.
    pub(crate) fn unsigned_material_v3(
        &self,
        original: &PalwFreePromptCommitmentV3,
        prompt: &[u8],
        entries: [ManifestEntryV1; MaterialScope::COUNT],
    ) -> Result<NativeTfp1UnsignedMaterialV3, Error> {
        let contract = |error| Error::Decode(format!("native original unsigned material: {error:?}"));
        let admission = self.admission_v3(original, prompt)?;
        let mut companion = {
            let _hash_scratch = reserve_identity_writers_v3(original, self.source_v3(), &self.class).map_err(supply_error)?;
            let job = admission.job;
            OriginalCommit {
                scope: admission.scope,
                job: Digest(job.job_id.as_bytes()),
                execution: Digest(job.context_hash().as_bytes()),
                miner: admission.miner,
                registered_class: Digest(original.job.class_id.as_bytes()),
                artifact: Digest(admission.artifact.as_bytes()),
                program: Digest(self.class.graph_ir_root().as_bytes()),
                prompt: admission.prompt,
                input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
                tokenizer: Digest(job.tokenizer_id.as_bytes()),
                execution_rules: Digest(job.cu_ruleset_id.as_bytes()),
                generation_seed: job.execution_seed,
                initial_state: admission.initial_state,
                dependencies: admission.dependencies,
                prefill: job.declared_prefill_tokens,
                decode: job.exact_decode_tokens,
                max_context: job.max_context_tokens,
                output: Digest(original.output_root.as_bytes()),
                trace_root: Digest(original.execution_root.as_bytes()),
                manifest_root: Digest([0; 64]),
                service_contract: admission.service_contract,
            }
        };
        let manifest = ManifestV1 { service_contract: companion.service_contract, entries };
        let bytes = PalwTirPublicSourceV1::native_identity_writer_requests_v3(ManifestV1::WIRE_BYTES as u64)?;
        let lease = reserve_native_pending_requests_v1(bytes).map_err(Error::Resource)?;
        let literal = companion
            .bind_manifest(&manifest, admission.manifest_scope_limits, admission.manifest_total_limit, admission.max_manifest_bytes)
            .map_err(contract)?;
        let literal = NativeRetainedV1::from_owner_reserved_v1(literal, lease);
        let checked = companion
            .authenticate_manifest(&literal, admission.max_manifest_bytes)
            .and_then(|manifest| manifest.decode(&companion, admission.manifest_scope_limits, admission.manifest_total_limit))
            .map_err(contract)?;
        self.supply.check_manifest_v1(companion.id(), &checked, prompt).map_err(supply_error)?;
        {
            let _hash_scratch = reserve_identity_writers_v3(original, self.source_v3(), &self.class).map_err(supply_error)?;
            admission.check_tfp1_candidate_v1(&companion, self.terms.obligation_v1(), &self.prospective_v3()).map_err(contract)?;
        }
        Ok(NativeTfp1UnsignedMaterialV3 { companion, manifest: literal })
    }

    pub(crate) fn prospective_v3(&self) -> PublicOriginalProspectiveReservationV2<'_> {
        PublicOriginalProspectiveReservationV2 {
            accepted_daa: self.clocks.0,
            accepted_blue_score: self.clocks.1,
            service_obligation: self.terms.obligation_v1().obligation_v1(),
            notice: self.policy.notice,
            case: self.policy.case,
            initial_supply: self.policy.initial_supply,
            rescue: self.policy.rescue,
            usage: &self.policy.usage,
            budget: &self.policy.budget,
        }
    }

    /// Quote the constructed companion against a disposable copy of the
    /// owner's book. The real signed path rebooks at its actual ordered
    /// prefix; neither slots nor an AcceptedOrigin are exported as authority.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn quote_unsigned_v3(
        &self,
        original: &PalwFreePromptCommitmentV3,
        prompt: &[u8],
        descriptor: &[u8],
        entries: [ManifestEntryV1; MaterialScope::COUNT],
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<NativeTfp1UnsignedQuoteV3, Error> {
        let book = &self
            .parent
            .court_service_state_v1()
            .ok_or_else(|| Error::Decode("native original quote parent has no service book".into()))?
            .book;
        self.quote_with_book_v3(original, prompt, descriptor, entries, book, price)
    }

    // The production entry above owns the parent/book relationship. Local
    // arithmetic fixtures can supply a book without fabricating chain rows.
    fn quote_with_book_v3(
        &self,
        original: &PalwFreePromptCommitmentV3,
        prompt: &[u8],
        descriptor: &[u8],
        entries: [ManifestEntryV1; MaterialScope::COUNT],
        book: &ServiceReservations,
        price: impl FnMut(BatchDeliveryV1, u64) -> Result<ServiceCost, String>,
    ) -> Result<NativeTfp1UnsignedQuoteV3, Error> {
        let material = self.unsigned_material_v3(original, prompt, entries)?;
        let admission = self.admission_v3(original, prompt)?;
        let mut candidate = NativeRetainedV1::try_from_clone_v1(book).map_err(Error::Resource)?;
        let funding = admission
            .quote_tfp1_material_funding_v3(
                self.parent,
                self.extras,
                self.prompt_form,
                original,
                self.source_v3(),
                &material.companion,
                prompt,
                &material.manifest,
                descriptor,
                &self.prospective_v3(),
                &self.projection,
                &self.terms,
                &mut candidate,
                self.policy.maximum_packets,
                price,
                self.policy.funding,
            )
            .map_err(|error| match error {
                Tfp1ProtectedFundingAdmissionErrorV1::LocalResource(coverage) => Error::Resource(coverage),
                other => Error::Decode(format!("native original quote: {other:?}")),
            })?;
        Ok(NativeTfp1UnsignedQuoteV3 { material, funding })
    }

    pub(crate) fn actual_v3(&self, origin: AcceptedOrigin) -> Result<PublicOriginalReservationV1<'_>, Error> {
        if (origin.accepted_daa, origin.accepted_blue_score) != self.clocks {
            return Err(Error::Decode("native original actual clocks differ from its recomputed plan".into()));
        }
        let p = self.prospective_v3();
        Ok(PublicOriginalReservationV1 {
            origin,
            service_obligation: p.service_obligation,
            notice: p.notice,
            case: p.case,
            initial_supply: p.initial_supply,
            rescue: p.rescue,
            usage: p.usage,
            budget: p.budget,
        })
    }
}

fn batch_error(error: BatchCourtErrorV1) -> Error {
    match error {
        BatchCourtErrorV1::LocalResource(coverage) => Error::Resource(coverage),
        other => Error::Decode(format!("native original reservation plan: {other:?}")),
    }
}
fn supply_error(error: InitialSupplyErrorV1) -> Error {
    match error {
        InitialSupplyErrorV1::LocalResource(coverage) => Error::Resource(coverage),
        other => Error::Decode(format!("native original material: {other:?}")),
    }
}
