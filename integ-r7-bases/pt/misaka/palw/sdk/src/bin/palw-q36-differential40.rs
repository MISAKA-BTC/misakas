//! PALWQ361 original runtime vs Source39 mirror differential measurement.
//! Writes a measurement record; never PASS. See native_q36_differential_v40.
use sha2::{Digest as _, Sha256};
use std::{collections::BTreeMap, io::Write, path::PathBuf};
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("q36 differential distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let artifact = PathBuf::from(get("--artifact")?);
    let artifact_sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--artifact-sha256")?)?;
    let compiled = PathBuf::from(get("--compiled")?);
    let tokens_path = PathBuf::from(get("--tokens-json")?);
    let tokens_sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--tokens-sha256")?)?;
    let workspace = get("--workspace-bytes")?.parse::<u64>().map_err(|e| e.to_string())?;
    let reads = get("--max-read-elements")?.parse::<usize>().map_err(|e| e.to_string())?;
    let out = PathBuf::from(get("--out")?);
    drop(get);
    if !a.is_empty() || out.exists() {
        return Err("q36 differential unknown option or existing output".into());
    }
    let raw = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(&tokens_path, 64 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != tokens_sha {
        return Err("q36 differential original token file pin differs".into());
    }
    let tokens: Vec<u32> = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    // The artifact is pinned whole before either side reads it.
    let mut hasher = Sha256::new();
    let mut f = std::fs::File::open(&artifact).map_err(|e| e.to_string())?;
    std::io::copy(&mut f, &mut hasher).map_err(|e| e.to_string())?;
    if <[u8; 32]>::from(hasher.finalize()) != artifact_sha {
        return Err("q36 differential original .palwq36 artifact pin differs".into());
    }
    let report = misaka_palw_sdk::native_q36_differential_v40::run_q36_differential_v40(&artifact, &compiled, &tokens, workspace, reads)?;
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("q36 differential refused: {e}");
        std::process::exit(1);
    }
}
