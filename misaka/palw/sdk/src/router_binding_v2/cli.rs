//! Offline signer; output is a Borsh `PalwConsensusObjectV2` carrier.
use super::*;
use std::{
    collections::BTreeMap,
    io::{Read, Write},
};

const USAGE: &str = "palw-class router-binding-v2 sign --network ID [--drill-salt HEX] --declaration FILE --key-file SEED --out FILE
FILE is borsh(PalwRouterBindingDeclarationV2): version 1, class_id, artifact_root, full registered profile,
and an explicit complete ascending (table, reader, router) list. Table 0 is GDN; table 1 is attention.
Writes the existing signed tag-244 carrier for misaka palw submit-object.
Native activation, registrant authority, artifact identity, immutability and admission remain required.

router-binding-v2 court-expectation --object CARRIER --binding-row ROW --block-hash HASH
  --expected-verdict ExecutorGuilty|FalseAccusation --id ID --role ROLE --out FILE
Exports a frozen campaign requirement from actual Borsh inputs; never an observation or approval.";
const MAX_DECLARATION_BYTES: u64 = 16 << 20;

fn take(options: &mut BTreeMap<String, String>, key: &str) -> Result<String, String> {
    options.remove(key).ok_or_else(|| format!("{key} required\n{USAGE}"))
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|arg| arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    if args[0] == "court-expectation" {
        return court_expectation_command(&args[1..]);
    }
    if args[0] != "sign" || (args.len() - 1) % 2 != 0 {
        return Err(USAGE.into());
    }
    let mut options = BTreeMap::new();
    for pair in args[1..].chunks_exact(2) {
        if !pair[0].starts_with("--") || options.insert(pair[0].clone(), pair[1].clone()).is_some() {
            return Err("unexpected positional or duplicate option".into());
        }
    }
    let network = take(&mut options, "--network")?.parse().map_err(|e| format!("network: {e}"))?;
    let salt = options
        .remove("--drill-salt")
        .map(|value| kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&value))
        .transpose()
        .map_err(|e| e.to_string())?;
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, salt.as_ref())?;
    let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    let declaration_path = take(&mut options, "--declaration")?;
    let key_path = take(&mut options, "--key-file")?;
    let output = take(&mut options, "--out")?;
    if !options.is_empty() {
        return Err(format!("unknown options: {:?}", options.keys()));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(&declaration_path)
        .map_err(|e| e.to_string())?
        .take(MAX_DECLARATION_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_DECLARATION_BYTES {
        return Err("router declaration exceeds offline input bound".into());
    }
    let declaration = borsh::from_slice(&bytes).map_err(|e| format!("router declaration: {e}"))?;
    // Refuse invalid graph/bindings before loading the signing key.
    palw_router_binding_check_v2(&declaration).map_err(str::to_owned)?;
    let seed = kaspa_pq_validator_core::load_validator_seed(&key_path)?;
    let object = signed_router_binding_v2(&ValidatorKey::from_seed(seed), domain, declaration)?;
    let encoded = borsh::to_vec(&object).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&output).map_err(|e| e.to_string())?;
    file.write_all(&encoded).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    println!("wrote {output} ({} bytes); native acceptance remains required", encoded.len());
    Ok(())
}

/// This is an offline input exporter, never a native outcome observer.
fn court_expectation_command(args: &[String]) -> Result<(), String> {
    if args.len() % 2 != 0 {
        return Err("court-expectation requires named option/value pairs".into());
    }
    let mut options = BTreeMap::new();
    for pair in args.chunks_exact(2) {
        if !pair[0].starts_with("--") || options.insert(pair[0].clone(), pair[1].clone()).is_some() {
            return Err("unexpected positional or duplicate option".into());
        }
    }
    let object_path = take(&mut options, "--object")?;
    let row_path = take(&mut options, "--binding-row")?;
    let block: Hash64 = take(&mut options, "--block-hash")?.parse().map_err(|e| format!("block hash: {e}"))?;
    let verdict = take(&mut options, "--expected-verdict")?;
    let id = take(&mut options, "--id")?;
    let role = take(&mut options, "--role")?;
    let output = take(&mut options, "--out")?;
    if !options.is_empty() {
        return Err(format!("unknown options: {:?}", options.keys()));
    }
    fn read_bound(path: &str) -> Result<Vec<u8>, String> {
        let mut data = Vec::new();
        std::fs::File::open(path)
            .map_err(|e| e.to_string())?
            .take(MAX_DECLARATION_BYTES + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 > MAX_DECLARATION_BYTES {
            return Err("court input exceeds offline bound".into());
        }
        Ok(data)
    }
    let object = borsh::from_slice(&read_bound(&object_path)?).map_err(|e| format!("court carrier: {e}"))?;
    let row = borsh::from_slice(&read_bound(&row_path)?).map_err(|e| format!("binding row: {e}"))?;
    let expected = super::court_expectation::expectation(&object, &row, block, &verdict, &id, &role)?;
    let bytes = serde_json::to_vec_pretty(&expected).map_err(|e| e.to_string())?;
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(&output).map_err(|e| e.to_string())?;
    file.write_all(&bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    println!("wrote frozen court expectation {output}; no observation or approval produced");
    Ok(())
}
