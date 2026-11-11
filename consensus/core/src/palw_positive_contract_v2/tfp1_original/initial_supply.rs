//! Native size floor for ordinary TFP1 claim material. These quantities do
//! not price service, perform inspection, or prove a provider can supply it.

use super::NativeTfp1PublicOriginalFactsV1;
use crate::{
    Hash64,
    palw_court_shared_tir_stream_v1::stream_digest,
    palw_freeprompt_v3::{PALW_FP_COMMITMENT_TX_MAX_BYTES, PalwFreePromptCommitmentV3},
    palw_native_resource_v1::{NativeCoverageV1, NativeDecodeErrorV1, NativeDemandLeaseV1, reserve_native_pending_requests_v1},
    palw_positive_contract_v2::PublicTfp1HfArtifactV1,
    palw_prompt_ids_v1::{PALW_PROMPT_IDS_TILE_LEN, PalwPromptIdsFormV1, prompt_ids_form_id_v1},
    palw_staged_court_state_v1::palw_staged_tir_layout_v1,
    palw_state_v2::{PalwBlockContextV2, PalwChainStateV2, PalwTransitionExtrasV1},
    palw_step_leg::step_leg_max_opening_siblings_v1,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_container_v1::palw_tir_bound_container_v1,
    palw_tir_court_v1::PublicOriginalTraceIndexV1,
    palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
    palw_tokenizer_rendering_v2::palw_rooted_tokenizer_for_original_at_clocks_v3,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{
        CheckedManifestV1, ManifestScopeRules, ManifestV1, MaterialScope, OriginalId, hf_artifact_material_codec_v1,
        prompt_text_material_codec_v1, tir_cold_dependencies_codec_v1, tir_cold_initial_state_codec_v1,
        tir_ordinary_tfp1_rules_material_codec_v1, tir_output_material_codec_v1, tir_program_material_codec_v1,
        tir_trace_material_codec_v1, tokenizer_json_material_codec_v1,
    },
    reservations::RemainingCapacity,
};

const MATERIAL_SCOPES: [MaterialScope; MaterialScope::COUNT] = [
    MaterialScope::Artifact,
    MaterialScope::Program,
    MaterialScope::Prompt,
    MaterialScope::InputTokens,
    MaterialScope::Tokenizer,
    MaterialScope::ExecutionRules,
    MaterialScope::InitialState,
    MaterialScope::Dependencies,
    MaterialScope::Trace,
    MaterialScope::Output,
];

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum InitialSupplyErrorV1 {
    MissingRegisteredClass,
    MissingRegisteredLayout,
    MissingRegisteredProgram,
    MissingTokenizerRegistration,
    MissingHfDescriptor,
    MissingBoundContainer,
    MissingTraceGeometry,
    Overflow,
    Contract(ContractError),
    LocalResource(NativeCoverageV1),
}

impl From<ContractError> for InitialSupplyErrorV1 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

/// Identity helpers serialize FP, context and layout sequentially. Reserve
/// their largest writer, including geometric growth, before invoking them.
/// A retained Source/class allocation is not pending serialization scratch.
pub(super) fn reserve_identity_writers_v3(
    fp: &PalwFreePromptCommitmentV3,
    source: &PalwTirPublicSourceV1,
    class: &PalwTirClassV1,
) -> Result<Option<NativeDemandLeaseV1>, InitialSupplyErrorV1> {
    let fp_bytes = borsh::object_length(fp).map_err(|_| ContractError::NonCanonical)?;
    let job = &source.header_v1().job_context;
    if fp_bytes > PALW_FP_COMMITMENT_TX_MAX_BYTES || job.network_id.len() > crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES {
        return Err(ContractError::Budget.into());
    }
    let context_bytes = borsh::object_length(job).map_err(|_| ContractError::NonCanonical)?;
    let layout_bytes = borsh::object_length(&class.layout).map_err(|_| ContractError::NonCanonical)?;
    let bytes = PalwTirPublicSourceV1::native_identity_writer_requests_v3(fp_bytes.max(context_bytes).max(layout_bytes) as u64)
        .map_err(|error| match error {
            NativeDecodeErrorV1::Resource(coverage) => InitialSupplyErrorV1::LocalResource(coverage),
            NativeDecodeErrorV1::Decode(_) => InitialSupplyErrorV1::Contract(ContractError::NonCanonical),
        })?;
    reserve_native_pending_requests_v1(bytes).map_err(InitialSupplyErrorV1::LocalResource)
}

/// All members are raw geometry, independent of tariffs and worker inference
/// speed. InputTokens counts one complete raw vector conservatively even if
/// the ordinary worker obtains it by re-encoding the supplied prompt. A
/// separate court publication of input openings has its own transport costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PublicTfp1InitialSupplyV1 {
    claim: Hash64,
    source: Hash64,
    literal_prompt: Hash64,
    hf_content_root: Digest,
    hf_chunk_bytes: u32,
    input_tokens: Digest,
    execution_rules_content: Digest,
    tokenizer_registration: Hash64,
    prompt_form: PalwPromptIdsFormV1,
    body_bytes: [u64; MaterialScope::COUNT],
    payload_bytes: u64,
    trace: TraceSupplyV1,
    claim_wire_bytes: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TraceSupplyV1 {
    leaves: u64,
    sibling_visits: u64,
    widest_value_bytes: u64,
    wire_bytes: u64,
}

