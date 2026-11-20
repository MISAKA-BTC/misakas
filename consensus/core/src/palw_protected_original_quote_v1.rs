//! Unsigned protected-original quote. The native admission planner owns the
//! policy, material checks and pricing. No accepted origin or funds are created.
use crate::{
    Hash64,
    palw_court_original_escrow_v1::OriginalFundingScopeV1,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_positive_contract_v2::tfp1_original::{
        native_plan::NativeTfp1OriginalPlanV3, public_policy::resolve_protected_planning_policy_v3,
    },
    palw_public_binding_v1::PalwPublicTirBindingHeaderV1,
    palw_state_v2::{PalwChainStateV2, PalwStateParamsV2, PalwTransitionExtrasV1},
};
pub use misaka_palw_positive_contract::public_one_honest::PromptMaterialEncodingV1;
use misaka_palw_positive_contract::public_one_honest::{ManifestEntryV1, MaterialScope, OriginalCommit};

pub const PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1: usize =
    crate::palw_tir_funded_original_envelope_v1::PALW_TIR_FUNDED_ORIGINAL_MAX_BYTES_V1;

/// Explicitly encoded publisher material, supplied before companion/funding signatures.
/// The entry subjects, sizes and native content roots are rechecked by the planner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectedOriginalQuoteMaterialV1 {
    pub prompt_encoding: PromptMaterialEncodingV1,
    pub prompt: Vec<u8>,
    pub hf_descriptor: Vec<u8>,
    pub entries: [ManifestEntryV1; MaterialScope::COUNT],
}
impl ProtectedOriginalQuoteMaterialV1 {
    /// Before signing, bind the retained prompt bytes to an explicit complete
    /// planning declaration. Other material and the native claim stay intact.
    /// The live quote must still authenticate this policy against its parent.
    pub fn with_planning_declaration_v3(
        mut self,
        declaration: &crate::palw_positive_contract_v2::ProtectedPlanningDeclarationV3,
    ) -> Result<Self, String> {
        self.check_bounds_v1()?;
        let entry = self.entries[MaterialScope::Prompt as usize];
        let prompt = std::str::from_utf8(&self.prompt).map_err(|_| "protected prompt is not UTF-8")?;
        self.entries[MaterialScope::Prompt as usize] = self
            .prompt_encoding
            .material_v1(
                declaration.prompt_ruleset_v3(self.prompt_encoding)?,
                prompt,
                misaka_palw_positive_contract::public_one_honest::ManifestScopeRules {
                    codec: entry.codec,
                    max_bytes: entry.total_bytes,
                    chunk_bytes: entry.chunk_bytes,
                },
            )
            .map_err(|e| format!("protected planning prompt: {e:?}"))?;
        Ok(self)
    }

    /// Preserve the exact worker input. Token IDs alone cannot invent public text.
    pub fn prompt_of_worker_input_v2(
        input: &crate::palw_freeprompt_v3::PalwFpWorkerInputV3,
        max_bytes: u64,
    ) -> Result<(PromptMaterialEncodingV1, Vec<u8>), String> {
        use crate::palw_freeprompt_v3::{PalwFpPromptSegmentV1 as Segment, PalwFpWorkerInputV3 as Input};
        use misaka_palw_positive_contract::public_one_honest::{PromptSegmentRefV1, encode_prompt_segments_v1};
        match input {
            Input::Text(bytes) => {
                PromptMaterialEncodingV1::LiteralUtf8.check_body_v1(bytes, max_bytes).map_err(|e| format!("worker text: {e:?}"))?;
                Ok((PromptMaterialEncodingV1::LiteralUtf8, bytes.clone()))
            }
            Input::Segments(segments) => {
                // Every segment needs framing bytes; reject count before copying views.
                if segments.len() as u64 > max_bytes {
                    return Err("worker segment count exceeds prompt bound".into());
                }
                let mut views = Vec::new();
                views.try_reserve_exact(segments.len()).map_err(|_| "worker segment allocation")?;
                for segment in segments {
                    views.push(match segment {
                        Segment::Text(bytes) => {
                            PromptSegmentRefV1::Text(std::str::from_utf8(bytes).map_err(|_| "worker text is not UTF-8")?)
                        }
                        Segment::Special(id) => PromptSegmentRefV1::Special(*id),
                    });
                }
                let body = encode_prompt_segments_v1(&views, max_bytes).map_err(|e| format!("worker segments: {e:?}"))?;
                Ok((PromptMaterialEncodingV1::SegmentsV1, body.into_bytes()))
            }
            Input::TokenIds(_) => Err("protected public prompt needs the retained Text or Segments request".into()),
        }
    }

