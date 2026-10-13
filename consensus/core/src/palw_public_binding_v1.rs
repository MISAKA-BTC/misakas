//! Compact original public execution metadata. Profiles/programs resolve only
//! from authenticated registered state; no synthetic legacy binding represents TIR.
use crate::{Hash64, palw_v2::PalwJobContextV2};
use borsh::{BorshDeserialize, BorshSerialize};

pub(crate) fn read_bounded_bytes_v1<R: std::io::Read>(reader: &mut R, maximum: usize) -> std::io::Result<Vec<u8>> {
    let length = u32::deserialize_reader(reader)? as usize;
    if length > maximum {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "public byte vector exceeds its bound before allocation"));
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::OutOfMemory, "bounded public allocation failed"))?;
    bytes.resize(length, 0);
    reader.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub(crate) fn read_signature_v1<R: std::io::Read>(reader: &mut R) -> std::io::Result<Vec<u8>> {
    let signature = read_bounded_bytes_v1(reader, crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN)?;
    if signature.len() != crate::mldsa87_primitives::MLDSA87_SIGNATURE_LEN {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "public signature has noncanonical length"));
    }
    Ok(signature)
}
fn read_job_context_v1<R: std::io::Read>(reader: &mut R) -> std::io::Result<PalwJobContextV2> {
    Ok(PalwJobContextV2 {
        version: BorshDeserialize::deserialize_reader(reader)?,
        network_id: read_bounded_bytes_v1(reader, crate::palw_v2::PALW_V2_MAX_NETWORK_ID_BYTES)?,
        job_id: BorshDeserialize::deserialize_reader(reader)?,
        job_nullifier: BorshDeserialize::deserialize_reader(reader)?,
        assignment_id: BorshDeserialize::deserialize_reader(reader)?,
        execution_seed: BorshDeserialize::deserialize_reader(reader)?,
        model_profile_id: BorshDeserialize::deserialize_reader(reader)?,
        runtime_manifest_hash: BorshDeserialize::deserialize_reader(reader)?,
        runtime_class_id: BorshDeserialize::deserialize_reader(reader)?,
        shape_profile_id: BorshDeserialize::deserialize_reader(reader)?,
        trace_scheme_id: BorshDeserialize::deserialize_reader(reader)?,
        cu_ruleset_id: BorshDeserialize::deserialize_reader(reader)?,
        tokenizer_id: BorshDeserialize::deserialize_reader(reader)?,
        prompt_token_ids_hash: BorshDeserialize::deserialize_reader(reader)?,
        declared_prefill_tokens: BorshDeserialize::deserialize_reader(reader)?,
        exact_decode_tokens: BorshDeserialize::deserialize_reader(reader)?,
        max_context_tokens: BorshDeserialize::deserialize_reader(reader)?,
    })
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwPublicLegacyBindingHeaderV1 {
    pub version: u16,
    pub job_context: PalwJobContextV2,
    pub checkpoint_profile: crate::palw_legs::PalwCheckpointProfileV1,
    pub state_chunk_map_id: Hash64,
    pub full_logits_trace_root: Hash64,
    pub activation_leg_root: Hash64,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
    pub checkpoint_count: u32,
    pub checkpoint_merkle_root: Hash64,
    pub committed_execution_root: Hash64,
}
impl BorshDeserialize for PalwPublicLegacyBindingHeaderV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            job_context: read_job_context_v1(reader)?,
            checkpoint_profile: BorshDeserialize::deserialize_reader(reader)?,
            state_chunk_map_id: BorshDeserialize::deserialize_reader(reader)?,
            full_logits_trace_root: BorshDeserialize::deserialize_reader(reader)?,
            activation_leg_root: BorshDeserialize::deserialize_reader(reader)?,
            step_leaf_count: BorshDeserialize::deserialize_reader(reader)?,
            step_merkle_root: BorshDeserialize::deserialize_reader(reader)?,
            checkpoint_count: BorshDeserialize::deserialize_reader(reader)?,
            checkpoint_merkle_root: BorshDeserialize::deserialize_reader(reader)?,
            committed_execution_root: BorshDeserialize::deserialize_reader(reader)?,
        })
    }
}
impl PalwPublicLegacyBindingHeaderV1 {
    pub fn of_binding_v1(binding: &crate::palw_step_leg::PalwStepBindingV2) -> Self {
        Self {
            version: binding.version,
            job_context: binding.job_context.clone(),
            checkpoint_profile: binding.checkpoint_profile.clone(),
            state_chunk_map_id: binding.state_chunk_map_id,
            full_logits_trace_root: binding.full_logits_trace_root,
            activation_leg_root: binding.activation_leg_root,
            step_leaf_count: binding.step_leaf_count,
            step_merkle_root: binding.step_merkle_root,
            checkpoint_count: binding.checkpoint_count,
            checkpoint_merkle_root: binding.checkpoint_merkle_root,
            committed_execution_root: binding.committed_execution_root,
        }
    }
    pub fn resolve_v1(
        &self,
        profile: &crate::palw_step::PalwShapeProfileV3,
        expected_root: Hash64,
        maximum_leaves: u64,
    ) -> Result<crate::palw_step_leg::PalwStepBindingV2, String> {
        if self.committed_execution_root != expected_root
            || self.step_leaf_count > maximum_leaves
            || profile.shape_profile_id() != self.job_context.shape_profile_id
        {
            return Err("public legacy header changes original execution, registered profile or ladder".into());
        }
        let binding = crate::palw_step_leg::PalwStepBindingV2 {
            version: self.version,
            job_context: self.job_context.clone(),
            shape_profile: profile.clone(),
            checkpoint_profile: self.checkpoint_profile.clone(),
            state_chunk_map_id: self.state_chunk_map_id,
            full_logits_trace_root: self.full_logits_trace_root,
            activation_leg_root: self.activation_leg_root,
            step_leaf_count: self.step_leaf_count,
            step_merkle_root: self.step_merkle_root,
            checkpoint_count: self.checkpoint_count,
            checkpoint_merkle_root: self.checkpoint_merkle_root,
            committed_execution_root: self.committed_execution_root,
        };
        crate::palw_step_leg::verify_binding_v1(&binding).map_err(|e| e.to_string())?;
        Ok(binding)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize)]
