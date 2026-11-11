//! Literal manifest and model-reference authentication. These bounded bodies
//! describe identities; they neither contain model weights nor certify that
//! a remote provider can serve them.

use super::PublicOriginalAdmissionV1;
use crate::palw_funded_original_coordinator_v1::PalwFundedOriginalPairingV1;
use misaka_palw_positive_contract::{
    ContractError,
    public_one_honest::{HfArtifactDescriptorV1, ManifestScopeRules, MaterialScope, OriginalCommit},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum MaterialPairingErrorV2 {
    Contract(ContractError),
    LocalResource(crate::palw_native_resource_v1::NativeCoverageV1),
    InitialSupply(super::initial_supply::InitialSupplyErrorV1),
}

impl From<super::initial_supply::InitialSupplyErrorV1> for MaterialPairingErrorV2 {
    fn from(error: super::initial_supply::InitialSupplyErrorV1) -> Self {
        match error {
            super::initial_supply::InitialSupplyErrorV1::LocalResource(why) => Self::LocalResource(why),
            super::initial_supply::InitialSupplyErrorV1::Contract(why) => Self::Contract(why),
            other => Self::InitialSupply(other),
        }
    }
}

impl From<ContractError> for MaterialPairingErrorV2 {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

pub(super) fn check_pairing_material_v2(
    admission: &PublicOriginalAdmissionV1<'_>,
    pairing: &PalwFundedOriginalPairingV1,
    original: &OriginalCommit,
) -> Result<(), ContractError> {
    check_literal_material_v2(
        original,
        pairing.public_prompt_bytes_v3().ok_or(ContractError::Scope)?,
        pairing.public_manifest_bytes_v2().ok_or(ContractError::Scope)?,
        pairing.public_hf_descriptor_bytes_v2().ok_or(ContractError::Scope)?,
        admission.max_manifest_bytes,
        admission.manifest_scope_limits,
        admission.manifest_total_limit,
    )
}

/// The owner verifies the companion signature and native identity separately.
/// Every body here is taken from that companion's actual immutable carrier.
pub(crate) fn check_literal_material_v2(
    original: &OriginalCommit,
    prompt: &[u8],
    manifest: &[u8],
    descriptor: &[u8],
    max_manifest_bytes: u32,
    scope_limits: &[ManifestScopeRules; MaterialScope::COUNT],
    total_limit: u64,
) -> Result<(), ContractError> {
    let manifest = original.authenticate_manifest(manifest, max_manifest_bytes)?.decode(original, scope_limits, total_limit)?;
    manifest.open_prompt_text(prompt)?;
    let descriptor = HfArtifactDescriptorV1::decode_v1(descriptor)?;
    let pins = descriptor.native_pins_v1();
    if pins.class != original.registered_class
        || pins.program != original.program
        || pins.tokenizer != original.tokenizer
        || pins.inventory != original.artifact
        || descriptor.manifest_entry_v1(scope_limits[MaterialScope::Artifact as usize])? != *manifest.entry(MaterialScope::Artifact)
    {
        return Err(ContractError::Scope);
    }
    Ok(())
}
