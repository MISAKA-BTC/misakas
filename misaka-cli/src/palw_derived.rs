//! **ADR-0078 Decision 5 — verification belongs to the consumer, and the chain makes it possible.**
//!
//! `misaka palw derived` reads what the chain holds about one free-prompt claim's derivations, and
//! `misaka palw derived-verify` re-runs the derivation over the answer the consumer keeps and says
//! whether the chain's copy agrees.
//!
//! Why this is a CLI and not a browser tab: Decision 5's promise is that "a false object is
//! publicly demonstrable by anyone holding the DSL", and a demonstration nobody can perform is a
//! promise the chain does not keep. The check is three pure functions over bytes the consumer
//! already has (invariant X6):
//!
//! ```text
//! output_root   = output_commitment_v2(job_context_hash, output_token_ids, family_rendered_hash)
//! dsl_hash      = H(grammar_id ‖ grammar.canonicalize(answer))
//! artifact_hash = H(transformer.run(canonical DSL))
//! ```
//!
//! **What the chain does not have, and what the verifier must therefore hold.** The answer's
//! `output_token_ids` are on no chain in any form — the claim commits `output_root` and nothing
//! else, because ADR-0044 Decision 8's sentence about not silently publishing prompts applies to
//! answers word for word. So the gateway's own response is the second half of the evidence: it
//! carries `output_token_ids`, `job_context_hash`, `family` and the canonical DSL beside the
//! answer, and this command takes that file. Verification is a comparison of two independent
//! sources, and a verifier who had both from the same source would be checking nothing.
//!
//! Nothing here signs, spends or submits: two reads and some arithmetic.

use crate::node::Ctx;
use crate::{CliError, CliResult, OutputFormat, exit};
use kaspa_consensus_core::Hash64;
use kaspa_consensus_core::network::{EndpointKind, NetworkId};
use kaspa_consensus_core::palw_derived_v1::{PALW_DERIVED_V1_VERSION, PalwDerivedArtifactV1, derived_id_v1, kind};
use kaspa_rpc_core::api::rpc::RpcApi;
use kaspa_rpc_core::{GetPalwDerivedArtifactsResponse, RpcPalwDerivedArtifact};
use kaspa_wrpc_client::client::{ConnectOptions, ConnectStrategy};
use kaspa_wrpc_client::{KaspaRpcClient, WrpcEncoding};
use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;
use std::str::FromStr;
use std::time::Duration;

// ---------------------------------------------------------------------------------------------
// the connection
// ---------------------------------------------------------------------------------------------

/// A read-only wRPC connection, deliberately lighter than `wallet::connect`.
///
/// The wallet's connect refuses a node without `--utxoindex`, which is right for a spender and
/// wrong here: a stranger verifying somebody else's provenance has no UTXOs to select and no
/// reason to be turned away by an index they will never read. The network match IS kept — a claim
/// id means nothing without the chain it was made on.
pub(crate) struct Reader {
    pub(crate) client: KaspaRpcClient,
    network_domain: Hash64,
}