    fn check_bounds_v1(&self) -> Result<(), String> {
        let entries = borsh::object_length(&self.entries).map_err(|e| e.to_string())?;
        let bytes = 10usize
            .checked_add(self.prompt.len())
            .and_then(|n| n.checked_add(self.hf_descriptor.len()))
            .and_then(|n| n.checked_add(entries))
            .ok_or("protected quote length overflow")?;
        if bytes > PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1
            || self.hf_descriptor.len() > misaka_palw_positive_contract::public_one_honest::HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1
        {
            return Err("protected quote material exceeds its carrier bound".into());
        }
        self.prompt_encoding
            .check_body_v1(&self.prompt, PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1 as u64)
            .map_err(|e| format!("protected quote prompt: {e:?}"))?;
        Ok(())
    }

    pub fn encode_v1(&self) -> Result<Vec<u8>, String> {
        self.check_bounds_v1()?;
        let version = match self.prompt_encoding {
            PromptMaterialEncodingV1::LiteralUtf8 => 1u16,
            PromptMaterialEncodingV1::SegmentsV1 => 2u16,
        };
        let bytes = borsh::to_vec(&(version, &self.prompt, &self.hf_descriptor, &self.entries)).map_err(|e| e.to_string())?;
        if bytes.len() > PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1 {
            return Err("protected quote material exceeds its carrier bound".into());
        }
        Ok(bytes)
    }
    pub fn decode_v1(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1 || bytes.len() < 2 {
            return Err("protected quote material version or length".into());
        }
        let prompt_encoding = match u16::from_le_bytes(bytes[..2].try_into().unwrap()) {
            1 => PromptMaterialEncodingV1::LiteralUtf8,
            2 => PromptMaterialEncodingV1::SegmentsV1,
            _ => return Err("protected quote material version".into()),
        };
        // Validate each declared length against the actual bounded body before
        // allocating. A forged u32 length cannot request a large Vec allocation.
        fn take<'a>(rest: &mut &'a [u8]) -> Result<&'a [u8], String> {
            let prefix = rest.get(..4).ok_or("protected quote material length")?;
            let len = u32::from_le_bytes(prefix.try_into().unwrap()) as usize;
            *rest = &rest[4..];
            let value = rest.get(..len).ok_or("protected quote material truncated")?;
            *rest = &rest[len..];
            Ok(value)
        }
        let mut rest = &bytes[2..];
        let prompt = take(&mut rest)?;
        std::str::from_utf8(prompt).map_err(|_| "protected quote prompt is not UTF-8")?;
        let descriptor = take(&mut rest)?;
        if descriptor.len() > misaka_palw_positive_contract::public_one_honest::HF_ARTIFACT_DESCRIPTOR_MAX_BYTES_V1 {
            return Err("protected quote descriptor exceeds its bound".into());
        }
        let entries = borsh::from_slice(rest).map_err(|e| format!("protected quote entries: {e}"))?;
        prompt_encoding
            .check_body_v1(prompt, PROTECTED_QUOTE_MATERIAL_MAX_BYTES_V1 as u64)
            .map_err(|e| format!("protected quote prompt: {e:?}"))?;
        Ok(Self { prompt_encoding, prompt: prompt.to_vec(), hf_descriptor: descriptor.to_vec(), entries })
    }
}

