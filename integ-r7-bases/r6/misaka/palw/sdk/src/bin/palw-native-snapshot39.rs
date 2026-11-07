//! Explicit local owning39 wrapper generator. Never synthesizes calibration.
use kaspa_consensus_core::{
    Hash64, config::params::Params, network::NetworkId, palw_native_calibration_v39::NativeCalibrationSnapshotV39,
};
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, io::Write, path::PathBuf};
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let base: Params = get("--network")?.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let path = PathBuf::from(get("--original-snapshot34")?);
    let root = get("--original-root")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--original-sha256")?)?;
    let maximum_horizon = get("--maximum-horizon-daa")?.parse::<u64>().map_err(|e| e.to_string())?;
    let catalog = get("--catalog-cap")?.parse::<u8>().map_err(|e| e.to_string())?;
    let output = PathBuf::from(get("--out")?);
    drop(get);
    if !a.is_empty() || output.exists() {
        return Err("native39 exact options and fresh output required".into());
    }
    let raw = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(
        &path,
        kaspa_consensus_core::palw_native_calibration_v34::NATIVE_CALIBRATION_SNAPSHOT_MAX_BYTES_V34,
    )?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != sha {
        return Err("native39 original34 wholefile SHA256 differs".into());
    }
    let snapshot = NativeCalibrationSnapshotV39::wrap(&base, raw, root.as_bytes(), maximum_horizon, catalog)?;
    let bytes = borsh::to_vec(&snapshot).map_err(|e| e.to_string())?;
    let params = snapshot.apply(&base, snapshot.root())?;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(output).map_err(|e| e.to_string())?;
    f.write_all(&bytes).and_then(|_| f.sync_all()).map_err(|e| e.to_string())?;
    let hex = misaka_palw_sdk::runtime_pack::commit::hex;
    println!(
        "{}",
        serde_json::json!({"schema":"misaka.palw.local-native-calibration-snapshot.v39","root":hex(&snapshot.root()),"sha256":hex(&Sha256::digest(&bytes)),"original_root":root.to_string(),"genesis":params.genesis.hash.to_string(),"consensus_params_id":params.consensus_params_id().to_string(),"consensus_schedule_id":params.consensus_schedule_id().to_string(),"rfc02_qualified":false,"local_qualified_validation_network":params.native_final_is_local_qualified_v34()})
    );
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("native39 snapshot refused: {e}");
        std::process::exit(1);
    }
}
