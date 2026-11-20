//! Offline key export for the exact protected factory. No model fixture or
//! accepted state is generated; all registration and claim carriers remain live.
use super::PrivateProtectedClaimProfileV3;
use kaspa_consensus_core::config::{
    Config,
    drill::{
        PalwDrillKeyRoleV1, PalwDrillKeyV1, palw_t12_drill_cards_v1, palw_t12_drill_fee_float_outpoint_v1, palw_t12_drill_main_key_v1,
        palw_t12_drill_premine_outpoint_v1,
    },
};
use std::{fs, io::Write, path::Path};

fn write_new(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| format!("{}: {e}", path.display()))
}

/// Only a fresh absolute directory is accepted, including after a partial
/// failed export. The manifest is written last and never contains seed bytes.
pub fn export_private_protected_bootstrap_v3(config: &Config, output: &Path) -> Result<serde_json::Value, String> {
    if !output.is_absolute() {
        return Err("protected bootstrap export requires an absolute fresh directory".into());
    }
    let profile = PrivateProtectedClaimProfileV3::of_params(&config.params)?.ok_or("protected private policy absent")?;
    profile.check_config_v3(config)?;
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("protected private salt absent")?;
    let mut directory = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory.mode(0o700);
    }
    directory.create(output).map_err(|e| format!("fresh protected bootstrap directory {}: {e}", output.display()))?;
    let output = output.canonicalize().map_err(|e| e.to_string())?;
    let write_key = |key: &PalwDrillKeyV1| -> Result<serde_json::Value, String> {
        let name = format!("{}-{}.seed", key.role.name(), key.n);
        write_new(&output.join(&name), faster_hex::hex_string(&key.seed).as_bytes())?;
        Ok(serde_json::json!({
            "n": key.n, "address": key.address(kaspa_addresses::Prefix::Testnet).to_string(),
            "publicKey": faster_hex::hex_string(&key.pubkey), "seedFile": name,
        }))
    };
    let outpoint = |value: kaspa_consensus_core::tx::TransactionOutpoint| format!("{}:{}", value.transaction_id, value.index);
    let mut main = write_key(&palw_t12_drill_main_key_v1(salt))?;
    main["outpoint"] =
        outpoint(palw_t12_drill_premine_outpoint_v1(salt, kaspa_consensus_core::config::premine::MAIN_PREMINE_INDEX)).into();
    let mut seats = Vec::new();
    for card in palw_t12_drill_cards_v1(salt) {
        let mut bond = write_key(&card.bond)?;
        bond["bondOutpoint"] = outpoint(palw_t12_drill_premine_outpoint_v1(salt, card.premine_index)).into();
        bond["feeFloatOutpoint"] = outpoint(palw_t12_drill_fee_float_outpoint_v1(salt, card.bond.n)).into();
        seats.push(serde_json::json!({"bond": bond, "operator": write_key(&card.operator)?}));
    }
    let mut workers = serde_json::Map::new();
    for role in [PalwDrillKeyRoleV1::Heartbeat, PalwDrillKeyRoleV1::Payout, PalwDrillKeyRoleV1::Validator] {
        workers.insert(role.name().into(), write_key(&PalwDrillKeyV1::derive(salt, role, 0))?);
    }
    let tokenizer_path = output.join("tokenizer-policy.borsh");
    write_new(&tokenizer_path, &borsh::to_vec(&profile.tokenizer).map_err(|e| e.to_string())?)?;
    let policy_path = output.join("protected-policy.json");
    let policy = serde_json::json!({
        "version": 3, "planning": profile.planning, "manifest": profile.manifest,
        "tokenizer_policy_path": tokenizer_path,
    });
    write_new(&policy_path, &serde_json::to_vec_pretty(&policy).map_err(|e| e.to_string())?)?;
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        config.params.net.to_string().as_bytes(),
        Some(config.params.genesis.hash),
    );
    let manifest = serde_json::json!({
        "version": 3, "kind": "protected-private-bootstrap-keys",
        "profile": crate::private_integration::private_probe_manifest(config),
        "genesisTimestampMs": config.params.genesis.timestamp,
        "networkDomain": network_domain.to_string(),
        "consensusIdentityId": config.params.consensus_identity_id().to_string(),
        "consensusScheduleId": config.params.consensus_schedule_id().to_string(),
        "policyFile": policy_path, "main": main, "seats": seats, "workers": workers,
        "registrationSubmitted": false, "claimAdmitted": false, "releaseApproval": false,
    });
    write_new(&output.join("manifest.json"), &serde_json::to_vec_pretty(&manifest).map_err(|e| e.to_string())?)?;
    Ok(manifest)
}
