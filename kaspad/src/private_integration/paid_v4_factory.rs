//! A new private construction profile for prunable release recovery. The paid-v3 factory and its manifest bytes remain
//! unchanged. All numbers below are finite mechanics scopes, pending actual measurement; neither a nonzero report marker nor
//! successful private admission is approval.
//!
//! PRINCIPLES §8 (2026-10-15): this profile no longer arms the compact V4 fence (retired: no compact proof record is
//! generated, bound, accepted or credited). The profile's identity is PAV3 + material release recovery V2.
use kaspa_consensus_core::{
    config::{
        drill::PalwDrillSaltV1,
        params::{ForkActivation, Params},
    },
    palw_material_release_recovery_v2::{PalwMaterialReleaseRecoveryFenceV2, PalwMaterialReleaseRecoveryPolicyV2},
};
use kaspa_hashes::Hash64;

fn private_recovery_scope_v4() -> PalwMaterialReleaseRecoveryFenceV2 {
    PalwMaterialReleaseRecoveryFenceV2 {
        activation: ForkActivation::new(0),
        policy: PalwMaterialReleaseRecoveryPolicyV2 {
            version: 2,
            max_reserved_contracts: 1024,
            max_generations_per_contract: 8,
            max_provider_streams: 4096,
            max_reserved_history_cells: 1024 * 8 * 8,
            measurement_root: Hash64::from_bytes(misaka_palw_kernel::hash::object_id(
                b"MISAKA/PRIVATE/UNAPPROVED-RELEASE-RECOVERY-SCOPE/V4",
                &(),
            )),
        },
    }
}

pub fn private_paid_claim_params_v4(salt: &PalwDrillSaltV1) -> Result<Params, String> {
    let mut params = super::paid_claim_factory::private_paid_claim_params_v3(salt)?;
    params.palw_claim_compact_v4 = None;
    params.palw_material_release_recovery_v2 = Some(private_recovery_scope_v4());
    params.sync_palw_bond_budget_v1();
    params.sync_palw_claim_compact_v4();
    validate_private_paid_claim_relations_v4(&params)?;
    Ok(params)
}

pub(super) fn validate_private_paid_claim_relations_v4(params: &Params) -> Result<(), String> {
    super::paid_claim_factory::validate_private_paid_claim_relations_v3(params)?;
    if params.palw_claim_compact_v4.is_some() {
        return Err(kaspa_consensus_core::palw_claim_compact_v1::PALW_COMPACT_CONTRACT_RETIRED_REASON_V1.into());
    }
    let recovery = params.palw_material_release_recovery_v2.as_ref().ok_or("private v4 release recovery absent")?;
    recovery.policy.validate().map_err(str::to_owned)?;
    if *recovery != private_recovery_scope_v4() {
        return Err("private v4 capabilities differ from their explicit unapproved mechanics scopes".into());
    }
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("private v4 requires its V2 state mirrors".into());
    };
    if bundle.state.claim_compact_v4().is_some()
        || bundle.state.material_release_recovery_from_v2() != Some(0)
        || bundle.state.material_release_recovery_at_v2(0) != Some(&recovery.policy)
    {
        return Err("private v4 capability mirror differs from its actual fence/policy".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paid_v4_has_a_separate_identity_and_never_upgrades_the_v3_factory() {
        let salt = PalwDrillSaltV1::from_hex(&"34".repeat(32)).unwrap();
        let old = super::super::private_paid_claim_params_v3(&salt).unwrap();
        let new = private_paid_claim_params_v4(&salt).unwrap();
        assert!(old.palw_claim_compact_v4.is_none());
        assert!(old.palw_material_release_recovery_v2.is_none());
        // PRINCIPLES §8: the retired compact V4 fence is never armed, and configuring it is refused.
        assert!(new.palw_claim_compact_v4.is_none());
        assert!(new.validate_palw_claim_compact_v4().is_ok());
        assert_ne!(old.consensus_params_id(), new.consensus_params_id());
        assert_eq!(old.genesis.hash, new.genesis.hash);
        assert!(new.validate_palw_material_release_recovery_v2().is_err());
        let mut changed = new;
        changed.palw_material_release_recovery_v2.as_mut().unwrap().policy.max_provider_streams -= 1;
        assert!(validate_private_paid_claim_relations_v4(&changed).is_err());
    }
}
