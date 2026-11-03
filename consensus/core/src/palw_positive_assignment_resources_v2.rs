//! Result-free, deterministic RFC-0045 native resource inputs. These complete
//! typed policy/price preimages and registry-derived counts; none is a native
//! lease, accepted origin, delivered service, P approval or money capability.
use crate::{
    Hash64,
    config::params::PositiveCourtPolicyV2,
    palw_positive_assignment_v2::{AssignmentBindingErrorV2, AssignmentJobBindingV2},
    palw_positive_check_v2::{CheckError, CheckLimits, CostEnvelope, CoveragePlan},
    palw_positive_metadata_v2::PositiveMetadataLimitsV2,
    palw_positive_source_geometry_v2::{GeometryLimitsV2, PositiveSourceGeometryErrorV2, SourceCatalogQuoteV2, TirSourceCatalogV2},
    palw_positive_source_v2::SourceLimitsV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_state_v2::PalwChainStateV2,
    palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, PalwTirClassV1, PalwTirLayoutV1},
    palw_tir_step_v1::PalwTirStepSpaceV1,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    assignment::{AssignmentTermsV2, policy::AssignmentPolicyV2},
    identity::Digest,
};
fn hash<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.update(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    borsh::to_writer(&mut writer, value).expect("fixed resource preimage");
    Digest(writer.0.finalize().as_bytes().try_into().expect("64"))
}
fn finite(values: &[u64]) -> bool {
    values.iter().all(|n| *n > 0 && *n < u64::MAX)
}

/// Deterministic ServiceCost work units. These coefficients are ORIGINAL
/// native policy inputs, never milliseconds or an empirical RSS assertion.
/// No released preset supplies values. Native execution must derive quantities
/// from its actual typed preflight/check counters and separately own capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentWorkScheduleV2 {
    pub version: u16,
    pub revision: u16,
    pub per_codec_step: u64,
    pub per_hash_byte: u64,
    pub per_signature: u64,
    pub per_element: u64,
    pub per_term: u64,
    pub per_io_byte: u64,
    pub per_control: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssignmentWorkQuantitiesV2 {
    pub codec_steps: u64,
    pub hash_bytes: u64,
    pub signatures: u64,
    pub elements: u64,
    pub terms: u64,
    pub io_bytes: u64,
    pub controls: u64,
}
/// Conservative signature precharge beyond the signature carried in the body.
pub(crate) const ASSIGNED_COURT_ROLE_SIGNATURES_V2: u64 = 3;

