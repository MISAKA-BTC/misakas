//! Resolve one protected-original planning policy from the pinned court
//! service estimate, the pinned batch schedule, and the adopted notice/hard
//! windows. Callers do not supply profile ids, seat counts, or a relative
//! graph. This does not arm a preset, prove model execution, or install rows.

use super::{
    Tfp1PublicAdmissionPolicyV1,
    funding::Tfp1FundingPolicyV1,
    initial_supply::PublicTfp1InitialSupplyV1,
    native_plan::Tfp1NativePlanningPolicyV3,
    preclaim::{
        initial_request::{InitialMaterialRequestPlanV1, MaterialRequestSeriesV2},
        original_duties::{OriginalEarlyDutyCostV1, ProtectedEarlyDutyPolicyV1},
    },
};
use crate::{
    Hash64,
    palw_court_policy_v1::{DISPUTE_WINDOW_DAA, NOTICE_WINDOW_DAA},
    palw_court_service_v1::ServiceCost,
    palw_court_shared_material_intake_v1::batch::{
        admission::{
            plain_selection::{PlainSelectionBudgetV3, SelectionContinuationBudgetV3},
            plain_tokenizer::{PlainTokenizerBatchBudgetV2, PlainTokenizerResourceFloorV2},
        },
        plain_output::projection::PlainBatchOriginalProjectionV2,
        selection::budget::SelectionTerminalBoundsV2,
    },
    palw_freeprompt_v3::PalwFreePromptCommitmentV3,
    palw_g14_values_v1::{
        PALW_G14_PACKET_CARRIAGE_SOMPI_V1, palw_g14_funded_batch_schedule_estimate_v1, palw_g14_funded_original_schedule_estimate_v1,
    },
    palw_model_registry_v1::PALW_REGISTRY_GLOBALS_V1,
    palw_positive_contract_v2::PublicTfp1HfArtifactV1,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
};
use misaka_palw_positive_contract::{
    identity::{ChainScope, Digest},
    messages::AggregateBudget,
    public_one_honest::{
        FirstNoticeDemand, HfArtifactDescriptorV1, ManifestScopeRules, ManifestV1, MaterialCaseDemand, MaterialScope,
        ProtectedRescueDemand, hf_artifact_material_codec_v1, tir_cold_dependencies_codec_v1, tir_cold_initial_state_codec_v1,
        tir_ordinary_tfp1_rules_material_codec_v1, tir_output_material_codec_v1, tir_program_material_codec_v1,
        tir_trace_material_codec_v1, tokenizer_json_material_codec_v1,
    },
    reservations::RemainingCapacity,
    rows::AggregateUsage,
};

/// Local allocation refusal must not become a semantic block rejection when
/// the native admission owner resolves the same policy as the quote endpoint.
#[derive(Debug)]
pub(crate) struct ProtectedPolicyErrorV1(crate::palw_native_resource_v1::NativeDecodeErrorV1<String>);

impl ProtectedPolicyErrorV1 {
    fn native(context: &str, error: crate::palw_native_resource_v1::NativeDecodeErrorV1<String>) -> Self {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
        Self(match error {
            E::Resource(coverage) => E::Resource(coverage),
            E::Decode(why) => E::Decode(format!("{context}: {why}")),
        })
    }

    fn supply(error: super::initial_supply::InitialSupplyErrorV1) -> Self {
        use crate::palw_native_resource_v1::NativeDecodeErrorV1 as E;
        Self(match error {
            super::initial_supply::InitialSupplyErrorV1::LocalResource(coverage) => E::Resource(coverage),
            other => E::Decode(format!("initial supply: {other:?}")),
        })
    }

    pub(crate) fn into_native_v1(self) -> crate::palw_native_resource_v1::NativeDecodeErrorV1<String> {
        self.0
    }
}

impl From<String> for ProtectedPolicyErrorV1 {
    fn from(why: String) -> Self {
        Self(crate::palw_native_resource_v1::NativeDecodeErrorV1::Decode(why))
    }
}

impl From<&str> for ProtectedPolicyErrorV1 {
    fn from(why: &str) -> Self {
        why.to_string().into()
    }
}

mod declaration;
pub use declaration::ProtectedPlanningDeclarationV3;

/// Exactly the funded terms read by the protected planner. Ordinary funded
/// admission's unrelated geometry/roles are not an implicit private default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ProtectedFundingTermsV3 {
    profile_ids: [Hash64; 4],
    original_fee_sompi: u64,
    capital_floor_sompi: u64,
    maximum_dependencies: u64,
    maximum_material_visits: u64,
}