pub(crate) async fn connect(ctx: &Ctx) -> Result<Reader, CliError> {
    let net = NetworkId::from_str(&ctx.network)
        .map_err(|e| CliError::new(exit::GENERIC, format!("bad --network '{}': {e}", ctx.network)))?;
    let registry = misaka_endpoints::EndpointRegistry::load(&ctx.network);
    let hostport = misaka_endpoints::resolve(&net, EndpointKind::NodeWrpcBorsh, ctx.rpc.as_deref(), registry.as_ref());
    let url = format!("ws://{hostport}");
    let client = KaspaRpcClient::new(WrpcEncoding::Borsh, Some(&url), None, None, None)
        .map_err(|e| CliError::new(exit::CONNECTION, format!("build wRPC client: {e}")))?;
    let options = ConnectOptions {
        block_async_connect: true,
        connect_timeout: Some(Duration::from_secs(ctx.timeout_secs.clamp(2, 15))),
        strategy: ConnectStrategy::Fallback,
        ..Default::default()
    };
    client
        .connect(Some(options))
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("connect {url}: {e} (node up with --rpclisten-borsh?)")))?;
    let server = client.get_server_info().await.map_err(|e| CliError::new(exit::CONNECTION, format!("getServerInfo: {e}")))?;
    if server.network_id.to_string() != ctx.network {
        return Err(CliError::new(
            exit::NETWORK_MISMATCH,
            format!("node is '{}' but --network is '{}'", server.network_id, ctx.network),
        ));
    }
    // The domain a derivation was signed under is bound to the GENESIS, not the network's name:
    // two incarnations of testnet-11 are two different chains and an object of one is not an
    // object of the other. Taken from the node so it cannot be a constant the CLI gets wrong.
    let params = kaspa_consensus_core::config::params::Params::from(server.network_id);
    let network_domain = kaspa_consensus_core::palw_attempt_v2::palw_network_domain_v2_for(
        params.net.to_string().as_bytes(),
        Some(params.genesis.hash),
    );
    Ok(Reader { client, network_domain })
}

fn parse_claim_id(s: &str) -> Result<Hash64, CliError> {
    s.trim()
        .parse::<Hash64>()
        .map_err(|_| CliError::new(exit::GENERIC, format!("'{s}' is not a 128-hex claim id (the gateway's `fp_claim_id`)")))
}

// ---------------------------------------------------------------------------------------------
// `misaka palw derived <claim-id>` — what the chain says
// ---------------------------------------------------------------------------------------------

pub async fn show(ctx: &Ctx, claim_id: &str, json: bool) -> CliResult {
    let claim = parse_claim_id(claim_id)?;
    let reader = connect(ctx).await?;
    let response = reader
        .client
        .get_palw_derived_artifacts(claim.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwDerivedArtifacts: {e}")))?;
    let as_json = json || ctx.output == OutputFormat::Json;
    if as_json {
        println!("{}", serde_json::to_string_pretty(&derived_json(&response)).expect("serializable"));
    } else {
        print_human(&response);
    }
    if !response.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no claim {claim}")));
    }
    Ok(())
}

fn derived_json(r: &GetPalwDerivedArtifactsResponse) -> serde_json::Value {
    serde_json::json!({
        "schema": "misaka.palw.chain-derived-artifacts.v1",
        "found": r.found,
        "claim_id": r.claim_id,
        "output_root": r.output_root,
        "executor_pubkey": r.executor_pubkey,
        "executor_bond": r.executor_bond,
        "class_id": r.class_id,
        "claim_phase": r.claim_phase,
        "claim_void_reason": r.claim_void_reason,
        "claim_accepted_block": r.claim_accepted_block,
        "claim_accepted_daa": r.claim_accepted_daa,
        "artifacts": r.artifacts.iter().map(|a| serde_json::json!({
            "transformer_id": a.transformer_id,
            "derived_id": a.derived_id,
            "grammar_id": a.grammar_id,
            "kind": a.kind,
            "kind_name": a.kind_name,
            "dsl_hash": a.dsl_hash,
            "artifact_hash": a.artifact_hash,
            "artifact_bytes": a.artifact_bytes,
            "accepted_daa": a.accepted_daa,
        })).collect::<Vec<_>>(),
        // Said in the payload, not only in a doc comment: a reader who scripts against this file
        // must not conclude the ids were withheld by accident.
        "output_token_ids": serde_json::Value::Null,
        "note": "output_token_ids are on NO chain (ADR-0078 Decision 2): the claim commits \
                 output_commitment_v2(job_context_hash, ids, family_rendered_hash) and nothing else. \
                 Hold the ids from the gateway response and run `misaka palw derived-verify`.",
    })
}

