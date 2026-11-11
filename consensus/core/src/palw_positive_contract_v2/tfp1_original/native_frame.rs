//! Immutable native original identity for the protected TFP1 profile.
//! Preparing or decoding a frame does not reserve service or fund escrow.

use super::{NativeTfp1PublicOriginalFactsV1, PreparedTfp1PublicOriginalV1};
use crate::{
    Hash64,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_fp_execution_v3::{PALW_FP_V3_DOMAIN_JOB_PIN, palw_fp_execution_seed_v3},
    palw_freeprompt_v3::{
        PALW_FP_COMMITMENT_TX_MAX_BYTES, PALW_FP_V3_DOMAIN_CLAIM_ID, PALW_FP_V3_DOMAIN_JOB_ID, PALW_FP_V3_VERSION,
        PALW_FP_V4_DOMAIN_JOB_ID, PALW_FP_V4_VERSION, PalwFreePromptCommitmentV3,
    },
    palw_native_resource_v1::{
        NativeCoverageV1, NativeDecodeErrorV1, NativeRetainedV1, NativeWireReaderV1, NativeWireScanV1, NativeWireShapeV1,
        native_struct, native_typed_value_requests_v1, reserve_native_pending_requests_v1, wire::wire_bad,
    },
    palw_positive_contract_v2::{check_public_origin_point_v1, require_current_public_origin_v1},
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwBlockContextV2, PalwBondKeyV2, PalwChainStateV2, PalwClaimSourceV2},
    palw_tir_admission_v1::PalwTirClassRecordV1,
    palw_tir_public_claim_v1::palw_tir_public_claim_v1,
    palw_tir_public_fp_envelope_v1::PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1,
    palw_tir_public_source_v1::{PalwTirPublicSourceV1, public_source_header_id_v1},
    palw_v2::{PALW_V2_DOMAIN_JOB_CONTEXT, PalwJobContextV2},
};
use borsh::{BorshDeserialize, BorshSerialize};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{OriginalClock, OriginalId},
    rows::AcceptedOrigin,
};
use std::io::{self, Read, Write};

const VERSION: u16 = 1;
const ID_DOMAIN: &[u8] = b"misaka-palw/tfp1-protected-native-frame/v1";

#[derive(Clone, Debug, PartialEq, Eq, BorshDeserialize, BorshSerialize)]
struct FramePinsV1 {
    claim: Hash64,
    source: Hash64,
    origin: AcceptedOrigin,
    original: OriginalId,
    profile: Hash64,
    resource: Hash64,
    obligation: Digest,
    reservation: Digest,
    material_until_daa: u64,
    maximum_leaves: u64,
}

native_struct!(FramePinsV1; claim, source, origin, original, profile, resource, obligation, reservation, material_until_daa, maximum_leaves);
crate::palw_native_resource_v1::wire::wire_struct!(FramePinsV1; Hash64, Hash64, AcceptedOrigin, OriginalId, Hash64, Hash64, Digest, Digest, u64, u64);

/// A candidate for the native paired writer. The writer must independently
/// require complete booking, actual escrow and the sealed history boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicTfp1FrameV1 {
    version: u16,
    original: PalwFreePromptCommitmentV3,
    header: PalwPublicTirBindingHeaderV1,
    pins: FramePinsV1,
    identity: Hash64,
}
native_struct!(PublicTfp1FrameV1; version, original, header, pins, identity);

