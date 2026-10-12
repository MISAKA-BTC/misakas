//! **ADR-0101 — a membership is served by anyone, and a client CHECKS rather than trusts.**
//!
//! The primitives landed with the ADR: a provider signs a [`PalwServiceDescriptorV1`], and
//! `palw_service_descriptor_check_v1` judges it against what the chain says about the line. What
//! did not land was a way to run either from a command line, so the claim was true of the library
//! and of nothing an operator could do. These two commands close that:
//!
//! * `misaka palw service-describe` — a provider signs a descriptor with its own key. The node is
//!   asked only for the facts, so the command refuses a descriptor the chain would refuse BEFORE
//!   it is published anywhere: a grant the line does not declare, a root the line does not own, an
//!   origin grant under a stranger's key.
//! * `misaka palw service-check` — a client judges someone else's descriptor against a node's
//!   answer, and prints which kind of provider it is or the named reason it is not one.
//!
//! **Nothing here touches consensus.** No object is built, no fee is paid, no transaction is sent.
//! A descriptor is a statement to a client (ADR-0101 §3), the chain holds no URL, and a provider
//! that does not serve loses its clients rather than a bond.

use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::palw_model_benefits_v1::grant;
use kaspa_consensus_core::palw_service_descriptor_v1::{
    PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT, PALW_SERVICE_DESCRIPTOR_VERSION_V1, PalwLineServiceFactsV1, PalwServiceDescriptorV1,
    PalwServiceProviderKindV1, palw_service_descriptor_check_v1, palw_service_descriptor_id_v1,
};

use kaspa_rpc_core::api::rpc::RpcApi;

use crate::bond::network_domain;
use crate::node::Ctx;
use crate::wallet::{NodeView, connect};
use crate::{CliError, CliResult, OutputFormat, exit};

/// The verifier every binary ships, as the check wants it.
fn verify(pk: &[u8], msg: &[u8], sig: &[u8], ctx: &[u8]) -> bool {
    kaspa_txscript::verify_mldsa87_with_context(pk, msg, sig, ctx).unwrap_or(false)
}

fn parse_hash64(s: &str, what: &str) -> Result<Hash64, CliError> {
    let mut bytes = [0u8; 64];
    faster_hex::hex_decode(s.trim().as_bytes(), &mut bytes)
        .map_err(|_| CliError::new(exit::GENERIC, format!("{what} must be 128 hex characters, got {:?}", s.trim())))?;
    Ok(Hash64::from_bytes(bytes))
}

fn parse_grants(names: &str) -> Result<u32, CliError> {
    let mut grants = 0u32;
    for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        let up = name.to_ascii_uppercase();
        let bit = grant::NAMES.iter().position(|n| *n == up).ok_or_else(|| {
            CliError::new(exit::GENERIC, format!("'{name}' is not a grant. The set is: {}", grant::NAMES.join(", ")))
        })?;
        grants |= 1 << bit;
    }
    if grants == 0 {
        return Err(CliError::new(exit::GENERIC, "a descriptor that offers no grant offers nothing".to_string()));
    }
    Ok(grants)
}

/// The facts as a node answers them, for one line, at its tip.
async fn line_facts(nv: &NodeView, line: Hash64) -> Result<(PalwLineServiceFactsV1, u64), CliError> {
    let r = nv
        .client
        .get_palw_model_line(line.to_string())
        .await
        .map_err(|e| CliError::new(exit::GENERIC, format!("this node could not answer for line {line}: {e}")))?;
    if !r.exists {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no line {line} (and no class of that id)")));
    }
    let f = &r.service_facts;
    let roots = f.roots.iter().map(|h| parse_hash64(h, "a root")).collect::<Result<Vec<_>, _>>()?;
    let origin_pubkeys = f
        .origin_pubkeys
        .iter()
        .map(|k| {
            let mut bytes = vec![0u8; k.len() / 2];
            faster_hex::hex_decode(k.as_bytes(), &mut bytes)
                .map_err(|_| CliError::new(exit::GENERIC, format!("this node sent a malformed origin key: {k}")))?;
            Ok(bytes)
        })
        .collect::<Result<Vec<Vec<u8>>, CliError>>()?;
    Ok((
        PalwLineServiceFactsV1 { line_id: line, declared_grants: f.declared_grants, roots, origin_pubkeys, now_daa: r.tip_daa },
        r.tip_daa,
    ))
}

