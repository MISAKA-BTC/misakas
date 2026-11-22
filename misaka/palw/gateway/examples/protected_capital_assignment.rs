//! Sign the first model-capital assignment for an isolated protected registration drill.
//! The caller reads the current native assignment and independent funding outpoint before signing.
use kaspa_consensus_core::{
    config::params::Params,
    mass::MassCalculator,
    network::NetworkId,
    palw_bond_budget_v1::{PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1, palw_capital_assignment_message_v1},
    palw_state_v2::{PalwBondKeyV2, PalwConsensusObjectV2},
    tx::{TransactionOutpoint, UtxoEntry},
};
use kaspa_hashes::Hash64;
use kaspa_pq_validator_core::{PALW_TX_FEE_FLOOR_SOMPI, ValidatorKey, load_validator_seed, relay_fee_for_compute_mass};
use serde_json::Value;
use std::{error::Error, fs, io, path::Path, str::FromStr};

fn outpoint(value: &str) -> Result<TransactionOutpoint, Box<dyn Error>> {
    let (id, index) = value.split_once(':').ok_or("expected txid:index")?;
    Ok(TransactionOutpoint::new(Hash64::from_str(id)?, index.parse()?))
}

fn main() -> Result<(), Box<dyn Error>> {
    let request: Value = serde_json::from_reader(io::stdin().lock())?;
    let manifest_path = Path::new(request["manifest"].as_str().ok_or("missing manifest")?);
    let manifest: Value = serde_json::from_slice(&fs::read(manifest_path)?)?;
    let profile = &manifest["profile"];
    if profile["kind"] != "rfc12-private-integration-probe"
        || profile["network"] != "testnet-12"
        || profile["releaseApproval"] != false
        || profile["protectedClaimPrivatePolicyV3"]["releaseApproval"] != false
        || profile["genesis"] != request["genesis"]
    {
        return Err("expected matching isolated protected genesis".into());
    }
    let seats = manifest["seats"].as_array().ok_or("missing seats")?;
    let owner = seats.iter().map(|s| &s["bond"]).find(|b| b["bondOutpoint"] == request["bond"]).ok_or("unknown private bond")?;
    let keypath = manifest_path.with_file_name(owner["seedFile"].as_str().ok_or("missing seed file")?);
    let key = ValidatorKey::from_seed(load_validator_seed(keypath.to_str().ok_or("invalid key path")?)?);
    if faster_hex::hex_string(key.public_key()) != owner["publicKey"] {
        return Err("private seat key does not match the manifest".into());
    }
    let bond = PalwBondKeyV2(outpoint(request["bond"].as_str().ok_or("missing bond")?)?);
    let input = request["funding"]["outpoint"].as_str().ok_or("missing funding outpoint")?;
    if seats.iter().any(|s| s["bond"]["bondOutpoint"] == input || s["bond"]["feeFloatOutpoint"] == input) {
        return Err("carrier funding overlaps a reserved seat output".into());
    }
    let input = outpoint(input)?;
    let class = Hash64::from_str(request["class"].as_str().ok_or("missing class")?)?;
    let amount = request["amountSompi"].as_u64().filter(|v| *v > 0).ok_or("expected positive assignment")?;
    let domain = Hash64::from_str(manifest["networkDomain"].as_str().ok_or("missing domain")?)?;
    let assignments = vec![(class, amount)];
    let sequence = 1;
    let message = palw_capital_assignment_message_v1(domain, &bond, &assignments, sequence);
    let signature = key.sign_with_context(message.as_byte_slice(), PALW_CAPITAL_ASSIGNMENT_MLDSA87_CONTEXT_V1).to_vec();
    let object = PalwConsensusObjectV2::BondCapitalAssignedV1 { bond, assignments, sequence, signature };
    let spk = misaka_palw_remote::bundle::funding_spk_of_pubkey_v1(key.public_key());
    let utxo = UtxoEntry::new(request["funding"]["amountSompi"].as_u64().ok_or("missing funding amount")?, spk, 0, false);
    let params = Params::from(NetworkId::from_str("testnet-12")?);
    let mass = MassCalculator::new(
        params.mass_per_tx_byte,
        params.mass_per_script_pub_key_byte,
        params.mass_per_sig_op,
        params.storage_mass_parameter,
    );
    let probe = key.build_palw_lifecycle_tx(&object, input, &utxo, PALW_TX_FEE_FLOOR_SOMPI)?;
    let fee = relay_fee_for_compute_mass(mass.calc_non_contextual_masses(&probe).compute_mass).max(PALW_TX_FEE_FLOOR_SOMPI);
    let tx = key.build_palw_lifecycle_tx(&object, input, &utxo, fee)?;
    println!("{}", serde_json::json!({"id": tx.id().to_string(), "transaction": kaspa_rpc_core::RpcTransaction::from(&tx)}));
    Ok(())
}