impl PublicTfp1FrameV1 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn prepare_v1(
        prepared: &PreparedTfp1PublicOriginalV1,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        class: &PalwTirClassRecordV1,
        point: &PalwBlockContextV2,
        reservation: Digest,
        material_until_daa: u64,
        maximum_leaves: u64,
    ) -> Result<NativeRetainedV1<Self>, NativeDecodeErrorV1<ContractError>> {
        use NativeDecodeErrorV1::{Decode, Resource};

        let companion = prepared.companion_v1();
        let origin = companion.carrier.origin;
        require_current_public_origin_v1(origin, point).map_err(Decode)?;
        if prepared.pairing.is_none()
            || material_until_daa != companion.budget.retention_until
            || source.leaf_count_v1() > maximum_leaves
            || source.decode_count_v1() != fp.decode_tokens_executed
            || source.vocab_v1() != class.logits_vocab
            || source.supports_tiled_trace_v1() != class.facts.tiled
        {
            return Err(Decode(ContractError::Scope));
        }
        let header = source.header_v1();
        check_body_bounds_v1(fp, header).map_err(Decode)?;
        check_class_v1(class, header).map_err(Decode)?;
        let obligation = prepared.obligation_v1();
        let pins = FramePinsV1 {
            claim: obligation.facts.claim,
            source: source.source_id_v1(),
            origin,
            original: companion.original,
            profile: obligation.profile,
            resource: obligation.resource,
            obligation: obligation.obligation,
            reservation,
            material_until_daa,
            maximum_leaves,
        };

        let facts = checked_facts_v1(fp, header, &pins).map_err(Decode)?;
        if facts != obligation.facts {
            return Err(Decode(ContractError::Scope));
        }
        let identity = frame_identity_v1(VERSION, fp, header, &pins).map_err(Decode)?;
        // Cover both deep copies before either clone.
        let requests = native_typed_value_requests_v1(fp)
            .and_then(|fp_bytes| {
                let header_bytes = native_typed_value_requests_v1(header)?;
                fp_bytes
                    .checked_add(header_bytes)
                    .and_then(|bytes| bytes.checked_add(std::mem::size_of::<NativeRetainedV1<Self>>() as u64))
                    .ok_or_else(|| NativeCoverageV1::unsupported::<Self>("protected TFP1 frame", "retained copy size overflow"))
            })
            .map_err(Resource)?;
        let owner = reserve_native_pending_requests_v1(requests).map_err(Resource)?;
        Ok(NativeRetainedV1::from_owner_reserved_v1(
            Self { version: VERSION, original: fp.clone(), header: header.clone(), pins, identity },
            owner,
        ))
    }

    pub(crate) fn check_structure_v1(&self, claim: Hash64) -> Result<(), String> {
        if self.claim_v1() != claim {
            return Err("protected TFP1 frame has a foreign claim key".into());
        }
        self.native_facts_v1().map(|_| ()).map_err(|e| e.to_string())
    }

    pub(crate) fn native_facts_v1(&self) -> Result<NativeTfp1PublicOriginalFactsV1, ContractError> {
        if self.version != VERSION || self.identity == Hash64::default() {
            return Err(ContractError::Identity);
        }
        check_body_bounds_v1(&self.original, &self.header)?;
        let facts = checked_facts_v1(&self.original, &self.header, &self.pins)?;
        if frame_identity_v1(self.version, &self.original, &self.header, &self.pins)? != self.identity {
            return Err(ContractError::Identity);
        }
        Ok(facts)
    }

    /// The supplied point may be later than the original's accepting block,
    /// as it is during snapshot validation and archival reconstruction.
    pub(crate) fn check_state_at_v1(&self, state: &PalwChainStateV2, point: &PalwBlockContextV2) -> Result<(), ContractError> {
        check_public_origin_point_v1(self.origin_v1(), point)?;
        self.check_native_claim_v1(state)
    }

    /// Native identity check usable within the accepting checkpoint, before
    /// LastPoint advances. It confers no inclusion-time or funding authority;
    /// the paired writer and final-state validator check those separately.
    fn check_native_claim_v1(&self, state: &PalwChainStateV2) -> Result<(), ContractError> {
        self.native_facts_v1()?;
        let origin = self.origin_v1();
        let claim_id = self.claim_v1();
        let claim = state.claim(&claim_id).ok_or(ContractError::Scope)?;
        if palw_tir_public_claim_v1(state, claim_id).is_some()
            || !matches!(&claim.source, PalwClaimSourceV2::FreePrompt { .. })
            || claim.accepted_block.as_bytes() != origin.accepting_block.0
            || claim.accepted_daa != origin.accepted_daa
            || claim.accepted_blue_score != origin.accepted_blue_score
            || claim.class_id != self.original.job.class_id
            || claim.bond != self.producer_v1()
            || claim.execution_root != self.original.execution_root
            || claim.output_root != self.original.output_root
            || claim.trace_root != self.original.trace_root
            || claim.work_leaves != self.original.work_leaves
            || claim.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_of_context_v1(&self.header.job_context)
            || claim.trace_retention_daa < self.material_until_daa_v1()
        {
            return Err(ContractError::Scope);
        }
        let class = state.tir_class_v1(&claim.class_id).ok_or(ContractError::Scope)?;
        check_class_v1(class, &self.header)
    }

    /// The dedicated frame tag and aux-204 version 2 identify this new scope.
    /// Legacy bindings continue to name their actual allocation, unchanged.
    /// Packet coverage, payment and release are independent checks.
    pub(crate) fn check_service_binding_v1(
        &self,
        state: &PalwChainStateV2,
        binding: &crate::palw_court_service_state_v2::ServiceClaimBinding,
    ) -> Result<(), ContractError> {
        self.check_native_claim_v1(state)?;
        self.check_service_scope_v1(binding)
    }

    /// Fixed-field comparison for already validated native state. Admission
    /// and snapshot validation still perform full frame/claim validation.
    pub(crate) fn check_service_scope_v1(
        &self,
        binding: &crate::palw_court_service_state_v2::ServiceClaimBinding,
    ) -> Result<(), ContractError> {
        if binding.claim != self.claim_v1()
            || binding.allocation_id != self.identity_v1()
            || binding.source_id != self.source_v1()
            || binding.original_accepted_daa != self.origin_v1().accepted_daa
            || binding.hard_deadline_daa != OriginalClock::new(self.origin_v1().accepted_daa)?.hard()
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }

    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.pins.claim
    }

    pub(crate) fn source_v1(&self) -> Hash64 {
        self.pins.source
    }

    pub(crate) fn producer_v1(&self) -> PalwBondKeyV2 {
        PalwBondKeyV2(self.original.job.executor_bond)
    }

    pub(crate) fn original_v1(&self) -> &PalwFreePromptCommitmentV3 {
        &self.original
    }

    pub(crate) fn header_v1(&self) -> &PalwPublicTirBindingHeaderV1 {
        &self.header
    }

    pub(crate) fn original_id_v1(&self) -> OriginalId {
        self.pins.original
    }

    pub(crate) fn origin_v1(&self) -> AcceptedOrigin {
        self.pins.origin
    }

    pub(crate) fn profile_v1(&self) -> Hash64 {
        self.pins.profile
    }

    pub(crate) fn resource_v1(&self) -> Hash64 {
        self.pins.resource
    }

    pub(crate) fn obligation_v1(&self) -> Digest {
        self.pins.obligation
    }

    pub(crate) fn reservation_v1(&self) -> Digest {
        self.pins.reservation
    }

    pub(crate) fn material_until_daa_v1(&self) -> u64 {
        self.pins.material_until_daa
    }

    pub(crate) fn maximum_leaves_v1(&self) -> u64 {
        self.pins.maximum_leaves
    }

    pub(crate) fn identity_v1(&self) -> Hash64 {
        self.identity
    }
}

