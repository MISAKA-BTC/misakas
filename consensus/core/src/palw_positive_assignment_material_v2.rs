//! Immutable ORIGINAL admission inputs for RFC-0045. All retained values below
//! are fixed-size data. No full Params or allocating job/context Deserialize is
//! accepted, no Result is fabricated and no saved Pass is restored.
//!
//! A private native VP factory must verify the actual current rooted state,
//! accepted transaction/prefix/ancestry and custody owners before using these
//! data. This module supplies no native admission, fund or credit capability.
use crate::{
    Hash64,
    config::params::{
        AssignmentFundingRelayProfileV2, AssignmentNativePreimagesV2, PositiveCourtPolicyV2,
    },
    palw_positive_assignment_certification_v2::AssignmentGateCDrawEpochsV2,
    palw_positive_assignment_funds_v2::{AssignmentMoneyQuoteV2, quote_assignment_money_v2},
    palw_positive_assignment_horizon_v2::AssignmentLiabilityHorizonV2,
    palw_positive_source_geometry_v2::SourceCatalogQuoteV2,
    palw_prompt_ids_v1::PalwPromptIdsFormV1,
    palw_security_certification_v1::PalwSecurityCertificationPolicyV1,
    palw_v2::PalwJobContextV2,
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    assignment::{
        AssignmentId,
        open_material::{
            ASSIGNMENT_OPEN_PREIMAGES_V2, AssignmentOpenLimitsV2, AssignmentOpenPreimageKindV2,
            AssignmentOpenViewV2,
        },
        policy::AssignmentPolicyV2,
        rows::AssignmentRow,
    },
    identity::Digest,
    rows::{AcceptedMaterial, AcceptedOrigin, NativeHash64},
};