fn descriptor_json(d: &PalwServiceDescriptorV1) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw.service-descriptor.v1",
        "version": d.version,
        "line_id": d.line_id.to_string(),
        "grants": d.grants,
        "grant_names": grant::names_of(d.grants),
        "roots": d.roots.iter().map(|r| r.to_string()).collect::<Vec<_>>(),
        "endpoints": d.endpoints,
        "valid_from_daa": d.valid_from_daa,
        "expires_daa": d.expires_daa,
        "provider_pubkey": faster_hex::hex_string(&d.provider_pubkey),
        "signature": faster_hex::hex_string(&d.signature),
    })
}

fn descriptor_from_json(v: &serde_json::Value) -> Result<PalwServiceDescriptorV1, CliError> {
    let bad = |what: &str| CliError::new(exit::GENERIC, format!("the descriptor has no usable {what}"));
    let hexed = |key: &str| -> Result<Vec<u8>, CliError> {
        let s = v.get(key).and_then(|x| x.as_str()).ok_or_else(|| bad(key))?;
        let mut bytes = vec![0u8; s.len() / 2];
        faster_hex::hex_decode(s.as_bytes(), &mut bytes).map_err(|_| bad(key))?;
        Ok(bytes)
    };
    let strings = |key: &str| -> Result<Vec<String>, CliError> {
        Ok(v.get(key)
            .and_then(|x| x.as_array())
            .ok_or_else(|| bad(key))?
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect())
    };
    Ok(PalwServiceDescriptorV1 {
        version: v.get("version").and_then(|x| x.as_u64()).unwrap_or(PALW_SERVICE_DESCRIPTOR_VERSION_V1 as u64) as u16,
        line_id: parse_hash64(v.get("line_id").and_then(|x| x.as_str()).ok_or_else(|| bad("line_id"))?, "line id")?,
        grants: v.get("grants").and_then(|x| x.as_u64()).ok_or_else(|| bad("grants"))? as u32,
        roots: strings("roots")?.iter().map(|r| parse_hash64(r, "a root")).collect::<Result<Vec<_>, _>>()?,
        endpoints: strings("endpoints")?,
        valid_from_daa: v.get("valid_from_daa").and_then(|x| x.as_u64()).ok_or_else(|| bad("valid_from_daa"))?,
        expires_daa: v.get("expires_daa").and_then(|x| x.as_u64()).ok_or_else(|| bad("expires_daa"))?,
        provider_pubkey: hexed("provider_pubkey")?,
        signature: hexed("signature")?,
    })
}

/// A refusal reads the same in both commands, so a provider sees before publishing exactly what a
/// client would see after.
fn print_refusal(
    line: Hash64,
    facts: &PalwLineServiceFactsV1,
    e: &kaspa_consensus_core::palw_service_descriptor_v1::PalwServiceDescriptorError,
) {
    println!("REFUSED  {e}");
    println!("  the chain says about line {line} at daa {}:", facts.now_daa);
    println!(
        "    declares   {}",
        if facts.declared_grants == 0 { "nothing".to_string() } else { grant::names_of(facts.declared_grants).join(", ") }
    );
    println!("    owns       {} root(s)", facts.roots.len());
    for root in &facts.roots {
        println!("               {root}");
    }
    println!("    origin     {} key(s) of its own bonds", facts.origin_pubkeys.len());
}

