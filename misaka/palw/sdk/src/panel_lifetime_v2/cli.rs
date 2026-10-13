//! Offline native Panel producer consent. Roots must come from the caller's
//! authenticated selected-chain point. These commands do not grant activation.
use super::*;
use std::collections::BTreeMap;
use std::io::{Read, Write};
const USAGE: &str = "palw-class panel-lifetime-v2 authorize|reaffirm|sign|inspect
All commands: --carriage FILE --state-root HEX --network ID [--drill-salt HEX] --out FILE
AUTHORIZE: --bond TXID:INDEX --terms AUTHORIZATION_BORSH
REAFFIRM: --bond TXID:INDEX --claim HEX
SIGN: --bond TXID:INDEX --draft KERNEL_OBJECT --key-file SEED
INSPECT: --claim HEX or --authorization HEX
The state root must be authenticated from your selected chain. No future claim hash is needed.
AUTHORIZE/REAFFIRM write unsigned inner66/65 drafts; SIGN writes the native outer object for misaka palw submit-object.
A public compiled policy that is not approved remains unavailable; this CLI does not override it.";
fn take(f: &mut BTreeMap<String, String>, k: &str) -> Result<String, String> {
    f.remove(k).ok_or_else(|| format!("{k} required\n{USAGE}"))
}
fn read(path: &str, limit: usize) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err("bounded offline input exceeded".into());
    }
    Ok(bytes)
}
fn write(path: &str, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new().create_new(true).write(true).open(path).map_err(|e| e.to_string())?;
    file.write_all(bytes).and_then(|_| file.sync_all()).map_err(|e| e.to_string())?;
    println!("wrote {path} ({} bytes); native admission remains required", bytes.len());
    Ok(())
}
fn hash(f: &mut BTreeMap<String, String>, name: &str) -> Result<Hash64, String> {
    crate::improve_objects::spec::hash(&take(f, name)?)
}
pub fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() || args.iter().any(|a| a == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    if (args.len() - 1) % 2 != 0 {
        return Err(USAGE.into());
    }
    let mut f = BTreeMap::new();
    for pair in args[1..].chunks_exact(2) {
        if !pair[0].starts_with("--") || f.insert(pair[0].clone(), pair[1].clone()).is_some() {
            return Err("unexpected positional or duplicate option".into());
        }
    }
    let out = take(&mut f, "--out")?;
    let root = hash(&mut f, "--state-root")?;
    let network: kaspa_consensus_core::network::NetworkId = take(&mut f, "--network")?.parse().map_err(|e| format!("network: {e}"))?;
    let salt = f
        .remove("--drill-salt")
        .map(|s| kaspa_consensus_core::config::drill::PalwDrillSaltV1::from_hex(&s))
        .transpose()
        .map_err(|e| e.to_string())?;
    let params = kaspa_consensus_core::config::drill::palw_chain_params_v1(network, salt.as_ref())?;
    let kaspa_consensus_core::palw_mode_v2::PalwConsensusMode::ConsensusV2(bundle) = &params.palw_consensus_mode else {
        return Err("compiled native consensus unavailable".into());
    };
    let bytes = read(&take(&mut f, "--carriage")?, 512 << 20)?;
    let carriage: kaspa_consensus_core::palw_state_v2::PalwStateCarriageV2 = borsh::from_slice(&bytes).map_err(|e| e.to_string())?;
    let daa = carriage.last_point.as_ref().map(|p| p.daa_score).unwrap_or(0);
    let armed = params.palw_uncertified_weightless.is_some_and(|a| a.is_active(daa));
    let state =
        carriage.into_state_v3(&bundle.state, Some(root), armed, params.palw_canonical_work_daa()).map_err(|e| e.to_string())?;
    let bytes = match args[0].to_ascii_lowercase().as_str() {
        "authorize" => {
            let producer = crate::improve_objects::spec::bond(&take(&mut f, "--bond")?)?;
            let terms: PanelLifetimeAuthorizationV2 =
                borsh::from_slice(&read(&take(&mut f, "--terms")?, 4096)?).map_err(|e| e.to_string())?;
            prepare_panel_authorization_object_v2(&params, &state, &producer, daa, terms).map_err(|e| e.to_string())?.encode()
        }
        "reaffirm" => {
            let producer = crate::improve_objects::spec::bond(&take(&mut f, "--bond")?)?;
            let claim = hash(&mut f, "--claim")?;
            prepare_panel_lifetime_object_v2(&params, &state, &producer, &claim, daa).map_err(|e| e.to_string())?.encode()
        }
        "sign" => {
            let bond = crate::improve_objects::spec::bond(&take(&mut f, "--bond")?)?;
            let draft = KernelRouteObjectV1::decode(&read(&take(&mut f, "--draft")?, 4096)?).map_err(|e| e.to_string())?;
            match &draft {
                KernelRouteObjectV1::AuthorizePanelLifetimeV2 { authorization } => {
                    prepare_panel_authorization_object_v2(&params, &state, &bond, daa, *authorization).map_err(|e| e.to_string())?;
                }
                KernelRouteObjectV1::EnrollPanelConsumerLifetimeV2 { terms } => {
                    let expected = prepare_panel_lifetime_object_v2(&params, &state, &bond, &Hash64::from_bytes(terms.claim), daa)
                        .map_err(|e| e.to_string())?;
                    if expected.encode() != draft.encode() {
                        return Err("draft changes captured native consent".into());
                    }
                }
                _ => return Err("Panel signer accepts inner65/66 only".into()),
            }
            let seed = kaspa_pq_validator_core::load_validator_seed(&take(&mut f, "--key-file")?)?;
            let key = kaspa_pq_validator_core::ValidatorKey::from_seed(seed);
            let domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
                params.net.to_string().as_bytes(),
                Some(params.genesis.hash),
            );
            borsh::to_vec(&sign_panel_lifetime_object_v2(&state, &key, domain, bond, &draft)?).map_err(|e| e.to_string())?
        }
        "inspect" => {
            let route = state.kernel_route().ok_or("pinned native consent route missing")?;
            if f.contains_key("--claim") {
                borsh::to_vec(&route.panel_lifetime_enrollment_v2(&hash(&mut f, "--claim")?)?).map_err(|e| e.to_string())?
            } else {
                borsh::to_vec(&route.panel_authorization_v2(&hash(&mut f, "--authorization")?.as_bytes())?)
                    .map_err(|e| e.to_string())?
            }
        }
        _ => return Err(USAGE.into()),
    };
    if !f.is_empty() {
        return Err(format!("unknown options: {:?}", f.keys()));
    }
    write(&out, &bytes)
}