fn print_human(r: &GetPalwDerivedArtifactsResponse) {
    if !r.found {
        println!("claim {}: not on this chain", r.claim_id);
        return;
    }
    println!("claim         {}", r.claim_id);
    println!(
        "  phase       {}{}",
        r.claim_phase,
        if r.claim_void_reason.is_empty() { String::new() } else { format!(" ({})", r.claim_void_reason) }
    );
    println!("  class       {}", r.class_id);
    println!("  output_root {}", r.output_root);
    println!("  executor    bond {} key {}…", r.executor_bond, short(&r.executor_pubkey));
    println!("  accepted    block {} at DAA {}", r.claim_accepted_block, r.claim_accepted_daa);
    if r.artifacts.is_empty() {
        println!("  derivations none");
        return;
    }
    println!("  derivations {}", r.artifacts.len());
    for a in &r.artifacts {
        let name = if a.kind_name.is_empty() { "?".to_string() } else { a.kind_name.clone() };
        println!("    kind {} ({name})  {} bytes, accepted at DAA {}", a.kind, a.artifact_bytes, a.accepted_daa);
        println!("      transformer_id {}", a.transformer_id);
        println!("      grammar_id     {}", a.grammar_id);
        println!("      dsl_hash       {}", a.dsl_hash);
        println!("      artifact_hash  {}", a.artifact_hash);
        println!("      derived_id     {}", a.derived_id);
    }
    println!(
        "  the answer's token ids are on no chain — verify with the gateway response: \
         misaka palw derived-verify {} --answer <response.json>",
        r.claim_id
    );
}

fn short(hex: &str) -> &str {
    &hex[..hex.len().min(16)]
}

// ---------------------------------------------------------------------------------------------
// `misaka palw derived-verify` — the consumer's arithmetic
// ---------------------------------------------------------------------------------------------

/// What the consumer holds beside the chain: the answer's ids, the job's context hash, the family
/// whose rendered-hash rule applies, and the answer bytes the derivation consumed.
struct ConsumerMaterial {
    output_token_ids: Option<Vec<u32>>,
    job_context_hash: Option<Hash64>,
    family: Option<PalwRcFamilyV1>,
    answer: Option<Vec<u8>>,
}

/// Read the gateway's own response — the `misaka` block of a chat completion, the block itself, or
/// a bare JSON array of ids — plus any flag overrides.
///
/// A bare array is accepted because a consumer may keep only the ids; then the other three inputs
/// come from flags. A missing one narrows what can be checked and the verdict says so by name —
/// and with none of them AND no DSL, `verify` refuses rather than printing a pass it did not earn.
fn read_material(
    answer_path: &std::path::Path,
    dsl_path: Option<&std::path::Path>,
    job_context_hash: Option<&str>,
    family: Option<&str>,
) -> Result<ConsumerMaterial, CliError> {
    let bytes = std::fs::read(answer_path).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", answer_path.display())))?;
    let doc: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| {
        CliError::new(
            exit::GENERIC,
            format!(
                "{} is not JSON: {e} — pass the gateway's response (its `misaka` block) or a JSON array of output token ids",
                answer_path.display()
            ),
        )
    })?;
    // The gateway nests the verifier's facts under `misaka` inside a chat-completion response;
    // the block on its own and the array on its own are both legal inputs.
    let block = doc.get("misaka").unwrap_or(&doc);
    let ids = if let Some(arr) = doc.as_array() {
        Some(parse_ids(arr, answer_path)?)
    } else if let Some(arr) = block.get("output_token_ids").and_then(|v| v.as_array()) {
        Some(parse_ids(arr, answer_path)?)
    } else {
        None
    };
    let ctx_hex =
        job_context_hash.map(|s| s.to_string()).or_else(|| block.get("job_context_hash").and_then(|v| v.as_str()).map(str::to_string));
    let job_context_hash = match ctx_hex {
        Some(h) => Some(
            h.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("job context hash '{h}' is not 128-hex")))?,
        ),
        None => None,
    };
    let family_name = family.map(|s| s.to_string()).or_else(|| block.get("family").and_then(|v| v.as_str()).map(str::to_string));
    let family = match family_name {
        Some(name) => Some(PalwRcFamilyV1::parse(&name).ok_or_else(|| {
            CliError::new(exit::GENERIC, format!("unknown family '{name}': this build knows base0, qwen36 and qwen25-a16"))
        })?),
        None => None,
    };
    // The answer the derivation consumed: an explicit file wins, else the canonical DSL the
    // gateway returned beside the artifact.
    let answer = match dsl_path {
        Some(p) => Some(std::fs::read(p).map_err(|e| CliError::new(exit::GENERIC, format!("{}: {e}", p.display())))?),
        None => block.get("derivation").and_then(|d| d.get("dsl")).and_then(|v| v.as_str()).map(|s| s.as_bytes().to_vec()),
    };
    Ok(ConsumerMaterial { output_token_ids: ids, job_context_hash, family, answer })
}

