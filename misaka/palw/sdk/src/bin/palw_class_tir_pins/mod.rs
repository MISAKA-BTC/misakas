//! Derive candidate TKR1 parent pins with native identities, before the live quote.
//! The supplied semantic clock and EOF offer are not assertions of acceptance.
use kaspa_consensus_core::{
    config::drill::{PalwDrillSaltV1, palw_chain_params_v1},
    network::NetworkId,
    palw_attempt_v2::palw_network_domain_v2_for,
    palw_class_semantics_v1::PalwClassSemanticsDeclarationV1,
    palw_tir_admission_v1::palw_tir_class_record_v1,
    palw_tokenizer_artifact_inventory_v1::PalwTokenizerArtifactInventoryOfferV1,
    palw_tokenizer_registration_v1::TokenizerRegistrationPolicyV1,
};
use misaka_palw_sdk::lineages::tir::TirLineageV1;
use std::{fs, io::Read, io::Write, path::Path};

fn read(path: &str, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path).and_then(|file| file.take(limit + 1).read_to_end(&mut bytes)).map_err(|e| format!("{path}: {e}"))?;
    if bytes.len() as u64 > limit {
        return Err(format!("{path} exceeds its input bound"));
    }
    Ok(bytes)
}

pub(super) fn run(args: &mut Vec<String>, network: Option<&str>) -> Result<(), String> {
    let network: NetworkId = network.ok_or("--network is required")?.parse().map_err(|e| format!("--network: {e}"))?;
    let salt = super::take_flag(args, "--drill-salt").map(|s| PalwDrillSaltV1::from_hex(&s).map_err(|e| e.to_string())).transpose()?;
    let params = palw_chain_params_v1(network, salt.as_ref())?;
    let mut required = |flag| super::take_flag(args, flag).ok_or_else(|| format!("{flag} is required"));
    let artifact = required("--artifact")?;
    let container = required("--container")?;
    let semantics_path = required("--semantics")?;
    let bound_daa: u64 = required("--semantics-bound-daa")?.parse().map_err(|e| format!("--semantics-bound-daa: {e}"))?;
    let offer_path = required("--eof-offer")?;
    let policy_path = required("--policy")?;
    let output = required("--out")?;
    if !args.is_empty() {
        return Err("unrecognized tir-fp-pins arguments".into());
    }
    let entry = TirLineageV1::open_entry(Path::new(&artifact))?;
    let class = entry.class_id();
    let (record, _) = palw_tir_class_record_v1(&entry.class, &entry.artifact_root).map_err(|e| e.to_string())?;
    let semantics: PalwClassSemanticsDeclarationV1 = borsh::from_slice(&read(&semantics_path, 4096)?).map_err(|e| e.to_string())?;
    semantics.check().map_err(str::to_owned)?;
    if semantics.class_id != class || semantics.artifact_root != entry.artifact_root || semantics.tokenizer_id != record.tokenizer_id {
        return Err("semantic declaration differs from the selected TIR class".into());
    }
    let binding = super::checked_tir_container_binding(Path::new(&container), class, entry.artifact_root, &record)?;
    let offer: PalwTokenizerArtifactInventoryOfferV1 = borsh::from_slice(&read(&offer_path, 8192)?).map_err(|e| e.to_string())?;
    offer.check_v1()?;
    if offer.artifact_hash != semantics.tokenizer_id || offer.maximum_bytes != semantics.raw_tokenizer_bytes {
        return Err("EOF offer does not name the selected tokenizer and exact raw length".into());
    }
    let policy: TokenizerRegistrationPolicyV1 = borsh::from_slice(&read(&policy_path, 4096)?).map_err(|e| e.to_string())?;
    policy.check_v1()?;
    let rule = borsh::to_vec(&semantics.rendering_rule).map_err(|e| e.to_string())?;
    if rule.len() != 1 {
        return Err("unknown native rendering rule encoding".into());
    }
    let result = serde_json::json!({
        "version":1,"kind":"tokenizerParentPinsCandidate","parentsAuthenticated":false,
        "semanticsBoundDaa":bound_daa,"semanticsBoundDaaAuthenticated":false,
        "offerSignatureVerified":false,"nativeInventoryAccepted":false,
        "partialSpecification":{
            "version":1,
            "network":palw_network_domain_v2_for(params.net.to_string().as_bytes(),Some(params.genesis.hash)).to_string(),
            "policy_id":policy.id_v1()?.to_string(),"class_id":class.to_string(),
            "container_root":binding.container_root.to_string(),
            "semantics_id":semantics.as_bound_row(bound_daa).commitment().to_string(),
            "inventory_id":offer.id_v1().to_string(),"artifact_root":entry.artifact_root.to_string(),
            "tokenizer_id":semantics.tokenizer_id.to_string(),"table_root":semantics.token_table_root.to_string(),
            "vocab":record.logits_vocab,"rendering_rule":rule[0],
            "opener_transaction_id":offer.opener.0.transaction_id.to_string(),"opener_index":offer.opener.0.index,
            "declared_key_events":semantics.tokenizer_key_events
        },
        "requiredSpecificationFields":["nonce","proposed_accepting_daa","accept_until_daa","finish_until_daa","max_registration_charge"],
        "next":"fill explicit nonce/clocks/charge; draft-open then require the live native quote before preparing/signing TKR1"
    });
    let bytes = serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?;
    let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&output).map_err(|e| format!("{output}: {e}"))?;
    file.write_all(&bytes).map_err(|e| format!("{output}: {e}"))?;
    println!("{result}");
    Ok(())
}