fn frame_identity_v1(
    version: u16,
    original: &PalwFreePromptCommitmentV3,
    header: &PalwPublicTirBindingHeaderV1,
    pins: &FramePinsV1,
) -> Result<Hash64, ContractError> {
    stream_digest(ID_DOMAIN, &(version, original, header, pins)).map_err(|_| ContractError::NonCanonical)
}

fn check_class_v1(class: &PalwTirClassRecordV1, header: &PalwPublicTirBindingHeaderV1) -> Result<(), ContractError> {
    use crate::palw_tir_class_v1::{PALW_TIR_CLASS_VERSION_V1, palw_tir_class_id_from_digests_v1};
    if class.version != crate::palw_tir_admission_v1::PALW_TIR_CLASS_RECORD_VERSION_V1
        || class.facts.class_id != header.job_context.shape_profile_id
        || class.tokenizer_id != header.job_context.tokenizer_id
        || header.job_context.max_context_tokens > class.facts.max_context
        || palw_tir_class_id_from_digests_v1(
            PALW_TIR_CLASS_VERSION_V1,
            &class.graph_ir_root,
            &class.layout_digest,
            &header.artifact_root,
            &class.tokenizer_id,
        ) != class.facts.class_id
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}

fn check_body_bounds_v1(original: &PalwFreePromptCommitmentV3, header: &PalwPublicTirBindingHeaderV1) -> Result<(), ContractError> {
    let fp_len = borsh::object_length(original).map_err(|_| ContractError::NonCanonical)?;
    let header_len = borsh::object_length(header).map_err(|_| ContractError::NonCanonical)?;
    if fp_len > PALW_FP_COMMITMENT_TX_MAX_BYTES || header_len > PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1 {
        return Err(ContractError::Budget);
    }
    if !matches!((original.job.version, original.job.decode.is_some()), (PALW_FP_V3_VERSION, false) | (PALW_FP_V4_VERSION, true))
        || original.job.tail.is_some()
        || header.version != crate::palw_tir_step_v1::PALW_TIR_STEP_BINDING_VERSION_V1
    {
        return Err(ContractError::UnsupportedProfile);
    }
    crate::palw_slash::check_job_context_shape(&header.job_context).map_err(|_| ContractError::NonCanonical)
}