impl PublicTfp1InitialSupplyV1 {
    /// The caller supplies the authenticated selected parent and native
    /// prompt form. Both accepting clocks must be later than the tokenizer
    /// registration. The HF reference must resolve to the same bound model;
    /// no source files are downloaded or counted here.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_v1(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        at: &PalwBlockContextV2,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        class: &PalwTirClassV1,
        prompt_form: PalwPromptIdsFormV1,
        literal_prompt: &[u8],
        hf: Option<&PublicTfp1HfArtifactV1>,
    ) -> Result<Self, InitialSupplyErrorV1> {
        Self::derive_at_clocks_v2(parent, extras, at.daa_score, at.blue_score, fp, source, class, prompt_form, literal_prompt, hf)
    }

    /// The unsigned and signed paths share native geometry. Only actual
    /// intake has an AcceptedOrigin; size/registration planning needs clocks.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn derive_at_clocks_v2(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        candidate_daa: u64,
        candidate_blue_score: u64,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        class: &PalwTirClassV1,
        prompt_form: PalwPromptIdsFormV1,
        literal_prompt: &[u8],
        hf: Option<&PublicTfp1HfArtifactV1>,
    ) -> Result<Self, InitialSupplyErrorV1> {
        use InitialSupplyErrorV1::*;
        let header = source.header_v1();
        let job = &header.job_context;
        super::super::tfp1_material::require_cold_tfp1_v1(fp, job)?;
        if !source.supports_tiled_trace_v1() {
            return Err(ContractError::UnsupportedProfile.into());
        }
        if literal_prompt.len() > PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1
            || borsh::object_length(fp).map_err(|_| ContractError::NonCanonical)? > PALW_FP_COMMITMENT_TX_MAX_BYTES
        {
            return Err(ContractError::Budget.into());
        }
        let record = parent.tir_class_v1(&fp.job.class_id).ok_or(MissingRegisteredClass)?;
        let registered = parent.class(&fp.job.class_id).ok_or(MissingRegisteredClass)?;
        let layout = palw_staged_tir_layout_v1(parent, fp.job.class_id).ok_or(MissingRegisteredLayout)?;
        if record.program_bytes == 0 || record.program.is_empty() {
            return Err(MissingRegisteredProgram);
        }
        record.check_program_v1().map_err(|_| ContractError::Identity)?;
        if registered.artifact_root != header.artifact_root
            || record.facts.class_id != fp.job.class_id
            || &class.layout != layout
            || class.program.as_slice() != record.program.as_slice()
            || class.tokenizer_id != job.tokenizer_id
        {
            return Err(ContractError::Scope.into());
        }

        // Legacy FP/job/context hashes and HF's class check temporarily
        // serialize their preimages. Quote from the actual bounded inputs
        // before calling those paths; no model data are cloned or parsed.
        let _hash_scratch = reserve_identity_writers_v3(fp, source, class)?;
        let facts = NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)?;
        let (claim, source_id, _) = facts.native_identity_v1();

        let hf = hf.ok_or(MissingHfDescriptor)?;
        hf.check_context_v1(fp, source, class)?;
        let bound = palw_tir_bound_container_v1(parent, fp.job.class_id).ok_or(MissingBoundContainer)?;
        let hf_pins = hf.descriptor_v1().native_pins_v1();
        if hf_pins.container != Digest(bound.container_root.as_bytes())
            || hf_pins.binding_manifest != Digest(bound.manifest_digest.as_bytes())
        {
            return Err(ContractError::Scope.into());
        }
        let hf_entry = hf.entry_v1();
        let rules = super::super::tfp1_material::native_ordinary_tfp1_rules_material_v1(
            fp,
            source,
            class,
            prompt_form,
            ManifestScopeRules { codec: tir_ordinary_tfp1_rules_material_codec_v1(), max_bytes: 512, chunk_bytes: 512 },
        )?
        .0;

        let (tokenizer, _) =
            palw_rooted_tokenizer_for_original_at_clocks_v3(parent, extras, candidate_daa, candidate_blue_score, fp, header)
                .ok_or(MissingTokenizerRegistration)?;
        let tokenizer_bytes = tokenizer.facts_v1().statement.exact_raw_bytes;
        if tokenizer_bytes == 0 {
            return Err(MissingTokenizerRegistration);
        }

        std::str::from_utf8(literal_prompt).map_err(|_| ContractError::NonCanonical)?;
        if literal_prompt.is_empty() || job.declared_prefill_tokens == 0 || job.exact_decode_tokens == 0 {
            return Err(ContractError::Range.into());
        }
        let literal_prompt_id = literal_prompt_id_v1(literal_prompt)?;
        let trace_values = source.trace_value_bytes_v1().ok_or(MissingTraceGeometry)?;
        let trace = TraceSupplyV1::derive_v1(source.leaf_count_v1(), trace_values, source.widest_leaf_value_bytes_v1())?;
        let trace_body = (PublicOriginalTraceIndexV1::WIRE_BYTES as u64).checked_add(trace_values).ok_or(Overflow)?;
        // The fixed lengths are the byte-array return types of
        // public_ordinary_tfp1_rules_material_v1 and public_tfp1_cold_material_v1.
        let body_bytes = [
            hf_entry.total_bytes,
            u64::from(record.program_bytes),
            u64::try_from(literal_prompt.len()).map_err(|_| Overflow)?,
            u64::from(job.declared_prefill_tokens) * 4,
            tokenizer_bytes,
            512,
            128,
            68,
            trace_body,
            u64::from(job.exact_decode_tokens) * 4,
        ];
        let payload_bytes = body_bytes.iter().try_fold(0u64, |sum, bytes| sum.checked_add(*bytes).ok_or(Overflow))?;
        let claim_wire_bytes = payload_bytes
            .checked_sub(trace_body)
            .and_then(|bytes| bytes.checked_add(trace.wire_bytes))
            .and_then(|bytes| bytes.checked_add(u64::from(ManifestV1::WIRE_BYTES)))
            .ok_or(Overflow)?;
        Ok(Self {
            claim,
            source: source_id,
            literal_prompt: literal_prompt_id,
            hf_content_root: hf_entry.content_root,
            hf_chunk_bytes: hf_entry.chunk_bytes,
            input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
            execution_rules_content: rules.content_root,
            tokenizer_registration: tokenizer.record_id_v1(),
            prompt_form,
            body_bytes,
            payload_bytes,
            trace,
            claim_wire_bytes,
        })
    }

    /// Validate the ordinary manifest codecs, chunk geometry and all body
    /// limits. Transmit/retain ceilings must also cover one complete provider
    /// set including trace envelopes. Additional work is a pricing input:
    /// this geometry does not treat bytes or Panel inference as Node work.
    pub(crate) fn check_limits_v1(
        &self,
        max_manifest_bytes: u32,
        limits: &[ManifestScopeRules; MaterialScope::COUNT],
        manifest_total_limit: u64,
        initial_supply: RemainingCapacity,
    ) -> Result<(), InitialSupplyErrorV1> {
        ManifestV1::fetch_size(max_manifest_bytes)?;
        for ((scope, limit), codec) in MATERIAL_SCOPES.into_iter().zip(limits).zip(self.codecs_v1()) {
            if limit.codec != codec {
                return Err(ContractError::UnsupportedProfile.into());
            }
            if self.body_bytes_v1(scope) > limit.max_bytes || !self.chunk_fits_v1(scope, limit.chunk_bytes) {
                return Err(ContractError::Budget.into());
            }
        }
        if self.payload_bytes > manifest_total_limit
            || initial_supply.transmitted_bytes < self.claim_wire_bytes
            || initial_supply.retained_bytes < self.claim_wire_bytes
        {
            return Err(ContractError::Budget.into());
        }
        Ok(())
    }

    /// Compare the literal manifest with the same native material geometry
    /// used to price supply. The caller obtains `expected_original` from the
    /// authenticated companion for this claim and Source and checks policy
    /// ceilings separately. A checked manifest alone establishes neither.
    pub(crate) fn check_manifest_v1(
        &self,
        expected_original: OriginalId,
        manifest: &CheckedManifestV1,
        literal_prompt: &[u8],
    ) -> Result<(), InitialSupplyErrorV1> {
        if manifest.original() != expected_original {
            return Err(ContractError::Scope.into());
        }
        for (scope, codec) in MATERIAL_SCOPES.into_iter().zip(self.codecs_v1()) {
            let entry = manifest.entry(scope);
            if entry.codec != codec {
                return Err(ContractError::UnsupportedProfile.into());
            }
            if entry.total_bytes != self.body_bytes_v1(scope) || !self.chunk_fits_v1(scope, entry.chunk_bytes) {
                return Err(ContractError::Budget.into());
            }
            let expected_content = match scope {
                MaterialScope::Artifact => self.hf_content_root,
                MaterialScope::ExecutionRules => self.execution_rules_content,
                _ => entry.subject,
            };
            if entry.content_root != expected_content {
                return Err(ContractError::Identity.into());
            }
        }
        // The InputTokens codecs count raw u32 values, not opening frames.
        // Both the miner and the panel bind their content root to this pin.
        if manifest.entry(MaterialScope::Artifact).content_root != self.hf_content_root
            || manifest.entry(MaterialScope::InputTokens).content_root != self.input_tokens
            || literal_prompt.len() as u64 != self.body_bytes_v1(MaterialScope::Prompt)
            || literal_prompt_id_v1(literal_prompt)? != self.literal_prompt
        {
            return Err(ContractError::Identity.into());
        }
        manifest.open_prompt_text(literal_prompt)?;
        Ok(())
    }

    fn codecs_v1(&self) -> [Digest; MaterialScope::COUNT] {
        [
            hf_artifact_material_codec_v1(),
            tir_program_material_codec_v1(),
            prompt_text_material_codec_v1(),
            Digest(prompt_ids_form_id_v1(self.prompt_form).as_bytes()),
            tokenizer_json_material_codec_v1(),
            tir_ordinary_tfp1_rules_material_codec_v1(),
            tir_cold_initial_state_codec_v1(),
            tir_cold_dependencies_codec_v1(),
            tir_trace_material_codec_v1(),
            tir_output_material_codec_v1(),
        ]
    }

    fn chunk_fits_v1(&self, scope: MaterialScope, chunk_bytes: u32) -> bool {
        if chunk_bytes == 0 {
            return false;
        }
        let chunk = u64::from(chunk_bytes);
        match scope {
            MaterialScope::Artifact => chunk_bytes == self.hf_chunk_bytes && self.body_bytes_v1(scope) <= chunk,
            MaterialScope::Trace => chunk >= (PublicOriginalTraceIndexV1::WIRE_BYTES as u64).max(self.trace.widest_value_bytes),
            MaterialScope::InputTokens if self.prompt_form == PalwPromptIdsFormV1::MerkleV1 => {
                chunk_bytes == PALW_PROMPT_IDS_TILE_LEN * 4
            }
            _ => self.body_bytes_v1(scope) <= chunk,
        }
    }

    pub(crate) fn claim_v1(&self) -> Hash64 {
        self.claim
    }

    pub(crate) fn source_v1(&self) -> Hash64 {
        self.source
    }

    pub(crate) fn tokenizer_registration_v1(&self) -> Hash64 {
        self.tokenizer_registration
    }

    pub(crate) fn literal_prompt_v1(&self) -> Hash64 {
        self.literal_prompt
    }

    pub(crate) fn hf_content_root_v1(&self) -> Digest {
        self.hf_content_root
    }

    pub(crate) fn body_bytes_v1(&self, scope: MaterialScope) -> u64 {
        self.body_bytes[scope as usize]
    }

    pub(crate) fn manifest_body_bytes_v1(&self) -> u64 {
        u64::from(ManifestV1::WIRE_BYTES)
    }

    pub(crate) fn manifest_payload_bytes_v1(&self) -> u64 {
        self.payload_bytes
    }

    pub(crate) fn trace_leaves_v1(&self) -> u64 {
        self.trace.leaves
    }

    pub(crate) fn trace_sibling_visits_v1(&self) -> u64 {
        self.trace.sibling_visits
    }

    pub(crate) fn trace_wire_bytes_v1(&self) -> u64 {
        self.trace.wire_bytes
    }

    pub(crate) fn claim_wire_bytes_v1(&self) -> u64 {
        self.claim_wire_bytes
    }

    pub(crate) fn retained_byte_daa_v1(&self, from: u64, until: u64) -> Result<u64, InitialSupplyErrorV1> {
        let span = until.checked_sub(from).ok_or(ContractError::Range)?;
        self.claim_wire_bytes.checked_mul(span).ok_or(InitialSupplyErrorV1::Overflow)
    }
}

