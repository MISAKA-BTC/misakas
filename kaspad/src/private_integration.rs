//! Construction boundary for the unpublished integration probe only.
//!
//! This has no Args/CLI activation path. Production presets and their startup
//! refusals remain unchanged; the helper runs an explicitly supplied private
//! Config over loopback services and a separately owned, fingerprinted database.

use crate::{
    args::Args,
    daemon::{Runtime, create_core_with_runtime_inner, validate_args},
};
use kaspa_consensus_core::config::private_population::derive_private_population_v1;
use kaspa_consensus_core::config::{
    Config,
    drill::{
        PalwDrillKeyringV1, palw_drill_network_v1, palw_t12_drill_genesis_block_v1, palw_t12_drill_genesis_utxos_v1,
        palw_t12_drill_main_key_v1, palw_t12_public_genesis_txid_v1,
    },
};
use kaspa_core::core::Core;
use kaspa_rpc_service::service::RpcCoreService;
use kaspa_utils::networking::ContextualNetAddress;
use kaspa_wrpc_server::service::WrpcEncoding;
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::Path,
    sync::Arc,
};

pub const PRIVATE_PROBE_MARKER_V1: &str = "rfc12-private-probe-v1.json";

/// Test-policy identity, not a release result or economic/security approval.
pub fn private_probe_manifest(config: &Config) -> serde_json::Value {
    let genesis_money = config.palw_drill_genesis_salt.as_ref().map(|salt| {
        let key = palw_t12_drill_main_key_v1(salt);
        let script = kaspa_consensus_core::mldsa87_primitives::p2pkh_mldsa87_spk(&key.payload);
        let initial_utxos = match config.private_population {
            Some(profile) => derive_private_population_v1(salt, profile).expect("validated private population").utxos,
            None => palw_t12_drill_genesis_utxos_v1(salt),
        };
        let mut utxos: Vec<_> = initial_utxos
            .into_iter()
            .filter(|(_, entry)| entry.script_public_key == script)
            .map(|(outpoint, entry)| {
                serde_json::json!({
                    "transactionId": outpoint.transaction_id.to_string(), "index": outpoint.index,
                    "amount": entry.amount.to_string(), "blockDaaScore": entry.block_daa_score,
                    "isCoinbase": entry.is_coinbase,
                })
            })
            .collect();
        utxos.sort_by_key(|row| (row["transactionId"].as_str().unwrap().to_owned(), row["index"].as_u64().unwrap()));
        serde_json::json!({"address": key.address(kaspa_addresses::Prefix::Testnet).to_string(), "utxos": utxos})
    });
    let retirement = config.params.palw_dns_retirement.map(|r| {
        serde_json::json!({
            "activationDaa": r.activation.daa_score(),
            "policyId": r.settlement.id().to_string(),
            "depth": r.settlement.settled_anchor_depth,
            "uniqueWork": r.settlement.unique_mature_work.to_string(),
            "maxOperatorPermille": r.settlement.max_operator_permille,
            "maxClassPermille": r.settlement.max_class_permille,
            "legacyEvidenceHorizonDaa": r.legacy_evidence_horizon_daa,
        })
    });
    serde_json::json!({
        "version": 1,
        "kind": "rfc12-private-integration-probe",
        "network": config.params.net.to_string(),
        "genesis": config.params.genesis.hash.to_string(),
        "consensusParamsId": config.params.consensus_params_id().to_string(),
        "saltId": config.palw_drill_genesis_salt.as_ref().map(|salt| salt.id()),
        "genesisMoneyProbe": genesis_money,
        "population": config.private_population.map(|profile| serde_json::json!({
            "version": 1, "totalBonds": profile.total_bonds(),
            "premineSupplySompi": kaspa_consensus_core::config::premine::MISAKA_PREMINE_CAP_SOMPI.to_string(),
            "capabilitiesAdded": false, "participantIndependenceProven": false,
        })),
        "retirement": retirement,
        "skipProofOfWork": config.params.skip_proof_of_work,
        "releaseApproval": false,
        "securitySoundnessMeasured": false,
    })
}

