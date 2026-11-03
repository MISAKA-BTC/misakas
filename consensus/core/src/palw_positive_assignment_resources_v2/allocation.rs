//! Real owner for the allocations made by pure Assignment derivation. This
//! reserves construction/retained memory only, never future Case/P/work supply,
//! funds, accepted provenance, delivery or credit. Literal layout transport has
//! its independent existing native owner before this function is called.
use super::*;
use crate::{
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, TirNativeRequestsV1,
        reserve_native_requests_v1, source_tir_program_shape_v1,
    },
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, prompt_ids_tile_count_v1},
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_attempt_v1::palw_tir_attempt_canonical_of_v1,
    palw_v2::PalwJobContextV2,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentDerivedAllocationQuoteV2 {
    pub source_program_and_validator: u64,
    pub final_plan_program_and_validator: u64,
    pub temporary_program_and_validator: u64,
    pub retained_inventory_and_instance_analysis: u64,
    pub final_plan_step_space: u64,
    pub temporary_leaf_count_step_space: u64,
    pub raw_class_and_layout_owners: u64,
    pub prompt_and_commitment_owners: u64,
    pub inline_and_job_owners: u64,
    pub total_requests: u64,
}
fn native(error: NativeCoverageV1) -> AssignmentResourceErrorV2 {
    AssignmentResourceErrorV2::Native(error)
}
fn add(q: &mut TirNativeRequestsV1, n: u64) -> Result<(), AssignmentResourceErrorV2> {
    q.add_v1(n).map_err(native)
}
fn checked_product(a: u64, b: u64) -> Result<u64, AssignmentResourceErrorV2> {
    TirNativeRequestsV1::product_v1(a, b).map_err(native)
}
fn bounded(quote: u64, maximum: u64) -> Result<(), AssignmentResourceErrorV2> {
    if maximum == 0 || maximum == u64::MAX || quote > maximum {
        return Err(AssignmentResourceErrorV2::ConstructionCap {
            required: quote,
            maximum,
        });
    }
    Ok(())
}
fn require_lease(
    lease: Option<NativeDemandLeaseV1>,
) -> Result<NativeDemandLeaseV1, AssignmentResourceErrorV2> {
    lease.ok_or(AssignmentResourceErrorV2::MissingNativeLease)
}

