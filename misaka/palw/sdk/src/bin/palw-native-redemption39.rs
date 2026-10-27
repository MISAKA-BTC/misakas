//! Owning39 payout/redemption audit from own-node points (see
//! native_redemption_v39). The spec JSON names point folders and proofs; it is
//! pinned by SHA-256 and carries no authority. The report never says PASS.
use kaspa_consensus_core::{Hash64, config::params::Params, network::NetworkId};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, io::Write, path::PathBuf};
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("redemption39 distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let snapshot = PathBuf::from(get("--own-params-snapshot39")?);
    let pin = get("--own-params-snapshot-root")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--own-params-sha256")?)?;
    let root = PathBuf::from(get("--evidence-root")?).canonicalize().map_err(|e| e.to_string())?;
    let spec_path = PathBuf::from(get("--spec")?);
    let spec_sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--spec-sha256")?)?;
    let out = PathBuf::from(get("--out")?);
    drop(get);
    let archive_dir = a.remove("--archive-dir");
    if !a.is_empty() || out.exists() {
        return Err("redemption39 unknown option or existing output".into());
    }
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = misaka_palw_sdk::native_params_v39::load_own_native_parameters_v39(&base, &snapshot, pin.as_bytes(), sha)?;
    let params = own.parameters();
    if params.genesis.hash.to_string() != genesis {
        return Err("redemption39 own genesis differs".into());
    }
    let raw = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(&spec_path, 1 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != spec_sha {
        return Err("redemption39 spec pin differs".into());
    }
    let spec: misaka_palw_sdk::native_redemption_v39::RedemptionSpecV39 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let archive =
        archive_dir.map(|d| misaka_palw_sdk::native_archive_v39::NativeArchiveDirV39::open(std::path::Path::new(&d))).transpose()?;
    let archive_ref =
        archive.as_ref().map(|a| a as &dyn kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39);
    let report = misaka_palw_sdk::native_redemption_v39::audit_native_redemption_v39(&root, &spec, params, archive_ref)?;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("redemption39 refused: {e}");
        std::process::exit(1);
    }
}