/// **`misaka palw service-describe --line <id> --grants A,B --endpoint <url> --until <daa>`.**
///
/// Signs a descriptor with the key and prints it. The node is asked for the facts and the SAME
/// check a client will run is run here first, so a descriptor that would be refused is never
/// published — including the two refusals that are easy to earn by accident: offering a grant the
/// line has not declared (or whose declaration has lapsed), and offering a root the line does not
/// own, which past ADR-0143's fence is what a copy line's descriptor is.
#[allow(clippy::too_many_arguments)]
pub async fn service_describe(
    ctx: &Ctx,
    ks: &crate::keys::KeySource,
    line_id: &str,
    grants: &str,
    roots: Vec<String>,
    endpoints: Vec<String>,
    valid_from_daa: Option<u64>,
    expires_daa: u64,
) -> CliResult {
    let line = parse_hash64(line_id, "line id")?;
    let key = ks.load_key()?;
    let grants = parse_grants(grants)?;
    let nv = connect(ctx).await?;
    let (facts, tip) = line_facts(&nv, line).await?;

    // Default the roots to everything the line owns: the common case is "I serve this line", and
    // making the operator retype the chain's own answer is how a descriptor comes to name a root
    // the chain no longer says is the line's.
    let roots = if roots.is_empty() {
        facts.roots.clone()
    } else {
        roots.iter().map(|r| parse_hash64(r, "a root")).collect::<Result<Vec<_>, _>>()?
    };
    if endpoints.is_empty() {
        return Err(CliError::new(exit::GENERIC, "a descriptor with no endpoint tells a client nowhere to go".to_string()));
    }
    if expires_daa <= tip {
        return Err(CliError::new(
            exit::GENERIC,
            format!("--expires-daa {expires_daa} is at or behind the tip ({tip}): the descriptor would be expired when signed"),
        ));
    }

    let mut d = PalwServiceDescriptorV1 {
        version: PALW_SERVICE_DESCRIPTOR_VERSION_V1,
        line_id: line,
        grants,
        roots,
        endpoints,
        valid_from_daa: valid_from_daa.unwrap_or(tip),
        expires_daa,
        provider_pubkey: key.public_key().to_vec(),
        signature: Vec::new(),
    };
    let domain = network_domain(&nv);
    d.signature = key
        .sign_with_context(palw_service_descriptor_id_v1(domain, &d).as_byte_slice(), PALW_SERVICE_DESCRIPTOR_MLDSA87_CONTEXT)
        .to_vec();

    match palw_service_descriptor_check_v1(domain, &d, &facts, verify) {
        Ok(kind) => {
            if ctx.output == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&descriptor_json(&d)).expect("serializable"));
            } else {
                println!("signed a {} descriptor for line {line}", kind_word(kind));
                println!("  grants     {}", grant::names_of(d.grants).join(", "));
                println!("  roots      {}", d.roots.len());
                println!("  endpoints  {}", d.endpoints.join(" "));
                println!("  window     daa {} .. {}", d.valid_from_daa, d.expires_daa);
                println!();
                println!("{}", serde_json::to_string(&descriptor_json(&d)).expect("serializable"));
            }
            Ok(())
        }
        Err(e) => {
            print_refusal(line, &facts, &e);
            Err(CliError::new(exit::GENERIC, format!("this descriptor would be refused by any client: {e}")))
        }
    }
}

fn kind_word(kind: PalwServiceProviderKindV1) -> &'static str {
    match kind {
        PalwServiceProviderKindV1::Origin => "LINE-ORIGIN",
        PalwServiceProviderKindV1::Open => "OPEN-PROVIDER",
    }
}