fn literal_prompt_id_v1(prompt: &[u8]) -> Result<Hash64, ContractError> {
    stream_digest(b"misaka-palw/tfp1-initial-supply-prompt/v1", &prompt).map_err(|_| ContractError::NonCanonical)
}

impl TraceSupplyV1 {
    fn derive_v1(leaves: u64, value_bytes: u64, widest_value_bytes: u64) -> Result<Self, InitialSupplyErrorV1> {
        use InitialSupplyErrorV1::*;
        if leaves == 0 || value_bytes == 0 || widest_value_bytes == 0 || widest_value_bytes > value_bytes {
            return Err(MissingTraceGeometry);
        }
        // The public provider's canonical unit is Borsh (PalwStepOpeningV1,
        // PalwStepTileLeafV1). The fixed prefixes include both vector lengths.
        // This matches remote::public_material::trace_tile_wire_overhead.
        const OPENING_PREFIX: u64 = 8 + 64 + 4;
        const TILE_PREFIX: u64 = 2 + 4 * 4 + 4 + 4;
        let depth = step_leg_max_opening_siblings_v1(leaves) as u64;
        let sibling_visits = leaves.checked_mul(depth).ok_or(Overflow)?;
        let wire_bytes = leaves
            .checked_mul(OPENING_PREFIX + TILE_PREFIX)
            .and_then(|bytes| bytes.checked_add(sibling_visits.checked_mul(64)?))
            .and_then(|bytes| bytes.checked_add(value_bytes))
            .and_then(|bytes| bytes.checked_add(PublicOriginalTraceIndexV1::WIRE_BYTES as u64))
            .ok_or(Overflow)?;
        Ok(Self { leaves, sibling_visits, widest_value_bytes, wire_bytes })
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        palw_step::PalwStepCoordinateV1,
        palw_step_leg::{PalwStepOpeningV1, PalwStepTileLeafV1},
    };

