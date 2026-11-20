//! Explicit salted protected-claim startup. The declared profile owns its
//! service book; it does not arm the legacy family/income/global settlement
//! routes. Every ordinary value, dependency and readiness validator still runs.
#[cfg(feature = "private-integration-clock")]
use kaspa_consensus_core::config::{drill, params};
use kaspa_consensus_core::{
    config::{
        drill::{PalwDrillSaltV1, palw_chain_params_v1, palw_drill_network_v1},
        params::{ForkActivation, ManifestIngressFenceV3, Params, ProtectedPlanningFenceV3, TokenizerRegistrationFenceV1},
    },
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

const MAX_PROFILE_BYTES: usize = 32 * 1024;
const MAX_TOKENIZER_POLICY_BYTES: usize = 4096;

#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct InputV3 {
    version: u16,
    planning: ProtectedPlanningFenceV3,
    manifest: ManifestIngressFenceV3,
    /// Exact Borsh TokenizerRegistrationPolicyV1, read once before construction.
    tokenizer_policy_path: PathBuf,
}

/// Parsed values, never paths, are reused by startup and identity checks.
/// All three transports activate together; this profile has no tokenizer Open
/// cutoff. Neither capacity nor prices are filled in by this constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrivateProtectedClaimProfileV3 {
    planning: ProtectedPlanningFenceV3,
    manifest: ManifestIngressFenceV3,
    tokenizer: TokenizerRegistrationPolicyV1,
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    if !path.is_absolute() {
        return Err("protected private policy paths must be absolute".into());
    }
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("protected private policy must be a regular file".into());
    }
    let mut bytes = Vec::with_capacity(limit + 1);
    file.take((limit + 1) as u64).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("protected private policy exceeds its byte bound".into());
    }
    Ok(bytes)
}

impl PrivateProtectedClaimProfileV3 {
    /// Shared by node startup and offline key export. Exporting keys must not
    /// silently describe the ordinary salted preset instead of this profile.
    pub(super) fn check_config_v3(&self, config: &kaspa_consensus_core::config::Config) -> Result<(), String> {
        let salt = config.palw_drill_genesis_salt.as_ref().ok_or("protected private profile requires an explicit salt")?;
        let expected = self.params_v3(salt)?;
        if config.private_population.is_some()
            || config.params.skip_proof_of_work
            || config.params.genesis.hash != expected.genesis.hash
            || config.params.consensus_params_id() != expected.consensus_params_id()
            || config.params.consensus_schedule_id() != expected.consensus_schedule_id()
        {
            return Err("protected private Config differs from its exact salted factory policy".into());
        }
        #[cfg(feature = "devnet-prealloc")]
        if !config.initial_utxo_set.is_empty() {
            return Err("protected private profile refuses arbitrary initial monetary rows".into());
        }
        config.params.validate_palw_v1().map_err(|error| error.to_string())?;
        config.params.validate_palw_v2().map_err(|error| error.to_string())
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        let input: InputV3 = serde_json::from_slice(&read_bounded(path, MAX_PROFILE_BYTES)?).map_err(|e| e.to_string())?;
        if input.version != 3 {
            return Err("protected private profile requires version 3".into());
        }
        let tokenizer = borsh::from_slice(&read_bounded(&input.tokenizer_policy_path, MAX_TOKENIZER_POLICY_BYTES)?)
            .map_err(|e| format!("protected tokenizer policy: {e}"))?;
        let profile = Self { planning: input.planning, manifest: input.manifest, tokenizer };
        profile.validate_shape()?;
        Ok(profile)
    }

    fn validate_shape(&self) -> Result<(), String> {
        if self.planning.activation == ForkActivation::never() || self.manifest.activation != self.planning.activation {
            return Err("protected private transports require one equal finite activation".into());
        }
        self.planning.declaration.validate_v3()?;
        self.manifest.validate_v3().map_err(|e| e.to_string())
    }

    pub fn params_v3(&self, salt: &PalwDrillSaltV1) -> Result<Params, String> {
        self.validate_shape()?;
        let mut params = palw_chain_params_v1(palw_drill_network_v1(), Some(salt))?;
        let at = self.planning.activation;
        if salt.is_protected_lifecycle_v1() {
            #[cfg(not(feature = "private-integration-clock"))]
            return Err("the protected lifecycle salt requires private-integration-clock".into());
            #[cfg(feature = "private-integration-clock")]
            {
                if at.daa_score() != 200 {
                    return Err("the protected lifecycle salt requires registration activation at DAA 200".into());
                }
                arm_protected_lifecycle_prerequisites(&mut params)?;
            }
        }
        // TKR1 validates its rendering declaration against the registered class
        // semantics. The public preset leaves that intake dormant, so the
        // explicitly selected private profile must activate it with TKR1.
        params.palw_class_semantics_v1 = Some(at);
        params.palw_batch_court_v1 = Some(at);
        params.palw_tokenizer_registration_v1 =
            Some(TokenizerRegistrationFenceV1 { activation: at, open_cutoff: None, policy: self.tokenizer });
        params.palw_model_panel_one_retention_v1 = Some(at);
        params.sync_palw_g14_gates_v1();
        params.palw_manifest_ingress_v3 = Some(self.manifest);
        params.palw_protected_planning_v3 = Some(self.planning);
        // No readiness exception, altered genesis funds, capability subset or
        // shifted historical activation is part of this private profile.
        params.validate_palw_v1().map_err(|e| e.to_string())?;
        params.validate_palw_v2().map_err(|e| e.to_string())?;
        Ok(params)
    }

    pub(super) fn of_params(params: &Params) -> Result<Option<Self>, String> {
        let Some(planning) = params.palw_protected_planning_v3 else { return Ok(None) };
        let manifest = params.palw_manifest_ingress_v3.ok_or("protected private profile has no Manifest policy")?;
        let tokenizer = params.palw_tokenizer_registration_v1.ok_or("protected private profile has no tokenizer policy")?;
        if tokenizer.activation != planning.activation || tokenizer.open_cutoff.is_some() {
            return Err("protected private tokenizer differs from its factory activation/cutoff".into());
        }
        let profile = Self { planning, manifest, tokenizer: tokenizer.policy };
        profile.validate_shape()?;
        Ok(Some(profile))
    }
}

#[cfg(feature = "private-integration-clock")]
fn arm_protected_lifecycle_prerequisites(params: &mut Params) -> Result<(), String> {
    if !params.has_protected_lifecycle_drill_genesis_v1() {
        return Err("the early protected lifecycle requires an isolated genesis".into());
    }
    drill::palw_drill_post_launch_fences_at_v1(params, 6)?;
    drill::palw_drill_post_launch_fences_v2_at_v1(params, 10)?;
    drill::palw_drill_post_launch_fences_v3_at_v1(params, 14)?;
    drill::palw_drill_tir_fence_at_v1(params, 20)?;
    drill::palw_drill_tir_fence2_at_v1(params, 24)?;
    drill::palw_drill_int11_at_v1(params, 28)?;
    drill::palw_drill_int13_at_v1(params, 60)?;
    params::palw_t12_arm_after_int13_at_v1(params, params::PALW_T12_G14_17000_FENCES_V1, Some(80));
    params.palw_model_panel_one_v1 = Some(ForkActivation::new(100));
    params.configure_protected_lifecycle_receipt_maturity_v1()?;
    Ok(())
}

mod bootstrap;
pub use bootstrap::export_private_protected_bootstrap_v3;

#[cfg(test)]
mod tests;
