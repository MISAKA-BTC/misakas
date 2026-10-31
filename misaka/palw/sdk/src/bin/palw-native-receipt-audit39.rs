//! Offline owning39 two-window receipt audit (A6/A8 input) over a complete
//! own-node live39 transcript. Every point is re-authenticated from its own
//! anchored full aux stream; only one point's checked projection is alive at a
//! time. Pointer JSON carries no authority. The report never says PASS.
#![allow(warnings)]
use kaspa_consensus_core::{Hash64, config::params::Params, network::NetworkId};
use misaka_palw_sdk::native_receipt_windows_v39::{AnchoredReceiptPointV39, ReceiptAccumulatorV39, ReceiptWindowContractV39};
use sha2::{Digest as _, Sha256};
use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};
fn run() -> Result<(), String> {
    let mut a = BTreeMap::new();
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        if !k.starts_with("--") || a.insert(k, it.next().ok_or("flag value required")?).is_some() {
            return Err("receipt39 audit distinct exact flags required".into());
        }
    }
    let mut get = |k: &str| a.remove(k).ok_or_else(|| format!("{k} required"));
    let network = get("--network")?;
    let genesis = get("--genesis")?;
    let snapshot = PathBuf::from(get("--own-params-snapshot39")?);
    let pin = get("--own-params-snapshot-root")?.parse::<Hash64>().map_err(|e| e.to_string())?;
    let sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--own-params-sha256")?)?;
    let contract_path = PathBuf::from(get("--contract")?);
    let contract_sha = misaka_palw_sdk::native_params_v34::parse_sha256_hex_v34(&get("--contract-sha256")?)?;
    let root = PathBuf::from(get("--evidence-root")?).canonicalize().map_err(|e| e.to_string())?;
    let points = PathBuf::from(get("--points")?);
    let out = PathBuf::from(get("--out")?);
    #[allow(clippy::drop_non_drop)]
    drop(get);
    let archive_dir = a.remove("--archive-dir");
    if !a.is_empty() || out.exists() {
        return Err("receipt39 audit unknown option or existing output".into());
    }
    let base: Params = network.parse::<NetworkId>().map_err(|e| e.to_string())?.into();
    let own = misaka_palw_sdk::native_params_v39::load_own_native_parameters_v39(&base, &snapshot, pin.as_bytes(), sha)?;
    let params = own.parameters();
    if params.genesis.hash.to_string() != genesis {
        return Err("receipt39 audit own genesis differs".into());
    }
    let raw = misaka_palw_sdk::native_audit_io_v32::read_bounded_file_v32(&contract_path, 64 << 20)?;
    if <[u8; 32]>::from(Sha256::digest(&raw)) != contract_sha {
        return Err("receipt39 audit frozen contract pin differs".into());
    }
    let contract: ReceiptWindowContractV39 = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let archive = archive_dir.map(|d| misaka_palw_sdk::native_archive_v39::NativeArchiveDirV39::open(Path::new(&d))).transpose()?;
    let archive_ref =
        archive.as_ref().map(|a| a as &dyn kaspa_consensus_core::palw_native_execution_v39::NativeOriginalSourceArchiveV39);
    let mut accumulator = ReceiptAccumulatorV39::new(&contract)?;
    let file = std::fs::File::open(&points).map_err(|e| e.to_string())?;
    let mut lines = BufReader::new(file).lines();
    let header = lines.next().ok_or("receipt39 audit empty transcript")?.map_err(|e| e.to_string())?;
    if header != "{\"schema\":\"misaka.rfc02.receipt-state-stream.v39\"}" {
        return Err("receipt39 audit transcript schema".into());
    }
    let mut count = 0usize;
    for line in lines {
        let line = line.map_err(|e| e.to_string())?;
        if line.len() > 4096 {
            return Err("receipt39 audit pointer line bound".into());
        }
        let p: serde_json::Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
        let child = p["child"].as_str().ok_or("receipt39 child")?.parse::<Hash64>().map_err(|e| e.to_string())?;
        let parent = p["selected_parent"].as_str().ok_or("receipt39 parent")?.parse::<Hash64>().map_err(|e| e.to_string())?;
        let dir = root.join(p["collected"].as_str().ok_or("receipt39 collected path")?).canonicalize().map_err(|e| e.to_string())?;
        if !dir.starts_with(&root) {
            return Err("receipt39 point escapes the evidence root".into());
        }
        let stream = misaka_palw_sdk::native_audit_io_v39::read_native_aux_stream_anchored_v39(
            &root,
            &dir.join("aux-stream39.jsonl"),
            &dir.join("child-header.json"),
            &dir.join("selected-parent-header.json"),
            &dir.join("envelope39.bin"),
            child,
            parent,
            params,
        )?;
        accumulator.push(AnchoredReceiptPointV39::derive(&stream, params, &contract, archive_ref)?)?;
        count += 1;
    }
    let mut report = accumulator.finish()?;
    report["network"] = serde_json::json!(network);
    report["genesis"] = serde_json::json!(genesis);
    report["transcript_points"] = serde_json::json!(count);
    report["local_qualified_validation_network"] = serde_json::json!(params.native_final_is_local_qualified_v34());
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&out).map_err(|e| e.to_string())?;
    f.write_all(&serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?).and_then(|_| f.sync_all()).map_err(|e| e.to_string())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("receipt39 audit refused: {e}");
        std::process::exit(1);
    }
}
