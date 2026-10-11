//! Offline CLI. Output is reviewable signed material; `misaka palw submit-object` carries it.
use super::*;
use rand::{RngCore, rngs::OsRng};
use std::{
    collections::BTreeMap,
    fs::OpenOptions,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const USAGE: &str = "palw-class flat-v6 — prepare protected native candidates offline

All commands: --network ID [--drill-salt HEX] --key-file SEED --bond TXID:INDEX --out PATH
    post   --class HEX --binding HEX --post FLAT_V3_BODY
    seal   --vector HEX --proof FLAT_V5_FAULT --secret-out PRIVATE_PATH
    reveal --secret PRIVATE_PATH --row NATIVE_SEAL_ROW --height DAA
    expire --vector HEX --actor HEX

seal writes only the public seal to --out. It creates --secret-out with a fresh OS-random
salt and the exact proof, network and signing bond, with mode 0600 on Unix. Keep it private.
reveal checks the supplied row and height before writing a public proof. Supplied rows are
UNVERIFIED_REMOTE_STATE; the node authenticates its own row and clocks at acceptance.
Outputs are never overwritten. Large objects have .chunkN parts beside --out. Large posts
use generic carriage; large reveals use the sealing bond's kernel slots. Timely inclusion
and retention are not established by this tool.
Submit the listed carriers with `misaka palw submit-object --object PATH`. Review them first.
Preparation grants no activation, admission, fidelity, reward eligibility or execution Final.";

fn read_bounded(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut bytes = Vec::new();
    file.take((max + 1) as u64).read_to_end(&mut bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.len() > max {
        return Err(format!("{} exceeds the {max}-byte bound", path.display()));
    }
    Ok(bytes)
}
/// Reserve all outputs with create_new before writing any bytes. Clean up only the newly
/// created files on an ordinary I/O failure; existing files and symlinks are never touched.
fn write_new_files(files: &[(PathBuf, Vec<u8>)]) -> Result<(), String> {
    let mut opened = Vec::new();
    let result = (|| {
        for (path, _) in files {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options.open(path).map_err(|e| format!("{}: {e}; outputs are never overwritten", path.display()))?;
            opened.push(file);
        }
        for (file, (_, bytes)) in opened.iter_mut().zip(files) {
            file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if result.is_err() {
        for (path, _) in files.iter().take(opened.len()) {
            let _ = std::fs::remove_file(path);
        }
    }
    result
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args == ["--help"] {
        println!("{USAGE}");
        return Ok(());
    }
    let kind = args.first().ok_or(USAGE)?;
    if !matches!(kind.as_str(), "post" | "seal" | "reveal" | "expire") {
        return Err(USAGE.into());
    }
    let mut flags = BTreeMap::new();
    let mut rest = args[1..].iter();
    while let Some(flag) = rest.next() {
        let value = rest.next().filter(|v| !v.starts_with("--")).ok_or_else(|| format!("{flag} requires a value"))?;
        if !flag.starts_with("--") || flags.insert(flag.clone(), value.clone()).is_some() {
            return Err(format!("invalid or repeated option: {flag}"));
        }
    }
    let take = |flags: &mut BTreeMap<String, String>, name: &str| flags.remove(name).ok_or_else(|| format!("{name} is required"));
    let network: kaspa_consensus_core::network::NetworkId =
        take(&mut flags, "--network")?.parse().map_err(|e| format!("--network: {e}"))?;
    let drill = flags
        .remove("--drill-salt")
        .map(|hex| kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&hex))
        .transpose()
        .map_err(|e| format!("--drill-salt: {e}"))?;
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, drill.as_ref())?;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let key_file = take(&mut flags, "--key-file")?;
    let bond = crate::improve_objects::spec::bond(&take(&mut flags, "--bond")?)?;
    let out = PathBuf::from(take(&mut flags, "--out")?);
    let hash = |value: String| crate::improve_objects::spec::hash(&value);
    let mut private = None;
    let mut reveal_row = None;
    let (vector, object) = match kind.as_str() {
        "post" => {
            let class = hash(take(&mut flags, "--class")?)?;
            let binding = hash(take(&mut flags, "--binding")?)?;
            let path = PathBuf::from(take(&mut flags, "--post")?);
            let body = decode_flat_model_vector_post_v3(&read_bounded(
                &path,
                kaspa_consensus_core::palw_flat_model_vector_v3::FLAT_MODEL_VECTOR_MAX_POST_BYTES_V3,
            )?)
            .map_err(|e| e.to_string())?;
            prepare_flat_model_post_v6(class, binding, body)?
        }
        "seal" => {
            let vector = hash(take(&mut flags, "--vector")?)?;
            let path = PathBuf::from(take(&mut flags, "--proof")?);
            let secret_path = PathBuf::from(take(&mut flags, "--secret-out")?);
            let mut salt = [0; 64];
            OsRng.try_fill_bytes(&mut salt).map_err(|e| format!("OS randomness: {e}"))?;
            let secret = FlatProofSecretV6::new(domain, vector, bond, salt, read_bounded(&path, FLAT_SEAL_MAX_PROOF_BYTES_V6)?)?;
            let object = secret.seal_object(domain, &bond)?;
            private = Some((secret_path, secret.to_bytes()?));
            (vector, object)
        }
        "reveal" => {
            let path = PathBuf::from(take(&mut flags, "--secret")?);
            let secret =
                FlatProofSecretV6::from_bytes(&read_bounded(&path, FLAT_PROOF_SECRET_MAX_BYTES_V6)?).map_err(|e| e.to_string())?;
            let row_path = PathBuf::from(take(&mut flags, "--row")?);
            let row: FlatNativeSealRowV6 = borsh::from_slice(&read_bounded(&row_path, 512)?).map_err(|e| format!("seal row: {e}"))?;
            let daa = take(&mut flags, "--height")?.parse().map_err(|_| "--height must be a u64")?;
            reveal_row = Some((row.clone(), daa));
            (secret.vector, secret.reveal_object(domain, &bond, &row, daa)?)
        }
        "expire" => {
            let vector = hash(take(&mut flags, "--vector")?)?;
            let actor = hash(take(&mut flags, "--actor")?)?;
            (vector, K::ExpireFlatModelSealV6 { vector: vector.as_bytes(), actor: actor.as_bytes() })
        }
        _ => unreachable!(),
    };
    if !flags.is_empty() {
        return Err(format!("unknown options: {}", flags.keys().cloned().collect::<Vec<_>>().join(", ")));
    }
    let key = kaspa_pq_validator_core::ValidatorKey::from_seed(kaspa_pq_validator_core::load_validator_seed(&key_file)?);
    let signed = sign_flat_object_v6(&key, domain, bond, &object)?;
    let whole = borsh::to_vec(&signed).map_err(|e| e.to_string())?;
    let carriers = match reveal_row {
        Some((row, daa)) => flat_reveal_carriers_v6(
            &key,
            domain,
            bond,
            signed,
            &row,
            daa,
            kaspa_consensus_core::palw_state_v2::PALW_OBJECT_CHUNK_MAX_BYTES,
        )?,
        None => flat_object_carriers_v6(signed)?,
    };
    let mut files = vec![(out.clone(), whole)];
    let mut paths = vec![out.clone()];
    if carriers.len() > 1 {
        paths.clear();
        for (i, carrier) in carriers.iter().enumerate() {
            let mut path = out.clone().into_os_string();
            path.push(format!(".chunk{i}"));
            let path = PathBuf::from(path);
            files.push((path.clone(), borsh::to_vec(carrier).map_err(|e| e.to_string())?));
            paths.push(path);
        }
    }
    let private_path = private.as_ref().map(|(path, _)| path.clone());
    if let Some(file) = private {
        files.push(file);
    }
    write_new_files(&files)?;
    println!(
        "{}",
        serde_json::json!({
            "schema": "misaka.palw.flat-native-preparation.v6", "operation": kind,
            "vector": vector.to_string(), "whole_object": out, "carriers": paths,
            "private_material": private_path, "supplied_row_trust": "UNVERIFIED_REMOTE_STATE",
            "activation": "not_granted", "execution_final": "not_granted",
        })
    );
    Ok(())
}