/// **`misaka palw service-check --descriptor <file|->`.**
///
/// The client's side: read someone's descriptor, ask a node what the chain says about the line it
/// names, and run ADR-0101's check. Prints the provider kind, or the refusal by name — which is the
/// point of carrying named errors rather than a boolean.
pub async fn service_check(ctx: &Ctx, path: &str) -> CliResult {
    let text = if path == "-" {
        std::io::read_to_string(std::io::stdin()).map_err(|e| CliError::new(exit::GENERIC, format!("could not read stdin: {e}")))?
    } else {
        std::fs::read_to_string(path).map_err(|e| CliError::new(exit::GENERIC, format!("could not read {path}: {e}")))?
    };
    let value: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| CliError::new(exit::GENERIC, format!("{path} is not the descriptor JSON: {e}")))?;
    let d = descriptor_from_json(&value)?;
    let nv = connect(ctx).await?;
    let (facts, _) = line_facts(&nv, d.line_id).await?;
    let domain = network_domain(&nv);
    match palw_service_descriptor_check_v1(domain, &d, &facts, verify) {
        Ok(kind) => {
            if ctx.output == OutputFormat::Json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "schema": "misaka.palw.service-check.v1",
                        "ok": true,
                        "kind": kind_word(kind),
                        "line_id": d.line_id.to_string(),
                        "grants": grant::names_of(d.grants),
                        "endpoints": d.endpoints,
                        "tip_daa": facts.now_daa,
                    }))
                    .expect("serializable")
                );
            } else {
                println!("{} for line {}", kind_word(kind), d.line_id);
                println!("  grants     {}", grant::names_of(d.grants).join(", "));
                println!("  endpoints  {}", d.endpoints.join(" "));
                println!("  window     daa {} .. {} (tip {})", d.valid_from_daa, d.expires_daa, facts.now_daa);
                println!("  checked against this node's answer — no consensus rule reads a descriptor");
            }
            Ok(())
        }
        Err(e) => {
            print_refusal(d.line_id, &facts, &e);
            Err(CliError::new(exit::GENERIC, format!("this descriptor is not one to serve from: {e}")))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The JSON is the wire between the two commands**, and it is hand-written on both sides —
    /// which is exactly the shape that drifts silently. A field dropped from `descriptor_json` or
    /// misspelled in `descriptor_from_json` would not fail to compile; it would produce a
    /// descriptor whose id differs from the one that was signed, and the failure would present as
    /// `Signature` — the least informative refusal there is, pointing at the key rather than at the
    /// encoding.
    #[test]
    fn a_descriptor_survives_the_round_trip_it_is_signed_over() {
        let d = PalwServiceDescriptorV1 {
            version: PALW_SERVICE_DESCRIPTOR_VERSION_V1,
            line_id: Hash64::from_u64_word(7),
            grants: grant::PRIORITY_INFERENCE | grant::SUPPORT,
            roots: vec![Hash64::from_u64_word(100), Hash64::from_u64_word(101)],
            endpoints: vec!["https://provider.example/v1".into(), "https://eu.provider.example/v1".into()],
            valid_from_daa: 10,
            expires_daa: 100,
            provider_pubkey: vec![0xAB; 32],
            signature: vec![0xCD; 64],
        };
        let back = descriptor_from_json(&descriptor_json(&d)).expect("the pair agrees");
        assert_eq!(back, d, "every field the id is computed over survives the round trip");
        // The id is the thing that actually has to survive: two encodings that differ anywhere the
        // id reads would sign one statement and check another.
        let domain = Hash64::from_u64_word(0xD0);
        assert_eq!(palw_service_descriptor_id_v1(domain, &back), palw_service_descriptor_id_v1(domain, &d));
    }

    #[test]
    fn the_grant_names_are_the_networks_spelling_and_an_empty_offer_is_refused() {
        assert_eq!(parse_grants("priority_inference, SUPPORT").unwrap(), grant::PRIORITY_INFERENCE | grant::SUPPORT);
        assert!(parse_grants("").is_err(), "a descriptor that offers no grant offers nothing");
        assert!(parse_grants("FASTER").is_err(), "and an invented grant is not a grant");
    }
}