fn validate_private_probe(args: &Args, config: &Config) -> Result<(), String> {
    validate_args(args).map_err(|error| error.to_string())?;
    if args.network() != palw_drill_network_v1() || config.params.net != args.network() {
        return Err("the private probe requires its testnet-12 identity and matching Args".into());
    }
    let salt = config.palw_drill_genesis_salt.as_ref().ok_or("the private probe needs an explicit salted genesis")?;
    let expected_genesis = match config.private_population {
        Some(profile) => {
            let expected = derive_private_population_v1(salt, profile)?;
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(actual) = &config.params.palw_consensus_mode else {
                return Err("the private population needs its native registry".into());
            };
            let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(fixed) = &expected.params.palw_consensus_mode
            else {
                unreachable!()
            };
            if actual.genesis_objects != fixed.genesis_objects {
                return Err("private population registrations differ from the exact salt/size/funded factory".into());
            }
            expected.params.genesis.hash
        }
        None => palw_t12_drill_genesis_block_v1(salt).hash,
    };
    if config.params.genesis.hash != expected_genesis {
        return Err("the private Config does not carry the salt/profile's exact genesis".into());
    }
    #[cfg(feature = "devnet-prealloc")]
    if !config.initial_utxo_set.is_empty() {
        return Err("private population refuses arbitrary initial monetary rows".into());
    }
    if config.params.skip_proof_of_work {
        return Err("the process/P2P probe must validate work; skip_proof_of_work is refused".into());
    }
    // Normal Args validation sees no public drill flag in this construction
    // path. Reuse its actual key checks against the Config's salt anyway.
    crate::palw_drill::palw_drill_keys_are_drill_only_v1(args, &PalwDrillKeyringV1::new(*salt)).map_err(|error| error.to_string())?;
    for outpoint in [&args.palw_producer_bond, &args.palw_fee_outpoint, &args.stake_bond].into_iter().flatten() {
        let outpoint = crate::palw_producer::parse_outpoint(outpoint).map_err(|error| error.to_string())?;
        if palw_t12_public_genesis_txid_v1(&outpoint.transaction_id) {
            return Err("a private probe must never name a public genesis monetary outpoint".into());
        }
    }
    config.params.validate_palw_v1().map_err(|error| error.to_string())?;
    config.params.validate_palw_v2().map_err(|error| error.to_string())?;
    let mut applied = config.clone();
    args.apply_to_config(&mut applied);
    if applied.params.consensus_params_id() != config.params.consensus_params_id()
        || applied.palw_drill_genesis_salt != config.palw_drill_genesis_salt
    {
        return Err("private probe Args may not replace its pinned consensus Config".into());
    }
    if !args.disable_dns_seeding
        || args.outbound_target != 0
        || !args.disable_upnp
        || !args.connect_peers.is_empty()
        || !args.add_peers.is_empty()
        || args.externalip.is_some()
    {
        return Err("the private probe requires controlled loopback peers, no discovery or UPnP".into());
    }
    if args.palw_drill_genesis_salt.is_some() || args.override_params_file.is_some() {
        return Err("private Config injection cannot be mixed with Args parameter overrides".into());
    }
    let loopback = |address: ContextualNetAddress| address.has_port() && address.normalize(0).ip.is_loopback();
    if !args.listen.is_some_and(loopback) || !args.rpclisten.is_some_and(loopback) {
        return Err("the private probe requires explicit loopback P2P and gRPC ports".into());
    }
    for (address, encoding) in [(&args.rpclisten_borsh, WrpcEncoding::Borsh), (&args.rpclisten_json, WrpcEncoding::SerdeJson)] {
        if let Some(address) = address
            && !loopback(address.to_address(&config.params.net.network_type, &encoding))
        {
            return Err("every private probe wRPC endpoint must be loopback".into());
        }
    }
    if args.evm_rpc_listen.is_some_and(|address| !loopback(address)) {
        return Err("the private probe EVM endpoint must be loopback".into());
    }
    if !args.appdir.as_ref().is_some_and(|directory| Path::new(directory).is_absolute()) {
        return Err("the private probe needs an explicit absolute owned app directory".into());
    }
    Ok(())
}

/// Refuse to reuse arbitrary existing data or another private policy. A cold
/// restart is allowed only with the exact previously pinned private identity.
fn pin_owned_probe_directory(args: &Args, config: &Config) -> Result<(), String> {
    let directory = Path::new(args.appdir.as_ref().ok_or("missing private app directory")?);
    fs::create_dir_all(directory).map_err(|error| error.to_string())?;
    let marker = directory.join(PRIVATE_PROBE_MARKER_V1);
    let expected = private_probe_manifest(config);
    if marker.exists() {
        let actual: serde_json::Value =
            serde_json::from_slice(&fs::read(&marker).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
        if actual != expected {
            return Err("private probe DB fingerprint/policy differs from its pinned identity".into());
        }
        return Ok(());
    }
    if fs::read_dir(directory).map_err(|error| error.to_string())?.next().is_some() {
        return Err("a private probe may initialize only its empty owned directory".into());
    }
    let mut file = OpenOptions::new().write(true).create_new(true).open(marker).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(&expected).map_err(|error| error.to_string())?).map_err(|error| error.to_string())?;
    file.sync_all().map_err(|error| error.to_string())?;
    Ok(())
}

