//! Identity-only HF reference for the new ordinary public-material profile.
//! No Node network access, availability decision or computation certification.
use crate::{
    Hash64,
    palw_freeprompt_v3::{PalwFreePromptCommitmentV3, fp_claim_id_v3},
    palw_state_v2::PalwChainStateV2,
    palw_tir_class_v1::PalwTirClassV1,
    palw_tir_container_v1::palw_tir_bound_container_v1,
    palw_tir_public_source_v1::PalwTirPublicSourceV1,
};
use misaka_palw_positive_contract::{
    ContractError,
    identity::Digest,
    public_one_honest::{
        HfArtifactDescriptorV1, HfArtifactNativePinsV1, ManifestEntryV1, ManifestScopeRules, ManifestV1, MaterialScope, OriginalCommit,
    },
};

/// Resolved data, not an admission capability. The caller must authenticate
/// the parent snapshot. Private fields prevent replacing the expected roots
/// with a recipe's self-declared roots after resolution.
pub struct PublicTfp1HfArtifactV1 {
    claim: Hash64,
    source: Hash64,
    descriptor: HfArtifactDescriptorV1,
    entry: ManifestEntryV1,
}
impl PublicTfp1HfArtifactV1 {
    pub fn resolve_v1(
        parent: &PalwChainStateV2,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        descriptor: HfArtifactDescriptorV1,
        rules: ManifestScopeRules,
    ) -> Result<Self, ContractError> {
        super::tfp1_original::NativeTfp1PublicOriginalFactsV1::derive_v1(fp, source)?;
        let header = source.header_v1();
        super::tfp1_material::require_cold_tfp1_v1(fp, &header.job_context)?;
        let class = fp.job.class_id;
        let registered = parent.class(&class).ok_or(ContractError::Scope)?;
        let record = parent.tir_class_v1(&class).ok_or(ContractError::Scope)?;
        let bound = palw_tir_bound_container_v1(parent, class).ok_or(ContractError::Scope)?;
        let point = parent.last_point().ok_or(ContractError::Scope)?;
        if registered.artifact_root != header.artifact_root
            || record.facts.class_id != class
            || record.tokenizer_id != header.job_context.tokenizer_id
            || class != header.job_context.shape_profile_id
            || bound.bound_daa > point.daa_score
            || bound.inner_leaf_count == 0
        {
            return Err(ContractError::Scope);
        }
        descriptor.check_native_pins_v1(HfArtifactNativePinsV1 {
            class: Digest(class.as_bytes()),
            program: Digest(record.graph_ir_root.as_bytes()),
            tokenizer: Digest(record.tokenizer_id.as_bytes()),
            inventory: Digest(registered.artifact_root.as_bytes()),
            container: Digest(bound.container_root.as_bytes()),
            binding_manifest: Digest(bound.manifest_digest.as_bytes()),
        })?;
        let entry = descriptor.manifest_entry_v1(rules)?;
        Ok(Self { claim: fp_claim_id_v3(fp), source: source.source_id_v1(), descriptor, entry })
    }
    pub fn descriptor_v1(&self) -> &HfArtifactDescriptorV1 {
        &self.descriptor
    }
    pub fn entry_v1(&self) -> ManifestEntryV1 {
        self.entry
    }
    /// Local backend/class correspondence. This does not establish that the
    /// backend came from HF; conversion/source-file checking is off-chain.
    pub fn check_context_v1(
        &self,
        fp: &PalwFreePromptCommitmentV3,
        source: &PalwTirPublicSourceV1,
        class: &PalwTirClassV1,
    ) -> Result<(), ContractError> {
        let pins = self.descriptor.native_pins_v1();
        if self.claim != fp_claim_id_v3(fp)
            || self.source != source.source_id_v1()
            || pins.class != Digest(fp.job.class_id.as_bytes())
            || pins.inventory != Digest(source.header_v1().artifact_root.as_bytes())
            || pins.class != Digest(class.class_id(&source.header_v1().artifact_root).as_bytes())
            || pins.program != Digest(class.graph_ir_root().as_bytes())
            || pins.tokenizer != Digest(class.tokenizer_id.as_bytes())
        {
            return Err(ContractError::Scope);
        }
        Ok(())
    }
    /// Attach this model's bounded descriptor during unsigned manifest assembly.
    /// It may be reused for the same model across claims. The companion owner
    /// must still validate the complete claim; this is not claim admission.
    /// The signed manifest root commits the complete descriptor.
    pub fn set_manifest_entry_v1(&self, original: &OriginalCommit, manifest: &mut ManifestV1) -> Result<(), ContractError> {
        let pins = self.descriptor.native_pins_v1();
        if original.registered_class != pins.class
            || original.artifact != pins.inventory
            || original.program != pins.program
            || original.tokenizer != pins.tokenizer
        {
            return Err(ContractError::Scope);
        }
        manifest.entries[MaterialScope::Artifact as usize] = self.entry;
        Ok(())
    }
}

#[cfg(test)]
#[path = "tfp1_hf_artifact_tests.rs"]
mod tests;