fn parse_ids(arr: &[serde_json::Value], path: &std::path::Path) -> Result<Vec<u32>, CliError> {
    arr.iter()
        .map(|v| {
            v.as_u64()
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| CliError::new(exit::GENERIC, format!("{}: output_token_ids must be u32 values", path.display())))
        })
        .collect()
}

/// One disagreement between the chain and the consumer's own recomputation, named.
///
/// "MISMATCH" tells a reader the object is false and not where to look; ADR-0078 Decision 5's
/// value is a demonstration, and a demonstration names the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Mismatch {
    pub field: &'static str,
    pub on_chain: String,
    pub recomputed: String,
}

/// **The comparison, as a pure function** — the whole of `derived-verify`'s arithmetic, so it can
/// be driven from a test with no node and no network.
///
/// `object` is the derivation REBUILT from what the chain returned (row + claim + the network's
/// domain); `chain_derived_id` is the id the chain filed it under. The checks, in the order a
/// reader should read them:
///
/// 1. `output_root` — X6's first recomputation, from the ids the consumer holds. Skipped, and
///    said to be skipped, when they did not supply ids / context hash / family.
/// 2. `dsl_hash`, `artifact_hash`, `artifact_bytes`, `kind` — the re-run of the derivation itself
///    (`misaka_palw_derive::verify`).
/// 3. `derived_id` — LAST, and it is a check on the READER rather than on the executor: the id is
///    total over the object, so a disagreement means the object this reader rebuilt is not the one
///    the chain accepted (a different network domain, or an executor key the claim's bond no
///    longer holds), not that the derivation is false.
pub(crate) fn compare(
    object: &PalwDerivedArtifactV1,
    chain_derived_id: Hash64,
    answer: Option<&[u8]>,
    output_root_inputs: Option<(PalwRcFamilyV1, Hash64, &[u32])>,
) -> Result<Vec<Mismatch>, String> {
    let mut out = Vec::new();
    if let Some((family, job_context_hash, ids)) = output_root_inputs {
        let recomputed = misaka_palw_derive::recompute_output_root(family, &job_context_hash, ids);
        if recomputed != object.output_root {
            out.push(Mismatch { field: "output_root", on_chain: object.output_root.to_string(), recomputed: recomputed.to_string() });
        }
    }
    if let Some(answer) = answer {
        // A transformer this build does not publish is SA-5's case: the reader cannot verify and
        // says so, rather than reporting a pass it did not earn.
        let v = misaka_palw_derive::verify(object, answer).map_err(|e| e.to_string())?;
        if !v.dsl_hash_matches {
            out.push(Mismatch {
                field: "dsl_hash",
                on_chain: object.dsl_hash.to_string(),
                recomputed: v.recomputed_dsl_hash.to_string(),
            });
        }
        if !v.artifact_hash_matches {
            out.push(Mismatch {
                field: "artifact_hash",
                on_chain: object.artifact_hash.to_string(),
                recomputed: v.recomputed_artifact_hash.to_string(),
            });
        }
        if !v.artifact_bytes_matches {
            out.push(Mismatch {
                field: "artifact_bytes",
                on_chain: object.artifact_bytes.to_string(),
                recomputed: v.recomputed_artifact_bytes.to_string(),
            });
        }
        // X8: the chain checks `kind != 0` and interprets no kind, so a kind that disagrees with
        // its own transformer's manifest is only ever the consumer's to catch.
        if !v.kind_matches {
            out.push(Mismatch { field: "kind", on_chain: object.kind.to_string(), recomputed: v.manifest_kind.to_string() });
        }
    }
    let rebuilt = derived_id_v1(object);
    if rebuilt != chain_derived_id {
        out.push(Mismatch { field: "derived_id", on_chain: chain_derived_id.to_string(), recomputed: rebuilt.to_string() });
    }
    Ok(out)
}