pub fn create_private_integration_core_with_runtime(
    runtime: &Runtime,
    args: &Args,
    fd_total_budget: i32,
    config: Config,
) -> Result<(Arc<Core>, Arc<RpcCoreService>), String> {
    validate_private_probe(args, &config)?;
    pin_owned_probe_directory(args, &config)?;
    kaspa_core::warn!("RFC12 PRIVATE INTEGRATION PROBE: injected test policy; release/soundness evidence remains unapproved");
    Ok(create_core_with_runtime_inner(runtime, args, fd_total_budget, Some(config)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kaspa_consensus_core::config::drill::{PalwDrillSaltV1, palw_chain_params_v1};

    #[test]
    fn private_constructor_refuses_public_identity_skipped_work_and_uncontrolled_peers() {
        let salt = PalwDrillSaltV1::from_bytes([0x12; 32]).unwrap();
        let mut config = Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-test-probe").display().to_string()),
            ..Args::default()
        };
        assert!(validate_private_probe(&args, &config).is_ok());
        config.params.skip_proof_of_work = true;
        assert!(validate_private_probe(&args, &config).unwrap_err().contains("skip_proof_of_work"));
        config.params.skip_proof_of_work = false;
        let mut uncontrolled = args.clone();
        uncontrolled.outbound_target = 1;
        assert!(validate_private_probe(&uncontrolled, &config).unwrap_err().contains("controlled"));
        let mut public_payout = args.clone();
        public_payout.palw_heartbeat_miner_address = Some(
            kaspa_addresses::Address::new(kaspa_addresses::Prefix::Testnet, kaspa_addresses::Version::PubKeyHashMlDsa87, &[0; 64])
                .to_string(),
        );
        assert!(validate_private_probe(&public_payout, &config).unwrap_err().contains("not an address of drill"));
        let mut public = config.clone();
        public.palw_drill_genesis_salt = None;
        assert!(validate_private_probe(&args, &public).unwrap_err().contains("salted"));
    }

    #[test]
    fn private_population_constructor_refuses_replaced_factory_rows_or_size() {
        use kaspa_consensus_core::config::private_population::PrivatePopulationV1;
        let salt = PalwDrillSaltV1::from_bytes([0x14; 32]).unwrap();
        let profile = PrivatePopulationV1::new(17).unwrap();
        let derived = derive_private_population_v1(&salt, profile).unwrap();
        let mut config = Config::new(derived.params);
        config.private_population = Some(profile);
        config.palw_drill_genesis_salt = Some(salt);
        let args = Args {
            testnet: true,
            testnet_suffix: 12,
            disable_dns_seeding: true,
            outbound_target: 0,
            disable_upnp: true,
            listen: Some("127.0.0.1:53001".parse().unwrap()),
            rpclisten: Some("127.0.0.1:53002".parse().unwrap()),
            appdir: Some(std::env::temp_dir().join("unopened-rfc12-population-probe").display().to_string()),
            ..Args::default()
        };
        assert!(validate_private_probe(&args, &config).is_ok());
        let mut wrong_size = config.clone();
        wrong_size.private_population = Some(PrivatePopulationV1::new(18).unwrap());
        assert!(validate_private_probe(&args, &wrong_size).unwrap_err().contains("exact salt/size/funded factory"));
        let mut changed = config.clone();
        let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &mut changed.params.palw_consensus_mode
        else {
            unreachable!()
        };
        let kaspa_consensus_core::palw_state_v2::PalwConsensusObjectV2::BondRegistered { collateral, .. } =
            bundle.genesis_objects.last_mut().unwrap()
        else {
            unreachable!()
        };
        *collateral += 1;
        assert!(validate_private_probe(&args, &changed).unwrap_err().contains("exact salt/size/funded factory"));
        let manifest = private_probe_manifest(&config);
        assert_eq!(manifest["population"]["totalBonds"], 17);
        assert_ne!(
            manifest["genesis"],
            private_probe_manifest(&Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap()))["genesis"]
        );
    }

    #[test]
    fn private_probe_db_restarts_only_under_its_exact_fingerprint() {
        let salt = PalwDrillSaltV1::from_bytes([0x13; 32]).unwrap();
        let mut config = Config::new(palw_chain_params_v1(palw_drill_network_v1(), Some(&salt)).unwrap());
        config.palw_drill_genesis_salt = Some(salt);
        let directory = kaspa_database::utils::get_kaspa_tempdir();
        let args = Args { appdir: Some(directory.path().display().to_string()), ..Args::default() };
        pin_owned_probe_directory(&args, &config).unwrap();
        fs::write(directory.path().join("owned-db-test-file"), b"retained").unwrap();
        pin_owned_probe_directory(&args, &config).unwrap();
        let mut different = config.clone();
        different.params.skip_proof_of_work = true;
        assert!(pin_owned_probe_directory(&args, &different).unwrap_err().contains("fingerprint"));
        assert_eq!(fs::read(directory.path().join("owned-db-test-file")).unwrap(), b"retained");
        let other = kaspa_database::utils::get_kaspa_tempdir();
        fs::write(other.path().join("existing-data"), b"keep").unwrap();
        let args = Args { appdir: Some(other.path().display().to_string()), ..Args::default() };
        assert!(pin_owned_probe_directory(&args, &config).unwrap_err().contains("empty owned"));
        assert_eq!(fs::read(other.path().join("existing-data")).unwrap(), b"keep");
        assert!(!other.path().join(PRIVATE_PROBE_MARKER_V1).exists());
    }
}