// Legacy FP IDs prefix the canonical Borsh stream with its u64 byte
// length. Count once, then stream that exact preimage into the existing domain.
fn fp_id_v1(domain: &[u8], value: &impl BorshSerialize) -> Result<Hash64, ContractError> {
    let len = borsh::object_length(value).map_err(|_| ContractError::NonCanonical)? as u64;
    stream_digest(domain, &(len, value)).map_err(|_| ContractError::NonCanonical)
}

fn cold_fp_job_pin_v1(original: &PalwFreePromptCommitmentV3) -> Result<Hash64, ContractError> {
    let job = &original.job;
    let domain = match job.version {
        PALW_FP_V3_VERSION => PALW_FP_V3_DOMAIN_JOB_ID,
        PALW_FP_V4_VERSION => PALW_FP_V4_DOMAIN_JOB_ID,
        _ => return Err(ContractError::UnsupportedProfile),
    };
    let job_id = fp_id_v1(domain, job)?;
    stream_digest(
        PALW_FP_V3_DOMAIN_JOB_PIN,
        &(
            job_id,
            palw_fp_execution_seed_v3(job),
            job.tokenizer_id,
            job.prompt_token_ids_hash,
            job.prompt_tokens,
            original.decode_tokens_executed,
            job.max_context_tokens,
        ),
    )
    .map_err(|_| ContractError::NonCanonical)
}

// The current context's Borsh fields are exactly its canonical hash preimage
// (including the u32 length of network_id). The parity test pins this relation.
fn context_hash_v1(context: &PalwJobContextV2) -> Result<Hash64, ContractError> {
    stream_digest(PALW_V2_DOMAIN_JOB_CONTEXT, context).map_err(|_| ContractError::NonCanonical)
}

fn checked_facts_v1(
    original: &PalwFreePromptCommitmentV3,
    header: &PalwPublicTirBindingHeaderV1,
    pins: &FramePinsV1,
) -> Result<NativeTfp1PublicOriginalFactsV1, ContractError> {
    let clock = OriginalClock::new(pins.origin.accepted_daa)?;
    if [pins.claim, pins.source, pins.profile, pins.resource].contains(&Hash64::default())
        || [pins.original.0, pins.obligation, pins.reservation].contains(&Digest([0; 64]))
        || pins.origin.accepting_block.0 == [0; 64]
        || pins.origin.carrier_block.0 == [0; 64]
        || pins.origin.carrier_transaction.0 == [0; 64]
        || pins.material_until_daa < clock.hard()
        || header.step_leaf_count == 0
        || header.step_leaf_count > pins.maximum_leaves
        || fp_id_v1(PALW_FP_V3_DOMAIN_CLAIM_ID, original)? != pins.claim
        || public_source_header_id_v1(header).map_err(|_| ContractError::NonCanonical)? != pins.source
    {
        return Err(ContractError::Scope);
    }
    let context = context_hash_v1(&header.job_context)?;
    if crate::palw_tir_step_v1::palw_tir_execution_root_v1(
        &context,
        &header.full_logits_trace_root,
        &header.job_context.shape_profile_id,
        header.step_leaf_count,
        &header.step_merkle_root,
    ) != header.committed_execution_root
    {
        return Err(ContractError::Scope);
    }
    let facts = NativeTfp1PublicOriginalFactsV1::from_native_header_with_hashes_v1(
        original,
        header,
        pins.claim,
        pins.source,
        || cold_fp_job_pin_v1(original),
        || context,
    )?;
    if !facts.cold_execution || facts.service_obligation_v1(pins.profile, pins.resource)?.obligation_v1() != pins.obligation {
        return Err(ContractError::Scope);
    }
    Ok(facts)
}

impl BorshSerialize for PublicTfp1FrameV1 {
    fn serialize<W: Write>(&self, writer: &mut W) -> io::Result<()> {
        self.version.serialize(writer)?;
        write_body_v1(writer, &self.original, PALW_FP_COMMITMENT_TX_MAX_BYTES)?;
        write_body_v1(writer, &self.header, PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1)?;
        self.pins.serialize(writer)?;
        self.identity.serialize(writer)
    }
}

fn write_body_v1<W: Write>(writer: &mut W, value: &impl BorshSerialize, maximum: usize) -> io::Result<()> {
    let length = borsh::object_length(value)?;
    if length == 0 || length > maximum {
        return Err(wire_bad("protected TFP1 frame body length"));
    }
    (length as u32).serialize(writer)?;
    value.serialize(writer)
}