impl AssignmentDerivedAllocationQuoteV2 {
    /// Actual registered raw bytes and ORIGINAL facts feed the audited scanner.
    /// No decoded caller cardinality, encoded-size multiplier or current-tip
    /// rescaling substitutes for the concrete typed request inventory.
    pub(crate) fn from_registered(
        registered: &PalwTirClassRecordV1,
        layout: &PalwTirLayoutV1,
        assignment: &AssignmentPolicyV2,
        original_network_form: PalwPromptIdsFormV1,
    ) -> Result<Self, AssignmentResourceErrorV2> {
        assignment
            .validate_shape()
            .map_err(|_| AssignmentResourceErrorV2::Policy)?;
        if registered.program_bytes == 0
            || registered.program.len() as u64 != u64::from(registered.program_bytes)
            || registered.program_bytes > assignment.max_program_bytes
            || layout.commit_tiles.len() as u64 > u64::from(registered.program_bytes)
            || layout.state_tiles.len() as u64 > u64::from(registered.program_bytes)
            || (layout.commit_tiles.len() as u64)
                .checked_add(layout.state_tiles.len() as u64)
                .is_none_or(|n| n > u64::from(assignment.max_layout_entries))
            || layout.max_context != registered.facts.max_context
        {
            return Err(AssignmentResourceErrorV2::Binding);
        }
        // CoveragePlan's active ExactStream accepts I32 tiled logits. Refuse
        // the known wide suffix BEFORE its distinct profiled derivation can
        // allocate. This is deny-only, never profile authentication.
        if registered
            .program
            .ends_with(&misaka_palw_tir::program::LOGITS_I64_Q24_SCHEME_V2)
        {
            return Err(AssignmentResourceErrorV2::Check(
                CheckError::UnsupportedProfile,
            ));
        }
        let (shape, normal) =
            source_tir_program_shape_v1(&registered.program).map_err(|error| match error {
                NativeDecodeErrorV1::Resource(e) => native(e),
                NativeDecodeErrorV1::Decode(_) => AssignmentResourceErrorV2::Binding,
            })?;
        let step = crate::palw_tir_step_v1::native_tir_step_space_additional_requests_v1(&shape)
            .map_err(native)?;
        let inventory = crate::palw_tir_court_v1::native_tir_inventory_index_requests_v1(&shape)
            .map_err(native)?;
        let canonical = palw_tir_attempt_canonical_of_v1(registered.facts.max_context)
            .ok_or(AssignmentResourceErrorV2::Binding)?;
        registered
            .facts
            .max_context
            .checked_add(1)
            .ok_or(AssignmentResourceErrorV2::Budget)?;
        // Named real parsed-program owners may coexist: retained source, final
        // CoveragePlan program, and one metadata/StepSpace decode validator.
        // Each profile is the narrow audited SOURCE normal-form profile, not
        // general program_requests (which also prices unused cone/gate work).
        let mut total = TirNativeRequestsV1::new();
        for amount in [normal, normal, normal, inventory, step, step] {
            add(&mut total, amount)?;
        }
        let mut raw = TirNativeRequestsV1::new();
        raw.exact_v1::<u8>(registered.program.len() as u64)
            .map_err(native)?; // owned class.program clone
        let layout_entries = (layout.commit_tiles.len() as u64)
            .checked_add(layout.state_tiles.len() as u64)
            .ok_or(AssignmentResourceErrorV2::Budget)?;
        for _owner in 0..3 {
            // class.layout, final plan.space.layout, temporary leaf-count space.layout
            raw.exact_v1::<u32>(layout.commit_tiles.len() as u64)
                .map_err(native)?;
            raw.exact_v1::<u32>(layout.state_tiles.len() as u64)
                .map_err(native)?;
        }
        let layout_wire = checked_product(layout_entries, 4)?
            .checked_add(22)
            .ok_or(AssignmentResourceErrorV2::Budget)?;
        // Existing borsh::to_vec starts at1024; ordinary metadata/layout hash
        // buffers are actual byte writers and get the pinned u8 growth bound.
        raw.vec_v1::<u8>(layout_wire.max(1024)).map_err(native)?;
        let raw_class_and_layout_owners = raw.bytes_v1();
        add(&mut total, raw_class_and_layout_owners)?;
        let prompt = prompt_requests(
            u64::from(canonical.0),
            registered.facts.prompt_ids_form(original_network_form),
        )?;
        add(&mut total, prompt)?;
        let mut inline = TirNativeRequestsV1::new();
        inline
            .exact_v1::<AssignmentDerivedResourcesV2>(1)
            .map_err(native)?;
        inline.exact_v1::<PalwJobContextV2>(2).map_err(native)?; // binding + plan contexts/move workspace
        // Both current constructors use the existing native14-byte literal.
        // Use the protocol's64-byte maximum for the retained binding/plan
        // strings and canonical context writer; no token/context field changes.
        for _context in 0..2 {
            inline
                .exact_v1::<u8>(crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES as u64)
                .map_err(native)?;
        }
        let context_wire =
            2 + 4 + (crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES as u64) + 11 * 64 + 32 + 3 * 4;
        inline.vec_v1::<u8>(context_wire.max(512)).map_err(native)?;
        // Fixed Positive policy root uses existing borsh::to_vec initial1024.
        inline.exact_v1::<u8>(1024).map_err(native)?;
        let inline_and_job_owners = inline.bytes_v1();
        add(&mut total, inline_and_job_owners)?;
        let quote = Self {
            source_program_and_validator: normal,
            final_plan_program_and_validator: normal,
            temporary_program_and_validator: normal,
            retained_inventory_and_instance_analysis: inventory,
            final_plan_step_space: step,
            temporary_leaf_count_step_space: step,
            raw_class_and_layout_owners,
            prompt_and_commitment_owners: prompt,
            inline_and_job_owners,
            total_requests: total.bytes_v1(),
        };
        bounded(quote.total_requests, assignment.max_heap_bytes)?;
        Ok(quote)
    }
    /// Quote actual native registry data without constructing derived vectors.
    pub fn for_state(
        state: &PalwChainStateV2,
        terms: &AssignmentTermsV2,
        layout: &PalwTirLayoutV1,
        assignment: &AssignmentPolicyV2,
        original_network_form: PalwPromptIdsFormV1,
    ) -> Result<Self, AssignmentResourceErrorV2> {
        let registered = state
            .tir_class_v1(&Hash64::from_bytes(terms.registered_class.0))
            .ok_or(AssignmentResourceErrorV2::MissingRegistry)?;
        Self::from_registered(registered, layout, assignment, original_network_form)
    }
}
fn prompt_requests(
    tokens: u64,
    form: PalwPromptIdsFormV1,
) -> Result<u64, AssignmentResourceErrorV2> {
    let mut q = TirNativeRequestsV1::new();
    q.exact_v1::<u32>(tokens).map_err(native)?; // actual canonical prompt owner
    match form {
        PalwPromptIdsFormV1::Flat => {
            let bytes = checked_product(tokens, 4)?
                .checked_add(4)
                .ok_or(AssignmentResourceErrorV2::Budget)?;
            q.vec_v1::<u8>(bytes.max(512)).map_err(native)?; // existing CanonicalWriter::with_capacity512
        }
        PalwPromptIdsFormV1::MerkleV1 => {
            let tiles =
                prompt_ids_tile_count_v1(tokens).ok_or(AssignmentResourceErrorV2::Budget)?;
            q.vec_v1::<Hash64>(tiles).map_err(native)?; // original tile-leaf Vec
            q.vec_v1::<Hash64>(tiles).map_err(native)?; // step_merkle mapped current level
            q.exact_v1::<Hash64>(tiles.div_ceil(2)).map_err(native)?; // simultaneously allocated next level
            q.exact_v1::<u8>(checked_product(
                tokens.min(u64::from(PALW_PROMPT_IDS_TILE_LEN)),
                4,
            )?)
            .map_err(native)?; // one tile leaf preimage
            q.exact_v1::<u8>(256).map_err(native)?; // existing step-leaf Writer::with_capacity256
        }
    }
    Ok(q.bytes_v1())
}