/// Rebuild the object the chain accepted from what the read returned. Every field of the id's
/// preimage is here, which is why `derived_id` is a check and not a decoration.
fn object_from_chain(
    response: &GetPalwDerivedArtifactsResponse,
    row: &RpcPalwDerivedArtifact,
    network_domain: Hash64,
) -> Result<(PalwDerivedArtifactV1, Hash64), CliError> {
    let hex64 = |s: &str, what: &str| -> Result<Hash64, CliError> {
        s.trim().parse::<Hash64>().map_err(|_| CliError::new(exit::GENERIC, format!("node returned a malformed {what}: '{s}'")))
    };
    let mut executor_pubkey = vec![0u8; response.executor_pubkey.len() / 2];
    faster_hex::hex_decode(response.executor_pubkey.as_bytes(), &mut executor_pubkey)
        .map_err(|e| CliError::new(exit::GENERIC, format!("node returned a malformed executor key: {e}")))?;
    let object = PalwDerivedArtifactV1 {
        version: PALW_DERIVED_V1_VERSION,
        network_domain,
        claim_id: hex64(&response.claim_id, "claim id")?,
        output_root: hex64(&response.output_root, "output root")?,
        grammar_id: hex64(&row.grammar_id, "grammar id")?,
        transformer_id: hex64(&row.transformer_id, "transformer id")?,
        kind: u16::try_from(row.kind).map_err(|_| CliError::new(exit::GENERIC, format!("node returned kind {}", row.kind)))?,
        dsl_hash: hex64(&row.dsl_hash, "dsl hash")?,
        artifact_hash: hex64(&row.artifact_hash, "artifact hash")?,
        artifact_bytes: row.artifact_bytes,
        executor_pubkey,
    };
    Ok((object, hex64(&row.derived_id, "derived id")?))
}