    /// Reuse the ordinary TFP1 fixture with an actual earlier registration,
    /// bound container and native Source. The descriptor is local fixture data.
    pub(crate) fn exercise_initial_supply_v1(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        at: &PalwBlockContextV2,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        prompt_form: PalwPromptIdsFormV1,
        raw_tokenizer_bytes: usize,
    ) {
        use misaka_palw_positive_contract::public_one_honest::{
            HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1, HfArtifactDescriptorV1, HfArtifactNativePinsV1,
        };
        let class_id = fp.job.class_id;
        let header = source.header_v1();
        let record = parent.tir_class_v1(&class_id).unwrap();
        let layout = palw_staged_tir_layout_v1(parent, class_id).unwrap();
        let container = palw_tir_bound_container_v1(parent, class_id).unwrap();
        let binding = header.resolve_registered_layout_v1(record, layout, fp.execution_root, source.leaf_count_v1()).unwrap();
        let class = &binding.class;
        let descriptor = HfArtifactDescriptorV1::new_v1(
            HfArtifactNativePinsV1 {
                class: Digest(class_id.as_bytes()),
                program: Digest(record.graph_ir_root.as_bytes()),
                tokenizer: Digest(record.tokenizer_id.as_bytes()),
                inventory: Digest(header.artifact_root.as_bytes()),
                container: Digest(container.container_root.as_bytes()),
                binding_manifest: Digest(container.manifest_digest.as_bytes()),
            },
            "HuggingFaceTB/SmolLM2-135M",
            [1; 20],
            Digest([2; 64]),
            Digest([3; 64]),
        )
        .unwrap();
        let hf = PublicTfp1HfArtifactV1::resolve_v1(
            parent,
            fp,
            source,
            descriptor,
            ManifestScopeRules {
                codec: hf_artifact_material_codec_v1(),
                max_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u64,
                chunk_bytes: HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 as u32,
            },
        )
        .unwrap();
        let supply =
            PublicTfp1InitialSupplyV1::derive_v1(parent, extras, at, fp, source, class, prompt_form, b"fixture prompt", Some(&hf))
                .unwrap();
        assert_eq!(supply.claim_v1(), crate::palw_freeprompt_v3::fp_claim_id_v3(fp));
        assert_eq!(supply.source_v1(), source.source_id_v1());
        assert_eq!(supply.body_bytes_v1(MaterialScope::Artifact), hf.entry_v1().total_bytes);
        assert_eq!(supply.body_bytes_v1(MaterialScope::Program), u64::from(record.program_bytes));
        assert_eq!(supply.body_bytes_v1(MaterialScope::Prompt), b"fixture prompt".len() as u64);
        assert_eq!(supply.body_bytes_v1(MaterialScope::InputTokens), u64::from(fp.job.prompt_tokens) * 4);
        assert_eq!(supply.body_bytes_v1(MaterialScope::Tokenizer), raw_tokenizer_bytes as u64);
        assert_eq!(supply.body_bytes_v1(MaterialScope::Output), u64::from(fp.decode_tokens_executed) * 4);
        assert_eq!(supply.trace_leaves_v1(), source.leaf_count_v1());
        assert_eq!(
            supply.body_bytes_v1(MaterialScope::Trace),
            source.trace_value_bytes_v1().unwrap() + PublicOriginalTraceIndexV1::WIRE_BYTES as u64
        );
        exercise_checked_manifest_v1(&supply, fp, source, &hf);
        exercise_unsigned_material_v3(parent, extras, at, fp, source, class, prompt_form, &supply, &hf);

        let unadvanced = parent.last_point().unwrap();
        assert_eq!(
            PublicTfp1InitialSupplyV1::derive_v1(
                parent,
                extras,
                unadvanced,
                fp,
                source,
                class,
                prompt_form,
                b"fixture prompt",
                Some(&hf)
            ),
            Err(InitialSupplyErrorV1::MissingTokenizerRegistration)
        );
        assert_eq!(
            PublicTfp1InitialSupplyV1::derive_v1(parent, extras, at, fp, source, class, prompt_form, b"fixture prompt", None),
            Err(InitialSupplyErrorV1::MissingHfDescriptor)
        );
    }