/// Advisory values to sign. The accepting block recalculates these on its own
/// ordered prefix. This carries neither booked slots nor an AcceptedOrigin.
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct ProtectedOriginalQuoteV1 {
    pub version: u16,
    pub parent: Hash64,
    pub quote_daa: u64,
    pub quote_blue_score: u64,
    pub claim: Hash64,
    pub companion: OriginalCommit,
    pub manifest: Vec<u8>,
    pub funding: OriginalFundingScopeV1,
    pub original_fee_sompi: u64,
    pub future_carrier_fee_sompi: u64,
}

impl ProtectedOriginalQuoteV1 {
    /// Absolute payer limits, derived without inventing a public acceptance
    /// window. V2 may be accepted until the executor's signed trace horizon
    /// can no longer cover the entire relative service retention duration.
    pub fn funding_window_v2(&self, original: &PalwFreePromptCommitmentV3) -> Result<(u64, u64, u64), String> {
        if self.version == 1 {
            return Ok((self.quote_daa, self.quote_daa, self.funding.required_material_until_daa));
        }
        if !matches!(self.version, 2 | 3) {
            return Err("unknown protected quote version".into());
        }
        let duration = self
            .funding
            .required_material_until_daa
            .checked_sub(self.quote_daa)
            .filter(|duration| *duration >= crate::palw_court_policy_v1::DISPUTE_WINDOW_DAA)
            .ok_or("protected quote does not cover the hard retention window")?;
        let until = original
            .trace_retention_daa
            .checked_sub(duration)
            .filter(|until| *until >= self.quote_daa)
            .ok_or("signed trace retention cannot cover protected original acceptance")?;
        Ok((self.quote_daa, until, original.trace_retention_daa))
    }