pub async fn verify(
    ctx: &Ctx,
    claim_id: &str,
    answer_path: &std::path::Path,
    dsl_path: Option<&std::path::Path>,
    job_context_hash: Option<&str>,
    family: Option<&str>,
    json: bool,
) -> CliResult {
    let claim = parse_claim_id(claim_id)?;
    let material = read_material(answer_path, dsl_path, job_context_hash, family)?;
    let reader = connect(ctx).await?;
    let response = reader
        .client
        .get_palw_derived_artifacts(claim.to_string())
        .await
        .map_err(|e| CliError::new(exit::CONNECTION, format!("getPalwDerivedArtifacts: {e}")))?;
    if !response.found {
        return Err(CliError::new(exit::GENERIC, format!("this chain holds no claim {claim} — nothing to verify against")));
    }
    if response.artifacts.is_empty() {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "claim {claim} is on this chain and carries no derivation (ADR-0078 X4: an answer that did not parse still certifies)"
            ),
        ));
    }
    let output_root_inputs = match (material.family, material.job_context_hash, material.output_token_ids.as_ref()) {
        (Some(f), Some(h), Some(ids)) => Some((f, h, ids.as_slice())),
        _ => None,
    };
    // **A vacuous pass is worse than a refusal.** With neither the answer bytes nor the three
    // output_root inputs there is nothing left to compare but the reconstruction, and printing
    // "consistent" for that would tell a reader they verified a derivation they did not touch.
    if material.answer.is_none() && output_root_inputs.is_none() {
        return Err(CliError::new(
            exit::GENERIC,
            format!(
                "{} carries neither the answer's canonical DSL nor (output_token_ids, job_context_hash, family), \
                 so nothing about this derivation could be checked — pass the gateway's response, or --dsl with \
                 --job-context-hash and --family",
                answer_path.display()
            ),
        ));
    }
    let as_json = json || ctx.output == OutputFormat::Json;
    let mut all_consistent = true;
    let mut rows = Vec::new();
    for row in &response.artifacts {
        let (object, chain_derived_id) = object_from_chain(&response, row, reader.network_domain)?;
        let name = kind::name(object.kind).unwrap_or("?");
        let verdict = match compare(&object, chain_derived_id, material.answer.as_deref(), output_root_inputs) {
            Ok(mismatches) if mismatches.is_empty() => {
                let checked = describe_checked(material.answer.is_some(), output_root_inputs.is_some());
                serde_json::json!({ "transformer_id": row.transformer_id, "kind": row.kind, "kind_name": name, "verdict": "consistent", "checked": checked })
            }
            Ok(mismatches) => {
                all_consistent = false;
                let first = &mismatches[0];
                serde_json::json!({
                    "transformer_id": row.transformer_id,
                    "kind": row.kind,
                    "kind_name": name,
                    "verdict": "MISMATCH",
                    "first_mismatch": first.field,
                    "on_chain": first.on_chain,
                    "recomputed": first.recomputed,
                    "all_mismatches": mismatches.iter().map(|m| m.field).collect::<Vec<_>>(),
                })
            }
            Err(why) => {
                // Not being able to re-run IS a demonstrable gap (SA-5): an object naming a
                // transformer this build does not publish is one nobody can check, and saying
                // "consistent" about it would be the failure the whole command exists to prevent.
                all_consistent = false;
                serde_json::json!({
                    "transformer_id": row.transformer_id,
                    "kind": row.kind,
                    "kind_name": name,
                    "verdict": "UNVERIFIABLE",
                    "reason": why,
                })
            }
        };
        rows.push(verdict);
    }
    let document = serde_json::json!({
        "schema": "misaka.palw.chain-derive-verify.v1",
        "claim_id": response.claim_id,
        "claim_phase": response.claim_phase,
        "claim_void_reason": response.claim_void_reason,
        "output_root": response.output_root,
        "executor_bond": response.executor_bond,
        "derivations": rows,
        "verdict": if all_consistent { "consistent" } else { "MISMATCH — a demonstrable false object (ADR-0078 Decision 5)" },
    });
    if as_json {
        println!("{}", serde_json::to_string_pretty(&document).expect("serializable"));
    } else {
        println!("claim {} ({})", response.claim_id, response.claim_phase);
        for row in document["derivations"].as_array().expect("array") {
            let v = row["verdict"].as_str().unwrap_or("?");
            println!("  kind {} ({})  {v}", row["kind"], row["kind_name"].as_str().unwrap_or("?"));
            if let Some(field) = row.get("first_mismatch").and_then(|v| v.as_str()) {
                println!("    first mismatch: {field}");
                println!("      on chain   {}", row["on_chain"].as_str().unwrap_or(""));
                println!("      recomputed {}", row["recomputed"].as_str().unwrap_or(""));
            }
            if let Some(reason) = row.get("reason").and_then(|v| v.as_str()) {
                println!("    {reason}");
            }
            if let Some(checked) = row.get("checked").and_then(|v| v.as_str()) {
                println!("    checked: {checked}");
            }
        }
        println!("{}", document["verdict"].as_str().unwrap_or(""));
    }
    if all_consistent { Ok(()) } else { Err(CliError::new(exit::GENERIC, "the chain and the recomputation disagree")) }
}