/// The retained source catalog keeps its own share of the derived allocation lease.
pub(crate) struct ReservedAssignmentSourceGeometryV2 {
    pub(crate) geometry: crate::palw_positive_source_geometry_v2::TirSourceGeometryV2,
    _lease: NativeDemandLeaseV1,
}
impl ReservedAssignmentSourceGeometryV2 {
    pub(crate) fn heap_bytes(&self) -> u64 {
        self._lease.memory_v1()
    }
}

/// Borrowed derived outputs stay within this owner. Source geometry can be
/// transferred into its own owner together with the reserved memory.
pub struct ReservedAssignmentDerivedResourcesV2 {
    resources: AssignmentDerivedResourcesV2,
    quote: AssignmentDerivedAllocationQuoteV2,
    _lease: NativeDemandLeaseV1,
}
impl ReservedAssignmentDerivedResourcesV2 {
    /// Resolve the same job and source catalog for Miner/Panel from an
    /// accepted Open row and its frozen admission material.
    pub fn from_accepted_open_v2(
        state: &PalwChainStateV2,
        material: &crate::palw_positive_assignment_material_v2::AssignmentAdmissionMaterialV2,
        layout: &PalwTirLayoutV1,
        now: u64,
    ) -> Result<Self, AssignmentResourceErrorV2> {
        use misaka_palw_positive_contract::assignment::rows::AssignmentStatusV2;

        let rows = crate::palw_positive_rows_v2::PositiveRowsV2::new(state)
            .assignment(material.assignment)
            .map_err(|_| AssignmentResourceErrorV2::Binding)?;
        let row = &rows.assignment;
        material
            .check_row(row)
            .map_err(|_| AssignmentResourceErrorV2::Binding)?;
        material
            .check_registered_class_v2(state)
            .map_err(|_| AssignmentResourceErrorV2::MissingRegistry)?;
        if row.status != AssignmentStatusV2::Open
            || now < row.origin.accepted_daa
            || now >= row.bundle.terms.result_before_daa
        {
            return Err(AssignmentResourceErrorV2::Binding);
        }
        let owner = Self::derive(
            state,
            row.bundle.terms,
            layout,
            &material.assignment_policy,
            &material.positive_policy,
            &material.native_preimages.source_policy,
            material.network_prompt_form,
            material
                .custody
                .heap_bytes
                .min(material.native_preimages.physical.max_heap_bytes),
        )?;
        let resources = owner.resources();
        material
            .job
            .check_native(resources.binding().job())
            .map_err(|_| AssignmentResourceErrorV2::Binding)?;
        if crate::palw_positive_assignment_material_v2::AssignmentSourceQuoteDescriptorV2::from(
            resources.source_quote(),
        ) != material.source
        {
            return Err(AssignmentResourceErrorV2::Binding);
        }
        Ok(owner)
    }