pub struct PalwPublicTirBindingHeaderV1 {
    pub version: u16,
    pub job_context: PalwJobContextV2,
    pub artifact_root: Hash64,
    pub full_logits_trace_root: Hash64,
    pub step_leaf_count: u64,
    pub step_merkle_root: Hash64,
    pub committed_execution_root: Hash64,
}
impl BorshDeserialize for PalwPublicTirBindingHeaderV1 {
    fn deserialize_reader<R: std::io::Read>(reader: &mut R) -> std::io::Result<Self> {
        Ok(Self {
            version: BorshDeserialize::deserialize_reader(reader)?,
            job_context: read_job_context_v1(reader)?,
            artifact_root: BorshDeserialize::deserialize_reader(reader)?,
            full_logits_trace_root: BorshDeserialize::deserialize_reader(reader)?,
            step_leaf_count: BorshDeserialize::deserialize_reader(reader)?,
            step_merkle_root: BorshDeserialize::deserialize_reader(reader)?,
            committed_execution_root: BorshDeserialize::deserialize_reader(reader)?,
        })
    }
}
impl PalwPublicTirBindingHeaderV1 {
    pub fn of_binding_v1(binding: &crate::palw_tir_step_v1::PalwTirStepBindingV1) -> Self {
        Self {
            version: binding.version,
            job_context: binding.job_context.clone(),
            artifact_root: binding.artifact_root,
            full_logits_trace_root: binding.full_logits_trace_root,
            step_leaf_count: binding.step_leaf_count,
            step_merkle_root: binding.step_merkle_root,
            committed_execution_root: binding.committed_execution_root,
        }
    }
    pub fn resolve_v1(
        &self,
        class: &crate::palw_tir_class_v1::PalwTirClassV1,
        expected_root: Hash64,
        maximum_leaves: u64,
    ) -> Result<crate::palw_tir_step_v1::PalwTirStepBindingV1, String> {
        if self.committed_execution_root != expected_root || class.class_id(&self.artifact_root) != self.job_context.shape_profile_id {
            return Err("public TIR header changes the original execution or registered program/class".into());
        }
        let binding = crate::palw_tir_step_v1::PalwTirStepBindingV1 {
            version: self.version,
            job_context: self.job_context.clone(),
            class: class.clone(),
            artifact_root: self.artifact_root,
            full_logits_trace_root: self.full_logits_trace_root,
            step_leaf_count: self.step_leaf_count,
            step_merkle_root: self.step_merkle_root,
            committed_execution_root: self.committed_execution_root,
        };
        crate::palw_tir_step_v1::verify_tir_binding_v1(&binding, maximum_leaves).map_err(|e| e.to_string())?;
        Ok(binding)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum PalwPublicExecutionBindingHeaderV1 {
    Legacy(PalwPublicLegacyBindingHeaderV1),
    Tir(PalwPublicTirBindingHeaderV1),
}
impl PalwPublicExecutionBindingHeaderV1 {
    pub fn job_context_v1(&self) -> &PalwJobContextV2 {
        match self {
            Self::Legacy(header) => &header.job_context,
            Self::Tir(header) => &header.job_context,
        }
    }
    pub fn execution_root_v1(&self) -> Hash64 {
        match self {
            Self::Legacy(header) => header.committed_execution_root,
            Self::Tir(header) => header.committed_execution_root,
        }
    }
    pub fn check_wire_v1(&self) -> Result<(), String> {
        crate::palw_slash::check_job_context_shape(self.job_context_v1()).map_err(|e| e.to_string())?;
        if borsh::object_length(self).map_err(|e| e.to_string())? > 2048 {
            return Err("public original binding header exceeds its fixed metadata envelope".into());
        }
        Ok(())
    }
}