impl ProtectedFundingTermsV3 {
    fn pinned_v3() -> Self {
        let funded = palw_g14_funded_original_schedule_estimate_v1();
        Self {
            profile_ids: [
                funded.fee_profile_id(),
                funded.capital_profile_id(),
                funded.retention_profile_id(),
                funded.settlement_policy_id(),
            ],
            original_fee_sompi: funded.fee_sompi(),
            capital_floor_sompi: funded.capital_floor_sompi(),
            maximum_dependencies: funded.source_budget().max_queries,
            maximum_material_visits: funded.source_budget().max_vertices,
        }
    }
}

/// One selected set of preimages for protected planning. Private fields prevent
/// callers from replacing only the service capacity or material TTL. Selection
/// does not reserve resources, activate a profile or authenticate a claim.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProtectedPlanningSchedulesV3 {
    service_book: crate::palw_court_service_binding_v1::CourtServiceProfileV1,
    batch: crate::palw_funded_original_coordinator_v1::FundedBatchScheduleV1,
    material: crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1,
    funded: ProtectedFundingTermsV3,
    packet_carriage_sompi: u64,
    rounds: u64,
    /// Private declarations supply a construction/walk ceiling. The pinned
    /// legacy selection continues to use the local native limit directly.
    cover_workspace_limit: Option<u64>,
}

impl ProtectedPlanningSchedulesV3 {
    /// The accepting owner resolves activation at the actual block point.
    /// Historical callers must supply extras for the original inclusion point.
    /// An invalid active declaration is an error, never a pinned fallback.
    pub(crate) fn selected_v3(declaration: Option<ProtectedPlanningDeclarationV3>) -> Result<Self, ProtectedPolicyErrorV1> {
        match declaration {
            Some(declaration) => declaration.schedules_v3().map_err(Into::into),
            None => Self::pinned_v3(),
        }
    }

    /// Existing pinned selection only. A future private selection must supply
    /// the complete set and be wired into acceptance and historical readers;
    /// this constructor grants no such configuration or activation authority.
    pub(crate) fn pinned_v3() -> Result<Self, ProtectedPolicyErrorV1> {
        Ok(Self {
            service_book: crate::palw_court_service_binding_v1::PALW_COURT_SERVICE_PROFILE_V1
                .ok_or("protected original has no pinned service profile")?,
            batch: palw_g14_funded_batch_schedule_estimate_v1(),
            material: crate::palw_court_shared_material_intake_v1::material_intake_schedule_v1()
                .map_err(|_| "protected original has no pinned material-intake schedule")?,
            funded: ProtectedFundingTermsV3::pinned_v3(),
            packet_carriage_sompi: PALW_G14_PACKET_CARRIAGE_SOMPI_V1,
            rounds: crate::palw_g14_values_v1::PALW_G14_PACKETS_PER_SIGNER_V1,
            cover_workspace_limit: None,
        })
    }
}

pub(crate) struct PinnedProtectedPolicyV1 {
    carrier_schedule: crate::palw_funded_original_coordinator_v1::FundedBatchScheduleV1,
    relative_service_v2: bool,
    limits: [ManifestScopeRules; MaterialScope::COUNT],
    admission_scope: ChainScope,
    maximum_leaves: u64,
    cover: crate::palw_tir_public_source_v1::parameter_cover_v1::StreamingParameterCoverBudgetV1,
    maximum_ranges: usize,
    output_verification: ServiceCost,
    output_verification_daa: u64,
    selection: PlainSelectionBudgetV3,
    service_book_profile: crate::palw_court_service_binding_v1::CourtServiceProfileV1,
    service_profile: Hash64,
    delivery: crate::palw_court_shared_material_intake_v1::batch::admission::NativeBatchDeliveryCostsV1,
    tokenizer: PlainTokenizerBatchBudgetV2,
    storage: crate::palw_court_shared_material_intake_v1::MaterialIntakeScheduleV1,
    early: ProtectedEarlyDutyPolicyV1,
    request_service: ServiceCost,
    request_retained_bytes: u64,
    request_max_gap_daa: u64,
    notice: FirstNoticeDemand,
    case: MaterialCaseDemand,
    initial_supply: RemainingCapacity,
    rescue: ProtectedRescueDemand,
    budget: AggregateBudget,
    funding: Tfp1FundingPolicyV1,
    maximum_packets: usize,
}

impl PinnedProtectedPolicyV1 {
    pub(crate) fn price_delivery_v1(
        &self,
        stage: crate::palw_court_shared_material_intake_v1::batch::admission::BatchDeliveryV1,
        bytes: u64,
    ) -> Result<ServiceCost, String> {
        self.carrier_schedule.price_delivery_v1(stage, bytes)
    }