    /// Prepare the lifecycle payload from the signed actor Result and poster's
    /// Case funding consent. The caller supplies and signs the actual UTXOs.
    pub fn encode_assigned_result_payload_v2(
        &self,
        payload: crate::palw_positive_court_v2::coordinator::assignment_body::AssignedResultPayloadV2,
        miner_key: &[u8; misaka_palw_positive_contract::identity::PUBKEY_BYTES],
        payer_key: &[u8; misaka_palw_positive_contract::identity::PUBKEY_BYTES],
        now: u64,
        limits: crate::palw_positive_court_v2::coordinator::assignment_body::AssignmentBodyLimitsV2,
    ) -> Result<Vec<u8>, AssignmentResourceErrorV2> {
        use crate::palw_positive_court_v2::coordinator::assignment_body::{
            PositiveAssignmentNativeBodyV2, PositiveAssignmentNativeOperationV2,
            encode_assignment_body_v2,
        };
        use misaka_palw_positive_contract::{CONTRACT_REVISION, CONTRACT_VERSION};

        let binding = self.resources().binding();
        let terms = binding.terms();
        binding.check_result(&payload.result)?;
        payload
            .result
            .authenticate(miner_key, &payload.miner_signature)
            .map_err(|_| AssignmentResourceErrorV2::Binding)?;
        if now >= terms.result_before_daa
            || payload.consent.payer != terms.payer
            || payload.consent.refund_payload != terms.refund_payload
            || payload.service.source_catalog != self.resources().source_quote().digest
        {
            return Err(AssignmentResourceErrorV2::Binding);
        }
        payload
            .consent
            .authenticate(
                &payload.result,
                &payload.service,
                &payload.basis,
                now,
                payer_key,
                &payload.payer_signature,
            )
            .map_err(|_| AssignmentResourceErrorV2::Binding)?;
        let body = PositiveAssignmentNativeBodyV2 {
            version: CONTRACT_VERSION,
            revision: CONTRACT_REVISION,
            operation: PositiveAssignmentNativeOperationV2::AdmitAssignedResult {
                assignment: terms.id(),
                payload,
            },
        };
        let body = encode_assignment_body_v2(&body, limits)
            .map_err(|_| AssignmentResourceErrorV2::Budget)?;
        let mut lifecycle = Vec::new();
        lifecycle
            .try_reserve_exact(
                body.len()
                    .checked_add(3)
                    .ok_or(AssignmentResourceErrorV2::Budget)?,
            )
            .map_err(|_| AssignmentResourceErrorV2::Budget)?;
        lifecycle.extend_from_slice(&[1, 0, 176]);
        lifecycle.extend_from_slice(&body);
        Ok(lifecycle)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        state: &PalwChainStateV2,
        terms: AssignmentTermsV2,
        layout: &PalwTirLayoutV1,
        assignment: &AssignmentPolicyV2,
        positive: &PositiveCourtPolicyV2,
        source_policy: &AssignmentSourcePolicyV2,
        original_network_form: PalwPromptIdsFormV1,
        physical_max_heap_bytes: u64,
    ) -> Result<Self, AssignmentResourceErrorV2> {
        positive
            .validate_shape()
            .map_err(|_| AssignmentResourceErrorV2::Policy)?;
        source_policy.validate()?;
        let quote = AssignmentDerivedAllocationQuoteV2::for_state(
            state,
            &terms,
            layout,
            assignment,
            original_network_form,
        )?;
        bounded(quote.total_requests, physical_max_heap_bytes)?;
        // This MUST succeed with Some before any registered-program decode,
        // class/layout clone, inventory, prompt or CoveragePlan allocation.
        let lease =
            require_lease(reserve_native_requests_v1(quote.total_requests).map_err(native)?)?;
        let resources = AssignmentDerivedResourcesV2::derive(
            state,
            terms,
            layout,
            assignment,
            positive,
            source_policy,
            original_network_form,
        )?;
        Ok(Self {
            resources,
            quote,
            _lease: lease,
        })
    }
    pub fn resources(&self) -> &AssignmentDerivedResourcesV2 {
        &self.resources
    }
    pub(crate) fn into_source_geometry_v2(
        mut self,
        result: &misaka_palw_positive_contract::messages::MinerResultCommit,
    ) -> Result<ReservedAssignmentSourceGeometryV2, AssignmentResourceErrorV2> {
        self.resources.binding.check_result(result)?;
        let retained = self
            .quote
            .source_program_and_validator
            .checked_add(self.quote.retained_inventory_and_instance_analysis)
            .ok_or(AssignmentResourceErrorV2::Budget)?;
        let lease = self._lease.split_memory_v1(retained).map_err(native)?;
        let geometry =
            crate::palw_positive_source_geometry_v2::TirSourceGeometryV2::bind_catalog_v2(
                result,
                self.resources.source,
            )?;
        Ok(ReservedAssignmentSourceGeometryV2 {
            geometry,
            _lease: lease,
        })
    }
    pub fn allocation_quote(&self) -> AssignmentDerivedAllocationQuoteV2 {
        self.quote
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn construction_cap_and_no_lease_never_become_success() {
        assert!(matches!(
            bounded(100, 99),
            Err(AssignmentResourceErrorV2::ConstructionCap {
                required: 100,
                maximum: 99
            })
        ));
        assert!(bounded(100, 100).is_ok());
        assert!(bounded(1, 0).is_err());
        assert!(bounded(1, u64::MAX).is_err());
        assert!(matches!(
            require_lease(None),
            Err(AssignmentResourceErrorV2::MissingNativeLease)
        ));
    }
    #[test]
    fn prompt_arrays_and_merkle_levels_have_checked_extent_and_growth() {
        assert!(prompt_requests(u64::MAX, PalwPromptIdsFormV1::Flat).is_err());
        assert!(prompt_requests(u64::MAX, PalwPromptIdsFormV1::MerkleV1).is_err());
        assert!(prompt_requests(1, PalwPromptIdsFormV1::Flat).unwrap() >= 4 + 512);
        assert!(
            prompt_requests(129, PalwPromptIdsFormV1::MerkleV1).unwrap()
                > prompt_requests(1, PalwPromptIdsFormV1::MerkleV1).unwrap()
        );
    }
    #[test]
    fn actual_registered_shape_quote_caps_before_derived_allocation() {
        use crate::config::params::{ForkActivation, PositiveCourtFenceV2};
        use misaka_palw_positive_contract::assignment::{
            AssignmentModeV2,
            policy::{AssignmentLiveCapsV2, canonical_assignment_derivation_id},
        };
        let execution = crate::palw_tir_court_v1::test_support::tiny_execution(None);
        let mut class = execution.binding.class;
        class.layout.max_context = 64;
        let (registered, _) = crate::palw_tir_admission_v1::palw_tir_class_record_v1(
            &class,
            &execution.binding.artifact_root,
        )
        .unwrap();
        let positive = PositiveCourtFenceV2::unapproved_probe_at(ForkActivation::new(1));
        let mut policy = AssignmentPolicyV2 {
            version: 2,
            revision: 1,
            mode: AssignmentModeV2::CanonicalTirAssignmentV1,
            positive_policy: Digest(positive.policy.root().as_bytes()),
            derivation_policy: canonical_assignment_derivation_id(),
            source_policy: Digest([1; 64]),
            assignment_fee_schedule: Digest([2; 64]),
            case_fee_schedule: Digest([3; 64]),
            reserve_schedule: Digest([4; 64]),
            physical_service_profile: Digest([5; 64]),
            cancellation_policy: Digest([6; 64]),
            live: AssignmentLiveCapsV2 {
                global: 8,
                per_poster: 2,
                per_worker: 2,
                per_prover: 2,
                per_class: 2,
            },
            max_frame_bytes: 32000,
            max_program_bytes: 65536,
            max_layout_entries: 1000,
            max_source_catalog_entries: 1000,
            max_role_candidates: 5,
            max_prover_joins: 2,
            max_history_frames: 100,
            max_history_bytes: 100000,
            max_retained_bytes: 200000,
            max_replay_bytes: 300000,
            max_replay_work: 1000000,
            max_heap_bytes: 10000000,
            accept_window_daa: 100,
            result_window_daa: 100,
            build_window_daa: 100,
            check_window_daa: 100,
            fraud_tail_daa: 100,
            refund_window_daa: 100,
            retention_window_daa: 1000,
        };
        let quote = AssignmentDerivedAllocationQuoteV2::from_registered(
            &registered,
            &class.layout,
            &policy,
            PalwPromptIdsFormV1::Flat,
        )
        .unwrap();
        assert!(quote.total_requests > registered.program.len() as u64);
        assert!(
            quote.source_program_and_validator > 0
                && quote.final_plan_step_space > 0
                && quote.retained_inventory_and_instance_analysis > 0
        );
        policy.max_heap_bytes = quote.total_requests - 1;
        assert!(matches!(
            AssignmentDerivedAllocationQuoteV2::from_registered(
                &registered,
                &class.layout,
                &policy,
                PalwPromptIdsFormV1::Flat
            ),
            Err(AssignmentResourceErrorV2::ConstructionCap { .. })
        ));
        policy.max_heap_bytes = quote.total_requests;
        assert_eq!(
            AssignmentDerivedAllocationQuoteV2::from_registered(
                &registered,
                &class.layout,
                &policy,
                PalwPromptIdsFormV1::Flat
            )
            .unwrap(),
            quote
        );
    }
}