impl BorshDeserialize for PublicTfp1FrameV1 {
    fn deserialize_reader<R: Read>(reader: &mut R) -> io::Result<Self> {
        let version = u16::deserialize_reader(reader)?;
        check_version_v1(version)?;
        // Frame bodies are capped independently and preflighted before any
        // nested allocation. The scratch lives on stack once per decode.
        let original = read_body_v1::<_, PALW_FP_COMMITMENT_TX_MAX_BYTES>(reader)?;
        let header = read_body_v1::<_, PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1>(reader)?;
        Ok(Self {
            version,
            original,
            header,
            pins: FramePinsV1::deserialize_reader(reader)?,
            identity: Hash64::deserialize_reader(reader)?,
        })
    }
}

fn check_version_v1(version: u16) -> io::Result<()> {
    if version == VERSION { Ok(()) } else { Err(wire_bad("unknown protected TFP1 frame version")) }
}

fn check_body_length_v1(length: u32, maximum: usize) -> io::Result<usize> {
    let length = length as usize;
    if length == 0 || length > maximum {
        return Err(wire_bad("protected TFP1 frame body length"));
    }
    Ok(length)
}

fn read_body_v1<T: BorshDeserialize + NativeWireScanV1, const MAX: usize>(reader: &mut impl Read) -> io::Result<T> {
    let length = check_body_length_v1(u32::deserialize_reader(reader)?, MAX)?;
    let mut scratch = [0u8; MAX];
    let body = &mut scratch[..length];
    reader.read_exact(body)?;
    let mut scanner = NativeWireReaderV1::new(&*body, length as u64);
    T::scan_native_wire_v1(&mut scanner)?;
    if scanner.remaining() != 0 {
        return Err(wire_bad("trailing protected TFP1 frame body bytes"));
    }
    borsh::from_slice(body)
}

impl NativeWireScanV1 for PublicTfp1FrameV1 {
    fn scan_native_wire_v1<R: Read>(reader: &mut NativeWireReaderV1<R>) -> io::Result<NativeWireShapeV1> {
        reader.step()?;
        check_version_v1(reader.short()?)?;
        scan_body_v1::<_, PalwFreePromptCommitmentV3>(reader, PALW_FP_COMMITMENT_TX_MAX_BYTES)?
            .merge(scan_body_v1::<_, PalwPublicTirBindingHeaderV1>(reader, PALW_TIR_PUBLIC_FP_HEADER_MAX_BYTES_V1)?)?
            .merge(FramePinsV1::scan_native_wire_v1(reader)?)?
            .merge(Hash64::scan_native_wire_v1(reader)?)
    }
}

