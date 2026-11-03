//! Immutable ORIGINAL admission inputs for RFC-0045. All retained values below
//! are fixed-size data. No full Params or allocating job/context Deserialize is
//! accepted, no Result is fabricated and no saved Pass is restored.
//!
//! A private native VP factory must verify the actual current rooted state,
//! accepted transaction/prefix/ancestry and custody owners before using these
//! data. This module supplies no native admission, fund or credit capability.
use crate::{
    Hash64,
    config::params::{AssignmentFundingRelayProfileV2, AssignmentNativePreimagesV2, PositiveCourtPolicyV2},
    palw_positive_assignment_funds_v2::{AssignmentMoneyQuoteV2, quote_assignment_money_v2},
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
        open_material::{ASSIGNMENT_OPEN_PREIMAGES_V2, AssignmentOpenLimitsV2, AssignmentOpenViewV2},
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
    let mut writer = Writer(blake2b_simd::Params::new().hash_length(64).key(domain).to_state());
    value.serialize(&mut writer).expect("fixed retained material serialization");
    Digest(writer.0.finalize().as_bytes().try_into().expect("64-byte digest"))
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
            || self.declared_prefill_tokens.checked_add(self.exact_decode_tokens).is_none_or(|n| n > self.max_context_tokens)
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
        digest(b"misaka/positive/assignment-registered-metadata/v2/r1", self)
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
    pub fn id(&self) -> Digest {
        digest(b"misaka/positive/assignment-admission-material/v2/r1", self)
    }
    /// Fixed bounded decoder; there is no arbitrary Params or collection decode.
    pub fn decode(raw: &[u8], max_bytes: u64) -> Result<Self, ContractError> {
        if max_bytes == 0 || max_bytes == u64::MAX || raw.is_empty() || raw.len() as u64 > max_bytes {
            return Err(ContractError::Budget);
        }
        let structural = crate::palw_native_resource_v1::preflight_native_value_v1::<Self>(raw, raw.len() as u64)
            .map_err(|_| ContractError::NonCanonical)?;
        if structural.logical_requests_v1() != std::mem::size_of::<Self>() as u64 {
            return Err(ContractError::Budget);
        }
        let value = borsh::from_slice::<Self>(raw).map_err(|_| ContractError::NonCanonical)?;
        Ok(value)
    }
    /// Unique retained history must also fit two complete reads during replay.
    pub fn native_assignment_journal_storage_limit_v2(&self) -> u64 {
        self.assignment_policy.max_history_bytes.min(self.money.usage.retained_bytes).min(self.money.usage.replay_bytes / 2)
    }

    /// The framed native record includes the actual control envelope and
    /// journal header. Both retention and replay must pay for those bytes.
    pub fn check_native_open_record_extent_v2(&self, payload_bytes: u64) -> Result<(), ContractError> {
        use misaka_palw_positive_contract::assignment::journal::ASSIGNMENT_JOURNAL_HEADER_BYTES_V2;

        let record_bytes = payload_bytes.checked_add(ASSIGNMENT_JOURNAL_HEADER_BYTES_V2 as u64).ok_or(ContractError::Budget)?;
        if payload_bytes == 0
            || payload_bytes > self.assignment_policy.max_frame_bytes
            || record_bytes > self.native_assignment_journal_storage_limit_v2()
        {
            return Err(ContractError::Budget);
        }
        Ok(())
    }

    pub fn check_row(&self, row: &AssignmentRow) -> Result<(), ContractError> {
        row.validate_shape()?;
        self.assignment_policy.check_terms(&row.bundle.terms, self.origin.accepted_daa)?;
        self.positive_policy.validate_shape().map_err(|_| ContractError::Scope)?;
        self.native_preimages.validate(&self.assignment_policy).map_err(|_| ContractError::Scope)?;
        self.funding_relay.validate_original_scope(self.fired_rules).map_err(|_| ContractError::Scope)?;
        self.certification_policy.validate().map_err(|_| ContractError::Scope)?;
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
            || self.positive_policy.service_profile_id.as_bytes() != self.native_preimages.source_policy.id().0
            || self.positive_policy.funding_schedule_id.as_bytes() != self.native_preimages.case_fees.id().0
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
            || row.admission_material != (AcceptedMaterial { origin: row.origin, digest: self.id() })
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
        let fixed_bytes = borsh::object_length(self).map_err(|_| ContractError::NonCanonical)? as u64;
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
        let material = AssignmentAdmissionMaterialV2::decode(open.material(), limits.max_material_bytes)?;
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
        if registered != self.material.registered || AssignmentSourceQuoteDescriptorV2::from(source) != self.material.source {
            return Err(ContractError::Identity);
        }
        Ok(())
    }
}