    /// The actual registered tokenizer/container fixture reaches the shared
    /// quote/intake material path with prospective clocks only. Test tariffs
    /// and metadata do not authorize a public profile or an accepted carrier.
    #[allow(clippy::too_many_arguments)]
    fn exercise_unsigned_material_v3(
        parent: &PalwChainStateV2,
        extras: &PalwTransitionExtrasV1,
        at: &PalwBlockContextV2,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        class: &PalwTirClassV1,
        form: PalwPromptIdsFormV1,
        supply: &PublicTfp1InitialSupplyV1,
        hf: &PublicTfp1HfArtifactV1,
    ) {
        use crate::palw_positive_contract_v2::{
            PublicOriginalAdmissionV1, PublicOriginalProspectiveReservationV2, tfp1_original::Tfp1PublicAdmissionPolicyV1,
        };
        use misaka_palw_positive_contract::{
            identity::ChainScope,
            messages::AggregateBudget,
            public_one_honest::{FirstNoticeDemand, ManifestEntryV1, MaterialCaseDemand, OriginalCommit, ProtectedRescueDemand},
            rows::AggregateUsage,
        };
        let scope =
            ChainScope { network: Digest(fp.job.network_domain.as_bytes()), ruleset: Digest([91; 64]), branch: Digest([92; 64]) };
        let mut limits = supply.codecs_v1().map(|codec| ManifestScopeRules { codec, max_bytes: 1, chunk_bytes: 1 });
        for kind in MATERIAL_SCOPES {
            let bytes = supply.body_bytes_v1(kind);
            limits[kind as usize].max_bytes = bytes.max(1);
            limits[kind as usize].chunk_bytes = match kind {
                MaterialScope::Artifact => hf.entry_v1().chunk_bytes,
                MaterialScope::InputTokens if form == PalwPromptIdsFormV1::MerkleV1 => PALW_PROMPT_IDS_TILE_LEN * 4,
                MaterialScope::Trace => u32::try_from(bytes.max(136)).unwrap(),
                _ => u32::try_from(bytes.max(1)).unwrap(),
            };
        }
        let capacity = RemainingCapacity {
            transmitted_bytes: supply.claim_wire_bytes_v1(),
            retained_bytes: supply.claim_wire_bytes_v1(),
            additional_work: 100,
        };
        let rescue = ProtectedRescueDemand {
            max_drawn_bonds: 1,
            per_drawn_bond: capacity,
            max_outsiders: 1,
            per_outsider: capacity,
            defensive: capacity,
        };
        let total = capacity.checked_add(rescue.total().unwrap()).unwrap();
        let budget = AggregateBudget {
            rounds: 100,
            unique_source_bytes: supply.claim_wire_bytes_v1(),
            transmitted_bytes: total.transmitted_bytes,
            additional_work: total.additional_work,
            retained_bytes: total.retained_bytes,
            retention_until: at.daa_score + 12_000,
            fee: 100_000,
            witness_attempts: 10,
        };
        let obligation = NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)
            .unwrap()
            .service_obligation_v1(Hash64::from_u64_word(93), Hash64::from_u64_word(94))
            .unwrap();
        let admission = PublicOriginalAdmissionV1::from_native_tfp1_v1(
            fp,
            source,
            class,
            Tfp1PublicAdmissionPolicyV1 {
                scope,
                max_manifest_bytes: ManifestV1::WIRE_BYTES,
                manifest_scope_limits: &limits,
                manifest_total_limit: supply.manifest_payload_bytes_v1(),
            },
            b"fixture prompt",
            obligation,
            &budget,
        )
        .unwrap();
        let job = &source.header_v1().job_context;
        let mut companion = OriginalCommit {
            scope,
            job: Digest(job.job_id.as_bytes()),
            execution: Digest(job.context_hash().as_bytes()),
            miner: admission.miner,
            registered_class: Digest(fp.job.class_id.as_bytes()),
            artifact: Digest(admission.artifact.as_bytes()),
            program: Digest(class.graph_ir_root().as_bytes()),
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
            output: Digest(fp.output_root.as_bytes()),
            trace_root: Digest(fp.execution_root.as_bytes()),
            manifest_root: Digest([0; 64]),
            service_contract: admission.service_contract,
        };
        let subjects = [
            companion.artifact,
            companion.program,
            companion.prompt,
            companion.input_tokens,
            companion.tokenizer,
            companion.execution_rules,
            companion.initial_state,
            companion.dependencies,
            companion.trace_root,
            companion.output,
        ];
        let mut manifest = ManifestV1 {
            service_contract: companion.service_contract,
            entries: std::array::from_fn(|i| ManifestEntryV1 {
                subject: subjects[i],
                content_root: subjects[i],
                codec: limits[i].codec,
                total_bytes: supply.body_bytes[i],
                chunk_bytes: limits[i].chunk_bytes,
            }),
        };
        manifest.entries[MaterialScope::Artifact as usize] = hf.entry_v1();
        manifest.entries[MaterialScope::ExecutionRules as usize].content_root = supply.execution_rules_content;
        manifest.entries[MaterialScope::Prompt as usize] = misaka_palw_positive_contract::public_one_honest::prompt_text_material_v1(
            scope.ruleset,
            "fixture prompt",
            limits[MaterialScope::Prompt as usize],
        )
        .unwrap();
        let manifest_bytes =
            companion.bind_manifest(&manifest, &limits, supply.manifest_payload_bytes_v1(), ManifestV1::WIRE_BYTES).unwrap();
        let descriptor_bytes = hf.descriptor_v1().encode_v1().unwrap();
        let usage = AggregateUsage::default();
        let reservation = PublicOriginalProspectiveReservationV2 {
            accepted_daa: at.daa_score,
            accepted_blue_score: at.blue_score,
            service_obligation: obligation.obligation_v1(),
            notice: FirstNoticeDemand {
                draw_ready_by: at.daa_score + 1,
                material_ready_by: at.daa_score + 2,
                cold_acquisition_daa: 2,
                full_inspection_daa: 2,
                notice_inclusion_daa: 2,
                missing_material_detected_by: at.daa_score + 4,
                material_request_inclusion_daa: 2,
            },
            case: MaterialCaseDemand {
                material_delivery_daa: 2,
                full_inspection_daa: 2,
                localization_daa: 2,
                filing_daa: 2,
                adjudication_and_recovery_daa: 2,
            },
            initial_supply: capacity,
            rescue,
            usage: &usage,
            budget: &budget,
        };
        let check = |prompt: &[u8], manifest: &[u8], descriptor: &[u8], reservation: &PublicOriginalProspectiveReservationV2<'_>| {
            admission.check_tfp1_candidate_material_v3(
                parent,
                extras,
                form,
                fp,
                source,
                &companion,
                prompt,
                manifest,
                descriptor,
                obligation,
                reservation,
            )
        };
        let (same_hf, same_supply) = check(b"fixture prompt", &manifest_bytes, &descriptor_bytes, &reservation).unwrap();
        assert_eq!(same_hf.entry_v1(), hf.entry_v1());
        assert_eq!(&same_supply, supply);
        assert!(check(b"Fixture prompt", &manifest_bytes, &descriptor_bytes, &reservation).is_err());
        let mut wrong_manifest = manifest_bytes.clone();
        wrong_manifest[0] ^= 1;
        assert!(check(b"fixture prompt", &wrong_manifest, &descriptor_bytes, &reservation).is_err());
        let mut wrong_descriptor = descriptor_bytes.clone();
        let last = wrong_descriptor.len() - 1;
        wrong_descriptor[last] ^= 1;
        assert!(check(b"fixture prompt", &manifest_bytes, &wrong_descriptor, &reservation).is_err());
        let stale =
            PublicOriginalProspectiveReservationV2 { accepted_blue_score: parent.last_point().unwrap().blue_score, ..reservation };
        assert!(check(b"fixture prompt", &manifest_bytes, &descriptor_bytes, &stale).is_err());
    }

    fn exercise_checked_manifest_v1(
        supply: &PublicTfp1InitialSupplyV1,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        hf: &PublicTfp1HfArtifactV1,
    ) {
        use misaka_palw_positive_contract::{
            identity::{ChainScope, principal},
            public_one_honest::{ManifestEntryV1, OriginalCommit, prompt_text_material_v1},
        };
        let prompt = "fixture prompt";
        let tag = Digest([42; 64]);
        let job = &source.header_v1().job_context;
        let pins = hf.descriptor_v1().native_pins_v1();
        let mut limits = supply.codecs_v1().map(|codec| ManifestScopeRules { codec, max_bytes: 0, chunk_bytes: 0 });
        for scope in MATERIAL_SCOPES {
            let bytes = supply.body_bytes_v1(scope);
            let limit = &mut limits[scope as usize];
            limit.max_bytes = bytes + 1;
            limit.chunk_bytes = match scope {
                MaterialScope::Artifact => supply.hf_chunk_bytes,
                MaterialScope::InputTokens if supply.prompt_form == PalwPromptIdsFormV1::MerkleV1 => PALW_PROMPT_IDS_TILE_LEN * 4,
                _ => u32::try_from(bytes).unwrap(),
            };
        }
        let prompt_rules = limits[MaterialScope::Prompt as usize];
        let prompt_entry = prompt_text_material_v1(tag, prompt, prompt_rules).unwrap();
        let original = OriginalCommit {
            scope: ChainScope { network: Digest(fp.job.network_domain.as_bytes()), ruleset: tag, branch: tag },
            job: Digest(job.job_id.as_bytes()),
            execution: Digest(job.context_hash().as_bytes()),
            miner: principal(fp.job.executor_pubkey.as_slice().try_into().unwrap()),
            registered_class: pins.class,
            artifact: pins.inventory,
            program: pins.program,
            prompt: prompt_entry.subject,
            input_tokens: Digest(job.prompt_token_ids_hash.as_bytes()),
            tokenizer: pins.tokenizer,
            execution_rules: Digest(job.cu_ruleset_id.as_bytes()),
            generation_seed: job.execution_seed,
            initial_state: tag,
            dependencies: tag,
            prefill: job.declared_prefill_tokens,
            decode: job.exact_decode_tokens,
            max_context: job.max_context_tokens,
            output: Digest(fp.output_root.as_bytes()),
            trace_root: Digest(fp.execution_root.as_bytes()),
            manifest_root: tag,
            service_contract: tag,
        };
        // The body sizes and native HF/Prompt/Input entries are real. Other
        // content roots are fixture metadata for this size-only comparison.
        let subjects = [
            original.artifact,
            original.program,
            original.prompt,
            original.input_tokens,
            original.tokenizer,
            original.execution_rules,
            original.initial_state,
            original.dependencies,
            original.trace_root,
            original.output,
        ];
        let mut manifest = ManifestV1 {
            service_contract: original.service_contract,
            entries: std::array::from_fn(|i| ManifestEntryV1 {
                subject: subjects[i],
                content_root: subjects[i],
                codec: limits[i].codec,
                total_bytes: supply.body_bytes[i],
                chunk_bytes: limits[i].chunk_bytes,
            }),
        };
        manifest.entries[MaterialScope::Artifact as usize] = hf.entry_v1();
        manifest.entries[MaterialScope::ExecutionRules as usize].content_root = supply.execution_rules_content;
        manifest.entries[MaterialScope::Prompt as usize] = prompt_entry;
        let checked = |mut original: OriginalCommit, body: &ManifestV1, limits: &[ManifestScopeRules; MaterialScope::COUNT]| {
            let total_limit = supply.payload_bytes + 1;
            let wire = original.bind_manifest(body, limits, total_limit, ManifestV1::WIRE_BYTES).unwrap();
            let manifest = original.authenticate_manifest(&wire, ManifestV1::WIRE_BYTES).unwrap();
            (original.id(), manifest.decode(&original, limits, total_limit).unwrap())
        };
        let (expected, actual) = checked(original.clone(), &manifest, &limits);
        assert_eq!(supply.check_manifest_v1(expected, &actual, prompt.as_bytes()), Ok(()));
        assert_eq!(
            supply.check_manifest_v1(OriginalId(tag), &actual, prompt.as_bytes()),
            Err(InitialSupplyErrorV1::Contract(ContractError::Scope))
        );
        for scope in MATERIAL_SCOPES {
            let mut changed = manifest.clone();
            changed.entries[scope as usize].total_bytes += 1;
            let (id, changed) = checked(original.clone(), &changed, &limits);
            assert_eq!(
                supply.check_manifest_v1(id, &changed, prompt.as_bytes()),
                Err(InitialSupplyErrorV1::Contract(ContractError::Budget))
            );
        }
        for scope in MATERIAL_SCOPES {
            let mut changed = manifest.clone();
            changed.entries[scope as usize].content_root.0[0] ^= 1;
            let (id, changed) = checked(original.clone(), &changed, &limits);
            assert_eq!(
                supply.check_manifest_v1(id, &changed, prompt.as_bytes()),
                Err(InitialSupplyErrorV1::Contract(ContractError::Identity))
            );
        }
        let mut different_chunk = manifest.clone();
        let mut different_limits = limits;
        different_limits[MaterialScope::Artifact as usize].chunk_bytes += 1;
        different_chunk.entries[MaterialScope::Artifact as usize].chunk_bytes += 1;
        let (id, different_chunk) = checked(original, &different_chunk, &different_limits);
        assert_eq!(
            supply.check_manifest_v1(id, &different_chunk, prompt.as_bytes()),
            Err(InitialSupplyErrorV1::Contract(ContractError::Budget))
        );
        assert_eq!(
            supply.check_manifest_v1(expected, &actual, b"Fixture prompt"),
            Err(InitialSupplyErrorV1::Contract(ContractError::Identity))
        );
    }

    #[test]
    fn initial_tfp1_trace_supply_matches_maximum_opening_wire_v1() {
        // A three-leaf promote-odd tree has maximum depth two. Budget that
        // width for every provider unit, including its two Borsh Vec headers.
        let trace = TraceSupplyV1::derive_v1(3, 24, 8).unwrap();
        let opening = PalwStepOpeningV1 { leaf_index: 0, leaf_hash: Hash64::default(), siblings: vec![Hash64::default(); 2] };
        let tile = PalwStepTileLeafV1 {
            version: 1,
            coord: PalwStepCoordinateV1 { call_index: 0, position: 0, node_slot: 0, tile_index: 0 },
            value_count: 2,
            values_le: vec![0; 8],
        };
        let unit = borsh::object_length(&(opening, tile)).unwrap() as u64;
        assert_eq!(trace.sibling_visits, 6);
        assert_eq!(trace.wire_bytes, 3 * unit + PublicOriginalTraceIndexV1::WIRE_BYTES as u64);
        assert_eq!(TraceSupplyV1::derive_v1(0, 24, 8), Err(InitialSupplyErrorV1::MissingTraceGeometry));
        assert_eq!(TraceSupplyV1::derive_v1(3, 24, 25), Err(InitialSupplyErrorV1::MissingTraceGeometry));
        assert_eq!(TraceSupplyV1::derive_v1(u64::MAX, 24, 8), Err(InitialSupplyErrorV1::Overflow));
        assert_eq!(TraceSupplyV1::derive_v1(1, u64::MAX, 8), Err(InitialSupplyErrorV1::Overflow));
    }

    #[test]
    fn initial_tfp1_supply_separates_manifest_wire_and_retention_v1() {
        let trace = TraceSupplyV1::derive_v1(3, 24, 8).unwrap();
        let body_bytes = [450, 500, 5, 144, 8_192, 512, 128, 68, PublicOriginalTraceIndexV1::WIRE_BYTES as u64 + 24, 8];
        let payload_bytes: u64 = body_bytes.iter().sum();
        let claim_wire_bytes =
            payload_bytes - body_bytes[MaterialScope::Trace as usize] + trace.wire_bytes + u64::from(ManifestV1::WIRE_BYTES);
        let supply = PublicTfp1InitialSupplyV1 {
            claim: Hash64::from_u64_word(1),
            source: Hash64::from_u64_word(2),
            literal_prompt: Hash64::from_u64_word(3),
            hf_content_root: Digest([4; 64]),
            hf_chunk_bytes: 450,
            input_tokens: Digest([6; 64]),
            execution_rules_content: Digest([7; 64]),
            tokenizer_registration: Hash64::from_u64_word(5),
            prompt_form: PalwPromptIdsFormV1::MerkleV1,
            body_bytes,
            payload_bytes,
            trace,
            claim_wire_bytes,
        };
        let codecs = [
            hf_artifact_material_codec_v1(),
            tir_program_material_codec_v1(),
            prompt_text_material_codec_v1(),
            Digest(prompt_ids_form_id_v1(PalwPromptIdsFormV1::MerkleV1).as_bytes()),
            tokenizer_json_material_codec_v1(),
            tir_ordinary_tfp1_rules_material_codec_v1(),
            tir_cold_initial_state_codec_v1(),
            tir_cold_dependencies_codec_v1(),
            tir_trace_material_codec_v1(),
            tir_output_material_codec_v1(),
        ];
        let mut limits = std::array::from_fn(|i| ManifestScopeRules {
            codec: codecs[i],
            max_bytes: body_bytes[i],
            chunk_bytes: body_bytes[i] as u32,
        });
        limits[MaterialScope::InputTokens as usize].chunk_bytes = PALW_PROMPT_IDS_TILE_LEN * 4;
        let capacity = RemainingCapacity { transmitted_bytes: claim_wire_bytes, additional_work: 0, retained_bytes: claim_wire_bytes };
        let check = |max, limits, total, capacity| supply.check_limits_v1(max, limits, total, capacity);
        assert_eq!(check(ManifestV1::WIRE_BYTES, &limits, payload_bytes, capacity), Ok(()));
        assert!(check(ManifestV1::WIRE_BYTES - 1, &limits, payload_bytes, capacity).is_err());
        assert!(check(ManifestV1::WIRE_BYTES, &limits, payload_bytes - 1, capacity).is_err());
        for capacity in [
            RemainingCapacity { transmitted_bytes: claim_wire_bytes - 1, ..capacity },
            RemainingCapacity { retained_bytes: claim_wire_bytes - 1, ..capacity },
        ] {
            assert_eq!(
                check(ManifestV1::WIRE_BYTES, &limits, payload_bytes, capacity),
                Err(InitialSupplyErrorV1::Contract(ContractError::Budget))
            );
        }
        let mut wrong_input_form = limits;
        wrong_input_form[MaterialScope::InputTokens as usize].chunk_bytes += 1;
        assert!(check(ManifestV1::WIRE_BYTES, &wrong_input_form, payload_bytes, capacity).is_err());
        let mut short_trace = limits;
        short_trace[MaterialScope::Trace as usize].chunk_bytes = PublicOriginalTraceIndexV1::WIRE_BYTES as u32 - 1;
        assert!(check(ManifestV1::WIRE_BYTES, &short_trace, payload_bytes, capacity).is_err());
        assert_eq!(supply.retained_byte_daa_v1(10, 12), Ok(claim_wire_bytes * 2));
        assert_eq!(supply.retained_byte_daa_v1(12, 10), Err(InitialSupplyErrorV1::Contract(ContractError::Range)));
        assert_eq!(supply.retained_byte_daa_v1(0, u64::MAX), Err(InitialSupplyErrorV1::Overflow));
    }
}