    pub(crate) fn policy(&self) -> Tfp1NativePlanningPolicyV3<'_> {
        Tfp1NativePlanningPolicyV3 {
            relative_service_v2: self.relative_service_v2,
            admission: Tfp1PublicAdmissionPolicyV1 {
                scope: self.admission_scope,
                max_manifest_bytes: ManifestV1::WIRE_BYTES,
                manifest_scope_limits: &self.limits,
                manifest_total_limit: self
                    .limits
                    .iter()
                    .try_fold(0u64, |sum, rule| sum.checked_add(rule.max_bytes))
                    .unwrap_or(u64::MAX),
            },
            maximum_leaves: self.maximum_leaves,
            cover: self.cover,
            maximum_ranges: self.maximum_ranges,
            output_verification: self.output_verification,
            output_verification_daa: self.output_verification_daa,
            selection: self.selection,
            service_book_profile: self.service_book_profile,
            service_profile: self.service_profile,
            delivery: self.delivery,
            tokenizer: self.tokenizer,
            storage: self.storage,
            early: self.early,
            request_service: self.request_service,
            request_retained_bytes: self.request_retained_bytes,
            request_max_gap_daa: self.request_max_gap_daa,
            notice: self.notice,
            case: self.case,
            initial_supply: self.initial_supply,
            rescue: self.rescue,
            budget: self.budget,
            usage: AggregateUsage::default(),
            funding: self.funding,
            maximum_packets: self.maximum_packets,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_protected_planning_policy_v3(
    parent: &PalwChainStateV2,
    extras: &PalwTransitionExtrasV1,
    params: &PalwStateParamsV2,
    daa: u64,
    blue: u64,
    original: &PalwFreePromptCommitmentV3,
    header: &PalwPublicTirBindingHeaderV1,
    prompt: &[u8],
    descriptor_bytes: &[u8],
    form: PalwPromptIdsFormV1,
    relative_service_v2: bool,
    prompt_encoding: misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1,
    schedules: ProtectedPlanningSchedulesV3,
) -> Result<PinnedProtectedPolicyV1, ProtectedPolicyErrorV1> {
    prompt_encoding
        .check_body_v1(prompt, crate::palw_tir_funded_original_envelope_v3::MAX_BYTES as u64)
        .map_err(|e| format!("prompt encoding: {e:?}"))?;
    let ProtectedPlanningSchedulesV3 {
        service_book: service_book_profile,
        mut batch,
        material: storage,
        funded,
        packet_carriage_sompi,
        rounds,
        cover_workspace_limit,
    } = schedules;
    // A complete streaming walk includes native Source construction and a
    // root's live traversal. G14's per-branch service heap estimate does not
    // bound that local allocation. Use the existing native request checker;
    // Source/walk reservations still reject insufficient simultaneous capacity.
    // Keep this local bound out of tokenizer/service costs and commitments.
    let mut cover = batch.cover;
    let local_cover_limit = crate::palw_native_resource_v1::native_request_memory_limit_v1()
        .map_err(|coverage| ProtectedPolicyErrorV1(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(coverage)))?;
    cover.max_workspace_bytes = cover_workspace_limit.map_or(local_cover_limit, |declared| declared.min(local_cover_limit));
    batch.delivery = protected_delivery_within_draft_v1(batch.delivery, storage.draft_ttl_daa)?;
    let packet = batch.delivery.numerical_verification.service_cost_v1().ok_or("pinned numerical replay has no service cost")?;
    let notice_bytes = crate::palw_court_shared_material_intake_v1::batch::selection::notice::SignedSelectionNoticeV3::BYTES;
    let carrier_bytes = super::manifest_carrier::MANIFEST_REQUEST_CARRIER_BYTES_V1 as u64;
    let wire = notice_bytes.max(carrier_bytes).max(packet.bytes);
    let service = ServiceCost { bytes: wire, work: packet.work.max(wire), io: packet.io.max(wire), controls: 1 };
    let span = batch.delivery.timing.parts_daa;
    if span == 0 || span > NOTICE_WINDOW_DAA {
        return Err("pinned service span does not fit the notice window".into());
    }
    let case = protected_material_case_demand_v1(batch.delivery)?;
    let step = batch.delivery.timing.open_daa.max(1);
    // The bound panel is recorded as the draw through the adopted notice window.
    // The three inclusion steps still finish at notice end, so the draw itself
    // stays open until that end minus those steps. A 4 DAA open estimate closed
    // the draw before a panel bound inside the notice could be stored.
    let inclusion = step.checked_mul(3).ok_or("notice clock")?;
    let draw_by = daa
        .checked_add(NOTICE_WINDOW_DAA)
        .and_then(|end| end.checked_sub(inclusion))
        .filter(|draw_by| *draw_by > daa)
        .ok_or("notice clock")?;
    let notice = FirstNoticeDemand {
        draw_ready_by: draw_by,
        material_ready_by: draw_by,
        cold_acquisition_daa: step,
        full_inspection_daa: step,
        notice_inclusion_daa: step,
        missing_material_detected_by: draw_by,
        material_request_inclusion_daa: step,
    };
    let seats = extras
        .model_registry
        .as_ref()
        .map_or(u64::from(PALW_REGISTRY_GLOBALS_V1.seat_count), |registry| u64::from(registry.globals.seat_count));
    if seats == 0 {
        return Err("protected original has no panel seats".into());
    }
    let artifact_chunk = u32::try_from(misaka_palw_positive_contract::public_one_honest::HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1)
        .map_err(|_| "HF descriptor exceeds a chunk")?;
    let wide = |codec| ManifestScopeRules { codec, max_bytes: u64::from(u32::MAX), chunk_bytes: u32::MAX };
    let mut limits = [
        ManifestScopeRules {
            codec: hf_artifact_material_codec_v1(),
            max_bytes: u64::from(artifact_chunk),
            chunk_bytes: artifact_chunk,
        },
        wide(tir_program_material_codec_v1()),
        wide(prompt_encoding.codec_v1()),
        wide(Digest(crate::palw_prompt_ids_v1::prompt_ids_form_id_v1(form).as_bytes())),
        wide(tokenizer_json_material_codec_v1()),
        ManifestScopeRules { codec: tir_ordinary_tfp1_rules_material_codec_v1(), max_bytes: 512, chunk_bytes: 512 },
        ManifestScopeRules { codec: tir_cold_initial_state_codec_v1(), max_bytes: 128, chunk_bytes: 128 },
        ManifestScopeRules { codec: tir_cold_dependencies_codec_v1(), max_bytes: 68, chunk_bytes: 68 },
        wide(tir_trace_material_codec_v1()),
        wide(tir_output_material_codec_v1()),
    ];
    if form == PalwPromptIdsFormV1::MerkleV1 {
        let tile = crate::palw_prompt_ids_v1::PALW_PROMPT_IDS_TILE_LEN.checked_mul(4).ok_or("prompt tile")?;
        limits[MaterialScope::InputTokens as usize].chunk_bytes = tile;
        limits[MaterialScope::InputTokens as usize].max_bytes = u64::from(u32::MAX);
    }
    let descriptor = HfArtifactDescriptorV1::decode_v1(descriptor_bytes).map_err(|error| format!("HF reference: {error:?}"))?;
    let provisional = ServiceCost { bytes: wire, work: wire, io: wire, controls: 1 };
    let projection = PlainBatchOriginalProjectionV2::derive_at_clocks_v2(
        parent,
        extras,
        daa,
        blue,
        original,
        header,
        form,
        batch.cover.total.max_original_leaves,
        cover,
        batch.maximum_ranges,
        provisional,
        batch.delivery.timing.verification_daa,
    )
    .map_err(|error| ProtectedPolicyErrorV1::native("protected projection", error))?;
    let source = projection.source_v1();
    let record = parent.tir_class_v1(&original.job.class_id).ok_or("protected original class is absent")?;
    let layout = crate::palw_staged_court_state_v1::palw_staged_tir_layout_v1(parent, original.job.class_id)
        .ok_or("protected original layout is absent")?;
    let binding = header
        .resolve_registered_layout_v1(record, layout, original.execution_root, batch.cover.total.max_original_leaves)
        .map_err(|error| format!("registered layout: {error}"))?;
    let hf = PublicTfp1HfArtifactV1::resolve_v1(parent, original, source, descriptor, limits[MaterialScope::Artifact as usize])
        .map_err(|error| format!("HF pins: {error:?}"))?;
    let supply = PublicTfp1InitialSupplyV1::derive_at_clocks_v2(
        parent,
        extras,
        daa,
        blue,
        original,
        source,
        &binding.class,
        form,
        prompt,
        Some(&hf),
    )
    .and_then(|supply| supply.with_prompt_encoding_v5(prompt_encoding, prompt))
    .map_err(ProtectedPolicyErrorV1::supply)?;
    let allowance_bytes = supply.claim_wire_bytes_v1();
    if allowance_bytes == 0 {
        return Err("protected original supply is empty".into());
    }
    if relative_service_v2 {
        // The v4 commitment promises this actual native material geometry,
        // not every per-scope transport maximum at once.
        for (scope, rule) in super::initial_supply::MATERIAL_SCOPES.into_iter().zip(&mut limits) {
            let bytes = supply.body_bytes_v1(scope);
            if bytes > rule.max_bytes {
                return Err("native material exceeds its transport ceiling".into());
            }
            rule.max_bytes = bytes;
        }
    }
    let allowance =
        RemainingCapacity { transmitted_bytes: allowance_bytes, retained_bytes: allowance_bytes, additional_work: allowance_bytes };
    let tokenizer_bounds = projection.tokenizer_bounds_v2();
    let tokenizer = PlainTokenizerResourceFloorV2::derive_v2(tokenizer_bounds)
        .map_err(|error| format!("protected tokenizer resources: {error:?}"))?
        .budget_v2(service, batch.cover.max_workspace_bytes.max(1), batch.delivery.timing.verification_daa.max(1));
    let tokenizer_cost = tokenizer.verification;
    let selection_bounds = SelectionTerminalBoundsV2::derive_v2(original, source, record, &layout)
        .map_err(|error| ProtectedPolicyErrorV1::native("selection bounds", error))?;
    let (selection_work, selection_io, selection_workspace) = PlainSelectionBudgetV3::service_floors_v3(selection_bounds)
        .map_err(|error| format!("protected Selection resources: {error:?}"))?;
    let selection_bytes = selection_bounds.signed_control_bytes.max(notice_bytes);
    let selection = PlainSelectionBudgetV3 {
        continuation: SelectionContinuationBudgetV3 {
            ready_offset_daa: NOTICE_WINDOW_DAA
                .checked_add(case.material_delivery_daa)
                .and_then(|n| n.checked_add(case.full_inspection_daa))
                .and_then(|n| n.checked_add(case.localization_daa))
                // The discovered row can bind only after localization, not
                // at the last DAA where localization may still be running.
                .and_then(|n| n.checked_add(1))
                .ok_or("selection continuation clock")?,
            bind: ServiceCost { bytes: notice_bytes, work: notice_bytes, io: notice_bytes, controls: 1 },
            bind_daa: batch.delivery.timing.open_daa.max(1),
            open_daa: batch.delivery.timing.open_daa.max(1),
            provider_wait_daa: batch.delivery.timing.provider_wait_daa.max(1),
            begin_daa: batch.delivery.timing.begin_daa.max(1),
            parts_daa: span,
            finish_daa: batch.delivery.timing.finish_daa.max(1),
        },
        verification: ServiceCost {
            bytes: selection_bytes,
            work: selection_work.max(notice_bytes),
            io: selection_io.max(notice_bytes),
            controls: 1,
        },
        verification_daa: batch.delivery.timing.verification_daa.max(1),
        workspace_bytes: selection_workspace.max(1),
        maximum_proof_bytes: selection_bounds.proof_bytes.max(selection_bounds.signed_control_bytes).max(1),
        maximum_dependencies: funded.maximum_dependencies.max(1),
        maximum_ranges_per_role: u64::try_from(batch.maximum_ranges).map_err(|_| "range cap")?.max(1),
        maximum_material_visits: funded.maximum_material_visits.max(1),
    };
    // The terminal independently rechecks all three branches. The ordinary
    // numerical-only terminal cannot cover tokenizer and Selection work.
    // These are required costs, not additional capacity in the rooted book.
    let mut delivery = batch.delivery;
    delivery.terminal = protected_terminal_cost_v1(packet, tokenizer_cost, selection.verification, delivery.terminal)?;
    let retained = InitialMaterialRequestPlanV1::minimum_retained_bytes_v1();
    let request_service = ServiceCost {
        bytes: service.bytes.max(retained),
        work: service.work.max(retained),
        io: service.io.max(retained),
        controls: 1,
    };
    let early = ProtectedEarlyDutyPolicyV1 {
        first_notice_wire_bytes: notice_bytes,
        first_notice: OriginalEarlyDutyCostV1 {
            service: ServiceCost {
                bytes: service.bytes.max(notice_bytes),
                work: service.work.max(notice_bytes),
                io: service.io.max(notice_bytes),
                controls: 1,
            },
            retained_bytes: notice_bytes,
            escrow_sompi: packet_carriage_sompi,
            future_carrier_sompi: packet_carriage_sompi,
        },
    };
    let service = early.first_notice.service;
    let clock = |value: u64| if relative_service_v2 { value.checked_sub(daa).ok_or("non-batch relative clock") } else { Ok(value) };
    let graph = crate::palw_court_shared_tir_stream_v1::stream_digest(
        if relative_service_v2 { b"misaka-palw/tfp1-pinned-nonbatch-graph/v2" } else { b"misaka-palw/tfp1-pinned-nonbatch-graph/v1" },
        &(
            [
                clock(notice.draw_ready_by)?,
                clock(notice.material_ready_by)?,
                notice.cold_acquisition_daa,
                notice.full_inspection_daa,
                notice.notice_inclusion_daa,
                clock(notice.missing_material_detected_by)?,
                notice.material_request_inclusion_daa,
            ],
            [
                case.material_delivery_daa,
                case.full_inspection_daa,
                case.localization_daa,
                case.filing_daa,
                case.adjudication_and_recovery_daa,
            ],
            span,
            [service.bytes, service.work, service.io, service.controls],
            [early.first_notice.retained_bytes, early.first_notice.escrow_sompi, early.first_notice.future_carrier_sompi],
            [
                funded.profile_ids[0].as_bytes(),
                funded.profile_ids[1].as_bytes(),
                funded.profile_ids[2].as_bytes(),
                funded.profile_ids[3].as_bytes(),
            ],
        ),
    )
    .map_err(|_| "non-batch graph digest")?;
    let scope = ChainScope {
        network: Digest(original.job.network_domain.as_bytes()),
        ruleset: prompt_encoding.ruleset_v1(Digest(funded.profile_ids[0].as_bytes())),
        branch: Digest(original.job.anchor_block.as_bytes()),
    };
    let roles = seats.checked_add(1).ok_or("rescue roles")?;
    let total_roles = if relative_service_v2 { roles.checked_add(1).ok_or("initial plus rescue roles")? } else { roles };
    let transmitted = allowance_bytes.checked_mul(total_roles).ok_or("supply budget")?;
    let retention_until = if params.court_clock_v2_from_daa().is_some_and(|from| daa >= from) {
        crate::palw_court_clock_v2::palw_court_clock_v2_required_retention_with(
            daa,
            crate::palw_state_v2::palw_da_disclose_window_daa_v1(params),
            crate::palw_court_clock_v2::palw_court_clock_v2_reorg_retention(extras.staged_public_court.as_ref()),
        )
        .map_err(|_| "retention clock")?
    } else {
        daa.checked_add(DISPUTE_WINDOW_DAA).ok_or("retention clock")?
    };
    let mut fee = funded
        .original_fee_sompi
        .checked_add(funded.capital_floor_sompi)
        .and_then(|n| n.checked_add(packet_carriage_sompi.checked_mul(roles)?))
        .ok_or("fee budget")?;
    let mut rescue = ProtectedRescueDemand {
        max_drawn_bonds: seats,
        per_drawn_bond: allowance,
        max_outsiders: 0,
        per_outsider: allowance,
        defensive: allowance,
    };
    let mut aggregate = RemainingCapacity {
        transmitted_bytes: transmitted,
        additional_work: if relative_service_v2 { transmitted } else { allowance_bytes },
        retained_bytes: if relative_service_v2 { transmitted } else { allowance_bytes },
    };
    if relative_service_v2 {
        // A material byte count is not the cost of every delivery, replay,
        // request, terminal and retained fault in a protected case. Compile
        // the same final geometry as native admission before signing a ceiling.
        let projection = projection
            .with_output_verification_v2(original, tokenizer_cost, batch.delivery.timing.verification_daa.max(1))
            .and_then(|projection| {
                projection.with_native_selection_at_clocks_v3(parent, extras, params, daa, blue, original, selection)
            })
            .map_err(|error| ProtectedPolicyErrorV1::native("protected accounting projection", error))?;
        let batch_error = |error| match error {
            crate::palw_batch_court_v1::BatchCourtErrorV1::LocalResource(coverage) => {
                ProtectedPolicyErrorV1(crate::palw_native_resource_v1::NativeDecodeErrorV1::Resource(coverage))
            }
            other => ProtectedPolicyErrorV1::from(format!("protected accounting: {other:?}")),
        };
        let projected = projection
            .compile_prospective_original_terms_v2(
                funded.profile_ids[0],
                retention_until,
                delivery,
                tokenizer,
                storage,
                |stage, bytes| batch.price_delivery_v1(stage, bytes),
            )
            .map_err(batch_error)?;
        let requests = MaterialRequestSeriesV2::new_v2(
            InitialMaterialRequestPlanV1::new_v1(daa, notice, request_service, retained).map_err(batch_error)?,
            span,
        )
        .map_err(batch_error)?;
        let accounting = projected.accounting_with_requests_v2(request_service, retained, requests.count_v2()).map_err(batch_error)?;
        let trace = projection.public_trace_payload_bytes_v1().map_err(batch_error)?;
        if allowance.transmitted_bytes < trace || allowance.retained_bytes < trace {
            return Err("protected initial supply understates the original trace".into());
        }
        // Each protected role may need the complete original material as well
        // as its own booked case graph. Those are simultaneous obligations,
        // so neither one can stand in for the other via `max`.
        let defensive = RemainingCapacity {
            transmitted_bytes: accounting.service.bytes,
            additional_work: accounting.service.work,
            retained_bytes: accounting
                .retained_payload_bytes
                .checked_add(accounting.retained_numerical_fault_bytes)
                .ok_or("protected retained budget")?,
        }
        .checked_add(allowance)
        .map_err(|_| "protected case and material supply budget")?;
        let notice_capacity = RemainingCapacity {
            transmitted_bytes: early.first_notice.service.bytes,
            additional_work: early.first_notice.service.work,
            retained_bytes: early.first_notice.retained_bytes,
        };
        let drawn = defensive.checked_add(notice_capacity).map_err(|_| "protected notice budget")?;
        rescue.per_drawn_bond = drawn;
        rescue.per_outsider = drawn;
        rescue.defensive = defensive;
        aggregate = drawn
            .checked_scale(seats)
            .and_then(|n| n.checked_add(defensive))
            .and_then(|n| n.checked_add(allowance))
            .map_err(|_| "protected aggregate budget")?;
        fee = funded
            .original_fee_sompi
            .checked_add(funded.capital_floor_sompi)
            .and_then(|n| n.checked_add(accounting.storage_rent_sompi.checked_mul(roles)?))
            .and_then(|n| {
                n.checked_add(
                    early.first_notice.escrow_sompi.checked_add(early.first_notice.future_carrier_sompi)?.checked_mul(seats)?,
                )
            })
            .ok_or("protected fee budget")?;
    }
    Ok(PinnedProtectedPolicyV1 {
        carrier_schedule: batch,
        relative_service_v2,
        limits,
        admission_scope: scope,
        maximum_leaves: batch.cover.total.max_original_leaves,
        cover,
        maximum_ranges: batch.maximum_ranges,
        output_verification: tokenizer_cost,
        output_verification_daa: batch.delivery.timing.verification_daa.max(1),
        selection,
        service_book_profile,
        service_profile: funded.profile_ids[0],
        delivery,
        tokenizer,
        storage,
        early,
        request_service,
        request_retained_bytes: retained,
        request_max_gap_daa: span,
        notice,
        case,
        initial_supply: allowance,
        rescue,
        budget: AggregateBudget {
            rounds,
            unique_source_bytes: allowance_bytes,
            transmitted_bytes: aggregate.transmitted_bytes,
            additional_work: aggregate.additional_work,
            retained_bytes: aggregate.retained_bytes,
            retention_until,
            fee,
            witness_attempts: u32::try_from(rounds).unwrap_or(u32::MAX),
        },
        funding: Tfp1FundingPolicyV1 {
            profiles: [funded.profile_ids[0], funded.profile_ids[1], funded.profile_ids[2], funded.profile_ids[3]],
            other_relative_graph: graph,
            original_fee_sompi: funded.original_fee_sompi,
            capital_floor_sompi: funded.capital_floor_sompi,
        },
        maximum_packets: batch.delivery.maximum_packets.max(1),
    })
}

/// The protected grammar reserves a second Finish for tokenizer authentication.
/// Draft lifetime starts at Begin, so both Finish windows must fit after Parts.
/// Intersect the proposed Parts width with the existing TTL; retain every wait,
/// cost and packet. The complete reservation still refuses insufficient slots.
fn protected_delivery_within_draft_v1(
    mut delivery: crate::palw_court_shared_material_intake_v1::batch::admission::NativeBatchDeliveryCostsV1,
    draft_ttl_daa: u64,
) -> Result<crate::palw_court_shared_material_intake_v1::batch::admission::NativeBatchDeliveryCostsV1, String> {
    let finishing = delivery.timing.finish_daa.checked_mul(2).ok_or("protected draft finishing clock overflow")?;
    let parts = draft_ttl_daa
        .checked_sub(delivery.timing.begin_daa)
        .and_then(|remaining| remaining.checked_sub(finishing))
        .filter(|remaining| *remaining > 0)
        .ok_or("protected draft has no room for Parts and both Finish windows")?;
    if delivery.timing.parts_daa == 0 {
        return Err("protected draft has no Parts window".into());
    }
    delivery.timing.parts_daa = delivery.timing.parts_daa.min(parts);
    Ok(delivery)
}

/// Compose the same phases as the protected plain Output/Tokenizer/Selection
/// compiler. These are inclusive deadlines: the first delivery starts at
/// notice+1 and late Selection binding starts at localized_by+1.
fn protected_material_case_demand_v1(
    delivery: crate::palw_court_shared_material_intake_v1::batch::admission::NativeBatchDeliveryCostsV1,
) -> Result<MaterialCaseDemand, String> {
    let timing = delivery.timing;
    let sum = |parts: &[u64]| {
        parts.iter().try_fold(0u64, |total, part| total.checked_add(*part)).ok_or("protected material case clock overflow")
    };
    Ok(MaterialCaseDemand {
        material_delivery_daa: sum(&[
            timing.open_daa,
            timing.provider_wait_daa,
            timing.begin_daa,
            timing.parts_daa,
            timing.finish_daa,
            timing.finish_daa, // authenticated tokenizer Finish
        ])?,
        full_inspection_daa: sum(&[
            timing.verification_daa, // numerical
            timing.verification_daa, // Output
            timing.verification_daa, // Tokenizer
        ])?,
        localization_daa: timing.stop_daa,
        filing_daa: sum(&[
            timing.open_daa, // Selection binding
            timing.open_daa,
            timing.provider_wait_daa,
            timing.begin_daa,
            timing.parts_daa,
            timing.finish_daa,
            timing.verification_daa, // signed Selection evidence
        ])?,
        adjudication_and_recovery_daa: sum(&[timing.terminal_daa, delivery.recovery_daa])?,
    })
}

fn protected_terminal_cost_v1(
    numerical: ServiceCost,
    tokenizer: ServiceCost,
    selection: ServiceCost,
    floor: ServiceCost,
) -> Result<ServiceCost, String> {
    let work = numerical.work.checked_add(tokenizer.work).and_then(|n| n.checked_add(selection.work)).ok_or("terminal work")?;
    let io = numerical.io.checked_add(tokenizer.io).and_then(|n| n.checked_add(selection.io)).ok_or("terminal io")?;
    Ok(ServiceCost {
        bytes: floor.bytes.max(numerical.bytes).max(selection.bytes),
        work: floor.work.max(work),
        io: floor.io.max(io),
        controls: 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protected_material_deadlines_cover_delivery_inspection_and_late_selection() {
        use misaka_palw_positive_contract::public_one_honest::{OriginalClock, VerificationSchedule};
        let delivery = protected_delivery_within_draft_v1(
            palw_g14_funded_batch_schedule_estimate_v1().delivery,
            crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.draft_ttl_daa,
        )
        .unwrap();
        let case = protected_material_case_demand_v1(delivery).unwrap();
        let a = 100;
        let schedule = VerificationSchedule::new(
            OriginalClock::new(a).unwrap(),
            FirstNoticeDemand {
                draw_ready_by: a + NOTICE_WINDOW_DAA - 12,
                material_ready_by: a + NOTICE_WINDOW_DAA - 12,
                cold_acquisition_daa: 4,
                full_inspection_daa: 4,
                notice_inclusion_daa: 4,
                missing_material_detected_by: a + NOTICE_WINDOW_DAA - 12,
                material_request_inclusion_daa: 4,
            },
            case,
        )
        .unwrap();
        // Native windows start one DAA after the preceding inclusive boundary.
        assert_eq!(schedule.material_ready_by(), a + NOTICE_WINDOW_DAA + 766);
        assert_eq!(schedule.inspection_by(), schedule.material_ready_by() + 126);
        assert_eq!(schedule.localized_by(), schedule.inspection_by() + 42);
        assert_eq!(schedule.filed_by(), schedule.localized_by() + 770);
        assert_eq!(schedule.terminal_by(), schedule.filed_by() + 42 + delivery.recovery_daa);
        assert!(schedule.terminal_by() < schedule.hard());
        let mut overflow = delivery;
        overflow.timing.provider_wait_daa = u64::MAX;
        assert!(protected_material_case_demand_v1(overflow).is_err());
    }

    #[test]
    fn protected_delivery_finishes_before_native_draft_expiry() {
        let original = palw_g14_funded_batch_schedule_estimate_v1().delivery;
        let ttl = crate::palw_g14_values_v1::PALW_G14_MATERIAL_INTAKE_ESTIMATE_V1.draft_ttl_daa;
        let fixed_phases = original.timing.begin_daa + 2 * original.timing.finish_daa;
        assert!(fixed_phases + original.timing.parts_daa > ttl, "the pinned widths exceed the draft lifetime");
        let fitted = protected_delivery_within_draft_v1(original, ttl).unwrap();
        assert_eq!(fixed_phases + fitted.timing.parts_daa, ttl);
        let mut expected = original;
        expected.timing.parts_daa = fitted.timing.parts_daa;
        assert_eq!(fitted, expected, "only Parts width changes; costs, waits and packet bounds remain pinned");
        assert!(protected_delivery_within_draft_v1(original, fixed_phases).is_err());
        assert_eq!(protected_delivery_within_draft_v1(original, fixed_phases + 1).unwrap().timing.parts_daa, 1);
        assert_eq!(protected_delivery_within_draft_v1(original, u64::MAX).unwrap(), original);
        let mut invalid = original;
        invalid.timing.finish_daa = u64::MAX;
        assert!(protected_delivery_within_draft_v1(invalid, u64::MAX).is_err());
        invalid = original;
        invalid.timing.parts_daa = 0;
        assert!(protected_delivery_within_draft_v1(invalid, ttl).is_err());
    }
}