fn scan_body_v1<R: Read, T: NativeWireScanV1>(reader: &mut NativeWireReaderV1<R>, maximum: usize) -> io::Result<NativeWireShapeV1> {
    let length = check_body_length_v1(reader.word()?, maximum)? as u64;
    if length > reader.remaining() {
        return Err(wire_bad("truncated protected TFP1 frame body"));
    }
    let mut body = NativeWireReaderV1::new(reader, length);
    let shape = T::scan_native_wire_v1(&mut body)?;
    if body.remaining() != 0 {
        return Err(wire_bad("trailing protected TFP1 frame body bytes"));
    }
    Ok(shape)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        palw_decode_pipeline_v4::DecodeConfigV4, palw_fp_execution_v3::palw_fp_job_pin_v1, palw_freeprompt_v3::fp_claim_id_v3,
        palw_tir_public_claim_v1::palw_tir_public_signed_material_frame_fixture_v1,
    };
    use misaka_palw_positive_contract::rows::NativeHash64;

    #[test]
    fn protected_tfp1_frame_streamed_identity_matches_legacy_v1() {
        let fixture = palw_tir_public_signed_material_frame_fixture_v1(&[11; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]);
        let frame = &fixture.frame;
        let mut v3 = frame.original_v1().clone();
        v3.job.version = PALW_FP_V3_VERSION;
        v3.job.decode = None;
        let mut v4 = v3.clone();
        v4.job = v4.job.into_v4(DecodeConfigV4 { logit_bias: vec![(1, 7), (19, -3)], ..DecodeConfigV4::NOOP });
        for original in [&v3, &v4] {
            assert_eq!(fp_id_v1(PALW_FP_V3_DOMAIN_CLAIM_ID, original).unwrap(), fp_claim_id_v3(original));
            assert_eq!(cold_fp_job_pin_v1(original).unwrap(), palw_fp_job_pin_v1(original));
        }
        let mut context = frame.header_v1().job_context.clone();
        context.job_id = Hash64::from_u64_word(1);
        context.job_nullifier = Hash64::from_u64_word(2);
        context.assignment_id = Hash64::from_u64_word(3);
        context.execution_seed = [4; 32];
        context.model_profile_id = Hash64::from_u64_word(5);
        context.runtime_manifest_hash = Hash64::from_u64_word(6);
        context.runtime_class_id = Hash64::from_u64_word(7);
        context.shape_profile_id = Hash64::from_u64_word(8);
        context.trace_scheme_id = Hash64::from_u64_word(9);
        context.cu_ruleset_id = Hash64::from_u64_word(10);
        context.tokenizer_id = Hash64::from_u64_word(11);
        context.prompt_token_ids_hash = Hash64::from_u64_word(12);
        context.declared_prefill_tokens = 13;
        context.exact_decode_tokens = 14;
        context.max_context_tokens = 15;
        for network in [b"a".as_slice(), b"testnet-12-with-a-different-length".as_slice()] {
            context.network_id = network.to_vec();
            assert_eq!(context_hash_v1(&context).unwrap(), context.context_hash());
        }
    }

    #[test]
    fn protected_tfp1_frame_codec_bounds_and_identity_v1() {
        let fixture = palw_tir_public_signed_material_frame_fixture_v1(&[11; crate::mldsa87_primitives::MLDSA87_PUBKEY_LEN]);
        let old = &fixture.frame;
        let original = old.original_v1().clone();
        let header = old.header_v1().clone();
        let claim = fp_claim_id_v3(&original);
        let source = public_source_header_id_v1(&header).unwrap();
        let facts = NativeTfp1PublicOriginalFactsV1::from_native_header_v1(&original, &header, claim, source).unwrap();
        let profile = Hash64::from_u64_word(8);
        let resource = Hash64::from_u64_word(9);
        let origin = AcceptedOrigin {
            accepting_block: NativeHash64([1; 64]),
            carrier_block: NativeHash64([2; 64]),
            carrier_transaction: NativeHash64([3; 64]),
            accepted_tx_ordinal: 1,
            event_ordinal: 2,
            accepted_daa: 4,
            accepted_blue_score: 4,
        };
        let pins = FramePinsV1 {
            claim,
            source,
            origin,
            original: OriginalId(Digest([5; 64])),
            profile,
            resource,
            obligation: facts.service_obligation_v1(profile, resource).unwrap().obligation_v1(),
            reservation: Digest([6; 64]),
            material_until_daa: OriginalClock::new(origin.accepted_daa).unwrap().hard(),
            maximum_leaves: header.step_leaf_count,
        };
        let identity = frame_identity_v1(VERSION, &original, &header, &pins).unwrap();
        let frame = PublicTfp1FrameV1 { version: VERSION, original, header, pins, identity };
        assert_eq!(frame.native_facts_v1().unwrap(), facts);
        assert!(frame.check_structure_v1(Hash64::from_u64_word(1)).is_err());

        let bytes = borsh::to_vec(&frame).unwrap();
        let preflight =
            |bytes: &[u8]| crate::palw_native_resource_v1::preflight_native_value_v1::<PublicTfp1FrameV1>(bytes, bytes.len() as u64);
        assert!(preflight(&bytes).is_ok());
        assert_eq!(borsh::from_slice::<PublicTfp1FrameV1>(&bytes).unwrap(), frame);
        assert!(preflight(&bytes[..bytes.len() - 1]).is_err());

        let mut wrong_version = bytes.clone();
        wrong_version[..2].copy_from_slice(&(VERSION + 1).to_le_bytes());
        assert!(preflight(&wrong_version).is_err());
        assert!(borsh::from_slice::<PublicTfp1FrameV1>(&wrong_version).is_err());

        let mut excessive_fp = bytes;
        excessive_fp[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(preflight(&excessive_fp).is_err());
        assert!(borsh::from_slice::<PublicTfp1FrameV1>(&excessive_fp).is_err());

        let mut altered = frame.clone();
        altered.pins.reservation.0[0] ^= 1;
        assert_eq!(altered.native_facts_v1(), Err(ContractError::Identity));
        altered.pins.maximum_leaves = 0;
        altered.identity = frame_identity_v1(VERSION, &altered.original, &altered.header, &altered.pins).unwrap();
        assert_eq!(altered.native_facts_v1(), Err(ContractError::Scope));
    }
}