impl AssignmentWorkScheduleV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-work-schedule/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentResourceErrorV2> {
        if self.version != 2
            || self.revision != 1
            || !finite(&[
                self.per_codec_step,
                self.per_hash_byte,
                self.per_signature,
                self.per_element,
                self.per_term,
                self.per_io_byte,
                self.per_control,
            ])
        {
            return Err(AssignmentResourceErrorV2::Policy);
        }
        Ok(())
    }
    pub fn charge(&self, q: AssignmentWorkQuantitiesV2) -> Result<u64, AssignmentResourceErrorV2> {
        self.validate()?;
        [
            (self.per_codec_step, q.codec_steps),
            (self.per_hash_byte, q.hash_bytes),
            (self.per_signature, q.signatures),
            (self.per_element, q.elements),
            (self.per_term, q.terms),
            (self.per_io_byte, q.io_bytes),
            (self.per_control, q.controls),
        ]
        .into_iter()
        .try_fold(0u64, |total, (rate, amount)| {
            amount.checked_mul(rate).and_then(|n| total.checked_add(n)).ok_or(AssignmentResourceErrorV2::Budget)
        })
    }

    pub(crate) fn court_body_work(&self, body_bytes: u64) -> Result<u64, AssignmentResourceErrorV2> {
        self.charge(AssignmentWorkQuantitiesV2 {
            codec_steps: body_bytes,
            hash_bytes: body_bytes,
            signatures: 0,
            elements: 0,
            terms: 0,
            io_bytes: 0,
            controls: 1,
        })
    }

    pub(crate) fn court_operation_work(
        &self,
        carrier_signatures: u64,
        role_candidates: u64,
        elements: u64,
        terms: u64,
    ) -> Result<u64, AssignmentResourceErrorV2> {
        self.charge(AssignmentWorkQuantitiesV2 {
            codec_steps: 0,
            hash_bytes: 0,
            signatures: carrier_signatures,
            elements,
            terms,
            io_bytes: 0,
            controls: role_candidates,
        })
    }
}
/// Explicit initial source service mode, authenticated through the full
/// source-policy ID in the signed bundle/consent. Fee-only duties do not create
/// an unsigned collateral penalty; computation/certification remains separate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentSourceResponsibilityV2 {
    FeeOnlyV1,
}
/// No clock-only release. Native still establishes all signed outstanding
/// duties, earned fees, retention and applicable original liability at close.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentSourceReleaseV2 {
    AfterSignedDutiesRetentionAndLiabilityV1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentSourcePolicyV2 {
    pub version: u16,
    pub revision: u16,
    pub responsibility: AssignmentSourceResponsibilityV2,
    pub release: AssignmentSourceReleaseV2,
    pub work: AssignmentWorkScheduleV2,
    pub max_parameter_instances: u64,
    pub max_requests: u64,
    pub max_responses: u64,
    pub max_history_events: u64,
    pub max_history_bytes: u64,
    pub max_intervals: u64,
    pub max_units_per_request: u64,
    pub max_native_body_bytes: u64,
}
impl AssignmentSourcePolicyV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-source-policy/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentResourceErrorV2> {
        self.work.validate()?;
        if self.version != 2
            || self.revision != 1
            || !finite(&[
                self.max_parameter_instances,
                self.max_requests,
                self.max_responses,
                self.max_history_events,
                self.max_history_bytes,
                self.max_intervals,
                self.max_units_per_request,
                self.max_native_body_bytes,
            ])
            || self.max_requests.checked_add(self.max_responses).is_none_or(|n| n > self.max_history_events)
            || self.max_intervals > self.max_requests
            || self.max_native_body_bytes > self.max_history_bytes
        {
            return Err(AssignmentResourceErrorV2::Policy);
        }
        Ok(())
    }
    pub fn limits(&self) -> SourceLimitsV2 {
        SourceLimitsV2 {
            max_requests: self.max_requests,
            max_responses: self.max_responses,
            max_history_events: self.max_history_events,
            max_history_bytes: self.max_history_bytes,
            max_intervals: self.max_intervals,
            max_units_per_request: self.max_units_per_request,
            max_native_body_bytes: self.max_native_body_bytes,
        }
    }
}