    /// Client-side identity checks before either actor signs. Native admission
    /// still recomputes policy, prices, class bindings and service capacity.
    pub fn check_signing_material_v1(
        &self,
        original: &PalwFreePromptCommitmentV3,
        header: &PalwPublicTirBindingHeaderV1,
        material: &ProtectedOriginalQuoteMaterialV1,
        executor_key: &[u8],
    ) -> Result<(), String> {
        use misaka_palw_positive_contract::{
            identity::{Digest, principal},
            public_one_honest::{HfArtifactDescriptorV1, ManifestScopeRules, ManifestV1},
        };
        material.check_bounds_v1()?;
        let job = &header.job_context;
        let companion = &self.companion;
        if !matches!(
            (self.version, material.prompt_encoding),
            (1 | 2, PromptMaterialEncodingV1::LiteralUtf8) | (3, PromptMaterialEncodingV1::SegmentsV1)
        ) || self.parent == Hash64::default()
            || self.claim != fp_claim_id_v3(original)
            || self.funding.claim != self.claim
            || self.funding.execution_root != original.execution_root
            || self.funding.job_identity != crate::palw_fp_execution_v3::palw_fp_job_pin_v1(original)
            || self.funding.source_id != crate::palw_tir_public_source_v1::public_source_header_id_v1(header)?
            || executor_key != original.job.executor_pubkey.as_slice()
            || companion.miner != principal(executor_key.try_into().map_err(|_| "executor key width")?)
            || companion.scope.network != Digest(original.job.network_domain.as_bytes())
            || companion.registered_class != Digest(original.job.class_id.as_bytes())
            || companion.artifact != Digest(header.artifact_root.as_bytes())
            || companion.job != Digest(job.job_id.as_bytes())
            || companion.execution != Digest(job.context_hash().as_bytes())
            || companion.output != Digest(original.output_root.as_bytes())
            || companion.trace_root != Digest(original.execution_root.as_bytes())
            || companion.input_tokens != Digest(job.prompt_token_ids_hash.as_bytes())
            || companion.tokenizer != Digest(job.tokenizer_id.as_bytes())
            || companion.execution_rules != Digest(job.cu_ruleset_id.as_bytes())
            || companion.generation_seed != job.execution_seed
            || companion.prefill != job.declared_prefill_tokens
            || companion.decode != job.exact_decode_tokens
            || companion.max_context != job.max_context_tokens
            || self.funding.required_amount_sompi == 0
            || self.funding.required_material_until_daa <= self.quote_daa
        {
            return Err("protected quote does not describe this original and executor".into());
        }
        self.funding_window_v2(original)?;
        let literal = borsh::to_vec(&ManifestV1 { service_contract: companion.service_contract, entries: material.entries })
            .map_err(|e| e.to_string())?;
        if self.manifest != literal {
            return Err("quote changed publisher manifest entries".into());
        }
        let limits = material.entries.map(|entry| ManifestScopeRules {
            codec: entry.codec,
            max_bytes: entry.total_bytes,
            chunk_bytes: entry.chunk_bytes,
        });
        let total = material
            .entries
            .iter()
            .try_fold(0u64, |n, entry| n.checked_add(entry.total_bytes))
            .ok_or("manifest byte total overflow")?;
        companion
            .authenticate_manifest(&self.manifest, ManifestV1::WIRE_BYTES)
            .and_then(|manifest| manifest.decode(companion, &limits, total))
            .map_err(|e| format!("quoted manifest: {e:?}"))?;
        let descriptor =
            HfArtifactDescriptorV1::decode_v1(&material.hf_descriptor).map_err(|e| format!("quoted descriptor: {e:?}"))?;
        let artifact = MaterialScope::Artifact as usize;
        let pins = descriptor.native_pins_v1();
        if descriptor.manifest_entry_v1(limits[artifact]).map_err(|e| format!("quoted artifact: {e:?}"))? != material.entries[artifact]
            || pins.class != companion.registered_class
            || pins.inventory != companion.artifact
            || pins.program != companion.program
            || pins.tokenizer != companion.tokenizer
        {
            return Err("quoted descriptor differs from the literal artifact".into());
        }
        let prompt = &material.entries[MaterialScope::Prompt as usize];
        let text = std::str::from_utf8(&material.prompt).map_err(|_| "prompt UTF-8")?;
        let actual = material
            .prompt_encoding
            .material_v1(
                companion.scope.ruleset,
                text,
                ManifestScopeRules { codec: prompt.codec, max_bytes: prompt.total_bytes, chunk_bytes: prompt.chunk_bytes },
            )
            .map_err(|e| format!("quoted prompt: {e:?}"))?;
        if &actual != prompt || actual.subject != companion.prompt {
            return Err("quoted literal prompt differs".into());
        }
        Ok(())
    }

    pub fn signed_companion_bytes_v1(&self, key: &[u8], signature: &[u8]) -> Result<Vec<u8>, String> {
        use misaka_palw_positive_contract::public_one_honest::SignedPublicOriginalV1;
        let key = key.try_into().map_err(|_| "companion key width")?;
        let signature = signature.try_into().map_err(|_| "companion signature width")?;
        self.companion.authenticate(&key, &signature).map_err(|e| format!("companion signature: {e:?}"))?;
        borsh::to_vec(&SignedPublicOriginalV1 { original: self.companion.clone(), key, signature }).map_err(|e| e.to_string())
    }
}

pub use misaka_palw_positive_contract::public_one_honest::ORIGINAL_SIGN_CONTEXT as PROTECTED_ORIGINAL_SIGN_CONTEXT_V1;