/// A pass has to say what it covered: "consistent" over one of the three recomputations is not
/// the same statement as "consistent" over all of them, and a reader who cannot tell them apart
/// has been given a verdict they cannot use.
fn describe_checked(reran: bool, output_root: bool) -> &'static str {
    match (reran, output_root) {
        (true, true) => "output_root, dsl_hash, artifact_hash, artifact_bytes, kind, derived_id (ADR-0078 X6, in full)",
        (true, false) => {
            "dsl_hash, artifact_hash, artifact_bytes, kind, derived_id — NOT output_root (pass the gateway response, or --job-context-hash and --family with the ids)"
        }
        (false, true) => "output_root, derived_id — NOT the derivation (pass --dsl, or a gateway response carrying the canonical DSL)",
        (false, false) => "derived_id only — no answer bytes and no ids were supplied, so nothing about this derivation was checked",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use misaka_palw_derive::{ClaimBinding, derive_named, recompute_output_root};

    const MUSIC_TRANSFORMER: &str = "music/smf/v1";

    /// The shipped corpus's first row (`misaka-palw-derive/corpus/music/01-single-note.json`) —
    /// one note, the smallest thing the MIDI writer can be asked for, and a real derivation rather
    /// than a hand-written hash.
    fn corpus_answer() -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../misaka-palw-derive/corpus/music/01-single-note.json");
        std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn h(byte: u8) -> Hash64 {
        Hash64::from_bytes([byte; 64])
    }

    /// A binding whose `output_root` is the one those ids actually imply, so X6's first
    /// recomputation is a real comparison and not a tautology over a constant.
    fn binding(ids: &[u32]) -> (ClaimBinding, PalwRcFamilyV1, Hash64) {
        let family = PalwRcFamilyV1::Base0;
        let job_context_hash = h(0xC7);
        let output_root = recompute_output_root(family, &job_context_hash, ids);
        (
            ClaimBinding { network_domain: h(0x11), claim_id: h(0xC1), output_root, executor_pubkey: vec![0xAB; 2592] },
            family,
            job_context_hash,
        )
    }

    /// **The whole of Decision 5, in one test.** A real derivation of the shipped music corpus, a
    /// fixture row shaped exactly as the chain read returns it, and the consumer's recomputation
    /// agreeing on every one of X6's three values.
    #[test]
    fn a_true_derivation_of_the_music_corpus_is_consistent_with_its_chain_row() {
        let ids = vec![7u32, 11, 13];
        let (binding, family, job_context_hash) = binding(&ids);
        let answer = corpus_answer();
        let derivation = derive_named(MUSIC_TRANSFORMER, &binding, &answer).expect("the shipped corpus derives");
        assert_eq!(derivation.kind, kind::MUSIC, "the music corpus derives a music artifact");

        // The chain's row, as `GetPalwDerivedArtifacts` hands it over.
        let chain_derived_id = derived_id_v1(&derivation.object);
        let mismatches = compare(&derivation.object, chain_derived_id, Some(&answer), Some((family, job_context_hash, &ids)))
            .expect("the transformer is published in this tree (SA-5)");
        assert!(mismatches.is_empty(), "a true derivation disagrees with the chain about nothing: {mismatches:?}");
    }

    /// **A false object, and the field it is false about.** The executor filed a derivation whose
    /// `artifact_hash` is not the one the transformer produces; the chain accepted it, because the
    /// chain never runs a transformer (Decision 5). The consumer names the field.
    #[test]
    fn a_false_artifact_hash_is_reported_by_name() {
        let ids = vec![7u32, 11, 13];
        let (binding, family, job_context_hash) = binding(&ids);
        let answer = corpus_answer();
        let derivation = derive_named(MUSIC_TRANSFORMER, &binding, &answer).expect("the shipped corpus derives");

        let mut lie = derivation.object.clone();
        lie.artifact_hash = h(0xBA);
        // The chain filed the LIE's id, which is what makes this a false object rather than a
        // reader that rebuilt the wrong thing — `derived_id` is total over every field.
        let chain_derived_id = derived_id_v1(&lie);
        let mismatches =
            compare(&lie, chain_derived_id, Some(&answer), Some((family, job_context_hash, &ids))).expect("still re-runnable");
        assert_eq!(
            mismatches.iter().map(|m| m.field).collect::<Vec<_>>(),
            vec!["artifact_hash"],
            "exactly the field that was lied about, and no collateral noise"
        );
        assert_eq!(mismatches[0].on_chain, h(0xBA).to_string());
        assert_eq!(mismatches[0].recomputed, derivation.object.artifact_hash.to_string());
    }

    /// X6's first recomputation on its own: ids that are not the claim's ids do not produce the
    /// claim's `output_root`, and that is the check that ties a derivation to an answer at all.
    #[test]
    fn ids_that_are_not_the_answers_ids_fail_the_output_root() {
        let ids = vec![7u32, 11, 13];
        let (binding, family, job_context_hash) = binding(&ids);
        let answer = corpus_answer();
        let derivation = derive_named(MUSIC_TRANSFORMER, &binding, &answer).expect("the shipped corpus derives");
        let chain_derived_id = derived_id_v1(&derivation.object);

        let other = vec![7u32, 11, 14];
        let mismatches = compare(&derivation.object, chain_derived_id, Some(&answer), Some((family, job_context_hash, &other)))
            .expect("re-runnable");
        assert_eq!(mismatches.iter().map(|m| m.field).collect::<Vec<_>>(), vec!["output_root"]);
        assert_eq!(mismatches[0].on_chain, derivation.object.output_root.to_string());
    }

    /// A reader that rebuilt the object under the WRONG network domain gets `derived_id` — the
    /// check that says "this is not the object the chain accepted" instead of accusing the
    /// executor of a falsehood it did not commit.
    #[test]
    fn a_wrong_network_domain_shows_up_as_derived_id_and_not_as_a_false_derivation() {
        let ids = vec![7u32];
        let (binding, family, job_context_hash) = binding(&ids);
        let answer = corpus_answer();
        let derivation = derive_named(MUSIC_TRANSFORMER, &binding, &answer).expect("the shipped corpus derives");
        let chain_derived_id = derived_id_v1(&derivation.object);

        let mut rebuilt = derivation.object.clone();
        rebuilt.network_domain = h(0x22);
        let mismatches =
            compare(&rebuilt, chain_derived_id, Some(&answer), Some((family, job_context_hash, &ids))).expect("re-runnable");
        assert_eq!(mismatches.iter().map(|m| m.field).collect::<Vec<_>>(), vec!["derived_id"]);
    }

    /// Without the answer bytes and without the ids, the only thing left to check is the
    /// reconstruction — and the verdict says exactly that rather than reporting a bare pass.
    #[test]
    fn a_pass_says_which_of_the_three_recomputations_it_covered() {
        assert!(describe_checked(true, true).contains("in full"));
        assert!(describe_checked(true, false).contains("NOT output_root"));
        assert!(describe_checked(false, true).contains("NOT the derivation"));
        assert!(describe_checked(false, false).contains("nothing about this derivation was checked"));
    }

    /// The gateway's own response is the consumer's half of the evidence: the ids, the job's
    /// context hash, the family and the canonical DSL, read out of the `misaka` block.
    #[test]
    fn the_gateway_response_is_read_as_the_consumers_material() {
        let dir = std::env::temp_dir().join(format!("misaka-derived-verify-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch dir");
        let path = dir.join("response.json");
        let doc = serde_json::json!({
            "id": "palwcmpl-abc",
            "misaka": {
                "fp_claim_id": h(0xC1).to_string(),
                "output_token_ids": [7, 11, 13],
                "job_context_hash": h(0xC7).to_string(),
                "family": "base0",
                "derivation": { "dsl": "{\"v\":1}" },
            },
        });
        std::fs::write(&path, serde_json::to_vec_pretty(&doc).unwrap()).expect("write");
        let material = read_material(&path, None, None, None).expect("the gateway response reads");
        assert_eq!(material.output_token_ids.as_deref(), Some([7u32, 11, 13].as_slice()));
        assert_eq!(material.job_context_hash, Some(h(0xC7)));
        assert_eq!(material.family, Some(PalwRcFamilyV1::Base0));
        assert_eq!(material.answer.as_deref(), Some(b"{\"v\":1}".as_slice()));
        let _ = std::fs::remove_file(&path);
    }
}