fn digest<T: BorshSerialize>(domain: &[u8], value: &T) -> Digest {
    struct Writer(blake2b_simd::State);
    impl std::io::Write for Writer {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0.update(b);
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut writer = Writer(
        blake2b_simd::Params::new()
            .hash_length(64)
            .key(domain)
            .to_state(),
    );
    value
        .serialize(&mut writer)
        .expect("fixed retained material serialization");
    Digest(
        writer
            .0
            .finalize()
            .as_bytes()
            .try_into()
            .expect("64-byte digest"),
    )
}
pub fn assignment_custody_bytes_id_v2(bytes: &[u8]) -> Digest {
    digest(b"misaka/positive/assignment-custody-bytes/v2/r1", &bytes)
}

/// Current canonical TIR jobs use the fixed native network literal. This fixed
/// descriptor still retains its exact bytes and length. A future longer job
/// network needs an explicit new retained codec, never an unbounded Vec decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentJobDescriptorV2 {
    pub version: u16,
    pub network_length: u8,
    pub network: [u8; 64],
    pub job_id: Hash64,
    pub job_nullifier: Hash64,
    pub assignment_id: Hash64,
    pub execution_seed: [u8; 32],
    pub model_profile_id: Hash64,
    pub runtime_manifest_hash: Hash64,
    pub runtime_class_id: Hash64,
    pub shape_profile_id: Hash64,
    pub trace_scheme_id: Hash64,
    pub cu_ruleset_id: Hash64,
    pub tokenizer_id: Hash64,
    pub prompt_token_ids_hash: Hash64,
    pub declared_prefill_tokens: u32,
    pub exact_decode_tokens: u32,
    pub max_context_tokens: u32,
    pub context_hash: Hash64,
}
impl AssignmentJobDescriptorV2 {
    pub fn from_native(job: &PalwJobContextV2) -> Result<Self, ContractError> {
        if job.version != 2 || job.network_id.is_empty() || job.network_id.len() > 64 {
            return Err(ContractError::Budget);
        }
        let mut network = [0; 64];
        network[..job.network_id.len()].copy_from_slice(&job.network_id);
        Ok(Self {
            version: job.version,
            network_length: job.network_id.len() as u8,
            network,
            job_id: job.job_id,
            job_nullifier: job.job_nullifier,
            assignment_id: job.assignment_id,
            execution_seed: job.execution_seed,
            model_profile_id: job.model_profile_id,
            runtime_manifest_hash: job.runtime_manifest_hash,
            runtime_class_id: job.runtime_class_id,
            shape_profile_id: job.shape_profile_id,
            trace_scheme_id: job.trace_scheme_id,
            cu_ruleset_id: job.cu_ruleset_id,
            tokenizer_id: job.tokenizer_id,
            prompt_token_ids_hash: job.prompt_token_ids_hash,
            declared_prefill_tokens: job.declared_prefill_tokens,
            exact_decode_tokens: job.exact_decode_tokens,
            max_context_tokens: job.max_context_tokens,
            context_hash: job.context_hash(),
        })
    }
    fn validate(&self) -> Result<(), ContractError> {
        let n = usize::from(self.network_length);
        if self.version != 2
            || n == 0
            || n > 64
            || self.network[n..].iter().any(|b| *b != 0)
            || self.declared_prefill_tokens == 0
            || self.exact_decode_tokens == 0
            || self.max_context_tokens == 0
            || self
                .declared_prefill_tokens
                .checked_add(self.exact_decode_tokens)
                .is_none_or(|n| n > self.max_context_tokens)
        {
            return Err(ContractError::NonCanonical);
        }
        Ok(())
    }
    /// Full equality against independent native derivation, including native
    /// network bytes, original max-context/CU profile and all seed/count fields.
    pub fn check_native(&self, job: &PalwJobContextV2) -> Result<(), ContractError> {
        self.validate()?;
        if Self::from_native(job)? != *self {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentRegisteredDescriptorV2 {
    pub class: Digest,
    pub artifact: Digest,
    pub program: Digest,
    pub layout: Digest,
    pub registered_tokenizer: Digest,
    /// Actual original registration material, not current class eligibility.
    pub registration: AcceptedMaterial,
}
impl AssignmentRegisteredDescriptorV2 {
    pub fn id(&self) -> Digest {
        digest(
            b"misaka/positive/assignment-registered-metadata/v2/r1",
            self,
        )
    }

    pub(crate) fn check_rooted_class_v2(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
    ) -> Result<(), ContractError> {
        use crate::palw_class_registration_origin_v1::{
            PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1, PalwClassRegistrationDoorV1,
            palw_class_registration_origin_row_v1,
        };

        let class_id = Hash64::from_bytes(self.class.0);
        let class = state.class(&class_id).ok_or(ContractError::Identity)?;
        let tir = state
            .tir_class_v1(&class_id)
            .ok_or(ContractError::Identity)?;
        let origin = self.registration.origin;
        if tir.facts.class_id != class_id
            || class.artifact_root.as_bytes() != self.artifact.0
            || tir.graph_ir_root.as_bytes() != self.program.0
            || tir.layout_digest.as_bytes() != self.layout.0
            || tir.tokenizer_id.as_bytes() != self.registered_tokenizer.0
            || class.registered_daa != origin.accepted_daa
        {
            return Err(ContractError::Identity);
        }
        if let Some(saved) =
            palw_class_registration_origin_row_v1(state.kernel_route_state_v1(), &class_id)
            && (saved.version != PALW_CLASS_REGISTRATION_ORIGIN_ROW_VERSION_V1
                || saved.door != PalwClassRegistrationDoorV1::Tir
                || saved.class_id != class_id
                || saved.daa_score != origin.accepted_daa
                || saved.block.as_bytes() != origin.accepting_block.0
                || saved
                    .admission_digest
                    .map(|digest| Digest(digest.as_bytes()))
                    != Some(self.registration.digest))
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentSourceQuoteDescriptorV2 {
    pub digest: Digest,
    pub units: u64,
    pub data_bytes: u64,
    pub opening_bytes: u64,
    pub max_unit_bytes: u64,
}
impl From<SourceCatalogQuoteV2> for AssignmentSourceQuoteDescriptorV2 {
    fn from(q: SourceCatalogQuoteV2) -> Self {
        Self {
            digest: q.digest,
            units: q.units,
            data_bytes: q.data_bytes,
            opening_bytes: q.opening_bytes,
            max_unit_bytes: q.max_unit_bytes,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentCustodyDescriptorV2 {
    pub reservation: AcceptedMaterial,
    pub liability: AcceptedMaterial,
    /// Byte-identity hashes preserve exact native owner preimages. They do NOT
    /// replace that owner's purpose/domain/state validation or live capacity.
    pub preimage_bytes: [Digest; ASSIGNMENT_OPEN_PREIMAGES_V2],
    pub retention_until_daa: u64,
    pub retained_bytes: u64,
    pub replay_bytes: u64,
    pub replay_work: u64,
    pub heap_bytes: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentAdmissionMaterialV2 {
    pub version: u16,
    pub revision: u16,
    pub assignment: AssignmentId,
    pub unsigned_bundle: Digest,
    pub origin: AcceptedOrigin,
    pub network: Digest,
    pub ruleset: Digest,
    pub fired_rules: Digest,
    pub branch_anchor: NativeHash64,
    pub assignment_policy: AssignmentPolicyV2,
    pub positive_policy: PositiveCourtPolicyV2,
    pub native_preimages: AssignmentNativePreimagesV2,
    pub funding_relay: AssignmentFundingRelayProfileV2,
    pub certification_policy: PalwSecurityCertificationPolicyV1,
    pub network_prompt_form: PalwPromptIdsFormV1,
    pub class_prompt_form: PalwPromptIdsFormV1,
    pub job: AssignmentJobDescriptorV2,
    pub registered: AssignmentRegisteredDescriptorV2,
    pub source: AssignmentSourceQuoteDescriptorV2,
    pub money: AssignmentMoneyQuoteV2,
    pub custody: AssignmentCustodyDescriptorV2,
}
impl AssignmentAdmissionMaterialV2 {
    /// Assemble the frozen Open material from the validated native inputs.
    /// The Open encoder binds the final reservation and liability pins.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_native_open_v2(
        row: &AssignmentRow,
        rules: &crate::palw_state_v2::NativeAssignmentRulesV2,
        resources: &crate::palw_positive_assignment_resources_v2::AssignmentDerivedResourcesV2,
        registered: AssignmentRegisteredDescriptorV2,
        money: AssignmentMoneyQuoteV2,
        class_prompt_form: PalwPromptIdsFormV1,
        replay_work: u64,
    ) -> Result<Self, ContractError> {
        let native_preimages = rules
            .native_preimages()
            .ok_or(ContractError::Scope)?
            .clone();
        let certification_policy = *rules.certification_policy().ok_or(ContractError::Scope)?;
        let terms = &row.bundle.terms;
        let job = AssignmentJobDescriptorV2::from_native(resources.binding().job())?;
        let source = AssignmentSourceQuoteDescriptorV2::from(resources.source_quote());
        if job.job_id.as_bytes() != row.job.0
            || job.prompt_token_ids_hash.as_bytes() != row.input.0
            || job.context_hash.as_bytes() != row.context.0
            || registered.id() != row.registered_metadata
            || source.digest != row.bundle.source_catalog
            || money.id() != row.bundle.quote
            || replay_work == 0
            || replay_work > rules.assignment_policy().max_replay_work
        {
            return Err(ContractError::Identity);
        }
        Ok(Self {
            version: 2,
            revision: 1,
            assignment: row.id(),
            unsigned_bundle: row.bundle.id(),
            origin: row.origin,
            network: terms.network,
            ruleset: terms.ruleset,
            fired_rules: row.fired_rules,
            branch_anchor: terms.branch_anchor,
            assignment_policy: *rules.assignment_policy(),
            positive_policy: rules.positive_policy().clone(),
            native_preimages,
            funding_relay: rules.funding_relay_profile().clone(),
            certification_policy,
            network_prompt_form: rules.prompt_form(),
            class_prompt_form,
            job,
            registered,
            source,
            money,
            custody: AssignmentCustodyDescriptorV2 {
                reservation: row.reservation,
                liability: row.liability,
                preimage_bytes: [Digest([0; 64]); ASSIGNMENT_OPEN_PREIMAGES_V2],
                retention_until_daa: terms.reserve_until_daa,
                retained_bytes: money.usage.retained_bytes,
                replay_bytes: money.usage.replay_bytes,
                replay_work,
                heap_bytes: money.usage.heap_bytes,
            },
        })
    }

    pub fn id(&self) -> Digest {
        digest(b"misaka/positive/assignment-admission-material/v2/r1", self)
    }
    /// Fixed bounded decoder; there is no arbitrary Params or collection decode.
    pub fn decode(raw: &[u8], max_bytes: u64) -> Result<Self, ContractError> {
        if max_bytes == 0 || max_bytes == u64::MAX || raw.is_empty() || raw.len() as u64 > max_bytes
        {
            return Err(ContractError::Budget);
        }
        let structural = crate::palw_native_resource_v1::preflight_native_value_v1::<Self>(
            raw,
            raw.len() as u64,
        )
        .map_err(|_| ContractError::NonCanonical)?;
        if structural.logical_requests_v1() != std::mem::size_of::<Self>() as u64 {
            return Err(ContractError::Budget);
        }
        let value = borsh::from_slice::<Self>(raw).map_err(|_| ContractError::NonCanonical)?;
        Ok(value)
    }
    /// Unique retained history must also fit two complete reads during replay.
    pub fn native_assignment_journal_storage_limit_v2(&self) -> u64 {
        self.assignment_policy
            .max_history_bytes
            .min(self.money.usage.retained_bytes)
            .min(self.money.usage.replay_bytes / 2)
    }

    /// The framed native record includes the actual control envelope and
    /// journal header. Both retention and replay must pay for those bytes.
    pub fn check_native_open_record_extent_v2(
        &self,
        payload_bytes: u64,
    ) -> Result<(), ContractError> {
        use misaka_palw_positive_contract::assignment::journal::ASSIGNMENT_JOURNAL_HEADER_BYTES_V2;

        let record_bytes = payload_bytes
            .checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64)
            .ok_or(ContractError::Budget)?;
        if payload_bytes == 0
            || payload_bytes > self.assignment_policy.max_frame_bytes
            || record_bytes > self.native_assignment_journal_storage_limit_v2()
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }

    /// Reprice the literal native operation under its original policy.
    pub fn check_native_open_quote_v2(
        &self,
        row: &AssignmentRow,
        body: &[u8],
    ) -> Result<(), ContractError> {
        let (quote, _) = crate::palw_positive_assignment_funds_v2::quote_native_assignment_open_v2(
            &self.assignment_policy,
            &row.bundle.terms,
            &self.native_preimages.assignment_fees,
            &self.native_preimages.physical,
            body,
            row.origin.accepted_daa,
        )
        .map_err(|_| ContractError::Budget)?;
        if quote != self.money {
            return Err(ContractError::Identity);
        }
        Ok(())
    }

    /// Compare the retained class descriptor with the actual original registry.
    /// If the original chain has an aux141 origin, it also pins the accepting
    /// block and admission digest. The carrier reader owns the transaction proof.
    pub fn check_registered_class_v2(
        &self,
        state: &crate::palw_state_v2::PalwChainStateV2,
    ) -> Result<(), ContractError> {
        if self.registered.registration.origin.accepted_daa > self.origin.accepted_daa {
            return Err(ContractError::Identity);
        }
        self.registered.check_rooted_class_v2(state)
    }

    pub fn check_row(&self, row: &AssignmentRow) -> Result<(), ContractError> {
        row.validate_shape()?;
        self.assignment_policy
            .check_terms(&row.bundle.terms, self.origin.accepted_daa)?;
        self.positive_policy
            .validate_shape()
            .map_err(|_| ContractError::Scope)?;
        self.native_preimages
            .validate(&self.assignment_policy)
            .map_err(|_| ContractError::Scope)?;
        self.funding_relay
            .validate_original_scope(self.fired_rules)
            .map_err(|_| ContractError::Scope)?;
        self.certification_policy
            .validate()
            .map_err(|_| ContractError::Scope)?;
        self.job.validate()?;
        if self.version != 2 || self.revision != 1 {
            return Err(ContractError::Version);
        }
        if self.assignment != row.id()
            || self.unsigned_bundle != row.bundle.id()
            || self.origin != row.origin
            || self.network != row.bundle.terms.network
            || self.ruleset != row.bundle.terms.ruleset
            || self.fired_rules != row.fired_rules
            || self.branch_anchor != row.bundle.terms.branch_anchor
            || self.assignment_policy.id() != row.assignment_policy
            || self.positive_policy.root().as_bytes() != row.positive_policy.0
            || self.assignment_policy.positive_policy != row.positive_policy
            || self.positive_policy.service_profile_id.as_bytes()
                != self.native_preimages.source_policy.id().0
            || self.positive_policy.funding_schedule_id.as_bytes()
                != self.native_preimages.case_fees.id().0
            || self.job.job_id.as_bytes() != row.job.0
            || self.job.prompt_token_ids_hash.as_bytes() != row.input.0
            || self.job.context_hash.as_bytes() != row.context.0
            || self.job.job_id.as_bytes() != row.id().0.0
            || self.registered.class != row.bundle.terms.registered_class
            || self.registered.id() != row.registered_metadata
            || self.source.digest != row.bundle.source_catalog
            || self.money.id() != row.bundle.quote
            || self.custody.reservation != row.reservation
            || self.custody.liability != row.liability
            || self.custody.retention_until_daa < row.bundle.terms.reserve_until_daa
            || self.custody.retention_until_daa == u64::MAX
            || self
                .custody
                .retention_until_daa
                .checked_sub(row.origin.accepted_daa)
                .is_none_or(|n| n > self.native_preimages.physical.max_retention_daa)
            || row.admission_material
                != (AcceptedMaterial {
                    origin: row.origin,
                    digest: self.id(),
                })
        {
            return Err(ContractError::Identity);
        }
        if self.custody.retained_bytes < self.money.usage.retained_bytes
            || self.custody.replay_bytes < self.money.usage.replay_bytes
            || self.custody.heap_bytes < self.money.usage.heap_bytes
            || self.custody.replay_work == 0
            || self.custody.replay_work > self.assignment_policy.max_replay_work
            || self.source.units == 0
            || self.source.units > u64::from(self.assignment_policy.max_source_catalog_entries)
            || self.source.data_bytes == 0
            || self.source.opening_bytes < self.source.data_bytes
            || self.source.max_unit_bytes == 0
        {
            return Err(ContractError::Budget);
        }
        let fixed_bytes =
            borsh::object_length(self).map_err(|_| ContractError::NonCanonical)? as u64;
        if self.money.usage.retained_bytes < fixed_bytes
            || self.money.usage.replay_bytes < fixed_bytes
            || self.money.usage.heap_bytes < (std::mem::size_of::<Self>() as u64)
        {
            return Err(ContractError::Budget);
        }
        let money = quote_assignment_money_v2(
            &self.assignment_policy,
            &row.bundle.terms,
            &self.native_preimages.assignment_fees,
            &self.native_preimages.physical,
            self.money.usage,
            self.origin.accepted_daa,
        )
        .map_err(|_| ContractError::Budget)?;
        if money != self.money {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
pub(crate) const ASSIGNMENT_COHORT_MAGIC_V2: &[u8; 8] = b"PC2ACOH1";
pub(crate) const ASSIGNMENT_COHORT_HEADER_V2: usize = ASSIGNMENT_COHORT_MAGIC_V2.len() + 4;
pub(crate) const ASSIGNMENT_COHORT_KEY_BYTES_V2: usize = 64 + 4;

pub(crate) fn split_assignment_reservation_v2(
    bytes: &[u8],
) -> Result<(&[[u8; ASSIGNMENT_COHORT_KEY_BYTES_V2]], &[u8]), ContractError> {
    let header = bytes
        .get(..ASSIGNMENT_COHORT_HEADER_V2)
        .ok_or(ContractError::NonCanonical)?;
    if !header.starts_with(ASSIGNMENT_COHORT_MAGIC_V2) {
        return Err(ContractError::NonCanonical);
    }
    let count = u32::from_le_bytes(
        header[ASSIGNMENT_COHORT_MAGIC_V2.len()..]
            .try_into()
            .expect("fixed count width"),
    );
    let keys_bytes = (count as usize)
        .checked_mul(ASSIGNMENT_COHORT_KEY_BYTES_V2)
        .ok_or(ContractError::Budget)?;
    let (keys, reservation) = bytes[ASSIGNMENT_COHORT_HEADER_V2..]
        .split_at_checked(keys_bytes)
        .ok_or(ContractError::NonCanonical)?;
    let (keys, []) = keys.as_chunks::<ASSIGNMENT_COHORT_KEY_BYTES_V2>() else {
        unreachable!("fixed key width")
    };
    Ok((keys, reservation))
}

/// Retained alongside the exact verifier cohort in the original Open record.
/// The bounds are selected by the original native quote and signed duties.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentReservationCustodyV2 {
    version: u16,
    revision: u16,
    assignment: AssignmentId,
    origin: AcceptedOrigin,
    quote: Digest,
    source: Digest,
    prover: Digest,
    retention_until_daa: u64,
    journal_bytes: u64,
    replay_work: u64,
}

impl AssignmentReservationCustodyV2 {
    pub fn derive(row: &AssignmentRow, material: &AssignmentAdmissionMaterialV2) -> Self {
        Self {
            version: 2,
            revision: 1,
            assignment: row.id(),
            origin: row.origin,
            quote: material.money.id(),
            source: row.source_consent.id(),
            prover: row.prover_consent.id(),
            retention_until_daa: material.custody.retention_until_daa,
            journal_bytes: material.native_assignment_journal_storage_limit_v2(),
            replay_work: material.custody.replay_work,
        }
    }
}

/// Construct the original Open bytes from independently derived admission
/// inputs. The caller holds the native capacity owners for these bytes and the
/// admitted duties until the Open checkpoint completes.
pub(crate) fn encode_native_assignment_open_v2(
    mut row: AssignmentRow,
    mut material: AssignmentAdmissionMaterialV2,
    native_body: &[u8],
    cohort: &[u8],
    limits: AssignmentOpenLimitsV2,
) -> Result<(AssignmentRow, Vec<u8>), ContractError> {
    use misaka_palw_positive_contract::assignment::open_material::encode_assignment_open_v2;

    let (_, remainder) = split_assignment_reservation_v2(cohort)?;
    if !remainder.is_empty() {
        return Err(ContractError::NonCanonical);
    }
    let reservation = borsh::to_vec(&AssignmentReservationCustodyV2::derive(&row, &material))
        .map_err(|_| ContractError::NonCanonical)?;
    let reservation_len = cohort
        .len()
        .checked_add(reservation.len())
        .ok_or(ContractError::Budget)?;
    if reservation_len as u64 > limits.max_preimage_bytes {
        return Err(ContractError::Budget);
    }
    let mut combined = Vec::new();
    combined
        .try_reserve_exact(reservation_len)
        .map_err(|_| ContractError::Budget)?;
    combined.extend_from_slice(cohort);
    combined.extend_from_slice(&reservation);
    let liability = borsh::to_vec(&AssignmentLiabilityCustodyV2::derive(&row, &material)?)
        .map_err(|_| ContractError::NonCanonical)?;
    material.custody.preimage_bytes = [
        assignment_custody_bytes_id_v2(&combined),
        assignment_custody_bytes_id_v2(&liability),
    ];
    material.custody.reservation = AcceptedMaterial {
        origin: row.origin,
        digest: material.custody.preimage_bytes[0],
    };
    material.custody.liability = AcceptedMaterial {
        origin: row.origin,
        digest: material.custody.preimage_bytes[1],
    };
    row.reservation = material.custody.reservation;
    row.liability = material.custody.liability;
    row.admission_material = AcceptedMaterial {
        origin: row.origin,
        digest: material.id(),
    };
    let encoded = borsh::to_vec(&material).map_err(|_| ContractError::NonCanonical)?;
    let open = encode_assignment_open_v2(&encoded, native_body, [&combined, &liability], limits)?;
    AssignmentAdmissionMaterialViewV2::decode(&row, &open, native_body, limits)?;
    Ok((row, open))
}

/// The C reservation keeps the exact A custody and links it to the actual
/// Result origin, signed service, original Case coin and maturity.
pub(crate) fn assigned_case_reservation_v2(
    assignment: &AssignmentRow,
    origin: AcceptedOrigin,
    case: &misaka_palw_positive_contract::rows::CaseRow,
    funding: &misaka_palw_positive_contract::rows::FundingRow,
) -> AcceptedMaterial {
    let commitment = (
        *b"PC2ACASE",
        assignment.id(),
        assignment.reservation,
        case.case,
        case.result,
        case.service.id(),
        case.maturity_daa,
        funding.consent.id(),
        funding.original_outpoint,
        origin,
    );
    AcceptedMaterial {
        origin,
        digest: digest(
            b"misaka/positive/assignment-custody-bytes/v2/r1",
            &commitment,
        ),
    }
}

/// Literal liability custody in the Open journal. The epoch interval and
/// release limits are derived under the actual original rule set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct AssignmentLiabilityCustodyV2 {
    version: u16,
    revision: u16,
    assignment: AssignmentId,
    origin: AcceptedOrigin,
    fired_rules: Digest,
    first_draw_epoch: u64,
    last_draw_epoch: u64,
    horizon: AssignmentLiabilityHorizonV2,
}

impl AssignmentLiabilityCustodyV2 {
    pub fn derive(
        row: &AssignmentRow,
        material: &AssignmentAdmissionMaterialV2,
    ) -> Result<Self, ContractError> {
        row.source_consent.check(&row.bundle)?;
        row.prover_consent
            .check(&row.bundle, material.assignment_policy.positive_policy)?;
        let controls = material
            .assignment_policy
            .max_replay_work
            .checked_div(material.native_preimages.source_policy.work.per_control)
            .ok_or(ContractError::Budget)?;
        let epochs = AssignmentGateCDrawEpochsV2::from_result_window(
            &material.certification_policy,
            row.origin.accepted_daa,
            row.bundle.terms.result_before_daa,
            controls,
        )
        .map_err(|_| ContractError::Budget)?
        .epochs();
        let horizon = AssignmentLiabilityHorizonV2::derive(
            &row.bundle,
            &row.source_consent,
            &row.prover_consent,
            &material.assignment_policy,
            &material.certification_policy,
            material.positive_policy.max_retention_daa,
            row.origin.accepted_daa,
        )
        .map_err(|_| ContractError::Budget)?;
        Ok(Self {
            version: 2,
            revision: 1,
            assignment: row.id(),
            origin: row.origin,
            fired_rules: material.fired_rules,
            first_draw_epoch: *epochs.start(),
            last_draw_epoch: *epochs.end(),
            horizon,
        })
    }
}

/// Borrowed original material and literal carrier. No admission or replay-pass
/// capability can be constructed from this DTO; native replay uses real owners.
pub struct AssignmentAdmissionMaterialViewV2<'a> {
    pub material: AssignmentAdmissionMaterialV2,
    pub open: AssignmentOpenViewV2<'a>,
}
impl<'a> AssignmentAdmissionMaterialViewV2<'a> {
    pub fn decode(
        row: &AssignmentRow,
        raw: &'a [u8],
        actual_native_op8: &[u8],
        limits: AssignmentOpenLimitsV2,
    ) -> Result<Self, ContractError> {
        let open = AssignmentOpenViewV2::decode(raw, limits)?;
        if open.native_body() != actual_native_op8 {
            return Err(ContractError::Identity);
        }
        let material =
            AssignmentAdmissionMaterialV2::decode(open.material(), limits.max_material_bytes)?;
        material.check_row(row)?;
        if material.money.usage.retained_bytes < raw.len() as u64
            || material.money.usage.replay_bytes < raw.len() as u64
            || material.money.usage.admission_bytes < actual_native_op8.len() as u64
        {
            return Err(ContractError::Budget);
        }
        for (expected, actual) in material.custody.preimage_bytes.iter().zip(open.preimages()) {
            if *expected != assignment_custody_bytes_id_v2(actual) {
                return Err(ContractError::Identity);
            }
        }
        let reservation_kind = AssignmentOpenPreimageKindV2::ReservationCustody;
        let (_, reservation) = split_assignment_reservation_v2(open.preimage(reservation_kind))?;
        let actual = borsh::from_slice::<AssignmentReservationCustodyV2>(reservation)
            .map_err(|_| ContractError::NonCanonical)?;
        let expected = AssignmentReservationCustodyV2::derive(row, &material);
        let reservation_pin = AcceptedMaterial {
            origin: row.origin,
            digest: material.custody.preimage_bytes[reservation_kind as usize],
        };
        if actual != expected || material.custody.reservation != reservation_pin {
            return Err(ContractError::Identity);
        }
        let liability_kind = AssignmentOpenPreimageKindV2::LiabilityCustody;
        let actual =
            borsh::from_slice::<AssignmentLiabilityCustodyV2>(open.preimage(liability_kind))
                .map_err(|_| ContractError::NonCanonical)?;
        let expected = AssignmentLiabilityCustodyV2::derive(row, &material)?;
        let liability_pin = AcceptedMaterial {
            origin: row.origin,
            digest: material.custody.preimage_bytes[liability_kind as usize],
        };
        if actual != expected || material.custody.liability != liability_pin {
            return Err(ContractError::Identity);
        }
        Ok(Self { material, open })
    }
    /// Compare independent reconstructed metadata; never copy these DTO values
    /// into an expected native class/job/source quote to make this check pass.
    pub fn check_derivation(
        &self,
        job: &PalwJobContextV2,
        registered: AssignmentRegisteredDescriptorV2,
        source: SourceCatalogQuoteV2,
    ) -> Result<(), ContractError> {
        self.material.job.check_native(job)?;
        if registered != self.material.registered
            || AssignmentSourceQuoteDescriptorV2::from(source) != self.material.source
        {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