#[allow(clippy::too_many_arguments)]
pub fn quote_protected_original_v1(
    parent: &PalwChainStateV2,
    params: &PalwStateParamsV2,
    extras: &PalwTransitionExtrasV1,
    daa: u64,
    blue: u64,
    original: &PalwFreePromptCommitmentV3,
    header: &PalwPublicTirBindingHeaderV1,
    material: &ProtectedOriginalQuoteMaterialV1,
) -> Result<ProtectedOriginalQuoteV1, String> {
    material.check_bounds_v1()?;
    let point = parent.last_point().ok_or("protected quote has no accepted parent")?;
    if daa < point.daa_score || blue < point.blue_score {
        return Err("protected quote clock precedes its parent".into());
    }
    let form = extras.prompt_ids_form_v1();
    let schedules = crate::palw_positive_contract_v2::tfp1_original::public_policy::ProtectedPlanningSchedulesV3::selected_v3(
        extras.protected_planning_v3,
    )
    .map_err(|error| format!("protected original schedules: {error:?}"))?;
    let pinned = resolve_protected_planning_policy_v3(
        parent,
        extras,
        params,
        daa,
        blue,
        original,
        header,
        &material.prompt,
        &material.hf_descriptor,
        form,
        true,
        material.prompt_encoding,
        schedules,
    )
    .map_err(|error| format!("protected original policy: {error:?}"))?;
    let policy = pinned.policy();
    let price = |stage, bytes| pinned.price_delivery_v1(stage, bytes);
    let plan = NativeTfp1OriginalPlanV3::derive_v3(
        parent,
        extras,
        params,
        daa,
        blue,
        original,
        header,
        form,
        &material.prompt,
        &material.hf_descriptor,
        &policy,
        price,
    )
    .map_err(|e| format!("protected original planning: {e:?}"))?;
    let quote = plan
        .quote_unsigned_v3(original, &material.prompt, &material.hf_descriptor, material.entries, price)
        .map_err(|e| format!("protected original quote: {e:?}"))?;
    Ok(ProtectedOriginalQuoteV1 {
        version: if material.prompt_encoding == PromptMaterialEncodingV1::SegmentsV1 { 3 } else { 2 },
        parent: point.block,
        quote_daa: daa,
        quote_blue_score: blue,
        claim: fp_claim_id_v3(original),
        companion: quote.material.companion,
        manifest: quote.material.manifest.to_vec(),
        funding: quote.funding.funding,
        original_fee_sompi: quote.funding.original_fee_sompi,
        future_carrier_fee_sompi: quote.funding.future_carrier_fee_sompi,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_positive_contract::identity::Digest;

    #[test]
    fn explicit_planning_changes_only_prompt_scope_for_each_encoding() {
        use crate::palw_freeprompt_v3::{PalwFpPromptSegmentV1 as Segment, PalwFpWorkerInputV3 as Input};
        use crate::palw_positive_contract_v2::ProtectedPlanningDeclarationV3;
        use misaka_palw_positive_contract::public_one_honest::ManifestScopeRules;
        let bytes = include_bytes!("../tests/fixtures/protected-planning-schema-v3.json");
        let declaration = ProtectedPlanningDeclarationV3::decode_json_v3(bytes).unwrap();
        let mut changed: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        changed["service_book"]["budget"]["work"] = serde_json::json!(100001);
        let changed = ProtectedPlanningDeclarationV3::decode_json_v3(&serde_json::to_vec(&changed).unwrap()).unwrap();
        for input in [
            Input::Text(b"same prompt".to_vec()),
            Input::Segments(vec![Segment::Special(128), Segment::Text(b"same prompt".to_vec())]),
        ] {
            let (prompt_encoding, prompt) = ProtectedOriginalQuoteMaterialV1::prompt_of_worker_input_v2(&input, 4096).unwrap();
            let entry = prompt_encoding
                .material_v1(
                    Digest([7; 64]),
                    std::str::from_utf8(&prompt).unwrap(),
                    ManifestScopeRules { codec: prompt_encoding.codec_v1(), max_bytes: 4096, chunk_bytes: u32::MAX },
                )
                .unwrap();
            let original = ProtectedOriginalQuoteMaterialV1 {
                prompt_encoding,
                prompt,
                hf_descriptor: vec![9; 16],
                entries: [entry; MaterialScope::COUNT],
            };
            let bound = original.clone().with_planning_declaration_v3(&declaration).unwrap();
            assert_eq!(bound.prompt, original.prompt);
            assert_eq!(bound.hf_descriptor, original.hf_descriptor);
            assert_eq!(bound.prompt_encoding, original.prompt_encoding);
            for index in 0..MaterialScope::COUNT {
                if index == MaterialScope::Prompt as usize {
                    assert_ne!(bound.entries[index], original.entries[index]);
                } else {
                    assert_eq!(bound.entries[index], original.entries[index]);
                }
            }
            assert_eq!(bound.clone().with_planning_declaration_v3(&declaration).unwrap(), bound);
            assert_ne!(
                bound.clone().with_planning_declaration_v3(&changed).unwrap().entries[MaterialScope::Prompt as usize],
                bound.entries[MaterialScope::Prompt as usize]
            );
            let mut invalid = original;
            invalid.entries[MaterialScope::Prompt as usize].codec = Digest([0; 64]);
            assert!(invalid.with_planning_declaration_v3(&declaration).is_err());
        }
    }

    #[test]
    fn protected_quote_material_refuses_truncated_lengths_and_cross_version_bodies() {
        let material = ProtectedOriginalQuoteMaterialV1 {
            prompt_encoding: PromptMaterialEncodingV1::LiteralUtf8,
            prompt: "literal prompt".as_bytes().to_vec(),
            hf_descriptor: vec![7; 16],
            entries: [ManifestEntryV1 {
                subject: Digest([1; 64]),
                content_root: Digest([2; 64]),
                codec: Digest([3; 64]),
                total_bytes: 4,
                chunk_bytes: 4,
            }; MaterialScope::COUNT],
        };
        let bytes = material.encode_v1().unwrap();
        assert_eq!(ProtectedOriginalQuoteMaterialV1::decode_v1(&bytes).unwrap(), material);
        for end in [0, 1, 2, 5, bytes.len() - 1] {
            assert!(ProtectedOriginalQuoteMaterialV1::decode_v1(&bytes[..end]).is_err());
        }
        let mut forged = bytes.clone();
        forged[2..6].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(ProtectedOriginalQuoteMaterialV1::decode_v1(&forged).is_err());
        let mut wrong_version = bytes.clone();
        wrong_version[0] = 2;
        assert!(ProtectedOriginalQuoteMaterialV1::decode_v1(&wrong_version).is_err());
        let mut segmented = material.clone();
        let input = crate::palw_freeprompt_v3::PalwFpWorkerInputV3::Segments(vec![
            crate::palw_freeprompt_v3::PalwFpPromptSegmentV1::Special(128),
            crate::palw_freeprompt_v3::PalwFpPromptSegmentV1::Text(b"<|im_start|>".to_vec()),
        ]);
        (segmented.prompt_encoding, segmented.prompt) =
            ProtectedOriginalQuoteMaterialV1::prompt_of_worker_input_v2(&input, 4096).unwrap();
        let encoded = segmented.encode_v1().unwrap();
        assert_eq!(&encoded[..2], &2u16.to_le_bytes());
        assert_eq!(ProtectedOriginalQuoteMaterialV1::decode_v1(&encoded).unwrap(), segmented);
        let mut literal_marker = segmented.clone();
        literal_marker.prompt_encoding = PromptMaterialEncodingV1::LiteralUtf8;
        let legacy = literal_marker.encode_v1().unwrap();
        assert_eq!(&legacy[..2], &1u16.to_le_bytes());
        assert_eq!(ProtectedOriginalQuoteMaterialV1::decode_v1(&legacy).unwrap(), literal_marker);
        assert!(
            ProtectedOriginalQuoteMaterialV1::prompt_of_worker_input_v2(
                &crate::palw_freeprompt_v3::PalwFpWorkerInputV3::TokenIds(vec![128]),
                4096
            )
            .is_err()
        );
        let mut unknown = encoded.clone();
        unknown[0] = 3;
        assert!(ProtectedOriginalQuoteMaterialV1::decode_v1(&unknown).is_err());
        let mut trailing = bytes;
        trailing.push(0);
        assert!(ProtectedOriginalQuoteMaterialV1::decode_v1(&trailing).is_err());
    }
}