/// Existing currencies only. No network values are assigned here. First-only
/// service rates are distinct from computation success and ordinary tx fees.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCaseFeeScheduleV2 {
    pub version: u16,
    pub revision: u16,
    pub admission: u64,
    pub per_source_unit: u64,
    pub per_source_opening_byte: u64,
    pub per_checked_relation: u64,
    pub per_record_byte: u64,
    pub capital_per_retained_byte: u64,
}
impl AssignmentCaseFeeScheduleV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-case-fee-schedule/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentResourceErrorV2> {
        if self.version != 2
            || self.revision != 1
            || !finite(&[
                self.admission,
                self.per_source_unit,
                self.per_source_opening_byte,
                self.per_checked_relation,
                self.per_record_byte,
                self.capital_per_retained_byte,
            ])
        {
            return Err(AssignmentResourceErrorV2::Policy);
        }
        Ok(())
    }
    pub fn source_unit_due(&self, opening_bytes: u64) -> Result<u64, AssignmentResourceErrorV2> {
        self.validate()?;
        opening_bytes
            .checked_mul(self.per_source_opening_byte)
            .and_then(|n| n.checked_add(self.per_source_unit))
            .ok_or(AssignmentResourceErrorV2::Budget)
    }
    /// The whole catalog can earn each unit fee once, even when read by
    /// multiple authorized roles or retransmitted.
    pub(crate) fn source_catalog_due(&self, source: &SourceCatalogQuoteV2) -> Result<u64, AssignmentResourceErrorV2> {
        self.validate()?;
        self.per_source_unit
            .checked_mul(source.units)
            .and_then(|n| self.per_source_opening_byte.checked_mul(source.opening_bytes).and_then(|x| n.checked_add(x)))
            .ok_or(AssignmentResourceErrorV2::Budget)
    }

    pub fn quote(
        &self,
        source: &SourceCatalogQuoteV2,
        attempt_relations: u64,
        record_bytes: u64,
        retained_bytes: u64,
    ) -> Result<AssignmentCaseMoneyQuoteV2, AssignmentResourceErrorV2> {
        let fee = self
            .source_catalog_due(source)?
            .checked_add(self.per_checked_relation.checked_mul(attempt_relations).ok_or(AssignmentResourceErrorV2::Budget)?)
            .and_then(|n| self.per_record_byte.checked_mul(record_bytes).and_then(|x| n.checked_add(x)))
            .and_then(|n| n.checked_add(self.admission))
            .ok_or(AssignmentResourceErrorV2::Budget)?;
        let capital = retained_bytes.checked_mul(self.capital_per_retained_byte).ok_or(AssignmentResourceErrorV2::Budget)?;
        fee.checked_add(capital).ok_or(AssignmentResourceErrorV2::Budget)?;
        Ok(AssignmentCaseMoneyQuoteV2 { fee, capital, schedule: self.id() })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCaseMoneyQuoteV2 {
    pub fee: u64,
    pub capital: u64,
    pub schedule: Digest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum AssignmentCancellationRuleV2 {
    PreserveEarnedAndLiabilityV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCancellationPolicyV2 {
    pub version: u16,
    pub revision: u16,
    pub rule: AssignmentCancellationRuleV2,
}
impl AssignmentCancellationPolicyV2 {
    pub fn id(&self) -> Digest {
        hash(b"misaka/positive/assignment-cancellation-policy/v2/r1", self)
    }
    pub fn validate(&self) -> Result<(), AssignmentResourceErrorV2> {
        if self.version != 2 || self.revision != 1 {
            return Err(AssignmentResourceErrorV2::Policy);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentResourceErrorV2 {
    Policy,
    Binding,
    MissingRegistry,
    Budget,
    Job(AssignmentBindingErrorV2),
    Geometry(PositiveSourceGeometryErrorV2),
    Check(CheckError),
    Native(crate::palw_native_resource_v1::NativeCoverageV1),
    MissingNativeLease,
    ConstructionCap { required: u64, maximum: u64 },
}
impl From<AssignmentBindingErrorV2> for AssignmentResourceErrorV2 {
    fn from(e: AssignmentBindingErrorV2) -> Self {
        Self::Job(e)
    }
}
impl From<PositiveSourceGeometryErrorV2> for AssignmentResourceErrorV2 {
    fn from(e: PositiveSourceGeometryErrorV2) -> Self {
        Self::Geometry(e)
    }
}
impl From<CheckError> for AssignmentResourceErrorV2 {
    fn from(e: CheckError) -> Self {
        Self::Check(e)
    }
}

/// Independently registry-derived full canonical work/source geometry. No wire
/// Result, Case label or saved pass is constructed before actual C admission.
pub struct AssignmentDerivedResourcesV2 {
    binding: AssignmentJobBindingV2,
    class: PalwTirClassV1,
    source: TirSourceCatalogV2,
    source_quote: SourceCatalogQuoteV2,
    plan: CoveragePlan,
    check_limits: CheckLimits,
    all_attempt_relations: u64,
    all_attempt_cost: CostEnvelope,
}
impl AssignmentDerivedResourcesV2 {
    /// Pure native-input derivation. The actual owner must reserve this entire
    /// construction/replay footprint BEFORE calling, then atomically reserve
    /// future all-branch resources and funds before installing any row.
    #[allow(clippy::too_many_arguments)]
    pub fn derive(
        state: &PalwChainStateV2,
        terms: AssignmentTermsV2,
        layout: &PalwTirLayoutV1,
        assignment: &AssignmentPolicyV2,
        positive: &PositiveCourtPolicyV2,
        source_policy: &AssignmentSourcePolicyV2,
        original_prompt_form: PalwPromptIdsFormV1,
    ) -> Result<Self, AssignmentResourceErrorV2> {
        assignment.validate_shape().map_err(|_| AssignmentResourceErrorV2::Policy)?;
        positive.validate_shape().map_err(|_| AssignmentResourceErrorV2::Policy)?;
        source_policy.validate()?;
        if assignment.positive_policy.0 != positive.root().as_bytes()
            || assignment.source_policy != source_policy.id()
            || positive.service_profile_id.as_bytes() != source_policy.id().0
            || positive.funding_schedule_id.as_bytes() != assignment.case_fee_schedule.0
        {
            return Err(AssignmentResourceErrorV2::Binding);
        }
        let registered =
            state.tir_class_v1(&Hash64::from_bytes(terms.registered_class.0)).ok_or(AssignmentResourceErrorV2::MissingRegistry)?;
        let native_class =
            state.class(&Hash64::from_bytes(terms.registered_class.0)).ok_or(AssignmentResourceErrorV2::MissingRegistry)?;
        let binding = AssignmentJobBindingV2::derive(
            state,
            terms,
            layout,
            PositiveMetadataLimitsV2 {
                max_program_bytes: assignment.max_program_bytes,
                max_layout_entries: assignment.max_layout_entries,
                max_prompt_tokens: registered.facts.max_context,
            },
            original_prompt_form,
        )?;
        let class = PalwTirClassV1 {
            version: PALW_TIR_CLASS_VERSION_V1,
            program: registered.program.as_slice().to_vec(),
            layout: layout.clone(),
            tokenizer_id: registered.tokenizer_id,
        };
        let source = TirSourceCatalogV2::derive(
            &class,
            registered,
            native_class.artifact_root,
            binding.job(),
            original_prompt_form,
            GeometryLimitsV2 {
                max_program_bytes: u64::from(assignment.max_program_bytes),
                max_parameter_instances: source_policy.max_parameter_instances,
                max_inventory_leaves: assignment.max_source_catalog_entries,
                max_units_per_range: source_policy.max_units_per_request,
            },
        )?;
        let source_quote = source.quote(
            terms.id(),
            Digest(binding.job().context_hash().as_bytes()),
            assignment.source_policy,
            assignment.max_source_catalog_entries,
        )?;
        let leaves = PalwTirStepSpaceV1::new(&class)
            .map_err(|_| AssignmentResourceErrorV2::Binding)?
            .leaf_count_capped(binding.job(), positive.max_relations)
            .map_err(|_| AssignmentResourceErrorV2::Budget)?;
        let all_attempt_relations =
            leaves.checked_mul(u64::from(positive.max_attempts)).filter(|n| *n > 0).ok_or(AssignmentResourceErrorV2::Budget)?;
        let check_limits = CheckLimits {
            max_leaves: positive.max_relations,
            max_part_bytes: positive.max_frame_bytes,
            max_part_heap: positive.max_part_heap,
            max_total_bytes: positive.max_all_attempt_bytes / u64::from(positive.max_attempts),
            max_elements_per_leaf: positive.max_all_attempt_elements / all_attempt_relations,
            max_terms_per_leaf: positive.max_all_attempt_terms / all_attempt_relations,
            max_total_elements: positive.max_all_attempt_elements / u64::from(positive.max_attempts),
            max_total_terms: positive.max_all_attempt_terms / u64::from(positive.max_attempts),
        };
        let plan = CoveragePlan::derive(&class, native_class.artifact_root, binding.job(), original_prompt_form, check_limits)?;
        let cost = plan.cost();
        let mul = |n: u64| n.checked_mul(u64::from(positive.max_attempts)).ok_or(AssignmentResourceErrorV2::Budget);
        let all_attempt_cost = CostEnvelope {
            parts: mul(cost.parts)?,
            encoded_bytes: mul(cost.encoded_bytes)?,
            evaluation_elements: mul(cost.evaluation_elements)?,
            evaluation_terms: mul(cost.evaluation_terms)?,
            decoded_part_bytes: cost.decoded_part_bytes,
        };
        Ok(Self { binding, class, source, source_quote, plan, check_limits, all_attempt_relations, all_attempt_cost })
    }
    pub fn binding(&self) -> &AssignmentJobBindingV2 {
        &self.binding
    }
    pub fn class(&self) -> &PalwTirClassV1 {
        &self.class
    }
    pub fn source(&self) -> &TirSourceCatalogV2 {
        &self.source
    }
    pub fn source_quote(&self) -> SourceCatalogQuoteV2 {
        self.source_quote
    }
    pub fn plan(&self) -> &CoveragePlan {
        &self.plan
    }
    pub fn check_limits(&self) -> CheckLimits {
        self.check_limits
    }
    pub fn all_attempt_relations(&self) -> u64 {
        self.all_attempt_relations
    }
    /// The same complete frames and ML-DSA packet signatures reserved by
    /// ContractCourtV2 when the first candidate opens.
    pub(crate) fn all_attempt_signed_frame_bytes_v2(&self) -> Result<u64, AssignmentResourceErrorV2> {
        let quote = crate::palw_positive_contract_v2::signed_quote(self.all_attempt_relations, self.check_limits.max_part_bytes)
            .map_err(|_| AssignmentResourceErrorV2::Budget)?;
        Ok(quote.total_packet_bytes)
    }
    /// Work reserved by the native replay meter for every Witness and exact
    /// frame at the admitted bounds. Other operations use the same live meter.
    pub(crate) fn check_court_replay_work_v2(
        &self,
        schedule: &AssignmentWorkScheduleV2,
        assignment: &AssignmentPolicyV2,
        positive: &PositiveCourtPolicyV2,
    ) -> Result<(), AssignmentResourceErrorV2> {
        use crate::palw_positive_court_v2::coordinator::native_body::{EXACT_FRAME_BODY_OVERHEAD_V2, WITNESS_BODY_OVERHEAD_V2};

        let budget = || AssignmentResourceErrorV2::Budget;
        let frame_bytes = self.check_limits.max_part_bytes.checked_add(EXACT_FRAME_BODY_OVERHEAD_V2).ok_or_else(budget)?;
        let witness_bytes = misaka_palw_positive_contract::record::wire_size(self.binding.job().exact_decode_tokens)
            .map_err(|_| budget())?
            .checked_add(WITNESS_BODY_OVERHEAD_V2)
            .ok_or_else(budget)?;
        let frame_work = schedule
            .court_body_work(frame_bytes)?
            .checked_add(schedule.court_operation_work(
                1 + ASSIGNED_COURT_ROLE_SIGNATURES_V2,
                u64::from(assignment.max_role_candidates),
                self.check_limits.max_elements_per_leaf,
                self.check_limits.max_terms_per_leaf,
            )?)
            .ok_or_else(budget)?;
        let witness_work = schedule
            .court_body_work(witness_bytes)?
            .checked_add(schedule.court_operation_work(
                1 + ASSIGNED_COURT_ROLE_SIGNATURES_V2,
                u64::from(assignment.max_role_candidates),
                0,
                0,
            )?)
            .ok_or_else(budget)?;
        let all_frame_work = frame_work.checked_mul(self.all_attempt_relations).ok_or_else(budget)?;
        let all_witness_work = witness_work.checked_mul(u64::from(positive.max_attempts)).ok_or_else(budget)?;
        let work = all_frame_work.checked_add(all_witness_work).ok_or_else(budget)?;
        if work > positive.max_case_work.min(assignment.max_replay_work) {
            return Err(budget());
        }
        Ok(())
    }

    pub fn all_attempt_cost(&self) -> CostEnvelope {
        self.all_attempt_cost
    }
}

mod allocation;
pub use allocation::{AssignmentDerivedAllocationQuoteV2, ReservedAssignmentDerivedResourcesV2};
